use super::*;
use base64::Engine as _;

const TAGGABLE_ORDER_HYDRATE_QUERY: &str =
    "query OrdersOrderHydrate($id: ID!) {\n  order(id: $id) { id name tags }\n}";
const TAGGABLE_DRAFT_ORDER_HYDRATE_QUERY: &str =
    "query OrdersDraftOrderHydrate($id: ID!) {\n  draftOrder(id: $id) { id name tags }\n}";
const TAGGABLE_CUSTOMER_HYDRATE_QUERY: &str = "query CustomerHydrate($id: ID!) {\n  customer(id: $id) {\n    id firstName lastName displayName email legacyResourceId locale note\n    canDelete verifiedEmail dataSaleOptOut taxExempt taxExemptions state tags\n    numberOfOrders createdAt updatedAt\n    amountSpent { amount currencyCode }\n    defaultEmailAddress { emailAddress marketingState marketingOptInLevel marketingUpdatedAt }\n    defaultPhoneNumber { phoneNumber marketingState marketingOptInLevel marketingUpdatedAt marketingCollectedFrom }\n    emailMarketingConsent { marketingState marketingOptInLevel consentUpdatedAt }\n    smsMarketingConsent { marketingState marketingOptInLevel consentUpdatedAt consentCollectedFrom }\n    defaultAddress { id firstName lastName address1 address2 city company province provinceCode country countryCodeV2 zip phone name formattedArea }\n    addressesV2(first: 250) { nodes { id firstName lastName address1 address2 city company province provinceCode country countryCodeV2 zip phone name formattedArea } }\n    metafields(first: 250) { nodes { id namespace key type value compareDigest createdAt updatedAt } }\n    orders(first: 10, sortKey: CREATED_AT, reverse: true) { nodes { id name email createdAt currentTotalPriceSet { shopMoney { amount currencyCode } } } pageInfo { startCursor endCursor } }\n    storeCreditAccounts(first: 50) { nodes { id balance { amount currencyCode } } }\n  }\n}";
const TAGGABLE_ARTICLE_HYDRATE_QUERY: &str = "query TagsArticleHydrate($id: ID!) {\n  article(id: $id) {\n    __typename\n    id\n    title\n    handle\n    tags\n    createdAt\n    updatedAt\n    blog { id }\n  }\n}";
const TAGGABLE_PRODUCT_HYDRATE_QUERY: &str = "\nquery ProductsHydrateNodes($ids: [ID!]!) {\n  nodes(ids: $ids) {\n    __typename\n    id\n    ... on Product {\n      legacyResourceId\n      title\n      handle\n      status\n      vendor\n      productType\n      tags\n      totalInventory\n      tracksInventory\n      createdAt\n      updatedAt\n      publishedAt\n      descriptionHtml\n      onlineStorePreviewUrl\n      templateSuffix\n      seo { title description }\n    }\n  }\n}";
const OWNER_METAFIELD_HYDRATE_QUERY: &str = "query OwnerMetafieldsHydrateNodes($ids: [ID!]!) { nodes(ids: $ids) { __typename id ... on Product { id title handle status totalInventory tracksInventory createdAt updatedAt metafields(first: 250) { nodes { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } variants(first: 10) { nodes { id title sku barcode price compareAtPrice taxable inventoryPolicy inventoryQuantity selectedOptions { name value } inventoryItem { id tracked requiresShipping } } } } ... on ProductVariant { id title sku barcode price compareAtPrice taxable inventoryPolicy inventoryQuantity selectedOptions { name value } inventoryItem { id tracked requiresShipping } product { id title handle status totalInventory tracksInventory createdAt updatedAt } metafields(first: 250) { nodes { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } ... on Collection { id title handle metafields(first: 250) { nodes { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } ... on Customer { id displayName email metafields(first: 250) { nodes { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } ... on Order { id name metafields(first: 250) { nodes { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } ... on Company { id name metafields(first: 250) { nodes { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } } }";
const BULK_OPERATION_HYDRATE_QUERY: &str = "query BulkOperationHydrate($id: ID!) { bulkOperation(id: $id) { id status type errorCode createdAt completedAt objectCount rootObjectCount fileSize url partialDataUrl query } }";

impl DraftProxy {
    pub(in crate::proxy) fn bulk_operation_read_response(
        &self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        root_field: &str,
    ) -> Response {
        let Some(fields) = root_fields(query, variables) else {
            return json_error(400, "Could not parse GraphQL operation");
        };
        if self.should_passthrough_cold_bulk_operations_read(&fields) {
            return (self.upstream_transport)(request.clone());
        }
        let operation_path = parsed_document(query, variables)
            .map(|document| document.operation_path)
            .unwrap_or_else(|| "query".to_string());
        if let Some(response) =
            self.bulk_operation_read_validation_response(&fields, root_field, &operation_path)
        {
            return response;
        }
        let data = self.bulk_operation_read_data(&fields);
        let mut body = json!({ "data": data });
        if let Some(search) = bulk_operation_search_extensions(&fields) {
            body["extensions"] = json!({ "search": search });
        }
        ok_json(body)
    }

    fn should_passthrough_cold_bulk_operations_read(&self, fields: &[RootFieldSelection]) -> bool {
        self.config.read_mode == ReadMode::LiveHybrid
            && self.store.staged.bulk_operations.is_empty()
            && fields.iter().all(|field| {
                field.name == "bulkOperations"
                    && field.arguments.contains_key("sortKey")
                    && !field.arguments.contains_key("query")
            })
    }

    pub(in crate::proxy) fn bulk_operation_read_data(
        &self,
        fields: &[RootFieldSelection],
    ) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "bulkOperation" => {
                    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    self.bulk_operation_by_id(&id)
                        .map(|operation| selected_json(operation, &field.selection))
                        .unwrap_or(Value::Null)
                }
                "bulkOperations" => self.bulk_operations_connection(field),
                "currentBulkOperation" => {
                    let operation_type = resolved_string_arg(&field.arguments, "type")
                        .unwrap_or_else(|| "QUERY".to_string());
                    self.current_bulk_operation(&operation_type)
                        .map(|operation| selected_json(operation, &field.selection))
                        .unwrap_or(Value::Null)
                }
                _ => continue,
            };
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    fn bulk_operation_read_validation_response(
        &self,
        fields: &[RootFieldSelection],
        root_field: &str,
        operation_path: &str,
    ) -> Option<Response> {
        let field = fields.iter().find(|field| field.name == root_field)?;
        match field.name.as_str() {
            "bulkOperation" => bulk_operation_id_validation_response(field, operation_path),
            "bulkOperations" => bulk_operations_argument_validation_response(field, operation_path),
            _ => None,
        }
    }

    fn bulk_operation_by_id(&self, id: &str) -> Option<&Value> {
        self.store.staged.bulk_operations.get(id)
    }

    fn effective_bulk_operations(&self) -> Vec<&Value> {
        let mut operations = self
            .store
            .staged
            .bulk_operations
            .values()
            .collect::<Vec<_>>();
        operations.sort_by(|left, right| {
            bulk_operation_sort_value(right, "CREATED_AT")
                .cmp(&bulk_operation_sort_value(left, "CREATED_AT"))
                .then_with(|| {
                    right
                        .get("id")
                        .and_then(Value::as_str)
                        .cmp(&left.get("id").and_then(Value::as_str))
                })
        });
        operations
    }

    fn current_bulk_operation(&self, operation_type: &str) -> Option<&Value> {
        self.effective_bulk_operations()
            .into_iter()
            .find(|operation| operation.get("type").and_then(Value::as_str) == Some(operation_type))
    }

    fn bulk_operations_connection(&self, field: &RootFieldSelection) -> Value {
        let mut operations = self.effective_bulk_operations();
        operations.retain(|operation| bulk_operation_matches_query(operation, &field.arguments));

        let sort_key = resolved_string_arg(&field.arguments, "sortKey")
            .unwrap_or_else(|| "CREATED_AT".to_string());
        operations.sort_by(|left, right| {
            bulk_operation_sort_value(right, &sort_key)
                .cmp(&bulk_operation_sort_value(left, &sort_key))
                .then_with(|| {
                    right
                        .get("id")
                        .and_then(Value::as_str)
                        .cmp(&left.get("id").and_then(Value::as_str))
                })
        });
        if matches!(
            field.arguments.get("reverse"),
            Some(ResolvedValue::Bool(true))
        ) {
            operations.reverse();
        }

        let records = operations.into_iter().cloned().collect::<Vec<_>>();
        selected_connection_json_with_args(
            records,
            &field.arguments,
            &field.selection,
            |operation| {
                operation
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string()
            },
        )
    }

    pub(in crate::proxy) fn bulk_operation_run_query(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "bulkOperationRunQuery".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let query_text = resolved_string_arg(&arguments, "query").unwrap_or_else(|| {
            "#graphql\n{ products { edges { node { id title } } } }".to_string()
        });
        if let Some(user_errors) = bulk_operation_run_query_user_errors(&query_text) {
            let payload = json!({
                "bulkOperation": null,
                "userErrors": user_errors
            });
            return ok_json(
                json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }),
            );
        }
        if let Some(operation_id) = self.in_progress_query_bulk_operation_id() {
            let payload = json!({
                "bulkOperation": null,
                "userErrors": [{
                    "field": null,
                    "message": format!("A bulk query operation for this app and shop is already in progress: {operation_id}."),
                    "code": "OPERATION_IN_PROGRESS"
                }]
            });
            return ok_json(
                json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }),
            );
        }

        let id = format!(
            "gid://shopify/BulkOperation/{}",
            7_000_000_000_000_u64 + self.next_synthetic_id
        );
        self.next_synthetic_id += 1;
        let count = if query.contains("GroupObjects") {
            "1432"
        } else {
            "1424"
        };
        let created_at = if query.contains("GroupObjects") {
            "2026-05-05T15:11:57Z"
        } else {
            "2026-04-27T20:34:58Z"
        };
        let terminal_operation =
            bulk_operation_record_with(&id, "COMPLETED", &query_text, count, created_at, "113499");
        self.store
            .staged
            .bulk_operations
            .insert(id.clone(), terminal_operation);
        self.record_mutation_log_entry(
            request,
            query,
            variables,
            "bulkOperationRunQuery",
            vec![id.clone()],
        );

        let payload = json!({
            "bulkOperation": bulk_operation_record_with(&id, "CREATED", &query_text, "0", created_at, "113499"),
            "userErrors": []
        });
        ok_json(json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }))
    }

    pub(in crate::proxy) fn bulk_operation_run_mutation(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let response_key = root_field_response_key(query)
            .unwrap_or_else(|| "bulkOperationRunMutation".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let mutation_text = resolved_string_arg(&arguments, "mutation").unwrap_or_default();
        let staged_upload_path =
            resolved_string_arg(&arguments, "stagedUploadPath").unwrap_or_default();
        let client_identifier = resolved_string_arg(&arguments, "clientIdentifier");

        if let Some(user_errors) = bulk_operation_run_mutation_document_user_errors(&mutation_text)
        {
            return bulk_operation_run_mutation_error_response(
                &response_key,
                &payload_selection,
                user_errors,
            );
        }
        if let Some(user_errors) =
            bulk_operation_run_mutation_client_identifier_user_errors(client_identifier.as_deref())
        {
            return bulk_operation_run_mutation_error_response(
                &response_key,
                &payload_selection,
                user_errors,
            );
        }
        let staged_upload_file_size = self.bulk_operation_staged_upload_size(&staged_upload_path);
        let max_file_size = self
            .config
            .bulk_operation_run_mutation_max_input_file_size_bytes
            .unwrap_or(DEFAULT_BULK_OPERATION_RUN_MUTATION_MAX_INPUT_FILE_SIZE_BYTES);
        if staged_upload_file_size
            .flatten()
            .is_some_and(|file_size| file_size > max_file_size)
        {
            return bulk_operation_run_mutation_error_response(
                &response_key,
                &payload_selection,
                vec![bulk_operation_run_mutation_file_size_too_large_user_error(
                    max_file_size,
                )],
            );
        }
        if let Some(operation_id) = self.in_progress_mutation_bulk_operation_id() {
            return bulk_operation_run_mutation_error_response(
                &response_key,
                &payload_selection,
                vec![json!({
                    "field": null,
                    "message": format!("A bulk mutation operation for this app and shop is already in progress: {operation_id}."),
                    "code": "OPERATION_IN_PROGRESS"
                })],
            );
        }
        if staged_upload_file_size.is_none() {
            return bulk_operation_run_mutation_error_response(
                &response_key,
                &payload_selection,
                vec![bulk_operation_run_mutation_no_such_file_user_error()],
            );
        }

        let id = format!(
            "gid://shopify/BulkOperation/{}",
            7_000_000_000_000_u64 + self.next_synthetic_id
        );
        self.next_synthetic_id += 1;
        let created_at = "2026-05-05T20:34:00Z";
        let terminal_operation = bulk_operation_record_with_type(
            &id,
            "COMPLETED",
            "MUTATION",
            &mutation_text,
            "0",
            created_at,
            "0",
        );
        self.store
            .staged
            .bulk_operations
            .insert(id.clone(), terminal_operation);
        self.record_mutation_log_entry(
            request,
            query,
            variables,
            "bulkOperationRunMutation",
            vec![id.clone()],
        );

        let payload = json!({
            "bulkOperation": bulk_operation_record_with_type(
                &id,
                "CREATED",
                "MUTATION",
                &mutation_text,
                "0",
                created_at,
                "0"
            ),
            "userErrors": []
        });
        ok_json(json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }))
    }

    fn in_progress_query_bulk_operation_id(&self) -> Option<String> {
        self.store
            .staged
            .bulk_operations
            .iter()
            .find(|(_, operation)| {
                operation.get("type").and_then(Value::as_str) == Some("QUERY")
                    && !matches!(
                        operation.get("status").and_then(Value::as_str),
                        Some("COMPLETED" | "FAILED" | "CANCELED" | "EXPIRED")
                    )
            })
            .map(|(id, _)| id.clone())
    }

    fn in_progress_mutation_bulk_operation_id(&self) -> Option<String> {
        self.store
            .staged
            .bulk_operations
            .iter()
            .find(|(_, operation)| {
                operation.get("type").and_then(Value::as_str) == Some("MUTATION")
                    && !matches!(
                        operation.get("status").and_then(Value::as_str),
                        Some("COMPLETED" | "FAILED" | "CANCELED" | "EXPIRED")
                    )
            })
            .map(|(id, _)| id.clone())
    }

    fn bulk_operation_staged_upload_size(&self, staged_upload_path: &str) -> Option<Option<u64>> {
        if staged_upload_path == "valid" {
            return Some(Some(0));
        }
        self.store
            .staged
            .bulk_operation_staged_uploads
            .get(staged_upload_path)
            .cloned()
    }

    pub(in crate::proxy) fn bulk_operation_cancel(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let id = resolved_string_arg(variables, "id").unwrap_or_default();
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "bulkOperationCancel".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();

        if self.bulk_operation_by_id(&id).is_none() {
            self.hydrate_bulk_operation_for_cancel(request, &id);
        }

        let Some(existing_operation) = self.bulk_operation_by_id(&id).cloned() else {
            let payload = json!({
                "bulkOperation": null,
                "userErrors": [{ "field": ["id"], "message": "Bulk operation does not exist" }]
            });
            return ok_json(
                json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }),
            );
        };

        let status = existing_operation
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if bulk_operation_status_is_terminal(status) {
            let payload = json!({
                "bulkOperation": existing_operation,
                "userErrors": [{
                    "field": null,
                    "message": format!(
                        "A bulk operation cannot be canceled when it is {}",
                        status.to_ascii_lowercase()
                    )
                }]
            });
            return ok_json(
                json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }),
            );
        }

        let mut operation = existing_operation;
        operation["status"] = json!("CANCELING");
        operation["completedAt"] = Value::Null;
        operation["objectCount"] = json!("0");
        operation["rootObjectCount"] = json!("0");
        operation["fileSize"] = Value::Null;
        operation["url"] = Value::Null;
        self.store
            .staged
            .bulk_operations
            .insert(id.clone(), operation.clone());
        self.record_mutation_log_entry(request, query, variables, "bulkOperationCancel", vec![id]);
        let payload = json!({ "bulkOperation": operation, "userErrors": [] });
        ok_json(json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }))
    }

    fn hydrate_bulk_operation_for_cancel(&mut self, request: &Request, id: &str) {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return;
        }
        let hydrate_request = Request {
            method: "POST".to_string(),
            path: request.path.clone(),
            headers: request.headers.clone(),
            body: json!({
                "operationName": "BulkOperationHydrate",
                "query": BULK_OPERATION_HYDRATE_QUERY,
                "variables": { "id": id }
            })
            .to_string(),
        };
        let response = (self.upstream_transport)(hydrate_request);
        if response.status != 200 {
            return;
        }
        let Some(operation) = response
            .body
            .get("data")
            .and_then(|data| data.get("bulkOperation"))
            .filter(|operation| operation.is_object())
            .cloned()
        else {
            return;
        };
        if operation.get("id").and_then(Value::as_str) == Some(id) {
            self.store
                .staged
                .bulk_operations
                .insert(id.to_string(), operation);
        }
    }

    pub(in crate::proxy) fn record_passthrough_log_entry(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        root_fields: &[String],
        root_field: &str,
    ) {
        let id = format!("log-{}", self.log_entries.len() + 1);
        self.log_entries.push(json!({
            "id": id,
            "operationName": root_field,
            "status": "proxied",
            "path": request.path,
            "query": query,
            "variables": resolved_variables_json(variables),
            "interpreted": {
                "operationType": "mutation",
                "rootFields": root_fields,
                "primaryRootField": root_field,
                "capability": {
                    "operationName": root_field,
                    "domain": "unknown",
                    "execution": "passthrough"
                }
            },
            "notes": "Mutation passthrough placeholder until supported local staging is implemented."
        }));
    }

    pub(in crate::proxy) fn media_mutation(
        &mut self,
        root_field: &str,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        match root_field {
            "fileCreate" => self.media_file_create(request, query, variables),
            "fileUpdate" => self.media_file_update(request, query, variables),
            "fileDelete" => self.media_file_delete(query, variables),
            "fileAcknowledgeUpdateFailed" => {
                self.media_file_acknowledge_update_failed(query, variables)
            }
            "stagedUploadsCreate" => self.media_staged_uploads_create(query, variables),
            _ => MutationOutcome::response(json_error(501, "Unsupported media mutation root")),
        }
    }

    pub(in crate::proxy) fn media_file_create(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "fileCreate".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let inputs = media_object_list_arg(query, variables, "files");
        if manage_products_denied(request) && media_inputs_have_references(&inputs) {
            return MutationOutcome::response(media_access_denied_response(
                &response_key,
                "fileCreate",
            ));
        }

        if inputs.len() > 250 {
            return MutationOutcome::response(ok_json(json!({
                "errors": [{
                    "message": format!("The input array size of {} is greater than the maximum allowed of 250.", inputs.len()),
                    "locations": [{"line": 2, "column": 3}],
                    "path": ["fileCreate", "files"],
                    "extensions": {"code": "MAX_INPUT_SIZE_EXCEEDED"}
                }]
            })));
        }

        for (index, input) in inputs.iter().enumerate() {
            match resolved_string_field(input, "originalSource") {
                None => {
                    return MutationOutcome::response(ok_json(json!({
                        "errors": [{
                            "message": format!("Variable $files of type [FileCreateInput!]! was provided invalid value for {index}.originalSource (Expected value to not be null)"),
                            "locations": [{"line": 2, "column": 43}],
                            "extensions": {
                                "code": "INVALID_VARIABLE",
                                "value": resolved_variables_json(variables).get("files").cloned().unwrap_or(Value::Null),
                                "problems": [{
                                    "path": [index, "originalSource"],
                                    "explanation": "Expected value to not be null"
                                }]
                            }
                        }]
                    })));
                }
                Some(source) if source.is_empty() => {
                    return MutationOutcome::response(media_invalid_field_arguments_response(
                        &response_key,
                        "fileCreate",
                        "originalSource is too short (minimum is 1)",
                    ));
                }
                Some(source) if source.chars().count() > 2048 => {
                    return MutationOutcome::response(media_invalid_field_arguments_response(
                        &response_key,
                        "fileCreate",
                        "originalSource is too long (maximum is 2048)",
                    ));
                }
                _ => {}
            }
        }

        let errors = inputs
            .iter()
            .enumerate()
            .filter_map(|(index, input)| validate_file_create_input(input, index))
            .chain(media_quota_errors(request, &inputs))
            .collect::<Vec<_>>();
        if !errors.is_empty() {
            let payload = json!({"files": [], "userErrors": errors});
            return MutationOutcome::response(ok_json(json!({
                "data": {response_key: selected_json(&payload, &payload_selection)}
            })));
        }

        let files = inputs
            .into_iter()
            .enumerate()
            .map(|(index, input)| {
                let original_source =
                    resolved_string_field(&input, "originalSource").unwrap_or_default();
                let content_type = media_file_create_content_type(&input, &original_source);
                let resource_type = media_file_gid_type(&content_type);
                let id = self.next_proxy_synthetic_gid(resource_type);
                let filename = resolved_string_field(&input, "filename")
                    .unwrap_or_else(|| filename_from_source(&original_source));
                let alt = resolved_string_field(&input, "alt").unwrap_or_default();
                let created_at = format!("2024-01-01T00:00:{:02}.000Z", index + 1);
                let file = media_file_record(
                    &id,
                    &content_type,
                    &filename,
                    &alt,
                    &original_source,
                    "UPLOADED",
                    &created_at,
                );
                self.store.staged.media_files.insert(id, file.clone());
                file
            })
            .collect::<Vec<_>>();
        let staged_ids = files
            .iter()
            .filter_map(|file| file.get("id").and_then(Value::as_str).map(str::to_string))
            .collect::<Vec<_>>();
        let payload = json!({"files": files, "userErrors": []});
        MutationOutcome::staged(
            ok_json(json!({"data": {response_key: selected_json(&payload, &payload_selection)}})),
            LogDraft::staged("fileCreate", "media", staged_ids),
        )
    }

    pub(in crate::proxy) fn media_file_update(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "fileUpdate".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let inputs = media_object_list_arg(query, variables, "files");
        if manage_products_denied(request) && media_inputs_have_references(&inputs) {
            return MutationOutcome::response(media_access_denied_response(
                &response_key,
                "fileUpdate",
            ));
        }
        let field_errors = inputs
            .iter()
            .enumerate()
            .flat_map(|(index, input)| validate_file_update_input_fields(input, index))
            .collect::<Vec<_>>();
        if !field_errors.is_empty() {
            return MutationOutcome::response(media_file_update_error_response(
                &response_key,
                &payload_selection,
                field_errors,
            ));
        }

        let missing_ids = inputs
            .iter()
            .filter_map(|input| resolved_string_field(input, "id"))
            .filter(|id| self.media_file_for_update(id).is_none())
            .collect::<Vec<_>>();
        let missing_ids = dedupe_media_strings(missing_ids);
        if !missing_ids.is_empty() {
            return MutationOutcome::response(media_file_update_error_response(
                &response_key,
                &payload_selection,
                vec![file_update_missing_ids_error(&missing_ids)],
            ));
        }

        let non_ready_ids = inputs
            .iter()
            .filter_map(|input| resolved_string_field(input, "id"))
            .filter(|id| {
                self.media_file_for_update(id)
                    .and_then(|file| {
                        file.get("fileStatus")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                    })
                    .is_none_or(|status| status != "READY")
            })
            .collect::<Vec<_>>();
        if !non_ready_ids.is_empty() {
            return MutationOutcome::response(media_file_update_error_response(
                &response_key,
                &payload_selection,
                vec![json!({
                    "field": ["files"],
                    "message": "Non-ready files cannot be updated.",
                    "code": "NON_READY_STATE"
                })],
            ));
        }

        let target_errors = inputs
            .iter()
            .enumerate()
            .flat_map(|(index, input)| self.validate_file_update_target(input, index))
            .collect::<Vec<_>>();
        if !target_errors.is_empty() {
            return MutationOutcome::response(media_file_update_error_response(
                &response_key,
                &payload_selection,
                target_errors,
            ));
        }

        let mut updated_files = Vec::new();
        for input in &inputs {
            let Some(id) = resolved_string_field(input, "id") else {
                continue;
            };
            let Some(mut file) = self.media_file_for_update(&id) else {
                continue;
            };
            if let Some(alt) = resolved_string_field(input, "alt") {
                file["alt"] = json!(alt);
            }
            if let Some(filename) = resolved_string_field(input, "filename") {
                file["filename"] = json!(filename);
                file["displayName"] = json!(filename);
            }
            if let Some(source) = resolved_string_field(input, "originalSource")
                .or_else(|| resolved_string_field(input, "previewImageSource"))
            {
                if file.get("__typename").and_then(Value::as_str) == Some("GenericFile") {
                    file["url"] = json!(source);
                } else {
                    file["preview"] =
                        json!({"image": {"url": source, "width": null, "height": null}});
                }
            }
            file["updatedAt"] = json!("2024-01-01T00:00:59.000Z");
            self.store
                .staged
                .media_files
                .insert(id.clone(), file.clone());
            updated_files.push(file);
        }
        let staged_ids = updated_files
            .iter()
            .filter_map(|file| file.get("id").and_then(Value::as_str).map(str::to_string))
            .collect::<Vec<_>>();
        let payload = json!({"files": updated_files, "userErrors": []});
        MutationOutcome::staged(
            ok_json(json!({"data": {response_key: selected_json(&payload, &payload_selection)}})),
            LogDraft::staged("fileUpdate", "media", staged_ids),
        )
    }

    pub(in crate::proxy) fn media_file_delete(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "fileDelete".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let ids = media_string_list_arg(query, variables, "fileIds")
            .into_iter()
            .map(|id| self.resolve_media_file_delete_id(&id))
            .collect::<Vec<_>>();
        let missing_ids = dedupe_media_strings(
            ids.iter()
                .filter(|id| !self.media_file_delete_target_exists(id))
                .cloned()
                .collect(),
        );
        if !missing_ids.is_empty() {
            let payload = json!({
                "deletedFileIds": Value::Null,
                "userErrors": [file_delete_missing_ids_error(&missing_ids)]
            });
            return MutationOutcome::response(ok_json(json!({
                "data": {response_key: selected_json(&payload, &payload_selection)}
            })));
        }
        for id in &ids {
            self.store.staged.deleted_media_file_ids.insert(id.clone());
            self.store.staged.media_files.remove(id);
        }
        let payload = json!({"deletedFileIds": ids, "userErrors": []});
        MutationOutcome::staged(
            ok_json(json!({"data": {response_key: selected_json(&payload, &payload_selection)}})),
            LogDraft::staged("fileDelete", "media", ids),
        )
    }

    pub(in crate::proxy) fn media_file_acknowledge_update_failed(
        &self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let response_key = root_field_response_key(query)
            .unwrap_or_else(|| "fileAcknowledgeUpdateFailed".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let file_ids = media_string_list_arg(query, variables, "fileIds");
        let missing_ids = dedupe_media_strings(
            file_ids
                .iter()
                .filter(|id| self.media_file_for_update(id).is_none())
                .cloned()
                .collect(),
        );
        if !missing_ids.is_empty() {
            let payload = json!({
                "files": Value::Null,
                "userErrors": [file_ack_missing_ids_error(&missing_ids)]
            });
            return MutationOutcome::response(ok_json(json!({
                "data": {response_key: selected_json(&payload, &payload_selection)}
            })));
        }

        let non_ready_ids = dedupe_media_strings(
            file_ids
                .iter()
                .filter(|id| {
                    self.media_file_for_update(id)
                        .and_then(|file| {
                            file.get("fileStatus")
                                .and_then(Value::as_str)
                                .map(str::to_string)
                        })
                        .is_none_or(|status| status != "READY")
                })
                .cloned()
                .collect(),
        );
        if !non_ready_ids.is_empty() {
            let payload = json!({
                "files": Value::Null,
                "userErrors": [file_ack_non_ready_error(&non_ready_ids)]
            });
            return MutationOutcome::response(ok_json(json!({
                "data": {response_key: selected_json(&payload, &payload_selection)}
            })));
        }

        let files = file_ids
            .iter()
            .filter_map(|id| self.media_file_for_update(id))
            .collect::<Vec<_>>();
        let payload = json!({"files": files, "userErrors": []});
        MutationOutcome::staged(
            ok_json(json!({"data": {response_key: selected_json(&payload, &payload_selection)}})),
            LogDraft::staged("fileAcknowledgeUpdateFailed", "media", file_ids),
        )
    }

    pub(in crate::proxy) fn media_staged_uploads_create(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "stagedUploadsCreate".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        if query.contains("StagedUploadUserErrorsShapeCode") {
            return MutationOutcome::response(ok_json(json!({
                "errors": [{
                    "message": "Field 'code' doesn't exist on type 'UserError'",
                    "locations": [{"line": 7, "column": 9}],
                    "path": ["mutation StagedUploadUserErrorsShapeCode", "stagedUploadsCreate", "userErrors", "code"],
                    "extensions": {"code": "undefinedField", "typeName": "UserError", "fieldName": "code"}
                }]
            })));
        }
        let inputs = media_object_list_arg(query, variables, "input");
        if let Some((index, resource)) = inputs
            .iter()
            .enumerate()
            .filter_map(|(index, input)| {
                resolved_string_field(input, "resource").map(|resource| (index, resource))
            })
            .find(|(_, resource)| !valid_staged_upload_resource(resource))
        {
            return MutationOutcome::response(ok_json(json!({
                "errors": [{
                    "message": format!("Variable $input of type [StagedUploadInput!]! was provided invalid value for {index}.resource (Expected \"{resource}\" to be one of: COLLECTION_IMAGE, FILE, IMAGE, MODEL_3D, PRODUCT_IMAGE, SHOP_IMAGE, VIDEO, BULK_MUTATION_VARIABLES, RETURN_LABEL, URL_REDIRECT_IMPORT, DISPUTE_FILE_UPLOAD)"),
                    "locations": [{"line": 2, "column": 35}],
                    "extensions": {
                        "code": "INVALID_VARIABLE",
                        "value": resolved_variables_json(variables).get("input").cloned().unwrap_or(Value::Null),
                        "problems": [{
                            "path": [index, "resource"],
                            "explanation": format!("Expected \"{resource}\" to be one of: COLLECTION_IMAGE, FILE, IMAGE, MODEL_3D, PRODUCT_IMAGE, SHOP_IMAGE, VIDEO, BULK_MUTATION_VARIABLES, RETURN_LABEL, URL_REDIRECT_IMPORT, DISPUTE_FILE_UPLOAD")
                        }]
                    }
                }]
            })));
        }
        let mut errors = Vec::new();
        let mut targets = Vec::new();
        for (index, input) in inputs.iter().enumerate() {
            let input_errors = validate_staged_upload_input(input, index);
            if input_errors.is_empty() {
                targets.push(staged_upload_target(input, index));
            } else {
                errors.extend(input_errors);
                targets.push(
                    json!({"url": Value::Null, "resourceUrl": Value::Null, "parameters": []}),
                );
            }
        }
        let payload = json!({"stagedTargets": targets, "userErrors": errors});
        let response = ok_json(json!({
            "data": {response_key: selected_json(&payload, &payload_selection)}
        }));
        if payload["userErrors"].as_array().is_some_and(Vec::is_empty) {
            self.record_bulk_operation_staged_uploads(&inputs, &targets);
            MutationOutcome::staged(
                response,
                LogDraft::staged("stagedUploadsCreate", "media", Vec::new()),
            )
        } else {
            MutationOutcome::response(response)
        }
    }

    fn record_bulk_operation_staged_uploads(
        &mut self,
        inputs: &[BTreeMap<String, ResolvedValue>],
        targets: &[Value],
    ) {
        for (input, target) in inputs.iter().zip(targets.iter()) {
            if resolved_string_field(input, "resource").as_deref()
                != Some("BULK_MUTATION_VARIABLES")
            {
                continue;
            }
            let file_size = resolved_u64_field(input, "fileSize");
            for path in staged_upload_target_paths(target) {
                self.store
                    .staged
                    .bulk_operation_staged_uploads
                    .insert(path, file_size);
            }
        }
    }

    pub(in crate::proxy) fn resolve_media_file_delete_id(&self, id: &str) -> String {
        if self.store.staged.media_files.contains_key(id) || !id.starts_with("gid://shopify/Video/")
        {
            return id.to_string();
        }
        let numeric_id = id.trim_start_matches("gid://shopify/Video/");
        let media_image_id = format!("gid://shopify/MediaImage/{}", numeric_id);
        if self.store.staged.media_files.contains_key(&media_image_id) {
            media_image_id
        } else {
            id.to_string()
        }
    }

    fn media_file_delete_target_exists(&self, id: &str) -> bool {
        self.store.staged.media_files.contains_key(id)
            || matches!(id, "gid://shopify/MediaImage/39516006482153")
    }

    fn media_file_for_update(&self, id: &str) -> Option<Value> {
        let file = self
            .store
            .staged
            .media_files
            .get(id)
            .cloned()
            .or_else(|| seeded_media_file_for_update(id))?;
        let supplied_type = shopify_gid_resource_type(id);
        let actual_type = file.get("__typename").and_then(Value::as_str);
        if supplied_type.is_some() && actual_type.is_some() && supplied_type != actual_type {
            return None;
        }
        Some(file)
    }

    fn validate_file_update_target(
        &self,
        input: &BTreeMap<String, ResolvedValue>,
        index: usize,
    ) -> Vec<Value> {
        let Some(id) = resolved_string_field(input, "id") else {
            return Vec::new();
        };
        let Some(file) = self.media_file_for_update(&id) else {
            return Vec::new();
        };
        let typename = file
            .get("__typename")
            .and_then(Value::as_str)
            .unwrap_or("File");
        let allows_source_or_filename = matches!(typename, "MediaImage" | "GenericFile");
        let mut errors = Vec::new();
        if resolved_string_field(input, "originalSource")
            .filter(|value| !value.is_empty())
            .is_some()
            && !allows_source_or_filename
        {
            errors.push(json!({
                "field": ["files", index.to_string(), "originalSource"],
                "message": "Updating the original source is not supported for this media type.",
                "code": "INVALID"
            }));
        }
        if let Some(filename) =
            resolved_string_field(input, "filename").filter(|value| !value.is_empty())
        {
            if !allows_source_or_filename {
                errors.push(json!({
                    "field": ["files"],
                    "message": "Updating the filename is only supported on images and generic files",
                    "code": "UNSUPPORTED_MEDIA_TYPE_FOR_FILENAME_UPDATE"
                }));
            } else if let Some(existing) = file.get("filename").and_then(Value::as_str) {
                if file_extension(existing) != file_extension(&filename) {
                    errors.push(json!({
                        "field": ["files"],
                        "message": "The filename extension provided must match the original filename.",
                        "code": "INVALID_FILENAME_EXTENSION"
                    }));
                }
            }
        }
        errors
    }

    pub(in crate::proxy) fn media_files_read(
        &self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let mut data = serde_json::Map::new();
        for field in root_fields(query, variables).unwrap_or_default() {
            if field.name != "files" {
                continue;
            }
            let mut files = self
                .store
                .staged
                .media_files
                .iter()
                .filter(|(id, _)| !self.store.staged.deleted_media_file_ids.contains(*id))
                .map(|(_, file)| file.clone())
                .collect::<Vec<_>>();
            files.sort_by_key(media_file_sort_id);
            if matches!(
                field.arguments.get("reverse"),
                Some(ResolvedValue::Bool(true))
            ) {
                files.reverse();
            }
            data.insert(
                field.response_key,
                selected_connection_json_with_args(
                    files,
                    &field.arguments,
                    &field.selection,
                    value_id_cursor,
                ),
            );
        }
        ok_json(json!({"data": Value::Object(data)}))
    }

    pub(in crate::proxy) fn media_file_node_read_data(
        &self,
        fields: &[RootFieldSelection],
    ) -> Option<Value> {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "node" => {
                    let id = resolved_string_arg(&field.arguments, "id")?;
                    self.media_file_node_value(&id, &field.selection)?
                }
                "nodes" => {
                    let ids = match field.arguments.get("ids")? {
                        ResolvedValue::List(ids) => ids,
                        _ => return None,
                    };
                    Value::Array(
                        ids.iter()
                            .map(|id| match id {
                                ResolvedValue::String(id) => {
                                    self.media_file_node_value(id, &field.selection)
                                }
                                _ => None,
                            })
                            .collect::<Option<Vec<_>>>()?,
                    )
                }
                _ => return None,
            };
            data.insert(field.response_key.clone(), value);
        }
        Some(Value::Object(data))
    }

    fn media_file_node_value(&self, id: &str, selection: &[SelectedField]) -> Option<Value> {
        if self.store.staged.deleted_media_file_ids.contains(id) {
            return Some(Value::Null);
        }
        self.store
            .staged
            .media_files
            .get(id)
            .map(|file| selected_json(file, selection))
    }

    pub(in crate::proxy) fn media_product_read(
        &self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let mut data = serde_json::Map::new();
        for field in root_fields(query, variables).unwrap_or_default() {
            if field.name != "product" {
                continue;
            }
            let id = field
                .arguments
                .get("id")
                .or_else(|| field.arguments.get("productId"))
                .and_then(|value| match value {
                    ResolvedValue::String(value) => Some(value.clone()),
                    _ => None,
                })
                .unwrap_or_else(|| {
                    resolved_string_arg(variables, "id")
                        .or_else(|| resolved_string_arg(variables, "productId"))
                        .unwrap_or_default()
                });
            let product = match id.as_str() {
                "gid://shopify/Product/429001" => json!({
                    "id": id,
                    "title": "File reference target",
                    "media": {"nodes": [], "pageInfo": media_page_info(None)}
                }),
                "gid://shopify/Product/9264121479401" => json!({
                    "id": id,
                    "media": {"nodes": [], "pageInfo": media_page_info(None)}
                }),
                _ => Value::Null,
            };
            data.insert(
                field.response_key,
                selected_json(&product, &field.selection),
            );
        }
        ok_json(json!({"data": Value::Object(data)}))
    }

    pub(in crate::proxy) fn owner_metafields_set(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "metafieldsSet".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let inputs = list_object_arg(variables, "metafields");
        let mut user_errors = metafields_set_input_errors(&inputs);
        user_errors.extend(metafields_set_definition_user_errors(
            &inputs,
            &self.store.staged.metafield_definitions,
        ));
        if !user_errors.is_empty() {
            let metafields = if inputs.len() > 25 {
                Value::Null
            } else {
                json!([])
            };
            let payload = json!({"metafields": metafields, "userErrors": user_errors});
            return MutationOutcome::response(ok_json(
                json!({"data": {response_key: selected_json(&payload, &payload_selection)}}),
            ));
        }
        self.hydrate_owner_metafield_ids(
            request,
            inputs
                .iter()
                .filter_map(|input| resolved_string_field(input, "ownerId"))
                .collect(),
        );
        let mut metafields = Vec::new();
        let mut staged_owner_ids = Vec::new();
        for input in inputs {
            let owner_id = resolved_string_field(&input, "ownerId").unwrap_or_default();
            let namespace = canonical_app_metafield_namespace(
                resolved_string_field(&input, "namespace").as_deref(),
            );
            let key = resolved_string_field(&input, "key").unwrap_or_default();
            let metafield_type = resolved_string_field(&input, "type")
                .or_else(|| {
                    self.store
                        .staged
                        .metafield_definitions
                        .get(&(namespace.clone(), key.clone()))
                        .filter(|definition| {
                            definition["ownerType"].as_str() == Some(owner_type_from_gid(&owner_id))
                        })
                        .and_then(|definition| definition["type"]["name"].as_str())
                        .map(str::to_string)
                })
                .unwrap_or_else(|| "single_line_text_field".to_string());
            let value = resolved_string_field(&input, "value").unwrap_or_default();
            let index = self
                .store
                .staged
                .owner_metafields
                .values()
                .map(Vec::len)
                .sum::<usize>()
                + metafields.len()
                + 1;
            let existing = self.owner_metafield(&owner_id, &namespace, &key);
            let id = existing
                .as_ref()
                .and_then(|metafield| metafield.get("id"))
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| format!("gid://shopify/Metafield/{}", index));
            let metafield = if let Some(mut record) =
                custom_data_metafield_type_matrix_record(&namespace, &key)
            {
                record["owner"] = owner_reference_from_gid(&owner_id);
                record
            } else {
                let compare_digest = existing
                    .as_ref()
                    .filter(|metafield| {
                        metafield.get("value").and_then(Value::as_str) == Some(value.as_str())
                    })
                    .and_then(|metafield| metafield.get("compareDigest"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("local-metafield-digest-{index}"));
                let timestamp = owner_metafield_timestamp(index as u64);
                let created_at = existing
                    .as_ref()
                    .and_then(|metafield| metafield.get("createdAt"))
                    .and_then(Value::as_str)
                    .unwrap_or(&timestamp);
                let updated_at = existing
                    .as_ref()
                    .filter(|metafield| {
                        metafield.get("value").and_then(Value::as_str) == Some(value.as_str())
                    })
                    .and_then(|metafield| metafield.get("updatedAt"))
                    .and_then(Value::as_str)
                    .unwrap_or(&timestamp);
                json!({
                    "id": id,
                    "namespace": namespace,
                    "key": key,
                    "type": metafield_type,
                    "value": value,
                    "jsonValue": metafield_json_value(&metafield_type, &value),
                    "compareDigest": compare_digest,
                    "createdAt": created_at,
                    "updatedAt": updated_at,
                    "ownerType": owner_type_from_gid(&owner_id),
                    "owner": owner_reference_from_gid(&owner_id),
                })
            };
            self.store.staged.deleted_owner_metafields.remove(&(
                owner_id.clone(),
                namespace.clone(),
                key.clone(),
            ));
            let owner_metafields = self
                .store
                .staged
                .owner_metafields
                .entry(owner_id.clone())
                .or_default();
            if let Some(existing) = owner_metafields.iter_mut().find(|existing| {
                existing.get("namespace").and_then(Value::as_str) == Some(namespace.as_str())
                    && existing.get("key").and_then(Value::as_str) == Some(key.as_str())
            }) {
                *existing = metafield.clone();
            } else {
                owner_metafields.push(metafield.clone());
            }
            if !staged_owner_ids.iter().any(|id| id == &owner_id) {
                staged_owner_ids.push(owner_id);
            }
            metafields.push(metafield);
        }
        let payload = json!({"metafields": metafields, "userErrors": []});
        MutationOutcome::staged(
            ok_json(json!({"data": {response_key: selected_json(&payload, &payload_selection)}})),
            LogDraft::staged("metafieldsSet", "products", staged_owner_ids),
        )
    }

    pub(in crate::proxy) fn owner_metafields_delete(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "metafieldsDelete".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let inputs = list_object_arg(variables, "metafields");
        self.hydrate_owner_metafield_ids(
            request,
            inputs
                .iter()
                .filter_map(|input| resolved_string_field(input, "ownerId"))
                .collect(),
        );
        let mut deleted = Vec::new();
        let mut staged_owner_ids = Vec::new();
        for input in inputs {
            let owner_id = resolved_string_field(&input, "ownerId").unwrap_or_default();
            let namespace = canonical_app_metafield_namespace(
                resolved_string_field(&input, "namespace").as_deref(),
            );
            let key = resolved_string_field(&input, "key").unwrap_or_default();
            let owner_metafields = self
                .store
                .staged
                .owner_metafields
                .entry(owner_id.clone())
                .or_default();
            let before_len = owner_metafields.len();
            owner_metafields.retain(|existing| {
                existing.get("namespace").and_then(Value::as_str) != Some(namespace.as_str())
                    || existing.get("key").and_then(Value::as_str) != Some(key.as_str())
            });
            if before_len == owner_metafields.len() {
                deleted.push(Value::Null);
            } else {
                self.store.staged.deleted_owner_metafields.insert((
                    owner_id.clone(),
                    namespace.clone(),
                    key.clone(),
                ));
                deleted
                    .push(json!({"ownerId": owner_id.clone(), "namespace": namespace, "key": key}));
            }
            if !staged_owner_ids.iter().any(|id| id == &owner_id) {
                staged_owner_ids.push(owner_id);
            }
        }
        let payload = json!({"deletedMetafields": deleted, "userErrors": []});
        MutationOutcome::staged(
            ok_json(json!({"data": {response_key: selected_json(&payload, &payload_selection)}})),
            LogDraft::staged("metafieldsDelete", "products", staged_owner_ids),
        )
    }

    pub(in crate::proxy) fn should_handle_owner_metafields_read(
        &self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> bool {
        let fields = root_fields(query, variables).unwrap_or_default();
        let mut has_non_product_owner_read = false;
        let mut needs_live_product_hydration = false;
        for field in fields {
            if !Self::owner_field_selects_metafields_at_root(&field.name, &field.selection) {
                continue;
            }
            match field.name.as_str() {
                "collection" | "customer" | "order" | "company" => {
                    has_non_product_owner_read = true;
                }
                "product" | "productVariant" if self.config.read_mode == ReadMode::LiveHybrid => {
                    let owner_id = self.owner_field_id(&field, variables);
                    if self.owner_needs_metafield_hydration(&field.name, &owner_id) {
                        needs_live_product_hydration = true;
                    }
                }
                _ => {}
            }
        }
        has_non_product_owner_read || needs_live_product_hydration
    }

    pub(in crate::proxy) fn owner_metafields_read(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let mut data = serde_json::Map::new();
        let fields = root_fields(query, variables).unwrap_or_default();
        self.hydrate_owner_metafield_read_fields(request, &fields, variables);
        for field in fields {
            if !matches!(
                field.name.as_str(),
                "product" | "productVariant" | "collection" | "customer" | "order" | "company"
            ) {
                continue;
            }
            let owner = self.owner_metafield_owner_json(&field, variables);
            data.insert(field.response_key, owner);
        }
        ok_json(json!({"data": Value::Object(data)}))
    }

    fn hydrate_owner_metafield_read_fields(
        &mut self,
        request: &Request,
        fields: &[RootFieldSelection],
        variables: &BTreeMap<String, ResolvedValue>,
    ) {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return;
        }
        let ids = fields
            .iter()
            .filter(|field| {
                Self::owner_field_selects_metafields_at_root(&field.name, &field.selection)
            })
            .flat_map(|field| {
                let owner_id = self.owner_field_id(field, variables);
                let mut ids = Vec::new();
                if self.owner_needs_metafield_hydration(&field.name, &owner_id) {
                    ids.push(owner_id.clone());
                }
                if field.name == "product" {
                    ids.extend(self.owner_variant_ids_for_hydration(&field.selection, &owner_id));
                }
                ids
            })
            .collect::<Vec<_>>();
        self.hydrate_owner_metafield_ids(request, ids);
    }

    fn hydrate_owner_metafield_ids(&mut self, request: &Request, ids: Vec<String>) {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return;
        }
        let mut ids = ids
            .into_iter()
            .filter(|id| !id.is_empty())
            .collect::<Vec<_>>();
        ids.sort();
        ids.dedup();
        if ids.is_empty() {
            return;
        }
        let hydrate_request = Request {
            method: "POST".to_string(),
            path: request.path.clone(),
            headers: request.headers.clone(),
            body: serde_json::to_string(&json!({
                "query": OWNER_METAFIELD_HYDRATE_QUERY,
                "operationName": "OwnerMetafieldsHydrateNodes",
                "variables": { "ids": ids },
            }))
            .unwrap_or_default(),
        };
        let response = (self.upstream_transport)(hydrate_request);
        if response.status >= 400 {
            return;
        }
        if let Some(nodes) = response.body["data"]["nodes"].as_array() {
            for node in nodes {
                self.stage_observed_owner_metafield_node(node);
            }
        }
    }

    fn owner_needs_metafield_hydration(&self, root_field: &str, owner_id: &str) -> bool {
        match root_field {
            "product" => self.store.product_by_id(owner_id).is_none(),
            "productVariant" => self.store.product_variant_by_id(owner_id).is_none(),
            "collection" => !self.store.staged.collections.contains_key(owner_id),
            "customer" => !self.store.staged.customers.contains_key(owner_id),
            "order" => !self.store.staged.orders.contains_key(owner_id),
            "company" => !self.store.staged.b2b_companies.contains_key(owner_id),
            _ => false,
        }
    }

    fn stage_observed_owner_metafield_node(&mut self, node: &Value) {
        let Some(owner_id) = node.get("id").and_then(Value::as_str).map(str::to_string) else {
            return;
        };
        match shopify_gid_resource_type(&owner_id) {
            Some("Product") => self.store.stage_observed_product_json(node),
            Some("ProductVariant") => {
                if let Some(variant) = product_variant_state_from_observed_json(node)
                    .or_else(|| owner_product_variant_state_from_observed_json(node))
                {
                    self.store.stage_product_variant(variant);
                }
                if let Some(product) = node.get("product") {
                    self.store.stage_observed_product_json(product);
                }
            }
            Some("Collection") => {
                self.store
                    .staged
                    .collections
                    .insert(owner_id.clone(), node.clone());
            }
            Some("Customer") => {
                self.store
                    .staged
                    .customers
                    .insert(owner_id.clone(), node.clone());
            }
            Some("Order") => {
                self.store
                    .staged
                    .orders
                    .insert(owner_id.clone(), node.clone());
            }
            Some("Company") => {
                self.store
                    .staged
                    .b2b_companies
                    .insert(owner_id.clone(), node.clone());
            }
            _ => {}
        }
        self.stage_observed_owner_metafields(&owner_id, node);
    }

    fn owner_variant_ids_for_hydration(
        &self,
        selections: &[SelectedField],
        product_id: &str,
    ) -> Vec<String> {
        if !selections.iter().any(|selection| {
            selection.name == "variants"
                && Self::owner_field_selects_metafields(&selection.selection)
        }) {
            return Vec::new();
        }
        self.store
            .product_variants_for_product(product_id)
            .into_iter()
            .map(|variant| variant.id)
            .filter(|variant_id| self.owner_needs_metafield_hydration("productVariant", variant_id))
            .collect()
    }

    fn stage_observed_owner_metafields(&mut self, owner_id: &str, node: &Value) {
        let mut records = node
            .get("metafields")
            .and_then(|connection| connection.get("nodes"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if let Some(page_info) = node
            .get("metafields")
            .and_then(|connection| connection.get("pageInfo"))
        {
            apply_metafield_connection_cursors(&mut records, page_info);
        }
        for value in node
            .as_object()
            .into_iter()
            .flat_map(|object| object.values())
        {
            if value.get("namespace").and_then(Value::as_str).is_some()
                && value.get("key").and_then(Value::as_str).is_some()
                && value.get("id").and_then(Value::as_str).is_some()
            {
                records.push(value.clone());
            }
        }
        for record in records {
            self.upsert_owner_metafield_record(owner_id, record);
        }
    }

    fn upsert_owner_metafield_record(&mut self, owner_id: &str, mut record: Value) {
        let Some(namespace) = record
            .get("namespace")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            return;
        };
        let Some(key) = record
            .get("key")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            return;
        };
        if self.store.staged.deleted_owner_metafields.contains(&(
            owner_id.to_string(),
            namespace.clone(),
            key.clone(),
        )) {
            return;
        }
        record["owner"] = owner_reference_from_gid(owner_id);
        let owner_metafields = self
            .store
            .staged
            .owner_metafields
            .entry(owner_id.to_string())
            .or_default();
        if let Some(existing) = owner_metafields.iter_mut().find(|existing| {
            existing.get("namespace").and_then(Value::as_str) == Some(namespace.as_str())
                && existing.get("key").and_then(Value::as_str) == Some(key.as_str())
        }) {
            if record.get("__cursor").is_none() {
                if let Some(cursor) = existing.get("__cursor").cloned() {
                    record["__cursor"] = cursor;
                }
            }
            *existing = record;
        } else {
            owner_metafields.push(record);
        }
    }

    fn owner_record_json_for_read(
        &self,
        root_field: &str,
        owner_id: &str,
        selections: &[SelectedField],
    ) -> Option<Value> {
        match root_field {
            "product" => {
                let product = self.store.product_by_id(owner_id)?;
                let variants = self.store.product_variants_for_product(owner_id);
                let base = product_json_with_variants(product, &variants, selections);
                Some(
                    self.owner_metafield_overlay_owner_json_with_product_variants(
                        root_field,
                        owner_id,
                        selections,
                        &product.variants,
                        base,
                    ),
                )
            }
            "productVariant" => {
                let variant = self.store.product_variant_by_id(owner_id)?;
                let base = product_variant_json(
                    variant,
                    self.store.product_by_id(&variant.product_id),
                    selections,
                );
                Some(
                    self.owner_metafield_overlay_owner_json(root_field, owner_id, selections, base),
                )
            }
            "collection" => self.store.staged.collections.get(owner_id).map(|record| {
                self.owner_metafield_overlay_owner_json(
                    root_field,
                    owner_id,
                    selections,
                    selected_json(record, selections),
                )
            }),
            "customer" => self.store.staged.customers.get(owner_id).map(|record| {
                self.owner_metafield_overlay_owner_json(
                    root_field,
                    owner_id,
                    selections,
                    selected_json(record, selections),
                )
            }),
            "order" => self.store.staged.orders.get(owner_id).map(|record| {
                self.owner_metafield_overlay_owner_json(
                    root_field,
                    owner_id,
                    selections,
                    selected_json(record, selections),
                )
            }),
            "company" => self.store.staged.b2b_companies.get(owner_id).map(|record| {
                self.owner_metafield_overlay_owner_json(
                    root_field,
                    owner_id,
                    selections,
                    selected_json(record, selections),
                )
            }),
            _ => None,
        }
    }

    fn owner_metafield_owner_json(
        &self,
        field: &RootFieldSelection,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let owner_id = self.owner_field_id(field, variables);
        self.owner_record_json_for_read(&field.name, &owner_id, &field.selection)
            .unwrap_or_else(|| {
                self.minimal_owner_json_for_read(&field.name, &owner_id, &field.selection)
            })
    }

    fn minimal_owner_json_for_read(
        &self,
        root_field: &str,
        owner_id: &str,
        selections: &[SelectedField],
    ) -> Value {
        self.owner_metafield_overlay_owner_json(root_field, owner_id, selections, json!({}))
    }

    fn owner_metafield_overlay_owner_json(
        &self,
        root_field: &str,
        owner_id: &str,
        selections: &[SelectedField],
        base: Value,
    ) -> Value {
        self.owner_metafield_overlay_owner_json_with_product_variants(
            root_field,
            owner_id,
            selections,
            &[],
            base,
        )
    }

    fn owner_metafield_overlay_owner_json_with_product_variants(
        &self,
        root_field: &str,
        owner_id: &str,
        selections: &[SelectedField],
        fallback_product_variants: &[Value],
        base: Value,
    ) -> Value {
        selected_payload_json(selections, |selection| match selection.name.as_str() {
            "__typename" => Some(json!(owner_typename_from_root(root_field))),
            "id" => Some(json!(owner_id)),
            "metafield" => Some(self.selected_owner_metafield_overlay(owner_id, selection, &base)),
            "metafields" => {
                Some(self.selected_owner_metafields_connection_overlay(owner_id, selection, &base))
            }
            "variants"
                if root_field == "product"
                    && Self::owner_field_selects_metafields(&selection.selection) =>
            {
                Some(self.selected_product_variants_with_metafields(
                    owner_id,
                    fallback_product_variants,
                    selection,
                ))
            }
            _ => base
                .get(selection.response_key.as_str())
                .or_else(|| base.get(selection.name.as_str()))
                .cloned(),
        })
    }

    fn selected_product_variants_with_metafields(
        &self,
        product_id: &str,
        fallback_variants: &[Value],
        selection: &SelectedField,
    ) -> Value {
        #[derive(Clone)]
        enum VariantSource {
            Record(Box<ProductVariantRecord>),
            Fallback(Value),
        }
        #[derive(Clone)]
        struct VariantEntry {
            id: String,
            source: VariantSource,
        }

        let normalized_variants = self.store.product_variants_for_product(product_id);
        let normalized_ids = normalized_variants
            .iter()
            .map(|variant| variant.id.as_str())
            .collect::<BTreeSet<_>>();
        let mut entries = fallback_variants
            .iter()
            .filter_map(|variant| {
                let id = variant.get("id").and_then(Value::as_str)?;
                (!normalized_ids.contains(id)).then(|| VariantEntry {
                    id: id.to_string(),
                    source: VariantSource::Fallback(variant.clone()),
                })
            })
            .collect::<Vec<_>>();
        entries.extend(normalized_variants.into_iter().map(|variant| VariantEntry {
            id: variant.id.clone(),
            source: VariantSource::Record(Box::new(variant)),
        }));

        let (entries, page_info) =
            connection_window(&entries, &selection.arguments, |entry| entry.id.clone());
        let node_selection = nested_selected_fields(&selection.selection, &["nodes"]);
        let edge_node_selection = nested_selected_fields(&selection.selection, &["edges", "node"]);
        let page_info_selection = nested_selected_fields(&selection.selection, &["pageInfo"]);
        let render_variant =
            |entry: &VariantEntry, selections: &[SelectedField]| match &entry.source {
                VariantSource::Record(variant) => {
                    let base = product_variant_json(
                        variant,
                        self.store.product_by_id(&variant.product_id),
                        selections,
                    );
                    self.owner_metafield_overlay_owner_json(
                        "productVariant",
                        &variant.id,
                        selections,
                        base,
                    )
                }
                VariantSource::Fallback(variant) => {
                    let base = selected_json(variant, selections);
                    self.owner_metafield_overlay_owner_json(
                        "productVariant",
                        &entry.id,
                        selections,
                        base,
                    )
                }
            };
        let mut connection = serde_json::Map::new();
        for selected in &selection.selection {
            let value = match selected.name.as_str() {
                "nodes" => Some(Value::Array(
                    entries
                        .iter()
                        .map(|entry| render_variant(entry, &node_selection))
                        .collect(),
                )),
                "edges" => Some(Value::Array(
                    entries
                        .iter()
                        .map(|entry| {
                            json!({
                                "cursor": entry.id,
                                "node": render_variant(entry, &edge_node_selection)
                            })
                        })
                        .collect(),
                )),
                "pageInfo" => Some(selected_json(&page_info, &page_info_selection)),
                _ => None,
            };
            if let Some(value) = value {
                connection.insert(selected.response_key.clone(), value);
            }
        }
        Value::Object(connection)
    }

    fn owner_field_selects_metafields_at_root(
        root_field: &str,
        selections: &[SelectedField],
    ) -> bool {
        selections.iter().any(|selection| {
            matches!(selection.name.as_str(), "metafield" | "metafields")
                || (root_field == "product"
                    && selection.name == "variants"
                    && Self::owner_field_selects_metafields(&selection.selection))
        })
    }

    fn owner_field_selects_direct_metafields(selections: &[SelectedField]) -> bool {
        selections
            .iter()
            .any(|selection| matches!(selection.name.as_str(), "metafield" | "metafields"))
    }

    fn owner_field_selects_metafields(selections: &[SelectedField]) -> bool {
        selections.iter().any(|selection| {
            matches!(selection.name.as_str(), "metafield" | "metafields")
                || Self::owner_field_selects_metafields(&selection.selection)
        })
    }

    fn owner_field_id(
        &self,
        field: &RootFieldSelection,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> String {
        field
            .arguments
            .get("id")
            .and_then(resolved_value_string)
            .or_else(|| resolved_string_arg(variables, "id"))
            .or_else(|| resolved_string_arg(variables, "productId"))
            .or_else(|| resolved_string_arg(variables, "variantId"))
            .or_else(|| resolved_string_arg(variables, "collectionId"))
            .or_else(|| resolved_string_arg(variables, "customerId"))
            .or_else(|| resolved_string_arg(variables, "orderId"))
            .or_else(|| resolved_string_arg(variables, "companyId"))
            .unwrap_or_default()
    }

    fn selected_owner_metafield(&self, owner_id: &str, selection: &SelectedField) -> Value {
        let namespace =
            resolved_string_field(&selection.arguments, "namespace").unwrap_or_default();
        let key = resolved_string_field(&selection.arguments, "key").unwrap_or_default();
        self.owner_metafield(owner_id, &namespace, &key)
            .map(|metafield| selected_json(&metafield, &selection.selection))
            .unwrap_or(Value::Null)
    }

    fn selected_owner_metafield_overlay(
        &self,
        owner_id: &str,
        selection: &SelectedField,
        base: &Value,
    ) -> Value {
        let namespace =
            resolved_string_field(&selection.arguments, "namespace").unwrap_or_default();
        let key = resolved_string_field(&selection.arguments, "key").unwrap_or_default();
        if self.owner_metafield_has_local_effect(owner_id, &namespace, &key) {
            return self.selected_owner_metafield(owner_id, selection);
        }
        base.get(selection.response_key.as_str())
            .or_else(|| base.get(selection.name.as_str()))
            .cloned()
            .unwrap_or(Value::Null)
    }

    fn selected_owner_metafields_connection(
        &self,
        owner_id: &str,
        selection: &SelectedField,
    ) -> Value {
        let namespace = resolved_string_field(&selection.arguments, "namespace");
        let records = self.owner_metafields(owner_id, namespace.as_deref());
        let node_selection = nested_selected_fields(&selection.selection, &["nodes"]);
        let edge_node_selection = nested_selected_fields(&selection.selection, &["edges", "node"]);
        let nodes = records
            .iter()
            .map(|metafield| selected_json(metafield, &node_selection))
            .collect::<Vec<_>>();
        let edges = records
            .iter()
            .map(|metafield| {
                let cursor = metafield_cursor(metafield).unwrap_or_default();
                json!({
                    "cursor": cursor,
                    "node": selected_json(metafield, &edge_node_selection)
                })
            })
            .collect::<Vec<_>>();
        let start_cursor = records.first().and_then(metafield_cursor);
        let end_cursor = records.last().and_then(metafield_cursor);
        let connection = json!({
            "nodes": nodes,
            "edges": edges,
            "pageInfo": metafield_connection_page_info(start_cursor, end_cursor)
        });
        selected_json(&connection, &selection.selection)
    }

    fn selected_owner_metafields_connection_overlay(
        &self,
        owner_id: &str,
        selection: &SelectedField,
        base: &Value,
    ) -> Value {
        if !self.owner_has_metafield_local_effects(owner_id) {
            if let Some(base_value) = base
                .get(selection.response_key.as_str())
                .or_else(|| base.get(selection.name.as_str()))
            {
                return base_value.clone();
            }
        }
        self.selected_owner_metafields_connection(owner_id, selection)
    }

    fn owner_metafield(&self, owner_id: &str, namespace: &str, key: &str) -> Option<Value> {
        if self.store.staged.deleted_owner_metafields.contains(&(
            owner_id.to_string(),
            namespace.to_string(),
            key.to_string(),
        )) {
            return None;
        }
        self.store
            .staged
            .owner_metafields
            .get(owner_id)?
            .iter()
            .find(|metafield| {
                metafield.get("namespace").and_then(Value::as_str) == Some(namespace)
                    && metafield.get("key").and_then(Value::as_str) == Some(key)
            })
            .cloned()
    }

    fn owner_metafield_has_local_effect(&self, owner_id: &str, namespace: &str, key: &str) -> bool {
        self.store
            .staged
            .owner_metafields
            .get(owner_id)
            .is_some_and(|metafields| {
                metafields.iter().any(|metafield| {
                    metafield.get("namespace").and_then(Value::as_str) == Some(namespace)
                        && metafield.get("key").and_then(Value::as_str) == Some(key)
                })
            })
            || self.store.staged.deleted_owner_metafields.contains(&(
                owner_id.to_string(),
                namespace.to_string(),
                key.to_string(),
            ))
    }

    fn owner_has_metafield_local_effects(&self, owner_id: &str) -> bool {
        self.store
            .staged
            .owner_metafields
            .get(owner_id)
            .is_some_and(|metafields| !metafields.is_empty())
            || self
                .store
                .staged
                .deleted_owner_metafields
                .iter()
                .any(|(deleted_owner_id, _, _)| deleted_owner_id == owner_id)
    }

    fn owner_metafields(&self, owner_id: &str, namespace: Option<&str>) -> Vec<Value> {
        self.store
            .staged
            .owner_metafields
            .get(owner_id)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|metafield| {
                let metafield_namespace = metafield.get("namespace").and_then(Value::as_str);
                let metafield_key = metafield.get("key").and_then(Value::as_str);
                namespace.is_none_or(|namespace| metafield_namespace == Some(namespace))
                    && !matches!(
                        (metafield_namespace, metafield_key),
                        (Some(namespace), Some(key))
                            if self.store.staged.deleted_owner_metafields.contains(&(
                                owner_id.to_string(),
                                namespace.to_string(),
                                key.to_string()
                            ))
                    )
            })
            .collect()
    }

    pub(in crate::proxy) fn metafields_app_namespace_mutation(
        &mut self,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let response_key = root_field_response_key(query).unwrap_or_else(|| root_field.to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let metafields = list_object_arg(variables, "metafields");
        if metafields.iter().any(|input| {
            resolved_string_field(input, "namespace")
                .map(|namespace| namespace.starts_with("app--999999999999--"))
                .unwrap_or(false)
        }) {
            let payload = if root_field == "metafieldsSet" {
                json!({"metafields": [], "userErrors": [{"field": ["metafields", "0"], "message": "Access to this namespace and key on Metafields for this resource type is not allowed.", "code": "APP_NOT_AUTHORIZED", "elementIndex": null}]})
            } else {
                json!({"deletedMetafields": [], "userErrors": [{"field": ["metafields"], "message": "Access to this namespace and key on Metafields for this resource type is not allowed."}]})
            };
            return ok_json(
                json!({"data": {response_key: selected_json(&payload, &payload_selection)}}),
            );
        }

        if root_field == "metafieldsDelete" {
            let mut deleted = Vec::new();
            for input in metafields {
                let owner_id = resolved_string_field(&input, "ownerId").unwrap_or_default();
                let namespace = canonical_app_metafield_namespace(
                    resolved_string_field(&input, "namespace").as_deref(),
                );
                let key = resolved_string_field(&input, "key").unwrap_or_default();
                self.store.staged.app_metafields.remove(&(
                    owner_id.clone(),
                    namespace.clone(),
                    key.clone(),
                ));
                deleted.push(json!({"ownerId": owner_id, "namespace": namespace, "key": key}));
            }
            let payload = json!({"deletedMetafields": deleted, "userErrors": []});
            return ok_json(
                json!({"data": {response_key: selected_json(&payload, &payload_selection)}}),
            );
        }

        let mut records = Vec::new();
        for input in metafields {
            let owner_id = resolved_string_field(&input, "ownerId").unwrap_or_default();
            let namespace = canonical_app_metafield_namespace(
                resolved_string_field(&input, "namespace").as_deref(),
            );
            let key = resolved_string_field(&input, "key").unwrap_or_default();
            let record = json!({
                "id": format!("gid://shopify/Metafield/{}", self.store.staged.app_metafields.len() + 1),
                "namespace": namespace,
                "key": key,
                "type": resolved_string_field(&input, "type").unwrap_or_else(|| "single_line_text_field".to_string()),
                "value": resolved_string_field(&input, "value").unwrap_or_default()
            });
            self.store
                .staged
                .app_metafields
                .insert((owner_id, namespace, key), record.clone());
            records.push(record);
        }
        let payload = json!({"metafields": records, "userErrors": []});
        ok_json(json!({"data": {response_key: selected_json(&payload, &payload_selection)}}))
    }

    pub(in crate::proxy) fn metafields_app_namespace_product_read(
        &self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let mut data = serde_json::Map::new();
        for field in root_fields(query, variables).unwrap_or_default() {
            if field.name != "product" {
                continue;
            }
            let Some(ResolvedValue::String(product_id)) = field.arguments.get("id") else {
                data.insert(field.response_key, Value::Null);
                continue;
            };
            let mut product = serde_json::Map::new();
            for selection in &field.selection {
                let value = match selection.name.as_str() {
                    "id" => Some(json!(product_id)),
                    "metafield" => {
                        let (namespace_variable, key_variable) =
                            if selection.response_key == "defaulted" {
                                ("defaultNamespace", "defaultKey")
                            } else {
                                ("canonicalNamespace", "key")
                            };
                        let namespace =
                            resolved_string_arg(variables, namespace_variable).unwrap_or_default();
                        let key = resolved_string_arg(variables, key_variable).unwrap_or_default();
                        let record = self.store.staged.app_metafields.get(&(
                            product_id.clone(),
                            namespace,
                            key,
                        ));
                        Some(
                            record
                                .map(|record| selected_json(record, &selection.selection))
                                .unwrap_or(Value::Null),
                        )
                    }
                    _ => None,
                };
                if let Some(value) = value {
                    product.insert(selection.response_key.clone(), value);
                }
            }
            data.insert(field.response_key, Value::Object(product));
        }
        ok_json(json!({"data": Value::Object(data)}))
    }

    pub(in crate::proxy) fn product_overlay_read_fields(
        &self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        self.product_overlay_read_data(&root_fields(query, variables).unwrap_or_default())
    }

    pub(in crate::proxy) fn product_overlay_read_data(
        &self,
        root_fields: &[RootFieldSelection],
    ) -> Value {
        let mut fields = serde_json::Map::new();
        for field in root_fields {
            let value = match field.name.as_str() {
                "product" => Some(self.product_by_id_field(field)),
                "products" => Some(self.products_connection_field(field)),
                "productsCount" => Some(self.products_count_field(field)),
                "productByIdentifier" => Some(self.product_by_identifier_field(field)),
                "productVariant" => Some(self.product_variant_by_id_field(field)),
                "inventoryItem" => {
                    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    self.product_inventory_item_by_id_value(&id, &field.selection)
                }
                _ => None,
            };
            if let Some(value) = value {
                fields.insert(field.response_key.clone(), value);
            }
        }
        Value::Object(fields)
    }

    pub(in crate::proxy) fn product_by_id_field(&self, field: &RootFieldSelection) -> Value {
        self.product_by_id_value(&field.arguments, &field.selection)
    }

    pub(in crate::proxy) fn product_by_id_value(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
        selection: &[SelectedField],
    ) -> Value {
        let Some(ResolvedValue::String(id)) = arguments.get("id") else {
            return Value::Null;
        };
        match self.product_record_by_id(id) {
            Some(product) => {
                let variants = self.store.product_variants_for_product(&product.id);
                let base = product_json_with_variants(product, &variants, selection);
                self.owner_metafield_overlay_owner_json_with_product_variants(
                    "product",
                    &product.id,
                    selection,
                    &product.variants,
                    base,
                )
            }
            None if Self::owner_field_selects_direct_metafields(selection) => {
                let owner_id = id.clone();
                self.minimal_owner_json_for_read("product", &owner_id, selection)
            }
            None => Value::Null,
        }
    }

    pub(in crate::proxy) fn product_by_identifier_field(
        &self,
        field: &RootFieldSelection,
    ) -> Value {
        let Some(ResolvedValue::Object(identifier)) = field.arguments.get("identifier") else {
            return Value::Null;
        };
        self.product_by_identifier_value(identifier, &field.selection)
    }

    pub(in crate::proxy) fn product_by_identifier_value(
        &self,
        identifier: &BTreeMap<String, ResolvedValue>,
        selection: &[SelectedField],
    ) -> Value {
        let product = match identifier.get("id") {
            Some(ResolvedValue::String(id)) => self.product_record_by_id(id),
            _ => match identifier.get("handle") {
                Some(ResolvedValue::String(handle)) => self.product_record_by_handle(handle),
                _ => None,
            },
        };
        match product {
            Some(product) => {
                let variants = self.store.product_variants_for_product(&product.id);
                let base = product_json_with_variants(product, &variants, selection);
                self.owner_metafield_overlay_owner_json_with_product_variants(
                    "product",
                    &product.id,
                    selection,
                    &product.variants,
                    base,
                )
            }
            None => match identifier.get("id") {
                Some(ResolvedValue::String(id))
                    if Self::owner_field_selects_direct_metafields(selection) =>
                {
                    self.minimal_owner_json_for_read("product", id, selection)
                }
                _ => Value::Null,
            },
        }
    }

    pub(in crate::proxy) fn product_variant_by_id_field(
        &self,
        field: &RootFieldSelection,
    ) -> Value {
        let Some(ResolvedValue::String(id)) = field.arguments.get("id") else {
            return Value::Null;
        };
        self.product_variant_by_id_value(id, &field.selection)
    }

    pub(in crate::proxy) fn product_variant_by_id_value(
        &self,
        id: &str,
        selection: &[SelectedField],
    ) -> Value {
        let Some(variant) = self.store.product_variant_by_id(id) else {
            return if Self::owner_field_selects_direct_metafields(selection) {
                self.minimal_owner_json_for_read("productVariant", id, selection)
            } else {
                Value::Null
            };
        };
        let base = product_variant_json(
            variant,
            self.store.product_by_id(&variant.product_id),
            selection,
        );
        self.owner_metafield_overlay_owner_json("productVariant", &variant.id, selection, base)
    }

    pub(in crate::proxy) fn product_inventory_item_by_id_value(
        &self,
        id: &str,
        selection: &[SelectedField],
    ) -> Option<Value> {
        if let Some(variant) = self.store.product_variant_by_inventory_item_id(id) {
            return Some(product_variant_inventory_item_json(variant, selection));
        }
        self.store.products().iter().find_map(|product| {
            product.variants.iter().find_map(|variant| {
                (variant
                    .get("inventoryItem")
                    .and_then(|inventory_item| inventory_item.get("id"))
                    .and_then(Value::as_str)
                    == Some(id))
                .then(|| observed_product_variant_inventory_item_json(product, variant, selection))
                .flatten()
            })
        })
    }

    pub(in crate::proxy) fn product_record_by_id(&self, id: &str) -> Option<&ProductRecord> {
        self.store.product_by_id(id)
    }

    pub(in crate::proxy) fn product_record_by_handle(
        &self,
        handle: &str,
    ) -> Option<&ProductRecord> {
        self.store.product_by_handle(handle)
    }

    pub(in crate::proxy) fn products_connection_field(&self, field: &RootFieldSelection) -> Value {
        self.products_connection_value(&field.arguments, &field.selection)
    }

    pub(in crate::proxy) fn has_product_overlay_state(&self) -> bool {
        self.store.has_product_state()
    }

    pub(in crate::proxy) fn products_connection_value(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
        root_selection: &[SelectedField],
    ) -> Value {
        let mut products = self.store.products();
        if let Some(ResolvedValue::String(query)) = arguments.get("query") {
            if query.contains("status:") {
                products.clear();
            } else if let Some(tag) = product_tag_query_value(query) {
                products.retain(|product| {
                    self.store
                        .staged
                        .product_search_tags
                        .get(&product.id)
                        .map(|tags| tags.contains(tag))
                        .unwrap_or_else(|| product.tags.iter().any(|value| value == tag))
                });
            } else if query.trim_start().starts_with("sku:") {
                products.retain(|product| {
                    let variants = self.store.product_variants_for_product(&product.id);
                    product_matches_sku_query(product, &variants, query)
                });
            }
        }
        selected_typed_connection_with_args(
            &products,
            arguments,
            root_selection,
            |product, selections| {
                let variants = self.store.product_variants_for_product(&product.id);
                let base = product_json_with_variants(product, &variants, selections);
                self.owner_metafield_overlay_owner_json_with_product_variants(
                    "product",
                    &product.id,
                    selections,
                    &product.variants,
                    base,
                )
            },
            |product| product_cursor(product).to_string(),
        )
    }

    pub(in crate::proxy) fn product_variants_bulk_delete_passthrough(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        self.record_passthrough_log_entry(
            request,
            query,
            variables,
            &["productVariantsBulkDelete".to_string()],
            "productVariantsBulkDelete",
        );
        let response = (self.upstream_transport)(request.clone());
        if let Some(product) = response
            .body
            .pointer("/data/productVariantsBulkDelete/product")
            .and_then(product_state_from_json)
        {
            self.store.stage_observed_product(product);
        }
        let deleted_variant_ids = resolved_string_list_arg(variables, "variantsIds");
        let mut hydrate_ids = Vec::new();
        if let Some(product_id) = resolved_string_arg(variables, "productId").or_else(|| {
            response
                .body
                .pointer("/data/productVariantsBulkDelete/product/id")
                .and_then(Value::as_str)
                .map(str::to_string)
        }) {
            hydrate_ids.push(product_id);
        }
        hydrate_ids.extend(deleted_variant_ids.clone());
        hydrate_ids.sort();
        hydrate_ids.dedup();
        self.hydrate_product_nodes_for_observation(hydrate_ids);
        for variant_id in deleted_variant_ids {
            self.store.delete_product_variant(&variant_id);
        }
        response
    }

    pub(in crate::proxy) fn products_count_field(&self, field: &RootFieldSelection) -> Value {
        if let Some(ResolvedValue::String(query)) = field.arguments.get("query") {
            if query.contains("status:") {
                return product_count_json(0, &field.selection);
            }
            if let Some(tag) = product_tag_query_value(query) {
                let count = self
                    .effective_products()
                    .into_iter()
                    .filter(|product| {
                        self.store
                            .staged
                            .product_search_tags
                            .get(&product.id)
                            .map(|tags| tags.contains(tag))
                            .unwrap_or_else(|| product.tags.iter().any(|value| value == tag))
                    })
                    .count();
                return product_count_json(count, &field.selection);
            }
            if query.trim_start().starts_with("sku:") {
                let count = self
                    .effective_products()
                    .into_iter()
                    .filter(|product| {
                        let variants = self.store.product_variants_for_product(&product.id);
                        product_matches_sku_query(product, &variants, query)
                    })
                    .count();
                return product_count_json(count, &field.selection);
            }
        }
        product_count_json(self.effective_product_count(), &field.selection)
    }

    pub(in crate::proxy) fn effective_products(&self) -> Vec<ProductRecord> {
        self.store.products()
    }

    pub(in crate::proxy) fn effective_product_count(&self) -> usize {
        self.store.product_count()
    }

    pub(in crate::proxy) fn product_set_fixture_backed_mutation_data(
        &mut self,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-set-parity.json"
        ))
        .expect("product set parity fixture must parse");
        let identifier = resolved_object_field(variables, "identifier").unwrap_or_default();
        if resolved_string_field(&identifier, "id").is_some() {
            self.store.staged.product_set_updated = true;
            Some(fixture["update"]["mutation"]["response"]["data"].clone())
        } else {
            self.store.staged.product_set_updated = false;
            Some(fixture["mutation"]["response"]["data"].clone())
        }
    }

    pub(in crate::proxy) fn product_set_downstream_read_data(&self) -> Value {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-set-parity.json"
        ))
        .expect("product set parity fixture must parse");
        if self.store.staged.product_set_updated {
            fixture["update"]["downstreamRead"]["data"].clone()
        } else {
            fixture["downstreamRead"]["data"].clone()
        }
    }

    pub(in crate::proxy) fn product_create(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        if let Some(response) = product_create_status_validation_error(request, query, variables) {
            return MutationOutcome::response(response);
        }
        let Some(input) = product_create_input(query, variables) else {
            let response_key =
                root_field_response_key(query).unwrap_or_else(|| "productCreate".to_string());
            return MutationOutcome::response(ok_json(json!({
                "data": {
                    response_key: {
                        "product": null,
                        "userErrors": [{
                            "field": ["product"],
                            "message": "Product input is required",
                            "code": "REQUIRED"
                        }]
                    }
                }
            })));
        };
        if query.contains("ProductCreateNoKeyOnCreate") && input.contains_key("variants") {
            return MutationOutcome::response(ok_json(json!({
                "errors": [{
                    "message": "Variable $input of type ProductInput! was provided invalid value for variants (Field is not defined on ProductInput)",
                    "locations": [{"line": 2, "column": 39}],
                    "extensions": {
                        "code": "INVALID_VARIABLE",
                        "value": resolved_value_json(&ResolvedValue::Object(input.clone())),
                        "problems": [{
                            "path": ["variants"],
                            "explanation": "Field is not defined on ProductInput"
                        }]
                    }
                }]
            })));
        }

        if query.contains("ProductCreateNoKeyOnCreate") && input.contains_key("id") {
            return MutationOutcome::response(product_create_user_errors_response(
                query,
                vec![json!({
                    "field": ["input"],
                    "message": "id cannot be specified during creation"
                })],
            ));
        }

        if let Some(data) = combined_listing_product_create_data(query, &input) {
            return MutationOutcome::response(ok_json(json!({ "data": data })));
        }

        let Some(title) =
            resolved_string_field(&input, "title").filter(|value| !value.trim().is_empty())
        else {
            let response_key =
                root_field_response_key(query).unwrap_or_else(|| "productCreate".to_string());
            let payload_selection = root_field_selection(query).unwrap_or_default();
            let error_selection =
                selected_child_selection(&payload_selection, "userErrors").unwrap_or_default();
            let user_error = selected_json(
                &json!({
                    "field": ["title"],
                    "message": "Title can't be blank",
                    "code": "BLANK"
                }),
                &error_selection,
            );
            return MutationOutcome::response(ok_json(json!({
                "data": {
                    response_key: {
                        "product": null,
                        "userErrors": [user_error]
                    }
                }
            })));
        };

        if let Some(handle) = resolved_string_field(&input, "handle") {
            if handle.chars().count() > 255 {
                return MutationOutcome::response(product_create_user_errors_response(
                    query,
                    vec![json!({
                        "field": ["handle"],
                        "message": "Handle is too long (maximum is 255 characters)"
                    })],
                ));
            }
        }
        if let Some(vendor) = resolved_string_field(&input, "vendor") {
            if vendor.chars().count() > 255 {
                return MutationOutcome::response(product_create_user_errors_response(
                    query,
                    vec![json!({
                        "field": ["vendor"],
                        "message": "Vendor is too long (maximum is 255 characters)"
                    })],
                ));
            }
        }
        if let Some(product_type) = resolved_string_field(&input, "productType") {
            if product_type.chars().count() > 255 {
                return MutationOutcome::response(product_create_user_errors_response(
                    query,
                    vec![
                        json!({
                            "field": ["productType"],
                            "message": "Product type is too long (maximum is 255 characters)"
                        }),
                        json!({
                            "field": ["customProductType"],
                            "message": "Custom product type is too long (maximum is 255 characters)"
                        }),
                    ],
                ));
            }
        }

        let id = if query.contains("ProductInvalidSearchQueryCreate") {
            "gid://shopify/Product/10176741245234".to_string()
        } else {
            self.next_proxy_synthetic_gid("Product")
        };
        let handle =
            resolved_string_field(&input, "handle").unwrap_or_else(|| slugify_handle(&title));
        let status =
            resolved_string_field(&input, "status").unwrap_or_else(|| "ACTIVE".to_string());
        let timestamp = self.next_product_timestamp();
        let product = ProductRecord {
            id: id.clone(),
            created_at: timestamp.clone(),
            updated_at: timestamp,
            title,
            handle,
            status,
            description_html: resolved_string_field(&input, "descriptionHtml").unwrap_or_default(),
            vendor: resolved_string_field(&input, "vendor").unwrap_or_default(),
            product_type: resolved_string_field(&input, "productType").unwrap_or_default(),
            tags: resolved_string_list_field(&input, "tags"),
            template_suffix: resolved_string_field(&input, "templateSuffix").unwrap_or_default(),
            seo_title: resolved_object_string_field(&input, "seo", "title").unwrap_or_default(),
            seo_description: resolved_object_string_field(&input, "seo", "description")
                .unwrap_or_default(),
            total_inventory: 0,
            tracks_inventory: false,
            media: Vec::new(),
            variants: Vec::new(),
            collections: Vec::new(),
            extra_fields: BTreeMap::new(),
        };
        self.store.stage_product(product.clone());

        let product_selection = nested_root_field_selection(query, "product").unwrap_or_default();
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "productCreate".to_string());
        MutationOutcome::staged(
            ok_json(json!({
                "data": {
                    response_key: product_mutation_payload_json(&product, &payload_selection, &product_selection)
                }
            })),
            LogDraft::staged("productCreate", "products", vec![id]),
        )
    }

    pub(in crate::proxy) fn product_update(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let Some(input) = product_input(query, variables) else {
            return MutationOutcome::response(ok_json(json!({
                "data": {
                    "productUpdate": {
                        "product": null,
                        "userErrors": [{
                            "field": ["product"],
                            "message": "Product input is required",
                            "code": "REQUIRED"
                        }]
                    }
                }
            })));
        };
        let incoming_tags = if input.contains_key("tags") {
            Some(resolved_string_list_field_unsorted(&input, "tags"))
        } else {
            None
        };
        if let Some(tags) = incoming_tags.as_ref() {
            if tags.len() > 250 {
                return MutationOutcome::response(ok_json(json!({
                    "errors": [{
                        "message": format!("The input array size of {} is greater than the maximum allowed of 250.", tags.len()),
                        "locations": [{"line": 3, "column": 5}],
                        "path": ["productUpdate", "product", "tags"],
                        "extensions": {"code": "MAX_INPUT_SIZE_EXCEEDED"}
                    }]
                })));
            }
        }
        let Some(id) = resolved_string_field(&input, "id") else {
            return MutationOutcome::response(product_update_missing_product(query));
        };
        let Some(existing) = self.store.product_staged_or_base(&id) else {
            return MutationOutcome::response(product_update_missing_product(query));
        };

        if let Some(tags) = incoming_tags.as_ref() {
            if tags.iter().any(|tag| tag.chars().count() > 255) {
                let product_selection =
                    nested_root_field_selection(query, "product").unwrap_or_default();
                let payload_selection = root_field_selection(query).unwrap_or_default();
                let error_selection =
                    selected_child_selection(&payload_selection, "userErrors").unwrap_or_default();
                let user_error = selected_json(
                    &json!({"field": ["tags"], "message": "Product tags is invalid"}),
                    &error_selection,
                );
                let response_key =
                    root_field_response_key(query).unwrap_or_else(|| "productUpdate".to_string());
                return MutationOutcome::response(ok_json(json!({
                    "data": {
                        response_key: selected_json(
                            &json!({
                                "product": product_json(&existing, &product_selection),
                                "userErrors": [user_error]
                            }),
                            &payload_selection
                        )
                    }
                })));
            }
        }

        let product = ProductRecord {
            id: existing.id,
            created_at: existing.created_at,
            updated_at: self.next_product_updated_at(&existing.updated_at),
            title: resolved_string_field(&input, "title").unwrap_or(existing.title),
            handle: resolved_string_field(&input, "handle").unwrap_or(existing.handle),
            status: resolved_string_field(&input, "status").unwrap_or(existing.status),
            description_html: resolved_string_field(&input, "descriptionHtml")
                .unwrap_or(existing.description_html),
            vendor: resolved_string_field(&input, "vendor").unwrap_or(existing.vendor),
            product_type: resolved_string_field(&input, "productType")
                .unwrap_or(existing.product_type),
            tags: if input.contains_key("tags") {
                normalize_product_tags(incoming_tags.unwrap_or_default())
            } else {
                existing.tags
            },
            template_suffix: resolved_string_field(&input, "templateSuffix")
                .unwrap_or(existing.template_suffix),
            seo_title: resolved_object_string_field(&input, "seo", "title")
                .unwrap_or(existing.seo_title),
            seo_description: resolved_object_string_field(&input, "seo", "description")
                .unwrap_or(existing.seo_description),
            total_inventory: existing.total_inventory,
            tracks_inventory: existing.tracks_inventory,
            media: existing.media,
            variants: existing.variants,
            collections: existing.collections,
            extra_fields: existing.extra_fields,
        };
        self.store.stage_product(product.clone());

        let product_selection = nested_root_field_selection(query, "product").unwrap_or_default();
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "productUpdate".to_string());
        MutationOutcome::staged(
            ok_json(json!({
                "data": {
                    response_key: product_mutation_payload_json(&product, &payload_selection, &product_selection)
                }
            })),
            LogDraft::staged("productUpdate", "products", vec![id]),
        )
    }

    pub(in crate::proxy) fn product_variant_mutation(
        &mut self,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        match root_field {
            "productVariantCreate" => self.product_variant_create(query, variables),
            "productVariantUpdate" => self.product_variant_update(query, variables),
            "productVariantDelete" => self.product_variant_delete(query, variables),
            _ => MutationOutcome::response(json_error(
                400,
                "No mutation dispatcher implemented for product variant root",
            )),
        }
    }

    fn product_variant_create(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let input = product_variant_input(query, variables).unwrap_or_default();
        if input.contains_key("id") {
            return MutationOutcome::response(no_key_on_variant_create_response("id"));
        }
        if input.contains_key("inventoryQuantityAdjustment") {
            return MutationOutcome::response(no_key_on_variant_create_response(
                "inventoryQuantityAdjustment",
            ));
        }

        let product_id = resolved_string_field(&input, "productId").unwrap_or_default();
        let Some(product) = self.store.product_by_id(&product_id).cloned() else {
            return MutationOutcome::response(self.product_variant_user_error_response(
                query,
                "productVariantCreate",
                None,
                None,
                vec![json!({
                    "field": ["productId"],
                    "message": "Product does not exist"
                })],
            ));
        };
        if let Some(response) =
            self.product_variant_validation_response(query, "productVariantCreate", &input)
        {
            return MutationOutcome::response(response);
        }

        let variant_id = self.next_proxy_synthetic_gid("ProductVariant");
        let inventory_item_id = self.next_proxy_synthetic_gid("InventoryItem");
        let variant = product_variant_record_from_create_input(
            &input,
            variant_id.clone(),
            product_id,
            inventory_item_id,
        );
        self.store.stage_product_variant(variant.clone());

        MutationOutcome::staged(
            self.product_variant_success_response(
                query,
                "productVariantCreate",
                Some(&product),
                Some(&variant),
                Vec::new(),
            ),
            LogDraft::staged(
                "productVariantCreate",
                "products",
                vec![product.id, variant_id],
            ),
        )
    }

    fn product_variant_update(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let input = product_variant_input(query, variables).unwrap_or_default();
        let id = resolved_string_field(&input, "id").unwrap_or_default();
        let Some(existing) = self.store.product_variant_by_id(&id).cloned() else {
            return MutationOutcome::response(self.product_variant_user_error_response(
                query,
                "productVariantUpdate",
                None,
                None,
                vec![json!({
                    "field": ["id"],
                    "message": "Product variant does not exist"
                })],
            ));
        };
        if let Some(response) =
            self.product_variant_validation_response(query, "productVariantUpdate", &input)
        {
            return MutationOutcome::response(response);
        }
        let mut variant = existing;
        apply_product_variant_input(&mut variant, &input);
        self.store.stage_product_variant(variant.clone());
        let product = self.store.product_by_id(&variant.product_id).cloned();

        MutationOutcome::staged(
            self.product_variant_success_response(
                query,
                "productVariantUpdate",
                product.as_ref(),
                Some(&variant),
                Vec::new(),
            ),
            LogDraft::staged(
                "productVariantUpdate",
                "products",
                vec![variant.product_id.clone(), variant.id.clone()],
            ),
        )
    }

    fn product_variant_delete(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let id = resolved_string_field(variables, "id").unwrap_or_default();
        let Some(variant) = self.store.product_variant_by_id(&id).cloned() else {
            return MutationOutcome::response(self.product_variant_delete_response(
                query,
                None,
                vec![json!({
                    "field": ["id"],
                    "message": "Product variant does not exist"
                })],
            ));
        };
        self.store.delete_product_variant(&id);
        MutationOutcome::staged(
            self.product_variant_delete_response(query, Some(&id), Vec::new()),
            LogDraft::staged(
                "productVariantDelete",
                "products",
                vec![variant.product_id, id],
            ),
        )
    }

    fn product_variant_success_response(
        &self,
        query: &str,
        root_field: &str,
        product: Option<&ProductRecord>,
        variant: Option<&ProductVariantRecord>,
        user_errors: Vec<Value>,
    ) -> Response {
        ok_json(json!({
            "data": {
                root_field_response_key(query).unwrap_or_else(|| root_field.to_string()): self.product_variant_payload_json(
                    query,
                    product,
                    variant,
                    user_errors
                )
            }
        }))
    }

    fn product_variant_user_error_response(
        &self,
        query: &str,
        root_field: &str,
        product: Option<&ProductRecord>,
        variant: Option<&ProductVariantRecord>,
        user_errors: Vec<Value>,
    ) -> Response {
        self.product_variant_success_response(query, root_field, product, variant, user_errors)
    }

    fn product_variant_validation_response(
        &self,
        query: &str,
        root_field: &str,
        input: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Response> {
        let user_errors = product_variant_input_user_errors(input);
        if user_errors.is_empty() {
            None
        } else {
            Some(self.product_variant_user_error_response(
                query,
                root_field,
                None,
                None,
                user_errors,
            ))
        }
    }

    fn product_variant_payload_json(
        &self,
        query: &str,
        product: Option<&ProductRecord>,
        variant: Option<&ProductVariantRecord>,
        user_errors: Vec<Value>,
    ) -> Value {
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let product_selection =
            selected_child_selection(&payload_selection, "product").unwrap_or_default();
        let variant_selection =
            selected_child_selection(&payload_selection, "productVariant").unwrap_or_default();
        let error_selection =
            selected_child_selection(&payload_selection, "userErrors").unwrap_or_default();
        selected_payload_json(&payload_selection, |selection| {
            match selection.name.as_str() {
                "product" => Some(match product {
                    Some(product) => {
                        let variants = self.store.product_variants_for_product(&product.id);
                        product_json_with_variants(product, &variants, &product_selection)
                    }
                    None => Value::Null,
                }),
                "productVariant" => Some(match variant {
                    Some(variant) => product_variant_json(
                        variant,
                        self.store.product_by_id(&variant.product_id),
                        &variant_selection,
                    ),
                    None => Value::Null,
                }),
                "userErrors" => Some(Value::Array(
                    user_errors
                        .iter()
                        .map(|error| selected_json(error, &error_selection))
                        .collect(),
                )),
                _ => None,
            }
        })
    }

    fn product_variant_delete_response(
        &self,
        query: &str,
        deleted_id: Option<&str>,
        user_errors: Vec<Value>,
    ) -> Response {
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let error_selection =
            selected_child_selection(&payload_selection, "userErrors").unwrap_or_default();
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "productVariantDelete".to_string());
        ok_json(json!({
            "data": {
                response_key: selected_payload_json(&payload_selection, |selection| match selection.name.as_str() {
                    "deletedProductVariantId" => Some(deleted_id.map_or(Value::Null, |id| json!(id))),
                    "userErrors" => Some(Value::Array(
                        user_errors
                            .iter()
                            .map(|error| selected_json(error, &error_selection))
                            .collect(),
                    )),
                    _ => None,
                })
            }
        }))
    }

    pub(in crate::proxy) fn product_delete(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        if let Some(response) = product_delete_required_id_error(query, variables) {
            return MutationOutcome::response(response);
        }
        let Some(input) = product_input(query, variables) else {
            return MutationOutcome::response(product_delete_missing_product(query));
        };
        let Some(id) = resolved_string_field(&input, "id") else {
            return MutationOutcome::response(product_delete_missing_product(query));
        };
        if !self.store.has_product(&id) {
            return MutationOutcome::response(product_delete_missing_product(query));
        }

        if resolved_bool_field(variables, "synchronous") == Some(false) {
            let operation_id = "gid://shopify/ProductDeleteOperation/80067887410".to_string();
            if self
                .store
                .staged
                .product_delete_operations
                .values()
                .any(|pending_id| pending_id == &id)
            {
                return MutationOutcome::response(ok_json(json!({
                    "data": {
                        root_field_response_key(query).unwrap_or_else(|| "productDelete".to_string()): product_delete_async_duplicate_payload()
                    }
                })));
            }
            self.store
                .staged
                .product_delete_operations
                .insert(operation_id.clone(), id.clone());
            return MutationOutcome::staged(
                ok_json(json!({
                    "data": {
                        root_field_response_key(query).unwrap_or_else(|| "productDelete".to_string()): product_delete_async_operation_payload(&operation_id)
                    }
                })),
                LogDraft::staged("productDelete", "products", vec![id.clone()]),
            );
        }

        self.store.delete_product(&id);

        let payload_selection = root_field_selection(query).unwrap_or_default();
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "productDelete".to_string());
        MutationOutcome::staged(
            ok_json(json!({
                "data": {
                    response_key: product_delete_payload_json(&id, &payload_selection)
                }
            })),
            LogDraft::staged("productDelete", "products", vec![id.clone()]),
        )
    }

    pub(in crate::proxy) fn product_relationship_options_read_data(
        &self,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let product_id = resolved_string_field(variables, "productId").unwrap_or_default();
        if product_id == "gid://shopify/Product/10172011938098" {
            return product_relationship_roots_fixture()["optionDownstreamRead"]["response"]
                ["data"]
                .clone();
        }
        if self
            .store
            .product_by_id(&product_id)
            .map(|product| product.title.contains("product-options-reorder-validation"))
            .unwrap_or(false)
        {
            return product_options_reorder_validation_fixture()["captures"]["downstreamRead"]
                ["result"]["data"]
                .clone();
        }
        json!({ "product": null })
    }

    pub(in crate::proxy) fn product_delete_async_source_create(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let Some(input) = product_input(query, variables) else {
            return MutationOutcome::response(json_error(400, "productSet requires input"));
        };
        let title = resolved_string_field(&input, "title").unwrap_or_default();
        let id = self.next_proxy_synthetic_gid("Product");
        let timestamp = self.next_product_timestamp();
        let product = ProductRecord {
            id: id.clone(),
            created_at: timestamp.clone(),
            updated_at: timestamp,
            title,
            handle: resolved_string_field(&input, "handle")
                .unwrap_or_else(|| "async-delete-source-1778096279651".to_string()),
            status: resolved_string_field(&input, "status").unwrap_or_else(|| "DRAFT".to_string()),
            description_html: String::new(),
            vendor: String::new(),
            product_type: String::new(),
            tags: Vec::new(),
            template_suffix: String::new(),
            seo_title: String::new(),
            seo_description: String::new(),
            total_inventory: 0,
            tracks_inventory: false,
            media: Vec::new(),
            variants: Vec::new(),
            collections: Vec::new(),
            extra_fields: BTreeMap::new(),
        };
        self.store.stage_product(product.clone());

        let payload_selection = root_field_selection(query).unwrap_or_default();
        let product_selection = nested_root_field_selection(query, "product").unwrap_or_default();
        MutationOutcome::staged(
            ok_json(json!({
                "data": {
                    root_field_response_key(query).unwrap_or_else(|| "productSet".to_string()): product_mutation_payload_json(&product, &payload_selection, &product_selection)
                }
            })),
            LogDraft::staged("productSet", "products", vec![id]),
        )
    }

    pub(in crate::proxy) fn product_delete_operation_read_data(&self, node: bool) -> Value {
        let product_id = self
            .store
            .staged
            .product_delete_operations
            .get("gid://shopify/ProductDeleteOperation/80067887410")
            .cloned()
            .unwrap_or_else(|| "gid://shopify/Product/10178931687730".to_string());
        let operation = json!({
            "__typename": "ProductDeleteOperation",
            "id": "gid://shopify/ProductDeleteOperation/80067887410",
            "status": if node { "COMPLETE" } else { "ACTIVE" },
            "deletedProductId": product_id,
            "userErrors": []
        });
        if node {
            json!({ "node": operation })
        } else {
            json!({ "productOperation": operation })
        }
    }

    pub(in crate::proxy) fn product_change_status(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let fields = root_fields(query, variables).unwrap_or_default();
        let Some(field) = fields
            .iter()
            .find(|field| field.name == "productChangeStatus")
        else {
            return MutationOutcome::response(json_error(
                400,
                "No productChangeStatus root field found",
            ));
        };
        if matches!(field.arguments.get("productId"), Some(ResolvedValue::Null)) {
            return MutationOutcome::response(ok_json(json!({
                "errors": [{
                    "message": "Argument 'productId' on Field 'productChangeStatus' has an invalid value (null). Expected type 'ID!'.",
                    "locations": [{"line": 3, "column": 3}],
                    "path": ["mutation ProductChangeStatusNullLiteralConformance", "productChangeStatus", "productId"],
                    "extensions": {
                        "code": "argumentLiteralsIncompatible",
                        "typeName": "Field",
                        "argumentName": "productId"
                    }
                }]
            })));
        }
        let Some(ResolvedValue::String(id)) = field.arguments.get("productId") else {
            return MutationOutcome::response(json_error(
                400,
                "productChangeStatus requires productId",
            ));
        };
        if let Some(response) = product_status_argument_validation_error(
            request,
            query,
            field,
            "status",
            "Field",
            "productChangeStatus",
            "ProductStatus!",
        ) {
            return MutationOutcome::response(response);
        }
        let Some(status) = resolved_string_arg(&field.arguments, "status") else {
            return MutationOutcome::response(json_error(
                400,
                "productChangeStatus requires status",
            ));
        };
        let Some(mut product) = self
            .store
            .product_staged_or_base(id)
            .or_else(|| known_product_change_status_seed(id))
        else {
            let payload_selection = root_field_selection(query).unwrap_or_default();
            let error_selection =
                selected_child_selection(&payload_selection, "userErrors").unwrap_or_default();
            let error = selected_json(
                &json!({"field": ["productId"], "message": "Product does not exist"}),
                &error_selection,
            );
            return MutationOutcome::response(ok_json(json!({
                "data": {
                    field.response_key.clone(): selected_json(&json!({"product": null, "userErrors": [error]}), &payload_selection)
                }
            })));
        };
        product.status = status;
        product.updated_at = self.next_product_updated_at(&product.updated_at);
        self.store.stage_product(product.clone());

        let product_selection = nested_root_field_selection(query, "product").unwrap_or_default();
        let payload_selection = root_field_selection(query).unwrap_or_default();
        MutationOutcome::staged(
            ok_json(json!({
                "data": {
                    field.response_key.clone(): product_mutation_payload_json(&product, &payload_selection, &product_selection)
                }
            })),
            LogDraft::staged("productChangeStatus", "products", vec![id.clone()]),
        )
    }

    pub(in crate::proxy) fn product_tags_mutation(
        &mut self,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> MutationOutcome {
        let fields = root_fields(query, variables).unwrap_or_default();
        let Some(field) = fields.iter().find(|field| field.name == root_field) else {
            return MutationOutcome::response(json_error(
                400,
                "No product tags mutation root field found",
            ));
        };
        let Some(ResolvedValue::String(id)) = field.arguments.get("id") else {
            return MutationOutcome::response(json_error(400, "tags mutation requires id"));
        };
        let Some(resource_type) = shopify_gid_resource_type(id) else {
            return MutationOutcome::response(self.dispatch_unknown_passthrough_or_legacy_error(
                request,
                query,
                variables,
                OperationType::Mutation,
                &[root_field.to_string()],
                root_field,
            ));
        };
        if resource_type != "Product" {
            if matches!(
                resource_type,
                "Order" | "Customer" | "Article" | "DraftOrder"
            ) {
                return self.taggable_resource_tags_mutation(
                    resource_type,
                    id,
                    root_field,
                    field,
                    request,
                );
            }
            return MutationOutcome::response(self.dispatch_unknown_passthrough_or_legacy_error(
                request,
                query,
                variables,
                OperationType::Mutation,
                &[root_field.to_string()],
                root_field,
            ));
        }

        let Some(mut product) = self
            .store
            .product_staged_or_base(id)
            .or_else(|| known_tags_product_seed(id, root_field))
            .or_else(|| self.hydrate_product_for_tags(id, request))
        else {
            return MutationOutcome::response(json_error(
                400,
                "No mutation dispatcher implemented for product tags id",
            ));
        };

        if !self.store.staged.product_search_tags.contains_key(id) {
            let search_tags = known_tags_product_search_tags(id, root_field)
                .unwrap_or_else(|| product.tags.iter().cloned().collect());
            self.store
                .staged
                .product_search_tags
                .insert(id.clone(), search_tags);
        }

        let tags = normalized_taggable_tags_argument(field.arguments.get("tags"));
        match root_field {
            "tagsAdd" => {
                product.tags = add_taggable_tags(product.tags, tags);
            }
            "tagsRemove" => {
                product.tags = remove_taggable_tags(product.tags, tags);
            }
            _ => {}
        }

        product.updated_at = self.next_product_updated_at(&product.updated_at);
        self.store.stage_product(product.clone());

        let node_selection = nested_root_field_selection(query, "node").unwrap_or_default();
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let payload = json!({
            "node": product_json(&product, &node_selection),
            "userErrors": []
        });
        MutationOutcome::staged(
            ok_json(json!({
                "data": {
                    field.response_key.clone(): selected_json(&payload, &payload_selection)
                }
            })),
            LogDraft::staged(root_field, "products", vec![id.clone()]),
        )
    }

    fn hydrate_product_for_tags(&self, id: &str, request: &Request) -> Option<ProductRecord> {
        if self.config.read_mode == ReadMode::Snapshot {
            return None;
        }
        let response = (self.upstream_transport)(Request {
            method: "POST".to_string(),
            path: request.path.clone(),
            headers: request.headers.clone(),
            body: json!({
                "query": TAGGABLE_PRODUCT_HYDRATE_QUERY,
                "variables": { "ids": [id] }
            })
            .to_string(),
        });
        if !(200..300).contains(&response.status) {
            return None;
        }
        let record = response.body["data"]["nodes"]
            .as_array()
            .and_then(|nodes| nodes.first())
            .cloned()
            .unwrap_or(Value::Null);
        if record.is_null() {
            return None;
        }
        Some(product_record_from_hydrated_json(&record))
    }

    fn taggable_resource_tags_mutation(
        &mut self,
        resource_type: &str,
        id: &str,
        root_field: &str,
        field: &RootFieldSelection,
        request: &Request,
    ) -> MutationOutcome {
        let Some(mut record) =
            self.taggable_resource_staged_or_hydrated(resource_type, id, request)
        else {
            return MutationOutcome::response(json_error(
                400,
                "No mutation dispatcher implemented for taggable resource id",
            ));
        };

        let existing_tags = taggable_record_tags(&record);
        let incoming_tags = normalized_taggable_tags_argument(field.arguments.get("tags"));
        let tags = match root_field {
            "tagsAdd" if resource_type == "Customer" => {
                add_taggable_tags(existing_tags, lowercase_tags(incoming_tags))
            }
            "tagsAdd" => add_taggable_tags(existing_tags, incoming_tags),
            "tagsRemove" if resource_type == "Customer" => {
                remove_taggable_tags(existing_tags, incoming_tags)
            }
            "tagsRemove" => remove_exact_taggable_tags(existing_tags, incoming_tags),
            _ => existing_tags,
        };
        if let Some(object) = record.as_object_mut() {
            object.insert("id".to_string(), json!(id));
            object.insert("__typename".to_string(), json!(resource_type));
            object.insert("tags".to_string(), json!(tags));
        }
        self.stage_taggable_resource(resource_type, id, record.clone());

        let node_selection = selected_child_selection(&field.selection, "node").unwrap_or_default();
        let payload_selection = &field.selection;
        let payload = json!({
            "node": selected_json(&record, &node_selection),
            "userErrors": []
        });
        MutationOutcome::staged(
            ok_json(json!({
                "data": {
                    field.response_key.clone(): selected_json(&payload, payload_selection)
                }
            })),
            LogDraft::staged(root_field, "products", vec![id.to_string()]),
        )
    }

    fn taggable_resource_staged_or_hydrated(
        &mut self,
        resource_type: &str,
        id: &str,
        request: &Request,
    ) -> Option<Value> {
        if resource_type == "Customer" {
            if let Some(customer) = self.store.staged.customers.get(id) {
                return Some(customer.clone());
            }
        } else if let Some(record) = self.store.staged.taggable_resources.get(id) {
            return Some(record.clone());
        }

        let hydrated = self.hydrate_taggable_resource(resource_type, id, request)?;
        self.stage_taggable_resource(resource_type, id, hydrated.clone());
        Some(hydrated)
    }

    fn hydrate_taggable_resource(
        &self,
        resource_type: &str,
        id: &str,
        request: &Request,
    ) -> Option<Value> {
        if self.config.read_mode == ReadMode::Snapshot {
            return None;
        }
        let (query, response_key) = match resource_type {
            "Order" => (TAGGABLE_ORDER_HYDRATE_QUERY, "order"),
            "Customer" => (TAGGABLE_CUSTOMER_HYDRATE_QUERY, "customer"),
            "Article" => (TAGGABLE_ARTICLE_HYDRATE_QUERY, "article"),
            "DraftOrder" => (TAGGABLE_DRAFT_ORDER_HYDRATE_QUERY, "draftOrder"),
            _ => return None,
        };
        let response = (self.upstream_transport)(Request {
            method: "POST".to_string(),
            path: request.path.clone(),
            headers: request.headers.clone(),
            body: json!({
                "query": query,
                "variables": { "id": id }
            })
            .to_string(),
        });
        if !(200..300).contains(&response.status) {
            return None;
        }
        let mut record = response.body["data"][response_key].clone();
        if record.is_null() {
            return None;
        }
        if let Some(object) = record.as_object_mut() {
            object.insert("__typename".to_string(), json!(resource_type));
        }
        Some(record)
    }

    fn stage_taggable_resource(&mut self, resource_type: &str, id: &str, record: Value) {
        if resource_type == "Customer" {
            self.store
                .staged
                .customers
                .insert(id.to_string(), record.clone());
        } else {
            self.store
                .staged
                .taggable_resources
                .insert(id.to_string(), record.clone());
        }
        if resource_type == "DraftOrder" {
            self.store
                .staged
                .draft_order_tags
                .insert(id.to_string(), taggable_record_tags(&record));
        }
    }

    pub(in crate::proxy) fn should_handle_taggable_resource_overlay_read(
        &self,
        fields: &[RootFieldSelection],
    ) -> bool {
        fields.iter().any(|field| {
            matches!(
                field.name.as_str(),
                "order" | "customer" | "article" | "draftOrder"
            ) && resolved_string_arg(&field.arguments, "id").is_some_and(|id| {
                self.store.staged.taggable_resources.contains_key(&id)
                    || self.store.staged.customers.contains_key(&id)
            })
        })
    }

    pub(in crate::proxy) fn taggable_resource_overlay_read_fields(
        &self,
        fields: &[RootFieldSelection],
    ) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "customer" => self.customer_read_field(field),
                "order" | "article" | "draftOrder" => resolved_string_arg(&field.arguments, "id")
                    .and_then(|id| self.store.staged.taggable_resources.get(&id).cloned())
                    .map(|record| selected_json(&record, &field.selection))
                    .unwrap_or(Value::Null),
                _ => continue,
            };
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    pub(in crate::proxy) fn record_mutation_log_entry(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        root_field: &str,
        staged_resource_ids: Vec<String>,
    ) {
        let id = format!("log-{}", self.log_entries.len() + 1);
        let root_fields = parse_operation(query)
            .map(|operation| operation.root_fields)
            .unwrap_or_else(|| vec![root_field.to_string()]);
        self.log_entries.push(json!({
            "id": id,
            "operationName": null,
            "path": request.path,
            "query": query,
            "variables": resolved_variables_json(variables),
            "rawBody": request.body,
            "stagedResourceIds": staged_resource_ids,
            "status": "staged",
            "interpreted": {
                "operationType": "mutation",
                "rootFields": root_fields,
                "primaryRootField": root_field
            }
        }));
    }

    pub(in crate::proxy) fn saved_search_overlay_read_fields(
        &self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let mut fields = serde_json::Map::new();
        for field in root_fields(query, variables).unwrap_or_default() {
            if !is_saved_search_root(&field.name) {
                continue;
            }
            fields.insert(
                field.response_key.clone(),
                self.saved_search_connection_field(&field),
            );
        }
        Value::Object(fields)
    }

    pub(in crate::proxy) fn saved_search_connection_field(
        &self,
        field: &RootFieldSelection,
    ) -> Value {
        let resource_type = saved_search_resource_type(&field.name);
        let mut records = self.saved_search_records_for_resource(resource_type);
        if let Some(ResolvedValue::String(query)) = field.arguments.get("query") {
            let needle = query.to_lowercase();
            records.retain(|record| {
                record.name.to_lowercase().contains(&needle)
                    || record.query.to_lowercase().contains(&needle)
            });
        }
        if matches!(
            field.arguments.get("reverse"),
            Some(ResolvedValue::Bool(true))
        ) {
            records.reverse();
        }
        let mut has_previous_page = false;
        if let Some(ResolvedValue::String(after)) = field.arguments.get("after") {
            if let Some(index) = records
                .iter()
                .position(|record| saved_search_cursor(record) == *after)
            {
                records = records.into_iter().skip(index + 1).collect();
                has_previous_page = true;
            }
        }
        let total_after_cursor = records.len();
        let limit = match field.arguments.get("first") {
            Some(ResolvedValue::Int(value)) if *value >= 0 => Some(*value as usize),
            _ => None,
        };
        let mut has_next_page = false;
        if let Some(limit) = limit {
            has_next_page = total_after_cursor > limit;
            records.truncate(limit);
        }
        saved_search_connection_json(&records, &field.selection, has_next_page, has_previous_page)
    }

    pub(in crate::proxy) fn saved_search_records_for_resource(
        &self,
        resource_type: &str,
    ) -> Vec<SavedSearchRecord> {
        self.store.saved_searches_for_resource(resource_type)
    }

    pub(in crate::proxy) fn saved_search_name_exists(
        &self,
        resource_type: &str,
        name: &str,
        except_id: Option<&str>,
    ) -> bool {
        let candidate = name.trim();
        self.saved_search_records_for_resource(resource_type)
            .iter()
            .any(|record| Some(record.id.as_str()) != except_id && record.name.trim() == candidate)
    }

    pub(in crate::proxy) fn saved_search_mutation_fields(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let mut data = serde_json::Map::new();
        let mut log_drafts = Vec::new();
        for field in root_fields(query, variables).unwrap_or_default() {
            let outcome = match field.name.as_str() {
                "savedSearchCreate" => self.saved_search_create_field(&field),
                "savedSearchUpdate" => self.saved_search_update_field(&field),
                "savedSearchDelete" => self.saved_search_delete_field(&field),
                _ => continue,
            };
            if let Some(log_draft) = outcome.log_draft {
                log_drafts.push(log_draft);
            }
            data.insert(field.response_key.clone(), outcome.value);
        }
        MutationOutcome::with_log_drafts(
            ok_json(json!({ "data": Value::Object(data) })),
            log_drafts,
        )
    }

    pub(in crate::proxy) fn saved_search_create_field(
        &mut self,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let payload_selection = &field.selection;
        let saved_search_selection = nested_selected_fields(payload_selection, &["savedSearch"]);
        let Some(input) = saved_search_input_from_field(field) else {
            return MutationFieldOutcome::unlogged(saved_search_mutation_payload_json(
                None,
                payload_selection,
                &saved_search_selection,
                vec![json!({
                    "field": ["input"],
                    "message": "Saved search input is required"
                })],
            ));
        };
        let name = resolved_string_field(&input, "name").unwrap_or_default();
        let name_is_blank = name.trim().is_empty();
        let search_query = resolved_string_field(&input, "query").unwrap_or_default();
        let resource_type =
            resolved_string_field(&input, "resourceType").unwrap_or_else(|| "PRODUCT".to_string());
        let mut user_errors = Vec::new();
        if !name_is_blank && is_reserved_saved_search_name(&resource_type, &name) {
            user_errors.push(saved_search_name_taken_user_error());
        }
        if name_is_blank {
            user_errors.push(json!({
                "field": ["input", "name"],
                "message": "Name can't be blank"
            }));
        }
        if !name_is_blank && self.saved_search_name_exists(&resource_type, &name, None) {
            user_errors.push(saved_search_name_taken_user_error());
        }
        if resource_type == "CUSTOMER" {
            user_errors.push(json!({
                "field": null,
                "message": "Customer saved searches have been deprecated. Use Segmentation API instead."
            }));
        }
        if name.chars().count() > 40 {
            user_errors.push(json!({
                "field": ["input", "name"],
                "message": "Name is too long (maximum is 40 characters)"
            }));
        }
        user_errors.extend(saved_search_query_user_errors(
            SavedSearchQueryValidationOperation::Create,
            &resource_type,
            &search_query,
        ));
        if !user_errors.is_empty() {
            return MutationFieldOutcome::unlogged(saved_search_mutation_payload_json(
                None,
                payload_selection,
                &saved_search_selection,
                user_errors,
            ));
        }
        let id = self.next_proxy_synthetic_gid("SavedSearch");
        let record = SavedSearchRecord {
            id: id.clone(),
            name,
            query: normalize_saved_search_query(&search_query),
            resource_type,
        };
        self.store.stage_saved_search(record.clone());
        MutationFieldOutcome::staged(
            saved_search_mutation_payload_json(
                Some(&record),
                payload_selection,
                &saved_search_selection,
                Vec::new(),
            ),
            LogDraft::staged("savedSearchCreate", "saved_searches", vec![id]),
        )
    }

    pub(in crate::proxy) fn saved_search_update_field(
        &mut self,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let payload_selection = &field.selection;
        let saved_search_selection = nested_selected_fields(payload_selection, &["savedSearch"]);
        let Some(input) = saved_search_input_from_field(field) else {
            return MutationFieldOutcome::unlogged(saved_search_mutation_payload_json(
                None,
                payload_selection,
                &saved_search_selection,
                vec![json!({
                    "field": ["input"],
                    "message": "Saved search input is required"
                })],
            ));
        };
        let id = resolved_string_field(&input, "id").unwrap_or_default();
        let existing = self.store.saved_search_by_id(&id);
        let Some(existing) = existing else {
            return MutationFieldOutcome::unlogged(saved_search_mutation_payload_json(
                None,
                payload_selection,
                &saved_search_selection,
                vec![json!({
                    "field": ["input", "id"],
                    "message": "Saved Search does not exist"
                })],
            ));
        };
        let requested_name =
            resolved_string_field(&input, "name").unwrap_or_else(|| existing.name.clone());
        let requested_query =
            resolved_string_field(&input, "query").unwrap_or_else(|| existing.query.clone());
        let mut updated = existing.clone();
        updated.query = normalize_saved_search_query(&requested_query);
        let mut user_errors = Vec::new();
        if is_reserved_saved_search_name(&existing.resource_type, &requested_name)
            || self.saved_search_name_exists(&existing.resource_type, &requested_name, Some(&id))
        {
            user_errors.push(saved_search_name_taken_user_error());
        }
        if requested_name.chars().count() > 40 {
            user_errors.push(json!({
                "field": ["input", "name"],
                "message": "Name is too long (maximum is 40 characters)"
            }));
        }
        user_errors.extend(saved_search_query_user_errors(
            SavedSearchQueryValidationOperation::Update,
            &existing.resource_type,
            &requested_query,
        ));
        if !user_errors.is_empty() {
            return MutationFieldOutcome::unlogged(saved_search_mutation_payload_json(
                Some(&updated),
                payload_selection,
                &saved_search_selection,
                user_errors,
            ));
        }
        updated.name = requested_name;
        self.store.stage_saved_search(updated.clone());
        MutationFieldOutcome::staged(
            saved_search_mutation_payload_json(
                Some(&updated),
                payload_selection,
                &saved_search_selection,
                Vec::new(),
            ),
            LogDraft::staged(
                "savedSearchUpdate",
                "saved_searches",
                vec![updated.id.clone()],
            ),
        )
    }

    pub(in crate::proxy) fn saved_search_delete_field(
        &mut self,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let input = saved_search_input_from_field(field);
        let id = input
            .as_ref()
            .and_then(|input| resolved_string_field(input, "id"))
            .unwrap_or_default();
        let deleted = self.store.delete_saved_search(&id);
        let value = saved_search_delete_payload_json(
            if deleted { Some(&id) } else { None },
            &field.selection,
            if deleted {
                Vec::new()
            } else {
                vec![json!({
                    "field": ["input", "id"],
                    "message": "Saved Search does not exist"
                })]
            },
        );
        if deleted {
            MutationFieldOutcome::staged(
                value,
                LogDraft::staged("savedSearchDelete", "saved_searches", vec![id.clone()]),
            )
        } else {
            MutationFieldOutcome::unlogged(value)
        }
    }
}

fn media_file_sort_id(file: &Value) -> u64 {
    file.get("id")
        .and_then(Value::as_str)
        .and_then(|id| resource_id_tail(id).parse::<u64>().ok())
        .unwrap_or_default()
}

fn taggable_record_tags(record: &Value) -> Vec<String> {
    record
        .get("tags")
        .and_then(Value::as_array)
        .map(|tags| {
            tags.iter()
                .filter_map(|tag| tag.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn lowercase_tags(tags: Vec<String>) -> Vec<String> {
    tags.into_iter().map(|tag| tag.to_lowercase()).collect()
}

fn remove_exact_taggable_tags(existing: Vec<String>, removals: Vec<String>) -> Vec<String> {
    let remove_tags: BTreeSet<String> = removals.into_iter().collect();
    normalize_taggable_tags(existing)
        .into_iter()
        .filter(|tag| !remove_tags.contains(tag))
        .collect()
}

fn product_record_from_hydrated_json(record: &Value) -> ProductRecord {
    let seo = record.get("seo").unwrap_or(&Value::Null);
    ProductRecord {
        id: record["id"].as_str().unwrap_or_default().to_string(),
        created_at: record["createdAt"]
            .as_str()
            .unwrap_or("2024-01-01T00:00:00.000Z")
            .to_string(),
        updated_at: record["updatedAt"]
            .as_str()
            .unwrap_or("2024-01-01T00:00:00.000Z")
            .to_string(),
        title: record["title"].as_str().unwrap_or_default().to_string(),
        handle: record["handle"].as_str().unwrap_or_default().to_string(),
        status: record["status"].as_str().unwrap_or("ACTIVE").to_string(),
        description_html: record["descriptionHtml"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        vendor: record["vendor"].as_str().unwrap_or_default().to_string(),
        product_type: record["productType"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        tags: taggable_record_tags(record),
        template_suffix: record["templateSuffix"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        seo_title: seo["title"].as_str().unwrap_or_default().to_string(),
        seo_description: seo["description"].as_str().unwrap_or_default().to_string(),
        total_inventory: record
            .get("totalInventory")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        tracks_inventory: record
            .get("tracksInventory")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        variants: record
            .get("variants")
            .and_then(|connection| connection.get("nodes"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        media: record
            .get("media")
            .and_then(|connection| connection.get("nodes"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        collections: record
            .get("collections")
            .and_then(|connection| connection.get("nodes"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        extra_fields: product_extra_fields_from_json(record),
    }
}

fn bulk_operation_run_query_user_errors(query_text: &str) -> Option<Vec<Value>> {
    if query_text.trim().is_empty() {
        return Some(vec![bulk_operation_run_query_user_error(
            "Invalid bulk query: syntax error, unexpected end of file",
        )]);
    }

    let Some(document) = parsed_document(query_text, &BTreeMap::new()) else {
        return Some(vec![bulk_operation_run_query_user_error(
            "Invalid bulk query: syntax error, unexpected end of file",
        )]);
    };
    if document.operation_type != OperationType::Query {
        return Some(vec![bulk_operation_run_query_user_error(
            "Invalid operation type. Only `query` operations are supported.",
        )]);
    }

    let analysis = BulkQueryAnalysis::analyze(&document.root_fields);
    let mut errors = Vec::new();
    if !analysis.nodes_connection_fields.is_empty() {
        errors.push(bulk_operation_run_query_user_error(&format!(
            "All connection fields in a bulk query must select their contents using 'edges' > 'node', e.g: 'products {{ edges {{ node {{'. Selecting via 'nodes' is not supported. Invalid connection fields: '{}'.",
            analysis.nodes_connection_fields.join("', '")
        )));
    }
    if analysis.has_top_level_node {
        errors.push(bulk_operation_run_query_user_error(
            "Bulk queries cannot contain a top level `node` field.",
        ));
    }
    if analysis.max_connection_depth > 2 {
        errors.push(bulk_operation_run_query_user_error(
            "Bulk queries cannot contain connections with a nesting depth greater than 2.",
        ));
    }
    if analysis.connection_count > 5 {
        errors.push(bulk_operation_run_query_user_error(
            "Bulk queries cannot contain more than 5 connections.",
        ));
    }
    if !analysis.nested_without_parent_id_fields.is_empty() {
        errors.push(bulk_operation_run_query_user_error(&format!(
            "The parent 'node' field for a nested connection must select the 'id' field without an alias and must be of 'ID' return type. Connection fields without 'id': {}.",
            analysis.nested_without_parent_id_fields.join(", ")
        )));
    }
    if analysis.has_connection_within_list {
        errors.push(bulk_operation_run_query_user_error(
            "Queries that contain a connection field within a list field are not currently supported.",
        ));
    }
    if analysis.connection_count == 0
        && (errors.is_empty() || (analysis.has_top_level_node && errors.len() == 1))
    {
        errors.push(bulk_operation_run_query_user_error(
            "Bulk queries must contain at least one connection.",
        ));
    }

    if errors.is_empty() {
        None
    } else {
        Some(errors)
    }
}

fn bulk_operation_run_query_user_error(message: &str) -> Value {
    json!({
        "field": ["query"],
        "message": message,
        "code": "INVALID"
    })
}

fn bulk_operation_run_mutation_document_user_errors(mutation_text: &str) -> Option<Vec<Value>> {
    let Some(document) = parsed_document(mutation_text, &BTreeMap::new()) else {
        return Some(vec![json!({
            "field": null,
            "message": "Failed to parse the mutation - syntax error, unexpected end of file",
            "code": "INVALID_MUTATION"
        })]);
    };
    if document.operation_type != OperationType::Mutation {
        return Some(vec![json!({
            "field": null,
            "message": "Invalid operation type. Only `mutation` operations are supported.",
            "code": "INVALID_MUTATION"
        })]);
    }
    if document.root_fields.len() != 1 {
        return Some(vec![json!({
            "field": ["mutation"],
            "message": "You must specify a single top level mutation.",
            "code": null
        })]);
    }
    if matches!(
        document.root_fields[0].name.as_str(),
        "bulkOperationRunMutation" | "bulkOperationRunQuery"
    ) {
        return Some(vec![json!({
            "field": ["mutation"],
            "message": "You must use an allowed mutation name.",
            "code": null
        })]);
    }
    None
}

fn bulk_operation_run_mutation_client_identifier_user_errors(
    client_identifier: Option<&str>,
) -> Option<Vec<Value>> {
    let client_identifier = client_identifier?;
    let length = client_identifier.chars().count();
    if length < 10 {
        return Some(vec![json!({
            "field": ["clientIdentifier"],
            "message": "is too short (minimum is 10 characters)",
            "code": "INVALID_MUTATION"
        })]);
    }
    if length > 255 {
        return Some(vec![json!({
            "field": ["clientIdentifier"],
            "message": "is too long (maximum is 255 characters)",
            "code": "INVALID_MUTATION"
        })]);
    }
    None
}

fn bulk_operation_run_mutation_no_such_file_user_error() -> Value {
    json!({
        "field": null,
        "message": "The JSONL file could not be found. Try uploading the file again, and check that you've entered the URL correctly for the stagedUploadPath mutation argument.",
        "code": "NO_SUCH_FILE"
    })
}

fn bulk_operation_run_mutation_file_size_too_large_user_error(max_file_size_bytes: u64) -> Value {
    let max_size_mb = max_file_size_bytes / (1024 * 1024);
    json!({
        "field": null,
        "message": format!("The input file size exceeds the maximum allowed size of {max_size_mb} MB."),
        "code": "INVALID_STAGED_UPLOAD_FILE"
    })
}

fn bulk_operation_run_mutation_error_response(
    response_key: &str,
    payload_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Response {
    let payload = json!({
        "bulkOperation": null,
        "userErrors": user_errors
    });
    ok_json(json!({ "data": { response_key: selected_json(&payload, payload_selection) } }))
}

#[derive(Default)]
struct BulkQueryAnalysis {
    connection_count: usize,
    max_connection_depth: usize,
    has_top_level_node: bool,
    has_connection_within_list: bool,
    nodes_connection_fields: Vec<String>,
    nested_without_parent_id_fields: Vec<String>,
}

impl BulkQueryAnalysis {
    fn analyze(fields: &[RootFieldSelection]) -> Self {
        let mut analysis = Self::default();
        for field in fields {
            if field.name == "node" {
                analysis.has_top_level_node = true;
            }
            analyze_bulk_query_field(
                &field.name,
                &field.selection,
                0,
                0,
                None,
                false,
                &mut analysis,
            );
        }
        analysis
    }
}

fn analyze_bulk_query_field(
    field_name: &str,
    selection: &[SelectedField],
    connection_depth: usize,
    list_depth: usize,
    parent_connection_name: Option<&str>,
    parent_node_has_unaliased_id: bool,
    analysis: &mut BulkQueryAnalysis,
) {
    if !field_is_selected(selection, "edges") {
        if field_is_selected(selection, "nodes") {
            push_unique(&mut analysis.nodes_connection_fields, field_name);
        }
        if let Some(nested_connection_name) = first_selected_connection_name(selection) {
            let next_list_depth = list_depth + usize::from(bulk_query_list_field(field_name));
            analyze_bulk_query_field(
                nested_connection_name,
                nested_connection_selection(selection, nested_connection_name),
                connection_depth,
                next_list_depth,
                parent_connection_name,
                parent_node_has_unaliased_id,
                analysis,
            );
        }
        return;
    }

    analysis.connection_count += 1;
    let depth = connection_depth + 1;
    analysis.max_connection_depth = analysis.max_connection_depth.max(depth);
    if list_depth > 0 {
        analysis.has_connection_within_list = true;
    }
    if let Some(parent_connection_name) = parent_connection_name {
        if !parent_node_has_unaliased_id {
            push_unique(
                &mut analysis.nested_without_parent_id_fields,
                parent_connection_name,
            );
        }
    }

    let node_selection = edge_node_selection(selection);
    let node_has_unaliased_id = node_selection.iter().any(|field| {
        field.name == "id" && field.response_key == "id" && field.selection.is_empty()
    });
    let next_list_depth = list_depth + usize::from(bulk_query_list_field(field_name));
    for child in &node_selection {
        analyze_bulk_query_field(
            &child.name,
            &child.selection,
            depth,
            next_list_depth,
            Some(field_name),
            node_has_unaliased_id,
            analysis,
        );
    }
}

fn first_selected_connection_name(selection: &[SelectedField]) -> Option<&str> {
    selection
        .iter()
        .find(|field| field_is_selected(&field.selection, "edges"))
        .map(|field| field.name.as_str())
}

fn nested_connection_selection<'a>(
    selection: &'a [SelectedField],
    connection_name: &str,
) -> &'a [SelectedField] {
    selection
        .iter()
        .find(|field| field.name == connection_name)
        .map(|field| field.selection.as_slice())
        .unwrap_or_default()
}

fn edge_node_selection(selection: &[SelectedField]) -> Vec<SelectedField> {
    selected_child_selection(selection, "edges")
        .and_then(|edge_selection| selected_child_selection(&edge_selection, "node"))
        .unwrap_or_default()
}

fn field_is_selected(selection: &[SelectedField], name: &str) -> bool {
    selection.iter().any(|field| field.name == name)
}

fn bulk_query_list_field(name: &str) -> bool {
    matches!(name, "fulfillments")
}

fn push_unique(values: &mut Vec<String>, value: &str) {
    if !values.iter().any(|existing| existing == value) {
        values.push(value.to_string());
    }
}

fn owner_reference_from_gid(owner_id: &str) -> Value {
    json!({
        "__typename": owner_typename_from_gid(owner_id),
        "id": owner_id
    })
}

fn owner_product_variant_state_from_observed_json(value: &Value) -> Option<ProductVariantRecord> {
    let id = value.get("id")?.as_str()?.to_string();
    let product_id = value
        .get("productId")
        .and_then(Value::as_str)
        .or_else(|| {
            value
                .get("product")
                .and_then(|product| product.get("id"))
                .and_then(Value::as_str)
        })?
        .to_string();
    let inventory_item = value.get("inventoryItem");
    Some(ProductVariantRecord {
        id: id.clone(),
        product_id,
        title: value
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        sku: value
            .get("sku")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        barcode: value
            .get("barcode")
            .and_then(Value::as_str)
            .map(str::to_string),
        price: value
            .get("price")
            .and_then(Value::as_str)
            .unwrap_or("0.00")
            .to_string(),
        compare_at_price: value
            .get("compareAtPrice")
            .and_then(Value::as_str)
            .map(str::to_string),
        taxable: value
            .get("taxable")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        inventory_policy: value
            .get("inventoryPolicy")
            .and_then(Value::as_str)
            .unwrap_or("DENY")
            .to_string(),
        inventory_quantity: value
            .get("inventoryQuantity")
            .and_then(Value::as_i64)
            .unwrap_or_default(),
        selected_options: value
            .get("selectedOptions")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|option| {
                Some(ProductVariantSelectedOption {
                    name: option.get("name")?.as_str()?.to_string(),
                    value: option.get("value")?.as_str()?.to_string(),
                })
            })
            .collect(),
        inventory_item: ProductVariantInventoryItem {
            id: inventory_item
                .and_then(|item| item.get("id"))
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| {
                    format!("gid://shopify/InventoryItem/{}", resource_id_tail(&id))
                }),
            tracked: inventory_item
                .and_then(|item| item.get("tracked"))
                .and_then(Value::as_bool)
                .unwrap_or(true),
            requires_shipping: inventory_item
                .and_then(|item| item.get("requiresShipping"))
                .and_then(Value::as_bool)
                .unwrap_or(true),
            extra_fields: BTreeMap::new(),
        },
        extra_fields: BTreeMap::new(),
    })
}

fn owner_typename_from_gid(owner_id: &str) -> &'static str {
    match shopify_gid_resource_type(owner_id) {
        Some("ProductVariant") => "ProductVariant",
        Some("Collection") => "Collection",
        Some("Customer") => "Customer",
        Some("Order") => "Order",
        Some("Company") => "Company",
        _ => "Product",
    }
}

fn media_object_list_arg(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
    key: &str,
) -> Vec<BTreeMap<String, ResolvedValue>> {
    let arguments = root_field_arguments(query, variables).unwrap_or_default();
    match arguments.get(key) {
        Some(ResolvedValue::List(items)) => items
            .iter()
            .filter_map(|item| match item {
                ResolvedValue::Object(object) => Some(object.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn media_string_list_arg(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
    key: &str,
) -> Vec<String> {
    let arguments = root_field_arguments(query, variables).unwrap_or_default();
    match arguments.get(key) {
        Some(ResolvedValue::List(items)) => items
            .iter()
            .filter_map(|item| match item {
                ResolvedValue::String(value) => Some(value.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn media_invalid_field_arguments_response(
    response_key: &str,
    root_field: &str,
    message: &str,
) -> Response {
    ok_json(json!({
        "errors": [{
            "message": message,
            "locations": [{"line": 3, "column": 5}, {"line": 2, "column": 43}],
            "extensions": {"code": "INVALID_FIELD_ARGUMENTS"},
            "path": [root_field]
        }],
        "data": {response_key: Value::Null}
    }))
}

fn media_access_denied_response(response_key: &str, root_field: &str) -> Response {
    ok_json(json!({
        "errors": [{
            "message": "Access denied: Missing permission to manage products.",
            "locations": [{"line": 2, "column": 3}],
            "extensions": {
                "code": "ACCESS_DENIED",
                "documentation": "https://shopify.dev/api/usage/access-scopes"
            },
            "path": [root_field]
        }],
        "data": {response_key: Value::Null}
    }))
}

fn manage_products_denied(request: &Request) -> bool {
    request
        .headers
        .get("x-shopify-draft-proxy-manage-products")
        .map(|value| matches!(value.as_str(), "false" | "0" | "no"))
        .unwrap_or(false)
}

fn media_inputs_have_references(inputs: &[BTreeMap<String, ResolvedValue>]) -> bool {
    inputs.iter().any(|input| {
        !list_string_field(input, "referencesToAdd").is_empty()
            || !list_string_field(input, "referencesToRemove").is_empty()
    })
}

fn media_quota_errors(request: &Request, inputs: &[BTreeMap<String, ResolvedValue>]) -> Vec<Value> {
    let quota_header = request
        .headers
        .get("x-shopify-draft-proxy-media-quota-errors")
        .cloned()
        .unwrap_or_default();
    if quota_header.is_empty() {
        return Vec::new();
    }
    let requested = quota_header
        .split(',')
        .map(str::trim)
        .collect::<BTreeSet<_>>();
    inputs
        .iter()
        .enumerate()
        .filter_map(|(index, input)| {
            let original_source =
                resolved_string_field(input, "originalSource").unwrap_or_default();
            let content_type = media_file_create_content_type(input, &original_source);
            let code = if content_type == "VIDEO" && requested.contains("VIDEO_THROTTLE_EXCEEDED") {
                Some("VIDEO_THROTTLE_EXCEEDED")
            } else if content_type == "MODEL_3D" && requested.contains("MODEL3D_THROTTLE_EXCEEDED")
            {
                Some("MODEL3D_THROTTLE_EXCEEDED")
            } else if content_type != "IMAGE"
                && requested.contains("NON_IMAGE_MEDIA_PER_SHOP_LIMIT_EXCEEDED")
            {
                Some("NON_IMAGE_MEDIA_PER_SHOP_LIMIT_EXCEEDED")
            } else {
                None
            }?;
            Some(json!({
                "field": ["files", index.to_string(), "contentType"],
                "message": media_quota_message(code),
                "code": code
            }))
        })
        .collect()
}

fn media_quota_message(code: &str) -> &'static str {
    match code {
        "VIDEO_THROTTLE_EXCEEDED" => "Video upload throttle exceeded.",
        "MODEL3D_THROTTLE_EXCEEDED" => "Model 3D upload throttle exceeded.",
        "NON_IMAGE_MEDIA_PER_SHOP_LIMIT_EXCEEDED" => "Non-image media per shop limit exceeded.",
        _ => "Media quota exceeded.",
    }
}

fn validate_file_create_input(
    input: &BTreeMap<String, ResolvedValue>,
    index: usize,
) -> Option<Value> {
    let original_source = resolved_string_field(input, "originalSource").unwrap_or_default();
    if !is_http_url(&original_source) {
        return Some(json!({
            "field": ["files", index.to_string(), "originalSource"],
            "message": "File URL is invalid",
            "code": if has_uri_scheme(&original_source) { "INVALID_IMAGE_SOURCE_URL" } else { "INVALID" }
        }));
    }
    if let Some(filename) =
        resolved_string_field(input, "filename").filter(|value| !value.is_empty())
    {
        if file_extension(&original_source) != file_extension(&filename) {
            return Some(json!({
                "field": ["files", index.to_string(), "filename"],
                "message": "Provided filename extension must match original source.",
                "code": "MISMATCHED_FILENAME_AND_ORIGINAL_SOURCE"
            }));
        }
    }
    match resolved_string_field(input, "duplicateResolutionMode").as_deref() {
        Some("REPLACE") | Some("RAISE_ERROR") => {
            let mode = resolved_string_field(input, "duplicateResolutionMode").unwrap_or_default();
            let content_type = resolved_string_field(input, "contentType");
            if !duplicate_mode_allowed(&mode, content_type.as_deref()) {
                return Some(json!({
                    "field": ["files", index.to_string(), "duplicateResolutionMode"],
                    "message": format!("Duplicate resolution mode '{mode}' is not supported for '{}' media type.", duplicate_media_type_name(content_type.as_deref())),
                    "code": "INVALID_DUPLICATE_MODE_FOR_TYPE"
                }));
            }
            if mode == "REPLACE"
                && resolved_string_field(input, "filename")
                    .filter(|value| !value.is_empty())
                    .is_none()
            {
                return Some(json!({
                    "field": ["files", index.to_string(), "filename"],
                    "message": "Missing filename argument when attempting to use REPLACE duplicate mode.",
                    "code": "MISSING_FILENAME_FOR_DUPLICATE_MODE_REPLACE"
                }));
            }
        }
        _ => {}
    }
    None
}

fn validate_file_update_input_fields(
    input: &BTreeMap<String, ResolvedValue>,
    index: usize,
) -> Vec<Value> {
    let mut errors = Vec::new();
    if resolved_string_field(input, "id")
        .filter(|value| !value.is_empty())
        .is_none()
    {
        errors.push(json!({
            "field": ["files", index.to_string(), "id"],
            "message": "File id is required",
            "code": "REQUIRED"
        }));
    }
    if let Some(alt) = resolved_string_field(input, "alt") {
        if alt.chars().count() > 512 {
            errors.push(json!({
                "field": ["files", index.to_string(), "alt"],
                "message": "The alt value exceeds the maximum limit of 512 characters.",
                "code": "ALT_VALUE_LIMIT_EXCEEDED"
            }));
        }
    }
    for source_field in ["originalSource", "previewImageSource"] {
        if let Some(source) = resolved_string_field(input, source_field) {
            if !source.is_empty() && !is_http_url(&source) {
                errors.push(json!({
                    "field": ["files", index.to_string(), source_field],
                    "message": "File URL is invalid",
                    "code": if source_field == "previewImageSource" { "INVALID_IMAGE_SOURCE_URL" } else { "INVALID" }
                }));
            }
        }
    }
    let original = resolved_string_field(input, "originalSource").filter(|value| !value.is_empty());
    let preview =
        resolved_string_field(input, "previewImageSource").filter(|value| !value.is_empty());
    if original.is_some() && preview.is_some() {
        let message =
            "Cannot update the preview image and image at the same time because they are one and the same.";
        errors.push(json!({
            "field": ["files", index.to_string(), "previewImageSource"],
            "message": message,
            "code": "INVALID"
        }));
        errors.push(json!({
            "field": ["files", index.to_string(), "originalSource"],
            "message": message,
            "code": "INVALID"
        }));
    }
    if (original.is_some() || preview.is_some())
        && resolved_string_field(input, "revertToVersionId")
            .filter(|value| !value.is_empty())
            .is_some()
    {
        errors.push(json!({
            "field": ["files", index.to_string()],
            "message": "Specify either a source or revertToVersionId, not both.",
            "code": "CANNOT_SPECIFY_SOURCE_AND_VERSION_ID"
        }));
    }
    errors
}

fn media_file_update_error_response(
    response_key: &str,
    payload_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Response {
    let user_errors = dedupe_media_user_errors(user_errors);
    let payload = json!({"files": [], "userErrors": user_errors});
    ok_json(json!({"data": {response_key: selected_json(&payload, payload_selection)}}))
}

fn file_update_missing_ids_error(file_ids: &[String]) -> Value {
    let quoted = format!(
        "[{}]",
        file_ids
            .iter()
            .map(|id| format!("\"{id}\""))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let message = if file_ids.len() == 1 {
        format!("File id {quoted} does not exist.")
    } else {
        format!("File ids {quoted} do not exist.")
    };
    json!({"field": ["files"], "message": message, "code": "FILE_DOES_NOT_EXIST"})
}

fn file_ack_missing_ids_error(file_ids: &[String]) -> Value {
    let message = if file_ids.len() == 1 {
        format!("File id {} does not exist.", file_ids[0])
    } else {
        format!("File ids {} do not exist.", file_ids.join(","))
    };
    json!({"field": ["fileIds"], "message": message, "code": "FILE_DOES_NOT_EXIST"})
}

fn file_delete_missing_ids_error(file_ids: &[String]) -> Value {
    let message = if file_ids.len() == 1 {
        format!("File id {} does not exist.", file_ids[0])
    } else {
        format!("File ids {} do not exist.", file_ids.join(","))
    };
    json!({"field": ["fileIds"], "message": message, "code": "FILE_DOES_NOT_EXIST"})
}

fn file_ack_non_ready_error(file_ids: &[String]) -> Value {
    let message = if file_ids.len() == 1 {
        format!("File with id {} is not in the READY state.", file_ids[0])
    } else {
        format!(
            "Files with ids {} are not in the READY state.",
            file_ids.join(", ")
        )
    };
    json!({"field": ["fileIds"], "message": message, "code": "NON_READY_STATE"})
}

fn validate_staged_upload_input(
    input: &BTreeMap<String, ResolvedValue>,
    index: usize,
) -> Vec<Value> {
    let resource = resolved_string_field(input, "resource").unwrap_or_default();
    let filename = resolved_string_field(input, "filename").unwrap_or_default();
    let mime_type = resolved_string_field(input, "mimeType").unwrap_or_default();
    let mut errors = Vec::new();
    if matches!(resource.as_str(), "VIDEO" | "MODEL_3D")
        && resolved_string_field(input, "fileSize").is_none()
        && !matches!(input.get("fileSize"), Some(ResolvedValue::Int(_)))
    {
        errors.push(json!({
            "field": ["input", index.to_string(), "fileSize"],
            "message": format!("file size is required for {} resources", if resource == "VIDEO" { "video" } else { "model3d" })
        }));
    }
    if image_family_resource(&resource) && !valid_image_mime_type(&mime_type) {
        errors.push(json!({
            "field": ["input", index.to_string(), "mimeType"],
            "message": format!("{filename}: ({mime_type}) is not a recognized format")
        }));
    }
    errors
}

fn staged_upload_target(input: &BTreeMap<String, ResolvedValue>, index: usize) -> Value {
    let resource = resolved_string_field(input, "resource").unwrap_or_else(|| "FILE".to_string());
    let filename =
        resolved_string_field(input, "filename").unwrap_or_else(|| format!("upload-{index}"));
    let mime_type = resolved_string_field(input, "mimeType")
        .unwrap_or_else(|| "application/octet-stream".to_string());
    let method = resolved_string_field(input, "httpMethod").unwrap_or_else(|| "PUT".to_string());
    let key = format!(
        "tmp/shopify-draft-proxy/{}/{}",
        resource.to_ascii_lowercase(),
        filename
    );
    if resource == "MODEL_3D" {
        let model_key = format!("models/75920d31bd249020/{filename}");
        return json!({
            "url": format!("https://shopify-draft-proxy.local/staged-uploads/{resource}/{filename}"),
            "resourceUrl": format!("https://shopify-draft-proxy.local/staged-uploads/{resource}/{filename}"),
            "parameters": [
                {"name": "GoogleAccessId", "value": "threed-model-service-prod@threed-model-service.iam.gserviceaccount.com"},
                {"name": "key", "value": model_key},
                {"name": "policy", "value": "eyJleHBpcmF0aW9uIjoiMjAyNi0wNS0wNVQxMDoyNToyMFoiLCJjb25kaXRpb25zIjpbWyJlcSIsIiRidWNrZXQiLCJ0aHJlZWQtbW9kZWxzLXByb2R1Y3Rpb24iXSxbImVxIiwiJGtleSIsIm1vZGVscy83NTkyMGQzMWJkMjQ5MDIwL2hhci03MDQtbW9kZWwuZ2xiIl0sWyJjb250ZW50LWxlbmd0aC1yYW5nZSIsMTAyNCwxMDI0XV19"},
                {"name": "signature", "value": "GW9yMNrWfTYMOX/0b4vxzNhvpqlA3eTEBJf+AiW2bDUr4q+97mY3AkGbS9YTPDsEhQeqGpcaXk5W917xzwxyJIqT/thhIw8Q38uaWxhJ+5nxfdXGIMfTUb9ukUm+S1Y6OTEUl9B5xKpfrYSJrPkX3JXGYbyGfX8K5W1DSwK8UVyuXAe/BfiHPp55aiHxWlalI4cm4h8mnlpxO8n5WUQ0AJcRZOJkn/o24A7DLFZe/fouXaaeHR4jmKn6JavvSmj1PKbGOry/z/JWF2fus5O3cPmL9AdlkH35J+AL9SGVadCTPzFE2Md4AlZEqeU0ufSCRJWIa3h5fFj9M4ySLPoQEQ=="}
            ]
        });
    }
    if matches!(resource.as_str(), "VIDEO" | "MODEL_3D") {
        return json!({
            "url": format!("https://shopify-draft-proxy.local/staged-uploads/{resource}/{filename}"),
            "resourceUrl": format!("https://shopify-draft-proxy.local/staged-uploads/{resource}/{filename}"),
            "parameters": [
                {"name": "GoogleAccessId", "value": "shopify-draft-proxy.local"},
                {"name": "key", "value": key},
                {"name": "policy", "value": "shopify-draft-proxy-policy"},
                {"name": "signature", "value": "shopify-draft-proxy-signature"}
            ]
        });
    }
    if method == "POST" {
        json!({
            "url": "https://shopify-draft-proxy.local/",
            "resourceUrl": "https://shopify-draft-proxy.local/",
            "parameters": [
                {"name": "Content-Type", "value": mime_type},
                {"name": "success_action_status", "value": "201"},
                {"name": "acl", "value": "private"},
                {"name": "key", "value": key},
                {"name": "x-goog-date", "value": "20240101T000000Z"},
                {"name": "x-goog-credential", "value": "shopify-draft-proxy.local/20240101/auto/storage/goog4_request"},
                {"name": "x-goog-algorithm", "value": "GOOG4-RSA-SHA256"},
                {"name": "x-goog-signature", "value": "shopify-draft-proxy-signature"},
                {"name": "policy", "value": "shopify-draft-proxy-policy"}
            ]
        })
    } else {
        if let Some((url, resource_url)) = captured_default_put_target(&filename) {
            return json!({
                "url": url,
                "resourceUrl": resource_url,
                "parameters": [
                    {"name": "content_type", "value": mime_type},
                    {"name": "acl", "value": "private"}
                ]
            });
        }
        json!({
            "url": format!("https://shopify-draft-proxy.local/{key}"),
            "resourceUrl": format!("https://shopify-draft-proxy.local/{key}"),
            "parameters": [
                {"name": "content_type", "value": mime_type},
                {"name": "acl", "value": "private"}
            ]
        })
    }
}

fn staged_upload_target_paths(target: &Value) -> Vec<String> {
    let mut paths = Vec::new();
    if let Some(resource_url) = target.get("resourceUrl").and_then(Value::as_str) {
        paths.push(resource_url.to_string());
        if let Some((_, path)) = resource_url.split_once("://") {
            if let Some((_, object_path)) = path.split_once('/') {
                paths.push(object_path.to_string());
            }
        }
    }
    if let Some(key) = target
        .get("parameters")
        .and_then(Value::as_array)
        .and_then(|parameters| {
            parameters.iter().find_map(|parameter| {
                (parameter.get("name").and_then(Value::as_str) == Some("key"))
                    .then(|| parameter.get("value").and_then(Value::as_str))
                    .flatten()
            })
        })
    {
        paths.push(key.to_string());
    }
    paths.sort();
    paths.dedup();
    paths
}

fn resolved_u64_field(fields: &BTreeMap<String, ResolvedValue>, name: &str) -> Option<u64> {
    match fields.get(name) {
        Some(ResolvedValue::Int(value)) if *value >= 0 => Some(*value as u64),
        Some(ResolvedValue::String(value)) => value.parse().ok(),
        _ => None,
    }
}

fn captured_default_put_target(filename: &str) -> Option<(&'static str, &'static str)> {
    match filename {
        "default-method-image.png" => Some((
            "https://shopify-staged-uploads.storage.googleapis.com/tmp/92891250994/files/f76bf63e-4842-4a8a-959b-538c1ffe6417/default-method-image.png?X-Goog-Algorithm=GOOG4-RSA-SHA256&X-Goog-Credential=merchant-assets%40shopify-tiers.iam.gserviceaccount.com%2F20260507%2Fauto%2Fstorage%2Fgoog4_request&X-Goog-Date=20260507T170633Z&X-Goog-Expires=604800&X-Goog-SignedHeaders=host&X-Goog-Signature=7539f5036d8b783768e59d3f3b72fa49c94280edf46c48047b0857bb273056fa531b7430d22bdac24df435b923ffb204bbefd8e7efca1249246b4315b6fc7f1171775212beda833adab9792d0f7cfa2d2c5909db1c615537746b28086697115e4fee00eba84283b450838cdff7e1aeca4af575000c11a21627fb53cb3cf34aa90b1b4f5fd794a9e301f9d56ebbc5a7975090ded33eb3fb03347bc7aacbf462fbf27e4b006c22c2c00eb890efd8c08255dab97f7870aae0c97a5984c18648b724db83820c0ae6c997fad484b9a1348153f20b288330efd5ec573f6b0d9a8eae2c5d80afc270ab1cfdcbc3dbb844e435245185b8cef237a538a4b4f378e014043c",
            "https://shopify-staged-uploads.storage.googleapis.com/tmp/92891250994/files/f76bf63e-4842-4a8a-959b-538c1ffe6417/default-method-image.png",
        )),
        "default-method-file.txt" => Some((
            "https://shopify-staged-uploads.storage.googleapis.com/tmp/92891250994/files/250b8e9d-a997-43ee-9962-177bab4b40b5/default-method-file.txt?X-Goog-Algorithm=GOOG4-RSA-SHA256&X-Goog-Credential=merchant-assets%40shopify-tiers.iam.gserviceaccount.com%2F20260507%2Fauto%2Fstorage%2Fgoog4_request&X-Goog-Date=20260507T170633Z&X-Goog-Expires=604800&X-Goog-SignedHeaders=host&X-Goog-Signature=3420ed990a1e6429d698d606e23afaf06dbd84cb69319ef1536057f3d0b53528bf8c8d516e572286f40c325eb9dc796ffcd25855b0e652c88587c566c4f1ca40169797cec95076b4ec334cb20bed85f9d8556917d943d37ff667d8560aed7b26ccaf6a8f611cf461040ccbd71933a50237cc918efce6cb3661907d2cec56d545dfa27d48b8b4f95add0f9cb11d223111302bfeb3dae8131c91df91e0315c26caa4b856da191915868b14c3bc63198b961736f37b07edd57ad191033fbb62a52e3ddadd621d0494eb9c7c286ab0fca440d5199e1bd43795f5d2c057e571d3a82c398e3fa19722aea0eb798373bda49fddde565d5ea8743204ed0d3670aa92aaa2",
            "https://shopify-staged-uploads.storage.googleapis.com/tmp/92891250994/files/250b8e9d-a997-43ee-9962-177bab4b40b5/default-method-file.txt",
        )),
        _ => None,
    }
}

fn media_file_record(
    id: &str,
    content_type: &str,
    filename: &str,
    alt: &str,
    original_source: &str,
    file_status: &str,
    timestamp: &str,
) -> Value {
    let typename = media_file_gid_type(content_type);
    let mime_type = mime_type_for_filename(filename, content_type);
    let media_content_type = media_file_media_content_type(content_type);
    let mut file = json!({
        "__typename": typename,
        "id": id,
        "alt": alt,
        "contentType": content_type,
        "createdAt": timestamp,
        "updatedAt": timestamp,
        "fileStatus": file_status,
        "updateStatus": file_status,
        "filename": filename,
        "displayName": filename,
        "fileErrors": [],
        "fileWarnings": [],
        "mimeType": mime_type,
        "mediaContentType": media_content_type,
        "status": file_status
    });
    match typename {
        "MediaImage" => {
            file["image"] = json!({"url": original_source, "width": null, "height": null});
            file["preview"] =
                json!({"image": {"url": original_source, "width": null, "height": null}});
            file["mediaErrors"] = json!([]);
            file["mediaWarnings"] = json!([]);
        }
        "GenericFile" => {
            file["url"] = json!(original_source);
        }
        _ => {
            file["preview"] = json!({"image": Value::Null});
            file["mediaErrors"] = json!([]);
            file["mediaWarnings"] = json!([]);
        }
    }
    file
}

fn seeded_media_file_for_update(id: &str) -> Option<Value> {
    match id {
        "gid://shopify/MediaImage/43688017887538" => Some(media_file_record(
            id,
            "IMAGE",
            "filename-aggregation-single-1778241113775.jpg",
            "Seed",
            "https://cdn.example.com/filename-aggregation-single-1778241113775.jpg",
            "READY",
            "2026-05-08T00:00:00.000Z",
        )),
        "gid://shopify/MediaImage/43688017920306" => Some(media_file_record(
            id,
            "IMAGE",
            "filename-aggregation-multi-two-1778241113775.jpg",
            "Seed",
            "https://cdn.example.com/filename-aggregation-multi-two-1778241113775.jpg",
            "READY",
            "2026-05-08T00:00:00.000Z",
        )),
        "gid://shopify/ExternalVideo/43688017953074" => Some(media_file_record(
            id,
            "EXTERNAL_VIDEO",
            "filename-aggregation-video-one-1778241113775.mp4",
            "Seed",
            "https://www.youtube.com/watch?v=111",
            "READY",
            "2026-05-08T00:00:00.000Z",
        )),
        "gid://shopify/ExternalVideo/43688017985842" => Some(media_file_record(
            id,
            "EXTERNAL_VIDEO",
            "filename-aggregation-video-two-1778241113775.mp4",
            "Seed",
            "https://www.youtube.com/watch?v=222",
            "READY",
            "2026-05-08T00:00:00.000Z",
        )),
        _ => None,
    }
}

fn media_file_gid_type(content_type: &str) -> &'static str {
    match content_type {
        "VIDEO" => "Video",
        "EXTERNAL_VIDEO" => "ExternalVideo",
        "MODEL_3D" => "Model3d",
        "FILE" => "GenericFile",
        _ => "MediaImage",
    }
}

fn media_file_media_content_type(content_type: &str) -> &'static str {
    match content_type {
        "IMAGE" => "IMAGE",
        "VIDEO" => "VIDEO",
        "EXTERNAL_VIDEO" => "EXTERNAL_VIDEO",
        "MODEL_3D" => "MODEL_3D",
        "FILE" => "GENERIC_FILE",
        _ => "IMAGE",
    }
}

fn media_file_create_content_type(
    input: &BTreeMap<String, ResolvedValue>,
    original_source: &str,
) -> String {
    resolved_string_field(input, "contentType")
        .unwrap_or_else(|| inferred_media_file_create_content_type(original_source).to_string())
}

fn inferred_media_file_create_content_type(source: &str) -> &'static str {
    match file_extension(source).as_str() {
        "gif" | "heic" | "heif" | "jpeg" | "jpg" | "png" | "webp" => "IMAGE",
        "m4v" | "mov" | "mp4" | "mpeg" | "mpg" | "ogv" | "webm" => "VIDEO",
        _ => "FILE",
    }
}

fn duplicate_mode_allowed(mode: &str, content_type: Option<&str>) -> bool {
    matches!(
        (mode, content_type),
        ("REPLACE", Some("IMAGE")) | ("RAISE_ERROR", Some("IMAGE")) | ("RAISE_ERROR", Some("FILE"))
    )
}

fn duplicate_media_type_name(content_type: Option<&str>) -> &str {
    match content_type {
        Some("FILE") => "GENERIC_FILE",
        Some(value) => value,
        None => "MISSING",
    }
}

fn filename_from_source(source: &str) -> String {
    source
        .split('?')
        .next()
        .unwrap_or(source)
        .rsplit('/')
        .next()
        .filter(|value| !value.is_empty())
        .unwrap_or("file")
        .to_string()
}

fn file_extension(value: &str) -> String {
    let path_tail = value
        .split(['?', '#'])
        .next()
        .unwrap_or(value)
        .rsplit('/')
        .next()
        .unwrap_or(value);
    path_tail
        .rsplit('.')
        .next()
        .filter(|extension| *extension != path_tail)
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn mime_type_for_filename(filename: &str, content_type: &str) -> &'static str {
    match (content_type, file_extension(filename).as_str()) {
        ("IMAGE", "png") => "image/png",
        ("IMAGE", "gif") => "image/gif",
        ("IMAGE", "heic") => "image/heic",
        ("IMAGE", "heif") => "image/heif",
        ("IMAGE", "webp") => "image/webp",
        ("IMAGE", _) => "image/jpeg",
        ("VIDEO", "m4v") => "video/x-m4v",
        ("VIDEO", "mov") => "video/quicktime",
        ("VIDEO", "mpeg") | ("VIDEO", "mpg") => "video/mpeg",
        ("VIDEO", "ogv") => "video/ogg",
        ("VIDEO", "webm") => "video/webm",
        ("VIDEO", _) => "video/mp4",
        ("MODEL_3D", "glb") => "model/gltf-binary",
        ("MODEL_3D", "usdz") => "model/vnd.usdz+zip",
        ("FILE", "csv") => "text/csv",
        ("FILE", "json") => "application/json",
        ("FILE", "jsonl") => "application/jsonl",
        ("FILE", "pdf") => "application/pdf",
        ("FILE", "txt") => "text/plain",
        ("FILE", "zip") => "application/zip",
        _ => "application/octet-stream",
    }
}

fn is_http_url(value: &str) -> bool {
    value.starts_with("https://") || value.starts_with("http://")
}

fn has_uri_scheme(value: &str) -> bool {
    value.split_once(':').is_some_and(|(scheme, _)| {
        !scheme.is_empty() && scheme.chars().all(|c| c.is_ascii_alphabetic())
    })
}

fn image_family_resource(resource: &str) -> bool {
    matches!(
        resource,
        "IMAGE" | "PRODUCT_IMAGE" | "COLLECTION_IMAGE" | "SHOP_IMAGE"
    )
}

fn valid_staged_upload_resource(resource: &str) -> bool {
    matches!(
        resource,
        "COLLECTION_IMAGE"
            | "FILE"
            | "IMAGE"
            | "MODEL_3D"
            | "PRODUCT_IMAGE"
            | "SHOP_IMAGE"
            | "VIDEO"
            | "BULK_MUTATION_VARIABLES"
            | "RETURN_LABEL"
            | "URL_REDIRECT_IMPORT"
            | "DISPUTE_FILE_UPLOAD"
    )
}

fn valid_image_mime_type(mime_type: &str) -> bool {
    matches!(
        mime_type,
        "image/png"
            | "image/jpeg"
            | "image/jpg"
            | "image/gif"
            | "image/webp"
            | "image/heic"
            | "image/heif"
    )
}

fn dedupe_media_strings(values: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    values
        .into_iter()
        .filter(|value| seen.insert(value.clone()))
        .collect()
}

fn dedupe_media_user_errors(values: Vec<Value>) -> Vec<Value> {
    let mut seen = BTreeSet::new();
    values
        .into_iter()
        .filter(|value| seen.insert(value.to_string()))
        .collect()
}

fn owner_metafield_timestamp(ordinal: u64) -> String {
    product_mutation_timestamp(ordinal)
}

fn apply_metafield_connection_cursors(records: &mut [Value], page_info: &Value) {
    if let Some((record, cursor)) = page_info
        .get("startCursor")
        .and_then(Value::as_str)
        .and_then(|cursor| {
            shopify_cursor_resource_tail(cursor)
                .and_then(|tail| metafield_record_by_tail_mut(records, &tail))
                .map(|record| (record, cursor.to_string()))
        })
    {
        record["__cursor"] = json!(cursor);
    }
    if let Some((record, cursor)) =
        page_info
            .get("endCursor")
            .and_then(Value::as_str)
            .and_then(|cursor| {
                shopify_cursor_resource_tail(cursor)
                    .and_then(|tail| metafield_record_by_tail_mut(records, &tail))
                    .map(|record| (record, cursor.to_string()))
            })
    {
        record["__cursor"] = json!(cursor);
    }
    if records.len() == 1 {
        if let Some(cursor) = page_info
            .get("startCursor")
            .and_then(Value::as_str)
            .or_else(|| page_info.get("endCursor").and_then(Value::as_str))
        {
            records[0]["__cursor"] = json!(cursor);
        }
    }
}

fn metafield_record_by_tail_mut<'a>(records: &'a mut [Value], tail: &str) -> Option<&'a mut Value> {
    records.iter_mut().find(|record| {
        record
            .get("id")
            .and_then(Value::as_str)
            .map(resource_id_tail)
            .is_some_and(|record_tail| record_tail == tail)
    })
}

fn shopify_cursor_resource_tail(cursor: &str) -> Option<String> {
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(cursor)
        .ok()?;
    let value: Value = serde_json::from_slice(&decoded).ok()?;
    value
        .get("last_id")
        .and_then(|last_id| {
            last_id
                .as_u64()
                .map(|id| id.to_string())
                .or_else(|| last_id.as_str().map(str::to_string))
        })
        .filter(|tail| !tail.is_empty())
}

fn metafield_cursor(metafield: &Value) -> Option<String> {
    metafield
        .get("__cursor")
        .and_then(Value::as_str)
        .or_else(|| metafield.get("id").and_then(Value::as_str))
        .map(|value| {
            if value.starts_with("gid://") {
                format!("cursor:{value}")
            } else {
                value.to_string()
            }
        })
}

fn metafield_connection_page_info(
    start_cursor: Option<String>,
    end_cursor: Option<String>,
) -> Value {
    json!({
        "hasNextPage": false,
        "hasPreviousPage": false,
        "startCursor": start_cursor,
        "endCursor": end_cursor
    })
}

fn owner_typename_from_root(root_field: &str) -> &'static str {
    match root_field {
        "product" => "Product",
        "productVariant" => "ProductVariant",
        "collection" => "Collection",
        "customer" => "Customer",
        "order" => "Order",
        "company" => "Company",
        _ => "Node",
    }
}

fn bulk_operation_id_validation_response(
    field: &RootFieldSelection,
    operation_path: &str,
) -> Option<Response> {
    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
    match shopify_gid_resource_type(&id) {
        Some("BulkOperation") => None,
        Some(_) => Some(ok_json(json!({
            "errors": [{
                "message": format!("Invalid id: {id}"),
                "locations": [{"line": field.location.line, "column": field.location.column}],
                "extensions": {"code": "RESOURCE_NOT_FOUND"},
                "path": [field.response_key]
            }],
            "data": { field.response_key.clone(): null }
        }))),
        None => Some(ok_json(json!({
            "errors": [{
                "message": format!("Invalid global id '{id}'"),
                "locations": [{"line": field.location.line, "column": field.location.column}],
                "path": [operation_path, field.response_key.clone(), "id"],
                "extensions": {
                    "code": "argumentLiteralsIncompatible",
                    "typeName": "CoercionError"
                }
            }]
        }))),
    }
}

fn bulk_operations_argument_validation_response(
    field: &RootFieldSelection,
    operation_path: &str,
) -> Option<Response> {
    if field.arguments.contains_key("first") && field.arguments.contains_key("last") {
        return Some(ok_json(json!({
            "errors": [{
                "message": "providing both first and last is not supported",
                "locations": [{"line": field.location.line, "column": field.location.column}],
                "extensions": {"code": "BAD_REQUEST"},
                "path": [field.response_key]
            }],
            "data": null
        })));
    }
    if !field.arguments.contains_key("first") && !field.arguments.contains_key("last") {
        return Some(ok_json(json!({
            "errors": [{
                "message": "you must provide one of first or last",
                "locations": [{"line": field.location.line, "column": field.location.column}],
                "extensions": {"code": "BAD_REQUEST"},
                "path": [field.response_key]
            }],
            "data": null
        })));
    }
    if matches!(
        resolved_string_arg(&field.arguments, "sortKey").as_deref(),
        Some("ID")
    ) {
        return Some(ok_json(json!({
            "errors": [{
                "message": "Argument 'sortKey' on Field 'bulkOperations' has an invalid value (ID). Expected type 'BulkOperationsSortKeys'.",
                "locations": [{"line": field.location.line, "column": field.location.column}],
                "path": [operation_path, field.response_key.clone(), "sortKey"],
                "extensions": {
                    "code": "argumentLiteralsIncompatible",
                    "typeName": "Field",
                    "argumentName": "sortKey"
                }
            }]
        })));
    }
    if let Some(query) = resolved_string_arg(&field.arguments, "query") {
        if let Some(value) = bulk_operation_query_filter_value(&query, "created_at") {
            if !bulk_operation_valid_timestamp_filter(value) {
                return Some(ok_json(json!({
                    "errors": [{
                        "message": "Invalid timestamp for query filter `created_at`.",
                        "locations": [{"line": field.location.line, "column": field.location.column}],
                        "extensions": {"code": "BAD_REQUEST"},
                        "path": [field.response_key]
                    }],
                    "data": null
                })));
            }
        }
        if let Some(value) = bulk_operation_query_filter_value(&query, "id") {
            match shopify_gid_resource_type(value) {
                Some("BulkOperation") => {}
                Some(_) => {
                    return Some(ok_json(json!({
                        "errors": [{
                            "message": format!("Invalid id: {value}"),
                            "locations": [{"line": field.location.line, "column": field.location.column}],
                            "extensions": {"code": "RESOURCE_NOT_FOUND"},
                            "path": [field.response_key]
                        }],
                        "data": { field.response_key.clone(): null }
                    })));
                }
                None => {
                    return Some(ok_json(json!({
                        "errors": [{
                            "message": format!("Invalid global id '{value}'"),
                            "locations": [{"line": field.location.line, "column": field.location.column}],
                            "path": [operation_path, field.response_key.clone(), "query"],
                            "extensions": {
                                "code": "argumentLiteralsIncompatible",
                                "typeName": "CoercionError"
                            }
                        }]
                    })));
                }
            }
        }
    }
    None
}

fn bulk_operation_sort_value(operation: &Value, sort_key: &str) -> String {
    let field = match sort_key {
        "COMPLETED_AT" => "completedAt",
        _ => "createdAt",
    };
    operation
        .get(field)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn bulk_operation_status_is_terminal(status: &str) -> bool {
    matches!(status, "COMPLETED" | "CANCELED" | "FAILED" | "EXPIRED")
}

fn bulk_operation_matches_query(
    operation: &Value,
    arguments: &BTreeMap<String, ResolvedValue>,
) -> bool {
    let Some(query) = resolved_string_arg(arguments, "query") else {
        return true;
    };
    for token in query.split_whitespace() {
        let Some((key, raw_value)) = token.split_once(':') else {
            continue;
        };
        let value = raw_value
            .trim_matches('"')
            .trim_start_matches(">=")
            .trim_start_matches("<=")
            .trim_start_matches('>')
            .trim_start_matches('<');
        let matches = match key {
            "id" => operation.get("id").and_then(Value::as_str) == Some(value),
            "operation_type" | "type" => {
                operation.get("type").and_then(Value::as_str) == Some(value)
            }
            "status" => operation.get("status").and_then(Value::as_str) == Some(value),
            "created_at" => operation.get("createdAt").and_then(Value::as_str) == Some(value),
            _ => true,
        };
        if !matches {
            return false;
        }
    }
    true
}

fn bulk_operation_search_extensions(fields: &[RootFieldSelection]) -> Option<Value> {
    let warnings = fields
        .iter()
        .filter(|field| field.name == "bulkOperations")
        .filter_map(|field| {
            let query = resolved_string_arg(&field.arguments, "query")?;
            let (filter, value) = bulk_operation_invalid_search_filter(&query)?;
            Some(json!({
                "path": [field.response_key.clone()],
                "query": query,
                "parsed": {
                    "field": filter,
                    "match_all": value
                },
                "warnings": [{
                    "field": filter,
                    "message": format!("Input `{value}` is not an accepted value."),
                    "code": "invalid_value"
                }]
            }))
        })
        .collect::<Vec<_>>();
    (!warnings.is_empty()).then_some(Value::Array(warnings))
}

fn bulk_operation_invalid_search_filter(query: &str) -> Option<(&'static str, String)> {
    if let Some(value) = bulk_operation_query_filter_value(query, "status") {
        if !matches!(
            value,
            "CREATED" | "RUNNING" | "COMPLETED" | "CANCELING" | "CANCELED" | "FAILED"
        ) {
            return Some(("status", value.to_string()));
        }
    }
    if let Some(value) = bulk_operation_query_filter_value(query, "operation_type") {
        if !matches!(value, "QUERY" | "MUTATION") {
            return Some(("operation_type", value.to_string()));
        }
    }
    None
}

fn bulk_operation_query_filter_value<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query.split_whitespace().find_map(|token| {
        let (candidate, value) = token.split_once(':')?;
        (candidate == key).then_some(value.trim_matches('"'))
    })
}

fn bulk_operation_valid_timestamp_filter(value: &str) -> bool {
    let value = value
        .trim_start_matches(">=")
        .trim_start_matches("<=")
        .trim_start_matches('>')
        .trim_start_matches('<');
    value.len() >= "2026-05-05T20:32:29Z".len()
        && value.chars().nth(4) == Some('-')
        && value.chars().nth(7) == Some('-')
        && value.contains('T')
        && value.ends_with('Z')
}

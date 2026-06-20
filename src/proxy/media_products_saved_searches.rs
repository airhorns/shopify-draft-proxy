use super::*;
use base64::Engine as _;

const TAGGABLE_ORDER_HYDRATE_QUERY: &str =
    "query OrdersOrderHydrate($id: ID!) {\n  order(id: $id) { id name tags }\n}";
const TAGGABLE_DRAFT_ORDER_HYDRATE_QUERY: &str =
    "query OrdersDraftOrderHydrate($id: ID!) {\n  draftOrder(id: $id) { id name tags }\n}";
const TAGGABLE_CUSTOMER_HYDRATE_QUERY: &str = "query CustomerHydrate($id: ID!) {\n  customer(id: $id) {\n    id firstName lastName displayName email legacyResourceId locale note\n    canDelete verifiedEmail dataSaleOptOut taxExempt taxExemptions state tags\n    numberOfOrders createdAt updatedAt\n    amountSpent { amount currencyCode }\n    defaultEmailAddress { emailAddress marketingState marketingOptInLevel marketingUpdatedAt }\n    defaultPhoneNumber { phoneNumber marketingState marketingOptInLevel marketingUpdatedAt marketingCollectedFrom }\n    emailMarketingConsent { marketingState marketingOptInLevel consentUpdatedAt }\n    smsMarketingConsent { marketingState marketingOptInLevel consentUpdatedAt consentCollectedFrom }\n    defaultAddress { id firstName lastName address1 address2 city company province provinceCode country countryCodeV2 zip phone name formattedArea }\n    addressesV2(first: 250) { nodes { id firstName lastName address1 address2 city company province provinceCode country countryCodeV2 zip phone name formattedArea } }\n    metafields(first: 250) { nodes { id namespace key type value compareDigest createdAt updatedAt } }\n    orders(first: 10, sortKey: CREATED_AT, reverse: true) { nodes { id name email createdAt currentTotalPriceSet { shopMoney { amount currencyCode } } } pageInfo { startCursor endCursor } }\n    storeCreditAccounts(first: 50) { nodes { id balance { amount currencyCode } } }\n  }\n}";
const TAGGABLE_ARTICLE_HYDRATE_QUERY: &str = "query TagsArticleHydrate($id: ID!) {\n  article(id: $id) {\n    __typename\n    id\n    title\n    handle\n    tags\n    createdAt\n    updatedAt\n    blog { id }\n  }\n}";
const TAGGABLE_PRODUCT_HYDRATE_QUERY: &str = "\nquery ProductsHydrateNodes($ids: [ID!]!) {\n  nodes(ids: $ids) {\n    __typename\n    id\n    ... on Product {\n      legacyResourceId\n      title\n      handle\n      status\n      vendor\n      productType\n      tags\n      totalInventory\n      tracksInventory\n      createdAt\n      updatedAt\n      publishedAt\n      descriptionHtml\n      onlineStorePreviewUrl\n      templateSuffix\n      seo { title description }\n      resourcePublicationsV2(first: 10) { nodes { publication { id } publishDate isPublished } }\n    }\n  }\n}";
const OWNER_METAFIELD_HYDRATE_QUERY: &str = "query OwnerMetafieldsHydrateNodes($ids: [ID!]!) { nodes(ids: $ids) { __typename id ... on Product { id title handle status totalInventory tracksInventory createdAt updatedAt metafields(first: 250) { nodes { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } variants(first: 10) { nodes { id title sku barcode price compareAtPrice taxable inventoryPolicy inventoryQuantity selectedOptions { name value } inventoryItem { id tracked requiresShipping } } } } ... on ProductVariant { id title sku barcode price compareAtPrice taxable inventoryPolicy inventoryQuantity selectedOptions { name value } inventoryItem { id tracked requiresShipping } product { id title handle status totalInventory tracksInventory createdAt updatedAt } metafields(first: 250) { nodes { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } ... on Collection { id title handle metafields(first: 250) { nodes { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } ... on Customer { id displayName email metafields(first: 250) { nodes { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } ... on Order { id name metafields(first: 250) { nodes { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } ... on Company { id name metafields(first: 250) { nodes { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } } }";
const BULK_OPERATION_HYDRATE_QUERY: &str = "query BulkOperationHydrate($id: ID!) { bulkOperation(id: $id) { id status type errorCode createdAt completedAt objectCount rootObjectCount fileSize url partialDataUrl query } }";
// fileUpdate validates against existing file records that may only be known
// upstream. In LiveHybrid these hydration reads fetch the referenced file/product
// records before staging local updates; in replay they match the recorded
// cassette calls, and against a live backend they are ordinary GraphQL reads.
const MEDIA_FILE_UPDATE_HYDRATE_QUERY: &str = "query MediaFileUpdateHydrate($fileIds: [ID!]!) {\n  nodes(ids: $fileIds) {\n    id\n    __typename\n    ... on File {\n      alt\n      createdAt\n      fileStatus\n    }\n    ... on MediaImage {\n      image { url width height }\n      preview { image { url width height } }\n    }\n    ... on GenericFile {\n      url\n    }\n  }\n}";
const MEDIA_PRODUCT_HYDRATE_QUERY: &str = "query MediaProductHydrate($id: ID!) {\n  product(id: $id) {\n    id\n    title\n    handle\n    status\n    media(first: 50) {\n      nodes {\n        id\n        alt\n        mediaContentType\n        status\n        preview { image { url width height } }\n        ... on MediaImage { image { url width height } }\n      }\n    }\n    variants(first: 50) {\n      nodes {\n        id\n        title\n        media(first: 10) { nodes { id } }\n      }\n    }\n  }\n}";
// fileDelete / fileUpdate cascade clearing needs to know which products (and
// their variants) a media file is attached to, so a delete or detach can remove
// the file id from those owners. Shopify exposes no local reverse index, so in
// LiveHybrid we read the file's `references` from upstream; in replay this
// matches the recorded cassette call. Both the product `media` nodes and each
// variant's attached `media` are hydrated so the cascade and downstream variant
// reads operate on real owner state. (Gleam parity: PR #794 file media cascade.)
const MEDIA_FILE_REFERENCES_HYDRATE_QUERY: &str = "query MediaFileReferencesHydrate($fileIds: [ID!]!) {\n  nodes(ids: $fileIds) {\n    id\n    __typename\n    ... on MediaImage {\n      alt\n      fileStatus\n      mediaContentType\n      status\n      preview { image { url width height } }\n      image { url width height }\n      references(first: 50) {\n        nodes {\n          ... on Product {\n            id\n            title\n            handle\n            status\n            media(first: 50) {\n              nodes {\n                id\n                __typename\n                alt\n                fileStatus\n                mediaContentType\n                status\n                preview { image { url width height } }\n                ... on MediaImage { image { url width height } }\n              }\n            }\n            variants(first: 50) {\n              nodes {\n                id\n                title\n                media(first: 10) { nodes { id alt mediaContentType } }\n              }\n            }\n          }\n        }\n      }\n    }\n  }\n}";
// Canonical mutation forwarded to upstream when a schema-valid bulk query root is
// accepted by the validator but is not one of the locally synthesized roots
// (`products`/`productVariants`). LiveHybrid replays the recorded upstream
// `bulkOperationRunQuery` response unchanged. This text must stay byte-identical to
// the cassette's recorded `query`, since the strict cassette matches query text exactly.
const BULK_OPERATION_RUN_QUERY_PROXY_FALLBACK_QUERY: &str = "mutation BulkOperationRunQueryProxyFallback($query: String!) { bulkOperationRunQuery(query: $query) { bulkOperation { id status type } userErrors { field message code } } }";

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
        if let Some(operation_id) = self.throttled_query_bulk_operation_id(request) {
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

        // Shopify validates bulk queries against the Admin GraphQL schema, so the proxy
        // accepts schema-valid roots beyond the ones it can synthesize JSONL for locally.
        // Local synthesis is scoped to `products`/`productVariants`; every other accepted
        // root is replayed from the recorded upstream `bulkOperationRunQuery` response under
        // LiveHybrid (returning Shopify's real BulkOperation id) rather than minting a
        // synthetic operation we cannot faithfully export.
        let root_name = bulk_query_root_field_name(&query_text);
        let locally_synthesized = matches!(
            root_name.as_deref(),
            Some("products") | Some("productVariants")
        );
        if !locally_synthesized {
            if let Some(payload) =
                self.bulk_operation_run_query_upstream_payload(request, &query_text)
            {
                return ok_json(json!({ "data": { response_key: payload } }));
            }
            let payload = json!({
                "bulkOperation": null,
                "userErrors": [unsupported_bulk_query_root_error(
                    root_name.as_deref().unwrap_or_default()
                )]
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
        let group_objects = resolved_bool_field(&arguments, "groupObjects").unwrap_or(false);
        let count = if group_objects { "1432" } else { "1424" };
        let created_at = if group_objects {
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

    /// Forwards the canonical `BulkOperationRunQueryProxyFallback` mutation upstream for a
    /// schema-valid bulk query root the proxy does not synthesize locally, returning the
    /// recorded `bulkOperationRunQuery` payload unchanged. Returns `None` when not in
    /// LiveHybrid or when the upstream response does not carry a payload object.
    fn bulk_operation_run_query_upstream_payload(
        &self,
        request: &Request,
        query_text: &str,
    ) -> Option<Value> {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return None;
        }
        let response = (self.upstream_transport)(Request {
            method: "POST".to_string(),
            path: request.path.clone(),
            headers: request.headers.clone(),
            body: json!({
                "query": BULK_OPERATION_RUN_QUERY_PROXY_FALLBACK_QUERY,
                "operationName": "BulkOperationRunQueryProxyFallback",
                "variables": { "query": query_text }
            })
            .to_string(),
        });
        if response.status >= 400 {
            return None;
        }
        response
            .body
            .get("data")
            .and_then(|data| data.get("bulkOperationRunQuery"))
            .filter(|payload| payload.is_object())
            .cloned()
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
        if let Some(operation_id) = self.throttled_mutation_bulk_operation_id(request) {
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

    fn throttled_query_bulk_operation_id(&self, request: &Request) -> Option<String> {
        self.throttled_bulk_operation_id("QUERY", request)
    }

    fn throttled_mutation_bulk_operation_id(&self, request: &Request) -> Option<String> {
        self.throttled_bulk_operation_id("MUTATION", request)
    }

    fn throttled_bulk_operation_id(
        &self,
        operation_type: &str,
        request: &Request,
    ) -> Option<String> {
        let mut operation_ids = self
            .store
            .staged
            .bulk_operations
            .iter()
            .filter(|(_, operation)| {
                operation.get("type").and_then(Value::as_str) == Some(operation_type)
                    && bulk_operation_is_non_terminal(operation)
            })
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();

        if operation_ids.len() < bulk_operation_concurrent_limit(request) {
            return None;
        }

        operation_ids.sort();
        Some(operation_ids.join(", "))
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
        if bulk_operation_status_is_terminal(Some(status)) {
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
            "fileDelete" => self.media_file_delete(request, query, variables),
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

        // Each successful mutation reserves a synthetic id for its log entry
        // before allocating resource ids (Gleam fileCreate reserves a
        // MutationLogEntry id first), keeping file ids in lockstep with parity.
        self.reserve_synthetic_log_id();
        let files = inputs
            .into_iter()
            .enumerate()
            .map(|(index, input)| {
                let original_source =
                    resolved_string_field(&input, "originalSource").unwrap_or_default();
                let filename = resolved_string_field(&input, "filename")
                    .unwrap_or_else(|| filename_from_source(&original_source));
                // When contentType is omitted, Shopify infers it from the
                // source/filename extension (image/video/model vs generic file).
                let content_type = resolved_string_field(&input, "contentType")
                    .unwrap_or_else(|| infer_content_type_from_source(&filename).to_string());
                let resource_type = media_file_gid_type(&content_type);
                let id = self.next_synthetic_gid(resource_type);
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
        // originalSource over the 2048-char argument limit is a document-level
        // coercion error (top-level errors + null payload), matching Shopify.
        for input in &inputs {
            if let Some(source) = resolved_string_field(input, "originalSource") {
                if source.chars().count() > 2048 {
                    return MutationOutcome::response(media_invalid_field_arguments_response(
                        &response_key,
                        "fileUpdate",
                        "originalSource is too long (maximum is 2048)",
                    ));
                }
            }
        }

        let required_field_errors = inputs
            .iter()
            .enumerate()
            .filter_map(|(index, input)| validate_file_update_required_fields(input, index))
            .collect::<Vec<_>>();
        if !required_field_errors.is_empty() {
            return MutationOutcome::response(media_file_update_error_response(
                &response_key,
                &payload_selection,
                required_field_errors,
            ));
        }

        // Hydrate referenced products and file-update targets from upstream so
        // existence/validation checks run against the real records (Gleam parity:
        // maybe_hydrate_referenced_products + maybe_hydrate_file_update_targets).
        self.hydrate_referenced_products(request, &inputs);
        self.hydrate_file_update_targets(request, &inputs);

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

        let post_readiness_field_errors = inputs
            .iter()
            .enumerate()
            .flat_map(|(index, input)| validate_file_update_post_readiness_fields(input, index))
            .collect::<Vec<_>>();
        if !post_readiness_field_errors.is_empty() {
            return MutationOutcome::response(media_file_update_error_response(
                &response_key,
                &payload_selection,
                post_readiness_field_errors,
            ));
        }

        // Supplying both originalSource and previewImageSource is rejected with
        // two INVALID userErrors, but only for ready files: Shopify resolves the
        // NON_READY_STATE gate above first (see media-file-update-validation-ordering).
        let ready_source_errors = inputs
            .iter()
            .enumerate()
            .flat_map(|(index, input)| validate_file_update_ready_source_fields(input, index))
            .collect::<Vec<_>>();
        if !ready_source_errors.is_empty() {
            return MutationOutcome::response(media_file_update_error_response(
                &response_key,
                &payload_selection,
                ready_source_errors,
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

        let source_version_errors = inputs
            .iter()
            .enumerate()
            .filter_map(|(index, input)| file_update_source_version_conflict(input, index))
            .collect::<Vec<_>>();
        if !source_version_errors.is_empty() {
            return MutationOutcome::response(media_file_update_error_response(
                &response_key,
                &payload_selection,
                source_version_errors,
            ));
        }

        let reference_target_errors = self.validate_file_update_reference_targets(&inputs);
        if !reference_target_errors.is_empty() {
            return MutationOutcome::response(media_file_update_error_response(
                &response_key,
                &payload_selection,
                reference_target_errors,
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
            // Source/preview updates invalidate the rendered image until the
            // backend reprocesses it. The immediate payload nulls `image` (Gleam
            // update_file_record) while the existing `preview`/`url` are retained,
            // because regeneration is asynchronous.
            let content_type = file
                .get("contentType")
                .and_then(Value::as_str)
                .map(str::to_string);
            let original_source =
                resolved_string_field(input, "originalSource").filter(|value| !value.is_empty());
            let preview_source = resolved_string_field(input, "previewImageSource")
                .filter(|value| !value.is_empty());
            let source_as_preview = if content_type.as_deref() == Some("IMAGE") {
                original_source.clone()
            } else {
                None
            };
            let explicit_preview = preview_source.or(source_as_preview);
            if explicit_preview.is_some() {
                file["image"] = Value::Null;
            }
            // GenericFile renders `url` from the accepted originalSource (Gleam
            // next_original_source for FILE). Image-type files defer to async
            // regeneration and keep their hydrated preview/url instead.
            if content_type.as_deref() == Some("FILE") {
                if let Some(source) = &original_source {
                    file["url"] = json!(source);
                }
            }
            file["updatedAt"] = json!("2024-01-01T00:00:59.000Z");
            self.store
                .staged
                .media_files
                .insert(id.clone(), file.clone());
            // Cascade: detaching a file from a product (referencesToRemove)
            // removes that file from the product's media and from every variant
            // that had it attached (Gleam parity: remove_media_ids_from_variants_for_products).
            let remove_products = list_string_field(input, "referencesToRemove");
            if !remove_products.is_empty() {
                self.store
                    .clear_media_ids(std::slice::from_ref(&id), Some(&remove_products));
            }
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
        request: &Request,
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
        // Hydrate the referenced files (and their owning products/variants) so
        // existence checks run against the real backend and the post-delete
        // cascade can clear the file from those owners.
        self.hydrate_media_file_references(request, &ids);
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
        // Cascade: detach the deleted files from every product/variant that
        // referenced them, so subsequent product.media / variant.media reads no
        // longer surface the removed file (Gleam parity: delete_staged_files).
        self.store.clear_media_ids(&ids, None);
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
        let user_error_selection =
            selected_child_selection(&payload_selection, "userErrors").unwrap_or_default();
        if user_error_selection
            .iter()
            .any(|field| field.name == "code")
        {
            let operation_path = parsed_document(query, variables)
                .map(|document| document.operation_path)
                .unwrap_or_else(|| "mutation".to_string());
            return MutationOutcome::response(ok_json(json!({
                "errors": [{
                    "message": "Field 'code' doesn't exist on type 'UserError'",
                    "locations": [{"line": 7, "column": 9}],
                    "path": [operation_path, "stagedUploadsCreate", "userErrors", "code"],
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
        // Validate every input up front so we know whether the mutation will
        // succeed. A successful mutation reserves a synthetic id for its log
        // entry before allocating target ids (Gleam reserves a MutationLogEntry
        // id first), keeping target ids in lockstep with parity.
        let validations: Vec<Vec<Value>> = inputs
            .iter()
            .enumerate()
            .map(|(index, input)| validate_staged_upload_input(input, index))
            .collect();
        if validations.iter().all(Vec::is_empty) {
            self.reserve_synthetic_log_id();
        }
        let mut errors = Vec::new();
        let mut targets = Vec::new();
        for ((index, input), input_errors) in inputs.iter().enumerate().zip(validations.into_iter())
        {
            if input_errors.is_empty() {
                let id = self.next_synthetic_gid(&format!("StagedUploadTarget{index}"));
                targets.push(staged_upload_target(input, index, &id));
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

    // Hydrate file-update target records from upstream when they are not already
    // known locally, so existence/type/state validation matches the real backend.
    // In replay these reads match the recorded cassette calls; against a live
    // backend they are ordinary `nodes(ids:)` reads. A node the backend does not
    // know about comes back null and is simply not staged (-> FILE_DOES_NOT_EXIST).
    fn hydrate_file_update_targets(
        &mut self,
        request: &Request,
        inputs: &[BTreeMap<String, ResolvedValue>],
    ) {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return;
        }
        let missing_ids = dedupe_media_strings(
            inputs
                .iter()
                .filter_map(|input| resolved_string_field(input, "id"))
                .filter(|id| !id.is_empty() && self.media_file_for_update(id).is_none())
                .collect(),
        );
        if missing_ids.is_empty() {
            return;
        }
        let hydrate_request = Request {
            method: "POST".to_string(),
            path: request.path.clone(),
            headers: request.headers.clone(),
            body: serde_json::to_string(&json!({
                "query": MEDIA_FILE_UPDATE_HYDRATE_QUERY,
                "operationName": "MediaFileUpdateHydrate",
                "variables": { "fileIds": missing_ids },
            }))
            .unwrap_or_default(),
        };
        let response = (self.upstream_transport)(hydrate_request);
        if response.status >= 400 {
            return;
        }
        if let Some(nodes) = response.body["data"]["nodes"].as_array() {
            for node in nodes {
                if let Some(record) = media_file_record_from_node(node) {
                    if let Some(id) = record.get("id").and_then(Value::as_str).map(str::to_string) {
                        self.store.staged.media_files.insert(id, record);
                    }
                }
            }
        }
    }

    // Hydrate products referenced by referencesToAdd/referencesToRemove so that
    // attaching an existing product stays local-only after the read, and a missing
    // product surfaces REFERENCE_TARGET_DOES_NOT_EXIST.
    fn hydrate_referenced_products(
        &mut self,
        request: &Request,
        inputs: &[BTreeMap<String, ResolvedValue>],
    ) {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return;
        }
        let product_ids = dedupe_media_strings(
            inputs
                .iter()
                .flat_map(|input| {
                    let mut ids = list_string_field(input, "referencesToAdd");
                    ids.extend(list_string_field(input, "referencesToRemove"));
                    ids
                })
                .collect(),
        );
        for product_id in product_ids {
            if product_id.is_empty() || self.store.product_by_id(&product_id).is_some() {
                continue;
            }
            let hydrate_request = Request {
                method: "POST".to_string(),
                path: request.path.clone(),
                headers: request.headers.clone(),
                body: serde_json::to_string(&json!({
                    "query": MEDIA_PRODUCT_HYDRATE_QUERY,
                    "operationName": "MediaProductHydrate",
                    "variables": { "id": product_id },
                }))
                .unwrap_or_default(),
            };
            let response = (self.upstream_transport)(hydrate_request);
            if response.status >= 400 {
                continue;
            }
            if response.body["data"]["product"].is_object() {
                let product_node = response.body["data"]["product"].clone();
                self.observe_media_product_node(&product_node);
            }
        }
    }

    // Hydrate the products and variants that reference the given media files,
    // along with the file records themselves, from upstream. Used by fileDelete
    // (and the cascade that follows it) so existence checks and downstream
    // product.media / variant.media reads run against the real owner state.
    fn hydrate_media_file_references(&mut self, request: &Request, file_ids: &[String]) {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return;
        }
        let missing = dedupe_media_strings(
            file_ids
                .iter()
                .filter(|id| {
                    !id.is_empty() && !self.store.staged.media_files.contains_key(id.as_str())
                })
                .cloned()
                .collect(),
        );
        if missing.is_empty() {
            return;
        }
        let hydrate_request = Request {
            method: "POST".to_string(),
            path: request.path.clone(),
            headers: request.headers.clone(),
            body: serde_json::to_string(&json!({
                "query": MEDIA_FILE_REFERENCES_HYDRATE_QUERY,
                "operationName": "MediaFileReferencesHydrate",
                "variables": { "fileIds": missing },
            }))
            .unwrap_or_default(),
        };
        let response = (self.upstream_transport)(hydrate_request);
        if response.status >= 400 {
            return;
        }
        let Some(nodes) = response.body["data"]["nodes"].as_array().cloned() else {
            return;
        };
        for node in nodes {
            // Stage the file record itself so the existence check passes.
            if let Some(record) = media_file_record_from_node(&node) {
                if let Some(id) = record.get("id").and_then(Value::as_str).map(str::to_string) {
                    if !self.store.staged.deleted_media_file_ids.contains(&id) {
                        self.store.staged.media_files.entry(id).or_insert(record);
                    }
                }
            }
            // Stage every product (and its variants/media) that references it.
            if let Some(references) = node.pointer("/references/nodes").and_then(Value::as_array) {
                for product_node in references.clone() {
                    self.observe_media_product_node(&product_node);
                }
            }
        }
    }

    // Stage a product node observed from a media hydration read: the product
    // record (raw media + variant nodes), a file record for each product media
    // node, and a `ProductVariantRecord` (with `media_ids`) for each variant.
    // Without the latter two, the cascade clear and downstream variant.media
    // reads would have nothing concrete to operate on.
    fn observe_media_product_node(&mut self, product_node: &Value) {
        let Some(product_id) = product_node
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| id.starts_with("gid://shopify/Product/"))
            .map(str::to_string)
        else {
            return;
        };
        self.store.stage_observed_product_json(product_node);
        for media_node in product_node
            .pointer("/media/nodes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(record) = media_file_record_from_node(media_node) {
                if let Some(id) = record.get("id").and_then(Value::as_str).map(str::to_string) {
                    if !self.store.staged.deleted_media_file_ids.contains(&id) {
                        self.store.staged.media_files.entry(id).or_insert(record);
                    }
                }
            }
        }
        for variant_node in product_node
            .pointer("/variants/nodes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let mut variant_value = variant_node.clone();
            if let Some(object) = variant_value.as_object_mut() {
                object
                    .entry("productId".to_string())
                    .or_insert_with(|| json!(product_id));
            }
            if let Some(variant) = product_variant_state_from_observed_json(&variant_value) {
                self.store.stage_product_variant(variant);
            }
        }
    }

    // Files referencing products that do not exist (after hydration) fail with
    // REFERENCE_TARGET_DOES_NOT_EXIST (Gleam parity: validate_file_update_reference_targets).
    fn validate_file_update_reference_targets(
        &self,
        inputs: &[BTreeMap<String, ResolvedValue>],
    ) -> Vec<Value> {
        let any_missing = inputs.iter().any(|input| {
            let mut product_ids = list_string_field(input, "referencesToAdd");
            product_ids.extend(list_string_field(input, "referencesToRemove"));
            dedupe_media_strings(product_ids).iter().any(|product_id| {
                !product_id.is_empty() && self.store.product_by_id(product_id).is_none()
            })
        });
        if any_missing {
            vec![json!({
                "field": ["files"],
                "message": "The reference target does not exist",
                "code": "REFERENCE_TARGET_DOES_NOT_EXIST"
            })]
        } else {
            Vec::new()
        }
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
            match field.name.as_str() {
                "files" => {
                    let mut files = self
                        .store
                        .staged
                        .media_files
                        .iter()
                        .filter(|(id, _)| !self.store.staged.deleted_media_file_ids.contains(*id))
                        .map(|(_, file)| file.clone())
                        .collect::<Vec<_>>();
                    // Order by sortKey: ID (the numeric resource id), then honor
                    // `reverse`. Synthetic creation order tracks the numeric id,
                    // so this also approximates the default CREATED_AT ordering. A
                    // lexicographic string sort over the full gid would interleave
                    // by typename (GenericFile < MediaImage < Video), so it must be
                    // numeric.
                    files.sort_by_key(media_file_numeric_id);
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
                            media_file_cursor,
                        ),
                    );
                }
                // Saved searches are not modeled yet, so the connection mirrors
                // Shopify's empty-state shape (no nodes, null cursors) rather than
                // being dropped from a combined `files`/`fileSavedSearches` read.
                "fileSavedSearches" => {
                    data.insert(
                        field.response_key,
                        selected_connection_json_with_args(
                            Vec::<Value>::new(),
                            &field.arguments,
                            &field.selection,
                            media_file_cursor,
                        ),
                    );
                }
                _ => continue,
            }
        }
        ok_json(json!({"data": Value::Object(data)}))
    }

    // metafieldsSet/metafieldsDelete read their `metafields` list from the
    // resolved root-field arguments so inline-document forms work, not only the
    // `$metafields` variable form (matches the Gleam reference, which reads from
    // the field arguments). Falls back to top-level variables for safety.
    pub(in crate::proxy) fn owner_metafields_set(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "metafieldsSet".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let inputs = metafields_mutation_inputs(query, variables, "metafieldsSet");
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
                    "value": normalize_metafield_value_string(&metafield_type, &value),
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
        let inputs = metafields_mutation_inputs(query, variables, "metafieldsDelete");
        // A delete targeting another app's reserved namespace is not permitted;
        // Shopify rejects the whole batch before deleting anything.
        if inputs.iter().any(|input| {
            app_namespace_belongs_to_other_app(&canonical_app_metafield_namespace(
                resolved_string_field(input, "namespace").as_deref(),
            ))
        }) {
            let payload = json!({
                "deletedMetafields": [],
                "userErrors": [{
                    "field": ["metafields"],
                    "message": "Access to this namespace and key on Metafields for this resource type is not allowed."
                }]
            });
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
        // Metafields not backed by a definition return `definition: null`; hydration
        // and metafieldsSet inputs never carry one, so default it so singular
        // `metafield(namespace:, key:) { definition }` reads emit null, not undefined.
        if record.get("definition").is_none() {
            record["definition"] = Value::Null;
        }
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

    /// Stage the `metafields` array on a product-variant create/update input into
    /// the owner-metafield overlay keyed by the variant GID, mirroring how
    /// `metafieldsSet` records owner metafields. This lets a follow-up
    /// `variants { nodes { metafield(namespace:, key:) } }` read resolve the
    /// metafield through the same overlay path used for products.
    fn stage_input_variant_metafields(
        &mut self,
        owner_id: &str,
        input: &BTreeMap<String, ResolvedValue>,
    ) {
        for metafield in resolved_object_list_field(input, "metafields") {
            let Some(namespace) = resolved_string_field(&metafield, "namespace") else {
                continue;
            };
            let Some(key) = resolved_string_field(&metafield, "key") else {
                continue;
            };
            let value = resolved_string_field(&metafield, "value").unwrap_or_default();
            let metafield_type = resolved_string_field(&metafield, "type")
                .unwrap_or_else(|| "single_line_text_field".to_string());
            let index = self
                .store
                .staged
                .owner_metafields
                .values()
                .map(Vec::len)
                .sum::<usize>()
                + 1;
            let timestamp = owner_metafield_timestamp(index as u64);
            let record = json!({
                "id": format!("gid://shopify/Metafield/{index}"),
                "namespace": namespace,
                "key": key,
                "type": metafield_type,
                "value": normalize_metafield_value_string(&metafield_type, &value),
                "jsonValue": metafield_json_value(&metafield_type, &value),
                "compareDigest": format!("local-metafield-digest-{index}"),
                "createdAt": timestamp,
                "updatedAt": timestamp,
                "ownerType": owner_type_from_gid(owner_id),
            });
            self.upsert_owner_metafield_record(owner_id, record);
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
        let mut records = self.owner_metafields(owner_id, namespace.as_deref());

        // Relay pagination over the owner's metafields (stored id-ascending, which
        // mirrors Shopify's default metafield ordering). `after` drops everything up
        // to and including the cursor record; `first` truncates and drives
        // hasNextPage so chained `metafields(first:n, after:)` reads page correctly.
        let mut has_previous_page = false;
        if let Some(after) = resolved_string_field(&selection.arguments, "after") {
            if let Some(index) = records
                .iter()
                .position(|record| metafield_cursor(record).as_deref() == Some(after.as_str()))
            {
                records = records.split_off(index + 1);
                has_previous_page = true;
            }
        }
        let total_after_cursor = records.len();
        let mut has_next_page = false;
        if let Some(first) = resolved_int_field(&selection.arguments, "first") {
            if first >= 0 {
                let limit = first as usize;
                has_next_page = total_after_cursor > limit;
                records.truncate(limit);
            }
        }

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
            "pageInfo": metafield_connection_page_info(
                start_cursor,
                end_cursor,
                has_next_page,
                has_previous_page
            )
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
                "productOperation" => Some(self.product_operation_by_id_field(field)),
                "productVariant" => Some(self.product_variant_by_id_field(field)),
                "inventoryItem" => {
                    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    self.product_inventory_item_by_id_value(&id, &field.selection)
                }
                // Mixed reads pairing `product` with sibling `collection(id:)` lookups
                // (e.g. collectionsToJoin downstream parity) resolve membership locally.
                "collection" => Some(self.collection_membership_value(field)),
                _ => None,
            };
            if let Some(value) = value {
                fields.insert(field.response_key.clone(), value);
            }
        }
        Value::Object(fields)
    }

    pub(in crate::proxy) fn product_operation_by_id_field(
        &self,
        field: &RootFieldSelection,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        // `productDelete` async operations live in their own map; Set/Duplicate/Bundle
        // operations are staged in `product_operations`. Try the delete map first, then
        // fall back to the general operation store so async productSet/productDuplicate/
        // productBundleCreate reads resolve their staged operation (and its product).
        self.product_delete_operation_value_by_id(&id, &field.selection)
            .or_else(|| {
                self.store
                    .staged
                    .product_operations
                    .get(&id)
                    .map(|operation| self.product_operation_json(operation, &field.selection))
            })
            .unwrap_or(Value::Null)
    }

    pub(in crate::proxy) fn product_delete_operation_value_by_id(
        &self,
        id: &str,
        selection: &[SelectedField],
    ) -> Option<Value> {
        self.store
            .staged
            .product_delete_operations
            .get(id)
            .map(|deleted_product_id| {
                selected_json(
                    &json!({
                        "__typename": "ProductDeleteOperation",
                        "id": id,
                        "status": "COMPLETE",
                        "deletedProductId": deleted_product_id,
                        "userErrors": []
                    }),
                    selection,
                )
            })
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
                let variants = self
                    .store
                    .product_variants_for_product(&product.id)
                    .iter()
                    .map(|variant| self.variant_with_inventory_levels(variant))
                    .collect::<Vec<_>>();
                let base =
                    self.product_json_with_selling_plan_overlay(product, &variants, selection);
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
                let variants = self
                    .store
                    .product_variants_for_product(&product.id)
                    .iter()
                    .map(|variant| self.variant_with_inventory_levels(variant))
                    .collect::<Vec<_>>();
                let base =
                    self.product_json_with_selling_plan_overlay(product, &variants, selection);
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
        let variant = self.variant_with_inventory_levels(variant);
        let base = self.product_variant_json_with_selling_plan_overlay(
            &variant,
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
            let variant = self.variant_with_inventory_levels(variant);
            let product = self.store.product_by_id(&variant.product_id);
            return Some(product_variant_inventory_item_json(
                &variant, product, selection,
            ));
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

    pub(in crate::proxy) fn has_collection_overlay_state(&self) -> bool {
        self.store.has_collection_state()
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
        if input.contains_key("variants") {
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

        if input.contains_key("id") {
            return MutationOutcome::response(product_create_user_errors_response(
                query,
                vec![json!({
                    "field": ["input"],
                    "message": "id cannot be specified during creation"
                })],
            ));
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

        let id = self.next_proxy_synthetic_gid("Product");
        let handle =
            resolved_string_field(&input, "handle").unwrap_or_else(|| slugify_handle(&title));
        let status =
            resolved_string_field(&input, "status").unwrap_or_else(|| "ACTIVE".to_string());
        let timestamp = self.next_product_timestamp();
        let extra_fields = resolved_string_field(&input, "combinedListingRole")
            .map(|role| BTreeMap::from([("combinedListingRole".to_string(), json!(role))]))
            .unwrap_or_default();
        let mut product = ProductRecord {
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
            extra_fields,
        };
        // Echo product-level inputs that Shopify persists verbatim onto the created product
        // and surfaces through downstream reads.
        if let Some(requires_selling_plan) = resolved_bool_field(&input, "requiresSellingPlan") {
            product.extra_fields.insert(
                "requiresSellingPlan".to_string(),
                json!(requires_selling_plan),
            );
        }
        let is_gift_card = resolved_bool_field(&input, "giftCard").unwrap_or(false);
        if is_gift_card {
            product
                .extra_fields
                .insert("isGiftCard".to_string(), json!(true));
        }
        if let Some(suffix) = resolved_string_field(&input, "giftCardTemplateSuffix") {
            product
                .extra_fields
                .insert("giftCardTemplateSuffix".to_string(), json!(suffix));
        }
        // Shopify resolves the input `category` taxonomy GID into a `{id, fullName}`
        // object on the created product, surfaced through both the mutation payload and
        // downstream reads.
        if let Some(category_id) = product_category_input_id(&input) {
            product
                .extra_fields
                .insert("category".to_string(), product_category_value(&category_id));
        }

        // `productCreate` always materializes at least one variant. With `productOptions`,
        // Shopify creates the lead combination (only the first value of each option; further
        // values are added later via bulk create). Without options it creates a single
        // "Default Title" standalone variant.
        let mut staged_ids = vec![id.clone()];
        let variant = if let Some((options, variant)) =
            self.product_options_and_default_variant(&input, &id)
        {
            product
                .extra_fields
                .insert("options".to_string(), json!(options));
            variant
        } else {
            self.default_standalone_variant(&id, is_gift_card)
        };
        product.variants = vec![product_variant_state_json(&variant)];
        staged_ids.push(variant.id.clone());
        self.store.stage_product_variant(variant);

        // `collectionsToJoin` adds the new product to existing collections. Add the minimal
        // collection refs to the product surface before staging so the mutation response
        // renders them.
        let collections_to_join = resolved_string_list_field_unsorted(&input, "collectionsToJoin");
        for collection_id in &collections_to_join {
            if let Some(collection) = self.store.collection_by_id(collection_id).cloned() {
                upsert_minimal_collection(&mut product.collections, &collection);
            }
        }

        // Stage any `metafields` supplied on create so downstream metafield reads resolve them.
        self.stage_owner_metafields_from_input(&id, &input);

        self.store.stage_product(product.clone());

        // Register collection membership so downstream `collection` reads expose hasProduct,
        // productsCount, and the product in their member list.
        for collection_id in &collections_to_join {
            self.add_product_to_collection_membership(collection_id, &product);
        }

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
            LogDraft::staged("productCreate", "products", staged_ids),
        )
    }

    /// Build the `options` JSON and the single default variant implied by a
    /// `productCreate` `productOptions` input. Returns `None` when no `productOptions`
    /// were supplied. The lead variant uses the first value of every option; option value
    /// lists likewise contain only those lead values, matching Shopify's create behaviour.
    fn product_options_and_default_variant(
        &mut self,
        input: &BTreeMap<String, ResolvedValue>,
        product_id: &str,
    ) -> Option<(Vec<Value>, ProductVariantRecord)> {
        let product_options = Self::resolved_object_list_arg(input, "productOptions");
        if product_options.is_empty() {
            return None;
        }
        let mut options = Vec::new();
        let mut selected_options = Vec::new();
        for (index, option) in product_options.iter().enumerate() {
            let name = resolved_string_field(option, "name").unwrap_or_default();
            let value_names: Vec<String> = Self::resolved_object_list_arg(option, "values")
                .iter()
                .filter_map(|value| resolved_string_field(value, "name"))
                .collect();
            let first_value = value_names.first().cloned().unwrap_or_default();
            let option_id = self.next_proxy_synthetic_gid("ProductOption");
            // `optionValues` lists every supplied value, but only the lead value gains a
            // variant on create, so `hasVariants` is true for it alone; the string `values`
            // list (legacy field) contains just the value(s) that back a variant.
            let mut option_values = Vec::new();
            for (value_index, value_name) in value_names.iter().enumerate() {
                let option_value_id = self.next_proxy_synthetic_gid("ProductOptionValue");
                option_values.push(json!({
                    "id": option_value_id,
                    "name": value_name,
                    "hasVariants": value_index == 0,
                }));
            }
            options.push(json!({
                "id": option_id,
                "name": name,
                "position": index + 1,
                "values": [first_value.clone()],
                "optionValues": option_values,
            }));
            selected_options.push(ProductVariantSelectedOption {
                name,
                value: first_value,
            });
        }
        let title = selected_options
            .iter()
            .map(|option| option.value.clone())
            .collect::<Vec<_>>()
            .join(" / ");
        let variant = ProductVariantRecord {
            id: self.next_proxy_synthetic_gid("ProductVariant"),
            product_id: product_id.to_string(),
            title,
            sku: String::new(),
            barcode: None,
            price: "0.00".to_string(),
            compare_at_price: None,
            taxable: true,
            inventory_policy: "DENY".to_string(),
            inventory_quantity: 0,
            selected_options,
            inventory_item: ProductVariantInventoryItem {
                id: self.next_proxy_synthetic_gid("InventoryItem"),
                tracked: true,
                requires_shipping: true,
                extra_fields: BTreeMap::new(),
            },
            media_ids: Vec::new(),
            extra_fields: BTreeMap::from([("position".to_string(), json!(1))]),
        };
        Some((options, variant))
    }

    /// Build the implicit "Default Title" standalone variant Shopify creates for a product
    /// with no `productOptions`. Gift cards default to a non-taxable, non-shippable variant.
    fn default_standalone_variant(
        &mut self,
        product_id: &str,
        is_gift_card: bool,
    ) -> ProductVariantRecord {
        ProductVariantRecord {
            id: self.next_proxy_synthetic_gid("ProductVariant"),
            product_id: product_id.to_string(),
            title: "Default Title".to_string(),
            sku: String::new(),
            barcode: None,
            price: "0.00".to_string(),
            compare_at_price: None,
            taxable: !is_gift_card,
            inventory_policy: "DENY".to_string(),
            inventory_quantity: 0,
            selected_options: vec![ProductVariantSelectedOption {
                name: "Title".to_string(),
                value: "Default Title".to_string(),
            }],
            inventory_item: ProductVariantInventoryItem {
                id: self.next_proxy_synthetic_gid("InventoryItem"),
                tracked: false,
                requires_shipping: !is_gift_card,
                extra_fields: BTreeMap::new(),
            },
            media_ids: Vec::new(),
            extra_fields: BTreeMap::from([("position".to_string(), json!(1))]),
        }
    }

    /// Add a single product to a collection's membership, preserving any existing members,
    /// so downstream `collection` reads expose hasProduct/productsCount/products for it.
    fn add_product_to_collection_membership(
        &mut self,
        collection_id: &str,
        product: &ProductRecord,
    ) {
        let Some(collection) = self.store.collection_by_id(collection_id).cloned() else {
            return;
        };
        let mut members: Vec<ProductRecord> = collection
            .get("products")
            .and_then(|connection| connection.get("nodes"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(product_state_from_json)
            .collect();
        if !members.iter().any(|member| member.id == product.id) {
            members.push(product.clone());
        }
        self.store.stage_collection_membership(collection, members);
    }

    /// Stage product metafields supplied through a `metafields` create/update input so that
    /// downstream `metafield`/`metafields` reads resolve them on the owning product.
    fn stage_owner_metafields_from_input(
        &mut self,
        owner_id: &str,
        input: &BTreeMap<String, ResolvedValue>,
    ) {
        for metafield_input in resolved_object_list_field(input, "metafields") {
            let namespace =
                resolved_string_field(&metafield_input, "namespace").unwrap_or_default();
            let key = resolved_string_field(&metafield_input, "key").unwrap_or_default();
            if namespace.is_empty() && key.is_empty() {
                continue;
            }
            let metafield_type = resolved_string_field(&metafield_input, "type")
                .unwrap_or_else(|| "single_line_text_field".to_string());
            let value = resolved_string_field(&metafield_input, "value").unwrap_or_default();
            let index = self
                .store
                .staged
                .owner_metafields
                .values()
                .map(Vec::len)
                .sum::<usize>()
                + 1;
            let timestamp = owner_metafield_timestamp(index as u64);
            let metafield = json!({
                "id": format!("gid://shopify/Metafield/{index}"),
                "namespace": namespace,
                "key": key,
                "type": metafield_type,
                "value": normalize_metafield_value_string(&metafield_type, &value),
                "jsonValue": metafield_json_value(&metafield_type, &value),
                "compareDigest": format!("local-metafield-digest-{index}"),
                "createdAt": timestamp,
                "updatedAt": timestamp,
                "ownerType": owner_type_from_gid(owner_id),
                "owner": owner_reference_from_gid(owner_id),
            });
            self.store.staged.deleted_owner_metafields.remove(&(
                owner_id.to_string(),
                namespace.clone(),
                key.clone(),
            ));
            self.store
                .staged
                .owner_metafields
                .entry(owner_id.to_string())
                .or_default()
                .push(metafield);
        }
    }

    pub(in crate::proxy) fn product_update(
        &mut self,
        request: &Request,
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
        if self.store.product_by_id(&id).is_none() && self.config.read_mode == ReadMode::LiveHybrid
        {
            self.hydrate_product_nodes_for_observation_with_request(request, vec![id.clone()]);
        }
        let Some(existing) = self.store.product_staged_or_base(&id) else {
            return MutationOutcome::response(product_update_missing_product(query));
        };

        if input.contains_key("title")
            && resolved_string_field(&input, "title")
                .unwrap_or_default()
                .trim()
                .is_empty()
        {
            return self.product_update_field_user_error(
                query,
                &existing,
                "title",
                "Title can't be blank",
            );
        }

        if let Some(handle) = resolved_string_field(&input, "handle") {
            if handle.chars().count() > 255 {
                return self.product_update_field_user_error(
                    query,
                    &existing,
                    "handle",
                    "Handle is too long (maximum is 255 characters)",
                );
            }
        }

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

    /// Build a productUpdate response that returns the (unchanged) product alongside a single
    /// field-scoped userError — the shape Shopify emits when an input value is rejected
    /// (e.g. blank title, over-long handle) without persisting the mutation.
    fn product_update_field_user_error(
        &self,
        query: &str,
        existing: &ProductRecord,
        field: &str,
        message: &str,
    ) -> MutationOutcome {
        let product_selection = nested_root_field_selection(query, "product").unwrap_or_default();
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let error_selection =
            selected_child_selection(&payload_selection, "userErrors").unwrap_or_default();
        let user_error = selected_json(
            &json!({"field": [field], "message": message}),
            &error_selection,
        );
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "productUpdate".to_string());
        MutationOutcome::response(ok_json(json!({
            "data": {
                response_key: selected_json(
                    &json!({
                        "product": product_json(existing, &product_selection),
                        "userErrors": [user_error]
                    }),
                    &payload_selection
                )
            }
        })))
    }

    pub(in crate::proxy) fn product_variant_mutation(
        &mut self,
        request: &Request,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        match root_field {
            "productVariantCreate" => self.product_variant_create(query, variables),
            "productVariantUpdate" => self.product_variant_update(query, variables),
            "productVariantDelete" => self.product_variant_delete(query, variables),
            "productVariantAppendMedia" | "productVariantDetachMedia" => {
                self.product_variant_media_mutation(root_field, query, variables)
            }
            "productVariantsBulkCreate" => {
                self.product_variants_bulk_create(request, query, variables)
            }
            "productVariantsBulkUpdate" => {
                self.product_variants_bulk_update(request, query, variables)
            }
            "productVariantsBulkDelete" => {
                self.product_variants_bulk_delete(request, query, variables)
            }
            "productVariantsBulkReorder" => {
                self.product_variants_bulk_reorder(request, query, variables)
            }
            _ => MutationOutcome::response(json_error(
                400,
                "No mutation dispatcher implemented for product variant root",
            )),
        }
    }

    fn product_variant_media_mutation(
        &mut self,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let response_key = root_field_response_key(query).unwrap_or_else(|| root_field.to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let product_id = resolved_string_field(variables, "productId").unwrap_or_default();
        let variant_media = resolved_object_list_field(variables, "variantMedia");
        self.hydrate_product_variant_media_owner_state(&product_id, &variant_media);
        let user_errors =
            self.product_variant_media_user_errors(root_field, &product_id, &variant_media);

        if !user_errors.is_empty() {
            let payload = self.product_variant_media_payload_json(
                &payload_selection,
                &product_id,
                Vec::new(),
                user_errors,
            );
            return MutationOutcome::response(ok_json(json!({
                "data": { response_key: payload }
            })));
        }

        let mut changed_variant_ids = Vec::new();
        for item in &variant_media {
            let Some(variant_id) = resolved_string_field(item, "variantId") else {
                continue;
            };
            let media_ids = resolved_string_list_field_unsorted(item, "mediaIds");
            let Some(mut variant) = self.store.product_variant_by_id(&variant_id).cloned() else {
                continue;
            };
            match root_field {
                "productVariantAppendMedia" => {
                    for media_id in media_ids {
                        if !variant
                            .media_ids
                            .iter()
                            .any(|existing| existing == &media_id)
                        {
                            variant.media_ids.push(media_id);
                        }
                    }
                }
                "productVariantDetachMedia" => {
                    let removals = media_ids.into_iter().collect::<BTreeSet<_>>();
                    variant
                        .media_ids
                        .retain(|media_id| !removals.contains(media_id));
                }
                _ => {}
            }
            changed_variant_ids.push(variant.id.clone());
            self.store.stage_product_variant(variant);
        }

        let payload = self.product_variant_media_payload_json(
            &payload_selection,
            &product_id,
            changed_variant_ids.clone(),
            Vec::new(),
        );
        MutationOutcome::staged(
            ok_json(json!({ "data": { response_key: payload } })),
            LogDraft::staged(
                root_field,
                "products",
                std::iter::once(product_id)
                    .chain(changed_variant_ids)
                    .collect(),
            ),
        )
    }

    fn hydrate_product_variant_media_owner_state(
        &mut self,
        product_id: &str,
        variant_media: &[BTreeMap<String, ResolvedValue>],
    ) {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return;
        }
        let mut ids = Vec::new();
        if !product_id.is_empty() && self.store.product_by_id(product_id).is_none() {
            ids.push(product_id.to_string());
        }
        for item in variant_media {
            let Some(variant_id) = resolved_string_field(item, "variantId") else {
                continue;
            };
            if self.store.product_variant_by_id(&variant_id).is_none() {
                ids.push(variant_id);
            }
        }
        ids.sort();
        ids.dedup();
        self.hydrate_product_nodes_for_observation(ids);
    }

    fn product_variant_media_user_errors(
        &self,
        root_field: &str,
        product_id: &str,
        variant_media: &[BTreeMap<String, ResolvedValue>],
    ) -> Vec<Value> {
        let mut user_errors = Vec::new();
        for (entry_index, item) in variant_media.iter().enumerate() {
            let variant_id = resolved_string_field(item, "variantId").unwrap_or_default();
            let media_ids = resolved_string_list_field_unsorted(item, "mediaIds");
            let Some(variant) = self.store.product_variant_by_id(&variant_id) else {
                user_errors.push(product_variant_media_user_error(
                    &["variantMedia", &entry_index.to_string(), "variantId"],
                    "Variant does not exist on the specified product.",
                    "PRODUCT_VARIANT_DOES_NOT_EXIST_ON_PRODUCT",
                ));
                continue;
            };
            if variant.product_id != product_id {
                user_errors.push(product_variant_media_user_error(
                    &["variantMedia", &entry_index.to_string(), "variantId"],
                    "Variant does not exist on the specified product.",
                    "PRODUCT_VARIANT_DOES_NOT_EXIST_ON_PRODUCT",
                ));
                continue;
            }
            for media_id in media_ids {
                let media = self.store.product_media_by_id(product_id, &media_id);
                if media.is_none() {
                    user_errors.push(product_variant_media_user_error(
                        &["variantMedia", &entry_index.to_string(), "mediaIds"],
                        "Media does not exist on the specified product.",
                        "MEDIA_DOES_NOT_EXIST_ON_PRODUCT",
                    ));
                    continue;
                }
                if root_field == "productVariantAppendMedia"
                    && media
                        .as_ref()
                        .and_then(|media| media.get("status"))
                        .and_then(Value::as_str)
                        != Some("READY")
                {
                    user_errors.push(product_variant_media_user_error(
                        &["variantMedia", &entry_index.to_string(), "mediaIds"],
                        "Non-ready media cannot be attached to variants.",
                        "NON_READY_MEDIA",
                    ));
                    continue;
                }
                if root_field == "productVariantDetachMedia"
                    && !variant
                        .media_ids
                        .iter()
                        .any(|existing| existing == &media_id)
                {
                    user_errors.push(product_variant_media_user_error(
                        &["variantMedia", &entry_index.to_string(), "variantId"],
                        "The specified media is not attached to the specified variant.",
                        "MEDIA_IS_NOT_ATTACHED_TO_VARIANT",
                    ));
                }
            }
        }
        user_errors
    }

    fn product_variant_media_payload_json(
        &self,
        payload_selection: &[SelectedField],
        product_id: &str,
        variant_ids: Vec<String>,
        user_errors: Vec<Value>,
    ) -> Value {
        selected_payload_json(payload_selection, |selection| {
            match selection.name.as_str() {
                "product" => Some(match self.store.product_by_id(product_id) {
                    Some(product) if user_errors.is_empty() => {
                        let variants = self.store.product_variants_for_product(product_id);
                        product_json_with_variants(product, &variants, &selection.selection)
                    }
                    _ => Value::Null,
                }),
                "productVariants" => Some(if user_errors.is_empty() {
                    Value::Array(
                        variant_ids
                            .iter()
                            .filter_map(|variant_id| self.store.product_variant_by_id(variant_id))
                            .map(|variant| {
                                product_variant_json(
                                    variant,
                                    self.store.product_by_id(&variant.product_id),
                                    &selection.selection,
                                )
                            })
                            .collect(),
                    )
                } else {
                    Value::Null
                }),
                "userErrors" => Some(Value::Array(
                    user_errors
                        .iter()
                        .map(|error| selected_json(error, &selection.selection))
                        .collect(),
                )),
                _ => None,
            }
        })
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

        // Shopify replaces the implicit `Title: Default Title` standalone variant the first
        // time a real variant is created on a product that still only carries it, rather than
        // keeping the auto-generated default alongside the new variant. Capture the pre-create
        // variant set so we can drop the default once the real variant is staged.
        let existing_variants = self.store.product_variants_for_product(&variant.product_id);
        let replace_default = existing_variants.len() == 1
            && existing_variants
                .first()
                .is_some_and(Self::is_standalone_default_variant);

        self.store.stage_product_variant(variant.clone());

        if replace_default {
            for existing in &existing_variants {
                self.store.delete_product_variant(&existing.id);
            }
            let final_variants = self.store.product_variants_for_product(&variant.product_id);
            self.recompute_product_options_from_variants(&variant.product_id, &final_variants);
        }

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

    /// Shopify's auto-generated standalone variant carries the single option
    /// `Title: Default Title`. Recognising it lets the default bulk-create strategy
    /// replace it (rather than appending alongside it).
    fn is_standalone_default_variant(variant: &ProductVariantRecord) -> bool {
        variant.selected_options.len() == 1
            && variant.selected_options[0].name == "Title"
            && variant.selected_options[0].value == "Default Title"
    }

    /// Rederive a product's `options` (and their `optionValues`) from the supplied
    /// variant set, in first-seen order. Existing option and option-value identities are
    /// preserved by name so a value that survives keeps its id; newly introduced values
    /// receive freshly allocated synthetic ids.
    /// Recompute the product's denormalized inventory aggregates from its current
    /// effective variant set and re-stage the product. `totalInventory` only counts
    /// variants whose inventory item is tracked, and `tracksInventory` is true when
    /// any variant is tracked. Mirrors the `productSet` recompute so bulk-variant
    /// mutations keep `product.totalInventory`/`tracksInventory` consistent with the
    /// staged variants for downstream reads.
    fn sync_product_inventory_aggregates(&mut self, product_id: &str) {
        let final_variants = self.store.product_variants_for_product(product_id);
        let Some(mut product) = self.store.product_by_id(product_id).cloned() else {
            return;
        };
        product.total_inventory = final_variants
            .iter()
            .filter(|variant| variant.inventory_item.tracked)
            .map(|variant| variant.inventory_quantity)
            .sum::<i64>();
        product.tracks_inventory = final_variants
            .iter()
            .any(|variant| variant.inventory_item.tracked);
        self.store.stage_product(product);
    }

    fn recompute_product_options_from_variants(
        &mut self,
        product_id: &str,
        final_variants: &[ProductVariantRecord],
    ) {
        let Some(mut product) = self.store.product_by_id(product_id).cloned() else {
            return;
        };
        let existing_options: Vec<Value> = product
            .extra_fields
            .get("options")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut existing_option_id: BTreeMap<String, Value> = BTreeMap::new();
        let mut existing_value_id: BTreeMap<(String, String), Value> = BTreeMap::new();
        for option in &existing_options {
            let name = option
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if let Some(id) = option.get("id") {
                if !id.is_null() {
                    existing_option_id.insert(name.clone(), id.clone());
                }
            }
            for value in option
                .get("optionValues")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if let (Some(value_name), Some(id)) =
                    (value.get("name").and_then(Value::as_str), value.get("id"))
                {
                    if !id.is_null() {
                        existing_value_id
                            .insert((name.clone(), value_name.to_string()), id.clone());
                    }
                }
            }
        }

        let mut option_names: Vec<String> = Vec::new();
        let mut option_values_by_name: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for variant in final_variants {
            for selected in &variant.selected_options {
                if !option_names.contains(&selected.name) {
                    option_names.push(selected.name.clone());
                }
                let values = option_values_by_name
                    .entry(selected.name.clone())
                    .or_default();
                if !values.contains(&selected.value) {
                    values.push(selected.value.clone());
                }
            }
        }

        let mut new_options = Vec::new();
        for (position, name) in option_names.iter().enumerate() {
            let values = option_values_by_name.get(name).cloned().unwrap_or_default();
            let option_id = match existing_option_id.get(name) {
                Some(id) => id.clone(),
                None => json!(self.next_proxy_synthetic_gid("ProductOption")),
            };
            let mut option_values = Vec::new();
            for value in &values {
                let value_id = match existing_value_id.get(&(name.clone(), value.clone())) {
                    Some(id) => id.clone(),
                    None => json!(self.next_proxy_synthetic_gid("ProductOptionValue")),
                };
                option_values.push(json!({
                    "id": value_id,
                    "name": value,
                    "hasVariants": true,
                }));
            }
            new_options.push(json!({
                "id": option_id,
                "name": name,
                "position": position + 1,
                "values": values,
                "optionValues": option_values,
            }));
        }

        product
            .extra_fields
            .insert("options".to_string(), json!(new_options));
        self.store.stage_product(product);
    }

    fn product_variants_bulk_create(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let Some(field) =
            Self::product_variant_bulk_root_field(query, variables, "productVariantsBulkCreate")
        else {
            return MutationOutcome::response(json_error(
                400,
                "No productVariantsBulkCreate root field found",
            ));
        };
        let product_id = resolved_string_arg(&field.arguments, "productId").unwrap_or_default();
        let variants_input = Self::resolved_object_list_arg(&field.arguments, "variants");
        if variants_input.len() > 2048 {
            return MutationOutcome::response(Self::product_variant_bulk_input_size_error(
                &field,
                variants_input.len(),
            ));
        }
        let Some(product) = self
            .product_for_bulk_variant_mutation(request, &product_id)
            .cloned()
        else {
            return MutationOutcome::response(self.product_variants_bulk_response(
                &field,
                "productVariantsBulkCreate",
                None,
                Some(Vec::new()),
                vec![Self::bulk_user_error(
                    &["productId"],
                    "Product does not exist",
                    Some("PRODUCT_DOES_NOT_EXIST"),
                )],
            ));
        };

        let mut user_errors = Vec::new();
        for (index, input) in variants_input.iter().enumerate() {
            user_errors.extend(product_variant_input_user_errors_with_prefix(
                input,
                &["variants".to_string(), index.to_string()],
            ));
            user_errors.extend(Self::product_variant_bulk_option_user_errors(
                input, &product, index, false,
            ));
            user_errors
                .extend(self.product_variant_bulk_inventory_location_user_errors(input, index));
        }
        if Self::product_variant_effective_count_after_create(
            &self.store,
            &product.id,
            variants_input.len(),
        ) > 2048
        {
            user_errors.push(Self::bulk_user_error(
                &["variants"],
                "Product cannot have more than 2048 variants",
                Some("TOO_MANY_VARIANTS"),
            ));
        }
        if !user_errors.is_empty() {
            return MutationOutcome::response(self.product_variants_bulk_response(
                &field,
                "productVariantsBulkCreate",
                None,
                Some(Vec::new()),
                user_errors,
            ));
        }

        let strategy = resolved_string_arg(&field.arguments, "strategy");
        let existing_variants = self.store.product_variants_for_product(&product.id);
        let existing_variant_count = existing_variants.len();
        let mut created_variants = Vec::new();
        for (index, input) in variants_input.iter().enumerate() {
            let variant_id = self.next_proxy_synthetic_gid("ProductVariant");
            let inventory_item_id = self.next_proxy_synthetic_gid("InventoryItem");
            let mut variant = product_variant_record_from_create_input(
                input,
                variant_id,
                product.id.clone(),
                inventory_item_id,
            );
            Self::normalize_bulk_variant_title(&mut variant);
            // Shopify assigns sequential 1-based positions in product-variant order;
            // the bulk create input never supplies one, so derive it from the variant
            // count already on the product.
            variant
                .extra_fields
                .entry("position".to_string())
                .or_insert_with(|| json!(existing_variant_count + index + 1));
            created_variants.push(variant);
        }
        for variant in &created_variants {
            self.store.stage_product_variant(variant.clone());
        }
        for (variant, input) in created_variants.iter().zip(variants_input.iter()) {
            self.stage_input_variant_metafields(&variant.id, input);
        }

        // Apply the bulk-create variant strategy. `REMOVE_STANDALONE_VARIANT` drops the
        // product's lone pre-existing variant; the default strategy only drops it when it
        // is Shopify's auto-generated `Title: Default Title` standalone variant. With either
        // removal, and whenever a strategy is supplied, the product's option values are
        // rederived from the surviving variant set (existing values are preserved by name).
        if let Some(strategy) = strategy.as_deref() {
            let remove_existing = match strategy {
                "REMOVE_STANDALONE_VARIANT" => existing_variant_count == 1,
                "DEFAULT" => {
                    existing_variant_count == 1
                        && existing_variants
                            .first()
                            .is_some_and(Self::is_standalone_default_variant)
                }
                _ => false,
            };
            if remove_existing {
                for variant in &existing_variants {
                    self.store.delete_product_variant(&variant.id);
                }
            }
            let final_variants = self.store.product_variants_for_product(&product.id);
            self.recompute_product_options_from_variants(&product.id, &final_variants);
        }

        self.sync_product_inventory_aggregates(&product.id);

        let mut staged_ids = vec![product.id.clone()];
        staged_ids.extend(created_variants.iter().map(|variant| variant.id.clone()));
        MutationOutcome::staged(
            self.product_variants_bulk_response(
                &field,
                "productVariantsBulkCreate",
                self.store.product_by_id(&product.id),
                Some(created_variants),
                Vec::new(),
            ),
            LogDraft::staged("productVariantsBulkCreate", "products", staged_ids),
        )
    }

    fn product_variants_bulk_update(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let Some(field) =
            Self::product_variant_bulk_root_field(query, variables, "productVariantsBulkUpdate")
        else {
            return MutationOutcome::response(json_error(
                400,
                "No productVariantsBulkUpdate root field found",
            ));
        };
        let product_id = resolved_string_arg(&field.arguments, "productId").unwrap_or_default();
        let variants_input = Self::resolved_object_list_arg(&field.arguments, "variants");
        // Hydrate the product together with the variants referenced by the update so
        // a cold backend stages both before the update is applied, matching the
        // node hydration the proxy records during capture.
        let hydrate_variant_ids: Vec<String> = variants_input
            .iter()
            .filter_map(|input| resolved_string_field(input, "id"))
            .collect();
        let Some(product) = self
            .product_for_bulk_variant_mutation_with_variant_ids(
                request,
                &product_id,
                &hydrate_variant_ids,
            )
            .cloned()
        else {
            return MutationOutcome::response(self.product_variants_bulk_response(
                &field,
                "productVariantsBulkUpdate",
                None,
                None,
                vec![Self::bulk_user_error(
                    &["productId"],
                    "Product does not exist",
                    Some("PRODUCT_DOES_NOT_EXIST"),
                )],
            ));
        };
        if variants_input.is_empty() {
            return MutationOutcome::response(self.product_variants_bulk_response(
                &field,
                "productVariantsBulkUpdate",
                None,
                None,
                vec![json!({
                    "field": Value::Null,
                    "message": "Something went wrong, please try again.",
                    "code": Value::Null,
                })],
            ));
        }

        let mut user_errors = Vec::new();
        let mut updated_variants = Vec::new();
        for (index, input) in variants_input.iter().enumerate() {
            let prefix = ["variants".to_string(), index.to_string()];
            let Some(variant_id) = resolved_string_field(input, "id") else {
                user_errors.push(Self::bulk_user_error(
                    &["variants", &index.to_string(), "id"],
                    "Product variant is missing ID attribute",
                    Some("PRODUCT_VARIANT_ID_MISSING"),
                ));
                continue;
            };
            let Some(existing) = self.store.product_variant_by_id(&variant_id).cloned() else {
                user_errors.push(Self::bulk_user_error(
                    &["variants", &index.to_string(), "id"],
                    "Product variant does not exist",
                    Some("PRODUCT_VARIANT_DOES_NOT_EXIST"),
                ));
                continue;
            };
            if existing.product_id != product.id {
                user_errors.push(Self::bulk_user_error(
                    &["variants", &index.to_string(), "id"],
                    "Product variant does not exist",
                    Some("PRODUCT_VARIANT_DOES_NOT_EXIST"),
                ));
                continue;
            }
            if input.contains_key("inventoryQuantities") {
                user_errors.push(Self::bulk_user_error(
                    &["variants", &index.to_string(), "inventoryQuantities"],
                    "Inventory quantities can only be provided during create. To update inventory for existing variants, use inventoryAdjustQuantities.",
                    Some("NO_INVENTORY_QUANTITIES_ON_VARIANTS_UPDATE"),
                ));
            }
            user_errors.extend(product_variant_input_user_errors_with_prefix(
                input, &prefix,
            ));
            user_errors.extend(Self::product_variant_bulk_option_user_errors(
                input, &product, index, true,
            ));
            let mut variant = existing;
            apply_product_variant_input(&mut variant, input);
            Self::normalize_bulk_variant_title(&mut variant);
            updated_variants.push(variant);
        }
        if !user_errors.is_empty() {
            return MutationOutcome::response(self.product_variants_bulk_response(
                &field,
                "productVariantsBulkUpdate",
                Some(&product),
                None,
                user_errors,
            ));
        }

        for variant in &updated_variants {
            self.store.stage_product_variant(variant.clone());
        }
        for (variant, input) in updated_variants.iter().zip(variants_input.iter()) {
            self.stage_input_variant_metafields(&variant.id, input);
        }
        let mut staged_ids = vec![product.id.clone()];
        staged_ids.extend(updated_variants.iter().map(|variant| variant.id.clone()));
        MutationOutcome::staged(
            self.product_variants_bulk_response(
                &field,
                "productVariantsBulkUpdate",
                self.store.product_by_id(&product.id),
                Some(updated_variants),
                Vec::new(),
            ),
            LogDraft::staged("productVariantsBulkUpdate", "products", staged_ids),
        )
    }

    fn product_variants_bulk_delete(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let Some(field) =
            Self::product_variant_bulk_root_field(query, variables, "productVariantsBulkDelete")
        else {
            return MutationOutcome::response(json_error(
                400,
                "No productVariantsBulkDelete root field found",
            ));
        };
        let product_id = resolved_string_arg(&field.arguments, "productId").unwrap_or_default();
        let variant_ids = resolved_string_list_arg(&field.arguments, "variantsIds");
        // Hydrate the product together with the variants being deleted so a cold
        // backend stages both before applying the delete, matching the node
        // hydration recorded during capture.
        let Some(product) = self
            .product_for_bulk_variant_mutation_with_variant_ids(request, &product_id, &variant_ids)
            .cloned()
        else {
            return MutationOutcome::response(self.product_variants_bulk_response(
                &field,
                "productVariantsBulkDelete",
                None,
                None,
                vec![Self::bulk_user_error(
                    &["productId"],
                    "Product does not exist",
                    Some("PRODUCT_DOES_NOT_EXIST"),
                )],
            ));
        };

        let mut user_errors = Vec::new();
        for (index, variant_id) in variant_ids.iter().enumerate() {
            let belongs_to_product = self
                .store
                .product_variant_by_id(variant_id)
                .is_some_and(|variant| variant.product_id == product.id);
            if !belongs_to_product {
                user_errors.push(Self::bulk_user_error(
                    &["variantsIds", &index.to_string()],
                    "At least one variant does not belong to the product",
                    Some("AT_LEAST_ONE_VARIANT_DOES_NOT_BELONG_TO_THE_PRODUCT"),
                ));
            }
        }
        if !user_errors.is_empty() {
            return MutationOutcome::response(self.product_variants_bulk_response(
                &field,
                "productVariantsBulkDelete",
                None,
                None,
                user_errors,
            ));
        }

        for variant_id in &variant_ids {
            self.store.delete_product_variant(variant_id);
        }
        MutationOutcome::staged(
            self.product_variants_bulk_response(
                &field,
                "productVariantsBulkDelete",
                self.store.product_by_id(&product.id),
                None,
                Vec::new(),
            ),
            LogDraft::staged(
                "productVariantsBulkDelete",
                "products",
                std::iter::once(product.id.clone())
                    .chain(variant_ids.iter().cloned())
                    .collect(),
            ),
        )
    }

    fn product_variants_bulk_reorder(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let Some(field) =
            Self::product_variant_bulk_root_field(query, variables, "productVariantsBulkReorder")
        else {
            return MutationOutcome::response(json_error(
                400,
                "No productVariantsBulkReorder root field found",
            ));
        };
        let product_id = resolved_string_arg(&field.arguments, "productId").unwrap_or_default();
        let positions = Self::resolved_object_list_arg(&field.arguments, "positions");
        let position_variant_ids = positions
            .iter()
            .filter_map(|position| resolved_string_field(position, "id"))
            .collect::<Vec<_>>();
        let Some(product) = self
            .product_for_bulk_variant_mutation_with_variant_ids(
                request,
                &product_id,
                &position_variant_ids,
            )
            .cloned()
        else {
            return MutationOutcome::response(self.product_variants_bulk_response(
                &field,
                "productVariantsBulkReorder",
                None,
                None,
                vec![Self::bulk_user_error(
                    &["productId"],
                    "Product does not exist",
                    Some("PRODUCT_DOES_NOT_EXIST"),
                )],
            ));
        };

        let mut user_errors = Vec::new();
        let mut ordered_positions = Vec::new();
        for (index, position) in positions.iter().enumerate() {
            let Some(variant_id) = resolved_string_field(position, "id") else {
                user_errors.push(Self::bulk_user_error(
                    &["positions", &index.to_string(), "id"],
                    "Product variant is missing ID attribute",
                    Some("PRODUCT_VARIANT_ID_MISSING"),
                ));
                continue;
            };
            if self
                .store
                .product_variant_by_id(&variant_id)
                .is_none_or(|variant| variant.product_id != product.id)
            {
                user_errors.push(Self::bulk_user_error(
                    &["positions", &index.to_string(), "id"],
                    "Product variant does not exist",
                    Some("PRODUCT_VARIANT_DOES_NOT_EXIST"),
                ));
                continue;
            }
            let position_value = resolved_int_field(position, "position").unwrap_or(index as i64);
            ordered_positions.push((position_value, index, variant_id));
        }
        if !user_errors.is_empty() {
            return MutationOutcome::response(self.product_variants_bulk_response(
                &field,
                "productVariantsBulkReorder",
                None,
                None,
                user_errors,
            ));
        }

        ordered_positions.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));
        let ordered_ids = ordered_positions
            .into_iter()
            .map(|(_, _, variant_id)| variant_id)
            .collect::<Vec<_>>();
        self.store
            .reorder_product_variants(&product.id, &ordered_ids);
        MutationOutcome::staged(
            self.product_variants_bulk_response(
                &field,
                "productVariantsBulkReorder",
                self.store.product_by_id(&product.id),
                None,
                Vec::new(),
            ),
            LogDraft::staged(
                "productVariantsBulkReorder",
                "products",
                std::iter::once(product.id.clone())
                    .chain(ordered_ids)
                    .collect(),
            ),
        )
    }

    fn product_variants_bulk_response(
        &self,
        field: &RootFieldSelection,
        _root_field: &str,
        product: Option<&ProductRecord>,
        variants: Option<Vec<ProductVariantRecord>>,
        user_errors: Vec<Value>,
    ) -> Response {
        let payload =
            self.product_variants_bulk_payload_json(field, product, variants, user_errors);
        ok_json(json!({
            "data": {
                field.response_key.clone(): payload
            }
        }))
    }

    fn product_variants_bulk_payload_json(
        &self,
        field: &RootFieldSelection,
        product: Option<&ProductRecord>,
        variants: Option<Vec<ProductVariantRecord>>,
        user_errors: Vec<Value>,
    ) -> Value {
        let root_field = field.name.as_str();
        selected_payload_json(&field.selection, |selection| {
            match selection.name.as_str() {
                "product" => Some(match product {
                    Some(product) => {
                        let variants = self.store.product_variants_for_product(&product.id);
                        product_json_with_variants(product, &variants, &selection.selection)
                    }
                    None => Value::Null,
                }),
                "productVariants" => Some(match variants.as_ref() {
                    Some(variants) => Value::Array(
                        variants
                            .iter()
                            .map(|variant| {
                                product_variant_json(
                                    variant,
                                    self.store.product_by_id(&variant.product_id),
                                    &selection.selection,
                                )
                            })
                            .collect(),
                    ),
                    None if root_field == "productVariantsBulkCreate" => Value::Array(Vec::new()),
                    None => Value::Null,
                }),
                "userErrors" => Some(Value::Array(
                    user_errors
                        .iter()
                        .map(|error| selected_json(error, &selection.selection))
                        .collect(),
                )),
                _ => None,
            }
        })
    }

    fn product_for_bulk_variant_mutation(
        &mut self,
        request: &Request,
        product_id: &str,
    ) -> Option<&ProductRecord> {
        self.product_for_bulk_variant_mutation_with_variant_ids(request, product_id, &[])
    }

    fn product_for_bulk_variant_mutation_with_variant_ids(
        &mut self,
        request: &Request,
        product_id: &str,
        variant_ids: &[String],
    ) -> Option<&ProductRecord> {
        if self.store.product_by_id(product_id).is_none()
            && self.config.read_mode == ReadMode::LiveHybrid
        {
            let mut hydrate_ids = vec![product_id.to_string()];
            hydrate_ids.extend(variant_ids.iter().cloned());
            if hydrate_ids.len() > 1 {
                let mut tail = hydrate_ids.split_off(1);
                tail.sort();
                hydrate_ids.extend(tail);
            }
            self.hydrate_product_nodes_for_observation_with_request(request, hydrate_ids);
        }
        self.store.product_by_id(product_id)
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

    fn product_variant_bulk_root_field(
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        root_field: &str,
    ) -> Option<RootFieldSelection> {
        root_fields(query, variables)?
            .into_iter()
            .find(|field| field.name == root_field)
    }

    fn resolved_object_list_arg(
        arguments: &BTreeMap<String, ResolvedValue>,
        name: &str,
    ) -> Vec<BTreeMap<String, ResolvedValue>> {
        match arguments.get(name) {
            Some(ResolvedValue::List(values)) => values
                .iter()
                .filter_map(|value| match value {
                    ResolvedValue::Object(object) => Some(object.clone()),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    fn product_variant_bulk_input_size_error(field: &RootFieldSelection, size: usize) -> Response {
        ok_json(json!({
            "errors": [{
                "message": format!(
                    "The input array size of {} is greater than the maximum allowed of 2048.",
                    size
                ),
                "locations": [{
                    "line": field.location.line,
                    "column": field.location.column
                }],
                "path": [field.name, "variants"],
                "extensions": {
                    "code": "MAX_INPUT_SIZE_EXCEEDED"
                }
            }]
        }))
    }

    fn bulk_user_error(field: &[&str], message: &str, code: Option<&str>) -> Value {
        json!({
            "field": field,
            "message": message,
            "code": code
                .map(|code| Value::String(code.to_string()))
                .unwrap_or(Value::Null),
        })
    }

    fn product_variant_bulk_option_user_errors(
        input: &BTreeMap<String, ResolvedValue>,
        product: &ProductRecord,
        index: usize,
        update: bool,
    ) -> Vec<Value> {
        let options = resolved_object_list_field(input, "optionValues");
        let mut errors = Vec::new();
        let mut names = BTreeSet::new();
        let product_option_names = Self::product_option_names(product);
        for (option_index, option) in options.iter().enumerate() {
            let option_name = resolved_string_field(option, "optionName")
                .or_else(|| resolved_string_field(option, "name"))
                .unwrap_or_default();
            if !option_name.is_empty() && !names.insert(option_name.clone()) {
                errors.push(Self::bulk_user_error(
                    &["variants", &index.to_string(), "optionValues"],
                    &format!("Duplicated option name '{}'", option_name),
                    Some("INVALID_INPUT"),
                ));
                break;
            }
            if (option_name.is_empty()
                && (option.contains_key("optionId") || option.contains_key("id")))
                || (!option_name.is_empty()
                    && !product_option_names.is_empty()
                    && !product_option_names.contains(&option_name))
            {
                errors.push(Self::bulk_user_error(
                    &[
                        "variants",
                        &index.to_string(),
                        "optionValues",
                        &option_index.to_string(),
                    ],
                    "Option does not exist",
                    Some(if update {
                        "OPTION_DOES_NOT_EXIST"
                    } else {
                        "INVALID_INPUT"
                    }),
                ));
                break;
            }
        }
        if errors.is_empty() && !update {
            for option_name in product_option_names {
                if !names.contains(&option_name) {
                    errors.push(Self::bulk_user_error(
                        &["variants", &index.to_string()],
                        &format!("You need to add option values for {}", option_name),
                        Some("NEED_TO_ADD_OPTION_VALUES"),
                    ));
                    break;
                }
            }
        }
        errors
    }

    fn product_variant_bulk_inventory_location_user_errors(
        &self,
        input: &BTreeMap<String, ResolvedValue>,
        index: usize,
    ) -> Vec<Value> {
        let variant_title = resolved_product_variant_selected_options(input)
            .iter()
            .map(|option| option.value.as_str())
            .collect::<Vec<_>>()
            .join(" / ");
        if resolved_object_list_field(input, "inventoryQuantities")
            .iter()
            .any(|quantity| {
                resolved_string_field(quantity, "locationId")
                    .is_some_and(|location_id| !self.bulk_variant_location_exists(&location_id))
            })
        {
            vec![Self::bulk_user_error(
                &["variants", &index.to_string(), "inventoryQuantities"],
                &format!(
                    "Quantity for {} couldn't be set because the location was deleted.",
                    if variant_title.is_empty() {
                        "variant"
                    } else {
                        &variant_title
                    }
                ),
                Some("TRACKED_VARIANT_LOCATION_NOT_FOUND"),
            )]
        } else {
            Vec::new()
        }
    }

    fn bulk_variant_location_exists(&self, location_id: &str) -> bool {
        location_id == "gid://shopify/Location/1"
            || self.store.staged.locations.contains_key(location_id)
            || self
                .store
                .staged
                .fulfillment_service_locations
                .contains_key(location_id)
    }

    fn product_option_names(product: &ProductRecord) -> BTreeSet<String> {
        product
            .extra_fields
            .get("options")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|option| option.get("name").and_then(Value::as_str))
            .map(str::to_string)
            .collect()
    }

    fn product_variant_effective_count_after_create(
        store: &Store,
        product_id: &str,
        create_count: usize,
    ) -> usize {
        store.product_variants_for_product(product_id).len() + create_count
    }

    fn normalize_bulk_variant_title(variant: &mut ProductVariantRecord) {
        if variant.title == "Default Title" && !variant.selected_options.is_empty() {
            variant.title = variant
                .selected_options
                .iter()
                .map(|option| option.value.as_str())
                .collect::<Vec<_>>()
                .join(" / ");
        }
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
        request: &Request,
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
        if !self.store.has_product(&id) && self.config.read_mode == ReadMode::LiveHybrid {
            self.hydrate_product_nodes_for_observation_with_request(request, vec![id.clone()]);
        }
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

    pub(in crate::proxy) fn hydrate_product_for_tags(
        &self,
        id: &str,
        request: &Request,
    ) -> Option<ProductRecord> {
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

    pub(in crate::proxy) fn taggable_resource_staged_or_hydrated(
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
        let name_is_blank = requested_name.trim().is_empty();
        if name_is_blank {
            user_errors.push(json!({
                "field": ["input", "name"],
                "message": "Name can't be blank"
            }));
        }
        if !name_is_blank
            && (is_reserved_saved_search_name(&existing.resource_type, &requested_name)
                || self.saved_search_name_exists(
                    &existing.resource_type,
                    &requested_name,
                    Some(&id),
                ))
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

    pub(in crate::proxy) fn product_publication_mutation(
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
                "No product publication mutation root field found",
            ));
        };
        let response_key = field.response_key.clone();
        let payload_selection = field.selection.clone();
        let product_selection =
            selected_child_selection(&payload_selection, "product").unwrap_or_default();
        let input = match field.arguments.get("input") {
            Some(ResolvedValue::Object(input)) => input.clone(),
            _ => BTreeMap::new(),
        };
        let product_id = resolved_string_field(&input, "id").unwrap_or_default();
        let local_product = self.store.product_staged_or_base(&product_id);
        let enforce_known_publication_state = local_product
            .as_ref()
            .is_some_and(product_publication_state_known);
        let mut product = local_product
            .or_else(|| self.hydrate_product_for_publication(&product_id, request))
            .unwrap_or_else(|| {
                let timestamp = default_product_timestamp(&product_id);
                ProductRecord {
                    id: product_id.clone(),
                    created_at: timestamp.clone(),
                    updated_at: timestamp,
                    status: "ACTIVE".to_string(),
                    ..ProductRecord::default()
                }
            });

        let targets = product_publication_input_entries(&input);
        let user_errors = self.product_publication_user_errors(
            root_field,
            &product,
            &targets,
            enforce_known_publication_state,
        );
        if user_errors.is_empty() {
            let mut existing = product_publication_entries(&product);
            match root_field {
                "productPublish" => {
                    for target in &targets {
                        let Some(publication_id) = target.target_id() else {
                            continue;
                        };
                        if !existing
                            .iter()
                            .any(|entry| entry.publication_id == publication_id)
                        {
                            existing.push(ProductPublicationEntry {
                                publication_id: publication_id.to_string(),
                                publish_date: target.publish_date.clone(),
                                published_at: Some(
                                    target
                                        .publish_date
                                        .clone()
                                        .unwrap_or_else(|| self.next_product_timestamp()),
                                ),
                            });
                        }
                    }
                }
                "productUnpublish" => {
                    let remove_ids = targets
                        .iter()
                        .filter_map(ProductPublicationInputEntry::target_id)
                        .collect::<BTreeSet<_>>();
                    existing.retain(|entry| !remove_ids.contains(entry.publication_id.as_str()));
                }
                _ => {}
            }
            product.updated_at = self.next_product_updated_at(&product.updated_at);
            set_product_publication_entries(&mut product, existing);
            self.store.stage_product(product.clone());
        }

        let payload = selected_payload_json(&payload_selection, |selection| {
            match selection.name.as_str() {
                "product" => Some(product_json(&product, &product_selection)),
                "userErrors" => Some(selected_product_publication_user_errors(
                    &user_errors,
                    &selection.selection,
                )),
                _ => None,
            }
        });
        let response = ok_json(json!({ "data": { response_key: payload } }));
        if user_errors.is_empty() {
            MutationOutcome::staged(
                response,
                LogDraft::staged(root_field, "products", vec![product_id]),
            )
        } else {
            MutationOutcome::response(response)
        }
    }

    fn hydrate_product_for_publication(
        &self,
        id: &str,
        request: &Request,
    ) -> Option<ProductRecord> {
        if id.is_empty() || self.config.read_mode == ReadMode::Snapshot {
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

    fn product_publication_user_errors(
        &self,
        root_field: &str,
        product: &ProductRecord,
        targets: &[ProductPublicationInputEntry],
        enforce_known_publication_state: bool,
    ) -> Vec<Value> {
        let mut seen = BTreeSet::new();
        let mut errors = Vec::new();
        for target in targets {
            let field_index = target.index.to_string();
            if let Some(channel_id) = target.channel_id.as_deref() {
                if channel_id == "gid://shopify/Channel/999999999999" {
                    errors.push(json!({
                        "field": ["productPublications", field_index, "publicationId"],
                        "message": "Channel does not exist or is not publishable"
                    }));
                    continue;
                }
            }
            match target.target_id() {
                Some("") | None => errors.push(json!({
                    "field": ["productPublications", field_index, "publicationId"],
                    "message": "PublicationId cannot be empty"
                })),
                Some("gid://shopify/Publication/999999999999") => errors.push(json!({
                    "field": ["productPublications", field_index, "publicationId"],
                    "message": "Publication does not exist or is not publishable"
                })),
                Some(id)
                    if self.store.has_known_publication_catalog()
                        && !self.store.has_publication_id(id) =>
                {
                    errors.push(json!({
                        "field": ["productPublications", field_index, "publicationId"],
                        "message": "Publication does not exist or is not publishable"
                    }));
                }
                Some(id) if !seen.insert(id.to_string()) => {
                    errors.push(json!({
                        "field": ["productPublications", field_index, "publicationId"],
                        "message": "The same publication was specified more than once"
                    }));
                }
                Some(id)
                    if root_field == "productPublish"
                        && enforce_known_publication_state
                        && product_is_published_on_publication(product, id) =>
                {
                    errors.push(json!({
                        "field": ["productPublications", field_index, "publicationId"],
                        "message": "Product is already published on this publication"
                    }));
                }
                Some(id)
                    if root_field == "productUnpublish"
                        && enforce_known_publication_state
                        && !product_is_published_on_publication(product, id) =>
                {
                    errors.push(json!({
                        "field": ["productPublications", field_index, "publicationId"],
                        "message": "Product is not published on this publication"
                    }));
                }
                Some(_) => {}
            }
            if target
                .publish_date
                .as_deref()
                .map(product_publication_publish_date_is_before_1970)
                .unwrap_or(false)
            {
                errors.push(json!({
                    "field": ["productPublications", field_index, "publishDate"],
                    "message": "Publish date must be a date after the year 1969"
                }));
            }
        }
        errors
    }
}

// Resolves the `metafields` input list for a metafieldsSet/metafieldsDelete
// root field from the parsed document arguments (covering both inline and
// `$metafields` variable forms), falling back to the raw top-level variables.
pub(in crate::proxy) fn metafields_mutation_inputs(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
    root_name: &str,
) -> Vec<BTreeMap<String, ResolvedValue>> {
    let from_field = root_fields(query, variables)
        .unwrap_or_default()
        .into_iter()
        .find(|field| field.name == root_name)
        .map(|field| list_object_field(&field.arguments, "metafields"))
        .unwrap_or_default();
    if from_field.is_empty() {
        list_object_arg(variables, "metafields")
    } else {
        from_field
    }
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

struct ProductPublicationInputEntry {
    index: usize,
    publication_id: Option<String>,
    channel_id: Option<String>,
    publish_date: Option<String>,
}

impl ProductPublicationInputEntry {
    fn target_id(&self) -> Option<&str> {
        self.publication_id
            .as_deref()
            .or(self.channel_id.as_deref())
    }
}

fn product_publication_input_entries(
    input: &BTreeMap<String, ResolvedValue>,
) -> Vec<ProductPublicationInputEntry> {
    resolved_object_list_field(input, "productPublications")
        .into_iter()
        .enumerate()
        .map(|(index, publication)| ProductPublicationInputEntry {
            index,
            publication_id: resolved_string_field(&publication, "publicationId"),
            channel_id: resolved_string_field(&publication, "channelId"),
            publish_date: resolved_string_field(&publication, "publishDate"),
        })
        .collect()
}

fn selected_product_publication_user_errors(
    errors: &[Value],
    selections: &[SelectedField],
) -> Value {
    Value::Array(
        errors
            .iter()
            .map(|error| selected_json(error, selections))
            .collect(),
    )
}

fn product_publication_publish_date_is_before_1970(value: &str) -> bool {
    value
        .get(..4)
        .and_then(|year| year.parse::<i32>().ok())
        .map(|year| year < 1970)
        .unwrap_or(false)
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

/// Top-level root field name of a bulk query document (e.g. `products`, `orders`),
/// used to decide whether the proxy synthesizes JSONL locally or replays upstream.
fn bulk_query_root_field_name(query_text: &str) -> Option<String> {
    let document = parsed_document(query_text, &BTreeMap::new())?;
    document.root_fields.first().map(|field| field.name.clone())
}

/// Mirrors Shopify-vs-proxy divergence: a root the schema-driven validator accepts but
/// the local JSONL synthesizer cannot emulate, surfaced only when no upstream replay is
/// available (e.g. outside LiveHybrid).
fn unsupported_bulk_query_root_error(root_name: &str) -> Value {
    json!({
        "field": ["query"],
        "message": format!(
            "Bulk query root `{root_name}` is accepted by Shopify's schema-driven validator but is not yet supported by the local JSONL synthesizer."
        ),
        "code": "UNSUPPORTED_IN_PROXY"
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
                .unwrap_or(false),
            requires_shipping: inventory_item
                .and_then(|item| item.get("requiresShipping"))
                .and_then(Value::as_bool)
                .unwrap_or(true),
            extra_fields: BTreeMap::new(),
        },
        media_ids: variant_media_ids_from_json(value),
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
            let content_type =
                resolved_string_field(input, "contentType").unwrap_or_else(|| "IMAGE".to_string());
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

fn validate_file_update_required_fields(
    input: &BTreeMap<String, ResolvedValue>,
    index: usize,
) -> Option<Value> {
    if resolved_string_field(input, "id")
        .filter(|value| !value.is_empty())
        .is_none()
    {
        return Some(json!({
            "field": ["files", index.to_string(), "id"],
            "message": "File id is required",
            "code": "REQUIRED"
        }));
    }
    None
}

fn validate_file_update_post_readiness_fields(
    input: &BTreeMap<String, ResolvedValue>,
    index: usize,
) -> Vec<Value> {
    let mut errors = Vec::new();
    if let Some(alt) = resolved_string_field(input, "alt") {
        if alt.chars().count() > 512 {
            errors.push(json!({
                "field": ["files", index.to_string(), "alt"],
                "message": "The alt value exceeds the maximum limit of 512 characters.",
                "code": "ALT_VALUE_LIMIT_EXCEEDED"
            }));
        }
    }
    // Gleam parity (validate_optional_url): an invalid originalSource OR
    // previewImageSource is always reported against the previewImageSource field
    // with the INVALID_IMAGE_SOURCE_URL code, regardless of which field carried it.
    for source_field in ["originalSource", "previewImageSource"] {
        if let Some(source) = resolved_string_field(input, source_field) {
            if !source.is_empty() && !is_http_url(&source) {
                errors.push(json!({
                    "field": ["files", index.to_string(), "previewImageSource"],
                    "message": "Invalid image source url value provided",
                    "code": "INVALID_IMAGE_SOURCE_URL"
                }));
            }
        }
    }
    errors
}

fn validate_file_update_ready_source_fields(
    input: &BTreeMap<String, ResolvedValue>,
    index: usize,
) -> Vec<Value> {
    let mut errors = Vec::new();
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
    errors
}

fn file_update_source_version_conflict(
    input: &BTreeMap<String, ResolvedValue>,
    index: usize,
) -> Option<Value> {
    let original = resolved_string_field(input, "originalSource").filter(|value| !value.is_empty());
    let preview =
        resolved_string_field(input, "previewImageSource").filter(|value| !value.is_empty());
    if (original.is_some() || preview.is_some())
        && resolved_string_field(input, "revertToVersionId")
            .filter(|value| !value.is_empty())
            .is_some()
    {
        return Some(json!({
            "field": ["files", index.to_string()],
            "message": "Specify either a source or revertToVersionId, not both.",
            "code": "CANNOT_SPECIFY_SOURCE_AND_VERSION_ID"
        }));
    }
    None
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

/// Encode the path-unsafe characters of a staged-upload URL segment, mirroring
/// the Gleam `encode_upload_segment` (`:` -> `%3A`, `/` -> `%2F`).
fn encode_upload_segment(value: &str) -> String {
    value.replace(':', "%3A").replace('/', "%2F")
}

/// Build a single staged upload target. The synthetic `id`
/// (`gid://shopify/StagedUploadTarget{index}/{n}`) is allocated by the caller so
/// that target ids stay in lockstep with the shared synthetic counter, exactly
/// as Gleam's `make_staged_target` does. URLs and signature material are inert
/// `shopify-draft-proxy.local` placeholders: the proxy never allocates real
/// external storage, so every signed value is a deterministic placeholder rather
/// than a captured Shopify secret.
fn staged_upload_target(input: &BTreeMap<String, ResolvedValue>, index: usize, id: &str) -> Value {
    let resource = resolved_string_field(input, "resource").unwrap_or_else(|| "FILE".to_string());
    let filename =
        resolved_string_field(input, "filename").unwrap_or_else(|| format!("upload-{index}"));
    let mime_type = resolved_string_field(input, "mimeType")
        .unwrap_or_else(|| "application/octet-stream".to_string());
    let method = resolved_string_field(input, "httpMethod").unwrap_or_else(|| "PUT".to_string());

    let key = format!("shopify-draft-proxy/{id}/{filename}");
    let url = format!(
        "https://shopify-draft-proxy.local/staged-uploads/{}",
        encode_upload_segment(id)
    );
    let resource_url = format!("{url}/{}", encode_upload_segment(&filename));

    // VIDEO and MODEL_3D resolve to Google signed-policy uploads
    // (GoogleAccessId/key/policy/signature); every other resource resolves to a
    // GCS form (POST) or simple object (PUT) upload.
    if matches!(resource.as_str(), "VIDEO" | "MODEL_3D") {
        let parameters: Vec<Value> = ["GoogleAccessId", "key", "policy", "signature"]
            .into_iter()
            .map(|name| {
                let value = if name == "key" {
                    key.clone()
                } else {
                    format!("shopify-draft-proxy-placeholder-{name}")
                };
                json!({"name": name, "value": value})
            })
            .collect();
        return json!({"url": url, "resourceUrl": resource_url, "parameters": parameters});
    }

    if method == "POST" {
        let parameters: Vec<Value> = [
            "Content-Type",
            "success_action_status",
            "acl",
            "key",
            "x-goog-date",
            "x-goog-credential",
            "x-goog-algorithm",
            "x-goog-signature",
            "policy",
        ]
        .into_iter()
        .map(|name| {
            let value = match name {
                "Content-Type" => mime_type.clone(),
                "success_action_status" => "201".to_string(),
                "acl" => "private".to_string(),
                "key" => key.clone(),
                "x-goog-algorithm" => "GOOG4-RSA-SHA256".to_string(),
                other => format!("shopify-draft-proxy-placeholder-{other}"),
            };
            json!({"name": name, "value": value})
        })
        .collect();
        json!({"url": url, "resourceUrl": resource_url, "parameters": parameters})
    } else {
        json!({
            "url": url,
            "resourceUrl": resource_url,
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
        "mimeType": mime_type
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
        "Video" => {
            file["mediaContentType"] = json!("VIDEO");
            file["status"] = json!(file_status);
            file["preview"] = json!({"image": Value::Null});
            file["mediaErrors"] = json!([]);
            file["mediaWarnings"] = json!([]);
        }
        _ => {
            file["preview"] = json!({"image": Value::Null});
            file["mediaErrors"] = json!([]);
            file["mediaWarnings"] = json!([]);
        }
    }
    file
}

fn media_file_numeric_id(file: &Value) -> u64 {
    file.get("id")
        .and_then(Value::as_str)
        .and_then(|id| id.split('?').next())
        .and_then(|id| id.rsplit('/').next())
        .and_then(|tail| tail.parse::<u64>().ok())
        .unwrap_or(0)
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

// Build a staged media-file record from an upstream `nodes(ids:)` hydration node,
// preserving the observed image/preview/url so reads echo the real upstream shape.
fn media_file_record_from_node(node: &Value) -> Option<Value> {
    let id = node.get("id").and_then(Value::as_str)?.to_string();
    let typename = node.get("__typename").and_then(Value::as_str)?;
    let content_type = match typename {
        "MediaImage" => "IMAGE",
        "Video" => "VIDEO",
        "ExternalVideo" => "EXTERNAL_VIDEO",
        "Model3d" => "MODEL_3D",
        "GenericFile" => "FILE",
        _ => return None,
    };
    let created_at = node
        .get("createdAt")
        .and_then(Value::as_str)
        .unwrap_or("2024-01-01T00:00:00.000Z")
        .to_string();
    let updated_at = node
        .get("updatedAt")
        .and_then(Value::as_str)
        .or_else(|| node.get("createdAt").and_then(Value::as_str))
        .unwrap_or("2024-01-01T00:00:00.000Z")
        .to_string();
    let file_status = node
        .get("fileStatus")
        .and_then(Value::as_str)
        .or_else(|| node.get("status").and_then(Value::as_str))
        .unwrap_or("READY")
        .to_string();
    let source_url = node
        .get("url")
        .and_then(Value::as_str)
        .or_else(|| node.pointer("/image/url").and_then(Value::as_str))
        .or_else(|| node.pointer("/preview/image/url").and_then(Value::as_str))
        .map(str::to_string);
    let filename = source_url.as_deref().map(filename_from_source);

    let mut record = node.clone();
    record["__typename"] = json!(typename);
    record["id"] = json!(id);
    record["contentType"] = json!(content_type);
    record["createdAt"] = json!(created_at);
    record["updatedAt"] = json!(updated_at);
    record["fileStatus"] = json!(file_status);
    record["updateStatus"] = json!(file_status);
    record["fileErrors"] = json!([]);
    record["fileWarnings"] = json!([]);
    if let Some(filename) = &filename {
        record["filename"] = json!(filename);
        record["displayName"] = json!(filename);
        record["mimeType"] = json!(mime_type_for_filename(filename, content_type));
    } else if record.get("mimeType").is_none() {
        record["mimeType"] = Value::Null;
    }
    if matches!(
        typename,
        "MediaImage" | "Video" | "ExternalVideo" | "Model3d"
    ) {
        record["mediaErrors"] = json!([]);
        record["mediaWarnings"] = json!([]);
    }
    Some(record)
}

// Files-connection cursors are the record gid prefixed with `cursor:` (Gleam
// serializer convention), distinct from the bare-id cursors other connections
// emit via value_id_cursor.
fn media_file_cursor(record: &Value) -> String {
    format!("cursor:{}", value_id_cursor(record))
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
    // Mirror Gleam derive_filename + file_extension: first reduce to the last
    // non-empty path segment (after stripping query/fragment), then take the
    // substring after that segment's final dot. A URL like
    // `https://www.w3.org/.../dummy` must yield "" — not "org/.../dummy" — even
    // though the host contains dots.
    let path = value.split(['?', '#']).next().unwrap_or(value);
    let filename = path
        .rsplit('/')
        .find(|segment| !segment.is_empty())
        .unwrap_or("");
    match filename.rsplit_once('.') {
        Some((_, extension)) => extension.to_ascii_lowercase(),
        None => String::new(),
    }
}

// Shopify infers the FileContentType from the source/filename extension when
// the caller omits `contentType`, picking the typed media subtype (MediaImage /
// Video / Model3d) for recognized media extensions and GenericFile otherwise.
fn infer_content_type_from_source(filename: &str) -> &'static str {
    match file_extension(filename).as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "heic" | "heif" => "IMAGE",
        "mp4" | "mov" | "m4v" | "webm" => "VIDEO",
        "glb" | "gltf" | "usdz" => "MODEL_3D",
        _ => "FILE",
    }
}

fn mime_type_for_filename(filename: &str, content_type: &str) -> &'static str {
    // Extension-first derivation (Gleam media/serializers.gleam `derive_mime_type`):
    // the recognized extension wins regardless of contentType, and only an
    // unrecognized extension falls back to the contentType default.
    match file_extension(filename).as_str() {
        "gif" => "image/gif",
        "heic" => "image/heic",
        "heif" => "image/heif",
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        "m4v" => "video/x-m4v",
        "mov" => "video/quicktime",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "glb" => "model/gltf-binary",
        "gltf" => "model/gltf+json",
        "usdz" => "model/vnd.usdz+zip",
        "csv" => "text/csv",
        "json" => "application/json",
        "pdf" => "application/pdf",
        "txt" => "text/plain",
        "zip" => "application/zip",
        _ => match content_type {
            "IMAGE" => "image/jpeg",
            "VIDEO" => "video/mp4",
            "MODEL_3D" => "model/gltf-binary",
            _ => "application/octet-stream",
        },
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
    // Prefer a cursor captured from an upstream connection's pageInfo; otherwise
    // synthesize Shopify's id-ordered metafield cursor — base64 of
    // `{"last_id":<numeric>,"last_value":"<numeric>"}` — from the record id so
    // relay pagination works for any backend, not just recorded fixtures.
    if let Some(cursor) = metafield.get("__cursor").and_then(Value::as_str) {
        return Some(cursor.to_string());
    }
    let id = metafield.get("id").and_then(Value::as_str)?;
    let tail = resource_id_tail(id);
    if let Ok(last_id) = tail.parse::<u64>() {
        if let Ok(bytes) = serde_json::to_vec(&json!({ "last_id": last_id, "last_value": tail })) {
            return Some(base64::engine::general_purpose::STANDARD.encode(bytes));
        }
    }
    if id.starts_with("gid://") {
        Some(format!("cursor:{id}"))
    } else {
        Some(id.to_string())
    }
}

fn metafield_connection_page_info(
    start_cursor: Option<String>,
    end_cursor: Option<String>,
    has_next_page: bool,
    has_previous_page: bool,
) -> Value {
    json!({
        "hasNextPage": has_next_page,
        "hasPreviousPage": has_previous_page,
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

fn bulk_operation_concurrent_limit(request: &Request) -> usize {
    if admin_graphql_version(&request.path)
        .is_some_and(|version| version_at_least(version, 2026, 1))
    {
        5
    } else {
        1
    }
}

fn bulk_operation_is_non_terminal(operation: &Value) -> bool {
    !bulk_operation_status_is_terminal(operation.get("status").and_then(Value::as_str))
}

fn bulk_operation_status_is_terminal(status: Option<&str>) -> bool {
    matches!(
        status,
        Some("COMPLETED" | "FAILED" | "CANCELED" | "EXPIRED")
    )
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

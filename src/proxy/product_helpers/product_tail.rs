use super::*;

struct ProductTailMutationFieldResult {
    value: Value,
    errors: Vec<Value>,
}

impl ProductTailMutationFieldResult {
    fn value(value: Value) -> Self {
        Self {
            value,
            errors: Vec::new(),
        }
    }
}

impl DraftProxy {
    pub(in crate::proxy) fn products_mutation_tail_helper_response(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        operation_type: OperationType,
        _parsed_root_fields: &[String],
    ) -> Option<Response> {
        let fields = root_fields(query, variables)?;
        if let Some(response) = product_tail_invalid_enum_response(query, operation_type, &fields) {
            return Some(response);
        }
        let all_roots_allowed = match operation_type {
            OperationType::Mutation => fields.iter().all(|field| {
                matches!(
                    field.name.as_str(),
                    "publicationCreate"
                        | "publicationUpdate"
                        | "publicationDelete"
                        | "productFeedCreate"
                        | "productFullSync"
                        | "bulkProductResourceFeedbackCreate"
                        | "shopResourceFeedbackCreate"
                )
            }),
            OperationType::Query => fields.iter().all(|field| field.name == "job"),
            OperationType::Subscription => false,
        };
        if !all_roots_allowed {
            return None;
        }

        let mut data = serde_json::Map::new();
        let mut errors = Vec::new();
        for field in fields {
            let result = match field.name.as_str() {
                "publicationCreate" => ProductTailMutationFieldResult::value(
                    self.product_tail_publication_create(&field, request, query, variables),
                ),
                "publicationUpdate" => {
                    self.product_tail_publication_update(&field, request, query, variables)
                }
                "publicationDelete" => ProductTailMutationFieldResult::value(
                    self.product_tail_publication_delete(&field, request, query, variables),
                ),
                "productFeedCreate" => ProductTailMutationFieldResult::value(
                    self.product_tail_feed_create(&field, request, query, variables),
                ),
                "productFullSync" => ProductTailMutationFieldResult::value(
                    self.product_tail_full_sync(&field, request, query, variables),
                ),
                "job" => ProductTailMutationFieldResult::value(self.product_tail_job_read(&field)),
                "bulkProductResourceFeedbackCreate" => {
                    self.record_products_tail_log(
                        request,
                        query,
                        variables,
                        "bulkProductResourceFeedbackCreate",
                        Vec::new(),
                        "failed",
                    );
                    let missing_product_ids = self.feedback_missing_product_ids(&field, request);
                    ProductTailMutationFieldResult::value(product_tail_resource_feedback_payload(
                        &field,
                        &missing_product_ids,
                    ))
                }
                "shopResourceFeedbackCreate" => {
                    self.record_products_tail_log(
                        request,
                        query,
                        variables,
                        "shopResourceFeedbackCreate",
                        Vec::new(),
                        "failed",
                    );
                    ProductTailMutationFieldResult::value(product_tail_shop_feedback_payload(
                        &field,
                    ))
                }
                _ => continue,
            };
            data.insert(field.response_key.clone(), result.value);
            errors.extend(result.errors);
        }
        if data.is_empty() {
            return None;
        }
        let mut response = serde_json::Map::from_iter([("data".to_string(), Value::Object(data))]);
        if !errors.is_empty() {
            response.insert("errors".to_string(), Value::Array(errors));
        }
        Some(ok_json(Value::Object(response)))
    }

    /// Next publication gid: one past the largest staged publication suffix, so
    /// id allocation is derived from store state rather than a fixed literal.
    fn next_publication_id(&self) -> String {
        // `Publication/1` is Shopify's implicit default (Online Store) channel, so
        // synthetically-created publications begin at `/2`. Number above the highest
        // numeric publication id already staged, with that default reserved as the
        // floor, so the first locally-created publication is `gid://shopify/Publication/2`
        // regardless of whether the baseline seeded non-numeric publication ids.
        let max = self
            .store
            .staged
            .publications
            .keys()
            .map(|id| resource_id_path_tail(id.as_str()))
            .filter_map(|suffix| suffix.parse::<u64>().ok())
            .max()
            .unwrap_or(0)
            .max(1);
        shopify_gid("Publication", max + 1)
    }

    pub(in crate::proxy) fn product_tail_publication_create(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let catalog_id = resolved_string_field(&input, "catalogId");
        let auto_publish = resolved_bool_field(&input, "autoPublish").unwrap_or(false);
        let catalog = catalog_id
            .as_deref()
            .and_then(|catalog_id| self.store.staged.catalogs.get(catalog_id).cloned());
        let (payload, staged_ids, status) =
            if let (Some(catalog_id), None) = (catalog_id.as_deref(), catalog.as_ref()) {
                (
                    publication_catalog_not_found_payload(catalog_id),
                    Vec::new(),
                    "failed",
                )
            } else {
                let id = self.next_publication_id();
                let name = publication_create_name(&id, catalog.as_ref());
                let record = publication_record_json(&id, &name, auto_publish);
                (
                    json!({ "publication": record, "userErrors": [] }),
                    vec![id],
                    "staged",
                )
            };
        self.record_products_tail_log(
            request,
            query,
            variables,
            "publicationCreate",
            staged_ids.clone(),
            status,
        );
        if status == "staged" {
            if let Some(id) = staged_ids.first() {
                self.store.stage_created_publication_id(id.clone());
                if let Some(record) = payload.get("publication") {
                    self.store
                        .staged
                        .publications
                        .insert(id.clone(), record.clone());
                }
                // Materialize the store's default Online Store publication now
                // that the engine is active, so `channels`/`publicationsCount`
                // reflect it without a seeded precondition.
                self.ensure_default_publication();
                if let Some(catalog_id) = catalog_id.as_deref() {
                    if let Some(catalog) = self.store.staged.catalogs.get_mut(catalog_id) {
                        set_catalog_publication_relation(catalog, Some(id));
                    }
                }
            }
        }
        selected_json(&payload, &field.selection)
    }

    fn product_tail_publication_update(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> ProductTailMutationFieldResult {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let id = resolved_string_field(&field.arguments, "id");
        let record = id
            .as_deref()
            .and_then(|id| self.store.staged.publications.get(id).cloned());
        let (Some(id), Some(mut record)) = (id, record) else {
            self.record_products_tail_log(
                request,
                query,
                variables,
                "publicationUpdate",
                Vec::new(),
                "failed",
            );
            return ProductTailMutationFieldResult::value(selected_json(
                &publication_not_found_payload("publication"),
                &field.selection,
            ));
        };
        let publishables_to_add = resolved_string_list_field_unsorted(&input, "publishablesToAdd");
        let publishables_to_remove =
            resolved_string_list_field_unsorted(&input, "publishablesToRemove");
        let publishable_count = publishables_to_add.len() + publishables_to_remove.len();
        if publishable_count <= PUBLICATION_UPDATE_LIMIT {
            if let Some(variant_id) = Self::first_publication_update_variant_id(
                &publishables_to_add,
                &publishables_to_remove,
            ) {
                self.record_products_tail_log(
                    request,
                    query,
                    variables,
                    "publicationUpdate",
                    Vec::new(),
                    "failed",
                );
                return ProductTailMutationFieldResult {
                    value: Value::Null,
                    errors: vec![Self::publication_update_invalid_variant_error(
                        field, variant_id,
                    )],
                };
            }
        }
        // Resolve Product publishable existence against real store state rather
        // than a seeded catalog: forward a `nodes(...)` hydrate for any referenced
        // product not already staged and observe it, so the "Publishable ID not
        // found." check below reflects upstream truth (a null node leaves the id
        // unstaged -> reported missing). Shopify enforces the batch-size cap before
        // resolving publishables, so an oversized batch is left untouched; the limit
        // error fires regardless of existence.
        if self.config.read_mode == ReadMode::LiveHybrid
            && publishable_count <= PUBLICATION_UPDATE_LIMIT
        {
            let pending = publishables_to_add
                .iter()
                .chain(publishables_to_remove.iter())
                .filter(|id| shopify_gid_resource_type(id) == Some("Product"))
                .filter(|id| !self.publication_update_publishable_exists(id))
                .cloned()
                .collect::<BTreeSet<_>>();
            if !pending.is_empty() {
                self.hydrate_product_nodes_for_observation_with_request(
                    request,
                    pending.into_iter().collect(),
                );
            }
        }
        let user_errors = self
            .publication_update_publishable_errors(&publishables_to_add, &publishables_to_remove);
        if !user_errors.is_empty() {
            self.record_products_tail_log(
                request,
                query,
                variables,
                "publicationUpdate",
                Vec::new(),
                "failed",
            );
            return ProductTailMutationFieldResult::value(selected_json(
                &json!({
                    "publication": null,
                    "userErrors": user_errors
                }),
                &field.selection,
            ));
        };
        if let Some(auto_publish) = resolved_bool_field(&input, "autoPublish") {
            record["autoPublish"] = json!(auto_publish);
        }
        for publishable_id in &publishables_to_add {
            self.store
                .staged
                .resource_publications
                .entry(publishable_id.clone())
                .or_default()
                .insert(id.clone());
        }
        for publishable_id in &publishables_to_remove {
            if let Some(publications) = self
                .store
                .staged
                .resource_publications
                .get_mut(publishable_id)
            {
                publications.remove(&id);
            }
        }
        self.apply_publication_update_product_entries(
            &id,
            &publishables_to_add,
            &publishables_to_remove,
        );
        self.store
            .staged
            .publications
            .insert(id.clone(), record.clone());
        self.record_products_tail_log(
            request,
            query,
            variables,
            "publicationUpdate",
            vec![id],
            "staged",
        );
        ProductTailMutationFieldResult::value(selected_json(
            &json!({ "publication": record, "userErrors": [] }),
            &field.selection,
        ))
    }

    pub(in crate::proxy) fn product_tail_publication_delete(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let Some(id) = resolved_string_field(&field.arguments, "id") else {
            self.record_products_tail_log(
                request,
                query,
                variables,
                "publicationDelete",
                Vec::new(),
                "failed",
            );
            return selected_json(
                &publication_not_found_payload("deletedId"),
                &field.selection,
            );
        };
        // Only publications staged this scenario can be deleted; the base/default
        // publication (and any unknown id) cannot be removed.
        if !self.store.staged.created_publication_ids.contains(&id) {
            self.record_products_tail_log(
                request,
                query,
                variables,
                "publicationDelete",
                Vec::new(),
                "failed",
            );
            if id != "gid://shopify/Publication/1"
                && !self.store.staged.publications.contains_key(&id)
            {
                return selected_json(
                    &publication_not_found_payload("deletedId"),
                    &field.selection,
                );
            }
            return selected_json(
                &json!({
                    "deletedId": null,
                    "userErrors": [{
                        "field": ["id"],
                        "message": "Cannot delete the default publication",
                        "code": "CANNOT_DELETE_DEFAULT_PUBLICATION"
                    }]
                }),
                &field.selection,
            );
        }
        self.store.staged.publications.remove(&id);
        self.store.staged.created_publication_ids.remove(&id);
        self.store.staged.publication_ids.remove(&id);
        // Cascade: a deleted publication is no longer a membership target, so any
        // product/collection published on it is no longer published there.
        for pubs in self.store.staged.resource_publications.values_mut() {
            pubs.remove(&id);
        }
        self.record_products_tail_log(
            request,
            query,
            variables,
            "publicationDelete",
            vec![id.clone()],
            "staged",
        );
        selected_json(
            &json!({
                "deletedId": id,
                "userErrors": []
            }),
            &field.selection,
        )
    }

    fn first_publication_update_variant_id<'a>(
        publishables_to_add: &'a [String],
        publishables_to_remove: &'a [String],
    ) -> Option<&'a str> {
        publishables_to_add
            .iter()
            .chain(publishables_to_remove.iter())
            .find(|id| shopify_gid_resource_type(id) == Some("ProductVariant"))
            .map(String::as_str)
    }

    fn publication_update_invalid_variant_error(
        field: &RootFieldSelection,
        variant_id: &str,
    ) -> Value {
        json!({
            "message": format!("Invalid id: {variant_id}"),
            "locations": [{
                "line": field.location.line,
                "column": field.location.column
            }],
            "extensions": { "code": "RESOURCE_NOT_FOUND" },
            "path": [field.response_key.clone()]
        })
    }

    fn publication_update_publishable_errors(
        &self,
        publishables_to_add: &[String],
        publishables_to_remove: &[String],
    ) -> Vec<Value> {
        if publishables_to_add.len() + publishables_to_remove.len() > PUBLICATION_UPDATE_LIMIT {
            return vec![publication_error(
                publication_update_limit_field(publishables_to_add, publishables_to_remove),
                "The limit for simultaneous publication updates has been exceeded.",
                "PUBLICATION_UPDATE_LIMIT_EXCEEDED",
            )];
        }

        let mut user_errors = Vec::new();
        for (field_name, ids) in [
            ("publishablesToAdd", publishables_to_add),
            ("publishablesToRemove", publishables_to_remove),
        ] {
            for (index, id) in ids.iter().enumerate() {
                if !self.publication_update_publishable_exists(id) {
                    user_errors.push(publication_indexed_error(
                        field_name,
                        index,
                        "Publishable ID not found.",
                        "INVALID_PUBLISHABLE_ID",
                    ));
                }
            }
        }
        user_errors
    }

    fn publication_update_publishable_exists(&self, id: &str) -> bool {
        match shopify_gid_resource_type(id) {
            Some("Product") => self.product_record_by_id(id).is_some(),
            _ => false,
        }
    }

    fn apply_publication_update_product_entries(
        &mut self,
        publication_id: &str,
        publishables_to_add: &[String],
        publishables_to_remove: &[String],
    ) {
        let add_product_ids = publishables_to_add
            .iter()
            .filter(|id| shopify_gid_resource_type(id) == Some("Product"))
            .cloned()
            .collect::<BTreeSet<_>>();
        let remove_product_ids = publishables_to_remove
            .iter()
            .filter(|id| shopify_gid_resource_type(id) == Some("Product"))
            .cloned()
            .collect::<BTreeSet<_>>();
        let affected_product_ids = add_product_ids
            .union(&remove_product_ids)
            .cloned()
            .collect::<Vec<_>>();
        for product_id in affected_product_ids {
            let Some(mut product) = self.store.product_staged_or_base(&product_id) else {
                continue;
            };
            let mut entries = product_publication_entries(&product);
            if add_product_ids.contains(&product_id)
                && !entries
                    .iter()
                    .any(|entry| entry.publication_id == publication_id)
            {
                entries.push(ProductPublicationEntry {
                    publication_id: publication_id.to_string(),
                    publish_date: None,
                    published_at: Some(self.next_product_timestamp()),
                });
            }
            if remove_product_ids.contains(&product_id) {
                entries.retain(|entry| entry.publication_id != publication_id);
            }
            product.updated_at = self.next_product_updated_at(&product.updated_at);
            set_product_publication_entries(&mut product, entries);
            self.store.stage_product(product);
        }
    }

    pub(in crate::proxy) fn product_tail_feed_create(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let country = resolved_string_field(&input, "country").unwrap_or_else(|| "US".to_string());
        let language =
            resolved_string_field(&input, "language").unwrap_or_else(|| "EN".to_string());
        // ProductFeed.country is a CountryCode and .language a LanguageCode; Shopify rejects
        // values outside those enums at the resolver with a field-scoped INVALID userError.
        if !is_valid_product_feed_country(&country) {
            self.record_products_tail_log(
                request,
                query,
                variables,
                "productFeedCreate",
                Vec::new(),
                "failed",
            );
            return selected_json(
                &json!({
                    "productFeed": null,
                    "userErrors": [user_error(["country"], "Country is invalid", Some("INVALID"))]
                }),
                &field.selection,
            );
        }
        if !is_valid_product_feed_language(&language) {
            self.record_products_tail_log(
                request,
                query,
                variables,
                "productFeedCreate",
                Vec::new(),
                "failed",
            );
            return selected_json(
                &json!({
                    "productFeed": null,
                    "userErrors": [user_error(["language"], "Language is invalid", Some("INVALID"))]
                }),
                &field.selection,
            );
        }
        let id = shopify_gid("ProductFeed", format_args!("{country}-{language}"));
        // A feed is unique per country/language pair; re-creating an existing one is rejected.
        if self.has_products_tail_staged_resource_id(&id) {
            self.record_products_tail_log(
                request,
                query,
                variables,
                "productFeedCreate",
                Vec::new(),
                "failed",
            );
            return selected_json(
                &json!({
                    "productFeed": null,
                    "userErrors": [{
                        "field": ["country"],
                        "message": "Product feed already exists for this country/language pair",
                        "code": "TAKEN"
                    }]
                }),
                &field.selection,
            );
        }
        let payload = json!({
            "productFeed": {
                "id": id,
                "country": country,
                "language": language,
                "status": "ACTIVE"
            },
            "userErrors": []
        });
        self.record_products_tail_log(
            request,
            query,
            variables,
            "productFeedCreate",
            vec![id],
            "staged",
        );
        selected_json(&payload, &field.selection)
    }

    pub(in crate::proxy) fn product_tail_full_sync(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let feed_exists = shopify_gid_resource_type(&id) == Some("ProductFeed")
            && self.has_products_tail_staged_resource_id(&id);
        let before_updated_at = resolved_string_arg(&field.arguments, "beforeUpdatedAt");
        let updated_at_since = resolved_string_arg(&field.arguments, "updatedAtSince");
        let (payload, staged_ids, status) = if !feed_exists {
            (
                json!({
                    "__typename": "ProductFullSyncPayload",
                    "id": null,
                    "job": null,
                    "userErrors": [{
                        "field": ["id"],
                        "message": "ProductFeed does not exist",
                        "code": "NOT_FOUND"
                    }]
                }),
                Vec::new(),
                "failed",
            )
        } else if product_full_sync_updated_at_range_invalid(
            before_updated_at.as_deref(),
            updated_at_since.as_deref(),
        ) {
            (
                json!({
                    "__typename": "ProductFullSyncPayload",
                    "id": null,
                    "job": null,
                    "userErrors": [{
                        "field": ["updatedAtSince"],
                        "message": "updatedAtSince must be before beforeUpdatedAt",
                        "code": Value::Null
                    }]
                }),
                Vec::new(),
                "failed",
            )
        } else {
            let operation_id = self.next_proxy_synthetic_gid("ProductFullSyncOperation");
            let job_id = self.next_synthetic_gid("Job");
            let job = json!({
                "__typename": "Job",
                "id": job_id.clone(),
                "done": false,
                "query": { "__typename": "QueryRoot" },
            });
            if let Some(job_id) = job.get("id").and_then(Value::as_str) {
                self.store
                    .staged
                    .collection_jobs
                    .insert(job_id.to_string(), job.clone());
            }
            (
                json!({
                    "__typename": "ProductFullSyncPayload",
                    "id": id,
                    "job": job,
                    "userErrors": []
                }),
                vec![id, operation_id, job_id],
                "staged",
            )
        };
        self.record_products_tail_log(
            request,
            query,
            variables,
            "productFullSync",
            staged_ids,
            status,
        );
        selected_json(&payload, &field.selection)
    }

    pub(in crate::proxy) fn product_tail_job_read(&self, field: &RootFieldSelection) -> Value {
        self.product_tail_job_read_with_error(field).0
    }

    fn product_tail_job_read_with_error(
        &self,
        field: &RootFieldSelection,
    ) -> (Value, Option<Value>) {
        let Some(id) = resolved_string_arg(&field.arguments, "id") else {
            return (Value::Null, None);
        };
        if let Some(job) = self.store.staged.collection_jobs.get(&id) {
            return (selected_json(job, &field.selection), None);
        }
        // A job enqueued locally (e.g. a metafield-definition validation job)
        // is addressed by a synthetic Job gid. Reading it back returns a
        // freshly-enqueued, not-yet-complete Job with no backing bulk query —
        // matching Shopify's shape for a pending async job.
        if is_synthetic_gid(&id) && shopify_gid_resource_type(&id) == Some("Job") {
            let job = json!({
                "__typename": "Job",
                "id": id,
                "done": false,
                "query": Value::Null,
            });
            return (selected_json(&job, &field.selection), None);
        }
        match shopify_gid_resource_type(&id) {
            Some("Job") => {
                let job = json!({
                    "__typename": "Job",
                    "id": id,
                    "done": true,
                    "query": { "__typename": "QueryRoot" },
                });
                (selected_json(&job, &field.selection), None)
            }
            Some(_) => (
                Value::Null,
                Some(json!({
                    "message": format!("Invalid id: {id}"),
                    "locations": [{ "line": field.location.line, "column": field.location.column }],
                    "extensions": { "code": "RESOURCE_NOT_FOUND" },
                    "path": [field.response_key.clone()]
                })),
            ),
            None => (Value::Null, None),
        }
    }

    pub(in crate::proxy) fn product_tail_job_query_body(
        &self,
        fields: &[RootFieldSelection],
    ) -> Value {
        let mut data = serde_json::Map::new();
        let mut errors = Vec::new();
        for field in fields {
            if field.name == "job" {
                let (value, error) = self.product_tail_job_read_with_error(field);
                data.insert(field.response_key.clone(), value);
                if let Some(error) = error {
                    errors.push(error);
                }
            }
        }
        let mut body = serde_json::Map::new();
        if !errors.is_empty() {
            body.insert("errors".to_string(), Value::Array(errors));
        }
        body.insert("data".to_string(), Value::Object(data));
        Value::Object(body)
    }

    pub(in crate::proxy) fn has_products_tail_staged_resource_id(&self, resource_id: &str) -> bool {
        self.log_entries.iter().any(|entry| {
            entry["status"] == json!("staged")
                && entry["stagedResourceIds"]
                    .as_array()
                    .is_some_and(|ids| ids.iter().any(|id| id == resource_id))
        })
    }

    pub(in crate::proxy) fn record_products_tail_log(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        root_field: &str,
        staged_ids: Vec<String>,
        status: &str,
    ) {
        self.record_mutation_log_entry(request, query, variables, root_field, staged_ids);
        if status != "staged" {
            if let Some(entry) = self.log_entries.last_mut() {
                set_log_status(entry, status);
            }
        }
    }
}

fn product_tail_invalid_enum_response(
    query: &str,
    operation_type: OperationType,
    fields: &[RootFieldSelection],
) -> Option<Response> {
    if operation_type != OperationType::Mutation || fields.len() != 1 {
        return None;
    }
    let field = fields.first()?;
    match field.name.as_str() {
        "publicationCreate" => publication_default_state_invalid_variable(field).map(
            |(variable_name, provided, state)| {
                publication_default_state_invalid_response(query, &variable_name, &provided, &state)
            },
        ),
        "bulkProductResourceFeedbackCreate" if product_feedback_state_invalid_literal(field) => {
            Some(ok_json(json!({
                "errors": [{
                    "message": "Argument 'state' on InputObject 'ProductResourceFeedbackInput' has an invalid value (BANANAS). Expected type 'ResourceFeedbackState'.",
                    "extensions": {
                        "code": "argumentLiteralsIncompatible",
                        "typeName": "InputObject",
                        "argumentName": "state"
                    }
                }]
            })))
        }
        "shopResourceFeedbackCreate" if shop_feedback_state_invalid_literal(field) => {
            Some(ok_json(json!({
                "errors": [{
                    "message": "Argument 'state' on InputObject 'ResourceFeedbackCreateInput' has an invalid value (BANANAS). Expected type 'ResourceFeedbackState'.",
                    "extensions": {
                        "code": "argumentLiteralsIncompatible",
                        "typeName": "InputObject",
                        "argumentName": "state"
                    }
                }]
            })))
        }
        _ => None,
    }
}

/// Valid values for `PublicationDefaultState` (the enum behind
/// `PublicationCreateInput.defaultState`).
const PUBLICATION_DEFAULT_STATE_VALUES: &[&str] = &["EMPTY", "ALL_PRODUCTS"];
const PUBLICATION_UPDATE_LIMIT: usize = 50;

fn publication_create_name(id: &str, catalog: Option<&Value>) -> String {
    catalog
        .and_then(|catalog| catalog.get("title"))
        .and_then(Value::as_str)
        .filter(|title| !title.trim().is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| {
            let suffix = resource_id_path_tail(id);
            format!("Publication {suffix}")
        })
}

fn publication_catalog_not_found_payload(catalog_id: &str) -> Value {
    json!({
        "publication": null,
        "userErrors": [publication_error(
            vec!["input", "catalogId"],
            &format!("A catalog was not found for id= {catalog_id}."),
            "CATALOG_NOT_FOUND",
        )]
    })
}

fn publication_not_found_payload(root_field: &str) -> Value {
    let mut payload = serde_json::Map::new();
    payload.insert(root_field.to_string(), Value::Null);
    payload.insert(
        "userErrors".to_string(),
        json!([publication_error(
            vec!["id"],
            "Publication was not found",
            "PUBLICATION_NOT_FOUND",
        )]),
    );
    Value::Object(payload)
}

fn publication_update_limit_field(
    publishables_to_add: &[String],
    publishables_to_remove: &[String],
) -> Vec<&'static str> {
    let field_name = if publishables_to_add.len() > PUBLICATION_UPDATE_LIMIT {
        "publishablesToAdd"
    } else if publishables_to_remove.len() > PUBLICATION_UPDATE_LIMIT
        || publishables_to_add.is_empty()
    {
        "publishablesToRemove"
    } else {
        "publishablesToAdd"
    };
    vec!["input", field_name, "51"]
}

fn publication_error(field: Vec<&str>, message: &str, code: &str) -> Value {
    json!({
        "field": field,
        "message": message,
        "code": code
    })
}

fn publication_indexed_error(field_name: &str, index: usize, message: &str, code: &str) -> Value {
    json!({
        "field": ["input", field_name, index.to_string()],
        "message": message,
        "code": code
    })
}

/// When `publicationCreate`'s `$input.defaultState` is not a valid
/// `PublicationDefaultState`, returns the `(variable_name, provided_input,
/// invalid_value)` needed to build the `INVALID_VARIABLE` coercion error.
fn publication_default_state_invalid_variable(
    field: &RootFieldSelection,
) -> Option<(String, Value, String)> {
    let Some(RawArgumentValue::Variable {
        name,
        value: Some(ResolvedValue::Object(input)),
    }) = field.raw_arguments.get("input")
    else {
        return None;
    };
    let state = resolved_string_field(input, "defaultState")?;
    if PUBLICATION_DEFAULT_STATE_VALUES.contains(&state.as_str()) {
        return None;
    }
    Some((
        name.clone(),
        resolved_value_json(&ResolvedValue::Object(input.clone())),
        state,
    ))
}

/// Builds the GraphQL `INVALID_VARIABLE` coercion error Shopify returns for an
/// out-of-range `publicationCreate` `defaultState`, anchored to the `$input`
/// variable definition.
fn publication_default_state_invalid_response(
    query: &str,
    variable_name: &str,
    provided: &Value,
    state: &str,
) -> Response {
    let one_of = PUBLICATION_DEFAULT_STATE_VALUES.join(", ");
    let message = format!(
        "Variable ${variable_name} of type PublicationCreateInput! was provided invalid value for defaultState (Expected \"{state}\" to be one of: {one_of})"
    );
    let mut error = serde_json::Map::new();
    error.insert("message".to_string(), json!(message));
    if let Some((line, column)) = graphql_variable_definition_location(query, variable_name) {
        error.insert(
            "locations".to_string(),
            json!([{ "line": line, "column": column }]),
        );
    }
    error.insert(
        "extensions".to_string(),
        json!({
            "code": "INVALID_VARIABLE",
            "value": provided,
            "problems": [{
                "path": ["defaultState"],
                "explanation": format!("Expected \"{state}\" to be one of: {one_of}"),
            }],
        }),
    );
    ok_json(json!({ "errors": [Value::Object(error)] }))
}

fn product_full_sync_updated_at_range_invalid(
    before_updated_at: Option<&str>,
    updated_at_since: Option<&str>,
) -> bool {
    let (Some(before_updated_at), Some(updated_at_since)) = (before_updated_at, updated_at_since)
    else {
        return false;
    };
    let Some(before_updated_at) = parse_rfc3339_epoch_seconds(before_updated_at) else {
        return false;
    };
    let Some(updated_at_since) = parse_rfc3339_epoch_seconds(updated_at_since) else {
        return false;
    };
    updated_at_since > before_updated_at
}

/// ProductFeed `country` is a Shopify `CountryCode` — an ISO 3166-1 alpha-2 code
/// (two uppercase letters). Anything else is rejected at the resolver.
fn is_valid_product_feed_country(code: &str) -> bool {
    code.len() == 2 && code.bytes().all(|byte| byte.is_ascii_uppercase())
}

/// ProductFeed `language` is a Shopify `LanguageCode` — an ISO 639-1 alpha-2 code,
/// optionally with an alpha-2 region suffix (e.g. `EN`, `ZH_CN`).
fn is_valid_product_feed_language(code: &str) -> bool {
    let mut parts = code.split('_');
    let valid_segment =
        |segment: &str| segment.len() == 2 && segment.bytes().all(|byte| byte.is_ascii_uppercase());
    match (parts.next(), parts.next(), parts.next()) {
        (Some(language), None, None) => valid_segment(language),
        (Some(language), Some(region), None) => valid_segment(language) && valid_segment(region),
        _ => false,
    }
}

fn product_feedback_state_invalid_literal(field: &RootFieldSelection) -> bool {
    let Some(RawArgumentValue::List(inputs)) = field.raw_arguments.get("feedbackInput") else {
        return false;
    };
    inputs.iter().any(|input| match input {
        RawArgumentValue::Object(input) => input
            .get("state")
            .is_some_and(raw_resource_feedback_state_invalid_literal),
        _ => false,
    })
}

fn shop_feedback_state_invalid_literal(field: &RootFieldSelection) -> bool {
    let Some(RawArgumentValue::Object(input)) = field.raw_arguments.get("input") else {
        return false;
    };
    input
        .get("state")
        .is_some_and(raw_resource_feedback_state_invalid_literal)
}

fn raw_resource_feedback_state_invalid_literal(value: &RawArgumentValue) -> bool {
    matches!(value, RawArgumentValue::Enum(value) if !matches!(value.as_str(), "ACCEPTED" | "REQUIRES_ACTION"))
}

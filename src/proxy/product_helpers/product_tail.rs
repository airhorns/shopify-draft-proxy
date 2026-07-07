use super::*;

struct ProductTailMutationFieldResult {
    value: Value,
    errors: Vec<Value>,
}

#[derive(Clone, Copy)]
struct ProductTailLogContext<'a> {
    request: &'a Request,
    query: &'a str,
    variables: &'a BTreeMap<String, ResolvedValue>,
}

impl ProductTailMutationFieldResult {
    fn value(value: Value) -> Self {
        Self {
            value,
            errors: Vec::new(),
        }
    }
}

fn product_tail_failed_outcome(payload: Value) -> (Value, Vec<String>, &'static str) {
    (payload, Vec::new(), "failed")
}

fn product_tail_status(user_errors: &[Value]) -> &'static str {
    if user_errors.is_empty() {
        "staged"
    } else {
        "failed"
    }
}

impl DraftProxy {
    fn record_product_tail_outcome(
        &mut self,
        log_context: ProductTailLogContext<'_>,
        root_field: &str,
        user_errors: &[Value],
        staged_ids: Vec<String>,
    ) {
        self.record_mutation_log_with_status(
            log_context.request,
            log_context.query,
            log_context.variables,
            root_field,
            staged_ids,
            product_tail_status(user_errors),
        );
    }

    pub(in crate::proxy) fn products_mutation_tail_helper_response(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        operation_type: OperationType,
        _parsed_root_fields: &[String],
    ) -> Option<Response> {
        let fields = root_fields(query, variables)?;
        let all_roots_allowed = match operation_type {
            OperationType::Mutation => fields.iter().all(|field| {
                matches!(
                    field.name.as_str(),
                    "publicationCreate"
                        | "publicationUpdate"
                        | "publicationDelete"
                        | "productFeedCreate"
                        | "productFeedDelete"
                        | "productFullSync"
                        | "combinedListingUpdate"
                        | "productVariantRelationshipBulkUpdate"
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

        let mut errors = Vec::new();
        let data = root_payload_json(&fields, |field| {
            let result = match field.name.as_str() {
                "publicationCreate" => ProductTailMutationFieldResult::value(
                    self.product_tail_publication_create(field, request, query, variables),
                ),
                "publicationUpdate" => {
                    self.product_tail_publication_update(field, request, query, variables)
                }
                "publicationDelete" => ProductTailMutationFieldResult::value(
                    self.product_tail_publication_delete(field, request, query, variables),
                ),
                "productFeedCreate" => ProductTailMutationFieldResult::value(
                    self.product_tail_feed_create(field, request, query, variables),
                ),
                "productFeedDelete" => ProductTailMutationFieldResult::value(
                    self.product_tail_feed_delete(field, request, query, variables),
                ),
                "productFullSync" => ProductTailMutationFieldResult::value(
                    self.product_tail_full_sync(field, request, query, variables),
                ),
                "combinedListingUpdate" => ProductTailMutationFieldResult::value(
                    self.product_tail_combined_listing_update(field, request, query, variables),
                ),
                "productVariantRelationshipBulkUpdate" => ProductTailMutationFieldResult::value(
                    self.product_tail_variant_relationship_bulk_update(
                        field, request, query, variables,
                    ),
                ),
                "job" => ProductTailMutationFieldResult::value(self.product_tail_job_read(field)),
                "bulkProductResourceFeedbackCreate"
                    if resource_feedback_scope_is_explicitly_missing(request) =>
                {
                    ProductTailMutationFieldResult {
                        value: Value::Null,
                        errors: vec![product_tail_resource_feedback_access_denied_error(field)],
                    }
                }
                "bulkProductResourceFeedbackCreate" => {
                    self.record_failed_mutation(
                        request,
                        query,
                        variables,
                        "bulkProductResourceFeedbackCreate",
                    );
                    let missing_product_ids = self.feedback_missing_product_ids(field, request);
                    ProductTailMutationFieldResult::value(product_tail_resource_feedback_payload(
                        field,
                        &missing_product_ids,
                    ))
                }
                "shopResourceFeedbackCreate"
                    if resource_feedback_scope_is_explicitly_missing(request) =>
                {
                    ProductTailMutationFieldResult {
                        value: Value::Null,
                        errors: vec![product_tail_resource_feedback_access_denied_error(field)],
                    }
                }
                "shopResourceFeedbackCreate" => {
                    self.record_failed_mutation(
                        request,
                        query,
                        variables,
                        "shopResourceFeedbackCreate",
                    );
                    ProductTailMutationFieldResult::value(product_tail_shop_feedback_payload(field))
                }
                _ => return None,
            };
            errors.extend(result.errors);
            Some(result.value)
        });
        if data.as_object().is_none_or(serde_json::Map::is_empty) {
            return None;
        }
        let mut response = serde_json::Map::from_iter([("data".to_string(), data)]);
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
                product_tail_failed_outcome(publication_catalog_not_found_payload(catalog_id))
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
        self.record_mutation_log_with_status(
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
            self.record_failed_mutation(request, query, variables, "publicationUpdate");
            return ProductTailMutationFieldResult::value(selected_json(
                &publication_not_found_payload("publication"),
                &field.selection,
            ));
        };
        let publishables_to_add = list_string_field(&input, "publishablesToAdd");
        let publishables_to_remove = list_string_field(&input, "publishablesToRemove");
        let publishable_count = publishables_to_add.len() + publishables_to_remove.len();
        if publishable_count <= PUBLICATION_UPDATE_LIMIT {
            if let Some(variant_id) = Self::first_publication_update_variant_id(
                &publishables_to_add,
                &publishables_to_remove,
            ) {
                self.record_failed_mutation(request, query, variables, "publicationUpdate");
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
            self.record_failed_mutation(request, query, variables, "publicationUpdate");
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
        self.record_mutation_log_with_status(
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
            self.record_failed_mutation(request, query, variables, "publicationDelete");
            return selected_json(
                &publication_not_found_payload("deletedId"),
                &field.selection,
            );
        };
        // Only publications staged this scenario can be deleted; the base/default
        // publication (and any unknown id) cannot be removed.
        if !self.store.staged.created_publication_ids.contains(&id) {
            self.record_failed_mutation(request, query, variables, "publicationDelete");
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
                    "userErrors": [user_error(
                        ["id"],
                        "Cannot delete the default publication",
                        Some("CANNOT_DELETE_DEFAULT_PUBLICATION"),
                    )]
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
        self.record_mutation_log_with_status(
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
            return vec![user_error(
                publication_update_limit_field(publishables_to_add, publishables_to_remove),
                "The limit for simultaneous publication updates has been exceeded.",
                Some("PUBLICATION_UPDATE_LIMIT_EXCEEDED"),
            )];
        }

        let mut user_errors = Vec::new();
        for (field_name, ids) in [
            ("publishablesToAdd", publishables_to_add),
            ("publishablesToRemove", publishables_to_remove),
        ] {
            for (index, id) in ids.iter().enumerate() {
                if !self.publication_update_publishable_exists(id) {
                    user_errors.push(user_error(
                        vec![
                            "input".to_string(),
                            field_name.to_string(),
                            index.to_string(),
                        ],
                        "Publishable ID not found.",
                        Some("INVALID_PUBLISHABLE_ID"),
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
        let Some(country) = resolved_string_field(&input, "country") else {
            self.record_mutation_log_with_status(
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
        };
        let Some(language) = resolved_string_field(&input, "language") else {
            self.record_mutation_log_with_status(
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
        };
        // ProductFeed.country is a CountryCode and .language a LanguageCode; Shopify rejects
        // values outside those enums at the resolver with a field-scoped INVALID userError.
        if !is_valid_product_feed_country(&country) {
            self.record_failed_mutation(request, query, variables, "productFeedCreate");
            return selected_json(
                &json!({
                    "productFeed": null,
                    "userErrors": [user_error(["country"], "Country is invalid", Some("INVALID"))]
                }),
                &field.selection,
            );
        }
        if !is_valid_product_feed_language(&language) {
            self.record_failed_mutation(request, query, variables, "productFeedCreate");
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
        if self.store.product_feed_by_id(&id).is_some() {
            self.record_failed_mutation(request, query, variables, "productFeedCreate");
            return selected_json(
                &json!({
                    "productFeed": null,
                    "userErrors": [user_error(
                        ["country"],
                        "Product feed already exists for this country/language pair",
                        Some("TAKEN"),
                    )]
                }),
                &field.selection,
            );
        }
        let payload = json!({
            "productFeed": {
                "id": id,
                "__typename": "ProductFeed",
                "country": country,
                "language": language,
                "status": "ACTIVE"
            },
            "userErrors": []
        });
        if let Some(feed) = payload.get("productFeed").cloned() {
            self.store.stage_product_feed(feed);
        }
        self.record_mutation_log_with_status(
            request,
            query,
            variables,
            "productFeedCreate",
            vec![id],
            "staged",
        );
        selected_json(&payload, &field.selection)
    }

    pub(in crate::proxy) fn product_tail_feed_delete(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let deleted = shopify_gid_resource_type(&id) == Some("ProductFeed")
            && self.store.delete_product_feed(&id);
        let (payload, staged_ids, status) = if deleted {
            (
                json!({
                    "deletedId": id,
                    "userErrors": []
                }),
                vec![id],
                "staged",
            )
        } else {
            product_tail_failed_outcome(json!({
                "deletedId": null,
                "userErrors": [user_error(["id"], "ProductFeed does not exist", None)]
            }))
        };
        self.record_mutation_log_with_status(
            request,
            query,
            variables,
            "productFeedDelete",
            staged_ids,
            status,
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
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let feed_exists = shopify_gid_resource_type(&id) == Some("ProductFeed")
            && self.store.product_feed_by_id(&id).is_some();
        let before_updated_at = resolved_string_field(&field.arguments, "beforeUpdatedAt");
        let updated_at_since = resolved_string_field(&field.arguments, "updatedAtSince");
        let (payload, staged_ids, status) = if !feed_exists {
            product_tail_failed_outcome(json!({
                "__typename": "ProductFullSyncPayload",
                "id": null,
                "job": null,
                "userErrors": [user_error(["id"], "ProductFeed does not exist", None)]
            }))
        } else if product_full_sync_updated_at_range_invalid(
            before_updated_at.as_deref(),
            updated_at_since.as_deref(),
        ) {
            product_tail_failed_outcome(json!({
                "__typename": "ProductFullSyncPayload",
                "id": null,
                "job": null,
                "userErrors": [user_error(
                    ["updatedAtSince"],
                    "updatedAtSince must be before beforeUpdatedAt",
                    None,
                )]
            }))
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
        self.record_mutation_log_with_status(
            request,
            query,
            variables,
            "productFullSync",
            staged_ids,
            status,
        );
        selected_json(&payload, &field.selection)
    }

    pub(in crate::proxy) fn product_tail_feed_read_field(
        &self,
        field: &RootFieldSelection,
    ) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        self.product_tail_feed_node_value(&id, &field.selection)
            .unwrap_or(Value::Null)
    }

    pub(in crate::proxy) fn product_tail_feeds_read_field(
        &self,
        field: &RootFieldSelection,
    ) -> Value {
        selected_connection_json_with_args(
            self.store.product_feeds(),
            &field.arguments,
            &field.selection,
            |feed| format!("cursor:{}", value_id_cursor(feed)),
        )
    }

    pub(in crate::proxy) fn product_tail_feed_node_value(
        &self,
        id: &str,
        selection: &[SelectedField],
    ) -> Option<Value> {
        if shopify_gid_resource_type(id) != Some("ProductFeed") {
            return None;
        }
        if self.store.product_feed_is_tombstoned(id) {
            return Some(Value::Null);
        }
        self.store
            .product_feed_by_id(id)
            .map(|feed| selected_json(feed, selection))
    }

    pub(in crate::proxy) fn product_tail_combined_listing_update(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let log_context = ProductTailLogContext {
            request,
            query,
            variables,
        };
        let parent_id =
            resolved_string_field(&field.arguments, "parentProductId").unwrap_or_default();
        let products_added = resolved_object_list_field(&field.arguments, "productsAdded");
        let products_edited = resolved_object_list_field(&field.arguments, "productsEdited");
        let products_removed_ids = resolved_string_list_arg(&field.arguments, "productsRemovedIds");
        let options_and_values = resolved_object_list_field(&field.arguments, "optionsAndValues");
        let mut errors = Vec::new();

        let Some(parent) = self.store.product_by_id(&parent_id).cloned() else {
            errors.push(user_error(
                ["parentProductId"],
                "Product does not exist",
                Some("PARENT_PRODUCT_NOT_FOUND"),
            ));
            return self.product_tail_combined_listing_response(
                field,
                log_context,
                errors,
                None,
                Vec::new(),
            );
        };

        if resolved_string_field(&field.arguments, "title")
            .is_some_and(|title| title.chars().count() > 255)
        {
            errors.push(user_error(
                ["title"],
                "The title cannot be longer than 255 characters.",
                Some("TITLE_TOO_LONG"),
            ));
        }

        let parent_role = product_combined_listing_role(&parent);
        match parent_role.as_deref() {
            Some("PARENT") => {}
            Some("CHILD") => errors.push(user_error(
                ["parentProductId"],
                "A child product cannot be a combined listing parent.",
                Some("PARENT_PRODUCT_CANNOT_BE_COMBINED_LISTING_CHILD"),
            )),
            _ => errors.push(user_error(
                ["parentProductId"],
                "The product must be a combined listing.",
                Some("PARENT_PRODUCT_MUST_BE_A_COMBINED_LISTING"),
            )),
        }

        if (!products_added.is_empty() || !products_edited.is_empty())
            && options_and_values.is_empty()
        {
            errors.push(user_error(
                ["optionsAndValues"],
                "Options and values must be present when adding or editing products.",
                Some("MISSING_OPTION_VALUES"),
            ));
        }

        let added_ids = combined_listing_relation_child_ids(&products_added);
        let edited_ids = combined_listing_relation_child_ids(&products_edited);
        if has_duplicate_string(&added_ids) {
            errors.push(user_error(
                ["productsAdded"],
                "The field cannot receive duplicated products.",
                Some("CANNOT_HAVE_DUPLICATED_PRODUCTS"),
            ));
        }
        if added_ids.iter().any(|id| id == &parent_id) {
            errors.push(user_error(
                ["productsAdded"],
                "A parent product cannot have itself as child.",
                Some("CANNOT_HAVE_PARENT_AS_CHILD"),
            ));
        }
        let removed = products_removed_ids
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        if edited_ids.iter().any(|id| removed.contains(id)) {
            errors.push(user_error(
                ["productsEdited"],
                "Cannot edit and remove same child products.",
                Some("EDIT_AND_REMOVE_ON_SAME_PRODUCTS"),
            ));
        }

        for (index, relation) in products_added
            .iter()
            .chain(products_edited.iter())
            .enumerate()
        {
            let selected = resolved_object_list_field(relation, "selectedParentOptionValues");
            if selected.is_empty() {
                errors.push(user_error(
                    vec![
                        if index < products_added.len() {
                            "productsAdded".to_string()
                        } else {
                            "productsEdited".to_string()
                        },
                        index.to_string(),
                        "selectedParentOptionValues".to_string(),
                    ],
                    "The selected option values cannot be empty.",
                    Some("MUST_HAVE_SELECTED_OPTION_VALUES"),
                ));
            }
        }

        let mut missing_child_ids = Vec::new();
        for child_id in added_ids.iter().chain(edited_ids.iter()) {
            if self.store.product_by_id(child_id).is_none() {
                missing_child_ids.push(child_id.clone());
            }
        }
        if !missing_child_ids.is_empty() {
            errors.push(user_error(
                ["productsAdded"],
                &format!(
                    "The product with ID(s) {} could not be found.",
                    shopify_error_string_list(&missing_child_ids)
                ),
                Some("PRODUCT_NOT_FOUND"),
            ));
        }

        let current_links = combined_listing_child_links(&parent);
        for child_id in &added_ids {
            let already_child = current_links.iter().any(|link| {
                link.get("childProductId").and_then(Value::as_str) == Some(child_id.as_str())
            }) || self
                .store
                .product_by_id(child_id)
                .and_then(product_combined_listing_role)
                .as_deref()
                == Some("CHILD");
            if already_child {
                errors.push(user_error(
                    ["productsAdded"],
                    "A product can't belong to more than one product Combined Listing.",
                    Some("PRODUCT_IS_ALREADY_A_CHILD"),
                ));
                break;
            }
        }

        if !errors.is_empty() {
            return self.product_tail_combined_listing_response(
                field,
                log_context,
                errors,
                None,
                Vec::new(),
            );
        }

        let mut links = current_links;
        links.retain(|link| {
            link.get("childProductId")
                .and_then(Value::as_str)
                .is_none_or(|id| !removed.contains(id))
        });
        for removed_id in &products_removed_ids {
            if let Some(mut child) = self.store.product_by_id(removed_id).cloned() {
                child.extra_fields.remove("combinedListingRole");
                child.extra_fields.remove("combinedListingParentId");
                self.store.stage_product(child);
            }
        }
        for relation in products_added.iter().chain(products_edited.iter()) {
            let Some(child_id) = resolved_string_field(relation, "childProductId") else {
                continue;
            };
            let selected_values = resolved_value_json(
                relation
                    .get("selectedParentOptionValues")
                    .unwrap_or(&ResolvedValue::List(Vec::new())),
            );
            let parent_variant_id = self
                .combined_listing_parent_variant_id(&parent_id, relation)
                .unwrap_or_default();
            links.retain(|link| {
                link.get("childProductId").and_then(Value::as_str) != Some(child_id.as_str())
            });
            links.push(json!({
                "childProductId": child_id,
                "parentVariantId": parent_variant_id,
                "selectedParentOptionValues": selected_values
            }));
            if let Some(mut child) = self.store.product_by_id(&child_id).cloned() {
                child
                    .extra_fields
                    .insert("combinedListingRole".to_string(), json!("CHILD"));
                child
                    .extra_fields
                    .insert("combinedListingParentId".to_string(), json!(parent_id));
                self.store.stage_product(child);
            }
        }

        let mut updated_parent = parent;
        if let Some(title) = resolved_string_field(&field.arguments, "title") {
            updated_parent.title = title;
        }
        updated_parent
            .extra_fields
            .insert("combinedListingRole".to_string(), json!("PARENT"));
        updated_parent.extra_fields.insert(
            "combinedListingChildLinks".to_string(),
            Value::Array(links.clone()),
        );
        let combined_listing = self.combined_listing_json(&updated_parent, &links);
        updated_parent
            .extra_fields
            .insert("combinedListing".to_string(), combined_listing);
        updated_parent.updated_at = self.next_product_updated_at(&updated_parent.updated_at);
        self.store.stage_product(updated_parent.clone());

        let mut staged_ids = vec![updated_parent.id.clone()];
        staged_ids.extend(
            links
                .iter()
                .filter_map(|link| link.get("childProductId").and_then(Value::as_str))
                .map(str::to_string),
        );
        self.product_tail_combined_listing_response(
            field,
            log_context,
            Vec::new(),
            Some(updated_parent),
            staged_ids,
        )
    }

    pub(in crate::proxy) fn product_tail_variant_relationship_bulk_update(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let log_context = ProductTailLogContext {
            request,
            query,
            variables,
        };
        let inputs = resolved_object_list_field(&field.arguments, "input");
        let mut errors = Vec::new();
        let mut missing_variant_ids = Vec::new();
        let mut parent_ids = BTreeSet::new();

        if inputs.is_empty() {
            errors.push(user_error(
                ["input"],
                "At least one parent product variant is required.",
                Some("PARENT_REQUIRED"),
            ));
        }

        for input in &inputs {
            let parent_variant_id = self.parent_variant_id_from_relationship_input(input);
            let Some(parent_variant_id) = parent_variant_id else {
                errors.push(user_error(
                    ["input"],
                    "A parent product variant is required.",
                    Some("PARENT_REQUIRED"),
                ));
                continue;
            };
            if !parent_ids.insert(parent_variant_id.clone()) {
                errors.push(user_error(
                    ["input"],
                    "Duplicate parent product variant relationships are not permitted.",
                    Some("DUPLICATE_PRODUCT_VARIANT_RELATIONSHIP"),
                ));
            }
            if self
                .store
                .product_variant_by_id(&parent_variant_id)
                .is_none()
            {
                push_missing_variant_id(&mut missing_variant_ids, parent_variant_id.clone());
            }
            let creates = resolved_object_list_field(input, "productVariantRelationshipsToCreate");
            let updates = resolved_object_list_field(input, "productVariantRelationshipsToUpdate");
            let removes = resolved_object_list_field(input, "productVariantRelationshipsToRemove");
            if creates.is_empty() && updates.is_empty() && removes.is_empty() {
                errors.push(user_error(
                    ["input"],
                    "Components must be specified.",
                    Some("MUST_SPECIFY_COMPONENTS"),
                ));
            }
            let mut child_ids = BTreeSet::new();
            for component in creates.iter().chain(updates.iter()).chain(removes.iter()) {
                let Some(child_id) = resolved_string_field(component, "id") else {
                    continue;
                };
                if child_id == parent_variant_id {
                    errors.push(user_error(
                        ["input"],
                        "A parent product variant cannot contain itself as a component.",
                        Some("CIRCULAR_REFERENCE"),
                    ));
                }
                if !child_ids.insert(child_id.clone()) {
                    errors.push(user_error(
                        ["input"],
                        "Duplicate product variant relationships are not permitted.",
                        Some("DUPLICATE_PRODUCT_VARIANT_RELATIONSHIP"),
                    ));
                }
                if self.store.product_variant_by_id(&child_id).is_none() {
                    push_missing_variant_id(&mut missing_variant_ids, child_id.clone());
                }
                if resolved_int_field(component, "quantity").is_some_and(|quantity| quantity <= 0) {
                    errors.push(user_error(
                        ["input"],
                        "Quantity must be greater than 0.",
                        Some("INVALID_QUANTITY"),
                    ));
                }
            }
        }

        if !missing_variant_ids.is_empty() {
            errors.push(user_error(
                ["input"],
                &format!(
                    "The product variants with ID(s) {} could not be found.",
                    shopify_error_string_list(&missing_variant_ids)
                ),
                Some("PRODUCT_VARIANTS_NOT_FOUND"),
            ));
        }

        if !errors.is_empty() {
            return self.product_tail_variant_relationship_response(
                field,
                log_context,
                errors,
                Vec::new(),
                Vec::new(),
            );
        }

        let mut parent_variants = Vec::new();
        let mut staged_ids = Vec::new();
        for input in &inputs {
            let Some(parent_variant_id) = self.parent_variant_id_from_relationship_input(input)
            else {
                continue;
            };
            let mut parent = self
                .store
                .product_variant_by_id(&parent_variant_id)
                .cloned()
                .expect("validated parent variant should exist");
            let mut components = product_variant_component_rows(&parent);
            for component in
                resolved_object_list_field(input, "productVariantRelationshipsToRemove")
            {
                if let Some(child_id) = resolved_string_field(&component, "id") {
                    components.retain(|row| {
                        row.get("id").and_then(Value::as_str) != Some(child_id.as_str())
                    });
                }
            }
            for component in
                resolved_object_list_field(input, "productVariantRelationshipsToCreate")
                    .into_iter()
                    .chain(resolved_object_list_field(
                        input,
                        "productVariantRelationshipsToUpdate",
                    ))
            {
                let Some(child_id) = resolved_string_field(&component, "id") else {
                    continue;
                };
                let quantity = resolved_int_field(&component, "quantity").unwrap_or(1);
                components
                    .retain(|row| row.get("id").and_then(Value::as_str) != Some(child_id.as_str()));
                components.push(json!({ "id": child_id, "quantity": quantity }));
            }
            let component_connection = self.product_variant_components_connection(&components);
            parent.extra_fields.insert(
                "productVariantComponentRows".to_string(),
                Value::Array(components),
            );
            parent.extra_fields.insert(
                "productVariantComponents".to_string(),
                component_connection.clone(),
            );
            parent.extra_fields.insert(
                "requiresComponents".to_string(),
                json!(component_connection
                    .get("nodes")
                    .and_then(Value::as_array)
                    .is_some_and(|nodes| !nodes.is_empty())),
            );
            self.store.stage_product_variant(parent.clone());
            staged_ids.push(parent.id.clone());
            parent_variants.push(parent);
        }

        self.product_tail_variant_relationship_response(
            field,
            log_context,
            Vec::new(),
            parent_variants,
            staged_ids,
        )
    }

    pub(in crate::proxy) fn product_tail_job_read(&self, field: &RootFieldSelection) -> Value {
        self.product_tail_job_read_with_error(field).0
    }

    fn product_tail_job_read_with_error(
        &self,
        field: &RootFieldSelection,
    ) -> (Value, Option<Value>) {
        let Some(id) = resolved_string_field(&field.arguments, "id") else {
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
        let mut errors = Vec::new();
        let data = root_payload_json(fields, |field| {
            if field.name == "job" {
                let (value, error) = self.product_tail_job_read_with_error(field);
                if let Some(error) = error {
                    errors.push(error);
                }
                Some(value)
            } else {
                None
            }
        });
        let mut body = serde_json::Map::new();
        if !errors.is_empty() {
            body.insert("errors".to_string(), Value::Array(errors));
        }
        body.insert("data".to_string(), data);
        Value::Object(body)
    }

    fn product_tail_combined_listing_response(
        &mut self,
        field: &RootFieldSelection,
        log_context: ProductTailLogContext<'_>,
        user_errors: Vec<Value>,
        product: Option<ProductRecord>,
        staged_ids: Vec<String>,
    ) -> Value {
        self.record_product_tail_outcome(
            log_context,
            "combinedListingUpdate",
            &user_errors,
            staged_ids,
        );
        let product_value = product
            .as_ref()
            .map(|product| {
                self.product_json_with_variants_and_currency_context(
                    product,
                    &self.store.product_variants_for_product(&product.id),
                    selected_child_selection(&field.selection, "product")
                        .as_deref()
                        .unwrap_or(&[]),
                    &self.store.shop_currency_code(),
                )
            })
            .unwrap_or(Value::Null);
        selected_json(
            &json!({
                "product": product_value,
                "userErrors": user_errors
            }),
            &field.selection,
        )
    }

    fn combined_listing_parent_variant_id(
        &self,
        parent_product_id: &str,
        relation: &BTreeMap<String, ResolvedValue>,
    ) -> Option<String> {
        let selected_values = resolved_object_list_field(relation, "selectedParentOptionValues");
        let variants = self.store.product_variants_for_product(parent_product_id);
        variants
            .iter()
            .find(|variant| selected_options_match(&variant.selected_options, &selected_values))
            .or_else(|| variants.first())
            .map(|variant| variant.id.clone())
    }

    fn combined_listing_json(&self, parent: &ProductRecord, links: &[Value]) -> Value {
        let children = links
            .iter()
            .filter_map(|link| {
                let child_id = link.get("childProductId").and_then(Value::as_str)?;
                let child = self.store.product_by_id(child_id)?;
                let parent_variant = link
                    .get("parentVariantId")
                    .and_then(Value::as_str)
                    .and_then(|id| self.store.product_variant_by_id(id))
                    .map(product_variant_state_json)
                    .unwrap_or(Value::Null);
                Some(json!({
                    "__typename": "CombinedListingChild",
                    "product": combined_listing_product_node(child),
                    "parentVariant": parent_variant
                }))
            })
            .collect::<Vec<_>>();
        json!({
            "__typename": "CombinedListing",
            "parentProduct": combined_listing_product_node(parent),
            "combinedListingChildren": connection_json(children)
        })
    }

    fn parent_variant_id_from_relationship_input(
        &self,
        input: &BTreeMap<String, ResolvedValue>,
    ) -> Option<String> {
        resolved_string_field(input, "parentProductVariantId").or_else(|| {
            let product_id = resolved_string_field(input, "parentProductId")?;
            self.store
                .product_variants_for_product(&product_id)
                .first()
                .map(|variant| variant.id.clone())
        })
    }

    fn product_variant_components_connection(&self, rows: &[Value]) -> Value {
        let nodes = rows
            .iter()
            .filter_map(|row| {
                let child_id = row.get("id").and_then(Value::as_str)?;
                let quantity = row.get("quantity").cloned().unwrap_or_else(|| json!(1));
                let variant = self.store.product_variant_by_id(child_id)?;
                Some(json!({
                    "__typename": "ProductVariantComponent",
                    "quantity": quantity,
                    "productVariant": product_variant_state_json(variant)
                }))
            })
            .collect::<Vec<_>>();
        connection_json(nodes)
    }

    fn product_tail_variant_relationship_response(
        &mut self,
        field: &RootFieldSelection,
        log_context: ProductTailLogContext<'_>,
        user_errors: Vec<Value>,
        parent_variants: Vec<ProductVariantRecord>,
        staged_ids: Vec<String>,
    ) -> Value {
        self.record_product_tail_outcome(
            log_context,
            "productVariantRelationshipBulkUpdate",
            &user_errors,
            staged_ids,
        );
        let parent_values = if user_errors.is_empty() {
            Value::Array(
                parent_variants
                    .iter()
                    .map(|variant| {
                        self.product_variant_json_with_current_publication_context(
                            variant,
                            self.store.product_by_id(&variant.product_id),
                            selected_child_selection(&field.selection, "parentProductVariants")
                                .as_deref()
                                .unwrap_or(&[]),
                        )
                    })
                    .collect(),
            )
        } else {
            Value::Null
        };
        selected_json(
            &json!({
                "parentProductVariants": parent_values,
                "userErrors": user_errors
            }),
            &field.selection,
        )
    }
    // Collect the `feedbackInput[].productId`s that reference a product the
    // proxy can prove is unavailable to resource feedback, so
    // `bulkProductResourceFeedbackCreate` can emit Shopify's per-entry missing
    // product userError. A locally tombstoned id is reported missing
    // immediately. Known non-ACTIVE products are also unavailable. An id merely
    // absent from the local catalog is NOT assumed missing — the proxy never
    // seeds every real product, so absence alone is no proof. Instead we confirm
    // against upstream with a cassette-backed `nodes(...)` hydrate: a null node
    // (or, in Snapshot mode, no upstream to consult) means the product does not
    // exist; a hydrated node means it does and feedback stages normally.
    pub(in crate::proxy) fn feedback_missing_product_ids(
        &self,
        field: &RootFieldSelection,
        request: &Request,
    ) -> BTreeSet<String> {
        let mut missing = BTreeSet::new();
        let inputs = resolved_object_list_field(&field.arguments, "feedbackInput");
        // Shopify enforces the 50-entry batch cap before resolving any entry, so an
        // oversized batch returns TOO_LONG without ever looking up a product. Never
        // forward an existence lookup the resolver itself would not perform.
        if inputs.len() > 50 {
            return missing;
        }
        for input in inputs.iter() {
            // Per-entry message / generated-at / length guards run before the
            // existence check, mirroring Shopify's resolver order: an entry that
            // fails one of those reports only that error and never resolves (nor
            // forwards a lookup for) its product.
            if resource_feedback_validation_error(input, None).is_some() {
                continue;
            }
            let Some(id) = resolved_string_field(input, "productId") else {
                continue;
            };
            if self.store.product_is_tombstoned(&id) {
                missing.insert(id);
                continue;
            }
            if let Some(product) = self.store.product_staged_or_base(&id) {
                if !product.status.is_empty() && product.status != "ACTIVE" {
                    missing.insert(id);
                }
                continue;
            }
            // Only LiveHybrid can prove a product's absence by hydrating it
            // upstream (a definitive null node). In Snapshot mode there is no
            // upstream to consult, so an unseeded product is treated as existing
            // (fail open) rather than fabricated-missing — absence from the local
            // seed is not evidence the product does not exist.
            if self.config.read_mode == ReadMode::LiveHybrid
                && self.hydrate_product_for_tags(&id, request).is_none()
            {
                missing.insert(id);
            }
        }
        missing
    }
}

pub(in crate::proxy) fn product_tail_resource_feedback_payload(
    field: &RootFieldSelection,
    missing_product_ids: &BTreeSet<String>,
) -> Value {
    let inputs = resolved_object_list_field(&field.arguments, "feedbackInput");
    let payload = if inputs.len() > 50 {
        json!({
            "feedback": [],
            "userErrors": [{
                "field": ["feedback"],
                "message": "Feedback cannot contain more than 50 entries",
                "code": "TOO_LONG"
            }]
        })
    } else {
        let mut feedback = Vec::new();
        let mut user_errors = Vec::new();
        for (index, input) in inputs.iter().enumerate() {
            if let Some(error) = resource_feedback_validation_error(input, Some(index)) {
                user_errors.push(error);
                continue;
            }
            // Per-entry product availability is validated only after the message /
            // generated-at / length guards pass, mirroring Shopify's resolver
            // order: a blank-message or future-date entry never also reports the
            // product missing.
            let product_id = resolved_string_field(input, "productId").unwrap_or_default();
            if missing_product_ids.contains(&product_id) {
                user_errors.push(resource_feedback_missing_product_error(Some(index)));
            } else {
                feedback.push(product_resource_feedback_json(input));
            }
        }
        json!({ "feedback": feedback, "userErrors": user_errors })
    };
    selected_json(&payload, &field.selection)
}

pub(in crate::proxy) fn product_tail_shop_feedback_payload(field: &RootFieldSelection) -> Value {
    let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
    let payload = if let Some(error) = resource_feedback_validation_error(&input, None) {
        json!({
            "feedback": null,
            "userErrors": [error]
        })
    } else {
        json!({ "feedback": shop_resource_feedback_json(&input), "userErrors": [] })
    };
    selected_json(&payload, &field.selection)
}

fn product_resource_feedback_json(input: &BTreeMap<String, ResolvedValue>) -> Value {
    json!({
        "productId": resolved_string_field(input, "productId").unwrap_or_default(),
        "state": resolved_string_field(input, "state").unwrap_or_default(),
        "messages": list_string_field(input, "messages"),
        "feedbackGeneratedAt": resolved_string_field(input, "feedbackGeneratedAt").unwrap_or_default(),
        "productUpdatedAt": resolved_string_field(input, "productUpdatedAt").unwrap_or_default()
    })
}

fn shop_resource_feedback_json(input: &BTreeMap<String, ResolvedValue>) -> Value {
    let messages = list_string_field(input, "messages")
        .into_iter()
        .map(|message| json!({ "message": message }))
        .collect::<Vec<_>>();
    json!({
        "state": resolved_string_field(input, "state").unwrap_or_default(),
        "messages": messages,
        "feedbackGeneratedAt": resolved_string_field(input, "feedbackGeneratedAt").unwrap_or_default()
    })
}

fn resource_feedback_validation_error(
    input: &BTreeMap<String, ResolvedValue>,
    feedback_index: Option<usize>,
) -> Option<Value> {
    let messages = list_string_field(input, "messages");
    if messages.is_empty() {
        return Some(presence_user_error(
            feedback_field_path(feedback_index, "messages", None),
            "Messages",
        ));
    }

    let generated_at = resolved_string_field(input, "feedbackGeneratedAt").unwrap_or_default();
    if feedback_generated_at_is_future(&generated_at) {
        return Some(resource_feedback_user_error(
            feedback_field_path(feedback_index, "feedbackGeneratedAt", None),
            "Feedback generated at must not be in the future",
            "INVALID",
        ));
    }

    messages
        .iter()
        .position(|message| message.chars().count() > 100)
        .map(|message_index| {
            length_user_error(
                feedback_field_path(feedback_index, "messages", Some(message_index)),
                "Message",
                LengthUserErrorBound::TooLong { maximum: 100 },
            )
        })
}

fn feedback_field_path(
    feedback_index: Option<usize>,
    field: &str,
    nested_index: Option<usize>,
) -> Vec<String> {
    let mut path = match feedback_index {
        Some(index) => vec!["feedback".to_string(), index.to_string()],
        None => vec!["feedback".to_string()],
    };
    path.push(field.to_string());
    if let Some(index) = nested_index {
        path.push(index.to_string());
    }
    path
}

fn resource_feedback_user_error(field: Vec<String>, message: &str, code: &str) -> Value {
    user_error(field, message, Some(code))
}

// Shopify reports referenced-but-unavailable products at the product id field,
// distinct from the BLANK / INVALID / TOO_LONG resolver guards.
fn resource_feedback_missing_product_error(feedback_index: Option<usize>) -> Value {
    let field = feedback_index
        .map(|index| json!(["feedback", index.to_string(), "productId"]))
        .unwrap_or(Value::Null);
    user_error(field, "Product does not exist", None)
}

fn feedback_generated_at_is_future(generated_at: &str) -> bool {
    let Some(generated_at) = parse_rfc3339_epoch_seconds(generated_at) else {
        return false;
    };
    let Ok(now) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) else {
        return false;
    };
    generated_at > now.as_secs() as i64
}

fn resource_feedback_scope_is_explicitly_missing(request: &Request) -> bool {
    request_header(request, ACCESS_SCOPES_HEADER).is_some()
        && !app_access_scope_handles(&current_app_installation_from_request(request))
            .contains("write_resource_feedbacks")
}

fn product_tail_resource_feedback_access_denied_error(field: &RootFieldSelection) -> Value {
    const REQUIRED_ACCESS: &str = "`write_resource_feedbacks` access scope. Also: App must be configured to use the Storefront API or as a Sales Channel.";
    top_level_access_denied_error_envelope(
        format!(
            "Access denied for {} field. Required access: {REQUIRED_ACCESS}",
            field.name
        ),
        Some(field.location),
        vec![json!(field.response_key.clone())],
        Some(REQUIRED_ACCESS),
    )
}

fn product_combined_listing_role(product: &ProductRecord) -> Option<String> {
    product
        .extra_fields
        .get("combinedListingRole")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn combined_listing_relation_child_ids(
    relations: &[BTreeMap<String, ResolvedValue>],
) -> Vec<String> {
    relations
        .iter()
        .filter_map(|relation| resolved_string_field(relation, "childProductId"))
        .collect()
}

fn combined_listing_child_links(product: &ProductRecord) -> Vec<Value> {
    product
        .extra_fields
        .get("combinedListingChildLinks")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn has_duplicate_string(values: &[String]) -> bool {
    let mut seen = BTreeSet::new();
    values.iter().any(|value| !seen.insert(value))
}

fn push_missing_variant_id(missing_ids: &mut Vec<String>, id: String) {
    if !missing_ids.contains(&id) {
        missing_ids.push(id);
    }
}

fn selected_options_match(
    variant_options: &[ProductVariantSelectedOption],
    selected_values: &[BTreeMap<String, ResolvedValue>],
) -> bool {
    if selected_values.is_empty() {
        return true;
    }
    selected_values.iter().all(|selected| {
        let name = resolved_string_field(selected, "name");
        let value = resolved_string_field(selected, "value");
        variant_options.iter().any(|option| {
            Some(option.name.as_str()) == name.as_deref()
                && Some(option.value.as_str()) == value.as_deref()
        })
    })
}

fn combined_listing_product_node(product: &ProductRecord) -> Value {
    json!({
        "__typename": "Product",
        "id": product.id,
        "title": product.title,
        "handle": product.handle,
        "status": product.status,
        "combinedListingRole": product
            .extra_fields
            .get("combinedListingRole")
            .cloned()
            .unwrap_or(Value::Null)
    })
}

fn product_variant_component_rows(variant: &ProductVariantRecord) -> Vec<Value> {
    variant
        .extra_fields
        .get("productVariantComponentRows")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn shopify_error_string_list(values: &[String]) -> String {
    let quoted = values
        .iter()
        .map(|value| serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string()))
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{quoted}]")
}

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
        "userErrors": [user_error(
            ["input", "catalogId"],
            &format!("A catalog was not found for id= {catalog_id}."),
            Some("CATALOG_NOT_FOUND"),
        )]
    })
}

fn publication_not_found_payload(root_field: &str) -> Value {
    let mut payload = serde_json::Map::new();
    payload.insert(root_field.to_string(), Value::Null);
    payload.insert(
        "userErrors".to_string(),
        json!([user_error(
            ["id"],
            "Publication was not found",
            Some("PUBLICATION_NOT_FOUND"),
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

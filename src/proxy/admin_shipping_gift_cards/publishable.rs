use super::*;

const PUBLISHABLE_SHOP_HYDRATE_QUERY: &str = r#"#graphql
  query StorePropertiesPublishableInputValidationHydrate($id: ID!) {
    publishable: node(id: $id) {
      ... on Product {
        id
        publishedOnCurrentPublication
        resourcePublicationsCount {
          count
          precision
        }
      }
    }
    shop {
      publicationCount
    }
    publications(first: 20) {
      nodes {
        id
        name
      }
    }
  }
"#;
const PUBLISHABLE_PUBLICATION_CATALOG_HYDRATE_QUERY: &str = r#"#graphql
  query StorePropertiesPublishableInputValidationHydrate {
    shop {
      publicationCount
      currencyCode
    }
    publications(first: 20) {
      nodes {
        id
        name
      }
    }
  }
"#;
// Must byte-match the recorded upstream location hydrate query in the
// store-properties lifecycle captures (strict cassette compares query text +
// variables). Issued to replay the real baseline location through the cassette
// so activate/deactivate preserve its captured name/scope/state instead of
// fabricating a synthetic record.

impl DraftProxy {
    pub(in crate::proxy) fn product_publishable_mutation(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let RootInvocation {
            response_key,
            root_name: root_field,
            query,
            variables,
            request,
            ..
        } = invocation;
        // When a scenario has seeded publications, the publish/unpublish target
        // mutates that local publication-membership engine (so subsequent
        // publication/product/collection reads reflect the change) instead of the
        // standalone shop-publication-count path below.
        if self.publication_engine_active() {
            return self.publishable_publish_with_publications(
                root_field,
                query,
                variables,
                request,
                response_key,
            );
        }
        let Some(document) = parsed_document(query, variables) else {
            return resolver_http_error_outcome(400, "Unable to parse publishable mutation");
        };
        let operation_path = document.operation_path.clone();
        let Some(fields) = self.execution_root_fields(query, variables) else {
            return resolver_http_error_outcome(400, "Operation has no root field");
        };
        for field in fields {
            if field.name != root_field || field.response_key != response_key {
                continue;
            }
            let Some(resource_id) = resolved_string_field(&field.arguments, "id") else {
                continue;
            };
            if let Some(error) =
                publishable_empty_string_publication_error(query, &operation_path, &field)
            {
                return graphql_error_outcome(vec![error], response_key);
            }

            let payload_selection = field.selection.clone();
            let publishable_selection =
                selected_child_selection(&payload_selection, "publishable").unwrap_or_default();
            let to_current = root_field == "publishablePublishToCurrentChannel"
                || root_field == "publishableUnpublishToCurrentChannel";
            let publish = root_field == "publishablePublish"
                || root_field == "publishablePublishToCurrentChannel";

            if selected_child_selection(&payload_selection, "shop")
                .as_deref()
                .is_some_and(|selection| self.publishable_payload_shop_needs_hydration(selection))
            {
                self.hydrate_publishable_payload_shop(&resource_id, request);
            }
            if self
                .publishable_payload_resource_needs_hydration(&resource_id, &publishable_selection)
            {
                self.hydrate_publishable_payload_shop(&resource_id, request);
                if self.publishable_payload_resource_needs_hydration(
                    &resource_id,
                    &publishable_selection,
                ) {
                    self.hydrate_publishable_resource(&resource_id, request);
                }
            }

            let mut user_errors = Vec::new();
            let resource_exists = self.publishable_resource_exists(&resource_id, request);
            if !resource_exists {
                user_errors.push(user_error_omit_code(
                    ["id"],
                    "Resource does not exist",
                    Some("RESOURCE_DOES_NOT_EXIST"),
                ));
            }
            if resource_exists
                && is_shopify_gid_of_type(&resource_id, "Product")
                && publishable_input_needs_publication_catalog_hydration(
                    field.arguments.get("input"),
                    to_current,
                    self.store.has_known_publication_ids(),
                )
            {
                if admin_graphql_version(&request.path)
                    .is_some_and(|version| version_at_least(version, 2026, 4))
                {
                    self.hydrate_publishable_publication_catalog(request);
                } else {
                    self.hydrate_publishable_payload_shop(&resource_id, request);
                }
            }
            user_errors.extend(
                self.publishable_publication_input_errors(field.arguments.get("input"), to_current),
            );

            let current_channel_id = if resource_exists && to_current {
                self.resolve_current_channel_publication_id(request)
            } else {
                None
            };
            if resource_exists && to_current && current_channel_id.is_none() {
                user_errors.push(user_error_omit_code(
                    ["id"],
                    "Channel does not exist",
                    Some("CHANNEL_DOES_NOT_EXIST"),
                ));
            }

            if user_errors.is_empty() {
                let publication_ids = if to_current {
                    current_channel_id.into_iter().collect::<Vec<_>>()
                } else {
                    publishable_input_publication_ids(&field.arguments)
                };
                let published_at = self.next_product_timestamp();
                let set = self
                    .store
                    .staged
                    .resource_publications
                    .entry(resource_id.clone())
                    .or_default();
                for publication_id in &publication_ids {
                    if publish {
                        set.insert(publication_id.clone());
                    } else {
                        set.remove(publication_id);
                    }
                }
                self.sync_product_publication_entries(
                    &resource_id,
                    &publication_ids,
                    publish,
                    &published_at,
                );
            }

            let publishable = if user_errors.iter().any(|error| {
                error
                    .get("code")
                    .and_then(Value::as_str)
                    .is_some_and(|code| code == "RESOURCE_DOES_NOT_EXIST")
            }) {
                Value::Null
            } else {
                self.publishable_resource_value(&resource_id, &publishable_selection)
            };
            let shop = self.store.effective_shop();
            let payload = selected_payload_json(&payload_selection, |selection| {
                match selection.name.as_str() {
                    "publishable" => Some(publishable.clone()),
                    "shop" => Some(selected_json(&shop, &selection.selection)),
                    "userErrors" => selected_user_errors_field(user_errors.as_slice(), selection),
                    _ => None,
                }
            });
            let outcome = ResolverOutcome::value(payload);
            return if user_errors.is_empty() {
                outcome.with_log_draft(LogDraft::staged(root_field, "store_properties", Vec::new()))
            } else {
                outcome
            };
        }
        ResolverOutcome::value(Value::Null)
    }

    pub(in crate::proxy) fn publishable_payload_shop_needs_hydration(
        &self,
        selection: &[SelectedField],
    ) -> bool {
        self.config.read_mode != ReadMode::Snapshot
            && (self.store.base.publication_count.is_none()
                || selection.iter().any(|field| {
                    field.name != "publicationCount"
                        && self.store.base.shop.get(&field.name).is_none()
                }))
    }

    pub(in crate::proxy) fn hydrate_publishable_payload_shop(
        &mut self,
        publishable_id: &str,
        request: &Request,
    ) {
        if self.config.read_mode == ReadMode::Snapshot {
            return;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": PUBLISHABLE_SHOP_HYDRATE_QUERY,
                "variables": { "id": publishable_id }
            }),
        );
        if !(200..300).contains(&response.status) {
            return;
        }
        if let Some(id) = response
            .body
            .pointer("/data/publishable/id")
            .and_then(Value::as_str)
        {
            self.store
                .staged
                .resource_publications
                .entry(id.to_string())
                .or_default();
        }
        self.hydrate_shop_state_from_response_data(&response.body["data"]);
    }

    fn hydrate_publishable_publication_catalog(&mut self, request: &Request) {
        if self.config.read_mode == ReadMode::Snapshot {
            return;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": PUBLISHABLE_PUBLICATION_CATALOG_HYDRATE_QUERY,
                "variables": {}
            }),
        );
        if !(200..300).contains(&response.status) {
            return;
        }
        self.hydrate_shop_state_from_response_data(&response.body["data"]);
    }

    pub(in crate::proxy) fn sync_product_publication_entries(
        &mut self,
        resource_id: &str,
        publication_ids: &[String],
        publish: bool,
        published_at: &str,
    ) {
        if !is_shopify_gid_of_type(resource_id, "Product") || publication_ids.is_empty() {
            return;
        }
        let Some(mut product) = self.store.product_by_id(resource_id).cloned() else {
            return;
        };
        let publication_ids = publication_ids.iter().cloned().collect::<BTreeSet<_>>();
        let mut entries = product_publication_entries(&product);
        if publish {
            for publication_id in &publication_ids {
                if let Some(entry) = entries
                    .iter_mut()
                    .find(|entry| entry.publication_id == *publication_id)
                {
                    if entry.published_at.is_none() && entry.publish_date.is_none() {
                        entry.published_at = Some(published_at.to_string());
                    }
                } else {
                    entries.push(ProductPublicationEntry {
                        publication_id: publication_id.clone(),
                        publish_date: None,
                        published_at: Some(published_at.to_string()),
                    });
                }
            }
        } else {
            entries.retain(|entry| !publication_ids.contains(&entry.publication_id));
        }
        set_product_publication_entries(&mut product, entries);
        self.store.stage_product(product);
    }

    pub(in crate::proxy) fn hydrate_shop_state_from_response_data(&mut self, data: &Value) {
        if let Some(shop) = data.get("shop").filter(|shop| shop.is_object()) {
            let (policies, order) = shop_policy_state_from_shop(shop);
            if !policies.is_empty() {
                self.store
                    .base
                    .shop_policies
                    .replace_with_order(policies, order);
            }
            self.store.base.shop =
                shallow_merged_object(self.store.base.shop.clone(), shop.clone());
        }
        if let Some(nodes) = data["publications"]["nodes"].as_array() {
            self.store.base.publication_ids = nodes
                .iter()
                .filter_map(|node| node.get("id").and_then(Value::as_str).map(str::to_string))
                .collect();
        }
        self.store.base.publication_count = data["shop"]["publicationCount"]
            .as_u64()
            .map(|count| count as usize)
            .or(Some(self.store.base.publication_ids.len()));
        if let Some(publishable) = data.get("publishable").filter(|value| value.is_object()) {
            let id = publishable
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if is_shopify_gid_of_type(id, "Collection") {
                self.store.stage_collection(publishable.clone());
            }
        }
    }

    pub(in crate::proxy) fn publishable_payload_resource_needs_hydration(
        &self,
        publishable_id: &str,
        selection: &[SelectedField],
    ) -> bool {
        if self.config.read_mode == ReadMode::Snapshot {
            return false;
        }
        let resource_type = shopify_gid_resource_type(publishable_id).unwrap_or_default();
        if resource_type != "Collection" {
            return false;
        }
        let Some(collection) = self.store.collection_by_id(publishable_id) else {
            return selection.iter().any(|field| {
                publishable_selection_field_applies(field, resource_type)
                    && matches!(field.name.as_str(), "title" | "handle")
            });
        };
        selection.iter().any(|field| {
            publishable_selection_field_applies(field, resource_type)
                && matches!(field.name.as_str(), "title" | "handle")
                && collection.get(&field.name).is_none()
        })
    }
}

fn publishable_selection_field_applies(field: &SelectedField, resource_type: &str) -> bool {
    field
        .type_condition
        .as_deref()
        .is_none_or(|condition| condition == resource_type || condition == "Node")
}

pub(in crate::proxy) fn publishable_input_needs_publication_catalog_hydration(
    input: Option<&ResolvedValue>,
    current_channel_root: bool,
    has_known_publication_ids: bool,
) -> bool {
    if current_channel_root || has_known_publication_ids {
        return false;
    }
    let Some(ResolvedValue::List(publications)) = input else {
        return false;
    };
    publications.iter().any(|publication| {
        let ResolvedValue::Object(publication) = publication else {
            return false;
        };
        resolved_string_field(publication, "publicationId")
            .as_deref()
            .is_some_and(|id| !id.is_empty())
    })
}

/// The publication gids named in a `publishablePublish`/`publishableUnpublish`
/// `input: [{ publicationId }]` list, in order.
pub(in crate::proxy) fn publishable_input_publication_ids(
    arguments: &BTreeMap<String, ResolvedValue>,
) -> Vec<String> {
    match arguments.get("input") {
        Some(ResolvedValue::List(items)) => items
            .iter()
            .filter_map(|item| match item {
                ResolvedValue::Object(publication) => {
                    resolved_string_field(publication, "publicationId")
                }
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

impl DraftProxy {
    pub(in crate::proxy) fn publishable_publication_input_errors(
        &self,
        input: Option<&ResolvedValue>,
        current_channel_root: bool,
    ) -> Vec<Value> {
        if current_channel_root {
            return Vec::new();
        }
        let Some(ResolvedValue::List(publications)) = input else {
            return Vec::new();
        };

        let mut seen = BTreeSet::new();
        let mut user_errors = Vec::new();
        let validate_publication_ids = self.store.has_known_publication_ids();
        for (index, publication) in publications.iter().enumerate() {
            let ResolvedValue::Object(publication) = publication else {
                continue;
            };
            let field_index = index.to_string();
            let publication_id = resolved_string_field(publication, "publicationId");
            match publication_id.as_deref() {
                Some("") => {
                    user_errors.push(user_error_omit_code(
                        json!(["input", field_index, "publicationId"]),
                        "PublicationId cannot be empty",
                        None,
                    ));
                    continue;
                }
                Some(id) if validate_publication_ids && !self.store.has_publication_id(id) => {
                    user_errors.push(user_error_omit_code(
                        json!(["input", field_index, "publicationId"]),
                        "Publication does not exist or is not publishable",
                        None,
                    ));
                    continue;
                }
                Some(id) if !seen.insert(id.to_string()) => {
                    user_errors.push(user_error_omit_code(
                        json!(["input", field_index, "publicationId"]),
                        "The same publication was specified more than once",
                        None,
                    ));
                }
                Some(_) => {}
                None => user_errors.push(user_error_omit_code(
                    json!(["input", field_index, "publicationId"]),
                    "PublicationId cannot be empty",
                    None,
                )),
            }

            if resolved_string_field(publication, "publishDate")
                .as_deref()
                .map(publishable_publish_date_is_before_1970)
                .unwrap_or(false)
            {
                user_errors.push(user_error_omit_code(
                    json!(["input", field_index, "publishDate"]),
                    "Publish date must be a date after the year 1969",
                    None,
                ));
            }
        }
        user_errors
    }
}

fn publishable_publish_date_is_before_1970(value: &str) -> bool {
    value
        .get(..4)
        .and_then(|year| year.parse::<i32>().ok())
        .map(|year| year < 1970)
        .unwrap_or(false)
}

pub(in crate::proxy) fn publishable_empty_string_publication_error(
    query: &str,
    operation_path: &str,
    field: &RootFieldSelection,
) -> Option<Value> {
    let input = field.arguments.get("input")?;
    let ResolvedValue::List(publications) = input else {
        return None;
    };
    let (index, _) = publications.iter().enumerate().find(|(_, publication)| {
        let ResolvedValue::Object(publication) = publication else {
            return false;
        };
        resolved_string_field(publication, "publicationId").as_deref() == Some("")
    })?;

    if let Some(RawArgumentValue::Variable { name, value }) = field.raw_arguments.get("input") {
        let variable_definition = variable_definition_info(query, name)?;
        let variable_value = value.as_ref().unwrap_or(input);
        let explanation = "Invalid global id ''";
        let path_display = format!("{index}.publicationId");
        let problem = json!({
            "path": [index, "publicationId"],
            "explanation": explanation,
            "message": explanation,
        });
        let message = format!(
            "Variable ${name} of type {} was provided invalid value for {path_display} ({explanation})",
            variable_definition.type_display
        );
        return Some(invalid_variable_error_envelope(
            message,
            variable_definition.location,
            resolved_values::resolved_value_json(variable_value),
            json!([problem]),
        ));
    }

    let location = inline_argument_list_item_object_location(query, field, "input", index)
        .unwrap_or(field.location);
    Some(json!({
        "message": "Invalid global id ''",
        "locations": [{ "line": location.line, "column": location.column }],
        "path": [operation_path, field.response_key, "input", index, "publicationId"],
        "extensions": {
            "code": "argumentLiteralsIncompatible",
            "typeName": "CoercionError"
        }
    }))
}

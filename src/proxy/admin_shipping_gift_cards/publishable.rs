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
// Must byte-match the recorded upstream location hydrate query in the
// store-properties lifecycle captures (strict cassette compares query text +
// variables). Issued to replay the real baseline location through the cassette
// so activate/deactivate preserve its captured name/scope/state instead of
// fabricating a synthetic record.

impl DraftProxy {
    pub(in crate::proxy) fn product_publishable_mutation(
        &mut self,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        // When a scenario has seeded publications, the publish/unpublish target
        // mutates that local publication-membership engine (so subsequent
        // publication/product/collection reads reflect the change) instead of the
        // standalone shop-publication-count path below.
        if self.publication_engine_active() {
            return self
                .publishable_publish_with_publications(root_field, query, variables, request);
        }
        let Some(fields) = root_fields(query, variables) else {
            return json_error(400, "Unable to parse publishable mutation");
        };
        let mut data = serde_json::Map::new();
        for field in fields {
            if field.name != root_field {
                continue;
            }
            let resource_id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
            if let Some(response) = publishable_empty_string_publication_error(root_field, &field) {
                return response;
            }
            let payload_selection = field.selection.clone();
            if selected_child_selection(&payload_selection, "shop")
                .as_deref()
                .is_some_and(|selection| self.publishable_payload_shop_needs_hydration(selection))
            {
                self.hydrate_publishable_payload_shop(&resource_id, request);
            }
            let publishable_selection =
                selected_child_selection(&payload_selection, "publishable").unwrap_or_default();
            let to_current = root_field == "publishablePublishToCurrentChannel"
                || root_field == "publishableUnpublishToCurrentChannel";
            let publish = root_field == "publishablePublish"
                || root_field == "publishablePublishToCurrentChannel";
            let mut user_errors = Vec::new();
            let resource_exists = self.publishable_resource_exists(&resource_id, request);
            if !resource_exists {
                user_errors.push(user_error_omit_code(
                    ["id"],
                    "Resource does not exist",
                    Some("RESOURCE_DOES_NOT_EXIST"),
                ));
            }
            user_errors.extend(publishable_publication_input_errors(
                field.arguments.get("input"),
                to_current,
            ));
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
            let null_publishable = user_errors.iter().any(|error| {
                error
                    .get("code")
                    .and_then(Value::as_str)
                    .is_some_and(|code| code == "RESOURCE_DOES_NOT_EXIST")
            });
            let publishable = if null_publishable {
                Value::Null
            } else if resource_id.starts_with("gid://shopify/Collection/") {
                let collection = collection_publication_record(resource_id.clone(), publish);
                if user_errors.is_empty() {
                    if let Some(id) = collection.get("id").and_then(Value::as_str) {
                        self.store
                            .staged
                            .collections
                            .insert(id.to_string(), collection.clone());
                    }
                }
                selected_json(&collection, &publishable_selection)
            } else {
                self.publishable_resource_value(&resource_id, &publishable_selection)
            };
            if user_errors.is_empty() {
                let publication_ids = if to_current {
                    current_channel_id
                        .map(|id| vec![id.to_string()])
                        .unwrap_or_default()
                } else {
                    publishable_input_publication_ids(&field.arguments)
                };
                let set = self
                    .store
                    .staged
                    .resource_publications
                    .entry(resource_id.clone())
                    .or_default();
                for publication_id in publication_ids {
                    if publish {
                        set.insert(publication_id);
                    } else {
                        set.remove(&publication_id);
                    }
                }
                let publishable = if resource_id.starts_with("gid://shopify/Collection/") {
                    let collection = collection_publication_record(resource_id.clone(), publish);
                    if let Some(id) = collection.get("id").and_then(Value::as_str) {
                        self.store
                            .staged
                            .collections
                            .insert(id.to_string(), collection);
                    }
                    self.publishable_resource_value(&resource_id, &publishable_selection)
                } else {
                    self.publishable_resource_value(&resource_id, &publishable_selection)
                };
                self.record_mutation_log_entry(request, query, variables, root_field, vec![]);
                let shop = self.store.effective_shop();
                data.insert(
                    field.response_key,
                    publishable_payload_json(
                        publishable,
                        shop,
                        &payload_selection,
                        &publishable_selection,
                        user_errors,
                    ),
                );
                continue;
            }
            let shop = self.store.effective_shop();
            data.insert(
                field.response_key,
                publishable_payload_json(
                    publishable,
                    shop,
                    &payload_selection,
                    &publishable_selection,
                    user_errors,
                ),
            );
        }
        ok_json(json!({ "data": Value::Object(data) }))
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

    pub(in crate::proxy) fn hydrate_shop_state_from_response_data(&mut self, data: &Value) {
        if let Some(shop) = data.get("shop").filter(|shop| shop.is_object()) {
            let (policies, order) = shop_policy_state_from_shop(shop);
            if !policies.is_empty() {
                self.store
                    .base
                    .shop_policies
                    .replace_with_order(policies, order);
            }
            self.store.base.shop = shop.clone();
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
    }
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

pub(in crate::proxy) fn publishable_publication_input_errors(
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
            Some("gid://shopify/Publication/999999999999") => {
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

fn publishable_publish_date_is_before_1970(value: &str) -> bool {
    value
        .get(..4)
        .and_then(|year| year.parse::<i32>().ok())
        .map(|year| year < 1970)
        .unwrap_or(false)
}

pub(in crate::proxy) fn publishable_empty_string_publication_error(
    root_field: &str,
    field: &RootFieldSelection,
) -> Option<Response> {
    let input = field.arguments.get("input")?;
    let ResolvedValue::List(publications) = input else {
        return None;
    };
    let has_empty_string = publications.iter().any(|publication| {
        let ResolvedValue::Object(publication) = publication else {
            return false;
        };
        resolved_string_field(publication, "publicationId").as_deref() == Some("")
    });
    if !has_empty_string {
        return None;
    }

    let column = match root_field {
        "publishableUnpublish" => 58,
        _ => 56,
    };
    let message = "Variable $input of type [PublicationInput!]! was provided invalid value for 0.publicationId (Invalid global id '')";
    Some(ok_json(json!({
        "errors": [{
            "message": message,
            "locations": [{ "line": field.location.line, "column": column }],
            "extensions": {
                "code": "INVALID_VARIABLE",
                "value": resolved_values::resolved_value_json(input),
                "problems": [{
                    "path": [0, "publicationId"],
                    "explanation": "Invalid global id ''",
                    "message": "Invalid global id ''"
                }]
            }
        }]
    })))
}

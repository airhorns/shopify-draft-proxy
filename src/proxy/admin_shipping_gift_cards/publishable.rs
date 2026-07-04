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
            let Some(product_id) = resolved_string_field(&field.arguments, "id") else {
                continue;
            };
            if let Some(response) = publishable_empty_string_publication_error(root_field, &field) {
                return response;
            }
            let payload_selection = field.selection.clone();
            if selected_child_selection(&payload_selection, "shop")
                .as_deref()
                .is_some_and(|selection| self.publishable_payload_shop_needs_hydration(selection))
            {
                self.hydrate_publishable_payload_shop(&product_id, request);
            }
            let publishable_selection =
                selected_child_selection(&payload_selection, "publishable").unwrap_or_default();
            let to_current = root_field == "publishablePublishToCurrentChannel"
                || root_field == "publishableUnpublishToCurrentChannel";
            let publish = root_field == "publishablePublish"
                || root_field == "publishablePublishToCurrentChannel";
            let user_errors =
                self.publishable_publication_input_errors(field.arguments.get("input"), to_current);
            if user_errors.is_empty() {
                if self.publishable_payload_resource_needs_hydration(
                    &product_id,
                    &publishable_selection,
                ) {
                    self.hydrate_publishable_payload_shop(&product_id, request);
                    if self.publishable_payload_resource_needs_hydration(
                        &product_id,
                        &publishable_selection,
                    ) {
                        self.hydrate_publishable_resource(&product_id, request);
                    }
                }
                let mut publication_ids = publishable_input_publication_ids(&field.arguments);
                if to_current {
                    if let Some(publication_id) = self.store.current_publication_id() {
                        publication_ids.push(publication_id.to_string());
                    }
                }
                let set = self
                    .store
                    .staged
                    .resource_publications
                    .entry(product_id.clone())
                    .or_default();
                for publication_id in publication_ids {
                    if publish {
                        set.insert(publication_id);
                    } else {
                        set.remove(&publication_id);
                    }
                }
                self.record_mutation_log_entry(request, query, variables, root_field, vec![]);
            }
            let publishable = self.publishable_resource_value(&product_id, &publishable_selection);
            let shop = self.store.effective_shop();
            data.insert(
                field.response_key,
                selected_payload_json(&payload_selection, |selection| {
                    match selection.name.as_str() {
                        "publishable" => Some(publishable.clone()),
                        "shop" => Some(selected_json(&shop, &selection.selection)),
                        "userErrors" => {
                            selected_user_errors_field(user_errors.as_slice(), selection)
                        }
                        _ => None,
                    }
                }),
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
        if let Some(publishable) = data.get("publishable").filter(|value| value.is_object()) {
            let id = publishable
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if id.starts_with("gid://shopify/Collection/") {
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

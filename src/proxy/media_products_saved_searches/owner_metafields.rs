use super::media::media_file_record_from_node;
use super::*;

pub(in crate::proxy) fn owner_metafield_field_resolver_registrations(
) -> Vec<FieldResolverRegistration> {
    [
        "CartTransform",
        "DeliveryCustomization",
        "FulfillmentConstraintRule",
        "Location",
        "Order",
        "PaymentCustomization",
        "Shop",
        "Validation",
        "Company",
    ]
    .into_iter()
    .flat_map(|parent_type| {
        [
            FieldResolverRegistration::explicit(
                ApiSurface::Admin,
                parent_type,
                "metafield",
                owner_metafield_field,
            ),
            FieldResolverRegistration::explicit(
                ApiSurface::Admin,
                parent_type,
                "metafields",
                owner_metafields_field,
            ),
        ]
    })
    .collect()
}

fn owner_metafield_field(
    proxy: &mut DraftProxy,
    request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let api_client_id = request_app_namespace_api_client_id(request);
    Ok(proxy.canonical_embedded_or_owner_metafield_value(
        invocation.parent,
        &resolved_arguments_from_json(&invocation.arguments),
        api_client_id.as_deref(),
    ))
}

fn owner_metafields_field(
    proxy: &mut DraftProxy,
    request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let api_client_id = request_app_namespace_api_client_id(request);
    Ok(
        proxy.canonical_embedded_or_owner_metafields_connection_value(
            invocation.parent,
            &resolved_arguments_from_json(&invocation.arguments),
            api_client_id.as_deref(),
        ),
    )
}

impl DraftProxy {
    pub(in crate::proxy) fn should_route_owner_metafields_read(
        &self,
        fields: &[RootFieldSelection],
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> bool {
        self.should_handle_owner_metafields_read(fields, variables)
            && fields.iter().all(|field| {
                matches!(
                    field.name.as_str(),
                    "product"
                        | "productVariant"
                        | "collection"
                        | "customer"
                        | "order"
                        | "company"
                        | "shop"
                )
            })
    }
}
use base64::Engine as _;

const OWNER_METAFIELD_OBSERVATION_FIELDS: &str =
    "id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType";
const OWNER_METAFIELD_PAGE_INFO_FIELDS: &str =
    "pageInfo { hasNextPage hasPreviousPage startCursor endCursor }";
const OWNER_PRODUCT_BASE_FIELDS: &str =
    "id title handle status totalInventory tracksInventory createdAt updatedAt";
const OWNER_PRODUCT_VARIANT_BASE_FIELDS: &str = "id title sku barcode price compareAtPrice taxable inventoryPolicy inventoryQuantity selectedOptions { name value } inventoryItem { id tracked requiresShipping }";

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct OwnerMetafieldHydrationShape {
    metafields: Vec<OwnerMetafieldHydrateField>,
    connections: Vec<OwnerMetafieldsHydrateConnection>,
    product_variants: Option<OwnerMetafieldHydrateConnectionWindow>,
}

impl OwnerMetafieldHydrationShape {
    fn extend_from_owner_selections(
        &mut self,
        selections: &[SelectedField],
        api_client_id: Option<&str>,
    ) {
        for selection in selections {
            match selection.name.as_str() {
                "metafield" => {
                    let namespace =
                        owner_metafield_read_namespace(&selection.arguments, api_client_id);
                    let key =
                        resolved_string_field(&selection.arguments, "key").unwrap_or_default();
                    if !namespace.is_empty() && !key.is_empty() {
                        self.push_metafield(namespace, key);
                    }
                }
                "metafields" => {
                    self.push_connection(OwnerMetafieldsHydrateConnection {
                        window: OwnerMetafieldHydrateConnectionWindow::from_args(
                            &selection.arguments,
                            10,
                        ),
                        namespace: owner_metafields_connection_namespace(
                            &selection.arguments,
                            api_client_id,
                        ),
                        keys: owner_metafields_connection_keys_with_app_namespace(
                            &selection.arguments,
                            api_client_id,
                        )
                        .map(|keys| {
                            keys.into_iter()
                                .map(|(namespace, key)| format!("{namespace}.{key}"))
                                .collect()
                        }),
                    });
                }
                "variants" if Self::selection_selects_metafields(&selection.selection) => {
                    self.product_variants = Some(OwnerMetafieldHydrateConnectionWindow::from_args(
                        &selection.arguments,
                        10,
                    ));
                    self.extend_from_owner_selections(&selection.selection, api_client_id);
                }
                _ => self.extend_from_owner_selections(&selection.selection, api_client_id),
            }
        }
    }

    fn push_metafield(&mut self, namespace: String, key: String) {
        let field = OwnerMetafieldHydrateField { namespace, key };
        if !self.metafields.contains(&field) {
            self.metafields.push(field);
        }
    }

    fn push_connection(&mut self, connection: OwnerMetafieldsHydrateConnection) {
        if !self.connections.contains(&connection) {
            self.connections.push(connection);
        }
    }

    fn selection_selects_metafields(selections: &[SelectedField]) -> bool {
        selections.iter().any(|selection| {
            matches!(selection.name.as_str(), "metafield" | "metafields")
                || Self::selection_selects_metafields(&selection.selection)
        })
    }

    fn is_empty(&self) -> bool {
        self.metafields.is_empty() && self.connections.is_empty()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct OwnerMetafieldHydrateField {
    namespace: String,
    key: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct OwnerMetafieldsHydrateConnection {
    window: OwnerMetafieldHydrateConnectionWindow,
    namespace: Option<String>,
    keys: Option<Vec<String>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct OwnerMetafieldHydrateConnectionWindow {
    first: Option<i64>,
    after: Option<String>,
    last: Option<i64>,
    before: Option<String>,
    reverse: Option<bool>,
}

impl OwnerMetafieldHydrateConnectionWindow {
    fn from_args(arguments: &BTreeMap<String, ResolvedValue>, default_first: i64) -> Self {
        let first = resolved_int_field(arguments, "first");
        let last = resolved_int_field(arguments, "last");
        Self {
            first: first.or_else(|| last.is_none().then_some(default_first)),
            after: resolved_string_field(arguments, "after"),
            last,
            before: resolved_string_field(arguments, "before"),
            reverse: resolved_bool_field(arguments, "reverse"),
        }
    }

    fn push_graphql_args(
        &self,
        variable_prefix: &str,
        definitions: &mut Vec<String>,
        variables: &mut serde_json::Map<String, Value>,
        args: &mut Vec<String>,
    ) {
        push_optional_graphql_arg(
            definitions,
            variables,
            args,
            "first",
            &format!("{variable_prefix}First"),
            "Int",
            self.first.map(Value::from),
        );
        push_optional_graphql_arg(
            definitions,
            variables,
            args,
            "after",
            &format!("{variable_prefix}After"),
            "String",
            self.after.as_ref().map(|value| json!(value)),
        );
        push_optional_graphql_arg(
            definitions,
            variables,
            args,
            "last",
            &format!("{variable_prefix}Last"),
            "Int",
            self.last.map(Value::from),
        );
        push_optional_graphql_arg(
            definitions,
            variables,
            args,
            "before",
            &format!("{variable_prefix}Before"),
            "String",
            self.before.as_ref().map(|value| json!(value)),
        );
        push_optional_graphql_arg(
            definitions,
            variables,
            args,
            "reverse",
            &format!("{variable_prefix}Reverse"),
            "Boolean",
            self.reverse.map(Value::from),
        );
    }
}

fn owner_metafield_hydrate_request(
    ids: Vec<String>,
    shape: &OwnerMetafieldHydrationShape,
) -> Option<(String, Value)> {
    if shape.is_empty() {
        return None;
    }

    let mut variable_definitions = vec!["$ids: [ID!]!".to_string()];
    let mut variables = serde_json::Map::new();
    variables.insert("ids".to_string(), json!(ids));
    let owner_metafields =
        owner_metafield_hydrate_fields(shape, &mut variable_definitions, &mut variables);
    let product_variants = shape.product_variants.as_ref().map(|window| {
        let mut args = Vec::new();
        window.push_graphql_args(
            "productVariants",
            &mut variable_definitions,
            &mut variables,
            &mut args,
        );
        format!(
            "variants({}) {{ nodes {{ {OWNER_PRODUCT_VARIANT_BASE_FIELDS} {owner_metafields} }} }}",
            args.join(", ")
        )
    });
    let product_variants = product_variants.unwrap_or_default();
    let variable_definition_list = variable_definitions.join(", ");
    let query = format!(
        "query OwnerMetafieldsHydrateNodes({variable_definition_list}) {{ nodes(ids: $ids) {{ __typename id ... on Product {{ {OWNER_PRODUCT_BASE_FIELDS} {owner_metafields} {product_variants} }} ... on ProductVariant {{ {OWNER_PRODUCT_VARIANT_BASE_FIELDS} product {{ {OWNER_PRODUCT_BASE_FIELDS} }} {owner_metafields} }} ... on Collection {{ id title handle {owner_metafields} }} ... on Customer {{ id displayName email {owner_metafields} }} ... on Order {{ id name {owner_metafields} }} ... on Company {{ id name {owner_metafields} }} ... on Shop {{ id {owner_metafields} }} }} }}"
    );
    Some((query, Value::Object(variables)))
}

fn owner_metafield_hydrate_fields(
    shape: &OwnerMetafieldHydrationShape,
    variable_definitions: &mut Vec<String>,
    variables: &mut serde_json::Map<String, Value>,
) -> String {
    let mut fields = Vec::new();
    for (index, field) in shape.metafields.iter().enumerate() {
        let namespace_variable = format!("metafield{index}Namespace");
        let key_variable = format!("metafield{index}Key");
        variable_definitions.push(format!("${namespace_variable}: String!"));
        variable_definitions.push(format!("${key_variable}: String!"));
        variables.insert(namespace_variable.clone(), json!(field.namespace));
        variables.insert(key_variable.clone(), json!(field.key));
        fields.push(format!(
            "metafield{index}: metafield(namespace: ${namespace_variable}, key: ${key_variable}) {{ {OWNER_METAFIELD_OBSERVATION_FIELDS} }}"
        ));
    }
    for (index, connection) in shape.connections.iter().enumerate() {
        let prefix = format!("metafields{index}");
        let mut args = Vec::new();
        connection
            .window
            .push_graphql_args(&prefix, variable_definitions, variables, &mut args);
        push_optional_graphql_arg(
            variable_definitions,
            variables,
            &mut args,
            "namespace",
            &format!("{prefix}Namespace"),
            "String",
            connection.namespace.as_ref().map(|value| json!(value)),
        );
        push_optional_graphql_arg(
            variable_definitions,
            variables,
            &mut args,
            "keys",
            &format!("{prefix}Keys"),
            "[String!]",
            connection.keys.as_ref().map(|value| json!(value)),
        );
        fields.push(format!(
            "metafields{index}: metafields({}) {{ nodes {{ {OWNER_METAFIELD_OBSERVATION_FIELDS} }} {OWNER_METAFIELD_PAGE_INFO_FIELDS} }}",
            args.join(", ")
        ));
    }
    fields.join(" ")
}

fn push_optional_graphql_arg(
    variable_definitions: &mut Vec<String>,
    variables: &mut serde_json::Map<String, Value>,
    args: &mut Vec<String>,
    arg_name: &str,
    variable_name: &str,
    variable_type: &str,
    value: Option<Value>,
) {
    if let Some(value) = value {
        variable_definitions.push(format!("${variable_name}: {variable_type}"));
        variables.insert(variable_name.to_string(), value);
        args.push(format!("{arg_name}: ${variable_name}"));
    }
}

fn observed_value_field_paths(value: &Value) -> BTreeSet<Vec<String>> {
    fn collect(value: &Value, prefix: &mut Vec<String>, paths: &mut BTreeSet<Vec<String>>) {
        match value {
            Value::Object(object) => {
                for (field, value) in object {
                    prefix.push(field.clone());
                    paths.insert(prefix.clone());
                    collect(value, prefix, paths);
                    prefix.pop();
                }
            }
            Value::Array(values) => {
                for value in values {
                    collect(value, prefix, paths);
                }
            }
            _ => {}
        }
    }

    let mut paths = BTreeSet::new();
    collect(value, &mut Vec::new(), &mut paths);
    paths
}

fn owner_metafield_child_path(path: &[String]) -> bool {
    path.iter()
        .any(|field| matches!(field.as_str(), "metafield" | "metafields"))
}

impl DraftProxy {
    pub(in crate::proxy) fn owner_parent_is_partial(&self, id: &str) -> bool {
        self.store
            .staged
            .owner_parent_observed_field_paths
            .contains_key(id)
            && !self.owner_parent_is_tombstoned(id)
    }

    pub(in crate::proxy) fn owner_parent_shape_is_complete(
        &self,
        id: &str,
        requested_field_paths: &BTreeSet<Vec<String>>,
    ) -> bool {
        let Some(observed) = self.store.staged.owner_parent_observed_field_paths.get(id) else {
            return true;
        };
        requested_field_paths.iter().all(|path| {
            owner_metafield_child_path(path)
                || !self.owner_parent_field_path_applies(id, path)
                || observed.contains(path)
        })
    }

    fn owner_parent_field_path_applies(&self, id: &str, path: &[String]) -> bool {
        let Some(mut parent_type) = shopify_gid_resource_type(id).map(str::to_string) else {
            return true;
        };
        let Some(version) = self.execution_session.api_version.as_deref() else {
            return true;
        };
        for field in path {
            if field == "__typename" {
                return true;
            }
            let Some(field_type) =
                crate::admin_graphql::output_field_named_type(version, &parent_type, field)
            else {
                return false;
            };
            parent_type = field_type;
        }
        true
    }

    fn owner_parent_record_exists(&self, id: &str) -> bool {
        match shopify_gid_resource_type(id) {
            Some("Product") => self.store.product_by_id(id).is_some(),
            Some("ProductVariant") => self.store.product_variant_by_id(id).is_some(),
            Some("Collection") => self.store.collection_by_id(id).is_some(),
            Some("Customer") => self.store.staged.customers.contains_key(id),
            Some("Order") => self.store.observed_order_by_id(id).is_some(),
            Some("Company") => {
                self.store.staged.b2b_companies.contains_key(id)
                    || self.store.base.b2b_companies.get(id).is_some()
            }
            _ => false,
        }
    }

    fn owner_parent_is_tombstoned(&self, id: &str) -> bool {
        match shopify_gid_resource_type(id) {
            Some("Product") => self.store.product_is_tombstoned(id),
            Some("ProductVariant") => self.store.product_variants.staged.is_tombstoned(id),
            Some("Collection") => self.store.collection_is_deleted(id),
            Some("Customer") => self.store.staged.customers.is_tombstoned(id),
            Some("Order") => self.store.staged.orders.is_tombstoned(id),
            Some("Company") => self.store.staged.deleted_b2b_company_ids.contains(id),
            _ => false,
        }
    }

    fn stage_observed_owner_parent_record(&mut self, node: &Value) {
        let Some(id) = node.get("id").and_then(Value::as_str).map(str::to_string) else {
            return;
        };
        if self.owner_parent_is_tombstoned(&id) {
            return;
        }
        let already_partial = self.owner_parent_is_partial(&id);
        if self.owner_parent_record_exists(&id) && !already_partial {
            return;
        }
        if self.execution_session.api_surface == Some(ApiSurface::Admin) {
            if let Some(version) = self.execution_session.api_version.as_deref() {
                self.execution_session
                    .entity_cache
                    .borrow_mut()
                    .remove(&RequestEntityCacheKey::admin(version, &id));
            }
        }
        self.store
            .staged
            .owner_parent_observed_field_paths
            .entry(id.clone())
            .or_default()
            .extend(observed_value_field_paths(node));
        match shopify_gid_resource_type(&id) {
            Some("Product") => {
                let merged = self
                    .store
                    .product_by_id(&id)
                    .map(product_state_json)
                    .map(|mut existing| {
                        merge_json_values(&mut existing, node);
                        existing
                    })
                    .unwrap_or_else(|| node.clone());
                self.store.stage_observed_product_json(&merged);
            }
            Some("ProductVariant") => {
                let merged = self
                    .store
                    .product_variant_by_id(&id)
                    .map(product_variant_state_json)
                    .map(|mut existing| {
                        merge_json_values(&mut existing, node);
                        existing
                    })
                    .unwrap_or_else(|| node.clone());
                self.store.stage_observed_product_variant_json(&merged);
            }
            Some("Collection") => {
                let merged = self
                    .store
                    .collection_by_id(&id)
                    .map(|existing| {
                        let mut merged = existing.clone();
                        merge_json_values(&mut merged, node);
                        merged
                    })
                    .unwrap_or_else(|| node.clone());
                self.stage_collection_from_observed_json(&merged);
            }
            Some("Customer") => {
                let merged = self
                    .store
                    .staged
                    .customers
                    .get(&id)
                    .map(|existing| {
                        let mut merged = existing.clone();
                        merge_json_values(&mut merged, node);
                        merged
                    })
                    .unwrap_or_else(|| node.clone());
                self.store.staged.customers.insert(id, merged);
            }
            Some("Order") => {
                let merged = self
                    .store
                    .observed_order_by_id(&id)
                    .map(|existing| {
                        let mut merged = existing.clone();
                        merge_json_values(&mut merged, node);
                        merged
                    })
                    .unwrap_or_else(|| node.clone());
                self.store.staged.orders.insert(id, merged);
            }
            Some("Company") => {
                let merged = self
                    .store
                    .staged
                    .b2b_companies
                    .get(&id)
                    .or_else(|| self.store.base.b2b_companies.get(&id))
                    .map(|existing| {
                        let mut merged = existing.clone();
                        merge_json_values(&mut merged, node);
                        merged
                    })
                    .unwrap_or_else(|| node.clone());
                self.store.staged.b2b_companies.insert(id, merged);
            }
            _ => {}
        }
    }

    pub(crate) fn owner_metafields_set(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let request = invocation.request;
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        let inputs = resolved_object_list_field(&arguments, "metafields");
        let api_client_id = request_app_namespace_api_client_id(request);
        let fallback_reference_ids = if inputs.len() <= METAFIELDS_SET_INPUT_LIMIT {
            self.hydrate_metafield_reference_ids(
                request,
                self.metafields_set_reference_values(&inputs, api_client_id.as_deref()),
            )
        } else {
            BTreeSet::new()
        };
        if inputs.len() <= METAFIELDS_SET_INPUT_LIMIT {
            self.hydrate_owner_metafield_inputs(request, &inputs, api_client_id.as_deref());
        }
        let mut user_errors = if inputs.len() <= METAFIELDS_SET_INPUT_LIMIT {
            self.metafields_set_compare_digest_errors(&inputs, api_client_id.as_deref())
        } else {
            Vec::new()
        };
        user_errors.extend(self.metafields_set_input_errors(
            &inputs,
            api_client_id.as_deref(),
            |id| self.metafield_reference_exists(id) || fallback_reference_ids.contains(id),
        ));
        user_errors
            .extend(self.metafields_set_definition_user_errors(&inputs, api_client_id.as_deref()));
        if !user_errors.is_empty() {
            let metafields = if inputs.len() > METAFIELDS_SET_INPUT_LIMIT {
                Value::Null
            } else {
                json!([])
            };
            let payload = json!({"metafields": metafields, "userErrors": user_errors});
            return ResolverOutcome::value(payload);
        }
        let mut metafields = Vec::new();
        let mut staged_owner_ids = Vec::new();
        for input in inputs {
            let owner_id = resolved_string_field(&input, "ownerId").unwrap_or_default();
            let namespace = canonical_app_metafield_namespace(
                resolved_string_field(&input, "namespace").as_deref(),
                api_client_id.as_deref(),
            );
            let key = resolved_string_field(&input, "key").unwrap_or_default();
            let owner_type = owner_type_from_gid(&owner_id);
            let definition = self.owner_metafield_definition(&owner_type, &namespace, &key);
            let metafield_type = self
                .metafields_set_effective_type(&input, api_client_id.as_deref())
                .unwrap_or_else(|| "single_line_text_field".to_string());
            let value = resolved_string_field(&input, "value").unwrap_or_default();
            let index = self.next_owner_metafield_index(metafields.len());
            let existing = self.owner_metafield(&owner_id, &namespace, &key);
            let metafield = owner_metafield_record(OwnerMetafieldRecordArgs {
                owner_id: &owner_id,
                namespace: &namespace,
                key: &key,
                metafield_type: &metafield_type,
                value: &value,
                index,
                existing: existing.as_ref(),
                include_owner: true,
                definition: definition.unwrap_or(Value::Null),
            });
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
            self.sync_cart_transform_owner_metafields(&staged_owner_ids);
            metafields.push(metafield);
        }
        let payload = json!({"metafields": metafields, "userErrors": []});
        ResolverOutcome::value(payload).with_log_draft(LogDraft::staged(
            "metafieldsSet",
            "products",
            staged_owner_ids,
        ))
    }

    pub(crate) fn owner_metafields_delete(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let request = invocation.request;
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        let inputs = resolved_object_list_field(&arguments, "metafields");
        let api_client_id = request_app_namespace_api_client_id(request);
        if let Some(index) = inputs.iter().position(|input| {
            app_metafield_namespace_requires_api_client(
                resolved_string_field(input, "namespace").as_deref(),
            ) && api_client_id.is_none()
        }) {
            let payload = json!({
                "deletedMetafields": [],
                "userErrors": [user_error_omit_code(
                    vec!["metafields".to_string(), index.to_string(), "namespace".to_string()],
                    APP_NAMESPACE_IDENTITY_REQUIRED_MESSAGE,
                    None,
                )]
            });
            return ResolverOutcome::value(payload);
        }
        // A delete targeting another app's reserved namespace is not permitted;
        // Shopify rejects the whole batch before deleting anything.
        if inputs.iter().any(|input| {
            app_namespace_belongs_to_other_app(
                &canonical_app_metafield_namespace(
                    resolved_string_field(input, "namespace").as_deref(),
                    api_client_id.as_deref(),
                ),
                api_client_id.as_deref(),
            )
        }) {
            let payload = json!({
                "deletedMetafields": [],
                "userErrors": [user_error_omit_code(
                    ["metafields"],
                    "Access to this namespace and key on Metafields for this resource type is not allowed.",
                    None,
                )]
            });
            return ResolverOutcome::value(payload);
        }
        self.hydrate_owner_metafield_inputs(request, &inputs, api_client_id.as_deref());
        let mut deleted = Vec::new();
        let mut staged_owner_ids = Vec::new();
        for input in inputs {
            let owner_id = resolved_string_field(&input, "ownerId").unwrap_or_default();
            let namespace = canonical_app_metafield_namespace(
                resolved_string_field(&input, "namespace").as_deref(),
                api_client_id.as_deref(),
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
        ResolverOutcome::value(payload).with_log_draft(LogDraft::staged(
            "metafieldsDelete",
            "products",
            staged_owner_ids,
        ))
    }

    fn metafields_set_compare_digest_errors(
        &self,
        inputs: &[BTreeMap<String, ResolvedValue>],
        api_client_id: Option<&str>,
    ) -> Vec<Value> {
        inputs
            .iter()
            .enumerate()
            .filter_map(|(index, input)| {
                let compare_digest = input.get("compareDigest")?;
                let owner_id = resolved_string_field(input, "ownerId")?;
                let namespace = canonical_app_metafield_namespace(
                    resolved_string_field(input, "namespace").as_deref(),
                    api_client_id,
                );
                let key = resolved_string_field(input, "key")?;
                let existing = self.owner_metafield(&owner_id, &namespace, &key);
                match compare_digest {
                    ResolvedValue::String(supplied) => {
                        let Some(existing) = existing else {
                            return Some(metafields_set_row_user_error(
                                index,
                                "INVALID_COMPARE_DIGEST",
                                "Invalid `compareDigest` value.",
                            ));
                        };
                        let current_digest =
                            owner_metafield_compare_digest(&existing).unwrap_or_default();
                        if supplied == &current_digest {
                            None
                        } else {
                            Some(metafields_set_row_user_error(
                                index,
                                "STALE_OBJECT",
                                "The resource has been updated since it was loaded. Try again with an updated `compareDigest` value.",
                            ))
                        }
                    }
                    ResolvedValue::Null => existing.map(|_| {
                        metafields_set_row_user_error(
                            index,
                            "STALE_OBJECT",
                            "The resource has been updated since it was loaded. Try again with an updated `compareDigest` value.",
                        )
                    }),
                    _ => None,
                }
            })
            .collect()
    }

    pub(in crate::proxy) fn should_handle_owner_metafields_read(
        &self,
        fields: &[RootFieldSelection],
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> bool {
        let mut has_non_product_owner_read = false;
        let mut needs_live_product_hydration = false;
        for field in fields {
            if !Self::owner_field_selects_metafields_at_root(&field.name, &field.selection) {
                continue;
            }
            if self.config.read_mode == ReadMode::LiveHybrid {
                let owner_id = self.owner_field_id(field, variables);
                let cold = self.owner_needs_metafield_hydration(&field.name, &owner_id);
                let has_local_effect = self.owner_has_metafield_local_effects(&owner_id);
                let requested_field_paths = selected_field_paths(&field.selection);
                let unresolved_or_incomplete_parent = has_local_effect
                    && (!self.owner_parent_record_exists(&owner_id)
                        || (self.owner_parent_is_partial(&owner_id)
                            && !self.owner_parent_shape_is_complete(
                                &owner_id,
                                &requested_field_paths,
                            )));
                // A cold (unstaged) owner that also selects sub-resources the
                // metafields overlay cannot synthesize (addresses, orders, events, ...)
                // must forward the whole read upstream as a passthrough rather than be
                // answered with a metafields-only projection that silently drops them.
                // Once a child effect exists, however, the original read must hydrate
                // the parent and then apply that local overlay instead of losing it to
                // full passthrough.
                if cold
                    && !Self::owner_metafields_read_selection_is_metafields_only(&field.selection)
                    && !unresolved_or_incomplete_parent
                {
                    continue;
                }
            }
            match field.name.as_str() {
                "collection" | "customer" | "order" | "company" => {
                    has_non_product_owner_read = true;
                }
                "shop" => {
                    let owner_id = self.owner_field_id(field, variables);
                    if !owner_id.is_empty() && self.owner_has_metafield_local_effects(&owner_id) {
                        has_non_product_owner_read = true;
                    }
                }
                "product" | "productVariant" if self.config.read_mode == ReadMode::LiveHybrid => {
                    let owner_id = self.owner_field_id(field, variables);
                    if self.owner_needs_metafield_hydration(&field.name, &owner_id) {
                        needs_live_product_hydration = true;
                    }
                }
                _ => {}
            }
        }
        has_non_product_owner_read || needs_live_product_hydration
    }

    /// True when an owner read selects only fields the metafields overlay can synthesize
    /// for a cold (unstaged) owner: `id`, `__typename`, `metafield`, `metafields`. Any other
    /// field (addresses, orders, events, ...) cannot be projected from an empty base, so the
    /// read must instead forward upstream as a full passthrough.
    fn owner_metafields_read_selection_is_metafields_only(selections: &[SelectedField]) -> bool {
        selections.iter().all(|selection| {
            matches!(
                selection.name.as_str(),
                "id" | "__typename" | "metafield" | "metafields"
            )
        })
    }

    pub(in crate::proxy) fn hydrate_owner_metafield_read_fields(
        &mut self,
        request: &Request,
        fields: &[RootFieldSelection],
        variables: &BTreeMap<String, ResolvedValue>,
    ) {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return;
        }
        let requested_owner_ids = fields
            .iter()
            .filter(|field| {
                Self::owner_field_selects_metafields_at_root(&field.name, &field.selection)
            })
            .map(|field| self.owner_field_id(field, variables))
            .filter(|owner_id| !owner_id.is_empty())
            .collect::<Vec<_>>();
        self.execution_session
            .owner_metafield_read_ids
            .extend(requested_owner_ids);
        // A read operation already contains the exact aliases, arguments, and
        // windows needed to hydrate its owner fields. Execute that document
        // once through the request cache instead of synthesizing a second
        // query, then observe each canonical owner before the engine resolves
        // local overlays.
        let response = self.cached_or_forward_upstream_response(request);
        if (200..300).contains(&response.status) {
            let canonical_data = self
                .execution_session
                .upstream_query_data
                .as_ref()
                .and_then(Value::as_object);
            let observed = fields
                .iter()
                .filter(|field| {
                    Self::owner_field_selects_metafields_at_root(&field.name, &field.selection)
                })
                .filter_map(|field| {
                    let owner_id = self.owner_field_id(field, variables);
                    let node = canonical_data.and_then(|data| data.get(&field.response_key))?;
                    Some((owner_id, node.clone()))
                })
                .collect::<Vec<_>>();
            for (owner_id, node) in observed {
                if !owner_id.is_empty() {
                    self.execution_session
                        .owner_metafield_hydrated_ids
                        .insert(owner_id.clone());
                    if node.is_null() {
                        self.execution_session
                            .owner_metafield_missing_ids
                            .insert(owner_id);
                        continue;
                    }
                }
                if node.is_object() {
                    self.stage_observed_owner_metafield_node(&node);
                }
            }
            return;
        }

        // Preserve the narrow hydration fallback for transports that cannot
        // execute the complete read document.
        let api_client_id = request_app_namespace_api_client_id(request);
        let mut shape = OwnerMetafieldHydrationShape::default();
        let ids = fields
            .iter()
            .filter(|field| {
                Self::owner_field_selects_metafields_at_root(&field.name, &field.selection)
            })
            .flat_map(|field| {
                shape.extend_from_owner_selections(&field.selection, api_client_id.as_deref());
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
        self.hydrate_owner_metafield_ids(request, ids, shape);
    }

    fn hydrate_owner_metafield_inputs(
        &mut self,
        request: &Request,
        inputs: &[BTreeMap<String, ResolvedValue>],
        api_client_id: Option<&str>,
    ) {
        let mut shape = OwnerMetafieldHydrationShape::default();
        let ids = inputs
            .iter()
            .filter_map(|input| {
                let owner_id = resolved_string_field(input, "ownerId")?;
                shopify_gid_resource_type(&owner_id)?;
                let namespace = canonical_app_metafield_namespace(
                    resolved_string_field(input, "namespace").as_deref(),
                    api_client_id,
                );
                let key = resolved_string_field(input, "key").unwrap_or_default();
                if self.owner_metafield_has_local_effect(&owner_id, &namespace, &key) {
                    return None;
                }
                if !namespace.is_empty() && !key.is_empty() {
                    shape.push_metafield(namespace, key);
                }
                Some(owner_id)
            })
            .collect::<Vec<_>>();
        self.hydrate_owner_metafield_ids(request, ids, shape);
    }

    fn hydrate_owner_metafield_ids(
        &mut self,
        request: &Request,
        ids: Vec<String>,
        shape: OwnerMetafieldHydrationShape,
    ) {
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
        let ids = ids
            .into_iter()
            .filter(|id| {
                !self
                    .execution_session
                    .owner_metafield_hydrated_ids
                    .contains(id)
            })
            .collect::<Vec<_>>();
        let Some((query, variables)) = owner_metafield_hydrate_request(ids.clone(), &shape) else {
            return;
        };
        let response = self.upstream_post(
            request,
            json!({
                "query": query,
                "operationName": "OwnerMetafieldsHydrateNodes",
                "variables": variables,
            }),
        );
        if response.status >= 400 {
            return;
        }
        if let Some(nodes) = response.body["data"]["nodes"].as_array() {
            self.execution_session
                .owner_metafield_hydrated_ids
                .extend(ids);
            for node in nodes {
                self.stage_observed_owner_metafield_node(node);
            }
        }
    }

    fn hydrate_metafield_reference_ids(
        &mut self,
        request: &Request,
        ids: Vec<String>,
    ) -> BTreeSet<String> {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return BTreeSet::new();
        }
        let mut ids = ids
            .into_iter()
            .filter(|id| !id.is_empty() && !self.metafield_reference_exists(id))
            .collect::<Vec<_>>();
        ids.sort();
        ids.dedup();
        if ids.is_empty() {
            return BTreeSet::new();
        }

        let mut product_domain_ids = Vec::new();
        let mut generic_ids = Vec::new();
        for id in ids {
            match shopify_gid_resource_type(&id) {
                Some("Product" | "ProductVariant" | "Collection") => product_domain_ids.push(id),
                _ => generic_ids.push(id),
            }
        }
        if !product_domain_ids.is_empty() {
            let response = self.upstream_post(
                request,
                json!({
                    "query": PRODUCTS_HYDRATE_NODES_OBSERVATION_QUERY,
                    "operationName": "ProductsHydrateNodes",
                    "variables": { "ids": product_domain_ids.clone() }
                }),
            );
            if response.status >= 400 {
                return BTreeSet::new();
            } else {
                self.observe_nodes_response(&response);
            }
        }
        if generic_ids.is_empty() {
            return BTreeSet::new();
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": "query MetafieldReferenceHydrateNodes($ids: [ID!]!) { nodes(ids: $ids) { id __typename } }",
                "operationName": "MetafieldReferenceHydrateNodes",
                "variables": { "ids": generic_ids },
            }),
        );
        if response.status >= 400 {
            return BTreeSet::new();
        }
        if let Some(nodes) = response.body["data"]["nodes"].as_array() {
            for node in nodes {
                self.stage_metafield_reference_node(node);
            }
        }
        BTreeSet::new()
    }

    fn stage_metafield_reference_node(&mut self, node: &Value) {
        let Some(id) = node
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .map(str::to_string)
        else {
            return;
        };
        self.store.staged.metafield_reference_ids.insert(id.clone());
        match shopify_gid_resource_type(&id) {
            Some("Product") => self.store.stage_observed_product_json(node),
            Some("ProductVariant") => {
                if let Some(variant) = product_variant_state_from_observed_json(node) {
                    self.store.stage_product_variant(variant);
                }
            }
            Some("Collection") => {
                self.store
                    .staged
                    .collections
                    .entry(id)
                    .or_insert_with(|| node.clone());
            }
            Some("Customer") => {
                self.store
                    .staged
                    .customers
                    .entry(id)
                    .or_insert_with(|| node.clone());
            }
            Some("Order") => {
                self.store
                    .staged
                    .orders
                    .entry(id)
                    .or_insert_with(|| node.clone());
            }
            Some("Company") => {
                self.store
                    .staged
                    .b2b_companies
                    .entry(id)
                    .or_insert_with(|| node.clone());
            }
            Some("Metaobject") => {
                if !self.store.staged.metaobjects.is_tombstoned(&id) {
                    self.store
                        .staged
                        .metaobjects
                        .entry(id)
                        .or_insert_with(|| node.clone());
                }
            }
            Some("MediaImage" | "Video" | "ExternalVideo" | "Model3d" | "GenericFile") => {
                if let Some(record) = media_file_record_from_node(node) {
                    self.store.staged.media_files.entry(id).or_insert(record);
                }
            }
            _ => {}
        }
    }

    fn metafield_reference_exists(&self, id: &str) -> bool {
        if self.store.staged.metafield_reference_ids.contains(id) {
            return true;
        }
        match shopify_gid_resource_type(id) {
            Some("Product") => self.store.product_by_id(id).is_some(),
            Some("ProductVariant") => self.store.product_variant_by_id(id).is_some(),
            Some("Collection") => self.store.collection_by_id(id).is_some(),
            Some("Customer") => {
                self.store.staged.customers.contains_key(id)
                    && !self.store.staged.customers.is_tombstoned(id)
            }
            Some("Order") => {
                self.store.staged.orders.contains_key(id)
                    && !self.store.staged.orders.is_tombstoned(id)
            }
            Some("Company") => self.store.staged.b2b_companies.contains_key(id),
            Some("Metaobject") => self.metaobject_by_id(id).is_some(),
            Some("MediaImage" | "Video" | "ExternalVideo" | "Model3d" | "GenericFile") => {
                self.store.staged.media_files.contains_key(id)
                    && !self.store.staged.media_files.is_tombstoned(id)
            }
            _ => false,
        }
    }

    fn owner_needs_metafield_hydration(&self, root_field: &str, owner_id: &str) -> bool {
        if self
            .execution_session
            .owner_metafield_hydrated_ids
            .contains(owner_id)
        {
            return false;
        }
        if owner_id.is_empty() || is_synthetic_gid(owner_id) {
            return false;
        }
        match root_field {
            // A partially observed entity is not proof that the metafield
            // selection requested by this operation is authoritative. Real
            // Shopify IDs remain hydratable; local tombstones still win.
            "product" => !self.store.product_is_tombstoned(owner_id),
            "productVariant" => !self.store.product_variants.staged.is_tombstoned(owner_id),
            "collection" => !self.store.collection_is_deleted(owner_id),
            "customer" => !self.store.staged.customers.is_tombstoned(owner_id),
            "order" => !self.store.staged.orders.is_tombstoned(owner_id),
            "company" | "shop" => true,
            _ => false,
        }
    }

    pub(in crate::proxy) fn stage_observed_owner_metafield_node(&mut self, node: &Value) {
        let Some(owner_id) = node.get("id").and_then(Value::as_str).map(str::to_string) else {
            return;
        };
        self.stage_observed_owner_parent_record(node);
        if shopify_gid_resource_type(&owner_id) == Some("ProductVariant") {
            if let Some(product) = node.get("product") {
                self.stage_observed_owner_parent_record(product);
            }
        } else if shopify_gid_resource_type(&owner_id) == Some("Shop") {
            self.store.base.shop =
                shallow_merged_object(self.store.base.shop.clone(), node.clone());
        }
        self.stage_observed_owner_metafields(&owner_id, node);
        if shopify_gid_resource_type(&owner_id) == Some("Product") {
            for variant in node
                .get("variants")
                .map(connection_nodes)
                .unwrap_or_default()
                .iter()
            {
                if let Some(variant_id) = variant.get("id").and_then(Value::as_str) {
                    let mut variant = variant.clone();
                    if let Some(object) = variant.as_object_mut() {
                        object.insert("productId".to_string(), json!(owner_id));
                    }
                    self.stage_observed_owner_parent_record(&variant);
                    self.stage_observed_owner_metafields(variant_id, &variant);
                }
            }
        }
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

    pub(in crate::proxy) fn stage_observed_owner_metafields(
        &mut self,
        owner_id: &str,
        node: &Value,
    ) {
        let mut records = node
            .get("metafields")
            .map(connection_nodes)
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
            let mut connection_records = value
                .get("nodes")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            if let Some(page_info) = value.get("pageInfo") {
                apply_metafield_connection_cursors(&mut connection_records, page_info);
            }
            records.extend(connection_records.into_iter().filter(|value| {
                value.get("namespace").and_then(Value::as_str).is_some()
                    && value.get("key").and_then(Value::as_str).is_some()
                    && value.get("id").and_then(Value::as_str).is_some()
            }));
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

    pub(in crate::proxy) fn replace_owner_metafields_from_connection(
        &mut self,
        owner_id: &str,
        connection: &Value,
    ) {
        let mut records = connection
            .get("nodes")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if let Some(page_info) = connection.get("pageInfo") {
            apply_metafield_connection_cursors(&mut records, page_info);
        }
        self.store
            .staged
            .owner_metafields
            .insert(owner_id.to_string(), Vec::new());
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
        if record.get("compareDigest").is_none() {
            if let Some(value) = record.get("value").and_then(Value::as_str) {
                record["compareDigest"] = json!(metafield_compare_digest(value));
            }
        }
        record["definition"] = self.owner_metafield_definition_value(owner_id, &namespace, &key);
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
    pub(super) fn stage_input_variant_metafields(
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
            let definition = self.owner_metafield_definition_value(owner_id, &namespace, &key);
            let index = self.next_owner_metafield_index(0);
            let record = owner_metafield_record(OwnerMetafieldRecordArgs {
                owner_id,
                namespace: &namespace,
                key: &key,
                metafield_type: &metafield_type,
                value: &value,
                index,
                existing: None,
                include_owner: false,
                definition,
            });
            self.upsert_owner_metafield_record(owner_id, record);
        }
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
        if field.name == "shop" {
            return self.shop_owner_id_for_read().unwrap_or_default();
        }
        field
            .arguments
            .get("id")
            .and_then(resolved_value_string)
            .or_else(|| resolved_string_field(variables, "id"))
            .or_else(|| resolved_string_field(variables, "productId"))
            .or_else(|| resolved_string_field(variables, "variantId"))
            .or_else(|| resolved_string_field(variables, "collectionId"))
            .or_else(|| resolved_string_field(variables, "customerId"))
            .or_else(|| resolved_string_field(variables, "orderId"))
            .or_else(|| resolved_string_field(variables, "companyId"))
            .unwrap_or_default()
    }

    fn shop_owner_id_for_read(&self) -> Option<String> {
        self.store
            .effective_shop()
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| shopify_gid_resource_type(id) == Some("Shop"))
            .map(str::to_string)
            .or_else(|| {
                self.store
                    .staged
                    .owner_metafields
                    .keys()
                    .find(|id| shopify_gid_resource_type(id) == Some("Shop"))
                    .cloned()
            })
            .or_else(|| {
                self.store
                    .staged
                    .deleted_owner_metafields
                    .iter()
                    .find_map(|(owner_id, _, _)| {
                        (shopify_gid_resource_type(owner_id) == Some("Shop"))
                            .then(|| owner_id.clone())
                    })
            })
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
            .map(|metafield| self.owner_metafield_with_effective_definition(owner_id, metafield))
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

    pub(in crate::proxy) fn owner_has_metafield_local_effects(&self, owner_id: &str) -> bool {
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

    fn sync_cart_transform_owner_metafields(&mut self, owner_ids: &[String]) {
        for owner_id in owner_ids {
            if shopify_gid_resource_type(owner_id) != Some("CartTransform") {
                continue;
            }
            let Some(record) = self.store.staged.function_cart_transforms.get_mut(owner_id) else {
                continue;
            };
            let metafields = self
                .store
                .staged
                .owner_metafields
                .get(owner_id)
                .cloned()
                .unwrap_or_default();
            let first_metafield = metafields.first().cloned().unwrap_or(Value::Null);
            record["metafields"] = json!({ "nodes": metafields });
            if first_metafield.is_null() {
                record.as_object_mut().unwrap().remove("metafield");
            } else {
                record["metafield"] = first_metafield;
            }
            if self
                .store
                .staged
                .function_cart_transform
                .as_ref()
                .and_then(|current| current.get("id"))
                .and_then(Value::as_str)
                == Some(owner_id.as_str())
            {
                self.store.staged.function_cart_transform = Some(record.clone());
            }
        }
    }

    pub(in crate::proxy) fn owner_metafields(
        &self,
        owner_id: &str,
        namespace: Option<&str>,
        keys: Option<&[(String, String)]>,
    ) -> Vec<Value> {
        let mut records = self
            .store
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
                    && keys.is_none_or(|keys| {
                        matches!(
                            (metafield_namespace, metafield_key),
                            (Some(namespace), Some(key))
                                if keys.iter().any(|(filter_namespace, filter_key)| {
                                    filter_namespace == namespace && filter_key == key
                                })
                        )
                    })
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
            .map(|metafield| self.owner_metafield_with_effective_definition(owner_id, metafield))
            .collect::<Vec<_>>();
        if let Some(keys) = keys {
            records.sort_by_key(|metafield| owner_metafield_key_position(metafield, keys));
        }
        records
    }

    /// Resolve one owner metafield without consulting a GraphQL selection.
    /// The dynamic schema executor owns projection of the returned canonical
    /// record, including aliases and nested fields.
    pub(in crate::proxy) fn canonical_owner_metafield_value(
        &self,
        owner_id: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
        api_client_id: Option<&str>,
    ) -> Value {
        let namespace = owner_metafield_read_namespace(arguments, api_client_id);
        let key = resolved_string_field(arguments, "key").unwrap_or_default();
        if namespace.is_empty() || key.is_empty() {
            return Value::Null;
        }
        self.owner_metafield(owner_id, &namespace, &key)
            .unwrap_or(Value::Null)
    }

    pub(in crate::proxy) fn canonical_embedded_or_owner_metafield_value(
        &self,
        parent: &Value,
        arguments: &BTreeMap<String, ResolvedValue>,
        api_client_id: Option<&str>,
    ) -> Value {
        let namespace = owner_metafield_read_namespace(arguments, api_client_id);
        let key = resolved_string_field(arguments, "key").unwrap_or_default();
        if namespace.is_empty() || key.is_empty() {
            return Value::Null;
        }
        let owner_id = parent.get("id").and_then(Value::as_str);
        if owner_id.is_some_and(|owner_id| {
            self.owner_metafield_has_local_effect(owner_id, &namespace, &key)
        }) {
            return self.canonical_owner_metafield_value(
                owner_id.unwrap_or_default(),
                arguments,
                api_client_id,
            );
        }
        let embedded = parent["metafields"]
            .as_array()
            .cloned()
            .unwrap_or_else(|| connection_nodes(&parent["metafields"]))
            .into_iter()
            .chain(
                parent
                    .get("metafield")
                    .filter(|value| value.is_object())
                    .cloned(),
            )
            .find(|metafield| {
                metafield.get("namespace").and_then(Value::as_str) == Some(namespace.as_str())
                    && metafield.get("key").and_then(Value::as_str) == Some(key.as_str())
            });
        if let Some(metafield) = embedded {
            return metafield;
        }
        owner_id
            .map(|owner_id| {
                self.canonical_owner_metafield_value(owner_id, arguments, api_client_id)
            })
            .unwrap_or(Value::Null)
    }

    /// Resolve a complete owner-metafield connection from store state. Search,
    /// key ordering, reverse, and cursor windows are applied before the engine
    /// projects nodes/edges/pageInfo.
    pub(in crate::proxy) fn canonical_owner_metafields_connection_value(
        &self,
        owner_id: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
        api_client_id: Option<&str>,
    ) -> Value {
        let namespace = owner_metafields_connection_namespace(arguments, api_client_id);
        let keys = owner_metafields_connection_keys_with_app_namespace(arguments, api_client_id);
        let mut records = self.owner_metafields(owner_id, namespace.as_deref(), keys.as_deref());
        if resolved_bool_field(arguments, "reverse").unwrap_or(false) {
            records.reverse();
        }
        let (records, page_info) = connection_window(&records, arguments, |record| {
            metafield_cursor(record).unwrap_or_default()
        });
        let records = if keys.is_some() {
            records
                .into_iter()
                .map(owner_metafield_with_connection_key)
                .collect()
        } else {
            records
        };
        connection_json_with_cursor(
            records,
            |_, record| metafield_cursor(record).unwrap_or_default(),
            page_info,
        )
    }

    pub(in crate::proxy) fn canonical_embedded_or_owner_metafields_connection_value(
        &self,
        parent: &Value,
        arguments: &BTreeMap<String, ResolvedValue>,
        api_client_id: Option<&str>,
    ) -> Value {
        let owner_id = parent.get("id").and_then(Value::as_str);
        if let Some(owner_id) = owner_id {
            if self.owner_has_metafield_local_effects(owner_id) {
                return self.canonical_owner_metafields_connection_value(
                    owner_id,
                    arguments,
                    api_client_id,
                );
            }
        }
        if let Some(connection) = parent.get("metafields").filter(|value| value.is_object()) {
            // With no local overlay, the observed connection is authoritative
            // for its exact arguments, cursors, and pageInfo. Let the GraphQL
            // engine project it without rebuilding transport metadata.
            return connection.clone();
        }
        if let Some(records) = parent.get("metafields").and_then(Value::as_array) {
            // Local mutation payloads commonly retain relationship source data
            // as a canonical record list. Normalize that list to the schema's
            // connection type; only upstream connection objects are safe to
            // return verbatim.
            let namespace = owner_metafields_connection_namespace(arguments, api_client_id);
            let keys =
                owner_metafields_connection_keys_with_app_namespace(arguments, api_client_id);
            let mut records = records
                .iter()
                .filter(|metafield| {
                    namespace.as_deref().is_none_or(|namespace| {
                        metafield.get("namespace").and_then(Value::as_str) == Some(namespace)
                    })
                })
                .filter(|metafield| {
                    keys.as_deref().is_none_or(|keys| {
                        owner_metafield_key_position(metafield, keys) != usize::MAX
                    })
                })
                .cloned()
                .collect::<Vec<_>>();
            if let Some(keys) = keys.as_deref() {
                records.sort_by_key(|metafield| owner_metafield_key_position(metafield, keys));
            }
            if resolved_bool_field(arguments, "reverse").unwrap_or(false) {
                records.reverse();
            }
            let (records, page_info) = connection_window(&records, arguments, |metafield| {
                metafield_cursor(metafield).unwrap_or_default()
            });
            let records = if keys.is_some() {
                records
                    .into_iter()
                    .map(owner_metafield_with_connection_key)
                    .collect()
            } else {
                records
            };
            return connection_json_with_cursor(
                records,
                |_, metafield| metafield_cursor(metafield).unwrap_or_default(),
                page_info,
            );
        }
        owner_id
            .map(|owner_id| {
                self.canonical_owner_metafields_connection_value(owner_id, arguments, api_client_id)
            })
            .unwrap_or_else(|| connection_json(Vec::new()))
    }

    pub(in crate::proxy) fn canonical_metafield_reference_value(
        &self,
        record: &Value,
        request: Option<&Request>,
    ) -> Value {
        if let Some(existing) = record.get("reference") {
            return existing.clone();
        }
        scalar_reference_id(record)
            .and_then(|id| self.canonical_metafield_reference_node(&id, request))
            .unwrap_or(Value::Null)
    }

    pub(in crate::proxy) fn canonical_metafield_references_connection_value(
        &self,
        record: &Value,
        arguments: &BTreeMap<String, ResolvedValue>,
        request: Option<&Request>,
    ) -> Value {
        let nodes =
            if let Some(existing) = record.get("references").filter(|value| value.is_object()) {
                connection_nodes(existing)
            } else {
                list_reference_ids(record)
                    .into_iter()
                    .filter_map(|id| self.canonical_metafield_reference_node(&id, request))
                    .collect()
            };
        connection_value_with_args(nodes, arguments, |node| {
            node.get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        })
    }

    fn canonical_metafield_reference_node(
        &self,
        id: &str,
        request: Option<&Request>,
    ) -> Option<Value> {
        match self.request_entity_load_state(ApiSurface::Admin, id, request) {
            crate::node_resolver_inventory::NodeLoadState::Found(entity) => Some(entity.value),
            crate::node_resolver_inventory::NodeLoadState::KnownMissing
            | crate::node_resolver_inventory::NodeLoadState::NeedsHydration
            | crate::node_resolver_inventory::NodeLoadState::UnsupportedType => None,
        }
    }

    pub(in crate::proxy) fn owner_metafield_definition(
        &self,
        owner_type: &str,
        namespace: &str,
        key: &str,
    ) -> Option<Value> {
        self.effective_metafield_definition(owner_type, namespace, key)
    }

    fn owner_metafield_definition_value(
        &self,
        owner_id: &str,
        namespace: &str,
        key: &str,
    ) -> Value {
        let owner_type = owner_type_from_gid(owner_id);
        self.owner_metafield_definition(&owner_type, namespace, key)
            .unwrap_or(Value::Null)
    }

    fn owner_metafield_with_effective_definition(
        &self,
        owner_id: &str,
        mut metafield: Value,
    ) -> Value {
        let namespace = metafield
            .get("namespace")
            .and_then(Value::as_str)
            .map(str::to_string);
        let key = metafield
            .get("key")
            .and_then(Value::as_str)
            .map(str::to_string);
        if let (Some(namespace), Some(key)) = (namespace, key) {
            metafield["definition"] =
                self.owner_metafield_definition_value(owner_id, &namespace, &key);
        } else if metafield.get("definition").is_none() {
            metafield["definition"] = Value::Null;
        }
        metafield
    }

    /// Stage owner metafields supplied through a `metafields` create/update input so that
    /// downstream `metafield`/`metafields` reads resolve them on the owning resource.
    pub(in crate::proxy) fn stage_owner_metafields_from_input(
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
            let definition = self.owner_metafield_definition_value(owner_id, &namespace, &key);
            let index = self.next_owner_metafield_index(0);
            let metafield = owner_metafield_record(OwnerMetafieldRecordArgs {
                owner_id,
                namespace: &namespace,
                key: &key,
                metafield_type: &metafield_type,
                value: &value,
                index,
                existing: None,
                include_owner: true,
                definition,
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

    pub(in crate::proxy) fn stage_owner_metafield_value(
        &mut self,
        owner_id: &str,
        namespace: &str,
        key: &str,
        metafield_type: &str,
        value: &str,
    ) -> Value {
        let definition = self.owner_metafield_definition_value(owner_id, namespace, key);
        let existing = self.owner_metafield(owner_id, namespace, key);
        let index = self.next_owner_metafield_index(0);
        let metafield = owner_metafield_record(OwnerMetafieldRecordArgs {
            owner_id,
            namespace,
            key,
            metafield_type,
            value,
            index,
            existing: existing.as_ref(),
            include_owner: true,
            definition,
        });
        self.store.staged.deleted_owner_metafields.remove(&(
            owner_id.to_string(),
            namespace.to_string(),
            key.to_string(),
        ));
        let owner_metafields = self
            .store
            .staged
            .owner_metafields
            .entry(owner_id.to_string())
            .or_default();
        if let Some(existing) = owner_metafields.iter_mut().find(|existing| {
            existing.get("namespace").and_then(Value::as_str) == Some(namespace)
                && existing.get("key").and_then(Value::as_str) == Some(key)
        }) {
            *existing = metafield.clone();
        } else {
            owner_metafields.push(metafield.clone());
        }
        metafield
    }
}

impl DraftProxy {
    fn next_owner_metafield_index(&self, pending_offset: usize) -> usize {
        self.store
            .staged
            .owner_metafields
            .values()
            .map(Vec::len)
            .sum::<usize>()
            + pending_offset
            + 1
    }
}

struct OwnerMetafieldRecordArgs<'a> {
    owner_id: &'a str,
    namespace: &'a str,
    key: &'a str,
    metafield_type: &'a str,
    value: &'a str,
    index: usize,
    existing: Option<&'a Value>,
    include_owner: bool,
    definition: Value,
}

fn owner_metafield_record(
    OwnerMetafieldRecordArgs {
        owner_id,
        namespace,
        key,
        metafield_type,
        value,
        index,
        existing,
        include_owner,
        definition,
    }: OwnerMetafieldRecordArgs<'_>,
) -> Value {
    let normalized_value = normalize_metafield_value_string(metafield_type, value);
    let timestamp = product_mutation_timestamp(index as u64);
    let created_at = existing
        .and_then(|metafield| metafield.get("createdAt"))
        .and_then(Value::as_str)
        .unwrap_or(&timestamp);
    let updated_at = existing
        .filter(|metafield| {
            metafield.get("value").and_then(Value::as_str) == Some(normalized_value.as_str())
        })
        .and_then(|metafield| metafield.get("updatedAt"))
        .and_then(Value::as_str)
        .unwrap_or(&timestamp);
    let mut record = json!({
        "id": existing
            .and_then(|metafield| metafield.get("id"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| shopify_gid("Metafield", index)),
        "namespace": namespace,
        "key": key,
        "type": metafield_type,
        "value": normalized_value,
        "jsonValue": metafield_json_value(metafield_type, &normalized_value),
        "compareDigest": metafield_compare_digest(&normalized_value),
        "createdAt": created_at,
        "updatedAt": updated_at,
        "ownerType": owner_type_from_gid(owner_id),
        "definition": definition,
    });
    if include_owner {
        record["owner"] = owner_reference_from_gid(owner_id);
    }
    record
}

fn owner_reference_from_gid(owner_id: &str) -> Value {
    json!({
        "__typename": metafield_owner_gid_resource_type(owner_id),
        "id": owner_id
    })
}

fn reference_type_allows_node_resolution(field_type: &str) -> bool {
    field_type == "mixed_reference" || field_type.ends_with("_reference")
}

pub(in crate::proxy) fn scalar_reference_id(record: &Value) -> Option<String> {
    let field_type = record.get("type").and_then(Value::as_str)?;
    if field_type.starts_with("list.") || !reference_type_allows_node_resolution(field_type) {
        return None;
    }
    record
        .get("value")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(in crate::proxy) fn list_reference_ids(record: &Value) -> Vec<String> {
    let Some(inner_type) = record
        .get("type")
        .and_then(Value::as_str)
        .and_then(|field_type| field_type.strip_prefix("list."))
    else {
        return Vec::new();
    };
    if !reference_type_allows_node_resolution(inner_type) {
        return Vec::new();
    }
    record
        .get("jsonValue")
        .and_then(reference_id_array)
        .or_else(|| {
            record
                .get("value")
                .and_then(Value::as_str)
                .and_then(|value| serde_json::from_str::<Value>(value).ok())
                .as_ref()
                .and_then(reference_id_array)
        })
        .unwrap_or_default()
}

fn reference_id_array(value: &Value) -> Option<Vec<String>> {
    Some(
        value
            .as_array()?
            .iter()
            .filter_map(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect(),
    )
}

fn owner_metafield_read_namespace(
    arguments: &BTreeMap<String, ResolvedValue>,
    api_client_id: Option<&str>,
) -> String {
    resolved_string_field(arguments, "namespace")
        .map(|namespace| canonical_app_metafield_read_namespace(&namespace, api_client_id))
        .unwrap_or_else(|| canonical_app_metafield_namespace(None, api_client_id))
}

fn owner_metafields_connection_namespace(
    arguments: &BTreeMap<String, ResolvedValue>,
    api_client_id: Option<&str>,
) -> Option<String> {
    resolved_string_field(arguments, "namespace")
        .map(|namespace| canonical_app_metafield_read_namespace(&namespace, api_client_id))
}

fn canonical_app_metafield_read_namespace(namespace: &str, api_client_id: Option<&str>) -> String {
    if namespace == "$app" || namespace.starts_with("$app:") {
        canonical_app_metafield_namespace(Some(namespace), api_client_id)
    } else {
        namespace.to_string()
    }
}

pub(super) fn owner_metafields_connection_keys(
    arguments: &BTreeMap<String, ResolvedValue>,
) -> Option<Vec<(String, String)>> {
    owner_metafields_connection_keys_with_app_namespace(arguments, None)
}

fn owner_metafields_connection_keys_with_app_namespace(
    arguments: &BTreeMap<String, ResolvedValue>,
    api_client_id: Option<&str>,
) -> Option<Vec<(String, String)>> {
    match arguments.get("keys") {
        None | Some(ResolvedValue::Null) => None,
        Some(_) => Some(
            list_string_field(arguments, "keys")
                .into_iter()
                .filter_map(|key| {
                    let (namespace, key) = key.split_once('.')?;
                    if namespace.is_empty() || key.is_empty() {
                        return None;
                    }
                    Some((
                        canonical_app_metafield_read_namespace(namespace, api_client_id),
                        key.to_string(),
                    ))
                })
                .collect(),
        ),
    }
}

pub(super) fn owner_metafield_key_position(metafield: &Value, keys: &[(String, String)]) -> usize {
    let namespace = metafield.get("namespace").and_then(Value::as_str);
    let key = metafield.get("key").and_then(Value::as_str);
    keys.iter()
        .position(|(filter_namespace, filter_key)| {
            namespace == Some(filter_namespace.as_str()) && key == Some(filter_key.as_str())
        })
        .unwrap_or(usize::MAX)
}

pub(super) fn owner_metafield_with_connection_key(mut metafield: Value) -> Value {
    if let (Some(namespace), Some(key)) = (
        metafield
            .get("namespace")
            .and_then(Value::as_str)
            .map(str::to_string),
        metafield
            .get("key")
            .and_then(Value::as_str)
            .map(str::to_string),
    ) {
        metafield["key"] = json!(format!("{namespace}.{key}"));
    }
    metafield
}

fn owner_metafield_compare_digest(metafield: &Value) -> Option<String> {
    metafield
        .get("compareDigest")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            metafield
                .get("value")
                .and_then(Value::as_str)
                .map(metafield_compare_digest)
        })
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

pub(super) fn metafield_cursor(metafield: &Value) -> Option<String> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    const PRODUCT_ID: &str = "gid://shopify/Product/100";
    const VARIANT_ID: &str = "gid://shopify/ProductVariant/200";
    const COLLECTION_ID: &str = "gid://shopify/Collection/300";
    const CUSTOMER_ID: &str = "gid://shopify/Customer/400";
    const ORDER_ID: &str = "gid://shopify/Order/500";
    const COMPANY_ID: &str = "gid://shopify/Company/600";

    fn live_hybrid_proxy(calls: Arc<Mutex<Vec<Value>>>) -> DraftProxy {
        DraftProxy::new(Config {
            read_mode: ReadMode::LiveHybrid,
            unsupported_mutation_mode: None,
            bulk_operation_run_mutation_max_input_file_size_bytes: None,
            port: 0,
            shopify_admin_origin: "https://shopify.com".to_string(),
            snapshot_path: None,
        })
        .with_upstream_transport(move |request| {
            let body: Value = serde_json::from_str(&request.body).unwrap();
            let product_id = body["variables"]["productId"].clone();
            let collection_id = body["variables"]["collectionId"].clone();
            calls.lock().unwrap().push(body);
            Response {
                status: 200,
                headers: BTreeMap::new(),
                body: json!({
                    "data": {
                        "firstProduct": {
                            "id": product_id,
                            "metafields": { "nodes": [] }
                        },
                        "repeatedProduct": {
                            "id": product_id,
                            "metafield": null
                        },
                        "collection": {
                            "id": collection_id,
                            "metafield": null
                        }
                    }
                }),
            }
        })
    }

    fn graphql_request(query: &str, variables: Value) -> Request {
        Request {
            method: "POST".to_string(),
            path: "/admin/api/2026-04/graphql.json".to_string(),
            headers: BTreeMap::new(),
            body: json!({ "query": query, "variables": variables }).to_string(),
        }
    }

    fn owner_metafield_inputs() -> Value {
        json!([
            { "ownerId": PRODUCT_ID, "namespace": "custom", "key": "completeness", "type": "single_line_text_field", "value": "product-local" },
            { "ownerId": VARIANT_ID, "namespace": "custom", "key": "completeness", "type": "single_line_text_field", "value": "variant-local" },
            { "ownerId": COLLECTION_ID, "namespace": "custom", "key": "completeness", "type": "single_line_text_field", "value": "collection-local" },
            { "ownerId": CUSTOMER_ID, "namespace": "custom", "key": "completeness", "type": "single_line_text_field", "value": "customer-local" },
            { "ownerId": ORDER_ID, "namespace": "custom", "key": "completeness", "type": "single_line_text_field", "value": "order-local" },
            { "ownerId": COMPANY_ID, "namespace": "custom", "key": "completeness", "type": "single_line_text_field", "value": "company-local" }
        ])
    }

    fn owner_hydration_node(id: &str) -> Value {
        match shopify_gid_resource_type(id) {
            Some("Product") => json!({
                "__typename": "Product",
                "id": id,
                "title": "Observed product title",
                "handle": "observed-product",
                "status": "ACTIVE",
                "totalInventory": 5,
                "tracksInventory": true,
                "createdAt": "2026-07-01T00:00:00Z",
                "updatedAt": "2026-07-02T00:00:00Z",
                "metafield0": Value::Null
            }),
            Some("ProductVariant") => json!({
                "__typename": "ProductVariant",
                "id": id,
                "title": "Observed variant title",
                "sku": "OBSERVED-SKU",
                "barcode": Value::Null,
                "price": "12.00",
                "compareAtPrice": Value::Null,
                "taxable": true,
                "inventoryPolicy": "DENY",
                "inventoryQuantity": 5,
                "selectedOptions": [{ "name": "Title", "value": "Default Title" }],
                "inventoryItem": {
                    "id": "gid://shopify/InventoryItem/201",
                    "tracked": true,
                    "requiresShipping": true
                },
                "product": {
                    "id": PRODUCT_ID,
                    "title": "Observed product title",
                    "handle": "observed-product",
                    "status": "ACTIVE",
                    "totalInventory": 5,
                    "tracksInventory": true,
                    "createdAt": "2026-07-01T00:00:00Z",
                    "updatedAt": "2026-07-02T00:00:00Z"
                },
                "metafield0": Value::Null
            }),
            Some("Collection") => json!({
                "__typename": "Collection",
                "id": id,
                "title": "Observed collection title",
                "handle": "observed-collection",
                "metafield0": Value::Null
            }),
            Some("Customer") => json!({
                "__typename": "Customer",
                "id": id,
                "displayName": "Observed Customer",
                "email": "observed@example.com",
                "metafield0": Value::Null
            }),
            Some("Order") => json!({
                "__typename": "Order",
                "id": id,
                "name": "#500",
                "metafield0": Value::Null
            }),
            Some("Company") => json!({
                "__typename": "Company",
                "id": id,
                "name": "Observed Company",
                "metafield0": Value::Null
            }),
            _ => Value::Null,
        }
    }

    fn owner_sibling_data() -> Value {
        json!({
            "product": { "id": PRODUCT_ID, "descriptionHtml": "<p>Authoritative product description</p>" },
            "productVariant": { "id": VARIANT_ID, "position": 7 },
            "collection": { "id": COLLECTION_ID, "updatedAt": "2026-07-04T00:00:00Z" },
            "customer": { "id": CUSTOMER_ID, "firstName": "Authoritative" },
            "order": { "id": ORDER_ID, "email": "authoritative-order@example.com" },
            "company": { "id": COMPANY_ID, "externalId": "authoritative-company" }
        })
    }

    fn owner_metafield_read_data() -> Value {
        json!({
            "product": { "id": PRODUCT_ID, "metafield": Value::Null },
            "productVariant": { "id": VARIANT_ID, "metafield": Value::Null },
            "collection": { "id": COLLECTION_ID, "metafield": Value::Null },
            "customer": { "id": CUSTOMER_ID, "metafield": Value::Null },
            "order": { "id": ORDER_ID, "metafield": Value::Null },
            "company": { "id": COMPANY_ID, "metafield": Value::Null }
        })
    }

    fn owner_node_sibling_data() -> Value {
        json!([
            { "__typename": "Product", "id": PRODUCT_ID, "descriptionHtml": "<p>Authoritative product description</p>" },
            { "__typename": "ProductVariant", "id": VARIANT_ID, "position": 7 },
            { "__typename": "Collection", "id": COLLECTION_ID, "updatedAt": "2026-07-04T00:00:00Z" },
            { "__typename": "Customer", "id": CUSTOMER_ID, "firstName": "Authoritative" },
            { "__typename": "Order", "id": ORDER_ID, "email": "authoritative-order@example.com" },
            { "__typename": "Company", "id": COMPANY_ID, "externalId": "authoritative-company" }
        ])
    }

    fn owner_completeness_proxy(calls: Arc<Mutex<Vec<Value>>>) -> DraftProxy {
        DraftProxy::new(Config {
            read_mode: ReadMode::LiveHybrid,
            unsupported_mutation_mode: None,
            bulk_operation_run_mutation_max_input_file_size_bytes: None,
            port: 0,
            shopify_admin_origin: "https://shopify.com".to_string(),
            snapshot_path: None,
        })
        .with_upstream_transport(move |request| {
            let body: Value = serde_json::from_str(&request.body).unwrap();
            let query = body["query"].as_str().unwrap_or_default();
            calls.lock().unwrap().push(body.clone());
            let data = if query.contains("OwnerMetafieldsHydrateNodes") {
                let nodes = Value::Array(
                    body["variables"]["ids"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|id| owner_hydration_node(id.as_str().unwrap()))
                        .collect(),
                );
                json!({ "nodes": nodes })
            } else if query.contains("ReadProductSeoTitle") {
                json!({ "product": {
                    "id": PRODUCT_ID,
                    "seo": { "title": "Authoritative SEO title" },
                    "metafield": Value::Null
                } })
            } else if query.contains("ReadProductSeoDescription") {
                json!({ "product": {
                    "id": PRODUCT_ID,
                    "seo": { "description": "Authoritative SEO description" },
                    "metafield": Value::Null
                } })
            } else if query.contains("ReadAliasedCustomerSibling") {
                json!({ "customer": {
                    "id": CUSTOMER_ID,
                    "canonicalFirst": "Authoritative"
                } })
            } else if query.contains("ReadDisjointOwnerNodes") {
                json!({ "nodes": owner_node_sibling_data() })
            } else if query.contains("descriptionHtml") {
                owner_sibling_data()
            } else {
                owner_metafield_read_data()
            };
            Response {
                status: 200,
                headers: BTreeMap::new(),
                body: json!({ "data": data }),
            }
        })
    }

    #[test]
    fn owner_metafield_read_hydrates_requested_window_and_deduped_owner_ids() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let mut proxy = live_hybrid_proxy(calls.clone());
        let product_id = "gid://shopify/Product/100";
        let collection_id = "gid://shopify/Collection/200";

        let response = proxy.process_request(graphql_request(
            r#"
            query ReadOwnerMetafields($productId: ID!, $collectionId: ID!) {
              firstProduct: product(id: $productId) {
                id
                metafields(first: 2, namespace: "custom") {
                  nodes { id namespace key value }
                }
              }
              repeatedProduct: product(id: $productId) {
                metafield(namespace: "custom", key: "color") { id value }
              }
              collection(id: $collectionId) {
                metafield(namespace: "custom", key: "featured") { id value }
              }
            }
            "#,
            json!({"productId": product_id, "collectionId": collection_id}),
        ));

        assert_eq!(response.status, 200);
        let calls = calls.lock().unwrap();
        assert_eq!(calls.len(), 1, "unexpected upstream calls: {calls:#?}");
        let body = &calls[0];
        assert_eq!(
            body["variables"],
            json!({"productId": product_id, "collectionId": collection_id})
        );
        assert!(body.get("operationName").is_none());
        let query = body["query"].as_str().unwrap();
        assert!(query.contains("query ReadOwnerMetafields"));
        assert!(query.contains("metafields(first: 2, namespace: \"custom\")"));
        assert!(query.contains("metafield(namespace: \"custom\", key: \"color\")"));
        assert!(query.contains("metafield(namespace: \"custom\", key: \"featured\")"));
    }

    #[test]
    fn owner_metafield_set_does_not_complete_six_parent_shapes() {
        let calls = Arc::new(Mutex::new(Vec::<Value>::new()));
        let mut proxy = owner_completeness_proxy(calls.clone());

        let mutation = proxy.process_request(graphql_request(
            r#"
            mutation SetOwnerMetafields($metafields: [MetafieldsSetInput!]!) {
              metafieldsSet(metafields: $metafields) {
                metafields { ownerType value }
                userErrors { field message code }
              }
            }
            "#,
            json!({ "metafields": owner_metafield_inputs() }),
        ));
        assert_eq!(mutation.status, 200);
        assert_eq!(
            mutation.body["data"]["metafieldsSet"]["userErrors"],
            json!([])
        );
        assert!(calls.lock().unwrap()[0]["query"]
            .as_str()
            .is_some_and(|query| query.trim_start().starts_with("query ")));

        let siblings = proxy.process_request(graphql_request(
            r#"
            query ReadDisjointOwnerSiblings {
              product(id: "gid://shopify/Product/100") { id descriptionHtml }
              productVariant(id: "gid://shopify/ProductVariant/200") { id position }
              collection(id: "gid://shopify/Collection/300") { id updatedAt }
              customer(id: "gid://shopify/Customer/400") { id firstName }
              order(id: "gid://shopify/Order/500") { id email }
              company(id: "gid://shopify/Company/600") { id externalId }
            }
            "#,
            json!({}),
        ));
        assert_eq!(siblings.status, 200);
        assert_eq!(
            siblings.body["data"],
            owner_sibling_data(),
            "unexpected sibling response: {:#?}",
            siblings.body
        );
        assert_eq!(
            calls.lock().unwrap().len(),
            2,
            "sibling read must hydrate once"
        );

        let metafields = proxy.process_request(graphql_request(
            r#"
            query ReadLocalOwnerMetafields {
              product(id: "gid://shopify/Product/100") { metafield(namespace: "custom", key: "completeness") { value } }
              productVariant(id: "gid://shopify/ProductVariant/200") { metafield(namespace: "custom", key: "completeness") { value } }
              collection(id: "gid://shopify/Collection/300") { metafield(namespace: "custom", key: "completeness") { value } }
              customer(id: "gid://shopify/Customer/400") { metafield(namespace: "custom", key: "completeness") { value } }
              order(id: "gid://shopify/Order/500") { metafield(namespace: "custom", key: "completeness") { value } }
              company(id: "gid://shopify/Company/600") { metafield(namespace: "custom", key: "completeness") { value } }
            }
            "#,
            json!({}),
        ));
        assert_eq!(metafields.status, 200);
        for (root, expected) in [
            ("product", "product-local"),
            ("productVariant", "variant-local"),
            ("collection", "collection-local"),
            ("customer", "customer-local"),
            ("order", "order-local"),
            ("company", "company-local"),
        ] {
            assert_eq!(
                metafields.body["data"][root]["metafield"]["value"],
                expected
            );
        }
        assert_eq!(
            calls.lock().unwrap().len(),
            3,
            "metafield overlay reuses one read"
        );
    }

    #[test]
    fn owner_metafield_read_does_not_complete_six_parent_shapes() {
        let calls = Arc::new(Mutex::new(Vec::<Value>::new()));
        let mut proxy = owner_completeness_proxy(calls.clone());

        let metafields = proxy.process_request(graphql_request(
            r#"
            query ReadOnlyOwnerMetafields {
              product(id: "gid://shopify/Product/100") { id metafield(namespace: "custom", key: "completeness") { value } }
              productVariant(id: "gid://shopify/ProductVariant/200") { id metafield(namespace: "custom", key: "completeness") { value } }
              collection(id: "gid://shopify/Collection/300") { id metafield(namespace: "custom", key: "completeness") { value } }
              customer(id: "gid://shopify/Customer/400") { id metafield(namespace: "custom", key: "completeness") { value } }
              order(id: "gid://shopify/Order/500") { id metafield(namespace: "custom", key: "completeness") { value } }
              company(id: "gid://shopify/Company/600") { id metafield(namespace: "custom", key: "completeness") { value } }
            }
            "#,
            json!({}),
        ));
        assert_eq!(metafields.status, 200);
        for root in [
            "product",
            "productVariant",
            "collection",
            "customer",
            "order",
            "company",
        ] {
            assert_eq!(metafields.body["data"][root]["metafield"], Value::Null);
        }

        let siblings = proxy.process_request(graphql_request(
            r#"
            query ReadDisjointOwnerSiblingsAfterMetafieldRead {
              product(id: "gid://shopify/Product/100") { id descriptionHtml }
              productVariant(id: "gid://shopify/ProductVariant/200") { id position }
              collection(id: "gid://shopify/Collection/300") { id updatedAt }
              customer(id: "gid://shopify/Customer/400") { id firstName }
              order(id: "gid://shopify/Order/500") { id email }
              company(id: "gid://shopify/Company/600") { id externalId }
            }
            "#,
            json!({}),
        ));
        assert_eq!(siblings.status, 200);
        assert_eq!(siblings.body["data"], owner_sibling_data());
        assert_eq!(calls.lock().unwrap().len(), 2);
    }

    #[test]
    fn aliased_customer_hydration_records_the_canonical_parent_shape() {
        let calls = Arc::new(Mutex::new(Vec::<Value>::new()));
        let mut proxy = owner_completeness_proxy(calls.clone());
        let mutation = proxy.process_request(graphql_request(
            r#"
            mutation SetOwnerMetafields($metafields: [MetafieldsSetInput!]!) {
              metafieldsSet(metafields: $metafields) { userErrors { message } }
            }
            "#,
            json!({ "metafields": owner_metafield_inputs() }),
        ));
        assert_eq!(
            mutation.body["data"]["metafieldsSet"]["userErrors"],
            json!([])
        );

        let aliased = proxy.process_request(graphql_request(
            r#"
            query ReadAliasedCustomerSibling {
              customer(id: "gid://shopify/Customer/400") {
                id
                canonicalFirst: firstName
              }
            }
            "#,
            json!({}),
        ));
        assert_eq!(
            aliased.body["data"]["customer"],
            json!({ "id": CUSTOMER_ID, "canonicalFirst": "Authoritative" })
        );

        let repeated = proxy.process_request(graphql_request(
            r#"
            query ReadCanonicalCustomerSiblingAgain {
              customer(id: "gid://shopify/Customer/400") { id firstName }
            }
            "#,
            json!({}),
        ));
        assert_eq!(
            repeated.body["data"]["customer"],
            json!({ "id": CUSTOMER_ID, "firstName": "Authoritative" })
        );
        assert_eq!(
            calls.lock().unwrap().len(),
            2,
            "canonical completeness should avoid a repeated hydrate"
        );
    }

    #[test]
    fn owner_parent_completeness_survives_dump_restore() {
        let initial_calls = Arc::new(Mutex::new(Vec::<Value>::new()));
        let mut proxy = owner_completeness_proxy(initial_calls);
        let mutation = proxy.process_request(graphql_request(
            r#"
            mutation SetOwnerMetafields($metafields: [MetafieldsSetInput!]!) {
              metafieldsSet(metafields: $metafields) { userErrors { message } }
            }
            "#,
            json!({ "metafields": owner_metafield_inputs() }),
        ));
        assert_eq!(
            mutation.body["data"]["metafieldsSet"]["userErrors"],
            json!([])
        );

        let dump = proxy.process_request(Request {
            method: "POST".to_string(),
            path: "/__meta/dump".to_string(),
            headers: BTreeMap::new(),
            body: "{}".to_string(),
        });
        assert_eq!(dump.status, 200);
        assert!(
            dump.body["state"]["stagedState"]["ownerParentObservedFieldPaths"]
                .get(PRODUCT_ID)
                .is_some()
        );

        let restored_calls = Arc::new(Mutex::new(Vec::<Value>::new()));
        let mut restored = owner_completeness_proxy(restored_calls.clone());
        let restore = restored.process_request(Request {
            method: "POST".to_string(),
            path: "/__meta/restore".to_string(),
            headers: BTreeMap::new(),
            body: dump.body.to_string(),
        });
        assert_eq!(restore.status, 200);

        let response = restored.process_request(graphql_request(
            r#"
            query ReadRestoredOwnerSibling {
              product(id: "gid://shopify/Product/100") { id descriptionHtml }
            }
            "#,
            json!({}),
        ));
        assert_eq!(
            response.body["data"]["product"],
            owner_sibling_data()["product"]
        );
        assert_eq!(restored_calls.lock().unwrap().len(), 1);
    }

    #[test]
    fn disjoint_nested_owner_observations_preserve_richer_parent_fields() {
        let calls = Arc::new(Mutex::new(Vec::<Value>::new()));
        let mut proxy = owner_completeness_proxy(calls.clone());
        let mutation = proxy.process_request(graphql_request(
            r#"
            mutation SetOwnerMetafields($metafields: [MetafieldsSetInput!]!) {
              metafieldsSet(metafields: $metafields) { userErrors { message } }
            }
            "#,
            json!({ "metafields": owner_metafield_inputs() }),
        ));
        assert_eq!(
            mutation.body["data"]["metafieldsSet"]["userErrors"],
            json!([])
        );

        let title = proxy.process_request(graphql_request(
            r#"
            query ReadProductSeoTitle {
              product(id: "gid://shopify/Product/100") {
                id
                seo { title }
                metafield(namespace: "custom", key: "completeness") { value }
              }
            }
            "#,
            json!({}),
        ));
        assert_eq!(
            title.body["data"]["product"]["seo"]["title"],
            "Authoritative SEO title",
            "response={:#?} stored={:#?} paths={:#?}",
            title.body,
            proxy
                .store
                .product_by_id(PRODUCT_ID)
                .map(product_state_json),
            proxy.store.staged.owner_parent_observed_field_paths
        );
        assert_eq!(
            title.body["data"]["product"]["metafield"]["value"],
            "product-local"
        );

        let description = proxy.process_request(graphql_request(
            r#"
            query ReadProductSeoDescription {
              product(id: "gid://shopify/Product/100") {
                id
                seo { description }
                metafield(namespace: "custom", key: "completeness") { value }
              }
            }
            "#,
            json!({}),
        ));
        assert_eq!(
            description.body["data"]["product"]["seo"]["description"],
            "Authoritative SEO description"
        );
        assert_eq!(
            description.body["data"]["product"]["metafield"]["value"],
            "product-local"
        );
        assert_eq!(
            proxy
                .store
                .product_by_id(PRODUCT_ID)
                .map(product_state_json)
                .unwrap()["seo"],
            json!({
                "title": "Authoritative SEO title",
                "description": "Authoritative SEO description"
            })
        );
        assert_eq!(
            calls.lock().unwrap().len(),
            3,
            "each public metafield-overlay read should use one upstream request"
        );
    }

    #[test]
    fn owner_parent_completeness_applies_to_node_loaders() {
        let calls = Arc::new(Mutex::new(Vec::<Value>::new()));
        let mut proxy = owner_completeness_proxy(calls.clone());
        let mutation = proxy.process_request(graphql_request(
            r#"
            mutation SetOwnerMetafields($metafields: [MetafieldsSetInput!]!) {
              metafieldsSet(metafields: $metafields) { userErrors { message } }
            }
            "#,
            json!({ "metafields": owner_metafield_inputs() }),
        ));
        assert_eq!(
            mutation.body["data"]["metafieldsSet"]["userErrors"],
            json!([])
        );

        let response = proxy.process_request(graphql_request(
            r#"
            query ReadDisjointOwnerNodes {
              nodes(ids: [
                "gid://shopify/Product/100",
                "gid://shopify/ProductVariant/200",
                "gid://shopify/Collection/300",
                "gid://shopify/Customer/400",
                "gid://shopify/Order/500",
                "gid://shopify/Company/600"
              ]) {
                __typename
                ... on Product { id descriptionHtml }
                ... on ProductVariant { id position }
                ... on Collection { id updatedAt }
                ... on Customer { id firstName }
                ... on Order { id email }
                ... on Company { id externalId }
              }
            }
            "#,
            json!({}),
        ));
        assert_eq!(response.status, 200);
        assert_eq!(
            response.body["data"]["nodes"],
            owner_node_sibling_data(),
            "unexpected node response: {:#?}",
            response.body
        );
        assert_eq!(calls.lock().unwrap().len(), 2);
        for (id, field) in [
            (PRODUCT_ID, "descriptionHtml"),
            (VARIANT_ID, "position"),
            (COLLECTION_ID, "updatedAt"),
            (CUSTOMER_ID, "firstName"),
            (ORDER_ID, "email"),
            (COMPANY_ID, "externalId"),
        ] {
            assert!(
                proxy
                    .owner_parent_shape_is_complete(id, &BTreeSet::from([vec![field.to_string()]])),
                "{id} did not retain {field}: {:#?}",
                proxy.store.staged.owner_parent_observed_field_paths
            );
        }

        let repeated = proxy.process_request(graphql_request(
            r#"
            query ReadDisjointOwnerNodesAgain {
              nodes(ids: [
                "gid://shopify/Product/100",
                "gid://shopify/ProductVariant/200",
                "gid://shopify/Collection/300",
                "gid://shopify/Customer/400",
                "gid://shopify/Order/500",
                "gid://shopify/Company/600"
              ]) {
                __typename
                ... on Product { id descriptionHtml }
                ... on ProductVariant { id position }
                ... on Collection { id updatedAt }
                ... on Customer { id firstName }
                ... on Order { id email }
                ... on Company { id externalId }
              }
            }
            "#,
            json!({}),
        ));
        assert_eq!(repeated.status, 200);
        assert_eq!(repeated.body["data"]["nodes"], owner_node_sibling_data());
        let calls = calls.lock().unwrap();
        assert_eq!(
            calls.len(),
            2,
            "the same concrete owner shapes should remain complete: {calls:#?}"
        );
    }

    #[test]
    fn owner_parent_completeness_never_overrides_exact_tombstones() {
        let calls = Arc::new(Mutex::new(Vec::<Value>::new()));
        let mut proxy = owner_completeness_proxy(calls.clone());
        let mutation = proxy.process_request(graphql_request(
            r#"
            mutation SetOwnerMetafields($metafields: [MetafieldsSetInput!]!) {
              metafieldsSet(metafields: $metafields) { userErrors { message } }
            }
            "#,
            json!({ "metafields": owner_metafield_inputs() }),
        ));
        assert_eq!(
            mutation.body["data"]["metafieldsSet"]["userErrors"],
            json!([])
        );

        proxy.store.delete_product(PRODUCT_ID);
        assert!(proxy.store.delete_product_variant(VARIANT_ID));
        proxy
            .store
            .staged
            .collections
            .tombstone(COLLECTION_ID.to_string());
        proxy
            .store
            .staged
            .customers
            .tombstone(CUSTOMER_ID.to_string());
        proxy.store.staged.orders.tombstone(ORDER_ID.to_string());
        proxy
            .store
            .staged
            .deleted_b2b_company_ids
            .insert(COMPANY_ID.to_string());

        let response = proxy.process_request(graphql_request(
            r#"
            query ReadTombstonedOwners {
              product(id: "gid://shopify/Product/100") { id descriptionHtml }
              productVariant(id: "gid://shopify/ProductVariant/200") { id position }
              collection(id: "gid://shopify/Collection/300") { id updatedAt }
              customer(id: "gid://shopify/Customer/400") { id firstName }
              order(id: "gid://shopify/Order/500") { id email }
              company(id: "gid://shopify/Company/600") { id externalId }
              nodes(ids: [
                "gid://shopify/Product/100",
                "gid://shopify/ProductVariant/200",
                "gid://shopify/Collection/300",
                "gid://shopify/Customer/400",
                "gid://shopify/Order/500",
                "gid://shopify/Company/600"
              ]) {
                __typename
                ... on Product { id descriptionHtml }
                ... on ProductVariant { id position }
                ... on Collection { id updatedAt }
                ... on Customer { id firstName }
                ... on Order { id email }
                ... on Company { id externalId }
              }
            }
            "#,
            json!({}),
        ));
        assert_eq!(response.status, 200);
        for root in [
            "product",
            "productVariant",
            "collection",
            "customer",
            "order",
            "company",
        ] {
            assert!(response.body["data"][root].is_null(), "{root} resurfaced");
        }
        assert_eq!(
            response.body["data"]["nodes"],
            Value::Array(vec![Value::Null; 6])
        );
        let calls = calls.lock().unwrap();
        assert_eq!(
            calls.len(),
            1,
            "unexpected tombstone read transport: {calls:#?}"
        );
        assert!(calls[0]["query"]
            .as_str()
            .is_some_and(|query| query.trim_start().starts_with("query ")));
    }

    #[test]
    fn child_only_metafields_do_not_fabricate_unresolved_parents() {
        let mut proxy = DraftProxy::new(Config {
            read_mode: ReadMode::Snapshot,
            unsupported_mutation_mode: None,
            bulk_operation_run_mutation_max_input_file_size_bytes: None,
            port: 0,
            shopify_admin_origin: String::new(),
            snapshot_path: None,
        });
        let mutation = proxy.process_request(graphql_request(
            r#"
            mutation SetUnresolvedOwnerMetafields($metafields: [MetafieldsSetInput!]!) {
              metafieldsSet(metafields: $metafields) { userErrors { message } }
            }
            "#,
            json!({ "metafields": owner_metafield_inputs() }),
        ));
        assert_eq!(
            mutation.body["data"]["metafieldsSet"]["userErrors"],
            json!([])
        );

        let response = proxy.process_request(graphql_request(
            r#"
            query ReadUnresolvedOwners {
              product(id: "gid://shopify/Product/100") { id metafield(namespace: "custom", key: "completeness") { value } }
              productVariant(id: "gid://shopify/ProductVariant/200") { id metafield(namespace: "custom", key: "completeness") { value } }
              collection(id: "gid://shopify/Collection/300") { id metafield(namespace: "custom", key: "completeness") { value } }
              customer(id: "gid://shopify/Customer/400") { id metafield(namespace: "custom", key: "completeness") { value } }
              order(id: "gid://shopify/Order/500") { id metafield(namespace: "custom", key: "completeness") { value } }
              company(id: "gid://shopify/Company/600") { id metafield(namespace: "custom", key: "completeness") { value } }
            }
            "#,
            json!({}),
        ));
        assert_eq!(response.status, 200);
        for root in [
            "product",
            "productVariant",
            "collection",
            "customer",
            "order",
            "company",
        ] {
            assert!(
                response.body["data"][root].is_null(),
                "{root} was fabricated"
            );
        }
    }
}

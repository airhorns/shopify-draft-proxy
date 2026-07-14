use super::media::media_file_record_from_node;
use super::*;
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

impl DraftProxy {
    // metafieldsSet/metafieldsDelete read their `metafields` list from the
    // resolved root-field arguments so inline-document forms work, not only the
    // `$metafields` variable form. Falls back to top-level variables for safety.
    pub(in crate::proxy) fn owner_metafields_set(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let (response_key, payload_selection, arguments) = self
            .execution_primary_root_response_parts(query, variables, || {
                "metafieldsSet".to_string()
            });
        let inputs = metafields_mutation_inputs(&arguments, variables);
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
            return MutationOutcome::response(ok_json(
                json!({"data": {response_key: selected_json(&payload, &payload_selection)}}),
            ));
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
        let (response_key, payload_selection, arguments) = self
            .execution_primary_root_response_parts(query, variables, || {
                "metafieldsDelete".to_string()
            });
        let inputs = metafields_mutation_inputs(&arguments, variables);
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
            return MutationOutcome::response(ok_json(
                json!({"data": {response_key: selected_json(&payload, &payload_selection)}}),
            ));
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
            return MutationOutcome::response(ok_json(
                json!({"data": {response_key: selected_json(&payload, &payload_selection)}}),
            ));
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
        MutationOutcome::staged(
            ok_json(json!({"data": {response_key: selected_json(&payload, &payload_selection)}})),
            LogDraft::staged("metafieldsDelete", "products", staged_owner_ids),
        )
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
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> bool {
        let fields = self
            .execution_root_fields(query, variables)
            .unwrap_or_default();
        let mut has_non_product_owner_read = false;
        let mut needs_live_product_hydration = false;
        for field in fields {
            if !Self::owner_field_selects_metafields_at_root(&field.name, &field.selection) {
                continue;
            }
            if self.config.read_mode == ReadMode::LiveHybrid {
                let owner_id = self.owner_field_id(&field, variables);
                let cold = self.owner_needs_metafield_hydration(&field.name, &owner_id);
                // A cold (unstaged) owner that also selects sub-resources the
                // metafields overlay cannot synthesize (addresses, orders, events, ...)
                // must forward the whole read upstream as a passthrough rather than be
                // answered with a metafields-only projection that silently drops them.
                if cold
                    && !Self::owner_metafields_read_selection_is_metafields_only(&field.selection)
                {
                    continue;
                }
            }
            match field.name.as_str() {
                "collection" | "customer" | "order" | "company" => {
                    has_non_product_owner_read = true;
                }
                "shop" => {
                    let owner_id = self.owner_field_id(&field, variables);
                    if !owner_id.is_empty() && self.owner_has_metafield_local_effects(&owner_id) {
                        has_non_product_owner_read = true;
                    }
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

    pub(in crate::proxy) fn owner_metafields_read(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let fields = self
            .execution_root_fields(query, variables)
            .unwrap_or_default();
        self.hydrate_owner_metafield_read_fields(request, &fields, variables);
        let api_client_id = request_app_namespace_api_client_id(request);
        let data = root_payload_json(&fields, |field| {
            if !matches!(
                field.name.as_str(),
                "product"
                    | "productVariant"
                    | "collection"
                    | "customer"
                    | "order"
                    | "company"
                    | "shop"
            ) {
                return None;
            }
            Some(self.owner_metafield_owner_json(field, variables, api_client_id.as_deref()))
        });
        ok_json(json!({"data": data}))
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
        let Some((query, variables)) = owner_metafield_hydrate_request(ids, &shape) else {
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
        match root_field {
            "product" => self.store.product_by_id(owner_id).is_none(),
            "productVariant" => self.store.product_variant_by_id(owner_id).is_none(),
            "collection" => !self.store.staged.collections.contains_key(owner_id),
            "customer" => !self.store.staged.customers.contains_key(owner_id),
            "order" => !self.store.staged.orders.contains_key(owner_id),
            "company" => !self.store.staged.b2b_companies.contains_key(owner_id),
            "shop" => {
                !owner_id.is_empty()
                    && !self.owner_has_metafield_local_effects(owner_id)
                    && self.store.base.shop.get("id").and_then(Value::as_str) != Some(owner_id)
            }
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
                if let Some(variant) = product_variant_state_from_observed_json(node) {
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
            Some("Shop") => {
                self.store.base.shop =
                    shallow_merged_object(self.store.base.shop.clone(), node.clone());
            }
            _ => {}
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
                    self.stage_observed_owner_metafields(variant_id, variant);
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

    fn stage_observed_owner_metafields(&mut self, owner_id: &str, node: &Value) {
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

    fn owner_record_json_for_read(
        &self,
        root_field: &str,
        owner_id: &str,
        selections: &[SelectedField],
        api_client_id: Option<&str>,
    ) -> Option<Value> {
        match root_field {
            "product" => {
                let product = self.store.product_by_id(owner_id)?;
                let variants = self.store.product_variants_for_product(owner_id);
                let base = self.product_json_with_store_currency(product, &variants, selections);
                Some(
                    self.owner_metafield_overlay_owner_json_with_product_variants_and_app_namespace_api_client_id(
                        root_field,
                        &product.id,
                        selections,
                        &product.variants,
                        base,
                        api_client_id,
                    ),
                )
            }
            "productVariant" => {
                let variant = self.store.product_variant_by_id(owner_id)?;
                let base = self.product_variant_json_with_current_publication_context(
                    variant,
                    self.store.product_by_id(&variant.product_id),
                    selections,
                );
                Some(
                    self.owner_metafield_overlay_owner_json_with_app_namespace_api_client_id(
                        root_field,
                        owner_id,
                        selections,
                        base,
                        api_client_id,
                    ),
                )
            }
            "collection" => self.store.staged.collections.get(owner_id).map(|record| {
                self.owner_metafield_overlay_owner_json_with_app_namespace_api_client_id(
                    root_field,
                    owner_id,
                    selections,
                    selected_json(record, selections),
                    api_client_id,
                )
            }),
            "customer" => self.store.staged.customers.get(owner_id).map(|record| {
                self.owner_metafield_overlay_owner_json_with_app_namespace_api_client_id(
                    root_field,
                    owner_id,
                    selections,
                    selected_json(record, selections),
                    api_client_id,
                )
            }),
            "order" => self.store.staged.orders.get(owner_id).map(|record| {
                self.owner_metafield_overlay_owner_json_with_app_namespace_api_client_id(
                    root_field,
                    owner_id,
                    selections,
                    selected_json(record, selections),
                    api_client_id,
                )
            }),
            "company" => self.store.staged.b2b_companies.get(owner_id).map(|record| {
                self.owner_metafield_overlay_owner_json_with_app_namespace_api_client_id(
                    root_field,
                    owner_id,
                    selections,
                    selected_json(record, selections),
                    api_client_id,
                )
            }),
            "shop" => {
                let mut shop = self.store.effective_shop();
                if !shop.is_object() {
                    shop = json!({});
                }
                if shop.get("id").and_then(Value::as_str).is_none() && !owner_id.is_empty() {
                    shop["id"] = json!(owner_id);
                }
                Some(
                    self.owner_metafield_overlay_owner_json_with_app_namespace_api_client_id(
                        root_field,
                        owner_id,
                        selections,
                        selected_json(&shop, selections),
                        api_client_id,
                    ),
                )
            }
            _ => None,
        }
    }

    fn owner_metafield_owner_json(
        &self,
        field: &RootFieldSelection,
        variables: &BTreeMap<String, ResolvedValue>,
        api_client_id: Option<&str>,
    ) -> Value {
        let owner_id = self.owner_field_id(field, variables);
        self.owner_record_json_for_read(&field.name, &owner_id, &field.selection, api_client_id)
            .unwrap_or_else(|| {
                self.minimal_owner_json_for_read_with_app_namespace_api_client_id(
                    &field.name,
                    &owner_id,
                    &field.selection,
                    api_client_id,
                )
            })
    }

    pub(super) fn minimal_owner_json_for_read_with_app_namespace_api_client_id(
        &self,
        root_field: &str,
        owner_id: &str,
        selections: &[SelectedField],
        api_client_id: Option<&str>,
    ) -> Value {
        self.owner_metafield_overlay_owner_json_with_app_namespace_api_client_id(
            root_field,
            owner_id,
            selections,
            json!({}),
            api_client_id,
        )
    }

    pub(in crate::proxy) fn owner_metafield_overlay_owner_json(
        &self,
        root_field: &str,
        owner_id: &str,
        selections: &[SelectedField],
        base: Value,
    ) -> Value {
        self.owner_metafield_overlay_owner_json_with_app_namespace_api_client_id(
            root_field, owner_id, selections, base, None,
        )
    }

    pub(super) fn owner_metafield_overlay_owner_json_with_app_namespace_api_client_id(
        &self,
        root_field: &str,
        owner_id: &str,
        selections: &[SelectedField],
        base: Value,
        api_client_id: Option<&str>,
    ) -> Value {
        self.owner_metafield_overlay_owner_json_with_product_variants_and_app_namespace_api_client_id(
            root_field,
            owner_id,
            selections,
            &[],
            base,
            api_client_id,
        )
    }

    pub(super) fn owner_metafield_overlay_owner_json_with_product_variants_and_app_namespace_api_client_id(
        &self,
        root_field: &str,
        owner_id: &str,
        selections: &[SelectedField],
        fallback_product_variants: &[Value],
        base: Value,
        api_client_id: Option<&str>,
    ) -> Value {
        selected_payload_json(selections, |selection| match selection.name.as_str() {
            "__typename" => Some(json!(owner_typename_from_root(root_field))),
            "id" => Some(json!(owner_id)),
            "metafield" => Some(self.selected_owner_metafield_overlay(
                owner_id,
                selection,
                &base,
                api_client_id,
            )),
            "metafields" => Some(self.selected_owner_metafields_connection_overlay(
                owner_id,
                selection,
                &base,
                api_client_id,
            )),
            "variants"
                if root_field == "product"
                    && Self::owner_field_selects_metafields(&selection.selection) =>
            {
                Some(self.selected_product_variants_with_metafields(
                    owner_id,
                    fallback_product_variants,
                    selection,
                    api_client_id,
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
        api_client_id: Option<&str>,
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
        let render_variant =
            |entry: &VariantEntry, selections: &[SelectedField]| match &entry.source {
                VariantSource::Record(variant) => {
                    let base = self.product_variant_json_with_current_publication_context(
                        variant,
                        self.store.product_by_id(&variant.product_id),
                        selections,
                    );
                    self.owner_metafield_overlay_owner_json_with_app_namespace_api_client_id(
                        "productVariant",
                        &variant.id,
                        selections,
                        base,
                        api_client_id,
                    )
                }
                VariantSource::Fallback(variant) => {
                    let base = selected_json(variant, selections);
                    self.owner_metafield_overlay_owner_json_with_app_namespace_api_client_id(
                        "productVariant",
                        &entry.id,
                        selections,
                        base,
                        api_client_id,
                    )
                }
            };
        selected_typed_connection_with_page_info(
            &entries,
            &selection.selection,
            render_variant,
            |entry| entry.id.clone(),
            page_info,
        )
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

    pub(super) fn owner_field_selects_direct_metafields(selections: &[SelectedField]) -> bool {
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

    fn selected_owner_metafield(
        &self,
        owner_id: &str,
        selection: &SelectedField,
        api_client_id: Option<&str>,
    ) -> Value {
        let namespace = owner_metafield_read_namespace(&selection.arguments, api_client_id);
        let key = resolved_string_field(&selection.arguments, "key").unwrap_or_default();
        self.owner_metafield(owner_id, &namespace, &key)
            .map(|metafield| {
                self.selected_reference_value_record_json(&metafield, &selection.selection)
            })
            .unwrap_or(Value::Null)
    }

    fn selected_owner_metafield_overlay(
        &self,
        owner_id: &str,
        selection: &SelectedField,
        base: &Value,
        api_client_id: Option<&str>,
    ) -> Value {
        let namespace = owner_metafield_read_namespace(&selection.arguments, api_client_id);
        let key = resolved_string_field(&selection.arguments, "key").unwrap_or_default();
        if self.owner_metafield_has_local_effect(owner_id, &namespace, &key) {
            return self.selected_owner_metafield(owner_id, selection, api_client_id);
        }
        if let Some(metafield) = base_owner_metafield(base, &namespace, &key) {
            return self.selected_reference_value_record_json(&metafield, &selection.selection);
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
        api_client_id: Option<&str>,
    ) -> Value {
        let namespace = owner_metafields_connection_namespace(&selection.arguments, api_client_id);
        let keys = owner_metafields_connection_keys_with_app_namespace(
            &selection.arguments,
            api_client_id,
        );
        let mut records = self.owner_metafields(owner_id, namespace.as_deref(), keys.as_deref());
        if resolved_bool_field(&selection.arguments, "reverse").unwrap_or(false) {
            records.reverse();
        }

        let (records, page_info) = connection_window(&records, &selection.arguments, |record| {
            metafield_cursor(record).unwrap_or_default()
        });

        let records_for_output = if keys.is_some() {
            records
                .into_iter()
                .map(owner_metafield_with_connection_key)
                .collect()
        } else {
            records
        };
        selected_typed_connection_with_page_info(
            &records_for_output,
            &selection.selection,
            |metafield, selections| {
                self.selected_reference_value_record_json(metafield, selections)
            },
            |metafield| metafield_cursor(metafield).unwrap_or_default(),
            page_info,
        )
    }

    fn selected_owner_metafields_connection_overlay(
        &self,
        owner_id: &str,
        selection: &SelectedField,
        base: &Value,
        api_client_id: Option<&str>,
    ) -> Value {
        if !self.owner_has_metafield_local_effects(owner_id) {
            if let Some(base_value) = base
                .get(selection.response_key.as_str())
                .or_else(|| base.get(selection.name.as_str()))
            {
                return base_value.clone();
            }
        }
        self.selected_owner_metafields_connection(owner_id, selection, api_client_id)
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

    pub(in crate::proxy) fn selected_reference_value_record_json(
        &self,
        record: &Value,
        selections: &[SelectedField],
    ) -> Value {
        selected_payload_json(selections, |selection| match selection.name.as_str() {
            "reference" => Some(self.selected_scalar_reference_json(record, selection)),
            "references" => Some(self.selected_reference_connection_json(record, selection)),
            _ => record
                .get(&selection.name)
                .map(|value| nullable_selected_json(value, &selection.selection)),
        })
    }

    fn selected_scalar_reference_json(&self, record: &Value, selection: &SelectedField) -> Value {
        if let Some(existing) = record.get("reference") {
            return nullable_selected_json(existing, &selection.selection);
        }
        let Some(id) = scalar_reference_id(record) else {
            return Value::Null;
        };
        self.selected_reference_node_json(&id, &selection.selection)
            .unwrap_or(Value::Null)
    }

    fn selected_reference_connection_json(
        &self,
        record: &Value,
        selection: &SelectedField,
    ) -> Value {
        if let Some(existing) = record.get("references").filter(|value| value.is_object()) {
            return project_seeded_connection(existing, &selection.arguments, &selection.selection);
        }
        let ids = list_reference_ids(record)
            .into_iter()
            .filter(|id| self.selected_reference_node_json(id, &[]).is_some())
            .collect::<Vec<_>>();
        let (ids, page_info) = connection_window(&ids, &selection.arguments, |id| id.clone());
        selected_typed_connection_with_page_info(
            &ids,
            &selection.selection,
            |id, selections| {
                self.selected_reference_node_json(id, selections)
                    .unwrap_or(Value::Null)
            },
            |id| id.clone(),
            page_info,
        )
    }

    fn selected_reference_node_json(
        &self,
        id: &str,
        selections: &[SelectedField],
    ) -> Option<Value> {
        match shopify_gid_resource_type(id) {
            Some("Product") => {
                let product = self.store.product_by_id(id)?;
                let variants = self.store.product_variants_for_product(id);
                Some(self.product_owner_json_with_store_currency(product, &variants, selections))
            }
            Some("ProductVariant") => {
                let variant = self.store.product_variant_by_id(id)?;
                let variant = self.variant_with_inventory_levels(variant);
                let base = self.product_variant_json_with_current_publication_context(
                    &variant,
                    self.store.product_by_id(&variant.product_id),
                    selections,
                );
                Some(self.owner_metafield_overlay_owner_json(
                    "productVariant",
                    id,
                    selections,
                    base,
                ))
            }
            Some("Collection") => self.store.collection_by_id(id).map(|record| {
                self.owner_metafield_overlay_owner_json(
                    "collection",
                    id,
                    selections,
                    selected_json(record, selections),
                )
            }),
            Some("Customer") => self.store.staged.customers.get(id).map(|record| {
                self.owner_metafield_overlay_owner_json(
                    "customer",
                    id,
                    selections,
                    selected_json(record, selections),
                )
            }),
            Some("Order") => self.store.staged.orders.get(id).map(|record| {
                self.owner_metafield_overlay_owner_json(
                    "order",
                    id,
                    selections,
                    selected_json(record, selections),
                )
            }),
            Some("Company") => self.store.staged.b2b_companies.get(id).map(|record| {
                self.owner_metafield_overlay_owner_json(
                    "company",
                    id,
                    selections,
                    selected_json(record, selections),
                )
            }),
            Some("Shop") => {
                let shop = self.store.effective_shop();
                if shop.get("id").and_then(Value::as_str) != Some(id) {
                    return None;
                }
                Some(self.owner_metafield_overlay_owner_json(
                    "shop",
                    id,
                    selections,
                    selected_json(&shop, selections),
                ))
            }
            Some("Metaobject") => {
                let record = self.metaobject_by_id(id)?;
                let record = self.project_metaobject_against_definition(&record);
                Some(self.selected_metaobject(&record, selections))
            }
            Some("MediaImage" | "Video" | "ExternalVideo" | "Model3d" | "GenericFile") => self
                .store
                .staged
                .media_files
                .get(id)
                .filter(|_| !self.store.staged.media_files.is_tombstoned(id))
                .map(|record| selected_json(record, selections)),
            _ => None,
        }
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

fn scalar_reference_id(record: &Value) -> Option<String> {
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

fn list_reference_ids(record: &Value) -> Vec<String> {
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
        .unwrap_or_default()
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

fn base_owner_metafield(base: &Value, namespace: &str, key: &str) -> Option<Value> {
    fn matches_metafield(value: &Value, namespace: &str, key: &str) -> bool {
        value.get("namespace").and_then(Value::as_str) == Some(namespace)
            && value.get("key").and_then(Value::as_str) == Some(key)
    }

    if let Some(record) = base
        .as_object()
        .into_iter()
        .flat_map(|object| object.values())
        .find(|value| matches_metafield(value, namespace, key))
    {
        return Some(record.clone());
    }

    if let Some(record) = base
        .get("metafields")
        .and_then(|connection| connection.get("nodes"))
        .and_then(Value::as_array)
        .into_iter()
        .flat_map(|nodes| nodes.iter())
        .find(|value| matches_metafield(value, namespace, key))
    {
        return Some(record.clone());
    }

    base.get("metafields")
        .and_then(|connection| connection.get("edges"))
        .and_then(Value::as_array)
        .into_iter()
        .flat_map(|edges| edges.iter())
        .filter_map(|edge| edge.get("node"))
        .find(|value| matches_metafield(value, namespace, key))
        .cloned()
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

fn owner_typename_from_root(root_field: &str) -> &'static str {
    match root_field {
        "product" => "Product",
        "productVariant" => "ProductVariant",
        "collection" => "Collection",
        "customer" => "Customer",
        "order" => "Order",
        "company" => "Company",
        "shop" => "Shop",
        _ => "Node",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

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
            calls
                .lock()
                .unwrap()
                .push(serde_json::from_str(&request.body).unwrap());
            Response {
                status: 200,
                headers: BTreeMap::new(),
                body: json!({"data": {"nodes": []}}),
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
        assert_eq!(calls.len(), 1);
        let body = &calls[0];
        assert_eq!(body["operationName"], "OwnerMetafieldsHydrateNodes");
        assert_eq!(
            body["variables"]["ids"],
            json!([collection_id, product_id]),
            "duplicate owner ids should be removed before hydration"
        );
        assert_eq!(body["variables"]["metafields0First"], json!(2));
        assert_eq!(body["variables"]["metafields0Namespace"], json!("custom"));
        assert_eq!(body["variables"]["metafield0Namespace"], json!("custom"));
        assert_eq!(body["variables"]["metafield0Key"], json!("color"));
        assert_eq!(body["variables"]["metafield1Key"], json!("featured"));
        let query = body["query"].as_str().unwrap();
        assert!(query.contains(
            "metafields0: metafields(first: $metafields0First, namespace: $metafields0Namespace)"
        ));
        assert!(query.contains(
            "metafield0: metafield(namespace: $metafield0Namespace, key: $metafield0Key)"
        ));
        assert!(!query.contains("first: 250"));
        assert!(!query.contains("variants("));
    }
}

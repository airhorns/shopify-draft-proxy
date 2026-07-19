use super::*;

const DELIVERY_CUSTOMIZATION_MAX_ENABLED: usize = 25;

pub(in crate::proxy) fn delivery_customization_field_resolver_registrations(
) -> Vec<FieldResolverRegistration> {
    vec![
        FieldResolverRegistration::explicit(
            ApiSurface::Admin,
            "DeliveryCustomization",
            "metafieldDefinitions",
            delivery_customization_metafield_definitions_field,
        ),
        FieldResolverRegistration::explicit(
            ApiSurface::Admin,
            "DeliveryCustomization",
            "errorHistory",
            delivery_customization_error_history_field,
        ),
    ]
}

fn delivery_customization_metafield_definitions_field(
    _proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    Ok(connection_value_with_args(
        Vec::new(),
        &resolved_arguments_from_json(&invocation.arguments),
        value_id_cursor,
    ))
}

fn delivery_customization_error_history_field(
    _proxy: &mut DraftProxy,
    _request: &Request,
    _invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    Ok(Value::Null)
}

pub(in crate::proxy) fn delivery_customization_function_key(value: &str) -> String {
    shopify_gid_tail_for_type(value, "ShopifyFunction")
        .unwrap_or(value)
        .to_string()
}

pub(in crate::proxy) fn delivery_customization_query_matches(
    record: &Value,
    query: Option<&str>,
) -> StagedSearchDecision {
    let Some(query) = query.map(str::trim).filter(|query| !query.is_empty()) else {
        return StagedSearchDecision::Match;
    };
    for token in query.split_whitespace() {
        let token = token.trim_matches('"');
        let matches = if let Some((field, value)) = token.split_once(':') {
            let value = value.trim_matches('"');
            match field {
                "id" => record
                    .get("id")
                    .and_then(Value::as_str)
                    .is_some_and(|id| resource_id_matches_gid_or_tail(id, value)),
                "title" => record
                    .get("title")
                    .and_then(Value::as_str)
                    .is_some_and(|title| {
                        title
                            .to_ascii_lowercase()
                            .contains(&value.to_ascii_lowercase())
                    }),
                "enabled" => record
                    .get("enabled")
                    .and_then(Value::as_bool)
                    .is_some_and(|enabled| value.eq_ignore_ascii_case(&enabled.to_string())),
                "function_id" | "functionId" => record
                    .get("functionId")
                    .and_then(Value::as_str)
                    .is_some_and(|id| resource_id_matches_gid_or_tail(id, value)),
                _ => false,
            }
        } else {
            let needle = token.to_ascii_lowercase();
            ["id", "title", "functionId"].iter().any(|field| {
                record
                    .get(*field)
                    .and_then(Value::as_str)
                    .is_some_and(|value| value.to_ascii_lowercase().contains(&needle))
            })
        };
        if !matches {
            return StagedSearchDecision::NoMatch;
        }
    }
    StagedSearchDecision::Match
}

pub(in crate::proxy) fn delivery_customization_sort_key(
    record: &Value,
    sort_key: Option<&str>,
) -> StagedSortKey {
    match sort_key.unwrap_or("ID") {
        "TITLE" => vec![StagedSortValue::String(
            record_string(record, "title").to_ascii_lowercase(),
        )],
        "ENABLED" => vec![StagedSortValue::I64(
            record
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or(false) as i64,
        )],
        "FUNCTION_ID" => vec![StagedSortValue::String(record_string(record, "functionId"))],
        "CREATED_AT" => vec![StagedSortValue::String(record_string(record, "createdAt"))],
        "UPDATED_AT" => vec![StagedSortValue::String(record_string(record, "updatedAt"))],
        _ => vec![resource_id_tail_sort_value(
            record.get("id").and_then(Value::as_str),
        )],
    }
}

fn record_string(record: &Value, field: &str) -> String {
    record
        .get(field)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

pub(in crate::proxy) fn delivery_customization_record(
    id: &str,
    input: &BTreeMap<String, ResolvedValue>,
    api_client_id: Option<&str>,
    resolved_function: Option<&Value>,
    timestamp: &str,
) -> Value {
    let function_id = resolved_string_field(input, "functionId");
    let function_handle = resolved_string_field(input, "functionHandle");
    let effective_function_id = resolved_function
        .and_then(|function| {
            function
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .or_else(|| function_id.clone())
        .unwrap_or_default();
    let shopify_function = resolved_function.cloned().unwrap_or_else(|| {
        delivery_customization_minimal_function(&effective_function_id, function_handle.as_deref())
    });
    let mut record = json!({
        "__typename": "DeliveryCustomization",
        "id": id,
        "title": resolved_string_field(input, "title").unwrap_or_default(),
        "enabled": resolved_bool_field(input, "enabled").unwrap_or(false),
        "functionId": effective_function_id,
        "shopifyFunction": shopify_function,
        "createdAt": timestamp,
        "updatedAt": timestamp
    });
    delivery_customization_set_metafields(
        &mut record,
        delivery_customization_metafields(id, input, api_client_id, timestamp, None),
    );
    record
}

fn delivery_customization_minimal_function(
    function_id: &str,
    function_handle: Option<&str>,
) -> Value {
    json!({
        "__typename": "ShopifyFunction",
        "id": function_id,
        "title": function_handle.unwrap_or_default(),
        "handle": function_handle,
        "apiType": "DELIVERY_CUSTOMIZATION"
    })
}

pub(in crate::proxy) fn delivery_customization_metafields(
    customization_id: &str,
    input: &BTreeMap<String, ResolvedValue>,
    api_client_id: Option<&str>,
    timestamp: &str,
    existing_record: Option<&Value>,
) -> Vec<Value> {
    resolved_object_list_field(input, "metafields")
        .into_iter()
        .enumerate()
        .map(|(index, metafield)| {
            let namespace = resolved_string_field(&metafield, "namespace")
                .map(|namespace| canonical_app_metafield_namespace(Some(&namespace), api_client_id))
                .unwrap_or_else(|| canonical_app_metafield_namespace(None, api_client_id));
            let key = resolved_string_field(&metafield, "key").unwrap_or_default();
            let existing_metafield =
                delivery_customization_existing_metafield(existing_record, &namespace, &key);
            let id = resolved_string_field(&metafield, "id")
                .or_else(|| {
                    existing_metafield
                        .and_then(|metafield| metafield.get("id").and_then(Value::as_str))
                        .map(str::to_string)
                })
                .unwrap_or_else(|| {
                    shopify_gid(
                        "Metafield",
                        format!(
                            "delivery-customization-{}-{}",
                            resource_id_tail(customization_id),
                            index + 1
                        ),
                    )
                });
            let created_at = existing_metafield
                .and_then(|metafield| metafield.get("createdAt").and_then(Value::as_str))
                .unwrap_or(timestamp);
            let metafield_type = resolved_string_field(&metafield, "type").unwrap_or_default();
            let value = resolved_string_field(&metafield, "value").unwrap_or_default();
            json!({
                "__typename": "Metafield",
                "id": id,
                "namespace": namespace,
                "key": key,
                "type": metafield_type,
                "value": value,
                "jsonValue": metafield_json_value(&metafield_type, &value),
                "compareDigest": metafield_compare_digest(&value),
                "ownerType": "DELIVERY_CUSTOMIZATION",
                "createdAt": created_at,
                "updatedAt": timestamp
            })
        })
        .collect()
}

fn delivery_customization_existing_metafield<'a>(
    record: Option<&'a Value>,
    namespace: &str,
    key: &str,
) -> Option<&'a Value> {
    record?
        .get("metafields")?
        .get("nodes")?
        .as_array()?
        .iter()
        .find(|metafield| {
            metafield.get("namespace").and_then(Value::as_str) == Some(namespace)
                && metafield.get("key").and_then(Value::as_str) == Some(key)
        })
}

pub(in crate::proxy) fn delivery_customization_set_metafields(
    record: &mut Value,
    metafields: Vec<Value>,
) {
    let connection = connection_json_with_cursor(
        metafields.clone(),
        |index, _| format!("cursor{}", index + 1),
        empty_page_info(),
    );
    record["metafields"] = connection;
}

pub(in crate::proxy) fn delivery_customization_payload(
    customization: Option<&Value>,
    user_errors: Vec<Value>,
    ids: Option<Vec<String>>,
    deleted_id: Option<Value>,
) -> Value {
    json!({
        "deliveryCustomization": customization.cloned().unwrap_or(Value::Null),
        "ids": ids.unwrap_or_default(),
        "deletedId": deleted_id.unwrap_or(Value::Null),
        "userErrors": user_errors
    })
}

pub(in crate::proxy) fn delivery_customization_error_payload(user_errors: Vec<Value>) -> Value {
    delivery_customization_payload(None, user_errors, None, None)
}

pub(in crate::proxy) fn delivery_customization_record_payload(customization: &Value) -> Value {
    delivery_customization_payload(Some(customization), Vec::new(), None, None)
}

pub(in crate::proxy) fn delivery_customization_user_error(
    field: impl Into<UserErrorField>,
    code: &str,
    message: &str,
) -> Value {
    user_error(field, message, Some(code))
}

pub(in crate::proxy) fn delivery_customization_required_input_field_error(field: &str) -> Value {
    delivery_customization_user_error(
        vec!["deliveryCustomization", field],
        "REQUIRED_INPUT_FIELD",
        "Required input field must be present.",
    )
}

pub(in crate::proxy) fn delivery_customization_metafield_validation_errors(
    input: &BTreeMap<String, ResolvedValue>,
) -> Vec<Value> {
    if !input.contains_key("metafields") {
        return Vec::new();
    }
    let mut errors = Vec::new();
    for (index, metafield) in resolved_object_list_field(input, "metafields")
        .iter()
        .enumerate()
    {
        let mut required_errors = 0;
        for field in ["key", "value"] {
            if resolved_string_field(metafield, field)
                .map(|value| value.trim().is_empty())
                .unwrap_or(true)
            {
                required_errors += 1;
                errors.push(delivery_customization_invalid_metafield_error(
                    index,
                    field,
                    "may not be empty",
                ));
            }
        }
        if required_errors > 0 {
            continue;
        }

        if resolved_string_field(metafield, "type")
            .map(|value| value.trim().is_empty())
            .unwrap_or(true)
        {
            errors.push(delivery_customization_invalid_metafield_error(
                index,
                "type",
                "can't be blank",
            ));
        }
        if let Some(namespace) = resolved_string_field(metafield, "namespace") {
            let namespace = namespace.trim();
            if !namespace.is_empty() && namespace.chars().count() < 3 {
                errors.push(delivery_customization_invalid_metafield_error(
                    index,
                    "namespace",
                    "is too short (minimum is 3 characters)",
                ));
            }
        }
    }
    errors
}

pub(in crate::proxy) fn delivery_customization_invalid_metafield_error(
    index: usize,
    field: &str,
    message: &str,
) -> Value {
    user_error(
        json!([
            "deliveryCustomization",
            "metafields",
            index.to_string(),
            field
        ]),
        message,
        Some("INVALID_METAFIELDS"),
    )
}

pub(in crate::proxy) fn delivery_customization_not_found_error(id: &str) -> Value {
    delivery_customization_user_error(
        vec!["id"],
        "DELIVERY_CUSTOMIZATION_NOT_FOUND",
        &format!("Could not find DeliveryCustomization with id: {id}"),
    )
}

pub(in crate::proxy) fn delivery_customization_activation_not_found_error(ids: &[String]) -> Value {
    delivery_customization_user_error(
        vec!["ids"],
        "DELIVERY_CUSTOMIZATION_NOT_FOUND",
        &format!(
            "Could not find delivery customizations with IDs: {}",
            ids.join(", ")
        ),
    )
}

pub(in crate::proxy) fn delivery_customization_immutable_function_error(field: &str) -> Value {
    delivery_customization_user_error(
        vec!["deliveryCustomization", field],
        "FUNCTION_ID_CANNOT_BE_CHANGED",
        "Function ID cannot be changed.",
    )
}

pub(in crate::proxy) fn delivery_customization_function_not_found_error(
    handle: &str,
    current_app_id: &str,
) -> Value {
    delivery_customization_user_error(
        vec!["deliveryCustomization", "functionHandle"],
        "FUNCTION_NOT_FOUND",
        &format!(
            "Function {handle} not found. Ensure that it is released in the current app ({current_app_id}), and that the app is installed."
        ),
    )
}

pub(in crate::proxy) fn delivery_customization_limit_error() -> Value {
    delivery_customization_user_error(
        vec!["deliveryCustomization", "enabled"],
        "MAXIMUM_ACTIVE_DELIVERY_CUSTOMIZATIONS",
        "Cannot have more than 25 active delivery customizations.",
    )
}

impl DraftProxy {
    pub(crate) fn delivery_customization_query_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        ResolverOutcome::value(
            self.delivery_customization_query_value(invocation.root_name, &arguments),
        )
    }

    fn delivery_customization_query_value(
        &self,
        root_name: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        match root_name {
            "deliveryCustomization" => {
                let id = resolved_string_field(arguments, "id").unwrap_or_default();
                self.store
                    .staged
                    .delivery_customizations
                    .get(&id)
                    .cloned()
                    .unwrap_or(Value::Null)
            }
            "deliveryCustomizations" => staged_connection_value_with_args(
                self.store
                    .staged
                    .delivery_customizations
                    .order
                    .iter()
                    .filter_map(|id| self.store.staged.delivery_customizations.get(id))
                    .cloned()
                    .collect(),
                arguments,
                delivery_customization_query_matches,
                delivery_customization_sort_key,
                Value::clone,
                value_id_cursor,
            ),
            _ => Value::Null,
        }
    }

    pub(crate) fn delivery_customization_mutation_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let RootInvocation {
            root_name,
            arguments,
            request,
            ..
        } = invocation;
        let arguments = resolved_arguments_from_json(&arguments);
        let api_client_id = request_app_namespace_api_client_id(request);
        let (payload, staged_ids) = match root_name {
            "deliveryCustomizationCreate" => self.delivery_customization_create_payload(
                request,
                &arguments,
                api_client_id.as_deref(),
            ),
            "deliveryCustomizationUpdate" => self.delivery_customization_update_payload(
                request,
                &arguments,
                api_client_id.as_deref(),
            ),
            "deliveryCustomizationActivation" => {
                self.delivery_customization_activation_payload(&arguments)
            }
            "deliveryCustomizationDelete" => self.delivery_customization_delete_payload(&arguments),
            _ => {
                return resolver_http_error_outcome(
                    501,
                    format!("Unsupported delivery customization mutation {root_name}"),
                );
            }
        };
        let mut outcome = ResolverOutcome::value(payload);
        if !staged_ids.is_empty() {
            outcome = outcome.with_log_draft(LogDraft::staged(
                root_name,
                "shipping-fulfillments",
                staged_ids,
            ));
        }
        outcome
    }

    fn delivery_customization_create_payload(
        &mut self,
        request: &Request,
        arguments: &BTreeMap<String, ResolvedValue>,
        api_client_id: Option<&str>,
    ) -> (Value, Vec<String>) {
        let input = resolved_object_field(arguments, "deliveryCustomization").unwrap_or_default();
        let function_id = resolved_string_field(&input, "functionId");
        let function_handle = resolved_string_field(&input, "functionHandle");
        let mut required_errors = Vec::new();
        if resolved_string_field(&input, "title")
            .map(|title| title.trim().is_empty())
            .unwrap_or(true)
        {
            required_errors.push(delivery_customization_required_input_field_error("title"));
        }
        if !input.contains_key("enabled") {
            required_errors.push(delivery_customization_required_input_field_error("enabled"));
        }
        if !required_errors.is_empty() {
            return (
                delivery_customization_error_payload(required_errors),
                Vec::new(),
            );
        }
        if function_id.is_some() && function_handle.is_some() {
            return (
                delivery_customization_error_payload(vec![delivery_customization_user_error(
                    vec!["deliveryCustomization"],
                    "MULTIPLE_FUNCTION_IDENTIFIERS",
                    "Only one of function_id or function_handle can be provided, not both.",
                )]),
                Vec::new(),
            );
        }
        if function_id.is_none() && function_handle.is_none() {
            return (
                delivery_customization_error_payload(vec![delivery_customization_user_error(
                    vec!["deliveryCustomization", "functionHandle"],
                    "MISSING_FUNCTION_IDENTIFIER",
                    "Either function_id or function_handle must be provided.",
                )]),
                Vec::new(),
            );
        }
        let resolved_function = if let Some(handle) = function_handle.as_deref() {
            let Some(function) =
                self.resolve_delivery_customization_function(request, None, Some(handle))
            else {
                return (
                    delivery_customization_error_payload(vec![
                        delivery_customization_function_not_found_error(
                            handle,
                            &request_api_client_id(request),
                        ),
                    ]),
                    Vec::new(),
                );
            };
            Some(function)
        } else {
            None
        };
        let metafield_errors = delivery_customization_metafield_validation_errors(&input);
        if !metafield_errors.is_empty() {
            return (
                delivery_customization_error_payload(metafield_errors),
                Vec::new(),
            );
        }
        if resolved_bool_field(&input, "enabled").unwrap_or(false)
            && self.delivery_customization_enabled_count(None) >= DELIVERY_CUSTOMIZATION_MAX_ENABLED
        {
            return (
                delivery_customization_error_payload(vec![delivery_customization_limit_error()]),
                Vec::new(),
            );
        }

        let id = shopify_gid("DeliveryCustomization", self.next_synthetic_id);
        self.next_synthetic_id += 1;
        let timestamp = self.next_mutation_timestamp();
        let record = delivery_customization_record(
            &id,
            &input,
            api_client_id,
            resolved_function.as_ref(),
            &timestamp,
        );
        self.store
            .staged
            .delivery_customizations
            .insert(id.clone(), record.clone());
        (delivery_customization_record_payload(&record), vec![id])
    }

    fn delivery_customization_update_payload(
        &mut self,
        request: &Request,
        arguments: &BTreeMap<String, ResolvedValue>,
        api_client_id: Option<&str>,
    ) -> (Value, Vec<String>) {
        let id = resolved_string_field(arguments, "id").unwrap_or_default();
        let input = resolved_object_field(arguments, "deliveryCustomization").unwrap_or_default();
        let Some(existing) = self.store.staged.delivery_customizations.get(&id).cloned() else {
            return (
                delivery_customization_error_payload(vec![delivery_customization_not_found_error(
                    &id,
                )]),
                Vec::new(),
            );
        };

        if resolved_string_field(&input, "title").is_some_and(|title| title.trim().is_empty()) {
            return (
                delivery_customization_error_payload(vec![
                    delivery_customization_required_input_field_error("title"),
                ]),
                Vec::new(),
            );
        }
        if let Some(handle) = resolved_string_field(&input, "functionHandle") {
            let Some(function) =
                self.resolve_delivery_customization_function(request, None, Some(&handle))
            else {
                return (
                    delivery_customization_error_payload(vec![
                        delivery_customization_function_not_found_error(
                            &handle,
                            &request_api_client_id(request),
                        ),
                    ]),
                    Vec::new(),
                );
            };
            let Some(function_key) = function
                .get("id")
                .and_then(Value::as_str)
                .map(delivery_customization_function_key)
            else {
                return (
                    delivery_customization_error_payload(vec![
                        delivery_customization_function_not_found_error(
                            &handle,
                            &request_api_client_id(request),
                        ),
                    ]),
                    Vec::new(),
                );
            };
            if !self.delivery_customization_record_matches_function_key(
                request,
                &existing,
                &function_key,
            ) {
                return (
                    delivery_customization_error_payload(vec![
                        delivery_customization_immutable_function_error("functionHandle"),
                    ]),
                    Vec::new(),
                );
            }
        }
        if let Some(function_id) = resolved_string_field(&input, "functionId") {
            let function_key = delivery_customization_function_key(&function_id);
            if !self.delivery_customization_record_matches_function_key(
                request,
                &existing,
                &function_key,
            ) {
                return (
                    delivery_customization_error_payload(vec![
                        delivery_customization_immutable_function_error("functionId"),
                    ]),
                    Vec::new(),
                );
            }
        }
        let metafield_errors = delivery_customization_metafield_validation_errors(&input);
        if !metafield_errors.is_empty() {
            return (
                delivery_customization_error_payload(metafield_errors),
                Vec::new(),
            );
        }

        let mut updated = existing;
        let mut changed = false;
        if let Some(title) = resolved_string_field(&input, "title") {
            if updated.get("title").and_then(Value::as_str) != Some(title.as_str()) {
                updated["title"] = json!(title);
                changed = true;
            }
        }
        if let Some(enabled) = resolved_bool_field(&input, "enabled") {
            if enabled
                && updated.get("enabled").and_then(Value::as_bool) != Some(true)
                && self.delivery_customization_enabled_count(Some(&id))
                    >= DELIVERY_CUSTOMIZATION_MAX_ENABLED
            {
                return (
                    delivery_customization_error_payload(
                        vec![delivery_customization_limit_error()],
                    ),
                    Vec::new(),
                );
            }
            if updated.get("enabled").and_then(Value::as_bool) != Some(enabled) {
                updated["enabled"] = json!(enabled);
                changed = true;
            }
        }
        if input.contains_key("metafields") {
            let timestamp = self.next_mutation_timestamp();
            let metafields = delivery_customization_metafields(
                &id,
                &input,
                api_client_id,
                &timestamp,
                Some(&updated),
            );
            delivery_customization_set_metafields(&mut updated, metafields);
            updated["updatedAt"] = json!(timestamp);
            changed = false;
        }
        if changed {
            updated["updatedAt"] = json!(self.next_mutation_timestamp());
        }
        self.store
            .staged
            .delivery_customizations
            .insert(id.clone(), updated.clone());
        (delivery_customization_record_payload(&updated), vec![id])
    }

    fn delivery_customization_activation_payload(
        &mut self,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> (Value, Vec<String>) {
        let ids = resolved_string_list_arg(arguments, "ids");
        let enabled = match arguments.get("enabled") {
            Some(ResolvedValue::Bool(value)) => *value,
            _ => false,
        };
        let mut valid_ids = Vec::new();
        let mut missing_ids = Vec::new();
        let mut limit_exceeded = false;
        let mut active_count = self.delivery_customization_enabled_count(None);
        let timestamp = self.next_mutation_timestamp();
        for id in ids {
            match self.store.staged.delivery_customizations.get_mut(&id) {
                Some(record) => {
                    let was_enabled = record.get("enabled").and_then(Value::as_bool) == Some(true);
                    if enabled && !was_enabled {
                        if active_count >= DELIVERY_CUSTOMIZATION_MAX_ENABLED {
                            limit_exceeded = true;
                            continue;
                        }
                        active_count += 1;
                    }
                    if !enabled && was_enabled {
                        active_count = active_count.saturating_sub(1);
                    }
                    if was_enabled != enabled {
                        record["enabled"] = json!(enabled);
                        record["updatedAt"] = json!(timestamp);
                    }
                    valid_ids.push(id);
                }
                None => missing_ids.push(id),
            }
        }
        let errors = if missing_ids.is_empty() {
            if limit_exceeded {
                vec![delivery_customization_limit_error()]
            } else {
                Vec::new()
            }
        } else {
            let mut errors = Vec::new();
            if limit_exceeded {
                errors.push(delivery_customization_limit_error());
            }
            errors.push(delivery_customization_activation_not_found_error(
                &missing_ids,
            ));
            errors
        };
        (
            delivery_customization_payload(None, errors, Some(valid_ids.clone()), None),
            valid_ids,
        )
    }

    fn delivery_customization_delete_payload(
        &mut self,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> (Value, Vec<String>) {
        let id = resolved_string_field(arguments, "id").unwrap_or_default();
        if self
            .store
            .staged
            .delivery_customizations
            .remove(&id)
            .is_some()
        {
            self.store
                .staged
                .delivery_customizations
                .tombstone(id.clone());
            (
                delivery_customization_payload(None, Vec::new(), None, Some(json!(id.clone()))),
                vec![id],
            )
        } else {
            (
                delivery_customization_payload(
                    None,
                    vec![delivery_customization_not_found_error(&id)],
                    None,
                    Some(Value::Null),
                ),
                Vec::new(),
            )
        }
    }

    fn delivery_customization_enabled_count(&self, excluding_id: Option<&str>) -> usize {
        self.store
            .staged
            .delivery_customizations
            .values()
            .filter(|record| {
                record.get("id").and_then(Value::as_str) != excluding_id
                    && record.get("enabled").and_then(Value::as_bool) == Some(true)
            })
            .count()
    }
}

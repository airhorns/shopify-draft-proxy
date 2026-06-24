use super::*;

pub(in crate::proxy) const MODELED_FUNCTION_APP_ID: &str = "347082227713";

impl DraftProxy {
    pub(in crate::proxy) fn functions_metadata_mutation_data(
        &mut self,
        fields: &[RootFieldSelection],
    ) -> Value {
        // Any function mutation marks the session as having local function
        // state, so later reads serve locally (read-after-write / -delete)
        // instead of forwarding the cold read to the upstream.
        self.store.staged.functions_dirty = true;
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "validationCreate" => self.function_validation_create_payload(field),
                "validationUpdate" => self.function_validation_update_payload(field),
                "validationDelete" => self.function_validation_delete_payload(field),
                "cartTransformCreate" => self.function_cart_transform_create_payload(field),
                "cartTransformDelete" => self.function_cart_transform_delete_payload(field),
                "fulfillmentConstraintRuleCreate" => {
                    self.function_fulfillment_constraint_rule_create_payload(field)
                }
                "fulfillmentConstraintRuleUpdate" => {
                    self.function_fulfillment_constraint_rule_update_payload(field)
                }
                "fulfillmentConstraintRuleDelete" => {
                    self.function_fulfillment_constraint_rule_delete_payload(field)
                }
                "taxAppConfigure" => self.function_tax_app_configure_payload(field),
                _ => Value::Null,
            };
            if !value.is_null() {
                data.insert(
                    field.response_key.clone(),
                    selected_json(&value, &field.selection),
                );
            }
        }
        Value::Object(data)
    }

    pub(in crate::proxy) fn functions_metadata_read_data(
        &self,
        fields: &[RootFieldSelection],
    ) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "validation" => resolved_field_string_arg(field, "id")
                    .and_then(|id| self.store.staged.function_validations.get(&id).cloned())
                    .or_else(|| self.store.staged.function_validation.clone())
                    .unwrap_or(Value::Null),
                "validations" => local_function_connection_from_nodes(
                    self.store
                        .staged
                        .function_validation_order
                        .iter()
                        .filter_map(|id| self.store.staged.function_validations.get(id).cloned())
                        .collect(),
                ),
                "cartTransforms" => local_function_connection_from_nodes(
                    self.store
                        .staged
                        .function_cart_transform_order
                        .iter()
                        .filter_map(|id| {
                            self.store
                                .staged
                                .function_cart_transforms
                                .get(id)
                                .map(|record| {
                                    cart_transform_record_for_selection(record, &field.selection)
                                })
                        })
                        .collect(),
                ),
                "fulfillmentConstraintRules" => Value::Array(
                    self.store
                        .staged
                        .function_fulfillment_constraint_rule_order
                        .iter()
                        .filter_map(|id| {
                            self.store
                                .staged
                                .function_fulfillment_constraint_rules
                                .get(id)
                                .map(|record| {
                                    fulfillment_constraint_rule_record_for_selection(
                                        record,
                                        &field.selection,
                                    )
                                })
                        })
                        .map(|record| selected_json(&record, &field.selection))
                        .collect(),
                ),
                "shopifyFunctions" => {
                    let api_type = resolved_enum_arg(field, "apiType").unwrap_or_default();
                    let api_type = match api_type.as_str() {
                        "CART_TRANSFORM" | "cart_transform" => "CART_TRANSFORM",
                        "FULFILLMENT_CONSTRAINT_RULE" | "fulfillment_constraint_rule" => {
                            "FULFILLMENT_CONSTRAINT_RULE"
                        }
                        _ => "VALIDATION",
                    };
                    json!({ "nodes": self.function_catalog_read_nodes(api_type) })
                }
                "shopifyFunction" => match resolved_field_string_arg(field, "id") {
                    Some(id) => {
                        function_by_id_or_handle(Some(id.as_str()), None).unwrap_or(Value::Null)
                    }
                    None => local_cart_transform_function(),
                },
                _ => Value::Null,
            };
            if value.is_null() {
                data.insert(field.response_key.clone(), Value::Null);
            } else if field.name == "fulfillmentConstraintRules" {
                data.insert(field.response_key.clone(), value);
            } else {
                data.insert(
                    field.response_key.clone(),
                    selected_json(&value, &field.selection),
                );
            }
        }
        Value::Object(data)
    }

    fn function_catalog_read_nodes(&self, api_type: &str) -> Vec<Value> {
        let mut seen = BTreeSet::new();
        let mut nodes = Vec::new();
        for function in self
            .store
            .staged
            .function_validation_order
            .iter()
            .filter_map(|id| self.store.staged.function_validations.get(id))
            .chain(
                self.store
                    .staged
                    .function_cart_transform_order
                    .iter()
                    .filter_map(|id| self.store.staged.function_cart_transforms.get(id)),
            )
            .chain(
                self.store
                    .staged
                    .function_fulfillment_constraint_rule_order
                    .iter()
                    .filter_map(|id| {
                        self.store
                            .staged
                            .function_fulfillment_constraint_rules
                            .get(id)
                    }),
            )
            .filter_map(|record| record.get("shopifyFunction"))
        {
            if function["apiType"].as_str() == Some(api_type) {
                if let Some(id) = function["id"].as_str() {
                    if seen.insert(id.to_string()) {
                        nodes.push(function.clone());
                    }
                }
            }
        }
        if nodes.is_empty() {
            function_catalog_by_api_type(api_type)
        } else {
            nodes
        }
    }

    /// True when any function lifecycle has been staged locally (a validation or
    /// cart-transform created/updated this session). Cold function reads with no
    /// staged state forward to the upstream so `shopifyFunctions` /
    /// `shopifyFunction` reflect the shop's real installed functions (with app
    /// ownership metadata) rather than the synthetic staging catalog.
    pub(in crate::proxy) fn local_has_function_state(&self) -> bool {
        self.store.staged.functions_dirty
            || self.store.staged.function_validation.is_some()
            || !self.store.staged.function_validations.is_empty()
            || !self.store.staged.function_validation_order.is_empty()
            || !self.store.staged.function_cart_transforms.is_empty()
            || !self.store.staged.function_cart_transform_order.is_empty()
            || !self
                .store
                .staged
                .function_fulfillment_constraint_rules
                .is_empty()
            || !self
                .store
                .staged
                .function_fulfillment_constraint_rule_order
                .is_empty()
    }
}

pub(in crate::proxy) fn function_by_id_or_handle(
    id: Option<&str>,
    handle: Option<&str>,
) -> Option<Value> {
    function_catalog().into_iter().find(|function| {
        id.is_some_and(|id| function["id"].as_str() == Some(id))
            || handle.is_some_and(|handle| function["handle"].as_str() == Some(handle))
    })
}

pub(in crate::proxy) fn function_catalog_by_api_type(api_type: &str) -> Vec<Value> {
    function_catalog()
        .into_iter()
        .filter(|function| function["apiType"].as_str() == Some(api_type))
        .collect()
}

fn function_catalog() -> Vec<Value> {
    vec![
        local_validation_function(),
        json!({
            "id": "gid://shopify/ShopifyFunction/validation-alpha",
            "title": "Validation Alpha",
            "handle": "validation-alpha",
            "apiType": "VALIDATION"
        }),
        json!({
            "id": "gid://shopify/ShopifyFunction/validation-beta",
            "title": "Validation Beta",
            "handle": "validation-beta",
            "apiType": "VALIDATION"
        }),
        json!({
            "id": "019dd44b-127f-7061-a930-422cbd4a751f",
            "title": "t:name",
            "handle": "conformance-validation",
            "apiType": "VALIDATION"
        }),
        functions_owner_validation_function(),
        local_cart_transform_function(),
        json!({
            "id": "gid://shopify/ShopifyFunction/cart-beta",
            "title": "Cart Beta",
            "handle": "cart-beta",
            "apiType": "CART_TRANSFORM"
        }),
        json!({
            "id": "019dd44b-127f-724b-a49c-70fc98ff4d72",
            "title": "Conformance Cart Transform",
            "handle": "conformance-cart-transform",
            "apiType": "CART_TRANSFORM"
        }),
        json!({
            "id": "019dd44b-127f-724b-a49c-70fc98ff4d72",
            "title": "Conformance Cart Transform",
            "handle": "cart-transform-delete-shape",
            "apiType": "CART_TRANSFORM"
        }),
        functions_owner_cart_function(),
        local_fulfillment_constraint_rule_function(),
        json!({
            "id": "gid://shopify/ShopifyFunction/guardrail-validation-plan",
            "title": "Guardrail validation plan",
            "handle": "guardrail-validation-plan",
            "apiType": "VALIDATION",
            "createGuardrailCode": "CUSTOM_APP_FUNCTION_NOT_ELIGIBLE",
            "createGuardrailMessage": "Shop must be on a Shopify Plus plan to activate functions from a custom app."
        }),
        json!({
            "id": "gid://shopify/ShopifyFunction/guardrail-validation-required-input",
            "title": "Guardrail validation required input",
            "handle": "guardrail-validation-required-input",
            "apiType": "VALIDATION",
            "createGuardrailCode": "REQUIRED_INPUT_FIELD",
            "createGuardrailMessage": "Required input field must be present."
        }),
        json!({
            "id": "gid://shopify/ShopifyFunction/guardrail-cart-transform-plan",
            "title": "Guardrail cart transform plan",
            "handle": "guardrail-cart-transform-plan",
            "apiType": "CART_TRANSFORM",
            "createGuardrailCode": "CUSTOM_APP_FUNCTION_NOT_ELIGIBLE",
            "createGuardrailMessage": "Shop must be on a Shopify Plus plan to activate functions from a custom app."
        }),
        json!({
            "id": "gid://shopify/ShopifyFunction/guardrail-cart-transform-pending-deletion",
            "title": "Guardrail cart transform pending deletion",
            "handle": "guardrail-cart-transform-pending-deletion",
            "apiType": "CART_TRANSFORM",
            "createGuardrailCode": "FUNCTION_PENDING_DELETION",
            "createGuardrailMessage": "Function is pending deletion."
        }),
        json!({
            "id": "gid://shopify/ShopifyFunction/guardrail-cart-transform-plus-only",
            "title": "Guardrail cart transform Plus only",
            "handle": "guardrail-cart-transform-plus-only",
            "apiType": "CART_TRANSFORM",
            "createGuardrailCode": "FUNCTION_IS_PLUS_ONLY",
            "createGuardrailMessage": "Shop must be on a Shopify Plus plan to activate this function."
        }),
    ]
}

fn function_identifier_input(
    input: &BTreeMap<String, ResolvedValue>,
) -> (Option<String>, Option<String>) {
    (
        resolved_string_field(input, "functionId"),
        resolved_string_field(input, "functionHandle"),
    )
}

fn function_payload_identifier_field(function_id: &Option<String>) -> &'static str {
    if function_id.is_some() {
        "functionId"
    } else {
        "functionHandle"
    }
}

fn function_user_error(field: Vec<Value>, message: &str, code: Option<&str>) -> Value {
    user_error(field, message, code)
}

fn validation_payload_error(error: Value) -> Value {
    json!({ "validation": Value::Null, "userErrors": [error] })
}

fn cart_transform_payload_error(error: Value) -> Value {
    json!({ "cartTransform": Value::Null, "userErrors": [error] })
}

fn validation_identifier_error(input: &BTreeMap<String, ResolvedValue>) -> Option<Value> {
    let (function_id, function_handle) = function_identifier_input(input);
    match (function_id.is_some(), function_handle.is_some()) {
        (false, false) => Some(validation_payload_error(function_user_error(
            vec![json!("validation"), json!("functionHandle")],
            "Either function_id or function_handle must be provided.",
            Some("MISSING_FUNCTION_IDENTIFIER"),
        ))),
        (true, true) => Some(validation_payload_error(function_user_error(
            vec![json!("validation")],
            "Only one of function_id or function_handle can be provided, not both.",
            Some("MULTIPLE_FUNCTION_IDENTIFIERS"),
        ))),
        _ => None,
    }
}

fn cart_transform_identifier_error(
    function_id: &Option<String>,
    function_handle: &Option<String>,
) -> Option<Value> {
    match (function_id.is_some(), function_handle.is_some()) {
        (false, false) => Some(cart_transform_payload_error(function_user_error(
            vec![json!("functionHandle")],
            "Either function_id or function_handle must be provided.",
            Some("MISSING_FUNCTION_IDENTIFIER"),
        ))),
        (true, true) => Some(cart_transform_payload_error(function_user_error(
            vec![json!("functionHandle")],
            "Only one of function_id or function_handle can be provided, not both.",
            Some("MULTIPLE_FUNCTION_IDENTIFIERS"),
        ))),
        _ => None,
    }
}

fn cart_transform_function_not_found_error(
    field_name: &str,
    function_id: &Option<String>,
    function_handle: &Option<String>,
) -> Value {
    let message = if let Some(id) = function_id {
        format!(
            "Function {id} not found. Ensure that it is released in the current app ({MODELED_FUNCTION_APP_ID}), and that the app is installed."
        )
    } else if let Some(handle) = function_handle {
        format!("Could not find function with handle: {handle}.")
    } else {
        "Function not found.".to_string()
    };
    cart_transform_payload_error(function_user_error(
        vec![json!(field_name)],
        &message,
        Some("FUNCTION_NOT_FOUND"),
    ))
}

fn validation_function_resolution_payload(
    input: &BTreeMap<String, ResolvedValue>,
) -> Result<Value, Value> {
    if let Some(payload) = validation_identifier_error(input) {
        return Err(payload);
    }
    let (function_id, function_handle) = function_identifier_input(input);
    let field_name = function_payload_identifier_field(&function_id);
    let function = function_by_id_or_handle(function_id.as_deref(), function_handle.as_deref())
        .ok_or_else(|| {
            validation_payload_error(function_user_error(
                vec![json!("validation"), json!(field_name)],
                "Extension not found.",
                Some("NOT_FOUND"),
            ))
        })?;
    if function["apiType"].as_str() != Some("VALIDATION") {
        return Err(validation_payload_error(function_user_error(
            vec![json!("validation"), json!(field_name)],
            "Unexpected Function API. The provided function must implement one of the following extension targets: [%{targets}].",
            Some("FUNCTION_DOES_NOT_IMPLEMENT"),
        )));
    }
    if let Some(code) = function["createGuardrailCode"].as_str() {
        return Err(validation_payload_error(function_user_error(
            vec![json!("validation"), json!(field_name)],
            function["createGuardrailMessage"]
                .as_str()
                .unwrap_or_default(),
            Some(code),
        )));
    }
    Ok(function)
}

fn cart_transform_function_resolution_payload(
    function_id: &Option<String>,
    function_handle: &Option<String>,
) -> Result<Value, Value> {
    if let Some(payload) = cart_transform_identifier_error(function_id, function_handle) {
        return Err(payload);
    }
    let field_name = function_payload_identifier_field(function_id);
    let function = function_by_id_or_handle(function_id.as_deref(), function_handle.as_deref())
        .ok_or_else(|| {
            cart_transform_function_not_found_error(field_name, function_id, function_handle)
        })?;
    if function["apiType"].as_str() != Some("CART_TRANSFORM") {
        let code = if function_id.is_some() {
            "FUNCTION_NOT_FOUND"
        } else {
            "FUNCTION_DOES_NOT_IMPLEMENT"
        };
        return Err(cart_transform_payload_error(function_user_error(
            vec![json!(field_name)],
            "Unexpected Function API. The provided function must implement one of the following extension targets: [purchase.cart-transform.run, cart.transform.run].",
            Some(code),
        )));
    }
    if let Some(code) = function["createGuardrailCode"].as_str() {
        return Err(cart_transform_payload_error(function_user_error(
            vec![json!(field_name)],
            function["createGuardrailMessage"]
                .as_str()
                .unwrap_or_default(),
            Some(code),
        )));
    }
    Ok(function)
}

fn metafield_input_error(
    metafield: &BTreeMap<String, ResolvedValue>,
    index: usize,
) -> Option<Value> {
    let field = vec![
        json!("validation"),
        json!("metafields"),
        json!(index.to_string()),
    ];
    let namespace = resolved_string_field(metafield, "namespace").unwrap_or_default();
    let key = resolved_string_field(metafield, "key");
    let type_name = resolved_string_field(metafield, "type");
    let value = resolved_string_field(metafield, "value");

    if key.is_none() {
        return Some(function_user_error(field, "presence", None));
    }
    if type_name.as_deref().unwrap_or_default().is_empty() {
        return Some(function_user_error(
            field,
            "One or more required inputs are blank.",
            Some("BLANK"),
        ));
    }
    if value.is_none() {
        return Some(function_user_error(field, "presence", None));
    }
    if namespace == "shopify" {
        return Some(function_user_error(
            field,
            "ApiPermission metafields can only be created or updated by the app owner.",
            Some("APP_NOT_AUTHORIZED"),
        ));
    }
    match type_name.as_deref() {
        Some("single_line_text_field") => {
            if value.as_deref() == Some("") {
                Some(function_user_error(
                    field,
                    "The value is invalid.",
                    Some("INVALID_VALUE"),
                ))
            } else {
                None
            }
        }
        Some("number_integer") => {
            if value
                .as_deref()
                .is_some_and(|value| value.parse::<i64>().is_ok())
            {
                None
            } else {
                Some(function_user_error(
                    field,
                    "The value is invalid.",
                    Some("INVALID_VALUE"),
                ))
            }
        }
        Some("json") => None,
        _ => Some(function_user_error(
            field,
            "The type is invalid.",
            Some("INVALID_TYPE"),
        )),
    }
}

fn validation_metafield_errors(input: &BTreeMap<String, ResolvedValue>) -> Vec<Value> {
    match input.get("metafields") {
        Some(ResolvedValue::List(metafields)) => metafields
            .iter()
            .enumerate()
            .filter_map(|(index, value)| match value {
                ResolvedValue::Object(metafield) => metafield_input_error(metafield, index),
                _ => Some(function_user_error(
                    vec![
                        json!("validation"),
                        json!("metafields"),
                        json!(index.to_string()),
                    ],
                    "The value is invalid.",
                    Some("INVALID_VALUE"),
                )),
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn validation_metafields_from_input(input: &BTreeMap<String, ResolvedValue>) -> Vec<Value> {
    match input.get("metafields") {
        Some(ResolvedValue::List(metafields)) => metafields
            .iter()
            .filter_map(|value| match value {
                ResolvedValue::Object(metafield) => Some(json!({
                    "namespace": resolved_string_field(metafield, "namespace").unwrap_or_default(),
                    "key": resolved_string_field(metafield, "key").unwrap_or_default(),
                    "type": resolved_string_field(metafield, "type").unwrap_or_default(),
                    "value": resolved_string_field(metafield, "value").unwrap_or_default(),
                    "updatedAt": "2026-05-07T08:02:25Z"
                })),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn validation_metafield_connection(metafields: Vec<Value>) -> Value {
    json!({ "nodes": metafields })
}

fn upsert_validation_metafields(record: &mut Value, metafields: Vec<Value>) {
    let existing = record["metafields"]["nodes"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mut merged = existing;
    for metafield in metafields {
        let namespace = metafield["namespace"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let key = metafield["key"].as_str().unwrap_or_default().to_string();
        if let Some(existing) = merged.iter_mut().find(|existing| {
            existing["namespace"].as_str() == Some(namespace.as_str())
                && existing["key"].as_str() == Some(key.as_str())
        }) {
            *existing = metafield;
        } else {
            merged.push(metafield);
        }
    }
    record["metafields"] = validation_metafield_connection(merged);
}

fn selected_title(input: &BTreeMap<String, ResolvedValue>, function: &Value) -> String {
    match input.get("title") {
        Some(ResolvedValue::String(title)) => title.clone(),
        Some(ResolvedValue::Null) | None => {
            function["title"].as_str().unwrap_or_default().to_string()
        }
        _ => String::new(),
    }
}

fn active_validation_count(records: &BTreeMap<String, Value>, exclude_id: Option<&str>) -> usize {
    records
        .iter()
        .filter(|(id, record)| {
            Some(id.as_str()) != exclude_id && record["enable"].as_bool() == Some(true)
        })
        .count()
}

pub(in crate::proxy) fn local_function_connection_from_nodes(nodes: Vec<Value>) -> Value {
    let start_cursor = nodes
        .first()
        .and_then(|node| node["id"].as_str())
        .map(|id| format!("cursor:{id}"));
    let end_cursor = nodes
        .last()
        .and_then(|node| node["id"].as_str())
        .map(|id| format!("cursor:{id}"));
    json!({
        "nodes": nodes,
        "pageInfo": {
            "hasNextPage": false,
            "hasPreviousPage": false,
            "startCursor": start_cursor.map(Value::from).unwrap_or(Value::Null),
            "endCursor": end_cursor.map(Value::from).unwrap_or(Value::Null)
        }
    })
}

fn cart_transform_metafield_error(
    metafield: &BTreeMap<String, ResolvedValue>,
    index: usize,
) -> Option<Value> {
    let value = resolved_string_field(metafield, "value").unwrap_or_default();
    if value.is_empty() {
        return Some(function_user_error(
            vec![
                json!("metafields"),
                json!(index.to_string()),
                json!("value"),
            ],
            "may not be empty",
            Some("INVALID_METAFIELDS"),
        ));
    }
    if resolved_string_field(metafield, "type").as_deref() == Some("json")
        && serde_json::from_str::<Value>(&value).is_err()
    {
        return Some(function_user_error(
            vec![
                json!("metafields"),
                json!(index.to_string()),
                json!("value"),
            ],
            &format!(
                "is invalid JSON: unexpected token '{}' at line 1 column 1.",
                value
            ),
            Some("INVALID_METAFIELDS"),
        ));
    }
    None
}

fn cart_transform_metafield_errors(field: &RootFieldSelection) -> Vec<Value> {
    match field.arguments.get("metafields") {
        Some(ResolvedValue::List(metafields)) => metafields
            .iter()
            .enumerate()
            .filter_map(|(index, value)| match value {
                ResolvedValue::Object(metafield) => {
                    cart_transform_metafield_error(metafield, index)
                }
                _ => Some(function_user_error(
                    vec![
                        json!("metafields"),
                        json!(index.to_string()),
                        json!("value"),
                    ],
                    "may not be empty",
                    Some("INVALID_METAFIELDS"),
                )),
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn staged_function_id_in_use(records: &BTreeMap<String, Value>, function_id: &str) -> bool {
    records
        .values()
        .any(|record| record["functionId"].as_str() == Some(function_id))
}

pub(in crate::proxy) fn cart_transform_metafields_from_field(
    field: &RootFieldSelection,
    ids: Vec<String>,
) -> Vec<Value> {
    match field.arguments.get("metafields") {
        Some(ResolvedValue::List(metafields)) => metafields
            .iter()
            .enumerate()
            .filter_map(|(index, value)| match value {
                ResolvedValue::Object(metafield) => {
                    let now = "2026-05-07T17:20:12Z";
                    Some(json!({
                        "id": match index {
                            0 => "gid://shopify/Metafield/43125986558258".to_string(),
                            1 => "gid://shopify/Metafield/43125986591026".to_string(),
                            _ => ids.get(index).cloned().unwrap_or_else(|| format!("gid://shopify/Metafield/{}", index + 1)),
                        },
                        "namespace": resolved_string_field(metafield, "namespace").unwrap_or_default(),
                        "key": resolved_string_field(metafield, "key").unwrap_or_default(),
                        "type": resolved_string_field(metafield, "type").unwrap_or_default(),
                        "value": resolved_string_field(metafield, "value").unwrap_or_default(),
                        "compareDigest": match index {
                            0 => "58440d4e2b7e81e7a5318441381af282c0a2ec83cf926af55397244ff23e1181".to_string(),
                            1 => "c30b019a8fd5bb26e69d73f4a11d3c12ac733b6063d8be2562d08dd2ce61344b".to_string(),
                            _ => format!("proxy-digest-{}", index + 1),
                        },
                        "ownerType": "CARTTRANSFORM",
                        "createdAt": now,
                        "updatedAt": now
                    }))
                }
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

pub(in crate::proxy) fn cart_transform_record_for_selection(
    record: &Value,
    connection_selection: &[SelectedField],
) -> Value {
    let mut record = record.clone();
    let Some(node_selection) = selected_child_selection(connection_selection, "nodes") else {
        return record;
    };
    let Some(metafield_selection) = node_selection
        .iter()
        .find(|field| field.name == "metafield")
    else {
        return record;
    };
    let namespace = metafield_selection
        .arguments
        .get("namespace")
        .and_then(|value| match value {
            ResolvedValue::String(value) => Some(value.as_str()),
            _ => None,
        });
    let key = metafield_selection
        .arguments
        .get("key")
        .and_then(|value| match value {
            ResolvedValue::String(value) => Some(value.as_str()),
            _ => None,
        });
    if let (Some(namespace), Some(key)) = (namespace, key) {
        let metafield = record["metafields"]["nodes"]
            .as_array()
            .and_then(|nodes| {
                nodes.iter().find(|node| {
                    node["namespace"].as_str() == Some(namespace)
                        && node["key"].as_str() == Some(key)
                })
            })
            .cloned()
            .unwrap_or(Value::Null);
        record["metafield"] = metafield;
    }
    record
}

fn fulfillment_constraint_rule_payload_error(error: Value) -> Value {
    json!({ "fulfillmentConstraintRule": Value::Null, "userErrors": [error] })
}

fn fulfillment_constraint_rule_identifier_error(
    function_id: &Option<String>,
    function_handle: &Option<String>,
) -> Option<Value> {
    match (function_id.is_some(), function_handle.is_some()) {
        (false, false) => Some(fulfillment_constraint_rule_payload_error(
            function_user_error(
                vec![json!("functionHandle")],
                "Either function_id or function_handle must be provided.",
                Some("MISSING_FUNCTION_IDENTIFIER"),
            ),
        )),
        (true, true) => Some(fulfillment_constraint_rule_payload_error(
            function_user_error(
                vec![json!("functionHandle")],
                "Only one of function_id or function_handle can be provided, not both.",
                Some("MULTIPLE_FUNCTION_IDENTIFIERS"),
            ),
        )),
        _ => None,
    }
}

fn fulfillment_constraint_rule_delivery_method_types(field: &RootFieldSelection) -> Vec<String> {
    match field.arguments.get("deliveryMethodTypes") {
        Some(ResolvedValue::List(values)) => values
            .iter()
            .filter_map(|value| match value {
                ResolvedValue::String(value) => Some(value.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn fulfillment_constraint_rule_delivery_method_error(
    delivery_method_types: &[String],
) -> Option<Value> {
    if delivery_method_types.is_empty() {
        Some(fulfillment_constraint_rule_payload_error(
            function_user_error(
                vec![json!("deliveryMethodTypes")],
                "Delivery method types cannot be empty.",
                Some("INPUT_INVALID"),
            ),
        ))
    } else {
        None
    }
}

fn fulfillment_constraint_rule_function_not_found_error(
    field_name: &str,
    function_id: &Option<String>,
    function_handle: &Option<String>,
) -> Value {
    let message = if let Some(handle) = function_handle {
        format!("Could not find function with handle: {handle}.")
    } else if let Some(id) = function_id {
        format!("Could not find function with id: {id}.")
    } else {
        "Could not find function.".to_string()
    };
    fulfillment_constraint_rule_payload_error(function_user_error(
        vec![json!(field_name)],
        &message,
        Some("FUNCTION_NOT_FOUND"),
    ))
}

fn fulfillment_constraint_rule_function_resolution_payload(
    function_id: &Option<String>,
    function_handle: &Option<String>,
) -> Result<Value, Value> {
    if let Some(payload) =
        fulfillment_constraint_rule_identifier_error(function_id, function_handle)
    {
        return Err(payload);
    }
    let field_name = function_payload_identifier_field(function_id);
    let function = function_by_id_or_handle(function_id.as_deref(), function_handle.as_deref())
        .ok_or_else(|| {
            fulfillment_constraint_rule_function_not_found_error(
                field_name,
                function_id,
                function_handle,
            )
        })?;
    if function["apiType"].as_str() != Some("FULFILLMENT_CONSTRAINT_RULE") {
        return Err(fulfillment_constraint_rule_payload_error(
            function_user_error(
                vec![json!(field_name)],
                "Unexpected Function API. The provided function must implement one of the following extension targets: [purchase.fulfillment-constraint-rule.run, cart.fulfillment-constraints.generate.run].",
                Some("FUNCTION_DOES_NOT_IMPLEMENT"),
            ),
        ));
    }
    if let Some(code) = function["createGuardrailCode"].as_str() {
        return Err(fulfillment_constraint_rule_payload_error(
            function_user_error(
                vec![json!(field_name)],
                function["createGuardrailMessage"]
                    .as_str()
                    .unwrap_or_default(),
                Some(code),
            ),
        ));
    }
    Ok(function)
}

fn fulfillment_constraint_rule_metafields_from_field(
    field: &RootFieldSelection,
    ids: Vec<String>,
) -> Vec<Value> {
    match field.arguments.get("metafields") {
        Some(ResolvedValue::List(metafields)) => metafields
            .iter()
            .enumerate()
            .filter_map(|(index, value)| match value {
                ResolvedValue::Object(metafield) => {
                    let now = "2026-05-07T17:20:12Z";
                    Some(json!({
                        "id": ids.get(index).cloned().unwrap_or_else(|| format!("gid://shopify/Metafield/{}", index + 1)),
                        "namespace": resolved_string_field(metafield, "namespace").unwrap_or_default(),
                        "key": resolved_string_field(metafield, "key").unwrap_or_default(),
                        "type": resolved_string_field(metafield, "type").unwrap_or_default(),
                        "value": resolved_string_field(metafield, "value").unwrap_or_default(),
                        "compareDigest": format!("proxy-fulfillment-constraint-digest-{}", index + 1),
                        "ownerType": "FULFILLMENTCONSTRAINTRULE",
                        "createdAt": now,
                        "updatedAt": now
                    }))
                }
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

pub(in crate::proxy) fn fulfillment_constraint_rule_record_for_selection(
    record: &Value,
    selection: &[SelectedField],
) -> Value {
    let mut record = record.clone();
    let Some(metafield_selection) = selection.iter().find(|field| field.name == "metafield") else {
        return record;
    };
    let namespace = metafield_selection
        .arguments
        .get("namespace")
        .and_then(|value| match value {
            ResolvedValue::String(value) => Some(value.as_str()),
            _ => None,
        });
    let key = metafield_selection
        .arguments
        .get("key")
        .and_then(|value| match value {
            ResolvedValue::String(value) => Some(value.as_str()),
            _ => None,
        });
    if let (Some(namespace), Some(key)) = (namespace, key) {
        let metafield = record["metafields"]["nodes"]
            .as_array()
            .and_then(|nodes| {
                nodes.iter().find(|node| {
                    node["namespace"].as_str() == Some(namespace)
                        && node["key"].as_str() == Some(key)
                })
            })
            .cloned()
            .unwrap_or(Value::Null);
        record["metafield"] = metafield;
    }
    record
}

impl DraftProxy {
    pub(in crate::proxy) fn function_validation_create_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let input = match field.arguments.get("validation") {
            Some(ResolvedValue::Object(input)) => input,
            _ => {
                return validation_payload_error(function_user_error(
                    vec![json!("validation")],
                    "Required input field must be present.",
                    Some("REQUIRED_INPUT_FIELD"),
                ));
            }
        };
        let function = match validation_function_resolution_payload(input) {
            Ok(function) => function,
            Err(payload) => return payload,
        };
        let errors = validation_metafield_errors(input);
        if !errors.is_empty() {
            return json!({ "validation": Value::Null, "userErrors": errors });
        }
        let enable = resolved_bool_field(input, "enable").unwrap_or(false);
        if enable && active_validation_count(&self.store.staged.function_validations, None) >= 25 {
            return validation_payload_error(function_user_error(
                Vec::new(),
                "Cannot have more than 25 active validation functions.",
                Some("MAX_VALIDATIONS_ACTIVATED"),
            ));
        }
        let id = if self.store.staged.function_validation_order.is_empty() {
            "gid://shopify/Validation/2".to_string()
        } else {
            format!(
                "gid://shopify/Validation/{}",
                self.store.staged.function_validation_order.len() + 2
            )
        };
        let metafields = validation_metafields_from_input(input);
        let validation = json!({
            "id": id,
            "title": selected_title(input, &function),
            "enable": enable,
            "enabled": enable,
            "blockOnFailure": resolved_bool_field(input, "blockOnFailure").unwrap_or(false),
            "functionId": function["id"].clone(),
            "functionHandle": function["handle"].clone(),
            "createdAt": "2024-01-01T00:00:01.000Z",
            "updatedAt": "2024-01-01T00:00:01.000Z",
            "shopifyFunction": function,
            "metafields": validation_metafield_connection(metafields)
        });
        self.stage_function_validation(validation.clone());
        json!({ "validation": validation, "userErrors": [] })
    }

    pub(in crate::proxy) fn function_validation_update_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let id = resolved_field_string_arg(field, "id").unwrap_or_default();
        let input = match field.arguments.get("validation") {
            Some(ResolvedValue::Object(input)) => input,
            _ => {
                return validation_payload_error(function_user_error(
                    vec![json!("validation")],
                    "Required input field must be present.",
                    Some("REQUIRED_INPUT_FIELD"),
                ));
            }
        };
        let Some(mut validation) = self.store.staged.function_validations.get(&id).cloned() else {
            return validation_payload_error(function_user_error(
                vec![json!("id")],
                "Extension not found.",
                Some("NOT_FOUND"),
            ));
        };
        let errors = validation_metafield_errors(input);
        if !errors.is_empty() {
            return json!({ "validation": Value::Null, "userErrors": errors });
        }
        let next_enable = resolved_bool_field(input, "enable")
            .or_else(|| resolved_bool_field(input, "enabled"))
            .unwrap_or(false);
        if next_enable
            && active_validation_count(&self.store.staged.function_validations, Some(&id)) >= 25
        {
            return validation_payload_error(function_user_error(
                Vec::new(),
                "Cannot have more than 25 active validation functions.",
                Some("MAX_VALIDATIONS_ACTIVATED"),
            ));
        }
        if let Some(title) = resolved_string_field(input, "title") {
            validation["title"] = json!(title);
        }
        validation["enable"] = json!(next_enable);
        validation["enabled"] = json!(next_enable);
        validation["blockOnFailure"] =
            json!(resolved_bool_field(input, "blockOnFailure").unwrap_or(false));
        validation["updatedAt"] = json!("2024-01-01T00:00:05.000Z");
        upsert_validation_metafields(&mut validation, validation_metafields_from_input(input));
        self.stage_function_validation(validation.clone());
        json!({ "validation": validation, "userErrors": [] })
    }

    pub(in crate::proxy) fn function_validation_delete_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let id = resolved_field_string_arg(field, "id").unwrap_or_default();
        if self.store.staged.function_validations.remove(&id).is_some() {
            self.store
                .staged
                .function_validation_order
                .retain(|ordered_id| ordered_id != &id);
            if self
                .store
                .staged
                .function_validation
                .as_ref()
                .and_then(|record| record["id"].as_str())
                == Some(id.as_str())
            {
                self.store.staged.function_validation = self
                    .store
                    .staged
                    .function_validation_order
                    .last()
                    .and_then(|id| self.store.staged.function_validations.get(id).cloned());
            }
            json!({ "deletedId": id, "userErrors": [] })
        } else {
            json!({
                "deletedId": Value::Null,
                "userErrors": [{
                    "field": ["id"],
                    "message": "Extension not found.",
                    "code": "NOT_FOUND"
                }]
            })
        }
    }

    pub(in crate::proxy) fn function_cart_transform_create_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let function_id = resolved_field_string_arg(field, "functionId");
        let function_handle = resolved_field_string_arg(field, "functionHandle");
        if let Some(payload) = cart_transform_identifier_error(&function_id, &function_handle) {
            return payload;
        }
        if let Some(function_id) = function_id.as_deref() {
            if staged_function_id_in_use(&self.store.staged.function_validations, function_id)
                || staged_function_id_in_use(
                    &self.store.staged.function_cart_transforms,
                    function_id,
                )
            {
                return cart_transform_payload_error(function_user_error(
                    vec![json!("functionId")],
                    "Could not enable cart transform because it is already registered",
                    Some("FUNCTION_ALREADY_REGISTERED"),
                ));
            }
        }
        let function =
            match cart_transform_function_resolution_payload(&function_id, &function_handle) {
                Ok(function) => function,
                Err(payload) => return payload,
            };
        let errors = cart_transform_metafield_errors(field);
        if !errors.is_empty() {
            return json!({ "cartTransform": Value::Null, "userErrors": errors });
        }
        let id = if self.store.staged.function_cart_transform_order.is_empty() {
            "gid://shopify/CartTransform/3".to_string()
        } else {
            format!(
                "gid://shopify/CartTransform/{}",
                self.store.staged.function_cart_transform_order.len() + 3
            )
        };
        let metafield_ids = match field.arguments.get("metafields") {
            Some(ResolvedValue::List(metafields)) => metafields
                .iter()
                .map(|_| self.next_proxy_synthetic_gid("Metafield"))
                .collect(),
            _ => Vec::new(),
        };
        let metafields = cart_transform_metafields_from_field(field, metafield_ids);
        let first_metafield = metafields.first().cloned().unwrap_or(Value::Null);
        let mut cart_transform = json!({
            "id": id,
            "blockOnFailure": resolved_bool_field(&field.arguments, "blockOnFailure").unwrap_or(false),
            "functionId": function["id"].clone(),
            "shopifyFunction": function,
            "metafield": first_metafield,
            "metafields": { "nodes": metafields }
        });
        if cart_transform["metafield"].is_null() {
            cart_transform.as_object_mut().unwrap().remove("metafield");
        }
        self.stage_function_cart_transform(cart_transform.clone());
        json!({ "cartTransform": cart_transform, "userErrors": [] })
    }

    pub(in crate::proxy) fn function_cart_transform_delete_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let id = resolved_field_string_arg(field, "id").unwrap_or_default();
        if self
            .store
            .staged
            .function_cart_transforms
            .remove(&id)
            .is_some()
        {
            self.store
                .staged
                .function_cart_transform_order
                .retain(|ordered_id| ordered_id != &id);
            if self
                .store
                .staged
                .function_cart_transform
                .as_ref()
                .and_then(|record| record["id"].as_str())
                == Some(id.as_str())
            {
                self.store.staged.function_cart_transform = self
                    .store
                    .staged
                    .function_cart_transform_order
                    .last()
                    .and_then(|id| self.store.staged.function_cart_transforms.get(id).cloned());
            }
            json!({ "deletedId": id, "userErrors": [] })
        } else {
            json!({
                "deletedId": Value::Null,
                "userErrors": [{
                    "field": ["id"],
                    "message": format!("Could not find cart transform with id: {id}"),
                    "code": "NOT_FOUND"
                }]
            })
        }
    }

    pub(in crate::proxy) fn function_fulfillment_constraint_rule_create_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let function_id = resolved_field_string_arg(field, "functionId");
        let function_handle = resolved_field_string_arg(field, "functionHandle");
        if let Some(payload) =
            fulfillment_constraint_rule_identifier_error(&function_id, &function_handle)
        {
            return payload;
        }
        let delivery_method_types = fulfillment_constraint_rule_delivery_method_types(field);
        if let Some(payload) =
            fulfillment_constraint_rule_delivery_method_error(&delivery_method_types)
        {
            return payload;
        }
        let function = match fulfillment_constraint_rule_function_resolution_payload(
            &function_id,
            &function_handle,
        ) {
            Ok(function) => function,
            Err(payload) => return payload,
        };
        let id = format!(
            "gid://shopify/FulfillmentConstraintRule/{}",
            self.store
                .staged
                .function_fulfillment_constraint_rule_order
                .len()
                + 1
        );
        let metafield_ids = match field.arguments.get("metafields") {
            Some(ResolvedValue::List(metafields)) => metafields
                .iter()
                .map(|_| self.next_proxy_synthetic_gid("Metafield"))
                .collect(),
            _ => Vec::new(),
        };
        let metafields = fulfillment_constraint_rule_metafields_from_field(field, metafield_ids);
        let first_metafield = metafields.first().cloned().unwrap_or(Value::Null);
        let mut rule = json!({
            "id": id,
            "deliveryMethodTypes": delivery_method_types,
            "functionId": function["id"].clone(),
            "functionHandle": function["handle"].clone(),
            "function": function.clone(),
            "shopifyFunction": function,
            "metafield": first_metafield,
            "metafields": { "nodes": metafields }
        });
        if rule["metafield"].is_null() {
            rule.as_object_mut().unwrap().remove("metafield");
        }
        self.stage_function_fulfillment_constraint_rule(rule.clone());
        json!({ "fulfillmentConstraintRule": rule, "userErrors": [] })
    }

    pub(in crate::proxy) fn function_fulfillment_constraint_rule_update_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let id = resolved_field_string_arg(field, "id").unwrap_or_default();
        let delivery_method_types = fulfillment_constraint_rule_delivery_method_types(field);
        if let Some(payload) =
            fulfillment_constraint_rule_delivery_method_error(&delivery_method_types)
        {
            return payload;
        }
        let Some(mut rule) = self
            .store
            .staged
            .function_fulfillment_constraint_rules
            .get(&id)
            .cloned()
        else {
            return fulfillment_constraint_rule_payload_error(function_user_error(
                vec![json!("id")],
                &format!("Could not find FulfillmentConstraintRule with id: {id}"),
                Some("NOT_FOUND"),
            ));
        };
        rule["deliveryMethodTypes"] = json!(delivery_method_types);
        self.stage_function_fulfillment_constraint_rule(rule.clone());
        json!({ "fulfillmentConstraintRule": rule, "userErrors": [] })
    }

    pub(in crate::proxy) fn function_fulfillment_constraint_rule_delete_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let id = resolved_field_string_arg(field, "id").unwrap_or_default();
        if self
            .store
            .staged
            .function_fulfillment_constraint_rules
            .remove(&id)
            .is_some()
        {
            self.store
                .staged
                .function_fulfillment_constraint_rule_order
                .retain(|ordered_id| ordered_id != &id);
            json!({ "success": true, "userErrors": [] })
        } else {
            json!({
                "success": false,
                "userErrors": [{
                    "field": ["id"],
                    "message": format!("Could not find FulfillmentConstraintRule with id: {id}"),
                    "code": "NOT_FOUND"
                }]
            })
        }
    }

    pub(in crate::proxy) fn function_tax_app_configure_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let ready = resolved_bool_field(&field.arguments, "ready").unwrap_or(true);
        json!({
            "taxAppConfiguration": {
                "id": "gid://shopify/TaxAppConfiguration/local",
                "ready": ready,
                "state": if ready { "READY" } else { "NOT_READY" },
                "updatedAt": "2024-01-01T00:00:03.000Z"
            },
            "userErrors": []
        })
    }

    fn stage_function_validation(&mut self, validation: Value) {
        let Some(id) = validation["id"].as_str().map(str::to_string) else {
            return;
        };
        if !self.store.staged.function_validations.contains_key(&id) {
            self.store.staged.function_validation_order.push(id.clone());
        }
        self.store
            .staged
            .function_validations
            .insert(id, validation.clone());
        self.store.staged.function_validation = Some(validation);
    }

    fn stage_function_cart_transform(&mut self, cart_transform: Value) {
        let Some(id) = cart_transform["id"].as_str().map(str::to_string) else {
            return;
        };
        if !self.store.staged.function_cart_transforms.contains_key(&id) {
            self.store
                .staged
                .function_cart_transform_order
                .push(id.clone());
        }
        self.store
            .staged
            .function_cart_transforms
            .insert(id, cart_transform.clone());
        self.store.staged.function_cart_transform = Some(cart_transform);
    }

    fn stage_function_fulfillment_constraint_rule(&mut self, rule: Value) {
        let Some(id) = rule["id"].as_str().map(str::to_string) else {
            return;
        };
        if !self
            .store
            .staged
            .function_fulfillment_constraint_rules
            .contains_key(&id)
        {
            self.store
                .staged
                .function_fulfillment_constraint_rule_order
                .push(id.clone());
        }
        self.store
            .staged
            .function_fulfillment_constraint_rules
            .insert(id, rule);
    }
}

pub(in crate::proxy) fn local_validation_function() -> Value {
    json!({
        "id": "gid://shopify/ShopifyFunction/validation-local",
        "title": "Validation Local",
        "handle": "validation-local",
        "apiType": "VALIDATION"
    })
}

pub(in crate::proxy) fn local_cart_transform_function() -> Value {
    json!({
        "id": "gid://shopify/ShopifyFunction/cart-transform-local",
        "title": "Cart Transform Local",
        "handle": "cart-transform-local",
        "apiType": "CART_TRANSFORM"
    })
}

pub(in crate::proxy) fn local_fulfillment_constraint_rule_function() -> Value {
    json!({
        "id": "gid://shopify/ShopifyFunction/fulfillment-constraint-local",
        "title": "Fulfillment Constraint Local",
        "handle": "fulfillment-constraint-local",
        "apiType": "FULFILLMENT_CONSTRAINT_RULE"
    })
}

/// Output fields defined on the `CartTransform` type (2026-04). A selection of
/// anything else is a query-validation error (`undefinedField`) Shopify rejects
/// before execution — so the read returns errors with no data.
const CART_TRANSFORM_OUTPUT_FIELDS: &[&str] = &[
    "id",
    "functionId",
    "blockOnFailure",
    "metafield",
    "metafields",
    "__typename",
];

pub(in crate::proxy) fn cart_transform_selection_errors(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
    fields: &[RootFieldSelection],
) -> Vec<Value> {
    let operation_path = parsed_document(query, variables)
        .map(|document| document.operation_path)
        .unwrap_or_default();
    let mut errors = Vec::new();
    for field in fields {
        if field.name != "cartTransforms" {
            continue;
        }
        for child in &field.selection {
            // cartTransforms(first: N) { nodes { <CartTransform> } }
            //                          { edges { node { <CartTransform> } } }
            let (container_path, selections): (Vec<&str>, Vec<&SelectedField>) =
                match child.name.as_str() {
                    "nodes" => (vec!["nodes"], child.selection.iter().collect()),
                    "edges" => (
                        vec!["edges", "node"],
                        child
                            .selection
                            .iter()
                            .filter(|edge_child| edge_child.name == "node")
                            .flat_map(|node| node.selection.iter())
                            .collect(),
                    ),
                    _ => continue,
                };
            for selection in selections {
                if !CART_TRANSFORM_OUTPUT_FIELDS.contains(&selection.name.as_str()) {
                    errors.push(cart_transform_undefined_field_error(
                        query,
                        &operation_path,
                        &field.response_key,
                        &container_path,
                        &selection.name,
                    ));
                }
            }
        }
    }
    errors
}

fn cart_transform_undefined_field_error(
    query: &str,
    operation_path: &str,
    response_key: &str,
    container_path: &[&str],
    field_name: &str,
) -> Value {
    let location = cart_transform_field_token_location(query, field_name)
        .unwrap_or(SourceLocation { line: 1, column: 1 });
    let mut path = vec![Value::from(operation_path), Value::from(response_key)];
    path.extend(container_path.iter().map(|segment| Value::from(*segment)));
    path.push(Value::from(field_name));
    json!({
        "message": format!("Field '{field_name}' doesn't exist on type 'CartTransform'"),
        "locations": [{ "line": location.line, "column": location.column }],
        "path": path,
        "extensions": {
            "code": "undefinedField",
            "typeName": "CartTransform",
            "fieldName": field_name
        }
    })
}

fn cart_transform_field_token_location(query: &str, field_name: &str) -> Option<SourceLocation> {
    let bytes = query.as_bytes();
    let mut from = 0;
    while let Some(relative) = query[from..].find(field_name) {
        let index = from + relative;
        let after = index + field_name.len();
        let before_ok = index == 0 || !is_cart_transform_name_byte(bytes[index - 1]);
        let after_ok = after >= bytes.len() || !is_cart_transform_name_byte(bytes[after]);
        if before_ok && after_ok {
            let line = query[..index].bytes().filter(|byte| *byte == b'\n').count() + 1;
            let line_start = query[..index].rfind('\n').map_or(0, |newline| newline + 1);
            return Some(SourceLocation {
                line,
                column: index - line_start + 1,
            });
        }
        from = after;
    }
    None
}

fn is_cart_transform_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

pub(in crate::proxy) fn resolved_enum_arg(
    field: &RootFieldSelection,
    name: &str,
) -> Option<String> {
    match field.arguments.get(name) {
        Some(ResolvedValue::String(value)) => Some(value.clone()),
        _ => None,
    }
}

pub(in crate::proxy) fn functions_owner_validation_function() -> Value {
    json!({
        "id": "gid://shopify/ShopifyFunction/validation-owned",
        "title": "Owned validation function",
        "handle": "validation-owned",
        "apiType": "VALIDATION",
        "description": "Function metadata captured from the installed app",
        "appKey": "validation-app-key",
        "app": {
            "__typename": "App",
            "id": "gid://shopify/App/validation-app",
            "title": "Validation App",
            "handle": "validation-app",
            "apiKey": "validation-app-key"
        }
    })
}

pub(in crate::proxy) fn functions_owner_cart_function() -> Value {
    json!({
        "id": "gid://shopify/ShopifyFunction/cart-owned",
        "title": "Owned cart function",
        "handle": "cart-owned",
        "apiType": "CART_TRANSFORM",
        "description": "Cart transform Function metadata captured from the installed app",
        "appKey": "cart-app-key",
        "app": {
            "__typename": "App",
            "id": "gid://shopify/App/cart-app",
            "title": "Cart App",
            "handle": "cart-app",
            "apiKey": "cart-app-key"
        }
    })
}

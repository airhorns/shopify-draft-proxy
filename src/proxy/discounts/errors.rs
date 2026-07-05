use super::*;

pub(super) fn discount_bxgy_variable_error(
    input: &BTreeMap<String, ResolvedValue>,
    is_code: bool,
    is_create: bool,
    graphql_type: &str,
) -> Option<Value> {
    let column = match (is_code, is_create) {
        (true, true) => 50,
        (true, false) => 60,
        (false, true) => 55,
        (false, false) => 65,
    };

    if let Some(value) = input.get("usesPerOrderLimit") {
        match (is_code, value) {
            (true, ResolvedValue::String(raw)) => {
                return Some(discount_bxgy_invalid_variable(
                    graphql_type,
                    "usesPerOrderLimit",
                    vec!["usesPerOrderLimit"],
                    format!("Could not coerce value \"{raw}\" to Int"),
                    false,
                    column,
                ));
            }
            (false, ResolvedValue::String(raw)) => match raw.parse::<i64>() {
                Ok(n) if n >= 0 => {}
                Ok(n) => {
                    return Some(discount_bxgy_invalid_variable(
                        graphql_type,
                        "usesPerOrderLimit",
                        vec!["usesPerOrderLimit"],
                        format!("UnsignedInt64 '{n}' is out of range"),
                        true,
                        column,
                    ));
                }
                Err(_) => {
                    return Some(discount_bxgy_invalid_variable(
                        graphql_type,
                        "usesPerOrderLimit",
                        vec!["usesPerOrderLimit"],
                        format!("UnsignedInt64 invalid value '{raw}'"),
                        true,
                        column,
                    ));
                }
            },
            (false, ResolvedValue::Int(n)) if *n < 0 => {
                return Some(discount_bxgy_invalid_variable(
                    graphql_type,
                    "usesPerOrderLimit",
                    vec!["usesPerOrderLimit"],
                    format!("UnsignedInt64 '{n}' is out of range"),
                    true,
                    column,
                ));
            }
            _ => {}
        }
    }

    for (path, label) in [
        (
            vec!["customerBuys", "value", "quantity"],
            "customerBuys.value.quantity",
        ),
        (
            vec!["customerGets", "value", "discountOnQuantity", "quantity"],
            "customerGets.value.discountOnQuantity.quantity",
        ),
    ] {
        if let Some(value) =
            resolved_object_path(Some(&ResolvedValue::Object(input.clone())), &path)
        {
            match value {
                ResolvedValue::String(raw) if raw.contains('.') => {
                    return Some(discount_bxgy_invalid_variable(
                        graphql_type,
                        label,
                        path,
                        format!("UnsignedInt64 invalid value '{raw}'"),
                        true,
                        column,
                    ));
                }
                ResolvedValue::String(raw) if raw.starts_with('-') => {
                    return Some(discount_bxgy_invalid_variable(
                        graphql_type,
                        label,
                        path,
                        format!("UnsignedInt64 '{raw}' is out of range"),
                        true,
                        column,
                    ));
                }
                _ => {}
            }
        }
    }
    None
}

pub(super) fn discount_bxgy_invalid_variable(
    graphql_type: &str,
    label: &str,
    path: Vec<&str>,
    explanation: String,
    include_problem_message: bool,
    column: i64,
) -> Value {
    let mut problem = serde_json::Map::new();
    problem.insert("path".to_string(), json!(path));
    problem.insert("explanation".to_string(), json!(explanation));
    if include_problem_message {
        problem.insert("message".to_string(), problem["explanation"].clone());
    }
    json!({
        "message": format!("Variable $input of type {graphql_type}! was provided invalid value for {label} ({})", problem["explanation"].as_str().unwrap_or_default()),
        "locations": [{ "line": 1, "column": column }],
        "extensions": {
            "code": "INVALID_VARIABLE",
            "problems": [Value::Object(problem)]
        }
    })
}

pub(super) fn discount_bxgy_user_error(
    input: &BTreeMap<String, ResolvedValue>,
    prefix: &str,
) -> Option<Value> {
    if let Some(value) = input.get("usesPerOrderLimit") {
        if let Some(n) = resolved_i64(value) {
            if n == 0 {
                return Some(discount_user_error(
                    vec![prefix, "usesPerOrderLimit"],
                    "Allocation limit cannot be zero",
                    "VALUE_OUTSIDE_RANGE",
                ));
            }
            if n < 0 {
                return Some(discount_user_error(
                    vec![prefix, "usesPerOrderLimit"],
                    "Allocation limit must be greater than 0",
                    "GREATER_THAN",
                ));
            }
            if n > 2_147_483_647 {
                return Some(discount_user_error(
                    vec![prefix, "usesPerOrderLimit"],
                    "Allocation limit must be less than or equal to 2147483647",
                    "LESS_THAN_OR_EQUAL_TO",
                ));
            }
        }
    }

    if let Some(n) = resolved_i64_path(input, &["customerBuys", "value", "quantity"]) {
        if n == 0 {
            return Some(discount_user_error(
                vec![prefix, "customerBuys", "value", "quantity"],
                "Prerequisite to entitlement quantity ratio antecedent must be greater than 0",
                "GREATER_THAN",
            ));
        }
        if n >= 100_000 {
            return Some(discount_user_error(
                vec![prefix, "customerBuys", "value", "quantity"],
                "Prerequisite to entitlement quantity ratio antecedent must be less than 100000",
                "LESS_THAN",
            ));
        }
    }

    if let Some(n) = resolved_i64_path(
        input,
        &["customerGets", "value", "discountOnQuantity", "quantity"],
    ) {
        if n == 0 {
            return Some(discount_user_error(
                vec![
                    prefix,
                    "customerGets",
                    "value",
                    "discountOnQuantity",
                    "quantity",
                ],
                "Prerequisite to entitlement quantity ratio consequent must be greater than 0",
                "GREATER_THAN",
            ));
        }
        if n >= 100_000 {
            return Some(discount_user_error(
                vec![
                    prefix,
                    "customerGets",
                    "value",
                    "discountOnQuantity",
                    "quantity",
                ],
                "Prerequisite to entitlement quantity ratio consequent must be less than 100000",
                "LESS_THAN",
            ));
        }
    }
    None
}

pub(super) fn discount_user_error(field: Vec<&str>, message: &str, code: &str) -> Value {
    user_error_with_extra_info(field, message, Some(code), Value::Null)
}

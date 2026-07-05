use super::*;

#[derive(Debug, Clone)]
pub(in crate::proxy) struct UserErrorField(Value);

impl UserErrorField {
    fn into_value(self) -> Value {
        self.0
    }
}

impl From<Value> for UserErrorField {
    fn from(field: Value) -> Self {
        Self(field)
    }
}

impl From<Vec<Value>> for UserErrorField {
    fn from(field: Vec<Value>) -> Self {
        Self(Value::Array(field))
    }
}

impl From<Vec<String>> for UserErrorField {
    fn from(field: Vec<String>) -> Self {
        Self(Value::Array(field.into_iter().map(Value::from).collect()))
    }
}

impl<'a> From<Vec<&'a str>> for UserErrorField {
    fn from(field: Vec<&'a str>) -> Self {
        Self(Value::Array(field.into_iter().map(Value::from).collect()))
    }
}

impl<'a, 'b> From<&'a [&'b str]> for UserErrorField {
    fn from(field: &'a [&'b str]) -> Self {
        Self(Value::Array(
            field.iter().copied().map(Value::from).collect(),
        ))
    }
}

impl<'a, 'b, const N: usize> From<&'a [&'b str; N]> for UserErrorField {
    fn from(field: &'a [&'b str; N]) -> Self {
        Self(Value::Array(
            field.iter().copied().map(Value::from).collect(),
        ))
    }
}

impl<'a, const N: usize> From<[&'a str; N]> for UserErrorField {
    fn from(field: [&'a str; N]) -> Self {
        Self(Value::Array(field.into_iter().map(Value::from).collect()))
    }
}

pub(in crate::proxy) fn user_error_field(field: impl Into<UserErrorField>) -> Value {
    field.into().into_value()
}

fn user_error_code(code: Option<&str>) -> Value {
    code.map(Value::from).unwrap_or(Value::Null)
}

pub(in crate::proxy) const BLANK_USER_ERROR_CODE: &str = "BLANK";
pub(in crate::proxy) const TOO_LONG_USER_ERROR_CODE: &str = "TOO_LONG";

pub(in crate::proxy) fn blank_message(field_name: &str) -> String {
    format!("{field_name} can't be blank")
}

pub(in crate::proxy) fn too_long_message(field_name: &str, maximum: usize) -> String {
    format!("{field_name} is too long (maximum is {maximum} characters)")
}

pub(in crate::proxy) fn user_error(
    field: impl Into<UserErrorField>,
    message: &str,
    code: Option<&str>,
) -> Value {
    user_error_with_code_value(field, message, user_error_code(code))
}

#[derive(Debug, Clone, Copy)]
pub(in crate::proxy) enum LengthUserErrorBound {
    TooLong { maximum: usize },
}

pub(in crate::proxy) fn presence_user_error(
    field: impl Into<UserErrorField>,
    field_name: &str,
) -> Value {
    user_error(
        field,
        &blank_message(field_name),
        Some(BLANK_USER_ERROR_CODE),
    )
}

pub(in crate::proxy) fn length_user_error(
    field: impl Into<UserErrorField>,
    field_name: &str,
    bound: LengthUserErrorBound,
) -> Value {
    let (message, code) = match bound {
        LengthUserErrorBound::TooLong { maximum } => (
            too_long_message(field_name, maximum),
            TOO_LONG_USER_ERROR_CODE,
        ),
    };
    user_error(field, &message, Some(code))
}

pub(in crate::proxy) fn max_input_size_exceeded_error(
    path: impl Into<UserErrorField>,
    size: usize,
    maximum: usize,
    locations: Option<Value>,
) -> Value {
    let mut error = json!({
        "message": format!(
            "The input array size of {size} is greater than the maximum allowed of {maximum}."
        ),
        "path": user_error_field(path),
        "extensions": {
            "code": "MAX_INPUT_SIZE_EXCEEDED",
        },
    });
    if let Some(locations) = locations {
        error["locations"] = locations;
    }
    error
}

pub(in crate::proxy) fn payload_error(root_key: &str, user_errors: Vec<Value>) -> Value {
    json!({
        root_key: Value::Null,
        "userErrors": user_errors,
    })
}

pub(in crate::proxy) fn user_error_with_code_value(
    field: impl Into<UserErrorField>,
    message: &str,
    code: Value,
) -> Value {
    json!({
        "field": user_error_field(field),
        "message": message,
        "code": code,
    })
}

pub(in crate::proxy) fn user_error_omit_code(
    field: impl Into<UserErrorField>,
    message: &str,
    code: Option<&str>,
) -> Value {
    let mut error = json!({
        "field": user_error_field(field),
        "message": message,
    });
    if let Some(code) = code {
        error["code"] = json!(code);
    }
    error
}

pub(in crate::proxy) fn user_error_typed(
    typename: &str,
    field: impl Into<UserErrorField>,
    message: &str,
    code: Option<&str>,
) -> Value {
    let mut error = user_error(field, message, code);
    error["__typename"] = json!(typename);
    error
}

pub(in crate::proxy) fn user_error_typed_with_code_value(
    typename: &str,
    field: impl Into<UserErrorField>,
    message: &str,
    code: Value,
) -> Value {
    let mut error = user_error_with_code_value(field, message, code);
    error["__typename"] = json!(typename);
    error
}

pub(in crate::proxy) fn user_error_typed_omit_code(
    typename: &str,
    field: impl Into<UserErrorField>,
    message: &str,
    code: Option<&str>,
) -> Value {
    let mut error = user_error_omit_code(field, message, code);
    error["__typename"] = json!(typename);
    error
}

pub(in crate::proxy) fn user_error_with_extra_info(
    field: impl Into<UserErrorField>,
    message: &str,
    code: Option<&str>,
    extra_info: Value,
) -> Value {
    let mut error = user_error(field, message, code);
    error["extraInfo"] = extra_info;
    error
}

pub(in crate::proxy) fn user_error_with_element_index(
    field: impl Into<UserErrorField>,
    message: &str,
    code: Option<&str>,
    element_index: Value,
) -> Value {
    let mut error = user_error(field, message, code);
    error["elementIndex"] = element_index;
    error
}

pub(in crate::proxy) fn metaobject_indexed_user_error(
    field: impl Into<UserErrorField>,
    message: &str,
    code: Option<&str>,
    element_key: Value,
    element_index: Value,
) -> Value {
    let mut error = user_error(field, message, code);
    error["elementKey"] = element_key;
    error["elementIndex"] = element_index;
    error
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_same_json_bytes(actual: Value, expected: Value) {
        assert_eq!(actual.to_string(), expected.to_string());
    }

    #[test]
    fn user_error_field_coerces_supported_shapes() {
        assert_eq!(
            user_error_field(["input", "title"]),
            json!(["input", "title"])
        );
        assert_eq!(
            user_error_field(&["input", "title"][..]),
            json!(["input", "title"])
        );
        assert_eq!(
            user_error_field(vec!["input", "title"]),
            json!(["input", "title"])
        );
        assert_eq!(
            user_error_field(vec!["input".to_string(), "title".to_string()]),
            json!(["input", "title"])
        );
        assert_eq!(
            user_error_field(vec![json!("input"), json!(0), json!("title")]),
            json!(["input", 0, "title"])
        );
        assert_eq!(
            user_error_field(json!(["input", 0, "title"])),
            json!(["input", 0, "title"])
        );
    }

    #[test]
    fn user_error_matches_string_code_and_nullable_code_helpers() {
        assert_same_json_bytes(
            user_error(["input", "name"], "Name can't be blank", Some("BLANK")),
            json!({
                "field": ["input", "name"],
                "message": "Name can't be blank",
                "code": "BLANK",
            }),
        );

        let mut expected_nullable = serde_json::Map::new();
        expected_nullable.insert("field".to_string(), json!(["fulfillmentOrderId"]));
        expected_nullable.insert(
            "message".to_string(),
            json!("Fulfillment order does not exist"),
        );
        expected_nullable.insert("code".to_string(), Value::Null);
        assert_same_json_bytes(
            user_error(
                json!(["fulfillmentOrderId"]),
                "Fulfillment order does not exist",
                None,
            ),
            Value::Object(expected_nullable),
        );
    }

    #[test]
    fn blank_and_too_long_message_helpers_match_user_error_shapes() {
        assert_eq!(blank_message("Title"), "Title can't be blank");
        assert_same_json_bytes(
            presence_user_error(["input", "title"], "Title"),
            json!({
                "field": ["input", "title"],
                "message": "Title can't be blank",
                "code": "BLANK",
            }),
        );

        assert_eq!(
            too_long_message("Title", 255),
            "Title is too long (maximum is 255 characters)"
        );
        assert_same_json_bytes(
            length_user_error(
                ["input", "title"],
                "Title",
                LengthUserErrorBound::TooLong { maximum: 255 },
            ),
            json!({
                "field": ["input", "title"],
                "message": "Title is too long (maximum is 255 characters)",
                "code": "TOO_LONG",
            }),
        );
    }

    #[test]
    fn max_input_size_exceeded_error_matches_graphql_error_shape() {
        assert_same_json_bytes(
            max_input_size_exceeded_error(
                ["productVariantsBulkCreate", "variants"],
                2049,
                2048,
                Some(json!([{
                    "line": 7,
                    "column": 11,
                }])),
            ),
            json!({
                "message": "The input array size of 2049 is greater than the maximum allowed of 2048.",
                "locations": [{
                    "line": 7,
                    "column": 11,
                }],
                "path": ["productVariantsBulkCreate", "variants"],
                "extensions": {
                    "code": "MAX_INPUT_SIZE_EXCEEDED",
                },
            }),
        );
        assert_same_json_bytes(
            max_input_size_exceeded_error(["media"], 251, 250, None),
            json!({
                "message": "The input array size of 251 is greater than the maximum allowed of 250.",
                "path": ["media"],
                "extensions": {
                    "code": "MAX_INPUT_SIZE_EXCEEDED",
                },
            }),
        );
    }

    #[test]
    fn payload_error_matches_null_root_user_errors_shape() {
        assert_same_json_bytes(
            payload_error(
                "catalog",
                vec![user_error_typed(
                    "CatalogUserError",
                    ["input", "title"],
                    "Title can't be blank",
                    Some("BLANK"),
                )],
            ),
            json!({
                "catalog": Value::Null,
                "userErrors": [{
                    "__typename": "CatalogUserError",
                    "field": ["input", "title"],
                    "message": "Title can't be blank",
                    "code": "BLANK",
                }],
            }),
        );
    }

    #[test]
    fn user_error_omit_code_matches_inventory_missing_code_shape() {
        assert_same_json_bytes(
            user_error_omit_code(vec!["input", "locationId"], "Location is invalid", None),
            json!({
                "field": ["input", "locationId"],
                "message": "Location is invalid",
            }),
        );
        assert_same_json_bytes(
            user_error_omit_code(
                vec!["input".to_string(), "inventoryItemId".to_string()],
                "Inventory item is invalid",
                Some("INVALID"),
            ),
            json!({
                "field": ["input", "inventoryItemId"],
                "message": "Inventory item is invalid",
                "code": "INVALID",
            }),
        );
    }

    #[test]
    fn user_error_typed_matches_typename_variants() {
        assert_same_json_bytes(
            user_error_typed(
                "MetafieldDefinitionUserError",
                json!(["definition", "name"]),
                "Name has already been taken",
                Some("TAKEN"),
            ),
            json!({
                "__typename": "MetafieldDefinitionUserError",
                "field": ["definition", "name"],
                "message": "Name has already been taken",
                "code": "TAKEN",
            }),
        );

        let mut expected_gift_card = serde_json::Map::new();
        expected_gift_card.insert("__typename".to_string(), json!("GiftCardCreateUserError"));
        expected_gift_card.insert("field".to_string(), json!(["input", "initialValue"]));
        expected_gift_card.insert("code".to_string(), Value::Null);
        expected_gift_card.insert("message".to_string(), json!("Initial value is invalid"));
        assert_same_json_bytes(
            user_error_typed(
                "GiftCardCreateUserError",
                vec!["input", "initialValue"],
                "Initial value is invalid",
                None,
            ),
            Value::Object(expected_gift_card),
        );
    }

    #[test]
    fn user_error_with_extra_info_matches_discount_shape() {
        assert_same_json_bytes(
            user_error_with_extra_info(
                vec![json!("basicCodeDiscount"), json!("startsAt")],
                "Starts at must be before ends at",
                Some("INVALID"),
                Value::Null,
            ),
            json!({
                "field": ["basicCodeDiscount", "startsAt"],
                "message": "Starts at must be before ends at",
                "code": "INVALID",
                "extraInfo": Value::Null,
            }),
        );
        assert_same_json_bytes(
            user_error_with_extra_info(
                vec![json!("automaticAppDiscount"), json!("functionId")],
                "Function does not exist",
                None,
                Value::Null,
            ),
            json!({
                "field": ["automaticAppDiscount", "functionId"],
                "message": "Function does not exist",
                "code": Value::Null,
                "extraInfo": Value::Null,
            }),
        );
    }

    #[test]
    fn metaobject_indexed_user_error_matches_element_key_and_index_shape() {
        assert_same_json_bytes(
            metaobject_indexed_user_error(
                vec!["metaobject", "fields"],
                "Field is invalid",
                Some("INVALID"),
                json!("seo.title"),
                json!(3),
            ),
            json!({
                "field": ["metaobject", "fields"],
                "message": "Field is invalid",
                "code": "INVALID",
                "elementKey": "seo.title",
                "elementIndex": 3,
            }),
        );
    }
}

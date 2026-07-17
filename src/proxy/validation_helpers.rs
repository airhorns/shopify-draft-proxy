use super::*;

use crate::graphql::variable_definition_info;

pub(in crate::proxy) struct ValidationContext<'a> {
    pub(in crate::proxy) query: &'a str,
    pub(in crate::proxy) operation_path: &'a str,
    pub(in crate::proxy) response_key: &'a str,
    pub(in crate::proxy) field_location: SourceLocation,
}

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
    match bound {
        LengthUserErrorBound::TooLong { maximum } => user_error(
            field,
            &too_long_message(field_name, maximum),
            Some(TOO_LONG_USER_ERROR_CODE),
        ),
    }
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

pub(in crate::proxy) fn payload_user_error(root_key: &str, user_error: Value) -> Value {
    payload_error(root_key, vec![user_error])
}

pub(in crate::proxy) fn missing_required_arguments_error(
    field_name: &str,
    arguments: &str,
    location: SourceLocation,
    path: Vec<Value>,
) -> Value {
    json!({
        "message": format!("Field '{field_name}' is missing required arguments: {arguments}"),
        "locations": graphql_locations(location),
        "path": path,
        "extensions": {
            "code": "missingRequiredArguments",
            "className": "Field",
            "name": field_name,
            "arguments": arguments
        }
    })
}

pub(in crate::proxy) fn required_argument_null_error(
    field_name: &str,
    argument_name: &str,
    expected_type: &str,
    location: SourceLocation,
    path: Vec<Value>,
) -> Value {
    json!({
        "message": format!(
            "Argument '{argument_name}' on Field '{field_name}' has an invalid value (null). Expected type '{expected_type}'."
        ),
        "locations": graphql_locations(location),
        "path": path,
        "extensions": {
            "code": "argumentLiteralsIncompatible",
            "typeName": "Field",
            "argumentName": argument_name
        }
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

#[derive(Debug, Clone, Copy)]
pub(in crate::proxy) struct VariableValidationContext<'a> {
    pub(in crate::proxy) variable_name: &'a str,
    pub(in crate::proxy) variable_type: &'a str,
    pub(in crate::proxy) location: SourceLocation,
}

pub(in crate::proxy) fn graphql_locations(location: SourceLocation) -> Value {
    json!([{ "line": location.line, "column": location.column }])
}

pub(in crate::proxy) fn top_level_access_denied_error_envelope(
    message: String,
    location: Option<SourceLocation>,
    path: Vec<Value>,
    required_access: Option<&str>,
) -> Value {
    let mut error = json!({
        "message": message,
        "extensions": {
            "code": "ACCESS_DENIED",
            "documentation": "https://shopify.dev/api/usage/access-scopes"
        },
        "path": path,
    });
    if let Some(location) = location {
        error["locations"] = graphql_locations(location);
    }
    if let Some(required_access) = required_access {
        error["extensions"]["requiredAccess"] = json!(required_access);
    }
    error
}

pub(in crate::proxy) fn argument_literals_incompatible_error_envelope(
    message: String,
    location: Option<SourceLocation>,
    path: Option<Value>,
    type_name: Option<&str>,
    argument_name: Option<&str>,
) -> Value {
    let mut error = json!({
        "message": message,
        "extensions": { "code": "argumentLiteralsIncompatible" }
    });
    if let Some(location) = location {
        error["locations"] = graphql_locations(location);
    }
    if let Some(path) = path {
        error["path"] = path;
    }
    if let Some(type_name) = type_name {
        error["extensions"]["typeName"] = json!(type_name);
    }
    if let Some(argument_name) = argument_name {
        error["extensions"]["argumentName"] = json!(argument_name);
    }
    error
}

pub(in crate::proxy) fn missing_required_input_object_attribute_error_envelope(
    input_object_type: &str,
    argument_name: &str,
    argument_type: &str,
    location: SourceLocation,
    path: Value,
) -> Value {
    json!({
        "message": format!(
            "Argument '{argument_name}' on InputObject '{input_object_type}' is required. Expected type {argument_type}"
        ),
        "locations": graphql_locations(location),
        "path": path,
        "extensions": {
            "code": "missingRequiredInputObjectAttribute",
            "argumentName": argument_name,
            "argumentType": argument_type,
            "inputObjectType": input_object_type
        }
    })
}

pub(in crate::proxy) fn invalid_variable_error_envelope(
    message: String,
    location: SourceLocation,
    value: Value,
    problems: Value,
) -> Value {
    json!({
        "message": message,
        "locations": graphql_locations(location),
        "extensions": {
            "code": "INVALID_VARIABLE",
            "value": value,
            "problems": problems,
        },
    })
}

fn variable_problem_path_display(path: &[Value]) -> Option<String> {
    if path.is_empty() {
        return None;
    }
    Some(
        path.iter()
            .map(|segment| {
                segment
                    .as_u64()
                    .map(|index| index.to_string())
                    .or_else(|| segment.as_str().map(str::to_string))
                    .unwrap_or_else(|| segment.to_string())
            })
            .collect::<Vec<_>>()
            .join("."),
    )
}

pub(in crate::proxy) fn input_object_argument_not_accepted_error(
    input_type_name: &str,
    argument_name: &str,
    path: &[Value],
    context: ValidationContext<'_>,
) -> Value {
    let target_depth = 1 + path.len() as i32;
    let location = inline_input_field_name_location(
        context.query,
        context.field_location,
        target_depth,
        argument_name,
    )
    .unwrap_or(context.field_location);
    json!({
        "message": format!("InputObject '{input_type_name}' doesn't accept argument '{argument_name}'"),
        "locations": graphql_locations(location),
        "path": input_error_path(context, path, argument_name),
        "extensions": {
            "code": "argumentNotAccepted",
            "name": input_type_name,
            "typeName": "InputObject",
            "argumentName": argument_name
        }
    })
}

pub(in crate::proxy) fn inline_input_field_name_location(
    query: &str,
    field_location: SourceLocation,
    target_depth: i32,
    name: &str,
) -> Option<SourceLocation> {
    let start = byte_offset_for_location(query, field_location)?;
    let bytes = query.as_bytes();
    let mut index = start;
    while index < bytes.len() {
        match bytes[index] {
            b'(' => break,
            b'{' => return None,
            _ => index += 1,
        }
    }
    if index >= bytes.len() {
        return None;
    }
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    while index < bytes.len() {
        let byte = bytes[index];
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            index += 1;
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'(' | b'{' | b'[' => depth += 1,
            b')' | b'}' | b']' => {
                depth -= 1;
                if depth == 0 {
                    return None;
                }
            }
            _ if depth == target_depth => {
                let before_ok = index == 0 || !graphql_name_byte(bytes[index - 1]);
                if before_ok && query[index..].starts_with(name) {
                    let after = index + name.len();
                    let after_ok = bytes
                        .get(after)
                        .is_none_or(|next| !graphql_name_byte(*next));
                    let followed_by_colon = query[after..].trim_start().starts_with(':');
                    if after_ok && followed_by_colon {
                        return source_location_for_byte_offset(query, index);
                    }
                }
            }
            _ => {}
        }
        index += 1;
    }
    None
}

pub(in crate::proxy) fn inline_input_field_value_location(
    query: &str,
    field_location: SourceLocation,
    target_depth: i32,
    name: &str,
) -> Option<SourceLocation> {
    let name_location =
        inline_input_field_name_location(query, field_location, target_depth, name)?;
    let name_offset = byte_offset_for_location(query, name_location)?;
    let after_name = name_offset + name.len();
    source_location_for_byte_offset(query, value_offset_after(query, after_name)?)
}

pub(in crate::proxy) fn inline_argument_value_location(
    query: &str,
    field: &RootFieldSelection,
    argument_name: &str,
) -> Option<SourceLocation> {
    let start = byte_offset_for_location(query, field.location)?;
    let haystack = &query[start..];
    let argument_start = find_argument_name_with_colon(haystack, argument_name)?;
    let after_name = start + argument_start + argument_name.len();
    source_location_for_byte_offset(query, value_offset_after(query, after_name)?)
}

pub(in crate::proxy) fn argument_name_location_after(
    query: &str,
    start: SourceLocation,
    argument_name: &str,
) -> Option<SourceLocation> {
    let start = byte_offset_for_location(query, start)?;
    let relative = find_argument_name_with_colon(&query[start..], argument_name)?;
    source_location_for_byte_offset(query, start + relative)
}

pub(in crate::proxy) fn argument_value_location_after(
    query: &str,
    start: SourceLocation,
    argument_name: &str,
) -> Option<SourceLocation> {
    let start = byte_offset_for_location(query, start)?;
    let relative = find_argument_name_with_colon(&query[start..], argument_name)?;
    let after_name = start + relative + argument_name.len();
    source_location_for_byte_offset(query, value_offset_after(query, after_name)?)
}

pub(in crate::proxy) fn inline_argument_list_item_object_location(
    query: &str,
    field: &RootFieldSelection,
    argument_name: &str,
    target_index: usize,
) -> Option<SourceLocation> {
    inline_argument_list_item_object_location_after(
        query,
        field.location,
        argument_name,
        target_index,
    )
}

pub(in crate::proxy) fn inline_argument_list_item_object_location_after(
    query: &str,
    field_location: SourceLocation,
    argument_name: &str,
    target_index: usize,
) -> Option<SourceLocation> {
    let start = byte_offset_for_location(query, field_location)?;
    let haystack = &query[start..];
    let argument_start = find_argument_name_with_colon(haystack, argument_name)?;
    let after_name = start + argument_start + argument_name.len();
    let after_colon = query[after_name..].find(':')? + after_name + 1;
    let value_offset = query[after_colon..]
        .char_indices()
        .find_map(|(offset, ch)| (!ch.is_whitespace()).then_some(after_colon + offset))?;
    if query.as_bytes().get(value_offset) != Some(&b'[') {
        return None;
    }

    let bytes = query.as_bytes();
    let mut index = value_offset;
    let mut list_depth = 0i32;
    let mut object_depth = 0i32;
    let mut item_index = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    while index < bytes.len() {
        let byte = bytes[index];
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            index += 1;
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'[' => list_depth += 1,
            b']' => {
                list_depth -= 1;
                if list_depth == 0 {
                    return None;
                }
            }
            b'{' if list_depth == 1 && object_depth == 0 => {
                if item_index == target_index {
                    return source_location_for_byte_offset(query, index);
                }
                item_index += 1;
                object_depth += 1;
            }
            b'{' => object_depth += 1,
            b'}' if object_depth > 0 => object_depth -= 1,
            _ => {}
        }
        index += 1;
    }
    None
}

fn value_offset_after(query: &str, after_name: usize) -> Option<usize> {
    let after_colon = query[after_name..].find(':')? + after_name + 1;
    query[after_colon..]
        .char_indices()
        .find_map(|(offset, ch)| (!ch.is_whitespace()).then_some(after_colon + offset))
}

fn find_argument_name_with_colon(haystack: &str, argument_name: &str) -> Option<usize> {
    let mut search_start = 0;
    while search_start < haystack.len() {
        let relative = haystack[search_start..].find(argument_name)?;
        let candidate = search_start + relative;
        let before_ok = haystack[..candidate]
            .chars()
            .next_back()
            .is_none_or(|ch| !graphql_name_char(ch));
        let after_name = candidate + argument_name.len();
        let followed_by_colon = haystack[after_name..]
            .chars()
            .find(|ch| !ch.is_whitespace())
            .is_some_and(|ch| ch == ':');
        if before_ok && followed_by_colon {
            return Some(candidate);
        }
        search_start = after_name;
    }
    None
}

pub(in crate::proxy) fn byte_offset_for_location(
    query: &str,
    location: SourceLocation,
) -> Option<usize> {
    let mut line = 1;
    let mut column = 1;
    for (offset, ch) in query.char_indices() {
        if line == location.line && column == location.column {
            return Some(offset);
        }
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    (line == location.line && column == location.column).then_some(query.len())
}

pub(in crate::proxy) fn source_location_for_byte_offset(
    query: &str,
    target_offset: usize,
) -> Option<SourceLocation> {
    if target_offset > query.len() || !query.is_char_boundary(target_offset) {
        return None;
    }
    let mut line = 1;
    let mut column = 1;
    for (offset, ch) in query.char_indices() {
        if offset == target_offset {
            return Some(SourceLocation { line, column });
        }
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    (target_offset == query.len()).then_some(SourceLocation { line, column })
}

/// Resolves the 1-based location of a variable definition (`$name`) in the query.
pub(in crate::proxy) fn graphql_variable_definition_location(
    query: &str,
    variable_name: &str,
) -> Option<(usize, usize)> {
    let location = variable_definition_info(query, variable_name)?.location;
    Some((location.line, location.column))
}

/// Resolves the declared GraphQL type of a variable definition (`$name: <TYPE>`).
pub(in crate::proxy) fn graphql_variable_definition_type(
    query: &str,
    variable_name: &str,
) -> Option<String> {
    Some(variable_definition_info(query, variable_name)?.type_display)
}

pub(in crate::proxy) fn invalid_variable_error(
    context: VariableValidationContext<'_>,
    value: &ResolvedValue,
    problems: Vec<Value>,
) -> Value {
    let problem_display = problems
        .iter()
        .filter_map(|problem| {
            let path = variable_problem_path_display(problem["path"].as_array()?)?;
            let explanation = problem["explanation"].as_str()?;
            Some(format!("{path} ({explanation})"))
        })
        .collect::<Vec<_>>()
        .join(", ");
    invalid_variable_error_envelope(
        format!(
            "Variable ${} of type {} was provided invalid value for {}",
            context.variable_name, context.variable_type, problem_display
        ),
        context.location,
        resolved_value_json(value),
        Value::Array(problems),
    )
}

pub(in crate::proxy) fn variable_problem_value_path(path: &[Value], explanation: &str) -> Value {
    json!({
        "path": path,
        "explanation": explanation
    })
}

pub(in crate::proxy) fn variable_problem_with_message_value_path(
    path: &[Value],
    explanation: &str,
) -> Value {
    json!({
        "path": path,
        "explanation": explanation,
        "message": explanation
    })
}

fn input_error_path(context: ValidationContext<'_>, path: &[Value], argument_name: &str) -> Value {
    let mut segments = vec![
        Value::String(context.operation_path.to_string()),
        Value::String(context.response_key.to_string()),
    ];
    segments.extend(path.iter().cloned());
    segments.push(Value::String(argument_name.to_string()));
    Value::Array(segments)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_same_json_bytes(actual: Value, expected: Value) {
        assert_eq!(actual.to_string(), expected.to_string());
    }

    #[test]
    fn nested_inline_input_value_location_points_at_the_object_value() {
        let query = "mutation { root(input: { nested: { amount: \"5.00\" } }) { id } }";
        assert_eq!(
            inline_input_field_value_location(
                query,
                SourceLocation {
                    line: 1,
                    column: 12
                },
                2,
                "nested",
            ),
            Some(SourceLocation {
                line: 1,
                column: 34
            })
        );
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

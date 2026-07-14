//! Shopify-specific GraphQL validation and error-envelope compatibility.
//!
//! The executable schema owns parsing, validation, and coercion. This module
//! only translates engine diagnostics into the captured Admin API wire
//! contract; domain resolvers and routing do not belong here.

use super::*;
use crate::admin_graphql::{self, AdminApiVersion};
use crate::graphql::{directive_invocations, ParsedDocument};
pub(super) fn shopify_engine_response(
    engine_response: async_graphql::Response,
    version: AdminApiVersion,
    document: Option<&ParsedDocument>,
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
    variable_input_orders: &BTreeMap<Vec<String>, Vec<String>>,
) -> Value {
    let mut body = serde_json::to_value(engine_response)
        .unwrap_or_else(|error| json!({ "errors": [{ "message": error.to_string() }] }));
    if let Some(errors) = body.get_mut("errors").and_then(Value::as_array_mut) {
        let explicit_error_roots = errors
            .iter()
            .filter(|error| {
                error.get("message").and_then(Value::as_str)
                    != Some("internal: non-null types require a return value")
            })
            .filter_map(|error| error.get("path")?.as_array()?.first().cloned())
            .collect::<Vec<_>>();
        errors.retain(|error| {
            if error.get("message").and_then(Value::as_str)
                != Some("internal: non-null types require a return value")
            {
                return true;
            }
            let root = error
                .get("path")
                .and_then(Value::as_array)
                .and_then(|path| path.first());
            !root.is_some_and(|root| explicit_error_roots.contains(root))
        });
    }
    normalize_engine_error_paths(&mut body);
    let validation_only = body
        .get("errors")
        .and_then(Value::as_array)
        .is_some_and(|errors| {
            !errors.is_empty() && errors.iter().all(|error| error.get("path").is_none())
        });
    if !validation_only {
        return body;
    }

    // Shopify omits `data` for parse/validation failures. async-graphql emits
    // `data: null`, so normalize only this pre-execution response envelope.
    if let Some(object) = body.as_object_mut() {
        object.remove("data");
    }
    if let Some(errors) = body.get_mut("errors").and_then(Value::as_array_mut) {
        for error in errors {
            let Some(message_text) = error
                .get("message")
                .and_then(Value::as_str)
                .map(str::to_string)
            else {
                continue;
            };
            match message_text.as_str() {
                "Operation name required in request." => {
                    error["message"] = json!("An operation name is required");
                }
                message_text
                    if message_text.starts_with("Unknown operation named \"")
                        && (message_text.ends_with('"') || message_text.ends_with("\".")) =>
                {
                    error["message"] = json!(format!(
                        "No operation named {}",
                        message_text
                            .trim_start_matches("Unknown operation named ")
                            .trim_end_matches('.')
                    ));
                }
                message_text if document.is_none() && message_text.contains("expected ") => {
                    let engine_location = error_location(error).unwrap_or((1, 1));
                    let (line, column) = shopify_parse_error_location(query, engine_location);
                    *error = json!({
                        "message": format!(
                            "syntax error, unexpected end of file at [{line}, {column}]"
                        ),
                        "locations": [{ "line": line, "column": column }],
                        "extensions": { "code": "PARSE_ERROR" }
                    });
                }
                message_text => {
                    if let Some(variable_error) = shopify_variable_input_error(
                        version,
                        message_text,
                        document,
                        variables,
                        variable_input_orders,
                        error,
                    ) {
                        *error = variable_error;
                        continue;
                    }
                    if let Some(input_error) =
                        shopify_input_literal_error(version, message_text, document, query, error)
                    {
                        *error = input_error;
                        continue;
                    }
                    if let Some(directive_error) = shopify_unknown_directive_argument_error(
                        message_text,
                        query,
                        variables,
                        error,
                    ) {
                        *error = directive_error;
                        continue;
                    }
                    if let Some(directive_error) = shopify_directive_literal_error(
                        message_text,
                        query,
                        variables,
                        error_location(error),
                    ) {
                        *error = directive_error;
                        continue;
                    }
                    if let Some(argument_error) =
                        shopify_unknown_field_argument_error(message_text, document, error)
                    {
                        *error = argument_error;
                        continue;
                    }
                    if let Some((directive_name, argument_name)) =
                        async_graphql_missing_directive_argument(message_text)
                    {
                        let path = document
                            .and_then(|document| {
                                error_location(error).and_then(|location| {
                                    response_path_for_location(document, location)
                                })
                            })
                            .unwrap_or_default();
                        let locations =
                            error.get("locations").cloned().unwrap_or_else(|| json!([]));
                        *error = json!({
                            "message": format!(
                                "Directive '{directive_name}' is missing required arguments: {argument_name}"
                            ),
                            "locations": locations,
                            "path": path,
                            "extensions": {
                                "code": "missingRequiredArguments",
                                "className": "Directive",
                                "name": directive_name,
                                "arguments": argument_name
                            }
                        });
                        continue;
                    }
                    if let Some((field_name, type_name)) =
                        async_graphql_missing_selection(message_text)
                    {
                        let path = document
                            .and_then(|document| {
                                error_location(error).and_then(|location| {
                                    response_path_for_location(document, location)
                                })
                            })
                            .unwrap_or_else(|| vec![json!(field_name)]);
                        let locations =
                            error.get("locations").cloned().unwrap_or_else(|| json!([]));
                        *error = json!({
                            "message": format!(
                                "Field must have selections (field '{field_name}' returns {type_name} but has no selections. Did you mean '{field_name} {{ ... }}'?)"
                            ),
                            "locations": locations,
                            "path": path,
                            "extensions": {
                                "code": "selectionMismatch",
                                "nodeName": format!("field '{field_name}'"),
                                "typeName": type_name
                            }
                        });
                        continue;
                    }
                    let Some((field_name, type_name)) = async_graphql_unknown_field(message_text)
                    else {
                        continue;
                    };
                    let path = document
                        .and_then(|document| {
                            error_location(error)
                                .and_then(|location| response_path_for_location(document, location))
                        })
                        .unwrap_or_else(|| vec![json!(field_name)]);
                    let locations = error.get("locations").cloned().unwrap_or_else(|| json!([]));
                    *error = json!({
                        "message": format!(
                            "Field '{field_name}' doesn't exist on type '{type_name}'"
                        ),
                        "locations": locations,
                        "path": path,
                        "extensions": {
                            "code": "undefinedField",
                            "typeName": type_name,
                            "fieldName": field_name
                        }
                    });
                }
            }
        }
    }

    if let (Some(document), Some(errors)) = (
        document,
        body.get_mut("errors").and_then(Value::as_array_mut),
    ) {
        *errors = errors
            .drain(..)
            .flat_map(|error| {
                expand_inline_unknown_input_errors(version, document, query, &error)
                    .or_else(|| expand_inline_missing_input_errors(version, document, &error))
                    .unwrap_or_else(|| vec![error])
            })
            .collect();
    }

    let Some(document) = document else {
        return body;
    };
    let Some(errors) = body.get("errors").and_then(Value::as_array) else {
        return body;
    };
    let mut grouped = Vec::<(String, Vec<String>)>::new();
    for error in errors {
        let Some((field_name, argument_name)) = error
            .get("message")
            .and_then(Value::as_str)
            .and_then(async_graphql_missing_field_argument)
        else {
            continue;
        };
        if let Some((_, arguments)) = grouped
            .iter_mut()
            .find(|(existing, _)| existing == &field_name)
        {
            arguments.push(argument_name);
        } else {
            grouped.push((field_name, vec![argument_name]));
        }
    }
    if grouped.is_empty()
        || grouped.iter().map(|(_, args)| args.len()).sum::<usize>() != errors.len()
    {
        return body;
    }

    let normalized = grouped
        .into_iter()
        .map(|(field_name, arguments)| {
            let arguments = arguments.join(", ");
            let root = document
                .root_fields
                .iter()
                .find(|field| field.name == field_name);
            let location = root
                .map(|field| field.location)
                .unwrap_or(document.location);
            let response_key = root
                .map(|field| field.response_key.as_str())
                .unwrap_or(field_name.as_str());
            missing_required_arguments_error(
                &field_name,
                &arguments,
                location,
                document_field_path(document, response_key),
            )
        })
        .collect::<Vec<_>>();
    body["errors"] = Value::Array(normalized);
    body
}

fn normalize_engine_error_paths(body: &mut Value) {
    let Some(errors) = body.get_mut("errors").and_then(Value::as_array_mut) else {
        return;
    };
    for error in errors {
        let Some(path) = error.get_mut("path").and_then(Value::as_array_mut) else {
            continue;
        };
        let duplicated_root = path.len() >= 3
            && path.first() == path.get(2)
            && path.get(1).and_then(Value::as_str).is_some_and(|segment| {
                segment == "query"
                    || segment == "mutation"
                    || segment == "subscription"
                    || segment.starts_with("query ")
                    || segment.starts_with("mutation ")
                    || segment.starts_with("subscription ")
            });
        if duplicated_root {
            path.remove(0);
        }
    }
}

pub(super) fn shopify_root_id_errors(
    version: AdminApiVersion,
    document: &ParsedDocument,
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Vec<Value> {
    let mut errors = Vec::new();
    let mut reported_variables = BTreeSet::new();
    for field in &document.root_fields {
        for (argument_name, raw_value) in &field.raw_arguments {
            let Some(metadata) = admin_graphql::input_field_at_path(
                version,
                document.operation_type,
                &field.name,
                &[argument_name],
            ) else {
                continue;
            };
            match raw_value {
                RawArgumentValue::Variable { name, .. }
                    if reported_variables.insert(name.clone()) =>
                {
                    let Some(value) = variables.get(name) else {
                        continue;
                    };
                    let problems = variable_global_id_problems(
                        version,
                        &metadata,
                        value,
                        &[],
                        metadata.named_type == "ID",
                    );
                    if problems.is_empty() {
                        continue;
                    }
                    let Some(definition) = document.variable_definitions.get(name) else {
                        continue;
                    };
                    let context = VariableValidationContext {
                        variable_name: name,
                        variable_type: &definition.type_display,
                        location: definition.location,
                    };
                    if problems.iter().any(|problem| {
                        problem
                            .get("path")
                            .and_then(Value::as_array)
                            .is_some_and(|path| !path.is_empty())
                    }) {
                        errors.push(invalid_variable_error(context, value, problems));
                    } else {
                        errors.push(invalid_variable_error_envelope(
                            format!(
                                "Variable ${name} of type {} was provided invalid value",
                                definition.type_display
                            ),
                            definition.location,
                            resolved_value_json(value),
                            Value::Array(problems),
                        ));
                    }
                }
                _ => collect_inline_global_id_errors(
                    version,
                    document,
                    query,
                    field,
                    raw_value,
                    &mut vec![argument_name.clone()],
                    &mut errors,
                ),
            }
        }
    }
    errors
}

fn collect_inline_global_id_errors(
    version: AdminApiVersion,
    document: &ParsedDocument,
    query: &str,
    field: &RootFieldSelection,
    value: &RawArgumentValue,
    path: &mut Vec<String>,
    errors: &mut Vec<Value>,
) {
    let path_refs = path.iter().map(String::as_str).collect::<Vec<_>>();
    let Some(metadata) = admin_graphql::input_field_at_path(
        version,
        document.operation_type,
        &field.name,
        &path_refs,
    ) else {
        return;
    };
    if metadata.named_type == "ID" {
        let strict_root_id = admin_graphql::input_field_at_path(
            version,
            document.operation_type,
            &field.name,
            &[path.first().map(String::as_str).unwrap_or_default()],
        )
        .is_some_and(|root| root.named_type == "ID");
        let invalid = match value {
            RawArgumentValue::String(value)
                if shopify_gid_resource_type(value).is_none()
                    && (strict_root_id || value.is_empty()) =>
            {
                Some(value.clone())
            }
            RawArgumentValue::Int(value) if strict_root_id => Some(value.to_string()),
            _ => None,
        };
        if let Some(invalid) = invalid {
            errors.push(invalid_global_id_literal_error(
                version, document, field, query, path, &invalid,
            ));
        }
        if let RawArgumentValue::List(values) = value {
            for (index, value) in values.iter().enumerate() {
                path.push(index.to_string());
                collect_inline_global_id_errors(
                    version, document, query, field, value, path, errors,
                );
                path.pop();
            }
        }
        return;
    }
    match value {
        RawArgumentValue::List(values) => {
            for (index, value) in values.iter().enumerate() {
                path.push(index.to_string());
                collect_inline_global_id_errors(
                    version, document, query, field, value, path, errors,
                );
                path.pop();
            }
        }
        RawArgumentValue::Object(fields) => {
            for (name, value) in fields {
                path.push(name.clone());
                collect_inline_global_id_errors(
                    version, document, query, field, value, path, errors,
                );
                path.pop();
            }
        }
        _ => {}
    }
}

fn variable_global_id_problems(
    version: AdminApiVersion,
    field: &admin_graphql::InputFieldMetadata,
    value: &ResolvedValue,
    path: &[Value],
    strict: bool,
) -> Vec<Value> {
    if field.list {
        let ResolvedValue::List(values) = value else {
            return Vec::new();
        };
        return values
            .iter()
            .enumerate()
            .flat_map(|(index, value)| {
                let mut item_path = path.to_vec();
                item_path.push(json!(index));
                variable_named_global_id_problems(
                    version,
                    &field.named_type,
                    value,
                    &item_path,
                    strict,
                )
            })
            .collect();
    }
    variable_named_global_id_problems(version, &field.named_type, value, path, strict)
}

fn variable_named_global_id_problems(
    version: AdminApiVersion,
    named_type: &str,
    value: &ResolvedValue,
    path: &[Value],
    strict: bool,
) -> Vec<Value> {
    if named_type == "ID" {
        let invalid_id = match value {
            ResolvedValue::String(value)
                if shopify_gid_resource_type(value).is_none() && (strict || value.is_empty()) =>
            {
                Some(value.clone())
            }
            ResolvedValue::Int(value) if strict => Some(value.to_string()),
            _ => None,
        };
        return invalid_id
            .map(|invalid_id| {
                let explanation = format!("Invalid global id '{invalid_id}'");
                vec![variable_problem_with_message_value_path(path, &explanation)]
            })
            .unwrap_or_default();
    }
    let Some(input_fields) = admin_graphql::input_object_fields(version, named_type) else {
        return Vec::new();
    };
    let ResolvedValue::Object(values) = value else {
        return Vec::new();
    };
    input_fields
        .into_iter()
        .filter_map(|field| values.get(&field.name).map(|value| (field, value)))
        .flat_map(|(field, value)| {
            let mut field_path = path.to_vec();
            field_path.push(json!(field.name));
            variable_global_id_problems(version, &field, value, &field_path, false)
        })
        .collect()
}

fn invalid_global_id_literal_error(
    version: AdminApiVersion,
    document: &ParsedDocument,
    field: &RootFieldSelection,
    query: &str,
    input_path: &[String],
    invalid_id: &str,
) -> Value {
    let mut path = document_field_path(document, &field.response_key);
    let root_is_id_list = input_path.first().is_some_and(|argument_name| {
        admin_graphql::input_field_at_path(
            version,
            document.operation_type,
            &field.name,
            &[argument_name],
        )
        .is_some_and(|metadata| metadata.named_type == "ID" && metadata.list)
    });
    let error_input_path = if root_is_id_list {
        &input_path[..1]
    } else {
        input_path
    };
    path.extend(input_path_values(
        error_input_path.iter().map(String::as_str),
    ));
    let location = input_path
        .iter()
        .enumerate()
        .rev()
        .find_map(|(position, segment)| {
            segment.parse::<usize>().ok().and_then(|index| {
                inline_argument_list_item_object_location(
                    query,
                    field,
                    input_path.first().map(String::as_str).unwrap_or_default(),
                    index,
                )
                .filter(|_| position + 1 < input_path.len())
            })
        })
        .unwrap_or(field.location);
    json!({
        "message": format!("Invalid global id '{invalid_id}'"),
        "locations": [{ "line": location.line, "column": location.column }],
        "path": path,
        "extensions": {
            "code": "argumentLiteralsIncompatible",
            "typeName": "CoercionError"
        }
    })
}

fn error_location(error: &Value) -> Option<(usize, usize)> {
    let location = error.get("locations")?.as_array()?.first()?;
    Some((
        location.get("line")?.as_u64()? as usize,
        location.get("column")?.as_u64()? as usize,
    ))
}

fn shopify_parse_error_location(query: &str, engine_location: (usize, usize)) -> (usize, usize) {
    let last_token_start = query
        .lines()
        .enumerate()
        .filter_map(|(line, source)| {
            source
                .chars()
                .position(|character| !character.is_whitespace())
                .map(|column| (line + 1, column + 1))
        })
        .last();

    match last_token_start {
        // async-graphql locates an unexpected EOF after a trailing newline on
        // the following empty line. Shopify reports the start of the final
        // token instead, which is also what its captured PARSE_ERROR message
        // embeds.
        Some(location) if engine_location.0 > location.0 => location,
        _ => engine_location,
    }
}

fn async_graphql_unknown_field(message: &str) -> Option<(String, String)> {
    let rest = message.strip_prefix("Unknown field \"")?;
    let (field_name, rest) = rest.split_once("\" on type \"")?;
    let (type_name, _) = rest.split_once('"')?;
    Some((field_name.to_string(), type_name.to_string()))
}

fn async_graphql_missing_selection(message: &str) -> Option<(String, String)> {
    let rest = message.strip_prefix("Field \"")?;
    let (field_name, rest) = rest.split_once("\" of type \"")?;
    let (type_name, rest) = rest.split_once('"')?;
    rest.contains("must have a selection of subfields")
        .then(|| (field_name.to_string(), type_name.to_string()))
}

fn async_graphql_missing_directive_argument(message: &str) -> Option<(String, String)> {
    let rest = message.strip_prefix("Directive \"@")?;
    let (directive_name, rest) = rest.split_once("\" argument \"")?;
    let (argument_name, rest) = rest.split_once('"')?;
    rest.contains("is required but not provided")
        .then(|| (directive_name.to_string(), argument_name.to_string()))
}

fn shopify_unknown_field_argument_error(
    message: &str,
    document: Option<&ParsedDocument>,
    engine_error: &Value,
) -> Option<Value> {
    let rest = message.strip_prefix("Unknown argument \"")?;
    let (argument_name, rest) = rest.split_once("\" on field \"")?;
    let (field_name, _) = rest.split_once('"')?;
    let document = document?;
    let field = document
        .root_fields
        .iter()
        .find(|field| field.name == field_name)?;
    let mut path = document_field_path(document, &field.response_key);
    path.push(json!(argument_name));
    Some(json!({
        "message": format!("Field '{field_name}' doesn't accept argument '{argument_name}'"),
        "locations": engine_error
            .get("locations")
            .cloned()
            .unwrap_or_else(|| json!([{ "line": field.location.line, "column": field.location.column }])),
        "path": path,
        "extensions": {
            "code": "argumentNotAccepted",
            "name": field_name,
            "typeName": "Field",
            "argumentName": argument_name
        }
    }))
}

fn shopify_directive_literal_error(
    message: &str,
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
    error_location: Option<(usize, usize)>,
) -> Option<Value> {
    let rest = message.strip_prefix("Invalid value for argument \"")?;
    let (argument_name, _) = rest.split_once('"')?;
    let invocations = directive_invocations(query, variables)?;
    let invocation = invocations
        .iter()
        .filter(|invocation| invocation.raw_arguments.contains_key(argument_name))
        .filter(|invocation| {
            error_location.is_none_or(|location| {
                (invocation.location.line, invocation.location.column) <= location
            })
        })
        .max_by_key(|invocation| (invocation.location.line, invocation.location.column))?;
    let expected_type = match (invocation.name.as_str(), argument_name) {
        ("include" | "skip", "if") => "Boolean!",
        _ => return None,
    };
    if let Some(RawArgumentValue::Variable { name, .. }) =
        invocation.raw_arguments.get(argument_name)
    {
        let value = supplied_variable(variables, name)?;
        if matches!(value, ResolvedValue::Bool(_)) {
            return None;
        }
        let definition = variable_definition_info(query, name)?;
        let json_value = resolved_value_json(value);
        return Some(json!({
            "message": format!(
                "Variable ${name} of type {} was provided invalid value",
                definition.type_display
            ),
            "locations": [{
                "line": definition.location.line,
                "column": definition.location.column
            }],
            "extensions": {
                "code": "INVALID_VARIABLE",
                "value": json_value,
                "problems": [{
                    "path": [],
                    "explanation": format!(
                        "Could not coerce value {} to Boolean",
                        resolved_value_json(value)
                    )
                }]
            }
        }));
    }
    if invocation.raw_arguments.get(argument_name) != Some(&RawArgumentValue::Null) {
        return None;
    }
    let mut path = invocation
        .path
        .iter()
        .map(|segment| json!(segment))
        .collect::<Vec<_>>();
    path.push(json!(argument_name));
    Some(json!({
        "message": format!(
            "Argument '{argument_name}' on Directive '{}' has an invalid value (null). Expected type '{expected_type}'.",
            invocation.name
        ),
        "locations": [{
            "line": invocation.location.line,
            "column": invocation.location.column
        }],
        "path": path,
        "extensions": {
            "code": "argumentLiteralsIncompatible",
            "typeName": "Directive",
            "argumentName": argument_name
        }
    }))
}

fn shopify_variable_input_error(
    version: AdminApiVersion,
    message: &str,
    document: Option<&ParsedDocument>,
    variables: &BTreeMap<String, ResolvedValue>,
    variable_input_orders: &BTreeMap<Vec<String>, Vec<String>>,
    engine_error: &Value,
) -> Option<Value> {
    let document = document?;
    let argument_path = async_graphql_input_argument_path(message)?;
    let path = argument_path.split('.').collect::<Vec<_>>();
    let field = input_error_root_field(document, &path, error_location(engine_error))?;
    let (variable_name, variable_field, variable_path) =
        input_error_variable(version, document, field, &path)?;
    let definition = document.variable_definitions.get(variable_name)?;
    let value = variables.get(variable_name)?;
    let mut problems = variable_value_problems(version, &variable_field, value, &variable_path);
    if problems.is_empty() {
        return None;
    }
    sort_variable_problems_by_input_order(&mut problems, variable_name, variable_input_orders);
    let context = VariableValidationContext {
        variable_name,
        variable_type: &definition.type_display,
        location: definition.location,
    };
    let has_nested_path = problems.iter().any(|problem| {
        problem
            .get("path")
            .and_then(Value::as_array)
            .is_some_and(|path| !path.is_empty())
    });
    Some(if has_nested_path {
        invalid_variable_error(context, value, problems)
    } else {
        invalid_variable_error_envelope(
            format!(
                "Variable ${variable_name} of type {} was provided invalid value",
                definition.type_display
            ),
            definition.location,
            resolved_value_json(value),
            Value::Array(problems),
        )
    })
}

fn sort_variable_problems_by_input_order(
    problems: &mut [Value],
    variable_name: &str,
    variable_input_orders: &BTreeMap<Vec<String>, Vec<String>>,
) {
    problems.sort_by_key(|problem| {
        let mut parent_path = vec![variable_name.to_string()];
        problem
            .get("path")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|segment| {
                let segment = segment
                    .as_str()
                    .map(str::to_string)
                    .or_else(|| segment.as_u64().map(|value| value.to_string()))
                    .unwrap_or_else(|| segment.to_string());
                let rank = segment.parse::<usize>().ok().unwrap_or_else(|| {
                    variable_input_orders
                        .get(&parent_path)
                        .and_then(|fields| fields.iter().position(|field| field == &segment))
                        .unwrap_or(usize::MAX)
                });
                parent_path.push(segment.clone());
                rank
            })
            .collect::<Vec<_>>()
    });
}

fn async_graphql_input_argument_path(message: &str) -> Option<&str> {
    message
        .strip_prefix("Invalid value for argument \"")?
        .split_once("\",")
        .map(|(path, _)| path)
}

fn input_error_root_field<'a>(
    document: &'a ParsedDocument,
    path: &[&str],
    location: Option<(usize, usize)>,
) -> Option<&'a RootFieldSelection> {
    let root_argument = path.first()?;
    let candidates = document
        .root_fields
        .iter()
        .filter(|field| field.raw_arguments.contains_key(*root_argument));
    if let Some(location) = location {
        candidates
            .filter(|field| (field.location.line, field.location.column) <= location)
            .max_by_key(|field| (field.location.line, field.location.column))
            .or_else(|| {
                document
                    .root_fields
                    .iter()
                    .find(|field| field.raw_arguments.contains_key(*root_argument))
            })
    } else {
        candidates.into_iter().next()
    }
}

fn input_error_variable<'a>(
    version: AdminApiVersion,
    document: &ParsedDocument,
    field: &'a RootFieldSelection,
    path: &[&str],
) -> Option<(&'a str, admin_graphql::InputFieldMetadata, Vec<Value>)> {
    let root_argument = path.first()?;
    let root_value = field.raw_arguments.get(*root_argument)?;
    let root_field = admin_graphql::input_field_at_path(
        version,
        document.operation_type,
        &field.name,
        &path[..1],
    )?;
    if let RawArgumentValue::Variable { name, .. } = root_value {
        return Some((name, root_field, Vec::new()));
    }

    let mut raw_value = root_value;
    for (index, segment) in path.iter().enumerate().skip(1) {
        raw_value = match raw_value {
            RawArgumentValue::Object(fields) => fields.get(*segment)?,
            RawArgumentValue::List(items) => items.get(segment.parse::<usize>().ok()?)?,
            RawArgumentValue::Variable { name, .. } => {
                let metadata = admin_graphql::input_field_at_path(
                    version,
                    document.operation_type,
                    &field.name,
                    &path[..index],
                )?;
                return Some((
                    name,
                    metadata,
                    input_path_values(path.iter().skip(index).copied()),
                ));
            }
            _ => return None,
        };
    }
    let RawArgumentValue::Variable { name, .. } = raw_value else {
        return None;
    };
    Some((
        name,
        admin_graphql::input_field_at_path(version, document.operation_type, &field.name, path)?,
        Vec::new(),
    ))
}

fn input_path_values<'a>(segments: impl Iterator<Item = &'a str>) -> Vec<Value> {
    segments
        .map(|segment| {
            segment
                .parse::<u64>()
                .map(Value::from)
                .unwrap_or_else(|_| Value::String(segment.to_string()))
        })
        .collect()
}

fn variable_value_problems(
    version: AdminApiVersion,
    field: &admin_graphql::InputFieldMetadata,
    value: &ResolvedValue,
    path: &[Value],
) -> Vec<Value> {
    if field.list {
        let ResolvedValue::List(values) = value else {
            return Vec::new();
        };
        return values
            .iter()
            .enumerate()
            .flat_map(|(index, value)| {
                let mut item_path = path.to_vec();
                item_path.push(json!(index));
                if matches!(value, ResolvedValue::Null) && field.list_item_required {
                    vec![variable_problem_value_path(
                        &item_path,
                        "Expected value to not be null",
                    )]
                } else {
                    variable_named_value_problems(version, &field.named_type, value, &item_path)
                }
            })
            .collect();
    }
    variable_named_value_problems(version, &field.named_type, value, path)
}

fn variable_named_value_problems(
    version: AdminApiVersion,
    named_type: &str,
    value: &ResolvedValue,
    path: &[Value],
) -> Vec<Value> {
    let scalar_problem = match (named_type, value) {
        ("Int", ResolvedValue::Int(_))
        | ("Float", ResolvedValue::Int(_) | ResolvedValue::Float(_))
        | ("String", ResolvedValue::String(_))
        | ("Boolean", ResolvedValue::Bool(_)) => None,
        ("Int", value) => Some(variable_problem_value_path(
            path,
            &format!(
                "Could not coerce value {} to Int",
                resolved_value_json(value)
            ),
        )),
        ("Float", value) => Some(variable_problem_value_path(
            path,
            &format!(
                "Could not coerce value {} to Float",
                resolved_value_json(value)
            ),
        )),
        ("String", value) => Some(variable_problem_value_path(
            path,
            &format!(
                "Could not coerce value {} to String",
                resolved_value_json(value)
            ),
        )),
        ("Boolean", value) => Some(variable_problem_value_path(
            path,
            &format!(
                "Could not coerce value {} to Boolean",
                resolved_value_json(value)
            ),
        )),
        ("Decimal", ResolvedValue::String(raw))
            if raw.parse::<f64>().is_err() || !raw.parse::<f64>().is_ok_and(f64::is_finite) =>
        {
            let explanation = format!("invalid decimal '{raw}'");
            Some(variable_problem_with_message_value_path(path, &explanation))
        }
        ("Decimal", ResolvedValue::Int(_) | ResolvedValue::Float(_)) => None,
        ("UnsignedInt64", ResolvedValue::String(raw)) if raw.parse::<u64>().is_err() => {
            let explanation = if raw.parse::<i128>().is_ok_and(|value| value < 0) {
                format!("UnsignedInt64 '{raw}' is out of range")
            } else {
                format!("UnsignedInt64 invalid value '{raw}'")
            };
            Some(variable_problem_with_message_value_path(path, &explanation))
        }
        ("UnsignedInt64", ResolvedValue::Int(value)) if *value < 0 => {
            let explanation = format!("UnsignedInt64 '{value}' is out of range");
            Some(variable_problem_with_message_value_path(path, &explanation))
        }
        ("URL", ResolvedValue::String(raw)) => admin_graphql::invalid_url_scalar_message(raw)
            .map(|explanation| variable_problem_with_message_value_path(path, &explanation)),
        _ => None,
    };
    if let Some(problem) = scalar_problem {
        return vec![problem];
    }
    if named_type == "ID" {
        let invalid_id = match value {
            ResolvedValue::String(value) if shopify_gid_resource_type(value).is_none() => {
                Some(value.clone())
            }
            ResolvedValue::Int(value) => Some(value.to_string()),
            _ => None,
        };
        if let Some(invalid_id) = invalid_id {
            let explanation = format!("Invalid global id '{invalid_id}'");
            return vec![variable_problem_with_message_value_path(path, &explanation)];
        }
    }
    if let Some(input_fields) = admin_graphql::input_object_fields(version, named_type) {
        let ResolvedValue::Object(values) = value else {
            return Vec::new();
        };
        let known_fields = input_fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<BTreeSet<_>>();
        let mut problems = values
            .keys()
            .filter(|name| !known_fields.contains(name.as_str()))
            .map(|name| {
                let mut field_path = path.to_vec();
                field_path.push(json!(name));
                variable_problem_value_path(
                    &field_path,
                    &format!("Field is not defined on {named_type}"),
                )
            })
            .collect::<Vec<_>>();
        for input_field in input_fields {
            let provided = values.get(&input_field.name);
            if input_field.required
                && provided.is_none_or(|value| matches!(value, ResolvedValue::Null))
            {
                let mut field_path = path.to_vec();
                field_path.push(json!(input_field.name));
                problems.push(variable_problem_value_path(
                    &field_path,
                    "Expected value to not be null",
                ));
                continue;
            }
            let Some(value) = provided else {
                continue;
            };
            let mut field_path = path.to_vec();
            field_path.push(json!(input_field.name));
            problems.extend(variable_value_problems(
                version,
                &input_field,
                value,
                &field_path,
            ));
        }
        return problems;
    }

    if let (Some(values), ResolvedValue::String(value)) =
        (admin_graphql::enum_values(version, named_type), value)
    {
        if !values.iter().any(|candidate| candidate == value) {
            return vec![variable_problem_value_path(
                path,
                &format!("Expected \"{value}\" to be one of: {}", values.join(", ")),
            )];
        }
    }
    Vec::new()
}

fn shopify_input_literal_error(
    version: AdminApiVersion,
    message: &str,
    document: Option<&ParsedDocument>,
    query: &str,
    engine_error: &Value,
) -> Option<Value> {
    let document = document?;
    let argument_path = async_graphql_input_argument_path(message)?;
    let path = argument_path.split('.').collect::<Vec<_>>();
    let field = input_error_root_field(document, &path, error_location(engine_error))?;
    if input_error_variable(version, document, field, &path).is_some() {
        return None;
    }
    let path_values = input_path_values(path.iter().copied());
    let locations =
        |location: SourceLocation| json!([{ "line": location.line, "column": location.column }]);

    let rest = message
        .strip_prefix("Invalid value for argument \"")?
        .split_once("\",")?
        .1
        .trim_start();
    if let Some(rest) = rest.strip_prefix("unknown field \"") {
        let (argument_name, rest) = rest.split_once("\" of type \"")?;
        let (input_type, _) = rest.split_once('"')?;
        let location = inline_input_field_name_location(
            query,
            field.location,
            1 + path.len() as i32,
            argument_name,
        )
        .unwrap_or(field.location);
        let mut error_path = document_field_path(document, &field.response_key);
        error_path.extend(path_values);
        error_path.push(json!(argument_name));
        return Some(json!({
            "message": format!("InputObject '{input_type}' doesn't accept argument '{argument_name}'"),
            "locations": locations(location),
            "path": error_path,
            "extensions": {
                "code": "argumentNotAccepted",
                "name": input_type,
                "typeName": "InputObject",
                "argumentName": argument_name
            }
        }));
    }
    if let Some(rest) = rest.strip_prefix("field \"") {
        let (argument_name, rest) = rest.split_once("\" of type \"")?;
        let (argument_type, rest) = rest.split_once('"')?;
        if !rest.contains("is required but not provided") {
            return None;
        }
        let input_type = admin_graphql::input_field_at_path(
            version,
            document.operation_type,
            &field.name,
            &path,
        )?
        .named_type;
        let mut error_path = document_field_path(document, &field.response_key);
        error_path.extend(path_values);
        error_path.push(json!(argument_name));
        let location = if path.len() == 1 {
            inline_argument_value_location(query, field, path[0])
        } else if let Some(index) = path
            .last()
            .and_then(|segment| segment.parse::<usize>().ok())
        {
            inline_argument_list_item_object_location(query, field, path[0], index)
        } else {
            inline_input_field_value_location(
                query,
                field.location,
                path.len() as i32,
                path.last().copied().unwrap_or(path[0]),
            )
            .or_else(|| inline_argument_value_location(query, field, path[0]))
        }
        .unwrap_or(field.location);
        return Some(json!({
            "message": format!(
                "Argument '{argument_name}' on InputObject '{input_type}' is required. Expected type {argument_type}"
            ),
            "locations": locations(location),
            "path": error_path,
            "extensions": {
                "code": "missingRequiredInputObjectAttribute",
                "argumentName": argument_name,
                "argumentType": argument_type,
                "inputObjectType": input_type
            }
        }));
    }
    if let Some(rest) = rest.strip_prefix("enumeration type \"") {
        let (_enum_type, rest) = rest.split_once("\" does not contain the value \"")?;
        let leaf_invalid_value = rest.strip_suffix('"')?;
        let semantic_path = path
            .iter()
            .copied()
            .rev()
            .skip_while(|segment| segment.parse::<usize>().is_ok())
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>();
        let invalid_value = if semantic_path.len() != path.len() {
            raw_input_value_at_path(field, &semantic_path)
                .map(raw_input_display)
                .unwrap_or_else(|| leaf_invalid_value.to_string())
        } else {
            leaf_invalid_value.to_string()
        };
        let input_field = admin_graphql::input_field_at_path(
            version,
            document.operation_type,
            &field.name,
            &semantic_path,
        )?;
        let (owner_kind, owner_name, location) = if semantic_path.len() == 1 {
            ("Field", field.name.clone(), field.location)
        } else {
            (
                "InputObject",
                admin_graphql::input_owner_at_path(
                    version,
                    document.operation_type,
                    &field.name,
                    &semantic_path,
                )?,
                inline_argument_value_location(query, field, path[0]).unwrap_or(field.location),
            )
        };
        let argument_name = semantic_path.last()?;
        let mut error_path = document_field_path(document, &field.response_key);
        error_path.extend(input_path_values(semantic_path.iter().copied()));
        return Some(json!({
            "message": format!(
                "Argument '{argument_name}' on {owner_kind} '{owner_name}' has an invalid value ({invalid_value}). Expected type '{}'.",
                input_field.type_display
            ),
            "locations": locations(location),
            "path": error_path,
            "extensions": {
                "code": "argumentLiteralsIncompatible",
                "typeName": owner_kind,
                "argumentName": argument_name
            }
        }));
    }
    if rest.starts_with("expected type \"")
        && matches!(
            raw_input_value_at_path(field, &path),
            Some(RawArgumentValue::Null)
        )
    {
        let input_field = admin_graphql::input_field_at_path(
            version,
            document.operation_type,
            &field.name,
            &path,
        )?;
        let (owner_kind, owner_name, location) = if path.len() == 1 {
            ("Field", field.name.clone(), field.location)
        } else {
            (
                "InputObject",
                admin_graphql::input_owner_at_path(
                    version,
                    document.operation_type,
                    &field.name,
                    &path,
                )?,
                inline_argument_value_location(query, field, path[0]).unwrap_or(field.location),
            )
        };
        let argument_name = path.last()?;
        let mut error_path = document_field_path(document, &field.response_key);
        error_path.extend(path_values);
        return Some(json!({
            "message": format!(
                "Argument '{argument_name}' on {owner_kind} '{owner_name}' has an invalid value (null). Expected type '{}'.",
                input_field.type_display
            ),
            "locations": locations(location),
            "path": error_path,
            "extensions": {
                "code": "argumentLiteralsIncompatible",
                "typeName": owner_kind,
                "argumentName": argument_name
            }
        }));
    }
    if rest == "expected type \"ID\"" {
        let raw_value = raw_input_value_at_path(field, &path)?;
        let invalid_id = match raw_value {
            RawArgumentValue::String(value) => value.clone(),
            RawArgumentValue::Int(value) => value.to_string(),
            _ => return None,
        };
        let mut error_path = document_field_path(document, &field.response_key);
        error_path.extend(path_values);
        return Some(json!({
            "message": format!("Invalid global id '{invalid_id}'"),
            "locations": locations(field.location),
            "path": error_path,
            "extensions": {
                "code": "argumentLiteralsIncompatible",
                "typeName": "CoercionError"
            }
        }));
    }
    if rest.starts_with("expected type \"") {
        let input_field = admin_graphql::input_field_at_path(
            version,
            document.operation_type,
            &field.name,
            &path,
        )?;
        if input_field.named_type == "URL" {
            let RawArgumentValue::String(raw_value) = raw_input_value_at_path(field, &path)? else {
                return None;
            };
            let message = admin_graphql::invalid_url_scalar_message(raw_value)?;
            let location = if path.len() == 1 {
                field.location
            } else {
                inline_argument_value_location(query, field, path[0]).unwrap_or(field.location)
            };
            let mut error_path = document_field_path(document, &field.response_key);
            error_path.extend(path_values);
            return Some(json!({
                "message": message,
                "locations": locations(location),
                "path": error_path,
                "extensions": {
                    "code": "argumentLiteralsIncompatible",
                    "typeName": "CoercionError"
                }
            }));
        }
    }
    if rest.starts_with("expected type \"") {
        let raw_value = raw_input_value_at_path(field, &path)?;
        let input_field = admin_graphql::input_field_at_path(
            version,
            document.operation_type,
            &field.name,
            &path,
        )?;
        let (owner_kind, owner_name, location) = if path.len() == 1 {
            ("Field", field.name.clone(), field.location)
        } else {
            (
                "InputObject",
                admin_graphql::input_owner_at_path(
                    version,
                    document.operation_type,
                    &field.name,
                    &path,
                )?,
                inline_argument_value_location(query, field, path[0]).unwrap_or(field.location),
            )
        };
        let argument_name = path.last()?;
        let mut error_path = document_field_path(document, &field.response_key);
        error_path.extend(path_values);
        return Some(json!({
            "message": format!(
                "Argument '{argument_name}' on {owner_kind} '{owner_name}' has an invalid value ({}). Expected type '{}'.",
                raw_input_display(raw_value),
                input_field.type_display
            ),
            "locations": locations(location),
            "path": error_path,
            "extensions": {
                "code": "argumentLiteralsIncompatible",
                "typeName": owner_kind,
                "argumentName": argument_name
            }
        }));
    }
    None
}

fn expand_inline_unknown_input_errors(
    version: AdminApiVersion,
    document: &ParsedDocument,
    query: &str,
    error: &Value,
) -> Option<Vec<Value>> {
    let extensions = error.get("extensions")?.as_object()?;
    if extensions.get("code").and_then(Value::as_str) != Some("argumentNotAccepted")
        || extensions.get("typeName").and_then(Value::as_str) != Some("InputObject")
    {
        return None;
    }
    let input_type = extensions.get("name")?.as_str()?;
    let error_path = error.get("path")?.as_array()?;
    if error_path.len() < 4 {
        return None;
    }
    let response_key = error_path.get(1)?.as_str()?;
    let field = document
        .root_fields
        .iter()
        .find(|field| field.response_key == response_key)?;
    let parent_path = error_path[2..error_path.len() - 1]
        .iter()
        .map(|segment| {
            segment
                .as_str()
                .map(str::to_string)
                .or_else(|| segment.as_u64().map(|value| value.to_string()))
        })
        .collect::<Option<Vec<_>>>()?;
    let parent_path_refs = parent_path.iter().map(String::as_str).collect::<Vec<_>>();
    let RawArgumentValue::Object(input_fields) = raw_input_value_at_path(field, &parent_path_refs)?
    else {
        return None;
    };
    let known_fields = admin_graphql::input_object_fields(version, input_type)?
        .into_iter()
        .map(|field| field.name)
        .collect::<BTreeSet<_>>();
    let target_depth = 1 + parent_path.len() as i32;
    let fallback_location = error_location(error)
        .map(|(line, column)| SourceLocation { line, column })
        .unwrap_or(field.location);
    let mut unknown_fields = input_fields
        .keys()
        .filter(|name| !known_fields.contains(*name))
        .map(|name| {
            let location =
                inline_input_field_name_location(query, field.location, target_depth, name)
                    .unwrap_or(fallback_location);
            (location, name.clone())
        })
        .collect::<Vec<_>>();
    if unknown_fields.len() <= 1 {
        return None;
    }
    unknown_fields.sort_by_key(|(location, name)| (location.line, location.column, name.clone()));

    Some(
        unknown_fields
            .into_iter()
            .map(|(location, argument_name)| {
                let mut path = document_field_path(document, response_key);
                path.extend(input_path_values(parent_path_refs.iter().copied()));
                path.push(json!(argument_name));
                json!({
                    "message": format!(
                        "InputObject '{input_type}' doesn't accept argument '{argument_name}'"
                    ),
                    "locations": [{ "line": location.line, "column": location.column }],
                    "path": path,
                    "extensions": {
                        "code": "argumentNotAccepted",
                        "name": input_type,
                        "typeName": "InputObject",
                        "argumentName": argument_name
                    }
                })
            })
            .collect(),
    )
}

fn expand_inline_missing_input_errors(
    version: AdminApiVersion,
    document: &ParsedDocument,
    error: &Value,
) -> Option<Vec<Value>> {
    let extensions = error.get("extensions")?.as_object()?;
    if extensions.get("code").and_then(Value::as_str) != Some("missingRequiredInputObjectAttribute")
    {
        return None;
    }
    let input_type = extensions.get("inputObjectType")?.as_str()?;
    let error_path = error.get("path")?.as_array()?;
    if error_path.len() < 4 {
        return None;
    }
    let response_key = error_path.get(1)?.as_str()?;
    let field = document
        .root_fields
        .iter()
        .find(|field| field.response_key == response_key)?;
    let parent_path = error_path[2..error_path.len() - 1]
        .iter()
        .map(|segment| {
            segment
                .as_str()
                .map(str::to_string)
                .or_else(|| segment.as_u64().map(|value| value.to_string()))
        })
        .collect::<Option<Vec<_>>>()?;
    let parent_path_refs = parent_path.iter().map(String::as_str).collect::<Vec<_>>();
    let RawArgumentValue::Object(provided_fields) =
        raw_input_value_at_path(field, &parent_path_refs)?
    else {
        return None;
    };
    let mut missing_fields = admin_graphql::input_object_fields(version, input_type)?
        .into_iter()
        .filter(|field| field.required && !provided_fields.contains_key(&field.name))
        .collect::<Vec<_>>();
    // async-graphql reports missing inline input fields in its registry order,
    // which is lexical. Expand the complete set in that same order while
    // variable coercion continues to follow the incoming JSON object order.
    missing_fields.sort_by(|left, right| left.name.cmp(&right.name));
    if missing_fields.len() <= 1 {
        return None;
    }
    let location = error_location(error)
        .map(|(line, column)| SourceLocation { line, column })
        .unwrap_or(field.location);

    Some(
        missing_fields
            .into_iter()
            .map(|missing| {
                let mut path = document_field_path(document, response_key);
                path.extend(input_path_values(parent_path_refs.iter().copied()));
                path.push(json!(missing.name));
                json!({
                    "message": format!(
                        "Argument '{}' on InputObject '{input_type}' is required. Expected type {}",
                        missing.name, missing.type_display
                    ),
                    "locations": [{ "line": location.line, "column": location.column }],
                    "path": path,
                    "extensions": {
                        "code": "missingRequiredInputObjectAttribute",
                        "argumentName": missing.name,
                        "argumentType": missing.type_display,
                        "inputObjectType": input_type
                    }
                })
            })
            .collect(),
    )
}

fn raw_input_value_at_path<'a>(
    field: &'a RootFieldSelection,
    path: &[&str],
) -> Option<&'a RawArgumentValue> {
    let mut value = field.raw_arguments.get(*path.first()?)?;
    for segment in path.iter().skip(1) {
        value = match value {
            RawArgumentValue::Object(fields) => fields.get(*segment)?,
            RawArgumentValue::List(items) => items.get(segment.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(value)
}

fn raw_input_display(value: &RawArgumentValue) -> String {
    match value {
        RawArgumentValue::String(value) => json!(value).to_string(),
        RawArgumentValue::Int(value) => value.to_string(),
        RawArgumentValue::Float(value) => value.to_string(),
        RawArgumentValue::Bool(value) => value.to_string(),
        RawArgumentValue::Null => "null".to_string(),
        RawArgumentValue::Enum(value) => value.clone(),
        RawArgumentValue::List(values) => format!(
            "[{}]",
            values
                .iter()
                .map(raw_input_display)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        RawArgumentValue::Object(fields) => format!(
            "{{{}}}",
            fields
                .iter()
                .map(|(name, value)| format!("{name}: {}", raw_input_display(value)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        RawArgumentValue::Variable { name, .. } => format!("${name}"),
    }
}

fn shopify_unknown_directive_argument_error(
    message: &str,
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
    engine_error: &Value,
) -> Option<Value> {
    let rest = message.strip_prefix("Unknown argument \"")?;
    let (argument_name, rest) = rest.split_once("\" on directive \"")?;
    let (directive_name, _) = rest.split_once('"')?;
    let invocation = directive_invocations(query, variables)?
        .into_iter()
        .find(|invocation| {
            invocation.name == directive_name
                && invocation.raw_arguments.contains_key(argument_name)
        })?;
    let mut path = invocation
        .path
        .iter()
        .map(|segment| json!(segment))
        .collect::<Vec<_>>();
    path.push(json!(argument_name));
    Some(json!({
        "message": format!(
            "Directive '{directive_name}' doesn't accept argument '{argument_name}'"
        ),
        "locations": engine_error
            .get("locations")
            .cloned()
            .unwrap_or_else(|| json!([])),
        "path": path,
        "extensions": {
            "code": "argumentNotAccepted",
            "name": directive_name,
            "typeName": "Directive",
            "argumentName": argument_name
        }
    }))
}

fn supplied_variable<'a>(
    variables: &'a BTreeMap<String, ResolvedValue>,
    name: &str,
) -> Option<&'a ResolvedValue> {
    variables.get(name)
}

fn response_path_for_location(
    document: &ParsedDocument,
    location: (usize, usize),
) -> Option<Vec<Value>> {
    for field in &document.root_fields {
        let mut path = document_field_path(document, &field.response_key);
        if (field.location.line, field.location.column) == location {
            return Some(path);
        }
        if selected_response_path_for_location(&field.selection, location, &mut path) {
            return Some(path);
        }
    }
    nearest_response_path(document, location)
}

fn nearest_response_path(
    document: &ParsedDocument,
    location: (usize, usize),
) -> Option<Vec<Value>> {
    let mut best: Option<((usize, usize), Vec<Value>)> = None;
    for field in &document.root_fields {
        let path = document_field_path(document, &field.response_key);
        consider_nearest_path(field.location, &path, location, &mut best);
        nearest_selected_response_path(&field.selection, location, path, &mut best);
    }
    best.map(|(_, path)| path)
}

fn document_field_path(document: &ParsedDocument, response_key: &str) -> Vec<Value> {
    vec![json!(document.operation_path), json!(response_key)]
}

fn nearest_selected_response_path(
    fields: &[SelectedField],
    location: (usize, usize),
    path: Vec<Value>,
    best: &mut Option<((usize, usize), Vec<Value>)>,
) {
    for field in fields {
        let mut field_path = path.clone();
        field_path.push(json!(field.response_key));
        consider_nearest_path(field.location, &field_path, location, best);
        nearest_selected_response_path(&field.selection, location, field_path, best);
    }
}

fn consider_nearest_path(
    candidate: SourceLocation,
    path: &[Value],
    location: (usize, usize),
    best: &mut Option<((usize, usize), Vec<Value>)>,
) {
    let position = (candidate.line, candidate.column);
    if position > location
        || best
            .as_ref()
            .is_some_and(|(current, _)| *current >= position)
    {
        return;
    }
    *best = Some((position, path.to_vec()));
}

fn selected_response_path_for_location(
    fields: &[SelectedField],
    location: (usize, usize),
    path: &mut Vec<Value>,
) -> bool {
    for field in fields {
        path.push(json!(field.response_key));
        if (field.location.line, field.location.column) == location
            || selected_response_path_for_location(&field.selection, location, path)
        {
            return true;
        }
        path.pop();
    }
    false
}

pub(super) fn required_variable_error(
    document: &ParsedDocument,
    supplied_variables: &BTreeMap<String, ResolvedValue>,
) -> Option<Value> {
    let definition = document.variable_definitions.values().find(|definition| {
        definition.type_display.ends_with('!')
            && match supplied_variables.get(&definition.name) {
                Some(ResolvedValue::Null) => true,
                Some(_) => false,
                None => definition.default_value.is_none(),
            }
    })?;
    Some(json!({
        "message": format!(
            "Variable ${} of type {} was provided invalid value",
            definition.name, definition.type_display
        ),
        "locations": [{
            "line": definition.location.line,
            "column": definition.location.column
        }],
        "extensions": {
            "code": "INVALID_VARIABLE",
            "value": null,
            "problems": [{
                "path": [],
                "explanation": "Expected value to not be null"
            }]
        }
    }))
}

pub(super) fn product_create_argument_arity_error(document: &ParsedDocument) -> Option<Value> {
    if document.operation_type != OperationType::Mutation {
        return None;
    }
    let field = document
        .root_fields
        .iter()
        .find(|field| field.name == "productCreate")?;
    let accepted_argument_count = usize::from(field.raw_arguments.contains_key("input"))
        + usize::from(field.raw_arguments.contains_key("product"));
    if accepted_argument_count == 1 {
        return None;
    }
    Some(json!({
        "data": { field.response_key.clone(): Value::Null },
        "errors": [{
            "message": "productCreate must include exactly one of the following arguments: input, product.",
            "locations": [{
                "line": field.location.line,
                "column": field.location.column
            }],
            "extensions": { "code": "INVALID_FIELD_ARGUMENTS" },
            "path": [field.response_key.clone()]
        }]
    }))
}

pub(super) fn directive_variable_mismatch_error(
    document: &ParsedDocument,
    query: &str,
    supplied_variables: &BTreeMap<String, ResolvedValue>,
) -> Option<Value> {
    let invocations = directive_invocations(query, supplied_variables)?;
    for invocation in invocations {
        for (argument_name, argument) in &invocation.raw_arguments {
            let RawArgumentValue::Variable { name, .. } = argument else {
                continue;
            };
            let Some(definition) = document.variable_definitions.get(name) else {
                let location =
                    argument_value_location_after(query, invocation.location, argument_name)
                        .unwrap_or(invocation.location);
                let mut path = invocation
                    .path
                    .iter()
                    .map(|segment| json!(segment))
                    .collect::<Vec<_>>();
                path.push(json!(argument_name));
                let operation = document.operation_name.as_deref().map_or_else(
                    || format!("anonymous {}", document.operation_type.keyword()),
                    |name| format!("{} {name}", document.operation_type.keyword()),
                );
                return Some(json!({
                    "message": format!(
                        "Variable ${name} is used by {operation} but not declared"
                    ),
                    "locations": [{
                        "line": location.line,
                        "column": location.column
                    }],
                    "path": path,
                    "extensions": {
                        "code": "variableNotDefined",
                        "variableName": name
                    }
                }));
            };
            let expected_type = match (invocation.name.as_str(), argument_name.as_str()) {
                ("include" | "skip", "if") => "Boolean!",
                _ => continue,
            };
            if definition.type_display.ends_with('!') || definition.default_value.is_some() {
                continue;
            }
            let location = argument_name_location_after(query, invocation.location, argument_name)
                .unwrap_or(invocation.location);
            let mut path = invocation
                .path
                .iter()
                .map(|segment| json!(segment))
                .collect::<Vec<_>>();
            path.push(json!(argument_name));
            return Some(json!({
                "message": format!(
                    "Nullability mismatch on variable ${name} and argument {argument_name} ({} / {expected_type})",
                    definition.type_display
                ),
                "locations": [{
                    "line": location.line,
                    "column": location.column
                }],
                "path": path,
                "extensions": {
                    "code": "variableMismatch",
                    "variableName": name,
                    "typeName": definition.type_display,
                    "argumentName": argument_name,
                    "errorMessage": "Nullability mismatch"
                }
            }));
        }
    }
    None
}

fn async_graphql_missing_field_argument(message: &str) -> Option<(String, String)> {
    let rest = message.strip_prefix("Field \"")?;
    let (field_name, rest) = rest.split_once("\" argument \"")?;
    let (argument_name, rest) = rest.split_once('"')?;
    rest.contains("is required but not provided")
        .then(|| (field_name.to_string(), argument_name.to_string()))
}

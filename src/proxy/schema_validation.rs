use super::*;

use crate::graphql::ParsedDocument;
use std::sync::OnceLock;

#[derive(Debug, Clone)]
struct SchemaTypeRef {
    display: String,
    named_type: String,
    non_null: bool,
}

#[derive(Debug, Clone)]
struct SchemaArgument {
    type_ref: SchemaTypeRef,
}

#[derive(Debug, Clone)]
struct SchemaInputField {
    type_ref: SchemaTypeRef,
}

#[derive(Debug, Clone, Default)]
struct AdminInputSchema {
    mutation_fields: BTreeMap<String, BTreeMap<String, SchemaArgument>>,
    input_objects: BTreeMap<String, BTreeMap<String, SchemaInputField>>,
}

#[derive(Debug, Clone, Copy)]
struct ValidationContext<'a> {
    query: &'a str,
    operation_path: &'a str,
    response_key: &'a str,
    field_location: SourceLocation,
}

#[derive(Debug, Clone, Copy)]
struct VariableValidationContext<'a> {
    variable_name: &'a str,
    variable_type: &'a str,
    location: SourceLocation,
}

pub(in crate::proxy) fn public_admin_schema_input_errors(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Vec<Value> {
    let Some(document) = parsed_document(query, variables) else {
        return Vec::new();
    };
    if document.operation_type != OperationType::Mutation {
        return Vec::new();
    }
    let schema = public_admin_input_schema();
    let mut errors = Vec::new();
    for field in &document.root_fields {
        let Some(arguments) = schema.mutation_fields.get(&field.name) else {
            continue;
        };
        let context = ValidationContext {
            query,
            operation_path: &document.operation_path,
            response_key: &field.response_key,
            field_location: field.location,
        };
        for (argument_name, argument_value) in &field.raw_arguments {
            let Some(argument_schema) = arguments.get(argument_name) else {
                errors.push(root_argument_not_accepted_error(
                    field,
                    argument_name,
                    context,
                ));
                continue;
            };
            errors.extend(validate_argument_value(
                argument_name,
                &argument_schema.type_ref,
                argument_value,
                field,
                &document,
                schema,
                context,
            ));
        }
        for (argument_name, argument_schema) in arguments {
            if argument_schema.type_ref.non_null && !field.raw_arguments.contains_key(argument_name)
            {
                errors.push(required_root_argument_error(
                    field,
                    argument_name,
                    &argument_schema.type_ref,
                    context,
                ));
            }
        }
    }
    errors
}

fn validate_argument_value(
    argument_name: &str,
    type_ref: &SchemaTypeRef,
    value: &RawArgumentValue,
    field: &RootFieldSelection,
    document: &ParsedDocument,
    schema: &AdminInputSchema,
    context: ValidationContext<'_>,
) -> Vec<Value> {
    let Some(input_object) = schema.input_objects.get(&type_ref.named_type) else {
        return Vec::new();
    };
    match value {
        RawArgumentValue::Object(fields) => validate_raw_input_object(
            &type_ref.named_type,
            input_object,
            fields,
            &[argument_name.to_string()],
            schema,
            context,
        ),
        RawArgumentValue::Variable { name, value } => {
            let Some(ResolvedValue::Object(fields)) = value.as_ref() else {
                return Vec::new();
            };
            let variable_type = document
                .variable_definitions
                .get(name)
                .map(|definition| definition.type_display.as_str())
                .unwrap_or(type_ref.display.as_str());
            let location = document
                .variable_definitions
                .get(name)
                .map(|definition| definition.location)
                .unwrap_or(field.location);
            let variable_context = VariableValidationContext {
                variable_name: name,
                variable_type,
                location,
            };
            let problems = validate_resolved_input_object(
                &type_ref.named_type,
                input_object,
                fields,
                &[],
                schema,
            );
            if problems.is_empty() {
                Vec::new()
            } else {
                vec![invalid_variable_error(
                    variable_context,
                    &ResolvedValue::Object(fields.clone()),
                    problems,
                )]
            }
        }
        RawArgumentValue::Null if type_ref.non_null => vec![required_root_argument_error(
            field,
            argument_name,
            type_ref,
            context,
        )],
        _ => Vec::new(),
    }
}

fn validate_raw_input_object(
    input_type_name: &str,
    input_object: &BTreeMap<String, SchemaInputField>,
    fields: &BTreeMap<String, RawArgumentValue>,
    path: &[String],
    schema: &AdminInputSchema,
    context: ValidationContext<'_>,
) -> Vec<Value> {
    let mut errors = Vec::new();
    for field_name in fields.keys() {
        if !input_object.contains_key(field_name)
            && !local_extension_input_field(input_type_name, field_name)
        {
            errors.push(input_object_argument_not_accepted_error(
                input_type_name,
                field_name,
                path,
                context,
            ));
        }
    }
    for (field_name, field_schema) in input_object {
        if field_schema.type_ref.non_null
            && (!fields.contains_key(field_name)
                || matches!(fields.get(field_name), Some(RawArgumentValue::Null)))
        {
            errors.push(missing_required_input_object_attribute_error(
                input_type_name,
                field_name,
                &field_schema.type_ref,
                path,
                context,
            ));
        }
    }
    for (field_name, field_value) in fields {
        let Some(field_schema) = input_object.get(field_name) else {
            continue;
        };
        let Some(nested_input_object) = schema.input_objects.get(&field_schema.type_ref.named_type)
        else {
            continue;
        };
        if let RawArgumentValue::Object(nested_fields) = field_value {
            let mut nested_path = path.to_vec();
            nested_path.push(field_name.clone());
            errors.extend(validate_raw_input_object(
                &field_schema.type_ref.named_type,
                nested_input_object,
                nested_fields,
                &nested_path,
                schema,
                context,
            ));
        }
    }
    errors
}

fn validate_resolved_input_object(
    input_type_name: &str,
    input_object: &BTreeMap<String, SchemaInputField>,
    fields: &BTreeMap<String, ResolvedValue>,
    problem_path: &[String],
    schema: &AdminInputSchema,
) -> Vec<Value> {
    let mut problems = Vec::new();
    for field_name in fields.keys() {
        if !input_object.contains_key(field_name)
            && !local_extension_input_field(input_type_name, field_name)
        {
            let mut nested_path = problem_path.to_vec();
            nested_path.push(field_name.clone());
            problems.push(variable_problem(
                &nested_path,
                &format!("Field is not defined on {input_type_name}"),
            ));
        }
    }
    for (field_name, field_schema) in input_object {
        if field_schema.type_ref.non_null
            && (!fields.contains_key(field_name)
                || matches!(fields.get(field_name), Some(ResolvedValue::Null)))
        {
            let mut nested_path = problem_path.to_vec();
            nested_path.push(field_name.clone());
            problems.push(variable_problem(
                &nested_path,
                "Expected value to not be null",
            ));
        }
    }
    for (field_name, field_value) in fields {
        let Some(field_schema) = input_object.get(field_name) else {
            continue;
        };
        let Some(nested_input_object) = schema.input_objects.get(&field_schema.type_ref.named_type)
        else {
            continue;
        };
        if let ResolvedValue::Object(nested_fields) = field_value {
            let mut nested_path = problem_path.to_vec();
            nested_path.push(field_name.clone());
            problems.extend(validate_resolved_input_object(
                &field_schema.type_ref.named_type,
                nested_input_object,
                nested_fields,
                &nested_path,
                schema,
            ));
        }
    }
    problems
}

fn root_argument_not_accepted_error(
    field: &RootFieldSelection,
    argument_name: &str,
    context: ValidationContext<'_>,
) -> Value {
    let location = root_argument_name_location(context.query, field, argument_name)
        .unwrap_or(context.field_location);
    json!({
        "message": format!("Field '{}' doesn't accept argument '{}'", field.name, argument_name),
        "locations": [{ "line": location.line, "column": location.column }],
        "path": [context.operation_path, context.response_key, argument_name],
        "extensions": {
            "code": "argumentNotAccepted",
            "name": field.name,
            "typeName": "Field",
            "argumentName": argument_name
        }
    })
}

fn root_argument_name_location(
    query: &str,
    field: &RootFieldSelection,
    argument_name: &str,
) -> Option<SourceLocation> {
    let start = byte_offset_for_location(query, field.location)?;
    let mut search_start = start;
    while search_start <= query.len() {
        let offset = query[search_start..].find(argument_name)? + search_start;
        let after_name = offset + argument_name.len();
        let next_non_whitespace =
            query[after_name..]
                .char_indices()
                .find_map(|(inner_offset, ch)| {
                    (!ch.is_whitespace()).then_some(after_name + inner_offset)
                })?;
        if query[next_non_whitespace..].starts_with(':') {
            return source_location_for_byte_offset(query, offset);
        }
        search_start = after_name;
    }
    None
}

fn required_root_argument_error(
    field: &RootFieldSelection,
    argument_name: &str,
    type_ref: &SchemaTypeRef,
    context: ValidationContext<'_>,
) -> Value {
    json!({
        "message": format!("Field '{}' is missing required arguments: {}", field.name, argument_name),
        "locations": [{ "line": context.field_location.line, "column": context.field_location.column }],
        "path": [context.operation_path, context.response_key],
        "extensions": {
            "code": "missingRequiredArguments",
            "className": field.name,
            "name": argument_name,
            "typeName": type_ref.display
        }
    })
}

fn input_object_argument_not_accepted_error(
    input_type_name: &str,
    argument_name: &str,
    path: &[String],
    context: ValidationContext<'_>,
) -> Value {
    json!({
        "message": format!("InputObject '{input_type_name}' doesn't accept argument '{argument_name}'"),
        "locations": [{ "line": context.field_location.line, "column": context.field_location.column }],
        "path": input_error_path(context, path, argument_name),
        "extensions": {
            "code": "argumentNotAccepted",
            "name": input_type_name,
            "typeName": "InputObject",
            "argumentName": argument_name
        }
    })
}

fn missing_required_input_object_attribute_error(
    input_type_name: &str,
    argument_name: &str,
    type_ref: &SchemaTypeRef,
    path: &[String],
    context: ValidationContext<'_>,
) -> Value {
    json!({
        "message": format!(
            "Argument '{argument_name}' on InputObject '{input_type_name}' is required. Expected type {}",
            type_ref.display
        ),
        "locations": [{ "line": context.field_location.line, "column": context.field_location.column }],
        "path": input_error_path(context, path, argument_name),
        "extensions": {
            "code": "missingRequiredInputObjectAttribute",
            "argumentName": argument_name,
            "argumentType": type_ref.display,
            "inputObjectType": input_type_name
        }
    })
}

fn invalid_variable_error(
    context: VariableValidationContext<'_>,
    value: &ResolvedValue,
    problems: Vec<Value>,
) -> Value {
    let problem_display = problems
        .iter()
        .filter_map(|problem| {
            let path = problem["path"]
                .as_array()?
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(".");
            let explanation = problem["explanation"].as_str()?;
            Some(format!("{path} ({explanation})"))
        })
        .collect::<Vec<_>>()
        .join(", ");
    json!({
        "message": format!(
            "Variable ${} of type {} was provided invalid value for {}",
            context.variable_name,
            context.variable_type,
            problem_display
        ),
        "locations": [{ "line": context.location.line, "column": context.location.column }],
        "extensions": {
            "code": "INVALID_VARIABLE",
            "value": resolved_value_json(value),
            "problems": problems
        }
    })
}

fn variable_problem(path: &[String], explanation: &str) -> Value {
    json!({
        "path": path,
        "explanation": explanation
    })
}

fn input_error_path(context: ValidationContext<'_>, path: &[String], argument_name: &str) -> Value {
    let mut segments = vec![
        Value::String(context.operation_path.to_string()),
        Value::String(context.response_key.to_string()),
    ];
    segments.extend(path.iter().cloned().map(Value::String));
    segments.push(Value::String(argument_name.to_string()));
    Value::Array(segments)
}

fn byte_offset_for_location(query: &str, location: SourceLocation) -> Option<usize> {
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

fn source_location_for_byte_offset(query: &str, target_offset: usize) -> Option<SourceLocation> {
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

fn local_extension_input_field(input_type_name: &str, field_name: &str) -> bool {
    matches!(
        (input_type_name, field_name),
        ("GiftCardCreateInput", "notify")
    )
}

fn public_admin_input_schema() -> &'static AdminInputSchema {
    static SCHEMA: OnceLock<AdminInputSchema> = OnceLock::new();
    SCHEMA.get_or_init(|| {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/gift-cards/gift-card-create-validation.json"
        ))
        .unwrap_or_else(|_| json!({}));
        let mut schema = schema_from_fixture(&fixture).unwrap_or_default();
        register_fulfillment_service_fields(&mut schema);
        schema
    })
}

fn register_fulfillment_service_fields(schema: &mut AdminInputSchema) {
    for (field_name, arguments) in [
        (
            "fulfillmentServiceCreate",
            &[
                ("name", "String"),
                ("callbackUrl", "URL"),
                ("trackingSupport", "Boolean"),
                ("inventoryManagement", "Boolean"),
                ("requiresShippingMethod", "Boolean"),
            ][..],
        ),
        (
            "fulfillmentServiceUpdate",
            &[
                ("id", "ID"),
                ("name", "String"),
                ("callbackUrl", "URL"),
                ("trackingSupport", "Boolean"),
                ("inventoryManagement", "Boolean"),
                ("requiresShippingMethod", "Boolean"),
            ][..],
        ),
    ] {
        schema.mutation_fields.insert(
            field_name.to_string(),
            arguments
                .iter()
                .map(|(argument_name, type_name)| {
                    (
                        (*argument_name).to_string(),
                        SchemaArgument {
                            type_ref: scalar_type_ref(type_name),
                        },
                    )
                })
                .collect(),
        );
    }
}

fn scalar_type_ref(type_name: &str) -> SchemaTypeRef {
    SchemaTypeRef {
        display: type_name.to_string(),
        named_type: type_name.to_string(),
        non_null: false,
    }
}

fn schema_from_fixture(fixture: &Value) -> Option<AdminInputSchema> {
    let data = &fixture["operations"]["schema"]["response"]["payload"]["data"];
    let mut schema = AdminInputSchema::default();

    let type_name = "GiftCardCreateInput";
    let fields = data[uncapitalize_type_alias(type_name)]["inputFields"].as_array()?;
    schema.input_objects.insert(
        type_name.to_string(),
        fields
            .iter()
            .filter_map(|field| {
                Some((
                    field["name"].as_str()?.to_string(),
                    SchemaInputField {
                        type_ref: schema_type_ref(&field["type"])?,
                    },
                ))
            })
            .collect(),
    );

    let mutation_fields = data["mutationRoot"]["fields"].as_array()?;
    for mutation_field in mutation_fields {
        let Some(field_name) = mutation_field["name"].as_str() else {
            continue;
        };
        let Some(args) = mutation_field["args"].as_array() else {
            continue;
        };
        let parsed_args = args
            .iter()
            .filter_map(|arg| {
                Some((
                    arg["name"].as_str()?.to_string(),
                    SchemaArgument {
                        type_ref: schema_type_ref(&arg["type"])?,
                    },
                ))
            })
            .collect::<BTreeMap<_, _>>();
        if !parsed_args
            .values()
            .any(|arg| schema.input_objects.contains_key(&arg.type_ref.named_type))
        {
            continue;
        }
        schema
            .mutation_fields
            .insert(field_name.to_string(), parsed_args);
    }

    Some(schema)
}

fn uncapitalize_type_alias(type_name: &str) -> String {
    let mut chars = type_name.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    format!(
        "{}{}",
        first.to_ascii_lowercase(),
        chars.collect::<String>()
    )
}

fn schema_type_ref(type_value: &Value) -> Option<SchemaTypeRef> {
    let kind = type_value["kind"].as_str()?;
    match kind {
        "NON_NULL" => {
            let mut inner = schema_type_ref(&type_value["ofType"])?;
            inner.display.push('!');
            inner.non_null = true;
            Some(inner)
        }
        "LIST" => {
            let inner = schema_type_ref(&type_value["ofType"])?;
            Some(SchemaTypeRef {
                display: format!("[{}]", inner.display),
                named_type: inner.named_type,
                non_null: false,
            })
        }
        _ => {
            let name = type_value["name"].as_str()?.to_string();
            Some(SchemaTypeRef {
                display: name.clone(),
                named_type: name,
                non_null: false,
            })
        }
    }
}

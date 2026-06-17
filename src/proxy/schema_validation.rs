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
        RawArgumentValue::List(_) => validate_raw_nested_input_object(
            &type_ref.named_type,
            input_object,
            value,
            &[argument_name.to_string()],
            schema,
            context,
        ),
        RawArgumentValue::Variable { name, value } => {
            let Some(value) = value.as_ref() else {
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
            let problems = validate_resolved_input_value(
                &type_ref.named_type,
                input_object,
                value,
                &[],
                schema,
            );
            if problems.is_empty() {
                Vec::new()
            } else {
                vec![invalid_variable_error(variable_context, value, problems)]
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
        if let Some(error) = validate_raw_value(
            input_type_name,
            field_name,
            field_value,
            &field_schema.type_ref,
            path,
            context,
        ) {
            errors.push(error);
        }
        let Some(nested_input_object) = schema.input_objects.get(&field_schema.type_ref.named_type)
        else {
            continue;
        };
        let mut nested_path = path.to_vec();
        nested_path.push(field_name.clone());
        errors.extend(validate_raw_nested_input_object(
            &field_schema.type_ref.named_type,
            nested_input_object,
            field_value,
            &nested_path,
            schema,
            context,
        ));
    }
    errors
}

fn validate_raw_nested_input_object(
    input_type_name: &str,
    input_object: &BTreeMap<String, SchemaInputField>,
    value: &RawArgumentValue,
    path: &[String],
    schema: &AdminInputSchema,
    context: ValidationContext<'_>,
) -> Vec<Value> {
    match value {
        RawArgumentValue::Object(nested_fields) => validate_raw_input_object(
            input_type_name,
            input_object,
            nested_fields,
            path,
            schema,
            context,
        ),
        RawArgumentValue::List(values) => values
            .iter()
            .flat_map(|value| {
                validate_raw_nested_input_object(
                    input_type_name,
                    input_object,
                    value,
                    path,
                    schema,
                    context,
                )
            })
            .collect(),
        _ => Vec::new(),
    }
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
        if let Some(problem) = validate_resolved_value(field_value, &field_schema.type_ref) {
            let mut nested_path = problem_path.to_vec();
            nested_path.push(field_name.clone());
            problems.push(variable_problem_with_message(&nested_path, &problem));
        }
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

fn validate_resolved_input_value(
    input_type_name: &str,
    input_object: &BTreeMap<String, SchemaInputField>,
    value: &ResolvedValue,
    problem_path: &[String],
    schema: &AdminInputSchema,
) -> Vec<Value> {
    match value {
        ResolvedValue::Object(fields) => validate_resolved_input_object(
            input_type_name,
            input_object,
            fields,
            problem_path,
            schema,
        ),
        ResolvedValue::List(values) => values
            .iter()
            .flat_map(|value| {
                validate_resolved_input_value(
                    input_type_name,
                    input_object,
                    value,
                    problem_path,
                    schema,
                )
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn validate_raw_value(
    input_type_name: &str,
    field_name: &str,
    value: &RawArgumentValue,
    type_ref: &SchemaTypeRef,
    path: &[String],
    context: ValidationContext<'_>,
) -> Option<Value> {
    match value {
        RawArgumentValue::Enum(value) => raw_enum_allowed_values(&type_ref.named_type)
            .filter(|allowed_values| !allowed_values.contains(&value.as_str()))
            .map(|_| {
                input_object_argument_literal_incompatible_error(
                    input_type_name,
                    field_name,
                    value,
                    type_ref,
                    path,
                    context,
                )
            }),
        RawArgumentValue::List(values) => values.iter().find_map(|value| {
            validate_raw_value(input_type_name, field_name, value, type_ref, path, context)
        }),
        _ => None,
    }
}

fn validate_resolved_value(value: &ResolvedValue, type_ref: &SchemaTypeRef) -> Option<String> {
    if let ResolvedValue::List(values) = value {
        return values
            .iter()
            .find_map(|value| validate_resolved_value(value, type_ref));
    }
    match (type_ref.named_type.as_str(), value) {
        ("Decimal", ResolvedValue::String(raw)) => raw
            .parse::<f64>()
            .err()
            .map(|_| format!("invalid decimal '{raw}'")),
        ("MetaobjectCustomerAccountAccess", ResolvedValue::String(raw)) => {
            let allowed_values = raw_enum_allowed_values(&type_ref.named_type)?;
            (!allowed_values.contains(&raw.as_str())).then(|| {
                format!(
                    "Expected \"{}\" to be one of: {}",
                    raw,
                    allowed_values.join(", ")
                )
            })
        }
        ("PublicationCreateInputPublicationDefaultState", ResolvedValue::String(raw)) => {
            let allowed_values = raw_enum_allowed_values(&type_ref.named_type)?;
            (!allowed_values.contains(&raw.as_str())).then(|| {
                format!(
                    "Expected \"{}\" to be one of: {}",
                    raw,
                    allowed_values.join(", ")
                )
            })
        }
        ("ResourceFeedbackState", ResolvedValue::String(raw)) => {
            let allowed_values = raw_enum_allowed_values(&type_ref.named_type)?;
            (!allowed_values.contains(&raw.as_str())).then(|| {
                format!(
                    "Expected \"{}\" to be one of: {}",
                    raw,
                    allowed_values.join(", ")
                )
            })
        }
        _ => None,
    }
}

fn raw_enum_allowed_values(type_name: &str) -> Option<&'static [&'static str]> {
    match type_name {
        "MetaobjectCustomerAccountAccess" => Some(&["NONE", "READ"]),
        "PublicationCreateInputPublicationDefaultState" => Some(&["EMPTY", "ALL_PRODUCTS"]),
        "ResourceFeedbackState" => Some(&["ACCEPTED", "REQUIRES_ACTION"]),
        _ => None,
    }
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
    if input_type_name == "ValidationUpdateInput"
        && matches!(argument_name, "functionId" | "functionHandle")
    {
        let location =
            input_field_name_location(context.query, context.field_location, argument_name)
                .unwrap_or(context.field_location);
        return json!({
            "message": format!("Field '{argument_name}' is not defined on ValidationUpdateInput"),
            "locations": [{ "line": location.line, "column": location.column }],
            "path": input_error_path(context, path, argument_name),
            "extensions": {
                "code": "argumentLiteralsIncompatible",
                "typeName": "InputObject",
                "argumentName": argument_name
            }
        });
    }
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

fn input_object_argument_literal_incompatible_error(
    input_type_name: &str,
    argument_name: &str,
    value: &str,
    type_ref: &SchemaTypeRef,
    path: &[String],
    context: ValidationContext<'_>,
) -> Value {
    let location_field = path.last().map(String::as_str).unwrap_or(argument_name);
    let location = raw_input_object_field_value_location(context.query, location_field)
        .unwrap_or(context.field_location);
    let expected_type = if raw_enum_allowed_values(&type_ref.named_type).is_some() {
        type_ref.named_type.as_str()
    } else {
        type_ref.display.as_str()
    };
    json!({
        "message": format!(
            "Argument '{argument_name}' on InputObject '{input_type_name}' has an invalid value ({value}). Expected type '{}'.",
            expected_type
        ),
        "locations": [{ "line": location.line, "column": location.column }],
        "path": input_error_path(context, path, argument_name),
        "extensions": {
            "code": "argumentLiteralsIncompatible",
            "typeName": "InputObject",
            "argumentName": argument_name
        }
    })
}

fn input_field_name_location(
    query: &str,
    field_location: SourceLocation,
    argument_name: &str,
) -> Option<SourceLocation> {
    let start = byte_offset_for_location(query, field_location)?;
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

fn variable_problem_with_message(path: &[String], explanation: &str) -> Value {
    json!({
        "path": path,
        "explanation": explanation,
        "message": explanation
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

fn raw_input_object_field_value_location(query: &str, field_name: &str) -> Option<SourceLocation> {
    let mut offset = 0;
    while let Some(relative_index) = query[offset..].find(field_name) {
        let name_start = offset + relative_index;
        let name_end = name_start + field_name.len();
        if !graphql_name_boundary(query, name_start, name_end) {
            offset = name_end;
            continue;
        }
        let colon_index = query[name_end..]
            .char_indices()
            .find_map(|(index, ch)| match ch {
                ':' => Some(name_end + index),
                ch if ch.is_whitespace() => None,
                _ => Some(usize::MAX),
            })?;
        if colon_index == usize::MAX {
            offset = name_end;
            continue;
        }
        let value_start = query[colon_index + 1..]
            .char_indices()
            .find_map(|(index, ch)| (!ch.is_whitespace()).then_some(colon_index + 1 + index))?;
        return source_location_for_byte_offset(query, value_start);
    }
    None
}

fn graphql_name_boundary(query: &str, start: usize, end: usize) -> bool {
    let before = query[..start].chars().next_back();
    let after = query[end..].chars().next();
    !before.is_some_and(is_graphql_name_char) && !after.is_some_and(is_graphql_name_char)
}

fn is_graphql_name_char(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphanumeric()
}

fn public_admin_input_schema() -> &'static AdminInputSchema {
    static SCHEMA: OnceLock<AdminInputSchema> = OnceLock::new();
    SCHEMA.get_or_init(|| {
        let mut schema = AdminInputSchema::default();
        extend_gift_card_input_schema(&mut schema);
        extend_discount_basic_input_schema(&mut schema);
        extend_fulfillment_service_input_schema(&mut schema);
        extend_functions_input_schema(&mut schema);
        extend_metaobject_definition_input_schema(&mut schema);
        extend_product_tail_input_schema(&mut schema);
        schema
    })
}

fn input_field(type_ref: SchemaTypeRef) -> SchemaInputField {
    SchemaInputField { type_ref }
}

fn mutation_arg(type_ref: SchemaTypeRef) -> SchemaArgument {
    SchemaArgument { type_ref }
}

fn mutation_args(args: &[(&str, SchemaTypeRef)]) -> BTreeMap<String, SchemaArgument> {
    args.iter()
        .map(|(name, type_ref)| ((*name).to_string(), mutation_arg(type_ref.clone())))
        .collect()
}

fn input_fields(fields: &[(&str, SchemaTypeRef)]) -> BTreeMap<String, SchemaInputField> {
    fields
        .iter()
        .map(|(name, type_ref)| ((*name).to_string(), input_field(type_ref.clone())))
        .collect()
}

fn extend_gift_card_input_schema(schema: &mut AdminInputSchema) {
    schema.input_objects.insert(
        "GiftCardCreateInput".to_string(),
        BTreeMap::from([
            ("initialValue".to_string(), input_field(non_null("Decimal"))),
            ("code".to_string(), input_field(named("String"))),
            ("customerId".to_string(), input_field(named("ID"))),
            ("expiresOn".to_string(), input_field(named("Date"))),
            ("note".to_string(), input_field(named("String"))),
            (
                "recipientAttributes".to_string(),
                input_field(named("GiftCardRecipientInput")),
            ),
            ("templateSuffix".to_string(), input_field(named("String"))),
        ]),
    );
    schema.mutation_fields.insert(
        "giftCardCreate".to_string(),
        BTreeMap::from([(
            "input".to_string(),
            mutation_arg(non_null("GiftCardCreateInput")),
        )]),
    );
}

fn extend_discount_basic_input_schema(schema: &mut AdminInputSchema) {
    schema.input_objects.insert(
        "DiscountCodeBasicInput".to_string(),
        BTreeMap::from([
            (
                "combinesWith".to_string(),
                input_field(named("DiscountCombinesWithInput")),
            ),
            ("title".to_string(), input_field(named("String"))),
            ("startsAt".to_string(), input_field(named("DateTime"))),
            ("endsAt".to_string(), input_field(named("DateTime"))),
            (
                "appliesOncePerCustomer".to_string(),
                input_field(named("Boolean")),
            ),
            ("code".to_string(), input_field(named("String"))),
            (
                "customerSelection".to_string(),
                input_field(named("DiscountCustomerSelectionInput")),
            ),
            ("usageLimit".to_string(), input_field(named("Int"))),
            (
                "context".to_string(),
                input_field(named("DiscountContextInput")),
            ),
            ("tags".to_string(), input_field(list_of_non_null("String"))),
            (
                "minimumRequirement".to_string(),
                input_field(named("DiscountMinimumRequirementInput")),
            ),
            (
                "customerGets".to_string(),
                input_field(named("DiscountCustomerGetsInput")),
            ),
            ("recurringCycleLimit".to_string(), input_field(named("Int"))),
        ]),
    );
    schema.input_objects.insert(
        "DiscountAutomaticBasicInput".to_string(),
        BTreeMap::from([
            (
                "combinesWith".to_string(),
                input_field(named("DiscountCombinesWithInput")),
            ),
            ("title".to_string(), input_field(named("String"))),
            ("startsAt".to_string(), input_field(named("DateTime"))),
            ("endsAt".to_string(), input_field(named("DateTime"))),
            (
                "context".to_string(),
                input_field(named("DiscountContextInput")),
            ),
            ("tags".to_string(), input_field(list_of_non_null("String"))),
            (
                "minimumRequirement".to_string(),
                input_field(named("DiscountMinimumRequirementInput")),
            ),
            (
                "customerGets".to_string(),
                input_field(named("DiscountCustomerGetsInput")),
            ),
            ("recurringCycleLimit".to_string(), input_field(named("Int"))),
        ]),
    );
    schema.input_objects.insert(
        "DiscountCombinesWithInput".to_string(),
        BTreeMap::from([
            (
                "productDiscounts".to_string(),
                input_field(named("Boolean")),
            ),
            ("orderDiscounts".to_string(), input_field(named("Boolean"))),
            (
                "shippingDiscounts".to_string(),
                input_field(named("Boolean")),
            ),
            (
                "productDiscountsWithTagsOnSameCartLine".to_string(),
                input_field(named("ProductDiscountsWithTagsOnSameCartLineInput")),
            ),
        ]),
    );
    schema.input_objects.insert(
        "ProductDiscountsWithTagsOnSameCartLineInput".to_string(),
        BTreeMap::from([
            ("add".to_string(), input_field(list_of_non_null("String"))),
            (
                "remove".to_string(),
                input_field(list_of_non_null("String")),
            ),
        ]),
    );
    schema.input_objects.insert(
        "DiscountCustomerSelectionInput".to_string(),
        BTreeMap::from([
            ("all".to_string(), input_field(named("Boolean"))),
            (
                "customers".to_string(),
                input_field(named("DiscountCustomersInput")),
            ),
            (
                "customerSegments".to_string(),
                input_field(named("DiscountCustomerSegmentsInput")),
            ),
        ]),
    );
    schema.input_objects.insert(
        "DiscountContextInput".to_string(),
        BTreeMap::from([
            (
                "all".to_string(),
                input_field(named("DiscountBuyerSelection")),
            ),
            (
                "customers".to_string(),
                input_field(named("DiscountCustomersInput")),
            ),
            (
                "customerSegments".to_string(),
                input_field(named("DiscountCustomerSegmentsInput")),
            ),
        ]),
    );
    schema.input_objects.insert(
        "DiscountCustomersInput".to_string(),
        BTreeMap::from([
            ("add".to_string(), input_field(list_of_non_null("ID"))),
            ("remove".to_string(), input_field(list_of_non_null("ID"))),
        ]),
    );
    schema.input_objects.insert(
        "DiscountCustomerSegmentsInput".to_string(),
        BTreeMap::from([
            ("add".to_string(), input_field(list_of_non_null("ID"))),
            ("remove".to_string(), input_field(list_of_non_null("ID"))),
        ]),
    );
    schema.input_objects.insert(
        "DiscountMinimumRequirementInput".to_string(),
        BTreeMap::from([
            (
                "quantity".to_string(),
                input_field(named("DiscountMinimumQuantityInput")),
            ),
            (
                "subtotal".to_string(),
                input_field(named("DiscountMinimumSubtotalInput")),
            ),
        ]),
    );
    schema.input_objects.insert(
        "DiscountMinimumQuantityInput".to_string(),
        BTreeMap::from([(
            "greaterThanOrEqualToQuantity".to_string(),
            input_field(named("UnsignedInt64")),
        )]),
    );
    schema.input_objects.insert(
        "DiscountMinimumSubtotalInput".to_string(),
        BTreeMap::from([(
            "greaterThanOrEqualToSubtotal".to_string(),
            input_field(named("Decimal")),
        )]),
    );
    schema.input_objects.insert(
        "DiscountCustomerGetsInput".to_string(),
        BTreeMap::from([
            (
                "value".to_string(),
                input_field(named("DiscountCustomerGetsValueInput")),
            ),
            (
                "items".to_string(),
                input_field(named("DiscountItemsInput")),
            ),
            (
                "appliesOnOneTimePurchase".to_string(),
                input_field(named("Boolean")),
            ),
            (
                "appliesOnSubscription".to_string(),
                input_field(named("Boolean")),
            ),
        ]),
    );
    schema.input_objects.insert(
        "DiscountCustomerGetsValueInput".to_string(),
        BTreeMap::from([
            (
                "discountOnQuantity".to_string(),
                input_field(named("DiscountOnQuantityInput")),
            ),
            ("percentage".to_string(), input_field(named("Float"))),
            (
                "discountAmount".to_string(),
                input_field(named("DiscountAmountInput")),
            ),
        ]),
    );
    schema.input_objects.insert(
        "DiscountItemsInput".to_string(),
        BTreeMap::from([
            (
                "products".to_string(),
                input_field(named("DiscountProductsInput")),
            ),
            (
                "collections".to_string(),
                input_field(named("DiscountCollectionsInput")),
            ),
            ("all".to_string(), input_field(named("Boolean"))),
        ]),
    );
    schema.input_objects.insert(
        "DiscountProductsInput".to_string(),
        BTreeMap::from([
            (
                "productsToAdd".to_string(),
                input_field(list_of_non_null("ID")),
            ),
            (
                "productsToRemove".to_string(),
                input_field(list_of_non_null("ID")),
            ),
            (
                "productVariantsToAdd".to_string(),
                input_field(list_of_non_null("ID")),
            ),
            (
                "productVariantsToRemove".to_string(),
                input_field(list_of_non_null("ID")),
            ),
        ]),
    );
    schema.input_objects.insert(
        "DiscountCollectionsInput".to_string(),
        BTreeMap::from([
            ("add".to_string(), input_field(list_of_non_null("ID"))),
            ("remove".to_string(), input_field(list_of_non_null("ID"))),
        ]),
    );
    schema.input_objects.insert(
        "DiscountOnQuantityInput".to_string(),
        BTreeMap::from([
            ("quantity".to_string(), input_field(named("UnsignedInt64"))),
            (
                "effect".to_string(),
                input_field(named("DiscountEffectInput")),
            ),
        ]),
    );
    schema.input_objects.insert(
        "DiscountEffectInput".to_string(),
        BTreeMap::from([
            ("percentage".to_string(), input_field(named("Float"))),
            ("amount".to_string(), input_field(named("Decimal"))),
        ]),
    );
    schema.input_objects.insert(
        "DiscountAmountInput".to_string(),
        BTreeMap::from([
            ("amount".to_string(), input_field(named("Decimal"))),
            (
                "appliesOnEachItem".to_string(),
                input_field(named("Boolean")),
            ),
        ]),
    );
    schema.mutation_fields.insert(
        "discountCodeBasicCreate".to_string(),
        BTreeMap::from([(
            "basicCodeDiscount".to_string(),
            mutation_arg(non_null("DiscountCodeBasicInput")),
        )]),
    );
    schema.mutation_fields.insert(
        "discountCodeBasicUpdate".to_string(),
        BTreeMap::from([
            ("id".to_string(), mutation_arg(non_null("ID"))),
            (
                "basicCodeDiscount".to_string(),
                mutation_arg(non_null("DiscountCodeBasicInput")),
            ),
        ]),
    );
    schema.mutation_fields.insert(
        "discountAutomaticBasicCreate".to_string(),
        BTreeMap::from([(
            "automaticBasicDiscount".to_string(),
            mutation_arg(non_null("DiscountAutomaticBasicInput")),
        )]),
    );
    schema.mutation_fields.insert(
        "discountAutomaticBasicUpdate".to_string(),
        BTreeMap::from([
            ("id".to_string(), mutation_arg(non_null("ID"))),
            (
                "automaticBasicDiscount".to_string(),
                mutation_arg(non_null("DiscountAutomaticBasicInput")),
            ),
        ]),
    );
}

fn extend_fulfillment_service_input_schema(schema: &mut AdminInputSchema) {
    schema.mutation_fields.insert(
        "fulfillmentServiceCreate".to_string(),
        mutation_args(&[
            ("name", non_null("String")),
            ("callbackUrl", named("URL")),
            ("trackingSupport", named("Boolean")),
            ("inventoryManagement", named("Boolean")),
            ("requiresShippingMethod", named("Boolean")),
        ]),
    );
    schema.mutation_fields.insert(
        "fulfillmentServiceUpdate".to_string(),
        mutation_args(&[
            ("id", non_null("ID")),
            ("name", named("String")),
            ("callbackUrl", named("URL")),
            ("trackingSupport", named("Boolean")),
            ("inventoryManagement", named("Boolean")),
            ("requiresShippingMethod", named("Boolean")),
        ]),
    );
}

fn extend_functions_input_schema(schema: &mut AdminInputSchema) {
    schema.input_objects.insert(
        "MetafieldInput".to_string(),
        input_fields(&[
            ("id", named("ID")),
            ("namespace", named("String")),
            ("key", named("String")),
            ("value", named("String")),
            ("type", named("String")),
        ]),
    );
    schema.input_objects.insert(
        "ValidationCreateInput".to_string(),
        input_fields(&[
            ("functionId", named("String")),
            ("functionHandle", named("String")),
            ("enable", named("Boolean")),
            ("blockOnFailure", named("Boolean")),
            ("metafields", list_of_non_null("MetafieldInput")),
            ("title", named("String")),
        ]),
    );
    schema.input_objects.insert(
        "ValidationUpdateInput".to_string(),
        input_fields(&[
            ("enable", named("Boolean")),
            ("blockOnFailure", named("Boolean")),
            ("metafields", list_of_non_null("MetafieldInput")),
            ("title", named("String")),
        ]),
    );
    schema.mutation_fields.insert(
        "validationCreate".to_string(),
        mutation_args(&[("validation", non_null("ValidationCreateInput"))]),
    );
    schema.mutation_fields.insert(
        "validationUpdate".to_string(),
        mutation_args(&[
            ("validation", non_null("ValidationUpdateInput")),
            ("id", non_null("ID")),
        ]),
    );
}

fn extend_metaobject_definition_input_schema(schema: &mut AdminInputSchema) {
    schema.input_objects.insert(
        "MetaobjectAccessInput".to_string(),
        input_fields(&[
            ("admin", named("MetaobjectAdminAccess")),
            ("storefront", named("MetaobjectStorefrontAccess")),
            ("customerAccount", named("MetaobjectCustomerAccountAccess")),
        ]),
    );
    schema.input_objects.insert(
        "MetaobjectDefinitionUpdateInput".to_string(),
        input_fields(&[
            ("access", named("MetaobjectAccessInput")),
            ("capabilities", named("MetaobjectCapabilityDataInput")),
            ("description", named("String")),
            ("displayNameKey", named("String")),
            (
                "fieldDefinitions",
                list_of_non_null("MetaobjectFieldDefinitionOperationInput"),
            ),
            ("name", named("String")),
            ("resetFieldOrder", named("Boolean")),
        ]),
    );
    schema.mutation_fields.insert(
        "metaobjectDefinitionUpdate".to_string(),
        mutation_args(&[
            ("id", non_null("ID")),
            ("definition", non_null("MetaobjectDefinitionUpdateInput")),
        ]),
    );
    schema.input_objects.insert(
        "MetaobjectDefinitionCreateInput".to_string(),
        input_fields(&[
            ("access", named("MetaobjectAccessInput")),
            ("capabilities", named("MetaobjectCapabilityDataInput")),
            ("description", named("String")),
            ("displayNameKey", named("String")),
            (
                "fieldDefinitions",
                list_of_non_null("MetaobjectFieldDefinitionCreateInput"),
            ),
            ("name", named("String")),
            ("type", named("String")),
        ]),
    );
    schema.mutation_fields.insert(
        "metaobjectDefinitionCreate".to_string(),
        mutation_args(&[("definition", non_null("MetaobjectDefinitionCreateInput"))]),
    );
    schema.mutation_fields.insert(
        "standardMetaobjectDefinitionEnable".to_string(),
        mutation_args(&[("type", non_null("String"))]),
    );
}

fn extend_product_tail_input_schema(schema: &mut AdminInputSchema) {
    schema.input_objects.insert(
        "PublicationCreateInput".to_string(),
        input_fields(&[
            ("catalogId", named("ID")),
            (
                "defaultState",
                named("PublicationCreateInputPublicationDefaultState"),
            ),
            ("autoPublish", named("Boolean")),
            ("channelId", named("ID")),
            ("name", named("String")),
        ]),
    );
    schema.input_objects.insert(
        "ProductResourceFeedbackInput".to_string(),
        input_fields(&[
            ("productId", non_null("ID")),
            ("state", non_null("ResourceFeedbackState")),
            ("feedbackGeneratedAt", non_null("DateTime")),
            ("productUpdatedAt", non_null("DateTime")),
            ("messages", list_of_non_null("String")),
            ("channelId", named("ID")),
        ]),
    );
    schema.input_objects.insert(
        "ResourceFeedbackCreateInput".to_string(),
        input_fields(&[
            ("feedbackGeneratedAt", non_null("DateTime")),
            ("messages", list_of_non_null("String")),
            ("state", non_null("ResourceFeedbackState")),
            ("channelId", named("ID")),
        ]),
    );
    schema.mutation_fields.insert(
        "publicationCreate".to_string(),
        mutation_args(&[("input", non_null("PublicationCreateInput"))]),
    );
    schema.mutation_fields.insert(
        "bulkProductResourceFeedbackCreate".to_string(),
        mutation_args(&[(
            "feedbackInput",
            non_null_list_of_non_null("ProductResourceFeedbackInput"),
        )]),
    );
    schema.mutation_fields.insert(
        "shopResourceFeedbackCreate".to_string(),
        mutation_args(&[("input", non_null("ResourceFeedbackCreateInput"))]),
    );
}

fn named(name: &str) -> SchemaTypeRef {
    SchemaTypeRef {
        display: name.to_string(),
        named_type: name.to_string(),
        non_null: false,
    }
}

fn non_null(name: &str) -> SchemaTypeRef {
    SchemaTypeRef {
        display: format!("{name}!"),
        named_type: name.to_string(),
        non_null: true,
    }
}

fn list_of_non_null(name: &str) -> SchemaTypeRef {
    SchemaTypeRef {
        display: format!("[{name}!]"),
        named_type: name.to_string(),
        non_null: false,
    }
}

fn non_null_list_of_non_null(name: &str) -> SchemaTypeRef {
    SchemaTypeRef {
        display: format!("[{name}!]!"),
        named_type: name.to_string(),
        non_null: true,
    }
}

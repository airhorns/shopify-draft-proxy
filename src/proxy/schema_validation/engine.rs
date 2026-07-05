use super::*;
use crate::graphql::ParsedDocument;
use graphql_parser::query::parse_query;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy)]
pub(in crate::proxy) struct ValidationContext<'a> {
    pub(in crate::proxy) query: &'a str,
    pub(in crate::proxy) operation_path: &'a str,
    pub(in crate::proxy) response_key: &'a str,
    pub(in crate::proxy) field_location: SourceLocation,
    // Raw request body text. serde_json parses JSON objects into sorted maps, so
    // the author's variable field order is lost by the time variables reach this
    // validator. Shopify reports INVALID_VARIABLE coercion problems in the order
    // fields appear in the request, so we recover that order from the raw body
    // text (which preserves it) when sorting unknown-field problems.
    pub(in crate::proxy) raw_body: &'a str,
}

/// First byte offset at which a JSON key (`"name"`) appears in `source`, used to
/// recover author/document order for fields whose order serde_json discarded.
/// Fields not found sort last (stable for unexpected shapes).
fn key_order_index(source: &str, field_name: &str) -> usize {
    let needle = format!("\"{field_name}\"");
    source.find(&needle).unwrap_or(usize::MAX)
}

#[derive(Debug, Clone, Copy)]
pub(in crate::proxy) struct VariableValidationContext<'a> {
    pub(in crate::proxy) variable_name: &'a str,
    pub(in crate::proxy) variable_type: &'a str,
    pub(in crate::proxy) location: SourceLocation,
}

pub(in crate::proxy) fn public_admin_schema_input_errors(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
    raw_body: &str,
    api_version: Option<&str>,
) -> Vec<Value> {
    let Some(api_version) = api_version.filter(|version| supported_admin_graphql_version(version))
    else {
        return Vec::new();
    };
    let Some(document) = parsed_document(query, variables) else {
        return Vec::new();
    };
    let mut errors = admin_platform_node_global_id_errors(query, raw_body, &document);
    if document.operation_type != OperationType::Mutation {
        return errors;
    }
    let schema = public_admin_input_schema(api_version)
        .expect("supported Admin API version should have captured input schema");
    for field in &document.root_fields {
        let Some(arguments) = schema.mutation_fields.get(&field.name) else {
            continue;
        };
        let context = ValidationContext {
            query,
            operation_path: &document.operation_path,
            response_key: &field.response_key,
            field_location: field.location,
            raw_body,
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
            if argument_schema.type_ref.non_null
                && !argument_schema.has_default
                && !field.raw_arguments.contains_key(argument_name)
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
    errors.extend(product_media_variable_errors(&document));
    errors.extend(metaobject_access_invalid_enum_errors(query, &document));
    errors
}

pub(in crate::proxy) fn public_admin_graphql_validation_response(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
    api_version: Option<&str>,
) -> Option<Response> {
    let api_version = api_version.filter(|version| supported_admin_graphql_version(version))?;

    if parse_query::<&str>(query).is_err() {
        return Some(ok_json(json!({
            "errors": [parse_error(query)]
        })));
    }

    let document = parsed_document(query, variables)?;
    let mut errors = missing_required_variable_errors(&document, variables);
    errors.extend(undefined_root_field_errors(&document, api_version));
    errors.extend(selection_mismatch_errors(&document, api_version));
    errors.extend(undefined_selection_field_errors(&document, api_version));
    if !errors.is_empty() {
        return Some(ok_json(json!({ "errors": errors })));
    }

    product_create_argument_arity_response(&document, api_version)
}

fn parse_error(query: &str) -> Value {
    let location = unexpected_end_of_file_location(query);
    json!({
        "message": format!(
            "syntax error, unexpected end of file at [{}, {}]",
            location.line, location.column
        ),
        "locations": [{ "line": location.line, "column": location.column }],
        "extensions": { "code": "PARSE_ERROR" }
    })
}

fn unexpected_end_of_file_location(query: &str) -> SourceLocation {
    let lines = query.lines().collect::<Vec<_>>();
    for (line_index, line) in lines.iter().enumerate().rev() {
        if let Some(column_index) = line.find(|character: char| !character.is_whitespace()) {
            return SourceLocation {
                line: line_index + 1,
                column: column_index + 1,
            };
        }
    }
    SourceLocation { line: 1, column: 1 }
}

fn missing_required_variable_errors(
    document: &ParsedDocument,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Vec<Value> {
    document
        .variable_definitions
        .values()
        .filter(|definition| definition.type_display.ends_with('!'))
        .filter(|definition| {
            !variables.contains_key(&definition.name)
                || matches!(variables.get(&definition.name), Some(ResolvedValue::Null))
        })
        .map(|definition| {
            non_null_variable_null_error(
                &definition.name,
                &definition.type_display,
                definition.location,
            )
        })
        .collect()
}

fn undefined_root_field_errors(document: &ParsedDocument, api_version: &str) -> Vec<Value> {
    let output_schema = public_admin_output_schema(api_version)
        .expect("supported Admin API version should have captured output schema");
    let mutation_root_names = public_admin_mutation_root_names(api_version)
        .expect("supported Admin API version should have captured mutation schema");
    document
        .root_fields
        .iter()
        .filter_map(|field| {
            let parent_type = match document.operation_type {
                OperationType::Query => {
                    (!output_schema.query_root_fields.contains_key(&field.name))
                        && !local_implemented_query_root_names().contains(&field.name)
                }
                .then_some("QueryRoot"),
                OperationType::Mutation => {
                    (!mutation_root_names.contains(&field.name))
                        && !local_implemented_mutation_root_names().contains(&field.name)
                }
                .then_some("Mutation"),
                OperationType::Subscription => None,
            }?;
            Some(undefined_field_error(
                document,
                field.location,
                parent_type,
                &field.name,
                vec![json!(document.operation_path), json!(field.response_key)],
            ))
        })
        .collect()
}

fn local_implemented_query_root_names() -> &'static BTreeSet<String> {
    static QUERY_ROOT_NAMES: OnceLock<BTreeSet<String>> = OnceLock::new();
    QUERY_ROOT_NAMES.get_or_init(|| local_implemented_root_names(OperationType::Query))
}

fn local_implemented_mutation_root_names() -> &'static BTreeSet<String> {
    static MUTATION_ROOT_NAMES: OnceLock<BTreeSet<String>> = OnceLock::new();
    MUTATION_ROOT_NAMES.get_or_init(|| local_implemented_root_names(OperationType::Mutation))
}

fn local_implemented_root_names(operation_type: OperationType) -> BTreeSet<String> {
    let mut roots = BTreeSet::new();
    for entry in default_registry()
        .into_iter()
        .filter(|entry| entry.implemented && entry.operation_type == operation_type)
    {
        roots.insert(entry.name);
        roots.extend(entry.match_names);
    }
    roots
}

fn selection_mismatch_errors(document: &ParsedDocument, api_version: &str) -> Vec<Value> {
    if document.operation_type != OperationType::Query {
        return Vec::new();
    }
    let output_schema = public_admin_output_schema(api_version)
        .expect("supported Admin API version should have captured output schema");
    document
        .root_fields
        .iter()
        .filter(|field| field.selection.is_empty())
        .filter_map(|field| {
            let output_type = output_schema.query_root_fields.get(&field.name)?;
            Some(json!({
                "message": format!(
                    "Field must have selections (field '{}' returns {} but has no selections. Did you mean '{} {{ ... }}'?)",
                    field.name, output_type.named_type, field.name
                ),
                "locations": [{ "line": field.location.line, "column": field.location.column }],
                "path": [document.operation_path.clone(), field.response_key.clone()],
                "extensions": {
                    "code": "selectionMismatch",
                    "nodeName": format!("field '{}'", field.name),
                    "typeName": output_type.named_type
                }
            }))
        })
        .collect()
}

fn undefined_selection_field_errors(document: &ParsedDocument, api_version: &str) -> Vec<Value> {
    let mut errors = Vec::new();
    let schema = public_admin_output_schema(api_version)
        .expect("supported Admin API version should have captured output schema");
    for field in &document.root_fields {
        let Some(output_type) = (match document.operation_type {
            OperationType::Query => schema.query_root_fields.get(&field.name),
            OperationType::Mutation => schema.mutation_root_fields.get(&field.name),
            OperationType::Subscription => None,
        }) else {
            continue;
        };
        let mode = match document.operation_type {
            OperationType::Query => UndefinedSelectionMode::AllFields,
            OperationType::Mutation => UndefinedSelectionMode::PlainUserErrorCodeOnly,
            OperationType::Subscription => continue,
        };
        collect_undefined_selection_field_errors(
            document,
            schema,
            &output_type.named_type,
            &field.selection,
            vec![json!(document.operation_path), json!(field.response_key)],
            mode,
            &mut errors,
        );
    }
    errors
}

#[derive(Clone, Copy)]
enum UndefinedSelectionMode {
    AllFields,
    PlainUserErrorCodeOnly,
}

fn collect_undefined_selection_field_errors(
    document: &ParsedDocument,
    output_schema: &AdminOutputSchema,
    parent_type: &str,
    selections: &[SelectedField],
    path: Vec<Value>,
    mode: UndefinedSelectionMode,
    errors: &mut Vec<Value>,
) {
    let schema_fields = output_schema.fields_by_parent.get(parent_type);
    for selection in selections {
        let mut child_path = path.clone();
        child_path.push(json!(selection.response_key));
        if selection.name == "__typename" {
            continue;
        }
        let selected_parent_type = selection.type_condition.as_deref().unwrap_or(parent_type);
        let selected_schema_fields = output_schema.fields_by_parent.get(selected_parent_type);
        let schema_fields = selected_schema_fields.or(schema_fields);

        if let Some(output_type) = schema_fields.and_then(|fields| fields.get(&selection.name)) {
            if !output_type.composite {
                continue;
            }
            collect_undefined_selection_field_errors(
                document,
                output_schema,
                &output_type.named_type,
                &selection.selection,
                child_path,
                mode,
                errors,
            );
        } else if schema_fields.is_some() {
            if matches!(mode, UndefinedSelectionMode::PlainUserErrorCodeOnly)
                && !(selected_parent_type == "UserError" && selection.name == "code")
            {
                continue;
            }
            errors.push(undefined_field_error(
                document,
                selection.location,
                selected_parent_type,
                &selection.name,
                child_path,
            ));
        }
    }
}

fn undefined_field_error(
    _document: &ParsedDocument,
    location: SourceLocation,
    parent_type: &str,
    field_name: &str,
    path: Vec<Value>,
) -> Value {
    json!({
        "message": format!("Field '{field_name}' doesn't exist on type '{parent_type}'"),
        "locations": [{ "line": location.line, "column": location.column }],
        "path": path,
        "extensions": {
            "code": "undefinedField",
            "typeName": parent_type,
            "fieldName": field_name
        }
    })
}

fn product_create_argument_arity_response(
    document: &ParsedDocument,
    api_version: &str,
) -> Option<Response> {
    if !version_at_least(api_version, 2025, 1) {
        return None;
    }

    if document.operation_type != OperationType::Mutation {
        return None;
    }
    let field = document
        .root_fields
        .iter()
        .find(|candidate| candidate.name == "productCreate")?;
    let accepted_argument_count = usize::from(field.raw_arguments.contains_key("input"))
        + usize::from(field.raw_arguments.contains_key("product"));
    if accepted_argument_count == 1 {
        return None;
    }
    let mut data = serde_json::Map::new();
    data.insert(field.response_key.clone(), Value::Null);
    Some(ok_json(json!({
        "data": Value::Object(data),
        "errors": [{
            "message": "productCreate must include exactly one of the following arguments: input, product.",
            "locations": [{ "line": field.location.line, "column": field.location.column }],
            "extensions": { "code": "INVALID_FIELD_ARGUMENTS" },
            "path": [field.response_key.clone()]
        }]
    })))
}

fn public_admin_mutation_root_names(api_version: &str) -> Option<&'static BTreeSet<String>> {
    static CACHE: VersionedSchemaCache<BTreeSet<String>> = VersionedSchemaCache::new();
    CACHE.get_or_init(api_version, || {
        let parsed = public_admin_schema_json(api_version, AdminSchemaKind::Mutation);
        parsed
            .get("mutations")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|mutation| mutation.get("name").and_then(Value::as_str))
            .map(str::to_string)
            .collect()
    })
}

fn public_admin_output_schema(api_version: &str) -> Option<&'static AdminOutputSchema> {
    static CACHE: VersionedSchemaCache<AdminOutputSchema> = VersionedSchemaCache::new();
    CACHE.get_or_init(api_version, || {
        let parsed = public_admin_schema_json(api_version, AdminSchemaKind::BulkQuery);
        let mut schema = AdminOutputSchema::default();
        for field in parsed
            .get("fields")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(parent_type) = field.get("parentType").and_then(Value::as_str) else {
                continue;
            };
            let Some(name) = field.get("name").and_then(Value::as_str) else {
                continue;
            };
            let Some(output_type) = output_field_type(field) else {
                continue;
            };
            if parent_type == "QueryRoot" {
                schema
                    .query_root_fields
                    .insert(name.to_string(), output_type.clone());
            }
            if parent_type == "Mutation" {
                schema
                    .mutation_root_fields
                    .insert(name.to_string(), output_type.clone());
            }
            schema
                .fields_by_parent
                .entry(parent_type.to_string())
                .or_default()
                .insert(name.to_string(), output_type);
        }
        schema.apply_local_projection_extensions();
        schema
    })
}

fn output_field_type(field: &Value) -> Option<OutputFieldType> {
    let kind = field.get("kind")?;
    let (named_type, composite) = match kind.get("type").and_then(Value::as_str)? {
        "object" => (
            kind.get("typeName").and_then(Value::as_str)?.to_string(),
            true,
        ),
        "connection" => {
            let node_type = kind.get("nodeType").and_then(Value::as_str)?;
            (format!("{node_type}Connection"), true)
        }
        "list" => (
            kind.get("elementType").and_then(Value::as_str)?.to_string(),
            true,
        ),
        "scalar" | "enum" => (
            kind.get("typeName").and_then(Value::as_str)?.to_string(),
            false,
        ),
        _ => return None,
    };
    Some(OutputFieldType {
        named_type,
        composite,
    })
}

pub(in crate::proxy) fn public_admin_output_field_named_type(
    api_version: &str,
    parent_type: &str,
    field_name: &str,
) -> Option<&'static str> {
    public_admin_output_schema(api_version)?
        .fields_by_parent
        .get(parent_type)?
        .get(field_name)
        .map(|field_type| field_type.named_type.as_str())
}

fn enum_values<'a>(schema: &'a AdminInputSchema, type_name: &str) -> Option<&'a [String]> {
    schema.enum_values.get(type_name).map(Vec::as_slice)
}

fn enum_value_allowed(values: &[String], provided: &str) -> bool {
    values
        .iter()
        .any(|candidate| candidate.as_str() == provided)
}

fn enum_expected_message(values: &[String], provided: &str) -> String {
    format!(
        "Expected \"{provided}\" to be one of: {}",
        values.join(", ")
    )
}

fn enum_literal_coercion_value(
    value: &RawArgumentValue,
    type_ref: &SchemaTypeRef,
    schema: &AdminInputSchema,
) -> Option<String> {
    let provided = match value {
        RawArgumentValue::Enum(value) | RawArgumentValue::String(value) => value,
        _ => return None,
    };
    let values = enum_values(schema, &type_ref.named_type)?;
    (!enum_value_allowed(values, provided)).then(|| provided.clone())
}

fn validate_resolved_scalar(
    value: &ResolvedValue,
    type_ref: &SchemaTypeRef,
    schema: &AdminInputSchema,
) -> Option<ScalarValidationProblem> {
    match type_ref.named_type.as_str() {
        "ID" => {
            let ResolvedValue::String(raw) = value else {
                return None;
            };
            raw.trim().is_empty().then(|| ScalarValidationProblem {
                explanation: format!("Invalid global id '{raw}'"),
                include_message: true,
            })
        }
        "Int" => {
            let ResolvedValue::Float(raw) = value else {
                return None;
            };
            Some(ScalarValidationProblem {
                explanation: format!("Could not coerce value {raw} to Int"),
                include_message: false,
            })
        }
        "Decimal" => {
            let ResolvedValue::String(raw) = value else {
                return None;
            };
            raw.parse::<f64>().err().map(|_| ScalarValidationProblem {
                explanation: format!("invalid decimal '{raw}'"),
                include_message: true,
            })
        }
        enum_type => {
            let values = enum_values(schema, enum_type)?;
            let ResolvedValue::String(raw) = value else {
                return None;
            };
            (!enum_value_allowed(values, raw)).then(|| ScalarValidationProblem {
                explanation: enum_expected_message(values, raw),
                include_message: false,
            })
        }
    }
}

fn admin_platform_node_global_id_errors(
    query: &str,
    raw_body: &str,
    document: &ParsedDocument,
) -> Vec<Value> {
    if document.operation_type != OperationType::Query {
        return Vec::new();
    }

    let mut errors = Vec::new();
    for field in &document.root_fields {
        let Some(argument_name) = (match field.name.as_str() {
            "node" => Some("id"),
            "nodes" => Some("ids"),
            _ => None,
        }) else {
            continue;
        };
        let context = ValidationContext {
            query,
            operation_path: &document.operation_path,
            response_key: &field.response_key,
            field_location: field.location,
            raw_body,
        };
        if let Some(error) = invalid_node_global_id_argument_error(
            document,
            field,
            argument_name,
            field.raw_arguments.get(argument_name),
            context,
        ) {
            errors.push(error);
        }
    }
    errors
}

fn invalid_node_global_id_argument_error(
    document: &ParsedDocument,
    field: &RootFieldSelection,
    argument_name: &str,
    argument_value: Option<&RawArgumentValue>,
    context: ValidationContext<'_>,
) -> Option<Value> {
    match argument_value? {
        RawArgumentValue::String(raw) if shopify_gid_resource_type(raw).is_none() => Some(
            invalid_global_id_literal_error(field, argument_name, raw, context),
        ),
        RawArgumentValue::List(values) => values.iter().find_map(|value| {
            invalid_node_global_id_argument_error(
                document,
                field,
                argument_name,
                Some(value),
                context,
            )
        }),
        RawArgumentValue::Variable { name, value } => invalid_global_id_variable_error(
            document,
            name,
            value.as_ref()?,
            first_invalid_variable_global_id_path(value.as_ref()?)?,
        ),
        _ => None,
    }
}

fn first_invalid_variable_global_id_path(value: &ResolvedValue) -> Option<(Vec<Value>, String)> {
    match value {
        ResolvedValue::String(raw) if shopify_gid_resource_type(raw).is_none() => {
            Some((Vec::new(), raw.clone()))
        }
        ResolvedValue::List(values) => values.iter().enumerate().find_map(|(index, value)| {
            let ResolvedValue::String(raw) = value else {
                return None;
            };
            (shopify_gid_resource_type(raw).is_none()).then(|| (vec![json!(index)], raw.clone()))
        }),
        _ => None,
    }
}

fn invalid_global_id_literal_error(
    field: &RootFieldSelection,
    argument_name: &str,
    invalid_id: &str,
    context: ValidationContext<'_>,
) -> Value {
    json!({
        "message": format!("Invalid global id '{invalid_id}'"),
        "locations": [{
            "line": context.field_location.line,
            "column": context.field_location.column,
        }],
        "path": [context.operation_path, field.response_key.as_str(), argument_name],
        "extensions": {
            "code": "argumentLiteralsIncompatible",
            "typeName": "CoercionError",
        },
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
        "locations": [{
            "line": location.line,
            "column": location.column,
        }],
        "extensions": {
            "code": "INVALID_VARIABLE",
            "value": value,
            "problems": problems,
        },
    })
}

fn invalid_global_id_variable_error(
    document: &ParsedDocument,
    variable_name: &str,
    variable_value: &ResolvedValue,
    (problem_path, invalid_id): (Vec<Value>, String),
) -> Option<Value> {
    let variable_definition = document.variable_definitions.get(variable_name)?;
    let explanation = format!("Invalid global id '{invalid_id}'");
    let path_display = variable_problem_path_display(&problem_path);
    let problem = json!({
        "path": problem_path,
        "explanation": explanation,
        "message": explanation,
    });
    let message = path_display.map_or_else(
        || {
            format!(
                "Variable ${variable_name} of type {} was provided invalid value",
                variable_definition.type_display
            )
        },
        |path_display| {
            format!(
                "Variable ${variable_name} of type {} was provided invalid value for {path_display} ({explanation})",
                variable_definition.type_display
            )
        },
    );
    Some(invalid_variable_error_envelope(
        message,
        variable_definition.location,
        resolved_value_json(variable_value),
        json!([problem]),
    ))
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

/// The product media mutations are not modelled in the declarative input
/// schema above, but they still reject a couple of variable-level shapes that
/// the parity captures assert on: a blank/invalid global id for the product,
/// and a `mediaContentType` enum value outside the allowed set. These are
/// genuine input checks (driven by the supplied values, not the fixture), so
/// they emit the same `INVALID_VARIABLE` coercion errors Shopify returns.
fn product_media_variable_errors(document: &ParsedDocument) -> Vec<Value> {
    let mut errors = Vec::new();
    for field in &document.root_fields {
        let (id_argument, media_argument) = match field.name.as_str() {
            "productCreateMedia" => ("productId", Some("media")),
            "productUpdateMedia" | "productDeleteMedia" => ("productId", None),
            "productReorderMedia" => ("id", None),
            _ => continue,
        };
        if let Some(error) = media_invalid_global_id_error(document, field, id_argument) {
            errors.push(error);
            // Product id precedence: a single coercion error short-circuits the
            // rest of the variable validation for this field.
            continue;
        }
        if let Some(media_argument) = media_argument {
            if let Some(error) = media_content_type_enum_error(document, field, media_argument) {
                errors.push(error);
            }
        }
    }
    errors
}

fn media_variable_binding(
    document: &ParsedDocument,
    field: &RootFieldSelection,
    argument_name: &str,
) -> Option<(String, String, ResolvedValue)> {
    match field.raw_arguments.get(argument_name)? {
        RawArgumentValue::Variable { name, value } => {
            let variable_type = document
                .variable_definitions
                .get(name)
                .map(|definition| definition.type_display.clone())?;
            Some((name.clone(), variable_type, value.clone()?))
        }
        _ => None,
    }
}

fn media_invalid_global_id_error(
    document: &ParsedDocument,
    field: &RootFieldSelection,
    argument_name: &str,
) -> Option<Value> {
    let (variable_name, variable_type, value) =
        media_variable_binding(document, field, argument_name)?;
    let id = match &value {
        ResolvedValue::String(raw) => raw.clone(),
        ResolvedValue::Null => String::new(),
        _ => return None,
    };
    if id.starts_with("gid://") {
        return None;
    }
    let explanation = format!("Invalid global id '{id}'");
    Some(json!({
        "message": format!(
            "Variable ${variable_name} of type {variable_type} was provided invalid value"
        ),
        "extensions": {
            "code": "INVALID_VARIABLE",
            "value": resolved_value_json(&value),
            "problems": [{
                "path": [],
                "explanation": explanation,
                "message": explanation,
            }]
        }
    }))
}

fn media_content_type_enum_error(
    document: &ParsedDocument,
    field: &RootFieldSelection,
    argument_name: &str,
) -> Option<Value> {
    const ALLOWED: [&str; 4] = ["VIDEO", "EXTERNAL_VIDEO", "MODEL_3D", "IMAGE"];
    let (variable_name, variable_type, value) =
        media_variable_binding(document, field, argument_name)?;
    let ResolvedValue::List(items) = &value else {
        return None;
    };
    for (index, item) in items.iter().enumerate() {
        let ResolvedValue::Object(fields) = item else {
            continue;
        };
        let Some(ResolvedValue::String(content_type)) = fields.get("mediaContentType") else {
            continue;
        };
        if ALLOWED.contains(&content_type.as_str()) {
            continue;
        }
        let explanation = format!(
            "Expected \"{content_type}\" to be one of: VIDEO, EXTERNAL_VIDEO, MODEL_3D, IMAGE"
        );
        return Some(json!({
            "message": format!(
                "Variable ${variable_name} of type {variable_type} was provided invalid value for {index}.mediaContentType ({explanation})"
            ),
            "extensions": {
                "code": "INVALID_VARIABLE",
                "value": resolved_value_json(&value),
                "problems": [{
                    "path": [index, "mediaContentType"],
                    "explanation": explanation,
                }]
            }
        }));
    }
    None
}

/// Valid values for the `MetaobjectCustomerAccountAccess` enum.
const METAOBJECT_CUSTOMER_ACCOUNT_ACCESS_VALUES: [&str; 3] = ["NONE", "READ", "READ_WRITE"];

/// `metaobjectDefinition{Create,Update}` reject an out-of-set `access.customerAccount`
/// enum literal at the GraphQL layer (before any local routing), reporting an
/// `argumentLiteralsIncompatible` error anchored at the `access:` value literal. The
/// declarative input schema does not model the definition input object, so this inline
/// enum check is expressed directly against the raw arguments.
fn metaobject_access_invalid_enum_errors(query: &str, document: &ParsedDocument) -> Vec<Value> {
    let mut errors = Vec::new();
    for field in &document.root_fields {
        if !matches!(
            field.name.as_str(),
            "metaobjectDefinitionCreate" | "metaobjectDefinitionUpdate"
        ) {
            continue;
        }
        let Some(RawArgumentValue::Object(definition)) = field.raw_arguments.get("definition")
        else {
            continue;
        };
        let Some(RawArgumentValue::Object(access)) = definition.get("access") else {
            continue;
        };
        let provided = match access.get("customerAccount") {
            Some(RawArgumentValue::Enum(value)) | Some(RawArgumentValue::String(value)) => {
                value.clone()
            }
            _ => continue,
        };
        if METAOBJECT_CUSTOMER_ACCOUNT_ACCESS_VALUES.contains(&provided.as_str()) {
            continue;
        }
        let location =
            inline_argument_value_location(query, field, "access").unwrap_or(field.location);
        errors.push(json!({
            "message": format!(
                "Argument 'customerAccount' on InputObject 'MetaobjectAccessInput' has an invalid value ({provided}). Expected type 'MetaobjectCustomerAccountAccess'."
            ),
            "locations": [{ "line": location.line, "column": location.column }],
            "path": [
                document.operation_path.clone(),
                field.response_key.clone(),
                "definition".to_string(),
                "access".to_string(),
                "customerAccount".to_string(),
            ],
            "extensions": {
                "code": "argumentLiteralsIncompatible",
                "typeName": "InputObject",
                "argumentName": "customerAccount"
            }
        }));
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
    if type_ref.named_type == "ID" {
        if let RawArgumentValue::String(s) = value {
            if s.trim().is_empty() {
                return vec![blank_id_argument_literal_error(
                    field,
                    argument_name,
                    context,
                )];
            }
        }
    }
    match value {
        RawArgumentValue::Null if type_ref.non_null => {
            return vec![non_null_argument_literal_error(
                field,
                argument_name,
                type_ref,
                context,
            )];
        }
        RawArgumentValue::Variable { name, value } if type_ref.non_null => {
            if matches!(value.as_ref(), None | Some(ResolvedValue::Null)) {
                let (variable_type, location) = resolve_variable_definition_type(
                    document,
                    name,
                    &type_ref.display,
                    field.location,
                );
                return vec![non_null_variable_null_error(name, &variable_type, location)];
            }
        }
        _ => {}
    }
    let leaf_errors = validate_argument_leaf_value(
        argument_name,
        type_ref,
        value,
        field,
        document,
        schema,
        context,
    );
    if !leaf_errors.is_empty() {
        return leaf_errors;
    }
    let Some(input_object) = schema.input_objects.get(&type_ref.named_type) else {
        return Vec::new();
    };
    match value {
        RawArgumentValue::Object(fields) => validate_input_object(
            &type_ref.named_type,
            input_object,
            InputObjectFields::Raw(fields),
            &[json!(argument_name)],
            schema,
            InputObjectMode::Raw {
                context,
                location: Some(inline_argument_location(
                    context.query,
                    field,
                    argument_name,
                )),
            },
        ),
        RawArgumentValue::List(items) if type_ref_is_list(type_ref) => {
            let mut errors = Vec::new();
            for (index, item) in items.iter().enumerate() {
                let path = vec![json!(argument_name), json!(index)];
                match item {
                    RawArgumentValue::Object(fields) => {
                        let item_location = inline_argument_list_item_object_location(
                            context.query,
                            field,
                            argument_name,
                            index,
                        )
                        .unwrap_or_else(|| {
                            inline_argument_location(context.query, field, argument_name)
                        });
                        errors.extend(validate_input_object(
                            &type_ref.named_type,
                            input_object,
                            InputObjectFields::Raw(fields),
                            &path,
                            schema,
                            InputObjectMode::Raw {
                                context,
                                location: Some(item_location),
                            },
                        ));
                    }
                    RawArgumentValue::Null if type_ref_has_non_null_list_items(type_ref) => errors
                        .push(non_null_argument_literal_error(
                            field,
                            argument_name,
                            type_ref,
                            context,
                        )),
                    _ => {}
                }
            }
            errors
        }
        RawArgumentValue::Variable { name, value } => {
            let (variable_type, location) =
                resolve_variable_definition_type(document, name, &type_ref.display, field.location);
            if type_ref_is_list(type_ref) {
                let Some(ResolvedValue::List(items)) = value.as_ref() else {
                    return Vec::new();
                };
                let mut problems = Vec::new();
                for (index, item) in items.iter().enumerate() {
                    let item_path = vec![json!(index)];
                    match item {
                        ResolvedValue::Object(fields) => {
                            problems.extend(validate_input_object(
                                &type_ref.named_type,
                                input_object,
                                InputObjectFields::Resolved(fields),
                                &item_path,
                                schema,
                                InputObjectMode::Resolved {
                                    order_source: context.raw_body,
                                },
                            ));
                        }
                        ResolvedValue::Null if type_ref_has_non_null_list_items(type_ref) => {
                            problems.push(variable_problem_value_path(
                                &item_path,
                                "Expected value to not be null",
                            ));
                        }
                        _ => {}
                    }
                }
                if problems.is_empty() {
                    return Vec::new();
                }
                return vec![invalid_variable_error(
                    VariableValidationContext {
                        variable_name: name,
                        variable_type: &variable_type,
                        location,
                    },
                    &ResolvedValue::List(items.clone()),
                    problems,
                )];
            }
            let Some(ResolvedValue::Object(fields)) = value.as_ref() else {
                return Vec::new();
            };
            let variable_context = VariableValidationContext {
                variable_name: name,
                variable_type: &variable_type,
                location,
            };
            let problems = validate_input_object(
                &type_ref.named_type,
                input_object,
                InputObjectFields::Resolved(fields),
                &[],
                schema,
                InputObjectMode::Resolved {
                    order_source: context.raw_body,
                },
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
        _ => Vec::new(),
    }
}

fn validate_argument_leaf_value(
    argument_name: &str,
    type_ref: &SchemaTypeRef,
    value: &RawArgumentValue,
    field: &RootFieldSelection,
    document: &ParsedDocument,
    schema: &AdminInputSchema,
    context: ValidationContext<'_>,
) -> Vec<Value> {
    match value {
        RawArgumentValue::Float(_) => int_literal_coercion_value(value, type_ref)
            .map(|invalid_value| {
                root_argument_literal_incompatible_error(
                    field,
                    argument_name,
                    &invalid_value,
                    &type_ref.display,
                    context,
                )
            })
            .into_iter()
            .collect(),
        RawArgumentValue::Enum(_) | RawArgumentValue::String(_) => {
            enum_literal_coercion_value(value, type_ref, schema)
                .map(|invalid_value| {
                    root_argument_literal_incompatible_error(
                        field,
                        argument_name,
                        &invalid_value,
                        &type_ref.display,
                        context,
                    )
                })
                .into_iter()
                .collect()
        }
        RawArgumentValue::Variable { name, value } => {
            let Some(value) = value.as_ref() else {
                return Vec::new();
            };
            let (variable_type, location) =
                resolve_variable_definition_type(document, name, &type_ref.display, field.location);
            let problems = validate_resolved_leaf_problems(value, type_ref, schema, &[]);
            if problems.is_empty() {
                Vec::new()
            } else {
                vec![invalid_variable_error_for_leaf(
                    VariableValidationContext {
                        variable_name: name,
                        variable_type: &variable_type,
                        location,
                    },
                    value,
                    problems,
                )]
            }
        }
        _ => Vec::new(),
    }
}

fn validate_resolved_leaf_problems(
    value: &ResolvedValue,
    type_ref: &SchemaTypeRef,
    schema: &AdminInputSchema,
    path: &[Value],
) -> Vec<Value> {
    if type_ref_is_list(type_ref) {
        let ResolvedValue::List(items) = value else {
            return Vec::new();
        };
        let mut problems = Vec::new();
        for (index, item) in items.iter().enumerate() {
            let mut item_path = path.to_vec();
            item_path.push(json!(index));
            if matches!(item, ResolvedValue::Null) && type_ref_has_non_null_list_items(type_ref) {
                problems.push(variable_problem_value_path(
                    &item_path,
                    "Expected value to not be null",
                ));
            } else if let Some(problem) = validate_resolved_scalar(item, type_ref, schema) {
                problems.push(variable_problem_from_scalar_problem(&item_path, problem));
            }
        }
        return problems;
    }

    validate_resolved_scalar(value, type_ref, schema)
        .map(|problem| variable_problem_from_scalar_problem(path, problem))
        .into_iter()
        .collect()
}

fn variable_problem_from_scalar_problem(path: &[Value], problem: ScalarValidationProblem) -> Value {
    if problem.include_message {
        variable_problem_with_message_value_path(path, &problem.explanation)
    } else {
        variable_problem_value_path(path, &problem.explanation)
    }
}

fn invalid_variable_error_for_leaf(
    context: VariableValidationContext<'_>,
    value: &ResolvedValue,
    problems: Vec<Value>,
) -> Value {
    if problems.iter().any(|problem| {
        problem
            .get("path")
            .and_then(Value::as_array)
            .is_some_and(|path| !path.is_empty())
    }) {
        invalid_variable_error(context, value, problems)
    } else {
        invalid_variable_error_envelope(
            format!(
                "Variable ${} of type {} was provided invalid value",
                context.variable_name, context.variable_type
            ),
            context.location,
            resolved_value_json(value),
            Value::Array(problems),
        )
    }
}

fn resolve_variable_definition_type(
    document: &ParsedDocument,
    variable_name: &str,
    fallback_type: &str,
    fallback_location: SourceLocation,
) -> (String, SourceLocation) {
    document
        .variable_definitions
        .get(variable_name)
        .map(|definition| (definition.type_display.clone(), definition.location))
        .unwrap_or_else(|| (fallback_type.to_string(), fallback_location))
}

fn is_unknown_input_field(
    input_object: &BTreeMap<String, SchemaInputField>,
    input_type_name: &str,
    field_name: &str,
) -> bool {
    !input_object.contains_key(field_name)
        && !local_extension_input_field(input_type_name, field_name)
}

#[derive(Clone, Copy)]
enum InputObjectFields<'a> {
    Raw(&'a BTreeMap<String, RawArgumentValue>),
    Resolved(&'a BTreeMap<String, ResolvedValue>),
}

#[derive(Clone, Copy)]
enum InputValueRef<'a> {
    Raw(&'a RawArgumentValue),
    Resolved(&'a ResolvedValue),
}

#[derive(Clone, Copy)]
enum InputObjectMode<'a> {
    Raw {
        context: ValidationContext<'a>,
        location: Option<SourceLocation>,
    },
    Resolved {
        order_source: &'a str,
    },
}

impl<'a> InputObjectFields<'a> {
    fn get(self, field_name: &str) -> Option<InputValueRef<'a>> {
        match self {
            Self::Raw(fields) => fields.get(field_name).map(InputValueRef::Raw),
            Self::Resolved(fields) => fields.get(field_name).map(InputValueRef::Resolved),
        }
    }
}

impl<'a> InputValueRef<'a> {
    fn is_null(self) -> bool {
        matches!(
            self,
            Self::Raw(RawArgumentValue::Null) | Self::Resolved(ResolvedValue::Null)
        )
    }

    fn object_fields(self) -> Option<InputObjectFields<'a>> {
        match self {
            Self::Raw(RawArgumentValue::Object(fields)) => Some(InputObjectFields::Raw(fields)),
            Self::Resolved(ResolvedValue::Object(fields)) => {
                Some(InputObjectFields::Resolved(fields))
            }
            _ => None,
        }
    }

    fn list_items(self) -> Option<Vec<InputValueRef<'a>>> {
        match self {
            Self::Raw(RawArgumentValue::List(items)) => {
                Some(items.iter().map(InputValueRef::Raw).collect())
            }
            Self::Resolved(ResolvedValue::List(items)) => {
                Some(items.iter().map(InputValueRef::Resolved).collect())
            }
            _ => None,
        }
    }
}

fn validate_input_object(
    input_type_name: &str,
    input_object: &BTreeMap<String, SchemaInputField>,
    fields: InputObjectFields<'_>,
    path: &[Value],
    schema: &AdminInputSchema,
    mode: InputObjectMode<'_>,
) -> Vec<Value> {
    let mut errors = Vec::new();
    let strict = schema.input_object_is_strict(input_type_name);
    let field_keys: Vec<&String> = match fields {
        InputObjectFields::Raw(fields) => fields.keys().collect(),
        InputObjectFields::Resolved(fields) => fields.keys().collect(),
    };
    let mut unknown_fields: Vec<&String> = field_keys
        .into_iter()
        .filter(|field_name| is_unknown_input_field(input_object, input_type_name, field_name))
        .collect();
    if strict {
        match mode {
            InputObjectMode::Raw { context, .. } => {
                let target_depth = 1 + path.len() as i32;
                unknown_fields.sort_by_key(|field_name| {
                    inline_input_field_name_location(
                        context.query,
                        context.field_location,
                        target_depth,
                        field_name,
                    )
                    .map(|location| (location.line, location.column))
                    .unwrap_or((usize::MAX, usize::MAX))
                });
            }
            InputObjectMode::Resolved { order_source } => {
                unknown_fields.sort_by_key(|field_name| key_order_index(order_source, field_name));
            }
        }
        for field_name in unknown_fields {
            match mode {
                InputObjectMode::Raw { context, .. } => {
                    errors.push(input_object_argument_not_accepted_error(
                        input_type_name,
                        field_name,
                        path,
                        context,
                    ))
                }
                InputObjectMode::Resolved { .. } => {
                    let mut nested_path = path.to_vec();
                    nested_path.push(json!(field_name));
                    errors.push(variable_problem_value_path(
                        &nested_path,
                        &format!("Field is not defined on {input_type_name}"),
                    ));
                }
            }
        }
    }

    if strict && matches!(mode, InputObjectMode::Raw { .. }) {
        for (field_name, field_schema) in input_object {
            if field_schema.type_ref.non_null
                && !field_schema.has_default
                && fields.get(field_name).is_none()
            {
                if let InputObjectMode::Raw { context, location } = mode {
                    errors.push(missing_required_input_object_attribute_error(
                        input_type_name,
                        field_name,
                        &field_schema.type_ref,
                        path,
                        context,
                        location.unwrap_or(context.field_location),
                    ));
                }
            }
        }
    }

    let value_field_names: Vec<&String> = match fields {
        InputObjectFields::Raw(fields) => fields.keys().collect(),
        InputObjectFields::Resolved(fields) if !strict => fields.keys().collect(),
        InputObjectFields::Resolved(_) => input_object.keys().collect(),
    };
    for field_name in value_field_names {
        let Some(field_schema) = input_object.get(field_name) else {
            continue;
        };
        let provided = fields.get(field_name);
        if matches!(mode, InputObjectMode::Resolved { .. })
            && field_schema.type_ref.non_null
            && !field_schema.has_default
            && strict
            && provided.is_none_or(|value| value.is_null())
        {
            let mut nested_path = path.to_vec();
            nested_path.push(json!(field_name));
            errors.push(variable_problem_value_path(
                &nested_path,
                "Expected value to not be null",
            ));
            continue;
        }
        let Some(field_value) = provided else {
            continue;
        };
        match (mode, field_value) {
            (InputObjectMode::Raw { context, location }, InputValueRef::Raw(value)) => {
                let location = location.unwrap_or(context.field_location);
                if matches!(value, RawArgumentValue::Null) && field_schema.type_ref.non_null {
                    errors.push(argument_literal_incompatible_error(
                        input_type_name,
                        field_name,
                        "null",
                        &field_schema.type_ref.display,
                        path,
                        context,
                        location,
                    ));
                    continue;
                }
                if let Some(invalid_value) =
                    int_literal_coercion_value(value, &field_schema.type_ref)
                {
                    errors.push(argument_literal_incompatible_error(
                        input_type_name,
                        field_name,
                        &invalid_value,
                        &field_schema.type_ref.display,
                        path,
                        context,
                        location,
                    ));
                }
                if let Some(invalid_value) =
                    enum_literal_coercion_value(value, &field_schema.type_ref, schema)
                {
                    errors.push(argument_literal_incompatible_error(
                        input_type_name,
                        field_name,
                        &invalid_value,
                        &field_schema.type_ref.display,
                        path,
                        context,
                        location,
                    ));
                }
            }
            (InputObjectMode::Resolved { .. }, InputValueRef::Resolved(value)) => {
                if let Some(problem) =
                    validate_resolved_scalar(value, &field_schema.type_ref, schema)
                {
                    let mut nested_path = path.to_vec();
                    nested_path.push(json!(field_name));
                    errors.push(if problem.include_message {
                        variable_problem_with_message_value_path(&nested_path, &problem.explanation)
                    } else {
                        variable_problem_value_path(&nested_path, &problem.explanation)
                    });
                }
            }
            _ => {}
        }

        let Some(nested_input_object) = schema.input_objects.get(&field_schema.type_ref.named_type)
        else {
            continue;
        };
        if let Some(nested_fields) = field_value.object_fields() {
            let mut nested_path = path.to_vec();
            nested_path.push(json!(field_name));
            let nested_mode = match mode {
                InputObjectMode::Raw { context, .. } => InputObjectMode::Raw {
                    context,
                    location: inline_input_field_value_location(
                        context.query,
                        context.field_location,
                        1 + path.len() as i32,
                        field_name,
                    ),
                },
                InputObjectMode::Resolved { .. } => mode,
            };
            errors.extend(validate_input_object(
                &field_schema.type_ref.named_type,
                nested_input_object,
                nested_fields,
                &nested_path,
                schema,
                nested_mode,
            ));
            continue;
        }
        if !type_ref_is_list(&field_schema.type_ref) {
            continue;
        }
        let Some(items) = field_value.list_items() else {
            continue;
        };
        for (index, item) in items.into_iter().enumerate() {
            let mut nested_path = path.to_vec();
            nested_path.push(json!(field_name));
            nested_path.push(json!(index));
            if let Some(nested_fields) = item.object_fields() {
                errors.extend(validate_input_object(
                    &field_schema.type_ref.named_type,
                    nested_input_object,
                    nested_fields,
                    &nested_path,
                    schema,
                    mode,
                ));
            } else if matches!(mode, InputObjectMode::Resolved { .. })
                && item.is_null()
                && type_ref_has_non_null_list_items(&field_schema.type_ref)
            {
                errors.push(variable_problem_value_path(
                    &nested_path,
                    "Expected value to not be null",
                ));
            }
        }
    }

    errors
}

struct ScalarValidationProblem {
    explanation: String,
    include_message: bool,
}

fn root_argument_not_accepted_error(
    field: &RootFieldSelection,
    argument_name: &str,
    context: ValidationContext<'_>,
) -> Value {
    let location = inline_argument_name_location(context.query, field, argument_name)
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

fn required_root_argument_error(
    field: &RootFieldSelection,
    argument_name: &str,
    _type_ref: &SchemaTypeRef,
    context: ValidationContext<'_>,
) -> Value {
    json!({
        "message": format!("Field '{}' is missing required arguments: {}", field.name, argument_name),
        "locations": [{ "line": context.field_location.line, "column": context.field_location.column }],
        "path": [context.operation_path, context.response_key],
        "extensions": {
            "code": "missingRequiredArguments",
            "className": "Field",
            "name": field.name,
            "arguments": argument_name
        }
    })
}

fn blank_id_argument_literal_error(
    _field: &RootFieldSelection,
    argument_name: &str,
    context: ValidationContext<'_>,
) -> Value {
    json!({
        "message": "Invalid global id ''",
        "locations": [{ "line": context.field_location.line, "column": context.field_location.column }],
        "path": [context.operation_path, context.response_key, argument_name],
        "extensions": {
            "code": "argumentLiteralsIncompatible",
            "typeName": "CoercionError"
        }
    })
}

fn non_null_argument_literal_error(
    field: &RootFieldSelection,
    argument_name: &str,
    type_ref: &SchemaTypeRef,
    context: ValidationContext<'_>,
) -> Value {
    json!({
        "message": format!(
            "Argument '{}' on Field '{}' has an invalid value (null). Expected type '{}'.",
            argument_name, field.name, type_ref.display
        ),
        "locations": [{ "line": context.field_location.line, "column": context.field_location.column }],
        "path": [context.operation_path, context.response_key, argument_name],
        "extensions": {
            "code": "argumentLiteralsIncompatible",
            "typeName": "Field",
            "argumentName": argument_name
        }
    })
}

fn root_argument_literal_incompatible_error(
    field: &RootFieldSelection,
    argument_name: &str,
    invalid_value: &str,
    expected_type: &str,
    context: ValidationContext<'_>,
) -> Value {
    json!({
        "message": format!(
            "Argument '{}' on Field '{}' has an invalid value ({}). Expected type '{}'.",
            argument_name, field.name, invalid_value, expected_type
        ),
        "locations": [{ "line": context.field_location.line, "column": context.field_location.column }],
        "path": [context.operation_path, context.response_key, argument_name],
        "extensions": {
            "code": "argumentLiteralsIncompatible",
            "typeName": "Field",
            "argumentName": argument_name
        }
    })
}

fn non_null_variable_null_error(
    variable_name: &str,
    variable_type: &str,
    location: SourceLocation,
) -> Value {
    invalid_variable_error_envelope(
        format!("Variable ${variable_name} of type {variable_type} was provided invalid value"),
        location,
        Value::Null,
        json!([{
                "path": [],
                "explanation": "Expected value to not be null"
        }]),
    )
}

fn argument_literal_incompatible_error(
    input_type_name: &str,
    argument_name: &str,
    invalid_value: &str,
    expected_type: &str,
    path: &[Value],
    context: ValidationContext<'_>,
    location: SourceLocation,
) -> Value {
    json!({
        "message": format!(
            "Argument '{argument_name}' on InputObject '{input_type_name}' has an invalid value ({invalid_value}). Expected type '{expected_type}'."
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

fn int_literal_coercion_value(
    value: &RawArgumentValue,
    type_ref: &SchemaTypeRef,
) -> Option<String> {
    if type_ref.named_type != "Int" {
        return None;
    }
    match value {
        RawArgumentValue::Float(raw) => Some(format!("{raw}")),
        _ => None,
    }
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
        "locations": [{ "line": location.line, "column": location.column }],
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
    path: &[Value],
    context: ValidationContext<'_>,
    location: SourceLocation,
) -> Value {
    json!({
        "message": format!(
            "Argument '{argument_name}' on InputObject '{input_type_name}' is required. Expected type {}",
            type_ref.display
        ),
        "locations": [{ "line": location.line, "column": location.column }],
        "path": input_error_path(context, path, argument_name),
        "extensions": {
            "code": "missingRequiredInputObjectAttribute",
            "argumentName": argument_name,
            "argumentType": type_ref.display,
            "inputObjectType": input_type_name
        }
    })
}

fn inline_argument_name_location(
    query: &str,
    field: &RootFieldSelection,
    argument_name: &str,
) -> Option<SourceLocation> {
    inline_input_field_name_location(query, field.location, 1, argument_name)
}

fn inline_argument_location(
    query: &str,
    field: &RootFieldSelection,
    argument_name: &str,
) -> SourceLocation {
    inline_argument_value_location(query, field, argument_name).unwrap_or(field.location)
}

fn inline_input_field_name_location(
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

fn inline_argument_list_item_object_location(
    query: &str,
    field: &RootFieldSelection,
    argument_name: &str,
    target_index: usize,
) -> Option<SourceLocation> {
    let start = byte_offset_for_location(query, field.location)?;
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

/// Locates the *value* of an input-object field nested at `target_depth` (the column of the
/// first non-whitespace character after its `name:`). Used to anchor a `missingRequiredInput
/// ObjectAttribute` error inside a nested input object at that object literal's opening token
/// — e.g. a `MoneyInput` supplied as `discount: { fixedValue: { amount: "5.00" } }` reports the
/// missing `currencyCode` at the `{` of the `fixedValue` value, not at the enclosing field.
fn inline_input_field_value_location(
    query: &str,
    field_location: SourceLocation,
    target_depth: i32,
    name: &str,
) -> Option<SourceLocation> {
    let name_location =
        inline_input_field_name_location(query, field_location, target_depth, name)?;
    let start = byte_offset_for_location(query, name_location)?;
    let after_name = start + name.len();
    source_location_for_byte_offset(query, value_offset_after(query, after_name)?)
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

fn graphql_variable_occurrence(
    query: &str,
    variable_name: &str,
    search_from: usize,
) -> Option<(usize, usize)> {
    let needle = format!("${variable_name}");
    let bytes = query.as_bytes();
    let mut search_from = search_from;
    while let Some(relative) = query[search_from..].find(&needle) {
        let start = search_from + relative;
        let after = start + needle.len();
        let is_boundary = match bytes.get(after) {
            None => true,
            Some(next) => !(next.is_ascii_alphanumeric() || *next == b'_'),
        };
        if is_boundary {
            return Some((start, after));
        }
        search_from = after;
    }
    None
}

/// Resolves the 1-based location of a variable definition (`$name`) in the query.
pub(in crate::proxy) fn graphql_variable_definition_location(
    query: &str,
    variable_name: &str,
) -> Option<(usize, usize)> {
    let (start, _) = graphql_variable_occurrence(query, variable_name, 0)?;
    let location = source_location_for_byte_offset(query, start)?;
    Some((location.line, location.column))
}

/// Resolves the declared GraphQL type of a variable definition (`$name: <TYPE>`).
pub(in crate::proxy) fn graphql_variable_definition_type(
    query: &str,
    variable_name: &str,
) -> Option<String> {
    let mut search_from = 0;
    while let Some((_, after)) = graphql_variable_occurrence(query, variable_name, search_from) {
        if let Some(type_part) = query[after..].trim_start().strip_prefix(':') {
            let declared: String = type_part
                .trim_start()
                .chars()
                .take_while(|c| !matches!(c, ',' | ')' | '=' | '\n' | '\r' | '{'))
                .collect();
            let declared = declared.trim();
            if !declared.is_empty() {
                return Some(declared.to_string());
            }
        }
        search_from = after;
    }
    None
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

fn local_extension_input_field(input_type_name: &str, field_name: &str) -> bool {
    matches!(
        (input_type_name, field_name),
        ("GiftCardCreateInput", "notify")
    )
}

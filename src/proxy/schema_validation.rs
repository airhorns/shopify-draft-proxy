use super::*;

use crate::graphql::ParsedDocument;
use graphql_parser::query::parse_query;
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

#[derive(Debug, Clone)]
struct OutputFieldType {
    named_type: String,
}

#[derive(Debug, Clone, Default)]
struct AdminOutputSchema {
    query_root_fields: BTreeMap<String, OutputFieldType>,
    fields_by_parent: BTreeMap<String, BTreeMap<String, OutputFieldType>>,
}

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

pub(in crate::proxy) fn user_error(
    field: impl Into<UserErrorField>,
    message: &str,
    code: Option<&str>,
) -> Value {
    json!({
        "field": user_error_field(field),
        "message": message,
        "code": user_error_code(code),
    })
}

#[derive(Debug, Clone, Copy)]
pub(in crate::proxy) enum LengthUserErrorBound {
    TooLong {
        maximum: usize,
    },
    #[allow(dead_code, reason = "TOO_SHORT supports follow-up migrations.")]
    TooShort {
        minimum: usize,
    },
}

pub(in crate::proxy) fn presence_user_error(
    field: impl Into<UserErrorField>,
    field_name: &str,
) -> Value {
    user_error(
        field,
        &format!("{field_name} can't be blank"),
        Some("BLANK"),
    )
}

pub(in crate::proxy) fn length_user_error(
    field: impl Into<UserErrorField>,
    field_name: &str,
    bound: LengthUserErrorBound,
) -> Value {
    let (message, code) = match bound {
        LengthUserErrorBound::TooLong { maximum } => (
            format!("{field_name} is too long (maximum is {maximum} characters)"),
            "TOO_LONG",
        ),
        LengthUserErrorBound::TooShort { minimum } => (
            format!("{field_name} is too short (minimum is {minimum} characters)"),
            "TOO_SHORT",
        ),
    };
    user_error(field, &message, Some(code))
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
    json!({
        "__typename": typename,
        "field": user_error_field(field),
        "message": message,
        "code": user_error_code(code),
    })
}

pub(in crate::proxy) fn user_error_typed_with_code_value(
    typename: &str,
    field: impl Into<UserErrorField>,
    message: &str,
    code: Value,
) -> Value {
    json!({
        "__typename": typename,
        "field": user_error_field(field),
        "message": message,
        "code": code,
    })
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
    json!({
        "field": user_error_field(field),
        "message": message,
        "code": user_error_code(code),
        "extraInfo": extra_info,
    })
}

pub(in crate::proxy) fn user_error_with_element_index(
    field: impl Into<UserErrorField>,
    message: &str,
    code: Option<&str>,
    element_index: Value,
) -> Value {
    json!({
        "field": user_error_field(field),
        "message": message,
        "code": user_error_code(code),
        "elementIndex": element_index,
    })
}

pub(in crate::proxy) fn metaobject_indexed_user_error(
    field: impl Into<UserErrorField>,
    message: &str,
    code: Option<&str>,
    element_key: Value,
    element_index: Value,
) -> Value {
    json!({
        "field": user_error_field(field),
        "message": message,
        "code": user_error_code(code),
        "elementKey": element_key,
        "elementIndex": element_index,
    })
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
) -> Vec<Value> {
    let Some(document) = parsed_document(query, variables) else {
        return Vec::new();
    };
    let mut errors = admin_platform_node_global_id_errors(query, raw_body, &document);
    if document.operation_type != OperationType::Mutation {
        return errors;
    }
    let schema = public_admin_input_schema();
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
    errors.extend(product_media_variable_errors(&document));
    errors.extend(metaobject_access_invalid_enum_errors(query, &document));
    errors
}

pub(in crate::proxy) fn public_admin_graphql_validation_response(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
    api_version: Option<&str>,
) -> Option<Response> {
    if api_version != Some("2025-01") {
        return None;
    }

    if parse_query::<&str>(query).is_err() {
        return Some(ok_json(json!({
            "errors": [parse_error(query)]
        })));
    }

    let document = parsed_document(query, variables)?;
    let mut errors = missing_required_variable_errors(&document, variables);
    errors.extend(undefined_root_field_errors(&document));
    errors.extend(selection_mismatch_errors(&document));
    errors.extend(undefined_product_selection_field_errors(&document));
    if !errors.is_empty() {
        return Some(ok_json(json!({ "errors": errors })));
    }

    product_create_argument_arity_response(&document)
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

fn undefined_root_field_errors(document: &ParsedDocument) -> Vec<Value> {
    document
        .root_fields
        .iter()
        .filter_map(|field| {
            let parent_type = match document.operation_type {
                OperationType::Query => {
                    (!public_admin_output_schema()
                        .query_root_fields
                        .contains_key(&field.name))
                        && !local_implemented_query_root_names().contains(&field.name)
                }
                .then_some("QueryRoot"),
                OperationType::Mutation => {
                    (!public_admin_mutation_root_names().contains(&field.name))
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

fn selection_mismatch_errors(document: &ParsedDocument) -> Vec<Value> {
    if document.operation_type != OperationType::Query {
        return Vec::new();
    }
    document
        .root_fields
        .iter()
        .filter(|field| field.selection.is_empty())
        .filter_map(|field| {
            let output_type = public_admin_output_schema()
                .query_root_fields
                .get(&field.name)?;
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

fn undefined_product_selection_field_errors(document: &ParsedDocument) -> Vec<Value> {
    if document.operation_type != OperationType::Query {
        return Vec::new();
    }
    let mut errors = Vec::new();
    for field in &document.root_fields {
        if field.name != "products" {
            continue;
        }
        collect_undefined_selection_field_errors(
            document,
            "ProductConnection",
            &field.selection,
            vec![json!(document.operation_path), json!(field.response_key)],
            &mut errors,
        );
    }
    errors
}

fn collect_undefined_selection_field_errors(
    document: &ParsedDocument,
    parent_type: &str,
    selections: &[SelectedField],
    path: Vec<Value>,
    errors: &mut Vec<Value>,
) {
    let schema_fields = public_admin_output_schema()
        .fields_by_parent
        .get(parent_type);
    for selection in selections {
        let mut child_path = path.clone();
        child_path.push(json!(selection.response_key));
        if let Some(output_type) = schema_fields.and_then(|fields| fields.get(&selection.name)) {
            collect_undefined_selection_field_errors(
                document,
                &output_type.named_type,
                &selection.selection,
                child_path,
                errors,
            );
        } else if !common_scalar_field_name(&selection.name) {
            errors.push(undefined_field_error(
                document,
                selection.location,
                parent_type,
                &selection.name,
                child_path,
            ));
        }
    }
}

fn common_scalar_field_name(field_name: &str) -> bool {
    matches!(
        field_name,
        "__typename"
            | "id"
            | "legacyResourceId"
            | "title"
            | "handle"
            | "status"
            | "createdAt"
            | "updatedAt"
            | "description"
            | "descriptionHtml"
            | "vendor"
            | "productType"
            | "tags"
            | "totalInventory"
            | "tracksInventory"
    )
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

fn product_create_argument_arity_response(document: &ParsedDocument) -> Option<Response> {
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
        }],
        "extensions": {
            "cost": {
                "requestedQueryCost": 10,
                "actualQueryCost": 10,
                "throttleStatus": {
                    "maximumAvailable": 2000,
                    "currentlyAvailable": 1990,
                    "restoreRate": 100
                }
            }
        }
    })))
}

fn public_admin_mutation_root_names() -> &'static BTreeSet<String> {
    static MUTATION_ROOT_NAMES: OnceLock<BTreeSet<String>> = OnceLock::new();
    MUTATION_ROOT_NAMES.get_or_init(|| {
        let parsed: Value = serde_json::from_str(include_str!(
            "../../config/admin-graphql-mutation-schema.json"
        ))
        .expect("checked-in Admin GraphQL mutation schema should be valid JSON");
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

fn public_admin_output_schema() -> &'static AdminOutputSchema {
    static OUTPUT_SCHEMA: OnceLock<AdminOutputSchema> = OnceLock::new();
    OUTPUT_SCHEMA.get_or_init(|| {
        let parsed: Value = serde_json::from_str(include_str!(
            "../../config/admin-graphql-bulk-query-schema.json"
        ))
        .expect("checked-in Admin GraphQL output schema should be valid JSON");
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
            schema
                .fields_by_parent
                .entry(parent_type.to_string())
                .or_default()
                .insert(name.to_string(), output_type);
        }
        schema
    })
}

fn output_field_type(field: &Value) -> Option<OutputFieldType> {
    let kind = field.get("kind")?;
    let named_type = match kind.get("type").and_then(Value::as_str)? {
        "object" => kind.get("typeName").and_then(Value::as_str)?.to_string(),
        "connection" => {
            let node_type = kind.get("nodeType").and_then(Value::as_str)?;
            format!("{node_type}Connection")
        }
        "list" => kind.get("elementType").and_then(Value::as_str)?.to_string(),
        _ => return None,
    };
    Some(OutputFieldType { named_type })
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
    Some(json!({
        "message": message,
        "locations": [{
            "line": variable_definition.location.line,
            "column": variable_definition.location.column,
        }],
        "extensions": {
            "code": "INVALID_VARIABLE",
            "value": resolved_value_json(variable_value),
            "problems": [problem],
        },
    }))
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
    // Check for blank literal ID values regardless of type lookup
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
    // Non-null coercion violations apply to *any* non-null argument, regardless
    // of whether its named type is a registered input object. A null literal or
    // an unbound/null variable supplied for a non-null argument fails coercion
    // before the resolver runs (e.g. `customerCreate(input: null)` or an unbound
    // `$id: ID!`). These checks must run even when the named type is a scalar
    // (ID) or an input object we intentionally leave unregistered.
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
                return vec![non_null_variable_null_error(name, variable_type, location)];
            }
        }
        _ => {}
    }
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
            inline_argument_value_location(context.query, field, argument_name),
        ),
        RawArgumentValue::List(items) if type_ref_is_list(type_ref) => {
            let mut errors = Vec::new();
            for (index, item) in items.iter().enumerate() {
                let path = vec![argument_name.to_string(), index.to_string()];
                match item {
                    RawArgumentValue::Object(fields) => {
                        errors.extend(validate_raw_input_object(
                            &type_ref.named_type,
                            input_object,
                            fields,
                            &path,
                            schema,
                            context,
                            inline_argument_value_location(context.query, field, argument_name),
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
            // A required (non-null) argument supplied a null or absent variable
            // fails coercion at the variable definition. Shopify reports this as
            // an INVALID_VARIABLE "Expected value to not be null" problem rather
            // than a missing-argument error.
            if type_ref.non_null && matches!(value.as_ref(), None | Some(ResolvedValue::Null)) {
                return vec![non_null_variable_null_error(name, variable_type, location)];
            }
            if type_ref_is_list(type_ref) {
                let Some(ResolvedValue::List(items)) = value.as_ref() else {
                    return Vec::new();
                };
                let mut problems = Vec::new();
                for (index, item) in items.iter().enumerate() {
                    let item_path = vec![json!(index)];
                    match item {
                        ResolvedValue::Object(fields) => {
                            problems.extend(validate_resolved_input_object(
                                &type_ref.named_type,
                                input_object,
                                fields,
                                &item_path,
                                schema,
                                context.raw_body,
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
                        variable_type,
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
                variable_type,
                location,
            };
            let problems = validate_resolved_input_object(
                &type_ref.named_type,
                input_object,
                fields,
                &[],
                schema,
                context.raw_body,
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
        RawArgumentValue::Null if type_ref.non_null => vec![non_null_argument_literal_error(
            field,
            argument_name,
            type_ref,
            context,
        )],
        RawArgumentValue::String(s) if type_ref.named_type == "ID" && s.trim().is_empty() => {
            vec![blank_id_argument_literal_error(
                field,
                argument_name,
                context,
            )]
        }
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
    location: Option<SourceLocation>,
) -> Vec<Value> {
    let mut errors = Vec::new();
    // Unknown-field rejections are reported in the order the fields appear in the
    // input-object *literal*, not the sorted map order serde/BTreeMap leaves us
    // with. Recover document order from each field-name token's location.
    let target_depth = 1 + path.len() as i32;
    let mut unknown_fields: Vec<&String> = fields
        .keys()
        .filter(|field_name| {
            !input_object.contains_key(*field_name)
                && !local_extension_input_field(input_type_name, field_name)
        })
        .collect();
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
    for field_name in unknown_fields {
        errors.push(input_object_argument_not_accepted_error(
            input_type_name,
            field_name,
            path,
            context,
        ));
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
                location.unwrap_or(context.field_location),
            ));
        }
    }
    for (field_name, field_value) in fields {
        let Some(field_schema) = input_object.get(field_name) else {
            continue;
        };
        // Scalar coercion: an Int field given a float literal fails coercion.
        // Shopify anchors the argumentLiteralsIncompatible error at the enclosing
        // argument value (the input-object literal), with the full path to the
        // offending field.
        if let Some(invalid_value) = int_literal_coercion_value(field_value, &field_schema.type_ref)
        {
            errors.push(argument_literal_incompatible_error(
                input_type_name,
                field_name,
                &invalid_value,
                &field_schema.type_ref.display,
                path,
                context,
                location.unwrap_or(context.field_location),
            ));
        }
        if let Some(invalid_value) =
            enum_literal_coercion_value(field_value, &field_schema.type_ref)
        {
            errors.push(argument_literal_incompatible_error(
                input_type_name,
                field_name,
                &invalid_value,
                &field_schema.type_ref.display,
                path,
                context,
                location.unwrap_or(context.field_location),
            ));
        }
        let Some(nested_input_object) = schema.input_objects.get(&field_schema.type_ref.named_type)
        else {
            continue;
        };
        match field_value {
            RawArgumentValue::Object(nested_fields) => {
                let mut nested_path = path.to_vec();
                nested_path.push(field_name.clone());
                // Anchor errors inside the nested object at that object literal's value
                // (the `{` after `field_name:`), so a missing required attribute reports
                // its own column rather than falling back to the enclosing field.
                let nested_location = inline_input_field_value_location(
                    context.query,
                    context.field_location,
                    target_depth,
                    field_name,
                );
                errors.extend(validate_raw_input_object(
                    &field_schema.type_ref.named_type,
                    nested_input_object,
                    nested_fields,
                    &nested_path,
                    schema,
                    context,
                    nested_location,
                ));
            }
            RawArgumentValue::List(items) if type_ref_is_list(&field_schema.type_ref) => {
                for (index, item) in items.iter().enumerate() {
                    let RawArgumentValue::Object(nested_fields) = item else {
                        continue;
                    };
                    let mut nested_path = path.to_vec();
                    nested_path.push(field_name.clone());
                    nested_path.push(index.to_string());
                    errors.extend(validate_raw_input_object(
                        &field_schema.type_ref.named_type,
                        nested_input_object,
                        nested_fields,
                        &nested_path,
                        schema,
                        context,
                        location,
                    ));
                }
            }
            _ => {}
        }
    }
    errors
}

fn validate_resolved_input_object(
    input_type_name: &str,
    input_object: &BTreeMap<String, SchemaInputField>,
    fields: &BTreeMap<String, ResolvedValue>,
    problem_path: &[Value],
    schema: &AdminInputSchema,
    order_source: &str,
) -> Vec<Value> {
    let mut problems = Vec::new();
    // Report unknown-field coercion problems in the order the fields appear in
    // the request body, not the sorted map order serde/BTreeMap leaves us with.
    let mut unknown_fields: Vec<&String> = fields
        .keys()
        .filter(|field_name| {
            !input_object.contains_key(*field_name)
                && !local_extension_input_field(input_type_name, field_name)
        })
        .collect();
    unknown_fields.sort_by_key(|field_name| key_order_index(order_source, field_name));
    for field_name in unknown_fields {
        let mut nested_path = problem_path.to_vec();
        nested_path.push(json!(field_name));
        problems.push(variable_problem_value_path(
            &nested_path,
            &format!("Field is not defined on {input_type_name}"),
        ));
    }
    // Coerce each schema field in a single pass (BTreeMap key order). Shopify's
    // GraphQL coercion reports problems in the order it walks the input object's
    // fields, interleaving "missing required" with "invalid scalar" rather than
    // emitting all of one kind before the other. Walking the schema fields once
    // — non-null check first, then scalar, then nested recursion — reproduces
    // that interleaving (e.g. PriceListCreateInput yields [currency, parent],
    // not [parent, currency]).
    for (field_name, field_schema) in input_object {
        let provided = fields.get(field_name);
        let missing_or_null =
            !fields.contains_key(field_name) || matches!(provided, Some(ResolvedValue::Null));
        if field_schema.type_ref.non_null && missing_or_null {
            let mut nested_path = problem_path.to_vec();
            nested_path.push(json!(field_name));
            problems.push(variable_problem_value_path(
                &nested_path,
                "Expected value to not be null",
            ));
            continue;
        }
        let Some(field_value) = provided else {
            continue;
        };
        if let Some(problem) = validate_resolved_scalar(field_value, &field_schema.type_ref) {
            let mut nested_path = problem_path.to_vec();
            nested_path.push(json!(field_name));
            if problem.include_message {
                problems.push(variable_problem_with_message_value_path(
                    &nested_path,
                    &problem.explanation,
                ));
            } else {
                problems.push(variable_problem_value_path(
                    &nested_path,
                    &problem.explanation,
                ));
            }
        }
        if let Some(nested_input_object) =
            schema.input_objects.get(&field_schema.type_ref.named_type)
        {
            match field_value {
                ResolvedValue::Object(nested_fields) => {
                    let mut nested_path = problem_path.to_vec();
                    nested_path.push(json!(field_name));
                    problems.extend(validate_resolved_input_object(
                        &field_schema.type_ref.named_type,
                        nested_input_object,
                        nested_fields,
                        &nested_path,
                        schema,
                        order_source,
                    ));
                }
                ResolvedValue::List(items) if type_ref_is_list(&field_schema.type_ref) => {
                    for (index, item) in items.iter().enumerate() {
                        let mut nested_path = problem_path.to_vec();
                        nested_path.push(json!(field_name));
                        nested_path.push(json!(index));
                        match item {
                            ResolvedValue::Object(nested_fields) => {
                                problems.extend(validate_resolved_input_object(
                                    &field_schema.type_ref.named_type,
                                    nested_input_object,
                                    nested_fields,
                                    &nested_path,
                                    schema,
                                    order_source,
                                ));
                            }
                            ResolvedValue::Null
                                if type_ref_has_non_null_list_items(&field_schema.type_ref) =>
                            {
                                problems.push(variable_problem_value_path(
                                    &nested_path,
                                    "Expected value to not be null",
                                ));
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
    }
    problems
}

struct ScalarValidationProblem {
    explanation: String,
    include_message: bool,
}

fn validate_resolved_scalar(
    value: &ResolvedValue,
    type_ref: &SchemaTypeRef,
) -> Option<ScalarValidationProblem> {
    match type_ref.named_type.as_str() {
        "ID" => {
            // Admin GraphQL coerces ID scalars as global ids. A blank string
            // (e.g. catalogId: "" provided through a variable input object)
            // fails coercion with the same "Invalid global id ''" problem the
            // literal-argument path reports, anchored at the variable
            // definition. Non-blank values are left to the local handler.
            let ResolvedValue::String(raw) = value else {
                return None;
            };
            raw.trim().is_empty().then(|| ScalarValidationProblem {
                explanation: format!("Invalid global id '{raw}'"),
                include_message: true,
            })
        }
        "Int" => {
            // Admin GraphQL coerces Int scalars from integer values only. A float
            // (e.g. recurringCycleLimit: 1.5 provided through a variable) fails
            // coercion with a "Could not coerce" problem anchored at the variable
            // definition.
            let ResolvedValue::Float(raw) = value else {
                return None;
            };
            Some(ScalarValidationProblem {
                explanation: format!(
                    "Could not coerce value {} to Int",
                    format_float_literal(*raw)
                ),
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
        "FulfillmentEventStatus" => {
            let ResolvedValue::String(raw) = value else {
                return None;
            };
            (!fulfillment_event_status_is_allowed(raw)).then(|| ScalarValidationProblem {
                explanation: fulfillment_event_status_expected_message(raw),
                include_message: false,
            })
        }
        "CurrencyCode" => {
            let ResolvedValue::String(raw) = value else {
                return None;
            };
            (!currency_code_is_allowed(raw)).then(|| ScalarValidationProblem {
                explanation: format!("Expected \"{raw}\" to be one of: {CURRENCY_CODE_VALUES}"),
                include_message: false,
            })
        }
        "DraftOrderAppliedDiscountType" => {
            let ResolvedValue::String(raw) = value else {
                return None;
            };
            (!draft_order_applied_discount_type_is_allowed(raw)).then(|| ScalarValidationProblem {
                explanation: draft_order_applied_discount_type_expected_message(raw),
                include_message: false,
            })
        }
        _ => None,
    }
}

/// The full `CurrencyCode` enum value list as Admin GraphQL 2026-04 reports it
/// in coercion errors. Order matters: the error message lists values in this
/// exact sequence, so it is reproduced verbatim rather than sorted.
const CURRENCY_CODE_VALUES: &str = "USD, EUR, GBP, CAD, AFN, ALL, DZD, AOA, ARS, AMD, AWG, AUD, BBD, AZN, BDT, BSD, BHD, BIF, BYN, BZD, BMD, BTN, BAM, BRL, BOB, BWP, BND, BGN, MMK, KHR, CVE, KYD, XAF, CLP, CNY, COP, KMF, CDF, CRC, HRK, CZK, DKK, DJF, DOP, XCD, EGP, ERN, ETB, FKP, XPF, FJD, GIP, GMD, GHS, GTQ, GYD, GEL, GNF, HTG, HNL, HKD, HUF, ISK, INR, IDR, ILS, IRR, IQD, JMD, JPY, JEP, JOD, KZT, KES, KID, KWD, KGS, LAK, LVL, LBP, LSL, LRD, LYD, LTL, MGA, MKD, MOP, MWK, MVR, MRU, MXN, MYR, MUR, MDL, MAD, MNT, MZN, NAD, NPR, ANG, NZD, NIO, NGN, NOK, OMR, PAB, PKR, PGK, PYG, PEN, PHP, PLN, QAR, RON, RUB, RWF, WST, SHP, SAR, RSD, SCR, SLL, SGD, SDG, SOS, SYP, ZAR, KRW, SSP, SBD, LKR, SRD, SZL, SEK, CHF, TWD, THB, TJS, TZS, TOP, TTD, TND, TRY, TMT, UGX, UAH, AED, UYU, UZS, VUV, VES, VND, XOF, YER, ZMW, USDC, BYR, STD, STN, VED, VEF, XXX";

fn currency_code_is_allowed(code: &str) -> bool {
    CURRENCY_CODE_VALUES.split(", ").any(|value| value == code)
}

fn draft_order_applied_discount_type_is_allowed(value: &str) -> bool {
    matches!(value, "FIXED_AMOUNT" | "PERCENTAGE")
}

fn draft_order_applied_discount_type_expected_message(value: &str) -> String {
    format!("Expected \"{value}\" to be one of: FIXED_AMOUNT, PERCENTAGE")
}

fn fulfillment_event_status_is_allowed(status: &str) -> bool {
    matches!(
        status,
        "LABEL_PURCHASED"
            | "LABEL_PRINTED"
            | "READY_FOR_PICKUP"
            | "CONFIRMED"
            | "IN_TRANSIT"
            | "OUT_FOR_DELIVERY"
            | "ATTEMPTED_DELIVERY"
            | "DELAYED"
            | "DELIVERED"
            | "FAILURE"
            | "CARRIER_PICKED_UP"
    )
}

fn fulfillment_event_status_expected_message(status: &str) -> String {
    format!(
        "Expected \"{status}\" to be one of: LABEL_PURCHASED, LABEL_PRINTED, READY_FOR_PICKUP, CONFIRMED, IN_TRANSIT, OUT_FOR_DELIVERY, ATTEMPTED_DELIVERY, DELAYED, DELIVERED, FAILURE, CARRIER_PICKED_UP"
    )
}

fn root_argument_not_accepted_error(
    field: &RootFieldSelection,
    argument_name: &str,
    context: ValidationContext<'_>,
) -> Value {
    // Shopify anchors an unaccepted-argument error at the argument name token,
    // not at the field. For a multi-line mutation each rejected argument points
    // at its own `name:` position.
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
    // A `null` literal supplied for a non-null argument fails GraphQL coercion
    // (it is not a "missing argument" — the argument is present, its value is
    // invalid). Shopify anchors the argumentLiteralsIncompatible error at the
    // field token.
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

fn non_null_variable_null_error(
    variable_name: &str,
    variable_type: &str,
    location: SourceLocation,
) -> Value {
    json!({
        "message": format!(
            "Variable ${variable_name} of type {variable_type} was provided invalid value"
        ),
        "locations": [{ "line": location.line, "column": location.column }],
        "extensions": {
            "code": "INVALID_VARIABLE",
            "value": Value::Null,
            "problems": [{
                "path": [],
                "explanation": "Expected value to not be null"
            }]
        }
    })
}

fn argument_literal_incompatible_error(
    input_type_name: &str,
    argument_name: &str,
    invalid_value: &str,
    expected_type: &str,
    path: &[String],
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

/// Detects an Int-typed field given a float literal, returning the rendered
/// literal for the error message. Integer literals parse as `Int` and never
/// reach here.
fn int_literal_coercion_value(
    value: &RawArgumentValue,
    type_ref: &SchemaTypeRef,
) -> Option<String> {
    if type_ref.named_type != "Int" {
        return None;
    }
    match value {
        RawArgumentValue::Float(raw) => Some(format_float_literal(*raw)),
        _ => None,
    }
}

fn enum_literal_coercion_value(
    value: &RawArgumentValue,
    type_ref: &SchemaTypeRef,
) -> Option<String> {
    let provided = match value {
        RawArgumentValue::Enum(value) | RawArgumentValue::String(value) => value,
        _ => return None,
    };
    match type_ref.named_type.as_str() {
        "DraftOrderAppliedDiscountType"
            if !draft_order_applied_discount_type_is_allowed(provided) =>
        {
            Some(provided.clone())
        }
        _ => None,
    }
}

fn format_float_literal(value: f64) -> String {
    format!("{value}")
}

pub(in crate::proxy) fn input_object_argument_not_accepted_error(
    input_type_name: &str,
    argument_name: &str,
    path: &[String],
    context: ValidationContext<'_>,
) -> Value {
    // Shopify anchors the error at the rejected field-name token inside the input-object
    // literal. The token sits at bracket depth 1 + the nesting (path) depth of its parent
    // input object: e.g. `themeUpdate(id: …, input: { role: MAIN })` reports `role`, not
    // `themeUpdate`. Variable-supplied input objects have no literal token, so fall back to
    // the field location.
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
    path: &[String],
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
    // A root argument lives at bracket depth 1 (inside the field's `(...)`).
    inline_input_field_name_location(query, field.location, 1, argument_name)
}

/// Locates the `name:` token of an argument or input-object field at a specific bracket
/// depth, starting from the root field. Depth 1 is the field's argument list, depth 2 is a
/// directly-nested input object (`field(arg: { name: ... })`), and so on. Shopify anchors an
/// argumentNotAccepted error at the rejected name token, not the enclosing field, so nested
/// input-object fields report their own column. String literals are skipped so a quoted
/// occurrence of the name is never matched.
fn inline_input_field_name_location(
    query: &str,
    field_location: SourceLocation,
    target_depth: i32,
    name: &str,
) -> Option<SourceLocation> {
    let start = byte_offset_for_location(query, field_location)?;
    let bytes = query.as_bytes();
    // Find the field's argument list. If a selection set opens first, the field
    // takes no arguments.
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
                let before_ok = index == 0 || !is_graphql_name_byte(bytes[index - 1]);
                if before_ok && query[index..].starts_with(name) {
                    let after = index + name.len();
                    let after_ok = bytes
                        .get(after)
                        .is_none_or(|next| !is_graphql_name_byte(*next));
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

fn inline_argument_value_location(
    query: &str,
    field: &RootFieldSelection,
    argument_name: &str,
) -> Option<SourceLocation> {
    let start = byte_offset_for_location(query, field.location)?;
    let haystack = &query[start..];
    let argument_start = find_argument_name_with_colon(haystack, argument_name)?;
    let after_name = start + argument_start + argument_name.len();
    let after_colon = query[after_name..].find(':')? + after_name + 1;
    let value_offset = query[after_colon..]
        .char_indices()
        .find_map(|(offset, ch)| (!ch.is_whitespace()).then_some(after_colon + offset))?;
    source_location_for_byte_offset(query, value_offset)
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
    let after_colon = query[after_name..].find(':')? + after_name + 1;
    let value_offset = query[after_colon..]
        .char_indices()
        .find_map(|(offset, ch)| (!ch.is_whitespace()).then_some(after_colon + offset))?;
    source_location_for_byte_offset(query, value_offset)
}

fn find_argument_name_with_colon(haystack: &str, argument_name: &str) -> Option<usize> {
    let mut search_start = 0;
    while search_start < haystack.len() {
        let relative = haystack[search_start..].find(argument_name)?;
        let candidate = search_start + relative;
        let before_ok = haystack[..candidate]
            .chars()
            .next_back()
            .is_none_or(|ch| !is_graphql_name_char(ch));
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

fn is_graphql_name_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

fn is_graphql_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
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

fn input_error_path(context: ValidationContext<'_>, path: &[String], argument_name: &str) -> Value {
    let mut segments = vec![
        Value::String(context.operation_path.to_string()),
        Value::String(context.response_key.to_string()),
    ];
    segments.extend(path.iter().cloned().map(Value::String));
    segments.push(Value::String(argument_name.to_string()));
    Value::Array(segments)
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
        let mut schema = AdminInputSchema::default();
        extend_graphql_base_validation_input_schema(&mut schema);
        extend_gift_card_input_schema(&mut schema);
        extend_discount_basic_input_schema(&mut schema);
        extend_customer_merge_input_schema(&mut schema);
        extend_customer_input_schema(&mut schema);
        extend_orders_input_schema(&mut schema);
        extend_marketing_engagement_input_schema(&mut schema);
        extend_functions_input_schema(&mut schema);
        extend_online_store_input_schema(&mut schema);
        extend_markets_input_schema(&mut schema);
        extend_product_variant_input_schema(&mut schema);
        extend_publication_input_schema(&mut schema);
        extend_payments_input_schema(&mut schema);
        extend_shipping_input_schema(&mut schema);
        extend_fulfillment_event_input_schema(&mut schema);
        extend_store_credit_input_schema(&mut schema);
        schema
    })
}

fn extend_graphql_base_validation_input_schema(schema: &mut AdminInputSchema) {
    let parsed: Value = serde_json::from_str(include_str!(
        "../../config/admin-graphql-mutation-schema.json"
    ))
    .expect("checked-in Admin GraphQL mutation schema should be valid JSON");
    if let Some((name, arguments)) =
        captured_mutation_arguments(&parsed, "pubSubWebhookSubscriptionCreate")
    {
        schema.mutation_fields.insert(name, arguments);
    }
    if let Some((name, fields)) =
        captured_input_object_fields(&parsed, "PubSubWebhookSubscriptionInput")
    {
        schema.input_objects.insert(name, fields);
    }
}

fn captured_mutation_arguments(
    parsed: &Value,
    mutation_name: &str,
) -> Option<(String, BTreeMap<String, SchemaArgument>)> {
    let mutation = parsed
        .get("mutations")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|mutation| mutation.get("name").and_then(Value::as_str) == Some(mutation_name))?;
    let arguments = mutation
        .get("args")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(schema_argument)
        .collect::<BTreeMap<_, _>>();
    Some((mutation_name.to_string(), arguments))
}

fn captured_input_object_fields(
    parsed: &Value,
    input_object_name: &str,
) -> Option<(String, BTreeMap<String, SchemaInputField>)> {
    let input_object = parsed
        .get("inputObjects")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|input_object| {
            input_object.get("name").and_then(Value::as_str) == Some(input_object_name)
        })?;
    let fields = input_object
        .get("inputFields")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(schema_input_field)
        .collect::<BTreeMap<_, _>>();
    Some((input_object_name.to_string(), fields))
}

fn schema_argument(argument: &Value) -> Option<(String, SchemaArgument)> {
    let name = argument.get("name").and_then(Value::as_str)?;
    let type_ref = schema_type_ref(argument.get("type")?)?;
    Some((name.to_string(), mutation_arg(type_ref)))
}

fn schema_input_field(field: &Value) -> Option<(String, SchemaInputField)> {
    let name = field.get("name").and_then(Value::as_str)?;
    let type_ref = schema_type_ref(field.get("type")?)?;
    Some((name.to_string(), input_field(type_ref)))
}

fn schema_type_ref(value: &Value) -> Option<SchemaTypeRef> {
    let (display, named_type, non_null) = schema_type_ref_parts(value)?;
    Some(SchemaTypeRef {
        display,
        named_type,
        non_null,
    })
}

fn schema_type_ref_parts(value: &Value) -> Option<(String, String, bool)> {
    let kind = value.get("kind").and_then(Value::as_str)?;
    match kind {
        "NON_NULL" => {
            let (display, named_type, _) = schema_type_ref_parts(value.get("ofType")?)?;
            Some((format!("{display}!"), named_type, true))
        }
        "LIST" => {
            let (display, named_type, _) = schema_type_ref_parts(value.get("ofType")?)?;
            Some((format!("[{display}]"), named_type, false))
        }
        _ => {
            let name = value.get("name").and_then(Value::as_str)?;
            Some((name.to_string(), name.to_string(), false))
        }
    }
}

fn extend_product_variant_input_schema(schema: &mut AdminInputSchema) {
    // The public Admin schema for `ProductVariantsBulkInput` exposes
    // `optionValues`, not the legacy/internal `options` key. Registering the
    // bulk input object keeps unsupported keys as GraphQL coercion errors before
    // the local product variant handler stages anything.
    schema.input_objects.insert(
        "ProductVariantsBulkInput".to_string(),
        BTreeMap::from([
            ("barcode".to_string(), input_field(named("String"))),
            ("compareAtPrice".to_string(), input_field(named("Money"))),
            ("id".to_string(), input_field(named("ID"))),
            (
                "mediaSrc".to_string(),
                input_field(list_of_non_null("String")),
            ),
            (
                "inventoryPolicy".to_string(),
                input_field(named("ProductVariantInventoryPolicy")),
            ),
            (
                "inventoryQuantities".to_string(),
                input_field(list_of_non_null("InventoryLevelInput")),
            ),
            (
                "quantityAdjustments".to_string(),
                input_field(list_of_non_null("InventoryQuantityAdjustmentInput")),
            ),
            (
                "inventoryItem".to_string(),
                input_field(named("InventoryItemInput")),
            ),
            ("mediaId".to_string(), input_field(named("ID"))),
            (
                "metafields".to_string(),
                input_field(list_of_non_null("MetafieldInput")),
            ),
            (
                "optionValues".to_string(),
                input_field(list_of_non_null("VariantOptionValueInput")),
            ),
            ("price".to_string(), input_field(named("Money"))),
            ("taxable".to_string(), input_field(named("Boolean"))),
            ("taxCode".to_string(), input_field(named("String"))),
            (
                "unitPriceMeasurement".to_string(),
                input_field(named("UnitPriceMeasurementInput")),
            ),
            ("showUnitPrice".to_string(), input_field(named("Boolean"))),
            (
                "requiresComponents".to_string(),
                input_field(named("Boolean")),
            ),
        ]),
    );
    schema.mutation_fields.insert(
        "productVariantsBulkCreate".to_string(),
        BTreeMap::from([
            ("productId".to_string(), mutation_arg(non_null("ID"))),
            (
                "variants".to_string(),
                mutation_arg(non_null_list_of_non_null("ProductVariantsBulkInput")),
            ),
            (
                "strategy".to_string(),
                mutation_arg(named("ProductVariantsBulkCreateStrategy")),
            ),
        ]),
    );
    schema.mutation_fields.insert(
        "productVariantsBulkUpdate".to_string(),
        BTreeMap::from([
            ("productId".to_string(), mutation_arg(non_null("ID"))),
            (
                "variants".to_string(),
                mutation_arg(non_null_list_of_non_null("ProductVariantsBulkInput")),
            ),
        ]),
    );
}

fn extend_publication_input_schema(schema: &mut AdminInputSchema) {
    schema.input_objects.insert(
        "PublicationCreateInput".to_string(),
        BTreeMap::from([
            ("catalogId".to_string(), input_field(named("ID"))),
            (
                "defaultState".to_string(),
                input_field(named("PublicationCreateInputPublicationDefaultState")),
            ),
            ("autoPublish".to_string(), input_field(named("Boolean"))),
        ]),
    );
    schema.input_objects.insert(
        "PublicationUpdateInput".to_string(),
        BTreeMap::from([
            (
                "publishablesToAdd".to_string(),
                input_field(list_of_non_null("ID")),
            ),
            (
                "publishablesToRemove".to_string(),
                input_field(list_of_non_null("ID")),
            ),
            ("autoPublish".to_string(), input_field(named("Boolean"))),
        ]),
    );
    schema.mutation_fields.insert(
        "publicationCreate".to_string(),
        BTreeMap::from([(
            "input".to_string(),
            mutation_arg(non_null("PublicationCreateInput")),
        )]),
    );
    schema.mutation_fields.insert(
        "publicationUpdate".to_string(),
        BTreeMap::from([
            ("id".to_string(), mutation_arg(non_null("ID"))),
            (
                "input".to_string(),
                mutation_arg(non_null("PublicationUpdateInput")),
            ),
        ]),
    );
    schema.mutation_fields.insert(
        "publicationDelete".to_string(),
        BTreeMap::from([("id".to_string(), mutation_arg(non_null("ID")))]),
    );
}

fn extend_fulfillment_event_input_schema(schema: &mut AdminInputSchema) {
    // `fulfillmentEventCreate(fulfillmentEvent: FulfillmentEventInput!)` on the
    // active public Admin schema (2026-04). `status` is a non-null
    // `FulfillmentEventStatus` enum, so an out-of-range value must surface a
    // top-level `INVALID_VARIABLE` coercion error (anchored at the variable
    // definition) before the local handler runs. Every other accepted field is
    // registered nullable so the validator only rejects an out-of-range `status`
    // or an unknown field, and never fabricates a missing-required error for the
    // happy-path mutation that omits the optional geolocation fields.
    schema.input_objects.insert(
        "FulfillmentEventInput".to_string(),
        BTreeMap::from([
            ("fulfillmentId".to_string(), input_field(named("ID"))),
            (
                "status".to_string(),
                input_field(non_null("FulfillmentEventStatus")),
            ),
            ("message".to_string(), input_field(named("String"))),
            ("happenedAt".to_string(), input_field(named("DateTime"))),
            (
                "estimatedDeliveryAt".to_string(),
                input_field(named("DateTime")),
            ),
            ("city".to_string(), input_field(named("String"))),
            ("province".to_string(), input_field(named("String"))),
            ("country".to_string(), input_field(named("String"))),
            ("zip".to_string(), input_field(named("String"))),
            ("address1".to_string(), input_field(named("String"))),
            ("latitude".to_string(), input_field(named("Float"))),
            ("longitude".to_string(), input_field(named("Float"))),
        ]),
    );
    schema.mutation_fields.insert(
        "fulfillmentEventCreate".to_string(),
        BTreeMap::from([(
            "fulfillmentEvent".to_string(),
            mutation_arg(non_null("FulfillmentEventInput")),
        )]),
    );
}

fn extend_store_credit_input_schema(schema: &mut AdminInputSchema) {
    // `storeCreditAccountCredit` / `storeCreditAccountDebit` on Admin API 2026-04
    // are staged locally, but their input objects are registered here so an
    // unsupported field (e.g. `attribution`, or `notify` on a *debit* input where
    // it is not defined) surfaces a top-level `INVALID_VARIABLE` coercion error
    // before the resolver runs — exactly as the live schema rejects fields it does
    // not define. `MoneyInput` is intentionally left unregistered so the resolver
    // owns money-field validation and the nested `amount`/`currencyCode` fields are
    // never flagged as unknown.
    schema.input_objects.insert(
        "StoreCreditAccountCreditInput".to_string(),
        BTreeMap::from([
            (
                "creditAmount".to_string(),
                input_field(non_null("MoneyInput")),
            ),
            ("expiresAt".to_string(), input_field(named("DateTime"))),
            ("notify".to_string(), input_field(named("Boolean"))),
        ]),
    );
    schema.input_objects.insert(
        "StoreCreditAccountDebitInput".to_string(),
        BTreeMap::from([(
            "debitAmount".to_string(),
            input_field(non_null("MoneyInput")),
        )]),
    );
    schema.mutation_fields.insert(
        "storeCreditAccountCredit".to_string(),
        BTreeMap::from([
            ("id".to_string(), mutation_arg(non_null("ID"))),
            (
                "creditInput".to_string(),
                mutation_arg(non_null("StoreCreditAccountCreditInput")),
            ),
        ]),
    );
    schema.mutation_fields.insert(
        "storeCreditAccountDebit".to_string(),
        BTreeMap::from([
            ("id".to_string(), mutation_arg(non_null("ID"))),
            (
                "debitInput".to_string(),
                mutation_arg(non_null("StoreCreditAccountDebitInput")),
            ),
        ]),
    );
}

fn extend_shipping_input_schema(schema: &mut AdminInputSchema) {
    // `fulfillmentServiceCreate` on the active public Admin schema (2026-04) accepts
    // only these field arguments. `permitsSkuSharing`, `inventorySyncEnabled`, and
    // `fulfillmentOrdersOptIn` are not exposed, so supplying one must raise a top-level
    // `argumentNotAccepted` GraphQL error (anchored at the argument name token) before
    // the resolver runs. Every accepted argument is registered nullable so the validator
    // only rejects unaccepted arguments and never fabricates a missing-required error for
    // the create docs that omit `callbackUrl`.
    schema.mutation_fields.insert(
        "fulfillmentServiceCreate".to_string(),
        BTreeMap::from([
            ("name".to_string(), mutation_arg(named("String"))),
            ("callbackUrl".to_string(), mutation_arg(named("URL"))),
            (
                "trackingSupport".to_string(),
                mutation_arg(named("Boolean")),
            ),
            (
                "inventoryManagement".to_string(),
                mutation_arg(named("Boolean")),
            ),
            (
                "requiresShippingMethod".to_string(),
                mutation_arg(named("Boolean")),
            ),
        ]),
    );
}

fn extend_payments_input_schema(schema: &mut AdminInputSchema) {
    // customerPaymentMethodCreditCardCreate on Admin API 2026-04 takes three
    // required (non-null) field arguments: `customerId`, `billingAddress`, and
    // `sessionId`. Omitting any of them must surface a top-level
    // `missingRequiredArguments` error before the local payment-method handler
    // runs (the field-vault handler only owns billing-address blank checks once
    // the arguments are structurally present). `MailingAddressInput` is left
    // unregistered so the resolver continues to own per-field blank validation.
    schema.mutation_fields.insert(
        "customerPaymentMethodCreditCardCreate".to_string(),
        BTreeMap::from([
            ("customerId".to_string(), mutation_arg(non_null("ID"))),
            (
                "billingAddress".to_string(),
                mutation_arg(non_null("MailingAddressInput")),
            ),
            ("sessionId".to_string(), mutation_arg(non_null("String"))),
        ]),
    );
}

fn input_field(type_ref: SchemaTypeRef) -> SchemaInputField {
    SchemaInputField { type_ref }
}

fn mutation_arg(type_ref: SchemaTypeRef) -> SchemaArgument {
    SchemaArgument { type_ref }
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

fn extend_markets_input_schema(schema: &mut AdminInputSchema) {
    // CatalogCreateInput on Admin API 2026-04: `context` is a required
    // (non-null) input field. Omitting it must surface a top-level
    // INVALID_VARIABLE coercion error before the local catalog handler runs.
    schema.input_objects.insert(
        "CatalogCreateInput".to_string(),
        BTreeMap::from([
            ("title".to_string(), input_field(named("String"))),
            ("status".to_string(), input_field(named("CatalogStatus"))),
            (
                "context".to_string(),
                input_field(non_null("CatalogContextInput")),
            ),
            ("priceListId".to_string(), input_field(named("ID"))),
            ("publicationId".to_string(), input_field(named("ID"))),
        ]),
    );
    schema.mutation_fields.insert(
        "catalogCreate".to_string(),
        BTreeMap::from([(
            "input".to_string(),
            mutation_arg(non_null("CatalogCreateInput")),
        )]),
    );

    // PriceListCreateInput on Admin API 2026-04: `currency` (a CurrencyCode
    // enum) and `parent` are both required. An out-of-range currency plus a
    // missing parent yields two ordered problems ([currency, parent]).
    schema.input_objects.insert(
        "PriceListCreateInput".to_string(),
        BTreeMap::from([
            ("name".to_string(), input_field(named("String"))),
            (
                "currency".to_string(),
                input_field(non_null("CurrencyCode")),
            ),
            (
                "parent".to_string(),
                input_field(non_null("PriceListParentCreateInput")),
            ),
            ("catalogId".to_string(), input_field(named("ID"))),
        ]),
    );
    schema.mutation_fields.insert(
        "priceListCreate".to_string(),
        BTreeMap::from([(
            "input".to_string(),
            mutation_arg(non_null("PriceListCreateInput")),
        )]),
    );

    // PriceListUpdateInput on Admin API 2026-04: every field is optional on
    // update. `catalogId` is an ID; a blank string fails global-id coercion
    // (INVALID_VARIABLE) before the local handler runs. `parent`'s type is
    // intentionally left unregistered in `input_objects` so adjustment-range
    // checks stay with the local handler (which emits INVALID_ADJUSTMENT_VALUE
    // as a userError, not a coercion error).
    schema.input_objects.insert(
        "PriceListUpdateInput".to_string(),
        BTreeMap::from([
            ("name".to_string(), input_field(named("String"))),
            ("currency".to_string(), input_field(named("CurrencyCode"))),
            (
                "parent".to_string(),
                input_field(named("PriceListParentUpdateInput")),
            ),
            ("catalogId".to_string(), input_field(named("ID"))),
        ]),
    );
    schema.mutation_fields.insert(
        "priceListUpdate".to_string(),
        BTreeMap::from([
            ("id".to_string(), mutation_arg(non_null("ID"))),
            (
                "input".to_string(),
                mutation_arg(non_null("PriceListUpdateInput")),
            ),
        ]),
    );
}

fn extend_marketing_engagement_input_schema(schema: &mut AdminInputSchema) {
    // MarketingEngagementInput on Admin API 2026-04: occurredOn, utcOffset, and
    // isCumulative are required (non-null) schema fields. Omitting any of them must
    // produce top-level coercion errors before the local handler stages anything.
    schema.input_objects.insert(
        "MarketingEngagementInput".to_string(),
        BTreeMap::from([
            ("occurredOn".to_string(), input_field(non_null("Date"))),
            ("utcOffset".to_string(), input_field(non_null("UtcOffset"))),
            ("isCumulative".to_string(), input_field(non_null("Boolean"))),
            ("impressionsCount".to_string(), input_field(named("Int"))),
            ("viewsCount".to_string(), input_field(named("Int"))),
            ("clicksCount".to_string(), input_field(named("Int"))),
            ("sharesCount".to_string(), input_field(named("Int"))),
            ("favoritesCount".to_string(), input_field(named("Int"))),
            ("commentsCount".to_string(), input_field(named("Int"))),
            ("unsubscribesCount".to_string(), input_field(named("Int"))),
            ("complaintsCount".to_string(), input_field(named("Int"))),
            ("failsCount".to_string(), input_field(named("Int"))),
            ("sendsCount".to_string(), input_field(named("Int"))),
            ("uniqueViewsCount".to_string(), input_field(named("Int"))),
            ("uniqueClicksCount".to_string(), input_field(named("Int"))),
            ("adSpend".to_string(), input_field(named("MoneyInput"))),
            ("sales".to_string(), input_field(named("MoneyInput"))),
            ("sessionsCount".to_string(), input_field(named("Int"))),
            ("orders".to_string(), input_field(named("Decimal"))),
            (
                "firstTimeCustomers".to_string(),
                input_field(named("Decimal")),
            ),
            (
                "returningCustomers".to_string(),
                input_field(named("Decimal")),
            ),
            (
                "primaryConversions".to_string(),
                input_field(named("Decimal")),
            ),
            ("allConversions".to_string(), input_field(named("Decimal"))),
        ]),
    );
    schema.mutation_fields.insert(
        "marketingEngagementCreate".to_string(),
        BTreeMap::from([
            ("marketingActivityId".to_string(), mutation_arg(named("ID"))),
            ("remoteId".to_string(), mutation_arg(named("String"))),
            ("channelHandle".to_string(), mutation_arg(named("String"))),
            (
                "marketingEngagement".to_string(),
                mutation_arg(non_null("MarketingEngagementInput")),
            ),
        ]),
    );
}

fn extend_functions_input_schema(schema: &mut AdminInputSchema) {
    // ValidationUpdateInput on Admin API 2026-04 accepts only enable,
    // blockOnFailure, metafields, and title. Rebinding a validation to a
    // different function is not supported, so functionId / functionHandle are
    // not fields on the input object — supplying them must raise a schema error
    // (argumentNotAccepted for a literal, INVALID_VARIABLE for a variable)
    // before the validationUpdate resolver runs.
    schema.input_objects.insert(
        "ValidationUpdateInput".to_string(),
        BTreeMap::from([
            ("enable".to_string(), input_field(named("Boolean"))),
            ("blockOnFailure".to_string(), input_field(named("Boolean"))),
            (
                "metafields".to_string(),
                input_field(list_of_non_null("MetafieldInput")),
            ),
            ("title".to_string(), input_field(named("String"))),
        ]),
    );
    schema.mutation_fields.insert(
        "validationUpdate".to_string(),
        BTreeMap::from([
            ("id".to_string(), mutation_arg(non_null("ID"))),
            (
                "validation".to_string(),
                mutation_arg(non_null("ValidationUpdateInput")),
            ),
        ]),
    );
    // cartTransformCreate takes scalar root arguments only; the function is
    // selected by functionId or functionHandle. There is no `cartTransform`
    // wrapper input and no `title` argument, so supplying either must raise a
    // top-level argumentNotAccepted error.
    schema.mutation_fields.insert(
        "cartTransformCreate".to_string(),
        BTreeMap::from([
            ("functionId".to_string(), mutation_arg(named("ID"))),
            ("functionHandle".to_string(), mutation_arg(named("String"))),
            ("blockOnFailure".to_string(), mutation_arg(named("Boolean"))),
            (
                "metafields".to_string(),
                mutation_arg(list_of_non_null("MetafieldInput")),
            ),
        ]),
    );
}

fn extend_online_store_input_schema(schema: &mut AdminInputSchema) {
    // OnlineStoreThemeInput on Admin API 2025-01 accepts only `name`. A theme's role is
    // set at creation (themeCreate(role:)) and changed via themePublish, never through
    // themeUpdate's input, so supplying `role` must raise a top-level argumentNotAccepted
    // schema error before the themeUpdate resolver runs.
    schema.input_objects.insert(
        "OnlineStoreThemeInput".to_string(),
        BTreeMap::from([("name".to_string(), input_field(named("String")))]),
    );
    schema.mutation_fields.insert(
        "themeUpdate".to_string(),
        BTreeMap::from([
            ("id".to_string(), mutation_arg(non_null("ID"))),
            (
                "input".to_string(),
                mutation_arg(non_null("OnlineStoreThemeInput")),
            ),
        ]),
    );
}

fn extend_customer_merge_input_schema(schema: &mut AdminInputSchema) {
    // customerMerge requires both customerOneId and customerTwoId as non-null IDs
    // overrideFields is optional
    // Mirror the live Admin schema's CustomerMergeOverrideFields so a valid call
    // that picks which customer's scalar fields / addresses survive the merge is
    // not flagged as `argumentNotAccepted` before the resolver runs.
    schema.input_objects.insert(
        "CustomerMergeOverrideFields".to_string(),
        BTreeMap::from([
            (
                "customerIdOfFirstNameToKeep".to_string(),
                input_field(named("ID")),
            ),
            (
                "customerIdOfLastNameToKeep".to_string(),
                input_field(named("ID")),
            ),
            (
                "customerIdOfEmailToKeep".to_string(),
                input_field(named("ID")),
            ),
            (
                "customerIdOfPhoneNumberToKeep".to_string(),
                input_field(named("ID")),
            ),
            (
                "customerIdOfDefaultAddressToKeep".to_string(),
                input_field(named("ID")),
            ),
            ("note".to_string(), input_field(named("String"))),
            ("tags".to_string(), input_field(list_of_non_null("String"))),
        ]),
    );
    schema.mutation_fields.insert(
        "customerMerge".to_string(),
        BTreeMap::from([
            ("customerOneId".to_string(), mutation_arg(non_null("ID"))),
            ("customerTwoId".to_string(), mutation_arg(non_null("ID"))),
            (
                "overrideFields".to_string(),
                mutation_arg(named("CustomerMergeOverrideFields")),
            ),
        ]),
    );
}

fn extend_customer_input_schema(schema: &mut AdminInputSchema) {
    // customerCreate(input: CustomerInput!) on Admin API 2025-01. Only the
    // top-level `input` argument is required; the CustomerInput object itself is
    // intentionally left unregistered so the local customerCreate handler keeps
    // ownership of field-level validation (it emits payload userErrors, not
    // top-level coercion errors). Registering the field alone is enough to
    // surface the missing-argument / null-literal / unbound-variable envelopes
    // (missingRequiredArguments, argumentLiteralsIncompatible, INVALID_VARIABLE)
    // before the resolver runs.
    schema.mutation_fields.insert(
        "customerCreate".to_string(),
        BTreeMap::from([("input".to_string(), mutation_arg(non_null("CustomerInput")))]),
    );

    // dataSaleOptOut(email: String!) on Admin API 2026-04. The single `email`
    // argument is non-null, so a missing or explicitly-null email must surface a
    // top-level `missingRequiredArguments` / null-coercion envelope before the
    // local privacy handler runs (rather than the handler's own FAILED userError).
    schema.mutation_fields.insert(
        "dataSaleOptOut".to_string(),
        BTreeMap::from([("email".to_string(), mutation_arg(non_null("String")))]),
    );
}

fn extend_orders_input_schema(schema: &mut AdminInputSchema) {
    schema.input_objects.insert(
        "DraftOrderAppliedDiscountInput".to_string(),
        BTreeMap::from([
            ("amount".to_string(), input_field(named("Money"))),
            (
                "amountWithCurrency".to_string(),
                input_field(named("MoneyInput")),
            ),
            ("description".to_string(), input_field(named("String"))),
            ("title".to_string(), input_field(named("String"))),
            ("value".to_string(), input_field(non_null("Float"))),
            (
                "valueType".to_string(),
                input_field(non_null("DraftOrderAppliedDiscountType")),
            ),
        ]),
    );
    schema.input_objects.insert(
        "DraftOrderLineItemInput".to_string(),
        BTreeMap::from([
            (
                "appliedDiscount".to_string(),
                input_field(named("DraftOrderAppliedDiscountInput")),
            ),
            (
                "customAttributes".to_string(),
                input_field(list_of_non_null("AttributeInput")),
            ),
            ("grams".to_string(), input_field(named("Int"))),
            ("originalUnitPrice".to_string(), input_field(named("Money"))),
            (
                "originalUnitPriceWithCurrency".to_string(),
                input_field(named("MoneyInput")),
            ),
            ("quantity".to_string(), input_field(non_null("Int"))),
            (
                "requiresShipping".to_string(),
                input_field(named("Boolean")),
            ),
            ("sku".to_string(), input_field(named("String"))),
            ("taxable".to_string(), input_field(named("Boolean"))),
            ("title".to_string(), input_field(named("String"))),
            ("variantId".to_string(), input_field(named("ID"))),
            ("weight".to_string(), input_field(named("WeightInput"))),
            ("uuid".to_string(), input_field(named("String"))),
            (
                "bundleComponents".to_string(),
                input_field(list_of_non_null(
                    "BundlesDraftOrderBundleLineItemComponentInput",
                )),
            ),
            (
                "components".to_string(),
                input_field(list_of_non_null("DraftOrderLineItemComponentInput")),
            ),
            (
                "generatePriceOverride".to_string(),
                input_field(named("Boolean")),
            ),
            (
                "priceOverride".to_string(),
                input_field(named("MoneyInput")),
            ),
        ]),
    );
    schema.input_objects.insert(
        "DraftOrderInput".to_string(),
        BTreeMap::from([
            (
                "appliedDiscount".to_string(),
                input_field(named("DraftOrderAppliedDiscountInput")),
            ),
            (
                "discountCodes".to_string(),
                input_field(list_of_non_null("String")),
            ),
            (
                "acceptAutomaticDiscounts".to_string(),
                input_field(named("Boolean")),
            ),
            (
                "billingAddress".to_string(),
                input_field(named("MailingAddressInput")),
            ),
            ("customerId".to_string(), input_field(named("ID"))),
            (
                "customAttributes".to_string(),
                input_field(list_of_non_null("AttributeInput")),
            ),
            ("email".to_string(), input_field(named("String"))),
            (
                "lineItems".to_string(),
                input_field(list_of_non_null("DraftOrderLineItemInput")),
            ),
            (
                "metafields".to_string(),
                input_field(list_of_non_null("MetafieldInput")),
            ),
            (
                "localizationExtensions".to_string(),
                input_field(list_of_non_null("LocalizationExtensionInput")),
            ),
            (
                "localizedFields".to_string(),
                input_field(list_of_non_null("LocalizedFieldInput")),
            ),
            ("note".to_string(), input_field(named("String"))),
            (
                "shippingAddress".to_string(),
                input_field(named("MailingAddressInput")),
            ),
            (
                "shippingLine".to_string(),
                input_field(named("ShippingLineInput")),
            ),
            ("tags".to_string(), input_field(list_of_non_null("String"))),
            ("taxExempt".to_string(), input_field(named("Boolean"))),
            (
                "useCustomerDefaultAddress".to_string(),
                input_field(named("Boolean")),
            ),
            (
                "visibleToCustomer".to_string(),
                input_field(named("Boolean")),
            ),
            (
                "reserveInventoryUntil".to_string(),
                input_field(named("DateTime")),
            ),
            (
                "presentmentCurrencyCode".to_string(),
                input_field(named("CurrencyCode")),
            ),
            (
                "marketRegionCountryCode".to_string(),
                input_field(named("CountryCode")),
            ),
            ("phone".to_string(), input_field(named("String"))),
            (
                "paymentTerms".to_string(),
                input_field(named("PaymentTermsInput")),
            ),
            (
                "purchasingEntity".to_string(),
                input_field(named("PurchasingEntityInput")),
            ),
            ("sourceName".to_string(), input_field(named("String"))),
            (
                "allowDiscountCodesInCheckout".to_string(),
                input_field(named("Boolean")),
            ),
            ("poNumber".to_string(), input_field(named("String"))),
            ("sessionToken".to_string(), input_field(named("String"))),
            (
                "transformerFingerprint".to_string(),
                input_field(named("String")),
            ),
        ]),
    );

    // The order/draft-order create + edit mutations require their primary
    // argument (a non-null input object or id). Each is registered with its full
    // set of accepted root arguments so that valid calls (which pass optional
    // arguments like paymentGatewayId / notifyCustomer) are not flagged as
    // "argument not accepted". Draft-order input objects are registered for the
    // public GraphQL coercion branches the local resolver never sees, while
    // domain-level userErrors stay with the local draft-order resolver.
    schema.mutation_fields.insert(
        "draftOrderCalculate".to_string(),
        BTreeMap::from([(
            "input".to_string(),
            mutation_arg(non_null("DraftOrderInput")),
        )]),
    );
    schema.mutation_fields.insert(
        "draftOrderCreate".to_string(),
        BTreeMap::from([(
            "input".to_string(),
            mutation_arg(non_null("DraftOrderInput")),
        )]),
    );
    schema.mutation_fields.insert(
        "draftOrderComplete".to_string(),
        BTreeMap::from([
            ("id".to_string(), mutation_arg(non_null("ID"))),
            ("paymentGatewayId".to_string(), mutation_arg(named("ID"))),
            ("paymentPending".to_string(), mutation_arg(named("Boolean"))),
            ("sourceName".to_string(), mutation_arg(named("String"))),
        ]),
    );
    schema.mutation_fields.insert(
        "draftOrderUpdate".to_string(),
        BTreeMap::from([
            ("id".to_string(), mutation_arg(non_null("ID"))),
            (
                "input".to_string(),
                mutation_arg(non_null("DraftOrderInput")),
            ),
        ]),
    );
    schema.mutation_fields.insert(
        "orderCreate".to_string(),
        BTreeMap::from([
            (
                "order".to_string(),
                mutation_arg(non_null("OrderCreateOrderInput")),
            ),
            (
                "options".to_string(),
                mutation_arg(named("OrderCreateOptionsInput")),
            ),
        ]),
    );
    schema.mutation_fields.insert(
        "orderEditBegin".to_string(),
        BTreeMap::from([("id".to_string(), mutation_arg(non_null("ID")))]),
    );
    schema.mutation_fields.insert(
        "orderEditCommit".to_string(),
        BTreeMap::from([
            ("id".to_string(), mutation_arg(non_null("ID"))),
            ("notifyCustomer".to_string(), mutation_arg(named("Boolean"))),
            ("staffNote".to_string(), mutation_arg(named("String"))),
        ]),
    );

    // Fulfillment lifecycle mutations. Routed locally now, so the proxy owns the
    // top-level missing-argument / null-literal / unbound-variable envelopes for
    // their required `id` / `fulfillmentId` arguments. The full accepted argument
    // set is registered so valid calls (which pass optional notifyCustomer /
    // trackingInfoInput) are not flagged "argument not accepted".
    schema.mutation_fields.insert(
        "fulfillmentCancel".to_string(),
        BTreeMap::from([("id".to_string(), mutation_arg(non_null("ID")))]),
    );
    schema.mutation_fields.insert(
        "fulfillmentTrackingInfoUpdate".to_string(),
        BTreeMap::from([
            ("fulfillmentId".to_string(), mutation_arg(non_null("ID"))),
            (
                "trackingInfoInput".to_string(),
                mutation_arg(non_null("FulfillmentTrackingInput")),
            ),
            ("notifyCustomer".to_string(), mutation_arg(named("Boolean"))),
        ]),
    );
    schema.input_objects.insert(
        "ReverseFulfillmentOrderDisposeInput".to_string(),
        BTreeMap::from([
            (
                "reverseFulfillmentOrderLineItemId".to_string(),
                input_field(non_null("ID")),
            ),
            ("quantity".to_string(), input_field(non_null("Int"))),
            ("locationId".to_string(), input_field(named("ID"))),
            (
                "dispositionType".to_string(),
                input_field(non_null("ReverseFulfillmentOrderDispositionType")),
            ),
        ]),
    );
    schema.mutation_fields.insert(
        "reverseFulfillmentOrderDispose".to_string(),
        BTreeMap::from([(
            "dispositionInputs".to_string(),
            mutation_arg(non_null_list_of_non_null(
                "ReverseFulfillmentOrderDisposeInput",
            )),
        )]),
    );

    // Order-edit calculated-session mutations. Each is registered with its full
    // accepted argument set (so valid edits are not flagged "argument not
    // accepted") plus the required arguments / input-object attributes Shopify
    // enforces during variable coercion. Routing these locally means the proxy
    // owns the top-level coercion / missing-argument / missing-input-attribute
    // envelopes that previously only surfaced when the call passed through to a
    // recorded response — the local edit engine never sees a malformed input.
    //
    // `MoneyInput` requires both `amount` (Decimal!) and `currencyCode`
    // (CurrencyCode!); the order-edit money arguments (custom-item price, applied
    // discount fixedValue, shipping-line price) descend into it so an inline
    // money object missing `currencyCode` raises `missingRequiredInputObjectAttribute`.
    schema.input_objects.insert(
        "MoneyInput".to_string(),
        BTreeMap::from([
            ("amount".to_string(), input_field(non_null("Decimal"))),
            (
                "currencyCode".to_string(),
                input_field(non_null("CurrencyCode")),
            ),
        ]),
    );
    schema.mutation_fields.insert(
        "orderEditAddVariant".to_string(),
        BTreeMap::from([
            ("id".to_string(), mutation_arg(non_null("ID"))),
            ("variantId".to_string(), mutation_arg(non_null("ID"))),
            ("quantity".to_string(), mutation_arg(non_null("Int"))),
            ("locationId".to_string(), mutation_arg(named("ID"))),
            (
                "allowDuplicates".to_string(),
                mutation_arg(named("Boolean")),
            ),
        ]),
    );
    schema.mutation_fields.insert(
        "orderEditSetQuantity".to_string(),
        BTreeMap::from([
            ("id".to_string(), mutation_arg(non_null("ID"))),
            ("lineItemId".to_string(), mutation_arg(non_null("ID"))),
            ("quantity".to_string(), mutation_arg(non_null("Int"))),
            ("restock".to_string(), mutation_arg(named("Boolean"))),
        ]),
    );
    schema.mutation_fields.insert(
        "orderEditAddCustomItem".to_string(),
        BTreeMap::from([
            ("id".to_string(), mutation_arg(non_null("ID"))),
            ("title".to_string(), mutation_arg(non_null("String"))),
            ("quantity".to_string(), mutation_arg(non_null("Int"))),
            ("price".to_string(), mutation_arg(non_null("MoneyInput"))),
            (
                "requiresShipping".to_string(),
                mutation_arg(named("Boolean")),
            ),
            ("taxable".to_string(), mutation_arg(named("Boolean"))),
        ]),
    );
    schema.input_objects.insert(
        "OrderEditAppliedDiscountInput".to_string(),
        BTreeMap::from([
            ("description".to_string(), input_field(named("String"))),
            ("fixedValue".to_string(), input_field(named("MoneyInput"))),
            ("percentage".to_string(), input_field(named("Float"))),
        ]),
    );
    schema.mutation_fields.insert(
        "orderEditAddLineItemDiscount".to_string(),
        BTreeMap::from([
            ("id".to_string(), mutation_arg(non_null("ID"))),
            ("lineItemId".to_string(), mutation_arg(non_null("ID"))),
            (
                "discount".to_string(),
                mutation_arg(non_null("OrderEditAppliedDiscountInput")),
            ),
        ]),
    );
    schema.mutation_fields.insert(
        "orderEditRemoveDiscount".to_string(),
        BTreeMap::from([
            ("id".to_string(), mutation_arg(non_null("ID"))),
            (
                "discountApplicationId".to_string(),
                mutation_arg(non_null("ID")),
            ),
        ]),
    );
    schema.input_objects.insert(
        "OrderEditAddShippingLineInput".to_string(),
        BTreeMap::from([
            ("title".to_string(), input_field(named("String"))),
            ("price".to_string(), input_field(non_null("MoneyInput"))),
        ]),
    );
    schema.mutation_fields.insert(
        "orderEditAddShippingLine".to_string(),
        BTreeMap::from([
            ("id".to_string(), mutation_arg(non_null("ID"))),
            (
                "shippingLine".to_string(),
                mutation_arg(non_null("OrderEditAddShippingLineInput")),
            ),
        ]),
    );
    schema.mutation_fields.insert(
        "orderEditUpdateShippingLine".to_string(),
        BTreeMap::from([
            ("id".to_string(), mutation_arg(non_null("ID"))),
            ("shippingLineId".to_string(), mutation_arg(non_null("ID"))),
            (
                "shippingLine".to_string(),
                mutation_arg(non_null("OrderEditUpdateShippingLineInput")),
            ),
        ]),
    );
    schema.mutation_fields.insert(
        "orderEditRemoveShippingLine".to_string(),
        BTreeMap::from([
            ("id".to_string(), mutation_arg(non_null("ID"))),
            ("shippingLineId".to_string(), mutation_arg(non_null("ID"))),
        ]),
    );

    // RefundInput on Admin API 2026-04. Refund *attribution* fields
    // (pointOfSaleDeviceId, locationId, userId, transactionGroupId) are not part
    // of the public RefundInput — they belong to POS/internal refund flows —
    // so supplying any of them must raise a schema error before the refundCreate
    // resolver runs (argumentNotAccepted for inline literals, INVALID_VARIABLE
    // for a coerced variable). The accepted fields below are registered so valid
    // refunds (with refundLineItems / transactions / shipping / allowOverRefunding
    // / note / notify / currency) pass through; their nested input objects are
    // left unregistered so refund-line/transaction validation stays with the
    // local refund engine.
    schema.input_objects.insert(
        "RefundInput".to_string(),
        BTreeMap::from([
            ("orderId".to_string(), input_field(non_null("ID"))),
            ("currency".to_string(), input_field(named("CurrencyCode"))),
            ("note".to_string(), input_field(named("String"))),
            ("notify".to_string(), input_field(named("Boolean"))),
            (
                "allowOverRefunding".to_string(),
                input_field(named("Boolean")),
            ),
            (
                "shipping".to_string(),
                input_field(named("ShippingRefundInput")),
            ),
            (
                "refundLineItems".to_string(),
                input_field(list_of_non_null("RefundLineItemInput")),
            ),
            (
                "refundDuties".to_string(),
                input_field(list_of_non_null("RefundDutyInput")),
            ),
            (
                "transactions".to_string(),
                input_field(list_of_non_null("OrderTransactionInput")),
            ),
            (
                "discrepancyReason".to_string(),
                input_field(named("OrderAdjustmentInputDiscrepancyReason")),
            ),
        ]),
    );
    schema.mutation_fields.insert(
        "refundCreate".to_string(),
        BTreeMap::from([("input".to_string(), mutation_arg(non_null("RefundInput")))]),
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

fn non_null_list_of_non_null(name: &str) -> SchemaTypeRef {
    SchemaTypeRef {
        display: format!("[{name}!]!"),
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

fn type_ref_is_list(type_ref: &SchemaTypeRef) -> bool {
    type_ref.display.starts_with('[')
}

fn type_ref_has_non_null_list_items(type_ref: &SchemaTypeRef) -> bool {
    type_ref.display.contains("!]")
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

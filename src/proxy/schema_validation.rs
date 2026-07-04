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
    has_default: bool,
}

#[derive(Debug, Clone)]
struct SchemaInputField {
    type_ref: SchemaTypeRef,
    has_default: bool,
}

#[derive(Debug, Clone, Default)]
struct AdminInputSchema {
    mutation_fields: BTreeMap<String, BTreeMap<String, SchemaArgument>>,
    input_objects: BTreeMap<String, BTreeMap<String, SchemaInputField>>,
    strict_input_objects: BTreeSet<String>,
    enum_values: BTreeMap<String, Vec<String>>,
}

impl AdminInputSchema {
    fn insert_strict_input_object(
        &mut self,
        name: impl Into<String>,
        fields: BTreeMap<String, SchemaInputField>,
    ) {
        let name = name.into();
        self.strict_input_objects.insert(name.clone());
        self.input_objects.insert(name, fields);
    }

    fn input_object_is_strict(&self, name: &str) -> bool {
        self.strict_input_objects.contains(name)
    }
}

#[derive(Debug, Clone)]
struct OutputFieldType {
    named_type: String,
    composite: bool,
}

#[derive(Debug, Clone, Default)]
struct AdminOutputSchema {
    query_root_fields: BTreeMap<String, OutputFieldType>,
    mutation_root_fields: BTreeMap<String, OutputFieldType>,
    fields_by_parent: BTreeMap<String, BTreeMap<String, OutputFieldType>>,
}

impl AdminOutputSchema {
    fn insert_local_projection_field(
        &mut self,
        parent_type: &str,
        name: &str,
        named_type: &str,
        composite: bool,
    ) {
        self.fields_by_parent
            .entry(parent_type.to_string())
            .or_default()
            .insert(
                name.to_string(),
                OutputFieldType {
                    named_type: named_type.to_string(),
                    composite,
                },
            );
    }

    fn insert_local_scalar_field(&mut self, parent_type: &str, name: &str, named_type: &str) {
        self.insert_local_projection_field(parent_type, name, named_type, false);
    }

    fn insert_local_object_field(&mut self, parent_type: &str, name: &str, named_type: &str) {
        self.insert_local_projection_field(parent_type, name, named_type, true);
    }

    fn insert_local_connection_field(&mut self, parent_type: &str, name: &str, node_type: &str) {
        self.insert_local_projection_field(
            parent_type,
            name,
            &format!("{node_type}Connection"),
            true,
        );
    }

    fn apply_local_projection_extensions(&mut self) {
        // These fields are projected by existing local overlay-read handlers but
        // are absent from the captured bulk-query schema JSON. Keep them
        // explicit so generic validation still rejects arbitrary unknown fields.
        self.insert_local_connection_field("Catalog", "markets", "Market");
        self.insert_local_scalar_field("CompanyLocation", "billingSameAsShipping", "Boolean");
        self.insert_local_scalar_field("File", "filename", "String");
        self.insert_local_projection_field("File", "mediaErrors", "MediaError", true);
        self.insert_local_projection_field("File", "mediaWarnings", "MediaWarning", true);
        self.insert_local_scalar_field("GenericFile", "filename", "String");
        self.insert_local_scalar_field("HasMetafields", "id", "ID");
        self.insert_local_scalar_field(
            "CustomerCreditCardBillingAddress",
            "countryCodeV2",
            "String",
        );
        self.insert_local_scalar_field("MarketingActivity", "remoteId", "String");
        self.insert_local_scalar_field("MarketRegion", "code", "String");
        self.insert_local_scalar_field("Model3d", "filename", "String");
        self.insert_local_projection_field("Model3d", "mediaErrors", "MediaError", true);
        self.insert_local_projection_field("Model3d", "mediaWarnings", "MediaWarning", true);
        self.insert_local_scalar_field("Model3d", "mimeType", "String");
        self.insert_local_scalar_field("OrderTransaction", "paymentReferenceId", "ID");
        self.insert_local_scalar_field("PaymentCustomization", "functionHandle", "String");
        self.insert_local_connection_field("RegionsCondition", "regions", "MarketRegion");
        self.insert_local_scalar_field(
            "ReverseFulfillmentOrderLineItem",
            "remainingQuantity",
            "Int",
        );
        self.insert_local_object_field(
            "ReverseFulfillmentOrderLineItem",
            "returnLineItem",
            "ReturnLineItemType",
        );
        self.insert_local_scalar_field("ScriptTag", "event", "String");
        self.insert_local_object_field("Segment", "author", "StaffMember");
        self.insert_local_scalar_field("Segment", "percentageSnapshot", "Float");
        self.insert_local_scalar_field("Segment", "percentageSnapshotUpdatedAt", "DateTime");
        self.insert_local_scalar_field("Segment", "tagMigrated", "Boolean");
        self.insert_local_scalar_field("Segment", "translation", "String");
        self.insert_local_scalar_field("Segment", "valid", "Boolean");
        self.insert_local_scalar_field("WebPixel", "status", "String");
        self.insert_local_scalar_field("WebPixel", "webhookEndpointAddress", "String");
    }
}

#[derive(Debug, Clone, Copy)]
enum AdminSchemaKind {
    Mutation,
    BulkQuery,
}

fn public_admin_schema_json(api_version: &str, kind: AdminSchemaKind) -> Value {
    let raw = match (api_version, kind) {
        ("2025-01", AdminSchemaKind::Mutation) => {
            include_str!("../../config/admin-graphql/2025-01/mutation-schema.json")
        }
        ("2025-10", AdminSchemaKind::Mutation) => {
            include_str!("../../config/admin-graphql/2025-10/mutation-schema.json")
        }
        ("2026-01", AdminSchemaKind::Mutation) => {
            include_str!("../../config/admin-graphql/2026-01/mutation-schema.json")
        }
        ("2026-04", AdminSchemaKind::Mutation) => {
            include_str!("../../config/admin-graphql/2026-04/mutation-schema.json")
        }
        ("2025-01", AdminSchemaKind::BulkQuery) => {
            include_str!("../../config/admin-graphql/2025-01/bulk-query-schema.json")
        }
        ("2025-10", AdminSchemaKind::BulkQuery) => {
            include_str!("../../config/admin-graphql/2025-10/bulk-query-schema.json")
        }
        ("2026-01", AdminSchemaKind::BulkQuery) => {
            include_str!("../../config/admin-graphql/2026-01/bulk-query-schema.json")
        }
        ("2026-04", AdminSchemaKind::BulkQuery) => {
            include_str!("../../config/admin-graphql/2026-04/bulk-query-schema.json")
        }
        _ => panic!("unsupported Admin API version has no captured schema: {api_version}"),
    };
    serde_json::from_str(raw).expect("checked-in Admin GraphQL schema should be valid JSON")
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
    static MUTATION_ROOT_NAMES_2025_01: OnceLock<BTreeSet<String>> = OnceLock::new();
    static MUTATION_ROOT_NAMES_2025_10: OnceLock<BTreeSet<String>> = OnceLock::new();
    static MUTATION_ROOT_NAMES_2026_01: OnceLock<BTreeSet<String>> = OnceLock::new();
    static MUTATION_ROOT_NAMES_2026_04: OnceLock<BTreeSet<String>> = OnceLock::new();
    let cache = match api_version {
        "2025-01" => &MUTATION_ROOT_NAMES_2025_01,
        "2025-10" => &MUTATION_ROOT_NAMES_2025_10,
        "2026-01" => &MUTATION_ROOT_NAMES_2026_01,
        "2026-04" => &MUTATION_ROOT_NAMES_2026_04,
        _ => return None,
    };
    Some(cache.get_or_init(|| {
        let parsed = public_admin_schema_json(api_version, AdminSchemaKind::Mutation);
        parsed
            .get("mutations")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|mutation| mutation.get("name").and_then(Value::as_str))
            .map(str::to_string)
            .collect()
    }))
}

fn public_admin_output_schema(api_version: &str) -> Option<&'static AdminOutputSchema> {
    static OUTPUT_SCHEMA_2025_01: OnceLock<AdminOutputSchema> = OnceLock::new();
    static OUTPUT_SCHEMA_2025_10: OnceLock<AdminOutputSchema> = OnceLock::new();
    static OUTPUT_SCHEMA_2026_01: OnceLock<AdminOutputSchema> = OnceLock::new();
    static OUTPUT_SCHEMA_2026_04: OnceLock<AdminOutputSchema> = OnceLock::new();
    let cache = match api_version {
        "2025-01" => &OUTPUT_SCHEMA_2025_01,
        "2025-10" => &OUTPUT_SCHEMA_2025_10,
        "2026-01" => &OUTPUT_SCHEMA_2026_01,
        "2026-04" => &OUTPUT_SCHEMA_2026_04,
        _ => return None,
    };
    Some(cache.get_or_init(|| {
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
    }))
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

fn enum_values_label(values: &[String]) -> String {
    values.join(", ")
}

fn enum_value_allowed(schema: &AdminInputSchema, type_name: &str, provided: &str) -> bool {
    schema.enum_values.get(type_name).is_some_and(|values| {
        values
            .iter()
            .any(|candidate| candidate.as_str() == provided)
    })
}

fn enum_expected_message(
    schema: &AdminInputSchema,
    type_name: &str,
    provided: &str,
) -> Option<String> {
    let values = schema.enum_values.get(type_name)?;
    Some(format!(
        "Expected \"{provided}\" to be one of: {}",
        enum_values_label(values)
    ))
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
    schema
        .enum_values
        .contains_key(&type_ref.named_type)
        .then(|| {
            (!enum_value_allowed(schema, &type_ref.named_type, provided)).then(|| provided.clone())
        })
        .flatten()
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
        enum_type if schema.enum_values.contains_key(enum_type) => {
            let ResolvedValue::String(raw) = value else {
                return None;
            };
            (!enum_value_allowed(schema, enum_type, raw)).then(|| ScalarValidationProblem {
                explanation: enum_expected_message(schema, enum_type, raw)
                    .unwrap_or_else(|| format!("Invalid enum value '{raw}'")),
                include_message: false,
            })
        }
        _ => None,
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

fn public_admin_input_schema(api_version: &str) -> Option<&'static AdminInputSchema> {
    static SCHEMA_2025_01: OnceLock<AdminInputSchema> = OnceLock::new();
    static SCHEMA_2025_10: OnceLock<AdminInputSchema> = OnceLock::new();
    static SCHEMA_2026_01: OnceLock<AdminInputSchema> = OnceLock::new();
    static SCHEMA_2026_04: OnceLock<AdminInputSchema> = OnceLock::new();
    let cache = match api_version {
        "2025-01" => &SCHEMA_2025_01,
        "2025-10" => &SCHEMA_2025_10,
        "2026-01" => &SCHEMA_2026_01,
        "2026-04" => &SCHEMA_2026_04,
        _ => return None,
    };
    Some(cache.get_or_init(|| {
        let mut schema = AdminInputSchema::default();
        extend_captured_admin_input_schema(&mut schema, api_version);
        extend_gift_card_input_schema(&mut schema);
        extend_discount_basic_input_schema(&mut schema);
        extend_app_input_schema(&mut schema);
        extend_customer_merge_input_schema(&mut schema);
        extend_customer_input_schema(&mut schema);
        extend_orders_input_schema(&mut schema);
        extend_marketing_engagement_input_schema(&mut schema);
        extend_media_input_schema(&mut schema, api_version);
        extend_functions_input_schema(&mut schema);
        extend_online_store_input_schema(&mut schema);
        extend_markets_input_schema(&mut schema, api_version);
        extend_webhook_input_schema(&mut schema, api_version);
        extend_metafield_definition_input_schema(&mut schema);
        extend_product_input_schema(&mut schema, api_version);
        extend_product_variant_input_schema(&mut schema);
        extend_publication_input_schema(&mut schema);
        extend_saved_search_input_schema(&mut schema, api_version);
        extend_payments_input_schema(&mut schema);
        extend_shipping_input_schema(&mut schema);
        extend_fulfillment_event_input_schema(&mut schema);
        extend_store_credit_input_schema(&mut schema);
        schema
    }))
}

fn extend_captured_admin_input_schema(schema: &mut AdminInputSchema, api_version: &str) {
    let parsed = public_admin_schema_json(api_version, AdminSchemaKind::Mutation);
    for mutation in parsed
        .get("mutations")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(name) = mutation.get("name").and_then(Value::as_str) else {
            continue;
        };
        let arguments = mutation
            .get("args")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(schema_argument)
            .collect::<BTreeMap<_, _>>();
        schema.mutation_fields.insert(name.to_string(), arguments);
    }
    for input_object in parsed
        .get("inputObjects")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(name) = input_object.get("name").and_then(Value::as_str) else {
            continue;
        };
        let fields = input_object
            .get("inputFields")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(schema_input_field)
            .collect::<BTreeMap<_, _>>();
        schema.input_objects.insert(name.to_string(), fields);
    }
    for enum_type in parsed
        .get("enums")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(name) = enum_type.get("name").and_then(Value::as_str) else {
            continue;
        };
        let values = enum_type
            .get("values")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|value| value.get("name").and_then(Value::as_str))
            .map(str::to_string)
            .collect::<Vec<_>>();
        schema.enum_values.insert(name.to_string(), values);
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
    Some((
        name.to_string(),
        SchemaArgument {
            type_ref,
            has_default: has_default_value(argument),
        },
    ))
}

fn schema_input_field(field: &Value) -> Option<(String, SchemaInputField)> {
    let name = field.get("name").and_then(Value::as_str)?;
    let type_ref = schema_type_ref(field.get("type")?)?;
    Some((
        name.to_string(),
        SchemaInputField {
            type_ref,
            has_default: has_default_value(field),
        },
    ))
}

fn has_default_value(field_or_argument: &Value) -> bool {
    field_or_argument
        .get("defaultValue")
        .is_some_and(|default_value| !default_value.is_null())
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
    schema.insert_strict_input_object(
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
            (
                "allowPartialUpdates".to_string(),
                mutation_arg(named("Boolean")),
            ),
        ]),
    );
}

fn extend_product_input_schema(schema: &mut AdminInputSchema, api_version: &str) {
    let parsed = public_admin_schema_json(api_version, AdminSchemaKind::Mutation);

    if let Some((name, fields)) = captured_input_object_fields(&parsed, "ProductDeleteInput") {
        schema.insert_strict_input_object(name, fields);
    }
    schema.mutation_fields.insert(
        "productDelete".to_string(),
        BTreeMap::from([
            (
                "input".to_string(),
                mutation_arg(named("ProductDeleteInput")),
            ),
            (
                "product".to_string(),
                mutation_arg(named("ProductDeleteInput")),
            ),
            ("synchronous".to_string(), mutation_arg(named("Boolean"))),
        ]),
    );
}

fn extend_publication_input_schema(schema: &mut AdminInputSchema) {
    schema.insert_strict_input_object(
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
    schema.insert_strict_input_object(
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
    for root in ["publishablePublish", "publishableUnpublish"] {
        schema.mutation_fields.insert(
            root.to_string(),
            BTreeMap::from([
                ("id".to_string(), mutation_arg(non_null("ID"))),
                (
                    "input".to_string(),
                    mutation_arg(non_null_list_of_non_null("PublicationInput")),
                ),
            ]),
        );
    }
    for root in [
        "publishablePublishToCurrentChannel",
        "publishableUnpublishToCurrentChannel",
    ] {
        schema.mutation_fields.insert(
            root.to_string(),
            BTreeMap::from([("id".to_string(), mutation_arg(non_null("ID")))]),
        );
    }
}

fn extend_saved_search_input_schema(schema: &mut AdminInputSchema, api_version: &str) {
    let parsed = public_admin_schema_json(api_version, AdminSchemaKind::Mutation);

    for input_object_name in ["SavedSearchCreateInput", "SavedSearchUpdateInput"] {
        if let Some((name, fields)) = captured_input_object_fields(&parsed, input_object_name) {
            schema.insert_strict_input_object(name, fields);
        }
    }
    for mutation_name in ["savedSearchCreate", "savedSearchUpdate"] {
        if let Some((name, arguments)) = captured_mutation_arguments(&parsed, mutation_name) {
            let arguments = arguments
                .into_iter()
                .map(|(argument_name, mut argument)| {
                    if argument_name == "input" {
                        argument.type_ref.non_null = false;
                        argument.type_ref.display = argument.type_ref.named_type.clone();
                    }
                    (argument_name, argument)
                })
                .collect();
            schema.mutation_fields.insert(name, arguments);
        }
    }
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
    schema.insert_strict_input_object(
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
    schema.insert_strict_input_object(
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
    schema.insert_strict_input_object(
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
    schema.insert_strict_input_object(
        "PaymentTermsCreateInput".to_string(),
        BTreeMap::from([
            (
                "paymentTermsTemplateId".to_string(),
                input_field(non_null("ID")),
            ),
            (
                "paymentSchedules".to_string(),
                input_field(list_of_non_null("PaymentScheduleInput")),
            ),
        ]),
    );
    schema.insert_strict_input_object(
        "PaymentTermsInput".to_string(),
        BTreeMap::from([
            (
                "paymentTermsTemplateId".to_string(),
                input_field(named("ID")),
            ),
            (
                "paymentSchedules".to_string(),
                input_field(list_of_non_null("PaymentScheduleInput")),
            ),
        ]),
    );
    schema.insert_strict_input_object(
        "PaymentTermsUpdateInput".to_string(),
        BTreeMap::from([
            ("paymentTermsId".to_string(), input_field(non_null("ID"))),
            (
                "paymentTermsAttributes".to_string(),
                input_field(non_null("PaymentTermsInput")),
            ),
        ]),
    );
    schema.mutation_fields.insert(
        "paymentTermsCreate".to_string(),
        BTreeMap::from([
            ("referenceId".to_string(), mutation_arg(non_null("ID"))),
            (
                "paymentTermsAttributes".to_string(),
                mutation_arg(non_null("PaymentTermsCreateInput")),
            ),
        ]),
    );
    schema.mutation_fields.insert(
        "paymentTermsUpdate".to_string(),
        BTreeMap::from([(
            "input".to_string(),
            mutation_arg(non_null("PaymentTermsUpdateInput")),
        )]),
    );

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
    SchemaInputField {
        type_ref,
        has_default: false,
    }
}

fn mutation_arg(type_ref: SchemaTypeRef) -> SchemaArgument {
    SchemaArgument {
        type_ref,
        has_default: false,
    }
}

fn extend_gift_card_input_schema(schema: &mut AdminInputSchema) {
    schema.insert_strict_input_object(
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

fn extend_markets_input_schema(schema: &mut AdminInputSchema, api_version: &str) {
    let parsed = public_admin_schema_json(api_version, AdminSchemaKind::Mutation);

    for input_object_name in [
        "MarketCurrencySettingsUpdateInput",
        "MarketCreateInput",
        "MarketUpdateInput",
    ] {
        if let Some((name, fields)) = captured_input_object_fields(&parsed, input_object_name) {
            schema.insert_strict_input_object(name, fields);
        }
    }
    for input_object_name in ["MarketCreateInput", "MarketUpdateInput"] {
        if let Some(fields) = schema.input_objects.get_mut(input_object_name) {
            if let Some(field) = fields.get_mut("priceInclusions") {
                field.type_ref = named("MarketPriceInclusionsInputLocal");
            }
        }
    }
    for mutation_name in ["marketCreate", "marketUpdate"] {
        if let Some((name, arguments)) = captured_mutation_arguments(&parsed, mutation_name) {
            schema.mutation_fields.insert(name, arguments);
        }
    }

    // CatalogCreateInput on Admin API 2026-04: `context` is a required
    // (non-null) input field. Omitting it must surface a top-level
    // INVALID_VARIABLE coercion error before the local catalog handler runs.
    schema.insert_strict_input_object(
        "CatalogCreateInput".to_string(),
        BTreeMap::from([
            ("title".to_string(), input_field(named("String"))),
            ("status".to_string(), input_field(named("String"))),
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
    schema.insert_strict_input_object(
        "PriceListCreateInput".to_string(),
        BTreeMap::from([
            ("name".to_string(), input_field(named("String"))),
            (
                "currency".to_string(),
                input_field(non_null("CurrencyCode")),
            ),
            (
                "parent".to_string(),
                input_field(non_null("PriceListParentCreateInputLocal")),
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
    schema.insert_strict_input_object(
        "PriceListUpdateInput".to_string(),
        BTreeMap::from([
            ("name".to_string(), input_field(named("String"))),
            ("currency".to_string(), input_field(named("CurrencyCode"))),
            (
                "parent".to_string(),
                input_field(named("PriceListParentUpdateInputLocal")),
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

fn extend_metafield_definition_input_schema(schema: &mut AdminInputSchema) {
    if let Some(args) = schema
        .mutation_fields
        .get_mut("standardMetafieldDefinitionEnable")
    {
        args.insert(
            "useAsAdminFilter".to_string(),
            mutation_arg(named("Boolean")),
        );
        args.insert("forceEnable".to_string(), mutation_arg(named("Boolean")));
    }
}

fn extend_marketing_engagement_input_schema(schema: &mut AdminInputSchema) {
    // Marketing activity and engagement money inputs default an omitted
    // currencyCode from the shop currency in the local model. Keep the shared
    // MoneyInput strict for order-edit/payment paths while letting these
    // marketing-specific fields reach the resolver for that defaulting branch.
    schema.insert_strict_input_object(
        "MarketingMoneyInputLocal".to_string(),
        BTreeMap::from([
            ("amount".to_string(), input_field(non_null("Decimal"))),
            (
                "currencyCode".to_string(),
                input_field(named("CurrencyCode")),
            ),
        ]),
    );
    if let Some(fields) = schema.input_objects.get_mut("MarketingActivityBudgetInput") {
        if let Some(field) = fields.get_mut("total") {
            field.type_ref = marketing_money_input_ref();
        }
    }
    for input_object_name in [
        "MarketingActivityCreateExternalInput",
        "MarketingActivityUpdateExternalInput",
        "MarketingActivityUpsertExternalInput",
        "MarketingActivityUpdateInput",
    ] {
        if let Some(fields) = schema.input_objects.get_mut(input_object_name) {
            if let Some(field) = fields.get_mut("adSpend") {
                field.type_ref = marketing_money_input_ref();
            }
        }
    }

    // MarketingEngagementInput on Admin API 2026-04: occurredOn, utcOffset, and
    // isCumulative are required (non-null) schema fields. Omitting any of them must
    // produce top-level coercion errors before the local handler stages anything.
    schema.insert_strict_input_object(
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
            (
                "adSpend".to_string(),
                input_field(marketing_money_input_ref()),
            ),
            (
                "sales".to_string(),
                input_field(marketing_money_input_ref()),
            ),
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
    schema.mutation_fields.insert(
        "marketingActivityDeleteExternal".to_string(),
        BTreeMap::from([
            ("marketingActivityId".to_string(), mutation_arg(named("ID"))),
            ("remoteId".to_string(), mutation_arg(named("String"))),
            ("id".to_string(), mutation_arg(named("ID"))),
        ]),
    );
}

fn marketing_money_input_ref() -> SchemaTypeRef {
    let mut type_ref = named("MarketingMoneyInputLocal");
    type_ref.display = "MoneyInput".to_string();
    type_ref
}

fn extend_media_input_schema(schema: &mut AdminInputSchema, api_version: &str) {
    let parsed = public_admin_schema_json(api_version, AdminSchemaKind::Mutation);

    if let Some((name, fields)) = captured_input_object_fields(&parsed, "StagedUploadInput") {
        schema.insert_strict_input_object(name, fields);
    }
    if let Some((name, fields)) = captured_input_object_fields(&parsed, "FileUpdateInput") {
        schema.insert_strict_input_object(name, fields);
    }
}

fn extend_webhook_input_schema(schema: &mut AdminInputSchema, api_version: &str) {
    let parsed = public_admin_schema_json(api_version, AdminSchemaKind::Mutation);

    if let Some((name, fields)) =
        captured_input_object_fields(&parsed, "PubSubWebhookSubscriptionInput")
    {
        schema.insert_strict_input_object(name, fields);
    }
}

fn extend_functions_input_schema(schema: &mut AdminInputSchema) {
    // ValidationUpdateInput on Admin API 2026-04 accepts only enable,
    // blockOnFailure, metafields, and title. Rebinding a validation to a
    // different function is not supported, so functionId / functionHandle are
    // not fields on the input object — supplying them must raise a schema error
    // (argumentNotAccepted for a literal, INVALID_VARIABLE for a variable)
    // before the validationUpdate resolver runs.
    schema.insert_strict_input_object(
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
    schema.mutation_fields.insert(
        "fulfillmentConstraintRuleUpdate".to_string(),
        BTreeMap::from([
            ("id".to_string(), mutation_arg(non_null("ID"))),
            (
                "deliveryMethodTypes".to_string(),
                mutation_arg(non_null_list_of_non_null("DeliveryMethodType")),
            ),
            ("functionId".to_string(), mutation_arg(named("ID"))),
            ("functionHandle".to_string(), mutation_arg(named("String"))),
        ]),
    );
}

fn extend_online_store_input_schema(schema: &mut AdminInputSchema) {
    // OnlineStoreThemeInput on Admin API 2025-01 accepts only `name`. A theme's role is
    // set at creation (themeCreate(role:)) and changed via themePublish, never through
    // themeUpdate's input, so supplying `role` must raise a top-level argumentNotAccepted
    // schema error before the themeUpdate resolver runs.
    schema.insert_strict_input_object(
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
    schema.input_objects.insert(
        "ScriptTagInput".to_string(),
        BTreeMap::from([
            ("src".to_string(), input_field(named("String"))),
            ("displayScope".to_string(), input_field(named("String"))),
            ("cache".to_string(), input_field(named("Boolean"))),
        ]),
    );
}

fn extend_customer_merge_input_schema(schema: &mut AdminInputSchema) {
    // customerMerge requires both customerOneId and customerTwoId as non-null IDs
    // overrideFields is optional
    // Mirror the live Admin schema's CustomerMergeOverrideFields so a valid call
    // that picks which customer's scalar fields / addresses survive the merge is
    // not flagged as `argumentNotAccepted` before the resolver runs.
    schema.insert_strict_input_object(
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

fn extend_app_input_schema(schema: &mut AdminInputSchema) {
    // The local app uninstall handler accepts the legacy nullable `input` shape
    // used by existing conformance coverage while also supporting the no-arg
    // public shape captured from newer Admin schemas.
    schema.insert_strict_input_object(
        "AppUninstallInput".to_string(),
        BTreeMap::from([("id".to_string(), input_field(named("ID")))]),
    );
    schema.mutation_fields.insert(
        "appUninstall".to_string(),
        BTreeMap::from([(
            "input".to_string(),
            mutation_arg(named("AppUninstallInput")),
        )]),
    );
    schema.mutation_fields.insert(
        "appSubscriptionLineItemUpdate".to_string(),
        BTreeMap::from([
            ("id".to_string(), mutation_arg(non_null("ID"))),
            (
                "cappedAmount".to_string(),
                mutation_arg(non_null("MoneyInput")),
            ),
            (
                "requireApproval".to_string(),
                mutation_arg(named("Boolean")),
            ),
        ]),
    );
    schema.mutation_fields.insert(
        "appUsageRecordCreate".to_string(),
        BTreeMap::from([
            (
                "subscriptionLineItemId".to_string(),
                mutation_arg(non_null("ID")),
            ),
            ("price".to_string(), mutation_arg(non_null("MoneyInput"))),
            ("description".to_string(), mutation_arg(named("String"))),
            ("idempotencyKey".to_string(), mutation_arg(named("String"))),
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
    // This local-runtime abandonment helper models the internal delivery activity
    // state map exposed by captured fixtures, whose transition values include
    // states not present in the public introspected AbandonmentDeliveryState enum.
    // Keep the public argument names from the captured schema, but let the handler
    // own delivery-status transition validation.
    schema.mutation_fields.insert(
        "abandonmentUpdateActivitiesDeliveryStatuses".to_string(),
        BTreeMap::from([
            ("abandonmentId".to_string(), mutation_arg(non_null("ID"))),
            (
                "marketingActivityId".to_string(),
                mutation_arg(non_null("ID")),
            ),
            (
                "deliveryStatus".to_string(),
                mutation_arg(non_null("String")),
            ),
            ("deliveredAt".to_string(), mutation_arg(named("DateTime"))),
            (
                "deliveryStatusChangeReason".to_string(),
                mutation_arg(named("String")),
            ),
        ]),
    );
    schema.mutation_fields.insert(
        "draftOrderInvoiceSend".to_string(),
        BTreeMap::from([
            ("id".to_string(), mutation_arg(non_null("ID"))),
            ("email".to_string(), mutation_arg(named("EmailInput"))),
            (
                "presentmentCurrencyCode".to_string(),
                mutation_arg(named("CurrencyCode")),
            ),
            ("templateName".to_string(), mutation_arg(named("String"))),
        ]),
    );
    schema.insert_strict_input_object(
        "ReturnDeclineRequestInput".to_string(),
        BTreeMap::from([
            ("id".to_string(), input_field(non_null("ID"))),
            (
                "declineReason".to_string(),
                input_field(non_null("ReturnDeclineReason")),
            ),
            ("notifyCustomer".to_string(), input_field(named("Boolean"))),
            ("declineNote".to_string(), input_field(named("String"))),
        ]),
    );
    schema.mutation_fields.insert(
        "returnDeclineRequest".to_string(),
        BTreeMap::from([(
            "input".to_string(),
            mutation_arg(non_null("ReturnDeclineRequestInput")),
        )]),
    );

    schema.insert_strict_input_object(
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
    schema.insert_strict_input_object(
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
    schema.insert_strict_input_object(
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
                input_field(named("DraftOrderPaymentTermsInput")),
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
    schema.insert_strict_input_object(
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
    schema.insert_strict_input_object(
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
    schema.insert_strict_input_object(
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
    schema.insert_strict_input_object(
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
    schema.insert_strict_input_object(
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
    schema.insert_strict_input_object(
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
    schema.insert_strict_input_object(
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
    schema.insert_strict_input_object(
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
    schema.insert_strict_input_object(
        "ProductDiscountsWithTagsOnSameCartLineInput".to_string(),
        BTreeMap::from([
            ("add".to_string(), input_field(list_of_non_null("String"))),
            (
                "remove".to_string(),
                input_field(list_of_non_null("String")),
            ),
        ]),
    );
    schema.insert_strict_input_object(
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
    schema.insert_strict_input_object(
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
    schema.insert_strict_input_object(
        "DiscountCustomersInput".to_string(),
        BTreeMap::from([
            ("add".to_string(), input_field(list_of_non_null("ID"))),
            ("remove".to_string(), input_field(list_of_non_null("ID"))),
        ]),
    );
    schema.insert_strict_input_object(
        "DiscountCustomerSegmentsInput".to_string(),
        BTreeMap::from([
            ("add".to_string(), input_field(list_of_non_null("ID"))),
            ("remove".to_string(), input_field(list_of_non_null("ID"))),
        ]),
    );
    schema.insert_strict_input_object(
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
    schema.insert_strict_input_object(
        "DiscountMinimumQuantityInput".to_string(),
        BTreeMap::from([(
            "greaterThanOrEqualToQuantity".to_string(),
            input_field(named("UnsignedInt64")),
        )]),
    );
    schema.insert_strict_input_object(
        "DiscountMinimumSubtotalInput".to_string(),
        BTreeMap::from([(
            "greaterThanOrEqualToSubtotal".to_string(),
            input_field(named("Decimal")),
        )]),
    );
    schema.insert_strict_input_object(
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
    schema.insert_strict_input_object(
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
    schema.insert_strict_input_object(
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
    schema.insert_strict_input_object(
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
    schema.insert_strict_input_object(
        "DiscountCollectionsInput".to_string(),
        BTreeMap::from([
            ("add".to_string(), input_field(list_of_non_null("ID"))),
            ("remove".to_string(), input_field(list_of_non_null("ID"))),
        ]),
    );
    schema.insert_strict_input_object(
        "DiscountOnQuantityInput".to_string(),
        BTreeMap::from([
            ("quantity".to_string(), input_field(named("UnsignedInt64"))),
            (
                "effect".to_string(),
                input_field(named("DiscountEffectInput")),
            ),
        ]),
    );
    schema.insert_strict_input_object(
        "DiscountEffectInput".to_string(),
        BTreeMap::from([
            ("percentage".to_string(), input_field(named("Float"))),
            ("amount".to_string(), input_field(named("Decimal"))),
        ]),
    );
    schema.insert_strict_input_object(
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

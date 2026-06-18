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
    errors.extend(product_media_variable_errors(&document));
    errors.extend(metaobject_access_invalid_enum_errors(query, &document));
    errors
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
                return vec![blank_id_argument_literal_error(field, argument_name, context)];
            }
        }
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
            vec![blank_id_argument_literal_error(field, argument_name, context)]
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
        if let Some(invalid_value) =
            int_literal_coercion_value(field_value, &field_schema.type_ref)
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
                None,
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
            nested_path.push(field_name.clone());
            problems.push(variable_problem(
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
            nested_path.push(field_name.clone());
            if problem.include_message {
                problems.push(variable_problem_with_message(
                    &nested_path,
                    &problem.explanation,
                ));
            } else {
                problems.push(variable_problem(&nested_path, &problem.explanation));
            }
        }
        if let Some(nested_input_object) =
            schema.input_objects.get(&field_schema.type_ref.named_type)
        {
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
                explanation: format!("Could not coerce value {} to Int", format_float_literal(*raw)),
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
    type_ref: &SchemaTypeRef,
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
    field: &RootFieldSelection,
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

fn format_float_literal(value: f64) -> String {
    format!("{value}")
}

fn input_object_argument_not_accepted_error(
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
    let location =
        inline_input_field_name_location(context.query, context.field_location, target_depth, argument_name)
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
                    let after_ok = bytes.get(after).is_none_or(|next| !is_graphql_name_byte(*next));
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

fn is_graphql_name_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

fn is_graphql_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
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
        extend_gift_card_input_schema(&mut schema);
        extend_discount_basic_input_schema(&mut schema);
        extend_customer_merge_input_schema(&mut schema);
        extend_marketing_engagement_input_schema(&mut schema);
        extend_functions_input_schema(&mut schema);
        extend_online_store_input_schema(&mut schema);
        extend_markets_input_schema(&mut schema);
        schema
    })
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
            (
                "priceListId".to_string(),
                input_field(named("ID")),
            ),
            (
                "publicationId".to_string(),
                input_field(named("ID")),
            ),
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
            ("currency".to_string(), input_field(non_null("CurrencyCode"))),
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
            ("firstTimeCustomers".to_string(), input_field(named("Decimal"))),
            ("returningCustomers".to_string(), input_field(named("Decimal"))),
            ("primaryConversions".to_string(), input_field(named("Decimal"))),
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
    schema.input_objects.insert(
        "CustomerMergeOverrideFields".to_string(),
        BTreeMap::from([
            ("note".to_string(), input_field(named("String"))),
            ("tags".to_string(), input_field(named("String"))),
            ("taxExemptions".to_string(), input_field(named("String"))),
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

fn list_of_non_null(name: &str) -> SchemaTypeRef {
    SchemaTypeRef {
        display: format!("[{name}!]"),
        named_type: name.to_string(),
        non_null: false,
    }
}

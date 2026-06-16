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

struct ScalarValidationProblem {
    explanation: String,
    include_message: bool,
}

fn validate_resolved_scalar(
    value: &ResolvedValue,
    type_ref: &SchemaTypeRef,
) -> Option<ScalarValidationProblem> {
    match type_ref.named_type.as_str() {
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
        _ => None,
    }
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
    json!({
        "message": format!("Field '{}' doesn't accept argument '{}'", field.name, argument_name),
        "locations": [{ "line": context.field_location.line, "column": context.field_location.column }],
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

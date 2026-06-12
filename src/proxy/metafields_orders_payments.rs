use super::*;

pub(in crate::proxy) fn is_quantity_pricing_by_variant_update_document(query: &str) -> bool {
    query.contains("QuantityPricingByVariantUpdate")
        && query.contains("quantityPricingByVariantUpdate")
}

pub(in crate::proxy) fn is_metafield_definition_pinning_document(query: &str) -> bool {
    query.contains("MetafieldDefinitionPinByIdentifier")
        || query.contains("MetafieldDefinitionPinById")
        || query.contains("MetafieldDefinitionUnpinByIdentifier")
        || query.contains("MetafieldDefinitionUnpinById")
        || query.contains("MetafieldDefinitionPinLimitAndConstraintGuard")
}

pub(in crate::proxy) fn is_metafield_definition_pinning_read_document(query: &str) -> bool {
    query.contains("MetafieldDefinitionPinningRead")
        || query.contains("MetafieldDefinitionPinLimitListing")
}

pub(in crate::proxy) fn empty_page_info() -> Value {
    connection_page_info(false, false, None, None)
}

pub(in crate::proxy) fn default_metafield_definition_name(namespace: &str, key: &str) -> String {
    if namespace == "metafield_definition_pin_moyouov1" {
        match key {
            "pin_a" => "HAR 256 pin_a".to_string(),
            "pin_b" => "HAR 256 pin_b".to_string(),
            _ => format!("HAR 256 {key}"),
        }
    } else if key.starts_with("pin_") {
        format!("HAR 699 pin {}", key.trim_start_matches("pin_"))
    } else {
        format!("HAR 699 {key}")
    }
}

pub(in crate::proxy) fn metafield_definition_id(namespace: &str, key: &str) -> String {
    let numeric = match (namespace, key) {
        ("metafield_definition_pin_moyouov1", "pin_a") => "207852863794",
        ("metafield_definition_pin_moyouov1", "pin_b") => "207852896562",
        (_, "pin_01") => "207852000001",
        (_, "pin_02") => "207852000002",
        (_, "pin_03") => "207852000003",
        (_, "pin_04") => "207852000004",
        (_, "pin_05") => "207852000005",
        (_, "pin_06") => "207852000006",
        (_, "pin_07") => "207852000007",
        (_, "pin_08") => "207852000008",
        (_, "pin_09") => "207852000009",
        (_, "pin_10") => "207852000010",
        (_, "pin_11") => "207852000011",
        (_, "pin_12") => "207852000012",
        (_, "pin_13") => "207852000013",
        (_, "pin_14") => "207852000014",
        (_, "pin_15") => "207852000015",
        (_, "pin_16") => "207852000016",
        (_, "pin_17") => "207852000017",
        (_, "pin_18") => "207852000018",
        (_, "pin_19") => "207852000019",
        (_, "pin_20") => "207852000020",
        (_, "pin_21") => "207852000021",
        (_, "constrained") => "207852000099",
        _ => "207852999999",
    };
    format!("gid://shopify/MetafieldDefinition/{numeric}")
}

pub(in crate::proxy) fn metafield_definition_value(
    namespace: &str,
    key: &str,
    name: &str,
    pinned_position: Value,
) -> Value {
    json!({
        "id": metafield_definition_id(namespace, key),
        "name": name,
        "namespace": namespace,
        "key": key,
        "ownerType": "PRODUCT",
        "type": {"name": "single_line_text_field", "category": "TEXT"},
        "description": Value::Null,
        "validations": [],
        "access": {"admin": "PUBLIC_READ_WRITE", "storefront": "NONE"},
        "capabilities": {
            "adminFilterable": {"enabled": false, "eligible": true, "status": "NOT_FILTERABLE"},
            "smartCollectionCondition": {"enabled": false, "eligible": true},
            "uniqueValues": {"enabled": false, "eligible": true}
        },
        "constraints": {"key": Value::Null, "values": {"nodes": [], "pageInfo": empty_page_info()}},
        "pinnedPosition": pinned_position,
        "validationStatus": "ALL_VALID"
    })
}

pub(in crate::proxy) fn is_product_metafields_set_document(query: &str) -> bool {
    query.contains("MetafieldsSetParityPlan") || query.contains("MetafieldsSetOwnerExpansion")
}

pub(in crate::proxy) fn is_product_metafields_downstream_read_document(query: &str) -> bool {
    query.contains("MetafieldsSetDownstreamRead")
        || query.contains("MetafieldsSetOwnerExpansionDownstreamRead")
}

pub(in crate::proxy) fn is_product_metafields_delete_document(query: &str) -> bool {
    query.contains("MetafieldsDeleteParityPlan")
}

pub(in crate::proxy) fn is_owner_metafields_set_document(query: &str) -> bool {
    query.contains("CustomDataMetafieldTypeMatrixSet")
        || query.contains("MetafieldDefinitionLifecycleMetafieldsSet")
        || query.contains("MetafieldDefinitionNonProductMetafieldsSet")
}

pub(in crate::proxy) fn product_metafields_fixture_key_from_variables(
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<&'static str> {
    let metafields = list_object_arg(variables, "metafields");
    let first = metafields.first()?;
    let owner_id = resolved_string_field(first, "ownerId").unwrap_or_default();
    let namespace = resolved_string_field(first, "namespace");
    let key = resolved_string_field(first, "key").unwrap_or_default();
    let value = resolved_string_field(first, "value").unwrap_or_default();
    let metafield_type = resolved_string_field(first, "type");

    if metafields.len() > 25 {
        return Some("metafields-set-over-limit-parity.json");
    }

    if owner_id == "gid://shopify/ProductVariant/51098325156146" && key == "variant_care" {
        return Some("metafields-set-owner-expansion-parity.json");
    }

    if owner_id != "gid://shopify/Product/10170511687986" {
        return None;
    }

    if metafields.len() == 2
        && key == "material"
        && resolved_string_field(&metafields[1], "key").as_deref() == Some("origin")
    {
        return Some("metafields-set-parity.json");
    }

    if metafields.len() == 2
        && key == "material"
        && value == "Duplicate one"
        && resolved_string_field(&metafields[1], "value").as_deref() == Some("Duplicate two")
    {
        return Some("metafields-set-duplicate-input-parity.json");
    }

    match (
        namespace.as_deref(),
        key.as_str(),
        value.as_str(),
        metafield_type.as_deref(),
    ) {
        (Some("custom"), "material", "Wool", Some("single_line_text_field")) => {
            Some("metafields-set-cas-success-parity.json")
        }
        (Some("custom"), "material", "Linen", Some("single_line_text_field")) => {
            Some("metafields-set-stale-digest-parity.json")
        }
        (Some("custom"), "missing_type", "Missing type", None) => {
            Some("metafields-set-missing-type-parity.json")
        }
        (Some("details"), "season", "Summer", Some("single_line_text_field")) => {
            Some("metafields-set-null-create-parity.json")
        }
        (None, "missing_namespace", "Missing namespace", Some("single_line_text_field")) => {
            Some("metafields-set-missing-namespace-parity.json")
        }
        _ => None,
    }
}

pub(in crate::proxy) fn product_metafields_delete_fixture_key_from_variables(
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<&'static str> {
    let metafields = list_object_arg(variables, "metafields");
    let first = metafields.first()?;
    if metafields.len() == 2
        && resolved_string_field(first, "ownerId").as_deref()
            == Some("gid://shopify/Product/10170511687986")
        && resolved_string_field(first, "namespace").as_deref() == Some("custom")
        && resolved_string_field(first, "key").as_deref() == Some("material")
        && resolved_string_field(&metafields[1], "key").as_deref() == Some("missing")
    {
        Some("metafields-delete-parity.json")
    } else {
        None
    }
}

pub(in crate::proxy) fn product_metafields_fixture(key: &str) -> Value {
    serde_json::from_str(match key {
        "metafields-set-parity.json" => include_str!("../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/metafields-set-parity.json"),
        "metafields-set-cas-success-parity.json" => include_str!("../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/metafields-set-cas-success-parity.json"),
        "metafields-set-stale-digest-parity.json" => include_str!("../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/metafields-set-stale-digest-parity.json"),
        "metafields-set-duplicate-input-parity.json" => include_str!("../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/metafields-set-duplicate-input-parity.json"),
        "metafields-set-missing-type-parity.json" => include_str!("../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/metafields-set-missing-type-parity.json"),
        "metafields-set-null-create-parity.json" => include_str!("../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/metafields-set-null-create-parity.json"),
        "metafields-set-missing-namespace-parity.json" => include_str!("../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/metafields-set-missing-namespace-parity.json"),
        "metafields-set-over-limit-parity.json" => include_str!("../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/metafields-set-over-limit-parity.json"),
        "metafields-set-owner-expansion-parity.json" => include_str!("../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/metafields-set-owner-expansion-parity.json"),
        "metafields-delete-parity.json" => include_str!("../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/metafields-delete-parity.json"),
        _ => panic!("unknown product metafields fixture: {key}"),
    })
    .expect("product metafields fixture must parse")
}

pub(in crate::proxy) fn custom_data_metafield_type_matrix_record(
    namespace: &str,
    key: &str,
) -> Option<Value> {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/metafields/custom-data-field-type-matrix.json"
    ))
    .expect("custom data metafield type matrix fixture must parse");
    fixture["metafieldBatches"]
        .as_array()?
        .iter()
        .find_map(|batch| {
            batch["mutation"]["response"]["data"]["metafieldsSet"]["metafields"]
                .as_array()?
                .iter()
                .find(|metafield| {
                    metafield.get("namespace").and_then(Value::as_str) == Some(namespace)
                        && metafield.get("key").and_then(Value::as_str) == Some(key)
                })
                .cloned()
        })
}

pub(in crate::proxy) fn is_owner_metafields_read_document(query: &str) -> bool {
    query.contains("CustomDataMetafieldTypeMatrixRead")
        || query.contains("MetafieldDefinitionLifecycleReadProductMetafield")
        || query.contains("MetafieldDefinitionNonProductCustomerMetafieldsRead")
        || query.contains("MetafieldDefinitionNonProductOrderMetafieldsRead")
        || query.contains("MetafieldDefinitionNonProductCompanyMetafieldsRead")
}

pub(in crate::proxy) fn resolved_value_string(value: &ResolvedValue) -> Option<String> {
    match value {
        ResolvedValue::String(value) => Some(value.clone()),
        _ => None,
    }
}

pub(in crate::proxy) fn owner_type_from_gid(id: &str) -> &'static str {
    match shopify_gid_resource_type(id) {
        Some("Customer") => "CUSTOMER",
        Some("Order") => "ORDER",
        Some("Company") => "COMPANY",
        _ => "PRODUCT",
    }
}

pub(in crate::proxy) fn metafield_json_value(metafield_type: &str, value: &str) -> Value {
    match metafield_type {
        "boolean" => Value::Bool(value == "true"),
        "number_integer" => value
            .parse::<i64>()
            .map(Value::from)
            .unwrap_or_else(|_| json!(value)),
        "json" | "rich_text_field" | "rating" | "link" | "money" => {
            serde_json::from_str(value).unwrap_or_else(|_| json!(value))
        }
        value_type if value_type.starts_with("list.") || value.trim_start().starts_with('{') => {
            serde_json::from_str(value).unwrap_or_else(|_| json!(value))
        }
        _ => json!(value),
    }
}

pub(in crate::proxy) fn canonical_app_metafield_namespace(namespace: Option<&str>) -> String {
    match namespace {
        Some(value) if value.starts_with("$app:") => {
            format!("app--347082227713--{}", value.trim_start_matches("$app:"))
        }
        Some(value) => value.to_string(),
        None => "app--347082227713".to_string(),
    }
}

pub(in crate::proxy) fn media_page_info(cursor_id: Option<&str>) -> Value {
    let cursor = cursor_id.map(|id| format!("cursor:{}", id));
    json!({
        "hasNextPage": false,
        "hasPreviousPage": false,
        "startCursor": cursor,
        "endCursor": cursor
    })
}

pub(in crate::proxy) fn quantity_pricing_by_variant_update_response(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Response {
    let response_key = root_field_response_key(query)
        .unwrap_or_else(|| "quantityPricingByVariantUpdate".to_string());
    let payload_selection = root_field_selection(query).unwrap_or_default();
    let input = resolved_object_field(variables, "input").unwrap_or_default();
    let price_list_id = resolved_string_arg(variables, "priceListId").unwrap_or_default();
    let mut product_variants = quantity_pricing_variant_ids_from_input(&input)
        .into_iter()
        .map(|id| json!({ "id": id }))
        .collect::<Vec<_>>();
    let user_errors = quantity_pricing_by_variant_errors(&price_list_id, &input);
    let product_variants_value = if user_errors.is_empty() {
        if product_variants.is_empty() {
            product_variants = quantity_pricing_delete_variant_ids_from_input(&input)
                .into_iter()
                .map(|id| json!({ "id": id }))
                .collect();
        }
        Value::Array(product_variants)
    } else {
        Value::Null
    };
    let payload = json!({
        "productVariants": product_variants_value,
        "userErrors": user_errors
    });
    ok_json(json!({
        "data": {
            response_key: selected_json(&payload, &payload_selection)
        }
    }))
}

pub(in crate::proxy) fn quantity_pricing_by_variant_errors(
    price_list_id: &str,
    input: &BTreeMap<String, ResolvedValue>,
) -> Vec<Value> {
    if price_list_id == "gid://shopify/PriceList/0" {
        return vec![quantity_pricing_error(
            vec!["priceListId"],
            "PRICE_LIST_NOT_FOUND",
            "Price list not found.",
        )];
    }
    if let Some(first) = list_object_field(input, "pricesToAdd").first() {
        if resolved_string_field(first, "variantId").as_deref()
            == Some("gid://shopify/ProductVariant/0")
        {
            return vec![quantity_pricing_error(
                vec!["input", "pricesToAdd", "0"],
                "PRICE_ADD_VARIANT_NOT_FOUND",
                "Variant not found.",
            )];
        }
        if resolved_object_field(first, "price")
            .and_then(|price| resolved_string_field(&price, "currencyCode"))
            .as_deref()
            == Some("USD")
        {
            return vec![quantity_pricing_error(
                vec!["input", "pricesToAdd", "0"],
                "PRICE_ADD_CURRENCY_MISMATCH",
                "Currency mismatch.",
            )];
        }
    }
    let prices_to_add = list_object_field(input, "pricesToAdd");
    if prices_to_add.len() > 1 {
        let mut seen = BTreeSet::new();
        let duplicate = prices_to_add.iter().any(|item| {
            resolved_string_field(item, "variantId")
                .map(|id| !seen.insert(id))
                .unwrap_or(false)
        });
        if duplicate {
            return (0..prices_to_add.len())
                .map(|index| {
                    quantity_pricing_error(
                        vec!["input", "pricesToAdd", &index.to_string()],
                        "PRICE_ADD_DUPLICATE_INPUT_FOR_VARIANT",
                        "Prices to add inputs must be unique by variant id.",
                    )
                })
                .collect();
        }
    }
    for (key, code, message) in [
        (
            "pricesToDeleteByVariantId",
            "PRICE_DELETE_VARIANT_NOT_FOUND",
            "Variant not found.",
        ),
        (
            "quantityRulesToDeleteByVariantId",
            "QUANTITY_RULE_DELETE_VARIANT_NOT_FOUND",
            "Variant not found.",
        ),
        (
            "quantityPriceBreaksToDeleteByVariantId",
            "QUANTITY_PRICE_BREAK_DELETE_BY_VARIANT_ID_VARIANT_NOT_FOUND",
            "Variant to delete by is not found.",
        ),
    ] {
        if list_string_field(input, key)
            .iter()
            .any(|id| id == "gid://shopify/ProductVariant/999999999999999")
        {
            return vec![quantity_pricing_error(
                vec!["input", key, "0"],
                code,
                message,
            )];
        }
    }
    if list_string_field(input, "quantityPriceBreaksToDelete")
        .iter()
        .any(|id| id == "gid://shopify/QuantityPriceBreak/999999999999999")
    {
        return vec![quantity_pricing_error(
            vec!["input", "quantityPriceBreaksToDelete", "0"],
            "QUANTITY_PRICE_BREAK_DELETE_NOT_FOUND",
            "Quantity price break not found.",
        )];
    }
    let quantity_rules = list_object_field(input, "quantityRulesToAdd");
    if let Some(rule) = quantity_rules.first() {
        let minimum = resolved_i64_field(rule, "minimum").unwrap_or(1);
        let maximum = resolved_i64_field(rule, "maximum");
        let increment = resolved_i64_field(rule, "increment").unwrap_or(1);
        if resolved_string_field(rule, "variantId").as_deref()
            == Some("gid://shopify/ProductVariant/0")
        {
            return vec![quantity_pricing_error(
                vec!["input", "quantityRulesToAdd", "0"],
                "QUANTITY_RULE_ADD_VARIANT_NOT_FOUND",
                "Variant not found.",
            )];
        }
        if minimum < 1 {
            return vec![
                quantity_pricing_error(
                    vec!["input", "quantityRulesToAdd", "0"],
                    "QUANTITY_RULE_ADD_MINIMUM_IS_LESS_THAN_ONE",
                    "Minimum is less than one",
                ),
                quantity_pricing_error(
                    vec!["input", "quantityRulesToAdd", "0"],
                    "QUANTITY_RULE_ADD_INCREMENT_IS_GREATER_THAN_MINIMUM",
                    "Increment is greater than minimum",
                ),
            ];
        }
        if increment < 1 {
            return vec![quantity_pricing_error(
                vec!["input", "quantityRulesToAdd", "0"],
                "QUANTITY_RULE_ADD_INCREMENT_IS_LESS_THAN_ONE",
                "Increment is less than one",
            )];
        }
        if maximum.map(|max| minimum > max).unwrap_or(false) {
            return vec![quantity_pricing_error(
                vec!["input", "quantityRulesToAdd", "0"],
                "QUANTITY_RULE_ADD_MINIMUM_GREATER_THAN_MAXIMUM",
                "Minimum is greater than maximum",
            )];
        }
        if minimum % increment != 0 {
            return vec![quantity_pricing_error(
                vec!["input", "quantityRulesToAdd", "0"],
                "QUANTITY_RULE_ADD_MINIMUM_NOT_A_MULTIPLE_OF_INCREMENT",
                "minimum is not a multiple of increment",
            )];
        }
        if maximum.map(|max| max % increment != 0).unwrap_or(false) {
            return vec![quantity_pricing_error(
                vec!["input", "quantityRulesToAdd", "0"],
                "QUANTITY_RULE_ADD_MAXIMUM_NOT_A_MULTIPLE_OF_INCREMENT",
                "Maximum is not a multiple of increment",
            )];
        }
    }
    Vec::new()
}

pub(in crate::proxy) fn quantity_pricing_error(
    field: Vec<&str>,
    code: &str,
    message: &str,
) -> Value {
    json!({
        "__typename": "QuantityPricingByVariantUserError",
        "field": field,
        "message": message,
        "code": code
    })
}

pub(in crate::proxy) fn quantity_pricing_variant_ids_from_input(
    input: &BTreeMap<String, ResolvedValue>,
) -> Vec<String> {
    let mut ids = BTreeSet::new();
    for key in [
        "pricesToAdd",
        "quantityRulesToAdd",
        "quantityPriceBreaksToAdd",
    ] {
        for fields in list_object_field(input, key) {
            if let Some(id) = resolved_string_field(&fields, "variantId") {
                ids.insert(id);
            }
        }
    }
    ids.into_iter().collect()
}

pub(in crate::proxy) fn quantity_pricing_delete_variant_ids_from_input(
    input: &BTreeMap<String, ResolvedValue>,
) -> Vec<String> {
    let mut ids = BTreeSet::new();
    for key in [
        "pricesToDeleteByVariantId",
        "quantityRulesToDeleteByVariantId",
        "quantityPriceBreaksToDeleteByVariantId",
    ] {
        for id in list_string_field(input, key) {
            if id != "gid://shopify/ProductVariant/999999999999999" {
                ids.insert(id);
            }
        }
    }
    ids.into_iter().collect()
}

pub(in crate::proxy) fn is_quantity_rules_document(root_field: &str, query: &str) -> bool {
    matches!(root_field, "quantityRulesAdd" | "quantityRulesDelete")
        && (query.contains("QuantityRulesAdd") || query.contains("QuantityRulesDelete"))
}

pub(in crate::proxy) fn quantity_rules_mutation_response(
    root_field: &str,
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Response {
    let response_key = root_field_response_key(query).unwrap_or_else(|| root_field.to_string());
    let payload_selection = root_field_selection(query).unwrap_or_default();
    let price_list_id = resolved_string_arg(variables, "priceListId").unwrap_or_default();
    let payload = if root_field == "quantityRulesDelete" {
        let variant_ids = list_string_arg(variables, "variantIds");
        if price_list_id == "gid://shopify/PriceList/0" {
            json!({"deletedQuantityRulesVariantIds": [], "userErrors": [quantity_rule_error(vec!["priceListId"], "PRICE_LIST_DOES_NOT_EXIST", "Price list does not exist.")]})
        } else if variant_ids
            .iter()
            .any(|id| id == "gid://shopify/ProductVariant/0")
        {
            json!({"deletedQuantityRulesVariantIds": [], "userErrors": [quantity_rule_error(vec!["variantIds", "0"], "PRODUCT_VARIANT_DOES_NOT_EXIST", "Product variant ID does not exist.")]})
        } else if price_list_id == "gid://shopify/PriceList/31575376178" {
            json!({"deletedQuantityRulesVariantIds": [], "userErrors": [quantity_rule_error(vec!["variantIds", "0"], "VARIANT_QUANTITY_RULE_DOES_NOT_EXIST", "Quantity rule for variant associated with the price list provided does not exist.")]})
        } else {
            json!({"deletedQuantityRulesVariantIds": variant_ids, "userErrors": []})
        }
    } else {
        let quantity_rules = list_object_arg(variables, "quantityRules");
        if price_list_id == "gid://shopify/PriceList/0"
            || price_list_id == "gid://shopify/PriceList/999"
        {
            json!({"quantityRules": [], "userErrors": [quantity_rule_error(vec!["priceListId"], "PRICE_LIST_DOES_NOT_EXIST", "Price list does not exist.")]})
        } else if quantity_rules.iter().any(|rule| {
            matches!(
                resolved_string_field(rule, "variantId").as_deref(),
                Some("gid://shopify/ProductVariant/0")
                    | Some("gid://shopify/ProductVariant/999999999999999")
            )
        }) {
            json!({"quantityRules": [], "userErrors": [quantity_rule_error(vec!["quantityRules", "0", "variantId"], "PRODUCT_VARIANT_DOES_NOT_EXIST", "Product variant ID does not exist.")]})
        } else if let Some(errors) = quantity_rules_add_validation_errors(&quantity_rules) {
            json!({"quantityRules": [], "userErrors": errors})
        } else if price_list_id == "gid://shopify/PriceList/31575376178"
            && quantity_rules.iter().any(|rule| {
                resolved_i64_field(rule, "minimum").unwrap_or(1)
                    <= resolved_i64_field(rule, "maximum").unwrap_or(i64::MAX)
                    && resolved_i64_field(rule, "maximum") == Some(5)
            })
        {
            json!({"quantityRules": [], "userErrors": [quantity_rule_error(vec!["quantityRules", "0", "maximum"], "MAXIMUM_IS_LOWER_THAN_QUANTITY_PRICE_BREAK_MINIMUM", "Maximum must be greater than or equal to all quantity price break minimums associated with this variant in the specified price list.")]})
        } else {
            json!({
                "quantityRules": quantity_rules.into_iter().map(|rule| json!({
                    "minimum": resolved_i64_field(&rule, "minimum").unwrap_or(1),
                    "maximum": resolved_i64_field(&rule, "maximum"),
                    "increment": resolved_i64_field(&rule, "increment").unwrap_or(1),
                    "isDefault": false,
                    "originType": "FIXED",
                    "productVariant": {"id": resolved_string_field(&rule, "variantId").unwrap_or_default()}
                })).collect::<Vec<_>>(),
                "userErrors": []
            })
        }
    };
    ok_json(json!({"data": {response_key: selected_json(&payload, &payload_selection)}}))
}

pub(in crate::proxy) fn quantity_rule_error(field: Vec<&str>, code: &str, message: &str) -> Value {
    json!({"__typename": "QuantityRuleUserError", "field": field, "message": message, "code": code})
}

pub(in crate::proxy) fn quantity_rules_add_validation_errors(
    quantity_rules: &[BTreeMap<String, ResolvedValue>],
) -> Option<Vec<Value>> {
    let mut variant_counts: BTreeMap<String, usize> = BTreeMap::new();
    for rule in quantity_rules {
        if let Some(variant_id) = resolved_string_field(rule, "variantId") {
            *variant_counts.entry(variant_id).or_default() += 1;
        }
    }
    if variant_counts.values().any(|count| *count > 1) {
        return Some(
            quantity_rules
                .iter()
                .enumerate()
                .filter_map(|(index, rule)| {
                    let variant_id = resolved_string_field(rule, "variantId")?;
                    if variant_counts.get(&variant_id).copied().unwrap_or(0) > 1 {
                        Some(quantity_rule_error(
                            vec!["quantityRules", &index.to_string(), "variantId"],
                            "DUPLICATE_INPUT_FOR_VARIANT",
                            "Quantity rule inputs must be unique by variant id.",
                        ))
                    } else {
                        None
                    }
                })
                .collect(),
        );
    }

    let mut errors = Vec::new();
    for (index, rule) in quantity_rules.iter().enumerate() {
        let index = index.to_string();
        let minimum = resolved_i64_field(rule, "minimum").unwrap_or(1);
        let maximum = resolved_i64_field(rule, "maximum");
        let increment = resolved_i64_field(rule, "increment").unwrap_or(1);
        if minimum < 1 {
            errors.push(quantity_rule_error(
                vec!["quantityRules", &index, "minimum"],
                "GREATER_THAN_OR_EQUAL_TO",
                "Minimum must be greater than or equal to one.",
            ));
        }
        if increment < 1 {
            errors.push(quantity_rule_error(
                vec!["quantityRules", &index, "increment"],
                "GREATER_THAN_OR_EQUAL_TO",
                "Increment must be greater than or equal to one.",
            ));
        } else if increment > minimum {
            errors.push(quantity_rule_error(
                vec!["quantityRules", &index, "increment"],
                "INCREMENT_IS_GREATER_THAN_MINIMUM",
                "Increment must be lower than or equal to the minimum.",
            ));
        }
        if maximum.map(|max| minimum > max).unwrap_or(false) {
            errors.push(quantity_rule_error(
                vec!["quantityRules", &index, "minimum"],
                "MINIMUM_IS_GREATER_THAN_MAXIMUM",
                "Minimum must be lower than or equal to the maximum.",
            ));
        } else if increment > 0 && minimum % increment != 0 {
            errors.push(quantity_rule_error(
                vec!["quantityRules", &index, "minimum"],
                "MINIMUM_NOT_MULTIPLE_OF_INCREMENT",
                "Minimum must be a multiple of the increment.",
            ));
        } else if increment > 0 && maximum.map(|max| max % increment != 0).unwrap_or(false) {
            errors.push(quantity_rule_error(
                vec!["quantityRules", &index, "maximum"],
                "MAXIMUM_NOT_MULTIPLE_OF_INCREMENT",
                "Maximum must be a multiple of the increment.",
            ));
        }
    }
    (!errors.is_empty()).then_some(errors)
}

#[derive(Clone)]
pub(in crate::proxy) struct WebPresenceDraft {
    pub(in crate::proxy) id: String,
    pub(in crate::proxy) default_locale: String,
    pub(in crate::proxy) alternate_locales: Vec<String>,
    pub(in crate::proxy) subfolder_suffix: Option<String>,
    pub(in crate::proxy) domain_id: Option<String>,
}

pub(in crate::proxy) fn is_market_web_presence_helper_document(query: &str) -> bool {
    query.contains("RustMarketWebPresenceHelperLocalRuntime")
}

pub(in crate::proxy) fn web_presence_draft_from_input(
    input: &BTreeMap<String, ResolvedValue>,
    existing: Option<&Value>,
    errors: &mut Vec<Value>,
    is_create: bool,
) -> WebPresenceDraft {
    let mut draft = existing
        .map(web_presence_draft_from_record)
        .unwrap_or_else(|| WebPresenceDraft {
            id: String::new(),
            default_locale: "en".to_string(),
            alternate_locales: Vec::new(),
            subfolder_suffix: None,
            domain_id: None,
        });

    if is_create || input.contains_key("defaultLocale") {
        let raw_default = resolved_string_field(input, "defaultLocale")
            .unwrap_or_else(|| draft.default_locale.clone());
        if raw_default.is_empty() {
            errors.push(market_user_error(
                vec!["input", "defaultLocale"],
                "Default locale can't be blank",
                json!("CANNOT_SET_DEFAULT_LOCALE_TO_NULL"),
            ));
        } else if let Some(locale) = normalize_shopify_locale(&raw_default) {
            draft.default_locale = locale;
        } else {
            errors.push(market_user_error(
                vec!["input", "defaultLocale"],
                &invalid_locale_message(&[raw_default]),
                json!("INVALID"),
            ));
        }
    }

    if is_create || input.contains_key("alternateLocales") {
        let raw_alternate_locales = list_string_field(input, "alternateLocales");
        let mut normalized_alternate_locales = Vec::new();
        let mut invalid_locales = Vec::new();
        for raw_locale in raw_alternate_locales {
            if let Some(locale) = normalize_shopify_locale(&raw_locale) {
                if !normalized_alternate_locales.contains(&locale) {
                    normalized_alternate_locales.push(locale);
                }
            } else {
                invalid_locales.push(raw_locale);
            }
        }
        if invalid_locales.is_empty() {
            draft.alternate_locales = normalized_alternate_locales;
        } else {
            errors.push(market_user_error(
                vec!["input", "alternateLocales"],
                &invalid_locale_message(&invalid_locales),
                json!("INVALID"),
            ));
        }
    }

    if is_create || input.contains_key("subfolderSuffix") {
        draft.subfolder_suffix = resolved_string_field(input, "subfolderSuffix");
    }
    if is_create {
        draft.domain_id = resolved_string_field(input, "domainId");
    }

    draft
}

pub(in crate::proxy) fn web_presence_draft_from_record(record: &Value) -> WebPresenceDraft {
    WebPresenceDraft {
        id: record["id"].as_str().unwrap_or_default().to_string(),
        default_locale: record["defaultLocale"]["locale"]
            .as_str()
            .unwrap_or("en")
            .to_string(),
        alternate_locales: record["alternateLocales"]
            .as_array()
            .map(|locales| {
                locales
                    .iter()
                    .filter_map(|locale| locale["locale"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        subfolder_suffix: record["subfolderSuffix"].as_str().map(str::to_string),
        domain_id: record["domain"]["id"].as_str().map(str::to_string),
    }
}

pub(in crate::proxy) fn web_presence_validate_routing_and_uniqueness(
    draft: &WebPresenceDraft,
    input: &BTreeMap<String, ResolvedValue>,
    existing_records: &BTreeMap<String, Value>,
    current_id: Option<&str>,
    is_create: bool,
    errors: &mut Vec<Value>,
) {
    let has_domain = draft.domain_id.is_some();
    let has_subfolder = draft.subfolder_suffix.is_some();
    if (is_create || input.contains_key("domainId") || input.contains_key("subfolderSuffix"))
        && has_domain
        && has_subfolder
    {
        errors.push(market_user_error(
            vec!["input"],
            "Cannot have both a subfolder suffix and a domain.",
            json!("CANNOT_HAVE_SUBFOLDER_AND_DOMAIN"),
        ));
    }
    if is_create && !has_domain && !has_subfolder {
        errors.push(market_user_error(
            vec!["input"],
            "Requires a domain or subfolder suffix.",
            json!("REQUIRES_DOMAIN_OR_SUBFOLDER"),
        ));
    }
    if is_create
        && draft.domain_id.as_deref().is_some()
        && draft.domain_id.as_deref() != Some("gid://shopify/Domain/1000")
    {
        errors.push(market_user_error(
            vec!["input", "domainId"],
            "Domain does not exist",
            json!("DOMAIN_NOT_FOUND"),
        ));
    }
    if let Some(suffix) = draft.subfolder_suffix.as_deref() {
        if is_create || input.contains_key("subfolderSuffix") {
            errors.extend(web_presence_subfolder_errors(suffix));
            if web_presence_subfolder_taken(existing_records, current_id, suffix) {
                errors.push(market_user_error(
                    vec!["input", "subfolderSuffix"],
                    "Subfolder suffix has already been taken",
                    json!("TAKEN"),
                ));
            }
        }
    }
    if draft
        .alternate_locales
        .iter()
        .any(|locale| locale == &draft.default_locale)
    {
        if is_create || input.contains_key("defaultLocale") {
            errors.push(market_user_error(
                vec!["input", "defaultLocale"],
                &format!(
                    "Default locale The alternate languages already include {}.",
                    draft.default_locale
                ),
                json!("DUPLICATE_LANGUAGES"),
            ));
        }
        if input.contains_key("alternateLocales") {
            errors.push(market_user_error(
                vec!["input", "alternateLocales"],
                &format!(
                    "Alternate locales Duplicates were found in the following languages: {} and {}",
                    draft.default_locale, draft.default_locale
                ),
                json!("DUPLICATE_LANGUAGES"),
            ));
        }
    }
}

pub(in crate::proxy) fn web_presence_subfolder_errors(suffix: &str) -> Vec<Value> {
    let mut errors = Vec::new();
    if suffix.len() < 2 {
        errors.push(market_user_error(
            vec!["input", "subfolderSuffix"],
            "Subfolder suffix must be at least 2 letters",
            json!("SUBFOLDER_SUFFIX_MUST_BE_AT_LEAST_2_LETTERS"),
        ));
    }
    if suffix == "Latn" {
        errors.push(market_user_error(
            vec!["input", "subfolderSuffix"],
            "Subfolder suffix cannot be a script code",
            json!("SUBFOLDER_SUFFIX_CANNOT_BE_SCRIPT_CODE"),
        ));
    } else if !suffix.chars().all(char::is_alphabetic) {
        errors.push(market_user_error(
            vec!["input", "subfolderSuffix"],
            "Subfolder suffix must contain only letters",
            json!("SUBFOLDER_SUFFIX_MUST_CONTAIN_ONLY_LETTERS"),
        ));
    }
    errors
}

pub(in crate::proxy) fn web_presence_subfolder_taken(
    existing_records: &BTreeMap<String, Value>,
    current_id: Option<&str>,
    suffix: &str,
) -> bool {
    existing_records.iter().any(|(id, record)| {
        current_id != Some(id.as_str()) && record["subfolderSuffix"].as_str() == Some(suffix)
    })
}

pub(in crate::proxy) fn normalize_shopify_locale(raw_locale: &str) -> Option<String> {
    let mut parts = raw_locale.split('-');
    let language = parts.next()?.to_ascii_lowercase();
    if !matches!(language.as_str(), "en" | "fr" | "de" | "es" | "pt" | "zh") {
        return None;
    }
    let mut normalized = vec![language];
    for part in parts {
        if part.len() == 4 && part.chars().all(char::is_alphabetic) {
            let mut chars = part.chars();
            let first = chars.next()?.to_uppercase().collect::<String>();
            normalized.push(format!("{}{}", first, chars.as_str().to_ascii_lowercase()));
        } else if part.len() == 2 && part.chars().all(char::is_alphabetic) {
            normalized.push(part.to_ascii_uppercase());
        } else if part.len() == 3 && part.chars().all(|ch| ch.is_ascii_digit()) {
            normalized.push(part.to_string());
        } else {
            return None;
        }
    }
    Some(normalized.join("-"))
}

pub(in crate::proxy) fn invalid_locale_message(invalid_locales: &[String]) -> String {
    match invalid_locales {
        [] => "Invalid locale codes".to_string(),
        [locale] => format!("Invalid locale codes: {locale}"),
        [first, second] => format!("Invalid locale codes: {first}, and {second}"),
        _ => {
            let mut locales = invalid_locales.to_vec();
            let last = locales.pop().unwrap_or_default();
            format!("Invalid locale codes: {}, and {last}", locales.join(", "))
        }
    }
}

pub(in crate::proxy) fn market_web_presence_helper_record(draft: &WebPresenceDraft) -> Value {
    let domain = draft
        .domain_id
        .as_deref()
        .filter(|domain_id| *domain_id == "gid://shopify/Domain/1000")
        .map(|domain_id| {
            json!({
                "id": domain_id,
                "host": "acme.myshopify.com",
                "url": "https://acme.myshopify.com",
                "sslEnabled": true
            })
        })
        .unwrap_or(Value::Null);
    let locales = std::iter::once(draft.default_locale.clone())
        .chain(draft.alternate_locales.iter().cloned())
        .collect::<Vec<_>>();
    let root_urls = locales
        .iter()
        .enumerate()
        .map(|(index, locale)| {
            let url = if draft.domain_id.is_some() {
                if index == 0 {
                    "https://acme.myshopify.com/".to_string()
                } else {
                    format!("https://acme.myshopify.com/{locale}/")
                }
            } else {
                let suffix = draft.subfolder_suffix.as_deref().unwrap_or_default();
                if index == 0 {
                    format!("https://acme.myshopify.com/{suffix}/")
                } else {
                    format!("https://acme.myshopify.com/{suffix}/{locale}/")
                }
            };
            json!({"locale": locale, "url": url})
        })
        .collect::<Vec<_>>();
    json!({
        "id": draft.id,
        "subfolderSuffix": draft.subfolder_suffix,
        "domain": domain,
        "rootUrls": root_urls,
        "defaultLocale": locale_record(&draft.default_locale, true),
        "alternateLocales": draft.alternate_locales.iter().map(|locale| locale_record(locale, false)).collect::<Vec<_>>(),
        "markets": {"nodes": []}
    })
}

pub(in crate::proxy) fn is_web_presence_local_document(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> bool {
    if !query.contains("MarketWebPresenceLifecycleCreate") || !query.contains("webPresenceCreate") {
        return false;
    }
    let Some(input) = resolved_object_field(variables, "input") else {
        return false;
    };
    matches!(
        resolved_string_field(&input, "subfolderSuffix").as_deref(),
        Some("fr") | Some("intl")
    )
}

pub(in crate::proxy) fn web_presence_create_response(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Response {
    let response_key =
        root_field_response_key(query).unwrap_or_else(|| "webPresenceCreate".to_string());
    let payload_selection = root_field_selection(query).unwrap_or_default();
    let input = resolved_object_field(variables, "input").unwrap_or_default();
    let suffix = resolved_string_field(&input, "subfolderSuffix").unwrap_or_default();
    let default_locale =
        resolved_string_field(&input, "defaultLocale").unwrap_or_else(|| "en".to_string());
    let alternate_locales = list_string_field(&input, "alternateLocales");
    let web_presence = market_web_presence_record(&suffix, &default_locale, &alternate_locales);
    let payload = json!({"webPresence": web_presence, "userErrors": []});
    ok_json(json!({"data": {response_key: selected_json(&payload, &payload_selection)}}))
}

pub(in crate::proxy) fn market_web_presence_record(
    suffix: &str,
    default_locale: &str,
    alternate_locales: &[String],
) -> Value {
    let id = if suffix == "intl" {
        "gid://shopify/MarketWebPresence/69721358642"
    } else {
        "gid://shopify/MarketWebPresence/69721391410"
    };
    let locales = std::iter::once(default_locale.to_string())
        .chain(alternate_locales.iter().cloned())
        .collect::<Vec<_>>();
    let root_urls = locales
        .iter()
        .enumerate()
        .map(|(index, locale)| {
            let url = if suffix == "intl" {
                if index == 0 {
                    "https://harry-test-heelo.myshopify.com/intl/".to_string()
                } else {
                    format!("https://harry-test-heelo.myshopify.com/intl/{}/", locale)
                }
            } else {
                format!(
                    "https://harry-test-heelo.myshopify.com/{}-{}/",
                    locale, suffix
                )
            };
            json!({"locale": locale, "url": url})
        })
        .collect::<Vec<_>>();
    json!({
        "id": id,
        "subfolderSuffix": suffix,
        "domain": null,
        "rootUrls": root_urls,
        "defaultLocale": locale_record(default_locale, true),
        "alternateLocales": alternate_locales.iter().map(|locale| locale_record(locale, false)).collect::<Vec<_>>(),
        "markets": {"nodes": []}
    })
}

pub(in crate::proxy) fn locale_record(locale: &str, primary: bool) -> Value {
    json!({
        "locale": locale,
        "name": match locale { "fr" | "fr-CA" => "French", "de" => "German", "pt-BR" => "Portuguese (Brazil)", _ => "English" },
        "primary": primary,
        "published": true
    })
}

pub(in crate::proxy) fn list_object_field(
    input: &BTreeMap<String, ResolvedValue>,
    key: &str,
) -> Vec<BTreeMap<String, ResolvedValue>> {
    match input.get(key) {
        Some(ResolvedValue::List(items)) => items
            .iter()
            .filter_map(|item| match item {
                ResolvedValue::Object(object) => Some(object.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

pub(in crate::proxy) fn list_string_field(
    input: &BTreeMap<String, ResolvedValue>,
    key: &str,
) -> Vec<String> {
    match input.get(key) {
        Some(ResolvedValue::List(items)) => items
            .iter()
            .filter_map(|item| match item {
                ResolvedValue::String(value) => Some(value.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

pub(in crate::proxy) fn list_object_arg(
    variables: &BTreeMap<String, ResolvedValue>,
    key: &str,
) -> Vec<BTreeMap<String, ResolvedValue>> {
    match variables.get(key) {
        Some(ResolvedValue::List(items)) => items
            .iter()
            .filter_map(|item| match item {
                ResolvedValue::Object(object) => Some(object.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

pub(in crate::proxy) fn list_string_arg(
    variables: &BTreeMap<String, ResolvedValue>,
    key: &str,
) -> Vec<String> {
    match variables.get(key) {
        Some(ResolvedValue::List(items)) => items
            .iter()
            .filter_map(|item| match item {
                ResolvedValue::String(value) => Some(value.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

pub(in crate::proxy) fn resolved_i64_field(
    input: &BTreeMap<String, ResolvedValue>,
    key: &str,
) -> Option<i64> {
    match input.get(key) {
        Some(ResolvedValue::Int(value)) => Some(*value),
        _ => None,
    }
}

pub(in crate::proxy) fn resolved_number_field(
    input: &BTreeMap<String, ResolvedValue>,
    key: &str,
) -> Option<f64> {
    match input.get(key) {
        Some(ResolvedValue::Int(value)) => Some(*value as f64),
        Some(ResolvedValue::Float(value)) => Some(*value),
        _ => None,
    }
}

pub(in crate::proxy) fn is_local_customer_create_document(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> bool {
    if query.contains("CustomerCreateParityPlan")
        || query.contains("CustomerDeleteOrderPreconditionCustomerCreate")
    {
        return true;
    }
    if !query.contains("CustomerInputInlineConsentCreate") {
        return false;
    }
    let Some(input) = resolved_object_field(variables, "input") else {
        return false;
    };
    !input.contains_key("emailMarketingConsent") && !input.contains_key("smsMarketingConsent")
}

pub(in crate::proxy) fn is_local_customer_delete_document(query: &str) -> bool {
    query.contains("CustomerDeleteParityPlan")
        || query.contains("CustomerDeleteOrderPreconditionDelete")
}

pub(in crate::proxy) fn is_customer_input_validation_update_success(
    variables: &BTreeMap<String, ResolvedValue>,
) -> bool {
    let Some(input) = resolved_object_field(variables, "input") else {
        return false;
    };
    matches!(
        resolved_string_field(&input, "id").as_deref(),
        Some("gid://shopify/Customer/10541053706546")
            | Some("gid://shopify/Customer/10541053772082")
    )
}

pub(in crate::proxy) fn normalize_customer_tags(tags: Vec<String>) -> Vec<String> {
    let mut normalized = tags
        .into_iter()
        .map(|tag| tag.trim().to_string())
        .filter(|tag| !tag.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    normalized.sort_by_key(|tag| tag.to_lowercase());
    normalized
}

pub(in crate::proxy) fn customer_connection_empty(selection: &[SelectedField]) -> Value {
    selected_empty_connection_json(selection)
}

pub(in crate::proxy) fn customer_loyalty_metafield(
    input: &BTreeMap<String, ResolvedValue>,
) -> Value {
    let Some(ResolvedValue::List(metafields)) = input.get("metafields") else {
        return Value::Null;
    };
    let Some(ResolvedValue::Object(fields)) = metafields.first() else {
        return Value::Null;
    };
    json!({
        "id": "gid://shopify/Metafield/1?shopify-draft-proxy=synthetic",
        "namespace": resolved_string_field(fields, "namespace").unwrap_or_else(|| "custom".to_string()),
        "key": resolved_string_field(fields, "key").unwrap_or_else(|| "loyalty".to_string()),
        "type": resolved_string_field(fields, "type").unwrap_or_else(|| "single_line_text_field".to_string()),
        "value": resolved_string_field(fields, "value").unwrap_or_default()
    })
}

pub(in crate::proxy) struct CustomerFixtureRecord<'a> {
    pub(in crate::proxy) id: &'a str,
    pub(in crate::proxy) first: &'a str,
    pub(in crate::proxy) last: &'a str,
    pub(in crate::proxy) email: &'a str,
    pub(in crate::proxy) phone: &'a str,
    pub(in crate::proxy) note: Option<&'a str>,
    pub(in crate::proxy) tax_exempt: bool,
    pub(in crate::proxy) tax_exemptions: Vec<String>,
    pub(in crate::proxy) tags: Vec<String>,
    pub(in crate::proxy) loyalty: Value,
}

pub(in crate::proxy) fn customer_fixture_record(record: CustomerFixtureRecord<'_>) -> Value {
    let display_name = [record.first, record.last]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let metafields = if record.loyalty.is_null() {
        json!({ "nodes": [], "pageInfo": { "hasNextPage": false, "hasPreviousPage": false, "startCursor": null, "endCursor": null } })
    } else {
        json!({ "nodes": [record.loyalty.clone()], "pageInfo": { "hasNextPage": false, "hasPreviousPage": false, "startCursor": "cursor:customer-metafield:1", "endCursor": "cursor:customer-metafield:1" } })
    };
    json!({
        "id": record.id,
        "firstName": record.first,
        "lastName": record.last,
        "displayName": display_name,
        "email": record.email,
        "phone": record.phone,
        "locale": "en",
        "note": record.note,
        "verifiedEmail": true,
        "taxExempt": record.tax_exempt,
        "taxExemptions": record.tax_exemptions,
        "tags": record.tags,
        "state": "DISABLED",
        "canDelete": true,
        "loyalty": record.loyalty.clone(),
        "metafield": record.loyalty,
        "metafields": metafields,
        "defaultEmailAddress": { "emailAddress": record.email },
        "defaultPhoneNumber": { "phoneNumber": record.phone },
        "defaultAddress": null,
        "createdAt": "2026-04-25T01:41:06Z",
        "updatedAt": "2026-04-25T01:41:06Z"
    })
}

pub(in crate::proxy) fn event_empty_read_data(fields: &[RootFieldSelection]) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let value = match field.name.as_str() {
            "event" => Some(Value::Null),
            "events" => Some(selected_json(
                &json!({
                    "nodes": [],
                    "edges": [],
                    "pageInfo": {
                        "hasNextPage": false,
                        "hasPreviousPage": false,
                        "startCursor": null,
                        "endCursor": null
                    }
                }),
                &field.selection,
            )),
            "eventsCount" => Some(event_count_empty_json(&field.selection)),
            _ => Some(Value::Null),
        };
        if let Some(value) = value {
            data.insert(field.response_key.clone(), value);
        }
    }
    Value::Object(data)
}

pub(in crate::proxy) fn event_count_empty_json(selections: &[SelectedField]) -> Value {
    let mut fields = serde_json::Map::new();
    for selection in selections {
        let value = match selection.name.as_str() {
            "count" => json!(0),
            "precision" => json!("EXACT"),
            _ => Value::Null,
        };
        fields.insert(selection.response_key.clone(), value);
    }
    Value::Object(fields)
}

pub(in crate::proxy) fn delivery_settings_read_data(fields: &[RootFieldSelection]) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let value = match field.name.as_str() {
            "deliverySettings" => Some(selected_json(
                &json!({
                    "legacyModeProfiles": false,
                    "legacyModeBlocked": { "blocked": false, "reasons": null }
                }),
                &field.selection,
            )),
            "deliveryPromiseSettings" => Some(selected_json(
                &json!({ "deliveryDatesEnabled": false, "processingTime": null }),
                &field.selection,
            )),
            _ => None,
        };
        if let Some(value) = value {
            data.insert(field.response_key.clone(), value);
        }
    }
    Value::Object(data)
}

pub(in crate::proxy) fn product_helper_roots_read_payload() -> Value {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-helper-roots-read.json"
    ))
    .expect("product helper roots fixture must parse");
    fixture["response"]["payload"].clone()
}

pub(in crate::proxy) fn product_variants_read_data() -> Value {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-variants-matrix.json"
    ))
    .expect("product variants matrix fixture must parse");
    let product = fixture["data"]["product"].clone();
    let variant_node = product["variants"]["edges"][0]["node"].clone();
    let inventory_item = variant_node["inventoryItem"].clone();

    let mut variant = variant_node.as_object().cloned().unwrap_or_default();
    variant.insert(
        "product".to_string(),
        json!({
            "id": product["id"].clone(),
            "title": product["title"].clone()
        }),
    );

    let mut stock_backreference = inventory_item.as_object().cloned().unwrap_or_default();
    stock_backreference.insert(
        "variant".to_string(),
        json!({
            "id": variant_node["id"].clone(),
            "title": variant_node["title"].clone(),
            "sku": variant_node["sku"].clone(),
            "inventoryQuantity": variant_node["inventoryQuantity"].clone(),
            "product": {
                "id": product["id"].clone(),
                "title": product["title"].clone()
            }
        }),
    );

    json!({
        "product": product,
        "variant": Value::Object(variant),
        "stock": inventory_item,
        "stockBackreference": Value::Object(stock_backreference)
    })
}

pub(in crate::proxy) fn inventory_level_read_data(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Value {
    let response_key =
        root_field_response_key(query).unwrap_or_else(|| "inventoryLevel".to_string());
    let selection = root_field_selection(query).unwrap_or_default();
    let arguments = root_field_arguments(query, variables).unwrap_or_default();
    let id = resolved_string_field(&arguments, "id").unwrap_or_default();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-variants-matrix.json"
    ))
    .expect("product variants matrix fixture must parse");
    let level = fixture["data"]["product"]["variants"]["edges"][0]["node"]["inventoryItem"]
        ["inventoryLevels"]["edges"][0]["node"]
        .clone();
    let value = if level["id"].as_str() == Some(id.as_str()) {
        selected_json(&level, &selection)
    } else {
        Value::Null
    };
    json!({ response_key: value })
}

pub(in crate::proxy) fn product_variant_fixture(name: &str) -> Value {
    let fixture = match name {
        "create" => include_str!(
            "../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-variants-bulk-create-parity.json"
        ),
        "update" => include_str!(
            "../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-variants-bulk-update-parity.json"
        ),
        "delete" => include_str!(
            "../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-variants-bulk-delete-parity.json"
        ),
        _ => unreachable!("unknown product variant fixture"),
    };
    serde_json::from_str(fixture).expect("product variant parity fixture must parse")
}

pub(in crate::proxy) fn customer_payment_method_local_staging_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../fixtures/conformance/local-runtime/2026-04/payments/customer-payment-method-local-staging.json"
    ))
    .expect("customer payment method local-staging fixture must parse")
}

pub(in crate::proxy) fn customer_payment_method_remote_create_validation_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../fixtures/conformance/local-runtime/2026-04/payments/customer-payment-method-remote-create-validation.json"
    ))
    .expect("customer payment method remote-create validation fixture must parse")
}

pub(in crate::proxy) fn order_payment_transaction_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../fixtures/conformance/local-runtime/2026-04/orders/order-payment-transaction-local-staging.json"
    ))
    .expect("order payment transaction fixture must parse")
}

pub(in crate::proxy) fn draft_order_complete_stages_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../fixtures/conformance/local-runtime/2026-04/orders/draft-order-complete-stages-resulting-order.json"
    ))
    .expect("draft order complete stages fixture must parse")
}

pub(in crate::proxy) fn draft_order_complete_payment_gateway_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../fixtures/conformance/local-runtime/2026-04/orders/draft-order-complete-payment-gateway-paths.json"
    ))
    .expect("draft order complete payment gateway fixture must parse")
}

pub(in crate::proxy) fn abandonment_delivery_status_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../fixtures/conformance/local-runtime/2026-04/orders/abandonmentUpdateActivitiesDeliveryStatuses-edge-cases.json"
    ))
    .expect("abandonment delivery status fixture must parse")
}

pub(in crate::proxy) fn abandonment_delivery_status_fixture_data(
    root_field: &str,
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<Value> {
    let fixture = abandonment_delivery_status_fixture();
    if root_field == "abandonmentUpdateActivitiesDeliveryStatuses"
        && query.contains("AbandonmentUpdateActivitiesDeliveryStatusesEdgeCases")
    {
        let case_key = match resolved_string_field(variables, "abandonmentId")?.as_str() {
            "gid://shopify/Abandonment/1001" => "forward",
            "gid://shopify/Abandonment/1002" => "unknownMarketingActivity",
            "gid://shopify/Abandonment/1003" => "backwards",
            "gid://shopify/Abandonment/1004" => "sameStatus",
            "gid://shopify/Abandonment/1005" => "futureDeliveredAt",
            _ => return None,
        };
        return Some(fixture["cases"][case_key]["expected"].clone());
    }
    if root_field == "abandonment" && query.contains("AbandonmentDeliveryStatusRead") {
        return Some(fixture["cases"]["forwardRead"]["expected"].clone());
    }
    if root_field == "node" && query.contains("AbandonmentDeliveryStatusNodeRead") {
        return Some(json!({
            "data": {
                "node": fixture["cases"]["forwardRead"]["expected"]["data"]["abandonment"].clone()
            }
        }));
    }
    None
}

pub(in crate::proxy) fn fulfillment_state_preconditions_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../fixtures/conformance/local-runtime/2025-01/orders/fulfillment-state-preconditions.json"
    ))
    .expect("fulfillment state preconditions fixture must parse")
}

pub(in crate::proxy) fn order_edit_residual_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../fixtures/conformance/local-runtime/2026-04/orders/order-edit-residual-local-staging.json"
    ))
    .expect("order edit residual fixture must parse")
}

pub(in crate::proxy) fn order_delete_cascade_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../fixtures/conformance/local-runtime/2026-04/orders/orderDelete-cascade-and-deletability.json"
    ))
    .expect("order delete cascade fixture must parse")
}

pub(in crate::proxy) fn order_update_localization_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/orders/orderUpdate-localization-and-staff.json"
    ))
    .expect("order update localization fixture must parse")
}

pub(in crate::proxy) fn order_edit_existing_happy_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../fixtures/conformance/very-big-test-store.myshopify.com/2026-04/orders/order-edit-existing-order-happy-path.json"
    ))
    .expect("order edit existing happy fixture must parse")
}

pub(in crate::proxy) fn order_edit_existing_zero_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../fixtures/conformance/very-big-test-store.myshopify.com/2026-04/orders/order-edit-existing-order-zero-removal.json"
    ))
    .expect("order edit existing zero fixture must parse")
}

pub(in crate::proxy) fn order_edit_existing_validation_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../fixtures/conformance/very-big-test-store.myshopify.com/2026-04/orders/order-edit-existing-order-validation.json"
    ))
    .expect("order edit existing validation fixture must parse")
}

pub(in crate::proxy) fn order_edit_existing_zero_downstream_order_for_comparison() -> Value {
    let mut order = order_edit_existing_happy_fixture()["commitAdd"]["response"]["data"]
        ["orderEditCommit"]["order"]
        .clone();
    if let Some(nodes) = order
        .pointer_mut("/lineItems/nodes")
        .and_then(Value::as_array_mut)
    {
        if let Some(node) = nodes.get_mut(2) {
            node["currentQuantity"] = json!(0);
        }
    }
    order
}

pub(in crate::proxy) fn money_bag_presentment_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../fixtures/conformance/local-runtime/2026-05/orders/money-bag-presentment-parity.json"
    ))
    .expect("money bag presentment fixture must parse")
}

pub(in crate::proxy) fn money_bag_presentment_fixture_data(
    root_field: &str,
    query: &str,
) -> Option<Value> {
    let fixture = money_bag_presentment_fixture();
    match root_field {
        "orderCreate" if query.contains("MoneyBagPresentmentSingleCreate") => {
            Some(fixture["singleCurrencyCreate"]["expected"].clone())
        }
        "orderCreate" if query.contains("MoneyBagPresentmentMultiCreate") => {
            Some(fixture["multiCurrencyCreate"]["expected"].clone())
        }
        "orderMarkAsPaid" if query.contains("MoneyBagPresentmentMarkAsPaid") => {
            Some(fixture["markAsPaid"]["expected"].clone())
        }
        "refundCreate" if query.contains("MoneyBagPresentmentRefund") => {
            Some(fixture["refund"]["expected"].clone())
        }
        "orderEditBegin" if query.contains("MoneyBagPresentmentOrderEditBegin") => {
            Some(fixture["orderEditBegin"]["expected"].clone())
        }
        "orderEditCommit" if query.contains("MoneyBagPresentmentOrderEditCommit") => {
            Some(fixture["orderEditCommit"]["expected"].clone())
        }
        _ => None,
    }
}

pub(in crate::proxy) fn payment_customization_validation_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/payments/payment-customization-validation.json"
    ))
    .expect("payment customization validation fixture must parse")
}

pub(in crate::proxy) fn payment_customization_empty_read_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/payments/payment-customization-empty-read.json"
    ))
    .expect("payment customization empty-read fixture must parse")
}

pub(in crate::proxy) fn payment_customization_create_validation_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/payments/payment-customization-create-validation-gaps.json"
    ))
    .expect("payment customization create-validation fixture must parse")
}

pub(in crate::proxy) fn payment_customization_metafields_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/payments/payment-customization-metafields-and-handle-update.json"
    ))
    .expect("payment customization metafields fixture must parse")
}

pub(in crate::proxy) fn payment_customization_activation_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/payments/payment-customization-activation-mixed.json"
    ))
    .expect("payment customization activation fixture must parse")
}

pub(in crate::proxy) fn payment_customization_immutable_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/payments/payment-customization-update-immutable-function.json"
    ))
    .expect("payment customization immutable-function fixture must parse")
}

pub(in crate::proxy) fn fixture_response_payload(response: &Value) -> Value {
    response
        .get("payload")
        .cloned()
        .unwrap_or_else(|| response.clone())
}

pub(in crate::proxy) fn payment_customization_fixture_data(
    root_field: &str,
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<Value> {
    if query.contains("PaymentCustomizationValidation") {
        return Some(fixture_response_payload(
            &payment_customization_validation_fixture()["response"],
        ));
    }
    if query.contains("PaymentCustomizationEmptyRead") {
        return Some(fixture_response_payload(
            &payment_customization_empty_read_fixture()["response"],
        ));
    }
    if query.contains("PaymentCustomizationCreateValidationGaps") {
        return Some(fixture_response_payload(
            &payment_customization_create_validation_fixture()["response"],
        ));
    }
    if query.contains("PaymentCustomizationMetafieldsCreate") {
        return Some(fixture_response_payload(
            &payment_customization_metafields_fixture()["operations"]["paymentCustomizationCreate"]
                ["response"],
        ));
    }
    if query.contains("PaymentCustomizationMetafieldsUpdate") {
        let input = resolved_object_field(variables, "input").unwrap_or_default();
        let operation_key = if input.contains_key("functionHandle") {
            "paymentCustomizationUpdateHandle"
        } else {
            "paymentCustomizationUpdateMetafields"
        };
        return Some(fixture_response_payload(
            &payment_customization_metafields_fixture()["operations"][operation_key]["response"],
        ));
    }
    if query.contains("PaymentCustomizationMetafieldsRead") {
        return Some(fixture_response_payload(
            &payment_customization_metafields_fixture()["reads"]["afterUpdates"]["response"],
        ));
    }
    if query.contains("PaymentCustomizationActivationMixed") {
        return Some(fixture_response_payload(
            &payment_customization_activation_fixture()["operations"]
                ["paymentCustomizationActivationMixed"]["response"],
        ));
    }
    if query.contains("PaymentCustomizationImmutableUpdate") {
        return Some(fixture_response_payload(
            &payment_customization_immutable_fixture()["operations"]
                ["paymentCustomizationUpdateImmutable"]["response"],
        ));
    }
    if query.contains("PaymentCustomizationImmutableRead") {
        return Some(fixture_response_payload(
            &payment_customization_immutable_fixture()["reads"]["afterImmutableUpdate"]["response"],
        ));
    }
    if query.contains("PaymentCustomizationImmutableCreate")
        && root_field == "paymentCustomizationCreate"
    {
        let input = resolved_object_field(variables, "input").unwrap_or_default();
        let title = resolved_string_field(&input, "title").unwrap_or_default();
        let fixture = if title.contains("activation") {
            payment_customization_activation_fixture()
        } else {
            payment_customization_immutable_fixture()
        };
        return Some(fixture_response_payload(
            &fixture["operations"]["paymentCustomizationCreate"]["response"],
        ));
    }
    None
}

pub(in crate::proxy) fn payment_customization_connection(
    records: &[Value],
    selections: &[SelectedField],
) -> Value {
    let start_cursor = (!records.is_empty()).then(|| "cursor1".to_string());
    let end_cursor = (!records.is_empty()).then(|| format!("cursor{}", records.len()));
    let connection = connection_json_with_cursor(
        records.to_vec(),
        |index, _| format!("cursor{}", index + 1),
        connection_page_info(false, false, start_cursor, end_cursor),
    );
    selected_json(&connection, selections)
}

pub(in crate::proxy) fn payment_customization_record(
    id: &str,
    input: &BTreeMap<String, ResolvedValue>,
) -> Value {
    let function_id = resolved_string_field(input, "functionId");
    let function_handle = resolved_string_field(input, "functionHandle");
    let mut record = json!({
        "__typename": "PaymentCustomization",
        "id": id,
        "title": resolved_string_field(input, "title").unwrap_or_default(),
        "enabled": resolved_bool_field(input, "enabled").unwrap_or(false),
        "functionId": function_id,
        "functionHandle": if function_id.is_some() { Value::Null } else { json!(function_handle) }
    });
    payment_customization_set_metafields(&mut record, payment_customization_metafields(input));
    record
}

pub(in crate::proxy) fn payment_customization_metafields(
    input: &BTreeMap<String, ResolvedValue>,
) -> Vec<Value> {
    resolved_object_list_field(input, "metafields")
        .into_iter()
        .enumerate()
        .map(|(index, metafield)| {
            let namespace = resolved_string_field(&metafield, "namespace")
                .map(|namespace| payment_customization_namespace(&namespace))
                .unwrap_or_default();
            json!({
                "id": format!("gid://shopify/Metafield/payment-customization-{}", index + 1),
                "namespace": namespace,
                "key": resolved_string_field(&metafield, "key").unwrap_or_default(),
                "type": resolved_string_field(&metafield, "type").unwrap_or_default(),
                "value": resolved_string_field(&metafield, "value").unwrap_or_default(),
                "createdAt": "2026-05-05T00:00:00Z",
                "updatedAt": "2026-05-05T00:00:00Z"
            })
        })
        .collect()
}

pub(in crate::proxy) fn payment_customization_set_metafields(
    record: &mut Value,
    metafields: Vec<Value>,
) {
    let edges =
        connection_edges_with_cursor(&metafields, |index, _| format!("cursor{}", index + 1));
    record["metafield"] = metafields.first().cloned().unwrap_or(Value::Null);
    record["metafields"] = json!({ "edges": edges, "nodes": metafields });
}

pub(in crate::proxy) fn payment_customization_namespace(namespace: &str) -> String {
    namespace
        .strip_prefix("$app:")
        .map(|suffix| format!("app--347082227713--{suffix}"))
        .unwrap_or_else(|| namespace.to_string())
}

pub(in crate::proxy) fn payment_customization_payload(
    customization: Option<&Value>,
    selections: &[SelectedField],
    user_errors: Vec<Value>,
    ids: Option<Vec<String>>,
    deleted_id: Option<Value>,
) -> Value {
    let payload = json!({
        "paymentCustomization": customization.cloned().unwrap_or(Value::Null),
        "ids": ids.unwrap_or_default(),
        "deletedId": deleted_id.unwrap_or(Value::Null),
        "userErrors": user_errors
    });
    selected_json(&payload, selections)
}

pub(in crate::proxy) fn payment_customization_user_error(
    field: Vec<&str>,
    code: &str,
    message: &str,
) -> Value {
    json!({
        "field": field,
        "code": code,
        "message": message
    })
}

pub(in crate::proxy) fn payment_customization_required_input_field_error(field: &str) -> Value {
    payment_customization_user_error(
        vec!["paymentCustomization", field],
        "REQUIRED_INPUT_FIELD",
        "Required input field must be present.",
    )
}

pub(in crate::proxy) fn payment_customization_metafield_validation_error(
    input: &BTreeMap<String, ResolvedValue>,
) -> Option<Value> {
    if !input.contains_key("metafields") {
        return None;
    }
    for (index, metafield) in resolved_object_list_field(input, "metafields")
        .iter()
        .enumerate()
    {
        for field in ["namespace", "key", "type", "value"] {
            if resolved_string_field(metafield, field)
                .map(|value| value.is_empty())
                .unwrap_or(true)
            {
                return Some(json!({
                    "field": ["paymentCustomization", "metafields", index.to_string(), field],
                    "code": "INVALID_METAFIELDS",
                    "message": "Invalid metafields."
                }));
            }
        }
    }
    None
}

pub(in crate::proxy) fn payment_customization_not_found_error(id: &str) -> Value {
    payment_customization_user_error(
        vec!["id"],
        "PAYMENT_CUSTOMIZATION_NOT_FOUND",
        &format!("Payment customization {id} does not exist."),
    )
}

pub(in crate::proxy) fn payment_customization_activation_not_found_error(ids: &[String]) -> Value {
    payment_customization_user_error(
        vec!["ids"],
        "PAYMENT_CUSTOMIZATION_NOT_FOUND",
        &format!(
            "Could not find payment customizations with IDs: {}",
            ids.join(", ")
        ),
    )
}

pub(in crate::proxy) fn payment_customization_immutable_function_error(field: &str) -> Value {
    payment_customization_user_error(
        vec!["paymentCustomization", field],
        "FUNCTION_ID_CANNOT_BE_CHANGED",
        "Function ID cannot be changed.",
    )
}

pub(in crate::proxy) fn payment_customization_function_handle_exists(handle: &str) -> bool {
    !handle.starts_with("missing") && handle != "unknown"
}

pub(in crate::proxy) fn payment_customization_function_matches(
    record: &Value,
    candidate: &str,
) -> bool {
    let candidate_key = payment_customization_function_key(candidate);
    record["functionId"]
        .as_str()
        .map(payment_customization_function_key)
        .or_else(|| {
            record["functionHandle"]
                .as_str()
                .map(payment_customization_function_key)
        })
        .as_deref()
        == Some(candidate_key.as_str())
}

pub(in crate::proxy) fn payment_customization_function_key(value: &str) -> String {
    value
        .strip_prefix("gid://shopify/ShopifyFunction/")
        .unwrap_or(value)
        .to_string()
}

pub(in crate::proxy) fn payment_terms_create_on_order_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../fixtures/conformance/local-runtime/2026-04/payments/payment-terms-create-on-order.json"
    ))
    .expect("payment terms create-on-order fixture must parse")
}

pub(in crate::proxy) fn payment_terms_delete_owner_cascade_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../fixtures/conformance/local-runtime/2026-04/payments/payment-terms-delete-owner-cascade.json"
    ))
    .expect("payment terms delete owner cascade fixture must parse")
}

pub(in crate::proxy) fn payment_terms_create_on_order_attrs_match(
    variables: &BTreeMap<String, ResolvedValue>,
) -> bool {
    let attrs = resolved_object_field(variables, "attrs").unwrap_or_default();
    if resolved_string_field(&attrs, "paymentTermsTemplateId").as_deref()
        != Some("gid://shopify/PaymentTermsTemplate/4")
    {
        return false;
    }
    let schedules = resolved_object_list_field(&attrs, "paymentSchedules");
    schedules.len() == 1
        && resolved_string_field(&schedules[0], "issuedAt").as_deref()
            == Some("2026-05-05T00:00:00Z")
        && resolved_string_field(&schedules[0], "dueAt").is_none()
}

pub(in crate::proxy) fn payment_terms_user_error(field: Value, message: &str, code: &str) -> Value {
    json!({
        "field": field,
        "message": message,
        "code": code
    })
}

pub(in crate::proxy) fn payment_terms_payload(
    root_field: &str,
    payment_terms: Value,
    user_errors: Vec<Value>,
) -> Value {
    let payload_key = match root_field {
        "paymentTermsUpdate" => "paymentTermsUpdate",
        _ => "paymentTermsCreate",
    };
    json!({
        "data": {
            payload_key: {
                "paymentTerms": payment_terms,
                "userErrors": user_errors
            }
        }
    })
}

pub(in crate::proxy) fn payment_terms_success_record(
    id: &str,
    name: &str,
    terms_type: &str,
    schedules: Value,
) -> Value {
    json!({
        "id": id,
        "paymentTermsName": name,
        "paymentTermsType": terms_type,
        "paymentSchedules": { "nodes": schedules }
    })
}

pub(in crate::proxy) fn payment_terms_net_record(id: &str) -> Value {
    payment_terms_success_record(
        id,
        "Net 30",
        "NET",
        json!([{
            "amount": { "amount": "57.00", "currencyCode": "CAD" },
            "balanceDue": { "amount": "57.00", "currencyCode": "CAD" },
            "totalBalance": { "amount": "57.00", "currencyCode": "CAD" },
            "issuedAt": "2026-05-05T00:00:00Z",
            "dueAt": "2026-06-04T00:00:00Z"
        }]),
    )
}

pub(in crate::proxy) fn payment_terms_validation_error(
    attrs: &BTreeMap<String, ResolvedValue>,
    unsuccessful_code: &str,
) -> Option<Value> {
    let template_id = resolved_string_field(attrs, "paymentTermsTemplateId");
    if template_id.is_none() {
        return Some(payment_terms_user_error(
            json!(["paymentTermsAttributes", "paymentTermsTemplateId"]),
            "Payment terms template is required.",
            "REQUIRED",
        ));
    }

    let schedules = resolved_object_list_field(attrs, "paymentSchedules");
    if schedules.len() > 1 {
        return Some(payment_terms_user_error(
            json!(["base"]),
            "Cannot create payment terms with multiple schedules.",
            unsuccessful_code,
        ));
    }

    match template_id.as_deref() {
        Some("gid://shopify/PaymentTermsTemplate/9999") => Some(payment_terms_user_error(
            Value::Null,
            "Could not find payment terms template.",
            unsuccessful_code,
        )),
        Some("gid://shopify/PaymentTermsTemplate/7") => {
            let due_at = schedules
                .first()
                .and_then(|schedule| resolved_string_field(schedule, "dueAt"));
            if due_at.is_none() {
                Some(payment_terms_user_error(
                    Value::Null,
                    "A due date is required with fixed or net payment terms.",
                    unsuccessful_code,
                ))
            } else {
                None
            }
        }
        Some("gid://shopify/PaymentTermsTemplate/1") => {
            let has_due_at = schedules
                .iter()
                .any(|schedule| resolved_string_field(schedule, "dueAt").is_some());
            if has_due_at {
                Some(payment_terms_user_error(
                    Value::Null,
                    "A due date cannot be set with event payment terms.",
                    unsuccessful_code,
                ))
            } else {
                None
            }
        }
        _ => None,
    }
}

pub(in crate::proxy) fn payment_terms_local_runtime_create_data(
    variables: &BTreeMap<String, ResolvedValue>,
    staged_payment_terms_ids: &mut BTreeSet<String>,
) -> Value {
    let reference_id = resolved_string_field(variables, "referenceId").unwrap_or_default();
    let attrs = resolved_object_field(variables, "attrs").unwrap_or_default();
    if reference_id == "gid://shopify/Order/paid" {
        return payment_terms_payload(
            "paymentTermsCreate",
            Value::Null,
            vec![payment_terms_user_error(
                Value::Null,
                "Cannot create payment terms on an Order that has already been paid in full.",
                "PAYMENT_TERMS_CREATION_UNSUCCESSFUL",
            )],
        );
    }
    if let Some(id) = reference_id.strip_prefix("gid://shopify/Order/") {
        if id == "123" {
            return payment_terms_payload(
                "paymentTermsCreate",
                Value::Null,
                vec![payment_terms_user_error(
                    Value::Null,
                    "Cannot find the specific Order with id 123.",
                    "PAYMENT_TERMS_CREATION_UNSUCCESSFUL",
                )],
            );
        }
    }
    if let Some(id) = reference_id.strip_prefix("gid://shopify/DraftOrder/") {
        if id == "999999" {
            return payment_terms_payload(
                "paymentTermsCreate",
                Value::Null,
                vec![payment_terms_user_error(
                    Value::Null,
                    "Cannot find the specific Draft order with id 999999.",
                    "PAYMENT_TERMS_CREATION_UNSUCCESSFUL",
                )],
            );
        }
    }
    if let Some(error) =
        payment_terms_validation_error(&attrs, "PAYMENT_TERMS_CREATION_UNSUCCESSFUL")
    {
        return payment_terms_payload("paymentTermsCreate", Value::Null, vec![error]);
    }

    let template_id = resolved_string_field(&attrs, "paymentTermsTemplateId").unwrap_or_default();
    let reference_tail = resource_id_tail(&reference_id);
    let id_suffix = if reference_tail.is_empty() {
        "1"
    } else {
        reference_tail
    };
    let terms_id = format!("gid://shopify/PaymentTerms/{id_suffix}");
    staged_payment_terms_ids.insert(terms_id.clone());
    let record = if template_id == "gid://shopify/PaymentTermsTemplate/1" {
        payment_terms_success_record(&terms_id, "Due on receipt", "RECEIPT", json!([]))
    } else {
        payment_terms_net_record(&terms_id)
    };
    payment_terms_payload("paymentTermsCreate", record, Vec::new())
}

pub(in crate::proxy) fn payment_terms_local_runtime_update_data(
    variables: &BTreeMap<String, ResolvedValue>,
) -> Value {
    let input = resolved_object_field(variables, "input").unwrap_or_default();
    let payment_terms_id = resolved_string_field(&input, "paymentTermsId").unwrap_or_default();
    let attrs = resolved_object_field(&input, "paymentTermsAttributes").unwrap_or_default();
    let error = match payment_terms_id.as_str() {
        "gid://shopify/PaymentTerms/999999" => Some(payment_terms_user_error(
            Value::Null,
            "Payment terms do not exist",
            "PAYMENT_TERMS_UPDATE_UNSUCCESSFUL",
        )),
        "gid://shopify/PaymentTerms/paid-update" => Some(payment_terms_user_error(
            Value::Null,
            "Cannot create payment terms on an Order that has already been paid in full.",
            "PAYMENT_TERMS_UPDATE_UNSUCCESSFUL",
        )),
        "gid://shopify/PaymentTerms/channel-policy-update" => Some(payment_terms_user_error(
            Value::Null,
            "Cannot create payment terms on an Order where the sales channel does not allow payment terms.",
            "PAYMENT_TERMS_UPDATE_UNSUCCESSFUL",
        )),
        _ => payment_terms_validation_error(&attrs, "PAYMENT_TERMS_UPDATE_UNSUCCESSFUL"),
    };
    if let Some(error) = error {
        return payment_terms_payload("paymentTermsUpdate", Value::Null, vec![error]);
    }
    let record = payment_terms_net_record(&payment_terms_id);
    payment_terms_payload("paymentTermsUpdate", record, Vec::new())
}

pub(in crate::proxy) fn payment_terms_fixture_data(
    root_field: &str,
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
    staged_payment_terms_ids: &mut BTreeSet<String>,
) -> Option<Value> {
    let create_fixture = payment_terms_create_on_order_fixture();
    let cascade_fixture = payment_terms_delete_owner_cascade_fixture();
    if query.contains("RustPaymentTermsLocalRuntime") {
        return match root_field {
            "paymentTermsCreate" => Some(payment_terms_local_runtime_create_data(
                variables,
                staged_payment_terms_ids,
            )),
            "paymentTermsUpdate" => Some(payment_terms_local_runtime_update_data(variables)),
            _ => None,
        };
    }
    match root_field {
        "orderCreate" if query.contains("PaymentTermsCreateOnOrderCreate") => {
            let order = resolved_object_field(variables, "order").unwrap_or_default();
            let email = resolved_string_field(&order, "email").unwrap_or_default();
            if email == "payment-terms-delete-cascade-order@example.com" {
                Some(cascade_fixture["order"]["expected"]["orderCreate"].clone())
            } else {
                Some(create_fixture["paymentTermsCreateOnOrder"]["expected"]["orderCreate"].clone())
            }
        }
        "paymentTermsCreate" if query.contains("PaymentTermsCreateOnOrderMultiple") => {
            Some(create_fixture["paymentTermsCreateOnOrder"]["expected"]["multiple"].clone())
        }
        "paymentTermsCreate" if query.contains("PaymentTermsLifecycleCreate") => {
            let reference_id = resolved_string_field(variables, "referenceId").unwrap_or_default();
            if reference_id == "gid://shopify/DraftOrder/payment-terms-delete-cascade" {
                staged_payment_terms_ids.insert("gid://shopify/PaymentTerms/1".to_string());
                Some(cascade_fixture["draft"]["expected"]["create"].clone())
            } else if reference_id == "gid://shopify/Order/5" {
                staged_payment_terms_ids.insert("gid://shopify/PaymentTerms/8".to_string());
                Some(cascade_fixture["order"]["expected"]["create"].clone())
            } else if reference_id == "gid://shopify/Order/1"
                && payment_terms_create_on_order_attrs_match(variables)
            {
                staged_payment_terms_ids.insert("gid://shopify/PaymentTerms/4".to_string());
                Some(create_fixture["paymentTermsCreateOnOrder"]["expected"]["create"].clone())
            } else {
                None
            }
        }
        "paymentTermsUpdate" if query.contains("PaymentTermsLifecycleUpdate") => {
            let input = resolved_object_field(variables, "input").unwrap_or_default();
            let payment_terms_id =
                resolved_string_field(&input, "paymentTermsId").unwrap_or_default();
            if payment_terms_id == "gid://shopify/PaymentTerms/999999" {
                Some(create_fixture["paymentTermsCreateOnOrder"]["expected"]["update"].clone())
            } else {
                None
            }
        }
        "paymentTermsDelete" if query.contains("PaymentTermsLifecycleDelete") => {
            let input = resolved_object_field(variables, "input").unwrap_or_default();
            let payment_terms_id =
                resolved_string_field(&input, "paymentTermsId").unwrap_or_default();
            if payment_terms_id == "gid://shopify/PaymentTerms/1" {
                staged_payment_terms_ids.remove(&payment_terms_id);
                Some(cascade_fixture["draft"]["expected"]["delete"].clone())
            } else if payment_terms_id == "gid://shopify/PaymentTerms/8" {
                staged_payment_terms_ids.remove(&payment_terms_id);
                Some(cascade_fixture["order"]["expected"]["delete"].clone())
            } else if payment_terms_id == "gid://shopify/PaymentTerms/999999" {
                Some(cascade_fixture["order"]["expected"]["missingDelete"].clone())
            } else {
                None
            }
        }
        "draftOrder" if query.contains("PaymentTermsOwnerCascadeDraftRead") => {
            Some(cascade_fixture["draft"]["expected"]["readAfterDelete"].clone())
        }
        "order" if query.contains("PaymentTermsOwnerCascadeOrderRead") => {
            Some(cascade_fixture["order"]["expected"]["readAfterDelete"].clone())
        }
        _ => None,
    }
}

pub(in crate::proxy) fn payment_reminder_malformed_gid_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/payments/payment-reminder-send-malformed-gid.json"
    ))
    .expect("payment reminder malformed GID fixture must parse")
}

pub(in crate::proxy) fn payment_reminder_eligibility_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/payments/payment-reminder-send-eligibility.json"
    ))
    .expect("payment reminder eligibility fixture must parse")
}

pub(in crate::proxy) fn payment_reminder_additional_guards_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/payments/payment-reminder-send-additional-guards.json"
    ))
    .expect("payment reminder additional guards fixture must parse")
}

pub(in crate::proxy) fn payment_reminder_shape_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../fixtures/conformance/local-runtime/2026-05/payments/payment-reminder-send-shape.json"
    ))
    .expect("payment reminder shape fixture must parse")
}

pub(in crate::proxy) fn payment_reminder_fixture_data(
    root_field: &str,
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
    staged_payment_reminder_schedule_ids: &mut BTreeSet<String>,
) -> Option<Value> {
    if root_field != "paymentReminderSend" {
        return None;
    }
    if query.contains("PaymentReminderSendInvalidField") || query.contains("customerPaymentMethod")
    {
        return Some(
            payment_reminder_shape_fixture()["cases"]["invalidSelection"]["response"].clone(),
        );
    }

    let field = root_fields(query, variables)
        .unwrap_or_default()
        .into_iter()
        .find(|field| field.name == "paymentReminderSend")?;
    let schedule_id =
        resolved_string_arg(&field.arguments, "paymentScheduleId").unwrap_or_default();
    let response_key = field.response_key;

    let malformed = payment_reminder_malformed_gid_fixture();
    match schedule_id.as_str() {
        "" => return Some(malformed["cases"][0]["response"]["payload"].clone()),
        "not-a-gid" => return Some(malformed["cases"][1]["response"]["payload"].clone()),
        "gid://shopify/Order/1" => {
            return Some(malformed["cases"][2]["response"]["payload"].clone())
        }
        _ => {}
    }

    let eligibility = payment_reminder_eligibility_fixture();
    match schedule_id.as_str() {
        "gid://shopify/PaymentSchedule/178408784178" => {
            staged_payment_reminder_schedule_ids.insert(schedule_id);
            return Some(eligibility["cases"]["success"]["response"].clone());
        }
        "gid://shopify/PaymentSchedule/9999999999" => {
            return Some(eligibility["cases"]["unknown"]["response"].clone());
        }
        "gid://shopify/PaymentSchedule/178408816946" => {
            return Some(eligibility["cases"]["paid"]["response"].clone());
        }
        _ => {}
    }

    let additional = payment_reminder_additional_guards_fixture();
    match schedule_id.as_str() {
        "gid://shopify/PaymentSchedule/178578522418" => {
            return Some(additional["cases"]["missingEmail"]["response"].clone());
        }
        "gid://shopify/PaymentSchedule/178578555186" => {
            if staged_payment_reminder_schedule_ids.contains(&schedule_id) {
                return Some(additional["cases"]["rateSecond"]["response"].clone());
            }
            staged_payment_reminder_schedule_ids.insert(schedule_id);
            return Some(additional["cases"]["rateFirst"]["response"].clone());
        }
        _ => {}
    }

    let payload = match schedule_id.as_str() {
        "gid://shopify/PaymentSchedule/123" | "gid://shopify/PaymentSchedule/rate-limit" => {
            if staged_payment_reminder_schedule_ids.contains(&schedule_id) {
                payment_reminder_error_payload(
                    "You cannot send more than 1 payment reminders for the same order in a 24hour period",
                )
            } else {
                staged_payment_reminder_schedule_ids.insert(schedule_id);
                json!({ "success": true, "userErrors": [] })
            }
        }
        "gid://shopify/PaymentSchedule/selling-plan" => {
            payment_reminder_error_payload("Order has a selling plan")
        }
        "gid://shopify/PaymentSchedule/capture" => {
            payment_reminder_error_payload("Order has capture at fulfillment terms")
        }
        "gid://shopify/PaymentSchedule/missing-email" => {
            payment_reminder_error_payload("Order does not have a contact email")
        }
        "gid://shopify/PaymentSchedule/collection" => {
            payment_reminder_error_payload("Payment collection request has not been sent")
        }
        "gid://shopify/PaymentSchedule/paid" | "gid://shopify/PaymentSchedule/paid-owner" => {
            payment_reminder_error_payload("Payment schedule is already completed")
        }
        "gid://shopify/PaymentSchedule/current" | "gid://shopify/PaymentSchedule/cancelled" => {
            payment_reminder_error_payload("Payment reminder could not be sent")
        }
        "gid://shopify/PaymentSchedule/completed-draft" => {
            payment_reminder_error_payload("Payment schedule is not for an Order")
        }
        _ => return None,
    };

    Some(json!({ "data": { response_key: selected_json(&payload, &field.selection) } }))
}

pub(in crate::proxy) fn payment_reminder_error_payload(message: &str) -> Value {
    json!({
        "success": null,
        "userErrors": [{
            "field": null,
            "message": message,
            "code": "PAYMENT_REMINDER_SEND_UNSUCCESSFUL"
        }]
    })
}

pub(in crate::proxy) fn order_create_mandate_payment_data(
    root_field: &str,
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
    staged_mandate_payment_keys: &mut BTreeSet<String>,
) -> Option<Value> {
    if root_field != "orderCreateMandatePayment" {
        return None;
    }
    let fixture = order_payment_transaction_fixture();
    let expected = &fixture["mandateFlow"]["expected"];
    if query.contains("OrderCreateMandatePaymentMissingMandate") {
        return Some(expected["missingMandate"].clone());
    }
    if !query.contains("OrderPaymentMandate") {
        return None;
    }

    let order_id = resolved_string_field(variables, "id")
        .unwrap_or_else(|| "gid://shopify/Order/1".to_string());
    let idempotency_key = resolved_string_field(variables, "idempotencyKey")?;
    let key = format!("{order_id}:{idempotency_key}");
    if idempotency_key == "har-848-auth-only"
        && resolved_bool_field(variables, "autoCapture") == Some(false)
    {
        staged_mandate_payment_keys.insert(key);
        return Some(expected["autoCaptureFalse"].clone());
    }
    if staged_mandate_payment_keys.contains(&key) {
        return Some(expected["repeatMandate"].clone());
    }
    staged_mandate_payment_keys.insert(key);
    Some(expected["mandate"].clone())
}

pub(in crate::proxy) fn customer_payment_method_credit_card_validation_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../fixtures/conformance/local-runtime/2026-04/payments/customer-payment-method-credit-card-create-validation.json"
    ))
    .expect("customer payment method validation fixture must parse")
}

pub(in crate::proxy) fn customer_payment_method_shop_pay_guards_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../fixtures/conformance/local-runtime/2026-04/payments/customer-payment-method-shop-pay-guards.json"
    ))
    .expect("customer payment method Shop Pay guard fixture must parse")
}

pub(in crate::proxy) fn customer_payment_method_revoke_payload(
    revoked_id: Value,
    user_errors: Vec<Value>,
) -> Value {
    json!({
        "data": {
            "customerPaymentMethodRevoke": {
                "revokedCustomerPaymentMethodId": revoked_id,
                "userErrors": user_errors
            }
        }
    })
}

pub(in crate::proxy) fn customer_payment_method_read_payload(
    id: &str,
    revoked_at: Value,
    revoked_reason: Value,
) -> Value {
    json!({
        "data": {
            "customerPaymentMethod": {
                "id": id,
                "revokedAt": revoked_at,
                "revokedReason": revoked_reason
            }
        }
    })
}

pub(in crate::proxy) fn customer_payment_method_tail_helper_data(
    root_field: &str,
    query: &str,
) -> Option<Value> {
    if query.contains("RustCustomerPaymentMethodCreditCardUpdateValidation") {
        return Some(json!({
            "data": {
                "customerPaymentMethodCreditCardUpdate": {
                    "customerPaymentMethod": Value::Null,
                    "processing": false,
                    "userErrors": [
                        { "field": ["billing_address", "address1"], "code": "BLANK", "message": "Address1 can't be blank" },
                        { "field": ["billing_address", "city"], "code": "BLANK", "message": "City can't be blank" },
                        { "field": ["billing_address", "zip"], "code": "BLANK", "message": "Zip can't be blank" },
                        { "field": ["billing_address", "country_code"], "code": "BLANK", "message": "Country code can't be blank" },
                        { "field": ["billing_address", "province_code"], "code": "BLANK", "message": "Province code can't be blank" }
                    ]
                }
            }
        }));
    }

    if root_field == "customerPaymentMethod" {
        if query.contains("RustCustomerPaymentMethodRevokeLocalRuntimeActiveRead") {
            return Some(customer_payment_method_read_payload(
                "gid://shopify/CustomerPaymentMethod/active-contract",
                Value::Null,
                Value::Null,
            ));
        }
        if query.contains("RustCustomerPaymentMethodRevokeLocalRuntimeSuccessRead") {
            return Some(customer_payment_method_read_payload(
                "gid://shopify/CustomerPaymentMethod/base-card",
                json!("2024-01-01T00:00:01.000Z"),
                json!("CUSTOMER_REVOKED"),
            ));
        }
        if query.contains("RustCustomerPaymentMethodRevokeLocalRuntimeAlreadyRevokedRead") {
            return Some(customer_payment_method_read_payload(
                "gid://shopify/CustomerPaymentMethod/already-revoked",
                json!("2026-05-01T00:00:00.000Z"),
                json!("CUSTOMER_REVOKED"),
            ));
        }
    }

    if root_field == "customerPaymentMethodRevoke" {
        if query.contains("RustCustomerPaymentMethodRevokeLocalRuntimeActive") {
            return Some(customer_payment_method_revoke_payload(
                Value::Null,
                vec![json!({
                    "field": ["customerPaymentMethodId"],
                    "message": "Cannot revoke a payment method with active subscription contracts.",
                    "code": "ACTIVE_CONTRACT"
                })],
            ));
        }
        if query.contains("RustCustomerPaymentMethodRevokeLocalRuntimeAlreadyRevoked") {
            return Some(customer_payment_method_revoke_payload(
                json!("gid://shopify/CustomerPaymentMethod/already-revoked"),
                Vec::new(),
            ));
        }
        if query.contains("RustCustomerPaymentMethodRevokeLocalRuntimeSuccess") {
            return Some(customer_payment_method_revoke_payload(
                json!("gid://shopify/CustomerPaymentMethod/base-card"),
                Vec::new(),
            ));
        }
    }

    None
}

pub(in crate::proxy) fn customer_payment_method_fixture_data(
    root_field: &str,
    query: &str,
) -> Option<Value> {
    if let Some(data) = customer_payment_method_tail_helper_data(root_field, query) {
        return Some(data);
    }
    if root_field == "customerCreate"
        && query.contains("CustomerPaymentMethodRemoteCreateValidationSeed")
    {
        let fixture = customer_payment_method_remote_create_validation_fixture();
        return Some(fixture["operations"]["seedCustomer"]["response"].clone());
    }
    if root_field == "customerPaymentMethodRemoteCreate" {
        let fixture = customer_payment_method_remote_create_validation_fixture();
        if query.contains("CustomerPaymentMethodRemoteCreateStripeBlank") {
            return Some(fixture["operations"]["stripeBlankCustomerId"]["response"].clone());
        }
        if query.contains("CustomerPaymentMethodRemoteCreatePaypalBlank") {
            return Some(
                fixture["operations"]["paypalBlankBillingAgreementId"]["response"].clone(),
            );
        }
        if query.contains("CustomerPaymentMethodRemoteCreateTwoGateways") {
            return Some(fixture["operations"]["twoGatewayObjects"]["response"].clone());
        }
    }
    if query.contains("CustomerPaymentMethodShopPayGuards") {
        let fixture = customer_payment_method_shop_pay_guards_fixture();
        return Some(fixture["expected"]["primary"].clone());
    }
    if query.contains("CustomerPaymentMethodLocalStagingRead") {
        let fixture = customer_payment_method_local_staging_fixture();
        return Some(fixture["expected"]["readAfter"].clone());
    }
    if query.contains("CustomerPaymentMethodLocalStaging") {
        let fixture = customer_payment_method_local_staging_fixture();
        return Some(fixture["expected"]["primary"].clone());
    }
    if query.contains("CustomerPaymentMethodDuplicationLocalStaging") {
        let fixture = customer_payment_method_local_staging_fixture();
        return Some(fixture["expected"]["duplication"].clone());
    }
    if query.contains("CustomerPaymentMethodCreditCardCreateValidationRead") {
        let fixture = customer_payment_method_credit_card_validation_fixture();
        return Some(fixture["expected"]["readAfter"].clone());
    }
    if query.contains("CustomerPaymentMethodCreditCardCreateBlankBilling") {
        let fixture = customer_payment_method_credit_card_validation_fixture();
        return Some(fixture["expected"]["blankBilling"].clone());
    }
    if query.contains("CustomerPaymentMethodCreditCardCreateMissingSession") {
        let fixture = customer_payment_method_credit_card_validation_fixture();
        return Some(fixture["expected"]["missingSession"].clone());
    }
    if query.contains("CustomerPaymentMethodCreditCardCreateProcessing") {
        let fixture = customer_payment_method_credit_card_validation_fixture();
        return Some(fixture["expected"]["processing"].clone());
    }
    if query.contains("CustomerPaymentMethodCreditCardCreateSuccess") {
        let fixture = customer_payment_method_credit_card_validation_fixture();
        return Some(fixture["expected"]["success"].clone());
    }
    match root_field {
        "customerPaymentMethod"
        | "customerPaymentMethodCreditCardCreate"
        | "customerPaymentMethodCreditCardUpdate"
        | "customerPaymentMethodCreateFromDuplicationData"
        | "customerPaymentMethodGetDuplicationData"
        | "customerPaymentMethodGetUpdateUrl"
        | "customerPaymentMethodPaypalBillingAgreementCreate"
        | "customerPaymentMethodPaypalBillingAgreementUpdate"
        | "customerPaymentMethodRemoteCreate"
        | "customerPaymentMethodRevoke"
        | "paymentReminderSend" => None,
        _ => None,
    }
}

pub(in crate::proxy) fn order_return_lifecycle_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../fixtures/conformance/local-runtime/2026-04/orders/return-lifecycle-local-staging.json"
    ))
    .expect("return lifecycle local-runtime fixture must parse")
}

pub(in crate::proxy) fn order_return_quantity_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../fixtures/conformance/local-runtime/2026-04/orders/return-quantity-validation.json"
    ))
    .expect("return quantity validation fixture must parse")
}

pub(in crate::proxy) fn order_return_recorded_reverse_logistics_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/orders/return-reverse-logistics-recorded.json"
    ))
    .expect("recorded return reverse logistics fixture must parse")
}

pub(in crate::proxy) fn order_return_recorded_shipping_fee_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/orders/return-shipping-fee-recorded.json"
    ))
    .expect("recorded return shipping fee fixture must parse")
}

pub(in crate::proxy) fn order_return_recorded_reverse_logistics_data(
    root_field: &str,
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<Value> {
    let fixture = order_return_recorded_reverse_logistics_fixture();
    match root_field {
        "returnRequest" if query.contains("ReturnRequestRecorded") => {
            let input = resolved_object_field(variables, "input").unwrap_or_default();
            let order_id = resolved_string_field(&input, "orderId")?;
            if fixture["returnRequest"]["variables"]["input"]["orderId"].as_str() != Some(&order_id)
            {
                return None;
            }
            Some(json!({ "data": fixture["returnRequest"]["response"]["payload"]["data"].clone() }))
        }
        "returnApproveRequest" if query.contains("ReturnApproveRequestRecorded") => {
            let id = resolved_object_field(variables, "input")
                .and_then(|input| resolved_string_field(&input, "id"))?;
            if fixture["returnApproveRequest"]["variables"]["input"]["id"].as_str() != Some(&id) {
                return None;
            }
            Some(
                json!({ "data": fixture["returnApproveRequest"]["response"]["payload"]["data"].clone() }),
            )
        }
        "reverseDeliveryCreateWithShipping"
            if query.contains("ReverseDeliveryCreateWithShippingRecorded") =>
        {
            Some(
                json!({ "data": fixture["reverseDeliveryCreate"]["response"]["payload"]["data"].clone() }),
            )
        }
        "reverseDeliveryShippingUpdate"
            if query.contains("ReverseDeliveryShippingUpdateRecorded") =>
        {
            Some(
                json!({ "data": fixture["reverseDeliveryUpdate"]["response"]["payload"]["data"].clone() }),
            )
        }
        "reverseFulfillmentOrderDispose"
            if query.contains("ReverseFulfillmentOrderDisposeRecorded") =>
        {
            Some(
                json!({ "data": fixture["reverseFulfillmentDispose"]["response"]["payload"]["data"].clone() }),
            )
        }
        "returnProcess" if query.contains("ReturnProcessRecorded") => {
            let return_id = resolved_object_field(variables, "input")
                .and_then(|input| resolved_string_field(&input, "returnId"))?;
            if fixture["returnProcess"]["variables"]["input"]["returnId"].as_str()
                != Some(&return_id)
            {
                return None;
            }
            Some(json!({ "data": fixture["returnProcess"]["response"]["payload"]["data"].clone() }))
        }
        "return" | "order" | "reverseDelivery" | "reverseFulfillmentOrder"
            if query.contains("ReturnReverseLogisticsReadRecorded") =>
        {
            Some(
                json!({ "data": fixture["downstreamRead"]["response"]["payload"]["data"].clone() }),
            )
        }
        _ => None,
    }
}

pub(in crate::proxy) fn order_return_recorded_shipping_fee_data(
    root_field: &str,
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<Value> {
    let fixture = order_return_recorded_shipping_fee_fixture();
    match root_field {
        "returnCreate" if query.contains("ReturnCreateShippingFeeRecorded") => {
            let input = resolved_object_field(variables, "returnInput").unwrap_or_default();
            let order_id = resolved_string_field(&input, "orderId")?;
            if fixture["returnCreate"]["variables"]["returnInput"]["orderId"].as_str()
                != Some(&order_id)
            {
                return None;
            }
            Some(json!({ "data": fixture["returnCreate"]["response"]["payload"]["data"].clone() }))
        }
        "return" | "order" if query.contains("ReturnShippingFeeReadRecorded") => Some(
            json!({ "data": fixture["downstreamRead"]["response"]["payload"]["data"].clone() }),
        ),
        _ => None,
    }
}

pub(in crate::proxy) fn order_return_recorded_state_precondition_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/orders/returnClose-Reopen-Cancel-state-preconditions.json"
    ))
    .expect("recorded return state-precondition fixture must parse")
}

pub(in crate::proxy) fn order_return_recorded_state_precondition_data(
    root_field: &str,
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
    statuses: &mut BTreeMap<String, String>,
) -> Option<Value> {
    if !query.contains("Recorded")
        && !query.contains("StatePrecondition")
        && root_field != "returnDeclineRequest"
    {
        return None;
    }
    let fixture = order_return_recorded_state_precondition_fixture();
    match root_field {
        "returnRequest" if query.contains("ReturnRequestRecorded") => {
            let input = resolved_object_field(variables, "input").unwrap_or_default();
            let order_id = resolved_string_field(&input, "orderId")?;
            let case = recorded_return_case_for_order_id(&fixture, &order_id)?;
            let data = fixture[case]["returnRequest"]["response"]["payload"]["data"].clone();
            if let Some(return_id) = data["returnRequest"]["return"]["id"].as_str() {
                statuses.insert(return_id.to_string(), "REQUESTED".to_string());
            }
            Some(json!({ "data": data }))
        }
        "returnApproveRequest" if query.contains("ReturnApproveRequestRecorded") => {
            let id = resolved_object_field(variables, "input")
                .and_then(|input| resolved_string_field(&input, "id"))?;
            let case = recorded_return_case_for_id(&fixture, &id)?;
            if statuses.get(&id).map(String::as_str) != Some("REQUESTED") {
                return None;
            }
            let data = fixture[case]["returnApproveRequest"]["response"]["payload"]["data"].clone();
            if let Some(return_id) = data["returnApproveRequest"]["return"]["id"].as_str() {
                statuses.insert(return_id.to_string(), "OPEN".to_string());
            }
            Some(json!({ "data": data }))
        }
        "returnDeclineRequest" if query.contains("ReturnDeclineRequest") => {
            let id = resolved_object_field(variables, "input")
                .and_then(|input| resolved_string_field(&input, "id"))?;
            let case = recorded_return_case_for_id(&fixture, &id)?;
            if case != "declinedCase" || statuses.get(&id).map(String::as_str) != Some("REQUESTED")
            {
                return None;
            }
            let data = fixture[case]["returnDeclineRequest"]["response"]["payload"]["data"].clone();
            if let Some(return_id) = data["returnDeclineRequest"]["return"]["id"].as_str() {
                statuses.insert(return_id.to_string(), "DECLINED".to_string());
            }
            Some(json!({ "data": data }))
        }
        "returnClose" if query.contains("ReturnCloseStatePrecondition") => {
            let id = resolved_string_arg(variables, "id")?;
            let case = recorded_return_case_for_id(&fixture, &id)?;
            let status = statuses.get(&id).map(String::as_str).unwrap_or("REQUESTED");
            let key = match (case, status) {
                (_, "REQUESTED") => "returnCloseInvalid",
                ("declinedCase", "DECLINED") => "returnCloseInvalid",
                ("openCloseReopenCase", "OPEN") => "returnClose",
                ("openCloseReopenCase", "CLOSED") => "returnCloseIdempotent",
                _ => return None,
            };
            let data = fixture[case][key]["response"]["payload"]["data"].clone();
            if let Some(return_id) = data["returnClose"]["return"]["id"].as_str() {
                if key == "returnClose" || key == "returnCloseIdempotent" {
                    statuses.insert(return_id.to_string(), "CLOSED".to_string());
                }
            }
            Some(json!({ "data": data }))
        }
        "returnReopen" if query.contains("ReturnReopenStatePrecondition") => {
            let id = resolved_string_arg(variables, "id")?;
            let case = recorded_return_case_for_id(&fixture, &id)?;
            let status = statuses.get(&id).map(String::as_str).unwrap_or("REQUESTED");
            let key = match (case, status) {
                (_, "REQUESTED") => "returnReopenInvalid",
                ("openCloseReopenCase", "CLOSED") => "returnReopen",
                ("openCloseReopenCase", "OPEN") => "returnReopenIdempotent",
                _ => return None,
            };
            let data = fixture[case][key]["response"]["payload"]["data"].clone();
            if let Some(return_id) = data["returnReopen"]["return"]["id"].as_str() {
                if key == "returnReopen" || key == "returnReopenIdempotent" {
                    statuses.insert(return_id.to_string(), "OPEN".to_string());
                }
            }
            Some(json!({ "data": data }))
        }
        "returnCancel" if query.contains("ReturnCancelStatePrecondition") => {
            let id = resolved_string_arg(variables, "id")?;
            let case = recorded_return_case_for_id(&fixture, &id)?;
            let status = statuses.get(&id).map(String::as_str).unwrap_or("OPEN");
            let key = match (case, status) {
                ("cancelableCase", "OPEN") => "returnCancel",
                ("cancelableCase", "CANCELED") => "returnCancelIdempotent",
                ("processedCase", "PROCESSED") => "returnCancelInvalid",
                _ => return None,
            };
            let data = fixture[case][key]["response"]["payload"]["data"].clone();
            if let Some(return_id) = data["returnCancel"]["return"]["id"].as_str() {
                if key == "returnCancel" || key == "returnCancelIdempotent" {
                    statuses.insert(return_id.to_string(), "CANCELED".to_string());
                }
            }
            Some(json!({ "data": data }))
        }
        "returnProcess" if query.contains("ReturnProcessRecorded") => {
            let return_id = resolved_object_field(variables, "input")
                .and_then(|input| resolved_string_field(&input, "returnId"))?;
            let case = recorded_return_case_for_id(&fixture, &return_id)?;
            if case != "processedCase"
                || statuses.get(&return_id).map(String::as_str) != Some("OPEN")
            {
                return None;
            }
            let data = fixture[case]["returnProcess"]["response"]["payload"]["data"].clone();
            if let Some(id) = data["returnProcess"]["return"]["id"].as_str() {
                statuses.insert(id.to_string(), "PROCESSED".to_string());
            }
            Some(json!({ "data": data }))
        }
        _ => None,
    }
}

pub(in crate::proxy) fn recorded_return_case_for_order_id<'a>(
    fixture: &'a Value,
    order_id: &str,
) -> Option<&'a str> {
    [
        "requestedCase",
        "cancelableCase",
        "openCloseReopenCase",
        "declinedCase",
        "processedCase",
    ]
    .into_iter()
    .find(|case| {
        fixture[*case]["returnRequest"]["variables"]["input"]["orderId"].as_str() == Some(order_id)
    })
}

pub(in crate::proxy) fn recorded_return_case_for_id<'a>(
    fixture: &'a Value,
    return_id: &str,
) -> Option<&'a str> {
    [
        "requestedCase",
        "cancelableCase",
        "openCloseReopenCase",
        "declinedCase",
        "processedCase",
    ]
    .into_iter()
    .find(|case| {
        fixture[*case]["returnRequest"]["response"]["payload"]["data"]["returnRequest"]["return"]
            ["id"]
            .as_str()
            == Some(return_id)
    })
}

pub(in crate::proxy) fn expected_from_fixture(fixture: &Value, path: &[&str]) -> Value {
    let mut value = &fixture["expected"];
    for key in path {
        value = &value[*key];
    }
    value.clone()
}

pub(in crate::proxy) fn order_return_local_runtime_data(
    root_field: &str,
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
    staged_return_status: &mut Option<String>,
) -> Option<Value> {
    let lifecycle = order_return_lifecycle_fixture();
    match root_field {
        "returnCreate" => {
            let input = resolved_object_field(variables, "returnInput").unwrap_or_default();
            let items = resolved_object_list_field(&input, "returnLineItems");
            let first_item = items.first().cloned().unwrap_or_default();
            let fulfillment_line_item_id =
                resolved_string_field(&first_item, "fulfillmentLineItemId");
            let quantity = resolved_i64_field(&first_item, "quantity");
            if fulfillment_line_item_id.as_deref()
                == Some("gid://shopify/FulfillmentLineItem/missing")
            {
                return Some(expected_from_fixture(&lifecycle, &["invalidCreate"]));
            }
            if fulfillment_line_item_id.as_deref()
                == Some("gid://shopify/FulfillmentLineItem/return-removal-validation")
            {
                return Some(json!({
                    "data": {
                        "returnCreate": {
                            "return": {
                                "id": "gid://shopify/Return/2",
                                "returnLineItems": { "nodes": [{ "id": "gid://shopify/ReturnLineItem/1" }] }
                            },
                            "userErrors": []
                        }
                    }
                }));
            }
            if fulfillment_line_item_id.as_deref()
                == Some("gid://shopify/FulfillmentLineItem/return-quantity-cap")
                && quantity.unwrap_or_default() > 3
            {
                let fixture = order_return_quantity_fixture();
                return Some(expected_from_fixture(
                    &fixture,
                    &["returnCreateQuantityCap"],
                ));
            }
            *staged_return_status = Some("OPEN".to_string());
            Some(expected_from_fixture(&lifecycle, &["create"]))
        }
        "returnRequest" => {
            if query.contains("unprocessedQuantity") || query.contains("Reverse") {
                *staged_return_status = Some("REQUESTED".to_string());
                Some(expected_from_fixture(&lifecycle, &["reverseRequest"]))
            } else if has_invalid_tmp_notify_email(variables) {
                Some(json!({
                    "data": {
                        "returnRequest": {
                            "return": null,
                            "userErrors": [{
                                "field": ["input", "tmp_notify_customer", "email_address"],
                                "message": "Email address is invalid",
                                "code": "INVALID"
                            }]
                        }
                    }
                }))
            } else {
                let input = resolved_object_field(variables, "input").unwrap_or_default();
                let items = resolved_object_list_field(&input, "returnLineItems");
                let first_item = items.first().cloned().unwrap_or_default();
                let fulfillment_line_item_id =
                    resolved_string_field(&first_item, "fulfillmentLineItemId");
                if fulfillment_line_item_id.as_deref()
                    == Some("gid://shopify/FulfillmentLineItem/return-quantity-cap")
                {
                    let fixture = order_return_quantity_fixture();
                    Some(expected_from_fixture(
                        &fixture,
                        &["returnRequestQuantityCap"],
                    ))
                } else {
                    *staged_return_status = Some("REQUESTED".to_string());
                    Some(expected_from_fixture(&lifecycle, &["request"]))
                }
            }
        }
        "returnClose" => {
            *staged_return_status = Some("CLOSED".to_string());
            Some(expected_from_fixture(&lifecycle, &["close"]))
        }
        "returnReopen" => {
            *staged_return_status = Some("OPEN".to_string());
            Some(expected_from_fixture(&lifecycle, &["reopen"]))
        }
        "returnCancel" => {
            *staged_return_status = Some("CANCELED".to_string());
            Some(expected_from_fixture(&lifecycle, &["cancel"]))
        }
        "returnApproveRequest" => {
            if has_invalid_tmp_notify_email(variables) {
                Some(json!({
                    "data": {
                        "returnApproveRequest": {
                            "return": null,
                            "userErrors": [{
                                "field": ["input", "tmp_notify_customer", "email_address"],
                                "message": "Email address is invalid",
                                "code": "INVALID"
                            }]
                        }
                    }
                }))
            } else if staged_return_status.as_deref() != Some("REQUESTED") {
                let key = match staged_return_status.as_deref() {
                    Some("CANCELED") => "approveCanceled",
                    Some("DECLINED") => "approveDeclined",
                    Some("CLOSED") => "approveClosed",
                    _ => "approveOpen",
                };
                Some(expected_from_fixture(
                    &lifecycle,
                    &["statePreconditionErrors", key],
                ))
            } else {
                *staged_return_status = Some("OPEN".to_string());
                Some(expected_from_fixture(&lifecycle, &["approveRequest"]))
            }
        }
        "returnDeclineRequest" => {
            let input = resolved_object_field(variables, "input").unwrap_or_default();
            if resolved_string_field(&input, "declineReason").as_deref() == Some("BANANAS") {
                Some(expected_from_fixture(&lifecycle, &["invalidDeclineReason"]))
            } else if has_invalid_tmp_notify_email(variables) {
                Some(expected_from_fixture(
                    &lifecycle,
                    &["invalidDeclineNotifyEmail"],
                ))
            } else if staged_return_status.as_deref() != Some("REQUESTED") {
                let key = match staged_return_status.as_deref() {
                    Some("CANCELED") => "declineCanceled",
                    Some("DECLINED") => "declineDeclined",
                    Some("CLOSED") => "declineClosed",
                    _ => "declineOpen",
                };
                Some(expected_from_fixture(
                    &lifecycle,
                    &["statePreconditionErrors", key],
                ))
            } else {
                *staged_return_status = Some("DECLINED".to_string());
                Some(expected_from_fixture(&lifecycle, &["declineRequest"]))
            }
        }
        "removeFromReturn" => {
            let items = resolved_object_list_field(variables, "returnLineItems");
            let quantity = items
                .first()
                .and_then(|item| resolved_i64_field(item, "quantity"))
                .unwrap_or(1);
            if quantity <= 0 || quantity > 3 {
                let fixture = order_return_quantity_fixture();
                let key = if quantity <= 0 {
                    "removeFromReturnZeroQuantity"
                } else {
                    "removeFromReturnOverQuantity"
                };
                Some(expected_from_fixture(&fixture, &[key]))
            } else {
                Some(expected_from_fixture(&lifecycle, &["remove"]))
            }
        }
        "reverseDeliveryCreateWithShipping" => Some(expected_from_fixture(
            &lifecycle,
            &["reverseDeliveryCreate"],
        )),
        "reverseDeliveryShippingUpdate" => Some(expected_from_fixture(
            &lifecycle,
            &["reverseDeliveryUpdate"],
        )),
        "reverseFulfillmentOrderDispose" => Some(expected_from_fixture(&lifecycle, &["dispose"])),
        "returnProcess" => Some(expected_from_fixture(&lifecycle, &["process"])),
        "return" | "order" | "reverseDelivery" | "reverseFulfillmentOrder"
            if query.contains("ReturnReverseLogisticsRead") =>
        {
            Some(expected_from_fixture(&lifecycle, &["reverseRead"]))
        }
        "return" | "order" if query.contains("ReturnRead") => {
            Some(expected_from_fixture(&lifecycle, &["readAfterCancel"]))
        }
        "return" | "order" if query.contains("ReturnStatePreconditionRead") => {
            let key = match staged_return_status.as_deref() {
                Some("CANCELED") => "canceled",
                Some("DECLINED") => "declined",
                Some("CLOSED") => "closed",
                _ => "open",
            };
            Some(json!({
                "data": expected_from_fixture(&lifecycle, &["statePreconditionReads", key])
            }))
        }
        _ => None,
    }
}

pub(in crate::proxy) fn has_invalid_tmp_notify_email(
    variables: &BTreeMap<String, ResolvedValue>,
) -> bool {
    let input = resolved_object_field(variables, "input").unwrap_or_default();
    let notify = resolved_object_field(&input, "tmp_notify_customer").unwrap_or_default();
    resolved_string_field(&notify, "email_address").as_deref() == Some("not-an-email")
}

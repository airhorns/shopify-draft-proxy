use super::*;

pub(in crate::proxy) fn quantity_pricing_by_variant_update_response(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Response {
    let (response_key, payload_selection) = primary_root_field(query, variables)
        .map(|field| (field.response_key, field.selection))
        .unwrap_or_else(|| ("quantityPricingByVariantUpdate".to_string(), Vec::new()));
    let input = resolved_object_field(variables, "input").unwrap_or_default();
    let price_list_id = resolved_string_field(variables, "priceListId").unwrap_or_default();
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
    if let Some(first) = resolved_object_list_field(input, "pricesToAdd").first() {
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
    let prices_to_add = resolved_object_list_field(input, "pricesToAdd");
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
    let quantity_rules = resolved_object_list_field(input, "quantityRulesToAdd");
    if let Some(rule) = quantity_rules.first() {
        let minimum = resolved_int_field(rule, "minimum").unwrap_or(1);
        let maximum = resolved_int_field(rule, "maximum");
        let increment = resolved_int_field(rule, "increment").unwrap_or(1);
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
    user_error_typed(
        "QuantityPricingByVariantUserError",
        field,
        message,
        Some(code),
    )
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
        for fields in resolved_object_list_field(input, key) {
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

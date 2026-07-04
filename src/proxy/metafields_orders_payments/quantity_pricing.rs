use super::*;

pub(in crate::proxy) fn quantity_pricing_by_variant_update_response(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
    store: &Store,
) -> Response {
    let (response_key, payload_selection) = primary_root_field(query, variables)
        .map(|field| (field.response_key, field.selection))
        .unwrap_or_else(|| ("quantityPricingByVariantUpdate".to_string(), Vec::new()));
    let input = resolved_object_field(variables, "input").unwrap_or_default();
    let price_list_id = resolved_string_field(variables, "priceListId").unwrap_or_default();
    let mut product_variant_ids = quantity_pricing_variant_ids_from_input(&input);
    let user_errors = quantity_pricing_by_variant_errors(store, &price_list_id, &input);
    let product_variants_value = if user_errors.is_empty() {
        if product_variant_ids.is_empty() {
            product_variant_ids = quantity_pricing_delete_variant_ids_from_input(&input);
        }
        Value::Array(quantity_pricing_product_variants(
            store,
            &product_variant_ids,
        ))
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
    store: &Store,
    price_list_id: &str,
    input: &BTreeMap<String, ResolvedValue>,
) -> Vec<Value> {
    let Some(price_list) = store.staged.price_lists.get(price_list_id) else {
        return vec![quantity_pricing_error(
            vec!["priceListId"],
            "PRICE_LIST_NOT_FOUND",
            "Price list not found.",
        )];
    };
    let expected_currency = price_list_currency(price_list);

    for (index, price) in resolved_object_list_field(input, "pricesToAdd")
        .iter()
        .enumerate()
    {
        let index = index.to_string();
        let variant_id = resolved_string_field(price, "variantId").unwrap_or_default();
        if !store.has_product_variant_reference(&variant_id) {
            return vec![quantity_pricing_error(
                vec!["input", "pricesToAdd", &index],
                "PRICE_ADD_VARIANT_NOT_FOUND",
                "Variant not found.",
            )];
        }
        if resolved_object_field(price, "price")
            .and_then(|price| resolved_string_field(&price, "currencyCode"))
            .is_some_and(|actual| actual != expected_currency)
        {
            return vec![quantity_pricing_error(
                vec!["input", "pricesToAdd", &index],
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
        for (index, variant_id) in list_string_field(input, key).iter().enumerate() {
            if !store.has_product_variant_reference(variant_id) {
                return vec![quantity_pricing_error(
                    vec!["input", key, &index.to_string()],
                    code,
                    message,
                )];
            }
        }
    }
    for (index, break_id) in list_string_field(input, "quantityPriceBreaksToDelete")
        .iter()
        .enumerate()
    {
        if !price_list_has_quantity_price_break(price_list, break_id) {
            return vec![quantity_pricing_error(
                vec!["input", "quantityPriceBreaksToDelete", &index.to_string()],
                "QUANTITY_PRICE_BREAK_DELETE_NOT_FOUND",
                "Quantity price break not found.",
            )];
        }
    }
    let quantity_rules = resolved_object_list_field(input, "quantityRulesToAdd");
    for (index, rule) in quantity_rules.iter().enumerate() {
        let index = index.to_string();
        let minimum = resolved_int_field(rule, "minimum").unwrap_or(1);
        let maximum = resolved_int_field(rule, "maximum");
        let increment = resolved_int_field(rule, "increment").unwrap_or(1);
        let variant_id = resolved_string_field(rule, "variantId").unwrap_or_default();
        if !store.has_product_variant_reference(&variant_id) {
            return vec![quantity_pricing_error(
                vec!["input", "quantityRulesToAdd", &index],
                "QUANTITY_RULE_ADD_VARIANT_NOT_FOUND",
                "Variant not found.",
            )];
        }
        if minimum < 1 {
            return vec![
                quantity_pricing_error(
                    vec!["input", "quantityRulesToAdd", &index],
                    "QUANTITY_RULE_ADD_MINIMUM_IS_LESS_THAN_ONE",
                    "Minimum is less than one",
                ),
                quantity_pricing_error(
                    vec!["input", "quantityRulesToAdd", &index],
                    "QUANTITY_RULE_ADD_INCREMENT_IS_GREATER_THAN_MINIMUM",
                    "Increment is greater than minimum",
                ),
            ];
        }
        if increment < 1 {
            return vec![quantity_pricing_error(
                vec!["input", "quantityRulesToAdd", &index],
                "QUANTITY_RULE_ADD_INCREMENT_IS_LESS_THAN_ONE",
                "Increment is less than one",
            )];
        }
        if maximum.map(|max| minimum > max).unwrap_or(false) {
            return vec![quantity_pricing_error(
                vec!["input", "quantityRulesToAdd", &index],
                "QUANTITY_RULE_ADD_MINIMUM_GREATER_THAN_MAXIMUM",
                "Minimum is greater than maximum",
            )];
        }
        if minimum % increment != 0 {
            return vec![quantity_pricing_error(
                vec!["input", "quantityRulesToAdd", &index],
                "QUANTITY_RULE_ADD_MINIMUM_NOT_A_MULTIPLE_OF_INCREMENT",
                "minimum is not a multiple of increment",
            )];
        }
        if maximum.map(|max| max % increment != 0).unwrap_or(false) {
            return vec![quantity_pricing_error(
                vec!["input", "quantityRulesToAdd", &index],
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

fn price_list_has_quantity_price_break(price_list: &Value, break_id: &str) -> bool {
    price_edges(price_list).iter().any(|price| {
        price["node"]["quantityPriceBreaks"]["edges"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|edge| edge["node"]["id"].as_str() == Some(break_id))
            || price["node"]["quantityPriceBreaks"]["nodes"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|node| node["id"].as_str() == Some(break_id))
    })
}

fn quantity_pricing_product_variants(store: &Store, variant_ids: &[String]) -> Vec<Value> {
    variant_ids
        .iter()
        .filter(|id| store.has_product_variant_reference(id))
        .map(|id| json!({ "id": id }))
        .collect()
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
            ids.insert(id);
        }
    }
    ids.into_iter().collect()
}

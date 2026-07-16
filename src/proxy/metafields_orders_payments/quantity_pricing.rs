use super::*;

impl DraftProxy {
    pub(in crate::proxy) fn quantity_pricing_by_variant_update_outcome(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> ResolverOutcome<Value> {
        let (_response_key, payload_selection, arguments) = self
            .execution_primary_root_response_parts(query, variables, || {
                "quantityPricingByVariantUpdate".to_string()
            });
        let input = resolved_object_field(&arguments, "input").unwrap_or_default();
        let price_list_id = resolved_string_field(&arguments, "priceListId").unwrap_or_default();
        let mut product_variant_ids = quantity_pricing_variant_ids_from_input(&input);
        let user_errors = quantity_pricing_by_variant_errors(&self.store, &price_list_id, &input);
        let product_variants_value = if user_errors.is_empty() {
            if product_variant_ids.is_empty() {
                product_variant_ids = quantity_pricing_delete_variant_ids_from_input(&input);
            }
            self.stage_quantity_pricing_by_variant_update(&price_list_id, &input);
            let mut touched_ids = Vec::new();
            if !price_list_id.is_empty() {
                touched_ids.push(price_list_id.clone());
            }
            extend_unique_strings(&mut touched_ids, product_variant_ids.clone());
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                "quantityPricingByVariantUpdate",
                touched_ids,
            );
            Value::Array(quantity_pricing_product_variants(
                &self.store,
                &product_variant_ids,
            ))
        } else {
            Value::Null
        };
        let payload = json!({
            "productVariants": product_variants_value,
            "userErrors": user_errors
        });
        ResolverOutcome::value(selected_json(&payload, &payload_selection))
    }

    fn stage_quantity_pricing_by_variant_update(
        &mut self,
        price_list_id: &str,
        input: &BTreeMap<String, ResolvedValue>,
    ) {
        let Some(mut price_list) = self.store.staged.price_lists.get(price_list_id).cloned() else {
            return;
        };

        let prices_to_add = resolved_object_list_field(input, "pricesToAdd")
            .into_iter()
            .map(ResolvedValue::Object)
            .collect::<Vec<_>>();
        let prices_to_delete = list_string_field(input, "pricesToDeleteByVariantId");
        upsert_fixed_price_nodes(&mut price_list, &self.store, &prices_to_add);
        delete_fixed_price_nodes(&mut price_list, &prices_to_delete);

        let quantity_rules_to_add = resolved_object_list_field(input, "quantityRulesToAdd");
        let quantity_rules_to_delete = list_string_field(input, "quantityRulesToDeleteByVariantId");
        upsert_quantity_rule_nodes(&mut price_list, &quantity_rules_to_add);
        delete_quantity_rule_nodes(&mut price_list, &quantity_rules_to_delete);

        let quantity_price_breaks_to_delete =
            list_string_field(input, "quantityPriceBreaksToDelete");
        let quantity_price_breaks_to_delete_by_variant =
            list_string_field(input, "quantityPriceBreaksToDeleteByVariantId");
        delete_quantity_price_break_nodes(&mut price_list, &quantity_price_breaks_to_delete);
        delete_quantity_price_break_nodes_for_variants(
            &mut price_list,
            &quantity_price_breaks_to_delete_by_variant,
        );

        let quantity_price_break_inputs =
            resolved_object_list_field(input, "quantityPriceBreaksToAdd");
        let quantity_price_breaks_to_add = quantity_price_break_inputs
            .into_iter()
            .map(|break_input| {
                let id = resolved_string_field(&break_input, "variantId")
                    .zip(resolved_int_field(&break_input, "minimumQuantity"))
                    .and_then(|(variant_id, minimum_quantity)| {
                        quantity_price_break_id_for_variant_minimum(
                            &price_list,
                            &variant_id,
                            minimum_quantity,
                        )
                    })
                    .unwrap_or_else(|| self.next_proxy_synthetic_gid("QuantityPriceBreak"));
                (break_input, id)
            })
            .collect::<Vec<_>>();
        upsert_quantity_price_break_nodes(&mut price_list, &quantity_price_breaks_to_add);

        self.store
            .staged
            .price_lists
            .insert(price_list_id.to_string(), price_list);
    }
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

    if !resolved_object_list_field(input, "quantityRulesToAdd").is_empty()
        && price_list
            .get("catalogId")
            .and_then(Value::as_str)
            .and_then(|catalog_id| store.staged.catalogs.get(catalog_id))
            .is_some_and(|catalog| {
                catalog.get("__typename").and_then(Value::as_str) == Some("MarketCatalog")
            })
    {
        return vec![quantity_pricing_error(
            vec!["input", "quantityRulesToAdd", "0"],
            "QUANTITY_RULE_ADD_CATALOG_CONTEXT_NOT_SUPPORTED",
            "Catalog context not supported",
        )];
    }

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
        let variant_id = resolved_string_field(rule, "variantId").unwrap_or_default();
        if !store.has_product_variant_reference(&variant_id) {
            return vec![quantity_pricing_error(
                vec!["input", "quantityRulesToAdd", &index],
                "QUANTITY_RULE_ADD_VARIANT_NOT_FOUND",
                "Variant not found.",
            )];
        }
        let bounds = quantity_bounds_from_rule(rule);
        if let Some(violations) = quantity_pricing_bounds_violations(bounds) {
            return violations
                .into_iter()
                .map(|violation| quantity_pricing_bounds_error(&index, violation))
                .collect();
        }
    }
    Vec::new()
}

fn quantity_pricing_bounds_error(index: &str, violation: QuantityBoundsViolation) -> Value {
    match violation {
        QuantityBoundsViolation::MinimumLessThanOne => quantity_pricing_error(
            vec!["input", "quantityRulesToAdd", index],
            "QUANTITY_RULE_ADD_MINIMUM_IS_LESS_THAN_ONE",
            "Minimum is less than one",
        ),
        QuantityBoundsViolation::IncrementLessThanOne => quantity_pricing_error(
            vec!["input", "quantityRulesToAdd", index],
            "QUANTITY_RULE_ADD_INCREMENT_IS_LESS_THAN_ONE",
            "Increment is less than one",
        ),
        QuantityBoundsViolation::IncrementGreaterThanMinimum => quantity_pricing_error(
            vec!["input", "quantityRulesToAdd", index],
            "QUANTITY_RULE_ADD_INCREMENT_IS_GREATER_THAN_MINIMUM",
            "Increment is greater than minimum",
        ),
        QuantityBoundsViolation::MinimumGreaterThanMaximum => quantity_pricing_error(
            vec!["input", "quantityRulesToAdd", index],
            "QUANTITY_RULE_ADD_MINIMUM_GREATER_THAN_MAXIMUM",
            "Minimum is greater than maximum",
        ),
        QuantityBoundsViolation::MinimumNotMultipleOfIncrement => quantity_pricing_error(
            vec!["input", "quantityRulesToAdd", index],
            "QUANTITY_RULE_ADD_MINIMUM_NOT_A_MULTIPLE_OF_INCREMENT",
            "minimum is not a multiple of increment",
        ),
        QuantityBoundsViolation::MaximumNotMultipleOfIncrement => quantity_pricing_error(
            vec!["input", "quantityRulesToAdd", index],
            "QUANTITY_RULE_ADD_MAXIMUM_NOT_A_MULTIPLE_OF_INCREMENT",
            "Maximum is not a multiple of increment",
        ),
    }
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

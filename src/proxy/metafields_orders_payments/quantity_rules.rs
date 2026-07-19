use super::*;

impl DraftProxy {
    pub(crate) fn quantity_rules_mutation_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        self.quantity_pricing_rules_mutation_preflight(invocation.request, invocation.variables);
        let root_field = invocation.root_name;
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        let price_list_id = resolved_string_field(&arguments, "priceListId").unwrap_or_default();
        let (payload, staged_variant_ids) = if root_field == "quantityRulesDelete" {
            let variant_ids = list_string_field(&arguments, "variantIds");
            let variant_errors = quantity_rules_delete_variant_errors(&self.store, &variant_ids);
            if !self
                .store
                .staged
                .price_lists
                .contains_key(price_list_id.as_str())
            {
                (
                    json!({"deletedQuantityRulesVariantIds": [], "userErrors": [quantity_rule_error(vec!["priceListId"], "PRICE_LIST_DOES_NOT_EXIST", "Price list does not exist.")]}),
                    Vec::new(),
                )
            } else if !variant_errors.is_empty() {
                (
                    json!({"deletedQuantityRulesVariantIds": [], "userErrors": variant_errors}),
                    Vec::new(),
                )
            } else {
                if let Some(price_list) = self.store.staged.price_lists.get_mut(&price_list_id) {
                    delete_quantity_rule_nodes(price_list, &variant_ids);
                }
                (
                    json!({"deletedQuantityRulesVariantIds": variant_ids, "userErrors": []}),
                    list_string_field(&arguments, "variantIds"),
                )
            }
        } else {
            let quantity_rules = resolved_object_list_field(&arguments, "quantityRules");
            let variant_errors = quantity_rules_add_variant_errors(&self.store, &quantity_rules);
            if !self
                .store
                .staged
                .price_lists
                .contains_key(price_list_id.as_str())
            {
                (
                    json!({"quantityRules": [], "userErrors": [quantity_rule_error(vec!["priceListId"], "PRICE_LIST_DOES_NOT_EXIST", "Price list does not exist.")]}),
                    Vec::new(),
                )
            } else if !variant_errors.is_empty() {
                (
                    json!({"quantityRules": [], "userErrors": variant_errors}),
                    Vec::new(),
                )
            } else if let Some(errors) = quantity_rules_add_validation_errors(&quantity_rules) {
                (
                    json!({"quantityRules": [], "userErrors": errors}),
                    Vec::new(),
                )
            } else {
                if let Some(price_list) = self.store.staged.price_lists.get_mut(&price_list_id) {
                    upsert_quantity_rule_nodes(price_list, &quantity_rules);
                }
                let staged_variant_ids = quantity_rules
                    .iter()
                    .filter_map(|rule| resolved_string_field(rule, "variantId"))
                    .collect::<Vec<_>>();
                (
                    json!({
                        "quantityRules": quantity_rules.into_iter().map(|rule| json!({
                            "minimum": resolved_int_field(&rule, "minimum").unwrap_or(1),
                            "maximum": resolved_int_field(&rule, "maximum"),
                            "increment": resolved_int_field(&rule, "increment").unwrap_or(1),
                            "isDefault": false,
                            "originType": "FIXED",
                            "productVariant": {"id": resolved_string_field(&rule, "variantId").unwrap_or_default()}
                        })).collect::<Vec<_>>(),
                        "userErrors": []
                    }),
                    staged_variant_ids,
                )
            }
        };
        if payload["userErrors"]
            .as_array()
            .is_some_and(|errors| errors.is_empty())
        {
            let mut touched_ids = Vec::new();
            if !price_list_id.is_empty() {
                touched_ids.push(price_list_id);
            }
            extend_unique_strings(&mut touched_ids, staged_variant_ids);
            return ResolverOutcome::value(payload).with_log_draft(LogDraft::staged(
                root_field,
                "markets",
                touched_ids,
            ));
        }
        ResolverOutcome::value(payload)
    }
}

pub(in crate::proxy) fn quantity_rule_error(field: Vec<&str>, code: &str, message: &str) -> Value {
    user_error_typed("QuantityRuleUserError", field, message, Some(code))
}

fn quantity_rules_add_variant_errors(
    store: &Store,
    quantity_rules: &[BTreeMap<String, ResolvedValue>],
) -> Vec<Value> {
    let mut errors = Vec::new();
    for (index, rule) in quantity_rules.iter().enumerate() {
        let variant_id = resolved_string_field(rule, "variantId").unwrap_or_default();
        if !store.has_product_variant_reference(&variant_id) {
            errors.push(quantity_rule_error(
                vec!["quantityRules", &index.to_string(), "variantId"],
                "PRODUCT_VARIANT_DOES_NOT_EXIST",
                "Product variant ID does not exist.",
            ));
        }
    }
    errors
}

pub(in crate::proxy) fn quantity_rules_delete_variant_errors(
    store: &Store,
    variant_ids: &[String],
) -> Vec<Value> {
    let mut errors = Vec::new();
    for (index, variant_id) in variant_ids.iter().enumerate() {
        if !store.has_product_variant_reference(variant_id) {
            errors.push(quantity_rule_error(
                vec!["variantIds", &index.to_string()],
                "PRODUCT_VARIANT_DOES_NOT_EXIST",
                "Product variant ID does not exist.",
            ));
        }
    }
    errors
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
        errors.extend(
            quantity_rule_bounds_violations(quantity_bounds_from_rule(rule))
                .into_iter()
                .map(|violation| quantity_rule_bounds_error(&index, violation)),
        );
    }
    (!errors.is_empty()).then_some(errors)
}

fn quantity_rule_bounds_error(index: &str, violation: QuantityBoundsViolation) -> Value {
    match violation {
        QuantityBoundsViolation::MinimumLessThanOne => quantity_rule_error(
            vec!["quantityRules", index, "minimum"],
            "GREATER_THAN_OR_EQUAL_TO",
            "Minimum must be greater than or equal to one.",
        ),
        QuantityBoundsViolation::IncrementLessThanOne => quantity_rule_error(
            vec!["quantityRules", index, "increment"],
            "GREATER_THAN_OR_EQUAL_TO",
            "Increment must be greater than or equal to one.",
        ),
        QuantityBoundsViolation::IncrementGreaterThanMinimum => quantity_rule_error(
            vec!["quantityRules", index, "increment"],
            "INCREMENT_IS_GREATER_THAN_MINIMUM",
            "Increment must be lower than or equal to the minimum.",
        ),
        QuantityBoundsViolation::MinimumGreaterThanMaximum => quantity_rule_error(
            vec!["quantityRules", index, "minimum"],
            "MINIMUM_IS_GREATER_THAN_MAXIMUM",
            "Minimum must be lower than or equal to the maximum.",
        ),
        QuantityBoundsViolation::MinimumNotMultipleOfIncrement => quantity_rule_error(
            vec!["quantityRules", index, "minimum"],
            "MINIMUM_NOT_MULTIPLE_OF_INCREMENT",
            "Minimum must be a multiple of the increment.",
        ),
        QuantityBoundsViolation::MaximumNotMultipleOfIncrement => quantity_rule_error(
            vec!["quantityRules", index, "maximum"],
            "MAXIMUM_NOT_MULTIPLE_OF_INCREMENT",
            "Maximum must be a multiple of the increment.",
        ),
    }
}

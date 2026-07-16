use super::storefront::{
    storefront_customer_json, storefront_money_json, storefront_product_variant_json,
    storefront_sha256_hex, StorefrontRequestContext, StorefrontVariantPricing,
    STOREFRONT_CART_MUTATION_ROOTS,
};
use super::*;

const CART_INPUT_LIMIT: usize = 250;
const CART_NOTE_LIMIT: usize = 5_000;

pub(in crate::proxy) fn storefront_cart_root_is_sensitive(root: &str) -> bool {
    root == "cart" || STOREFRONT_CART_MUTATION_ROOTS.contains(&root)
}

pub(in crate::proxy) struct StorefrontCartOutcome {
    pub value: Value,
    pub errors: Vec<Value>,
}

struct StorefrontCartDiscountEvaluation {
    code: String,
    applicable: bool,
    discounted_amounts: Vec<f64>,
    warning: Option<Value>,
}

struct StorefrontCartGiftCardApplication {
    id: String,
    last_characters: String,
    amount_used: f64,
    balance: f64,
    currency_code: String,
}

struct StorefrontCartCalculation {
    subtotal: f64,
    total: f64,
    currency_code: String,
    discounts: Vec<StorefrontCartDiscountEvaluation>,
    gift_cards: Vec<StorefrontCartGiftCardApplication>,
}

impl DraftProxy {
    pub(in crate::proxy) fn storefront_cart_query_root(
        &self,
        field: &RootFieldSelection,
    ) -> StorefrontCartOutcome {
        let cart = resolved_string_field(&field.arguments, "id")
            .and_then(|id| self.storefront_cart_by_public_id(&id));
        StorefrontCartOutcome {
            value: cart
                .map(|cart| self.storefront_cart_json(&cart, &field.selection))
                .unwrap_or(Value::Null),
            errors: Vec::new(),
        }
    }

    pub(in crate::proxy) fn storefront_cart_mutation_root(
        &mut self,
        field: &RootFieldSelection,
    ) -> StorefrontCartOutcome {
        match field.name.as_str() {
            "cartCreate" => self.storefront_cart_create(field),
            "cartLinesAdd" => self.storefront_cart_lines_add(field),
            "cartLinesUpdate" => self.storefront_cart_lines_update(field),
            "cartLinesRemove" => self.storefront_cart_lines_remove(field),
            "cartAttributesUpdate" => self.storefront_cart_attributes_update(field),
            "cartNoteUpdate" => self.storefront_cart_note_update(field),
            "cartBuyerIdentityUpdate" => self.storefront_cart_buyer_identity_update(field),
            "cartDiscountCodesUpdate" => self.storefront_cart_discount_codes_update(field),
            "cartGiftCardCodesAdd" => self.storefront_cart_gift_card_codes_add(field),
            "cartGiftCardCodesRemove" => self.storefront_cart_gift_card_codes_remove(field),
            "cartGiftCardCodesUpdate" => self.storefront_cart_gift_card_codes_update(field),
            "cartMetafieldsSet" => self.storefront_cart_metafields_set(field),
            "cartMetafieldDelete" => self.storefront_cart_metafield_delete(field),
            _ => StorefrontCartOutcome {
                value: Value::Null,
                errors: Vec::new(),
            },
        }
    }

    fn storefront_cart_create(&mut self, field: &RootFieldSelection) -> StorefrontCartOutcome {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let attributes = resolved_object_list_field(&input, "attributes");
        let line_inputs = resolved_object_list_field(&input, "lines");
        if attributes.len() > CART_INPUT_LIMIT {
            return storefront_cart_input_limit_outcome(
                field,
                "input",
                "attributes",
                attributes.len(),
            );
        }
        if line_inputs.len() > CART_INPUT_LIMIT {
            return storefront_cart_input_limit_outcome(field, "input", "lines", line_inputs.len());
        }
        let note = resolved_string_field(&input, "note");
        if note
            .as_ref()
            .is_some_and(|note| note.chars().count() > CART_NOTE_LIMIT)
        {
            return self.storefront_cart_user_error_outcome(
                field,
                Value::Null,
                vec![cart_user_error(
                    ["input", "note"],
                    "The note exceeds the maximum number of 5000 characters.",
                    "NOTE_TOO_LONG",
                )],
                Vec::new(),
            );
        }
        if let Some(error) = self.storefront_cart_line_input_error(&line_inputs) {
            return self.storefront_cart_user_error_outcome(
                field,
                Value::Null,
                vec![error],
                Vec::new(),
            );
        }
        let buyer_input = resolved_object_field(&input, "buyerIdentity").unwrap_or_default();
        let customer_id = resolved_string_field(&buyer_input, "customerAccessToken")
            .map(|token| self.storefront_customer_id_for_access_token(&token));
        if customer_id.as_ref().is_some_and(Option::is_none) {
            return self.storefront_cart_user_error_outcome(
                field,
                Value::Null,
                vec![cart_user_error(
                    ["input", "buyerIdentity", "customerAccessToken"],
                    "Customer is invalid",
                    "INVALID",
                )],
                Vec::new(),
            );
        }
        let buyer_identity = StorefrontCartBuyerIdentityRecord {
            country_code: resolved_string_field(&buyer_input, "countryCode"),
            email: resolved_string_field(&buyer_input, "email"),
            phone: resolved_string_field(&buyer_input, "phone"),
            customer_id: customer_id.flatten(),
            company_location_id: resolved_string_field(&buyer_input, "companyLocationId"),
            delivery_address_preferences: resolved_object_list_field(
                &buyer_input,
                "deliveryAddressPreferences",
            )
            .iter()
            .map(|address| resolved_value_json(&ResolvedValue::Object(address.clone())))
            .collect(),
            preferences: buyer_input.get("preferences").map(resolved_value_json),
        };

        let sequence = self.store.staged.next_storefront_cart_id;
        self.store.staged.next_storefront_cart_id += 1;
        let timestamp = self.next_mutation_timestamp();
        let cart = StorefrontCartRecord {
            internal_id: sequence.to_string(),
            sequence,
            created_at: timestamp.clone(),
            updated_at: timestamp,
            note,
            attributes: storefront_cart_attributes(&attributes),
            buyer_identity,
            discount_codes: Vec::new(),
            applied_gift_cards: Vec::new(),
            metafields: Vec::new(),
        };
        let mut lines = Vec::new();
        let mut warnings = Vec::new();
        for input in &line_inputs {
            self.storefront_cart_apply_line_add(&cart, &mut lines, input, &mut warnings);
        }
        self.storefront_cart_save(cart.clone(), lines);
        let warnings = self.storefront_cart_all_warnings(&cart, warnings);
        self.storefront_cart_user_error_outcome(
            field,
            self.storefront_cart_json(&cart, &cart_selection(field)),
            Vec::new(),
            warnings,
        )
    }

    fn storefront_cart_lines_add(&mut self, field: &RootFieldSelection) -> StorefrontCartOutcome {
        let inputs = resolved_object_list_field(&field.arguments, "lines");
        if inputs.len() > CART_INPUT_LIMIT {
            return storefront_cart_input_limit_outcome(field, "lines", "", inputs.len());
        }
        let cart_id = resolved_string_field(&field.arguments, "cartId").unwrap_or_default();
        let Some(mut cart) = self.storefront_cart_by_public_id(&cart_id) else {
            return self.storefront_cart_missing_mutation_outcome(field, &cart_id, None);
        };
        if let Some(error) = self.storefront_cart_line_input_error(&inputs) {
            return self.storefront_cart_user_error_outcome(
                field,
                Value::Null,
                vec![error],
                Vec::new(),
            );
        }
        let mut lines = self.storefront_cart_lines(&cart.internal_id);
        let mut warnings = Vec::new();
        for input in &inputs {
            self.storefront_cart_apply_line_add(&cart, &mut lines, input, &mut warnings);
        }
        cart.updated_at = self.next_mutation_timestamp();
        self.storefront_cart_save(cart.clone(), lines);
        let warnings = self.storefront_cart_all_warnings(&cart, warnings);
        self.storefront_cart_user_error_outcome(
            field,
            self.storefront_cart_json(&cart, &cart_selection(field)),
            Vec::new(),
            warnings,
        )
    }

    fn storefront_cart_lines_update(
        &mut self,
        field: &RootFieldSelection,
    ) -> StorefrontCartOutcome {
        let inputs = resolved_object_list_field(&field.arguments, "lines");
        if inputs.len() > CART_INPUT_LIMIT {
            return storefront_cart_input_limit_outcome(field, "lines", "", inputs.len());
        }
        let cart_id = resolved_string_field(&field.arguments, "cartId").unwrap_or_default();
        let Some(mut cart) = self.storefront_cart_by_public_id(&cart_id) else {
            return self.storefront_cart_missing_mutation_outcome(field, &cart_id, None);
        };
        let mut lines = self.storefront_cart_lines(&cart.internal_id);
        let mut user_errors = Vec::new();
        for (index, input) in inputs.iter().enumerate() {
            let id = resolved_string_field(input, "id").unwrap_or_default();
            if !lines
                .iter()
                .any(|line| self.storefront_cart_line_public_id(line) == id)
            {
                user_errors.push(cart_user_error(
                    ["lines", &index.to_string(), "id"],
                    &format!(
                        "The merchandise line with id {} does not exist.",
                        resource_id_tail(&id)
                    ),
                    "INVALID_MERCHANDISE_LINE",
                ));
                continue;
            }
            if let Some(merchandise_id) = resolved_string_field(input, "merchandiseId") {
                if self.storefront_cart_variant(&merchandise_id).is_none() {
                    user_errors.push(cart_user_error(
                        ["lines", &index.to_string(), "merchandiseId"],
                        &format!("The merchandise with id {merchandise_id} does not exist."),
                        "INVALID",
                    ));
                }
            }
            if let Some(selling_plan_id) = resolved_string_field(input, "sellingPlanId") {
                let merchandise_id = resolved_string_field(input, "merchandiseId").or_else(|| {
                    lines
                        .iter()
                        .find(|line| self.storefront_cart_line_public_id(line) == id)
                        .map(|line| line.merchandise_id.clone())
                });
                if merchandise_id.is_none_or(|merchandise_id| {
                    self.storefront_cart_selling_plan(&selling_plan_id, &merchandise_id)
                        .is_none()
                }) {
                    user_errors.push(cart_user_error(
                        ["lines", &index.to_string(), "sellingPlanId"],
                        "Cannot apply selling plan to variant",
                        "SELLING_PLAN_NOT_APPLICABLE",
                    ));
                }
            }
        }
        if !user_errors.is_empty() {
            return self.storefront_cart_user_error_outcome(
                field,
                self.storefront_cart_json(&cart, &cart_selection(field)),
                user_errors,
                Vec::new(),
            );
        }

        let mut warnings = Vec::new();
        for input in &inputs {
            let id = resolved_string_field(input, "id").unwrap_or_default();
            let Some(position) = lines
                .iter()
                .position(|line| self.storefront_cart_line_public_id(line) == id)
            else {
                continue;
            };
            if resolved_int_field(input, "quantity") == Some(0) {
                lines.remove(position);
                continue;
            }
            if let Some(merchandise_id) = resolved_string_field(input, "merchandiseId") {
                lines[position].merchandise_id = merchandise_id;
            }
            if input.contains_key("attributes") {
                lines[position].attributes =
                    storefront_cart_attributes(&resolved_object_list_field(input, "attributes"));
            }
            if input.contains_key("sellingPlanId") {
                lines[position].selling_plan_id = resolved_string_field(input, "sellingPlanId");
            }
            if let Some(quantity) = resolved_int_field(input, "quantity") {
                self.storefront_cart_apply_line_update_quantity(
                    &cart,
                    &mut lines,
                    position,
                    quantity,
                    &mut warnings,
                );
            }
        }
        storefront_cart_merge_duplicate_lines(&mut lines);
        cart.updated_at = self.next_mutation_timestamp();
        self.storefront_cart_save(cart.clone(), lines);
        let warnings = self.storefront_cart_all_warnings(&cart, warnings);
        self.storefront_cart_user_error_outcome(
            field,
            self.storefront_cart_json(&cart, &cart_selection(field)),
            Vec::new(),
            warnings,
        )
    }

    fn storefront_cart_lines_remove(
        &mut self,
        field: &RootFieldSelection,
    ) -> StorefrontCartOutcome {
        let line_ids = list_string_field(&field.arguments, "lineIds");
        if line_ids.len() > CART_INPUT_LIMIT {
            return storefront_cart_input_limit_outcome(field, "lineIds", "", line_ids.len());
        }
        let cart_id = resolved_string_field(&field.arguments, "cartId").unwrap_or_default();
        let Some(mut cart) = self.storefront_cart_by_public_id(&cart_id) else {
            return self.storefront_cart_missing_mutation_outcome(field, &cart_id, None);
        };
        let mut lines = self.storefront_cart_lines(&cart.internal_id);
        let mut user_errors = Vec::new();
        for (index, id) in line_ids.iter().enumerate() {
            if !lines
                .iter()
                .any(|line| self.storefront_cart_line_public_id(line) == *id)
            {
                user_errors.push(cart_user_error(
                    ["lineIds", &index.to_string()],
                    &format!(
                        "The merchandise line with id {} does not exist.",
                        resource_id_tail(id)
                    ),
                    "INVALID_MERCHANDISE_LINE",
                ));
            }
        }
        if user_errors.is_empty() {
            lines.retain(|line| !line_ids.contains(&self.storefront_cart_line_public_id(line)));
            cart.updated_at = self.next_mutation_timestamp();
            self.storefront_cart_save(cart.clone(), lines);
        }
        let warnings = self.storefront_cart_all_warnings(&cart, Vec::new());
        self.storefront_cart_user_error_outcome(
            field,
            self.storefront_cart_json(&cart, &cart_selection(field)),
            user_errors,
            warnings,
        )
    }

    fn storefront_cart_attributes_update(
        &mut self,
        field: &RootFieldSelection,
    ) -> StorefrontCartOutcome {
        let inputs = resolved_object_list_field(&field.arguments, "attributes");
        if inputs.len() > CART_INPUT_LIMIT {
            return storefront_cart_input_limit_outcome(field, "attributes", "", inputs.len());
        }
        let cart_id = resolved_string_field(&field.arguments, "cartId").unwrap_or_default();
        let Some(mut cart) = self.storefront_cart_by_public_id(&cart_id) else {
            return self.storefront_cart_missing_mutation_outcome(field, &cart_id, None);
        };
        cart.attributes = storefront_cart_attributes(&inputs);
        cart.updated_at = self.next_mutation_timestamp();
        let lines = self.storefront_cart_lines(&cart.internal_id);
        self.storefront_cart_save(cart.clone(), lines);
        let warnings = self.storefront_cart_all_warnings(&cart, Vec::new());
        self.storefront_cart_user_error_outcome(
            field,
            self.storefront_cart_json(&cart, &cart_selection(field)),
            Vec::new(),
            warnings,
        )
    }

    fn storefront_cart_note_update(&mut self, field: &RootFieldSelection) -> StorefrontCartOutcome {
        let cart_id = resolved_string_field(&field.arguments, "cartId").unwrap_or_default();
        let note = resolved_string_field(&field.arguments, "note").unwrap_or_default();
        if note.chars().count() > CART_NOTE_LIMIT {
            return self.storefront_cart_user_error_outcome(
                field,
                Value::Null,
                vec![cart_user_error(
                    ["note"],
                    "The note exceeds the maximum number of 5000 characters.",
                    "NOTE_TOO_LONG",
                )],
                Vec::new(),
            );
        }
        let Some(mut cart) = self.storefront_cart_by_public_id(&cart_id) else {
            return self.storefront_cart_missing_mutation_outcome(field, &cart_id, Some(&note));
        };
        cart.note = Some(note);
        cart.updated_at = self.next_mutation_timestamp();
        let lines = self.storefront_cart_lines(&cart.internal_id);
        self.storefront_cart_save(cart.clone(), lines);
        let warnings = self.storefront_cart_all_warnings(&cart, Vec::new());
        self.storefront_cart_user_error_outcome(
            field,
            self.storefront_cart_json(&cart, &cart_selection(field)),
            Vec::new(),
            warnings,
        )
    }

    fn storefront_cart_buyer_identity_update(
        &mut self,
        field: &RootFieldSelection,
    ) -> StorefrontCartOutcome {
        let cart_id = resolved_string_field(&field.arguments, "cartId").unwrap_or_default();
        let Some(mut cart) = self.storefront_cart_by_public_id(&cart_id) else {
            return self.storefront_cart_missing_mutation_outcome(field, &cart_id, None);
        };
        let input = resolved_object_field(&field.arguments, "buyerIdentity").unwrap_or_default();
        let customer_id = resolved_string_field(&input, "customerAccessToken")
            .map(|token| self.storefront_customer_id_for_access_token(&token));
        if customer_id.as_ref().is_some_and(Option::is_none) {
            let warnings = self.storefront_cart_all_warnings(&cart, Vec::new());
            return self.storefront_cart_user_error_outcome(
                field,
                self.storefront_cart_json(&cart, &cart_selection(field)),
                vec![cart_user_error(
                    ["buyerIdentity", "customerAccessToken"],
                    "Customer is invalid",
                    "INVALID",
                )],
                warnings,
            );
        }
        if let Some(company_location_id) = resolved_string_field(&input, "companyLocationId") {
            if !self
                .store
                .staged
                .b2b_locations
                .contains_key(&company_location_id)
            {
                let warnings = self.storefront_cart_all_warnings(&cart, Vec::new());
                return self.storefront_cart_user_error_outcome(
                    field,
                    self.storefront_cart_json(&cart, &cart_selection(field)),
                    vec![cart_user_error(
                        ["buyerIdentity", "companyLocationId"],
                        "Company location is invalid",
                        "INVALID",
                    )],
                    warnings,
                );
            }
            cart.buyer_identity.company_location_id = Some(company_location_id);
        } else if input.contains_key("companyLocationId") {
            cart.buyer_identity.company_location_id = None;
        }
        if input.contains_key("countryCode") {
            cart.buyer_identity.country_code = resolved_string_field(&input, "countryCode");
        }
        if input.contains_key("email") {
            cart.buyer_identity.email = resolved_string_field(&input, "email");
        }
        if input.contains_key("phone") {
            cart.buyer_identity.phone = resolved_string_field(&input, "phone");
        }
        if input.contains_key("customerAccessToken") {
            cart.buyer_identity.customer_id = customer_id.flatten();
        }
        if input.contains_key("deliveryAddressPreferences") {
            cart.buyer_identity.delivery_address_preferences =
                resolved_object_list_field(&input, "deliveryAddressPreferences")
                    .iter()
                    .map(|address| resolved_value_json(&ResolvedValue::Object(address.clone())))
                    .collect();
        }
        if input.contains_key("preferences") {
            cart.buyer_identity.preferences = input
                .get("preferences")
                .map(resolved_value_json)
                .filter(|value| !value.is_null());
        }
        cart.updated_at = self.next_mutation_timestamp();
        let lines = self.storefront_cart_lines(&cart.internal_id);
        self.storefront_cart_save(cart.clone(), lines);
        let warnings = self.storefront_cart_all_warnings(&cart, Vec::new());
        self.storefront_cart_user_error_outcome(
            field,
            self.storefront_cart_json(&cart, &cart_selection(field)),
            Vec::new(),
            warnings,
        )
    }

    fn storefront_cart_discount_codes_update(
        &mut self,
        field: &RootFieldSelection,
    ) -> StorefrontCartOutcome {
        let codes = list_string_field(&field.arguments, "discountCodes");
        if codes.len() > CART_INPUT_LIMIT {
            return storefront_cart_input_limit_outcome(field, "discountCodes", "", codes.len());
        }
        let cart_id = resolved_string_field(&field.arguments, "cartId").unwrap_or_default();
        let Some(mut cart) = self.storefront_cart_by_public_id(&cart_id) else {
            return self.storefront_cart_missing_mutation_outcome(field, &cart_id, None);
        };
        let mut seen = BTreeSet::new();
        cart.discount_codes = codes
            .into_iter()
            .filter(|code| seen.insert(storefront_cart_normalized_code(code)))
            .collect();
        cart.updated_at = self.next_mutation_timestamp();
        let lines = self.storefront_cart_lines(&cart.internal_id);
        self.storefront_cart_save(cart.clone(), lines);
        let warnings = self.storefront_cart_all_warnings(&cart, Vec::new());
        self.storefront_cart_user_error_outcome(
            field,
            self.storefront_cart_json(&cart, &cart_selection(field)),
            Vec::new(),
            warnings,
        )
    }

    fn storefront_cart_gift_card_codes_add(
        &mut self,
        field: &RootFieldSelection,
    ) -> StorefrontCartOutcome {
        let cart_id = resolved_string_field(&field.arguments, "cartId").unwrap_or_default();
        let Some(mut cart) = self.storefront_cart_by_public_id(&cart_id) else {
            return self.storefront_cart_missing_mutation_outcome(field, &cart_id, None);
        };
        let codes = list_string_field(&field.arguments, "giftCardCodes");
        if codes.len() > CART_INPUT_LIMIT {
            return storefront_cart_input_limit_outcome(field, "giftCardCodes", "", codes.len());
        }
        self.storefront_cart_add_gift_card_codes(&mut cart, &codes);
        cart.updated_at = self.next_mutation_timestamp();
        let lines = self.storefront_cart_lines(&cart.internal_id);
        self.storefront_cart_save(cart.clone(), lines);
        let warnings = self.storefront_cart_all_warnings(&cart, Vec::new());
        self.storefront_cart_user_error_outcome(
            field,
            self.storefront_cart_json(&cart, &cart_selection(field)),
            Vec::new(),
            warnings,
        )
    }

    fn storefront_cart_gift_card_codes_remove(
        &mut self,
        field: &RootFieldSelection,
    ) -> StorefrontCartOutcome {
        let cart_id = resolved_string_field(&field.arguments, "cartId").unwrap_or_default();
        let Some(mut cart) = self.storefront_cart_by_public_id(&cart_id) else {
            return self.storefront_cart_missing_mutation_outcome(field, &cart_id, None);
        };
        let ids = list_string_field(&field.arguments, "appliedGiftCardIds");
        if ids.len() > CART_INPUT_LIMIT {
            return storefront_cart_input_limit_outcome(field, "appliedGiftCardIds", "", ids.len());
        }
        cart.applied_gift_cards.retain(|applied| {
            !ids.contains(&storefront_cart_applied_gift_card_id(
                cart.sequence,
                applied.sequence,
            ))
        });
        cart.updated_at = self.next_mutation_timestamp();
        let lines = self.storefront_cart_lines(&cart.internal_id);
        self.storefront_cart_save(cart.clone(), lines);
        let warnings = self.storefront_cart_all_warnings(&cart, Vec::new());
        self.storefront_cart_user_error_outcome(
            field,
            self.storefront_cart_json(&cart, &cart_selection(field)),
            Vec::new(),
            warnings,
        )
    }

    fn storefront_cart_gift_card_codes_update(
        &mut self,
        field: &RootFieldSelection,
    ) -> StorefrontCartOutcome {
        let cart_id = resolved_string_field(&field.arguments, "cartId").unwrap_or_default();
        let Some(mut cart) = self.storefront_cart_by_public_id(&cart_id) else {
            return self.storefront_cart_missing_mutation_outcome(field, &cart_id, None);
        };
        let codes = list_string_field(&field.arguments, "giftCardCodes");
        if codes.len() > CART_INPUT_LIMIT {
            return storefront_cart_input_limit_outcome(field, "giftCardCodes", "", codes.len());
        }
        cart.applied_gift_cards.clear();
        self.storefront_cart_add_gift_card_codes(&mut cart, &codes);
        cart.updated_at = self.next_mutation_timestamp();
        let lines = self.storefront_cart_lines(&cart.internal_id);
        self.storefront_cart_save(cart.clone(), lines);
        let warnings = self.storefront_cart_all_warnings(&cart, Vec::new());
        self.storefront_cart_user_error_outcome(
            field,
            self.storefront_cart_json(&cart, &cart_selection(field)),
            Vec::new(),
            warnings,
        )
    }

    fn storefront_cart_add_gift_card_codes(
        &mut self,
        cart: &mut StorefrontCartRecord,
        codes: &[String],
    ) {
        let mut seen = cart
            .applied_gift_cards
            .iter()
            .map(|applied| normalize_gift_card_code(&applied.code))
            .collect::<BTreeSet<_>>();
        for code in codes {
            let normalized = normalize_gift_card_code(code);
            if !seen.insert(normalized.clone()) {
                continue;
            }
            let Some((gift_card_id, _)) = self.storefront_cart_gift_card_by_code(&normalized)
            else {
                continue;
            };
            let sequence = self.store.staged.next_storefront_cart_applied_gift_card_id;
            self.store.staged.next_storefront_cart_applied_gift_card_id += 1;
            cart.applied_gift_cards
                .push(StorefrontCartAppliedGiftCardRecord {
                    sequence,
                    gift_card_id,
                    code: normalized,
                });
        }
    }

    fn storefront_cart_metafields_set(
        &mut self,
        field: &RootFieldSelection,
    ) -> StorefrontCartOutcome {
        const CART_METAFIELD_LIMIT: usize = 25;
        let inputs = resolved_object_list_field(&field.arguments, "metafields");
        let mut user_errors = Vec::new();
        if inputs.len() > CART_METAFIELD_LIMIT {
            user_errors.push(json!({
                "field": ["metafields"],
                "message": "Exceeded the maximum metafields input limit of 25.",
                "code": "TOO_MANY_METAFIELDS",
                "elementIndex": Value::Null
            }));
        }
        let mut prepared = Vec::new();
        for (index, input) in inputs.iter().enumerate() {
            let owner_id = resolved_string_field(input, "ownerId").unwrap_or_default();
            let Some(cart) = self.storefront_cart_by_public_id(&owner_id) else {
                user_errors.push(storefront_cart_metafield_set_error(
                    index,
                    "ownerId",
                    "Owner does not exist.",
                    "INVALID_OWNER",
                ));
                continue;
            };
            let composite_key = resolved_string_field(input, "key").unwrap_or_default();
            let Some((namespace, key)) = storefront_cart_metafield_key(&composite_key) else {
                user_errors.push(storefront_cart_metafield_set_error(
                    index,
                    "key",
                    "Key must include a namespace.",
                    "INVALID_INPUT",
                ));
                continue;
            };
            let metafield_type = resolved_string_field(input, "type").unwrap_or_default();
            let value = resolved_string_field(input, "value").unwrap_or_default();
            let mut reference_exists = |_: &str| true;
            if let Some(message) =
                metafield_value_error_message(&metafield_type, &value, &mut reference_exists)
            {
                user_errors.push(storefront_cart_metafield_set_error(
                    index,
                    "value",
                    &message,
                    "INVALID_VALUE",
                ));
                continue;
            }
            if cart.metafields.iter().any(|existing| {
                existing.namespace == namespace
                    && existing.key == key
                    && existing.metafield_type != metafield_type
            }) {
                user_errors.push(storefront_cart_metafield_set_error(
                    index,
                    "type",
                    "Type must match the existing metafield type.",
                    "TYPE_MISMATCH",
                ));
                continue;
            }
            prepared.push((owner_id, namespace, key, metafield_type, value));
        }
        if !user_errors.is_empty() {
            return storefront_cart_metafields_set_outcome(field, Vec::new(), user_errors);
        }
        let timestamp = self.next_mutation_timestamp();
        let mut changed = Vec::new();
        for (owner_id, namespace, key, metafield_type, value) in prepared {
            let mut cart = self
                .storefront_cart_by_public_id(&owner_id)
                .expect("validated cart metafield owner should remain available");
            if let Some(existing) = cart
                .metafields
                .iter_mut()
                .find(|entry| entry.namespace == namespace && entry.key == key)
            {
                existing.value = value;
                existing.updated_at = timestamp.clone();
                changed.push((cart.sequence, existing.clone()));
            } else {
                let sequence = self.store.staged.next_storefront_cart_metafield_id;
                self.store.staged.next_storefront_cart_metafield_id += 1;
                let metafield = StorefrontCartMetafieldRecord {
                    sequence,
                    namespace,
                    key,
                    value,
                    metafield_type,
                    created_at: timestamp.clone(),
                    updated_at: timestamp.clone(),
                };
                cart.metafields.push(metafield.clone());
                changed.push((cart.sequence, metafield));
            }
            cart.updated_at = timestamp.clone();
            let lines = self.storefront_cart_lines(&cart.internal_id);
            self.storefront_cart_save(cart, lines);
        }
        let values = changed
            .into_iter()
            .map(|(cart_sequence, metafield)| {
                storefront_cart_metafield_json(
                    cart_sequence,
                    &metafield,
                    &metafield_selection(field),
                )
            })
            .collect();
        storefront_cart_metafields_set_outcome(field, values, Vec::new())
    }

    fn storefront_cart_metafield_delete(
        &mut self,
        field: &RootFieldSelection,
    ) -> StorefrontCartOutcome {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let owner_id = resolved_string_field(&input, "ownerId").unwrap_or_default();
        let Some(mut cart) = self.storefront_cart_by_public_id(&owner_id) else {
            return storefront_cart_metafield_delete_outcome(
                field,
                None,
                vec![user_error(
                    ["input", "ownerId"],
                    "Owner does not exist.",
                    Some("INVALID_OWNER"),
                )],
            );
        };
        let composite_key = resolved_string_field(&input, "key").unwrap_or_default();
        let Some((namespace, key)) = storefront_cart_metafield_key(&composite_key) else {
            return storefront_cart_metafield_delete_outcome(
                field,
                None,
                vec![user_error(
                    ["input", "key"],
                    "Metafield does not exist",
                    Some("METAFIELD_DOES_NOT_EXIST"),
                )],
            );
        };
        let Some(position) = cart
            .metafields
            .iter()
            .position(|entry| entry.namespace == namespace && entry.key == key)
        else {
            return storefront_cart_metafield_delete_outcome(
                field,
                None,
                vec![user_error(
                    ["input", "key"],
                    "Metafield does not exist",
                    Some("METAFIELD_DOES_NOT_EXIST"),
                )],
            );
        };
        let metafield = cart.metafields.remove(position);
        cart.updated_at = self.next_mutation_timestamp();
        let lines = self.storefront_cart_lines(&cart.internal_id);
        self.storefront_cart_save(cart.clone(), lines);
        storefront_cart_metafield_delete_outcome(
            field,
            Some(storefront_cart_metafield_id(
                cart.sequence,
                metafield.sequence,
            )),
            Vec::new(),
        )
    }

    fn storefront_cart_missing_mutation_outcome(
        &mut self,
        field: &RootFieldSelection,
        _cart_id: &str,
        note: Option<&str>,
    ) -> StorefrontCartOutcome {
        let cart = note.map(|note| {
            let sequence = self.store.staged.next_storefront_cart_id;
            self.store.staged.next_storefront_cart_id += 1;
            let timestamp = self.next_mutation_timestamp();
            let cart = StorefrontCartRecord {
                internal_id: sequence.to_string(),
                sequence,
                created_at: timestamp.clone(),
                updated_at: timestamp,
                note: Some(note.to_string()),
                attributes: Vec::new(),
                buyer_identity: StorefrontCartBuyerIdentityRecord::default(),
                discount_codes: Vec::new(),
                applied_gift_cards: Vec::new(),
                metafields: Vec::new(),
            };
            self.storefront_cart_save(cart.clone(), Vec::new());
            self.storefront_cart_json(&cart, &cart_selection(field))
        });
        self.storefront_cart_user_error_outcome(
            field,
            cart.unwrap_or(Value::Null),
            vec![cart_user_error(
                ["cartId"],
                "The specified cart does not exist.",
                "INVALID",
            )],
            Vec::new(),
        )
    }

    fn storefront_cart_user_error_outcome(
        &self,
        field: &RootFieldSelection,
        cart: Value,
        user_errors: Vec<Value>,
        warnings: Vec<Value>,
    ) -> StorefrontCartOutcome {
        StorefrontCartOutcome {
            value: selected_payload_json(&field.selection, |selection| {
                match selection.name.as_str() {
                    "cart" => Some(cart.clone()),
                    "userErrors" => Some(Value::Array(
                        user_errors
                            .iter()
                            .map(|error| selected_json(error, &selection.selection))
                            .collect(),
                    )),
                    "warnings" => Some(Value::Array(
                        warnings
                            .iter()
                            .map(|warning| selected_json(warning, &selection.selection))
                            .collect(),
                    )),
                    _ => None,
                }
            }),
            errors: Vec::new(),
        }
    }

    fn storefront_cart_by_public_id(&self, id: &str) -> Option<StorefrontCartRecord> {
        self.store
            .staged
            .storefront_carts
            .values()
            .find(|cart| storefront_cart_public_id(cart.sequence) == id)
            .cloned()
    }

    fn storefront_cart_lines(&self, cart_internal_id: &str) -> Vec<StorefrontCartLineRecord> {
        self.store
            .staged
            .storefront_cart_line_order
            .get(cart_internal_id)
            .into_iter()
            .flatten()
            .filter_map(|id| self.store.staged.storefront_cart_lines.get(id).cloned())
            .collect()
    }

    fn storefront_cart_save(
        &mut self,
        cart: StorefrontCartRecord,
        lines: Vec<StorefrontCartLineRecord>,
    ) {
        let old_ids = self
            .store
            .staged
            .storefront_cart_line_order
            .get(&cart.internal_id)
            .cloned()
            .unwrap_or_default();
        let new_ids = lines
            .iter()
            .map(|line| line.internal_id.clone())
            .collect::<Vec<_>>();
        for id in old_ids {
            if !new_ids.contains(&id) {
                self.store.staged.storefront_cart_lines.remove(&id);
            }
        }
        for line in lines {
            self.store
                .staged
                .storefront_cart_lines
                .insert(line.internal_id.clone(), line);
        }
        self.store
            .staged
            .storefront_cart_line_order
            .insert(cart.internal_id.clone(), new_ids);
        if !self
            .store
            .staged
            .storefront_cart_order
            .contains(&cart.internal_id)
        {
            self.store
                .staged
                .storefront_cart_order
                .push(cart.internal_id.clone());
        }
        self.store
            .staged
            .storefront_carts
            .insert(cart.internal_id.clone(), cart);
    }

    fn storefront_cart_variant(&self, id: &str) -> Option<ProductVariantRecord> {
        let variant = self.store.product_variant_by_id(id)?.clone();
        let product = self.store.product_by_id(&variant.product_id)?;
        self.storefront_product_is_visible(product)
            .then_some(variant)
    }

    fn storefront_cart_line_input_error(
        &self,
        inputs: &[BTreeMap<String, ResolvedValue>],
    ) -> Option<Value> {
        for (index, input) in inputs.iter().enumerate() {
            let merchandise_id = resolved_string_field(input, "merchandiseId").unwrap_or_default();
            if self.storefront_cart_variant(&merchandise_id).is_none() {
                return Some(cart_user_error(
                    ["lines", &index.to_string(), "merchandiseId"],
                    &format!("The merchandise with id {merchandise_id} does not exist."),
                    "INVALID",
                ));
            }
            if let Some(selling_plan_id) = resolved_string_field(input, "sellingPlanId") {
                if self
                    .storefront_cart_selling_plan(&selling_plan_id, &merchandise_id)
                    .is_none()
                {
                    return Some(cart_user_error(
                        ["lines", &index.to_string(), "sellingPlanId"],
                        "Cannot apply selling plan to variant",
                        "SELLING_PLAN_NOT_APPLICABLE",
                    ));
                }
            }
        }
        None
    }

    fn storefront_cart_selling_plan(
        &self,
        selling_plan_id: &str,
        variant_id: &str,
    ) -> Option<SellingPlanRecord> {
        let variant = self.store.product_variant_by_id(variant_id)?;
        self.store
            .staged
            .selling_plan_groups
            .iter()
            .find_map(|(_, group)| {
                let applies = group.product_variant_ids.iter().any(|id| id == variant_id)
                    || group.product_ids.iter().any(|id| id == &variant.product_id);
                applies.then(|| {
                    group
                        .selling_plans
                        .iter()
                        .find(|plan| plan.id == selling_plan_id)
                        .cloned()
                })?
            })
    }

    fn storefront_cart_apply_line_add(
        &mut self,
        cart: &StorefrontCartRecord,
        lines: &mut Vec<StorefrontCartLineRecord>,
        input: &BTreeMap<String, ResolvedValue>,
        warnings: &mut Vec<Value>,
    ) {
        let merchandise_id = resolved_string_field(input, "merchandiseId").unwrap_or_default();
        let requested = resolved_int_field(input, "quantity").unwrap_or(1).max(0);
        if requested == 0 {
            return;
        }
        let attributes =
            storefront_cart_attributes(&resolved_object_list_field(input, "attributes"));
        let selling_plan_id = resolved_string_field(input, "sellingPlanId");
        let existing = lines.iter().position(|line| {
            line.merchandise_id == merchandise_id
                && line.selling_plan_id == selling_plan_id
                && line.attributes == attributes
        });
        let current = lines
            .iter()
            .filter(|line| line.merchandise_id == merchandise_id)
            .map(|line| line.quantity)
            .sum::<i64>();
        let available = self.storefront_cart_available_quantity(&merchandise_id);
        let applied = if available == i64::MAX {
            requested
        } else {
            requested.min((available - current).max(0))
        };
        let target_position = if let Some(position) = existing {
            lines[position].quantity += applied;
            lines[position].out_of_stock_warning = applied == 0 && requested > 0;
            position
        } else {
            let sequence = self.store.staged.next_storefront_cart_line_id;
            self.store.staged.next_storefront_cart_line_id += 1;
            lines.insert(
                0,
                StorefrontCartLineRecord {
                    internal_id: sequence.to_string(),
                    sequence,
                    cart_internal_id: cart.internal_id.clone(),
                    merchandise_id: merchandise_id.clone(),
                    quantity: applied,
                    attributes,
                    selling_plan_id,
                    out_of_stock_warning: applied == 0,
                },
            );
            0
        };
        if applied < requested {
            let target = self.storefront_cart_line_public_id(&lines[target_position]);
            warnings.push(if applied == 0 {
                self.storefront_cart_out_of_stock_warning(&lines[target_position])
            } else {
                storefront_cart_stock_capped_warning(applied, &target)
            });
        }
    }

    fn storefront_cart_apply_line_update_quantity(
        &self,
        _cart: &StorefrontCartRecord,
        lines: &mut [StorefrontCartLineRecord],
        target_position: usize,
        requested: i64,
        warnings: &mut Vec<Value>,
    ) {
        let merchandise_id = lines[target_position].merchandise_id.clone();
        let available = self.storefront_cart_available_quantity(&merchandise_id);
        lines[target_position].quantity = if available == i64::MAX {
            requested.max(0)
        } else {
            requested.max(0).min(available)
        };
        lines[target_position].out_of_stock_warning =
            requested > 0 && lines[target_position].quantity == 0;
        let mut remaining = if available == i64::MAX {
            i64::MAX
        } else {
            (available - lines[target_position].quantity).max(0)
        };
        for (position, line) in lines.iter_mut().enumerate() {
            if position == target_position || line.merchandise_id != merchandise_id {
                continue;
            }
            if remaining == i64::MAX {
                continue;
            }
            let previous = line.quantity;
            line.quantity = previous.min(remaining);
            remaining -= line.quantity;
            if line.quantity < previous {
                let target = self.storefront_cart_line_public_id(line);
                warnings.push(if line.quantity == 0 {
                    line.out_of_stock_warning = true;
                    self.storefront_cart_out_of_stock_warning(line)
                } else {
                    storefront_cart_stock_capped_warning(line.quantity, &target)
                });
            }
        }
        if lines[target_position].quantity < requested.max(0) {
            let target = self.storefront_cart_line_public_id(&lines[target_position]);
            warnings.push(if lines[target_position].quantity == 0 {
                self.storefront_cart_out_of_stock_warning(&lines[target_position])
            } else {
                storefront_cart_stock_capped_warning(lines[target_position].quantity, &target)
            });
        }
    }

    fn storefront_cart_available_quantity(&self, merchandise_id: &str) -> i64 {
        self.storefront_cart_variant(merchandise_id)
            .map(|variant| {
                if !variant.inventory_item.tracked || variant.inventory_policy == "CONTINUE" {
                    i64::MAX
                } else {
                    variant.inventory_quantity.max(0)
                }
            })
            .unwrap_or(0)
    }

    fn storefront_cart_context(&self, cart: &StorefrontCartRecord) -> StorefrontRequestContext {
        StorefrontRequestContext {
            country: cart.buyer_identity.country_code.clone(),
            buyer_company_location_id: cart.buyer_identity.company_location_id.clone(),
            ..StorefrontRequestContext::default()
        }
    }

    fn storefront_cart_gift_card_by_code(&self, code: &str) -> Option<(String, Value)> {
        let normalized = normalize_gift_card_code(code);
        self.store
            .staged
            .gift_cards
            .iter()
            .chain(self.store.base.gift_cards.iter())
            .find_map(|(id, card)| {
                (card
                    .get("giftCardCode")
                    .and_then(Value::as_str)
                    .is_some_and(|candidate| normalize_gift_card_code(candidate) == normalized))
                .then(|| (id.clone(), card.clone()))
            })
            .filter(|(_, card)| {
                !gift_card_is_deactivated(card)
                    && !self.gift_card_is_expired(card)
                    && gift_card_balance_amount(card) > 0.0
            })
    }

    fn storefront_cart_gift_card_by_id(&self, id: &str) -> Option<&Value> {
        self.store
            .staged
            .gift_cards
            .get(id)
            .or_else(|| self.store.base.gift_cards.get(id))
            .filter(|card| {
                !gift_card_is_deactivated(card)
                    && !self.gift_card_is_expired(card)
                    && gift_card_balance_amount(card) > 0.0
            })
    }

    fn storefront_cart_calculation(
        &self,
        cart: &StorefrontCartRecord,
        lines: &[StorefrontCartLineRecord],
    ) -> StorefrontCartCalculation {
        let context = self.storefront_cart_context(cart);
        let currency_code = lines
            .iter()
            .find_map(|line| {
                self.store
                    .product_variant_by_id(&line.merchandise_id)
                    .map(|variant| {
                        self.storefront_variant_pricing(variant, &context)
                            .currency_code
                    })
            })
            .filter(|currency| !currency.is_empty())
            .unwrap_or_else(|| self.storefront_currency_code());
        let subtotal = lines
            .iter()
            .filter_map(|line| {
                let variant = self.store.product_variant_by_id(&line.merchandise_id)?;
                let price = self
                    .storefront_variant_pricing(variant, &context)
                    .price
                    .parse::<f64>()
                    .ok()?;
                Some(price * line.quantity as f64)
            })
            .sum::<f64>();
        let mut line_subtotals = lines
            .iter()
            .filter_map(|line| {
                let variant = self.store.product_variant_by_id(&line.merchandise_id)?;
                let price = self
                    .storefront_variant_pricing(variant, &context)
                    .price
                    .parse::<f64>()
                    .ok()?;
                Some(price * line.quantity as f64)
            })
            .filter(|amount| *amount > 0.0)
            .collect::<Vec<_>>();
        line_subtotals.sort_by(|left, right| left.total_cmp(right));
        let total_quantity = lines.iter().map(|line| line.quantity).sum::<i64>();
        let discounts = cart
            .discount_codes
            .iter()
            .map(|code| {
                self.storefront_cart_discount_evaluation(
                    cart,
                    code,
                    subtotal,
                    total_quantity,
                    &line_subtotals,
                )
            })
            .collect::<Vec<_>>();
        let discount_total = discounts
            .iter()
            .filter(|discount| discount.applicable)
            .flat_map(|discount| discount.discounted_amounts.iter().copied())
            .sum::<f64>()
            .min(subtotal);
        let mut total = (subtotal - discount_total).max(0.0);
        let mut gift_cards = Vec::new();
        for applied in &cart.applied_gift_cards {
            let Some(card) = self.storefront_cart_gift_card_by_id(&applied.gift_card_id) else {
                continue;
            };
            let card_currency = gift_card_currency(card, &currency_code);
            if card_currency != currency_code {
                continue;
            }
            let balance = gift_card_balance_amount(card);
            let amount_used = balance.min(total);
            total = (total - amount_used).max(0.0);
            gift_cards.push(StorefrontCartGiftCardApplication {
                id: storefront_cart_applied_gift_card_id(cart.sequence, applied.sequence),
                last_characters: card
                    .get("lastCharacters")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| gift_card_code_last_characters(&applied.code)),
                amount_used,
                balance: (balance - amount_used).max(0.0),
                currency_code: card_currency,
            });
        }
        StorefrontCartCalculation {
            subtotal,
            total,
            currency_code,
            discounts,
            gift_cards,
        }
    }

    fn storefront_cart_discount_evaluation(
        &self,
        cart: &StorefrontCartRecord,
        code: &str,
        subtotal: f64,
        total_quantity: i64,
        line_subtotals: &[f64],
    ) -> StorefrontCartDiscountEvaluation {
        let target = storefront_cart_warning_target(cart.sequence);
        let Some(record) = self.discount_record_by_code(code) else {
            return StorefrontCartDiscountEvaluation {
                code: code.to_string(),
                applicable: false,
                discounted_amounts: Vec::new(),
                warning: Some(json!({
                    "code": "DISCOUNT_NOT_FOUND",
                    "message": "Enter a valid discount code",
                    "target": target
                })),
            };
        };
        if self.effective_discount_status(record) != "ACTIVE" {
            return StorefrontCartDiscountEvaluation {
                code: code.to_string(),
                applicable: false,
                discounted_amounts: Vec::new(),
                warning: Some(json!({
                    "code": "DISCOUNT_CURRENTLY_INACTIVE",
                    "message": "This discount is not valid anymore",
                    "target": target
                })),
            };
        }
        let subtotal_minimum = record
            .pointer("/minimumRequirement/greaterThanOrEqualToSubtotal/amount")
            .and_then(Value::as_str)
            .and_then(|amount| amount.parse::<f64>().ok());
        let quantity_minimum = record
            .pointer("/minimumRequirement/greaterThanOrEqualToQuantity")
            .and_then(Value::as_i64);
        let context_applies = match record
            .pointer("/context/__typename")
            .and_then(Value::as_str)
        {
            Some("DiscountCustomers") => {
                cart.buyer_identity.customer_id.as_ref().is_some_and(|id| {
                    record
                        .pointer("/context/customers")
                        .and_then(Value::as_array)
                        .is_some_and(|customers| {
                            customers.iter().any(|customer| customer["id"] == *id)
                        })
                })
            }
            Some("DiscountCustomerSegments") => false,
            _ => true,
        };
        if subtotal <= 0.0 {
            return StorefrontCartDiscountEvaluation {
                code: code.to_string(),
                applicable: false,
                discounted_amounts: Vec::new(),
                warning: Some(json!({
                    "code": "DISCOUNT_CODE_NOT_HONOURED",
                    "message": format!("The {code} discount code is not honoured"),
                    "target": target
                })),
            };
        }
        let applicable = context_applies
            && subtotal_minimum.is_none_or(|minimum| subtotal >= minimum)
            && quantity_minimum.is_none_or(|minimum| total_quantity >= minimum);
        if !applicable {
            return StorefrontCartDiscountEvaluation {
                code: code.to_string(),
                applicable: false,
                discounted_amounts: Vec::new(),
                warning: Some(json!({
                    "code": "DISCOUNT_PURCHASE_NOT_IN_RANGE",
                    "message": format!("The {code} discount code is not valid for the items in your cart"),
                    "target": target
                })),
            };
        }
        let discounted_amounts = record
            .pointer("/customerGets/value/percentage")
            .and_then(Value::as_f64)
            .map(|percentage| {
                line_subtotals
                    .iter()
                    .map(|line_subtotal| line_subtotal * percentage)
                    .collect::<Vec<_>>()
            })
            .or_else(|| {
                record
                    .pointer("/customerGets/value/amount/amount")
                    .and_then(Value::as_str)
                    .and_then(|amount| amount.parse::<f64>().ok())
                    .map(|amount| vec![amount.min(subtotal)])
            })
            .unwrap_or_default();
        StorefrontCartDiscountEvaluation {
            code: code.to_string(),
            applicable: true,
            discounted_amounts,
            warning: None,
        }
    }

    fn storefront_cart_all_warnings(
        &self,
        cart: &StorefrontCartRecord,
        mut immediate: Vec<Value>,
    ) -> Vec<Value> {
        for line in self.storefront_cart_lines(&cart.internal_id) {
            if line.out_of_stock_warning {
                immediate.push(self.storefront_cart_out_of_stock_warning(&line));
            }
        }
        let lines = self.storefront_cart_lines(&cart.internal_id);
        let calculation = self.storefront_cart_calculation(cart, &lines);
        let mut discount_warnings = calculation
            .discounts
            .into_iter()
            .filter_map(|discount| discount.warning)
            .collect::<Vec<_>>();
        if calculation.subtotal <= 0.0 {
            discount_warnings.sort_by_key(|warning| match warning["code"].as_str() {
                Some("DISCOUNT_CURRENTLY_INACTIVE") => 0,
                Some("DISCOUNT_NOT_FOUND") => 1,
                Some("DISCOUNT_CODE_NOT_HONOURED") => 2,
                _ => 3,
            });
        }
        immediate.extend(discount_warnings);
        let mut seen = BTreeSet::new();
        immediate.retain(|warning| {
            seen.insert((
                warning["code"].as_str().unwrap_or_default().to_string(),
                warning["target"].as_str().unwrap_or_default().to_string(),
                warning["message"].as_str().unwrap_or_default().to_string(),
            ))
        });
        immediate
    }

    fn storefront_cart_out_of_stock_warning(&self, line: &StorefrontCartLineRecord) -> Value {
        let variant = self.store.product_variant_by_id(&line.merchandise_id);
        let product = variant.and_then(|variant| self.store.product_by_id(&variant.product_id));
        let title = match (product, variant) {
            (Some(product), Some(variant)) => format!("{} - {}", product.title, variant.title),
            _ => line.merchandise_id.clone(),
        };
        json!({
            "code": "MERCHANDISE_OUT_OF_STOCK",
            "message": format!("The product '{title}' is already sold out."),
            "target": self.storefront_cart_line_public_id(line)
        })
    }

    fn storefront_cart_json(
        &self,
        cart: &StorefrontCartRecord,
        selections: &[SelectedField],
    ) -> Value {
        let lines = self.storefront_cart_lines(&cart.internal_id);
        let total_quantity = lines.iter().map(|line| line.quantity).sum::<i64>();
        let calculation = self.storefront_cart_calculation(cart, &lines);
        selected_payload_json(selections, |selection| match selection.name.as_str() {
            "__typename" => Some(json!("Cart")),
            "id" => Some(json!(storefront_cart_public_id(cart.sequence))),
            "createdAt" => Some(json!(cart.created_at)),
            "updatedAt" => Some(json!(cart.updated_at)),
            "checkoutUrl" => Some(json!(self.storefront_cart_checkout_url(cart.sequence))),
            "totalQuantity" => Some(json!(total_quantity)),
            "note" => Some(
                cart.note
                    .as_ref()
                    .map(|note| json!(note))
                    .unwrap_or(Value::Null),
            ),
            "attribute" => Some(
                resolved_string_field(&selection.arguments, "key")
                    .and_then(|key| {
                        cart.attributes
                            .iter()
                            .find(|attribute| attribute.key == key)
                    })
                    .map(|attribute| {
                        storefront_cart_attribute_json(attribute, &selection.selection)
                    })
                    .unwrap_or(Value::Null),
            ),
            "attributes" => Some(Value::Array(
                cart.attributes
                    .iter()
                    .map(|attribute| {
                        storefront_cart_attribute_json(attribute, &selection.selection)
                    })
                    .collect(),
            )),
            "lines" => Some(selected_typed_connection_with_args(
                &lines,
                &selection.arguments,
                &selection.selection,
                |line, selections| self.storefront_cart_line_json(cart, line, selections),
                |line| self.storefront_cart_line_cursor(line),
            )),
            "cost" => Some(storefront_cart_cost_json(
                calculation.subtotal,
                calculation.total,
                &calculation.currency_code,
                &selection.selection,
                true,
            )),
            "estimatedCost" => Some(storefront_cart_cost_json(
                calculation.subtotal,
                calculation.total,
                &calculation.currency_code,
                &selection.selection,
                false,
            )),
            "appliedGiftCards" => Some(Value::Array(
                calculation
                    .gift_cards
                    .iter()
                    .map(|gift_card| {
                        storefront_cart_applied_gift_card_json(gift_card, &selection.selection)
                    })
                    .collect(),
            )),
            "discountAllocations" => Some(Value::Array(
                calculation
                    .discounts
                    .iter()
                    .filter(|discount| discount.applicable)
                    .flat_map(|discount| {
                        discount.discounted_amounts.iter().map(|amount| {
                            storefront_cart_discount_allocation_json(
                                discount,
                                *amount,
                                &calculation.currency_code,
                                &selection.selection,
                            )
                        })
                    })
                    .collect(),
            )),
            "discountCodes" => Some(Value::Array(
                calculation
                    .discounts
                    .iter()
                    .map(|discount| {
                        selected_json(
                            &json!({ "code": discount.code, "applicable": discount.applicable }),
                            &selection.selection,
                        )
                    })
                    .collect(),
            )),
            "buyerIdentity" => {
                Some(self.storefront_cart_buyer_identity_json(cart, &selection.selection))
            }
            "delivery" => Some(selected_json(
                &json!({ "addresses": [], "groups": [] }),
                &selection.selection,
            )),
            "deliveryGroups" => Some(selected_empty_connection_json(&selection.selection)),
            "metafield" => Some(
                storefront_cart_metafield_lookup(cart, selection)
                    .map(|metafield| {
                        storefront_cart_metafield_json(
                            cart.sequence,
                            metafield,
                            &selection.selection,
                        )
                    })
                    .unwrap_or(Value::Null),
            ),
            "metafields" => Some(Value::Array(
                storefront_cart_metafields_lookup(cart, selection)
                    .into_iter()
                    .map(|metafield| {
                        storefront_cart_metafield_json(
                            cart.sequence,
                            metafield,
                            &selection.selection,
                        )
                    })
                    .collect(),
            )),
            _ => None,
        })
    }

    fn storefront_cart_buyer_identity_json(
        &self,
        cart: &StorefrontCartRecord,
        selections: &[SelectedField],
    ) -> Value {
        selected_payload_json(selections, |selection| match selection.name.as_str() {
            "countryCode" => Some(
                cart.buyer_identity
                    .country_code
                    .as_ref()
                    .map(|value| json!(value))
                    .unwrap_or(Value::Null),
            ),
            "email" => Some(
                cart.buyer_identity
                    .email
                    .as_ref()
                    .map(|value| json!(value))
                    .unwrap_or(Value::Null),
            ),
            "phone" => Some(
                cart.buyer_identity
                    .phone
                    .as_ref()
                    .map(|value| json!(value))
                    .unwrap_or(Value::Null),
            ),
            "customer" => Some(
                cart.buyer_identity
                    .customer_id
                    .as_deref()
                    .and_then(|id| self.storefront_customer_by_id(id))
                    .map(|customer| {
                        selected_json(&storefront_customer_json(&customer), &selection.selection)
                    })
                    .unwrap_or(Value::Null),
            ),
            "deliveryAddressPreferences" => Some(Value::Array(
                cart.buyer_identity.delivery_address_preferences.clone(),
            )),
            "preferences" => Some(
                cart.buyer_identity
                    .preferences
                    .as_ref()
                    .map(|value| selected_json(value, &selection.selection))
                    .unwrap_or(Value::Null),
            ),
            "purchasingCompany" => Some(
                cart.buyer_identity
                    .company_location_id
                    .as_deref()
                    .and_then(|id| self.store.staged.b2b_locations.get(id))
                    .map(|location| {
                        selected_json(
                            &json!({
                                "company": location.get("company").cloned().unwrap_or(Value::Null),
                                "location": location
                            }),
                            &selection.selection,
                        )
                    })
                    .unwrap_or(Value::Null),
            ),
            _ => None,
        })
    }

    fn storefront_cart_line_json(
        &self,
        cart: &StorefrontCartRecord,
        line: &StorefrontCartLineRecord,
        selections: &[SelectedField],
    ) -> Value {
        let variant = self.store.product_variant_by_id(&line.merchandise_id);
        let product = variant.and_then(|variant| self.store.product_by_id(&variant.product_id));
        let context = self.storefront_cart_context(cart);
        let pricing = variant.map(|variant| self.storefront_variant_pricing(variant, &context));
        let currency_code = pricing
            .as_ref()
            .map(|pricing| pricing.currency_code.clone())
            .filter(|currency| !currency.is_empty())
            .unwrap_or_else(|| self.storefront_currency_code());
        let line_total = pricing
            .as_ref()
            .and_then(|pricing| pricing.price.parse::<f64>().ok())
            .map(|price| price * line.quantity as f64)
            .unwrap_or(0.0);
        selected_payload_json(selections, |selection| {
            match selection.name.as_str() {
            "__typename" => Some(json!("CartLine")),
            "id" => Some(json!(self.storefront_cart_line_public_id(line))),
            "quantity" => Some(json!(line.quantity)),
            "attribute" => Some(
                resolved_string_field(&selection.arguments, "key")
                    .and_then(|key| line.attributes.iter().find(|attribute| attribute.key == key))
                    .map(|attribute| storefront_cart_attribute_json(attribute, &selection.selection))
                    .unwrap_or(Value::Null),
            ),
            "attributes" => Some(Value::Array(
                line.attributes
                    .iter()
                    .map(|attribute| storefront_cart_attribute_json(attribute, &selection.selection))
                    .collect(),
            )),
            "merchandise" => Some(
                variant
                    .map(|variant| {
                        storefront_product_variant_json(
                            self,
                            variant,
                            product,
                            &context,
                            Some(&currency_code),
                            &selection.selection,
                        )
                    })
                    .unwrap_or(Value::Null),
            ),
            "sellingPlanAllocation" => Some(
                line.selling_plan_id
                    .as_deref()
                    .and_then(|id| self.storefront_cart_selling_plan(id, &line.merchandise_id))
                    .map(|plan| {
                        selected_json(
                            &json!({
                                "sellingPlan": { "id": plan.id, "name": plan.name },
                                "checkoutChargeAmount": storefront_money_value(0.0, &currency_code),
                                "remainingBalanceChargeAmount": storefront_money_value(0.0, &currency_code),
                                "priceAdjustments": []
                            }),
                            &selection.selection,
                        )
                    })
                    .unwrap_or(Value::Null),
            ),
            "cost" => Some(storefront_cart_line_cost_json(
                pricing.as_ref(),
                line_total,
                &currency_code,
                &selection.selection,
                true,
            )),
            "estimatedCost" => Some(storefront_cart_line_cost_json(
                pricing.as_ref(),
                line_total,
                &currency_code,
                &selection.selection,
                false,
            )),
            "discountAllocations" => Some(Value::Array(Vec::new())),
            "instructions" => Some(selected_json(
                &json!({ "deliveryProfile": Value::Null }),
                &selection.selection,
            )),
            "parentRelationship" => Some(Value::Null),
            _ => None,
        }
        })
    }

    fn storefront_cart_checkout_url(&self, sequence: u64) -> String {
        format!(
            "{}/cart/c/{}?key={}",
            self.config.shopify_admin_origin.trim_end_matches('/'),
            storefront_cart_token(sequence),
            storefront_cart_key(sequence)
        )
    }

    fn storefront_cart_line_public_id(&self, line: &StorefrontCartLineRecord) -> String {
        let cart = self
            .store
            .staged
            .storefront_carts
            .get(&line.cart_internal_id);
        let cart_token = cart
            .map(|cart| storefront_cart_token(cart.sequence))
            .unwrap_or_default();
        format!(
            "gid://shopify/CartLine/{}?cart={cart_token}",
            storefront_cart_line_token(line.sequence)
        )
    }

    fn storefront_cart_line_cursor(&self, line: &StorefrontCartLineRecord) -> String {
        format!(
            "storefront-cart-line-cursor-{}-{}",
            line.sequence,
            &storefront_sha256_hex(&self.storefront_cart_line_public_id(line))[..16]
        )
    }
}

fn cart_selection(field: &RootFieldSelection) -> Vec<SelectedField> {
    field
        .selection
        .iter()
        .find(|selection| selection.name == "cart")
        .map(|selection| selection.selection.clone())
        .unwrap_or_default()
}

fn metafield_selection(field: &RootFieldSelection) -> Vec<SelectedField> {
    field
        .selection
        .iter()
        .find(|selection| selection.name == "metafields")
        .map(|selection| selection.selection.clone())
        .unwrap_or_default()
}

fn storefront_cart_normalized_code(code: &str) -> String {
    code.trim().to_ascii_uppercase()
}

fn storefront_cart_warning_target(cart_sequence: u64) -> String {
    format!(
        "gid://shopify/Cart/{}",
        storefront_cart_token(cart_sequence)
    )
}

fn storefront_cart_applied_gift_card_id(cart_sequence: u64, sequence: u64) -> String {
    synthetic_shopify_gid(
        "AppliedGiftCard",
        format!("cart-{cart_sequence}-{sequence}"),
    )
}

fn storefront_cart_metafield_id(cart_sequence: u64, sequence: u64) -> String {
    synthetic_shopify_gid("Metafield", format!("cart-{cart_sequence}-{sequence}"))
}

fn storefront_cart_metafield_key(composite: &str) -> Option<(String, String)> {
    let (namespace, key) = composite.split_once('.')?;
    (!namespace.is_empty() && !key.is_empty()).then(|| (namespace.to_string(), key.to_string()))
}

fn storefront_cart_metafield_set_error(
    index: usize,
    field: &str,
    message: &str,
    code: &str,
) -> Value {
    json!({
        "field": ["metafields", index.to_string(), field],
        "message": message,
        "code": code,
        "elementIndex": Value::Null
    })
}

fn storefront_cart_metafields_set_outcome(
    field: &RootFieldSelection,
    metafields: Vec<Value>,
    user_errors: Vec<Value>,
) -> StorefrontCartOutcome {
    StorefrontCartOutcome {
        value: selected_payload_json(&field.selection, |selection| {
            match selection.name.as_str() {
                "metafields" => Some(Value::Array(metafields.clone())),
                "userErrors" => Some(Value::Array(
                    user_errors
                        .iter()
                        .map(|error| selected_json(error, &selection.selection))
                        .collect(),
                )),
                _ => None,
            }
        }),
        errors: Vec::new(),
    }
}

fn storefront_cart_metafield_delete_outcome(
    field: &RootFieldSelection,
    deleted_id: Option<String>,
    user_errors: Vec<Value>,
) -> StorefrontCartOutcome {
    StorefrontCartOutcome {
        value: selected_payload_json(&field.selection, |selection| {
            match selection.name.as_str() {
                "deletedId" => Some(
                    deleted_id
                        .as_ref()
                        .map(|id| json!(id))
                        .unwrap_or(Value::Null),
                ),
                "userErrors" => Some(Value::Array(
                    user_errors
                        .iter()
                        .map(|error| selected_json(error, &selection.selection))
                        .collect(),
                )),
                _ => None,
            }
        }),
        errors: Vec::new(),
    }
}

fn storefront_cart_metafield_json(
    cart_sequence: u64,
    metafield: &StorefrontCartMetafieldRecord,
    selections: &[SelectedField],
) -> Value {
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "__typename" => Some(json!("Metafield")),
        "id" => Some(json!(storefront_cart_metafield_id(
            cart_sequence,
            metafield.sequence
        ))),
        "namespace" => Some(json!(metafield.namespace)),
        "key" => Some(json!(metafield.key)),
        "value" => Some(json!(metafield.value)),
        "type" => Some(json!(metafield.metafield_type)),
        "list" => Some(json!(metafield.metafield_type.starts_with("list."))),
        "createdAt" => Some(json!(metafield.created_at)),
        "updatedAt" => Some(json!(metafield.updated_at)),
        "description" | "reference" | "references" => Some(Value::Null),
        "parentResource" => Some(selected_payload_json(
            &selection.selection,
            |parent_selection| match parent_selection.name.as_str() {
                "__typename" => Some(json!("Cart")),
                "id" => Some(json!(storefront_cart_public_id(cart_sequence))),
                _ => None,
            },
        )),
        _ => None,
    })
}

fn storefront_cart_metafield_lookup<'a>(
    cart: &'a StorefrontCartRecord,
    selection: &SelectedField,
) -> Option<&'a StorefrontCartMetafieldRecord> {
    let namespace = resolved_string_field(&selection.arguments, "namespace")?;
    let key = resolved_string_field(&selection.arguments, "key")?;
    cart.metafields
        .iter()
        .find(|metafield| metafield.namespace == namespace && metafield.key == key)
}

fn storefront_cart_metafields_lookup<'a>(
    cart: &'a StorefrontCartRecord,
    selection: &SelectedField,
) -> Vec<&'a StorefrontCartMetafieldRecord> {
    resolved_object_list_field(&selection.arguments, "identifiers")
        .into_iter()
        .filter_map(|identifier| {
            let namespace = resolved_string_field(&identifier, "namespace")?;
            let key = resolved_string_field(&identifier, "key")?;
            cart.metafields
                .iter()
                .find(|metafield| metafield.namespace == namespace && metafield.key == key)
        })
        .collect()
}

fn storefront_cart_applied_gift_card_json(
    gift_card: &StorefrontCartGiftCardApplication,
    selections: &[SelectedField],
) -> Value {
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "__typename" => Some(json!("AppliedGiftCard")),
        "id" => Some(json!(gift_card.id)),
        "lastCharacters" => Some(json!(gift_card.last_characters)),
        "amountUsed" | "amountUsedV2" | "presentmentAmountUsed" => Some(selected_json(
            &storefront_money_value(gift_card.amount_used, &gift_card.currency_code),
            &selection.selection,
        )),
        "balance" | "balanceV2" => Some(selected_json(
            &storefront_money_value(gift_card.balance, &gift_card.currency_code),
            &selection.selection,
        )),
        _ => None,
    })
}

fn storefront_cart_discount_allocation_json(
    discount: &StorefrontCartDiscountEvaluation,
    discounted_amount: f64,
    currency_code: &str,
    selections: &[SelectedField],
) -> Value {
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "__typename" => Some(json!("CartCodeDiscountAllocation")),
        "code" => Some(json!(discount.code)),
        "discountedAmount" => Some(selected_json(
            &storefront_money_value(discounted_amount, currency_code),
            &selection.selection,
        )),
        "targetType" => Some(json!("LINE_ITEM")),
        "discountApplication" => Some(selected_json(
            &json!({
                "allocationMethod": "ACROSS",
                "targetSelection": "ALL",
                "targetType": "LINE_ITEM",
                "value": {
                    "__typename": "PricingPercentageValue",
                    "percentage": 0.0
                }
            }),
            &selection.selection,
        )),
        _ => None,
    })
}

fn storefront_cart_attributes(
    inputs: &[BTreeMap<String, ResolvedValue>],
) -> Vec<StorefrontCartAttributeRecord> {
    let mut attributes = Vec::<StorefrontCartAttributeRecord>::new();
    for input in inputs {
        let key = resolved_string_field(input, "key").unwrap_or_default();
        let value = resolved_string_field(input, "value").unwrap_or_default();
        if let Some(existing) = attributes.iter_mut().find(|attribute| attribute.key == key) {
            existing.value = value;
        } else {
            attributes.push(StorefrontCartAttributeRecord { key, value });
        }
    }
    attributes
}

fn storefront_cart_merge_duplicate_lines(lines: &mut Vec<StorefrontCartLineRecord>) {
    let mut index = 0;
    while index < lines.len() {
        let mut other = index + 1;
        while other < lines.len() {
            if lines[index].merchandise_id == lines[other].merchandise_id
                && lines[index].selling_plan_id == lines[other].selling_plan_id
                && lines[index].attributes == lines[other].attributes
            {
                lines[index].quantity += lines[other].quantity;
                lines[index].out_of_stock_warning |= lines[other].out_of_stock_warning;
                lines.remove(other);
            } else {
                other += 1;
            }
        }
        index += 1;
    }
}

fn storefront_cart_attribute_json(
    attribute: &StorefrontCartAttributeRecord,
    selections: &[SelectedField],
) -> Value {
    selected_json(
        &json!({ "key": attribute.key, "value": attribute.value }),
        selections,
    )
}

fn storefront_cart_public_id(sequence: u64) -> String {
    format!(
        "gid://shopify/Cart/{}?key={}",
        storefront_cart_token(sequence),
        storefront_cart_key(sequence)
    )
}

fn storefront_cart_token(sequence: u64) -> String {
    format!(
        "sdp_cart_{sequence}_{}",
        &storefront_sha256_hex(&format!("storefront-cart-token:{sequence}"))[..24]
    )
}

fn storefront_cart_key(sequence: u64) -> String {
    format!(
        "sdp_key_{}",
        &storefront_sha256_hex(&format!("storefront-cart-key:{sequence}"))[..24]
    )
}

fn storefront_cart_line_token(sequence: u64) -> String {
    format!(
        "sdp_line_{sequence}_{}",
        &storefront_sha256_hex(&format!("storefront-cart-line:{sequence}"))[..16]
    )
}

fn cart_user_error<const N: usize>(field: [&str; N], message: &str, code: &str) -> Value {
    user_error(field, message, Some(code))
}

fn storefront_cart_stock_capped_warning(quantity: i64, target: &str) -> Value {
    let noun = if quantity == 1 {
        "item was"
    } else {
        "items were"
    };
    json!({
        "code": "MERCHANDISE_NOT_ENOUGH_STOCK",
        "message": format!("Only {quantity} {noun} added to your cart due to availability."),
        "target": target
    })
}

fn storefront_cart_input_limit_outcome(
    field: &RootFieldSelection,
    first_path: &str,
    second_path: &str,
    size: usize,
) -> StorefrontCartOutcome {
    let mut path = vec![json!(field.response_key), json!(first_path)];
    if !second_path.is_empty() {
        path.push(json!(second_path));
    }
    StorefrontCartOutcome {
        value: Value::Null,
        errors: vec![json!({
            "message": format!("The input array size of {size} is greater than the maximum allowed of 250."),
            "locations": [{ "line": field.location.line, "column": field.location.column }],
            "path": path,
            "extensions": { "code": "MAX_INPUT_SIZE_EXCEEDED" }
        })],
    }
}

fn storefront_money_value(amount: f64, currency_code: &str) -> Value {
    json!({
        "amount": format_money_amount(amount),
        "currencyCode": currency_code
    })
}

fn storefront_cart_cost_json(
    subtotal: f64,
    total: f64,
    currency_code: &str,
    selections: &[SelectedField],
    current: bool,
) -> Value {
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "checkoutChargeAmount" | "subtotalAmount" => Some(selected_json(
            &storefront_money_value(subtotal, currency_code),
            &selection.selection,
        )),
        "totalAmount" => Some(selected_json(
            &storefront_money_value(total, currency_code),
            &selection.selection,
        )),
        "totalDutyAmount" | "totalTaxAmount" => Some(Value::Null),
        "subtotalAmountEstimated"
        | "totalAmountEstimated"
        | "totalDutyAmountEstimated"
        | "totalTaxAmountEstimated"
            if current =>
        {
            Some(json!(true))
        }
        _ => None,
    })
}

fn storefront_cart_line_cost_json(
    pricing: Option<&StorefrontVariantPricing>,
    total: f64,
    currency_code: &str,
    selections: &[SelectedField],
    current: bool,
) -> Value {
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "amountPerQuantity" if current => pricing.map(|pricing| {
            storefront_money_json(&pricing.price, currency_code, &selection.selection)
        }),
        "compareAtAmountPerQuantity" if current => Some(
            pricing
                .and_then(|pricing| pricing.compare_at_price.as_deref())
                .map(|amount| storefront_money_json(amount, currency_code, &selection.selection))
                .unwrap_or(Value::Null),
        ),
        "amount" if !current => pricing.map(|pricing| {
            storefront_money_json(&pricing.price, currency_code, &selection.selection)
        }),
        "compareAtAmount" if !current => Some(
            pricing
                .and_then(|pricing| pricing.compare_at_price.as_deref())
                .map(|amount| storefront_money_json(amount, currency_code, &selection.selection))
                .unwrap_or(Value::Null),
        ),
        "subtotalAmount" | "totalAmount" => Some(selected_json(
            &storefront_money_value(total, currency_code),
            &selection.selection,
        )),
        _ => None,
    })
}

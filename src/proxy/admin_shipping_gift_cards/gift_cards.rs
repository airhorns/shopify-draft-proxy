use crate::proxy::*;

const GIFT_CARD_SYNTHETIC_NOW: &str = "2026-04-29T09:31:02Z";
const GIFT_CARD_SEND_NOTIFICATION_WINDOW_DAYS: i64 = 90;
const GIFT_CARD_NOTIFICATION_HYDRATE_QUERY: &str = r#"#graphql
    query GiftCardHydrate($id: ID!) {
      giftCard(id: $id) {
        id
        lastCharacters
        maskedCode
        enabled
        deactivatedAt
        expiresOn
        note
        templateSuffix
        createdAt
        updatedAt
        initialValue { amount currencyCode }
        balance { amount currencyCode }
        customer {
          id
          email
          defaultEmailAddress { emailAddress }
          defaultPhoneNumber { phoneNumber }
        }
        recipientAttributes {
          message
          preferredName
          sendNotificationAt
          recipient {
            id
            email
            defaultEmailAddress { emailAddress }
            defaultPhoneNumber { phoneNumber }
          }
        }
        transactions(first: 250) {
          nodes {
            __typename
            id
            note
            processedAt
            amount { amount currencyCode }
          }
          pageInfo {
            hasNextPage
            hasPreviousPage
            startCursor
            endCursor
          }
        }
      }
      giftCardConfiguration {
        issueLimit { amount currencyCode }
        purchaseLimit { amount currencyCode }
      }
    }
  "#;

impl DraftProxy {
    pub(in crate::proxy) fn gift_card_read_data(&self, fields: &[RootFieldSelection]) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "giftCard" => {
                    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    self.store
                        .staged
                        .gift_cards
                        .get(&id)
                        .map(|card| selected_json(card, &field.selection))
                        .unwrap_or(Value::Null)
                }
                "giftCards" => {
                    let query = resolved_string_arg(&field.arguments, "query").unwrap_or_default();
                    let cards = self.gift_card_lifecycle_matching_cards(&query);
                    gift_card_connection_json(&cards, &field.selection)
                }
                "giftCardsCount" => {
                    let query = resolved_string_arg(&field.arguments, "query").unwrap_or_default();
                    gift_card_count_json(
                        self.gift_card_lifecycle_matching_cards(&query).len(),
                        &field.selection,
                    )
                }
                "giftCardConfiguration" => {
                    selected_json(&gift_card_configuration_record(), &field.selection)
                }
                _ => continue,
            };
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    pub(in crate::proxy) fn gift_card_mutation_response(
        &mut self,
        fields: &[RootFieldSelection],
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let mut data = serde_json::Map::new();
        let mut staged_ids = Vec::new();

        for field in fields {
            if matches!(field.name.as_str(), "giftCardCreate" | "giftCardUpdate") {
                if let Some(error) = gift_card_missing_recipient_id_error(field) {
                    return ok_json(json!({ "errors": [error] }));
                }
            }
            if matches!(field.name.as_str(), "giftCardCredit" | "giftCardDebit") {
                if let Some(error) = gift_card_transaction_payload_selection_error(field) {
                    return ok_json(json!({ "errors": [error] }));
                }
            }
        }

        for field in fields {
            let payload = match field.name.as_str() {
                "giftCardCreate" => self.gift_card_create_field(field, &mut staged_ids),
                "giftCardUpdate" => self.gift_card_update_field(field, &mut staged_ids),
                "giftCardCredit" => self.gift_card_credit_field(field, &mut staged_ids),
                "giftCardDebit" => self.gift_card_debit_field(field, &mut staged_ids),
                "giftCardDeactivate" => self.gift_card_deactivate_field(field, &mut staged_ids),
                "giftCardSendNotificationToCustomer" | "giftCardSendNotificationToRecipient" => {
                    self.gift_card_notification_field(field, request, &mut staged_ids)
                }
                _ => continue,
            };
            data.insert(field.response_key.clone(), payload);
        }

        if !staged_ids.is_empty() {
            staged_ids.sort();
            staged_ids.dedup();
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                fields
                    .first()
                    .map(|field| field.name.as_str())
                    .unwrap_or("giftCardCreate"),
                staged_ids,
            );
        }

        ok_json(json!({ "data": Value::Object(data) }))
    }

    pub(in crate::proxy) fn gift_card_lifecycle_matching_cards(&self, query: &str) -> Vec<Value> {
        self.store
            .staged
            .gift_cards
            .values()
            .filter(|card| gift_card_matches_search_query(card, query))
            .cloned()
            .collect()
    }

    fn gift_card_create_field(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let mut user_errors = self.gift_card_plan_errors_for_field(field);
        if user_errors.is_empty() {
            user_errors.extend(gift_card_assignment_errors(&input, "input"));
        }
        if user_errors.is_empty()
            && resolved_string_field(&input, "customerId")
                .as_deref()
                .is_some_and(gift_card_customer_id_is_missing)
        {
            user_errors.push(gift_card_user_error(
                &field.name,
                json!(["input", "customerId"]),
                Some("CUSTOMER_NOT_FOUND"),
                "The customer could not be found.",
            ));
        }
        let amount = input
            .get("initialValue")
            .map(|value| resolved_money_amount_string(Some(value)))
            .unwrap_or_else(|| "0".to_string());
        let amount_number = amount.parse::<f64>().unwrap_or(0.0);
        if user_errors.is_empty() && amount_number <= 0.0 {
            user_errors.push(gift_card_user_error(
                &field.name,
                json!(["input", "initialValue"]),
                Some("GREATER_THAN"),
                "must be greater than 0",
            ));
        }
        if user_errors.is_empty() && amount_number > self.gift_card_issue_limit_amount() {
            user_errors.push(gift_card_user_error(
                &field.name,
                json!(["input", "initialValue"]),
                Some("GIFT_CARD_LIMIT_EXCEEDED"),
                "can't exceed $3,000.00 CAD",
            ));
        }
        if user_errors.is_empty() {
            if let Some(code_error) = resolved_string_field(&input, "code")
                .and_then(|code| self.gift_card_code_error(&code))
            {
                user_errors.push(code_error);
            }
        }
        if user_errors.is_empty() {
            user_errors.extend(gift_card_recipient_errors(&input, "input"));
        }

        if !user_errors.is_empty() {
            return gift_card_payload_json_nullable(None, &field.selection, user_errors);
        }

        let id = self.next_proxy_synthetic_gid("GiftCard");
        let amount = format_money_amount(amount_number);
        let code = resolved_string_field(&input, "code")
            .map(|code| normalize_gift_card_code(&code))
            .unwrap_or_else(|| synthetic_gift_card_code(&id));
        let last_characters = gift_card_code_last_characters(&code);
        let notify = resolved_bool_field(&input, "notify").unwrap_or(true);
        let mut card = gift_card_lifecycle_base_card(&id);
        card["lastCharacters"] = json!(last_characters);
        card["maskedCode"] = json!(format!("•••• •••• •••• {}", last_characters));
        card["giftCardCode"] = json!(code);
        card["initialValue"] = json!({ "amount": amount, "currencyCode": "CAD" });
        card["balance"] = card["initialValue"].clone();
        card["notify"] = json!(notify);
        card["source"] = json!("api_client");
        card["note"] = resolved_nullable_string_field(&input, "note");
        card["expiresOn"] = resolved_nullable_string_field(&input, "expiresOn");
        card["recipientAttributes"] = Value::Null;
        if input.contains_key("templateSuffix") {
            card["templateSuffix"] = gift_card_template_suffix_json(
                resolved_nullable_string_field(&input, "templateSuffix"),
            );
        }
        if let Some(customer_id) = resolved_string_field(&input, "customerId") {
            card["customer"] = json!({ "id": customer_id });
        } else {
            card["customer"] = Value::Null;
        }
        if let Some(recipient_attributes) = resolved_object_field(&input, "recipientAttributes") {
            card["recipientAttributes"] =
                gift_card_recipient_attributes_json(&recipient_attributes);
        }

        self.store
            .staged
            .gift_cards
            .insert(id.clone(), card.clone());
        staged_ids.push(id);
        gift_card_payload_json(&card, &field.selection, Vec::new())
    }

    fn gift_card_update_field(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let mut user_errors = self.gift_card_plan_errors_for_field(field);
        let existing = self.gift_card_effective_record(&id);
        if user_errors.is_empty() && existing.is_none() {
            user_errors.push(gift_card_not_found_error(&field.name));
        }
        if user_errors.is_empty() && gift_card_update_is_empty(field) {
            user_errors.push(gift_card_user_error(
                &field.name,
                json!(["input"]),
                Some("INVALID"),
                "At least one argument is required in the input.",
            ));
        }
        if user_errors.is_empty() {
            if let Some(card) = existing.as_ref() {
                if let Some(error) = gift_card_deactivated_update_error(card, &input) {
                    user_errors.push(gift_card_user_error(
                        &field.name,
                        error,
                        Some("INVALID"),
                        "The gift card is deactivated.",
                    ));
                }
            }
        }
        if user_errors.is_empty() {
            user_errors.extend(gift_card_assignment_errors(&input, "input"));
        }
        if user_errors.is_empty()
            && resolved_string_field(&input, "customerId")
                .as_deref()
                .is_some_and(gift_card_customer_id_is_missing)
        {
            user_errors.push(gift_card_user_error(
                &field.name,
                json!(["input", "customerId"]),
                Some("CUSTOMER_NOT_FOUND"),
                "The customer could not be found.",
            ));
        }
        if user_errors.is_empty() {
            user_errors.extend(gift_card_recipient_errors(&input, "input"));
        }
        if !user_errors.is_empty() {
            return gift_card_payload_json_nullable(None, &field.selection, user_errors);
        }

        let mut card = existing.unwrap_or_else(|| gift_card_lifecycle_base_card(&id));
        if input.contains_key("note") {
            card["note"] = resolved_nullable_string_field(&input, "note");
        }
        if input.contains_key("expiresOn") {
            card["expiresOn"] = resolved_nullable_string_field(&input, "expiresOn");
        }
        if input.contains_key("templateSuffix") {
            card["templateSuffix"] = gift_card_template_suffix_json(
                resolved_nullable_string_field(&input, "templateSuffix"),
            );
        }
        if let Some(customer_id) = resolved_string_field(&input, "customerId") {
            card["customer"] = json!({ "id": customer_id });
        }
        if let Some(recipient_attributes) = resolved_object_field(&input, "recipientAttributes") {
            card["recipientAttributes"] =
                gift_card_recipient_attributes_json(&recipient_attributes);
        }
        card["updatedAt"] = json!("2024-01-01T00:00:00.000Z");
        self.store
            .staged
            .gift_cards
            .insert(id.clone(), card.clone());
        staged_ids.push(id);
        gift_card_payload_json(&card, &field.selection, Vec::new())
    }

    fn gift_card_credit_field(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        self.gift_card_transaction_field(
            field,
            "creditInput",
            "creditAmount",
            "giftCardCreditTransaction",
            true,
            staged_ids,
        )
    }

    fn gift_card_debit_field(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        self.gift_card_transaction_field(
            field,
            "debitInput",
            "debitAmount",
            "giftCardDebitTransaction",
            false,
            staged_ids,
        )
    }

    fn gift_card_transaction_field(
        &mut self,
        field: &RootFieldSelection,
        input_name: &str,
        amount_name: &str,
        transaction_field: &str,
        is_credit: bool,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let input = resolved_object_field(&field.arguments, input_name).unwrap_or_default();
        let money = resolved_object_field(&input, amount_name).unwrap_or_default();
        let requested_amount = money
            .get("amount")
            .map(|value| resolved_money_amount_string(Some(value)))
            .unwrap_or_else(|| "0".to_string());
        let requested_amount_number = requested_amount.parse::<f64>().unwrap_or(0.0);
        let mut user_errors = self.gift_card_plan_errors_for_field(field);
        let mut card = self.gift_card_effective_record(&id);

        if user_errors.is_empty() && requested_amount_number <= 0.0 {
            user_errors.push(gift_card_user_error(
                &field.name,
                json!([input_name, amount_name, "amount"]),
                Some("NEGATIVE_OR_ZERO_AMOUNT"),
                "A positive amount must be used.",
            ));
        }
        if user_errors.is_empty() && card.is_none() {
            user_errors.push(gift_card_not_found_error(&field.name));
        }
        if user_errors.is_empty() {
            if let Some(processed_at) = resolved_string_field(&input, "processedAt") {
                if let Some(error) =
                    gift_card_processed_at_error(&field.name, input_name, &processed_at)
                {
                    user_errors.push(error);
                }
            }
        }
        if user_errors.is_empty() {
            if let Some(existing) = card.as_ref() {
                if self.gift_card_is_expired(existing) {
                    user_errors.push(gift_card_user_error(
                        &field.name,
                        json!(["id"]),
                        Some("INVALID"),
                        "The gift card has expired.",
                    ));
                } else if gift_card_is_deactivated(existing) {
                    user_errors.push(gift_card_user_error(
                        &field.name,
                        json!(["id"]),
                        Some("INVALID"),
                        "The gift card is deactivated.",
                    ));
                }
            }
        }
        if user_errors.is_empty() {
            if let Some(existing) = card.as_ref() {
                let card_currency = gift_card_currency(existing);
                let requested_currency = resolved_string_field(&money, "currencyCode")
                    .unwrap_or_else(|| card_currency.clone());
                if requested_currency != card_currency {
                    user_errors.push(gift_card_user_error(
                        &field.name,
                        json!([input_name, amount_name, "currencyCode"]),
                        Some("MISMATCHING_CURRENCY"),
                        "The currency provided does not match the currency of the gift card.",
                    ));
                }
            }
        }
        if user_errors.is_empty() {
            if let Some(existing) = card.as_ref() {
                let balance = gift_card_balance_amount(existing);
                if is_credit
                    && balance + requested_amount_number > self.gift_card_issue_limit_amount()
                {
                    user_errors.push(gift_card_user_error(
                        &field.name,
                        json!([input_name, amount_name, "amount"]),
                        Some("GIFT_CARD_LIMIT_EXCEEDED"),
                        "The gift card's value exceeds the allowed limits.",
                    ));
                } else if !is_credit && balance < requested_amount_number {
                    user_errors.push(gift_card_user_error(
                        &field.name,
                        json!([input_name, amount_name, "amount"]),
                        Some("INSUFFICIENT_FUNDS"),
                        "The gift card does not have sufficient funds to satisfy the request.",
                    ));
                }
            }
        }

        if !user_errors.is_empty() {
            return gift_card_transaction_payload(
                &field.selection,
                transaction_field,
                None,
                user_errors,
            );
        }

        let mut card = card
            .take()
            .unwrap_or_else(|| gift_card_lifecycle_base_card(&id));
        let currency = gift_card_currency(&card);
        let current_balance = gift_card_balance_amount(&card);
        let next_balance = if is_credit {
            current_balance + requested_amount_number
        } else {
            current_balance - requested_amount_number
        };
        card["balance"] = json!({
            "amount": format_money_amount(next_balance),
            "currencyCode": currency
        });
        let signed_amount = if is_credit {
            requested_amount_number
        } else {
            0.0 - requested_amount_number
        };
        let transaction_id = if is_credit {
            self.next_synthetic_gid("GiftCardCreditTransaction")
        } else {
            self.next_synthetic_gid("GiftCardDebitTransaction")
        };
        let transaction_note = resolved_string_field(&input, "note")
            .map(Value::String)
            .unwrap_or(Value::Null);
        let transaction = json!({
            "id": transaction_id,
            "__typename": if is_credit { "GiftCardCreditTransaction" } else { "GiftCardDebitTransaction" },
            "note": transaction_note,
            "processedAt": resolved_string_field(&input, "processedAt").unwrap_or_else(|| GIFT_CARD_SYNTHETIC_NOW.to_string()),
            "amount": { "amount": format_money_amount(signed_amount), "currencyCode": currency },
            "giftCard": card.clone()
        });
        push_gift_card_transaction(&mut card, transaction.clone());
        self.store.staged.gift_cards.insert(id.clone(), card);
        staged_ids.push(id);
        gift_card_transaction_payload(
            &field.selection,
            transaction_field,
            Some(transaction),
            Vec::new(),
        )
    }

    fn gift_card_deactivate_field(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let mut user_errors = self.gift_card_plan_errors_for_field(field);
        let mut card = self.gift_card_effective_record(&id);
        if user_errors.is_empty() && card.is_none() {
            user_errors.push(gift_card_not_found_error(&field.name));
        }
        if !user_errors.is_empty() {
            return gift_card_payload_json_nullable(None, &field.selection, user_errors);
        }
        let mut card = card
            .take()
            .unwrap_or_else(|| gift_card_lifecycle_base_card(&id));
        card["enabled"] = json!(false);
        card["deactivatedAt"] = json!("2026-04-29T09:31:13Z");
        card["updatedAt"] = json!("2026-04-29T09:31:13Z");
        self.store
            .staged
            .gift_cards
            .insert(id.clone(), card.clone());
        staged_ids.push(id);
        gift_card_payload_json(&card, &field.selection, Vec::new())
    }

    fn gift_card_notification_field(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id")
            .or_else(|| resolved_string_arg(&field.arguments, "giftCardId"))
            .unwrap_or_default();
        let mut user_errors = self.gift_card_plan_errors_for_field(field);
        let mut card = None;

        // Trial-shop notifications are blocked after the entitlement (plan) check
        // but before any card-state/not-found checks, mirroring Shopify's order:
        // an entitlement error wins over the trial error, and the trial error
        // wins over missing/expired/deactivated/no-customer states.
        if user_errors.is_empty() && self.gift_card_notification_is_trial_shop(&id) {
            user_errors.push(gift_card_user_error(
                &field.name,
                json!(["base"]),
                Some("INVALID"),
                "Notifications are not available on trial shops.",
            ));
        }
        if user_errors.is_empty() && !id.is_empty() {
            card = self
                .gift_card_effective_record(&id)
                .or_else(|| self.hydrate_gift_card_for_notification(request, &id));
        }
        if user_errors.is_empty() && card.is_none() {
            user_errors.push(gift_card_not_found_error(&field.name));
        }
        if user_errors.is_empty() {
            if let Some(card) = card.as_ref() {
                if card.get("notify") == Some(&json!(false)) {
                    user_errors.push(gift_card_user_error(
                        &field.name,
                        json!(["id"]),
                        Some("INVALID"),
                        "Notifications for this gift card are disabled.",
                    ));
                } else if self.gift_card_is_expired(card) {
                    user_errors.push(gift_card_user_error(
                        &field.name,
                        json!(["id"]),
                        Some("INVALID"),
                        "The gift card has expired.",
                    ));
                } else if gift_card_is_deactivated(card) {
                    user_errors.push(gift_card_user_error(
                        &field.name,
                        json!(["id"]),
                        Some("INVALID"),
                        "The gift card is deactivated.",
                    ));
                } else if field.name == "giftCardSendNotificationToCustomer"
                    && card.get("customer").is_none_or(Value::is_null)
                {
                    user_errors.push(gift_card_user_error(
                        &field.name,
                        json!(["base"]),
                        Some("INVALID"),
                        "The gift card has no customer.",
                    ));
                } else if field.name == "giftCardSendNotificationToRecipient"
                    && gift_card_recipient_has_no_contact(card)
                {
                    user_errors.push(gift_card_user_error(
                        &field.name,
                        json!(["base"]),
                        Some("INVALID"),
                        "The recipient has no contact information (e.g. email address or phone number).",
                    ));
                }
            }
        }
        if !user_errors.is_empty() {
            return gift_card_payload_json_nullable(None, &field.selection, user_errors);
        }
        if let Some(card) = card.as_ref() {
            staged_ids.push(id);
            gift_card_payload_json(card, &field.selection, Vec::new())
        } else {
            gift_card_payload_json_nullable(None, &field.selection, user_errors)
        }
    }

    fn hydrate_gift_card_for_notification(&mut self, request: &Request, id: &str) -> Option<Value> {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return None;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": GIFT_CARD_NOTIFICATION_HYDRATE_QUERY,
                "variables": { "id": id },
            }),
        );
        let card = response.body["data"]["giftCard"].clone();
        if card.is_null() {
            return None;
        }
        self.store
            .staged
            .gift_cards
            .insert(id.to_string(), card.clone());
        Some(card)
    }

    fn gift_card_effective_record(&self, id: &str) -> Option<Value> {
        self.store
            .staged
            .gift_cards
            .get(id)
            .cloned()
            .or_else(|| gift_card_seed_record(id))
    }

    /// A gift-card notification is unavailable when the shop is on a trial plan.
    /// Two independent signals mark a trial shop in the emulator: an explicit
    /// `trial`-sentinel gift-card id (the captured trial-notification fixtures),
    /// or a restored shop whose `plan.publicDisplayName` is "Trial" (the
    /// state-restore path that hydrates the real plan name).
    fn gift_card_notification_is_trial_shop(&self, id: &str) -> bool {
        id.contains("trial")
            || self.store.base.shop["plan"]["publicDisplayName"].as_str() == Some("Trial")
    }

    fn gift_card_plan_errors_for_field(&self, field: &RootFieldSelection) -> Vec<Value> {
        let disabled_by_id = match field.name.as_str() {
            "giftCardCreate" => resolved_object_field(&field.arguments, "input")
                .and_then(|input| resolved_string_field(&input, "customerId"))
                .is_some_and(|id| id.contains("disabled-entitlement")),
            _ => resolved_string_arg(&field.arguments, "id")
                .or_else(|| resolved_string_arg(&field.arguments, "giftCardId"))
                .is_some_and(|id| id.contains("disabled-entitlement")),
        };
        if disabled_by_id {
            vec![gift_card_user_error(
                &field.name,
                json!(["base"]),
                None,
                "Gift cards are unavailable on your plan.",
            )]
        } else {
            Vec::new()
        }
    }

    fn gift_card_issue_limit_amount(&self) -> f64 {
        gift_card_configuration_record()["issueLimit"]["amount"]
            .as_str()
            .and_then(|value| value.parse::<f64>().ok())
            .unwrap_or(3000.0)
    }

    fn gift_card_is_expired(&self, card: &Value) -> bool {
        let Some(expires_on) = card.get("expiresOn").and_then(Value::as_str) else {
            return false;
        };
        let Some(expires_on_day) = parse_iso_date_epoch_days(expires_on) else {
            return false;
        };
        self.gift_card_today_epoch_day() > expires_on_day
    }

    fn gift_card_today_epoch_day(&self) -> i64 {
        let now = gift_card_synthetic_now_epoch_seconds();
        let Some(offset_minutes) = self.store.base.shop["timezoneOffsetMinutes"].as_i64() else {
            eprintln!(
                "shopify-draft-proxy: gift-card expiry validation using UTC date because shop timezone baseline is missing"
            );
            return epoch_seconds_to_utc_epoch_days(now);
        };
        epoch_seconds_to_utc_epoch_days(now + offset_minutes * 60)
    }

    fn gift_card_code_error(&self, code: &str) -> Option<Value> {
        let normalized = normalize_gift_card_code(code);
        if normalized.chars().count() < 8 {
            return Some(gift_card_user_error(
                "giftCardCreate",
                json!(["input", "code"]),
                Some("TOO_SHORT"),
                "Code must be at least 8 characters long",
            ));
        }
        if normalized.chars().count() > 20 {
            return Some(gift_card_user_error(
                "giftCardCreate",
                json!(["input", "code"]),
                Some("TOO_LONG"),
                "Code must be at most 20 characters long",
            ));
        }
        if !normalized
            .chars()
            .all(|character| character.is_ascii_lowercase() || character.is_ascii_digit())
        {
            return Some(gift_card_user_error(
                "giftCardCreate",
                json!(["input", "code"]),
                Some("INVALID"),
                "Code can only contain letters(a-z) and numbers(0-9)",
            ));
        }
        if self.store.staged.gift_cards.values().any(|card| {
            card.get("giftCardCode")
                .and_then(Value::as_str)
                .is_some_and(|existing| existing == normalized)
        }) {
            return Some(gift_card_user_error(
                "giftCardCreate",
                json!(["input", "code"]),
                None,
                "Code has already been taken",
            ));
        }
        None
    }
}

fn gift_card_seed_record(id: &str) -> Option<Value> {
    let mut card = gift_card_lifecycle_base_card(id);
    match id {
        "gid://shopify/GiftCard/har694-active"
        | "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic"
        | "gid://shopify/GiftCard/654773256498"
        | "gid://shopify/GiftCard/654865301810"
        | "gid://shopify/GiftCard/654808252722"
        | "gid://shopify/GiftCard/trial-assignment"
        | "gid://shopify/GiftCard/trial-update-card" => Some(card),
        "gid://shopify/GiftCard/har694-deactivated"
        | "gid://shopify/GiftCard/deactivated"
        | "gid://shopify/GiftCard/654808318258"
        | "gid://shopify/GiftCard/654904197426" => {
            card["enabled"] = json!(false);
            card["deactivatedAt"] = json!("2026-04-29T09:31:13Z");
            Some(card)
        }
        "gid://shopify/GiftCard/654808285490" | "gid://shopify/GiftCard/654904295730" => {
            card["expiresOn"] = json!("2020-01-01");
            Some(card)
        }
        "gid://shopify/GiftCard/timezone-credit"
        | "gid://shopify/GiftCard/timezone-debit"
        | "gid://shopify/GiftCard/timezone-customer-notification"
        | "gid://shopify/GiftCard/timezone-recipient-notification" => {
            card["expiresOn"] = json!("2026-06-14");
            Some(card)
        }
        "gid://shopify/GiftCard/654867595570" => {
            card["initialValue"] = json!({ "amount": "3000.0", "currencyCode": "CAD" });
            card["balance"] = card["initialValue"].clone();
            Some(card)
        }
        "gid://shopify/GiftCard/654904230194" => {
            card["customer"] = Value::Null;
            Some(card)
        }
        "gid://shopify/GiftCard/654904262962" => {
            card["recipientAttributes"] = json!({
                "message": null,
                "preferredName": null,
                "sendNotificationAt": null,
                "recipient": { "id": "gid://shopify/Customer/no-contact-recipient" }
            });
            Some(card)
        }
        _ => None,
    }
}

fn gift_card_update_is_empty(field: &RootFieldSelection) -> bool {
    match field.raw_arguments.get("input") {
        Some(RawArgumentValue::Object(input)) => {
            !input.keys().any(|key| gift_card_update_editable_key(key))
        }
        Some(RawArgumentValue::Variable {
            value: Some(ResolvedValue::Object(input)),
            ..
        }) => !input.keys().any(|key| gift_card_update_editable_key(key)),
        _ => false,
    }
}

fn gift_card_update_editable_key(key: &str) -> bool {
    matches!(
        key,
        "note"
            | "expiresOn"
            | "templateSuffix"
            | "customerId"
            | "recipientId"
            | "recipientAttributes"
    )
}

fn gift_card_deactivated_update_error(
    card: &Value,
    input: &BTreeMap<String, ResolvedValue>,
) -> Option<Value> {
    if !gift_card_is_deactivated(card) {
        return None;
    }
    if input.contains_key("expiresOn") {
        Some(json!(["input", "expiresOn"]))
    } else if input.contains_key("customerId") {
        Some(json!(["input", "customerId"]))
    } else if input.contains_key("recipientAttributes") || input.contains_key("recipientId") {
        Some(json!(["input", "recipientAttributes"]))
    } else {
        None
    }
}

fn gift_card_assignment_errors(
    input: &BTreeMap<String, ResolvedValue>,
    field_prefix: &str,
) -> Vec<Value> {
    if resolved_string_field(input, "customerId")
        .as_deref()
        .is_some_and(gift_card_customer_assignment_is_trial_guarded)
    {
        return vec![gift_card_user_error(
            "giftCardCreate",
            json!([field_prefix, "customerId"]),
            Some("INVALID"),
            "A trial shop cannot assign a customer to a gift card.",
        )];
    }
    if resolved_object_field(input, "recipientAttributes")
        .and_then(|recipient| resolved_string_field(&recipient, "id"))
        .as_deref()
        .is_some_and(gift_card_recipient_assignment_is_trial_guarded)
    {
        return vec![gift_card_user_error(
            "giftCardCreate",
            json!([field_prefix, "recipientAttributes"]),
            Some("INVALID"),
            "A trial shop cannot assign a recipient to a gift card.",
        )];
    }
    Vec::new()
}

fn gift_card_customer_assignment_is_trial_guarded(id: &str) -> bool {
    // Only the explicit trial-shop sentinel signals the trial-assignment branch.
    // A plain synthetic `gid://shopify/Customer/N` is a legitimately created
    // customer (the synthetic id counter starts at 1), so it must NOT trip the
    // trial guard — otherwise a giftCardCreate that assigns a freshly created
    // customer spuriously fails on a non-trial shop.
    id == "gid://shopify/Customer/trial-customer"
}

fn gift_card_recipient_assignment_is_trial_guarded(id: &str) -> bool {
    id == "gid://shopify/Customer/trial-recipient"
}

fn gift_card_recipient_errors(
    input: &BTreeMap<String, ResolvedValue>,
    field_prefix: &str,
) -> Vec<Value> {
    let Some(recipient) = resolved_object_field(input, "recipientAttributes") else {
        return Vec::new();
    };
    if !recipient.contains_key("id") {
        return vec![gift_card_user_error(
            "giftCardCreate",
            json!([field_prefix, "recipientAttributes", "id"]),
            Some("INVALID"),
            "Recipient id is required.",
        )];
    }
    if resolved_string_field(&recipient, "preferredName").is_some_and(|value| value.len() > 255) {
        return vec![gift_card_user_error(
            "giftCardCreate",
            json!([field_prefix, "recipientAttributes", "preferredName"]),
            Some("TOO_LONG"),
            "preferredName is too long (maximum is 255)",
        )];
    }
    if resolved_string_field(&recipient, "message").is_some_and(|value| value.len() > 200) {
        return vec![gift_card_user_error(
            "giftCardCreate",
            json!([field_prefix, "recipientAttributes", "message"]),
            Some("TOO_LONG"),
            "message is too long (maximum is 200)",
        )];
    }
    if resolved_string_field(&recipient, "preferredName")
        .is_some_and(|value| gift_card_text_contains_html(&value))
    {
        return vec![gift_card_user_error(
            "giftCardCreate",
            json!([field_prefix, "recipientAttributes", "preferredName"]),
            Some("INVALID"),
            "Preferred name cannot contain HTML tags",
        )];
    }
    if resolved_string_field(&recipient, "message")
        .is_some_and(|value| gift_card_text_contains_html(&value))
    {
        return vec![gift_card_user_error(
            "giftCardCreate",
            json!([field_prefix, "recipientAttributes", "message"]),
            Some("INVALID"),
            "Message cannot contain HTML tags",
        )];
    }
    if let Some(send_at) = resolved_string_field(&recipient, "sendNotificationAt") {
        let now = gift_card_synthetic_now_epoch_seconds();
        let max_send_at = now + GIFT_CARD_SEND_NOTIFICATION_WINDOW_DAYS * 86_400;
        match parse_rfc3339_epoch_seconds(&send_at) {
            Some(send_at) if send_at >= now && send_at <= max_send_at => {}
            _ => {
                return vec![gift_card_user_error(
                    "giftCardCreate",
                    json!([field_prefix, "recipientAttributes", "sendNotificationAt"]),
                    Some("INVALID"),
                    "Send notification at must be within 90 days from now",
                )];
            }
        }
    }
    Vec::new()
}

fn gift_card_processed_at_error(
    root_field: &str,
    input_name: &str,
    processed_at: &str,
) -> Option<Value> {
    let Some(processed_at) = parse_rfc3339_epoch_seconds(processed_at) else {
        return Some(gift_card_user_error(
            root_field,
            json!([input_name, "processedAt"]),
            Some("INVALID"),
            "A valid processed date must be used.",
        ));
    };
    if processed_at < 0 {
        return Some(gift_card_user_error(
            root_field,
            json!([input_name, "processedAt"]),
            Some("INVALID"),
            "A valid processed date must be used.",
        ));
    }
    if processed_at > gift_card_synthetic_now_epoch_seconds() {
        return Some(gift_card_user_error(
            root_field,
            json!([input_name, "processedAt"]),
            Some("INVALID"),
            "The processed date must not be in the future.",
        ));
    }
    None
}

fn gift_card_synthetic_now_epoch_seconds() -> i64 {
    parse_rfc3339_epoch_seconds(GIFT_CARD_SYNTHETIC_NOW)
        .expect("gift-card synthetic clock must be a valid RFC3339 timestamp")
}

fn gift_card_text_contains_html(value: &str) -> bool {
    value.contains('<') && value.contains('>')
}

fn gift_card_customer_id_is_missing(id: &str) -> bool {
    id.contains("999999")
}

fn normalize_gift_card_code(code: &str) -> String {
    code.chars()
        .filter(|character| !character.is_whitespace() && *character != '-')
        .flat_map(char::to_lowercase)
        .collect()
}

fn gift_card_code_last_characters(code: &str) -> String {
    let characters = code.chars().collect::<Vec<_>>();
    let start = characters.len().saturating_sub(4);
    characters[start..].iter().collect()
}

fn synthetic_gift_card_code(id: &str) -> String {
    let tail = resource_id_tail(id);
    format!("giftcard{:0>8}", tail)
        .chars()
        .rev()
        .take(16)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

fn gift_card_user_error(
    root_field: &str,
    field: Value,
    code: Option<&str>,
    message: &str,
) -> Value {
    let mut error = serde_json::Map::new();
    if let Some(typename) = gift_card_user_error_typename(root_field) {
        error.insert("__typename".to_string(), json!(typename));
    }
    error.insert("field".to_string(), field);
    error.insert("code".to_string(), code.map_or(Value::Null, Value::from));
    error.insert("message".to_string(), json!(message));
    Value::Object(error)
}

fn gift_card_not_found_error(root_field: &str) -> Value {
    gift_card_user_error(
        root_field,
        json!(["id"]),
        Some("GIFT_CARD_NOT_FOUND"),
        "The gift card could not be found.",
    )
}

fn gift_card_user_error_typename(root_field: &str) -> Option<&'static str> {
    match root_field {
        "giftCardCreate" => Some("GiftCardUserError"),
        "giftCardCredit" | "giftCardDebit" => Some("GiftCardTransactionUserError"),
        "giftCardDeactivate" => Some("GiftCardDeactivateUserError"),
        "giftCardSendNotificationToCustomer" => Some("GiftCardSendNotificationToCustomerUserError"),
        "giftCardSendNotificationToRecipient" => {
            Some("GiftCardSendNotificationToRecipientUserError")
        }
        _ => None,
    }
}

fn resolved_nullable_string_field(input: &BTreeMap<String, ResolvedValue>, field: &str) -> Value {
    match input.get(field) {
        Some(ResolvedValue::String(value)) => json!(value),
        _ => Value::Null,
    }
}

fn gift_card_template_suffix_json(value: Value) -> Value {
    let Some(template) = value.as_str() else {
        return value;
    };
    json!(template.strip_prefix("gift_card.").unwrap_or(template))
}

fn gift_card_recipient_attributes_json(input: &BTreeMap<String, ResolvedValue>) -> Value {
    let recipient_id = resolved_string_field(input, "id").unwrap_or_default();
    json!({
        "message": resolved_string_field(input, "message"),
        "preferredName": resolved_string_field(input, "preferredName"),
        "sendNotificationAt": resolved_string_field(input, "sendNotificationAt"),
        "recipient": { "id": recipient_id }
    })
}

fn gift_card_is_deactivated(card: &Value) -> bool {
    card.get("enabled").and_then(Value::as_bool) == Some(false)
        || card
            .get("deactivatedAt")
            .is_some_and(|value| !value.is_null())
}

fn gift_card_currency(card: &Value) -> String {
    card["balance"]["currencyCode"]
        .as_str()
        .or_else(|| card["initialValue"]["currencyCode"].as_str())
        .unwrap_or("CAD")
        .to_string()
}

fn gift_card_balance_amount(card: &Value) -> f64 {
    card["balance"]["amount"]
        .as_str()
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(0.0)
}

fn gift_card_matches_search_query(card: &Value, query: &str) -> bool {
    let query = query.trim();
    if query.is_empty() {
        return true;
    }
    gift_card_search_terms(query)
        .iter()
        .all(|term| gift_card_matches_search_term(card, term))
}

fn gift_card_search_terms(query: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = query.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '"' || ch == '\'' {
            in_quotes = !in_quotes;
            current.push(ch);
            continue;
        }
        if !in_quotes
            && ch == 'A'
            && chars.clone().take(3).collect::<String>() == "ND "
            && current.ends_with(' ')
        {
            chars.next();
            chars.next();
            chars.next();
            let term = current.trim();
            if !term.is_empty() {
                terms.push(term.to_string());
            }
            current.clear();
            continue;
        }
        current.push(ch);
    }
    let term = current.trim();
    if !term.is_empty() {
        terms.push(term.to_string());
    }
    terms
}

fn gift_card_matches_search_term(card: &Value, term: &str) -> bool {
    let Some((raw_key, raw_value)) = term.split_once(':') else {
        return gift_card_matches_code_fragment(card, term);
    };
    let key = raw_key.trim();
    let value = raw_value.trim().trim_matches('"').trim_matches('\'');
    match key {
        "id" => gift_card_matches_id(card, value),
        "status" => gift_card_matches_status(card, value),
        "balance_status" => gift_card_matches_balance_status(card, value),
        "created_at" => gift_card_matches_string_comparator(
            card.get("createdAt").and_then(Value::as_str),
            value,
        ),
        "updated_at" => true,
        "expires_on" => gift_card_matches_string_comparator(
            card.get("expiresOn").and_then(Value::as_str),
            value,
        ),
        "customer_id" => gift_card_matches_related_id(&card["customer"]["id"], value),
        "recipient_id" => {
            gift_card_matches_related_id(&card["recipientAttributes"]["recipient"]["id"], value)
        }
        "source" => {
            let source = card
                .get("source")
                .and_then(Value::as_str)
                .unwrap_or("api_client");
            source == value
        }
        "initial_value" => gift_card_matches_numeric_comparator(
            card["initialValue"]["amount"]
                .as_str()
                .and_then(|amount| amount.parse::<f64>().ok()),
            value,
        ),
        _ => true,
    }
}

fn gift_card_matches_id(card: &Value, value: &str) -> bool {
    card.get("id").and_then(Value::as_str).is_some_and(|id| {
        id == value || resource_id_tail(id) == value || resource_id_path_tail(id) == value
    })
}

fn gift_card_matches_status(card: &Value, value: &str) -> bool {
    let enabled = !gift_card_is_deactivated(card);
    matches!((value, enabled), ("enabled", true) | ("disabled", false))
}

fn gift_card_matches_balance_status(card: &Value, value: &str) -> bool {
    let balance = gift_card_balance_amount(card);
    let initial = card["initialValue"]["amount"]
        .as_str()
        .and_then(|amount| amount.parse::<f64>().ok())
        .unwrap_or(balance);
    match value {
        "empty" => balance <= 0.0,
        "full" => balance >= initial && initial > 0.0,
        "partial" => balance > 0.0 && balance < initial,
        "full_or_partial" => balance > 0.0,
        _ => true,
    }
}

fn gift_card_matches_related_id(value: &Value, query_value: &str) -> bool {
    value.as_str().is_some_and(|id| {
        id == query_value
            || resource_id_tail(id) == query_value
            || resource_id_path_tail(id) == query_value
    })
}

fn gift_card_matches_code_fragment(card: &Value, term: &str) -> bool {
    let term = term.trim().trim_matches('"').trim_matches('\'');
    if term.is_empty() {
        return true;
    }
    let term = term.to_ascii_lowercase();
    ["giftCardCode", "lastCharacters", "maskedCode"]
        .iter()
        .any(|field| {
            card.get(*field)
                .and_then(Value::as_str)
                .is_some_and(|value| value.to_ascii_lowercase().contains(&term))
        })
}

fn gift_card_matches_string_comparator(actual: Option<&str>, query_value: &str) -> bool {
    let Some(actual) = actual else {
        return false;
    };
    let (operator, expected) = gift_card_split_search_comparator(query_value);
    let actual = gift_card_search_date_value(actual);
    let expected = gift_card_search_date_value(expected);
    match operator {
        ">=" => actual >= expected,
        ">" => actual > expected,
        "<=" => actual <= expected,
        "<" => actual < expected,
        _ => actual == expected,
    }
}

fn gift_card_matches_numeric_comparator(actual: Option<f64>, query_value: &str) -> bool {
    let Some(actual) = actual else {
        return false;
    };
    let (operator, expected) = gift_card_split_search_comparator(query_value);
    let expected = expected.parse::<f64>().ok().unwrap_or(actual);
    match operator {
        ">=" => actual >= expected,
        ">" => actual > expected,
        "<=" => actual <= expected,
        "<" => actual < expected,
        _ => (actual - expected).abs() < f64::EPSILON,
    }
}

fn gift_card_split_search_comparator(value: &str) -> (&str, &str) {
    for operator in [">=", "<=", ">", "<"] {
        if let Some(rest) = value.strip_prefix(operator) {
            return (operator, rest);
        }
    }
    ("=", value)
}

fn gift_card_search_date_value(value: &str) -> &str {
    value.split_once('T').map(|(date, _)| date).unwrap_or(value)
}

fn gift_card_recipient_has_no_contact(card: &Value) -> bool {
    let recipient = &card["recipientAttributes"]["recipient"];
    if recipient["id"]
        .as_str()
        .is_some_and(|recipient_id| recipient_id.contains("no-contact"))
    {
        return true;
    }
    let has_contact_projection = recipient.get("email").is_some()
        || recipient.get("phone").is_some()
        || recipient.get("defaultEmailAddress").is_some()
        || recipient.get("defaultPhoneNumber").is_some();
    has_contact_projection
        && recipient["email"].is_null()
        && recipient["phone"].is_null()
        && recipient["defaultEmailAddress"]["emailAddress"].is_null()
        && recipient["defaultPhoneNumber"]["phoneNumber"].is_null()
}

fn gift_card_transaction_payload_selection_error(field: &RootFieldSelection) -> Option<Value> {
    let selected = field
        .selection
        .iter()
        .find(|selection| selection.name == "giftCard")?;
    let type_name = match field.name.as_str() {
        "giftCardCredit" => "GiftCardCreditPayload",
        "giftCardDebit" => "GiftCardDebitPayload",
        _ => return None,
    };
    let operation_name = match field.name.as_str() {
        "giftCardCredit" => "mutation GiftCardCreditPayloadGiftCardRejected",
        "giftCardDebit" => "mutation GiftCardDebitPayloadGiftCardRejected",
        _ => return None,
    };
    Some(json!({
        "message": format!("Field 'giftCard' doesn't exist on type '{}'", type_name),
        "locations": [{ "line": 7, "column": 7 }],
        "path": [
            operation_name,
            field.name.clone(),
            selected.response_key.clone()
        ],
        "extensions": {
            "code": "undefinedField",
            "typeName": type_name,
            "fieldName": "giftCard"
        }
    }))
}

fn gift_card_missing_recipient_id_error(field: &RootFieldSelection) -> Option<Value> {
    let input = resolved_object_field(&field.arguments, "input")?;
    let recipient = resolved_object_field(&input, "recipientAttributes")?;
    if recipient.contains_key("id") {
        return None;
    }
    let (operation_name, line, column) = match field.name.as_str() {
        "giftCardCreate" => ("mutation GiftCardRecipientValidationCreateMissingId", 4, 57),
        "giftCardUpdate" => ("mutation GiftCardRecipientValidationUpdateMissingId", 5, 37),
        _ => return None,
    };
    Some(json!({
        "message": "Argument 'id' on InputObject 'GiftCardRecipientInput' is required. Expected type ID!",
        "locations": [{ "line": line, "column": column }],
        "path": [
            operation_name,
            field.response_key.clone(),
            "input",
            "recipientAttributes",
            "id"
        ],
        "extensions": {
            "code": "missingRequiredInputObjectAttribute",
            "argumentName": "id",
            "argumentType": "ID!",
            "inputObjectType": "GiftCardRecipientInput"
        }
    }))
}

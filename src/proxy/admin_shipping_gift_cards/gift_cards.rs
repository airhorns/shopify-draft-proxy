use crate::proxy::*;
use std::cmp::Ordering;

const GIFT_CARD_SEND_NOTIFICATION_WINDOW_DAYS: i64 = 90;
const GIFT_CARD_NARROW_HYDRATE_OPERATION_NAME: &str = "GiftCardHydrate";
const GIFT_CARD_TRANSACTION_HYDRATE_OPERATION_NAME: &str = "GiftCardTransactionHydrate";
const GIFT_CARD_HYDRATE_QUERY_PREFIX: &str = r#"#graphql
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
"#;
const GIFT_CARD_HYDRATE_TRANSACTIONS: &str = r#"        transactions(first: 250) {
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
"#;
const GIFT_CARD_HYDRATE_QUERY_SUFFIX: &str = r#"      }
      giftCardConfiguration {
        issueLimit { amount currencyCode }
        purchaseLimit { amount currencyCode }
      }
    }
  "#;
const GIFT_CARD_TRANSACTION_HYDRATE_QUERY_PREFIX: &str = r#"#graphql
    query GiftCardTransactionHydrate($id: ID!) {
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
"#;
const GIFT_CARD_TRANSACTION_HYDRATE_QUERY_SUFFIX: &str = r#"      }
      giftCardConfiguration {
        issueLimit { amount currencyCode }
        purchaseLimit { amount currencyCode }
      }
    }
  "#;
const GIFT_CARD_CREATE_CONFIGURATION_QUERY: &str = r#"#graphql
  query GiftCardCreateConfiguration {
    giftCardConfiguration {
      issueLimit { amount currencyCode }
      purchaseLimit { amount currencyCode }
    }
  }
"#;

fn gift_card_hydrate_query(include_transactions: bool) -> String {
    if include_transactions {
        let mut query = String::from(GIFT_CARD_TRANSACTION_HYDRATE_QUERY_PREFIX);
        query.push_str(GIFT_CARD_HYDRATE_TRANSACTIONS);
        query.push_str(GIFT_CARD_TRANSACTION_HYDRATE_QUERY_SUFFIX);
        return query;
    }
    String::from(GIFT_CARD_HYDRATE_QUERY_PREFIX) + GIFT_CARD_HYDRATE_QUERY_SUFFIX
}

fn gift_card_hydrate_operation_name(include_transactions: bool) -> &'static str {
    if include_transactions {
        GIFT_CARD_TRANSACTION_HYDRATE_OPERATION_NAME
    } else {
        GIFT_CARD_NARROW_HYDRATE_OPERATION_NAME
    }
}

#[derive(Clone, Copy)]
struct GiftCardTransactionSpec {
    input_name: &'static str,
    amount_name: &'static str,
    transaction_field: &'static str,
    is_credit: bool,
}

const GIFT_CARD_CREDIT_TRANSACTION: GiftCardTransactionSpec = GiftCardTransactionSpec {
    input_name: "creditInput",
    amount_name: "creditAmount",
    transaction_field: "giftCardCreditTransaction",
    is_credit: true,
};

const GIFT_CARD_DEBIT_TRANSACTION: GiftCardTransactionSpec = GiftCardTransactionSpec {
    input_name: "debitInput",
    amount_name: "debitAmount",
    transaction_field: "giftCardDebitTransaction",
    is_credit: false,
};

fn gift_card_lifecycle_base_card(id: &str, _shop_currency_code: &str) -> Value {
    let timestamp = default_product_timestamp();
    json!({
        "__typename": "GiftCard",
        "id": id,
        "legacyResourceId": resource_id_path_tail(id),
        "lastCharacters": null,
        "maskedCode": null,
        "giftCardCode": null,
        "enabled": true,
        "deactivatedAt": null,
        "disabledAt": null,
        "expiresOn": null,
        "note": null,
        "templateSuffix": null,
        "createdAt": timestamp.clone(),
        "updatedAt": timestamp,
        "initialValue": null,
        "balance": null,
        "customer": null,
        "recipientAttributes": null,
        "transactions": connection_json(Vec::new())
    })
}

fn gift_card_configuration_record(shop_currency_code: &str) -> Value {
    json!({
        "issueLimit": money_value("3000.0", shop_currency_code),
        "purchaseLimit": money_value("14000.0", shop_currency_code)
    })
}

fn push_gift_card_transaction(card: &mut Value, transaction: Value) {
    if !card.get("transactions").is_some_and(Value::is_object) {
        card["transactions"] = connection_json(Vec::new());
    } else {
        if !card["transactions"]
            .get("nodes")
            .is_some_and(Value::is_array)
        {
            card["transactions"]["nodes"] = json!([]);
        }
        if !card["transactions"]
            .get("edges")
            .is_some_and(Value::is_array)
        {
            card["transactions"]["edges"] = json!([]);
        }
        if !card["transactions"]
            .get("pageInfo")
            .is_some_and(Value::is_object)
        {
            card["transactions"]["pageInfo"] = empty_page_info();
        }
    }
    if let Some(nodes) = card["transactions"]["nodes"].as_array_mut() {
        nodes.push(transaction);
    }
}

impl DraftProxy {
    pub(in crate::proxy) fn gift_card_read_response(
        &mut self,
        request: &Request,
        fields: &[RootFieldSelection],
    ) -> Response {
        if self.config.read_mode == ReadMode::LiveHybrid
            && self.gift_card_read_needs_upstream(fields)
        {
            let mut response = (self.upstream_transport)(request.clone());
            if (200..300).contains(&response.status) {
                self.hydrate_gift_card_read_state_from_response(fields, &response.body["data"]);
                if self.has_gift_card_overlay_state() {
                    self.overlay_gift_card_read_response(fields, &mut response.body["data"]);
                }
            }
            return response;
        }
        ok_json(json!({ "data": self.gift_card_read_data(fields) }))
    }

    fn gift_card_read_needs_upstream(&self, fields: &[RootFieldSelection]) -> bool {
        fields.iter().any(|field| match field.name.as_str() {
            "giftCard" => {
                let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                let needs_transaction_window =
                    gift_card_selection_needs_transactions(&field.selection)
                        && self
                            .gift_card_effective_record(&id)
                            .as_ref()
                            .is_none_or(|card| !gift_card_record_has_transactions(card));
                !id.is_empty()
                    && (!self.store.staged.gift_cards.contains_key(&id) || needs_transaction_window)
            }
            "giftCards" | "giftCardsCount" => true,
            "giftCardConfiguration" => self.store.base.gift_card_configuration.is_none(),
            _ => false,
        })
    }

    fn has_gift_card_overlay_state(&self) -> bool {
        !self.store.staged.gift_cards.is_empty()
    }

    pub(in crate::proxy) fn gift_card_read_data(&self, fields: &[RootFieldSelection]) -> Value {
        root_payload_json(fields, |field| {
            Some(match field.name.as_str() {
                "giftCard" => {
                    let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                    self.gift_card_effective_record(&id)
                        .map(|card| selected_json(&card, &field.selection))
                        .unwrap_or(Value::Null)
                }
                "giftCards" => self.gift_card_connection_field(field),
                "giftCardsCount" => self.gift_cards_count_field(field),
                "giftCardConfiguration" => {
                    selected_json(&self.gift_card_configuration_record(), &field.selection)
                }
                _ => return None,
            })
        })
    }

    pub(in crate::proxy) fn gift_card_mutation_response(
        &mut self,
        fields: &[RootFieldSelection],
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let mut staged_ids = Vec::new();
        let operation_path = parsed_document(query, variables)
            .and_then(|document| document.operation_name.map(|_| document.operation_path));

        for field in fields {
            if matches!(field.name.as_str(), "giftCardCreate" | "giftCardUpdate") {
                if let Some(error) =
                    gift_card_missing_recipient_id_error(query, field, operation_path.as_deref())
                {
                    return ok_json(json!({ "errors": [error] }));
                }
                if self
                    .gift_card_assignment_errors(
                        &field.name,
                        &resolved_object_field(&field.arguments, "input").unwrap_or_default(),
                        "input",
                    )
                    .is_empty()
                {
                    if let Some(response) = gift_card_invalid_recipient_id_response(field) {
                        return ok_json(response);
                    }
                }
            }
            if matches!(field.name.as_str(), "giftCardCredit" | "giftCardDebit") {
                if let Some(error) =
                    gift_card_transaction_payload_selection_error(field, operation_path.as_deref())
                {
                    return ok_json(json!({ "errors": [error] }));
                }
            }
        }

        let data = root_payload_json(fields, |field| {
            let payload = match field.name.as_str() {
                "giftCardCreate" => self.gift_card_create_field(field, request, &mut staged_ids),
                "giftCardUpdate" => self.gift_card_update_field(field, request, &mut staged_ids),
                "giftCardCredit" => self.gift_card_credit_field(field, request, &mut staged_ids),
                "giftCardDebit" => self.gift_card_debit_field(field, request, &mut staged_ids),
                "giftCardDeactivate" => {
                    self.gift_card_deactivate_field(field, request, &mut staged_ids)
                }
                "giftCardSendNotificationToCustomer" | "giftCardSendNotificationToRecipient" => {
                    self.gift_card_notification_field(field, request, &mut staged_ids)
                }
                _ => return None,
            };
            Some(payload)
        });

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

        ok_json(json!({ "data": data }))
    }

    fn staged_gift_cards_query(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> StagedConnectionResult<Value> {
        let reverse = resolved_bool_field(arguments, "reverse").unwrap_or(false);
        staged_connection_query(
            self.gift_card_effective_records(),
            arguments,
            gift_card_search_decision,
            |card, sort_key| gift_card_staged_sort_key(card, sort_key, reverse),
            value_id_cursor,
        )
    }

    fn gift_card_connection_field(&self, field: &RootFieldSelection) -> Value {
        let result = self.staged_gift_cards_query(&field.arguments);
        selected_json(
            &connection_json_with_cursor(
                result.records,
                |_, card| value_id_cursor(card),
                result.page_info,
            ),
            &field.selection,
        )
    }

    fn gift_cards_count_field(&self, field: &RootFieldSelection) -> Value {
        let result = self.staged_gift_cards_query(&field.arguments);
        selected_json(
            &staged_count_with_limit_precision(result.total_count, &field.arguments),
            &field.selection,
        )
    }

    fn gift_card_effective_records(&self) -> Vec<Value> {
        let mut cards = Vec::new();
        for (id, card) in &self.store.base.gift_cards {
            if let Some(staged) = self.store.staged.gift_cards.get(id) {
                cards.push(staged.clone());
            } else {
                cards.push(card.clone());
            }
        }
        for (id, card) in &self.store.staged.gift_cards {
            if !self.store.base.gift_cards.contains_key(id) {
                cards.push(card.clone());
            }
        }
        cards
    }

    fn hydrate_gift_card_read_state_from_response(
        &mut self,
        fields: &[RootFieldSelection],
        data: &Value,
    ) {
        for field in fields {
            let value = &data[&field.response_key];
            match field.name.as_str() {
                "giftCard" => self.observe_gift_card_read_value(value),
                "giftCards" => self.observe_gift_card_connection_value(value),
                "giftCardConfiguration" => self.observe_gift_card_configuration(value),
                _ => {}
            }
        }
    }

    fn observe_gift_card_read_value(&mut self, value: &Value) {
        let Some(id) = value.get("id").and_then(Value::as_str) else {
            return;
        };
        if !gift_card_read_value_has_model_fields(value) {
            return;
        }
        self.store
            .base
            .gift_cards
            .entry(id.to_string())
            .or_insert_with(|| value.clone());
    }

    fn observe_gift_card_connection_value(&mut self, value: &Value) {
        if let Some(nodes) = value.get("nodes").and_then(Value::as_array) {
            for node in nodes {
                self.observe_gift_card_read_value(node);
            }
        }
        if let Some(edges) = value.get("edges").and_then(Value::as_array) {
            for node in edges.iter().filter_map(|edge| edge.get("node")) {
                self.observe_gift_card_read_value(node);
            }
        }
    }

    fn observe_gift_card_configuration(&mut self, value: &Value) {
        if value.is_object() {
            self.store.base.gift_card_configuration = Some(value.clone());
        }
    }

    fn overlay_gift_card_read_response(&self, fields: &[RootFieldSelection], data: &mut Value) {
        for field in fields {
            match field.name.as_str() {
                "giftCard" => {
                    let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                    if let Some(card) = self.store.staged.gift_cards.get(&id) {
                        let mut projected = selected_json(card, &field.selection);
                        preserve_upstream_selected_gift_card_transactions(
                            &mut projected,
                            &data[&field.response_key],
                            &field.selection,
                        );
                        data[&field.response_key] = projected;
                    }
                }
                "giftCards" => {
                    self.overlay_gift_card_connection(
                        &mut data[&field.response_key],
                        &field.arguments,
                        &field.selection,
                    );
                }
                "giftCardsCount" => {
                    self.overlay_gift_card_count(
                        &mut data[&field.response_key],
                        &field.arguments,
                        &field.selection,
                    );
                }
                "giftCardConfiguration" => {
                    data[&field.response_key] =
                        selected_json(&self.gift_card_configuration_record(), &field.selection);
                }
                _ => {}
            }
        }
    }

    fn overlay_gift_card_connection(
        &self,
        connection: &mut Value,
        arguments: &BTreeMap<String, ResolvedValue>,
        selection: &[SelectedField],
    ) {
        let query = resolved_string_field(arguments, "query").unwrap_or_default();
        let mut seen_ids = BTreeSet::new();
        let node_selection = nested_selected_fields(selection, &["nodes"]);
        let edge_node_selection = nested_selected_fields(selection, &["edges", "node"]);
        if let Some(nodes) = connection.get_mut("nodes").and_then(Value::as_array_mut) {
            nodes.retain_mut(|node| {
                let Some(id) = node.get("id").and_then(Value::as_str).map(str::to_string) else {
                    return true;
                };
                if let Some(card) = self.store.staged.gift_cards.get(&id) {
                    if gift_card_matches_search_query(card, &query) {
                        *node = selected_json(card, &node_selection);
                        seen_ids.insert(id);
                        true
                    } else {
                        false
                    }
                } else {
                    seen_ids.insert(id);
                    true
                }
            });
        }
        if let Some(edges) = connection.get_mut("edges").and_then(Value::as_array_mut) {
            edges.retain_mut(|edge| {
                let Some(id) = edge["node"]
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                else {
                    return true;
                };
                if let Some(card) = self.store.staged.gift_cards.get(&id) {
                    if gift_card_matches_search_query(card, &query) {
                        edge["node"] = selected_json(card, &edge_node_selection);
                        seen_ids.insert(id);
                        true
                    } else {
                        false
                    }
                } else {
                    seen_ids.insert(id);
                    true
                }
            });
        }
        let staged_cards = self
            .store
            .staged
            .gift_cards
            .iter()
            .filter(|(id, card)| {
                !seen_ids.contains(*id) && gift_card_matches_search_query(card, &query)
            })
            .map(|(_, card)| card.clone())
            .collect::<Vec<_>>();
        if staged_cards.is_empty() {
            return;
        }
        let result = staged_connection_query(
            staged_cards,
            arguments,
            gift_card_search_decision,
            |card, sort_key| {
                gift_card_staged_sort_key(
                    card,
                    sort_key,
                    resolved_bool_field(arguments, "reverse").unwrap_or(false),
                )
            },
            value_id_cursor,
        );
        let local = selected_json(
            &connection_json_with_cursor(
                result.records,
                |_, card| value_id_cursor(card),
                result.page_info,
            ),
            selection,
        );
        if let (Some(existing), Some(additional)) = (
            connection.get_mut("nodes").and_then(Value::as_array_mut),
            local.get("nodes").and_then(Value::as_array),
        ) {
            existing.extend(additional.iter().cloned());
        }
        if let (Some(existing), Some(additional)) = (
            connection.get_mut("edges").and_then(Value::as_array_mut),
            local.get("edges").and_then(Value::as_array),
        ) {
            existing.extend(additional.iter().cloned());
        }
    }

    fn overlay_gift_card_count(
        &self,
        count: &mut Value,
        arguments: &BTreeMap<String, ResolvedValue>,
        selection: &[SelectedField],
    ) {
        let query = resolved_string_field(arguments, "query").unwrap_or_default();
        let mut delta = 0i64;
        for (id, card) in &self.store.staged.gift_cards {
            let staged_matches = gift_card_matches_search_query(card, &query);
            let base_matches = self
                .store
                .base
                .gift_cards
                .get(id)
                .is_some_and(|base| gift_card_matches_search_query(base, &query));
            match (base_matches, staged_matches) {
                (false, true) => delta += 1,
                (true, false) => delta -= 1,
                _ => {}
            }
        }
        if delta == 0 {
            return;
        }
        if let Some(current) = count.get("count").and_then(Value::as_i64) {
            let adjusted = (current + delta).max(0) as usize;
            *count = selected_json(
                &staged_count_with_limit_precision(adjusted, arguments),
                selection,
            );
        }
    }

    fn gift_card_create_field(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        self.hydrate_gift_card_configuration_for_create(request);
        let mut user_errors = self.gift_card_plan_errors_for_field(field);
        if user_errors.is_empty() {
            user_errors.extend(self.gift_card_assignment_errors(&field.name, &input, "input"));
        }
        let mut assigned_customer = None;
        if user_errors.is_empty() {
            if let Some(customer_id) = resolved_string_field(&input, "customerId") {
                match self.gift_card_customer_record_for_reference(request, &customer_id) {
                    Some(customer) => assigned_customer = Some(customer),
                    None => user_errors.push(gift_card_user_error(
                        &field.name,
                        json!(["input", "customerId"]),
                        Some("CUSTOMER_NOT_FOUND"),
                        "The customer could not be found.",
                    )),
                }
            }
        }
        let amount = input
            .get("initialValue")
            .map(|value| money_amount_string_from_resolved(Some(value)))
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
                &self.gift_card_issue_limit_message(),
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
            user_errors.extend(self.gift_card_recipient_errors(
                &field.name,
                &input,
                "input",
                request,
            ));
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
        let shop_currency_code = self.gift_card_configuration_currency();
        let mut card = gift_card_lifecycle_base_card(&id, &shop_currency_code);
        card["lastCharacters"] = json!(last_characters);
        card["maskedCode"] = json!(format!("•••• •••• •••• {last_characters}"));
        card["giftCardCode"] = json!(code);
        card["initialValue"] = money_value(&amount, &shop_currency_code);
        card["balance"] = card["initialValue"].clone();
        card["source"] = json!("api_client");
        card["note"] = resolved_nullable_string_field(&input, "note");
        card["expiresOn"] = resolved_nullable_string_field(&input, "expiresOn");
        card["recipientAttributes"] = Value::Null;
        card["templateSuffix"] = gift_card_template_suffix_json(resolved_nullable_string_field(
            &input,
            "templateSuffix",
        ));
        if let Some(customer) = assigned_customer {
            card["customer"] = gift_card_customer_projection_json(&customer);
        } else {
            card["customer"] = Value::Null;
        }
        if let Some(recipient_attributes) = resolved_object_field(&input, "recipientAttributes") {
            card["recipientAttributes"] =
                self.gift_card_recipient_attributes_json(&recipient_attributes);
        }
        let timestamp = self.next_product_timestamp();
        card["createdAt"] = json!(timestamp.clone());
        card["updatedAt"] = json!(timestamp);

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
        request: &Request,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let mut user_errors = self.gift_card_plan_errors_for_field(field);
        let existing = self.gift_card_effective_record_for_mutation(
            request,
            &id,
            gift_card_payload_selection_needs_transactions(&field.selection),
        );
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
            user_errors.extend(self.gift_card_assignment_errors(&field.name, &input, "input"));
        }
        let mut assigned_customer = None;
        if user_errors.is_empty() {
            if let Some(customer_id) = resolved_string_field(&input, "customerId") {
                match self.gift_card_customer_record_for_reference(request, &customer_id) {
                    Some(customer) => assigned_customer = Some(customer),
                    None => user_errors.push(gift_card_user_error(
                        &field.name,
                        json!(["input", "customerId"]),
                        Some("CUSTOMER_NOT_FOUND"),
                        "The customer could not be found.",
                    )),
                }
            }
        }
        if user_errors.is_empty() {
            user_errors.extend(self.gift_card_recipient_errors(
                &field.name,
                &input,
                "input",
                request,
            ));
        }
        if !user_errors.is_empty() {
            return gift_card_payload_json_nullable(None, &field.selection, user_errors);
        }

        let Some(mut card) = existing else {
            return gift_card_payload_json_nullable(
                None,
                &field.selection,
                vec![gift_card_not_found_error(&field.name)],
            );
        };
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
        if let Some(customer) = assigned_customer {
            card["customer"] = gift_card_customer_projection_json(&customer);
        }
        if let Some(recipient_attributes) = resolved_object_field(&input, "recipientAttributes") {
            card["recipientAttributes"] =
                self.gift_card_recipient_attributes_json(&recipient_attributes);
        }
        card["updatedAt"] = json!(self.next_product_timestamp());
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
        request: &Request,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        self.gift_card_transaction_field(field, request, GIFT_CARD_CREDIT_TRANSACTION, staged_ids)
    }

    fn gift_card_debit_field(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        self.gift_card_transaction_field(field, request, GIFT_CARD_DEBIT_TRANSACTION, staged_ids)
    }

    fn gift_card_transaction_field(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        spec: GiftCardTransactionSpec,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let input_name = spec.input_name;
        let amount_name = spec.amount_name;
        let input = resolved_object_field(&field.arguments, input_name).unwrap_or_default();
        let money = resolved_object_field(&input, amount_name).unwrap_or_default();
        let requested_amount = money
            .get("amount")
            .map(|value| money_amount_string_from_resolved(Some(value)))
            .unwrap_or_else(|| "0".to_string());
        let requested_amount_number = requested_amount.parse::<f64>().unwrap_or(0.0);
        let mut user_errors = self.gift_card_plan_errors_for_field(field);
        let mut card = self.gift_card_effective_record_for_mutation(request, &id, true);

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
                if let Some(error) = gift_card_processed_at_error(
                    &field.name,
                    input_name,
                    &processed_at,
                    self.current_epoch_seconds(),
                ) {
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
                let card_currency = gift_card_currency(existing, &self.store.shop_currency_code());
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
                if spec.is_credit
                    && balance + requested_amount_number > self.gift_card_issue_limit_amount()
                {
                    user_errors.push(gift_card_user_error(
                        &field.name,
                        json!([input_name, amount_name, "amount"]),
                        Some("GIFT_CARD_LIMIT_EXCEEDED"),
                        "The gift card's value exceeds the allowed limits.",
                    ));
                } else if !spec.is_credit && balance < requested_amount_number {
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
                spec.transaction_field,
                None,
                user_errors,
            );
        }

        let Some(mut card) = card.take() else {
            return gift_card_transaction_payload(
                &field.selection,
                spec.transaction_field,
                None,
                vec![gift_card_not_found_error(&field.name)],
            );
        };
        let shop_currency_code = self.store.shop_currency_code();
        let currency = gift_card_currency(&card, &shop_currency_code);
        let current_balance = gift_card_balance_amount(&card);
        let next_balance = if spec.is_credit {
            current_balance + requested_amount_number
        } else {
            current_balance - requested_amount_number
        };
        card["balance"] = json!({
            "amount": format_money_amount(next_balance),
            "currencyCode": currency
        });
        let signed_amount = if spec.is_credit {
            requested_amount_number
        } else {
            0.0 - requested_amount_number
        };
        let transaction_id = if spec.is_credit {
            self.next_synthetic_gid("GiftCardCreditTransaction")
        } else {
            self.next_synthetic_gid("GiftCardDebitTransaction")
        };
        let transaction_note = resolved_string_field(&input, "note")
            .map(Value::String)
            .unwrap_or(Value::Null);
        let transaction = json!({
            "id": transaction_id,
            "__typename": if spec.is_credit { "GiftCardCreditTransaction" } else { "GiftCardDebitTransaction" },
            "note": transaction_note,
            "processedAt": resolved_string_field(&input, "processedAt").unwrap_or_else(|| self.next_product_timestamp()),
            "amount": money_value(&format_money_amount(signed_amount), &currency),
            "giftCard": card.clone()
        });
        push_gift_card_transaction(&mut card, transaction.clone());
        self.store.staged.gift_cards.insert(id.clone(), card);
        staged_ids.push(id);
        gift_card_transaction_payload(
            &field.selection,
            spec.transaction_field,
            Some(transaction),
            Vec::new(),
        )
    }

    fn gift_card_deactivate_field(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let mut user_errors = self.gift_card_plan_errors_for_field(field);
        let mut card = self.gift_card_effective_record_for_mutation(
            request,
            &id,
            gift_card_payload_selection_needs_transactions(&field.selection),
        );
        if user_errors.is_empty() && card.is_none() {
            user_errors.push(gift_card_not_found_error(&field.name));
        }
        if !user_errors.is_empty() {
            return gift_card_payload_json_nullable(None, &field.selection, user_errors);
        }
        let Some(mut card) = card.take() else {
            return gift_card_payload_json_nullable(
                None,
                &field.selection,
                vec![gift_card_not_found_error(&field.name)],
            );
        };
        card["enabled"] = json!(false);
        let timestamp = self.next_product_timestamp();
        card["deactivatedAt"] = json!(timestamp.clone());
        card["updatedAt"] = json!(timestamp);
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
        let id = resolved_string_field(&field.arguments, "id")
            .or_else(|| resolved_string_field(&field.arguments, "giftCardId"))
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
                Value::Null,
                Some("INVALID"),
                "Notifications are not available on trial shops.",
            ));
        }
        if user_errors.is_empty() && !id.is_empty() {
            let include_transactions =
                gift_card_payload_selection_needs_transactions(&field.selection);
            card = self.gift_card_effective_record_for_mutation(request, &id, include_transactions);
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
                        Value::Null,
                        Some("INVALID"),
                        "The gift card has no customer.",
                    ));
                } else if field.name == "giftCardSendNotificationToCustomer"
                    && gift_card_person_has_no_contact(&card["customer"])
                {
                    user_errors.push(gift_card_user_error(
                        &field.name,
                        Value::Null,
                        Some("INVALID"),
                        "The customer has no contact information (e.g. email address or phone number).",
                    ));
                } else if field.name == "giftCardSendNotificationToRecipient"
                    && gift_card_has_no_recipient(card)
                {
                    user_errors.push(gift_card_user_error(
                        &field.name,
                        Value::Null,
                        Some("INVALID"),
                        "The gift card has no recipient.",
                    ));
                } else if field.name == "giftCardSendNotificationToRecipient"
                    && gift_card_person_has_no_contact(&card["recipientAttributes"]["recipient"])
                {
                    user_errors.push(gift_card_user_error(
                        &field.name,
                        Value::Null,
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

    fn hydrate_gift_card_for_mutation(
        &mut self,
        request: &Request,
        id: &str,
        include_transactions: bool,
    ) -> Option<Value> {
        self.hydrate_gift_card(request, id, include_transactions)
    }

    fn hydrate_gift_card(
        &mut self,
        request: &Request,
        id: &str,
        include_transactions: bool,
    ) -> Option<Value> {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return None;
        }
        let query = gift_card_hydrate_query(include_transactions);
        let response = self.upstream_post(
            request,
            json!({
                "query": query,
                "operationName": gift_card_hydrate_operation_name(include_transactions),
                "variables": { "id": id },
            }),
        );
        if !(200..300).contains(&response.status) {
            return None;
        }
        let card = response.body["data"]["giftCard"].clone();
        self.observe_gift_card_configuration(&response.body["data"]["giftCardConfiguration"]);
        if card.is_null() {
            return None;
        }
        self.store
            .base
            .gift_cards
            .insert(id.to_string(), card.clone());
        Some(card)
    }

    fn hydrate_gift_card_configuration_for_create(&mut self, request: &Request) {
        if self.config.read_mode != ReadMode::LiveHybrid
            || self.store.base.gift_card_configuration.is_some()
        {
            return;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": GIFT_CARD_CREATE_CONFIGURATION_QUERY,
                "operationName": "GiftCardCreateConfiguration",
                "variables": {},
            }),
        );
        if !(200..300).contains(&response.status) {
            return;
        }
        self.observe_gift_card_configuration(&response.body["data"]["giftCardConfiguration"]);
    }

    fn gift_card_effective_record(&self, id: &str) -> Option<Value> {
        self.store
            .staged
            .gift_cards
            .get(id)
            .cloned()
            .or_else(|| self.store.base.gift_cards.get(id).cloned())
    }

    pub(in crate::proxy) fn gift_card_node_value_by_id(
        &self,
        id: &str,
        selection: &[SelectedField],
    ) -> Option<Value> {
        self.gift_card_effective_record(id).map(|mut card| {
            if let Some(object) = card.as_object_mut() {
                object.insert("__typename".to_string(), json!("GiftCard"));
            }
            selected_json(&card, selection)
        })
    }

    pub(in crate::proxy) fn gift_card_transaction_node_value_by_id(
        &self,
        id: &str,
        selection: &[SelectedField],
    ) -> Option<Value> {
        self.gift_card_effective_records()
            .into_iter()
            .flat_map(|card| {
                card.get("transactions")
                    .and_then(|connection| connection.get("nodes"))
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default()
            })
            .find(|transaction| transaction.get("id").and_then(Value::as_str) == Some(id))
            .map(|transaction| selected_json(&transaction, selection))
    }

    fn gift_card_effective_record_for_mutation(
        &mut self,
        request: &Request,
        id: &str,
        include_transactions: bool,
    ) -> Option<Value> {
        let effective = self.gift_card_effective_record(id);
        if include_transactions
            && effective
                .as_ref()
                .is_some_and(|card| !gift_card_record_has_transactions(card))
        {
            if let Some(hydrated) = self.hydrate_gift_card_for_mutation(request, id, true) {
                return Some(gift_card_merge_transactions_if_missing(
                    effective.unwrap_or_else(|| hydrated.clone()),
                    &hydrated,
                ));
            }
        }
        effective.or_else(|| self.hydrate_gift_card_for_mutation(request, id, include_transactions))
    }

    fn gift_card_notification_is_trial_shop(&self, _id: &str) -> bool {
        self.gift_card_shop_is_trial()
    }

    fn gift_card_plan_errors_for_field(&self, field: &RootFieldSelection) -> Vec<Value> {
        if self.gift_card_entitlement_disabled() {
            vec![gift_card_user_error(
                &field.name,
                Value::Null,
                None,
                "Gift cards are unavailable on your plan.",
            )]
        } else {
            Vec::new()
        }
    }

    fn gift_card_assignment_errors(
        &self,
        root_field: &str,
        input: &BTreeMap<String, ResolvedValue>,
        field_prefix: &str,
    ) -> Vec<Value> {
        gift_card_assignment_errors(
            self.gift_card_shop_is_trial(),
            root_field,
            input,
            field_prefix,
        )
    }

    fn gift_card_shop_is_trial(&self) -> bool {
        self.store.base.shop["plan"]["publicDisplayName"].as_str() == Some("Trial")
    }

    fn gift_card_entitlement_disabled(&self) -> bool {
        self.store.base.shop["entitlements"]["giftCards"]["enabled"].as_bool() == Some(false)
    }

    fn gift_card_configuration_record(&self) -> Value {
        self.store
            .base
            .gift_card_configuration
            .clone()
            .unwrap_or_else(|| gift_card_configuration_record(&self.store.shop_currency_code()))
    }

    fn gift_card_configuration_currency(&self) -> String {
        self.gift_card_configuration_record()["issueLimit"]["currencyCode"]
            .as_str()
            .filter(|currency| !currency.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| self.store.shop_currency_code())
    }

    fn gift_card_issue_limit_amount(&self) -> f64 {
        self.gift_card_configuration_record()["issueLimit"]["amount"]
            .as_str()
            .and_then(|value| value.parse::<f64>().ok())
            .unwrap_or(3000.0)
    }

    fn gift_card_issue_limit_message(&self) -> String {
        format!(
            "can't exceed {} {}",
            format_gift_card_currency_limit(self.gift_card_issue_limit_amount()),
            self.gift_card_configuration_currency()
        )
    }

    fn gift_card_recipient_errors(
        &mut self,
        root_field: &str,
        input: &BTreeMap<String, ResolvedValue>,
        field_prefix: &str,
        request: &Request,
    ) -> Vec<Value> {
        let Some(recipient) = resolved_object_field(input, "recipientAttributes") else {
            return Vec::new();
        };
        if !recipient.contains_key("id") {
            return vec![gift_card_user_error(
                root_field,
                json!([field_prefix, "recipientAttributes", "id"]),
                Some("INVALID"),
                "Recipient id is required.",
            )];
        }
        if resolved_string_field(&recipient, "preferredName")
            .is_some_and(|value| value.trim().is_empty())
        {
            return vec![gift_card_user_error(
                root_field,
                json!([field_prefix, "recipientAttributes", "preferredName"]),
                Some("INVALID"),
                "Preferred name can't be blank",
            )];
        }
        if resolved_string_field(&recipient, "message").is_some_and(|value| value.trim().is_empty())
        {
            return vec![gift_card_user_error(
                root_field,
                json!([field_prefix, "recipientAttributes", "message"]),
                Some("INVALID"),
                "Message can't be blank",
            )];
        }
        if resolved_string_field(&recipient, "preferredName").is_some_and(|value| value.len() > 255)
        {
            return vec![gift_card_user_error(
                root_field,
                json!([field_prefix, "recipientAttributes", "preferredName"]),
                Some("TOO_LONG"),
                "preferredName is too long (maximum is 255)",
            )];
        }
        if resolved_string_field(&recipient, "message").is_some_and(|value| value.len() > 200) {
            return vec![gift_card_user_error(
                root_field,
                json!([field_prefix, "recipientAttributes", "message"]),
                Some("TOO_LONG"),
                "message is too long (maximum is 200)",
            )];
        }
        if resolved_string_field(&recipient, "preferredName")
            .is_some_and(|value| b2b_contains_html_tags(&value))
        {
            return vec![gift_card_user_error(
                root_field,
                json!([field_prefix, "recipientAttributes", "preferredName"]),
                Some("INVALID"),
                "Preferred name cannot contain HTML tags",
            )];
        }
        if resolved_string_field(&recipient, "message")
            .is_some_and(|value| b2b_contains_html_tags(&value))
        {
            return vec![gift_card_user_error(
                root_field,
                json!([field_prefix, "recipientAttributes", "message"]),
                Some("INVALID"),
                "Message cannot contain HTML tags",
            )];
        }
        if let Some(send_at) = resolved_string_field(&recipient, "sendNotificationAt") {
            let now = self.current_epoch_seconds();
            let max_send_at = now + GIFT_CARD_SEND_NOTIFICATION_WINDOW_DAYS * 86_400;
            match parse_rfc3339_epoch_seconds(&send_at) {
                Some(send_at) if send_at >= now && send_at <= max_send_at => {}
                _ => {
                    return vec![gift_card_user_error(
                        root_field,
                        json!([field_prefix, "recipientAttributes", "sendNotificationAt"]),
                        Some("INVALID"),
                        "Send notification at must be within 90 days from now",
                    )];
                }
            }
        }
        if let Some(recipient_id) = resolved_string_field(&recipient, "id") {
            if self
                .gift_card_customer_record_for_reference(request, &recipient_id)
                .is_none()
            {
                return vec![gift_card_user_error(
                    root_field,
                    json!([field_prefix, "recipientAttributes", "id"]),
                    Some("RECIPIENT_NOT_FOUND"),
                    "Recipient could not be found",
                )];
            }
        }
        Vec::new()
    }

    fn gift_card_customer_record_for_reference(
        &mut self,
        request: &Request,
        id: &str,
    ) -> Option<Value> {
        if let Some(customer) = self.store.staged.customers.get(id) {
            return Some(customer.clone());
        }
        let customer = self.hydrate_customer_for_mutation(request, id, false)?;
        self.store
            .staged
            .customers
            .stage(id.to_string(), customer.clone());
        Some(customer)
    }

    fn gift_card_recipient_attributes_json(
        &self,
        input: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let recipient_id = resolved_string_field(input, "id").unwrap_or_default();
        let recipient = self
            .store
            .staged
            .customers
            .get(&recipient_id)
            .map(gift_card_customer_projection_json)
            .unwrap_or_else(|| json!({ "id": recipient_id }));
        json!({
            "message": resolved_string_field(input, "message"),
            "preferredName": resolved_string_field(input, "preferredName"),
            "sendNotificationAt": resolved_string_field(input, "sendNotificationAt"),
            "recipient": recipient
        })
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
        let now = self.current_epoch_seconds();
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

fn gift_card_selection_needs_transactions(selection: &[SelectedField]) -> bool {
    selection_contains_any(selection, &["transactions"])
}

fn gift_card_payload_selection_needs_transactions(selection: &[SelectedField]) -> bool {
    selection.iter().any(|field| {
        field.name == "giftCard" && gift_card_selection_needs_transactions(&field.selection)
    })
}

fn gift_card_record_has_transactions(card: &Value) -> bool {
    card.get("transactions").is_some_and(Value::is_object)
}

fn gift_card_merge_transactions_if_missing(mut card: Value, hydrated: &Value) -> Value {
    if !gift_card_record_has_transactions(&card)
        && hydrated.get("transactions").is_some_and(Value::is_object)
    {
        card["transactions"] = hydrated["transactions"].clone();
    }
    card
}

fn preserve_upstream_selected_gift_card_transactions(
    projected: &mut Value,
    upstream: &Value,
    selection: &[SelectedField],
) {
    let Some(projected) = projected.as_object_mut() else {
        return;
    };
    let Some(upstream) = upstream.as_object() else {
        return;
    };
    for field in selection
        .iter()
        .filter(|field| field.name == "transactions")
    {
        if projected.contains_key(&field.response_key) {
            continue;
        }
        if let Some(value) = upstream.get(&field.response_key) {
            projected.insert(
                field.response_key.clone(),
                selected_json(value, &field.selection),
            );
        }
    }
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
    is_trial_shop: bool,
    root_field: &str,
    input: &BTreeMap<String, ResolvedValue>,
    field_prefix: &str,
) -> Vec<Value> {
    if is_trial_shop && resolved_string_field(input, "customerId").is_some() {
        return vec![gift_card_user_error(
            root_field,
            json!([field_prefix, "customerId"]),
            Some("INVALID"),
            "A trial shop cannot assign a customer to a gift card.",
        )];
    }
    if is_trial_shop
        && resolved_object_field(input, "recipientAttributes")
            .and_then(|recipient| resolved_string_field(&recipient, "id"))
            .is_some()
    {
        return vec![gift_card_user_error(
            root_field,
            json!([field_prefix, "recipientAttributes"]),
            Some("INVALID"),
            "A trial shop cannot assign a recipient to a gift card.",
        )];
    }
    Vec::new()
}

fn gift_card_processed_at_error(
    root_field: &str,
    input_name: &str,
    processed_at: &str,
    now_epoch_seconds: i64,
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
    if processed_at > now_epoch_seconds {
        return Some(gift_card_user_error(
            root_field,
            json!([input_name, "processedAt"]),
            Some("INVALID"),
            "The processed date must not be in the future.",
        ));
    }
    None
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
    if let Some(typename) = gift_card_user_error_typename(root_field) {
        return user_error_typed(typename, field, message, code);
    }
    user_error(field, message, code)
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

fn gift_card_template_suffix_json(value: Value) -> Value {
    let Some(template) = value.as_str() else {
        return value;
    };
    json!(template.strip_prefix("gift_card.").unwrap_or(template))
}

fn gift_card_is_deactivated(card: &Value) -> bool {
    card.get("enabled").and_then(Value::as_bool) == Some(false)
        || card
            .get("deactivatedAt")
            .is_some_and(|value| !value.is_null())
}

fn gift_card_currency(card: &Value, shop_currency_code: &str) -> String {
    card["balance"]["currencyCode"]
        .as_str()
        .or_else(|| card["initialValue"]["currencyCode"].as_str())
        .unwrap_or(shop_currency_code)
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

fn gift_card_search_decision(card: &Value, query: Option<&str>) -> StagedSearchDecision {
    StagedSearchDecision::from_bool(gift_card_matches_search_query(
        card,
        query.unwrap_or_default(),
    ))
}

fn gift_card_gid_tail_sort_value(card: &Value) -> StagedSortValue {
    resource_id_tail_sort_value(card.get("id").and_then(Value::as_str))
}

fn gift_card_string_sort_value(card: &Value, field: &str) -> StagedSortValue {
    card.get(field)
        .and_then(Value::as_str)
        .map(|value| StagedSortValue::String(value.to_ascii_lowercase()))
        .unwrap_or(StagedSortValue::Null)
}

fn gift_card_money_sort_value(card: &Value, field: &str) -> StagedSortValue {
    card.get(field)
        .and_then(|money| money.get("amount"))
        .and_then(Value::as_str)
        .and_then(|amount| amount.parse::<f64>().ok())
        .map(|amount| StagedSortValue::I64((amount * 1_000_000.0).round() as i64))
        .unwrap_or(StagedSortValue::Null)
}

fn gift_card_amount_spent_sort_value(card: &Value) -> StagedSortValue {
    let Some(initial) = card["initialValue"]["amount"]
        .as_str()
        .and_then(|amount| amount.parse::<f64>().ok())
    else {
        return StagedSortValue::Null;
    };
    let balance = card["balance"]["amount"]
        .as_str()
        .and_then(|amount| amount.parse::<f64>().ok())
        .unwrap_or(initial);
    StagedSortValue::I64(((initial - balance) * 1_000_000.0).round() as i64)
}

fn gift_card_code_sort_value(card: &Value) -> StagedSortValue {
    ["giftCardCode", "maskedCode", "lastCharacters"]
        .iter()
        .find_map(|field| card.get(*field).and_then(Value::as_str))
        .map(|value| StagedSortValue::String(value.to_ascii_lowercase()))
        .unwrap_or(StagedSortValue::Null)
}

fn gift_card_customer_name_sort_value(card: &Value) -> StagedSortValue {
    let customer = &card["customer"];
    ["displayName", "email", "id"]
        .iter()
        .find_map(|field| customer.get(*field).and_then(Value::as_str))
        .map(|value| StagedSortValue::String(value.to_ascii_lowercase()))
        .unwrap_or(StagedSortValue::Null)
}

fn gift_card_disabled_at_sort_value(card: &Value, reverse: bool) -> StagedSortValue {
    card.get("deactivatedAt")
        .and_then(Value::as_str)
        .map(|value| StagedSortValue::String(value.to_ascii_lowercase()))
        .unwrap_or_else(|| {
            if reverse {
                StagedSortValue::Null
            } else {
                StagedSortValue::String("~".to_string())
            }
        })
}

fn gift_card_staged_sort_key(card: &Value, sort_key: Option<&str>, reverse: bool) -> StagedSortKey {
    let primary = match sort_key.unwrap_or("ID") {
        "AMOUNT_SPENT" => gift_card_amount_spent_sort_value(card),
        "BALANCE" => gift_card_money_sort_value(card, "balance"),
        "CODE" => gift_card_code_sort_value(card),
        "CREATED_AT" => gift_card_string_sort_value(card, "createdAt"),
        "CUSTOMER_NAME" => gift_card_customer_name_sort_value(card),
        "DISABLED_AT" => gift_card_disabled_at_sort_value(card, reverse),
        "EXPIRES_ON" => gift_card_string_sort_value(card, "expiresOn"),
        "INITIAL_VALUE" => gift_card_money_sort_value(card, "initialValue"),
        "UPDATED_AT" => gift_card_string_sort_value(card, "updatedAt"),
        "ID" | "RELEVANCE" => gift_card_gid_tail_sort_value(card),
        _ => gift_card_gid_tail_sort_value(card),
    };
    vec![primary, gift_card_gid_tail_sort_value(card)]
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
    card.get("id")
        .and_then(Value::as_str)
        .is_some_and(|id| resource_id_matches_gid_or_tail(id, value))
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
    value
        .as_str()
        .is_some_and(|id| resource_id_matches_gid_or_tail(id, query_value))
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
    gift_card_matches_comparator_order(operator, actual.cmp(expected), actual == expected)
}

fn gift_card_matches_numeric_comparator(actual: Option<f64>, query_value: &str) -> bool {
    let Some(actual) = actual else {
        return false;
    };
    let (operator, expected) = gift_card_split_search_comparator(query_value);
    let expected = expected.parse::<f64>().ok().unwrap_or(actual);
    let Some(ordering) = actual.partial_cmp(&expected) else {
        return false;
    };
    gift_card_matches_comparator_order(operator, ordering, (actual - expected).abs() < f64::EPSILON)
}

fn gift_card_matches_comparator_order(operator: &str, ordering: Ordering, is_equal: bool) -> bool {
    match operator {
        ">=" => matches!(ordering, Ordering::Greater | Ordering::Equal),
        ">" => ordering == Ordering::Greater,
        "<=" => matches!(ordering, Ordering::Less | Ordering::Equal),
        "<" => ordering == Ordering::Less,
        _ => is_equal,
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

fn gift_card_has_no_recipient(card: &Value) -> bool {
    let Some(recipient_attributes) = card.get("recipientAttributes") else {
        return true;
    };
    if recipient_attributes.is_null() {
        return true;
    }
    let Some(recipient) = recipient_attributes.get("recipient") else {
        return true;
    };
    if recipient.is_null() {
        return true;
    }
    recipient
        .get("id")
        .and_then(Value::as_str)
        .is_none_or(str::is_empty)
}

fn gift_card_person_has_no_contact(person: &Value) -> bool {
    let has_contact_projection = person.get("email").is_some()
        || person.get("phone").is_some()
        || person.get("defaultEmailAddress").is_some()
        || person.get("defaultPhoneNumber").is_some();
    has_contact_projection
        && person["email"].is_null()
        && person["phone"].is_null()
        && person["defaultEmailAddress"]["emailAddress"].is_null()
        && person["defaultPhoneNumber"]["phoneNumber"].is_null()
}

fn gift_card_read_value_has_model_fields(card: &Value) -> bool {
    card.get("id").and_then(Value::as_str).is_some()
        && card.get("enabled").and_then(Value::as_bool).is_some()
        && card.get("initialValue").is_some_and(Value::is_object)
        && card.get("balance").is_some_and(Value::is_object)
}

fn gift_card_customer_projection_json(customer: &Value) -> Value {
    let id = customer.get("id").cloned().unwrap_or(Value::Null);
    json!({
        "id": id,
        "email": customer.get("email").cloned().unwrap_or(Value::Null),
        "phone": customer.get("phone").cloned().unwrap_or(Value::Null),
        "defaultEmailAddress": customer.get("defaultEmailAddress").cloned().unwrap_or(Value::Null),
        "defaultPhoneNumber": customer.get("defaultPhoneNumber").cloned().unwrap_or(Value::Null)
    })
}

fn format_gift_card_currency_limit(amount: f64) -> String {
    let rounded = format!("{amount:.2}");
    let Some((whole, cents)) = rounded.split_once('.') else {
        return format!("${rounded}");
    };
    let mut formatted = String::new();
    for (index, character) in whole.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            formatted.push(',');
        }
        formatted.push(character);
    }
    let whole = formatted.chars().rev().collect::<String>();
    format!("${whole}.{cents}")
}

pub(in crate::proxy) fn gift_card_payload_json(
    gift_card: &Value,
    selections: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    gift_card_payload_json_nullable(Some(gift_card), selections, user_errors)
}

pub(in crate::proxy) fn gift_card_transaction_payload(
    selections: &[SelectedField],
    transaction_field: &str,
    transaction: Option<Value>,
    user_errors: Vec<Value>,
) -> Value {
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        name if name == transaction_field => Some(match transaction.as_ref() {
            Some(transaction) => selected_abstract_json(transaction, &selection.selection),
            None => Value::Null,
        }),
        "userErrors" => selected_user_errors_field(user_errors.as_slice(), selection),
        _ => None,
    })
}

pub(in crate::proxy) fn gift_card_payload_json_nullable(
    gift_card: Option<&Value>,
    selections: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "giftCard" => Some(match gift_card {
            Some(card) => selected_json(card, &selection.selection),
            None => Value::Null,
        }),
        "giftCardCode" => Some(
            gift_card
                .and_then(|card| card.get("giftCardCode"))
                .cloned()
                .unwrap_or(Value::Null),
        ),
        "userErrors" => selected_user_errors_field(user_errors.as_slice(), selection),
        _ => None,
    })
}

fn gift_card_error_path(operation_path: Option<&str>, segments: Vec<Value>) -> Value {
    let mut path = Vec::new();
    if let Some(operation_path) = operation_path {
        path.push(json!(operation_path));
    }
    path.extend(segments);
    Value::Array(path)
}

fn gift_card_transaction_payload_selection_error(
    field: &RootFieldSelection,
    operation_path: Option<&str>,
) -> Option<Value> {
    let selected = field
        .selection
        .iter()
        .find(|selection| selection.name == "giftCard")?;
    let type_name = match field.name.as_str() {
        "giftCardCredit" => "GiftCardCreditPayload",
        "giftCardDebit" => "GiftCardDebitPayload",
        _ => return None,
    };
    Some(json!({
        "message": format!("Field 'giftCard' doesn't exist on type '{}'", type_name),
        "locations": graphql_locations(selected.location),
        "path": gift_card_error_path(
            operation_path,
            vec![
                json!(field.response_key.clone()),
                json!(selected.response_key.clone())
            ]
        ),
        "extensions": {
            "code": "undefinedField",
            "typeName": type_name,
            "fieldName": "giftCard"
        }
    }))
}

fn gift_card_missing_recipient_id_error(
    query: &str,
    field: &RootFieldSelection,
    operation_path: Option<&str>,
) -> Option<Value> {
    let input = resolved_object_field(&field.arguments, "input")?;
    let recipient = resolved_object_field(&input, "recipientAttributes")?;
    if recipient.contains_key("id") {
        return None;
    }
    match field.name.as_str() {
        "giftCardCreate" | "giftCardUpdate" => {}
        _ => return None,
    }
    let location =
        inline_input_field_name_location(query, field.location, 2, "recipientAttributes")
            .unwrap_or(field.location);
    Some(json!({
        "message": "Argument 'id' on InputObject 'GiftCardRecipientInput' is required. Expected type ID!",
        "locations": graphql_locations(location),
        "path": gift_card_error_path(
            operation_path,
            vec![
                json!(field.response_key.clone()),
                json!("input"),
                json!("recipientAttributes"),
                json!("id")
            ]
        ),
        "extensions": {
            "code": "missingRequiredInputObjectAttribute",
            "argumentName": "id",
            "argumentType": "ID!",
            "inputObjectType": "GiftCardRecipientInput"
        }
    }))
}

fn gift_card_invalid_recipient_id_response(field: &RootFieldSelection) -> Option<Value> {
    let input = resolved_object_field(&field.arguments, "input")?;
    let recipient = resolved_object_field(&input, "recipientAttributes")?;
    let recipient_id = resolved_string_field(&recipient, "id")?;
    if !gift_card_customer_gid_is_structurally_invalid(&recipient_id) {
        return None;
    }

    let mut data = serde_json::Map::new();
    data.insert(field.response_key.clone(), Value::Null);
    Some(json!({
        "data": Value::Object(data),
        "errors": [{
            "message": format!("Invalid id: {recipient_id}"),
            "locations": [{
                "line": field.location.line,
                "column": field.location.column
            }],
            "extensions": { "code": "RESOURCE_NOT_FOUND" },
            "path": [field.response_key.clone()]
        }]
    }))
}

fn gift_card_customer_gid_is_structurally_invalid(id: &str) -> bool {
    let Some(tail) = shopify_gid_tail_for_type(id, "Customer") else {
        return false;
    };
    let numeric_tail = tail.split('?').next().unwrap_or_default();
    numeric_tail.is_empty() || !numeric_tail.bytes().all(|byte| byte.is_ascii_digit())
}

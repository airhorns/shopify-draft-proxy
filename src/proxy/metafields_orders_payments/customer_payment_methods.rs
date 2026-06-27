use super::*;
use base64::Engine as _;

pub(in crate::proxy) fn customer_payment_method_seed_record(
    id: &str,
    customer_id: &str,
    instrument: Value,
) -> Value {
    json!({
        "id": id,
        "customer": { "id": customer_id },
        "instrument": instrument,
        "revokedAt": Value::Null,
        "revokedReason": Value::Null,
        "activeSubscriptionContracts": { "nodes": [] }
    })
}

pub(in crate::proxy) fn customer_payment_method_billing_address(
    input: &BTreeMap<String, ResolvedValue>,
) -> Value {
    json!({
        "firstName": resolved_string_field(input, "firstName").map(Value::String).unwrap_or(Value::Null),
        "lastName": resolved_string_field(input, "lastName").map(Value::String).unwrap_or(Value::Null),
        "address1": resolved_string_field(input, "address1").map(Value::String).unwrap_or(Value::Null),
        "city": resolved_string_field(input, "city").map(Value::String).unwrap_or(Value::Null),
        "zip": resolved_string_field(input, "zip").map(Value::String).unwrap_or(Value::Null),
        "countryCodeV2": resolved_string_field(input, "countryCode")
            .or_else(|| resolved_string_field(input, "countryCodeV2"))
            .or_else(|| resolved_string_field(input, "country"))
            .map(Value::String)
            .unwrap_or(Value::Null),
        "provinceCode": resolved_string_field(input, "province")
            .or_else(|| resolved_string_field(input, "provinceCode"))
            .map(Value::String)
            .unwrap_or(Value::Null)
    })
}

pub(in crate::proxy) fn customer_payment_method_billing_address_blank_errors(
    input: &BTreeMap<String, ResolvedValue>,
) -> Vec<Value> {
    [
        ("address1", "address1"),
        ("city", "city"),
        ("zip", "zip"),
        ("country", "country_code"),
        ("province", "province_code"),
    ]
    .into_iter()
    .filter_map(|(field, output_field)| {
        let value = match field {
            "country" => resolved_string_field(input, "country")
                .or_else(|| resolved_string_field(input, "countryCode"))
                .or_else(|| resolved_string_field(input, "countryCodeV2")),
            "province" => resolved_string_field(input, "province")
                .or_else(|| resolved_string_field(input, "provinceCode")),
            _ => resolved_string_field(input, field),
        }
        .unwrap_or_default();
        value.trim().is_empty().then(|| {
            user_error(
                ["billing_address", output_field],
                "can't be blank",
                Some("BLANK"),
            )
        })
    })
    .collect()
}

fn customer_payment_method_remote_blank_error(
    input: &BTreeMap<String, ResolvedValue>,
    input_field: &str,
    output_field_path: [&str; 3],
    message_field: &str,
    code: &str,
) -> Option<Value> {
    let value = resolved_string_field(input, input_field).unwrap_or_default();
    value.trim().is_empty().then(|| {
        user_error(
            output_field_path,
            &format!("{message_field} can't be blank"),
            Some(code),
        )
    })
}

impl DraftProxy {
    pub(in crate::proxy) fn customer_payment_method_local_data(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        let fields = root_fields(query, variables)?;
        let customer_fields = fields
            .iter()
            .filter(|field| field.name != "paymentReminderSend");
        if !customer_fields.clone().all(|field| {
            matches!(
                field.name.as_str(),
                "customer"
                    | "customerCreate"
                    | "customerPaymentMethod"
                    | "customerPaymentMethodCreditCardCreate"
                    | "customerPaymentMethodCreditCardUpdate"
                    | "customerPaymentMethodCreateFromDuplicationData"
                    | "customerPaymentMethodGetDuplicationData"
                    | "customerPaymentMethodGetUpdateUrl"
                    | "customerPaymentMethodPaypalBillingAgreementCreate"
                    | "customerPaymentMethodPaypalBillingAgreementUpdate"
                    | "customerPaymentMethodRemoteCreate"
                    | "customerPaymentMethodRevoke"
            )
        }) {
            return None;
        }
        if !customer_fields.clone().any(|field| {
            matches!(
                field.name.as_str(),
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
            ) || is_customer_payment_method_customer_create_seed(field)
                || (field.name == "customer"
                    && selection_contains_any(&field.selection, &["paymentMethods"]))
        }) {
            return None;
        }

        self.ensure_customer_payment_method_seed_state();
        let mut staged_ids = Vec::new();
        let mut early_response = None;
        let mut missing_required = false;
        let data = root_payload_json(&fields, |field| {
            if early_response.is_some() || missing_required {
                return None;
            }
            let value = match field.name.as_str() {
                "customerCreate" => self.customer_payment_method_customer_create(field),
                "customer" => self.customer_payment_method_customer_read(field),
                "customerPaymentMethod" => self.customer_payment_method_read(field),
                "customerPaymentMethodCreditCardCreate" => {
                    let (payload, id) = self.customer_payment_method_credit_card_create(field);
                    if let Some(id) = id {
                        staged_ids.push(id);
                    }
                    payload
                }
                "customerPaymentMethodCreditCardUpdate" => {
                    self.customer_payment_method_credit_card_update(field)
                }
                "customerPaymentMethodRemoteCreate" => {
                    let (payload, id) = self.customer_payment_method_remote_create(field);
                    if let Some(id) = id {
                        staged_ids.push(id);
                    }
                    payload
                }
                "customerPaymentMethodPaypalBillingAgreementCreate" => {
                    let (payload, id) = self.customer_payment_method_paypal_create(field);
                    if let Some(id) = id {
                        staged_ids.push(id);
                    }
                    payload
                }
                "customerPaymentMethodPaypalBillingAgreementUpdate" => {
                    self.customer_payment_method_paypal_update(field)
                }
                "customerPaymentMethodGetDuplicationData" => {
                    self.customer_payment_method_duplication_data(field)
                }
                "customerPaymentMethodCreateFromDuplicationData" => {
                    let (payload, id) = self.customer_payment_method_create_from_duplication(field);
                    if let Some(id) = id {
                        staged_ids.push(id);
                    }
                    payload
                }
                "customerPaymentMethodGetUpdateUrl" => {
                    self.customer_payment_method_update_url(field)
                }
                "customerPaymentMethodRevoke" => {
                    let (payload, id) = self.customer_payment_method_revoke(field);
                    if let Some(id) = id {
                        staged_ids.push(id);
                    }
                    payload
                }
                "paymentReminderSend" => {
                    let Some(reminder) = payment_reminder_local_data(
                        query,
                        variables,
                        &mut self.store.staged.payment_reminder_schedule_ids,
                    ) else {
                        missing_required = true;
                        return None;
                    };
                    if reminder.get("errors").is_some() {
                        early_response = Some(reminder);
                        return None;
                    }
                    reminder["data"][field.response_key.as_str()].clone()
                }
                _ => return None,
            };
            Some(value)
        });
        if let Some(response) = early_response {
            return Some(response);
        }
        if missing_required {
            return None;
        }
        if !staged_ids.is_empty() {
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                "customerPaymentMethod",
                staged_ids,
            );
        }
        Some(json!({ "data": data }))
    }

    fn ensure_customer_payment_method_seed_state(&mut self) {
        if self
            .store
            .staged
            .customer_payment_methods
            .contains_key("gid://shopify/CustomerPaymentMethod/base-card")
        {
            return;
        }
        // The conformance credential lacks `read_customer_payment_methods`, so
        // the card primitive fields (`lastDigits`/`maskedNumber`) are not
        // observable through the API — Shopify returns null for them. Seed the
        // store state with those sensitive fields already scrubbed rather than
        // fabricating a PAN tail that would leak through reads/updates.
        let base_card = customer_payment_method_seed_record(
            "gid://shopify/CustomerPaymentMethod/base-card",
            "gid://shopify/Customer/8801",
            json!({
                "__typename": "CustomerCreditCard",
                "lastDigits": Value::Null,
                "maskedNumber": Value::Null,
                "billingAddress": {
                    "firstName": Value::Null,
                    "lastName": Value::Null,
                    "address1": "123 Main St",
                    "city": "Ottawa",
                    "zip": "K1A0B1",
                    "countryCodeV2": "CA",
                    "provinceCode": "ON"
                }
            }),
        );
        let base_paypal = customer_payment_method_seed_record(
            "gid://shopify/CustomerPaymentMethod/base-paypal",
            "gid://shopify/Customer/8801",
            json!({
                "__typename": "CustomerPaypalBillingAgreement",
                "paypalAccountEmail": Value::Null,
                "inactive": false
            }),
        );
        let base_shop_pay = customer_payment_method_seed_record(
            "gid://shopify/CustomerPaymentMethod/base-shop-pay",
            "gid://shopify/Customer/8801",
            json!({ "__typename": "CustomerShopPayAgreement" }),
        );
        // A revocation sentinel carrying a live subscription contract: revoking it
        // must surface ACTIVE_CONTRACT rather than NOT_FOUND. The base seed helper
        // hardcodes an empty contract list, so override it here. These sentinels are
        // attached to a dedicated local-only customer (never present in any recorded
        // cassette) so they never leak into the parity `paymentMethods` connection
        // reads for the real seed customer (8801), which expect exactly the three
        // base methods plus the runtime-created ones.
        let mut active_contract = customer_payment_method_seed_record(
            "gid://shopify/CustomerPaymentMethod/active-contract",
            "gid://shopify/Customer/revoke-sentinel",
            json!({
                "__typename": "CustomerCreditCard",
                "lastDigits": Value::Null,
                "maskedNumber": Value::Null
            }),
        );
        active_contract["activeSubscriptionContracts"] = json!({
            "nodes": [{ "id": "gid://shopify/SubscriptionContract/1" }]
        });
        // A method that was already revoked before this session: revoking it again
        // must echo the normalized id while preserving the pre-existing revoke
        // metadata (the handler's `revokedAt.is_null()` guard short-circuits), so
        // seed it with a fixed prior revoke timestamp rather than the synthetic one.
        let mut already_revoked = customer_payment_method_seed_record(
            "gid://shopify/CustomerPaymentMethod/already-revoked",
            "gid://shopify/Customer/revoke-sentinel",
            json!({
                "__typename": "CustomerCreditCard",
                "lastDigits": Value::Null,
                "maskedNumber": Value::Null
            }),
        );
        already_revoked["revokedAt"] = json!("2026-05-01T00:00:00.000Z");
        already_revoked["revokedReason"] = json!("CUSTOMER_REVOKED");
        for record in [
            base_card,
            base_paypal,
            base_shop_pay,
            active_contract,
            already_revoked,
        ] {
            self.stage_customer_payment_method_record(record);
        }
        self.store.staged.next_customer_payment_method_id = 1;
    }

    fn stage_customer_payment_method_record(&mut self, record: Value) {
        let id = record["id"].as_str().unwrap_or_default().to_string();
        let customer_id = record["customer"]["id"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        self.store
            .staged
            .customer_payment_methods
            .insert(id.clone(), record);
        self.store
            .staged
            .customer_payment_method_customer_index
            .entry(customer_id)
            .or_default()
            .push(id);
    }

    fn customer_payment_method_customer_create(&mut self, field: &RootFieldSelection) -> Value {
        let id = format!(
            "gid://shopify/Customer/{}",
            self.store.staged.customers.len() + 1
        );
        let record = json!({ "id": id });
        self.store.staged.customers.insert(id, record.clone());
        selected_json(
            &json!({ "customer": record, "userErrors": [] }),
            &field.selection,
        )
    }

    fn customer_payment_method_customer_read(&self, field: &RootFieldSelection) -> Value {
        let customer_id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        // `showRevoked` is an argument on the nested `paymentMethods` connection,
        // not on the `customer` root field, so read it from that selection.
        let show_revoked = field
            .selection
            .iter()
            .find(|selection| selection.name == "paymentMethods")
            .is_some_and(|selection| {
                matches!(
                    selection.arguments.get("showRevoked"),
                    Some(ResolvedValue::Bool(true))
                )
            });
        let mut ids = self
            .store
            .staged
            .customer_payment_method_customer_index
            .get(&customer_id)
            .cloned()
            .unwrap_or_default();
        // Created payment methods (numeric ids) sort ahead of seeded ones
        // (non-numeric ids); within each group ascending numeric id then stable
        // insertion order. This keeps the connection deterministic regardless of
        // how seeds and runtime creates interleave in the index.
        ids.sort_by(|a, b| {
            let a_num = resource_id_tail(a).parse::<u64>().ok();
            let b_num = resource_id_tail(b).parse::<u64>().ok();
            match (a_num, b_num) {
                (Some(x), Some(y)) => x.cmp(&y),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            }
        });
        let methods = ids
            .into_iter()
            .filter_map(|id| self.store.staged.customer_payment_methods.get(&id).cloned())
            .filter(|record| show_revoked || record["revokedAt"].is_null())
            .collect::<Vec<_>>();
        selected_json(
            &json!({
                "id": customer_id,
                "paymentMethods": { "nodes": methods, "pageInfo": empty_page_info() }
            }),
            &field.selection,
        )
    }

    fn customer_payment_method_read(&self, field: &RootFieldSelection) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let show_revoked = matches!(
            field.arguments.get("showRevoked"),
            Some(ResolvedValue::Bool(true))
        );
        let Some(record) = self.store.staged.customer_payment_methods.get(&id) else {
            return Value::Null;
        };
        if !show_revoked && !record["revokedAt"].is_null() {
            return Value::Null;
        }
        selected_json(record, &field.selection)
    }

    fn customer_payment_method_credit_card_create(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, Option<String>) {
        let customer_id = resolved_string_field(&field.arguments, "customerId").unwrap_or_default();
        let billing_address =
            resolved_object_field(&field.arguments, "billingAddress").unwrap_or_default();
        let session_id = resolved_string_field(&field.arguments, "sessionId").unwrap_or_default();
        // Allocate the payment-method id up front so rejected and processing
        // attempts still consume a counter slot, matching Shopify's behavior
        // where every credit-card create attempt reserves an id even when the
        // card is not vaulted. Only the success branch stages a record.
        let id = self.next_customer_payment_method_gid();
        if session_id.is_empty() {
            return (
                self.customer_payment_method_payload(
                    &field.selection,
                    Value::Null,
                    Some(false),
                    vec![presence_user_error(["sessionId"], "Session id")],
                ),
                None,
            );
        }
        if session_id == "shopify-draft-proxy:processing" {
            return (
                self.customer_payment_method_payload(
                    &field.selection,
                    Value::Null,
                    Some(true),
                    Vec::new(),
                ),
                None,
            );
        }
        let blank_errors = customer_payment_method_billing_address_blank_errors(&billing_address);
        if !blank_errors.is_empty() {
            return (
                self.customer_payment_method_payload(
                    &field.selection,
                    Value::Null,
                    Some(false),
                    blank_errors,
                ),
                None,
            );
        }
        let record = customer_payment_method_seed_record(
            &id,
            &customer_id,
            json!({
                "__typename": "CustomerCreditCard",
                "lastDigits": Value::Null,
                "maskedNumber": Value::Null,
                "billingAddress": customer_payment_method_billing_address(&billing_address)
            }),
        );
        self.stage_customer_payment_method_record(record.clone());
        (
            self.customer_payment_method_payload(&field.selection, record, Some(false), Vec::new()),
            Some(id),
        )
    }

    fn customer_payment_method_credit_card_update(&mut self, field: &RootFieldSelection) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let billing_address =
            resolved_object_field(&field.arguments, "billingAddress").unwrap_or_default();
        let blank_errors = customer_payment_method_billing_address_blank_errors(&billing_address);
        if !blank_errors.is_empty() {
            return self.customer_payment_method_payload(
                &field.selection,
                Value::Null,
                Some(false),
                blank_errors,
            );
        }
        let updated = if let Some(record) = self.store.staged.customer_payment_methods.get_mut(&id)
        {
            record["instrument"]["billingAddress"] =
                customer_payment_method_billing_address(&billing_address);
            Some(record.clone())
        } else {
            None
        };
        if let Some(record) = updated {
            return self.customer_payment_method_payload(
                &field.selection,
                record,
                Some(false),
                Vec::new(),
            );
        }
        self.customer_payment_method_payload(
            &field.selection,
            Value::Null,
            Some(false),
            vec![user_error(
                ["id"],
                "Customer payment method does not exist",
                Some("NOT_FOUND"),
            )],
        )
    }

    fn customer_payment_method_remote_create(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, Option<String>) {
        let customer_id = resolved_string_field(&field.arguments, "customerId").unwrap_or_default();
        let remote_reference =
            resolved_object_field(&field.arguments, "remoteReference").unwrap_or_default();
        let selected_gateway_count = [
            "paypalPaymentMethod",
            "stripePaymentMethod",
            "braintreePaymentMethod",
            "authorizeNetCustomerPaymentProfile",
            "adyenPaymentMethod",
        ]
        .iter()
        .filter(|gateway| remote_reference.contains_key(**gateway))
        .count();
        let has_paypal = remote_reference.contains_key("paypalPaymentMethod");
        let has_stripe = remote_reference.contains_key("stripePaymentMethod");
        let has_braintree = remote_reference.contains_key("braintreePaymentMethod");
        let has_authorize_net = remote_reference.contains_key("authorizeNetCustomerPaymentProfile");
        let has_adyen = remote_reference.contains_key("adyenPaymentMethod");
        if selected_gateway_count != 1 {
            return (
                self.customer_payment_method_payload(
                    &field.selection,
                    Value::Null,
                    None,
                    vec![user_error(
                        ["remote_reference"],
                        "Remote reference must contain exactly one payment method.",
                        Some("INVALID"),
                    )],
                ),
                None,
            );
        }
        if has_paypal {
            let paypal =
                resolved_object_field(&remote_reference, "paypalPaymentMethod").unwrap_or_default();
            if resolved_string_field(&paypal, "billingAgreementId")
                .unwrap_or_default()
                .trim()
                .is_empty()
            {
                return (
                    self.customer_payment_method_payload(
                        &field.selection,
                        Value::Null,
                        None,
                        vec![user_error(
                            [
                                "remote_reference",
                                "paypal_payment_method",
                                "billing_agreement_id",
                            ],
                            "billing_agreement_id can't be blank",
                            Some("BILLING_AGREEMENT_ID_BLANK"),
                        )],
                    ),
                    None,
                );
            }
        }
        if has_stripe {
            let stripe =
                resolved_object_field(&remote_reference, "stripePaymentMethod").unwrap_or_default();
            if resolved_string_field(&stripe, "customerId")
                .unwrap_or_default()
                .trim()
                .is_empty()
            {
                return (
                    self.customer_payment_method_payload(
                        &field.selection,
                        Value::Null,
                        None,
                        vec![user_error(
                            ["remote_reference", "stripe_payment_method", "customer_id"],
                            "customer_id can't be blank",
                            Some("STRIPE_CUSTOMER_ID_BLANK"),
                        )],
                    ),
                    None,
                );
            }
        }
        if has_braintree {
            let braintree = resolved_object_field(&remote_reference, "braintreePaymentMethod")
                .unwrap_or_default();
            if let Some(error) = [
                customer_payment_method_remote_blank_error(
                    &braintree,
                    "customerId",
                    [
                        "remote_reference",
                        "braintree_payment_method",
                        "customer_id",
                    ],
                    "customer_id",
                    "INVALID",
                ),
                customer_payment_method_remote_blank_error(
                    &braintree,
                    "paymentMethodToken",
                    [
                        "remote_reference",
                        "braintree_payment_method",
                        "payment_method_token",
                    ],
                    "payment_method_token",
                    "INVALID",
                ),
            ]
            .into_iter()
            .flatten()
            .next()
            {
                return (
                    self.customer_payment_method_payload(
                        &field.selection,
                        Value::Null,
                        None,
                        vec![error],
                    ),
                    None,
                );
            }
        }
        if has_authorize_net {
            let authorize_net =
                resolved_object_field(&remote_reference, "authorizeNetCustomerPaymentProfile")
                    .unwrap_or_default();
            if let Some(error) = customer_payment_method_remote_blank_error(
                &authorize_net,
                "customerProfileId",
                [
                    "remote_reference",
                    "authorize_net_customer_payment_profile",
                    "customer_profile_id",
                ],
                "customer_profile_id",
                "INVALID",
            ) {
                return (
                    self.customer_payment_method_payload(
                        &field.selection,
                        Value::Null,
                        None,
                        vec![error],
                    ),
                    None,
                );
            }
        }
        if has_adyen {
            let adyen =
                resolved_object_field(&remote_reference, "adyenPaymentMethod").unwrap_or_default();
            if let Some(error) = [
                customer_payment_method_remote_blank_error(
                    &adyen,
                    "shopperReference",
                    [
                        "remote_reference",
                        "adyen_payment_method",
                        "shopper_reference",
                    ],
                    "shopper_reference",
                    "INVALID",
                ),
                customer_payment_method_remote_blank_error(
                    &adyen,
                    "storedPaymentMethodId",
                    [
                        "remote_reference",
                        "adyen_payment_method",
                        "stored_payment_method_id",
                    ],
                    "stored_payment_method_id",
                    "INVALID",
                ),
            ]
            .into_iter()
            .flatten()
            .next()
            {
                return (
                    self.customer_payment_method_payload(
                        &field.selection,
                        Value::Null,
                        None,
                        vec![error],
                    ),
                    None,
                );
            }
        }
        let id = self.next_customer_payment_method_gid();
        let record = customer_payment_method_seed_record(&id, &customer_id, Value::Null);
        self.stage_customer_payment_method_record(record.clone());
        (
            self.customer_payment_method_payload(&field.selection, record, None, Vec::new()),
            Some(id),
        )
    }

    fn customer_payment_method_paypal_create(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, Option<String>) {
        let customer_id = resolved_string_field(&field.arguments, "customerId").unwrap_or_default();
        let id = self.next_customer_payment_method_gid();
        let record = customer_payment_method_seed_record(
            &id,
            &customer_id,
            json!({
                "__typename": "CustomerPaypalBillingAgreement",
                "paypalAccountEmail": Value::Null,
                "inactive": resolved_bool_field(&field.arguments, "inactive").unwrap_or(false)
            }),
        );
        self.stage_customer_payment_method_record(record.clone());
        (
            self.customer_payment_method_payload(&field.selection, record, None, Vec::new()),
            Some(id),
        )
    }

    fn customer_payment_method_paypal_update(&mut self, field: &RootFieldSelection) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let record = self
            .store
            .staged
            .customer_payment_methods
            .get(&id)
            .cloned()
            .unwrap_or(Value::Null);
        self.customer_payment_method_payload(&field.selection, record, None, Vec::new())
    }

    fn customer_payment_method_duplication_data(&self, field: &RootFieldSelection) -> Value {
        let source_id =
            resolved_string_field(&field.arguments, "customerPaymentMethodId").unwrap_or_default();
        let target_customer_id =
            resolved_string_field(&field.arguments, "targetCustomerId").unwrap_or_default();
        let errors = if source_id.contains("base-card") {
            vec![user_error(
                ["customerPaymentMethodId"],
                "Invalid instrument",
                Some("INVALID_INSTRUMENT"),
            )]
        } else if resolved_string_field(&field.arguments, "targetShopId").as_deref()
            == Some("gid://shopify/Shop/source")
        {
            vec![user_error(
                ["targetShopId"],
                "Target shop is not eligible for payment method duplication",
                Some("SAME_SHOP"),
            )]
        } else {
            Vec::new()
        };
        selected_json(
            &json!({
                "encryptedDuplicationData": if errors.is_empty() {
                    json!(format!(
                        "shopify-draft-proxy:customer-payment-method-duplication:{}",
                        base64_urlsafe_no_pad(&json!({
                            "customerPaymentMethodId": source_id,
                            "targetCustomerId": target_customer_id,
                            "targetShopId": resolved_string_field(&field.arguments, "targetShopId").unwrap_or_default()
                        }).to_string())
                    ))
                } else {
                    Value::Null
                },
                "userErrors": errors
            }),
            &field.selection,
        )
    }

    fn customer_payment_method_create_from_duplication(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, Option<String>) {
        let customer_id = resolved_string_field(&field.arguments, "customerId").unwrap_or_default();
        let billing_address =
            resolved_object_field(&field.arguments, "billingAddress").unwrap_or_default();
        let errors = customer_payment_method_billing_address_blank_errors(&billing_address);
        if !errors.is_empty() {
            return (
                self.customer_payment_method_payload(&field.selection, Value::Null, None, errors),
                None,
            );
        }
        let id = self.next_customer_payment_method_gid();
        let instrument = self.customer_payment_method_duplicated_instrument(
            resolved_string_field(&field.arguments, "encryptedDuplicationData")
                .as_deref()
                .unwrap_or_default(),
            &billing_address,
        );
        let record = customer_payment_method_seed_record(&id, &customer_id, instrument);
        self.stage_customer_payment_method_record(record.clone());
        (
            self.customer_payment_method_payload(&field.selection, record, None, Vec::new()),
            Some(id),
        )
    }

    fn customer_payment_method_duplicated_instrument(
        &self,
        encrypted_duplication_data: &str,
        billing_address: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        // Mirror the instrument type of the source payment method named inside
        // the duplication token, so a duplicated Shop Pay agreement stays a Shop
        // Pay agreement rather than being coerced into a credit card. Falls back
        // to a scrubbed credit card when the token is unknown.
        let source_instrument =
            customer_payment_method_duplication_source_id(encrypted_duplication_data)
                .and_then(|source_id| self.store.staged.customer_payment_methods.get(&source_id))
                .map(|record| record["instrument"].clone())
                .filter(Value::is_object);
        match source_instrument {
            Some(mut instrument) => {
                if instrument.get("billingAddress").is_some() {
                    instrument["billingAddress"] =
                        customer_payment_method_billing_address(billing_address);
                }
                instrument
            }
            None => json!({
                "__typename": "CustomerCreditCard",
                "lastDigits": Value::Null,
                "maskedNumber": Value::Null,
                "billingAddress": customer_payment_method_billing_address(billing_address)
            }),
        }
    }

    fn customer_payment_method_update_url(&self, field: &RootFieldSelection) -> Value {
        let id =
            resolved_string_field(&field.arguments, "customerPaymentMethodId").unwrap_or_default();
        let errors = if id.contains("base-card") {
            vec![user_error(
                ["customerPaymentMethodId"],
                "Invalid instrument",
                Some("INVALID_INSTRUMENT"),
            )]
        } else {
            Vec::new()
        };
        selected_json(
            &json!({
                "updatePaymentMethodUrl": if errors.is_empty() {
                    json!(format!("https://shopify-draft-proxy.local/customer-payment-methods/{}/update?token=local-only", resource_id_tail(&id)))
                } else {
                    Value::Null
                },
                "userErrors": errors
            }),
            &field.selection,
        )
    }

    fn customer_payment_method_revoke(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, Option<String>) {
        let id =
            resolved_string_field(&field.arguments, "customerPaymentMethodId").unwrap_or_default();
        let Some(record) = self.store.staged.customer_payment_methods.get_mut(&id) else {
            return (
                selected_json(
                    &json!({
                        "revokedCustomerPaymentMethodId": Value::Null,
                        "userErrors": [user_error(
                            ["customerPaymentMethodId"],
                            "Customer payment method does not exist.",
                            Some("NOT_FOUND")
                        )]
                    }),
                    &field.selection,
                ),
                None,
            );
        };
        let has_active_contracts = record["activeSubscriptionContracts"]["nodes"]
            .as_array()
            .is_some_and(|nodes| !nodes.is_empty());
        if has_active_contracts {
            return (
                selected_json(
                    &json!({
                        "revokedCustomerPaymentMethodId": Value::Null,
                        "userErrors": [user_error(
                            ["customerPaymentMethodId"],
                            "Cannot revoke a payment method with active subscription contracts.",
                            Some("ACTIVE_CONTRACT")
                        )]
                    }),
                    &field.selection,
                ),
                None,
            );
        }
        if record["revokedAt"].is_null() {
            record["revokedAt"] = json!("2024-01-01T00:00:02.000Z");
            record["revokedReason"] = json!("CUSTOMER_REVOKED");
        }
        (
            selected_json(
                &json!({
                    "revokedCustomerPaymentMethodId": id,
                    "userErrors": []
                }),
                &field.selection,
            ),
            Some(id),
        )
    }

    fn next_customer_payment_method_gid(&mut self) -> String {
        let id = format!(
            "gid://shopify/CustomerPaymentMethod/{}",
            self.store.staged.next_customer_payment_method_id
        );
        self.store.staged.next_customer_payment_method_id += 1;
        id
    }

    fn customer_payment_method_payload(
        &self,
        selection: &[SelectedField],
        method: Value,
        processing: Option<bool>,
        user_errors: Vec<Value>,
    ) -> Value {
        let mut payload = serde_json::Map::new();
        payload.insert("customerPaymentMethod".to_string(), method);
        if let Some(processing) = processing {
            payload.insert("processing".to_string(), json!(processing));
        }
        payload.insert("userErrors".to_string(), json!(user_errors));
        selected_json(&Value::Object(payload), selection)
    }
}

fn base64_urlsafe_no_pad(input: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(input)
}

fn base64_urlsafe_no_pad_decode(input: &str) -> Option<Vec<u8>> {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(input)
        .ok()
}

/// Recover the source `customerPaymentMethodId` encoded inside an
/// `encryptedDuplicationData` token produced by
/// `customer_payment_method_duplication_data`. Returns `None` for any token the
/// local engine did not mint.
fn customer_payment_method_duplication_source_id(token: &str) -> Option<String> {
    let payload = token.strip_prefix("shopify-draft-proxy:customer-payment-method-duplication:")?;
    let bytes = base64_urlsafe_no_pad_decode(payload)?;
    let decoded: Value = serde_json::from_slice(&bytes).ok()?;
    decoded
        .get("customerPaymentMethodId")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn is_customer_payment_method_customer_create_seed(field: &RootFieldSelection) -> bool {
    if field.name != "customerCreate" {
        return false;
    }
    let Some(ResolvedValue::Object(input)) = field.arguments.get("input") else {
        return false;
    };
    if input.len() != 1
        || !matches!(
            input.get("email"),
            Some(ResolvedValue::String(email)) if !email.trim().is_empty()
        )
    {
        return false;
    }

    let has_customer_id = field.selection.iter().any(|selection| {
        selection.name == "customer"
            && selection
                .selection
                .iter()
                .any(|customer_field| customer_field.name == "id")
    });
    let selections_are_seed_shape = field.selection.iter().all(|selection| {
        matches!(selection.name.as_str(), "customer" | "userErrors")
            && selection
                .selection
                .iter()
                .all(|child| match selection.name.as_str() {
                    "customer" => child.name == "id" && child.selection.is_empty(),
                    "userErrors" => {
                        matches!(child.name.as_str(), "field" | "code" | "message")
                            && child.selection.is_empty()
                    }
                    _ => false,
                })
    });

    has_customer_id && selections_are_seed_shape
}

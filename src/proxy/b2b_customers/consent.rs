use super::*;

/// Hydration query for the store-wide `customersCount` baseline used by the
/// `customer*TaxExemptions` / marketing-consent downstream reads in LiveHybrid
/// mode. Mirrors the per-resource hydrate queries; the count is cached into
/// `customers_count_base` so subsequent reads track deletions generically.
const CUSTOMER_COUNT_HYDRATE_QUERY: &str =
    include_str!("../../../config/parity-requests/customers/customer-count-hydrate.graphql");

impl DraftProxy {
    /// `customerAddTaxExemptions` / `customerRemoveTaxExemptions` /
    /// `customerReplaceTaxExemptions`: stage the resulting tax-exemption set onto
    /// the staged (or hydrated) customer and return its canonical payload.
    pub(crate) fn customer_tax_exemptions_mutation_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        if let Some(error) = customer_tax_exemptions_invalid_enum_error(
            invocation.query,
            invocation.root_name,
            invocation.root_location,
            &invocation.raw_arguments,
        ) {
            return graphql_error_outcome(vec![error], invocation.response_key);
        }
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        let (payload, staged_id) = self.customer_tax_exemptions_field_payload(
            invocation.root_name,
            &arguments,
            invocation.request,
        );
        let outcome = ResolverOutcome::value(payload);
        staged_id.map_or(outcome.clone(), |id| {
            outcome.with_log_draft(LogDraft::staged(
                invocation.root_name,
                "customers",
                vec![id],
            ))
        })
    }

    fn customer_tax_exemptions_field_payload(
        &mut self,
        root_name: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> (Value, Option<String>) {
        let customer_id = resolved_string_field(arguments, "customerId").unwrap_or_default();
        if customer_id.is_empty() || self.store.staged.customers.is_tombstoned(&customer_id) {
            return (
                customer_tax_exemptions_payload(
                    Value::Null,
                    vec![customer_tax_exemptions_user_error()],
                ),
                None,
            );
        }
        if !self.store.staged.customers.contains_key(&customer_id) {
            self.taggable_resource_staged_or_hydrated("Customer", &customer_id, request);
        }
        if !self.store.staged.customers.contains_key(&customer_id) {
            return (
                customer_tax_exemptions_payload(
                    Value::Null,
                    vec![customer_tax_exemptions_user_error()],
                ),
                None,
            );
        }

        let tax_exemptions =
            normalize_customer_tax_exemptions(list_string_field(arguments, "taxExemptions"));
        let mut customer = self
            .store
            .staged
            .customers
            .get(&customer_id)
            .cloned()
            .unwrap_or(Value::Null);
        let existing = customer_tax_exemptions(&customer);
        let next = match root_name {
            "customerAddTaxExemptions" => add_customer_tax_exemptions(existing, tax_exemptions),
            "customerRemoveTaxExemptions" => {
                remove_customer_tax_exemptions(existing, tax_exemptions)
            }
            "customerReplaceTaxExemptions" => tax_exemptions,
            _ => existing,
        };
        customer["taxExemptions"] = json!(next);
        customer["updatedAt"] = json!(self.next_product_timestamp());
        self.store
            .staged
            .customers
            .insert(customer_id.clone(), customer.clone());

        (
            customer_tax_exemptions_payload(customer, Vec::new()),
            Some(customer_id),
        )
    }

    /// In LiveHybrid mode, hydrate the store-wide `customersCount` baseline from
    /// upstream once (cached into `customers_count_base`) so a downstream
    /// `customersCount` read served from the staged overlay reports the live
    /// total. No-op in Snapshot mode or when the baseline is already known.
    pub(in crate::proxy) fn hydrate_customers_count_for_overlay_read(&mut self, request: &Request) {
        if self.config.read_mode != ReadMode::LiveHybrid
            || self.store.staged.customers_count_base.is_some()
        {
            return;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": CUSTOMER_COUNT_HYDRATE_QUERY,
                "operationName": "CustomerCountHydrate",
                "variables": {},
            }),
        );
        if !(200..300).contains(&response.status) {
            return;
        }
        if let Some(count) = response.body["data"]["customersCount"]["count"].as_u64() {
            self.store.staged.customers_count_base = Some(count);
        }
    }

    /// `customerEmailMarketingConsentUpdate` / `customerSmsMarketingConsentUpdate`:
    /// apply the resolved consent state onto the staged (or hydrated) customer and
    /// project the requested selection, mirroring Shopify's resolver-error shapes.
    pub(crate) fn customer_marketing_consent_update_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        if invocation.root_name == "customerSmsMarketingConsentUpdate" {
            if let Some(error) =
                customer_sms_consent_invalid_enum_error(invocation.query, &invocation.raw_arguments)
            {
                return graphql_error_outcome(vec![error], invocation.response_key);
            }
        }
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        let field_outcome = self.customer_marketing_consent_update_field(
            invocation.root_name,
            &arguments,
            invocation.request,
            invocation.root_location,
            invocation.response_key,
        );
        let value = if field_outcome.top_level_error.is_some() {
            Value::Null
        } else {
            field_outcome.payload
        };
        let mut outcome = ResolverOutcome::value(value);
        if let Some(error) = field_outcome.top_level_error {
            outcome.errors = root_field_errors_from_json(&[error], invocation.response_key);
        }
        outcome.log_drafts.push(field_outcome.log_draft);
        outcome
    }

    fn customer_marketing_consent_update_field(
        &mut self,
        root_name: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
        request: &Request,
        root_location: SourceLocation,
        response_key: &str,
    ) -> CustomerConsentOutcome {
        let is_email = root_name == "customerEmailMarketingConsentUpdate";
        let consent_key = if is_email {
            "emailMarketingConsent"
        } else {
            "smsMarketingConsent"
        };
        let input = resolved_object_field(arguments, "input").unwrap_or_default();
        let customer_id = resolved_string_field(&input, "customerId").unwrap_or_default();
        let consent = resolved_object_field(&input, consent_key).unwrap_or_default();
        let marketing_state = resolved_string_field(&consent, "marketingState").unwrap_or_default();

        if matches!(marketing_state.as_str(), "NOT_SUBSCRIBED" | "REDACTED")
            || (is_email && marketing_state == "INVALID")
        {
            return CustomerConsentOutcome::failed(
                root_name,
                Value::Null,
                Some(customer_consent_invalid_state_error(
                    root_location,
                    response_key,
                    &marketing_state,
                )),
            );
        }

        let Some(existing_customer) =
            self.taggable_resource_staged_or_hydrated("Customer", &customer_id, request)
        else {
            let user_error = if is_email {
                user_error(
                    ["input", "customerId"],
                    "Customer not found",
                    Some("INVALID"),
                )
            } else {
                user_error(Value::Null, "Customer not found", None)
            };
            return CustomerConsentOutcome::failed(
                root_name,
                customer_consent_payload(Value::Null, vec![user_error]),
                None,
            );
        };

        let marketing_opt_in_level_input = resolved_string_field(&consent, "marketingOptInLevel");
        if marketing_state == "SUBSCRIBED" && marketing_opt_in_level_input.is_none() {
            let customer = if is_email {
                existing_customer.clone()
            } else {
                Value::Null
            };
            return CustomerConsentOutcome::failed(
                root_name,
                customer_consent_payload(
                    customer,
                    vec![customer_consent_user_error(
                        vec!["input", consent_key, "marketingOptInLevel"],
                        "Marketing opt in level must exist",
                        "MISSING_ARGUMENT",
                    )],
                ),
                None,
            );
        }
        let marketing_opt_in_level = marketing_opt_in_level_input
            .unwrap_or_else(|| current_consent_opt_in_level(&existing_customer, is_email));
        let consent_updated_at = resolved_string_field(&consent, "consentUpdatedAt");

        if let Some(consent_updated_at) = consent_updated_at.as_deref() {
            if customer_consent_updated_at_is_future(consent_updated_at) {
                let customer = if is_email {
                    existing_customer.clone()
                } else {
                    Value::Null
                };
                return CustomerConsentOutcome::failed(
                    root_name,
                    customer_consent_payload(
                        customer,
                        vec![customer_consent_user_error(
                            vec!["input", consent_key, "consentUpdatedAt"],
                            "Consent updated at must not be in the future",
                            "INVALID",
                        )],
                    ),
                    None,
                );
            }
        }

        if marketing_state == "PENDING" && marketing_opt_in_level != "CONFIRMED_OPT_IN" {
            let customer = if is_email {
                existing_customer.clone()
            } else {
                Value::Null
            };
            return CustomerConsentOutcome::failed(
                root_name,
                customer_consent_payload(
                    customer,
                    vec![customer_consent_user_error(
                        vec!["input", consent_key, "marketingOptInLevel"],
                        "Marketing opt in level must be confirmed opt-in for pending consent state",
                        "INVALID",
                    )],
                ),
                None,
            );
        }

        if !is_email && !customer_has_default_phone(&existing_customer) {
            return CustomerConsentOutcome::failed(
                root_name,
                customer_consent_payload(
                    Value::Null,
                    vec![customer_consent_user_error(
                        vec!["input", "smsMarketingConsent"],
                        "A phone number is required to set the SMS consent state.",
                        "INVALID",
                    )],
                ),
                None,
            );
        }

        if is_email && !customer_has_default_email(&existing_customer) {
            return CustomerConsentOutcome::staged(
                root_name,
                customer_consent_payload(existing_customer, Vec::new()),
                vec![customer_id],
            );
        }

        let updated_at = consent_updated_at
            .or_else(|| current_consent_updated_at(&existing_customer, is_email))
            .unwrap_or_else(|| self.next_product_timestamp());
        let mut customer = existing_customer;
        apply_customer_marketing_consent(
            &mut customer,
            is_email,
            &marketing_state,
            &marketing_opt_in_level,
            Some(updated_at.as_str()),
        );
        self.store
            .staged
            .customers
            .insert(customer_id.clone(), customer.clone());
        CustomerConsentOutcome::staged(
            root_name,
            customer_consent_payload(customer, Vec::new()),
            vec![customer_id],
        )
    }
}

/// Validates the `taxExemptions` argument of the `customer*TaxExemptions`
/// mutations before any staging, mirroring Shopify's enum coercion errors:
/// invalid literals raise `argumentLiteralsIncompatible`, invalid variable
/// values raise `INVALID_VARIABLE`. Returns `None` when every value is known.
pub(in crate::proxy) fn customer_tax_exemptions_invalid_enum_error(
    query: &str,
    root_name: &str,
    root_location: SourceLocation,
    raw_arguments: &BTreeMap<String, RawArgumentValue>,
) -> Option<Value> {
    let raw_value = raw_arguments.get("taxExemptions")?;
    if let Some(literal) = raw_tax_exemption_literal(raw_value) {
        return Some(tax_exemption_invalid_literal_error(
            query,
            root_name,
            root_location,
            "taxExemptions",
            literal,
            "[TaxExemption!]!",
        ));
    }
    if let Some(invalid) = tax_exemption_invalid_variable(raw_value) {
        return Some(tax_exemption_invalid_variable_error(query, &invalid));
    }
    None
}

fn customer_tax_exemptions_payload(customer: Value, user_errors: Vec<Value>) -> Value {
    json!({
        "customer": customer,
        "userErrors": user_errors
    })
}

fn customer_tax_exemptions_user_error() -> Value {
    user_error_omit_code(["customerId"], "Customer does not exist.", None)
}

fn customer_tax_exemptions(customer: &Value) -> Vec<String> {
    customer
        .get("taxExemptions")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn normalize_customer_tax_exemptions(exemptions: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut normalized = Vec::new();
    for exemption in exemptions {
        if seen.insert(exemption.clone()) {
            normalized.push(exemption);
        }
    }
    normalized
}

fn add_customer_tax_exemptions(existing: Vec<String>, additions: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut merged = Vec::new();
    for exemption in existing.into_iter().chain(additions) {
        if seen.insert(exemption.clone()) {
            merged.push(exemption);
        }
    }
    merged
}

fn remove_customer_tax_exemptions(existing: Vec<String>, removals: Vec<String>) -> Vec<String> {
    let removals = removals.into_iter().collect::<BTreeSet<_>>();
    existing
        .into_iter()
        .filter(|exemption| !removals.contains(exemption))
        .collect()
}

/// Outcome of a single `customer*MarketingConsentUpdate` root field: either a
/// projected payload (with field-level `userErrors`) or a top-level GraphQL
/// error (Shopify raises these for disallowed marketing states).
struct CustomerConsentOutcome {
    payload: Value,
    top_level_error: Option<Value>,
    log_draft: LogDraft,
}

impl CustomerConsentOutcome {
    fn failed(root_field: &str, payload: Value, top_level_error: Option<Value>) -> Self {
        Self {
            payload,
            top_level_error,
            log_draft: LogDraft::failed(
                root_field,
                "customers",
                "Customer consent mutation rejected by local validation.",
            ),
        }
    }

    fn staged(root_field: &str, payload: Value, staged_ids: Vec<String>) -> Self {
        Self {
            payload,
            top_level_error: None,
            log_draft: LogDraft::staged(root_field, "customers", staged_ids),
        }
    }
}

fn customer_consent_payload(customer: Value, user_errors: Vec<Value>) -> Value {
    json!({
        "customer": customer,
        "userErrors": user_errors
    })
}

fn customer_consent_user_error(field: Vec<&str>, message: &str, code: &str) -> Value {
    user_error(field, message, Some(code))
}

fn customer_consent_invalid_state_error(
    location: SourceLocation,
    response_key: &str,
    state: &str,
) -> Value {
    json!({
        "message": format!("Cannot specify {state} as a marketing state input"),
        "locations": [{ "line": location.line, "column": location.column }],
        "extensions": { "code": "INVALID" },
        "path": [response_key]
    })
}

fn customer_has_default_email(customer: &Value) -> bool {
    customer
        .get("defaultEmailAddress")
        .and_then(|contact| contact.get("emailAddress"))
        .and_then(Value::as_str)
        .is_some_and(|email| !email.trim().is_empty())
}

fn customer_has_default_phone(customer: &Value) -> bool {
    customer
        .get("defaultPhoneNumber")
        .and_then(|contact| contact.get("phoneNumber"))
        .and_then(Value::as_str)
        .is_some_and(|phone| !phone.trim().is_empty())
}

fn current_consent_opt_in_level(customer: &Value, is_email: bool) -> String {
    let contact_key = if is_email {
        "defaultEmailAddress"
    } else {
        "defaultPhoneNumber"
    };
    customer
        .get(contact_key)
        .and_then(|contact| contact.get("marketingOptInLevel"))
        .and_then(Value::as_str)
        .unwrap_or("SINGLE_OPT_IN")
        .to_string()
}

fn current_consent_updated_at(customer: &Value, is_email: bool) -> Option<String> {
    let contact_key = if is_email {
        "defaultEmailAddress"
    } else {
        "defaultPhoneNumber"
    };
    customer
        .get(contact_key)
        .and_then(|contact| contact.get("marketingUpdatedAt"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn customer_consent_updated_at_is_future(value: &str) -> bool {
    let Some(updated_at) = parse_rfc3339_epoch_seconds(value) else {
        return false;
    };
    let Ok(now) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) else {
        return false;
    };
    updated_at > now.as_secs() as i64
}

const TAX_EXEMPTION_VALUES: &[&str] = &[
    "CA_STATUS_CARD_EXEMPTION",
    "CA_BC_RESELLER_EXEMPTION",
    "CA_MB_RESELLER_EXEMPTION",
    "CA_SK_RESELLER_EXEMPTION",
    "CA_SK_VPT_RESELLER_EXEMPTION",
    "CA_NL_VPT_RESELLER_EXEMPTION",
    "CA_DIPLOMAT_EXEMPTION",
    "CA_BC_COMMERCIAL_FISHERY_EXEMPTION",
    "CA_MB_COMMERCIAL_FISHERY_EXEMPTION",
    "CA_NS_COMMERCIAL_FISHERY_EXEMPTION",
    "CA_PE_COMMERCIAL_FISHERY_EXEMPTION",
    "CA_SK_COMMERCIAL_FISHERY_EXEMPTION",
    "CA_BC_PRODUCTION_AND_MACHINERY_EXEMPTION",
    "CA_SK_PRODUCTION_AND_MACHINERY_EXEMPTION",
    "CA_BC_SUB_CONTRACTOR_EXEMPTION",
    "CA_SK_SUB_CONTRACTOR_EXEMPTION",
    "CA_BC_CONTRACTOR_EXEMPTION",
    "CA_SK_CONTRACTOR_EXEMPTION",
    "CA_ON_PURCHASE_EXEMPTION",
    "CA_MB_FARMER_EXEMPTION",
    "CA_NS_FARMER_EXEMPTION",
    "CA_SK_FARMER_EXEMPTION",
    "EU_REVERSE_CHARGE_EXEMPTION_RULE",
    "US_AL_RESELLER_EXEMPTION",
    "US_AK_RESELLER_EXEMPTION",
    "US_AZ_RESELLER_EXEMPTION",
    "US_AR_RESELLER_EXEMPTION",
    "US_CA_RESELLER_EXEMPTION",
    "US_CO_RESELLER_EXEMPTION",
    "US_CT_RESELLER_EXEMPTION",
    "US_DE_RESELLER_EXEMPTION",
    "US_FL_RESELLER_EXEMPTION",
    "US_GA_RESELLER_EXEMPTION",
    "US_HI_RESELLER_EXEMPTION",
    "US_ID_RESELLER_EXEMPTION",
    "US_IL_RESELLER_EXEMPTION",
    "US_IN_RESELLER_EXEMPTION",
    "US_IA_RESELLER_EXEMPTION",
    "US_KS_RESELLER_EXEMPTION",
    "US_KY_RESELLER_EXEMPTION",
    "US_LA_RESELLER_EXEMPTION",
    "US_ME_RESELLER_EXEMPTION",
    "US_MD_RESELLER_EXEMPTION",
    "US_MA_RESELLER_EXEMPTION",
    "US_MI_RESELLER_EXEMPTION",
    "US_MN_RESELLER_EXEMPTION",
    "US_MS_RESELLER_EXEMPTION",
    "US_MO_RESELLER_EXEMPTION",
    "US_MT_RESELLER_EXEMPTION",
    "US_NE_RESELLER_EXEMPTION",
    "US_NV_RESELLER_EXEMPTION",
    "US_NH_RESELLER_EXEMPTION",
    "US_NJ_RESELLER_EXEMPTION",
    "US_NM_RESELLER_EXEMPTION",
    "US_NY_RESELLER_EXEMPTION",
    "US_NC_RESELLER_EXEMPTION",
    "US_ND_RESELLER_EXEMPTION",
    "US_OH_RESELLER_EXEMPTION",
    "US_OK_RESELLER_EXEMPTION",
    "US_OR_RESELLER_EXEMPTION",
    "US_PA_RESELLER_EXEMPTION",
    "US_RI_RESELLER_EXEMPTION",
    "US_SC_RESELLER_EXEMPTION",
    "US_SD_RESELLER_EXEMPTION",
    "US_TN_RESELLER_EXEMPTION",
    "US_TX_RESELLER_EXEMPTION",
    "US_UT_RESELLER_EXEMPTION",
    "US_VT_RESELLER_EXEMPTION",
    "US_VA_RESELLER_EXEMPTION",
    "US_WA_RESELLER_EXEMPTION",
    "US_WV_RESELLER_EXEMPTION",
    "US_WI_RESELLER_EXEMPTION",
    "US_WY_RESELLER_EXEMPTION",
    "US_DC_RESELLER_EXEMPTION",
];

/// An invalid `[TaxExemption!]` variable value detected during request validation.
struct InvalidTaxExemptionVariable {
    variable_name: String,
    /// The full provided value, echoed back in `extensions.value`.
    provided: Value,
    /// `(list index, invalid value)` for every element that is not a known exemption.
    problems: Vec<(usize, String)>,
}

pub(in crate::proxy) fn b2b_tax_settings_invalid_enum_error(
    query: &str,
    root_name: &str,
    root_location: SourceLocation,
    raw_arguments: &BTreeMap<String, RawArgumentValue>,
) -> Option<Value> {
    for argument_name in ["exemptionsToAssign", "exemptionsToRemove"] {
        let Some(raw_value) = raw_arguments.get(argument_name) else {
            continue;
        };
        if let Some(literal) = raw_tax_exemption_literal(raw_value) {
            return Some(tax_exemption_invalid_literal_error(
                query,
                root_name,
                root_location,
                argument_name,
                literal,
                "[TaxExemption!]",
            ));
        }
        if let Some(invalid) = tax_exemption_invalid_variable(raw_value) {
            return Some(tax_exemption_invalid_variable_error(query, &invalid));
        }
    }
    None
}

fn raw_tax_exemption_literal(value: &RawArgumentValue) -> Option<&str> {
    match value {
        RawArgumentValue::Enum(value) if !is_known_tax_exemption(value) => Some(value.as_str()),
        RawArgumentValue::List(values) => values.iter().find_map(raw_tax_exemption_literal),
        _ => None,
    }
}

fn tax_exemption_invalid_literal_error(
    query: &str,
    root_name: &str,
    root_location: SourceLocation,
    argument_name: &str,
    literal: &str,
    expected_type: &str,
) -> Value {
    json!({
        "message": tax_exemption_invalid_literal_message(
            argument_name,
            root_name,
            literal,
            expected_type,
        ),
        "locations": [{
            "line": root_location.line,
            "column": root_location.column,
        }],
        "extensions": {
            "code": "argumentLiteralsIncompatible",
            "typeName": "Field",
            "argumentName": argument_name,
        },
        "path": tax_exemption_invalid_literal_path(query, root_name, argument_name),
    })
}

fn tax_exemption_invalid_literal_message(
    argument_name: &str,
    field_name: &str,
    literal: &str,
    expected_type: &str,
) -> String {
    format!(
        "Argument '{argument_name}' on Field '{field_name}' has an invalid value ([{literal}]). Expected type '{expected_type}'."
    )
}

fn tax_exemption_invalid_literal_path(query: &str, root_name: &str, argument_name: &str) -> Value {
    let operation_path = parsed_document(query, &BTreeMap::new())
        .map(|document| document.operation_path)
        .unwrap_or_else(|| "mutation".to_string());
    json!([operation_path, root_name, argument_name])
}

fn tax_exemption_invalid_variable(value: &RawArgumentValue) -> Option<InvalidTaxExemptionVariable> {
    let RawArgumentValue::Variable {
        name,
        value: Some(resolved),
    } = value
    else {
        return None;
    };
    let mut problems = Vec::new();
    if let Some(items) = resolved_value_list(resolved) {
        for (index, item) in items.iter().enumerate() {
            if let Some(item) = resolved_value_string(item) {
                if !is_known_tax_exemption(&item) {
                    problems.push((index, item));
                }
            }
        }
    }
    if problems.is_empty() {
        return None;
    }
    Some(InvalidTaxExemptionVariable {
        variable_name: name.clone(),
        provided: resolved_value_json(resolved),
        problems,
    })
}

fn tax_exemption_invalid_variable_error(
    query: &str,
    invalid: &InvalidTaxExemptionVariable,
) -> Value {
    let one_of = TAX_EXEMPTION_VALUES.join(", ");
    let problems: Vec<Value> = invalid
        .problems
        .iter()
        .map(|(index, value)| {
            json!({
                "path": [index],
                "explanation": format!("Expected \"{value}\" to be one of: {one_of}"),
            })
        })
        .collect();
    let (first_index, first_value) = &invalid.problems[0];
    let declared_type = graphql_variable_definition_type(query, &invalid.variable_name)
        .unwrap_or_else(|| "[TaxExemption!]".to_string());
    let message = format!(
        "Variable ${} of type {declared_type} was provided invalid value for {first_index} (Expected \"{first_value}\" to be one of: {one_of})",
        invalid.variable_name
    );
    let location = graphql_variable_definition_location(query, &invalid.variable_name)
        .map(|(line, column)| SourceLocation { line, column })
        .unwrap_or(SourceLocation { line: 1, column: 1 });
    invalid_variable_error_envelope(
        message,
        location,
        invalid.provided.clone(),
        Value::Array(problems),
    )
}

/// Members of the `CustomerSmsMarketingState` GraphQL enum. Values outside this set
/// (e.g. `INVALID`) fail variable coercion *before* the resolver checks for
/// valid-but-disallowed input states (`NOT_SUBSCRIBED`, `REDACTED`). `INVALID` is a
/// real member of the *email* enum but not the SMS one, hence the channel-specific set.
const SMS_MARKETING_STATES: &[&str] = &[
    "NOT_SUBSCRIBED",
    "PENDING",
    "SUBSCRIBED",
    "UNSUBSCRIBED",
    "REDACTED",
];

/// Validates the `smsMarketingConsent.marketingState` enum value of
/// `customerSmsMarketingConsentUpdate` before any staging. Shopify rejects values
/// outside `CustomerSmsMarketingState` at variable-coercion time with an
/// `INVALID_VARIABLE` error, returning `None` when the state is a known member.
pub(in crate::proxy) fn customer_sms_consent_invalid_enum_error(
    query: &str,
    raw_arguments: &BTreeMap<String, RawArgumentValue>,
) -> Option<Value> {
    let RawArgumentValue::Variable {
        name,
        value: Some(resolved),
    } = raw_arguments.get("input")?
    else {
        return None;
    };
    let input = resolved_value_object(resolved)?;
    let consent = resolved_object_field(&input, "smsMarketingConsent")?;
    let state = resolved_string_field(&consent, "marketingState")?;
    if !SMS_MARKETING_STATES.contains(&state.as_str()) {
        return Some(sms_consent_invalid_variable_error(
            query, name, resolved, &state,
        ));
    }
    None
}

fn sms_consent_invalid_variable_error(
    query: &str,
    variable_name: &str,
    input: &ResolvedValue,
    state: &str,
) -> Value {
    let one_of = SMS_MARKETING_STATES.join(", ");
    let declared_type = graphql_variable_definition_type(query, variable_name)
        .unwrap_or_else(|| "CustomerSmsMarketingConsentUpdateInput!".to_string());
    let explanation = format!("Expected \"{state}\" to be one of: {one_of}");
    let message = format!(
        "Variable ${variable_name} of type {declared_type} was provided invalid value for smsMarketingConsent.marketingState ({explanation})"
    );
    let location = graphql_variable_definition_location(query, variable_name)
        .map(|(line, column)| SourceLocation { line, column })
        .unwrap_or(SourceLocation { line: 1, column: 1 });
    invalid_variable_error_envelope(
        message,
        location,
        resolved_value_json(input),
        json!([{
            "path": ["smsMarketingConsent", "marketingState"],
            "explanation": explanation,
        }]),
    )
}

fn is_known_tax_exemption(value: &str) -> bool {
    TAX_EXEMPTION_VALUES.contains(&value)
}

pub(super) fn customer_update_inline_consent_errors(
    input: &BTreeMap<String, ResolvedValue>,
) -> Vec<Value> {
    let mut errors = Vec::new();
    if input.contains_key("smsMarketingConsent") {
        errors.push(customer_update_inline_consent_error(
            "smsMarketingConsent",
            "customerSmsMarketingConsentUpdate",
        ));
    }
    if input.contains_key("emailMarketingConsent") {
        errors.push(customer_update_inline_consent_error(
            "emailMarketingConsent",
            "customerEmailMarketingConsentUpdate",
        ));
    }
    if input.contains_key("whatsAppMarketingConsent") {
        errors.push(customer_update_inline_consent_error(
            "whatsAppMarketingConsent",
            "customerWhatsAppMarketingConsentUpdate",
        ));
    }
    errors
}

fn customer_update_inline_consent_error(field: &str, mutation: &str) -> Value {
    user_error_omit_code(
        json!([field]),
        &format!("To update {field}, please use the {mutation} Mutation instead"),
        None,
    )
}

pub(super) fn resolved_inline_consent_state(
    consent: &BTreeMap<String, ResolvedValue>,
    field_name: &str,
) -> Option<String> {
    if field_name == "whatsAppMarketingConsent" {
        resolved_string_field(consent, "marketingState")
            .or_else(|| resolved_string_field(consent, "state"))
    } else {
        resolved_string_field(consent, "marketingState")
    }
}

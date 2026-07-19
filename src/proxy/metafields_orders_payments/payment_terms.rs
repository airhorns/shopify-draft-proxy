use super::*;

pub(in crate::proxy) fn payment_terms_field_resolver_registrations(
) -> Vec<FieldResolverRegistration> {
    vec![FieldResolverRegistration::explicit(
        ApiSurface::Admin,
        "PaymentTerms",
        "paymentSchedules",
        payment_terms_schedules_field,
    )]
}

fn payment_terms_schedules_field(
    _proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let connection = invocation.parent.get("paymentSchedules");
    Ok(connection_value_with_args(
        connection.map(connection_nodes).unwrap_or_default(),
        &resolved_arguments_from_json(&invocation.arguments),
        |schedule| {
            schedule
                .get("id")
                .and_then(Value::as_str)
                .map(|id| format!("cursor:{id}"))
                .unwrap_or_default()
        },
    ))
}

/// Exact GraphQL document the proxy issues to hydrate an **Order** owner before
/// payment-terms staging. The text must match the recorded `PaymentTermsOwnerHydrate`
/// cassette byte-for-byte (modulo trailing whitespace) so the strict upstream
/// matcher in `scripts/parity-cassette.ts` replays the real recorded reply.
pub(in crate::proxy) const PAYMENT_TERMS_OWNER_HYDRATE_QUERY: &str = "query PaymentTermsOwnerHydrate($id: ID!) {\n    order(id: $id) {\n      id\n      displayFinancialStatus\n      closed\n      closedAt\n      cancelledAt\n      paymentTerms {\n        id\n      }\n      totalOutstandingSet {\n        shopMoney { amount currencyCode }\n        presentmentMoney { amount currencyCode }\n      }\n      currentTotalPriceSet {\n        shopMoney { amount currencyCode }\n        presentmentMoney { amount currencyCode }\n      }\n      totalPriceSet {\n        shopMoney { amount currencyCode }\n        presentmentMoney { amount currencyCode }\n      }\n    }\n  }";

/// Exact GraphQL document for hydrating a **DraftOrder** owner. Drafts have no
/// `displayFinancialStatus`/`order`-shaped money, so a distinct document selects
/// the draft money bags. Matches the synthetic delete-owner-cascade cassette.
pub(in crate::proxy) const PAYMENT_TERMS_DRAFT_HYDRATE_QUERY: &str = "query PaymentTermsDraftHydrate($id: ID!) {\n    draftOrder(id: $id) {\n      id\n      name\n      paymentTerms {\n        id\n      }\n      subtotalPriceSet {\n        shopMoney { amount currencyCode }\n        presentmentMoney { amount currencyCode }\n      }\n      totalPriceSet {\n        shopMoney { amount currencyCode }\n        presentmentMoney { amount currencyCode }\n      }\n    }\n  }";

/// Exact GraphQL document the proxy issues to hydrate a **PaymentTerms node** by
/// id for the cold update-eligibility path (no local owner link). Must match the
/// recorded `PaymentTermsHydrate` cassette byte-for-byte.
pub(in crate::proxy) const PAYMENT_TERMS_NODE_HYDRATE_QUERY: &str = "query PaymentTermsHydrate($id: ID!) {\n    paymentTerms: node(id: $id) {\n      ... on PaymentTerms {\n        id\n        due\n        overdue\n        dueInDays\n        paymentTermsName\n        paymentTermsType\n        translatedName\n        order {\n          id\n          email\n          closed\n          closedAt\n          cancelledAt\n          displayFinancialStatus\n          totalOutstandingSet {\n            shopMoney { amount currencyCode }\n            presentmentMoney { amount currencyCode }\n          }\n          currentTotalPriceSet {\n            shopMoney { amount currencyCode }\n            presentmentMoney { amount currencyCode }\n          }\n          totalPriceSet {\n            shopMoney { amount currencyCode }\n            presentmentMoney { amount currencyCode }\n          }\n          lineItems(first: 1) {\n            nodes {\n              sellingPlan {\n                name\n              }\n            }\n          }\n        }\n        draftOrder {\n          id\n          status\n          completedAt\n          subtotalPriceSet {\n            shopMoney { amount currencyCode }\n            presentmentMoney { amount currencyCode }\n          }\n          totalPriceSet {\n            shopMoney { amount currencyCode }\n            presentmentMoney { amount currencyCode }\n          }\n        }\n        paymentSchedules(first: 10) {\n          nodes {\n            id\n            dueAt\n            issuedAt\n            completedAt\n            due\n            amount { amount currencyCode }\n            balanceDue { amount currencyCode }\n            totalBalance { amount currencyCode }\n          }\n        }\n      }\n    }\n  }";

pub(in crate::proxy) fn payment_terms_user_error(field: Value, message: &str, code: &str) -> Value {
    user_error(field, message, Some(code))
}

pub(in crate::proxy) fn payment_terms_payload_value(
    payment_terms: Value,
    user_errors: Vec<Value>,
) -> Value {
    json!({
        "paymentTerms": payment_terms,
        "userErrors": user_errors
    })
}

fn payment_terms_node_order_paid(node: &Value) -> bool {
    node.get("order")
        .filter(|order| !order.is_null())
        .is_some_and(payment_terms_order_paid)
}

fn payment_terms_order_paid(order: &Value) -> bool {
    order.get("displayFinancialStatus").and_then(Value::as_str) == Some("PAID")
}

fn payment_terms_falsey_hint(value: &Value) -> bool {
    value.as_bool().is_some_and(|allowed| !allowed)
        || value.as_str().is_some_and(|allowed| {
            matches!(
                allowed.trim().to_ascii_lowercase().as_str(),
                "false" | "0" | "no"
            )
        })
}

fn payment_terms_order_channel_disallowed(order: &Value) -> bool {
    for key in [
        "paymentTermsAllowed",
        "payment_terms_allowed",
        "__draftProxyPaymentTermsAllowed",
    ] {
        if order.get(key).is_some_and(payment_terms_falsey_hint) {
            return true;
        }
    }
    order
        .get("customAttributes")
        .and_then(Value::as_array)
        .is_some_and(|attributes| {
            attributes.iter().any(|attribute| {
                let key = attribute
                    .get("key")
                    .or_else(|| attribute.get("name"))
                    .and_then(Value::as_str);
                matches!(
                    key,
                    Some(
                        "paymentTermsAllowed"
                            | "payment_terms_allowed"
                            | "__draftProxyPaymentTermsAllowed"
                    )
                ) && attribute
                    .get("value")
                    .is_some_and(payment_terms_falsey_hint)
            })
        })
}

fn payment_terms_node_order_channel_disallowed(node: &Value) -> bool {
    node.get("order")
        .filter(|order| !order.is_null())
        .is_some_and(payment_terms_order_channel_disallowed)
}

pub(in crate::proxy) fn payment_terms_success_record(
    id: &str,
    name: &str,
    terms_type: &str,
    due_in_days: Option<i64>,
    schedules: Value,
) -> Value {
    let terms_due = schedules.as_array().is_some_and(|nodes| {
        nodes
            .iter()
            .any(|node| node.get("due").and_then(Value::as_bool).unwrap_or(false))
    });
    // Shopify connection cursors are opaque, stable-per-node strings. We anchor
    // them to the first/last schedule node id so they round-trip and are always
    // non-empty for a populated connection (null for an empty schedule set).
    let (start_cursor, end_cursor) = schedules
        .as_array()
        .filter(|nodes| !nodes.is_empty())
        .map(|nodes| {
            let first = nodes
                .first()
                .and_then(|node| node.get("id"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            let last = nodes
                .last()
                .and_then(|node| node.get("id"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            (
                Some(format!("cursor:{first}")),
                Some(format!("cursor:{last}")),
            )
        })
        .unwrap_or((None, None));
    let payment_schedule_connection = connection_json_with_cursor(
        schedules.as_array().cloned().unwrap_or_default(),
        |_, node| {
            node.get("id")
                .and_then(Value::as_str)
                .map(|id| format!("cursor:{id}"))
                .unwrap_or_default()
        },
        connection_page_info(false, false, start_cursor, end_cursor),
    );
    json!({
        "id": id,
        "due": terms_due,
        "overdue": terms_due,
        "dueInDays": due_in_days.map(|days| json!(days)).unwrap_or(Value::Null),
        "paymentTermsName": name,
        "paymentTermsType": terms_type,
        "translatedName": name,
        "paymentSchedules": payment_schedule_connection
    })
}

pub(in crate::proxy) fn payment_terms_record_with_effective_due(
    payment_terms: &Value,
    now_epoch: i64,
) -> Value {
    let mut record = payment_terms.clone();
    let mut terms_due = false;
    if let Some(nodes) = record
        .get_mut("paymentSchedules")
        .and_then(|connection| connection.get_mut("nodes"))
        .and_then(Value::as_array_mut)
    {
        for node in nodes {
            let due = payment_schedule_due_state(
                node.get("dueAt").and_then(Value::as_str),
                node.get("completedAt").and_then(Value::as_str),
                now_epoch,
            );
            node["due"] = json!(due);
            terms_due |= due;
        }
    }
    record["due"] = json!(terms_due);
    record["overdue"] = json!(terms_due);
    record
}

/// Projects the Shopify payment-terms template id onto its (name, type, dueInDays)
/// tuple. The template catalog is fixed (see the live payment-terms-templates-read
/// capture): Net N templates carry their day count, Fixed/Due-on-receipt/Due-on-
/// fulfillment carry a null dueInDays. Unknown or blank template ids fall back to
/// Net 30, matching Shopify's default term.
pub(in crate::proxy) fn payment_terms_template_projection(
    template_id: &str,
) -> (&'static str, &'static str, Option<i64>) {
    let tail = resource_id_tail(template_id);
    PAYMENT_TERMS_TEMPLATE_CATALOG
        .iter()
        .find(|(catalog_tail, ..)| *catalog_tail == tail)
        .map(|(_, name, _, due_in_days, terms_type)| (*name, *terms_type, *due_in_days))
        // Template/4 is Net 30; unknown/blank ids fall back to the same default term.
        .unwrap_or(("Net 30", "NET", Some(30)))
}

/// Shopify's payment-terms template catalog is a fixed, store-independent global
/// list (Due on receipt / fulfillment, Net 7/15/30/45/60/90, Fixed). The tuple is
/// `(id-tail, name, description, dueInDays, paymentTermsType)` projected verbatim
/// from the live `payment-terms-templates-read` capture so the strict-json parity
/// read matches; `translatedName` mirrors `name` for the default (English) locale.
/// Ordering matters: the live catalog returns receipt, fulfillment, the net rung,
/// then fixed.
const PAYMENT_TERMS_TEMPLATE_CATALOG: &[(&str, &str, &str, Option<i64>, &str)] = &[
    ("1", "Due on receipt", "Due on receipt", None, "RECEIPT"),
    (
        "9",
        "Due on fulfillment",
        "Due on fulfillment",
        None,
        "FULFILLMENT",
    ),
    ("2", "Net 7", "Within 7 days", Some(7), "NET"),
    ("3", "Net 15", "Within 15 days", Some(15), "NET"),
    ("4", "Net 30", "Within 30 days", Some(30), "NET"),
    ("8", "Net 45", "Within 45 days", Some(45), "NET"),
    ("5", "Net 60", "Within 60 days", Some(60), "NET"),
    ("6", "Net 90", "Within 90 days", Some(90), "NET"),
    ("7", "Fixed", "Fixed date", None, "FIXED"),
];

/// True when `template_id` (a `gid://shopify/PaymentTermsTemplate/<tail>`) names a
/// template in the fixed global catalog above. Shopify rejects unknown templates
/// with a "Could not find payment terms template." user error; this membership
/// check derives that rejection from the catalog rather than matching a single
/// sentinel id.
fn payment_terms_template_exists(template_id: &str) -> bool {
    let tail = resource_id_tail(template_id);
    PAYMENT_TERMS_TEMPLATE_CATALOG
        .iter()
        .any(|(catalog_tail, ..)| *catalog_tail == tail)
}

fn payment_terms_template_type(template_id: &str) -> Option<&'static str> {
    let tail = resource_id_tail(template_id);
    PAYMENT_TERMS_TEMPLATE_CATALOG
        .iter()
        .find(|(catalog_tail, ..)| *catalog_tail == tail)
        .map(|(_, _, _, _, terms_type)| *terms_type)
}

impl DraftProxy {
    /// Return the canonical fixed template catalog. The GraphQL engine owns
    /// aliases and output projection; this resolver only consumes coerced root
    /// arguments.
    pub(crate) fn payment_terms_templates_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let type_filter = invocation
            .arguments
            .get("paymentTermsType")
            .or_else(|| invocation.arguments.get("type"))
            .and_then(Value::as_str);
        let templates: Vec<Value> = PAYMENT_TERMS_TEMPLATE_CATALOG
            .iter()
            .filter(|(_, _, _, _, terms_type)| {
                type_filter.is_none_or(|filter| *terms_type == filter)
            })
            .map(|(tail, name, description, due_in_days, terms_type)| {
                json!({
                    "id": shopify_gid("PaymentTermsTemplate", tail),
                    "name": name,
                    "description": description,
                    "dueInDays": due_in_days.map(Value::from).unwrap_or(Value::Null),
                    "paymentTermsType": terms_type,
                    "translatedName": name,
                    "__typename": "PaymentTermsTemplate"
                })
            })
            .collect();
        ResolverOutcome::value(Value::Array(templates))
    }
}

/// Adds `days` to the date portion of an ISO-8601 timestamp, preserving the
/// time-of-day and zone suffix verbatim ("2026-04-27T12:00:00Z" + 30 ->
/// "2026-05-27T12:00:00Z").
fn add_days_to_iso(iso: &str, days: i64) -> String {
    let (date_part, rest) = match iso.split_once('T') {
        Some((date, rest)) => (date, Some(rest)),
        None => (iso, None),
    };
    let parts: Vec<&str> = date_part.split('-').collect();
    if parts.len() != 3 {
        return iso.to_string();
    }
    let (Ok(year), Ok(month), Ok(day)) = (
        parts[0].parse::<i32>(),
        parts[1].parse::<u32>(),
        parts[2].parse::<u32>(),
    ) else {
        return iso.to_string();
    };
    let (ny, nm, nd) = civil_from_days(days_from_civil(year, month, day) + days);
    let new_date = format!("{ny:04}-{nm:02}-{nd:02}");
    match rest {
        Some(rest) => format!("{new_date}T{rest}"),
        None => new_date,
    }
}

fn payment_schedule_due_state(
    due_at: Option<&str>,
    completed_at: Option<&str>,
    now_epoch: i64,
) -> bool {
    if completed_at.is_some() {
        return false;
    }
    let Some(due_at) = due_at else {
        return false;
    };
    let Some(due_at_epoch) = super::parse_rfc3339_epoch_seconds(due_at) else {
        return false;
    };
    due_at_epoch <= now_epoch
}

/// Builds a materialized PaymentSchedule node from the owner money and the
/// requested schedule. NET terms compute `dueAt` from `issuedAt` plus the
/// template's due-day count when the input omits an explicit `dueAt`; FIXED
/// terms carry the explicit `dueAt` with a null `issuedAt`.
fn payment_schedule_node(
    schedule_id: &str,
    input_schedule: Option<&BTreeMap<String, ResolvedValue>>,
    due_in_days: Option<i64>,
    amount: &str,
    currency: &str,
    now_epoch: i64,
) -> Value {
    let issued_at = input_schedule.and_then(|schedule| resolved_string_field(schedule, "issuedAt"));
    let input_due_at = input_schedule.and_then(|schedule| resolved_string_field(schedule, "dueAt"));
    let due_at = match input_due_at {
        Some(due) => Some(due),
        None => match (issued_at.as_deref(), due_in_days) {
            (Some(issued), Some(days)) => Some(add_days_to_iso(issued, days)),
            _ => None,
        },
    };
    let due = payment_schedule_due_state(due_at.as_deref(), None, now_epoch);
    let money = money_value(&normalize_money_amount(amount), currency);
    json!({
        "id": schedule_id,
        "issuedAt": issued_at.map(Value::String).unwrap_or(Value::Null),
        "dueAt": due_at.map(Value::String).unwrap_or(Value::Null),
        "completedAt": Value::Null,
        "due": due,
        "amount": money.clone(),
        "balanceDue": money.clone(),
        "totalBalance": money
    })
}

/// Pulls the owner's outstanding money for the payment schedule. Orders carry a
/// presentment money bag (the schedule is denominated in presentment currency);
/// seeded/hydrated drafts expose shop money on `totalPriceSet`/`subtotalPriceSet`.
fn payment_terms_extract_owner_money(owner: &Value) -> Option<(String, String)> {
    for set_key in [
        "totalOutstandingSet",
        "currentTotalPriceSet",
        "totalPriceSet",
        "subtotalPriceSet",
    ] {
        let Some(set) = owner.get(set_key) else {
            continue;
        };
        for money_key in ["presentmentMoney", "shopMoney"] {
            let Some(money) = set.get(money_key) else {
                continue;
            };
            if let (Some(amount), Some(currency)) = (
                money.get("amount").and_then(Value::as_str),
                money.get("currencyCode").and_then(Value::as_str),
            ) {
                return Some((normalize_money_amount(amount), currency.to_string()));
            }
        }
    }
    None
}

pub(in crate::proxy) fn payment_terms_validation_error(
    attrs: &BTreeMap<String, ResolvedValue>,
    unsuccessful_code: &str,
) -> Option<Value> {
    let template_id = resolved_string_field(attrs, "paymentTermsTemplateId");

    let schedules = resolved_object_list_field(attrs, "paymentSchedules");
    if schedules.len() > 1 {
        return Some(payment_terms_user_error(
            Value::Null,
            "Cannot create payment terms with multiple payment schedules.",
            unsuccessful_code,
        ));
    }

    let template_id = template_id.as_deref()?;
    if !payment_terms_template_exists(template_id) {
        return Some(payment_terms_user_error(
            Value::Null,
            "Could not find payment terms template.",
            unsuccessful_code,
        ));
    }

    let terms_type = payment_terms_template_type(template_id).unwrap_or("NET");
    let has_due_at = schedules
        .iter()
        .any(|schedule| resolved_string_field(schedule, "dueAt").is_some());
    match terms_type {
        "FIXED" => {
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
        "NET" => {
            let has_schedule_date = schedules.first().is_some_and(|schedule| {
                resolved_string_field(schedule, "issuedAt").is_some()
                    || resolved_string_field(schedule, "dueAt").is_some()
            });
            if !has_schedule_date {
                Some(payment_terms_user_error(
                    Value::Null,
                    "A due date is required with fixed or net payment terms.",
                    unsuccessful_code,
                ))
            } else {
                None
            }
        }
        "RECEIPT" | "FULFILLMENT" if has_due_at => Some(payment_terms_user_error(
            Value::Null,
            "A due date cannot be set with event payment terms.",
            unsuccessful_code,
        )),
        _ => None,
    }
}

pub(in crate::proxy) fn payment_terms_delete_payload_value(
    deleted_id: Value,
    user_errors: Vec<Value>,
) -> Value {
    json!({
        "deletedId": deleted_id,
        "userErrors": user_errors
    })
}

pub(in crate::proxy) fn payment_terms_attrs_from_create_arguments(
    arguments: &BTreeMap<String, ResolvedValue>,
) -> BTreeMap<String, ResolvedValue> {
    resolved_object_field(arguments, "paymentTermsAttributes")
        .unwrap_or_else(|| resolved_object_field(arguments, "attrs").unwrap_or_default())
}

pub(in crate::proxy) fn payment_terms_attrs_from_update_arguments(
    arguments: &BTreeMap<String, ResolvedValue>,
) -> (String, BTreeMap<String, ResolvedValue>) {
    let input = resolved_object_field(arguments, "input").unwrap_or_default();
    let payment_terms_id = resolved_string_field(&input, "paymentTermsId").unwrap_or_default();
    let attrs = resolved_object_field(&input, "paymentTermsAttributes").unwrap_or_default();
    (payment_terms_id, attrs)
}

pub(in crate::proxy) fn payment_terms_record_from_attrs(
    id: &str,
    attrs: &BTreeMap<String, ResolvedValue>,
    amount: &str,
    currency: &str,
    now_epoch: i64,
) -> Value {
    let template_id = resolved_string_field(attrs, "paymentTermsTemplateId").unwrap_or_default();
    let (name, terms_type, due_in_days) = payment_terms_template_projection(&template_id);
    // Due-on-receipt and due-on-fulfillment terms have no materialized schedule;
    // fixed and net terms project a single schedule node whose money mirrors the
    // owning order/draft and whose dates derive from the requested schedule.
    let schedules = if matches!(terms_type, "RECEIPT" | "FULFILLMENT") {
        json!([])
    } else {
        let schedule_id = shopify_gid("PaymentSchedule", resource_id_tail(id));
        let input_schedules = resolved_object_list_field(attrs, "paymentSchedules");
        let node = payment_schedule_node(
            &schedule_id,
            input_schedules.first(),
            due_in_days,
            amount,
            currency,
            now_epoch,
        );
        json!([node])
    };
    payment_terms_success_record(id, name, terms_type, due_in_days, schedules)
}

pub(in crate::proxy) fn payment_terms_create_value(
    arguments: &BTreeMap<String, ResolvedValue>,
) -> Result<(String, String, BTreeMap<String, ResolvedValue>), Value> {
    let reference_id = resolved_string_field(arguments, "referenceId").unwrap_or_default();
    let attrs = payment_terms_attrs_from_create_arguments(arguments);
    if let Some(error) =
        payment_terms_validation_error(&attrs, "PAYMENT_TERMS_CREATION_UNSUCCESSFUL")
    {
        return Err(payment_terms_payload_value(Value::Null, vec![error]));
    }

    let reference_tail = resource_id_tail(&reference_id);
    let id_suffix = if reference_tail.is_empty() {
        "1"
    } else {
        reference_tail
    };
    let terms_id = shopify_gid("PaymentTerms", id_suffix);
    Ok((reference_id, terms_id, attrs))
}

pub(in crate::proxy) fn payment_terms_update_value(
    arguments: &BTreeMap<String, ResolvedValue>,
) -> Result<(String, BTreeMap<String, ResolvedValue>), Value> {
    let (payment_terms_id, attrs) = payment_terms_attrs_from_update_arguments(arguments);
    if let Some(error) = payment_terms_validation_error(&attrs, "PAYMENT_TERMS_UPDATE_UNSUCCESSFUL")
    {
        return Err(payment_terms_payload_value(Value::Null, vec![error]));
    }
    Ok((payment_terms_id, attrs))
}

fn payment_terms_owner_paid_payload(code: &str) -> Value {
    payment_terms_payload_value(
        Value::Null,
        vec![payment_terms_user_error(
            Value::Null,
            "Cannot create payment terms on an Order that has already been paid in full.",
            code,
        )],
    )
}

fn payment_terms_owner_channel_policy_payload(code: &str) -> Value {
    payment_terms_payload_value(
        Value::Null,
        vec![payment_terms_user_error(
            Value::Null,
            "Cannot create payment terms on an Order where the sales channel does not allow payment terms.",
            code,
        )],
    )
}

fn payment_terms_owner_not_found_payload(owner_id: &str, code: &str) -> Value {
    let (resource_name, tail) =
        if let Some(tail) = shopify_gid_tail_for_type(owner_id, "DraftOrder") {
            ("Draft order", tail)
        } else if let Some(tail) = shopify_gid_tail_for_type(owner_id, "Order") {
            ("Order", tail)
        } else {
            ("Order", resource_id_tail(owner_id))
        };
    payment_terms_payload_value(
        Value::Null,
        vec![payment_terms_user_error(
            Value::Null,
            &format!("Cannot find the specific {resource_name} with id {tail}."),
            code,
        )],
    )
}

fn payment_terms_order_total_price_set(
    order_input: &BTreeMap<String, ResolvedValue>,
    shop_currency_code: &str,
) -> Value {
    let default_shop_currency = resolved_string_field(order_input, "currency")
        .or_else(|| resolved_string_field(order_input, "currencyCode"))
        .unwrap_or_else(|| shop_currency_code.to_string());
    let default_presentment_currency = resolved_string_field(order_input, "presentmentCurrency")
        .or_else(|| resolved_string_field(order_input, "presentmentCurrencyCode"))
        .unwrap_or_else(|| default_shop_currency.clone());
    let [shop_amount, shop_currency, presentment_amount, presentment_currency] =
        line_items_price_set_values(
            order_input,
            [
                "0.0",
                &default_shop_currency,
                "0.0",
                &default_presentment_currency,
            ],
            ["0.0", &default_shop_currency],
            Some(["0.0", &default_presentment_currency]),
        );
    money_set_pair(
        &shop_amount,
        &shop_currency,
        &presentment_amount,
        &presentment_currency,
    )
}

impl DraftProxy {
    pub(crate) fn payment_terms_mutation_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        let (value, staged_id) = match invocation.root_name {
            "paymentTermsCreate" => {
                self.payment_terms_create_payload(invocation.request, &arguments)
            }
            "paymentTermsUpdate" => {
                self.payment_terms_update_payload(invocation.request, &arguments)
            }
            "paymentTermsDelete" => self.payment_terms_delete_payload(&arguments),
            root => {
                return ResolverOutcome::error(format!(
                    "Unknown payment-terms mutation root `{root}`"
                ));
            }
        };
        let mut outcome = ResolverOutcome::value(value);
        if let Some(id) = staged_id {
            outcome = outcome.with_log_draft(LogDraft::staged(
                invocation.root_name,
                "payments",
                vec![id],
            ));
        }
        outcome
    }

    fn payment_terms_create_payload(
        &mut self,
        request: &Request,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> (Value, Option<String>) {
        let (owner_id, terms_id, attrs) = match payment_terms_create_value(arguments) {
            Ok(value) => value,
            Err(payload) => return (payload, None),
        };
        let Some(owner) = self.payment_terms_owner_record(request, &owner_id) else {
            return (
                payment_terms_owner_not_found_payload(
                    &owner_id,
                    "PAYMENT_TERMS_CREATION_UNSUCCESSFUL",
                ),
                None,
            );
        };
        if is_shopify_gid_of_type(&owner_id, "Order") && payment_terms_order_paid(&owner) {
            return (
                payment_terms_owner_paid_payload("PAYMENT_TERMS_CREATION_UNSUCCESSFUL"),
                None,
            );
        }
        if is_shopify_gid_of_type(&owner_id, "Order")
            && payment_terms_order_channel_disallowed(&owner)
        {
            return (
                payment_terms_owner_channel_policy_payload("PAYMENT_TERMS_CREATION_UNSUCCESSFUL"),
                None,
            );
        }

        let (amount, currency) = payment_terms_extract_owner_money(&owner)
            .unwrap_or_else(|| ("0.0".to_string(), "CAD".to_string()));
        let record = payment_terms_record_from_attrs(
            &terms_id,
            &attrs,
            &amount,
            &currency,
            self.current_epoch_seconds(),
        );
        self.store
            .staged
            .payment_terms
            .insert(terms_id.clone(), record.clone());
        self.store
            .staged
            .payment_terms_owner_index
            .insert(owner_id.clone(), terms_id.clone());
        self.attach_payment_terms_to_owner(&owner_id, Some(record.clone()));
        (
            payment_terms_payload_value(
                payment_terms_record_with_effective_due(&record, self.current_epoch_seconds()),
                Vec::new(),
            ),
            Some(terms_id),
        )
    }

    fn payment_terms_update_payload(
        &mut self,
        request: &Request,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> (Value, Option<String>) {
        let (terms_id, attrs) = match payment_terms_update_value(arguments) {
            Ok(value) => value,
            Err(payload) => return (payload, None),
        };
        let owner_id = self.payment_terms_owner_id(&terms_id);
        let has_staged_record = self.store.staged.payment_terms.contains_key(&terms_id);
        let owner_record = owner_id
            .as_deref()
            .and_then(|owner| self.payment_terms_owner_record(request, owner));
        let cold_node = if owner_id.is_none() && !has_staged_record {
            self.hydrate_payment_terms_node(request, &terms_id)
        } else {
            None
        };
        if owner_id.is_none() && !has_staged_record && cold_node.is_none() {
            return (
                payment_terms_payload_value(
                    Value::Null,
                    vec![payment_terms_user_error(
                        Value::Null,
                        "Could not find payment terms.",
                        "PAYMENT_TERMS_UPDATE_UNSUCCESSFUL",
                    )],
                ),
                None,
            );
        }
        if owner_id
            .as_deref()
            .is_some_and(|owner| is_shopify_gid_of_type(owner, "Order"))
            && owner_record.as_ref().is_some_and(payment_terms_order_paid)
            || cold_node
                .as_ref()
                .is_some_and(payment_terms_node_order_paid)
        {
            return (
                payment_terms_owner_paid_payload("PAYMENT_TERMS_UPDATE_UNSUCCESSFUL"),
                None,
            );
        }
        if owner_id
            .as_deref()
            .is_some_and(|owner| is_shopify_gid_of_type(owner, "Order"))
            && owner_record
                .as_ref()
                .is_some_and(payment_terms_order_channel_disallowed)
            || cold_node
                .as_ref()
                .is_some_and(payment_terms_node_order_channel_disallowed)
        {
            return (
                payment_terms_owner_channel_policy_payload("PAYMENT_TERMS_UPDATE_UNSUCCESSFUL"),
                None,
            );
        }

        let (amount, currency) = match owner_id.as_deref() {
            Some(_) => owner_record
                .as_ref()
                .and_then(payment_terms_extract_owner_money)
                .unwrap_or_else(|| ("0.0".to_string(), "CAD".to_string())),
            None => self
                .payment_terms_record_money(&terms_id)
                .unwrap_or_else(|| ("0.0".to_string(), "CAD".to_string())),
        };
        let record = payment_terms_record_from_attrs(
            &terms_id,
            &attrs,
            &amount,
            &currency,
            self.current_epoch_seconds(),
        );
        self.store
            .staged
            .payment_terms
            .insert(terms_id.clone(), record.clone());
        if let Some(owner_id) = owner_id {
            self.attach_payment_terms_to_owner(&owner_id, Some(record.clone()));
        }
        (
            payment_terms_payload_value(
                payment_terms_record_with_effective_due(&record, self.current_epoch_seconds()),
                Vec::new(),
            ),
            Some(terms_id),
        )
    }

    fn payment_terms_delete_payload(
        &mut self,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> (Value, Option<String>) {
        let input = resolved_object_field(arguments, "input").unwrap_or_default();
        let payment_terms_id = resolved_string_field(&input, "paymentTermsId").unwrap_or_default();
        if self
            .store
            .staged
            .payment_terms
            .remove(&payment_terms_id)
            .is_some()
        {
            if let Some(owner_id) = self.remove_payment_terms_owner_link(&payment_terms_id) {
                self.attach_payment_terms_to_owner(&owner_id, None);
            }
            return (
                payment_terms_delete_payload_value(json!(payment_terms_id), Vec::new()),
                Some(payment_terms_id),
            );
        }
        (
            payment_terms_delete_payload_value(
                Value::Null,
                vec![payment_terms_user_error(
                    Value::Null,
                    "Could not find payment terms.",
                    "PAYMENT_TERMS_DELETE_UNSUCCESSFUL",
                )],
            ),
            None,
        )
    }

    pub(in crate::proxy) fn payment_terms_local_outcome(
        &mut self,
        request: &Request,
        root_name: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
        requests_payment_terms: bool,
    ) -> Option<ResolverOutcome<Value>> {
        match root_name {
            "orderCreate" if requests_payment_terms => {
                if resolved_object_field(arguments, "order")
                    .is_some_and(|input| order_create_input_needs_shop_currency_default(&input))
                {
                    self.hydrate_shop_pricing_state_if_missing(request, true, false);
                }
                let order = self.stage_payment_terms_order(arguments);
                let staged_ids = vec![order["id"].as_str().unwrap_or_default().to_string()];
                Some(
                    ResolverOutcome::value(json!({ "order": order, "userErrors": [] }))
                        .with_log_draft(LogDraft::staged("orderCreate", "orders", staged_ids)),
                )
            }
            "order" | "draftOrder" => {
                let id = resolved_string_field(arguments, "id")?;
                let has_staged_owner = self
                    .store
                    .staged
                    .payment_terms_owner_index
                    .contains_key(&id)
                    || self.store.staged.orders.contains_key(&id)
                    || self.store.staged.draft_orders.contains_key(&id);
                has_staged_owner.then(|| {
                    let record = if root_name == "draftOrder" {
                        self.store.staged.draft_orders.get(&id)
                    } else {
                        self.store.staged.orders.get(&id)
                    };
                    let value = record
                        .map(|record| self.payment_terms_owner_record_with_effective_due(record))
                        .map(|record| {
                            if root_name == "order" {
                                self.order_with_return_status_value(&record)
                            } else {
                                record
                            }
                        })
                        .unwrap_or(Value::Null);
                    ResolverOutcome::value(value)
                })
            }
            _ => None,
        }
    }

    fn payment_terms_owner_id(&self, terms_id: &str) -> Option<String> {
        self.store.staged.payment_terms_owner_index.iter().find_map(
            |(owner_id, staged_terms_id)| (staged_terms_id == terms_id).then(|| owner_id.clone()),
        )
    }

    fn payment_terms_owner_record(&mut self, request: &Request, owner_id: &str) -> Option<Value> {
        if let Some(owner) = self
            .store
            .staged
            .orders
            .get(owner_id)
            .or_else(|| self.store.staged.draft_orders.get(owner_id))
        {
            return Some(owner.clone());
        }
        let owner = self.hydrate_payment_terms_owner(request, owner_id)?;
        if is_shopify_gid_of_type(owner_id, "DraftOrder") {
            self.store
                .staged
                .draft_orders
                .entry(owner_id.to_string())
                .or_insert_with(|| owner.clone());
        } else {
            self.store
                .staged
                .orders
                .entry(owner_id.to_string())
                .or_insert_with(|| owner.clone());
        }
        Some(owner)
    }

    /// Cassette-backed owner hydration: in live-hybrid replay, issue the exact
    /// recorded `PaymentTermsOwnerHydrate` (Order) or `PaymentTermsDraftHydrate`
    /// (DraftOrder) document so the strict upstream matcher replays the real
    /// owner reply. Gated on LiveHybrid so other read modes are untouched;
    /// returns the `order`/`draftOrder` node from the recorded reply.
    fn hydrate_payment_terms_owner(&self, request: &Request, owner_id: &str) -> Option<Value> {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return None;
        }
        let (query, operation_name) = if is_shopify_gid_of_type(owner_id, "DraftOrder") {
            (
                PAYMENT_TERMS_DRAFT_HYDRATE_QUERY,
                "PaymentTermsDraftHydrate",
            )
        } else {
            (
                PAYMENT_TERMS_OWNER_HYDRATE_QUERY,
                "PaymentTermsOwnerHydrate",
            )
        };
        let response = self.upstream_post(
            request,
            json!({
                "query": query,
                "operationName": operation_name,
                "variables": { "id": owner_id }
            }),
        );
        if response.status >= 400 {
            return None;
        }
        let data = response.body.get("data")?;
        data.get("draftOrder")
            .or_else(|| data.get("order"))
            .filter(|owner| !owner.is_null())
            .cloned()
    }

    /// Cassette-backed PaymentTerms-node hydration for the cold update path:
    /// issues the exact recorded `PaymentTermsHydrate` document and returns the
    /// resolved `paymentTerms` node. Gated on LiveHybrid.
    fn hydrate_payment_terms_node(&self, request: &Request, terms_id: &str) -> Option<Value> {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return None;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": PAYMENT_TERMS_NODE_HYDRATE_QUERY,
                "operationName": "PaymentTermsHydrate",
                "variables": { "id": terms_id }
            }),
        );
        if response.status >= 400 {
            return None;
        }
        response
            .body
            .get("data")?
            .get("paymentTerms")
            .filter(|node| !node.is_null())
            .cloned()
    }

    /// Reads the money already materialized on a staged payment-terms record's
    /// first schedule node, so an update whose owner link is unavailable reuses
    /// the money established at create time.
    fn payment_terms_record_money(&self, terms_id: &str) -> Option<(String, String)> {
        let node = self
            .store
            .staged
            .payment_terms
            .get(terms_id)?
            .get("paymentSchedules")?
            .get("nodes")?
            .as_array()?
            .first()?;
        let money = node.get("amount")?;
        Some((
            money.get("amount")?.as_str()?.to_string(),
            money.get("currencyCode")?.as_str()?.to_string(),
        ))
    }

    fn remove_payment_terms_owner_link(&mut self, terms_id: &str) -> Option<String> {
        let owner_id = self.payment_terms_owner_id(terms_id)?;
        self.store
            .staged
            .payment_terms_owner_index
            .remove(&owner_id);
        Some(owner_id)
    }

    fn attach_payment_terms_to_owner(&mut self, owner_id: &str, terms: Option<Value>) {
        let terms = terms
            .as_ref()
            .map(|terms| {
                payment_terms_record_with_effective_due(terms, self.current_epoch_seconds())
            })
            .unwrap_or(Value::Null);
        let entry = if is_shopify_gid_of_type(owner_id, "DraftOrder") {
            self.store
                .staged
                .draft_orders
                .entry(owner_id.to_string())
                .or_insert_with(|| {
                    json!({
                        "id": owner_id,
                        "name": "#DRAFT"
                    })
                })
        } else {
            self.store
                .staged
                .orders
                .entry(owner_id.to_string())
                .or_insert_with(|| {
                    json!({
                        "id": owner_id,
                        "name": "#1"
                    })
                })
        };
        entry["paymentTerms"] = terms;
    }

    pub(in crate::proxy) fn payment_terms_owner_record_with_effective_due(
        &self,
        owner: &Value,
    ) -> Value {
        let mut owner = owner.clone();
        if let Some(payment_terms) = owner
            .get("paymentTerms")
            .filter(|payment_terms| !payment_terms.is_null())
            .cloned()
        {
            owner["paymentTerms"] = payment_terms_record_with_effective_due(
                &payment_terms,
                self.current_epoch_seconds(),
            );
        }
        owner
    }

    fn stage_payment_terms_order(&mut self, arguments: &BTreeMap<String, ResolvedValue>) -> Value {
        let order_input = resolved_object_field(arguments, "order").unwrap_or_default();
        let id = self.next_synthetic_gid("Order");
        let shop_currency_code = self.store.shop_currency_code();
        let price_set = payment_terms_order_total_price_set(&order_input, &shop_currency_code);
        let order_name = self.next_order_name();
        let order = json!({
            "id": id,
            "name": order_name,
            "currentTotalPriceSet": price_set.clone(),
            "totalPriceSet": price_set.clone(),
            "totalOutstandingSet": price_set,
            "paymentTerms": Value::Null
        });
        self.store.staged.orders.insert(
            order["id"].as_str().unwrap_or_default().to_string(),
            order.clone(),
        );
        order
    }
}

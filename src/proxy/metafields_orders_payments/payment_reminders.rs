use super::*;

/// Exact GraphQL document the proxy issues to hydrate a PaymentSchedule before
/// locally deciding `paymentReminderSend`. It must match the recorded
/// `PaymentScheduleReminderHydrate` cassette byte-for-byte.
pub(in crate::proxy) const PAYMENT_SCHEDULE_REMINDER_HYDRATE_QUERY: &str = "query PaymentScheduleReminderHydrate($id: ID!) {\n  paymentSchedule: node(id: $id) {\n    ... on PaymentSchedule {\n      id\n      dueAt\n      issuedAt\n      completedAt\n      paymentTerms {\n        id\n        overdue\n        dueInDays\n        paymentTermsName\n        paymentTermsType\n        translatedName\n        order {\n          id\n          email\n          closed\n          closedAt\n          cancelledAt\n          displayFinancialStatus\n          lineItems(first: 1) {\n            nodes {\n              sellingPlan {\n                name\n              }\n            }\n          }\n        }\n        draftOrder {\n          id\n          status\n          completedAt\n        }\n        paymentSchedules(first: 10) {\n          nodes {\n            id\n            dueAt\n            issuedAt\n            completedAt\n          }\n        }\n      }\n    }\n  }\n}";

impl DraftProxy {
    pub(crate) fn payment_reminder_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let schedule_id = invocation
            .arguments
            .get("paymentScheduleId")
            .and_then(Value::as_str)
            .unwrap_or_default();

        if schedule_id.is_empty() || !has_shopify_gid_prefix(schedule_id) {
            return payment_reminder_top_level_error_outcome(
                payment_reminder_invalid_gid_error(
                    schedule_id,
                    variable_definition_info(invocation.query, "paymentScheduleId")
                        .map(|info| info.location)
                        .unwrap_or(invocation.root_location),
                ),
                invocation.response_key,
            );
        }

        if !is_shopify_gid_of_type(schedule_id, "PaymentSchedule") {
            return payment_reminder_top_level_error_outcome(
                payment_reminder_resource_not_found_error(
                    invocation.root_location,
                    invocation.response_key,
                ),
                invocation.response_key,
            );
        }

        ResolverOutcome::value(
            self.payment_reminder_payload_for_schedule(invocation.request, schedule_id),
        )
    }

    fn payment_reminder_payload_for_schedule(
        &mut self,
        request: &Request,
        schedule_id: &str,
    ) -> Value {
        if self
            .store
            .staged
            .deleted_payment_schedule_ids
            .contains(schedule_id)
        {
            return payment_reminder_error_payload("Payment schedule does not exist");
        }
        if self
            .store
            .staged
            .payment_reminder_schedule_ids
            .contains(schedule_id)
        {
            return payment_reminder_rate_limit_payload();
        }
        if let Some(schedule) = self.staged_payment_reminder_schedule(schedule_id) {
            let rate_limit_key = payment_reminder_rate_limit_key(schedule_id, &schedule);
            if self
                .store
                .staged
                .payment_reminder_schedule_ids
                .contains(&rate_limit_key)
            {
                return payment_reminder_rate_limit_payload();
            }
            let payload =
                self.payment_reminder_payload_from_hydrated_schedule(schedule_id, &schedule);
            if payment_reminder_payload_is_success(&payload) {
                self.store
                    .staged
                    .payment_reminder_schedule_ids
                    .insert(rate_limit_key);
                self.store
                    .staged
                    .payment_reminder_schedule_ids
                    .insert(schedule_id.to_string());
            }
            return payload;
        }
        let schedule = self
            .hydrate_payment_reminder_schedule(request, schedule_id)
            .unwrap_or(Value::Null);
        let rate_limit_key = payment_reminder_rate_limit_key(schedule_id, &schedule);
        if self
            .store
            .staged
            .payment_reminder_schedule_ids
            .contains(&rate_limit_key)
        {
            return payment_reminder_rate_limit_payload();
        }
        let payload = self.payment_reminder_payload_from_hydrated_schedule(schedule_id, &schedule);
        if payment_reminder_payload_is_success(&payload) {
            self.store
                .staged
                .payment_reminder_schedule_ids
                .insert(rate_limit_key);
            self.store
                .staged
                .payment_reminder_schedule_ids
                .insert(schedule_id.to_string());
        }
        payload
    }

    fn staged_payment_reminder_schedule(&self, schedule_id: &str) -> Option<Value> {
        for (terms_id, terms) in &self.store.staged.payment_terms {
            let schedule = terms
                .get("paymentSchedules")
                .and_then(|connection| connection.get("nodes"))
                .and_then(Value::as_array)
                .and_then(|nodes| {
                    nodes
                        .iter()
                        .find(|node| node.get("id").and_then(Value::as_str) == Some(schedule_id))
                });
            let Some(schedule) = schedule else {
                continue;
            };
            let Some(owner_id) = self.payment_reminder_owner_id_for_terms(terms_id) else {
                continue;
            };
            let owner = if is_shopify_gid_of_type(&owner_id, "DraftOrder") {
                self.store.staged.draft_orders.get(&owner_id)
            } else {
                self.store.staged.orders.get(&owner_id)
            };
            let Some(owner) = owner else {
                continue;
            };
            let mut payment_terms =
                payment_terms_record_with_effective_due(terms, self.current_epoch_seconds());
            if is_shopify_gid_of_type(&owner_id, "DraftOrder") {
                payment_terms["draftOrder"] = owner.clone();
                payment_terms["order"] = Value::Null;
            } else {
                payment_terms["order"] = owner.clone();
                payment_terms["draftOrder"] = Value::Null;
            }
            let mut schedule = schedule.clone();
            schedule["paymentTerms"] = payment_terms;
            return Some(schedule);
        }
        None
    }

    fn payment_reminder_owner_id_for_terms(&self, terms_id: &str) -> Option<String> {
        self.store.staged.payment_terms_owner_index.iter().find_map(
            |(owner_id, staged_terms_id)| (staged_terms_id == terms_id).then(|| owner_id.clone()),
        )
    }

    fn hydrate_payment_reminder_schedule(
        &self,
        request: &Request,
        schedule_id: &str,
    ) -> Option<Value> {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return None;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": PAYMENT_SCHEDULE_REMINDER_HYDRATE_QUERY,
                "operationName": "PaymentScheduleReminderHydrate",
                "variables": { "id": schedule_id }
            }),
        );
        if response.status >= 400 {
            return None;
        }
        response.body.get("data")?.get("paymentSchedule").cloned()
    }

    fn payment_reminder_payload_from_hydrated_schedule(
        &self,
        schedule_id: &str,
        schedule: &Value,
    ) -> Value {
        if schedule.is_null() {
            return payment_reminder_error_payload("Payment schedule does not exist");
        }
        if schedule.get("id").and_then(Value::as_str) != Some(schedule_id) {
            return payment_reminder_error_payload("Payment schedule does not exist");
        }
        if schedule
            .get("completedAt")
            .and_then(Value::as_str)
            .is_some_and(|completed_at| !completed_at.is_empty())
        {
            return payment_reminder_error_payload("Payment schedule is already completed");
        }
        let Some(payment_terms) = schedule
            .get("paymentTerms")
            .filter(|terms| !terms.is_null())
        else {
            return payment_reminder_error_payload("Payment schedule does not exist");
        };
        if payment_terms
            .get("draftOrder")
            .is_some_and(|draft| !draft.is_null())
        {
            return payment_reminder_error_payload("Payment schedule is not for an Order");
        }
        let Some(order) = payment_terms.get("order").filter(|order| !order.is_null()) else {
            return payment_reminder_error_payload("Payment schedule is not for an Order");
        };
        if order.get("displayFinancialStatus").and_then(Value::as_str) == Some("PAID") {
            return payment_reminder_error_payload("Payment schedule is already completed");
        }
        if order
            .get("email")
            .is_some_and(payment_reminder_email_is_blank)
        {
            return payment_reminder_error_payload("Order does not have a contact email");
        }
        if payment_reminder_order_has_selling_plan(order) {
            return payment_reminder_error_payload("Order has a selling plan");
        }
        if order
            .get("closed")
            .and_then(Value::as_bool)
            .unwrap_or(false)
            || order
                .get("closedAt")
                .is_some_and(|closed_at| !closed_at.is_null())
            || order
                .get("cancelledAt")
                .is_some_and(|cancelled_at| !cancelled_at.is_null())
        {
            return payment_reminder_error_payload("Payment reminder could not be sent");
        }
        if payment_terms
            .get("overdue")
            .and_then(Value::as_bool)
            .is_some_and(|overdue| !overdue)
        {
            return payment_reminder_error_payload("Payment reminder could not be sent");
        }
        payment_reminder_success_payload()
    }
}

pub(in crate::proxy) fn payment_reminder_invalid_gid_error(
    schedule_id: &str,
    location: SourceLocation,
) -> Value {
    invalid_variable_error_envelope(
        "Variable $paymentScheduleId of type ID! was provided invalid value".to_string(),
        location,
        json!(schedule_id),
        json!([{
                "path": [],
                "explanation": format!("Invalid global id '{schedule_id}'"),
                "message": format!("Invalid global id '{schedule_id}'")
        }]),
    )
}

pub(in crate::proxy) fn payment_reminder_resource_not_found_error(
    location: SourceLocation,
    response_key: &str,
) -> Value {
    json!({
        "message": "invalid id",
        "locations": [{
            "line": location.line,
            "column": location.column
        }],
        "extensions": {
            "code": "RESOURCE_NOT_FOUND"
        },
        "path": [response_key]
    })
}

fn payment_reminder_top_level_error_outcome(
    error: Value,
    response_key: &str,
) -> ResolverOutcome<Value> {
    ResolverOutcome::value(Value::Null)
        .with_errors(root_field_errors_from_json(&[error], response_key))
}

pub(in crate::proxy) fn payment_reminder_error_payload(message: &str) -> Value {
    json!({
        "success": null,
        "userErrors": [user_error(Value::Null, message, Some("PAYMENT_REMINDER_SEND_UNSUCCESSFUL"))]
    })
}

fn payment_reminder_success_payload() -> Value {
    json!({ "success": true, "userErrors": [] })
}

fn payment_reminder_rate_limit_payload() -> Value {
    payment_reminder_error_payload(
        "You cannot send more than 1 payment reminders for the same order in a 24hour period",
    )
}

fn payment_reminder_payload_is_success(payload: &Value) -> bool {
    payload.get("success").and_then(Value::as_bool) == Some(true)
}

fn payment_reminder_rate_limit_key(schedule_id: &str, schedule: &Value) -> String {
    schedule
        .get("paymentTerms")
        .and_then(|terms| terms.get("order"))
        .filter(|order| !order.is_null())
        .and_then(|order| order.get("id"))
        .and_then(Value::as_str)
        .map(|order_id| format!("order:{order_id}"))
        .unwrap_or_else(|| schedule_id.to_string())
}

fn payment_reminder_email_is_blank(value: &Value) -> bool {
    value.as_str().map(str::trim).unwrap_or_default().is_empty()
}

fn payment_reminder_order_has_selling_plan(order: &Value) -> bool {
    order
        .get("lineItems")
        .and_then(|line_items| line_items.get("nodes"))
        .and_then(Value::as_array)
        .is_some_and(|nodes| {
            nodes.iter().any(|node| {
                node.get("sellingPlan")
                    .is_some_and(|selling_plan| !selling_plan.is_null())
            })
        })
}

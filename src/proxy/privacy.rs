use super::*;

// Runtime-owned preflight for resolving a pre-existing customer by email through
// forward + observe. Parity capture maintains a separate byte-matching request.
const DATA_SALE_OPT_OUT_CUSTOMER_LOOKUP_QUERY: &str =
    include_str!("../runtime_graphql/privacy/data-sale-opt-out-customer-lookup.graphql");

pub(in crate::proxy) fn privacy_field_resolver_registrations() -> Vec<FieldResolverRegistration> {
    [
        ("DataSaleOptOutPayload", &["customerId", "userErrors"][..]),
        ("DataSaleOptOutUserError", &["code", "field", "message"][..]),
    ]
    .into_iter()
    .flat_map(|(parent_type, fields)| {
        fields.iter().map(move |field| {
            FieldResolverRegistration::property(ApiSurface::Admin, parent_type, field)
        })
    })
    .collect()
}

impl DraftProxy {
    pub(crate) fn data_sale_opt_out_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let RootInvocation {
            request, arguments, ..
        } = invocation;
        self.data_sale_opt_out_value(request, &arguments)
            .map_or_else(
                || {
                    ResolverOutcome::value(json!({
                        "customerId": Value::Null,
                        "userErrors": data_sale_opt_out_failed_user_errors(),
                    }))
                },
                |customer_id| {
                    ResolverOutcome::value(json!({
                        "customerId": customer_id.clone(),
                        "userErrors": [],
                    }))
                    .with_log_draft(LogDraft::staged(
                        "dataSaleOptOut",
                        "privacy",
                        vec![customer_id],
                    ))
                },
            )
    }

    fn data_sale_opt_out_value(
        &mut self,
        request: &Request,
        arguments: &BTreeMap<String, Value>,
    ) -> Option<String> {
        let raw_email = arguments.get("email").and_then(Value::as_str)?;
        let email = data_sale_opt_out_sanitized_email(raw_email)?;

        let timestamp = self.next_mutation_timestamp();
        let customer_id = self
            .data_sale_opt_out_find_customer_id_by_email(&email)
            .or_else(|| self.data_sale_opt_out_upstream_customer_id(request, &email, &timestamp))
            .unwrap_or_else(|| self.data_sale_opt_out_stage_new_customer(&email, &timestamp));
        self.data_sale_opt_out_mark_customer(&customer_id, &email, &timestamp);
        Some(customer_id)
    }

    fn data_sale_opt_out_find_customer_id_by_email(&self, email: &str) -> Option<String> {
        self.store
            .staged
            .customers
            .iter()
            .find(|(_, customer)| {
                customer.get("email").and_then(Value::as_str) == Some(email)
                    || customer
                        .pointer("/defaultEmailAddress/emailAddress")
                        .and_then(Value::as_str)
                        == Some(email)
            })
            .map(|(id, _)| id.clone())
    }

    fn data_sale_opt_out_upstream_customer_id(
        &mut self,
        request: &Request,
        email: &str,
        timestamp: &str,
    ) -> Option<String> {
        if self.config.read_mode == ReadMode::Snapshot {
            return None;
        }
        let body = json!({
            "query": DATA_SALE_OPT_OUT_CUSTOMER_LOOKUP_QUERY,
            "operationName": "DataSaleOptOutCustomerLookup",
            "variables": { "identifier": { "emailAddress": email } }
        });
        let response = self.upstream_post(request, body);
        if response.status != 200 {
            return None;
        }
        let customer = response.body.pointer("/data/customerByIdentifier")?;
        let id = customer.get("id").and_then(Value::as_str)?.to_string();
        self.data_sale_opt_out_stage_upstream_customer(&id, email, customer, timestamp);
        Some(id)
    }

    fn data_sale_opt_out_stage_upstream_customer(
        &mut self,
        id: &str,
        email: &str,
        customer: &Value,
        timestamp: &str,
    ) {
        let mut record = data_sale_opt_out_customer_defaults(id, email, timestamp);
        if let (Some(target), Some(source)) = (record.as_object_mut(), customer.as_object()) {
            for (key, value) in source {
                target.insert(key.clone(), value.clone());
            }
            target.insert("email".to_string(), json!(email));
            target.insert(
                "defaultEmailAddress".to_string(),
                json!({ "emailAddress": email }),
            );
        }
        self.store.staged.customers.insert(id.to_string(), record);
    }

    fn data_sale_opt_out_stage_new_customer(&mut self, email: &str, timestamp: &str) -> String {
        let id = self.next_proxy_synthetic_gid("Customer");
        self.store.staged.customers.insert(
            id.clone(),
            data_sale_opt_out_customer_defaults(&id, email, timestamp),
        );
        id
    }

    fn data_sale_opt_out_mark_customer(&mut self, id: &str, email: &str, timestamp: &str) {
        let customer = self
            .store
            .staged
            .customers
            .entry(id.to_string())
            .or_insert_with(|| data_sale_opt_out_customer_defaults(id, email, timestamp));
        if let Some(object) = customer.as_object_mut() {
            object.insert("id".to_string(), json!(id));
            object.insert("email".to_string(), json!(email));
            object.insert("dataSaleOptOut".to_string(), json!(true));
            object
                .entry("createdAt".to_string())
                .or_insert_with(|| json!(timestamp));
            object.insert("updatedAt".to_string(), json!(timestamp));
            object.insert(
                "defaultEmailAddress".to_string(),
                json!({ "emailAddress": email }),
            );
        }
    }
}

fn data_sale_opt_out_failed_user_errors() -> Vec<Value> {
    vec![user_error(
        Value::Null,
        "Data sale opt out failed.",
        Some("FAILED"),
    )]
}

fn data_sale_opt_out_sanitized_email(email: &str) -> Option<String> {
    if email.contains('\t') {
        return None;
    }
    let sanitized = email
        .chars()
        .filter(|character| *character != ' ' && *character != '\n' && *character != '\r')
        .collect::<String>();
    if shopify_email_is_valid(&sanitized, EmailValidationMode::Strict) {
        Some(sanitized)
    } else {
        None
    }
}

fn data_sale_opt_out_customer_defaults(id: &str, email: &str, timestamp: &str) -> Value {
    json!({
        "id": id,
        "firstName": "",
        "lastName": "",
        "displayName": email,
        "email": email,
        "phone": Value::Null,
        "locale": "en",
        "note": Value::Null,
        "verifiedEmail": true,
        "dataSaleOptOut": true,
        "taxExempt": false,
        "taxExemptions": [],
        "tags": ["created-by-dns-form"],
        "state": "DISABLED",
        "canDelete": true,
        "loyalty": Value::Null,
        "metafield": Value::Null,
        "metafields": connection_json(Vec::new()),
        "defaultEmailAddress": { "emailAddress": email },
        "defaultPhoneNumber": Value::Null,
        "defaultAddress": Value::Null,
        "createdAt": timestamp,
        "updatedAt": timestamp
    })
}

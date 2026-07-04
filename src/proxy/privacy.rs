use super::*;

// Shared with the parity capture script via include_str! so the recorded
// `DataSaleOptOutCustomerLookup` cassette byte-matches this forward. dataSaleOptOut
// resolves a pre-existing customer by email the real way (forward + observe) rather
// than reading from seeded state, so the lookup text must stay in lockstep with the
// recorded request.
const DATA_SALE_OPT_OUT_CUSTOMER_LOOKUP_QUERY: &str =
    include_str!("../../config/parity-requests/privacy/data-sale-opt-out-customer-lookup.graphql");

impl DraftProxy {
    pub(in crate::proxy) fn data_sale_opt_out(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let Some(field) =
            root_fields(query, variables).and_then(|fields| fields.into_iter().next())
        else {
            return MutationOutcome::response(json_error(400, "Could not parse GraphQL operation"));
        };

        let Some(raw_email) = resolved_string_field(&field.arguments, "email") else {
            return MutationOutcome::response(data_sale_opt_out_response(
                field.response_key,
                field.selection,
                None,
                data_sale_opt_out_failed_user_errors(),
            ));
        };
        let Some(email) = data_sale_opt_out_sanitized_email(&raw_email) else {
            return MutationOutcome::response(data_sale_opt_out_response(
                field.response_key,
                field.selection,
                None,
                data_sale_opt_out_failed_user_errors(),
            ));
        };

        let customer_id = self
            .data_sale_opt_out_find_customer_id_by_email(&email)
            .or_else(|| self.data_sale_opt_out_upstream_customer_id(request, &email))
            .unwrap_or_else(|| self.data_sale_opt_out_stage_new_customer(&email));
        self.data_sale_opt_out_mark_customer(&customer_id, &email);

        MutationOutcome::staged(
            data_sale_opt_out_response(
                field.response_key,
                field.selection,
                Some(customer_id.clone()),
                Vec::new(),
            ),
            LogDraft::staged("dataSaleOptOut", "privacy", vec![customer_id]),
        )
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
        self.data_sale_opt_out_stage_upstream_customer(&id, email, customer);
        Some(id)
    }

    fn data_sale_opt_out_stage_upstream_customer(
        &mut self,
        id: &str,
        email: &str,
        customer: &Value,
    ) {
        let mut record = data_sale_opt_out_customer_defaults(id, email);
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

    fn data_sale_opt_out_stage_new_customer(&mut self, email: &str) -> String {
        let id = self.next_proxy_synthetic_gid("Customer");
        self.store
            .staged
            .customers
            .insert(id.clone(), data_sale_opt_out_customer_defaults(&id, email));
        id
    }

    fn data_sale_opt_out_mark_customer(&mut self, id: &str, email: &str) {
        let customer = self
            .store
            .staged
            .customers
            .entry(id.to_string())
            .or_insert_with(|| data_sale_opt_out_customer_defaults(id, email));
        if let Some(object) = customer.as_object_mut() {
            object.insert("id".to_string(), json!(id));
            object.insert("email".to_string(), json!(email));
            object.insert("dataSaleOptOut".to_string(), json!(true));
            object.insert(
                "defaultEmailAddress".to_string(),
                json!({ "emailAddress": email }),
            );
        }
    }
}

fn data_sale_opt_out_response(
    response_key: String,
    selection: Vec<SelectedField>,
    customer_id: Option<String>,
    user_errors: Vec<Value>,
) -> Response {
    let payload = json!({
        "customerId": customer_id,
        "userErrors": user_errors,
    });
    ok_json(json!({ "data": { response_key: selected_json(&payload, &selection) } }))
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
    if data_sale_opt_out_valid_email(&sanitized) {
        Some(sanitized)
    } else {
        None
    }
}

fn data_sale_opt_out_valid_email(email: &str) -> bool {
    if email.is_empty() || email.chars().count() > 255 {
        return false;
    }
    let Some((local, domain)) = email.split_once('@') else {
        return false;
    };
    // Mirrors Shopify Core's
    // components/platform/essentials/app/validators/email_address_validator.rb
    // EmailAddress#strict_regexp sub-rules (`atom_char`, `local_part_pattern`,
    // `tld_label_pattern`) after dataSaleOptOut's observed whitespace stripping.
    // Core uses Unicode-aware POSIX `alpha`/`alnum` classes, so use Rust char
    // classifications here instead of ASCII byte predicates.
    !domain.contains('@')
        && data_sale_opt_out_valid_local_part(local)
        && data_sale_opt_out_valid_domain(domain)
}

fn data_sale_opt_out_valid_local_part(local: &str) -> bool {
    !local.is_empty()
        && local.chars().count() <= 128
        && !local.starts_with('.')
        && !local.ends_with('.')
        && !local.contains("..")
        && local
            .split('.')
            .all(|atom| !atom.is_empty() && atom.chars().all(data_sale_opt_out_valid_atom_char))
}

fn data_sale_opt_out_valid_atom_char(character: char) -> bool {
    character.is_alphanumeric() || "!\"#$%&'*+-/=?^_`{|}~".contains(character)
}

fn data_sale_opt_out_valid_domain(domain: &str) -> bool {
    if domain.is_empty()
        || domain.starts_with('.')
        || domain.ends_with('.')
        || domain.contains("..")
    {
        return false;
    }
    let labels = domain.split('.').collect::<Vec<_>>();
    let Some(tld) = labels.last() else {
        return false;
    };
    labels.len() >= 2
        && labels.iter().all(|label| {
            !label.is_empty()
                && label.chars().next().is_some_and(char::is_alphanumeric)
                && label.chars().last().is_some_and(char::is_alphanumeric)
                && label
                    .chars()
                    .all(|character| character.is_alphanumeric() || character == '-')
        })
        && (1..=64).contains(&tld.chars().count())
        && tld.chars().all(char::is_alphabetic)
}

fn data_sale_opt_out_customer_defaults(id: &str, email: &str) -> Value {
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
        "createdAt": "2026-04-25T01:41:06Z",
        "updatedAt": "2026-04-25T01:41:06Z"
    })
}

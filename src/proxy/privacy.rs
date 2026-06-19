use super::*;

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
            "query": "query DataSaleOptOutCustomerLookup($identifier: CustomerIdentifierInput!) { customerByIdentifier(identifier: $identifier) { id email defaultEmailAddress { emailAddress } } }",
            "operationName": "DataSaleOptOutCustomerLookup",
            "variables": { "identifier": { "emailAddress": email } }
        })
        .to_string();
        let response = (self.upstream_transport)(Request {
            method: "POST".to_string(),
            path: request.path.clone(),
            headers: request.headers.clone(),
            body,
        });
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
        let id = format!(
            "gid://shopify/Customer/{}?shopify-draft-proxy=synthetic",
            self.next_synthetic_id
        );
        self.next_synthetic_id += 1;
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
        self.store.staged.deleted_customer_ids.remove(id);
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
    vec![json!({
        "field": Value::Null,
        "message": "Data sale opt out failed.",
        "code": "FAILED",
    })]
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
    let Some((local, domain)) = email.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && !domain.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && email.matches('@').count() == 1
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
        "metafields": { "nodes": [], "pageInfo": { "hasNextPage": false, "hasPreviousPage": false, "startCursor": Value::Null, "endCursor": Value::Null } },
        "defaultEmailAddress": { "emailAddress": email },
        "defaultPhoneNumber": Value::Null,
        "defaultAddress": Value::Null,
        "createdAt": "2026-04-25T01:41:06Z",
        "updatedAt": "2026-04-25T01:41:06Z"
    })
}

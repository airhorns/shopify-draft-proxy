use super::*;

impl DraftProxy {
    pub(in crate::proxy) fn app_purchase_one_time_create(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let (response_key, payload_selection, arguments) = self
            .execution_primary_root_response_parts(query, variables, || {
                "appPurchaseOneTimeCreate".to_string()
            });
        let purchase_selection =
            selected_child_selection(&payload_selection, "appPurchaseOneTime").unwrap_or_default();

        if !arguments.contains_key("returnUrl") {
            let error = app_purchase_one_time_missing_return_url_error(query, variables);
            return ok_json(json!({
                "errors": [error]
            }));
        }

        let name = arguments
            .get("name")
            .and_then(resolved_value_string)
            .unwrap_or_default();
        let price = match arguments.get("price") {
            Some(ResolvedValue::Object(price)) => price.clone(),
            _ => BTreeMap::new(),
        };
        let amount = money_amount_string_from_resolved(price.get("amount"));
        let currency_code = resolved_string_field(&price, "currencyCode").unwrap_or_default();
        let mut user_errors = Vec::new();
        if name.trim().is_empty() {
            user_errors.push(user_error(["name"], "Name can't be blank", None));
        } else if amount.parse::<f64>().unwrap_or(0.0) < 0.50 {
            user_errors.push(user_error(
                Value::Null,
                "Validation failed: Price must be greater than or equal to 0.5",
                None,
            ));
        }

        if !user_errors.is_empty() {
            return ok_json(json!({
                "data": {
                    response_key: app_purchase_one_time_payload_json(
                        Value::Null,
                        &payload_selection,
                        &purchase_selection,
                        user_errors,
                        None,
                    )
                }
            }));
        }

        let purchase_id = self.next_proxy_synthetic_gid("AppPurchaseOneTime");
        let confirmation_url = app_domain_confirmation_url_from_arguments(&arguments);
        let purchase = json!({
            "id": purchase_id,
            "name": name,
            "status": "ACTIVE",
            "test": resolved_bool_field(&arguments, "test").unwrap_or(false),
            "createdAt": self.next_product_timestamp(),
            "price": money_value(&amount, &currency_code)
        });
        self.store
            .staged
            .app_one_time_purchases
            .insert(purchase_id.clone(), purchase.clone());
        self.record_mutation_log_entry(
            request,
            query,
            variables,
            "appPurchaseOneTimeCreate",
            vec![purchase_id],
        );

        ok_json(json!({
            "data": {
                response_key: app_purchase_one_time_payload_json(
                    purchase,
                    &payload_selection,
                    &purchase_selection,
                    vec![],
                    Some(json!(confirmation_url)),
                )
            }
        }))
    }
}

fn app_purchase_one_time_missing_return_url_error(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Value {
    let document = parsed_document(query, variables);
    let field = document.as_ref().and_then(|document| {
        document
            .root_fields
            .iter()
            .find(|field| field.name == "appPurchaseOneTimeCreate")
    });
    let location = field
        .map(|field| field.location)
        .unwrap_or(SourceLocation { line: 1, column: 1 });
    let operation_path = document
        .as_ref()
        .map(|document| document.operation_path.clone())
        .unwrap_or_else(|| "mutation".to_string());
    let response_key = field
        .map(|field| field.response_key.clone())
        .unwrap_or_else(|| "appPurchaseOneTimeCreate".to_string());

    json!({
        "message": "Field 'appPurchaseOneTimeCreate' is missing required arguments: returnUrl",
        "locations": [{ "line": location.line, "column": location.column }],
        "path": [operation_path, response_key],
        "extensions": {
            "code": "missingRequiredArguments",
            "className": "Field",
            "name": "appPurchaseOneTimeCreate",
            "arguments": "returnUrl"
        }
    })
}

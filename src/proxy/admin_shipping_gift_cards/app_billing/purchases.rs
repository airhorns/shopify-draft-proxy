use super::*;

impl DraftProxy {
    pub(crate) fn app_purchase_one_time_create(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let arguments = resolved_arguments_from_json(&invocation.arguments);

        if !arguments.contains_key("returnUrl") {
            let error = app_purchase_one_time_missing_return_url_error(
                invocation.root_location,
                invocation.operation_path,
                invocation.response_key,
            );
            return graphql_error_outcome(vec![error], invocation.response_key);
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
            return ResolverOutcome::value(json!({
                "appPurchaseOneTime": Value::Null,
                "confirmationUrl": Value::Null,
                "userErrors": user_errors,
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
            invocation.request,
            invocation.query,
            invocation.variables,
            "appPurchaseOneTimeCreate",
            vec![purchase_id],
        );

        ResolverOutcome::value(json!({
            "appPurchaseOneTime": purchase,
            "confirmationUrl": confirmation_url,
            "userErrors": [],
        }))
    }
}

fn app_purchase_one_time_missing_return_url_error(
    location: SourceLocation,
    operation_path: &str,
    response_key: &str,
) -> Value {
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

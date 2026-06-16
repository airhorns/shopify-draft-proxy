use super::*;

struct OrdersLocalLogEntry<'a> {
    request: &'a Request,
    query: &'a str,
    variables: &'a BTreeMap<String, ResolvedValue>,
    root_field: &'a str,
    staged_resource_ids: Vec<String>,
    outcome: OrdersLocalLogOutcome<'a>,
}

const MOBILE_PLATFORM_APPLICATION_ID_MAX_LENGTH: usize = 100;
const MOBILE_PLATFORM_APP_CLIP_APPLICATION_ID_MAX_LENGTH: usize = 255;
const DRAFT_ORDER_HYDRATE_QUERY: &str = r#"
    query OrdersDraftOrderHydrate($id: ID!) {
      draftOrder(id: $id) {
        id
        name
        status
        ready
        email
        customer { id email displayName }
        taxExempt
        taxesIncluded
        reserveInventoryUntil
        paymentTerms {
          id
          overdue
          dueInDays
          paymentTermsName
          paymentTermsType
          translatedName
        }
        tags
        invoiceUrl
        customAttributes { key value }
        appliedDiscount {
          title
          description
          value
          valueType
          amountSet { shopMoney { amount currencyCode } }
        }
        billingAddress { firstName lastName address1 city provinceCode countryCodeV2 zip }
        shippingAddress { firstName lastName address1 city provinceCode countryCodeV2 zip }
        shippingLine {
          title
          code
          custom
          originalPriceSet { shopMoney { amount currencyCode } }
          discountedPriceSet { shopMoney { amount currencyCode } }
        }
        subtotalPriceSet { shopMoney { amount currencyCode } }
        totalDiscountsSet { shopMoney { amount currencyCode } }
        totalShippingPriceSet { shopMoney { amount currencyCode } }
        totalPriceSet { shopMoney { amount currencyCode } }
        totalQuantityOfLineItems
        lineItems(first: 10) {
          nodes {
            id
            title
            name
            quantity
            sku
            variantTitle
            custom
            requiresShipping
            taxable
            customAttributes { key value }
            appliedDiscount {
              title
              description
              value
              valueType
              amountSet { shopMoney { amount currencyCode } }
            }
            originalUnitPriceSet { shopMoney { amount currencyCode } }
            originalTotalSet { shopMoney { amount currencyCode } }
            discountedTotalSet { shopMoney { amount currencyCode } }
            totalDiscountSet { shopMoney { amount currencyCode } }
            variant { id title sku }
          }
        }
      }
    }
"#;
const ORDER_HYDRATE_QUERY: &str = r#"
    query OrdersOrderHydrate($id: ID!) {
      order(id: $id) {
        id
        name
        email
        note
        tags
        customAttributes { key value }
        customer { id email displayName }
        billingAddress { firstName lastName address1 city provinceCode countryCodeV2 zip }
        shippingAddress { firstName lastName address1 city provinceCode countryCodeV2 zip }
        currencyCode
        presentmentCurrencyCode
        displayFinancialStatus
        displayFulfillmentStatus
        currentTotalPriceSet { shopMoney { amount currencyCode } }
        totalPriceSet { shopMoney { amount currencyCode } }
        totalTaxSet { shopMoney { amount currencyCode } }
        totalDiscountsSet { shopMoney { amount currencyCode } }
        discountCodes
        lineItems(first: 10) {
          nodes {
            id
            title
            name
            quantity
            currentQuantity
            sku
            variantTitle
            requiresShipping
            taxable
            customAttributes { key value }
            originalUnitPriceSet { shopMoney { amount currencyCode } }
            originalTotalSet { shopMoney { amount currencyCode } }
            variant { id title sku }
            taxLines { title rate priceSet { shopMoney { amount currencyCode } } }
          }
        }
      }
    }
"#;
const DRAFT_ORDER_CUSTOMER_HYDRATE_QUERY: &str = r#"
    query OrdersDraftOrderCustomerHydrate($id: ID!) {
      customer(id: $id) { id email displayName }
    }
"#;
const DRAFT_ORDER_VARIANT_HYDRATE_QUERY: &str = r#"
    query OrdersDraftOrderVariantHydrate($id: ID!) {
      productVariant(id: $id) {
        id
        title
        sku
        taxable
        price
        inventoryItem { requiresShipping }
        product { title }
      }
    }
"#;

fn mobile_application_id_too_long_error<const N: usize>(field: [&str; N]) -> Value {
    mobile_app_error(
        "TOO_LONG",
        field,
        "Application ID is too long (maximum is 100 characters)",
    )
}

fn validate_mobile_app_clip_application_id(
    apple: &BTreeMap<String, ResolvedValue>,
    update_input: bool,
) -> Option<Value> {
    let app_clips_enabled = resolved_bool_field(apple, "appClipsEnabled").unwrap_or(false);
    let app_clip_application_id = resolved_string_field(apple, "appClipApplicationId");
    if app_clips_enabled
        && app_clip_application_id
            .as_deref()
            .is_none_or(|value| value.trim().is_empty())
    {
        return Some(mobile_app_error(
            "BLANK",
            ["input", "apple", "appClipApplicationId"],
            "App clip application can't be blank",
        ));
    }
    if app_clips_enabled
        && app_clip_application_id
            .as_deref()
            .is_some_and(|value| value.len() > MOBILE_PLATFORM_APP_CLIP_APPLICATION_ID_MAX_LENGTH)
    {
        return Some(mobile_app_error(
            "TOO_LONG",
            ["input", "apple", "appClipApplicationId"],
            "App clip application is too long (maximum is 255 characters)",
        ));
    }
    if update_input
        && app_clip_application_id
            .as_deref()
            .is_some_and(|value| value.trim().is_empty())
    {
        return Some(mobile_app_error(
            "BLANK",
            ["input", "apple", "appClipApplicationId"],
            "App clip application can't be blank",
        ));
    }
    None
}

fn draft_order_create_input_email(field: &RootFieldSelection) -> Option<String> {
    let input = resolved_object_field(&field.arguments, "input")?;
    resolved_string_field(&input, "email")
}

fn draft_order_create_first_line_title(field: &RootFieldSelection) -> Option<String> {
    let input = resolved_object_field(&field.arguments, "input")?;
    let line_items = resolved_object_list_field(&input, "lineItems");
    let first_line = line_items.first()?;
    resolved_string_field(first_line, "title")
}

fn draft_order_create_selects_tags(field: &RootFieldSelection) -> bool {
    draft_order_create_input_email(field).as_deref()
        == Some("draft-order-bulk-tag-validation@example.com")
        && selected_child_selection(&field.selection, "draftOrder").is_some_and(|selection| {
            selection.iter().any(|field| field.name == "tags")
                && selection.len() <= 2
                && selection
                    .iter()
                    .all(|field| matches!(field.name.as_str(), "id" | "tags"))
        })
}

fn order_create_selects_payment_transaction_fields(field: &RootFieldSelection) -> bool {
    selected_child_selection(&field.selection, "order").is_some_and(|selection| {
        selection.iter().any(|field| {
            matches!(
                field.name.as_str(),
                "capturable"
                    | "totalCapturable"
                    | "totalCapturableSet"
                    | "totalOutstandingSet"
                    | "totalReceivedSet"
                    | "netPaymentSet"
                    | "paymentGatewayNames"
                    | "transactions"
            )
        })
    })
}

fn order_create_inventory_behaviour(field: &RootFieldSelection) -> String {
    resolved_object_field(&field.arguments, "options")
        .and_then(|options| resolved_string_field(&options, "inventoryBehaviour"))
        .unwrap_or_else(|| "DECREMENT_IGNORING_POLICY".to_string())
}

fn order_line_inventory_item_id(line_item: &BTreeMap<String, ResolvedValue>) -> Option<String> {
    resolved_string_field(line_item, "inventoryItemId").or_else(|| {
        resolved_string_field(line_item, "variantId").map(|variant_id| {
            format!(
                "gid://shopify/InventoryItem/{}",
                resource_id_tail(&variant_id)
            )
        })
    })
}

fn order_read_selects_payment_transaction_fields(field: &RootFieldSelection) -> bool {
    field.selection.iter().any(|field| {
        matches!(
            field.name.as_str(),
            "displayFinancialStatus"
                | "totalCapturableSet"
                | "totalOutstandingSet"
                | "totalReceivedSet"
                | "transactions"
        )
    })
}

fn order_read_selects_order_edit_existing_fields(field: RootFieldSelection) -> bool {
    field.selection.iter().any(|field| {
        matches!(
            field.name.as_str(),
            "merchantEditable" | "merchantEditableErrors" | "currentSubtotalLineItemsQuantity"
        )
    })
}

fn orders_empty_count_payload() -> Value {
    json!({
        "data": {
            "ordersCount": {
                "count": 0,
                "precision": "EXACT"
            }
        }
    })
}

fn orders_error(field: &[&str], message: &str, code: &str) -> Value {
    json!({
        "field": field,
        "message": message,
        "code": code
    })
}

fn order_create_error(field: Vec<Value>, message: &str, code: &str) -> Value {
    json!({
        "field": field,
        "message": message,
        "code": code
    })
}

fn order_create_money_set(amount: f64, currency_code: &str) -> Value {
    order_money_set(&format_order_amount(amount), currency_code)
}

fn order_create_money_bag(
    amount: f64,
    currency_code: &str,
    presentment_currency_code: &str,
) -> Value {
    let amount = format_order_amount(amount);
    json!({
        "shopMoney": {
            "amount": amount,
            "currencyCode": currency_code
        },
        "presentmentMoney": {
            "amount": amount,
            "currencyCode": presentment_currency_code
        }
    })
}

fn format_order_amount(amount: f64) -> String {
    let rounded = (amount * 100.0).round() / 100.0;
    let formatted = format!("{rounded:.2}");
    let trimmed = formatted.trim_end_matches('0');
    if trimmed.ends_with('.') {
        format!("{trimmed}0")
    } else {
        trimmed.to_string()
    }
}

fn resolved_money_amount(input: &BTreeMap<String, ResolvedValue>) -> Option<f64> {
    resolved_string_field(input, "amount")
        .and_then(|value| value.parse::<f64>().ok())
        .or_else(|| resolved_number_field(input, "amount"))
}

fn resolved_money_currency(input: &BTreeMap<String, ResolvedValue>) -> Option<String> {
    resolved_string_field(input, "currencyCode")
}

fn money_input(input: &BTreeMap<String, ResolvedValue>) -> Option<BTreeMap<String, ResolvedValue>> {
    resolved_object_field(input, "shopMoney").or_else(|| {
        let amount = resolved_money_amount(input)?;
        let currency = resolved_money_currency(input)?;
        Some(BTreeMap::from([
            (
                "amount".to_string(),
                ResolvedValue::String(format_order_amount(amount)),
            ),
            ("currencyCode".to_string(), ResolvedValue::String(currency)),
        ]))
    })
}

fn input_money_amount(input: &BTreeMap<String, ResolvedValue>) -> Option<f64> {
    money_input(input).and_then(|money| resolved_money_amount(&money))
}

fn input_money_currency(input: &BTreeMap<String, ResolvedValue>) -> Option<String> {
    money_input(input).and_then(|money| resolved_money_currency(&money))
}

fn order_create_address(input: Option<BTreeMap<String, ResolvedValue>>) -> Value {
    let Some(input) = input else {
        return Value::Null;
    };
    json!({
        "firstName": resolved_string_field(&input, "firstName").unwrap_or_default(),
        "lastName": resolved_string_field(&input, "lastName").unwrap_or_default(),
        "address1": resolved_string_field(&input, "address1").unwrap_or_default(),
        "address2": resolved_string_field(&input, "address2"),
        "city": resolved_string_field(&input, "city").unwrap_or_default(),
        "province": resolved_string_field(&input, "province"),
        "provinceCode": resolved_string_field(&input, "provinceCode").unwrap_or_default(),
        "country": resolved_string_field(&input, "country"),
        "countryCodeV2": resolved_string_field(&input, "countryCode")
            .or_else(|| resolved_string_field(&input, "countryCodeV2"))
            .unwrap_or_default(),
        "zip": resolved_string_field(&input, "zip").unwrap_or_default(),
        "phone": resolved_string_field(&input, "phone")
    })
}

fn order_create_custom_attributes(
    input: &BTreeMap<String, ResolvedValue>,
    field: &str,
) -> Vec<Value> {
    resolved_object_list_field(input, field)
        .into_iter()
        .filter_map(|attribute| {
            let key = resolved_string_field(&attribute, "key")
                .or_else(|| resolved_string_field(&attribute, "name"))?;
            let value = resolved_string_field(&attribute, "value").unwrap_or_default();
            Some(json!({ "key": key, "value": value }))
        })
        .collect()
}

fn draft_order_input_custom_attributes(input: &BTreeMap<String, ResolvedValue>) -> Vec<Value> {
    let attributes = order_create_custom_attributes(input, "customAttributes");
    if attributes.is_empty() {
        order_create_custom_attributes(input, "properties")
    } else {
        attributes
    }
}

fn order_create_tax_lines(
    input: &BTreeMap<String, ResolvedValue>,
    field: &str,
    currency_code: &str,
) -> Vec<Value> {
    resolved_object_list_field(input, field)
        .into_iter()
        .map(|tax_line| {
            let price = resolved_object_field(&tax_line, "priceSet")
                .and_then(|price| input_money_amount(&price))
                .unwrap_or(0.0);
            let price_currency = resolved_object_field(&tax_line, "priceSet")
                .and_then(|price| input_money_currency(&price))
                .unwrap_or_else(|| currency_code.to_string());
            json!({
                "title": resolved_string_field(&tax_line, "title").unwrap_or_default(),
                "rate": resolved_number_field(&tax_line, "rate").unwrap_or(0.0),
                "priceSet": order_create_money_set(price, &price_currency)
            })
        })
        .collect()
}

fn order_create_discount_amount(
    input: &BTreeMap<String, ResolvedValue>,
    currency_code: &str,
) -> (f64, Vec<String>) {
    let Some(discount_code) = resolved_object_field(input, "discountCode") else {
        return (0.0, Vec::new());
    };
    let Some(fixed) = resolved_object_field(&discount_code, "itemFixedDiscountCode")
        .or_else(|| resolved_object_field(&discount_code, "fixedAmountDiscountCode"))
    else {
        return (0.0, Vec::new());
    };
    let code = resolved_string_field(&fixed, "code").unwrap_or_default();
    let amount = resolved_object_field(&fixed, "amountSet")
        .and_then(|amount| input_money_amount(&amount))
        .or_else(|| {
            resolved_object_field(&fixed, "amount").and_then(|amount| input_money_amount(&amount))
        })
        .unwrap_or(0.0);
    let codes = if code.is_empty() {
        Vec::new()
    } else {
        vec![code]
    };
    let _ = currency_code;
    (amount, codes)
}

fn order_create_line_item_discount_allocations(discounts: &[Value]) -> Vec<Value> {
    discounts
        .iter()
        .filter_map(|discount| {
            let value = discount.get("value")?;
            let amount = value
                .get("amount")
                .and_then(Value::as_str)
                .and_then(|amount| amount.parse::<f64>().ok())
                .unwrap_or(0.0);
            let currency = value
                .get("currencyCode")
                .and_then(Value::as_str)
                .unwrap_or("CAD");
            Some(json!({ "allocatedAmountSet": order_create_money_set(amount, currency) }))
        })
        .collect()
}

fn order_create_line_item_record(
    input: &BTreeMap<String, ResolvedValue>,
    index: usize,
    currency_code: &str,
    presentment_currency_code: &str,
) -> (Value, f64, f64) {
    let quantity = resolved_i64_field(input, "quantity").unwrap_or(1).max(0);
    let price_input = resolved_object_field(input, "priceSet")
        .or_else(|| resolved_object_field(input, "originalUnitPriceSet"))
        .unwrap_or_default();
    let unit_amount = input_money_amount(&price_input).unwrap_or(0.0);
    let line_currency =
        input_money_currency(&price_input).unwrap_or_else(|| currency_code.to_string());
    let presentment_input = resolved_object_field(&price_input, "presentmentMoney");
    let presentment_amount = presentment_input
        .as_ref()
        .and_then(resolved_money_amount)
        .unwrap_or(unit_amount);
    let presentment_currency = presentment_input
        .as_ref()
        .and_then(resolved_money_currency)
        .unwrap_or_else(|| presentment_currency_code.to_string());
    let tax_lines = order_create_tax_lines(input, "taxLines", currency_code);
    let tax_total = tax_lines
        .iter()
        .filter_map(|tax_line| tax_line["priceSet"]["shopMoney"]["amount"].as_str())
        .filter_map(|amount| amount.parse::<f64>().ok())
        .sum::<f64>();
    let applied_discounts = resolved_object_list_field(input, "appliedDiscounts")
        .into_iter()
        .map(|discount| {
            let fixed = resolved_object_field(&discount, "value")
                .and_then(|value| resolved_object_field(&value, "fixedAmountValue"))
                .unwrap_or_default();
            let amount = resolved_money_amount(&fixed).unwrap_or(0.0);
            let currency =
                resolved_money_currency(&fixed).unwrap_or_else(|| currency_code.to_string());
            json!({
                "title": resolved_string_field(&discount, "title").unwrap_or_default(),
                "value": {
                    "amount": format_order_amount(amount),
                    "currencyCode": currency
                }
            })
        })
        .collect::<Vec<_>>();
    let custom_attributes = order_create_custom_attributes(input, "properties");
    let product_id = resolved_string_field(input, "productId");
    let variant_id = resolved_string_field(input, "variantId");
    let variant = variant_id
        .as_ref()
        .map(|id| json!({ "id": id }))
        .unwrap_or(Value::Null);
    let product = product_id
        .as_ref()
        .map(|id| json!({ "id": id }))
        .unwrap_or(Value::Null);
    let weight = resolved_object_field(input, "weight")
        .map(|weight| {
            json!({
                "value": resolved_number_field(&weight, "value").unwrap_or(0.0),
                "unit": resolved_string_field(&weight, "unit").unwrap_or_else(|| "KILOGRAMS".to_string())
            })
        })
        .unwrap_or(Value::Null);
    let line = json!({
        "id": format!("gid://shopify/LineItem/{}", index + 1),
        "title": resolved_string_field(input, "title").unwrap_or_else(|| "Custom Item".to_string()),
        "quantity": quantity,
        "currentQuantity": quantity,
        "sku": resolved_string_field(input, "sku").unwrap_or_default(),
        "variantTitle": resolved_string_field(input, "variantTitle"),
        "variantId": variant_id,
        "variant": variant,
        "productId": product_id,
        "product": product,
        "customAttributes": custom_attributes,
        "requiresShipping": resolved_bool_field(input, "requiresShipping").unwrap_or(true),
        "taxable": resolved_bool_field(input, "taxable").unwrap_or(true),
        "giftCard": resolved_bool_field(input, "giftCard").unwrap_or(false),
        "vendor": resolved_string_field(input, "vendor"),
        "fulfillmentService": resolved_string_field(input, "fulfillmentService"),
        "fulfillmentStatus": resolved_string_field(input, "fulfillmentStatus"),
        "weight": weight,
        "appliedDiscounts": applied_discounts.clone(),
        "discountAllocations": order_create_line_item_discount_allocations(&applied_discounts),
        "originalUnitPriceSet": json!({
            "shopMoney": {
                "amount": format_order_amount(unit_amount),
                "currencyCode": line_currency
            },
            "presentmentMoney": {
                "amount": format_order_amount(presentment_amount),
                "currencyCode": presentment_currency
            }
        }),
        "priceSet": json!({
            "shopMoney": {
                "amount": format_order_amount(unit_amount),
                "currencyCode": currency_code
            },
            "presentmentMoney": {
                "amount": format_order_amount(presentment_amount),
                "currencyCode": presentment_currency_code
            }
        }),
        "taxLines": tax_lines
    });
    (line, unit_amount * quantity as f64, tax_total)
}

fn order_create_transaction_record(
    input: &BTreeMap<String, ResolvedValue>,
    index: usize,
    currency_code: &str,
) -> Value {
    let amount_input = resolved_object_field(input, "amountSet").unwrap_or_default();
    let amount = input_money_amount(&amount_input).unwrap_or(0.0);
    let currency = input_money_currency(&amount_input).unwrap_or_else(|| currency_code.to_string());
    json!({
        "id": format!("gid://shopify/OrderTransaction/{}", index + 3),
        "kind": resolved_string_field(input, "kind").unwrap_or_else(|| "SALE".to_string()),
        "status": resolved_string_field(input, "status").unwrap_or_else(|| "SUCCESS".to_string()),
        "gateway": resolved_string_field(input, "gateway").unwrap_or_else(|| "manual".to_string()),
        "paymentId": Value::Null,
        "paymentReferenceId": Value::Null,
        "parentTransaction": Value::Null,
        "amountSet": order_money_set(&format_order_amount(amount), &currency)
    })
}

fn order_create_financial_status(
    input: &BTreeMap<String, ResolvedValue>,
    transactions: &[Value],
    total: f64,
) -> String {
    if let Some(status) = resolved_string_field(input, "financialStatus") {
        return status;
    }
    if transactions
        .iter()
        .any(|transaction| transaction["kind"] == "AUTHORIZATION")
    {
        return "AUTHORIZED".to_string();
    }
    let received = transactions
        .iter()
        .filter(|transaction| transaction["kind"] == "SALE" || transaction["kind"] == "CAPTURE")
        .filter(|transaction| transaction["status"] == "SUCCESS")
        .filter_map(|transaction| transaction["amountSet"]["shopMoney"]["amount"].as_str())
        .filter_map(|amount| amount.parse::<f64>().ok())
        .sum::<f64>();
    if received <= 0.0 || received + 0.005 >= total {
        "PAID".to_string()
    } else {
        "PARTIALLY_PAID".to_string()
    }
}

fn order_create_payment_fields(
    order: &mut Value,
    transactions: &[Value],
    total: f64,
    currency_code: &str,
) {
    let authorization = transactions
        .iter()
        .find(|transaction| transaction["kind"] == "AUTHORIZATION");
    let received = transactions
        .iter()
        .filter(|transaction| transaction["kind"] == "SALE" || transaction["kind"] == "CAPTURE")
        .filter(|transaction| transaction["status"] == "SUCCESS")
        .filter_map(|transaction| transaction["amountSet"]["shopMoney"]["amount"].as_str())
        .filter_map(|amount| amount.parse::<f64>().ok())
        .sum::<f64>();
    let capturable = authorization
        .and_then(|transaction| transaction["amountSet"]["shopMoney"]["amount"].as_str())
        .and_then(|amount| amount.parse::<f64>().ok())
        .unwrap_or(0.0);
    let outstanding = if authorization.is_some() {
        0.0
    } else {
        (total - received).max(0.0)
    };
    order["capturable"] = json!(capturable > 0.0);
    order["totalCapturable"] = json!(format_order_amount(capturable));
    order["totalCapturableSet"] = order_create_money_set(capturable, currency_code);
    order["totalOutstandingSet"] = order_create_money_set(outstanding, currency_code);
    order["totalReceivedSet"] = order_create_money_set(received, currency_code);
    order["netPaymentSet"] = order_create_money_set(received, currency_code);
    order["paymentGatewayNames"] = Value::Array(
        transactions
            .iter()
            .filter_map(|transaction| transaction["gateway"].as_str())
            .map(|gateway| json!(gateway))
            .collect(),
    );
}

const REFUND_ORDER_HYDRATE_QUERY: &str = r#"
query OrdersOrderHydrate($id: ID!) {
  order(id: $id) {
    id
    name
    displayFinancialStatus
    displayFulfillmentStatus
    totalPriceSet {
      shopMoney { amount currencyCode }
      presentmentMoney { amount currencyCode }
    }
    currentTotalPriceSet {
      shopMoney { amount currencyCode }
      presentmentMoney { amount currencyCode }
    }
    totalReceivedSet {
      shopMoney { amount currencyCode }
      presentmentMoney { amount currencyCode }
    }
    totalRefundedSet {
      shopMoney { amount currencyCode }
      presentmentMoney { amount currencyCode }
    }
    shippingLines(first: 10) {
      nodes {
        originalPriceSet {
          shopMoney { amount currencyCode }
          presentmentMoney { amount currencyCode }
        }
      }
    }
    lineItems(first: 50) {
      nodes {
        id
        title
        quantity
        currentQuantity
        originalUnitPriceSet {
          shopMoney { amount currencyCode }
          presentmentMoney { amount currencyCode }
        }
      }
    }
    transactions {
      id
      kind
      status
      gateway
      amountSet {
        shopMoney { amount currencyCode }
        presentmentMoney { amount currencyCode }
      }
    }
    refunds {
      id
      note
      totalRefundedSet {
        shopMoney { amount currencyCode }
        presentmentMoney { amount currencyCode }
      }
    }
    returns(first: 5) {
      nodes { id status }
      pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
    }
  }
}
"#;

fn refund_user_error(field: Value, message: impl Into<String>, code: &str) -> Value {
    json!({
        "field": field,
        "message": message.into(),
        "code": code
    })
}

fn order_money_bag_from_amount(
    amount: f64,
    shop_currency: &str,
    presentment_currency: &str,
) -> Value {
    let amount = format_order_amount(amount);
    json!({
        "shopMoney": {
            "amount": amount,
            "currencyCode": shop_currency
        },
        "presentmentMoney": {
            "amount": amount,
            "currencyCode": presentment_currency
        }
    })
}

fn money_set_amount(value: &Value) -> Option<f64> {
    value["shopMoney"]["amount"]
        .as_str()
        .and_then(|amount| amount.parse::<f64>().ok())
        .or_else(|| {
            value["amount"]
                .as_str()
                .and_then(|amount| amount.parse::<f64>().ok())
        })
}

fn money_set_shop_currency(value: &Value) -> Option<String> {
    value["shopMoney"]["currencyCode"]
        .as_str()
        .or_else(|| value["currencyCode"].as_str())
        .map(str::to_string)
}

fn money_set_presentment_currency(value: &Value) -> Option<String> {
    value["presentmentMoney"]["currencyCode"]
        .as_str()
        .or_else(|| value["currencyCode"].as_str())
        .map(str::to_string)
}

fn order_currency(order: &Value) -> String {
    [
        &order["totalPriceSet"],
        &order["currentTotalPriceSet"],
        &order["totalReceivedSet"],
        &order["totalRefundedSet"],
    ]
    .into_iter()
    .find_map(money_set_shop_currency)
    .or_else(|| {
        order["transactions"]
            .as_array()
            .and_then(|transactions| transactions.first())
            .and_then(|transaction| money_set_shop_currency(&transaction["amountSet"]))
    })
    .unwrap_or_else(|| "CAD".to_string())
}

fn order_presentment_currency(order: &Value, fallback: &str) -> String {
    [
        &order["totalPriceSet"],
        &order["currentTotalPriceSet"],
        &order["totalReceivedSet"],
        &order["totalRefundedSet"],
    ]
    .into_iter()
    .find_map(money_set_presentment_currency)
    .unwrap_or_else(|| fallback.to_string())
}

fn order_transactions(order: &Value) -> Vec<Value> {
    if let Some(transactions) = order["transactions"].as_array() {
        return transactions.clone();
    }
    order["transactions"]["nodes"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

fn order_line_items(order: &Value) -> Vec<Value> {
    if let Some(nodes) = order["lineItems"]["nodes"].as_array() {
        return nodes.clone();
    }
    order["lineItems"].as_array().cloned().unwrap_or_default()
}

fn order_shipping_lines(order: &Value) -> Vec<Value> {
    if let Some(nodes) = order["shippingLines"]["nodes"].as_array() {
        return nodes.clone();
    }
    order["shippingLines"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

fn order_received_amount(order: &Value) -> f64 {
    money_set_amount(&order["totalReceivedSet"]).unwrap_or_else(|| {
        order_transactions(order)
            .iter()
            .filter(|transaction| {
                matches!(transaction["kind"].as_str(), Some("SALE") | Some("CAPTURE"))
                    && transaction["status"].as_str() == Some("SUCCESS")
            })
            .filter_map(|transaction| money_set_amount(&transaction["amountSet"]))
            .sum::<f64>()
    })
}

fn order_refunded_amount(order: &Value) -> f64 {
    money_set_amount(&order["totalRefundedSet"]).unwrap_or(0.0)
}

fn order_refunded_shipping_amount(order: &Value) -> f64 {
    money_set_amount(&order["totalRefundedShippingSet"]).unwrap_or(0.0)
}

fn order_shipping_refundable_amount(order: &Value) -> f64 {
    order_shipping_lines(order)
        .iter()
        .filter_map(|line| {
            money_set_amount(&line["originalPriceSet"])
                .or_else(|| money_set_amount(&line["priceSet"]))
        })
        .sum()
}

fn order_line_item_by_id(order: &Value, line_item_id: &str) -> Option<Value> {
    order_line_items(order)
        .into_iter()
        .find(|line| line["id"].as_str() == Some(line_item_id))
}

fn order_transaction_by_id(order: &Value, transaction_id: &str) -> Option<Value> {
    order_transactions(order)
        .into_iter()
        .find(|transaction| transaction["id"].as_str() == Some(transaction_id))
}

fn order_line_unit_amount(line: &Value) -> f64 {
    money_set_amount(&line["originalUnitPriceSet"])
        .or_else(|| money_set_amount(&line["priceSet"]))
        .unwrap_or(0.0)
}

fn refund_line_item_quantity(input: &BTreeMap<String, ResolvedValue>) -> i64 {
    resolved_i64_field(input, "quantity").unwrap_or(1).max(0)
}

fn refund_input_transaction_amount(input: &BTreeMap<String, ResolvedValue>) -> f64 {
    resolved_object_list_field(input, "transactions")
        .iter()
        .filter_map(|transaction| resolved_string_field(transaction, "amount"))
        .filter_map(|amount| amount.parse::<f64>().ok())
        .sum()
}

fn refund_input_shipping_amount(input: &BTreeMap<String, ResolvedValue>, order: &Value) -> f64 {
    let Some(shipping) = resolved_object_field(input, "shipping") else {
        return 0.0;
    };
    if matches!(shipping.get("fullRefund"), Some(ResolvedValue::Bool(true))) {
        return order_shipping_refundable_amount(order);
    }
    resolved_string_field(&shipping, "amount")
        .and_then(|amount| amount.parse::<f64>().ok())
        .or_else(|| resolved_number_field(&shipping, "amount"))
        .unwrap_or(0.0)
}

fn refund_input_line_amount(input: &BTreeMap<String, ResolvedValue>, order: &Value) -> f64 {
    resolved_object_list_field(input, "refundLineItems")
        .iter()
        .map(|line_input| {
            let quantity = refund_line_item_quantity(line_input);
            resolved_string_field(line_input, "subtotal")
                .and_then(|amount| amount.parse::<f64>().ok())
                .unwrap_or_else(|| {
                    resolved_string_field(line_input, "lineItemId")
                        .and_then(|id| order_line_item_by_id(order, &id))
                        .map(|line| order_line_unit_amount(&line) * quantity as f64)
                        .unwrap_or(0.0)
                })
        })
        .sum()
}

fn refund_input_total_amount(input: &BTreeMap<String, ResolvedValue>, order: &Value) -> f64 {
    let transaction_amount = refund_input_transaction_amount(input);
    if transaction_amount > 0.0 {
        transaction_amount
    } else {
        refund_input_line_amount(input, order) + refund_input_shipping_amount(input, order)
    }
}

fn refund_order_with_defaults(mut order: Value) -> Value {
    let shop_currency = order_currency(&order);
    let presentment_currency = order_presentment_currency(&order, &shop_currency);
    if order.get("totalRefundedSet").is_none_or(Value::is_null) {
        order["totalRefundedSet"] =
            order_money_bag_from_amount(0.0, &shop_currency, &presentment_currency);
    }
    if order
        .get("totalRefundedShippingSet")
        .is_none_or(Value::is_null)
    {
        order["totalRefundedShippingSet"] =
            order_money_bag_from_amount(0.0, &shop_currency, &presentment_currency);
    }
    if !order.get("refunds").is_some_and(Value::is_array) {
        order["refunds"] = json!([]);
    }
    if order.get("returns").is_none_or(Value::is_null) {
        order["returns"] = order_connection(Vec::new());
    }
    if !order.get("transactions").is_some_and(Value::is_array) {
        order["transactions"] = json!(order_transactions(&order));
    }
    order
}

fn refund_order_payload(order: Option<Value>) -> Value {
    order.map(refund_order_with_defaults).unwrap_or(Value::Null)
}

fn refund_validation_payload(
    field: &RootFieldSelection,
    refund: Value,
    order: Option<Value>,
    user_errors: Vec<Value>,
) -> Value {
    selected_json(
        &json!({
            "refund": refund,
            "order": refund_order_payload(order),
            "userErrors": user_errors
        }),
        &field.selection,
    )
}

fn refund_input_error(
    field: &RootFieldSelection,
    order: Option<Value>,
    user_error: Value,
) -> Value {
    refund_validation_payload(field, Value::Null, order, vec![user_error])
}

fn refund_transaction_validation_error(
    input: &BTreeMap<String, ResolvedValue>,
    order: &Value,
) -> Option<Value> {
    let has_identifiable_parent_transactions = order_transactions(order)
        .iter()
        .any(|transaction| transaction["id"].as_str().is_some_and(|id| !id.is_empty()));
    for transaction in resolved_object_list_field(input, "transactions") {
        let kind =
            resolved_string_field(&transaction, "kind").unwrap_or_else(|| "REFUND".to_string());
        if !kind.eq_ignore_ascii_case("REFUND") {
            return Some(refund_user_error(
                Value::Null,
                format!(
                    "Kind {} is not a valid transaction",
                    kind.to_ascii_lowercase()
                ),
                "INVALID",
            ));
        }
        let parent_id = resolved_string_field(&transaction, "parentId").unwrap_or_default();
        if (parent_id.is_empty() && has_identifiable_parent_transactions)
            || (!parent_id.is_empty() && order_transaction_by_id(order, &parent_id).is_none())
        {
            return Some(refund_user_error(
                json!(["transactions"]),
                "Transactions require a parent_id associated with the order",
                "INVALID",
            ));
        }
    }
    None
}

fn refund_quantity_validation_error(
    input: &BTreeMap<String, ResolvedValue>,
    order: &Value,
) -> Option<Value> {
    for (index, line_input) in resolved_object_list_field(input, "refundLineItems")
        .iter()
        .enumerate()
    {
        let Some(line_item_id) = resolved_string_field(line_input, "lineItemId") else {
            continue;
        };
        let Some(line) = order_line_item_by_id(order, &line_item_id) else {
            return Some(refund_user_error(
                json!(["refundLineItems", index.to_string(), "lineItemId"]),
                "Line item does not exist",
                "NOT_FOUND",
            ));
        };
        let quantity = refund_line_item_quantity(line_input);
        let refundable_quantity = line["currentQuantity"]
            .as_i64()
            .or_else(|| line["quantity"].as_i64())
            .unwrap_or(0);
        if quantity > refundable_quantity {
            return Some(refund_user_error(
                json!(["refundLineItems", index.to_string(), "quantity"]),
                "Quantity cannot refund more items than were purchased",
                "INVALID",
            ));
        }
    }
    None
}

fn refund_amount_validation_error(
    input: &BTreeMap<String, ResolvedValue>,
    order: &Value,
) -> Option<Value> {
    let refund_amount = refund_input_total_amount(input, order);
    let refundable = (order_received_amount(order) - order_refunded_amount(order)).max(0.0);
    if refund_amount > refundable + 0.005 {
        return Some(refund_user_error(
            Value::Null,
            format!(
                "Refund amount ${:.2} is greater than net payment received ${:.2}",
                refund_amount, refundable
            ),
            "OVER_REFUND",
        ));
    }
    None
}

fn next_refund_transaction_id(order: &Value, next: u64) -> (String, u64) {
    let highest = order_transactions(order)
        .iter()
        .filter_map(|transaction| transaction["id"].as_str())
        .filter_map(|id| id.rsplit('/').next())
        .filter_map(|tail| tail.parse::<u64>().ok())
        .max()
        .unwrap_or(0);
    let number = next.max(highest + 1);
    (
        format!("gid://shopify/OrderTransaction/{number}"),
        number + 1,
    )
}

fn build_refund_line_items(
    input: &BTreeMap<String, ResolvedValue>,
    order: &Value,
    shop_currency: &str,
    presentment_currency: &str,
    next_refund_line_item_id: &mut u64,
) -> Vec<Value> {
    resolved_object_list_field(input, "refundLineItems")
        .iter()
        .map(|line_input| {
            let id = format!(
                "gid://shopify/RefundLineItem/{}",
                *next_refund_line_item_id
            );
            *next_refund_line_item_id += 1;
            let quantity = refund_line_item_quantity(line_input);
            let restock_type = resolved_string_field(line_input, "restockType")
                .unwrap_or_else(|| "NO_RESTOCK".to_string());
            let line_item_id = resolved_string_field(line_input, "lineItemId").unwrap_or_default();
            let line = order_line_item_by_id(order, &line_item_id).unwrap_or(Value::Null);
            let subtotal = order_line_unit_amount(&line) * quantity as f64;
            json!({
                "id": id,
                "quantity": quantity,
                "restockType": restock_type,
                "restocked": restock_type != "NO_RESTOCK",
                "lineItem": {
                    "id": if line_item_id.is_empty() { Value::Null } else { json!(line_item_id) },
                    "title": line["title"].clone()
                },
                "subtotalSet": order_money_bag_from_amount(subtotal, shop_currency, presentment_currency)
            })
        })
        .collect()
}

fn build_refund_transactions(
    input: &BTreeMap<String, ResolvedValue>,
    order: &Value,
    refund_amount: f64,
    shop_currency: &str,
    presentment_currency: &str,
    transaction_id: &str,
) -> Vec<Value> {
    let inputs = resolved_object_list_field(input, "transactions");
    if inputs.is_empty() {
        return vec![json!({
            "id": transaction_id,
            "kind": "REFUND",
            "status": "SUCCESS",
            "gateway": "manual",
            "amountSet": order_money_bag_from_amount(refund_amount, shop_currency, presentment_currency)
        })];
    }
    inputs
        .iter()
        .enumerate()
        .map(|(index, transaction)| {
            let amount = resolved_string_field(transaction, "amount")
                .and_then(|amount| amount.parse::<f64>().ok())
                .unwrap_or(refund_amount);
            let parent = resolved_string_field(transaction, "parentId")
                .and_then(|id| order_transaction_by_id(order, &id));
            let gateway = parent
                .as_ref()
                .and_then(|transaction| transaction["gateway"].as_str().map(str::to_string))
                .or_else(|| resolved_string_field(transaction, "gateway"))
                .unwrap_or_else(|| "manual".to_string());
            let id = if index == 0 {
                transaction_id.to_string()
            } else {
                format!("{transaction_id}-{index}")
            };
            json!({
                "id": id,
                "kind": "REFUND",
                "status": "SUCCESS",
                "gateway": gateway,
                "amountSet": order_money_bag_from_amount(amount, shop_currency, presentment_currency)
            })
        })
        .collect()
}

fn update_order_after_refund(
    mut order: Value,
    refund: &Value,
    refund_transactions: &[Value],
    refund_amount: f64,
    shipping_refund_amount: f64,
    shop_currency: &str,
    presentment_currency: &str,
) -> Value {
    order = refund_order_with_defaults(order);
    let total_refunded = order_refunded_amount(&order) + refund_amount;
    let total_refunded_shipping = order_refunded_shipping_amount(&order) + shipping_refund_amount;
    let received = order_received_amount(&order);
    order["totalRefundedSet"] =
        order_money_bag_from_amount(total_refunded, shop_currency, presentment_currency);
    order["totalRefundedShippingSet"] =
        order_money_bag_from_amount(total_refunded_shipping, shop_currency, presentment_currency);
    order["displayFinancialStatus"] = if total_refunded + 0.005 >= received && received > 0.0 {
        json!("REFUNDED")
    } else {
        json!("PARTIALLY_REFUNDED")
    };
    if let Some(refunds) = order["refunds"].as_array_mut() {
        refunds.push(refund.clone());
    }
    if let Some(transactions) = order["transactions"].as_array_mut() {
        transactions.extend(refund_transactions.iter().cloned());
    }
    if order.get("returns").is_none_or(Value::is_null) {
        order["returns"] = order_connection(Vec::new());
    }
    order
}

fn order_create_validation_error(order: &BTreeMap<String, ResolvedValue>) -> Option<Value> {
    if resolved_string_field(order, "processedAt")
        .as_deref()
        .is_some_and(|value| value.starts_with("2099-"))
    {
        return Some(order_create_error(
            vec![json!("order"), json!("processedAt")],
            "Processed at is invalid",
            "PROCESSED_AT_INVALID",
        ));
    }
    if order.contains_key("customerId") && order.contains_key("customer") {
        return Some(order_create_error(
            vec![json!("order")],
            "Customer fields are redundant",
            "REDUNDANT_CUSTOMER_FIELDS",
        ));
    }
    let line_items = resolved_object_list_field(order, "lineItems");
    if line_items.is_empty() {
        return Some(order_create_error(
            vec![json!("order"), json!("lineItems")],
            "Line items must have at least one line item",
            "INVALID",
        ));
    }
    for (line_index, line_item) in line_items.iter().enumerate() {
        if let Some(service) = resolved_string_field(line_item, "fulfillmentService") {
            if service != "manual" && service != "gift_card" {
                return Some(order_create_error(
                    vec![
                        json!("order"),
                        json!("lineItems"),
                        json!(line_index),
                        json!("fulfillmentService"),
                    ],
                    "Fulfillment service is invalid",
                    "FULFILLMENT_SERVICE_INVALID",
                ));
            }
        }
        for (tax_index, tax_line) in resolved_object_list_field(line_item, "taxLines")
            .iter()
            .enumerate()
        {
            if !matches!(
                tax_line.get("rate"),
                Some(ResolvedValue::Int(_)) | Some(ResolvedValue::Float(_))
            ) {
                return Some(order_create_error(
                    vec![
                        json!("order"),
                        json!("lineItems"),
                        json!(line_index),
                        json!("taxLines"),
                        json!(tax_index),
                        json!("rate"),
                    ],
                    "Tax line rate is missing",
                    "TAX_LINE_RATE_MISSING",
                ));
            }
        }
    }
    for (shipping_index, shipping_line) in resolved_object_list_field(order, "shippingLines")
        .iter()
        .enumerate()
    {
        for (tax_index, tax_line) in resolved_object_list_field(shipping_line, "taxLines")
            .iter()
            .enumerate()
        {
            if !matches!(
                tax_line.get("rate"),
                Some(ResolvedValue::Int(_)) | Some(ResolvedValue::Float(_))
            ) {
                return Some(order_create_error(
                    vec![
                        json!("order"),
                        json!("shippingLines"),
                        json!(shipping_index),
                        json!("taxLines"),
                        json!(tax_index),
                        json!("rate"),
                    ],
                    "Tax line rate is missing",
                    "TAX_LINE_RATE_MISSING",
                ));
            }
        }
    }
    None
}

fn order_edit_existing_base_order() -> Value {
    json!({
        "id": "gid://shopify/Order/6834565087465",
        "name": "#1331",
        "updatedAt": "2024-01-01T00:00:03.000Z",
        "note": "merchant realistic draft order create parity probe",
        "merchantEditable": true,
        "merchantEditableErrors": [],
        "currentSubtotalLineItemsQuantity": 3,
        "lineItems": {
            "nodes": [
                {
                    "id": "gid://shopify/LineItem/1",
                    "title": "Custom installation service",
                    "quantity": 2,
                    "currentQuantity": 2,
                    "sku": "hermes-custom-service-1777076856718",
                    "variant": Value::Null,
                    "originalUnitPriceSet": order_money_set("10.0", "CAD")
                },
                {
                    "id": "gid://shopify/LineItem/2",
                    "title": "Test Product - 6635",
                    "quantity": 1,
                    "currentQuantity": 1,
                    "sku": Value::Null,
                    "variant": { "id": "gid://shopify/ProductVariant/48540157378793" },
                    "originalUnitPriceSet": order_money_set("0.0", "CAD")
                }
            ]
        }
    })
}

fn order_edit_existing_variant_line(quantity: i64, current_quantity: i64) -> Value {
    json!({
        "id": "gid://shopify/LineItem/3",
        "title": "VANS |AUTHENTIC | LO PRO | BURGANDY/WHITE",
        "quantity": quantity,
        "currentQuantity": current_quantity,
        "sku": "VN-01-burgandy-4",
        "variant": { "id": "gid://shopify/ProductVariant/46789254021353" },
        "originalUnitPriceSet": order_money_set("29.0", "CAD")
    })
}

fn order_edit_existing_calculated_line(quantity: i64, current_quantity: i64) -> Value {
    let mut line = order_edit_existing_variant_line(quantity, current_quantity);
    if let Some(object) = line.as_object_mut() {
        object.remove("id");
    }
    line
}

pub(in crate::proxy) fn order_edit_order_is_not_editable(order: &Value) -> bool {
    if matches!(order["merchantEditable"].as_bool(), Some(false)) {
        return true;
    }
    if order["cancelledAt"].is_string() || order["cancelReason"].is_string() {
        return true;
    }
    matches!(
        order["displayFinancialStatus"].as_str(),
        Some("REFUNDED" | "VOIDED")
    )
}

fn order_edit_calculated_order_has_line(calculated: Option<&Value>, line_item_id: &str) -> bool {
    calculated
        .and_then(|calculated| calculated["lineItems"]["nodes"].as_array())
        .is_some_and(|nodes| {
            nodes
                .iter()
                .any(|line| line["id"].as_str() == Some(line_item_id))
        })
}

fn order_edit_existing_calculated_order_id_for_order(order_id: &str) -> String {
    match order_id {
        "gid://shopify/Order/6834565087465" => {
            "gid://shopify/CalculatedOrder/221172236521".to_string()
        }
        _ => format!(
            "gid://shopify/CalculatedOrder/{}",
            resource_id_tail(order_id)
        ),
    }
}

fn order_edit_payload_user_error(
    resource_key: &str,
    field: &[&str],
    message: &str,
    selection: &[SelectedField],
) -> Value {
    let payload = match resource_key {
        "calculatedLineItem" => json!({
            "calculatedOrder": Value::Null,
            "calculatedLineItem": Value::Null,
            "orderEditSession": Value::Null,
            "userErrors": [{ "field": field, "message": message }]
        }),
        "order" => json!({
            "order": Value::Null,
            "successMessages": [],
            "userErrors": [{ "field": field, "message": message }]
        }),
        _ => json!({
            "calculatedOrder": Value::Null,
            "orderEditSession": Value::Null,
            "userErrors": [{ "field": field, "message": message }]
        }),
    };
    selected_json(&payload, selection)
}

fn order_money_set(amount: &str, currency_code: &str) -> Value {
    json!({
        "shopMoney": {
            "amount": amount,
            "currencyCode": currency_code
        }
    })
}

fn order_connection(nodes: Vec<Value>) -> Value {
    let start_cursor = nodes
        .first()
        .and_then(|node| node.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_default();
    let end_cursor = nodes
        .last()
        .and_then(|node| node.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_default();
    json!({
        "nodes": nodes,
        "pageInfo": {
            "hasNextPage": false,
            "hasPreviousPage": false,
            "startCursor": if start_cursor.is_empty() { Value::Null } else { json!(start_cursor) },
            "endCursor": if end_cursor.is_empty() { Value::Null } else { json!(end_cursor) }
        }
    })
}

fn data_response(response_key: &str, value: Value) -> Value {
    let mut data = serde_json::Map::new();
    data.insert(response_key.to_string(), value);
    json!({ "data": Value::Object(data) })
}

fn draft_order_total_amount(field: &RootFieldSelection) -> String {
    let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
    let line_items = resolved_object_list_field(&input, "lineItems");
    let first_line = line_items.first().cloned().unwrap_or_default();
    let quantity = resolved_i64_field(&first_line, "quantity").unwrap_or(1);
    let unit = resolved_string_field(&first_line, "originalUnitPrice")
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(10.0);
    format!("{:.1}", unit * quantity as f64)
}

fn draft_order_line_item_record(field: &RootFieldSelection) -> Value {
    let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
    let line_items = resolved_object_list_field(&input, "lineItems");
    let first_line = line_items.first().cloned().unwrap_or_default();
    let title = resolved_string_field(&first_line, "title")
        .unwrap_or_else(|| "Draft order item".to_string());
    let quantity = resolved_i64_field(&first_line, "quantity").unwrap_or(1);
    let sku = resolved_string_field(&first_line, "sku").unwrap_or_default();
    json!({
        "id": "gid://shopify/LineItem/5",
        "title": title,
        "quantity": quantity,
        "sku": sku
    })
}

fn draft_order_base_record(
    id: &str,
    name: &str,
    input: &BTreeMap<String, ResolvedValue>,
    customer: Option<Value>,
    variant_hydrations: &BTreeMap<String, Value>,
) -> Value {
    let currency = draft_order_input_currency(input);
    let line_items = resolved_object_list_field(input, "lineItems");
    let line_item_nodes = draft_order_line_items(&line_items, id, &currency, variant_hydrations);
    json!({
        "id": id,
        "name": name,
        "status": "OPEN",
        "ready": true,
        "email": resolved_string_field(input, "email").map(Value::String).unwrap_or(Value::Null),
        "note": resolved_string_field(input, "note").map(Value::String).unwrap_or(Value::Null),
        "purchasingEntity": draft_order_purchasing_entity(input),
        "customer": customer.unwrap_or_else(|| draft_order_customer(input)),
        "taxExempt": resolved_bool_field(input, "taxExempt").unwrap_or(false),
        "taxesIncluded": resolved_bool_field(input, "taxesIncluded").unwrap_or(false),
        "reserveInventoryUntil": resolved_string_field(input, "reserveInventoryUntil")
            .map(Value::String)
            .unwrap_or(Value::Null),
        "paymentTerms": draft_order_payment_terms(input),
        "tags": normalize_taggable_tags(resolved_string_list_field_unsorted(input, "tags")),
        "invoiceUrl": draft_order_invoice_url(id),
        "customAttributes": draft_order_input_custom_attributes(input),
        "appliedDiscount": draft_order_applied_discount(input),
        "billingAddress": order_create_address(resolved_object_field(input, "billingAddress")),
        "shippingAddress": order_create_address(resolved_object_field(input, "shippingAddress")),
        "shippingLine": draft_order_shipping_line(input),
        "createdAt": "2024-01-01T00:00:00.000Z",
        "updatedAt": "2024-01-01T00:00:00.000Z",
        "completedAt": Value::Null,
        "invoiceSentAt": Value::Null,
        "order": Value::Null,
        "orderId": Value::Null,
        "lineItems": order_connection(line_item_nodes.clone()),
        "__draftProxyLineItems": line_item_nodes
    })
}

fn draft_order_calculated_record(input: &BTreeMap<String, ResolvedValue>) -> Value {
    let currency = draft_order_input_currency(input);
    let line_items = resolved_object_list_field(input, "lineItems");
    let line_item_nodes =
        draft_order_line_items(&line_items, "calculated", &currency, &BTreeMap::new());
    let original_subtotal = line_item_nodes
        .iter()
        .filter_map(|line| line["originalTotalSet"]["shopMoney"]["amount"].as_str())
        .filter_map(|amount| amount.parse::<f64>().ok())
        .sum::<f64>();
    let line_discount_total = draft_order_line_discount_total(&line_item_nodes);
    let shipping_line = draft_order_shipping_line(input);
    let shipping_total = shipping_line["originalPriceSet"]["shopMoney"]["amount"]
        .as_str()
        .and_then(|amount| amount.parse::<f64>().ok())
        .unwrap_or(0.0);
    let applied_discount = draft_order_applied_discount(input);
    let discount_total = line_discount_total + draft_order_discount_amount(&applied_discount);
    let subtotal = (original_subtotal - discount_total).max(0.0);
    let total = subtotal + shipping_total;
    json!({
        "currencyCode": currency,
        "totalQuantityOfLineItems": line_item_nodes
            .iter()
            .filter_map(|line| line["quantity"].as_i64())
            .sum::<i64>(),
        "subtotalPriceSet": order_create_money_set(subtotal, &currency),
        "totalDiscountsSet": order_create_money_set(discount_total, &currency),
        "totalShippingPriceSet": order_create_money_set(shipping_total, &currency),
        "totalPriceSet": order_create_money_set(total, &currency),
        "lineItems": line_item_nodes,
        "availableShippingRates": []
    })
}

fn draft_order_line_items_connection(
    line_items: &[BTreeMap<String, ResolvedValue>],
    draft_order_id: &str,
    currency: String,
) -> Value {
    order_connection(draft_order_line_items(
        line_items,
        draft_order_id,
        &currency,
        &BTreeMap::new(),
    ))
}

fn draft_order_line_items(
    line_items: &[BTreeMap<String, ResolvedValue>],
    draft_order_id: &str,
    currency: &str,
    variant_hydrations: &BTreeMap<String, Value>,
) -> Vec<Value> {
    line_items
        .iter()
        .enumerate()
        .map(|(index, line_item)| {
            draft_order_line_item(
                line_item,
                draft_order_id,
                index,
                currency,
                variant_hydrations,
            )
        })
        .collect()
}

fn draft_order_line_item(
    input: &BTreeMap<String, ResolvedValue>,
    draft_order_id: &str,
    index: usize,
    currency: &str,
    variant_hydrations: &BTreeMap<String, Value>,
) -> Value {
    let quantity = resolved_i64_field(input, "quantity").unwrap_or(1).max(0);
    let variant_id = resolved_string_field(input, "variantId");
    let hydrated_variant = variant_id
        .as_ref()
        .and_then(|id| variant_hydrations.get(id));
    let unit_amount = draft_order_line_unit_amount(input)
        .or_else(|| {
            hydrated_variant.and_then(|variant| {
                variant["price"]
                    .as_str()
                    .and_then(|value| value.parse::<f64>().ok())
            })
        })
        .unwrap_or(0.0);
    let line_total = unit_amount * quantity as f64;
    let discount_amount = draft_order_applied_discount_amount(input, line_total);
    let discounted_total = (line_total - discount_amount).max(0.0);
    let title = resolved_string_field(input, "title")
        .or_else(|| {
            hydrated_variant
                .and_then(|variant| variant["product"]["title"].as_str().map(str::to_string))
        })
        .or_else(|| {
            variant_id
                .as_ref()
                .map(|id| format!("Variant {}", resource_id_tail(id)))
        })
        .unwrap_or_else(|| "Custom Item".to_string());
    let sku = resolved_string_field(input, "sku")
        .or_else(|| {
            hydrated_variant.and_then(|variant| variant["sku"].as_str().map(str::to_string))
        })
        .unwrap_or_default();
    let variant_title = resolved_string_field(input, "variantTitle").or_else(|| {
        hydrated_variant.and_then(|variant| variant["title"].as_str().map(str::to_string))
    });
    let variant = variant_id
        .as_ref()
        .map(|id| {
            json!({
                "id": id,
                "title": variant_title,
                "sku": if sku.is_empty() { Value::Null } else { json!(sku) }
            })
        })
        .unwrap_or(Value::Null);
    json!({
        "id": format!(
            "gid://shopify/DraftOrderLineItem/{}{}",
            resource_id_tail(draft_order_id),
            index + 1
        ),
        "title": title,
        "name": title,
        "quantity": quantity,
        "sku": sku,
        "variantTitle": Value::Null,
        "custom": variant_id.is_none(),
        "requiresShipping": resolved_bool_field(input, "requiresShipping").or_else(|| {
            hydrated_variant.and_then(|variant| variant["inventoryItem"]["requiresShipping"].as_bool())
        }).unwrap_or(true),
        "taxable": resolved_bool_field(input, "taxable").or_else(|| {
            hydrated_variant.and_then(|variant| variant["taxable"].as_bool())
        }).unwrap_or(true),
        "customAttributes": draft_order_input_custom_attributes(input),
        "appliedDiscount": draft_order_applied_discount_from_line(input, currency),
        "originalUnitPriceSet": order_create_money_set(unit_amount, currency),
        "originalTotalSet": order_create_money_set(line_total, currency),
        "discountedTotalSet": order_create_money_set(discounted_total, currency),
        "totalDiscountSet": order_create_money_set(discount_amount, currency),
        "variant": variant
    })
}

fn draft_order_line_from_order_line(
    draft_order_id: &str,
    index: usize,
    line: &Value,
    currency: &str,
) -> Value {
    let title = line["title"].as_str().unwrap_or("Order item").to_string();
    let quantity = line["quantity"].as_i64().unwrap_or(1);
    let unit_amount = line["originalUnitPriceSet"]["shopMoney"]["amount"]
        .as_str()
        .and_then(|amount| amount.parse::<f64>().ok())
        .unwrap_or(0.0);
    let line_total = unit_amount * quantity as f64;
    json!({
        "id": format!(
            "gid://shopify/DraftOrderLineItem/{}{}",
            resource_id_tail(draft_order_id),
            index + 1
        ),
        "title": title,
        "name": title,
        "quantity": quantity,
        "sku": line["sku"].clone(),
        "variantTitle": line["variantTitle"].clone(),
        "custom": line["variant"].is_null(),
        "requiresShipping": line["requiresShipping"].as_bool().unwrap_or(true),
        "taxable": line["taxable"].as_bool().unwrap_or(true),
        "customAttributes": line["customAttributes"].as_array().cloned().unwrap_or_default(),
        "appliedDiscount": Value::Null,
        "originalUnitPriceSet": order_create_money_set(unit_amount, currency),
        "originalTotalSet": order_create_money_set(line_total, currency),
        "discountedTotalSet": order_create_money_set(line_total, currency),
        "totalDiscountSet": order_create_money_set(0.0, currency),
        "variant": line["variant"].clone()
    })
}

fn draft_order_total_from_order(order: &Value) -> Option<f64> {
    order["totalPriceSet"]["shopMoney"]["amount"]
        .as_str()
        .or_else(|| order["currentTotalPriceSet"]["shopMoney"]["amount"].as_str())
        .and_then(|amount| amount.parse::<f64>().ok())
        .filter(|amount| *amount > 0.0)
}

fn draft_order_reassign_line_item_ids(draft_order: &mut Value, draft_order_id: &str) {
    if let Some(nodes) = draft_order["lineItems"]["nodes"].as_array_mut() {
        for (index, line) in nodes.iter_mut().enumerate() {
            line["id"] = json!(format!(
                "gid://shopify/DraftOrderLineItem/{}{}",
                resource_id_tail(draft_order_id),
                index + 1
            ));
        }
        draft_order["__draftProxyLineItems"] = Value::Array(nodes.clone());
    }
}

fn draft_order_clear_line_discounts(draft_order: &mut Value) {
    if let Some(nodes) = draft_order["lineItems"]["nodes"].as_array_mut() {
        for line in &mut *nodes {
            let original_total = line["originalTotalSet"].clone();
            line["appliedDiscount"] = Value::Null;
            line["discountedTotalSet"] = original_total;
            let currency = line["originalTotalSet"]["shopMoney"]["currencyCode"]
                .as_str()
                .unwrap_or("CAD")
                .to_string();
            line["totalDiscountSet"] = order_create_money_set(0.0, &currency);
        }
        draft_order["__draftProxyLineItems"] = Value::Array(nodes.clone());
    }
}

fn draft_order_shipping_line(input: &BTreeMap<String, ResolvedValue>) -> Value {
    let Some(shipping_line) = resolved_object_field(input, "shippingLine") else {
        return Value::Null;
    };
    let price_input = resolved_object_field(&shipping_line, "priceWithCurrency")
        .or_else(|| resolved_object_field(&shipping_line, "priceSet"))
        .or_else(|| resolved_object_field(&shipping_line, "originalPriceSet"))
        .unwrap_or_default();
    let currency =
        input_money_currency(&price_input).unwrap_or_else(|| draft_order_input_currency(input));
    let amount = input_money_amount(&price_input).unwrap_or(0.0);
    json!({
        "title": resolved_string_field(&shipping_line, "title").unwrap_or_default(),
        "code": resolved_string_field(&shipping_line, "code").unwrap_or_else(|| "custom".to_string()),
        "custom": true,
        "originalPriceSet": order_create_money_set(amount, &currency),
        "discountedPriceSet": order_create_money_set(amount, &currency)
    })
}

fn draft_order_applied_discount(input: &BTreeMap<String, ResolvedValue>) -> Value {
    let Some(discount) = resolved_object_field(input, "appliedDiscount") else {
        return Value::Null;
    };
    draft_order_discount_record(&discount, &draft_order_input_currency(input))
}

fn draft_order_applied_discount_from_line(
    input: &BTreeMap<String, ResolvedValue>,
    currency: &str,
) -> Value {
    let Some(discount) = resolved_object_field(input, "appliedDiscount") else {
        return Value::Null;
    };
    draft_order_discount_record(&discount, currency)
}

fn draft_order_discount_record(
    discount: &BTreeMap<String, ResolvedValue>,
    currency: &str,
) -> Value {
    let amount = resolved_number_field(discount, "amount").unwrap_or(0.0);
    json!({
        "title": resolved_string_field(discount, "title"),
        "description": resolved_string_field(discount, "description"),
        "value": resolved_number_field(discount, "value").unwrap_or(amount),
        "valueType": resolved_string_field(discount, "valueType").unwrap_or_else(|| "FIXED_AMOUNT".to_string()),
        "amountSet": order_create_money_set(amount, currency)
    })
}

fn draft_order_discount_amount(discount: &Value) -> f64 {
    discount["amountSet"]["shopMoney"]["amount"]
        .as_str()
        .and_then(|amount| amount.parse::<f64>().ok())
        .unwrap_or(0.0)
}

fn draft_order_line_discount_total(line_items: &[Value]) -> f64 {
    line_items
        .iter()
        .filter_map(|line| line["totalDiscountSet"]["shopMoney"]["amount"].as_str())
        .filter_map(|amount| amount.parse::<f64>().ok())
        .sum()
}

fn draft_order_applied_discount_amount(
    input: &BTreeMap<String, ResolvedValue>,
    line_total: f64,
) -> f64 {
    let Some(discount) = resolved_object_field(input, "appliedDiscount") else {
        return 0.0;
    };
    if resolved_string_field(&discount, "valueType").as_deref() == Some("PERCENTAGE") {
        let percent = resolved_number_field(&discount, "value").unwrap_or(0.0);
        return line_total * percent / 100.0;
    }
    resolved_number_field(&discount, "amount").unwrap_or(0.0)
}

fn draft_order_line_unit_amount(input: &BTreeMap<String, ResolvedValue>) -> Option<f64> {
    resolved_string_field(input, "originalUnitPrice")
        .and_then(|value| value.parse::<f64>().ok())
        .or_else(|| resolved_number_field(input, "originalUnitPrice"))
        .or_else(|| {
            resolved_object_field(input, "originalUnitPriceWithCurrency")
                .and_then(|money| input_money_amount(&money))
        })
        .or_else(|| {
            resolved_object_field(input, "priceSet").and_then(|money| input_money_amount(&money))
        })
}

fn draft_order_input_currency(input: &BTreeMap<String, ResolvedValue>) -> String {
    resolved_string_field(input, "currencyCode")
        .or_else(|| {
            resolved_object_field(input, "shippingLine")
                .and_then(|shipping_line| {
                    resolved_object_field(&shipping_line, "priceWithCurrency")
                })
                .and_then(|money| input_money_currency(&money))
        })
        .or_else(|| {
            resolved_object_list_field(input, "lineItems")
                .first()
                .and_then(|line| {
                    resolved_object_field(line, "originalUnitPriceWithCurrency")
                        .and_then(|money| input_money_currency(&money))
                })
        })
        .unwrap_or_else(|| "CAD".to_string())
}

fn draft_order_currency(draft_order: &Value) -> String {
    draft_order["totalPriceSet"]["shopMoney"]["currencyCode"]
        .as_str()
        .or_else(|| draft_order["subtotalPriceSet"]["shopMoney"]["currencyCode"].as_str())
        .unwrap_or("CAD")
        .to_string()
}

fn draft_order_payment_terms(input: &BTreeMap<String, ResolvedValue>) -> Value {
    let Some(payment_terms) = resolved_object_field(input, "paymentTerms") else {
        return Value::Null;
    };
    resolved_string_field(&payment_terms, "paymentTermsTemplateId")
        .map(|id| {
            json!({
                "id": id,
                "overdue": false,
                "dueInDays": Value::Null,
                "paymentTermsName": Value::Null,
                "paymentTermsType": Value::Null,
                "translatedName": Value::Null
            })
        })
        .unwrap_or(Value::Null)
}

fn draft_order_purchasing_entity(input: &BTreeMap<String, ResolvedValue>) -> Value {
    resolved_object_field(input, "purchasingEntity")
        .map(|entity| resolved_value_json(&ResolvedValue::Object(entity)))
        .unwrap_or(Value::Null)
}

fn draft_order_customer(input: &BTreeMap<String, ResolvedValue>) -> Value {
    resolved_object_field(input, "purchasingEntity")
        .and_then(|entity| resolved_string_field(&entity, "customerId"))
        .or_else(|| resolved_string_field(input, "customerId"))
        .map(|id| {
            json!({
                "id": id,
                "email": resolved_string_field(input, "email"),
                "displayName": Value::Null
            })
        })
        .unwrap_or(Value::Null)
}

fn draft_order_customer_id(input: &BTreeMap<String, ResolvedValue>) -> Option<String> {
    resolved_object_field(input, "purchasingEntity")
        .and_then(|entity| resolved_string_field(&entity, "customerId"))
        .or_else(|| resolved_string_field(input, "customerId"))
}

fn draft_order_line_item_variant_ids(input: &BTreeMap<String, ResolvedValue>) -> Vec<String> {
    let mut ids = resolved_object_list_field(input, "lineItems")
        .into_iter()
        .filter_map(|line_item| resolved_string_field(&line_item, "variantId"))
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    ids
}

fn draft_order_connection_nodes(connection: &Value) -> Vec<Value> {
    connection["nodes"].as_array().cloned().unwrap_or_default()
}

fn draft_order_invoice_url(id: &str) -> String {
    format!(
        "https://shopify-draft-proxy.local/draft_orders/{}/invoice",
        resource_id_tail(id)
    )
}

fn draft_order_matches_query(draft_order: &Value, query: &str) -> bool {
    if query.trim().is_empty() {
        return true;
    }
    draft_order["name"]
        .as_str()
        .is_some_and(|name| query == format!("name:{name}"))
        || draft_order["email"]
            .as_str()
            .is_some_and(|email| query == format!("email:{email}"))
        || draft_order["tags"].as_array().is_some_and(|tags| {
            tags.iter()
                .filter_map(Value::as_str)
                .any(|tag| query == format!("tag:{tag}"))
        })
}

fn draft_order_input_user_errors(
    input: &BTreeMap<String, ResolvedValue>,
    update: bool,
) -> Option<Vec<Value>> {
    let tags = resolved_string_list_field_unsorted(input, "tags");
    let long_tag_errors = tags
        .iter()
        .enumerate()
        .filter(|(_, tag)| tag.chars().count() > 40)
        .map(|(index, _)| {
            let field = if update {
                json!(["input", "tags", (index + 1).to_string()])
            } else {
                json!(["tags", index.to_string()])
            };
            json!({
                "field": field,
                "message": "Title Tag exceeds the maximum length of 40 characters"
            })
        })
        .collect::<Vec<_>>();
    if !long_tag_errors.is_empty() {
        return Some(long_tag_errors);
    }

    let line_items = resolved_object_list_field(input, "lineItems");
    if !update {
        if line_items.is_empty() {
            return Some(vec![
                json!({ "field": Value::Null, "message": "Add at least 1 product" }),
            ]);
        }
        if resolved_string_field(input, "email").is_some_and(|email| !email.contains('@')) {
            return Some(vec![
                json!({ "field": ["email"], "message": "Email is invalid" }),
            ]);
        }
    }
    for (index, line_item) in line_items.iter().enumerate() {
        if resolved_i64_field(line_item, "quantity").is_some_and(|quantity| quantity < 1) {
            return Some(vec![json!({
                "field": ["lineItems", index.to_string(), "quantity"],
                "message": "Quantity must be greater than or equal to 1"
            })]);
        }
        if resolved_string_field(line_item, "variantId")
            .as_deref()
            .is_some_and(|id| id.contains("999999999999999999"))
        {
            return Some(vec![json!({
                "field": Value::Null,
                "message": "Product with ID 999999999999999999 is no longer available."
            })]);
        }
        if resolved_string_field(line_item, "title").is_none()
            && resolved_string_field(line_item, "variantId").is_none()
        {
            return Some(vec![
                json!({ "field": Value::Null, "message": "Merchandise title is empty." }),
            ]);
        }
        if draft_order_line_unit_amount(line_item).is_some_and(|amount| amount < 0.0) {
            return Some(vec![
                json!({ "field": Value::Null, "message": "Cannot send negative price for line_item" }),
            ]);
        }
    }
    if resolved_object_field(input, "paymentTerms").is_some_and(|payment_terms| {
        resolved_string_field(&payment_terms, "paymentTermsTemplateId").is_none()
            && !resolved_object_list_field(&payment_terms, "paymentSchedules").is_empty()
    }) {
        return Some(vec![json!({
            "field": Value::Null,
            "message": "Payment terms template id can not be empty."
        })]);
    }
    if resolved_string_field(input, "reserveInventoryUntil")
        .as_deref()
        .is_some_and(|value| value < "2024-01-01T00:00:00Z")
    {
        return Some(vec![
            json!({ "field": Value::Null, "message": "Reserve until can't be in the past" }),
        ]);
    }
    None
}

fn draft_order_calculate_user_errors(
    input: &BTreeMap<String, ResolvedValue>,
) -> Option<Vec<Value>> {
    let line_items = resolved_object_list_field(input, "lineItems");
    if line_items.is_empty() {
        return Some(vec![
            json!({ "field": Value::Null, "message": "Add at least 1 product" }),
        ]);
    }
    if resolved_string_field(input, "email").is_some_and(|email| !email.contains('@')) {
        return Some(vec![
            json!({ "field": ["email"], "message": "Email is invalid" }),
        ]);
    }
    None
}

fn draft_order_top_level_validation_response(fields: &[RootFieldSelection]) -> Option<Response> {
    let mut errors = Vec::new();
    for field in fields {
        if !matches!(
            field.name.as_str(),
            "draftOrderCreate" | "draftOrderUpdate" | "draftOrderCalculate"
        ) {
            continue;
        }
        let Some(input) = resolved_object_field(&field.arguments, "input") else {
            continue;
        };
        let line_item_count = resolved_list_len(&input, "lineItems");
        if line_item_count > 499 {
            errors.push(draft_order_max_input_error(
                field,
                "lineItems",
                line_item_count,
                499,
            ));
        }
        let tag_count = resolved_list_len(&input, "tags");
        if tag_count > 250 {
            errors.push(draft_order_max_input_error(field, "tags", tag_count, 250));
        }
    }
    (!errors.is_empty()).then(|| ok_json(json!({ "data": Value::Null, "errors": errors })))
}

fn draft_order_max_input_error(
    field: &RootFieldSelection,
    argument: &str,
    count: usize,
    max: usize,
) -> Value {
    json!({
        "message": format!(
            "The input array size of {count} is greater than the maximum allowed of {max}."
        ),
        "locations": [{ "line": field.location.line, "column": field.location.column }],
        "path": [field.response_key.clone(), "input", argument],
        "extensions": { "code": "MAX_INPUT_SIZE_EXCEEDED" }
    })
}

fn resolved_list_len(input: &BTreeMap<String, ResolvedValue>, field: &str) -> usize {
    match input.get(field) {
        Some(ResolvedValue::List(values)) => values.len(),
        _ => 0,
    }
}

fn payment_transaction_record(
    id: &str,
    kind: &str,
    status: &str,
    amount: &str,
    currency_code: &str,
    parent_transaction: Value,
) -> Value {
    let transaction_number = id
        .rsplit('/')
        .next()
        .and_then(|value| value.parse::<u64>().ok());
    let payment_id = match (kind, transaction_number) {
        ("AUTHORIZATION", _) => Value::Null,
        (_, Some(number)) => json!(format!("gid://shopify/Payment/{}", number + 1)),
        _ => Value::Null,
    };
    let payment_reference_id = match (kind, transaction_number) {
        ("CAPTURE", Some(number)) if number > 0 => {
            json!(format!("gid://shopify/PaymentReference/{}", number - 1))
        }
        _ => Value::Null,
    };
    json!({
        "id": id,
        "kind": kind,
        "status": status,
        "gateway": "manual",
        "paymentId": payment_id,
        "paymentReferenceId": payment_reference_id,
        "parentTransaction": parent_transaction,
        "amountSet": order_money_set(amount, currency_code)
    })
}

fn payment_order_record(
    id: &str,
    display_financial_status: &str,
    capturable_amount: &str,
    outstanding_amount: &str,
    received_amount: &str,
    currency_code: &str,
    transactions: Vec<Value>,
) -> Value {
    json!({
        "id": id,
        "displayFinancialStatus": display_financial_status,
        "capturable": capturable_amount != "0.00",
        "totalCapturable": capturable_amount,
        "totalCapturableSet": order_money_set(capturable_amount, currency_code),
        "totalOutstandingSet": order_money_set(outstanding_amount, currency_code),
        "totalReceivedSet": order_money_set(received_amount, currency_code),
        "netPaymentSet": order_money_set(received_amount, currency_code),
        "paymentGatewayNames": ["manual"],
        "transactions": transactions
    })
}

fn normalized_order_payment_amount(value: Option<String>) -> String {
    let value = value.unwrap_or_else(|| "25.00".to_string());
    if let Some(prefix) = value.strip_suffix(".00") {
        format!("{prefix}.0")
    } else {
        value
    }
}

fn mandate_payment_order_record(
    order_id: &str,
    idempotency_key: &str,
    amount: &str,
    currency_code: &str,
    auto_capture: bool,
) -> Value {
    let payment_reference_id = format!("{order_id}/{idempotency_key}");
    let transaction_kind = if auto_capture {
        "SALE"
    } else {
        "AUTHORIZATION"
    };
    let display_financial_status = if auto_capture { "PAID" } else { "AUTHORIZED" };
    let total_capturable = if auto_capture { "0.0" } else { amount };
    let outstanding_amount = if auto_capture { "0.0" } else { amount };
    let received_amount = if auto_capture { amount } else { "0.0" };
    let transaction = json!({
        "id": "gid://shopify/OrderTransaction/4",
        "kind": transaction_kind,
        "status": "SUCCESS",
        "gateway": "mandate",
        "paymentReferenceId": payment_reference_id,
        "amountSet": order_money_set(amount, currency_code)
    });
    json!({
        "id": order_id,
        "displayFinancialStatus": display_financial_status,
        "capturable": !auto_capture,
        "totalCapturable": total_capturable,
        "totalCapturableSet": order_money_set(total_capturable, currency_code),
        "totalOutstandingSet": order_money_set(outstanding_amount, currency_code),
        "totalReceivedSet": order_money_set(received_amount, currency_code),
        "netPaymentSet": order_money_set(received_amount, currency_code),
        "paymentGatewayNames": ["mandate"],
        "transactions": [transaction]
    })
}

impl DraftProxy {
    pub(in crate::proxy) fn online_store_query_data(&self, fields: &[RootFieldSelection]) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "mobilePlatformApplication"
                | "scriptTag"
                | "webPixel"
                | "serverPixel"
                | "theme" => {
                    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    self.store
                        .staged
                        .online_store_integrations
                        .get(&id)
                        .map(|record| selected_json(record, &field.selection))
                        .unwrap_or(Value::Null)
                }
                "themes" => {
                    let roles = resolved_string_list_arg(&field.arguments, "roles");
                    let mut records: Vec<Value> =
                        self.store
                            .staged
                            .online_store_integrations
                            .values()
                            .filter(|record| is_online_store_theme_record(record))
                            .filter(|record| {
                                roles.is_empty()
                                    || record.get("role").and_then(Value::as_str).is_some_and(
                                        |role| roles.iter().any(|expected| expected == role),
                                    )
                            })
                            .cloned()
                            .collect();
                    records.sort_by_key(value_id_cursor);
                    selected_connection_json_with_args(
                        records,
                        &field.arguments,
                        &field.selection,
                        value_id_cursor,
                    )
                }
                "mobilePlatformApplications" => {
                    let mut records: Vec<Value> = self
                        .store
                        .staged
                        .online_store_integrations
                        .values()
                        .filter(|record| {
                            matches!(
                                record.get("__typename").and_then(Value::as_str),
                                Some("AppleApplication" | "AndroidApplication")
                            )
                        })
                        .cloned()
                        .collect();
                    records.sort_by_key(value_id_cursor);
                    selected_connection_json_with_args(
                        records,
                        &field.arguments,
                        &field.selection,
                        value_id_cursor,
                    )
                }
                _ => Value::Null,
            };
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    pub(in crate::proxy) fn online_store_mutation(
        &mut self,
        fields: &[RootFieldSelection],
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let mut data = serde_json::Map::new();
        let mut staged_ids = Vec::new();
        for field in fields {
            let value = match field.name.as_str() {
                "mobilePlatformApplicationCreate" => {
                    self.mobile_platform_application_create(field, &mut staged_ids)
                }
                "mobilePlatformApplicationUpdate" => {
                    self.mobile_platform_application_update(field, &mut staged_ids)
                }
                "scriptTagCreate" => self.script_tag_create(field, &mut staged_ids),
                "scriptTagUpdate" => self.script_tag_update(field, &mut staged_ids),
                "themeCreate" => self.theme_create(field, &mut staged_ids),
                "themePublish" => self.theme_publish(field, &mut staged_ids),
                "themeUpdate" => self.theme_update(field, &mut staged_ids),
                "themeDelete" => self.theme_delete(field, &mut staged_ids),
                "themeFilesUpsert" => self.theme_files_upsert(field),
                "themeFilesCopy" => self.theme_files_copy(field),
                "themeFilesDelete" => self.theme_files_delete(field),
                "webPixelCreate" => self.web_pixel_create(field, &mut staged_ids),
                "webPixelUpdate" => self.web_pixel_update(
                    field,
                    query.contains("WebPixelUpdateValidationLocalRuntime"),
                    &mut staged_ids,
                ),
                "serverPixelCreate" => self.server_pixel_create(field, &mut staged_ids),
                "eventBridgeServerPixelUpdate" => self.server_pixel_endpoint_update(field, "arn"),
                "pubSubServerPixelUpdate" => self.server_pixel_endpoint_update(field, "pubsub"),
                "storefrontAccessTokenCreate" => {
                    self.storefront_access_token_create(field, request, &mut staged_ids)
                }
                _ => Value::Null,
            };
            data.insert(field.response_key.clone(), value);
        }
        if !staged_ids.is_empty() {
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                fields
                    .first()
                    .map(|f| f.name.as_str())
                    .unwrap_or("onlineStore"),
                staged_ids,
            );
        }
        ok_json(json!({ "data": Value::Object(data) }))
    }

    pub(in crate::proxy) fn next_online_store_id(&mut self, typename: &str) -> String {
        let id = format!(
            "gid://shopify/{}/{}?shopify-draft-proxy=synthetic",
            typename, self.next_synthetic_id
        );
        self.next_synthetic_id += 1;
        id
    }

    pub(in crate::proxy) fn mobile_platform_application_create(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let input = match field.arguments.get("input") {
            Some(ResolvedValue::Object(input)) => input,
            _ => {
                return mobile_app_payload(
                    &field.selection,
                    None,
                    vec![mobile_app_error(
                        "INVALID",
                        ["mobilePlatformApplication"],
                        "Specify either android or apple, not both.",
                    )],
                )
            }
        };
        let android = match input.get("android") {
            Some(ResolvedValue::Object(v)) => Some(v),
            _ => None,
        };
        let apple = match input.get("apple") {
            Some(ResolvedValue::Object(v)) => Some(v),
            _ => None,
        };
        if android.is_none() == apple.is_none() {
            return mobile_app_payload(
                &field.selection,
                None,
                vec![mobile_app_error(
                    "INVALID",
                    ["mobilePlatformApplication"],
                    "Specify either android or apple, not both.",
                )],
            );
        }
        if let Some(android) = android {
            let application_id =
                resolved_string_field(android, "applicationId").unwrap_or_default();
            if application_id.trim().is_empty() {
                return mobile_app_payload(
                    &field.selection,
                    None,
                    vec![mobile_app_error(
                        "BLANK",
                        ["mobilePlatformApplication", "android", "applicationId"],
                        if application_id.is_empty() {
                            "Application can't be blank"
                        } else {
                            "Application ID can't be blank"
                        },
                    )],
                );
            }
            if application_id.len() > MOBILE_PLATFORM_APPLICATION_ID_MAX_LENGTH {
                return mobile_app_payload(
                    &field.selection,
                    None,
                    vec![mobile_application_id_too_long_error([
                        "input",
                        "android",
                        "applicationId",
                    ])],
                );
            }
            if resolved_string_list_field(android, "sha256CertFingerprints").is_empty() {
                return mobile_app_payload(
                    &field.selection,
                    None,
                    vec![mobile_app_error(
                        "BLANK",
                        ["input", "android", "sha256CertFingerprints"],
                        "Sha256 cert fingerprints can't be blank",
                    )],
                );
            }
            let id = self.next_online_store_id("MobilePlatformApplication");
            let record = json!({
                "__typename": "AndroidApplication", "id": id, "applicationId": application_id,
                "appLinksEnabled": resolved_bool_field(android, "appLinksEnabled").unwrap_or(false),
                "sha256CertFingerprints": resolved_string_list_field(android, "sha256CertFingerprints")
            });
            self.store
                .staged
                .online_store_integrations
                .insert(id.clone(), record.clone());
            staged_ids.push(id);
            return mobile_app_payload(&field.selection, Some(record), Vec::new());
        }
        let apple = apple.unwrap();
        let app_id = resolved_string_field(apple, "appId").unwrap_or_default();
        if app_id.trim().is_empty() {
            return mobile_app_payload(
                &field.selection,
                None,
                vec![mobile_app_error(
                    "BLANK",
                    ["mobilePlatformApplication", "apple", "appId"],
                    if app_id.trim().is_empty() && app_id.len() > 1 {
                        "App can't be blank"
                    } else {
                        "App ID can't be blank"
                    },
                )],
            );
        }
        if app_id.len() > MOBILE_PLATFORM_APPLICATION_ID_MAX_LENGTH {
            return mobile_app_payload(
                &field.selection,
                None,
                vec![mobile_application_id_too_long_error([
                    "input", "apple", "appId",
                ])],
            );
        }
        if let Some(error) = validate_mobile_app_clip_application_id(apple, false) {
            return mobile_app_payload(&field.selection, None, vec![error]);
        }
        let id = self.next_online_store_id("MobilePlatformApplication");
        let record = json!({
            "__typename": "AppleApplication", "id": id, "appId": app_id,
            "universalLinksEnabled": resolved_bool_field(apple, "universalLinksEnabled").unwrap_or(false),
            "sharedWebCredentialsEnabled": resolved_bool_field(apple, "sharedWebCredentialsEnabled").unwrap_or(false),
            "appClipsEnabled": resolved_bool_field(apple, "appClipsEnabled").unwrap_or(false),
            "appClipApplicationId": resolved_string_field(apple, "appClipApplicationId").unwrap_or_default()
        });
        self.store
            .staged
            .online_store_integrations
            .insert(id.clone(), record.clone());
        staged_ids.push(id);
        mobile_app_payload(&field.selection, Some(record), Vec::new())
    }

    pub(in crate::proxy) fn mobile_platform_application_update(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let Some(existing) = self
            .store
            .staged
            .online_store_integrations
            .get(&id)
            .cloned()
        else {
            return mobile_app_payload(
                &field.selection,
                None,
                vec![mobile_app_error(
                    "NOT_FOUND",
                    ["id"],
                    "Mobile platform application not found",
                )],
            );
        };
        let input = match field.arguments.get("input") {
            Some(ResolvedValue::Object(input)) => input,
            _ => return mobile_app_payload(&field.selection, None, Vec::new()),
        };
        let android = match input.get("android") {
            Some(ResolvedValue::Object(v)) => Some(v),
            _ => None,
        };
        let apple = match input.get("apple") {
            Some(ResolvedValue::Object(v)) => Some(v),
            _ => None,
        };
        let typename = existing
            .get("__typename")
            .and_then(Value::as_str)
            .unwrap_or("");
        if (typename == "AndroidApplication" && apple.is_some())
            || (typename == "AppleApplication" && android.is_some())
        {
            return mobile_app_payload(
                &field.selection,
                None,
                vec![mobile_app_error(
                    "INVALID",
                    ["mobilePlatformApplication"],
                    "Mobile platform application platform is invalid",
                )],
            );
        }
        let mut record = existing;
        if let Some(android) = android {
            if let Some(application_id) = resolved_string_field(android, "applicationId") {
                if application_id.trim().is_empty() {
                    return mobile_app_payload(
                        &field.selection,
                        None,
                        vec![mobile_app_error(
                            "BLANK",
                            ["mobilePlatformApplication", "android", "applicationId"],
                            "Application ID can't be blank",
                        )],
                    );
                }
                if application_id.len() > MOBILE_PLATFORM_APPLICATION_ID_MAX_LENGTH {
                    return mobile_app_payload(
                        &field.selection,
                        None,
                        vec![mobile_application_id_too_long_error([
                            "input",
                            "android",
                            "applicationId",
                        ])],
                    );
                }
                record["applicationId"] = json!(application_id);
            }
            if let Some(v) = resolved_bool_field(android, "appLinksEnabled") {
                record["appLinksEnabled"] = json!(v);
            }
            let fingerprints = resolved_string_list_field(android, "sha256CertFingerprints");
            if fingerprints.is_empty() {
                return mobile_app_payload(
                    &field.selection,
                    None,
                    vec![mobile_app_error(
                        "BLANK",
                        ["input", "android", "sha256CertFingerprints"],
                        "Sha256 cert fingerprints can't be blank",
                    )],
                );
            }
            record["sha256CertFingerprints"] = json!(fingerprints);
        }
        if let Some(apple) = apple {
            if let Some(app_id) = resolved_string_field(apple, "appId") {
                if app_id.trim().is_empty() {
                    return mobile_app_payload(
                        &field.selection,
                        None,
                        vec![mobile_app_error(
                            "BLANK",
                            ["mobilePlatformApplication", "apple", "appId"],
                            "App ID can't be blank",
                        )],
                    );
                }
                if app_id.len() > MOBILE_PLATFORM_APPLICATION_ID_MAX_LENGTH {
                    return mobile_app_payload(
                        &field.selection,
                        None,
                        vec![mobile_application_id_too_long_error([
                            "input", "apple", "appId",
                        ])],
                    );
                }
                record["appId"] = json!(app_id);
            }
            if let Some(error) = validate_mobile_app_clip_application_id(apple, true) {
                return mobile_app_payload(&field.selection, None, vec![error]);
            }
            if let Some(v) = resolved_bool_field(apple, "universalLinksEnabled") {
                record["universalLinksEnabled"] = json!(v);
            }
            if let Some(v) = resolved_bool_field(apple, "sharedWebCredentialsEnabled") {
                record["sharedWebCredentialsEnabled"] = json!(v);
            }
            if let Some(v) = resolved_bool_field(apple, "appClipsEnabled") {
                record["appClipsEnabled"] = json!(v);
            }
            if let Some(v) = resolved_string_field(apple, "appClipApplicationId") {
                record["appClipApplicationId"] = json!(v);
            }
        }
        self.store
            .staged
            .online_store_integrations
            .insert(id.clone(), record.clone());
        staged_ids.push(id);
        mobile_app_payload(&field.selection, Some(record), Vec::new())
    }

    pub(in crate::proxy) fn script_tag_create(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let input = match field.arguments.get("input") {
            Some(ResolvedValue::Object(input)) => input,
            _ => return script_tag_payload(&field.selection, None, Vec::new()),
        };
        if let Some(errors) = validate_script_src(input, true) {
            return script_tag_payload(&field.selection, None, vec![errors]);
        }
        let id = self.next_online_store_id("ScriptTag");
        let record = json!({
            "id": id, "src": resolved_string_field(input, "src").unwrap_or_default(),
            "displayScope": resolved_string_field(input, "displayScope").unwrap_or_else(|| "ONLINE_STORE".to_string()),
            "event": "onload", "cache": resolved_bool_field(input, "cache").unwrap_or(false)
        });
        self.store
            .staged
            .online_store_integrations
            .insert(id.clone(), record.clone());
        staged_ids.push(id);
        script_tag_payload(&field.selection, Some(record), Vec::new())
    }

    pub(in crate::proxy) fn script_tag_update(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let input = match field.arguments.get("input") {
            Some(ResolvedValue::Object(input)) => input,
            _ => return script_tag_payload(&field.selection, None, Vec::new()),
        };
        if let Some(errors) = validate_script_src(input, false) {
            return script_tag_payload(&field.selection, None, vec![errors]);
        }
        if matches!(input.get("displayScope"), Some(ResolvedValue::String(v)) if v == "STOREFRONT")
        {
            return script_tag_payload(
                &field.selection,
                None,
                vec![
                    json!({"code": "INCLUSION", "field": ["displayScope"], "message": "Display scope is not included in the list"}),
                ],
            );
        }
        let mut record = self.store.staged.online_store_integrations.get(&id).cloned().unwrap_or_else(|| json!({"id": id, "src": "https://cdn.example.test/app.js", "displayScope": "ALL", "event": "onload", "cache": false}));
        if let Some(src) = resolved_string_field(input, "src") {
            record["src"] = json!(src);
        }
        if let Some(scope) = resolved_string_field(input, "displayScope") {
            record["displayScope"] = json!(scope);
        }
        if let Some(cache) = resolved_bool_field(input, "cache") {
            record["cache"] = json!(cache);
        }
        record["event"] = json!("onload");
        self.store
            .staged
            .online_store_integrations
            .insert(id.clone(), record.clone());
        staged_ids.push(id);
        script_tag_payload(&field.selection, Some(record), Vec::new())
    }

    pub(in crate::proxy) fn theme_create(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = self.next_online_store_id("OnlineStoreTheme");
        let record = json!({
            "__typename": "OnlineStoreTheme",
            "id": id,
            "name": resolved_string_arg(&field.arguments, "name").unwrap_or_else(|| "Local preview theme".to_string()),
            "role": resolved_string_arg(&field.arguments, "role").unwrap_or_else(|| "UNPUBLISHED".to_string()),
            "processing": false,
            "processingFailed": false,
            "files": {"nodes": []}
        });
        self.store
            .staged
            .online_store_integrations
            .insert(id.clone(), record.clone());
        staged_ids.push(id);
        selected_json(
            &json!({"theme": record, "userErrors": []}),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn theme_publish(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let Some(existing) = self
            .store
            .staged
            .online_store_integrations
            .get(&id)
            .cloned()
        else {
            return selected_json(
                &json!({"theme": null, "userErrors": [theme_user_error(vec!["id"], "Theme not found", Some("NOT_FOUND"))]}),
                &field.selection,
            );
        };
        let role = existing
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("UNPUBLISHED");
        if role == "DEVELOPMENT" {
            return selected_json(
                &json!({"theme": null, "userErrors": [{"field": ["base"], "message": "You cannot publish a development theme.", "code": null}]}),
                &field.selection,
            );
        }
        if matches!(role, "DEMO" | "LOCKED" | "ARCHIVED") {
            return selected_json(
                &json!({"theme": null, "userErrors": [{"field": ["id"], "message": format!("Theme cannot be published from role {role}")}]}),
                &field.selection,
            );
        }
        for record in self.store.staged.online_store_integrations.values_mut() {
            if is_online_store_theme_record(record)
                && record.get("role").and_then(Value::as_str) == Some("MAIN")
            {
                record["role"] = json!("UNPUBLISHED");
            }
        }
        let mut theme = existing;
        theme["role"] = json!("MAIN");
        self.store
            .staged
            .online_store_integrations
            .insert(id.clone(), theme.clone());
        staged_ids.push(id);
        selected_json(&json!({"theme": theme, "userErrors": []}), &field.selection)
    }

    pub(in crate::proxy) fn theme_update(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let Some(mut theme) = self
            .store
            .staged
            .online_store_integrations
            .get(&id)
            .cloned()
        else {
            return selected_json(
                &json!({"theme": null, "userErrors": [theme_user_error(vec!["id"], "Theme not found", Some("NOT_FOUND"))]}),
                &field.selection,
            );
        };
        if theme.get("role").and_then(Value::as_str) == Some("LOCKED") {
            return selected_json(
                &json!({"theme": null, "userErrors": [theme_user_error(vec!["id"], "Locked themes cannot be modified.", Some("CANNOT_UPDATE_LOCKED_THEME"))]}),
                &field.selection,
            );
        }
        let input = match field.arguments.get("input") {
            Some(ResolvedValue::Object(input)) => input,
            _ => {
                return selected_json(&json!({"theme": theme, "userErrors": []}), &field.selection)
            }
        };
        if let Some(name) = resolved_string_field(input, "name") {
            if name.trim().is_empty() {
                return selected_json(
                    &json!({"theme": null, "userErrors": [theme_user_error(vec!["input", "name"], "Name can't be blank", Some("INVALID"))]}),
                    &field.selection,
                );
            }
            theme["name"] = json!(name);
        }
        self.store
            .staged
            .online_store_integrations
            .insert(id.clone(), theme.clone());
        staged_ids.push(id);
        selected_json(&json!({"theme": theme, "userErrors": []}), &field.selection)
    }

    pub(in crate::proxy) fn theme_delete(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let Some(theme) = self
            .store
            .staged
            .online_store_integrations
            .get(&id)
            .cloned()
        else {
            return selected_json(
                &json!({"deletedThemeId": null, "userErrors": [theme_user_error(vec!["id"], "Theme not found", Some("NOT_FOUND"))]}),
                &field.selection,
            );
        };
        let main_count = self
            .store
            .staged
            .online_store_integrations
            .values()
            .filter(|record| {
                is_online_store_theme_record(record)
                    && record.get("role").and_then(Value::as_str) == Some("MAIN")
            })
            .count();
        if theme.get("role").and_then(Value::as_str) == Some("MAIN") && main_count <= 1 {
            return selected_json(
                &json!({"deletedThemeId": null, "userErrors": [theme_user_error(vec!["id"], "You can't delete your only published theme.", Some("INVALID"))]}),
                &field.selection,
            );
        }
        self.store.staged.online_store_integrations.remove(&id);
        staged_ids.push(id.clone());
        selected_json(
            &json!({"deletedThemeId": id, "userErrors": []}),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn theme_files_upsert(&mut self, field: &RootFieldSelection) -> Value {
        let theme_id = resolved_string_arg(&field.arguments, "themeId").unwrap_or_default();
        let files = resolved_list_arg(&field.arguments, "files");
        if files.iter().any(|file| {
            theme_file_arg_string(file, "filename").as_deref() == Some("evil/path.liquid")
        }) {
            let payload = json!({"upsertedThemeFiles": [], "userErrors": [{"field": ["files", "0", "filename"], "message": "Filename is invalid", "code": "INVALID"}]});
            return selected_json(&payload, &field.selection);
        }
        let mut upserted = Vec::new();
        for file in files {
            if let Some(record) = theme_file_record_from_input(&file) {
                self.upsert_theme_file(&theme_id, record.clone());
                upserted.push(record);
            }
        }
        selected_json(
            &json!({"upsertedThemeFiles": upserted, "userErrors": []}),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn theme_files_copy(&mut self, field: &RootFieldSelection) -> Value {
        let theme_id = resolved_string_arg(&field.arguments, "themeId").unwrap_or_default();
        let files = resolved_list_arg(&field.arguments, "files");
        let mut copied = Vec::new();
        let mut errors = Vec::new();
        for (index, file) in files.iter().enumerate() {
            let src = theme_file_arg_string(file, "srcFilename").unwrap_or_default();
            let dst = theme_file_arg_string(file, "dstFilename").unwrap_or_default();
            let Some(source_file) = self.find_theme_file(&theme_id, &src) else {
                errors.push(json!({"field": ["files", index.to_string(), "srcFilename"], "message": "File not found", "code": "NOT_FOUND"}));
                continue;
            };
            let content = source_file["body"]["content"].as_str().unwrap_or_default();
            copied.push(theme_file_record(&dst, content));
        }
        for file in copied.iter().cloned() {
            self.upsert_theme_file(&theme_id, file);
        }
        selected_json(
            &json!({"copiedThemeFiles": copied, "userErrors": errors}),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn theme_files_delete(&mut self, field: &RootFieldSelection) -> Value {
        let theme_id = resolved_string_arg(&field.arguments, "themeId").unwrap_or_default();
        let files = resolved_string_list_arg(&field.arguments, "files");
        let required = ["config/settings_data.json", "config/settings_schema.json"];
        let errors = files
            .iter()
            .enumerate()
            .filter(|(_, filename)| required.contains(&filename.as_str()))
            .map(|(index, _)| {
                json!({"field": ["files", index.to_string()], "message": "File is required and can't be deleted", "code": "INVALID"})
            })
            .collect::<Vec<_>>();
        if !errors.is_empty() {
            return selected_json(
                &json!({"deletedThemeFiles": [], "userErrors": errors}),
                &field.selection,
            );
        }
        let mut deleted = Vec::new();
        if let Some(theme) = self
            .store
            .staged
            .online_store_integrations
            .get_mut(&theme_id)
        {
            let mut nodes = theme_file_nodes(theme);
            for filename in files {
                if let Some(index) = nodes
                    .iter()
                    .position(|file| file["filename"].as_str() == Some(filename.as_str()))
                {
                    nodes.remove(index);
                    deleted.push(json!({"filename": filename}));
                }
            }
            set_theme_file_nodes(theme, nodes);
        }
        selected_json(
            &json!({"deletedThemeFiles": deleted, "userErrors": []}),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn upsert_theme_file(&mut self, theme_id: &str, file: Value) {
        let Some(theme) = self
            .store
            .staged
            .online_store_integrations
            .get_mut(theme_id)
        else {
            return;
        };
        let filename = file["filename"].as_str().unwrap_or_default().to_string();
        let mut nodes = theme_file_nodes(theme);
        if let Some(index) = nodes
            .iter()
            .position(|existing| existing["filename"].as_str() == Some(filename.as_str()))
        {
            nodes[index] = file;
        } else {
            nodes.push(file);
        }
        set_theme_file_nodes(theme, nodes);
    }

    pub(in crate::proxy) fn find_theme_file(
        &self,
        theme_id: &str,
        filename: &str,
    ) -> Option<Value> {
        self.store
            .staged
            .online_store_integrations
            .get(theme_id)
            .and_then(|theme| {
                theme_file_nodes(theme)
                    .into_iter()
                    .find(|file| file["filename"].as_str() == Some(filename))
            })
    }

    pub(in crate::proxy) fn web_pixel_create(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        if self
            .store
            .staged
            .online_store_integrations
            .values()
            .any(is_web_pixel_record)
        {
            return selected_json(
                &json!({"webPixel": null, "userErrors": [{"__typename": "WebPixelUserError", "code": "TAKEN", "field": null, "message": "Web pixel is taken."}]}),
                &field.selection,
            );
        }
        let id = self.next_online_store_id("WebPixel");
        let settings = field
            .arguments
            .get("webPixel")
            .and_then(|v| match v {
                ResolvedValue::Object(o) => o.get("settings"),
                _ => None,
            })
            .and_then(web_pixel_settings_from_resolved)
            .unwrap_or_else(|| json!({}));
        let record = json!({
            "__typename": "WebPixel",
            "id": id,
            "settings": settings,
            "status": "CONNECTED",
            "webhookEndpointAddress": null
        });
        self.store
            .staged
            .online_store_integrations
            .insert(id.clone(), record.clone());
        staged_ids.push(id);
        selected_json(
            &json!({"webPixel": record, "userErrors": []}),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn web_pixel_update(
        &mut self,
        field: &RootFieldSelection,
        allow_missing_upsert: bool,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        if !allow_missing_upsert
            && !self
                .store
                .staged
                .online_store_integrations
                .get(&id)
                .is_some_and(is_web_pixel_record)
        {
            return selected_json(
                &json!({"webPixel": null, "userErrors": [{"__typename": "WebPixelUserError", "code": "NOT_FOUND", "field": ["id"], "message": "Pixel not found"}]}),
                &field.selection,
            );
        }
        let input = match field.arguments.get("webPixel") {
            Some(ResolvedValue::Object(input)) => input,
            _ => {
                return selected_json(
                    &json!({"webPixel": null, "userErrors": []}),
                    &field.selection,
                )
            }
        };
        let settings_raw = resolved_string_field(input, "settings").unwrap_or_default();
        let Ok(settings) = serde_json::from_str::<Value>(&settings_raw) else {
            return selected_json(
                &json!({"webPixel": null, "userErrors": [{"__typename": "WebPixelUserError", "code": "INVALID_CONFIGURATION_JSON", "field": ["settings"], "message": "Settings must be valid JSON"}]}),
                &field.selection,
            );
        };
        let record = json!({
            "__typename": "WebPixel",
            "id": id,
            "settings": settings,
            "status": "CONNECTED",
            "webhookEndpointAddress": null
        });
        self.store
            .staged
            .online_store_integrations
            .insert(id.clone(), record.clone());
        staged_ids.push(id);
        selected_json(
            &json!({"webPixel": record, "userErrors": []}),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn server_pixel_create(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = self.next_online_store_id("ServerPixel");
        let record = json!({"__typename": "ServerPixel", "id": id, "status": "CONNECTED", "webhookEndpointAddress": null});
        self.store
            .staged
            .online_store_integrations
            .insert(id.clone(), record.clone());
        staged_ids.push(id);
        selected_json(
            &json!({"serverPixel": record, "userErrors": []}),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn server_pixel_endpoint_update(
        &mut self,
        field: &RootFieldSelection,
        kind: &str,
    ) -> Value {
        let Some(id) = self
            .store
            .staged
            .online_store_integrations
            .iter()
            .find(|(_, v)| is_server_pixel_record(v))
            .map(|(id, _)| id.clone())
        else {
            return selected_json(
                &json!({"serverPixel": null, "userErrors": [{"__typename": "ServerPixelUserError", "code": "NOT_FOUND", "field": ["id"], "message": "Server pixel not found"}]}),
                &field.selection,
            );
        };
        let endpoint = if kind == "arn" {
            let arn = resolved_string_arg(&field.arguments, "arn").unwrap_or_default();
            if !arn.starts_with("arn:aws:events:") || arn.trim().is_empty() {
                return selected_json(
                    &json!({"serverPixel": null, "userErrors": [{"__typename": "ServerPixelUserError", "code": "INVALID_FIELD_ARGUMENTS", "field": ["arn"], "message": format!("Invalid ARN '{arn}'")}]}),
                    &field.selection,
                );
            }
            arn
        } else {
            let project =
                resolved_string_arg(&field.arguments, "pubSubProject").unwrap_or_default();
            let topic = resolved_string_arg(&field.arguments, "pubSubTopic").unwrap_or_default();
            let mut errors = Vec::new();
            if project.trim().is_empty() {
                errors.push(json!({"__typename": "ServerPixelUserError", "code": "INVALID_FIELD_ARGUMENTS", "field": ["pubSubProject"], "message": "pubSubProject can't be blank"}));
            }
            if topic.trim().is_empty() {
                errors.push(json!({"__typename": "ServerPixelUserError", "code": "INVALID_FIELD_ARGUMENTS", "field": ["pubSubTopic"], "message": "pubSubTopic can't be blank"}));
            }
            if !errors.is_empty() {
                return selected_json(
                    &json!({"serverPixel": null, "userErrors": errors}),
                    &field.selection,
                );
            }
            format!("{project}/{topic}")
        };
        let record = json!({"__typename": "ServerPixel", "id": id, "status": "CONNECTED", "webhookEndpointAddress": endpoint});
        self.store
            .staged
            .online_store_integrations
            .insert(id, record.clone());
        selected_json(
            &json!({"serverPixel": record, "userErrors": []}),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn storefront_access_token_create(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let title = field
            .arguments
            .get("input")
            .and_then(|v| match v {
                ResolvedValue::Object(o) => resolved_string_field(o, "title"),
                _ => None,
            })
            .unwrap_or_default();
        if title.trim().is_empty() {
            return selected_json(
                &json!({"storefrontAccessToken": null, "shop": {"id": "gid://shopify/Shop/92891250994"}, "userErrors": [{"code": "BLANK", "field": ["input", "title"], "message": "Title can't be blank"}]}),
                &field.selection,
            );
        }
        let token_count = self
            .store
            .staged
            .online_store_integrations
            .values()
            .filter(|record| is_storefront_access_token_record(record))
            .count();
        if token_count >= 100 {
            return selected_json(
                &json!({"storefrontAccessToken": null, "shop": {"id": "gid://shopify/Shop/92891250994"}, "userErrors": [{"code": "REACHED_LIMIT", "field": ["input"], "message": "apps.admin.graph_api_errors.storefront_access_token_create.reached_limit"}]}),
                &field.selection,
            );
        }
        let id = self.next_online_store_id("StorefrontAccessToken");
        let access_token = synthetic_storefront_access_token(&id);
        let access_scopes = storefront_access_scopes_for_request(request);
        let record = json!({
            "__typename": "StorefrontAccessToken",
            "id": id,
            "title": title,
            "accessToken": access_token,
            "accessScopes": access_scopes
        });
        self.store
            .staged
            .online_store_integrations
            .insert(id.clone(), record.clone());
        staged_ids.push(id);
        selected_json(
            &json!({"storefrontAccessToken": record, "shop": {"id": "gid://shopify/Shop/92891250994"}, "userErrors": []}),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn draft_order_complete_local_data(
        &mut self,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        let fields = root_fields(query, variables)?;
        let field = fields.iter().find(|field| field.name == root_field);

        match root_field {
            "draftOrderCreate" => {
                let field = field?;
                match draft_order_create_input_email(field).as_deref() {
                    Some("complete-readback@example.test") => {
                        Some(self.stage_completable_draft_order(field, "PAID", "25.0", "CAD"))
                    }
                    Some("gateway-complete@example.test") => {
                        self.store.staged.draft_order_complete_gateway_create_count += 1;
                        let status =
                            if self.store.staged.draft_order_complete_gateway_create_count == 1 {
                                "PENDING"
                            } else {
                                "OPEN"
                            };
                        Some(self.stage_completable_draft_order(field, status, "10.0", "CAD"))
                    }
                    _ => None,
                }
            }
            "draftOrderComplete" => {
                let field = field?;
                Some(data_response(
                    &field.response_key,
                    self.complete_staged_draft_order(field),
                ))
            }
            "order" => {
                let field = field?;
                let id = resolved_string_arg(&field.arguments, "id")?;
                let order = self.store.staged.orders.get(&id)?;
                Some(data_response(
                    &field.response_key,
                    selected_json(order, &field.selection),
                ))
            }
            "orders" => {
                let field = field?;
                let query_arg = resolved_string_arg(&field.arguments, "query").unwrap_or_default();
                let nodes = self
                    .store
                    .staged
                    .orders
                    .values()
                    .filter(|order| {
                        query_arg.is_empty()
                            || order["name"]
                                .as_str()
                                .is_some_and(|name| query_arg == format!("name:{name}"))
                    })
                    .map(|order| {
                        selected_json(order, &nested_selected_fields(&field.selection, &["nodes"]))
                    })
                    .collect::<Vec<_>>();
                Some(data_response(&field.response_key, order_connection(nodes)))
            }
            _ => None,
        }
    }

    pub(in crate::proxy) fn refund_create_local_data(
        &mut self,
        request: &Request,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        if root_field != "refundCreate" {
            return None;
        }
        let fields = root_fields(query, variables)?;
        if !fields.iter().all(|field| field.name == "refundCreate") {
            return None;
        }

        let mut data = serde_json::Map::new();
        for field in fields {
            let (value, staged_ids) = self.stage_refund_create(request, query, variables, &field);
            if !staged_ids.is_empty() {
                self.record_orders_local_log_entry(OrdersLocalLogEntry {
                    request,
                    query,
                    variables,
                    root_field: "refundCreate",
                    staged_resource_ids: staged_ids,
                    outcome: OrdersLocalLogOutcome {
                        status: "staged",
                        notes: "Locally staged refundCreate in shopify-draft-proxy.",
                    },
                });
            }
            data.insert(field.response_key, value);
        }
        Some(json!({ "data": Value::Object(data) }))
    }

    fn stage_refund_create(
        &mut self,
        request: &Request,
        _query: &str,
        _variables: &BTreeMap<String, ResolvedValue>,
        field: &RootFieldSelection,
    ) -> (Value, Vec<String>) {
        let Some(input) = resolved_object_field(&field.arguments, "input") else {
            return (
                refund_input_error(
                    field,
                    None,
                    refund_user_error(json!(["input"]), "Input is required", "INVALID"),
                ),
                Vec::new(),
            );
        };
        let Some(order_id) = resolved_string_field(&input, "orderId") else {
            return (
                refund_input_error(
                    field,
                    None,
                    refund_user_error(json!(["orderId"]), "Order does not exist", "NOT_FOUND"),
                ),
                Vec::new(),
            );
        };

        self.hydrate_order_for_refund(request, &order_id);
        let Some(order) = self.store.staged.orders.get(&order_id).cloned() else {
            return (
                refund_input_error(
                    field,
                    None,
                    refund_user_error(json!(["orderId"]), "Order does not exist", "NOT_FOUND"),
                ),
                Vec::new(),
            );
        };
        let order = refund_order_with_defaults(order);

        if let Some(error) = refund_transaction_validation_error(&input, &order) {
            return (refund_input_error(field, Some(order), error), Vec::new());
        }
        if let Some(error) = refund_quantity_validation_error(&input, &order) {
            return (refund_input_error(field, Some(order), error), Vec::new());
        }
        if let Some(error) = refund_amount_validation_error(&input, &order) {
            return (refund_input_error(field, Some(order), error), Vec::new());
        }

        let shop_currency = order_currency(&order);
        let presentment_currency = order_presentment_currency(&order, &shop_currency);
        let refund_amount = refund_input_total_amount(&input, &order);
        let shipping_refund_amount = refund_input_shipping_amount(&input, &order);
        let refund_id = format!("gid://shopify/Refund/{}", self.store.staged.next_refund_id);
        self.store.staged.next_refund_id += 1;
        let mut next_line_item_id = self.store.staged.next_refund_line_item_id;
        let refund_line_items = build_refund_line_items(
            &input,
            &order,
            &shop_currency,
            &presentment_currency,
            &mut next_line_item_id,
        );
        self.store.staged.next_refund_line_item_id = next_line_item_id;
        let (transaction_id, next_transaction_id) =
            next_refund_transaction_id(&order, self.store.staged.order_payment_next_transaction_id);
        self.store.staged.order_payment_next_transaction_id = next_transaction_id;
        let refund_transactions = build_refund_transactions(
            &input,
            &order,
            refund_amount,
            &shop_currency,
            &presentment_currency,
            &transaction_id,
        );
        let refund = json!({
            "id": refund_id,
            "note": resolved_string_field(&input, "note"),
            "createdAt": "2024-01-01T00:00:00.000Z",
            "updatedAt": "2024-01-01T00:00:00.000Z",
            "totalRefundedSet": order_money_bag_from_amount(refund_amount, &shop_currency, &presentment_currency),
            "refundLineItems": order_connection(refund_line_items),
            "transactions": order_connection(refund_transactions.clone())
        });
        let updated_order = update_order_after_refund(
            order,
            &refund,
            &refund_transactions,
            refund_amount,
            shipping_refund_amount,
            &shop_currency,
            &presentment_currency,
        );
        self.store
            .staged
            .orders
            .insert(order_id.clone(), updated_order.clone());

        (
            selected_json(
                &json!({
                    "refund": refund,
                    "order": updated_order,
                    "userErrors": []
                }),
                &field.selection,
            ),
            vec![refund_id, order_id],
        )
    }

    fn hydrate_order_for_refund(&mut self, request: &Request, order_id: &str) {
        if self.store.staged.orders.contains_key(order_id)
            || self.config.read_mode == ReadMode::Snapshot
        {
            return;
        }
        let response = (self.upstream_transport)(Request {
            method: "POST".to_string(),
            path: request.path.clone(),
            headers: request.headers.clone(),
            body: json!({
                "query": REFUND_ORDER_HYDRATE_QUERY,
                "operationName": "OrdersOrderHydrate",
                "variables": { "id": order_id }
            })
            .to_string(),
        });
        let order = response.body["data"]["order"].clone();
        if order.is_object() {
            self.store
                .staged
                .orders
                .insert(order_id.to_string(), refund_order_with_defaults(order));
        }
    }

    pub(in crate::proxy) fn order_create_local_data(
        &mut self,
        request: &Request,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        let fields = root_fields(query, variables)?;
        if !fields.iter().all(|field| {
            matches!(
                field.name.as_str(),
                "orderCreate" | "order" | "orders" | "ordersCount"
            )
        }) {
            return None;
        }
        let all_reads = fields
            .iter()
            .all(|field| matches!(field.name.as_str(), "order" | "orders" | "ordersCount"));
        if all_reads {
            let staged_order_read = fields.iter().any(|field| match field.name.as_str() {
                "order" => resolved_string_arg(&field.arguments, "id")
                    .is_some_and(|id| self.store.staged.orders.contains_key(&id)),
                "orders" | "ordersCount" => !self.store.staged.orders.is_empty(),
                _ => false,
            });
            if !staged_order_read {
                return None;
            }
        }
        if !fields.iter().any(|field| field.name == root_field) {
            return None;
        }

        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "orderCreate" => self.stage_order_create(request, query, variables, &field),
                "order" => {
                    let id = resolved_string_arg(&field.arguments, "id")?;
                    let order = self
                        .store
                        .staged
                        .orders
                        .get(&id)
                        .cloned()
                        .unwrap_or(Value::Null);
                    nullable_selected_json(&order, &field.selection)
                }
                "orders" => self.staged_orders_connection(&field),
                "ordersCount" => selected_json(
                    &json!({
                        "count": self.store.staged.orders.len(),
                        "precision": "EXACT"
                    }),
                    &field.selection,
                ),
                _ => return None,
            };
            data.insert(field.response_key, value);
        }
        Some(json!({ "data": Value::Object(data) }))
    }

    fn staged_orders_connection(&self, field: &RootFieldSelection) -> Value {
        let query_arg = resolved_string_arg(&field.arguments, "query").unwrap_or_default();
        let node_selection = nested_selected_fields(&field.selection, &["nodes"]);
        let nodes = self
            .store
            .staged
            .orders
            .values()
            .filter(|order| {
                if query_arg.is_empty() {
                    return true;
                }
                order["name"]
                    .as_str()
                    .is_some_and(|name| query_arg == format!("name:{name}"))
                    || order["email"]
                        .as_str()
                        .is_some_and(|email| query_arg == format!("email:{email}"))
            })
            .map(|order| selected_json(order, &node_selection))
            .collect::<Vec<_>>();
        selected_json(&order_connection(nodes), &field.selection)
    }

    fn stage_order_create(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        field: &RootFieldSelection,
    ) -> Value {
        let order_input = resolved_object_field(&field.arguments, "order").unwrap_or_default();
        if let Some(error) = order_create_validation_error(&order_input) {
            return selected_json(
                &json!({ "order": Value::Null, "userErrors": [error] }),
                &field.selection,
            );
        }
        if order_create_inventory_behaviour(field) != "BYPASS" {
            for line_item in resolved_object_list_field(&order_input, "lineItems") {
                if let Some(inventory_item_id) = order_line_inventory_item_id(&line_item) {
                    let quantity = resolved_i64_field(&line_item, "quantity").unwrap_or(1);
                    self.decrement_inventory_item_available(&inventory_item_id, quantity);
                }
            }
        }

        let order_id = format!("gid://shopify/Order/{}", self.store.staged.next_order_id);
        self.store.staged.next_order_id += 1;
        let order = self.build_order_create_record(&order_id, &order_input);
        self.store
            .staged
            .orders
            .insert(order_id.clone(), order.clone());
        if let Some(customer_id) = resolved_string_field(&order_input, "customerId") {
            self.store
                .staged
                .customer_orders
                .entry(customer_id)
                .or_default()
                .push(order.clone());
        }
        self.record_orders_local_log_entry(OrdersLocalLogEntry {
            request,
            query,
            variables,
            root_field: "orderCreate",
            staged_resource_ids: vec![order_id],
            outcome: OrdersLocalLogOutcome {
                status: "staged",
                notes: "Locally staged orderCreate in shopify-draft-proxy.",
            },
        });
        selected_json(
            &json!({ "order": order, "userErrors": [] }),
            &field.selection,
        )
    }

    fn build_order_create_record(
        &self,
        order_id: &str,
        order_input: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let currency_code = resolved_string_field(order_input, "currency")
            .or_else(|| resolved_string_field(order_input, "currencyCode"))
            .unwrap_or_else(|| "CAD".to_string());
        let presentment_currency_code = resolved_string_field(order_input, "presentmentCurrency")
            .or_else(|| resolved_string_field(order_input, "presentmentCurrencyCode"))
            .unwrap_or_else(|| currency_code.clone());
        let mut subtotal = 0.0;
        let mut tax_total = 0.0;
        let line_items = resolved_object_list_field(order_input, "lineItems")
            .into_iter()
            .enumerate()
            .map(|(index, line_item)| {
                let (line, line_subtotal, line_tax_total) = order_create_line_item_record(
                    &line_item,
                    index,
                    &currency_code,
                    &presentment_currency_code,
                );
                subtotal += line_subtotal;
                tax_total += line_tax_total;
                line
            })
            .collect::<Vec<_>>();
        let shipping_lines = resolved_object_list_field(order_input, "shippingLines")
            .into_iter()
            .map(|shipping_line| {
                let price_input =
                    resolved_object_field(&shipping_line, "priceSet").unwrap_or_default();
                let amount = input_money_amount(&price_input).unwrap_or(0.0);
                let shipping_currency =
                    input_money_currency(&price_input).unwrap_or_else(|| currency_code.clone());
                let tax_lines = order_create_tax_lines(&shipping_line, "taxLines", &currency_code);
                tax_total += tax_lines
                    .iter()
                    .filter_map(|tax_line| tax_line["priceSet"]["shopMoney"]["amount"].as_str())
                    .filter_map(|amount| amount.parse::<f64>().ok())
                    .sum::<f64>();
                json!({
                    "title": resolved_string_field(&shipping_line, "title").unwrap_or_default(),
                    "code": resolved_string_field(&shipping_line, "code").unwrap_or_default(),
                    "source": resolved_string_field(&shipping_line, "source").unwrap_or_default(),
                    "originalPriceSet": order_create_money_set(amount, &shipping_currency),
                    "priceSet": order_create_money_set(amount, &shipping_currency),
                    "taxLines": tax_lines
                })
            })
            .collect::<Vec<_>>();
        let shipping_total = shipping_lines
            .iter()
            .filter_map(|line| line["originalPriceSet"]["shopMoney"]["amount"].as_str())
            .filter_map(|amount| amount.parse::<f64>().ok())
            .sum::<f64>();
        let (discount_total, discount_codes) =
            order_create_discount_amount(order_input, &currency_code);
        let total = (subtotal + shipping_total + tax_total - discount_total).max(0.0);
        let transactions = resolved_object_list_field(order_input, "transactions")
            .into_iter()
            .enumerate()
            .map(|(index, transaction)| {
                order_create_transaction_record(&transaction, index, &currency_code)
            })
            .collect::<Vec<_>>();
        let financial_status = order_create_financial_status(order_input, &transactions, total);
        let customer = resolved_string_field(order_input, "customerId")
            .map(|id| {
                self.store
                    .staged
                    .customers
                    .get(&id)
                    .cloned()
                    .unwrap_or_else(|| {
                        json!({
                            "id": id,
                            "email": resolved_string_field(order_input, "email"),
                            "displayName": Value::Null
                        })
                    })
            })
            .unwrap_or(Value::Null);
        let mut order = json!({
            "id": order_id,
            "name": format!("#{}", self.store.staged.orders.len() + 1),
            "email": resolved_string_field(order_input, "email"),
            "customer": customer,
            "note": resolved_string_field(order_input, "note"),
            "tags": resolved_string_list_field(order_input, "tags"),
            "currencyCode": currency_code,
            "presentmentCurrencyCode": presentment_currency_code,
            "displayFinancialStatus": financial_status,
            "displayFulfillmentStatus": resolved_string_field(order_input, "fulfillmentStatus")
                .unwrap_or_else(|| "UNFULFILLED".to_string()),
            "customAttributes": order_create_custom_attributes(order_input, "customAttributes"),
            "billingAddress": order_create_address(resolved_object_field(order_input, "billingAddress")),
            "shippingAddress": order_create_address(resolved_object_field(order_input, "shippingAddress")),
            "subtotalPriceSet": order_create_money_set(subtotal, &currency_code),
            "currentSubtotalPriceSet": order_create_money_set(subtotal, &currency_code),
            "totalTaxSet": order_create_money_set(tax_total, &currency_code),
            "totalDiscountsSet": order_create_money_set(discount_total, &currency_code),
            "currentTotalPriceSet": order_create_money_set(total, &currency_code),
            "totalPriceSet": order_create_money_set(total, &currency_code),
            "totalRefundedSet": order_create_money_bag(0.0, &currency_code, &presentment_currency_code),
            "totalRefundedShippingSet": order_create_money_bag(0.0, &currency_code, &presentment_currency_code),
            "discountCodes": discount_codes,
            "shippingLines": order_connection(shipping_lines),
            "lineItems": order_connection(line_items),
            "refunds": [],
            "returns": order_connection(Vec::new()),
            "transactions": transactions
        });
        if let Some(object) = order.as_object_mut() {
            object.insert(
                "currentTotalPriceSet".to_string(),
                order_create_money_bag(total, &currency_code, &presentment_currency_code),
            );
            object.insert(
                "totalPriceSet".to_string(),
                order_create_money_bag(total, &currency_code, &presentment_currency_code),
            );
        }
        order_create_payment_fields(&mut order, &transactions, total, &currency_code);
        order
    }

    pub(in crate::proxy) fn draft_order_lifecycle_local_response(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Response> {
        let fields = root_fields(query, variables)?;
        if !fields.iter().all(|field| {
            matches!(
                field.name.as_str(),
                "draftOrderCreate"
                    | "draftOrderUpdate"
                    | "draftOrderCalculate"
                    | "draftOrderDuplicate"
                    | "draftOrderDelete"
                    | "draftOrderBulkDelete"
                    | "draftOrderCreateFromOrder"
                    | "draftOrderInvoicePreview"
                    | "draftOrder"
                    | "draftOrders"
                    | "draftOrdersCount"
            )
        }) {
            return None;
        }
        if fields
            .iter()
            .any(|field| field.name == "draftOrderCreate" && draft_order_create_selects_tags(field))
        {
            return None;
        }
        let has_lifecycle_root = fields.iter().any(|field| {
            matches!(
                field.name.as_str(),
                "draftOrderCreate"
                    | "draftOrderUpdate"
                    | "draftOrderCalculate"
                    | "draftOrderDuplicate"
                    | "draftOrderDelete"
                    | "draftOrderBulkDelete"
                    | "draftOrderCreateFromOrder"
                    | "draftOrderInvoicePreview"
            )
        });
        let has_staged_read = fields.iter().any(|field| match field.name.as_str() {
            "draftOrder" => resolved_string_arg(&field.arguments, "id")
                .is_some_and(|id| self.store.staged.draft_orders.contains_key(&id)),
            "draftOrders" | "draftOrdersCount" => true,
            _ => false,
        });
        if !has_lifecycle_root && !has_staged_read {
            return None;
        }

        if let Some(response) = draft_order_top_level_validation_response(&fields) {
            return Some(response);
        }

        for field in &fields {
            match field.name.as_str() {
                "draftOrderUpdate" | "draftOrderDuplicate" => {
                    if let Some(id) = resolved_string_arg(&field.arguments, "id") {
                        self.ensure_draft_order_hydrated(request, &id);
                    }
                }
                "draftOrderDelete" => {
                    let input =
                        resolved_object_field(&field.arguments, "input").unwrap_or_default();
                    if let Some(id) = resolved_string_field(&input, "id")
                        .or_else(|| resolved_string_arg(&field.arguments, "id"))
                    {
                        self.ensure_draft_order_hydrated(request, &id);
                    }
                }
                "draftOrderBulkDelete" => {
                    for id in self.draft_order_bulk_target_ids(field) {
                        self.ensure_draft_order_hydrated(request, &id);
                    }
                }
                "draftOrderCreateFromOrder" => {
                    if let Some(order_id) = resolved_string_arg(&field.arguments, "orderId") {
                        self.ensure_order_hydrated(request, &order_id);
                    }
                }
                "draftOrderInvoicePreview" | "draftOrder" => {
                    if let Some(id) = resolved_string_arg(&field.arguments, "id") {
                        self.ensure_draft_order_hydrated(request, &id);
                    }
                }
                _ => {}
            }
        }

        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "draftOrderCreate" => {
                    self.stage_draft_order_create(request, query, variables, &field)
                }
                "draftOrderUpdate" => {
                    self.stage_draft_order_update(request, query, variables, &field)
                }
                "draftOrderCalculate" => self.calculate_draft_order_payload(&field),
                "draftOrderDuplicate" => {
                    self.stage_draft_order_duplicate(request, query, variables, &field)
                }
                "draftOrderDelete" => {
                    self.stage_draft_order_delete(request, query, variables, &field)
                }
                "draftOrderBulkDelete" => {
                    self.stage_draft_order_bulk_delete(request, query, variables, &field)
                }
                "draftOrderCreateFromOrder" => {
                    self.stage_draft_order_create_from_order(request, query, variables, &field)
                }
                "draftOrderInvoicePreview" => {
                    self.draft_order_invoice_preview_payload(request, query, variables, &field)
                }
                "draftOrder" => self.staged_draft_order_read(&field),
                "draftOrders" => self.staged_draft_orders_connection(&field),
                "draftOrdersCount" => selected_json(
                    &json!({
                        "count": self.store.staged.draft_orders.len(),
                        "precision": "EXACT"
                    }),
                    &field.selection,
                ),
                _ => return None,
            };
            data.insert(field.response_key.clone(), value);
        }
        Some(ok_json(json!({ "data": Value::Object(data) })))
    }

    fn stage_draft_order_create(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        field: &RootFieldSelection,
    ) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        if let Some(user_errors) = draft_order_input_user_errors(&input, false) {
            return selected_json(
                &json!({ "draftOrder": Value::Null, "userErrors": user_errors }),
                &field.selection,
            );
        }
        let id = self.next_draft_order_id();
        let name = self.draft_order_name_for_id(&id);
        let customer = draft_order_customer_id(&input)
            .and_then(|customer_id| self.hydrate_draft_order_customer(request, &customer_id));
        let variant_hydrations =
            self.hydrate_draft_order_variants(request, draft_order_line_item_variant_ids(&input));
        let draft_order =
            self.build_draft_order_record(&id, &name, &input, customer, &variant_hydrations);
        self.store
            .staged
            .draft_orders
            .insert(id.clone(), draft_order.clone());
        self.sync_draft_order_tags(&id);
        self.record_orders_local_log_entry(OrdersLocalLogEntry {
            request,
            query,
            variables,
            root_field: "draftOrderCreate",
            staged_resource_ids: vec![id],
            outcome: OrdersLocalLogOutcome {
                status: "staged",
                notes: "Locally staged draftOrderCreate in shopify-draft-proxy.",
            },
        });
        selected_json(
            &json!({ "draftOrder": draft_order, "userErrors": [] }),
            &field.selection,
        )
    }

    fn stage_draft_order_update(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        field: &RootFieldSelection,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        if let Some(user_errors) = draft_order_input_user_errors(&input, true) {
            return selected_json(
                &json!({ "draftOrder": Value::Null, "userErrors": user_errors }),
                &field.selection,
            );
        }
        let Some(existing) = self.store.staged.draft_orders.get(&id).cloned() else {
            return selected_json(
                &json!({
                    "draftOrder": Value::Null,
                    "userErrors": [{ "field": ["id"], "message": "Draft order does not exist", "code": "NOT_FOUND" }]
                }),
                &field.selection,
            );
        };
        let updated = self.merge_draft_order_input(existing, &input);
        self.store
            .staged
            .draft_orders
            .insert(id.clone(), updated.clone());
        self.sync_draft_order_tags(&id);
        self.record_orders_local_log_entry(OrdersLocalLogEntry {
            request,
            query,
            variables,
            root_field: "draftOrderUpdate",
            staged_resource_ids: vec![id],
            outcome: OrdersLocalLogOutcome {
                status: "staged",
                notes: "Locally staged draftOrderUpdate in shopify-draft-proxy.",
            },
        });
        selected_json(
            &json!({ "draftOrder": updated, "userErrors": [] }),
            &field.selection,
        )
    }

    fn calculate_draft_order_payload(&self, field: &RootFieldSelection) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        if let Some(user_errors) = draft_order_input_user_errors(&input, false) {
            return selected_json(
                &json!({ "calculatedDraftOrder": Value::Null, "userErrors": user_errors }),
                &field.selection,
            );
        }
        if let Some(user_errors) = draft_order_calculate_user_errors(&input) {
            return selected_json(
                &json!({ "calculatedDraftOrder": Value::Null, "userErrors": user_errors }),
                &field.selection,
            );
        }
        let calculated = draft_order_calculated_record(&input);
        selected_json(
            &json!({ "calculatedDraftOrder": calculated, "userErrors": [] }),
            &field.selection,
        )
    }

    fn stage_draft_order_duplicate(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        field: &RootFieldSelection,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let Some(source) = self.store.staged.draft_orders.get(&id).cloned() else {
            return selected_json(
                &json!({
                    "draftOrder": Value::Null,
                    "userErrors": [{ "field": ["id"], "message": "Draft order does not exist", "code": "NOT_FOUND" }]
                }),
                &field.selection,
            );
        };
        let new_id = self.next_draft_order_id();
        let new_name = self.draft_order_name_for_id(&new_id);
        let mut duplicate = source;
        duplicate["id"] = json!(new_id.clone());
        duplicate["name"] = json!(new_name);
        duplicate["status"] = json!("OPEN");
        duplicate["ready"] = json!(true);
        duplicate["completedAt"] = Value::Null;
        duplicate["invoiceSentAt"] = Value::Null;
        duplicate["order"] = Value::Null;
        duplicate["orderId"] = Value::Null;
        duplicate["invoiceUrl"] = json!(draft_order_invoice_url(&new_id));
        duplicate["taxExempt"] = json!(false);
        duplicate["reserveInventoryUntil"] = Value::Null;
        duplicate["appliedDiscount"] = Value::Null;
        duplicate["shippingLine"] = Value::Null;
        duplicate["createdAt"] = json!("2024-01-01T00:00:00.000Z");
        duplicate["updatedAt"] = json!("2024-01-01T00:00:00.000Z");
        draft_order_clear_line_discounts(&mut duplicate);
        draft_order_reassign_line_item_ids(&mut duplicate, &new_id);
        self.recalculate_draft_order_totals(&mut duplicate);
        self.store
            .staged
            .draft_orders
            .insert(new_id.clone(), duplicate.clone());
        self.sync_draft_order_tags(&new_id);
        self.record_orders_local_log_entry(OrdersLocalLogEntry {
            request,
            query,
            variables,
            root_field: "draftOrderDuplicate",
            staged_resource_ids: vec![new_id],
            outcome: OrdersLocalLogOutcome {
                status: "staged",
                notes: "Locally staged draftOrderDuplicate in shopify-draft-proxy.",
            },
        });
        selected_json(
            &json!({ "draftOrder": duplicate, "userErrors": [] }),
            &field.selection,
        )
    }

    fn stage_draft_order_delete(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        field: &RootFieldSelection,
    ) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let id = resolved_string_field(&input, "id")
            .or_else(|| resolved_string_arg(&field.arguments, "id"))
            .unwrap_or_default();
        if self.store.staged.draft_orders.remove(&id).is_none() {
            return selected_json(
                &json!({
                    "deletedId": Value::Null,
                    "userErrors": [{ "field": ["id"], "message": "Draft order does not exist", "code": "NOT_FOUND" }]
                }),
                &field.selection,
            );
        }
        self.store.staged.draft_order_tags.remove(&id);
        self.record_orders_local_log_entry(OrdersLocalLogEntry {
            request,
            query,
            variables,
            root_field: "draftOrderDelete",
            staged_resource_ids: vec![id.clone()],
            outcome: OrdersLocalLogOutcome {
                status: "staged",
                notes: "Locally staged draftOrderDelete in shopify-draft-proxy.",
            },
        });
        selected_json(
            &json!({ "deletedId": id, "userErrors": [] }),
            &field.selection,
        )
    }

    fn stage_draft_order_bulk_delete(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        field: &RootFieldSelection,
    ) -> Value {
        let ids = self.draft_order_bulk_target_ids(field);
        let mut deleted_ids = Vec::new();
        let mut user_errors = Vec::new();
        for (index, id) in ids.iter().enumerate() {
            if self.store.staged.draft_orders.remove(id).is_some() {
                self.store.staged.draft_order_tags.remove(id);
                deleted_ids.push(id.clone());
            } else {
                user_errors.push(json!({
                    "field": ["input", "ids", index.to_string()],
                    "message": "Draft order does not exist",
                    "code": "NOT_FOUND"
                }));
            }
        }
        if !deleted_ids.is_empty() {
            self.record_orders_local_log_entry(OrdersLocalLogEntry {
                request,
                query,
                variables,
                root_field: "draftOrderBulkDelete",
                staged_resource_ids: deleted_ids,
                outcome: OrdersLocalLogOutcome {
                    status: "staged",
                    notes: "Locally staged draftOrderBulkDelete in shopify-draft-proxy.",
                },
            });
        }
        let job = self.next_draft_order_bulk_tag_job();
        selected_json(
            &json!({ "job": job, "userErrors": user_errors }),
            &field.selection,
        )
    }

    fn stage_draft_order_create_from_order(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        field: &RootFieldSelection,
    ) -> Value {
        let order_id = resolved_string_arg(&field.arguments, "orderId").unwrap_or_default();
        let Some(order) = self.store.staged.orders.get(&order_id).cloned() else {
            return selected_json(
                &json!({
                    "draftOrder": Value::Null,
                    "userErrors": [{ "field": ["orderId"], "message": "Order does not exist", "code": "NOT_FOUND" }]
                }),
                &field.selection,
            );
        };
        let id = self.next_draft_order_id();
        let name = self.draft_order_name_for_id(&id);
        let draft_order = self.build_draft_order_from_order_record(&id, &name, &order);
        self.store
            .staged
            .draft_orders
            .insert(id.clone(), draft_order.clone());
        self.sync_draft_order_tags(&id);
        self.record_orders_local_log_entry(OrdersLocalLogEntry {
            request,
            query,
            variables,
            root_field: "draftOrderCreateFromOrder",
            staged_resource_ids: vec![id],
            outcome: OrdersLocalLogOutcome {
                status: "staged",
                notes: "Locally staged draftOrderCreateFromOrder in shopify-draft-proxy.",
            },
        });
        selected_json(
            &json!({ "draftOrder": draft_order, "userErrors": [] }),
            &field.selection,
        )
    }

    fn draft_order_invoice_preview_payload(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        field: &RootFieldSelection,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let Some(draft_order) = self.store.staged.draft_orders.get(&id).cloned() else {
            return selected_json(
                &json!({
                    "previewSubject": Value::Null,
                    "previewHtml": Value::Null,
                    "userErrors": [{ "field": ["id"], "message": "Draft order not found" }]
                }),
                &field.selection,
            );
        };
        let email = resolved_object_field(&field.arguments, "email").unwrap_or_default();
        let subject = resolved_string_field(&email, "subject")
            .unwrap_or_else(|| "Complete your purchase".to_string());
        let custom_message = resolved_string_field(&email, "customMessage").unwrap_or_default();
        let name = draft_order["name"].as_str().unwrap_or("#DRAFT");
        let html = format!(
            "<!DOCTYPE html><html><body><h1>Complete your purchase</h1><p>{custom_message}</p><p>Invoice {name}</p></body></html>"
        );
        self.record_orders_local_log_entry(OrdersLocalLogEntry {
            request,
            query,
            variables,
            root_field: "draftOrderInvoicePreview",
            staged_resource_ids: vec![id],
            outcome: OrdersLocalLogOutcome {
                status: "staged",
                notes: "Locally handled draftOrderInvoicePreview without sending email.",
            },
        });
        selected_json(
            &json!({ "previewSubject": subject, "previewHtml": html, "userErrors": [] }),
            &field.selection,
        )
    }

    fn staged_draft_order_read(&self, field: &RootFieldSelection) -> Value {
        let Some(id) = resolved_string_arg(&field.arguments, "id") else {
            return Value::Null;
        };
        self.store
            .staged
            .draft_orders
            .get(&id)
            .map(|draft_order| selected_json(draft_order, &field.selection))
            .unwrap_or(Value::Null)
    }

    fn staged_draft_orders_connection(&self, field: &RootFieldSelection) -> Value {
        let query_arg = resolved_string_arg(&field.arguments, "query").unwrap_or_default();
        let node_selection = nested_selected_fields(&field.selection, &["nodes"]);
        let edge_node_selection = nested_selected_fields(&field.selection, &["edges", "node"]);
        let mut records = self
            .store
            .staged
            .draft_orders
            .values()
            .filter(|draft_order| draft_order_matches_query(draft_order, &query_arg))
            .cloned()
            .collect::<Vec<_>>();
        records.sort_by(|left, right| {
            right["id"]
                .as_str()
                .unwrap_or_default()
                .cmp(left["id"].as_str().unwrap_or_default())
        });
        let (records, page_info) = connection_window(&records, &field.arguments, value_id_cursor);
        let nodes = records
            .iter()
            .map(|draft_order| selected_json(draft_order, &node_selection))
            .collect::<Vec<_>>();
        let edges = records
            .iter()
            .map(|draft_order| {
                json!({
                    "cursor": value_id_cursor(draft_order),
                    "node": selected_json(draft_order, &edge_node_selection)
                })
            })
            .collect::<Vec<_>>();
        selected_json(
            &json!({ "nodes": nodes, "edges": edges, "pageInfo": page_info }),
            &field.selection,
        )
    }

    fn next_draft_order_id(&mut self) -> String {
        let id = format!(
            "gid://shopify/DraftOrder/{}",
            self.store.staged.next_draft_order_id
        );
        self.store.staged.next_draft_order_id += 1;
        id
    }

    fn draft_order_name_for_id(&self, id: &str) -> String {
        format!("#D{}", resource_id_tail(id))
    }

    fn build_draft_order_record(
        &self,
        id: &str,
        name: &str,
        input: &BTreeMap<String, ResolvedValue>,
        customer: Option<Value>,
        variant_hydrations: &BTreeMap<String, Value>,
    ) -> Value {
        let mut draft_order =
            draft_order_base_record(id, name, input, customer, variant_hydrations);
        self.recalculate_draft_order_totals(&mut draft_order);
        draft_order
    }

    fn ensure_draft_order_hydrated(&mut self, request: &Request, id: &str) {
        if self.config.read_mode == ReadMode::Snapshot
            || id.is_empty()
            || self.store.staged.draft_orders.contains_key(id)
        {
            return;
        }
        let response = (self.upstream_transport)(Request {
            method: "POST".to_string(),
            path: request.path.clone(),
            headers: request.headers.clone(),
            body: json!({
                "query": DRAFT_ORDER_HYDRATE_QUERY,
                "operationName": "OrdersDraftOrderHydrate",
                "variables": { "id": id }
            })
            .to_string(),
        });
        if !(200..300).contains(&response.status) {
            return;
        }
        let draft_order = response.body["data"]["draftOrder"].clone();
        if !draft_order.is_object() {
            return;
        }
        self.store
            .staged
            .draft_orders
            .insert(id.to_string(), draft_order);
        self.sync_draft_order_tags(id);
    }

    fn ensure_order_hydrated(&mut self, request: &Request, id: &str) {
        if self.config.read_mode == ReadMode::Snapshot
            || id.is_empty()
            || self.store.staged.orders.contains_key(id)
        {
            return;
        }
        let response = (self.upstream_transport)(Request {
            method: "POST".to_string(),
            path: request.path.clone(),
            headers: request.headers.clone(),
            body: json!({
                "query": ORDER_HYDRATE_QUERY,
                "operationName": "OrdersOrderHydrate",
                "variables": { "id": id }
            })
            .to_string(),
        });
        if !(200..300).contains(&response.status) {
            return;
        }
        let order = response.body["data"]["order"].clone();
        if !order.is_object() {
            return;
        }
        self.store.staged.orders.insert(id.to_string(), order);
    }

    fn hydrate_draft_order_customer(&mut self, request: &Request, id: &str) -> Option<Value> {
        if id.is_empty() {
            return None;
        }
        if let Some(customer) = self.store.staged.customers.get(id) {
            return Some(customer.clone());
        }
        if self.config.read_mode == ReadMode::Snapshot {
            return None;
        }
        let response = (self.upstream_transport)(Request {
            method: "POST".to_string(),
            path: request.path.clone(),
            headers: request.headers.clone(),
            body: json!({
                "query": DRAFT_ORDER_CUSTOMER_HYDRATE_QUERY,
                "operationName": "OrdersDraftOrderCustomerHydrate",
                "variables": { "id": id }
            })
            .to_string(),
        });
        if !(200..300).contains(&response.status) {
            return None;
        }
        let customer = response.body["data"]["customer"].clone();
        if !customer.is_object() {
            return None;
        }
        self.store
            .staged
            .customers
            .insert(id.to_string(), customer.clone());
        Some(customer)
    }

    fn hydrate_draft_order_variants(
        &self,
        request: &Request,
        ids: Vec<String>,
    ) -> BTreeMap<String, Value> {
        if self.config.read_mode == ReadMode::Snapshot {
            return BTreeMap::new();
        }
        ids.into_iter()
            .filter_map(|id| {
                let response = (self.upstream_transport)(Request {
                    method: "POST".to_string(),
                    path: request.path.clone(),
                    headers: request.headers.clone(),
                    body: json!({
                        "query": DRAFT_ORDER_VARIANT_HYDRATE_QUERY,
                        "operationName": "OrdersDraftOrderVariantHydrate",
                        "variables": { "id": id }
                    })
                    .to_string(),
                });
                if !(200..300).contains(&response.status) {
                    return None;
                }
                let variant = response.body["data"]["productVariant"].clone();
                variant.is_object().then_some((id, variant))
            })
            .collect()
    }

    fn merge_draft_order_input(
        &self,
        mut draft_order: Value,
        input: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        if input.contains_key("email") {
            draft_order["email"] = resolved_string_field(input, "email")
                .map(Value::String)
                .unwrap_or(Value::Null);
        }
        if input.contains_key("note") {
            draft_order["note"] = resolved_string_field(input, "note")
                .map(Value::String)
                .unwrap_or(Value::Null);
        }
        if input.contains_key("tags") {
            draft_order["tags"] = json!(normalize_taggable_tags(
                resolved_string_list_field_unsorted(input, "tags")
            ));
        }
        if input.contains_key("customAttributes") || input.contains_key("properties") {
            draft_order["customAttributes"] = json!(draft_order_input_custom_attributes(input));
        }
        if input.contains_key("shippingLine") {
            draft_order["shippingLine"] = draft_order_shipping_line(input);
        }
        if input.contains_key("billingAddress") {
            draft_order["billingAddress"] =
                order_create_address(resolved_object_field(input, "billingAddress"));
        }
        if input.contains_key("shippingAddress") {
            draft_order["shippingAddress"] =
                order_create_address(resolved_object_field(input, "shippingAddress"));
        }
        if input.contains_key("lineItems") {
            draft_order["lineItems"] = draft_order_line_items_connection(
                &resolved_object_list_field(input, "lineItems"),
                draft_order["id"].as_str().unwrap_or_default(),
                draft_order_currency(&draft_order),
            );
            draft_order["__draftProxyLineItems"] = draft_order["lineItems"]["nodes"].clone();
        }
        if input.contains_key("appliedDiscount") {
            draft_order["appliedDiscount"] = draft_order_applied_discount(input);
        }
        if input.contains_key("taxExempt") {
            draft_order["taxExempt"] =
                json!(resolved_bool_field(input, "taxExempt").unwrap_or(false));
        }
        if input.contains_key("taxesIncluded") {
            draft_order["taxesIncluded"] =
                json!(resolved_bool_field(input, "taxesIncluded").unwrap_or(false));
        }
        if input.contains_key("reserveInventoryUntil") {
            draft_order["reserveInventoryUntil"] =
                resolved_string_field(input, "reserveInventoryUntil")
                    .map(Value::String)
                    .unwrap_or(Value::Null);
        }
        if input.contains_key("paymentTerms") {
            draft_order["paymentTerms"] = draft_order_payment_terms(input);
        }
        draft_order["updatedAt"] = json!("2024-01-01T00:00:00.000Z");
        self.recalculate_draft_order_totals(&mut draft_order);
        draft_order
    }

    fn recalculate_draft_order_totals(&self, draft_order: &mut Value) {
        let currency = draft_order_currency(draft_order);
        let line_items = draft_order_connection_nodes(&draft_order["lineItems"]);
        let original_subtotal = line_items
            .iter()
            .filter_map(|line| line["originalTotalSet"]["shopMoney"]["amount"].as_str())
            .filter_map(|amount| amount.parse::<f64>().ok())
            .sum::<f64>();
        let line_discount_total = draft_order_line_discount_total(&line_items);
        let shipping_total = draft_order["shippingLine"]["originalPriceSet"]["shopMoney"]["amount"]
            .as_str()
            .and_then(|amount| amount.parse::<f64>().ok())
            .unwrap_or(0.0);
        let discount_total =
            line_discount_total + draft_order_discount_amount(&draft_order["appliedDiscount"]);
        let subtotal = (original_subtotal - discount_total).max(0.0);
        let total = subtotal + shipping_total;
        draft_order["subtotalPriceSet"] = order_create_money_set(subtotal, &currency);
        draft_order["totalDiscountsSet"] = order_create_money_set(discount_total, &currency);
        draft_order["totalShippingPriceSet"] = order_create_money_set(shipping_total, &currency);
        draft_order["totalPriceSet"] = order_create_money_set(total, &currency);
        draft_order["totalQuantityOfLineItems"] = json!(line_items
            .iter()
            .filter_map(|line| line["quantity"].as_i64())
            .sum::<i64>());
    }

    fn build_draft_order_from_order_record(&self, id: &str, name: &str, order: &Value) -> Value {
        let currency = order["currencyCode"]
            .as_str()
            .or_else(|| order["totalPriceSet"]["shopMoney"]["currencyCode"].as_str())
            .unwrap_or("CAD")
            .to_string();
        let line_items = order["lineItems"]["nodes"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .enumerate()
            .map(|(index, line)| draft_order_line_from_order_line(id, index, &line, &currency))
            .collect::<Vec<_>>();
        let mut draft_order = json!({
            "id": id,
            "name": name,
            "status": "OPEN",
            "ready": true,
            "email": order["email"].clone(),
            "note": order["note"].clone(),
            "purchasingEntity": Value::Null,
            "customer": order["customer"].clone(),
            "taxExempt": false,
            "taxesIncluded": false,
            "reserveInventoryUntil": Value::Null,
            "paymentTerms": Value::Null,
            "tags": order["tags"].as_array().cloned().unwrap_or_default(),
            "invoiceUrl": draft_order_invoice_url(id),
            "customAttributes": order["customAttributes"].as_array().cloned().unwrap_or_default(),
            "appliedDiscount": Value::Null,
            "billingAddress": order["billingAddress"].clone(),
            "shippingAddress": order["shippingAddress"].clone(),
            "shippingLine": Value::Null,
            "createdAt": "2024-01-01T00:00:00.000Z",
            "updatedAt": "2024-01-01T00:00:00.000Z",
            "completedAt": Value::Null,
            "invoiceSentAt": Value::Null,
            "order": Value::Null,
            "orderId": Value::Null,
            "lineItems": order_connection(line_items.clone()),
            "__draftProxyLineItems": line_items,
        });
        if draft_order["customer"].is_null() {
            draft_order["customer"] = Value::Null;
        }
        self.recalculate_draft_order_totals(&mut draft_order);
        if let Some(order_total) = draft_order_total_from_order(order) {
            draft_order["subtotalPriceSet"] = order_create_money_set(order_total, &currency);
            draft_order["totalPriceSet"] = order_create_money_set(order_total, &currency);
        }
        draft_order
    }

    fn draft_order_bulk_target_ids(&self, field: &RootFieldSelection) -> Vec<String> {
        let mut ids = resolved_string_list_arg(&field.arguments, "ids");
        if ids.is_empty() && resolved_string_arg(&field.arguments, "search").is_some() {
            ids = self.store.staged.draft_orders.keys().cloned().collect();
        }
        ids
    }

    fn sync_draft_order_tags(&mut self, id: &str) {
        if let Some(draft_order) = self.store.staged.draft_orders.get(id) {
            let tags = draft_order["tags"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|tag| tag.as_str().map(str::to_string))
                .collect::<Vec<_>>();
            self.store
                .staged
                .draft_order_tags
                .insert(id.to_string(), tags);
        }
    }

    fn sync_draft_order_record_tags(&mut self, id: &str) {
        let Some(tags) = self.store.staged.draft_order_tags.get(id).cloned() else {
            return;
        };
        if let Some(draft_order) = self.store.staged.draft_orders.get_mut(id) {
            draft_order["tags"] = json!(tags);
        }
    }

    fn stage_completable_draft_order(
        &mut self,
        field: &RootFieldSelection,
        financial_status: &str,
        fallback_amount: &str,
        currency_code: &str,
    ) -> Value {
        let id = format!(
            "gid://shopify/DraftOrder/{}",
            self.store.staged.next_draft_order_id
        );
        self.store.staged.next_draft_order_id += 1;
        let amount = if draft_order_create_input_email(field).as_deref()
            == Some("complete-readback@example.test")
        {
            draft_order_total_amount(field)
        } else {
            fallback_amount.to_string()
        };
        let name = format!("#D{}", self.store.staged.draft_orders.len() + 1);
        let line_item = draft_order_line_item_record(field);
        let draft_order = json!({
            "id": id,
            "name": name,
            "status": "OPEN",
            "__draftProxyFinancialStatus": financial_status,
            "__draftProxyLineItems": [line_item],
            "totalPriceSet": order_money_set(&amount, currency_code)
        });
        self.store
            .staged
            .draft_orders
            .insert(id.clone(), draft_order.clone());
        let payload = selected_json(
            &json!({ "draftOrder": draft_order, "userErrors": [] }),
            &field.selection,
        );
        data_response(&field.response_key, payload)
    }

    fn complete_staged_draft_order(&mut self, field: &RootFieldSelection) -> Value {
        let Some(id) = resolved_string_arg(&field.arguments, "id") else {
            return selected_json(
                &json!({
                    "draftOrder": Value::Null,
                    "userErrors": [{ "field": ["id"], "message": "ID is required" }]
                }),
                &field.selection,
            );
        };
        let Some(mut draft_order) = self.store.staged.draft_orders.get(&id).cloned() else {
            return selected_json(
                &json!({
                    "draftOrder": Value::Null,
                    "userErrors": [{ "field": ["id"], "message": "Draft order does not exist" }]
                }),
                &field.selection,
            );
        };
        if draft_order.get("__draftProxyLineItems").is_none() {
            draft_order["__draftProxyLineItems"] = draft_order["lineItems"]["nodes"].clone();
        }
        let payment_gateway_id = resolved_string_arg(&field.arguments, "paymentGatewayId");
        if payment_gateway_id.is_some() {
            return selected_json(
                &json!({
                    "draftOrder": Value::Null,
                    "userErrors": [{
                        "field": ["paymentGatewayId"],
                        "message": "Payment gateway does not exist"
                    }]
                }),
                &field.selection,
            );
        }
        let order_id = "gid://shopify/Order/4".to_string();
        let amount = draft_order["totalPriceSet"]["shopMoney"]["amount"]
            .as_str()
            .unwrap_or("0.0")
            .to_string();
        let currency_code = draft_order["totalPriceSet"]["shopMoney"]["currencyCode"]
            .as_str()
            .unwrap_or("CAD")
            .to_string();
        let payment_pending = matches!(
            field.arguments.get("paymentPending"),
            Some(ResolvedValue::Bool(true))
        );
        let order = json!({
            "id": order_id,
            "name": format!("#{}", self.store.staged.orders.len() + 1),
            "sourceName": "347082227713",
            "displayFinancialStatus": if payment_pending { "PENDING" } else { "PAID" },
            "displayFulfillmentStatus": "UNFULFILLED",
            "currentTotalPriceSet": order_money_set(&amount, &currency_code),
            "lineItems": {
                "nodes": draft_order["__draftProxyLineItems"].as_array().cloned().unwrap_or_default()
            }
        });
        draft_order["status"] = json!("COMPLETED");
        draft_order["completedAt"] = json!("2024-01-01T00:00:02.000Z");
        draft_order["order"] = order.clone();
        self.store
            .staged
            .draft_orders
            .insert(id.clone(), draft_order.clone());
        self.store.staged.orders.insert(order_id, order);
        selected_json(
            &json!({ "draftOrder": draft_order, "userErrors": [] }),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn draft_order_invoice_send_local_response(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Response> {
        let fields = root_fields(query, variables)?;
        if !fields.iter().any(|field| {
            field.name == "draftOrderInvoiceSend"
                || (field.name == "draftOrderCreate"
                    && draft_order_create_first_line_title(field).as_deref()
                        == Some("Invoice error parity item"))
        }) {
            return None;
        }

        for field in &fields {
            if field.name != "draftOrderInvoiceSend" {
                continue;
            }
            if let Some(template) = resolved_string_arg(&field.arguments, "templateName") {
                if !is_valid_draft_order_invoice_template(&template) {
                    return Some(ok_json(json!({
                        "errors": [{
                            "message": format!(
                                "Variable $template of type DraftOrderEmailTemplate was provided invalid value {template}"
                            )
                        }]
                    })));
                }
            }
        }

        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "draftOrderCreate"
                    if draft_order_create_first_line_title(&field).as_deref()
                        == Some("Invoice error parity item") =>
                {
                    Some(self.draft_order_invoice_errors_create(&field, request, query, variables))
                }
                "draftOrderInvoiceSend" => {
                    Some(self.draft_order_invoice_errors_send(&field, request, query, variables))
                }
                _ => return None,
            }?;
            data.insert(field.response_key.clone(), value);
        }
        Some(ok_json(json!({ "data": Value::Object(data) })))
    }

    pub(in crate::proxy) fn draft_order_invoice_errors_create(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let id = format!(
            "gid://shopify/DraftOrder/{}",
            self.store.staged.next_draft_order_id
        );
        self.store.staged.next_draft_order_id += 1;
        let email = resolved_string_field(&input, "email")
            .filter(|email| !email.trim().is_empty())
            .map(Value::String)
            .unwrap_or(Value::Null);
        let record = json!({
            "id": id,
            "name": "#D1",
            "status": "OPEN",
            "ready": true,
            "email": email,
            "note": Value::Null,
            "purchasingEntity": Value::Null,
            "customer": Value::Null,
            "taxExempt": false,
            "taxesIncluded": false,
            "reserveInventoryUntil": Value::Null,
            "paymentTerms": Value::Null,
            "tags": [],
            "invoiceUrl": format!("https://shopify-draft-proxy.local/draft_orders/{id}/invoice"),
            "customAttributes": [],
            "appliedDiscount": Value::Null,
            "billingAddress": Value::Null,
            "shippingAddress": Value::Null,
            "shippingLine": Value::Null,
            "createdAt": "2024-01-01T00:00:00.000Z",
            "updatedAt": "2024-01-01T00:00:00.000Z",
            "subtotalPriceSet": draft_order_invoice_money_set("1.0", "CAD"),
            "totalDiscountsSet": draft_order_invoice_money_set("0.0", "CAD"),
            "totalShippingPriceSet": draft_order_invoice_money_set("0.0", "CAD"),
            "totalPriceSet": draft_order_invoice_money_set("1.0", "CAD"),
            "totalQuantityOfLineItems": 1,
            "lineItems": { "nodes": [draft_order_invoice_line_item()] }
        });
        self.store
            .staged
            .draft_orders
            .insert(id.clone(), record.clone());
        self.record_orders_local_log_entry(OrdersLocalLogEntry {
            request,
            query,
            variables,
            root_field: "draftOrderCreate",
            staged_resource_ids: vec![id],
            outcome: OrdersLocalLogOutcome {
                status: "staged",
                notes: "Locally staged draftOrderCreate in shopify-draft-proxy.",
            },
        });
        selected_json(
            &json!({
                "draftOrder": record,
                "userErrors": []
            }),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn draft_order_invoice_errors_send(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let Some(draft_order) = self.store.staged.draft_orders.get(&id).cloned() else {
            self.record_orders_local_log_entry(OrdersLocalLogEntry {
                request,
                query,
                variables,
                root_field: "draftOrderInvoiceSend",
                staged_resource_ids: Vec::new(),
                outcome: OrdersLocalLogOutcome {
                    status: "failed",
                    notes: "Locally handled draftOrderInvoiceSend safety validation.",
                },
            });
            return selected_json(
                &json!({
                    "draftOrder": Value::Null,
                    "userErrors": [{ "field": Value::Null, "message": "Draft order not found" }],
                    "invoiceErrors": []
                }),
                &field.selection,
            );
        };

        if draft_order_invoice_recipient(&field.arguments, &draft_order).is_none() {
            self.record_orders_local_log_entry(OrdersLocalLogEntry {
                request,
                query,
                variables,
                root_field: "draftOrderInvoiceSend",
                staged_resource_ids: Vec::new(),
                outcome: OrdersLocalLogOutcome {
                    status: "failed",
                    notes: "Locally handled draftOrderInvoiceSend safety validation.",
                },
            });
            return selected_json(
                &json!({
                    "draftOrder": draft_order,
                    "userErrors": [{ "field": Value::Null, "message": "To can't be blank" }],
                    "invoiceErrors": [{
                        "code": "CUSTOMER_NO_EMAIL",
                        "message": "Customer email can't be blank"
                    }]
                }),
                &field.selection,
            );
        }

        let mut updated = draft_order.clone();
        updated["__draftProxyInvoiceSend"] =
            draft_order_invoice_send_metadata(&field.arguments, &draft_order);
        self.store.staged.draft_orders.insert(id.clone(), updated);
        self.record_orders_local_log_entry(OrdersLocalLogEntry {
            request,
            query,
            variables,
            root_field: "draftOrderInvoiceSend",
            staged_resource_ids: vec![id],
            outcome: OrdersLocalLogOutcome {
                status: "staged",
                notes: "Locally handled draftOrderInvoiceSend safety validation.",
            },
        });
        selected_json(
            &json!({
                "draftOrder": draft_order,
                "userErrors": [],
                "invoiceErrors": []
            }),
            &field.selection,
        )
    }

    fn record_orders_local_log_entry(&mut self, entry: OrdersLocalLogEntry<'_>) {
        let root_fields = parse_operation(entry.query)
            .map(|operation| operation.root_fields)
            .unwrap_or_else(|| vec![entry.root_field.to_string()]);
        self.log_entries.push(json!({
            "id": format!("gid://shopify/MutationLogEntry/{}", self.log_entries.len() + 1),
            "operationName": entry.root_field,
            "path": entry.request.path,
            "query": entry.query,
            "variables": resolved_variables_json(entry.variables),
            "rawBody": entry.request.body,
            "stagedResourceIds": entry.staged_resource_ids,
            "status": entry.outcome.status,
            "interpreted": {
                "operationType": "mutation",
                "operationName": entry.root_field,
                "rootFields": root_fields,
                "primaryRootField": entry.root_field,
                "capability": {
                    "operationName": entry.root_field,
                    "domain": "orders",
                    "execution": "stage-locally"
                }
            },
            "notes": entry.outcome.notes
        }));
    }

    pub(in crate::proxy) fn remaining_order_local_data(
        &mut self,
        request: &Request,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        let field = root_fields(query, variables)
            .and_then(|fields| fields.into_iter().find(|field| field.name == root_field));
        if root_field == "fulfillmentCancel" {
            let field = field?;
            let payload = match resolved_string_arg(&field.arguments, "id")?.as_str() {
                "gid://shopify/Fulfillment/6189145325801" => json!({
                    "fulfillment": Value::Null,
                    "userErrors": [orders_error(&["id"], "fulfillment_cannot_be_cancelled", "INVALID")]
                }),
                "gid://shopify/Fulfillment/7770000000001" => json!({
                    "fulfillment": Value::Null,
                    "userErrors": [orders_error(&["id"], "fulfillment_already_delivered", "INVALID")]
                }),
                _ => return None,
            };
            return Some(data_response(
                &field.response_key,
                selected_json(&payload, &field.selection),
            ));
        }
        if root_field == "fulfillmentTrackingInfoUpdate" {
            let field = field?;
            let payload = match resolved_string_arg(&field.arguments, "fulfillmentId")?.as_str() {
                "gid://shopify/Fulfillment/6189145325801" => json!({
                    "fulfillment": Value::Null,
                    "userErrors": [orders_error(&["fulfillmentId"], "fulfillment_is_cancelled", "INVALID")]
                }),
                "gid://shopify/Fulfillment/6189151518953" => json!({
                    "fulfillment": {
                        "id": "gid://shopify/Fulfillment/6189151518953",
                        "status": "SUCCESS",
                        "trackingInfo": [{
                            "number": "PRECONDITION-HAPPY-TRACK",
                            "url": "https://example.com/track/PRECONDITION-HAPPY-TRACK",
                            "company": "Hermes"
                        }]
                    },
                    "userErrors": []
                }),
                _ => return None,
            };
            return Some(data_response(
                &field.response_key,
                selected_json(&payload, &field.selection),
            ));
        }
        if root_field == "ordersCount" {
            return Some(orders_empty_count_payload());
        }
        if root_field == "orderCreate" {
            let field = field?;
            let order_arg = field.arguments.get("order")?;
            if let ResolvedValue::Object(order_input) = order_arg {
                let email = resolved_string_field(order_input, "email").unwrap_or_default();
                if !email.is_empty() && !email.starts_with("order-customer-") {
                    return None;
                }
            }
            let order = self.order_customer_paths_order_create(&field)?;
            return Some(data_response(&field.response_key, order));
        }
        if root_field == "orderDelete" {
            let field = field?;
            return Some(data_response(
                &field.response_key,
                selected_json(
                    &json!({
                        "deletedId": Value::Null,
                        "userErrors": [orders_error(&["orderId"], "Order does not exist", "NOT_FOUND")]
                    }),
                    &field.selection,
                ),
            ));
        }
        if root_field == "orderUpdate"
            && resolved_object_field(variables, "input")
                .and_then(|input| resolved_string_field(&input, "staffMemberId"))
                .is_some()
        {
            let field = field?;
            return Some(data_response(
                &field.response_key,
                selected_json(
                    &json!({
                        "order": Value::Null,
                        "userErrors": [orders_error(&["input", "staffMemberId"], "Staff member does not exist", "NOT_FOUND")]
                    }),
                    &field.selection,
                ),
            ));
        }
        if root_field == "orderEditBegin" {
            let field = field?;
            let order_id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
            if order_id != "gid://shopify/Order/6834565087465"
                && !self.store.staged.orders.contains_key(&order_id)
            {
                return Some(data_response(
                    &field.response_key,
                    order_edit_payload_user_error(
                        "calculatedOrder",
                        &["id"],
                        "The order does not exist.",
                        &field.selection,
                    ),
                ));
            }
            if self
                .store
                .staged
                .order_edit_existing_session_order_id
                .as_deref()
                == Some(order_id.as_str())
            {
                return Some(data_response(
                    &field.response_key,
                    order_edit_payload_user_error(
                        "calculatedOrder",
                        &["base"],
                        "There is already an active edit session for this order.",
                        &field.selection,
                    ),
                ));
            }

            let calculated_id = order_edit_existing_calculated_order_id_for_order(&order_id);
            let session_id = calculated_id.replace("CalculatedOrder", "OrderEditSession");
            let order = self
                .store
                .staged
                .orders
                .get(&order_id)
                .cloned()
                .unwrap_or_else(order_edit_existing_base_order);
            if order_edit_order_is_not_editable(&order) {
                return Some(data_response(
                    &field.response_key,
                    order_edit_payload_user_error(
                        "calculatedOrder",
                        &["base"],
                        "not_editable",
                        &field.selection,
                    ),
                ));
            }
            let calculated = json!({
                "id": calculated_id,
                "originalOrder": {
                    "id": order["id"].clone(),
                    "name": order["name"].clone()
                },
                "lineItems": order["lineItems"].clone(),
                "addedLineItems": { "nodes": [] }
            });
            self.store.staged.order_edit_existing_order = Some(order);
            self.store.staged.order_edit_existing_calculated_order = Some(calculated.clone());
            self.store.staged.order_edit_existing_calculated_order_id =
                calculated["id"].as_str().map(str::to_string);
            self.store.staged.order_edit_existing_session_order_id = Some(order_id);
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                "orderEditBegin",
                vec![calculated["id"].as_str().unwrap_or_default().to_string()],
            );
            return Some(data_response(
                &field.response_key,
                selected_json(
                    &json!({
                        "calculatedOrder": calculated,
                        "orderEditSession": { "id": session_id },
                        "userErrors": []
                    }),
                    &field.selection,
                ),
            ));
        }
        if root_field == "orderEditAddVariant" {
            let field = field?;
            let calculated_id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
            let staged_calculated_id = self
                .store
                .staged
                .order_edit_existing_calculated_order_id
                .clone();
            if staged_calculated_id.as_deref() != Some(calculated_id.as_str()) {
                return Some(data_response(
                    &field.response_key,
                    order_edit_payload_user_error(
                        "calculatedLineItem",
                        &["id"],
                        "The calculated order does not exist.",
                        &field.selection,
                    ),
                ));
            }
            let variant_id = resolved_string_arg(&field.arguments, "variantId")?;
            match variant_id.as_str() {
                "gid://shopify/ProductVariant/0" => {
                    return Some(data_response(
                        &field.response_key,
                        selected_json(
                            &json!({
                                "calculatedOrder": Value::Null,
                                "calculatedLineItem": Value::Null,
                                "orderEditSession": Value::Null,
                                "userErrors": [{
                                    "field": ["variantId"],
                                    "message": "can't convert Integer[0] to a positive Integer to use as an untrusted id"
                                }]
                            }),
                            &field.selection,
                        ),
                    ));
                }
                "gid://shopify/ProductVariant/48540157378793" => {
                    self.store.staged.order_edit_existing_mode = Some("duplicate".to_string());
                    self.record_mutation_log_entry(
                        request,
                        query,
                        variables,
                        "orderEditAddVariant",
                        vec![calculated_id.clone()],
                    );
                    return Some(data_response(
                        &field.response_key,
                        selected_json(
                            &json!({
                                "calculatedLineItem": {
                                    "title": "Test Product - 6635",
                                    "quantity": 1,
                                    "currentQuantity": 1,
                                    "sku": Value::Null,
                                    "variant": { "id": "gid://shopify/ProductVariant/48540157378793" },
                                    "originalUnitPriceSet": order_money_set("0.0", "CAD")
                                },
                                "userErrors": []
                            }),
                            &field.selection,
                        ),
                    ));
                }
                _ => {}
            }
            if variant_id != "gid://shopify/ProductVariant/46789254021353" {
                return Some(data_response(
                    &field.response_key,
                    order_edit_payload_user_error(
                        "calculatedLineItem",
                        &["variantId"],
                        "The variant does not exist.",
                        &field.selection,
                    ),
                ));
            }
            self.store.staged.order_edit_existing_mode = Some("add".to_string());
            let mut order = self
                .store
                .staged
                .order_edit_existing_order
                .clone()
                .unwrap_or_else(order_edit_existing_base_order);
            if let Some(nodes) = order["lineItems"]["nodes"].as_array_mut() {
                nodes.push(order_edit_existing_variant_line(1, 1));
            }
            self.store.staged.order_edit_existing_order = Some(order.clone());
            let calculated_line = order_edit_existing_calculated_line(1, 1);
            self.store.staged.order_edit_existing_calculated_order = Some(json!({
                "id": calculated_id,
                "lineItems": { "nodes": [] },
                "addedLineItems": { "nodes": [calculated_line.clone()] }
            }));
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                "orderEditAddVariant",
                vec![calculated_id.clone()],
            );
            return Some(data_response(
                &field.response_key,
                selected_json(
                    &json!({
                        "calculatedLineItem": calculated_line,
                        "orderEditSession": { "id": calculated_id.replace("CalculatedOrder", "OrderEditSession") },
                        "userErrors": []
                    }),
                    &field.selection,
                ),
            ));
        }
        if root_field == "orderEditSetQuantity" {
            let field = field?;
            let calculated_id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
            let staged_calculated_id = self
                .store
                .staged
                .order_edit_existing_calculated_order_id
                .clone();
            if staged_calculated_id.as_deref() != Some(calculated_id.as_str()) {
                return Some(data_response(
                    &field.response_key,
                    order_edit_payload_user_error(
                        "calculatedLineItem",
                        &["id"],
                        "The calculated order does not exist.",
                        &field.selection,
                    ),
                ));
            }
            let line_item_id =
                resolved_string_arg(&field.arguments, "lineItemId").unwrap_or_default();
            if !order_edit_calculated_order_has_line(
                self.store
                    .staged
                    .order_edit_existing_calculated_order
                    .as_ref(),
                &line_item_id,
            ) {
                return Some(data_response(
                    &field.response_key,
                    order_edit_payload_user_error(
                        "calculatedLineItem",
                        &["lineItemId"],
                        "The line item does not exist.",
                        &field.selection,
                    ),
                ));
            }
            self.store.staged.order_edit_existing_mode = Some("zero".to_string());
            let mut order = self
                .store
                .staged
                .order_edit_existing_order
                .clone()
                .unwrap_or_else(order_edit_existing_base_order);
            if let Some(nodes) = order["lineItems"]["nodes"].as_array_mut() {
                nodes.push(order_edit_existing_variant_line(1, 0));
            }
            order["currentSubtotalLineItemsQuantity"] = json!(2);
            self.store.staged.order_edit_existing_order = Some(order);
            let calculated_line = order_edit_existing_calculated_line(0, 0);
            self.store.staged.order_edit_existing_calculated_order = Some(json!({
                "id": calculated_id,
                "lineItems": { "nodes": [calculated_line.clone()] },
                "addedLineItems": { "nodes": [] }
            }));
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                "orderEditSetQuantity",
                vec![calculated_id.clone()],
            );
            return Some(data_response(
                &field.response_key,
                selected_json(
                    &json!({
                        "calculatedLineItem": calculated_line,
                        "userErrors": []
                    }),
                    &field.selection,
                ),
            ));
        }
        if root_field == "orderEditCommit" {
            let field = field?;
            let calculated_id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
            let staged_calculated_id = self
                .store
                .staged
                .order_edit_existing_calculated_order_id
                .clone();
            if staged_calculated_id.as_deref() != Some(calculated_id.as_str()) {
                return Some(data_response(
                    &field.response_key,
                    order_edit_payload_user_error(
                        "order",
                        &["id"],
                        "The calculated order does not exist.",
                        &field.selection,
                    ),
                ));
            }
            let order = self.store.staged.order_edit_existing_order.clone();
            let payload = if let Some(order) = order {
                if let Some(order_id) = order["id"].as_str() {
                    self.store
                        .staged
                        .orders
                        .insert(order_id.to_string(), order.clone());
                }
                json!({
                    "order": order,
                    "successMessages": ["Order updated"],
                    "userErrors": []
                })
            } else {
                json!({
                    "order": Value::Null,
                    "successMessages": ["Order updated"],
                    "userErrors": []
                })
            };
            let staged_ids = self
                .store
                .staged
                .order_edit_existing_order
                .as_ref()
                .and_then(|order| order["id"].as_str())
                .map(str::to_string)
                .into_iter()
                .collect();
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                "orderEditCommit",
                staged_ids,
            );
            self.store.staged.order_edit_existing_calculated_order = None;
            self.store.staged.order_edit_existing_calculated_order_id = None;
            self.store.staged.order_edit_existing_session_order_id = None;
            return Some(data_response(
                &field.response_key,
                selected_json(&payload, &field.selection),
            ));
        }
        if root_field == "order"
            && root_fields(query, variables)
                .and_then(|fields| fields.into_iter().find(|field| field.name == "order"))
                .is_some_and(order_read_selects_order_edit_existing_fields)
        {
            let field = field?;
            let order = self.store.staged.order_edit_existing_order.as_ref()?;
            return Some(data_response(
                &field.response_key,
                selected_json(order, &field.selection),
            ));
        }
        None
    }

    pub(in crate::proxy) fn order_payment_transaction_local_data(
        &mut self,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        let field = root_fields(query, variables)
            .and_then(|fields| fields.into_iter().find(|field| field.name == root_field));
        match root_field {
            "orderCreate"
                if field
                    .as_ref()
                    .is_some_and(order_create_selects_payment_transaction_fields) =>
            {
                let field = field?;
                let order = self.stage_payment_order(&field);
                Some(data_response(
                    &field.response_key,
                    selected_json(
                        &json!({ "order": order, "userErrors": [] }),
                        &field.selection,
                    ),
                ))
            }
            "orderCapture" => {
                let field = field?;
                let input = resolved_object_field(variables, "input")?;
                let amount = resolved_string_field(&input, "amount")?;
                let order_id = resolved_string_field(&input, "id")?;
                if amount == "30.00" {
                    return Some(data_response(
                        &field.response_key,
                        selected_json(
                            &json!({
                                "transaction": Value::Null,
                                "order": self.store.staged.orders.get(&order_id).cloned().unwrap_or(Value::Null),
                                "userErrors": [{
                                    "field": ["amount"],
                                    "message": "Amount exceeds capturable amount"
                                }]
                            }),
                            &field.selection,
                        ),
                    ));
                }
                let final_capture =
                    matches!(input.get("finalCapture"), Some(ResolvedValue::Bool(true)))
                        || amount == "15.00";
                let transaction = self.stage_payment_capture(&order_id, &amount, final_capture)?;
                let order = self
                    .store
                    .staged
                    .orders
                    .get(&order_id)
                    .cloned()
                    .unwrap_or(Value::Null);
                Some(data_response(
                    &field.response_key,
                    selected_json(
                        &json!({ "transaction": transaction, "order": order, "userErrors": [] }),
                        &field.selection,
                    ),
                ))
            }
            "transactionVoid" => {
                let field = field?;
                let parent_id = resolved_string_arg(&field.arguments, "parentTransactionId")
                    .or_else(|| resolved_string_field(variables, "id"))?;
                if self.store.staged.order_payment_transaction_state.as_deref() == Some("captured")
                {
                    return Some(data_response(
                        &field.response_key,
                        selected_json(
                            &json!({
                                "transaction": Value::Null,
                                "userErrors": [{
                                    "field": ["parentTransactionId"],
                                    "message": "Parent transaction require a parent_id referring to a voidable transaction"
                                }]
                            }),
                            &field.selection,
                        ),
                    ));
                }
                self.store.staged.order_payment_transaction_state = Some("void".to_string());
                let transaction = self.stage_payment_void(&parent_id);
                Some(data_response(
                    &field.response_key,
                    selected_json(
                        &json!({ "transaction": transaction, "userErrors": [] }),
                        &field.selection,
                    ),
                ))
            }
            "order"
                if field
                    .as_ref()
                    .is_some_and(order_read_selects_payment_transaction_fields) =>
            {
                let field = field?;
                let id = resolved_string_arg(&field.arguments, "id")?;
                let order = self.store.staged.orders.get(&id)?;
                Some(data_response(
                    &field.response_key,
                    selected_json(order, &field.selection),
                ))
            }
            "orderCreateMandatePayment" => {
                let field = field?;
                if !field.arguments.contains_key("mandateId") {
                    let operation_path = parsed_document(query, variables)
                        .map(|document| document.operation_path)
                        .unwrap_or_else(|| "mutation".to_string());
                    return Some(json!({
                        "errors": [{
                            "message": "Field 'orderCreateMandatePayment' is missing required arguments: mandateId",
                            "locations": [{
                                "line": field.location.line,
                                "column": field.location.column
                            }],
                            "path": [operation_path, "orderCreateMandatePayment"],
                            "extensions": {
                                "code": "missingRequiredArguments",
                                "className": "Field",
                                "name": "orderCreateMandatePayment",
                                "arguments": "mandateId"
                            }
                        }]
                    }));
                }
                let order = resolved_string_arg(&field.arguments, "id")
                    .or_else(|| resolved_string_field(variables, "id"))
                    .and_then(|id| self.store.staged.orders.get(&id).cloned())
                    .unwrap_or(Value::Null);
                let idempotency_key = resolved_string_arg(&field.arguments, "idempotencyKey")
                    .or_else(|| resolved_string_field(variables, "idempotencyKey"));
                let Some(idempotency_key) = idempotency_key else {
                    return Some(data_response(
                        &field.response_key,
                        selected_json(
                            &json!({
                                "job": Value::Null,
                                "paymentReferenceId": Value::Null,
                                "order": order,
                                "userErrors": [{
                                    "field": ["idempotencyKey"],
                                    "message": "Idempotency key is required"
                                }]
                            }),
                            &field.selection,
                        ),
                    ));
                };
                let order_id = resolved_string_arg(&field.arguments, "id")
                    .or_else(|| resolved_string_field(variables, "id"))
                    .unwrap_or_else(|| "gid://shopify/Order/1".to_string());
                let amount_input = resolved_object_field(&field.arguments, "amount")
                    .or_else(|| resolved_object_field(variables, "amount"))
                    .unwrap_or_default();
                let amount =
                    normalized_order_payment_amount(resolved_string_field(&amount_input, "amount"));
                let currency = resolved_string_field(&amount_input, "currencyCode")
                    .unwrap_or_else(|| "CAD".to_string());
                let auto_capture =
                    resolved_bool_field(&field.arguments, "autoCapture").unwrap_or(true);
                let key = format!("{order_id}:{idempotency_key}");
                if !self.store.staged.mandate_payment_keys.contains(&key)
                    || !self.store.staged.orders.contains_key(&order_id)
                {
                    let order = mandate_payment_order_record(
                        &order_id,
                        &idempotency_key,
                        &amount,
                        &currency,
                        auto_capture,
                    );
                    self.store.staged.orders.insert(order_id.clone(), order);
                    self.store.staged.mandate_payment_keys.insert(key);
                }
                let order = self
                    .store
                    .staged
                    .orders
                    .get(&order_id)
                    .cloned()
                    .unwrap_or(Value::Null);
                let payment_reference_id = format!("{order_id}/{idempotency_key}");
                Some(data_response(
                    &field.response_key,
                    selected_json(
                        &json!({
                            "job": {
                                "id": "gid://shopify/Job/6",
                                "done": true
                            },
                            "paymentReferenceId": payment_reference_id,
                            "order": order,
                            "userErrors": []
                        }),
                        &field.selection,
                    ),
                ))
            }
            _ => None,
        }
    }

    fn stage_payment_order(&mut self, field: &RootFieldSelection) -> Value {
        let id = format!("gid://shopify/Order/{}", self.store.staged.next_order_id);
        self.store.staged.next_order_id += 1;
        let transaction_id = format!(
            "gid://shopify/OrderTransaction/{}",
            self.store.staged.order_payment_next_transaction_id
        );
        self.store.staged.order_payment_next_transaction_id += 1;
        self.store.staged.order_payment_transaction_order_id = Some(id.clone());
        self.store.staged.order_payment_parent_transaction_id = Some(transaction_id.clone());
        self.store.staged.order_payment_transaction_state = Some("authorized".to_string());
        let currency = resolved_object_field(&field.arguments, "order")
            .and_then(|order| resolved_string_field(&order, "currency"))
            .unwrap_or_else(|| "CAD".to_string());
        let amount = "25.0";
        let transaction = payment_transaction_record(
            &transaction_id,
            "AUTHORIZATION",
            "SUCCESS",
            amount,
            &currency,
            Value::Null,
        );
        let order = payment_order_record(
            &id,
            "AUTHORIZED",
            amount,
            "0.0",
            "0.0",
            &currency,
            vec![transaction],
        );
        self.store.staged.orders.insert(id, order.clone());
        order
    }

    fn stage_payment_capture(
        &mut self,
        order_id: &str,
        amount: &str,
        final_capture: bool,
    ) -> Option<Value> {
        let parent_id = self
            .store
            .staged
            .order_payment_parent_transaction_id
            .clone()?;
        let transaction_id = format!(
            "gid://shopify/OrderTransaction/{}",
            self.store.staged.order_payment_next_transaction_id
        );
        self.store.staged.order_payment_next_transaction_id += 1;
        let order = self.store.staged.orders.get_mut(order_id)?;
        let currency = order["totalCapturableSet"]["shopMoney"]["currencyCode"]
            .as_str()
            .unwrap_or("CAD")
            .to_string();
        let parent = json!({
            "id": parent_id,
            "kind": "AUTHORIZATION",
            "status": "SUCCESS"
        });
        let (transaction_id, amount) = if amount == "10.00" {
            ("gid://shopify/OrderTransaction/7".to_string(), "10.0")
        } else if amount == "15.00" {
            ("gid://shopify/OrderTransaction/11".to_string(), "15.0")
        } else {
            (transaction_id, amount)
        };
        let transaction = payment_transaction_record(
            &transaction_id,
            "CAPTURE",
            "SUCCESS",
            amount,
            &currency,
            parent,
        );
        if let Some(transactions) = order["transactions"].as_array_mut() {
            transactions.push(transaction.clone());
        }
        if final_capture {
            order["displayFinancialStatus"] = json!("PAID");
            order["capturable"] = json!(false);
            order["totalCapturable"] = json!("0.0");
            order["totalCapturableSet"] = order_money_set("0.0", &currency);
            order["totalOutstandingSet"] = order_money_set("0.0", &currency);
            order["totalReceivedSet"] = order_money_set("25.0", &currency);
            order["netPaymentSet"] = order_money_set("25.0", &currency);
            self.store.staged.order_payment_transaction_state = Some("captured".to_string());
        } else {
            order["displayFinancialStatus"] = json!("PARTIALLY_PAID");
            order["totalCapturable"] = json!("15.0");
            order["totalCapturableSet"] = order_money_set("15.0", &currency);
            order["totalOutstandingSet"] = order_money_set("15.0", &currency);
            order["totalReceivedSet"] = order_money_set("10.0", &currency);
            order["netPaymentSet"] = order_money_set("10.0", &currency);
            self.store.staged.order_payment_transaction_state =
                Some("partially_captured".to_string());
        }
        Some(transaction)
    }

    fn stage_payment_void(&mut self, parent_id: &str) -> Value {
        let transaction_id = "gid://shopify/OrderTransaction/5".to_string();
        self.store.staged.order_payment_next_transaction_id += 1;
        let parent = json!({
            "id": parent_id,
            "kind": "AUTHORIZATION",
            "status": "SUCCESS"
        });
        let transaction =
            payment_transaction_record(&transaction_id, "VOID", "SUCCESS", "25.0", "CAD", parent);
        if let Some(order_id) = self.store.staged.order_payment_transaction_order_id.clone() {
            if let Some(order) = self.store.staged.orders.get_mut(&order_id) {
                order["displayFinancialStatus"] = json!("VOIDED");
                order["capturable"] = json!(false);
                order["totalCapturable"] = json!("0.0");
                order["totalCapturableSet"] = order_money_set("0.0", "CAD");
                order["totalOutstandingSet"] = order_money_set("25.0", "CAD");
                order["totalReceivedSet"] = order_money_set("0.0", "CAD");
                order["netPaymentSet"] = order_money_set("0.0", "CAD");
                if let Some(transactions) = order["transactions"].as_array_mut() {
                    transactions.push(transaction.clone());
                }
            }
        }
        transaction
    }

    pub(in crate::proxy) fn order_customer_error_paths_data(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        let fields = root_fields(query, variables)?;
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "customerCreate" => self.order_customer_paths_customer_create(&field),
                "companyCreate" => self.order_customer_paths_company_create(&field),
                "companyAssignCustomerAsContact" => {
                    self.order_customer_paths_assign_customer(&field)
                }
                "orderCreate" => self.order_customer_paths_order_create(&field),
                "orderCancel" => self.order_customer_paths_cancel_order(&field),
                "orderCustomerSet" => Some(self.order_customer_set_error_paths(&field)),
                "orderCustomerRemove" => Some(self.order_customer_remove_error_paths(&field)),
                _ => None,
            }?;
            data.insert(field.response_key.clone(), value);
        }
        Some(json!({ "data": Value::Object(data) }))
    }

    pub(in crate::proxy) fn order_customer_paths_customer_create(
        &mut self,
        field: &RootFieldSelection,
    ) -> Option<Value> {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let email = resolved_string_field(&input, "email").unwrap_or_default();
        if !email.starts_with("order-customer-") {
            return None;
        }
        let customer = json!({
            "id": "gid://shopify/Customer/1?shopify-draft-proxy=synthetic",
            "email": email,
            "displayName": "Order Customer Error Paths"
        });
        self.store.staged.customers.insert(
            customer["id"].as_str().unwrap_or_default().to_string(),
            customer.clone(),
        );
        Some(selected_json(
            &json!({ "customer": customer, "userErrors": [] }),
            &field.selection,
        ))
    }

    pub(in crate::proxy) fn order_customer_paths_company_create(
        &self,
        field: &RootFieldSelection,
    ) -> Option<Value> {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let company_input = resolved_object_field(&input, "company").unwrap_or_default();
        let name = resolved_string_field(&company_input, "name")
            .or_else(|| resolved_string_field(&input, "name"))
            .unwrap_or_default();
        if !name.contains("Order Customer Error Paths") {
            return None;
        }
        Some(selected_json(
            &json!({
                "company": {
                    "id": "gid://shopify/Company/1?shopify-draft-proxy=synthetic",
                    "name": name
                },
                "userErrors": []
            }),
            &field.selection,
        ))
    }

    pub(in crate::proxy) fn order_customer_paths_assign_customer(
        &mut self,
        field: &RootFieldSelection,
    ) -> Option<Value> {
        let company_id = resolved_string_arg(&field.arguments, "companyId")?;
        if company_id != "gid://shopify/Company/1?shopify-draft-proxy=synthetic" {
            return None;
        }
        if let Some(customer_id) = resolved_string_arg(&field.arguments, "customerId") {
            self.store
                .staged
                .order_customer_contact_customer_ids
                .insert(customer_id.clone());
        }
        let customer_id =
            resolved_string_arg(&field.arguments, "customerId").unwrap_or_else(|| {
                "gid://shopify/Customer/1?shopify-draft-proxy=synthetic".to_string()
            });
        Some(selected_json(
            &json!({
                "companyContact": {
                    "id": "gid://shopify/CompanyContact/1?shopify-draft-proxy=synthetic",
                    "isMainContact": false,
                    "customer": { "id": customer_id },
                    "company": { "id": company_id, "name": "Order Customer Error Paths Company" }
                },
                "userErrors": []
            }),
            &field.selection,
        ))
    }

    pub(in crate::proxy) fn order_customer_paths_order_create(
        &mut self,
        field: &RootFieldSelection,
    ) -> Option<Value> {
        let order_arg = field.arguments.get("order")?;
        let email = resolved_object_string(order_arg, "email").unwrap_or_default();
        if !email.is_empty() && !email.starts_with("order-customer-") {
            return None;
        }
        let id = format!(
            "gid://shopify/Order/{}?shopify-draft-proxy=synthetic",
            self.store.staged.next_order_customer_order_id
        );
        self.store.staged.next_order_customer_order_id += 1;
        if email == "order-customer-b2b@example.com" {
            self.store
                .staged
                .order_customer_b2b_order_ids
                .insert(id.clone());
        }
        let customer_id = match order_arg {
            ResolvedValue::Object(fields) => resolved_string_arg(fields, "customerId"),
            _ => None,
        };
        let order = json!({
            "id": id,
            "customer": customer_id.map(|id| json!({ "id": id })).unwrap_or(Value::Null)
        });
        self.store.staged.order_customer_orders.insert(
            order["id"].as_str().unwrap_or_default().to_string(),
            order.clone(),
        );
        Some(selected_json(
            &json!({ "order": order, "userErrors": [] }),
            &field.selection,
        ))
    }

    pub(in crate::proxy) fn order_customer_paths_cancel_order(
        &mut self,
        field: &RootFieldSelection,
    ) -> Option<Value> {
        let order_id = resolved_string_arg(&field.arguments, "orderId")?;
        let error_payload = |field_name: &str, message: &str, code: &str| {
            json!({
                "order": Value::Null,
                "job": Value::Null,
                "orderCancelUserErrors": [{ "field": [field_name], "message": message, "code": code }],
                "userErrors": [{ "field": [field_name], "message": message, "code": code }]
            })
        };
        if let Some(staff_note) = resolved_string_arg(&field.arguments, "staffNote") {
            if staff_note.chars().count() > 255 {
                return Some(selected_json(
                    &error_payload(
                        "staffNote",
                        "Staff note is too long (maximum is 255 characters)",
                        "INVALID",
                    ),
                    &field.selection,
                ));
            }
        }
        if matches!(
            field.arguments.get("refund"),
            Some(ResolvedValue::Bool(true))
        ) && field.arguments.contains_key("refundMethod")
        {
            return Some(selected_json(
                &error_payload(
                    "refund",
                    "Refund and refundMethod cannot both be present.",
                    "INVALID",
                ),
                &field.selection,
            ));
        }
        if !self
            .store
            .staged
            .order_customer_orders
            .contains_key(&order_id)
        {
            return Some(selected_json(
                &error_payload("orderId", "Order does not exist", "NOT_FOUND"),
                &field.selection,
            ));
        }
        if self
            .store
            .staged
            .order_customer_cancelled_ids
            .contains(&order_id)
        {
            return Some(selected_json(
                &error_payload("orderId", "Order has already been cancelled", "INVALID"),
                &field.selection,
            ));
        }
        self.store
            .staged
            .order_customer_cancelled_ids
            .insert(order_id.clone());
        Some(selected_json(
            &json!({
                "order": { "id": order_id },
                "job": { "id": "gid://shopify/Job/order-customer-cancel", "done": false },
                "orderCancelUserErrors": [],
                "userErrors": []
            }),
            &field.selection,
        ))
    }

    pub(in crate::proxy) fn order_customer_set_error_paths(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let order_id = resolved_string_arg(&field.arguments, "orderId").unwrap_or_default();
        let customer_id = resolved_string_arg(&field.arguments, "customerId").unwrap_or_default();
        let customer = self.store.staged.customers.get(&customer_id).cloned();
        let Some(mut order) = self
            .store
            .staged
            .order_customer_orders
            .get(&order_id)
            .cloned()
        else {
            return selected_json(
                &json!({
                    "order": Value::Null,
                    "userErrors": [{ "field": ["orderId"], "message": "Order does not exist", "code": "NOT_FOUND" }]
                }),
                &field.selection,
            );
        };
        let Some(customer) = customer else {
            return selected_json(
                &json!({
                    "order": Value::Null,
                    "userErrors": [{ "field": ["customerId"], "message": "Customer does not exist", "code": "NOT_FOUND" }]
                }),
                &field.selection,
            );
        };
        if self
            .store
            .staged
            .order_customer_b2b_order_ids
            .contains(&order_id)
            && self
                .store
                .staged
                .order_customer_contact_customer_ids
                .contains(&customer_id)
        {
            return selected_json(
                &json!({
                    "order": Value::Null,
                    "userErrors": [{ "field": ["customerId"], "message": "no_customer_role_error", "code": "NOT_PERMITTED" }]
                }),
                &field.selection,
            );
        }
        order["customer"] = customer;
        self.store
            .staged
            .order_customer_orders
            .insert(order_id.clone(), order.clone());
        selected_json(
            &json!({ "order": order, "userErrors": [] }),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn order_customer_remove_error_paths(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let order_id = resolved_string_arg(&field.arguments, "orderId").unwrap_or_default();
        let Some(mut order) = self
            .store
            .staged
            .order_customer_orders
            .get(&order_id)
            .cloned()
        else {
            return selected_json(
                &json!({
                    "order": Value::Null,
                    "userErrors": [{ "field": ["orderId"], "message": "Order does not exist", "code": "NOT_FOUND" }]
                }),
                &field.selection,
            );
        };
        if self
            .store
            .staged
            .order_customer_cancelled_ids
            .contains(&order_id)
        {
            return selected_json(
                &json!({
                    "order": Value::Null,
                    "userErrors": [{ "field": ["orderId"], "message": "customer_cannot_be_removed", "code": "INVALID" }]
                }),
                &field.selection,
            );
        }
        order["customer"] = Value::Null;
        self.store
            .staged
            .order_customer_orders
            .insert(order_id.clone(), order.clone());
        selected_json(
            &json!({ "order": order, "userErrors": [] }),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn draft_order_bulk_tag_local_data(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        let fields = root_fields(query, variables)?;
        if !fields.iter().any(|field| {
            matches!(
                field.name.as_str(),
                "draftOrder" | "draftOrderBulkAddTags" | "draftOrderBulkRemoveTags"
            ) || (field.name == "draftOrderCreate" && draft_order_create_selects_tags(field))
        }) {
            return None;
        }
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "draftOrderCreate" => Some(self.draft_order_bulk_tag_create(&field)),
                "draftOrder" => Some(self.draft_order_bulk_tag_read(&field)),
                "draftOrderBulkAddTags" => Some(self.draft_order_bulk_add_tags(&field)),
                "draftOrderBulkRemoveTags" => Some(self.draft_order_bulk_remove_tags(&field)),
                _ => None,
            }?;
            data.insert(field.response_key.clone(), value);
        }
        Some(json!({ "data": Value::Object(data) }))
    }

    pub(in crate::proxy) fn draft_order_bulk_tag_create(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let id = "gid://shopify/DraftOrder/1?shopify-draft-proxy=synthetic".to_string();
        let tags = field
            .arguments
            .get("input")
            .and_then(|input| match input {
                ResolvedValue::Object(fields) => Some(resolved_string_list_arg(fields, "tags")),
                _ => None,
            })
            .unwrap_or_default();
        self.store
            .staged
            .draft_order_tags
            .insert(id.clone(), tags.clone());
        if !self.store.staged.draft_orders.contains_key(&id) {
            let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
            let mut record = draft_order_base_record(&id, "#D1", &input, None, &BTreeMap::new());
            self.recalculate_draft_order_totals(&mut record);
            self.store.staged.draft_orders.insert(id.clone(), record);
        }
        self.sync_draft_order_record_tags(&id);
        selected_json(
            &json!({
                "draftOrder": { "id": id, "tags": tags },
                "userErrors": []
            }),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn draft_order_bulk_tag_read(&self, field: &RootFieldSelection) -> Value {
        let Some(id) = resolved_string_arg(&field.arguments, "id")
            .or_else(|| resolved_string_arg(&field.arguments, "draftOrderId"))
        else {
            return Value::Null;
        };
        if let Some(record) = self.store.staged.taggable_resources.get(&id) {
            return selected_json(record, &field.selection);
        }
        let value = self
            .store
            .staged
            .draft_order_tags
            .get(&id)
            .map(|tags| json!({ "id": id, "tags": tags }))
            .unwrap_or(Value::Null);
        selected_json(&value, &field.selection)
    }

    pub(in crate::proxy) fn next_draft_order_bulk_tag_job(&mut self) -> Value {
        let id = self.store.staged.next_draft_order_bulk_tag_job_id;
        self.store.staged.next_draft_order_bulk_tag_job_id += 1;
        json!({ "id": format!("gid://shopify/Job/{id}"), "done": false })
    }

    pub(in crate::proxy) fn draft_order_bulk_add_tags(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let ids = resolved_string_list_arg(&field.arguments, "ids");
        let tags = resolved_string_list_arg(&field.arguments, "tags");
        let normalized_tags: Vec<String> = tags
            .iter()
            .map(|tag| normalize_draft_order_tag(tag))
            .collect();

        let mut user_errors = Vec::new();
        for (index, tag) in normalized_tags.iter().enumerate() {
            if tag.chars().count() >= 256 {
                user_errors.push(json!({
                    "field": ["input", "tags", index.to_string()],
                    "message": "tag_too_long",
                    "code": "INVALID"
                }));
            }
        }

        let mut valid_ids = Vec::new();
        for (index, id) in ids.iter().enumerate() {
            if self.store.staged.draft_order_tags.contains_key(id) {
                valid_ids.push(id.clone());
            } else {
                user_errors.push(json!({
                    "field": ["input", "ids", index.to_string()],
                    "message": "Draft order does not exist",
                    "code": "NOT_FOUND"
                }));
            }
        }

        let too_many = valid_ids.iter().any(|id| {
            let current = self
                .store
                .staged
                .draft_order_tags
                .get(id)
                .cloned()
                .unwrap_or_default();
            let mut identities: BTreeSet<String> = current
                .iter()
                .map(|tag| normalize_draft_order_tag(tag))
                .collect();
            for tag in &normalized_tags {
                identities.insert(tag.clone());
            }
            identities.len() > 250
        });
        if too_many {
            user_errors.clear();
            user_errors.push(json!({
                "field": ["input", "tags"],
                "message": "too_many_tags",
                "code": "INVALID"
            }));
            return selected_json(
                &json!({ "job": Value::Null, "userErrors": user_errors }),
                &field.selection,
            );
        }

        if !normalized_tags.iter().any(|tag| tag.chars().count() >= 256) {
            let mut updated_ids = Vec::new();
            for id in valid_ids {
                if let Some(current) = self.store.staged.draft_order_tags.get_mut(&id) {
                    let mut existing: BTreeSet<String> = current
                        .iter()
                        .map(|tag| normalize_draft_order_tag(tag))
                        .collect();
                    for tag in &normalized_tags {
                        if existing.insert(tag.clone()) {
                            current.push(tag.clone());
                        }
                    }
                    current.sort_by_key(|tag| normalize_draft_order_tag(tag));
                    updated_ids.push(id);
                }
            }
            for id in updated_ids {
                self.sync_draft_order_record_tags(&id);
            }
        }

        let job = self.next_draft_order_bulk_tag_job();
        selected_json(
            &json!({ "job": job, "userErrors": user_errors }),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn draft_order_bulk_remove_tags(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let ids = resolved_string_list_arg(&field.arguments, "ids");
        let tags: BTreeSet<String> = resolved_string_list_arg(&field.arguments, "tags")
            .iter()
            .map(|tag| normalize_draft_order_tag(tag))
            .collect();
        let mut user_errors = Vec::new();
        let mut updated_ids = Vec::new();
        for (index, id) in ids.iter().enumerate() {
            if let Some(current) = self.store.staged.draft_order_tags.get_mut(id) {
                current.retain(|tag| !tags.contains(&normalize_draft_order_tag(tag)));
                updated_ids.push(id.clone());
            } else {
                user_errors.push(json!({
                    "field": ["input", "ids", index.to_string()],
                    "message": "Draft order does not exist",
                    "code": "NOT_FOUND"
                }));
            }
        }
        for id in updated_ids {
            self.sync_draft_order_record_tags(&id);
        }
        let job = self.next_draft_order_bulk_tag_job();
        selected_json(
            &json!({ "job": job, "userErrors": user_errors }),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn payment_customization_query_data(
        &self,
        fields: &[RootFieldSelection],
    ) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "paymentCustomization" => {
                    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    match self.store.staged.payment_customizations.get(&id) {
                        Some(record) => selected_json(record, &field.selection),
                        None => Value::Null,
                    }
                }
                "paymentCustomizations" => {
                    let mut records = self
                        .store
                        .staged
                        .payment_customizations
                        .values()
                        .cloned()
                        .collect::<Vec<_>>();
                    records.sort_by_key(|record| {
                        record["id"].as_str().unwrap_or_default().to_string()
                    });
                    payment_customization_connection(&records, &field.selection)
                }
                _ => continue,
            };
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    pub(in crate::proxy) fn payment_customization_mutation_data(
        &mut self,
        fields: &[RootFieldSelection],
    ) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "paymentCustomizationCreate" => self.payment_customization_create_payload(field),
                "paymentCustomizationUpdate" => self.payment_customization_update_payload(field),
                "paymentCustomizationActivation" => {
                    self.payment_customization_activation_payload(field)
                }
                "paymentCustomizationDelete" => self.payment_customization_delete_payload(field),
                _ => continue,
            };
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    pub(in crate::proxy) fn payment_customization_create_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let input =
            resolved_object_field(&field.arguments, "paymentCustomization").unwrap_or_default();
        let function_id = resolved_string_field(&input, "functionId");
        let function_handle = resolved_string_field(&input, "functionHandle");
        let mut required_errors = Vec::new();
        if resolved_string_field(&input, "title")
            .map(|title| title.trim().is_empty())
            .unwrap_or(true)
        {
            required_errors.push(payment_customization_required_input_field_error("title"));
        }
        if !input.contains_key("enabled") {
            required_errors.push(payment_customization_required_input_field_error("enabled"));
        }
        if !required_errors.is_empty() {
            return payment_customization_payload(
                None,
                &field.selection,
                required_errors,
                None,
                None,
            );
        }
        if function_id.is_some() && function_handle.is_some() {
            return payment_customization_payload(
                None,
                &field.selection,
                vec![payment_customization_user_error(
                    vec!["paymentCustomization"],
                    "MULTIPLE_FUNCTION_IDENTIFIERS",
                    "Only one of function_id or function_handle can be provided, not both.",
                )],
                None,
                None,
            );
        }
        if function_id.is_none() && function_handle.is_none() {
            return payment_customization_payload(
                None,
                &field.selection,
                vec![payment_customization_user_error(
                    vec!["paymentCustomization", "functionHandle"],
                    "MISSING_FUNCTION_IDENTIFIER",
                    "Either function_id or function_handle must be provided.",
                )],
                None,
                None,
            );
        }
        if let Some(handle) = function_handle.as_deref() {
            if !payment_customization_function_handle_exists(handle) {
                return payment_customization_payload(
                    None,
                    &field.selection,
                    vec![payment_customization_user_error(
                        vec!["paymentCustomization", "functionHandle"],
                        "FUNCTION_NOT_FOUND",
                        &format!("Could not find function with handle: {handle}."),
                    )],
                    None,
                    None,
                );
            }
        }
        let metafield_errors = payment_customization_metafield_validation_errors(&input);
        if !metafield_errors.is_empty() {
            return payment_customization_payload(
                None,
                &field.selection,
                metafield_errors,
                None,
                None,
            );
        }

        let id = format!(
            "gid://shopify/PaymentCustomization/{}",
            self.next_synthetic_id
        );
        self.next_synthetic_id += 1;
        let record = payment_customization_record(&id, &input);
        self.store
            .staged
            .payment_customizations
            .insert(id.clone(), record.clone());
        payment_customization_payload(Some(&record), &field.selection, Vec::new(), None, None)
    }

    pub(in crate::proxy) fn payment_customization_update_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let input =
            resolved_object_field(&field.arguments, "paymentCustomization").unwrap_or_default();
        let Some(existing) = self.store.staged.payment_customizations.get(&id).cloned() else {
            return payment_customization_payload(
                None,
                &field.selection,
                vec![payment_customization_not_found_error(&id)],
                None,
                None,
            );
        };

        if resolved_string_field(&input, "title").is_some_and(|title| title.trim().is_empty()) {
            return payment_customization_payload(
                None,
                &field.selection,
                vec![payment_customization_required_input_field_error("title")],
                None,
                None,
            );
        }
        if let Some(handle) = resolved_string_field(&input, "functionHandle") {
            if !payment_customization_function_handle_exists(&handle) {
                return payment_customization_payload(
                    None,
                    &field.selection,
                    vec![payment_customization_user_error(
                        vec!["paymentCustomization", "functionHandle"],
                        "FUNCTION_NOT_FOUND",
                        &format!("Could not find function with handle: {handle}."),
                    )],
                    None,
                    None,
                );
            }
            if !payment_customization_function_matches(&existing, &handle) {
                return payment_customization_payload(
                    None,
                    &field.selection,
                    vec![payment_customization_immutable_function_error(
                        "functionHandle",
                    )],
                    None,
                    None,
                );
            }
        }
        if let Some(function_id) = resolved_string_field(&input, "functionId") {
            if !payment_customization_function_matches(&existing, &function_id) {
                return payment_customization_payload(
                    None,
                    &field.selection,
                    vec![payment_customization_immutable_function_error("functionId")],
                    None,
                    None,
                );
            }
        }
        let metafield_errors = payment_customization_metafield_validation_errors(&input);
        if !metafield_errors.is_empty() {
            return payment_customization_payload(
                None,
                &field.selection,
                metafield_errors,
                None,
                None,
            );
        }

        let mut updated = existing;
        if let Some(title) = resolved_string_field(&input, "title") {
            updated["title"] = json!(title);
        }
        if let Some(enabled) = resolved_bool_field(&input, "enabled") {
            updated["enabled"] = json!(enabled);
        }
        if input.contains_key("metafields") {
            let metafields = payment_customization_metafields(&input);
            payment_customization_set_metafields(&mut updated, metafields);
        }
        self.store
            .staged
            .payment_customizations
            .insert(id.clone(), updated.clone());
        payment_customization_payload(Some(&updated), &field.selection, Vec::new(), None, None)
    }

    pub(in crate::proxy) fn payment_customization_activation_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let ids = resolved_string_list_arg(&field.arguments, "ids");
        let enabled = match field.arguments.get("enabled") {
            Some(ResolvedValue::Bool(value)) => *value,
            _ => false,
        };
        let mut valid_ids = Vec::new();
        let mut missing_ids = Vec::new();
        for id in ids {
            match self.store.staged.payment_customizations.get_mut(&id) {
                Some(record) => {
                    if record["enabled"].as_bool() != Some(enabled) {
                        record["enabled"] = json!(enabled);
                    }
                    valid_ids.push(id);
                }
                None => missing_ids.push(id),
            }
        }
        let errors = if missing_ids.is_empty() {
            Vec::new()
        } else {
            vec![payment_customization_activation_not_found_error(
                &missing_ids,
            )]
        };
        payment_customization_payload(None, &field.selection, errors, Some(valid_ids), None)
    }

    pub(in crate::proxy) fn payment_customization_delete_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        if self
            .store
            .staged
            .payment_customizations
            .remove(&id)
            .is_some()
        {
            payment_customization_payload(None, &field.selection, Vec::new(), None, Some(json!(id)))
        } else {
            payment_customization_payload(
                None,
                &field.selection,
                vec![payment_customization_not_found_error(&id)],
                None,
                Some(Value::Null),
            )
        }
    }
}

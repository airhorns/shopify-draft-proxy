use super::*;

use base64::Engine as _;

pub(in crate::proxy) fn catalog_user_error(field: Vec<&str>, message: &str, code: &str) -> Value {
    json!({
        "__typename": "CatalogUserError",
        "field": field,
        "message": message,
        "code": code
    })
}

pub(in crate::proxy) fn catalog_payload_error(
    field: Vec<&str>,
    message: &str,
    code: &str,
) -> Value {
    catalog_payload_error_with_root("catalog", field, message, code)
}

pub(in crate::proxy) fn catalog_payload_error_with_root(
    root_key: &str,
    field: Vec<&str>,
    message: &str,
    code: &str,
) -> Value {
    json!({
        root_key: null,
        "userErrors": [catalog_user_error(field, message, code)]
    })
}

pub(in crate::proxy) fn catalog_markets_connection(
    market_ids: &[String],
    market_names: &BTreeMap<String, String>,
) -> Value {
    // Shopify's MarketCatalog.markets connection lists markets in reverse
    // attachment order (most recently associated first), which is the join
    // table's default id-descending ordering. `market_ids` is stored in
    // attachment order, so iterate it in reverse to match.
    json!({
        "nodes": market_ids
            .iter()
            .rev()
            .map(|id| match market_names.get(id) {
                Some(name) => json!({"id": id, "name": name}),
                None => json!({"id": id}),
            })
            .collect::<Vec<_>>()
    })
}

pub(in crate::proxy) fn catalog_record(
    id: &str,
    title: &str,
    status: &str,
    market_ids: &[String],
    market_names: &BTreeMap<String, String>,
) -> Value {
    json!({
        "__typename": "MarketCatalog",
        "id": id,
        "title": title,
        "status": status,
        "marketIds": market_ids,
        "markets": catalog_markets_connection(market_ids, market_names),
        "operations": [],
        "priceList": null,
        "publication": null
    })
}

pub(in crate::proxy) fn catalog_market_ids(catalog: &Value) -> Vec<String> {
    catalog["marketIds"]
        .as_array()
        .map(|ids| {
            ids.iter()
                .filter_map(|id| id.as_str().map(ToString::to_string))
                .collect()
        })
        .unwrap_or_default()
}

pub(in crate::proxy) fn set_catalog_market_ids(
    catalog: &mut Value,
    market_ids: &[String],
    market_names: &BTreeMap<String, String>,
) {
    if let Some(object) = catalog.as_object_mut() {
        object.insert("marketIds".to_string(), json!(market_ids));
        object.insert(
            "markets".to_string(),
            catalog_markets_connection(market_ids, market_names),
        );
    }
}

pub(in crate::proxy) fn web_presence_market_ids(web_presence: &Value) -> Vec<String> {
    web_presence["marketIds"]
        .as_array()
        .map(|ids| {
            ids.iter()
                .filter_map(|id| id.as_str().map(ToString::to_string))
                .collect()
        })
        .or_else(|| {
            web_presence["markets"]["nodes"].as_array().map(|nodes| {
                nodes
                    .iter()
                    .filter_map(|node| node["id"].as_str().map(ToString::to_string))
                    .collect()
            })
        })
        .unwrap_or_default()
}

pub(in crate::proxy) fn set_web_presence_market_ids(
    web_presence: &mut Value,
    market_ids: &[String],
) {
    if let Some(object) = web_presence.as_object_mut() {
        object.insert("marketIds".to_string(), json!(market_ids));
        object.insert(
            "markets".to_string(),
            json!({
                "nodes": market_ids.iter().map(|id| json!({"id": id})).collect::<Vec<_>>()
            }),
        );
    }
}

pub(in crate::proxy) fn catalog_relation_id(
    catalog: &Value,
    id_key: &str,
    object_key: &str,
) -> Option<String> {
    catalog[id_key]
        .as_str()
        .or_else(|| catalog[object_key]["id"].as_str())
        .map(ToString::to_string)
}

pub(in crate::proxy) fn set_catalog_price_list_relation(
    catalog: &mut Value,
    price_list_id: Option<&str>,
) {
    set_relation(catalog, "priceListId", "priceList", price_list_id);
}

pub(in crate::proxy) fn set_catalog_publication_relation(
    catalog: &mut Value,
    publication_id: Option<&str>,
) {
    set_relation(catalog, "publicationId", "publication", publication_id);
}

pub(in crate::proxy) fn set_price_list_catalog_relation(
    price_list: &mut Value,
    catalog_id: Option<&str>,
) {
    set_relation(price_list, "catalogId", "catalog", catalog_id);
}

pub(in crate::proxy) fn missing_customization_message(ids: &[String]) -> String {
    let suffixes = ids
        .iter()
        .map(|id| resource_id_path_tail(id).to_string())
        .collect::<Vec<_>>();
    format!(
        "The following customization IDs were not found: {}",
        suffixes.join(", ")
    )
}

pub(in crate::proxy) const PRICE_LIST_INVALID_ADJUSTMENT_MESSAGE: &str = "The adjustment value must be a positive value and not be greater than 100% for PERCENTAGE_DECREASE and not be greater than 1000% for PERCENTAGE_INCREASE.";

pub(in crate::proxy) fn price_list_user_error(
    field: Vec<&str>,
    message: &str,
    code: &str,
) -> Value {
    json!({
        "__typename": "PriceListUserError",
        "field": field,
        "message": message,
        "code": code
    })
}

pub(in crate::proxy) fn price_list_payload_error(
    root_key: &str,
    field: Vec<&str>,
    message: &str,
    code: &str,
) -> Value {
    json!({
        root_key: null,
        "userErrors": [price_list_user_error(field, message, code)]
    })
}

pub(in crate::proxy) fn price_list_adjustment_value_json(
    adjustment: &BTreeMap<String, ResolvedValue>,
) -> Value {
    match adjustment.get("value") {
        Some(ResolvedValue::Int(value)) => json!(value),
        Some(ResolvedValue::Float(value)) if value.fract() == 0.0 => json!(*value as i64),
        Some(ResolvedValue::Float(value)) => json!(value),
        _ => json!(0),
    }
}

pub(in crate::proxy) fn price_list_record(
    id: &str,
    name: &str,
    currency: &str,
    adjustment_type: &str,
    adjustment_value: Value,
    catalog_id: Option<&str>,
) -> Value {
    let catalog = catalog_id
        .map(|id| json!({"id": id}))
        .unwrap_or(Value::Null);
    json!({
        "__typename": "PriceList",
        "id": id,
        "name": name,
        "currency": currency,
        "parent": {"adjustment": {"type": adjustment_type, "value": adjustment_value}},
        "catalogId": catalog_id,
        "catalog": catalog,
        "fixedPricesCount": 0,
        "fixedPriceRows": [],
        "prices": connection_json_with_empty_edges(Vec::new())
    })
}

pub(in crate::proxy) fn fixed_price_by_product_error(
    field: Value,
    message: &str,
    code: &str,
) -> Value {
    json!({
        "__typename": "PriceListFixedPricesByProductBulkUpdateUserError",
        "field": field,
        "message": message,
        "code": code
    })
}

pub(in crate::proxy) fn price_list_price_error(field: Value, message: &str, code: &str) -> Value {
    json!({
        "__typename": "PriceListPriceUserError",
        "field": field,
        "message": message,
        "code": code
    })
}

// ----------------------------------------------------------------------------
// Fixed-price edge model (ported from Gleam markets/serializers.gleam). Price
// lists carry their fixed prices under `prices.edges[].node`; the helpers below
// read, build, and rewrite that connection so the handlers are store-backed
// rather than fabricating seeded records.
// ----------------------------------------------------------------------------

pub(in crate::proxy) fn price_edges(price_list: &Value) -> Vec<Value> {
    price_list["prices"]["edges"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

pub(in crate::proxy) fn fixed_price_edge_variant_id(edge: &Value) -> Option<String> {
    edge["node"]["variant"]["id"].as_str().map(str::to_string)
}

pub(in crate::proxy) fn fixed_price_variant_ids(price_list: &Value) -> Vec<String> {
    price_edges(price_list)
        .iter()
        .filter_map(fixed_price_edge_variant_id)
        .collect()
}

pub(in crate::proxy) fn fixed_price_variant_ids_in_request_order(
    price_list: &Value,
    variant_ids: &[String],
) -> Vec<String> {
    let fixed = fixed_price_variant_ids(price_list);
    variant_ids
        .iter()
        .filter(|id| fixed.contains(id))
        .cloned()
        .collect()
}

pub(in crate::proxy) fn fixed_price_nodes_for_variant_ids(
    price_list: &Value,
    variant_ids: &[String],
) -> Vec<Value> {
    price_edges(price_list)
        .iter()
        .filter_map(|edge| {
            let variant_id = fixed_price_edge_variant_id(edge)?;
            variant_ids
                .contains(&variant_id)
                .then(|| edge.get("node").cloned())
                .flatten()
        })
        .collect()
}

pub(in crate::proxy) fn price_list_currency(price_list: &Value) -> String {
    price_list["currency"].as_str().unwrap_or("USD").to_string()
}

pub(in crate::proxy) fn mutation_variant_ids(inputs: &[ResolvedValue]) -> Vec<String> {
    inputs
        .iter()
        .filter_map(|input| resolved_nonempty_string(input, "variantId"))
        .collect()
}

/// `read_arg_string_nonempty` — an object field that is a non-empty string.
pub(in crate::proxy) fn resolved_nonempty_string(
    value: &ResolvedValue,
    name: &str,
) -> Option<String> {
    resolved_object_string(value, name).filter(|value| !value.is_empty())
}

/// The object-valued items of a list argument (mirrors `read_arg_object_array`).
pub(in crate::proxy) fn resolved_object_list(
    arguments: &BTreeMap<String, ResolvedValue>,
    name: &str,
) -> Vec<ResolvedValue> {
    resolved_list_arg(arguments, name)
        .into_iter()
        .filter(|value| matches!(value, ResolvedValue::Object(_)))
        .collect()
}

fn fixed_price_money_object_present(input: &ResolvedValue, field: &str) -> bool {
    matches!(
        input,
        ResolvedValue::Object(fields)
            if matches!(fields.get(field), Some(ResolvedValue::Object(_)))
    )
}

/// `money_payload` / `optional_money_payload`: a present money object becomes
/// `{amount, currencyCode}` (amount normalized, currency defaulting to the price
/// list currency); an absent object becomes null.
pub(in crate::proxy) fn fixed_price_money_payload(
    input: &ResolvedValue,
    field: &str,
    currency: &str,
) -> Value {
    if !fixed_price_money_object_present(input, field) {
        return Value::Null;
    }
    let amount = fixed_price_input_amount(input, field).unwrap_or_else(|| "0".to_string());
    let currency_code = fixed_price_input_currency(input, field)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| currency.to_string());
    json!({"amount": amount, "currencyCode": currency_code})
}

pub(in crate::proxy) fn fixed_price_product_payload(product: &ProductRecord) -> Value {
    json!({
        "__typename": "Product",
        "id": product.id,
        "title": product.title,
        "handle": product.handle,
        "status": product.status
    })
}

pub(in crate::proxy) fn fixed_price_product_payloads(store: &Store, ids: &[String]) -> Vec<Value> {
    ids.iter()
        .filter_map(|id| store.product_by_id(id).map(fixed_price_product_payload))
        .collect()
}

fn fixed_price_variant_payload(variant: &Value, product: &ProductRecord) -> Value {
    let sku = variant
        .get("sku")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty());
    json!({
        "__typename": "ProductVariant",
        "id": variant.get("id").and_then(Value::as_str).unwrap_or_default(),
        "title": variant.get("title").and_then(Value::as_str).unwrap_or_default(),
        "sku": sku,
        "product": fixed_price_product_payload(product)
    })
}

fn fixed_price_edge_for_variant(
    variant: &Value,
    product: &ProductRecord,
    input: &ResolvedValue,
    currency: &str,
) -> Value {
    json!({
        "cursor": variant.get("id").and_then(Value::as_str).unwrap_or_default(),
        "node": {
            "__typename": "PriceListPrice",
            "price": fixed_price_money_payload(input, "price", currency),
            "compareAtPrice": fixed_price_money_payload(input, "compareAtPrice", currency),
            "originType": "FIXED",
            "variant": fixed_price_variant_payload(variant, product),
            "quantityPriceBreaks": price_connection_from_edges(&[])
        }
    })
}

pub(in crate::proxy) fn price_connection_from_edges(edges: &[Value]) -> Value {
    let cursors = edges
        .iter()
        .filter_map(|edge| edge["cursor"].as_str())
        .collect::<Vec<_>>();
    let nodes = edges
        .iter()
        .filter_map(|edge| edge.get("node").cloned())
        .collect::<Vec<_>>();
    json!({
        "edges": edges,
        "nodes": nodes,
        "pageInfo": {
            "hasNextPage": false,
            "hasPreviousPage": false,
            "startCursor": cursors.first().copied(),
            "endCursor": cursors.last().copied()
        }
    })
}

fn rebuild_price_list_prices(price_list: &mut Value, edges: Vec<Value>) {
    let fixed_count = edges
        .iter()
        .filter(|edge| edge["node"]["originType"].as_str() == Some("FIXED"))
        .count();
    if let Some(object) = price_list.as_object_mut() {
        object.insert("fixedPricesCount".to_string(), json!(fixed_count));
        object.insert("prices".to_string(), price_connection_from_edges(&edges));
    }
}

/// Dedupe inputs by `variantId`, keeping the last occurrence (mirrors Gleam
/// `last_fixed_price_inputs_by_variant`).
fn last_fixed_price_inputs_by_variant(inputs: &[ResolvedValue]) -> Vec<ResolvedValue> {
    let mut accumulator: Vec<ResolvedValue> = Vec::new();
    for input in inputs {
        match resolved_nonempty_string(input, "variantId") {
            Some(variant_id) => {
                accumulator.retain(|existing| {
                    resolved_nonempty_string(existing, "variantId").as_deref()
                        != Some(variant_id.as_str())
                });
                accumulator.push(input.clone());
            }
            None => accumulator.push(input.clone()),
        }
    }
    accumulator
}

pub(in crate::proxy) fn upsert_fixed_price_nodes(
    price_list: &mut Value,
    store: &Store,
    inputs: &[ResolvedValue],
) {
    let inputs = last_fixed_price_inputs_by_variant(inputs);
    let input_variant_ids = mutation_variant_ids(&inputs);
    let mut retained = price_edges(price_list)
        .into_iter()
        .filter(|edge| match fixed_price_edge_variant_id(edge) {
            Some(id) => !input_variant_ids.contains(&id),
            None => true,
        })
        .collect::<Vec<_>>();
    let currency = price_list_currency(price_list);
    let mut new_edges = Vec::new();
    for input in &inputs {
        let Some(variant_id) = resolved_nonempty_string(input, "variantId") else {
            continue;
        };
        let Some((variant, product)) = store.fixed_price_variant_lookup(&variant_id) else {
            continue;
        };
        new_edges.push(fixed_price_edge_for_variant(
            &variant, &product, input, &currency,
        ));
    }
    new_edges.append(&mut retained);
    rebuild_price_list_prices(price_list, new_edges);
}

pub(in crate::proxy) fn delete_fixed_price_nodes(price_list: &mut Value, variant_ids: &[String]) {
    let retained = price_edges(price_list)
        .into_iter()
        .filter(|edge| match fixed_price_edge_variant_id(edge) {
            Some(id) => !variant_ids.contains(&id),
            None => true,
        })
        .collect::<Vec<_>>();
    rebuild_price_list_prices(price_list, retained);
}

// ----------------------------------------------------------------------------
// Fixed-price validation (variant-level), ported from Gleam.
// ----------------------------------------------------------------------------

pub(in crate::proxy) fn price_list_fixed_price_target_errors(
    price_list_id: &Option<String>,
    price_list: &Option<Value>,
) -> Vec<Value> {
    match (price_list_id, price_list) {
        (Some(_), Some(_)) => Vec::new(),
        _ => vec![price_list_price_error(
            json!(["priceListId"]),
            "Price list does not exist.",
            "PRICE_LIST_NOT_FOUND",
        )],
    }
}

pub(in crate::proxy) fn fixed_price_variant_errors(
    store: &Store,
    inputs: &[ResolvedValue],
    field_name: &str,
) -> Vec<Value> {
    let mut errors = Vec::new();
    for (index, input) in inputs.iter().enumerate() {
        let variant_id = resolved_nonempty_string(input, "variantId").unwrap_or_default();
        if store.fixed_price_variant_lookup(&variant_id).is_none() {
            errors.push(price_list_price_error(
                json!([field_name, index.to_string(), "variantId"]),
                "Product variant ID does not exist.",
                "VARIANT_NOT_FOUND",
            ));
        }
    }
    errors
}

pub(in crate::proxy) fn fixed_price_currency_errors(
    price_list: &Value,
    inputs: &[ResolvedValue],
    field_name: &str,
) -> Vec<Value> {
    let expected = price_list_currency(price_list);
    let mut errors = Vec::new();
    for (index, input) in inputs.iter().enumerate() {
        if let Some(actual) =
            fixed_price_input_currency(input, "price").filter(|value| !value.is_empty())
        {
            if actual != expected {
                errors.push(price_list_price_error(
                    json!([field_name, index.to_string(), "price", "currencyCode"]),
                    "The specified currency does not match the price list's currency.",
                    "PRICE_LIST_CURRENCY_MISMATCH",
                ));
            }
        }
    }
    errors
}

pub(in crate::proxy) fn fixed_price_input_errors(
    store: &Store,
    price_list: &Value,
    inputs: &[ResolvedValue],
    field_name: &str,
) -> Vec<Value> {
    let mut errors = fixed_price_variant_errors(store, inputs, field_name);
    errors.extend(fixed_price_currency_errors(price_list, inputs, field_name));
    errors
}

pub(in crate::proxy) fn fixed_price_delete_variant_errors(
    store: &Store,
    variant_ids: &[String],
    field_name: &str,
) -> Vec<Value> {
    let mut errors = Vec::new();
    for (index, variant_id) in variant_ids.iter().enumerate() {
        if store.fixed_price_variant_lookup(variant_id).is_none() {
            errors.push(price_list_price_error(
                json!([field_name, index.to_string()]),
                "Product variant ID does not exist.",
                "VARIANT_NOT_FOUND",
            ));
        }
    }
    errors
}

pub(in crate::proxy) fn fixed_price_delete_not_fixed_errors(
    store: &Store,
    price_list: &Value,
    variant_ids: &[String],
    field_name: &str,
) -> Vec<Value> {
    let fixed = fixed_price_variant_ids(price_list);
    let mut errors = Vec::new();
    for (index, variant_id) in variant_ids.iter().enumerate() {
        if store.fixed_price_variant_lookup(variant_id).is_some() && !fixed.contains(variant_id) {
            errors.push(price_list_price_error(
                json!([field_name, index.to_string()]),
                "Only fixed prices can be deleted.",
                "PRICE_NOT_FIXED",
            ));
        }
    }
    errors
}

/// `read_price_list_id` (serializers.gleam): the mutation's price list id comes
/// from the `priceListId` argument, falling back to `id`, then `input.priceListId`.
pub(in crate::proxy) fn read_price_list_id(
    arguments: &BTreeMap<String, ResolvedValue>,
) -> Option<String> {
    if let Some(id) =
        resolved_string_arg(arguments, "priceListId").filter(|value| !value.is_empty())
    {
        return Some(id);
    }
    if let Some(id) = resolved_string_arg(arguments, "id").filter(|value| !value.is_empty()) {
        return Some(id);
    }
    resolved_object_field(arguments, "input")
        .and_then(|input| resolved_string_field(&input, "priceListId"))
        .filter(|value| !value.is_empty())
}

/// `read_fixed_price_update_inputs` (mutations.gleam): the update mutation reads
/// `prices` if present, otherwise `pricesToAdd`, returning the chosen field name
/// so error paths point at the argument the caller supplied.
pub(in crate::proxy) fn read_fixed_price_update_inputs(
    arguments: &BTreeMap<String, ResolvedValue>,
) -> (Vec<ResolvedValue>, &'static str) {
    let prices = resolved_object_list(arguments, "prices");
    if prices.is_empty() {
        (
            resolved_object_list(arguments, "pricesToAdd"),
            "pricesToAdd",
        )
    } else {
        (prices, "prices")
    }
}

/// The by-product preflight hydrate variables (queries.gleam
/// `product_fixed_prices_preflight_variables`): a `priceListId`/`priceQuery`
/// pulled verbatim from the operation variables plus the de-duplicated product
/// ids referenced by `pricesToAdd` and `pricesToDeleteByProductIds`.
pub(in crate::proxy) fn product_fixed_prices_preflight_variables(
    variables: &BTreeMap<String, ResolvedValue>,
) -> Value {
    let string_variable = |name: &str| match variables.get(name) {
        Some(ResolvedValue::String(value)) => json!(value),
        _ => Value::Null,
    };
    let mut product_ids: Vec<String> = Vec::new();
    if let Some(ResolvedValue::List(items)) = variables.get("pricesToAdd") {
        for item in items {
            if let Some(id) = resolved_object_string(item, "productId") {
                if !product_ids.contains(&id) {
                    product_ids.push(id);
                }
            }
        }
    }
    if let Some(ResolvedValue::List(items)) = variables.get("pricesToDeleteByProductIds") {
        for item in items {
            if let ResolvedValue::String(id) = item {
                if !product_ids.contains(id) {
                    product_ids.push(id.clone());
                }
            }
        }
    }
    json!({
        "priceListId": string_variable("priceListId"),
        "priceQuery": string_variable("priceQuery"),
        "productIds": product_ids,
    })
}

/// `product_level_fixed_price_errors` (serializers.gleam): the ordered validation
/// suite for `priceListFixedPricesByProductUpdate`. Mirrors the Gleam
/// `combine_error_lists` ordering exactly: no-op, missing add products, missing
/// delete products, currency mismatches, duplicate add ids, duplicate delete
/// ids, mutual-exclusion conflicts, then the fixed-price limit.
pub(in crate::proxy) fn product_level_fixed_price_errors(
    store: &Store,
    price_list: &Option<Value>,
    price_inputs: &[ResolvedValue],
    delete_product_ids: &[String],
) -> Vec<Value> {
    let mut errors = Vec::new();
    if price_inputs.is_empty() && delete_product_ids.is_empty() {
        errors.push(fixed_price_by_product_error(
            Value::Null,
            "No update operations are specified. `pricesToAdd` and `pricesToDeleteByProductIds` are empty.",
            "NO_UPDATE_OPERATIONS_SPECIFIED",
        ));
    }
    for (index, input) in price_inputs.iter().enumerate() {
        let product_id = resolved_nonempty_string(input, "productId").unwrap_or_default();
        if store.product_by_id(&product_id).is_none() {
            errors.push(fixed_price_by_product_error(
                json!(["pricesToAdd", index.to_string(), "productId"]),
                &format!("Product {product_id} in `pricesToAdd` does not exist."),
                "PRODUCT_DOES_NOT_EXIST",
            ));
        }
    }
    for (index, product_id) in delete_product_ids.iter().enumerate() {
        if store.product_by_id(product_id).is_none() {
            errors.push(fixed_price_by_product_error(
                json!(["pricesToDeleteByProductIds", index.to_string()]),
                &format!("Product {product_id} in `pricesToDeleteByProductIds` does not exist."),
                "PRODUCT_DOES_NOT_EXIST",
            ));
        }
    }
    if let Some(existing) = price_list {
        let currency = price_list_currency(existing);
        for (index, input) in price_inputs.iter().enumerate() {
            for money_field in ["price", "compareAtPrice"] {
                if let Some(actual) =
                    fixed_price_input_currency(input, money_field).filter(|value| !value.is_empty())
                {
                    if actual != currency {
                        let product_id =
                            resolved_nonempty_string(input, "productId").unwrap_or_default();
                        errors.push(fixed_price_by_product_error(
                            json!(["pricesToAdd", index.to_string(), money_field, "currencyCode"]),
                            &format!(
                                "The currency specified in `pricesToAdd` for product ID {product_id} does not match the price list's currency of {currency}."
                            ),
                            "PRICES_TO_ADD_CURRENCY_MISMATCH",
                        ));
                    }
                }
            }
        }
    }
    let mut seen_add: Vec<String> = Vec::new();
    for input in price_inputs {
        if let Some(product_id) = resolved_nonempty_string(input, "productId") {
            if seen_add.contains(&product_id) {
                errors.push(fixed_price_by_product_error(
                    json!(["pricesToAdd"]),
                    "Duplicate ID exists in `pricesToAdd`.",
                    "DUPLICATE_ID_IN_INPUT",
                ));
            } else {
                seen_add.push(product_id);
            }
        }
    }
    let mut seen_delete: Vec<String> = Vec::new();
    for product_id in delete_product_ids {
        if seen_delete.contains(product_id) {
            errors.push(fixed_price_by_product_error(
                json!(["pricesToDeleteByProductIds"]),
                "Duplicate ID exists in `pricesToDeleteByProductIds`.",
                "DUPLICATE_ID_IN_INPUT",
            ));
        } else {
            seen_delete.push(product_id.clone());
        }
    }
    for input in price_inputs {
        if let Some(product_id) = resolved_nonempty_string(input, "productId") {
            if delete_product_ids.contains(&product_id) {
                errors.push(fixed_price_by_product_error(
                    Value::Null,
                    "IDs specified in `pricesToAdd` and `pricesToDeleteByProductIds` must be mutually exclusive.",
                    "ID_MUST_BE_MUTUALLY_EXCLUSIVE",
                ));
            }
        }
    }
    if let Some(existing) = price_list {
        if resulting_fixed_price_variant_ids(store, existing, price_inputs, delete_product_ids)
            .len()
            >= 10_000
        {
            errors.push(fixed_price_by_product_error(
                json!(["pricesToAdd"]),
                "The maximum number of fixed prices allowed for the price list has been exceeded.",
                "PRICE_LIMIT_EXCEEDED",
            ));
        }
    }
    errors
}

/// `resulting_fixed_price_variant_ids` (serializers.gleam): the variant ids that
/// would remain fixed after applying a by-product update — existing FIXED edges
/// minus the deleted products' variants, plus the added products' variants.
fn resulting_fixed_price_variant_ids(
    store: &Store,
    price_list: &Value,
    price_inputs: &[ResolvedValue],
    delete_product_ids: &[String],
) -> Vec<String> {
    let delete_variant_ids: Vec<String> = delete_product_ids
        .iter()
        .flat_map(|product_id| store.fixed_price_variants_for_product(product_id))
        .filter_map(|variant| variant["id"].as_str().map(str::to_string))
        .collect();
    let mut retained: Vec<String> = price_edges(price_list)
        .iter()
        .filter(|edge| edge["node"]["originType"].as_str() == Some("FIXED"))
        .filter_map(fixed_price_edge_variant_id)
        .filter(|variant_id| !delete_variant_ids.contains(variant_id))
        .collect();
    let add_variant_ids: Vec<String> = price_inputs
        .iter()
        .filter_map(|input| resolved_nonempty_string(input, "productId"))
        .flat_map(|product_id| store.fixed_price_variants_for_product(&product_id))
        .filter_map(|variant| variant["id"].as_str().map(str::to_string))
        .collect();
    extend_unique_strings(&mut retained, &add_variant_ids);
    retained
}

pub(in crate::proxy) fn fixed_price_input_currency(
    input: &ResolvedValue,
    money_field: &str,
) -> Option<String> {
    let ResolvedValue::Object(fields) = input else {
        return None;
    };
    let Some(ResolvedValue::Object(money)) = fields.get(money_field) else {
        return None;
    };
    resolved_string_field(money, "currencyCode")
}

pub(in crate::proxy) fn fixed_price_input_amount(
    input: &ResolvedValue,
    money_field: &str,
) -> Option<String> {
    let ResolvedValue::Object(fields) = input else {
        return None;
    };
    let Some(ResolvedValue::Object(money)) = fields.get(money_field) else {
        return None;
    };
    resolved_string_field(money, "amount").map(|amount| normalized_money_amount(&amount))
}

pub(in crate::proxy) fn normalized_money_amount(amount: &str) -> String {
    if !amount.contains('.') {
        return amount.to_string();
    }
    let mut normalized = amount.to_string();
    while normalized.ends_with('0') {
        normalized.pop();
    }
    if normalized.ends_with('.') {
        normalized.push('0');
    }
    normalized
}

pub(in crate::proxy) fn market_status_enabled_mismatch(
    input: &BTreeMap<String, ResolvedValue>,
) -> bool {
    matches!(
        (
            resolved_string_field(input, "status").as_deref(),
            resolved_bool_field(input, "enabled")
        ),
        (Some("DRAFT"), Some(true)) | (Some("ACTIVE"), Some(false))
    )
}

pub(in crate::proxy) fn market_has_location_price_inclusion_conflict(
    input: &BTreeMap<String, ResolvedValue>,
) -> bool {
    let Some(conditions) = resolved_object_field(input, "conditions") else {
        return false;
    };
    if resolved_object_field(&conditions, "locationsCondition").is_none() {
        return false;
    }
    let Some(price_inclusions) = resolved_object_field(input, "priceInclusions") else {
        return false;
    };
    matches!(
        (
            resolved_string_field(&price_inclusions, "taxPricingStrategy").as_deref(),
            resolved_string_field(&price_inclusions, "dutiesPricingStrategy").as_deref()
        ),
        (Some("INCLUDES_TAXES_IN_PRICE"), _) | (_, Some("INCLUDE_DUTIES_IN_PRICE"))
    )
}

pub(in crate::proxy) fn market_currency_settings(
    input: &BTreeMap<String, ResolvedValue>,
) -> Option<BTreeMap<String, ResolvedValue>> {
    resolved_object_field(input, "currencySettings")
}

pub(in crate::proxy) fn market_region_country_codes(
    input: &BTreeMap<String, ResolvedValue>,
) -> Vec<String> {
    let mut codes = region_country_codes_from_value(input.get("regions"));
    if let Some(conditions) = resolved_object_field(input, "conditions") {
        if let Some(regions_condition) = resolved_object_field(&conditions, "regionsCondition") {
            codes.extend(region_country_codes_from_value(
                regions_condition.get("regions"),
            ));
        }
    }
    codes
}

pub(in crate::proxy) fn region_country_codes_from_value(
    value: Option<&ResolvedValue>,
) -> Vec<String> {
    match value {
        Some(ResolvedValue::List(regions)) => regions
            .iter()
            .filter_map(|region| resolved_object_string(region, "countryCode"))
            .collect(),
        _ => Vec::new(),
    }
}

pub(in crate::proxy) fn market_record_from_input(
    id: &str,
    input: &BTreeMap<String, ResolvedValue>,
    name: &str,
    handle: &str,
    region_codes: &[String],
) -> Value {
    // Defaults match Gleam market_data (serializers.gleam:226): status falls
    // back to ACTIVE only when enabled is explicitly true, otherwise DRAFT;
    // enabled falls back to status==ACTIVE; type is REGION when any region
    // input is present, else NONE.
    let status = resolved_string_field(input, "status").unwrap_or_else(|| {
        if resolved_bool_field(input, "enabled") == Some(true) {
            "ACTIVE".to_string()
        } else {
            "DRAFT".to_string()
        }
    });
    let enabled = resolved_bool_field(input, "enabled").unwrap_or(status == "ACTIVE");
    let market_type = if region_codes.is_empty() {
        "NONE"
    } else {
        "REGION"
    };
    let region_nodes = region_codes
        .iter()
        .map(|code| json!({"code": code}))
        .collect::<Vec<_>>();
    json!({
        "id": id,
        "name": name,
        "handle": handle,
        "status": status,
        "enabled": enabled,
        "type": market_type,
        "priceInclusions": market_price_inclusions(input),
        "currencySettings": market_currency_settings_json(input),
        "regionCodes": region_codes,
        "conditions": {
            "regionsCondition": {
                "regions": {
                    "nodes": region_nodes
                }
            }
        },
        "catalogs": {"nodes": []},
        "webPresences": {"nodes": []}
    })
}

pub(in crate::proxy) fn market_price_inclusions(input: &BTreeMap<String, ResolvedValue>) -> Value {
    let Some(price_inclusions) = resolved_object_field(input, "priceInclusions") else {
        return Value::Null;
    };
    json!({
        "inclusiveDutiesPricingStrategy": resolved_string_field(&price_inclusions, "dutiesPricingStrategy").unwrap_or_else(|| "NOT_INCLUDED".to_string()),
        "inclusiveTaxPricingStrategy": resolved_string_field(&price_inclusions, "taxPricingStrategy").unwrap_or_else(|| "ADD_TAXES_AT_CHECKOUT".to_string())
    })
}

pub(in crate::proxy) fn market_currency_settings_json(
    input: &BTreeMap<String, ResolvedValue>,
) -> Value {
    let Some(currency_settings) = resolved_object_field(input, "currencySettings") else {
        return Value::Null;
    };
    let currency_code = resolved_string_field(&currency_settings, "baseCurrency")
        .unwrap_or_else(|| "USD".to_string());
    json!({
        "baseCurrency": {
            "currencyCode": currency_code,
            "currencyName": market_currency_name(&currency_code)
        },
        "localCurrencies": resolved_bool_field(&currency_settings, "localCurrencies").unwrap_or(false),
        "roundingEnabled": resolved_bool_field(&currency_settings, "roundingEnabled").unwrap_or(false)
    })
}

pub(in crate::proxy) fn market_currency_name(code: &str) -> &'static str {
    match code {
        "AED" => "United Arab Emirates Dirham",
        "AFN" => "Afghan Afghani",
        "ALL" => "Albanian Lek",
        "AMD" => "Armenian Dram",
        "ANG" => "Netherlands Antillean Guilder",
        "AOA" => "Angolan Kwanza",
        "ARS" => "Argentine Peso",
        "AUD" => "Australian Dollar",
        "AWG" => "Aruban Florin",
        "AZN" => "Azerbaijani Manat",
        "BAM" => "Bosnia-Herzegovina Convertible Mark",
        "BBD" => "Barbadian Dollar",
        "BDT" => "Bangladeshi Taka",
        "BGN" => "Bulgarian Lev",
        "BHD" => "Bahraini Dinar",
        "BIF" => "Burundian Franc",
        "BMD" => "Bermudian Dollar",
        "BND" => "Brunei Dollar",
        "BOB" => "Bolivian Boliviano",
        "BRL" => "Brazilian Real",
        "BSD" => "Bahamian Dollar",
        "BTN" => "Bhutanese Ngultrum",
        "BWP" => "Botswanan Pula",
        "BYN" => "Belarusian Ruble",
        "BZD" => "Belize Dollar",
        "CAD" => "Canadian Dollar",
        "CDF" => "Congolese Franc",
        "CHF" => "Swiss Franc",
        "CLP" => "Chilean Peso",
        "CNY" => "Chinese Yuan",
        "COP" => "Colombian Peso",
        "CRC" => "Costa Rican Colon",
        "CVE" => "Cape Verdean Escudo",
        "CZK" => "Czech Koruna",
        "DJF" => "Djiboutian Franc",
        "DKK" => "Danish Krone",
        "DOP" => "Dominican Peso",
        "DZD" => "Algerian Dinar",
        "EGP" => "Egyptian Pound",
        "ERN" => "Eritrean Nakfa",
        "ETB" => "Ethiopian Birr",
        "EUR" => "Euro",
        "FJD" => "Fijian Dollar",
        "FKP" => "Falkland Islands Pound",
        "GBP" => "British Pound",
        "GEL" => "Georgian Lari",
        "GHS" => "Ghanaian Cedi",
        "GIP" => "Gibraltar Pound",
        "GMD" => "Gambian Dalasi",
        "GNF" => "Guinean Franc",
        "GTQ" => "Guatemalan Quetzal",
        "GYD" => "Guyanese Dollar",
        "HKD" => "Hong Kong Dollar",
        "HNL" => "Honduran Lempira",
        "HRK" => "Croatian Kuna",
        "HTG" => "Haitian Gourde",
        "HUF" => "Hungarian Forint",
        "IDR" => "Indonesian Rupiah",
        "ILS" => "Israeli New Shekel",
        "INR" => "Indian Rupee",
        "IQD" => "Iraqi Dinar",
        "ISK" => "Icelandic Krona",
        "JMD" => "Jamaican Dollar",
        "JOD" => "Jordanian Dinar",
        "JPY" => "Japanese Yen",
        "KES" => "Kenyan Shilling",
        "KGS" => "Kyrgyzstani Som",
        "KHR" => "Cambodian Riel",
        "KID" => "Kiribati Dollar",
        "KMF" => "Comorian Franc",
        "KRW" => "South Korean Won",
        "KWD" => "Kuwaiti Dinar",
        "KYD" => "Cayman Islands Dollar",
        "KZT" => "Kazakhstani Tenge",
        "LAK" => "Lao Kip",
        "LBP" => "Lebanese Pound",
        "LKR" => "Sri Lankan Rupee",
        "LRD" => "Liberian Dollar",
        "LSL" => "Lesotho Loti",
        "LTL" => "Lithuanian Litas",
        "LVL" => "Latvian Lats",
        "LYD" => "Libyan Dinar",
        "MAD" => "Moroccan Dirham",
        "MDL" => "Moldovan Leu",
        "MGA" => "Malagasy Ariary",
        "MKD" => "Macedonian Denar",
        "MMK" => "Myanmar Kyat",
        "MNT" => "Mongolian Tugrik",
        "MOP" => "Macanese Pataca",
        "MRU" => "Mauritanian Ouguiya",
        "MUR" => "Mauritian Rupee",
        "MVR" => "Maldivian Rufiyaa",
        "MWK" => "Malawian Kwacha",
        "MXN" => "Mexican Peso",
        "MYR" => "Malaysian Ringgit",
        "MZN" => "Mozambican Metical",
        "NAD" => "Namibian Dollar",
        "NGN" => "Nigerian Naira",
        "NIO" => "Nicaraguan Cordoba",
        "NOK" => "Norwegian Krone",
        "NPR" => "Nepalese Rupee",
        "NZD" => "New Zealand Dollar",
        "OMR" => "Omani Rial",
        "PAB" => "Panamanian Balboa",
        "PEN" => "Peruvian Sol",
        "PGK" => "Papua New Guinean Kina",
        "PHP" => "Philippine Peso",
        "PKR" => "Pakistani Rupee",
        "PLN" => "Polish Zloty",
        "PYG" => "Paraguayan Guarani",
        "QAR" => "Qatari Riyal",
        "RON" => "Romanian Leu",
        "RSD" => "Serbian Dinar",
        "RUB" => "Russian Ruble",
        "RWF" => "Rwandan Franc",
        "SAR" => "Saudi Riyal",
        "SBD" => "Solomon Islands Dollar",
        "SCR" => "Seychellois Rupee",
        "SDG" => "Sudanese Pound",
        "SEK" => "Swedish Krona",
        "SGD" => "Singapore Dollar",
        "SHP" => "Saint Helena Pound",
        "SLE" => "Sierra Leonean Leone",
        "SLL" => "Sierra Leonean Leone",
        "SOS" => "Somali Shilling",
        "SRD" => "Surinamese Dollar",
        "SSP" => "South Sudanese Pound",
        "STN" => "Sao Tome and Principe Dobra",
        "SYP" => "Syrian Pound",
        "SZL" => "Swazi Lilangeni",
        "THB" => "Thai Baht",
        "TJS" => "Tajikistani Somoni",
        "TMT" => "Turkmenistani Manat",
        "TND" => "Tunisian Dinar",
        "TOP" => "Tongan Pa'anga",
        "TRY" => "Turkish Lira",
        "TTD" => "Trinidad and Tobago Dollar",
        "TWD" => "New Taiwan Dollar",
        "TZS" => "Tanzanian Shilling",
        "UAH" => "Ukrainian Hryvnia",
        "UGX" => "Ugandan Shilling",
        "USD" => "US Dollar",
        "UYU" => "Uruguayan Peso",
        "UZS" => "Uzbekistani Som",
        "VES" => "Venezuelan Bolivar",
        "VND" => "Vietnamese Dong",
        "VUV" => "Vanuatu Vatu",
        "WST" => "Samoan Tala",
        "XCD" => "East Caribbean Dollar",
        "YER" => "Yemeni Rial",
        "ZAR" => "South African Rand",
        "ZMW" => "Zambian Kwacha",
        _ => "Unknown Currency",
    }
}

pub(in crate::proxy) fn market_user_error(field: Vec<&str>, message: &str, code: Value) -> Value {
    json!({
        "__typename": "MarketUserError",
        "field": field,
        "message": message,
        "code": code
    })
}

pub(in crate::proxy) fn default_available_locales() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("af".to_string(), "Afrikaans".to_string()),
        ("ak".to_string(), "Akan".to_string()),
        ("sq".to_string(), "Albanian".to_string()),
        ("am".to_string(), "Amharic".to_string()),
        ("ar".to_string(), "Arabic".to_string()),
        ("hy".to_string(), "Armenian".to_string()),
        ("as".to_string(), "Assamese".to_string()),
        ("az".to_string(), "Azerbaijani".to_string()),
        ("bm".to_string(), "Bambara".to_string()),
        ("bn".to_string(), "Bangla".to_string()),
        ("eu".to_string(), "Basque".to_string()),
        ("be".to_string(), "Belarusian".to_string()),
        ("bs".to_string(), "Bosnian".to_string()),
        ("br".to_string(), "Breton".to_string()),
        ("bg".to_string(), "Bulgarian".to_string()),
        ("my".to_string(), "Burmese".to_string()),
        ("ca".to_string(), "Catalan".to_string()),
        ("ckb".to_string(), "Central Kurdish".to_string()),
        ("ce".to_string(), "Chechen".to_string()),
        ("zh-CN".to_string(), "Chinese (Simplified)".to_string()),
        ("zh-TW".to_string(), "Chinese (Traditional)".to_string()),
        ("kw".to_string(), "Cornish".to_string()),
        ("hr".to_string(), "Croatian".to_string()),
        ("cs".to_string(), "Czech".to_string()),
        ("da".to_string(), "Danish".to_string()),
        ("nl".to_string(), "Dutch".to_string()),
        ("dz".to_string(), "Dzongkha".to_string()),
        ("en".to_string(), "English".to_string()),
        ("eo".to_string(), "Esperanto".to_string()),
        ("et".to_string(), "Estonian".to_string()),
        ("ee".to_string(), "Ewe".to_string()),
        ("fo".to_string(), "Faroese".to_string()),
        ("fil".to_string(), "Filipino".to_string()),
        ("fi".to_string(), "Finnish".to_string()),
        ("fr".to_string(), "French".to_string()),
        ("ff".to_string(), "Fulah".to_string()),
        ("gl".to_string(), "Galician".to_string()),
        ("lg".to_string(), "Ganda".to_string()),
        ("ka".to_string(), "Georgian".to_string()),
        ("de".to_string(), "German".to_string()),
        ("el".to_string(), "Greek".to_string()),
        ("gu".to_string(), "Gujarati".to_string()),
        ("ha".to_string(), "Hausa".to_string()),
        ("he".to_string(), "Hebrew".to_string()),
        ("hi".to_string(), "Hindi".to_string()),
        ("hu".to_string(), "Hungarian".to_string()),
        ("is".to_string(), "Icelandic".to_string()),
        ("ig".to_string(), "Igbo".to_string()),
        ("id".to_string(), "Indonesian".to_string()),
        ("ia".to_string(), "Interlingua".to_string()),
        ("ga".to_string(), "Irish".to_string()),
        ("it".to_string(), "Italian".to_string()),
        ("ja".to_string(), "Japanese".to_string()),
        ("jv".to_string(), "Javanese".to_string()),
        ("kl".to_string(), "Kalaallisut".to_string()),
        ("kn".to_string(), "Kannada".to_string()),
        ("ks".to_string(), "Kashmiri".to_string()),
        ("kk".to_string(), "Kazakh".to_string()),
        ("km".to_string(), "Khmer".to_string()),
        ("ki".to_string(), "Kikuyu".to_string()),
        ("rw".to_string(), "Kinyarwanda".to_string()),
        ("ko".to_string(), "Korean".to_string()),
        ("ku".to_string(), "Kurdish".to_string()),
        ("ky".to_string(), "Kyrgyz".to_string()),
        ("lo".to_string(), "Lao".to_string()),
        ("lv".to_string(), "Latvian".to_string()),
        ("ln".to_string(), "Lingala".to_string()),
        ("lt".to_string(), "Lithuanian".to_string()),
        ("lu".to_string(), "Luba-Katanga".to_string()),
        ("lb".to_string(), "Luxembourgish".to_string()),
        ("mk".to_string(), "Macedonian".to_string()),
        ("mg".to_string(), "Malagasy".to_string()),
        ("ms".to_string(), "Malay".to_string()),
        ("ml".to_string(), "Malayalam".to_string()),
        ("mt".to_string(), "Maltese".to_string()),
        ("gv".to_string(), "Manx".to_string()),
        ("mr".to_string(), "Marathi".to_string()),
        ("mn".to_string(), "Mongolian".to_string()),
        ("mi".to_string(), "M\u{101}ori".to_string()),
        ("ne".to_string(), "Nepali".to_string()),
        ("nd".to_string(), "North Ndebele".to_string()),
        ("se".to_string(), "Northern Sami".to_string()),
        ("no".to_string(), "Norwegian".to_string()),
        ("nb".to_string(), "Norwegian (Bokm\u{e5}l)".to_string()),
        ("nn".to_string(), "Norwegian Nynorsk".to_string()),
        ("or".to_string(), "Odia".to_string()),
        ("om".to_string(), "Oromo".to_string()),
        ("os".to_string(), "Ossetic".to_string()),
        ("ps".to_string(), "Pashto".to_string()),
        ("fa".to_string(), "Persian".to_string()),
        ("pl".to_string(), "Polish".to_string()),
        ("pt-BR".to_string(), "Portuguese (Brazil)".to_string()),
        ("pt-PT".to_string(), "Portuguese (Portugal)".to_string()),
        ("pa".to_string(), "Punjabi".to_string()),
        ("qu".to_string(), "Quechua".to_string()),
        ("ro".to_string(), "Romanian".to_string()),
        ("rm".to_string(), "Romansh".to_string()),
        ("rn".to_string(), "Rundi".to_string()),
        ("ru".to_string(), "Russian".to_string()),
        ("sg".to_string(), "Sango".to_string()),
        ("sa".to_string(), "Sanskrit".to_string()),
        ("sc".to_string(), "Sardinian".to_string()),
        ("gd".to_string(), "Scottish Gaelic".to_string()),
        ("sr".to_string(), "Serbian".to_string()),
        ("sn".to_string(), "Shona".to_string()),
        ("ii".to_string(), "Sichuan Yi".to_string()),
        ("sd".to_string(), "Sindhi".to_string()),
        ("si".to_string(), "Sinhala".to_string()),
        ("sk".to_string(), "Slovak".to_string()),
        ("sl".to_string(), "Slovenian".to_string()),
        ("so".to_string(), "Somali".to_string()),
        ("es".to_string(), "Spanish".to_string()),
        ("su".to_string(), "Sundanese".to_string()),
        ("sw".to_string(), "Swahili".to_string()),
        ("sv".to_string(), "Swedish".to_string()),
        ("tg".to_string(), "Tajik".to_string()),
        ("ta".to_string(), "Tamil".to_string()),
        ("tt".to_string(), "Tatar".to_string()),
        ("te".to_string(), "Telugu".to_string()),
        ("th".to_string(), "Thai".to_string()),
        ("bo".to_string(), "Tibetan".to_string()),
        ("ti".to_string(), "Tigrinya".to_string()),
        ("to".to_string(), "Tongan".to_string()),
        ("tr".to_string(), "Turkish".to_string()),
        ("tk".to_string(), "Turkmen".to_string()),
        ("uk".to_string(), "Ukrainian".to_string()),
        ("ur".to_string(), "Urdu".to_string()),
        ("ug".to_string(), "Uyghur".to_string()),
        ("uz".to_string(), "Uzbek".to_string()),
        ("vi".to_string(), "Vietnamese".to_string()),
        ("cy".to_string(), "Welsh".to_string()),
        ("fy".to_string(), "Western Frisian".to_string()),
        ("wo".to_string(), "Wolof".to_string()),
        ("xh".to_string(), "Xhosa".to_string()),
        ("yi".to_string(), "Yiddish".to_string()),
        ("yo".to_string(), "Yoruba".to_string()),
        ("zu".to_string(), "Zulu".to_string()),
    ])
}

pub(in crate::proxy) fn shop_locale_record(locale: &str, name: &str, published: bool) -> Value {
    json!({
        "locale": locale,
        "name": name,
        "primary": locale == "en",
        "published": published,
        "marketWebPresences": []
    })
}

pub(in crate::proxy) fn shop_locale_user_error(
    field: Vec<&str>,
    message: &str,
    code: &str,
) -> Value {
    json!({
        "field": field,
        "message": message,
        "code": code
    })
}

pub(in crate::proxy) fn is_known_market_web_presence_id(id: &str) -> bool {
    !id.contains("9999999999") && !id.contains("unknown")
}

pub(in crate::proxy) fn shop_locale_market_web_presence_record(id: &str) -> Value {
    json!({
        "id": id,
        "__typename": "MarketWebPresence",
        "defaultLocale": { "locale": "en" }
    })
}

pub(in crate::proxy) fn resolved_list_arg(
    arguments: &BTreeMap<String, ResolvedValue>,
    name: &str,
) -> Vec<ResolvedValue> {
    match arguments.get(name) {
        Some(ResolvedValue::List(values)) => values.clone(),
        _ => Vec::new(),
    }
}

pub(in crate::proxy) fn resolved_string_list_arg(
    arguments: &BTreeMap<String, ResolvedValue>,
    name: &str,
) -> Vec<String> {
    resolved_list_arg(arguments, name)
        .iter()
        .filter_map(|value| match value {
            ResolvedValue::String(value) => Some(value.clone()),
            _ => None,
        })
        .collect()
}

pub(in crate::proxy) fn resolved_object_string(
    value: &ResolvedValue,
    name: &str,
) -> Option<String> {
    match value {
        ResolvedValue::Object(fields) => match fields.get(name) {
            Some(ResolvedValue::String(value)) => Some(value.clone()),
            _ => None,
        },
        _ => None,
    }
}

pub(in crate::proxy) fn normalize_localized_handle(value: &str) -> String {
    let mut normalized = String::new();
    let mut previous_dash = false;
    for ch in value.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            normalized.push(ch);
            previous_dash = false;
        } else if !previous_dash {
            normalized.push('-');
            previous_dash = true;
        }
    }
    let normalized = normalized.trim_matches('-').to_string();
    if normalized.is_empty() {
        "store-localization/generic-dynamic-content-translation".to_string()
    } else {
        normalized
    }
}

pub(in crate::proxy) fn translation_from_input(input: &ResolvedValue) -> Value {
    let locale = resolved_object_string(input, "locale").unwrap_or_else(|| "fr".to_string());
    let key = resolved_object_string(input, "key").unwrap_or_else(|| "title".to_string());
    let value = resolved_object_string(input, "value").unwrap_or_default();
    let market = resolved_object_string(input, "marketId")
        .map(|id| json!({ "id": id }))
        .unwrap_or(Value::Null);
    json!({
        "key": key,
        "value": value,
        "locale": locale,
        "outdated": false,
        "market": market
    })
}

pub(in crate::proxy) fn market_localization_error(
    field: Vec<&str>,
    code: &str,
    message: &str,
) -> Value {
    json!({
        "__typename": "TranslationUserError",
        "field": field,
        "message": message,
        "code": code
    })
}

pub(in crate::proxy) fn is_online_store_theme_record(record: &Value) -> bool {
    record.get("__typename").and_then(Value::as_str) == Some("OnlineStoreTheme")
        || record
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|id| id.starts_with("gid://shopify/OnlineStoreTheme/"))
}

pub(in crate::proxy) fn is_online_store_script_tag_record(record: &Value) -> bool {
    record
        .get("id")
        .and_then(Value::as_str)
        .is_some_and(|id| id.starts_with("gid://shopify/ScriptTag/"))
}

pub(in crate::proxy) fn is_web_pixel_record(record: &Value) -> bool {
    record.get("__typename").and_then(Value::as_str) == Some("WebPixel")
        || record
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|id| id.starts_with("gid://shopify/WebPixel/"))
}

pub(in crate::proxy) fn is_server_pixel_record(record: &Value) -> bool {
    record.get("__typename").and_then(Value::as_str) == Some("ServerPixel")
        || record
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|id| id.starts_with("gid://shopify/ServerPixel/"))
}

pub(in crate::proxy) fn is_storefront_access_token_record(record: &Value) -> bool {
    record.get("__typename").and_then(Value::as_str) == Some("StorefrontAccessToken")
        || record
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|id| id.starts_with("gid://shopify/StorefrontAccessToken/"))
}

pub(in crate::proxy) fn web_pixel_settings_from_resolved(value: &ResolvedValue) -> Option<Value> {
    match value {
        ResolvedValue::String(raw) => serde_json::from_str::<Value>(raw).ok(),
        ResolvedValue::Object(_) | ResolvedValue::List(_) => Some(resolved_value_json(value)),
        ResolvedValue::Null => None,
        _ => Some(resolved_value_json(value)),
    }
}

pub(in crate::proxy) fn synthetic_storefront_access_token(id: &str) -> String {
    let suffix = resource_id_tail(id).parse::<u64>().ok().unwrap_or(0);
    let token = match suffix {
        1 => "bcc6fd83f41123b4",
        3 => "43199f7763e24d2f",
        5 => "5ceddc5ce1576036",
        _ => {
            return format!(
                "shpat_{:016x}",
                0xbcc6_fd83_f411_23b4u64.wrapping_add(suffix)
            )
        }
    };
    format!("shpat_{token}")
}

pub(in crate::proxy) fn storefront_access_scopes_for_request(request: &Request) -> Vec<Value> {
    let scopes = request
        .headers
        .get("x-shopify-draft-proxy-access-scopes")
        .map(|header| {
            header
                .split(',')
                .map(str::trim)
                .filter(|scope| scope.starts_with("unauthenticated_"))
                .map(|scope| json!({"handle": scope}))
                .collect::<Vec<_>>()
        })
        .filter(|scopes| !scopes.is_empty())
        .unwrap_or_else(|| {
            vec![
                json!({"handle": "unauthenticated_read_product_listings"}),
                json!({"handle": "unauthenticated_read_product_inventory"}),
            ]
        });
    scopes
}

pub(in crate::proxy) fn theme_user_error(
    field: Vec<&str>,
    message: &str,
    code: Option<&str>,
) -> Value {
    let field: Vec<&str> = field.into_iter().collect();
    let mut error = json!({"field": field, "message": message});
    if let Some(code) = code {
        error["code"] = json!(code);
    }
    error
}

pub(in crate::proxy) fn theme_file_nodes(theme: &Value) -> Vec<Value> {
    theme["files"]["nodes"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

pub(in crate::proxy) fn set_theme_file_nodes(theme: &mut Value, nodes: Vec<Value>) {
    if let Some(object) = theme.as_object_mut() {
        object.insert("files".to_string(), json!({"nodes": nodes}));
    }
}

pub(in crate::proxy) fn theme_file_arg_string(
    value: &ResolvedValue,
    field: &str,
) -> Option<String> {
    match value {
        ResolvedValue::Object(input) => resolved_string_field(input, field),
        _ => None,
    }
}

pub(in crate::proxy) fn theme_file_record_from_input(
    value: &ResolvedValue,
) -> Result<Option<Value>, ()> {
    let ResolvedValue::Object(input) = value else {
        return Ok(None);
    };
    let Some(filename) = resolved_string_field(input, "filename") else {
        return Ok(None);
    };
    let Some(body) = input.get("body") else {
        return Err(());
    };
    Ok(Some(theme_file_record_from_body(&filename, body)?))
}

pub(in crate::proxy) fn theme_file_record(filename: &str, content: &str) -> Value {
    json!({
        "filename": filename,
        "checksumMd5": theme_file_checksum_md5(content),
        "size": content.len(),
        "body": {"content": content}
    })
}

pub(in crate::proxy) fn theme_file_record_from_body(
    filename: &str,
    body: &ResolvedValue,
) -> Result<Value, ()> {
    let ResolvedValue::Object(body) = body else {
        return Err(());
    };
    let body_type = resolved_string_field(body, "type").unwrap_or_else(|| "TEXT".to_string());
    let value = resolved_string_field(body, "value").unwrap_or_default();
    match body_type.as_str() {
        "TEXT" => Ok(theme_file_record(filename, &value)),
        "BASE64" => {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(value.as_bytes())
                .map_err(|_| ())?;
            let content = String::from_utf8(bytes).map_err(|_| ())?;
            Ok(theme_file_record(filename, &content))
        }
        "URL" => Ok(theme_file_url_record(filename)),
        _ => Err(()),
    }
}

pub(in crate::proxy) fn theme_file_url_record(filename: &str) -> Value {
    json!({
        "filename": filename,
        "checksumMd5": theme_file_checksum_md5(""),
        "size": 0,
        "body": {"type": "URL", "value": Value::Null}
    })
}

pub(in crate::proxy) fn theme_file_operation_result(record: &Value) -> Value {
    json!({
        "filename": record["filename"],
        "createdAt": record
            .get("createdAt")
            .cloned()
            .unwrap_or_else(|| json!("2024-01-01T00:00:00.000Z")),
        "updatedAt": record
            .get("updatedAt")
            .cloned()
            .unwrap_or_else(|| json!("2024-01-01T00:00:00.000Z")),
        "checksumMd5": record["checksumMd5"],
        "size": record["size"],
        "body": record
            .get("body")
            .cloned()
            .unwrap_or(Value::Null)
    })
}

pub(in crate::proxy) fn theme_file_checksum_md5(content: &str) -> String {
    format!("{:x}", md5::compute(content.as_bytes()))
}

pub(in crate::proxy) fn mobile_app_error<const N: usize>(
    code: &str,
    field: [&str; N],
    message: &str,
) -> Value {
    let field: Vec<&str> = field.into_iter().collect();
    json!({"code": code, "field": field, "message": message})
}

pub(in crate::proxy) fn mobile_app_payload(
    selection: &[SelectedField],
    record: Option<Value>,
    errors: Vec<Value>,
) -> Value {
    selected_json(
        &json!({"mobilePlatformApplication": record, "userErrors": errors}),
        selection,
    )
}

pub(in crate::proxy) fn script_tag_payload(
    selection: &[SelectedField],
    record: Option<Value>,
    errors: Vec<Value>,
) -> Value {
    selected_json(
        &json!({"scriptTag": record, "userErrors": errors}),
        selection,
    )
}

pub(in crate::proxy) fn validate_script_src(
    input: &BTreeMap<String, ResolvedValue>,
    create: bool,
) -> Option<Value> {
    let src = resolved_string_field(input, "src")?;
    let field = if create {
        json!(["input", "src"])
    } else {
        json!(["src"])
    };
    if src.trim().is_empty() {
        return Some(json!({"code": "BLANK", "field": field, "message": "Source can't be blank"}));
    }
    if src.len() > 255 {
        return Some(
            json!({"code": "TOO_LONG", "field": field, "message": "Source is too long (maximum is 255 characters)"}),
        );
    }
    if !(src.starts_with("https://") && src.contains('.')) {
        return Some(json!({"code": "INVALID", "field": field, "message": "Source is invalid"}));
    }
    None
}

pub(in crate::proxy) fn webhook_endpoint(uri: &str) -> Value {
    if uri.starts_with("arn:aws:events:") {
        json!({ "__typename": "WebhookEventBridgeEndpoint", "arn": uri })
    } else if let Some(tail) = uri.strip_prefix("pubsub://") {
        let (project, topic) = tail.split_once(':').unwrap_or((tail, ""));
        json!({ "__typename": "WebhookPubSubEndpoint", "pubSubProject": project, "pubSubTopic": topic })
    } else {
        json!({ "__typename": "WebhookHttpEndpoint", "callbackUrl": uri })
    }
}

pub(in crate::proxy) fn webhook_subscription_string_field(record: &Value, field: &str) -> String {
    record[field]
        .as_str()
        .unwrap_or_default()
        .to_ascii_lowercase()
}

pub(in crate::proxy) fn valid_gcp_project_id(project: &str) -> bool {
    if project.chars().all(|ch| ch.is_ascii_digit()) {
        return !project.is_empty();
    }

    let len = project.len();
    (6..=30).contains(&len)
        && project
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_lowercase())
        && project
            .chars()
            .last()
            .is_some_and(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit())
        && project
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
}

pub(in crate::proxy) fn valid_gcp_pubsub_topic_id(topic: &str) -> bool {
    let Some(decoded_topic) = percent_decode_ascii_topic(topic) else {
        return false;
    };

    let len = decoded_topic.len();
    (3..=255).contains(&len)
        && !decoded_topic.starts_with("goog")
        && decoded_topic
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_alphabetic())
        && decoded_topic
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '~' | '%'))
}

fn percent_decode_ascii_topic(topic: &str) -> Option<String> {
    let bytes = topic.as_bytes();
    let mut decoded = String::with_capacity(topic.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = bytes.get(index + 1).copied().and_then(hex_value)?;
            let low = bytes.get(index + 2).copied().and_then(hex_value)?;
            let byte = high * 16 + low;
            if !byte.is_ascii() {
                return None;
            }
            decoded.push(char::from(byte));
            index += 3;
        } else {
            decoded.push(char::from(bytes[index]));
            index += 1;
        }
    }
    Some(decoded)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

pub(in crate::proxy) fn eventbridge_arn_api_client_id(uri: &str) -> Option<&str> {
    let parts: Vec<&str> = uri.splitn(6, ':').collect();
    if parts.len() != 6
        || parts[0] != "arn"
        || parts[1] != "aws"
        || parts[2] != "events"
        || !valid_eventbridge_region(parts[3])
        || !parts[4].is_empty()
    {
        return None;
    }
    let resource = parts[5];
    let tail = resource
        .strip_prefix("event-source/aws.partner/shopify.com/")
        .or_else(|| resource.strip_prefix("event-source/aws.partner/shopify.com.test/"))?;
    let (api_client_id, event_source_name) = tail.split_once('/')?;
    if api_client_id.is_empty()
        || !api_client_id.chars().all(|ch| ch.is_ascii_digit())
        || event_source_name.is_empty()
    {
        return None;
    }
    Some(api_client_id)
}

fn valid_eventbridge_region(region: &str) -> bool {
    let mut parts = region.split('-');
    let Some(prefix) = parts.next() else {
        return false;
    };
    let Some(name) = parts.next() else {
        return false;
    };
    let Some(number) = parts.next() else {
        return false;
    };
    parts.next().is_none()
        && prefix.len() == 2
        && prefix.chars().all(|ch| ch.is_ascii_lowercase())
        && !name.is_empty()
        && name.chars().all(|ch| ch.is_ascii_lowercase())
        && !number.is_empty()
        && number.chars().all(|ch| ch.is_ascii_digit())
}

pub(in crate::proxy) fn webhook_uri_uses_disallowed_host(uri: &str) -> bool {
    let Some(host) = webhook_uri_host(uri) else {
        return false;
    };
    if host == "shopify.com"
        || host.ends_with(".shopify.com")
        || host.ends_with(".myshopify.com")
        || host.ends_with(".shopifypreview.com")
        || host.ends_with(".myshopify.dev")
        || host == "localhost"
    {
        return true;
    }
    if let Ok(std::net::IpAddr::V4(address)) = host.parse::<std::net::IpAddr>() {
        let octets = address.octets();
        return octets[0] == 0
            || octets[0] == 10
            || octets[0] == 127
            || (octets[0] == 172 && (16..=31).contains(&octets[1]))
            || (octets[0] == 192 && octets[1] == 168);
    }
    false
}

pub(in crate::proxy) fn webhook_uri_host(uri: &str) -> Option<String> {
    let rest = uri
        .strip_prefix("https://")
        .or_else(|| uri.strip_prefix("http://"))?;
    let host_with_port = rest.split('/').next().unwrap_or_default();
    Some(
        host_with_port
            .split(':')
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase(),
    )
}

pub(in crate::proxy) fn webhook_subscription_legacy_id(id: &str) -> String {
    resource_id_tail(id).to_string()
}

pub(in crate::proxy) fn webhook_subscription_numeric_id(record: &Value) -> u64 {
    record["id"]
        .as_str()
        .map(webhook_subscription_legacy_id)
        .and_then(|tail| tail.parse::<u64>().ok())
        .unwrap_or(0)
}

pub(in crate::proxy) fn webhook_subscription_matches_field_args(
    record: &Value,
    arguments: &BTreeMap<String, ResolvedValue>,
) -> bool {
    if let Some(format) = resolved_string_arg(arguments, "format") {
        if !record["format"]
            .as_str()
            .is_some_and(|value| value.eq_ignore_ascii_case(&format))
        {
            return false;
        }
    }

    if let Some(uri) = resolved_string_arg(arguments, "uri") {
        if record["uri"].as_str() != Some(uri.as_str())
            && record["callbackUrl"].as_str() != Some(uri.as_str())
        {
            return false;
        }
    }

    let topics = resolved_string_list_arg(arguments, "topics");
    if !topics.is_empty()
        && !record["topic"].as_str().is_some_and(|topic| {
            topics
                .iter()
                .any(|wanted| topic.eq_ignore_ascii_case(wanted))
        })
    {
        return false;
    }

    if let Some(query) = resolved_string_arg(arguments, "query") {
        if !webhook_subscription_matches_query(record, &query) {
            return false;
        }
    }

    true
}

pub(in crate::proxy) fn webhook_subscription_matches_query(record: &Value, query: &str) -> bool {
    for raw_token in query.split_whitespace() {
        let token = raw_token.trim();
        if token.is_empty() || token.eq_ignore_ascii_case("AND") || token.eq_ignore_ascii_case("OR")
        {
            continue;
        }
        let (negated, token) = token
            .strip_prefix('-')
            .map_or((false, token), |tail| (true, tail));
        let Some((field, value)) = token.split_once(':') else {
            continue;
        };
        let matches = webhook_subscription_matches_query_term(record, field, value);
        if matches == negated {
            return false;
        }
    }
    true
}

pub(in crate::proxy) fn webhook_subscription_matches_query_term(
    record: &Value,
    field: &str,
    value: &str,
) -> bool {
    let wanted = value.to_ascii_lowercase();
    match field.to_ascii_lowercase().as_str() {
        "id" => record["id"].as_str().is_some_and(|id| {
            id.eq_ignore_ascii_case(value)
                || webhook_subscription_legacy_id(id).eq_ignore_ascii_case(value)
        }),
        "topic" => webhook_subscription_string_field(record, "topic").contains(&wanted),
        "format" => webhook_subscription_string_field(record, "format") == wanted,
        "uri" | "callbackurl" => {
            webhook_subscription_string_field(record, "uri").contains(&wanted)
                || webhook_subscription_string_field(record, "callbackUrl").contains(&wanted)
        }
        _ => false,
    }
}

pub(in crate::proxy) fn inventory_empty_connection(selection: &[SelectedField]) -> Value {
    selected_json(
        &json!({
            "nodes": [],
            "pageInfo": empty_page_info()
        }),
        selection,
    )
}

pub(in crate::proxy) struct InventoryLevelViewState<'a> {
    pub inventory_level_ids: &'a BTreeMap<(String, String), String>,
    pub inactive_levels: &'a BTreeSet<(String, String)>,
    pub quantity_updated_at: &'a BTreeMap<(String, String, String), String>,
    pub locations: Option<&'a BTreeMap<String, Value>>,
}

pub(in crate::proxy) fn inventory_levels_connection_selected_json(
    inventory_item_id: &str,
    levels: &[(String, BTreeMap<String, i64>)],
    view_state: &InventoryLevelViewState<'_>,
    arguments: &BTreeMap<String, ResolvedValue>,
    selections: &[SelectedField],
) -> Value {
    let include_inactive = matches!(
        arguments.get("includeInactive"),
        Some(ResolvedValue::Bool(true))
    );
    let visible_levels = levels
        .iter()
        .filter(|(location_id, _)| {
            include_inactive
                || !view_state
                    .inactive_levels
                    .contains(&(inventory_item_id.to_string(), location_id.clone()))
        })
        .collect::<Vec<_>>();
    let first = resolved_int_field(arguments, "first")
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(visible_levels.len());
    let mut fields = serde_json::Map::new();
    for selection in selections {
        let value = match selection.name.as_str() {
            "nodes" => Some(Value::Array(
                visible_levels
                    .iter()
                    .take(first)
                    .map(|(location_id, quantities)| {
                        inventory_level_selected_json(
                            inventory_item_id,
                            location_id,
                            quantities,
                            view_state,
                            &selection.selection,
                        )
                    })
                    .collect(),
            )),
            "pageInfo" => Some(selected_json(&empty_page_info(), &selection.selection)),
            _ => None,
        };
        if let Some(value) = value {
            fields.insert(selection.response_key.clone(), value);
        }
    }
    Value::Object(fields)
}

pub(in crate::proxy) fn inventory_level_selected_json(
    inventory_item_id: &str,
    location_id: &str,
    quantities: &BTreeMap<String, i64>,
    view_state: &InventoryLevelViewState<'_>,
    selections: &[SelectedField],
) -> Value {
    let is_active = !view_state
        .inactive_levels
        .contains(&(inventory_item_id.to_string(), location_id.to_string()));
    let mut fields = serde_json::Map::new();
    for selection in selections {
        let value = match selection.name.as_str() {
            "id" => Some(json!(view_state
                .inventory_level_ids
                .get(&(inventory_item_id.to_string(), location_id.to_string()))
                .cloned()
                .unwrap_or_else(|| inventory_level_id(
                    inventory_item_id,
                    location_id
                )))),
            "isActive" => Some(json!(is_active)),
            "item" => Some(selected_json(
                &json!({ "id": inventory_item_id }),
                &selection.selection,
            )),
            "location" => Some(
                view_state
                    .locations
                    .and_then(|locations| locations.get(location_id))
                    .map(|location| selected_json(location, &selection.selection))
                    .unwrap_or_else(|| {
                        selected_json(
                            &json!({
                                "id": location_id,
                                "name": inventory_location_name(location_id)
                            }),
                            &selection.selection,
                        )
                    }),
            ),
            "quantities" => Some(Value::Array(
                inventory_quantity_names(&selection.arguments)
                    .into_iter()
                    .map(|name| {
                        let updated_at = view_state
                            .quantity_updated_at
                            .get(&(
                                inventory_item_id.to_string(),
                                location_id.to_string(),
                                name.clone(),
                            ))
                            .map_or(Value::Null, |value| json!(value));
                        selected_json(
                            &json!({
                                "name": name,
                                "quantity": quantities.get(&name).copied().unwrap_or(0),
                                "updatedAt": updated_at
                            }),
                            &selection.selection,
                        )
                    })
                    .collect(),
            )),
            _ => None,
        };
        if let Some(value) = value {
            fields.insert(selection.response_key.clone(), value);
        }
    }
    Value::Object(fields)
}

fn inventory_quantity_names(arguments: &BTreeMap<String, ResolvedValue>) -> Vec<String> {
    match arguments.get("names") {
        Some(ResolvedValue::List(values)) => values
            .iter()
            .filter_map(|value| match value {
                ResolvedValue::String(name) => Some(name.clone()),
                _ => None,
            })
            .collect(),
        _ => vec![
            "available".to_string(),
            "on_hand".to_string(),
            "damaged".to_string(),
        ],
    }
}

pub(in crate::proxy) fn inventory_level_id(inventory_item_id: &str, location_id: &str) -> String {
    format!(
        "gid://shopify/InventoryLevel/{}-{}?inventory_item_id={}",
        resource_id_tail(inventory_item_id),
        resource_id_tail(location_id),
        inventory_item_id
    )
}

pub(in crate::proxy) fn inventory_level_parts_from_id(id: &str) -> Option<(String, String)> {
    let rest = id.strip_prefix("gid://shopify/InventoryLevel/")?;
    let (level_tail, query) = rest.split_once("?inventory_item_id=")?;
    let (item_tail, location_tail) = level_tail.rsplit_once('-')?;
    let item_id = if query.starts_with("gid://shopify/InventoryItem/") {
        query.to_string()
    } else {
        format!("gid://shopify/InventoryItem/{item_tail}")
    };
    Some((item_id, format!("gid://shopify/Location/{location_tail}")))
}

pub(in crate::proxy) fn inventory_properties_json() -> Value {
    json!({
        "quantityNames": [
            {"name": "available", "displayName": "Available", "isInUse": true, "belongsTo": ["on_hand"], "comprises": []},
            {"name": "committed", "displayName": "Committed", "isInUse": true, "belongsTo": ["on_hand"], "comprises": []},
            {"name": "damaged", "displayName": "Damaged", "isInUse": false, "belongsTo": ["on_hand"], "comprises": []},
            {"name": "incoming", "displayName": "Incoming", "isInUse": false, "belongsTo": [], "comprises": []},
            {"name": "on_hand", "displayName": "On hand", "isInUse": true, "belongsTo": [], "comprises": ["available", "committed", "damaged", "quality_control", "reserved", "safety_stock"]},
            {"name": "quality_control", "displayName": "Quality control", "isInUse": false, "belongsTo": ["on_hand"], "comprises": []},
            {"name": "reserved", "displayName": "Reserved", "isInUse": true, "belongsTo": ["on_hand"], "comprises": []},
            {"name": "safety_stock", "displayName": "Safety stock", "isInUse": false, "belongsTo": ["on_hand"], "comprises": []}
        ]
    })
}

pub(in crate::proxy) fn inventory_change_json(
    item_id: &str,
    name: &str,
    delta: i64,
    _quantity_after_change: i64,
    ledger: Option<&str>,
    location_id: &str,
    location_name: &str,
) -> Value {
    // Real Shopify returns `quantityAfterChange: null` for changes read back
    // from inventoryAdjust/Set/MoveQuantities mutation responses (the field is
    // only populated in certain ledger contexts). Match that to stay faithful to
    // the recorded live captures rather than the staging engine's running total.
    json!({
        "name": name,
        "delta": delta,
        "quantityAfterChange": Value::Null,
        "ledgerDocumentUri": ledger,
        "item": {
            "id": item_id
        },
        "location": {
            "id": location_id,
            "name": location_name
        }
    })
}

pub(in crate::proxy) fn inventory_location_name(location_id: &str) -> &'static str {
    match location_id {
        "gid://shopify/Location/1" => "Source location",
        "gid://shopify/Location/2" => "Destination location",
        "gid://shopify/Location/106318430514" => "Shop location",
        "gid://shopify/Location/106318463282" => "My Custom Location",
        _ => "Shop location",
    }
}

pub(in crate::proxy) fn marketing_connection(
    records: Vec<Value>,
    selection: &[SelectedField],
) -> Value {
    let full = connection_json_with_cursor(
        records,
        |_, record| format!("cursor:{}", record["id"].as_str().unwrap_or("local")),
        empty_page_info(),
    );
    selected_json(&full, selection)
}

pub(in crate::proxy) fn marketing_activity_payload(
    activity: Option<Value>,
    user_errors: Vec<Value>,
) -> Value {
    json!({ "marketingActivity": activity.unwrap_or(Value::Null), "userErrors": user_errors })
}

pub(in crate::proxy) fn marketing_engagement_payload(
    engagement: Option<Value>,
    user_errors: Vec<Value>,
) -> Value {
    json!({ "marketingEngagement": engagement.unwrap_or(Value::Null), "userErrors": user_errors })
}

pub(in crate::proxy) fn marketing_activity_missing_error() -> Value {
    json!({
        "field": null,
        "message": "Marketing activity does not exist.",
        "code": "MARKETING_ACTIVITY_DOES_NOT_EXIST"
    })
}

pub(in crate::proxy) fn marketing_activity_child_events_error() -> Value {
    json!({
        "field": null,
        "message": "This activity has child activities and thus cannot be deleted. Child activities must be deleted before a parent activity.",
        "code": "CANNOT_DELETE_ACTIVITY_WITH_CHILD_EVENTS"
    })
}

pub(in crate::proxy) fn marketing_activity_cannot_update_tactic_to_storefront_error() -> Value {
    json!({
        "field": ["input"],
        "message": "You can not update an activity tactic to STOREFRONT_APP. This type of tactic can only be specified when creating a new activity.",
        "code": "CANNOT_UPDATE_TACTIC_TO_STOREFRONT_APP"
    })
}

pub(in crate::proxy) fn marketing_activity_cannot_update_tactic_from_storefront_error() -> Value {
    json!({
        "field": ["input"],
        "message": "You can not update an activity tactic from STOREFRONT_APP.",
        "code": "CANNOT_UPDATE_TACTIC_IF_ORIGINALLY_STOREFRONT_APP"
    })
}

pub(in crate::proxy) fn marketing_event_missing_error() -> Value {
    json!({
        "field": null,
        "message": "Marketing event does not exist.",
        "code": "MARKETING_EVENT_DOES_NOT_EXIST"
    })
}

pub(in crate::proxy) fn marketing_activity_from_input(
    id: &str,
    input: BTreeMap<String, ResolvedValue>,
    existing: Option<&Value>,
    api_client_id: Option<String>,
) -> Value {
    let old = existing.cloned().unwrap_or_else(|| json!({}));
    let title = resolved_string_field(&input, "title").unwrap_or_else(|| {
        old["title"]
            .as_str()
            .unwrap_or("Marketing activity")
            .to_string()
    });
    let remote_id = resolved_string_field(&input, "remoteId").unwrap_or_else(|| {
        old["remoteId"]
            .as_str()
            .unwrap_or("local-remote")
            .to_string()
    });
    let status = resolved_string_field(&input, "status")
        .unwrap_or_else(|| old["status"].as_str().unwrap_or("UNDEFINED").to_string());
    let tactic = resolved_string_field(&input, "tactic")
        .unwrap_or_else(|| old["tactic"].as_str().unwrap_or("NEWSLETTER").to_string());
    let channel_type = resolved_string_field(&input, "marketingChannelType").unwrap_or_else(|| {
        old["marketingChannelType"]
            .as_str()
            .unwrap_or("EMAIL")
            .to_string()
    });
    let remote_url = resolved_string_field(&input, "remoteUrl").or_else(|| {
        old["marketingEvent"]["manageUrl"]
            .as_str()
            .map(str::to_string)
    });
    let preview_url = resolved_string_field(&input, "previewUrl").or_else(|| {
        old["marketingEvent"]["previewUrl"]
            .as_str()
            .map(str::to_string)
    });
    let url_parameter_value = resolved_string_field(&input, "urlParameterValue")
        .or_else(|| old["urlParameterValue"].as_str().map(str::to_string));
    let channel_handle = resolved_string_field(&input, "channelHandle")
        .map(Value::String)
        .or_else(|| old["marketingEvent"].get("channelHandle").cloned())
        .unwrap_or(Value::Null);
    let utm = resolved_object_field(&input, "utm");
    let old_utm = &old["utmParameters"];
    let campaign = utm
        .as_ref()
        .and_then(|u| resolved_string_field(u, "campaign"))
        .unwrap_or_else(|| {
            old_utm["campaign"]
                .as_str()
                .unwrap_or(&remote_id)
                .to_string()
        });
    let source = utm
        .as_ref()
        .and_then(|u| resolved_string_field(u, "source"))
        .unwrap_or_else(|| {
            old_utm["source"]
                .as_str()
                .unwrap_or("newsletter")
                .to_string()
        });
    let medium = utm
        .as_ref()
        .and_then(|u| resolved_string_field(u, "medium"))
        .unwrap_or_else(|| old_utm["medium"].as_str().unwrap_or("email").to_string());
    let source_medium = marketing_source_and_medium(
        &channel_type,
        &tactic,
        resolved_string_field(&input, "referringDomain").as_deref(),
    );
    let numeric = resource_id_path_tail(id);
    let event_id = old["marketingEvent"]["id"]
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| {
            format!(
                "gid://shopify/MarketingEvent/{}",
                numeric.parse::<u64>().unwrap_or(1) + 1
            )
        });
    let status_label = marketing_status_label(&status, &tactic, None);
    let budget = resolved_object_field(&input, "budget")
        .map(marketing_budget_json)
        .unwrap_or_else(|| old.get("budget").cloned().unwrap_or(Value::Null));
    let ad_spend = resolved_object_field(&input, "adSpend")
        .map(|obj| {
            json!({
                "amount": resolved_string_field(&obj, "amount")
                    .map(|a| normalized_money_amount(&a))
                    .unwrap_or_default(),
                "currencyCode": resolved_string_field(&obj, "currencyCode")
                    .unwrap_or_else(|| "USD".to_string())
            })
        })
        .unwrap_or_else(|| old.get("adSpend").cloned().unwrap_or(Value::Null));
    let scheduled_to_end_at = resolved_string_field(&input, "scheduledEnd")
        .map(Value::String)
        .unwrap_or_else(|| old["marketingEvent"]["scheduledToEndAt"].clone());
    json!({
        "__typename": "MarketingActivity",
        "id": id,
        "apiClientId": api_client_id,
        "title": title,
        "remoteId": remote_id,
        "createdAt": old["createdAt"].as_str().unwrap_or("2026-05-05T00:00:00Z"),
        "updatedAt": "2026-05-05T00:00:00Z",
        "status": status,
        "statusLabel": status_label,
        "targetStatus": null,
        "tactic": tactic,
        "marketingChannelType": channel_type,
        "sourceAndMedium": source_medium,
        "isExternal": true,
        "inMainWorkflowVersion": false,
        "urlParameterValue": url_parameter_value,
        "parentRemoteId": resolved_string_field(&input, "parentRemoteId")
            .map(Value::String)
            .unwrap_or_else(|| old.get("parentRemoteId").cloned().unwrap_or(Value::Null)),
        "hierarchyLevel": resolved_string_field(&input, "hierarchyLevel")
            .map(Value::String)
            .unwrap_or_else(|| old.get("hierarchyLevel").cloned().unwrap_or(Value::Null)),
        "utmParameters": { "campaign": campaign, "source": source, "medium": medium },
        "budget": budget,
        "adSpend": ad_spend,
        "app": { "id": "gid://shopify/App/1", "title": "Draft proxy app" },
        "marketingEvent": {
            "__typename": "MarketingEvent",
            "id": event_id,
            "type": tactic,
            "remoteId": remote_id,
            "channelHandle": channel_handle,
            "startedAt": "2026-05-05T00:00:00Z",
            "endedAt": if matches!(status.as_str(), "INACTIVE" | "DELETED_EXTERNALLY") { json!("2026-05-05T00:00:00Z") } else { Value::Null },
            "scheduledToEndAt": scheduled_to_end_at,
            "manageUrl": remote_url,
            "previewUrl": preview_url,
            "utmCampaign": campaign,
            "utmMedium": medium,
            "utmSource": source,
            "description": title,
            "marketingChannelType": channel_type,
            "sourceAndMedium": source_medium
        }
    })
}

pub(in crate::proxy) fn marketing_budget_json(input: BTreeMap<String, ResolvedValue>) -> Value {
    let total = resolved_object_field(&input, "total").unwrap_or_default();
    json!({
        "budgetType": resolved_string_field(&input, "budgetType").unwrap_or_else(|| "DAILY".to_string()),
        "total": {
            "amount": resolved_string_field(&total, "amount").unwrap_or_else(|| "0.00".to_string()),
            "currencyCode": resolved_string_field(&total, "currencyCode").unwrap_or_else(|| "USD".to_string())
        }
    })
}

pub(in crate::proxy) fn marketing_engagement_from_input(
    input: &BTreeMap<String, ResolvedValue>,
    activity: Option<&Value>,
) -> Value {
    let money = |key: &str| marketing_money_json(input, key);
    json!({
        "__typename": "MarketingEngagement",
        "occurredOn": resolved_string_field(input, "occurredOn"),
        "utcOffset": resolved_string_field(input, "utcOffset"),
        "isCumulative": resolved_bool_field(input, "isCumulative"),
        "impressionsCount": resolved_int_field(input, "impressionsCount"),
        "viewsCount": resolved_int_field(input, "viewsCount"),
        "clicksCount": resolved_int_field(input, "clicksCount"),
        "uniqueClicksCount": resolved_int_field(input, "uniqueClicksCount"),
        "adSpend": money("adSpend"),
        "sales": money("sales"),
        "orders": resolved_string_field(input, "orders"),
        "primaryConversions": resolved_string_field(input, "primaryConversions"),
        "allConversions": resolved_string_field(input, "allConversions"),
        "firstTimeCustomers": resolved_string_field(input, "firstTimeCustomers"),
        "returningCustomers": resolved_string_field(input, "returningCustomers"),
        "marketingActivity": activity.cloned().unwrap_or(Value::Null)
    })
}

pub(in crate::proxy) fn marketing_money_json(
    input: &BTreeMap<String, ResolvedValue>,
    key: &str,
) -> Value {
    let Some(obj) = resolved_object_field(input, key) else {
        return Value::Null;
    };
    json!({
        "amount": resolved_string_field(&obj, "amount").unwrap_or_default(),
        "currencyCode": resolved_string_field(&obj, "currencyCode").unwrap_or_else(|| "USD".to_string())
    })
}

pub(in crate::proxy) fn marketing_money_currency(
    input: &BTreeMap<String, ResolvedValue>,
    key: &str,
) -> Option<String> {
    resolved_object_field(input, key).and_then(|obj| resolved_string_field(&obj, "currencyCode"))
}

pub(in crate::proxy) fn has_marketing_currency_mismatch(
    input: &BTreeMap<String, ResolvedValue>,
) -> bool {
    let mut currencies = BTreeSet::new();
    if let Some(c) = resolved_object_field(input, "budget")
        .and_then(|b| resolved_object_field(&b, "total"))
        .and_then(|t| resolved_string_field(&t, "currencyCode"))
    {
        currencies.insert(c);
    }
    if let Some(c) = marketing_money_currency(input, "adSpend") {
        currencies.insert(c);
    }
    currencies.len() > 1
}

pub(in crate::proxy) fn has_engagement_currency_mismatch(
    input: &BTreeMap<String, ResolvedValue>,
) -> bool {
    let mut currencies = BTreeSet::new();
    for key in ["adSpend", "sales"] {
        if let Some(c) = marketing_money_currency(input, key) {
            currencies.insert(c);
        }
    }
    currencies.len() > 1
}

pub(in crate::proxy) fn invalid_marketing_url_error(
    input: &BTreeMap<String, ResolvedValue>,
    _root: &str,
) -> Option<Value> {
    for (field, value) in [
        ("remoteUrl", resolved_string_field(input, "remoteUrl")),
        ("previewUrl", resolved_string_field(input, "previewUrl")),
    ] {
        if let Some(url) = value {
            if !(url.starts_with("http://") || url.starts_with("https://")) {
                return Some(json!({
                    "field": ["input", field],
                    "message": format!("{} is not a valid URL", field),
                    "code": "INVALID"
                }));
            }
        }
    }
    None
}

/// How Shopify's URL scalar treats a marketing URL value.
pub(in crate::proxy) enum MarketingUrlError {
    /// The value parsed as a URL but its scheme is not http/https. Shopify rejects this
    /// at field-argument validation: a top-level INVALID_FIELD_ARGUMENTS error with the
    /// root field nulled out in `data`.
    WrongScheme,
    /// The value could not be coerced to the URL scalar at all (opaque scheme, no host,
    /// e.g. `data:`/`javascript:`). Shopify rejects this during variable coercion: a
    /// top-level INVALID_VARIABLE error and no execution of the operation.
    MissingHost {
        field: String,
        url: String,
        value: Value,
    },
}

enum UrlScheme {
    Valid,
    WrongScheme,
    MissingHost,
}

fn classify_url_scheme(url: &str) -> UrlScheme {
    let scheme = url.split(':').next().unwrap_or("").to_ascii_lowercase();
    if scheme == "http" || scheme == "https" {
        return UrlScheme::Valid;
    }
    let rest = &url[scheme.len()..];
    // A URI with an authority component (`scheme://host...`) coerces to the URL scalar
    // successfully (so does `mailto:`, a recognized hierarchical scheme); only then does
    // the http/https field validator reject it. Opaque schemes with no host fail coercion.
    if rest.starts_with("://") || scheme == "mailto" {
        UrlScheme::WrongScheme
    } else {
        UrlScheme::MissingHost
    }
}

/// Returns the marketing URL scheme error (if any) for the URL-typed fields of an external
/// marketing activity input, modelling Shopify's URL scalar coercion + scheme validation.
pub(in crate::proxy) fn marketing_url_scheme_error(
    input: &BTreeMap<String, ResolvedValue>,
) -> Option<MarketingUrlError> {
    for field in ["remoteUrl", "remotePreviewImageUrl"] {
        if let Some(url) = resolved_string_field(input, field) {
            match classify_url_scheme(&url) {
                UrlScheme::Valid => {}
                UrlScheme::WrongScheme => return Some(MarketingUrlError::WrongScheme),
                UrlScheme::MissingHost => {
                    return Some(MarketingUrlError::MissingHost {
                        field: field.to_string(),
                        url,
                        value: resolved_variables_json(input),
                    });
                }
            }
        }
    }
    None
}

pub(in crate::proxy) fn marketing_external_input_type_name(root_field: &str) -> &'static str {
    match root_field {
        "marketingActivityUpdateExternal" => "MarketingActivityUpdateExternalInput",
        "marketingActivityUpsertExternal" => "MarketingActivityUpsertExternalInput",
        _ => "MarketingActivityCreateExternalInput",
    }
}

pub(in crate::proxy) fn marketing_input_has_tactic(
    input: &BTreeMap<String, ResolvedValue>,
) -> bool {
    input.contains_key("tactic")
}

pub(in crate::proxy) fn marketing_input_tactic_is_storefront_app(
    input: &BTreeMap<String, ResolvedValue>,
) -> bool {
    input
        .get("tactic")
        .is_some_and(|value| matches!(value, ResolvedValue::String(t) if t == "STOREFRONT" || t == "STOREFRONT_APP"))
}

pub(in crate::proxy) fn marketing_activity_tactic_is_storefront_app(activity: &Value) -> bool {
    matches!(
        activity["tactic"].as_str(),
        Some("STOREFRONT") | Some("STOREFRONT_APP")
    )
}

pub(in crate::proxy) fn marketing_status_label(
    status: &str,
    tactic: &str,
    target_status: Option<&str>,
) -> String {
    if target_status == Some("PAUSED") {
        return "Pausing".to_string();
    }
    match (status, tactic) {
        ("PENDING", "AD") => "In review",
        ("ACTIVE", "POST") => "Posting",
        ("ACTIVE", _) => "Sending",
        ("PAUSED", _) => "Paused",
        ("INACTIVE", "POST") => "Posted",
        ("INACTIVE", "NEWSLETTER") => "Sent",
        ("INACTIVE", _) => "Ended",
        ("DELETED_EXTERNALLY", _) => "Deleted",
        ("UNDEFINED", _) => "Undefined",
        _ => status,
    }
    .to_string()
}

pub(in crate::proxy) fn marketing_source_and_medium(
    channel: &str,
    tactic: &str,
    referring_domain: Option<&str>,
) -> String {
    match (channel, tactic, referring_domain) {
        ("EMAIL", "ABANDONED_CART", _) => "Abandoned cart email",
        ("SEARCH", "AFFILIATE", _) => "Affiliate link",
        ("DISPLAY", "LOYALTY", _) => "Loyalty program",
        ("DISPLAY", "RETARGETING", Some("facebook.com")) => "Facebook retargeting ad",
        ("DISPLAY", "RETARGETING", _) => "Retargeting ad",
        ("SEARCH", "MESSAGE", Some("facebook.com")) => "Message via Facebook Messenger",
        ("SEARCH", "MESSAGE", Some("twitter.com")) => "Twitter message",
        ("SEARCH", "AD", Some("instagram.com")) => "Instagram ad",
        ("SEARCH", "AD", Some(domain)) => return format!("{domain} ad"),
        ("SEARCH", "AD", _) => "Search ad",
        (_, "AD", _) => "Ad",
        ("EMAIL", "NEWSLETTER", _) => "Email newsletter",
        _ => "Email newsletter",
    }
    .to_string()
}

pub(in crate::proxy) fn resolved_string_arg(
    arguments: &BTreeMap<String, ResolvedValue>,
    name: &str,
) -> Option<String> {
    match arguments.get(name) {
        Some(ResolvedValue::String(value)) => Some(value.clone()),
        _ => None,
    }
}

pub(in crate::proxy) fn normalize_draft_order_tag(tag: &str) -> String {
    tag.trim().to_ascii_lowercase()
}

pub(in crate::proxy) fn is_valid_draft_order_invoice_template(template: &str) -> bool {
    template.starts_with("DRAFT_ORDER_") && template != "NOT_A_REAL_TEMPLATE"
}

pub(in crate::proxy) fn draft_order_invoice_recipient(
    args: &BTreeMap<String, ResolvedValue>,
    draft_order: &Value,
) -> Option<String> {
    let recipient = resolved_object_field(args, "email")
        .and_then(|email| resolved_string_field(&email, "to"))
        .or_else(|| draft_order["email"].as_str().map(str::to_string))?;
    let trimmed = recipient.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

pub(in crate::proxy) fn draft_order_invoice_send_metadata(
    args: &BTreeMap<String, ResolvedValue>,
    draft_order: &Value,
) -> Value {
    let email_arg = resolved_object_field(args, "email");
    let recipient = email_arg
        .as_ref()
        .and_then(|email| resolved_string_field(email, "to"))
        .or_else(|| draft_order["email"].as_str().map(str::to_string));

    let mut email = serde_json::Map::new();
    if let Some(value) = recipient {
        email.insert("to".to_string(), json!(value));
    }
    if let Some(email_arg) = email_arg {
        for field in ["subject", "customMessage", "from"] {
            if let Some(value) = resolved_string_field(&email_arg, field) {
                email.insert(field.to_string(), json!(value));
            }
        }
        let bcc = resolved_string_list_field_unsorted(&email_arg, "bcc");
        if !bcc.is_empty() {
            email.insert("bcc".to_string(), json!(bcc));
        }
    }

    let mut metadata = serde_json::Map::new();
    metadata.insert(
        "templateName".to_string(),
        json!(resolved_string_arg(args, "templateName")
            .unwrap_or_else(|| "DRAFT_ORDER_INVOICE".to_string())),
    );
    if let Some(currency) = resolved_string_arg(args, "presentmentCurrencyCode") {
        metadata.insert("presentmentCurrencyCode".to_string(), json!(currency));
    }
    metadata.insert("email".to_string(), Value::Object(email));
    Value::Object(metadata)
}

pub(in crate::proxy) fn draft_order_invoice_money_set(amount: &str, currency_code: &str) -> Value {
    json!({
        "shopMoney": {
            "amount": amount,
            "currencyCode": currency_code
        },
        "presentmentMoney": {
            "amount": amount,
            "currencyCode": currency_code
        }
    })
}

pub(in crate::proxy) fn draft_order_invoice_line_item() -> Value {
    json!({
        "id": "gid://shopify/DraftOrderLineItem/2",
        "title": "Invoice error parity item",
        "name": "Invoice error parity item",
        "quantity": 1,
        "sku": Value::Null,
        "variantTitle": Value::Null,
        "custom": true,
        "requiresShipping": true,
        "taxable": true,
        "customAttributes": [],
        "appliedDiscount": Value::Null,
        "originalUnitPriceSet": draft_order_invoice_money_set("1.0", "CAD"),
        "originalTotalSet": draft_order_invoice_money_set("1.0", "CAD"),
        "discountedTotalSet": draft_order_invoice_money_set("1.0", "CAD"),
        "totalDiscountSet": draft_order_invoice_money_set("0.0", "CAD"),
        "variant": Value::Null
    })
}

pub(in crate::proxy) fn bulk_operation_record_with(
    id: &str,
    status: &str,
    query: &str,
    count: &str,
    created_at: &str,
    file_size: &str,
) -> Value {
    bulk_operation_record_with_type(id, status, "QUERY", query, count, created_at, file_size)
}

pub(in crate::proxy) fn bulk_operation_record_with_type(
    id: &str,
    status: &str,
    operation_type: &str,
    query: &str,
    count: &str,
    created_at: &str,
    file_size: &str,
) -> Value {
    let completed = status == "COMPLETED";
    let file_size_value = if completed {
        json!(file_size)
    } else {
        Value::Null
    };
    json!({
        "id": id,
        "status": status,
        "type": operation_type,
        "errorCode": null,
        "createdAt": created_at,
        "completedAt": if completed { json!(created_at) } else { Value::Null },
        "objectCount": if completed { count } else { "0" },
        "rootObjectCount": if completed { count } else { "0" },
        "fileSize": file_size_value,
        "url": if completed { json!(format!("/__meta/bulk-operations/{}/result.jsonl", resource_id_path_tail(id))) } else { Value::Null },
        "partialDataUrl": null,
        "query": query
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // The `priceListFixedPricesByProductUpdate` validation suite is covered
    // end-to-end against recorded Shopify responses by the markets parity specs
    // (config/parity-specs/markets/price-list-fixed-prices-*): no-op,
    // currency-mismatch, duplicate-id, mutual-exclusion, product-not-exist, and
    // the variant-level add/update/delete guards. The one branch with no parity
    // coverage is the fixed-price cap, exercised here directly against
    // engine-computed state — the limit is derived from the FIXED edges actually
    // present on the price list, never from a synthetic magic id.
    #[test]
    fn product_level_fixed_price_errors_flags_no_op_and_price_limit() {
        let store = Store::default();

        // Empty `pricesToAdd` and `pricesToDeleteByProductIds` with no price list
        // yields only the no-op error.
        let none: Option<Value> = None;
        let no_op = product_level_fixed_price_errors(&store, &none, &[], &[]);
        assert_eq!(no_op.len(), 1);
        assert_eq!(no_op[0]["code"], json!("NO_UPDATE_OPERATIONS_SPECIFIED"));

        // A price list already holding 10,000 FIXED prices sits at the cap, so any
        // resulting set that stays at or above 10,000 trips PRICE_LIMIT_EXCEEDED.
        let edges: Vec<Value> = (0..10_000)
            .map(|index| {
                json!({
                    "node": {
                        "originType": "FIXED",
                        "variant": { "id": format!("gid://shopify/ProductVariant/{index}") }
                    }
                })
            })
            .collect();
        let price_list = json!({ "currency": "EUR", "prices": { "edges": edges } });
        let at_limit = product_level_fixed_price_errors(&store, &Some(price_list), &[], &[]);
        assert!(
            at_limit
                .iter()
                .any(|error| error["code"] == json!("PRICE_LIMIT_EXCEEDED")),
            "expected PRICE_LIMIT_EXCEEDED, got {at_limit:?}"
        );
    }
}

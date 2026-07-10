use super::*;
use sha2::{Digest, Sha256};
use std::fmt::Write as _;

pub(in crate::proxy) fn catalog_user_error(field: Vec<&str>, message: &str, code: &str) -> Value {
    user_error_typed("CatalogUserError", field, message, Some(code))
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
    payload_error(root_key, vec![catalog_user_error(field, message, code)])
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::proxy) enum CatalogContextDriver {
    Market,
    CompanyLocation,
    Country,
}

impl CatalogContextDriver {
    pub(in crate::proxy) fn from_type_name(type_name: &str) -> Option<Self> {
        Some(match type_name {
            "MarketCatalog" | "MARKET" => Self::Market,
            "CompanyLocationCatalog" | "COMPANY_LOCATION" => Self::CompanyLocation,
            "CountryCatalog" | "COUNTRY" => Self::Country,
            _ => return None,
        })
    }

    pub(in crate::proxy) fn catalog_type_name(self) -> &'static str {
        match self {
            Self::Market => "MarketCatalog",
            Self::CompanyLocation => "CompanyLocationCatalog",
            Self::Country => "CountryCatalog",
        }
    }
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
        "contextDriverType": "MARKET",
        "marketIds": market_ids,
        "markets": catalog_markets_connection(market_ids, market_names),
        "operations": [],
        "priceList": null,
        "publication": null
    })
}

pub(in crate::proxy) fn company_location_catalog_record(
    id: &str,
    title: &str,
    status: &str,
    company_location_ids: &[String],
    company_locations: &BTreeMap<String, Value>,
) -> Value {
    json!({
        "__typename": "CompanyLocationCatalog",
        "id": id,
        "title": title,
        "status": status,
        "contextDriverType": "COMPANY_LOCATION",
        "companyLocationIds": company_location_ids,
        "locationIds": company_location_ids,
        "companyLocations": catalog_company_locations_connection(company_location_ids, company_locations),
        "companyLocationsCount": count_object(company_location_ids.len()),
        "operations": [],
        "priceList": null,
        "publication": null
    })
}

pub(in crate::proxy) fn country_catalog_record(
    id: &str,
    title: &str,
    status: &str,
    country_codes: &[String],
) -> Value {
    json!({
        "__typename": "CountryCatalog",
        "id": id,
        "title": title,
        "status": status,
        "contextDriverType": "COUNTRY",
        "countryCodes": country_codes,
        "countries": catalog_countries_connection(country_codes),
        "countriesCount": count_object(country_codes.len()),
        "operations": [],
        "priceList": null,
        "publication": null
    })
}

pub(in crate::proxy) fn catalog_market_ids(catalog: &Value) -> Vec<String> {
    string_array_from_json(&catalog["marketIds"])
}

pub(in crate::proxy) fn catalog_company_location_ids(catalog: &Value) -> Vec<String> {
    let ids = string_array_from_json(&catalog["companyLocationIds"]);
    if ids.is_empty() {
        string_array_from_json(&catalog["locationIds"])
    } else {
        ids
    }
}

pub(in crate::proxy) fn catalog_country_codes(catalog: &Value) -> Vec<String> {
    string_array_from_json(&catalog["countryCodes"])
}

pub(in crate::proxy) fn catalog_context_driver(catalog: &Value) -> CatalogContextDriver {
    catalog["contextDriverType"]
        .as_str()
        .and_then(CatalogContextDriver::from_type_name)
        .or_else(|| {
            catalog["__typename"]
                .as_str()
                .and_then(CatalogContextDriver::from_type_name)
        })
        .or_else(|| {
            catalog["id"]
                .as_str()
                .and_then(shopify_gid_resource_type)
                .and_then(CatalogContextDriver::from_type_name)
        })
        .unwrap_or(CatalogContextDriver::Market)
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

pub(in crate::proxy) fn set_catalog_company_location_ids(
    catalog: &mut Value,
    company_location_ids: &[String],
    company_locations: &BTreeMap<String, Value>,
) {
    if let Some(object) = catalog.as_object_mut() {
        object.insert(
            "companyLocationIds".to_string(),
            json!(company_location_ids),
        );
        object.insert("locationIds".to_string(), json!(company_location_ids));
        object.insert(
            "companyLocations".to_string(),
            catalog_company_locations_connection(company_location_ids, company_locations),
        );
        object.insert(
            "companyLocationsCount".to_string(),
            count_object(company_location_ids.len()),
        );
    }
}

pub(in crate::proxy) fn set_catalog_country_codes(catalog: &mut Value, country_codes: &[String]) {
    if let Some(object) = catalog.as_object_mut() {
        object.insert("countryCodes".to_string(), json!(country_codes));
        object.insert(
            "countries".to_string(),
            catalog_countries_connection(country_codes),
        );
        object.insert(
            "countriesCount".to_string(),
            count_object(country_codes.len()),
        );
    }
}

pub(in crate::proxy) fn catalog_company_locations_connection(
    company_location_ids: &[String],
    company_locations: &BTreeMap<String, Value>,
) -> Value {
    json!({
        "nodes": company_location_ids
            .iter()
            .map(|id| {
                company_locations
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| json!({ "id": id }))
            })
            .collect::<Vec<_>>()
    })
}

pub(in crate::proxy) fn catalog_countries_connection(country_codes: &[String]) -> Value {
    json!({
        "nodes": country_codes
            .iter()
            .map(|code| json!({ "code": code }))
            .collect::<Vec<_>>()
    })
}

pub(in crate::proxy) fn web_presence_market_ids(web_presence: &Value) -> Vec<String> {
    if web_presence["marketIds"].is_array() {
        string_array_from_json(&web_presence["marketIds"])
    } else {
        web_presence["markets"]["nodes"]
            .as_array()
            .map(|nodes| {
                nodes
                    .iter()
                    .filter_map(|node| node["id"].as_str().map(ToString::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }
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

pub(in crate::proxy) type PriceListValidationError =
    (Vec<&'static str>, &'static str, &'static str);

pub(in crate::proxy) fn price_list_user_error(
    field: Vec<&str>,
    message: &str,
    code: &str,
) -> Value {
    user_error_typed("PriceListUserError", field, message, Some(code))
}

pub(in crate::proxy) fn price_list_payload_error(
    root_key: &str,
    field: Vec<&str>,
    message: &str,
    code: &str,
) -> Value {
    payload_error(root_key, vec![price_list_user_error(field, message, code)])
}

pub(in crate::proxy) fn price_list_adjustment_error(
    adjustment: &BTreeMap<String, ResolvedValue>,
) -> Option<PriceListValidationError> {
    let adjustment_type = resolved_string_field(adjustment, "type").unwrap_or_default();
    if !matches!(
        adjustment_type.as_str(),
        "PERCENTAGE_DECREASE" | "PERCENTAGE_INCREASE"
    ) {
        return Some((
            vec!["input", "parent", "adjustment", "type"],
            "Type is invalid",
            "INVALID",
        ));
    }

    let adjustment_value = resolved_number_field(adjustment, "value").unwrap_or_default();
    let invalid_adjustment = adjustment_value < 0.0
        || (adjustment_type == "PERCENTAGE_DECREASE" && adjustment_value > 100.0)
        || (adjustment_type == "PERCENTAGE_INCREASE" && adjustment_value > 1000.0);
    invalid_adjustment.then_some((
        vec!["input", "parent", "adjustment", "value"],
        PRICE_LIST_INVALID_ADJUSTMENT_MESSAGE,
        "INVALID_ADJUSTMENT_VALUE",
    ))
}

pub(in crate::proxy) fn price_list_name_error(
    price_lists: &BTreeMap<String, Value>,
    name: &str,
    current_id: Option<&str>,
) -> Option<PriceListValidationError> {
    if name.trim().is_empty() {
        return Some((vec!["input", "name"], "Name can't be blank", "BLANK"));
    }
    if name.chars().count() > 255 {
        return Some((
            vec!["input", "name"],
            "Name is too long (maximum is 255 characters)",
            "TOO_LONG",
        ));
    }
    price_lists
        .iter()
        .any(|(id, price_list)| {
            current_id != Some(id.as_str()) && price_list["name"].as_str() == Some(name)
        })
        .then_some((
            vec!["input", "name"],
            "Name has already been taken",
            "TAKEN",
        ))
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

pub(in crate::proxy) fn price_list_parent_json(parent: &BTreeMap<String, ResolvedValue>) -> Value {
    let adjustment = resolved_object_field(parent, "adjustment").unwrap_or_default();
    let adjustment_type = resolved_string_field(&adjustment, "type").unwrap_or_default();
    let mut parent_json = json!({
        "adjustment": {
            "type": adjustment_type,
            "value": price_list_adjustment_value_json(&adjustment)
        }
    });
    if let Some(settings) = resolved_object_field(parent, "settings") {
        if let Some(compare_at_mode) = resolved_string_field(&settings, "compareAtMode") {
            if let Some(object) = parent_json.as_object_mut() {
                object.insert(
                    "settings".to_string(),
                    json!({ "compareAtMode": compare_at_mode }),
                );
            }
        }
    }
    parent_json
}

pub(in crate::proxy) fn price_list_record(
    id: &str,
    name: &str,
    currency: &str,
    parent: Value,
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
        "parent": parent,
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
    user_error_typed(
        "PriceListFixedPricesByProductBulkUpdateUserError",
        field,
        message,
        Some(code),
    )
}

pub(in crate::proxy) fn price_list_price_error(field: Value, message: &str, code: &str) -> Value {
    user_error_typed("PriceListPriceUserError", field, message, Some(code))
}

// ----------------------------------------------------------------------------
// Fixed-price edge model. Price
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

fn price_edge_cursor(edge: &Value) -> String {
    edge["cursor"]
        .as_str()
        .map(str::to_string)
        .or_else(|| fixed_price_edge_variant_id(edge))
        .unwrap_or_default()
}

pub(in crate::proxy) fn fixed_price_edge_variant_id(edge: &Value) -> Option<String> {
    edge["node"]["variant"]["id"].as_str().map(str::to_string)
}

fn quantity_rule_node_variant_id(node: &Value) -> Option<String> {
    node["productVariant"]["id"].as_str().map(str::to_string)
}

fn quantity_rule_edges(price_list: &Value) -> Vec<Value> {
    let edges = price_list["quantityRules"]["edges"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    if !edges.is_empty() {
        return edges;
    }
    price_list["quantityRules"]["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|node| {
            json!({
                "cursor": quantity_rule_node_variant_id(node).unwrap_or_default(),
                "node": node,
            })
        })
        .collect()
}

fn quantity_rule_edge_cursor(edge: &Value) -> String {
    edge["cursor"]
        .as_str()
        .map(str::to_string)
        .or_else(|| edge.get("node").and_then(quantity_rule_node_variant_id))
        .unwrap_or_default()
}

fn quantity_rule_nodes(price_list: &Value) -> Vec<Value> {
    let edge_nodes = quantity_rule_edges(price_list)
        .into_iter()
        .filter_map(|edge| edge.get("node").cloned())
        .collect::<Vec<_>>();
    if !edge_nodes.is_empty() {
        return edge_nodes;
    }
    price_list["quantityRules"]["nodes"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

fn quantity_rule_connection_from_nodes(nodes: Vec<Value>) -> Value {
    let cursors = nodes
        .iter()
        .filter_map(quantity_rule_node_variant_id)
        .collect::<Vec<_>>();
    connection_json_with_cursor(
        nodes,
        |_, node| quantity_rule_node_variant_id(node).unwrap_or_default(),
        connection_page_info(
            false,
            false,
            cursors.first().cloned(),
            cursors.last().cloned(),
        ),
    )
}

fn quantity_rule_node_from_input(input: &BTreeMap<String, ResolvedValue>) -> Value {
    json!({
        "minimum": resolved_int_field(input, "minimum").unwrap_or(1),
        "maximum": resolved_int_field(input, "maximum"),
        "increment": resolved_int_field(input, "increment").unwrap_or(1),
        "isDefault": false,
        "originType": "FIXED",
        "productVariant": {"id": resolved_string_field(input, "variantId").unwrap_or_default()}
    })
}

pub(in crate::proxy) fn upsert_quantity_rule_nodes(
    price_list: &mut Value,
    inputs: &[BTreeMap<String, ResolvedValue>],
) {
    let input_variant_ids = inputs
        .iter()
        .filter_map(|input| resolved_string_field(input, "variantId"))
        .collect::<BTreeSet<_>>();
    let mut retained = quantity_rule_nodes(price_list)
        .into_iter()
        .filter(|node| {
            quantity_rule_node_variant_id(node)
                .map(|variant_id| !input_variant_ids.contains(&variant_id))
                .unwrap_or(true)
        })
        .collect::<Vec<_>>();
    let mut nodes = inputs
        .iter()
        .rev()
        .filter_map(|input| {
            let variant_id = resolved_string_field(input, "variantId")?;
            input_variant_ids
                .contains(&variant_id)
                .then(|| quantity_rule_node_from_input(input))
        })
        .collect::<Vec<_>>();
    nodes.reverse();
    nodes.append(&mut retained);
    if let Some(object) = price_list.as_object_mut() {
        object.insert(
            "quantityRules".to_string(),
            quantity_rule_connection_from_nodes(nodes),
        );
    }
}

pub(in crate::proxy) fn delete_quantity_rule_nodes(price_list: &mut Value, variant_ids: &[String]) {
    let nodes = quantity_rule_nodes(price_list)
        .into_iter()
        .filter(|node| {
            quantity_rule_node_variant_id(node)
                .map(|variant_id| !variant_ids.contains(&variant_id))
                .unwrap_or(true)
        })
        .collect::<Vec<_>>();
    if let Some(object) = price_list.as_object_mut() {
        object.insert(
            "quantityRules".to_string(),
            quantity_rule_connection_from_nodes(nodes),
        );
    }
}

fn price_edge_product_id(edge: &Value) -> Option<String> {
    edge["node"]["variant"]["product"]["id"]
        .as_str()
        .map(str::to_string)
}

fn price_edge_origin_type(edge: &Value) -> Option<&str> {
    edge["node"]["originType"].as_str()
}

fn price_edge_matches_id_filter(actual_id: Option<String>, expected: &str) -> bool {
    let Some(actual_id) = actual_id else {
        return false;
    };
    actual_id == expected || resource_id_tail(&actual_id) == expected
}

/// Local staged price search intentionally supports only captured ID filters.
/// Unknown terms resolve to no matches so the proxy does not pretend to emulate
/// Shopify's broader search grammar without evidence.
fn price_edge_matches_query(edge: &Value, query: Option<&str>) -> bool {
    let Some(query) = query.map(str::trim).filter(|query| !query.is_empty()) else {
        return true;
    };
    query.split_whitespace().all(|term| {
        let Some((name, expected)) = term.split_once(':') else {
            return false;
        };
        let expected = expected.trim();
        if expected.is_empty() {
            return false;
        }
        match name.trim() {
            "variant_id" => {
                price_edge_matches_id_filter(fixed_price_edge_variant_id(edge), expected)
            }
            "product_id" => price_edge_matches_id_filter(price_edge_product_id(edge), expected),
            _ => false,
        }
    })
}

fn price_edge_matches_args(edge: &Value, arguments: &BTreeMap<String, ResolvedValue>) -> bool {
    if let Some(origin_type) = resolved_string_field(arguments, "originType") {
        if price_edge_origin_type(edge) != Some(origin_type.as_str()) {
            return false;
        }
    }
    price_edge_matches_query(edge, resolved_string_field(arguments, "query").as_deref())
}

pub(in crate::proxy) fn selected_price_list_prices(
    price_list: &Value,
    arguments: &BTreeMap<String, ResolvedValue>,
    selection: &[SelectedField],
) -> Value {
    let matched = price_edges(price_list)
        .into_iter()
        .filter(|edge| price_edge_matches_args(edge, arguments))
        .collect::<Vec<_>>();
    let (edges, page_info) = connection_window(&matched, arguments, price_edge_cursor);
    let nodes = edges
        .iter()
        .filter_map(|edge| edge.get("node").cloned())
        .collect::<Vec<_>>();
    selected_json(
        &json!({
            "edges": edges,
            "nodes": nodes,
            "pageInfo": page_info
        }),
        selection,
    )
}

pub(in crate::proxy) fn selected_price_list_quantity_rules(
    price_list: &Value,
    arguments: &BTreeMap<String, ResolvedValue>,
    selection: &[SelectedField],
) -> Value {
    let matched = quantity_rule_edges(price_list);
    let (edges, page_info) = connection_window(&matched, arguments, quantity_rule_edge_cursor);
    let nodes = edges
        .iter()
        .filter_map(|edge| edge.get("node").cloned())
        .collect::<Vec<_>>();
    selected_json(
        &json!({
            "edges": edges,
            "nodes": nodes,
            "pageInfo": page_info
        }),
        selection,
    )
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
    money_value(&amount, &currency_code)
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
        "pageInfo": connection_page_info(
            false,
            false,
            cursors.first().map(|cursor| (*cursor).to_string()),
            cursors.last().map(|cursor| (*cursor).to_string())
        )
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

fn quantity_price_break_nodes(edge: &Value) -> Vec<Value> {
    let edge_nodes = edge["node"]["quantityPriceBreaks"]["edges"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|edge| edge.get("node").cloned())
        .collect::<Vec<_>>();
    if !edge_nodes.is_empty() {
        return edge_nodes;
    }
    edge["node"]["quantityPriceBreaks"]["nodes"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

fn quantity_price_break_node_id(node: &Value) -> Option<String> {
    node["id"].as_str().map(str::to_string)
}

fn quantity_price_break_node_variant_id(node: &Value) -> Option<String> {
    node["variant"]["id"].as_str().map(str::to_string)
}

fn quantity_price_break_node_minimum(node: &Value) -> Option<i64> {
    node["minimumQuantity"].as_i64()
}

pub(in crate::proxy) fn quantity_price_break_id_for_variant_minimum(
    price_list: &Value,
    variant_id: &str,
    minimum_quantity: i64,
) -> Option<String> {
    price_edges(price_list).into_iter().find_map(|edge| {
        quantity_price_break_nodes(&edge)
            .into_iter()
            .find_map(|node| {
                let matches_variant =
                    quantity_price_break_node_variant_id(&node).as_deref() == Some(variant_id);
                let matches_minimum =
                    quantity_price_break_node_minimum(&node) == Some(minimum_quantity);
                (matches_variant && matches_minimum)
                    .then(|| quantity_price_break_node_id(&node))
                    .flatten()
            })
    })
}

fn quantity_price_break_connection_from_nodes(nodes: Vec<Value>) -> Value {
    let cursors = nodes
        .iter()
        .filter_map(quantity_price_break_node_id)
        .collect::<Vec<_>>();
    connection_json_with_cursor(
        nodes,
        |index, node| {
            quantity_price_break_node_id(node).unwrap_or_else(|| {
                format!(
                    "{}:{}",
                    quantity_price_break_node_variant_id(node).unwrap_or_default(),
                    index
                )
            })
        },
        connection_page_info(
            false,
            false,
            cursors.first().cloned(),
            cursors.last().cloned(),
        ),
    )
}

fn rebuild_quantity_price_breaks(edge: &mut Value, nodes: Vec<Value>) {
    if let Some(node) = edge.get_mut("node").and_then(Value::as_object_mut) {
        node.insert(
            "quantityPriceBreaks".to_string(),
            quantity_price_break_connection_from_nodes(nodes),
        );
    }
}

pub(in crate::proxy) fn delete_quantity_price_break_nodes(
    price_list: &mut Value,
    break_ids: &[String],
) {
    let mut edges = price_edges(price_list);
    for edge in &mut edges {
        let nodes = quantity_price_break_nodes(edge)
            .into_iter()
            .filter(|node| {
                quantity_price_break_node_id(node)
                    .map(|id| !break_ids.contains(&id))
                    .unwrap_or(true)
            })
            .collect::<Vec<_>>();
        rebuild_quantity_price_breaks(edge, nodes);
    }
    rebuild_price_list_prices(price_list, edges);
}

pub(in crate::proxy) fn delete_quantity_price_break_nodes_for_variants(
    price_list: &mut Value,
    variant_ids: &[String],
) {
    let mut edges = price_edges(price_list);
    for edge in &mut edges {
        let nodes = quantity_price_break_nodes(edge)
            .into_iter()
            .filter(|node| {
                quantity_price_break_node_variant_id(node)
                    .or_else(|| fixed_price_edge_variant_id(edge))
                    .map(|variant_id| !variant_ids.contains(&variant_id))
                    .unwrap_or(true)
            })
            .collect::<Vec<_>>();
        rebuild_quantity_price_breaks(edge, nodes);
    }
    rebuild_price_list_prices(price_list, edges);
}

pub(in crate::proxy) fn quantity_price_break_node_from_input(
    input: &BTreeMap<String, ResolvedValue>,
    id: String,
    currency: &str,
) -> Value {
    let resolved = ResolvedValue::Object(input.clone());
    let amount = fixed_price_input_amount(&resolved, "price").unwrap_or_else(|| "0.0".to_string());
    let currency_code = fixed_price_input_currency(&resolved, "price")
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| currency.to_string());
    let variant_id = resolved_string_field(input, "variantId").unwrap_or_default();
    json!({
        "__typename": "QuantityPriceBreak",
        "id": id,
        "minimumQuantity": resolved_int_field(input, "minimumQuantity").unwrap_or(1),
        "price": money_value(&amount, &currency_code),
        "variant": {"id": variant_id}
    })
}

pub(in crate::proxy) fn upsert_quantity_price_break_nodes(
    price_list: &mut Value,
    inputs: &[(BTreeMap<String, ResolvedValue>, String)],
) {
    let mut edges = price_edges(price_list);
    for edge in &mut edges {
        let Some(variant_id) = fixed_price_edge_variant_id(edge) else {
            continue;
        };
        let currency = price_list_currency(price_list);
        let new_nodes = inputs
            .iter()
            .filter(|(input, _)| {
                resolved_string_field(input, "variantId").as_deref() == Some(variant_id.as_str())
            })
            .map(|(input, id)| quantity_price_break_node_from_input(input, id.clone(), &currency))
            .collect::<Vec<_>>();
        if new_nodes.is_empty() {
            continue;
        }
        let replacement_keys = new_nodes
            .iter()
            .filter_map(|node| {
                Some((
                    quantity_price_break_node_variant_id(node)?,
                    quantity_price_break_node_minimum(node)?,
                ))
            })
            .collect::<BTreeSet<_>>();
        let mut retained = quantity_price_break_nodes(edge)
            .into_iter()
            .filter(|node| {
                let key = quantity_price_break_node_variant_id(node)
                    .zip(quantity_price_break_node_minimum(node));
                key.map(|key| !replacement_keys.contains(&key))
                    .unwrap_or(true)
            })
            .collect::<Vec<_>>();
        let mut nodes = new_nodes;
        nodes.append(&mut retained);
        rebuild_quantity_price_breaks(edge, nodes);
    }
    rebuild_price_list_prices(price_list, edges);
}

/// Dedupe inputs by `variantId`, keeping the last occurrence.
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

pub(in crate::proxy) fn clear_fixed_price_nodes(price_list: &mut Value) {
    if let Some(object) = price_list.as_object_mut() {
        object.insert("fixedPriceRows".to_string(), json!([]));
    }
    rebuild_price_list_prices(price_list, Vec::new());
}

// ----------------------------------------------------------------------------
// Fixed-price validation (variant-level).
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

pub(in crate::proxy) fn fixed_price_input_errors(
    store: &Store,
    price_list: &Value,
    inputs: &[ResolvedValue],
    field_name: &str,
) -> Vec<Value> {
    let expected = price_list_currency(price_list);
    let mut errors = Vec::new();
    for (index, input) in inputs.iter().enumerate() {
        let variant_id = resolved_nonempty_string(input, "variantId").unwrap_or_default();
        if store.fixed_price_variant_lookup(&variant_id).is_none() {
            errors.push(price_list_price_error(
                json!([field_name, index.to_string(), "variantId"]),
                "Product variant ID does not exist.",
                "VARIANT_NOT_FOUND",
            ));
            continue;
        }

        for money_field in ["price", "compareAtPrice"] {
            if let Some(actual) =
                fixed_price_input_currency(input, money_field).filter(|value| !value.is_empty())
            {
                if actual != expected {
                    errors.push(price_list_price_error(
                        json!([field_name, index.to_string(), money_field, "currencyCode"]),
                        "The specified currency does not match the price list's currency.",
                        "PRICE_LIST_CURRENCY_MISMATCH",
                    ));
                }
            }
        }
    }
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

/// the mutation's price list id comes
/// from the `priceListId` argument, falling back to `id`, then `input.priceListId`.
pub(in crate::proxy) fn read_price_list_id(
    arguments: &BTreeMap<String, ResolvedValue>,
) -> Option<String> {
    if let Some(id) =
        resolved_string_field(arguments, "priceListId").filter(|value| !value.is_empty())
    {
        return Some(id);
    }
    if let Some(id) = resolved_string_field(arguments, "id").filter(|value| !value.is_empty()) {
        return Some(id);
    }
    resolved_object_field(arguments, "input")
        .and_then(|input| resolved_string_field(&input, "priceListId"))
        .filter(|value| !value.is_empty())
}

/// the update mutation reads
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

/// The by-product preflight hydrate variables: a `priceListId`/`priceQuery`
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
                push_unique_string(&mut product_ids, id);
            }
        }
    }
    if let Some(ResolvedValue::List(items)) = variables.get("pricesToDeleteByProductIds") {
        for item in items {
            if let ResolvedValue::String(id) = item {
                push_unique_string(&mut product_ids, id);
            }
        }
    }
    json!({
        "priceListId": string_variable("priceListId"),
        "priceQuery": string_variable("priceQuery"),
        "productIds": product_ids,
    })
}

pub(in crate::proxy) fn variant_fixed_prices_preflight_variables(
    fields: &[RootFieldSelection],
) -> Value {
    let mut price_list_id: Option<String> = None;
    let mut variant_ids: Vec<String> = Vec::new();
    let mut output = serde_json::Map::new();

    for field in fields {
        if !matches!(
            field.name.as_str(),
            "priceListFixedPricesAdd" | "priceListFixedPricesUpdate" | "priceListFixedPricesDelete"
        ) {
            continue;
        }
        if price_list_id.is_none() {
            price_list_id = read_price_list_id(&field.arguments);
        }
        for argument_name in ["prices", "pricesToAdd", "variantIdsToDelete", "variantIds"] {
            if let Some(value) = field.arguments.get(argument_name) {
                output.insert(argument_name.to_string(), resolved_value_json(value));
            }
        }
        match field.name.as_str() {
            "priceListFixedPricesAdd" => {
                extend_unique_strings(
                    &mut variant_ids,
                    mutation_variant_ids(&resolved_object_list(&field.arguments, "prices")),
                );
            }
            "priceListFixedPricesUpdate" => {
                let (price_inputs, _) = read_fixed_price_update_inputs(&field.arguments);
                extend_unique_strings(&mut variant_ids, mutation_variant_ids(&price_inputs));
                extend_unique_strings(
                    &mut variant_ids,
                    resolved_string_list_arg(&field.arguments, "variantIdsToDelete"),
                );
            }
            "priceListFixedPricesDelete" => {
                extend_unique_strings(
                    &mut variant_ids,
                    resolved_string_list_arg(&field.arguments, "variantIds"),
                );
            }
            _ => {}
        }
    }

    output.insert(
        "priceListId".to_string(),
        price_list_id.map(Value::String).unwrap_or(Value::Null),
    );
    output.insert("variantIds".to_string(), json!(variant_ids));
    Value::Object(output)
}

/// the ordered validation
/// suite for `priceListFixedPricesByProductUpdate`. Preserves captured error ordering: no-op, missing add products, missing
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

/// the variant ids that
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
    resolved_string_field(money, "amount").map(|amount| normalize_money_amount(&amount))
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

pub(in crate::proxy) fn market_region_country_nodes(
    market_id: &str,
    region_codes: &[String],
) -> Vec<Value> {
    region_codes
        .iter()
        .enumerate()
        .map(|(index, code)| market_region_country_node(market_id, index, code))
        .collect()
}

fn market_region_country_node(market_id: &str, index: usize, code: &str) -> Value {
    let code = code.to_ascii_uppercase();
    let name = country_name_for_code(&code)
        .map(str::to_string)
        .unwrap_or_else(|| code.clone());
    json!({
        "__typename": "MarketRegionCountry",
        "id": market_region_country_id(market_id, index),
        "name": name,
        "code": code
    })
}

fn market_region_country_id(market_id: &str, index: usize) -> String {
    let tail = resource_id_tail(market_id);
    match tail.parse::<u64>() {
        Ok(market_number) => {
            let region_number = market_number.saturating_sub(1) * 1000 + index as u64 + 1;
            shopify_gid("Market/Region", region_number)
        }
        Err(_) => shopify_gid("Market/Region", format!("{tail}-{}", index + 1)),
    }
}

pub(in crate::proxy) fn market_record_from_input(
    id: &str,
    input: &BTreeMap<String, ResolvedValue>,
    name: &str,
    handle: &str,
    region_codes: &[String],
    shop_currency_code: &str,
) -> Value {
    // Defaults for staged market data: status falls
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
    let region_nodes = market_region_country_nodes(id, region_codes);
    json!({
        "id": id,
        "name": name,
        "handle": handle,
        "status": status,
        "enabled": enabled,
        "type": market_type,
        "priceInclusions": market_price_inclusions(input),
        "currencySettings": market_currency_settings_json(input, shop_currency_code),
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
    shop_currency_code: &str,
) -> Value {
    let Some(currency_settings) = resolved_object_field(input, "currencySettings") else {
        return Value::Null;
    };
    let currency_code = resolved_string_field(&currency_settings, "baseCurrency")
        .unwrap_or_else(|| shop_currency_code.to_string());
    json!({
        "baseCurrency": {
            "currencyCode": currency_code,
            "currencyName": market_currency_name(&currency_code)
        },
        "localCurrencies": resolved_bool_field(&currency_settings, "localCurrencies").unwrap_or(false),
        "roundingEnabled": resolved_bool_field(&currency_settings, "roundingEnabled").unwrap_or(false)
    })
}

pub(in crate::proxy) fn market_user_error(field: Vec<&str>, message: &str, code: Value) -> Value {
    user_error_typed_with_code_value("MarketUserError", field, message, code)
}

pub(in crate::proxy) fn shop_locale_record(
    locale: &str,
    name: &str,
    published: bool,
    primary_locale: &str,
) -> Value {
    json!({
        "locale": locale,
        "name": name,
        "primary": locale == primary_locale,
        "published": published,
        "marketWebPresences": []
    })
}

pub(in crate::proxy) fn shop_locale_user_error(field: Vec<&str>, message: &str) -> Value {
    user_error_omit_code(field, message, None)
}

pub(in crate::proxy) fn shop_locale_market_web_presence_record(
    id: &str,
    default_locale: &str,
) -> Value {
    json!({
        "id": id,
        "__typename": "MarketWebPresence",
        "defaultLocale": { "locale": default_locale }
    })
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
        localized_handle_hash_fallback(value)
    } else {
        normalized
    }
}

fn localized_handle_hash_fallback(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    let mut fallback = String::with_capacity("localized-".len() + 24);
    fallback.push_str("localized-");
    for byte in digest.iter().take(12) {
        write!(&mut fallback, "{byte:02x}").expect("writing to String cannot fail");
    }
    fallback
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
    message: &str,
    code: &str,
) -> Value {
    user_error_typed("TranslationUserError", field, message, Some(code))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_localized_handle_uses_valid_deterministic_fallback_for_empty_ascii_slug() {
        let japanese_market = normalize_localized_handle("日本");
        let tokyo_market = normalize_localized_handle("東京");

        assert_ne!(
            japanese_market,
            "store-localization/generic-dynamic-content-translation"
        );
        assert!(!japanese_market.contains('/'));
        assert!(japanese_market
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-'));
        assert_eq!(japanese_market, normalize_localized_handle("日本"));
        assert_ne!(japanese_market, tokyo_market);
    }

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
                        "variant": { "id": shopify_gid("ProductVariant", index) }
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

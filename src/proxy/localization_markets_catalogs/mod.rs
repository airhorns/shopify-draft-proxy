use super::market_unsupported_country_regions::is_unsupported_country_region;
use super::*;
use sha2::{Digest, Sha256};

mod catalogs;
mod localization;
mod markets;
mod web_presence;
mod web_presence_helpers;

pub(in crate::proxy) use self::web_presence_helpers::*;

#[allow(dead_code)]
const BACKUP_REGION_MARKETS_HYDRATE_QUERY: &str = r#"query BackupRegionMarketsHydrate($first: Int!, $regionsFirst: Int!) {
  markets(first: $first) {
    nodes {
      id
      name
      handle
      status
      type
      conditions {
        conditionTypes
        regionsCondition {
          regions(first: $regionsFirst) {
            nodes {
              __typename
              id
              name
              ... on MarketRegionCountry {
                code
              }
            }
          }
        }
      }
    }
  }
}"#;

const BACKUP_REGION_AVAILABLE_HYDRATE_QUERY: &str = r#"query BackupRegionAvailableHydrate {
  availableBackupRegions {
    __typename
    id
    name
    ... on MarketRegionCountry {
      code
    }
  }
}"#;

fn market_relation_connection<'a>(
    records: impl Iterator<Item = &'a Value>,
    market_id: &str,
    market_ids: impl Fn(&Value) -> Vec<String>,
) -> Value {
    connection_json(market_related_records(records, market_id, market_ids))
}

fn market_related_records<'a>(
    records: impl Iterator<Item = &'a Value>,
    market_id: &str,
    market_ids: impl Fn(&Value) -> Vec<String>,
) -> Vec<Value> {
    records
        .filter(|record| market_ids(record).iter().any(|id| id == market_id))
        .cloned()
        .collect()
}

fn selected_market_relation_connection<'a, Records, MarketIds, NodeJson>(
    records: Records,
    market_id: &str,
    arguments: &BTreeMap<String, ResolvedValue>,
    selection: &[SelectedField],
    market_ids: MarketIds,
    node_json: NodeJson,
) -> Value
where
    Records: Iterator<Item = &'a Value>,
    MarketIds: Fn(&Value) -> Vec<String>,
    NodeJson: Fn(&Value, &[SelectedField]) -> Value,
{
    let records = market_related_records(records, market_id, market_ids);
    selected_typed_connection_with_args(&records, arguments, selection, node_json, value_id_cursor)
}

/// Variant-level fixed-price mutations (`priceListFixedPricesAdd`/`Update`/`Delete`)
/// hydrate their baseline price-list/product/variant records through a real Admin
/// GraphQL preflight keyed by the mutation's price list and variant ids.
const FIXED_PRICE_VARIANT_PREFLIGHT_QUERY: &str = "query MarketsMutationPreflightHydrate($priceListId: ID!, $variantIds: [ID!]!) { priceList(id: $priceListId) { __typename id name currency fixedPricesCount prices(first: 20, originType: FIXED) { edges { cursor node { price { amount currencyCode } compareAtPrice { amount currencyCode } originType variant { id sku product { id title } } } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } productVariants: nodes(ids: $variantIds) { __typename ... on ProductVariant { id title sku price compareAtPrice product { id title handle status variants(first: 10) { nodes { id title sku price compareAtPrice } } } } } }";

/// `priceListFixedPricesByProductUpdate` hydrates from the real multi-product
/// preflight query (the canonical Admin GraphQL form recorded from live Shopify)
/// keyed on the de-duplicated product ids.
const FIXED_PRICE_BY_PRODUCT_PREFLIGHT_QUERY: &str = "query MarketsMutationPreflightHydrate($priceListId: ID!, $productIds: [ID!]!, $priceQuery: String) { priceList(id: $priceListId) { __typename id name currency fixedPricesCount prices(first: 10, query: $priceQuery, originType: FIXED) { edges { cursor node { price { amount currencyCode } compareAtPrice { amount currencyCode } originType variant { id sku product { id title } } } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } productNodes: nodes(ids: $productIds) { __typename ... on Product { id title handle status variants(first: 10) { nodes { id title sku price compareAtPrice } } } } }";

/// Quantity pricing/rules mutations validate against an observed price list and
/// product variants. In live-hybrid parity this exact preflight captures that
/// real Shopify context before the supported mutation stays local.
const QUANTITY_PRICING_RULES_PREFLIGHT_QUERY: &str = "query MarketsMutationPreflightHydrate($priceListId: ID!) { priceList(id: $priceListId) { __typename id name currency fixedPricesCount quantityRules(first: 20) { edges { cursor node { minimum maximum increment isDefault originType productVariant { id } } } } prices(first: 20, originType: FIXED) { edges { cursor node { price { amount currencyCode } compareAtPrice { amount currencyCode } originType variant { id sku product { id title } } quantityPriceBreaks(first: 20) { edges { cursor node { id minimumQuantity price { amount currencyCode } variant { id } } } } } } } } products(first: 10) { nodes { id title variants(first: 20) { nodes { id title sku } } } } }";

const CATALOG_RELATION_PRICE_LIST_PREFLIGHT_QUERY: &str = "query CatalogRelationPriceListHydrate($id: ID!) { priceList(id: $id) { __typename id name currency parent { adjustment { type value } } catalog { id } } }";

const CATALOG_RELATION_PUBLICATION_PREFLIGHT_QUERY: &str = "query CatalogRelationPublicationHydrate($id: ID!) { publication(id: $id) { __typename id name autoPublish } }";

/// Web-presence mutations (`webPresenceCreate`/`Update`/`Delete`) hydrate the
/// shop's baseline web presences from a real Admin GraphQL preflight before
/// applying the local mutation. The cassette stores the exact request Shopify saw,
/// so parity cannot hide behind a provenance descriptor string.
const WEB_PRESENCE_PREFLIGHT_QUERY: &str = "query MarketsMutationPreflightHydrate($first: Int!) { shop { myshopifyDomain primaryDomain { id host url sslEnabled } domains { id host url sslEnabled } } webPresences(first: $first) { nodes { id subfolderSuffix domain { id host url sslEnabled } rootUrls { locale url } defaultLocale { locale name primary published } alternateLocales { locale name primary published } markets(first: 5) { nodes { id name handle status type } } } } }";
const WEB_PRESENCE_PREFLIGHT_FIRST: i64 = 20;

/// Market-localization mutations (`marketLocalizationsRegister`/`Remove`) hydrate the
/// target resource's content/digests, the shop's markets, and existing localizations
/// for the target market from an exact Admin GraphQL preflight.
const MARKET_LOCALIZATION_PREFLIGHT_QUERY: &str = "query MarketsMutationPreflightHydrate($resourceId: ID!, $marketId: ID!, $marketsFirst: Int!) { marketLocalizableResource(resourceId: $resourceId) { resourceId marketLocalizableContent { key value digest } marketLocalizations(marketId: $marketId) { key value updatedAt outdated market { id name } } } markets(first: $marketsFirst) { nodes { id name handle status type } } }";
const MARKET_LOCALIZATION_PREFLIGHT_MARKETS_FIRST: i64 = 50;
const PRIMARY_LOCALE_CHANGE_MESSAGE: &str =
    "The primary locale of your store can't be changed through this endpoint.";

const MARKET_MUTATION_TARGETS_HYDRATE_QUERY: &str =
    include_str!("../../../config/parity-requests/markets/market-mutation-targets-hydrate.graphql");

fn first_market_localization_market_id(
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<String> {
    variables
        .get("marketLocalizations")
        .and_then(resolved_value_list)
        .and_then(|localizations| {
            localizations.into_iter().find_map(|localization| {
                resolved_value_object(&localization)
                    .and_then(|object| object.get("marketId").and_then(resolved_value_string))
            })
        })
        .or_else(|| {
            variables
                .get("marketIds")
                .and_then(resolved_value_list)
                .and_then(|market_ids| {
                    market_ids
                        .into_iter()
                        .find_map(|market_id| resolved_value_string(&market_id))
                })
        })
}

fn quantity_pricing_rules_preflight_variant_ids(
    variables: &BTreeMap<String, ResolvedValue>,
) -> Vec<String> {
    let mut ids = BTreeSet::new();
    if let Some(input) = resolved_object_field(variables, "input") {
        for key in [
            "pricesToDeleteByVariantId",
            "quantityRulesToDeleteByVariantId",
            "quantityPriceBreaksToDeleteByVariantId",
        ] {
            ids.extend(list_string_field(&input, key));
        }
        for key in [
            "pricesToAdd",
            "quantityRulesToAdd",
            "quantityPriceBreaksToAdd",
        ] {
            for item in resolved_object_list_field(&input, key) {
                if let Some(id) = resolved_string_field(&item, "variantId") {
                    ids.insert(id);
                }
            }
        }
    }
    ids.extend(list_string_field(variables, "variantIds"));
    for rule in resolved_object_list_field(variables, "quantityRules") {
        if let Some(id) = resolved_string_field(&rule, "variantId") {
            ids.insert(id);
        }
    }
    ids.into_iter().collect()
}

fn quantity_pricing_needs_price_break_preflight(
    variables: &BTreeMap<String, ResolvedValue>,
) -> bool {
    resolved_object_field(variables, "input")
        .map(|input| !list_string_field(&input, "quantityPriceBreaksToDelete").is_empty())
        .unwrap_or(false)
}

fn market_localization_preflight_variables(variables: &BTreeMap<String, ResolvedValue>) -> Value {
    json!({
        "resourceId": variables
            .get("resourceId")
            .and_then(resolved_value_string)
            .unwrap_or_default(),
        "marketId": first_market_localization_market_id(variables).unwrap_or_default(),
        "marketsFirst": MARKET_LOCALIZATION_PREFLIGHT_MARKETS_FIRST,
    })
}

const LOCALIZATION_MUTATION_TARGETS_HYDRATE_QUERY: &str = r#"query LocalizationMutationTargetsHydrate($ids: [ID!]!) {
  nodes(ids: $ids) {
    __typename
    ... on Market {
      id
      name
      handle
      status
      type
    }
    ... on MarketWebPresence {
      id
      subfolderSuffix
      domain {
        id
        host
        url
        sslEnabled
      }
      rootUrls {
        locale
        url
      }
      defaultLocale {
        locale
        name
        primary
        published
      }
      alternateLocales {
        locale
        name
        primary
        published
      }
      markets(first: 250) {
        nodes {
          id
          name
          handle
          status
          type
        }
      }
    }
  }
}"#;

/// Synthetic `updatedAt` stamped on locally-staged market localizations. The specs
/// match this field loosely (`iso-timestamp` / `non-empty-string`), so a fixed
/// deterministic value keeps state round-tripping reproducible.
const SYNTHETIC_MARKET_LOCALIZATION_TIMESTAMP: &str = "2026-01-01T00:00:00Z";

pub(in crate::proxy) struct PriceListFieldOutcome {
    value: Value,
    errors: Vec<Value>,
}

impl PriceListFieldOutcome {
    fn payload(value: Value) -> Self {
        Self {
            value,
            errors: Vec::new(),
        }
    }

    fn price_list_error(field: &RootFieldSelection, error: PriceListValidationError) -> Self {
        let (path, message, code) = error;
        Self::payload(selected_json(
            &price_list_payload_error("priceList", path, message, code),
            &field.selection,
        ))
    }

    fn resource_not_found(id: &str, field: &RootFieldSelection) -> Self {
        Self {
            value: Value::Null,
            errors: vec![json!({
                "message": format!("Invalid id: {id}"),
                "extensions": {"code": "RESOURCE_NOT_FOUND"},
                "path": [field.response_key.clone()]
            })],
        }
    }
}

fn price_list_catalog_id_has_wrong_gid_type(id: &str) -> bool {
    matches!(shopify_gid_resource_type(id), Some(resource_type) if resource_type != "MarketCatalog")
}

fn selected_record_field(record: &Value, selection: &SelectedField) -> Option<Value> {
    let projected = selected_json(record, std::slice::from_ref(selection));
    projected.get(&selection.response_key).cloned()
}

fn selected_record_with_connections(
    record: &Value,
    selections: &[SelectedField],
    mut connection_field: impl FnMut(&SelectedField) -> Option<Value>,
) -> Value {
    if record.is_null() {
        return Value::Null;
    }
    selected_payload_json(selections, |selection| {
        connection_field(selection).or_else(|| selected_record_field(record, selection))
    })
}

fn selected_resource_payload(
    field: &RootFieldSelection,
    resource_key: &str,
    resource: Value,
    user_errors: Vec<Value>,
    resource_json: impl Fn(&Value, &[SelectedField]) -> Value,
) -> Value {
    selected_payload_json(&field.selection, |selection| {
        match selection.name.as_str() {
            "userErrors" => Some(selected_user_errors(&user_errors, &selection.selection)),
            name if name == resource_key => Some(resource_json(&resource, &selection.selection)),
            _ => None,
        }
    })
}

fn value_string<'a>(value: &'a Value, field: &str) -> &'a str {
    value.get(field).and_then(Value::as_str).unwrap_or_default()
}

fn normalized_sort_string(value: &str) -> StagedSortValue {
    StagedSortValue::String(value.to_ascii_lowercase())
}

fn value_gid_tail_sort_value(value: &Value) -> StagedSortValue {
    resource_id_tail_sort_value(value.get("id").and_then(Value::as_str))
}

fn catalog_gid_tail_sort_value(catalog: &Value) -> StagedSortValue {
    value_gid_tail_sort_value(catalog)
}

fn catalog_normalized_string(catalog: &Value, field: &str) -> StagedSortValue {
    normalized_sort_string(value_string(catalog, field))
}

fn catalog_staged_sort_key(catalog: &Value, sort_key: Option<&str>) -> StagedSortKey {
    let id = catalog_gid_tail_sort_value(catalog);
    let primary = match sort_key.unwrap_or("ID") {
        "TITLE" => catalog_normalized_string(catalog, "title"),
        "ID" => id.clone(),
        _ => id.clone(),
    };
    vec![primary, id]
}

fn catalog_type_value(catalog: &Value) -> String {
    if let Some(driver_type) = catalog.get("contextDriverType").and_then(Value::as_str) {
        return driver_type.to_ascii_uppercase();
    }
    let type_name = catalog
        .get("__typename")
        .and_then(Value::as_str)
        .or_else(|| {
            catalog
                .get("id")
                .and_then(Value::as_str)
                .and_then(shopify_gid_resource_type)
        })
        .unwrap_or("MarketCatalog");
    match type_name {
        "MarketCatalog" | "MARKET" => "MARKET",
        "CompanyLocationCatalog" | "COMPANY_LOCATION" => "COMPANY_LOCATION",
        "CountryCatalog" | "COUNTRY" => "COUNTRY",
        "AppCatalog" | "APP" => "APP",
        "NoneCatalog" | "NONE" => "NONE",
        other => other,
    }
    .to_ascii_uppercase()
}

fn catalog_matches_type(catalog: &Value, type_filter: Option<&str>) -> bool {
    type_filter
        .map(|expected| catalog_type_value(catalog).eq_ignore_ascii_case(expected))
        .unwrap_or(true)
}

fn catalog_search_decision(
    catalog: &Value,
    query: Option<&str>,
    type_filter: Option<&str>,
) -> StagedSearchDecision {
    if !catalog_matches_type(catalog, type_filter) {
        return StagedSearchDecision::NoMatch;
    }
    let Some(query) = query else {
        return StagedSearchDecision::Match;
    };
    let query = query.trim();
    if query.is_empty() {
        return StagedSearchDecision::Match;
    }
    for term in query.split_whitespace() {
        if term.eq_ignore_ascii_case("AND") {
            continue;
        }
        match catalog_search_term_decision(catalog, term) {
            StagedSearchDecision::Match => {}
            StagedSearchDecision::NoMatch => return StagedSearchDecision::NoMatch,
            StagedSearchDecision::Unsupported => return StagedSearchDecision::Unsupported,
        }
    }
    StagedSearchDecision::Match
}

fn catalog_search_term_decision(catalog: &Value, term: &str) -> StagedSearchDecision {
    let term = unquote_search_value(term);
    if term.is_empty() {
        return StagedSearchDecision::Match;
    }
    if let Some((key, value)) = term.split_once(':') {
        let value = unquote_search_value(value);
        return match key {
            "id" => StagedSearchDecision::from_bool(
                catalog
                    .get("id")
                    .and_then(Value::as_str)
                    .map(|id| {
                        id.eq_ignore_ascii_case(value)
                            || resource_id_tail(id).eq_ignore_ascii_case(value)
                    })
                    .unwrap_or(false),
            ),
            "status" => StagedSearchDecision::from_bool(
                catalog
                    .get("status")
                    .and_then(Value::as_str)
                    .is_some_and(|status| status.eq_ignore_ascii_case(value)),
            ),
            "title" => StagedSearchDecision::from_bool(catalog_text_matches(
                catalog.get("title").and_then(Value::as_str),
                value,
            )),
            "type" => StagedSearchDecision::from_bool(
                catalog_type_value(catalog).eq_ignore_ascii_case(value),
            ),
            _ => StagedSearchDecision::Unsupported,
        };
    }

    let fields = [
        catalog.get("id").and_then(Value::as_str),
        catalog.get("title").and_then(Value::as_str),
        catalog.get("status").and_then(Value::as_str),
        catalog.get("__typename").and_then(Value::as_str),
    ];
    StagedSearchDecision::from_bool(
        fields
            .into_iter()
            .any(|value| catalog_text_matches(value, term)),
    )
}

fn catalog_text_matches(value: Option<&str>, term: &str) -> bool {
    let needle = search_term_needle(term);
    !needle.is_empty() && value.is_some_and(|value| value_contains_ci(value, term))
}

fn market_sort_key(market: &Value, sort_key: Option<&str>) -> StagedSortKey {
    let id_sort = value_gid_tail_sort_value(market);
    let primary = match sort_key.unwrap_or("ID") {
        "NAME" => normalized_sort_string(value_string(market, "name")),
        "HANDLE" => normalized_sort_string(value_string(market, "handle")),
        "STATUS" => normalized_sort_string(value_string(market, "status")),
        "TYPE" => normalized_sort_string(value_string(market, "type")),
        "ID" | "RELEVANCE" => id_sort.clone(),
        _ => id_sort.clone(),
    };
    vec![primary, id_sort]
}

fn market_search_decision(market: &Value, query: Option<&str>) -> StagedSearchDecision {
    let Some(query) = query else {
        return StagedSearchDecision::Match;
    };
    let query = query.trim();
    if query.is_empty() {
        return StagedSearchDecision::Match;
    }
    for term in query.split_whitespace() {
        match market_search_term_decision(market, term) {
            StagedSearchDecision::Match => {}
            StagedSearchDecision::NoMatch => return StagedSearchDecision::NoMatch,
            StagedSearchDecision::Unsupported => return StagedSearchDecision::Unsupported,
        }
    }
    StagedSearchDecision::Match
}

fn market_search_term_decision(market: &Value, term: &str) -> StagedSearchDecision {
    let term = unquote_search_value(term);
    if term.is_empty() {
        return StagedSearchDecision::Match;
    }
    if let Some((key, value)) = term.split_once(':') {
        let value = unquote_search_value(value);
        let matches = match key {
            "id" => {
                let id = value_string(market, "id");
                id == value || resource_id_tail(id) == value
            }
            "name" => value_contains_ci(value_string(market, "name"), value),
            "handle" => value_contains_ci(value_string(market, "handle"), value),
            "status" => value_string(market, "status").eq_ignore_ascii_case(value),
            "type" => value_string(market, "type").eq_ignore_ascii_case(value),
            "enabled" => market
                .get("enabled")
                .and_then(Value::as_bool)
                .is_some_and(|enabled| enabled.to_string().eq_ignore_ascii_case(value)),
            _ => return StagedSearchDecision::Unsupported,
        };
        return StagedSearchDecision::from_bool(matches);
    }

    let haystack = format!(
        "{} {} {} {} {}",
        value_string(market, "id"),
        value_string(market, "name"),
        value_string(market, "handle"),
        value_string(market, "status"),
        value_string(market, "type")
    )
    .to_ascii_lowercase();
    StagedSearchDecision::from_bool(value_contains_ci(&haystack, term))
}

fn value_contains_ci(value: &str, needle: &str) -> bool {
    value
        .to_ascii_lowercase()
        .contains(&search_term_needle(needle))
}

fn unquote_search_value(value: &str) -> &str {
    value.trim().trim_matches('\'').trim_matches('"')
}

fn search_term_needle(value: &str) -> String {
    value.trim_end_matches('*').to_ascii_lowercase()
}

fn apply_context_id_diff<ReadIds>(
    ids: &mut Vec<String>,
    contexts_to_remove: Option<&BTreeMap<String, ResolvedValue>>,
    contexts_to_add: Option<&BTreeMap<String, ResolvedValue>>,
    read_ids: ReadIds,
) where
    ReadIds: Fn(&BTreeMap<String, ResolvedValue>) -> Vec<String>,
{
    if let Some(context) = contexts_to_remove {
        let remove = read_ids(context).into_iter().collect::<BTreeSet<_>>();
        ids.retain(|id| !remove.contains(id));
    }
    if let Some(context) = contexts_to_add {
        for id in read_ids(context) {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
    }
}

fn next_markets_catalogs_numeric_id(store: &Store, extra_len: usize) -> usize {
    (store.staged.markets.len() * 2) + (store.staged.catalogs.len() * 2) + extra_len + 1
}
fn selected_catalog_error(
    field: &RootFieldSelection,
    path: Vec<&str>,
    message: &str,
    code: &str,
) -> Value {
    selected_json(
        &catalog_payload_error(path, message, code),
        &field.selection,
    )
}

const CATALOG_CONTEXT_DRIVER_MISMATCH_MESSAGE: &str =
    "The arguments `contexts_to_add` and `contexts_to_remove` must match existing catalog context type.";
const COMPANY_LOCATION_NOT_FOUND_MESSAGE: &str =
    "A company location within the catalog does not exist.";

fn catalog_context_type_fields(
    context: &BTreeMap<String, ResolvedValue>,
) -> Vec<(CatalogContextDriver, &'static str)> {
    let mut fields = Vec::new();
    if context.contains_key("marketIds") {
        fields.push((CatalogContextDriver::Market, "marketIds"));
    }
    if context.contains_key("companyLocationIds") {
        fields.push((CatalogContextDriver::CompanyLocation, "companyLocationIds"));
    }
    if context.contains_key("locationIds") {
        fields.push((CatalogContextDriver::CompanyLocation, "locationIds"));
    }
    if context.contains_key("countryCodes") {
        fields.push((CatalogContextDriver::Country, "countryCodes"));
    }
    fields
}

fn company_location_ids_from_context(context: &BTreeMap<String, ResolvedValue>) -> Vec<String> {
    let mut ids = list_string_field(context, "companyLocationIds");
    for id in list_string_field(context, "locationIds") {
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    ids
}

fn country_codes_from_context(context: &BTreeMap<String, ResolvedValue>) -> Vec<String> {
    list_string_field(context, "countryCodes")
        .into_iter()
        .map(|code| code.to_ascii_uppercase())
        .collect()
}

fn selected_market_user_errors(field: &RootFieldSelection, user_errors: Vec<Value>) -> Value {
    selected_json(
        &json!({"market": Value::Null, "userErrors": user_errors}),
        &field.selection,
    )
}

fn selected_market_error(
    field: &RootFieldSelection,
    path: Vec<&str>,
    message: &str,
    code: Value,
) -> Value {
    selected_market_user_errors(field, vec![market_user_error(path, message, code)])
}

fn selected_payload_user_error(
    selection: &[SelectedField],
    root_key: &str,
    user_error: Value,
) -> Value {
    selected_json(&payload_user_error(root_key, user_error), selection)
}

fn shop_locale_payload_error(root_key: &str, message: &str) -> Value {
    payload_user_error(root_key, shop_locale_user_error(vec!["locale"], message))
}

fn selected_market_localization_error(
    selection: &[SelectedField],
    path: Vec<&str>,
    code: &str,
    message: &str,
) -> Value {
    selected_payload_user_error(
        selection,
        "marketLocalizations",
        market_localization_error(path, message, code),
    )
}

fn market_id_payload_error(root_key: &str, message: &str, code: &str) -> Value {
    payload_user_error(
        root_key,
        market_user_error(vec!["id"], message, json!(code)),
    )
}

fn selected_translation_error(selection: &[SelectedField], message: &str, code: &str) -> Value {
    selected_payload_user_error(
        selection,
        "translations",
        user_error(["resourceId"], message, Some(code)),
    )
}

/// Add an alternate locale + root URL to a staged web-presence record if absent.
fn web_presence_add_locale(record: &mut Value, locale: &str, name: Option<&str>) {
    let Some(obj) = record.as_object_mut() else {
        return;
    };
    let display_name = name.unwrap_or(locale).to_string();
    let suffix = obj
        .get("subfolderSuffix")
        .and_then(Value::as_str)
        .filter(|suffix| !suffix.is_empty())
        .map(str::to_string);
    let origin = obj
        .get("rootUrls")
        .and_then(Value::as_array)
        .and_then(|urls| urls.first())
        .and_then(|entry| entry.get("url"))
        .and_then(Value::as_str)
        .and_then(web_presence_origin);

    if let Some(alternates) = obj
        .entry("alternateLocales")
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
    {
        if !alternates
            .iter()
            .any(|entry| entry["locale"].as_str() == Some(locale))
        {
            alternates.push(json!({
                "locale": locale,
                "name": display_name,
                "primary": false,
                "published": false
            }));
        }
    }

    if let Some(origin) = origin {
        if let Some(root_urls) = obj
            .entry("rootUrls")
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
        {
            if !root_urls
                .iter()
                .any(|entry| entry["locale"].as_str() == Some(locale))
            {
                let url = match suffix.as_deref() {
                    Some(suffix) => format!("{origin}/{locale}-{suffix}/"),
                    None => format!("{origin}/{locale}/"),
                };
                root_urls.push(json!({ "locale": locale, "url": url }));
            }
        }
    }
}

/// Remove an alternate locale + its root URL from a staged web-presence record.
fn web_presence_remove_locale(record: &mut Value, locale: &str) {
    let Some(obj) = record.as_object_mut() else {
        return;
    };
    if let Some(alternates) = obj
        .get_mut("alternateLocales")
        .and_then(Value::as_array_mut)
    {
        alternates.retain(|entry| entry["locale"].as_str() != Some(locale));
    }
    if let Some(root_urls) = obj.get_mut("rootUrls").and_then(Value::as_array_mut) {
        root_urls.retain(|entry| entry["locale"].as_str() != Some(locale));
    }
}

/// The shop's authoritative myshopify domain, used as the host for synthesized
/// web-presence root URLs when no custom domain is selected.
fn web_presence_shop_domain(store: &Store) -> Option<String> {
    let shop = store.effective_shop();
    if let Some(domain) = shop
        .get("myshopifyDomain")
        .and_then(Value::as_str)
        .filter(|domain| !domain.is_empty())
        .filter(|domain| *domain != "shopify-draft-proxy.local")
    {
        return Some(domain.to_string());
    }
    observed_web_presence_shop_domain(store)
}

fn observed_web_presence_shop_domain(store: &Store) -> Option<String> {
    let mut fallback = None;
    for record in store.staged.web_presences.values() {
        if let Some(host) = record
            .get("domain")
            .and_then(|domain| domain.get("host"))
            .and_then(Value::as_str)
            .filter(|host| !host.is_empty())
        {
            if host.ends_with(".myshopify.com") {
                return Some(host.to_string());
            }
            fallback.get_or_insert_with(|| host.to_string());
        }
        if let Some(root_urls) = record.get("rootUrls").and_then(Value::as_array) {
            for root_url in root_urls {
                let Some(host) = root_url
                    .get("url")
                    .and_then(Value::as_str)
                    .and_then(web_presence_host)
                else {
                    continue;
                };
                if host.ends_with(".myshopify.com") {
                    return Some(host);
                }
                fallback.get_or_insert(host);
            }
        }
    }
    fallback
}

fn web_presence_targets_shop_primary_host(store: &Store, record: &Value) -> bool {
    let Some(target_host) = record
        .get("domain")
        .and_then(web_presence_domain_normalized_host)
    else {
        return false;
    };
    web_presence_primary_domain_host(store).as_deref() == Some(target_host.as_str())
}

fn web_presence_primary_domain_host(store: &Store) -> Option<String> {
    let shop = store.effective_shop();
    shop.get("primaryDomain")
        .and_then(web_presence_domain_normalized_host)
        .or_else(|| {
            shop.get("myshopifyDomain")
                .and_then(Value::as_str)
                .and_then(web_presence_normalized_host)
        })
}

fn web_presence_domain_normalized_host(domain: &Value) -> Option<String> {
    domain
        .get("host")
        .and_then(Value::as_str)
        .and_then(web_presence_normalized_host)
        .or_else(|| {
            domain
                .get("url")
                .and_then(Value::as_str)
                .and_then(web_presence_normalized_host)
        })
}

fn web_presence_normalized_host(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let without_scheme = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
        .unwrap_or(trimmed);
    without_scheme
        .split('/')
        .next()
        .map(str::trim)
        .map(|host| host.trim_end_matches('.'))
        .filter(|host| !host.is_empty())
        .map(str::to_ascii_lowercase)
}

fn web_presence_host(url: &str) -> Option<String> {
    web_presence_scheme_host(url).map(|(_, host)| host.to_string())
}

/// Extract `scheme://host` from a URL, dropping any path/query suffix.
fn web_presence_origin(url: &str) -> Option<String> {
    let (scheme, host) = web_presence_scheme_host(url)?;
    Some(format!("{scheme}://{host}"))
}

fn web_presence_scheme_host(url: &str) -> Option<(&str, &str)> {
    let (scheme, rest) = url.split_once("://")?;
    let host = rest.split('/').next().unwrap_or("");
    if host.is_empty() {
        None
    } else {
        Some((scheme, host))
    }
}

fn localization_content_digest(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn market_localizable_content_is_money_metafield(content_entry: &Value) -> bool {
    if content_entry.get("key").and_then(Value::as_str) != Some("value") {
        return false;
    }
    let Some(value) = content_entry.get("value").and_then(Value::as_str) else {
        return false;
    };
    let Ok(parsed) = serde_json::from_str::<Value>(value) else {
        return false;
    };
    parsed
        .as_object()
        .is_some_and(|object| object.contains_key("amount") && object.contains_key("currency_code"))
}

fn markets_collect_records(data: &Value, connection_key: &str, singular_key: &str) -> Vec<Value> {
    let mut records = data
        .get(connection_key)
        .map(connection_nodes)
        .unwrap_or_default();
    if let Some(record) = data.get(singular_key).filter(|value| value.is_object()) {
        records.push(record.clone());
    }
    records
}

/// The `marketId` argument applied to a read's nested `marketLocalizations`
/// selection, used to filter staged localizations to a single market the way the
/// live `marketLocalizableResource.marketLocalizations(marketId:)` field does.
fn market_localizations_market_filter(selection: &[SelectedField]) -> Option<String> {
    selection
        .iter()
        .find(|field| field.name == "marketLocalizations")
        .and_then(|field| resolved_string_field(&field.arguments, "marketId"))
}

fn record_gid(record: &Value, resource_type: &str) -> Option<String> {
    record
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| {
            if resource_type.is_empty() {
                shopify_gid_resource_type(id).is_some()
            } else {
                is_shopify_gid_of_type(id, resource_type)
            }
        })
        .map(str::to_string)
}

/// Next synthetic `MarketWebPresence` numeric id: one greater than the highest
/// numeric id already staged. Deriving from the max (not the count) keeps a newly
/// created presence sorting after any live baseline ids hydrated by the preflight,
/// so a downstream `webPresences` read returns Shopify's id-ascending order. The
/// live ids are equal-width integers, so the staged `BTreeMap` key order matches
/// numeric order.
fn next_web_presence_numeric_id(web_presences: &BTreeMap<String, Value>) -> u64 {
    web_presences
        .keys()
        .map(|key| resource_id_path_tail(key.as_str()))
        .filter_map(|numeric| numeric.parse::<u64>().ok())
        .max()
        .unwrap_or(0)
        + 1
}

/// A market participates in backup-region coverage when it is enabled, of REGION
/// type, and not a legacy market. Used for captured backup-region coverage decisions.
fn market_record_is_active_region_non_legacy(market: &Value) -> bool {
    market_record_enabled(market)
        && market_record_region_type(market)
        && !market_record_legacy(market)
}

fn market_record_enabled(market: &Value) -> bool {
    match market.get("enabled") {
        Some(Value::Bool(enabled)) => *enabled,
        _ => market.get("status").and_then(Value::as_str) == Some("ACTIVE"),
    }
}

fn market_record_region_type(market: &Value) -> bool {
    match market.get("type").and_then(Value::as_str) {
        Some("REGION") => true,
        _ => !market_record_country_codes(market).is_empty(),
    }
}

fn market_field_omits_base_currency(field: &RootFieldSelection) -> bool {
    if !matches!(field.name.as_str(), "marketCreate" | "marketUpdate") {
        return false;
    }
    let Some(currency_settings) = resolved_object_field(&field.arguments, "input")
        .and_then(|input| resolved_object_field(&input, "currencySettings"))
    else {
        return false;
    };
    !currency_settings.contains_key("baseCurrency")
}

fn market_taxes_included(market: &Value) -> Option<bool> {
    match market["priceInclusions"]["inclusiveTaxPricingStrategy"].as_str()? {
        "INCLUDES_TAXES_IN_PRICE" => Some(true),
        "ADD_TAXES_AT_CHECKOUT" => Some(false),
        _ => None,
    }
}

fn market_update_currency_settings_json(
    existing: Option<&Value>,
    input: &BTreeMap<String, ResolvedValue>,
    shop_currency_code: &str,
) -> Value {
    let currency_settings = resolved_object_field(input, "currencySettings").unwrap_or_default();
    let currency_code = resolved_string_field(&currency_settings, "baseCurrency")
        .or_else(|| value_string_field(existing, "baseCurrency", "currencyCode"))
        .unwrap_or_else(|| shop_currency_code.to_string());
    let currency_name = market_currency_name(&currency_code);
    json!({
        "baseCurrency": {
            "currencyCode": currency_code,
            "currencyName": currency_name
        },
        "localCurrencies": resolved_bool_field(&currency_settings, "localCurrencies")
            .or_else(|| value_bool_field(existing, "localCurrencies"))
            .unwrap_or(false),
        "roundingEnabled": resolved_bool_field(&currency_settings, "roundingEnabled")
            .or_else(|| value_bool_field(existing, "roundingEnabled"))
            .unwrap_or(false)
    })
}

fn market_update_price_inclusions_json(
    existing: Option<&Value>,
    input: &BTreeMap<String, ResolvedValue>,
) -> Value {
    let price_inclusions = resolved_object_field(input, "priceInclusions").unwrap_or_default();
    json!({
        "inclusiveDutiesPricingStrategy": resolved_string_field(&price_inclusions, "dutiesPricingStrategy")
            .or_else(|| value_string_field(existing, "inclusiveDutiesPricingStrategy", ""))
            .unwrap_or_else(|| "NOT_INCLUDED".to_string()),
        "inclusiveTaxPricingStrategy": resolved_string_field(&price_inclusions, "taxPricingStrategy")
            .or_else(|| value_string_field(existing, "inclusiveTaxPricingStrategy", ""))
            .unwrap_or_else(|| "ADD_TAXES_AT_CHECKOUT".to_string())
    })
}

fn market_update_region_input_present(input: &BTreeMap<String, ResolvedValue>) -> bool {
    if input.contains_key("regions") {
        return true;
    }
    let Some(ResolvedValue::Object(conditions)) = input.get("conditions") else {
        return false;
    };
    let Some(ResolvedValue::Object(regions_condition)) = conditions.get("regionsCondition") else {
        return false;
    };
    regions_condition.contains_key("regions")
}

fn value_string_field(existing: Option<&Value>, field: &str, nested_field: &str) -> Option<String> {
    let value = existing?.get(field)?;
    let value = if nested_field.is_empty() {
        value
    } else {
        value.get(nested_field)?
    };
    value.as_str().map(str::to_string)
}

fn value_bool_field(existing: Option<&Value>, field: &str) -> Option<bool> {
    existing?.get(field)?.as_bool()
}

fn market_record_legacy(market: &Value) -> bool {
    market
        .get("isLegacyMarket")
        .or_else(|| market.get("isLegacy"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// Region country codes declared by a market record, reading from the captured
/// `conditions.regionsCondition.regions` connection (nodes and/or edges). Supports both upstream-hydrated and mutation-staged market shapes.
fn market_record_country_codes(market: &Value) -> Vec<String> {
    let Some(regions) = market
        .get("conditions")
        .and_then(|conditions| conditions.get("regionsCondition"))
        .and_then(|regions_condition| regions_condition.get("regions"))
    else {
        return Vec::new();
    };
    let mut codes = Vec::new();
    if let Some(nodes) = regions.get("nodes").and_then(Value::as_array) {
        codes.extend(nodes.iter().filter_map(region_code_from_node));
    }
    if let Some(edges) = regions.get("edges").and_then(Value::as_array) {
        codes.extend(
            edges
                .iter()
                .filter_map(|edge| edge.get("node").and_then(region_code_from_node)),
        );
    }
    codes
}

fn region_code_from_node(node: &Value) -> Option<String> {
    node.get("code")
        .and_then(Value::as_str)
        .or_else(|| node.get("countryCode").and_then(Value::as_str))
        .map(str::to_string)
}

fn market_record_country_region(market: &Value, country_code: &str) -> Option<Value> {
    let regions = market
        .get("conditions")
        .and_then(|conditions| conditions.get("regionsCondition"))
        .and_then(|regions_condition| regions_condition.get("regions"))?;
    if let Some(region) = regions
        .get("nodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find_map(|node| market_region_country_from_node(node, country_code))
    {
        return Some(region);
    }
    regions
        .get("edges")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|edge| edge.get("node"))
        .find_map(|node| market_region_country_from_node(node, country_code))
}

pub(in crate::proxy) fn market_region_country_from_node(
    node: &Value,
    country_code: &str,
) -> Option<Value> {
    let code = region_code_from_node(node)?;
    if code.to_ascii_uppercase() != country_code {
        return None;
    }
    let mut region = node.as_object()?.clone();
    let id = node
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| shopify_gid("MarketRegionCountry", format!("local-{country_code}")));
    let name = node
        .get("name")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| country_name_for_code(&code).map(str::to_string))
        .unwrap_or_else(|| code.clone());
    region
        .entry("__typename".to_string())
        .or_insert_with(|| json!("MarketRegionCountry"));
    region.insert("id".to_string(), json!(id));
    region.insert("name".to_string(), json!(name));
    region.insert("code".to_string(), json!(code));
    Some(Value::Object(region))
}

fn localization_product_translatable_content(product: &ProductRecord, locale: &str) -> Vec<Value> {
    let mut content = vec![localization_content_entry(
        "title",
        &product.title,
        locale,
        "SINGLE_LINE_TEXT_FIELD",
    )];
    if !product.description_html.is_empty() {
        content.push(localization_content_entry(
            "body_html",
            &product.description_html,
            locale,
            "HTML",
        ));
    }
    content.push(localization_content_entry(
        "handle",
        &product.handle,
        locale,
        "URI",
    ));
    content.push(localization_content_entry(
        "product_type",
        &product.product_type,
        locale,
        "SINGLE_LINE_TEXT_FIELD",
    ));
    if !product.seo_title.is_empty() {
        content.push(localization_content_entry(
            "meta_title",
            &product.seo_title,
            locale,
            "MULTI_LINE_TEXT_FIELD",
        ));
    }
    if !product.seo_description.is_empty() {
        content.push(localization_content_entry(
            "meta_description",
            &product.seo_description,
            locale,
            "MULTI_LINE_TEXT_FIELD",
        ));
    }
    content
}

fn localization_collection_translatable_content(collection: &Value, locale: &str) -> Vec<Value> {
    let mut content = Vec::new();
    if let Some(title) = collection.get("title").and_then(Value::as_str) {
        content.push(localization_content_entry(
            "title",
            title,
            locale,
            "SINGLE_LINE_TEXT_FIELD",
        ));
    }
    if let Some(body) = collection
        .get("descriptionHtml")
        .or_else(|| collection.get("bodyHtml"))
        .and_then(Value::as_str)
    {
        content.push(localization_content_entry(
            "body_html",
            body,
            locale,
            "HTML",
        ));
    }
    if let Some(handle) = collection.get("handle").and_then(Value::as_str) {
        content.push(localization_content_entry("handle", handle, locale, "URI"));
    }
    if let Some(meta_title) = collection.pointer("/seo/title").and_then(Value::as_str) {
        content.push(localization_content_entry(
            "meta_title",
            meta_title,
            locale,
            "MULTI_LINE_TEXT_FIELD",
        ));
    }
    if let Some(meta_description) = collection
        .pointer("/seo/description")
        .and_then(Value::as_str)
    {
        content.push(localization_content_entry(
            "meta_description",
            meta_description,
            locale,
            "MULTI_LINE_TEXT_FIELD",
        ));
    }
    content
}

fn localization_content_entry(key: &str, value: &str, locale: &str, content_type: &str) -> Value {
    json!({
        "key": key,
        "value": value,
        "digest": localization_content_digest(value),
        "locale": locale,
        "type": content_type
    })
}

fn collection_set_seo_field(
    object: &mut serde_json::Map<String, Value>,
    field: &str,
    value: String,
) {
    let seo = object.entry("seo".to_string()).or_insert_with(|| json!({}));
    if !seo.is_object() {
        *seo = json!({});
    }
    if let Some(seo_object) = seo.as_object_mut() {
        seo_object.insert(field.to_string(), json!(value));
    }
}

pub(in crate::proxy) fn localization_resource_type_matches(
    resource_id: &str,
    resource_type: &str,
) -> bool {
    let Some(gid_type) = shopify_gid_resource_type(resource_id) else {
        return false;
    };
    gid_type.eq_ignore_ascii_case(&resource_type.replace('_', ""))
}

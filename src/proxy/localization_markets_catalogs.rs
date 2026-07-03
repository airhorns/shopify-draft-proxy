use super::market_unsupported_country_regions::is_unsupported_country_region;
use super::*;
use sha2::{Digest, Sha256};

mod web_presence_helpers;

pub(in crate::proxy) use self::web_presence_helpers::*;

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

fn market_relation_connection<'a>(
    records: impl Iterator<Item = &'a Value>,
    market_id: &str,
    market_ids: impl Fn(&Value) -> Vec<String>,
) -> Value {
    let nodes = records
        .filter(|record| market_ids(record).iter().any(|id| id == market_id))
        .cloned()
        .collect::<Vec<_>>();
    json!({"nodes": nodes})
}

/// Variant-level fixed-price mutations (`priceListFixedPricesAdd`/`Update`/`Delete`)
/// hydrate their baseline price-list/product/variant records through a real Admin
/// GraphQL preflight keyed by the mutation's price list and variant ids.
const FIXED_PRICE_VARIANT_PREFLIGHT_QUERY: &str = "query MarketsMutationPreflightHydrate($priceListId: ID!, $variantIds: [ID!]!) { priceList(id: $priceListId) { __typename id name currency fixedPricesCount prices(first: 20, originType: FIXED) { edges { cursor node { price { amount currencyCode } compareAtPrice { amount currencyCode } originType variant { id sku product { id title } } } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } productVariants: nodes(ids: $variantIds) { __typename ... on ProductVariant { id title sku price compareAtPrice product { id title handle status variants(first: 10) { nodes { id title sku price compareAtPrice } } } } } }";

/// `priceListFixedPricesByProductUpdate` hydrates from the real multi-product
/// preflight query (the canonical Admin GraphQL form recorded from live Shopify)
/// keyed on the de-duplicated product ids.
const FIXED_PRICE_BY_PRODUCT_PREFLIGHT_QUERY: &str = "query MarketsMutationPreflightHydrate($priceListId: ID!, $productIds: [ID!]!, $priceQuery: String) { priceList(id: $priceListId) { __typename id name currency fixedPricesCount prices(first: 10, query: $priceQuery, originType: FIXED) { edges { cursor node { price { amount currencyCode } compareAtPrice { amount currencyCode } originType variant { id sku product { id title } } } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } productNodes: nodes(ids: $productIds) { __typename ... on Product { id title handle status variants(first: 10) { nodes { id title sku price compareAtPrice } } } } }";

const CATALOG_RELATION_PRICE_LIST_PREFLIGHT_QUERY: &str = "query CatalogRelationPriceListHydrate($id: ID!) { priceList(id: $id) { __typename id name currency parent { adjustment { type value } } catalog { id } } }";

const CATALOG_RELATION_PUBLICATION_PREFLIGHT_QUERY: &str = "query CatalogRelationPublicationHydrate($id: ID!) { publication(id: $id) { __typename id name autoPublish } }";

/// Web-presence mutations (`webPresenceCreate`/`Update`/`Delete`) hydrate the
/// shop's baseline web presences from a real Admin GraphQL preflight before
/// applying the local mutation. The cassette stores the exact request Shopify saw,
/// so parity cannot hide behind a provenance descriptor string.
const WEB_PRESENCE_PREFLIGHT_QUERY: &str = "query MarketsMutationPreflightHydrate($first: Int!) { webPresences(first: $first) { nodes { id subfolderSuffix domain { id host url sslEnabled } rootUrls { locale url } defaultLocale { locale name primary published } alternateLocales { locale name primary published } markets(first: 5) { nodes { id name handle status type } } } } }";
const WEB_PRESENCE_PREFLIGHT_FIRST: i64 = 20;

/// Market-localization mutations (`marketLocalizationsRegister`/`Remove`) hydrate the
/// target resource's content/digests, the shop's markets, and existing localizations
/// for the target market from an exact Admin GraphQL preflight.
const MARKET_LOCALIZATION_PREFLIGHT_QUERY: &str = "query MarketsMutationPreflightHydrate($resourceId: ID!, $marketId: ID!, $marketsFirst: Int!) { marketLocalizableResource(resourceId: $resourceId) { resourceId marketLocalizableContent { key value digest } marketLocalizations(marketId: $marketId) { key value updatedAt outdated market { id name } } } markets(first: $marketsFirst) { nodes { id name handle status type } } }";
const MARKET_LOCALIZATION_PREFLIGHT_MARKETS_FIRST: i64 = 50;

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

    fn price_list_with_user_errors(
        field: &RootFieldSelection,
        price_list: Value,
        user_errors: Vec<Value>,
    ) -> Self {
        Self::payload(selected_json(
            &json!({"priceList": price_list, "userErrors": user_errors}),
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

fn staged_nodes_connection(
    records: &BTreeMap<String, Value>,
    selection: &[SelectedField],
    with_empty_edges: bool,
) -> Value {
    let nodes = records.values().cloned().collect::<Vec<_>>();
    let connection = if with_empty_edges {
        connection_json_with_empty_edges(nodes)
    } else {
        json!({ "nodes": nodes })
    };
    selected_json(&connection, selection)
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

fn selected_catalog_payload(
    field: &RootFieldSelection,
    catalog: Value,
    user_errors: Vec<Value>,
) -> Value {
    selected_json(
        &json!({"catalog": catalog, "userErrors": user_errors}),
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

fn selected_market_payload(
    field: &RootFieldSelection,
    market: Value,
    user_errors: Vec<Value>,
) -> Value {
    selected_json(
        &json!({"market": market, "userErrors": user_errors}),
        &field.selection,
    )
}

fn selected_market_user_errors(field: &RootFieldSelection, user_errors: Vec<Value>) -> Value {
    selected_market_payload(field, Value::Null, user_errors)
}

fn selected_market_error(
    field: &RootFieldSelection,
    path: Vec<&str>,
    message: &str,
    code: Value,
) -> Value {
    selected_market_user_errors(field, vec![market_user_error(path, message, code)])
}

impl DraftProxy {
    pub(in crate::proxy) fn localization_query_data(
        &mut self,
        fields: &[RootFieldSelection],
        request: &Request,
    ) -> Value {
        root_payload_json(fields, |field| {
            Some(match field.name.as_str() {
                "availableLocales" => Value::Array(
                    self.localization_available_locales()
                        .iter()
                        .map(|locale| selected_json(locale, &field.selection))
                        .collect(),
                ),
                "shopLocales" => {
                    let published_filter = resolved_bool_field(&field.arguments, "published");
                    Value::Array(
                        self.localization_shop_locales(published_filter)
                            .iter()
                            .map(|locale| selected_json(locale, &field.selection))
                            .collect(),
                    )
                }
                "translatableResource" => {
                    let resource_id = resolved_string_field(&field.arguments, "resourceId")
                        .unwrap_or_else(|| "gid://shopify/Product/9801098789170".to_string());
                    if !self.localization_translatable_resource_exists(&resource_id) {
                        Value::Null
                    } else {
                        self.localization_translatable_resource_selected(
                            &resource_id,
                            &field.selection,
                        )
                    }
                }
                "translatableResources" => {
                    self.localization_translatable_resources_connection(field)
                }
                "translatableResourcesByIds" => {
                    self.localization_translatable_resources_by_ids_connection(field)
                }
                "markets" => self.localization_markets_connection(field, request),
                _ => Value::Null,
            })
        })
    }

    pub(in crate::proxy) fn localization_mutation_data(
        &mut self,
        fields: &[RootFieldSelection],
    ) -> Value {
        root_payload_json(fields, |field| {
            Some(match field.name.as_str() {
                "shopLocaleEnable" => self.shop_locale_enable_response(field),
                "shopLocaleUpdate" => self.shop_locale_update_response(field),
                "shopLocaleDisable" => self.shop_locale_disable_response(field),
                "translationsRegister" => self.localization_register_response(field),
                "translationsRemove" => self.localization_remove_response(field),
                _ => Value::Null,
            })
        })
    }

    pub(in crate::proxy) fn localization_mutation_preflight(
        &mut self,
        fields: &[RootFieldSelection],
        request: &Request,
    ) {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return;
        }
        let ids = self
            .localization_mutation_target_ids(fields)
            .into_iter()
            .filter(|id| {
                (id.starts_with("gid://shopify/Market/") && !self.market_exists(id))
                    || (id.starts_with("gid://shopify/MarketWebPresence/")
                        && !self.market_web_presence_exists(id))
            })
            .collect::<Vec<_>>();
        if ids.is_empty() {
            return;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": LOCALIZATION_MUTATION_TARGETS_HYDRATE_QUERY,
                "operationName": "LocalizationMutationTargetsHydrate",
                "variables": { "ids": ids }
            }),
        );
        if response.status < 400 {
            self.hydrate_markets_from_upstream(&response.body);
        }
    }

    fn localization_mutation_target_ids(&self, fields: &[RootFieldSelection]) -> Vec<String> {
        let mut ids = Vec::new();
        for field in fields {
            match field.name.as_str() {
                "shopLocaleEnable" => {
                    ids.extend(resolved_string_list_arg(
                        &field.arguments,
                        "marketWebPresenceIds",
                    ));
                }
                "shopLocaleUpdate" => {
                    let input =
                        resolved_object_field(&field.arguments, "shopLocale").unwrap_or_default();
                    ids.extend(resolved_string_list_field_unsorted(
                        &input,
                        "marketWebPresenceIds",
                    ));
                }
                "translationsRegister" => {
                    for translation in resolved_list_arg(&field.arguments, "translations") {
                        if let Some(market_id) = resolved_object_string(&translation, "marketId") {
                            ids.push(market_id);
                        }
                    }
                }
                "translationsRemove" => {
                    ids.extend(resolved_string_list_arg(&field.arguments, "marketIds"));
                }
                _ => {}
            }
        }
        ids.sort();
        ids.dedup();
        ids
    }

    pub(in crate::proxy) fn localization_available_locales(&self) -> Vec<Value> {
        self.store
            .base
            .available_locales
            .iter()
            .map(|(iso_code, name)| {
                json!({
                    "isoCode": iso_code,
                    "name": name
                })
            })
            .collect()
    }

    pub(in crate::proxy) fn localization_available_locale_name(
        &self,
        locale: &str,
    ) -> Option<&str> {
        self.store
            .base
            .available_locales
            .get(locale)
            .map(String::as_str)
    }

    pub(in crate::proxy) fn localization_shop_locales(
        &self,
        published_filter: Option<bool>,
    ) -> Vec<Value> {
        let mut by_code: BTreeMap<String, Value> = BTreeMap::new();
        for locale in self.store.base.shop_locales.values() {
            if let Some(code) = locale["locale"].as_str() {
                by_code.insert(code.to_string(), locale.clone());
            }
        }
        for locale in self.store.staged.shop_locales.values() {
            if let Some(code) = locale["locale"].as_str() {
                by_code.insert(code.to_string(), locale.clone());
            }
        }
        let mut locales = by_code.into_values().collect::<Vec<_>>();
        locales.sort_by_key(|locale| locale["locale"].as_str().unwrap_or_default().to_string());
        if let Some(published) = published_filter {
            locales.retain(|locale| locale["published"].as_bool() == Some(published));
        }
        locales
    }

    pub(in crate::proxy) fn shop_locale_enable_response(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let locale =
            resolved_string_field(&field.arguments, "locale").unwrap_or_else(|| "fr".to_string());
        let payload = if locale == "en" {
            json!({
                "shopLocale": null,
                "userErrors": [shop_locale_user_error(vec!["locale"], "The primary locale of your store can't be changed through this endpoint.")]
            })
        } else if self.localization_available_locale_name(&locale).is_none() {
            json!({
                "shopLocale": null,
                "userErrors": [shop_locale_user_error(vec!["locale"], "Locale is invalid")]
            })
        } else if self.store.staged.shop_locales.contains_key(&locale) {
            json!({
                "shopLocale": null,
                "userErrors": [shop_locale_user_error(vec!["locale"], "Locale has already been taken")]
            })
        } else if self
            .localization_shop_locales(None)
            .iter()
            .filter(|locale| !locale["primary"].as_bool().unwrap_or(false))
            .count()
            >= 20
        {
            json!({
                "shopLocale": null,
                "userErrors": [user_error_omit_code(Value::Null, &format!(
                        "Your store has reached its 20 language limit. To add {}, delete one of your other languages.",
                        self.localization_available_locale_name(&locale).unwrap_or(locale.as_str())
                    ), None)]
            })
        } else {
            let name = self
                .localization_available_locale_name(&locale)
                .unwrap_or(locale.as_str());
            let mut record = shop_locale_record(&locale, name, false);
            let target_web_presence_ids = self.known_market_web_presence_ids(
                resolved_string_list_arg(&field.arguments, "marketWebPresenceIds"),
            );
            record["marketWebPresences"] = Value::Array(
                target_web_presence_ids
                    .iter()
                    .map(|id| shop_locale_market_web_presence_record(id))
                    .collect(),
            );
            self.store
                .staged
                .shop_locales
                .insert(locale.clone(), record.clone());
            self.sync_web_presence_locales(&locale, &target_web_presence_ids, false);
            json!({ "shopLocale": record, "userErrors": [] })
        };
        selected_json(&payload, &field.selection)
    }

    pub(in crate::proxy) fn shop_locale_update_response(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let locale =
            resolved_string_field(&field.arguments, "locale").unwrap_or_else(|| "fr".to_string());
        let input = resolved_object_field(&field.arguments, "shopLocale").unwrap_or_default();
        let published = resolved_bool_field(&input, "published");
        let market_web_presence_ids = list_string_field(&input, "marketWebPresenceIds");

        if locale == "en" && published.is_some() {
            return selected_json(
                &json!({
                    "shopLocale": null,
                    "userErrors": [shop_locale_user_error(vec!["locale"], "The primary locale of your store can't be changed through this endpoint.")]
                }),
                &field.selection,
            );
        }

        let locale_exists = locale == "en" || self.store.staged.shop_locales.contains_key(&locale);
        if !locale_exists && published.is_some() {
            return selected_json(
                &json!({
                    "shopLocale": null,
                    "userErrors": [shop_locale_user_error(vec!["locale"], "The locale doesn't exist.")]
                }),
                &field.selection,
            );
        }

        let mut record = self
            .store
            .staged
            .shop_locales
            .get(&locale)
            .cloned()
            .unwrap_or_else(|| {
                let name = self
                    .localization_available_locale_name(&locale)
                    .unwrap_or(locale.as_str());
                shop_locale_record(&locale, name, false)
            });
        if let Some(published) = published {
            record["published"] = json!(published);
        }
        if input.contains_key("marketWebPresenceIds") {
            let target_web_presence_ids =
                self.known_market_web_presence_ids(market_web_presence_ids);
            record["marketWebPresences"] = Value::Array(
                target_web_presence_ids
                    .iter()
                    .map(|id| shop_locale_market_web_presence_record(id))
                    .collect(),
            );
            self.sync_web_presence_locales(&locale, &target_web_presence_ids, true);
        }
        if locale != "en" {
            self.store
                .staged
                .shop_locales
                .insert(locale, record.clone());
        }
        selected_json(
            &json!({ "shopLocale": record, "userErrors": [] }),
            &field.selection,
        )
    }

    fn known_market_web_presence_ids(&self, ids: Vec<String>) -> Vec<String> {
        ids.into_iter()
            .filter(|id| self.market_web_presence_exists(id))
            .collect()
    }

    fn market_web_presence_exists(&self, id: &str) -> bool {
        self.store.staged.web_presences.contains_key(id)
            || self.localization_shop_locales(None).iter().any(|locale| {
                locale["marketWebPresences"]
                    .as_array()
                    .is_some_and(|presences| {
                        presences
                            .iter()
                            .any(|presence| presence["id"].as_str() == Some(id))
                    })
            })
    }

    pub(in crate::proxy) fn shop_locale_disable_response(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let locale =
            resolved_string_field(&field.arguments, "locale").unwrap_or_else(|| "fr".to_string());
        let payload = if locale == "en" {
            json!({
                "locale": null,
                "userErrors": [shop_locale_user_error(vec!["locale"], "The primary locale of your store can't be changed through this endpoint.")]
            })
        } else if !self.store.staged.shop_locales.contains_key(&locale) {
            json!({
                "locale": null,
                "userErrors": [shop_locale_user_error(vec!["locale"], "The locale doesn't exist.")]
            })
        } else {
            self.store.staged.shop_locales.remove(&locale);
            self.store
                .staged
                .localization_translations
                .retain(|translation| translation["locale"] != json!(locale));
            self.store.staged.localization_dirty = true;
            json!({ "locale": locale, "userErrors": [] })
        };
        selected_json(&payload, &field.selection)
    }

    /// Unified Markets overlay read. A single GraphQL query can select several
    /// markets-domain root fields at once (e.g. the delete-cascade downstream
    /// read selects `webPresences`, `market`, and `catalog` together). Routing
    /// the whole operation to one entity-specific handler would null every field
    /// that handler doesn't own, so each root field is projected independently
    /// from its staged store here.
    pub(in crate::proxy) fn markets_overlay_query_data(
        &self,
        fields: &[RootFieldSelection],
    ) -> Value {
        root_payload_json(fields, |field| {
            Some(match field.name.as_str() {
                "market" => {
                    let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                    self.store
                        .staged
                        .markets
                        .get(&id)
                        .map(|market| selected_json(market, &field.selection))
                        .unwrap_or(Value::Null)
                }
                "catalog" => {
                    let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                    self.store
                        .staged
                        .catalogs
                        .get(&id)
                        .map(|catalog| selected_json(catalog, &field.selection))
                        .unwrap_or(Value::Null)
                }
                "catalogs" => {
                    staged_nodes_connection(&self.store.staged.catalogs, &field.selection, false)
                }
                "catalogsCount" => self.catalogs_count_value(field),
                "priceList" => {
                    let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                    self.store
                        .staged
                        .price_lists
                        .get(&id)
                        .map(|price_list| selected_json(price_list, &field.selection))
                        .unwrap_or(Value::Null)
                }
                "priceLists" => {
                    staged_nodes_connection(&self.store.staged.price_lists, &field.selection, false)
                }
                "webPresences" => staged_nodes_connection(
                    &self.store.staged.web_presences,
                    &field.selection,
                    true,
                ),
                "marketsResolvedValues" => self.markets_resolved_values_value(field),
                "marketLocalizableResources" | "marketLocalizableResourcesByIds" => selected_json(
                    &connection_json_with_empty_edges(Vec::new()),
                    &field.selection,
                ),
                // The `markets` plural connection projects the staged markets store.
                // Hydration from upstream happens in the LiveHybrid fetch path before
                // this handler is reached, so here we only serve what is already
                // staged — an empty connection (not a fabricated node) when a backend
                // has no markets.
                "markets" => {
                    let records = self
                        .store
                        .staged
                        .markets
                        .values()
                        .cloned()
                        .collect::<Vec<_>>();
                    selected_typed_connection_with_args(
                        &records,
                        &field.arguments,
                        &field.selection,
                        selected_json,
                        value_id_cursor,
                    )
                }
                _ => Value::Null,
            })
        })
    }

    fn catalogs_count_value(&self, field: &RootFieldSelection) -> Value {
        selected_count_json(self.store.staged.catalogs.len(), &field.selection)
    }

    fn markets_resolved_values_value(&self, field: &RootFieldSelection) -> Value {
        let mut payload = serde_json::Map::new();
        for selection in &field.selection {
            let value = match selection.name.as_str() {
                "currencyCode" => Some(json!(self.store.shop_currency_code())),
                "priceInclusivity" => Some(selected_json(
                    &json!({
                        "dutiesIncluded": false,
                        "taxesIncluded": false
                    }),
                    &selection.selection,
                )),
                "catalogs" => {
                    let records = self
                        .store
                        .staged
                        .catalogs
                        .values()
                        .cloned()
                        .collect::<Vec<_>>();
                    Some(selected_typed_connection_with_args(
                        &records,
                        &selection.arguments,
                        &selection.selection,
                        selected_json,
                        value_id_cursor,
                    ))
                }
                "webPresences" => {
                    let records = self
                        .store
                        .staged
                        .web_presences
                        .values()
                        .cloned()
                        .collect::<Vec<_>>();
                    Some(selected_typed_connection_with_args(
                        &records,
                        &selection.arguments,
                        &selection.selection,
                        selected_json,
                        value_id_cursor,
                    ))
                }
                _ => None,
            };
            if let Some(value) = value {
                payload.insert(selection.response_key.clone(), value);
            }
        }
        Value::Object(payload)
    }

    pub(in crate::proxy) fn market_create_mutation_data(
        &mut self,
        fields: &[RootFieldSelection],
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let mut staged_ids = Vec::new();
        let mut log_root_field: Option<String> = None;
        let data = root_payload_json(fields, |field| {
            let value = match field.name.as_str() {
                "marketCreate" => self.market_create_response(field),
                "marketUpdate" => self.market_update_response(field),
                "marketDelete" => self.market_delete_response(field),
                _ => Value::Null,
            };
            if let Some(id) = value["market"]["id"]
                .as_str()
                .or_else(|| value["deletedId"].as_str())
            {
                staged_ids.push(id.to_string());
                if log_root_field.is_none() {
                    log_root_field = Some(field.name.clone());
                }
            }
            Some(value)
        });
        if !staged_ids.is_empty() {
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                log_root_field.as_deref().unwrap_or("marketCreate"),
                staged_ids,
            );
        }
        data
    }

    pub(in crate::proxy) fn market_create_response(&mut self, field: &RootFieldSelection) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        if market_status_enabled_mismatch(&input) {
            return selected_market_error(
                field,
                vec!["input"],
                "Invalid status and enabled combination.",
                json!("INVALID_STATUS_AND_ENABLED_COMBINATION"),
            );
        }
        if market_has_location_price_inclusion_conflict(&input) {
            return selected_market_error(
                field,
                vec!["input", "priceInclusions"],
                "Inclusive pricing cannot be added to a market with the specified condition types.",
                json!("INCLUSIVE_PRICING_NOT_COMPATIBLE_WITH_CONDITION_TYPES"),
            );
        }
        if market_currency_settings(&input)
            .and_then(|settings| resolved_number_field(&settings, "baseCurrencyManualRate"))
            .is_some_and(|rate| rate <= 0.0)
        {
            return selected_market_error(
                field,
                vec!["input", "currencySettings", "baseCurrencyManualRate"],
                "Enter a rate above 0.",
                Value::Null,
            );
        }
        let region_codes = market_region_country_codes(&input);
        if let Some((index, country_code)) = region_codes
            .iter()
            .enumerate()
            .find(|(_, country_code)| is_unsupported_country_region(country_code))
        {
            return selected_market_error(
                field,
                vec!["input", "regions", &index.to_string(), "countryCode"],
                &format!("{country_code} is not a supported country or region code."),
                json!("UNSUPPORTED_COUNTRY_REGION"),
            );
        }
        if let Some((index, _country_code)) = region_codes
            .iter()
            .enumerate()
            .find(|(_, country_code)| self.market_region_code_exists(country_code))
        {
            return selected_market_error(
                field,
                vec!["input", "regions", &index.to_string(), "countryCode"],
                "Code has already been taken",
                json!("TAKEN"),
            );
        }

        let name = resolved_string_field(&input, "name").unwrap_or_default();
        let mut name_errors = Vec::new();
        if name.is_empty() {
            name_errors.push(market_user_error(
                vec!["input", "name"],
                "Name can't be blank",
                json!("BLANK"),
            ));
        }
        if name.chars().count() < 2 {
            name_errors.push(market_user_error(
                vec!["input", "name"],
                "Name is too short (minimum is 2 characters)",
                json!("TOO_SHORT"),
            ));
        }
        if !name_errors.is_empty() {
            return selected_market_user_errors(field, name_errors);
        }
        if self.store.staged.markets.values().any(|market| {
            market["name"]
                .as_str()
                .is_some_and(|existing_name| existing_name.eq_ignore_ascii_case(&name))
        }) {
            return selected_market_error(
                field,
                vec!["input", "name"],
                "Name has already been taken",
                json!("TAKEN"),
            );
        }

        let explicit_handle = resolved_string_field(&input, "handle");
        let mut handle = normalize_localized_handle(explicit_handle.as_deref().unwrap_or(&name));
        let existing_handles = self
            .store
            .staged
            .markets
            .values()
            .filter_map(|market| market["handle"].as_str())
            .map(ToString::to_string)
            .collect::<BTreeSet<_>>();
        if explicit_handle.is_some() && existing_handles.contains(&handle) {
            return selected_market_error(
                field,
                vec!["input", "handle"],
                "Generated handle has already been taken",
                json!("GENERATED_DUPLICATED_HANDLE"),
            );
        }
        if explicit_handle.is_none() {
            let base_handle = handle.clone();
            let mut suffix = 1;
            while existing_handles.contains(&handle) {
                handle = format!("{base_handle}-{suffix}");
                suffix += 1;
            }
        }

        let id = shopify_gid("Market", self.store.staged.markets.len() + 1);
        let market = market_record_from_input(&id, &input, &name, &handle, &region_codes);
        self.store.staged.markets.insert(id, market.clone());
        selected_market_payload(field, market, Vec::new())
    }

    pub(in crate::proxy) fn market_delete_response(&mut self, field: &RootFieldSelection) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let payload = if self.store.staged.markets.remove(&id).is_some() {
            self.cascade_market_delete(&id);
            json!({"deletedId": id, "userErrors": []})
        } else {
            json!({
                "deletedId": null,
                "userErrors": [market_user_error(vec!["id"], "Market does not exist", json!("MARKET_NOT_FOUND"))]
            })
        };
        selected_json(&payload, &field.selection)
    }

    pub(in crate::proxy) fn cascade_market_delete(&mut self, market_id: &str) {
        self.store.staged.web_presences.retain(|_, web_presence| {
            !web_presence_market_ids(web_presence)
                .iter()
                .any(|id| id == market_id)
        });
        let market_names = self.staged_market_names();
        for catalog in self.store.staged.catalogs.values_mut() {
            let mut market_ids = catalog_market_ids(catalog);
            market_ids.retain(|id| id != market_id);
            set_catalog_market_ids(catalog, &market_ids, &market_names);
        }
        self.store
            .staged
            .localization_translations
            .retain(|translation| {
                translation["market"]["id"].as_str() != Some(market_id)
                    && translation["marketId"].as_str() != Some(market_id)
            });
    }

    pub(in crate::proxy) fn market_update_response(&mut self, field: &RootFieldSelection) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let Some(existing_market) = self.store.staged.markets.get(&id).cloned() else {
            return selected_market_error(
                field,
                vec!["id"],
                "Market does not exist",
                json!("MARKET_NOT_FOUND"),
            );
        };
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();

        let catalogs_to_add = list_string_field(&input, "catalogsToAdd");
        let missing_catalogs = catalogs_to_add
            .iter()
            .filter(|catalog_id| !self.store.staged.catalogs.contains_key(*catalog_id))
            .cloned()
            .collect::<Vec<_>>();
        if !missing_catalogs.is_empty() {
            return selected_market_error(
                field,
                vec!["input", "catalogsToAdd"],
                &missing_customization_message(&missing_catalogs),
                json!("CUSTOMIZATIONS_NOT_FOUND"),
            );
        }

        let web_presences_to_add = list_string_field(&input, "webPresencesToAdd");
        let missing_web_presences = web_presences_to_add
            .iter()
            .filter(|web_presence_id| {
                !self
                    .store
                    .staged
                    .web_presences
                    .contains_key(*web_presence_id)
            })
            .cloned()
            .collect::<Vec<_>>();
        if !missing_web_presences.is_empty() {
            return selected_market_error(
                field,
                vec!["input", "webPresencesToAdd"],
                &missing_customization_message(&missing_web_presences),
                json!("CUSTOMIZATIONS_NOT_FOUND"),
            );
        }

        for catalog_id in catalogs_to_add {
            self.add_market_to_catalog(&catalog_id, &id);
        }
        for catalog_id in list_string_field(&input, "catalogsToDelete") {
            self.remove_market_from_catalog(&catalog_id, &id);
        }
        for web_presence_id in web_presences_to_add {
            self.add_market_to_web_presence(&web_presence_id, &id);
        }
        for web_presence_id in list_string_field(&input, "webPresencesToDelete") {
            self.remove_market_from_web_presence(&web_presence_id, &id);
        }

        let mut updated_market = existing_market;
        Self::apply_market_update_scalar_fields(&mut updated_market, &input, &id);
        self.set_market_relation_fields(&mut updated_market, &id);
        self.store.staged.markets.insert(id, updated_market.clone());
        selected_market_payload(field, updated_market, Vec::new())
    }

    fn apply_market_update_scalar_fields(
        market: &mut Value,
        input: &BTreeMap<String, ResolvedValue>,
        market_id: &str,
    ) {
        let Some(object) = market.as_object_mut() else {
            return;
        };

        if let Some(name) = resolved_string_field(input, "name") {
            object.insert("name".to_string(), json!(name));
        }
        if let Some(handle) = resolved_string_field(input, "handle") {
            object.insert(
                "handle".to_string(),
                json!(normalize_localized_handle(&handle)),
            );
        }

        let status_input = resolved_string_field(input, "status");
        let enabled_input = resolved_bool_field(input, "enabled");
        match (status_input, enabled_input) {
            (Some(status), Some(enabled)) => {
                object.insert("status".to_string(), json!(status));
                object.insert("enabled".to_string(), json!(enabled));
            }
            (Some(status), None) => {
                let enabled = status == "ACTIVE";
                object.insert("status".to_string(), json!(status));
                object.insert("enabled".to_string(), json!(enabled));
            }
            (None, Some(enabled)) => {
                let status = if enabled { "ACTIVE" } else { "DRAFT" };
                object.insert("status".to_string(), json!(status));
                object.insert("enabled".to_string(), json!(enabled));
            }
            (None, None) => {}
        }

        if matches!(
            input.get("currencySettings"),
            Some(ResolvedValue::Object(_))
        ) {
            let currency_settings =
                market_update_currency_settings_json(object.get("currencySettings"), input);
            object.insert("currencySettings".to_string(), currency_settings);
        }
        if matches!(input.get("priceInclusions"), Some(ResolvedValue::Object(_))) {
            let price_inclusions =
                market_update_price_inclusions_json(object.get("priceInclusions"), input);
            object.insert("priceInclusions".to_string(), price_inclusions);
        }
        if market_update_region_input_present(input) {
            let region_codes = market_region_country_codes(input);
            let region_nodes = market_region_country_nodes(market_id, &region_codes);
            object.insert("regionCodes".to_string(), json!(region_codes));
            object.insert(
                "type".to_string(),
                json!(if region_nodes.is_empty() {
                    "NONE"
                } else {
                    "REGION"
                }),
            );
            object.insert(
                "conditions".to_string(),
                json!({
                    "regionsCondition": {
                        "regions": {
                            "nodes": region_nodes
                        }
                    }
                }),
            );
        }
    }

    pub(in crate::proxy) fn set_market_relation_fields(&self, market: &mut Value, market_id: &str) {
        if let Some(object) = market.as_object_mut() {
            object.insert(
                "catalogs".to_string(),
                self.market_catalogs_connection(market_id),
            );
            object.insert(
                "webPresences".to_string(),
                self.market_web_presences_connection(market_id),
            );
        }
    }

    pub(in crate::proxy) fn market_catalogs_connection(&self, market_id: &str) -> Value {
        market_relation_connection(
            self.store.staged.catalogs.values(),
            market_id,
            catalog_market_ids,
        )
    }

    pub(in crate::proxy) fn market_web_presences_connection(&self, market_id: &str) -> Value {
        market_relation_connection(
            self.store.staged.web_presences.values(),
            market_id,
            web_presence_market_ids,
        )
    }

    pub(in crate::proxy) fn add_market_to_catalog(&mut self, catalog_id: &str, market_id: &str) {
        let market_names = self.staged_market_names();
        if let Some(catalog) = self.store.staged.catalogs.get_mut(catalog_id) {
            let mut market_ids = catalog_market_ids(catalog);
            if !market_ids.iter().any(|id| id == market_id) {
                market_ids.push(market_id.to_string());
                set_catalog_market_ids(catalog, &market_ids, &market_names);
            }
        }
    }

    pub(in crate::proxy) fn remove_market_from_catalog(
        &mut self,
        catalog_id: &str,
        market_id: &str,
    ) {
        let market_names = self.staged_market_names();
        if let Some(catalog) = self.store.staged.catalogs.get_mut(catalog_id) {
            let mut market_ids = catalog_market_ids(catalog);
            market_ids.retain(|id| id != market_id);
            set_catalog_market_ids(catalog, &market_ids, &market_names);
        }
    }

    pub(in crate::proxy) fn add_market_to_web_presence(
        &mut self,
        web_presence_id: &str,
        market_id: &str,
    ) {
        if let Some(web_presence) = self.store.staged.web_presences.get_mut(web_presence_id) {
            let mut market_ids = web_presence_market_ids(web_presence);
            if !market_ids.iter().any(|id| id == market_id) {
                market_ids.push(market_id.to_string());
                set_web_presence_market_ids(web_presence, &market_ids);
            }
        }
    }

    pub(in crate::proxy) fn remove_market_from_web_presence(
        &mut self,
        web_presence_id: &str,
        market_id: &str,
    ) {
        if let Some(web_presence) = self.store.staged.web_presences.get_mut(web_presence_id) {
            let mut market_ids = web_presence_market_ids(web_presence);
            market_ids.retain(|id| id != market_id);
            set_web_presence_market_ids(web_presence, &market_ids);
        }
    }

    pub(in crate::proxy) fn market_region_code_exists(&self, country_code: &str) -> bool {
        self.store.staged.markets.values().any(|market| {
            market["regionCodes"]
                .as_array()
                .is_some_and(|codes| codes.iter().any(|code| code.as_str() == Some(country_code)))
        })
    }

    pub(in crate::proxy) fn market_exists(&self, market_id: &str) -> bool {
        self.store.staged.markets.contains_key(market_id)
    }

    /// Snapshot of every staged market's id -> name. Used to denormalize names
    /// into a catalog's `markets` connection nodes, which are projected directly
    /// from the stored catalog by `selected_json`. Resolving from the live market
    /// registry (rather than fabricating) keeps the connection faithful to the
    /// markets the backend actually has.
    pub(in crate::proxy) fn staged_market_names(&self) -> BTreeMap<String, String> {
        self.store
            .staged
            .markets
            .iter()
            .filter_map(|(id, market)| {
                market["name"]
                    .as_str()
                    .map(|name| (id.clone(), name.to_string()))
            })
            .collect()
    }

    /// Resolve the given country from active, non-legacy REGION-type market
    /// data. There is no captured per-shop fallback; callers hydrate real
    /// market data first when running outside snapshot mode.
    pub(in crate::proxy) fn backup_region_country_for_code(
        &self,
        country_code: &str,
    ) -> Option<Value> {
        let normalized = country_code.to_ascii_uppercase();
        self.store
            .staged
            .markets
            .values()
            .filter(|market| market_record_is_active_region_non_legacy(market))
            .find_map(|market| market_record_country_region(market, &normalized))
    }

    pub(in crate::proxy) fn hydrate_backup_region_markets_from_upstream(
        &mut self,
        request: &Request,
    ) -> Response {
        let response = self.upstream_post(
            request,
            json!({
                "query": BACKUP_REGION_MARKETS_HYDRATE_QUERY,
                "operationName": "BackupRegionMarketsHydrate",
                "variables": { "first": 250, "regionsFirst": 250 }
            }),
        );
        if response.status < 400 {
            self.hydrate_markets_from_upstream(&response.body);
        }
        response
    }

    pub(in crate::proxy) fn catalog_mutation_data(
        &mut self,
        fields: &[RootFieldSelection],
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let mut touched_ids = Vec::new();
        let data = root_payload_json(fields, |field| {
            let value = match field.name.as_str() {
                "catalogCreate" => self.catalog_create_response(field, request),
                "catalogUpdate" => self.catalog_update_response(field, request),
                "catalogDelete" => self.catalog_delete_response(field),
                "catalogContextUpdate" => self.catalog_context_update_response(field),
                _ => Value::Null,
            };
            if let Some(id) = value["catalog"]["id"]
                .as_str()
                .or_else(|| value["deletedId"].as_str())
            {
                touched_ids.push(id.to_string());
            }
            Some(value)
        });
        if !touched_ids.is_empty() {
            self.record_mutation_log_entry(request, query, variables, "catalog", touched_ids);
        }
        data
    }

    pub(in crate::proxy) fn catalog_create_response(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
    ) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        if resolved_string_field(&input, "title").is_some_and(|title| title.trim().is_empty()) {
            return selected_catalog_error(
                field,
                vec!["input", "title"],
                "Title can't be blank",
                "BLANK",
            );
        }
        let Some(status) = resolved_string_field(&input, "status") else {
            return selected_catalog_error(
                field,
                vec!["input", "status"],
                "Status is required",
                "REQUIRED",
            );
        };
        if !matches!(status.as_str(), "ACTIVE" | "DRAFT") {
            return selected_catalog_error(
                field,
                vec!["input", "status"],
                "Status is invalid",
                "INVALID",
            );
        }
        let Some(context) = resolved_object_field(&input, "context") else {
            return selected_catalog_error(
                field,
                vec!["input", "context"],
                "Context is required",
                "INVALID",
            );
        };
        let context_type_fields = catalog_context_type_fields(&context);
        if context_type_fields.len() != 1 {
            return selected_catalog_error(
                field,
                vec!["input", "context"],
                "Must provide exactly one context type.",
                "MUST_PROVIDE_EXACTLY_ONE_CONTEXT_TYPE",
            );
        }
        let driver_type = context_type_fields[0].0;
        let market_ids = list_string_field(&context, "marketIds");
        let company_location_ids = company_location_ids_from_context(&context);
        let country_codes = country_codes_from_context(&context);
        match driver_type {
            CatalogContextDriver::Market => {
                if market_ids.is_empty() {
                    return selected_catalog_error(
                        field,
                        vec!["input", "context", "marketIds"],
                        "Market ids can't be blank",
                        "INVALID",
                    );
                }
                for (index, market_id) in market_ids.iter().enumerate() {
                    if !self.market_exists(market_id) {
                        return selected_catalog_error(
                            field,
                            vec!["input", "context", "marketIds", &index.to_string()],
                            "Market not found.",
                            "MARKET_NOT_FOUND",
                        );
                    }
                }
            }
            CatalogContextDriver::CompanyLocation => {
                if company_location_ids.is_empty() {
                    return selected_catalog_error(
                        field,
                        vec!["input", "context", "companyLocationIds"],
                        "Company location ids can't be blank",
                        "INVALID",
                    );
                }
                for (field_name, ids) in [
                    (
                        "companyLocationIds",
                        list_string_field(&context, "companyLocationIds"),
                    ),
                    ("locationIds", list_string_field(&context, "locationIds")),
                ] {
                    for (index, location_id) in ids.iter().enumerate() {
                        if !self.store.staged.b2b_locations.contains_key(location_id) {
                            return selected_catalog_error(
                                field,
                                vec!["input", "context", field_name, &index.to_string()],
                                COMPANY_LOCATION_NOT_FOUND_MESSAGE,
                                "COMPANY_LOCATION_NOT_FOUND",
                            );
                        }
                    }
                }
            }
            CatalogContextDriver::Country => {
                if country_codes.is_empty() {
                    return selected_catalog_error(
                        field,
                        vec!["input", "context", "countryCodes"],
                        "Country codes can't be blank",
                        "INVALID",
                    );
                }
            }
        }
        let price_list_id = resolved_string_field(&input, "priceListId");
        if let Some(price_list_id) = price_list_id.as_deref() {
            self.catalog_relation_price_list_preflight(request, price_list_id);
            if !self.catalog_relation_price_list_exists(price_list_id) {
                return selected_catalog_error(
                    field,
                    vec!["input", "priceListId"],
                    "Price list not found.",
                    "PRICE_LIST_NOT_FOUND",
                );
            }
            if self.catalog_price_list_taken(price_list_id, None) {
                return selected_catalog_error(
                    field,
                    vec!["input", "priceListId"],
                    "Price list has already been taken",
                    "TAKEN",
                );
            }
        }
        let publication_id = resolved_string_field(&input, "publicationId");
        if let Some(publication_id) = publication_id.as_deref() {
            self.catalog_relation_publication_preflight(request, publication_id);
            if !self.catalog_relation_publication_exists(publication_id) {
                return selected_catalog_error(
                    field,
                    vec!["input", "publicationId"],
                    "Publication not found.",
                    "PUBLICATION_NOT_FOUND",
                );
            }
            if self.catalog_publication_taken(publication_id, None) {
                return selected_catalog_error(
                    field,
                    vec!["input", "publicationId"],
                    "Publication is already attached to another catalog",
                    "PUBLICATION_TAKEN",
                );
            }
        }

        let id = self.next_catalog_id(driver_type);
        let title = resolved_string_field(&input, "title").unwrap_or_default();
        let mut catalog = match driver_type {
            CatalogContextDriver::Market => {
                let market_names = self.staged_market_names();
                catalog_record(&id, &title, &status, &market_ids, &market_names)
            }
            CatalogContextDriver::CompanyLocation => company_location_catalog_record(
                &id,
                &title,
                &status,
                &company_location_ids,
                &self.staged_company_locations_for_catalog(),
            ),
            CatalogContextDriver::Country => {
                country_catalog_record(&id, &title, &status, &country_codes)
            }
        };
        set_catalog_price_list_relation(&mut catalog, price_list_id.as_deref());
        set_catalog_publication_relation(&mut catalog, publication_id.as_deref());
        self.store
            .staged
            .catalogs
            .insert(id.clone(), catalog.clone());
        if let Some(price_list_id) = price_list_id.as_deref() {
            self.attach_price_list_to_catalog(&id, price_list_id);
        }
        selected_catalog_payload(field, catalog, Vec::new())
    }

    pub(in crate::proxy) fn catalog_update_response(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
    ) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let Some(existing_catalog) = self.store.staged.catalogs.get(&id).cloned() else {
            return selected_catalog_error(
                field,
                vec!["id"],
                "Catalog does not exist",
                "CATALOG_NOT_FOUND",
            );
        };
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let mut updated_catalog = existing_catalog;

        if let Some(price_list_id) = resolved_string_field(&input, "priceListId") {
            self.catalog_relation_price_list_preflight(request, &price_list_id);
            if !self.catalog_relation_price_list_exists(&price_list_id) {
                return selected_catalog_error(
                    field,
                    vec!["input", "priceListId"],
                    "Price list not found.",
                    "PRICE_LIST_NOT_FOUND",
                );
            }
            if self.catalog_price_list_taken(&price_list_id, Some(&id)) {
                return selected_catalog_error(
                    field,
                    vec!["input", "priceListId"],
                    "Price list has already been taken",
                    "TAKEN",
                );
            }
            self.detach_existing_catalog_price_list(&updated_catalog);
            set_catalog_price_list_relation(&mut updated_catalog, Some(&price_list_id));
            if let Some(price_list) = self.store.staged.price_lists.get_mut(&price_list_id) {
                set_price_list_catalog_relation(price_list, Some(&id));
            }
        } else if input.get("priceListId") == Some(&ResolvedValue::Null) {
            self.detach_existing_catalog_price_list(&updated_catalog);
            set_catalog_price_list_relation(&mut updated_catalog, None);
        }

        if let Some(publication_id) = resolved_string_field(&input, "publicationId") {
            self.catalog_relation_publication_preflight(request, &publication_id);
            if !self.catalog_relation_publication_exists(&publication_id) {
                return selected_catalog_error(
                    field,
                    vec!["input", "publicationId"],
                    "Publication not found.",
                    "PUBLICATION_NOT_FOUND",
                );
            }
            if self.catalog_publication_taken(&publication_id, Some(&id)) {
                return selected_catalog_error(
                    field,
                    vec!["input", "publicationId"],
                    "Publication is already attached to another catalog",
                    "PUBLICATION_TAKEN",
                );
            }
            set_catalog_publication_relation(&mut updated_catalog, Some(&publication_id));
        } else if input.get("publicationId") == Some(&ResolvedValue::Null) {
            set_catalog_publication_relation(&mut updated_catalog, None);
        }

        self.store
            .staged
            .catalogs
            .insert(id, updated_catalog.clone());
        selected_catalog_payload(field, updated_catalog, Vec::new())
    }

    pub(in crate::proxy) fn catalog_delete_response(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let payload = if let Some(catalog) = self.store.staged.catalogs.remove(&id) {
            self.detach_existing_catalog_price_list(&catalog);
            json!({"deletedId": id, "userErrors": []})
        } else {
            json!({"deletedId": null, "userErrors": [catalog_user_error(vec!["id"], "Catalog does not exist", "CATALOG_NOT_FOUND")]})
        };
        selected_json(&payload, &field.selection)
    }

    pub(in crate::proxy) fn catalog_context_update_response(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let catalog_id = resolved_string_field(&field.arguments, "catalogId").unwrap_or_default();
        let Some(existing_catalog) = self.store.staged.catalogs.get(&catalog_id).cloned() else {
            return selected_catalog_error(
                field,
                vec!["catalogId"],
                "Catalog does not exist",
                "CATALOG_NOT_FOUND",
            );
        };
        let contexts_to_add = resolved_object_field(&field.arguments, "contextsToAdd");
        let contexts_to_remove = resolved_object_field(&field.arguments, "contextsToRemove");
        if contexts_to_add.is_none() && contexts_to_remove.is_none() {
            return selected_catalog_error(
                field,
                vec!["contextsToAdd"],
                "Must have `contexts_to_add` or `contexts_to_remove` argument.",
                "REQUIRES_CONTEXTS_TO_ADD_OR_REMOVE",
            );
        }

        let catalog_driver = catalog_context_driver(&existing_catalog);
        let mut errors = Vec::new();
        for (field_prefix, context) in [
            ("contextsToAdd", contexts_to_add.as_ref()),
            ("contextsToRemove", contexts_to_remove.as_ref()),
        ] {
            if let Some(context) = context {
                for (driver, field_name) in catalog_context_type_fields(context) {
                    if driver != catalog_driver {
                        errors.push(catalog_user_error(
                            vec![field_prefix, field_name],
                            CATALOG_CONTEXT_DRIVER_MISMATCH_MESSAGE,
                            "CONTEXT_DRIVER_MISMATCH",
                        ));
                        continue;
                    }
                    match driver {
                        CatalogContextDriver::Market => {
                            for (index, market_id) in
                                list_string_field(context, field_name).iter().enumerate()
                            {
                                if !self.market_exists(market_id) {
                                    errors.push(catalog_user_error(
                                        vec![field_prefix, field_name, &index.to_string()],
                                        "Market does not exist",
                                        "MARKET_NOT_FOUND",
                                    ));
                                }
                            }
                        }
                        CatalogContextDriver::CompanyLocation => {
                            for (index, location_id) in
                                list_string_field(context, field_name).iter().enumerate()
                            {
                                if !self.store.staged.b2b_locations.contains_key(location_id) {
                                    errors.push(catalog_user_error(
                                        vec![field_prefix, field_name, &index.to_string()],
                                        COMPANY_LOCATION_NOT_FOUND_MESSAGE,
                                        "COMPANY_LOCATION_NOT_FOUND",
                                    ));
                                }
                            }
                        }
                        CatalogContextDriver::Country => {}
                    }
                }
            }
        }
        if !errors.is_empty() {
            return selected_catalog_payload(field, Value::Null, errors);
        }

        let mut updated_catalog = existing_catalog;
        match catalog_driver {
            CatalogContextDriver::Market => {
                let mut market_ids = catalog_market_ids(&updated_catalog);
                if let Some(context) = contexts_to_remove.as_ref() {
                    let remove = list_string_field(context, "marketIds")
                        .into_iter()
                        .collect::<BTreeSet<_>>();
                    market_ids.retain(|id| !remove.contains(id));
                }
                if let Some(context) = contexts_to_add.as_ref() {
                    for market_id in list_string_field(context, "marketIds") {
                        if !market_ids.contains(&market_id) {
                            market_ids.push(market_id);
                        }
                    }
                }
                let market_names = self.staged_market_names();
                set_catalog_market_ids(&mut updated_catalog, &market_ids, &market_names);
            }
            CatalogContextDriver::CompanyLocation => {
                let mut company_location_ids = catalog_company_location_ids(&updated_catalog);
                if let Some(context) = contexts_to_remove.as_ref() {
                    let mut remove = list_string_field(context, "companyLocationIds")
                        .into_iter()
                        .collect::<BTreeSet<_>>();
                    remove.extend(list_string_field(context, "locationIds"));
                    company_location_ids.retain(|id| !remove.contains(id));
                }
                if let Some(context) = contexts_to_add.as_ref() {
                    for location_id in company_location_ids_from_context(context) {
                        if !company_location_ids.contains(&location_id) {
                            company_location_ids.push(location_id);
                        }
                    }
                }
                set_catalog_company_location_ids(
                    &mut updated_catalog,
                    &company_location_ids,
                    &self.staged_company_locations_for_catalog(),
                );
            }
            CatalogContextDriver::Country => {
                let mut country_codes = catalog_country_codes(&updated_catalog);
                if let Some(context) = contexts_to_remove.as_ref() {
                    let remove = country_codes_from_context(context)
                        .into_iter()
                        .collect::<BTreeSet<_>>();
                    country_codes.retain(|code| !remove.contains(code));
                }
                if let Some(context) = contexts_to_add.as_ref() {
                    for country_code in country_codes_from_context(context) {
                        if !country_codes.contains(&country_code) {
                            country_codes.push(country_code);
                        }
                    }
                }
                set_catalog_country_codes(&mut updated_catalog, &country_codes);
            }
        }
        self.store
            .staged
            .catalogs
            .insert(catalog_id.clone(), updated_catalog.clone());
        selected_catalog_payload(field, updated_catalog, Vec::new())
    }

    pub(in crate::proxy) fn next_catalog_id(&self, driver_type: CatalogContextDriver) -> String {
        let numeric_id =
            (self.store.staged.markets.len() * 2) + (self.store.staged.catalogs.len() * 2) + 1;
        shopify_gid(driver_type.catalog_type_name(), numeric_id)
    }

    pub(in crate::proxy) fn staged_company_locations_for_catalog(&self) -> BTreeMap<String, Value> {
        self.store
            .staged
            .b2b_locations
            .iter()
            .map(|(id, location)| (id.clone(), location.clone()))
            .collect()
    }

    pub(in crate::proxy) fn price_list_mutation_data(
        &mut self,
        fields: &[RootFieldSelection],
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        self.fixed_price_mutation_preflight(fields, request, variables);
        if fields.iter().any(|field| {
            matches!(
                field.name.as_str(),
                "webPresenceCreate" | "webPresenceUpdate" | "webPresenceDelete"
            )
        }) {
            self.web_presence_mutation_preflight(variables, request);
        }
        let mut errors = Vec::new();
        let mut touched_ids = Vec::new();
        let data = root_payload_json(fields, |field| {
            let outcome = match field.name.as_str() {
                "priceListCreate" => self.price_list_create_response(field),
                "priceListUpdate" => self.price_list_update_response(field),
                "priceListDelete" => {
                    PriceListFieldOutcome::payload(self.price_list_delete_response(field))
                }
                "priceListFixedPricesByProductUpdate" => PriceListFieldOutcome::payload(
                    self.price_list_fixed_prices_by_product_update_response(field),
                ),
                "priceListFixedPricesAdd" => {
                    PriceListFieldOutcome::payload(self.price_list_fixed_prices_add_response(field))
                }
                "priceListFixedPricesUpdate" => PriceListFieldOutcome::payload(
                    self.price_list_fixed_prices_update_response(field),
                ),
                "priceListFixedPricesDelete" => PriceListFieldOutcome::payload(
                    self.price_list_fixed_prices_delete_response(field),
                ),
                "quantityRulesDelete" => PriceListFieldOutcome::payload(
                    self.quantity_rules_delete_price_list_response(field),
                ),
                "webPresenceCreate" => PriceListFieldOutcome::payload(
                    self.web_presence_create_price_list_response(field, request, query, variables),
                ),
                "webPresenceUpdate" => PriceListFieldOutcome::payload(
                    self.web_presence_update_price_list_response(field, request, query, variables),
                ),
                "webPresenceDelete" => PriceListFieldOutcome::payload(
                    self.web_presence_delete_price_list_response(field),
                ),
                _ => PriceListFieldOutcome::payload(Value::Null),
            };
            if let Some(id) = outcome.value["priceList"]["id"]
                .as_str()
                .or_else(|| outcome.value["webPresence"]["id"].as_str())
                .or_else(|| outcome.value["deletedId"].as_str())
            {
                touched_ids.push(id.to_string());
            }
            errors.extend(outcome.errors);
            Some(outcome.value)
        });
        if !touched_ids.is_empty() {
            self.record_mutation_log_entry(request, query, variables, "priceList", touched_ids);
        }
        let mut body = serde_json::Map::new();
        body.insert("data".to_string(), data);
        if !errors.is_empty() {
            body.insert("errors".to_string(), Value::Array(errors));
        }
        Value::Object(body)
    }

    pub(in crate::proxy) fn price_list_create_response(
        &mut self,
        field: &RootFieldSelection,
    ) -> PriceListFieldOutcome {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let name = resolved_string_field(&input, "name").unwrap_or_default();
        if let Some(error) = price_list_name_error(&self.store.staged.price_lists, &name, None) {
            return PriceListFieldOutcome::price_list_error(field, error);
        }
        let Some(currency) = resolved_string_field(&input, "currency") else {
            return PriceListFieldOutcome::price_list_error(
                field,
                (
                    vec!["input", "currency"],
                    "Currency can't be blank",
                    "BLANK",
                ),
            );
        };
        let Some(parent) = resolved_object_field(&input, "parent") else {
            return PriceListFieldOutcome::price_list_error(
                field,
                (vec!["input", "parent"], "Parent must exist", "REQUIRED"),
            );
        };
        let adjustment = resolved_object_field(&parent, "adjustment").unwrap_or_default();
        if let Some(error) = price_list_adjustment_error(&adjustment) {
            return PriceListFieldOutcome::price_list_error(field, error);
        }
        let adjustment_type = resolved_string_field(&adjustment, "type").unwrap_or_default();

        let catalog_id = resolved_string_field(&input, "catalogId");
        if let Some(catalog_id) = catalog_id.as_deref() {
            if price_list_catalog_id_has_wrong_gid_type(catalog_id) {
                return PriceListFieldOutcome::resource_not_found(catalog_id, field);
            }
            if let Some(error) = self.price_list_catalog_validation_error(catalog_id, None) {
                return PriceListFieldOutcome::price_list_with_user_errors(
                    field,
                    Value::Null,
                    vec![error],
                );
            }
        }

        let id = self.next_price_list_id();
        let price_list = price_list_record(
            &id,
            &name,
            &currency,
            &adjustment_type,
            price_list_adjustment_value_json(&adjustment),
            catalog_id.as_deref(),
        );
        if let Some(catalog_id) = catalog_id.as_deref() {
            self.attach_price_list_to_catalog(catalog_id, &id);
        }
        self.store.staged.price_lists.insert(id, price_list.clone());
        PriceListFieldOutcome::price_list_with_user_errors(field, price_list, Vec::new())
    }

    pub(in crate::proxy) fn price_list_update_response(
        &mut self,
        field: &RootFieldSelection,
    ) -> PriceListFieldOutcome {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let Some(existing) = self.store.staged.price_lists.get(&id).cloned() else {
            return PriceListFieldOutcome::price_list_error(
                field,
                (
                    vec!["id"],
                    "Price list does not exist.",
                    "PRICE_LIST_NOT_FOUND",
                ),
            );
        };
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        if let Some(name) = resolved_string_field(&input, "name") {
            if let Some(error) =
                price_list_name_error(&self.store.staged.price_lists, &name, Some(&id))
            {
                return PriceListFieldOutcome::price_list_error(field, error);
            }
        }
        let parent_update = resolved_object_field(&input, "parent");
        if let Some(parent) = parent_update.as_ref() {
            let adjustment = resolved_object_field(parent, "adjustment").unwrap_or_default();
            if let Some(error) = price_list_adjustment_error(&adjustment) {
                let (path, message, code) = error;
                return PriceListFieldOutcome::price_list_with_user_errors(
                    field,
                    existing.clone(),
                    vec![price_list_user_error(path, message, code)],
                );
            }
        }
        if input.get("catalogId") != Some(&ResolvedValue::Null) {
            if let Some(catalog_id) = resolved_string_field(&input, "catalogId") {
                if price_list_catalog_id_has_wrong_gid_type(&catalog_id) {
                    return PriceListFieldOutcome::resource_not_found(&catalog_id, field);
                }
                if let Some(error) =
                    self.price_list_catalog_validation_error(&catalog_id, Some(&id))
                {
                    return PriceListFieldOutcome::price_list_with_user_errors(
                        field,
                        Value::Null,
                        vec![error],
                    );
                }
            }
        }

        let mut updated = existing;
        if let Some(name) = resolved_string_field(&input, "name") {
            if let Some(object) = updated.as_object_mut() {
                object.insert("name".to_string(), json!(name));
            }
        }
        if let Some(currency) = resolved_string_field(&input, "currency") {
            if let Some(object) = updated.as_object_mut() {
                object.insert("currency".to_string(), json!(currency));
            }
        }
        if let Some(parent) = parent_update.as_ref() {
            let adjustment = resolved_object_field(parent, "adjustment").unwrap_or_default();
            let adjustment_type = resolved_string_field(&adjustment, "type").unwrap_or_default();
            if let Some(object) = updated.as_object_mut() {
                object.insert(
                    "parent".to_string(),
                    json!({"adjustment": {"type": adjustment_type, "value": price_list_adjustment_value_json(&adjustment)}}),
                );
            }
        }
        if input.get("catalogId") == Some(&ResolvedValue::Null) {
            self.detach_price_list_from_catalogs(&id);
            if let Some(object) = updated.as_object_mut() {
                object.insert("catalogId".to_string(), Value::Null);
                object.insert("catalog".to_string(), Value::Null);
            }
        } else if let Some(catalog_id) = resolved_string_field(&input, "catalogId") {
            self.detach_price_list_from_catalogs(&id);
            self.attach_price_list_to_catalog(&catalog_id, &id);
            if let Some(object) = updated.as_object_mut() {
                object.insert("catalogId".to_string(), json!(catalog_id));
                object.insert("catalog".to_string(), json!({"id": catalog_id}));
            }
        }
        self.store.staged.price_lists.insert(id, updated.clone());
        PriceListFieldOutcome::price_list_with_user_errors(field, updated, Vec::new())
    }

    pub(in crate::proxy) fn price_list_delete_response(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let payload = if self.store.staged.price_lists.remove(&id).is_some() {
            self.detach_price_list_from_catalogs(&id);
            json!({"deletedId": id, "userErrors": []})
        } else {
            price_list_payload_error(
                "deletedId",
                vec!["id"],
                "Price list does not exist.",
                "PRICE_LIST_NOT_FOUND",
            )
        };
        selected_json(&payload, &field.selection)
    }

    /// Hydrate the staged store from a cassette-backed preflight before applying a
    /// fixed-price mutation, mirroring the production Admin GraphQL reads the live
    /// capture scripts record. Gated on LiveHybrid so other read modes are untouched.
    /// The cassette serves recorded real Shopify data, which the generic staging
    /// logic below loads into the local store — no fixture is hardcoded.
    pub(in crate::proxy) fn fixed_price_mutation_preflight(
        &mut self,
        fields: &[RootFieldSelection],
        request: &Request,
        variables: &BTreeMap<String, ResolvedValue>,
    ) {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return;
        }
        let by_product = fields
            .iter()
            .any(|field| field.name == "priceListFixedPricesByProductUpdate");
        let variant_level = fields.iter().any(|field| {
            matches!(
                field.name.as_str(),
                "priceListFixedPricesAdd"
                    | "priceListFixedPricesUpdate"
                    | "priceListFixedPricesDelete"
            )
        });
        let body = if by_product {
            json!({
                "query": FIXED_PRICE_BY_PRODUCT_PREFLIGHT_QUERY,
                "variables": product_fixed_prices_preflight_variables(variables),
                "operationName": "MarketsMutationPreflightHydrate",
            })
        } else if variant_level {
            json!({
                "query": FIXED_PRICE_VARIANT_PREFLIGHT_QUERY,
                "variables": variant_fixed_prices_preflight_variables(fields),
                "operationName": "MarketsMutationPreflightHydrate",
            })
        } else {
            return;
        };
        self.run_markets_preflight(request, body, Self::stage_fixed_price_preflight);
    }

    fn run_markets_preflight(
        &mut self,
        request: &Request,
        body: Value,
        stage: impl FnOnce(&mut Self, &Value),
    ) {
        let response = self.upstream_post(request, body);
        if response.status < 400 {
            stage(self, &response.body);
        }
    }

    fn cold_markets_preflight(
        &mut self,
        query: &str,
        variables: Value,
        request: &Request,
        stage: impl FnOnce(&mut Self, &Value),
    ) {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return;
        }
        if self.has_markets_overlay_state() {
            return;
        }
        let body = json!({
            "query": query,
            "variables": variables,
            "operationName": "MarketsMutationPreflightHydrate",
        });
        self.run_markets_preflight(request, body, stage);
    }

    /// Stage the records a fixed-price preflight returns. Products always merge
    /// (idempotent observation); price lists insert only when absent so a
    /// multi-step lifecycle (add → update → delete) preserves the edges accumulated
    /// by earlier mutations instead of being reset to the clean baseline each step.
    pub(in crate::proxy) fn stage_fixed_price_preflight(&mut self, body: &Value) {
        let Some(data) = body.get("data").filter(|data| data.is_object()) else {
            return;
        };
        for product in markets_collect_records(data, "products", "product") {
            self.store.stage_observed_product_json(&product);
        }
        if let Some(nodes) = data.get("productNodes").and_then(Value::as_array) {
            for product in nodes {
                if product.is_object() {
                    self.store.stage_observed_product_json(product);
                }
            }
        }
        if let Some(nodes) = data.get("productVariants").and_then(Value::as_array) {
            for variant in nodes {
                if let Some(product) = variant.get("product").filter(|value| value.is_object()) {
                    self.store.stage_observed_product_json(product);
                }
            }
        }
        for record in markets_collect_records(data, "priceLists", "priceList") {
            if let Some(id) = record_gid(&record, "PriceList") {
                self.store
                    .staged
                    .price_lists
                    .entry(id)
                    .or_insert_with(|| record.clone());
            }
        }
    }

    pub(in crate::proxy) fn price_list_fixed_prices_by_product_update_response(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let price_list_id = read_price_list_id(&field.arguments);
        let price_list = price_list_id
            .as_ref()
            .and_then(|id| self.store.staged.price_lists.get(id).cloned());
        let price_inputs = resolved_object_list(&field.arguments, "pricesToAdd");
        let delete_product_ids =
            resolved_string_list_arg(&field.arguments, "pricesToDeleteByProductIds");

        let mut errors = match (&price_list_id, &price_list) {
            (Some(_), Some(_)) => Vec::new(),
            _ => vec![fixed_price_by_product_error(
                json!(["priceListId"]),
                "Price list does not exist.",
                "PRICE_LIST_DOES_NOT_EXIST",
            )],
        };
        errors.extend(product_level_fixed_price_errors(
            &self.store,
            &price_list,
            &price_inputs,
            &delete_product_ids,
        ));

        match (price_list, errors.is_empty()) {
            (Some(existing), true) => {
                let added_product_ids: Vec<String> = price_inputs
                    .iter()
                    .filter_map(|input| resolved_nonempty_string(input, "productId"))
                    .collect();
                let mut fixed_inputs: Vec<ResolvedValue> = Vec::new();
                for input in &price_inputs {
                    let Some(product_id) = resolved_nonempty_string(input, "productId") else {
                        continue;
                    };
                    let ResolvedValue::Object(base_fields) = input else {
                        continue;
                    };
                    for variant in self.store.fixed_price_variants_for_product(&product_id) {
                        let Some(variant_id) = variant["id"].as_str() else {
                            continue;
                        };
                        let mut object = base_fields.clone();
                        object.insert(
                            "variantId".to_string(),
                            ResolvedValue::String(variant_id.to_string()),
                        );
                        fixed_inputs.push(ResolvedValue::Object(object));
                    }
                }
                let delete_variant_ids: Vec<String> = delete_product_ids
                    .iter()
                    .flat_map(|product_id| self.store.fixed_price_variants_for_product(product_id))
                    .filter_map(|variant| variant["id"].as_str().map(str::to_string))
                    .collect();
                let mut updated = existing;
                upsert_fixed_price_nodes(&mut updated, &self.store, &fixed_inputs);
                delete_fixed_price_nodes(&mut updated, &delete_variant_ids);
                let prices_to_add_products =
                    fixed_price_product_payloads(&self.store, &added_product_ids);
                let prices_to_delete_products =
                    fixed_price_product_payloads(&self.store, &delete_product_ids);
                if let Some(id) = price_list_id {
                    self.store.staged.price_lists.insert(id, updated.clone());
                }
                selected_json(
                    &json!({
                        "priceList": updated,
                        "pricesToAddProducts": prices_to_add_products,
                        "pricesToDeleteProducts": prices_to_delete_products,
                        "fixedPriceVariantIds": [],
                        "deletedFixedPriceVariantIds": [],
                        "userErrors": []
                    }),
                    &field.selection,
                )
            }
            (_, _) => selected_json(
                &json!({
                    "priceList": null,
                    "pricesToAddProducts": null,
                    "pricesToDeleteProducts": null,
                    "userErrors": errors
                }),
                &field.selection,
            ),
        }
    }

    pub(in crate::proxy) fn price_list_fixed_prices_add_response(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let price_list_id = read_price_list_id(&field.arguments);
        let price_list = price_list_id
            .as_ref()
            .and_then(|id| self.store.staged.price_lists.get(id).cloned());
        let price_inputs = resolved_object_list(&field.arguments, "prices");

        let mut errors = price_list_fixed_price_target_errors(&price_list_id, &price_list);
        if let Some(existing) = &price_list {
            errors.extend(fixed_price_input_errors(
                &self.store,
                existing,
                &price_inputs,
                "prices",
            ));
        }

        match (price_list, errors.is_empty()) {
            (Some(existing), true) => {
                let mut updated = existing;
                upsert_fixed_price_nodes(&mut updated, &self.store, &price_inputs);
                let prices = fixed_price_nodes_for_variant_ids(
                    &updated,
                    &mutation_variant_ids(&price_inputs),
                );
                if let Some(id) = price_list_id {
                    self.store.staged.price_lists.insert(id, updated);
                }
                selected_json(
                    &json!({"prices": prices, "userErrors": []}),
                    &field.selection,
                )
            }
            (price_list, _) => {
                let prices = if price_list.is_some() {
                    json!([])
                } else {
                    Value::Null
                };
                selected_json(
                    &json!({"prices": prices, "userErrors": errors}),
                    &field.selection,
                )
            }
        }
    }

    pub(in crate::proxy) fn price_list_fixed_prices_update_response(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let price_list_id = read_price_list_id(&field.arguments);
        let price_list = price_list_id
            .as_ref()
            .and_then(|id| self.store.staged.price_lists.get(id).cloned());
        let (price_inputs, price_input_field) = read_fixed_price_update_inputs(&field.arguments);
        let delete_variant_ids = resolved_string_list_arg(&field.arguments, "variantIdsToDelete");

        let mut errors = price_list_fixed_price_target_errors(&price_list_id, &price_list);
        if let Some(existing) = &price_list {
            errors.extend(fixed_price_input_errors(
                &self.store,
                existing,
                &price_inputs,
                price_input_field,
            ));
            errors.extend(fixed_price_delete_variant_errors(
                &self.store,
                &delete_variant_ids,
                "variantIdsToDelete",
            ));
        }

        match (price_list, errors.is_empty()) {
            (Some(existing), true) => {
                let deleted =
                    fixed_price_variant_ids_in_request_order(&existing, &delete_variant_ids);
                let mut updated = existing;
                upsert_fixed_price_nodes(&mut updated, &self.store, &price_inputs);
                delete_fixed_price_nodes(&mut updated, &delete_variant_ids);
                let mut changed = mutation_variant_ids(&price_inputs);
                extend_unique_strings(&mut changed, &deleted);
                let prices_added = fixed_price_nodes_for_variant_ids(&updated, &changed);
                if let Some(id) = price_list_id {
                    self.store.staged.price_lists.insert(id, updated.clone());
                }
                selected_json(
                    &json!({
                        "priceList": updated,
                        "pricesAdded": prices_added,
                        "deletedFixedPriceVariantIds": deleted,
                        "userErrors": []
                    }),
                    &field.selection,
                )
            }
            (price_list, _) => {
                let empty_or_null = if price_list.is_some() {
                    json!([])
                } else {
                    Value::Null
                };
                selected_json(
                    &json!({
                        "priceList": price_list.unwrap_or(Value::Null),
                        "pricesAdded": empty_or_null.clone(),
                        "deletedFixedPriceVariantIds": empty_or_null,
                        "userErrors": errors
                    }),
                    &field.selection,
                )
            }
        }
    }

    pub(in crate::proxy) fn price_list_fixed_prices_delete_response(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let price_list_id = read_price_list_id(&field.arguments);
        let price_list = price_list_id
            .as_ref()
            .and_then(|id| self.store.staged.price_lists.get(id).cloned());
        let variant_ids = resolved_string_list_arg(&field.arguments, "variantIds");

        let mut errors = price_list_fixed_price_target_errors(&price_list_id, &price_list);
        if let Some(existing) = &price_list {
            errors.extend(fixed_price_delete_variant_errors(
                &self.store,
                &variant_ids,
                "variantIds",
            ));
            errors.extend(fixed_price_delete_not_fixed_errors(
                &self.store,
                existing,
                &variant_ids,
                "variantIds",
            ));
        }

        match (price_list, errors.is_empty()) {
            (Some(existing), true) => {
                let deleted = fixed_price_variant_ids_in_request_order(&existing, &variant_ids);
                let mut updated = existing;
                delete_fixed_price_nodes(&mut updated, &variant_ids);
                if let Some(id) = price_list_id {
                    self.store.staged.price_lists.insert(id, updated);
                }
                selected_json(
                    &json!({"deletedFixedPriceVariantIds": deleted, "userErrors": []}),
                    &field.selection,
                )
            }
            (price_list, _) => {
                let deleted = if price_list.is_some() {
                    json!([])
                } else {
                    Value::Null
                };
                selected_json(
                    &json!({"deletedFixedPriceVariantIds": deleted, "userErrors": errors}),
                    &field.selection,
                )
            }
        }
    }

    pub(in crate::proxy) fn quantity_rules_delete_price_list_response(
        &self,
        field: &RootFieldSelection,
    ) -> Value {
        let price_list_id =
            resolved_string_field(&field.arguments, "priceListId").unwrap_or_default();
        let variant_ids = resolved_string_list_arg(&field.arguments, "variantIds");
        let payload = if !self
            .store
            .staged
            .price_lists
            .contains_key(price_list_id.as_str())
        {
            json!({"deletedQuantityRulesVariantIds": [], "userErrors": [quantity_rule_error(vec!["priceListId"], "PRICE_LIST_DOES_NOT_EXIST", "Price list does not exist.")]})
        } else if variant_ids
            .iter()
            .any(|id| id == "gid://shopify/ProductVariant/0")
        {
            json!({"deletedQuantityRulesVariantIds": [], "userErrors": [quantity_rule_error(vec!["variantIds", "0"], "PRODUCT_VARIANT_DOES_NOT_EXIST", "Product variant ID does not exist.")]})
        } else {
            json!({"deletedQuantityRulesVariantIds": variant_ids, "userErrors": []})
        };
        selected_json(&payload, &field.selection)
    }

    pub(in crate::proxy) fn web_presence_create_price_list_response(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let payload =
            self.web_presence_helper_create_payload_inner(&input, request, query, variables, false);
        selected_json(&payload, &field.selection)
    }

    pub(in crate::proxy) fn web_presence_update_price_list_response(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let payload = self.web_presence_helper_update_payload_inner(
            &id, &input, request, query, variables, false,
        );
        selected_json(&payload, &field.selection)
    }

    pub(in crate::proxy) fn web_presence_delete_price_list_response(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let payload = self.web_presence_delete_payload(&id);
        selected_json(&payload, &field.selection)
    }

    pub(in crate::proxy) fn web_presence_helper_query(&self, query: &str) -> Response {
        let fields = root_fields(query, &BTreeMap::new()).unwrap_or_default();
        let mut data = serde_json::Map::new();
        for field in fields {
            if field.name == "webPresences" {
                data.insert(
                    field.response_key,
                    staged_nodes_connection(
                        &self.store.staged.web_presences,
                        &field.selection,
                        true,
                    ),
                );
            }
        }
        ok_json(json!({"data": Value::Object(data)}))
    }

    /// Hydrate the staged store from a cassette-backed preflight before applying a
    /// web-presence mutation on a cold store, mirroring `fixed_price_mutation_preflight`.
    /// Gated on LiveHybrid so other read modes are untouched, and on a cold markets
    /// overlay so only the first mutation in a scenario seeds the baseline (later
    /// mutations operate on the already-staged records). The cassette serves recorded
    /// real Shopify `webPresences` data, which `stage_web_presence_preflight` loads
    /// into the local store — no fixture is hardcoded.
    pub(in crate::proxy) fn web_presence_mutation_preflight(
        &mut self,
        _variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) {
        self.cold_markets_preflight(
            WEB_PRESENCE_PREFLIGHT_QUERY,
            json!({ "first": WEB_PRESENCE_PREFLIGHT_FIRST }),
            request,
            Self::stage_web_presence_preflight,
        );
    }

    /// Forward a market-localization mutation preflight on a cold store so the
    /// target resource's content (valid keys/digests), the shop's markets, and any
    /// existing localizations are staged before the register/remove is validated and
    /// applied. Gated like the web-presence preflight: once any markets-domain record
    /// is staged (including markets observed from a read carrying a `markets` field),
    /// the baseline is already known and the preflight is skipped. The cassette
    /// matches the sentinel query plus the mutation's own variables exactly; an
    /// unmatched preflight (a capture recorded with a different preflight form, or
    /// none) returns an error body and is ignored.
    pub(in crate::proxy) fn market_localization_mutation_preflight(
        &mut self,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) {
        self.cold_markets_preflight(
            MARKET_LOCALIZATION_PREFLIGHT_QUERY,
            market_localization_preflight_variables(variables),
            request,
            Self::hydrate_markets_from_upstream,
        );
    }

    /// Stage the baseline `webPresences` a preflight returns. Records insert only
    /// when absent so a multi-step lifecycle (create → update → delete) preserves
    /// records staged by earlier mutations instead of resetting to the baseline.
    pub(in crate::proxy) fn stage_web_presence_preflight(&mut self, body: &Value) {
        let Some(data) = body.get("data").filter(|data| data.is_object()) else {
            return;
        };
        for record in markets_collect_records(data, "webPresences", "webPresence") {
            if let Some(id) = record_gid(&record, "MarketWebPresence") {
                self.store.staged.web_presences.entry(id).or_insert(record);
            }
        }
    }

    pub(in crate::proxy) fn web_presence_helper_mutation(
        &mut self,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let (response_key, payload_selection, arguments) = primary_root_field(query, variables)
            .map(|field| (field.response_key, field.selection, field.arguments))
            .unwrap_or_else(|| (root_field.to_string(), Vec::new(), BTreeMap::new()));
        let payload = match root_field {
            "webPresenceCreate" => {
                let input = resolved_object_field(&arguments, "input").unwrap_or_default();
                self.web_presence_helper_create_payload(&input, request, query, variables)
            }
            "webPresenceUpdate" => {
                let id = resolved_string_field(&arguments, "id").unwrap_or_default();
                let input = resolved_object_field(&arguments, "input").unwrap_or_default();
                self.web_presence_helper_update_payload(&id, &input, request, query, variables)
            }
            "webPresenceDelete" => {
                let id = resolved_string_field(&arguments, "id").unwrap_or_default();
                self.web_presence_delete_payload(&id)
            }
            _ => Value::Null,
        };
        ok_json(json!({"data": {response_key: selected_json(&payload, &payload_selection)}}))
    }

    /// Stage a `webPresenceDelete`. Shopify rejects deleting a presence that does
    /// not exist (`WEB_PRESENCE_NOT_FOUND`) and refuses to delete the presence that
    /// serves the shop's primary domain (`SHOP_MUST_HAVE_PRIMARY_DOMAIN_WEB_PRESENCE`);
    /// only subfolder presences (which carry a null `domain`) can be removed.
    pub(in crate::proxy) fn web_presence_delete_payload(&mut self, id: &str) -> Value {
        let Some(record) = self.store.staged.web_presences.get(id) else {
            return json!({
                "deletedId": null,
                "userErrors": [market_user_error(vec!["id"], "The market web presence wasn't found.", json!("WEB_PRESENCE_NOT_FOUND"))]
            });
        };
        if record.get("domain").is_some_and(|domain| !domain.is_null()) {
            return json!({
                "deletedId": null,
                "userErrors": [market_user_error(vec!["id"], "The shop must have a web presence that uses the primary domain.", json!("SHOP_MUST_HAVE_PRIMARY_DOMAIN_WEB_PRESENCE"))]
            });
        }
        self.store.staged.web_presences.remove(id);
        json!({"deletedId": id, "userErrors": []})
    }

    pub(in crate::proxy) fn web_presence_helper_create_payload(
        &mut self,
        input: &BTreeMap<String, ResolvedValue>,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        self.web_presence_helper_create_payload_inner(input, request, query, variables, true)
    }

    fn web_presence_helper_create_payload_inner(
        &mut self,
        input: &BTreeMap<String, ResolvedValue>,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        record_log: bool,
    ) -> Value {
        let mut errors = Vec::new();
        let mut draft = web_presence_draft_from_input(input, None, &mut errors, true);
        let linked_domain = draft
            .domain_id
            .as_deref()
            .and_then(|id| self.store.domain_by_id(id));
        web_presence_validate_routing_and_uniqueness(
            &draft,
            input,
            &self.store.staged.web_presences,
            None,
            true,
            linked_domain.as_ref(),
            &mut errors,
        );
        if !errors.is_empty() {
            return json!({"webPresence": null, "userErrors": errors});
        }
        let id = shopify_gid(
            "MarketWebPresence",
            next_web_presence_numeric_id(&self.store.staged.web_presences),
        );
        draft.id = id.clone();
        let shop_domain = web_presence_shop_domain(&self.store);
        let record =
            market_web_presence_helper_record(&draft, &shop_domain, linked_domain.as_ref());
        self.store
            .staged
            .web_presences
            .insert(id.clone(), record.clone());
        if record_log {
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                "webPresenceCreate",
                vec![id],
            );
        }
        json!({"webPresence": record, "userErrors": []})
    }

    pub(in crate::proxy) fn web_presence_helper_update_payload(
        &mut self,
        id: &str,
        input: &BTreeMap<String, ResolvedValue>,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        self.web_presence_helper_update_payload_inner(id, input, request, query, variables, true)
    }

    fn web_presence_helper_update_payload_inner(
        &mut self,
        id: &str,
        input: &BTreeMap<String, ResolvedValue>,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        record_log: bool,
    ) -> Value {
        let Some(existing) = self.store.staged.web_presences.get(id).cloned() else {
            return json!({"webPresence": null, "userErrors": [market_user_error(vec!["id"], "The market web presence wasn't found.", json!("WEB_PRESENCE_NOT_FOUND"))]});
        };
        let mut errors = Vec::new();
        let draft = web_presence_draft_from_input(input, Some(&existing), &mut errors, false);
        let linked_domain = draft
            .domain_id
            .as_deref()
            .and_then(|id| self.store.domain_by_id(id));
        web_presence_validate_routing_and_uniqueness(
            &draft,
            input,
            &self.store.staged.web_presences,
            Some(id),
            false,
            linked_domain.as_ref(),
            &mut errors,
        );
        if !errors.is_empty() {
            return json!({"webPresence": null, "userErrors": errors});
        }
        let shop_domain = web_presence_shop_domain(&self.store);
        let record =
            market_web_presence_helper_record(&draft, &shop_domain, linked_domain.as_ref());
        self.store
            .staged
            .web_presences
            .insert(id.to_string(), record.clone());
        if record_log {
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                "webPresenceUpdate",
                vec![id.to_string()],
            );
        }
        json!({"webPresence": record, "userErrors": []})
    }

    pub(in crate::proxy) fn next_price_list_id(&self) -> String {
        let numeric_id = (self.store.staged.markets.len() * 2)
            + (self.store.staged.catalogs.len() * 2)
            + self.store.staged.price_lists.len()
            + 1;
        shopify_gid("PriceList", numeric_id)
    }

    pub(in crate::proxy) fn attach_price_list_to_catalog(
        &mut self,
        catalog_id: &str,
        price_list_id: &str,
    ) {
        if let Some(catalog) = self.store.staged.catalogs.get_mut(catalog_id) {
            set_catalog_price_list_relation(catalog, Some(price_list_id));
        }
        if let Some(price_list) = self.store.staged.price_lists.get_mut(price_list_id) {
            set_price_list_catalog_relation(price_list, Some(catalog_id));
        }
    }

    pub(in crate::proxy) fn detach_price_list_from_catalogs(&mut self, price_list_id: &str) {
        for catalog in self.store.staged.catalogs.values_mut() {
            if catalog_relation_id(catalog, "priceListId", "priceList").as_deref()
                == Some(price_list_id)
            {
                set_catalog_price_list_relation(catalog, None);
            }
        }
    }

    pub(in crate::proxy) fn detach_existing_catalog_price_list(&mut self, catalog: &Value) {
        if let Some(price_list_id) = catalog_relation_id(catalog, "priceListId", "priceList") {
            if let Some(price_list) = self.store.staged.price_lists.get_mut(&price_list_id) {
                set_price_list_catalog_relation(price_list, None);
            }
        }
    }

    pub(in crate::proxy) fn price_list_catalog_validation_error(
        &self,
        catalog_id: &str,
        current_price_list_id: Option<&str>,
    ) -> Option<Value> {
        let Some(catalog) = self.store.staged.catalogs.get(catalog_id) else {
            return Some(price_list_user_error(
                vec!["input", "catalogId"],
                "Catalog does not exist.",
                "CATALOG_DOES_NOT_EXIST",
            ));
        };
        let price_list_id = catalog_relation_id(catalog, "priceListId", "priceList")?;
        if current_price_list_id == Some(price_list_id.as_str()) {
            return None;
        }
        if self.catalog_relation_price_list_exists(&price_list_id)
            && self.catalog_price_list_taken(&price_list_id, None)
        {
            return Some(price_list_user_error(
                vec!["input", "catalogId"],
                "Catalog has a price list already assigned.",
                "CATALOG_TAKEN",
            ));
        }
        None
    }

    pub(in crate::proxy) fn catalog_relation_price_list_exists(&self, price_list_id: &str) -> bool {
        self.store.staged.price_lists.contains_key(price_list_id)
    }

    pub(in crate::proxy) fn catalog_relation_publication_exists(
        &self,
        publication_id: &str,
    ) -> bool {
        self.store.has_publication_id(publication_id)
    }

    fn catalog_relation_price_list_preflight(&mut self, request: &Request, price_list_id: &str) {
        if self.config.read_mode == ReadMode::Snapshot
            || self.catalog_relation_price_list_exists(price_list_id)
        {
            return;
        }
        let body = json!({
            "query": CATALOG_RELATION_PRICE_LIST_PREFLIGHT_QUERY,
            "variables": {"id": price_list_id},
            "operationName": "CatalogRelationPriceListHydrate",
        });
        self.run_markets_preflight(request, body, Self::hydrate_markets_from_upstream);
    }

    fn catalog_relation_publication_preflight(&mut self, request: &Request, publication_id: &str) {
        if self.config.read_mode == ReadMode::Snapshot
            || self.catalog_relation_publication_exists(publication_id)
        {
            return;
        }
        let body = json!({
            "query": CATALOG_RELATION_PUBLICATION_PREFLIGHT_QUERY,
            "variables": {"id": publication_id},
            "operationName": "CatalogRelationPublicationHydrate",
        });
        self.run_markets_preflight(request, body, Self::stage_catalog_publication_preflight);
    }

    fn stage_catalog_publication_preflight(&mut self, body: &Value) {
        let Some(publication) = body
            .get("data")
            .and_then(|data| data.get("publication"))
            .filter(|publication| publication.is_object())
        else {
            return;
        };
        if let Some(id) = record_gid(publication, "Publication") {
            self.store.staged.publication_ids.insert(id.clone());
            self.store
                .staged
                .publications
                .insert(id, publication.clone());
        }
    }

    pub(in crate::proxy) fn catalog_price_list_taken(
        &self,
        price_list_id: &str,
        current_catalog_id: Option<&str>,
    ) -> bool {
        self.store
            .staged
            .catalogs
            .iter()
            .any(|(catalog_id, catalog)| {
                current_catalog_id != Some(catalog_id.as_str())
                    && catalog_relation_id(catalog, "priceListId", "priceList").as_deref()
                        == Some(price_list_id)
            })
    }

    pub(in crate::proxy) fn catalog_publication_taken(
        &self,
        publication_id: &str,
        current_catalog_id: Option<&str>,
    ) -> bool {
        self.store
            .staged
            .catalogs
            .iter()
            .any(|(catalog_id, catalog)| {
                current_catalog_id != Some(catalog_id.as_str())
                    && catalog_relation_id(catalog, "publicationId", "publication").as_deref()
                        == Some(publication_id)
            })
    }

    pub(in crate::proxy) fn market_localization_query_data(
        &mut self,
        fields: &[RootFieldSelection],
        request: &Request,
    ) -> Value {
        root_payload_json(fields, |field| {
            Some(match field.name.as_str() {
                "marketLocalizableResource" => {
                    let resource_id = resolved_string_field(&field.arguments, "resourceId")
                        .unwrap_or_else(|| "gid://shopify/Metafield/localizable".to_string());
                    if resource_id.contains("missing") {
                        Value::Null
                    } else {
                        let market_filter = market_localizations_market_filter(&field.selection);
                        selected_json(
                            &self.market_localizable_resource(
                                &resource_id,
                                market_filter.as_deref(),
                            ),
                            &field.selection,
                        )
                    }
                }
                // Local read-after-write serve only reaches the connections after the
                // resource was observed; a backend with no staged localizable owners
                // returns an empty connection (not a fabricated node) for both variants.
                "marketLocalizableResources" | "marketLocalizableResourcesByIds" => selected_json(
                    &connection_json_with_empty_edges(Vec::new()),
                    &field.selection,
                ),
                "markets" => self.localization_markets_connection(field, request),
                _ => Value::Null,
            })
        })
    }

    /// Project a market-localizable resource from observed/staged state: the
    /// `marketLocalizableContent` recorded when the resource was first read (empty
    /// when never observed), plus the staged `marketLocalizations` for this resource
    /// filtered to the read's `marketId` argument. No field metadata is fabricated.
    pub(in crate::proxy) fn market_localizable_resource(
        &self,
        resource_id: &str,
        market_filter: Option<&str>,
    ) -> Value {
        let content = self
            .store
            .staged
            .localization_resources
            .get(resource_id)
            .cloned()
            .unwrap_or_else(|| json!([]));
        let localizations = self
            .store
            .staged
            .localization_translations
            .iter()
            .filter(|translation| translation["resourceId"].as_str() == Some(resource_id))
            .filter(|translation| match market_filter {
                Some(market_id) => translation["market"]["id"].as_str() == Some(market_id),
                None => true,
            })
            .cloned()
            .collect::<Vec<_>>();
        json!({
            "resourceId": resource_id,
            "marketLocalizableContent": content,
            "marketLocalizations": localizations
        })
    }

    pub(in crate::proxy) fn market_localization_mutation_data(
        &mut self,
        fields: &[RootFieldSelection],
    ) -> Value {
        root_payload_json(fields, |field| {
            Some(match field.name.as_str() {
                "marketLocalizationsRegister" => self.market_localizations_register_response(field),
                "marketLocalizationsRemove" => self.market_localizations_remove_response(field),
                _ => Value::Null,
            })
        })
    }

    pub(in crate::proxy) fn market_localizations_register_response(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let resource_id = resolved_string_field(&field.arguments, "resourceId").unwrap_or_default();
        let localizations = resolved_list_arg(&field.arguments, "marketLocalizations");
        // 1. Per-mutation key cap fires before resource existence (matches live Shopify).
        if localizations.len() > 100 {
            return selected_json(
                &json!({
                    "marketLocalizations": null,
                    "userErrors": [market_localization_error(vec!["resourceId"], "TOO_MANY_KEYS_FOR_RESOURCE", "Too many keys for resource - maximum 100 per mutation")]
                }),
                &field.selection,
            );
        }
        // 2. The resource must have been observed (cold read / mutation preflight).
        let Some(content) = self
            .store
            .staged
            .localization_resources
            .get(&resource_id)
            .cloned()
        else {
            return selected_json(
                &json!({
                    "marketLocalizations": null,
                    "userErrors": [market_localization_error(vec!["resourceId"], "RESOURCE_NOT_FOUND", &format!("Resource {resource_id} does not exist"))]
                }),
                &field.selection,
            );
        };

        let mut staged = Vec::new();
        for (index, input) in localizations.iter().enumerate() {
            let field_index = index.to_string();
            let market_id = resolved_object_string(input, "marketId").unwrap_or_default();
            if market_id.is_empty() || !self.market_exists(&market_id) {
                return selected_json(
                    &json!({
                        "marketLocalizations": null,
                        "userErrors": [market_localization_error(vec!["marketLocalizations", &field_index, "marketId"], "MARKET_DOES_NOT_EXIST", "The market does not exist")]
                    }),
                    &field.selection,
                );
            }
            let key = resolved_object_string(input, "key").unwrap_or_default();
            // 3. The key must be one of the resource's localizable content keys.
            let Some(content_entry) = content.as_array().and_then(|entries| {
                entries
                    .iter()
                    .find(|entry| entry["key"].as_str() == Some(key.as_str()))
            }) else {
                return selected_json(
                    &json!({
                        "marketLocalizations": null,
                        "userErrors": [market_localization_error(vec!["marketLocalizations", &field_index, "key"], "INVALID_KEY_FOR_MODEL", &format!("Key {key} is not a valid market localizable field"))]
                    }),
                    &field.selection,
                );
            };
            // 4. The supplied digest must match the resource's current content digest.
            let expected_digest = content_entry["digest"].as_str();
            if resolved_object_string(input, "marketLocalizableContentDigest").as_deref()
                != expected_digest
            {
                return selected_json(
                    &json!({
                        "marketLocalizations": null,
                        "userErrors": [market_localization_error(vec!["marketLocalizations", &field_index, "marketLocalizableContentDigest"], "INVALID_MARKET_LOCALIZABLE_CONTENT", "The provided content digest does not match the latest resource content")]
                    }),
                    &field.selection,
                );
            }
            // 5. The localized value must not be blank.
            if resolved_object_string(input, "value").as_deref() == Some("") {
                return selected_json(
                    &json!({
                        "marketLocalizations": null,
                        "userErrors": [market_localization_error(vec!["marketLocalizations", &field_index, "value"], "FAILS_RESOURCE_VALIDATION", "Value can't be blank")]
                    }),
                    &field.selection,
                );
            }
            // 6. Shopify exposes definition-backed money metafields as a
            // `value` market-localizable field, but rejects JSON money payloads
            // during register with a resource-validation error.
            if market_localizable_content_is_money_metafield(content_entry) {
                return selected_json(
                    &json!({
                        "marketLocalizations": null,
                        "userErrors": [market_localization_error(vec!["marketLocalizations", &field_index, "value"], "FAILS_RESOURCE_VALIDATION", "Market Localizable content is invalid")]
                    }),
                    &field.selection,
                );
            }
            staged.push(self.market_localization_staged_record(&resource_id, &market_id, input));
        }

        for record in &staged {
            let resource_id = record["resourceId"].as_str().unwrap_or_default();
            let key = record["key"].as_str().unwrap_or_default();
            let market_id = record["market"]["id"].as_str().unwrap_or_default();
            self.store
                .staged
                .localization_translations
                .retain(|existing| {
                    existing["resourceId"].as_str() != Some(resource_id)
                        || existing["key"].as_str() != Some(key)
                        || existing["market"]["id"].as_str() != Some(market_id)
                });
            self.store
                .staged
                .localization_translations
                .push(record.clone());
        }

        selected_json(
            &json!({ "marketLocalizations": staged, "userErrors": [] }),
            &field.selection,
        )
    }

    /// Build a staged market-localization record with the live market name resolved
    /// from staged markets and a synthetic `updatedAt` (matched loosely by the specs).
    fn market_localization_staged_record(
        &self,
        resource_id: &str,
        market_id: &str,
        input: &ResolvedValue,
    ) -> Value {
        let value = resolved_object_string(input, "value").unwrap_or_default();
        let key = resolved_object_string(input, "key").unwrap_or_default();
        let market_name = self
            .store
            .staged
            .markets
            .get(market_id)
            .and_then(|market| market.get("name"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        json!({
            "resourceId": resource_id,
            "key": key,
            "value": value,
            "updatedAt": SYNTHETIC_MARKET_LOCALIZATION_TIMESTAMP,
            "outdated": false,
            "market": { "id": market_id, "name": market_name }
        })
    }

    pub(in crate::proxy) fn market_localizations_remove_response(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let resource_id = resolved_string_field(&field.arguments, "resourceId").unwrap_or_default();
        if !self
            .store
            .staged
            .localization_resources
            .contains_key(&resource_id)
        {
            return selected_json(
                &json!({
                    "marketLocalizations": null,
                    "userErrors": [market_localization_error(vec!["resourceId"], "RESOURCE_NOT_FOUND", &format!("Resource {resource_id} does not exist"))]
                }),
                &field.selection,
            );
        }
        let keys = resolved_string_list_arg(&field.arguments, "marketLocalizationKeys");
        let market_ids = resolved_string_list_arg(&field.arguments, "marketIds");
        if keys.is_empty() {
            return selected_json(
                &json!({ "marketLocalizations": null, "userErrors": [] }),
                &field.selection,
            );
        }

        let mut removed = Vec::new();
        self.store
            .staged
            .localization_translations
            .retain(|translation| {
                let matches_resource =
                    translation["resourceId"].as_str() == Some(resource_id.as_str());
                let matches_key = translation["key"]
                    .as_str()
                    .is_some_and(|key| keys.iter().any(|candidate| candidate == key));
                let matches_market = market_ids.is_empty()
                    || translation["market"]["id"]
                        .as_str()
                        .is_some_and(|id| market_ids.iter().any(|candidate| candidate == id));
                let should_remove = matches_resource && matches_key && matches_market;
                if should_remove {
                    removed.push(translation.clone());
                }
                !should_remove
            });
        let removed = if removed.is_empty() {
            Value::Null
        } else {
            Value::Array(removed)
        };
        selected_json(
            &json!({ "marketLocalizations": removed, "userErrors": [] }),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn localization_register_response(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let resource_id = resolved_string_field(&field.arguments, "resourceId").unwrap_or_default();
        if !self.localization_translatable_resource_exists(&resource_id) {
            return selected_json(
                &json!({
                    "translations": null,
                    "userErrors": [user_error(["resourceId"], &format!("Resource {resource_id} does not exist"), Some("RESOURCE_NOT_FOUND"))]
                }),
                &field.selection,
            );
        }

        let translations = resolved_list_arg(&field.arguments, "translations");
        if translations.is_empty() {
            return selected_json(
                &json!({ "translations": [], "userErrors": [] }),
                &field.selection,
            );
        }
        if translations.len() > 100 {
            return selected_json(
                &json!({
                    "translations": null,
                    "userErrors": [user_error(["resourceId"], "Too many keys for resource - maximum 100 per mutation", Some("TOO_MANY_KEYS_FOR_RESOURCE"))]
                }),
                &field.selection,
            );
        }
        let mut staged = Vec::new();
        let mut user_errors = Vec::new();
        let primary_locale = self.localization_primary_locale();
        for (index, translation_input) in translations.iter().enumerate() {
            let field_index = index.to_string();
            let locale = resolved_object_string(translation_input, "locale")
                .unwrap_or_else(|| "fr".to_string());
            if locale == primary_locale {
                user_errors.push(user_error(
                    json!(["translations", field_index, "locale"]),
                    "Locale cannot be the same as the shop's primary locale",
                    Some("INVALID_LOCALE_FOR_SHOP"),
                ));
                continue;
            }
            if !self.localization_shop_locale_added(&locale) {
                user_errors.push(user_error(
                    json!(["translations", field_index, "locale"]),
                    "Locale is not a valid locale for the shop",
                    Some("INVALID_LOCALE_FOR_SHOP"),
                ));
                continue;
            }
            let market_id = resolved_object_string(translation_input, "marketId");
            if matches!(market_id.as_deref(), Some(id) if !self.market_exists(id)) {
                user_errors.push(user_error(
                    json!(["translations", field_index, "marketId"]),
                    "The market corresponding to the `marketId` argument doesn't exist",
                    Some("MARKET_DOES_NOT_EXIST"),
                ));
                continue;
            }
            if resolved_object_string(translation_input, "value").as_deref() == Some("") {
                user_errors.push(user_error(
                    json!(["translations", field_index, "value"]),
                    "Value can't be blank",
                    Some("FAILS_RESOURCE_VALIDATION"),
                ));
                continue;
            }
            let key = resolved_object_string(translation_input, "key").unwrap_or_default();
            if self.localization_resource_has_modeled_translation_keys(&resource_id)
                && !self.localization_translation_key_is_valid(&resource_id, &key)
            {
                user_errors.push(user_error(
                    json!(["translations", field_index, "key"]),
                    &format!("Key {key} is not a valid translatable field"),
                    Some("INVALID_KEY_FOR_MODEL"),
                ));
                continue;
            }
            let value = resolved_object_string(translation_input, "value").unwrap_or_default();
            if market_id.is_some()
                && self
                    .localization_shop_level_translation_value(&resource_id, &key, &locale)
                    .is_some_and(|base_value| base_value == value)
            {
                user_errors.push(user_error(
                    json!(["translations", field_index, "value"]),
                    "Value cannot match original content",
                    Some("FAILS_RESOURCE_VALIDATION"),
                ));
                continue;
            }
            if let Some(supplied_digest) =
                resolved_object_string(translation_input, "translatableContentDigest")
            {
                let digest_invalid = supplied_digest.starts_with("invalid-")
                    || self
                        .localization_source_content_value(&resource_id, &key)
                        .is_some_and(|value| {
                            localization_content_digest(&value) != supplied_digest
                        });
                if digest_invalid {
                    user_errors.push(user_error(
                        json!(["translations", field_index, "translatableContentDigest"]),
                        "Translatable content hash is invalid",
                        Some("INVALID_TRANSLATABLE_CONTENT"),
                    ));
                    continue;
                }
            }
            if market_id.is_some()
                && !self.localization_translation_key_is_market_customizable(&resource_id, &key)
            {
                user_errors.push(user_error(
                    json!(["translations", field_index, "key"]),
                    &format!(
                        "Key {key} cannot be customized for a market; it can only be translated."
                    ),
                    Some("RESOURCE_NOT_MARKET_CUSTOMIZABLE"),
                ));
                continue;
            }

            let mut translation = translation_from_input(translation_input);
            translation["resourceId"] = json!(resource_id);
            translation["updatedAt"] = json!(self.next_localization_translation_timestamp());
            if translation["key"] == json!("handle") {
                let original_value = translation["value"].as_str().unwrap_or_default();
                if original_value.chars().count() > 255 {
                    user_errors.push(user_error(json!(["translations", field_index, "value"]), "Value fails validation on resource: [\"Handle is too long (maximum is 255 characters)\"]", Some("FAILS_RESOURCE_VALIDATION")));
                    continue;
                }
                translation["value"] = json!(normalize_localized_handle(original_value));
            }
            staged.push(translation);
        }

        for translation in &staged {
            self.store
                .staged
                .localization_translations
                .retain(|existing| {
                    existing["resourceId"] != translation["resourceId"]
                        || existing["key"] != translation["key"]
                        || existing["locale"] != translation["locale"]
                        || existing["market"] != translation["market"]
                });
            self.store
                .staged
                .localization_translations
                .push(translation.clone());
        }

        selected_json(
            &json!({ "translations": staged, "userErrors": user_errors }),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn localization_remove_response(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let resource_id = resolved_string_field(&field.arguments, "resourceId").unwrap_or_default();
        if !self.localization_translatable_resource_exists(&resource_id) {
            return selected_json(
                &json!({
                    "translations": null,
                    "userErrors": [user_error(["resourceId"], &format!("Resource {resource_id} does not exist"), Some("RESOURCE_NOT_FOUND"))]
                }),
                &field.selection,
            );
        }
        let keys = resolved_string_list_arg(&field.arguments, "translationKeys");
        let market_ids = resolved_string_list_arg(&field.arguments, "marketIds");
        let locales = resolved_string_list_arg(&field.arguments, "locales");
        if locales.is_empty() {
            return selected_json(
                &json!({ "translations": null, "userErrors": [] }),
                &field.selection,
            );
        }
        self.store.staged.localization_dirty = true;
        let mut removed = Vec::new();
        let mut retained = Vec::new();
        for translation in self.store.staged.localization_translations.drain(..) {
            let key_matches =
                keys.is_empty() || keys.iter().any(|key| translation["key"] == json!(key));
            let locale_matches = locales
                .iter()
                .any(|locale| translation["locale"] == json!(locale));
            let market_matches = if market_ids.is_empty() {
                translation["market"].is_null()
            } else {
                market_ids
                    .iter()
                    .any(|id| translation["market"]["id"] == json!(id))
            };
            if key_matches && locale_matches && market_matches {
                removed.push(translation);
            } else {
                retained.push(translation);
            }
        }
        self.store.staged.localization_translations = retained;
        let removed = if removed.is_empty() {
            Value::Null
        } else {
            Value::Array(removed)
        };
        selected_json(
            &json!({ "translations": removed, "userErrors": [] }),
            &field.selection,
        )
    }

    fn next_localization_translation_timestamp(&self) -> String {
        product_mutation_timestamp(self.log_entries.len() as u64)
    }

    pub(in crate::proxy) fn localization_translatable_resource_selected(
        &self,
        resource_id: &str,
        selections: &[SelectedField],
    ) -> Value {
        selected_payload_json(selections, |selection| match selection.name.as_str() {
            "resourceId" => Some(json!(resource_id)),
            "translatableContent" => Some(Value::Array(
                self.localization_translatable_content(resource_id)
                    .iter()
                    .map(|content| selected_json(content, &selection.selection))
                    .collect(),
            )),
            "translations" => {
                let locale = resolved_string_field(&selection.arguments, "locale");
                let market_id = resolved_string_field(&selection.arguments, "marketId");
                Some(Value::Array(
                    self.localization_translations_for(
                        resource_id,
                        locale.as_deref(),
                        market_id.as_deref(),
                    )
                    .iter()
                    .map(|translation| selected_json(translation, &selection.selection))
                    .collect(),
                ))
            }
            _ => None,
        })
    }

    pub(in crate::proxy) fn localization_translatable_resources_connection(
        &self,
        field: &RootFieldSelection,
    ) -> Value {
        let resource_type = resolved_string_field(&field.arguments, "resourceType")
            .unwrap_or_else(|| "PRODUCT".to_string());
        let mut records = self
            .localization_translatable_resource_ids()
            .into_iter()
            .filter(|id| localization_resource_type_matches(id, &resource_type))
            .collect::<Vec<_>>();
        if records.is_empty() {
            records.push(default_localization_resource_id(&resource_type));
        }
        selected_typed_connection_with_args(
            &records,
            &field.arguments,
            &field.selection,
            |id, selection| self.localization_translatable_resource_selected(id, selection),
            |id| id.clone(),
        )
    }

    pub(in crate::proxy) fn localization_translatable_resources_by_ids_connection(
        &self,
        field: &RootFieldSelection,
    ) -> Value {
        let records = resolved_string_list_arg(&field.arguments, "resourceIds")
            .into_iter()
            .filter(|id| self.localization_translatable_resource_exists(id))
            .collect::<Vec<_>>();
        selected_typed_connection_with_args(
            &records,
            &field.arguments,
            &field.selection,
            |id, selection| self.localization_translatable_resource_selected(id, selection),
            |id| id.clone(),
        )
    }

    pub(in crate::proxy) fn localization_markets_connection(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
    ) -> Value {
        let mut records = self
            .store
            .staged
            .markets
            .values()
            .cloned()
            .collect::<Vec<_>>();
        if records.is_empty() {
            records = self.hydrate_localization_markets(field, request);
        }
        selected_typed_connection_with_args(
            &records,
            &field.arguments,
            &field.selection,
            selected_json,
            value_id_cursor,
        )
    }

    fn hydrate_localization_markets(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
    ) -> Vec<Value> {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return Vec::new();
        }
        let first = resolved_int_field(&field.arguments, "first")
            .unwrap_or(50)
            .max(0);
        if first == 0 {
            return Vec::new();
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": "query LocalizationMarketsHydrate($first: Int!) { markets(first: $first) { nodes { id name handle status type } } }",
                "operationName": "LocalizationMarketsHydrate",
                "variables": { "first": first }
            }),
        );
        self.stage_observed_localization_source_data(&response.body["data"]);
        if response.status >= 400 {
            return self.hydrate_localization_markets_from_original_request(field, request);
        }
        let records = response.body["data"]["markets"]["nodes"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if records.is_empty() && response.body["data"]["markets"].is_null() {
            return self.hydrate_localization_markets_from_original_request(field, request);
        }
        self.stage_observed_localization_markets(&records);
        records
    }

    fn hydrate_localization_markets_from_original_request(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
    ) -> Vec<Value> {
        let response = (self.upstream_transport)(request.clone());
        self.stage_observed_localization_source_data(&response.body["data"]);
        if response.status >= 400 {
            return Vec::new();
        }
        let market_connection = &response.body["data"][&field.response_key];
        let mut records = market_connection["nodes"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if records.is_empty() {
            records = market_connection["edges"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|edge| edge.get("node").cloned())
                .collect();
        }
        self.stage_observed_localization_markets(&records);
        records
    }

    fn stage_observed_localization_source_data(&mut self, data: &Value) {
        let Some(data) = data.as_object() else {
            return;
        };
        for value in data.values() {
            if let Some(locales) = value.as_array() {
                self.stage_observed_shop_locales(locales);
            }
            if let Some(nodes) = value.get("nodes").and_then(Value::as_array) {
                self.stage_observed_localization_markets(nodes);
            }
        }
    }

    fn stage_observed_shop_locales(&mut self, locales: &[Value]) {
        for locale in locales {
            let Some(locale_code) = locale.get("locale").and_then(Value::as_str) else {
                continue;
            };
            if locale_code == "en"
                || !locale.get("name").is_some_and(Value::is_string)
                || !locale.get("primary").is_some_and(Value::is_boolean)
                || !locale.get("published").is_some_and(Value::is_boolean)
            {
                continue;
            }
            self.store
                .staged
                .shop_locales
                .insert(locale_code.to_string(), locale.clone());
        }
    }

    fn stage_observed_localization_markets(&mut self, records: &[Value]) {
        for market in records {
            let Some(id) = market.get("id").and_then(Value::as_str) else {
                continue;
            };
            if !id.starts_with("gid://shopify/Market/")
                || !market.get("name").is_some_and(Value::is_string)
                || !market.get("handle").is_some_and(Value::is_string)
                || !market.get("status").is_some_and(Value::is_string)
            {
                continue;
            }
            self.store
                .staged
                .markets
                .insert(id.to_string(), market.clone());
        }
    }

    /// True when any markets-domain record has been staged. Mirrors Gleam's
    /// `has_local_markets_query_state` (minus the product check, since the Rust
    /// markets stores are staged-only with no base layer). Once a lifecycle has
    /// staged a market/catalog/price-list/web-presence, plural reads serve
    /// locally (read-after-write); before that, cold reads forward upstream.
    pub(in crate::proxy) fn has_markets_overlay_state(&self) -> bool {
        !self.store.staged.markets.is_empty()
            || !self.store.staged.catalogs.is_empty()
            || !self.store.staged.price_lists.is_empty()
            || !self.store.staged.web_presences.is_empty()
    }

    /// LiveHybrid cold-read decision for the Markets domain, ported from Gleam
    /// `should_fetch_upstream_in_live_hybrid` (markets/queries.gleam:111). When
    /// this returns true the dispatcher forwards the original request verbatim
    /// upstream and hydrates the staged store from the response.
    pub(in crate::proxy) fn markets_should_fetch_upstream(
        &self,
        root_field: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> bool {
        match root_field {
            "market" => !markets_variables_have_local_id(variables, &self.store.staged.markets),
            "catalog" => !markets_variables_have_local_id(variables, &self.store.staged.catalogs),
            "priceList" => {
                !markets_variables_have_local_id(variables, &self.store.staged.price_lists)
            }
            // A market-localizable resource read forwards once per resource: until the
            // resource's content has been observed (cold read or mutation preflight),
            // forward verbatim so Shopify reports its real content/digests; afterwards
            // serve the staged read-after-write projection locally.
            "marketLocalizableResource" => resolved_string_field(variables, "resourceId")
                .map(|resource_id| {
                    !self
                        .store
                        .staged
                        .localization_resources
                        .contains_key(&resource_id)
                })
                .unwrap_or(true),
            "markets"
            | "catalogs"
            | "catalogsCount"
            | "priceLists"
            | "webPresences"
            | "marketsResolvedValues"
            | "marketLocalizableResources"
            | "marketLocalizableResourcesByIds" => !self.has_markets_overlay_state(),
            _ => false,
        }
    }

    /// Hydrate the staged markets stores from an upstream GraphQL response body,
    /// ported from Gleam `hydrate_from_upstream_response` (markets/queries.gleam:644).
    /// Records are observed as a side effect of a cold read so later targets
    /// (read-after-write, catalog delete, market localization) resolve locally.
    pub(in crate::proxy) fn hydrate_markets_from_upstream(&mut self, body: &Value) {
        let Some(data) = body.get("data") else {
            return;
        };
        if !data.is_object() {
            return;
        }
        // Shop record (primaryDomain etc.) for web-presence reads.
        if let Some(shop) = data.get("shop").filter(|shop| shop.is_object()) {
            if shop.get("id").and_then(Value::as_str).is_some() {
                shallow_merge_object(&mut self.store.base.shop, shop.clone());
            }
        }
        let hydrate_nodes = data
            .get("nodes")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|node| node.is_object())
            .collect::<Vec<_>>();
        let mut market_records = markets_collect_records(data, "markets", "market");
        market_records.extend(
            hydrate_nodes
                .iter()
                .filter(|node| {
                    node.get("__typename").and_then(Value::as_str) == Some("Market")
                        || record_gid(node, "gid://shopify/Market/").is_some()
                })
                .cloned(),
        );
        for record in &market_records {
            if let Some(id) = record_gid(record, "Market") {
                self.store.staged.markets.insert(id, record.clone());
            }
        }
        // Catalogs: top-level plus nested under each market.
        let mut catalog_records = markets_collect_records(data, "catalogs", "catalog");
        for market in &market_records {
            catalog_records.extend(markets_connection_nodes(market.get("catalogs")));
        }
        for record in &catalog_records {
            if let Some(id) = record_gid(record, "") {
                self.store.staged.catalogs.insert(id, record.clone());
            }
        }
        // Price lists: top-level plus nested under each catalog (singular field).
        let mut price_list_records = markets_collect_records(data, "priceLists", "priceList");
        for catalog in &catalog_records {
            if let Some(price_list) = catalog.get("priceList").filter(|value| value.is_object()) {
                price_list_records.push(price_list.clone());
            }
        }
        for record in &price_list_records {
            if let Some(id) = record_gid(record, "PriceList") {
                self.store.staged.price_lists.insert(id, record.clone());
            }
        }
        // Web presences: top-level plus nested under each market.
        let mut web_presence_records = markets_collect_records(data, "webPresences", "webPresence");
        for market in &market_records {
            web_presence_records.extend(markets_connection_nodes(market.get("webPresences")));
        }
        web_presence_records.extend(
            hydrate_nodes
                .iter()
                .filter(|node| {
                    node.get("__typename").and_then(Value::as_str) == Some("MarketWebPresence")
                        || record_gid(node, "gid://shopify/MarketWebPresence/").is_some()
                })
                .cloned(),
        );
        for record in &web_presence_records {
            if let Some(id) = record_gid(record, "MarketWebPresence") {
                // A web presence can surface both as a full top-level node (with
                // its `markets` connection) and as a sparse `{id}` pointer nested
                // under `market.webPresences`. Keep the richer projection so a
                // relationship stub never clobbers the markets connection the
                // delete cascade relies on to detach the deleted market.
                let richer = self
                    .store
                    .staged
                    .web_presences
                    .get(&id)
                    .map(|existing| {
                        record.as_object().map_or(0, serde_json::Map::len)
                            > existing.as_object().map_or(0, serde_json::Map::len)
                    })
                    .unwrap_or(true);
                if richer {
                    self.store.staged.web_presences.insert(id, record.clone());
                }
            }
        }
        // Products / variants (fixed-price preflight, localizable resources).
        for product in markets_collect_records(data, "products", "product") {
            self.store.stage_observed_product_json(&product);
        }
        if let Some(nodes) = data.get("productNodes").and_then(Value::as_array) {
            for product in nodes {
                if product.is_object() {
                    self.store.stage_observed_product_json(product);
                }
            }
        }
        // Market-localizable resources: the singular field, the type-scoped and
        // by-ids connections, plus the mutation-preflight `marketLocalizableResource`.
        let mut localizable_records = markets_collect_records(
            data,
            "marketLocalizableResources",
            "marketLocalizableResource",
        );
        localizable_records.extend(markets_connection_nodes(
            data.get("marketLocalizableResourcesByIds"),
        ));
        for record in &localizable_records {
            self.stage_observed_market_localizable_resource(record);
        }
    }

    /// Record a market-localizable resource observed upstream: index its
    /// `marketLocalizableContent` by `resourceId` (existence + valid keys/digests)
    /// and stage any pre-existing `marketLocalizations` so read-after-write filtering
    /// reflects Shopify's prior state for an arbitrary backend.
    fn stage_observed_market_localizable_resource(&mut self, resource: &Value) {
        let Some(resource_id) = resource.get("resourceId").and_then(Value::as_str) else {
            return;
        };
        if let Some(content) = resource
            .get("marketLocalizableContent")
            .filter(|content| content.is_array())
        {
            self.store
                .staged
                .localization_resources
                .insert(resource_id.to_string(), content.clone());
        }
        let Some(localizations) = resource
            .get("marketLocalizations")
            .and_then(Value::as_array)
            .filter(|localizations| !localizations.is_empty())
        else {
            return;
        };
        for localization in localizations {
            let key = localization.get("key").and_then(Value::as_str);
            let market_id = localization
                .get("market")
                .and_then(|market| market.get("id"))
                .and_then(Value::as_str);
            self.store
                .staged
                .localization_translations
                .retain(|existing| {
                    existing["resourceId"].as_str() != Some(resource_id)
                        || existing["key"].as_str() != key
                        || existing["market"]["id"].as_str() != market_id
                });
            let mut record = serde_json::Map::new();
            record.insert("resourceId".to_string(), json!(resource_id));
            for field in ["key", "value", "updatedAt", "outdated", "market"] {
                if let Some(value) = localization.get(field) {
                    record.insert(field.to_string(), value.clone());
                }
            }
            self.store
                .staged
                .localization_translations
                .push(Value::Object(record));
        }
    }

    /// Cold LiveHybrid localization reads need the captured upstream
    /// product/source-content slice before local translation mutations can
    /// validate resource existence and stage read-after-write effects. Once any
    /// localization/product/collection state exists, stay local so staged locale
    /// and translation changes are not bypassed by passthrough. Ported from Gleam
    /// `should_fetch_upstream_in_live_hybrid` (localization/queries.gleam:100).
    pub(in crate::proxy) fn localization_should_fetch_upstream(&self, root_field: &str) -> bool {
        if !matches!(
            root_field,
            "availableLocales"
                | "shopLocales"
                | "translatableResource"
                | "translatableResources"
                | "translatableResourcesByIds"
        ) {
            return false;
        }
        !self.has_localization_query_state()
    }

    fn has_localization_query_state(&self) -> bool {
        !self.store.staged.localization_translations.is_empty()
            || !self.store.staged.shop_locales.is_empty()
            || self.store.staged.localization_dirty
            || self.store.has_product_state()
            || self.store.has_collection_state()
    }

    /// Hydrate localization base state from an upstream GraphQL response body,
    /// ported from Gleam `hydrate_from_upstream_response`
    /// (localization/queries.gleam:234). Shop locales, available locales and
    /// translatable-resource product ids are observed as a side effect of a cold
    /// read so later targets (locale validation, read-after-write) resolve
    /// locally against real Shopify state.
    pub(in crate::proxy) fn hydrate_localization_from_upstream(&mut self, body: &Value) {
        let Some(data) = body.get("data").and_then(Value::as_object) else {
            return;
        };
        // Scan every top-level field (queries alias fields freely, e.g.
        // `allShopLocales: shopLocales`, `single: translatableResource`).
        let mut resources: Vec<Value> = Vec::new();
        for value in data.values() {
            // shopLocales / availableLocales arrays.
            if let Some(items) = value.as_array() {
                for item in items {
                    if item.get("isoCode").and_then(Value::as_str).is_some() {
                        if let (Some(code), Some(name)) = (
                            item.get("isoCode").and_then(Value::as_str),
                            item.get("name").and_then(Value::as_str),
                        ) {
                            self.store
                                .base
                                .available_locales
                                .insert(code.to_string(), name.to_string());
                        }
                    } else if item.get("primary").is_some() {
                        if let Some(code) = item.get("locale").and_then(Value::as_str) {
                            self.store
                                .base
                                .shop_locales
                                .insert(code.to_string(), item.clone());
                        }
                    }
                }
            }
            // translatableResource (single) or a connection of resources.
            if value.get("resourceId").and_then(Value::as_str).is_some() {
                resources.push(value.clone());
            }
            if let Some(nodes) = value.get("nodes").and_then(Value::as_array) {
                resources.extend(
                    nodes
                        .iter()
                        .filter(|node| node.get("resourceId").and_then(Value::as_str).is_some())
                        .cloned(),
                );
            }
        }
        for resource in &resources {
            if let Some(resource_id) = resource.get("resourceId").and_then(Value::as_str) {
                if resource_id.starts_with("gid://shopify/Product/") {
                    self.store
                        .base
                        .localization_product_ids
                        .insert(resource_id.to_string());
                    self.stage_observed_localization_product_source(resource_id, resource);
                } else if resource_id.starts_with("gid://shopify/Collection/") {
                    self.stage_observed_localization_collection_source(resource_id, resource);
                }
            }
        }
    }

    fn stage_observed_localization_product_source(&mut self, resource_id: &str, resource: &Value) {
        let Some(content) = resource
            .get("translatableContent")
            .and_then(Value::as_array)
        else {
            return;
        };
        let timestamp = default_product_timestamp();
        let mut product = self
            .store
            .product_staged_or_base(resource_id)
            .unwrap_or_else(|| ProductRecord {
                id: resource_id.to_string(),
                created_at: timestamp.clone(),
                updated_at: timestamp,
                status: "ACTIVE".to_string(),
                ..ProductRecord::default()
            });
        let mut observed = false;
        for entry in content {
            let Some(key) = entry.get("key").and_then(Value::as_str) else {
                continue;
            };
            let value = entry
                .get("value")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            match key {
                "title" => product.title = value,
                "body_html" => product.description_html = value,
                "handle" => product.handle = value,
                "product_type" => product.product_type = value,
                "meta_title" => product.seo_title = value,
                "meta_description" => product.seo_description = value,
                _ => continue,
            }
            observed = true;
        }
        if observed {
            self.store.stage_product(product);
        }
    }

    fn stage_observed_localization_collection_source(
        &mut self,
        resource_id: &str,
        resource: &Value,
    ) {
        let Some(content) = resource
            .get("translatableContent")
            .and_then(Value::as_array)
        else {
            return;
        };
        let mut collection = self
            .store
            .collection_by_id(resource_id)
            .cloned()
            .unwrap_or_else(|| json!({ "id": resource_id }));
        let Some(object) = collection.as_object_mut() else {
            return;
        };
        let mut observed = false;
        for entry in content {
            let Some(key) = entry.get("key").and_then(Value::as_str) else {
                continue;
            };
            let value = entry
                .get("value")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            match key {
                "title" => {
                    object.insert("title".to_string(), json!(value));
                }
                "body_html" => {
                    object.insert("descriptionHtml".to_string(), json!(value));
                }
                "handle" => {
                    object.insert("handle".to_string(), json!(value));
                }
                "meta_title" => {
                    collection_set_seo_field(object, "title", value);
                }
                "meta_description" => {
                    collection_set_seo_field(object, "description", value);
                }
                _ => continue,
            }
            observed = true;
        }
        if observed {
            self.store.stage_collection(Value::Object(object.clone()));
        }
    }

    fn localization_shop_locale_added(&self, locale: &str) -> bool {
        self.store.base.shop_locales.contains_key(locale)
            || self.store.staged.shop_locales.contains_key(locale)
    }

    pub(in crate::proxy) fn localization_translatable_resource_ids(&self) -> Vec<String> {
        let mut ids = self
            .store
            .staged
            .localization_translations
            .iter()
            .filter_map(|translation| translation["resourceId"].as_str().map(ToString::to_string))
            .collect::<Vec<_>>();
        ids.extend(self.store.products().into_iter().map(|product| product.id));
        ids.extend(self.store.base.localization_product_ids.iter().cloned());
        ids.extend(
            self.store
                .staged
                .collections
                .iter()
                .map(|(id, _)| id.clone()),
        );
        ids.sort();
        ids.dedup();
        ids
    }

    pub(in crate::proxy) fn localization_translations_for(
        &self,
        resource_id: &str,
        locale: Option<&str>,
        market_id: Option<&str>,
    ) -> Vec<Value> {
        self.store
            .staged
            .localization_translations
            .iter()
            .filter(|translation| translation["resourceId"].as_str() == Some(resource_id))
            .filter(|translation| {
                locale.is_none_or(|locale| translation["locale"].as_str() == Some(locale))
            })
            .filter(|translation| match market_id {
                Some(market_id) => translation["market"]["id"].as_str() == Some(market_id),
                None => true,
            })
            .cloned()
            .collect()
    }

    pub(in crate::proxy) fn localization_translatable_resource_exists(
        &self,
        resource_id: &str,
    ) -> bool {
        if resource_id.starts_with("gid://shopify/Product/") {
            return self.store.has_localization_product(resource_id);
        }
        true
    }

    fn localization_translatable_content(&self, resource_id: &str) -> Vec<Value> {
        let locale = self.localization_primary_locale();
        if resource_id.starts_with("gid://shopify/Product/") {
            return self
                .store
                .product_staged_or_base(resource_id)
                .map(|product| localization_product_translatable_content(&product, &locale))
                .unwrap_or_default();
        }
        if resource_id.starts_with("gid://shopify/Collection/") {
            return self
                .store
                .collection_by_id(resource_id)
                .map(|collection| localization_collection_translatable_content(collection, &locale))
                .unwrap_or_default();
        }
        Vec::new()
    }

    fn localization_primary_locale(&self) -> String {
        self.localization_shop_locales(None)
            .into_iter()
            .find(|locale| locale.get("primary").and_then(Value::as_bool) == Some(true))
            .and_then(|locale| {
                locale
                    .get("locale")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .unwrap_or_else(|| "en".to_string())
    }

    /// The current source-content value for a translatable resource field, when the
    /// proxy holds authoritative local state for it. Translatable content digests are
    /// `sha256(value)` of the source string (verified against live Shopify captures),
    /// so this lets the register path reject stale/incorrect `translatableContentDigest`
    /// inputs exactly like Shopify. Returns `None` for resources whose source content
    /// the proxy hasn't observed (hydrated-only ids), in which case digest validation
    /// is skipped — mirroring Gleam's "content not found → no digest error".
    fn localization_source_content_value(&self, resource_id: &str, key: &str) -> Option<String> {
        if resource_id.starts_with("gid://shopify/Product/") {
            let product = self.store.product_staged_or_base(resource_id)?;
            let value = match key {
                "title" => product.title.clone(),
                "body_html" => product.description_html.clone(),
                "handle" => product.handle.clone(),
                "product_type" => product.product_type.clone(),
                "meta_title" => product.seo_title.clone(),
                "meta_description" => product.seo_description.clone(),
                _ => return None,
            };
            return Some(value);
        }
        if resource_id.starts_with("gid://shopify/Collection/") {
            let collection = self.store.collection_by_id(resource_id)?;
            let value = match key {
                "title" => collection
                    .get("title")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                "body_html" => collection
                    .get("descriptionHtml")
                    .or_else(|| collection.get("bodyHtml"))
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                "handle" => collection
                    .get("handle")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                "meta_title" => collection
                    .pointer("/seo/title")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                "meta_description" => collection
                    .pointer("/seo/description")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                _ => return None,
            };
            return Some(value.to_string());
        }
        None
    }

    fn localization_shop_level_translation_value(
        &self,
        resource_id: &str,
        key: &str,
        locale: &str,
    ) -> Option<String> {
        self.store
            .staged
            .localization_translations
            .iter()
            .rev()
            .find(|translation| {
                translation["resourceId"].as_str() == Some(resource_id)
                    && translation["key"].as_str() == Some(key)
                    && translation["locale"].as_str() == Some(locale)
                    && translation["market"].is_null()
            })
            .and_then(|translation| translation["value"].as_str().map(ToString::to_string))
    }

    fn localization_resource_has_modeled_translation_keys(&self, resource_id: &str) -> bool {
        resource_id.starts_with("gid://shopify/Product/")
            || (resource_id.starts_with("gid://shopify/Collection/")
                && self.store.collection_by_id(resource_id).is_some())
    }

    fn localization_translation_key_is_valid(&self, resource_id: &str, key: &str) -> bool {
        if resource_id.starts_with("gid://shopify/Product/") {
            return matches!(
                key,
                "title"
                    | "body_html"
                    | "handle"
                    | "product_type"
                    | "meta_title"
                    | "meta_description"
            );
        }
        if resource_id.starts_with("gid://shopify/Collection/") {
            return matches!(
                key,
                "title" | "body_html" | "handle" | "meta_title" | "meta_description"
            );
        }
        false
    }

    fn localization_translation_key_is_market_customizable(
        &self,
        resource_id: &str,
        key: &str,
    ) -> bool {
        match shopify_gid_resource_type(resource_id) {
            Some("Product") => matches!(key, "title" | "body_html" | "product_type"),
            Some("Collection") => matches!(key, "title" | "body_html"),
            _ => false,
        }
    }

    /// Mirror Shopify's web-presence ↔ alternate-locale sync. When a non-primary
    /// locale is associated with one or more market web presences via
    /// `shopLocaleEnable`/`shopLocaleUpdate`, Shopify reflects it on the
    /// `MarketWebPresence` itself: every target presence gains the locale in
    /// `alternateLocales` (unpublished) plus a matching `rootUrls` entry. On an
    /// update the association is authoritative, so non-target presences lose the
    /// locale (`replace = true`); enable only adds. The downstream `webPresences`
    /// read is served from `staged.web_presences`, so the staged records are
    /// mutated in place. Ported from the Gleam localization mutation handlers.
    fn sync_web_presence_locales(&mut self, locale: &str, target_ids: &[String], replace: bool) {
        if locale == "en" {
            return;
        }
        let name = self
            .localization_available_locale_name(locale)
            .map(str::to_string);
        for (id, record) in self.store.staged.web_presences.iter_mut() {
            if target_ids.iter().any(|target| target == id) {
                web_presence_add_locale(record, locale, name.as_deref());
            } else if replace {
                web_presence_remove_locale(record, locale);
            }
        }
    }
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

/// The shop's myshopify domain, used as the host for synthesized web-presence
/// root URLs. Falls back to the neutral cold-runtime domain when the shop record
/// has no `myshopifyDomain` (mirrors the fallback used by region-coverage
/// lookups).
fn web_presence_shop_domain(store: &Store) -> String {
    let shop = store.effective_shop();
    if let Some(domain) = shop
        .get("myshopifyDomain")
        .and_then(Value::as_str)
        .filter(|domain| !domain.is_empty())
        .filter(|domain| *domain != "shopify-draft-proxy.local")
    {
        return domain.to_string();
    }
    observed_web_presence_shop_domain(store)
        .unwrap_or_else(|| "shopify-draft-proxy.local".to_string())
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

fn web_presence_host(url: &str) -> Option<String> {
    let (_, rest) = url.split_once("://")?;
    let host = rest.split('/').next().unwrap_or("");
    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}

/// Extract `scheme://host` from a URL, dropping any path/query suffix.
fn web_presence_origin(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    let host = rest.split('/').next().unwrap_or("");
    if host.is_empty() {
        None
    } else {
        Some(format!("{scheme}://{host}"))
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

fn markets_variables_have_local_id(
    variables: &BTreeMap<String, ResolvedValue>,
    records: &BTreeMap<String, Value>,
) -> bool {
    variables.values().any(|value| match value {
        ResolvedValue::String(id) => is_synthetic_gid(id) || records.contains_key(id),
        _ => false,
    })
}

fn markets_connection_nodes(value: Option<&Value>) -> Vec<Value> {
    let Some(value) = value else {
        return Vec::new();
    };
    let mut nodes = value
        .get("nodes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if nodes.is_empty() {
        nodes = value
            .get("edges")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|edge| edge.get("node").cloned())
            .filter(|node| node.is_object())
            .collect();
    }
    nodes
}

fn markets_collect_records(data: &Value, connection_key: &str, singular_key: &str) -> Vec<Value> {
    let mut records = markets_connection_nodes(data.get(connection_key));
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
/// type, and not a legacy market. Ported from Gleam
/// `markets.market_record_is_active_region_non_legacy` (markets.gleam:227).
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

fn market_update_currency_settings_json(
    existing: Option<&Value>,
    input: &BTreeMap<String, ResolvedValue>,
) -> Value {
    let currency_settings = resolved_object_field(input, "currencySettings").unwrap_or_default();
    let currency_code = resolved_string_field(&currency_settings, "baseCurrency")
        .or_else(|| value_string_field(existing, "baseCurrency", "currencyCode"))
        .unwrap_or_else(|| "USD".to_string());
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
/// `conditions.regionsCondition.regions` connection (nodes and/or edges). Ported
/// from Gleam `serializers.market_country_codes` (markets/serializers.gleam:450)
/// so both upstream-hydrated and mutation-staged market shapes resolve.
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

fn market_region_country_from_node(node: &Value, country_code: &str) -> Option<Value> {
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

pub(in crate::proxy) fn default_localization_resource_id(resource_type: &str) -> String {
    let gid_type = match resource_type.to_ascii_uppercase().as_str() {
        "COLLECTION" => "Collection",
        "ONLINE_STORE_THEME" => "OnlineStoreTheme",
        _ => "Product",
    };
    shopify_gid(gid_type, "9801098789170")
}

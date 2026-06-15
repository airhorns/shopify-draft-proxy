use super::*;
use sha2::{Digest, Sha256};

const FALLBACK_PRODUCT_TRANSLATION_TITLE: &str = "The Inventory Not Tracked Snowboard";
const FALLBACK_PRODUCT_TRANSLATION_HANDLE: &str = "the-inventory-not-tracked-snowboard";
const FALLBACK_PRODUCT_TRANSLATION_PRODUCT_TYPE: &str = "snowboard";

impl DraftProxy {
    pub(in crate::proxy) fn functions_metadata_mutation_data(
        &mut self,
        fields: &[RootFieldSelection],
    ) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "validationCreate" => self.function_validation_create_payload(field),
                "validationUpdate" => self.function_validation_update_payload(field),
                "validationDelete" => self.function_validation_delete_payload(field),
                "cartTransformCreate" => self.function_cart_transform_create_payload(field),
                "cartTransformDelete" => self.function_cart_transform_delete_payload(field),
                "taxAppConfigure" => self.function_tax_app_configure_payload(field),
                _ => Value::Null,
            };
            if !value.is_null() {
                data.insert(
                    field.response_key.clone(),
                    selected_json(&value, &field.selection),
                );
            }
        }
        Value::Object(data)
    }

    pub(in crate::proxy) fn functions_metadata_read_data(
        &self,
        fields: &[RootFieldSelection],
    ) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "validation" => resolved_field_string_arg(field, "id")
                    .and_then(|id| self.store.staged.function_validations.get(&id).cloned())
                    .or_else(|| self.store.staged.function_validation.clone())
                    .unwrap_or(Value::Null),
                "validations" => local_function_connection_from_nodes(
                    self.store
                        .staged
                        .function_validation_order
                        .iter()
                        .filter_map(|id| self.store.staged.function_validations.get(id).cloned())
                        .collect(),
                ),
                "cartTransforms" => local_function_connection_from_nodes(
                    self.store
                        .staged
                        .function_cart_transform_order
                        .iter()
                        .filter_map(|id| {
                            self.store
                                .staged
                                .function_cart_transforms
                                .get(id)
                                .map(|record| {
                                    cart_transform_record_for_selection(record, &field.selection)
                                })
                        })
                        .collect(),
                ),
                "shopifyFunctions" => {
                    let api_type = resolved_enum_arg(field, "apiType").unwrap_or_default();
                    let api_type = if api_type == "CART_TRANSFORM" {
                        "CART_TRANSFORM"
                    } else {
                        "VALIDATION"
                    };
                    json!({ "nodes": self.function_catalog_read_nodes(api_type) })
                }
                "shopifyFunction" => match resolved_field_string_arg(field, "id") {
                    Some(id) => {
                        function_by_id_or_handle(Some(id.as_str()), None).unwrap_or(Value::Null)
                    }
                    None => local_cart_transform_function(),
                },
                _ => Value::Null,
            };
            if value.is_null() {
                data.insert(field.response_key.clone(), Value::Null);
            } else {
                data.insert(
                    field.response_key.clone(),
                    selected_json(&value, &field.selection),
                );
            }
        }
        Value::Object(data)
    }

    fn function_catalog_read_nodes(&self, api_type: &str) -> Vec<Value> {
        let mut seen = BTreeSet::new();
        let mut nodes = Vec::new();
        for function in self
            .store
            .staged
            .function_validation_order
            .iter()
            .filter_map(|id| self.store.staged.function_validations.get(id))
            .chain(
                self.store
                    .staged
                    .function_cart_transform_order
                    .iter()
                    .filter_map(|id| self.store.staged.function_cart_transforms.get(id)),
            )
            .filter_map(|record| record.get("shopifyFunction"))
        {
            if function["apiType"].as_str() == Some(api_type) {
                if let Some(id) = function["id"].as_str() {
                    if seen.insert(id.to_string()) {
                        nodes.push(function.clone());
                    }
                }
            }
        }
        if nodes.is_empty() {
            function_catalog_by_api_type(api_type)
        } else {
            nodes
        }
    }

    pub(in crate::proxy) fn localization_query_data(
        &mut self,
        fields: &[RootFieldSelection],
        _query: &str,
    ) -> Value {
        let mut data = Value::Object(serde_json::Map::new());
        for field in fields {
            match field.name.as_str() {
                "availableLocales" => {
                    data[field.response_key.as_str()] = Value::Array(
                        self.localization_available_locales()
                            .iter()
                            .map(|locale| selected_json(locale, &field.selection))
                            .collect(),
                    );
                }
                "shopLocales" => {
                    let published_filter = resolved_bool_field(&field.arguments, "published");
                    data[field.response_key.as_str()] = Value::Array(
                        self.localization_shop_locales(published_filter)
                            .iter()
                            .map(|locale| selected_json(locale, &field.selection))
                            .collect(),
                    );
                }
                "translatableResource" => {
                    let resource_id = resolved_string_arg(&field.arguments, "resourceId")
                        .unwrap_or_else(|| "gid://shopify/Product/9801098789170".to_string());
                    if !self.localization_translatable_resource_exists(&resource_id) {
                        data[field.response_key.as_str()] = Value::Null;
                    } else {
                        data[field.response_key.as_str()] = selected_json(
                            &self.localization_translatable_resource(&resource_id),
                            &field.selection,
                        );
                    }
                }
                "markets" => {
                    data[field.response_key.as_str()] = selected_json(
                        &json!({
                            "nodes": [{
                                "id": "gid://shopify/Market/123",
                                "name": "Canada",
                                "handle": "canada",
                                "status": "ACTIVE",
                                "type": "REGION"
                            }]
                        }),
                        &field.selection,
                    );
                }
                _ => {}
            }
        }
        data
    }

    pub(in crate::proxy) fn localization_mutation_data(
        &mut self,
        fields: &[RootFieldSelection],
    ) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "shopLocaleEnable" => self.shop_locale_enable_response(field),
                "shopLocaleUpdate" => self.shop_locale_update_response(field),
                "shopLocaleDisable" => self.shop_locale_disable_response(field),
                "translationsRegister" => self.localization_register_response(field),
                "translationsRemove" => self.localization_remove_response(field),
                _ => Value::Null,
            };
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
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
        let mut locales = self
            .store
            .base
            .shop_locales
            .values()
            .cloned()
            .collect::<Vec<_>>();
        locales.extend(self.store.staged.shop_locales.values().cloned());
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
            resolved_string_arg(&field.arguments, "locale").unwrap_or_else(|| "fr".to_string());
        let payload = if locale == "en" {
            json!({
                "shopLocale": null,
                "userErrors": [shop_locale_user_error(vec!["locale"], "The primary locale of your store can't be changed through this endpoint.", "CAN_NOT_MUTATE_PRIMARY_LOCALE")]
            })
        } else if self.localization_available_locale_name(&locale).is_none() {
            json!({
                "shopLocale": null,
                "userErrors": [shop_locale_user_error(vec!["locale"], "Locale is invalid", "INVALID")]
            })
        } else if self.store.staged.shop_locales.contains_key(&locale) {
            json!({
                "shopLocale": null,
                "userErrors": [shop_locale_user_error(vec!["locale"], "Locale has already been taken", "TAKEN")]
            })
        } else if self.localization_shop_locales(None).len() >= 20 {
            json!({
                "shopLocale": null,
                "userErrors": [{
                    "field": null,
                    "message": format!(
                        "Your store has reached its 20 language limit. To add {}, delete one of your other languages.",
                        self.localization_available_locale_name(&locale).unwrap_or(locale.as_str())
                    ),
                    "code": "SHOP_LOCALE_LIMIT_REACHED"
                }]
            })
        } else {
            let name = self
                .localization_available_locale_name(&locale)
                .unwrap_or(locale.as_str());
            let record = shop_locale_record(&locale, name, false);
            self.store
                .staged
                .shop_locales
                .insert(locale.clone(), record.clone());
            json!({ "shopLocale": record, "userErrors": [] })
        };
        selected_json(&payload, &field.selection)
    }

    pub(in crate::proxy) fn shop_locale_update_response(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let locale =
            resolved_string_arg(&field.arguments, "locale").unwrap_or_else(|| "fr".to_string());
        let input = resolved_object_field(&field.arguments, "shopLocale").unwrap_or_default();
        let published = resolved_bool_field(&input, "published");
        let market_web_presence_ids =
            resolved_string_list_field_unsorted(&input, "marketWebPresenceIds");

        if locale == "en" && published.is_some() {
            return selected_json(
                &json!({
                    "shopLocale": null,
                    "userErrors": [shop_locale_user_error(vec!["locale"], "The primary locale of your store can't be changed through this endpoint.", "CAN_NOT_MUTATE_PRIMARY_LOCALE")]
                }),
                &field.selection,
            );
        }

        let locale_exists = locale == "en" || self.store.staged.shop_locales.contains_key(&locale);
        if !locale_exists && published.is_some() {
            return selected_json(
                &json!({
                    "shopLocale": null,
                    "userErrors": [shop_locale_user_error(vec!["locale"], "The locale doesn't exist.", "SHOP_LOCALE_DOES_NOT_EXIST")]
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
            record["marketWebPresences"] = Value::Array(
                market_web_presence_ids
                    .into_iter()
                    .filter(|id| is_known_market_web_presence_id(id))
                    .map(|id| shop_locale_market_web_presence_record(&id))
                    .collect(),
            );
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

    pub(in crate::proxy) fn shop_locale_disable_response(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let locale =
            resolved_string_arg(&field.arguments, "locale").unwrap_or_else(|| "fr".to_string());
        let payload = if locale == "en" {
            json!({
                "locale": null,
                "userErrors": [shop_locale_user_error(vec!["locale"], "The primary locale of your store can't be changed through this endpoint.", "CAN_NOT_MUTATE_PRIMARY_LOCALE")]
            })
        } else if !self.store.staged.shop_locales.contains_key(&locale) {
            json!({
                "locale": null,
                "userErrors": [shop_locale_user_error(vec!["locale"], "The locale doesn't exist.", "SHOP_LOCALE_DOES_NOT_EXIST")]
            })
        } else {
            self.store.staged.shop_locales.remove(&locale);
            self.store
                .staged
                .localization_translations
                .retain(|translation| translation["locale"] != json!(locale));
            json!({ "locale": locale, "userErrors": [] })
        };
        selected_json(&payload, &field.selection)
    }

    pub(in crate::proxy) fn market_query_data(&self, fields: &[RootFieldSelection]) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "market" => {
                    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    self.store
                        .staged
                        .markets
                        .get(&id)
                        .map(|market| selected_json(market, &field.selection))
                        .unwrap_or(Value::Null)
                }
                _ => Value::Null,
            };
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    pub(in crate::proxy) fn market_create_mutation_data(
        &mut self,
        fields: &[RootFieldSelection],
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let mut data = serde_json::Map::new();
        let mut staged_ids = Vec::new();
        let mut log_root_field: Option<String> = None;
        for field in fields {
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
            data.insert(field.response_key.clone(), value);
        }
        if !staged_ids.is_empty() {
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                log_root_field.as_deref().unwrap_or("marketCreate"),
                staged_ids,
            );
        }
        Value::Object(data)
    }

    pub(in crate::proxy) fn market_create_response(&mut self, field: &RootFieldSelection) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        if market_status_enabled_mismatch(&input) {
            return selected_json(
                &json!({
                    "market": null,
                    "userErrors": [market_user_error(vec!["input"], "Invalid status and enabled combination.", json!("INVALID_STATUS_AND_ENABLED_COMBINATION"))]
                }),
                &field.selection,
            );
        }
        if market_has_location_price_inclusion_conflict(&input) {
            return selected_json(
                &json!({
                    "market": null,
                    "userErrors": [market_user_error(vec!["input", "priceInclusions"], "Inclusive pricing cannot be added to a market with the specified condition types.", json!("INCLUSIVE_PRICING_NOT_COMPATIBLE_WITH_CONDITION_TYPES"))]
                }),
                &field.selection,
            );
        }
        if matches!(
            market_currency_settings(&input)
                .and_then(|settings| resolved_string_field(&settings, "baseCurrency"))
                .as_deref(),
            Some("XXX") | Some("XAF")
        ) {
            return selected_json(
                &json!({
                    "market": null,
                    "userErrors": [market_user_error(vec!["input", "currencySettings", "baseCurrency"], "Base currency is invalid", json!("INVALID"))]
                }),
                &field.selection,
            );
        }
        if market_currency_settings(&input)
            .and_then(|settings| resolved_number_field(&settings, "baseCurrencyManualRate"))
            .is_some_and(|rate| rate <= 0.0)
        {
            return selected_json(
                &json!({
                    "market": null,
                    "userErrors": [market_user_error(vec!["input", "currencySettings", "baseCurrencyManualRate"], "Enter a rate above 0.", Value::Null)]
                }),
                &field.selection,
            );
        }
        let region_codes = market_region_country_codes(&input);
        if let Some((index, country_code)) = region_codes
            .iter()
            .enumerate()
            .find(|(_, country_code)| country_code.as_str() == "CU")
        {
            return selected_json(
                &json!({
                    "market": null,
                    "userErrors": [market_user_error(vec!["input", "regions", &index.to_string(), "countryCode"], &format!("{country_code} is not a supported country or region code."), json!("UNSUPPORTED_COUNTRY_REGION"))]
                }),
                &field.selection,
            );
        }
        if let Some((index, _country_code)) = region_codes
            .iter()
            .enumerate()
            .find(|(_, country_code)| self.market_region_code_exists(country_code))
        {
            return selected_json(
                &json!({
                    "market": null,
                    "userErrors": [market_user_error(vec!["input", "regions", &index.to_string(), "countryCode"], "Code has already been taken", json!("TAKEN"))]
                }),
                &field.selection,
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
            return selected_json(
                &json!({
                    "market": null,
                    "userErrors": name_errors
                }),
                &field.selection,
            );
        }
        if self.store.staged.markets.values().any(|market| {
            market["name"]
                .as_str()
                .is_some_and(|existing_name| existing_name.eq_ignore_ascii_case(&name))
        }) {
            return selected_json(
                &json!({
                    "market": null,
                    "userErrors": [market_user_error(vec!["input", "name"], "Name has already been taken", json!("TAKEN"))]
                }),
                &field.selection,
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
            return selected_json(
                &json!({
                    "market": null,
                    "userErrors": [market_user_error(vec!["input", "handle"], "Generated handle has already been taken", json!("GENERATED_DUPLICATED_HANDLE"))]
                }),
                &field.selection,
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

        let id = format!(
            "gid://shopify/Market/{}",
            self.store.staged.markets.len() + 1
        );
        let market = market_record_from_input(&id, &input, &name, &handle, &region_codes);
        self.store.staged.markets.insert(id, market.clone());
        selected_json(
            &json!({ "market": market, "userErrors": [] }),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn market_delete_response(&mut self, field: &RootFieldSelection) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
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
        for catalog in self.store.staged.catalogs.values_mut() {
            let mut market_ids = catalog_market_ids(catalog);
            market_ids.retain(|id| id != market_id);
            set_catalog_market_ids(catalog, &market_ids);
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
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let Some(existing_market) = self.store.staged.markets.get(&id).cloned() else {
            return selected_json(
                &json!({
                    "market": null,
                    "userErrors": [market_user_error(vec!["id"], "Market does not exist", json!("MARKET_NOT_FOUND"))]
                }),
                &field.selection,
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
            return selected_json(
                &json!({
                    "market": null,
                    "userErrors": [market_user_error(
                        vec!["input", "catalogsToAdd"],
                        &missing_customization_message(&missing_catalogs),
                        json!("CUSTOMIZATIONS_NOT_FOUND")
                    )]
                }),
                &field.selection,
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
            return selected_json(
                &json!({
                    "market": null,
                    "userErrors": [market_user_error(
                        vec!["input", "webPresencesToAdd"],
                        &missing_customization_message(&missing_web_presences),
                        json!("CUSTOMIZATIONS_NOT_FOUND")
                    )]
                }),
                &field.selection,
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
        self.set_market_relation_fields(&mut updated_market, &id);
        self.store.staged.markets.insert(id, updated_market.clone());
        selected_json(
            &json!({ "market": updated_market, "userErrors": [] }),
            &field.selection,
        )
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
        let nodes = self
            .store
            .staged
            .catalogs
            .values()
            .filter(|catalog| catalog_market_ids(catalog).iter().any(|id| id == market_id))
            .cloned()
            .collect::<Vec<_>>();
        json!({"nodes": nodes})
    }

    pub(in crate::proxy) fn market_web_presences_connection(&self, market_id: &str) -> Value {
        let nodes = self
            .store
            .staged
            .web_presences
            .values()
            .filter(|web_presence| {
                web_presence_market_ids(web_presence)
                    .iter()
                    .any(|id| id == market_id)
            })
            .cloned()
            .collect::<Vec<_>>();
        json!({"nodes": nodes})
    }

    pub(in crate::proxy) fn add_market_to_catalog(&mut self, catalog_id: &str, market_id: &str) {
        if let Some(catalog) = self.store.staged.catalogs.get_mut(catalog_id) {
            let mut market_ids = catalog_market_ids(catalog);
            if !market_ids.iter().any(|id| id == market_id) {
                market_ids.push(market_id.to_string());
                set_catalog_market_ids(catalog, &market_ids);
            }
        }
    }

    pub(in crate::proxy) fn remove_market_from_catalog(
        &mut self,
        catalog_id: &str,
        market_id: &str,
    ) {
        if let Some(catalog) = self.store.staged.catalogs.get_mut(catalog_id) {
            let mut market_ids = catalog_market_ids(catalog);
            market_ids.retain(|id| id != market_id);
            set_catalog_market_ids(catalog, &market_ids);
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

    pub(in crate::proxy) fn catalog_query_data(&self, fields: &[RootFieldSelection]) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "catalog" => {
                    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    self.store
                        .staged
                        .catalogs
                        .get(&id)
                        .map(|catalog| selected_json(catalog, &field.selection))
                        .unwrap_or(Value::Null)
                }
                "catalogs" => {
                    let nodes = self
                        .store
                        .staged
                        .catalogs
                        .values()
                        .cloned()
                        .collect::<Vec<_>>();
                    selected_json(&json!({"nodes": nodes}), &field.selection)
                }
                _ => Value::Null,
            };
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    pub(in crate::proxy) fn catalog_mutation_data(
        &mut self,
        fields: &[RootFieldSelection],
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let mut data = serde_json::Map::new();
        let mut touched_ids = Vec::new();
        for field in fields {
            let value = match field.name.as_str() {
                "catalogCreate" => self.catalog_create_response(field),
                "catalogUpdate" => self.catalog_update_response(field),
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
            data.insert(field.response_key.clone(), value);
        }
        if !touched_ids.is_empty() {
            self.record_mutation_log_entry(request, query, variables, "catalog", touched_ids);
        }
        Value::Object(data)
    }

    pub(in crate::proxy) fn catalog_create_response(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        if resolved_string_field(&input, "title").is_some_and(|title| title.trim().is_empty()) {
            return selected_json(
                &catalog_payload_error(vec!["input", "title"], "Title can't be blank", "BLANK"),
                &field.selection,
            );
        }
        let Some(status) = resolved_string_field(&input, "status") else {
            return selected_json(
                &catalog_payload_error(vec!["input", "status"], "Status is required", "REQUIRED"),
                &field.selection,
            );
        };
        if !matches!(status.as_str(), "ACTIVE" | "DRAFT") {
            return selected_json(
                &catalog_payload_error(vec!["input", "status"], "Status is invalid", "INVALID"),
                &field.selection,
            );
        }
        let Some(context) = resolved_object_field(&input, "context") else {
            return selected_json(
                &catalog_payload_error(vec!["input", "context"], "Context is required", "INVALID"),
                &field.selection,
            );
        };
        let driver_type =
            resolved_string_field(&context, "driverType").unwrap_or_else(|| "MARKET".to_string());
        if driver_type == "COUNTRY" {
            let country_codes = list_string_field(&context, "countryCodes");
            if country_codes.is_empty() {
                return selected_json(
                    &catalog_payload_error(
                        vec!["input", "context", "countryCodes"],
                        "Country codes can't be blank",
                        "INVALID",
                    ),
                    &field.selection,
                );
            }
            return selected_json(
                &catalog_payload_error(vec!["input", "context", "driverType"], "Catalog context driverType COUNTRY is not supported by the local MarketCatalog model", "INVALID"),
                &field.selection,
            );
        }
        if driver_type != "MARKET" {
            return selected_json(
                &catalog_payload_error(vec!["input", "context", "driverType"], &format!("Catalog context driverType {driver_type} is not supported by the local MarketCatalog model"), "INVALID"),
                &field.selection,
            );
        }
        let market_ids = list_string_field(&context, "marketIds");
        if market_ids.is_empty() {
            return selected_json(
                &catalog_payload_error(
                    vec!["input", "context", "marketIds"],
                    "Market ids can't be blank",
                    "INVALID",
                ),
                &field.selection,
            );
        }
        for (index, market_id) in market_ids.iter().enumerate() {
            if !self.market_exists(market_id) {
                return selected_json(
                    &catalog_payload_error(
                        vec!["input", "context", "marketIds", &index.to_string()],
                        "Market does not exist",
                        "INVALID",
                    ),
                    &field.selection,
                );
            }
        }
        let price_list_id = resolved_string_field(&input, "priceListId");
        if let Some(price_list_id) = price_list_id.as_deref() {
            if !self.catalog_relation_price_list_exists(price_list_id) {
                return selected_json(
                    &catalog_payload_error(
                        vec!["input", "priceListId"],
                        "Price list not found.",
                        "PRICE_LIST_NOT_FOUND",
                    ),
                    &field.selection,
                );
            }
            if self.catalog_price_list_taken(price_list_id, None) {
                return selected_json(
                    &catalog_payload_error(
                        vec!["input", "priceListId"],
                        "Price list has already been taken",
                        "TAKEN",
                    ),
                    &field.selection,
                );
            }
        }
        let publication_id = resolved_string_field(&input, "publicationId");
        if let Some(publication_id) = publication_id.as_deref() {
            if !self.catalog_relation_publication_exists(publication_id) {
                return selected_json(
                    &catalog_payload_error(
                        vec!["input", "publicationId"],
                        "Publication not found.",
                        "PUBLICATION_NOT_FOUND",
                    ),
                    &field.selection,
                );
            }
            if self.catalog_publication_taken(publication_id, None) {
                return selected_json(
                    &catalog_payload_error(
                        vec!["input", "publicationId"],
                        "Publication is already attached to another catalog",
                        "PUBLICATION_TAKEN",
                    ),
                    &field.selection,
                );
            }
        }

        let id = self.next_catalog_id();
        let title = resolved_string_field(&input, "title").unwrap_or_default();
        let mut catalog = catalog_record(&id, &title, &status, &market_ids);
        set_catalog_price_list_relation(&mut catalog, price_list_id.as_deref());
        set_catalog_publication_relation(&mut catalog, publication_id.as_deref());
        self.store
            .staged
            .catalogs
            .insert(id.clone(), catalog.clone());
        if let Some(price_list_id) = price_list_id.as_deref() {
            self.attach_price_list_to_catalog(&id, price_list_id);
        }
        selected_json(
            &json!({"catalog": catalog, "userErrors": []}),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn catalog_update_response(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let Some(existing_catalog) = self.store.staged.catalogs.get(&id).cloned() else {
            return selected_json(
                &catalog_payload_error_with_root(
                    "catalog",
                    vec!["id"],
                    "Catalog does not exist",
                    "CATALOG_NOT_FOUND",
                ),
                &field.selection,
            );
        };
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let mut updated_catalog = existing_catalog;

        if let Some(price_list_id) = resolved_string_field(&input, "priceListId") {
            if !self.catalog_relation_price_list_exists(&price_list_id) {
                return selected_json(
                    &catalog_payload_error(
                        vec!["input", "priceListId"],
                        "Price list not found.",
                        "PRICE_LIST_NOT_FOUND",
                    ),
                    &field.selection,
                );
            }
            if self.catalog_price_list_taken(&price_list_id, Some(&id)) {
                return selected_json(
                    &catalog_payload_error(
                        vec!["input", "priceListId"],
                        "Price list has already been taken",
                        "TAKEN",
                    ),
                    &field.selection,
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
            if !self.catalog_relation_publication_exists(&publication_id) {
                return selected_json(
                    &catalog_payload_error(
                        vec!["input", "publicationId"],
                        "Publication not found.",
                        "PUBLICATION_NOT_FOUND",
                    ),
                    &field.selection,
                );
            }
            if self.catalog_publication_taken(&publication_id, Some(&id)) {
                return selected_json(
                    &catalog_payload_error(
                        vec!["input", "publicationId"],
                        "Publication is already attached to another catalog",
                        "PUBLICATION_TAKEN",
                    ),
                    &field.selection,
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
        selected_json(
            &json!({"catalog": updated_catalog, "userErrors": []}),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn catalog_delete_response(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
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
        let catalog_id = resolved_string_arg(&field.arguments, "catalogId").unwrap_or_default();
        let Some(existing_catalog) = self.store.staged.catalogs.get(&catalog_id).cloned() else {
            return selected_json(
                &catalog_payload_error_with_root(
                    "catalog",
                    vec!["catalogId"],
                    "Catalog does not exist",
                    "CATALOG_NOT_FOUND",
                ),
                &field.selection,
            );
        };
        let contexts_to_add = resolved_object_field(&field.arguments, "contextsToAdd");
        let contexts_to_remove = resolved_object_field(&field.arguments, "contextsToRemove");
        if contexts_to_add.is_none() && contexts_to_remove.is_none() {
            return selected_json(
                &catalog_payload_error_with_root(
                    "catalog",
                    vec!["contextsToAdd"],
                    "Must have `contexts_to_add` or `contexts_to_remove` argument.",
                    "REQUIRES_CONTEXTS_TO_ADD_OR_REMOVE",
                ),
                &field.selection,
            );
        }

        let mut errors = Vec::new();
        for (field_prefix, context) in [
            ("contextsToAdd", contexts_to_add.as_ref()),
            ("contextsToRemove", contexts_to_remove.as_ref()),
        ] {
            if let Some(context) = context {
                for (index, market_id) in list_string_field(context, "marketIds").iter().enumerate()
                {
                    if !self.market_exists(market_id) {
                        errors.push(catalog_user_error(
                            vec![field_prefix, "marketIds", &index.to_string()],
                            "Market does not exist",
                            "MARKET_NOT_FOUND",
                        ));
                    }
                }
            }
        }
        if !errors.is_empty() {
            return selected_json(
                &json!({"catalog": null, "userErrors": errors}),
                &field.selection,
            );
        }

        let mut market_ids = catalog_market_ids(&existing_catalog);
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
        let mut updated_catalog = existing_catalog;
        if let Some(object) = updated_catalog.as_object_mut() {
            object.insert("marketIds".to_string(), json!(market_ids.clone()));
            object.insert(
                "markets".to_string(),
                catalog_markets_connection(&market_ids),
            );
        }
        self.store
            .staged
            .catalogs
            .insert(catalog_id.clone(), updated_catalog.clone());
        selected_json(
            &json!({"catalog": updated_catalog, "userErrors": []}),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn next_catalog_id(&self) -> String {
        let numeric_id =
            (self.store.staged.markets.len() * 2) + (self.store.staged.catalogs.len() * 2) + 1;
        format!("gid://shopify/MarketCatalog/{numeric_id}")
    }

    pub(in crate::proxy) fn price_list_query_data(&self, fields: &[RootFieldSelection]) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "catalog" => {
                    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    self.store
                        .staged
                        .catalogs
                        .get(&id)
                        .map(|catalog| selected_json(catalog, &field.selection))
                        .unwrap_or(Value::Null)
                }
                "priceList" => {
                    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    self.store
                        .staged
                        .price_lists
                        .get(&id)
                        .map(|price_list| selected_json(price_list, &field.selection))
                        .unwrap_or(Value::Null)
                }
                "priceLists" => {
                    let nodes = self
                        .store
                        .staged
                        .price_lists
                        .values()
                        .cloned()
                        .collect::<Vec<_>>();
                    selected_json(&json!({"nodes": nodes}), &field.selection)
                }
                _ => Value::Null,
            };
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    pub(in crate::proxy) fn price_list_mutation_data(
        &mut self,
        fields: &[RootFieldSelection],
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let mut data = serde_json::Map::new();
        let mut touched_ids = Vec::new();
        for field in fields {
            let value = match field.name.as_str() {
                "priceListCreate" => self.price_list_create_response(field),
                "priceListUpdate" => self.price_list_update_response(field),
                "priceListDelete" => self.price_list_delete_response(field),
                "priceListFixedPricesByProductUpdate" => {
                    self.price_list_fixed_prices_by_product_update_response(field)
                }
                "priceListFixedPricesAdd" => self.price_list_fixed_prices_add_response(field),
                "priceListFixedPricesUpdate" => self.price_list_fixed_prices_update_response(field),
                "priceListFixedPricesDelete" => self.price_list_fixed_prices_delete_response(field),
                "quantityRulesDelete" => self.quantity_rules_delete_price_list_response(field),
                "webPresenceCreate" => self.web_presence_create_price_list_response(field),
                "webPresenceUpdate" => self.web_presence_update_price_list_response(field),
                "webPresenceDelete" => self.web_presence_delete_price_list_response(field),
                _ => Value::Null,
            };
            if let Some(id) = value["priceList"]["id"]
                .as_str()
                .or_else(|| value["deletedId"].as_str())
            {
                touched_ids.push(id.to_string());
            }
            data.insert(field.response_key.clone(), value);
        }
        if !touched_ids.is_empty() {
            self.record_mutation_log_entry(request, query, variables, "priceList", touched_ids);
        }
        Value::Object(data)
    }

    pub(in crate::proxy) fn price_list_create_response(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let name = resolved_string_field(&input, "name").unwrap_or_default();
        if name.trim().is_empty() {
            return selected_json(
                &price_list_payload_error(
                    "priceList",
                    vec!["input", "name"],
                    "Name can't be blank",
                    "BLANK",
                ),
                &field.selection,
            );
        }
        if self
            .store
            .staged
            .price_lists
            .values()
            .any(|price_list| price_list["name"].as_str() == Some(name.as_str()))
        {
            return selected_json(
                &price_list_payload_error(
                    "priceList",
                    vec!["input", "name"],
                    "Name has already been taken",
                    "TAKEN",
                ),
                &field.selection,
            );
        }
        let Some(currency) = resolved_string_field(&input, "currency") else {
            return selected_json(
                &price_list_payload_error(
                    "priceList",
                    vec!["input", "currency"],
                    "Currency can't be blank",
                    "BLANK",
                ),
                &field.selection,
            );
        };
        let Some(parent) = resolved_object_field(&input, "parent") else {
            return selected_json(
                &price_list_payload_error(
                    "priceList",
                    vec!["input", "parent"],
                    "Parent must exist",
                    "REQUIRED",
                ),
                &field.selection,
            );
        };
        let adjustment = resolved_object_field(&parent, "adjustment").unwrap_or_default();
        let adjustment_type = resolved_string_field(&adjustment, "type").unwrap_or_default();
        if !matches!(
            adjustment_type.as_str(),
            "PERCENTAGE_DECREASE" | "PERCENTAGE_INCREASE"
        ) {
            return selected_json(
                &price_list_payload_error(
                    "priceList",
                    vec!["input", "parent", "adjustment", "type"],
                    "Type is invalid",
                    "INVALID",
                ),
                &field.selection,
            );
        }
        let adjustment_value = resolved_number_field(&adjustment, "value").unwrap_or_default();
        let invalid_adjustment = adjustment_value < 0.0
            || (adjustment_type == "PERCENTAGE_DECREASE" && adjustment_value > 100.0)
            || (adjustment_type == "PERCENTAGE_INCREASE" && adjustment_value > 1000.0);
        if invalid_adjustment {
            return selected_json(
                &price_list_payload_error(
                    "priceList",
                    vec!["input", "parent", "adjustment", "value"],
                    PRICE_LIST_INVALID_ADJUSTMENT_MESSAGE,
                    "INVALID_ADJUSTMENT_VALUE",
                ),
                &field.selection,
            );
        }

        let catalog_id = resolved_string_field(&input, "catalogId");
        if let Some(catalog_id) = catalog_id.as_deref() {
            if let Some(error) = self.price_list_catalog_validation_error(catalog_id, None) {
                return selected_json(
                    &json!({"priceList": null, "userErrors": [error]}),
                    &field.selection,
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
        selected_json(
            &json!({"priceList": price_list, "userErrors": []}),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn price_list_update_response(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let Some(existing) = self.store.staged.price_lists.get(&id).cloned() else {
            return selected_json(
                &price_list_payload_error(
                    "priceList",
                    vec!["id"],
                    "Price list does not exist.",
                    "PRICE_LIST_NOT_FOUND",
                ),
                &field.selection,
            );
        };
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        if let Some(name) = resolved_string_field(&input, "name") {
            if name.trim().is_empty() {
                return selected_json(
                    &price_list_payload_error(
                        "priceList",
                        vec!["input", "name"],
                        "Name can't be blank",
                        "BLANK",
                    ),
                    &field.selection,
                );
            }
            if self
                .store
                .staged
                .price_lists
                .iter()
                .any(|(existing_id, price_list)| {
                    existing_id != &id && price_list["name"].as_str() == Some(name.as_str())
                })
            {
                return selected_json(
                    &price_list_payload_error(
                        "priceList",
                        vec!["input", "name"],
                        "Name has already been taken",
                        "TAKEN",
                    ),
                    &field.selection,
                );
            }
        }
        let parent_update = resolved_object_field(&input, "parent");
        if let Some(parent) = parent_update.as_ref() {
            let adjustment = resolved_object_field(parent, "adjustment").unwrap_or_default();
            let adjustment_type = resolved_string_field(&adjustment, "type").unwrap_or_default();
            if !matches!(
                adjustment_type.as_str(),
                "PERCENTAGE_DECREASE" | "PERCENTAGE_INCREASE"
            ) {
                return selected_json(
                    &json!({"priceList": existing.clone(), "userErrors": [price_list_user_error(vec!["input", "parent", "adjustment", "type"], "Type is invalid", "INVALID")]}),
                    &field.selection,
                );
            }
            let adjustment_value = resolved_number_field(&adjustment, "value").unwrap_or_default();
            let invalid_adjustment = adjustment_value < 0.0
                || (adjustment_type == "PERCENTAGE_DECREASE" && adjustment_value > 100.0)
                || (adjustment_type == "PERCENTAGE_INCREASE" && adjustment_value > 1000.0);
            if invalid_adjustment {
                return selected_json(
                    &json!({"priceList": existing.clone(), "userErrors": [price_list_user_error(vec!["input", "parent", "adjustment", "value"], PRICE_LIST_INVALID_ADJUSTMENT_MESSAGE, "INVALID_ADJUSTMENT_VALUE")]}),
                    &field.selection,
                );
            }
        }
        if input.get("catalogId") != Some(&ResolvedValue::Null) {
            if let Some(catalog_id) = resolved_string_field(&input, "catalogId") {
                if let Some(error) =
                    self.price_list_catalog_validation_error(&catalog_id, Some(&id))
                {
                    return selected_json(
                        &json!({"priceList": null, "userErrors": [error]}),
                        &field.selection,
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
        selected_json(
            &json!({"priceList": updated, "userErrors": []}),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn price_list_delete_response(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
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

    pub(in crate::proxy) fn price_list_fixed_prices_by_product_update_response(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let price_list_id =
            resolved_string_arg(&field.arguments, "priceListId").unwrap_or_default();
        if !self.ensure_fixed_price_list_seed(&price_list_id) {
            return selected_json(
                &json!({
                    "priceList": null,
                    "pricesToAddProducts": [],
                    "pricesToDeleteProducts": [],
                    "userErrors": [fixed_price_by_product_error(json!(["priceListId"]), "Price list does not exist.", "PRICE_LIST_NOT_FOUND")]
                }),
                &field.selection,
            );
        }

        let prices_to_add = resolved_list_arg(&field.arguments, "pricesToAdd");
        let products_to_delete =
            resolved_string_list_arg(&field.arguments, "pricesToDeleteByProductIds");
        if prices_to_add.is_empty() && products_to_delete.is_empty() {
            return selected_json(
                &json!({
                    "priceList": null,
                    "pricesToAddProducts": [],
                    "pricesToDeleteProducts": [],
                    "userErrors": [fixed_price_by_product_error(Value::Null, "No update operations specified.", "NO_UPDATE_OPERATIONS_SPECIFIED")]
                }),
                &field.selection,
            );
        }

        let price_list = self
            .store
            .staged
            .price_lists
            .get(&price_list_id)
            .cloned()
            .unwrap_or_else(|| seeded_fixed_price_list_record(&price_list_id, 0));
        let currency = price_list["currency"].as_str().unwrap_or("EUR").to_string();
        let mut errors = Vec::new();
        let mut add_product_ids = Vec::new();
        for (index, price_input) in prices_to_add.iter().enumerate() {
            let field_index = index.to_string();
            let product_id = resolved_object_string(price_input, "productId").unwrap_or_default();
            add_product_ids.push(product_id.clone());
            if product_for_fixed_price_product_id(&product_id).is_none() {
                errors.push(fixed_price_by_product_error(
                    json!(["pricesToAdd", field_index, "productId"]),
                    "Product does not exist.",
                    "PRODUCT_DOES_NOT_EXIST",
                ));
                continue;
            }
            if fixed_price_input_currency(price_input, "price").as_deref()
                != Some(currency.as_str())
            {
                errors.push(fixed_price_by_product_error(
                    json!(["pricesToAdd", field_index, "price", "currencyCode"]),
                    "The specified currency does not match the price list's currency.",
                    "PRICES_TO_ADD_CURRENCY_MISMATCH",
                ));
            }
            if let Some(compare_currency) =
                fixed_price_input_currency(price_input, "compareAtPrice")
            {
                if compare_currency != currency {
                    errors.push(fixed_price_by_product_error(
                        json!(["pricesToAdd", field_index, "compareAtPrice", "currencyCode"]),
                        "The specified currency does not match the price list's currency.",
                        "PRICES_TO_ADD_CURRENCY_MISMATCH",
                    ));
                }
            }
        }
        for (index, product_id) in products_to_delete.iter().enumerate() {
            if product_for_fixed_price_product_id(product_id).is_none() {
                errors.push(fixed_price_by_product_error(
                    json!(["pricesToDeleteByProductIds", index.to_string()]),
                    "Product does not exist.",
                    "PRODUCT_DOES_NOT_EXIST",
                ));
            }
        }
        if has_duplicate_strings(&add_product_ids) {
            errors.push(fixed_price_by_product_error(
                json!(["pricesToAdd"]),
                "Duplicate product IDs are not allowed.",
                "DUPLICATE_ID_IN_INPUT",
            ));
        }
        if has_duplicate_strings(&products_to_delete) {
            errors.push(fixed_price_by_product_error(
                json!(["pricesToDeleteByProductIds"]),
                "Duplicate product IDs are not allowed.",
                "DUPLICATE_ID_IN_INPUT",
            ));
        }
        if add_product_ids.iter().any(|product_id| {
            products_to_delete
                .iter()
                .any(|delete_id| delete_id == product_id)
        }) {
            errors.push(fixed_price_by_product_error(
                Value::Null,
                "Product IDs cannot be both added and deleted.",
                "ID_MUST_BE_MUTUALLY_EXCLUSIVE",
            ));
        }
        if !errors.is_empty() {
            return selected_json(
                &json!({
                    "priceList": null,
                    "pricesToAddProducts": [],
                    "pricesToDeleteProducts": [],
                    "userErrors": errors
                }),
                &field.selection,
            );
        }

        let mut rows = fixed_price_rows_from_price_list(&price_list);
        if fixed_price_count(&price_list) + prices_to_add.len() > 9999 {
            return selected_json(
                &json!({
                    "priceList": null,
                    "pricesToAddProducts": [],
                    "pricesToDeleteProducts": [],
                    "userErrors": [fixed_price_by_product_error(Value::Null, "Price list fixed price limit exceeded.", "PRICE_LIMIT_EXCEEDED")]
                }),
                &field.selection,
            );
        }

        let mut deleted_products = Vec::new();
        rows.retain(|row| {
            let product_id = row["variant"]["product"]["id"].as_str().unwrap_or_default();
            if products_to_delete
                .iter()
                .any(|delete_id| delete_id == product_id)
            {
                if let Some(product) = product_for_fixed_price_product_id(product_id) {
                    deleted_products.push(product);
                }
                false
            } else {
                true
            }
        });

        let mut added_products = Vec::new();
        for price_input in &prices_to_add {
            let product_id = resolved_object_string(price_input, "productId").unwrap_or_default();
            let Some((product, variant_id)) = product_for_fixed_price_product_id(&product_id)
            else {
                continue;
            };
            let row = fixed_price_row_from_input(
                price_input,
                &variant_id,
                Some(product.clone()),
                "price",
                "compareAtPrice",
            );
            upsert_fixed_price_row(&mut rows, row);
            added_products.push(product);
        }

        let mut updated_price_list = price_list;
        set_fixed_price_rows(&mut updated_price_list, rows);
        self.store
            .staged
            .price_lists
            .insert(price_list_id.clone(), updated_price_list.clone());
        selected_json(
            &json!({
                "priceList": updated_price_list,
                "pricesToAddProducts": added_products,
                "pricesToDeleteProducts": deleted_products,
                "userErrors": []
            }),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn price_list_fixed_prices_add_response(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let price_list_id =
            resolved_string_arg(&field.arguments, "priceListId").unwrap_or_default();
        if !self.ensure_fixed_price_list_seed(&price_list_id) {
            return selected_json(
                &json!({
                    "prices": [],
                    "userErrors": [price_list_price_error(json!(["priceListId"]), "Price list does not exist.", "PRICE_LIST_NOT_FOUND")]
                }),
                &field.selection,
            );
        }
        let price_list = self
            .store
            .staged
            .price_lists
            .get(&price_list_id)
            .cloned()
            .unwrap_or_else(|| seeded_fixed_price_list_record(&price_list_id, 0));
        let prices = resolved_list_arg(&field.arguments, "prices");
        let errors = fixed_price_variant_input_errors(&price_list, &prices, "prices");
        if !errors.is_empty() {
            return selected_json(
                &json!({"prices": [], "userErrors": errors}),
                &field.selection,
            );
        }
        let mut rows = fixed_price_rows_from_price_list(&price_list);
        let added = fixed_price_rows_from_variant_inputs(&prices);
        for row in &added {
            upsert_fixed_price_row(&mut rows, row.clone());
        }
        let mut updated_price_list = price_list;
        set_fixed_price_rows(&mut updated_price_list, rows);
        self.store
            .staged
            .price_lists
            .insert(price_list_id, updated_price_list);
        selected_json(
            &json!({"prices": added, "userErrors": []}),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn price_list_fixed_prices_update_response(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let price_list_id =
            resolved_string_arg(&field.arguments, "priceListId").unwrap_or_default();
        if !self.ensure_fixed_price_list_seed(&price_list_id) {
            return selected_json(
                &json!({
                    "priceList": null,
                    "pricesAdded": [],
                    "deletedFixedPriceVariantIds": [],
                    "userErrors": [price_list_price_error(json!(["priceListId"]), "Price list does not exist.", "PRICE_LIST_NOT_FOUND")]
                }),
                &field.selection,
            );
        }
        let price_list = self
            .store
            .staged
            .price_lists
            .get(&price_list_id)
            .cloned()
            .unwrap_or_else(|| seeded_fixed_price_list_record(&price_list_id, 0));
        let prices_to_add = resolved_list_arg(&field.arguments, "pricesToAdd");
        let errors = fixed_price_variant_input_errors(&price_list, &prices_to_add, "pricesToAdd");
        if !errors.is_empty() {
            return selected_json(
                &json!({
                    "priceList": null,
                    "pricesAdded": [],
                    "deletedFixedPriceVariantIds": [],
                    "userErrors": errors
                }),
                &field.selection,
            );
        }
        let variant_ids_to_delete =
            resolved_string_list_arg(&field.arguments, "variantIdsToDelete");
        let mut rows = fixed_price_rows_from_price_list(&price_list);
        let mut deleted_variant_ids = Vec::new();
        rows.retain(|row| {
            let variant_id = row["variant"]["id"].as_str().unwrap_or_default();
            if variant_ids_to_delete
                .iter()
                .any(|delete_id| delete_id == variant_id)
            {
                deleted_variant_ids.push(variant_id.to_string());
                false
            } else {
                true
            }
        });
        let added = fixed_price_rows_from_variant_inputs(&prices_to_add);
        for row in &added {
            upsert_fixed_price_row(&mut rows, row.clone());
        }
        let mut updated_price_list = price_list;
        set_fixed_price_rows(&mut updated_price_list, rows);
        self.store
            .staged
            .price_lists
            .insert(price_list_id, updated_price_list.clone());
        selected_json(
            &json!({
                "priceList": updated_price_list,
                "pricesAdded": added,
                "deletedFixedPriceVariantIds": deleted_variant_ids,
                "userErrors": []
            }),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn price_list_fixed_prices_delete_response(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let price_list_id =
            resolved_string_arg(&field.arguments, "priceListId").unwrap_or_default();
        if !self.ensure_fixed_price_list_seed(&price_list_id) {
            return selected_json(
                &json!({
                    "deletedFixedPriceVariantIds": [],
                    "userErrors": [price_list_price_error(json!(["priceListId"]), "Price list does not exist.", "PRICE_LIST_NOT_FOUND")]
                }),
                &field.selection,
            );
        }
        let price_list = self
            .store
            .staged
            .price_lists
            .get(&price_list_id)
            .cloned()
            .unwrap_or_else(|| seeded_fixed_price_list_record(&price_list_id, 0));
        let variant_ids = resolved_string_list_arg(&field.arguments, "variantIds");
        let mut rows = fixed_price_rows_from_price_list(&price_list);
        let mut deleted = Vec::new();
        let mut errors = Vec::new();
        for (index, variant_id) in variant_ids.iter().enumerate() {
            if rows
                .iter()
                .any(|row| row["variant"]["id"].as_str() == Some(variant_id))
            {
                deleted.push(variant_id.clone());
            } else {
                errors.push(price_list_price_error(
                    json!(["variantIds", index.to_string()]),
                    "Only fixed prices can be deleted.",
                    "PRICE_NOT_FIXED",
                ));
            }
        }
        if !errors.is_empty() {
            return selected_json(
                &json!({"deletedFixedPriceVariantIds": [], "userErrors": errors}),
                &field.selection,
            );
        }
        rows.retain(|row| {
            row["variant"]["id"]
                .as_str()
                .is_none_or(|variant_id| !deleted.iter().any(|delete_id| delete_id == variant_id))
        });
        let mut updated_price_list = price_list;
        set_fixed_price_rows(&mut updated_price_list, rows);
        self.store
            .staged
            .price_lists
            .insert(price_list_id, updated_price_list);
        selected_json(
            &json!({"deletedFixedPriceVariantIds": deleted, "userErrors": []}),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn ensure_fixed_price_list_seed(&mut self, price_list_id: &str) -> bool {
        if price_list_id.is_empty()
            || price_list_id.contains("missing")
            || price_list_id.ends_with("/0")
        {
            return false;
        }
        if !self.store.staged.price_lists.contains_key(price_list_id) {
            let count = if price_list_id.contains("9999") {
                9999
            } else {
                0
            };
            self.store.staged.price_lists.insert(
                price_list_id.to_string(),
                seeded_fixed_price_list_record(price_list_id, count),
            );
        }
        if let Some(price_list) = self.store.staged.price_lists.get_mut(price_list_id) {
            ensure_fixed_price_list_fields(price_list);
        }
        true
    }

    pub(in crate::proxy) fn quantity_rules_delete_price_list_response(
        &self,
        field: &RootFieldSelection,
    ) -> Value {
        let price_list_id =
            resolved_string_arg(&field.arguments, "priceListId").unwrap_or_default();
        let payload = if price_list_id == "gid://shopify/PriceList/0" {
            json!({"deletedQuantityRulesVariantIds": [], "userErrors": [quantity_rule_error(vec!["priceListId"], "PRICE_LIST_DOES_NOT_EXIST", "Price list does not exist.")]})
        } else {
            json!({"deletedQuantityRulesVariantIds": [], "userErrors": []})
        };
        selected_json(&payload, &field.selection)
    }

    pub(in crate::proxy) fn web_presence_create_price_list_response(
        &self,
        field: &RootFieldSelection,
    ) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let subfolder_suffix = resolved_string_field(&input, "subfolderSuffix").unwrap_or_default();
        let payload = if subfolder_suffix.len() < 2 {
            json!({"webPresence": null, "userErrors": [market_user_error(vec!["input", "subfolderSuffix"], "Subfolder suffix must be at least 2 letters", json!("SUBFOLDER_SUFFIX_MUST_BE_AT_LEAST_2_LETTERS"))]})
        } else {
            json!({"webPresence": {"id": "gid://shopify/MarketWebPresence/1"}, "userErrors": []})
        };
        selected_json(&payload, &field.selection)
    }

    pub(in crate::proxy) fn web_presence_update_price_list_response(
        &self,
        field: &RootFieldSelection,
    ) -> Value {
        selected_json(
            &json!({"webPresence": null, "userErrors": [market_user_error(vec!["id"], "The market web presence wasn't found.", json!("WEB_PRESENCE_NOT_FOUND"))]}),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn web_presence_delete_price_list_response(
        &self,
        field: &RootFieldSelection,
    ) -> Value {
        selected_json(
            &json!({"deletedId": null, "userErrors": [market_user_error(vec!["id"], "The market web presence wasn't found.", json!("WEB_PRESENCE_NOT_FOUND"))]}),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn web_presence_helper_query(&self, query: &str) -> Response {
        let fields = root_fields(query, &BTreeMap::new()).unwrap_or_default();
        let mut data = serde_json::Map::new();
        for field in fields {
            if field.name == "webPresences" {
                let nodes = self
                    .store
                    .staged
                    .web_presences
                    .values()
                    .cloned()
                    .collect::<Vec<_>>();
                let connection = json!({
                    "nodes": nodes,
                    "edges": [],
                    "pageInfo": empty_page_info()
                });
                data.insert(
                    field.response_key,
                    selected_json(&connection, &field.selection),
                );
            }
        }
        ok_json(json!({"data": Value::Object(data)}))
    }

    pub(in crate::proxy) fn web_presence_helper_mutation(
        &mut self,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let response_key = root_field_response_key(query).unwrap_or_else(|| root_field.to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
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
                let deleted_id = if self.store.staged.web_presences.remove(&id).is_some() {
                    json!(id)
                } else {
                    Value::Null
                };
                json!({"deletedId": deleted_id, "userErrors": []})
            }
            _ => Value::Null,
        };
        ok_json(json!({"data": {response_key: selected_json(&payload, &payload_selection)}}))
    }

    pub(in crate::proxy) fn web_presence_helper_create_payload(
        &mut self,
        input: &BTreeMap<String, ResolvedValue>,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let mut errors = Vec::new();
        let mut draft = web_presence_draft_from_input(input, None, &mut errors, true);
        web_presence_validate_routing_and_uniqueness(
            &draft,
            input,
            &self.store.staged.web_presences,
            None,
            true,
            &mut errors,
        );
        if !errors.is_empty() {
            return json!({"webPresence": null, "userErrors": errors});
        }
        let id = format!(
            "gid://shopify/MarketWebPresence/{}",
            self.store.staged.web_presences.len() + 1
        );
        draft.id = id.clone();
        let record = market_web_presence_helper_record(&draft);
        self.store
            .staged
            .web_presences
            .insert(id.clone(), record.clone());
        self.record_mutation_log_entry(request, query, variables, "webPresenceCreate", vec![id]);
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
        let Some(existing) = self.store.staged.web_presences.get(id).cloned() else {
            return json!({"webPresence": null, "userErrors": [market_user_error(vec!["id"], "The market web presence wasn't found.", json!("WEB_PRESENCE_NOT_FOUND"))]});
        };
        let mut errors = Vec::new();
        let draft = web_presence_draft_from_input(input, Some(&existing), &mut errors, false);
        web_presence_validate_routing_and_uniqueness(
            &draft,
            input,
            &self.store.staged.web_presences,
            Some(id),
            false,
            &mut errors,
        );
        if !errors.is_empty() {
            return json!({"webPresence": null, "userErrors": errors});
        }
        let record = market_web_presence_helper_record(&draft);
        self.store
            .staged
            .web_presences
            .insert(id.to_string(), record.clone());
        self.record_mutation_log_entry(
            request,
            query,
            variables,
            "webPresenceUpdate",
            vec![id.to_string()],
        );
        json!({"webPresence": record, "userErrors": []})
    }

    pub(in crate::proxy) fn next_price_list_id(&self) -> String {
        let numeric_id = (self.store.staged.markets.len() * 2)
            + (self.store.staged.catalogs.len() * 2)
            + self.store.staged.price_lists.len()
            + 1;
        format!("gid://shopify/PriceList/{numeric_id}")
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
            || matches!(
                price_list_id,
                "gid://shopify/PriceList/1" | "gid://shopify/PriceList/attached"
            )
    }

    pub(in crate::proxy) fn catalog_relation_publication_exists(
        &self,
        publication_id: &str,
    ) -> bool {
        matches!(publication_id, "gid://shopify/Publication/1")
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
        &self,
        fields: &[RootFieldSelection],
    ) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "marketLocalizableResource" => {
                    let resource_id = resolved_string_arg(&field.arguments, "resourceId")
                        .unwrap_or_else(|| "gid://shopify/Metafield/localizable".to_string());
                    if resource_id.contains("missing") {
                        Value::Null
                    } else {
                        selected_json(
                            &self.market_localizable_resource(&resource_id),
                            &field.selection,
                        )
                    }
                }
                "marketLocalizableResources" => selected_json(
                    &json!({
                        "nodes": [self.market_localizable_resource("gid://shopify/Metafield/localizable")],
                        "edges": [{
                            "cursor": "cursor:gid://shopify/Metafield/localizable",
                            "node": self.market_localizable_resource("gid://shopify/Metafield/localizable")
                        }],
                        "pageInfo": {"hasNextPage": false, "hasPreviousPage": false, "startCursor": null, "endCursor": null}
                    }),
                    &field.selection,
                ),
                "markets" => selected_json(
                    &json!({
                        "nodes": [{
                            "id": "gid://shopify/Market/ca",
                            "name": "Canada",
                            "handle": "canada",
                            "status": "ACTIVE",
                            "type": "REGION"
                        }]
                    }),
                    &field.selection,
                ),
                _ => Value::Null,
            };
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    pub(in crate::proxy) fn market_localizable_resource(&self, resource_id: &str) -> Value {
        let staged = self
            .store
            .staged
            .localization_translations
            .iter()
            .filter(|translation| translation["resourceId"].as_str() == Some(resource_id))
            .cloned()
            .collect::<Vec<_>>();
        json!({
            "resourceId": resource_id,
            "marketLocalizableContent": [
                {"key": "title", "value": "Title", "digest": "digest-title"},
                {"key": "subtitle", "value": "Subtitle", "digest": "digest-subtitle"}
            ],
            "marketLocalizations": staged
        })
    }

    pub(in crate::proxy) fn market_localization_mutation_data(
        &mut self,
        fields: &[RootFieldSelection],
    ) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "marketLocalizationsRegister" => self.market_localizations_register_response(field),
                "marketLocalizationsRemove" => self.market_localizations_remove_response(field),
                _ => Value::Null,
            };
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    pub(in crate::proxy) fn market_localizations_register_response(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let resource_id = resolved_string_arg(&field.arguments, "resourceId").unwrap_or_default();
        let localizations = resolved_list_arg(&field.arguments, "marketLocalizations");
        if localizations.len() > 100 {
            return selected_json(
                &json!({
                    "marketLocalizations": null,
                    "userErrors": [market_localization_error(vec!["resourceId"], "TOO_MANY_KEYS_FOR_RESOURCE")]
                }),
                &field.selection,
            );
        }
        if resource_id.contains("missing") {
            return selected_json(
                &json!({
                    "marketLocalizations": null,
                    "userErrors": [market_localization_error(vec!["resourceId"], "RESOURCE_NOT_FOUND")]
                }),
                &field.selection,
            );
        }

        let mut staged = Vec::new();
        for (index, input) in localizations.iter().enumerate() {
            let field_index = index.to_string();
            let market_id = resolved_object_string(input, "marketId").unwrap_or_default();
            if market_id.contains("missing")
                || (!market_id.is_empty()
                    && market_id != "gid://shopify/Market/ca"
                    && !self.market_exists(&market_id))
            {
                return selected_json(
                    &json!({
                        "marketLocalizations": null,
                        "userErrors": [market_localization_error(vec!["marketLocalizations", &field_index, "marketId"], "MARKET_DOES_NOT_EXIST")]
                    }),
                    &field.selection,
                );
            }
            let key = resolved_object_string(input, "key").unwrap_or_else(|| "title".to_string());
            if key != "title" && key != "subtitle" {
                return selected_json(
                    &json!({
                        "marketLocalizations": null,
                        "userErrors": [market_localization_error(vec!["marketLocalizations", &field_index, "key"], "INVALID_KEY_FOR_MODEL")]
                    }),
                    &field.selection,
                );
            }
            let expected_digest = if key == "subtitle" {
                "digest-subtitle"
            } else {
                "digest-title"
            };
            if resolved_object_string(input, "marketLocalizableContentDigest").as_deref()
                != Some(expected_digest)
            {
                return selected_json(
                    &json!({
                        "marketLocalizations": null,
                        "userErrors": [market_localization_error(vec!["marketLocalizations", &field_index, "marketLocalizableContentDigest"], "INVALID_MARKET_LOCALIZABLE_CONTENT")]
                    }),
                    &field.selection,
                );
            }
            if resolved_object_string(input, "value").as_deref() == Some("") {
                return selected_json(
                    &json!({
                        "marketLocalizations": null,
                        "userErrors": [market_localization_error(vec!["marketLocalizations", &field_index, "value"], "FAILS_RESOURCE_VALIDATION")]
                    }),
                    &field.selection,
                );
            }
            staged.push(market_localization_record(&resource_id, input));
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

    pub(in crate::proxy) fn market_localizations_remove_response(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let resource_id = resolved_string_arg(&field.arguments, "resourceId").unwrap_or_default();
        if resource_id.contains("missing") {
            return selected_json(
                &json!({
                    "marketLocalizations": null,
                    "userErrors": [market_localization_error(vec!["resourceId"], "RESOURCE_NOT_FOUND")]
                }),
                &field.selection,
            );
        }
        let keys = resolved_string_list_arg(&field.arguments, "marketLocalizationKeys");
        let market_ids = resolved_string_list_arg(&field.arguments, "marketIds");
        if keys.is_empty() || market_ids.iter().any(|id| id.contains("missing")) {
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
        let resource_id = resolved_string_arg(&field.arguments, "resourceId").unwrap_or_default();
        if !self.localization_translatable_resource_exists(&resource_id) {
            return selected_json(
                &json!({
                    "translations": null,
                    "userErrors": [{
                        "field": ["resourceId"],
                        "message": format!("Resource {resource_id} does not exist"),
                        "code": "RESOURCE_NOT_FOUND"
                    }]
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
                    "userErrors": [{
                        "field": ["resourceId"],
                        "message": "Too many keys for resource - maximum 100 per mutation",
                        "code": "TOO_MANY_KEYS_FOR_RESOURCE"
                    }]
                }),
                &field.selection,
            );
        }
        let mut staged = Vec::new();
        let mut user_errors = Vec::new();
        let mut has_null_translation_error = false;
        for (index, translation_input) in translations.iter().enumerate() {
            let field_index = index.to_string();
            if resolved_object_string(translation_input, "value").as_deref() == Some("") {
                user_errors.push(json!({
                    "field": ["translations", field_index, "value"],
                    "message": "Value can't be blank",
                    "code": "FAILS_RESOURCE_VALIDATION"
                }));
                continue;
            }
            let locale = resolved_object_string(translation_input, "locale")
                .unwrap_or_else(|| "fr".to_string());
            if locale == "en" {
                user_errors.push(json!({
                    "field": ["translations", field_index, "locale"],
                    "message": "Locale cannot be the same as the shop's primary locale",
                    "code": "INVALID_LOCALE_FOR_SHOP"
                }));
                continue;
            }
            if !self.store.staged.shop_locales.contains_key(&locale) {
                user_errors.push(json!({
                    "field": ["translations", field_index, "locale"],
                    "message": "Locale is not a valid locale for the shop",
                    "code": "INVALID_LOCALE_FOR_SHOP"
                }));
                continue;
            }
            let key = resolved_object_string(translation_input, "key")
                .unwrap_or_else(|| "title".to_string());
            if self
                .localization_translatable_content_digest(&resource_id, &key)
                .is_some_and(|expected_digest| {
                    resolved_object_string(translation_input, "translatableContentDigest")
                        .as_deref()
                        != Some(expected_digest.as_str())
                })
            {
                user_errors.push(json!({
                    "field": ["translations", field_index, "translatableContentDigest"],
                    "message": "Translatable content hash is invalid",
                    "code": "INVALID_TRANSLATABLE_CONTENT"
                }));
                continue;
            }
            let market_id = resolved_object_string(translation_input, "marketId");
            if matches!(market_id.as_deref(), Some(id) if id.contains("999999")) {
                has_null_translation_error = true;
                user_errors.push(json!({
                    "field": ["translations", field_index, "marketId"],
                    "message": "The market corresponding to the `marketId` argument doesn't exist",
                    "code": "MARKET_DOES_NOT_EXIST"
                }));
                continue;
            }
            if resource_id.contains("PackingSlipTemplate") {
                has_null_translation_error = true;
                user_errors.push(json!({
                    "field": ["translations", field_index, "key"],
                    "message": "Key body cannot be customized for a market; it can only be translated.",
                    "code": "RESOURCE_NOT_MARKET_CUSTOMIZABLE"
                }));
                continue;
            }

            let mut translation = translation_from_input(translation_input);
            if translation["key"] == json!("handle") {
                let original_value = translation["value"].as_str().unwrap_or_default();
                if original_value.chars().count() > 255 {
                    user_errors.push(json!({
                        "field": ["translations", field_index, "value"],
                        "message": "Value fails validation on resource: [\"Handle is too long (maximum is 255 characters)\"]",
                        "code": "FAILS_RESOURCE_VALIDATION"
                    }));
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
                    existing["key"] != translation["key"]
                        || existing["locale"] != translation["locale"]
                        || existing["market"] != translation["market"]
                });
            self.store
                .staged
                .localization_translations
                .push(translation.clone());
        }

        let translations = if staged.is_empty() && has_null_translation_error {
            Value::Null
        } else {
            Value::Array(staged)
        };
        selected_json(
            &json!({ "translations": translations, "userErrors": user_errors }),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn localization_remove_response(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let resource_id = resolved_string_arg(&field.arguments, "resourceId").unwrap_or_default();
        if !self.localization_translatable_resource_exists(&resource_id) {
            return selected_json(
                &json!({
                    "translations": null,
                    "userErrors": [{
                        "field": ["resourceId"],
                        "message": format!("Resource {resource_id} does not exist"),
                        "code": "RESOURCE_NOT_FOUND"
                    }]
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
        if market_ids.iter().any(|id| id.contains("999999")) {
            return selected_json(
                &json!({ "translations": [], "userErrors": [] }),
                &field.selection,
            );
        }
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

    pub(in crate::proxy) fn localization_translatable_resource_exists(
        &self,
        resource_id: &str,
    ) -> bool {
        if resource_id.starts_with("gid://shopify/Product/") {
            return self.store.has_localization_product(resource_id);
        }

        true
    }

    pub(in crate::proxy) fn localization_translatable_resource(&self, resource_id: &str) -> Value {
        json!({
            "resourceId": resource_id,
            "translatableContent": self.localization_translatable_content(resource_id),
            "translations": self.store.staged.localization_translations.clone()
        })
    }

    fn localization_translatable_content(&self, resource_id: &str) -> Vec<Value> {
        if let Some(product) = self.store.product_by_id(resource_id) {
            let mut content = vec![
                localization_translatable_content_record(
                    "title",
                    &product.title,
                    "SINGLE_LINE_TEXT_FIELD",
                ),
                localization_translatable_content_record("handle", &product.handle, "URI"),
                localization_translatable_content_record(
                    "product_type",
                    &product.product_type,
                    "SINGLE_LINE_TEXT_FIELD",
                ),
            ];
            if !product.description_html.is_empty() {
                content.push(localization_translatable_content_record(
                    "body_html",
                    &product.description_html,
                    "HTML",
                ));
            }
            if !product.seo_title.is_empty() {
                content.push(localization_translatable_content_record(
                    "seo_title",
                    &product.seo_title,
                    "SINGLE_LINE_TEXT_FIELD",
                ));
            }
            if !product.seo_description.is_empty() {
                content.push(localization_translatable_content_record(
                    "seo_description",
                    &product.seo_description,
                    "MULTI_LINE_TEXT_FIELD",
                ));
            }
            return content;
        }

        vec![
            localization_translatable_content_record(
                "title",
                FALLBACK_PRODUCT_TRANSLATION_TITLE,
                "SINGLE_LINE_TEXT_FIELD",
            ),
            localization_translatable_content_record(
                "handle",
                FALLBACK_PRODUCT_TRANSLATION_HANDLE,
                "URI",
            ),
            localization_translatable_content_record(
                "product_type",
                FALLBACK_PRODUCT_TRANSLATION_PRODUCT_TYPE,
                "SINGLE_LINE_TEXT_FIELD",
            ),
        ]
    }

    fn localization_translatable_content_digest(
        &self,
        resource_id: &str,
        key: &str,
    ) -> Option<String> {
        self.localization_translatable_content(resource_id)
            .into_iter()
            .find(|content| content["key"].as_str() == Some(key))
            .and_then(|content| content["digest"].as_str().map(str::to_string))
    }
}

fn localization_translatable_content_record(key: &str, value: &str, content_type: &str) -> Value {
    json!({
        "key": key,
        "value": value,
        "digest": localization_content_digest(value),
        "locale": "en",
        "type": content_type
    })
}

fn localization_content_digest(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    format!("{digest:x}")
}

use super::*;

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
                    let resource_id =
                        resolved_string_field(&field.arguments, "resourceId").unwrap_or_default();
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
                (is_shopify_gid_of_type(id, "Market") && !self.market_exists(id))
                    || (is_shopify_gid_of_type(id, "MarketWebPresence")
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
        let primary_locale = self.localization_primary_locale();
        let payload = if locale == primary_locale {
            shop_locale_payload_error("shopLocale", PRIMARY_LOCALE_CHANGE_MESSAGE)
        } else if self.localization_available_locale_name(&locale).is_none() {
            shop_locale_payload_error("shopLocale", "Locale is invalid")
        } else if self.localization_shop_locale_added(&locale) {
            shop_locale_payload_error("shopLocale", "Locale has already been taken")
        } else if self
            .localization_shop_locales(None)
            .iter()
            .filter(|locale| !locale["primary"].as_bool().unwrap_or(false))
            .count()
            >= 20
        {
            payload_user_error(
                "shopLocale",
                user_error_omit_code(Value::Null, &format!(
                        "Your store has reached its 20 language limit. To add {}, delete one of your other languages.",
                        self.localization_available_locale_name(&locale).unwrap_or(locale.as_str())
                    ), None),
            )
        } else {
            let name = self
                .localization_available_locale_name(&locale)
                .unwrap_or(locale.as_str());
            let mut record = shop_locale_record(&locale, name, false, &primary_locale);
            let target_web_presence_ids = self.known_market_web_presence_ids(
                resolved_string_list_arg(&field.arguments, "marketWebPresenceIds"),
            );
            record["marketWebPresences"] = Value::Array(
                target_web_presence_ids
                    .iter()
                    .map(|id| {
                        let default_locale = self.market_web_presence_default_locale(id);
                        shop_locale_market_web_presence_record(id, &default_locale)
                    })
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
        let primary_locale = self.localization_primary_locale();

        if locale == primary_locale && published.is_some() {
            return selected_json(
                &shop_locale_payload_error("shopLocale", PRIMARY_LOCALE_CHANGE_MESSAGE),
                &field.selection,
            );
        }

        let locale_exists = self.localization_shop_locale_added(&locale);
        if !locale_exists && published.is_some() {
            return selected_json(
                &shop_locale_payload_error("shopLocale", "The locale doesn't exist."),
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
                shop_locale_record(&locale, name, false, &primary_locale)
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
                    .map(|id| {
                        let default_locale = self.market_web_presence_default_locale(id);
                        shop_locale_market_web_presence_record(id, &default_locale)
                    })
                    .collect(),
            );
            self.sync_web_presence_locales(&locale, &target_web_presence_ids, true);
        }
        if locale != primary_locale {
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

    fn market_web_presence_default_locale(&self, id: &str) -> String {
        self.store
            .staged
            .web_presences
            .get(id)
            .and_then(|presence| presence.pointer("/defaultLocale/locale"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| self.localization_primary_locale())
    }

    pub(in crate::proxy) fn shop_locale_disable_response(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let locale =
            resolved_string_field(&field.arguments, "locale").unwrap_or_else(|| "fr".to_string());
        let primary_locale = self.localization_primary_locale();
        let payload = if locale == primary_locale {
            shop_locale_payload_error("locale", PRIMARY_LOCALE_CHANGE_MESSAGE)
        } else if !self.store.staged.shop_locales.contains_key(&locale) {
            shop_locale_payload_error("locale", "The locale doesn't exist.")
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
    pub(in crate::proxy) fn market_localization_query_data(
        &mut self,
        fields: &[RootFieldSelection],
        request: &Request,
    ) -> Value {
        root_payload_json(fields, |field| {
            Some(match field.name.as_str() {
                "marketLocalizableResource" => {
                    let resource_id =
                        resolved_string_field(&field.arguments, "resourceId").unwrap_or_default();
                    if !self.market_localizable_resource_exists(&resource_id) {
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
                "marketLocalizableResources" => self.market_localizable_resources_connection(field),
                "marketLocalizableResourcesByIds" => {
                    self.market_localizable_resources_by_ids_connection(field)
                }
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

    fn market_localizable_resource_selected(
        &self,
        resource_id: &str,
        selections: &[SelectedField],
    ) -> Value {
        let market_filter = market_localizations_market_filter(selections);
        selected_json(
            &self.market_localizable_resource(resource_id, market_filter.as_deref()),
            selections,
        )
    }

    pub(in crate::proxy) fn market_localizable_resources_connection(
        &self,
        field: &RootFieldSelection,
    ) -> Value {
        let resource_type = resolved_string_field(&field.arguments, "resourceType");
        let records = self
            .market_localizable_resource_ids()
            .into_iter()
            .filter(|resource_id| {
                resource_type.as_deref().is_none_or(|resource_type| {
                    localization_resource_type_matches(resource_id, resource_type)
                })
            })
            .collect::<Vec<_>>();
        selected_typed_connection_with_args(
            &records,
            &field.arguments,
            &field.selection,
            |resource_id, selection| {
                self.market_localizable_resource_selected(resource_id, selection)
            },
            |resource_id| resource_id.clone(),
        )
    }

    pub(in crate::proxy) fn market_localizable_resources_by_ids_connection(
        &self,
        field: &RootFieldSelection,
    ) -> Value {
        let records = resolved_string_list_arg(&field.arguments, "resourceIds")
            .into_iter()
            .filter(|resource_id| self.market_localizable_resource_exists(resource_id))
            .collect::<Vec<_>>();
        selected_typed_connection_with_args(
            &records,
            &field.arguments,
            &field.selection,
            |resource_id, selection| {
                self.market_localizable_resource_selected(resource_id, selection)
            },
            |resource_id| resource_id.clone(),
        )
    }

    fn market_localizable_resource_ids(&self) -> Vec<String> {
        let mut ids = self
            .store
            .staged
            .localization_resources
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>();
        ids.extend(
            self.store
                .staged
                .localization_translations
                .iter()
                .filter_map(|translation| {
                    translation["resourceId"]
                        .as_str()
                        .filter(|resource_id| !resource_id.is_empty())
                        .map(ToString::to_string)
                }),
        );
        ids.into_iter().collect()
    }

    pub(in crate::proxy) fn has_market_localizable_resource_state(&self) -> bool {
        !self.market_localizable_resource_ids().is_empty()
    }

    fn market_localizable_resource_exists(&self, resource_id: &str) -> bool {
        !resource_id.is_empty()
            && (self
                .store
                .staged
                .localization_resources
                .contains_key(resource_id)
                || self
                    .store
                    .staged
                    .localization_translations
                    .iter()
                    .any(|translation| {
                        translation["resourceId"].as_str() == Some(resource_id)
                            && !translation["market"].is_null()
                    }))
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
            return selected_market_localization_error(
                &field.selection,
                vec!["resourceId"],
                "TOO_MANY_KEYS_FOR_RESOURCE",
                "Too many keys for resource - maximum 100 per mutation",
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
            return selected_market_localization_error(
                &field.selection,
                vec!["resourceId"],
                "RESOURCE_NOT_FOUND",
                &format!("Resource {resource_id} does not exist"),
            );
        };

        let mut staged = Vec::new();
        for (index, input) in localizations.iter().enumerate() {
            let field_index = index.to_string();
            let market_id = resolved_object_string(input, "marketId").unwrap_or_default();
            if market_id.is_empty() || !self.market_exists(&market_id) {
                return selected_market_localization_error(
                    &field.selection,
                    vec!["marketLocalizations", &field_index, "marketId"],
                    "MARKET_DOES_NOT_EXIST",
                    "The market does not exist",
                );
            }
            let key = resolved_object_string(input, "key").unwrap_or_default();
            // 3. The key must be one of the resource's localizable content keys.
            let Some(content_entry) = content.as_array().and_then(|entries| {
                entries
                    .iter()
                    .find(|entry| entry["key"].as_str() == Some(key.as_str()))
            }) else {
                return selected_market_localization_error(
                    &field.selection,
                    vec!["marketLocalizations", &field_index, "key"],
                    "INVALID_KEY_FOR_MODEL",
                    &format!("Key {key} is not a valid market localizable field"),
                );
            };
            // 4. The supplied digest must match the resource's current content digest.
            let expected_digest = content_entry["digest"].as_str();
            if resolved_object_string(input, "marketLocalizableContentDigest").as_deref()
                != expected_digest
            {
                return selected_market_localization_error(
                    &field.selection,
                    vec![
                        "marketLocalizations",
                        &field_index,
                        "marketLocalizableContentDigest",
                    ],
                    "INVALID_MARKET_LOCALIZABLE_CONTENT",
                    "The provided content digest does not match the latest resource content",
                );
            }
            // 5. The localized value must not be blank.
            if resolved_object_string(input, "value").as_deref() == Some("") {
                return selected_market_localization_error(
                    &field.selection,
                    vec!["marketLocalizations", &field_index, "value"],
                    "FAILS_RESOURCE_VALIDATION",
                    "Value can't be blank",
                );
            }
            // 6. Shopify exposes definition-backed money metafields as a
            // `value` market-localizable field, but rejects JSON money payloads
            // during register with a resource-validation error.
            if market_localizable_content_is_money_metafield(content_entry) {
                return selected_market_localization_error(
                    &field.selection,
                    vec!["marketLocalizations", &field_index, "value"],
                    "FAILS_RESOURCE_VALIDATION",
                    "Market Localizable content is invalid",
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
            return selected_market_localization_error(
                &field.selection,
                vec!["resourceId"],
                "RESOURCE_NOT_FOUND",
                &format!("Resource {resource_id} does not exist"),
            );
        }
        let keys = resolved_string_list_arg(&field.arguments, "marketLocalizationKeys");
        let market_ids = resolved_string_list_arg(&field.arguments, "marketIds");
        if keys.is_empty() {
            return selected_json(
                &payload_error("marketLocalizations", vec![]),
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
        if !self.localization_translation_mutation_resource_exists(&resource_id) {
            return selected_translation_error(
                &field.selection,
                &format!("Resource {resource_id} does not exist"),
                "RESOURCE_NOT_FOUND",
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
            return selected_translation_error(
                &field.selection,
                "Too many keys for resource - maximum 100 per mutation",
                "TOO_MANY_KEYS_FOR_RESOURCE",
            );
        }
        let mut staged = Vec::new();
        let mut user_errors = Vec::new();
        let primary_locale = self.localization_primary_locale();
        for (index, translation_input) in translations.iter().enumerate() {
            let field_index = index.to_string();
            let locale = resolved_object_string(translation_input, "locale")
                .unwrap_or_else(|| "fr".to_string());
            let market_id = resolved_object_string(translation_input, "marketId");
            if matches!(market_id.as_deref(), Some(id) if !self.market_exists(id)) {
                user_errors.push(user_error(
                    json!(["translations", field_index, "marketId"]),
                    "The market corresponding to the `marketId` argument doesn't exist",
                    Some("MARKET_DOES_NOT_EXIST"),
                ));
                continue;
            }
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
                let digest_invalid = self
                    .localization_source_content_value(&resource_id, &key)
                    .is_some_and(|value| localization_content_digest(&value) != supplied_digest);
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
        if !self.localization_translation_mutation_resource_exists(&resource_id) {
            return selected_translation_error(
                &field.selection,
                &format!("Resource {resource_id} does not exist"),
                "RESOURCE_NOT_FOUND",
            );
        }
        let keys = resolved_string_list_arg(&field.arguments, "translationKeys");
        let market_ids = resolved_string_list_arg(&field.arguments, "marketIds");
        let locales = resolved_string_list_arg(&field.arguments, "locales");
        if keys.is_empty() || locales.is_empty() {
            return selected_json(&payload_error("translations", vec![]), &field.selection);
        }
        self.store.staged.localization_dirty = true;
        let mut removed = Vec::new();
        let mut retained = Vec::new();
        for translation in self.store.staged.localization_translations.drain(..) {
            let key_matches = keys.iter().any(|key| translation["key"] == json!(key));
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
        if resolved_bool_field(&field.arguments, "reverse").unwrap_or(false) {
            records.reverse();
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
        selected_staged_connection_with_args(
            records,
            &field.arguments,
            &field.selection,
            market_search_decision,
            market_sort_key,
            |market, selection| self.selected_market_json(market, selection),
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
            if !is_shopify_gid_of_type(id, "Market")
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

    /// Record a market-localizable resource observed upstream: index its
    /// `marketLocalizableContent` by `resourceId` (existence + valid keys/digests)
    /// and stage any pre-existing `marketLocalizations` so read-after-write filtering
    /// reflects Shopify's prior state for an arbitrary backend.
    pub(in crate::proxy) fn stage_observed_market_localizable_resource(
        &mut self,
        resource: &Value,
    ) {
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
    /// and translation changes are not bypassed by passthrough. Modeled from captured LiveHybrid localization behavior.
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
    /// fed by captured upstream response hydration. Shop locales, available locales and
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
                if is_shopify_gid_of_type(resource_id, "Product") {
                    self.store
                        .base
                        .localization_product_ids
                        .insert(resource_id.to_string());
                    self.stage_observed_localization_product_source(resource_id, resource);
                } else if is_shopify_gid_of_type(resource_id, "Collection") {
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
        if resource_id.is_empty() {
            return false;
        }
        match shopify_gid_resource_type(resource_id) {
            Some("Product") => self.store.has_localization_product(resource_id),
            Some("Collection") => self.store.collection_by_id(resource_id).is_some(),
            Some(_) => true,
            _ => false,
        }
    }

    /// Mutations must reject resource IDs the proxy cannot resolve locally, while
    /// read roots still keep Shopify-like empty placeholders for unmodeled types.
    fn localization_translation_mutation_resource_exists(&self, resource_id: &str) -> bool {
        if resource_id.is_empty() {
            return false;
        }
        match shopify_gid_resource_type(resource_id) {
            Some("Product") => self.store.has_localization_product(resource_id),
            Some("Collection") => self.store.collection_by_id(resource_id).is_some(),
            Some("PackingSlipTemplate") => true,
            _ => false,
        }
    }

    fn localization_translatable_content(&self, resource_id: &str) -> Vec<Value> {
        let locale = self.localization_primary_locale();
        if is_shopify_gid_of_type(resource_id, "Product") {
            return self
                .store
                .product_staged_or_base(resource_id)
                .map(|product| localization_product_translatable_content(&product, &locale))
                .unwrap_or_default();
        }
        if is_shopify_gid_of_type(resource_id, "Collection") {
            return self
                .store
                .collection_by_id(resource_id)
                .map(|collection| localization_collection_translatable_content(collection, &locale))
                .unwrap_or_default();
        }
        Vec::new()
    }

    pub(in crate::proxy) fn localization_primary_locale(&self) -> String {
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
    /// is skipped — matching Shopify's captured "content not found -> no digest error" behavior.
    fn localization_source_content_value(&self, resource_id: &str, key: &str) -> Option<String> {
        if is_shopify_gid_of_type(resource_id, "Product") {
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
        if is_shopify_gid_of_type(resource_id, "Collection") {
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
        is_shopify_gid_of_type(resource_id, "Product")
            || (is_shopify_gid_of_type(resource_id, "Collection")
                && self.store.collection_by_id(resource_id).is_some())
    }

    fn localization_translation_key_is_valid(&self, resource_id: &str, key: &str) -> bool {
        if is_shopify_gid_of_type(resource_id, "Product") {
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
        if is_shopify_gid_of_type(resource_id, "Collection") {
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
    /// mutated in place. Modeled from captured localization mutation behavior.
    fn sync_web_presence_locales(&mut self, locale: &str, target_ids: &[String], replace: bool) {
        let primary_locale = self.localization_primary_locale();
        if locale == primary_locale {
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

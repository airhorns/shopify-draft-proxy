use super::*;

pub(in crate::proxy) fn localization_field_resolver_registrations() -> Vec<FieldResolverRegistration>
{
    vec![
        FieldResolverRegistration::explicit(
            ApiSurface::Admin,
            "TranslatableResource",
            "translatableContent",
            translatable_resource_content_field,
        ),
        FieldResolverRegistration::explicit(
            ApiSurface::Admin,
            "TranslatableResource",
            "translations",
            translatable_resource_translations_field,
        ),
        FieldResolverRegistration::explicit(
            ApiSurface::Admin,
            "TranslatableResource",
            "nestedTranslatableResources",
            translatable_resource_nested_resources_field,
        ),
        FieldResolverRegistration::explicit(
            ApiSurface::Admin,
            "MarketLocalizableResource",
            "marketLocalizations",
            market_localizable_resource_localizations_field,
        ),
    ]
}

fn market_localizable_resource_localizations_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let resource_id = invocation
        .parent
        .get("resourceId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let arguments = resolved_arguments_from_json(&invocation.arguments);
    let market_id = resolved_string_field(&arguments, "marketId");
    Ok(
        proxy.market_localizable_resource(resource_id, market_id.as_deref())["marketLocalizations"]
            .clone(),
    )
}

fn translatable_resource_id(
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> String {
    invocation
        .parent
        .get("resourceId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn translatable_resource_content_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let resource_id = translatable_resource_id(invocation);
    Ok(Value::Array(
        proxy.localization_translatable_content(&resource_id),
    ))
}

fn translatable_resource_translations_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let arguments = resolved_arguments_from_json(&invocation.arguments);
    let locale = resolved_string_field(&arguments, "locale");
    let market_id = resolved_string_field(&arguments, "marketId");
    let resource_id = translatable_resource_id(invocation);
    Ok(Value::Array(proxy.localization_translations_for(
        &resource_id,
        locale.as_deref(),
        market_id.as_deref(),
    )))
}

fn translatable_resource_nested_resources_field(
    _proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let arguments = resolved_arguments_from_json(&invocation.arguments);
    Ok(invocation
        .parent
        .get("nestedTranslatableResources")
        .map(|connection| seeded_connection_value(connection, &arguments))
        .unwrap_or_else(|| connection_json(Vec::new())))
}

/// Engine-coerced input for one localization mutation root. Selection and
/// transport metadata stay at the GraphQL executor boundary.
struct LocalizationMutationInput {
    name: String,
    arguments: BTreeMap<String, ResolvedValue>,
}

fn localization_mutation_target_ids(
    root_name: &str,
    arguments: &BTreeMap<String, ResolvedValue>,
) -> Vec<String> {
    let mut ids = Vec::new();
    match root_name {
        "shopLocaleEnable" => {
            ids.extend(resolved_string_list_arg(arguments, "marketWebPresenceIds"));
        }
        "shopLocaleUpdate" => {
            let input = resolved_object_field(arguments, "shopLocale").unwrap_or_default();
            ids.extend(resolved_string_list_field_unsorted(
                &input,
                "marketWebPresenceIds",
            ));
        }
        "translationsRegister" => {
            for translation in resolved_list_arg(arguments, "translations") {
                if let Some(market_id) = resolved_object_string(&translation, "marketId") {
                    ids.push(market_id);
                }
            }
        }
        "translationsRemove" => {
            ids.extend(resolved_string_list_arg(arguments, "marketIds"));
        }
        _ => {}
    }
    ids.sort();
    ids.dedup();
    ids
}

impl DraftProxy {
    pub(crate) fn localization_query_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let RootInvocation {
            response_key,
            arguments,
            request,
            root_name,
            ..
        } = invocation;
        let arguments = resolved_arguments_from_json(&arguments);
        if self.execution_session.localization_context_preflighted {
            return ResolverOutcome::value(self.localization_query_value(
                root_name,
                response_key,
                &arguments,
                request,
                false,
            ));
        }
        if self.config.read_mode == ReadMode::LiveHybrid
            && self.localization_should_fetch_upstream(root_name)
        {
            // A localization document commonly selects several same-domain
            // roots. Hydrate from one request-scoped execution of the complete
            // document so sibling resolvers do not each consume the same
            // upstream call and so aliases are observed together.
            let result = self.cached_or_forward_upstream_graphql_result(request, response_key);
            if result.transport_succeeded && result.outcome.errors.is_empty() {
                self.hydrate_localization_from_upstream(&json!({ "data": result.data }));
            }
            return result.outcome;
        }
        ResolverOutcome::value(self.localization_query_value(
            root_name,
            response_key,
            &arguments,
            request,
            true,
        ))
    }

    pub(crate) fn localization_mutation_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let RootInvocation {
            response_key,
            arguments,
            request,
            root_name,
            ..
        } = invocation;
        let input = LocalizationMutationInput {
            name: root_name.to_string(),
            arguments: resolved_arguments_from_json(&arguments),
        };
        self.localization_mutation_preflight(&input, request);
        let result = self.localization_mutation_value(&input);
        let mut outcome = ResolverOutcome::value(result.value);
        if result.staged {
            outcome = outcome.with_log_draft(LogDraft::staged(
                root_name,
                "localization",
                vec![response_key.to_string()],
            ));
        }
        outcome
    }

    pub(in crate::proxy) fn preflight_localization_markets_context(
        &mut self,
        request: &Request,
        fields: &[RootFieldSelection],
        use_original_request: bool,
    ) {
        self.execution_session.localization_context_preflighted = true;
        self.execution_session.markets_query_preflighted = true;
        if use_original_request {
            let response = (self.upstream_transport)(request.clone());
            if (200..300).contains(&response.status) && response.body.get("errors").is_none() {
                self.hydrate_markets_from_upstream_for_fields(&response.body, fields);
                self.hydrate_localization_from_upstream(&response.body);
            }
            return;
        }
        let Some(field) = fields.iter().find(|field| field.name == "markets") else {
            return;
        };
        let first = resolved_int_field(&field.arguments, "first")
            .unwrap_or(50)
            .max(0);
        if first == 0 {
            return;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": "query LocalizationMarketsHydrate($first: Int!) { markets(first: $first) { nodes { id name handle status type } } }",
                "operationName": "LocalizationMarketsHydrate",
                "variables": { "first": first }
            }),
        );
        if (200..300).contains(&response.status) && response.body.get("errors").is_none() {
            self.stage_observed_localization_source_data(&response.body["data"]);
        }
    }

    fn localization_query_value(
        &mut self,
        root_name: &str,
        response_key: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
        request: &Request,
        hydrate_missing_markets: bool,
    ) -> Value {
        match root_name {
            "availableLocales" => Value::Array(self.localization_available_locales()),
            "shopLocales" => {
                // Shopify's schema default is `published: false`, where false means
                // "do not restrict to published locales" rather than "only return
                // unpublished locales". Only true activates the filter.
                let published_filter =
                    resolved_bool_field(arguments, "published").filter(|published| *published);
                Value::Array(self.localization_shop_locales(published_filter))
            }
            "translatableResource" => {
                let resource_id =
                    resolved_string_field(arguments, "resourceId").unwrap_or_default();
                if !self.localization_translatable_resource_exists(&resource_id) {
                    Value::Null
                } else {
                    self.localization_translatable_resource_value(&resource_id)
                }
            }
            "translatableResources" => {
                self.localization_translatable_resources_connection(arguments)
            }
            "translatableResourcesByIds" => {
                self.localization_translatable_resources_by_ids_connection(arguments)
            }
            "markets" => self.localization_markets_connection_with_hydration(
                arguments,
                response_key,
                request,
                hydrate_missing_markets,
            ),
            _ => Value::Null,
        }
    }

    fn localization_mutation_value(
        &mut self,
        input: &LocalizationMutationInput,
    ) -> LocalMutationResult {
        match input.name.as_str() {
            "shopLocaleEnable" => self.shop_locale_enable_response(input),
            "shopLocaleUpdate" => self.shop_locale_update_response(input),
            "shopLocaleDisable" => self.shop_locale_disable_response(input),
            "translationsRegister" => self.localization_register_response(input),
            "translationsRemove" => self.localization_remove_response(input),
            _ => LocalMutationResult::no_stage(Value::Null),
        }
    }

    fn localization_mutation_preflight(
        &mut self,
        input: &LocalizationMutationInput,
        request: &Request,
    ) {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return;
        }
        let ids = localization_mutation_target_ids(&input.name, &input.arguments)
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

    fn shop_locale_enable_response(
        &mut self,
        input: &LocalizationMutationInput,
    ) -> LocalMutationResult {
        let locale =
            resolved_string_field(&input.arguments, "locale").unwrap_or_else(|| "fr".to_string());
        let primary_locale = self.localization_primary_locale();
        if locale == primary_locale {
            LocalMutationResult::no_stage(shop_locale_payload_error(
                "shopLocale",
                PRIMARY_LOCALE_CHANGE_MESSAGE,
            ))
        } else if self.localization_available_locale_name(&locale).is_none() {
            LocalMutationResult::no_stage(shop_locale_payload_error(
                "shopLocale",
                "Locale is invalid",
            ))
        } else if self.localization_shop_locale_added(&locale) {
            LocalMutationResult::no_stage(shop_locale_payload_error(
                "shopLocale",
                "Locale has already been taken",
            ))
        } else if self
            .localization_shop_locales(None)
            .iter()
            .filter(|locale| !locale["primary"].as_bool().unwrap_or(false))
            .count()
            >= 20
        {
            LocalMutationResult::no_stage(payload_user_error(
                "shopLocale",
                user_error_omit_code(Value::Null, &format!(
                        "Your store has reached its 20 language limit. To add {}, delete one of your other languages.",
                        self.localization_available_locale_name(&locale).unwrap_or(locale.as_str())
                    ), None),
            ))
        } else {
            let name = self
                .localization_available_locale_name(&locale)
                .unwrap_or(locale.as_str());
            let mut record = shop_locale_record(&locale, name, false, &primary_locale);
            let target_web_presence_ids = self.known_market_web_presence_ids(
                resolved_string_list_arg(&input.arguments, "marketWebPresenceIds"),
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
            LocalMutationResult::staged(json!({ "shopLocale": record, "userErrors": [] }))
        }
    }

    fn shop_locale_update_response(
        &mut self,
        input: &LocalizationMutationInput,
    ) -> LocalMutationResult {
        let locale =
            resolved_string_field(&input.arguments, "locale").unwrap_or_else(|| "fr".to_string());
        let shop_locale = resolved_object_field(&input.arguments, "shopLocale").unwrap_or_default();
        let published = resolved_bool_field(&shop_locale, "published");
        let market_web_presence_ids = list_string_field(&shop_locale, "marketWebPresenceIds");
        let primary_locale = self.localization_primary_locale();

        if locale == primary_locale && published.is_some() {
            return LocalMutationResult::no_stage(shop_locale_payload_error(
                "shopLocale",
                PRIMARY_LOCALE_CHANGE_MESSAGE,
            ));
        }

        let locale_exists = self.localization_shop_locale_added(&locale);
        if !locale_exists && published.is_some() {
            return LocalMutationResult::no_stage(shop_locale_payload_error(
                "shopLocale",
                "The locale doesn't exist.",
            ));
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
        if shop_locale.contains_key("marketWebPresenceIds") {
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
        let staged = locale != primary_locale;
        if staged {
            self.store
                .staged
                .shop_locales
                .insert(locale, record.clone());
        }
        if staged {
            LocalMutationResult::staged(json!({ "shopLocale": record, "userErrors": [] }))
        } else {
            LocalMutationResult::no_stage(json!({ "shopLocale": record, "userErrors": [] }))
        }
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

    fn shop_locale_disable_response(
        &mut self,
        input: &LocalizationMutationInput,
    ) -> LocalMutationResult {
        let locale =
            resolved_string_field(&input.arguments, "locale").unwrap_or_else(|| "fr".to_string());
        let primary_locale = self.localization_primary_locale();
        if locale == primary_locale {
            LocalMutationResult::no_stage(shop_locale_payload_error(
                "locale",
                PRIMARY_LOCALE_CHANGE_MESSAGE,
            ))
        } else if !self.store.staged.shop_locales.contains_key(&locale) {
            LocalMutationResult::no_stage(shop_locale_payload_error(
                "locale",
                "The locale doesn't exist.",
            ))
        } else {
            self.store.staged.shop_locales.remove(&locale);
            self.store
                .staged
                .localization_translations
                .retain(|translation| translation["locale"] != json!(locale));
            self.store.staged.localization_dirty = true;
            LocalMutationResult::staged(json!({ "locale": locale, "userErrors": [] }))
        }
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

    pub(in crate::proxy) fn market_localizable_resources_connection(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let resource_type = resolved_string_field(arguments, "resourceType");
        let records = self
            .market_localizable_resource_ids()
            .into_iter()
            .filter(|resource_id| {
                resource_type.as_deref().is_none_or(|resource_type| {
                    localization_resource_type_matches(resource_id, resource_type)
                })
            })
            .collect::<Vec<_>>();
        connection_value_with_args(
            records
                .iter()
                .map(|resource_id| self.market_localizable_resource(resource_id, None))
                .collect(),
            arguments,
            |resource| {
                resource
                    .get("resourceId")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string()
            },
        )
    }

    pub(in crate::proxy) fn market_localizable_resources_by_ids_connection(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let records = resolved_string_list_arg(arguments, "resourceIds")
            .into_iter()
            .filter(|resource_id| self.market_localizable_resource_exists(resource_id))
            .collect::<Vec<_>>();
        connection_value_with_args(
            records
                .iter()
                .map(|resource_id| self.market_localizable_resource(resource_id, None))
                .collect(),
            arguments,
            |resource| {
                resource
                    .get("resourceId")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string()
            },
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

    pub(in crate::proxy) fn market_localizable_resources_by_ids_should_fetch_upstream(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> bool {
        let resource_ids = resolved_string_list_arg(arguments, "resourceIds");
        resource_ids.is_empty()
            || resource_ids.iter().any(|resource_id| {
                !self
                    .store
                    .staged
                    .localization_resources
                    .contains_key(resource_id)
            })
    }

    pub(super) fn market_localizable_resource_exists(&self, resource_id: &str) -> bool {
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

    pub(in crate::proxy) fn market_localization_mutation_value(
        &mut self,
        field: &MarketsRootInput,
    ) -> LocalMutationResult {
        match field.name.as_str() {
            "marketLocalizationsRegister" => self.market_localizations_register_response(field),
            "marketLocalizationsRemove" => self.market_localizations_remove_response(field),
            _ => LocalMutationResult::no_stage(Value::Null),
        }
    }

    pub(in crate::proxy) fn market_localizations_register_response(
        &mut self,
        field: &MarketsRootInput,
    ) -> LocalMutationResult {
        let resource_id = resolved_string_field(&field.arguments, "resourceId").unwrap_or_default();
        let localizations = resolved_list_arg(&field.arguments, "marketLocalizations");
        // 1. Per-mutation key cap fires before resource existence (matches live Shopify).
        if localizations.len() > 100 {
            return LocalMutationResult::no_stage(selected_market_localization_error(
                field,
                vec!["resourceId"],
                "TOO_MANY_KEYS_FOR_RESOURCE",
                "Too many keys for resource - maximum 100 per mutation",
            ));
        }
        // 2. The resource must have been observed (cold read / mutation preflight).
        let Some(content) = self
            .store
            .staged
            .localization_resources
            .get(&resource_id)
            .cloned()
        else {
            return LocalMutationResult::no_stage(selected_market_localization_error(
                field,
                vec!["resourceId"],
                "RESOURCE_NOT_FOUND",
                &format!("Resource {resource_id} does not exist"),
            ));
        };

        let mut staged_inputs = Vec::new();
        for (index, input) in localizations.iter().enumerate() {
            let field_index = index.to_string();
            let market_id = resolved_object_string(input, "marketId").unwrap_or_default();
            if market_id.is_empty() || !self.market_exists(&market_id) {
                return LocalMutationResult::no_stage(selected_market_localization_error(
                    field,
                    vec!["marketLocalizations", &field_index, "marketId"],
                    "MARKET_DOES_NOT_EXIST",
                    "The market does not exist",
                ));
            }
            let key = resolved_object_string(input, "key").unwrap_or_default();
            // 3. The key must be one of the resource's localizable content keys.
            let Some(content_entry) = content.as_array().and_then(|entries| {
                entries
                    .iter()
                    .find(|entry| entry["key"].as_str() == Some(key.as_str()))
            }) else {
                return LocalMutationResult::no_stage(selected_market_localization_error(
                    field,
                    vec!["marketLocalizations", &field_index, "key"],
                    "INVALID_KEY_FOR_MODEL",
                    &format!("Key {key} is not a valid market localizable field"),
                ));
            };
            // 4. The supplied digest must match the resource's current content digest.
            let expected_digest = content_entry["digest"].as_str();
            if resolved_object_string(input, "marketLocalizableContentDigest").as_deref()
                != expected_digest
            {
                return LocalMutationResult::no_stage(selected_market_localization_error(
                    field,
                    vec![
                        "marketLocalizations",
                        &field_index,
                        "marketLocalizableContentDigest",
                    ],
                    "INVALID_MARKET_LOCALIZABLE_CONTENT",
                    "The provided content digest does not match the latest resource content",
                ));
            }
            // 5. The localized value must not be blank.
            if resolved_object_string(input, "value").as_deref() == Some("") {
                return LocalMutationResult::no_stage(selected_market_localization_error(
                    field,
                    vec!["marketLocalizations", &field_index, "value"],
                    "FAILS_RESOURCE_VALIDATION",
                    "Value can't be blank",
                ));
            }
            // 6. Shopify exposes definition-backed money metafields as a
            // `value` market-localizable field, but rejects JSON money payloads
            // during register with a resource-validation error.
            if market_localizable_content_is_money_metafield(content_entry) {
                return LocalMutationResult::no_stage(selected_market_localization_error(
                    field,
                    vec!["marketLocalizations", &field_index, "value"],
                    "FAILS_RESOURCE_VALIDATION",
                    "Market Localizable content is invalid",
                ));
            }
            staged_inputs.push((market_id, input));
        }

        let updated_at = if staged_inputs.is_empty() {
            None
        } else {
            Some(self.next_mutation_timestamp())
        };
        let staged = staged_inputs
            .into_iter()
            .map(|(market_id, input)| {
                self.market_localization_staged_record(
                    &resource_id,
                    &market_id,
                    input,
                    updated_at.as_deref().unwrap_or_default(),
                )
            })
            .collect::<Vec<_>>();

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

        if staged.is_empty() {
            LocalMutationResult::no_stage(
                json!({ "marketLocalizations": staged, "userErrors": [] }),
            )
        } else {
            LocalMutationResult::staged(json!({ "marketLocalizations": staged, "userErrors": [] }))
        }
    }

    /// Build a staged market-localization record with the live market name resolved
    /// from staged markets and the successful mutation's clock timestamp.
    fn market_localization_staged_record(
        &self,
        resource_id: &str,
        market_id: &str,
        input: &ResolvedValue,
        updated_at: &str,
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
            "updatedAt": updated_at,
            "outdated": false,
            "market": { "id": market_id, "name": market_name }
        })
    }

    pub(in crate::proxy) fn market_localizations_remove_response(
        &mut self,
        field: &MarketsRootInput,
    ) -> LocalMutationResult {
        let resource_id = resolved_string_field(&field.arguments, "resourceId").unwrap_or_default();
        if !self
            .store
            .staged
            .localization_resources
            .contains_key(&resource_id)
        {
            return LocalMutationResult::no_stage(selected_market_localization_error(
                field,
                vec!["resourceId"],
                "RESOURCE_NOT_FOUND",
                &format!("Resource {resource_id} does not exist"),
            ));
        }
        let keys = resolved_string_list_arg(&field.arguments, "marketLocalizationKeys");
        let market_ids = resolved_string_list_arg(&field.arguments, "marketIds");
        if keys.is_empty() {
            return LocalMutationResult::no_stage(payload_error("marketLocalizations", vec![]));
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
        let staged = !removed.is_empty();
        let removed = if removed.is_empty() {
            Value::Null
        } else {
            Value::Array(removed)
        };
        let value = json!({ "marketLocalizations": removed, "userErrors": [] });
        if staged {
            LocalMutationResult::staged(value)
        } else {
            LocalMutationResult::no_stage(value)
        }
    }

    fn localization_register_response(
        &mut self,
        input: &LocalizationMutationInput,
    ) -> LocalMutationResult {
        let resource_id = resolved_string_field(&input.arguments, "resourceId").unwrap_or_default();
        if !self.localization_translation_mutation_resource_exists(&resource_id) {
            return LocalMutationResult::no_stage(translation_payload_error(
                &format!("Resource {resource_id} does not exist"),
                "RESOURCE_NOT_FOUND",
            ));
        }

        let translations = resolved_list_arg(&input.arguments, "translations");
        if translations.is_empty() {
            return LocalMutationResult::no_stage(json!({ "translations": [], "userErrors": [] }));
        }
        if translations.len() > 100 {
            return LocalMutationResult::no_stage(translation_payload_error(
                "Too many keys for resource - maximum 100 per mutation",
                "TOO_MANY_KEYS_FOR_RESOURCE",
            ));
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

        if !staged.is_empty() {
            let updated_at = self.next_mutation_timestamp();
            for translation in &mut staged {
                translation["updatedAt"] = json!(updated_at.clone());
            }
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

        if staged.is_empty() {
            LocalMutationResult::no_stage(
                json!({ "translations": staged, "userErrors": user_errors }),
            )
        } else {
            LocalMutationResult::staged(
                json!({ "translations": staged, "userErrors": user_errors }),
            )
        }
    }

    fn localization_remove_response(
        &mut self,
        input: &LocalizationMutationInput,
    ) -> LocalMutationResult {
        let resource_id = resolved_string_field(&input.arguments, "resourceId").unwrap_or_default();
        if !self.localization_translation_mutation_resource_exists(&resource_id) {
            return LocalMutationResult::no_stage(translation_payload_error(
                &format!("Resource {resource_id} does not exist"),
                "RESOURCE_NOT_FOUND",
            ));
        }
        let keys = resolved_string_list_arg(&input.arguments, "translationKeys");
        let market_ids = resolved_string_list_arg(&input.arguments, "marketIds");
        let locales = resolved_string_list_arg(&input.arguments, "locales");
        if keys.is_empty() || locales.is_empty() {
            return LocalMutationResult::no_stage(payload_error("translations", vec![]));
        }
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
        let staged = !removed.is_empty();
        if staged {
            self.store.staged.localization_dirty = true;
        }
        let removed = if removed.is_empty() {
            Value::Null
        } else {
            Value::Array(removed)
        };
        let value = json!({ "translations": removed, "userErrors": [] });
        if staged {
            LocalMutationResult::staged(value)
        } else {
            LocalMutationResult::no_stage(value)
        }
    }

    pub(in crate::proxy) fn localization_translatable_resource_value(
        &self,
        resource_id: &str,
    ) -> Value {
        let nested_resources = match shopify_gid_resource_type(resource_id) {
            Some("Product") => self
                .store
                .product_by_id(resource_id)
                .and_then(|product| product.extra_fields.get("nestedTranslatableResources")),
            Some("Collection") => self
                .store
                .collection_by_id(resource_id)
                .and_then(|collection| collection.get("nestedTranslatableResources")),
            _ => None,
        };
        let mut value = json!({"resourceId": resource_id});
        if let Some(nested_resources) = nested_resources {
            value["nestedTranslatableResources"] = nested_resources.clone();
        }
        value
    }

    pub(in crate::proxy) fn localization_translatable_resources_connection(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let resource_type = resolved_string_field(arguments, "resourceType")
            .unwrap_or_else(|| "PRODUCT".to_string());
        let records = self
            .localization_translatable_resource_ids()
            .into_iter()
            .filter(|id| localization_resource_type_matches(id, &resource_type))
            .collect::<Vec<_>>();
        connection_value_with_args(
            records
                .into_iter()
                .map(|id| self.localization_translatable_resource_value(&id))
                .collect(),
            arguments,
            |resource| {
                resource
                    .get("resourceId")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string()
            },
        )
    }

    pub(in crate::proxy) fn localization_translatable_resources_by_ids_connection(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let records = resolved_string_list_arg(arguments, "resourceIds")
            .into_iter()
            .filter(|id| self.localization_translatable_resource_exists(id))
            .collect::<Vec<_>>();
        connection_value_with_args(
            records
                .into_iter()
                .map(|id| self.localization_translatable_resource_value(&id))
                .collect(),
            arguments,
            |resource| {
                resource
                    .get("resourceId")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string()
            },
        )
    }

    fn localization_markets_connection_with_hydration(
        &mut self,
        arguments: &BTreeMap<String, ResolvedValue>,
        response_key: &str,
        request: &Request,
        hydrate_if_missing: bool,
    ) -> Value {
        let mut records = self
            .store
            .staged
            .markets
            .values()
            .cloned()
            .collect::<Vec<_>>();
        if records.is_empty() && hydrate_if_missing {
            records = self.hydrate_localization_markets(arguments, response_key, request);
        }
        staged_connection_value_with_args(
            records,
            arguments,
            market_search_decision,
            market_sort_key,
            Value::clone,
            value_id_cursor,
        )
    }

    fn hydrate_localization_markets(
        &mut self,
        arguments: &BTreeMap<String, ResolvedValue>,
        response_key: &str,
        request: &Request,
    ) -> Vec<Value> {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return Vec::new();
        }
        let first = resolved_int_field(arguments, "first").unwrap_or(50).max(0);
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
            return self.hydrate_localization_markets_from_original_request(response_key, request);
        }
        let records = response.body["data"]["markets"]["nodes"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if records.is_empty() && response.body["data"]["markets"].is_null() {
            return self.hydrate_localization_markets_from_original_request(response_key, request);
        }
        self.stage_observed_localization_markets(&records);
        records
    }

    fn hydrate_localization_markets_from_original_request(
        &mut self,
        response_key: &str,
        request: &Request,
    ) -> Vec<Value> {
        let response = (self.upstream_transport)(request.clone());
        self.stage_observed_localization_source_data(&response.body["data"]);
        if response.status >= 400 {
            return Vec::new();
        }
        let market_connection = &response.body["data"][response_key];
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
                                .entry(code.to_string())
                                .and_modify(|existing| {
                                    *existing =
                                        shallow_merged_object(existing.clone(), item.clone());
                                })
                                .or_insert_with(|| item.clone());
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
        if let Some(nested_resources) = resource.get("nestedTranslatableResources") {
            product.extra_fields.insert(
                "nestedTranslatableResources".to_string(),
                nested_resources.clone(),
            );
            observed = true;
        }
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
        if let Some(nested_resources) = resource.get("nestedTranslatableResources") {
            object.insert(
                "nestedTranslatableResources".to_string(),
                nested_resources.clone(),
            );
            observed = true;
        }
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

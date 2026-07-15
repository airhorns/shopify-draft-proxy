use super::*;

impl DraftProxy {
    pub(in crate::proxy) fn resolve_markets_graphql(
        &mut self,
        context: RootResolverContext<'_>,
    ) -> Response {
        let RootResolverContext {
            request,
            query,
            variables,
            operation,
            root_name,
            mode,
        } = context;
        let fields = match self.root_fields_or_error(query, variables) {
            Ok(fields) => fields,
            Err(response) => return response,
        };
        match mode {
            LocalResolverMode::OverlayRead => {
                // Cold LiveHybrid reads hydrate every selected markets family.
                // Existing staged state is then rendered over that base graph.
                if self.config.read_mode == ReadMode::LiveHybrid
                    && self.markets_should_fetch_upstream(&fields, variables)
                {
                    let had_markets_overlay_state = self.has_markets_overlay_state();
                    let response = (self.upstream_transport)(request.clone());
                    if response.status < 400 {
                        self.hydrate_markets_from_upstream_for_fields(&response.body, &fields);
                        self.hydrate_localization_from_upstream(&response.body);
                    }
                    if !had_markets_overlay_state {
                        return response;
                    }
                }
                if operation
                    .root_fields
                    .iter()
                    .all(|field| field == "webPresences")
                {
                    return self.web_presence_helper_query(query, variables);
                }
                self.hydrate_markets_resolved_values_pricing_if_selected(request, &fields);
                let data = if operation.root_fields.iter().any(|field| {
                    matches!(
                        field.as_str(),
                        "marketLocalizableResource"
                            | "marketLocalizableResources"
                            | "marketLocalizableResourcesByIds"
                    )
                }) {
                    self.market_localization_query_data(&fields, request)
                } else {
                    self.markets_overlay_query_data(&fields)
                };
                ok_json(json!({ "data": data }))
            }
            LocalResolverMode::StageLocally => {
                self.hydrate_market_currency_defaults_if_needed(request, &fields);
                if let Some(response) = self.market_mutation_wrong_resource_response(&fields) {
                    return response;
                }
                let data = if operation.root_fields.iter().all(|field| {
                    matches!(
                        field.as_str(),
                        "marketLocalizationsRegister" | "marketLocalizationsRemove"
                    )
                }) {
                    self.market_localization_mutation_preflight(variables, request);
                    self.market_localization_mutation_data(&fields)
                } else if operation.root_fields.iter().all(|field| {
                    matches!(
                        field.as_str(),
                        "webPresenceCreate" | "webPresenceUpdate" | "webPresenceDelete"
                    )
                }) {
                    self.web_presence_mutation_preflight(variables, request);
                    return self.web_presence_helper_mutation(root_name, query, variables, request);
                } else if operation
                    .root_fields
                    .iter()
                    .all(|field| field == "quantityPricingByVariantUpdate")
                {
                    self.quantity_pricing_rules_mutation_preflight(request, variables);
                    return self
                        .quantity_pricing_by_variant_update_response(query, variables, request);
                } else if operation.root_fields.iter().all(|field| {
                    matches!(field.as_str(), "quantityRulesAdd" | "quantityRulesDelete")
                }) {
                    self.quantity_pricing_rules_mutation_preflight(request, variables);
                    return self
                        .quantity_rules_mutation_response(root_name, query, variables, request);
                } else if operation.root_fields.iter().any(|field| {
                    matches!(
                        field.as_str(),
                        "priceListCreate"
                            | "priceListUpdate"
                            | "priceListDelete"
                            | "priceListFixedPricesByProductUpdate"
                            | "priceListFixedPricesAdd"
                            | "priceListFixedPricesUpdate"
                            | "priceListFixedPricesDelete"
                    )
                }) {
                    return ok_json(
                        self.price_list_mutation_data(&fields, request, query, variables),
                    );
                } else if operation.root_fields.iter().any(|field| {
                    matches!(
                        field.as_str(),
                        "catalogCreate"
                            | "catalogUpdate"
                            | "catalogDelete"
                            | "catalogContextUpdate"
                    )
                }) {
                    self.catalog_mutation_data(&fields, request, query, variables)
                } else {
                    self.market_mutation_target_preflight(&fields, request);
                    self.market_create_mutation_data(&fields, request, query, variables)
                };
                if operation.root_fields.iter().all(|field| {
                    matches!(
                        field.as_str(),
                        "marketLocalizationsRegister" | "marketLocalizationsRemove"
                    )
                }) {
                    self.record_mutation_log_entry(
                        request,
                        query,
                        variables,
                        root_name,
                        fields
                            .iter()
                            .map(|field| field.response_key.clone())
                            .collect(),
                    );
                }
                ok_json(json!({ "data": data }))
            }
        }
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
                        .map(|market| self.selected_market_json(market, &field.selection))
                        .unwrap_or(Value::Null)
                }
                "catalog" => {
                    let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                    self.store
                        .staged
                        .catalogs
                        .get(&id)
                        .map(|catalog| self.selected_catalog_json(catalog, &field.selection))
                        .unwrap_or(Value::Null)
                }
                "catalogs" => self.catalogs_connection_value(field),
                "catalogsCount" => self.catalogs_count_value(field),
                "priceList" => {
                    let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                    self.store
                        .staged
                        .price_lists
                        .get(&id)
                        .map(|price_list| {
                            self.selected_price_list_json(price_list, &field.selection)
                        })
                        .unwrap_or(Value::Null)
                }
                "priceLists" => self
                    .selected_price_lists_connection_with_args(&field.arguments, &field.selection),
                "webPresences" => {
                    let records = self
                        .store
                        .staged
                        .web_presences
                        .values()
                        .cloned()
                        .collect::<Vec<_>>();
                    selected_typed_connection_with_args(
                        &records,
                        &field.arguments,
                        &field.selection,
                        |web_presence, selection| {
                            self.selected_web_presence_json(web_presence, selection)
                        },
                        value_id_cursor,
                    )
                }
                "marketsResolvedValues" => self.markets_resolved_values_value(field),
                "marketLocalizableResources" => self.market_localizable_resources_connection(field),
                "marketLocalizableResourcesByIds" => {
                    self.market_localizable_resources_by_ids_connection(field)
                }
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
                _ => Value::Null,
            })
        })
    }

    fn catalogs_count_value(&self, field: &RootFieldSelection) -> Value {
        selected_json(
            &staged_count_with_limit_precision(
                self.matching_catalogs_query(&field.arguments).total_count,
                &field.arguments,
            ),
            &field.selection,
        )
    }

    fn matching_catalogs_query(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> StagedConnectionResult<Value> {
        let type_filter = resolved_string_field(arguments, "type");
        staged_connection_query(
            self.store
                .staged
                .catalogs
                .values()
                .cloned()
                .collect::<Vec<_>>(),
            arguments,
            move |catalog, query| catalog_search_decision(catalog, query, type_filter.as_deref()),
            catalog_staged_sort_key,
            value_id_cursor,
        )
    }

    fn catalogs_connection_from_args(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
        selection: &[SelectedField],
    ) -> Value {
        let result = self.matching_catalogs_query(arguments);
        selected_typed_connection_with_page_info(
            &result.records,
            selection,
            |catalog, node_selection| self.selected_catalog_json(catalog, node_selection),
            value_id_cursor,
            result.page_info,
        )
    }

    fn catalogs_connection_value(&self, field: &RootFieldSelection) -> Value {
        self.catalogs_connection_from_args(&field.arguments, &field.selection)
    }

    pub(in crate::proxy) fn selected_market_json(
        &self,
        market: &Value,
        selections: &[SelectedField],
    ) -> Value {
        let market_id = value_string(market, "id");
        selected_record_with_connections(market, selections, |selection| {
            match selection.name.as_str() {
                "catalogs" => Some(selected_market_relation_connection(
                    self.store.staged.catalogs.values(),
                    market_id,
                    &selection.arguments,
                    &selection.selection,
                    catalog_market_ids,
                    |catalog, node_selection| self.selected_catalog_json(catalog, node_selection),
                )),
                "webPresences" => Some(selected_market_relation_connection(
                    self.store.staged.web_presences.values(),
                    market_id,
                    &selection.arguments,
                    &selection.selection,
                    web_presence_market_ids,
                    |web_presence, node_selection| {
                        self.selected_web_presence_json(web_presence, node_selection)
                    },
                )),
                _ => None,
            }
        })
    }

    fn selected_market_payload(
        &self,
        field: &RootFieldSelection,
        market: Value,
        user_errors: Vec<Value>,
    ) -> Value {
        selected_resource_payload(field, "market", market, user_errors, |market, selection| {
            self.selected_market_json(market, selection)
        })
    }

    fn selected_catalog_json(&self, catalog: &Value, selections: &[SelectedField]) -> Value {
        selected_record_with_connections(catalog, selections, |selection| {
            match selection.name.as_str() {
                "markets" => Some(self.selected_catalog_markets_connection(catalog, selection)),
                "priceList" => {
                    let price_list_id = catalog_relation_id(catalog, "priceListId", "priceList");
                    price_list_id
                        .as_deref()
                        .and_then(|id| self.store.staged.price_lists.get(id))
                        .map(|price_list| {
                            self.selected_price_list_json(price_list, &selection.selection)
                        })
                        .or_else(|| selected_record_field(catalog, selection))
                }
                "publication" => {
                    let publication_id =
                        catalog_relation_id(catalog, "publicationId", "publication");
                    publication_id
                        .as_deref()
                        .and_then(|id| self.store.staged.publications.get(id))
                        .map(|publication| selected_json(publication, &selection.selection))
                        .or_else(|| selected_record_field(catalog, selection))
                }
                _ => None,
            }
        })
    }

    fn selected_catalog_markets_connection(
        &self,
        catalog: &Value,
        selection: &SelectedField,
    ) -> Value {
        let embedded = catalog["markets"]["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|market| {
                market["id"]
                    .as_str()
                    .map(|id| (id.to_string(), market.clone()))
            })
            .collect::<BTreeMap<_, _>>();
        let records = catalog_market_ids(catalog)
            .into_iter()
            .rev()
            .map(|id| {
                self.store
                    .staged
                    .markets
                    .get(&id)
                    .cloned()
                    .or_else(|| embedded.get(&id).cloned())
                    .unwrap_or_else(|| json!({ "id": id }))
            })
            .collect::<Vec<_>>();
        selected_typed_connection_with_args(
            &records,
            &selection.arguments,
            &selection.selection,
            |market, node_selection| self.selected_market_json(market, node_selection),
            value_id_cursor,
        )
    }

    pub(in crate::proxy) fn selected_catalog_payload(
        &self,
        field: &RootFieldSelection,
        catalog: Value,
        user_errors: Vec<Value>,
    ) -> Value {
        selected_resource_payload(
            field,
            "catalog",
            catalog,
            user_errors,
            |catalog, selection| self.selected_catalog_json(catalog, selection),
        )
    }

    pub(in crate::proxy) fn selected_price_list_json(
        &self,
        price_list: &Value,
        selection: &[SelectedField],
    ) -> Value {
        if price_list.is_null() {
            return Value::Null;
        }
        let mut record = serde_json::Map::new();
        for field in selection {
            if !selected_field_applies_to_type("PriceList", field) {
                continue;
            }
            let value = match field.name.as_str() {
                "prices" => Some(selected_price_list_prices(
                    price_list,
                    &field.arguments,
                    &field.selection,
                )),
                "quantityRules" => Some(selected_price_list_quantity_rules(
                    price_list,
                    &field.arguments,
                    &field.selection,
                )),
                "catalog" => {
                    let catalog_id = catalog_relation_id(price_list, "catalogId", "catalog");
                    catalog_id
                        .as_deref()
                        .and_then(|id| self.store.staged.catalogs.get(id))
                        .map(|catalog| self.selected_catalog_json(catalog, &field.selection))
                        .or_else(|| selected_record_field(price_list, field))
                }
                _ => selected_record_field(price_list, field),
            };
            if let Some(value) = value {
                record.insert(field.response_key.clone(), value);
            }
        }
        Value::Object(record)
    }

    pub(in crate::proxy) fn selected_price_list_payload(
        &self,
        field: &RootFieldSelection,
        price_list: Value,
        user_errors: Vec<Value>,
    ) -> Value {
        selected_payload_json(&field.selection, |selection| {
            match selection.name.as_str() {
                "priceList" => {
                    Some(self.selected_price_list_json(&price_list, &selection.selection))
                }
                "userErrors" => Some(selected_user_errors(&user_errors, &selection.selection)),
                _ => None,
            }
        })
    }

    pub(in crate::proxy) fn selected_price_list_outcome(
        &self,
        field: &RootFieldSelection,
        price_list: Value,
        user_errors: Vec<Value>,
    ) -> PriceListFieldOutcome {
        PriceListFieldOutcome::payload(self.selected_price_list_payload(
            field,
            price_list,
            user_errors,
        ))
    }

    pub(in crate::proxy) fn selected_price_lists_connection_with_args(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
        selection: &[SelectedField],
    ) -> Value {
        let records = self
            .store
            .staged
            .price_lists
            .values()
            .cloned()
            .collect::<Vec<_>>();
        selected_typed_connection_with_args(
            &records,
            arguments,
            selection,
            |price_list, node_selection| self.selected_price_list_json(price_list, node_selection),
            value_id_cursor,
        )
    }

    pub(in crate::proxy) fn selected_web_presence_json(
        &self,
        web_presence: &Value,
        selections: &[SelectedField],
    ) -> Value {
        let market_ids = web_presence_market_ids(web_presence);
        selected_record_with_connections(web_presence, selections, |selection| {
            match selection.name.as_str() {
                "markets" => Some(self.selected_markets_by_ids_connection(
                    market_ids.clone(),
                    &selection.arguments,
                    &selection.selection,
                )),
                _ => None,
            }
        })
    }

    fn selected_markets_by_ids_connection(
        &self,
        market_ids: Vec<String>,
        arguments: &BTreeMap<String, ResolvedValue>,
        selection: &[SelectedField],
    ) -> Value {
        let records = market_ids
            .into_iter()
            .map(|id| {
                self.store
                    .staged
                    .markets
                    .get(&id)
                    .cloned()
                    .unwrap_or_else(|| json!({ "id": id }))
            })
            .collect::<Vec<_>>();
        selected_typed_connection_with_args(
            &records,
            arguments,
            selection,
            |market, node_selection| self.selected_market_json(market, node_selection),
            value_id_cursor,
        )
    }

    fn markets_resolved_values_value(&self, field: &RootFieldSelection) -> Value {
        let price_inclusivity = self.markets_resolved_price_inclusivity(field);
        let mut payload = serde_json::Map::new();
        for selection in &field.selection {
            let value = match selection.name.as_str() {
                "currencyCode" => Some(json!(self.store.shop_currency_code())),
                "priceInclusivity" => Some(selected_json(&price_inclusivity, &selection.selection)),
                "catalogs" => Some(
                    self.catalogs_connection_from_args(&selection.arguments, &selection.selection),
                ),
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

    pub(in crate::proxy) fn hydrate_markets_resolved_values_pricing_if_selected(
        &mut self,
        request: &Request,
        fields: &[RootFieldSelection],
    ) {
        let mut needs_currency = false;
        let mut needs_tax_flags = false;
        for field in fields
            .iter()
            .filter(|field| field.name == "marketsResolvedValues")
        {
            needs_currency |= field
                .selection
                .iter()
                .any(|selection| selection.name == "currencyCode");
            needs_tax_flags |= field
                .selection
                .iter()
                .any(|selection| selection.name == "priceInclusivity");
        }
        self.hydrate_shop_pricing_state_if_missing(request, needs_currency, needs_tax_flags);
    }

    pub(in crate::proxy) fn hydrate_market_currency_defaults_if_needed(
        &mut self,
        request: &Request,
        fields: &[RootFieldSelection],
    ) {
        let needs_currency = fields.iter().any(market_field_omits_base_currency);
        self.hydrate_shop_pricing_state_if_missing(request, needs_currency, false);
    }

    fn markets_resolved_price_inclusivity(&self, field: &RootFieldSelection) -> Value {
        let matched_market = self.markets_resolved_values_market(field);
        let duties_included = self.store.shop_duties_included().unwrap_or(false);
        let taxes_included = matched_market
            .and_then(market_taxes_included)
            .or_else(|| self.store.shop_taxes_included())
            .unwrap_or(false);
        json!({
            "dutiesIncluded": duties_included,
            "taxesIncluded": taxes_included
        })
    }

    fn markets_resolved_values_market(&self, field: &RootFieldSelection) -> Option<&Value> {
        let buyer_country = resolved_object_field(&field.arguments, "buyerSignal")
            .and_then(|buyer_signal| resolved_string_field(&buyer_signal, "countryCode"))
            .map(|country_code| country_code.to_ascii_uppercase());
        match buyer_country {
            Some(country_code) => self.store.staged.markets.values().find(|market| {
                market_record_enabled(market)
                    && market_record_country_codes(market)
                        .iter()
                        .any(|code| code.eq_ignore_ascii_case(&country_code))
            }),
            None => self
                .store
                .staged
                .markets
                .values()
                .find(|market| market_record_enabled(market)),
        }
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
                "marketUpdate" => self.market_update_response(field, request),
                "marketDelete" => self.market_delete_response(field, request),
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
            self.mark_markets_family_dirty("markets");
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

    pub(in crate::proxy) fn market_mutation_wrong_resource_response(
        &self,
        fields: &[RootFieldSelection],
    ) -> Option<Response> {
        let errors = fields
            .iter()
            .filter_map(market_mutation_wrong_resource_error)
            .collect::<Vec<_>>();
        if errors.is_empty() {
            return None;
        }
        let data = fields
            .iter()
            .map(|field| (field.response_key.clone(), Value::Null))
            .collect::<serde_json::Map<_, _>>();
        Some(ok_json(json!({
            "data": Value::Object(data),
            "errors": errors
        })))
    }

    pub(in crate::proxy) fn market_mutation_target_preflight(
        &mut self,
        fields: &[RootFieldSelection],
        request: &Request,
    ) {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return;
        }
        let ids = fields
            .iter()
            .filter(|field| matches!(field.name.as_str(), "marketUpdate" | "marketDelete"))
            .filter_map(|field| resolved_string_field(&field.arguments, "id"))
            .filter(|id| !self.store.staged.markets.contains_key(id))
            .filter(|id| !self.store.staged.deleted_market_ids.contains(id))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        self.hydrate_market_mutation_targets(&ids, request);
    }

    fn hydrate_market_mutation_target(&mut self, id: &str, request: &Request) {
        self.hydrate_market_mutation_targets(&[id.to_string()], request);
    }

    fn hydrate_market_mutation_targets(&mut self, ids: &[String], request: &Request) {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return;
        }
        let ids = ids
            .iter()
            .filter(|id| !id.trim().is_empty())
            .filter(|id| !self.store.staged.markets.contains_key(*id))
            .filter(|id| !self.store.staged.deleted_market_ids.contains(*id))
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        if ids.is_empty() {
            return;
        }
        let body = json!({
            "query": MARKET_MUTATION_TARGETS_HYDRATE_QUERY,
            "variables": { "ids": ids },
            "operationName": "MarketsMutationPreflightHydrate",
        });
        self.run_markets_preflight(request, body, |proxy, body| {
            proxy.hydrate_markets_from_upstream(body);
            for family in ["markets", "catalogs", "priceLists", "webPresences"] {
                proxy
                    .store
                    .staged
                    .markets_hydrated_scopes
                    .insert(format!("{family}:{{}}"));
            }
        });
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
                json!("TAKEN"),
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
        let shop_currency_code = self.store.shop_currency_code();
        let market = market_record_from_input(
            &id,
            &input,
            &name,
            &handle,
            &region_codes,
            &shop_currency_code,
        );
        self.store.staged.deleted_market_ids.remove(&id);
        self.store.staged.markets.insert(id, market.clone());
        self.selected_market_payload(field, market, Vec::new())
    }

    pub(in crate::proxy) fn market_delete_response(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
    ) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        self.hydrate_market_mutation_target(&id, request);
        let payload = if self.store.staged.markets.remove(&id).is_some() {
            self.store.staged.deleted_market_ids.insert(id.clone());
            self.cascade_market_delete(&id);
            json!({"deletedId": id, "userErrors": []})
        } else {
            market_id_payload_error("deletedId", "Market does not exist", "MARKET_NOT_FOUND")
        };
        selected_json(&payload, &field.selection)
    }

    pub(in crate::proxy) fn cascade_market_delete(&mut self, market_id: &str) {
        self.mark_markets_family_dirty("catalogs");
        self.mark_markets_family_dirty("webPresences");
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

    pub(in crate::proxy) fn market_update_response(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
    ) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        self.hydrate_market_mutation_target(&id, request);
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
            self.mark_markets_family_dirty("catalogs");
            self.add_market_to_catalog(&catalog_id, &id);
        }
        for catalog_id in list_string_field(&input, "catalogsToDelete") {
            self.mark_markets_family_dirty("catalogs");
            self.remove_market_from_catalog(&catalog_id, &id);
        }
        for web_presence_id in web_presences_to_add {
            self.mark_markets_family_dirty("webPresences");
            self.add_market_to_web_presence(&web_presence_id, &id);
        }
        for web_presence_id in list_string_field(&input, "webPresencesToDelete") {
            self.mark_markets_family_dirty("webPresences");
            self.remove_market_from_web_presence(&web_presence_id, &id);
        }

        let mut updated_market = existing_market;
        let shop_currency_code = self.store.shop_currency_code();
        Self::apply_market_update_scalar_fields(
            &mut updated_market,
            &input,
            &id,
            &shop_currency_code,
        );
        self.set_market_relation_fields(&mut updated_market, &id);
        self.store.staged.markets.insert(id, updated_market.clone());
        self.selected_market_payload(field, updated_market, Vec::new())
    }

    fn apply_market_update_scalar_fields(
        market: &mut Value,
        input: &BTreeMap<String, ResolvedValue>,
        market_id: &str,
        shop_currency_code: &str,
    ) {
        let existing_region_codes = market_record_country_codes(market);
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
            let currency_settings = market_update_currency_settings_json(
                object.get("currencySettings"),
                input,
                shop_currency_code,
            );
            object.insert("currencySettings".to_string(), currency_settings);
        }
        if matches!(input.get("priceInclusions"), Some(ResolvedValue::Object(_))) {
            let price_inclusions =
                market_update_price_inclusions_json(object.get("priceInclusions"), input);
            object.insert("priceInclusions".to_string(), price_inclusions);
        }
        if market_update_region_input_present(input) {
            let mut region_codes = existing_region_codes;
            if input.contains_key("regions")
                || resolved_object_field(input, "conditions")
                    .is_some_and(|conditions| conditions.contains_key("regionsCondition"))
            {
                region_codes = market_region_country_codes(input);
            }
            if let Some(conditions) = resolved_object_field(input, "conditions") {
                if let Some(to_delete) = resolved_object_field(&conditions, "conditionsToDelete") {
                    let deleted = market_region_country_codes(&to_delete);
                    region_codes.retain(|code| !deleted.contains(code));
                }
                if let Some(to_add) = resolved_object_field(&conditions, "conditionsToAdd") {
                    for code in market_region_country_codes(&to_add) {
                        if !region_codes.contains(&code) {
                            region_codes.push(code);
                        }
                    }
                }
            }
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

    pub(in crate::proxy) fn available_backup_region_for_code(
        &self,
        country_code: &str,
    ) -> Option<Value> {
        self.store
            .staged
            .available_backup_regions
            .get(&country_code.to_ascii_uppercase())
            .cloned()
    }

    pub(in crate::proxy) fn hydrate_available_backup_regions_from_upstream(
        &mut self,
        request: &Request,
    ) -> Response {
        let response = self.upstream_post(
            request,
            json!({
                "query": BACKUP_REGION_AVAILABLE_HYDRATE_QUERY,
                "operationName": "BackupRegionAvailableHydrate",
                "variables": {}
            }),
        );
        if response.status < 400 {
            self.hydrate_available_backup_regions_from_body(&response.body);
        }
        response
    }

    fn hydrate_available_backup_regions_from_body(&mut self, body: &Value) {
        let Some(regions) = body
            .pointer("/data/availableBackupRegions")
            .and_then(Value::as_array)
        else {
            return;
        };
        for region in regions {
            let Some(code) = region_code_from_node(region).map(|code| code.to_ascii_uppercase())
            else {
                continue;
            };
            if let Some(region) = market_region_country_from_node(region, &code) {
                self.store
                    .staged
                    .available_backup_regions
                    .insert(code, region);
            }
        }
    }

    /// True when any markets-domain record has been staged. Tracks local markets query state (minus the product check, since the Rust
    /// markets stores are staged-only with no base layer). Once a lifecycle has
    /// staged a market/catalog/price-list/web-presence, plural reads serve
    /// locally (read-after-write); before that, cold reads forward upstream.
    pub(in crate::proxy) fn has_markets_overlay_state(&self) -> bool {
        !self.store.staged.markets.is_empty()
            || !self.store.staged.deleted_market_ids.is_empty()
            || !self.store.staged.catalogs.is_empty()
            || !self.store.staged.price_lists.is_empty()
            || !self.store.staged.web_presences.is_empty()
    }

    /// LiveHybrid cold-read decision for the Markets domain. When this returns
    /// true, at least one requested root/scope still needs Shopify baseline
    /// data before local staged deltas can be projected correctly.
    pub(in crate::proxy) fn markets_should_fetch_upstream(
        &self,
        fields: &[RootFieldSelection],
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> bool {
        let plural_localizable_state_selected = fields
            .iter()
            .any(|field| field.name == "marketLocalizableResources")
            && self.has_market_localizable_resource_state();
        fields.iter().any(|field| {
            self.markets_field_should_fetch_upstream(
                field,
                variables,
                plural_localizable_state_selected,
            )
        })
    }

    fn markets_field_should_fetch_upstream(
        &self,
        field: &RootFieldSelection,
        variables: &BTreeMap<String, ResolvedValue>,
        plural_localizable_state_selected: bool,
    ) -> bool {
        match field.name.as_str() {
            "market" => {
                !markets_field_has_local_id(&field.arguments, &self.store.staged.markets)
                    && !resolved_string_field(&field.arguments, "id")
                        .is_some_and(|id| self.store.staged.deleted_market_ids.contains(&id))
            }
            "catalog" => !markets_field_has_local_id(&field.arguments, &self.store.staged.catalogs),
            "priceList" => {
                !markets_field_has_local_id(&field.arguments, &self.store.staged.price_lists)
            }
            "marketLocalizableResource" => resolved_string_field(&field.arguments, "resourceId")
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
            | "marketsResolvedValues" => markets_hydration_scope_keys(field)
                .iter()
                .any(|key| self.markets_scope_needs_upstream(key)),
            "marketLocalizableResources" => !self.has_market_localizable_resource_state(),
            "marketLocalizableResourcesByIds" => {
                !plural_localizable_state_selected
                    && self.market_localizable_resources_by_ids_should_fetch_upstream(variables)
            }
            _ => false,
        }
    }

    pub(in crate::proxy) fn mark_markets_family_dirty(&mut self, family: &str) {
        self.store
            .staged
            .markets_dirty_families
            .insert(family.to_string());
    }

    #[allow(dead_code)]
    fn markets_scope_needs_upstream(&self, key: &str) -> bool {
        if self.store.staged.markets_hydrated_scopes.contains(key) {
            return false;
        }
        let family = markets_scope_family(key);
        !self.store.staged.markets_dirty_families.contains(family)
            || self.markets_family_has_records(family)
    }

    #[allow(dead_code)]
    fn markets_family_has_records(&self, family: &str) -> bool {
        match family {
            "markets" => !self.store.staged.markets.is_empty(),
            "catalogs" => !self.store.staged.catalogs.is_empty(),
            "priceLists" => !self.store.staged.price_lists.is_empty(),
            "webPresences" => !self.store.staged.web_presences.is_empty(),
            _ => false,
        }
    }

    #[allow(dead_code)]
    pub(in crate::proxy) fn hydrate_markets_from_upstream_for_fields(
        &mut self,
        body: &Value,
        fields: &[RootFieldSelection],
    ) {
        let normalized = markets_body_with_canonical_response_keys(body, fields);
        self.hydrate_markets_from_upstream(&normalized);
        self.mark_markets_hydrated_scopes_from_fields(&normalized, fields);
    }

    #[allow(dead_code)]
    fn mark_markets_hydrated_scopes_from_fields(
        &mut self,
        body: &Value,
        fields: &[RootFieldSelection],
    ) {
        let Some(data) = body.get("data").and_then(Value::as_object) else {
            return;
        };
        for field in fields {
            if !markets_response_connection_complete(data.get(&field.response_key)) {
                continue;
            }
            for key in markets_hydration_scope_keys(field) {
                self.store.staged.markets_hydrated_scopes.insert(key);
            }
        }
    }

    /// Hydrate the staged markets stores from an upstream GraphQL response body,
    /// fed by captured upstream response hydration. Records are observed as a side effect of a cold read so later targets
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
                if !self.store.staged.deleted_market_ids.contains(&id) {
                    self.store.staged.markets.insert(id, record.clone());
                }
            }
        }
        // Catalogs: top-level plus nested under each market.
        let mut catalog_records = markets_collect_records(data, "catalogs", "catalog");
        for market in &market_records {
            catalog_records.extend(
                market
                    .get("catalogs")
                    .map(connection_nodes)
                    .unwrap_or_default(),
            );
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
                if markets_record_is_richer_than_existing(
                    &self.store.staged.price_lists,
                    &id,
                    record,
                ) {
                    self.store.staged.price_lists.insert(id, record.clone());
                }
            }
        }
        // Web presences: top-level plus nested under each market.
        let mut web_presence_records = markets_collect_records(data, "webPresences", "webPresence");
        for market in &market_records {
            web_presence_records.extend(
                market
                    .get("webPresences")
                    .map(connection_nodes)
                    .unwrap_or_default(),
            );
        }
        web_presence_records.extend(
            hydrate_nodes
                .iter()
                .filter(|node| {
                    node.get("__typename").and_then(Value::as_str) == Some("MarketWebPresence")
                        || record_gid(node, "MarketWebPresence").is_some()
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
        localizable_records.extend(
            data.get("marketLocalizableResourcesByIds")
                .map(connection_nodes)
                .unwrap_or_default(),
        );
        for record in &localizable_records {
            self.stage_observed_market_localizable_resource(record);
        }
    }
}

fn market_mutation_wrong_resource_error(field: &RootFieldSelection) -> Option<Value> {
    if !matches!(field.name.as_str(), "marketUpdate" | "marketDelete") {
        return None;
    }
    let id = resolved_string_field(&field.arguments, "id")?;
    match shopify_gid_resource_type(&id) {
        Some("Market") | None => None,
        Some(_) => Some(json!({
            "message": format!("Invalid id: {id}"),
            "locations": [{"line": field.location.line, "column": field.location.column}],
            "extensions": {"code": "RESOURCE_NOT_FOUND"},
            "path": [field.response_key.clone()]
        })),
    }
}

fn markets_field_has_local_id(
    arguments: &BTreeMap<String, ResolvedValue>,
    records: &BTreeMap<String, Value>,
) -> bool {
    resolved_string_field(arguments, "id")
        .as_deref()
        .is_some_and(|id| is_synthetic_gid(id) || records.contains_key(id))
}

fn markets_hydration_scope_keys(field: &RootFieldSelection) -> Vec<String> {
    match field.name.as_str() {
        "markets" => vec![markets_hydration_scope_key("markets", &field.arguments)],
        "catalogs" | "catalogsCount" => {
            vec![markets_hydration_scope_key("catalogs", &field.arguments)]
        }
        "priceLists" => vec![markets_hydration_scope_key("priceLists", &field.arguments)],
        "webPresences" => vec![markets_hydration_scope_key(
            "webPresences",
            &field.arguments,
        )],
        "marketsResolvedValues" => field
            .selection
            .iter()
            .filter_map(|selection| match selection.name.as_str() {
                "catalogs" => Some(markets_hydration_scope_key(
                    "catalogs",
                    &selection.arguments,
                )),
                "webPresences" => Some(markets_hydration_scope_key(
                    "webPresences",
                    &selection.arguments,
                )),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

#[allow(dead_code)]
fn markets_hydration_scope_key(
    family: &str,
    arguments: &BTreeMap<String, ResolvedValue>,
) -> String {
    let scope_arguments = arguments
        .iter()
        .filter(|(name, _)| !markets_pagination_argument(name))
        .map(|(name, value)| (name.clone(), resolved_value_json(value)))
        .collect::<serde_json::Map<_, _>>();
    format!("{family}:{}", Value::Object(scope_arguments))
}

#[allow(dead_code)]
fn markets_scope_family(key: &str) -> &str {
    key.split_once(':').map(|(family, _)| family).unwrap_or(key)
}

#[allow(dead_code)]
fn markets_pagination_argument(name: &str) -> bool {
    matches!(name, "first" | "last" | "after" | "before")
}

#[allow(dead_code)]
fn markets_response_connection_complete(value: Option<&Value>) -> bool {
    let Some(connection) = value.filter(|value| value.is_object()) else {
        return false;
    };
    if connection.get("nodes").is_none() && connection.get("edges").is_none() {
        return false;
    }
    connection
        .get("pageInfo")
        .and_then(|page_info| page_info.get("hasNextPage"))
        .and_then(Value::as_bool)
        != Some(true)
}

fn markets_record_is_richer_than_existing(
    records: &BTreeMap<String, Value>,
    id: &str,
    candidate: &Value,
) -> bool {
    records
        .get(id)
        .map(|existing| {
            candidate.as_object().map_or(0, serde_json::Map::len)
                > existing.as_object().map_or(0, serde_json::Map::len)
        })
        .unwrap_or(true)
}

#[allow(dead_code)]
fn markets_body_with_canonical_response_keys(body: &Value, fields: &[RootFieldSelection]) -> Value {
    let mut normalized = body.clone();
    let Some(data) = normalized.get_mut("data").and_then(Value::as_object_mut) else {
        return normalized;
    };
    for field in fields {
        if field.response_key == field.name || data.contains_key(&field.name) {
            continue;
        }
        if let Some(value) = data.get(&field.response_key).cloned() {
            data.insert(field.name.clone(), value);
        }
    }
    normalized
}

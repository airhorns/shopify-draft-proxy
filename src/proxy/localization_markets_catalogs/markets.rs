use super::*;

pub(in crate::proxy) fn markets_field_resolver_registrations() -> Vec<FieldResolverRegistration> {
    let mut registrations = vec![
        FieldResolverRegistration::explicit(
            ApiSurface::Admin,
            "PriceList",
            "prices",
            price_list_prices_field,
        ),
        FieldResolverRegistration::explicit(
            ApiSurface::Admin,
            "PriceList",
            "quantityRules",
            price_list_quantity_rules_field,
        ),
        FieldResolverRegistration::explicit(
            ApiSurface::Admin,
            "PriceListPrice",
            "quantityPriceBreaks",
            price_list_price_quantity_breaks_field,
        ),
        FieldResolverRegistration::explicit(
            ApiSurface::Admin,
            "RegionsCondition",
            "regions",
            regions_condition_regions_field,
        ),
        FieldResolverRegistration::explicit(
            ApiSurface::Admin,
            "Market",
            "regions",
            market_regions_field,
        ),
    ];
    for (parent_type, field_name, handler) in [
        (
            "Market",
            "catalogs",
            market_catalogs_field as crate::resolver_registry::FieldResolverHandler,
        ),
        ("Market", "webPresences", market_web_presences_field),
        ("MarketCatalog", "markets", market_catalog_markets_field),
        ("MarketWebPresence", "markets", web_presence_markets_field),
        (
            "MarketsResolvedValues",
            "catalogs",
            markets_resolved_catalogs_field,
        ),
        (
            "MarketsResolvedValues",
            "webPresences",
            markets_resolved_web_presences_field,
        ),
    ] {
        registrations.push(FieldResolverRegistration::explicit(
            ApiSurface::Admin,
            parent_type,
            field_name,
            handler,
        ));
    }
    for parent_type in ["AppCatalog", "CompanyLocationCatalog", "MarketCatalog"] {
        registrations.push(FieldResolverRegistration::explicit(
            ApiSurface::Admin,
            parent_type,
            "priceList",
            catalog_price_list_field,
        ));
        registrations.push(FieldResolverRegistration::explicit(
            ApiSurface::Admin,
            parent_type,
            "publication",
            catalog_publication_field,
        ));
    }
    registrations.push(FieldResolverRegistration::explicit(
        ApiSurface::Admin,
        "PriceList",
        "catalog",
        price_list_catalog_field,
    ));
    registrations
}

pub(in crate::proxy) fn markets_field_resolver_type_policies() -> Vec<FieldResolverTypePolicy> {
    [
        "AppCatalog",
        "Catalog",
        "CatalogCsvOperation",
        "ChannelsCondition",
        "CompanyLocationsCondition",
        "LocationsCondition",
        "Locale",
        "Market",
        "MarketCatalog",
        "MarketConditions",
        "MarketCurrencySettings",
        "MarketDeliveryConfigurations",
        "MarketLocalization",
        "MarketLocalizableResource",
        "MarketPriceInclusions",
        "MarketRegion",
        "MarketRegionCountry",
        "MarketRegionSubdivision",
        "MarketRegionSubdivisionCountry",
        "MarketsB2BEntitlement",
        "MarketsCatalogsEntitlement",
        "MarketsRegionsEntitlement",
        "MarketsResolvedValues",
        "MarketsRetailEntitlement",
        "MarketsThemesEntitlement",
        "MarketsType",
        "MarketWebPresence",
        "MarketWebPresenceRootUrl",
        "PriceList",
        "PriceListAdjustment",
        "PriceListAdjustmentSettings",
        "PriceListParent",
        "PriceListPrice",
        "RegionsCondition",
        "ShopLocale",
        "TranslatableContent",
        "TranslatableResource",
        "Translation",
    ]
    .into_iter()
    .map(|parent_type| {
        FieldResolverTypePolicy::property_backed_ordinary_fields(
            ApiSurface::Admin,
            parent_type,
            "argument-bearing market or catalog field has no explicit canonical resolver",
        )
    })
    .collect()
}

fn field_arguments(
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> BTreeMap<String, ResolvedValue> {
    resolved_arguments_from_json(&invocation.arguments)
}

fn market_catalogs_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let market_id = invocation
        .parent
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    Ok(connection_value_with_args(
        market_related_records(
            proxy.store.staged.catalogs.values(),
            market_id,
            catalog_market_ids,
        ),
        &field_arguments(invocation),
        value_id_cursor,
    ))
}

fn market_web_presences_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let market_id = invocation
        .parent
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    Ok(connection_value_with_args(
        market_related_records(
            proxy.store.staged.web_presences.values(),
            market_id,
            web_presence_market_ids,
        ),
        &field_arguments(invocation),
        value_id_cursor,
    ))
}

fn related_markets(proxy: &DraftProxy, parent: &Value, ids: Vec<String>) -> Vec<Value> {
    let embedded = parent
        .get("markets")
        .map(connection_nodes)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|market| {
            let id = market
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_string)?;
            Some((id, market))
        })
        .collect::<BTreeMap<_, _>>();
    let ids = if ids.is_empty() {
        embedded.keys().cloned().collect()
    } else {
        ids
    };
    ids.into_iter()
        .filter_map(|id| {
            proxy
                .store
                .staged
                .markets
                .get(&id)
                .cloned()
                .or_else(|| embedded.get(&id).cloned())
        })
        .collect()
}

fn market_catalog_markets_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let mut markets = related_markets(
        proxy,
        invocation.parent,
        catalog_market_ids(invocation.parent),
    );
    markets.reverse();
    Ok(connection_value_with_args(
        markets,
        &field_arguments(invocation),
        value_id_cursor,
    ))
}

fn web_presence_markets_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let cursors_by_id = invocation
        .parent
        .pointer("/markets/edges")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|edge| {
            Some((
                edge.pointer("/node/id")?.as_str()?.to_string(),
                edge.get("cursor")?.as_str()?.to_string(),
            ))
        })
        .collect::<BTreeMap<_, _>>();
    Ok(connection_value_with_args(
        related_markets(
            proxy,
            invocation.parent,
            web_presence_market_ids(invocation.parent),
        ),
        &field_arguments(invocation),
        |market| {
            market
                .get("id")
                .and_then(Value::as_str)
                .and_then(|id| cursors_by_id.get(id).cloned())
                .unwrap_or_else(|| value_id_cursor(market))
        },
    ))
}

fn markets_resolved_catalogs_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let arguments = field_arguments(invocation);
    let result = proxy.matching_catalogs_query(&arguments);
    Ok(typed_connection_value(
        &result.records,
        Value::clone,
        value_id_cursor,
        result.page_info,
    ))
}

fn markets_resolved_web_presences_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    Ok(connection_value_with_args(
        proxy.store.staged.web_presences.values().cloned().collect(),
        &field_arguments(invocation),
        value_id_cursor,
    ))
}

fn catalog_price_list_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let id = catalog_relation_id(invocation.parent, "priceListId", "priceList");
    Ok(id
        .as_deref()
        .and_then(|id| proxy.store.staged.price_lists.get(id))
        .cloned()
        .or_else(|| invocation.parent.get("priceList").cloned())
        .unwrap_or(Value::Null))
}

fn catalog_publication_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let id = catalog_relation_id(invocation.parent, "publicationId", "publication");
    Ok(id
        .as_deref()
        .and_then(|id| proxy.store.staged.publications.get(id))
        .cloned()
        .or_else(|| invocation.parent.get("publication").cloned())
        .unwrap_or(Value::Null))
}

fn price_list_catalog_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let id = catalog_relation_id(invocation.parent, "catalogId", "catalog");
    Ok(id
        .as_deref()
        .and_then(|id| proxy.store.staged.catalogs.get(id))
        .cloned()
        .or_else(|| invocation.parent.get("catalog").cloned())
        .unwrap_or(Value::Null))
}

fn market_regions_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let market = invocation
        .parent
        .get("id")
        .and_then(Value::as_str)
        .and_then(|id| proxy.store.staged.markets.get(id))
        .unwrap_or(invocation.parent);
    let connection = market
        .get("regions")
        .or_else(|| market.pointer("/conditions/regionsCondition/regions"));
    Ok(connection_value_with_args(
        connection.map(connection_nodes).unwrap_or_default(),
        &resolved_arguments_from_json(&invocation.arguments),
        value_id_cursor,
    ))
}

fn regions_condition_regions_field(
    _proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let connection = invocation.parent.get("regions");
    Ok(connection_value_with_args(
        connection.map(connection_nodes).unwrap_or_default(),
        &resolved_arguments_from_json(&invocation.arguments),
        value_id_cursor,
    ))
}

fn canonical_price_list_for_field(
    proxy: &DraftProxy,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Value {
    invocation
        .parent
        .get("id")
        .and_then(Value::as_str)
        .and_then(|id| proxy.store.staged.price_lists.get(id))
        .cloned()
        .unwrap_or_else(|| invocation.parent.clone())
}

fn price_list_prices_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let price_list = canonical_price_list_for_field(proxy, invocation);
    Ok(price_list_prices_value(
        &price_list,
        &resolved_arguments_from_json(&invocation.arguments),
    ))
}

fn price_list_quantity_rules_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let price_list = canonical_price_list_for_field(proxy, invocation);
    Ok(price_list_quantity_rules_value(
        &price_list,
        &resolved_arguments_from_json(&invocation.arguments),
    ))
}

fn price_list_price_quantity_breaks_field(
    _proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let nodes = invocation
        .parent
        .get("quantityPriceBreaks")
        .map(connection_nodes)
        .unwrap_or_default();
    Ok(connection_value_with_args(
        nodes,
        &resolved_arguments_from_json(&invocation.arguments),
        value_id_cursor,
    ))
}

impl DraftProxy {
    pub(crate) fn markets_query_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let operation_roots = invocation.operation_roots.clone();
        let RootInvocation {
            response_key,
            request,
            root_name,
            arguments,
            requested_field_paths,
            ..
        } = invocation;
        let arguments = resolved_arguments_from_json(&arguments);
        if self.config.read_mode == ReadMode::LiveHybrid
            && !self.execution_session.markets_query_preflighted
            && self.markets_operation_should_fetch_upstream(&operation_roots)
        {
            let had_markets_overlay_state = self.has_markets_overlay_state();
            let result = self.cached_or_forward_upstream_graphql_result(request, response_key);
            let upstream_succeeded = result.transport_succeeded && result.outcome.errors.is_empty();
            if upstream_succeeded {
                let body = json!({ "data": result.data });
                self.hydrate_markets_from_upstream_roots(&body, &operation_roots);
                self.hydrate_localization_from_upstream(&body);
            }
            self.execution_session.markets_query_preflighted = true;
            if !had_markets_overlay_state
                && (upstream_succeeded
                    || !self.localization_operation_has_local_overlay(&operation_roots))
            {
                return result.outcome;
            }
        }
        self.hydrate_markets_resolved_values_pricing_if_selected(
            request,
            root_name,
            &requested_field_paths,
        );
        ResolverOutcome::value(self.markets_overlay_query_value(root_name, &arguments))
    }

    pub(crate) fn markets_mutation_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let RootInvocation {
            response_key,
            request,
            query,
            variables,
            root_name,
            root_location,
            arguments,
            ..
        } = invocation;
        let field = MarketsRootInput {
            name: root_name.to_string(),
            response_key: response_key.to_string(),
            location: root_location,
            arguments: resolved_arguments_from_json(&arguments),
        };
        self.hydrate_market_currency_defaults_if_needed(request, root_name, &field.arguments);
        if let Some(error) = self.market_mutation_wrong_resource_error(&field) {
            return graphql_error_outcome(vec![error], response_key);
        }
        if matches!(
            root_name,
            "priceListCreate"
                | "priceListUpdate"
                | "priceListDelete"
                | "priceListFixedPricesByProductUpdate"
                | "priceListFixedPricesAdd"
                | "priceListFixedPricesUpdate"
                | "priceListFixedPricesDelete"
        ) {
            return self.price_list_mutation_outcome(&field, request, query, variables);
        }
        let value = if matches!(
            root_name,
            "marketLocalizationsRegister" | "marketLocalizationsRemove"
        ) {
            self.market_localization_mutation_preflight(variables, request);
            self.market_localization_mutation_value(&field)
        } else if matches!(
            root_name,
            "catalogCreate" | "catalogUpdate" | "catalogDelete" | "catalogContextUpdate"
        ) {
            self.catalog_mutation_data(&field, request, query, variables)
        } else {
            self.market_mutation_target_preflight(&field, request);
            self.market_create_mutation_data(&field, request, query, variables)
        };
        if matches!(
            root_name,
            "marketLocalizationsRegister" | "marketLocalizationsRemove"
        ) {
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                root_name,
                vec![response_key.to_string()],
            );
        }
        ResolverOutcome::value(value)
    }

    /// Unified Markets overlay read. A single GraphQL query can select several
    /// markets-domain root fields at once (e.g. the delete-cascade downstream
    /// read selects `webPresences`, `market`, and `catalog` together). Routing
    /// the whole operation to one entity-specific handler would null every field
    /// that handler doesn't own, so each root field is projected independently
    /// from its staged store here.
    pub(in crate::proxy) fn markets_overlay_query_value(
        &self,
        root_name: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        match root_name {
            "market" => {
                let id = resolved_string_field(arguments, "id").unwrap_or_default();
                self.store
                    .staged
                    .markets
                    .get(&id)
                    .cloned()
                    .unwrap_or(Value::Null)
            }
            "catalog" => {
                let id = resolved_string_field(arguments, "id").unwrap_or_default();
                self.store
                    .staged
                    .catalogs
                    .get(&id)
                    .cloned()
                    .unwrap_or(Value::Null)
            }
            "catalogs" => self.catalogs_connection_canonical_value(arguments),
            "catalogsCount" => self.catalogs_effective_count(arguments),
            "priceList" => {
                let id = resolved_string_field(arguments, "id").unwrap_or_default();
                self.store
                    .staged
                    .price_lists
                    .get(&id)
                    .cloned()
                    .unwrap_or(Value::Null)
            }
            "priceLists" => connection_value_with_args(
                self.store.staged.price_lists.values().cloned().collect(),
                arguments,
                value_id_cursor,
            ),
            "webPresences" => {
                let records = self
                    .store
                    .staged
                    .web_presences
                    .values()
                    .cloned()
                    .collect::<Vec<_>>();
                connection_value_with_args(records, arguments, value_id_cursor)
            }
            "marketsResolvedValues" => self.markets_resolved_values_canonical_value(arguments),
            "marketLocalizableResource" => {
                let resource_id =
                    resolved_string_field(arguments, "resourceId").unwrap_or_default();
                if !self.market_localizable_resource_exists(&resource_id) {
                    Value::Null
                } else {
                    self.market_localizable_resource(&resource_id, None)
                }
            }
            "marketLocalizableResources" => self.market_localizable_resources_connection(arguments),
            "marketLocalizableResourcesByIds" => {
                self.market_localizable_resources_by_ids_connection(arguments)
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
                staged_connection_value_with_args(
                    records,
                    arguments,
                    market_search_decision,
                    market_sort_key,
                    Value::clone,
                    value_id_cursor,
                )
            }
            _ => Value::Null,
        }
    }

    fn catalogs_effective_count(&self, arguments: &BTreeMap<String, ResolvedValue>) -> Value {
        let key = markets_hydration_scope_key("catalogs", arguments);
        let Some(upstream_count) = self.store.staged.markets_upstream_counts.get(&key) else {
            return snapshot_count_with_limit_precision(
                self.matching_catalogs_query(arguments).total_count,
                arguments,
            );
        };
        let base_count = upstream_count
            .get("count")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        let precision = upstream_count
            .get("precision")
            .and_then(Value::as_str)
            .unwrap_or("EXACT");
        if precision != "EXACT" {
            return count_object_with_precision(base_count, precision);
        }
        let local_created = self.matching_created_catalog_count(arguments) as u64;
        snapshot_count_with_limit_precision((base_count + local_created) as usize, arguments)
    }

    fn matching_created_catalog_count(&self, arguments: &BTreeMap<String, ResolvedValue>) -> usize {
        let type_filter = resolved_string_field(arguments, "type");
        staged_connection_query(
            self.store
                .staged
                .created_catalog_ids
                .iter()
                .filter_map(|id| self.store.staged.catalogs.get(id))
                .cloned()
                .collect::<Vec<_>>(),
            arguments,
            move |catalog, query| catalog_search_decision(catalog, query, type_filter.as_deref()),
            catalog_staged_sort_key,
            value_id_cursor,
        )
        .total_count
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

    fn catalogs_connection_canonical_value(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let result = self.matching_catalogs_query(arguments);
        typed_connection_value(
            &result.records,
            Value::clone,
            value_id_cursor,
            result.page_info,
        )
    }

    fn selected_market_payload(
        &self,
        _field: &MarketsRootInput,
        market: Value,
        user_errors: Vec<Value>,
    ) -> Value {
        json!({ "market": market, "userErrors": user_errors })
    }

    pub(in crate::proxy) fn selected_catalog_payload(
        &self,
        _field: &MarketsRootInput,
        catalog: Value,
        user_errors: Vec<Value>,
    ) -> Value {
        json!({ "catalog": catalog, "userErrors": user_errors })
    }

    fn canonical_price_list_value(&self, price_list: &Value) -> Value {
        if price_list.is_null() {
            return Value::Null;
        }
        let mut record = price_list.clone();
        if let Some(catalog_id) = catalog_relation_id(price_list, "catalogId", "catalog") {
            if let Some(catalog) = self.store.staged.catalogs.get(&catalog_id) {
                record["catalog"] = catalog.clone();
            }
        }
        record
    }

    pub(in crate::proxy) fn selected_price_list_payload(
        &self,
        _field: &MarketsRootInput,
        price_list: Value,
        user_errors: Vec<Value>,
    ) -> Value {
        json!({
            "priceList": self.canonical_price_list_value(&price_list),
            "userErrors": user_errors
        })
    }

    pub(in crate::proxy) fn selected_price_list_outcome(
        &self,
        field: &MarketsRootInput,
        price_list: Value,
        user_errors: Vec<Value>,
    ) -> PriceListFieldOutcome {
        PriceListFieldOutcome::payload(self.selected_price_list_payload(
            field,
            price_list,
            user_errors,
        ))
    }

    fn markets_resolved_values_canonical_value(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        json!({
            "currencyCode": self.store.shop_currency_code(),
            "priceInclusivity": self.markets_resolved_price_inclusivity(arguments),
        })
    }

    pub(in crate::proxy) fn hydrate_markets_resolved_values_pricing_if_selected(
        &mut self,
        request: &Request,
        root_name: &str,
        requested_field_paths: &BTreeSet<Vec<String>>,
    ) {
        if root_name != "marketsResolvedValues" {
            return;
        }
        let needs_currency = requested_field_paths
            .iter()
            .any(|path| path.first().is_some_and(|field| field == "currencyCode"));
        let needs_tax_flags = requested_field_paths.iter().any(|path| {
            path.first()
                .is_some_and(|field| field == "priceInclusivity")
        });
        self.hydrate_shop_pricing_state_if_missing(request, needs_currency, needs_tax_flags);
    }

    pub(in crate::proxy) fn hydrate_market_currency_defaults_if_needed(
        &mut self,
        request: &Request,
        root_name: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) {
        let needs_currency = market_field_omits_base_currency(root_name, arguments);
        self.hydrate_shop_pricing_state_if_missing(request, needs_currency, false);
    }

    fn markets_resolved_price_inclusivity(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let matched_market = self.markets_resolved_values_market(arguments);
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

    fn markets_resolved_values_market(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Option<&Value> {
        let buyer_country = resolved_object_field(arguments, "buyerSignal")
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
        field: &MarketsRootInput,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let value = match field.name.as_str() {
            "marketCreate" => self.market_create_response(field),
            "marketUpdate" => self.market_update_response(field, request),
            "marketDelete" => self.market_delete_response(field, request),
            _ => Value::Null,
        };
        let staged_ids = value["market"]["id"]
            .as_str()
            .or_else(|| value["deletedId"].as_str())
            .map(str::to_string)
            .into_iter()
            .collect::<Vec<_>>();
        if !staged_ids.is_empty() {
            self.mark_markets_family_dirty("markets");
            self.record_mutation_log_entry(request, query, variables, &field.name, staged_ids);
        }
        value
    }

    pub(in crate::proxy) fn market_mutation_wrong_resource_error(
        &self,
        field: &MarketsRootInput,
    ) -> Option<Value> {
        market_mutation_wrong_resource_error(field)
    }

    pub(in crate::proxy) fn market_mutation_target_preflight(
        &mut self,
        field: &MarketsRootInput,
        request: &Request,
    ) {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return;
        }
        let ids = matches!(field.name.as_str(), "marketUpdate" | "marketDelete")
            .then(|| resolved_string_field(&field.arguments, "id"))
            .flatten()
            .filter(|id| !self.store.staged.markets.contains_key(id))
            .filter(|id| !self.store.staged.deleted_market_ids.contains(id))
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
        });
    }

    pub(in crate::proxy) fn market_create_response(&mut self, field: &MarketsRootInput) -> Value {
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
        field: &MarketsRootInput,
        request: &Request,
    ) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        self.hydrate_market_mutation_target(&id, request);
        if self.store.staged.markets.remove(&id).is_some() {
            self.store.staged.deleted_market_ids.insert(id.clone());
            self.cascade_market_delete(&id);
            json!({"deletedId": id, "userErrors": []})
        } else {
            market_id_payload_error("deletedId", "Market does not exist", "MARKET_NOT_FOUND")
        }
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
        field: &MarketsRootInput,
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
    /// from the stored catalog by the GraphQL executor. Resolving from the live market
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

    fn markets_root_should_fetch_upstream(
        &self,
        root_name: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> bool {
        match root_name {
            "market" => {
                !markets_field_has_local_id(arguments, &self.store.staged.markets)
                    && !resolved_string_field(arguments, "id")
                        .is_some_and(|id| self.store.staged.deleted_market_ids.contains(&id))
            }
            "catalog" => !markets_field_has_local_id(arguments, &self.store.staged.catalogs),
            "priceList" => !markets_field_has_local_id(arguments, &self.store.staged.price_lists),
            "marketLocalizableResource" => resolved_string_field(arguments, "resourceId")
                .map(|resource_id| {
                    !self
                        .store
                        .staged
                        .localization_resources
                        .contains_key(&resource_id)
                })
                .unwrap_or(true),
            "markets" | "catalogs" | "priceLists" | "webPresences" => {
                let family = match root_name {
                    "markets" => "markets",
                    "catalogs" => "catalogs",
                    "priceLists" => "priceLists",
                    "webPresences" => "webPresences",
                    _ => unreachable!(),
                };
                self.markets_scope_needs_upstream(&markets_hydration_scope_key(family, arguments))
            }
            "catalogsCount" => {
                let key = markets_hydration_scope_key("catalogs", arguments);
                !self.store.staged.markets_upstream_counts.contains_key(&key)
            }
            "marketsResolvedValues" => !self.has_markets_overlay_state(),
            "marketLocalizableResources" => !self.has_market_localizable_resource_state(),
            "marketLocalizableResourcesByIds" => {
                self.market_localizable_resources_by_ids_should_fetch_upstream(arguments)
            }
            _ => false,
        }
    }

    fn markets_operation_should_fetch_upstream(
        &self,
        roots: &[crate::resolver_registry::OperationRootInvocation],
    ) -> bool {
        let plural_localizable_state_selected = roots
            .iter()
            .any(|root| root.name == "marketLocalizableResources")
            && self.has_market_localizable_resource_state();
        roots.iter().any(|root| {
            let arguments = resolved_arguments_from_json(&root.arguments);
            if root.name == "marketLocalizableResourcesByIds" {
                !plural_localizable_state_selected
                    && self.market_localizable_resources_by_ids_should_fetch_upstream(&arguments)
            } else {
                self.markets_root_should_fetch_upstream(&root.name, &arguments)
            }
        })
    }

    pub(in crate::proxy) fn mark_markets_family_dirty(&mut self, family: &str) {
        self.store
            .staged
            .markets_dirty_families
            .insert(family.to_string());
    }

    fn markets_scope_needs_upstream(&self, key: &str) -> bool {
        if self.store.staged.markets_hydrated_scopes.contains(key) {
            return false;
        }
        let family = markets_scope_family(key);
        !self.store.staged.markets_dirty_families.contains(family)
            || self.markets_family_has_records(family)
    }

    fn markets_family_has_records(&self, family: &str) -> bool {
        match family {
            "markets" => {
                !self.store.staged.markets.is_empty()
                    || !self.store.staged.deleted_market_ids.is_empty()
            }
            "catalogs" => !self.store.staged.catalogs.is_empty(),
            "priceLists" => !self.store.staged.price_lists.is_empty(),
            "webPresences" => !self.store.staged.web_presences.is_empty(),
            _ => false,
        }
    }

    fn hydrate_markets_from_upstream_root(
        &mut self,
        body: &Value,
        root_name: &str,
        response_key: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) {
        let mut normalized = body.clone();
        if response_key != root_name {
            if let Some(value) = normalized
                .pointer(&format!("/data/{response_key}"))
                .cloned()
            {
                normalized["data"][root_name] = value;
            }
        }
        self.hydrate_markets_from_upstream(&normalized);
        if root_name == "catalogsCount" {
            if let Some(count) = normalized["data"][root_name]
                .get("count")
                .and_then(Value::as_u64)
            {
                let precision = normalized["data"][root_name]
                    .get("precision")
                    .and_then(Value::as_str)
                    .unwrap_or("EXACT");
                let key = markets_hydration_scope_key("catalogs", arguments);
                self.store
                    .staged
                    .markets_upstream_counts
                    .insert(key, count_object_with_precision(count, precision));
            }
            return;
        }
        let family = match root_name {
            "markets" => Some("markets"),
            "catalogs" => Some("catalogs"),
            "priceLists" => Some("priceLists"),
            "webPresences" => Some("webPresences"),
            _ => None,
        };
        if let Some(family) = family {
            if markets_response_connection_complete(normalized["data"].get(root_name)) {
                self.store
                    .staged
                    .markets_hydrated_scopes
                    .insert(markets_hydration_scope_key(family, arguments));
            }
        }
    }

    fn hydrate_markets_from_upstream_roots(
        &mut self,
        body: &Value,
        roots: &[crate::resolver_registry::OperationRootInvocation],
    ) {
        for root in roots {
            self.hydrate_markets_from_upstream_root(
                body,
                &root.name,
                &root.response_key,
                &resolved_arguments_from_json(&root.arguments),
            );
        }
        self.stage_markets_upstream_counts_from_roots(body, roots);
    }

    fn stage_markets_upstream_counts_from_roots(
        &mut self,
        body: &Value,
        roots: &[crate::resolver_registry::OperationRootInvocation],
    ) {
        let Some(data) = body.get("data").and_then(Value::as_object) else {
            return;
        };
        for root in roots.iter().filter(|root| root.name == "catalogsCount") {
            let Some(count) = data
                .get(&root.response_key)
                .and_then(|value| value.get("count"))
                .and_then(Value::as_u64)
            else {
                continue;
            };
            let precision = data
                .get(&root.response_key)
                .and_then(|value| value.get("precision"))
                .and_then(Value::as_str)
                .unwrap_or("EXACT");
            let arguments = resolved_arguments_from_json(&root.arguments);
            let represented_created = if precision == "EXACT" {
                self.created_catalogs_represented_in_upstream_roots(data, roots, &arguments) as u64
            } else {
                0
            };
            let key = markets_hydration_scope_key("catalogs", &arguments);
            self.store.staged.markets_upstream_counts.insert(
                key,
                count_object_with_precision(count.saturating_sub(represented_created), precision),
            );
        }
    }

    fn created_catalogs_represented_in_upstream_roots(
        &self,
        data: &serde_json::Map<String, Value>,
        roots: &[crate::resolver_registry::OperationRootInvocation],
        count_arguments: &BTreeMap<String, ResolvedValue>,
    ) -> usize {
        let type_filter = resolved_string_field(count_arguments, "type");
        let query = resolved_string_field(count_arguments, "query");
        roots
            .iter()
            .filter(|root| root.name == "catalog")
            .filter_map(|root| {
                let arguments = resolved_arguments_from_json(&root.arguments);
                let id = resolved_string_field(&arguments, "id")?;
                let catalog = self.store.staged.catalogs.get(&id)?;
                if !self.store.staged.created_catalog_ids.contains(&id) {
                    return None;
                }
                if !matches!(
                    catalog_search_decision(catalog, query.as_deref(), type_filter.as_deref()),
                    StagedSearchDecision::Match
                ) {
                    return None;
                }
                data.get(&root.response_key)
                    .filter(|value| value.is_object())
                    .map(|_| id)
            })
            .collect::<BTreeSet<_>>()
            .len()
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
                    let hydrated = self
                        .store
                        .staged
                        .markets
                        .get(&id)
                        .filter(|_| self.store.staged.markets_dirty_families.contains("markets"))
                        .map(|existing| shallow_merged_object(record.clone(), existing.clone()))
                        .unwrap_or_else(|| record.clone());
                    self.store.staged.markets.insert(id, hydrated);
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
                let hydrated = self
                    .store
                    .staged
                    .catalogs
                    .get(&id)
                    .filter(|_| {
                        self.store
                            .staged
                            .markets_dirty_families
                            .contains("catalogs")
                    })
                    .map(|existing| shallow_merged_object(record.clone(), existing.clone()))
                    .unwrap_or_else(|| record.clone());
                self.store.staged.catalogs.insert(id, hydrated);
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
                if let Some(existing) = self
                    .store
                    .staged
                    .price_lists
                    .get(&id)
                    .filter(|_| {
                        self.store
                            .staged
                            .markets_dirty_families
                            .contains("priceLists")
                    })
                    .cloned()
                {
                    self.store
                        .staged
                        .price_lists
                        .insert(id, shallow_merged_object(record.clone(), existing));
                } else if markets_record_is_richer_than_existing(
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
                        if self
                            .store
                            .staged
                            .markets_dirty_families
                            .contains("webPresences")
                        {
                            return true;
                        }
                        record.as_object().map_or(0, serde_json::Map::len)
                            > existing.as_object().map_or(0, serde_json::Map::len)
                    })
                    .unwrap_or(true);
                if richer {
                    let hydrated = self
                        .store
                        .staged
                        .web_presences
                        .get(&id)
                        .filter(|_| {
                            self.store
                                .staged
                                .markets_dirty_families
                                .contains("webPresences")
                        })
                        .map(|existing| shallow_merged_object(record.clone(), existing.clone()))
                        .unwrap_or_else(|| record.clone());
                    self.store.staged.web_presences.insert(id, hydrated);
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

fn market_mutation_wrong_resource_error(field: &MarketsRootInput) -> Option<Value> {
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

fn markets_hydration_scope_key(
    family: &str,
    arguments: &BTreeMap<String, ResolvedValue>,
) -> String {
    let scope_arguments = arguments
        .iter()
        .filter(|(name, _)| !markets_pagination_argument(name))
        .filter(|(name, value)| {
            !(name.as_str() == "type" && matches!(value, ResolvedValue::Null)
                || name.as_str() == "limit" && matches!(value, ResolvedValue::Int(10_000)))
        })
        .map(|(name, value)| (name.clone(), resolved_value_json(value)))
        .collect::<serde_json::Map<_, _>>();
    format!("{family}:{}", Value::Object(scope_arguments))
}

fn markets_scope_family(key: &str) -> &str {
    key.split_once(':').map(|(family, _)| family).unwrap_or(key)
}

fn markets_pagination_argument(name: &str) -> bool {
    matches!(name, "first" | "last" | "after" | "before")
}

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

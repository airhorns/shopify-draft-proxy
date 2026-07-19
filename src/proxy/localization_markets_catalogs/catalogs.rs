use super::*;

impl DraftProxy {
    pub(in crate::proxy) fn catalog_mutation_data(
        &mut self,
        field: &MarketsRootInput,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let value = match field.name.as_str() {
            "catalogCreate" => self.catalog_create_response(field, request),
            "catalogUpdate" => self.catalog_update_response(field, request),
            "catalogDelete" => self.catalog_delete_response(field),
            "catalogContextUpdate" => self.catalog_context_update_response(field),
            _ => Value::Null,
        };
        let touched_ids = value["catalog"]["id"]
            .as_str()
            .or_else(|| value["deletedId"].as_str())
            .map(str::to_string)
            .into_iter()
            .collect::<Vec<_>>();
        if !touched_ids.is_empty() {
            self.mark_markets_family_dirty("catalogs");
            self.record_mutation_log_entry(request, query, variables, "catalog", touched_ids);
        }
        value
    }

    pub(in crate::proxy) fn catalog_create_response(
        &mut self,
        field: &MarketsRootInput,
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
                    "TAKEN",
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
        self.store.staged.created_catalog_ids.insert(id.clone());
        if let Some(price_list_id) = price_list_id.as_deref() {
            self.attach_price_list_to_catalog(&id, price_list_id);
        }
        self.selected_catalog_payload(field, catalog, Vec::new())
    }

    pub(in crate::proxy) fn catalog_update_response(
        &mut self,
        field: &MarketsRootInput,
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
        if let Some(title) = resolved_string_field(&input, "title") {
            if let Some(object) = updated_catalog.as_object_mut() {
                object.insert("title".to_string(), json!(title));
            }
        }
        if let Some(status) = resolved_string_field(&input, "status") {
            if let Some(object) = updated_catalog.as_object_mut() {
                object.insert("status".to_string(), json!(status));
            }
        }
        if let Some(context) = resolved_object_field(&input, "context") {
            if let Some(error) =
                self.apply_catalog_update_context_input(field, &mut updated_catalog, &context)
            {
                return error;
            }
        }

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
                    "TAKEN",
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
        self.selected_catalog_payload(field, updated_catalog, Vec::new())
    }

    fn apply_catalog_update_context_input(
        &self,
        field: &MarketsRootInput,
        catalog: &mut Value,
        context: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        let context_type_fields = catalog_context_type_fields(context);
        if context_type_fields.len() != 1 {
            return Some(selected_catalog_error(
                field,
                vec!["input", "context"],
                "Must provide exactly one context type.",
                "CONTEXT_DRIVER_MISMATCH",
            ));
        }

        let (driver, field_name) = context_type_fields[0];
        let catalog_driver = catalog_context_driver(catalog);
        if driver != catalog_driver {
            return Some(selected_catalog_error(
                field,
                vec!["input", "context", field_name],
                CATALOG_CONTEXT_DRIVER_MISMATCH_MESSAGE,
                "CONTEXT_DRIVER_MISMATCH",
            ));
        }

        match driver {
            CatalogContextDriver::Market => {
                let market_ids = list_string_field(context, field_name);
                for (index, market_id) in market_ids.iter().enumerate() {
                    if !self.market_exists(market_id) {
                        return Some(selected_catalog_error(
                            field,
                            vec!["input", "context", field_name, &index.to_string()],
                            "Market does not exist",
                            "MARKET_NOT_FOUND",
                        ));
                    }
                }
                let market_names = self.staged_market_names();
                set_catalog_market_ids(catalog, &market_ids, &market_names);
            }
            CatalogContextDriver::CompanyLocation => {
                let company_location_ids = company_location_ids_from_context(context);
                for (index, location_id) in company_location_ids.iter().enumerate() {
                    if !self.store.staged.b2b_locations.contains_key(location_id) {
                        return Some(selected_catalog_error(
                            field,
                            vec!["input", "context", field_name, &index.to_string()],
                            COMPANY_LOCATION_NOT_FOUND_MESSAGE,
                            "COMPANY_LOCATION_NOT_FOUND",
                        ));
                    }
                }
                set_catalog_company_location_ids(
                    catalog,
                    &company_location_ids,
                    &self.staged_company_locations_for_catalog(),
                );
            }
            CatalogContextDriver::Country => {
                let country_codes = country_codes_from_context(context);
                set_catalog_country_codes(catalog, &country_codes);
            }
        }
        None
    }

    pub(in crate::proxy) fn catalog_delete_response(&mut self, field: &MarketsRootInput) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        if let Some(catalog) = self.store.staged.catalogs.remove(&id) {
            self.store.staged.created_catalog_ids.remove(&id);
            self.detach_existing_catalog_price_list(&catalog);
            json!({"deletedId": id, "userErrors": []})
        } else {
            payload_user_error(
                "deletedId",
                catalog_user_error(vec!["id"], "Catalog does not exist", "CATALOG_NOT_FOUND"),
            )
        }
    }

    pub(in crate::proxy) fn catalog_context_update_response(
        &mut self,
        field: &MarketsRootInput,
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
            return self.selected_catalog_payload(field, Value::Null, errors);
        }

        let mut updated_catalog = existing_catalog;
        match catalog_driver {
            CatalogContextDriver::Market => {
                let mut market_ids = catalog_market_ids(&updated_catalog);
                apply_context_id_diff(
                    &mut market_ids,
                    contexts_to_remove.as_ref(),
                    contexts_to_add.as_ref(),
                    |context| list_string_field(context, "marketIds"),
                );
                let market_names = self.staged_market_names();
                set_catalog_market_ids(&mut updated_catalog, &market_ids, &market_names);
            }
            CatalogContextDriver::CompanyLocation => {
                let mut company_location_ids = catalog_company_location_ids(&updated_catalog);
                apply_context_id_diff(
                    &mut company_location_ids,
                    contexts_to_remove.as_ref(),
                    contexts_to_add.as_ref(),
                    company_location_ids_from_context,
                );
                set_catalog_company_location_ids(
                    &mut updated_catalog,
                    &company_location_ids,
                    &self.staged_company_locations_for_catalog(),
                );
            }
            CatalogContextDriver::Country => {
                let mut country_codes = catalog_country_codes(&updated_catalog);
                apply_context_id_diff(
                    &mut country_codes,
                    contexts_to_remove.as_ref(),
                    contexts_to_add.as_ref(),
                    country_codes_from_context,
                );
                set_catalog_country_codes(&mut updated_catalog, &country_codes);
            }
        }
        self.store
            .staged
            .catalogs
            .insert(catalog_id.clone(), updated_catalog.clone());
        self.selected_catalog_payload(field, updated_catalog, Vec::new())
    }

    pub(in crate::proxy) fn next_catalog_id(
        &mut self,
        driver_type: CatalogContextDriver,
    ) -> String {
        self.next_proxy_synthetic_gid(driver_type.catalog_type_name())
    }

    pub(in crate::proxy) fn staged_company_locations_for_catalog(&self) -> BTreeMap<String, Value> {
        self.store
            .staged
            .b2b_locations
            .iter()
            .map(|(id, location)| (id.clone(), location.clone()))
            .collect()
    }

    pub(in crate::proxy) fn price_list_mutation_outcome(
        &mut self,
        field: &MarketsRootInput,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> ResolverOutcome<Value> {
        self.fixed_price_mutation_preflight(field, request);
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
            "priceListFixedPricesUpdate" => {
                PriceListFieldOutcome::payload(self.price_list_fixed_prices_update_response(field))
            }
            "priceListFixedPricesDelete" => {
                PriceListFieldOutcome::payload(self.price_list_fixed_prices_delete_response(field))
            }
            "quantityRulesDelete" => PriceListFieldOutcome::payload(
                self.quantity_rules_delete_price_list_response(field),
            ),
            _ => PriceListFieldOutcome::payload(Value::Null),
        };
        let mut touched_ids = outcome.value["priceList"]["id"]
            .as_str()
            .or_else(|| outcome.value["webPresence"]["id"].as_str())
            .or_else(|| outcome.value["deletedId"].as_str())
            .map(str::to_string)
            .into_iter()
            .collect::<Vec<_>>();
        for id in outcome.value["deletedQuantityRulesVariantIds"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            push_unique_string(&mut touched_ids, id);
        }
        if !touched_ids.is_empty() {
            self.mark_markets_family_dirty("priceLists");
            self.record_mutation_log_entry(request, query, variables, "priceList", touched_ids);
        }
        ResolverOutcome::value(outcome.value).with_errors(root_field_errors_from_json(
            &outcome.errors,
            &field.response_key,
        ))
    }

    pub(in crate::proxy) fn price_list_create_response(
        &mut self,
        field: &MarketsRootInput,
    ) -> PriceListFieldOutcome {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let catalog_id = resolved_string_field(&input, "catalogId");
        if let Some(catalog_id) = catalog_id.as_deref() {
            if price_list_catalog_id_has_wrong_gid_type(catalog_id) {
                return PriceListFieldOutcome::resource_not_found(catalog_id, field);
            }
            if let Some(error) = self.price_list_catalog_validation_error(catalog_id, None) {
                return self.selected_price_list_outcome(field, Value::Null, vec![error]);
            }
        }

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

        let id = self.next_price_list_id();
        let price_list = price_list_record(
            &id,
            &name,
            &currency,
            price_list_parent_json(&parent),
            catalog_id.as_deref(),
        );
        if let Some(catalog_id) = catalog_id.as_deref() {
            self.attach_price_list_to_catalog(catalog_id, &id);
        }
        self.store.staged.price_lists.insert(id, price_list.clone());
        self.selected_price_list_outcome(field, price_list, Vec::new())
    }

    pub(in crate::proxy) fn price_list_update_response(
        &mut self,
        field: &MarketsRootInput,
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
                return self.selected_price_list_outcome(
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
                    return self.selected_price_list_outcome(field, Value::Null, vec![error]);
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
            let currency_changed = price_list_currency(&updated) != currency;
            if let Some(object) = updated.as_object_mut() {
                object.insert("currency".to_string(), json!(currency));
            }
            if currency_changed {
                clear_fixed_price_nodes(&mut updated);
            }
        }
        if let Some(parent) = parent_update.as_ref() {
            if let Some(object) = updated.as_object_mut() {
                object.insert("parent".to_string(), price_list_parent_json(parent));
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
        self.selected_price_list_outcome(field, updated, Vec::new())
    }

    pub(in crate::proxy) fn price_list_delete_response(
        &mut self,
        field: &MarketsRootInput,
    ) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        if self.store.staged.price_lists.remove(&id).is_some() {
            self.detach_price_list_from_catalogs(&id);
            json!({"deletedId": id, "userErrors": []})
        } else {
            price_list_payload_error(
                "deletedId",
                vec!["id"],
                "Price list does not exist.",
                "PRICE_LIST_NOT_FOUND",
            )
        }
    }

    /// Hydrate the staged store from a cassette-backed preflight before applying a
    /// fixed-price mutation, mirroring the production Admin GraphQL reads the live
    /// capture scripts record. Gated on LiveHybrid so other read modes are untouched.
    /// The cassette serves recorded real Shopify data, which the generic staging
    /// logic below loads into the local store — no fixture is hardcoded.
    pub(in crate::proxy) fn quantity_pricing_rules_mutation_preflight(
        &mut self,
        request: &Request,
        variables: &BTreeMap<String, ResolvedValue>,
    ) {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return;
        }
        let Some(price_list_id) =
            resolved_string_field(variables, "priceListId").filter(|id| !id.is_empty())
        else {
            return;
        };
        let variant_ids = quantity_pricing_rules_preflight_variant_ids(variables);
        let known_price_list = self.store.staged.price_lists.contains_key(&price_list_id);
        let known_variants = variant_ids
            .iter()
            .all(|id| self.store.has_product_variant_reference(id));
        if known_price_list
            && known_variants
            && !quantity_pricing_needs_price_break_preflight(variables)
        {
            return;
        }

        let body = json!({
            "query": QUANTITY_PRICING_RULES_PREFLIGHT_QUERY,
            "variables": resolved_variables_json(variables),
            "operationName": "MarketsMutationPreflightHydrate",
        });
        self.run_markets_preflight(request, body, Self::stage_fixed_price_preflight);
    }

    pub(in crate::proxy) fn fixed_price_mutation_preflight(
        &mut self,
        field: &MarketsRootInput,
        request: &Request,
    ) {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return;
        }
        let by_product = field.name == "priceListFixedPricesByProductUpdate";
        let variant_level = matches!(
            field.name.as_str(),
            "priceListFixedPricesAdd" | "priceListFixedPricesUpdate" | "priceListFixedPricesDelete"
        );
        let body = if by_product {
            let preflight_variables =
                product_fixed_prices_preflight_variables(&field.name, &field.arguments);
            if !self.product_fixed_prices_preflight_needed(&preflight_variables) {
                return;
            }
            json!({
                "query": FIXED_PRICE_BY_PRODUCT_PREFLIGHT_QUERY,
                "variables": preflight_variables,
                "operationName": "MarketsMutationPreflightHydrate",
            })
        } else if variant_level {
            let preflight_variables =
                variant_fixed_prices_preflight_variables(&field.name, &field.arguments);
            if !self.variant_fixed_prices_preflight_needed(&preflight_variables) {
                return;
            }
            json!({
                "query": FIXED_PRICE_VARIANT_PREFLIGHT_QUERY,
                "variables": preflight_variables,
                "operationName": "MarketsMutationPreflightHydrate",
            })
        } else {
            return;
        };
        self.run_markets_preflight(request, body, Self::stage_fixed_price_preflight);
    }

    fn product_fixed_prices_preflight_needed(&self, variables: &Value) -> bool {
        let Some(price_list_id) = variables.get("priceListId").and_then(Value::as_str) else {
            return false;
        };
        if price_list_id.is_empty() {
            return false;
        }
        if !self.store.staged.price_lists.contains_key(price_list_id) {
            return true;
        }
        Self::fixed_price_preflight_string_array(variables, "productIds")
            .iter()
            .any(|id| self.store.product_by_id(id).is_none())
    }

    fn variant_fixed_prices_preflight_needed(&self, variables: &Value) -> bool {
        let Some(price_list_id) = variables.get("priceListId").and_then(Value::as_str) else {
            return false;
        };
        if price_list_id.is_empty() {
            return false;
        }
        if !self.store.staged.price_lists.contains_key(price_list_id) {
            return true;
        }
        Self::fixed_price_preflight_string_array(variables, "variantIds")
            .iter()
            .any(|id| !self.store.has_product_variant_reference(id))
    }

    fn fixed_price_preflight_string_array(value: &Value, field: &str) -> Vec<String> {
        value
            .get(field)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect()
    }

    pub(in crate::proxy) fn run_markets_preflight(
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

    pub(in crate::proxy) fn cold_markets_preflight(
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
        field: &MarketsRootInput,
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
                json!({
                    "priceList": updated,
                    "pricesToAddProducts": prices_to_add_products,
                    "pricesToDeleteProducts": prices_to_delete_products,
                    "fixedPriceVariantIds": [],
                    "deletedFixedPriceVariantIds": [],
                    "userErrors": []
                })
            }
            (_, _) => json!({
                "priceList": null,
                "pricesToAddProducts": null,
                "pricesToDeleteProducts": null,
                "userErrors": errors
            }),
        }
    }

    pub(in crate::proxy) fn price_list_fixed_prices_add_response(
        &mut self,
        field: &MarketsRootInput,
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
                json!({"prices": prices, "userErrors": []})
            }
            (price_list, _) => {
                let prices = if price_list.is_some() {
                    json!([])
                } else {
                    Value::Null
                };
                json!({"prices": prices, "userErrors": errors})
            }
        }
    }

    pub(in crate::proxy) fn price_list_fixed_prices_update_response(
        &mut self,
        field: &MarketsRootInput,
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
                json!({
                    "priceList": updated,
                    "pricesAdded": prices_added,
                    "deletedFixedPriceVariantIds": deleted,
                    "userErrors": []
                })
            }
            (price_list, _) => {
                let empty_or_null = if price_list.is_some() {
                    json!([])
                } else {
                    Value::Null
                };
                json!({
                    "priceList": price_list.unwrap_or(Value::Null),
                    "pricesAdded": empty_or_null.clone(),
                    "deletedFixedPriceVariantIds": empty_or_null,
                    "userErrors": errors
                })
            }
        }
    }

    pub(in crate::proxy) fn price_list_fixed_prices_delete_response(
        &mut self,
        field: &MarketsRootInput,
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
                json!({"deletedFixedPriceVariantIds": deleted, "userErrors": []})
            }
            (price_list, _) => {
                let deleted = if price_list.is_some() {
                    json!([])
                } else {
                    Value::Null
                };
                json!({"deletedFixedPriceVariantIds": deleted, "userErrors": errors})
            }
        }
    }

    pub(in crate::proxy) fn quantity_rules_delete_price_list_response(
        &mut self,
        field: &MarketsRootInput,
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
        } else {
            let variant_errors = quantity_rules_delete_variant_errors(&self.store, &variant_ids);
            if variant_errors.is_empty() {
                if let Some(price_list) = self.store.staged.price_lists.get_mut(&price_list_id) {
                    delete_quantity_rule_nodes(price_list, &variant_ids);
                }
                json!({"deletedQuantityRulesVariantIds": variant_ids, "userErrors": []})
            } else {
                json!({"deletedQuantityRulesVariantIds": [], "userErrors": variant_errors})
            }
        };
        payload
    }

    pub(in crate::proxy) fn next_price_list_id(&mut self) -> String {
        self.next_proxy_synthetic_gid("PriceList")
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
}

use super::*;

const DELIVERY_PROFILE_VARIANTS_HYDRATE_QUERY: &str = "query ShippingDeliveryProfileVariantsHydrate($ids: [ID!]!) { nodes(ids: $ids) { ... on ProductVariant { id title product { id title handle } } } }";
const DELIVERY_PROFILE_LOCATION_NODES_HYDRATE_QUERY: &str = "query ShippingDeliveryProfileLocationNodesHydrate($ids: [ID!]!) { nodes(ids: $ids) { __typename ... on Location { id name isActive isFulfillmentService } } }";
// Must byte-match the recorded `ShippingDeliveryProfileHydrate` upstream call in
// the same captures. Issued when removing a profile the proxy has not staged
// locally, to learn whether the target is the shop's default profile (which
// cannot be deleted) from real store state rather than guessing.
const DELIVERY_PROFILE_DEFAULT_HYDRATE_QUERY: &str =
    "query ShippingDeliveryProfileHydrate($id: ID!) { deliveryProfile(id: $id) { id name default version } }";
const DELIVERY_PROFILE_UPDATE_HYDRATE_QUERY: &str = "query ShippingDeliveryProfileUpdateHydrate($id: ID!) { deliveryProfile(id: $id) { id name default version } }";
const DELIVERY_PROFILE_DEFAULT_REMOVE_MESSAGE: &str = "Cannot delete the default profile.";
const DELIVERY_PROFILE_LOCATION_CATALOG_FALLBACK_FIRST_VALUES: &[usize] = &[2, 3, 1];
const DELIVERY_PROFILE_GID_PREFIX: &str = "gid://shopify/DeliveryProfile/";

struct DeliveryProfileMutationOutcome {
    payload: Value,
    staged_ids: Vec<String>,
    errors: Vec<Value>,
}

impl DeliveryProfileMutationOutcome {
    fn staged(payload: Value, staged_ids: Vec<String>) -> Self {
        Self {
            payload,
            staged_ids,
            errors: Vec::new(),
        }
    }

    fn unstaged(payload: Value) -> Self {
        Self::staged(payload, Vec::new())
    }

    fn resource_not_found(field: &RootFieldSelection, id: &str) -> Self {
        Self {
            payload: Value::Null,
            staged_ids: Vec::new(),
            errors: vec![delivery_profile_resource_not_found_error(field, id)],
        }
    }
}

enum DeliveryProfileAssociationError {
    ResourceNotFound(String),
}

impl DraftProxy {
    pub(in crate::proxy) fn delivery_profile_read_response(
        &mut self,
        request: &Request,
        fields: &[RootFieldSelection],
    ) -> Response {
        // Cold-read passthrough: the merchant's pre-existing delivery profiles
        // (the default profile, the full catalog) are never staged locally — only
        // profiles this proxy created/updated/removed live in `staged`. When a read
        // targets a profile/catalog with no local overlay, forward upstream so the
        // real Shopify projection replays (the byte-exact recorded query matches the
        // cassette). Once a profile has been staged or tombstoned locally we serve it
        // from state so read-after-write reflects the mutation.
        if self.config.read_mode == ReadMode::LiveHybrid
            && self.delivery_profile_read_needs_upstream(fields)
        {
            let response = (self.upstream_transport)(request.clone());
            let observed_profiles = self.observe_delivery_profiles_response(&response);
            if !self.has_local_delivery_profile_overlay() {
                return response;
            }
            if !observed_profiles && self.store.base.delivery_profiles.order.is_empty() {
                return response;
            }
        }
        ok_json(json!({ "data": self.delivery_profile_read_data(fields) }))
    }

    fn delivery_profile_read_needs_upstream(&self, fields: &[RootFieldSelection]) -> bool {
        fields.iter().any(|field| match field.name.as_str() {
            "deliveryProfile" => {
                let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                !self.has_local_delivery_profile_overlay()
                    || !self.delivery_profile_is_known_locally(&id)
            }
            "deliveryProfiles" => true,
            _ => false,
        })
    }

    fn delivery_profile_is_known_locally(&self, id: &str) -> bool {
        if self.store.staged.delivery_profiles.is_tombstoned(id) {
            return true;
        }
        self.store.staged.delivery_profiles.contains_key(id)
            || self.store.base.delivery_profiles.get(id).is_some()
    }

    fn has_local_delivery_profile_overlay(&self) -> bool {
        self.store
            .staged
            .delivery_profiles
            .order
            .iter()
            .any(|id| !self.store.staged.delivery_profiles.is_tombstoned(id))
            || !self.store.staged.delivery_profiles.tombstones.is_empty()
    }

    fn observe_delivery_profiles_response(&mut self, response: &Response) -> bool {
        if !(200..300).contains(&response.status) {
            return false;
        }
        let mut profiles = Vec::new();
        collect_delivery_profile_response_values(&response.body["data"], &mut profiles);
        let mut observed = false;
        for profile in profiles {
            observed |= self.observe_base_delivery_profile(profile);
        }
        observed
    }

    fn observe_base_delivery_profile(&mut self, profile: Value) -> bool {
        let Some(profile) = normalized_delivery_profile_read_model(profile) else {
            return false;
        };
        let Some(id) = profile
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            return false;
        };
        self.store.base.delivery_profiles.insert(id, profile);
        true
    }

    pub(in crate::proxy) fn delivery_profile_read_data(
        &self,
        fields: &[RootFieldSelection],
    ) -> Value {
        root_payload_json(fields, |field| {
            Some(match field.name.as_str() {
                "deliveryProfile" => {
                    let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                    self.delivery_profile_for_read(&id)
                        .map(|profile| delivery_profile_selected_json(&profile, &field.selection))
                        .unwrap_or(Value::Null)
                }
                "deliveryProfiles" => {
                    self.delivery_profiles_connection_json(&field.arguments, &field.selection)
                }
                _ => return None,
            })
        })
    }

    pub(in crate::proxy) fn delivery_profile_mutation(
        &mut self,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let Some(fields) = root_fields(query, variables) else {
            return json_error(400, "Invalid delivery profile mutation");
        };
        let mut errors = Vec::new();
        let data = root_payload_json(&fields, |field| {
            let outcome = match field.name.as_str() {
                "deliveryProfileCreate" => self.delivery_profile_create_payload(field, request),
                "deliveryProfileUpdate" => self.delivery_profile_update_payload(field, request),
                "deliveryProfileRemove" => self.delivery_profile_remove_payload(field, request),
                _ => return None,
            };
            if !outcome.staged_ids.is_empty() {
                self.record_mutation_log_entry(
                    request,
                    query,
                    variables,
                    &field.name,
                    outcome.staged_ids,
                );
            }
            errors.extend(outcome.errors);
            Some(outcome.payload)
        });
        if data.as_object().is_none_or(serde_json::Map::is_empty) {
            json_error(
                501,
                &format!("Unsupported delivery profile mutation {root_field}"),
            )
        } else {
            let mut body = json!({ "data": data });
            if !errors.is_empty() {
                body["errors"] = Value::Array(errors);
            }
            ok_json(body)
        }
    }

    fn delivery_profile_create_payload(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
    ) -> DeliveryProfileMutationOutcome {
        let profile_input = resolved_object_field(&field.arguments, "profile").unwrap_or_default();
        let location_ids = delivery_profile_location_ids_from_input(&profile_input);
        self.hydrate_delivery_profile_locations(&location_ids, request);
        let mut location_exists =
            |location_id: &str| self.delivery_profile_location_exists(location_id);
        let user_errors = delivery_profile_create_user_errors(&profile_input, &mut location_exists);
        if !user_errors.is_empty() {
            return DeliveryProfileMutationOutcome::unstaged(delivery_profile_payload_json(
                Value::Null,
                &field.selection,
                user_errors,
            ));
        }

        let id = self.next_proxy_synthetic_gid("DeliveryProfile");
        let mut profile = self.delivery_profile_from_input(&id, &profile_input);
        if let Err(error) =
            self.delivery_profile_apply_associations(&mut profile, &profile_input, true, request)
        {
            return match error {
                DeliveryProfileAssociationError::ResourceNotFound(id) => {
                    DeliveryProfileMutationOutcome::resource_not_found(field, &id)
                }
            };
        }
        self.stage_delivery_profile(profile.clone());
        DeliveryProfileMutationOutcome::staged(
            delivery_profile_payload_json(profile, &field.selection, Vec::new()),
            vec![id],
        )
    }

    fn delivery_profile_update_payload(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
    ) -> DeliveryProfileMutationOutcome {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let Some(mut profile) = self
            .delivery_profile_for_read(&id)
            .or_else(|| self.delivery_profile_hydrate_for_update(&id, request))
        else {
            return DeliveryProfileMutationOutcome::unstaged(delivery_profile_payload_json(
                Value::Null,
                &field.selection,
                vec![user_error_omit_code(
                    Value::Null,
                    "Profile could not be updated.",
                    None,
                )],
            ));
        };

        let profile_input = resolved_object_field(&field.arguments, "profile").unwrap_or_default();
        let location_ids = delivery_profile_location_ids_from_input(&profile_input);
        self.hydrate_delivery_profile_locations(&location_ids, request);
        let mut location_exists =
            |location_id: &str| self.delivery_profile_location_exists(location_id);
        let user_errors = delivery_profile_update_user_errors(&profile_input, &mut location_exists);
        if !user_errors.is_empty() {
            return DeliveryProfileMutationOutcome::unstaged(delivery_profile_payload_json(
                Value::Null,
                &field.selection,
                user_errors,
            ));
        }

        if profile["default"].as_bool() != Some(true) {
            if let Some(name) = resolved_string_field(&profile_input, "name") {
                profile["name"] = json!(name);
            }
        }
        let version = profile["version"].as_i64().unwrap_or(1) + 1;
        profile["version"] = json!(version);
        self.delivery_profile_apply_update_input(&mut profile, &profile_input);
        if let Err(error) =
            self.delivery_profile_apply_associations(&mut profile, &profile_input, false, request)
        {
            return match error {
                DeliveryProfileAssociationError::ResourceNotFound(id) => {
                    DeliveryProfileMutationOutcome::resource_not_found(field, &id)
                }
            };
        }
        self.stage_delivery_profile(profile.clone());
        DeliveryProfileMutationOutcome::staged(
            delivery_profile_payload_json(profile, &field.selection, Vec::new()),
            vec![id],
        )
    }

    fn delivery_profile_remove_payload(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
    ) -> DeliveryProfileMutationOutcome {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let profile = self.delivery_profile_for_read(&id);
        if profile
            .as_ref()
            .and_then(|profile| profile.get("default"))
            .and_then(Value::as_bool)
            == Some(true)
        {
            let (payload, ids) = delivery_profile_remove_default_payload(&field.selection);
            return DeliveryProfileMutationOutcome::staged(payload, ids);
        }
        if profile.is_none() {
            if self.delivery_profile_hydrates_as_default(&id, request) {
                let (payload, ids) = delivery_profile_remove_default_payload(&field.selection);
                return DeliveryProfileMutationOutcome::staged(payload, ids);
            }
            return DeliveryProfileMutationOutcome::unstaged(delivery_profile_remove_payload_json(
                Value::Null,
                &field.selection,
                vec![user_error_omit_code(
                    Value::Null,
                    "The Delivery Profile cannot be found for the shop.",
                    None,
                )],
            ));
        }

        self.store.staged.delivery_profiles.remove(&id);
        self.store.staged.delivery_profiles.tombstone(id.clone());
        let job = json!({
            "id": self.next_proxy_synthetic_gid("Job"),
            "done": false
        });
        DeliveryProfileMutationOutcome::staged(
            delivery_profile_remove_payload_json(job, &field.selection, Vec::new()),
            vec![id],
        )
    }

    fn delivery_profile_from_input(
        &mut self,
        id: &str,
        input: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let groups = resolved_object_list_field(input, "locationGroupsToCreate")
            .into_iter()
            .map(|group_input| self.delivery_location_group_from_input(&group_input))
            .collect::<Vec<_>>();
        let mut profile = json!({
            "id": id,
            "name": resolved_string_field(input, "name").unwrap_or_default(),
            "default": false,
            "version": 1,
            "profileLocationGroups": groups,
            "profileItems": [],
            "sellingPlanGroups": [],
            "unassignedLocations": [],
            "locationsWithoutRatesCount": 0
        });
        refresh_delivery_profile_counts(&mut profile);
        profile
    }

    fn delivery_location_group_from_input(
        &mut self,
        input: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let location_ids = list_string_field(input, "locations");
        let locations = location_ids
            .into_iter()
            .map(|id| self.delivery_profile_location_record(&id))
            .collect::<Vec<_>>();
        let zones = resolved_object_list_field(input, "zonesToCreate")
            .into_iter()
            .map(|zone_input| self.delivery_zone_record_from_input(&zone_input))
            .collect::<Vec<_>>();
        let mut group = json!({
            "locationGroup": {
                "id": self.next_proxy_synthetic_gid("DeliveryLocationGroup"),
                "locations": locations,
                "locationsCount": count_object(locations.len())
            },
            "locationGroupZones": zones,
            "countriesInAnyZone": []
        });
        refresh_delivery_location_group_countries(&mut group);
        group
    }

    fn delivery_zone_record_from_input(
        &mut self,
        input: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let countries = delivery_profile_countries_from_input(input);
        let methods = resolved_object_list_field(input, "methodDefinitionsToCreate")
            .into_iter()
            .map(|method_input| self.delivery_method_definition_from_input(&method_input))
            .collect::<Vec<_>>();
        json!({
            "zone": {
                "id": self.next_proxy_synthetic_gid("DeliveryZone"),
                "name": resolved_string_field(input, "name").unwrap_or_default(),
                "countries": countries
            },
            "methodDefinitions": methods
        })
    }

    fn delivery_method_definition_from_input(
        &mut self,
        input: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let price = delivery_price_from_method_input(input);
        let mut conditions = Vec::new();
        for condition in resolved_object_list_field(input, "weightConditionsToCreate") {
            conditions.push(self.delivery_weight_condition_from_input(&condition));
        }
        for condition in resolved_object_list_field(input, "priceConditionsToCreate") {
            conditions.push(self.delivery_price_condition_from_input(&condition));
        }
        json!({
            "id": self.next_proxy_synthetic_gid("DeliveryMethodDefinition"),
            "name": resolved_string_field(input, "name").unwrap_or_default(),
            "active": resolved_bool_field(input, "active").unwrap_or(true),
            "description": delivery_method_description_from_input(input),
            "rateProvider": {
                "__typename": "DeliveryRateDefinition",
                "id": self.next_proxy_synthetic_gid("DeliveryRateDefinition"),
                "price": price
            },
            "methodConditions": conditions
        })
    }

    fn delivery_weight_condition_from_input(
        &mut self,
        input: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let criteria = resolved_object_field(input, "criteria").unwrap_or_default();
        json!({
            "id": self.next_proxy_synthetic_gid("DeliveryCondition"),
            "field": "TOTAL_WEIGHT",
            "operator": resolved_string_field(input, "operator").unwrap_or_else(|| "GREATER_THAN_OR_EQUAL_TO".to_string()),
            "conditionCriteria": {
                "__typename": "Weight",
                "value": resolved_number_field(&criteria, "value").unwrap_or(0.0),
                "unit": resolved_string_field(&criteria, "unit").unwrap_or_else(|| "KILOGRAMS".to_string())
            }
        })
    }

    fn delivery_price_condition_from_input(
        &mut self,
        input: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let criteria = resolved_object_field(input, "criteria").unwrap_or_default();
        json!({
            "id": self.next_proxy_synthetic_gid("DeliveryCondition"),
            "field": "TOTAL_PRICE",
            "operator": resolved_string_field(input, "operator").unwrap_or_else(|| "LESS_THAN_OR_EQUAL_TO".to_string()),
            "conditionCriteria": {
                "__typename": "MoneyV2",
                "amount": money_amount_string_from_resolved(criteria.get("amount")),
                "currencyCode": resolved_string_field(&criteria, "currencyCode").unwrap_or_else(|| "USD".to_string())
            }
        })
    }

    fn delivery_profile_apply_update_input(
        &mut self,
        profile: &mut Value,
        input: &BTreeMap<String, ResolvedValue>,
    ) {
        self.delivery_profile_delete_conditions(profile, input);
        self.delivery_profile_create_location_groups(profile, input);
        self.delivery_profile_update_location_groups(profile, input);
        refresh_delivery_profile_counts(profile);
    }

    fn delivery_profile_delete_conditions(
        &self,
        profile: &mut Value,
        input: &BTreeMap<String, ResolvedValue>,
    ) {
        let delete_ids = list_string_field(input, "conditionsToDelete")
            .into_iter()
            .collect::<BTreeSet<_>>();
        if delete_ids.is_empty() {
            return;
        }
        for group in profile["profileLocationGroups"]
            .as_array_mut()
            .into_iter()
            .flatten()
        {
            for zone in group["locationGroupZones"]
                .as_array_mut()
                .into_iter()
                .flatten()
            {
                for method in zone["methodDefinitions"]
                    .as_array_mut()
                    .into_iter()
                    .flatten()
                {
                    if let Some(conditions) = method["methodConditions"].as_array_mut() {
                        conditions.retain(|condition| {
                            condition
                                .get("id")
                                .and_then(Value::as_str)
                                .is_none_or(|id| !delete_ids.contains(id))
                        });
                    }
                }
            }
        }
    }

    fn delivery_profile_create_location_groups(
        &mut self,
        profile: &mut Value,
        input: &BTreeMap<String, ResolvedValue>,
    ) {
        for group_input in resolved_object_list_field(input, "locationGroupsToCreate") {
            let group = self.delivery_location_group_from_input(&group_input);
            if let Some(groups) = profile["profileLocationGroups"].as_array_mut() {
                groups.push(group);
            }
        }
    }

    fn delivery_profile_update_location_groups(
        &mut self,
        profile: &mut Value,
        input: &BTreeMap<String, ResolvedValue>,
    ) {
        for group_update in resolved_object_list_field(input, "locationGroupsToUpdate") {
            let group_id = resolved_string_field(&group_update, "id").unwrap_or_default();
            let Some(group) = profile["profileLocationGroups"]
                .as_array_mut()
                .into_iter()
                .flatten()
                .find(|group| group["locationGroup"]["id"].as_str() == Some(group_id.as_str()))
            else {
                continue;
            };
            self.delivery_profile_add_locations_to_group(group, &group_update);
            self.delivery_profile_update_zones(group, &group_update);
            refresh_delivery_location_group_countries(group);
        }
    }

    fn delivery_profile_add_locations_to_group(
        &mut self,
        group: &mut Value,
        group_update: &BTreeMap<String, ResolvedValue>,
    ) {
        let Some(locations) = group["locationGroup"]["locations"].as_array_mut() else {
            return;
        };
        for location_id in list_string_field(group_update, "locationsToAdd") {
            if !locations.iter().any(|location| {
                location.get("id").and_then(Value::as_str) == Some(location_id.as_str())
            }) {
                locations.push(self.delivery_profile_location_record(&location_id));
            }
        }
        let count = locations.len();
        group["locationGroup"]["locationsCount"] = count_object(count);
    }

    fn delivery_profile_update_zones(
        &mut self,
        group: &mut Value,
        group_update: &BTreeMap<String, ResolvedValue>,
    ) {
        for zone_update in resolved_object_list_field(group_update, "zonesToUpdate") {
            let zone_id = resolved_string_field(&zone_update, "id").unwrap_or_default();
            let Some(zone) = group["locationGroupZones"]
                .as_array_mut()
                .into_iter()
                .flatten()
                .find(|zone| zone["zone"]["id"].as_str() == Some(zone_id.as_str()))
            else {
                continue;
            };
            self.delivery_profile_update_zone(zone, &zone_update);
        }
    }

    fn delivery_profile_update_zone(
        &mut self,
        zone: &mut Value,
        zone_update: &BTreeMap<String, ResolvedValue>,
    ) {
        if let Some(name) = resolved_string_field(zone_update, "name") {
            zone["zone"]["name"] = json!(name);
        }
        if zone_update.contains_key("countries") {
            zone["zone"]["countries"] = json!(delivery_profile_countries_from_input(zone_update));
        }
        self.delivery_profile_update_method_definitions(zone, zone_update);
        self.delivery_profile_create_method_definitions(zone, zone_update);
    }

    fn delivery_profile_update_method_definitions(
        &self,
        zone: &mut Value,
        zone_update: &BTreeMap<String, ResolvedValue>,
    ) {
        for method_update in resolved_object_list_field(zone_update, "methodDefinitionsToUpdate") {
            let method_id = resolved_string_field(&method_update, "id").unwrap_or_default();
            let Some(method) = zone["methodDefinitions"]
                .as_array_mut()
                .into_iter()
                .flatten()
                .find(|method| method["id"].as_str() == Some(method_id.as_str()))
            else {
                continue;
            };
            if let Some(name) = resolved_string_field(&method_update, "name") {
                method["name"] = json!(name);
            }
            if let Some(active) = resolved_bool_field(&method_update, "active") {
                method["active"] = json!(active);
            }
            if method_update.contains_key("description") {
                method["description"] = delivery_method_description_from_input(&method_update);
            }
            if method_update.contains_key("rateDefinition") {
                method["rateProvider"]["price"] = delivery_price_from_method_input(&method_update);
            }
        }
    }

    fn delivery_profile_create_method_definitions(
        &mut self,
        zone: &mut Value,
        zone_update: &BTreeMap<String, ResolvedValue>,
    ) {
        let mut new_methods = resolved_object_list_field(zone_update, "methodDefinitionsToCreate")
            .into_iter()
            .map(|method_input| self.delivery_method_definition_from_input(&method_input))
            .collect::<Vec<_>>();
        if let Some(methods) = zone["methodDefinitions"].as_array_mut() {
            methods.append(&mut new_methods);
        }
    }

    fn delivery_profile_apply_associations(
        &mut self,
        profile: &mut Value,
        input: &BTreeMap<String, ResolvedValue>,
        create: bool,
        request: &Request,
    ) -> Result<(), DeliveryProfileAssociationError> {
        let mut variant_ids = profile["profileItems"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|item| {
                item["variants"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|variant| variant.get("id").and_then(Value::as_str))
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        for variant_id in list_string_field(input, "variantsToAssociate") {
            if !is_shopify_gid_of_type(&variant_id, "ProductVariant") {
                return Err(DeliveryProfileAssociationError::ResourceNotFound(
                    variant_id,
                ));
            }
            if !variant_ids.contains(&variant_id) {
                variant_ids.push(variant_id);
            }
        }
        if !create {
            let removals = list_string_field(input, "variantsToDissociate");
            for variant_id in &removals {
                if !is_shopify_gid_of_type(variant_id, "ProductVariant") {
                    return Err(DeliveryProfileAssociationError::ResourceNotFound(
                        variant_id.clone(),
                    ));
                }
            }
            let removals = removals.into_iter().collect::<BTreeSet<_>>();
            variant_ids.retain(|variant_id| !removals.contains(variant_id));
        }
        let mut resolved_items = BTreeMap::new();
        let unresolved_variant_ids = variant_ids
            .iter()
            .filter_map(|variant_id| {
                match self.delivery_profile_item_for_local_variant(variant_id) {
                    Some(item) => {
                        resolved_items.insert(variant_id.clone(), item);
                        None
                    }
                    None => Some(variant_id.clone()),
                }
            })
            .collect::<Vec<_>>();
        resolved_items.extend(
            self.delivery_profile_hydrated_variant_items(&unresolved_variant_ids, request)?,
        );
        profile["profileItems"] = Value::Array(
            variant_ids
                .into_iter()
                .filter_map(|variant_id| resolved_items.remove(&variant_id))
                .collect(),
        );
        refresh_delivery_profile_counts(profile);
        Ok(())
    }

    fn delivery_profile_item_for_local_variant(&self, variant_id: &str) -> Option<Value> {
        if let Some(variant) = self.store.product_variant_by_id(variant_id) {
            let product = self.store.product_by_id(&variant.product_id)?;
            return Some(delivery_profile_item_for_resolved_variant(
                &variant.id,
                &variant.title,
                &product.id,
                &product.title,
            ));
        }
        let (variant, product) = self.store.fixed_price_variant_lookup(variant_id)?;
        let variant_title = variant.get("title").and_then(Value::as_str)?;
        Some(delivery_profile_item_for_resolved_variant(
            variant_id,
            variant_title,
            &product.id,
            &product.title,
        ))
    }

    fn delivery_profile_hydrated_variant_items(
        &mut self,
        variant_ids: &[String],
        request: &Request,
    ) -> Result<BTreeMap<String, Value>, DeliveryProfileAssociationError> {
        if variant_ids.is_empty() || self.config.read_mode != ReadMode::LiveHybrid {
            return Ok(BTreeMap::new());
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": DELIVERY_PROFILE_VARIANTS_HYDRATE_QUERY,
                "variables": { "ids": variant_ids }
            }),
        );
        if !(200..300).contains(&response.status)
            || response
                .body
                .get("errors")
                .and_then(Value::as_array)
                .is_some_and(|errors| !errors.is_empty())
        {
            return Err(DeliveryProfileAssociationError::ResourceNotFound(
                variant_ids[0].clone(),
            ));
        }
        let Some(nodes) = response
            .body
            .pointer("/data/nodes")
            .and_then(Value::as_array)
        else {
            return Err(DeliveryProfileAssociationError::ResourceNotFound(
                variant_ids[0].clone(),
            ));
        };
        Ok(nodes
            .iter()
            .filter_map(|node| {
                let id = node.get("id").and_then(Value::as_str)?.to_string();
                let item = delivery_profile_item_for_observed_variant(node)?;
                Some((id, item))
            })
            .collect())
    }

    fn delivery_profile_hydrates_as_default(&self, id: &str, request: &Request) -> bool {
        if id.is_empty() {
            return false;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": DELIVERY_PROFILE_DEFAULT_HYDRATE_QUERY,
                "variables": { "id": id }
            }),
        );
        response
            .body
            .pointer("/data/node")
            .or_else(|| response.body.pointer("/data/deliveryProfile"))
            .and_then(|profile| profile.get("default"))
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    fn delivery_profile_hydrate_for_update(&self, id: &str, request: &Request) -> Option<Value> {
        if self.config.read_mode != ReadMode::LiveHybrid || id.is_empty() {
            return None;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": DELIVERY_PROFILE_UPDATE_HYDRATE_QUERY,
                "variables": { "id": id }
            }),
        );
        let mut profile = response
            .body
            .pointer("/data/deliveryProfile")
            .or_else(|| response.body.pointer("/data/node"))
            .filter(|profile| profile.get("id").and_then(Value::as_str) == Some(id))?
            .clone();
        if profile.get("profileLocationGroups").is_none() {
            profile["profileLocationGroups"] = json!([]);
        }
        if profile.get("profileItems").is_none() {
            profile["profileItems"] = json!([]);
        }
        if profile.get("sellingPlanGroups").is_none() {
            profile["sellingPlanGroups"] = json!([]);
        }
        if profile.get("unassignedLocations").is_none() {
            profile["unassignedLocations"] = json!([]);
        }
        Some(profile)
    }

    fn stage_delivery_profile(&mut self, profile: Value) {
        let Some(id) = profile
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            return;
        };
        self.store.staged.delivery_profiles.insert(id, profile);
    }

    fn delivery_profile_for_read(&self, profile_id: &str) -> Option<Value> {
        if self
            .store
            .staged
            .delivery_profiles
            .is_tombstoned(profile_id)
        {
            return None;
        }
        self.store
            .staged
            .delivery_profiles
            .get(profile_id)
            .cloned()
            .or_else(|| self.store.base.delivery_profiles.get(profile_id).cloned())
    }

    fn effective_delivery_profiles(&self) -> Vec<Value> {
        let mut profiles = Vec::new();
        let mut seen = BTreeSet::new();
        for id in &self.store.base.delivery_profiles.order {
            if self.store.staged.delivery_profiles.is_tombstoned(id) {
                continue;
            }
            if let Some(profile) = self
                .store
                .staged
                .delivery_profiles
                .get(id)
                .or_else(|| self.store.base.delivery_profiles.get(id))
            {
                profiles.push(profile.clone());
                seen.insert(id.clone());
            }
        }
        for id in &self.store.staged.delivery_profiles.order {
            if seen.contains(id) || self.store.staged.delivery_profiles.is_tombstoned(id) {
                continue;
            }
            if let Some(profile) = self.store.staged.delivery_profiles.get(id) {
                profiles.push(profile.clone());
            }
        }
        profiles
    }

    fn delivery_profile_location_record(&self, id: &str) -> Value {
        self.location_for_read(id).unwrap_or_else(|| {
            json!({
                "id": id
            })
        })
    }

    fn hydrate_delivery_profile_locations(&mut self, location_ids: &[String], request: &Request) {
        if self.config.read_mode == ReadMode::Snapshot {
            return;
        }

        let mut missing_location_ids = Vec::new();
        for location_id in location_ids {
            if self.location_for_read(location_id).is_some() {
                continue;
            }
            missing_location_ids.push(location_id.clone());
        }
        if missing_location_ids.is_empty() {
            return;
        }

        self.hydrate_delivery_profile_location_nodes(&missing_location_ids, request);

        let mut unresolved_location_ids = Vec::new();
        for location_id in missing_location_ids {
            if self.location_for_read(&location_id).is_none() {
                unresolved_location_ids.push(location_id);
            }
        }
        if !unresolved_location_ids.is_empty() {
            self.hydrate_delivery_profile_location_catalog_fallback(
                &unresolved_location_ids,
                request,
            );
        }
    }

    fn hydrate_delivery_profile_location_nodes(
        &mut self,
        location_ids: &[String],
        request: &Request,
    ) {
        let response = self.upstream_post(
            request,
            json!({
                "query": DELIVERY_PROFILE_LOCATION_NODES_HYDRATE_QUERY,
                "variables": { "ids": location_ids }
            }),
        );
        if !(200..300).contains(&response.status) {
            return;
        }
        let Some(nodes) = response.body["data"]["nodes"].as_array() else {
            return;
        };
        for node in nodes {
            if node.get("__typename").and_then(Value::as_str) != Some("Location") {
                continue;
            }
            self.stage_observed_shipping_location(node.clone());
        }
    }

    fn hydrate_delivery_profile_location_catalog_fallback(
        &mut self,
        location_ids: &[String],
        request: &Request,
    ) {
        for first in delivery_profile_location_catalog_fallback_first_values(location_ids.len()) {
            if location_ids
                .iter()
                .all(|location_id| self.location_for_read(location_id).is_some())
            {
                return;
            }
            let response = self.upstream_post(
                request,
                json!({
                    "query": delivery_profile_locations_hydrate_query(first),
                    "variables": {}
                }),
            );
            if !(200..300).contains(&response.status) {
                continue;
            }
            self.observe_delivery_profile_locations_response(&response);
        }
    }

    fn delivery_profile_location_exists(&self, id: &str) -> bool {
        !id.is_empty() && self.location_for_read(id).is_some()
    }

    fn delivery_profiles_connection_json(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
        selections: &[SelectedField],
    ) -> Value {
        let mut profiles = self.effective_delivery_profiles();
        if resolved_bool_field(arguments, "reverse").unwrap_or(false) {
            profiles.reverse();
        }
        let (profiles, page_info) = connection_window(&profiles, arguments, value_id_cursor);
        selected_json(
            &connection_json_with_cursor(
                profiles,
                |_, profile| value_id_cursor(profile),
                page_info,
            ),
            selections,
        )
    }

    pub(in crate::proxy) fn delivery_profile_locations_read_response(
        &mut self,
        request: &Request,
        fields: &[RootFieldSelection],
    ) -> Response {
        if self.config.read_mode != ReadMode::Snapshot
            && self.store.staged.observed_shipping_locations.is_empty()
            && self.store.staged.locations.is_empty()
        {
            let response = (self.upstream_transport)(request.clone());
            self.observe_delivery_profile_locations_response(&response);
            return response;
        }
        ok_json(json!({
            "data": self.delivery_profile_locations_read_data(fields)
        }))
    }

    pub(in crate::proxy) fn delivery_profile_locations_read_data(
        &self,
        fields: &[RootFieldSelection],
    ) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            if field.name != "locationsAvailableForDeliveryProfilesConnection" {
                continue;
            }
            data.insert(
                field.response_key.clone(),
                self.delivery_profile_locations_connection_json(&field.arguments, &field.selection),
            );
        }
        Value::Object(data)
    }

    fn delivery_profile_locations_connection_json(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
        selections: &[SelectedField],
    ) -> Value {
        location_connection_json(self.effective_shipping_locations(), arguments, selections)
    }

    fn effective_shipping_locations(&self) -> Vec<Value> {
        let mut locations = Vec::new();
        let mut seen = BTreeSet::new();
        for id in &self.store.staged.observed_shipping_location_order {
            if let Some(location) = self.location_for_read(id) {
                seen.insert(id.clone());
                locations.push(location);
            }
        }
        for id in &self.store.staged.locations.order {
            if seen.contains(id) {
                continue;
            }
            if let Some(location) = self.store.staged.locations.get(id).cloned() {
                seen.insert(id.clone());
                locations.push(location);
            }
        }
        locations
    }

    pub(in crate::proxy) fn observe_delivery_profile_locations_response(
        &mut self,
        response: &Response,
    ) {
        let Some(nodes) = response.body["data"]["locationsAvailableForDeliveryProfilesConnection"]
            ["nodes"]
            .as_array()
        else {
            return;
        };
        for node in nodes {
            self.stage_observed_shipping_location(node.clone());
        }
    }

    pub(in crate::proxy) fn stage_observed_shipping_location(&mut self, mut location: Value) {
        let Some(id) = location
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            return;
        };
        location["isActive"] = location
            .get("isActive")
            .cloned()
            .unwrap_or(Value::Bool(true));
        location["isFulfillmentService"] = location
            .get("isFulfillmentService")
            .cloned()
            .unwrap_or(Value::Bool(false));
        if location.get("localPickupSettings").is_none() {
            location["localPickupSettings"] = location
                .get("localPickupSettingsV2")
                .cloned()
                .unwrap_or(Value::Null);
        }
        if !self
            .store
            .staged
            .observed_shipping_locations
            .contains_key(&id)
        {
            self.store
                .staged
                .observed_shipping_location_order
                .push(id.clone());
        }
        self.store
            .staged
            .observed_shipping_locations
            .insert(id, location);
    }
}

fn delivery_profile_locations_hydrate_query(first: usize) -> String {
    format!(
        "query ShippingDeliveryProfileLocationsHydrate {{\n    locationsAvailableForDeliveryProfilesConnection(first: {first}) {{\n      nodes {{\n        id\n        name\n        isActive\n        isFulfillmentService\n      }}\n    }}\n  }}"
    )
}

fn delivery_profile_location_catalog_fallback_first_values(requested_count: usize) -> Vec<usize> {
    let mut first_values = Vec::new();
    if (1..=3).contains(&requested_count) {
        first_values.push(requested_count);
    }
    for first in DELIVERY_PROFILE_LOCATION_CATALOG_FALLBACK_FIRST_VALUES {
        if !first_values.contains(first) {
            first_values.push(*first);
        }
    }
    first_values
}

fn collect_delivery_profile_response_values(value: &Value, profiles: &mut Vec<Value>) {
    if value
        .get("id")
        .and_then(Value::as_str)
        .is_some_and(|id| id.starts_with(DELIVERY_PROFILE_GID_PREFIX))
    {
        profiles.push(value.clone());
        return;
    }

    if let Some(nodes) = value.get("nodes").and_then(Value::as_array) {
        for node in nodes {
            collect_delivery_profile_response_values(node, profiles);
        }
    }
    if let Some(edges) = value.get("edges").and_then(Value::as_array) {
        for edge in edges {
            if let Some(node) = edge.get("node") {
                collect_delivery_profile_response_values(node, profiles);
            }
        }
    }
    if value.get("nodes").is_some() || value.get("edges").is_some() {
        return;
    }

    if let Some(object) = value.as_object() {
        for child in object.values() {
            collect_delivery_profile_response_values(child, profiles);
        }
    } else if let Some(items) = value.as_array() {
        for item in items {
            collect_delivery_profile_response_values(item, profiles);
        }
    }
}

fn delivery_profile_location_ids_from_input(
    input: &BTreeMap<String, ResolvedValue>,
) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut location_ids = Vec::new();
    for group in resolved_object_list_field(input, "locationGroupsToCreate") {
        collect_unique_delivery_profile_location_ids(
            list_string_field(&group, "locations"),
            &mut seen,
            &mut location_ids,
        );
    }
    for group in resolved_object_list_field(input, "locationGroupsToUpdate") {
        collect_unique_delivery_profile_location_ids(
            list_string_field(&group, "locationsToAdd"),
            &mut seen,
            &mut location_ids,
        );
    }
    location_ids
}

fn collect_unique_delivery_profile_location_ids(
    ids: Vec<String>,
    seen: &mut BTreeSet<String>,
    location_ids: &mut Vec<String>,
) {
    for id in ids {
        if id.is_empty() || !seen.insert(id.clone()) {
            continue;
        }
        location_ids.push(id);
    }
}

fn normalized_delivery_profile_read_model(mut profile: Value) -> Option<Value> {
    profile
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| id.starts_with(DELIVERY_PROFILE_GID_PREFIX))?;
    ensure_delivery_profile_collection_defaults(&mut profile);
    Some(profile)
}

fn ensure_delivery_profile_collection_defaults(profile: &mut Value) {
    if profile.get("profileLocationGroups").is_none() {
        profile["profileLocationGroups"] = json!([]);
    }
    if profile.get("profileItems").is_none() {
        profile["profileItems"] = json!([]);
    }
    if profile.get("sellingPlanGroups").is_none() {
        profile["sellingPlanGroups"] = json!([]);
    }
    if profile.get("unassignedLocations").is_none() {
        profile["unassignedLocations"] = json!([]);
    }
}

fn delivery_profile_remove_default_payload(selections: &[SelectedField]) -> (Value, Vec<String>) {
    (
        delivery_profile_remove_payload_json(
            Value::Null,
            selections,
            vec![user_error_omit_code(
                Value::Null,
                DELIVERY_PROFILE_DEFAULT_REMOVE_MESSAGE,
                None,
            )],
        ),
        Vec::new(),
    )
}

fn delivery_profile_resource_not_found_error(field: &RootFieldSelection, id: &str) -> Value {
    json!({
        "message": format!("Invalid id: {id}"),
        "locations": graphql_locations(field.location),
        "extensions": { "code": "RESOURCE_NOT_FOUND" },
        "path": [field.response_key.clone()]
    })
}

fn delivery_method_description_from_input(input: &BTreeMap<String, ResolvedValue>) -> Value {
    resolved_string_field(input, "description")
        .map(Value::String)
        .unwrap_or(Value::Null)
}

fn refresh_delivery_location_group_countries(group: &mut Value) {
    let mut seen = BTreeSet::new();
    let mut countries_in_any_zone = Vec::new();
    for zone in group["locationGroupZones"].as_array().into_iter().flatten() {
        let zone_name = zone["zone"]["name"].as_str().unwrap_or_default();
        for country in zone["zone"]["countries"].as_array().into_iter().flatten() {
            let key = delivery_country_union_key(country);
            if key.is_empty() || !seen.insert(key) {
                continue;
            }
            countries_in_any_zone.push(json!({
                "zone": zone_name,
                "country": country
            }));
        }
    }
    group["countriesInAnyZone"] = Value::Array(countries_in_any_zone);
}

fn delivery_country_union_key(country: &Value) -> String {
    if country["code"]["restOfWorld"].as_bool() == Some(true) {
        return "REST_OF_WORLD".to_string();
    }
    country["code"]["countryCode"]
        .as_str()
        .or_else(|| country.get("id").and_then(Value::as_str))
        .unwrap_or_default()
        .to_string()
}

use super::*;

const DELIVERY_PROFILE_VARIANTS_HYDRATE_QUERY: &str = "query ShippingDeliveryProfileVariantsHydrate($ids: [ID!]!) { nodes(ids: $ids) { ... on ProductVariant { id title product { id title handle } } } }";
// Must byte-match the recorded `ShippingDeliveryProfileHydrate` upstream call in
// the same captures. Issued when removing a profile the proxy has not staged
// locally, to learn whether the target is the shop's default profile (which
// cannot be deleted) from real store state rather than guessing.
const DELIVERY_PROFILE_DEFAULT_HYDRATE_QUERY: &str =
    "query ShippingDeliveryProfileHydrate($id: ID!) { deliveryProfile(id: $id) { id name default version } }";
const DELIVERY_PROFILE_UPDATE_HYDRATE_QUERY: &str = "query ShippingDeliveryProfileUpdateHydrate($id: ID!) { deliveryProfile(id: $id) { id name default version } }";
const DELIVERY_PROFILE_DEFAULT_REMOVE_MESSAGE: &str = "Cannot delete the default profile.";

impl DraftProxy {
    pub(in crate::proxy) fn delivery_profile_read_response(
        &self,
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
            return (self.upstream_transport)(request.clone());
        }
        ok_json(json!({ "data": self.delivery_profile_read_data(fields) }))
    }

    fn delivery_profile_read_needs_upstream(&self, fields: &[RootFieldSelection]) -> bool {
        fields.iter().any(|field| match field.name.as_str() {
            "deliveryProfile" => {
                let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                !self.store.staged.delivery_profiles.contains_key(&id)
                    && !self.store.staged.delivery_profiles.is_tombstoned(&id)
            }
            "deliveryProfiles" => !self
                .store
                .staged
                .delivery_profiles
                .order
                .iter()
                .any(|id| !self.store.staged.delivery_profiles.is_tombstoned(id)),
            _ => false,
        })
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
        let data = root_payload_json(&fields, |field| {
            let (payload, ids) = match field.name.as_str() {
                "deliveryProfileCreate" => self.delivery_profile_create_payload(field, request),
                "deliveryProfileUpdate" => self.delivery_profile_update_payload(field, request),
                "deliveryProfileRemove" => self.delivery_profile_remove_payload(field, request),
                _ => return None,
            };
            if !ids.is_empty() {
                self.record_mutation_log_entry(request, query, variables, &field.name, ids);
            }
            Some(payload)
        });
        if data.as_object().is_none_or(serde_json::Map::is_empty) {
            json_error(
                501,
                &format!("Unsupported delivery profile mutation {root_field}"),
            )
        } else {
            ok_json(json!({ "data": data }))
        }
    }

    fn delivery_profile_create_payload(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
    ) -> (Value, Vec<String>) {
        let profile_input = resolved_object_field(&field.arguments, "profile").unwrap_or_default();
        let user_errors = delivery_profile_create_user_errors(&profile_input);
        if !user_errors.is_empty() {
            return (
                delivery_profile_payload_json(Value::Null, &field.selection, user_errors),
                Vec::new(),
            );
        }

        let id = self.next_proxy_synthetic_gid("DeliveryProfile");
        let mut profile = self.delivery_profile_from_input(&id, &profile_input);
        self.delivery_profile_apply_associations(&mut profile, &profile_input, true, request);
        self.stage_delivery_profile(profile.clone());
        (
            delivery_profile_payload_json(profile, &field.selection, Vec::new()),
            vec![id],
        )
    }

    fn delivery_profile_update_payload(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
    ) -> (Value, Vec<String>) {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let Some(mut profile) = self
            .delivery_profile_for_read(&id)
            .or_else(|| self.delivery_profile_hydrate_for_update(&id, request))
        else {
            return (
                delivery_profile_payload_json(
                    Value::Null,
                    &field.selection,
                    vec![user_error_omit_code(
                        Value::Null,
                        "Profile could not be updated.",
                        None,
                    )],
                ),
                Vec::new(),
            );
        };

        let profile_input = resolved_object_field(&field.arguments, "profile").unwrap_or_default();
        let user_errors = delivery_profile_update_user_errors(&profile_input);
        if !user_errors.is_empty() {
            return (
                delivery_profile_payload_json(Value::Null, &field.selection, user_errors),
                Vec::new(),
            );
        }

        if profile["default"].as_bool() != Some(true) {
            if let Some(name) = resolved_string_field(&profile_input, "name") {
                profile["name"] = json!(name);
            }
        }
        let version = profile["version"].as_i64().unwrap_or(1) + 1;
        profile["version"] = json!(version);
        self.delivery_profile_apply_update_input(&mut profile, &profile_input);
        self.delivery_profile_apply_associations(&mut profile, &profile_input, false, request);
        self.stage_delivery_profile(profile.clone());
        (
            delivery_profile_payload_json(profile, &field.selection, Vec::new()),
            vec![id],
        )
    }

    fn delivery_profile_remove_payload(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
    ) -> (Value, Vec<String>) {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let profile = self.delivery_profile_for_read(&id);
        if profile
            .as_ref()
            .and_then(|profile| profile.get("default"))
            .and_then(Value::as_bool)
            == Some(true)
        {
            return delivery_profile_remove_default_payload(&field.selection);
        }
        if profile.is_none() {
            if self.delivery_profile_hydrates_as_default(&id, request) {
                return delivery_profile_remove_default_payload(&field.selection);
            }
            return (
                delivery_profile_remove_payload_json(
                    Value::Null,
                    &field.selection,
                    vec![user_error_omit_code(
                        Value::Null,
                        "The Delivery Profile cannot be found for the shop.",
                        None,
                    )],
                ),
                Vec::new(),
            );
        }

        self.store.staged.delivery_profiles.remove(&id);
        self.store.staged.delivery_profiles.tombstone(id.clone());
        let job = json!({
            "id": self.next_proxy_synthetic_gid("Job"),
            "done": false
        });
        (
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
        json!({
            "locationGroup": {
                "id": self.next_proxy_synthetic_gid("DeliveryLocationGroup"),
                "locations": locations,
                "locationsCount": count_object(locations.len())
            },
            "locationGroupZones": zones,
            "countriesInAnyZone": []
        })
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
            "description": null,
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
        let delete_ids = list_string_field(input, "conditionsToDelete")
            .into_iter()
            .collect::<BTreeSet<_>>();
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

        for group_input in resolved_object_list_field(input, "locationGroupsToCreate") {
            let group = self.delivery_location_group_from_input(&group_input);
            if let Some(groups) = profile["profileLocationGroups"].as_array_mut() {
                groups.push(group);
            }
        }
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
            if let Some(locations) = group["locationGroup"]["locations"].as_array_mut() {
                for location_id in list_string_field(&group_update, "locationsToAdd") {
                    if !locations.iter().any(|location| {
                        location.get("id").and_then(Value::as_str) == Some(location_id.as_str())
                    }) {
                        locations.push(self.delivery_profile_location_record(&location_id));
                    }
                }
                let count = locations.len();
                group["locationGroup"]["locationsCount"] = count_object(count);
            }
            for zone_update in resolved_object_list_field(&group_update, "zonesToUpdate") {
                let zone_id = resolved_string_field(&zone_update, "id").unwrap_or_default();
                let Some(zone) = group["locationGroupZones"]
                    .as_array_mut()
                    .into_iter()
                    .flatten()
                    .find(|zone| zone["zone"]["id"].as_str() == Some(zone_id.as_str()))
                else {
                    continue;
                };
                if let Some(name) = resolved_string_field(&zone_update, "name") {
                    zone["zone"]["name"] = json!(name);
                }
                for method_update in
                    resolved_object_list_field(&zone_update, "methodDefinitionsToUpdate")
                {
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
                    if method_update.contains_key("rateDefinition") {
                        method["rateProvider"]["price"] =
                            delivery_price_from_method_input(&method_update);
                    }
                }
                let mut new_methods =
                    resolved_object_list_field(&zone_update, "methodDefinitionsToCreate")
                        .into_iter()
                        .map(|method_input| {
                            self.delivery_method_definition_from_input(&method_input)
                        })
                        .collect::<Vec<_>>();
                if let Some(methods) = zone["methodDefinitions"].as_array_mut() {
                    methods.append(&mut new_methods);
                }
            }
        }
        refresh_delivery_profile_counts(profile);
    }

    fn delivery_profile_apply_associations(
        &mut self,
        profile: &mut Value,
        input: &BTreeMap<String, ResolvedValue>,
        create: bool,
        request: &Request,
    ) {
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
            if !variant_ids.contains(&variant_id) {
                variant_ids.push(variant_id);
            }
        }
        if !create {
            let removals = list_string_field(input, "variantsToDissociate")
                .into_iter()
                .collect::<BTreeSet<_>>();
            variant_ids.retain(|variant_id| !removals.contains(variant_id));
        }
        let hydrated_items = self.delivery_profile_hydrated_variant_items(&variant_ids, request);
        profile["profileItems"] = Value::Array(
            variant_ids
                .into_iter()
                .map(|variant_id| {
                    hydrated_items
                        .get(&variant_id)
                        .cloned()
                        .unwrap_or_else(|| delivery_profile_item_for_variant(&variant_id, None))
                })
                .collect(),
        );
        refresh_delivery_profile_counts(profile);
    }

    fn delivery_profile_hydrated_variant_items(
        &mut self,
        variant_ids: &[String],
        request: &Request,
    ) -> BTreeMap<String, Value> {
        if variant_ids.is_empty() {
            return BTreeMap::new();
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": DELIVERY_PROFILE_VARIANTS_HYDRATE_QUERY,
                "variables": { "ids": variant_ids }
            }),
        );
        response
            .body
            .pointer("/data/nodes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|node| {
                let id = node.get("id").and_then(Value::as_str)?.to_string();
                Some((
                    id.clone(),
                    delivery_profile_item_for_variant(&id, Some(node)),
                ))
            })
            .collect()
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
        self.store.staged.delivery_profiles.get(profile_id).cloned()
    }

    fn delivery_profile_location_record(&self, id: &str) -> Value {
        self.location_for_read(id).unwrap_or_else(|| {
            json!({
                "id": id,
                "name": "Location"
            })
        })
    }

    fn delivery_profiles_connection_json(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
        selections: &[SelectedField],
    ) -> Value {
        let mut profiles = self
            .store
            .staged
            .delivery_profiles
            .order
            .iter()
            .filter(|id| !self.store.staged.delivery_profiles.is_tombstoned(id))
            .filter_map(|id| self.store.staged.delivery_profiles.get(id).cloned())
            .collect::<Vec<_>>();
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

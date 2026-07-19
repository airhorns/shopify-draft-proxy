use super::*;

const DELIVERY_PROFILE_VARIANTS_HYDRATE_QUERY: &str = "query ShippingDeliveryProfileVariantsHydrate($ids: [ID!]!) { nodes(ids: $ids) { ... on ProductVariant { id title product { id title handle } } } }";
const DELIVERY_PROFILE_SELLING_PLAN_GROUPS_HYDRATE_QUERY: &str = "query ShippingDeliveryProfileSellingPlanGroupsHydrate($ids: [ID!]!) { nodes(ids: $ids) { ... on SellingPlanGroup { id name } } }";
const DELIVERY_PROFILE_LOCATION_NODES_HYDRATE_QUERY: &str = "query ShippingDeliveryProfileLocationNodesHydrate($ids: [ID!]!) { nodes(ids: $ids) { __typename ... on Location { id name isActive isFulfillmentService } } }";
// Must byte-match the recorded `ShippingDeliveryProfileHydrate` upstream call in
// the same captures. Issued when removing a profile the proxy has not staged
// locally, to learn whether the target is the shop's default profile (which
// cannot be deleted) from real store state rather than guessing.
const DELIVERY_PROFILE_DEFAULT_HYDRATE_QUERY: &str =
    "query ShippingDeliveryProfileHydrate($id: ID!) { deliveryProfile(id: $id) { id name default version } }";
const DELIVERY_PROFILE_UPDATE_HYDRATE_QUERY: &str = r#"query ShippingDeliveryProfileUpdateHydrate($id: ID!) {
  deliveryProfile(id: $id) {
    id name default version activeMethodDefinitionsCount locationsWithoutRatesCount originLocationCount zoneCountryCount
    productVariantsCount { count precision }
    profileItems(first: 250) {
      nodes { id product { id title } variants(first: 250) { nodes { id title } pageInfo { hasNextPage endCursor } } }
      pageInfo { hasNextPage endCursor }
    }
    profileLocationGroups {
      countriesInAnyZone { zone country { id name translatedName code { countryCode restOfWorld } provinces { id name code } } }
      locationGroup {
        id locations(first: 250) { nodes { id name } pageInfo { hasNextPage endCursor } } locationsCount { count precision }
      }
      locationGroupZones(first: 250) {
        nodes {
          zone { id name countries { id name translatedName code { countryCode restOfWorld } provinces { id name code } } }
          methodDefinitions(first: 250) {
            nodes {
              id name active description
              rateProvider {
                ... on DeliveryRateDefinition { id price { amount currencyCode } }
                ... on DeliveryParticipant { id fixedFee { amount currencyCode } percentageOfRateFee }
              }
              methodConditions {
                id field operator conditionCriteria {
                  __typename ... on MoneyV2 { amount currencyCode } ... on Weight { unit value }
                }
              }
            }
            pageInfo { hasNextPage endCursor }
          }
        }
        pageInfo { hasNextPage endCursor }
      }
    }
    sellingPlanGroups(first: 250) { nodes { id name } pageInfo { hasNextPage endCursor } }
    unassignedLocationsPaginated(first: 250) { nodes { id name } pageInfo { hasNextPage endCursor } }
  }
}"#;
const DELIVERY_PROFILE_ITEMS_PAGE_QUERY: &str = r#"query ShippingDeliveryProfileItemsPage($id: ID!, $after: String!) {
  deliveryProfile(id: $id) {
    profileItems(first: 250, after: $after) {
      nodes { id product { id title } variants(first: 250) { nodes { id title } pageInfo { hasNextPage endCursor } } }
      pageInfo { hasNextPage endCursor }
    }
  }
}"#;
const DELIVERY_PROFILE_ITEM_VARIANTS_PAGE_QUERY: &str = r#"query ShippingDeliveryProfileItemVariantsPage($id: ID!, $after: String!) {
  node(id: $id) { ... on DeliveryProfileItem { id variants(first: 250, after: $after) { nodes { id title } pageInfo { hasNextPage endCursor } } } }
}"#;
const DELIVERY_PROFILE_LOCATIONS_PAGE_QUERY: &str = r#"query ShippingDeliveryProfileLocationsPage($id: ID!, $after: String!) {
  node(id: $id) { ... on DeliveryLocationGroup { id locations(first: 250, after: $after) { nodes { id name } pageInfo { hasNextPage endCursor } } } }
}"#;
const DELIVERY_PROFILE_ZONES_PAGE_QUERY: &str = r#"query ShippingDeliveryProfileZonesPage($id: ID!, $locationGroupId: ID!, $after: String!) {
  deliveryProfile(id: $id) {
    profileLocationGroups(locationGroupId: $locationGroupId) {
      locationGroup { id }
      locationGroupZones(first: 250, after: $after) {
        nodes {
          zone { id name countries { id name translatedName code { countryCode restOfWorld } provinces { id name code } } }
          methodDefinitions(first: 250) {
            nodes {
              id name active description
              rateProvider {
                ... on DeliveryRateDefinition { id price { amount currencyCode } }
                ... on DeliveryParticipant { id fixedFee { amount currencyCode } percentageOfRateFee }
              }
              methodConditions {
                id field operator conditionCriteria {
                  __typename ... on MoneyV2 { amount currencyCode } ... on Weight { unit value }
                }
              }
            }
            pageInfo { hasNextPage endCursor }
          }
        }
        pageInfo { hasNextPage endCursor }
      }
    }
  }
}"#;
const DELIVERY_PROFILE_METHODS_PAGE_QUERY: &str = r#"query ShippingDeliveryProfileMethodsPage($id: ID!, $locationGroupId: ID!, $after: String!) {
  deliveryProfile(id: $id) {
    profileLocationGroups(locationGroupId: $locationGroupId) {
      locationGroup { id }
      locationGroupZones(first: 250) {
        nodes {
          zone { id }
          methodDefinitions(first: 250, after: $after) {
            nodes {
              id name active description
              rateProvider {
                ... on DeliveryRateDefinition { id price { amount currencyCode } }
                ... on DeliveryParticipant { id fixedFee { amount currencyCode } percentageOfRateFee }
              }
              methodConditions {
                id field operator conditionCriteria {
                  __typename ... on MoneyV2 { amount currencyCode } ... on Weight { unit value }
                }
              }
            }
            pageInfo { hasNextPage endCursor }
          }
        }
      }
    }
  }
}"#;
const DELIVERY_PROFILE_SELLING_PLAN_GROUPS_PAGE_QUERY: &str = r#"query ShippingDeliveryProfileSellingPlanGroupsPage($id: ID!, $after: String!) {
  deliveryProfile(id: $id) { sellingPlanGroups(first: 250, after: $after) { nodes { id name } pageInfo { hasNextPage endCursor } } }
}"#;
const DELIVERY_PROFILE_UNASSIGNED_LOCATIONS_PAGE_QUERY: &str = r#"query ShippingDeliveryProfileUnassignedLocationsPage($id: ID!, $after: String!) {
  deliveryProfile(id: $id) { unassignedLocationsPaginated(first: 250, after: $after) { nodes { id name } pageInfo { hasNextPage endCursor } } }
}"#;
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

    fn resource_not_found(location: SourceLocation, response_key: &str, id: &str) -> Self {
        Self {
            payload: Value::Null,
            staged_ids: Vec::new(),
            errors: vec![delivery_profile_resource_not_found_error(
                location,
                response_key,
                id,
            )],
        }
    }
}

enum DeliveryProfileAssociationError {
    ResourceNotFound(String),
}

impl DraftProxy {
    pub(crate) fn delivery_profile_locations_query_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let mut arguments = resolved_arguments_from_json(&invocation.arguments);
        if self.config.read_mode != ReadMode::Snapshot {
            if self.store.staged.locations.is_empty() {
                let result = self.cached_or_forward_upstream_graphql_result(
                    invocation.request,
                    invocation.response_key,
                );
                if result.transport_succeeded {
                    self.observe_delivery_profile_locations_data(&result.data);
                }
                return result.outcome;
            }
            if self.store.staged.observed_shipping_locations.is_empty() {
                self.hydrate_delivery_profile_locations_baseline(invocation.request);
            }
        }
        arguments
            .entry("sortKey".to_string())
            .or_insert_with(|| ResolvedValue::String("ID".to_string()));
        ResolverOutcome::value(location_connection_value(
            self.effective_shipping_locations(),
            &arguments,
        ))
    }

    pub(crate) fn delivery_profile_query_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        if self.config.read_mode == ReadMode::LiveHybrid
            && self.delivery_profile_root_needs_upstream(invocation.root_name, &arguments)
        {
            let result = self.cached_or_forward_upstream_graphql_result(
                invocation.request,
                invocation.response_key,
            );
            let observed_profiles =
                result.transport_succeeded && self.observe_delivery_profiles_data(&result.data);
            if !self.has_local_delivery_profile_overlay()
                || (!observed_profiles && self.store.base.delivery_profiles.order.is_empty())
            {
                return result.outcome;
            }
        }
        ResolverOutcome::value(self.delivery_profile_read_value(invocation.root_name, &arguments))
    }

    fn delivery_profile_root_needs_upstream(
        &self,
        root_name: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> bool {
        match root_name {
            "deliveryProfile" => {
                let id = resolved_string_field(arguments, "id").unwrap_or_default();
                !self.has_local_delivery_profile_overlay()
                    || !self.delivery_profile_is_known_locally(&id)
            }
            "deliveryProfiles" => true,
            _ => false,
        }
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

    fn observe_delivery_profiles_data(&mut self, data: &Value) -> bool {
        let mut profiles = Vec::new();
        collect_delivery_profile_response_values(data, &mut profiles);
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

    fn delivery_profile_read_value(
        &self,
        root_name: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        match root_name {
            "deliveryProfile" => {
                let id = resolved_string_field(arguments, "id").unwrap_or_default();
                self.delivery_profile_for_read(&id)
                    .map(|profile| canonical_delivery_profile_value(&profile))
                    .unwrap_or(Value::Null)
            }
            "deliveryProfiles" => self.delivery_profiles_connection_json(arguments),
            _ => Value::Null,
        }
    }

    pub(crate) fn delivery_profile_mutation_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let RootInvocation {
            root_name,
            response_key,
            root_location,
            arguments,
            request,
            ..
        } = invocation;
        let arguments = resolved_arguments_from_json(&arguments);
        let domain_outcome = match root_name {
            "deliveryProfileCreate" => self.delivery_profile_create_payload(
                &arguments,
                request,
                root_location,
                response_key,
            ),
            "deliveryProfileUpdate" => self.delivery_profile_update_payload(
                &arguments,
                request,
                root_location,
                response_key,
            ),
            "deliveryProfileRemove" => self.delivery_profile_remove_payload(&arguments, request),
            _ => {
                return resolver_http_error_outcome(
                    501,
                    format!("Unsupported delivery profile mutation {root_name}"),
                );
            }
        };
        let mut outcome = ResolverOutcome::value(domain_outcome.payload);
        if !domain_outcome.errors.is_empty() {
            outcome.errors = root_field_errors_from_json(&domain_outcome.errors, response_key);
        }
        if !domain_outcome.staged_ids.is_empty() {
            outcome = outcome.with_log_draft(LogDraft::staged(
                root_name,
                "shipping-fulfillments",
                domain_outcome.staged_ids,
            ));
        }
        outcome
    }

    fn delivery_profile_create_payload(
        &mut self,
        arguments: &BTreeMap<String, ResolvedValue>,
        request: &Request,
        root_location: SourceLocation,
        response_key: &str,
    ) -> DeliveryProfileMutationOutcome {
        let profile_input = resolved_object_field(arguments, "profile").unwrap_or_default();
        let location_ids = delivery_profile_location_ids_from_input(&profile_input);
        self.hydrate_delivery_profile_locations(&location_ids, request);
        let mut location_exists =
            |location_id: &str| self.delivery_profile_location_exists(location_id);
        let user_errors = delivery_profile_create_user_errors(&profile_input, &mut location_exists);
        if !user_errors.is_empty() {
            return DeliveryProfileMutationOutcome::unstaged(delivery_profile_payload_json(
                Value::Null,
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
                    DeliveryProfileMutationOutcome::resource_not_found(
                        root_location,
                        response_key,
                        &id,
                    )
                }
            };
        }
        if let Err(error) = self.delivery_profile_apply_selling_plan_associations(
            &mut profile,
            &profile_input,
            true,
            request,
        ) {
            return match error {
                DeliveryProfileAssociationError::ResourceNotFound(id) => {
                    DeliveryProfileMutationOutcome::resource_not_found(
                        root_location,
                        response_key,
                        &id,
                    )
                }
            };
        }
        self.stage_delivery_profile(profile.clone());
        DeliveryProfileMutationOutcome::staged(
            delivery_profile_payload_json(profile, Vec::new()),
            vec![id],
        )
    }

    fn delivery_profile_update_payload(
        &mut self,
        arguments: &BTreeMap<String, ResolvedValue>,
        request: &Request,
        root_location: SourceLocation,
        response_key: &str,
    ) -> DeliveryProfileMutationOutcome {
        let id = resolved_string_field(arguments, "id").unwrap_or_default();
        let Some(mut profile) = self.delivery_profile_for_update(&id, request) else {
            return DeliveryProfileMutationOutcome::unstaged(delivery_profile_payload_json(
                Value::Null,
                vec![user_error_omit_code(
                    Value::Null,
                    "Profile could not be updated.",
                    None,
                )],
            ));
        };

        let profile_input = resolved_object_field(arguments, "profile").unwrap_or_default();
        let location_ids = delivery_profile_location_ids_from_input(&profile_input);
        self.hydrate_delivery_profile_locations(&location_ids, request);
        let mut location_exists =
            |location_id: &str| self.delivery_profile_location_exists(location_id);
        let user_errors = delivery_profile_update_user_errors(&profile_input, &mut location_exists);
        if !user_errors.is_empty() {
            return DeliveryProfileMutationOutcome::unstaged(delivery_profile_payload_json(
                Value::Null,
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
        let changes_variant_associations = profile_input.contains_key("variantsToAssociate")
            || profile_input.contains_key("variantsToDissociate");
        if changes_variant_associations {
            if let Err(error) = self.delivery_profile_apply_associations(
                &mut profile,
                &profile_input,
                false,
                request,
            ) {
                return match error {
                    DeliveryProfileAssociationError::ResourceNotFound(id) => {
                        DeliveryProfileMutationOutcome::resource_not_found(
                            root_location,
                            response_key,
                            &id,
                        )
                    }
                };
            }
        }
        let changes_selling_plan_associations = profile_input
            .contains_key("sellingPlanGroupsToAssociate")
            || profile_input.contains_key("sellingPlanGroupsToDissociate");
        if changes_selling_plan_associations {
            if let Err(error) = self.delivery_profile_apply_selling_plan_associations(
                &mut profile,
                &profile_input,
                false,
                request,
            ) {
                return match error {
                    DeliveryProfileAssociationError::ResourceNotFound(id) => {
                        DeliveryProfileMutationOutcome::resource_not_found(
                            root_location,
                            response_key,
                            &id,
                        )
                    }
                };
            }
        }
        self.stage_delivery_profile(profile.clone());
        DeliveryProfileMutationOutcome::staged(
            delivery_profile_payload_json(profile, Vec::new()),
            vec![id],
        )
    }

    fn delivery_profile_remove_payload(
        &mut self,
        arguments: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> DeliveryProfileMutationOutcome {
        let id = resolved_string_field(arguments, "id").unwrap_or_default();
        let profile = self.delivery_profile_for_read(&id);
        if profile
            .as_ref()
            .and_then(|profile| profile.get("default"))
            .and_then(Value::as_bool)
            == Some(true)
        {
            let (payload, ids) = delivery_profile_remove_default_payload();
            return DeliveryProfileMutationOutcome::staged(payload, ids);
        }
        if profile.is_none() {
            if self.delivery_profile_hydrates_as_default(&id, request) {
                let (payload, ids) = delivery_profile_remove_default_payload();
                return DeliveryProfileMutationOutcome::staged(payload, ids);
            }
            return DeliveryProfileMutationOutcome::unstaged(delivery_profile_remove_payload_json(
                Value::Null,
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
            delivery_profile_remove_payload_json(job, Vec::new()),
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
        let changes_location_graph = [
            "conditionsToDelete",
            "methodDefinitionsToDelete",
            "zonesToDelete",
            "locationGroupsToDelete",
            "locationGroupsToCreate",
            "locationGroupsToUpdate",
        ]
        .iter()
        .any(|field| input.contains_key(*field));
        let product_variants_count = profile.get("productVariantsCount").cloned();
        self.delivery_profile_delete_conditions(profile, input);
        self.delivery_profile_delete_method_definitions(profile, input);
        self.delivery_profile_delete_zones(profile, input);
        self.delivery_profile_delete_location_groups(profile, input);
        self.delivery_profile_create_location_groups(profile, input);
        self.delivery_profile_update_location_groups(profile, input);
        if changes_location_graph {
            refresh_delivery_profile_counts(profile);
            if let Some(product_variants_count) = product_variants_count {
                profile["productVariantsCount"] = product_variants_count;
            }
        }
    }

    fn delivery_profile_delete_location_groups(
        &self,
        profile: &mut Value,
        input: &BTreeMap<String, ResolvedValue>,
    ) {
        let delete_ids = list_string_field(input, "locationGroupsToDelete")
            .into_iter()
            .collect::<BTreeSet<_>>();
        if let Some(groups) = profile["profileLocationGroups"].as_array_mut() {
            groups.retain(|group| {
                group["locationGroup"]["id"]
                    .as_str()
                    .is_none_or(|id| !delete_ids.contains(id))
            });
        }
    }

    fn delivery_profile_delete_zones(
        &self,
        profile: &mut Value,
        input: &BTreeMap<String, ResolvedValue>,
    ) {
        let delete_ids = list_string_field(input, "zonesToDelete")
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
            if let Some(zones) = group["locationGroupZones"].as_array_mut() {
                zones.retain(|zone| {
                    zone["zone"]["id"]
                        .as_str()
                        .is_none_or(|id| !delete_ids.contains(id))
                });
            }
            refresh_delivery_location_group_countries(group);
        }
    }

    fn delivery_profile_delete_method_definitions(
        &self,
        profile: &mut Value,
        input: &BTreeMap<String, ResolvedValue>,
    ) {
        let delete_ids = list_string_field(input, "methodDefinitionsToDelete")
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
                if let Some(methods) = zone["methodDefinitions"].as_array_mut() {
                    methods.retain(|method| {
                        method["id"]
                            .as_str()
                            .is_none_or(|id| !delete_ids.contains(id))
                    });
                }
            }
        }
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
            self.delivery_profile_update_group_locations(group, &group_update);
            self.delivery_profile_update_zones(group, &group_update);
            refresh_delivery_location_group_countries(group);
        }
    }

    fn delivery_profile_update_group_locations(
        &mut self,
        group: &mut Value,
        group_update: &BTreeMap<String, ResolvedValue>,
    ) {
        let Some(locations) = group["locationGroup"]["locations"].as_array_mut() else {
            return;
        };
        let removals = list_string_field(group_update, "locationsToRemove")
            .into_iter()
            .collect::<BTreeSet<_>>();
        locations.retain(|location| {
            location["id"]
                .as_str()
                .is_none_or(|id| !removals.contains(id))
        });
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
        let mut items = profile["profileItems"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let additions = list_string_field(input, "variantsToAssociate");
        for variant_id in &additions {
            if !is_shopify_gid_of_type(variant_id, "ProductVariant") {
                return Err(DeliveryProfileAssociationError::ResourceNotFound(
                    variant_id.clone(),
                ));
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
            for item in &mut items {
                if let Some(variants) = item["variants"].as_array_mut() {
                    variants.retain(|variant| {
                        variant["id"]
                            .as_str()
                            .is_none_or(|id| !removals.contains(id))
                    });
                }
            }
            items.retain(|item| {
                item["variants"]
                    .as_array()
                    .is_some_and(|variants| !variants.is_empty())
            });
        }

        let existing_variant_ids = items
            .iter()
            .flat_map(|item| item["variants"].as_array().into_iter().flatten())
            .filter_map(|variant| variant["id"].as_str().map(str::to_string))
            .collect::<BTreeSet<_>>();
        let additions = additions
            .into_iter()
            .filter(|variant_id| !existing_variant_ids.contains(variant_id))
            .collect::<Vec<_>>();
        let mut resolved_items = BTreeMap::new();
        let unresolved_variant_ids = additions
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
        for variant_id in additions {
            let Some(mut resolved_item) = resolved_items.remove(&variant_id) else {
                continue;
            };
            let Some(product_id) = resolved_item["product"]["id"].as_str().map(str::to_string)
            else {
                continue;
            };
            let Some(variant) = resolved_item["variants"]
                .as_array_mut()
                .and_then(|variants| variants.pop())
            else {
                continue;
            };
            if let Some(existing_item) = items
                .iter_mut()
                .find(|item| item["product"]["id"].as_str() == Some(product_id.as_str()))
            {
                if let Some(variants) = existing_item["variants"].as_array_mut() {
                    variants.push(variant);
                }
            } else {
                resolved_item["id"] = json!(self.next_proxy_synthetic_gid("DeliveryProfileItem"));
                resolved_item["variants"] = Value::Array(vec![variant]);
                items.push(resolved_item);
            }
        }
        profile["profileItems"] = Value::Array(items);
        refresh_delivery_profile_counts(profile);
        Ok(())
    }

    fn delivery_profile_apply_selling_plan_associations(
        &mut self,
        profile: &mut Value,
        input: &BTreeMap<String, ResolvedValue>,
        create: bool,
        request: &Request,
    ) -> Result<(), DeliveryProfileAssociationError> {
        let mut groups = profile["sellingPlanGroups"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if !create {
            let removals = list_string_field(input, "sellingPlanGroupsToDissociate");
            for id in &removals {
                if !is_shopify_gid_of_type(id, "SellingPlanGroup") {
                    return Err(DeliveryProfileAssociationError::ResourceNotFound(
                        id.clone(),
                    ));
                }
            }
            let removals = removals.into_iter().collect::<BTreeSet<_>>();
            groups.retain(|group| group["id"].as_str().is_none_or(|id| !removals.contains(id)));
        }

        let existing_ids = groups
            .iter()
            .filter_map(|group| group["id"].as_str().map(str::to_string))
            .collect::<BTreeSet<_>>();
        let additions = list_string_field(input, "sellingPlanGroupsToAssociate")
            .into_iter()
            .filter(|id| !existing_ids.contains(id))
            .collect::<Vec<_>>();
        for id in &additions {
            if !is_shopify_gid_of_type(id, "SellingPlanGroup") {
                return Err(DeliveryProfileAssociationError::ResourceNotFound(
                    id.clone(),
                ));
            }
        }
        groups.extend(self.delivery_profile_hydrated_selling_plan_groups(&additions, request)?);
        profile["sellingPlanGroups"] = Value::Array(groups);
        Ok(())
    }

    fn delivery_profile_hydrated_selling_plan_groups(
        &self,
        ids: &[String],
        request: &Request,
    ) -> Result<Vec<Value>, DeliveryProfileAssociationError> {
        if ids.is_empty() || self.config.read_mode != ReadMode::LiveHybrid {
            return Ok(Vec::new());
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": DELIVERY_PROFILE_SELLING_PLAN_GROUPS_HYDRATE_QUERY,
                "variables": { "ids": ids }
            }),
        );
        let Some(nodes) =
            delivery_profile_upstream_value(&response, "/data/nodes").and_then(Value::as_array)
        else {
            return Err(DeliveryProfileAssociationError::ResourceNotFound(
                ids[0].clone(),
            ));
        };
        let groups = nodes
            .iter()
            .filter(|node| {
                node["id"]
                    .as_str()
                    .is_some_and(|id| is_shopify_gid_of_type(id, "SellingPlanGroup"))
            })
            .cloned()
            .collect::<Vec<_>>();
        if groups.len() != ids.len() {
            return Err(DeliveryProfileAssociationError::ResourceNotFound(
                ids.iter()
                    .find(|id| {
                        !groups
                            .iter()
                            .any(|group| group["id"].as_str() == Some(id.as_str()))
                    })
                    .cloned()
                    .unwrap_or_else(|| ids[0].clone()),
            ));
        }
        Ok(groups)
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

    fn delivery_profile_for_update(&mut self, id: &str, request: &Request) -> Option<Value> {
        if self.store.staged.delivery_profiles.is_tombstoned(id) {
            return None;
        }
        if let Some(profile) = self.store.staged.delivery_profiles.get(id) {
            return Some(profile.clone());
        }
        if self.config.read_mode == ReadMode::LiveHybrid {
            return self.delivery_profile_hydrate_for_update(id, request);
        }
        self.store.base.delivery_profiles.get(id).cloned()
    }

    fn delivery_profile_hydrate_for_update(
        &mut self,
        id: &str,
        request: &Request,
    ) -> Option<Value> {
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
        let mut profile = delivery_profile_upstream_value(&response, "/data/deliveryProfile")
            .or_else(|| delivery_profile_upstream_value(&response, "/data/node"))?
            .clone();
        if profile.get("id").and_then(Value::as_str) != Some(id) {
            return None;
        }

        self.hydrate_delivery_profile_items_pages(&mut profile, id, request)?;
        self.hydrate_delivery_profile_selling_plan_group_pages(&mut profile, id, request)?;
        self.hydrate_delivery_profile_unassigned_location_pages(&mut profile, id, request)?;
        self.hydrate_delivery_profile_location_group_pages(&mut profile, id, request)?;
        normalize_authoritative_delivery_profile(profile)
    }

    fn hydrate_delivery_profile_items_pages(
        &self,
        profile: &mut Value,
        profile_id: &str,
        request: &Request,
    ) -> Option<()> {
        let mut after = delivery_profile_next_page_cursor(profile.get("profileItems")?)?;
        while let Some(cursor) = after {
            let response = self.upstream_post(
                request,
                json!({
                    "query": DELIVERY_PROFILE_ITEMS_PAGE_QUERY,
                    "variables": { "id": profile_id, "after": cursor }
                }),
            );
            let page =
                delivery_profile_upstream_value(&response, "/data/deliveryProfile/profileItems")?
                    .clone();
            after = append_delivery_profile_connection_page(
                &mut profile["profileItems"],
                &page,
                &cursor,
            )?;
        }

        let item_count = connection_nodes(&profile["profileItems"]).len();
        for item_index in 0..item_count {
            let item = connection_nodes(&profile["profileItems"])[item_index].clone();
            let item_id = item.get("id")?.as_str()?.to_string();
            let mut variants = item.get("variants")?.clone();
            let mut after = delivery_profile_next_page_cursor(&variants)?;
            while let Some(cursor) = after {
                let response = self.upstream_post(
                    request,
                    json!({
                        "query": DELIVERY_PROFILE_ITEM_VARIANTS_PAGE_QUERY,
                        "variables": { "id": item_id, "after": cursor }
                    }),
                );
                let page =
                    delivery_profile_upstream_value(&response, "/data/node/variants")?.clone();
                after = append_delivery_profile_connection_page(&mut variants, &page, &cursor)?;
            }
            profile["profileItems"]["nodes"][item_index]["variants"] = variants;
        }
        Some(())
    }

    fn hydrate_delivery_profile_selling_plan_group_pages(
        &self,
        profile: &mut Value,
        profile_id: &str,
        request: &Request,
    ) -> Option<()> {
        let mut after = delivery_profile_next_page_cursor(profile.get("sellingPlanGroups")?)?;
        while let Some(cursor) = after {
            let response = self.upstream_post(
                request,
                json!({
                    "query": DELIVERY_PROFILE_SELLING_PLAN_GROUPS_PAGE_QUERY,
                    "variables": { "id": profile_id, "after": cursor }
                }),
            );
            let page = delivery_profile_upstream_value(
                &response,
                "/data/deliveryProfile/sellingPlanGroups",
            )?
            .clone();
            after = append_delivery_profile_connection_page(
                &mut profile["sellingPlanGroups"],
                &page,
                &cursor,
            )?;
        }
        Some(())
    }

    fn hydrate_delivery_profile_unassigned_location_pages(
        &self,
        profile: &mut Value,
        profile_id: &str,
        request: &Request,
    ) -> Option<()> {
        let mut after =
            delivery_profile_next_page_cursor(profile.get("unassignedLocationsPaginated")?)?;
        while let Some(cursor) = after {
            let response = self.upstream_post(
                request,
                json!({
                    "query": DELIVERY_PROFILE_UNASSIGNED_LOCATIONS_PAGE_QUERY,
                    "variables": { "id": profile_id, "after": cursor }
                }),
            );
            let page = delivery_profile_upstream_value(
                &response,
                "/data/deliveryProfile/unassignedLocationsPaginated",
            )?
            .clone();
            after = append_delivery_profile_connection_page(
                &mut profile["unassignedLocationsPaginated"],
                &page,
                &cursor,
            )?;
        }
        Some(())
    }

    fn hydrate_delivery_profile_location_group_pages(
        &self,
        profile: &mut Value,
        profile_id: &str,
        request: &Request,
    ) -> Option<()> {
        let group_count = profile.get("profileLocationGroups")?.as_array()?.len();
        for group_index in 0..group_count {
            let location_group_id = profile["profileLocationGroups"][group_index]["locationGroup"]
                ["id"]
                .as_str()?
                .to_string();
            self.hydrate_delivery_profile_group_location_pages(
                profile,
                group_index,
                &location_group_id,
                request,
            )?;
            self.hydrate_delivery_profile_group_zone_pages(
                profile,
                profile_id,
                group_index,
                &location_group_id,
                request,
            )?;
            self.hydrate_delivery_profile_method_pages(
                profile,
                profile_id,
                group_index,
                &location_group_id,
                request,
            )?;
        }
        Some(())
    }

    fn hydrate_delivery_profile_group_location_pages(
        &self,
        profile: &mut Value,
        group_index: usize,
        location_group_id: &str,
        request: &Request,
    ) -> Option<()> {
        let locations =
            &mut profile["profileLocationGroups"][group_index]["locationGroup"]["locations"];
        let mut after = delivery_profile_next_page_cursor(locations)?;
        while let Some(cursor) = after {
            let response = self.upstream_post(
                request,
                json!({
                    "query": DELIVERY_PROFILE_LOCATIONS_PAGE_QUERY,
                    "variables": { "id": location_group_id, "after": cursor }
                }),
            );
            let page = delivery_profile_upstream_value(&response, "/data/node/locations")?.clone();
            after = append_delivery_profile_connection_page(locations, &page, &cursor)?;
        }
        Some(())
    }

    fn hydrate_delivery_profile_group_zone_pages(
        &self,
        profile: &mut Value,
        profile_id: &str,
        group_index: usize,
        location_group_id: &str,
        request: &Request,
    ) -> Option<()> {
        let zones = &mut profile["profileLocationGroups"][group_index]["locationGroupZones"];
        let mut after = delivery_profile_next_page_cursor(zones)?;
        while let Some(cursor) = after {
            let response = self.upstream_post(
                request,
                json!({
                    "query": DELIVERY_PROFILE_ZONES_PAGE_QUERY,
                    "variables": {
                        "id": profile_id,
                        "locationGroupId": location_group_id,
                        "after": cursor
                    }
                }),
            );
            let page = delivery_profile_upstream_value(
                &response,
                "/data/deliveryProfile/profileLocationGroups/0/locationGroupZones",
            )?
            .clone();
            after = append_delivery_profile_connection_page(zones, &page, &cursor)?;
        }
        Some(())
    }

    fn hydrate_delivery_profile_method_pages(
        &self,
        profile: &mut Value,
        profile_id: &str,
        group_index: usize,
        location_group_id: &str,
        request: &Request,
    ) -> Option<()> {
        let zone_count =
            connection_nodes(&profile["profileLocationGroups"][group_index]["locationGroupZones"])
                .len();
        for zone_index in 0..zone_count {
            let zone = connection_nodes(
                &profile["profileLocationGroups"][group_index]["locationGroupZones"],
            )[zone_index]
                .clone();
            let zone_id = zone["zone"]["id"].as_str()?.to_string();
            let mut methods = zone.get("methodDefinitions")?.clone();
            let mut after = delivery_profile_next_page_cursor(&methods)?;
            while let Some(cursor) = after {
                let response = self.upstream_post(
                    request,
                    json!({
                        "query": DELIVERY_PROFILE_METHODS_PAGE_QUERY,
                        "variables": {
                            "id": profile_id,
                            "locationGroupId": location_group_id,
                            "after": cursor
                        }
                    }),
                );
                let zones = delivery_profile_upstream_value(
                    &response,
                    "/data/deliveryProfile/profileLocationGroups/0/locationGroupZones",
                )?;
                let page = connection_nodes(zones)
                    .into_iter()
                    .find(|candidate| candidate["zone"]["id"].as_str() == Some(zone_id.as_str()))?
                    ["methodDefinitions"]
                    .clone();
                after = append_delivery_profile_connection_page(&mut methods, &page, &cursor)?;
            }
            profile["profileLocationGroups"][group_index]["locationGroupZones"]["nodes"]
                [zone_index]["methodDefinitions"] = methods;
        }
        Some(())
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

    pub(in crate::proxy) fn effective_delivery_profiles(&self) -> Vec<Value> {
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

    pub(in crate::proxy) fn hydrate_delivery_profile_locations(
        &mut self,
        location_ids: &[String],
        request: &Request,
    ) {
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
    ) -> Value {
        let mut profiles = self
            .effective_delivery_profiles()
            .iter()
            .map(canonical_delivery_profile_value)
            .collect::<Vec<_>>();
        if resolved_bool_field(arguments, "reverse").unwrap_or(false) {
            profiles.reverse();
        }
        let (profiles, page_info) = connection_window(&profiles, arguments, value_id_cursor);
        connection_json_with_cursor(profiles, |_, profile| value_id_cursor(profile), page_info)
    }

    pub(in crate::proxy) fn hydrate_delivery_profile_locations_baseline(
        &mut self,
        request: &Request,
    ) {
        if self.config.read_mode == ReadMode::Snapshot
            || !self.store.staged.observed_shipping_locations.is_empty()
        {
            return;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": delivery_profile_locations_hydrate_query(250),
                "operationName": "ShippingDeliveryProfileLocationsHydrate",
                "variables": {}
            }),
        );
        if (200..300).contains(&response.status) {
            self.observe_delivery_profile_locations_response(&response);
        }
    }

    fn effective_shipping_locations(&self) -> Vec<Value> {
        let mut locations = Vec::new();
        let mut seen = BTreeSet::new();
        for id in &self.store.staged.observed_shipping_location_order {
            self.push_effective_shipping_location(id, &mut seen, &mut locations);
        }
        if self.config.read_mode == ReadMode::Snapshot {
            for id in &self.store.staged.locations.order {
                self.push_effective_shipping_location(id, &mut seen, &mut locations);
            }
        }
        locations
    }

    fn push_effective_shipping_location(
        &self,
        id: &str,
        seen: &mut BTreeSet<String>,
        locations: &mut Vec<Value>,
    ) {
        if seen.contains(id) {
            return;
        }
        let Some(location) = self.location_for_read(id) else {
            return;
        };
        seen.insert(id.to_string());
        locations.push(location);
    }

    pub(in crate::proxy) fn observe_delivery_profile_locations_response(
        &mut self,
        response: &Response,
    ) {
        if (200..300).contains(&response.status) {
            self.observe_delivery_profile_locations_data(&response.body["data"]);
        }
    }

    pub(in crate::proxy) fn observe_delivery_profile_locations_data(&mut self, data: &Value) {
        let Some(nodes) =
            data["locationsAvailableForDeliveryProfilesConnection"]["nodes"].as_array()
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

fn delivery_profile_upstream_value<'a>(response: &'a Response, pointer: &str) -> Option<&'a Value> {
    if !(200..300).contains(&response.status)
        || response
            .body
            .get("errors")
            .and_then(Value::as_array)
            .is_some_and(|errors| !errors.is_empty())
    {
        return None;
    }
    response
        .body
        .pointer(pointer)
        .filter(|value| !value.is_null())
}

fn delivery_profile_next_page_cursor(connection: &Value) -> Option<Option<String>> {
    let page_info = connection.get("pageInfo")?;
    if !page_info.get("hasNextPage")?.as_bool()? {
        return Some(None);
    }
    Some(Some(
        page_info
            .get("endCursor")?
            .as_str()
            .filter(|cursor| !cursor.is_empty())?
            .to_string(),
    ))
}

fn append_delivery_profile_connection_page(
    connection: &mut Value,
    page: &Value,
    requested_cursor: &str,
) -> Option<Option<String>> {
    let page_nodes = page.get("nodes")?.as_array()?;
    if page_nodes.is_empty() {
        return None;
    }
    connection
        .get_mut("nodes")?
        .as_array_mut()?
        .extend(page_nodes.clone());
    connection["pageInfo"] = page.get("pageInfo")?.clone();
    let next = delivery_profile_next_page_cursor(connection)?;
    if next.as_deref() == Some(requested_cursor) {
        return None;
    }
    Some(next)
}

fn normalize_authoritative_delivery_profile(mut profile: Value) -> Option<Value> {
    let items = connection_nodes(profile.get("profileItems")?);
    let mut normalized_items = Vec::with_capacity(items.len());
    for mut item in items {
        let variants = connection_nodes(item.get("variants")?);
        item["variants"] = Value::Array(variants);
        normalized_items.push(item);
    }
    profile["profileItems"] = Value::Array(normalized_items);

    let groups = profile.get("profileLocationGroups")?.as_array()?.clone();
    let mut normalized_groups = Vec::with_capacity(groups.len());
    for mut group in groups {
        let locations = connection_nodes(group.pointer("/locationGroup/locations")?);
        group["locationGroup"]["locations"] = Value::Array(locations);
        let zones = connection_nodes(group.get("locationGroupZones")?);
        let mut normalized_zones = Vec::with_capacity(zones.len());
        for mut zone in zones {
            let methods = connection_nodes(zone.get("methodDefinitions")?);
            zone["methodDefinitions"] = Value::Array(methods);
            normalized_zones.push(zone);
        }
        group["locationGroupZones"] = Value::Array(normalized_zones);
        normalized_groups.push(group);
    }
    profile["profileLocationGroups"] = Value::Array(normalized_groups);

    profile["sellingPlanGroups"] =
        Value::Array(connection_nodes(profile.get("sellingPlanGroups")?));
    profile["unassignedLocations"] = Value::Array(connection_nodes(
        profile.get("unassignedLocationsPaginated")?,
    ));
    profile
        .as_object_mut()?
        .remove("unassignedLocationsPaginated");
    Some(profile)
}

fn normalized_delivery_profile_read_model(mut profile: Value) -> Option<Value> {
    profile
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| id.starts_with(DELIVERY_PROFILE_GID_PREFIX))?;
    normalize_selected_delivery_profile_relationships(&mut profile);
    Some(profile)
}

fn normalize_selected_delivery_profile_relationships(profile: &mut Value) {
    if let Some(items) = profile.get("profileItems").cloned() {
        let normalized_items = connection_nodes(&items)
            .into_iter()
            .map(|mut item| {
                if let Some(variants) = item.get("variants").cloned() {
                    item["variants"] = Value::Array(connection_nodes(&variants));
                }
                item
            })
            .collect::<Vec<_>>();
        profile["profileItems"] = Value::Array(normalized_items);
    }
    if let Some(groups) = profile
        .get("profileLocationGroups")
        .and_then(Value::as_array)
        .cloned()
    {
        profile["profileLocationGroups"] = Value::Array(
            groups
                .into_iter()
                .map(|mut group| {
                    if let Some(locations) = group.pointer("/locationGroup/locations").cloned() {
                        group["locationGroup"]["locations"] =
                            Value::Array(connection_nodes(&locations));
                    }
                    if let Some(zones) = group.get("locationGroupZones").cloned() {
                        group["locationGroupZones"] = Value::Array(
                            connection_nodes(&zones)
                                .into_iter()
                                .map(|mut zone| {
                                    if let Some(methods) = zone.get("methodDefinitions").cloned() {
                                        zone["methodDefinitions"] =
                                            Value::Array(connection_nodes(&methods));
                                    }
                                    zone
                                })
                                .collect(),
                        );
                    }
                    group
                })
                .collect(),
        );
    }
    if let Some(selling_plan_groups) = profile.get("sellingPlanGroups").cloned() {
        profile["sellingPlanGroups"] = Value::Array(connection_nodes(&selling_plan_groups));
    }
    if let Some(unassigned_locations) = profile.get("unassignedLocationsPaginated").cloned() {
        profile["unassignedLocations"] = Value::Array(connection_nodes(&unassigned_locations));
    }
}

fn delivery_profile_remove_default_payload() -> (Value, Vec<String>) {
    (
        delivery_profile_remove_payload_json(
            Value::Null,
            vec![user_error_omit_code(
                Value::Null,
                DELIVERY_PROFILE_DEFAULT_REMOVE_MESSAGE,
                None,
            )],
        ),
        Vec::new(),
    )
}

fn delivery_profile_resource_not_found_error(
    location: SourceLocation,
    response_key: &str,
    id: &str,
) -> Value {
    json!({
        "message": format!("Invalid id: {id}"),
        "locations": graphql_locations(location),
        "extensions": { "code": "RESOURCE_NOT_FOUND" },
        "path": [response_key]
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

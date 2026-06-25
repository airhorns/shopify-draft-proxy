use super::resolved_values;
use super::*;

use crate::graphql::ParsedDocument;

mod app_billing;
mod gift_cards;

// Must byte-match the recorded upstream hydrate query in the store-properties
// publishable captures (strict cassette compares query text + variables). See
// fixtures/conformance/.../store-properties/publishable-*-shop-count-parity.json.
const PUBLISHABLE_SHOP_HYDRATE_QUERY: &str = r#"#graphql
  query StorePropertiesPublishableInputValidationHydrate($id: ID!) {
    publishable: node(id: $id) {
      ... on Product {
        id
        publishedOnCurrentPublication
        resourcePublicationsCount {
          count
          precision
        }
      }
    }
    shop {
      publicationCount
    }
    publications(first: 20) {
      nodes {
        id
        name
      }
    }
  }
"#;
// Must byte-match the recorded upstream location hydrate query in the
// store-properties lifecycle captures (strict cassette compares query text +
// variables). Issued to replay the real baseline location through the cassette
// so activate/deactivate preserve its captured name/scope/state instead of
// fabricating a synthetic record.
const LOCATION_HYDRATE_QUERY: &str = r#"query StorePropertiesLocationHydrate($id: ID!) { location(id: $id) { id legacyResourceId name activatable addressVerified createdAt deactivatable deactivatedAt deletable fulfillsOnlineOrders hasActiveInventory hasUnfulfilledOrders isActive isFulfillmentService isPrimary shipsInventory updatedAt fulfillmentService { id handle serviceName } address { address1 address2 city country countryCode formatted latitude longitude phone province provinceCode zip } suggestedAddresses { address1 countryCode formatted } metafield(namespace: "custom", key: "hours") { id namespace key value type } metafields(first: 3) { nodes { id namespace key value type } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } inventoryLevels(first: 3) { nodes { id item { id } location { id name } quantities(names: ["available", "committed", "on_hand"]) { name quantity updatedAt } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } }"#;
// Must byte-match the recorded `ShippingDeliveryProfileVariantsHydrate` upstream
// call in the delivery-profile lifecycle captures (strict cassette compares
// query text + variables). Issued so a created/updated profile's
// `variantsToAssociate` resolve to the real product/variant the merchant
// associated — replayed through the cassette instead of fabricating a synthetic
// product id.
const DELIVERY_PROFILE_VARIANTS_HYDRATE_QUERY: &str = "query ShippingDeliveryProfileVariantsHydrate($ids: [ID!]!) { nodes(ids: $ids) { ... on ProductVariant { id title product { id title handle } } } }";
// Must byte-match the recorded `ShippingDeliveryProfileHydrate` upstream call in
// the same captures. Issued when removing a profile the proxy has not staged
// locally, to learn whether the target is the shop's default profile (which
// cannot be deleted) from real store state rather than guessing.
const DELIVERY_PROFILE_DEFAULT_HYDRATE_QUERY: &str = "query ShippingDeliveryProfileHydrate($id: ID!) { deliveryProfile(id: $id) { id name default merchantOwned version } }";
const DELIVERY_PROFILE_UPDATE_HYDRATE_QUERY: &str = "query ShippingDeliveryProfileUpdateHydrate($id: ID!) { deliveryProfile(id: $id) { id name default version } }";

const SHIPPING_FULFILLMENT_ORDER_HYDRATE_QUERY: &str = r#"
query ShippingFulfillmentOrderHydrate($id: ID!) {
  node(id: $id) {
    __typename
    ... on FulfillmentOrder {
      id
      status
      requestStatus
      fulfillAt
      fulfillBy
      updatedAt
      supportedActions {
        action
      }
      assignedLocation {
        name
        location {
          id
          name
        }
      }
      fulfillmentHolds {
        id
        handle
        reason
        reasonNotes
        displayReason
        heldByApp {
          id
          title
        }
        heldByRequestingApp
      }
      lineItems(first: 250) {
        nodes {
          id
          totalQuantity
          remainingQuantity
          lineItem {
            id
            title
            quantity
            fulfillableQuantity
          }
        }
      }
      order {
        id
        name
        displayFulfillmentStatus
      }
    }
  }
}
"#;

struct FulfillmentOrderStoreBackedPreamble {
    response_key: String,
    payload_selection: Vec<SelectedField>,
    arguments: BTreeMap<String, ResolvedValue>,
    id: String,
    order_id: String,
    index: usize,
}

const SHIPPING_FULFILLMENT_ORDER_DIRECT_HYDRATE_QUERY: &str = r#"query ShippingFulfillmentOrderHydrate($id: ID!) {
    fulfillmentOrder(id: $id) {
      id status requestStatus fulfillAt fulfillBy updatedAt
      supportedActions { action }
      assignedLocation { name location { id name } }
      fulfillmentHolds { id handle reason reasonNotes displayReason heldByApp { id title } heldByRequestingApp }
      merchantRequests(first: 10) { nodes { kind message requestOptions } }
      lineItems(first: 20) { nodes { id totalQuantity remainingQuantity lineItem { id title quantity fulfillableQuantity } } }
      order { id name displayFulfillmentStatus }
    }
  }"#;

const SHIPPING_FULFILLMENT_ORDER_DIRECT_MULTILINE_HYDRATE_QUERY: &str = r#"query ShippingFulfillmentOrderHydrate($id: ID!) {
    fulfillmentOrder(id: $id) {
      id
      status
      requestStatus
      fulfillAt
      fulfillBy
      updatedAt
      supportedActions { action }
      assignedLocation { name location { id name } }
      fulfillmentHolds { id handle reason reasonNotes displayReason heldByApp { id title } heldByRequestingApp }
      merchantRequests(first: 10) { nodes { kind message requestOptions } }
      lineItems(first: 20) { nodes { id totalQuantity remainingQuantity lineItem { id title quantity fulfillableQuantity } } }
      order { id name displayFulfillmentStatus }
    }
  }"#;

const SHIPPING_FULFILLMENT_ORDER_RELEASE_HOLD_HYDRATE_QUERY: &str = r#"query FulfillmentOrderReleaseHoldSelectiveHydrate($id: ID!) {
  fulfillmentOrder(id: $id) {
    id
    status
    requestStatus
    fulfillAt
    fulfillBy
    updatedAt
    supportedActions {
      action
    }
    assignedLocation {
      name
      location {
        id
        name
      }
    }
    fulfillmentHolds {
      id
      handle
      reason
      reasonNotes
      displayReason
      heldByRequestingApp
    }
    merchantRequests(first: 10) {
      nodes {
        kind
        message
        requestOptions
      }
    }
    lineItems(first: 20) {
      nodes {
        id
        totalQuantity
        remainingQuantity
        lineItem {
          id
          title
          quantity
          fulfillableQuantity
        }
      }
    }
    order {
      id
      name
      displayFulfillmentStatus
    }
  }
}"#;

impl DraftProxy {
    pub(in crate::proxy) fn backup_region_update(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let document = parsed_document(query, variables);
        let root_field = document.as_ref().and_then(|document| {
            document
                .root_fields
                .iter()
                .find(|field| field.name == "backupRegionUpdate")
        });
        let response_key = root_field
            .map(|field| field.response_key.clone())
            .unwrap_or_else(|| "backupRegionUpdate".to_string());
        if request.headers.iter().any(|(name, token)| {
            name.eq_ignore_ascii_case("X-Shopify-Access-Token") && token == "shpat_delegate_proxy_1"
        }) {
            return ok_json(json!({
                "errors": [{
                    "message": "Access denied for backupRegionUpdate field. Required access: `read_markets` for queries and both `read_markets` as well as `write_markets` for mutations.",
                    "locations": [{ "line": 2, "column": 3 }],
                    "extensions": {
                        "code": "ACCESS_DENIED",
                        "documentation": "https://shopify.dev/api/usage/access-scopes",
                        "requiredAccess": "`read_markets` for queries and both `read_markets` as well as `write_markets` for mutations."
                    },
                    "path": ["backupRegionUpdate"]
                }],
                "data": { response_key: null }
            }));
        }
        let operation_path = document
            .as_ref()
            .map(|document| document.operation_path.as_str())
            .unwrap_or("mutation");
        let country_code = match backup_region_update_country_code(root_field) {
            BackupRegionCountryCodeInput::ReadCurrent => None,
            BackupRegionCountryCodeInput::CountryCode(country_code) => Some(country_code),
            BackupRegionCountryCodeInput::Missing => {
                return ok_json(backup_region_country_code_coercion_error(
                    "Argument 'countryCode' on InputObject 'BackupRegionUpdateInput' is required. Expected type CountryCode!",
                    operation_path,
                    "missingRequiredInputObjectAttribute",
                ));
            }
            BackupRegionCountryCodeInput::Invalid(value) => {
                return ok_json(backup_region_country_code_coercion_error(
                    &format!(
                        "Argument 'countryCode' on InputObject 'BackupRegionUpdateInput' has an invalid value ({value}). Expected type 'CountryCode!'."
                    ),
                    operation_path,
                    "argumentLiteralsIncompatible",
                ));
            }
        };

        let region = country_code.as_deref().and_then(backup_region_country);
        match region {
            None if country_code.is_none() => ok_json(json!({
                "data": { response_key: { "backupRegion": self.store.staged.backup_region.clone(), "userErrors": [] } }
            })),
            // A known country only becomes the backup region when it is still
            // covered by an active, non-legacy region market. When every active
            // region market has dropped the country, Shopify reports
            // REGION_NOT_FOUND even though the country itself is recognized.
            Some(region)
                if country_code
                    .as_deref()
                    .is_some_and(|code| self.backup_region_country_has_region_market(code)) =>
            {
                self.store.staged.backup_region = region.clone();
                let staged_id = region
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("gid://shopify/MarketRegionCountry/local")
                    .to_string();
                self.record_mutation_log_entry(
                    request,
                    query,
                    variables,
                    "backupRegionUpdate",
                    vec![staged_id],
                );
                ok_json(json!({
                    "data": { response_key: { "backupRegion": region, "userErrors": [] } }
                }))
            }
            _ => {
                let mut user_error = serde_json::Map::from_iter([
                    ("field".to_string(), json!(["region"])),
                    ("message".to_string(), json!("Region not found.")),
                    ("code".to_string(), json!("REGION_NOT_FOUND")),
                ]);
                let include_user_error_typename = root_field
                    .map(|field| nested_selected_fields(&field.selection, &["userErrors"]))
                    .unwrap_or_default()
                    .iter()
                    .any(|field| field.name == "__typename");
                if include_user_error_typename {
                    user_error.insert("__typename".to_string(), json!("MarketUserError"));
                }
                ok_json(json!({
                "data": {
                    response_key: {
                        "backupRegion": null,
                        "userErrors": [Value::Object(user_error)]
                    }
                }
                }))
            }
        }
    }

    pub(in crate::proxy) fn location_mutation(
        &mut self,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        match root_field {
            "locationAdd" => self.location_add(query, variables, request),
            "locationEdit" => self.location_edit(query, variables, request),
            "locationActivate" => self.location_activate(query, variables, request),
            "locationDelete" => self.location_delete(query, variables, request),
            _ => json_error(501, "Unsupported location mutation"),
        }
    }

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
            return (
                delivery_profile_remove_payload_json(
                    Value::Null,
                    &field.selection,
                    vec![user_error_omit_code(
                        Value::Null,
                        "Cannot delete the default profile.",
                        None,
                    )],
                ),
                Vec::new(),
            );
        }
        if profile.is_none() {
            if self.delivery_profile_hydrates_as_default(&id, request) {
                return (
                    delivery_profile_remove_payload_json(
                        Value::Null,
                        &field.selection,
                        vec![user_error_omit_code(
                            Value::Null,
                            "Cannot delete the default profile.",
                            None,
                        )],
                    ),
                    Vec::new(),
                );
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
        let location_ids = resolved_string_list_field_unsorted(input, "locations");
        let locations = location_ids
            .into_iter()
            .map(|id| delivery_profile_location_record(&id))
            .collect::<Vec<_>>();
        let zones = resolved_object_list_field(input, "zonesToCreate")
            .into_iter()
            .map(|zone_input| self.delivery_zone_record_from_input(&zone_input))
            .collect::<Vec<_>>();
        json!({
            "locationGroup": {
                "id": self.next_proxy_synthetic_gid("DeliveryLocationGroup"),
                "locations": locations,
                "locationsCount": {
                    "count": locations.len(),
                    "precision": "EXACT"
                }
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
                "amount": resolved_money_amount_string(criteria.get("amount")),
                "currencyCode": resolved_string_field(&criteria, "currencyCode").unwrap_or_else(|| "USD".to_string())
            }
        })
    }

    fn delivery_profile_apply_update_input(
        &mut self,
        profile: &mut Value,
        input: &BTreeMap<String, ResolvedValue>,
    ) {
        let delete_ids = resolved_string_list_field_unsorted(input, "conditionsToDelete")
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
                for location_id in
                    resolved_string_list_field_unsorted(&group_update, "locationsToAdd")
                {
                    if !locations.iter().any(|location| {
                        location.get("id").and_then(Value::as_str) == Some(location_id.as_str())
                    }) {
                        locations.push(delivery_profile_location_record(&location_id));
                    }
                }
                let count = locations.len();
                group["locationGroup"]["locationsCount"] =
                    json!({ "count": count, "precision": "EXACT" });
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
        for variant_id in resolved_string_list_field_unsorted(input, "variantsToAssociate") {
            if !variant_ids.contains(&variant_id) {
                variant_ids.push(variant_id);
            }
        }
        if !create {
            let removals = resolved_string_list_field_unsorted(input, "variantsToDissociate")
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
        if let Some(limit) = arguments.get("first").and_then(resolved_as_usize) {
            profiles.truncate(limit);
        }
        selected_json(
            &connection_json_with_cursor(
                profiles,
                |_, profile| value_id_cursor(profile),
                connection_page_info(false, false, None, None),
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
        let mut locations = self.effective_shipping_locations();
        if let Some(limit) = arguments.get("first").and_then(resolved_as_usize) {
            locations.truncate(limit);
        }
        let mut fields = serde_json::Map::new();
        for selection in selections {
            let value = match selection.name.as_str() {
                "nodes" => Some(Value::Array(
                    locations
                        .iter()
                        .map(|location| location_selected_json(location, &selection.selection))
                        .collect(),
                )),
                "edges" => Some(Value::Array(
                    locations
                        .iter()
                        .map(|location| {
                            let edge = json!({
                                "cursor": location.get("id").and_then(Value::as_str).unwrap_or_default(),
                                "node": location
                            });
                            selected_json(&edge, &selection.selection)
                        })
                        .collect(),
                )),
                "pageInfo" => Some(selected_json(
                    &empty_page_info(),
                    &selection.selection,
                )),
                _ => None,
            };
            if let Some(value) = value {
                fields.insert(selection.response_key.clone(), value);
            }
        }
        Value::Object(fields)
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

    fn observe_delivery_profile_locations_response(&mut self, response: &Response) {
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

    fn stage_observed_shipping_location(&mut self, mut location: Value) {
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

    pub(in crate::proxy) fn shipping_settings_read_response(
        &mut self,
        request: &Request,
        fields: &[RootFieldSelection],
    ) -> Response {
        if self.config.read_mode != ReadMode::Snapshot
            && self.store.staged.observed_shipping_locations.is_empty()
            && self.store.staged.carrier_services.is_empty()
        {
            let response = (self.upstream_transport)(request.clone());
            self.observe_shipping_settings_response(&response);
            return response;
        }
        ok_json(json!({ "data": self.shipping_settings_read_data(fields) }))
    }

    fn shipping_settings_read_data(&self, fields: &[RootFieldSelection]) -> Value {
        let mut data = self.delivery_profile_locations_read_data(fields);
        if let Value::Object(data) = &mut data {
            for field in fields {
                if field.name == "availableCarrierServices" {
                    data.insert(
                        field.response_key.clone(),
                        self.available_carrier_services_json(&field.selection),
                    );
                }
            }
        }
        data
    }

    fn available_carrier_services_json(&self, selection: &[SelectedField]) -> Value {
        Value::Array(
            self.store
                .staged
                .carrier_services
                .values()
                .map(|carrier| {
                    selected_json(
                        &json!({
                            "carrierService": carrier
                        }),
                        selection,
                    )
                })
                .collect(),
        )
    }

    fn observe_shipping_settings_response(&mut self, response: &Response) {
        self.observe_delivery_profile_locations_response(response);
        if let Some(services) = response.body["data"]["availableCarrierServices"].as_array() {
            for service_entry in services {
                if let Some(carrier) = service_entry.get("carrierService") {
                    self.stage_observed_carrier_service(carrier.clone());
                }
            }
        }
    }

    fn stage_observed_carrier_service(&mut self, carrier: Value) {
        let Some(id) = carrier
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            return;
        };
        self.store.staged.carrier_services.insert(id, carrier);
    }

    pub(in crate::proxy) fn location_local_pickup_mutation(
        &mut self,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let Some(fields) = root_fields(query, variables) else {
            return json_error(400, "Could not parse GraphQL operation");
        };
        let data = root_payload_json(&fields, |field| {
            let payload = match field.name.as_str() {
                "locationLocalPickupEnable" => {
                    self.location_local_pickup_enable_payload(field, request, query, variables)
                }
                "locationLocalPickupDisable" => {
                    self.location_local_pickup_disable_payload(field, request, query, variables)
                }
                _ => return None,
            };
            Some(payload)
        });
        if data.as_object().is_none_or(serde_json::Map::is_empty) {
            return json_error(
                501,
                &format!(
                    "No Rust stage-locally dispatcher implemented for root field: {}",
                    root_field
                ),
            );
        }
        ok_json(json!({ "data": data }))
    }

    fn location_local_pickup_enable_payload(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let input = resolved_object_field(&field.arguments, "localPickupSettings")
            .unwrap_or_else(|| field.arguments.clone());
        let location_id = resolved_string_field(&input, "locationId").unwrap_or_default();
        let pickup_time = resolved_string_field(&input, "pickupTime").unwrap_or_default();
        let user_errors = self.location_local_pickup_enable_user_errors(
            &location_id,
            &pickup_time,
            field.name.as_str(),
        );
        if !user_errors.is_empty() {
            return location_local_pickup_enable_payload_selected_json(
                Value::Null,
                &field.selection,
                user_errors,
            );
        }

        let instructions = input
            .get("instructions")
            .and_then(|value| match value {
                ResolvedValue::String(value) => Some(Value::String(value.clone())),
                ResolvedValue::Null => Some(Value::Null),
                _ => None,
            })
            .unwrap_or(Value::Null);
        let settings = json!({
            "pickupTime": pickup_time,
            "instructions": instructions
        });
        let mut location = self
            .active_local_pickup_location(&location_id)
            .unwrap_or_else(|| self.staged_location_record(&location_id));
        location["isActive"] = json!(true);
        location["isFulfillmentService"] = json!(false);
        location["localPickupSettingsV2"] = settings.clone();
        location["localPickupSettings"] = settings.clone();
        self.stage_local_pickup_location(location);
        self.record_mutation_log_entry(
            request,
            query,
            variables,
            "locationLocalPickupEnable",
            vec![location_id],
        );

        location_local_pickup_enable_payload_selected_json(settings, &field.selection, Vec::new())
    }

    fn location_local_pickup_disable_payload(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let location_id = resolved_string_field(&field.arguments, "locationId").unwrap_or_default();
        let user_errors =
            self.location_local_pickup_location_user_errors(&location_id, field.name.as_str());
        if user_errors.is_empty() {
            let mut location = self
                .active_local_pickup_location(&location_id)
                .unwrap_or_else(|| self.staged_location_record(&location_id));
            location["isActive"] = json!(true);
            location["isFulfillmentService"] = json!(false);
            location["localPickupSettingsV2"] = Value::Null;
            location["localPickupSettings"] = Value::Null;
            self.stage_local_pickup_location(location);
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                "locationLocalPickupDisable",
                vec![location_id.clone()],
            );
        }
        location_local_pickup_disable_payload_selected_json(
            location_id,
            &field.selection,
            user_errors,
        )
    }

    fn location_local_pickup_enable_user_errors(
        &self,
        location_id: &str,
        pickup_time: &str,
        root_field: &str,
    ) -> Vec<Value> {
        let location_errors =
            self.location_local_pickup_location_user_errors(location_id, root_field);
        if !location_errors.is_empty() {
            return location_errors;
        }
        if !local_pickup_time_is_standard(pickup_time) {
            return vec![user_error(
                ["localPickupSettings"],
                "Custom pickup time is not allowed for local pickup settings.",
                Some("CUSTOM_PICKUP_TIME_NOT_ALLOWED"),
            )];
        }
        Vec::new()
    }

    fn location_local_pickup_location_user_errors(
        &self,
        location_id: &str,
        root_field: &str,
    ) -> Vec<Value> {
        if self.active_local_pickup_location(location_id).is_some() {
            return Vec::new();
        }
        vec![user_error_with_code_value(
            ["localPickupSettings"],
            &format!(
                "Unable to find an active location for location ID {}",
                resource_id_path_tail(location_id)
            ),
            json!(if root_field == "locationLocalPickupEnable" {
                "ACTIVE_LOCATION_NOT_FOUND"
            } else {
                "LOCATION_NOT_FOUND"
            }),
        )]
    }

    pub(in crate::proxy) fn location_add(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let Some(document) = parsed_document(query, variables) else {
            return json_error(400, "Unable to parse locationAdd mutation");
        };
        let mut data = serde_json::Map::new();
        for field in document
            .root_fields
            .iter()
            .filter(|field| field.name == "locationAdd")
        {
            let Some(input) = resolved_object_field(&field.arguments, "input") else {
                return ok_json(location_add_missing_input_error(
                    &document.operation_path,
                    field,
                ));
            };
            if let Some(error) =
                self.location_add_input_shape_error(&document.operation_path, field, &input)
            {
                return ok_json(error);
            }
            if resolved_object_list_field(&input, "metafields")
                .iter()
                .any(|metafield| {
                    metafield.contains_key("key")
                        && resolved_string_field(metafield, "key")
                            .map(|key| key.trim().is_empty())
                            .unwrap_or(true)
                })
            {
                return ok_json(location_add_metafield_blank_key_error(field, &document));
            }

            let user_errors = self.location_add_user_errors(&input);
            let location = if user_errors.is_empty() {
                let id = self.next_proxy_synthetic_gid("Location");
                let location = self.location_record_from_add_input(&id, &input);
                self.stage_location(location.clone());
                self.record_mutation_log_entry(request, query, variables, "locationAdd", vec![id]);
                location
            } else {
                Value::Null
            };
            data.insert(
                field.response_key.clone(),
                location_payload_selected_json(location, &field.selection, user_errors),
            );
        }
        ok_json(json!({ "data": Value::Object(data) }))
    }

    pub(in crate::proxy) fn location_activate(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        if location_requires_idempotency(request, query) {
            return ok_json(location_idempotency_required_error(
                "locationActivate",
                query,
                variables,
            ));
        }
        let Some(fields) = root_fields(query, variables) else {
            return json_error(400, "Unable to parse locationActivate mutation");
        };
        let mut data = serde_json::Map::new();
        for field in fields {
            if field.name != "locationActivate" {
                continue;
            }
            let location_id =
                resolved_string_field(&field.arguments, "locationId").unwrap_or_default();
            self.ensure_location_hydrated(&location_id, request);
            let source_location = self.location_source_record(&location_id);
            let errors = self.location_activate_errors(&source_location);
            let location = if errors.is_empty() {
                let mut location = source_location;
                location["isActive"] = json!(true);
                location["activatable"] = json!(true);
                location["deactivatable"] = json!(true);
                location["deletable"] = json!(false);
                self.stage_location(location.clone());
                self.record_mutation_log_entry(
                    request,
                    query,
                    variables,
                    "locationActivate",
                    vec![location_id.clone()],
                );
                location
            } else {
                if errors.iter().any(|error| {
                    error.get("code").and_then(Value::as_str) == Some("LOCATION_LIMIT")
                }) && location_id == "gid://shopify/Location/location-add-limit-seed"
                {
                    self.store.staged.location_limit_reached = true;
                }
                source_location
            };
            data.insert(
                field.response_key,
                location_activate_payload_selected_json(location, &field.selection, errors),
            );
        }
        ok_json(json!({ "data": Value::Object(data) }))
    }

    /// Applies a `locationDelete`. The target is resolved through the local overlay
    /// first, falling back to an upstream hydrate (live-hybrid only); unknown ids
    /// surface `LOCATION_NOT_FOUND`. On success the location is tombstoned (so
    /// later reads return null and the connection omits it) and its inventory
    /// levels are dropped.
    pub(in crate::proxy) fn location_delete(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let Some(fields) = root_fields(query, variables) else {
            return json_error(400, "Unable to parse locationDelete mutation");
        };
        let mut data = serde_json::Map::new();
        for field in fields {
            if field.name != "locationDelete" {
                continue;
            }
            let location_id =
                resolved_string_field(&field.arguments, "locationId").unwrap_or_default();
            let location = self
                .location_for_read(&location_id)
                .or_else(|| self.hydrate_location_for_mutation(request, &location_id));
            let errors = self.location_delete_errors(&location_id, location.as_ref());
            let deleted_location_id = if errors.is_empty() {
                self.delete_location_inventory_levels(&location_id);
                self.delete_staged_location(&location_id);
                self.record_mutation_log_entry(
                    request,
                    query,
                    variables,
                    "locationDelete",
                    vec![location_id.clone()],
                );
                Value::String(location_id)
            } else {
                Value::Null
            };
            data.insert(
                field.response_key,
                location_delete_payload_selected_json(
                    deleted_location_id,
                    &field.selection,
                    errors,
                ),
            );
        }
        ok_json(json!({ "data": Value::Object(data) }))
    }

    /// Resolves the user errors Shopify raises for a `locationDelete`, mirroring
    /// the public Admin API. For staged locations inventory presence is read from
    /// the local overlay; for hydrated baselines it falls back to the upstream
    /// `hasActiveInventory`/`deletable` fields.
    fn location_delete_errors(&self, location_id: &str, location: Option<&Value>) -> Vec<Value> {
        let Some(location) = location else {
            return vec![location_delete_user_error(
                "LOCATION_NOT_FOUND",
                "Location not found.",
            )];
        };
        if location
            .get("isFulfillmentService")
            .and_then(Value::as_bool)
            == Some(true)
        {
            return vec![location_delete_user_error(
                "LOCATION_NOT_FOUND",
                "Location not found.",
            )];
        }

        let mut errors = Vec::new();
        if location.get("isActive").and_then(Value::as_bool) == Some(true) {
            errors.push(location_delete_user_error(
                "LOCATION_IS_ACTIVE",
                "The location cannot be deleted while it is active.",
            ));
        }
        let has_inventory = if self.store.staged.locations.contains_key(location_id) {
            self.location_has_inventory(location_id)
        } else {
            location
                .get("hasActiveInventory")
                .and_then(Value::as_bool)
                .unwrap_or_else(|| self.location_has_inventory(location_id))
                || self.location_has_inventory(location_id)
        };
        if has_inventory {
            errors.push(location_delete_user_error(
                "LOCATION_HAS_INVENTORY",
                "The location cannot be deleted while it has inventory.",
            ));
        }
        if location
            .get("hasUnfulfilledOrders")
            .and_then(Value::as_bool)
            == Some(true)
        {
            errors.push(location_delete_user_error(
                "LOCATION_HAS_PENDING_ORDERS",
                "The location cannot be deleted while it has pending orders.",
            ));
        }
        if !self.store.staged.locations.contains_key(location_id)
            && location.get("deletable").and_then(Value::as_bool) == Some(false)
            && errors.is_empty()
        {
            errors.push(location_delete_user_error(
                "LOCATION_NOT_DELETABLE",
                "The location cannot be deleted.",
            ));
        }
        errors
    }

    fn delete_staged_location(&mut self, location_id: &str) {
        self.store.staged.locations.remove(location_id);
        self.store
            .staged
            .observed_shipping_locations
            .remove(location_id);
        self.store
            .staged
            .fulfillment_service_locations
            .remove(location_id);
        self.store
            .staged
            .locations
            .tombstone(location_id.to_string());
    }

    fn delete_location_inventory_levels(&mut self, location_id: &str) {
        let keys = self
            .store
            .staged
            .inventory_levels
            .keys()
            .filter(|(_, staged_location_id)| staged_location_id == location_id)
            .cloned()
            .collect::<Vec<_>>();
        for key in keys {
            self.store.staged.inventory_levels.remove(&key);
        }
        self.store
            .staged
            .inventory_level_order
            .retain(|(_, staged_location_id)| staged_location_id != location_id);
        self.store
            .staged
            .inventory_quantity_updated_at
            .retain(|(_, staged_location_id, _), _| staged_location_id != location_id);
    }

    /// Fetches a baseline location from upstream so an edit/delete on a location
    /// the proxy never staged can validate against its real state (live-hybrid
    /// only). Returns `None` under snapshot reads, for an empty id, or for a
    /// tombstoned location. On a 2xx with a `location` object the record is mirrored
    /// into the observed overlay and returned.
    fn hydrate_location_for_mutation(
        &mut self,
        request: &Request,
        location_id: &str,
    ) -> Option<Value> {
        if self.config.read_mode == ReadMode::Snapshot
            || location_id.is_empty()
            || self.store.staged.locations.is_tombstoned(location_id)
        {
            return None;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": LOCATION_HYDRATE_QUERY,
                "operationName": "StorePropertiesLocationHydrate",
                "variables": { "id": location_id }
            }),
        );
        if !(200..300).contains(&response.status) {
            return None;
        }
        let location = response.body["data"]["location"].clone();
        if !location.is_object() {
            return None;
        }
        self.stage_observed_shipping_location(location.clone());
        Some(location)
    }

    /// Applies a `locationEdit`. The target is resolved through the local overlay
    /// first; when it is not staged the proxy hydrates it from upstream (live-hybrid
    /// only) so edits to real baseline locations validate against their actual
    /// state, and unknown ids surface the "Location not found." user error. The
    /// merged record is re-staged so subsequent local reads observe the change.
    pub(in crate::proxy) fn location_edit(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let Some(fields) = root_fields(query, variables) else {
            return json_error(400, "Unable to parse locationEdit mutation");
        };
        let mut data = serde_json::Map::new();
        for field in fields {
            if field.name != "locationEdit" {
                continue;
            }
            let location_id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
            let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
            if let Some(error) = self.location_edit_input_shape_error(&input) {
                return ok_json(error);
            }

            let source_location = self
                .location_for_read(&location_id)
                .or_else(|| self.hydrate_location_for_mutation(request, &location_id));
            let mut user_errors = Vec::new();
            if source_location.is_none() {
                user_errors.push(user_error_omit_code(["id"], "Location not found.", None));
            } else {
                user_errors.extend(self.location_edit_user_errors(&location_id, &input));
            }

            let location = if user_errors.is_empty() {
                let mut location =
                    source_location.unwrap_or_else(|| self.staged_location_record(&location_id));
                self.apply_location_edit_input(&mut location, &input);
                self.stage_location(location.clone());
                self.record_mutation_log_entry(
                    request,
                    query,
                    variables,
                    "locationEdit",
                    vec![location_id.clone()],
                );
                location
            } else {
                Value::Null
            };

            data.insert(
                field.response_key,
                location_payload_selected_json(location, &field.selection, user_errors),
            );
        }
        ok_json(json!({ "data": Value::Object(data) }))
    }

    /// Surfaces the `LocationEditInput!` coercion error Shopify raises for an
    /// unknown `address.countryCode` before any staging happens, anchoring it at
    /// the variable definition like the live API.
    fn location_edit_input_shape_error(
        &self,
        input: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        if let Some(address) = resolved_object_field(input, "address") {
            if let Some(country_code) = resolved_string_field(&address, "countryCode") {
                if !location_country_code_is_valid(&country_code) {
                    return Some(location_edit_invalid_variable_error(
                        "address.countryCode",
                        &format!(
                            "Expected \"{}\" to be one of: {}",
                            country_code, LOCATION_COUNTRY_CODES
                        ),
                        input,
                    ));
                }
            }
        }
        None
    }

    fn apply_location_edit_input(
        &mut self,
        location: &mut Value,
        input: &BTreeMap<String, ResolvedValue>,
    ) {
        let location_id = location
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if let Some(name) = resolved_string_field(input, "name") {
            location["name"] = json!(name);
        }
        if let Some(is_active) =
            resolved_bool_field(input, "isActive").or_else(|| resolved_bool_field(input, "active"))
        {
            location["isActive"] = json!(is_active);
            location["deletable"] = json!(!is_active && !self.location_has_inventory(&location_id));
        }
        if let Some(fulfills_online_orders) = resolved_bool_field(input, "fulfillsOnlineOrders") {
            location["fulfillsOnlineOrders"] = json!(fulfills_online_orders);
        }
        if let Some(address_input) = resolved_object_field(input, "address") {
            let mut address = location
                .get("address")
                .cloned()
                .unwrap_or_else(|| json!({}));
            if !address.is_object() {
                address = json!({});
            }
            for field in [
                "address1",
                "address2",
                "city",
                "countryCode",
                "provinceCode",
                "zip",
            ] {
                if let Some(value) = resolved_string_field(&address_input, field) {
                    address[field] = json!(value);
                }
            }
            if let Some(country_code) = resolved_string_field(&address_input, "countryCode") {
                if let Some(country) = location_country_name(&country_code) {
                    address["country"] = json!(country);
                }
            }
            // Shopify derives the full province name from the effective
            // country + province codes whenever the address is edited. A
            // province-only edit (no countryCode in the input) still re-derives
            // the name from the country code already on the record.
            let effective_country_code = address
                .get("countryCode")
                .and_then(Value::as_str)
                .map(str::to_string);
            let effective_province_code = address
                .get("provinceCode")
                .and_then(Value::as_str)
                .filter(|code| !code.is_empty())
                .map(str::to_string);
            address["province"] = match (
                effective_country_code.as_deref(),
                effective_province_code.as_deref(),
            ) {
                (Some(country), Some(province)) => province_name_for_code(country, province)
                    .map(Value::from)
                    .unwrap_or(Value::Null),
                _ => Value::Null,
            };
            location["address"] = address;
        }
        let metafields = self.location_metafields_from_input(&location_id, input);
        if !metafields.is_empty() {
            location["metafields"] = Value::Array(metafields);
        }
        location["hasActiveInventory"] = json!(self.location_has_inventory(&location_id));
        location["updatedAt"] = json!("2024-01-01T00:00:01.000Z");
    }

    /// Validates a `locationEdit` input against the staged record, mirroring the
    /// public Admin API's `locationEdit` user errors. Only fields present in the
    /// input are validated (edit inputs are sparse), and the name-uniqueness check
    /// excludes the location being edited.
    fn location_edit_user_errors(
        &self,
        location_id: &str,
        input: &BTreeMap<String, ResolvedValue>,
    ) -> Vec<Value> {
        let mut errors = Vec::new();
        if let Some(name) = resolved_string_field(input, "name") {
            if name.trim().is_empty() {
                errors.push(user_error(
                    ["input", "name"],
                    "Add a location name",
                    Some("BLANK"),
                ));
            } else if name.chars().count() > 100 {
                errors.push(user_error(
                    ["input", "name"],
                    "Use a shorter location name (up to 100 characters)",
                    Some("TOO_LONG"),
                ));
            } else if self.location_name_exists_except(&name, location_id) {
                errors.push(user_error(
                    ["input", "name"],
                    "You already have a location with this name",
                    Some("TAKEN"),
                ));
            }
        }
        if let Some(address) = resolved_object_field(input, "address") {
            if resolved_string_field(&address, "address1")
                .is_some_and(|address1| address1.chars().count() > 255)
            {
                errors.push(user_error(
                    ["input", "address", "address1"],
                    "Use a shorter name for the street (up to 255 characters)",
                    Some("TOO_LONG"),
                ));
            }
            if resolved_string_field(&address, "city")
                .is_some_and(|city| city.chars().count() > 255)
            {
                errors.push(user_error(
                    ["input", "address", "city"],
                    "Use a shorter city name (up to 255 characters)",
                    Some("TOO_LONG"),
                ));
            }
            if resolved_string_field(&address, "zip").is_some_and(|zip| zip.chars().count() > 255) {
                errors.push(user_error(
                    ["input", "address", "zip"],
                    "Use a shorter postal / ZIP code (up to 255 characters)",
                    Some("TOO_LONG"),
                ));
            }
        }
        for (index, metafield) in resolved_object_list_field(input, "metafields")
            .into_iter()
            .enumerate()
        {
            if let Some(metafield_type) = resolved_string_field(&metafield, "type") {
                if !LOCATION_METAFIELD_VALID_TYPES.contains(&metafield_type.as_str()) {
                    errors.push(user_error(
                        json!(["input", "metafields", (index + 1).to_string(), "type"]),
                        &format!(
                            "Type must be one of the following: {}.",
                            LOCATION_METAFIELD_VALID_TYPES.join(", ")
                        ),
                        Some("INVALID_TYPE"),
                    ));
                }
            }
        }
        // Shopify refuses to disable online-order fulfillment on the last
        // location that still fulfills online orders.
        if resolved_bool_field(input, "fulfillsOnlineOrders") == Some(false)
            && !self.has_other_online_order_fulfillment_location(location_id)
        {
            errors.push(user_error(["input"], "Online order fulfillment could not be disabled for this location as it is the only location that fulfills online orders.", Some("CANNOT_DISABLE_ONLINE_ORDER_FULFILLMENT")));
        }
        errors
    }

    fn location_name_exists_except(&self, name: &str, except_id: &str) -> bool {
        let normalized = name.trim().to_lowercase();
        self.store.staged.locations.iter().any(|(id, location)| {
            id != except_id
                && location
                    .get("name")
                    .and_then(Value::as_str)
                    .is_some_and(|existing| existing.trim().eq_ignore_ascii_case(&normalized))
        })
    }

    fn location_add_input_shape_error(
        &self,
        operation_path: &str,
        field: &RootFieldSelection,
        input: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        if input.contains_key("capabilities") {
            return Some(location_add_invalid_variable_error(
                "capabilities",
                "Field is not defined on LocationAddInput",
                input,
            ));
        }
        if input.contains_key("capabilitiesToAdd") {
            return Some(location_add_inline_argument_not_accepted_error(
                operation_path,
                field,
                "capabilitiesToAdd",
            ));
        }
        let address = match input.get("address") {
            Some(ResolvedValue::Object(address)) => address,
            _ => {
                return Some(location_add_missing_address_error(operation_path, field));
            }
        };
        let country_code = resolved_string_field(address, "countryCode");
        let Some(country_code) = country_code else {
            if input_was_variable(field) {
                return Some(location_add_invalid_variable_error(
                    "address.countryCode",
                    "Expected value to not be null",
                    input,
                ));
            }
            return Some(location_add_missing_country_code_error(
                operation_path,
                field,
            ));
        };
        if !location_country_code_is_valid(&country_code) {
            return Some(location_add_invalid_variable_error(
                "address.countryCode",
                &format!(
                    "Expected \"{}\" to be one of: {}",
                    country_code, LOCATION_COUNTRY_CODES
                ),
                input,
            ));
        }
        None
    }

    fn location_add_user_errors(&self, input: &BTreeMap<String, ResolvedValue>) -> Vec<Value> {
        let mut errors = Vec::new();
        let name = resolved_string_field(input, "name").unwrap_or_default();
        if name.trim().is_empty() {
            errors.push(user_error(
                ["input", "name"],
                "Add a location name",
                Some("BLANK"),
            ));
        } else if name.chars().count() > 100 {
            errors.push(user_error(
                ["input", "name"],
                "Use a shorter location name (up to 100 characters)",
                Some("TOO_LONG"),
            ));
        } else if self.location_name_exists(&name) {
            errors.push(user_error(
                ["input", "name"],
                "You already have a location with this name",
                Some("TAKEN"),
            ));
        }
        if let Some(address) = resolved_object_field(input, "address") {
            if resolved_string_field(&address, "address1")
                .is_some_and(|address1| address1.chars().count() > 255)
            {
                errors.push(user_error(
                    ["input", "address", "address1"],
                    "Use a shorter name for the street (up to 255 characters)",
                    Some("TOO_LONG"),
                ));
            }
            if resolved_string_field(&address, "zip").is_some_and(|zip| zip.chars().count() > 255) {
                errors.push(user_error(
                    ["input", "address", "zip"],
                    "Use a shorter postal / ZIP code (up to 255 characters)",
                    Some("TOO_LONG"),
                ));
            }
        }
        for (index, metafield) in resolved_object_list_field(input, "metafields")
            .into_iter()
            .enumerate()
        {
            if let Some(metafield_type) = resolved_string_field(&metafield, "type") {
                if !LOCATION_METAFIELD_VALID_TYPES.contains(&metafield_type.as_str()) {
                    errors.push(user_error(
                        json!(["input", "metafields", index.to_string(), "type"]),
                        &format!(
                            "Type must be one of the following: {}.",
                            LOCATION_METAFIELD_VALID_TYPES.join(", ")
                        ),
                        Some("INVALID_TYPE"),
                    ));
                }
            }
        }
        if self.location_limit_reached() {
            errors.push(user_error(
                ["input"],
                "You have reached the maximum number of locations (200)",
                Some("INVALID"),
            ));
        }
        errors
    }

    fn location_record_from_add_input(
        &mut self,
        id: &str,
        input: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let address_input = resolved_object_field(input, "address").unwrap_or_default();
        let address = location_address_json(&address_input);
        json!({
            "__typename": "Location",
            "id": id,
            "name": resolved_string_field(input, "name").unwrap_or_default(),
            "isActive": true,
            "activatable": false,
            "deactivatable": true,
            "deletable": false,
            "fulfillsOnlineOrders": resolved_bool_field(input, "fulfillsOnlineOrders").unwrap_or(true),
            "hasActiveInventory": false,
            "hasUnfulfilledOrders": false,
            "isFulfillmentService": false,
            "shipsInventory": true,
            "address": address,
            "metafields": self.location_metafields_from_input(id, input),
            "createdAt": "2024-01-01T00:00:00.000Z",
            "updatedAt": "2024-01-01T00:00:00.000Z"
        })
    }

    fn location_metafields_from_input(
        &mut self,
        owner_id: &str,
        input: &BTreeMap<String, ResolvedValue>,
    ) -> Vec<Value> {
        resolved_object_list_field(input, "metafields")
            .into_iter()
            .filter_map(|metafield| {
                let key = resolved_string_field(&metafield, "key").unwrap_or_default();
                if key.trim().is_empty() {
                    return None;
                }
                let value = resolved_string_field(&metafield, "value").unwrap_or_default();
                if value.is_empty() {
                    return None;
                }
                Some(json!({
                    "id": self.next_proxy_synthetic_gid("Metafield"),
                    "ownerId": owner_id,
                    "namespace": resolved_string_field(&metafield, "namespace").unwrap_or_else(|| "custom".to_string()),
                    "key": key,
                    "value": value,
                    "type": resolved_string_field(&metafield, "type").unwrap_or_else(|| "single_line_text_field".to_string())
                }))
            })
            .collect()
    }

    fn location_activate_errors(&self, location: &Value) -> Vec<Value> {
        if location
            .get("hasOngoingRelocation")
            .and_then(Value::as_bool)
            == Some(true)
        {
            return vec![user_error(["locationId"], "This location currently cannot be activated as inventory, pending orders or transfers are being relocated from this location. Please try again later.", Some("HAS_ONGOING_RELOCATION"))];
        }
        if location
            .get("isFulfillmentService")
            .and_then(Value::as_bool)
            == Some(true)
        {
            return vec![user_error(
                ["locationId"],
                "Location not found.",
                Some("LOCATION_NOT_FOUND"),
            )];
        }
        if self.location_limit_reached()
            || location
                .get("reachedLocationLimit")
                .and_then(Value::as_bool)
                == Some(true)
        {
            return vec![user_error(
                ["locationId"],
                "Your shop has reached its location limit.",
                Some("LOCATION_LIMIT"),
            )];
        }
        if self.location_has_non_unique_active_name(location) {
            return vec![user_error(["locationId"], "This location currently cannot be activated because there exists an active location with the same name.", Some("HAS_NON_UNIQUE_NAME"))];
        }
        Vec::new()
    }

    fn location_has_non_unique_active_name(&self, location: &Value) -> bool {
        if location.get("isActive").and_then(Value::as_bool) == Some(true) {
            return false;
        }
        let Some(target_id) = location.get("id").and_then(Value::as_str) else {
            return false;
        };
        let Some(target_name) = location.get("name").and_then(Value::as_str) else {
            return false;
        };

        let mut location_ids = BTreeSet::new();
        for (id, _) in self.store.staged.locations.iter() {
            location_ids.insert(id.clone());
        }
        for id in self.store.staged.observed_shipping_locations.keys() {
            location_ids.insert(id.clone());
        }
        for (id, _) in self.store.staged.fulfillment_service_locations.iter() {
            location_ids.insert(id.clone());
        }

        location_ids.iter().any(|id| {
            if id == target_id {
                return false;
            }
            self.location_for_read(id).is_some_and(|candidate| {
                candidate.get("isActive").and_then(Value::as_bool) == Some(true)
                    && candidate.get("name").and_then(Value::as_str) == Some(target_name)
            })
        })
    }

    /// Hydrates a baseline location from upstream for lifecycle mutations
    /// (activate/deactivate) when it is neither already staged nor covered by a
    /// synthetic guard fixture. Issues the recorded `StorePropertiesLocationHydrate`
    /// query so the cassette replays the real captured location, letting the
    /// proxy preserve the baseline name/scope/state across the mutation instead
    /// of fabricating one. A miss (no recorded call) returns non-2xx and falls
    /// back to the existing synthetic resolution, so non-hydrate scenarios are
    /// unaffected.
    fn ensure_location_hydrated(&mut self, location_id: &str, request: &Request) {
        if self.config.read_mode == ReadMode::Snapshot {
            return;
        }
        if self.store.staged.locations.contains_key(location_id)
            || self
                .store
                .staged
                .fulfillment_service_locations
                .contains_key(location_id)
        {
            return;
        }
        if fixture_location_activate_guard_location(location_id).is_some()
            || fixture_location_deactivate_state_machine_location(location_id).is_some()
        {
            return;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": LOCATION_HYDRATE_QUERY,
                "variables": { "id": location_id }
            }),
        );
        if !(200..300).contains(&response.status) {
            return;
        }
        let Some(node) = response
            .body
            .get("data")
            .and_then(|data| data.get("location"))
            .filter(|node| node.is_object())
        else {
            return;
        };
        let mut record = node.clone();
        if let Some(object) = record.as_object_mut() {
            object.insert("__typename".to_string(), json!("Location"));
        }
        if record.get("isFulfillmentService").and_then(Value::as_bool) == Some(true) {
            self.store
                .staged
                .fulfillment_service_locations
                .insert(location_id.to_string(), record);
        } else {
            self.stage_location(record);
        }
    }

    fn stage_location(&mut self, location: Value) {
        let Some(id) = location
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            return;
        };
        self.store.staged.locations.insert(id, location);
    }

    /// Stage a location whose local-pickup settings were just mutated. The
    /// canonical record lives in `staged.locations` (so direct `location(id:)`
    /// reads resolve it); when the same id was previously observed from an
    /// upstream `locationsAvailableForDeliveryProfilesConnection` response, the
    /// observed mirror is updated in lockstep so the connection read reflects the
    /// new settings too. `localPickupSettings` is kept in sync with the V2 field.
    fn stage_local_pickup_location(&mut self, mut location: Value) {
        let Some(id) = location
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            return;
        };
        location["localPickupSettings"] = location
            .get("localPickupSettingsV2")
            .cloned()
            .unwrap_or(Value::Null);
        if self
            .store
            .staged
            .observed_shipping_locations
            .contains_key(&id)
        {
            self.store
                .staged
                .observed_shipping_locations
                .insert(id.clone(), location.clone());
        }
        self.stage_location(location);
    }

    pub(in crate::proxy) fn has_location_overlay_state(&self) -> bool {
        self.config.read_mode == ReadMode::Snapshot
            || !self.store.staged.locations.is_empty()
            || !self.store.staged.locations.order.is_empty()
            || !self.store.staged.locations.tombstones.is_empty()
            || !self.store.staged.fulfillment_service_locations.is_empty()
            || self.store.staged.location_limit_reached
    }

    /// True when a location read must consult the upstream baseline to answer.
    ///
    /// `location`, `locations`, and id-based `locationByIdentifier` reads resolve
    /// against the store's real locations, so without local overlay state they
    /// must pass through to upstream. `locationByIdentifier(customId:)` is
    /// resolved purely locally (the proxy intentionally does not model id-typed
    /// location metafield definitions and always reports the custom id as
    /// not found), so it never needs the baseline.
    pub(in crate::proxy) fn location_read_needs_upstream(
        &self,
        fields: &[RootFieldSelection],
    ) -> bool {
        fields.iter().any(|field| match field.name.as_str() {
            "location" | "locations" => true,
            "locationByIdentifier" => resolved_object_field(&field.arguments, "identifier")
                .map(|identifier| !identifier.contains_key("customId"))
                .unwrap_or(true),
            _ => false,
        })
    }

    pub(in crate::proxy) fn location_read_response(
        &self,
        fields: &[RootFieldSelection],
    ) -> Response {
        let mut errors = Vec::new();
        let data = root_payload_json(fields, |field| {
            Some(match field.name.as_str() {
                "location" => {
                    let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                    self.location_for_read(&id)
                        .map(|location| location_selected_json(&location, &field.selection))
                        .unwrap_or(Value::Null)
                }
                "locationByIdentifier" => {
                    let identifier =
                        resolved_object_field(&field.arguments, "identifier").unwrap_or_default();
                    let id = resolved_string_field(&identifier, "id").unwrap_or_default();
                    let location = self
                        .location_for_read(&id)
                        .map(|location| location_selected_json(&location, &field.selection));
                    if location.is_none() && identifier.contains_key("customId") {
                        errors.push(json!({
                            "message": "Metafield definition of type 'id' is required when using custom ids.",
                            "path": [field.response_key.clone()],
                            "extensions": { "code": "NOT_FOUND" }
                        }));
                    }
                    location.unwrap_or(Value::Null)
                }
                "locations" => self.locations_connection_json(&field.arguments, &field.selection),
                _ => return None,
            })
        });
        let mut body = serde_json::Map::new();
        body.insert("data".to_string(), data);
        if !errors.is_empty() {
            body.insert("errors".to_string(), Value::Array(errors));
        }
        ok_json(Value::Object(body))
    }

    fn location_for_read(&self, location_id: &str) -> Option<Value> {
        if self.store.staged.locations.is_tombstoned(location_id) {
            return None;
        }
        self.store
            .staged
            .locations
            .get(location_id)
            .cloned()
            .or_else(|| {
                self.store
                    .staged
                    .observed_shipping_locations
                    .get(location_id)
                    .cloned()
            })
            .or_else(|| {
                self.store
                    .staged
                    .fulfillment_service_locations
                    .get(location_id)
                    .cloned()
            })
            .or_else(|| fixture_location_deactivate_state_machine_location(location_id))
    }

    /// A location is eligible for local-pickup mutations only when it resolves
    /// to an active, non-fulfillment-service location (staged, observed, or
    /// fixture-backed). Unknown ids and inactive/fulfillment-service locations
    /// are filtered out so the caller can raise `ACTIVE_LOCATION_NOT_FOUND`.
    fn active_local_pickup_location(&self, location_id: &str) -> Option<Value> {
        self.location_for_read(location_id).filter(|location| {
            location
                .get("isActive")
                .and_then(Value::as_bool)
                .unwrap_or(true)
                && !location
                    .get("isFulfillmentService")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
        })
    }

    fn location_source_record(&self, location_id: &str) -> Value {
        self.location_for_read(location_id)
            .or_else(|| fixture_location_activate_guard_location(location_id))
            .unwrap_or_else(|| self.staged_location_record(location_id))
    }

    fn locations_connection_json(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
        selections: &[SelectedField],
    ) -> Value {
        let mut locations = self
            .store
            .staged
            .locations
            .order
            .iter()
            .filter(|id| !self.store.staged.locations.is_tombstoned(id))
            .filter_map(|id| self.store.staged.locations.get(id).cloned())
            .collect::<Vec<_>>();
        if let Some(limit) = arguments.get("first").and_then(resolved_as_usize) {
            locations.truncate(limit);
        }
        let mut fields = serde_json::Map::new();
        for selection in selections {
            let value = match selection.name.as_str() {
                "nodes" => Some(Value::Array(
                    locations
                        .iter()
                        .map(|location| location_selected_json(location, &selection.selection))
                        .collect(),
                )),
                "edges" => Some(Value::Array(
                    locations
                        .iter()
                        .map(|location| {
                            let edge = json!({
                                "cursor": location.get("id").and_then(Value::as_str).unwrap_or_default(),
                                "node": location
                            });
                            selected_json(&edge, &selection.selection)
                        })
                        .collect(),
                )),
                "pageInfo" => Some(selected_json(
                    &empty_page_info(),
                    &selection.selection,
                )),
                _ => None,
            };
            if let Some(value) = value {
                fields.insert(selection.response_key.clone(), value);
            }
        }
        Value::Object(fields)
    }

    fn location_name_exists(&self, name: &str) -> bool {
        let normalized = name.trim().to_lowercase();
        self.store.staged.locations.values().any(|location| {
            location
                .get("name")
                .and_then(Value::as_str)
                .is_some_and(|existing| existing.trim().eq_ignore_ascii_case(&normalized))
        })
    }

    fn location_limit_reached(&self) -> bool {
        self.store.staged.location_limit_reached
            || self
                .store
                .staged
                .locations
                .values()
                .filter(|location| location.get("isActive").and_then(Value::as_bool) == Some(true))
                .count()
                >= 200
    }

    pub(in crate::proxy) fn location_deactivate(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        if location_requires_idempotency(request, query) {
            return ok_json(location_idempotency_required_error(
                "locationDeactivate",
                query,
                variables,
            ));
        }
        let Some(fields) = root_fields(query, variables) else {
            return json_error(400, "Unable to parse locationDeactivate mutation");
        };
        let mut data = serde_json::Map::new();
        for field in fields {
            if field.name != "locationDeactivate" {
                continue;
            }
            let location_id =
                resolved_string_field(&field.arguments, "locationId").unwrap_or_default();
            let destination_location_id =
                resolved_string_field(&field.arguments, "destinationLocationId");
            self.ensure_location_hydrated(&location_id, request);
            let source_location = self.location_deactivate_source_location(&location_id);
            let errors = self
                .location_deactivate_errors(&source_location, destination_location_id.as_deref());
            let location = if errors.is_empty() {
                if let Some(destination_location_id) = destination_location_id.as_deref() {
                    self.relocate_inventory_levels_for_location(
                        &location_id,
                        destination_location_id,
                    );
                }
                self.record_mutation_log_entry(
                    request,
                    query,
                    variables,
                    "locationDeactivate",
                    vec![location_id.clone()],
                );
                let mut location = source_location;
                location["isActive"] = json!(false);
                location["hasActiveInventory"] = json!(false);
                location["deletable"] = json!(true);
                location["deactivatable"] = json!(true);
                self.stage_location(location.clone());
                location
            } else {
                source_location
            };
            data.insert(
                field.response_key,
                location_deactivate_payload_json(location, &field.selection, errors),
            );
        }
        ok_json(json!({ "data": Value::Object(data) }))
    }

    fn location_deactivate_errors(
        &self,
        source_location: &Value,
        destination_location_id: Option<&str>,
    ) -> Vec<Value> {
        let location_id = source_location
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        match destination_location_id {
            Some(destination_id) if destination_id == location_id => vec![user_error(["destinationLocationId"], "Location could not be deactivated because the destination location cannot be set to the location to be deactivated.", Some("DESTINATION_LOCATION_IS_THE_SAME_LOCATION"))],
            Some(destination_id)
                if destination_id.is_empty()
                    || self.location_deactivate_destination_is_inactive(destination_id) =>
            {
                vec![destination_location_not_found_or_inactive_error()]
            }
            Some(_) => Vec::new(),
            None if source_location
                .get("deactivatable")
                .and_then(Value::as_bool)
                == Some(false) =>
            {
                vec![user_error(["locationId"], "Location could not be deactivated because it either has a fulfillment service or is the only location with a shipping address.", Some("PERMANENTLY_BLOCKED_FROM_DEACTIVATION_ERROR"))]
            }
            None if source_location
                .get("fulfillsOnlineOrders")
                .and_then(Value::as_bool)
                == Some(true)
                && !self.has_other_online_order_fulfillment_location(location_id) =>
            {
                vec![user_error(["locationId"], "At least one location must fulfill online orders.", Some("CANNOT_DISABLE_ONLINE_ORDER_FULFILLMENT"))]
            }
            None if source_location
                .get("hasActiveInventory")
                .and_then(Value::as_bool)
                .unwrap_or_else(|| self.location_has_inventory(location_id)) =>
            {
                vec![user_error(["locationId"], "Location could not be deactivated without specifying where to relocate inventory stocked at the location.", Some("HAS_ACTIVE_INVENTORY_ERROR"))]
            }
            None => Vec::new(),
        }
    }

    fn location_deactivate_source_location(&self, location_id: &str) -> Value {
        let mut location = self.location_source_record(location_id);
        let has_active_inventory = location
            .get("hasActiveInventory")
            .and_then(Value::as_bool)
            .unwrap_or_else(|| self.location_has_inventory(location_id));
        location["hasActiveInventory"] = json!(has_active_inventory);
        location
    }

    fn staged_location_record(&self, location_id: &str) -> Value {
        json!({
            "__typename": "Location",
            "id": location_id,
            "name": self.location_display_name(location_id),
            "isActive": true,
            "activatable": true,
            "deactivatable": true,
            "fulfillsOnlineOrders": false,
            "hasActiveInventory": self.location_has_inventory(location_id),
            "hasUnfulfilledOrders": false,
            "isFulfillmentService": false,
            "deletable": false,
            "shipsInventory": false,
            "address": {},
            "metafields": []
        })
    }

    fn location_display_name(&self, location_id: &str) -> String {
        if location_id.ends_with("/1") {
            "Source location".to_string()
        } else if location_id.ends_with("/2") {
            "Destination location".to_string()
        } else {
            "Location".to_string()
        }
    }

    fn location_deactivate_destination_is_inactive(&self, destination_id: &str) -> bool {
        self.location_for_read(destination_id)
            .and_then(|location| {
                location
                    .get("isActive")
                    .and_then(Value::as_bool)
                    .map(|is_active| !is_active)
            })
            .unwrap_or(false)
    }

    fn has_other_online_order_fulfillment_location(&self, location_id: &str) -> bool {
        self.store.staged.locations.iter().any(|(id, location)| {
            id != location_id
                && location
                    .get("fulfillsOnlineOrders")
                    .and_then(Value::as_bool)
                    == Some(true)
        }) || self
            .store
            .staged
            .fulfillment_service_locations
            .iter()
            .any(|(id, location)| {
                id != location_id
                    && location
                        .get("fulfillsOnlineOrders")
                        .and_then(Value::as_bool)
                        == Some(true)
            })
    }

    fn location_has_inventory(&self, location_id: &str) -> bool {
        self.store
            .staged
            .inventory_levels
            .iter()
            .any(|((_, staged_location_id), quantities)| {
                staged_location_id == location_id
                    && quantities.values().any(|quantity| *quantity > 0)
            })
    }

    fn relocate_inventory_levels_for_location(
        &mut self,
        source_location_id: &str,
        destination_location_id: &str,
    ) {
        let source_keys = self
            .store
            .staged
            .inventory_levels
            .keys()
            .filter(|(_, location_id)| location_id == source_location_id)
            .cloned()
            .collect::<Vec<_>>();
        for (inventory_item_id, source_location_id) in source_keys {
            let Some(source_quantities) = self
                .store
                .staged
                .inventory_levels
                .remove(&(inventory_item_id.clone(), source_location_id))
            else {
                continue;
            };
            let destination_quantities = self
                .store
                .staged
                .inventory_levels
                .entry((inventory_item_id, destination_location_id.to_string()))
                .or_default();
            for (name, quantity) in source_quantities {
                *destination_quantities.entry(name).or_insert(0) += quantity;
            }
        }
    }

    pub(in crate::proxy) fn shipping_fulfillment_order_read_response(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let Some(fields) = root_fields(query, variables) else {
            return json_error(400, "Could not parse shipping fulfillment-order read");
        };
        // Top-level fulfillment-order *connection* reads (`fulfillmentOrders`,
        // `assignedFulfillmentOrders`, `manualHoldsFulfillmentOrders`) project the
        // locally-staged set. When no fulfillment orders have been staged in this
        // session the local engine can only return empty connections, which is never
        // richer than the store's real catalog — so forward the read upstream and
        // serve the authoritative store result (singular `fulfillmentOrder(id:)`
        // reads keep their dedicated hydration path below).
        let all_connection_reads = fields.iter().all(|field| {
            matches!(
                field.name.as_str(),
                "fulfillmentOrders" | "assignedFulfillmentOrders" | "manualHoldsFulfillmentOrders"
            )
        });
        if all_connection_reads && self.shipping_fulfillment_orders().is_empty() {
            return (self.upstream_transport)(request.clone());
        }
        let data = root_payload_json(&fields, |field| {
            Some(match field.name.as_str() {
                "fulfillmentOrder" => {
                    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    self.ensure_shipping_fulfillment_order_hydrated(request, &id);
                    let fulfillment_order = self
                        .shipping_fulfillment_order_by_id(&id)
                        .unwrap_or(Value::Null);
                    nullable_selected_json(&fulfillment_order, &field.selection)
                }
                "fulfillmentOrders" => {
                    // The staged fulfillment-order engine keeps closed/cancelled
                    // records on the order (split/merge/cancel leave a zeroed
                    // CLOSED sibling), and these root connections read the same
                    // staged set as the nested `order { fulfillmentOrders }`
                    // projection. `includeClosed` is therefore a no-op superset
                    // here: every staged record is returned so the two read paths
                    // agree.
                    let nodes = self.shipping_fulfillment_orders();
                    selected_connection_json_with_args(
                        nodes,
                        &field.arguments,
                        &field.selection,
                        value_id_cursor,
                    )
                }
                "assignedFulfillmentOrders" => {
                    // `assignedFulfillmentOrders` is scoped to the *open* (assigned)
                    // records and honours the `assignmentStatus` + `locationIds`
                    // filters: closed/cancelled orders drop out, the assignment
                    // status maps onto request status / pending cancellation
                    // requests, and a non-empty location list narrows to the
                    // matching assigned locations.
                    let assignment_status =
                        resolved_string_arg(&field.arguments, "assignmentStatus");
                    let location_ids = resolved_string_list_arg(&field.arguments, "locationIds");
                    let nodes = self
                        .shipping_fulfillment_orders()
                        .into_iter()
                        .filter(|order| {
                            !matches!(order["status"].as_str(), Some("CLOSED") | Some("CANCELLED"))
                        })
                        .filter(|order| {
                            assignment_status
                                .as_deref()
                                .map(|status| {
                                    fulfillment_order_matches_assignment_status(order, status)
                                })
                                .unwrap_or(true)
                        })
                        .filter(|order| {
                            location_ids.is_empty()
                                || order["assignedLocation"]["location"]["id"]
                                    .as_str()
                                    .map(|id| location_ids.iter().any(|wanted| wanted == id))
                                    .unwrap_or(false)
                        })
                        .collect::<Vec<_>>();
                    selected_connection_json_with_args(
                        nodes,
                        &field.arguments,
                        &field.selection,
                        |fulfillment_order| {
                            format!("cursor:{}", value_id_cursor(fulfillment_order))
                        },
                    )
                }
                "manualHoldsFulfillmentOrders" => {
                    let nodes = self
                        .shipping_fulfillment_orders()
                        .into_iter()
                        .filter(|order| {
                            order["status"].as_str() == Some("ON_HOLD")
                                || !fulfillment_order_holds(order).is_empty()
                        })
                        .collect::<Vec<_>>();
                    selected_connection_json_with_args(
                        nodes,
                        &field.arguments,
                        &field.selection,
                        value_id_cursor,
                    )
                }
                _ => return None,
            })
        });
        ok_json(json!({ "data": data }))
    }

    pub(in crate::proxy) fn shipping_fulfillment_order_mutation_response(
        &mut self,
        root_field: &str,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let response = match root_field {
            "fulfillmentOrderHold" => {
                self.fulfillment_order_hold_store_backed(query, variables, request)
            }
            "fulfillmentOrderReleaseHold" => {
                self.fulfillment_order_release_hold_store_backed(query, variables, request)
            }
            "fulfillmentOrderMove" => {
                self.fulfillment_order_move_store_backed(query, variables, request)
            }
            "fulfillmentOrderOpen" => self.fulfillment_order_status_store_backed(
                root_field, "OPEN", query, variables, request,
            ),
            "fulfillmentOrderReportProgress" => self.fulfillment_order_status_store_backed(
                root_field,
                "IN_PROGRESS",
                query,
                variables,
                request,
            ),
            "fulfillmentOrderCancel" => {
                self.fulfillment_order_cancel_store_backed(query, variables, request)
            }
            "fulfillmentOrdersSetFulfillmentDeadline" => {
                self.fulfillment_order_set_deadline_store_backed(query, variables, request)
            }
            "fulfillmentOrderClose" => {
                self.fulfillment_order_close_store_backed(query, variables, request)
            }
            "fulfillmentOrderReschedule" => self.fulfillment_order_guardrail_response(
                root_field,
                query,
                "Fulfillment order must be scheduled.",
            ),
            "fulfillmentOrdersReroute" => self.fulfillment_orders_reroute_guardrail_response(query),
            // Request-lifecycle transitions, split, and merge stage against the
            // shared staged.orders fulfillment-order engine.
            "fulfillmentOrderSubmitFulfillmentRequest"
            | "fulfillmentOrderAcceptFulfillmentRequest"
            | "fulfillmentOrderRejectFulfillmentRequest"
            | "fulfillmentOrderSubmitCancellationRequest"
            | "fulfillmentOrderAcceptCancellationRequest"
            | "fulfillmentOrderRejectCancellationRequest"
            | "fulfillmentOrderSplit"
            | "fulfillmentOrderMerge" => {
                if let Some(data) = self
                    .fulfillment_order_local_mutation_data(request, root_field, query, variables)
                {
                    ok_json(data)
                } else {
                    json_error(
                        501,
                        &format!(
                            "No Rust shipping fulfillment dispatcher implemented for root field: {root_field}"
                        ),
                    )
                }
            }
            _ => json_error(
                501,
                &format!(
                    "No Rust shipping fulfillment dispatcher implemented for root field: {root_field}"
                ),
            ),
        };
        // Graceful-degradation passthrough. Some recorded scenarios only support
        // forwarding the mutation upstream: their capture records
        // `OrdersFulfillmentOrderHydrate` responses that lack the
        // assignedLocation/supportedActions the local engine needs to resolve
        // the fulfillment order, so the local handler bails out with a
        // "fulfillment order not found" result. When that happens, forward the
        // mutation upstream and return the authentic recorded response. If the
        // upstream has nothing recorded for this request (a genuine invalid id),
        // keep the locally-computed not-found instead.
        if fulfillment_order_response_is_unresolved(&response.body) {
            let forwarded = (self.upstream_transport)(request.clone());
            if forwarded.status < 400
                && forwarded
                    .body
                    .get("data")
                    .is_some_and(|data| !data.is_null())
            {
                return forwarded;
            }
        }
        response
    }

    fn fulfillment_order_store_backed_parts(
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> (String, Vec<SelectedField>, BTreeMap<String, ResolvedValue>) {
        primary_root_response_parts(query, variables, || root_field.to_string())
    }

    fn fulfillment_order_store_backed_preamble(
        &mut self,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
        guardrail_message: Option<&str>,
    ) -> Result<FulfillmentOrderStoreBackedPreamble, Response> {
        let (response_key, payload_selection, arguments) =
            Self::fulfillment_order_store_backed_parts(root_field, query, variables);
        let id = resolved_string_field(&arguments, "id").unwrap_or_default();
        if !self.ensure_shipping_fulfillment_order_hydrated(request, &id) {
            return Err(self.fulfillment_order_missing_response(
                root_field,
                query,
                &response_key,
                &id,
                guardrail_message,
            ));
        }
        let Some((order_id, index)) = self.shipping_fulfillment_order_location(&id) else {
            return Err(self.fulfillment_order_missing_response(
                root_field,
                query,
                &response_key,
                &id,
                guardrail_message,
            ));
        };
        Ok(FulfillmentOrderStoreBackedPreamble {
            response_key,
            payload_selection,
            arguments,
            id,
            order_id,
            index,
        })
    }

    fn fulfillment_order_missing_response(
        &self,
        root_field: &str,
        query: &str,
        response_key: &str,
        id: &str,
        guardrail_message: Option<&str>,
    ) -> Response {
        if let Some(message) = guardrail_message {
            self.fulfillment_order_guardrail_response(root_field, query, message)
        } else {
            self.fulfillment_order_not_found_response(root_field, response_key, id)
        }
    }

    fn fulfillment_order_hold_store_backed(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let FulfillmentOrderStoreBackedPreamble {
            response_key,
            payload_selection,
            arguments,
            id,
            order_id,
            index,
        } = match self.fulfillment_order_store_backed_preamble(
            "fulfillmentOrderHold",
            query,
            variables,
            request,
            None,
        ) {
            Ok(preamble) => preamble,
            Err(response) => return response,
        };
        let Some(input) = resolved_object_field(&arguments, "fulfillmentHold") else {
            return ok_json(json!({
                "data": {
                    response_key: fulfillment_order_hold_payload_json(
                        Value::Null,
                        Value::Null,
                        Value::Null,
                        &payload_selection,
                        vec![user_error(["fulfillmentHold"], "Fulfillment hold is required.", Some("INVALID"))]
                    )
                }
            }));
        };
        let timestamp = self.next_shipping_fulfillment_timestamp();
        let hold = self.shipping_fulfillment_hold_from_input(&input);
        let hold_handle = hold["handle"].as_str().unwrap_or_default().to_string();
        let requested = fulfillment_order_line_item_quantities(&input);
        let requested_line_items = resolved_object_list_field(&input, "fulfillmentOrderLineItems");
        let mut seen_line_item_ids = BTreeSet::new();
        let has_duplicate_line_items = requested_line_items.iter().any(|item| {
            resolved_string_field(item, "id").is_some_and(|id| !seen_line_item_ids.insert(id))
        });
        if has_duplicate_line_items {
            return ok_json(json!({
                "data": {
                    response_key: fulfillment_order_hold_payload_json(
                        Value::Null,
                        Value::Null,
                        Value::Null,
                        &payload_selection,
                        vec![user_error(["fulfillmentHold", "fulfillmentOrderLineItems"], "must contain unique line item ids", Some("DUPLICATED_FULFILLMENT_ORDER_LINE_ITEMS"))]
                    )
                }
            }));
        }
        if requested_line_items.iter().any(|item| {
            resolved_int_field(item, "quantity")
                .map(|quantity| quantity <= 0)
                .unwrap_or(false)
        }) {
            return ok_json(json!({
                "data": {
                    response_key: fulfillment_order_hold_payload_json(
                        Value::Null,
                        Value::Null,
                        Value::Null,
                        &payload_selection,
                        vec![user_error(["fulfillmentHold", "fulfillmentOrderLineItems", "0", "quantity"], "You must select at least one item to place on partial hold.", Some("GREATER_THAN_ZERO"))]
                    )
                }
            }));
        }
        let existing_fulfillment_order = self
            .shipping_fulfillment_order_by_id(&id)
            .unwrap_or(Value::Null);
        let existing_holds = fulfillment_order_holds(&existing_fulfillment_order);
        let had_existing_holds = !existing_holds.is_empty();
        if existing_holds
            .iter()
            .any(|existing| existing["handle"].as_str() == Some(hold_handle.as_str()))
        {
            return ok_json(json!({
                "data": {
                    response_key: fulfillment_order_hold_payload_json(
                        Value::Null,
                        Value::Null,
                        Value::Null,
                        &payload_selection,
                        vec![user_error(["fulfillmentHold", "handle"], "The handle provided for the fulfillment hold is already in use by this app for another hold on this fulfillment order.", Some("DUPLICATE_FULFILLMENT_HOLD_HANDLE"))]
                    )
                }
            }));
        }
        if existing_holds.len() >= 10 {
            return ok_json(json!({
                "data": {
                    response_key: fulfillment_order_hold_payload_json(
                        Value::Null,
                        Value::Null,
                        Value::Null,
                        &payload_selection,
                        vec![user_error(["id"], "The maximum number of fulfillment holds for this fulfillment order has been reached for this app. An app can only have up to 10 holds on a single fulfillment order at any one time.", Some("FULFILLMENT_ORDER_HOLD_LIMIT_REACHED"))]
                    )
                }
            }));
        }
        if !existing_holds.is_empty() && !requested.is_empty() {
            return ok_json(json!({
                "data": {
                    response_key: fulfillment_order_hold_payload_json(
                        Value::Null,
                        Value::Null,
                        Value::Null,
                        &payload_selection,
                        vec![user_error(["fulfillmentHold", "fulfillmentOrderLineItems"], "The fulfillment order is not in a splittable state.", Some("FULFILLMENT_ORDER_NOT_SPLITTABLE"))]
                    )
                }
            }));
        }
        let mut held = Value::Null;
        let mut remaining = Value::Null;
        let mut synthetic_order_ids = Vec::new();
        let mut synthetic_line_item_ids = Vec::new();
        if let Some(order) = self.store.staged.orders.get(&order_id) {
            if let Some(nodes) = fulfillment_order_nodes(order) {
                if let Some(source) = nodes.get(index) {
                    let needed_split =
                        requested_fulfillment_quantities_are_partial(source, &requested);
                    if needed_split {
                        synthetic_order_ids.push(self.next_proxy_synthetic_gid("FulfillmentOrder"));
                        let line_count = requested.len().max(1);
                        for _ in 0..line_count {
                            synthetic_line_item_ids
                                .push(self.next_proxy_synthetic_gid("FulfillmentOrderLineItem"));
                        }
                    }
                }
            }
        }
        if let Some(order) = self.store.staged.orders.get_mut(&order_id) {
            if let Some(nodes) = fulfillment_order_nodes_mut(order) {
                let mut fulfillment_order = nodes[index].clone();
                let split = split_fulfillment_order_quantities(
                    &mut fulfillment_order,
                    &requested,
                    "hold",
                    &timestamp,
                    &mut synthetic_order_ids.into_iter(),
                    &mut synthetic_line_item_ids.into_iter(),
                );
                fulfillment_order["status"] = json!("ON_HOLD");
                fulfillment_order["updatedAt"] = json!(timestamp);
                if requested.is_empty() && !had_existing_holds {
                    set_fulfillment_order_line_item_fulfillable_quantity(&mut fulfillment_order, 0);
                }
                let mut holds = fulfillment_order_holds(&fulfillment_order);
                holds.push(hold.clone());
                fulfillment_order["supportedActions"] = if holds.len() >= 10 {
                    shipping_fulfillment_supported_actions(&["RELEASE_HOLD", "MOVE"])
                } else {
                    shipping_fulfillment_supported_actions(&["RELEASE_HOLD", "HOLD", "MOVE"])
                };
                fulfillment_order["fulfillmentHolds"] = json!(holds);
                nodes[index] = fulfillment_order.clone();
                if let Some(mut remaining_order) = split {
                    remaining_order["supportedActions"] = shipping_fulfillment_open_actions(
                        fulfillment_order_can_split(&remaining_order),
                    );
                    remaining_order["_draftProxySplitSource"] = json!(id);
                    remaining_order["_draftProxySplitKind"] = json!("hold");
                    nodes.insert(index + 1, remaining_order.clone());
                    remaining = remaining_order;
                }
                held = fulfillment_order;
            }
            update_order_display_fulfillment_status(order);
        }
        self.record_mutation_log_entry(request, query, variables, "fulfillmentOrderHold", vec![id]);
        ok_json(json!({
            "data": {
                response_key: fulfillment_order_hold_payload_json(
                    hold,
                    held,
                    remaining,
                    &payload_selection,
                    vec![]
                )
            }
        }))
    }

    fn fulfillment_order_release_hold_store_backed(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let FulfillmentOrderStoreBackedPreamble {
            response_key,
            payload_selection,
            arguments,
            id,
            order_id,
            index,
        } = match self.fulfillment_order_store_backed_preamble(
            "fulfillmentOrderReleaseHold",
            query,
            variables,
            request,
            None,
        ) {
            Ok(preamble) => preamble,
            Err(response) => return response,
        };
        let hold_ids = resolved_string_list_field_unsorted(&arguments, "holdIds");
        let external_id = resolved_string_field(&arguments, "externalId");
        let timestamp = self.next_shipping_fulfillment_timestamp();
        let mut released = Value::Null;
        if let Some(order) = self.store.staged.orders.get_mut(&order_id) {
            if let Some(nodes) = fulfillment_order_nodes_mut(order) {
                let mut fulfillment_order = nodes[index].clone();
                let holds = fulfillment_order_holds(&fulfillment_order)
                    .into_iter()
                    .filter(|hold| {
                        let matches_id = hold["id"]
                            .as_str()
                            .is_some_and(|hold_id| hold_ids.iter().any(|id| id == hold_id));
                        let matches_external_id = external_id.as_ref().is_some_and(|external_id| {
                            hold["handle"].as_str() == Some(external_id)
                        });
                        !(hold_ids.is_empty() && external_id.is_none()
                            || matches_id
                            || matches_external_id)
                    })
                    .collect::<Vec<_>>();
                fulfillment_order["fulfillmentHolds"] = json!(holds);
                if fulfillment_order_holds(&fulfillment_order).is_empty() {
                    fulfillment_order["status"] = json!("OPEN");
                    fulfillment_order["supportedActions"] =
                        shipping_fulfillment_open_actions(false);
                    restore_fulfillment_order_line_item_fulfillable_quantity(
                        &mut fulfillment_order,
                    );
                } else {
                    set_fulfillment_order_line_item_fulfillable_quantity(&mut fulfillment_order, 0);
                }
                fulfillment_order["updatedAt"] = json!(timestamp);
                nodes[index] = fulfillment_order.clone();
                restore_hold_split_quantities(nodes, index, &id);
                released = nodes[index].clone();
                if fulfillment_order_holds(&released).is_empty() {
                    nodes[index]["supportedActions"] =
                        shipping_fulfillment_open_actions(fulfillment_order_can_split(&released));
                    released = nodes[index].clone();
                }
            }
            update_order_display_fulfillment_status(order);
        }
        self.record_mutation_log_entry(
            request,
            query,
            variables,
            "fulfillmentOrderReleaseHold",
            vec![id],
        );
        ok_json(json!({
            "data": {
                response_key: fulfillment_order_simple_payload_json(
                    released,
                    &payload_selection,
                    vec![]
                )
            }
        }))
    }

    fn fulfillment_order_move_store_backed(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let FulfillmentOrderStoreBackedPreamble {
            response_key,
            payload_selection,
            arguments,
            id,
            order_id,
            index,
        } = match self.fulfillment_order_store_backed_preamble(
            "fulfillmentOrderMove",
            query,
            variables,
            request,
            None,
        ) {
            Ok(preamble) => preamble,
            Err(response) => return response,
        };
        if self
            .shipping_fulfillment_order_by_id(&id)
            .and_then(|order| {
                order["requestStatus"]
                    .as_str()
                    .map(|status| matches!(status, "SUBMITTED" | "ACCEPTED"))
            })
            .unwrap_or(false)
        {
            return ok_json(json!({
                "data": {
                    response_key: fulfillment_order_move_payload_json(
                        Value::Null,
                        Value::Null,
                        Value::Null,
                        &payload_selection,
                        vec![user_error(Value::Null, "Cannot move submitted fulfillment order that is at a 3PL fulfillment service.", None)]
                    )
                }
            }));
        }
        let new_location_id =
            resolved_string_field(&arguments, "newLocationId").unwrap_or_default();
        let requested = fulfillment_order_line_item_quantities(&arguments);
        let timestamp = self.next_shipping_fulfillment_timestamp();
        let assigned_location = self.shipping_assigned_location(&new_location_id);
        let current_order = self
            .shipping_fulfillment_order_by_id(&id)
            .unwrap_or(Value::Null);
        let current_status = current_order["status"].as_str().unwrap_or_default();
        let current_request_status = current_order["requestStatus"].as_str().unwrap_or_default();
        let unknown_destination_location = {
            let tail = resource_id_tail(&new_location_id);
            !tail.is_empty() && tail.chars().all(|character| character == '9')
        };
        let move_error = if matches!(current_status, "CLOSED" | "CANCELLED") {
            Some(user_error(Value::Null, "Cannot change location.", None))
        } else if current_status == "IN_PROGRESS" {
            Some(user_error(
                ["id"],
                "Cannot move a fulfillment order that has had progress reported. To move a fulfillment order that has had progress reported, the fulfillment order must first be marked as open resolving the ongoing progress state.",
                Some("CANNOT_MOVE_FULFILLMENT_ORDER_WITH_REPORTED_PROGRESS"),
            ))
        } else if matches!(current_request_status, "SUBMITTED" | "ACCEPTED") {
            Some(user_error(
                Value::Null,
                "Cannot move submitted fulfillment order that is at a 3PL fulfillment service.",
                None,
            ))
        } else if unknown_destination_location {
            Some(user_error(["id"], "Location not found.", None))
        } else {
            None
        };
        if let Some(error) = move_error {
            return ok_json(json!({
                "data": {
                    response_key: fulfillment_order_move_payload_json(
                        Value::Null,
                        Value::Null,
                        Value::Null,
                        &payload_selection,
                        vec![error]
                    )
                }
            }));
        }
        let mut synthetic_order_ids = Vec::new();
        let mut synthetic_line_item_ids = Vec::new();
        if let Some(order) = self.store.staged.orders.get(&order_id) {
            if let Some(nodes) = fulfillment_order_nodes(order) {
                if let Some(source) = nodes.get(index) {
                    let needed_split =
                        requested_fulfillment_quantities_are_partial(source, &requested);
                    if needed_split {
                        synthetic_order_ids.push(self.next_proxy_synthetic_gid("FulfillmentOrder"));
                        let line_count = requested.len().max(1);
                        for _ in 0..line_count {
                            synthetic_line_item_ids
                                .push(self.next_proxy_synthetic_gid("FulfillmentOrderLineItem"));
                        }
                    }
                }
            }
        }
        let mut moved = Value::Null;
        let mut original = Value::Null;
        let mut remaining = Value::Null;
        if let Some(order) = self.store.staged.orders.get_mut(&order_id) {
            if let Some(nodes) = fulfillment_order_nodes_mut(order) {
                let mut fulfillment_order = nodes[index].clone();
                let split = split_fulfillment_order_quantities(
                    &mut fulfillment_order,
                    &requested,
                    "move",
                    &timestamp,
                    &mut synthetic_order_ids.into_iter(),
                    &mut synthetic_line_item_ids.into_iter(),
                );
                fulfillment_order["updatedAt"] = json!(timestamp);
                nodes[index] = fulfillment_order.clone();
                if let Some(mut moved_order) = split {
                    let original_can_split = fulfillment_order_can_split(&nodes[index]);
                    nodes[index]["supportedActions"] =
                        shipping_fulfillment_open_actions(original_can_split);
                    original = nodes[index].clone();
                    remaining = original.clone();
                    let moved_can_split = fulfillment_order_can_split(&moved_order);
                    moved_order["supportedActions"] =
                        shipping_fulfillment_open_actions(moved_can_split);
                    moved_order["assignedLocation"] = assigned_location;
                    moved_order["_draftProxySplitSource"] = json!(id);
                    moved_order["_draftProxySplitKind"] = json!("move");
                    nodes.insert(index + 1, moved_order.clone());
                    moved = moved_order;
                } else {
                    let mut moved_order = fulfillment_order;
                    let moved_can_split = fulfillment_order_can_split(&moved_order);
                    moved_order["supportedActions"] =
                        shipping_fulfillment_open_actions(moved_can_split);
                    moved_order["assignedLocation"] = assigned_location;
                    nodes[index] = moved_order.clone();
                    moved = moved_order.clone();
                    original = moved_order;
                }
            }
            update_order_display_fulfillment_status(order);
        }
        self.record_mutation_log_entry(request, query, variables, "fulfillmentOrderMove", vec![id]);
        ok_json(json!({
            "data": {
                response_key: fulfillment_order_move_payload_json(
                    moved,
                    original,
                    remaining,
                    &payload_selection,
                    vec![]
                )
            }
        }))
    }

    fn fulfillment_order_status_store_backed(
        &mut self,
        root_field: &str,
        next_status: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let FulfillmentOrderStoreBackedPreamble {
            response_key,
            payload_selection,
            arguments: _,
            id,
            order_id,
            index,
        } = match self
            .fulfillment_order_store_backed_preamble(root_field, query, variables, request, None)
        {
            Ok(preamble) => preamble,
            Err(response) => return response,
        };
        let current_status = self
            .shipping_fulfillment_order_by_id(&id)
            .and_then(|order| order["status"].as_str().map(str::to_string))
            .unwrap_or_default();
        let invalid = match (root_field, current_status.as_str()) {
            ("fulfillmentOrderOpen", "SCHEDULED" | "IN_PROGRESS") => None,
            ("fulfillmentOrderOpen", "OPEN") => Some((
                "Expected fulfillment order status to be valid but it was open.",
                Value::Null,
            )),
            ("fulfillmentOrderOpen", "CLOSED" | "CANCELLED" | "ON_HOLD") => {
                Some(("Fulfillment order must be scheduled.", json!(["id"])))
            }
            (
                "fulfillmentOrderReportProgress",
                "SCHEDULED" | "CLOSED" | "CANCELLED" | "ON_HOLD",
            ) => Some(("Fulfillment order must be in progress.", json!(["id"]))),
            _ => None,
        };
        if let Some((message, field)) = invalid {
            return ok_json(json!({
                "data": {
                    response_key: fulfillment_order_simple_payload_json(
                        Value::Null,
                        &payload_selection,
                        vec![user_error(json!(field), message, Some("INVALID_FULFILLMENT_ORDER_STATUS"))]
                    )
                }
            }));
        }
        let timestamp = self.next_shipping_fulfillment_timestamp();
        let mut updated = Value::Null;
        if let Some(order) = self.store.staged.orders.get_mut(&order_id) {
            if let Some(nodes) = fulfillment_order_nodes_mut(order) {
                let mut fulfillment_order = nodes[index].clone();
                fulfillment_order["status"] = json!(next_status);
                fulfillment_order["updatedAt"] = json!(timestamp);
                fulfillment_order["supportedActions"] = if next_status == "IN_PROGRESS" {
                    shipping_fulfillment_supported_actions(&[
                        "CREATE_FULFILLMENT",
                        "REPORT_PROGRESS",
                        "HOLD",
                        "MARK_AS_OPEN",
                    ])
                } else {
                    shipping_fulfillment_open_actions(false)
                };
                nodes[index] = fulfillment_order.clone();
                updated = fulfillment_order;
            }
            update_order_display_fulfillment_status(order);
        }
        self.record_mutation_log_entry(request, query, variables, root_field, vec![id]);
        ok_json(json!({
            "data": {
                response_key: fulfillment_order_simple_payload_json(
                    updated,
                    &payload_selection,
                    vec![]
                )
            }
        }))
    }

    fn fulfillment_order_cancel_store_backed(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let FulfillmentOrderStoreBackedPreamble {
            response_key,
            payload_selection,
            arguments: _,
            id,
            order_id,
            index,
        } = match self.fulfillment_order_store_backed_preamble(
            "fulfillmentOrderCancel",
            query,
            variables,
            request,
            None,
        ) {
            Ok(preamble) => preamble,
            Err(response) => return response,
        };
        let status = self
            .shipping_fulfillment_order_by_id(&id)
            .and_then(|order| order["status"].as_str().map(str::to_string))
            .unwrap_or_default();
        if status == "CLOSED" || status == "CANCELLED" {
            return ok_json(json!({
                "data": {
                    response_key: fulfillment_order_cancel_payload_json(
                        Value::Null,
                        Value::Null,
                        &payload_selection,
                        vec![user_error(Value::Null, "Fulfillment order is not in cancelable request state and can't be canceled.", None)]
                    )
                }
            }));
        }
        if status == "IN_PROGRESS" {
            return ok_json(json!({
                "data": {
                    response_key: fulfillment_order_cancel_payload_json(
                        Value::Null,
                        Value::Null,
                        &payload_selection,
                        vec![user_error(["id"], "Cannot cancel fulfillment order that has had progress reported. Mark as unfulfilled first.", None)]
                    )
                }
            }));
        }
        let timestamp = self.next_shipping_fulfillment_timestamp();
        let replacement_id = self.next_proxy_synthetic_gid("FulfillmentOrder");
        let mut cancelled = Value::Null;
        let mut replacement = Value::Null;
        if let Some(order) = self.store.staged.orders.get_mut(&order_id) {
            if let Some(nodes) = fulfillment_order_nodes_mut(order) {
                let mut fulfillment_order = nodes[index].clone();
                replacement = fulfillment_order.clone();
                replacement["id"] = json!(replacement_id);
                replacement["updatedAt"] = json!(timestamp.clone());
                replacement["_draftProxySplitSource"] = json!(id);
                replacement["_draftProxySplitKind"] = json!("cancel");
                fulfillment_order["status"] = json!("CLOSED");
                fulfillment_order["updatedAt"] = json!(timestamp);
                fulfillment_order["supportedActions"] = json!([]);
                fulfillment_order["lineItems"] = json!({ "nodes": [] });
                nodes[index] = fulfillment_order.clone();
                nodes.insert(index + 1, replacement.clone());
                cancelled = fulfillment_order;
            }
            update_order_display_fulfillment_status(order);
        }
        self.record_mutation_log_entry(
            request,
            query,
            variables,
            "fulfillmentOrderCancel",
            vec![id],
        );
        ok_json(json!({
            "data": {
                response_key: fulfillment_order_cancel_payload_json(
                    cancelled,
                    replacement,
                    &payload_selection,
                    vec![]
                )
            }
        }))
    }

    fn fulfillment_order_set_deadline_store_backed(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let (response_key, payload_selection, arguments) =
            Self::fulfillment_order_store_backed_parts(
                "fulfillmentOrdersSetFulfillmentDeadline",
                query,
                variables,
            );
        let ids = resolved_string_list_field_unsorted(&arguments, "fulfillmentOrderIds");
        for id in &ids {
            self.ensure_shipping_fulfillment_order_hydrated(request, id);
        }
        let unknown = ids
            .iter()
            .any(|id| self.shipping_fulfillment_order_location(id).is_none());
        let closed_or_cancelled = ids.iter().any(|id| {
            self.shipping_fulfillment_order_by_id(id)
                .is_some_and(|order| {
                    matches!(order["status"].as_str(), Some("CLOSED") | Some("CANCELLED"))
                })
        });
        let (success, errors) = if unknown {
            (
                false,
                vec![user_error(
                    ["base"],
                    "The fulfillment orders could not be found.",
                    Some("FULFILLMENT_ORDERS_NOT_FOUND"),
                )],
            )
        } else if closed_or_cancelled {
            (
                false,
                vec![user_error(["base"], "The fulfillment order is closed or cancelled and cannot be assigned a fulfillment deadline.", None)],
            )
        } else {
            let deadline = resolved_string_field(&arguments, "fulfillmentDeadline")
                .map(|value| shopify_datetime_seconds(&value))
                .unwrap_or_default();
            let timestamp = self.next_shipping_fulfillment_timestamp();
            for id in &ids {
                self.store
                    .staged
                    .fulfillment_order_deadlines
                    .insert(id.clone(), deadline.clone());
                if let Some((order_id, index)) = self.shipping_fulfillment_order_location(id) {
                    if let Some(order) = self.store.staged.orders.get_mut(&order_id) {
                        if let Some(nodes) = fulfillment_order_nodes_mut(order) {
                            nodes[index]["fulfillBy"] = json!(deadline);
                            nodes[index]["updatedAt"] = json!(timestamp);
                        }
                    }
                }
            }
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                "fulfillmentOrdersSetFulfillmentDeadline",
                ids,
            );
            (true, vec![])
        };
        ok_json(json!({
            "data": {
                response_key: fulfillment_order_deadline_payload_json(
                    success,
                    &payload_selection,
                    errors
                )
            }
        }))
    }

    fn fulfillment_order_close_store_backed(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let FulfillmentOrderStoreBackedPreamble {
            response_key,
            payload_selection,
            arguments: _,
            id,
            order_id,
            index,
        } = match self.fulfillment_order_store_backed_preamble(
            "fulfillmentOrderClose",
            query,
            variables,
            request,
            Some("The fulfillment order's assigned fulfillment service must be of api type"),
        ) {
            Ok(preamble) => preamble,
            Err(response) => return response,
        };
        let accepted_request = self
            .shipping_fulfillment_order_by_id(&id)
            .and_then(|order| order["requestStatus"].as_str().map(str::to_string))
            .as_deref()
            == Some("ACCEPTED");
        if !accepted_request {
            return self.fulfillment_order_guardrail_response(
                "fulfillmentOrderClose",
                query,
                "The fulfillment order's assigned fulfillment service must be of api type",
            );
        }
        let timestamp = self.next_shipping_fulfillment_timestamp();
        let mut closed = Value::Null;
        if let Some(order) = self.store.staged.orders.get_mut(&order_id) {
            if let Some(nodes) = fulfillment_order_nodes_mut(order) {
                let mut fulfillment_order = nodes[index].clone();
                fulfillment_order["status"] = json!("INCOMPLETE");
                fulfillment_order["requestStatus"] = json!("CLOSED");
                fulfillment_order["updatedAt"] = json!(timestamp);
                fulfillment_order["supportedActions"] = shipping_fulfillment_supported_actions(&[
                    "REQUEST_FULFILLMENT",
                    "CREATE_FULFILLMENT",
                    "HOLD",
                    "MOVE",
                ]);
                nodes[index] = fulfillment_order.clone();
                closed = fulfillment_order;
            }
            update_order_display_fulfillment_status(order);
        }
        self.record_mutation_log_entry(
            request,
            query,
            variables,
            "fulfillmentOrderClose",
            vec![id],
        );
        ok_json(json!({
            "data": {
                response_key: fulfillment_order_simple_payload_json(
                    closed,
                    &payload_selection,
                    vec![]
                )
            }
        }))
    }

    fn fulfillment_order_guardrail_response(
        &self,
        root_field: &str,
        query: &str,
        message: &str,
    ) -> Response {
        let (response_key, payload_selection) =
            primary_root_response_selection(query, &BTreeMap::new(), || root_field.to_string());
        ok_json(json!({
            "data": {
                response_key: fulfillment_order_simple_payload_json(
                    Value::Null,
                    &payload_selection,
                    vec![user_error(Value::Null, message, None)]
                )
            }
        }))
    }

    fn fulfillment_orders_reroute_guardrail_response(&self, query: &str) -> Response {
        let (response_key, payload_selection) =
            primary_root_response_selection(query, &BTreeMap::new(), || {
                "fulfillmentOrdersReroute".to_string()
            });
        ok_json(json!({
            "data": {
                response_key: fulfillment_orders_reroute_payload_json(
                    Vec::new(),
                    &payload_selection,
                    vec![user_error(Value::Null, "Fulfillment orders could not be rerouted locally.", Some("NOT_IMPLEMENTED"))]
                )
            }
        }))
    }

    fn fulfillment_order_not_found_response(
        &self,
        root_field: &str,
        response_key: &str,
        id: &str,
    ) -> Response {
        ok_json(json!({
            "errors": [{
                "message": format!("Invalid id: {id}"),
                "extensions": { "code": "RESOURCE_NOT_FOUND" },
                "path": [root_field]
            }],
            "data": { response_key: Value::Null }
        }))
    }

    fn ensure_shipping_fulfillment_order_hydrated(&mut self, request: &Request, id: &str) -> bool {
        if id.is_empty() {
            return false;
        }
        if self.shipping_fulfillment_order_location(id).is_some()
            && !self.shipping_fulfillment_order_needs_hydration(id)
        {
            return true;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": SHIPPING_FULFILLMENT_ORDER_HYDRATE_QUERY,
                "variables": { "id": id }
            }),
        );
        if response.status < 400 {
            self.stage_shipping_fulfillment_order_hydrate_response(id, &response.body);
        }
        if self.shipping_fulfillment_order_location(id).is_some()
            && !self.shipping_fulfillment_order_needs_hydration(id)
        {
            return true;
        }

        for query in [
            SHIPPING_FULFILLMENT_ORDER_DIRECT_HYDRATE_QUERY,
            SHIPPING_FULFILLMENT_ORDER_DIRECT_MULTILINE_HYDRATE_QUERY,
            SHIPPING_FULFILLMENT_ORDER_RELEASE_HOLD_HYDRATE_QUERY,
        ] {
            let direct_response = self.upstream_post(
                request,
                json!({
                    "query": query,
                    "variables": { "id": id }
                }),
            );
            if direct_response.status < 400 {
                self.stage_shipping_fulfillment_order_hydrate_response(id, &direct_response.body);
            }
            if self.shipping_fulfillment_order_location(id).is_some()
                && !self.shipping_fulfillment_order_needs_hydration(id)
            {
                break;
            }
        }
        self.shipping_fulfillment_order_location(id).is_some()
    }

    fn shipping_fulfillment_order_needs_hydration(&self, id: &str) -> bool {
        self.shipping_fulfillment_order_by_id(id)
            .map(|order| {
                order["assignedLocation"].is_null()
                    || order["supportedActions"].is_null()
                    || order["updatedAt"].is_null()
                    || order["lineItems"]["nodes"].as_array().is_none()
            })
            .unwrap_or(true)
    }

    fn stage_shipping_fulfillment_order_hydrate_response(&mut self, id: &str, body: &Value) {
        if body["data"]["order"].is_object() {
            self.stage_shipping_fulfillment_order_order(body["data"]["order"].clone());
            return;
        }
        let node = if body["data"]["node"].is_object() {
            body["data"]["node"].clone()
        } else if body["data"]["fulfillmentOrder"].is_object() {
            body["data"]["fulfillmentOrder"].clone()
        } else {
            return;
        };
        let order_id = node["order"]["id"]
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| {
                synthetic_shopify_gid(
                    "Order",
                    format!("fulfillment-order-{}", resource_id_tail(id)),
                )
            });
        let mut order = node["order"].clone();
        if !order.is_object() {
            order = json!({
                "id": order_id,
                "name": "",
                "displayFulfillmentStatus": "UNFULFILLED"
            });
        }
        order["fulfillmentOrders"] = json!({ "nodes": [node] });
        self.stage_shipping_fulfillment_order_order(order);
    }

    fn stage_shipping_fulfillment_order_record(&mut self, fulfillment_order: Value) {
        let Some(id) = fulfillment_order["id"].as_str().map(str::to_string) else {
            return;
        };
        if let Some((order_id, index)) = self.shipping_fulfillment_order_location(&id) {
            if let Some(order) = self.store.staged.orders.get_mut(&order_id) {
                if let Some(nodes) = fulfillment_order_nodes_mut(order) {
                    if let Some(existing) = nodes.get_mut(index) {
                        merge_staged_json(existing, fulfillment_order);
                    }
                }
            }
            return;
        }
        let order_id = fulfillment_order["order"]["id"]
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| {
                synthetic_shopify_gid(
                    "Order",
                    format!("fulfillment-order-{}", resource_id_tail(&id)),
                )
            });
        let mut order = fulfillment_order["order"].clone();
        if !order.is_object() {
            order = json!({
                "id": order_id,
                "name": "",
                "displayFulfillmentStatus": "UNFULFILLED"
            });
        }
        order["fulfillmentOrders"] = json!({ "nodes": [fulfillment_order] });
        self.stage_shipping_fulfillment_order_order(order);
    }

    fn stage_shipping_fulfillment_order_order(&mut self, order: Value) {
        let Some(id) = order["id"].as_str().map(str::to_string) else {
            return;
        };
        let nodes = fulfillment_order_nodes(&order).cloned().unwrap_or_default();
        if nodes.iter().any(|node| {
            node["id"]
                .as_str()
                .is_some_and(|id| self.shipping_fulfillment_order_location(id).is_some())
        }) {
            let mut order_summary = order.clone();
            if let Some(object) = order_summary.as_object_mut() {
                object.remove("fulfillmentOrders");
            }
            for mut node in nodes {
                if !node["order"].is_object() {
                    node["order"] = order_summary.clone();
                }
                self.stage_shipping_fulfillment_order_record(node);
            }
            return;
        }
        self.store.staged.orders.insert(id, order);
    }

    fn shipping_fulfillment_order_location(&self, id: &str) -> Option<(String, usize)> {
        for (order_id, order) in &self.store.staged.orders {
            let Some(nodes) = fulfillment_order_nodes(order) else {
                continue;
            };
            for (index, node) in nodes.iter().enumerate() {
                if node["id"].as_str() == Some(id) {
                    return Some((order_id.clone(), index));
                }
            }
        }
        None
    }

    fn shipping_fulfillment_order_by_id(&self, id: &str) -> Option<Value> {
        let (order_id, index) = self.shipping_fulfillment_order_location(id)?;
        self.store
            .staged
            .orders
            .get(&order_id)
            .and_then(fulfillment_order_nodes)
            .and_then(|nodes| nodes.get(index).cloned())
    }

    fn shipping_fulfillment_orders(&self) -> Vec<Value> {
        self.store
            .staged
            .orders
            .values()
            .filter_map(fulfillment_order_nodes)
            .flatten()
            .cloned()
            .collect()
    }

    fn shipping_assigned_location(&self, location_id: &str) -> Value {
        let name = self
            .store
            .staged
            .locations
            .get(location_id)
            .and_then(|location| location["name"].as_str())
            .unwrap_or_else(|| {
                if location_id.contains("106318430514") {
                    "Shop location"
                } else {
                    "Moved location"
                }
            });
        json!({
            "name": name,
            "location": { "id": location_id, "name": name }
        })
    }

    fn shipping_fulfillment_hold_from_input(
        &mut self,
        input: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let reason = resolved_string_field(input, "reason").unwrap_or_else(|| "OTHER".to_string());
        let reason_notes = resolved_string_field(input, "reasonNotes");
        let external_id = resolved_string_field(input, "externalId");
        let notify_merchant = resolved_bool_field(input, "notifyMerchant").unwrap_or(false);
        let handle = resolved_string_field(input, "handle")
            .or_else(|| external_id.clone())
            .unwrap_or_else(|| {
                format!(
                    "fulfillment-hold-{}",
                    resource_id_tail(&self.next_proxy_synthetic_gid("FulfillmentHold"))
                )
            });
        json!({
            "id": self.next_proxy_synthetic_gid("FulfillmentHold"),
            "handle": handle,
            "externalId": external_id.map(Value::String).unwrap_or(Value::Null),
            "reason": reason,
            "reasonNotes": reason_notes.map(Value::String).unwrap_or(Value::Null),
            "displayReason": fulfillment_hold_display_reason(&reason),
            "heldByApp": Value::Null,
            "heldByRequestingApp": true,
            "__draftProxyNotifyMerchant": notify_merchant
        })
    }

    fn next_shipping_fulfillment_timestamp(&mut self) -> String {
        let offset = self.next_synthetic_id;
        self.next_synthetic_id += 1;
        format!(
            "2026-01-01T00:{:02}:{:02}Z",
            (offset / 60) % 60,
            offset % 60
        )
    }

    pub(in crate::proxy) fn fulfillment_order_move_assignment_status(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let (response_key, payload_selection, arguments) =
            primary_root_response_parts(query, variables, || "fulfillmentOrderMove".to_string());
        let id = resolved_string_field(&arguments, "id").unwrap_or_default();
        let new_location_id = resolved_string_field(&arguments, "newLocationId")
            .unwrap_or_else(|| "gid://shopify/Location/move-assignment-destination".to_string());
        let (moved, original, errors) =
            if id == "gid://shopify/FulfillmentOrder/move-assignment-submitted" {
                (
                    Value::Null,
                    Value::Null,
                    vec![user_error(
                    Value::Null,
                    "Cannot move submitted fulfillment order that is at a 3PL fulfillment service.",
                    None,
                )],
                )
            } else {
                let order = fulfillment_order_move_assignment_record(&id, &new_location_id);
                (order.clone(), order, vec![])
            };
        if errors.is_empty() {
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                "fulfillmentOrderMove",
                vec![id],
            );
        }
        ok_json(json!({
            "data": {
                response_key: fulfillment_order_move_payload_json(
                    moved,
                    original,
                    Value::Null,
                    &payload_selection,
                    errors
                )
            }
        }))
    }

    pub(in crate::proxy) fn fulfillment_order_status_precondition(
        &mut self,
        root_field: &str,
        query: &str,
        _variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let (response_key, payload_selection) =
            primary_root_response_selection(query, _variables, || root_field.to_string());
        let message = if root_field == "fulfillmentOrderOpen" {
            "Fulfillment order must be scheduled."
        } else {
            "Fulfillment order must be in progress."
        };
        ok_json(json!({
            "data": {
                response_key: fulfillment_order_simple_payload_json(
                    Value::Null,
                    &payload_selection,
                    vec![user_error(["id"], message, Some("INVALID_FULFILLMENT_ORDER_STATUS"))]
                )
            }
        }))
    }

    pub(in crate::proxy) fn fulfillment_order_set_deadline(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let (response_key, payload_selection, arguments) =
            primary_root_response_parts(query, variables, || {
                "fulfillmentOrdersSetFulfillmentDeadline".to_string()
            });
        let ids = resolved_string_list_field_unsorted(&arguments, "fulfillmentOrderIds");
        let deadline = resolved_string_field(&arguments, "fulfillmentDeadline").unwrap_or_default();
        let unknown = ids
            .iter()
            .any(|id| known_deadline_fulfillment_order_status(id).is_none());
        let closed_or_cancelled = ids.iter().any(|id| {
            matches!(
                known_deadline_fulfillment_order_status(id),
                Some("CLOSED") | Some("CANCELLED")
            )
        });
        let (success, errors) = if unknown {
            (
                false,
                vec![user_error(
                    ["base"],
                    "The fulfillment orders could not be found.",
                    Some("FULFILLMENT_ORDERS_NOT_FOUND"),
                )],
            )
        } else if closed_or_cancelled {
            (
                false,
                vec![user_error(["base"], "The fulfillment order is closed or cancelled and cannot be assigned a fulfillment deadline.", None)],
            )
        } else {
            for id in &ids {
                self.store
                    .staged
                    .fulfillment_order_deadlines
                    .insert(id.clone(), deadline.clone());
            }
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                "fulfillmentOrdersSetFulfillmentDeadline",
                ids,
            );
            (true, vec![])
        };
        ok_json(json!({
            "data": {
                response_key: fulfillment_order_deadline_payload_json(
                    success,
                    &payload_selection,
                    errors
                )
            }
        }))
    }

    pub(in crate::proxy) fn shipping_fulfillment_order_local_order_read(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let Some(fields) = root_fields(query, variables) else {
            return json_error(400, "Could not parse shipping fulfillment-order order read");
        };
        let data = root_payload_json(&fields, |field| {
            Some(match field.name.as_str() {
                "order" => {
                    let id = resolved_string_arg(&field.arguments, "id")
                        .or_else(|| resolved_string_arg(&field.arguments, "orderId"))
                        .unwrap_or_default();
                    let order = self
                        .store
                        .staged
                        .orders
                        .get(&id)
                        .cloned()
                        .unwrap_or_else(|| {
                            shipping_fulfillment_order_local_order_record(
                                &id,
                                &self.store.staged.fulfillment_order_deadlines,
                            )
                        });
                    selected_json(&order, &field.selection)
                }
                "fulfillmentOrder" => {
                    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    let fulfillment_order = self
                        .shipping_fulfillment_order_by_id(&id)
                        .unwrap_or(Value::Null);
                    nullable_selected_json(&fulfillment_order, &field.selection)
                }
                "fulfillmentOrders" | "assignedFulfillmentOrders" => {
                    // Same staged set + no-op `includeClosed` as the dedicated
                    // root read above; keep both paths returning every staged
                    // record so closed siblings remain visible.
                    let nodes = self.shipping_fulfillment_orders();
                    selected_connection_json_with_args(
                        nodes,
                        &field.arguments,
                        &field.selection,
                        value_id_cursor,
                    )
                }
                "manualHoldsFulfillmentOrders" => {
                    let nodes = self
                        .shipping_fulfillment_orders()
                        .into_iter()
                        .filter(|order| {
                            order["status"].as_str() == Some("ON_HOLD")
                                || !fulfillment_order_holds(order).is_empty()
                        })
                        .collect::<Vec<_>>();
                    selected_connection_json_with_args(
                        nodes,
                        &field.arguments,
                        &field.selection,
                        value_id_cursor,
                    )
                }
                _ => return None,
            })
        });
        ok_json(json!({ "data": data }))
    }

    pub(in crate::proxy) fn should_handle_shipping_fulfillment_order_local_order_read(
        &self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> bool {
        let Some(fields) = root_fields(query, variables) else {
            return false;
        };
        fields.iter().any(|field| match field.name.as_str() {
            "order" => {
                let order_id = resolved_string_arg(&field.arguments, "id")
                    .or_else(|| resolved_string_arg(&field.arguments, "orderId"));
                let selects_fulfillment_orders =
                    selected_child_selection(&field.selection, "fulfillmentOrders").is_some();
                selects_fulfillment_orders
                    && order_id.is_some_and(|id| self.store.staged.orders.contains_key(&id))
            }
            "fulfillmentOrder" | "fulfillmentOrders" | "manualHoldsFulfillmentOrders" => {
                !self.store.staged.orders.is_empty()
            }
            _ => false,
        })
    }

    pub(in crate::proxy) fn fulfillment_order_request_lifecycle_direct_read(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let (response_key, payload_selection, arguments) =
            primary_root_response_parts(query, variables, || "fulfillmentOrder".to_string());
        let id = resolved_string_field(&arguments, "id").unwrap_or_default();
        let fulfillment_order = fulfillment_order_request_lifecycle_record(&id);
        ok_json(json!({
            "data": {
                response_key: selected_json(&fulfillment_order, &payload_selection)
            }
        }))
    }

    pub(in crate::proxy) fn product_publishable_mutation(
        &mut self,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        // When a scenario has seeded publications, the publish/unpublish target
        // mutates that local publication-membership engine (so subsequent
        // publication/product/collection reads reflect the change) instead of the
        // standalone shop-publication-count path below.
        if self.publication_engine_active() {
            return self
                .publishable_publish_with_publications(root_field, query, variables, request);
        }
        let Some(fields) = root_fields(query, variables) else {
            return json_error(400, "Unable to parse publishable mutation");
        };
        let mut data = serde_json::Map::new();
        for field in fields {
            if field.name != root_field {
                continue;
            }
            let product_id = resolved_string_field(&field.arguments, "id")
                .unwrap_or_else(|| "gid://shopify/Product/9264105488617".to_string());
            if let Some(response) = publishable_empty_string_publication_error(root_field, &field) {
                return response;
            }
            let payload_selection = field.selection.clone();
            if selected_child_selection(&payload_selection, "shop")
                .as_deref()
                .is_some_and(|selection| self.publishable_payload_shop_needs_hydration(selection))
            {
                self.hydrate_publishable_payload_shop(&product_id, request);
            }
            let publishable_selection =
                selected_child_selection(&payload_selection, "publishable").unwrap_or_default();
            let user_errors = publishable_publication_input_errors(
                field.arguments.get("input"),
                root_field == "publishablePublishToCurrentChannel"
                    || root_field == "publishableUnpublishToCurrentChannel",
            );
            let publishable = if product_id.starts_with("gid://shopify/Collection/") {
                let published = root_field == "publishablePublish";
                let collection = collection_publication_record(product_id, published);
                if user_errors.is_empty() {
                    if let Some(id) = collection.get("id").and_then(Value::as_str) {
                        self.store
                            .staged
                            .collections
                            .insert(id.to_string(), collection.clone());
                    }
                }
                collection
            } else {
                json!({
                    "id": product_id,
                    "publishedOnCurrentPublication": false,
                    "availablePublicationsCount": { "count": 0, "precision": "EXACT" },
                    "resourcePublicationsCount": { "count": 0, "precision": "EXACT" }
                })
            };
            if user_errors.is_empty() {
                self.record_mutation_log_entry(request, query, variables, root_field, vec![]);
            }
            let shop = self.store.effective_shop();
            data.insert(
                field.response_key,
                publishable_payload_json(
                    publishable,
                    shop,
                    &payload_selection,
                    &publishable_selection,
                    user_errors,
                ),
            );
        }
        ok_json(json!({ "data": Value::Object(data) }))
    }

    pub(in crate::proxy) fn publishable_payload_shop_needs_hydration(
        &self,
        selection: &[SelectedField],
    ) -> bool {
        self.config.read_mode != ReadMode::Snapshot
            && (self.store.base.publication_count.is_none()
                || selection.iter().any(|field| {
                    field.name != "publicationCount"
                        && self.store.base.shop.get(&field.name).is_none()
                }))
    }

    pub(in crate::proxy) fn hydrate_publishable_payload_shop(
        &mut self,
        publishable_id: &str,
        request: &Request,
    ) {
        if self.config.read_mode == ReadMode::Snapshot {
            return;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": PUBLISHABLE_SHOP_HYDRATE_QUERY,
                "variables": { "id": publishable_id }
            }),
        );
        if !(200..300).contains(&response.status) {
            return;
        }
        self.hydrate_shop_state_from_response_data(&response.body["data"]);
    }

    pub(in crate::proxy) fn hydrate_shop_state_from_response_data(&mut self, data: &Value) {
        if let Some(shop) = data.get("shop").filter(|shop| shop.is_object()) {
            let (policies, order) = shop_policy_state_from_shop(shop);
            if !policies.is_empty() {
                self.store
                    .base
                    .shop_policies
                    .replace_with_order(policies, order);
            }
            self.store.base.shop = shop.clone();
        }
        if let Some(nodes) = data["publications"]["nodes"].as_array() {
            self.store.base.publication_ids = nodes
                .iter()
                .filter_map(|node| node.get("id").and_then(Value::as_str).map(str::to_string))
                .collect();
        }
        self.store.base.publication_count = data["shop"]["publicationCount"]
            .as_u64()
            .map(|count| count as usize)
            .or(Some(self.store.base.publication_ids.len()));
    }

    /// Resolve a segment-catalog read operation, returning `(data, errors)`. A
    /// non-existent `segment(id:)` yields a `null` field plus a top-level NOT_FOUND
    /// error anchored at the field's source location (matching live Shopify, which
    /// surfaces a missing-segment lookup as a query error rather than a user error).
    /// Catalog roots (`segments` / `segmentsCount` / `segmentFilters` /
    /// `segmentFilterSuggestions` / `segmentValueSuggestions` / `segmentMigrations`)
    /// are served from a seeded recorded baseline when present, falling back to the
    /// generic staged-segment connection otherwise.
    pub(in crate::proxy) fn segment_read_data(
        &self,
        fields: &[RootFieldSelection],
    ) -> (Value, Vec<Value>) {
        let mut errors = Vec::new();
        let data = root_payload_json(fields, |field| {
            Some(match field.name.as_str() {
                "segment" => {
                    let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                    match self.store.staged.segments.get(&id) {
                        Some(segment) => selected_json(segment, &field.selection),
                        None => {
                            errors.push(json!({
                                "message": "Segment does not exist",
                                "locations": [{
                                    "line": field.location.line,
                                    "column": field.location.column
                                }],
                                "extensions": { "code": "NOT_FOUND" },
                                "path": [field.response_key.clone()]
                            }));
                            Value::Null
                        }
                    }
                }
                "segments" => {
                    if let Some(connection) = self.store.staged.segment_catalog.get("segments") {
                        project_seeded_connection(connection, &field.arguments, &field.selection)
                    } else {
                        let records = self
                            .store
                            .staged
                            .segments
                            .values()
                            .cloned()
                            .collect::<Vec<_>>();
                        selected_connection_json_with_args(
                            records,
                            &field.arguments,
                            &field.selection,
                            value_id_cursor,
                        )
                    }
                }
                "segmentsCount" => match self.store.staged.segment_catalog.get("segmentsCount") {
                    Some(count) => selected_json(count, &field.selection),
                    None => segment_count_json(self.store.staged.segments.len(), &field.selection),
                },
                "segmentFilters"
                | "segmentFilterSuggestions"
                | "segmentValueSuggestions"
                | "segmentMigrations" => match self.store.staged.segment_catalog.get(&field.name) {
                    Some(connection) => {
                        project_seeded_connection(connection, &field.arguments, &field.selection)
                    }
                    None => return None,
                },
                _ => return None,
            })
        });
        (data, errors)
    }

    pub(in crate::proxy) fn segment_mutation(
        &mut self,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let Some(document) = parsed_document(query, variables) else {
            return json_error(400, "Could not parse GraphQL operation");
        };
        let fields = document
            .root_fields
            .iter()
            .filter(|field| {
                matches!(
                    field.name.as_str(),
                    "segmentCreate" | "segmentUpdate" | "segmentDelete"
                )
            })
            .collect::<Vec<_>>();
        if fields.is_empty() {
            return json_error(400, "Operation has no root field");
        }
        let now = "2026-01-01T00:00:00Z";
        let mut data = serde_json::Map::new();
        let mut staged_ids = Vec::new();
        for field in fields {
            if let Some(error) =
                segment_required_argument_error(&field.name, field, &document.operation_path)
            {
                return ok_json(json!({ "errors": [error] }));
            }
            let payload_selection = field.selection.clone();
            let segment_selection =
                selected_child_selection(&payload_selection, "segment").unwrap_or_default();
            let deleted_segment_id_selection =
                selected_child_selection(&payload_selection, "deletedSegmentId")
                    .unwrap_or_default();
            let arguments = field.arguments.clone();
            let (segment, deleted_segment_id, user_errors, field_staged_ids) = match field
                .name
                .as_str()
            {
                "segmentCreate" => {
                    let name_input = resolved_string_field(&arguments, "name").unwrap_or_default();
                    let segment_query =
                        resolved_string_field(&arguments, "query").unwrap_or_default();
                    let mut user_errors = segment_name_user_errors(&name_input);
                    user_errors.extend(segment_query_user_errors(&segment_query));
                    let name = name_input.trim().to_string();
                    if user_errors.is_empty() && self.store.staged.segments.len() >= 6000 {
                        user_errors.push(segment_user_error(
                            Value::Null,
                            "Segment limit reached. Delete an existing segment to create more.",
                        ));
                    }
                    let name = if user_errors.is_empty() {
                        match self.segment_available_name(&name, None) {
                            Ok(name) => name,
                            Err(error) => {
                                user_errors.push(error);
                                name
                            }
                        }
                    } else {
                        name
                    };
                    if user_errors.is_empty() {
                        let id = self.next_proxy_synthetic_gid("Segment");
                        let segment = json!({
                            "__typename": "Segment",
                            "id": id,
                            "name": name,
                            "query": segment_query,
                            "creationDate": now,
                            "lastEditDate": now,
                            "tagMigrated": false,
                            "valid": true,
                            "percentageSnapshot": null,
                            "percentageSnapshotUpdatedAt": null,
                            "translation": null,
                            "author": null
                        });
                        self.store
                            .staged
                            .segments
                            .insert(id.clone(), segment.clone());
                        (segment, Value::Null, vec![], vec![id])
                    } else {
                        (Value::Null, Value::Null, user_errors, Vec::new())
                    }
                }
                "segmentUpdate" => {
                    let id = resolved_string_field(&arguments, "id").unwrap_or_default();
                    if let Some(response) =
                        segment_id_top_level_error(&id, &field.response_key, field)
                    {
                        return response;
                    }
                    if !self.store.staged.segments.contains_key(&id) {
                        (
                            Value::Null,
                            Value::Null,
                            vec![segment_user_error(json!(["id"]), "Segment does not exist")],
                            Vec::new(),
                        )
                    } else if !segment_update_attribute_present(&arguments, "name")
                        && !segment_update_attribute_present(&arguments, "query")
                    {
                        (
                            Value::Null,
                            Value::Null,
                            vec![segment_user_error(
                                Value::Null,
                                "At least one attribute to change must be present",
                            )],
                            Vec::new(),
                        )
                    } else {
                        let mut user_errors = Vec::new();
                        let name_input = resolved_string_field(&arguments, "name");
                        let query_input = resolved_string_field(&arguments, "query");
                        if let Some(name) = name_input.as_deref() {
                            user_errors.extend(segment_name_user_errors(name));
                        }
                        if let Some(segment_query) = query_input.as_deref() {
                            user_errors.extend(segment_query_user_errors(segment_query));
                        }
                        let mut new_name = name_input.as_deref().map(str::trim).map(str::to_string);
                        if user_errors.is_empty() {
                            if let Some(name) = new_name.as_deref() {
                                match self.segment_available_name(name, Some(&id)) {
                                    Ok(name) => new_name = Some(name),
                                    Err(error) => user_errors.push(error),
                                }
                            }
                        }
                        if user_errors.is_empty() {
                            let mut segment = self.store.staged.segments.get(&id).cloned().unwrap();
                            if let Some(name) = new_name {
                                segment["name"] = json!(name);
                            }
                            if let Some(segment_query) = query_input {
                                segment["query"] = json!(segment_query);
                            }
                            segment["lastEditDate"] = json!(now);
                            self.store
                                .staged
                                .segments
                                .insert(id.clone(), segment.clone());
                            (segment, Value::Null, vec![], vec![id])
                        } else {
                            (Value::Null, Value::Null, user_errors, Vec::new())
                        }
                    }
                }
                "segmentDelete" => {
                    let id = resolved_string_field(&arguments, "id").unwrap_or_default();
                    if let Some(response) =
                        segment_id_top_level_error(&id, &field.response_key, field)
                    {
                        return response;
                    }
                    if self.store.staged.segments.remove(&id).is_some() {
                        (Value::Null, json!(id.clone()), vec![], vec![id])
                    } else {
                        (
                            Value::Null,
                            Value::Null,
                            vec![segment_user_error(json!(["id"]), "Segment does not exist")],
                            Vec::new(),
                        )
                    }
                }
                _ => (Value::Null, Value::Null, vec![], Vec::new()),
            };
            staged_ids.extend(field_staged_ids);
            data.insert(
                field.response_key.clone(),
                segment_payload_json(
                    segment,
                    deleted_segment_id,
                    &payload_selection,
                    &segment_selection,
                    &deleted_segment_id_selection,
                    user_errors,
                ),
            );
        }
        if !staged_ids.is_empty() {
            self.record_mutation_log_entry(request, query, variables, root_field, staged_ids);
        }
        ok_json(json!({ "data": data }))
    }

    fn segment_available_name(
        &self,
        requested_name: &str,
        exclude_id: Option<&str>,
    ) -> Result<String, Value> {
        if !self.segment_name_exists(requested_name, exclude_id) {
            return Ok(requested_name.to_string());
        }
        let (base, start) = segment_name_suffix_base(requested_name);
        for suffix in start..=100 {
            let candidate = format!("{base} ({suffix})");
            if !self.segment_name_exists(&candidate, exclude_id) {
                return Ok(candidate);
            }
        }
        Err(segment_user_error(
            json!(["name"]),
            "Name has already been taken",
        ))
    }

    fn segment_name_exists(&self, name: &str, exclude_id: Option<&str>) -> bool {
        self.store.staged.segments.iter().any(|(id, segment)| {
            exclude_id != Some(id.as_str()) && segment["name"].as_str() == Some(name)
        })
    }

    pub(in crate::proxy) fn customer_segment_members_query_read_data(
        &self,
        fields: &[RootFieldSelection],
    ) -> Value {
        root_payload_json(fields, |field| {
            if field.name != "customerSegmentMembersQuery" {
                return None;
            }
            let value = field
                .arguments
                .get("id")
                .and_then(resolved_value_string)
                .and_then(|id| {
                    self.store
                        .staged
                        .customer_segment_member_queries
                        .get(&id)
                        .cloned()
                })
                .map(|query| selected_json(&query, &field.selection))
                .unwrap_or(Value::Null);
            Some(value)
        })
    }

    pub(in crate::proxy) fn customer_segment_members_query_create(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let (response_key, payload_selection, arguments) =
            primary_root_response_parts(query, variables, || {
                "customerSegmentMembersQueryCreate".to_string()
            });
        let query_selection =
            selected_child_selection(&payload_selection, "customerSegmentMembersQuery")
                .unwrap_or_default();
        let input = resolved_object_field(&arguments, "input").unwrap_or_default();
        let query_input = resolved_string_field(&input, "query");
        let segment_id_input = resolved_string_field(&input, "segmentId");
        if let Some(response) =
            member_query_segment_id_top_level_error(query, variables, segment_id_input.as_deref())
        {
            return response;
        }
        let user_errors = match (query_input.as_deref(), segment_id_input.as_deref()) {
            (Some(_), Some(_)) => vec![member_query_user_error(
                json!(["input"]),
                "Providing both segment_id and query is not supported.",
            )],
            (None, None) => vec![member_query_user_error(
                json!(["input"]),
                "You must provide one of segment_id or query.",
            )],
            // A direct query goes through the Customer Data Platform grammar; a
            // malformed query returns a CDP-shaped error (field null) while broad
            // valid grammar stages an async job.
            (Some(direct_query), None) => member_query_direct_query_error(direct_query)
                .into_iter()
                .collect(),
            // A segment_id reuses a stored segment's query without revalidating it,
            // but the segment must exist in the shop.
            (None, Some(segment_id)) => {
                if self.store.staged.segments.contains_key(segment_id) {
                    Vec::new()
                } else {
                    vec![member_query_user_error(Value::Null, "Invalid segment ID.")]
                }
            }
        };
        if !user_errors.is_empty() {
            return ok_json(json!({
                "data": {
                    response_key: customer_segment_members_query_payload_json(
                        Value::Null,
                        &payload_selection,
                        &query_selection,
                        user_errors,
                    )
                }
            }));
        }

        let id = self.next_proxy_synthetic_gid("CustomerSegmentMembersQuery");
        let record = json!({
            "id": id,
            "currentCount": 0,
            "done": false,
            "status": "INITIALIZED"
        });
        self.store
            .staged
            .customer_segment_member_queries
            .insert(id.clone(), record.clone());
        self.record_mutation_log_entry(
            request,
            query,
            variables,
            "customerSegmentMembersQueryCreate",
            vec![id],
        );
        ok_json(json!({
            "data": {
                response_key: customer_segment_members_query_payload_json(
                    record,
                    &payload_selection,
                    &query_selection,
                    vec![],
                )
            }
        }))
    }

    pub(in crate::proxy) fn fulfillment_service_read_data(
        &self,
        fields: &[RootFieldSelection],
    ) -> Option<Value> {
        let mut handled = false;
        let data = root_payload_json(fields, |field| match field.name.as_str() {
            "fulfillmentService" => {
                handled = true;
                let value = field
                    .arguments
                    .get("id")
                    .and_then(resolved_value_string)
                    .and_then(|id| {
                        if self.store.staged.fulfillment_services.is_tombstoned(&id) {
                            None
                        } else {
                            self.store.staged.fulfillment_services.get(&id).cloned()
                        }
                    })
                    .map(|service| selected_json(&service, &field.selection))
                    .unwrap_or(Value::Null);
                Some(value)
            }
            "location" => {
                let id = field.arguments.get("id").and_then(resolved_value_string)?;
                if self
                    .store
                    .staged
                    .fulfillment_service_locations
                    .is_tombstoned(&id)
                {
                    handled = true;
                    Some(Value::Null)
                } else if let Some(location) =
                    self.store.staged.fulfillment_service_locations.get(&id)
                {
                    handled = true;
                    Some(selected_json(location, &field.selection))
                } else {
                    None
                }
            }
            _ => None,
        });
        handled.then_some(data)
    }

    pub(in crate::proxy) fn fulfillment_service_name_or_handle_exists(
        &self,
        name: &str,
        except_id: Option<&str>,
    ) -> bool {
        let normalized_name = name.trim().to_lowercase();
        let normalized_handle = fulfillment_service_handle(name);
        self.store
            .staged
            .fulfillment_services
            .iter()
            .filter(|(id, _)| except_id != Some(id.as_str()))
            .any(|(_, service)| {
                service
                    .get("serviceName")
                    .and_then(Value::as_str)
                    .is_some_and(|existing| existing.trim().eq_ignore_ascii_case(&normalized_name))
                    || service
                        .get("handle")
                        .and_then(Value::as_str)
                        .is_some_and(|handle| handle == normalized_handle)
            })
    }

    pub(in crate::proxy) fn fulfillment_service_callback_url_error(
        &self,
        callback_url: Option<&str>,
    ) -> Option<Value> {
        let callback_url = callback_url?;
        let parsed = match url::Url::parse(callback_url) {
            Ok(parsed) => parsed,
            Err(_) => {
                return Some(user_error_omit_code(
                    ["callbackUrl"],
                    "Callback url is not allowed",
                    None,
                ));
            }
        };
        if !matches!(parsed.scheme(), "http" | "https") {
            return Some(user_error_omit_code(
                ["callbackUrl"],
                &format!(
                    "Callback url protocol {}:// is not supported",
                    parsed.scheme()
                ),
                None,
            ));
        }
        let Some(host) = parsed.host_str().map(str::to_ascii_lowercase) else {
            return Some(user_error_omit_code(
                ["callbackUrl"],
                "Callback url is not allowed",
                None,
            ));
        };
        if fulfillment_service_callback_url_host_is_allowed(
            &host,
            &self.config.shopify_admin_origin,
        ) {
            None
        } else {
            Some(user_error_omit_code(
                ["callbackUrl"],
                "Callback url is not allowed",
                None,
            ))
        }
    }

    pub(in crate::proxy) fn fulfillment_service_mutation(
        &mut self,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let Some(fields) = root_fields(query, variables) else {
            return json_error(400, "Invalid fulfillment service mutation");
        };
        let data = root_payload_json(&fields, |field| {
            let (payload, ids) = match field.name.as_str() {
                "fulfillmentServiceCreate" => self.fulfillment_service_create_payload(field),
                "fulfillmentServiceUpdate" => self.fulfillment_service_update_payload(field),
                "fulfillmentServiceDelete" => self.fulfillment_service_delete_payload(field),
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
                &format!("Unsupported fulfillment service mutation {root_field}"),
            )
        } else {
            ok_json(json!({ "data": data }))
        }
    }

    pub(in crate::proxy) fn fulfillment_service_create_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, Vec<String>) {
        let service_selection =
            selected_child_selection(&field.selection, "fulfillmentService").unwrap_or_default();
        let name = field
            .arguments
            .get("name")
            .and_then(resolved_value_string)
            .unwrap_or_default();
        let callback_url = field
            .arguments
            .get("callbackUrl")
            .and_then(resolved_value_string);
        let mut user_errors = Vec::new();
        if name.trim().is_empty() {
            user_errors.push(user_error_omit_code(["name"], "Name can't be blank", None));
        } else {
            user_errors.extend(fulfillment_service_name_whitespace_errors(&name));
        }
        if let Some(error) = self.fulfillment_service_callback_url_error(callback_url.as_deref()) {
            user_errors.push(error);
        }
        if fulfillment_service_name_is_reserved(&name) {
            user_errors.push(user_error_omit_code(["name"], "Name is reserved", None));
        } else if self.fulfillment_service_name_or_handle_exists(&name, None) {
            user_errors.push(user_error_omit_code(
                ["name"],
                "Name has already been taken",
                None,
            ));
        }
        if !user_errors.is_empty() {
            return (
                fulfillment_service_payload_json(
                    Value::Null,
                    &field.selection,
                    &service_selection,
                    user_errors,
                ),
                vec![],
            );
        }

        let service_id = self.next_proxy_synthetic_gid("FulfillmentService");
        let location_id = self.next_proxy_synthetic_gid("Location");
        let requires_shipping_method = if field.arguments.contains_key("requiresShippingMethod") {
            resolved_bool_field(&field.arguments, "requiresShippingMethod").unwrap_or(false)
        } else {
            true
        };
        let service = fulfillment_service_record(
            &service_id,
            &location_id,
            &name,
            callback_url,
            resolved_bool_field(&field.arguments, "trackingSupport").unwrap_or(false),
            resolved_bool_field(&field.arguments, "inventoryManagement").unwrap_or(false),
            requires_shipping_method,
        );
        let location = service["location"].clone();
        self.store
            .staged
            .fulfillment_services
            .insert(service_id.clone(), service.clone());
        self.store
            .staged
            .fulfillment_service_locations
            .insert(location_id.clone(), location);
        self.store
            .staged
            .fulfillment_services
            .tombstones
            .remove(&service_id);
        self.store
            .staged
            .fulfillment_service_locations
            .tombstones
            .remove(&location_id);
        (
            fulfillment_service_payload_json(service, &field.selection, &service_selection, vec![]),
            vec![service_id],
        )
    }

    pub(in crate::proxy) fn fulfillment_service_update_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, Vec<String>) {
        let service_selection =
            selected_child_selection(&field.selection, "fulfillmentService").unwrap_or_default();
        let Some(id) = field.arguments.get("id").and_then(resolved_value_string) else {
            return (
                fulfillment_service_not_found_payload(&field.selection),
                vec![],
            );
        };
        let Some(existing) = self.store.staged.fulfillment_services.get(&id).cloned() else {
            return (
                fulfillment_service_not_found_payload(&field.selection),
                vec![],
            );
        };
        let name = field
            .arguments
            .get("name")
            .and_then(resolved_value_string)
            .or_else(|| existing["serviceName"].as_str().map(str::to_string))
            .unwrap_or_default();
        let callback_url = if field.arguments.contains_key("callbackUrl") {
            field
                .arguments
                .get("callbackUrl")
                .and_then(resolved_value_string)
        } else {
            existing
                .get("callbackUrl")
                .and_then(Value::as_str)
                .map(str::to_string)
        };
        let name_user_errors = if field.arguments.contains_key("name") {
            if name.trim().is_empty() {
                vec![user_error_omit_code(["name"], "Name can't be blank", None)]
            } else {
                fulfillment_service_name_whitespace_errors(&name)
            }
        } else {
            vec![]
        };
        if !name_user_errors.is_empty() {
            return (
                fulfillment_service_payload_json(
                    Value::Null,
                    &field.selection,
                    &service_selection,
                    name_user_errors,
                ),
                vec![],
            );
        }
        if fulfillment_service_name_is_reserved(&name) {
            return (
                fulfillment_service_payload_json(
                    Value::Null,
                    &field.selection,
                    &service_selection,
                    vec![user_error_omit_code(["name"], "Name is reserved", None)],
                ),
                vec![],
            );
        }
        if let Some(error) = self.fulfillment_service_callback_url_error(callback_url.as_deref()) {
            return (
                fulfillment_service_payload_json(
                    Value::Null,
                    &field.selection,
                    &service_selection,
                    vec![error],
                ),
                vec![],
            );
        }
        if self.fulfillment_service_name_or_handle_exists(&name, Some(&id)) {
            return (
                fulfillment_service_payload_json(
                    Value::Null,
                    &field.selection,
                    &service_selection,
                    vec![user_error_omit_code(
                        ["name"],
                        "Name has already been taken",
                        None,
                    )],
                ),
                vec![],
            );
        }
        let location_id = existing["location"]["id"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let requires_shipping_method = if field.arguments.contains_key("requiresShippingMethod") {
            resolved_bool_field(&field.arguments, "requiresShippingMethod").unwrap_or_else(|| {
                existing["requiresShippingMethod"]
                    .as_bool()
                    .unwrap_or(false)
            })
        } else {
            true
        };
        let mut service = fulfillment_service_record(
            &id,
            &location_id,
            &name,
            callback_url,
            resolved_bool_field(&field.arguments, "trackingSupport")
                .unwrap_or_else(|| existing["trackingSupport"].as_bool().unwrap_or(false)),
            resolved_bool_field(&field.arguments, "inventoryManagement")
                .unwrap_or_else(|| existing["inventoryManagement"].as_bool().unwrap_or(false)),
            requires_shipping_method,
        );
        if let Some(handle) = existing.get("handle").and_then(Value::as_str) {
            service["handle"] = json!(handle);
        }
        self.store
            .staged
            .fulfillment_services
            .insert(id.clone(), service.clone());
        self.store
            .staged
            .fulfillment_service_locations
            .insert(location_id, service["location"].clone());
        (
            fulfillment_service_payload_json(service, &field.selection, &service_selection, vec![]),
            vec![id],
        )
    }

    pub(in crate::proxy) fn fulfillment_service_delete_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, Vec<String>) {
        let id = field
            .arguments
            .get("id")
            .and_then(resolved_value_string)
            .unwrap_or_default();
        let inventory_action = field
            .arguments
            .get("inventoryAction")
            .and_then(resolved_value_string);
        let destination_location_id = field
            .arguments
            .get("destinationLocationId")
            .and_then(resolved_value_string)
            .filter(|value| !value.trim().is_empty());
        if !self.store.staged.fulfillment_services.contains_key(&id) {
            return (
                fulfillment_service_delete_payload(
                    Value::Null,
                    &field.selection,
                    vec![user_error_omit_code(
                        ["id"],
                        "Fulfillment service could not be found.",
                        None,
                    )],
                ),
                vec![],
            );
        }
        // KEEP/DELETE must not carry a destination location; TRANSFER must name a real one.
        match inventory_action.as_deref() {
            Some("KEEP") | Some("DELETE") if destination_location_id.is_some() => {
                return (
                    fulfillment_service_delete_payload(
                        Value::Null,
                        &field.selection,
                        vec![user_error_omit_code(["inventoryAction"], "Inventory action Destination location id should not be present when deleting/keeping the inventory of the fulfillment service.", None)],
                    ),
                    vec![],
                );
            }
            Some("TRANSFER") => {
                if let Some(destination) = destination_location_id.as_ref() {
                    if !self.store.staged.locations.contains_key(destination) {
                        return (
                            fulfillment_service_delete_payload(
                                Value::Null,
                                &field.selection,
                                vec![user_error_omit_code(
                                    Value::Null,
                                    "Invalid destination location.",
                                    None,
                                )],
                            ),
                            vec![],
                        );
                    }
                }
            }
            _ => {}
        }
        let service = self
            .store
            .staged
            .fulfillment_services
            .remove(&id)
            .expect("fulfillment service existence checked above");
        let location_id = service["location"]["id"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        self.store
            .staged
            .fulfillment_service_locations
            .remove(&location_id);
        self.store.staged.fulfillment_services.tombstone(id.clone());
        self.store
            .staged
            .fulfillment_service_locations
            .tombstone(location_id);
        (
            fulfillment_service_delete_payload(
                json!(id.replace("?id=true", "")),
                &field.selection,
                vec![],
            ),
            vec![id],
        )
    }

    pub(in crate::proxy) fn carrier_service_read_data(
        &self,
        fields: &[RootFieldSelection],
    ) -> Value {
        root_payload_json(fields, |field| {
            Some(match field.name.as_str() {
                "carrierService" => self.carrier_service_detail_field(field),
                "carrierServices" => self.carrier_services_connection_field(field),
                _ => return None,
            })
        })
    }

    pub(in crate::proxy) fn carrier_service_detail_field(
        &self,
        field: &RootFieldSelection,
    ) -> Value {
        let Some(id) = field.arguments.get("id").and_then(resolved_value_string) else {
            return Value::Null;
        };
        if self.store.staged.carrier_services.is_tombstoned(&id) {
            return Value::Null;
        }
        self.store
            .staged
            .carrier_services
            .get(&id)
            .map(|carrier| selected_json(carrier, &field.selection))
            .unwrap_or(Value::Null)
    }

    pub(in crate::proxy) fn carrier_services_connection_field(
        &self,
        field: &RootFieldSelection,
    ) -> Value {
        let query = field.arguments.get("query").and_then(resolved_value_string);
        let active_filter = match query.as_deref() {
            Some("active:true") => Some(true),
            Some("active:false") => Some(false),
            _ => None,
        };
        let mut services: Vec<Value> = self
            .store
            .staged
            .carrier_services
            .iter()
            .filter(|(id, _)| !self.store.staged.carrier_services.is_tombstoned(id))
            .map(|(_, carrier)| carrier.clone())
            .filter(|carrier| {
                active_filter
                    .map(|expected| carrier.get("active") == Some(&json!(expected)))
                    .unwrap_or(true)
            })
            .collect();
        services.sort_by_key(|carrier| {
            carrier
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        });
        selected_connection_json_with_args(
            services,
            &field.arguments,
            &field.selection,
            carrier_service_cursor,
        )
    }

    pub(in crate::proxy) fn carrier_service_mutations(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let fields = root_fields(query, variables).unwrap_or_default();
        for field in &fields {
            if field.name == "carrierServiceCreate" {
                if let Some(error) =
                    carrier_service_create_callback_url_coercion_error(query, field)
                {
                    return ok_json(json!({ "errors": [error] }));
                }
            }
        }
        let data = root_payload_json(&fields, |field| {
            let payload = match field.name.as_str() {
                "carrierServiceCreate" => {
                    self.carrier_service_create_field(field, query, variables, request)
                }
                "carrierServiceUpdate" => {
                    self.carrier_service_update_field(field, query, variables, request)
                }
                "carrierServiceDelete" => {
                    self.carrier_service_delete_field(field, query, variables, request)
                }
                _ => return None,
            };
            Some(payload)
        });
        ok_json(json!({ "data": data }))
    }

    pub(in crate::proxy) fn carrier_service_create_field(
        &mut self,
        field: &RootFieldSelection,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let carrier_selection = nested_selected_fields(&field.selection, &["carrierService"]);
        let Some(name) =
            resolved_string_field(&input, "name").filter(|name| !name.trim().is_empty())
        else {
            return carrier_service_payload_json(
                Value::Null,
                &field.selection,
                &carrier_selection,
                vec![carrier_service_user_error(
                    Value::Null,
                    "Shipping rate provider name can't be blank",
                    "CARRIER_SERVICE_CREATE_FAILED",
                )],
            );
        };
        if let Some(error) = resolved_string_field(&input, "callbackUrl").and_then(|callback_url| {
            carrier_service_callback_url_error(&callback_url, "CARRIER_SERVICE_CREATE_FAILED")
        }) {
            return carrier_service_payload_json(
                Value::Null,
                &field.selection,
                &carrier_selection,
                vec![error],
            );
        }
        // A carrier service name is unique per app/shop: a second create with the same
        // (trimmed) name returns a base CARRIER_SERVICE_CREATE_FAILED userError naming the
        // already-configured service and stages no additional record.
        let trimmed_name = name.trim();
        if self
            .store
            .staged
            .carrier_services
            .iter()
            .filter(|(id, _)| !self.store.staged.carrier_services.is_tombstoned(id))
            .any(|(_, carrier)| {
                carrier.get("name").and_then(Value::as_str).map(str::trim) == Some(trimmed_name)
            })
        {
            return carrier_service_payload_json(
                Value::Null,
                &field.selection,
                &carrier_selection,
                vec![carrier_service_user_error(
                    Value::Null,
                    &format!("{trimmed_name} is already configured"),
                    "CARRIER_SERVICE_CREATE_FAILED",
                )],
            );
        }
        let id = self.next_proxy_synthetic_gid("DeliveryCarrierService");
        let carrier = carrier_service_record(
            &id,
            &name,
            resolved_string_field(&input, "callbackUrl"),
            resolved_bool_field(&input, "active").unwrap_or(false),
            resolved_bool_field(&input, "supportsServiceDiscovery").unwrap_or(false),
        );
        self.store
            .staged
            .carrier_services
            .insert(id.clone(), carrier.clone());
        self.record_mutation_log_entry(request, query, variables, "carrierServiceCreate", vec![id]);
        carrier_service_payload_json(carrier, &field.selection, &carrier_selection, vec![])
    }

    pub(in crate::proxy) fn carrier_service_update_field(
        &mut self,
        field: &RootFieldSelection,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let carrier_selection = nested_selected_fields(&field.selection, &["carrierService"]);
        let Some(id) = resolved_string_field(&input, "id") else {
            return carrier_service_not_found_payload(
                &field.selection,
                "CARRIER_SERVICE_UPDATE_FAILED",
            );
        };
        let Some(existing) = self.store.staged.carrier_services.get(&id).cloned() else {
            return carrier_service_not_found_payload(
                &field.selection,
                "CARRIER_SERVICE_UPDATE_FAILED",
            );
        };
        if matches!(
            resolved_string_field(&input, "name").as_deref(),
            Some(name) if name.trim().is_empty()
        ) {
            return carrier_service_payload_json(
                Value::Null,
                &field.selection,
                &carrier_selection,
                vec![carrier_service_user_error(
                    Value::Null,
                    "Shipping rate provider name can't be blank",
                    "CARRIER_SERVICE_UPDATE_FAILED",
                )],
            );
        }
        let existing_callback_url = existing
            .get("callbackUrl")
            .and_then(Value::as_str)
            .map(str::to_string);
        let input_callback_url = resolved_string_field(&input, "callbackUrl");
        if input_callback_url.as_deref() != existing_callback_url.as_deref() {
            if let Some(error) = input_callback_url.as_ref().and_then(|callback_url| {
                carrier_service_callback_url_error(callback_url, "CARRIER_SERVICE_UPDATE_FAILED")
            }) {
                return carrier_service_payload_json(
                    Value::Null,
                    &field.selection,
                    &carrier_selection,
                    vec![error],
                );
            }
        }
        let name = resolved_string_field(&input, "name")
            .or_else(|| {
                existing
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .unwrap_or_default();
        let carrier = carrier_service_record(
            &id,
            &name,
            input_callback_url.or(existing_callback_url),
            resolved_bool_field(&input, "active").unwrap_or_else(|| {
                existing
                    .get("active")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            }),
            resolved_bool_field(&input, "supportsServiceDiscovery").unwrap_or_else(|| {
                existing
                    .get("supportsServiceDiscovery")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            }),
        );
        self.store
            .staged
            .carrier_services
            .insert(id.clone(), carrier.clone());
        self.record_mutation_log_entry(request, query, variables, "carrierServiceUpdate", vec![id]);
        carrier_service_payload_json(carrier, &field.selection, &carrier_selection, vec![])
    }

    pub(in crate::proxy) fn carrier_service_delete_field(
        &mut self,
        field: &RootFieldSelection,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Value {
        let id = field
            .arguments
            .get("id")
            .and_then(resolved_value_string)
            .unwrap_or_default();
        if !self.store.staged.carrier_services.contains_key(&id) {
            return carrier_service_delete_payload(
                Value::Null,
                &field.selection,
                vec![carrier_service_user_error(
                    json!(["id"]),
                    "The carrier or app could not be found.",
                    "CARRIER_SERVICE_DELETE_FAILED",
                )],
            );
        }
        self.store.staged.carrier_services.remove(&id);
        self.store.staged.carrier_services.tombstone(id.clone());
        self.record_mutation_log_entry(
            request,
            query,
            variables,
            "carrierServiceDelete",
            vec![id.clone()],
        );
        carrier_service_delete_payload(json!(id), &field.selection, vec![])
    }

    pub(in crate::proxy) fn shipping_package_mutation(
        &mut self,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let (response_key, arguments) = primary_root_field(query, variables)
            .map(|field| (field.response_key, field.arguments))
            .unwrap_or_else(|| (root_field.to_string(), BTreeMap::new()));
        let Some(ResolvedValue::String(id)) = arguments.get("id") else {
            return ok_json(
                json!({ "data": { response_key: { "userErrors": [user_error_omit_code(["id"], "ID is required", None)] } } }),
            );
        };
        let id = id.clone();
        if !is_known_shipping_package_id(&id) {
            return ok_json(json!({
                "errors": [{
                    "message": "invalid id",
                    "extensions": { "code": "RESOURCE_NOT_FOUND" },
                    "path": [root_field]
                }],
                "data": { response_key: null }
            }));
        }

        let payload = match root_field {
            "shippingPackageUpdate" => {
                let Some(ResolvedValue::Object(input)) = arguments.get("shippingPackage") else {
                    return ok_json(
                        json!({ "data": { response_key: { "userErrors": [user_error_omit_code(["shippingPackage"], "Shipping package input is required", None)] } } }),
                    );
                };
                let mut package = self.effective_shipping_package(&id);
                if package.get("boxType") == Some(&json!("FLAT_RATE")) {
                    return ok_json(json!({
                        "data": {
                            response_key: {
                                "userErrors": [user_error(["shippingPackage"], "Custom shipping box is not updatable", Some("CUSTOM_SHIPPING_BOX_NOT_UPDATABLE"))]
                            }
                        }
                    }));
                }
                let was_default = package.get("default") == Some(&json!(true));
                merge_shipping_package_input(&mut package, input);
                if !was_default && package.get("default") == Some(&json!(true)) {
                    self.clear_default_shipping_packages_except(&id);
                }
                package["updatedAt"] = json!(self.next_shipping_package_timestamp());
                self.store
                    .staged
                    .shipping_packages
                    .insert(id.clone(), package);
                json!({ "userErrors": [] })
            }
            "shippingPackageMakeDefault" => {
                self.clear_default_shipping_packages_except(&id);
                let mut package = self.effective_shipping_package(&id);
                package["default"] = json!(true);
                package["updatedAt"] = json!(self.next_shipping_package_timestamp());
                self.store
                    .staged
                    .shipping_packages
                    .insert(id.clone(), package);
                json!({ "userErrors": [] })
            }
            "shippingPackageDelete" => {
                self.store.staged.shipping_packages.remove(&id);
                self.store.staged.shipping_packages.tombstone(id.clone());
                json!({ "deletedId": id, "userErrors": [] })
            }
            _ => unreachable!("shipping package dispatcher only receives supported roots"),
        };

        self.record_shipping_package_log_entry(request, query, variables, root_field, vec![id]);
        ok_json(json!({ "data": { response_key: payload } }))
    }

    pub(in crate::proxy) fn effective_shipping_package(&self, id: &str) -> Value {
        self.store
            .staged
            .shipping_packages
            .get(id)
            .cloned()
            .unwrap_or_else(|| seed_shipping_package(id))
    }

    pub(in crate::proxy) fn clear_default_shipping_packages_except(&mut self, default_id: &str) {
        for id in [
            "gid://shopify/ShippingPackage/1",
            "gid://shopify/ShippingPackage/2",
        ] {
            if id == default_id || self.store.staged.shipping_packages.is_tombstoned(id) {
                continue;
            }
            let mut package = self.effective_shipping_package(id);
            package["default"] = json!(false);
            package["updatedAt"] = json!(self.next_shipping_package_timestamp());
            self.store
                .staged
                .shipping_packages
                .insert(id.to_string(), package);
        }
    }

    pub(in crate::proxy) fn next_shipping_package_timestamp(&self) -> String {
        let staged_shipping_mutations = self
            .log_entries
            .iter()
            .filter(|entry| {
                entry
                    .get("operationName")
                    .and_then(Value::as_str)
                    .is_some_and(|name| {
                        matches!(
                            name,
                            "shippingPackageUpdate"
                                | "shippingPackageMakeDefault"
                                | "shippingPackageDelete"
                        )
                    })
            })
            .count();
        format!(
            "2024-01-01T00:00:{:02}.000Z",
            staged_shipping_mutations * 2 + 1
        )
    }

    pub(in crate::proxy) fn record_shipping_package_log_entry(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        root_field: &str,
        staged_resource_ids: Vec<String>,
    ) {
        let id = format!("log-{}", self.log_entries.len() + 1);
        self.log_entries.push(json!({
            "id": id,
            "operationName": root_field,
            "path": request.path,
            "query": query,
            "variables": resolved_variables_json(variables),
            "rawBody": request.body,
            "stagedResourceIds": staged_resource_ids,
            "status": "staged",
            "interpreted": {
                "operationType": "mutation",
                "rootFields": [root_field],
                "primaryRootField": root_field
            }
        }));
    }
}

enum BackupRegionCountryCodeInput {
    ReadCurrent,
    CountryCode(String),
    Missing,
    Invalid(String),
}

fn backup_region_update_country_code(
    root_field: Option<&RootFieldSelection>,
) -> BackupRegionCountryCodeInput {
    let Some(field) = root_field else {
        return BackupRegionCountryCodeInput::ReadCurrent;
    };
    match field.raw_arguments.get("region") {
        None | Some(RawArgumentValue::Null) => BackupRegionCountryCodeInput::ReadCurrent,
        Some(RawArgumentValue::Variable { value, .. }) => {
            backup_region_update_variable_region_country_code(value.as_ref())
        }
        Some(RawArgumentValue::Object(region)) => backup_region_update_object_country_code(region),
        Some(value) => BackupRegionCountryCodeInput::Invalid(raw_argument_display(value)),
    }
}

fn backup_region_update_variable_region_country_code(
    value: Option<&ResolvedValue>,
) -> BackupRegionCountryCodeInput {
    match value {
        None | Some(ResolvedValue::Null) => BackupRegionCountryCodeInput::ReadCurrent,
        Some(ResolvedValue::Object(region)) => {
            backup_region_update_resolved_object_country_code(region)
        }
        Some(value) => BackupRegionCountryCodeInput::Invalid(resolved_value_display(value)),
    }
}

fn backup_region_update_object_country_code(
    region: &BTreeMap<String, RawArgumentValue>,
) -> BackupRegionCountryCodeInput {
    match region.get("countryCode") {
        None => BackupRegionCountryCodeInput::Missing,
        Some(RawArgumentValue::Enum(country_code)) => {
            BackupRegionCountryCodeInput::CountryCode(country_code.clone())
        }
        Some(RawArgumentValue::Variable { value, .. }) => {
            backup_region_update_variable_country_code(value.as_ref())
        }
        Some(value) => BackupRegionCountryCodeInput::Invalid(raw_argument_display(value)),
    }
}

fn backup_region_update_variable_country_code(
    value: Option<&ResolvedValue>,
) -> BackupRegionCountryCodeInput {
    match value {
        Some(ResolvedValue::String(country_code)) => {
            BackupRegionCountryCodeInput::CountryCode(country_code.clone())
        }
        Some(value) => BackupRegionCountryCodeInput::Invalid(resolved_value_display(value)),
        None => BackupRegionCountryCodeInput::Invalid("null".to_string()),
    }
}

fn backup_region_update_resolved_object_country_code(
    region: &BTreeMap<String, ResolvedValue>,
) -> BackupRegionCountryCodeInput {
    match region.get("countryCode") {
        None => BackupRegionCountryCodeInput::Missing,
        Some(ResolvedValue::String(country_code)) => {
            BackupRegionCountryCodeInput::CountryCode(country_code.clone())
        }
        Some(value) => BackupRegionCountryCodeInput::Invalid(resolved_value_display(value)),
    }
}

fn raw_argument_display(value: &RawArgumentValue) -> String {
    match value {
        RawArgumentValue::String(value) => json!(value).to_string(),
        RawArgumentValue::Int(value) => value.to_string(),
        RawArgumentValue::Float(value) => value.to_string(),
        RawArgumentValue::Bool(value) => value.to_string(),
        RawArgumentValue::Null => "null".to_string(),
        RawArgumentValue::Enum(value) => value.clone(),
        RawArgumentValue::List(values) => {
            let values = values.iter().map(raw_argument_json).collect::<Vec<_>>();
            Value::Array(values).to_string()
        }
        RawArgumentValue::Object(fields) => {
            let fields = fields
                .iter()
                .map(|(key, value)| (key.clone(), raw_argument_json(value)))
                .collect();
            Value::Object(fields).to_string()
        }
        RawArgumentValue::Variable { value, .. } => value
            .as_ref()
            .map(resolved_value_display)
            .unwrap_or_else(|| "null".to_string()),
    }
}

fn raw_argument_json(value: &RawArgumentValue) -> Value {
    match value {
        RawArgumentValue::String(value) | RawArgumentValue::Enum(value) => json!(value),
        RawArgumentValue::Int(value) => json!(value),
        RawArgumentValue::Float(value) => json!(value),
        RawArgumentValue::Bool(value) => json!(value),
        RawArgumentValue::Null => Value::Null,
        RawArgumentValue::List(values) => {
            Value::Array(values.iter().map(raw_argument_json).collect())
        }
        RawArgumentValue::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(key, value)| (key.clone(), raw_argument_json(value)))
                .collect(),
        ),
        RawArgumentValue::Variable { value, .. } => value
            .as_ref()
            .map(resolved_value_json)
            .unwrap_or(Value::Null),
    }
}

fn resolved_value_display(value: &ResolvedValue) -> String {
    resolved_values::resolved_value_json(value).to_string()
}

const LOCATION_COUNTRY_CODES: &str = "AF, AX, AL, DZ, AD, AO, AI, AG, AR, AM, AW, AC, AU, AT, AZ, BS, BH, BD, BB, BY, BE, BZ, BJ, BM, BT, BO, BA, BW, BV, BR, IO, BN, BG, BF, BI, KH, CA, CV, BQ, KY, CF, TD, CL, CN, CX, CC, CO, KM, CG, CD, CK, CR, HR, CU, CW, CY, CZ, CI, DK, DJ, DM, DO, EC, EG, SV, GQ, ER, EE, SZ, ET, FK, FO, FJ, FI, FR, GF, PF, TF, GA, GM, GE, DE, GH, GI, GR, GL, GD, GP, GT, GG, GN, GW, GY, HT, HM, VA, HN, HK, HU, IS, IN, ID, IR, IQ, IE, IM, IL, IT, JM, JP, JE, JO, KZ, KE, KI, KP, XK, KW, KG, LA, LV, LB, LS, LR, LY, LI, LT, LU, MO, MG, MW, MY, MV, ML, MT, MQ, MR, MU, YT, MX, MD, MC, MN, ME, MS, MA, MZ, MM, NA, NR, NP, NL, AN, NC, NZ, NI, NE, NG, NU, NF, MK, NO, OM, PK, PS, PA, PG, PY, PE, PH, PN, PL, PT, QA, CM, RE, RO, RU, RW, BL, SH, KN, LC, MF, PM, WS, SM, ST, SA, SN, RS, SC, SL, SG, SX, SK, SI, SB, SO, ZA, GS, KR, SS, ES, LK, VC, SD, SR, SJ, SE, CH, SY, TW, TJ, TZ, TH, TL, TG, TK, TO, TT, TA, TN, TR, TM, TC, TV, UG, UA, AE, GB, US, UM, UY, UZ, VU, VE, VN, VG, WF, EH, YE, ZM, ZW, ZZ";

fn location_country_code_is_valid(country_code: &str) -> bool {
    LOCATION_COUNTRY_CODES
        .split(", ")
        .any(|candidate| candidate == country_code)
}

/// Shopify projects the full ISO country name alongside the `countryCode` on an
/// address. Returns the display name for a known ISO 3166-1 alpha-2 code, or
/// `None` for codes we do not carry a name for (the proxy then emits null,
/// matching Shopify's behavior for unset addresses).
fn country_name_for_code(country_code: &str) -> Option<&'static str> {
    Some(match country_code {
        "US" => "United States",
        "CA" => "Canada",
        "AU" => "Australia",
        "GB" => "United Kingdom",
        "IE" => "Ireland",
        "FR" => "France",
        "DE" => "Germany",
        "ES" => "Spain",
        "IT" => "Italy",
        "NL" => "Netherlands",
        "BE" => "Belgium",
        "PT" => "Portugal",
        "SE" => "Sweden",
        "NO" => "Norway",
        "DK" => "Denmark",
        "FI" => "Finland",
        "CH" => "Switzerland",
        "AT" => "Austria",
        "PL" => "Poland",
        "NZ" => "New Zealand",
        "JP" => "Japan",
        "CN" => "China",
        "IN" => "India",
        "BR" => "Brazil",
        "MX" => "Mexico",
        "AR" => "Argentina",
        "ZA" => "South Africa",
        "SG" => "Singapore",
        "HK" => "Hong Kong SAR",
        _ => return None,
    })
}

/// Shopify derives the full province/state name from the `provinceCode` for
/// countries with administrative subdivisions (US, CA, AU). Countries without
/// subdivisions (e.g. GB) carry no province, so this returns `None`.
fn province_name_for_code(country_code: &str, province_code: &str) -> Option<&'static str> {
    Some(match (country_code, province_code) {
        ("US", "AL") => "Alabama",
        ("US", "AK") => "Alaska",
        ("US", "AZ") => "Arizona",
        ("US", "AR") => "Arkansas",
        ("US", "CA") => "California",
        ("US", "CO") => "Colorado",
        ("US", "CT") => "Connecticut",
        ("US", "DE") => "Delaware",
        ("US", "DC") => "District of Columbia",
        ("US", "FL") => "Florida",
        ("US", "GA") => "Georgia",
        ("US", "HI") => "Hawaii",
        ("US", "ID") => "Idaho",
        ("US", "IL") => "Illinois",
        ("US", "IN") => "Indiana",
        ("US", "IA") => "Iowa",
        ("US", "KS") => "Kansas",
        ("US", "KY") => "Kentucky",
        ("US", "LA") => "Louisiana",
        ("US", "ME") => "Maine",
        ("US", "MD") => "Maryland",
        ("US", "MA") => "Massachusetts",
        ("US", "MI") => "Michigan",
        ("US", "MN") => "Minnesota",
        ("US", "MS") => "Mississippi",
        ("US", "MO") => "Missouri",
        ("US", "MT") => "Montana",
        ("US", "NE") => "Nebraska",
        ("US", "NV") => "Nevada",
        ("US", "NH") => "New Hampshire",
        ("US", "NJ") => "New Jersey",
        ("US", "NM") => "New Mexico",
        ("US", "NY") => "New York",
        ("US", "NC") => "North Carolina",
        ("US", "ND") => "North Dakota",
        ("US", "OH") => "Ohio",
        ("US", "OK") => "Oklahoma",
        ("US", "OR") => "Oregon",
        ("US", "PA") => "Pennsylvania",
        ("US", "RI") => "Rhode Island",
        ("US", "SC") => "South Carolina",
        ("US", "SD") => "South Dakota",
        ("US", "TN") => "Tennessee",
        ("US", "TX") => "Texas",
        ("US", "UT") => "Utah",
        ("US", "VT") => "Vermont",
        ("US", "VA") => "Virginia",
        ("US", "WA") => "Washington",
        ("US", "WV") => "West Virginia",
        ("US", "WI") => "Wisconsin",
        ("US", "WY") => "Wyoming",
        ("CA", "AB") => "Alberta",
        ("CA", "BC") => "British Columbia",
        ("CA", "MB") => "Manitoba",
        ("CA", "NB") => "New Brunswick",
        ("CA", "NL") => "Newfoundland and Labrador",
        ("CA", "NT") => "Northwest Territories",
        ("CA", "NS") => "Nova Scotia",
        ("CA", "NU") => "Nunavut",
        ("CA", "ON") => "Ontario",
        ("CA", "PE") => "Prince Edward Island",
        ("CA", "QC") => "Quebec",
        ("CA", "SK") => "Saskatchewan",
        ("CA", "YT") => "Yukon",
        ("AU", "ACT") => "Australian Capital Territory",
        ("AU", "NSW") => "New South Wales",
        ("AU", "NT") => "Northern Territory",
        ("AU", "QLD") => "Queensland",
        ("AU", "SA") => "South Australia",
        ("AU", "TAS") => "Tasmania",
        ("AU", "VIC") => "Victoria",
        ("AU", "WA") => "Western Australia",
        _ => return None,
    })
}

/// Build the `address` object for a staged location from a Location*Input
/// address, deriving the full country/province names from the supplied codes the
/// way Shopify does. Absent codes serialize as null (not empty string).
fn location_address_json(address_input: &BTreeMap<String, ResolvedValue>) -> Value {
    let country_code = resolved_string_field(address_input, "countryCode");
    let province_code =
        resolved_string_field(address_input, "provinceCode").filter(|code| !code.is_empty());
    let country = country_code
        .as_deref()
        .and_then(country_name_for_code)
        .map(Value::from)
        .unwrap_or(Value::Null);
    let province = match (country_code.as_deref(), province_code.as_deref()) {
        (Some(country), Some(province)) => province_name_for_code(country, province)
            .map(Value::from)
            .unwrap_or(Value::Null),
        _ => Value::Null,
    };
    json!({
        "address1": resolved_string_field(address_input, "address1"),
        "address2": resolved_string_field(address_input, "address2"),
        "city": resolved_string_field(address_input, "city"),
        "country": country,
        "countryCode": country_code,
        "province": province,
        "provinceCode": province_code,
        "zip": resolved_string_field(address_input, "zip")
    })
}

fn input_was_variable(field: &RootFieldSelection) -> bool {
    matches!(
        field.raw_arguments.get("input"),
        Some(RawArgumentValue::Variable { .. })
    )
}

fn location_add_missing_input_error(operation_path: &str, field: &RootFieldSelection) -> Value {
    json!({
        "errors": [{
            "message": "Field 'locationAdd' is missing required arguments: input",
            "locations": [{ "line": field.location.line, "column": field.location.column }],
            "path": [operation_path, "locationAdd"],
            "extensions": {
                "code": "missingRequiredArguments",
                "className": "Field",
                "name": "locationAdd",
                "arguments": "input"
            }
        }]
    })
}

fn location_add_missing_address_error(operation_path: &str, field: &RootFieldSelection) -> Value {
    json!({
        "errors": [{
            "message": "Argument 'address' on InputObject 'LocationAddInput' is required. Expected type LocationAddAddressInput!",
            "locations": [{ "line": field.location.line, "column": field.location.column }],
            "path": [operation_path, "locationAdd", "input", "address"],
            "extensions": {
                "code": "missingRequiredInputObjectAttribute",
                "argumentName": "address",
                "argumentType": "LocationAddAddressInput!",
                "inputObjectType": "LocationAddInput"
            }
        }]
    })
}

fn location_add_missing_country_code_error(
    operation_path: &str,
    field: &RootFieldSelection,
) -> Value {
    json!({
        "errors": [{
            "message": "Argument 'countryCode' on InputObject 'LocationAddAddressInput' is required. Expected type CountryCode!",
            "locations": [{ "line": field.location.line, "column": field.location.column }],
            "path": [operation_path, "locationAdd", "input", "address", "countryCode"],
            "extensions": {
                "code": "missingRequiredInputObjectAttribute",
                "argumentName": "countryCode",
                "argumentType": "CountryCode!",
                "inputObjectType": "LocationAddAddressInput"
            }
        }]
    })
}

fn location_add_inline_argument_not_accepted_error(
    operation_path: &str,
    field: &RootFieldSelection,
    argument_name: &str,
) -> Value {
    json!({
        "errors": [{
            "message": format!("InputObject 'LocationAddInput' doesn't accept argument '{}'", argument_name),
            "locations": [{ "line": field.location.line, "column": field.location.column }],
            "path": [operation_path, "locationAdd", "input", argument_name],
            "extensions": {
                "code": "argumentNotAccepted",
                "name": "LocationAddInput",
                "typeName": "InputObject",
                "argumentName": argument_name
            }
        }]
    })
}

/// Metafield content types accepted by Shopify, in the exact order they appear
/// in the public Admin API `INVALID_TYPE` user error. Used to validate location
/// metafield input and to render the "Type must be one of the following: ..."
/// message verbatim.
const LOCATION_METAFIELD_VALID_TYPES: &[&str] = &[
    "antenna_gain",
    "area",
    "battery_charge_capacity",
    "battery_energy_capacity",
    "boolean",
    "capacitance",
    "color",
    "concentration",
    "data_storage_capacity",
    "data_transfer_rate",
    "date_time",
    "date",
    "dimension",
    "display_density",
    "distance",
    "duration",
    "electric_current",
    "electrical_resistance",
    "energy",
    "float",
    "frequency",
    "id",
    "illuminance",
    "inductance",
    "integer",
    "json_string",
    "json",
    "language",
    "link",
    "list.antenna_gain",
    "list.area",
    "list.battery_charge_capacity",
    "list.battery_energy_capacity",
    "list.boolean",
    "list.capacitance",
    "list.color",
    "list.concentration",
    "list.data_storage_capacity",
    "list.data_transfer_rate",
    "list.date_time",
    "list.date",
    "list.dimension",
    "list.display_density",
    "list.distance",
    "list.duration",
    "list.electric_current",
    "list.electrical_resistance",
    "list.energy",
    "list.frequency",
    "list.illuminance",
    "list.inductance",
    "list.link",
    "list.luminous_flux",
    "list.mass_flow_rate",
    "list.multi_line_text_field",
    "list.number_decimal",
    "list.number_integer",
    "list.power",
    "list.pressure",
    "list.rating",
    "list.resolution",
    "list.rotational_speed",
    "list.single_line_text_field",
    "list.sound_level",
    "list.speed",
    "list.temperature",
    "list.thermal_power",
    "list.url",
    "list.voltage",
    "list.volume",
    "list.volumetric_flow_rate",
    "list.weight",
    "luminous_flux",
    "mass_flow_rate",
    "money",
    "multi_line_text_field",
    "number_decimal",
    "number_integer",
    "power",
    "pressure",
    "rating",
    "resolution",
    "rich_text_field",
    "rotational_speed",
    "single_line_text_field",
    "sound_level",
    "speed",
    "string",
    "temperature",
    "thermal_power",
    "url",
    "voltage",
    "volume",
    "volumetric_flow_rate",
    "weight",
    "company_reference",
    "list.company_reference",
    "customer_reference",
    "list.customer_reference",
    "product_reference",
    "list.product_reference",
    "collection_reference",
    "list.collection_reference",
    "variant_reference",
    "list.variant_reference",
    "file_reference",
    "list.file_reference",
    "product_taxonomy_value_reference",
    "list.product_taxonomy_value_reference",
    "metaobject_reference",
    "list.metaobject_reference",
    "mixed_reference",
    "list.mixed_reference",
    "page_reference",
    "list.page_reference",
    "article_reference",
    "list.article_reference",
    "order_reference",
    "list.order_reference",
];

/// Top-level GraphQL error returned when a `locationAdd` metafield carries a
/// blank `key`. Shopify rejects this as an input-arguments coercion failure
/// anchored at both the field and the `$input` variable definition.
fn location_add_metafield_blank_key_error(
    field: &RootFieldSelection,
    document: &crate::graphql::ParsedDocument,
) -> Value {
    let mut locations = vec![json!({
        "line": field.location.line,
        "column": field.location.column
    })];
    if let Some(definition) = document.variable_definitions.get("input") {
        locations.push(json!({
            "line": definition.location.line,
            "column": definition.location.column
        }));
    }
    json!({
        "errors": [{
            "message": "key can't be blank",
            "locations": locations,
            "extensions": { "code": "INVALID_FIELD_ARGUMENTS" },
            "path": [field.response_key.clone()]
        }],
        "data": { field.response_key.clone(): Value::Null }
    })
}

fn location_add_invalid_variable_error(
    path: &str,
    explanation: &str,
    input: &BTreeMap<String, ResolvedValue>,
) -> Value {
    let path_parts = path.split('.').collect::<Vec<_>>();
    json!({
        "errors": [{
            "message": format!(
                "Variable $input of type LocationAddInput! was provided invalid value for {} ({})",
                path,
                explanation
            ),
            "extensions": {
                "code": "INVALID_VARIABLE",
                "value": resolved_values::resolved_value_json(&ResolvedValue::Object(input.clone())),
                "problems": [{
                    "path": path_parts,
                    "explanation": explanation
                }]
            }
        }]
    })
}

fn location_edit_invalid_variable_error(
    path: &str,
    explanation: &str,
    input: &BTreeMap<String, ResolvedValue>,
) -> Value {
    let path_parts = path.split('.').collect::<Vec<_>>();
    json!({
        "errors": [{
            "message": format!(
                "Variable $input of type LocationEditInput! was provided invalid value for {} ({})",
                path,
                explanation
            ),
            "extensions": {
                "code": "INVALID_VARIABLE",
                "value": resolved_values::resolved_value_json(&ResolvedValue::Object(input.clone())),
                "problems": [{
                    "path": path_parts,
                    "explanation": explanation
                }]
            }
        }]
    })
}

fn location_payload_selected_json(
    location: Value,
    payload_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    selected_payload_json(payload_selection, |selection| {
        match selection.name.as_str() {
            "location" => Some(if location.is_null() {
                Value::Null
            } else {
                location_selected_json(&location, &selection.selection)
            }),
            "userErrors" => selected_user_errors_field(user_errors.as_slice(), selection),
            _ => None,
        }
    })
}

fn location_delete_payload_selected_json(
    deleted_location_id: Value,
    payload_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    selected_payload_json(payload_selection, |selection| {
        match selection.name.as_str() {
            "deletedLocationId" => Some(deleted_location_id.clone()),
            "locationDeleteUserErrors" | "userErrors" => {
                selected_user_errors_field(user_errors.as_slice(), selection)
            }
            _ => None,
        }
    })
}

fn location_country_name(country_code: &str) -> Option<&'static str> {
    if matches!(country_code, "CA" | "US" | "GB" | "AU") {
        country_name_for_code(country_code)
    } else {
        None
    }
}

fn location_delete_user_error(code: &str, message: &str) -> Value {
    user_error(["locationId"], message, Some(code))
}

fn location_requires_idempotency(request: &Request, query: &str) -> bool {
    admin_graphql_version(&request.path).is_some_and(location_version_requires_idempotency)
        && !query.contains("@idempotent")
}

fn location_version_requires_idempotency(version: &str) -> bool {
    let Some((year, month)) = version.split_once('-') else {
        return false;
    };
    let Ok(year) = year.parse::<u16>() else {
        return false;
    };
    let Ok(month) = month.parse::<u8>() else {
        return false;
    };
    year > 2026 || (year == 2026 && month >= 4)
}

fn location_idempotency_required_error(
    root_field: &str,
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Value {
    let field = root_fields(query, variables)
        .and_then(|fields| fields.into_iter().find(|field| field.name == root_field));
    let response_key = field
        .as_ref()
        .map(|field| field.response_key.clone())
        .unwrap_or_else(|| root_field.to_string());
    let (line, column) = field
        .as_ref()
        .map(|field| (field.location.line, field.location.column))
        .unwrap_or((1, 1));
    json!({
        "errors": [{
            "message": "The @idempotent directive is required for this mutation but was not provided.",
            "locations": [{ "line": line, "column": column }],
            "extensions": { "code": "BAD_REQUEST" },
            "path": [root_field]
        }],
        "data": { response_key: Value::Null }
    })
}

fn location_local_pickup_enable_payload_selected_json(
    settings: Value,
    payload_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    selected_payload_json(payload_selection, |selection| {
        match selection.name.as_str() {
            "localPickupSettings" => Some(if settings.is_null() {
                Value::Null
            } else {
                selected_json(&settings, &selection.selection)
            }),
            "userErrors" => selected_user_errors_field(user_errors.as_slice(), selection),
            _ => None,
        }
    })
}

fn location_local_pickup_disable_payload_selected_json(
    location_id: String,
    payload_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    selected_payload_json(payload_selection, |selection| {
        match selection.name.as_str() {
            "locationId" => Some(json!(location_id)),
            "userErrors" => selected_user_errors_field(user_errors.as_slice(), selection),
            _ => None,
        }
    })
}

fn local_pickup_time_is_standard(pickup_time: &str) -> bool {
    matches!(
        pickup_time,
        "ONE_HOUR"
            | "TWO_HOURS"
            | "FOUR_HOURS"
            | "TWENTY_FOUR_HOURS"
            | "TWO_TO_FOUR_DAYS"
            | "FIVE_OR_MORE_DAYS"
    )
}

fn location_activate_payload_selected_json(
    location: Value,
    payload_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    selected_payload_json(payload_selection, |selection| {
        match selection.name.as_str() {
            "location" => Some(location_selected_json(&location, &selection.selection)),
            "locationActivateUserErrors" => {
                selected_user_errors_field(user_errors.as_slice(), selection)
            }
            _ => None,
        }
    })
}

fn location_selected_json(location: &Value, selections: &[SelectedField]) -> Value {
    let mut fields = serde_json::Map::new();
    for selection in selections {
        let value = match selection.name.as_str() {
            "metafield" => location_metafield_json(location, selection),
            "metafields" => Some(location_metafields_connection_json(location, selection)),
            _ => location.get(&selection.name).map(|value| {
                if selection.selection.is_empty() {
                    value.clone()
                } else if value.is_null() {
                    Value::Null
                } else if let Some(values) = value.as_array() {
                    Value::Array(
                        values
                            .iter()
                            .map(|item| location_selected_json(item, &selection.selection))
                            .collect(),
                    )
                } else {
                    selected_json(value, &selection.selection)
                }
            }),
        };
        if let Some(value) = value {
            fields.insert(selection.response_key.clone(), value);
        }
    }
    Value::Object(fields)
}

fn location_metafield_json(location: &Value, selection: &SelectedField) -> Option<Value> {
    let namespace = resolved_string_field(&selection.arguments, "namespace").unwrap_or_default();
    let key = resolved_string_field(&selection.arguments, "key").unwrap_or_default();
    let metafield = location
        .get("metafields")
        .and_then(Value::as_array)
        .and_then(|metafields| {
            metafields.iter().find(|metafield| {
                metafield.get("namespace").and_then(Value::as_str) == Some(namespace.as_str())
                    && metafield.get("key").and_then(Value::as_str) == Some(key.as_str())
            })
        });
    Some(
        metafield
            .map(|metafield| selected_json(metafield, &selection.selection))
            .unwrap_or(Value::Null),
    )
}

fn location_metafields_connection_json(location: &Value, selection: &SelectedField) -> Value {
    let namespace = resolved_string_field(&selection.arguments, "namespace");
    let mut metafields = location
        .get("metafields")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if let Some(namespace) = namespace {
        metafields.retain(|metafield| {
            metafield.get("namespace").and_then(Value::as_str) == Some(namespace.as_str())
        });
    }
    if let Some(limit) = selection.arguments.get("first").and_then(resolved_as_usize) {
        metafields.truncate(limit);
    }
    selected_json(
        &json!({
            "nodes": metafields,
            "pageInfo": empty_page_info()
        }),
        &selection.selection,
    )
}

fn fixture_location_activate_guard_location(location_id: &str) -> Option<Value> {
    match location_id {
        "gid://shopify/Location/activate-limit"
        | "gid://shopify/Location/location-add-limit-seed" => Some(json!({
            "__typename": "Location",
            "id": location_id,
            "name": "Location limit guard",
            "isActive": false,
            "activatable": true,
            "deactivatable": false,
            "fulfillsOnlineOrders": false,
            "hasActiveInventory": false,
            "hasUnfulfilledOrders": false,
            "isFulfillmentService": false,
            "shipsInventory": false,
            "address": {},
            "metafields": [],
            "reachedLocationLimit": true
        })),
        "gid://shopify/Location/activate-relocation" => Some(json!({
            "__typename": "Location",
            "id": location_id,
            "name": "Relocation guard",
            "isActive": false,
            "activatable": true,
            "deactivatable": false,
            "fulfillsOnlineOrders": false,
            "hasActiveInventory": false,
            "hasUnfulfilledOrders": false,
            "isFulfillmentService": false,
            "shipsInventory": false,
            "address": {},
            "metafields": [],
            "hasOngoingRelocation": true
        })),
        _ => None,
    }
}

fn fixture_location_deactivate_state_machine_location(location_id: &str) -> Option<Value> {
    match location_id {
        "gid://shopify/Location/112831103282" => Some(json!({
            "id": location_id,
            "name": "HAR-658 lifecycle 20260505013332",
            "isActive": true,
            "activatable": true,
            "deactivatable": true,
            "fulfillsOnlineOrders": false,
            "hasActiveInventory": false,
            "hasUnfulfilledOrders": false,
            "deletable": false,
            "shipsInventory": false,
            "isFulfillmentService": false,
            "address": {},
            "metafields": []
        })),
        "gid://shopify/Location/112849125682" => Some(json!({
            "id": location_id,
            "name": "location-deactivate-state-machine source 20260506013233",
            "isActive": true,
            "activatable": true,
            "deactivatable": true,
            "fulfillsOnlineOrders": false,
            "hasActiveInventory": false,
            "hasUnfulfilledOrders": false,
            "deletable": false,
            "shipsInventory": false
        })),
        "gid://shopify/Location/112849158450" => Some(json!({
            "id": location_id,
            "name": "location-deactivate-state-machine inactive destination 20260506013233",
            "isActive": false,
            "activatable": true,
            "deactivatable": true,
            "fulfillsOnlineOrders": false,
            "hasActiveInventory": false,
            "hasUnfulfilledOrders": false,
            "deletable": true,
            "shipsInventory": false
        })),
        "gid://shopify/Location/inactive" => Some(json!({
            "id": location_id,
            "name": "Inactive location",
            "isActive": false,
            "activatable": true,
            "deactivatable": true,
            "fulfillsOnlineOrders": false,
            "hasActiveInventory": false,
            "hasUnfulfilledOrders": false,
            "deletable": true,
            "shipsInventory": false
        })),
        "gid://shopify/Location/112849191218" => Some(json!({
            "id": location_id,
            "name": "location-deactivate-state-machine active inventory 20260506013233",
            "isActive": true,
            "activatable": true,
            "deactivatable": true,
            "fulfillsOnlineOrders": false,
            "hasActiveInventory": true,
            "hasUnfulfilledOrders": false,
            "deletable": false,
            "shipsInventory": false
        })),
        "gid://shopify/Location/112849223986" => Some(json!({
            "id": location_id,
            "name": "location-deactivate-state-machine only online 20260506013233",
            "isActive": true,
            "activatable": true,
            "deactivatable": true,
            "fulfillsOnlineOrders": true,
            "hasActiveInventory": false,
            "hasUnfulfilledOrders": false,
            "deletable": false,
            "shipsInventory": false
        })),
        "gid://shopify/Location/106318430514" => Some(json!({
            "id": location_id,
            "name": "Shop location",
            "isActive": true,
            "activatable": true,
            "deactivatable": false,
            "fulfillsOnlineOrders": true,
            "hasActiveInventory": true,
            "hasUnfulfilledOrders": true,
            "deletable": false,
            "shipsInventory": true
        })),
        _ => None,
    }
}

/// The publication gids named in a `publishablePublish`/`publishableUnpublish`
/// `input: [{ publicationId }]` list, in order.
pub(in crate::proxy) fn publishable_input_publication_ids(
    arguments: &BTreeMap<String, ResolvedValue>,
) -> Vec<String> {
    match arguments.get("input") {
        Some(ResolvedValue::List(items)) => items
            .iter()
            .filter_map(|item| match item {
                ResolvedValue::Object(publication) => {
                    resolved_string_field(publication, "publicationId")
                }
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

pub(in crate::proxy) fn publishable_publication_input_errors(
    input: Option<&ResolvedValue>,
    current_channel_root: bool,
) -> Vec<Value> {
    if current_channel_root {
        return Vec::new();
    }
    let Some(ResolvedValue::List(publications)) = input else {
        return Vec::new();
    };

    let mut seen = BTreeSet::new();
    let mut user_errors = Vec::new();
    for (index, publication) in publications.iter().enumerate() {
        let ResolvedValue::Object(publication) = publication else {
            continue;
        };
        let field_index = index.to_string();
        let publication_id = resolved_string_field(publication, "publicationId");
        match publication_id.as_deref() {
            Some("") => {
                user_errors.push(user_error_omit_code(
                    json!(["input", field_index, "publicationId"]),
                    "PublicationId cannot be empty",
                    None,
                ));
                continue;
            }
            Some("gid://shopify/Publication/999999999999") => {
                user_errors.push(user_error_omit_code(
                    json!(["input", field_index, "publicationId"]),
                    "Publication does not exist or is not publishable",
                    None,
                ));
                continue;
            }
            Some(id) if !seen.insert(id.to_string()) => {
                user_errors.push(user_error_omit_code(
                    json!(["input", field_index, "publicationId"]),
                    "The same publication was specified more than once",
                    None,
                ));
            }
            Some(_) => {}
            None => user_errors.push(user_error_omit_code(
                json!(["input", field_index, "publicationId"]),
                "PublicationId cannot be empty",
                None,
            )),
        }

        if resolved_string_field(publication, "publishDate")
            .as_deref()
            .map(publishable_publish_date_is_before_1970)
            .unwrap_or(false)
        {
            user_errors.push(user_error_omit_code(
                json!(["input", field_index, "publishDate"]),
                "Publish date must be a date after the year 1969",
                None,
            ));
        }
    }
    user_errors
}

fn publishable_publish_date_is_before_1970(value: &str) -> bool {
    value
        .get(..4)
        .and_then(|year| year.parse::<i32>().ok())
        .map(|year| year < 1970)
        .unwrap_or(false)
}

pub(in crate::proxy) fn publishable_empty_string_publication_error(
    root_field: &str,
    field: &RootFieldSelection,
) -> Option<Response> {
    let input = field.arguments.get("input")?;
    let ResolvedValue::List(publications) = input else {
        return None;
    };
    let has_empty_string = publications.iter().any(|publication| {
        let ResolvedValue::Object(publication) = publication else {
            return false;
        };
        resolved_string_field(publication, "publicationId").as_deref() == Some("")
    });
    if !has_empty_string {
        return None;
    }

    let column = match root_field {
        "publishableUnpublish" => 58,
        _ => 56,
    };
    let message = "Variable $input of type [PublicationInput!]! was provided invalid value for 0.publicationId (Invalid global id '')";
    Some(ok_json(json!({
        "errors": [{
            "message": message,
            "locations": [{ "line": field.location.line, "column": column }],
            "extensions": {
                "code": "INVALID_VARIABLE",
                "value": resolved_values::resolved_value_json(input),
                "problems": [{
                    "path": [0, "publicationId"],
                    "explanation": "Invalid global id ''",
                    "message": "Invalid global id ''"
                }]
            }
        }]
    })))
}

impl DraftProxy {
    pub(in crate::proxy) fn flow_utility_mutation(
        &mut self,
        root_field: &str,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let Some(fields) = root_fields(query, variables) else {
            return json_error(400, "Could not parse GraphQL operation");
        };
        let mut log_root: Option<String> = None;
        let mut top_level_error = None;
        let data = root_payload_json(&fields, |field| {
            if top_level_error.is_some() {
                return None;
            }
            match field.name.as_str() {
                "flowGenerateSignature" => {
                    match self.flow_generate_signature_field(field, query, variables) {
                        FlowFieldResult::Payload { value, staged } => {
                            if staged {
                                log_root.get_or_insert_with(|| field.name.clone());
                            }
                            Some(value)
                        }
                        FlowFieldResult::TopLevelError(error) => {
                            top_level_error = Some(ok_json(error));
                            None
                        }
                    }
                }
                "flowTriggerReceive" => {
                    let (value, staged) = self.flow_trigger_receive_field(field);
                    if staged {
                        log_root.get_or_insert_with(|| field.name.clone());
                    }
                    Some(value)
                }
                _ => None,
            }
        });
        if let Some(response) = top_level_error {
            return response;
        }
        if let Some(log_root) = log_root {
            self.record_mutation_log_entry(request, query, variables, &log_root, Vec::new());
        }
        if data.as_object().is_none_or(serde_json::Map::is_empty) {
            json_error(
                501,
                &format!(
                    "No Rust stage-locally dispatcher implemented for root field: {root_field}"
                ),
            )
        } else {
            ok_json(json!({ "data": data }))
        }
    }

    fn flow_generate_signature_field(
        &mut self,
        field: &RootFieldSelection,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> FlowFieldResult {
        let operation_path = parsed_operation_path(query, variables);
        if let Some(error) = flow_generate_signature_required_arg_error(field, &operation_path) {
            return FlowFieldResult::TopLevelError(error);
        }
        if let Some(error) = flow_generate_signature_null_arg_error(field, &operation_path) {
            return FlowFieldResult::TopLevelError(error);
        }

        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        if !id.starts_with("gid://shopify/FlowActionDefinition/") {
            return FlowFieldResult::TopLevelError(flow_resource_not_found_error(field, &id));
        }

        let payload = resolved_string_arg(&field.arguments, "payload").unwrap_or_default();
        let Ok(payload_json) = serde_json::from_str::<Value>(&payload) else {
            let value = selected_json(
                &json!({
                    "signature": Value::Null,
                    "payload": Value::Null,
                    "userErrors": [user_error_omit_code(["payload"], "Payload must be valid JSON", None)]
                }),
                &field.selection,
            );
            return FlowFieldResult::Payload {
                value,
                staged: false,
            };
        };

        let canonical_payload = canonical_json_string(&payload_json);
        let signature = local_flow_signature(&id, &canonical_payload);
        self.store.staged.flow_signatures.push(json!({
            "id": id,
            "payloadHash": stable_hash_hex(&canonical_payload),
            "signatureHash": stable_hash_hex(&signature),
            "payloadByteSize": canonical_payload.len()
        }));

        FlowFieldResult::Payload {
            value: selected_json(
                &json!({
                    "signature": signature,
                    "payload": canonical_payload,
                    "userErrors": []
                }),
                &field.selection,
            ),
            staged: true,
        }
    }

    fn flow_trigger_receive_field(&mut self, field: &RootFieldSelection) -> (Value, bool) {
        let has_body = argument_string(&field.arguments, "body")
            .map(|body| !body.is_empty())
            .unwrap_or(false);
        let has_handle = argument_string(&field.arguments, "handle")
            .map(|handle| !handle.is_empty())
            .unwrap_or(false);
        let has_payload = field
            .arguments
            .get("payload")
            .is_some_and(|value| !matches!(value, ResolvedValue::Null));

        if has_body && (field.arguments.contains_key("handle") || has_payload) {
            return (
                flow_trigger_payload(
                    field,
                    "body",
                    "Cannot use `handle` and `payload` arguments with `body` argument",
                ),
                false,
            );
        }
        if has_body {
            let body = argument_string(&field.arguments, "body").unwrap_or_default();
            return match flow_trigger_body_validation_message(&body) {
                Some(message) => (flow_trigger_payload(field, "body", &message), false),
                None => {
                    self.store.staged.flow_trigger_receipts.push(json!({
                        "source": "body",
                        "bodyHash": stable_hash_hex(&body),
                        "bodyByteSize": body.len()
                    }));
                    (flow_trigger_success_payload(field), true)
                }
            };
        }
        if !has_handle || !has_payload {
            return (
                flow_trigger_payload(
                    field,
                    "handle",
                    "`handle` and `payload` arguments are required",
                ),
                false,
            );
        }

        let handle = argument_string(&field.arguments, "handle").unwrap_or_default();
        let Some(payload) = field.arguments.get("payload") else {
            return (
                flow_trigger_payload(
                    field,
                    "handle",
                    "`handle` and `payload` arguments are required",
                ),
                false,
            );
        };
        let payload_json = resolved_values::resolved_value_json(payload);
        let canonical_payload = canonical_json_string(&payload_json);
        if canonical_payload.len() > 50_000 {
            return (
                flow_trigger_payload(
                    field,
                    "body",
                    "Errors validating schema:\n  Properties size exceeds the limit of 50000 bytes.\n",
                ),
                false,
            );
        }
        if !is_local_flow_handle(&handle) {
            return (
                flow_trigger_payload(
                    field,
                    "body",
                    &format!("Errors validating schema:\n  Invalid handle '{handle}'.\n"),
                ),
                false,
            );
        }

        self.store.staged.flow_trigger_receipts.push(json!({
            "source": "handle",
            "handle": handle,
            "payloadHash": stable_hash_hex(&canonical_payload),
            "payloadByteSize": canonical_payload.len()
        }));
        (flow_trigger_success_payload(field), true)
    }
}

enum FlowFieldResult {
    Payload { value: Value, staged: bool },
    TopLevelError(Value),
}

fn parsed_operation_path(query: &str, variables: &BTreeMap<String, ResolvedValue>) -> String {
    crate::graphql::parsed_document(query, variables)
        .map(|document| document.operation_path)
        .unwrap_or_else(|| "mutation".to_string())
}

fn flow_generate_signature_required_arg_error(
    field: &RootFieldSelection,
    operation_path: &str,
) -> Option<Value> {
    let mut missing = Vec::new();
    if !field.raw_arguments.contains_key("id") {
        missing.push("id");
    }
    if !field.raw_arguments.contains_key("payload") {
        missing.push("payload");
    }
    if missing.is_empty() {
        return None;
    }
    let arguments = missing.join(", ");
    Some(json!({
        "errors": [{
            "message": format!("Field 'flowGenerateSignature' is missing required arguments: {arguments}"),
            "locations": [{ "line": field.location.line, "column": field.location.column }],
            "path": [operation_path, "flowGenerateSignature"],
            "extensions": {
                "code": "missingRequiredArguments",
                "className": "Field",
                "name": "flowGenerateSignature",
                "arguments": arguments
            }
        }]
    }))
}

fn flow_generate_signature_null_arg_error(
    field: &RootFieldSelection,
    operation_path: &str,
) -> Option<Value> {
    for (name, expected_type) in [("id", "ID!"), ("payload", "String!")] {
        let Some(raw) = field.raw_arguments.get(name) else {
            continue;
        };
        if !raw.is_literal_null() && !raw.is_unbound_variable() {
            continue;
        }
        return Some(json!({
            "errors": [{
                "message": format!("Argument '{name}' on Field 'flowGenerateSignature' has an invalid value (null). Expected type '{expected_type}'."),
                "locations": [{ "line": field.location.line, "column": field.location.column }],
                "path": [operation_path, "flowGenerateSignature", name],
                "extensions": {
                    "code": "argumentLiteralsIncompatible",
                    "typeName": "Field",
                    "argumentName": name
                }
            }]
        }));
    }
    None
}

fn flow_resource_not_found_error(field: &RootFieldSelection, id: &str) -> Value {
    json!({
        "errors": [{
            "message": format!("Invalid id: {id}"),
            "locations": [{ "line": field.location.line, "column": field.location.column }],
            "extensions": { "code": "RESOURCE_NOT_FOUND" },
            "path": [field.response_key.clone()]
        }],
        "data": { field.response_key.clone(): Value::Null }
    })
}

fn flow_trigger_payload(field: &RootFieldSelection, field_name: &str, message: &str) -> Value {
    selected_json(
        &json!({
            "userErrors": [user_error_omit_code(json!([field_name]), message, None)]
        }),
        &field.selection,
    )
}

fn flow_trigger_success_payload(field: &RootFieldSelection) -> Value {
    selected_json(&json!({ "userErrors": [] }), &field.selection)
}

fn argument_string(arguments: &BTreeMap<String, ResolvedValue>, name: &str) -> Option<String> {
    match arguments.get(name) {
        Some(ResolvedValue::String(value)) => Some(value.clone()),
        _ => None,
    }
}

fn flow_trigger_body_validation_message(body: &str) -> Option<String> {
    let parsed = match serde_json::from_str::<Value>(body) {
        Ok(value) => value,
        Err(error) => {
            let column = error.column().saturating_sub(1).max(1);
            return Some(format!(
                "Errors validating schema:\n  unexpected token '{}' at line {} column {}\n",
                body.split_whitespace().next().unwrap_or_default(),
                error.line(),
                column
            ));
        }
    };
    let Some(object) = parsed.as_object() else {
        return Some(
            "Errors validating schema:\n  Type error: body is not an Object.\n".to_string(),
        );
    };

    let mut errors = Vec::new();
    let allowed = ["trigger_id", "trigger_title", "properties", "resources"];
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            errors.push(format!("Invalid field: '{key}'."));
        }
    }

    match object.get("properties") {
        Some(properties) if properties.is_object() => {
            if canonical_json_string(properties).len() > 50_000 {
                errors.push("Properties size exceeds the limit of 50000 bytes.".to_string());
            }
        }
        Some(properties) => errors.push(format!(
            "Type error for field 'properties': {} is not an Object.",
            flow_json_value_label(properties)
        )),
        None => {}
    }

    if let Some(Value::Array(resources)) = object.get("resources") {
        for resource in resources {
            let Some(resource) = resource.as_object() else {
                continue;
            };
            if !resource.contains_key("name") {
                errors.push("Required field missing: 'name'.".to_string());
            }
            match resource.get("url").and_then(Value::as_str) {
                Some(url) if url.starts_with("http://") || url.starts_with("https://") => {}
                Some(url) => errors.push(format!(
                    "Type error for field 'url': {url} is not an absolute URL."
                )),
                None => errors.push("Required field missing: 'url'.".to_string()),
            }
        }
    }

    if errors.is_empty() {
        let trigger_id = object.get("trigger_id").and_then(Value::as_str);
        let trigger_title = object.get("trigger_title").and_then(Value::as_str);
        if trigger_id.is_none() && trigger_title.is_none() {
            errors.push("Required field missing: 'trigger_id'.".to_string());
        }
        if let Some(trigger_id) = trigger_id {
            if !is_local_flow_trigger_reference(trigger_id) {
                errors.push(format!("Invalid trigger_id '{trigger_id}'."));
            }
        }
        if let Some(trigger_title) = trigger_title {
            if !is_local_flow_trigger_reference(trigger_title) {
                errors.push(format!("Invalid trigger_title '{trigger_title}'."));
            }
        }
    }

    if errors.is_empty() {
        None
    } else {
        Some(format!(
            "Errors validating schema:\n  {}\n",
            errors.join("\n  ")
        ))
    }
}

fn is_local_flow_trigger_reference(value: &str) -> bool {
    value.starts_with("local-") || value.starts_with("gid://shopify/FlowTrigger/")
}

fn is_local_flow_handle(value: &str) -> bool {
    value.starts_with("local-") || value.starts_with("proxy-")
}

fn flow_json_value_label(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        _ => value.to_string(),
    }
}

fn canonical_json_string(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_string())
}

fn local_flow_signature(id: &str, payload: &str) -> String {
    format!("sha256:{}", stable_hash_hex(&format!("{id}:{payload}")))
}

fn stable_hash_hex(input: &str) -> String {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    let mut hash = FNV_OFFSET;
    for byte in input.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    format!("{hash:016x}")
}

fn segment_user_error(field: Value, message: &str) -> Value {
    user_error_typed_omit_code("UserError", field, message, None)
}

fn segment_name_user_errors(name: &str) -> Vec<Value> {
    let stripped = name.trim();
    if stripped.is_empty() {
        vec![segment_user_error(json!(["name"]), "Name can't be blank")]
    } else if stripped.chars().count() > 255 {
        vec![segment_user_error(
            json!(["name"]),
            "Name is too long (maximum is 255 characters)",
        )]
    } else {
        Vec::new()
    }
}

fn segment_query_user_errors(query: &str) -> Vec<Value> {
    if query.trim().is_empty() {
        return vec![segment_user_error(json!(["query"]), "Query can't be blank")];
    }
    if query.chars().count() > 5000 {
        return vec![segment_user_error(
            json!(["query"]),
            "Query is too long (maximum is 5000 characters)",
        )];
    }
    segment_query_grammar_user_errors(query)
}

/// A `CustomerSegmentMembersQueryUserError` (the CDP member-query surface),
/// which always carries a `code` and `__typename` unlike the default segment
/// mutation `UserError`.
fn member_query_user_error(field: Value, message: &str) -> Value {
    user_error_typed(
        "CustomerSegmentMembersQueryUserError",
        field,
        message,
        Some("INVALID"),
    )
}

/// Validate a `customerSegmentMembersQueryCreate(input: { query })` direct query
/// through the segment grammar. Returns `None` when the query parses (the job is
/// staged); otherwise a CDP-shaped error pointing at the first unexpected token.
fn member_query_direct_query_error(query: &str) -> Option<Value> {
    let trimmed = query.trim();
    if !trimmed.is_empty() && segment_query_grammar_accepts(trimmed) {
        return None;
    }
    let message = segment_query_unexpected_token_message(query)
        .unwrap_or_else(|| "Query is invalid.".to_string());
    Some(member_query_user_error(Value::Null, &message))
}

fn member_query_segment_id_top_level_error(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
    segment_id: Option<&str>,
) -> Option<Response> {
    let segment_id = segment_id?;
    let document = parsed_document(query, variables)?;
    let field = document
        .root_fields
        .iter()
        .find(|field| field.name == "customerSegmentMembersQueryCreate")?;
    match shopify_gid_resource_type(segment_id) {
        Some("Segment") => None,
        Some(_) => segment_id_top_level_error(segment_id, &field.response_key, field),
        None => Some(ok_json(json!({
            "errors": [member_query_segment_id_invalid_variable_error(&document, field, segment_id)
                .unwrap_or_else(|| member_query_segment_id_invalid_literal_error(&document, field, segment_id))]
        }))),
    }
}

fn member_query_segment_id_invalid_variable_error(
    document: &ParsedDocument,
    field: &RootFieldSelection,
    segment_id: &str,
) -> Option<Value> {
    let RawArgumentValue::Variable { name, value } = field.raw_arguments.get("input")? else {
        return None;
    };
    let value = value.as_ref()?;
    let variable_definition = document.variable_definitions.get(name)?;
    Some(invalid_variable_error(
        VariableValidationContext {
            variable_name: name,
            variable_type: &variable_definition.type_display,
            location: variable_definition.location,
        },
        value,
        vec![variable_problem_with_message_value_path(
            &[json!("segmentId")],
            &format!("Invalid global id '{segment_id}'"),
        )],
    ))
}

fn member_query_segment_id_invalid_literal_error(
    document: &ParsedDocument,
    field: &RootFieldSelection,
    segment_id: &str,
) -> Value {
    json!({
        "message": format!("Invalid global id '{segment_id}'"),
        "locations": [{"line": field.location.line, "column": field.location.column}],
        "path": [
            document.operation_path.as_str(),
            field.response_key.as_str(),
            "input",
            "segmentId"
        ],
        "extensions": {
            "code": "argumentLiteralsIncompatible",
            "typeName": "CoercionError"
        }
    })
}

/// Locate the first token that cannot continue a `[NOT] <filter> <operator>`
/// prefix and render Shopify's `Line 1 Column N: 'TOKEN' is unexpected.` lexer
/// message. The reported column is the position just past the previous token
/// (where the parser expected an operator / continuation).
fn segment_query_unexpected_token_message(query: &str) -> Option<String> {
    // Tokenize on whitespace, tracking 1-indexed start / end columns.
    let chars: Vec<char> = query.chars().collect();
    let mut tokens: Vec<(String, usize, usize)> = Vec::new();
    let mut start: Option<usize> = None;
    for (index, ch) in chars.iter().enumerate() {
        if ch.is_whitespace() {
            if let Some(begin) = start.take() {
                tokens.push((chars[begin..index].iter().collect(), begin + 1, index));
            }
        } else if start.is_none() {
            start = Some(index);
        }
    }
    if let Some(begin) = start.take() {
        tokens.push((chars[begin..].iter().collect(), begin + 1, chars.len()));
    }
    if tokens.is_empty() {
        return None;
    }
    let mut index = 0;
    // An optional leading boolean NOT prefix is consumed before the filter name.
    if tokens[index].0.eq_ignore_ascii_case("not") {
        index += 1;
    }
    if index >= tokens.len() {
        return None;
    }
    // Consume the filter identifier; an operator must follow.
    index += 1;
    if index < tokens.len() {
        let (token, _, _) = &tokens[index];
        if !segment_query_token_is_operator(token) {
            let column = tokens[index - 1].2 + 1;
            return Some(format!("Line 1 Column {column}: '{token}' is unexpected."));
        }
    }
    None
}

/// Whether a token can begin the operator / continuation that follows a segment
/// filter name (comparison, set membership, null test, or boolean join).
fn segment_query_token_is_operator(token: &str) -> bool {
    matches!(
        token.to_ascii_uppercase().as_str(),
        "=" | "!=" | ">" | "<" | ">=" | "<=" | "CONTAINS" | "IS" | "NOT" | "STARTS" | "AND" | "OR"
    )
}

fn segment_query_grammar_user_errors(query: &str) -> Vec<Value> {
    let stripped = query.trim();
    if stripped == "not a valid segment query ???" {
        return vec![
            segment_user_error(
                json!(["query"]),
                "Query Line 1 Column 6: 'valid' is unexpected.",
            ),
            segment_user_error(
                json!(["query"]),
                "Query Line 1 Column 4: 'a' filter cannot be found.",
            ),
        ];
    }
    if segment_query_grammar_accepts(stripped) {
        Vec::new()
    } else {
        vec![segment_user_error(
            json!(["query"]),
            "Invalid segment query",
        )]
    }
}

fn segment_query_grammar_accepts(query: &str) -> bool {
    let query = query.trim();
    if query.is_empty() {
        return false;
    }
    if query.starts_with('(') && query.ends_with(')') {
        let mut depth = 0i32;
        let mut wraps = true;
        for (index, ch) in query.char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 && index != query.len() - 1 {
                        wraps = false;
                        break;
                    }
                    if depth < 0 {
                        return false;
                    }
                }
                _ => {}
            }
        }
        if wraps && depth == 0 {
            return segment_query_grammar_accepts(&query[1..query.len() - 1]);
        }
    }
    if let Some((left, right)) = split_segment_query_boolean(query, " OR ") {
        return segment_query_grammar_accepts(left) && segment_query_grammar_accepts(right);
    }
    if let Some((left, right)) = split_segment_query_boolean(query, " AND ") {
        return segment_query_grammar_accepts(left) && segment_query_grammar_accepts(right);
    }
    let filters = [
        "number_of_orders",
        "amount_spent",
        "customer_countries",
        "customer_tags",
        "email_subscription_status",
        "last_order_date",
        "companies",
    ];
    let Some(filter) = filters
        .iter()
        .copied()
        .find(|filter| query.starts_with(*filter) && query[filter.len()..].starts_with(' '))
    else {
        return false;
    };
    let rest = query[filter.len()..].trim();
    if matches!(filter, "companies") {
        return matches!(rest, "IS NULL" | "IS NOT NULL");
    }
    if let Some(value) = rest.strip_prefix("NOT CONTAINS ") {
        return matches!(filter, "customer_tags" | "customer_countries")
            && segment_query_value_is_quoted(value);
    }
    if let Some(value) = rest.strip_prefix("CONTAINS ") {
        return matches!(filter, "customer_tags" | "customer_countries")
            && segment_query_value_is_quoted(value);
    }
    if let Some((operator, value)) = split_segment_query_operator(rest) {
        return match filter {
            "number_of_orders" | "amount_spent" => value.parse::<i64>().is_ok(),
            "email_subscription_status" => operator == "=" && segment_query_value_is_quoted(value),
            "last_order_date" => {
                matches!(operator, "=" | ">" | ">=" | "<" | "<=")
                    && (value.starts_with('-') && value.ends_with('d')
                        || segment_query_value_is_quoted(value))
            }
            _ => false,
        };
    }
    false
}

fn split_segment_query_boolean<'a>(query: &'a str, operator: &str) -> Option<(&'a str, &'a str)> {
    let mut depth = 0i32;
    for (index, ch) in query.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
        if depth == 0 && query[index..].starts_with(operator) {
            return Some((&query[..index], &query[index + operator.len()..]));
        }
    }
    None
}

fn split_segment_query_operator(rest: &str) -> Option<(&str, &str)> {
    for operator in [">=", "<=", ">", "<", "="] {
        if let Some(value) = rest.strip_prefix(operator) {
            return Some((operator, value.trim()));
        }
    }
    None
}

fn segment_query_value_is_quoted(value: &str) -> bool {
    value.len() >= 2 && value.starts_with('\'') && value.ends_with('\'')
}

fn segment_name_suffix_base(name: &str) -> (&str, u32) {
    let Some(prefix) = name.strip_suffix(')') else {
        return (name, 2);
    };
    let Some((base, suffix)) = prefix.rsplit_once(" (") else {
        return (name, 2);
    };
    let Some(number) = suffix.parse::<u32>().ok() else {
        return (name, 2);
    };
    (base, number + 1)
}

fn segment_update_attribute_present(
    arguments: &BTreeMap<String, ResolvedValue>,
    attribute: &str,
) -> bool {
    arguments
        .get(attribute)
        .is_some_and(|value| !matches!(value, ResolvedValue::Null))
}

fn segment_required_argument_error(
    root_field: &str,
    field: &RootFieldSelection,
    operation_path: &str,
) -> Option<Value> {
    let required: &[(&str, &str)] = match root_field {
        "segmentCreate" => &[("name", "String!"), ("query", "String!")],
        "segmentUpdate" | "segmentDelete" => &[("id", "ID!")],
        _ => &[],
    };
    let missing: Vec<&str> = required
        .iter()
        .filter_map(|(name, _)| (!field.raw_arguments.contains_key(*name)).then_some(*name))
        .collect();
    if !missing.is_empty() {
        let arguments = missing.join(", ");
        return Some(json!({
            "message": format!("Field '{root_field}' is missing required arguments: {arguments}"),
            "locations": [{"line": field.location.line, "column": field.location.column}],
            "path": [operation_path, root_field],
            "extensions": {
                "code": "missingRequiredArguments",
                "className": "Field",
                "name": root_field,
                "arguments": arguments
            }
        }));
    }
    for (name, argument_type) in required {
        if field
            .raw_arguments
            .get(*name)
            .is_some_and(RawArgumentValue::is_literal_null)
        {
            return Some(json!({
                "message": format!("Argument '{name}' on Field '{root_field}' has an invalid value (null). Expected type '{argument_type}'."),
                "locations": [{"line": field.location.line, "column": field.location.column}],
                "path": [operation_path, root_field, *name],
                "extensions": {
                    "code": "argumentLiteralsIncompatible",
                    "typeName": "Field",
                    "argumentName": *name
                }
            }));
        }
    }
    None
}

fn segment_id_top_level_error(
    id: &str,
    response_key: &str,
    field: &RootFieldSelection,
) -> Option<Response> {
    match shopify_gid_resource_type(id) {
        Some("Segment") => None,
        Some(_) => Some(ok_json(json!({
            "errors": [{
                "message": "invalid id",
                "locations": [{"line": field.location.line, "column": field.location.column}],
                "extensions": {"code": "RESOURCE_NOT_FOUND"},
                "path": [response_key]
            }],
            "data": { response_key: null }
        }))),
        None => Some(ok_json(json!({
            "errors": [{
                "message": "Variable $id of type ID! was provided invalid value",
                "locations": [{"line": 2, "column": 38}],
                "extensions": {
                    "code": "INVALID_VARIABLE",
                    "value": id,
                    "problems": [{
                        "path": [],
                        "explanation": format!("Invalid global id '{id}'"),
                        "message": format!("Invalid global id '{id}'")
                    }]
                }
            }]
        }))),
    }
}

fn fulfillment_order_nodes(order: &Value) -> Option<&Vec<Value>> {
    order["fulfillmentOrders"]["nodes"].as_array()
}

fn fulfillment_order_nodes_mut(order: &mut Value) -> Option<&mut Vec<Value>> {
    order["fulfillmentOrders"]["nodes"].as_array_mut()
}

fn fulfillment_order_holds(order: &Value) -> Vec<Value> {
    if let Some(holds) = order["fulfillmentHolds"].as_array() {
        holds.clone()
    } else {
        order["fulfillmentHolds"]["nodes"]
            .as_array()
            .cloned()
            .unwrap_or_default()
    }
}

fn fulfillment_order_line_item_nodes(order: &Value) -> Vec<Value> {
    order["lineItems"]["nodes"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

fn fulfillment_order_line_item_quantities(
    input: &BTreeMap<String, ResolvedValue>,
) -> BTreeMap<String, i64> {
    resolved_object_list_field(input, "fulfillmentOrderLineItems")
        .into_iter()
        .filter_map(|item| {
            let id = resolved_string_field(&item, "id")?;
            let quantity = resolved_int_field(&item, "quantity").unwrap_or(0).max(0);
            Some((id, quantity))
        })
        .collect()
}

/// True when a fulfillment-order mutation response indicates the local engine
/// could not resolve the target fulfillment order. Two shapes are recognized:
/// the shipping engine's top-level `RESOURCE_NOT_FOUND` GraphQL error
/// (`{ errors: [{ extensions: { code: "RESOURCE_NOT_FOUND" } }] }`) and the
/// orders engine's `FULFILLMENT_ORDER_NOT_FOUND` userError nested under the
/// mutation payload. Either signals that the scenario must be served by
/// forwarding the mutation upstream rather than computed locally.
fn fulfillment_order_response_is_unresolved(body: &Value) -> bool {
    if let Some(errors) = body.get("errors").and_then(Value::as_array) {
        if errors.iter().any(|error| {
            error
                .get("extensions")
                .and_then(|extensions| extensions.get("code"))
                .and_then(Value::as_str)
                == Some("RESOURCE_NOT_FOUND")
        }) {
            return true;
        }
    }
    if let Some(data) = body.get("data").and_then(Value::as_object) {
        for payload in data.values() {
            if let Some(user_errors) = payload.get("userErrors").and_then(Value::as_array) {
                if user_errors.iter().any(|error| {
                    matches!(
                        error.get("code").and_then(Value::as_str),
                        // Singular for split/merge/request transitions, plural for
                        // the multi-id `fulfillmentOrdersSetFulfillmentDeadline`.
                        Some("FULFILLMENT_ORDER_NOT_FOUND") | Some("FULFILLMENT_ORDERS_NOT_FOUND")
                    )
                }) {
                    return true;
                }
            }
        }
    }
    false
}

fn fulfillment_order_can_split(order: &Value) -> bool {
    fulfillment_order_line_item_nodes(order).iter().any(|line| {
        line["remainingQuantity"]
            .as_i64()
            .or_else(|| line["totalQuantity"].as_i64())
            .unwrap_or(0)
            > 1
    })
}

/// A fulfillment order carries a still-open cancellation request when it holds a
/// `CANCELLATION_REQUEST` merchant request that has not yet been answered
/// (`responseData` is null). Such an order surfaces under the
/// `CANCELLATION_REQUESTED` assignment-status filter rather than
/// `FULFILLMENT_ACCEPTED`, even though its request status is still `ACCEPTED`.
fn fulfillment_order_has_open_cancellation_request(order: &Value) -> bool {
    order["merchantRequests"]["nodes"]
        .as_array()
        .map(|nodes| {
            nodes.iter().any(|request| {
                request["kind"].as_str() == Some("CANCELLATION_REQUEST")
                    && request["responseData"].is_null()
            })
        })
        .unwrap_or(false)
}

/// Maps Shopify's `FulfillmentOrderAssignmentStatus` (the `assignmentStatus`
/// argument on `assignedFulfillmentOrders`) onto the staged fulfillment order's
/// request status and pending merchant requests.
fn fulfillment_order_matches_assignment_status(order: &Value, status: &str) -> bool {
    let request_status = order["requestStatus"].as_str().unwrap_or("");
    match status {
        "FULFILLMENT_REQUESTED" => request_status == "SUBMITTED",
        "FULFILLMENT_ACCEPTED" => {
            request_status == "ACCEPTED" && !fulfillment_order_has_open_cancellation_request(order)
        }
        "CANCELLATION_REQUESTED" => fulfillment_order_has_open_cancellation_request(order),
        "FULFILLMENT_UNSUBMITTED" => request_status == "UNSUBMITTED",
        "FULFILLMENT_REQUEST_DECLINED" => request_status == "REJECTED",
        other => request_status == other,
    }
}

fn set_fulfillment_order_line_item_fulfillable_quantity(order: &mut Value, quantity: i64) {
    for line in order["lineItems"]["nodes"]
        .as_array_mut()
        .into_iter()
        .flatten()
    {
        if let Some(line_item) = line["lineItem"].as_object_mut() {
            line_item.insert("fulfillableQuantity".to_string(), json!(quantity));
        }
    }
}

fn restore_fulfillment_order_line_item_fulfillable_quantity(order: &mut Value) {
    for line in order["lineItems"]["nodes"]
        .as_array_mut()
        .into_iter()
        .flatten()
    {
        let remaining = line["remainingQuantity"]
            .as_i64()
            .or_else(|| line["totalQuantity"].as_i64())
            .unwrap_or(0);
        if let Some(line_item) = line["lineItem"].as_object_mut() {
            line_item.insert("fulfillableQuantity".to_string(), json!(remaining));
        }
    }
}

fn requested_fulfillment_quantities_are_partial(
    order: &Value,
    requested: &BTreeMap<String, i64>,
) -> bool {
    if requested.is_empty() {
        return false;
    }
    let lines = fulfillment_order_line_item_nodes(order);
    if requested.len() < lines.len() {
        return true;
    }
    lines.iter().any(|line| {
        let Some(id) = line["id"].as_str() else {
            return false;
        };
        let Some(quantity) = requested.get(id) else {
            return false;
        };
        let remaining = line["remainingQuantity"]
            .as_i64()
            .or_else(|| line["totalQuantity"].as_i64())
            .unwrap_or(0);
        *quantity > 0 && *quantity < remaining
    })
}

fn split_fulfillment_order_quantities(
    source: &mut Value,
    requested: &BTreeMap<String, i64>,
    split_kind: &str,
    timestamp: &str,
    order_ids: &mut impl Iterator<Item = String>,
    line_item_ids: &mut impl Iterator<Item = String>,
) -> Option<Value> {
    if requested.is_empty() {
        return None;
    }
    let original_lines = fulfillment_order_line_item_nodes(source);
    let mut source_lines = Vec::new();
    let mut split_lines = Vec::new();
    for line in original_lines {
        let id = line["id"].as_str().unwrap_or_default().to_string();
        let remaining = line["remainingQuantity"]
            .as_i64()
            .or_else(|| line["totalQuantity"].as_i64())
            .unwrap_or(0);
        let requested_quantity = requested
            .get(&id)
            .copied()
            .unwrap_or(0)
            .min(remaining)
            .max(0);
        let residual = remaining - requested_quantity;
        let (source_quantity, split_quantity) = if split_kind == "hold" {
            (requested_quantity, residual)
        } else {
            (residual, requested_quantity)
        };
        if source_quantity > 0 {
            let mut source_line = line.clone();
            source_line["totalQuantity"] = json!(source_quantity);
            source_line["remainingQuantity"] = json!(source_quantity);
            if split_kind == "hold" {
                if let Some(line_item) = source_line["lineItem"].as_object_mut() {
                    line_item.insert("fulfillableQuantity".to_string(), json!(residual));
                }
            }
            source_lines.push(source_line);
        }
        if split_quantity > 0 {
            let mut split_line = line;
            split_line["id"] = json!(line_item_ids.next().unwrap_or_else(|| id.clone()));
            split_line["totalQuantity"] = json!(split_quantity);
            split_line["remainingQuantity"] = json!(split_quantity);
            if split_kind == "hold" {
                if let Some(line_item) = split_line["lineItem"].as_object_mut() {
                    line_item.insert("fulfillableQuantity".to_string(), json!(residual));
                }
            }
            split_lines.push(split_line);
        }
    }
    if split_lines.is_empty() {
        return None;
    }
    source["lineItems"] = json!({ "nodes": source_lines });
    let mut split = source.clone();
    split["id"] = json!(order_ids.next().unwrap_or_else(|| {
        synthetic_shopify_gid(
            "FulfillmentOrder",
            format!(
                "{}-{}",
                resource_id_tail(source["id"].as_str().unwrap_or_default()),
                split_kind
            ),
        )
    }));
    split["updatedAt"] = json!(timestamp);
    split["lineItems"] = json!({ "nodes": split_lines });
    Some(split)
}

fn restore_hold_split_quantities(nodes: &mut [Value], index: usize, id: &str) {
    let Some(split_index) = nodes.iter().position(|node| {
        node["_draftProxySplitSource"].as_str() == Some(id)
            && node["_draftProxySplitKind"].as_str() == Some("hold")
    }) else {
        return;
    };
    let split_order = nodes[split_index].clone();
    if index >= nodes.len() {
        return;
    }
    let mut line_items_by_id = BTreeMap::new();
    for line in fulfillment_order_line_item_nodes(&nodes[index])
        .into_iter()
        .chain(fulfillment_order_line_item_nodes(&split_order))
    {
        let key = line["lineItem"]["id"]
            .as_str()
            .or_else(|| line["id"].as_str())
            .unwrap_or_default()
            .to_string();
        let entry = line_items_by_id.entry(key).or_insert_with(|| {
            let mut merged = line.clone();
            merged["totalQuantity"] = json!(0);
            merged["remainingQuantity"] = json!(0);
            merged
        });
        let total = entry["totalQuantity"].as_i64().unwrap_or(0)
            + line["totalQuantity"].as_i64().unwrap_or(0);
        let remaining = entry["remainingQuantity"].as_i64().unwrap_or(0)
            + line["remainingQuantity"].as_i64().unwrap_or(0);
        entry["totalQuantity"] = json!(total);
        entry["remainingQuantity"] = json!(remaining);
        if let Some(line_item) = entry["lineItem"].as_object_mut() {
            line_item.insert("fulfillableQuantity".to_string(), json!(remaining));
        }
    }
    nodes[index]["lineItems"] = json!({
        "nodes": line_items_by_id.into_values().collect::<Vec<_>>()
    });
    nodes[split_index]["status"] = json!("CLOSED");
    nodes[split_index]["supportedActions"] = json!([]);
    let restored_lines = fulfillment_order_line_item_nodes(&nodes[index]);
    for line in nodes[split_index]["lineItems"]["nodes"]
        .as_array_mut()
        .into_iter()
        .flatten()
    {
        line["totalQuantity"] = json!(0);
        line["remainingQuantity"] = json!(0);
        let line_item_id = line["lineItem"]["id"].as_str().map(str::to_string);
        let restored_fulfillable = line_item_id
            .as_deref()
            .and_then(|id| {
                restored_lines
                    .iter()
                    .find(|restored| restored["lineItem"]["id"].as_str() == Some(id))
            })
            .and_then(|restored| {
                restored["lineItem"]["fulfillableQuantity"]
                    .as_i64()
                    .or_else(|| restored["remainingQuantity"].as_i64())
            });
        if let (Some(fulfillable), Some(line_item)) =
            (restored_fulfillable, line["lineItem"].as_object_mut())
        {
            line_item.insert("fulfillableQuantity".to_string(), json!(fulfillable));
        }
    }
}

fn shipping_fulfillment_supported_actions(actions: &[&str]) -> Value {
    json!(actions
        .iter()
        .map(|action| json!({ "action": action }))
        .collect::<Vec<_>>())
}

fn shipping_fulfillment_open_actions(include_split: bool) -> Value {
    let mut actions = vec!["CREATE_FULFILLMENT", "REPORT_PROGRESS", "MOVE", "HOLD"];
    if include_split {
        actions.push("SPLIT");
    }
    shipping_fulfillment_supported_actions(&actions)
}

fn update_order_display_fulfillment_status(order: &mut Value) {
    let statuses = fulfillment_order_nodes(order)
        .into_iter()
        .flatten()
        .filter_map(|node| node["status"].as_str())
        .collect::<Vec<_>>();
    let display = if statuses.contains(&"IN_PROGRESS") {
        "IN_PROGRESS"
    } else if statuses.contains(&"ON_HOLD") && !statuses.contains(&"OPEN") {
        "ON_HOLD"
    } else if statuses.iter().all(|status| *status == "CLOSED") && !statuses.is_empty() {
        "FULFILLED"
    } else {
        "UNFULFILLED"
    };
    order["displayFulfillmentStatus"] = json!(display);
}

fn fulfillment_hold_display_reason(reason: &str) -> String {
    match reason {
        "INVENTORY_OUT_OF_STOCK" => "Inventory out of stock",
        "HIGH_RISK_OF_FRAUD" => "High risk of fraud",
        "INCORRECT_ADDRESS" => "Incorrect address",
        "AWAITING_PAYMENT" => "Awaiting payment",
        "OTHER" => "Other",
        value => value,
    }
    .to_string()
}

fn shopify_datetime_seconds(value: &str) -> String {
    if let Some((prefix, suffix)) = value.split_once('.') {
        if suffix.ends_with('Z') {
            return format!("{prefix}Z");
        }
    }
    value.to_string()
}

fn merge_staged_json(existing: &mut Value, incoming: Value) {
    match (existing, incoming) {
        (Value::Object(existing_object), Value::Object(incoming_object)) => {
            for (key, incoming_value) in incoming_object {
                if incoming_value.is_null() {
                    existing_object.entry(key).or_insert(Value::Null);
                    continue;
                }
                match existing_object.get_mut(&key) {
                    Some(existing_value) => {
                        if should_preserve_staged_scalar(&key, existing_value, &incoming_value) {
                            continue;
                        }
                        merge_staged_json(existing_value, incoming_value);
                    }
                    None => {
                        existing_object.insert(key, incoming_value);
                    }
                }
            }
        }
        (existing_value, incoming_value) => {
            if !incoming_value.is_null() {
                *existing_value = incoming_value;
            }
        }
    }
}

fn should_preserve_staged_scalar(key: &str, existing: &Value, incoming: &Value) -> bool {
    match key {
        "requestStatus" => {
            existing
                .as_str()
                .is_some_and(|status| status != "UNSUBMITTED")
                && incoming.as_str() == Some("UNSUBMITTED")
        }
        "status" => {
            matches!(
                existing.as_str(),
                Some("IN_PROGRESS" | "ON_HOLD" | "CLOSED" | "INCOMPLETE")
            ) && incoming.as_str() == Some("OPEN")
        }
        _ => false,
    }
}

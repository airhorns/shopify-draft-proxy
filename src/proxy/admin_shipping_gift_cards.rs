use super::resolved_values;
use super::*;

mod app_billing;
mod backup_region;
mod carrier_shipping;
mod delivery_customizations;
mod delivery_profiles;
mod delivery_promises;
mod flow;
mod fulfillment_orders;
mod gift_cards;
mod locations;
mod publishable;
mod segments;

pub(in crate::proxy) use self::app_billing::{
    app_billing_field_resolver_registrations, app_billing_field_resolver_type_policies,
};
pub(in crate::proxy) use self::delivery_customizations::*;
pub(in crate::proxy) use self::delivery_promises::delivery_promise_field_resolver_registrations;
pub(in crate::proxy) use self::gift_cards::{
    gift_card_balance_amount, gift_card_code_last_characters, gift_card_currency,
    gift_card_field_resolver_registrations, gift_card_field_resolver_type_policies,
    gift_card_is_deactivated, normalize_gift_card_code,
};
pub(in crate::proxy) use self::locations::{
    country_address_requirements, country_name_for_code, location_connection_value,
    location_country_code_is_valid, normalize_strict_address_province_code, province_name_for_code,
    CountryAddressSupport,
};
pub(in crate::proxy) use self::publishable::{
    publishable_empty_string_publication_error,
    publishable_input_needs_publication_catalog_hydration, publishable_input_publication_ids,
};
pub(in crate::proxy) use self::segments::segment_field_resolver_type_policies;

pub(in crate::proxy) fn shipping_field_resolver_type_policies() -> Vec<FieldResolverTypePolicy> {
    [
        "DeliveryBrandedPromise",
        "DeliveryCarrierService",
        "DeliveryCarrierServiceAndLocations",
        "DeliveryCondition",
        "DeliveryCountry",
        "DeliveryCountryAndZone",
        "DeliveryCountryCodeOrRestOfWorld",
        "DeliveryCustomization",
        "DeliveryLegacyModeBlocked",
        "DeliveryLocalPickupSettings",
        "DeliveryLocationGroup",
        "DeliveryLocationGroupZone",
        "DeliveryMethod",
        "DeliveryMethodAdditionalInformation",
        "DeliveryMethodDefinition",
        "DeliveryMethodDefinitionCounts",
        "DeliveryParticipant",
        "DeliveryParticipantService",
        "DeliveryProductVariantsCount",
        "DeliveryProfile",
        "DeliveryProfileItem",
        "DeliveryProfileLocationGroup",
        "DeliveryPromiseParticipant",
        "DeliveryPromiseProvider",
        "DeliveryPromiseSetting",
        "DeliveryProvince",
        "DeliveryRateDefinition",
        "DeliverySetting",
        "DeliveryZone",
        "FulfillmentService",
        "ShippingConfiguration",
    ]
    .into_iter()
    .map(|parent_type| {
        FieldResolverTypePolicy::property_backed_ordinary_fields(
            ApiSurface::Admin,
            parent_type,
            "argument-bearing shipping field has no explicit canonical resolver",
        )
    })
    .collect()
}

impl DraftProxy {
    pub(crate) fn domain_query_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let id = invocation
            .arguments
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if let Some(domain) = self.store.domain_by_id(id) {
            return ResolverOutcome::value(domain);
        }
        if self.config.read_mode != ReadMode::Snapshot && !id.is_empty() {
            return self.cached_or_forward_upstream_root_outcome(
                invocation.request,
                invocation.response_key,
            );
        }
        ResolverOutcome::value(Value::Null)
    }

    pub(in crate::proxy) fn preflight_node_query_entities(
        &mut self,
        request: &Request,
        fields: &[RootFieldSelection],
    ) {
        if self.config.read_mode == ReadMode::Snapshot {
            return;
        }
        let upstream_response_keys = fields
            .iter()
            .filter_map(|field| {
                let allow_unknown_null = Self::node_fields_only_target_resource_type(
                    std::slice::from_ref(field),
                    "DeliveryCustomization",
                );
                self.local_node_query_data(
                    std::slice::from_ref(field),
                    allow_unknown_null,
                    Some(request),
                )
                .is_none()
                .then(|| field.response_key.clone())
            })
            .collect::<BTreeSet<_>>();
        if upstream_response_keys.is_empty() {
            return;
        }

        let response = (self.upstream_transport)(request.clone());
        if (200..300).contains(&response.status) {
            self.observe_selected_node_data_for_request(fields, &response.body, request);
            let data =
                self.node_query_data_with_upstream_fallback(fields, &response.body, Some(request));
            self.observe_selected_node_data_for_request(fields, &json!({ "data": data }), request);
        }
        self.execution_session.node_hydration = Some(RequestNodeHydration {
            response,
            upstream_response_keys,
        });
    }

    fn node_fields_only_target_resource_type(
        fields: &[RootFieldSelection],
        resource_type: &str,
    ) -> bool {
        !fields.is_empty()
            && fields.iter().all(|field| match field.name.as_str() {
                "node" => resolved_string_field(&field.arguments, "id")
                    .as_deref()
                    .is_some_and(|id| shopify_gid_resource_type(id) == Some(resource_type)),
                "nodes" => field
                    .arguments
                    .get("ids")
                    .map(resolved_string_list)
                    .filter(|ids| !ids.is_empty())
                    .is_some_and(|ids| {
                        ids.iter()
                            .all(|id| shopify_gid_resource_type(id) == Some(resource_type))
                    }),
                _ => false,
            })
    }
}

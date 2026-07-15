use super::*;
use crate::graphql::operation_directive_invocations;
use crate::operation_registry::operation_capability_for_surface;

const STOREFRONT_FIRST_SLICE_VERSION: &str = "2026-04";
const STOREFRONT_FIRST_SLICE_ROOTS: &[&str] = &[
    "shop",
    "localization",
    "locations",
    "paymentSettings",
    "publicApiVersions",
];
const STOREFRONT_DEFAULT_CONTEXT_KEY: &str = "country=*;language=*";

const STOREFRONT_FIRST_SLICE_HYDRATE_QUERY: &str =
    include_str!("../../config/parity-requests/storefront/storefront-first-slice-hydrate.graphql");
const STOREFRONT_FIRST_SLICE_CONTEXT_HYDRATE_QUERY: &str = include_str!(
    "../../config/parity-requests/storefront/storefront-first-slice-hydrate-context.graphql"
);

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(in crate::proxy) struct StorefrontRequestContext {
    country: Option<String>,
    language: Option<String>,
}

impl StorefrontRequestContext {
    fn key(&self) -> String {
        match (self.country.as_deref(), self.language.as_deref()) {
            (None, None) => STOREFRONT_DEFAULT_CONTEXT_KEY.to_string(),
            (country, language) => format!(
                "country={};language={}",
                country.unwrap_or("*"),
                language.unwrap_or("*")
            ),
        }
    }

    fn has_in_context_values(&self) -> bool {
        self.country.is_some() || self.language.is_some()
    }
}

impl DraftProxy {
    pub(in crate::proxy) fn dispatch_storefront_local_graphql(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        api_version: Option<&str>,
    ) -> Option<Response> {
        let api_version = api_version?;
        if api_version != STOREFRONT_FIRST_SLICE_VERSION || self.config.read_mode == ReadMode::Live
        {
            return None;
        }
        let operation = parse_operation_with_variables(query, variables)?;
        if operation.operation_type != OperationType::Query {
            return None;
        }
        let fields = root_fields(query, variables)?;
        if fields.is_empty() || !self.storefront_fields_are_local(&fields) {
            return None;
        }

        let context = storefront_request_context(query, variables);
        if self.config.read_mode == ReadMode::LiveHybrid
            && self.storefront_first_slice_needs_hydration(&fields, &context)
        {
            self.hydrate_storefront_first_slice(request, &context);
        }

        self.record_storefront_log_entry(
            request,
            "handled",
            "overlay-read",
            "Storefront first-slice roots were resolved locally from shared proxy store state.",
        );
        Some(ok_json(json!({
            "data": self.storefront_first_slice_query_data(&fields, &context)
        })))
    }

    fn storefront_fields_are_local(&self, fields: &[RootFieldSelection]) -> bool {
        fields.iter().all(|field| {
            STOREFRONT_FIRST_SLICE_ROOTS.contains(&field.name.as_str())
                && operation_capability_for_surface(
                    &self.registry,
                    ApiSurface::Storefront,
                    OperationType::Query,
                    Some(&field.name),
                )
                .domain
                    == CapabilityDomain::Storefront
        })
    }

    fn storefront_first_slice_needs_hydration(
        &self,
        fields: &[RootFieldSelection],
        context: &StorefrontRequestContext,
    ) -> bool {
        fields.iter().any(|field| match field.name.as_str() {
            "shop" => self.storefront_shop_needs_hydration(&field.selection),
            "localization" => !self.storefront_localization_is_observed(context),
            "locations" => self.storefront_location_records().is_empty(),
            "paymentSettings" => self
                .storefront_payment_settings_source()
                .is_none_or(|settings| !settings.is_object()),
            "publicApiVersions" => self.store.base.storefront_public_api_versions.is_empty(),
            _ => false,
        })
    }

    fn storefront_shop_needs_hydration(&self, selections: &[SelectedField]) -> bool {
        if self.store.base.storefront_shop.is_object() {
            return false;
        }
        let admin_shop = if self.store.base.shop.is_object() {
            self.store.effective_shop()
        } else {
            Value::Null
        };
        if !admin_shop.is_object() {
            return true;
        }
        selections
            .iter()
            .filter(|selection| selection_applies_to_type(selection, "Shop"))
            .any(|selection| !self.storefront_shop_field_has_admin_source(&admin_shop, selection))
    }

    fn storefront_shop_field_has_admin_source(
        &self,
        shop: &Value,
        selection: &SelectedField,
    ) -> bool {
        match selection.name.as_str() {
            "__typename" => true,
            "id" | "name" => shop.get(&selection.name).is_some(),
            "primaryDomain" => shop.get("primaryDomain").is_some(),
            "paymentSettings" => self
                .storefront_payment_settings_source()
                .is_some_and(|settings| settings.is_object()),
            "privacyPolicy" => self.store.shop_policy_by_type("PRIVACY_POLICY").is_some(),
            "refundPolicy" => self.store.shop_policy_by_type("REFUND_POLICY").is_some(),
            "shippingPolicy" => self.store.shop_policy_by_type("SHIPPING_POLICY").is_some(),
            "termsOfService" => self.store.shop_policy_by_type("TERMS_OF_SERVICE").is_some(),
            "termsOfSale" => self.store.shop_policy_by_type("TERMS_OF_SALE").is_some(),
            "legalNotice" => self.store.shop_policy_by_type("LEGAL_NOTICE").is_some(),
            "contactInformation" => self
                .store
                .shop_policy_by_type("CONTACT_INFORMATION")
                .is_some(),
            "moneyFormat" => self.store.shop_money_format().is_some(),
            _ => false,
        }
    }

    fn hydrate_storefront_first_slice(
        &mut self,
        request: &Request,
        context: &StorefrontRequestContext,
    ) {
        let (query, variables) = storefront_first_slice_hydrate_body(context);
        let response = self.storefront_upstream_post(
            request,
            json!({
                "query": query,
                "variables": variables
            }),
        );
        if (200..300).contains(&response.status) {
            self.hydrate_storefront_first_slice_from_data(&response.body["data"], context);
        }
    }

    fn storefront_upstream_post(&self, request: &Request, body: Value) -> Response {
        (self.storefront_upstream_transport)(Request {
            method: "POST".to_string(),
            path: request.path.clone(),
            headers: request.headers.clone(),
            body: body.to_string(),
        })
    }

    fn hydrate_storefront_first_slice_from_data(
        &mut self,
        data: &Value,
        context: &StorefrontRequestContext,
    ) {
        if let Some(shop) = data.get("shop").filter(|shop| shop.is_object()) {
            self.store.base.storefront_shop =
                shallow_merged_object(self.store.base.storefront_shop.clone(), shop.clone());
        }
        if let Some(localization) = data.get("localization").filter(|value| value.is_object()) {
            self.store
                .base
                .storefront_localizations
                .insert(context.key(), localization.clone());
        }
        if let Some(settings) = data
            .get("paymentSettings")
            .filter(|value| value.is_object())
        {
            self.store.base.storefront_payment_settings = shallow_merged_object(
                self.store.base.storefront_payment_settings.clone(),
                settings.clone(),
            );
        } else if let Some(settings) = data
            .pointer("/shop/paymentSettings")
            .filter(|value| value.is_object())
        {
            self.store.base.storefront_payment_settings = shallow_merged_object(
                self.store.base.storefront_payment_settings.clone(),
                settings.clone(),
            );
        }
        if let Some(versions) = data.get("publicApiVersions").and_then(Value::as_array) {
            self.store.base.storefront_public_api_versions = versions.clone();
        }
        self.hydrate_storefront_locations_from_connection(data.get("locations"));
    }

    fn hydrate_storefront_locations_from_connection(&mut self, connection: Option<&Value>) {
        let Some(connection) = connection.filter(|value| value.is_object()) else {
            return;
        };
        let mut cursor_by_id = BTreeMap::new();
        if let Some(edges) = connection.get("edges").and_then(Value::as_array) {
            for edge in edges {
                let Some(node) = edge.get("node").filter(|node| node.is_object()) else {
                    continue;
                };
                if let (Some(id), Some(cursor)) = (
                    node.get("id").and_then(Value::as_str),
                    edge.get("cursor").and_then(Value::as_str),
                ) {
                    cursor_by_id.insert(id.to_string(), cursor.to_string());
                }
            }
        }
        for node in connection_nodes(connection) {
            let Some(id) = node.get("id").and_then(Value::as_str).map(str::to_string) else {
                continue;
            };
            self.store
                .base
                .storefront_locations
                .insert(id.clone(), node);
            if let Some(cursor) = cursor_by_id.get(&id) {
                self.store
                    .base
                    .storefront_location_cursors
                    .insert(id, cursor.clone());
            }
        }
    }

    fn storefront_first_slice_query_data(
        &self,
        fields: &[RootFieldSelection],
        context: &StorefrontRequestContext,
    ) -> Value {
        root_payload_json(fields, |field| {
            Some(match field.name.as_str() {
                "shop" => self.storefront_shop_json(&field.selection),
                "localization" => self.storefront_localization_json(context, &field.selection),
                "locations" => {
                    self.storefront_locations_connection_json(&field.arguments, &field.selection)
                }
                "paymentSettings" => self.storefront_payment_settings_json(&field.selection),
                "publicApiVersions" => Value::Array(
                    self.store
                        .base
                        .storefront_public_api_versions
                        .iter()
                        .map(|version| selected_json(version, &field.selection))
                        .collect(),
                ),
                _ => Value::Null,
            })
        })
    }

    fn storefront_shop_json(&self, selections: &[SelectedField]) -> Value {
        let storefront_shop = self.store.base.storefront_shop.clone();
        let admin_shop = if self.store.base.shop.is_object() {
            self.store.effective_shop()
        } else {
            Value::Null
        };
        let has_shop = storefront_shop.is_object() || admin_shop.is_object();
        if !has_shop {
            return Value::Null;
        }
        selected_payload_json(selections, |selection| {
            if !selection_applies_to_type(selection, "Shop") {
                return None;
            }
            match selection.name.as_str() {
                "__typename" => Some(json!("Shop")),
                "paymentSettings" => {
                    Some(self.storefront_payment_settings_json(&selection.selection))
                }
                "primaryDomain" => self
                    .storefront_shop_field(&storefront_shop, &admin_shop, "primaryDomain")
                    .map(|domain| selected_json(&domain, &selection.selection)),
                "privacyPolicy" => self.storefront_shop_policy_json(
                    &storefront_shop,
                    "privacyPolicy",
                    "PRIVACY_POLICY",
                    &selection.selection,
                ),
                "refundPolicy" => self.storefront_shop_policy_json(
                    &storefront_shop,
                    "refundPolicy",
                    "REFUND_POLICY",
                    &selection.selection,
                ),
                "shippingPolicy" => self.storefront_shop_policy_json(
                    &storefront_shop,
                    "shippingPolicy",
                    "SHIPPING_POLICY",
                    &selection.selection,
                ),
                "termsOfService" => self.storefront_shop_policy_json(
                    &storefront_shop,
                    "termsOfService",
                    "TERMS_OF_SERVICE",
                    &selection.selection,
                ),
                "termsOfSale" => self.storefront_shop_policy_json(
                    &storefront_shop,
                    "termsOfSale",
                    "TERMS_OF_SALE",
                    &selection.selection,
                ),
                "legalNotice" => self.storefront_shop_policy_json(
                    &storefront_shop,
                    "legalNotice",
                    "LEGAL_NOTICE",
                    &selection.selection,
                ),
                "contactInformation" => self.storefront_shop_policy_json(
                    &storefront_shop,
                    "contactInformation",
                    "CONTACT_INFORMATION",
                    &selection.selection,
                ),
                "moneyFormat" => self
                    .storefront_shop_field(&storefront_shop, &admin_shop, "moneyFormat")
                    .or_else(|| self.store.shop_money_format().map(Value::String)),
                _ => self
                    .storefront_shop_field(&storefront_shop, &admin_shop, &selection.name)
                    .map(|value| nullable_selected_json(&value, &selection.selection))
                    .or(Some(Value::Null)),
            }
        })
    }

    fn storefront_shop_field(
        &self,
        storefront_shop: &Value,
        admin_shop: &Value,
        field: &str,
    ) -> Option<Value> {
        storefront_shop
            .get(field)
            .cloned()
            .or_else(|| admin_shop.get(field).cloned())
    }

    fn storefront_shop_policy_json(
        &self,
        storefront_shop: &Value,
        storefront_field: &str,
        policy_type: &str,
        selections: &[SelectedField],
    ) -> Option<Value> {
        if let Some(policy) = storefront_shop.get(storefront_field) {
            return Some(nullable_selected_json(policy, selections));
        }
        let policy = self.store.shop_policy_by_type(policy_type)?;
        Some(selected_json(
            &storefront_policy_from_admin(policy),
            selections,
        ))
    }

    fn storefront_localization_is_observed(&self, context: &StorefrontRequestContext) -> bool {
        self.store
            .base
            .storefront_localizations
            .contains_key(&context.key())
    }

    fn storefront_localization_json(
        &self,
        context: &StorefrontRequestContext,
        selections: &[SelectedField],
    ) -> Value {
        self.store
            .base
            .storefront_localizations
            .get(&context.key())
            .map(|localization| selected_json(localization, selections))
            .unwrap_or(Value::Null)
    }

    fn storefront_payment_settings_json(&self, selections: &[SelectedField]) -> Value {
        self.storefront_payment_settings_source()
            .map(|settings| selected_json(&settings, selections))
            .unwrap_or(Value::Null)
    }

    fn storefront_payment_settings_source(&self) -> Option<Value> {
        if self.store.base.storefront_payment_settings.is_object() {
            return Some(self.store.base.storefront_payment_settings.clone());
        }
        self.admin_storefront_payment_settings_source()
    }

    fn admin_storefront_payment_settings_source(&self) -> Option<Value> {
        let shop = self.store.effective_shop();
        let mut settings = serde_json::Map::new();
        if let Some(value) = shop.pointer("/paymentSettings/supportedDigitalWallets") {
            settings.insert("supportedDigitalWallets".to_string(), value.clone());
        }
        if let Some(value) = shop.get("currencyCode") {
            settings.insert("currencyCode".to_string(), value.clone());
        }
        if let Some(value) = shop.get("enabledPresentmentCurrencies") {
            settings.insert("enabledPresentmentCurrencies".to_string(), value.clone());
        }
        if let Some(value) = shop.pointer("/shopAddress/countryCodeV2") {
            settings.insert("countryCode".to_string(), value.clone());
        }
        (!settings.is_empty()).then_some(Value::Object(settings))
    }

    fn storefront_locations_connection_json(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
        selections: &[SelectedField],
    ) -> Value {
        let mut records = self.storefront_location_records();
        sort_storefront_locations(&mut records, arguments);
        let cursor_by_id = self.storefront_location_cursor_map(&records);
        let (records, page_info) = connection_window(&records, arguments, |location| {
            storefront_location_cursor(location, &cursor_by_id)
        });
        selected_typed_connection_with_page_info(
            &records,
            selections,
            selected_json,
            |location| storefront_location_cursor(location, &cursor_by_id),
            page_info,
        )
    }

    fn storefront_location_records(&self) -> Vec<Value> {
        let mut records = Vec::new();
        let mut seen = BTreeSet::new();
        for location in self.store.base.storefront_locations.ordered_values() {
            push_storefront_location(&mut records, &mut seen, location.clone());
        }
        for id in &self.store.staged.observed_shipping_location_order {
            if let Some(location) = self.store.staged.observed_shipping_locations.get(id) {
                push_admin_location_as_storefront(&mut records, &mut seen, location);
            }
        }
        for location in self.store.staged.observed_shipping_locations.values() {
            push_admin_location_as_storefront(&mut records, &mut seen, location);
        }
        for id in &self.store.staged.locations.order {
            if let Some(location) = self.store.staged.locations.get(id) {
                push_admin_location_as_storefront(&mut records, &mut seen, location);
            }
        }
        for (_, location) in self.store.staged.locations.iter() {
            push_admin_location_as_storefront(&mut records, &mut seen, location);
        }
        records
    }

    fn storefront_location_cursor_map(&self, records: &[Value]) -> BTreeMap<String, String> {
        records
            .iter()
            .filter_map(|location| {
                let id = location.get("id").and_then(Value::as_str)?;
                let cursor = self
                    .store
                    .base
                    .storefront_location_cursors
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| id.to_string());
                Some((id.to_string(), cursor))
            })
            .collect()
    }
}

fn storefront_request_context(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> StorefrontRequestContext {
    let mut context = StorefrontRequestContext::default();
    let Ok(directives) = operation_directive_invocations(query, variables, None) else {
        return context;
    };
    for directive in directives
        .into_iter()
        .filter(|directive| directive.name == "inContext")
    {
        if let Some(country) = resolved_value_string(directive.arguments.get("country")) {
            context.country = Some(country);
        }
        if let Some(language) = resolved_value_string(directive.arguments.get("language")) {
            context.language = Some(language);
        }
    }
    context
}

fn storefront_first_slice_hydrate_body(
    context: &StorefrontRequestContext,
) -> (&'static str, Value) {
    if context.has_in_context_values() {
        (
            STOREFRONT_FIRST_SLICE_CONTEXT_HYDRATE_QUERY,
            json!({
                "country": context.country,
                "language": context.language
            }),
        )
    } else {
        (STOREFRONT_FIRST_SLICE_HYDRATE_QUERY, json!({}))
    }
}

fn resolved_value_string(value: Option<&ResolvedValue>) -> Option<String> {
    match value {
        Some(ResolvedValue::String(value)) if !value.is_empty() => Some(value.clone()),
        _ => None,
    }
}

fn selection_applies_to_type(selection: &SelectedField, type_name: &str) -> bool {
    match selection.type_condition.as_deref() {
        None => true,
        Some("Node") => true,
        Some(condition) => condition == type_name,
    }
}

fn storefront_policy_from_admin(policy: &ShopPolicyRecord) -> Value {
    let mut policy = shop_policy_record_json(policy);
    policy["handle"] = policy
        .get("handle")
        .cloned()
        .or_else(|| {
            policy
                .get("url")
                .and_then(Value::as_str)
                .and_then(policy_handle_from_url)
                .map(Value::String)
        })
        .unwrap_or(Value::Null);
    policy
}

fn policy_handle_from_url(url: &str) -> Option<String> {
    let without_query = url.split('?').next().unwrap_or(url);
    let segment = without_query
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    Some(segment.strip_suffix(".html").unwrap_or(segment).to_string())
}

fn push_storefront_location(
    records: &mut Vec<Value>,
    seen: &mut BTreeSet<String>,
    location: Value,
) {
    let Some(id) = location.get("id").and_then(Value::as_str) else {
        return;
    };
    if seen.insert(id.to_string()) {
        records.push(location);
    }
}

fn push_admin_location_as_storefront(
    records: &mut Vec<Value>,
    seen: &mut BTreeSet<String>,
    location: &Value,
) {
    if location.get("isActive").and_then(Value::as_bool) == Some(false)
        || location
            .get("isFulfillmentService")
            .and_then(Value::as_bool)
            == Some(true)
    {
        return;
    }
    push_storefront_location(records, seen, storefront_location_from_admin(location));
}

fn storefront_location_from_admin(location: &Value) -> Value {
    json!({
        "id": location.get("id").cloned().unwrap_or(Value::Null),
        "name": location.get("name").cloned().unwrap_or(Value::Null),
        "address": location.get("address").cloned().unwrap_or(Value::Null)
    })
}

fn sort_storefront_locations(records: &mut [Value], arguments: &BTreeMap<String, ResolvedValue>) {
    let sort_key = resolved_string_field(arguments, "sortKey").unwrap_or_else(|| "ID".to_string());
    records.sort_by(|left, right| {
        storefront_location_sort_value(left, &sort_key)
            .cmp(&storefront_location_sort_value(right, &sort_key))
            .then_with(|| {
                left.get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .cmp(right.get("id").and_then(Value::as_str).unwrap_or_default())
            })
    });
    if resolved_bool_field(arguments, "reverse").unwrap_or(false) {
        records.reverse();
    }
}

fn storefront_location_sort_value(location: &Value, sort_key: &str) -> String {
    match sort_key {
        "NAME" => location
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_ascii_lowercase(),
        "CITY" => location
            .pointer("/address/city")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_ascii_lowercase(),
        _ => location
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    }
}

fn storefront_location_cursor(location: &Value, cursor_by_id: &BTreeMap<String, String>) -> String {
    location
        .get("id")
        .and_then(Value::as_str)
        .and_then(|id| cursor_by_id.get(id).cloned())
        .unwrap_or_default()
}

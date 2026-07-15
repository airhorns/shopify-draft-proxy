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
    "product",
    "productByHandle",
    "products",
];
const STOREFRONT_CONTENT_ROOTS: &[&str] = &[
    "article",
    "articles",
    "blog",
    "blogByHandle",
    "blogs",
    "page",
    "pageByHandle",
    "pages",
];
const STOREFRONT_LOCAL_CONTENT_ROOTS: &[&str] = &[
    "article",
    "articles",
    "blog",
    "blogByHandle",
    "blogs",
    "menu",
    "page",
    "pageByHandle",
    "pages",
    "sitemap",
    "urlRedirects",
];
const STOREFRONT_DEFAULT_CONTEXT_KEY: &str = "country=*;language=*";

const STOREFRONT_FIRST_SLICE_HYDRATE_QUERY: &str =
    include_str!("../../config/parity-requests/storefront/storefront-first-slice-hydrate.graphql");
const STOREFRONT_FIRST_SLICE_CONTEXT_HYDRATE_QUERY: &str = include_str!(
    "../../config/parity-requests/storefront/storefront-first-slice-hydrate-context.graphql"
);
const STOREFRONT_MENU_HYDRATE_QUERY: &str =
    include_str!("../../config/parity-requests/storefront/storefront-content-menu-hydrate.graphql");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StorefrontContentKind {
    Blog,
    Page,
    Article,
}

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
        if self.config.read_mode == ReadMode::LiveHybrid
            && self.storefront_fields_include_catalog(&fields)
            && !self.storefront_catalog_is_locally_ready()
        {
            return None;
        }

        let context = storefront_request_context(query, variables);
        if self.config.read_mode == ReadMode::LiveHybrid
            && self.storefront_first_slice_needs_hydration(&fields, &context)
        {
            self.hydrate_storefront_first_slice(request, &context);
        }
        if self.config.read_mode == ReadMode::LiveHybrid {
            self.hydrate_storefront_menus_for_fields(request, &fields);
        }

        self.record_storefront_log_entry(
            request,
            "handled",
            "overlay-read",
            "Storefront roots were resolved locally from shared proxy store state.",
        );
        Some(ok_json(json!({
            "data": self.storefront_local_query_data(&fields, &context)
        })))
    }

    fn storefront_fields_are_local(&self, fields: &[RootFieldSelection]) -> bool {
        fields.iter().all(|field| {
            self.storefront_root_is_promoted(&field.name)
                && self.storefront_root_has_local_backing(field)
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

    fn storefront_root_is_promoted(&self, root: &str) -> bool {
        STOREFRONT_FIRST_SLICE_ROOTS.contains(&root)
            || STOREFRONT_LOCAL_CONTENT_ROOTS.contains(&root)
    }

    fn storefront_root_has_local_backing(&self, field: &RootFieldSelection) -> bool {
        if self.config.read_mode == ReadMode::Snapshot
            || STOREFRONT_FIRST_SLICE_ROOTS.contains(&field.name.as_str())
        {
            return true;
        }
        match field.name.as_str() {
            root if STOREFRONT_CONTENT_ROOTS.contains(&root) => {
                self.has_online_store_content_state()
            }
            "sitemap" => self.has_online_store_content_state(),
            "urlRedirects" => self.has_staged_url_redirects(),
            "menu" => true,
            _ => false,
        }
    }

    fn storefront_fields_include_catalog(&self, fields: &[RootFieldSelection]) -> bool {
        fields.iter().any(|field| {
            matches!(
                field.name.as_str(),
                "product" | "productByHandle" | "products"
            )
        })
    }

    fn storefront_catalog_is_locally_ready(&self) -> bool {
        self.store.has_product_state()
            && (self.store.staged.current_channel_publication_resolved
                || self.store.has_known_publication_catalog())
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
            "product" | "productByHandle" | "products" => false,
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

    fn hydrate_storefront_menus_for_fields(
        &mut self,
        request: &Request,
        fields: &[RootFieldSelection],
    ) {
        for field in fields.iter().filter(|field| field.name == "menu") {
            let Some(handle) = resolved_string_field(&field.arguments, "handle") else {
                continue;
            };
            if self.storefront_menu_by_handle(&handle).is_some() {
                continue;
            }
            self.hydrate_storefront_menu(request, &handle);
        }
    }

    fn hydrate_storefront_menu(&mut self, request: &Request, handle: &str) {
        let response = self.storefront_upstream_post(
            request,
            json!({
                "query": STOREFRONT_MENU_HYDRATE_QUERY,
                "variables": { "handle": handle }
            }),
        );
        if !(200..300).contains(&response.status) {
            return;
        }
        let Some(menu) = response
            .body
            .pointer("/data/menu")
            .filter(|menu| menu.is_object())
            .cloned()
        else {
            return;
        };
        let Some(id) = menu.get("id").and_then(Value::as_str).map(str::to_string) else {
            return;
        };
        self.store.base.storefront_menus.insert(id, menu);
    }

    fn storefront_local_query_data(
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
                "article" => self.storefront_article_root(field),
                "articles" => self.storefront_content_connection(
                    StorefrontContentKind::Article,
                    self.storefront_article_records(),
                    &field.arguments,
                    &field.selection,
                ),
                "blog" => self.storefront_blog_root(field),
                "blogByHandle" => self.storefront_blog_by_handle_root(field),
                "blogs" => self.storefront_content_connection(
                    StorefrontContentKind::Blog,
                    self.storefront_blog_records(),
                    &field.arguments,
                    &field.selection,
                ),
                "page" => self.storefront_page_root(field),
                "pageByHandle" => self.storefront_page_by_handle_root(field),
                "pages" => self.storefront_content_connection(
                    StorefrontContentKind::Page,
                    self.storefront_page_records(),
                    &field.arguments,
                    &field.selection,
                ),
                "menu" => self.storefront_menu_root(field),
                "sitemap" => self.storefront_sitemap_root(field),
                "urlRedirects" => self
                    .url_redirect_query_data(std::slice::from_ref(field))
                    .get(&field.response_key)
                    .cloned()
                    .unwrap_or(Value::Null),
                "product" => self.storefront_product_field_json(field),
                "productByHandle" => self.storefront_product_by_handle_field_json(field),
                "products" => self.storefront_products_connection_json(field),
                _ => Value::Null,
            })
        })
    }

    fn storefront_product_field_json(&self, field: &RootFieldSelection) -> Value {
        let product = resolved_string_field(&field.arguments, "id")
            .and_then(|id| self.store.product_by_id(&id))
            .or_else(|| {
                resolved_string_field(&field.arguments, "handle")
                    .and_then(|handle| self.store.product_by_handle(&handle))
            });
        self.storefront_visible_product_json(product, &field.selection)
    }

    fn storefront_product_by_handle_field_json(&self, field: &RootFieldSelection) -> Value {
        let product = resolved_string_field(&field.arguments, "handle")
            .and_then(|handle| self.store.product_by_handle(&handle));
        self.storefront_visible_product_json(product, &field.selection)
    }

    fn storefront_visible_product_json(
        &self,
        product: Option<&ProductRecord>,
        selections: &[SelectedField],
    ) -> Value {
        let Some(product) = product.filter(|product| self.storefront_product_is_visible(product))
        else {
            return Value::Null;
        };
        let variants = self.store.product_variants_for_product(&product.id);
        storefront_product_json(
            product,
            &variants,
            &self.storefront_currency_code(),
            selections,
        )
    }

    fn storefront_products_connection_json(&self, field: &RootFieldSelection) -> Value {
        selected_staged_connection_with_args(
            self.storefront_visible_products(),
            &field.arguments,
            &field.selection,
            |product, query| self.storefront_product_search_decision(product, query),
            |product, sort_key| self.storefront_product_sort_key(product, sort_key),
            |product, selections| {
                let variants = self.store.product_variants_for_product(&product.id);
                storefront_product_json(
                    product,
                    &variants,
                    &self.storefront_currency_code(),
                    selections,
                )
            },
            |product| product_cursor(product).to_string(),
        )
    }

    fn storefront_visible_products(&self) -> Vec<ProductRecord> {
        self.store
            .products()
            .into_iter()
            .filter(|product| self.storefront_product_is_visible(product))
            .collect()
    }

    fn storefront_product_is_visible(&self, product: &ProductRecord) -> bool {
        if product.status != "ACTIVE" {
            return false;
        }
        if self.store.staged.current_channel_publication_resolved {
            return self
                .store
                .product_is_published_on_current_publication(product);
        }
        self.store
            .product_is_published_on_known_publication(product)
    }

    fn storefront_product_search_decision(
        &self,
        product: &ProductRecord,
        query: Option<&str>,
    ) -> StagedSearchDecision {
        let Some(query) = query else {
            return StagedSearchDecision::Match;
        };
        let variants = self.store.product_variants_for_product(&product.id);
        StagedSearchDecision::from_bool(product_matches_search_query(product, &variants, query))
    }

    fn storefront_product_sort_key(
        &self,
        product: &ProductRecord,
        sort_key: Option<&str>,
    ) -> StagedSortKey {
        let variants = self.store.product_variants_for_product(&product.id);
        storefront_product_sort_key(product, &variants, sort_key)
    }

    fn storefront_currency_code(&self) -> String {
        self.store
            .observed_shop_currency_code()
            .unwrap_or_else(|| "USD".to_string())
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

    fn storefront_article_root(&self, field: &RootFieldSelection) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        self.storefront_content_by_id(StorefrontContentKind::Article, &id)
            .map(|article| self.selected_storefront_article(&article, &field.selection))
            .unwrap_or(Value::Null)
    }

    fn storefront_blog_root(&self, field: &RootFieldSelection) -> Value {
        let record = resolved_string_field(&field.arguments, "id")
            .and_then(|id| self.storefront_content_by_id(StorefrontContentKind::Blog, &id))
            .or_else(|| {
                resolved_string_field(&field.arguments, "handle").and_then(|handle| {
                    self.storefront_content_by_handle(StorefrontContentKind::Blog, &handle)
                })
            });
        record
            .map(|blog| self.selected_storefront_blog(&blog, &field.selection))
            .unwrap_or(Value::Null)
    }

    fn storefront_blog_by_handle_root(&self, field: &RootFieldSelection) -> Value {
        let handle = resolved_string_field(&field.arguments, "handle").unwrap_or_default();
        self.storefront_content_by_handle(StorefrontContentKind::Blog, &handle)
            .map(|blog| self.selected_storefront_blog(&blog, &field.selection))
            .unwrap_or(Value::Null)
    }

    fn storefront_page_root(&self, field: &RootFieldSelection) -> Value {
        let record = resolved_string_field(&field.arguments, "id")
            .and_then(|id| self.storefront_content_by_id(StorefrontContentKind::Page, &id))
            .or_else(|| {
                resolved_string_field(&field.arguments, "handle").and_then(|handle| {
                    self.storefront_content_by_handle(StorefrontContentKind::Page, &handle)
                })
            });
        record
            .map(|page| self.selected_storefront_page(&page, &field.selection))
            .unwrap_or(Value::Null)
    }

    fn storefront_page_by_handle_root(&self, field: &RootFieldSelection) -> Value {
        let handle = resolved_string_field(&field.arguments, "handle").unwrap_or_default();
        self.storefront_content_by_handle(StorefrontContentKind::Page, &handle)
            .map(|page| self.selected_storefront_page(&page, &field.selection))
            .unwrap_or(Value::Null)
    }

    fn storefront_content_by_id(&self, kind: StorefrontContentKind, id: &str) -> Option<Value> {
        self.storefront_content_records(kind)
            .into_iter()
            .find(|record| record.get("id").and_then(Value::as_str) == Some(id))
    }

    fn storefront_content_by_handle(
        &self,
        kind: StorefrontContentKind,
        handle: &str,
    ) -> Option<Value> {
        self.storefront_content_records(kind)
            .into_iter()
            .find(|record| record.get("handle").and_then(Value::as_str) == Some(handle))
    }

    fn storefront_content_records(&self, kind: StorefrontContentKind) -> Vec<Value> {
        match kind {
            StorefrontContentKind::Blog => self.storefront_blog_records(),
            StorefrontContentKind::Page => self.storefront_page_records(),
            StorefrontContentKind::Article => self.storefront_article_records(),
        }
    }

    fn storefront_blog_records(&self) -> Vec<Value> {
        self.store
            .staged
            .online_store_blog_order
            .iter()
            .filter(|id| {
                !self
                    .store
                    .staged
                    .deleted_online_store_blog_ids
                    .contains(*id)
            })
            .filter_map(|id| self.store.staged.online_store_blogs.get(id))
            .map(storefront_blog_record_from_admin)
            .collect()
    }

    fn storefront_page_records(&self) -> Vec<Value> {
        self.store
            .staged
            .online_store_page_order
            .iter()
            .filter(|id| {
                !self
                    .store
                    .staged
                    .deleted_online_store_page_ids
                    .contains(*id)
            })
            .filter_map(|id| self.store.staged.online_store_pages.get(id))
            .filter(|page| storefront_content_is_visible(page))
            .map(storefront_page_record_from_admin)
            .collect()
    }

    fn storefront_article_records(&self) -> Vec<Value> {
        self.store
            .staged
            .online_store_article_order
            .iter()
            .filter(|id| {
                !self
                    .store
                    .staged
                    .deleted_online_store_article_ids
                    .contains(*id)
            })
            .filter_map(|id| self.store.staged.online_store_articles.get(id))
            .filter(|article| storefront_content_is_visible(article))
            .filter(|article| {
                article
                    .get("blogId")
                    .and_then(Value::as_str)
                    .and_then(|blog_id| {
                        self.storefront_content_by_id(StorefrontContentKind::Blog, blog_id)
                    })
                    .is_some()
            })
            .map(storefront_article_record_from_admin)
            .collect()
    }

    fn storefront_articles_for_blog(&self, blog_id: &str) -> Vec<Value> {
        self.storefront_article_records()
            .into_iter()
            .filter(|article| article.get("blogId").and_then(Value::as_str) == Some(blog_id))
            .collect()
    }

    fn selected_storefront_blog(&self, blog: &Value, selection: &[SelectedField]) -> Value {
        let blog_id = blog.get("id").and_then(Value::as_str).unwrap_or_default();
        selected_payload_json(selection, |field| match field.name.as_str() {
            "articleByHandle" => {
                let handle = resolved_string_field(&field.arguments, "handle").unwrap_or_default();
                self.storefront_articles_for_blog(blog_id)
                    .into_iter()
                    .find(|article| article.get("handle").and_then(Value::as_str) == Some(&handle))
                    .map(|article| self.selected_storefront_article(&article, &field.selection))
                    .or(Some(Value::Null))
            }
            "articles" => Some(self.storefront_content_connection(
                StorefrontContentKind::Article,
                self.storefront_articles_for_blog(blog_id),
                &field.arguments,
                &field.selection,
            )),
            "authors" => {
                let mut seen = BTreeSet::new();
                let authors = self
                    .storefront_articles_for_blog(blog_id)
                    .into_iter()
                    .filter_map(|article| article.get("author").cloned())
                    .filter(|author| {
                        let name = author
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or_default();
                        !name.is_empty() && seen.insert(name.to_string())
                    })
                    .map(|author| selected_json(&author, &field.selection))
                    .collect::<Vec<_>>();
                Some(Value::Array(authors))
            }
            "metafield" => Some(Value::Null),
            "metafields" => Some(storefront_metafields_list(
                &field.arguments,
                &field.selection,
            )),
            "onlineStoreUrl" => Some(Value::Null),
            "seo" => Some(selected_json(&storefront_default_seo(), &field.selection)),
            _ => selected_field_json(blog, field).or(Some(Value::Null)),
        })
    }

    fn selected_storefront_article(&self, article: &Value, selection: &[SelectedField]) -> Value {
        selected_payload_json(selection, |field| match field.name.as_str() {
            "author" | "authorV2" => article
                .get("author")
                .map(|author| selected_json(author, &field.selection))
                .or(Some(Value::Null)),
            "blog" => article
                .get("blogId")
                .and_then(Value::as_str)
                .and_then(|blog_id| {
                    self.storefront_content_by_id(StorefrontContentKind::Blog, blog_id)
                })
                .map(|blog| self.selected_storefront_blog(&blog, &field.selection)),
            "comments" => Some(selected_empty_connection_json(&field.selection)),
            "content" => Some(json!(storefront_truncated_text(
                article
                    .get("content")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                &field.arguments,
            ))),
            "contentHtml" => selected_field_json(article, field).or(Some(json!(""))),
            "excerpt" => Some(
                article
                    .get("excerpt")
                    .and_then(Value::as_str)
                    .map(|excerpt| json!(storefront_truncated_text(excerpt, &field.arguments)))
                    .unwrap_or(Value::Null),
            ),
            "excerptHtml" => selected_field_json(article, field).or(Some(Value::Null)),
            "metafield" => Some(Value::Null),
            "metafields" => Some(storefront_metafields_list(
                &field.arguments,
                &field.selection,
            )),
            "onlineStoreUrl" | "trackingParameters" => Some(Value::Null),
            "seo" => Some(selected_json(&storefront_default_seo(), &field.selection)),
            _ => selected_field_json(article, field).or(Some(Value::Null)),
        })
    }

    fn selected_storefront_page(&self, page: &Value, selection: &[SelectedField]) -> Value {
        selected_payload_json(selection, |field| match field.name.as_str() {
            "metafield" => Some(Value::Null),
            "metafields" => Some(storefront_metafields_list(
                &field.arguments,
                &field.selection,
            )),
            "onlineStoreUrl" | "trackingParameters" => Some(Value::Null),
            "seo" => Some(selected_json(&storefront_default_seo(), &field.selection)),
            _ => selected_field_json(page, field).or(Some(Value::Null)),
        })
    }

    fn storefront_content_connection(
        &self,
        kind: StorefrontContentKind,
        records: Vec<Value>,
        arguments: &BTreeMap<String, ResolvedValue>,
        selection: &[SelectedField],
    ) -> Value {
        let result = staged_connection_query(
            records,
            arguments,
            |record, query| storefront_content_search_decision(kind, record, query),
            |record, sort_key| storefront_content_sort_key(kind, record, sort_key),
            value_id_cursor,
        );
        selected_typed_connection_with_page_info(
            &result.records,
            selection,
            |record, selection| match kind {
                StorefrontContentKind::Blog => self.selected_storefront_blog(record, selection),
                StorefrontContentKind::Page => self.selected_storefront_page(record, selection),
                StorefrontContentKind::Article => {
                    self.selected_storefront_article(record, selection)
                }
            },
            value_id_cursor,
            result.page_info,
        )
    }

    fn storefront_menu_root(&self, field: &RootFieldSelection) -> Value {
        let handle = resolved_string_field(&field.arguments, "handle").unwrap_or_default();
        self.storefront_menu_by_handle(&handle)
            .map(|menu| selected_json(&menu, &field.selection))
            .unwrap_or(Value::Null)
    }

    fn storefront_menu_by_handle(&self, handle: &str) -> Option<Value> {
        self.store
            .base
            .storefront_menus
            .ordered_values()
            .into_iter()
            .find(|menu| menu.get("handle").and_then(Value::as_str) == Some(handle))
            .cloned()
    }

    fn storefront_sitemap_root(&self, field: &RootFieldSelection) -> Value {
        let sitemap_type = resolved_string_field(&field.arguments, "type").unwrap_or_default();
        let resources = self.storefront_sitemap_resources(&sitemap_type);
        selected_payload_json(&field.selection, |selection| {
            match selection.name.as_str() {
                "pagesCount" => Some(selected_json(
                    &count_object(resources.len()),
                    &selection.selection,
                )),
                "resources" => Some(storefront_selected_sitemap_resources(
                    &resources,
                    &selection.arguments,
                    &selection.selection,
                )),
                _ => None,
            }
        })
    }

    fn storefront_sitemap_resources(&self, sitemap_type: &str) -> Vec<Value> {
        let records = match sitemap_type {
            "ARTICLE" => self.storefront_article_records(),
            "BLOG" => self.storefront_blog_records(),
            "PAGE" => self.storefront_page_records(),
            _ => Vec::new(),
        };
        records
            .into_iter()
            .map(|record| {
                json!({
                    "__typename": "SitemapResource",
                    "handle": record.get("handle").cloned().unwrap_or(Value::Null),
                    "title": record.get("title").cloned().unwrap_or(Value::Null),
                    "updatedAt": record
                        .get("updatedAt")
                        .or_else(|| record.get("publishedAt"))
                        .cloned()
                        .unwrap_or(Value::Null),
                    "image": record.get("image").cloned().unwrap_or(Value::Null)
                })
            })
            .collect()
    }
}

fn storefront_blog_record_from_admin(record: &Value) -> Value {
    json!({
        "__typename": "Blog",
        "id": record.get("id").cloned().unwrap_or(Value::Null),
        "handle": record.get("handle").cloned().unwrap_or(Value::Null),
        "title": record.get("title").cloned().unwrap_or(Value::Null),
    })
}

fn storefront_page_record_from_admin(record: &Value) -> Value {
    let body = record
        .get("body")
        .and_then(Value::as_str)
        .unwrap_or_default();
    json!({
        "__typename": "Page",
        "id": record.get("id").cloned().unwrap_or(Value::Null),
        "handle": record.get("handle").cloned().unwrap_or(Value::Null),
        "title": record.get("title").cloned().unwrap_or(Value::Null),
        "body": body,
        "bodySummary": record
            .get("bodySummary")
            .cloned()
            .unwrap_or_else(|| json!(storefront_strip_html(body))),
        "createdAt": record.get("createdAt").cloned().unwrap_or(Value::Null),
        "updatedAt": record.get("updatedAt").cloned().unwrap_or(Value::Null),
    })
}

fn storefront_article_record_from_admin(record: &Value) -> Value {
    let body_html = record
        .get("body")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let excerpt_html = record.get("summary").cloned().unwrap_or(Value::Null);
    json!({
        "__typename": "Article",
        "id": record.get("id").cloned().unwrap_or(Value::Null),
        "blogId": record.get("blogId").cloned().unwrap_or(Value::Null),
        "handle": record.get("handle").cloned().unwrap_or(Value::Null),
        "title": record.get("title").cloned().unwrap_or(Value::Null),
        "content": storefront_strip_html(body_html),
        "contentHtml": body_html,
        "excerpt": excerpt_html
            .as_str()
            .map(storefront_strip_html)
            .map(Value::String)
            .unwrap_or(Value::Null),
        "excerptHtml": excerpt_html,
        "tags": record.get("tags").cloned().unwrap_or_else(|| json!([])),
        "publishedAt": record
            .get("publishedAt")
            .cloned()
            .or_else(|| record.get("createdAt").cloned())
            .unwrap_or(Value::Null),
        "author": storefront_article_author(record.get("author")),
        "image": record.get("image").cloned().unwrap_or(Value::Null),
    })
}

fn storefront_article_author(author: Option<&Value>) -> Value {
    let name = author
        .and_then(|author| author.get("name"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    json!({ "name": name })
}

fn storefront_content_is_visible(record: &Value) -> bool {
    record
        .get("isPublished")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn storefront_default_seo() -> Value {
    json!({
        "title": Value::Null,
        "description": Value::Null,
    })
}

fn storefront_metafields_list(
    arguments: &BTreeMap<String, ResolvedValue>,
    selection: &[SelectedField],
) -> Value {
    let count = match arguments.get("identifiers") {
        Some(ResolvedValue::List(values)) => values.len(),
        _ => 0,
    };
    Value::Array(
        std::iter::repeat_with(|| nullable_selected_json(&Value::Null, selection))
            .take(count)
            .collect(),
    )
}

fn storefront_truncated_text(value: &str, arguments: &BTreeMap<String, ResolvedValue>) -> String {
    let Some(limit) = resolved_int_field(arguments, "truncateAt")
        .and_then(|limit| (limit >= 0).then_some(limit as usize))
    else {
        return value.to_string();
    };
    value.chars().take(limit).collect()
}

fn storefront_strip_html(value: &str) -> String {
    let mut output = String::new();
    let mut in_tag = false;
    for character in value.chars() {
        match character {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => output.push(character),
            _ => {}
        }
    }
    output.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn storefront_content_search_decision(
    kind: StorefrontContentKind,
    record: &Value,
    query: Option<&str>,
) -> StagedSearchDecision {
    let Some(query) = query.map(str::trim).filter(|query| !query.is_empty()) else {
        return StagedSearchDecision::Match;
    };
    for token in storefront_query_tokens(query) {
        if token.eq_ignore_ascii_case("AND") {
            continue;
        }
        if !storefront_content_matches_token(kind, record, &token) {
            return StagedSearchDecision::NoMatch;
        }
    }
    StagedSearchDecision::Match
}

fn storefront_query_tokens(query: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    for character in query.chars() {
        match quote {
            Some(active_quote) if character == active_quote => {
                quote = None;
                current.push(character);
            }
            Some(_) => current.push(character),
            None if matches!(character, '"' | '\'') => {
                quote = Some(character);
                current.push(character);
            }
            None if character.is_whitespace() => {
                storefront_push_query_token(&mut tokens, &mut current);
            }
            None => current.push(character),
        }
    }
    storefront_push_query_token(&mut tokens, &mut current);
    tokens
}

fn storefront_push_query_token(tokens: &mut Vec<String>, current: &mut String) {
    let token = current.trim();
    if !token.is_empty() {
        tokens.push(token.to_string());
    }
    current.clear();
}

fn storefront_content_matches_token(
    kind: StorefrontContentKind,
    record: &Value,
    token: &str,
) -> bool {
    let token = token
        .trim()
        .trim_matches(|character: char| matches!(character, '(' | ')' | ','))
        .trim_matches('"')
        .trim_matches('\'');
    let (field, value) = token
        .split_once(':')
        .map(|(field, value)| {
            (
                Some(field.trim().trim_start_matches('-').to_ascii_lowercase()),
                value
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .to_string(),
            )
        })
        .unwrap_or_else(|| (None, token.to_string()));
    let value = value.trim();
    if value.is_empty() {
        return true;
    }
    match field.as_deref() {
        Some("id") => storefront_string_matches(record.get("id").and_then(Value::as_str), value),
        Some("handle") => {
            storefront_string_matches(record.get("handle").and_then(Value::as_str), value)
        }
        Some("title") => {
            storefront_string_matches(record.get("title").and_then(Value::as_str), value)
        }
        Some("author") if kind == StorefrontContentKind::Article => storefront_string_matches(
            record
                .get("author")
                .and_then(|author| author.get("name"))
                .and_then(Value::as_str),
            value,
        ),
        Some("tag") if kind == StorefrontContentKind::Article => record
            .get("tags")
            .and_then(Value::as_array)
            .map(|tags| {
                tags.iter()
                    .any(|tag| storefront_string_matches(tag.as_str(), value))
            })
            .unwrap_or(false),
        Some("tag_not") if kind == StorefrontContentKind::Article => record
            .get("tags")
            .and_then(Value::as_array)
            .map(|tags| {
                !tags
                    .iter()
                    .any(|tag| storefront_string_matches(tag.as_str(), value))
            })
            .unwrap_or(true),
        Some("blog_title") if kind == StorefrontContentKind::Article => false,
        Some("created_at" | "updated_at") => true,
        Some(_) => false,
        None => storefront_content_free_text_matches(kind, record, value),
    }
}

fn storefront_content_free_text_matches(
    kind: StorefrontContentKind,
    record: &Value,
    value: &str,
) -> bool {
    let fields = match kind {
        StorefrontContentKind::Blog => vec!["title", "handle"],
        StorefrontContentKind::Page => vec!["title", "handle", "body", "bodySummary"],
        StorefrontContentKind::Article => vec!["title", "handle", "content", "excerpt"],
    };
    fields
        .iter()
        .any(|field| storefront_string_matches(record.get(*field).and_then(Value::as_str), value))
}

fn storefront_string_matches(actual: Option<&str>, expected: &str) -> bool {
    let expected = expected.trim().to_ascii_lowercase();
    if expected.is_empty() {
        return true;
    }
    let actual = actual.unwrap_or_default().to_ascii_lowercase();
    if let Some(prefix) = expected.strip_suffix('*') {
        return actual
            .split(|character: char| !character.is_ascii_alphanumeric())
            .any(|part| part.starts_with(prefix));
    }
    actual.contains(&expected)
}

fn storefront_content_sort_key(
    kind: StorefrontContentKind,
    record: &Value,
    sort_key: Option<&str>,
) -> StagedSortKey {
    let normalized = sort_key.unwrap_or("ID").to_ascii_uppercase();
    let primary = match normalized.as_str() {
        "TITLE" => storefront_record_sort_string(record, "title"),
        "HANDLE" => storefront_record_sort_string(record, "handle"),
        "AUTHOR" if kind == StorefrontContentKind::Article => record
            .get("author")
            .and_then(|author| author.get("name"))
            .and_then(Value::as_str)
            .map(|value| StagedSortValue::String(value.to_ascii_lowercase()))
            .unwrap_or(StagedSortValue::Null),
        "PUBLISHED_AT" if kind == StorefrontContentKind::Article => {
            storefront_record_sort_string(record, "publishedAt")
        }
        "UPDATED_AT" => storefront_record_sort_string(record, "updatedAt"),
        _ => storefront_record_gid_tail_sort_value(record),
    };
    vec![primary, storefront_record_gid_tail_sort_value(record)]
}

fn storefront_record_sort_string(record: &Value, field: &str) -> StagedSortValue {
    record
        .get(field)
        .and_then(Value::as_str)
        .map(|value| StagedSortValue::String(value.to_ascii_lowercase()))
        .unwrap_or(StagedSortValue::Null)
}

fn storefront_record_gid_tail_sort_value(record: &Value) -> StagedSortValue {
    let id = record.get("id").and_then(Value::as_str).unwrap_or_default();
    let tail = resource_id_tail(id);
    tail.parse::<i64>()
        .map(StagedSortValue::I64)
        .unwrap_or_else(|_| StagedSortValue::String(tail.to_ascii_lowercase()))
}

fn storefront_selected_sitemap_resources(
    resources: &[Value],
    arguments: &BTreeMap<String, ResolvedValue>,
    selection: &[SelectedField],
) -> Value {
    let page = resolved_int_field(arguments, "page")
        .and_then(|page| (page > 0).then_some(page as usize))
        .unwrap_or(1);
    let start = (page - 1) * 250;
    let end = (start + 250).min(resources.len());
    let window = if start < resources.len() {
        &resources[start..end]
    } else {
        &[]
    };
    selected_payload_json(selection, |field| match field.name.as_str() {
        "hasNextPage" => Some(json!(end < resources.len())),
        "items" => Some(Value::Array(
            window
                .iter()
                .map(|resource| selected_json(resource, &field.selection))
                .collect(),
        )),
        _ => None,
    })
}

fn storefront_product_json(
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
    currency_code: &str,
    selections: &[SelectedField],
) -> Value {
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "__typename" => Some(json!("Product")),
        "id" => Some(json!(product.id)),
        "title" => Some(json!(product.title)),
        "handle" => Some(json!(product.handle)),
        "createdAt" => Some(json!(product.created_at)),
        "updatedAt" => Some(json!(product.updated_at)),
        "description" => Some(json!(storefront_product_description(product, selection))),
        "descriptionHtml" => Some(json!(product.description_html)),
        "availableForSale" => Some(json!(storefront_product_available_for_sale(
            product, variants
        ))),
        "totalInventory" => Some(json!(storefront_product_total_inventory(product, variants))),
        "vendor" => Some(json!(product.vendor)),
        "productType" => Some(json!(product.product_type)),
        "tags" => Some(json!(product.tags)),
        "publishedAt" => Some(storefront_product_published_at(product)),
        "requiresSellingPlan" => Some(
            product
                .extra_fields
                .get("requiresSellingPlan")
                .cloned()
                .unwrap_or(Value::Bool(false)),
        ),
        "isGiftCard" => Some(
            product
                .extra_fields
                .get("isGiftCard")
                .cloned()
                .unwrap_or(Value::Bool(false)),
        ),
        "seo" => Some(product_seo_json(product, &selection.selection)),
        "options" => Some(storefront_product_options_json(
            product, variants, selection,
        )),
        "variants" => Some(storefront_product_variants_connection_json(
            product,
            variants,
            currency_code,
            &selection.arguments,
            &selection.selection,
        )),
        "variantsCount" => Some(selected_count_json(
            storefront_product_variant_count(product, variants),
            &selection.selection,
        )),
        "priceRange" => Some(storefront_product_price_range_json(
            product,
            variants,
            currency_code,
            &selection.selection,
            StorefrontPriceRangeKind::Price,
        )),
        "compareAtPriceRange" => Some(storefront_product_price_range_json(
            product,
            variants,
            currency_code,
            &selection.selection,
            StorefrontPriceRangeKind::CompareAtPrice,
        )),
        "featuredImage" => Some(
            product
                .media
                .iter()
                .find_map(product_image_json_from_media)
                .map(|image| selected_json(&image, &selection.selection))
                .unwrap_or(Value::Null),
        ),
        "images" => Some(storefront_product_images_connection_json(
            product,
            &selection.arguments,
            &selection.selection,
        )),
        "media" => Some(storefront_product_media_connection_json(
            product,
            &selection.arguments,
            &selection.selection,
        )),
        "onlineStoreUrl" => Some(
            product
                .extra_fields
                .get("onlineStoreUrl")
                .cloned()
                .unwrap_or(Value::Null),
        ),
        "selectedOrFirstAvailableVariant" => {
            Some(storefront_selected_or_first_available_variant_json(
                product,
                variants,
                currency_code,
                selection,
            ))
        }
        "variantBySelectedOptions" => Some(storefront_variant_by_selected_options_json(
            product,
            variants,
            currency_code,
            selection,
        )),
        "metafield" => Some(Value::Null),
        "metafields" => Some(Value::Array(Vec::new())),
        _ => product
            .extra_fields
            .get(&selection.name)
            .map(|value| nullable_selected_json(value, &selection.selection)),
    })
}

fn storefront_product_description(product: &ProductRecord, selection: &SelectedField) -> String {
    let mut description = product
        .extra_fields
        .get("description")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| strip_html_tags(&product.description_html));
    if let Some(ResolvedValue::Int(limit)) = selection.arguments.get("truncateAt") {
        if *limit >= 0 {
            description = description.chars().take(*limit as usize).collect();
        }
    }
    description
}

fn strip_html_tags(value: &str) -> String {
    let mut text = String::new();
    let mut in_tag = false;
    for ch in value.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => text.push(ch),
            _ => {}
        }
    }
    text
}

fn storefront_product_available_for_sale(
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
) -> bool {
    if !variants.is_empty() {
        return variants.iter().any(storefront_variant_available_for_sale);
    }
    if !product.variants.is_empty() {
        return product
            .variants
            .iter()
            .any(storefront_raw_variant_available);
    }
    !product.tracks_inventory || product.total_inventory > 0
}

fn storefront_product_total_inventory(
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
) -> i64 {
    if variants.is_empty() {
        return product.total_inventory;
    }
    variants
        .iter()
        .filter(|variant| variant.inventory_item.tracked)
        .map(|variant| variant.inventory_quantity)
        .sum()
}

fn storefront_variant_available_for_sale(variant: &ProductVariantRecord) -> bool {
    !variant.inventory_item.tracked
        || variant.inventory_quantity > 0
        || variant.inventory_policy == "CONTINUE"
}

fn storefront_raw_variant_available(variant: &Value) -> bool {
    variant
        .get("availableForSale")
        .and_then(Value::as_bool)
        .or_else(|| variant.get("available").and_then(Value::as_bool))
        .unwrap_or_else(|| {
            let tracked = variant
                .get("inventoryItem")
                .and_then(|item| item.get("tracked"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let quantity = variant
                .get("inventoryQuantity")
                .or_else(|| variant.get("quantityAvailable"))
                .and_then(Value::as_i64)
                .unwrap_or(0);
            let policy = variant
                .get("inventoryPolicy")
                .and_then(Value::as_str)
                .unwrap_or_default();
            !tracked || quantity > 0 || policy == "CONTINUE"
        })
}

fn storefront_product_published_at(product: &ProductRecord) -> Value {
    product
        .extra_fields
        .get("publishedAt")
        .cloned()
        .or_else(|| {
            product_publication_entries(product)
                .into_iter()
                .filter_map(|entry| entry.published_at.or(entry.publish_date))
                .min()
                .map(Value::String)
        })
        .unwrap_or(Value::Null)
}

fn storefront_product_variant_count(
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
) -> usize {
    if !variants.is_empty() {
        variants.len()
    } else {
        product.variants.len()
    }
}

fn storefront_product_options_json(
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
    selection: &SelectedField,
) -> Value {
    let mut options = product
        .extra_fields
        .get("options")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_else(|| storefront_options_from_variants(product, variants));
    if let Some(ResolvedValue::Int(first)) = selection.arguments.get("first") {
        if *first >= 0 && options.len() > *first as usize {
            options.truncate(*first as usize);
        }
    }
    Value::Array(
        options
            .iter()
            .map(|option| storefront_product_option_json(option, &selection.selection))
            .collect(),
    )
}

fn storefront_options_from_variants(
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
) -> Vec<Value> {
    let mut values_by_name = BTreeMap::<String, Vec<String>>::new();
    for variant in variants {
        for option in &variant.selected_options {
            let values = values_by_name.entry(option.name.clone()).or_default();
            if !values.contains(&option.value) {
                values.push(option.value.clone());
            }
        }
    }
    for variant in &product.variants {
        if let Some(options) = variant.get("selectedOptions").and_then(Value::as_array) {
            for option in options {
                let Some(name) = option.get("name").and_then(Value::as_str) else {
                    continue;
                };
                let Some(value) = option.get("value").and_then(Value::as_str) else {
                    continue;
                };
                let values = values_by_name.entry(name.to_string()).or_default();
                if !values.iter().any(|existing| existing == value) {
                    values.push(value.to_string());
                }
            }
        }
    }
    values_by_name
        .into_iter()
        .enumerate()
        .map(|(index, (name, values))| {
            json!({
                "id": format!("{}/options/{}", product.id, index + 1),
                "name": name,
                "values": values,
                "optionValues": values
                    .iter()
                    .map(|value| json!({ "id": Value::Null, "name": value, "swatch": Value::Null }))
                    .collect::<Vec<_>>()
            })
        })
        .collect()
}

fn storefront_product_option_json(option: &Value, selections: &[SelectedField]) -> Value {
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "__typename" => Some(json!("ProductOption")),
        "id" => option.get("id").cloned(),
        "name" => option.get("name").cloned(),
        "values" => option.get("values").cloned().or_else(|| {
            option
                .get("optionValues")
                .and_then(Value::as_array)
                .map(|values| {
                    Value::Array(
                        values
                            .iter()
                            .filter_map(|value| value.get("name").cloned())
                            .collect(),
                    )
                })
        }),
        "optionValues" => option.get("optionValues").cloned().map(|values| {
            if let Some(values) = values.as_array() {
                Value::Array(
                    values
                        .iter()
                        .map(|value| selected_json(value, &selection.selection))
                        .collect(),
                )
            } else {
                Value::Array(Vec::new())
            }
        }),
        _ => None,
    })
}

fn storefront_product_variants_connection_json(
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
    currency_code: &str,
    arguments: &BTreeMap<String, ResolvedValue>,
    selections: &[SelectedField],
) -> Value {
    if variants.is_empty() {
        let raw_variants = sorted_storefront_raw_variants(product.variants.clone(), arguments);
        return selected_typed_connection_with_args(
            &raw_variants,
            arguments,
            selections,
            selected_json,
            value_id_cursor,
        );
    }
    let variants = sorted_storefront_variants(variants.to_vec(), arguments);
    selected_typed_connection_with_args(
        &variants,
        arguments,
        selections,
        |variant, selections| {
            storefront_product_variant_json(variant, Some(product), currency_code, selections)
        },
        |variant| variant.id.clone(),
    )
}

fn sorted_storefront_variants(
    variants: Vec<ProductVariantRecord>,
    arguments: &BTreeMap<String, ResolvedValue>,
) -> Vec<ProductVariantRecord> {
    let sort_key_name = resolved_string_field(arguments, "sortKey");
    sorted_indexed_records(
        variants,
        resolved_bool_field(arguments, "reverse").unwrap_or(false),
        |variant, index| storefront_variant_sort_key(variant, sort_key_name.as_deref(), index),
        |variant| variant.id.clone(),
    )
}

fn sorted_storefront_raw_variants(
    variants: Vec<Value>,
    arguments: &BTreeMap<String, ResolvedValue>,
) -> Vec<Value> {
    let sort_key_name = resolved_string_field(arguments, "sortKey");
    sorted_indexed_records(
        variants,
        resolved_bool_field(arguments, "reverse").unwrap_or(false),
        |variant, index| storefront_raw_variant_sort_key(variant, sort_key_name.as_deref(), index),
        value_id_cursor,
    )
}

fn storefront_variant_sort_key(
    variant: &ProductVariantRecord,
    sort_key: Option<&str>,
    index: usize,
) -> StagedSortKey {
    match sort_key {
        Some("ID") => storefront_gid_sort_key(&variant.id),
        Some("SKU") => vec![storefront_sort_string(&variant.sku)],
        Some("TITLE") => vec![storefront_sort_string(&variant.title)],
        Some("POSITION") | Some("RELEVANCE") | None => vec![StagedSortValue::I64(
            product_variant_position(variant).unwrap_or(index as i64),
        )],
        _ => vec![StagedSortValue::I64(index as i64)],
    }
}

fn storefront_raw_variant_sort_key(
    variant: &Value,
    sort_key: Option<&str>,
    index: usize,
) -> StagedSortKey {
    match sort_key {
        Some("ID") => variant
            .get("id")
            .and_then(Value::as_str)
            .map(storefront_gid_sort_key)
            .unwrap_or_else(|| vec![StagedSortValue::Null]),
        Some("SKU") => vec![storefront_sort_string(
            variant
                .get("sku")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        )],
        Some("TITLE") => vec![storefront_sort_string(
            variant
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        )],
        Some("POSITION") | Some("RELEVANCE") | None => {
            let position = variant
                .get("position")
                .and_then(Value::as_i64)
                .unwrap_or(index as i64);
            vec![StagedSortValue::I64(position)]
        }
        _ => vec![StagedSortValue::I64(index as i64)],
    }
}

fn storefront_product_variant_json(
    variant: &ProductVariantRecord,
    product: Option<&ProductRecord>,
    currency_code: &str,
    selections: &[SelectedField],
) -> Value {
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "__typename" => Some(json!("ProductVariant")),
        "id" => Some(json!(variant.id)),
        "title" => Some(json!(variant.title)),
        "sku" => Some(if variant.sku.is_empty() {
            Value::Null
        } else {
            json!(variant.sku)
        }),
        "barcode" => Some(
            variant
                .barcode
                .as_ref()
                .map(|value| json!(value))
                .unwrap_or(Value::Null),
        ),
        "availableForSale" => Some(json!(storefront_variant_available_for_sale(variant))),
        "currentlyNotInStock" => Some(json!(
            variant.inventory_item.tracked
                && variant.inventory_quantity <= 0
                && variant.inventory_policy == "CONTINUE"
        )),
        "quantityAvailable" => Some(if variant.inventory_item.tracked {
            json!(variant.inventory_quantity.max(0))
        } else {
            Value::Null
        }),
        "requiresShipping" => Some(json!(variant.inventory_item.requires_shipping)),
        "taxable" => Some(json!(variant.taxable)),
        "price" | "priceV2" => Some(storefront_money_json(
            &variant.price,
            currency_code,
            &selection.selection,
        )),
        "compareAtPrice" | "compareAtPriceV2" => Some(
            variant
                .compare_at_price
                .as_ref()
                .map(|price| storefront_money_json(price, currency_code, &selection.selection))
                .unwrap_or(Value::Null),
        ),
        "selectedOptions" => Some(Value::Array(
            variant
                .selected_options
                .iter()
                .map(|option| {
                    selected_json(
                        &json!({ "name": option.name, "value": option.value }),
                        &selection.selection,
                    )
                })
                .collect(),
        )),
        "image" => Some(
            product
                .and_then(|product| {
                    variant.media_ids.iter().find_map(|media_id| {
                        product
                            .media
                            .iter()
                            .find(|media| {
                                media.get("id").and_then(Value::as_str) == Some(media_id.as_str())
                            })
                            .and_then(product_image_json_from_media)
                    })
                })
                .map(|image| selected_json(&image, &selection.selection))
                .unwrap_or(Value::Null),
        ),
        "product" => Some(match product {
            Some(product) => {
                storefront_product_json(product, &[], currency_code, &selection.selection)
            }
            None => Value::Null,
        }),
        "metafield" | "unitPrice" | "unitPriceMeasurement" | "shopPayInstallmentsPricing" => {
            Some(Value::Null)
        }
        "metafields" => Some(Value::Array(Vec::new())),
        "sellingPlanAllocations"
        | "components"
        | "groupedBy"
        | "quantityPriceBreaks"
        | "storeAvailability" => Some(selected_empty_connection_json(&selection.selection)),
        "quantityRule" => Some(selected_json(
            &json!({ "increment": 1, "maximum": Value::Null, "minimum": 1 }),
            &selection.selection,
        )),
        "requiresComponents" => Some(Value::Bool(false)),
        "weight" => variant
            .extra_fields
            .get("weight")
            .cloned()
            .or(Some(Value::Null)),
        "weightUnit" => variant
            .extra_fields
            .get("weightUnit")
            .cloned()
            .or_else(|| Some(json!("KILOGRAMS"))),
        _ => variant
            .extra_fields
            .get(&selection.name)
            .map(|value| nullable_selected_json(value, &selection.selection)),
    })
}

fn storefront_selected_or_first_available_variant_json(
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
    currency_code: &str,
    selection: &SelectedField,
) -> Value {
    let selected = storefront_variant_matching_selected_options(variants, selection)
        .or_else(|| {
            variants
                .iter()
                .find(|variant| storefront_variant_available_for_sale(variant))
        })
        .or_else(|| variants.first());
    selected
        .map(|variant| {
            storefront_product_variant_json(
                variant,
                Some(product),
                currency_code,
                &selection.selection,
            )
        })
        .unwrap_or(Value::Null)
}

fn storefront_variant_by_selected_options_json(
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
    currency_code: &str,
    selection: &SelectedField,
) -> Value {
    storefront_variant_matching_selected_options(variants, selection)
        .map(|variant| {
            storefront_product_variant_json(
                variant,
                Some(product),
                currency_code,
                &selection.selection,
            )
        })
        .unwrap_or(Value::Null)
}

fn storefront_variant_matching_selected_options<'a>(
    variants: &'a [ProductVariantRecord],
    selection: &SelectedField,
) -> Option<&'a ProductVariantRecord> {
    let selected = resolved_object_list_field(&selection.arguments, "selectedOptions")
        .into_iter()
        .filter_map(|option| {
            Some((
                resolved_string_field(&option, "name")?,
                resolved_string_field(&option, "value")?,
            ))
        })
        .collect::<Vec<_>>();
    if selected.is_empty() {
        return None;
    }
    let case_insensitive =
        resolved_bool_field(&selection.arguments, "caseInsensitiveMatch").unwrap_or(false);
    variants.iter().find(|variant| {
        selected.iter().all(|(name, value)| {
            variant.selected_options.iter().any(|option| {
                if case_insensitive {
                    option.name.eq_ignore_ascii_case(name)
                        && option.value.eq_ignore_ascii_case(value)
                } else {
                    option.name == *name && option.value == *value
                }
            })
        })
    })
}

#[derive(Clone, Copy)]
enum StorefrontPriceRangeKind {
    Price,
    CompareAtPrice,
}

fn storefront_product_price_range_json(
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
    currency_code: &str,
    selections: &[SelectedField],
    kind: StorefrontPriceRangeKind,
) -> Value {
    let prices = match kind {
        StorefrontPriceRangeKind::Price => storefront_product_variant_prices(product, variants),
        StorefrontPriceRangeKind::CompareAtPrice => {
            storefront_product_variant_compare_at_prices(product, variants)
        }
    };
    let (min_price, max_price) = storefront_price_bounds(prices).unwrap_or((0.0, 0.0));
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "__typename" => Some(json!("ProductPriceRange")),
        "minVariantPrice" => Some(storefront_money_json(
            &format!("{min_price:.2}"),
            currency_code,
            &selection.selection,
        )),
        "maxVariantPrice" => Some(storefront_money_json(
            &format!("{max_price:.2}"),
            currency_code,
            &selection.selection,
        )),
        _ => None,
    })
}

fn storefront_product_variant_prices(
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
) -> Vec<f64> {
    if !variants.is_empty() {
        return variants
            .iter()
            .filter_map(|variant| storefront_parse_price(&variant.price))
            .collect();
    }
    product
        .variants
        .iter()
        .filter_map(|variant| variant.get("price").and_then(Value::as_str))
        .filter_map(storefront_parse_price)
        .collect()
}

fn storefront_product_variant_compare_at_prices(
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
) -> Vec<f64> {
    if !variants.is_empty() {
        return variants
            .iter()
            .filter_map(|variant| variant.compare_at_price.as_deref())
            .filter_map(storefront_parse_price)
            .collect();
    }
    product
        .variants
        .iter()
        .filter_map(|variant| variant.get("compareAtPrice").and_then(Value::as_str))
        .filter_map(storefront_parse_price)
        .collect()
}

fn storefront_price_bounds(prices: Vec<f64>) -> Option<(f64, f64)> {
    let mut iter = prices.into_iter();
    let first = iter.next()?;
    let mut min_price = first;
    let mut max_price = first;
    for price in iter {
        if price < min_price {
            min_price = price;
        }
        if price > max_price {
            max_price = price;
        }
    }
    Some((min_price, max_price))
}

fn storefront_parse_price(price: &str) -> Option<f64> {
    price.trim().parse::<f64>().ok()
}

fn storefront_money_json(price: &str, currency_code: &str, selections: &[SelectedField]) -> Value {
    let amount = normalize_money_amount(price);
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "__typename" => Some(json!("MoneyV2")),
        "amount" => Some(json!(amount)),
        "currencyCode" => Some(json!(currency_code)),
        _ => None,
    })
}

fn storefront_product_images_connection_json(
    product: &ProductRecord,
    arguments: &BTreeMap<String, ResolvedValue>,
    selections: &[SelectedField],
) -> Value {
    let images = product
        .media
        .iter()
        .filter_map(product_image_json_from_media)
        .collect::<Vec<_>>();
    selected_connection_json_with_args(images, arguments, selections, value_id_cursor)
}

fn storefront_product_media_connection_json(
    product: &ProductRecord,
    arguments: &BTreeMap<String, ResolvedValue>,
    selections: &[SelectedField],
) -> Value {
    selected_connection_json_with_args(
        product.media.clone(),
        arguments,
        selections,
        value_id_cursor,
    )
}

fn storefront_product_sort_key(
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
    sort_key: Option<&str>,
) -> StagedSortKey {
    let primary = match sort_key {
        Some("TITLE") => storefront_sort_string(&product.title),
        Some("PRODUCT_TYPE") => storefront_sort_string(&product.product_type),
        Some("VENDOR") => storefront_sort_string(&product.vendor),
        Some("UPDATED_AT") => StagedSortValue::String(product.updated_at.clone()),
        None | Some("CREATED_AT") | Some("BEST_SELLING") | Some("RELEVANCE") => {
            StagedSortValue::String(product.created_at.clone())
        }
        Some("PRICE") => {
            let prices = if variants.is_empty() {
                product
                    .variants
                    .iter()
                    .filter_map(|variant| variant.get("price").and_then(Value::as_str))
                    .filter_map(storefront_parse_price)
                    .collect::<Vec<_>>()
            } else {
                variants
                    .iter()
                    .filter_map(|variant| storefront_parse_price(&variant.price))
                    .collect::<Vec<_>>()
            };
            storefront_price_bounds(prices)
                .map(|(min_price, _)| StagedSortValue::String(format!("{min_price:020.4}")))
                .unwrap_or(StagedSortValue::Null)
        }
        Some("ID") => return storefront_gid_sort_key(&product.id),
        Some(_) => storefront_gid_sort_key(&product.id)
            .into_iter()
            .next()
            .unwrap_or(StagedSortValue::Null),
    };
    vec![primary, storefront_gid_tail_sort_value(&product.id)]
}

fn storefront_gid_sort_key(id: &str) -> StagedSortKey {
    vec![storefront_gid_tail_sort_value(id)]
}

fn storefront_gid_tail_sort_value(id: &str) -> StagedSortValue {
    let tail = resource_id_tail(id);
    tail.parse::<i64>()
        .map(StagedSortValue::I64)
        .unwrap_or_else(|_| storefront_sort_string(tail))
}

fn storefront_sort_string(value: impl AsRef<str>) -> StagedSortValue {
    StagedSortValue::String(value.as_ref().to_ascii_lowercase())
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

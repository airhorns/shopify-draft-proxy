use super::*;

mod bulk_operations;
mod media;
mod owner_metafields;

const TAGGABLE_ORDER_HYDRATE_QUERY: &str =
    "query OrdersOrderHydrate($id: ID!) {\n  order(id: $id) { id name tags }\n}";
const TAGGABLE_DRAFT_ORDER_HYDRATE_QUERY: &str =
    "query OrdersDraftOrderHydrate($id: ID!) {\n  draftOrder(id: $id) { id name tags }\n}";
const TAGGABLE_CUSTOMER_HYDRATE_QUERY: &str =
    include_str!("../../config/parity-requests/customers/taggable-customer-hydrate.graphql");
const TAGGABLE_ARTICLE_HYDRATE_QUERY: &str = "query TagsArticleHydrate($id: ID!) {\n  article(id: $id) {\n    __typename\n    id\n    title\n    handle\n    tags\n    createdAt\n    updatedAt\n    blog { id }\n  }\n}";
const TAGGABLE_PRODUCT_HYDRATE_QUERY: &str = "\nquery ProductsHydrateNodes($ids: [ID!]!) {\n  nodes(ids: $ids) {\n    __typename\n    id\n    ... on Product {\n      legacyResourceId\n      title\n      handle\n      status\n      vendor\n      productType\n      tags\n      totalInventory\n      tracksInventory\n      createdAt\n      updatedAt\n      publishedAt\n      descriptionHtml\n      onlineStorePreviewUrl\n      templateSuffix\n      seo { title description }\n      resourcePublicationsV2(first: 10) { nodes { publication { id } publishDate isPublished } }\n    }\n  }\n}";

const PRODUCT_VARIANTS_BULK_CREATE_INVENTORY_QUANTITIES_LIMIT: usize = 50_000;
const PRODUCT_VARIANTS_BULK_CREATE_DEFAULT_LOCATION_LIMIT: usize = 200;

impl DraftProxy {
    pub(in crate::proxy) fn record_passthrough_log_entry(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        root_fields: &[String],
        root_field: &str,
    ) {
        let id = format!("log-{}", self.log_entries.len() + 1);
        self.log_entries.push(json!({
            "id": id,
            "operationName": root_field,
            "status": "proxied",
            "path": request.path,
            "query": query,
            "variables": resolved_variables_json(variables),
            "interpreted": {
                "operationType": "mutation",
                "rootFields": root_fields,
                "primaryRootField": root_field,
                "capability": {
                    "operationName": root_field,
                    "domain": "unknown",
                    "execution": "passthrough"
                }
            },
            "notes": "Mutation passthrough placeholder until supported local staging is implemented."
        }));
    }

    pub(in crate::proxy) fn product_overlay_read_data(
        &self,
        root_fields: &[RootFieldSelection],
    ) -> Value {
        let mut fields = serde_json::Map::new();
        for field in root_fields {
            let value = match field.name.as_str() {
                "product" => Some(self.product_by_id_field(field)),
                "products" => Some(self.products_connection_field(field)),
                "productsCount" => Some(self.products_count_field(field)),
                "productByIdentifier" => Some(self.product_by_identifier_field(field)),
                "productOperation" => Some(self.product_operation_by_id_field(field)),
                "productVariant" => Some(self.product_variant_by_id_field(field)),
                "inventoryItem" => {
                    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    self.product_inventory_item_by_id_value(&id, &field.selection)
                }
                // Mixed reads pairing `product` with sibling `collection(id:)` lookups
                // (e.g. collectionsToJoin downstream parity) resolve membership locally.
                "collection" => Some(self.collection_membership_value(field)),
                _ => None,
            };
            if let Some(value) = value {
                fields.insert(field.response_key.clone(), value);
            }
        }
        Value::Object(fields)
    }

    pub(in crate::proxy) fn product_operation_by_id_field(
        &self,
        field: &RootFieldSelection,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        // `productDelete` async operations live in their own map; Set/Duplicate/Bundle
        // operations are staged in `product_operations`. Try the delete map first, then
        // fall back to the general operation store so async productSet/productDuplicate/
        // productBundleCreate reads resolve their staged operation (and its product).
        self.product_delete_operation_value_by_id(&id, &field.selection)
            .or_else(|| {
                self.store
                    .staged
                    .product_operations
                    .get(&id)
                    .map(|operation| self.product_operation_json(operation, &field.selection))
            })
            .unwrap_or(Value::Null)
    }

    pub(in crate::proxy) fn product_delete_operation_value_by_id(
        &self,
        id: &str,
        selection: &[SelectedField],
    ) -> Option<Value> {
        self.store
            .staged
            .product_delete_operations
            .get(id)
            .map(|deleted_product_id| {
                selected_json(
                    &json!({
                        "__typename": "ProductDeleteOperation",
                        "id": id,
                        "status": "COMPLETE",
                        "deletedProductId": deleted_product_id,
                        "userErrors": []
                    }),
                    selection,
                )
            })
    }

    pub(in crate::proxy) fn product_by_id_field(&self, field: &RootFieldSelection) -> Value {
        self.product_by_id_value(&field.arguments, &field.selection)
    }

    pub(in crate::proxy) fn product_by_id_value(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
        selection: &[SelectedField],
    ) -> Value {
        let Some(ResolvedValue::String(id)) = arguments.get("id") else {
            return Value::Null;
        };
        match self.product_record_by_id(id) {
            Some(product) => {
                let variants = self
                    .store
                    .product_variants_for_product(&product.id)
                    .iter()
                    .map(|variant| self.variant_with_inventory_levels(variant))
                    .collect::<Vec<_>>();
                let base =
                    self.product_json_with_selling_plan_overlay(product, &variants, selection);
                self.owner_metafield_overlay_owner_json_with_product_variants(
                    "product",
                    &product.id,
                    selection,
                    &product.variants,
                    base,
                )
            }
            None if Self::owner_field_selects_direct_metafields(selection) => {
                let owner_id = id.clone();
                self.minimal_owner_json_for_read("product", &owner_id, selection)
            }
            None => Value::Null,
        }
    }

    pub(in crate::proxy) fn product_by_identifier_field(
        &self,
        field: &RootFieldSelection,
    ) -> Value {
        let Some(ResolvedValue::Object(identifier)) = field.arguments.get("identifier") else {
            return Value::Null;
        };
        self.product_by_identifier_value(identifier, &field.selection)
    }

    pub(in crate::proxy) fn product_by_identifier_value(
        &self,
        identifier: &BTreeMap<String, ResolvedValue>,
        selection: &[SelectedField],
    ) -> Value {
        let product = match identifier.get("id") {
            Some(ResolvedValue::String(id)) => self.product_record_by_id(id),
            _ => match identifier.get("handle") {
                Some(ResolvedValue::String(handle)) => self.product_record_by_handle(handle),
                _ => None,
            },
        };
        match product {
            Some(product) => {
                let variants = self
                    .store
                    .product_variants_for_product(&product.id)
                    .iter()
                    .map(|variant| self.variant_with_inventory_levels(variant))
                    .collect::<Vec<_>>();
                let base =
                    self.product_json_with_selling_plan_overlay(product, &variants, selection);
                self.owner_metafield_overlay_owner_json_with_product_variants(
                    "product",
                    &product.id,
                    selection,
                    &product.variants,
                    base,
                )
            }
            None => match identifier.get("id") {
                Some(ResolvedValue::String(id))
                    if Self::owner_field_selects_direct_metafields(selection) =>
                {
                    self.minimal_owner_json_for_read("product", id, selection)
                }
                _ => Value::Null,
            },
        }
    }

    pub(in crate::proxy) fn product_variant_by_id_field(
        &self,
        field: &RootFieldSelection,
    ) -> Value {
        let Some(ResolvedValue::String(id)) = field.arguments.get("id") else {
            return Value::Null;
        };
        self.product_variant_by_id_value(id, &field.selection)
    }

    pub(in crate::proxy) fn product_variant_by_id_value(
        &self,
        id: &str,
        selection: &[SelectedField],
    ) -> Value {
        let Some(variant) = self.store.product_variant_by_id(id) else {
            return if Self::owner_field_selects_direct_metafields(selection) {
                self.minimal_owner_json_for_read("productVariant", id, selection)
            } else {
                Value::Null
            };
        };
        let variant = self.variant_with_inventory_levels(variant);
        let base = self.product_variant_json_with_selling_plan_overlay(
            &variant,
            self.store.product_by_id(&variant.product_id),
            selection,
        );
        self.owner_metafield_overlay_owner_json("productVariant", &variant.id, selection, base)
    }

    pub(in crate::proxy) fn product_inventory_item_by_id_value(
        &self,
        id: &str,
        selection: &[SelectedField],
    ) -> Option<Value> {
        if let Some(variant) = self.store.product_variant_by_inventory_item_id(id) {
            let variant = self.variant_with_inventory_levels(variant);
            let product = self.store.product_by_id(&variant.product_id);
            return Some(product_variant_inventory_item_json(
                &variant, product, selection,
            ));
        }
        self.store.products().iter().find_map(|product| {
            product.variants.iter().find_map(|variant| {
                (variant
                    .get("inventoryItem")
                    .and_then(|inventory_item| inventory_item.get("id"))
                    .and_then(Value::as_str)
                    == Some(id))
                .then(|| observed_product_variant_inventory_item_json(product, variant, selection))
                .flatten()
            })
        })
    }

    pub(in crate::proxy) fn product_record_by_id(&self, id: &str) -> Option<&ProductRecord> {
        self.store.product_by_id(id)
    }

    pub(in crate::proxy) fn product_record_by_handle(
        &self,
        handle: &str,
    ) -> Option<&ProductRecord> {
        self.store.product_by_handle(handle)
    }

    pub(in crate::proxy) fn products_connection_field(&self, field: &RootFieldSelection) -> Value {
        self.products_connection_value(&field.arguments, &field.selection)
    }

    pub(in crate::proxy) fn has_product_overlay_state(&self) -> bool {
        self.store.has_product_state()
    }

    pub(in crate::proxy) fn products_connection_value(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
        root_selection: &[SelectedField],
    ) -> Value {
        let mut products = self.store.products();
        if let Some(ResolvedValue::String(query)) = arguments.get("query") {
            if query.contains("status:") {
                products.clear();
            } else if let Some(tag) = product_tag_query_value(query) {
                products.retain(|product| {
                    self.store
                        .staged
                        .product_search_tags
                        .get(&product.id)
                        .map(|tags| tags.contains(tag))
                        .unwrap_or_else(|| product.tags.iter().any(|value| value == tag))
                });
            } else if query.trim_start().starts_with("sku:") {
                products.retain(|product| {
                    let variants = self.store.product_variants_for_product(&product.id);
                    product_matches_sku_query(product, &variants, query)
                });
            }
        }
        selected_typed_connection_with_args(
            &products,
            arguments,
            root_selection,
            |product, selections| {
                let variants = self.store.product_variants_for_product(&product.id);
                let base = product_json_with_variants_and_currency(
                    product,
                    &variants,
                    selections,
                    &self.store.shop_currency_code(),
                );
                self.owner_metafield_overlay_owner_json_with_product_variants(
                    "product",
                    &product.id,
                    selections,
                    &product.variants,
                    base,
                )
            },
            |product| product_cursor(product).to_string(),
        )
    }

    pub(in crate::proxy) fn products_count_field(&self, field: &RootFieldSelection) -> Value {
        if let Some(ResolvedValue::String(query)) = field.arguments.get("query") {
            if query.contains("status:") {
                return product_count_json(0, &field.selection);
            }
            if let Some(tag) = product_tag_query_value(query) {
                let count = self
                    .effective_products()
                    .into_iter()
                    .filter(|product| {
                        self.store
                            .staged
                            .product_search_tags
                            .get(&product.id)
                            .map(|tags| tags.contains(tag))
                            .unwrap_or_else(|| product.tags.iter().any(|value| value == tag))
                    })
                    .count();
                return product_count_json(count, &field.selection);
            }
            if query.trim_start().starts_with("sku:") {
                let count = self
                    .effective_products()
                    .into_iter()
                    .filter(|product| {
                        let variants = self.store.product_variants_for_product(&product.id);
                        product_matches_sku_query(product, &variants, query)
                    })
                    .count();
                return product_count_json(count, &field.selection);
            }
        }
        product_count_json(self.effective_product_count(), &field.selection)
    }

    pub(in crate::proxy) fn effective_products(&self) -> Vec<ProductRecord> {
        self.store.products()
    }

    pub(in crate::proxy) fn effective_product_count(&self) -> usize {
        self.store.product_count()
    }

    pub(in crate::proxy) fn product_create(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        if let Some(response) = product_create_status_validation_error(request, query, variables) {
            return MutationOutcome::response(response);
        }
        let Some(input) = product_create_input(query, variables) else {
            let response_key = primary_root_field(query, variables)
                .map(|field| field.response_key)
                .unwrap_or_else(|| "productCreate".to_string());
            return MutationOutcome::response(ok_json(json!({
                "data": {
                    response_key: {
                        "product": null,
                        "userErrors": [{
                            "field": ["product"],
                            "message": "Product input is required",
                            "code": "REQUIRED"
                        }]
                    }
                }
            })));
        };
        if input.contains_key("variants") {
            return MutationOutcome::response(ok_json(json!({
                "errors": [{
                    "message": "Variable $input of type ProductInput! was provided invalid value for variants (Field is not defined on ProductInput)",
                    "locations": [{"line": 2, "column": 39}],
                    "extensions": {
                        "code": "INVALID_VARIABLE",
                        "value": resolved_value_json(&ResolvedValue::Object(input.clone())),
                        "problems": [{
                            "path": ["variants"],
                            "explanation": "Field is not defined on ProductInput"
                        }]
                    }
                }]
            })));
        }

        if input.contains_key("id") {
            return MutationOutcome::response(product_create_user_errors_response(
                query,
                vec![json!({
                    "field": ["input"],
                    "message": "id cannot be specified during creation"
                })],
            ));
        }

        let Some(title) =
            resolved_string_field(&input, "title").filter(|value| !value.trim().is_empty())
        else {
            let (response_key, payload_selection) =
                primary_root_response_selection(query, variables, || "productCreate".to_string());
            let error_selection =
                selected_child_selection(&payload_selection, "userErrors").unwrap_or_default();
            let user_error = selected_json(
                &json!({
                    "field": ["title"],
                    "message": "Title can't be blank",
                    "code": "BLANK"
                }),
                &error_selection,
            );
            return MutationOutcome::response(ok_json(json!({
                "data": {
                    response_key: {
                        "product": null,
                        "userErrors": [user_error]
                    }
                }
            })));
        };

        if let Some(handle) = resolved_string_field(&input, "handle") {
            if handle.chars().count() > 255 {
                return MutationOutcome::response(product_create_user_errors_response(
                    query,
                    vec![json!({
                        "field": ["handle"],
                        "message": "Handle is too long (maximum is 255 characters)"
                    })],
                ));
            }
        }
        if let Some(vendor) = resolved_string_field(&input, "vendor") {
            if vendor.chars().count() > 255 {
                return MutationOutcome::response(product_create_user_errors_response(
                    query,
                    vec![json!({
                        "field": ["vendor"],
                        "message": "Vendor is too long (maximum is 255 characters)"
                    })],
                ));
            }
        }
        if let Some(product_type) = resolved_string_field(&input, "productType") {
            if product_type.chars().count() > 255 {
                return MutationOutcome::response(product_create_user_errors_response(
                    query,
                    vec![
                        json!({
                            "field": ["productType"],
                            "message": "Product type is too long (maximum is 255 characters)"
                        }),
                        json!({
                            "field": ["customProductType"],
                            "message": "Custom product type is too long (maximum is 255 characters)"
                        }),
                    ],
                ));
            }
        }

        let id = self.next_proxy_synthetic_gid("Product");
        let handle =
            resolved_string_field(&input, "handle").unwrap_or_else(|| slugify_handle(&title));
        let status =
            resolved_string_field(&input, "status").unwrap_or_else(|| "ACTIVE".to_string());
        let timestamp = self.next_product_timestamp();
        let extra_fields = resolved_string_field(&input, "combinedListingRole")
            .map(|role| BTreeMap::from([("combinedListingRole".to_string(), json!(role))]))
            .unwrap_or_default();
        let mut product = ProductRecord {
            id: id.clone(),
            created_at: timestamp.clone(),
            updated_at: timestamp,
            title,
            handle,
            status,
            description_html: resolved_string_field(&input, "descriptionHtml").unwrap_or_default(),
            vendor: resolved_string_field(&input, "vendor").unwrap_or_default(),
            product_type: resolved_string_field(&input, "productType").unwrap_or_default(),
            tags: resolved_string_list_field(&input, "tags"),
            template_suffix: resolved_string_field(&input, "templateSuffix").unwrap_or_default(),
            seo_title: resolved_object_string_field(&input, "seo", "title").unwrap_or_default(),
            seo_description: resolved_object_string_field(&input, "seo", "description")
                .unwrap_or_default(),
            total_inventory: 0,
            tracks_inventory: false,
            media: Vec::new(),
            variants: Vec::new(),
            collections: Vec::new(),
            extra_fields,
        };
        // Echo product-level inputs that Shopify persists verbatim onto the created product
        // and surfaces through downstream reads.
        if let Some(requires_selling_plan) = resolved_bool_field(&input, "requiresSellingPlan") {
            product.extra_fields.insert(
                "requiresSellingPlan".to_string(),
                json!(requires_selling_plan),
            );
        }
        let is_gift_card = resolved_bool_field(&input, "giftCard").unwrap_or(false);
        if is_gift_card {
            product
                .extra_fields
                .insert("isGiftCard".to_string(), json!(true));
        }
        if let Some(suffix) = resolved_string_field(&input, "giftCardTemplateSuffix") {
            product
                .extra_fields
                .insert("giftCardTemplateSuffix".to_string(), json!(suffix));
        }
        // Shopify resolves the input `category` taxonomy GID into a `{id, fullName}`
        // object on the created product, surfaced through both the mutation payload and
        // downstream reads.
        if let Some(category_id) = product_category_input_id(&input) {
            product
                .extra_fields
                .insert("category".to_string(), product_category_value(&category_id));
        }

        // `productCreate` always materializes at least one variant. With `productOptions`,
        // Shopify creates the lead combination (only the first value of each option; further
        // values are added later via bulk create). Without options it creates a single
        // "Default Title" standalone variant.
        let mut staged_ids = vec![id.clone()];
        let variant = if let Some((options, variant)) =
            self.product_options_and_default_variant(&input, &id)
        {
            product
                .extra_fields
                .insert("options".to_string(), json!(options));
            variant
        } else {
            self.default_standalone_variant(&id, is_gift_card)
        };
        product.variants = vec![product_variant_state_json(&variant)];
        staged_ids.push(variant.id.clone());
        self.store.stage_product_variant(variant);

        // `collectionsToJoin` adds the new product to existing collections. Add the minimal
        // collection refs to the product surface before staging so the mutation response
        // renders them.
        let collections_to_join = resolved_string_list_field_unsorted(&input, "collectionsToJoin");
        for collection_id in &collections_to_join {
            if let Some(collection) = self.store.collection_by_id(collection_id).cloned() {
                upsert_minimal_collection(&mut product.collections, &collection);
            }
        }

        // Stage any `metafields` supplied on create so downstream metafield reads resolve them.
        self.stage_owner_metafields_from_input(&id, &input);

        self.store.stage_product(product.clone());

        // Register collection membership so downstream `collection` reads expose hasProduct,
        // productsCount, and the product in their member list.
        for collection_id in &collections_to_join {
            self.add_product_to_collection_membership(collection_id, &product);
        }

        let (response_key, payload_selection) =
            primary_root_response_selection(query, variables, || "productCreate".to_string());
        let product_selection =
            selected_child_selection(&payload_selection, "product").unwrap_or_default();
        let variants = self.store.product_variants_for_product(&product.id);
        MutationOutcome::staged(
            ok_json(json!({
                "data": {
                    response_key: product_mutation_payload_json(
                        &product,
                        &variants,
                        &payload_selection,
                        &product_selection,
                        &self.store.shop_currency_code(),
                    )
                }
            })),
            LogDraft::staged("productCreate", "products", staged_ids),
        )
    }

    /// Build the `options` JSON and the single default variant implied by a
    /// `productCreate` `productOptions` input. Returns `None` when no `productOptions`
    /// were supplied. The lead variant uses the first value of every option; option value
    /// lists likewise contain only those lead values, matching Shopify's create behaviour.
    fn product_options_and_default_variant(
        &mut self,
        input: &BTreeMap<String, ResolvedValue>,
        product_id: &str,
    ) -> Option<(Vec<Value>, ProductVariantRecord)> {
        let product_options = list_object_field(input, "productOptions");
        if product_options.is_empty() {
            return None;
        }
        let mut options = Vec::new();
        let mut selected_options = Vec::new();
        for (index, option) in product_options.iter().enumerate() {
            let name = resolved_string_field(option, "name").unwrap_or_default();
            let value_names: Vec<String> = list_object_field(option, "values")
                .iter()
                .filter_map(|value| resolved_string_field(value, "name"))
                .collect();
            let first_value = value_names.first().cloned().unwrap_or_default();
            let option_id = self.next_proxy_synthetic_gid("ProductOption");
            // `optionValues` lists every supplied value, but only the lead value gains a
            // variant on create, so `hasVariants` is true for it alone; the string `values`
            // list (legacy field) contains just the value(s) that back a variant.
            let mut option_values = Vec::new();
            for (value_index, value_name) in value_names.iter().enumerate() {
                let option_value_id = self.next_proxy_synthetic_gid("ProductOptionValue");
                option_values.push(json!({
                    "id": option_value_id,
                    "name": value_name,
                    "hasVariants": value_index == 0,
                }));
            }
            options.push(json!({
                "id": option_id,
                "name": name,
                "position": index + 1,
                "values": [first_value.clone()],
                "optionValues": option_values,
            }));
            selected_options.push(ProductVariantSelectedOption {
                name,
                value: first_value,
            });
        }
        let title = selected_options
            .iter()
            .map(|option| option.value.clone())
            .collect::<Vec<_>>()
            .join(" / ");
        let variant = ProductVariantRecord {
            id: self.next_proxy_synthetic_gid("ProductVariant"),
            product_id: product_id.to_string(),
            title,
            sku: String::new(),
            barcode: None,
            price: "0.00".to_string(),
            compare_at_price: None,
            taxable: true,
            inventory_policy: "DENY".to_string(),
            inventory_quantity: 0,
            selected_options,
            inventory_item: ProductVariantInventoryItem {
                id: self.next_proxy_synthetic_gid("InventoryItem"),
                tracked: true,
                requires_shipping: true,
                extra_fields: BTreeMap::new(),
            },
            media_ids: Vec::new(),
            extra_fields: BTreeMap::from([("position".to_string(), json!(1))]),
        };
        Some((options, variant))
    }

    /// Build the implicit "Default Title" standalone variant Shopify creates for a product
    /// with no `productOptions`. Gift cards default to a non-taxable, non-shippable variant.
    fn default_standalone_variant(
        &mut self,
        product_id: &str,
        is_gift_card: bool,
    ) -> ProductVariantRecord {
        ProductVariantRecord {
            id: self.next_proxy_synthetic_gid("ProductVariant"),
            product_id: product_id.to_string(),
            title: "Default Title".to_string(),
            sku: String::new(),
            barcode: None,
            price: "0.00".to_string(),
            compare_at_price: None,
            taxable: !is_gift_card,
            inventory_policy: "DENY".to_string(),
            inventory_quantity: 0,
            selected_options: vec![ProductVariantSelectedOption {
                name: "Title".to_string(),
                value: "Default Title".to_string(),
            }],
            inventory_item: ProductVariantInventoryItem {
                id: self.next_proxy_synthetic_gid("InventoryItem"),
                tracked: false,
                requires_shipping: !is_gift_card,
                extra_fields: BTreeMap::new(),
            },
            media_ids: Vec::new(),
            extra_fields: BTreeMap::from([("position".to_string(), json!(1))]),
        }
    }

    /// Add a single product to a collection's membership, preserving any existing members,
    /// so downstream `collection` reads expose hasProduct/productsCount/products for it.
    fn add_product_to_collection_membership(
        &mut self,
        collection_id: &str,
        product: &ProductRecord,
    ) {
        let Some(collection) = self.store.collection_by_id(collection_id).cloned() else {
            return;
        };
        let mut members: Vec<ProductRecord> = collection
            .get("products")
            .and_then(|connection| connection.get("nodes"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(product_state_from_json)
            .collect();
        if !members.iter().any(|member| member.id == product.id) {
            members.push(product.clone());
        }
        self.store.stage_collection_membership(collection, members);
    }

    pub(in crate::proxy) fn product_update(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let Some(input) = product_input(query, variables) else {
            return MutationOutcome::response(ok_json(json!({
                "data": {
                    "productUpdate": {
                        "product": null,
                        "userErrors": [{
                            "field": ["product"],
                            "message": "Product input is required",
                            "code": "REQUIRED"
                        }]
                    }
                }
            })));
        };
        let incoming_tags = if input.contains_key("tags") {
            Some(resolved_string_list_field_unsorted(&input, "tags"))
        } else {
            None
        };
        if let Some(tags) = incoming_tags.as_ref() {
            if tags.len() > 250 {
                return MutationOutcome::response(ok_json(json!({
                    "errors": [{
                        "message": format!("The input array size of {} is greater than the maximum allowed of 250.", tags.len()),
                        "locations": [{"line": 3, "column": 5}],
                        "path": ["productUpdate", "product", "tags"],
                        "extensions": {"code": "MAX_INPUT_SIZE_EXCEEDED"}
                    }]
                })));
            }
        }
        let Some(id) = resolved_string_field(&input, "id") else {
            return MutationOutcome::response(product_update_missing_product(query));
        };
        if self.store.product_by_id(&id).is_none() && self.config.read_mode == ReadMode::LiveHybrid
        {
            self.hydrate_product_nodes_for_observation_with_request(request, vec![id.clone()]);
        }
        let Some(existing) = self.store.product_staged_or_base(&id) else {
            return MutationOutcome::response(product_update_missing_product(query));
        };

        if input.contains_key("title")
            && resolved_string_field(&input, "title")
                .unwrap_or_default()
                .trim()
                .is_empty()
        {
            return self.product_update_field_user_error(
                query,
                &existing,
                "title",
                "Title can't be blank",
            );
        }

        if let Some(handle) = resolved_string_field(&input, "handle") {
            if handle.chars().count() > 255 {
                return self.product_update_field_user_error(
                    query,
                    &existing,
                    "handle",
                    "Handle is too long (maximum is 255 characters)",
                );
            }
        }

        if let Some(tags) = incoming_tags.as_ref() {
            if tags.iter().any(|tag| tag.chars().count() > 255) {
                let (response_key, payload_selection) =
                    primary_root_response_selection(query, variables, || {
                        "productUpdate".to_string()
                    });
                let product_selection =
                    selected_child_selection(&payload_selection, "product").unwrap_or_default();
                let error_selection =
                    selected_child_selection(&payload_selection, "userErrors").unwrap_or_default();
                let user_error = selected_json(
                    &json!({"field": ["tags"], "message": "Product tags is invalid"}),
                    &error_selection,
                );
                return MutationOutcome::response(ok_json(json!({
                    "data": {
                        response_key: selected_json(
                            &json!({
                                "product": product_json_with_currency(
                                    &existing,
                                    &product_selection,
                                    &self.store.shop_currency_code()
                                ),
                                "userErrors": [user_error]
                            }),
                            &payload_selection
                        )
                    }
                })));
            }
        }

        let product = ProductRecord {
            id: existing.id,
            created_at: existing.created_at,
            updated_at: self.next_product_updated_at(&existing.updated_at),
            title: resolved_string_field(&input, "title").unwrap_or(existing.title),
            handle: resolved_string_field(&input, "handle").unwrap_or(existing.handle),
            status: resolved_string_field(&input, "status").unwrap_or(existing.status),
            description_html: resolved_string_field(&input, "descriptionHtml")
                .unwrap_or(existing.description_html),
            vendor: resolved_string_field(&input, "vendor").unwrap_or(existing.vendor),
            product_type: resolved_string_field(&input, "productType")
                .unwrap_or(existing.product_type),
            tags: if input.contains_key("tags") {
                normalize_product_tags(incoming_tags.unwrap_or_default())
            } else {
                existing.tags
            },
            template_suffix: resolved_string_field(&input, "templateSuffix")
                .unwrap_or(existing.template_suffix),
            seo_title: resolved_object_string_field(&input, "seo", "title")
                .unwrap_or(existing.seo_title),
            seo_description: resolved_object_string_field(&input, "seo", "description")
                .unwrap_or(existing.seo_description),
            total_inventory: existing.total_inventory,
            tracks_inventory: existing.tracks_inventory,
            media: existing.media,
            variants: existing.variants,
            collections: existing.collections,
            extra_fields: existing.extra_fields,
        };
        self.store.stage_product(product.clone());

        let (response_key, payload_selection) =
            primary_root_response_selection(query, variables, || "productUpdate".to_string());
        let product_selection =
            selected_child_selection(&payload_selection, "product").unwrap_or_default();
        let variants = self.store.product_variants_for_product(&product.id);
        MutationOutcome::staged(
            ok_json(json!({
                "data": {
                    response_key: product_mutation_payload_json(
                        &product,
                        &variants,
                        &payload_selection,
                        &product_selection,
                        &self.store.shop_currency_code(),
                    )
                }
            })),
            LogDraft::staged("productUpdate", "products", vec![id]),
        )
    }

    /// Build a productUpdate response that returns the (unchanged) product alongside a single
    /// field-scoped userError — the shape Shopify emits when an input value is rejected
    /// (e.g. blank title, over-long handle) without persisting the mutation.
    fn product_update_field_user_error(
        &self,
        query: &str,
        existing: &ProductRecord,
        field: &str,
        message: &str,
    ) -> MutationOutcome {
        let (response_key, payload_selection) =
            primary_root_response_selection(query, &BTreeMap::new(), || {
                "productUpdate".to_string()
            });
        let product_selection =
            selected_child_selection(&payload_selection, "product").unwrap_or_default();
        let error_selection =
            selected_child_selection(&payload_selection, "userErrors").unwrap_or_default();
        let user_error = selected_json(
            &json!({"field": [field], "message": message}),
            &error_selection,
        );
        MutationOutcome::response(ok_json(json!({
            "data": {
                response_key: selected_json(
                    &json!({
                        "product": product_json_with_currency(
                            existing,
                            &product_selection,
                            &self.store.shop_currency_code()
                        ),
                        "userErrors": [user_error]
                    }),
                    &payload_selection
                )
            }
        })))
    }

    pub(in crate::proxy) fn product_variant_mutation(
        &mut self,
        request: &Request,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        match root_field {
            "productVariantCreate" => self.product_variant_create(query, variables),
            "productVariantUpdate" => self.product_variant_update(query, variables),
            "productVariantDelete" => self.product_variant_delete(query, variables),
            "productVariantAppendMedia" | "productVariantDetachMedia" => {
                self.product_variant_media_mutation(request, root_field, query, variables)
            }
            "productVariantsBulkCreate" => {
                self.product_variants_bulk_create(request, query, variables)
            }
            "productVariantsBulkUpdate" => {
                self.product_variants_bulk_update(request, query, variables)
            }
            "productVariantsBulkDelete" => {
                self.product_variants_bulk_delete(request, query, variables)
            }
            "productVariantsBulkReorder" => {
                self.product_variants_bulk_reorder(request, query, variables)
            }
            _ => MutationOutcome::response(json_error(
                400,
                "No mutation dispatcher implemented for product variant root",
            )),
        }
    }

    fn product_variant_media_mutation(
        &mut self,
        request: &Request,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let (response_key, payload_selection) =
            primary_root_response_selection(query, variables, || root_field.to_string());
        let product_id = resolved_string_field(variables, "productId").unwrap_or_default();
        let variant_media = resolved_object_list_field(variables, "variantMedia");
        self.hydrate_product_variant_media_owner_state(request, &product_id, &variant_media);
        let user_errors =
            self.product_variant_media_user_errors(root_field, &product_id, &variant_media);

        if !user_errors.is_empty() {
            let payload = self.product_variant_media_payload_json(
                &payload_selection,
                &product_id,
                Vec::new(),
                user_errors,
            );
            return MutationOutcome::response(ok_json(json!({
                "data": { response_key: payload }
            })));
        }

        let mut changed_variant_ids = Vec::new();
        for item in &variant_media {
            let Some(variant_id) = resolved_string_field(item, "variantId") else {
                continue;
            };
            let media_ids = resolved_string_list_field_unsorted(item, "mediaIds");
            let Some(mut variant) = self.store.product_variant_by_id(&variant_id).cloned() else {
                continue;
            };
            match root_field {
                "productVariantAppendMedia" => {
                    for media_id in media_ids {
                        if !variant
                            .media_ids
                            .iter()
                            .any(|existing| existing == &media_id)
                        {
                            variant.media_ids.push(media_id);
                        }
                    }
                }
                "productVariantDetachMedia" => {
                    let removals = media_ids.into_iter().collect::<BTreeSet<_>>();
                    variant
                        .media_ids
                        .retain(|media_id| !removals.contains(media_id));
                }
                _ => {}
            }
            changed_variant_ids.push(variant.id.clone());
            self.store.stage_product_variant(variant);
        }

        let payload = self.product_variant_media_payload_json(
            &payload_selection,
            &product_id,
            changed_variant_ids.clone(),
            Vec::new(),
        );
        MutationOutcome::staged(
            ok_json(json!({ "data": { response_key: payload } })),
            LogDraft::staged(
                root_field,
                "products",
                std::iter::once(product_id)
                    .chain(changed_variant_ids)
                    .collect(),
            ),
        )
    }

    fn hydrate_product_variant_media_owner_state(
        &mut self,
        request: &Request,
        product_id: &str,
        variant_media: &[BTreeMap<String, ResolvedValue>],
    ) {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return;
        }
        // The owning product (and its variants + media) must be in local state for
        // the variant-existence and media-existence checks to resolve against real
        // store data. The generic node-observation query does not select `media`, so
        // forward the media-aware product hydrate (which also brings the product's
        // variants) and observe it. Hydrate when the product is missing locally or
        // when any referenced variant is not yet known.
        if product_id.is_empty() {
            return;
        }
        let product_missing = self.store.product_by_id(product_id).is_none();
        let any_variant_missing = variant_media.iter().any(|item| {
            resolved_string_field(item, "variantId")
                .is_some_and(|variant_id| self.store.product_variant_by_id(&variant_id).is_none())
        });
        if !product_missing && !any_variant_missing {
            return;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": self::media::MEDIA_PRODUCT_HYDRATE_QUERY,
                "operationName": "MediaProductHydrate",
                "variables": { "id": product_id },
            }),
        );
        if response.status >= 400 {
            return;
        }
        if response.body["data"]["product"].is_object() {
            let product_node = response.body["data"]["product"].clone();
            self.observe_media_product_node(&product_node);
        }
    }

    fn product_variant_media_user_errors(
        &self,
        root_field: &str,
        product_id: &str,
        variant_media: &[BTreeMap<String, ResolvedValue>],
    ) -> Vec<Value> {
        let mut user_errors = Vec::new();
        if variant_media.len() > 100 {
            user_errors.push(product_variant_media_user_error(
                &["variantMedia"],
                "Exceeded 100 variant-media pairs per mutation.",
                "MAXIMUM_VARIANT_MEDIA_PAIRS_EXCEEDED",
            ));
            return user_errors;
        }

        for (entry_index, item) in variant_media.iter().enumerate() {
            let media_ids = resolved_string_list_field_unsorted(item, "mediaIds");
            if media_ids.len() > 1 {
                user_errors.push(product_variant_media_user_error(
                    &["variantMedia", &entry_index.to_string(), "mediaIds"],
                    "Only one mediaId is allowed per media input.",
                    "TOO_MANY_MEDIA_PER_INPUT_PAIR",
                ));
            }
        }
        if !user_errors.is_empty() {
            return user_errors;
        }

        let mut first_variant_indexes = BTreeMap::new();
        let mut duplicate_error_indexes = BTreeSet::new();
        for (entry_index, item) in variant_media.iter().enumerate() {
            let Some(variant_id) = resolved_string_field(item, "variantId")
                .filter(|variant_id| !variant_id.is_empty())
            else {
                continue;
            };
            if let Some(first_index) = first_variant_indexes.insert(variant_id, entry_index) {
                duplicate_error_indexes.insert(first_index);
            }
        }
        for entry_index in duplicate_error_indexes {
            user_errors.push(product_variant_media_user_error(
                &["variantMedia", &entry_index.to_string(), "variantId"],
                "Variant was specified in more than one media input.",
                "PRODUCT_VARIANT_SPECIFIED_MULTIPLE_TIMES",
            ));
        }
        if !user_errors.is_empty() {
            return user_errors;
        }

        for (entry_index, item) in variant_media.iter().enumerate() {
            let variant_id = resolved_string_field(item, "variantId").unwrap_or_default();
            let media_ids = resolved_string_list_field_unsorted(item, "mediaIds");
            let Some(variant) = self.store.product_variant_by_id(&variant_id) else {
                user_errors.push(product_variant_media_user_error(
                    &["variantMedia", &entry_index.to_string(), "variantId"],
                    "Variant does not exist on the specified product.",
                    "PRODUCT_VARIANT_DOES_NOT_EXIST_ON_PRODUCT",
                ));
                continue;
            };
            if variant.product_id != product_id {
                user_errors.push(product_variant_media_user_error(
                    &["variantMedia", &entry_index.to_string(), "variantId"],
                    "Variant does not exist on the specified product.",
                    "PRODUCT_VARIANT_DOES_NOT_EXIST_ON_PRODUCT",
                ));
                continue;
            }
            if root_field == "productVariantAppendMedia" && !variant.media_ids.is_empty() {
                user_errors.push(product_variant_media_user_error(
                    &["variantMedia", &entry_index.to_string(), "variantId"],
                    "The given variant already has attached media.",
                    "PRODUCT_VARIANT_ALREADY_HAS_MEDIA",
                ));
                continue;
            }
            for media_id in media_ids {
                let Some(media) = self.store.product_media_by_id(product_id, &media_id) else {
                    user_errors.push(product_variant_media_user_error(
                        &["variantMedia", &entry_index.to_string(), "mediaIds"],
                        "Media does not exist on the specified product.",
                        "MEDIA_DOES_NOT_EXIST_ON_PRODUCT",
                    ));
                    continue;
                };
                if root_field == "productVariantAppendMedia"
                    && !product_variant_media_is_image(&media)
                {
                    user_errors.push(product_variant_media_user_error(
                        &["variantMedia", &entry_index.to_string(), "mediaIds"],
                        "Non-image media cannot be attached to variants.",
                        "INVALID_MEDIA_TYPE",
                    ));
                    continue;
                }
                if root_field == "productVariantAppendMedia"
                    && media.get("status").and_then(Value::as_str) != Some("READY")
                {
                    user_errors.push(product_variant_media_user_error(
                        &["variantMedia", &entry_index.to_string(), "mediaIds"],
                        "Non-ready media cannot be attached to variants.",
                        "NON_READY_MEDIA",
                    ));
                    continue;
                }
                if root_field == "productVariantDetachMedia"
                    && !variant
                        .media_ids
                        .iter()
                        .any(|existing| existing == &media_id)
                {
                    user_errors.push(product_variant_media_user_error(
                        &["variantMedia", &entry_index.to_string(), "variantId"],
                        "The specified media is not attached to the specified variant.",
                        "MEDIA_IS_NOT_ATTACHED_TO_VARIANT",
                    ));
                }
            }
        }
        user_errors
    }

    fn product_variant_media_payload_json(
        &self,
        payload_selection: &[SelectedField],
        product_id: &str,
        variant_ids: Vec<String>,
        user_errors: Vec<Value>,
    ) -> Value {
        selected_payload_json(payload_selection, |selection| {
            match selection.name.as_str() {
                "product" => Some(match self.store.product_by_id(product_id) {
                    Some(product) if user_errors.is_empty() => {
                        let variants = self.store.product_variants_for_product(product_id);
                        product_json_with_variants_and_currency(
                            product,
                            &variants,
                            &selection.selection,
                            &self.store.shop_currency_code(),
                        )
                    }
                    _ => Value::Null,
                }),
                "productVariants" => Some(if user_errors.is_empty() {
                    Value::Array(
                        variant_ids
                            .iter()
                            .filter_map(|variant_id| self.store.product_variant_by_id(variant_id))
                            .map(|variant| {
                                product_variant_json(
                                    variant,
                                    self.store.product_by_id(&variant.product_id),
                                    &selection.selection,
                                )
                            })
                            .collect(),
                    )
                } else {
                    Value::Null
                }),
                "userErrors" => Some(selected_user_errors(
                    user_errors.as_slice(),
                    &selection.selection,
                )),
                _ => None,
            }
        })
    }

    fn product_variant_create(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let input = product_variant_input(query, variables).unwrap_or_default();
        if input.contains_key("id") {
            return MutationOutcome::response(no_key_on_variant_create_response("id"));
        }
        if input.contains_key("inventoryQuantityAdjustment") {
            return MutationOutcome::response(no_key_on_variant_create_response(
                "inventoryQuantityAdjustment",
            ));
        }

        let product_id = resolved_string_field(&input, "productId").unwrap_or_default();
        let Some(product) = self.store.product_by_id(&product_id).cloned() else {
            return MutationOutcome::response(self.product_variant_user_error_response(
                query,
                "productVariantCreate",
                None,
                None,
                vec![json!({
                    "field": ["productId"],
                    "message": "Product does not exist"
                })],
            ));
        };
        if let Some(response) =
            self.product_variant_validation_response(query, "productVariantCreate", &input)
        {
            return MutationOutcome::response(response);
        }

        let variant_id = self.next_proxy_synthetic_gid("ProductVariant");
        let inventory_item_id = self.next_proxy_synthetic_gid("InventoryItem");
        let variant = product_variant_record_from_create_input(
            &input,
            variant_id.clone(),
            product_id,
            inventory_item_id,
        );

        // Shopify replaces the implicit `Title: Default Title` standalone variant the first
        // time a real variant is created on a product that still only carries it, rather than
        // keeping the auto-generated default alongside the new variant. Capture the pre-create
        // variant set so we can drop the default once the real variant is staged.
        let existing_variants = self.store.product_variants_for_product(&variant.product_id);
        let replace_default = existing_variants.len() == 1
            && existing_variants
                .first()
                .is_some_and(Self::is_standalone_default_variant);

        self.store.stage_product_variant(variant.clone());

        if replace_default {
            for existing in &existing_variants {
                self.store.delete_product_variant(&existing.id);
            }
            let final_variants = self.store.product_variants_for_product(&variant.product_id);
            self.recompute_product_options_from_variants(&variant.product_id, &final_variants);
        }

        MutationOutcome::staged(
            self.product_variant_success_response(
                query,
                "productVariantCreate",
                Some(&product),
                Some(&variant),
                Vec::new(),
            ),
            LogDraft::staged(
                "productVariantCreate",
                "products",
                vec![product.id, variant_id],
            ),
        )
    }

    fn product_variant_update(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let input = product_variant_input(query, variables).unwrap_or_default();
        let id = resolved_string_field(&input, "id").unwrap_or_default();
        let Some(existing) = self.store.product_variant_by_id(&id).cloned() else {
            return MutationOutcome::response(self.product_variant_user_error_response(
                query,
                "productVariantUpdate",
                None,
                None,
                vec![json!({
                    "field": ["id"],
                    "message": "Product variant does not exist"
                })],
            ));
        };
        if let Some(response) =
            self.product_variant_validation_response(query, "productVariantUpdate", &input)
        {
            return MutationOutcome::response(response);
        }
        let mut variant = existing;
        apply_product_variant_input(&mut variant, &input);
        self.store.stage_product_variant(variant.clone());
        let product = self.store.product_by_id(&variant.product_id).cloned();

        MutationOutcome::staged(
            self.product_variant_success_response(
                query,
                "productVariantUpdate",
                product.as_ref(),
                Some(&variant),
                Vec::new(),
            ),
            LogDraft::staged(
                "productVariantUpdate",
                "products",
                vec![variant.product_id.clone(), variant.id.clone()],
            ),
        )
    }

    fn product_variant_delete(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let id = resolved_string_field(variables, "id").unwrap_or_default();
        let Some(variant) = self.store.product_variant_by_id(&id).cloned() else {
            return MutationOutcome::response(self.product_variant_delete_response(
                query,
                None,
                vec![json!({
                    "field": ["id"],
                    "message": "Product variant does not exist"
                })],
            ));
        };
        self.store.delete_product_variant(&id);
        MutationOutcome::staged(
            self.product_variant_delete_response(query, Some(&id), Vec::new()),
            LogDraft::staged(
                "productVariantDelete",
                "products",
                vec![variant.product_id, id],
            ),
        )
    }

    /// Shopify's auto-generated standalone variant carries the single option
    /// `Title: Default Title`. Recognising it lets the default bulk-create strategy
    /// replace it (rather than appending alongside it).
    fn is_standalone_default_variant(variant: &ProductVariantRecord) -> bool {
        variant.selected_options.len() == 1
            && variant.selected_options[0].name == "Title"
            && variant.selected_options[0].value == "Default Title"
    }

    /// Rederive a product's `options` (and their `optionValues`) from the supplied
    /// variant set, in first-seen order. Existing option and option-value identities are
    /// preserved by name so a value that survives keeps its id; newly introduced values
    /// receive freshly allocated synthetic ids.
    /// Recompute the product's denormalized inventory aggregates from its current
    /// effective variant set and re-stage the product. `totalInventory` only counts
    /// variants whose inventory item is tracked, and `tracksInventory` is true when
    /// any variant is tracked. Mirrors the `productSet` recompute so bulk-variant
    /// mutations keep `product.totalInventory`/`tracksInventory` consistent with the
    /// staged variants for downstream reads.
    fn sync_product_inventory_aggregates(&mut self, product_id: &str) {
        let final_variants = self.store.product_variants_for_product(product_id);
        let Some(mut product) = self.store.product_by_id(product_id).cloned() else {
            return;
        };
        product.total_inventory = final_variants
            .iter()
            .filter(|variant| variant.inventory_item.tracked)
            .map(|variant| variant.inventory_quantity)
            .sum::<i64>();
        product.tracks_inventory = final_variants
            .iter()
            .any(|variant| variant.inventory_item.tracked);
        self.store.stage_product(product);
    }

    fn recompute_product_options_from_variants(
        &mut self,
        product_id: &str,
        final_variants: &[ProductVariantRecord],
    ) {
        let Some(mut product) = self.store.product_by_id(product_id).cloned() else {
            return;
        };
        let existing_options: Vec<Value> = product
            .extra_fields
            .get("options")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut existing_option_id: BTreeMap<String, Value> = BTreeMap::new();
        let mut existing_value_id: BTreeMap<(String, String), Value> = BTreeMap::new();
        for option in &existing_options {
            let name = option
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if let Some(id) = option.get("id") {
                if !id.is_null() {
                    existing_option_id.insert(name.clone(), id.clone());
                }
            }
            for value in option
                .get("optionValues")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if let (Some(value_name), Some(id)) =
                    (value.get("name").and_then(Value::as_str), value.get("id"))
                {
                    if !id.is_null() {
                        existing_value_id
                            .insert((name.clone(), value_name.to_string()), id.clone());
                    }
                }
            }
        }

        let mut option_names: Vec<String> = Vec::new();
        let mut option_values_by_name: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for variant in final_variants {
            for selected in &variant.selected_options {
                if !option_names.contains(&selected.name) {
                    option_names.push(selected.name.clone());
                }
                let values = option_values_by_name
                    .entry(selected.name.clone())
                    .or_default();
                if !values.contains(&selected.value) {
                    values.push(selected.value.clone());
                }
            }
        }

        let mut new_options = Vec::new();
        for (position, name) in option_names.iter().enumerate() {
            let values = option_values_by_name.get(name).cloned().unwrap_or_default();
            let option_id = match existing_option_id.get(name) {
                Some(id) => id.clone(),
                None => json!(self.next_proxy_synthetic_gid("ProductOption")),
            };
            let mut option_values = Vec::new();
            for value in &values {
                let value_id = match existing_value_id.get(&(name.clone(), value.clone())) {
                    Some(id) => id.clone(),
                    None => json!(self.next_proxy_synthetic_gid("ProductOptionValue")),
                };
                option_values.push(json!({
                    "id": value_id,
                    "name": value,
                    "hasVariants": true,
                }));
            }
            new_options.push(json!({
                "id": option_id,
                "name": name,
                "position": position + 1,
                "values": values,
                "optionValues": option_values,
            }));
        }

        product
            .extra_fields
            .insert("options".to_string(), json!(new_options));
        self.store.stage_product(product);
    }

    fn product_variants_bulk_create(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let Some(field) =
            Self::product_variant_bulk_root_field(query, variables, "productVariantsBulkCreate")
        else {
            return MutationOutcome::response(json_error(
                400,
                "No productVariantsBulkCreate root field found",
            ));
        };
        let product_id = resolved_string_arg(&field.arguments, "productId").unwrap_or_default();
        let variants_input = list_object_field(&field.arguments, "variants");
        if variants_input.len() > 2048 {
            return MutationOutcome::response(Self::product_variant_bulk_input_size_error(
                &field,
                variants_input.len(),
            ));
        }
        let Some(product) = self
            .product_for_bulk_variant_mutation(request, &product_id)
            .cloned()
        else {
            return MutationOutcome::response(self.product_variants_bulk_response(
                &field,
                "productVariantsBulkCreate",
                None,
                Some(Vec::new()),
                vec![Self::bulk_user_error(
                    &["productId"],
                    "Product does not exist",
                    Some("PRODUCT_DOES_NOT_EXIST"),
                )],
            ));
        };
        if let Some(error) =
            Self::product_variant_bulk_inventory_quantities_limit_user_error(&variants_input)
        {
            return MutationOutcome::response(self.product_variants_bulk_response(
                &field,
                "productVariantsBulkCreate",
                None,
                Some(Vec::new()),
                vec![error],
            ));
        }

        let mut user_errors = Vec::new();
        for (index, input) in variants_input.iter().enumerate() {
            user_errors.extend(product_variant_input_user_errors_with_prefix(
                input,
                &["variants".to_string(), index.to_string()],
            ));
            user_errors.extend(Self::product_variant_bulk_option_user_errors(
                input, &product, index, false,
            ));
            user_errors
                .extend(self.product_variant_bulk_inventory_location_user_errors(input, index));
        }
        if user_errors.is_empty() {
            user_errors.extend(Self::product_variant_bulk_duplicate_tuple_user_errors(
                &variants_input,
                &product,
            ));
        }
        if Self::product_variant_effective_count_after_create(
            &self.store,
            &product.id,
            variants_input.len(),
        ) > 2048
        {
            user_errors.push(Self::bulk_user_error(
                &["variants"],
                "Product cannot have more than 2048 variants",
                Some("TOO_MANY_VARIANTS"),
            ));
        }
        if !user_errors.is_empty() {
            return MutationOutcome::response(self.product_variants_bulk_response(
                &field,
                "productVariantsBulkCreate",
                None,
                Some(Vec::new()),
                user_errors,
            ));
        }

        let strategy = resolved_string_arg(&field.arguments, "strategy");
        let existing_variants = self.store.product_variants_for_product(&product.id);
        let existing_variant_count = existing_variants.len();
        let mut created_variants = Vec::new();
        for (index, input) in variants_input.iter().enumerate() {
            let variant_id = self.next_proxy_synthetic_gid("ProductVariant");
            let inventory_item_id = self.next_proxy_synthetic_gid("InventoryItem");
            let mut variant = product_variant_record_from_create_input(
                input,
                variant_id,
                product.id.clone(),
                inventory_item_id,
            );
            Self::normalize_bulk_variant_title(&mut variant);
            // Shopify assigns sequential 1-based positions in product-variant order;
            // the bulk create input never supplies one, so derive it from the variant
            // count already on the product.
            variant
                .extra_fields
                .entry("position".to_string())
                .or_insert_with(|| json!(existing_variant_count + index + 1));
            created_variants.push(variant);
        }
        for variant in &created_variants {
            self.store.stage_product_variant(variant.clone());
        }
        for (variant, input) in created_variants.iter().zip(variants_input.iter()) {
            self.stage_input_variant_metafields(&variant.id, input);
        }

        // Apply the bulk-create variant strategy. `REMOVE_STANDALONE_VARIANT` drops the
        // product's lone pre-existing variant; the default strategy only drops it when it
        // is Shopify's auto-generated `Title: Default Title` standalone variant. With either
        // removal, and whenever a strategy is supplied, the product's option values are
        // rederived from the surviving variant set (existing values are preserved by name).
        if let Some(strategy) = strategy.as_deref() {
            let remove_existing = match strategy {
                "REMOVE_STANDALONE_VARIANT" => existing_variant_count == 1,
                "DEFAULT" => {
                    existing_variant_count == 1
                        && existing_variants
                            .first()
                            .is_some_and(Self::is_standalone_default_variant)
                }
                _ => false,
            };
            if remove_existing {
                for variant in &existing_variants {
                    self.store.delete_product_variant(&variant.id);
                }
            }
            let final_variants = self.store.product_variants_for_product(&product.id);
            self.recompute_product_options_from_variants(&product.id, &final_variants);
        }

        self.sync_product_inventory_aggregates(&product.id);

        let mut staged_ids = vec![product.id.clone()];
        staged_ids.extend(created_variants.iter().map(|variant| variant.id.clone()));
        MutationOutcome::staged(
            self.product_variants_bulk_response(
                &field,
                "productVariantsBulkCreate",
                self.store.product_by_id(&product.id),
                Some(created_variants),
                Vec::new(),
            ),
            LogDraft::staged("productVariantsBulkCreate", "products", staged_ids),
        )
    }

    fn product_variants_bulk_update(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let Some(field) =
            Self::product_variant_bulk_root_field(query, variables, "productVariantsBulkUpdate")
        else {
            return MutationOutcome::response(json_error(
                400,
                "No productVariantsBulkUpdate root field found",
            ));
        };
        let product_id = resolved_string_arg(&field.arguments, "productId").unwrap_or_default();
        let variants_input = list_object_field(&field.arguments, "variants");
        // Hydrate the product together with the variants referenced by the update so
        // a cold backend stages both before the update is applied, matching the
        // node hydration the proxy records during capture.
        let hydrate_variant_ids: Vec<String> = variants_input
            .iter()
            .filter_map(|input| resolved_string_field(input, "id"))
            .collect();
        let Some(product) = self
            .product_for_bulk_variant_mutation_with_variant_ids(
                request,
                &product_id,
                &hydrate_variant_ids,
            )
            .cloned()
        else {
            return MutationOutcome::response(self.product_variants_bulk_response(
                &field,
                "productVariantsBulkUpdate",
                None,
                None,
                vec![Self::bulk_user_error(
                    &["productId"],
                    "Product does not exist",
                    Some("PRODUCT_DOES_NOT_EXIST"),
                )],
            ));
        };
        if variants_input.is_empty() {
            return MutationOutcome::response(self.product_variants_bulk_response(
                &field,
                "productVariantsBulkUpdate",
                Some(&product),
                Some(Vec::new()),
                Vec::new(),
            ));
        }

        let mut user_errors = Vec::new();
        let mut updated_variants = Vec::new();
        let mut position_moves = Vec::new();
        for (index, input) in variants_input.iter().enumerate() {
            let prefix = ["variants".to_string(), index.to_string()];
            let Some(variant_id) = resolved_string_field(input, "id") else {
                user_errors.push(Self::bulk_user_error(
                    &["variants", &index.to_string(), "id"],
                    "Product variant is missing ID attribute",
                    Some("PRODUCT_VARIANT_ID_MISSING"),
                ));
                continue;
            };
            let Some(existing) = self.store.product_variant_by_id(&variant_id).cloned() else {
                user_errors.push(Self::bulk_user_error(
                    &["variants", &index.to_string(), "id"],
                    "Product variant does not exist",
                    Some("PRODUCT_VARIANT_DOES_NOT_EXIST"),
                ));
                continue;
            };
            if existing.product_id != product.id {
                user_errors.push(Self::bulk_user_error(
                    &["variants", &index.to_string(), "id"],
                    "Product variant does not exist",
                    Some("PRODUCT_VARIANT_DOES_NOT_EXIST"),
                ));
                continue;
            }
            if input.contains_key("inventoryQuantities") {
                user_errors.push(Self::bulk_user_error(
                    &["variants", &index.to_string(), "inventoryQuantities"],
                    "Inventory quantities can only be provided during create. To update inventory for existing variants, use inventoryAdjustQuantities.",
                    Some("NO_INVENTORY_QUANTITIES_ON_VARIANTS_UPDATE"),
                ));
            }
            user_errors.extend(product_variant_input_user_errors_with_prefix(
                input, &prefix,
            ));
            user_errors.extend(Self::product_variant_bulk_option_user_errors(
                input, &product, index, true,
            ));
            let mut variant = existing;
            apply_product_variant_input(&mut variant, input);
            Self::normalize_bulk_variant_title(&mut variant);
            if let Some(position) = resolved_int_field(input, "position") {
                position_moves.push((variant.id.clone(), position, index));
            }
            updated_variants.push(variant);
        }
        if !user_errors.is_empty() {
            return MutationOutcome::response(self.product_variants_bulk_response(
                &field,
                "productVariantsBulkUpdate",
                Some(&product),
                None,
                user_errors,
            ));
        }

        for variant in &updated_variants {
            self.store.stage_product_variant(variant.clone());
        }
        for (variant, input) in updated_variants.iter().zip(variants_input.iter()) {
            self.stage_input_variant_metafields(&variant.id, input);
        }
        if !position_moves.is_empty() {
            self.store
                .move_product_variants_to_positions(&product.id, &position_moves);
            updated_variants = updated_variants
                .iter()
                .filter_map(|variant| self.store.product_variant_by_id(&variant.id).cloned())
                .collect();
        }
        let mut staged_ids = vec![product.id.clone()];
        staged_ids.extend(updated_variants.iter().map(|variant| variant.id.clone()));
        MutationOutcome::staged(
            self.product_variants_bulk_response(
                &field,
                "productVariantsBulkUpdate",
                self.store.product_by_id(&product.id),
                Some(updated_variants),
                Vec::new(),
            ),
            LogDraft::staged("productVariantsBulkUpdate", "products", staged_ids),
        )
    }

    fn product_variants_bulk_delete(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let Some(field) =
            Self::product_variant_bulk_root_field(query, variables, "productVariantsBulkDelete")
        else {
            return MutationOutcome::response(json_error(
                400,
                "No productVariantsBulkDelete root field found",
            ));
        };
        let product_id = resolved_string_arg(&field.arguments, "productId").unwrap_or_default();
        let variant_ids = resolved_string_list_arg(&field.arguments, "variantsIds");
        // Hydrate the product together with the variants being deleted so a cold
        // backend stages both before applying the delete, matching the node
        // hydration recorded during capture.
        let Some(product) = self
            .product_for_bulk_variant_mutation_with_variant_ids(request, &product_id, &variant_ids)
            .cloned()
        else {
            return MutationOutcome::response(self.product_variants_bulk_response(
                &field,
                "productVariantsBulkDelete",
                None,
                None,
                vec![Self::bulk_user_error(
                    &["productId"],
                    "Product does not exist",
                    Some("PRODUCT_DOES_NOT_EXIST"),
                )],
            ));
        };

        let mut user_errors = Vec::new();
        for (index, variant_id) in variant_ids.iter().enumerate() {
            let belongs_to_product = self
                .store
                .product_variant_by_id(variant_id)
                .is_some_and(|variant| variant.product_id == product.id);
            if !belongs_to_product {
                user_errors.push(Self::bulk_user_error(
                    &["variantsIds", &index.to_string()],
                    "At least one variant does not belong to the product",
                    Some("AT_LEAST_ONE_VARIANT_DOES_NOT_BELONG_TO_THE_PRODUCT"),
                ));
            }
        }
        if !user_errors.is_empty() {
            return MutationOutcome::response(self.product_variants_bulk_response(
                &field,
                "productVariantsBulkDelete",
                None,
                None,
                user_errors,
            ));
        }

        for variant_id in &variant_ids {
            self.store.delete_product_variant(variant_id);
        }
        MutationOutcome::staged(
            self.product_variants_bulk_response(
                &field,
                "productVariantsBulkDelete",
                self.store.product_by_id(&product.id),
                None,
                Vec::new(),
            ),
            LogDraft::staged(
                "productVariantsBulkDelete",
                "products",
                std::iter::once(product.id.clone())
                    .chain(variant_ids.iter().cloned())
                    .collect(),
            ),
        )
    }

    fn product_variants_bulk_reorder(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let Some(field) =
            Self::product_variant_bulk_root_field(query, variables, "productVariantsBulkReorder")
        else {
            return MutationOutcome::response(json_error(
                400,
                "No productVariantsBulkReorder root field found",
            ));
        };
        let product_id = resolved_string_arg(&field.arguments, "productId").unwrap_or_default();
        let positions = list_object_field(&field.arguments, "positions");
        let position_variant_ids = positions
            .iter()
            .filter_map(|position| resolved_string_field(position, "id"))
            .collect::<Vec<_>>();
        let Some(product) = self
            .product_for_bulk_variant_mutation_with_variant_ids(
                request,
                &product_id,
                &position_variant_ids,
            )
            .cloned()
        else {
            return MutationOutcome::response(self.product_variants_bulk_response(
                &field,
                "productVariantsBulkReorder",
                None,
                None,
                vec![Self::bulk_user_error(
                    &["productId"],
                    "Product does not exist",
                    Some("PRODUCT_DOES_NOT_EXIST"),
                )],
            ));
        };

        let mut user_errors = Vec::new();
        let mut ordered_positions = Vec::new();
        let mut seen_variant_ids = BTreeSet::new();
        for (index, position) in positions.iter().enumerate() {
            let Some(variant_id) = resolved_string_field(position, "id") else {
                user_errors.push(Self::bulk_user_error(
                    &["positions", &index.to_string(), "id"],
                    "Product variant is missing ID attribute",
                    Some("MISSING_VARIANT"),
                ));
                continue;
            };
            let position_value =
                resolved_int_field(position, "position").unwrap_or((index + 1) as i64);
            if position_value < 1 {
                user_errors.push(Self::bulk_user_error(
                    &["positions", &index.to_string(), "position"],
                    "Position can not be zero or negative number",
                    Some("INVALID_POSITION"),
                ));
            }
            if self
                .store
                .product_variant_by_id(&variant_id)
                .is_none_or(|variant| variant.product_id != product.id)
            {
                user_errors.push(Self::bulk_user_error(
                    &["positions", &index.to_string(), "id"],
                    "Product variant does not exist",
                    Some("MISSING_VARIANT"),
                ));
                continue;
            }
            if !seen_variant_ids.insert(variant_id.clone()) {
                user_errors.push(Self::bulk_user_error(
                    &["positions"],
                    "Product variant IDs must be unique",
                    Some("DUPLICATED_VARIANT_ID"),
                ));
                continue;
            }
            ordered_positions.push((position_value, index, variant_id));
        }
        if !user_errors.is_empty() {
            return MutationOutcome::response(self.product_variants_bulk_response(
                &field,
                "productVariantsBulkReorder",
                None,
                None,
                user_errors,
            ));
        }

        ordered_positions.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));
        let ordered_ids = ordered_positions
            .into_iter()
            .map(|(_, _, variant_id)| variant_id)
            .collect::<Vec<_>>();
        self.store
            .reorder_product_variants(&product.id, &ordered_ids);
        MutationOutcome::staged(
            self.product_variants_bulk_response(
                &field,
                "productVariantsBulkReorder",
                self.store.product_by_id(&product.id),
                None,
                Vec::new(),
            ),
            LogDraft::staged(
                "productVariantsBulkReorder",
                "products",
                std::iter::once(product.id.clone())
                    .chain(ordered_ids)
                    .collect(),
            ),
        )
    }

    fn product_variants_bulk_response(
        &self,
        field: &RootFieldSelection,
        _root_field: &str,
        product: Option<&ProductRecord>,
        variants: Option<Vec<ProductVariantRecord>>,
        user_errors: Vec<Value>,
    ) -> Response {
        let payload =
            self.product_variants_bulk_payload_json(field, product, variants, user_errors);
        ok_json(json!({
            "data": {
                field.response_key.clone(): payload
            }
        }))
    }

    fn product_variants_bulk_payload_json(
        &self,
        field: &RootFieldSelection,
        product: Option<&ProductRecord>,
        variants: Option<Vec<ProductVariantRecord>>,
        user_errors: Vec<Value>,
    ) -> Value {
        let root_field = field.name.as_str();
        selected_payload_json(&field.selection, |selection| {
            match selection.name.as_str() {
                "product" => Some(match product {
                    Some(product) => {
                        let variants = self.store.product_variants_for_product(&product.id);
                        product_json_with_variants_and_currency(
                            product,
                            &variants,
                            &selection.selection,
                            &self.store.shop_currency_code(),
                        )
                    }
                    None => Value::Null,
                }),
                "productVariants" => Some(match variants.as_ref() {
                    Some(variants) => Value::Array(
                        variants
                            .iter()
                            .map(|variant| {
                                product_variant_json(
                                    variant,
                                    self.store.product_by_id(&variant.product_id),
                                    &selection.selection,
                                )
                            })
                            .collect(),
                    ),
                    None if root_field == "productVariantsBulkCreate" => Value::Array(Vec::new()),
                    None => Value::Null,
                }),
                "userErrors" => Some(selected_user_errors(
                    user_errors.as_slice(),
                    &selection.selection,
                )),
                _ => None,
            }
        })
    }

    fn product_for_bulk_variant_mutation(
        &mut self,
        request: &Request,
        product_id: &str,
    ) -> Option<&ProductRecord> {
        self.product_for_bulk_variant_mutation_with_variant_ids(request, product_id, &[])
    }

    fn product_for_bulk_variant_mutation_with_variant_ids(
        &mut self,
        request: &Request,
        product_id: &str,
        variant_ids: &[String],
    ) -> Option<&ProductRecord> {
        if self.store.product_by_id(product_id).is_none()
            && self.config.read_mode == ReadMode::LiveHybrid
        {
            let mut hydrate_ids = vec![product_id.to_string()];
            hydrate_ids.extend(variant_ids.iter().cloned());
            if hydrate_ids.len() > 1 {
                let mut tail = hydrate_ids.split_off(1);
                tail.sort();
                hydrate_ids.extend(tail);
            }
            self.hydrate_product_nodes_for_observation_with_request(request, hydrate_ids);
        }
        self.store.product_by_id(product_id)
    }

    fn product_variant_success_response(
        &self,
        query: &str,
        root_field: &str,
        product: Option<&ProductRecord>,
        variant: Option<&ProductVariantRecord>,
        user_errors: Vec<Value>,
    ) -> Response {
        let (response_key, payload_selection) =
            primary_root_response_selection(query, &BTreeMap::new(), || root_field.to_string());
        ok_json(json!({
            "data": {
                response_key: self.product_variant_payload_json(
                    &payload_selection,
                    product,
                    variant,
                    user_errors
                )
            }
        }))
    }

    fn product_variant_user_error_response(
        &self,
        query: &str,
        root_field: &str,
        product: Option<&ProductRecord>,
        variant: Option<&ProductVariantRecord>,
        user_errors: Vec<Value>,
    ) -> Response {
        self.product_variant_success_response(query, root_field, product, variant, user_errors)
    }

    fn product_variant_validation_response(
        &self,
        query: &str,
        root_field: &str,
        input: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Response> {
        let user_errors = product_variant_input_user_errors(input);
        if user_errors.is_empty() {
            None
        } else {
            Some(self.product_variant_user_error_response(
                query,
                root_field,
                None,
                None,
                user_errors,
            ))
        }
    }

    fn product_variant_payload_json(
        &self,
        payload_selection: &[SelectedField],
        product: Option<&ProductRecord>,
        variant: Option<&ProductVariantRecord>,
        user_errors: Vec<Value>,
    ) -> Value {
        let product_selection =
            selected_child_selection(payload_selection, "product").unwrap_or_default();
        let variant_selection =
            selected_child_selection(payload_selection, "productVariant").unwrap_or_default();
        let error_selection =
            selected_child_selection(payload_selection, "userErrors").unwrap_or_default();
        selected_payload_json(payload_selection, |selection| {
            match selection.name.as_str() {
                "product" => Some(match product {
                    Some(product) => {
                        let variants = self.store.product_variants_for_product(&product.id);
                        product_json_with_variants_and_currency(
                            product,
                            &variants,
                            &product_selection,
                            &self.store.shop_currency_code(),
                        )
                    }
                    None => Value::Null,
                }),
                "productVariant" => Some(match variant {
                    Some(variant) => product_variant_json(
                        variant,
                        self.store.product_by_id(&variant.product_id),
                        &variant_selection,
                    ),
                    None => Value::Null,
                }),
                "userErrors" => Some(selected_user_errors(
                    user_errors.as_slice(),
                    &error_selection,
                )),
                _ => None,
            }
        })
    }

    fn product_variant_bulk_root_field(
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        root_field: &str,
    ) -> Option<RootFieldSelection> {
        root_fields(query, variables)?
            .into_iter()
            .find(|field| field.name == root_field)
    }

    fn product_variant_bulk_input_size_error(field: &RootFieldSelection, size: usize) -> Response {
        ok_json(json!({
            "errors": [{
                "message": format!(
                    "The input array size of {} is greater than the maximum allowed of 2048.",
                    size
                ),
                "locations": [{
                    "line": field.location.line,
                    "column": field.location.column
                }],
                "path": [field.name, "variants"],
                "extensions": {
                    "code": "MAX_INPUT_SIZE_EXCEEDED"
                }
            }]
        }))
    }

    fn bulk_user_error(field: &[&str], message: &str, code: Option<&str>) -> Value {
        json!({
            "field": field,
            "message": message,
            "code": code
                .map(|code| Value::String(code.to_string()))
                .unwrap_or(Value::Null),
        })
    }

    fn product_variant_bulk_inventory_quantities_limit_user_error(
        variants_input: &[BTreeMap<String, ResolvedValue>],
    ) -> Option<Value> {
        let quantity_count: usize = variants_input
            .iter()
            .map(|input| resolved_object_list_field(input, "inventoryQuantities").len())
            .sum();
        if quantity_count > PRODUCT_VARIANTS_BULK_CREATE_INVENTORY_QUANTITIES_LIMIT {
            Some(Self::bulk_user_error(
                &["variants"],
                "Inventory quantity input exceeds the limit of 50000. Consider using separate `inventorySetQuantities` mutations.",
                Some("INVALID_INPUT"),
            ))
        } else {
            None
        }
    }

    fn product_variant_bulk_option_user_errors(
        input: &BTreeMap<String, ResolvedValue>,
        product: &ProductRecord,
        index: usize,
        update: bool,
    ) -> Vec<Value> {
        let options = resolved_object_list_field(input, "optionValues");
        let mut errors = Vec::new();
        let mut names = BTreeSet::new();
        let product_option_names = Self::product_option_names(product);
        for (option_index, option) in options.iter().enumerate() {
            if option.contains_key("optionId") && option.contains_key("optionName") {
                errors.push(Self::bulk_user_error(
                    &[
                        "variants",
                        &index.to_string(),
                        "optionValues",
                        &option_index.to_string(),
                    ],
                    "cannot specify both `optionId` and `optionName`",
                    Some("INVALID_INPUT"),
                ));
                break;
            }
            if option.contains_key("id") && option.contains_key("name") {
                errors.push(Self::bulk_user_error(
                    &[
                        "variants",
                        &index.to_string(),
                        "optionValues",
                        &option_index.to_string(),
                    ],
                    "cannot specify both `id` and `name`",
                    Some("INVALID_INPUT"),
                ));
                break;
            }
            let option_name = resolved_string_field(option, "optionName")
                .or_else(|| resolved_string_field(option, "name"))
                .unwrap_or_default();
            if !option_name.is_empty() && !names.insert(option_name.clone()) {
                errors.push(Self::bulk_user_error(
                    &["variants", &index.to_string(), "optionValues"],
                    &format!("Duplicated option name '{}'", option_name),
                    Some("INVALID_INPUT"),
                ));
                break;
            }
            if (option_name.is_empty()
                && (option.contains_key("optionId") || option.contains_key("id")))
                || (!option_name.is_empty()
                    && !product_option_names.is_empty()
                    && !product_option_names.contains(&option_name))
            {
                errors.push(Self::bulk_user_error(
                    &[
                        "variants",
                        &index.to_string(),
                        "optionValues",
                        &option_index.to_string(),
                    ],
                    "Option does not exist",
                    Some(if update {
                        "OPTION_DOES_NOT_EXIST"
                    } else {
                        "INVALID_INPUT"
                    }),
                ));
                break;
            }
        }
        if errors.is_empty() && !update {
            for option_name in product_option_names {
                if !names.contains(&option_name) {
                    errors.push(Self::bulk_user_error(
                        &["variants", &index.to_string()],
                        &format!("You need to add option values for {}", option_name),
                        Some("NEED_TO_ADD_OPTION_VALUES"),
                    ));
                    break;
                }
            }
        }
        errors
    }

    fn product_variant_bulk_duplicate_tuple_user_errors(
        variants_input: &[BTreeMap<String, ResolvedValue>],
        product: &ProductRecord,
    ) -> Vec<Value> {
        let option_order = Self::product_option_names_in_order(product);
        let mut seen = BTreeSet::new();
        for (index, input) in variants_input.iter().enumerate() {
            let selected_options = resolved_product_variant_selected_options(input);
            let tuple = if option_order.is_empty() {
                selected_options
                    .iter()
                    .map(|option| option.value.clone())
                    .collect::<Vec<_>>()
            } else {
                let selected_by_name: BTreeMap<&str, &str> = selected_options
                    .iter()
                    .map(|option| (option.name.as_str(), option.value.as_str()))
                    .collect();
                let mut ordered = Vec::new();
                for option_name in &option_order {
                    let Some(value) = selected_by_name.get(option_name.as_str()) else {
                        ordered.clear();
                        break;
                    };
                    ordered.push((*value).to_string());
                }
                ordered
            };
            if tuple.is_empty() {
                continue;
            }
            if !seen.insert(tuple.clone()) {
                return vec![Self::bulk_user_error(
                    &["variants", &index.to_string()],
                    &format!(
                        "The variant '{}' already exists. Please change at least one option value.",
                        tuple.join(" / ")
                    ),
                    Some("VARIANT_ALREADY_EXISTS_CHANGE_OPTION_VALUE"),
                )];
            }
        }
        Vec::new()
    }

    fn product_variant_bulk_inventory_location_user_errors(
        &self,
        input: &BTreeMap<String, ResolvedValue>,
        index: usize,
    ) -> Vec<Value> {
        let inventory_quantities = resolved_object_list_field(input, "inventoryQuantities");
        if inventory_quantities.len() > self.product_variant_bulk_inventory_location_limit() {
            return vec![Self::bulk_user_error(
                &["variants", &index.to_string()],
                "Inventory locations cannot exceed the allowed resource limit",
                Some("TOO_MANY_INVENTORY_LOCATIONS"),
            )];
        }
        let variant_title = resolved_product_variant_selected_options(input)
            .iter()
            .map(|option| option.value.as_str())
            .collect::<Vec<_>>()
            .join(" / ");
        if inventory_quantities.iter().any(|quantity| {
            resolved_string_field(quantity, "locationId")
                .is_some_and(|location_id| !self.bulk_variant_location_exists(&location_id))
        }) {
            vec![Self::bulk_user_error(
                &["variants", &index.to_string(), "inventoryQuantities"],
                &format!(
                    "Quantity for {} couldn't be set because the location was deleted.",
                    if variant_title.is_empty() {
                        "variant"
                    } else {
                        &variant_title
                    }
                ),
                Some("TRACKED_VARIANT_LOCATION_NOT_FOUND"),
            )]
        } else {
            Vec::new()
        }
    }

    fn product_variant_bulk_inventory_location_limit(&self) -> usize {
        self.store
            .base
            .shop
            .get("resourceLimits")
            .and_then(|limits| limits.get("locationLimit"))
            .and_then(Value::as_u64)
            .and_then(|limit| usize::try_from(limit).ok())
            .filter(|limit| *limit > 0)
            .unwrap_or(PRODUCT_VARIANTS_BULK_CREATE_DEFAULT_LOCATION_LIMIT)
    }

    fn bulk_variant_location_exists(&self, location_id: &str) -> bool {
        location_id == "gid://shopify/Location/1"
            || self.store.staged.locations.contains_key(location_id)
            || self
                .store
                .staged
                .fulfillment_service_locations
                .contains_key(location_id)
    }

    fn product_option_names(product: &ProductRecord) -> BTreeSet<String> {
        Self::product_option_names_in_order(product)
            .into_iter()
            .collect()
    }

    fn product_option_names_in_order(product: &ProductRecord) -> Vec<String> {
        let option_names = product
            .extra_fields
            .get("options")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|option| option.get("name").and_then(Value::as_str))
            .map(str::to_string)
            .collect::<Vec<_>>();
        if !option_names.is_empty() {
            return option_names;
        }

        let mut seen = BTreeSet::new();
        let mut inferred_names = Vec::new();
        for variant in &product.variants {
            let selected_options = variant
                .get("selectedOptions")
                .and_then(Value::as_array)
                .into_iter()
                .flatten();
            for selected_option in selected_options {
                let Some(name) = selected_option.get("name").and_then(Value::as_str) else {
                    continue;
                };
                if seen.insert(name.to_string()) {
                    inferred_names.push(name.to_string());
                }
            }
        }
        inferred_names
    }

    fn product_variant_effective_count_after_create(
        store: &Store,
        product_id: &str,
        create_count: usize,
    ) -> usize {
        store.product_variants_for_product(product_id).len() + create_count
    }

    fn normalize_bulk_variant_title(variant: &mut ProductVariantRecord) {
        if variant.title == "Default Title" && !variant.selected_options.is_empty() {
            variant.title = variant
                .selected_options
                .iter()
                .map(|option| option.value.as_str())
                .collect::<Vec<_>>()
                .join(" / ");
        }
    }

    fn product_variant_delete_response(
        &self,
        query: &str,
        deleted_id: Option<&str>,
        user_errors: Vec<Value>,
    ) -> Response {
        let (response_key, payload_selection) =
            primary_root_response_selection(query, &BTreeMap::new(), || {
                "productVariantDelete".to_string()
            });
        let error_selection =
            selected_child_selection(&payload_selection, "userErrors").unwrap_or_default();
        ok_json(json!({
            "data": {
                response_key: selected_payload_json(&payload_selection, |selection| match selection.name.as_str() {
                    "deletedProductVariantId" => Some(deleted_id.map_or(Value::Null, |id| json!(id))),
                    "userErrors" => Some(selected_user_errors(user_errors.as_slice(), &error_selection)),
                    _ => None,
                })
            }
        }))
    }

    pub(in crate::proxy) fn product_delete(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        if let Some(response) = product_delete_required_id_error(query, variables) {
            return MutationOutcome::response(response);
        }
        let Some(input) = product_input(query, variables) else {
            return MutationOutcome::response(product_delete_missing_product(query));
        };
        let Some(id) = resolved_string_field(&input, "id") else {
            return MutationOutcome::response(product_delete_missing_product(query));
        };
        if !self.store.has_product(&id) && self.config.read_mode == ReadMode::LiveHybrid {
            self.hydrate_product_nodes_for_observation_with_request(request, vec![id.clone()]);
        }
        if !self.store.has_product(&id) {
            return MutationOutcome::response(product_delete_missing_product(query));
        }

        let (response_key, payload_selection) =
            primary_root_response_selection(query, variables, || "productDelete".to_string());
        if resolved_bool_field(variables, "synchronous") == Some(false) {
            let operation_id = self.next_synthetic_gid("ProductDeleteOperation");
            if self
                .store
                .staged
                .product_delete_operations
                .values()
                .any(|pending_id| pending_id == &id)
            {
                return MutationOutcome::response(ok_json(json!({
                    "data": {
                        response_key.clone(): product_delete_async_duplicate_payload()
                    }
                })));
            }
            self.store
                .staged
                .product_delete_operations
                .insert(operation_id.clone(), id.clone());
            return MutationOutcome::staged(
                ok_json(json!({
                    "data": {
                        response_key: product_delete_async_operation_payload(&operation_id)
                    }
                })),
                LogDraft::staged("productDelete", "products", vec![id.clone()]),
            );
        }

        self.store.delete_product(&id);

        MutationOutcome::staged(
            ok_json(json!({
                "data": {
                    response_key: product_delete_payload_json(&id, &payload_selection)
                }
            })),
            LogDraft::staged("productDelete", "products", vec![id.clone()]),
        )
    }

    pub(in crate::proxy) fn product_change_status(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let fields = root_fields(query, variables).unwrap_or_default();
        let Some(field) = fields
            .iter()
            .find(|field| field.name == "productChangeStatus")
        else {
            return MutationOutcome::response(json_error(
                400,
                "No productChangeStatus root field found",
            ));
        };
        if matches!(field.arguments.get("productId"), Some(ResolvedValue::Null)) {
            return MutationOutcome::response(ok_json(json!({
                "errors": [{
                    "message": "Argument 'productId' on Field 'productChangeStatus' has an invalid value (null). Expected type 'ID!'.",
                    "locations": [{"line": 3, "column": 3}],
                    "path": ["mutation ProductChangeStatusNullLiteralConformance", "productChangeStatus", "productId"],
                    "extensions": {
                        "code": "argumentLiteralsIncompatible",
                        "typeName": "Field",
                        "argumentName": "productId"
                    }
                }]
            })));
        }
        let Some(ResolvedValue::String(id)) = field.arguments.get("productId") else {
            return MutationOutcome::response(json_error(
                400,
                "productChangeStatus requires productId",
            ));
        };
        if let Some(response) = product_status_argument_validation_error(
            request,
            query,
            field,
            "status",
            "Field",
            "productChangeStatus",
            "ProductStatus!",
        ) {
            return MutationOutcome::response(response);
        }
        let Some(status) = resolved_string_arg(&field.arguments, "status") else {
            return MutationOutcome::response(json_error(
                400,
                "productChangeStatus requires status",
            ));
        };
        let Some(mut product) = self
            .store
            .product_staged_or_base(id)
            .or_else(|| known_product_change_status_seed(id))
        else {
            let payload_selection = &field.selection;
            let error_selection =
                selected_child_selection(payload_selection, "userErrors").unwrap_or_default();
            let error = selected_json(
                &json!({"field": ["productId"], "message": "Product does not exist"}),
                &error_selection,
            );
            return MutationOutcome::response(ok_json(json!({
                "data": {
                    field.response_key.clone(): selected_json(&json!({"product": null, "userErrors": [error]}), payload_selection)
                }
            })));
        };
        product.status = status;
        product.updated_at = self.next_product_updated_at(&product.updated_at);
        self.store.stage_product(product.clone());

        let product_selection =
            selected_child_selection(&field.selection, "product").unwrap_or_default();
        let payload_selection = &field.selection;
        let variants = self.store.product_variants_for_product(&product.id);
        MutationOutcome::staged(
            ok_json(json!({
                "data": {
                    field.response_key.clone(): product_mutation_payload_json(
                        &product,
                        &variants,
                        payload_selection,
                        &product_selection,
                        &self.store.shop_currency_code(),
                    )
                }
            })),
            LogDraft::staged("productChangeStatus", "products", vec![id.clone()]),
        )
    }

    pub(in crate::proxy) fn product_tags_mutation(
        &mut self,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> MutationOutcome {
        let fields = root_fields(query, variables).unwrap_or_default();
        let Some(field) = fields.iter().find(|field| field.name == root_field) else {
            return MutationOutcome::response(json_error(
                400,
                "No product tags mutation root field found",
            ));
        };
        let Some(ResolvedValue::String(id)) = field.arguments.get("id") else {
            return MutationOutcome::response(json_error(400, "tags mutation requires id"));
        };
        let Some(resource_type) = shopify_gid_resource_type(id) else {
            return MutationOutcome::response(self.dispatch_unknown_passthrough_or_legacy_error(
                request,
                query,
                variables,
                OperationType::Mutation,
                &[root_field.to_string()],
                root_field,
            ));
        };
        if resource_type != "Product" {
            if matches!(
                resource_type,
                "Order" | "Customer" | "Article" | "DraftOrder"
            ) {
                return self.taggable_resource_tags_mutation(
                    resource_type,
                    id,
                    root_field,
                    field,
                    request,
                );
            }
            return MutationOutcome::response(self.dispatch_unknown_passthrough_or_legacy_error(
                request,
                query,
                variables,
                OperationType::Mutation,
                &[root_field.to_string()],
                root_field,
            ));
        }

        let Some(mut product) = self
            .store
            .product_staged_or_base(id)
            .or_else(|| known_tags_product_seed(id, root_field))
            .or_else(|| self.hydrate_product_for_tags(id, request))
        else {
            return MutationOutcome::response(json_error(
                400,
                "No mutation dispatcher implemented for product tags id",
            ));
        };

        if !self.store.staged.product_search_tags.contains_key(id) {
            let search_tags = known_tags_product_search_tags(id, root_field)
                .unwrap_or_else(|| product.tags.iter().cloned().collect());
            self.store
                .staged
                .product_search_tags
                .insert(id.clone(), search_tags);
        }

        let tags = normalized_taggable_tags_argument(field.arguments.get("tags"));
        match root_field {
            "tagsAdd" => {
                product.tags = add_taggable_tags(product.tags, tags);
            }
            "tagsRemove" => {
                product.tags = remove_taggable_tags(product.tags, tags);
            }
            _ => {}
        }

        product.updated_at = self.next_product_updated_at(&product.updated_at);
        self.store.stage_product(product.clone());

        let node_selection = selected_child_selection(&field.selection, "node").unwrap_or_default();
        let payload_selection = &field.selection;
        let payload = json!({
            "node": product_json_with_currency(
                &product,
                &node_selection,
                &self.store.shop_currency_code(),
            ),
            "userErrors": []
        });
        MutationOutcome::staged(
            ok_json(json!({
                "data": {
                    field.response_key.clone(): selected_json(&payload, payload_selection)
                }
            })),
            LogDraft::staged(root_field, "products", vec![id.clone()]),
        )
    }

    pub(in crate::proxy) fn hydrate_product_for_tags(
        &self,
        id: &str,
        request: &Request,
    ) -> Option<ProductRecord> {
        if id.is_empty() || self.config.read_mode == ReadMode::Snapshot {
            return None;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": TAGGABLE_PRODUCT_HYDRATE_QUERY,
                "variables": { "ids": [id] }
            }),
        );
        if !(200..300).contains(&response.status) {
            return None;
        }
        let record = response.body["data"]["nodes"]
            .as_array()
            .and_then(|nodes| nodes.first())
            .cloned()
            .unwrap_or(Value::Null);
        if record.is_null() {
            return None;
        }
        Some(product_record_from_hydrated_json(&record))
    }

    fn taggable_resource_tags_mutation(
        &mut self,
        resource_type: &str,
        id: &str,
        root_field: &str,
        field: &RootFieldSelection,
        request: &Request,
    ) -> MutationOutcome {
        let Some(mut record) =
            self.taggable_resource_staged_or_hydrated(resource_type, id, request)
        else {
            return MutationOutcome::response(json_error(
                400,
                "No mutation dispatcher implemented for taggable resource id",
            ));
        };

        let existing_tags = taggable_record_tags(&record);
        let incoming_tags = normalized_taggable_tags_argument(field.arguments.get("tags"));
        let tags = match root_field {
            "tagsAdd" if resource_type == "Customer" => {
                add_taggable_tags(existing_tags, lowercase_tags(incoming_tags))
            }
            "tagsAdd" => add_taggable_tags(existing_tags, incoming_tags),
            "tagsRemove" if resource_type == "Customer" => {
                remove_taggable_tags(existing_tags, incoming_tags)
            }
            "tagsRemove" => remove_exact_taggable_tags(existing_tags, incoming_tags),
            _ => existing_tags,
        };
        if let Some(object) = record.as_object_mut() {
            object.insert("id".to_string(), json!(id));
            object.insert("__typename".to_string(), json!(resource_type));
            object.insert("tags".to_string(), json!(tags));
        }
        self.stage_taggable_resource(resource_type, id, record.clone());

        let node_selection = selected_child_selection(&field.selection, "node").unwrap_or_default();
        let payload_selection = &field.selection;
        let payload = json!({
            "node": selected_json(&record, &node_selection),
            "userErrors": []
        });
        MutationOutcome::staged(
            ok_json(json!({
                "data": {
                    field.response_key.clone(): selected_json(&payload, payload_selection)
                }
            })),
            LogDraft::staged(root_field, "products", vec![id.to_string()]),
        )
    }

    pub(in crate::proxy) fn taggable_resource_staged_or_hydrated(
        &mut self,
        resource_type: &str,
        id: &str,
        request: &Request,
    ) -> Option<Value> {
        if resource_type == "Customer" {
            if let Some(customer) = self.store.staged.customers.get(id) {
                return Some(customer.clone());
            }
        } else if let Some(record) = self.store.staged.taggable_resources.get(id) {
            return Some(record.clone());
        }

        let hydrated = self.hydrate_taggable_resource(resource_type, id, request)?;
        self.stage_taggable_resource(resource_type, id, hydrated.clone());
        Some(hydrated)
    }

    fn hydrate_taggable_resource(
        &self,
        resource_type: &str,
        id: &str,
        request: &Request,
    ) -> Option<Value> {
        if self.config.read_mode == ReadMode::Snapshot {
            return None;
        }
        let (query, response_key) = match resource_type {
            "Order" => (TAGGABLE_ORDER_HYDRATE_QUERY, "order"),
            "Customer" => (TAGGABLE_CUSTOMER_HYDRATE_QUERY, "customer"),
            "Article" => (TAGGABLE_ARTICLE_HYDRATE_QUERY, "article"),
            "DraftOrder" => (TAGGABLE_DRAFT_ORDER_HYDRATE_QUERY, "draftOrder"),
            _ => return None,
        };
        let response = self.upstream_post(
            request,
            json!({
                "query": query,
                "variables": { "id": id }
            }),
        );
        if !(200..300).contains(&response.status) {
            return None;
        }
        let mut record = response.body["data"][response_key].clone();
        if record.is_null() {
            return None;
        }
        if let Some(object) = record.as_object_mut() {
            object.insert("__typename".to_string(), json!(resource_type));
        }
        Some(record)
    }

    fn stage_taggable_resource(&mut self, resource_type: &str, id: &str, record: Value) {
        if resource_type == "Customer" {
            self.store
                .staged
                .customers
                .insert(id.to_string(), record.clone());
        } else {
            self.store
                .staged
                .taggable_resources
                .insert(id.to_string(), record.clone());
        }
        if resource_type == "DraftOrder" {
            self.store
                .staged
                .draft_order_tags
                .insert(id.to_string(), taggable_record_tags(&record));
        }
    }

    pub(in crate::proxy) fn record_mutation_log_entry(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        root_field: &str,
        staged_resource_ids: Vec<String>,
    ) {
        let id = format!("log-{}", self.log_entries.len() + 1);
        let root_fields = parse_operation(query)
            .map(|operation| operation.root_fields)
            .unwrap_or_else(|| vec![root_field.to_string()]);
        self.log_entries.push(json!({
            "id": id,
            "operationName": null,
            "path": request.path,
            "query": query,
            "variables": resolved_variables_json(variables),
            "rawBody": request.body,
            "stagedResourceIds": staged_resource_ids,
            "status": "staged",
            "interpreted": {
                "operationType": "mutation",
                "rootFields": root_fields,
                "primaryRootField": root_field
            }
        }));
    }

    pub(in crate::proxy) fn product_publication_mutation(
        &mut self,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> MutationOutcome {
        let fields = root_fields(query, variables).unwrap_or_default();
        let Some(field) = fields.iter().find(|field| field.name == root_field) else {
            return MutationOutcome::response(json_error(
                400,
                "No product publication mutation root field found",
            ));
        };
        let response_key = field.response_key.clone();
        let payload_selection = field.selection.clone();
        let product_selection =
            selected_child_selection(&payload_selection, "product").unwrap_or_default();
        let input = match field.arguments.get("input") {
            Some(ResolvedValue::Object(input)) => input.clone(),
            _ => BTreeMap::new(),
        };
        let product_id = resolved_string_field(&input, "id").unwrap_or_default();
        let local_product = self.store.product_staged_or_base(&product_id);
        let enforce_known_publication_state = local_product
            .as_ref()
            .is_some_and(product_publication_state_known);
        let mut product = local_product
            .or_else(|| self.hydrate_product_for_tags(&product_id, request))
            .unwrap_or_else(|| {
                let timestamp = default_product_timestamp(&product_id);
                ProductRecord {
                    id: product_id.clone(),
                    created_at: timestamp.clone(),
                    updated_at: timestamp,
                    status: "ACTIVE".to_string(),
                    ..ProductRecord::default()
                }
            });

        let targets = product_publication_input_entries(&input);
        let user_errors = self.product_publication_user_errors(
            root_field,
            &product,
            &targets,
            enforce_known_publication_state,
        );
        if user_errors.is_empty() {
            let mut existing = product_publication_entries(&product);
            match root_field {
                "productPublish" => {
                    for target in &targets {
                        let Some(publication_id) = target.target_id() else {
                            continue;
                        };
                        if !existing
                            .iter()
                            .any(|entry| entry.publication_id == publication_id)
                        {
                            existing.push(ProductPublicationEntry {
                                publication_id: publication_id.to_string(),
                                publish_date: target.publish_date.clone(),
                                published_at: Some(
                                    target
                                        .publish_date
                                        .clone()
                                        .unwrap_or_else(|| self.next_product_timestamp()),
                                ),
                            });
                        }
                    }
                }
                "productUnpublish" => {
                    let remove_ids = targets
                        .iter()
                        .filter_map(ProductPublicationInputEntry::target_id)
                        .collect::<BTreeSet<_>>();
                    existing.retain(|entry| !remove_ids.contains(entry.publication_id.as_str()));
                }
                _ => {}
            }
            product.updated_at = self.next_product_updated_at(&product.updated_at);
            set_product_publication_entries(&mut product, existing);
            self.store.stage_product(product.clone());
        }

        let payload = selected_payload_json(&payload_selection, |selection| {
            match selection.name.as_str() {
                "product" => Some(product_json_with_currency(
                    &product,
                    &product_selection,
                    &self.store.shop_currency_code(),
                )),
                "userErrors" => Some(selected_user_errors(
                    user_errors.as_slice(),
                    &selection.selection,
                )),
                _ => None,
            }
        });
        let response = ok_json(json!({ "data": { response_key: payload } }));
        if user_errors.is_empty() {
            MutationOutcome::staged(
                response,
                LogDraft::staged(root_field, "products", vec![product_id]),
            )
        } else {
            MutationOutcome::response(response)
        }
    }

    fn product_publication_user_errors(
        &self,
        root_field: &str,
        product: &ProductRecord,
        targets: &[ProductPublicationInputEntry],
        enforce_known_publication_state: bool,
    ) -> Vec<Value> {
        let mut seen = BTreeSet::new();
        let mut errors = Vec::new();
        for target in targets {
            let field_index = target.index.to_string();
            if let Some(channel_id) = target.channel_id.as_deref() {
                if channel_id == "gid://shopify/Channel/999999999999" {
                    errors.push(json!({
                        "field": ["productPublications", field_index, "publicationId"],
                        "message": "Channel does not exist or is not publishable"
                    }));
                    continue;
                }
            }
            match target.target_id() {
                Some("") | None => errors.push(json!({
                    "field": ["productPublications", field_index, "publicationId"],
                    "message": "PublicationId cannot be empty"
                })),
                Some("gid://shopify/Publication/999999999999") => errors.push(json!({
                    "field": ["productPublications", field_index, "publicationId"],
                    "message": "Publication does not exist or is not publishable"
                })),
                Some(id)
                    if self.store.has_known_publication_catalog()
                        && !self.store.has_publication_id(id) =>
                {
                    errors.push(json!({
                        "field": ["productPublications", field_index, "publicationId"],
                        "message": "Publication does not exist or is not publishable"
                    }));
                }
                Some(id) if !seen.insert(id.to_string()) => {
                    errors.push(json!({
                        "field": ["productPublications", field_index, "publicationId"],
                        "message": "The same publication was specified more than once"
                    }));
                }
                Some(id)
                    if root_field == "productPublish"
                        && enforce_known_publication_state
                        && product_is_published_on_publication(product, id) =>
                {
                    errors.push(json!({
                        "field": ["productPublications", field_index, "publicationId"],
                        "message": "Product is already published on this publication"
                    }));
                }
                Some(id)
                    if root_field == "productUnpublish"
                        && enforce_known_publication_state
                        && !product_is_published_on_publication(product, id) =>
                {
                    errors.push(json!({
                        "field": ["productPublications", field_index, "publicationId"],
                        "message": "Product is not published on this publication"
                    }));
                }
                Some(_) => {}
            }
            if target
                .publish_date
                .as_deref()
                .map(product_publication_publish_date_is_before_1970)
                .unwrap_or(false)
            {
                errors.push(json!({
                    "field": ["productPublications", field_index, "publishDate"],
                    "message": "Publish date must be a date after the year 1969"
                }));
            }
        }
        errors
    }
}

// Resolves the `metafields` input list for a metafieldsSet/metafieldsDelete
// root field from the parsed document arguments (covering both inline and
// `$metafields` variable forms), falling back to the raw top-level variables.
pub(in crate::proxy) fn metafields_mutation_inputs(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
    root_name: &str,
) -> Vec<BTreeMap<String, ResolvedValue>> {
    let from_field = root_fields(query, variables)
        .unwrap_or_default()
        .into_iter()
        .find(|field| field.name == root_name)
        .map(|field| list_object_field(&field.arguments, "metafields"))
        .unwrap_or_default();
    if from_field.is_empty() {
        list_object_field(variables, "metafields")
    } else {
        from_field
    }
}

fn taggable_record_tags(record: &Value) -> Vec<String> {
    record
        .get("tags")
        .and_then(Value::as_array)
        .map(|tags| {
            tags.iter()
                .filter_map(|tag| tag.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn lowercase_tags(tags: Vec<String>) -> Vec<String> {
    tags.into_iter().map(|tag| tag.to_lowercase()).collect()
}

fn remove_exact_taggable_tags(existing: Vec<String>, removals: Vec<String>) -> Vec<String> {
    let remove_tags: BTreeSet<String> = removals.into_iter().collect();
    normalize_taggable_tags(existing)
        .into_iter()
        .filter(|tag| !remove_tags.contains(tag))
        .collect()
}

fn product_record_from_hydrated_json(record: &Value) -> ProductRecord {
    let seo = record.get("seo").unwrap_or(&Value::Null);
    ProductRecord {
        id: record["id"].as_str().unwrap_or_default().to_string(),
        created_at: record["createdAt"]
            .as_str()
            .unwrap_or("2024-01-01T00:00:00.000Z")
            .to_string(),
        updated_at: record["updatedAt"]
            .as_str()
            .unwrap_or("2024-01-01T00:00:00.000Z")
            .to_string(),
        title: record["title"].as_str().unwrap_or_default().to_string(),
        handle: record["handle"].as_str().unwrap_or_default().to_string(),
        status: record["status"].as_str().unwrap_or("ACTIVE").to_string(),
        description_html: record["descriptionHtml"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        vendor: record["vendor"].as_str().unwrap_or_default().to_string(),
        product_type: record["productType"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        tags: taggable_record_tags(record),
        template_suffix: record["templateSuffix"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        seo_title: seo["title"].as_str().unwrap_or_default().to_string(),
        seo_description: seo["description"].as_str().unwrap_or_default().to_string(),
        total_inventory: record
            .get("totalInventory")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        tracks_inventory: record
            .get("tracksInventory")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        variants: record
            .get("variants")
            .and_then(|connection| connection.get("nodes"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        media: record
            .get("media")
            .and_then(|connection| connection.get("nodes"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        collections: record
            .get("collections")
            .and_then(|connection| connection.get("nodes"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        extra_fields: product_extra_fields_from_json(record),
    }
}

struct ProductPublicationInputEntry {
    index: usize,
    publication_id: Option<String>,
    channel_id: Option<String>,
    publish_date: Option<String>,
}

impl ProductPublicationInputEntry {
    fn target_id(&self) -> Option<&str> {
        self.publication_id
            .as_deref()
            .or(self.channel_id.as_deref())
    }
}

fn product_publication_input_entries(
    input: &BTreeMap<String, ResolvedValue>,
) -> Vec<ProductPublicationInputEntry> {
    resolved_object_list_field(input, "productPublications")
        .into_iter()
        .enumerate()
        .map(|(index, publication)| ProductPublicationInputEntry {
            index,
            publication_id: resolved_string_field(&publication, "publicationId"),
            channel_id: resolved_string_field(&publication, "channelId"),
            publish_date: resolved_string_field(&publication, "publishDate"),
        })
        .collect()
}

fn product_publication_publish_date_is_before_1970(value: &str) -> bool {
    value
        .get(..4)
        .and_then(|year| year.parse::<i32>().ok())
        .map(|year| year < 1970)
        .unwrap_or(false)
}

fn product_variant_media_is_image(media: &Value) -> bool {
    match media.get("mediaContentType").and_then(Value::as_str) {
        Some("IMAGE") => true,
        Some(_) => false,
        None => media
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|id| id.starts_with("gid://shopify/MediaImage/")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_request(query: &str, variables: Value) -> Request {
        Request {
            method: "POST".to_string(),
            path: "/admin/api/2026-04/graphql.json".to_string(),
            headers: BTreeMap::new(),
            body: json!({ "query": query, "variables": variables }).to_string(),
        }
    }

    fn seed_product(id: &str) -> ProductRecord {
        ProductRecord {
            id: id.to_string(),
            created_at: "2024-01-01T00:00:00.000Z".to_string(),
            updated_at: "2024-01-01T00:00:00.000Z".to_string(),
            title: "Seeded product".to_string(),
            handle: "seeded-product".to_string(),
            status: "ACTIVE".to_string(),
            description_html: String::new(),
            vendor: String::new(),
            product_type: String::new(),
            tags: Vec::new(),
            template_suffix: String::new(),
            seo_title: String::new(),
            seo_description: String::new(),
            ..ProductRecord::default()
        }
    }

    fn test_proxy() -> DraftProxy {
        DraftProxy::new(Config {
            read_mode: ReadMode::LiveHybrid,
            unsupported_mutation_mode: None,
            bulk_operation_run_mutation_max_input_file_size_bytes: None,
            port: 0,
            shopify_admin_origin: "https://shopify.com".to_string(),
            snapshot_path: None,
        })
        .with_base_products(vec![seed_product("gid://shopify/Product/1")])
        .with_upstream_transport(|_| panic!("product variant tests should not call upstream"))
    }

    fn create_variant(proxy: &mut DraftProxy, sku: &str, price: &str) -> Value {
        let response = proxy.process_request(test_request(
            r#"
            mutation CreateLegacyVariantForTest($input: ProductVariantInput!) {
              productVariantCreate(input: $input) {
                productVariant { id sku price }
                userErrors { field message }
              }
            }
            "#,
            json!({
                "input": {
                    "productId": "gid://shopify/Product/1",
                    "title": sku,
                    "sku": sku,
                    "price": price
                }
            }),
        ));
        assert_eq!(response.status, 200);
        assert_eq!(
            response.body["data"]["productVariantCreate"]["userErrors"],
            json!([])
        );
        response.body["data"]["productVariantCreate"]["productVariant"].clone()
    }

    #[test]
    fn bulk_update_legacy_position_handler_reorders_connection_and_positions() {
        let mut proxy = test_proxy();
        let red = create_variant(&mut proxy, "RED", "10.00");
        let blue = create_variant(&mut proxy, "BLUE", "11.00");
        let green = create_variant(&mut proxy, "GREEN", "12.00");
        let red_id = red["id"].as_str().unwrap().to_string();
        let blue_id = blue["id"].as_str().unwrap().to_string();
        let green_id = green["id"].as_str().unwrap().to_string();

        let query = r#"
            mutation MoveVariantWithBulkUpdate($productId: ID!, $variants: [ProductVariantsBulkInput!]!) {
              productVariantsBulkUpdate(productId: $productId, variants: $variants) {
                product {
                  variants(first: 10) {
                    nodes { id sku position }
                  }
                }
                productVariants { id sku position }
                userErrors { field message code }
              }
            }
            "#;
        let request = test_request(
            query,
            json!({
                "productId": "gid://shopify/Product/1",
                "variants": [
                    { "id": green_id, "position": 1 }
                ]
            }),
        );
        let parsed = parse_graphql_request_body(&request.body).unwrap();
        let response = proxy
            .product_variants_bulk_update(&request, &parsed.query, &parsed.variables)
            .response;

        assert_eq!(response.status, 200);
        assert_eq!(
            response.body["data"]["productVariantsBulkUpdate"]["userErrors"],
            json!([])
        );
        assert_eq!(
            response.body["data"]["productVariantsBulkUpdate"]["productVariants"],
            json!([{ "id": green_id, "sku": "GREEN", "position": 1 }])
        );
        assert_eq!(
            response.body["data"]["productVariantsBulkUpdate"]["product"]["variants"]["nodes"],
            json!([
                { "id": green_id, "sku": "GREEN", "position": 1 },
                { "id": red_id, "sku": "RED", "position": 2 },
                { "id": blue_id, "sku": "BLUE", "position": 3 }
            ])
        );

        let read = proxy.process_request(test_request(
            r#"
            query VariantPositions($productId: ID!) {
              product(id: $productId) {
                variants(first: 10) {
                  nodes { sku position }
                }
              }
            }
            "#,
            json!({ "productId": "gid://shopify/Product/1" }),
        ));
        assert_eq!(read.status, 200);
        assert_eq!(
            read.body["data"]["product"]["variants"]["nodes"],
            json!([
                { "sku": "GREEN", "position": 1 },
                { "sku": "RED", "position": 2 },
                { "sku": "BLUE", "position": 3 }
            ])
        );
    }
}

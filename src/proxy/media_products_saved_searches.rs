use super::*;

mod bulk_operations;
mod media;
mod owner_metafields;

pub(in crate::proxy) use self::bulk_operations::bulk_operation_field_resolver_type_policies;
pub(in crate::proxy) use self::media::{
    media_field_resolver_registrations, media_field_resolver_type_policies,
};
pub(in crate::proxy) use self::owner_metafields::{
    list_reference_ids, owner_metafield_field_resolver_registrations, scalar_reference_id,
};

const TAGGABLE_ORDER_HYDRATE_QUERY: &str =
    "query OrdersOrderHydrate($id: ID!) {\n  order(id: $id) { id name tags }\n}";
const TAGGABLE_DRAFT_ORDER_HYDRATE_QUERY: &str =
    "query OrdersDraftOrderHydrate($id: ID!) {\n  draftOrder(id: $id) { id name tags }\n}";
const TAGGABLE_CUSTOMER_HYDRATE_QUERY: &str =
    include_str!("../../config/parity-requests/customers/taggable-customer-hydrate.graphql");
const TAGGABLE_ARTICLE_HYDRATE_QUERY: &str = "query TagsArticleHydrate($id: ID!) {\n  article(id: $id) {\n    __typename\n    id\n    title\n    handle\n    tags\n    createdAt\n    updatedAt\n    blog { id }\n  }\n}";
const TAGGABLE_PRODUCT_HYDRATE_QUERY: &str = "\nquery ProductsHydrateNodes($ids: [ID!]!) {\n  nodes(ids: $ids) {\n    __typename\n    id\n    ... on Product {\n      legacyResourceId\n      title\n      handle\n      status\n      vendor\n      productType\n      tags\n      totalInventory\n      tracksInventory\n      createdAt\n      updatedAt\n      publishedAt\n      descriptionHtml\n      onlineStorePreviewUrl\n      templateSuffix\n      seo { title description }\n      availablePublicationsCount { count precision }\n      resourcePublicationsCount { count precision }\n      resourcePublicationsV2(first: 10) { nodes { publication { id } publishDate isPublished } }\n      publications(first: 10) { nodes { isPublished publishDate product { id } } }\n    }\n  }\n}";
const PRODUCT_VARIANTS_BULK_CREATE_INVENTORY_QUANTITIES_LIMIT: usize = 50_000;
const PRODUCT_VARIANTS_BULK_CREATE_DEFAULT_LOCATION_LIMIT: usize = 200;

fn normalized_sort_string(value: &str) -> StagedSortValue {
    StagedSortValue::String(value.to_ascii_lowercase())
}

fn gid_tail_sort_key(id: &str) -> StagedSortKey {
    let tail = resource_id_tail(id);
    match tail.parse::<i64>() {
        Ok(value) => vec![StagedSortValue::I64(1), StagedSortValue::I64(value)],
        Err(_) => vec![
            StagedSortValue::I64(0),
            StagedSortValue::String(tail.to_ascii_lowercase()),
        ],
    }
}

pub(in crate::proxy) fn product_staged_sort_key(
    product: &ProductRecord,
    sort_key: Option<&str>,
) -> StagedSortKey {
    let mut primary = match sort_key {
        None | Some("CREATED_AT") => vec![StagedSortValue::String(product.created_at.clone())],
        Some("TITLE") => vec![normalized_sort_string(&product.title)],
        Some("VENDOR") => vec![normalized_sort_string(&product.vendor)],
        Some("PRODUCT_TYPE") => vec![normalized_sort_string(&product.product_type)],
        Some("PUBLISHED_AT") => vec![product
            .extra_fields
            .get("publishedAt")
            .and_then(Value::as_str)
            .map(|value| StagedSortValue::String(value.to_string()))
            .unwrap_or(StagedSortValue::Null)],
        Some("UPDATED_AT") => vec![StagedSortValue::String(product.updated_at.clone())],
        Some("INVENTORY_TOTAL") => vec![StagedSortValue::I64(product.total_inventory)],
        Some("ID") => gid_tail_sort_key(&product.id),
        // Shopify relevance is a search score. The local staged search adapter
        // only computes match/no-match, so keep a deterministic created-at order
        // instead of letting RELEVANCE disappear into the unknown-key branch.
        Some("RELEVANCE") => vec![StagedSortValue::String(product.created_at.clone())],
        Some(_) => gid_tail_sort_key(&product.id),
    };
    primary.extend(gid_tail_sort_key(&product.id));
    primary
}

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

    pub(in crate::proxy) fn product_record_by_id(&self, id: &str) -> Option<&ProductRecord> {
        self.store.product_by_id(id)
    }

    pub(in crate::proxy) fn products_filtered_by_search_query(
        &self,
        query: Option<&ResolvedValue>,
    ) -> Vec<ProductRecord> {
        let mut products = self.store.products();
        let Some(ResolvedValue::String(query)) = query else {
            return products;
        };
        products.retain(|product| {
            self.product_search_decision(product, Some(query)) == StagedSearchDecision::Match
        });
        products
    }

    pub(in crate::proxy) fn product_search_decision(
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

    pub(crate) fn product_create_outcome(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let request = invocation.request;
        let query = invocation.query;
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        if let Some(errors) = product_create_status_validation_errors(
            request,
            query,
            invocation.root_name,
            invocation.root_location,
            &invocation.raw_arguments,
        ) {
            return graphql_error_outcome(errors, invocation.response_key);
        }
        let Some(input) = product_input(&arguments) else {
            return ResolverOutcome::value(product_create_payload_value(
                None,
                vec![user_error(
                    ["product"],
                    "Product input is required",
                    Some("REQUIRED"),
                )],
            ));
        };
        if input.contains_key("variants") {
            return graphql_error_outcome(
                vec![invalid_variable_error_envelope(
                    "Variable $input of type ProductInput! was provided invalid value for variants (Field is not defined on ProductInput)".to_string(),
                    SourceLocation { line: 2, column: 39 },
                    resolved_value_json(&ResolvedValue::Object(input.clone())),
                    json!([{ "path": ["variants"], "explanation": "Field is not defined on ProductInput" }]),
                )],
                invocation.response_key,
            );
        }

        if input.contains_key("id") {
            return ResolverOutcome::value(product_create_payload_value(
                None,
                vec![user_error_omit_code(
                    ["input"],
                    "id cannot be specified during creation",
                    None,
                )],
            ));
        }

        let Some(title) =
            resolved_string_field(&input, "title").filter(|value| !value.trim().is_empty())
        else {
            return ResolverOutcome::value(product_create_payload_value(
                None,
                vec![presence_user_error(["title"], "Title")],
            ));
        };

        let length_errors = product_scalar_length_user_errors(
            &input,
            ProductScalarLengthValidationShape::ProductInput,
        );
        if !length_errors.is_empty() {
            return ResolverOutcome::value(product_create_payload_value(None, length_errors));
        }
        if resolved_object_list_field(&input, "productOptions")
            .iter()
            .filter_map(|option| resolved_string_field(option, "name"))
            .any(|name| product_option_name_has_title_delimiter(&name))
        {
            return ResolverOutcome::value(product_create_payload_value(
                None,
                vec![user_error_omit_code(
                    ["options"],
                    PRODUCT_CREATE_OPTION_NAME_DELIMITER_MESSAGE,
                    None,
                )],
            ));
        }

        let top_level_media_inputs = product_top_level_media_inputs(&arguments);
        if let Some(media_inputs) = top_level_media_inputs.as_ref() {
            let media_errors = product_top_level_media_user_errors(media_inputs);
            if !media_errors.is_empty() {
                return ResolverOutcome::value(product_create_payload_value(None, media_errors));
            }
        }

        let explicit_handle = resolved_string_field(&input, "handle");
        let handle = match self.resolve_product_handle(&title, explicit_handle.as_deref(), None) {
            Ok(handle) => handle,
            Err(error) => {
                return ResolverOutcome::value(product_create_payload_value(None, vec![error]));
            }
        };
        let category = match self.product_category_for_mutation_input(
            request,
            &input,
            invocation.response_key,
            invocation.root_location,
        ) {
            Ok(category) => category,
            Err(outcome) => return outcome,
        };

        let id = self.next_proxy_synthetic_gid("Product");
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
        let media_append = top_level_media_inputs
            .as_ref()
            .map(|media_inputs| self.product_top_level_media_append(media_inputs))
            .unwrap_or_default();
        product.media = media_append.staged_nodes.clone();
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
        // Shopify resolves the input taxonomy GID before product identity or related
        // resources are allocated. Only authoritative category metadata reaches state.
        if let Some(category) = category {
            product
                .extra_fields
                .insert("category".to_string(), category);
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
        staged_ids.extend(media_append.staged_ids.iter().cloned());
        self.store.stage_product_variant(variant);

        // `collectionsToJoin` adds the new product to existing collections. Add the minimal
        // collection refs to the product surface before staging so the mutation response
        // renders them.
        let collections_to_join = list_string_field(&input, "collectionsToJoin");
        for collection_id in &collections_to_join {
            if let Some(collection) = self.store.collection_by_id(collection_id).cloned() {
                upsert_minimal_collection(&mut product.collections, &collection);
            }
        }

        // Stage any `metafields` supplied on create so downstream metafield reads resolve them.
        self.stage_owner_metafields_from_input(&id, &input);

        self.store.stage_product(product.clone());
        let mut response_product = product.clone();
        response_product.media = media_append.mutation_nodes;

        // Register collection membership so downstream `collection` reads expose hasProduct,
        // productsCount, and the product in their member list.
        for collection_id in &collections_to_join {
            self.add_product_to_collection_membership(collection_id, &product);
        }

        let mut product_value = self.product_canonical_value(&response_product);
        product_value["media"] = connection_json(response_product.media.clone());
        ResolverOutcome::value(product_create_payload_value(
            Some(product_value),
            Vec::new(),
        ))
        .with_log_draft(LogDraft::staged("productCreate", "products", staged_ids))
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
        let product_options = resolved_object_list_field(input, "productOptions");
        if product_options.is_empty() {
            return None;
        }
        let mut options = Vec::new();
        let mut selected_options = Vec::new();
        for (index, option) in product_options.iter().enumerate() {
            let name = resolved_string_field(option, "name").unwrap_or_default();
            let value_names: Vec<String> = resolved_object_list_field(option, "values")
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
                tracked: false,
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

    pub(crate) fn product_update_outcome(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let request = invocation.request;
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        let Some(input) = product_input(&arguments) else {
            return ResolverOutcome::value(product_update_payload_value(
                None,
                vec![user_error(
                    ["product"],
                    "Product input is required",
                    Some("REQUIRED"),
                )],
            ));
        };
        let incoming_tags = if input.contains_key("tags") {
            Some(list_string_field(&input, "tags"))
        } else {
            None
        };
        if let Some(tags) = incoming_tags.as_ref() {
            if tags.len() > 250 {
                return graphql_error_outcome(
                    vec![max_input_size_exceeded_error(
                        ["productUpdate", "product", "tags"],
                        tags.len(),
                        250,
                        Some(json!([{"line": 3, "column": 5}])),
                    )],
                    invocation.response_key,
                );
            }
        }
        let Some(id) = resolved_string_field(&input, "id") else {
            return ResolverOutcome::value(product_update_missing_payload_value());
        };
        if self.store.product_by_id(&id).is_none() && self.config.read_mode == ReadMode::LiveHybrid
        {
            self.hydrate_product_nodes_for_observation_with_request(request, vec![id.clone()]);
        }
        let Some(existing) = self.store.product_staged_or_base(&id) else {
            return ResolverOutcome::value(product_update_missing_payload_value());
        };

        if input.contains_key("title")
            && resolved_string_field(&input, "title")
                .unwrap_or_default()
                .trim()
                .is_empty()
        {
            return self.product_update_field_user_error(
                &existing,
                presence_user_error(["title"], "Title"),
            );
        }

        let length_errors = product_scalar_length_user_errors(
            &input,
            ProductScalarLengthValidationShape::ProductInput,
        );
        if !length_errors.is_empty() {
            return self.product_update_field_user_errors(&existing, length_errors);
        }

        if let Some(tags) = incoming_tags.as_ref() {
            if tags.iter().any(|tag| tag.chars().count() > 255) {
                return self.product_update_field_user_error(
                    &existing,
                    user_error_omit_code(["tags"], "Product tags is invalid", None),
                );
            }
        }

        let top_level_media_inputs = product_top_level_media_inputs(&arguments);
        if let Some(media_inputs) = top_level_media_inputs.as_ref() {
            let media_errors = product_top_level_media_user_errors(media_inputs);
            if !media_errors.is_empty() {
                return self.product_update_field_user_errors(&existing, media_errors);
            }
        }

        let title =
            resolved_string_field(&input, "title").unwrap_or_else(|| existing.title.clone());
        let explicit_handle = resolved_string_field(&input, "handle");
        let handle = match self.resolve_product_handle(
            &title,
            explicit_handle.as_deref(),
            Some(&existing),
        ) {
            Ok(handle) => handle,
            Err(error) => return self.product_update_field_user_error(&existing, error),
        };

        let category = match self.product_category_for_mutation_input(
            request,
            &input,
            invocation.response_key,
            invocation.root_location,
        ) {
            Ok(category) => category,
            Err(outcome) => return outcome,
        };

        let mut extra_fields = existing.extra_fields;
        if let Some(category) = category {
            extra_fields.insert("category".to_string(), category);
        }

        let media_append = top_level_media_inputs
            .as_ref()
            .map(|media_inputs| self.product_top_level_media_append(media_inputs))
            .unwrap_or_default();
        let mut staged_media = existing.media.clone();
        staged_media.extend(media_append.staged_nodes.clone());
        let mut response_media = existing.media.clone();
        response_media.extend(media_append.mutation_nodes);

        let product = ProductRecord {
            id: existing.id,
            created_at: existing.created_at,
            updated_at: self.next_product_updated_at(&existing.updated_at),
            title,
            handle,
            status: resolved_string_field(&input, "status").unwrap_or(existing.status),
            description_html: resolved_string_field(&input, "descriptionHtml")
                .unwrap_or(existing.description_html),
            vendor: resolved_string_field(&input, "vendor").unwrap_or(existing.vendor),
            product_type: resolved_string_field(&input, "productType")
                .unwrap_or(existing.product_type),
            tags: if input.contains_key("tags") {
                normalize_taggable_tags(incoming_tags.unwrap_or_default())
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
            media: staged_media,
            variants: existing.variants,
            collections: existing.collections,
            extra_fields,
        };
        self.store.stage_product(product.clone());
        let mut response_product = product.clone();
        response_product.media = response_media;

        let mut staged_ids = vec![id];
        staged_ids.extend(media_append.staged_ids);
        let mut response_product_value = self.product_canonical_value(&response_product);
        response_product_value["media"] = connection_json(response_product.media.clone());
        ResolverOutcome::value(product_update_payload_value(
            Some(response_product_value),
            Vec::new(),
        ))
        .with_log_draft(LogDraft::staged("productUpdate", "products", staged_ids))
    }

    /// Build a productUpdate response that returns the (unchanged) product alongside a single
    /// field-scoped userError — the shape Shopify emits when an input value is rejected
    /// (e.g. blank title, over-long handle) without persisting the mutation.
    fn product_update_field_user_error(
        &self,
        existing: &ProductRecord,
        user_error: Value,
    ) -> ResolverOutcome<Value> {
        self.product_update_field_user_errors(existing, vec![user_error])
    }

    fn product_update_field_user_errors(
        &self,
        existing: &ProductRecord,
        user_errors: Vec<Value>,
    ) -> ResolverOutcome<Value> {
        ResolverOutcome::value(product_update_payload_value(
            Some(self.product_canonical_value(existing)),
            user_errors,
        ))
    }

    pub(crate) fn product_variant_outcome(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let request = invocation.request;
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        match invocation.root_name {
            "productVariantAppendMedia" | "productVariantDetachMedia" => {
                self.product_variant_media_outcome(request, invocation.root_name, &arguments)
            }
            "productVariantsBulkCreate" => {
                self.product_variants_bulk_create(request, &invocation, &arguments)
            }
            "productVariantsBulkUpdate" => self.product_variants_bulk_update(request, &arguments),
            "productVariantsBulkDelete" => self.product_variants_bulk_delete(request, &arguments),
            "productVariantsBulkReorder" => self.product_variants_bulk_reorder(request, &arguments),
            root => ResolverOutcome::error(format!(
                "No mutation dispatcher implemented for product variant root `{root}`"
            )),
        }
    }

    fn product_variant_media_outcome(
        &mut self,
        request: &Request,
        root_field: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> ResolverOutcome<Value> {
        let product_id = resolved_string_field(arguments, "productId").unwrap_or_default();
        let variant_media = resolved_object_list_field(arguments, "variantMedia");
        self.hydrate_product_variant_media_owner_state(request, &product_id, &variant_media);
        let user_errors =
            self.product_variant_media_user_errors(root_field, &product_id, &variant_media);

        if !user_errors.is_empty() {
            let payload =
                self.product_variant_media_payload_value(&product_id, Vec::new(), user_errors);
            return ResolverOutcome::value(payload);
        }

        let mut changed_variant_ids = Vec::new();
        for item in &variant_media {
            let Some(variant_id) = resolved_string_field(item, "variantId") else {
                continue;
            };
            let media_ids = list_string_field(item, "mediaIds");
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

        let payload = self.product_variant_media_payload_value(
            &product_id,
            changed_variant_ids.clone(),
            Vec::new(),
        );
        ResolverOutcome::value(payload).with_log_draft(LogDraft::staged(
            root_field,
            "products",
            std::iter::once(product_id)
                .chain(changed_variant_ids)
                .collect(),
        ))
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
            let media_ids = list_string_field(item, "mediaIds");
            if media_ids.len() > 1 {
                user_errors.push(product_variant_media_user_error(
                    &["variantMedia", &entry_index.to_string(), "mediaIds"],
                    "Only one mediaId is allowed per media input.",
                    "TOO_MANY_MEDIA_PER_INPUT_PAIR",
                ));
            } else if media_ids.is_empty() {
                user_errors.push(product_variant_media_user_error(
                    &["variantMedia", &entry_index.to_string(), "mediaIds"],
                    "The mediaIds list cannot be empty.",
                    "BLANK",
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
            let media_ids = list_string_field(item, "mediaIds");
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

    fn product_variant_media_payload_value(
        &self,
        product_id: &str,
        variant_ids: Vec<String>,
        user_errors: Vec<Value>,
    ) -> Value {
        let success = user_errors.is_empty();
        json!({
            "product": if success {
                self.store
                    .product_by_id(product_id)
                    .map(|product| self.product_canonical_value(product))
                    .unwrap_or(Value::Null)
            } else {
                Value::Null
            },
            "productVariants": if success {
                Value::Array(
                    variant_ids
                        .iter()
                        .filter_map(|variant_id| self.store.product_variant_by_id(variant_id))
                        .map(|variant| self.product_variant_canonical_value(variant))
                        .collect(),
                )
            } else {
                Value::Null
            },
            "userErrors": user_errors,
        })
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
    pub(in crate::proxy) fn sync_product_inventory_aggregates(&mut self, product_id: &str) {
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

    pub(in crate::proxy) fn sync_product_tracks_inventory(&mut self, product_id: &str) {
        let final_variants = self.store.product_variants_for_product(product_id);
        let Some(mut product) = self.store.product_by_id(product_id).cloned() else {
            return;
        };
        if final_variants.is_empty() {
            return;
        }
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
        invocation: &RootInvocation<'_>,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> ResolverOutcome<Value> {
        let product_id = resolved_string_field(arguments, "productId").unwrap_or_default();
        let variants_input = resolved_object_list_field(arguments, "variants");
        if variants_input.len() > 2048 {
            return graphql_error_outcome(
                vec![Self::product_variant_bulk_input_size_error(
                    invocation.root_name,
                    invocation.root_location,
                    variants_input.len(),
                )],
                invocation.response_key,
            );
        }
        let Some(product) = self
            .product_for_bulk_variant_mutation_with_variant_ids(request, &product_id, &[])
            .cloned()
        else {
            return ResolverOutcome::value(self.product_variants_bulk_payload_value(
                "productVariantsBulkCreate",
                None,
                Some(Vec::new()),
                vec![user_error(
                    ["productId"],
                    "Product does not exist",
                    Some("PRODUCT_DOES_NOT_EXIST"),
                )],
            ));
        };
        if let Some(error) =
            Self::product_variant_bulk_inventory_quantities_limit_user_error(&variants_input)
        {
            return ResolverOutcome::value(self.product_variants_bulk_payload_value(
                "productVariantsBulkCreate",
                None,
                Some(Vec::new()),
                vec![error],
            ));
        }

        let mut user_errors = Vec::new();
        for (index, input) in variants_input.iter().enumerate() {
            let error_count_before_variant = user_errors.len();
            user_errors.extend(product_variant_input_user_errors_with_prefix(
                input,
                &["variants".to_string(), index.to_string()],
            ));
            user_errors.extend(Self::product_variant_bulk_option_user_errors(
                input, &product, index, false,
            ));
            if user_errors.len() == error_count_before_variant {
                user_errors
                    .extend(self.product_variant_bulk_inventory_location_user_errors(input, index));
            }
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
            user_errors.push(user_error(
                ["variants"],
                "Product cannot have more than 2048 variants",
                Some("TOO_MANY_VARIANTS"),
            ));
        }
        if !user_errors.is_empty() {
            return ResolverOutcome::value(self.product_variants_bulk_payload_value(
                "productVariantsBulkCreate",
                None,
                Some(Vec::new()),
                user_errors,
            ));
        }

        let strategy =
            resolved_string_field(arguments, "strategy").unwrap_or_else(|| "DEFAULT".into());
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

        // Apply the bulk-create variant strategy. Shopify defaults omitted/null strategy
        // to `DEFAULT`, which only drops the lone pre-existing variant when it is the
        // auto-generated `Title: Default Title` standalone variant. `REMOVE_STANDALONE_VARIANT`
        // drops any lone pre-existing variant. The option set is rederived from the surviving
        // variants after strategy handling.
        let remove_existing = match strategy.as_str() {
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

        self.sync_product_inventory_aggregates(&product.id);

        let mut staged_ids = vec![product.id.clone()];
        staged_ids.extend(created_variants.iter().map(|variant| variant.id.clone()));
        ResolverOutcome::value(self.product_variants_bulk_payload_value(
            "productVariantsBulkCreate",
            self.store.product_by_id(&product.id),
            Some(created_variants),
            Vec::new(),
        ))
        .with_log_draft(LogDraft::staged(
            "productVariantsBulkCreate",
            "products",
            staged_ids,
        ))
    }

    fn product_variants_bulk_update(
        &mut self,
        request: &Request,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> ResolverOutcome<Value> {
        let product_id = resolved_string_field(arguments, "productId").unwrap_or_default();
        let allow_partial_updates =
            resolved_bool_field(arguments, "allowPartialUpdates").unwrap_or(false);
        let variants_input = resolved_object_list_field(arguments, "variants");
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
            return ResolverOutcome::value(self.product_variants_bulk_payload_value(
                "productVariantsBulkUpdate",
                None,
                None,
                vec![user_error(
                    ["productId"],
                    "Product does not exist",
                    Some("PRODUCT_DOES_NOT_EXIST"),
                )],
            ));
        };
        if variants_input.is_empty() {
            return ResolverOutcome::value(self.product_variants_bulk_payload_value(
                "productVariantsBulkUpdate",
                Some(&product),
                Some(Vec::new()),
                Vec::new(),
            ));
        }

        let mut user_errors = Vec::new();
        let mut updated_variants = Vec::new();
        let mut updated_variant_input_indexes = Vec::new();
        let mut response_variant_ids = Vec::new();
        let mut position_moves = Vec::new();
        let mut has_blocking_errors = false;
        for (index, input) in variants_input.iter().enumerate() {
            let prefix = ["variants".to_string(), index.to_string()];
            let Some(variant_id) = resolved_string_field(input, "id") else {
                has_blocking_errors = true;
                user_errors.push(user_error(
                    ["variants", &index.to_string(), "id"],
                    "Product variant is missing ID attribute",
                    Some("PRODUCT_VARIANT_ID_MISSING"),
                ));
                continue;
            };
            let Some(existing) = self.store.product_variant_by_id(&variant_id).cloned() else {
                has_blocking_errors = true;
                user_errors.push(user_error(
                    ["variants", &index.to_string(), "id"],
                    "Product variant does not exist",
                    Some("PRODUCT_VARIANT_DOES_NOT_EXIST"),
                ));
                continue;
            };
            if existing.product_id != product.id {
                has_blocking_errors = true;
                user_errors.push(user_error(
                    ["variants", &index.to_string(), "id"],
                    "Product variant does not exist",
                    Some("PRODUCT_VARIANT_DOES_NOT_EXIST"),
                ));
                continue;
            }
            let mut input_errors = Vec::new();
            let mut input_has_blocking_errors = false;
            if input.contains_key("inventoryQuantities") {
                input_has_blocking_errors = true;
                input_errors.push(user_error(
                    ["variants", &index.to_string(), "inventoryQuantities"],
                    "Inventory quantities can only be provided during create. To update inventory for existing variants, use inventoryAdjustQuantities.",
                    Some("NO_INVENTORY_QUANTITIES_ON_VARIANTS_UPDATE"),
                ));
            }
            input_errors.extend(product_variant_input_user_errors_with_prefix(
                input, &prefix,
            ));
            let option_errors =
                Self::product_variant_bulk_option_user_errors(input, &product, index, true);
            if !option_errors.is_empty() {
                input_has_blocking_errors = true;
            }
            input_errors.extend(option_errors);
            if input_has_blocking_errors {
                has_blocking_errors = true;
            } else {
                response_variant_ids.push(variant_id.clone());
            }
            if !input_errors.is_empty() {
                user_errors.extend(input_errors);
                continue;
            }
            let mut variant = existing;
            apply_product_variant_input(&mut variant, input);
            Self::normalize_bulk_variant_title(&mut variant);
            if let Some(position) = resolved_int_field(input, "position") {
                position_moves.push((variant.id.clone(), position, index));
            }
            updated_variants.push(variant);
            updated_variant_input_indexes.push(index);
        }
        Self::sort_user_errors_by_field_and_code(&mut user_errors);
        if !user_errors.is_empty() && (!allow_partial_updates || updated_variants.is_empty()) {
            let response_variants = if has_blocking_errors {
                None
            } else {
                Some(
                    response_variant_ids
                        .iter()
                        .filter_map(|id| self.store.product_variant_by_id(id).cloned())
                        .collect(),
                )
            };
            return ResolverOutcome::value(self.product_variants_bulk_payload_value(
                "productVariantsBulkUpdate",
                Some(&product),
                response_variants,
                user_errors,
            ));
        }

        for variant in &updated_variants {
            self.store.stage_product_variant(variant.clone());
        }
        for (variant, input_index) in updated_variants
            .iter()
            .zip(updated_variant_input_indexes.iter())
        {
            self.stage_input_variant_metafields(&variant.id, &variants_input[*input_index]);
        }
        if !position_moves.is_empty() {
            self.store
                .move_product_variants_to_positions(&product.id, &position_moves);
            updated_variants = updated_variants
                .iter()
                .filter_map(|variant| self.store.product_variant_by_id(&variant.id).cloned())
                .collect();
        }
        self.sync_product_tracks_inventory(&product.id);
        let mut staged_ids = vec![product.id.clone()];
        staged_ids.extend(updated_variants.iter().map(|variant| variant.id.clone()));
        let response_variants = if has_blocking_errors {
            updated_variants.clone()
        } else {
            response_variant_ids
                .iter()
                .filter_map(|id| self.store.product_variant_by_id(id).cloned())
                .collect()
        };
        ResolverOutcome::value(self.product_variants_bulk_payload_value(
            "productVariantsBulkUpdate",
            self.store.product_by_id(&product.id),
            Some(response_variants),
            user_errors,
        ))
        .with_log_draft(LogDraft::staged(
            "productVariantsBulkUpdate",
            "products",
            staged_ids,
        ))
    }

    fn product_variants_bulk_delete(
        &mut self,
        request: &Request,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> ResolverOutcome<Value> {
        let product_id = resolved_string_field(arguments, "productId").unwrap_or_default();
        let variant_ids = resolved_string_list_arg(arguments, "variantsIds");
        // Hydrate the product together with the variants being deleted so a cold
        // backend stages both before applying the delete, matching the node
        // hydration recorded during capture.
        let Some(product) = self
            .product_for_bulk_variant_mutation_with_variant_ids(request, &product_id, &variant_ids)
            .cloned()
        else {
            return ResolverOutcome::value(self.product_variants_bulk_payload_value(
                "productVariantsBulkDelete",
                None,
                None,
                vec![user_error(
                    ["productId"],
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
                user_errors.push(user_error(
                    ["variantsIds", &index.to_string()],
                    "At least one variant does not belong to the product",
                    Some("AT_LEAST_ONE_VARIANT_DOES_NOT_BELONG_TO_THE_PRODUCT"),
                ));
            }
        }
        if !user_errors.is_empty() {
            return ResolverOutcome::value(self.product_variants_bulk_payload_value(
                "productVariantsBulkDelete",
                None,
                None,
                user_errors,
            ));
        }

        for variant_id in &variant_ids {
            self.store.delete_product_variant(variant_id);
        }
        ResolverOutcome::value(self.product_variants_bulk_payload_value(
            "productVariantsBulkDelete",
            self.store.product_by_id(&product.id),
            None,
            Vec::new(),
        ))
        .with_log_draft(LogDraft::staged(
            "productVariantsBulkDelete",
            "products",
            std::iter::once(product.id.clone())
                .chain(variant_ids.iter().cloned())
                .collect(),
        ))
    }

    fn product_variants_bulk_reorder(
        &mut self,
        request: &Request,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> ResolverOutcome<Value> {
        let product_id = resolved_string_field(arguments, "productId").unwrap_or_default();
        let positions = resolved_object_list_field(arguments, "positions");
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
            return ResolverOutcome::value(self.product_variants_bulk_payload_value(
                "productVariantsBulkReorder",
                None,
                None,
                vec![user_error(
                    ["productId"],
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
                user_errors.push(user_error(
                    ["positions", &index.to_string(), "id"],
                    "Product variant is missing ID attribute",
                    Some("MISSING_VARIANT"),
                ));
                continue;
            };
            let position_value =
                resolved_int_field(position, "position").unwrap_or((index + 1) as i64);
            if position_value < 1 {
                user_errors.push(user_error(
                    ["positions", &index.to_string(), "position"],
                    "Position can not be zero or negative number",
                    Some("INVALID_POSITION"),
                ));
            }
            if self
                .store
                .product_variant_by_id(&variant_id)
                .is_none_or(|variant| variant.product_id != product.id)
            {
                user_errors.push(user_error(
                    ["positions", &index.to_string(), "id"],
                    "Product variant does not exist",
                    Some("MISSING_VARIANT"),
                ));
                continue;
            }
            if !seen_variant_ids.insert(variant_id.clone()) {
                user_errors.push(user_error(
                    ["positions"],
                    "Product variant IDs must be unique",
                    Some("DUPLICATED_VARIANT_ID"),
                ));
                continue;
            }
            ordered_positions.push((position_value, index, variant_id));
        }
        if !user_errors.is_empty() {
            return ResolverOutcome::value(self.product_variants_bulk_payload_value(
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
        ResolverOutcome::value(self.product_variants_bulk_payload_value(
            "productVariantsBulkReorder",
            self.store.product_by_id(&product.id),
            None,
            Vec::new(),
        ))
        .with_log_draft(LogDraft::staged(
            "productVariantsBulkReorder",
            "products",
            std::iter::once(product.id.clone())
                .chain(ordered_ids)
                .collect(),
        ))
    }

    fn product_variants_bulk_payload_value(
        &self,
        root_field: &str,
        product: Option<&ProductRecord>,
        variants: Option<Vec<ProductVariantRecord>>,
        user_errors: Vec<Value>,
    ) -> Value {
        json!({
            "product": product
                .map(|product| self.product_canonical_value(product))
                .unwrap_or(Value::Null),
            "productVariants": match variants {
                Some(variants) => Value::Array(
                    variants
                        .iter()
                        .map(|variant| self.product_variant_canonical_value(variant))
                        .collect(),
                ),
                None if root_field == "productVariantsBulkCreate" => Value::Array(Vec::new()),
                None => Value::Null,
            },
            "userErrors": user_errors,
        })
    }

    fn sort_user_errors_by_field_and_code(user_errors: &mut [Value]) {
        user_errors.sort_by_key(|error| {
            (
                Self::user_error_field_sort_key(error),
                Self::user_error_code_sort_key(error),
            )
        });
    }

    fn user_error_field_sort_key(error: &Value) -> Vec<String> {
        match error.get("field") {
            Some(Value::Array(field)) => field
                .iter()
                .map(|segment| {
                    segment
                        .as_str()
                        .map(str::to_string)
                        .unwrap_or_else(|| segment.to_string())
                })
                .collect(),
            Some(Value::String(field)) => vec![field.clone()],
            _ => Vec::new(),
        }
    }

    fn user_error_code_sort_key(error: &Value) -> String {
        match error.get("code") {
            Some(Value::String(code)) => code.clone(),
            Some(Value::Null) | None => String::new(),
            Some(code) => code.to_string(),
        }
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

    fn product_variant_bulk_input_size_error(
        root_name: &str,
        root_location: SourceLocation,
        size: usize,
    ) -> Value {
        max_input_size_exceeded_error(
            [root_name, "variants"],
            size,
            2048,
            Some(json!([{
                "line": root_location.line,
                "column": root_location.column
            }])),
        )
    }

    fn product_variant_bulk_inventory_quantities_limit_user_error(
        variants_input: &[BTreeMap<String, ResolvedValue>],
    ) -> Option<Value> {
        let quantity_count: usize = variants_input
            .iter()
            .map(|input| resolved_object_list_field(input, "inventoryQuantities").len())
            .sum();
        if quantity_count > PRODUCT_VARIANTS_BULK_CREATE_INVENTORY_QUANTITIES_LIMIT {
            Some(user_error(
                ["variants"],
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
                errors.push(user_error(
                    [
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
                errors.push(user_error(
                    [
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
                errors.push(user_error(
                    ["variants", &index.to_string(), "optionValues"],
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
                errors.push(user_error(
                    [
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
                    errors.push(user_error(
                        ["variants", &index.to_string()],
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
                return vec![user_error(
                    ["variants", &index.to_string()],
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
            return vec![user_error(
                ["variants", &index.to_string()],
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
            vec![user_error(
                ["variants", &index.to_string(), "inventoryQuantities"],
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
        self.location_for_read(location_id).is_some()
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
        if !variant.selected_options.is_empty() {
            variant.title = variant
                .selected_options
                .iter()
                .map(|option| option.value.as_str())
                .collect::<Vec<_>>()
                .join(" / ");
        }
    }

    pub(crate) fn product_delete_outcome(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let request = invocation.request;
        let query = invocation.query;
        let variables = invocation.variables;
        if let Some(errors) = product_delete_required_id_errors(query, variables) {
            return graphql_error_outcome(errors, invocation.response_key);
        }
        let root_arguments = resolved_arguments_from_json(&invocation.arguments);
        let Some(input) = product_input(&root_arguments) else {
            return ResolverOutcome::value(product_delete_missing_payload_value());
        };
        let Some(id) = resolved_string_field(&input, "id") else {
            return ResolverOutcome::value(product_delete_missing_payload_value());
        };
        let is_async_delete = resolved_bool_field(&root_arguments, "synchronous") == Some(false);
        if is_async_delete
            && self
                .store
                .staged
                .product_delete_operations
                .values()
                .any(|pending_id| pending_id == &id)
        {
            return ResolverOutcome::value(product_delete_async_duplicate_payload_value());
        }
        if !self.store.has_product(&id) && self.config.read_mode == ReadMode::LiveHybrid {
            self.hydrate_product_nodes_for_observation_with_request(request, vec![id.clone()]);
        }
        if !self.store.has_product(&id) {
            return ResolverOutcome::value(product_delete_missing_payload_value());
        }

        if is_async_delete {
            let operation_id = self.next_synthetic_gid("ProductDeleteOperation");
            self.store
                .staged
                .product_delete_operations
                .insert(operation_id.clone(), id.clone());
            self.store.delete_product(&id);
            return ResolverOutcome::value(product_delete_async_operation_payload_value(
                &operation_id,
            ))
            .with_log_draft(LogDraft::staged(
                "productDelete",
                "products",
                vec![id.clone()],
            ));
        }

        self.store.delete_product(&id);

        ResolverOutcome::value(product_delete_payload_value(&id)).with_log_draft(LogDraft::staged(
            "productDelete",
            "products",
            vec![id.clone()],
        ))
    }

    pub(crate) fn product_change_status_outcome(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let request = invocation.request;
        let query = invocation.query;
        if invocation
            .raw_arguments
            .get("productId")
            .is_some_and(|argument| matches!(argument.resolved_value(), ResolvedValue::Null))
        {
            return graphql_error_outcome(
                vec![argument_literals_incompatible_error_envelope(
                    "Argument 'productId' on Field 'productChangeStatus' has an invalid value (null). Expected type 'ID!'.".to_string(),
                    Some(invocation.root_location),
                    Some(json!([
                        invocation.operation_path,
                        invocation.response_key,
                        "productId"
                    ])),
                    Some("Field"),
                    Some("productId"),
                )],
                invocation.response_key,
            );
        }
        let Some(id) = invocation
            .arguments
            .get("productId")
            .and_then(Value::as_str)
        else {
            return ResolverOutcome::error("productChangeStatus requires productId");
        };
        if let Some(errors) = product_status_argument_validation_errors(
            request,
            query,
            invocation.root_name,
            invocation.root_location,
            &invocation.raw_arguments,
            ProductStatusArgumentContext {
                argument_name: "status",
                container_type_name: "Field",
                container_name: "productChangeStatus",
                expected_type: "ProductStatus!",
            },
        ) {
            return graphql_error_outcome(errors, invocation.response_key);
        }
        let Some(status) = invocation.arguments.get("status").and_then(Value::as_str) else {
            return ResolverOutcome::error("productChangeStatus requires status");
        };
        let Some(mut product) = self
            .store
            .product_staged_or_base(id)
            .or_else(|| self.hydrate_product_for_tags(id, request))
        else {
            return ResolverOutcome::value(product_change_status_payload_value(
                None,
                vec![user_error_omit_code(
                    ["productId"],
                    "Product does not exist",
                    Some("PRODUCT_NOT_FOUND"),
                )],
            ));
        };
        product.status = status.to_string();
        product.updated_at = self.next_product_updated_at(&product.updated_at);
        self.store.stage_product(product.clone());

        ResolverOutcome::value(product_change_status_payload_value(
            Some(self.product_canonical_value(&product)),
            Vec::new(),
        ))
        .with_log_draft(LogDraft::staged(
            "productChangeStatus",
            "products",
            vec![id.to_string()],
        ))
    }

    pub(crate) fn product_tags_outcome(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let root_field = invocation.root_name;
        let request = invocation.request;
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        let Some(id) = invocation.arguments.get("id").and_then(Value::as_str) else {
            return ResolverOutcome::error("tags mutation requires id");
        };
        let Some(resource_type) = shopify_gid_resource_type(id) else {
            return ResolverOutcome::error("tags mutation requires a Shopify GID");
        };
        let tags = normalized_taggable_tags_argument(arguments.get("tags"));
        if resource_type != "Product" {
            if matches!(
                resource_type,
                "Order" | "Customer" | "Article" | "DraftOrder"
            ) {
                return self.taggable_resource_tags_outcome(
                    resource_type,
                    id,
                    root_field,
                    tags,
                    request,
                );
            }
            return ResolverOutcome::error(format!(
                "Local tag mutation support is not implemented for {resource_type}"
            ));
        }

        let Some(mut product) = self
            .store
            .product_staged_or_base(id)
            .or_else(|| self.hydrate_product_for_tags(id, request))
        else {
            return tags_not_found_resolver_outcome("Product", id, root_field);
        };

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

        ResolverOutcome::value(tags_mutation_payload_value(Some(
            self.product_canonical_value(&product),
        )))
        .with_log_draft(LogDraft::staged(
            root_field,
            "products",
            vec![id.to_string()],
        ))
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

    fn taggable_resource_tags_outcome(
        &mut self,
        resource_type: &str,
        id: &str,
        root_field: &str,
        incoming_tags: Vec<String>,
        request: &Request,
    ) -> ResolverOutcome<Value> {
        let Some(mut record) =
            self.taggable_resource_staged_or_hydrated(resource_type, id, request)
        else {
            return tags_not_found_resolver_outcome(resource_type, id, root_field);
        };

        let existing_tags = taggable_record_tags(&record);
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

        ResolverOutcome::value(tags_mutation_payload_value(Some(record))).with_log_draft(
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
        // Whether the staging request carried the app-client-id header. Its consumer
        // (agent-server) only sends that header for a service-app caller, so this flag
        // records "staged by a service-app caller" — the signal `commit_staged_mutations`
        // replays under to pick the app's own credential rather than the staff user's.
        let service_app = request_app_namespace_api_client_id(request).is_some();
        self.log_entries.push(json!({
            "id": id,
            "operationName": null,
            "path": request.path,
            "query": query,
            "variables": resolved_variables_json(variables),
            "rawBody": request.body,
            "stagedResourceIds": staged_resource_ids,
            "status": "staged",
            "serviceApp": service_app,
            "interpreted": {
                "operationType": "mutation",
                "rootFields": root_fields,
                "primaryRootField": root_field
            }
        }));
    }

    pub(crate) fn product_publication_outcome(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let root_field = invocation.root_name;
        let request = invocation.request;
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        let input = match arguments.get("input") {
            Some(ResolvedValue::Object(input)) => input.clone(),
            _ => BTreeMap::new(),
        };
        let product_id = resolved_string_field(&input, "id").unwrap_or_default();
        let local_product = self.store.product_staged_or_base(&product_id);
        let needs_publication_hydration = local_product
            .as_ref()
            .is_none_or(|product| !product_publication_state_known(product));
        let hydrated_product = needs_publication_hydration
            .then(|| self.hydrate_product_for_tags(&product_id, request))
            .flatten();
        let mut product = hydrated_product.or(local_product).unwrap_or_else(|| {
            let timestamp = default_product_timestamp();
            ProductRecord {
                id: product_id.clone(),
                created_at: timestamp.clone(),
                updated_at: timestamp,
                status: "ACTIVE".to_string(),
                ..ProductRecord::default()
            }
        });
        let enforce_known_publication_state = product_publication_state_known(&product);

        let targets = product_publication_input_entries(&input);
        if !self.store.has_known_publication_catalog() {
            let _ = self.hydrate_publishable_payload_shop(&product_id, request);
        }
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
                        let Some(publication_id) =
                            self.product_publication_target_publication_id(target)
                        else {
                            continue;
                        };
                        if !existing
                            .iter()
                            .any(|entry| entry.publication_id == publication_id)
                        {
                            existing.push(ProductPublicationEntry {
                                publication_id,
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
                        .filter_map(|target| self.product_publication_target_publication_id(target))
                        .collect::<BTreeSet<_>>();
                    existing.retain(|entry| !remove_ids.contains(&entry.publication_id));
                }
                _ => {}
            }
            product.updated_at = self.next_product_updated_at(&product.updated_at);
            set_product_publication_entries(&mut product, existing);
            self.store.stage_product(product.clone());
        }

        let product_publications = product_visible_publication_entries(&product)
            .iter()
            .map(|entry| canonical_product_publication_node(&product, entry))
            .collect::<Vec<_>>();
        let has_user_errors = !user_errors.is_empty();
        let payload = json!({
            "product": self.product_canonical_value(&product),
            "productPublications": product_publications,
            "shop": Value::Null,
            "userErrors": user_errors,
        });
        if !has_user_errors {
            ResolverOutcome::value(payload).with_log_draft(LogDraft::staged(
                root_field,
                "products",
                vec![product_id],
            ))
        } else {
            ResolverOutcome::value(payload)
        }
    }

    fn product_publication_target_publication_id(
        &self,
        target: &ProductPublicationInputEntry,
    ) -> Option<String> {
        target.publication_id.clone().or_else(|| {
            target
                .channel_id
                .as_deref()
                .and_then(|channel_id| self.store.publication_id_for_channel_id(channel_id))
        })
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
            let target_publication_id = if let Some(publication_id) = target.publication_id.clone()
            {
                Some(publication_id)
            } else if let Some(channel_id) = target.channel_id.as_deref() {
                match self.store.publication_id_for_channel_id(channel_id) {
                    Some(publication_id) => Some(publication_id),
                    None if !channel_id.is_empty() => {
                        errors.push(user_error_omit_code(
                            json!(["productPublications", field_index, "publicationId"]),
                            "Channel does not exist or is not publishable",
                            None,
                        ));
                        continue;
                    }
                    None => None,
                }
            } else {
                None
            };
            match target_publication_id.as_deref() {
                Some("") | None => errors.push(user_error_omit_code(
                    json!(["productPublications", field_index, "publicationId"]),
                    "PublicationId cannot be empty",
                    None,
                )),
                Some(id) if !self.store.has_publication_id(id) => {
                    errors.push(user_error_omit_code(
                        json!(["productPublications", field_index, "publicationId"]),
                        "Publication does not exist or is not publishable",
                        None,
                    ));
                }
                Some(id) if !seen.insert(id.to_string()) => {
                    errors.push(user_error_omit_code(
                        json!(["productPublications", field_index, "publicationId"]),
                        "The same publication was specified more than once",
                        None,
                    ));
                }
                Some(id)
                    if root_field == "productPublish"
                        && enforce_known_publication_state
                        && product_is_published_on_publication(product, id) =>
                {
                    errors.push(user_error_omit_code(
                        json!(["productPublications", field_index, "publicationId"]),
                        "Product is already published on this publication",
                        None,
                    ));
                }
                Some(id)
                    if root_field == "productUnpublish"
                        && enforce_known_publication_state
                        && !product_is_published_on_publication(product, id) =>
                {
                    errors.push(user_error_omit_code(
                        json!(["productPublications", field_index, "publicationId"]),
                        "Product is not published on this publication",
                        None,
                    ));
                }
                Some(_) => {}
            }
            if target
                .publish_date
                .as_deref()
                .map(product_publication_publish_date_is_before_1970)
                .unwrap_or(false)
            {
                errors.push(user_error_omit_code(
                    json!(["productPublications", field_index, "publishDate"]),
                    "Publish date must be a date after the year 1969",
                    None,
                ));
            }
        }
        errors
    }
}

fn tags_mutation_payload_value(node: Option<Value>) -> Value {
    json!({
        "node": node,
        "userErrors": [],
    })
}

fn tags_not_found_resolver_outcome(
    resource_type: &str,
    id: &str,
    root_field: &str,
) -> ResolverOutcome<Value> {
    ResolverOutcome::value(json!({
        "node": Value::Null,
        "userErrors": [user_error_omit_code(
            ["id"],
            &format!("{resource_type} does not exist"),
            None,
        )],
    }))
    .with_log_draft(LogDraft::staged(
        root_field,
        "products",
        vec![id.to_string()],
    ))
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
            .is_some_and(|id| is_shopify_gid_of_type(id, "MediaImage")),
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
            mutation CreateVariantForTest($productId: ID!, $variants: [ProductVariantsBulkInput!]!) {
              productVariantsBulkCreate(productId: $productId, variants: $variants) {
                productVariants { id sku price }
                userErrors { field message }
              }
            }
            "#,
            json!({
                "productId": "gid://shopify/Product/1",
                "variants": [{
                    "inventoryItem": { "sku": sku },
                    "price": price
                }]
            }),
        ));
        assert_eq!(response.status, 200);
        assert_eq!(
            response.body["data"]["productVariantsBulkCreate"]["userErrors"],
            json!([])
        );
        response.body["data"]["productVariantsBulkCreate"]["productVariants"][0].clone()
    }

    #[test]
    fn product_create_stages_top_level_media_from_variable() {
        let mut proxy = test_proxy();
        let response = proxy.process_request(test_request(
            r#"
            mutation CreateProductWithMedia($productInput: ProductCreateInput!, $mediaPayload: [CreateMediaInput!]) {
              productCreate(product: $productInput, media: $mediaPayload) {
                product {
                  id
                  title
                  media(first: 10) {
                    nodes {
                      __typename
                      id
                      alt
                      mediaContentType
                      status
                      preview { image { url } }
                      ... on MediaImage { image { url } }
                    }
                  }
                }
                userErrors { field message }
              }
            }
            "#,
            json!({
                "productInput": {
                    "title": "Created with media",
                    "status": "DRAFT"
                },
                "mediaPayload": [{
                    "mediaContentType": "IMAGE",
                    "originalSource": "https://placehold.co/640x480/png",
                    "alt": "Top level image"
                }]
            }),
        ));
        assert_eq!(response.status, 200);
        let payload = &response.body["data"]["productCreate"];
        assert_eq!(payload["userErrors"], json!([]));
        assert_eq!(
            payload["product"]["media"]["nodes"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            payload["product"]["media"]["nodes"][0]["__typename"],
            json!("MediaImage")
        );
        assert_eq!(
            payload["product"]["media"]["nodes"][0]["mediaContentType"],
            json!("IMAGE")
        );
        assert_eq!(
            payload["product"]["media"]["nodes"][0]["status"],
            json!("UPLOADED")
        );
        assert_eq!(
            payload["product"]["media"]["nodes"][0]["alt"],
            json!("Top level image")
        );

        let product_id = payload["product"]["id"].as_str().unwrap();
        let read = proxy.process_request(test_request(
            r#"
            query ReadProductMedia($id: ID!) {
              product(id: $id) {
                media(first: 10) {
                  nodes {
                    __typename
                    id
                    alt
                    mediaContentType
                    status
                    preview { image { url } }
                    ... on MediaImage { image { url } }
                  }
                }
              }
            }
            "#,
            json!({ "id": product_id }),
        ));
        assert_eq!(read.status, 200);
        assert_eq!(
            read.body["data"]["product"]["media"]["nodes"][0]["id"],
            payload["product"]["media"]["nodes"][0]["id"]
        );
        assert_eq!(
            read.body["data"]["product"]["media"]["nodes"][0]["status"],
            json!("PROCESSING")
        );
    }

    #[test]
    fn product_create_and_update_stage_top_level_media_from_inline_literals() {
        let mut proxy = test_proxy();
        let create = proxy.process_request(test_request(
            r#"
            mutation CreateProductWithInlineMedia {
              productCreate(
                product: { title: "Inline media owner", status: DRAFT }
                media: [{
                  mediaContentType: IMAGE
                  originalSource: "https://placehold.co/640x480/png"
                  alt: "Inline image"
                }]
              ) {
                product {
                  id
                  media(first: 10) {
                    nodes {
                      __typename
                      id
                      alt
                      mediaContentType
                      status
                    }
                  }
                }
                userErrors { field message }
              }
            }
            "#,
            json!({}),
        ));
        assert_eq!(create.status, 200);
        assert_eq!(
            create.body["data"]["productCreate"]["userErrors"],
            json!([])
        );
        let product_id = create.body["data"]["productCreate"]["product"]["id"]
            .as_str()
            .unwrap()
            .to_string();
        let image_id = create.body["data"]["productCreate"]["product"]["media"]["nodes"][0]["id"]
            .as_str()
            .unwrap()
            .to_string();

        let update = proxy.process_request(test_request(
            r#"
            mutation UpdateProductWithInlineMedia($productId: ID!) {
              productUpdate(
                product: { id: $productId, title: "Inline media update" }
                media: [{
                  mediaContentType: EXTERNAL_VIDEO
                  originalSource: "https://youtu.be/dQw4w9WgXcQ"
                  alt: "Inline external video"
                }]
              ) {
                product {
                  title
                  media(first: 10) {
                    nodes {
                      __typename
                      id
                      alt
                      mediaContentType
                      status
                      ... on ExternalVideo {
                        originUrl
                        embedUrl
                      }
                    }
                  }
                }
                userErrors { field message }
              }
            }
            "#,
            json!({ "productId": product_id }),
        ));
        assert_eq!(update.status, 200);
        let payload = &update.body["data"]["productUpdate"];
        assert_eq!(payload["userErrors"], json!([]));
        assert_eq!(payload["product"]["title"], json!("Inline media update"));
        let nodes = payload["product"]["media"]["nodes"].as_array().unwrap();
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0]["id"], json!(image_id));
        assert_eq!(nodes[0]["alt"], json!("Inline image"));
        assert_eq!(nodes[1]["__typename"], json!("ExternalVideo"));
        assert_eq!(nodes[1]["alt"], json!("Inline external video"));
        assert_eq!(nodes[1]["originUrl"], json!("https://youtu.be/dQw4w9WgXcQ"));
        assert_eq!(
            nodes[1]["embedUrl"],
            json!("https://www.youtube.com/embed/dQw4w9WgXcQ")
        );
    }

    #[test]
    fn product_update_appends_top_level_media_and_preserves_existing_media() {
        let mut proxy = test_proxy();
        let create = proxy.process_request(test_request(
            r#"
            mutation CreateProductWithImage($product: ProductCreateInput!, $media: [CreateMediaInput!]) {
              productCreate(product: $product, media: $media) {
                product {
                  id
                  media(first: 10) {
                    nodes { id alt mediaContentType status }
                  }
                }
                userErrors { field message }
              }
            }
            "#,
            json!({
                "product": { "title": "Update media owner" },
                "media": [{
                    "mediaContentType": "IMAGE",
                    "originalSource": "https://placehold.co/320x240/png",
                    "alt": "Existing image"
                }]
            }),
        ));
        assert_eq!(create.status, 200);
        assert_eq!(
            create.body["data"]["productCreate"]["userErrors"],
            json!([])
        );
        let product_id = create.body["data"]["productCreate"]["product"]["id"]
            .as_str()
            .unwrap()
            .to_string();
        let image_id = create.body["data"]["productCreate"]["product"]["media"]["nodes"][0]["id"]
            .as_str()
            .unwrap()
            .to_string();

        let update = proxy.process_request(test_request(
            r#"
            mutation UpdateProductWithExternalVideo($input: ProductUpdateInput!, $mediaInput: [CreateMediaInput!]) {
              productUpdate(product: $input, media: $mediaInput) {
                product {
                  id
                  title
                  media(first: 10) {
                    nodes {
                      __typename
                      id
                      alt
                      mediaContentType
                      status
                      ... on ExternalVideo {
                        originUrl
                        embedUrl
                      }
                    }
                  }
                }
                userErrors { field message }
              }
            }
            "#,
            json!({
                "input": {
                    "id": product_id,
                    "title": "Updated with external video"
                },
                "mediaInput": [{
                    "mediaContentType": "EXTERNAL_VIDEO",
                    "originalSource": "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
                    "alt": "Top level external video"
                }]
            }),
        ));
        assert_eq!(update.status, 200);
        let payload = &update.body["data"]["productUpdate"];
        assert_eq!(payload["userErrors"], json!([]));
        let nodes = payload["product"]["media"]["nodes"].as_array().unwrap();
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0]["id"], json!(image_id));
        assert_eq!(nodes[0]["status"], json!("PROCESSING"));
        assert_eq!(nodes[1]["__typename"], json!("ExternalVideo"));
        assert_eq!(nodes[1]["mediaContentType"], json!("EXTERNAL_VIDEO"));
        assert_eq!(nodes[1]["status"], json!("UPLOADED"));
        assert_eq!(nodes[1]["originUrl"], json!("https://youtu.be/dQw4w9WgXcQ"));
        assert_eq!(
            nodes[1]["embedUrl"],
            json!("https://www.youtube.com/embed/dQw4w9WgXcQ")
        );

        let read = proxy.process_request(test_request(
            r#"
            query ReadUpdatedProductMedia($id: ID!) {
              product(id: $id) {
                title
                media(first: 10) {
                  nodes {
                    __typename
                    id
                    alt
                    mediaContentType
                    status
                    ... on ExternalVideo {
                      originUrl
                      embedUrl
                    }
                  }
                }
              }
            }
            "#,
            json!({ "id": product_id }),
        ));
        assert_eq!(read.status, 200);
        assert_eq!(
            read.body["data"]["product"]["title"],
            json!("Updated with external video")
        );
        assert_eq!(
            read.body["data"]["product"]["media"]["nodes"],
            payload["product"]["media"]["nodes"]
        );
    }

    #[test]
    fn product_media_validation_errors_are_atomic() {
        let mut proxy = test_proxy();
        let invalid_create = proxy.process_request(test_request(
            r#"
            mutation InvalidCreateMedia($product: ProductCreateInput!, $media: [CreateMediaInput!]) {
              productCreate(product: $product, media: $media) {
                product { id }
                userErrors { field message }
              }
            }
            "#,
            json!({
                "product": { "title": "Invalid media create" },
                "media": [{
                    "mediaContentType": "IMAGE",
                    "originalSource": "not-a-url",
                    "alt": "Invalid"
                }]
            }),
        ));
        assert_eq!(invalid_create.status, 200);
        assert_eq!(
            invalid_create.body["data"]["productCreate"]["product"],
            Value::Null
        );
        assert_eq!(
            invalid_create.body["data"]["productCreate"]["userErrors"],
            json!([{
                "field": ["media", "0", "originalSource"],
                "message": "Image URL is invalid"
            }])
        );

        let invalid_update = proxy.process_request(test_request(
            r#"
            mutation InvalidUpdateMedia($input: ProductUpdateInput!, $media: [CreateMediaInput!]) {
              productUpdate(product: $input, media: $media) {
                product {
                  id
                  title
                  media(first: 10) { nodes { id } }
                }
                userErrors { field message }
              }
            }
            "#,
            json!({
                "input": {
                    "id": "gid://shopify/Product/1",
                    "title": "Should not persist"
                },
                "media": [{
                    "mediaContentType": "IMAGE",
                    "originalSource": "not-a-url",
                    "alt": "Invalid"
                }]
            }),
        ));
        assert_eq!(invalid_update.status, 200);
        assert_eq!(
            invalid_update.body["data"]["productUpdate"]["userErrors"],
            json!([{
                "field": ["media", "0", "originalSource"],
                "message": "Image URL is invalid"
            }])
        );
        assert_eq!(
            invalid_update.body["data"]["productUpdate"]["product"]["title"],
            json!("Seeded product")
        );
        assert_eq!(
            invalid_update.body["data"]["productUpdate"]["product"]["media"]["nodes"],
            json!([])
        );

        let read = proxy.process_request(test_request(
            r#"
            query ReadAfterInvalidUpdate($id: ID!) {
              product(id: $id) {
                title
                media(first: 10) { nodes { id } }
              }
            }
            "#,
            json!({ "id": "gid://shopify/Product/1" }),
        ));
        assert_eq!(read.status, 200);
        assert_eq!(
            read.body["data"]["product"]["title"],
            json!("Seeded product")
        );
        assert_eq!(read.body["data"]["product"]["media"]["nodes"], json!([]));
    }

    #[test]
    fn bulk_update_position_is_rejected_by_the_versioned_schema_before_staging() {
        let mut proxy = test_proxy();
        create_variant(&mut proxy, "RED", "10.00");
        create_variant(&mut proxy, "BLUE", "11.00");
        let green = create_variant(&mut proxy, "GREEN", "12.00");
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
        let response = proxy.process_request(request);

        assert_eq!(response.status, 200);
        assert_eq!(response.body.get("data"), None);
        assert_eq!(
            response.body["errors"][0]["extensions"]["code"],
            json!("INVALID_VARIABLE")
        );
        assert_eq!(
            response.body["errors"][0]["extensions"]["problems"][0]["path"],
            json!([0, "position"])
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
                { "sku": "RED", "position": 1 },
                { "sku": "BLUE", "position": 2 },
                { "sku": "GREEN", "position": 3 }
            ])
        );
    }
}

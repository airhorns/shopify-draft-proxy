use super::*;

impl DraftProxy {
    pub(in crate::proxy) fn product_set(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let root_field = primary_root_field(query, variables);
        let (response_key, payload_selection, arguments) =
            primary_root_response_parts(query, variables, || "productSet".into());
        let product_selection =
            selected_child_selection(&payload_selection, "product").unwrap_or_default();
        let operation_selection =
            selected_child_selection(&payload_selection, "productSetOperation").unwrap_or_default();
        let input = match product_input(query, variables) {
            Some(input) => input,
            None => return MutationOutcome::response(json_error(400, "productSet requires input")),
        };
        let identifier = match arguments.get("identifier") {
            Some(ResolvedValue::Object(identifier)) => Some(identifier.clone()),
            _ => None,
        };

        if identifier.is_some() && input.contains_key("id") {
            return MutationOutcome::response(self.product_set_user_error_response(
                &response_key,
                &payload_selection,
                &product_selection,
                None,
                vec![user_error(
                    ["input"],
                    "The id field is not allowed if identifier is provided.",
                    Some("ID_NOT_ALLOWED"),
                )],
            ));
        }

        if let Some(response) =
            product_set_shape_error_response(&response_key, &payload_selection, &input)
        {
            return MutationOutcome::response(response);
        }

        let length_errors = product_scalar_length_user_errors(
            &input,
            ProductScalarLengthValidationShape::ProductSetInput,
        );
        if !length_errors.is_empty() {
            return MutationOutcome::response(self.product_set_user_error_response(
                &response_key,
                &payload_selection,
                &product_selection,
                None,
                length_errors,
            ));
        }

        let variant_input_errors = product_set_variant_input_errors(&input);
        if !variant_input_errors.is_empty() {
            return MutationOutcome::response(self.product_set_user_error_response(
                &response_key,
                &payload_selection,
                &product_selection,
                None,
                variant_input_errors,
            ));
        }

        // Reject input variants whose option-value combination duplicates an earlier
        // input variant. Shopify anchors one userError per later collision (the first
        // occurrence is accepted) and titles it with the variant's option values.
        let duplicate_variant_errors = product_set_duplicate_variant_errors(&input);
        if !duplicate_variant_errors.is_empty() {
            return MutationOutcome::response(self.product_set_user_error_response(
                &response_key,
                &payload_selection,
                &product_selection,
                None,
                duplicate_variant_errors,
            ));
        }

        let existing_id = resolved_string_field(&input, "id")
            .or_else(|| resolved_string_field(&input, "productId"))
            .or_else(|| {
                identifier
                    .as_ref()
                    .and_then(|identifier| resolved_string_field(identifier, "id"))
            });
        let existing = existing_id
            .as_deref()
            .and_then(|id| self.store.product_staged_or_base(id));
        if existing_id.is_some() && existing.is_none() {
            return MutationOutcome::response(self.product_set_user_error_response(
                &response_key,
                &payload_selection,
                &product_selection,
                None,
                vec![user_error(
                    ["input", "id"],
                    "Product does not exist",
                    Some("PRODUCT_DOES_NOT_EXIST"),
                )],
            ));
        }

        let by_handle = identifier.as_ref().and_then(|identifier| {
            resolved_string_field(identifier, "handle")
                .and_then(|handle| self.store.product_by_handle(&handle).cloned())
        });
        let base = existing.or(by_handle);
        let category = if let Some(category_id) = product_category_input_id(&input) {
            match product_category_value(&category_id) {
                Some(category) => Some(category),
                None => {
                    let location = root_field
                        .as_ref()
                        .map(|field| field.location)
                        .unwrap_or(SourceLocation { line: 1, column: 1 });
                    return MutationOutcome::response(invalid_product_taxonomy_node_id_response(
                        &response_key,
                        location,
                    ));
                }
            }
        } else {
            None
        };
        let product_id = base
            .as_ref()
            .map(|product| product.id.clone())
            .unwrap_or_else(|| self.next_proxy_synthetic_gid("Product"));
        let timestamp = self.next_product_timestamp();
        let current_updated_at = base
            .as_ref()
            .map(|product| product.updated_at.as_str())
            .unwrap_or(timestamp.as_str());
        let title = resolved_string_field(&input, "title")
            .or_else(|| base.as_ref().map(|product| product.title.clone()))
            .unwrap_or_default();
        let mut product = ProductRecord {
            id: product_id.clone(),
            created_at: base
                .as_ref()
                .map(|product| product.created_at.clone())
                .unwrap_or_else(|| timestamp.clone()),
            updated_at: self.next_product_updated_at(current_updated_at),
            title: title.clone(),
            handle: resolved_string_field(&input, "handle")
                .or_else(|| base.as_ref().map(|product| product.handle.clone()))
                .unwrap_or_else(|| slugify_handle(&title)),
            status: resolved_string_field(&input, "status")
                .or_else(|| base.as_ref().map(|product| product.status.clone()))
                .unwrap_or_else(|| "ACTIVE".to_string()),
            description_html: resolved_string_field(&input, "descriptionHtml")
                .or_else(|| {
                    base.as_ref()
                        .map(|product| product.description_html.clone())
                })
                .unwrap_or_default(),
            vendor: resolved_string_field(&input, "vendor")
                .or_else(|| base.as_ref().map(|product| product.vendor.clone()))
                .unwrap_or_default(),
            product_type: resolved_string_field(&input, "productType")
                .or_else(|| base.as_ref().map(|product| product.product_type.clone()))
                .unwrap_or_default(),
            tags: if input.contains_key("tags") {
                normalize_taggable_tags(list_string_field(&input, "tags"))
            } else {
                base.as_ref()
                    .map(|product| product.tags.clone())
                    .unwrap_or_default()
            },
            template_suffix: resolved_string_field(&input, "templateSuffix")
                .or_else(|| base.as_ref().map(|product| product.template_suffix.clone()))
                .unwrap_or_default(),
            seo_title: resolved_object_string_field(&input, "seo", "title")
                .or_else(|| base.as_ref().map(|product| product.seo_title.clone()))
                .unwrap_or_default(),
            seo_description: resolved_object_string_field(&input, "seo", "description")
                .or_else(|| base.as_ref().map(|product| product.seo_description.clone()))
                .unwrap_or_default(),
            total_inventory: base
                .as_ref()
                .map(|product| product.total_inventory)
                .unwrap_or_default(),
            tracks_inventory: base
                .as_ref()
                .map(|product| product.tracks_inventory)
                .unwrap_or(false),
            media: base
                .as_ref()
                .map(|product| product.media.clone())
                .unwrap_or_default(),
            variants: base
                .as_ref()
                .map(|product| product.variants.clone())
                .unwrap_or_default(),
            collections: base
                .as_ref()
                .map(|product| product.collections.clone())
                .unwrap_or_default(),
            extra_fields: base
                .as_ref()
                .map(|product| product.extra_fields.clone())
                .unwrap_or_default(),
        };

        if let Some(category) = category {
            product
                .extra_fields
                .insert("category".to_string(), category);
        }
        if let Some(requires_selling_plan) = input.get("requiresSellingPlan") {
            product.extra_fields.insert(
                "requiresSellingPlan".to_string(),
                resolved_value_json(requires_selling_plan),
            );
        }
        if input.contains_key("productOptions") {
            product.extra_fields.insert(
                "options".to_string(),
                product_set_options_json(&mut self.next_synthetic_id, &input),
            );
        }
        if input.contains_key("variants") {
            let variants = self.stage_product_set_variants(&product_id, &input);
            // `totalInventory` only counts tracked variants (see product_json_with_variants).
            product.total_inventory = variants
                .iter()
                .filter(|variant| variant.inventory_item.tracked)
                .map(|variant| variant.inventory_quantity)
                .sum::<i64>();
            product.tracks_inventory = variants
                .iter()
                .any(|variant| variant.inventory_item.tracked);
        }

        // Stage `metafields` supplied on the input so the mutation payload and the
        // downstream `product`/`metafield(s)` reads resolve them. Shopify allocates the
        // metafield GIDs independently, so the parity spec matches `id` via `shopify-gid`.
        let metafield_nodes = self.product_set_input_metafield_nodes(&input);
        if !metafield_nodes.is_empty() {
            product.extra_fields.insert(
                "metafields".to_string(),
                connection_json(metafield_nodes.clone()),
            );
            if let Some(first) = metafield_nodes.first() {
                product
                    .extra_fields
                    .insert("metafield".to_string(), first.clone());
            }
        }
        // Shopify returns a store-specific signed preview URL for staged products; the
        // parity spec matches it via `non-empty-string`, so a stable local URL suffices.
        product
            .extra_fields
            .entry("onlineStorePreviewUrl".to_string())
            .or_insert_with(|| {
                json!(format!(
                    "https://shopify-draft-proxy.preview/products/{}",
                    resource_id_tail(&product_id)
                ))
            });
        // Shopify reports `null` (not an empty string) for unset SEO fields and template
        // suffix. Render the effective value (input or carried-over base) as null when empty.
        product.extra_fields.insert(
            "seo".to_string(),
            json!({
                "title": (!product.seo_title.is_empty()).then(|| product.seo_title.clone()),
                "description":
                    (!product.seo_description.is_empty()).then(|| product.seo_description.clone()),
            }),
        );
        product.extra_fields.insert(
            "templateSuffix".to_string(),
            if product.template_suffix.is_empty() {
                Value::Null
            } else {
                json!(product.template_suffix)
            },
        );

        self.store.stage_product(product.clone());

        let operation = if resolved_bool_field(&arguments, "synchronous") == Some(false) {
            let operation = ProductOperationRecord {
                id: self.next_proxy_synthetic_gid("ProductSetOperation"),
                kind: ProductOperationKind::Set,
                product_id: Some(product_id.clone()),
                new_product_id: None,
                user_errors: Vec::new(),
            };
            self.store
                .staged
                .product_operations
                .insert(operation.id.clone(), operation.clone());
            Some(operation)
        } else {
            None
        };

        let payload = selected_payload_json(&payload_selection, |selection| {
            match selection.name.as_str() {
                "product" => Some(if operation.is_some() {
                    Value::Null
                } else {
                    self.product_json_with_variants_and_currency_context(
                        &product,
                        &self.store.product_variants_for_product(&product_id),
                        &product_selection,
                        &self.store.shop_currency_code(),
                    )
                }),
                "productSetOperation" => Some(
                    operation
                        .as_ref()
                        .map(|operation| {
                            self.product_operation_json_with_status(
                                operation,
                                &operation_selection,
                                "CREATED",
                            )
                        })
                        .unwrap_or(Value::Null),
                ),
                "userErrors" => Some(Value::Array(Vec::new())),
                _ => None,
            }
        });
        MutationOutcome::staged(
            ok_json(json!({ "data": { response_key: payload } })),
            LogDraft::staged("productSet", "products", vec![product_id]),
        )
    }

    /// Build metafield node JSON for the `metafields` supplied on a `productSet` input.
    /// Each gets a locally allocated synthetic Metafield GID; namespace/key/type/value are
    /// echoed verbatim from the input so downstream reads resolve the same values Shopify
    /// would persist. Entries without a namespace and key are skipped.
    fn product_set_input_metafield_nodes(
        &mut self,
        input: &BTreeMap<String, ResolvedValue>,
    ) -> Vec<Value> {
        let mut nodes = Vec::new();
        for metafield in resolved_object_list_field(input, "metafields") {
            let namespace = resolved_string_field(&metafield, "namespace").unwrap_or_default();
            let key = resolved_string_field(&metafield, "key").unwrap_or_default();
            if namespace.is_empty() && key.is_empty() {
                continue;
            }
            let metafield_type = resolved_string_field(&metafield, "type")
                .unwrap_or_else(|| "single_line_text_field".to_string());
            let value = resolved_string_field(&metafield, "value").unwrap_or_default();
            let id = self.next_proxy_synthetic_gid("Metafield");
            nodes.push(json!({
                "id": id,
                "namespace": namespace,
                "key": key,
                "type": metafield_type,
                "value": value,
            }));
        }
        nodes
    }

    pub(in crate::proxy) fn product_duplicate(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let field = primary_root_field(query, variables);
        if let Some(field) = field.as_ref() {
            if let Some(response) = product_status_argument_validation_error(
                request,
                query,
                field,
                "newStatus",
                "Field",
                "productDuplicate",
                "ProductStatus",
            ) {
                return MutationOutcome::response(response);
            }
        }
        let (response_key, payload_selection, arguments) = field
            .map(|field| (field.response_key, field.selection, field.arguments))
            .unwrap_or_else(|| {
                primary_root_response_parts(query, variables, || "productDuplicate".into())
            });
        let new_product_selection =
            selected_child_selection(&payload_selection, "newProduct").unwrap_or_default();
        let operation_selection =
            selected_child_selection(&payload_selection, "productDuplicateOperation")
                .unwrap_or_default();
        let product_id = resolved_string_field(&arguments, "productId").unwrap_or_default();
        let new_title = resolved_string_field(&arguments, "newTitle").unwrap_or_default();
        let new_status = resolved_string_field(&arguments, "newStatus");
        let synchronous = resolved_bool_field(&arguments, "synchronous").unwrap_or(true);
        // The source product usually lives upstream during parity replay; hydrate it via
        // the shared `nodes(ids:)` observation path so the duplicate is built from real
        // source data rather than failing with "Product does not exist".
        if !product_id.is_empty() && self.store.product_staged_or_base(&product_id).is_none() {
            self.hydrate_product_nodes_for_observation_with_request(
                request,
                vec![product_id.clone()],
            );
        }
        let source = self.store.product_staged_or_base(&product_id);

        if source.is_none() && !synchronous {
            let operation = ProductOperationRecord {
                id: self.next_proxy_synthetic_gid("ProductDuplicateOperation"),
                kind: ProductOperationKind::Duplicate,
                product_id: None,
                new_product_id: None,
                user_errors: vec![user_error_omit_code(
                    ["productId"],
                    "Product does not exist",
                    None,
                )],
            };
            self.store
                .staged
                .product_operations
                .insert(operation.id.clone(), operation.clone());
            let payload = self.product_duplicate_payload_json(
                None,
                Some(&operation),
                &payload_selection,
                &new_product_selection,
                &operation_selection,
                Vec::new(),
            );
            return MutationOutcome::staged(
                ok_json(json!({ "data": { response_key: payload } })),
                LogDraft::staged("productDuplicate", "products", vec![product_id]),
            );
        }

        let Some(source) = source else {
            let payload = self.product_duplicate_payload_json(
                None,
                None,
                &payload_selection,
                &new_product_selection,
                &operation_selection,
                vec![user_error_omit_code(
                    ["productId"],
                    "Product does not exist",
                    None,
                )],
            );
            return MutationOutcome::response(ok_json(
                json!({ "data": { response_key: payload } }),
            ));
        };

        let duplicate = self.duplicate_product_record(&source, &new_title, new_status.as_deref());
        let duplicate_id = duplicate.id.clone();
        self.stage_duplicate_variants(&source.id, &duplicate_id);
        self.store.stage_product(duplicate.clone());

        let operation = if synchronous {
            None
        } else {
            let operation = ProductOperationRecord {
                id: self.next_proxy_synthetic_gid("ProductDuplicateOperation"),
                kind: ProductOperationKind::Duplicate,
                product_id: Some(source.id.clone()),
                new_product_id: Some(duplicate_id.clone()),
                user_errors: Vec::new(),
            };
            self.store
                .staged
                .product_operations
                .insert(operation.id.clone(), operation.clone());
            Some(operation)
        };
        let payload = self.product_duplicate_payload_json(
            Some(&duplicate),
            operation.as_ref(),
            &payload_selection,
            &new_product_selection,
            &operation_selection,
            Vec::new(),
        );
        MutationOutcome::staged(
            ok_json(json!({ "data": { response_key: payload } })),
            LogDraft::staged(
                "productDuplicate",
                "products",
                vec![source.id.clone(), duplicate_id],
            ),
        )
    }

    pub(in crate::proxy) fn product_bundle_mutation(
        &mut self,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let (response_key, payload_selection, arguments) =
            primary_root_response_parts(query, variables, || root_field.into());
        let operation_selection =
            selected_child_selection(&payload_selection, "productBundleOperation")
                .unwrap_or_default();
        let input = match arguments.get("input") {
            Some(ResolvedValue::Object(input)) => input.clone(),
            _ => BTreeMap::new(),
        };

        if root_field == "productBundleUpdate" {
            let product_id = resolved_string_field(&input, "productId")
                .or_else(|| resolved_string_field(&arguments, "id"))
                .unwrap_or_default();
            let Some(mut product) = self.store.product_staged_or_base(&product_id) else {
                return MutationOutcome::response(self.product_bundle_error_response(
                    &response_key,
                    &payload_selection,
                    vec![user_error_omit_code(
                        Value::Null,
                        "Product does not exist",
                        None,
                    )],
                ));
            };
            if let Some(errors) = self.product_bundle_user_errors(&input) {
                return MutationOutcome::response(self.product_bundle_error_response(
                    &response_key,
                    &payload_selection,
                    errors,
                ));
            }
            if let Some(title) = resolved_string_field(&input, "title") {
                product.title = title;
                product.handle = slugify_handle(&product.title);
            }
            product.updated_at = self.next_product_updated_at(&product.updated_at);
            product
                .extra_fields
                .insert("requiresComponents".to_string(), Value::Bool(true));
            self.store.stage_product(product.clone());
            let operation =
                self.stage_product_bundle_operation(Some(product.id.clone()), Vec::new());
            let payload = self.product_bundle_payload_json(
                &operation,
                &payload_selection,
                &operation_selection,
                Vec::new(),
            );
            return MutationOutcome::staged(
                ok_json(json!({ "data": { response_key: payload } })),
                LogDraft::staged(root_field, "products", vec![product.id]),
            );
        }

        if let Some(errors) = self.product_bundle_user_errors(&input) {
            return MutationOutcome::response(self.product_bundle_error_response(
                &response_key,
                &payload_selection,
                errors,
            ));
        }

        let title = resolved_string_field(&input, "title").unwrap_or_default();
        let id = self.next_proxy_synthetic_gid("Product");
        let timestamp = self.next_product_timestamp();
        let mut product = ProductRecord {
            id: id.clone(),
            created_at: timestamp.clone(),
            updated_at: timestamp,
            title: title.clone(),
            handle: slugify_handle(&title),
            status: "DRAFT".to_string(),
            extra_fields: BTreeMap::from([("requiresComponents".to_string(), Value::Bool(true))]),
            ..ProductRecord::default()
        };
        if product.title.is_empty() {
            product.title = "Bundle".to_string();
            product.handle = "bundle".to_string();
        }
        self.store.stage_product(product.clone());
        let operation = self.stage_product_bundle_operation(Some(id.clone()), Vec::new());
        let payload = self.product_bundle_payload_json(
            &operation,
            &payload_selection,
            &operation_selection,
            Vec::new(),
        );
        MutationOutcome::staged(
            ok_json(json!({ "data": { response_key: payload } })),
            LogDraft::staged(root_field, "products", vec![id]),
        )
    }

    fn product_set_user_error_response(
        &self,
        response_key: &str,
        payload_selection: &[SelectedField],
        product_selection: &[SelectedField],
        product: Option<&ProductRecord>,
        user_errors: Vec<Value>,
    ) -> Response {
        let payload = selected_payload_json(payload_selection, |selection| {
            match selection.name.as_str() {
                "product" => Some(
                    product
                        .map(|product| {
                            self.product_json_with_variants_and_currency_context(
                                product,
                                &[],
                                product_selection,
                                &self.store.shop_currency_code(),
                            )
                        })
                        .unwrap_or(Value::Null),
                ),
                "productSetOperation" => Some(Value::Null),
                "userErrors" => selected_user_errors_field(user_errors.as_slice(), selection),
                _ => None,
            }
        });
        ok_json(json!({ "data": { response_key: payload } }))
    }

    fn stage_product_set_variants(
        &mut self,
        product_id: &str,
        input: &BTreeMap<String, ResolvedValue>,
    ) -> Vec<ProductVariantRecord> {
        // `productSet` replaces the full variant set, but Shopify matches incoming
        // variants to existing ones by their option-value signature and updates them
        // in place: the variant id and inventory item id are preserved, as are fields
        // the input does not re-specify (notably `inventoryItem.tracked`). Capture the
        // existing variants so we can reuse their identities and carried-over fields.
        let existing_variants = self.store.product_variants_for_product(product_id);
        let existing_by_signature = existing_variants
            .iter()
            .map(|variant| {
                (
                    product_set_selected_option_signature(&variant.selected_options),
                    variant.clone(),
                )
            })
            .collect::<BTreeMap<_, _>>();

        let variant_inputs = resolved_object_list_field(input, "variants");
        let mut staged = Vec::new();
        let mut staged_signatures = BTreeSet::new();
        for variant_input in &variant_inputs {
            let selected_options = product_set_variant_selected_options(variant_input);
            let signature = product_set_selected_option_signature(&selected_options);
            let existing = existing_by_signature.get(&signature);
            let variant_id = resolved_string_field(variant_input, "id")
                .or_else(|| existing.map(|variant| variant.id.clone()))
                .unwrap_or_else(|| self.next_proxy_synthetic_gid("ProductVariant"));
            let inventory_item_id = resolved_object_field(variant_input, "inventoryItem")
                .and_then(|inventory_item| resolved_string_field(&inventory_item, "id"))
                .or_else(|| existing.map(|variant| variant.inventory_item.id.clone()))
                .unwrap_or_else(|| self.next_proxy_synthetic_gid("InventoryItem"));
            let mut variant = product_variant_record_from_create_input(
                variant_input,
                variant_id,
                product_id.to_string(),
                inventory_item_id,
            );
            apply_product_set_option_values_to_variant(&mut variant, selected_options);
            apply_inventory_quantities_to_variant(&mut variant, variant_input);
            // When the input omits `inventoryItem.tracked`, Shopify preserves the
            // existing variant's value (defaulting to `true` for a brand-new variant).
            let explicit_tracked = resolved_object_field(variant_input, "inventoryItem")
                .and_then(|inventory_item| resolved_bool_field(&inventory_item, "tracked"));
            if explicit_tracked.is_none() {
                if let Some(existing) = existing {
                    variant.inventory_item.tracked = existing.inventory_item.tracked;
                }
            }
            self.stage_product_set_variant_inventory(&mut variant, variant_input);
            self.store.stage_product_variant(variant.clone());
            staged_signatures.insert(signature);
            staged.push(variant);
        }

        // Drop existing variants whose option signature is absent from the new set.
        for existing in &existing_variants {
            let signature = product_set_selected_option_signature(&existing.selected_options);
            if !staged_signatures.contains(&signature) {
                self.store.delete_product_variant(&existing.id);
            }
        }

        staged
    }

    /// Synthesize per-location inventory levels for a staged `productSet` variant from
    /// the input's `inventoryQuantities`. This populates both the store-level inventory
    /// state (so top-level `inventoryItem`/`productVariant` reads resolve `inventoryLevels`)
    /// and the variant's `inventoryItem.extra_fields` (so nested
    /// `product.variants.nodes[].inventoryItem` reads render the same connection). Shopify
    /// mirrors `on_hand` to the supplied `available` quantity and leaves `incoming` at 0.
    fn stage_product_set_variant_inventory(
        &mut self,
        variant: &mut ProductVariantRecord,
        variant_input: &BTreeMap<String, ResolvedValue>,
    ) {
        let inventory_item_id = variant.inventory_item.id.clone();
        // Group the `available` quantities by location, preserving first-seen order.
        let mut location_order_local: Vec<String> = Vec::new();
        let mut available_by_location: BTreeMap<String, i64> = BTreeMap::new();
        for quantity in resolved_object_list_field(variant_input, "inventoryQuantities") {
            let name =
                resolved_string_field(&quantity, "name").unwrap_or_else(|| "available".to_string());
            if name != "available" {
                continue;
            }
            let Some(location_id) = resolved_string_field(&quantity, "locationId") else {
                continue;
            };
            let amount = resolved_int_field(&quantity, "quantity").unwrap_or(0);
            if !location_order_local.contains(&location_id) {
                location_order_local.push(location_id.clone());
            }
            *available_by_location.entry(location_id).or_insert(0) += amount;
        }
        if available_by_location.is_empty() {
            return;
        }

        let updated_at = self.next_inventory_quantity_timestamp();
        let mut level_nodes = Vec::new();
        for location_id in &location_order_local {
            let available = available_by_location.get(location_id).copied().unwrap_or(0);
            if !self.store.staged.locations.order.contains(location_id) {
                self.store.staged.locations.order.push(location_id.clone());
            }
            let key = (inventory_item_id.clone(), location_id.clone());
            let mut quantities = BTreeMap::new();
            quantities.insert("available".to_string(), available);
            quantities.insert("on_hand".to_string(), available);
            self.store
                .staged
                .inventory_levels
                .insert(key.clone(), quantities);
            // Record creation order so materialized `inventoryLevels` connections surface
            // these levels in the input's location order rather than the BTreeMap's
            // sorted-by-location-id fallback (which would reverse two-location variants).
            if !self.store.staged.inventory_level_order.contains(&key) {
                self.store.staged.inventory_level_order.push(key);
            }
            self.store.staged.inventory_quantity_updated_at.insert(
                (
                    inventory_item_id.clone(),
                    location_id.clone(),
                    "available".to_string(),
                ),
                updated_at.clone(),
            );
            level_nodes.push(json!({
                "id": inventory_level_id(&inventory_item_id, location_id),
                "location": {
                    "id": location_id,
                    "name": self.inventory_location_display_name(location_id)
                },
                "quantities": [
                    { "name": "available", "quantity": available, "updatedAt": updated_at },
                    { "name": "on_hand", "quantity": available, "updatedAt": Value::Null },
                    { "name": "incoming", "quantity": 0, "updatedAt": Value::Null }
                ]
            }));
        }

        // Seed the nested `inventoryItem` fields the downstream reads select. Shopify
        // reports `null` for unset origin/HS-code fields and a zero-weight measurement.
        let inventory_item = &mut variant.inventory_item.extra_fields;
        inventory_item.insert(
            "inventoryLevels".to_string(),
            json!({ "nodes": level_nodes }),
        );
        inventory_item
            .entry("countryCodeOfOrigin".to_string())
            .or_insert(Value::Null);
        inventory_item
            .entry("provinceCodeOfOrigin".to_string())
            .or_insert(Value::Null);
        inventory_item
            .entry("harmonizedSystemCode".to_string())
            .or_insert(Value::Null);
        inventory_item
            .entry("measurement".to_string())
            .or_insert(json!({ "weight": { "unit": "KILOGRAMS", "value": 0 } }));
    }

    fn duplicate_product_record(
        &mut self,
        source: &ProductRecord,
        new_title: &str,
        new_status: Option<&str>,
    ) -> ProductRecord {
        let mut duplicate = source.clone();
        duplicate.id = self.next_proxy_synthetic_gid("Product");
        duplicate.title = new_title.to_string();
        duplicate.handle = slugify_handle(new_title);
        if let Some(status) = new_status {
            duplicate.status = status.to_string();
        }
        let timestamp = self.next_product_timestamp();
        duplicate.created_at = timestamp.clone();
        duplicate.updated_at = timestamp;
        duplicate.variants = Vec::new();
        // Shopify copies media asynchronously: the duplicate's immediate payload (and the
        // downstream read right after) expose an empty media connection.
        duplicate.media = Vec::new();
        duplicate
    }

    fn stage_duplicate_variants(&mut self, source_id: &str, duplicate_id: &str) {
        let variants = self.store.product_variants_for_product(source_id);
        for source_variant in variants {
            let mut variant = source_variant;
            variant.id = self.next_proxy_synthetic_gid("ProductVariant");
            variant.product_id = duplicate_id.to_string();
            variant.inventory_item.id = self.next_proxy_synthetic_gid("InventoryItem");
            self.store.stage_product_variant(variant);
        }
    }

    fn product_duplicate_payload_json(
        &self,
        duplicate: Option<&ProductRecord>,
        operation: Option<&ProductOperationRecord>,
        payload_selection: &[SelectedField],
        new_product_selection: &[SelectedField],
        operation_selection: &[SelectedField],
        user_errors: Vec<Value>,
    ) -> Value {
        selected_payload_json(payload_selection, |selection| {
            match selection.name.as_str() {
                "newProduct" => Some(if operation.is_some() {
                    Value::Null
                } else {
                    duplicate
                        .map(|product| {
                            self.product_json_with_variants_and_currency_context(
                                product,
                                &self.store.product_variants_for_product(&product.id),
                                new_product_selection,
                                &self.store.shop_currency_code(),
                            )
                        })
                        .unwrap_or(Value::Null)
                }),
                "productDuplicateOperation" => Some(
                    operation
                        .map(|operation| {
                            self.product_operation_json_with_status(
                                operation,
                                operation_selection,
                                "CREATED",
                            )
                        })
                        .unwrap_or(Value::Null),
                ),
                "userErrors" => selected_user_errors_field(user_errors.as_slice(), selection),
                _ => None,
            }
        })
    }

    fn product_bundle_user_errors(
        &self,
        input: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Vec<Value>> {
        let components = resolved_object_list_field(input, "components");
        if input.contains_key("components") && components.is_empty() {
            return Some(vec![user_error_omit_code(
                Value::Null,
                "At least one component is required.",
                None,
            )]);
        }
        for component in components {
            let product_id = resolved_string_field(&component, "productId").unwrap_or_default();
            let Some(product) = self.store.product_by_id(&product_id) else {
                return Some(vec![user_error_omit_code(
                    Value::Null,
                    &format!(
                        "Failed to locate the following products: [{}]",
                        resource_id_tail(&product_id)
                    ),
                    None,
                )]);
            };
            if resolved_int_field(&component, "quantity").unwrap_or(1) > 2000 {
                return Some(vec![user_error_omit_code(
                    Value::Null,
                    &format!(
                        "Quantity cannot be greater than 2000. The following products have a quantity that exceeds the maximum: [{}]",
                        resource_id_tail(&product_id)
                    ),
                    None,
                )]);
            }
            let option_selections = resolved_object_list_field(&component, "optionSelections");
            let option_count = product
                .extra_fields
                .get("options")
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or_default();
            if option_count > 0 && option_selections.len() != option_count {
                return Some(vec![user_error_omit_code(
                    Value::Null,
                    &format!(
                        "Mapping of components targeting products need to map all of the options of the product. Missing or invalid options found for components targeting product_ids [{}].",
                        resource_id_tail(&product_id)
                    ),
                    None,
                )]);
            }
            if let Some(quantity_option) = resolved_object_field(&component, "quantityOption") {
                if resolved_object_list_field(&quantity_option, "values").len() == 1 {
                    return Some(vec![user_error_omit_code(
                        Value::Null,
                        &format!(
                            "Quantity options must have at least two values. Invalid quantity options found for components targeting product_ids [{}].",
                            resource_id_tail(&product_id)
                        ),
                        None,
                    )]);
                }
            }
        }
        None
    }

    fn product_bundle_error_response(
        &self,
        response_key: &str,
        payload_selection: &[SelectedField],
        user_errors: Vec<Value>,
    ) -> Response {
        let payload = selected_payload_json(payload_selection, |selection| {
            match selection.name.as_str() {
                "productBundleOperation" => Some(Value::Null),
                "userErrors" => selected_user_errors_field(user_errors.as_slice(), selection),
                _ => None,
            }
        });
        ok_json(json!({ "data": { response_key: payload } }))
    }

    fn stage_product_bundle_operation(
        &mut self,
        product_id: Option<String>,
        user_errors: Vec<Value>,
    ) -> ProductOperationRecord {
        let operation = ProductOperationRecord {
            id: self.next_proxy_synthetic_gid("ProductBundleOperation"),
            kind: ProductOperationKind::Bundle,
            product_id,
            new_product_id: None,
            user_errors,
        };
        self.store
            .staged
            .product_operations
            .insert(operation.id.clone(), operation.clone());
        operation
    }

    fn product_bundle_payload_json(
        &self,
        operation: &ProductOperationRecord,
        payload_selection: &[SelectedField],
        operation_selection: &[SelectedField],
        user_errors: Vec<Value>,
    ) -> Value {
        selected_payload_json(payload_selection, |selection| {
            match selection.name.as_str() {
                "productBundleOperation" => Some(self.product_operation_json_with_status(
                    operation,
                    operation_selection,
                    "CREATED",
                )),
                "userErrors" => selected_user_errors_field(user_errors.as_slice(), selection),
                _ => None,
            }
        })
    }

    fn product_operation_json_with_status(
        &self,
        operation: &ProductOperationRecord,
        selections: &[SelectedField],
        status: &str,
    ) -> Value {
        let typename = product_operation_typename(operation.kind);
        selected_payload_json(selections, |selection| {
            if !product_operation_selection_matches(selection, typename) {
                return None;
            }
            match selection.name.as_str() {
                "__typename" => Some(json!(typename)),
                "id" => Some(json!(operation.id)),
                "status" => Some(json!(status)),
                "product"
                    if status == "CREATED" && operation.kind != ProductOperationKind::Duplicate =>
                {
                    Some(Value::Null)
                }
                "product" => Some(self.product_operation_product_json(
                    operation.product_id.as_deref(),
                    &selection.selection,
                )),
                "newProduct"
                    if status == "COMPLETE"
                        && operation.kind == ProductOperationKind::Duplicate =>
                {
                    Some(self.product_operation_product_json(
                        operation.new_product_id.as_deref(),
                        &selection.selection,
                    ))
                }
                "newProduct" => Some(Value::Null),
                "userErrors" if status == "CREATED" => Some(Value::Array(Vec::new())),
                "userErrors" => {
                    selected_user_errors_field(operation.user_errors.as_slice(), selection)
                }
                _ => None,
            }
        })
    }

    pub(in crate::proxy) fn product_operation_json(
        &self,
        operation: &ProductOperationRecord,
        selections: &[SelectedField],
    ) -> Value {
        self.product_operation_json_with_status(operation, selections, "COMPLETE")
    }

    fn product_operation_product_json(
        &self,
        product_id: Option<&str>,
        selections: &[SelectedField],
    ) -> Value {
        product_id
            .and_then(|id| self.store.product_by_id(id))
            .map(|product| {
                self.product_json_with_variants_and_currency_context(
                    product,
                    &self.store.product_variants_for_product(&product.id),
                    selections,
                    &self.store.shop_currency_code(),
                )
            })
            .unwrap_or(Value::Null)
    }
}

pub(in crate::proxy) fn is_product_operation_gid(id: &str) -> bool {
    matches!(
        shopify_gid_resource_type(id),
        Some("ProductSetOperation" | "ProductDuplicateOperation" | "ProductBundleOperation")
    )
}

fn product_operation_typename(kind: ProductOperationKind) -> &'static str {
    match kind {
        ProductOperationKind::Set => "ProductSetOperation",
        ProductOperationKind::Duplicate => "ProductDuplicateOperation",
        ProductOperationKind::Bundle => "ProductBundleOperation",
    }
}

fn product_operation_selection_matches(selection: &SelectedField, typename: &str) -> bool {
    selection
        .type_condition
        .as_deref()
        .is_none_or(|condition| condition == typename || condition == "Node")
}

fn product_set_shape_error_response(
    response_key: &str,
    payload_selection: &[SelectedField],
    input: &BTreeMap<String, ResolvedValue>,
) -> Option<Response> {
    let variants = resolved_object_list_field(input, "variants");
    if variants.len() > 2048 {
        return Some(ok_json(json!({
            "errors": [max_input_size_exceeded_error(
                [response_key, "input", "variants"],
                variants.len(),
                2048,
                None
            )]
        })));
    }
    if let Some(quantities_len) = variants
        .iter()
        .map(|variant| resolved_object_list_field(variant, "inventoryQuantities").len())
        .find(|len| *len > 250)
    {
        return Some(ok_json(json!({
            "errors": [max_input_size_exceeded_error(
                [response_key, "input", "variants", "inventoryQuantities"],
                quantities_len,
                250,
                None
            )]
        })));
    }

    let mut errors = Vec::new();
    let product_options = resolved_object_list_field(input, "productOptions");
    if product_options.len() > 3 {
        errors.push(user_error(
            ["input", "productOptions"],
            "Options are limited to 3 per product",
            Some("INVALID_INPUT"),
        ));
    }
    for (index, option) in product_options.iter().enumerate() {
        if resolved_string_field(option, "name")
            .is_some_and(|name| product_option_name_has_title_delimiter(&name))
        {
            errors.push(user_error(
                json!(["input", "productOptions", index.to_string(), "name"]),
                PRODUCT_OPTION_NAME_DELIMITER_MESSAGE,
                Some("INVALID_INPUT"),
            ));
        }
    }
    if product_options
        .iter()
        .any(product_set_option_values_over_limit)
    {
        errors.push(user_error(
            ["input", "productOptions"],
            "Option values are limited to 100 per option",
            Some("INVALID_INPUT"),
        ));
    }
    if input.contains_key("productOptions") && !input.contains_key("variants") {
        errors.push(user_error_omit_code(
            ["input", "variants"],
            "Variants input is required when updating product options",
            None,
        ));
    }
    if resolved_object_list_field(input, "files").len() > 250 {
        errors.push(user_error(
            ["input", "files"],
            "Files are limited to 250 per product",
            Some("INVALID_INPUT"),
        ));
    }
    if errors.is_empty() {
        None
    } else {
        let payload = selected_payload_json(payload_selection, |selection| {
            match selection.name.as_str() {
                "product" | "productSetOperation" => Some(Value::Null),
                "userErrors" => selected_user_errors_field(errors.as_slice(), selection),
                _ => None,
            }
        });
        Some(ok_json(json!({ "data": { response_key: payload } })))
    }
}

fn product_set_option_values_over_limit(option: &BTreeMap<String, ResolvedValue>) -> bool {
    resolved_object_list_field(option, "values").len() > 100
        || resolved_object_list_field(option, "optionValues").len() > 100
}

fn product_set_variant_input_errors(input: &BTreeMap<String, ResolvedValue>) -> Vec<Value> {
    resolved_object_list_field(input, "variants")
        .iter()
        .enumerate()
        .flat_map(|(index, variant)| {
            product_variant_input_user_errors_with_prefix(
                variant,
                &[
                    "input".to_string(),
                    "variants".to_string(),
                    index.to_string(),
                ],
            )
        })
        .collect()
}

fn product_set_options_json(
    next_synthetic_id: &mut u64,
    input: &BTreeMap<String, ResolvedValue>,
) -> Value {
    Value::Array(
        resolved_object_list_field(input, "productOptions")
            .into_iter()
            .enumerate()
            .map(|(index, option)| {
                let name = resolved_string_field(&option, "name")
                    .unwrap_or_else(|| format!("Option{}", index + 1));
                let values = product_set_option_value_names(&option);
                let option_id = synthetic_shopify_gid("ProductOption", *next_synthetic_id);
                *next_synthetic_id += 1;
                let option_values = values
                    .iter()
                    .map(|value| {
                        let id = synthetic_shopify_gid("ProductOptionValue", *next_synthetic_id);
                        *next_synthetic_id += 1;
                        json!({
                            "id": id,
                            "name": value,
                            "hasVariants": true
                        })
                    })
                    .collect::<Vec<_>>();
                json!({
                    "id": option_id,
                    "name": name,
                    "position": index + 1,
                    "values": values,
                    "optionValues": option_values
                })
            })
            .collect(),
    )
}

fn product_set_option_value_names(option: &BTreeMap<String, ResolvedValue>) -> Vec<String> {
    let from_values = resolved_object_list_field(option, "values")
        .into_iter()
        .filter_map(|value| resolved_string_field(&value, "name"))
        .collect::<Vec<_>>();
    if !from_values.is_empty() {
        return from_values;
    }
    resolved_object_list_field(option, "optionValues")
        .into_iter()
        .filter_map(|value| resolved_string_field(&value, "name"))
        .collect()
}

fn apply_inventory_quantities_to_variant(
    variant: &mut ProductVariantRecord,
    input: &BTreeMap<String, ResolvedValue>,
) {
    let quantities = resolved_object_list_field(input, "inventoryQuantities");
    if quantities.is_empty() {
        return;
    }
    variant.inventory_quantity = quantities
        .iter()
        .filter(|quantity| {
            resolved_string_field(quantity, "name")
                .map(|name| name == "available")
                .unwrap_or(true)
        })
        .filter_map(|quantity| resolved_int_field(quantity, "quantity"))
        .sum();
}

fn product_set_variant_selected_options(
    input: &BTreeMap<String, ResolvedValue>,
) -> Vec<ProductVariantSelectedOption> {
    resolved_object_list_field(input, "optionValues")
        .into_iter()
        .filter_map(|option_value| {
            Some(ProductVariantSelectedOption {
                name: resolved_string_field(&option_value, "optionName")
                    .or_else(|| resolved_string_field(&option_value, "name"))?,
                value: resolved_string_field(&option_value, "name")
                    .or_else(|| resolved_string_field(&option_value, "value"))?,
            })
        })
        .collect()
}

/// Option-value signature used to match `productSet` input variants to existing
/// variants and detect repeated input combinations.
fn product_set_selected_option_signature(options: &[ProductVariantSelectedOption]) -> String {
    let mut pairs = options
        .iter()
        .map(|option| (option.name.clone(), option.value.clone()))
        .collect::<Vec<_>>();
    pairs.sort();
    pairs
        .into_iter()
        .map(|(name, value)| format!("{name}\u{1}{value}"))
        .collect::<Vec<_>>()
        .join("\u{2}")
}

fn product_set_selected_option_title(options: &[ProductVariantSelectedOption]) -> String {
    options
        .iter()
        .map(|option| option.value.as_str())
        .collect::<Vec<_>>()
        .join(" / ")
}

/// Detect `productSet` input variants whose option-value combination repeats an earlier
/// input variant. Returns one userError per later collision (the first occurrence is
/// accepted), anchored at `["input", "variants", "<index>"]`.
fn product_set_duplicate_variant_errors(input: &BTreeMap<String, ResolvedValue>) -> Vec<Value> {
    let variants = resolved_object_list_field(input, "variants");
    let mut seen = BTreeSet::new();
    let mut errors = Vec::new();
    for (index, variant) in variants.iter().enumerate() {
        let selected_options = product_set_variant_selected_options(variant);
        let signature = product_set_selected_option_signature(&selected_options);
        if signature.is_empty() {
            continue;
        }
        if !seen.insert(signature) {
            let title = product_set_selected_option_title(&selected_options);
            errors.push(user_error_omit_code(
                vec![
                    "input".to_string(),
                    "variants".to_string(),
                    index.to_string(),
                ],
                &format!(
                    "The variant '{title}' already exists. Please change at least one option value."
                ),
                None,
            ));
        }
    }
    errors
}

fn apply_product_set_option_values_to_variant(
    variant: &mut ProductVariantRecord,
    selected_options: Vec<ProductVariantSelectedOption>,
) {
    if selected_options.is_empty() {
        return;
    }
    variant.title = product_set_selected_option_title(&selected_options);
    variant.selected_options = selected_options;
}

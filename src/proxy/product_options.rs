use super::*;

const PRODUCT_OPTION_LIMIT: usize = 3;
const PRODUCT_OPTION_VALUE_LIMIT: usize = 100;
const PRODUCT_OPTION_NAME_LIMIT: usize = 255;
const PRODUCT_VARIANT_LIMIT: usize = 2048;

#[derive(Clone)]
struct ProductOptionGraph {
    options: Vec<ProductOptionNode>,
}

#[derive(Clone)]
struct ProductOptionNode {
    id: String,
    name: String,
    position: usize,
    values: Vec<ProductOptionValueNode>,
    extra_fields: BTreeMap<String, Value>,
}

#[derive(Clone)]
struct ProductOptionValueNode {
    id: String,
    name: String,
    has_variants: bool,
    extra_fields: BTreeMap<String, Value>,
}

#[derive(Clone)]
struct ProductOptionUserError {
    field: Value,
    message: String,
    code: Option<String>,
}

impl DraftProxy {
    pub(in crate::proxy) fn product_option_mutation(
        &mut self,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        match root_field {
            "productOptionsCreate" => self.product_options_create(query, variables),
            "productOptionUpdate" => self.product_option_update(query, variables),
            "productOptionsDelete" => self.product_options_delete(query, variables),
            "productOptionsReorder" => self.product_options_reorder(query, variables),
            _ => MutationOutcome::response(json_error(
                400,
                "No mutation dispatcher implemented for product option root",
            )),
        }
    }

    fn load_product_option_owner<F>(
        &mut self,
        product_id: &str,
        missing_response: F,
    ) -> Result<ProductRecord, MutationOutcome>
    where
        F: FnOnce(&Self) -> Response,
    {
        self.hydrate_product_option_owner_state(product_id);
        self.store
            .product_staged_or_base(product_id)
            .ok_or_else(|| MutationOutcome::response(missing_response(self)))
    }

    fn product_options_create(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let product_id = resolved_string_field(variables, "productId").unwrap_or_default();
        let mut product = match self.load_product_option_owner(&product_id, |proxy| {
            proxy.product_option_payload_response(
                query,
                "productOptionsCreate",
                None,
                None,
                Vec::new(),
                vec![product_option_missing_product_error()],
            )
        }) {
            Ok(product) => product,
            Err(outcome) => return outcome,
        };

        let input_options = resolved_object_list_field(variables, "options");
        let mut graph = product_option_graph_from_product(&product);
        let errors = validate_product_options_create(&graph, &input_options);
        if !errors.is_empty() {
            return MutationOutcome::response(self.product_option_payload_response(
                query,
                "productOptionsCreate",
                Some(&product),
                None,
                Vec::new(),
                errors,
            ));
        }
        // Adding options with the CREATE strategy multiplies the product's
        // existing variants by every new option-value combination. Shopify caps
        // a product at 2048 variants and rejects the mutation when the resulting
        // count would exceed it.
        if resolved_variant_strategy(variables).as_deref() == Some("CREATE") {
            let existing_variants = self
                .store
                .product_variants_for_product(&product_id)
                .len()
                .max(1);
            let new_value_product: usize = input_options
                .iter()
                .map(|option| option_input_value_names(option).len().max(1))
                .product();
            if existing_variants.saturating_mul(new_value_product) > PRODUCT_VARIANT_LIMIT {
                return MutationOutcome::response(self.product_option_payload_response(
                    query,
                    "productOptionsCreate",
                    Some(&product),
                    None,
                    Vec::new(),
                    vec![ProductOptionUserError::new(
                        json!(["options"]),
                        "The number of created variants would exceed the 2048 variants per product limit",
                        Some("TOO_MANY_VARIANTS_CREATED"),
                    )],
                ));
            }
        }

        self.record_product_option_linked_metaobject_definitions(&input_options);

        let default_only = graph.is_default_only();
        if default_only {
            graph.options.clear();
        }

        let mut created_option_ids = Vec::new();
        for (input_index, input) in input_options.iter().enumerate() {
            let option_id = self.next_proxy_synthetic_gid("ProductOption");
            created_option_ids.push(option_id.clone());
            let mut values = option_input_value_nodes(input, self);
            if resolved_variant_strategy(variables).as_deref() == Some("CREATE") {
                for value in &mut values {
                    value.has_variants = true;
                }
            } else if default_only {
                if let Some(first_value) = values.first_mut() {
                    first_value.has_variants = true;
                }
            }
            graph.options.push(ProductOptionNode {
                id: option_id,
                name: resolved_string_field(input, "name").unwrap_or_default(),
                position: resolved_int_field(input, "position")
                    .and_then(|position| (position > 0).then_some(position as usize))
                    .unwrap_or(graph.options.len() + input_index + 1),
                values,
                extra_fields: option_extra_fields(input),
            });
        }
        graph.normalize_positions();

        let variants = self.store.product_variants_for_product(&product.id);
        let staged_variants = if resolved_variant_strategy(variables).as_deref() == Some("CREATE") {
            self.product_options_create_variants(&product, &graph, &variants, default_only)
        } else {
            self.product_options_apply_first_values(&product, &graph, &variants, default_only)
        };
        for variant in &staged_variants {
            self.store.stage_product_variant(variant.clone());
        }

        product = product_with_option_graph(product, &graph);
        self.store.stage_product(product.clone());

        MutationOutcome::staged(
            self.product_option_payload_response(
                query,
                "productOptionsCreate",
                Some(&product),
                Some(&graph),
                staged_variants,
                errors,
            ),
            LogDraft::staged(
                "productOptionsCreate",
                "products",
                std::iter::once(product_id)
                    .chain(created_option_ids)
                    .collect::<Vec<_>>(),
            ),
        )
    }

    fn product_options_create_variants(
        &mut self,
        product: &ProductRecord,
        graph: &ProductOptionGraph,
        variants: &[ProductVariantRecord],
        default_only: bool,
    ) -> Vec<ProductVariantRecord> {
        if graph.options.is_empty() {
            return Vec::new();
        }
        let combinations = option_value_combinations(graph);
        if combinations.is_empty() {
            return Vec::new();
        }

        let mut staged = Vec::new();
        let mut used_combinations = BTreeSet::new();
        for variant in variants {
            let mut variant = variant.clone();
            let mut selected_options = Vec::new();
            for option in &graph.options {
                let selected_value = variant
                    .selected_options
                    .iter()
                    .find(|selected| selected.name == option.name)
                    .and_then(|selected| {
                        option
                            .values
                            .iter()
                            .find(|value| value.name == selected.value)
                            .map(|value| value.name.clone())
                    })
                    .or_else(|| option.values.first().map(|value| value.name.clone()));
                if let Some(value) = selected_value {
                    selected_options.push(ProductVariantSelectedOption {
                        name: option.name.clone(),
                        value,
                    });
                }
            }
            let key = selected_options
                .iter()
                .map(|selected| (selected.name.clone(), selected.value.clone()))
                .collect::<Vec<_>>();
            used_combinations.insert(key);
            variant.selected_options = selected_options;
            variant.title = variant_title(&variant.selected_options);
            if default_only {
                variant.extra_fields.remove("legacyDefaultTitle");
            }
            staged.push(variant);
        }

        for combination in combinations {
            if used_combinations.contains(&combination) {
                continue;
            }
            let mut variant = {
                let id = self.next_proxy_synthetic_gid("ProductVariant");
                let inventory_item_id = self.next_proxy_synthetic_gid("InventoryItem");
                empty_product_variant_record(product.id.clone(), id, inventory_item_id)
            };
            variant.selected_options = combination
                .iter()
                .map(|(option_name, value_name)| ProductVariantSelectedOption {
                    name: option_name.clone(),
                    value: value_name.clone(),
                })
                .collect();
            variant.title = variant_title(&variant.selected_options);
            variant.extra_fields.insert("sku".to_string(), Value::Null);
            staged.push(variant);
        }
        staged
    }

    fn product_options_apply_first_values(
        &mut self,
        product: &ProductRecord,
        graph: &ProductOptionGraph,
        variants: &[ProductVariantRecord],
        default_only: bool,
    ) -> Vec<ProductVariantRecord> {
        if graph.options.is_empty() {
            return Vec::new();
        }
        let selected_options = graph
            .options
            .iter()
            .filter_map(|option| {
                let value = option.values.first()?;
                Some(ProductVariantSelectedOption {
                    name: option.name.clone(),
                    value: value.name.clone(),
                })
            })
            .collect::<Vec<_>>();
        if selected_options.is_empty() {
            return Vec::new();
        }
        if let Some(existing) = variants.first().cloned() {
            let mut variant = existing;
            variant.selected_options = selected_options;
            variant.title = variant_title(&variant.selected_options);
            return vec![variant];
        }
        if default_only {
            let id = self.next_proxy_synthetic_gid("ProductVariant");
            let inventory_item_id = self.next_proxy_synthetic_gid("InventoryItem");
            let mut variant =
                empty_product_variant_record(product.id.clone(), id, inventory_item_id);
            variant.selected_options = selected_options;
            variant.title = variant_title(&variant.selected_options);
            return vec![variant];
        }
        Vec::new()
    }

    fn product_option_update(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let product_id = resolved_string_field(variables, "productId").unwrap_or_default();
        let mut product = match self.load_product_option_owner(&product_id, |proxy| {
            proxy.product_option_payload_response(
                query,
                "productOptionUpdate",
                None,
                None,
                Vec::new(),
                vec![product_option_missing_product_error()],
            )
        }) {
            Ok(product) => product,
            Err(outcome) => return outcome,
        };
        let mut graph = product_option_graph_from_product(&product);
        let option_input = resolved_object_field(variables, "option").unwrap_or_default();
        let Some(option_index) = option_input
            .get("id")
            .and_then(resolved_value_string)
            .and_then(|id| graph.option_index_by_id(&id))
        else {
            return MutationOutcome::response(self.product_option_payload_response(
                query,
                "productOptionUpdate",
                Some(&product),
                Some(&graph),
                self.store.product_variants_for_product(&product.id),
                vec![ProductOptionUserError::new(
                    json!(["option"]),
                    "Option does not exist",
                    None::<&str>,
                )],
            ));
        };

        let errors = validate_product_option_update(&graph, option_index, variables, &option_input);
        if !errors.is_empty() {
            return MutationOutcome::response(self.product_option_payload_response(
                query,
                "productOptionUpdate",
                Some(&product),
                Some(&graph),
                self.store.product_variants_for_product(&product.id),
                errors,
            ));
        }

        let updated_option_id = graph.options[option_index].id.clone();
        let old_option_name = graph.options[option_index].name.clone();
        if let Some(name) = resolved_string_field(&option_input, "name") {
            graph.options[option_index].name = name;
        }
        if let Some(position) = resolved_int_field(&option_input, "position") {
            if position > 0 {
                graph.options[option_index].position = position as usize;
            }
        }

        let mut renamed_values = BTreeMap::new();
        let delete_ids = resolved_string_list_arg(variables, "optionValuesToDelete");
        graph.options[option_index]
            .values
            .retain(|value| !delete_ids.iter().any(|id| id == &value.id));
        for update in resolved_object_list_field(variables, "optionValuesToUpdate") {
            let Some(value_id) = resolved_string_field(&update, "id") else {
                continue;
            };
            if let Some(value) = graph.options[option_index]
                .values
                .iter_mut()
                .find(|value| value.id == value_id)
            {
                if let Some(name) = resolved_string_field(&update, "name") {
                    renamed_values.insert(value.name.clone(), name.clone());
                    value.name = name;
                }
            }
        }
        for add in resolved_object_list_field(variables, "optionValuesToAdd") {
            let name = resolved_string_field(&add, "name").unwrap_or_default();
            graph.options[option_index]
                .values
                .push(ProductOptionValueNode {
                    id: self.next_proxy_synthetic_gid("ProductOptionValue"),
                    name,
                    has_variants: false,
                    extra_fields: option_value_extra_fields(&add, None),
                });
        }
        graph.reposition(option_index);
        graph.normalize_positions();

        let mut variants = self.store.product_variants_for_product(&product.id);
        remap_variants_for_option_update(
            &mut variants,
            &updated_option_id,
            &old_option_name,
            &graph.options,
            &renamed_values,
        );
        remap_variants_to_graph(&mut variants, &graph);
        for variant in &variants {
            self.store.stage_product_variant(variant.clone());
        }
        mark_variant_backed_values(&mut graph, &variants);

        product = product_with_option_graph(product, &graph);
        self.store.stage_product(product.clone());
        MutationOutcome::staged(
            self.product_option_payload_response(
                query,
                "productOptionUpdate",
                Some(&product),
                Some(&graph),
                variants,
                Vec::new(),
            ),
            LogDraft::staged("productOptionUpdate", "products", vec![product_id]),
        )
    }

    fn product_options_delete(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let product_id = resolved_string_field(variables, "productId").unwrap_or_default();
        let mut product = match self.load_product_option_owner(&product_id, |proxy| {
            proxy.product_options_delete_response(
                query,
                None,
                None,
                Vec::new(),
                Vec::new(),
                vec![product_option_missing_product_error()],
            )
        }) {
            Ok(product) => product,
            Err(outcome) => return outcome,
        };
        let mut graph = product_option_graph_from_product(&product);
        let option_ids = resolved_string_list_arg(variables, "options");
        let mut errors = Vec::new();
        for (index, option_id) in option_ids.iter().enumerate() {
            if graph.option_index_by_id(option_id).is_none() {
                errors.push(ProductOptionUserError::new(
                    json!(["options", index.to_string()]),
                    "Option does not exist",
                    None::<&str>,
                ));
            }
        }
        if !errors.is_empty() {
            return MutationOutcome::response(self.product_options_delete_response(
                query,
                Some(&product),
                Some(&graph),
                self.store.product_variants_for_product(&product.id),
                Vec::new(),
                errors,
            ));
        }

        graph
            .options
            .retain(|option| !option_ids.iter().any(|id| id == &option.id));
        if graph.options.is_empty() {
            graph = default_title_graph(self);
        }
        graph.normalize_positions();

        let mut variants = self.store.product_variants_for_product(&product.id);
        remap_variants_to_graph(&mut variants, &graph);
        for variant in &variants {
            self.store.stage_product_variant(variant.clone());
        }
        mark_variant_backed_values(&mut graph, &variants);
        product = product_with_option_graph(product, &graph);
        self.store.stage_product(product.clone());

        MutationOutcome::staged(
            self.product_options_delete_response(
                query,
                Some(&product),
                Some(&graph),
                variants,
                option_ids.clone(),
                Vec::new(),
            ),
            LogDraft::staged(
                "productOptionsDelete",
                "products",
                std::iter::once(product_id)
                    .chain(option_ids)
                    .collect::<Vec<_>>(),
            ),
        )
    }

    fn product_options_reorder(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let product_id = resolved_string_field(variables, "productId").unwrap_or_default();
        let mut product = match self.load_product_option_owner(&product_id, |proxy| {
            proxy.product_option_payload_response(
                query,
                "productOptionsReorder",
                None,
                None,
                Vec::new(),
                vec![product_option_missing_product_error()],
            )
        }) {
            Ok(product) => product,
            Err(outcome) => return outcome,
        };
        let graph = product_option_graph_from_product(&product);
        let reorder_inputs = resolved_object_list_field(variables, "options");
        let errors = validate_product_options_reorder(&graph, &reorder_inputs);
        if !errors.is_empty() {
            return MutationOutcome::response(self.product_option_payload_response(
                query,
                "productOptionsReorder",
                Some(&product),
                Some(&graph),
                self.store.product_variants_for_product(&product.id),
                errors,
            ));
        }

        let mut reordered = reorder_product_option_graph(&graph, &reorder_inputs);
        let mut variants = self.store.product_variants_for_product(&product.id);
        remap_variants_to_graph(&mut variants, &reordered);
        variants.sort_by_key(|variant| variant_sort_key(variant, &reordered));
        for variant in &variants {
            self.store.stage_product_variant(variant.clone());
        }
        // Persist the new variant ordering so subsequent reads reflect it; staging an
        // already-present variant does not move it in the staged order vector.
        let ordered_variant_ids: Vec<String> =
            variants.iter().map(|variant| variant.id.clone()).collect();
        self.store
            .reorder_product_variants(&product.id, &ordered_variant_ids);
        mark_variant_backed_values(&mut reordered, &variants);
        partition_variant_backed_values_first(&mut reordered);
        product = product_with_option_graph(product, &reordered);
        self.store.stage_product(product.clone());

        MutationOutcome::staged(
            self.product_option_payload_response(
                query,
                "productOptionsReorder",
                Some(&product),
                Some(&reordered),
                variants,
                Vec::new(),
            ),
            LogDraft::staged("productOptionsReorder", "products", vec![product_id]),
        )
    }

    fn hydrate_product_option_owner_state(&mut self, product_id: &str) {
        if self.config.read_mode != ReadMode::LiveHybrid
            || product_id.is_empty()
            || self.store.product_by_id(product_id).is_some()
        {
            return;
        }
        // The reorder graph is derived from the product's `options` field, which the
        // generic node-observation query does not select. Forward the options-aware
        // hydrate so the option/optionValue graph is observed into local state.
        self.hydrate_product_options_owner(product_id);
    }

    fn record_product_option_linked_metaobject_definitions(
        &mut self,
        options: &[BTreeMap<String, ResolvedValue>],
    ) {
        for option in options {
            let Some(linked_metafield) = resolved_object_field(option, "linkedMetafield") else {
                continue;
            };
            // The `values` of a linked-metafield option are the metaobject entry
            // gids surfaced as the option's value list. Record the entries that
            // share a single option together so a later metaobjectUpdate/Upsert
            // that renames one entry to collide with a sibling's display name can
            // be rejected with DISPLAY_NAME_CONFLICT (Shopify forbids two linked
            // option values from resolving to the same display name).
            let linked_entry_ids = resolved_string_list_field_unsorted(&linked_metafield, "values")
                .into_iter()
                .collect::<BTreeSet<String>>();
            if linked_entry_ids.len() >= 2 {
                self.store
                    .staged
                    .linked_product_option_metaobject_sets
                    .push(linked_entry_ids);
            }
            let namespace =
                resolved_string_field(&linked_metafield, "namespace").unwrap_or_default();
            let key = resolved_string_field(&linked_metafield, "key").unwrap_or_default();
            let Some(definition) = self
                .store
                .staged
                .metafield_definitions
                .get(&metafield_definition_store_key("PRODUCT", &namespace, &key))
            else {
                continue;
            };
            let definition_id = definition["validations"]
                .as_array()
                .into_iter()
                .flatten()
                .find_map(|validation| {
                    (validation.get("name").and_then(Value::as_str)
                        == Some("metaobject_definition_id"))
                    .then(|| {
                        validation
                            .get("value")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                    })
                    .flatten()
                });
            if let Some(definition_id) = definition_id {
                self.store
                    .staged
                    .product_option_linked_metaobject_definition_ids
                    .insert(definition_id);
            }
        }
    }

    fn product_option_payload_response(
        &self,
        query: &str,
        root_field: &str,
        product: Option<&ProductRecord>,
        graph: Option<&ProductOptionGraph>,
        variants: Vec<ProductVariantRecord>,
        user_errors: Vec<ProductOptionUserError>,
    ) -> Response {
        let (response_key, payload_selection) = primary_root_field(query, &BTreeMap::new())
            .map(|field| (field.response_key, field.selection))
            .unwrap_or_else(|| (root_field.to_string(), Vec::new()));
        let payload = product_option_payload_json(
            &payload_selection,
            product,
            graph,
            variants,
            &self.store.shop_currency_code(),
            &user_errors,
        );
        ok_json(json!({ "data": { response_key: payload } }))
    }

    fn product_options_delete_response(
        &self,
        query: &str,
        product: Option<&ProductRecord>,
        graph: Option<&ProductOptionGraph>,
        variants: Vec<ProductVariantRecord>,
        deleted_options_ids: Vec<String>,
        user_errors: Vec<ProductOptionUserError>,
    ) -> Response {
        let (response_key, payload_selection) = primary_root_field(query, &BTreeMap::new())
            .map(|field| (field.response_key, field.selection))
            .unwrap_or_else(|| ("productOptionsDelete".to_string(), Vec::new()));
        let product_selection =
            selected_child_selection(&payload_selection, "product").unwrap_or_default();
        let error_selection =
            selected_child_selection(&payload_selection, "userErrors").unwrap_or_default();
        let payload = selected_payload_json(&payload_selection, |selection| {
            match selection.name.as_str() {
                "deletedOptionsIds" => Some(json!(deleted_options_ids)),
                "product" => Some(product_option_product_json(
                    product,
                    graph,
                    variants.clone(),
                    &product_selection,
                    &self.store.shop_currency_code(),
                )),
                "userErrors" => Some(Value::Array(
                    user_errors
                        .iter()
                        .map(|error| selected_json(&error.to_json(), &error_selection))
                        .collect(),
                )),
                _ => None,
            }
        });
        ok_json(json!({ "data": { response_key: payload } }))
    }
}

impl ProductOptionGraph {
    fn is_default_only(&self) -> bool {
        self.options.len() == 1
            && self.options[0].name == "Title"
            && self.options[0]
                .values
                .first()
                .is_some_and(|value| value.name == "Default Title")
    }

    fn normalize_positions(&mut self) {
        self.options.sort_by_key(|option| option.position);
        for (index, option) in self.options.iter_mut().enumerate() {
            option.position = index + 1;
        }
    }

    fn reposition(&mut self, option_index: usize) {
        if option_index >= self.options.len() {
            return;
        }
        let option = self.options.remove(option_index);
        let target = option.position.saturating_sub(1).min(self.options.len());
        self.options.insert(target, option);
    }

    fn option_index_by_id(&self, id: &str) -> Option<usize> {
        self.options.iter().position(|option| option.id == id)
    }

    fn option_index_by_name(&self, name: &str) -> Option<usize> {
        self.options.iter().position(|option| option.name == name)
    }
}

impl ProductOptionUserError {
    fn new(
        message_field: Value,
        message: impl Into<String>,
        code: Option<impl Into<String>>,
    ) -> Self {
        Self {
            field: message_field,
            message: message.into(),
            code: code.map(Into::into),
        }
    }

    fn to_json(&self) -> Value {
        let mut value = json!({
            "field": self.field,
            "message": self.message,
        });
        if let Some(code) = &self.code {
            value["code"] = json!(code);
        }
        value
    }
}

fn product_option_missing_product_error() -> ProductOptionUserError {
    ProductOptionUserError::new(
        json!(["productId"]),
        "Product does not exist",
        Some("PRODUCT_DOES_NOT_EXIST"),
    )
}

fn product_option_graph_from_product(product: &ProductRecord) -> ProductOptionGraph {
    let options = product
        .extra_fields
        .get("options")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(product_option_node_from_json)
        .collect::<Vec<_>>();
    ProductOptionGraph { options }
}

fn product_option_node_from_json(value: &Value) -> Option<ProductOptionNode> {
    let id = value.get("id")?.as_str()?.to_string();
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let position = value
        .get("position")
        .and_then(Value::as_u64)
        .map(|position| position as usize)
        .unwrap_or(1);
    let values = value
        .get("optionValues")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(product_option_value_node_from_json)
        .collect::<Vec<_>>();
    let mut extra_fields = BTreeMap::new();
    if let Some(object) = value.as_object() {
        for (key, value) in object {
            if !matches!(
                key.as_str(),
                "id" | "name" | "position" | "values" | "optionValues"
            ) {
                extra_fields.insert(key.clone(), value.clone());
            }
        }
    }
    Some(ProductOptionNode {
        id,
        name,
        position,
        values,
        extra_fields,
    })
}

fn product_option_value_node_from_json(value: &Value) -> Option<ProductOptionValueNode> {
    let id = value.get("id")?.as_str()?.to_string();
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let has_variants = value
        .get("hasVariants")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut extra_fields = BTreeMap::new();
    if let Some(object) = value.as_object() {
        for (key, value) in object {
            if !matches!(key.as_str(), "id" | "name" | "hasVariants") {
                extra_fields.insert(key.clone(), value.clone());
            }
        }
    }
    Some(ProductOptionValueNode {
        id,
        name,
        has_variants,
        extra_fields,
    })
}

fn option_input_value_nodes(
    input: &BTreeMap<String, ResolvedValue>,
    proxy: &mut DraftProxy,
) -> Vec<ProductOptionValueNode> {
    let mut values = option_input_values(input);
    if values.is_empty() {
        if let Some(linked_metafield) = resolved_object_field(input, "linkedMetafield") {
            values = resolved_list_arg(&linked_metafield, "values");
        }
    }
    let has_linked_metafield = resolved_object_field(input, "linkedMetafield").is_some();
    values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let linked_value = match value {
                ResolvedValue::Object(object) => {
                    resolved_string_field(object, "linkedMetafieldValue")
                }
                ResolvedValue::String(value) if has_linked_metafield => Some(value.clone()),
                _ => None,
            };
            // A linked option value's name mirrors the referenced metaobject's display
            // name (e.g. "One"/"Two"), not the raw gid; fall back to the gid if the
            // referenced entry isn't staged locally.
            let name = match value {
                ResolvedValue::Object(object) => resolved_string_field(object, "name")
                    .or_else(|| {
                        resolved_string_field(object, "linkedMetafieldValue")
                            .map(|gid| proxy.linked_metaobject_display_name(&gid).unwrap_or(gid))
                    })
                    .unwrap_or_default(),
                ResolvedValue::String(value) if has_linked_metafield => proxy
                    .linked_metaobject_display_name(value)
                    .unwrap_or_else(|| value.clone()),
                ResolvedValue::String(value) => value.clone(),
                _ => String::new(),
            };
            ProductOptionValueNode {
                id: proxy.next_proxy_synthetic_gid("ProductOptionValue"),
                name,
                has_variants: index == 0,
                extra_fields: option_value_extra_fields_from_value(value, linked_value),
            }
        })
        .collect()
}

fn option_input_values(input: &BTreeMap<String, ResolvedValue>) -> Vec<ResolvedValue> {
    resolved_list_arg(input, "values")
}

fn option_input_value_names(input: &BTreeMap<String, ResolvedValue>) -> Vec<String> {
    let mut values = option_input_values(input);
    if values.is_empty() {
        if let Some(linked_metafield) = resolved_object_field(input, "linkedMetafield") {
            values = resolved_list_arg(&linked_metafield, "values");
        }
    }
    values
        .iter()
        .filter_map(|value| match value {
            ResolvedValue::Object(object) => resolved_string_field(object, "name")
                .or_else(|| resolved_string_field(object, "linkedMetafieldValue")),
            ResolvedValue::String(value) => Some(value.clone()),
            _ => None,
        })
        .collect()
}

fn option_extra_fields(input: &BTreeMap<String, ResolvedValue>) -> BTreeMap<String, Value> {
    let mut fields = BTreeMap::new();
    if let Some(linked_metafield) = resolved_object_field(input, "linkedMetafield") {
        fields.insert(
            "linkedMetafield".to_string(),
            resolved_object_json(&linked_metafield),
        );
    }
    fields
}

fn option_value_extra_fields(
    input: &BTreeMap<String, ResolvedValue>,
    linked_value: Option<String>,
) -> BTreeMap<String, Value> {
    let mut fields = BTreeMap::new();
    if let Some(value) =
        linked_value.or_else(|| resolved_string_field(input, "linkedMetafieldValue"))
    {
        fields.insert("linkedMetafieldValue".to_string(), json!(value));
    }
    fields
}

fn option_value_extra_fields_from_value(
    value: &ResolvedValue,
    linked_value: Option<String>,
) -> BTreeMap<String, Value> {
    match value {
        ResolvedValue::Object(object) => option_value_extra_fields(object, linked_value),
        _ => {
            let mut fields = BTreeMap::new();
            if let Some(linked_value) = linked_value {
                fields.insert("linkedMetafieldValue".to_string(), json!(linked_value));
            }
            fields
        }
    }
}

fn validate_product_options_create(
    graph: &ProductOptionGraph,
    input_options: &[BTreeMap<String, ResolvedValue>],
) -> Vec<ProductOptionUserError> {
    let mut errors = Vec::new();
    let mut seen_names = BTreeSet::new();
    let existing_names = graph
        .options
        .iter()
        .map(|option| option.name.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    for (index, input) in input_options.iter().enumerate() {
        let name = resolved_string_field(input, "name").unwrap_or_default();
        if name.trim().is_empty() {
            errors.push(ProductOptionUserError::new(
                json!(["options", index.to_string()]),
                "Option name can't be blank.",
                Some("INVALID_NAME"),
            ));
        } else if name.chars().count() > PRODUCT_OPTION_NAME_LIMIT {
            errors.push(ProductOptionUserError::new(
                json!(["options", index.to_string()]),
                "Option name is too long.",
                Some("OPTION_NAME_TOO_LONG"),
            ));
        }
        let normalized_name = name.to_ascii_lowercase();
        if !seen_names.insert(normalized_name.clone()) {
            errors.push(ProductOptionUserError::new(
                json!(["options", index.to_string()]),
                "Duplicated option name.",
                Some("DUPLICATED_OPTION_NAME"),
            ));
        } else if existing_names.contains(&normalized_name) && !graph.is_default_only() {
            errors.push(ProductOptionUserError::new(
                json!(["options", index.to_string()]),
                format!("Option '{name}' already exists."),
                Some("OPTION_ALREADY_EXISTS"),
            ));
        }

        let value_names = option_input_value_names(input);
        if value_names.is_empty() && resolved_object_field(input, "linkedMetafield").is_none() {
            errors.push(ProductOptionUserError::new(
                json!(["options", index.to_string()]),
                format!("Option '{name}' must specify at least one option value."),
                Some("OPTION_VALUES_MISSING"),
            ));
        }
        let mut seen_values = BTreeSet::new();
        for value_name in &value_names {
            if value_name.chars().count() > PRODUCT_OPTION_NAME_LIMIT {
                errors.push(ProductOptionUserError::new(
                    json!(["options", index.to_string(), "values"]),
                    "Option value name is too long.",
                    Some("OPTION_VALUES_NAME_TOO_LONG"),
                ));
            }
            if !seen_values.insert(value_name.to_ascii_lowercase()) {
                errors.push(ProductOptionUserError::new(
                    json!(["options", index.to_string(), "values"]),
                    "Duplicated option value.",
                    Some("DUPLICATED_OPTION_VALUE"),
                ));
            }
        }
        if value_names.len() > PRODUCT_OPTION_VALUE_LIMIT {
            errors.push(ProductOptionUserError::new(
                json!(["options", index.to_string(), "values"]),
                "Can only specify a maximum of 100 values per option",
                Some("OPTION_VALUES_OVER_LIMIT"),
            ));
        }
    }
    let effective_existing = if graph.is_default_only() {
        0
    } else {
        graph.options.len()
    };
    if effective_existing + input_options.len() > PRODUCT_OPTION_LIMIT {
        errors.push(ProductOptionUserError::new(
            json!(["options"]),
            "Can only specify a maximum of 3 options",
            Some("OPTIONS_OVER_LIMIT"),
        ));
    }
    errors
}

fn validate_product_option_update(
    graph: &ProductOptionGraph,
    option_index: usize,
    variables: &BTreeMap<String, ResolvedValue>,
    option_input: &BTreeMap<String, ResolvedValue>,
) -> Vec<ProductOptionUserError> {
    let mut errors = Vec::new();
    if let Some(name) = resolved_string_field(option_input, "name") {
        if name.trim().is_empty() {
            errors.push(ProductOptionUserError::new(
                json!(["option", "name"]),
                "Option name can't be blank.",
                Some("INVALID_NAME"),
            ));
        } else if name.chars().count() > PRODUCT_OPTION_NAME_LIMIT {
            errors.push(ProductOptionUserError::new(
                json!(["option", "name"]),
                "Option name is too long.",
                Some("OPTION_NAME_TOO_LONG"),
            ));
        }
        if graph
            .options
            .iter()
            .enumerate()
            .any(|(index, option)| index != option_index && option.name.eq_ignore_ascii_case(&name))
        {
            errors.push(ProductOptionUserError::new(
                json!(["option", "name"]),
                "Duplicated option name.",
                Some("DUPLICATED_OPTION_NAME"),
            ));
        }
    }

    let option = &graph.options[option_index];
    for id in resolved_string_list_arg(variables, "optionValuesToDelete") {
        if !option.values.iter().any(|value| value.id == id) {
            errors.push(ProductOptionUserError::new(
                json!(["optionValuesToDelete"]),
                "Option value does not exist",
                None::<&str>,
            ));
        }
    }
    for update in resolved_object_list_field(variables, "optionValuesToUpdate") {
        let id = resolved_string_field(&update, "id").unwrap_or_default();
        if !option.values.iter().any(|value| value.id == id) {
            errors.push(ProductOptionUserError::new(
                json!(["optionValuesToUpdate"]),
                "Option value does not exist",
                None::<&str>,
            ));
        }
        if resolved_string_field(&update, "name").is_some_and(|name| {
            name.trim().is_empty() || name.chars().count() > PRODUCT_OPTION_NAME_LIMIT
        }) {
            errors.push(ProductOptionUserError::new(
                json!(["optionValuesToUpdate"]),
                "Option value name is invalid",
                Some("INVALID_NAME"),
            ));
        }
    }
    let add_count = resolved_object_list_field(variables, "optionValuesToAdd").len();
    let delete_count = resolved_string_list_arg(variables, "optionValuesToDelete").len();
    if option.values.len() + add_count > delete_count + PRODUCT_OPTION_VALUE_LIMIT {
        errors.push(ProductOptionUserError::new(
            json!(["optionValuesToAdd"]),
            "Can only specify a maximum of 100 values per option",
            Some("OPTION_VALUES_OVER_LIMIT"),
        ));
    }
    errors
}

fn validate_product_options_reorder(
    graph: &ProductOptionGraph,
    inputs: &[BTreeMap<String, ResolvedValue>],
) -> Vec<ProductOptionUserError> {
    let mut errors = Vec::new();

    // Shopify rejects mixing `id` and `name` selector keys wholesale, before any
    // per-element existence or duplicate checks. Options are validated first, then
    // values, each producing a single atomic error.
    let option_has_id = inputs
        .iter()
        .any(|input| resolved_string_field(input, "id").is_some());
    let option_has_name = inputs
        .iter()
        .any(|input| resolved_string_field(input, "name").is_some());
    if option_has_id && option_has_name {
        return vec![ProductOptionUserError::new(
            json!(["options"]),
            "Only specify one of `id` or `name` fields for options.",
            Some("MIXING_ID_AND_NAME_KEYS_IS_NOT_ALLOWED"),
        )];
    }
    // Value-selector mixing is rejected per option (mixing `id` and `name` within a
    // single option's values), not across different options.
    for input in inputs {
        let value_inputs = resolved_object_list_field(input, "values");
        let value_has_id = value_inputs
            .iter()
            .any(|value| resolved_string_field(value, "id").is_some());
        let value_has_name = value_inputs
            .iter()
            .any(|value| resolved_string_field(value, "name").is_some());
        if value_has_id && value_has_name {
            return vec![ProductOptionUserError::new(
                json!(["options"]),
                "Only specify one of `id` or `name` fields for option values.",
                Some("MIXING_ID_AND_NAME_KEYS_IS_NOT_ALLOWED"),
            )];
        }
    }

    let mut seen_options = BTreeSet::new();
    for (index, input) in inputs.iter().enumerate() {
        let option_match = option_match_for_reorder(graph, input);
        let Some(option_index) = option_match else {
            if let Some(id) = resolved_string_field(input, "id") {
                errors.push(ProductOptionUserError::new(
                    json!(["options"]),
                    format!("Option id '{}' does not exist.", resource_id_tail(&id)),
                    Some("OPTION_ID_DOES_NOT_EXIST"),
                ));
            } else if let Some(name) = resolved_string_field(input, "name") {
                errors.push(ProductOptionUserError::new(
                    json!(["options"]),
                    format!("Option name '{name}' does not exist."),
                    Some("OPTION_NAME_DOES_NOT_EXIST"),
                ));
            } else {
                errors.push(ProductOptionUserError::new(
                    json!(["options", index.to_string()]),
                    "Option selector is required.",
                    Some("OPTION_DOES_NOT_EXIST"),
                ));
            }
            continue;
        };
        let option = &graph.options[option_index];
        let option_key = option.name.to_ascii_lowercase();
        if !seen_options.insert(option_key.clone()) {
            errors.push(ProductOptionUserError::new(
                json!(["options", index.to_string()]),
                format!("Duplicated option name '{}'.", option.name),
                Some("DUPLICATED_OPTION_NAME"),
            ));
        }
        let mut seen_values = BTreeSet::new();
        for (value_index, value_input) in resolved_object_list_field(input, "values")
            .iter()
            .enumerate()
        {
            let value_match = option_value_match_for_reorder(option, value_input);
            let Some(value) = value_match else {
                if let Some(id) = resolved_string_field(value_input, "id") {
                    errors.push(ProductOptionUserError::new(
                        json!(["options"]),
                        format!(
                            "Option value id '{}' does not exist.",
                            resource_id_tail(&id)
                        ),
                        Some("OPTION_VALUE_ID_DOES_NOT_EXIST"),
                    ));
                } else if let Some(name) = resolved_string_field(value_input, "name") {
                    errors.push(ProductOptionUserError::new(
                        json!(["options"]),
                        format!("Option value '{name}' does not exist."),
                        Some("OPTION_VALUE_DOES_NOT_EXIST"),
                    ));
                }
                continue;
            };
            if !seen_values.insert(value.name.to_ascii_lowercase()) {
                errors.push(ProductOptionUserError::new(
                    json!(["options", value_index.to_string()]),
                    format!("Duplicated option value '{}'.", value.name),
                    Some("DUPLICATED_OPTION_VALUE"),
                ));
            }
        }
    }
    errors
}

fn reorder_product_option_graph(
    graph: &ProductOptionGraph,
    inputs: &[BTreeMap<String, ResolvedValue>],
) -> ProductOptionGraph {
    let mut reordered = Vec::new();
    let mut used_ids = BTreeSet::new();
    for input in inputs {
        let Some(index) = option_match_for_reorder(graph, input) else {
            continue;
        };
        let mut option = graph.options[index].clone();
        let value_inputs = resolved_object_list_field(input, "values");
        if !value_inputs.is_empty() {
            // Reorder the option's values to match the order given in the input,
            // appending any values the input did not mention in their original order.
            let mut reordered_values = Vec::new();
            let mut used_value_ids = BTreeSet::new();
            for value_input in &value_inputs {
                if let Some(value) = option_value_match_for_reorder(&option, value_input) {
                    if used_value_ids.insert(value.id.clone()) {
                        reordered_values.push(value.clone());
                    }
                }
            }
            reordered_values.extend(
                option
                    .values
                    .iter()
                    .filter(|value| !used_value_ids.contains(&value.id))
                    .cloned(),
            );
            option.values = reordered_values;
        }
        used_ids.insert(option.id.clone());
        reordered.push(option);
    }
    reordered.extend(
        graph
            .options
            .iter()
            .filter(|option| !used_ids.contains(&option.id))
            .cloned(),
    );
    for (index, option) in reordered.iter_mut().enumerate() {
        option.position = index + 1;
    }
    ProductOptionGraph { options: reordered }
}

fn option_match_for_reorder(
    graph: &ProductOptionGraph,
    input: &BTreeMap<String, ResolvedValue>,
) -> Option<usize> {
    if let Some(id) = resolved_string_field(input, "id") {
        return graph.option_index_by_id(&id);
    }
    if let Some(name) = resolved_string_field(input, "name") {
        return graph.option_index_by_name(&name);
    }
    None
}

fn option_value_match_for_reorder<'a>(
    option: &'a ProductOptionNode,
    input: &BTreeMap<String, ResolvedValue>,
) -> Option<&'a ProductOptionValueNode> {
    if let Some(id) = resolved_string_field(input, "id") {
        return option.values.iter().find(|value| value.id == id);
    }
    if let Some(name) = resolved_string_field(input, "name") {
        return option.values.iter().find(|value| value.name == name);
    }
    None
}

fn product_with_option_graph(
    mut product: ProductRecord,
    graph: &ProductOptionGraph,
) -> ProductRecord {
    product.extra_fields.insert(
        "options".to_string(),
        Value::Array(graph.options.iter().map(product_option_json).collect()),
    );
    product
}

fn product_option_payload_json(
    payload_selection: &[SelectedField],
    product: Option<&ProductRecord>,
    graph: Option<&ProductOptionGraph>,
    variants: Vec<ProductVariantRecord>,
    currency_code: &str,
    user_errors: &[ProductOptionUserError],
) -> Value {
    selected_payload_json(payload_selection, |selection| {
        match selection.name.as_str() {
            "product" => Some(product_option_product_json(
                product,
                graph,
                variants.clone(),
                &selection.selection,
                currency_code,
            )),
            "userErrors" => Some(Value::Array(
                user_errors
                    .iter()
                    .map(|error| selected_json(&error.to_json(), &selection.selection))
                    .collect(),
            )),
            _ => None,
        }
    })
}

fn product_option_product_json(
    product: Option<&ProductRecord>,
    graph: Option<&ProductOptionGraph>,
    variants: Vec<ProductVariantRecord>,
    selections: &[SelectedField],
    currency_code: &str,
) -> Value {
    let Some(product) = product else {
        return Value::Null;
    };
    let mut record = product.clone();
    if let Some(graph) = graph {
        record.extra_fields.insert(
            "options".to_string(),
            Value::Array(graph.options.iter().map(product_option_json).collect()),
        );
    }
    product_json_with_variants_and_currency(&record, &variants, selections, currency_code)
}

fn product_option_json(option: &ProductOptionNode) -> Value {
    let values = option
        .values
        .iter()
        .filter(|value| value.has_variants)
        .map(|value| json!(value.name))
        .collect::<Vec<_>>();
    let option_values = option
        .values
        .iter()
        .map(product_option_value_json)
        .collect::<Vec<_>>();
    let mut value = json!({
        "id": option.id,
        "name": option.name,
        "position": option.position,
        "values": values,
        "optionValues": option_values,
    });
    if let Some(object) = value.as_object_mut() {
        for (key, field_value) in &option.extra_fields {
            object.insert(key.clone(), field_value.clone());
        }
    }
    value
}

fn product_option_value_json(value: &ProductOptionValueNode) -> Value {
    let mut json_value = json!({
        "id": value.id,
        "name": value.name,
        "hasVariants": value.has_variants,
    });
    if let Some(object) = json_value.as_object_mut() {
        for (key, field_value) in &value.extra_fields {
            object.insert(key.clone(), field_value.clone());
        }
    }
    json_value
}

fn default_title_graph(proxy: &mut DraftProxy) -> ProductOptionGraph {
    ProductOptionGraph {
        options: vec![ProductOptionNode {
            id: proxy.next_proxy_synthetic_gid("ProductOption"),
            name: "Title".to_string(),
            position: 1,
            values: vec![ProductOptionValueNode {
                id: proxy.next_proxy_synthetic_gid("ProductOptionValue"),
                name: "Default Title".to_string(),
                has_variants: true,
                extra_fields: BTreeMap::new(),
            }],
            extra_fields: BTreeMap::new(),
        }],
    }
}

pub(in crate::proxy) fn empty_product_variant_record(
    product_id: String,
    id: String,
    inventory_item_id: String,
) -> ProductVariantRecord {
    ProductVariantRecord {
        id,
        product_id,
        title: "Default Title".to_string(),
        sku: String::new(),
        barcode: None,
        price: "0.00".to_string(),
        compare_at_price: None,
        taxable: true,
        inventory_policy: "DENY".to_string(),
        inventory_quantity: 0,
        selected_options: Vec::new(),
        inventory_item: ProductVariantInventoryItem {
            id: inventory_item_id,
            tracked: false,
            requires_shipping: true,
            extra_fields: BTreeMap::new(),
        },
        media_ids: Vec::new(),
        extra_fields: BTreeMap::new(),
    }
}

fn option_value_combinations(graph: &ProductOptionGraph) -> Vec<Vec<(String, String)>> {
    let mut combinations: Vec<Vec<(String, String)>> = vec![Vec::new()];
    for option in &graph.options {
        let names = option
            .values
            .iter()
            .map(|value| value.name.clone())
            .collect::<Vec<_>>();
        if names.is_empty() {
            continue;
        }
        let mut next = Vec::new();
        for existing in &combinations {
            for name in &names {
                let mut combination = existing.clone();
                combination.push((option.name.clone(), name.clone()));
                next.push(combination);
            }
        }
        combinations = next;
    }
    combinations
}

fn remap_variants_for_option_update(
    variants: &mut [ProductVariantRecord],
    updated_option_id: &str,
    old_option_name: &str,
    options: &[ProductOptionNode],
    renamed_values: &BTreeMap<String, String>,
) {
    let updated_option_name = options
        .iter()
        .find(|option| option.id == updated_option_id)
        .map(|option| option.name.clone())
        .unwrap_or_else(|| old_option_name.to_string());
    for variant in variants {
        for selected in &mut variant.selected_options {
            if selected.name == old_option_name {
                selected.name = updated_option_name.clone();
            }
            if let Some(new_value) = renamed_values.get(&selected.value) {
                selected.value = new_value.clone();
            }
        }
        order_variant_selected_options(&mut variant.selected_options, options);
        variant.title = variant_title(&variant.selected_options);
    }
}

fn remap_variants_to_graph(variants: &mut [ProductVariantRecord], graph: &ProductOptionGraph) {
    for variant in variants {
        let mut selected = Vec::new();
        for option in &graph.options {
            let existing = variant
                .selected_options
                .iter()
                .find(|selected| selected.name == option.name)
                .and_then(|selected| {
                    option
                        .values
                        .iter()
                        .find(|value| value.name == selected.value)
                        .map(|_| selected.value.clone())
                })
                .or_else(|| option.values.first().map(|value| value.name.clone()));
            if let Some(value) = existing {
                selected.push(ProductVariantSelectedOption {
                    name: option.name.clone(),
                    value,
                });
            }
        }
        variant.selected_options = selected;
        variant.title = variant_title(&variant.selected_options);
    }
}

fn order_variant_selected_options(
    selected_options: &mut Vec<ProductVariantSelectedOption>,
    options: &[ProductOptionNode],
) {
    let mut ordered = Vec::new();
    for option in options {
        if let Some(selected) = selected_options
            .iter()
            .find(|selected| selected.name == option.name)
            .cloned()
        {
            ordered.push(selected);
        }
    }
    *selected_options = ordered;
}

fn mark_variant_backed_values(graph: &mut ProductOptionGraph, variants: &[ProductVariantRecord]) {
    for option in &mut graph.options {
        for value in &mut option.values {
            value.has_variants = variants.iter().any(|variant| {
                variant
                    .selected_options
                    .iter()
                    .any(|selected| selected.name == option.name && selected.value == value.name)
            });
        }
    }
}

/// Shopify keeps option values that back a variant (`hasVariants: true`) ahead of
/// values that do not, regardless of any requested value reorder. A requested
/// `values` order is honored only within each group, so this stable partition runs
/// after `mark_variant_backed_values` to pin variant-backed values first while
/// preserving their relative (input-requested) order.
fn partition_variant_backed_values_first(graph: &mut ProductOptionGraph) {
    for option in &mut graph.options {
        let mut backed = Vec::new();
        let mut unbacked = Vec::new();
        for value in option.values.drain(..) {
            if value.has_variants {
                backed.push(value);
            } else {
                unbacked.push(value);
            }
        }
        backed.append(&mut unbacked);
        option.values = backed;
    }
}

fn variant_title(selected_options: &[ProductVariantSelectedOption]) -> String {
    if selected_options.is_empty() {
        "Default Title".to_string()
    } else {
        selected_options
            .iter()
            .map(|option| option.value.clone())
            .collect::<Vec<_>>()
            .join(" / ")
    }
}

fn variant_sort_key(variant: &ProductVariantRecord, graph: &ProductOptionGraph) -> Vec<usize> {
    graph
        .options
        .iter()
        .map(|option| {
            let value = variant
                .selected_options
                .iter()
                .find(|selected| selected.name == option.name)
                .map(|selected| selected.value.as_str())
                .unwrap_or_default();
            option
                .values
                .iter()
                .position(|candidate| candidate.name == value)
                .unwrap_or(usize::MAX)
        })
        .collect()
}

fn resolved_variant_strategy(variables: &BTreeMap<String, ResolvedValue>) -> Option<String> {
    match variables.get("variantStrategy") {
        Some(ResolvedValue::String(value)) => Some(value.clone()),
        _ => None,
    }
}

fn resolved_object_json(input: &BTreeMap<String, ResolvedValue>) -> Value {
    Value::Object(
        input
            .iter()
            .map(|(key, value)| (key.clone(), resolved_value_json(value)))
            .collect(),
    )
}

use super::storefront::{
    storefront_money_json, storefront_product_variant_json, storefront_sha256_hex,
    StorefrontRequestContext, STOREFRONT_CART_MUTATION_ROOTS,
};
use super::*;

const CART_INPUT_LIMIT: usize = 250;
const CART_NOTE_LIMIT: usize = 5_000;

pub(in crate::proxy) fn storefront_cart_root_is_sensitive(root: &str) -> bool {
    root == "cart" || STOREFRONT_CART_MUTATION_ROOTS.contains(&root)
}

pub(in crate::proxy) struct StorefrontCartOutcome {
    pub value: Value,
    pub errors: Vec<Value>,
}

impl DraftProxy {
    pub(in crate::proxy) fn storefront_cart_query_root(
        &self,
        field: &RootFieldSelection,
    ) -> StorefrontCartOutcome {
        let cart = resolved_string_field(&field.arguments, "id")
            .and_then(|id| self.storefront_cart_by_public_id(&id));
        StorefrontCartOutcome {
            value: cart
                .map(|cart| self.storefront_cart_json(&cart, &field.selection))
                .unwrap_or(Value::Null),
            errors: Vec::new(),
        }
    }

    pub(in crate::proxy) fn storefront_cart_mutation_root(
        &mut self,
        field: &RootFieldSelection,
    ) -> StorefrontCartOutcome {
        match field.name.as_str() {
            "cartCreate" => self.storefront_cart_create(field),
            "cartLinesAdd" => self.storefront_cart_lines_add(field),
            "cartLinesUpdate" => self.storefront_cart_lines_update(field),
            "cartLinesRemove" => self.storefront_cart_lines_remove(field),
            "cartAttributesUpdate" => self.storefront_cart_attributes_update(field),
            "cartNoteUpdate" => self.storefront_cart_note_update(field),
            _ => StorefrontCartOutcome {
                value: Value::Null,
                errors: Vec::new(),
            },
        }
    }

    fn storefront_cart_create(&mut self, field: &RootFieldSelection) -> StorefrontCartOutcome {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let attributes = resolved_object_list_field(&input, "attributes");
        let line_inputs = resolved_object_list_field(&input, "lines");
        if attributes.len() > CART_INPUT_LIMIT {
            return storefront_cart_input_limit_outcome(
                field,
                "input",
                "attributes",
                attributes.len(),
            );
        }
        if line_inputs.len() > CART_INPUT_LIMIT {
            return storefront_cart_input_limit_outcome(field, "input", "lines", line_inputs.len());
        }
        let note = resolved_string_field(&input, "note");
        if note
            .as_ref()
            .is_some_and(|note| note.chars().count() > CART_NOTE_LIMIT)
        {
            return self.storefront_cart_user_error_outcome(
                field,
                Value::Null,
                vec![cart_user_error(
                    ["input", "note"],
                    "The note exceeds the maximum number of 5000 characters.",
                    "NOTE_TOO_LONG",
                )],
                Vec::new(),
            );
        }
        if let Some(error) = self.storefront_cart_line_input_error(&line_inputs) {
            return self.storefront_cart_user_error_outcome(
                field,
                Value::Null,
                vec![error],
                Vec::new(),
            );
        }

        let sequence = self.store.staged.next_storefront_cart_id;
        self.store.staged.next_storefront_cart_id += 1;
        let timestamp = self.next_mutation_timestamp();
        let cart = StorefrontCartRecord {
            internal_id: sequence.to_string(),
            sequence,
            created_at: timestamp.clone(),
            updated_at: timestamp,
            note,
            attributes: storefront_cart_attributes(&attributes),
        };
        let mut lines = Vec::new();
        let mut warnings = Vec::new();
        for input in &line_inputs {
            self.storefront_cart_apply_line_add(&cart, &mut lines, input, &mut warnings);
        }
        self.storefront_cart_save(cart.clone(), lines);
        let warnings = self.storefront_cart_all_warnings(&cart, warnings);
        self.storefront_cart_user_error_outcome(
            field,
            self.storefront_cart_json(&cart, &cart_selection(field)),
            Vec::new(),
            warnings,
        )
    }

    fn storefront_cart_lines_add(&mut self, field: &RootFieldSelection) -> StorefrontCartOutcome {
        let inputs = resolved_object_list_field(&field.arguments, "lines");
        if inputs.len() > CART_INPUT_LIMIT {
            return storefront_cart_input_limit_outcome(field, "lines", "", inputs.len());
        }
        let cart_id = resolved_string_field(&field.arguments, "cartId").unwrap_or_default();
        let Some(mut cart) = self.storefront_cart_by_public_id(&cart_id) else {
            return self.storefront_cart_missing_mutation_outcome(field, &cart_id, None);
        };
        if let Some(error) = self.storefront_cart_line_input_error(&inputs) {
            return self.storefront_cart_user_error_outcome(
                field,
                Value::Null,
                vec![error],
                Vec::new(),
            );
        }
        let mut lines = self.storefront_cart_lines(&cart.internal_id);
        let mut warnings = Vec::new();
        for input in &inputs {
            self.storefront_cart_apply_line_add(&cart, &mut lines, input, &mut warnings);
        }
        cart.updated_at = self.next_mutation_timestamp();
        self.storefront_cart_save(cart.clone(), lines);
        let warnings = self.storefront_cart_all_warnings(&cart, warnings);
        self.storefront_cart_user_error_outcome(
            field,
            self.storefront_cart_json(&cart, &cart_selection(field)),
            Vec::new(),
            warnings,
        )
    }

    fn storefront_cart_lines_update(
        &mut self,
        field: &RootFieldSelection,
    ) -> StorefrontCartOutcome {
        let inputs = resolved_object_list_field(&field.arguments, "lines");
        if inputs.len() > CART_INPUT_LIMIT {
            return storefront_cart_input_limit_outcome(field, "lines", "", inputs.len());
        }
        let cart_id = resolved_string_field(&field.arguments, "cartId").unwrap_or_default();
        let Some(mut cart) = self.storefront_cart_by_public_id(&cart_id) else {
            return self.storefront_cart_missing_mutation_outcome(field, &cart_id, None);
        };
        let mut lines = self.storefront_cart_lines(&cart.internal_id);
        let mut user_errors = Vec::new();
        for (index, input) in inputs.iter().enumerate() {
            let id = resolved_string_field(input, "id").unwrap_or_default();
            if !lines
                .iter()
                .any(|line| self.storefront_cart_line_public_id(line) == id)
            {
                user_errors.push(cart_user_error(
                    ["lines", &index.to_string(), "id"],
                    &format!(
                        "The merchandise line with id {} does not exist.",
                        resource_id_tail(&id)
                    ),
                    "INVALID_MERCHANDISE_LINE",
                ));
                continue;
            }
            if let Some(merchandise_id) = resolved_string_field(input, "merchandiseId") {
                if self.storefront_cart_variant(&merchandise_id).is_none() {
                    user_errors.push(cart_user_error(
                        ["lines", &index.to_string(), "merchandiseId"],
                        &format!("The merchandise with id {merchandise_id} does not exist."),
                        "INVALID",
                    ));
                }
            }
            if let Some(selling_plan_id) = resolved_string_field(input, "sellingPlanId") {
                let merchandise_id = resolved_string_field(input, "merchandiseId").or_else(|| {
                    lines
                        .iter()
                        .find(|line| self.storefront_cart_line_public_id(line) == id)
                        .map(|line| line.merchandise_id.clone())
                });
                if merchandise_id.is_none_or(|merchandise_id| {
                    self.storefront_cart_selling_plan(&selling_plan_id, &merchandise_id)
                        .is_none()
                }) {
                    user_errors.push(cart_user_error(
                        ["lines", &index.to_string(), "sellingPlanId"],
                        "Cannot apply selling plan to variant",
                        "SELLING_PLAN_NOT_APPLICABLE",
                    ));
                }
            }
        }
        if !user_errors.is_empty() {
            let warnings = self.storefront_cart_all_warnings(&cart, Vec::new());
            return self.storefront_cart_user_error_outcome(
                field,
                self.storefront_cart_json(&cart, &cart_selection(field)),
                user_errors,
                warnings,
            );
        }

        let mut warnings = Vec::new();
        for input in &inputs {
            let id = resolved_string_field(input, "id").unwrap_or_default();
            let Some(position) = lines
                .iter()
                .position(|line| self.storefront_cart_line_public_id(line) == id)
            else {
                continue;
            };
            if resolved_int_field(input, "quantity") == Some(0) {
                lines.remove(position);
                continue;
            }
            if let Some(merchandise_id) = resolved_string_field(input, "merchandiseId") {
                lines[position].merchandise_id = merchandise_id;
            }
            if input.contains_key("attributes") {
                lines[position].attributes =
                    storefront_cart_attributes(&resolved_object_list_field(input, "attributes"));
            }
            if input.contains_key("sellingPlanId") {
                lines[position].selling_plan_id = resolved_string_field(input, "sellingPlanId");
            }
            if let Some(quantity) = resolved_int_field(input, "quantity") {
                self.storefront_cart_apply_line_update_quantity(
                    &cart,
                    &mut lines,
                    position,
                    quantity,
                    &mut warnings,
                );
            }
        }
        storefront_cart_merge_duplicate_lines(&mut lines);
        cart.updated_at = self.next_mutation_timestamp();
        self.storefront_cart_save(cart.clone(), lines);
        let warnings = self.storefront_cart_all_warnings(&cart, warnings);
        self.storefront_cart_user_error_outcome(
            field,
            self.storefront_cart_json(&cart, &cart_selection(field)),
            Vec::new(),
            warnings,
        )
    }

    fn storefront_cart_lines_remove(
        &mut self,
        field: &RootFieldSelection,
    ) -> StorefrontCartOutcome {
        let line_ids = list_string_field(&field.arguments, "lineIds");
        if line_ids.len() > CART_INPUT_LIMIT {
            return storefront_cart_input_limit_outcome(field, "lineIds", "", line_ids.len());
        }
        let cart_id = resolved_string_field(&field.arguments, "cartId").unwrap_or_default();
        let Some(mut cart) = self.storefront_cart_by_public_id(&cart_id) else {
            return self.storefront_cart_missing_mutation_outcome(field, &cart_id, None);
        };
        let mut lines = self.storefront_cart_lines(&cart.internal_id);
        let mut user_errors = Vec::new();
        for (index, id) in line_ids.iter().enumerate() {
            if !lines
                .iter()
                .any(|line| self.storefront_cart_line_public_id(line) == *id)
            {
                user_errors.push(cart_user_error(
                    ["lineIds", &index.to_string()],
                    &format!(
                        "The merchandise line with id {} does not exist.",
                        resource_id_tail(id)
                    ),
                    "INVALID_MERCHANDISE_LINE",
                ));
            }
        }
        if user_errors.is_empty() {
            lines.retain(|line| !line_ids.contains(&self.storefront_cart_line_public_id(line)));
            cart.updated_at = self.next_mutation_timestamp();
            self.storefront_cart_save(cart.clone(), lines);
        }
        let warnings = self.storefront_cart_all_warnings(&cart, Vec::new());
        self.storefront_cart_user_error_outcome(
            field,
            self.storefront_cart_json(&cart, &cart_selection(field)),
            user_errors,
            warnings,
        )
    }

    fn storefront_cart_attributes_update(
        &mut self,
        field: &RootFieldSelection,
    ) -> StorefrontCartOutcome {
        let inputs = resolved_object_list_field(&field.arguments, "attributes");
        if inputs.len() > CART_INPUT_LIMIT {
            return storefront_cart_input_limit_outcome(field, "attributes", "", inputs.len());
        }
        let cart_id = resolved_string_field(&field.arguments, "cartId").unwrap_or_default();
        let Some(mut cart) = self.storefront_cart_by_public_id(&cart_id) else {
            return self.storefront_cart_missing_mutation_outcome(field, &cart_id, None);
        };
        cart.attributes = storefront_cart_attributes(&inputs);
        cart.updated_at = self.next_mutation_timestamp();
        let lines = self.storefront_cart_lines(&cart.internal_id);
        self.storefront_cart_save(cart.clone(), lines);
        let warnings = self.storefront_cart_all_warnings(&cart, Vec::new());
        self.storefront_cart_user_error_outcome(
            field,
            self.storefront_cart_json(&cart, &cart_selection(field)),
            Vec::new(),
            warnings,
        )
    }

    fn storefront_cart_note_update(&mut self, field: &RootFieldSelection) -> StorefrontCartOutcome {
        let cart_id = resolved_string_field(&field.arguments, "cartId").unwrap_or_default();
        let note = resolved_string_field(&field.arguments, "note").unwrap_or_default();
        if note.chars().count() > CART_NOTE_LIMIT {
            return self.storefront_cart_user_error_outcome(
                field,
                Value::Null,
                vec![cart_user_error(
                    ["note"],
                    "The note exceeds the maximum number of 5000 characters.",
                    "NOTE_TOO_LONG",
                )],
                Vec::new(),
            );
        }
        let Some(mut cart) = self.storefront_cart_by_public_id(&cart_id) else {
            return self.storefront_cart_missing_mutation_outcome(field, &cart_id, Some(&note));
        };
        cart.note = Some(note);
        cart.updated_at = self.next_mutation_timestamp();
        let lines = self.storefront_cart_lines(&cart.internal_id);
        self.storefront_cart_save(cart.clone(), lines);
        let warnings = self.storefront_cart_all_warnings(&cart, Vec::new());
        self.storefront_cart_user_error_outcome(
            field,
            self.storefront_cart_json(&cart, &cart_selection(field)),
            Vec::new(),
            warnings,
        )
    }

    fn storefront_cart_missing_mutation_outcome(
        &mut self,
        field: &RootFieldSelection,
        _cart_id: &str,
        note: Option<&str>,
    ) -> StorefrontCartOutcome {
        let cart = note.map(|note| {
            let sequence = self.store.staged.next_storefront_cart_id;
            self.store.staged.next_storefront_cart_id += 1;
            let timestamp = self.next_mutation_timestamp();
            let cart = StorefrontCartRecord {
                internal_id: sequence.to_string(),
                sequence,
                created_at: timestamp.clone(),
                updated_at: timestamp,
                note: Some(note.to_string()),
                attributes: Vec::new(),
            };
            self.storefront_cart_save(cart.clone(), Vec::new());
            self.storefront_cart_json(&cart, &cart_selection(field))
        });
        self.storefront_cart_user_error_outcome(
            field,
            cart.unwrap_or(Value::Null),
            vec![cart_user_error(
                ["cartId"],
                "The specified cart does not exist.",
                "INVALID",
            )],
            Vec::new(),
        )
    }

    fn storefront_cart_user_error_outcome(
        &self,
        field: &RootFieldSelection,
        cart: Value,
        user_errors: Vec<Value>,
        warnings: Vec<Value>,
    ) -> StorefrontCartOutcome {
        StorefrontCartOutcome {
            value: selected_payload_json(&field.selection, |selection| {
                match selection.name.as_str() {
                    "cart" => Some(cart.clone()),
                    "userErrors" => Some(Value::Array(
                        user_errors
                            .iter()
                            .map(|error| selected_json(error, &selection.selection))
                            .collect(),
                    )),
                    "warnings" => Some(Value::Array(
                        warnings
                            .iter()
                            .map(|warning| selected_json(warning, &selection.selection))
                            .collect(),
                    )),
                    _ => None,
                }
            }),
            errors: Vec::new(),
        }
    }

    fn storefront_cart_by_public_id(&self, id: &str) -> Option<StorefrontCartRecord> {
        self.store
            .staged
            .storefront_carts
            .values()
            .find(|cart| storefront_cart_public_id(cart.sequence) == id)
            .cloned()
    }

    fn storefront_cart_lines(&self, cart_internal_id: &str) -> Vec<StorefrontCartLineRecord> {
        self.store
            .staged
            .storefront_cart_line_order
            .get(cart_internal_id)
            .into_iter()
            .flatten()
            .filter_map(|id| self.store.staged.storefront_cart_lines.get(id).cloned())
            .collect()
    }

    fn storefront_cart_save(
        &mut self,
        cart: StorefrontCartRecord,
        lines: Vec<StorefrontCartLineRecord>,
    ) {
        let old_ids = self
            .store
            .staged
            .storefront_cart_line_order
            .get(&cart.internal_id)
            .cloned()
            .unwrap_or_default();
        let new_ids = lines
            .iter()
            .map(|line| line.internal_id.clone())
            .collect::<Vec<_>>();
        for id in old_ids {
            if !new_ids.contains(&id) {
                self.store.staged.storefront_cart_lines.remove(&id);
            }
        }
        for line in lines {
            self.store
                .staged
                .storefront_cart_lines
                .insert(line.internal_id.clone(), line);
        }
        self.store
            .staged
            .storefront_cart_line_order
            .insert(cart.internal_id.clone(), new_ids);
        if !self
            .store
            .staged
            .storefront_cart_order
            .contains(&cart.internal_id)
        {
            self.store
                .staged
                .storefront_cart_order
                .push(cart.internal_id.clone());
        }
        self.store
            .staged
            .storefront_carts
            .insert(cart.internal_id.clone(), cart);
    }

    fn storefront_cart_variant(&self, id: &str) -> Option<ProductVariantRecord> {
        let variant = self.store.product_variant_by_id(id)?.clone();
        let product = self.store.product_by_id(&variant.product_id)?;
        self.storefront_product_is_visible(product)
            .then_some(variant)
    }

    fn storefront_cart_line_input_error(
        &self,
        inputs: &[BTreeMap<String, ResolvedValue>],
    ) -> Option<Value> {
        for (index, input) in inputs.iter().enumerate() {
            let merchandise_id = resolved_string_field(input, "merchandiseId").unwrap_or_default();
            if self.storefront_cart_variant(&merchandise_id).is_none() {
                return Some(cart_user_error(
                    ["lines", &index.to_string(), "merchandiseId"],
                    &format!("The merchandise with id {merchandise_id} does not exist."),
                    "INVALID",
                ));
            }
            if let Some(selling_plan_id) = resolved_string_field(input, "sellingPlanId") {
                if self
                    .storefront_cart_selling_plan(&selling_plan_id, &merchandise_id)
                    .is_none()
                {
                    return Some(cart_user_error(
                        ["lines", &index.to_string(), "sellingPlanId"],
                        "Cannot apply selling plan to variant",
                        "SELLING_PLAN_NOT_APPLICABLE",
                    ));
                }
            }
        }
        None
    }

    fn storefront_cart_selling_plan(
        &self,
        selling_plan_id: &str,
        variant_id: &str,
    ) -> Option<SellingPlanRecord> {
        let variant = self.store.product_variant_by_id(variant_id)?;
        self.store
            .staged
            .selling_plan_groups
            .iter()
            .find_map(|(_, group)| {
                let applies = group.product_variant_ids.iter().any(|id| id == variant_id)
                    || group.product_ids.iter().any(|id| id == &variant.product_id);
                applies.then(|| {
                    group
                        .selling_plans
                        .iter()
                        .find(|plan| plan.id == selling_plan_id)
                        .cloned()
                })?
            })
    }

    fn storefront_cart_apply_line_add(
        &mut self,
        cart: &StorefrontCartRecord,
        lines: &mut Vec<StorefrontCartLineRecord>,
        input: &BTreeMap<String, ResolvedValue>,
        warnings: &mut Vec<Value>,
    ) {
        let merchandise_id = resolved_string_field(input, "merchandiseId").unwrap_or_default();
        let requested = resolved_int_field(input, "quantity").unwrap_or(1).max(0);
        if requested == 0 {
            return;
        }
        let attributes =
            storefront_cart_attributes(&resolved_object_list_field(input, "attributes"));
        let selling_plan_id = resolved_string_field(input, "sellingPlanId");
        let existing = lines.iter().position(|line| {
            line.merchandise_id == merchandise_id
                && line.selling_plan_id == selling_plan_id
                && line.attributes == attributes
        });
        let current = lines
            .iter()
            .filter(|line| line.merchandise_id == merchandise_id)
            .map(|line| line.quantity)
            .sum::<i64>();
        let available = self.storefront_cart_available_quantity(&merchandise_id);
        let applied = if available == i64::MAX {
            requested
        } else {
            requested.min((available - current).max(0))
        };
        let target_position = if let Some(position) = existing {
            lines[position].quantity += applied;
            lines[position].out_of_stock_warning = applied == 0 && requested > 0;
            position
        } else {
            let sequence = self.store.staged.next_storefront_cart_line_id;
            self.store.staged.next_storefront_cart_line_id += 1;
            lines.insert(
                0,
                StorefrontCartLineRecord {
                    internal_id: sequence.to_string(),
                    sequence,
                    cart_internal_id: cart.internal_id.clone(),
                    merchandise_id: merchandise_id.clone(),
                    quantity: applied,
                    attributes,
                    selling_plan_id,
                    out_of_stock_warning: applied == 0,
                },
            );
            0
        };
        if applied < requested {
            let target = self.storefront_cart_line_public_id(&lines[target_position]);
            warnings.push(if applied == 0 {
                self.storefront_cart_out_of_stock_warning(&lines[target_position])
            } else {
                storefront_cart_stock_capped_warning(applied, &target)
            });
        }
    }

    fn storefront_cart_apply_line_update_quantity(
        &self,
        _cart: &StorefrontCartRecord,
        lines: &mut [StorefrontCartLineRecord],
        target_position: usize,
        requested: i64,
        warnings: &mut Vec<Value>,
    ) {
        let merchandise_id = lines[target_position].merchandise_id.clone();
        let available = self.storefront_cart_available_quantity(&merchandise_id);
        lines[target_position].quantity = if available == i64::MAX {
            requested.max(0)
        } else {
            requested.max(0).min(available)
        };
        lines[target_position].out_of_stock_warning =
            requested > 0 && lines[target_position].quantity == 0;
        let mut remaining = if available == i64::MAX {
            i64::MAX
        } else {
            (available - lines[target_position].quantity).max(0)
        };
        for (position, line) in lines.iter_mut().enumerate() {
            if position == target_position || line.merchandise_id != merchandise_id {
                continue;
            }
            if remaining == i64::MAX {
                continue;
            }
            let previous = line.quantity;
            line.quantity = previous.min(remaining);
            remaining -= line.quantity;
            if line.quantity < previous {
                let target = self.storefront_cart_line_public_id(line);
                warnings.push(if line.quantity == 0 {
                    line.out_of_stock_warning = true;
                    self.storefront_cart_out_of_stock_warning(line)
                } else {
                    storefront_cart_stock_capped_warning(line.quantity, &target)
                });
            }
        }
        if lines[target_position].quantity < requested.max(0) {
            let target = self.storefront_cart_line_public_id(&lines[target_position]);
            warnings.push(if lines[target_position].quantity == 0 {
                self.storefront_cart_out_of_stock_warning(&lines[target_position])
            } else {
                storefront_cart_stock_capped_warning(lines[target_position].quantity, &target)
            });
        }
    }

    fn storefront_cart_available_quantity(&self, merchandise_id: &str) -> i64 {
        self.storefront_cart_variant(merchandise_id)
            .map(|variant| {
                if !variant.inventory_item.tracked || variant.inventory_policy == "CONTINUE" {
                    i64::MAX
                } else {
                    variant.inventory_quantity.max(0)
                }
            })
            .unwrap_or(0)
    }

    fn storefront_cart_all_warnings(
        &self,
        cart: &StorefrontCartRecord,
        mut immediate: Vec<Value>,
    ) -> Vec<Value> {
        for line in self.storefront_cart_lines(&cart.internal_id) {
            if line.out_of_stock_warning {
                immediate.push(self.storefront_cart_out_of_stock_warning(&line));
            }
        }
        let mut seen = BTreeSet::new();
        immediate.retain(|warning| {
            seen.insert((
                warning["code"].as_str().unwrap_or_default().to_string(),
                warning["target"].as_str().unwrap_or_default().to_string(),
            ))
        });
        immediate
    }

    fn storefront_cart_out_of_stock_warning(&self, line: &StorefrontCartLineRecord) -> Value {
        let variant = self.store.product_variant_by_id(&line.merchandise_id);
        let product = variant.and_then(|variant| self.store.product_by_id(&variant.product_id));
        let title = match (product, variant) {
            (Some(product), Some(variant)) => format!("{} - {}", product.title, variant.title),
            _ => line.merchandise_id.clone(),
        };
        json!({
            "code": "MERCHANDISE_OUT_OF_STOCK",
            "message": format!("The product '{title}' is already sold out."),
            "target": self.storefront_cart_line_public_id(line)
        })
    }

    fn storefront_cart_json(
        &self,
        cart: &StorefrontCartRecord,
        selections: &[SelectedField],
    ) -> Value {
        let lines = self.storefront_cart_lines(&cart.internal_id);
        let total_quantity = lines.iter().map(|line| line.quantity).sum::<i64>();
        let total = self.storefront_cart_total(&lines);
        let currency_code = self.storefront_currency_code();
        selected_payload_json(selections, |selection| match selection.name.as_str() {
            "__typename" => Some(json!("Cart")),
            "id" => Some(json!(storefront_cart_public_id(cart.sequence))),
            "createdAt" => Some(json!(cart.created_at)),
            "updatedAt" => Some(json!(cart.updated_at)),
            "checkoutUrl" => Some(json!(self.storefront_cart_checkout_url(cart.sequence))),
            "totalQuantity" => Some(json!(total_quantity)),
            "note" => Some(
                cart.note
                    .as_ref()
                    .map(|note| json!(note))
                    .unwrap_or(Value::Null),
            ),
            "attribute" => Some(
                resolved_string_field(&selection.arguments, "key")
                    .and_then(|key| {
                        cart.attributes
                            .iter()
                            .find(|attribute| attribute.key == key)
                    })
                    .map(|attribute| {
                        storefront_cart_attribute_json(attribute, &selection.selection)
                    })
                    .unwrap_or(Value::Null),
            ),
            "attributes" => Some(Value::Array(
                cart.attributes
                    .iter()
                    .map(|attribute| {
                        storefront_cart_attribute_json(attribute, &selection.selection)
                    })
                    .collect(),
            )),
            "lines" => Some(selected_typed_connection_with_args(
                &lines,
                &selection.arguments,
                &selection.selection,
                |line, selections| self.storefront_cart_line_json(line, selections),
                |line| self.storefront_cart_line_cursor(line),
            )),
            "cost" => Some(storefront_cart_cost_json(
                total,
                &currency_code,
                &selection.selection,
                true,
            )),
            "estimatedCost" => Some(storefront_cart_cost_json(
                total,
                &currency_code,
                &selection.selection,
                false,
            )),
            "appliedGiftCards" | "discountAllocations" | "discountCodes" | "metafields" => {
                Some(Value::Array(Vec::new()))
            }
            "buyerIdentity" => Some(selected_json(
                &json!({
                    "countryCode": Value::Null,
                    "customer": Value::Null,
                    "deliveryAddressPreferences": [],
                    "email": Value::Null,
                    "phone": Value::Null,
                    "preferences": Value::Null,
                    "purchasingCompany": Value::Null
                }),
                &selection.selection,
            )),
            "delivery" => Some(selected_json(
                &json!({ "addresses": [], "groups": [] }),
                &selection.selection,
            )),
            "deliveryGroups" => Some(selected_empty_connection_json(&selection.selection)),
            "metafield" => Some(Value::Null),
            _ => None,
        })
    }

    fn storefront_cart_line_json(
        &self,
        line: &StorefrontCartLineRecord,
        selections: &[SelectedField],
    ) -> Value {
        let variant = self.store.product_variant_by_id(&line.merchandise_id);
        let product = variant.and_then(|variant| self.store.product_by_id(&variant.product_id));
        let currency_code = self.storefront_currency_code();
        let line_total = variant
            .and_then(|variant| variant.price.parse::<f64>().ok())
            .map(|price| price * line.quantity as f64)
            .unwrap_or(0.0);
        selected_payload_json(selections, |selection| {
            match selection.name.as_str() {
            "__typename" => Some(json!("CartLine")),
            "id" => Some(json!(self.storefront_cart_line_public_id(line))),
            "quantity" => Some(json!(line.quantity)),
            "attribute" => Some(
                resolved_string_field(&selection.arguments, "key")
                    .and_then(|key| line.attributes.iter().find(|attribute| attribute.key == key))
                    .map(|attribute| storefront_cart_attribute_json(attribute, &selection.selection))
                    .unwrap_or(Value::Null),
            ),
            "attributes" => Some(Value::Array(
                line.attributes
                    .iter()
                    .map(|attribute| storefront_cart_attribute_json(attribute, &selection.selection))
                    .collect(),
            )),
            "merchandise" => Some(
                variant
                    .map(|variant| {
                        storefront_product_variant_json(
                            self,
                            variant,
                            product,
                            &StorefrontRequestContext::default(),
                            Some(&currency_code),
                            &selection.selection,
                        )
                    })
                    .unwrap_or(Value::Null),
            ),
            "sellingPlanAllocation" => Some(
                line.selling_plan_id
                    .as_deref()
                    .and_then(|id| self.storefront_cart_selling_plan(id, &line.merchandise_id))
                    .map(|plan| {
                        selected_json(
                            &json!({
                                "sellingPlan": { "id": plan.id, "name": plan.name },
                                "checkoutChargeAmount": storefront_money_value(0.0, &currency_code),
                                "remainingBalanceChargeAmount": storefront_money_value(0.0, &currency_code),
                                "priceAdjustments": []
                            }),
                            &selection.selection,
                        )
                    })
                    .unwrap_or(Value::Null),
            ),
            "cost" => Some(storefront_cart_line_cost_json(
                variant,
                line_total,
                &currency_code,
                &selection.selection,
                true,
            )),
            "estimatedCost" => Some(storefront_cart_line_cost_json(
                variant,
                line_total,
                &currency_code,
                &selection.selection,
                false,
            )),
            "discountAllocations" => Some(Value::Array(Vec::new())),
            "instructions" => Some(selected_json(
                &json!({ "deliveryProfile": Value::Null }),
                &selection.selection,
            )),
            "parentRelationship" => Some(Value::Null),
            _ => None,
        }
        })
    }

    fn storefront_cart_total(&self, lines: &[StorefrontCartLineRecord]) -> f64 {
        lines
            .iter()
            .filter_map(|line| {
                let variant = self.store.product_variant_by_id(&line.merchandise_id)?;
                let price = variant.price.parse::<f64>().ok()?;
                Some(price * line.quantity as f64)
            })
            .sum()
    }

    fn storefront_cart_checkout_url(&self, sequence: u64) -> String {
        format!(
            "{}/cart/c/{}?key={}",
            self.config.shopify_admin_origin.trim_end_matches('/'),
            storefront_cart_token(sequence),
            storefront_cart_key(sequence)
        )
    }

    fn storefront_cart_line_public_id(&self, line: &StorefrontCartLineRecord) -> String {
        let cart = self
            .store
            .staged
            .storefront_carts
            .get(&line.cart_internal_id);
        let cart_token = cart
            .map(|cart| storefront_cart_token(cart.sequence))
            .unwrap_or_default();
        format!(
            "gid://shopify/CartLine/{}?cart={cart_token}",
            storefront_cart_line_token(line.sequence)
        )
    }

    fn storefront_cart_line_cursor(&self, line: &StorefrontCartLineRecord) -> String {
        format!(
            "storefront-cart-line-cursor-{}-{}",
            line.sequence,
            &storefront_sha256_hex(&self.storefront_cart_line_public_id(line))[..16]
        )
    }
}

fn cart_selection(field: &RootFieldSelection) -> Vec<SelectedField> {
    field
        .selection
        .iter()
        .find(|selection| selection.name == "cart")
        .map(|selection| selection.selection.clone())
        .unwrap_or_default()
}

fn storefront_cart_attributes(
    inputs: &[BTreeMap<String, ResolvedValue>],
) -> Vec<StorefrontCartAttributeRecord> {
    let mut attributes = Vec::<StorefrontCartAttributeRecord>::new();
    for input in inputs {
        let key = resolved_string_field(input, "key").unwrap_or_default();
        let value = resolved_string_field(input, "value").unwrap_or_default();
        if let Some(existing) = attributes.iter_mut().find(|attribute| attribute.key == key) {
            existing.value = value;
        } else {
            attributes.push(StorefrontCartAttributeRecord { key, value });
        }
    }
    attributes
}

fn storefront_cart_merge_duplicate_lines(lines: &mut Vec<StorefrontCartLineRecord>) {
    let mut index = 0;
    while index < lines.len() {
        let mut other = index + 1;
        while other < lines.len() {
            if lines[index].merchandise_id == lines[other].merchandise_id
                && lines[index].selling_plan_id == lines[other].selling_plan_id
                && lines[index].attributes == lines[other].attributes
            {
                lines[index].quantity += lines[other].quantity;
                lines[index].out_of_stock_warning |= lines[other].out_of_stock_warning;
                lines.remove(other);
            } else {
                other += 1;
            }
        }
        index += 1;
    }
}

fn storefront_cart_attribute_json(
    attribute: &StorefrontCartAttributeRecord,
    selections: &[SelectedField],
) -> Value {
    selected_json(
        &json!({ "key": attribute.key, "value": attribute.value }),
        selections,
    )
}

fn storefront_cart_public_id(sequence: u64) -> String {
    format!(
        "gid://shopify/Cart/{}?key={}",
        storefront_cart_token(sequence),
        storefront_cart_key(sequence)
    )
}

fn storefront_cart_token(sequence: u64) -> String {
    format!(
        "sdp_cart_{sequence}_{}",
        &storefront_sha256_hex(&format!("storefront-cart-token:{sequence}"))[..24]
    )
}

fn storefront_cart_key(sequence: u64) -> String {
    format!(
        "sdp_key_{}",
        &storefront_sha256_hex(&format!("storefront-cart-key:{sequence}"))[..24]
    )
}

fn storefront_cart_line_token(sequence: u64) -> String {
    format!(
        "sdp_line_{sequence}_{}",
        &storefront_sha256_hex(&format!("storefront-cart-line:{sequence}"))[..16]
    )
}

fn cart_user_error<const N: usize>(field: [&str; N], message: &str, code: &str) -> Value {
    user_error(field, message, Some(code))
}

fn storefront_cart_stock_capped_warning(quantity: i64, target: &str) -> Value {
    let noun = if quantity == 1 {
        "item was"
    } else {
        "items were"
    };
    json!({
        "code": "MERCHANDISE_NOT_ENOUGH_STOCK",
        "message": format!("Only {quantity} {noun} added to your cart due to availability."),
        "target": target
    })
}

fn storefront_cart_input_limit_outcome(
    field: &RootFieldSelection,
    first_path: &str,
    second_path: &str,
    size: usize,
) -> StorefrontCartOutcome {
    let mut path = vec![json!(field.response_key), json!(first_path)];
    if !second_path.is_empty() {
        path.push(json!(second_path));
    }
    StorefrontCartOutcome {
        value: Value::Null,
        errors: vec![json!({
            "message": format!("The input array size of {size} is greater than the maximum allowed of 250."),
            "locations": [{ "line": field.location.line, "column": field.location.column }],
            "path": path,
            "extensions": { "code": "MAX_INPUT_SIZE_EXCEEDED" }
        })],
    }
}

fn storefront_money_value(amount: f64, currency_code: &str) -> Value {
    json!({
        "amount": format_money_amount(amount),
        "currencyCode": currency_code
    })
}

fn storefront_cart_cost_json(
    total: f64,
    currency_code: &str,
    selections: &[SelectedField],
    current: bool,
) -> Value {
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "checkoutChargeAmount" | "subtotalAmount" | "totalAmount" => Some(selected_json(
            &storefront_money_value(total, currency_code),
            &selection.selection,
        )),
        "totalDutyAmount" | "totalTaxAmount" => Some(Value::Null),
        "subtotalAmountEstimated"
        | "totalAmountEstimated"
        | "totalDutyAmountEstimated"
        | "totalTaxAmountEstimated"
            if current =>
        {
            Some(json!(true))
        }
        _ => None,
    })
}

fn storefront_cart_line_cost_json(
    variant: Option<&ProductVariantRecord>,
    total: f64,
    currency_code: &str,
    selections: &[SelectedField],
    current: bool,
) -> Value {
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "amountPerQuantity" if current => variant.map(|variant| {
            storefront_money_json(&variant.price, currency_code, &selection.selection)
        }),
        "compareAtAmountPerQuantity" if current => Some(
            variant
                .and_then(|variant| variant.compare_at_price.as_deref())
                .map(|amount| storefront_money_json(amount, currency_code, &selection.selection))
                .unwrap_or(Value::Null),
        ),
        "amount" if !current => variant.map(|variant| {
            storefront_money_json(&variant.price, currency_code, &selection.selection)
        }),
        "compareAtAmount" if !current => Some(
            variant
                .and_then(|variant| variant.compare_at_price.as_deref())
                .map(|amount| storefront_money_json(amount, currency_code, &selection.selection))
                .unwrap_or(Value::Null),
        ),
        "subtotalAmount" | "totalAmount" => Some(selected_json(
            &storefront_money_value(total, currency_code),
            &selection.selection,
        )),
        _ => None,
    })
}

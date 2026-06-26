use super::*;

const MAX_SELLING_PLAN_GROUPS_PER_RESOURCE: usize = 31;
const MAX_SELLING_PLANS_PER_GROUP: usize = 31;
const INT32_MIN: i64 = i32::MIN as i64;
const INT32_MAX: i64 = i32::MAX as i64;

impl DraftProxy {
    pub(in crate::proxy) fn selling_plan_group_query_data(
        &self,
        fields: &[RootFieldSelection],
    ) -> Value {
        root_payload_json(fields, |field| {
            Some(match field.name.as_str() {
                "sellingPlanGroup" => {
                    let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                    self.store
                        .selling_plan_group_by_id(&id)
                        .map(|group| self.selling_plan_group_json(group, &field.selection))
                        .unwrap_or(Value::Null)
                }
                "sellingPlanGroups" => {
                    let groups = self.store.selling_plan_groups();
                    selected_typed_connection_with_args(
                        &groups,
                        &field.arguments,
                        &field.selection,
                        |group, selections| self.selling_plan_group_json(group, selections),
                        |group| group.id.clone(),
                    )
                }
                // Membership read-back queries pair `sellingPlanGroup` with sibling
                // `product`/`productVariant` roots that must surface the staged
                // selling-plan overlay (`sellingPlanGroups`/`sellingPlanGroupsCount`).
                // Resolve those roots through the same overlay-aware resolvers the
                // products arm uses so a mixed query routed here doesn't drop them.
                "product" => self.product_by_id_field(field),
                "products" => self.products_connection_field(field),
                "productsCount" => self.products_count_field(field),
                "productByIdentifier" => self.product_by_identifier_field(field),
                "productVariant" => self.product_variant_by_id_field(field),
                _ => return None,
            })
        })
    }

    pub(in crate::proxy) fn selling_plan_group_mutation(
        &mut self,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let Some(field) = mutation_root_field(query, variables, root_field) else {
            return MutationOutcome::response(json_error(400, "Could not parse GraphQL operation"));
        };
        match root_field {
            "sellingPlanGroupCreate" => self.selling_plan_group_create(&field),
            "sellingPlanGroupUpdate" => self.selling_plan_group_update(&field),
            "sellingPlanGroupDelete" => self.selling_plan_group_delete(&field),
            "sellingPlanGroupAddProducts" => {
                self.selling_plan_group_add_resources(&field, ResourceKind::Product)
            }
            "sellingPlanGroupAddProductVariants" => {
                self.selling_plan_group_add_resources(&field, ResourceKind::ProductVariant)
            }
            "sellingPlanGroupRemoveProducts" => {
                self.selling_plan_group_remove_resources(&field, ResourceKind::Product)
            }
            "sellingPlanGroupRemoveProductVariants" => {
                self.selling_plan_group_remove_resources(&field, ResourceKind::ProductVariant)
            }
            "productJoinSellingPlanGroups" => {
                self.resource_join_leave_selling_plan_groups(&field, ResourceKind::Product, true)
            }
            "productLeaveSellingPlanGroups" => {
                self.resource_join_leave_selling_plan_groups(&field, ResourceKind::Product, false)
            }
            "productVariantJoinSellingPlanGroups" => self.resource_join_leave_selling_plan_groups(
                &field,
                ResourceKind::ProductVariant,
                true,
            ),
            "productVariantLeaveSellingPlanGroups" => self.resource_join_leave_selling_plan_groups(
                &field,
                ResourceKind::ProductVariant,
                false,
            ),
            _ => MutationOutcome::response(json_error(
                400,
                "No mutation dispatcher implemented for selling-plan group root",
            )),
        }
    }

    fn selling_plan_group_create(&mut self, field: &RootFieldSelection) -> MutationOutcome {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let payload_selection = &field.selection;
        let fail = |proxy: &Self, user_errors: Vec<Value>, notes: &'static str| {
            proxy.selling_plan_failed_outcome(
                &field.name,
                selling_plan_group_payload(
                    None,
                    user_errors,
                    payload_selection,
                    &field.response_key,
                    proxy,
                ),
                notes,
            )
        };
        let user_errors =
            selling_plan_group_input_user_errors(&input, SellingPlanInputMode::Create);
        if !user_errors.is_empty() {
            return fail(
                self,
                user_errors,
                "Selling plan group input validation failed; original raw mutation retained for observability.",
            );
        }

        let resources = resolved_object_field(&field.arguments, "resources").unwrap_or_default();
        let product_ids = list_string_field(&resources, "productIds");
        let product_variant_ids = list_string_field(&resources, "productVariantIds");
        if let Some(error) = self.resource_existence_error(&product_ids, ResourceKind::Product) {
            return fail(
                self,
                vec![error],
                "Selling plan group resource validation failed; original raw mutation retained for observability.",
            );
        }
        if let Some(error) =
            self.resource_existence_error(&product_variant_ids, ResourceKind::ProductVariant)
        {
            return fail(
                self,
                vec![error],
                "Selling plan group resource validation failed; original raw mutation retained for observability.",
            );
        }

        let id = self.next_proxy_synthetic_gid("SellingPlanGroup");
        let created_at = self.next_product_timestamp();
        let selling_plans = resolved_object_list_field(&input, "sellingPlansToCreate")
            .into_iter()
            .enumerate()
            .map(|(index, plan_input)| {
                let plan_id = self.next_proxy_synthetic_gid("SellingPlan");
                selling_plan_record_from_input(plan_id, &created_at, index, &plan_input)
            })
            .collect::<Vec<_>>();
        let name = resolved_string_field(&input, "name").unwrap_or_default();
        let mut unique_product_ids = Vec::new();
        extend_unique_strings(&mut unique_product_ids, product_ids);
        let mut unique_product_variant_ids = Vec::new();
        extend_unique_strings(&mut unique_product_variant_ids, product_variant_ids);

        let group = SellingPlanGroupRecord {
            id: id.clone(),
            app_id: resolved_string_field(&input, "appId"),
            merchant_code: resolved_string_field(&input, "merchantCode")
                .unwrap_or_else(|| name.clone()),
            description: resolved_string_field(&input, "description").unwrap_or_default(),
            options: list_string_field(&input, "options"),
            position: resolved_int_field(&input, "position").unwrap_or(1),
            created_at,
            name,
            selling_plans,
            product_ids: unique_product_ids,
            product_variant_ids: unique_product_variant_ids,
        };
        self.store.stage_selling_plan_group(group.clone());

        MutationOutcome::staged(
            selling_plan_group_payload(
                Some(&group),
                Vec::new(),
                payload_selection,
                &field.response_key,
                self,
            ),
            LogDraft::staged(&field.name, "products", vec![id]),
        )
    }

    fn selling_plan_group_update(&mut self, field: &RootFieldSelection) -> MutationOutcome {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let payload_selection = &field.selection;
        let Some(mut group) = self.store.selling_plan_group_by_id(&id).cloned() else {
            return self.selling_plan_failed_outcome(
                &field.name,
                selling_plan_group_update_payload(
                    None,
                    None,
                    vec![group_does_not_exist_error()],
                    payload_selection,
                    &field.response_key,
                    self,
                ),
                "Selling plan group update targeted an unknown group; original raw mutation retained for observability.",
            );
        };

        let user_errors =
            selling_plan_group_input_user_errors(&input, SellingPlanInputMode::Update);
        if !user_errors.is_empty() {
            return self.selling_plan_failed_outcome(
                &field.name,
                selling_plan_group_update_payload(
                    None,
                    None,
                    user_errors,
                    payload_selection,
                    &field.response_key,
                    self,
                ),
                "Selling plan group input validation failed; original raw mutation retained for observability.",
            );
        }

        if let Some(name) = resolved_string_field(&input, "name") {
            group.name = name;
        }
        if input.contains_key("appId") {
            group.app_id = resolved_string_field(&input, "appId");
        }
        if let Some(merchant_code) = resolved_string_field(&input, "merchantCode") {
            group.merchant_code = merchant_code;
        }
        if let Some(description) = resolved_string_field(&input, "description") {
            group.description = description;
        }
        if input.contains_key("options") {
            group.options = list_string_field(&input, "options");
        }
        if let Some(position) = resolved_int_field(&input, "position") {
            group.position = position;
        }

        let mut deleted_plan_ids = Vec::new();
        for id in list_string_field(&input, "sellingPlansToDelete") {
            if let Some(index) = group.selling_plans.iter().position(|plan| plan.id == id) {
                deleted_plan_ids.push(group.selling_plans.remove(index).id);
            }
        }
        for plan_input in resolved_object_list_field(&input, "sellingPlansToUpdate") {
            let Some(plan_id) = resolved_string_field(&plan_input, "id") else {
                continue;
            };
            if let Some(plan) = group
                .selling_plans
                .iter_mut()
                .find(|plan| plan.id == plan_id)
            {
                apply_selling_plan_update(plan, &plan_input);
            }
        }
        let created_at = group.created_at.clone();
        for plan_input in resolved_object_list_field(&input, "sellingPlansToCreate") {
            let plan_id = self.next_proxy_synthetic_gid("SellingPlan");
            let index = group.selling_plans.len();
            group.selling_plans.push(selling_plan_record_from_input(
                plan_id,
                &created_at,
                index,
                &plan_input,
            ));
        }
        self.store.stage_selling_plan_group(group.clone());

        MutationOutcome::staged(
            selling_plan_group_update_payload(
                Some(&group),
                Some(deleted_plan_ids),
                Vec::new(),
                payload_selection,
                &field.response_key,
                self,
            ),
            LogDraft::staged(&field.name, "products", vec![id]),
        )
    }

    fn selling_plan_group_delete(&mut self, field: &RootFieldSelection) -> MutationOutcome {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let payload_selection = &field.selection;
        let existed = self.store.delete_selling_plan_group(&id);
        let (deleted_id, user_errors, log_draft) = if existed {
            (
                Some(id.clone()),
                Vec::new(),
                LogDraft::staged(&field.name, "products", vec![id]),
            )
        } else {
            (
                None,
                vec![group_does_not_exist_error()],
                LogDraft::failed(
                    &field.name,
                    "products",
                    "Selling plan group delete targeted an unknown group; original raw mutation retained for observability.",
                ),
            )
        };
        MutationOutcome::staged(
            selling_plan_group_delete_payload(
                deleted_id.as_deref(),
                user_errors,
                payload_selection,
                &field.response_key,
            ),
            log_draft,
        )
    }

    fn selling_plan_group_add_resources(
        &mut self,
        field: &RootFieldSelection,
        resource_kind: ResourceKind,
    ) -> MutationOutcome {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let ids = list_string_field(&field.arguments, resource_kind.ids_arg());
        let payload_selection = &field.selection;
        let fail = |proxy: &Self, user_errors: Vec<Value>, notes: &'static str| {
            proxy.selling_plan_failed_outcome(
                &field.name,
                selling_plan_group_payload(
                    None,
                    user_errors,
                    payload_selection,
                    &field.response_key,
                    proxy,
                ),
                notes,
            )
        };
        let Some(mut group) = self.store.selling_plan_group_by_id(&id).cloned() else {
            return fail(
                self,
                vec![group_does_not_exist_error()],
                "Selling plan group add targeted an unknown group; original raw mutation retained for observability.",
            );
        };
        if let Some(error) = self.resource_existence_error(&ids, resource_kind) {
            return fail(
                self,
                vec![error],
                "Selling plan group membership validation failed; original raw mutation retained for observability.",
            );
        }
        let members = resource_members_mut(&mut group, resource_kind);
        if ids.iter().any(|resource_id| members.contains(resource_id)) {
            return fail(
                self,
                vec![user_error(
                    [resource_kind.ids_arg()],
                    "Resource has already been taken",
                    Some("TAKEN"),
                )],
                "Selling plan group membership validation failed; original raw mutation retained for observability.",
            );
        }
        if self.resource_membership_count_after_add(resource_kind, &ids)
            > MAX_SELLING_PLAN_GROUPS_PER_RESOURCE
        {
            return fail(
                self,
                vec![too_many_groups_error(resource_kind.ids_arg())],
                "Selling plan group membership cap validation failed; original raw mutation retained for observability.",
            );
        }

        extend_unique_strings(members, ids.clone());
        self.store.stage_selling_plan_group(group.clone());
        let mut staged_ids = vec![group.id.clone()];
        staged_ids.extend(ids);
        MutationOutcome::staged(
            selling_plan_group_payload(
                Some(&group),
                Vec::new(),
                payload_selection,
                &field.response_key,
                self,
            ),
            LogDraft::staged(&field.name, "products", staged_ids),
        )
    }

    fn selling_plan_group_remove_resources(
        &mut self,
        field: &RootFieldSelection,
        resource_kind: ResourceKind,
    ) -> MutationOutcome {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let ids = list_string_field(&field.arguments, resource_kind.ids_arg());
        let payload_selection = &field.selection;
        let Some(mut group) = self.store.selling_plan_group_by_id(&id).cloned() else {
            return self.selling_plan_failed_outcome(
                &field.name,
                group_remove_payload(
                    None,
                    vec![group_does_not_exist_error()],
                    resource_kind,
                    payload_selection,
                    &field.response_key,
                ),
                "Selling plan group remove targeted an unknown group; original raw mutation retained for observability.",
            );
        };

        let members = resource_members_mut(&mut group, resource_kind);
        let mut removed = Vec::new();
        for resource_id in ids {
            if let Some(position) = members.iter().position(|member| member == &resource_id) {
                removed.push(members.remove(position));
            }
        }
        self.store.stage_selling_plan_group(group);
        MutationOutcome::staged(
            group_remove_payload(
                Some(removed.clone()),
                Vec::new(),
                resource_kind,
                payload_selection,
                &field.response_key,
            ),
            LogDraft::staged(&field.name, "products", removed),
        )
    }

    fn resource_join_leave_selling_plan_groups(
        &mut self,
        field: &RootFieldSelection,
        resource_kind: ResourceKind,
        is_join: bool,
    ) -> MutationOutcome {
        let resource_id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let group_ids = list_string_field(&field.arguments, "sellingPlanGroupIds");
        let payload_selection = &field.selection;
        let response = |proxy: &Self, user_errors: Vec<Value>| {
            resource_join_leave_payload(
                proxy,
                field,
                resource_kind,
                &resource_id,
                user_errors,
                payload_selection,
            )
        };

        let user_errors =
            self.join_leave_preflight_errors(resource_kind, &resource_id, &group_ids, is_join);
        if !user_errors.is_empty() {
            return self.selling_plan_failed_outcome(
                &field.name,
                response(self, user_errors),
                "Selling plan group join/leave validation failed; original raw mutation retained for observability.",
            );
        }

        for group_id in &group_ids {
            let Some(mut group) = self.store.selling_plan_group_by_id(group_id).cloned() else {
                continue;
            };
            let members = resource_members_mut(&mut group, resource_kind);
            if is_join {
                push_unique_string(members, &resource_id);
            } else if let Some(position) = members.iter().position(|member| member == &resource_id)
            {
                members.remove(position);
            }
            self.store.stage_selling_plan_group(group);
        }

        let mut staged_ids = vec![resource_id.clone()];
        staged_ids.extend(group_ids);
        MutationOutcome::staged(
            response(self, Vec::new()),
            LogDraft::staged(&field.name, "products", staged_ids),
        )
    }

    fn selling_plan_failed_outcome(
        &self,
        root_field: &str,
        response: Response,
        notes: &'static str,
    ) -> MutationOutcome {
        MutationOutcome::staged(response, LogDraft::failed(root_field, "products", notes))
    }

    fn resource_existence_error(
        &mut self,
        ids: &[String],
        resource_kind: ResourceKind,
    ) -> Option<Value> {
        let missing = ids
            .iter()
            .filter(|id| !self.has_resource(id, resource_kind))
            .cloned()
            .collect::<Vec<_>>();
        if !missing.is_empty() && self.config.read_mode != ReadMode::Snapshot {
            self.hydrate_product_nodes_for_observation(missing);
        }
        ids.iter()
            .find(|id| !self.has_resource(id, resource_kind))
            .map(|id| {
                user_error(
                    [resource_kind.ids_arg()],
                    &format!("{} {} does not exist.", resource_kind.label(), id),
                    Some("NOT_FOUND"),
                )
            })
    }

    fn has_resource(&self, id: &str, resource_kind: ResourceKind) -> bool {
        match resource_kind {
            ResourceKind::Product => self.store.product_by_id(id).is_some(),
            ResourceKind::ProductVariant => self.store.product_variant_by_id(id).is_some(),
        }
    }

    /// Decide whether a selling-plan mutation can be served authentically from
    /// local staged state. A lifecycle scenario stages its product/group locally
    /// before operating on it, so we own the truth and answer locally. A
    /// validation scenario instead exercises Shopify's own error behaviour
    /// against live-store products/groups that were never staged here; for those
    /// we must forward upstream rather than fabricate an inaccurate
    /// NOT_FOUND/GROUP_DOES_NOT_EXIST userError from empty local state.
    pub(in crate::proxy) fn selling_plan_mutation_serves_locally(
        &self,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> bool {
        let Some(field) = mutation_root_field(query, variables, root_field) else {
            // Unparseable: let the local handler surface the parse error.
            return true;
        };
        match root_field {
            "sellingPlanGroupCreate" => {
                let resources =
                    resolved_object_field(&field.arguments, "resources").unwrap_or_default();
                let product_ids = list_string_field(&resources, "productIds");
                let variant_ids = list_string_field(&resources, "productVariantIds");
                product_ids
                    .iter()
                    .all(|id| self.has_resource(id, ResourceKind::Product))
                    && variant_ids
                        .iter()
                        .all(|id| self.has_resource(id, ResourceKind::ProductVariant))
            }
            "sellingPlanGroupUpdate"
            | "sellingPlanGroupDelete"
            | "sellingPlanGroupAddProducts"
            | "sellingPlanGroupRemoveProducts"
            | "sellingPlanGroupAddProductVariants"
            | "sellingPlanGroupRemoveProductVariants" => {
                let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                self.store.selling_plan_group_by_id(&id).is_some()
            }
            "productJoinSellingPlanGroups" | "productLeaveSellingPlanGroups" => {
                let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                self.has_resource(&id, ResourceKind::Product)
            }
            "productVariantJoinSellingPlanGroups" | "productVariantLeaveSellingPlanGroups" => {
                let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                self.has_resource(&id, ResourceKind::ProductVariant)
            }
            _ => true,
        }
    }

    fn join_leave_preflight_errors(
        &self,
        resource_kind: ResourceKind,
        resource_id: &str,
        group_ids: &[String],
        is_join: bool,
    ) -> Vec<Value> {
        if group_ids.is_empty() {
            return vec![presence_user_error(
                ["sellingPlanGroupIds"],
                "Selling plan group IDs",
            )];
        }
        let mut seen = BTreeSet::new();
        if group_ids.iter().any(|id| !seen.insert(id)) {
            return vec![user_error(
                ["sellingPlanGroupIds"],
                "Selling plan group IDs contains duplicate values.",
                Some("DUPLICATE"),
            )];
        }
        if !self.has_resource(resource_id, resource_kind) {
            return vec![user_error(
                ["id"],
                &format!("{} does not exist.", resource_kind.label()),
                Some("NOT_FOUND"),
            )];
        }
        if let Some(_missing_group) = group_ids
            .iter()
            .find(|id| self.store.selling_plan_group_by_id(id).is_none())
        {
            return vec![user_error(
                ["sellingPlanGroupIds"],
                "Selling plan group does not exist.",
                Some("GROUP_DOES_NOT_EXIST"),
            )];
        }
        if is_join {
            let current = self.direct_group_ids_for_resource(resource_kind, resource_id);
            let additions = group_ids
                .iter()
                .filter(|group_id| !current.contains(*group_id))
                .count();
            if current.len() + additions > MAX_SELLING_PLAN_GROUPS_PER_RESOURCE {
                return vec![too_many_groups_error("sellingPlanGroupIds")];
            }
        } else if group_ids.iter().any(|group_id| {
            self.store
                .selling_plan_group_by_id(group_id)
                .is_some_and(|group| {
                    !resource_members(group, resource_kind).contains(&resource_id.to_string())
                })
        }) {
            return vec![user_error(
                ["sellingPlanGroupIds"],
                "Selling plan group is not a member.",
                Some("NOT_A_MEMBER"),
            )];
        }
        Vec::new()
    }

    fn resource_membership_count_after_add(
        &self,
        resource_kind: ResourceKind,
        resource_ids: &[String],
    ) -> usize {
        resource_ids
            .iter()
            .map(|resource_id| {
                self.direct_group_ids_for_resource(resource_kind, resource_id)
                    .len()
                    + 1
            })
            .max()
            .unwrap_or(0)
    }

    fn direct_group_ids_for_resource(
        &self,
        resource_kind: ResourceKind,
        resource_id: &str,
    ) -> BTreeSet<String> {
        self.store
            .selling_plan_groups()
            .into_iter()
            .filter(|group| {
                resource_members(group, resource_kind)
                    .iter()
                    .any(|id| id == resource_id)
            })
            .map(|group| group.id)
            .collect()
    }

    fn selling_plan_groups_for_product(&self, product_id: &str) -> Vec<SellingPlanGroupRecord> {
        self.store
            .selling_plan_groups()
            .into_iter()
            .filter(|group| group.product_ids.iter().any(|id| id == product_id))
            .collect()
    }

    fn selling_plan_groups_for_nodes_matching(
        &self,
        predicate: impl Fn(&SellingPlanGroupRecord) -> bool,
    ) -> Vec<SellingPlanGroupRecord> {
        let mut groups = Vec::new();
        let mut seen = BTreeSet::new();
        for group in self.store.selling_plan_groups() {
            if predicate(&group) && seen.insert(group.id.clone()) {
                groups.push(group);
            }
        }
        groups
    }

    fn selling_plan_group_json(
        &self,
        group: &SellingPlanGroupRecord,
        selections: &[SelectedField],
    ) -> Value {
        selected_payload_json(selections, |selection| match selection.name.as_str() {
            "__typename" => Some(json!("SellingPlanGroup")),
            "id" => Some(json!(group.id)),
            "appId" => Some(group.app_id.clone().map_or(Value::Null, Value::String)),
            "name" => Some(json!(group.name)),
            "merchantCode" => Some(json!(group.merchant_code)),
            "description" => Some(json!(group.description)),
            "options" => Some(json!(group.options)),
            "position" => Some(json!(group.position)),
            "summary" => Some(json!(selling_plan_group_summary(group))),
            "createdAt" => Some(json!(group.created_at)),
            "productsCount" => Some(product_count_json(
                group.product_ids.len(),
                &selection.selection,
            )),
            "productVariantsCount" => Some(product_count_json(
                group.product_variant_ids.len(),
                &selection.selection,
            )),
            "appliesToProduct" => {
                let id =
                    resolved_string_field(&selection.arguments, "productId").unwrap_or_default();
                Some(json!(group
                    .product_ids
                    .iter()
                    .any(|product_id| product_id == &id)))
            }
            "appliesToProductVariant" => {
                let id = resolved_string_field(&selection.arguments, "productVariantId")
                    .unwrap_or_default();
                Some(json!(group
                    .product_variant_ids
                    .iter()
                    .any(|variant_id| variant_id == &id)))
            }
            "appliesToProductVariants" => {
                let product_id =
                    resolved_string_field(&selection.arguments, "productId").unwrap_or_default();
                let applies = self
                    .store
                    .product_variants_for_product(&product_id)
                    .iter()
                    .any(|variant| group.product_variant_ids.iter().any(|id| id == &variant.id));
                Some(json!(applies))
            }
            "products" => {
                let products = group
                    .product_ids
                    .iter()
                    .filter_map(|id| self.store.product_by_id(id).cloned())
                    .collect::<Vec<_>>();
                Some(selected_typed_connection_with_args(
                    &products,
                    &selection.arguments,
                    &selection.selection,
                    product_json,
                    |product| product.id.clone(),
                ))
            }
            "productVariants" => {
                let variants = group
                    .product_variant_ids
                    .iter()
                    .filter_map(|id| self.store.product_variant_by_id(id).cloned())
                    .collect::<Vec<_>>();
                Some(selected_typed_connection_with_args(
                    &variants,
                    &selection.arguments,
                    &selection.selection,
                    |variant, selections| {
                        product_variant_json(
                            variant,
                            self.store.product_by_id(&variant.product_id),
                            selections,
                        )
                    },
                    |variant| variant.id.clone(),
                ))
            }
            "sellingPlans" => Some(selected_typed_connection_with_args(
                &group.selling_plans,
                &selection.arguments,
                &selection.selection,
                selling_plan_json,
                |plan| plan.id.clone(),
            )),
            _ => None,
        })
    }

    pub(in crate::proxy) fn product_json_with_selling_plan_overlay(
        &self,
        product: &ProductRecord,
        variants: &[ProductVariantRecord],
        selections: &[SelectedField],
    ) -> Value {
        let base = product_json_with_variants_and_currency(
            product,
            variants,
            selections,
            &self.store.shop_currency_code(),
        );
        let variant_ids = self
            .store
            .product_variants_for_product(&product.id)
            .into_iter()
            .map(|variant| variant.id)
            .collect::<BTreeSet<_>>();
        let groups = self.selling_plan_groups_for_nodes_matching(|group| {
            group.product_ids.iter().any(|id| id == &product.id)
                || group
                    .product_variant_ids
                    .iter()
                    .any(|id| variant_ids.contains(id))
        });
        let count = self.selling_plan_groups_for_product(&product.id).len();
        self.apply_selling_plan_overlay(selections, base, groups, count)
    }

    pub(in crate::proxy) fn product_variant_json_with_selling_plan_overlay(
        &self,
        variant: &ProductVariantRecord,
        product: Option<&ProductRecord>,
        selections: &[SelectedField],
    ) -> Value {
        let base = product_variant_json(variant, product, selections);
        let groups = self.selling_plan_groups_for_nodes_matching(|group| {
            group.product_ids.iter().any(|id| id == &variant.product_id)
                || group.product_variant_ids.iter().any(|id| id == &variant.id)
        });
        let count = self
            .direct_group_ids_for_resource(ResourceKind::ProductVariant, &variant.id)
            .len();
        self.apply_selling_plan_overlay(selections, base, groups, count)
    }

    fn apply_selling_plan_overlay(
        &self,
        selections: &[SelectedField],
        base: Value,
        groups: Vec<SellingPlanGroupRecord>,
        count: usize,
    ) -> Value {
        let mut object = base.as_object().cloned().unwrap_or_default();
        for selection in selections {
            let value = match selection.name.as_str() {
                "sellingPlanGroups" => Some(selected_typed_connection_with_args(
                    &groups,
                    &selection.arguments,
                    &selection.selection,
                    selling_plan_group_summary_json,
                    |group| group.id.clone(),
                )),
                "sellingPlanGroupsCount" => Some(product_count_json(count, &selection.selection)),
                _ => None,
            };
            if let Some(value) = value {
                object.insert(selection.response_key.clone(), value);
            }
        }
        Value::Object(object)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SellingPlanInputMode {
    Create,
    Update,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ResourceKind {
    Product,
    ProductVariant,
}

impl ResourceKind {
    fn ids_arg(self) -> &'static str {
        match self {
            Self::Product => "productIds",
            Self::ProductVariant => "productVariantIds",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Product => "Product",
            Self::ProductVariant => "Product variant",
        }
    }

    fn payload_resource_field(self) -> &'static str {
        match self {
            Self::Product => "product",
            Self::ProductVariant => "productVariant",
        }
    }
}

fn mutation_root_field(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
    root_field: &str,
) -> Option<RootFieldSelection> {
    root_fields(query, variables)?
        .into_iter()
        .find(|field| field.name == root_field)
}

fn selling_plan_group_input_user_errors(
    input: &BTreeMap<String, ResolvedValue>,
    mode: SellingPlanInputMode,
) -> Vec<Value> {
    let mut errors = Vec::new();
    if list_string_field(input, "options").len() > 3 {
        errors.push(user_error(
            ["input", "options"],
            "Too many selling plan group options (maximum 3 options)",
            Some("TOO_LONG"),
        ));
    }
    if let Some(position) = resolved_int_field(input, "position") {
        if !is_int32(position) {
            errors.push(position_invalid_error(vec!["input", "position"]));
        }
    }
    let create_field = "sellingPlansToCreate";
    for (index, plan) in resolved_object_list_field(input, create_field)
        .iter()
        .enumerate()
    {
        errors.extend(selling_plan_input_user_errors(
            plan,
            mode,
            create_field,
            index,
        ));
    }
    if mode == SellingPlanInputMode::Update {
        for (index, plan) in resolved_object_list_field(input, "sellingPlansToUpdate")
            .iter()
            .enumerate()
        {
            if resolved_string_field(plan, "id").is_none() {
                errors.push(user_error(
                    vec![
                        "input".to_string(),
                        "sellingPlansToUpdate".to_string(),
                        index.to_string(),
                        "id".to_string(),
                    ],
                    "Id must be specificed to update a Selling Plan.",
                    Some("PLAN_ID_MUST_BE_SPECIFIED_TO_UPDATE"),
                ));
            }
            errors.extend(selling_plan_input_user_errors(
                plan,
                mode,
                "sellingPlansToUpdate",
                index,
            ));
        }
    }
    if errors.is_empty() && mode == SellingPlanInputMode::Create {
        errors.extend(selling_plan_group_create_model_user_errors(input));
    }
    errors
}

fn selling_plan_group_create_model_user_errors(
    input: &BTreeMap<String, ResolvedValue>,
) -> Vec<Value> {
    let mut errors = Vec::new();
    if resolved_string_field(input, "name")
        .as_deref()
        .is_none_or(|name| name.trim().is_empty())
    {
        errors.push(presence_user_error(["input", "name"], "Name"));
    }

    let plans = resolved_object_list_field(input, "sellingPlansToCreate");
    if plans.len() > MAX_SELLING_PLANS_PER_GROUP {
        errors.push(user_error(
            ["input"],
            "Selling plan groups can't have more than 31 selling plans.",
            Some("SELLING_PLAN_COUNT_UPPER_BOUND"),
        ));
    } else if plans.is_empty() {
        errors.push(user_error(
            ["input"],
            "Selling plan groups must have at least 1 selling plan.",
            Some("SELLING_PLAN_COUNT_LOWER_BOUND"),
        ));
    }

    for (index, plan) in plans.iter().enumerate() {
        let index = index.to_string();
        if resolved_object_field(plan, "billingPolicy").is_none() {
            errors.push(user_error(
                [
                    "input",
                    "sellingPlansToCreate",
                    index.as_str(),
                    "billingPolicy",
                ],
                "Selling plans to create billing policy must be present.",
                Some("SELLING_PLAN_BILLING_POLICY_MISSING"),
            ));
        }
        if resolved_object_field(plan, "deliveryPolicy").is_none() {
            errors.push(user_error(
                [
                    "input",
                    "sellingPlansToCreate",
                    index.as_str(),
                    "deliveryPolicy",
                ],
                "Selling plans to create delivery policy must be present.",
                Some("SELLING_PLAN_DELIVERY_POLICY_MISSING"),
            ));
        }
    }

    errors
}

fn selling_plan_input_user_errors(
    plan: &BTreeMap<String, ResolvedValue>,
    mode: SellingPlanInputMode,
    list_field: &str,
    index: usize,
) -> Vec<Value> {
    let mut errors = Vec::new();
    let index = index.to_string();
    if list_string_field(plan, "options").len() > 3 {
        errors.push(user_error(
            vec![
                "input".to_string(),
                list_field.to_string(),
                index.clone(),
                "options".to_string(),
            ],
            "Too many selling plan options (maximum 3 options)",
            Some("TOO_LONG"),
        ));
    }
    let pricing_policies = resolved_object_list_field(plan, "pricingPolicies");
    if pricing_policies.len() > 2 {
        let message = match mode {
            SellingPlanInputMode::Create => {
                "Selling plans to create pricing policies can't have more than 2 pricing policies"
            }
            SellingPlanInputMode::Update => {
                "Selling plans to update pricing policies can't have more than 2 pricing policies"
            }
        };
        errors.push(user_error(
            vec![
                "input".to_string(),
                list_field.to_string(),
                index.clone(),
                "pricingPolicies".to_string(),
            ],
            message,
            Some("SELLING_PLAN_PRICING_POLICIES_LIMIT"),
        ));
    }
    if let Some(position) = resolved_int_field(plan, "position") {
        if !is_int32(position) {
            errors.push(position_invalid_error(vec![
                "input".to_string(),
                list_field.to_string(),
                index.clone(),
                "position".to_string(),
            ]));
        }
    }

    let billing_policy = resolved_object_field(plan, "billingPolicy").unwrap_or_default();
    let delivery_policy = resolved_object_field(plan, "deliveryPolicy").unwrap_or_default();
    let billing_recurring = resolved_object_field(&billing_policy, "recurring");
    let delivery_recurring = resolved_object_field(&delivery_policy, "recurring");
    let delivery_fixed = resolved_object_field(&delivery_policy, "fixed");
    if billing_recurring.is_some() && delivery_fixed.is_some() {
        match mode {
            SellingPlanInputMode::Create => errors.push(user_error(
                vec!["input".to_string(), list_field.to_string(), index.clone()],
                "billing and delivery policy types must be the same.",
                Some("BILLING_AND_DELIVERY_POLICY_TYPES_MUST_BE_THE_SAME"),
            )),
            SellingPlanInputMode::Update => errors.push(user_error(
                vec!["input".to_string(), list_field.to_string(), index.clone()],
                "Only one of fixed or recurring delivery policy is allowed",
                Some("ONLY_ONE_OF_FIXED_OR_RECURRING_DELIVERY"),
            )),
        }
    }
    if let Some(delivery_recurring) = delivery_recurring.as_ref() {
        if resolved_int_field(delivery_recurring, "intervalCount") == Some(0) {
            errors.push(user_error(
                selling_plan_recurring_field_path(
                    list_field,
                    &index,
                    "deliveryPolicy",
                    "intervalCount",
                ),
                "Interval count must be greater than 0",
                Some("GREATER_THAN"),
            ));
        }
        if let Some(cutoff) = resolved_int_field(delivery_recurring, "cutoff") {
            if !is_non_negative_int32(cutoff) {
                errors.push(range_invalid_error(
                    selling_plan_recurring_field_path(
                        list_field,
                        &index,
                        "deliveryPolicy",
                        "cutoff",
                    ),
                    "Cutoff must be within the range of 0 to 2,147,483,647",
                ));
            }
        }
    }
    if let Some(billing_recurring) = billing_recurring.as_ref() {
        if resolved_int_field(billing_recurring, "intervalCount") == Some(0) {
            errors.push(user_error(
                selling_plan_recurring_field_path(
                    list_field,
                    &index,
                    "billingPolicy",
                    "intervalCount",
                ),
                "Interval count must be greater than 0",
                Some("GREATER_THAN"),
            ));
        }
        if let Some(min_cycles) = resolved_int_field(billing_recurring, "minCycles") {
            if !is_positive_int32(min_cycles) {
                errors.push(range_invalid_error(
                    selling_plan_recurring_field_path(
                        list_field,
                        &index,
                        "billingPolicy",
                        "minCycles",
                    ),
                    "Min cycles must be within the range of 1 to 2,147,483,647",
                ));
            }
        }
        if let Some(max_cycles) = resolved_int_field(billing_recurring, "maxCycles") {
            if !is_positive_int32(max_cycles) {
                errors.push(range_invalid_error(
                    selling_plan_recurring_field_path(
                        list_field,
                        &index,
                        "billingPolicy",
                        "maxCycles",
                    ),
                    "Max cycles must be within the range of 1 to 2,147,483,647",
                ));
            }
        }
    }
    if let (Some(billing_recurring), Some(delivery_recurring)) =
        (billing_recurring.as_ref(), delivery_recurring.as_ref())
    {
        if resolved_value_json(
            billing_recurring
                .get("anchors")
                .unwrap_or(&ResolvedValue::Null),
        ) != resolved_value_json(
            delivery_recurring
                .get("anchors")
                .unwrap_or(&ResolvedValue::Null),
        ) {
            errors.push(user_error(
                vec!["input".to_string(), list_field.to_string(), index],
                "Billing and delivery policy anchors must be the same",
                Some("SELLING_PLAN_BILLING_AND_DELIVERY_POLICY_ANCHORS_MUST_BE_EQUAL"),
            ));
        }
    }
    errors
}

fn is_int32(value: i64) -> bool {
    (INT32_MIN..=INT32_MAX).contains(&value)
}

fn is_positive_int32(value: i64) -> bool {
    (1..=INT32_MAX).contains(&value)
}

fn is_non_negative_int32(value: i64) -> bool {
    (0..=INT32_MAX).contains(&value)
}

fn position_invalid_error(field: Vec<impl Into<String>>) -> Value {
    let field = field.into_iter().map(Into::into).collect::<Vec<String>>();
    user_error(
        field,
        "Position must be within the range of -2,147,483,648 to 2,147,483,647",
        Some("INVALID"),
    )
}

fn range_invalid_error(field: Vec<String>, message: &str) -> Value {
    user_error(field, message, Some("INVALID"))
}

fn selling_plan_recurring_field_path(
    list_field: &str,
    index: &str,
    policy_field: &str,
    recurring_field: &str,
) -> Vec<String> {
    vec![
        "input".to_string(),
        list_field.to_string(),
        index.to_string(),
        policy_field.to_string(),
        "recurring".to_string(),
        recurring_field.to_string(),
    ]
}

fn selling_plan_record_from_input(
    id: String,
    created_at: &str,
    index: usize,
    input: &BTreeMap<String, ResolvedValue>,
) -> SellingPlanRecord {
    SellingPlanRecord {
        id,
        name: resolved_string_field(input, "name").unwrap_or_default(),
        description: resolved_string_field(input, "description").unwrap_or_default(),
        options: list_string_field(input, "options"),
        position: resolved_int_field(input, "position").unwrap_or((index + 1) as i64),
        category: resolved_string_field(input, "category").unwrap_or_else(|| "OTHER".to_string()),
        created_at: created_at.to_string(),
        billing_policy: billing_policy_json(&resolved_object_field(input, "billingPolicy")),
        delivery_policy: delivery_policy_json(&resolved_object_field(input, "deliveryPolicy")),
        inventory_policy: inventory_policy_json(&resolved_object_field(input, "inventoryPolicy")),
        pricing_policies: pricing_policies_json(&resolved_object_list_field(
            input,
            "pricingPolicies",
        )),
    }
}

fn apply_selling_plan_update(
    plan: &mut SellingPlanRecord,
    input: &BTreeMap<String, ResolvedValue>,
) {
    if let Some(name) = resolved_string_field(input, "name") {
        plan.name = name;
    }
    if let Some(description) = resolved_string_field(input, "description") {
        plan.description = description;
    }
    if input.contains_key("options") {
        plan.options = list_string_field(input, "options");
    }
    if let Some(position) = resolved_int_field(input, "position") {
        plan.position = position;
    }
    if input.contains_key("category") {
        plan.category = resolved_string_field(input, "category").unwrap_or_default();
    }
    if let Some(policy) = resolved_object_field(input, "billingPolicy") {
        plan.billing_policy = billing_policy_json(&Some(policy));
    }
    if let Some(policy) = resolved_object_field(input, "deliveryPolicy") {
        plan.delivery_policy = delivery_policy_json(&Some(policy));
    }
    if let Some(policy) = resolved_object_field(input, "inventoryPolicy") {
        plan.inventory_policy = inventory_policy_json(&Some(policy));
    }
    plan.pricing_policies =
        pricing_policies_json(&resolved_object_list_field(input, "pricingPolicies"));
}

fn billing_policy_json(policy: &Option<BTreeMap<String, ResolvedValue>>) -> Value {
    let recurring = policy
        .as_ref()
        .and_then(|policy| resolved_object_field(policy, "recurring"));
    if let Some(recurring) = recurring {
        return json!({
            "__typename": "SellingPlanRecurringBillingPolicy",
            "interval": resolved_string_field(&recurring, "interval").unwrap_or_else(|| "MONTH".to_string()),
            "intervalCount": resolved_int_field(&recurring, "intervalCount").unwrap_or(1),
            "minCycles": resolved_int_field(&recurring, "minCycles"),
            "maxCycles": resolved_int_field(&recurring, "maxCycles")
        });
    }
    json!({ "__typename": "SellingPlanFixedBillingPolicy" })
}

fn delivery_policy_json(policy: &Option<BTreeMap<String, ResolvedValue>>) -> Value {
    let recurring = policy
        .as_ref()
        .and_then(|policy| resolved_object_field(policy, "recurring"));
    if let Some(recurring) = recurring {
        return json!({
            "__typename": "SellingPlanRecurringDeliveryPolicy",
            "interval": resolved_string_field(&recurring, "interval").unwrap_or_else(|| "MONTH".to_string()),
            "intervalCount": resolved_int_field(&recurring, "intervalCount").unwrap_or(1),
            "cutoff": resolved_int_field(&recurring, "cutoff").unwrap_or(0),
            "intent": resolved_string_field(&recurring, "intent").unwrap_or_else(|| "FULFILLMENT_BEGIN".to_string()),
            "preAnchorBehavior": resolved_string_field(&recurring, "preAnchorBehavior").unwrap_or_else(|| "ASAP".to_string())
        });
    }
    json!({ "__typename": "SellingPlanFixedDeliveryPolicy" })
}

fn inventory_policy_json(policy: &Option<BTreeMap<String, ResolvedValue>>) -> Value {
    json!({
        "reserve": policy
            .as_ref()
            .and_then(|policy| resolved_string_field(policy, "reserve"))
            .unwrap_or_else(|| "ON_FULFILLMENT".to_string())
    })
}

fn pricing_policies_json(policies: &[BTreeMap<String, ResolvedValue>]) -> Vec<Value> {
    policies
        .iter()
        .filter_map(|policy| {
            let fixed = resolved_object_field(policy, "fixed")?;
            let adjustment_value = resolved_object_field(&fixed, "adjustmentValue").unwrap_or_default();
            let adjustment_type = resolved_string_field(&fixed, "adjustmentType")
                .unwrap_or_else(|| "PERCENTAGE".to_string());
            let adjustment_value_json =
                if let Some(fixed_value) = resolved_decimal_text_field(&adjustment_value, "fixedValue") {
                    json!({
                        "__typename": "MoneyV2",
                        "amount": fixed_value,
                        "currencyCode": "USD"
                    })
                } else {
                    json!({
                        "__typename": "SellingPlanPricingPolicyPercentageValue",
                        "percentage": resolved_number_field(&adjustment_value, "percentage").unwrap_or(0.0)
                    })
                };
            Some(json!({
                "__typename": "SellingPlanFixedPricingPolicy",
                "adjustmentType": adjustment_type,
                "adjustmentValue": adjustment_value_json
            }))
        })
        .collect()
}

fn selling_plan_json(plan: &SellingPlanRecord, selections: &[SelectedField]) -> Value {
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "__typename" => Some(json!("SellingPlan")),
        "id" => Some(json!(plan.id)),
        "name" => Some(json!(plan.name)),
        "description" => Some(json!(plan.description)),
        "options" => Some(json!(plan.options)),
        "position" => Some(json!(plan.position)),
        "category" => Some(json!(plan.category)),
        "createdAt" => Some(json!(plan.created_at)),
        "billingPolicy" => Some(selected_json(&plan.billing_policy, &selection.selection)),
        "deliveryPolicy" => Some(selected_json(&plan.delivery_policy, &selection.selection)),
        "inventoryPolicy" => Some(selected_json(&plan.inventory_policy, &selection.selection)),
        "pricingPolicies" => Some(Value::Array(
            plan.pricing_policies
                .iter()
                .map(|policy| selected_json(policy, &selection.selection))
                .collect(),
        )),
        _ => None,
    })
}

fn selling_plan_group_summary(group: &SellingPlanGroupRecord) -> String {
    let plan_count = group.selling_plans.len();
    if plan_count == 0 {
        return String::new();
    }

    let mut percentages = Vec::new();
    let mut fixed_values = Vec::new();
    for policy in group
        .selling_plans
        .iter()
        .flat_map(|plan| plan.pricing_policies.iter())
    {
        if let Some(percentage) = policy
            .pointer("/adjustmentValue/percentage")
            .and_then(Value::as_f64)
        {
            percentages.push(percentage);
        }
        if let Some(fixed_value) = policy
            .pointer("/adjustmentValue/amount")
            .and_then(json_number_value)
        {
            fixed_values.push(fixed_value);
        }
    }

    let mut discount_pieces = Vec::new();
    if let Some(piece) = percentage_summary_piece(&percentages) {
        discount_pieces.push(piece);
    }
    if let Some(piece) = fixed_value_summary_piece(&fixed_values) {
        discount_pieces.push(piece);
    }
    let discount = discount_pieces.join("·");
    let frequencies = if plan_count == 1 {
        "frequency"
    } else {
        "frequencies"
    };
    format!(
        "{} delivery {}, {} discount",
        plan_count, frequencies, discount
    )
}

fn resolved_decimal_text_field(
    input: &BTreeMap<String, ResolvedValue>,
    key: &str,
) -> Option<String> {
    resolved_decimal_text(input.get(key))
}

fn json_number_value(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::String(value) => value.parse::<f64>().ok(),
        _ => None,
    }
}

fn percentage_summary_piece(values: &[f64]) -> Option<String> {
    let (min, max) = min_max(values)?;
    if (min - max).abs() < f64::EPSILON {
        Some(format!("{}%", format_summary_percentage(min)))
    } else {
        Some(format!(
            "{}-{}%",
            format_summary_percentage(min),
            format_summary_percentage(max)
        ))
    }
}

fn fixed_value_summary_piece(values: &[f64]) -> Option<String> {
    let (min, max) = min_max(values)?;
    if (min - max).abs() < f64::EPSILON {
        Some(format_summary_currency(min))
    } else {
        Some(format!(
            "{}-{}",
            format_summary_currency(min),
            format_summary_currency(max)
        ))
    }
}

fn min_max(values: &[f64]) -> Option<(f64, f64)> {
    let mut values = values.iter().copied();
    let first = values.next()?;
    Some(values.fold((first, first), |(min, max), value| {
        (min.min(value), max.max(value))
    }))
}

fn format_summary_percentage(value: f64) -> String {
    let rounded = (value * 100.0).round() / 100.0;
    let mut formatted = format!("{rounded:.2}");
    while formatted.contains('.') && formatted.ends_with('0') {
        formatted.pop();
    }
    if formatted.ends_with('.') {
        formatted.pop();
    }
    formatted
}

fn format_summary_currency(value: f64) -> String {
    format!("${:.0}", value.round())
}

fn selling_plan_group_summary_json(
    group: &SellingPlanGroupRecord,
    selections: &[SelectedField],
) -> Value {
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "__typename" => Some(json!("SellingPlanGroup")),
        "id" => Some(json!(group.id)),
        "name" => Some(json!(group.name)),
        "merchantCode" => Some(json!(group.merchant_code)),
        _ => None,
    })
}

fn user_errors_value(payload_selection: &[SelectedField], user_errors: &[Value]) -> Value {
    let error_selection =
        selected_child_selection(payload_selection, "userErrors").unwrap_or_default();
    selected_user_errors(user_errors, &error_selection)
}

fn selling_plan_group_payload(
    group: Option<&SellingPlanGroupRecord>,
    user_errors: Vec<Value>,
    payload_selection: &[SelectedField],
    response_key: &str,
    proxy: &DraftProxy,
) -> Response {
    let group_selection =
        selected_child_selection(payload_selection, "sellingPlanGroup").unwrap_or_default();
    ok_json(json!({
        "data": {
            response_key: selected_payload_json(payload_selection, |selection| match selection.name.as_str() {
                "sellingPlanGroup" => Some(group.map(|group| proxy.selling_plan_group_json(group, &group_selection)).unwrap_or(Value::Null)),
                "userErrors" => Some(user_errors_value(payload_selection, user_errors.as_slice())),
                _ => None,
            })
        }
    }))
}

fn selling_plan_group_update_payload(
    group: Option<&SellingPlanGroupRecord>,
    deleted_plan_ids: Option<Vec<String>>,
    user_errors: Vec<Value>,
    payload_selection: &[SelectedField],
    response_key: &str,
    proxy: &DraftProxy,
) -> Response {
    let group_selection =
        selected_child_selection(payload_selection, "sellingPlanGroup").unwrap_or_default();
    ok_json(json!({
        "data": {
            response_key: selected_payload_json(payload_selection, |selection| match selection.name.as_str() {
                "deletedSellingPlanIds" => Some(deleted_plan_ids.clone().map_or(Value::Null, |ids| json!(ids))),
                "sellingPlanGroup" => Some(group.map(|group| proxy.selling_plan_group_json(group, &group_selection)).unwrap_or(Value::Null)),
                "userErrors" => Some(user_errors_value(payload_selection, user_errors.as_slice())),
                _ => None,
            })
        }
    }))
}

fn selling_plan_group_delete_payload(
    deleted_id: Option<&str>,
    user_errors: Vec<Value>,
    payload_selection: &[SelectedField],
    response_key: &str,
) -> Response {
    ok_json(json!({
        "data": {
            response_key: selected_payload_json(payload_selection, |selection| match selection.name.as_str() {
                "deletedSellingPlanGroupId" => Some(deleted_id.map_or(Value::Null, |id| json!(id))),
                "userErrors" => Some(user_errors_value(payload_selection, user_errors.as_slice())),
                _ => None,
            })
        }
    }))
}

fn group_remove_payload(
    removed_ids: Option<Vec<String>>,
    user_errors: Vec<Value>,
    resource_kind: ResourceKind,
    payload_selection: &[SelectedField],
    response_key: &str,
) -> Response {
    let removed_field = match resource_kind {
        ResourceKind::Product => "removedProductIds",
        ResourceKind::ProductVariant => "removedProductVariantIds",
    };
    ok_json(json!({
        "data": {
            response_key: selected_payload_json(payload_selection, |selection| match selection.name.as_str() {
                name if name == removed_field => Some(removed_ids.clone().map_or(Value::Null, |ids| json!(ids))),
                "userErrors" => Some(user_errors_value(payload_selection, user_errors.as_slice())),
                _ => None,
            })
        }
    }))
}

fn resource_join_leave_payload(
    proxy: &DraftProxy,
    field: &RootFieldSelection,
    resource_kind: ResourceKind,
    resource_id: &str,
    user_errors: Vec<Value>,
    payload_selection: &[SelectedField],
) -> Response {
    let resource_selection =
        selected_child_selection(payload_selection, resource_kind.payload_resource_field())
            .unwrap_or_default();
    ok_json(json!({
        "data": {
            field.response_key.as_str(): selected_payload_json(payload_selection, |selection| match selection.name.as_str() {
                "userErrors" => Some(user_errors_value(payload_selection, user_errors.as_slice())),
                "product" if resource_kind == ResourceKind::Product => proxy
                    .store
                    .product_by_id(resource_id)
                    .map(|product| {
                        let variants = proxy.store.product_variants_for_product(&product.id);
                        proxy.product_json_with_selling_plan_overlay(product, &variants, &resource_selection)
                    })
                    .or(Some(Value::Null)),
                "productVariant" if resource_kind == ResourceKind::ProductVariant => proxy
                    .store
                    .product_variant_by_id(resource_id)
                    .map(|variant| {
                        proxy.product_variant_json_with_selling_plan_overlay(
                            variant,
                            proxy.store.product_by_id(&variant.product_id),
                            &resource_selection,
                        )
                    })
                    .or(Some(Value::Null)),
                _ => None,
            })
        }
    }))
}

fn group_does_not_exist_error() -> Value {
    user_error(
        ["id"],
        "Selling plan group does not exist.",
        Some("GROUP_DOES_NOT_EXIST"),
    )
}

fn too_many_groups_error(field: &str) -> Value {
    user_error(
        [field],
        "Exceeded maximum number of selling plan groups per resource.",
        Some("SELLING_PLAN_GROUPS_TOO_MANY"),
    )
}

fn resource_members(group: &SellingPlanGroupRecord, resource_kind: ResourceKind) -> &Vec<String> {
    match resource_kind {
        ResourceKind::Product => &group.product_ids,
        ResourceKind::ProductVariant => &group.product_variant_ids,
    }
}

fn resource_members_mut(
    group: &mut SellingPlanGroupRecord,
    resource_kind: ResourceKind,
) -> &mut Vec<String> {
    match resource_kind {
        ResourceKind::Product => &mut group.product_ids,
        ResourceKind::ProductVariant => &mut group.product_variant_ids,
    }
}

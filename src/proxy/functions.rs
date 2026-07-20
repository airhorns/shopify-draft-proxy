use super::*;
use crate::proxy::search::search_string_matches;
use crate::resolver_registry::OperationRootInvocation;

const FUNCTION_CANONICAL_API_TYPE_FIELD: &str = "__draftProxyCanonicalApiType";

struct FunctionRootInput {
    name: String,
    response_key: String,
    location: SourceLocation,
    raw_arguments: BTreeMap<String, RawArgumentValue>,
    arguments: BTreeMap<String, ResolvedValue>,
    field_selection: Vec<SelectedField>,
}

pub(in crate::proxy) fn function_field_resolver_type_policies() -> Vec<FieldResolverTypePolicy> {
    [
        "CartTransform",
        "FulfillmentConstraintRule",
        "FunctionsAppBridge",
        "FunctionsErrorHistory",
        "ShopifyFunction",
        "Validation",
    ]
    .into_iter()
    .map(|parent_type| {
        FieldResolverTypePolicy::property_backed_ordinary_fields(
            ApiSurface::Admin,
            parent_type,
            "argument-bearing Function field has no explicit canonical resolver",
        )
    })
    .collect()
}

const FUNCTION_HYDRATE_BY_ID_QUERY: &str = "query FunctionHydrateById($id: String!) {\n  shopifyFunction(id: $id) {\n    id\n    title\n    apiType\n    description\n    appKey\n    app {\n      __typename\n      id\n      title\n      apiKey\n    }\n  }\n}\n";
const FUNCTION_HYDRATE_BY_HANDLE_QUERY: &str = "query FunctionHydrateByHandle($handle: String!) {\n  shopifyFunctions(first: 1, handle: $handle) {\n    nodes {\n      id\n      title\n      handle\n      apiType\n      description\n      appKey\n      app {\n        __typename\n        id\n        title\n        handle\n        apiKey\n      }\n    }\n  }\n}\n";
const FUNCTION_VALIDATION_HYDRATE_BY_ID_QUERY: &str = "query FunctionValidationHydrateById($id: ID!) {\n  validation(id: $id) {\n    id\n    title\n    enabled\n    blockOnFailure\n    shopifyFunction {\n      id\n      title\n      handle\n      apiType\n      description\n      appKey\n      app {\n        __typename\n        id\n        title\n        handle\n        apiKey\n      }\n    }\n    metafields(first: 100) {\n      nodes {\n        id\n        namespace\n        key\n        type\n        value\n        updatedAt\n      }\n    }\n  }\n}\n";
const FUNCTION_CART_TRANSFORM_HYDRATE_BY_ID_QUERY: &str = "query FunctionCartTransformHydrateById($id: ID!) {\n  node(id: $id) {\n    ... on CartTransform {\n      id\n      functionId\n      blockOnFailure\n      metafields(first: 100) {\n        nodes {\n          id\n          namespace\n          key\n          type\n          value\n          compareDigest\n          ownerType\n          createdAt\n          updatedAt\n        }\n      }\n    }\n  }\n}\n";

impl DraftProxy {
    pub(crate) fn functions_query_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let operation_roots = invocation.operation_roots.clone();
        let operation_has_local_overlay = operation_roots
            .iter()
            .any(|root| self.function_root_has_local_overlay(&root.name));
        let RootInvocation {
            response_key,
            root_name,
            root_location,
            raw_arguments,
            arguments,
            field_selection,
            request,
            ..
        } = invocation;
        let field = FunctionRootInput {
            name: root_name.to_string(),
            response_key: response_key.to_string(),
            location: root_location,
            raw_arguments,
            arguments: resolved_arguments_from_json(&arguments),
            field_selection,
        };
        if self.config.read_mode != ReadMode::Snapshot {
            let connection_arguments = function_connection_arguments(&field);
            if self.function_root_has_local_overlay(&field.name) {
                if let Some((baseline, effective_arguments)) = self
                    .hydrate_function_connection_after_local_cursor(
                        request,
                        &field,
                        &connection_arguments,
                    )
                {
                    self.observe_function_root_value(
                        request,
                        &field.name,
                        &effective_arguments,
                        &baseline,
                    );
                    let rendered = self.render_function_connection_overlay(
                        request,
                        &field.name,
                        &baseline,
                        &connection_arguments,
                    );
                    self.observe_function_connection(
                        request,
                        &field.name,
                        &connection_arguments,
                        &rendered,
                    );
                    return ResolverOutcome::value(rendered);
                }
            }
            let result = self.cached_or_forward_upstream_graphql_result(request, response_key);
            if !result.transport_succeeded || !result.outcome.errors.is_empty() {
                return result.outcome;
            }
            self.observe_function_operation_roots(request, &operation_roots, &result.data);
            if !operation_has_local_overlay || !self.function_root_has_local_overlay(&field.name) {
                return result.outcome;
            }
            let upstream_value = result.data.get(response_key).cloned();
            return ResolverOutcome::value(self.function_root_value(
                request,
                &field,
                upstream_value.as_ref(),
            ));
        }
        ResolverOutcome::value(self.function_root_value(request, &field, None))
    }

    pub(crate) fn functions_mutation_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let RootInvocation {
            response_key,
            arguments,
            request,
            root_name,
            root_location,
            raw_arguments,
            ..
        } = invocation;
        let field = FunctionRootInput {
            name: root_name.to_string(),
            response_key: response_key.to_string(),
            location: root_location,
            raw_arguments,
            arguments: resolved_arguments_from_json(&arguments),
            field_selection: Vec::new(),
        };
        let (value, errors) = self.function_mutation_value(request, &field);
        let staged = !value.is_null();
        let mut outcome = ResolverOutcome::value(value)
            .with_errors(root_field_errors_from_json(&errors, response_key));
        if staged {
            outcome = outcome.with_log_draft(LogDraft::staged(root_name, "functions", Vec::new()));
        }
        outcome
    }

    fn function_mutation_value(
        &mut self,
        request: &Request,
        field: &FunctionRootInput,
    ) -> (Value, Vec<Value>) {
        let mut errors = Vec::new();
        let value = match field.name.as_str() {
            "validationCreate" => self.function_validation_create_payload(request, field),
            "validationUpdate" => self.function_validation_update_payload(request, field),
            "validationDelete" => self.function_validation_delete_payload(request, field),
            "cartTransformCreate" => self.function_cart_transform_create_payload(request, field),
            "cartTransformDelete" => self.function_cart_transform_delete_payload(request, field),
            "fulfillmentConstraintRuleCreate" => {
                self.function_fulfillment_constraint_rule_create_payload(request, field)
            }
            "fulfillmentConstraintRuleUpdate" => {
                self.function_fulfillment_constraint_rule_update_payload(request, field)
            }
            "fulfillmentConstraintRuleDelete" => {
                self.function_fulfillment_constraint_rule_delete_payload(field)
            }
            "taxAppConfigure" => {
                if tax_app_configure_has_authority(request) {
                    self.function_tax_app_configure_payload(field)
                } else {
                    errors.push(tax_app_configure_access_denied_error(field));
                    Value::Null
                }
            }
            _ => Value::Null,
        };
        (value, errors)
    }

    fn function_root_value(
        &mut self,
        request: &Request,
        field: &FunctionRootInput,
        upstream_value: Option<&Value>,
    ) -> Value {
        let connection_arguments = function_connection_arguments(field);
        match field.name.as_str() {
            "validation" => {
                let id = resolved_string_field(&field.arguments, "id");
                if id.as_deref().is_some_and(|id| {
                    self.store
                        .staged
                        .deleted_function_validation_ids
                        .contains(id)
                }) {
                    Value::Null
                } else {
                    id.and_then(|id| self.function_validation_read_value(&id))
                        .or_else(|| upstream_value.cloned())
                        .unwrap_or(Value::Null)
                }
            }
            "validations" if upstream_value.is_none() => {
                local_function_connection_from_nodes_with_args(
                    self.effective_function_validation_nodes(),
                    &connection_arguments,
                )
            }
            "validations" => self.function_connection_overlay_value(
                request,
                field,
                upstream_value,
                &connection_arguments,
            ),
            "cartTransforms" if upstream_value.is_none() => {
                local_function_connection_from_nodes_with_args(
                    self.effective_function_cart_transform_nodes(),
                    &connection_arguments,
                )
            }
            "cartTransforms" => self.function_connection_overlay_value(
                request,
                field,
                upstream_value,
                &connection_arguments,
            ),
            "fulfillmentConstraintRules" => {
                let mut hydrated = None;
                let baseline_lacks_identity = upstream_value
                    .and_then(Value::as_array)
                    .is_some_and(|rules| rules.iter().any(|rule| rule["id"].as_str().is_none()));
                if baseline_lacks_identity
                    && self.has_function_fulfillment_constraint_rule_overlay_state()
                {
                    hydrated = self.hydrate_function_list_root(request, field);
                    if let Some(value) = hydrated.as_ref() {
                        self.observe_function_root_value(
                            request,
                            &field.name,
                            &field.arguments,
                            value,
                        );
                    }
                }
                self.fulfillment_constraint_rules_overlay_value(
                    hydrated.as_ref().or(upstream_value),
                )
            }
            "shopifyFunctions" => {
                let api_type = requested_function_api_type(&field.arguments);
                upstream_value.cloned().unwrap_or_else(|| {
                    local_function_connection_from_nodes_with_args(
                        self.function_metadata_read_nodes(request, api_type.as_deref()),
                        &connection_arguments,
                    )
                })
            }
            "shopifyFunction" => upstream_value.cloned().unwrap_or_else(|| {
                resolved_string_field(&field.arguments, "id")
                    .map(|id| self.function_metadata_read_value(request, &id))
                    .unwrap_or(Value::Null)
            }),
            _ => Value::Null,
        }
    }

    fn fulfillment_constraint_rules_read_value(&self) -> Value {
        Value::Array(
            self.store
                .base
                .function_fulfillment_constraint_rule_order
                .iter()
                .chain(
                    self.store
                        .staged
                        .function_fulfillment_constraint_rule_order
                        .iter(),
                )
                .filter_map(|id| {
                    if self
                        .store
                        .staged
                        .deleted_function_fulfillment_constraint_rule_ids
                        .contains(id)
                    {
                        return None;
                    }
                    self.store
                        .staged
                        .function_fulfillment_constraint_rules
                        .get(id)
                        .or_else(|| {
                            self.store
                                .base
                                .function_fulfillment_constraint_rules
                                .get(id)
                        })
                        .map(fulfillment_constraint_rule_record_value)
                })
                .collect(),
        )
    }

    fn fulfillment_constraint_rules_overlay_value(&self, upstream_value: Option<&Value>) -> Value {
        let Some(upstream_rules) = upstream_value.and_then(Value::as_array) else {
            return self.fulfillment_constraint_rules_read_value();
        };
        let mut seen = BTreeSet::new();
        let mut rules = Vec::new();
        for upstream in upstream_rules {
            let Some(id) = upstream["id"].as_str() else {
                rules.push(upstream.clone());
                continue;
            };
            seen.insert(id.to_string());
            if self
                .store
                .staged
                .deleted_function_fulfillment_constraint_rule_ids
                .contains(id)
            {
                continue;
            }
            let mut rule = upstream.clone();
            if let Some(staged) = self
                .store
                .staged
                .function_fulfillment_constraint_rules
                .get(id)
            {
                merge_json_values(&mut rule, staged);
            }
            rules.push(fulfillment_constraint_rule_record_value(&rule));
        }
        for id in &self.store.staged.function_fulfillment_constraint_rule_order {
            if seen.contains(id)
                || self
                    .store
                    .staged
                    .deleted_function_fulfillment_constraint_rule_ids
                    .contains(id)
            {
                continue;
            }
            if let Some(rule) = self
                .store
                .staged
                .function_fulfillment_constraint_rules
                .get(id)
            {
                rules.push(fulfillment_constraint_rule_record_value(rule));
            }
        }
        Value::Array(rules)
    }

    fn function_validation_read_value(&self, id: &str) -> Option<Value> {
        if self
            .store
            .staged
            .deleted_function_validation_ids
            .contains(id)
        {
            return None;
        }
        self.function_validation_by_id(id)
            .map(validation_record_value)
    }

    fn function_metadata_read_value(&self, request: &Request, id: &str) -> Value {
        self.function_metadata_by_id_or_handle(Some(id), None)
            .filter(|function| function_belongs_to_request(function, request))
            .unwrap_or(Value::Null)
    }

    fn function_validation_by_id(&self, id: &str) -> Option<&Value> {
        self.store
            .staged
            .function_validations
            .get(id)
            .or_else(|| self.store.base.function_validations.get(id))
            .or_else(|| {
                self.store
                    .staged
                    .function_validation
                    .as_ref()
                    .filter(|record| record.get("id").and_then(Value::as_str) == Some(id))
            })
    }

    fn function_cart_transform_by_id(&self, id: &str) -> Option<&Value> {
        self.store
            .staged
            .function_cart_transforms
            .get(id)
            .or_else(|| self.store.base.function_cart_transforms.get(id))
            .or_else(|| {
                self.store
                    .staged
                    .function_cart_transform
                    .as_ref()
                    .filter(|record| record.get("id").and_then(Value::as_str) == Some(id))
            })
    }

    fn effective_active_validation_count(&self, exclude_id: Option<&str>) -> usize {
        self.effective_function_validation_records()
            .into_iter()
            .filter(|record| {
                record["id"].as_str() != exclude_id && record["enable"].as_bool() == Some(true)
            })
            .count()
    }

    fn effective_cart_transform_count(&self) -> usize {
        self.effective_function_cart_transform_records().len()
    }

    fn effective_function_id_in_use(&self, function_id: &str) -> bool {
        self.effective_function_validation_records()
            .into_iter()
            .chain(self.effective_function_cart_transform_records())
            .any(|record| record["functionId"].as_str() == Some(function_id))
    }

    fn effective_function_validation_records(&self) -> Vec<&Value> {
        let mut seen = BTreeSet::new();
        let mut records = Vec::new();
        for id in self
            .store
            .base
            .function_validation_order
            .iter()
            .chain(self.store.staged.function_validation_order.iter())
        {
            if !seen.insert(id.clone())
                || self
                    .store
                    .staged
                    .deleted_function_validation_ids
                    .contains(id)
            {
                continue;
            }
            if let Some(record) = self
                .store
                .staged
                .function_validations
                .get(id)
                .or_else(|| self.store.base.function_validations.get(id))
            {
                records.push(record);
            }
        }
        records
    }

    fn effective_function_cart_transform_records(&self) -> Vec<&Value> {
        let mut seen = BTreeSet::new();
        let mut records = Vec::new();
        for id in self
            .store
            .base
            .function_cart_transform_order
            .iter()
            .chain(self.store.staged.function_cart_transform_order.iter())
        {
            if !seen.insert(id.clone())
                || self
                    .store
                    .staged
                    .deleted_function_cart_transform_ids
                    .contains(id)
            {
                continue;
            }
            if let Some(record) = self
                .store
                .staged
                .function_cart_transforms
                .get(id)
                .or_else(|| self.store.base.function_cart_transforms.get(id))
            {
                records.push(record);
            }
        }
        records
    }

    fn effective_function_validation_nodes(&self) -> Vec<Value> {
        self.effective_function_validation_records()
            .into_iter()
            .map(validation_record_value)
            .collect()
    }

    fn effective_function_cart_transform_nodes(&self) -> Vec<Value> {
        self.effective_function_cart_transform_records()
            .into_iter()
            .map(cart_transform_record_value)
            .collect()
    }

    fn function_metadata_read_nodes(
        &self,
        request: &Request,
        api_type: Option<&str>,
    ) -> Vec<Value> {
        let mut seen = BTreeSet::new();
        let mut nodes = Vec::new();
        for id in self
            .store
            .base
            .function_metadata_order
            .iter()
            .chain(self.store.staged.function_metadata_order.iter())
        {
            let Some(function) = self
                .store
                .staged
                .function_metadata
                .get(id)
                .or_else(|| self.store.base.function_metadata.get(id))
            else {
                continue;
            };
            if api_type
                .is_none_or(|api_type| function_matches_canonical_api_type(function, api_type))
                && function_belongs_to_request(function, request)
                && seen.insert(id.clone())
            {
                nodes.push(function.clone());
            }
        }
        for function in self
            .store
            .staged
            .function_validation_order
            .iter()
            .filter_map(|id| self.store.staged.function_validations.get(id))
            .chain(
                self.store
                    .staged
                    .function_cart_transform_order
                    .iter()
                    .filter_map(|id| self.store.staged.function_cart_transforms.get(id)),
            )
            .chain(
                self.store
                    .staged
                    .function_fulfillment_constraint_rule_order
                    .iter()
                    .filter_map(|id| {
                        self.store
                            .staged
                            .function_fulfillment_constraint_rules
                            .get(id)
                    }),
            )
            .filter_map(|record| record.get("shopifyFunction"))
        {
            if api_type
                .is_none_or(|api_type| function_matches_canonical_api_type(function, api_type))
                && function_belongs_to_request(function, request)
            {
                if let Some(id) = function["id"].as_str() {
                    if seen.insert(id.to_string()) {
                        nodes.push(function.clone());
                    }
                }
            }
        }
        nodes
    }

    fn function_metadata_by_id_or_handle(
        &self,
        id: Option<&str>,
        handle: Option<&str>,
    ) -> Option<Value> {
        self.store
            .base
            .function_metadata_order
            .iter()
            .filter_map(|id| self.store.base.function_metadata.get(id))
            .chain(
                self.store
                    .staged
                    .function_metadata_order
                    .iter()
                    .filter_map(|id| self.store.staged.function_metadata.get(id)),
            )
            .chain(
                self.store
                    .base
                    .function_validations
                    .values()
                    .filter_map(|record| record.get("shopifyFunction")),
            )
            .chain(
                self.store
                    .base
                    .function_cart_transforms
                    .values()
                    .filter_map(|record| record.get("shopifyFunction")),
            )
            .chain(
                self.store
                    .base
                    .function_fulfillment_constraint_rules
                    .values()
                    .filter_map(|record| record.get("shopifyFunction")),
            )
            .chain(
                self.store
                    .staged
                    .function_validations
                    .values()
                    .filter_map(|record| record.get("shopifyFunction")),
            )
            .chain(
                self.store
                    .staged
                    .function_cart_transforms
                    .values()
                    .filter_map(|record| record.get("shopifyFunction")),
            )
            .chain(
                self.store
                    .staged
                    .function_fulfillment_constraint_rules
                    .values()
                    .filter_map(|record| record.get("shopifyFunction")),
            )
            .find(|function| {
                id.is_some_and(|id| function["id"].as_str() == Some(id))
                    || handle.is_some_and(|handle| function["handle"].as_str() == Some(handle))
            })
            .cloned()
    }

    fn resolve_function_metadata(
        &mut self,
        request: &Request,
        id: Option<&str>,
        handle: Option<&str>,
        api_type: &str,
    ) -> Option<Value> {
        if let Some(function) = self.function_metadata_by_id_or_handle(id, handle) {
            return function_belongs_to_request(&function, request).then_some(function);
        }
        if self.config.read_mode == ReadMode::Snapshot {
            return None;
        }
        let function = if let Some(id) = id {
            self.hydrate_function_metadata_by_id(request, id)
        } else {
            handle.and_then(|handle| {
                self.hydrate_function_metadata_by_handle(request, handle, api_type)
            })
        }?;
        if !function_belongs_to_request(&function, request) {
            return None;
        }
        self.stage_function_metadata(function.clone());
        Some(function)
    }

    fn hydrate_function_metadata_by_id(&mut self, request: &Request, id: &str) -> Option<Value> {
        let response = self.upstream_post(
            request,
            json!({
                "query": FUNCTION_HYDRATE_BY_ID_QUERY,
                "operationName": "FunctionHydrateById",
                "variables": { "id": id }
            }),
        );
        if response.status != 200 {
            return None;
        }
        let function =
            normalized_function_metadata(response.body["data"]["shopifyFunction"].clone())?;
        self.stage_function_metadata(function.clone());
        Some(function)
    }

    fn hydrate_function_metadata_by_handle(
        &mut self,
        request: &Request,
        handle: &str,
        api_type: &str,
    ) -> Option<Value> {
        let response = self.upstream_post(
            request,
            json!({
                "query": FUNCTION_HYDRATE_BY_HANDLE_QUERY,
                "operationName": "FunctionHydrateByHandle",
                "variables": { "handle": handle, "apiType": api_type }
            }),
        );
        if response.status != 200 {
            return None;
        }
        let nodes = response.body["data"]["shopifyFunctions"]["nodes"].as_array()?;
        let mut matches = nodes
            .iter()
            .filter(|function| function_metadata_matches_handle(function, handle))
            .cloned()
            .collect::<Vec<_>>();
        let selected = matches
            .iter()
            .position(|function| function_matches_canonical_api_type(function, api_type))
            .map(|index| matches.remove(index))
            .or_else(|| matches.into_iter().next())?;
        let function = normalized_function_metadata_with_handle(selected, Some(handle))?;
        self.stage_function_metadata(function.clone());
        Some(function)
    }

    fn stage_function_metadata(&mut self, function: Value) {
        let Some(id) = function["id"].as_str().map(str::to_string) else {
            return;
        };
        if !self.store.base.function_metadata.contains_key(&id) {
            self.store.base.function_metadata_order.push(id.clone());
            self.store.base.function_metadata.insert(id, function);
        } else if let Some(existing) = self.store.base.function_metadata.get_mut(&id) {
            merge_json_values(existing, &function);
        }
    }

    pub(in crate::proxy) fn resolve_payment_customization_function(
        &mut self,
        request: &Request,
        id: Option<&str>,
        handle: Option<&str>,
    ) -> Option<Value> {
        self.resolve_function_metadata(request, id, handle, "PAYMENT_CUSTOMIZATION")
    }

    pub(in crate::proxy) fn resolve_delivery_customization_function(
        &mut self,
        request: &Request,
        id: Option<&str>,
        handle: Option<&str>,
    ) -> Option<Value> {
        self.resolve_function_metadata(request, id, handle, "DELIVERY_CUSTOMIZATION")
    }

    pub(in crate::proxy) fn delivery_customization_record_matches_function_key(
        &mut self,
        request: &Request,
        record: &Value,
        candidate_key: &str,
    ) -> bool {
        self.delivery_customization_record_function_key(request, record)
            .as_deref()
            == Some(candidate_key)
    }

    fn delivery_customization_record_function_key(
        &mut self,
        request: &Request,
        record: &Value,
    ) -> Option<String> {
        if let Some(id) = record["functionId"].as_str() {
            return Some(delivery_customization_function_key(id));
        }
        let handle = record["shopifyFunction"]["handle"].as_str()?;
        self.resolve_delivery_customization_function(request, None, Some(handle))
            .and_then(|function| {
                function["id"]
                    .as_str()
                    .map(delivery_customization_function_key)
            })
            .or_else(|| Some(delivery_customization_function_key(handle)))
    }

    pub(in crate::proxy) fn payment_customization_record_matches_function_key(
        &mut self,
        request: &Request,
        record: &Value,
        candidate_key: &str,
    ) -> bool {
        self.payment_customization_record_function_key(request, record)
            .as_deref()
            == Some(candidate_key)
    }

    fn payment_customization_record_function_key(
        &mut self,
        request: &Request,
        record: &Value,
    ) -> Option<String> {
        if let Some(id) = record["functionId"].as_str() {
            return Some(payment_customization_function_key(id));
        }
        let handle = record["functionHandle"].as_str()?;
        self.resolve_payment_customization_function(request, None, Some(handle))
            .and_then(|function| {
                function["id"]
                    .as_str()
                    .map(payment_customization_function_key)
            })
            .or_else(|| Some(payment_customization_function_key(handle)))
    }

    fn observe_function_operation_roots(
        &mut self,
        request: &Request,
        roots: &[OperationRootInvocation],
        data: &Value,
    ) {
        for root in roots {
            let Some(value) = data.get(&root.response_key) else {
                continue;
            };
            let arguments = resolved_arguments_from_json(&root.arguments);
            self.observe_function_root_value(request, &root.name, &arguments, value);
        }
    }

    fn observe_function_root_value(
        &mut self,
        request: &Request,
        root_name: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
        value: &Value,
    ) {
        match root_name {
            "validation" => {
                if value.is_object() {
                    self.stage_base_function_validation(value.clone());
                }
            }
            "validations" => {
                self.observe_function_connection(request, root_name, arguments, value);
                for row in function_observed_connection_rows(value) {
                    self.stage_base_function_validation(row.node);
                }
            }
            "cartTransforms" => {
                self.observe_function_connection(request, root_name, arguments, value);
                for row in function_observed_connection_rows(value) {
                    self.stage_base_function_cart_transform(row.node);
                }
            }
            "fulfillmentConstraintRules" => {
                for rule in value.as_array().into_iter().flatten() {
                    self.stage_base_function_fulfillment_constraint_rule(rule.clone());
                }
            }
            "shopifyFunction" => {
                if let Some(function) = normalized_function_metadata(value.clone()) {
                    self.stage_function_metadata(function);
                }
            }
            "shopifyFunctions" => {
                self.observe_function_connection(request, root_name, arguments, value);
                for row in function_observed_connection_rows(value) {
                    if let Some(function) = normalized_function_metadata(row.node) {
                        self.stage_function_metadata(function);
                    }
                }
            }
            _ => {}
        }
    }

    fn observe_function_connection(
        &mut self,
        request: &Request,
        root_name: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
        connection: &Value,
    ) {
        let scope_key = function_connection_scope_key(request, root_name, arguments);
        let window_key = function_connection_window_key(request, root_name, arguments);
        self.store.base.function_connection_observations.insert(
            window_key,
            json!({
                "scopeKey": scope_key,
                "arguments": resolved_arguments_json(arguments),
                "connection": connection,
                "complete": function_observed_window_is_complete(connection, arguments)
            }),
        );
    }

    fn function_connection_overlay_value(
        &mut self,
        request: &Request,
        field: &FunctionRootInput,
        upstream_value: Option<&Value>,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let mut baseline = upstream_value
            .cloned()
            .unwrap_or_else(empty_function_connection);
        let delta_count = self.function_connection_delta_count(&field.name);
        let baseline_lacks_identity = function_observed_connection_rows(&baseline)
            .iter()
            .any(|row| row.node["id"].as_str().is_none());
        if delta_count > 0
            && (baseline_lacks_identity
                || self.function_connection_needs_refill(&field.name, &baseline, arguments))
        {
            if let Some((refill, refill_arguments)) = self
                .hydrate_bounded_function_connection_window(
                    request,
                    field,
                    arguments,
                    &baseline,
                    delta_count,
                )
            {
                self.observe_function_root_value(request, &field.name, &refill_arguments, &refill);
                baseline = refill;
            }
        }
        let rendered =
            self.render_function_connection_overlay(request, &field.name, &baseline, arguments);
        self.observe_function_connection(request, &field.name, arguments, &rendered);
        rendered
    }

    fn function_root_has_local_overlay(&self, root_name: &str) -> bool {
        match root_name {
            "validation" | "validations" => self.has_function_validation_overlay_state(),
            "cartTransforms" => self.has_function_cart_transform_overlay_state(),
            "fulfillmentConstraintRules" => {
                self.has_function_fulfillment_constraint_rule_overlay_state()
            }
            // Function metadata itself is never created, updated, or deleted by
            // the supported lifecycle. Observing a Function for validation or
            // cart-transform staging must not turn its catalog into a local one.
            "shopifyFunction" | "shopifyFunctions" => false,
            _ => false,
        }
    }

    fn hydrate_function_validation_by_id(&mut self, request: &Request, id: &str) -> Option<Value> {
        if self
            .store
            .staged
            .deleted_function_validation_ids
            .contains(id)
        {
            return None;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": FUNCTION_VALIDATION_HYDRATE_BY_ID_QUERY,
                "operationName": "FunctionValidationHydrateById",
                "variables": { "id": id }
            }),
        );
        if response.status != 200 {
            return None;
        }
        let validation =
            normalized_function_validation(response.body["data"]["validation"].clone())?;
        self.stage_base_function_validation(validation.clone());
        Some(validation)
    }

    fn hydrate_function_cart_transform_by_id(
        &mut self,
        request: &Request,
        id: &str,
    ) -> Option<Value> {
        if self
            .store
            .staged
            .deleted_function_cart_transform_ids
            .contains(id)
        {
            return None;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": FUNCTION_CART_TRANSFORM_HYDRATE_BY_ID_QUERY,
                "operationName": "FunctionCartTransformHydrateById",
                "variables": { "id": id }
            }),
        );
        if response.status != 200 {
            return None;
        }
        let cart_transform =
            normalized_function_cart_transform(response.body["data"]["node"].clone())?;
        self.stage_base_function_cart_transform(cart_transform.clone());
        Some(cart_transform)
    }

    fn stage_base_function_validation(&mut self, mut validation: Value) {
        let Some(id) = validation["id"].as_str().map(str::to_string) else {
            return;
        };
        if let Some(function) = validation
            .get("shopifyFunction")
            .and_then(|function| normalized_function_metadata(function.clone()))
        {
            validation["shopifyFunction"] = function.clone();
            self.stage_function_metadata(function);
        }
        if validation.get("enabled").is_none() {
            if let Some(enable) = validation.get("enable").cloned() {
                validation["enabled"] = enable;
            }
        }
        if validation.get("enable").is_none() {
            if let Some(enabled) = validation.get("enabled").cloned() {
                validation["enable"] = enabled;
            }
        }
        if !self.store.base.function_validations.contains_key(&id) {
            self.store.base.function_validation_order.push(id.clone());
            self.store.base.function_validations.insert(id, validation);
        } else if let Some(existing) = self.store.base.function_validations.get_mut(&id) {
            merge_json_values(existing, &validation);
        }
    }

    fn stage_base_function_cart_transform(&mut self, mut cart_transform: Value) {
        let Some(id) = cart_transform["id"].as_str().map(str::to_string) else {
            return;
        };
        if cart_transform.get("metafields").is_some()
            && cart_transform.get("metafield").is_none_or(Value::is_null)
        {
            if let Some(first) = cart_transform["metafields"]["nodes"]
                .as_array()
                .and_then(|nodes| nodes.first())
                .cloned()
            {
                cart_transform["metafield"] = first;
            }
        }
        if !self.store.base.function_cart_transforms.contains_key(&id) {
            self.store
                .base
                .function_cart_transform_order
                .push(id.clone());
            self.store
                .base
                .function_cart_transforms
                .insert(id, cart_transform);
        } else if let Some(existing) = self.store.base.function_cart_transforms.get_mut(&id) {
            merge_json_values(existing, &cart_transform);
        }
    }

    fn stage_base_function_fulfillment_constraint_rule(&mut self, mut rule: Value) {
        let Some(id) = rule["id"].as_str().map(str::to_string) else {
            return;
        };
        if let Some(function) = rule
            .get("function")
            .or_else(|| rule.get("shopifyFunction"))
            .and_then(|function| normalized_function_metadata(function.clone()))
        {
            rule["function"] = function.clone();
            rule["shopifyFunction"] = function.clone();
            self.stage_function_metadata(function);
        }
        if rule.get("metafields").is_some() && rule.get("metafield").is_none_or(Value::is_null) {
            if let Some(first) = rule["metafields"]["nodes"]
                .as_array()
                .and_then(|nodes| nodes.first())
                .cloned()
            {
                rule["metafield"] = first;
            }
        }
        if !self
            .store
            .base
            .function_fulfillment_constraint_rules
            .contains_key(&id)
        {
            self.store
                .base
                .function_fulfillment_constraint_rule_order
                .push(id.clone());
            self.store
                .base
                .function_fulfillment_constraint_rules
                .insert(id, rule);
        } else if let Some(existing) = self
            .store
            .base
            .function_fulfillment_constraint_rules
            .get_mut(&id)
        {
            merge_json_values(existing, &rule);
        }
    }

    fn has_function_validation_overlay_state(&self) -> bool {
        self.store.staged.function_validations_dirty
            || !self.store.staged.function_validations.is_empty()
            || !self.store.staged.deleted_function_validation_ids.is_empty()
    }

    fn has_function_cart_transform_overlay_state(&self) -> bool {
        self.store.staged.function_cart_transforms_dirty
            || !self.store.staged.function_cart_transforms.is_empty()
            || !self
                .store
                .staged
                .deleted_function_cart_transform_ids
                .is_empty()
    }

    fn has_function_fulfillment_constraint_rule_overlay_state(&self) -> bool {
        self.store
            .staged
            .function_fulfillment_constraint_rules_dirty
            || !self
                .store
                .staged
                .function_fulfillment_constraint_rules
                .is_empty()
            || !self
                .store
                .staged
                .deleted_function_fulfillment_constraint_rule_ids
                .is_empty()
    }

    fn function_connection_delta_count(&self, root_name: &str) -> usize {
        match root_name {
            "validations" => {
                self.store.staged.function_validations.len()
                    + self.store.staged.deleted_function_validation_ids.len()
            }
            "cartTransforms" => {
                self.store.staged.function_cart_transforms.len()
                    + self.store.staged.deleted_function_cart_transform_ids.len()
            }
            _ => 0,
        }
    }

    fn function_connection_needs_refill(
        &self,
        root_name: &str,
        connection: &Value,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> bool {
        let rows = function_observed_connection_rows(connection);
        let has_unobserved_boundary =
            if function_connection_direction(arguments) == FunctionConnectionDirection::Backward {
                connection["pageInfo"]["hasPreviousPage"].as_bool()
            } else {
                connection["pageInfo"]["hasNextPage"].as_bool()
            };
        if has_unobserved_boundary == Some(false) {
            return false;
        }
        let row_ids = rows
            .iter()
            .filter_map(|row| row.node["id"].as_str())
            .collect::<BTreeSet<_>>();
        match root_name {
            "validations" => {
                self.store
                    .staged
                    .deleted_function_validation_ids
                    .iter()
                    .any(|id| row_ids.contains(id.as_str()))
                    || (arguments.contains_key("sortKey")
                        && self
                            .store
                            .staged
                            .function_validations
                            .keys()
                            .any(|id| row_ids.contains(id.as_str())))
            }
            "cartTransforms" => self
                .store
                .staged
                .deleted_function_cart_transform_ids
                .iter()
                .any(|id| row_ids.contains(id.as_str())),
            _ => false,
        }
    }

    fn hydrate_bounded_function_connection_window(
        &self,
        request: &Request,
        field: &FunctionRootInput,
        arguments: &BTreeMap<String, ResolvedValue>,
        connection: &Value,
        delta_count: usize,
    ) -> Option<(Value, BTreeMap<String, ResolvedValue>)> {
        let requested = function_requested_window_size(arguments).max(1);
        let desired = requested.saturating_add(delta_count).saturating_add(1);
        if desired > 250
            && function_observed_connection_rows(connection)
                .iter()
                .all(|row| row.node["id"].as_str().is_some())
        {
            let (cursor_argument, boundary_cursor) = match function_connection_direction(arguments)
            {
                FunctionConnectionDirection::Forward => {
                    ("after", connection["pageInfo"]["endCursor"].as_str())
                }
                FunctionConnectionDirection::Backward => {
                    ("before", connection["pageInfo"]["startCursor"].as_str())
                }
            };
            if let Some(boundary_cursor) = boundary_cursor {
                let mut tail_arguments = arguments.clone();
                tail_arguments.remove("first");
                tail_arguments.remove("last");
                tail_arguments.remove("after");
                tail_arguments.remove("before");
                tail_arguments.insert(
                    cursor_argument.to_string(),
                    ResolvedValue::String(boundary_cursor.to_string()),
                );
                let tail_size = desired.saturating_sub(requested).clamp(1, 250);
                match function_connection_direction(arguments) {
                    FunctionConnectionDirection::Forward => {
                        tail_arguments
                            .insert("first".to_string(), ResolvedValue::Int(tail_size as i64));
                    }
                    FunctionConnectionDirection::Backward => {
                        tail_arguments
                            .insert("last".to_string(), ResolvedValue::Int(tail_size as i64));
                    }
                }
                let tail =
                    self.hydrate_function_connection_query(request, field, &tail_arguments)?;
                return Some((
                    combine_function_connection_windows(
                        connection,
                        &tail,
                        function_connection_direction(arguments),
                    ),
                    arguments.clone(),
                ));
            }
        }
        let mut effective_arguments = arguments.clone();
        let effective = desired.min(250);
        match function_connection_direction(arguments) {
            FunctionConnectionDirection::Forward => {
                effective_arguments.remove("last");
                effective_arguments
                    .insert("first".to_string(), ResolvedValue::Int(effective as i64));
            }
            FunctionConnectionDirection::Backward => {
                effective_arguments.remove("first");
                effective_arguments
                    .insert("last".to_string(), ResolvedValue::Int(effective as i64));
            }
        }
        self.hydrate_function_connection_query(request, field, &effective_arguments)
            .map(|connection| (connection, effective_arguments))
    }

    fn hydrate_function_connection_query(
        &self,
        request: &Request,
        field: &FunctionRootInput,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        let query =
            function_connection_hydration_query(&field.name, arguments, &field.field_selection)?;
        let response = self.upstream_post(
            request,
            json!({
                "query": query,
                "operationName": "FunctionConnectionWindowHydrate",
                "variables": {}
            }),
        );
        if !(200..300).contains(&response.status) || response.body.get("errors").is_some() {
            return None;
        }
        response.body["data"]
            .get(&field.name)
            .filter(|connection| connection.is_object())
            .cloned()
    }

    fn hydrate_function_list_root(
        &self,
        request: &Request,
        field: &FunctionRootInput,
    ) -> Option<Value> {
        let query = function_list_hydration_query(&field.name, &field.field_selection)?;
        let response = self.upstream_post(
            request,
            json!({
                "query": query,
                "operationName": "FunctionListWindowHydrate",
                "variables": {}
            }),
        );
        if !(200..300).contains(&response.status) || response.body.get("errors").is_some() {
            return None;
        }
        response.body["data"]
            .get(&field.name)
            .filter(|value| value.is_array())
            .cloned()
    }

    fn hydrate_function_connection_after_local_cursor(
        &self,
        request: &Request,
        field: &FunctionRootInput,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Option<(Value, BTreeMap<String, ResolvedValue>)> {
        let direction = function_connection_direction(arguments);
        let cursor_argument = match direction {
            FunctionConnectionDirection::Forward => "after",
            FunctionConnectionDirection::Backward => "before",
        };
        let cursor = resolved_string_field(arguments, cursor_argument)?;
        if !is_local_function_cursor(&cursor) {
            return None;
        }
        let scope_key = function_connection_scope_key(request, &field.name, arguments);
        let resume = function_local_cursor_resume(
            &self.store.base.function_connection_observations,
            &scope_key,
            &cursor,
            direction,
        )?;
        let mut effective_arguments = arguments.clone();
        if let Some(upstream_cursor) = resume.upstream_cursor {
            effective_arguments.insert(
                cursor_argument.to_string(),
                ResolvedValue::String(upstream_cursor),
            );
        } else {
            effective_arguments.remove(cursor_argument);
        }
        let requested = function_requested_window_size(arguments).max(1);
        let effective = requested
            .saturating_add(self.function_connection_delta_count(&field.name))
            .saturating_add(1)
            .min(250);
        match direction {
            FunctionConnectionDirection::Forward => {
                effective_arguments.remove("last");
                effective_arguments
                    .insert("first".to_string(), ResolvedValue::Int(effective as i64));
            }
            FunctionConnectionDirection::Backward => {
                effective_arguments.remove("first");
                effective_arguments
                    .insert("last".to_string(), ResolvedValue::Int(effective as i64));
            }
        }
        let query = function_connection_hydration_query(
            &field.name,
            &effective_arguments,
            &field.field_selection,
        )?;
        let response = self.upstream_post(
            request,
            json!({
                "query": query,
                "operationName": "FunctionConnectionWindowHydrate",
                "variables": {}
            }),
        );
        if !(200..300).contains(&response.status) || response.body.get("errors").is_some() {
            return None;
        }
        response.body["data"]
            .get(&field.name)
            .filter(|connection| connection.is_object())
            .cloned()
            .map(|connection| (connection, effective_arguments))
    }

    fn render_function_connection_overlay(
        &self,
        request: &Request,
        root_name: &str,
        connection: &Value,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let mut rows = function_observed_connection_rows(connection);
        let mut seen = rows
            .iter()
            .filter_map(|row| row.node["id"].as_str().map(str::to_string))
            .collect::<BTreeSet<_>>();
        rows.retain_mut(|row| {
            let Some(id) = row.node["id"].as_str().map(str::to_string) else {
                return true;
            };
            if self.function_connection_id_is_tombstoned(root_name, &id) {
                return false;
            }
            if let Some(staged) = self.function_connection_staged_record(root_name, &id) {
                merge_json_values(&mut row.node, staged);
            }
            true
        });
        rows.retain(|row| {
            function_observed_row_belongs_to_window(
                &self.store.base.function_connection_observations,
                request,
                root_name,
                &row.node,
                arguments,
            )
        });

        let mut candidates = self.function_connection_staged_candidates(root_name);
        candidates.retain(|candidate| {
            let Some(id) = candidate["id"].as_str() else {
                return false;
            };
            !self.function_connection_id_is_tombstoned(root_name, id)
                && seen.insert(id.to_string())
                && function_staged_candidate_belongs_to_window(
                    &self.store.base.function_connection_observations,
                    request,
                    root_name,
                    candidate,
                    connection,
                    arguments,
                )
        });
        let reverse = resolved_bool_field(arguments, "reverse").unwrap_or(false);
        if arguments.contains_key("sortKey") {
            rows.extend(candidates.into_iter().map(|node| ObservedConnectionRow {
                cursor: Some(local_function_cursor(&node)),
                node,
            }));
            sort_function_connection_rows(&mut rows, arguments);
        } else if reverse {
            let mut staged_rows = candidates
                .into_iter()
                .map(|node| ObservedConnectionRow {
                    cursor: Some(local_function_cursor(&node)),
                    node,
                })
                .collect::<Vec<_>>();
            staged_rows.reverse();
            staged_rows.extend(rows);
            rows = staged_rows;
        } else {
            rows.extend(candidates.into_iter().map(|node| ObservedConnectionRow {
                cursor: Some(local_function_cursor(&node)),
                node,
            }));
        }

        function_windowed_observed_connection(rows, connection, arguments)
    }

    fn function_connection_id_is_tombstoned(&self, root_name: &str, id: &str) -> bool {
        match root_name {
            "validations" => self
                .store
                .staged
                .deleted_function_validation_ids
                .contains(id),
            "cartTransforms" => self
                .store
                .staged
                .deleted_function_cart_transform_ids
                .contains(id),
            _ => false,
        }
    }

    fn function_connection_staged_record(&self, root_name: &str, id: &str) -> Option<&Value> {
        match root_name {
            "validations" => self.store.staged.function_validations.get(id),
            "cartTransforms" => self.store.staged.function_cart_transforms.get(id),
            _ => None,
        }
    }

    fn function_connection_staged_candidates(&self, root_name: &str) -> Vec<Value> {
        match root_name {
            "validations" => self
                .store
                .staged
                .function_validation_order
                .iter()
                .filter_map(|id| self.store.staged.function_validations.get(id))
                .map(validation_record_value)
                .collect(),
            "cartTransforms" => self
                .store
                .staged
                .function_cart_transform_order
                .iter()
                .filter_map(|id| self.store.staged.function_cart_transforms.get(id))
                .map(cart_transform_record_value)
                .collect(),
            _ => Vec::new(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FunctionConnectionDirection {
    Forward,
    Backward,
}

struct FunctionLocalCursorResume {
    upstream_cursor: Option<String>,
}

fn function_connection_direction(
    arguments: &BTreeMap<String, ResolvedValue>,
) -> FunctionConnectionDirection {
    if arguments.contains_key("last") || arguments.contains_key("before") {
        FunctionConnectionDirection::Backward
    } else {
        FunctionConnectionDirection::Forward
    }
}

fn function_requested_window_size(arguments: &BTreeMap<String, ResolvedValue>) -> usize {
    ["first", "last"]
        .into_iter()
        .find_map(|name| match arguments.get(name) {
            Some(ResolvedValue::Int(value)) if *value >= 0 => Some(*value as usize),
            _ => None,
        })
        .unwrap_or(50)
}

fn resolved_arguments_json(arguments: &BTreeMap<String, ResolvedValue>) -> Value {
    Value::Object(
        arguments
            .iter()
            .map(|(name, value)| (name.clone(), resolved_value_json(value)))
            .collect(),
    )
}

fn function_connection_scope_key(
    request: &Request,
    root_name: &str,
    arguments: &BTreeMap<String, ResolvedValue>,
) -> String {
    let scope_arguments = arguments
        .iter()
        .filter(|(name, _)| !matches!(name.as_str(), "first" | "last" | "after" | "before"))
        .map(|(name, value)| (name.clone(), resolved_value_json(value)))
        .collect::<serde_json::Map<_, _>>();
    json!({
        "root": root_name,
        "owner": request_header(request, API_CLIENT_ID_HEADER),
        "direction": match function_connection_direction(arguments) {
            FunctionConnectionDirection::Forward => "forward",
            FunctionConnectionDirection::Backward => "backward",
        },
        "arguments": scope_arguments
    })
    .to_string()
}

fn function_connection_window_key(
    request: &Request,
    root_name: &str,
    arguments: &BTreeMap<String, ResolvedValue>,
) -> String {
    json!({
        "scope": function_connection_scope_key(request, root_name, arguments),
        "window": resolved_arguments_json(arguments)
    })
    .to_string()
}

fn is_local_function_cursor(cursor: &str) -> bool {
    cursor.starts_with("cursor:gid://shopify/")
}

fn function_local_cursor_resume(
    observations: &BTreeMap<String, Value>,
    scope_key: &str,
    cursor: &str,
    direction: FunctionConnectionDirection,
) -> Option<FunctionLocalCursorResume> {
    let mut observed_cursor = false;
    for observation in observations.values() {
        if observation["scopeKey"].as_str() != Some(scope_key) {
            continue;
        }
        let Some(edges) = observation["connection"]["edges"].as_array() else {
            continue;
        };
        let Some(boundary_index) = edges
            .iter()
            .position(|edge| edge["cursor"].as_str() == Some(cursor))
        else {
            continue;
        };
        observed_cursor = true;
        let upstream_cursor = match direction {
            FunctionConnectionDirection::Forward => edges[..boundary_index]
                .iter()
                .rev()
                .filter_map(|edge| edge["cursor"].as_str())
                .find(|candidate| !is_local_function_cursor(candidate)),
            FunctionConnectionDirection::Backward => edges[boundary_index.saturating_add(1)..]
                .iter()
                .filter_map(|edge| edge["cursor"].as_str())
                .find(|candidate| !is_local_function_cursor(candidate)),
        }
        .map(str::to_string);
        if upstream_cursor.is_some() {
            return Some(FunctionLocalCursorResume { upstream_cursor });
        }
    }
    observed_cursor.then_some(FunctionLocalCursorResume {
        upstream_cursor: None,
    })
}

fn function_observed_window_is_complete(
    connection: &Value,
    arguments: &BTreeMap<String, ResolvedValue>,
) -> bool {
    match function_connection_direction(arguments) {
        FunctionConnectionDirection::Forward => {
            connection["pageInfo"]["hasNextPage"].as_bool() == Some(false)
        }
        FunctionConnectionDirection::Backward => {
            connection["pageInfo"]["hasPreviousPage"].as_bool() == Some(false)
        }
    }
}

fn empty_function_connection() -> Value {
    json!({ "nodes": [], "edges": [], "pageInfo": empty_page_info() })
}

fn function_observed_connection_rows(connection: &Value) -> Vec<ObservedConnectionRow> {
    let mut rows = observed_connection_rows(connection);
    if let Some(first) = rows.first_mut() {
        if first.cursor.is_none() {
            first.cursor = connection["pageInfo"]["startCursor"]
                .as_str()
                .map(str::to_string);
        }
    }
    if let Some(last) = rows.last_mut() {
        if last.cursor.is_none() {
            last.cursor = connection["pageInfo"]["endCursor"]
                .as_str()
                .map(str::to_string);
        }
    }
    rows
}

fn combine_function_connection_windows(
    connection: &Value,
    tail: &Value,
    direction: FunctionConnectionDirection,
) -> Value {
    let (mut rows, additional) = match direction {
        FunctionConnectionDirection::Forward => (
            function_observed_connection_rows(connection),
            function_observed_connection_rows(tail),
        ),
        FunctionConnectionDirection::Backward => (
            function_observed_connection_rows(tail),
            function_observed_connection_rows(connection),
        ),
    };
    let mut seen = rows
        .iter()
        .map(|row| {
            row.node["id"]
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| row.node.to_string())
        })
        .collect::<BTreeSet<_>>();
    rows.extend(additional.into_iter().filter(|row| {
        seen.insert(
            row.node["id"]
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| row.node.to_string()),
        )
    }));
    let has_next = match direction {
        FunctionConnectionDirection::Forward => {
            tail["pageInfo"]["hasNextPage"].as_bool().unwrap_or(false)
        }
        FunctionConnectionDirection::Backward => connection["pageInfo"]["hasNextPage"]
            .as_bool()
            .unwrap_or(false),
    };
    let has_previous = match direction {
        FunctionConnectionDirection::Forward => connection["pageInfo"]["hasPreviousPage"]
            .as_bool()
            .unwrap_or(false),
        FunctionConnectionDirection::Backward => tail["pageInfo"]["hasPreviousPage"]
            .as_bool()
            .unwrap_or(false),
    };
    let start_cursor = rows.first().and_then(|row| row.cursor.clone());
    let end_cursor = rows.last().and_then(|row| row.cursor.clone());
    let nodes = rows.iter().map(|row| row.node.clone()).collect::<Vec<_>>();
    let edges = rows
        .into_iter()
        .map(|row| json!({ "cursor": row.cursor, "node": row.node }))
        .collect::<Vec<_>>();
    json!({
        "nodes": nodes,
        "edges": edges,
        "pageInfo": connection_page_info(has_next, has_previous, start_cursor, end_cursor)
    })
}

fn canonical_hydration_field(mut field: SelectedField) -> SelectedField {
    field.response_key = field.name.clone();
    field.selection = field
        .selection
        .into_iter()
        .map(canonical_hydration_field)
        .collect();
    field
}

fn function_connection_node_selection(selection: &[SelectedField]) -> Vec<SelectedField> {
    let mut fields = Vec::new();
    for field in selection {
        match field.name.as_str() {
            "nodes" => fields.extend(field.selection.clone()),
            "edges" => fields.extend(
                field
                    .selection
                    .iter()
                    .filter(|child| child.name == "node")
                    .flat_map(|child| child.selection.clone()),
            ),
            _ => {}
        }
    }
    fields = fields.into_iter().map(canonical_hydration_field).collect();
    if !fields.iter().any(|field| field.name == "id") {
        fields.push(SelectedField {
            name: "id".to_string(),
            response_key: "id".to_string(),
            location: SourceLocation { line: 1, column: 1 },
            arguments: BTreeMap::new(),
            selection: Vec::new(),
            type_condition: None,
        });
    }
    let mut seen = BTreeSet::new();
    fields.retain(|field| {
        seen.insert(crate::proxy::graphql_runtime::serialize_selected_field(
            field,
        ))
    });
    fields
}

fn function_connection_hydration_query(
    root_name: &str,
    arguments: &BTreeMap<String, ResolvedValue>,
    selection: &[SelectedField],
) -> Option<String> {
    if !matches!(
        root_name,
        "validations" | "cartTransforms" | "shopifyFunctions"
    ) {
        return None;
    }
    let node_selection = function_connection_node_selection(selection);
    let node_fields = node_selection
        .iter()
        .map(crate::proxy::graphql_runtime::serialize_selected_field)
        .collect::<Vec<_>>()
        .join(" ");
    Some(format!(
        "query FunctionConnectionWindowHydrate {{ {root_name}{} {{ edges {{ cursor node {{ {node_fields} }} }} pageInfo {{ hasNextPage hasPreviousPage startCursor endCursor }} }} }}",
        serialize_function_connection_arguments(arguments)
    ))
}

fn function_list_hydration_query(root_name: &str, selection: &[SelectedField]) -> Option<String> {
    if root_name != "fulfillmentConstraintRules" {
        return None;
    }
    let mut fields = selection
        .iter()
        .cloned()
        .map(canonical_hydration_field)
        .collect::<Vec<_>>();
    if !fields.iter().any(|field| field.name == "id") {
        fields.push(SelectedField {
            name: "id".to_string(),
            response_key: "id".to_string(),
            location: SourceLocation { line: 1, column: 1 },
            arguments: BTreeMap::new(),
            selection: Vec::new(),
            type_condition: None,
        });
    }
    let fields = fields
        .iter()
        .map(crate::proxy::graphql_runtime::serialize_selected_field)
        .collect::<Vec<_>>()
        .join(" ");
    Some(format!(
        "query FunctionListWindowHydrate {{ {root_name} {{ {fields} }} }}"
    ))
}

fn serialize_function_connection_arguments(arguments: &BTreeMap<String, ResolvedValue>) -> String {
    if arguments.is_empty() {
        return String::new();
    }
    let rendered = arguments
        .iter()
        .map(|(name, value)| {
            let value = if name == "sortKey" {
                resolved_string_field(arguments, name).unwrap_or_default()
            } else {
                crate::proxy::graphql_runtime::serialize_resolved_value(value)
            };
            format!("{name}: {value}")
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("({rendered})")
}

fn function_observed_cursor_node(
    observations: &BTreeMap<String, Value>,
    scope_key: &str,
    cursor: &str,
) -> Option<Value> {
    observations.values().find_map(|observation| {
        (observation["scopeKey"].as_str() == Some(scope_key))
            .then(|| {
                observation["connection"]["edges"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|edge| edge["cursor"].as_str() == Some(cursor))
                    .and_then(|edge| edge.get("node").cloned())
            })
            .flatten()
    })
}

fn function_observed_cursor_order(
    observations: &BTreeMap<String, Value>,
    scope_key: &str,
    left_cursor: &str,
    right_cursor: &str,
) -> Option<std::cmp::Ordering> {
    observations.values().find_map(|observation| {
        if observation["scopeKey"].as_str() != Some(scope_key) {
            return None;
        }
        let edges = observation["connection"]["edges"].as_array()?;
        let left_index = edges
            .iter()
            .position(|edge| edge["cursor"].as_str() == Some(left_cursor))?;
        let right_index = edges
            .iter()
            .position(|edge| edge["cursor"].as_str() == Some(right_cursor))?;
        Some(left_index.cmp(&right_index))
    })
}

fn compare_function_nodes(left: &Value, right: &Value, sort_key: &str) -> std::cmp::Ordering {
    match sort_key {
        "TITLE" | "title" => left["title"]
            .as_str()
            .unwrap_or_default()
            .cmp(right["title"].as_str().unwrap_or_default())
            .then_with(|| {
                left["id"]
                    .as_str()
                    .unwrap_or_default()
                    .cmp(right["id"].as_str().unwrap_or_default())
            }),
        _ => left["id"]
            .as_str()
            .unwrap_or_default()
            .cmp(right["id"].as_str().unwrap_or_default()),
    }
}

fn function_staged_candidate_belongs_to_window(
    observations: &BTreeMap<String, Value>,
    request: &Request,
    root_name: &str,
    candidate: &Value,
    connection: &Value,
    arguments: &BTreeMap<String, ResolvedValue>,
) -> bool {
    for cursor_argument in ["after", "before"] {
        if resolved_string_field(arguments, cursor_argument)
            .as_deref()
            .is_some_and(|cursor| {
                is_local_function_cursor(cursor) && local_function_cursor(candidate) == cursor
            })
        {
            return false;
        }
    }
    let reverse = resolved_bool_field(arguments, "reverse").unwrap_or(false);
    let candidate_cursor = local_function_cursor(candidate);
    let scope_key = function_connection_scope_key(request, root_name, arguments);
    if let Some(after) = resolved_string_field(arguments, "after") {
        if is_local_function_cursor(&after) {
            if let Some(ordering) =
                function_observed_cursor_order(observations, &scope_key, &candidate_cursor, &after)
            {
                return ordering.is_gt();
            }
        }
    }
    if let Some(before) = resolved_string_field(arguments, "before") {
        if is_local_function_cursor(&before) {
            if let Some(ordering) =
                function_observed_cursor_order(observations, &scope_key, &candidate_cursor, &before)
            {
                return ordering.is_lt();
            }
        }
    }
    let Some(sort_key) = resolved_string_field(arguments, "sortKey") else {
        return match function_connection_direction(arguments) {
            FunctionConnectionDirection::Forward => {
                reverse || connection["pageInfo"]["hasNextPage"].as_bool() != Some(true)
            }
            FunctionConnectionDirection::Backward => !reverse,
        };
    };
    let compare = |left: &Value, right: &Value| {
        let ordering = compare_function_nodes(left, right, &sort_key);
        if reverse {
            ordering.reverse()
        } else {
            ordering
        }
    };
    if let Some(after) = resolved_string_field(arguments, "after") {
        if let Some(boundary) = function_observed_cursor_node(observations, &scope_key, &after) {
            return compare(candidate, &boundary).is_gt();
        }
    }
    if let Some(before) = resolved_string_field(arguments, "before") {
        if let Some(boundary) = function_observed_cursor_node(observations, &scope_key, &before) {
            return compare(candidate, &boundary).is_lt();
        }
    }
    true
}

fn function_observed_row_belongs_to_window(
    observations: &BTreeMap<String, Value>,
    request: &Request,
    root_name: &str,
    candidate: &Value,
    arguments: &BTreeMap<String, ResolvedValue>,
) -> bool {
    let Some(sort_key) = resolved_string_field(arguments, "sortKey") else {
        return true;
    };
    let reverse = resolved_bool_field(arguments, "reverse").unwrap_or(false);
    let scope_key = function_connection_scope_key(request, root_name, arguments);
    let compare = |left: &Value, right: &Value| {
        let ordering = compare_function_nodes(left, right, &sort_key);
        if reverse {
            ordering.reverse()
        } else {
            ordering
        }
    };
    if let Some(after) = resolved_string_field(arguments, "after") {
        if is_local_function_cursor(&after) {
            if let Some(boundary) = function_observed_cursor_node(observations, &scope_key, &after)
            {
                return compare(candidate, &boundary).is_gt();
            }
        }
    }
    if let Some(before) = resolved_string_field(arguments, "before") {
        if is_local_function_cursor(&before) {
            if let Some(boundary) = function_observed_cursor_node(observations, &scope_key, &before)
            {
                return compare(candidate, &boundary).is_lt();
            }
        }
    }
    true
}

fn sort_function_connection_rows(
    rows: &mut [ObservedConnectionRow],
    arguments: &BTreeMap<String, ResolvedValue>,
) {
    let sort_key = resolved_string_field(arguments, "sortKey").unwrap_or_else(|| "ID".to_string());
    rows.sort_by(|left, right| compare_function_nodes(&left.node, &right.node, &sort_key));
    if resolved_bool_field(arguments, "reverse").unwrap_or(false) {
        rows.reverse();
    }
}

fn function_windowed_observed_connection(
    rows: Vec<ObservedConnectionRow>,
    upstream: &Value,
    arguments: &BTreeMap<String, ResolvedValue>,
) -> Value {
    let requested = function_requested_window_size(arguments);
    let total = rows.len();
    let (start, end) = match function_connection_direction(arguments) {
        FunctionConnectionDirection::Forward => (0, total.min(requested)),
        FunctionConnectionDirection::Backward => (total.saturating_sub(requested), total),
    };
    let selected = rows[start..end].to_vec();
    let upstream_has_next = upstream["pageInfo"]["hasNextPage"]
        .as_bool()
        .unwrap_or(false);
    let upstream_has_previous = upstream["pageInfo"]["hasPreviousPage"]
        .as_bool()
        .unwrap_or(false);
    let (has_next, has_previous) = match function_connection_direction(arguments) {
        FunctionConnectionDirection::Forward => (
            upstream_has_next || end < total,
            upstream_has_previous || resolved_string_field(arguments, "after").is_some(),
        ),
        FunctionConnectionDirection::Backward => (
            upstream_has_next || resolved_string_field(arguments, "before").is_some(),
            upstream_has_previous || start > 0,
        ),
    };
    let start_cursor = selected.first().and_then(|row| row.cursor.clone());
    let end_cursor = selected.last().and_then(|row| row.cursor.clone());
    let nodes = selected
        .iter()
        .map(|row| row.node.clone())
        .collect::<Vec<_>>();
    let edges = selected
        .into_iter()
        .map(|row| json!({ "cursor": row.cursor, "node": row.node }))
        .collect::<Vec<_>>();
    json!({
        "nodes": nodes,
        "edges": edges,
        "pageInfo": connection_page_info(has_next, has_previous, start_cursor, end_cursor)
    })
}

const TAX_APP_CONFIGURE_REQUIRED_ACCESS: &str =
    "`write_taxes` access scope. Also: The caller must be a tax calculations app.";
const TAX_CALCULATIONS_APP_HEADER: &str = "x-shopify-draft-proxy-tax-calculations-app";

fn tax_app_configure_has_authority(request: &Request) -> bool {
    request_has_access_scope(request, "write_taxes")
        && request_header_truthy(request, TAX_CALCULATIONS_APP_HEADER)
}

fn request_has_access_scope(request: &Request, expected: &str) -> bool {
    request_header(request, ACCESS_SCOPES_HEADER).is_some_and(|scopes| {
        scopes
            .split(',')
            .map(str::trim)
            .any(|scope| scope == expected)
    })
}

fn request_header_truthy(request: &Request, header: &str) -> bool {
    request_header(request, header).is_some_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "true" | "1" | "yes"
        )
    })
}

fn tax_app_configure_access_denied_error(field: &FunctionRootInput) -> Value {
    top_level_access_denied_error_envelope(
        format!(
            "Access denied for {} field. Required access: {TAX_APP_CONFIGURE_REQUIRED_ACCESS}",
            field.name
        ),
        Some(field.location),
        vec![json!(field.response_key.clone())],
        Some(TAX_APP_CONFIGURE_REQUIRED_ACCESS),
    )
}

fn normalized_function_metadata(function: Value) -> Option<Value> {
    normalized_function_metadata_with_handle(function, None)
}

fn normalized_function_metadata_with_handle(
    mut function: Value,
    handle: Option<&str>,
) -> Option<Value> {
    function.get("id").and_then(Value::as_str)?;
    let api_type = function
        .get("apiType")
        .and_then(Value::as_str)
        .map(canonical_function_api_type)
        .unwrap_or_default();
    if api_type.is_empty() {
        return None;
    }
    function[FUNCTION_CANONICAL_API_TYPE_FIELD] = json!(api_type);
    if let Some(handle) = handle {
        if function.get("handle").is_none() {
            function["handle"] = json!(handle);
        }
    }
    if function.get("app").is_none() {
        function["app"] = Value::Null;
    }
    if function.get("appKey").is_none() {
        function["appKey"] = Value::Null;
    }
    if function.get("description").is_none() {
        function["description"] = Value::Null;
    }
    Some(function)
}

fn function_metadata_matches_handle(function: &Value, handle: &str) -> bool {
    [
        function["handle"].as_str(),
        function["title"].as_str(),
        function["description"].as_str(),
    ]
    .into_iter()
    .flatten()
    .any(|candidate| candidate == handle)
}

fn canonical_function_api_type(api_type: &str) -> String {
    match api_type {
        "VALIDATION" | "cart_checkout_validation" | "validation" => "VALIDATION".to_string(),
        "CART_TRANSFORM"
        | "cart_transform"
        | "purchase.cart-transform.run"
        | "cart.transform.run" => "CART_TRANSFORM".to_string(),
        "FULFILLMENT_CONSTRAINT_RULE"
        | "fulfillment_constraint_rule"
        | "purchase.fulfillment-constraint-rule.run"
        | "cart.fulfillment-constraints.generate.run" => "FULFILLMENT_CONSTRAINT_RULE".to_string(),
        "DISCOUNT" | "discount" | "product_discounts" | "order_discounts"
        | "shipping_discounts" => "DISCOUNT".to_string(),
        "PAYMENT_CUSTOMIZATION" | "payment_customization" => "PAYMENT_CUSTOMIZATION".to_string(),
        other => other.to_string(),
    }
}

fn requested_function_api_type(arguments: &BTreeMap<String, ResolvedValue>) -> Option<String> {
    resolved_string_field(arguments, "apiType")
        .map(|api_type| canonical_function_api_type(&api_type))
        .filter(|api_type| !api_type.is_empty())
}

fn function_canonical_api_type(function: &Value) -> String {
    function
        .get(FUNCTION_CANONICAL_API_TYPE_FIELD)
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            function["apiType"]
                .as_str()
                .map(canonical_function_api_type)
        })
        .unwrap_or_default()
}

fn function_matches_canonical_api_type(function: &Value, api_type: &str) -> bool {
    function_canonical_api_type(function) == api_type
}

fn function_belongs_to_request(function: &Value, request: &Request) -> bool {
    let Some(caller_api_client_id) = request
        .headers
        .get(API_CLIENT_ID_HEADER)
        .filter(|value| !value.is_empty())
    else {
        return true;
    };
    let function_api_key = function["app"]["apiKey"]
        .as_str()
        .or_else(|| function["appKey"].as_str());
    let function_app_id = function["app"]["id"].as_str().map(resource_id_tail);
    match (function_api_key, function_app_id) {
        (None, None) => true,
        (api_key, app_id) => {
            api_key == Some(caller_api_client_id) || app_id == Some(caller_api_client_id)
        }
    }
}

fn normalized_function_validation(mut validation: Value) -> Option<Value> {
    validation.get("id").and_then(Value::as_str)?;
    if !looks_like_function_validation(&validation) {
        return None;
    }
    if let Some(function) = validation
        .get("shopifyFunction")
        .and_then(|function| normalized_function_metadata(function.clone()))
    {
        validation["shopifyFunction"] = function;
    }
    if validation.get("enabled").is_none() {
        if let Some(enable) = validation.get("enable").cloned() {
            validation["enabled"] = enable;
        }
    }
    if validation.get("enable").is_none() {
        if let Some(enabled) = validation.get("enabled").cloned() {
            validation["enable"] = enabled;
        }
    }
    Some(validation)
}

fn looks_like_function_validation(value: &Value) -> bool {
    value.get("enabled").is_some()
        || value.get("enable").is_some()
        || (value.get("shopifyFunction").is_some() && value.get("functionId").is_none())
}

fn normalized_function_cart_transform(mut cart_transform: Value) -> Option<Value> {
    cart_transform.get("id").and_then(Value::as_str)?;
    if !looks_like_function_cart_transform(&cart_transform) {
        return None;
    }
    if cart_transform.get("metafields").is_some()
        && cart_transform.get("metafield").is_none_or(Value::is_null)
    {
        if let Some(first) = cart_transform["metafields"]["nodes"]
            .as_array()
            .and_then(|nodes| nodes.first())
            .cloned()
        {
            cart_transform["metafield"] = first;
        }
    }
    Some(cart_transform)
}

fn looks_like_function_cart_transform(value: &Value) -> bool {
    shopify_gid_resource_type(value.get("id").and_then(Value::as_str).unwrap_or_default())
        == Some("CartTransform")
        || (value.get("functionId").is_some()
            && value.get("enabled").is_none()
            && value.get("enable").is_none())
}

fn function_identifier_input(
    input: &BTreeMap<String, ResolvedValue>,
) -> (Option<String>, Option<String>) {
    (
        resolved_string_field(input, "functionId"),
        resolved_string_field(input, "functionHandle"),
    )
}

fn function_payload_identifier_field(function_id: &Option<String>) -> &'static str {
    if function_id.is_some() {
        "functionId"
    } else {
        "functionHandle"
    }
}

#[derive(Clone, Copy)]
struct FunctionPayloadDescriptor {
    payload_key: &'static str,
    field_prefix: &'static [&'static str],
    expected_api_type: &'static str,
    api_mismatch_message: &'static str,
    api_mismatch_id_code: &'static str,
    api_mismatch_handle_code: &'static str,
    not_found_code: &'static str,
    not_found_message: FunctionNotFoundMessage,
}

#[derive(Clone, Copy)]
enum FunctionNotFoundMessage {
    ExtensionNotFound,
    CartTransform,
    ReleasedFunction,
}

const VALIDATION_FUNCTION_PAYLOAD: FunctionPayloadDescriptor = FunctionPayloadDescriptor {
    payload_key: "validation",
    field_prefix: &["validation"],
    expected_api_type: "VALIDATION",
    api_mismatch_message: "Unexpected Function API. The provided function must implement one of the following extension targets: [%{targets}].",
    api_mismatch_id_code: "FUNCTION_DOES_NOT_IMPLEMENT",
    api_mismatch_handle_code: "FUNCTION_DOES_NOT_IMPLEMENT",
    not_found_code: "NOT_FOUND",
    not_found_message: FunctionNotFoundMessage::ExtensionNotFound,
};

const CART_TRANSFORM_FUNCTION_PAYLOAD: FunctionPayloadDescriptor = FunctionPayloadDescriptor {
    payload_key: "cartTransform",
    field_prefix: &[],
    expected_api_type: "CART_TRANSFORM",
    api_mismatch_message: "Unexpected Function API. The provided function must implement one of the following extension targets: [purchase.cart-transform.run, cart.transform.run].",
    api_mismatch_id_code: "FUNCTION_NOT_FOUND",
    api_mismatch_handle_code: "FUNCTION_DOES_NOT_IMPLEMENT",
    not_found_code: "FUNCTION_NOT_FOUND",
    not_found_message: FunctionNotFoundMessage::CartTransform,
};

const FULFILLMENT_CONSTRAINT_RULE_FUNCTION_PAYLOAD: FunctionPayloadDescriptor =
    FunctionPayloadDescriptor {
        payload_key: "fulfillmentConstraintRule",
        field_prefix: &[],
        expected_api_type: "FULFILLMENT_CONSTRAINT_RULE",
        api_mismatch_message: "Unexpected Function API. The provided function must implement one of the following extension targets: [purchase.fulfillment-constraint-rule.run, cart.fulfillment-constraints.generate.run].",
        api_mismatch_id_code: "FUNCTION_DOES_NOT_IMPLEMENT",
        api_mismatch_handle_code: "FUNCTION_DOES_NOT_IMPLEMENT",
        not_found_code: "FUNCTION_NOT_FOUND",
        not_found_message: FunctionNotFoundMessage::ReleasedFunction,
    };

fn maximum_cart_transforms_error() -> Value {
    payload_user_error(
        CART_TRANSFORM_FUNCTION_PAYLOAD.payload_key,
        user_error(
            ["base"],
            "The maximum number of cart transforms per shop has been reached.",
            Some("MAXIMUM_CART_TRANSFORMS"),
        ),
    )
}

fn function_identifier_error(
    desc: FunctionPayloadDescriptor,
    function_id: &Option<String>,
    function_handle: &Option<String>,
) -> Option<Value> {
    match (function_id.is_some(), function_handle.is_some()) {
        (false, false) => Some(payload_user_error(
            desc.payload_key,
            user_error(
                function_error_field(desc, "functionHandle"),
                "Either function_id or function_handle must be provided.",
                Some("MISSING_FUNCTION_IDENTIFIER"),
            ),
        )),
        (true, true) => Some(payload_user_error(
            desc.payload_key,
            user_error(
                function_multiple_identifier_field(desc),
                "Only one of function_id or function_handle can be provided, not both.",
                Some("MULTIPLE_FUNCTION_IDENTIFIERS"),
            ),
        )),
        _ => None,
    }
}

fn function_error_field(desc: FunctionPayloadDescriptor, field_name: &str) -> Vec<String> {
    let mut field = desc
        .field_prefix
        .iter()
        .map(|segment| (*segment).to_string())
        .collect::<Vec<_>>();
    field.push(field_name.to_string());
    field
}

fn function_multiple_identifier_field(desc: FunctionPayloadDescriptor) -> Vec<String> {
    if desc.field_prefix.is_empty() {
        function_error_field(desc, "functionHandle")
    } else {
        desc.field_prefix
            .iter()
            .map(|segment| (*segment).to_string())
            .collect()
    }
}

fn function_not_found_message(
    desc: FunctionPayloadDescriptor,
    function_id: &Option<String>,
    function_handle: &Option<String>,
    current_app_id: &str,
) -> String {
    match desc.not_found_message {
        FunctionNotFoundMessage::ExtensionNotFound => "Extension not found.".to_string(),
        FunctionNotFoundMessage::CartTransform => {
            if let Some(id) = function_id {
                format!(
                    "Function {id} not found. Ensure that it is released in the current app ({current_app_id}), and that the app is installed."
                )
            } else if let Some(handle) = function_handle {
                format!("Could not find function with handle: {handle}.")
            } else {
                "Function not found.".to_string()
            }
        }
        FunctionNotFoundMessage::ReleasedFunction => {
            if let Some(identifier) = function_id.as_deref().or(function_handle.as_deref()) {
                format!(
                    "Function {identifier} not found. Ensure that it is released in the current app ({current_app_id}), and that the app is installed."
                )
            } else {
                "Function not found.".to_string()
            }
        }
    }
}

fn function_not_found_error(
    desc: FunctionPayloadDescriptor,
    field_name: &str,
    function_id: &Option<String>,
    function_handle: &Option<String>,
    current_app_id: &str,
) -> Value {
    let message = function_not_found_message(desc, function_id, function_handle, current_app_id);
    payload_user_error(
        desc.payload_key,
        user_error(
            function_error_field(desc, field_name),
            &message,
            Some(desc.not_found_code),
        ),
    )
}

fn function_resolution_payload(
    proxy: &mut DraftProxy,
    request: &Request,
    desc: FunctionPayloadDescriptor,
    function_id: &Option<String>,
    function_handle: &Option<String>,
) -> Result<Value, Value> {
    if let Some(payload) = function_identifier_error(desc, function_id, function_handle) {
        return Err(payload);
    }
    let field_name = function_payload_identifier_field(function_id);
    let current_app_id = request_api_client_id(request);
    let function = proxy
        .resolve_function_metadata(
            request,
            function_id.as_deref(),
            function_handle.as_deref(),
            desc.expected_api_type,
        )
        .ok_or_else(|| {
            function_not_found_error(
                desc,
                field_name,
                function_id,
                function_handle,
                &current_app_id,
            )
        })?;
    if !function_matches_canonical_api_type(&function, desc.expected_api_type) {
        let code = if function_id.is_some() {
            desc.api_mismatch_id_code
        } else {
            desc.api_mismatch_handle_code
        };
        return Err(payload_user_error(
            desc.payload_key,
            user_error(
                function_error_field(desc, field_name),
                desc.api_mismatch_message,
                Some(code),
            ),
        ));
    }
    if let Some(code) = function["createGuardrailCode"].as_str() {
        return Err(payload_user_error(
            desc.payload_key,
            user_error(
                function_error_field(desc, field_name),
                function["createGuardrailMessage"]
                    .as_str()
                    .unwrap_or_default(),
                Some(code),
            ),
        ));
    }
    Ok(function)
}

fn metafield_input_error(
    metafield: &BTreeMap<String, ResolvedValue>,
    index: usize,
) -> Option<Value> {
    let field = vec![
        "validation".to_string(),
        "metafields".to_string(),
        index.to_string(),
    ];
    let namespace = resolved_string_field(metafield, "namespace").unwrap_or_default();
    let key = resolved_string_field(metafield, "key");
    let type_name = resolved_string_field(metafield, "type");
    let value = resolved_string_field(metafield, "value");

    if key.is_none() {
        return Some(user_error(field, "presence", None));
    }
    if type_name.as_deref().unwrap_or_default().is_empty() {
        return Some(user_error(
            field,
            "One or more required inputs are blank.",
            Some("BLANK"),
        ));
    }
    if value.is_none() {
        return Some(user_error(field, "presence", None));
    }
    if namespace == "shopify" {
        return Some(user_error(
            field,
            "ApiPermission metafields can only be created or updated by the app owner.",
            Some("APP_NOT_AUTHORIZED"),
        ));
    }
    let type_name = type_name.as_deref().unwrap_or_default();
    if !metafield_definition_type_allowed(type_name) {
        return Some(user_error(
            field,
            "The type is invalid.",
            Some("INVALID_TYPE"),
        ));
    }
    let mut reference_exists = validation_metafield_reference_exists;
    metafield_value_error_message(
        type_name,
        value.as_deref().unwrap_or_default(),
        &mut reference_exists,
    )
    .map(|_| user_error(field, "The value is invalid.", Some("INVALID_VALUE")))
}

fn validation_metafield_reference_exists(_: &str) -> bool {
    true
}

fn function_metafield_errors<MetafieldError, InvalidValueError>(
    metafields: Option<&ResolvedValue>,
    metafield_error: MetafieldError,
    invalid_value_error: InvalidValueError,
) -> Vec<Value>
where
    MetafieldError: Fn(&BTreeMap<String, ResolvedValue>, usize) -> Option<Value>,
    InvalidValueError: Fn(usize) -> Value,
{
    match metafields {
        Some(ResolvedValue::List(metafields)) => metafields
            .iter()
            .enumerate()
            .filter_map(|(index, value)| match value {
                ResolvedValue::Object(metafield) => metafield_error(metafield, index),
                _ => Some(invalid_value_error(index)),
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn validation_metafield_errors(input: &BTreeMap<String, ResolvedValue>) -> Vec<Value> {
    function_metafield_errors(input.get("metafields"), metafield_input_error, |index| {
        user_error(
            vec![
                "validation".to_string(),
                "metafields".to_string(),
                index.to_string(),
            ],
            "The value is invalid.",
            Some("INVALID_VALUE"),
        )
    })
}

fn validation_metafields_from_input(
    input: &BTreeMap<String, ResolvedValue>,
    timestamp: &str,
) -> Vec<Value> {
    match input.get("metafields") {
        Some(ResolvedValue::List(metafields)) => metafields
            .iter()
            .filter_map(|value| match value {
                ResolvedValue::Object(metafield) => Some(json!({
                    "namespace": resolved_string_field(metafield, "namespace").unwrap_or_default(),
                    "key": resolved_string_field(metafield, "key").unwrap_or_default(),
                    "type": resolved_string_field(metafield, "type").unwrap_or_default(),
                    "value": resolved_string_field(metafield, "value").unwrap_or_default(),
                    "updatedAt": timestamp
                })),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn validation_metafield_connection(metafields: Vec<Value>) -> Value {
    json!({ "nodes": metafields })
}

fn upsert_validation_metafields(record: &mut Value, metafields: Vec<Value>) {
    let existing = record["metafields"]["nodes"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mut merged = existing;
    for metafield in metafields {
        let namespace = metafield["namespace"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let key = metafield["key"].as_str().unwrap_or_default().to_string();
        if let Some(existing) = merged.iter_mut().find(|existing| {
            existing["namespace"].as_str() == Some(namespace.as_str())
                && existing["key"].as_str() == Some(key.as_str())
        }) {
            *existing = metafield;
        } else {
            merged.push(metafield);
        }
    }
    record["metafields"] = validation_metafield_connection(merged);
}

fn selected_title(input: &BTreeMap<String, ResolvedValue>, function: &Value) -> String {
    match input.get("title") {
        Some(ResolvedValue::String(title)) => title.clone(),
        Some(ResolvedValue::Null) | None => {
            function["title"].as_str().unwrap_or_default().to_string()
        }
        _ => String::new(),
    }
}

fn function_connection_arguments(field: &FunctionRootInput) -> BTreeMap<String, ResolvedValue> {
    let mut arguments = field.arguments.clone();
    // Hydrated upstream records are already in Shopify's default order. Keep
    // that order while merging staged records unless the caller explicitly
    // requests a sort key; the schema-injected ID default must not compare
    // synthetic and authoritative IDs as ordinary strings.
    if !field.raw_arguments.contains_key("sortKey") {
        arguments.remove("sortKey");
    }
    arguments
}

fn local_function_cursor(node: &Value) -> String {
    node["id"]
        .as_str()
        .map(|id| format!("cursor:{id}"))
        .unwrap_or_default()
}

pub(in crate::proxy) fn local_function_connection_from_nodes_with_args(
    mut nodes: Vec<Value>,
    arguments: &BTreeMap<String, ResolvedValue>,
) -> Value {
    if let Some(query) = resolved_string_field(arguments, "query") {
        nodes.retain(|node| function_node_matches_query(node, &query));
    }
    if let Some(sort_key) = resolved_string_field(arguments, "sortKey") {
        sort_function_nodes(&mut nodes, &sort_key);
    }
    if resolved_bool_field(arguments, "reverse").unwrap_or(false) {
        nodes.reverse();
    }
    let (nodes, page_info) = connection_window(&nodes, arguments, local_function_cursor);
    connection_json_with_cursor(nodes, |_, node| local_function_cursor(node), page_info)
}

fn function_node_matches_query(node: &Value, query: &str) -> bool {
    let query = query.trim();
    if query.is_empty() {
        return true;
    }
    [
        node["id"].as_str(),
        node["title"].as_str(),
        node["handle"].as_str(),
        node["functionId"].as_str(),
        node["functionHandle"].as_str(),
        node["apiType"].as_str(),
        node["shopifyFunction"]["handle"].as_str(),
        node["shopifyFunction"]["title"].as_str(),
    ]
    .into_iter()
    .flatten()
    .any(|candidate| search_string_matches(candidate, query))
}

fn sort_function_nodes(nodes: &mut [Value], sort_key: &str) {
    match sort_key {
        "TITLE" | "title" => nodes.sort_by(|left, right| {
            left["title"]
                .as_str()
                .unwrap_or_default()
                .cmp(right["title"].as_str().unwrap_or_default())
                .then_with(|| {
                    left["id"]
                        .as_str()
                        .unwrap_or_default()
                        .cmp(right["id"].as_str().unwrap_or_default())
                })
        }),
        "ID" | "id" => nodes.sort_by(|left, right| {
            left["id"]
                .as_str()
                .unwrap_or_default()
                .cmp(right["id"].as_str().unwrap_or_default())
        }),
        _ => {}
    }
}

fn cart_transform_metafield_error(
    metafield: &BTreeMap<String, ResolvedValue>,
    index: usize,
) -> Option<Value> {
    let value = resolved_string_field(metafield, "value").unwrap_or_default();
    if value.is_empty() {
        return Some(user_error(
            vec![
                "metafields".to_string(),
                index.to_string(),
                "value".to_string(),
            ],
            "may not be empty",
            Some("INVALID_METAFIELDS"),
        ));
    }
    if resolved_string_field(metafield, "type").as_deref() == Some("json")
        && serde_json::from_str::<Value>(&value).is_err()
    {
        return Some(user_error(
            vec![
                "metafields".to_string(),
                index.to_string(),
                "value".to_string(),
            ],
            &format!(
                "is invalid JSON: unexpected token '{}' at line 1 column 1.",
                value
            ),
            Some("INVALID_METAFIELDS"),
        ));
    }
    None
}

fn cart_transform_metafield_errors(field: &FunctionRootInput) -> Vec<Value> {
    function_metafield_errors(
        field.arguments.get("metafields"),
        cart_transform_metafield_error,
        |index| {
            user_error(
                vec![
                    "metafields".to_string(),
                    index.to_string(),
                    "value".to_string(),
                ],
                "may not be empty",
                Some("INVALID_METAFIELDS"),
            )
        },
    )
}

fn delete_staged_function_record(
    records: &mut BTreeMap<String, Value>,
    order: &mut Vec<String>,
    singleton: Option<&mut Option<Value>>,
    id: &str,
    deleted_payload: Value,
    not_found_payload: Value,
) -> (Value, bool) {
    if records.remove(id).is_none() {
        return (not_found_payload, false);
    }
    order.retain(|ordered_id| ordered_id != id);
    if let Some(singleton) = singleton {
        if singleton.as_ref().and_then(|record| record["id"].as_str()) == Some(id) {
            *singleton = order.last().and_then(|id| records.get(id).cloned());
        }
    }
    (deleted_payload, true)
}

fn function_metafields_from_field<IdForMetafield, DigestForValue>(
    field: &FunctionRootInput,
    ids: &[String],
    owner_type: &str,
    id_for_metafield: IdForMetafield,
    digest_for_value: DigestForValue,
    timestamp: &str,
) -> Vec<Value>
where
    IdForMetafield: Fn(usize, &[String], &str, &str, &str) -> String,
    DigestForValue: Fn(usize, &str) -> String,
{
    match field.arguments.get("metafields") {
        Some(ResolvedValue::List(metafields)) => metafields
            .iter()
            .enumerate()
            .filter_map(|(index, value)| match value {
                ResolvedValue::Object(metafield) => {
                    let namespace =
                        resolved_string_field(metafield, "namespace").unwrap_or_default();
                    let key = resolved_string_field(metafield, "key").unwrap_or_default();
                    let metafield_type =
                        resolved_string_field(metafield, "type").unwrap_or_default();
                    let value = resolved_string_field(metafield, "value").unwrap_or_default();
                    let compare_digest = digest_for_value(index, &value);
                    Some(json!({
                        "id": id_for_metafield(index, ids, &namespace, &key, &value),
                        "namespace": namespace,
                        "key": key,
                        "type": metafield_type,
                        "value": value,
                        "compareDigest": compare_digest,
                        "ownerType": owner_type,
                        "createdAt": timestamp,
                        "updatedAt": timestamp
                    }))
                }
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn cart_transform_metafield_id(owner_id: &str, namespace: &str, key: &str) -> String {
    let digest = metafield_compare_digest(&format!("{owner_id}\n{namespace}\n{key}"));
    shopify_gid("Metafield", &digest[..16])
}

pub(in crate::proxy) fn cart_transform_record_value(record: &Value) -> Value {
    function_record_with_output_fields(record, "CartTransform", CART_TRANSFORM_OUTPUT_FIELDS)
}

fn fulfillment_constraint_rule_delivery_method_types(field: &FunctionRootInput) -> Vec<String> {
    list_string_field(&field.arguments, "deliveryMethodTypes")
}

fn fulfillment_constraint_rule_delivery_method_error(
    delivery_method_types: &[String],
) -> Option<Value> {
    if delivery_method_types.is_empty() {
        Some(payload_user_error(
            FULFILLMENT_CONSTRAINT_RULE_FUNCTION_PAYLOAD.payload_key,
            user_error(
                ["deliveryMethodTypes"],
                "Delivery method types cannot be empty.",
                Some("INPUT_INVALID"),
            ),
        ))
    } else {
        None
    }
}

const VALIDATION_OUTPUT_FIELDS: &[&str] = &[
    "id",
    "title",
    "enabled",
    "shopifyFunction",
    "blockOnFailure",
    "errorHistory",
    "metafield",
    "metafields",
    "__typename",
];

pub(in crate::proxy) fn validation_record_value(record: &Value) -> Value {
    function_record_with_output_fields(record, "Validation", VALIDATION_OUTPUT_FIELDS)
}

const FULFILLMENT_CONSTRAINT_RULE_OUTPUT_FIELDS: &[&str] = &[
    "id",
    "function",
    "deliveryMethodTypes",
    "metafield",
    "metafields",
    "__typename",
];

pub(in crate::proxy) fn fulfillment_constraint_rule_record_value(record: &Value) -> Value {
    function_record_with_output_fields(
        record,
        "FulfillmentConstraintRule",
        FULFILLMENT_CONSTRAINT_RULE_OUTPUT_FIELDS,
    )
}

fn function_record_with_output_fields(
    record: &Value,
    type_name: &str,
    output_fields: &[&str],
) -> Value {
    let mut public = serde_json::Map::new();
    for field in output_fields {
        if *field == "__typename" {
            public.insert(field.to_string(), json!(type_name));
        } else if let Some(value) = record.get(*field) {
            public.insert(field.to_string(), value.clone());
        }
    }
    Value::Object(public)
}

impl DraftProxy {
    fn function_validation_create_payload(
        &mut self,
        request: &Request,
        field: &FunctionRootInput,
    ) -> Value {
        let input = match field.arguments.get("validation") {
            Some(ResolvedValue::Object(input)) => input,
            _ => {
                return payload_user_error(
                    VALIDATION_FUNCTION_PAYLOAD.payload_key,
                    user_error(
                        ["validation"],
                        "Required input field must be present.",
                        Some("REQUIRED_INPUT_FIELD"),
                    ),
                );
            }
        };
        let (function_id, function_handle) = function_identifier_input(input);
        let function = match function_resolution_payload(
            self,
            request,
            VALIDATION_FUNCTION_PAYLOAD,
            &function_id,
            &function_handle,
        ) {
            Ok(function) => function,
            Err(payload) => return payload,
        };
        let errors = validation_metafield_errors(input);
        if !errors.is_empty() {
            return payload_error(VALIDATION_FUNCTION_PAYLOAD.payload_key, errors);
        }
        let enable = resolved_bool_field(input, "enable").unwrap_or(false);
        if enable && self.effective_active_validation_count(None) >= 25 {
            return payload_user_error(
                VALIDATION_FUNCTION_PAYLOAD.payload_key,
                user_error(
                    Vec::<&str>::new(),
                    "Cannot have more than 25 active validation functions.",
                    Some("MAX_VALIDATIONS_ACTIVATED"),
                ),
            );
        }
        let id = self.next_proxy_synthetic_gid("Validation");
        let timestamp = self.next_product_timestamp();
        let metafields = validation_metafields_from_input(input, &timestamp);
        let validation = json!({
            "id": id,
            "title": selected_title(input, &function),
            "enable": enable,
            "enabled": enable,
            "blockOnFailure": resolved_bool_field(input, "blockOnFailure").unwrap_or(false),
            "functionId": function["id"].clone(),
            "functionHandle": function["handle"].clone(),
            "createdAt": timestamp.clone(),
            "updatedAt": timestamp,
            "shopifyFunction": function,
            "metafields": validation_metafield_connection(metafields)
        });
        self.stage_function_validation(validation.clone());
        json!({ "validation": validation, "userErrors": [] })
    }

    fn function_validation_update_payload(
        &mut self,
        request: &Request,
        field: &FunctionRootInput,
    ) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let input = match field.arguments.get("validation") {
            Some(ResolvedValue::Object(input)) => input,
            _ => {
                return payload_user_error(
                    VALIDATION_FUNCTION_PAYLOAD.payload_key,
                    user_error(
                        ["validation"],
                        "Required input field must be present.",
                        Some("REQUIRED_INPUT_FIELD"),
                    ),
                );
            }
        };
        if self
            .store
            .staged
            .deleted_function_validation_ids
            .contains(&id)
        {
            return payload_user_error(
                VALIDATION_FUNCTION_PAYLOAD.payload_key,
                user_error(["id"], "Extension not found.", Some("NOT_FOUND")),
            );
        }
        if self.function_validation_by_id(&id).is_none()
            && self.config.read_mode != ReadMode::Snapshot
        {
            self.hydrate_function_validation_by_id(request, &id);
        }
        let Some(mut validation) = self.function_validation_by_id(&id).cloned() else {
            return payload_user_error(
                VALIDATION_FUNCTION_PAYLOAD.payload_key,
                user_error(["id"], "Extension not found.", Some("NOT_FOUND")),
            );
        };
        let errors = validation_metafield_errors(input);
        if !errors.is_empty() {
            return payload_error(VALIDATION_FUNCTION_PAYLOAD.payload_key, errors);
        }
        let next_enable = resolved_bool_field(input, "enable")
            .or_else(|| resolved_bool_field(input, "enabled"))
            .unwrap_or(false);
        if next_enable && self.effective_active_validation_count(Some(&id)) >= 25 {
            return payload_user_error(
                VALIDATION_FUNCTION_PAYLOAD.payload_key,
                user_error(
                    Vec::<&str>::new(),
                    "Cannot have more than 25 active validation functions.",
                    Some("MAX_VALIDATIONS_ACTIVATED"),
                ),
            );
        }
        if let Some(title) = resolved_string_field(input, "title") {
            validation["title"] = json!(title);
        }
        validation["enable"] = json!(next_enable);
        validation["enabled"] = json!(next_enable);
        validation["blockOnFailure"] =
            json!(resolved_bool_field(input, "blockOnFailure").unwrap_or(false));
        let timestamp = self.next_product_timestamp();
        validation["updatedAt"] = json!(timestamp.clone());
        let metafield_updates = validation_metafields_from_input(input, &timestamp);
        if !metafield_updates.is_empty() {
            upsert_validation_metafields(&mut validation, metafield_updates);
        }
        self.stage_function_validation(validation.clone());
        json!({ "validation": validation, "userErrors": [] })
    }

    fn function_validation_delete_payload(
        &mut self,
        request: &Request,
        field: &FunctionRootInput,
    ) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        if self.function_validation_by_id(&id).is_none()
            && self.config.read_mode != ReadMode::Snapshot
        {
            self.hydrate_function_validation_by_id(request, &id);
        }
        let (payload, deleted) = delete_staged_function_record(
            &mut self.store.staged.function_validations,
            &mut self.store.staged.function_validation_order,
            Some(&mut self.store.staged.function_validation),
            &id,
            json!({ "deletedId": id.clone(), "userErrors": [] }),
            payload_user_error(
                "deletedId",
                user_error(["id"], "Extension not found.", Some("NOT_FOUND")),
            ),
        );
        let base_deleted = self.store.base.function_validations.contains_key(&id);
        let deleted = deleted || base_deleted;
        if deleted {
            self.store.staged.functions_dirty = true;
            self.store.staged.function_validations_dirty = true;
            self.store
                .staged
                .deleted_function_validation_ids
                .insert(id.clone());
        }
        if base_deleted {
            json!({ "deletedId": id, "userErrors": [] })
        } else {
            payload
        }
    }

    fn function_cart_transform_create_payload(
        &mut self,
        request: &Request,
        field: &FunctionRootInput,
    ) -> Value {
        let function_id = resolved_string_field(&field.arguments, "functionId");
        let function_handle = resolved_string_field(&field.arguments, "functionHandle");
        if let Some(payload) = function_identifier_error(
            CART_TRANSFORM_FUNCTION_PAYLOAD,
            &function_id,
            &function_handle,
        ) {
            return payload;
        }
        if let Some(function_id) = function_id.as_deref() {
            if self.effective_function_id_in_use(function_id) {
                return payload_user_error(
                    CART_TRANSFORM_FUNCTION_PAYLOAD.payload_key,
                    user_error(
                        ["functionId"],
                        "Could not enable cart transform because it is already registered",
                        Some("FUNCTION_ALREADY_REGISTERED"),
                    ),
                );
            }
        }
        let function = match function_resolution_payload(
            self,
            request,
            CART_TRANSFORM_FUNCTION_PAYLOAD,
            &function_id,
            &function_handle,
        ) {
            Ok(function) => function,
            Err(payload) => return payload,
        };
        if self.effective_cart_transform_count() > 0 {
            return maximum_cart_transforms_error();
        }
        let errors = cart_transform_metafield_errors(field);
        if !errors.is_empty() {
            return payload_error(CART_TRANSFORM_FUNCTION_PAYLOAD.payload_key, errors);
        }
        let id = self.next_proxy_synthetic_gid("CartTransform");
        let metafield_ids: Vec<String> = Vec::new();
        let timestamp = self.next_product_timestamp();
        let metafields = function_metafields_from_field(
            field,
            &metafield_ids,
            "CARTTRANSFORM",
            |_, _, namespace, key, _| cart_transform_metafield_id(&id, namespace, key),
            |_, value| metafield_compare_digest(value),
            &timestamp,
        );
        for metafield in &metafields {
            if let (Some(namespace), Some(key)) = (
                metafield.get("namespace").and_then(Value::as_str),
                metafield.get("key").and_then(Value::as_str),
            ) {
                self.store.staged.deleted_owner_metafields.remove(&(
                    id.clone(),
                    namespace.to_string(),
                    key.to_string(),
                ));
            }
        }
        if !metafields.is_empty() {
            self.store
                .staged
                .owner_metafields
                .insert(id.clone(), metafields.clone());
        }
        let first_metafield = metafields.first().cloned().unwrap_or(Value::Null);
        let mut cart_transform = json!({
            "id": id,
            "blockOnFailure": resolved_bool_field(&field.arguments, "blockOnFailure").unwrap_or(false),
            "functionId": function["id"].clone(),
            "shopifyFunction": function,
            "metafield": first_metafield,
            "metafields": { "nodes": metafields }
        });
        if cart_transform["metafield"].is_null() {
            cart_transform.as_object_mut().unwrap().remove("metafield");
        }
        self.stage_function_cart_transform(cart_transform.clone());
        json!({ "cartTransform": cart_transform, "userErrors": [] })
    }

    fn function_cart_transform_delete_payload(
        &mut self,
        request: &Request,
        field: &FunctionRootInput,
    ) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        if self.function_cart_transform_by_id(&id).is_none()
            && self.config.read_mode != ReadMode::Snapshot
        {
            self.hydrate_function_cart_transform_by_id(request, &id);
        }
        let (payload, deleted) = delete_staged_function_record(
            &mut self.store.staged.function_cart_transforms,
            &mut self.store.staged.function_cart_transform_order,
            Some(&mut self.store.staged.function_cart_transform),
            &id,
            json!({ "deletedId": id.clone(), "userErrors": [] }),
            payload_user_error(
                "deletedId",
                user_error(
                    ["id"],
                    &format!("Could not find cart transform with id: {id}"),
                    Some("NOT_FOUND"),
                ),
            ),
        );
        let base_deleted = self.store.base.function_cart_transforms.contains_key(&id);
        let deleted = deleted || base_deleted;
        if deleted {
            self.store.staged.functions_dirty = true;
            self.store.staged.function_cart_transforms_dirty = true;
            self.store
                .staged
                .deleted_function_cart_transform_ids
                .insert(id.clone());
        }
        if base_deleted {
            json!({ "deletedId": id, "userErrors": [] })
        } else {
            payload
        }
    }

    fn function_fulfillment_constraint_rule_create_payload(
        &mut self,
        request: &Request,
        field: &FunctionRootInput,
    ) -> Value {
        let function_id = resolved_string_field(&field.arguments, "functionId");
        let function_handle = resolved_string_field(&field.arguments, "functionHandle");
        if let Some(payload) = function_identifier_error(
            FULFILLMENT_CONSTRAINT_RULE_FUNCTION_PAYLOAD,
            &function_id,
            &function_handle,
        ) {
            return payload;
        }
        let delivery_method_types = fulfillment_constraint_rule_delivery_method_types(field);
        if let Some(payload) =
            fulfillment_constraint_rule_delivery_method_error(&delivery_method_types)
        {
            return payload;
        }
        let function = match function_resolution_payload(
            self,
            request,
            FULFILLMENT_CONSTRAINT_RULE_FUNCTION_PAYLOAD,
            &function_id,
            &function_handle,
        ) {
            Ok(function) => function,
            Err(payload) => return payload,
        };
        let id = self.next_synthetic_gid("FulfillmentConstraintRule");
        let metafield_ids = match field.arguments.get("metafields") {
            Some(ResolvedValue::List(metafields)) => metafields
                .iter()
                .map(|_| self.next_proxy_synthetic_gid("Metafield"))
                .collect(),
            _ => Vec::new(),
        };
        let timestamp = self.next_product_timestamp();
        let metafields = function_metafields_from_field(
            field,
            &metafield_ids,
            "FULFILLMENT_CONSTRAINT_RULE",
            |index, ids, _, _, _| {
                ids.get(index)
                    .cloned()
                    .unwrap_or_else(|| shopify_gid("Metafield", index + 1))
            },
            |_, value| metafield_compare_digest(value),
            &timestamp,
        );
        let first_metafield = metafields.first().cloned().unwrap_or(Value::Null);
        let mut rule = json!({
            "id": id,
            "deliveryMethodTypes": delivery_method_types,
            "functionId": function["id"].clone(),
            "functionHandle": function["handle"].clone(),
            "function": function.clone(),
            "shopifyFunction": function,
            "metafield": first_metafield,
            "metafields": { "nodes": metafields }
        });
        if rule["metafield"].is_null() {
            rule.as_object_mut().unwrap().remove("metafield");
        }
        self.stage_function_fulfillment_constraint_rule(rule.clone());
        json!({ "fulfillmentConstraintRule": rule, "userErrors": [] })
    }

    fn function_fulfillment_constraint_rule_update_payload(
        &mut self,
        request: &Request,
        field: &FunctionRootInput,
    ) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let function_id = resolved_string_field(&field.arguments, "functionId");
        let function_handle = resolved_string_field(&field.arguments, "functionHandle");
        if function_id.is_some() || function_handle.is_some() {
            if let Some(payload) = function_identifier_error(
                FULFILLMENT_CONSTRAINT_RULE_FUNCTION_PAYLOAD,
                &function_id,
                &function_handle,
            ) {
                return payload;
            }
        }
        let delivery_method_types = fulfillment_constraint_rule_delivery_method_types(field);
        if let Some(payload) =
            fulfillment_constraint_rule_delivery_method_error(&delivery_method_types)
        {
            return payload;
        }
        let Some(mut rule) = self
            .store
            .staged
            .function_fulfillment_constraint_rules
            .get(&id)
            .cloned()
        else {
            return payload_user_error(
                FULFILLMENT_CONSTRAINT_RULE_FUNCTION_PAYLOAD.payload_key,
                user_error(
                    ["id"],
                    &format!("Could not find FulfillmentConstraintRule with id: {id}"),
                    Some("NOT_FOUND"),
                ),
            );
        };
        if function_id.is_some() || function_handle.is_some() {
            let function = match function_resolution_payload(
                self,
                request,
                FULFILLMENT_CONSTRAINT_RULE_FUNCTION_PAYLOAD,
                &function_id,
                &function_handle,
            ) {
                Ok(function) => function,
                Err(payload) => return payload,
            };
            rule["functionId"] = function["id"].clone();
            rule["functionHandle"] = function["handle"].clone();
            rule["function"] = function.clone();
            rule["shopifyFunction"] = function;
        }
        rule["deliveryMethodTypes"] = json!(delivery_method_types);
        self.stage_function_fulfillment_constraint_rule(rule.clone());
        json!({ "fulfillmentConstraintRule": rule, "userErrors": [] })
    }

    fn function_fulfillment_constraint_rule_delete_payload(
        &mut self,
        field: &FunctionRootInput,
    ) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let (payload, deleted) = delete_staged_function_record(
            &mut self.store.staged.function_fulfillment_constraint_rules,
            &mut self.store.staged.function_fulfillment_constraint_rule_order,
            None,
            &id,
            json!({ "success": true, "userErrors": [] }),
            json!({
                "success": false,
                "userErrors": [user_error(
                    ["id"],
                    &format!("Could not find FulfillmentConstraintRule with id: {id}"),
                    Some("NOT_FOUND")
                )]
            }),
        );
        let base_deleted = self
            .store
            .base
            .function_fulfillment_constraint_rules
            .contains_key(&id);
        let deleted = deleted || base_deleted;
        if deleted {
            self.store.staged.functions_dirty = true;
            self.store
                .staged
                .function_fulfillment_constraint_rules_dirty = true;
            self.store
                .staged
                .deleted_function_fulfillment_constraint_rule_ids
                .insert(id.clone());
        }
        if base_deleted {
            json!({ "success": true, "userErrors": [] })
        } else {
            payload
        }
    }

    fn function_tax_app_configure_payload(&mut self, field: &FunctionRootInput) -> Value {
        let ready = resolved_bool_field(&field.arguments, "ready").unwrap_or(true);
        let id = self
            .store
            .staged
            .tax_app_configuration
            .as_ref()
            .and_then(|configuration| configuration["id"].as_str())
            .map(str::to_string)
            .unwrap_or_else(|| self.next_proxy_synthetic_gid("TaxAppConfiguration"));
        let configuration = json!({
            "__typename": "TaxAppConfiguration",
            "id": id,
            "ready": ready,
            "state": if ready { "READY" } else { "PENDING" },
            "updatedAt": self.next_product_timestamp()
        });
        self.store.staged.functions_dirty = true;
        self.store.staged.tax_app_configuration = Some(configuration.clone());
        json!({ "taxAppConfiguration": configuration, "userErrors": [] })
    }

    fn stage_function_validation(&mut self, validation: Value) {
        self.store.staged.function_validations_dirty = true;
        if let Some(id) = validation["id"].as_str() {
            self.store.staged.deleted_function_validation_ids.remove(id);
        }
        stage_function_record(
            &mut self.store.staged.functions_dirty,
            &mut self.store.staged.function_validations,
            &mut self.store.staged.function_validation_order,
            Some(&mut self.store.staged.function_validation),
            validation,
        );
    }

    fn stage_function_cart_transform(&mut self, cart_transform: Value) {
        self.store.staged.function_cart_transforms_dirty = true;
        if let Some(id) = cart_transform["id"].as_str() {
            self.store
                .staged
                .deleted_function_cart_transform_ids
                .remove(id);
        }
        stage_function_record(
            &mut self.store.staged.functions_dirty,
            &mut self.store.staged.function_cart_transforms,
            &mut self.store.staged.function_cart_transform_order,
            Some(&mut self.store.staged.function_cart_transform),
            cart_transform,
        );
    }

    fn stage_function_fulfillment_constraint_rule(&mut self, rule: Value) {
        self.store
            .staged
            .function_fulfillment_constraint_rules_dirty = true;
        if let Some(id) = rule["id"].as_str() {
            self.store
                .staged
                .deleted_function_fulfillment_constraint_rule_ids
                .remove(id);
        }
        stage_function_record(
            &mut self.store.staged.functions_dirty,
            &mut self.store.staged.function_fulfillment_constraint_rules,
            &mut self.store.staged.function_fulfillment_constraint_rule_order,
            None,
            rule,
        );
    }
}

fn stage_function_record(
    functions_dirty: &mut bool,
    records: &mut BTreeMap<String, Value>,
    order: &mut Vec<String>,
    singleton: Option<&mut Option<Value>>,
    record: Value,
) {
    *functions_dirty = true;
    let Some(id) = record["id"].as_str().map(str::to_string) else {
        return;
    };
    if !records.contains_key(&id) {
        order.push(id.clone());
    }
    if let Some(singleton) = singleton {
        *singleton = Some(record.clone());
    }
    records.insert(id, record);
}

/// Output fields defined on the `CartTransform` type (2026-04). A selection of
/// anything else is a query-validation error (`undefinedField`) Shopify rejects
/// before execution — so the read returns errors with no data.
const CART_TRANSFORM_OUTPUT_FIELDS: &[&str] = &[
    "id",
    "functionId",
    "blockOnFailure",
    "metafield",
    "metafields",
    "__typename",
];

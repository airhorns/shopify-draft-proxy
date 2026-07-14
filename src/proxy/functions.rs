use super::*;
use crate::proxy::search::search_string_matches;

const FUNCTION_CANONICAL_API_TYPE_FIELD: &str = "__draftProxyCanonicalApiType";

const FUNCTION_HYDRATE_BY_ID_QUERY: &str = "query FunctionHydrateById($id: String!) {\n  shopifyFunction(id: $id) {\n    id\n    title\n    apiType\n    description\n    appKey\n    app {\n      __typename\n      id\n      title\n      apiKey\n    }\n  }\n}\n";
const FUNCTION_HYDRATE_BY_HANDLE_QUERY: &str = "query FunctionHydrateByHandle($handle: String!) {\n  shopifyFunctions(first: 1, handle: $handle) {\n    nodes {\n      id\n      title\n      handle\n      apiType\n      description\n      appKey\n      app {\n        __typename\n        id\n        title\n        handle\n        apiKey\n      }\n    }\n  }\n}\n";
const FUNCTION_METADATA_CATALOG_HYDRATE_QUERY: &str = "query FunctionMetadataCatalogHydrate {\n  shopifyFunctions(first: 100) {\n    nodes {\n      id\n      title\n      handle\n      apiType\n      description\n      appKey\n      app {\n        __typename\n        id\n        title\n        handle\n        apiKey\n      }\n    }\n  }\n}\n";
const FUNCTION_VALIDATIONS_HYDRATE_QUERY: &str = "query FunctionValidationsHydrate {\n  validations(first: 100) {\n    nodes {\n      id\n      title\n      enabled\n      blockOnFailure\n      shopifyFunction {\n        id\n        title\n        handle\n        apiType\n        description\n        appKey\n        app {\n          __typename\n          id\n          title\n          handle\n          apiKey\n        }\n      }\n      metafields(first: 100) {\n        nodes {\n          id\n          namespace\n          key\n          type\n          value\n          updatedAt\n        }\n      }\n    }\n  }\n}\n";
const FUNCTION_VALIDATION_HYDRATE_BY_ID_QUERY: &str = "query FunctionValidationHydrateById($id: ID!) {\n  validation(id: $id) {\n    id\n    title\n    enabled\n    blockOnFailure\n    shopifyFunction {\n      id\n      title\n      handle\n      apiType\n      description\n      appKey\n      app {\n        __typename\n        id\n        title\n        handle\n        apiKey\n      }\n    }\n    metafields(first: 100) {\n      nodes {\n        id\n        namespace\n        key\n        type\n        value\n        updatedAt\n      }\n    }\n  }\n}\n";
const FUNCTION_CART_TRANSFORMS_HYDRATE_QUERY: &str = "query FunctionCartTransformsHydrate {\n  cartTransforms(first: 100) {\n    nodes {\n      id\n      functionId\n      blockOnFailure\n      metafields(first: 100) {\n        nodes {\n          id\n          namespace\n          key\n          type\n          value\n          compareDigest\n          ownerType\n          createdAt\n          updatedAt\n        }\n      }\n    }\n  }\n}\n";
const FUNCTION_CART_TRANSFORM_HYDRATE_BY_ID_QUERY: &str = "query FunctionCartTransformHydrateById($id: ID!) {\n  node(id: $id) {\n    ... on CartTransform {\n      id\n      functionId\n      blockOnFailure\n      metafields(first: 100) {\n        nodes {\n          id\n          namespace\n          key\n          type\n          value\n          compareDigest\n          ownerType\n          createdAt\n          updatedAt\n        }\n      }\n    }\n  }\n}\n";

impl DraftProxy {
    pub(in crate::proxy) fn functions_metadata_mutation_data(
        &mut self,
        request: &Request,
        fields: &[RootFieldSelection],
    ) -> (Value, Vec<Value>) {
        let mut errors = Vec::new();
        let data = root_payload_json(fields, |field| {
            let value = match field.name.as_str() {
                "validationCreate" => self.function_validation_create_payload(request, field),
                "validationUpdate" => self.function_validation_update_payload(request, field),
                "validationDelete" => self.function_validation_delete_payload(request, field),
                "cartTransformCreate" => {
                    self.function_cart_transform_create_payload(request, field)
                }
                "cartTransformDelete" => {
                    self.function_cart_transform_delete_payload(request, field)
                }
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
            if value.is_null() {
                Some(Value::Null)
            } else {
                Some(selected_json(&value, &field.selection))
            }
        });
        (data, errors)
    }

    pub(in crate::proxy) fn functions_metadata_read_data(
        &mut self,
        request: &Request,
        fields: &[RootFieldSelection],
    ) -> Value {
        root_payload_json(fields, |field| {
            let value = match field.name.as_str() {
                "validation" => resolved_string_field(&field.arguments, "id")
                    .and_then(|id| {
                        self.function_validation_read_value(request, &id, &field.selection)
                    })
                    .unwrap_or(Value::Null),
                "validations" => local_function_connection_from_nodes_with_args(
                    self.effective_function_validation_nodes(&field.selection),
                    &field.arguments,
                ),
                "cartTransforms" => local_function_connection_from_nodes_with_args(
                    self.effective_function_cart_transform_nodes(&field.selection),
                    &field.arguments,
                ),
                "fulfillmentConstraintRules" => self.fulfillment_constraint_rules_read_value(field),
                "shopifyFunctions" => {
                    let api_type = requested_function_api_type(&field.arguments);
                    local_function_connection_from_nodes_with_args(
                        self.function_metadata_read_nodes(request, api_type.as_deref()),
                        &field.arguments,
                    )
                }
                "shopifyFunction" => match resolved_string_field(&field.arguments, "id") {
                    Some(id) => self.function_metadata_read_value(request, &id),
                    None => Value::Null,
                },
                "node" => {
                    let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                    self.local_node_value_by_id(&id, &field.selection)
                        .unwrap_or(Value::Null)
                }
                "nodes" => Value::Array(
                    field
                        .arguments
                        .get("ids")
                        .map(resolved_string_list)
                        .unwrap_or_default()
                        .into_iter()
                        .map(|id| {
                            self.local_node_value_by_id(&id, &field.selection)
                                .unwrap_or(Value::Null)
                        })
                        .collect(),
                ),
                _ => Value::Null,
            };
            if value.is_null() {
                Some(Value::Null)
            } else if field.name == "fulfillmentConstraintRules" {
                Some(value)
            } else {
                Some(selected_json(&value, &field.selection))
            }
        })
    }

    fn fulfillment_constraint_rules_read_value(&self, field: &RootFieldSelection) -> Value {
        let records: Vec<Value> = self
            .store
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
                    .map(|record| {
                        fulfillment_constraint_rule_record_for_selection(record, &field.selection)
                    })
            })
            .collect();
        if fulfillment_constraint_rules_uses_connection_selection(&field.selection) {
            selected_json(
                &local_function_connection_from_nodes(records),
                &field.selection,
            )
        } else {
            Value::Array(
                records
                    .iter()
                    .map(|record| selected_json(record, &field.selection))
                    .collect(),
            )
        }
    }

    fn function_validation_read_value(
        &mut self,
        request: &Request,
        id: &str,
        selection: &[SelectedField],
    ) -> Option<Value> {
        if self
            .store
            .staged
            .deleted_function_validation_ids
            .contains(id)
        {
            return None;
        }
        if self.function_validation_by_id(id).is_none()
            && self.config.read_mode != ReadMode::Snapshot
        {
            self.hydrate_function_validation_by_id(request, id);
        }
        self.function_validation_by_id(id)
            .map(|record| validation_record_for_selection(record, selection))
    }

    fn function_metadata_read_value(&mut self, request: &Request, id: &str) -> Value {
        if self
            .function_metadata_by_id_or_handle(Some(id), None)
            .is_none()
            && self.config.read_mode != ReadMode::Snapshot
        {
            self.hydrate_function_metadata_by_id(request, id);
        }
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

    fn effective_function_validation_nodes(&self, selection: &[SelectedField]) -> Vec<Value> {
        self.effective_function_validation_records()
            .into_iter()
            .map(|record| validation_record_for_selection(record, selection))
            .collect()
    }

    fn effective_function_cart_transform_nodes(&self, selection: &[SelectedField]) -> Vec<Value> {
        self.effective_function_cart_transform_records()
            .into_iter()
            .map(|record| cart_transform_record_for_selection(record, selection))
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
        }
        self.store.base.function_metadata.insert(id, function);
    }

    fn stage_function_metadata_catalog(&mut self, data: &Value) {
        let mut seen = BTreeSet::new();
        let mut catalog_order = Vec::new();
        for function in data["shopifyFunctions"]["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|function| normalized_function_metadata(function.clone()))
        {
            let Some(id) = function["id"].as_str().map(str::to_string) else {
                continue;
            };
            if seen.insert(id.clone()) {
                catalog_order.push(id.clone());
            }
            self.store.base.function_metadata.insert(id, function);
        }
        for id in &self.store.base.function_metadata_order {
            if seen.insert(id.clone()) {
                catalog_order.push(id.clone());
            }
        }
        self.store.base.function_metadata_order = catalog_order;
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

    pub(in crate::proxy) fn hydrate_function_metadata_from_response_data(&mut self, data: &Value) {
        let mut functions = Vec::new();
        let mut validations = Vec::new();
        let mut cart_transforms = Vec::new();
        collect_function_connection_nodes(data, &mut functions);
        collect_function_metadata_values(data, &mut functions);
        for function in functions {
            self.stage_function_metadata(function);
        }
        collect_function_validation_values(data, &mut validations);
        for validation in validations {
            self.stage_base_function_validation(validation);
        }
        collect_function_cart_transform_values(data, &mut cart_transforms);
        for cart_transform in cart_transforms {
            self.stage_base_function_cart_transform(cart_transform);
        }
    }

    pub(in crate::proxy) fn mark_function_read_fields_hydrated(
        &mut self,
        fields: &[RootFieldSelection],
    ) {
        for field in fields {
            if field.name == "shopifyFunctions" {
                self.mark_function_metadata_catalog_hydrated(requested_function_api_type(
                    &field.arguments,
                ));
            }
        }
    }

    pub(in crate::proxy) fn hydrate_function_read_fields(
        &mut self,
        request: &Request,
        fields: &[RootFieldSelection],
    ) {
        for field in fields {
            match field.name.as_str() {
                "validation" => {
                    if let Some(id) = resolved_string_field(&field.arguments, "id") {
                        if self.function_validation_by_id(&id).is_none()
                            && !self
                                .store
                                .staged
                                .deleted_function_validation_ids
                                .contains(&id)
                        {
                            self.hydrate_function_validation_by_id(request, &id);
                        }
                    }
                }
                "validations" => {
                    if !self.store.base.function_validations_catalog_hydrated {
                        self.hydrate_function_validation_catalog(request);
                    }
                }
                "cartTransforms" => {
                    if !self.store.base.function_cart_transforms_catalog_hydrated {
                        self.hydrate_function_cart_transform_catalog(request);
                    }
                }
                "shopifyFunctions" => {
                    let api_type = requested_function_api_type(&field.arguments);
                    if !self.function_metadata_catalog_hydrated(api_type.as_deref()) {
                        self.hydrate_function_metadata_catalog(request);
                    }
                }
                "shopifyFunction" => {
                    if let Some(id) = resolved_string_field(&field.arguments, "id") {
                        if self
                            .function_metadata_by_id_or_handle(Some(&id), None)
                            .is_none()
                        {
                            self.hydrate_function_metadata_by_id(request, &id);
                        }
                    }
                }
                "node" => {
                    if let Some(id) = resolved_string_field(&field.arguments, "id") {
                        self.hydrate_function_node_id(request, &id);
                    }
                }
                "nodes" => {
                    for id in field
                        .arguments
                        .get("ids")
                        .map(resolved_string_list)
                        .unwrap_or_default()
                    {
                        self.hydrate_function_node_id(request, &id);
                    }
                }
                _ => {}
            }
        }
    }

    pub(in crate::proxy) fn function_read_has_local_overlay(
        &self,
        fields: &[RootFieldSelection],
    ) -> bool {
        fields.iter().any(|field| match field.name.as_str() {
            "validation" | "validations" => self.has_function_validation_overlay_state(),
            "cartTransforms" => self.has_function_cart_transform_overlay_state(),
            "fulfillmentConstraintRules" => {
                self.has_function_fulfillment_constraint_rule_overlay_state()
            }
            "shopifyFunction" | "shopifyFunctions" => self.has_function_metadata_overlay_state(),
            "node" => resolved_string_field(&field.arguments, "id")
                .is_some_and(|id| self.function_node_has_local_overlay(&id)),
            "nodes" => field
                .arguments
                .get("ids")
                .map(resolved_string_list)
                .unwrap_or_default()
                .iter()
                .any(|id| self.function_node_has_local_overlay(id)),
            _ => false,
        })
    }

    fn hydrate_function_metadata_catalog(&mut self, request: &Request) {
        let response = self.upstream_post(
            request,
            json!({
                "query": FUNCTION_METADATA_CATALOG_HYDRATE_QUERY,
                "operationName": "FunctionMetadataCatalogHydrate",
                "variables": {}
            }),
        );
        if response.status == 200 {
            self.stage_function_metadata_catalog(&response.body["data"]);
            self.mark_function_metadata_catalog_hydrated(None);
        }
    }

    fn hydrate_function_validation_catalog(&mut self, request: &Request) {
        let response = self.upstream_post(
            request,
            json!({
                "query": FUNCTION_VALIDATIONS_HYDRATE_QUERY,
                "operationName": "FunctionValidationsHydrate",
                "variables": {}
            }),
        );
        if response.status == 200 {
            self.hydrate_function_metadata_from_response_data(&response.body["data"]);
            self.store.base.function_validations_catalog_hydrated = true;
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

    fn hydrate_function_cart_transform_catalog(&mut self, request: &Request) {
        let response = self.upstream_post(
            request,
            json!({
                "query": FUNCTION_CART_TRANSFORMS_HYDRATE_QUERY,
                "operationName": "FunctionCartTransformsHydrate",
                "variables": {}
            }),
        );
        if response.status == 200 {
            self.hydrate_function_metadata_from_response_data(&response.body["data"]);
            self.store.base.function_cart_transforms_catalog_hydrated = true;
        }
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

    fn hydrate_function_node_id(&mut self, request: &Request, id: &str) {
        match shopify_gid_resource_type(id) {
            Some("ShopifyFunction") => {
                if self
                    .function_metadata_by_id_or_handle(Some(id), None)
                    .is_none()
                {
                    self.hydrate_function_metadata_by_id(request, id);
                }
            }
            Some("Validation")
                if self.function_validation_by_id(id).is_none()
                    && !self
                        .store
                        .staged
                        .deleted_function_validation_ids
                        .contains(id) =>
            {
                self.hydrate_function_validation_by_id(request, id);
            }
            Some("CartTransform")
                if self.function_cart_transform_by_id(id).is_none()
                    && !self
                        .store
                        .staged
                        .deleted_function_cart_transform_ids
                        .contains(id) =>
            {
                self.hydrate_function_cart_transform_by_id(request, id);
            }
            _ => {}
        }
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
        if validation.get("metafields").is_none() {
            validation["metafields"] = json!({ "nodes": [] });
        }
        if !self.store.base.function_validations.contains_key(&id) {
            self.store.base.function_validation_order.push(id.clone());
        }
        self.store.base.function_validations.insert(id, validation);
    }

    fn stage_base_function_cart_transform(&mut self, mut cart_transform: Value) {
        let Some(id) = cart_transform["id"].as_str().map(str::to_string) else {
            return;
        };
        if cart_transform.get("metafields").is_none() {
            cart_transform["metafields"] = json!({ "nodes": [] });
        }
        if cart_transform.get("metafield").is_none_or(Value::is_null) {
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
        }
        self.store
            .base
            .function_cart_transforms
            .insert(id, cart_transform);
    }

    fn has_function_validation_overlay_state(&self) -> bool {
        self.store.staged.function_validations_dirty
            || !self.store.base.function_validations.is_empty()
            || !self.store.staged.function_validations.is_empty()
            || !self.store.staged.deleted_function_validation_ids.is_empty()
    }

    fn has_function_cart_transform_overlay_state(&self) -> bool {
        self.store.staged.function_cart_transforms_dirty
            || !self.store.base.function_cart_transforms.is_empty()
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
                .base
                .function_fulfillment_constraint_rules
                .is_empty()
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

    fn has_function_metadata_overlay_state(&self) -> bool {
        !self.store.base.function_metadata.is_empty()
            || !self.store.staged.function_metadata.is_empty()
            || self.has_function_validation_overlay_state()
            || self.has_function_cart_transform_overlay_state()
            || self.has_function_fulfillment_constraint_rule_overlay_state()
    }

    fn function_metadata_catalog_hydrated(&self, api_type: Option<&str>) -> bool {
        self.store.base.function_metadata_catalog_hydrated
            || api_type.is_some_and(|api_type| {
                self.store
                    .base
                    .function_metadata_hydrated_api_types
                    .contains(api_type)
            })
    }

    fn mark_function_metadata_catalog_hydrated(&mut self, api_type: Option<String>) {
        if let Some(api_type) = api_type {
            self.store
                .base
                .function_metadata_hydrated_api_types
                .insert(api_type);
        } else {
            self.store.base.function_metadata_catalog_hydrated = true;
        }
    }

    fn function_node_has_local_overlay(&self, id: &str) -> bool {
        match shopify_gid_resource_type(id) {
            Some("ShopifyFunction") => self
                .function_metadata_by_id_or_handle(Some(id), None)
                .is_some(),
            Some("Validation") => {
                self.store
                    .staged
                    .deleted_function_validation_ids
                    .contains(id)
                    || self.function_validation_by_id(id).is_some()
            }
            Some("CartTransform") => {
                self.store
                    .staged
                    .deleted_function_cart_transform_ids
                    .contains(id)
                    || self.function_cart_transform_by_id(id).is_some()
            }
            Some("FulfillmentConstraintRule") => {
                self.store
                    .staged
                    .deleted_function_fulfillment_constraint_rule_ids
                    .contains(id)
                    || self
                        .store
                        .staged
                        .function_fulfillment_constraint_rules
                        .contains_key(id)
                    || self
                        .store
                        .base
                        .function_fulfillment_constraint_rules
                        .contains_key(id)
            }
            _ => false,
        }
    }
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

fn tax_app_configure_access_denied_error(field: &RootFieldSelection) -> Value {
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

fn collect_function_metadata_values(value: &Value, functions: &mut Vec<Value>) {
    if let Some(function) = normalized_function_metadata(value.clone()) {
        functions.push(function);
        return;
    }
    match value {
        Value::Array(values) => {
            for value in values {
                collect_function_metadata_values(value, functions);
            }
        }
        Value::Object(object) => {
            for value in object.values() {
                collect_function_metadata_values(value, functions);
            }
        }
        _ => {}
    }
}

fn collect_function_connection_nodes(value: &Value, functions: &mut Vec<Value>) {
    match value {
        Value::Array(values) => {
            for value in values {
                collect_function_connection_nodes(value, functions);
            }
        }
        Value::Object(object) => {
            if let Some(nodes) = object.get("nodes").and_then(Value::as_array) {
                for node in nodes {
                    collect_function_metadata_values(node, functions);
                }
            }
            for value in object.values() {
                collect_function_connection_nodes(value, functions);
            }
        }
        _ => {}
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
    if validation.get("metafields").is_none() {
        validation["metafields"] = json!({ "nodes": [] });
    }
    Some(validation)
}

fn looks_like_function_validation(value: &Value) -> bool {
    value.get("enabled").is_some()
        || value.get("enable").is_some()
        || (value.get("shopifyFunction").is_some() && value.get("functionId").is_none())
}

fn collect_function_validation_values(value: &Value, validations: &mut Vec<Value>) {
    if let Some(validation) = normalized_function_validation(value.clone()) {
        validations.push(validation);
        return;
    }
    match value {
        Value::Array(values) => {
            for value in values {
                collect_function_validation_values(value, validations);
            }
        }
        Value::Object(object) => {
            for value in object.values() {
                collect_function_validation_values(value, validations);
            }
        }
        _ => {}
    }
}

fn normalized_function_cart_transform(mut cart_transform: Value) -> Option<Value> {
    cart_transform.get("id").and_then(Value::as_str)?;
    if !looks_like_function_cart_transform(&cart_transform) {
        return None;
    }
    if cart_transform.get("metafields").is_none() {
        cart_transform["metafields"] = json!({ "nodes": [] });
    }
    if cart_transform.get("metafield").is_none_or(Value::is_null) {
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

fn collect_function_cart_transform_values(value: &Value, cart_transforms: &mut Vec<Value>) {
    if let Some(cart_transform) = normalized_function_cart_transform(value.clone()) {
        cart_transforms.push(cart_transform);
        return;
    }
    match value {
        Value::Array(values) => {
            for value in values {
                collect_function_cart_transform_values(value, cart_transforms);
            }
        }
        Value::Object(object) => {
            for value in object.values() {
                collect_function_cart_transform_values(value, cart_transforms);
            }
        }
        _ => {}
    }
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

pub(in crate::proxy) fn local_function_connection_from_nodes(nodes: Vec<Value>) -> Value {
    local_function_connection_from_nodes_with_args(nodes, &BTreeMap::new())
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

fn cart_transform_metafield_errors(field: &RootFieldSelection) -> Vec<Value> {
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
    field: &RootFieldSelection,
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

pub(in crate::proxy) fn cart_transform_record_for_selection(
    record: &Value,
    selection: &[SelectedField],
) -> Value {
    let mut public =
        function_record_with_output_fields(record, "CartTransform", CART_TRANSFORM_OUTPUT_FIELDS);
    if let Some(metafield_selection) =
        selected_output_type_field(selection, "CartTransform", "metafield", true)
    {
        apply_metafield_for_selection(&mut public, metafield_selection);
    }
    public
}

fn fulfillment_constraint_rule_delivery_method_types(field: &RootFieldSelection) -> Vec<String> {
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

pub(in crate::proxy) fn validation_record_for_selection(
    record: &Value,
    selection: &[SelectedField],
) -> Value {
    let mut public =
        function_record_with_output_fields(record, "Validation", VALIDATION_OUTPUT_FIELDS);
    if let Some(metafield_selection) =
        selected_output_type_field(selection, "Validation", "metafield", true)
    {
        apply_metafield_for_selection(&mut public, metafield_selection);
    }
    public
}

const FULFILLMENT_CONSTRAINT_RULE_OUTPUT_FIELDS: &[&str] = &[
    "id",
    "function",
    "deliveryMethodTypes",
    "metafield",
    "metafields",
    "__typename",
];

pub(in crate::proxy) fn fulfillment_constraint_rule_record_for_selection(
    record: &Value,
    selection: &[SelectedField],
) -> Value {
    let mut public = function_record_with_output_fields(
        record,
        "FulfillmentConstraintRule",
        FULFILLMENT_CONSTRAINT_RULE_OUTPUT_FIELDS,
    );
    if let Some(metafield_selection) =
        selected_output_type_field(selection, "FulfillmentConstraintRule", "metafield", true)
    {
        apply_metafield_for_selection(&mut public, metafield_selection);
    }
    public
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

fn fulfillment_constraint_rules_uses_connection_selection(selection: &[SelectedField]) -> bool {
    selection
        .iter()
        .any(|field| matches!(field.name.as_str(), "nodes" | "edges" | "pageInfo"))
}

fn selected_output_type_field<'a>(
    selections: &'a [SelectedField],
    type_name: &str,
    field_name: &str,
    include_direct: bool,
) -> Option<&'a SelectedField> {
    for selection in selections {
        if include_direct
            && selection.name == field_name
            && selection_applies_to_output_type(selection, type_name)
        {
            return Some(selection);
        }
        match selection.name.as_str() {
            "nodes" => {
                if let Some(field) =
                    selected_output_type_field(&selection.selection, type_name, field_name, true)
                {
                    return Some(field);
                }
            }
            "edges" => {
                for node in selection
                    .selection
                    .iter()
                    .filter(|child| child.name == "node")
                {
                    if let Some(field) =
                        selected_output_type_field(&node.selection, type_name, field_name, true)
                    {
                        return Some(field);
                    }
                }
            }
            _ => {}
        }
    }
    None
}

fn selection_applies_to_output_type(selection: &SelectedField, type_name: &str) -> bool {
    selected_field_applies_to_type(type_name, selection)
}

pub(in crate::proxy) fn functions_output_selection_errors(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
    fields: &[RootFieldSelection],
) -> Vec<Value> {
    let operation_path = parsed_document(query, variables)
        .map(|document| document.operation_path)
        .unwrap_or_default();
    let mut errors = Vec::new();
    for field in fields {
        match field.name.as_str() {
            "validation" => push_output_selection_errors(
                &mut errors,
                OutputSelectionErrorContext::new(
                    &operation_path,
                    &field.response_key,
                    "Validation",
                    VALIDATION_OUTPUT_FIELDS,
                    true,
                    false,
                ),
                &field.selection,
            ),
            "validations" => push_output_selection_errors(
                &mut errors,
                OutputSelectionErrorContext::new(
                    &operation_path,
                    &field.response_key,
                    "Validation",
                    VALIDATION_OUTPUT_FIELDS,
                    false,
                    false,
                ),
                &field.selection,
            ),
            "cartTransforms" => push_output_selection_errors(
                &mut errors,
                OutputSelectionErrorContext::new(
                    &operation_path,
                    &field.response_key,
                    "CartTransform",
                    CART_TRANSFORM_OUTPUT_FIELDS,
                    false,
                    false,
                ),
                &field.selection,
            ),
            "fulfillmentConstraintRules" => push_output_selection_errors(
                &mut errors,
                OutputSelectionErrorContext::new(
                    &operation_path,
                    &field.response_key,
                    "FulfillmentConstraintRule",
                    FULFILLMENT_CONSTRAINT_RULE_OUTPUT_FIELDS,
                    true,
                    false,
                ),
                &field.selection,
            ),
            "node" | "nodes" => {
                push_output_selection_errors(
                    &mut errors,
                    OutputSelectionErrorContext::new(
                        &operation_path,
                        &field.response_key,
                        "Validation",
                        VALIDATION_OUTPUT_FIELDS,
                        true,
                        true,
                    ),
                    &field.selection,
                );
                push_output_selection_errors(
                    &mut errors,
                    OutputSelectionErrorContext::new(
                        &operation_path,
                        &field.response_key,
                        "CartTransform",
                        CART_TRANSFORM_OUTPUT_FIELDS,
                        true,
                        true,
                    ),
                    &field.selection,
                );
                push_output_selection_errors(
                    &mut errors,
                    OutputSelectionErrorContext::new(
                        &operation_path,
                        &field.response_key,
                        "FulfillmentConstraintRule",
                        FULFILLMENT_CONSTRAINT_RULE_OUTPUT_FIELDS,
                        true,
                        true,
                    ),
                    &field.selection,
                );
            }
            _ => {}
        }
    }
    errors
}

#[derive(Clone, Copy)]
struct OutputSelectionErrorContext<'a> {
    operation_path: &'a str,
    response_key: &'a str,
    type_name: &'static str,
    output_fields: &'static [&'static str],
    include_direct: bool,
    require_type_condition: bool,
}

impl<'a> OutputSelectionErrorContext<'a> {
    fn new(
        operation_path: &'a str,
        response_key: &'a str,
        type_name: &'static str,
        output_fields: &'static [&'static str],
        include_direct: bool,
        require_type_condition: bool,
    ) -> Self {
        Self {
            operation_path,
            response_key,
            type_name,
            output_fields,
            include_direct,
            require_type_condition,
        }
    }

    fn nested(self) -> Self {
        Self {
            include_direct: true,
            ..self
        }
    }
}

fn push_output_selection_errors(
    errors: &mut Vec<Value>,
    context: OutputSelectionErrorContext<'_>,
    selections: &[SelectedField],
) {
    collect_output_selection_errors(errors, context, selections, &[]);
}

fn collect_output_selection_errors(
    errors: &mut Vec<Value>,
    context: OutputSelectionErrorContext<'_>,
    selections: &[SelectedField],
    container_path: &[&str],
) {
    for selection in selections {
        match selection.name.as_str() {
            "nodes" => {
                let mut next_path = container_path.to_vec();
                next_path.push("nodes");
                collect_output_selection_errors(
                    errors,
                    context.nested(),
                    &selection.selection,
                    &next_path,
                );
            }
            "edges" => {
                for node in selection
                    .selection
                    .iter()
                    .filter(|child| child.name == "node")
                {
                    let mut next_path = container_path.to_vec();
                    next_path.push("edges");
                    next_path.push("node");
                    collect_output_selection_errors(
                        errors,
                        context.nested(),
                        &node.selection,
                        &next_path,
                    );
                }
            }
            _ if context.include_direct
                && selection_matches_validation_scope(
                    selection,
                    context.type_name,
                    context.require_type_condition,
                )
                && !context.output_fields.contains(&selection.name.as_str()) =>
            {
                errors.push(function_output_undefined_field_error(
                    context.operation_path,
                    context.response_key,
                    container_path,
                    context.type_name,
                    selection,
                ));
            }
            _ => {}
        }
    }
}

fn selection_matches_validation_scope(
    selection: &SelectedField,
    type_name: &str,
    require_type_condition: bool,
) -> bool {
    if require_type_condition {
        selection.type_condition.as_deref() == Some(type_name)
    } else {
        selection_applies_to_output_type(selection, type_name)
    }
}

fn function_output_undefined_field_error(
    operation_path: &str,
    response_key: &str,
    container_path: &[&str],
    type_name: &str,
    selection: &SelectedField,
) -> Value {
    let mut path = vec![Value::from(operation_path), Value::from(response_key)];
    path.extend(container_path.iter().map(|segment| Value::from(*segment)));
    if let Some(type_condition) = selection.type_condition.as_deref() {
        path.push(Value::from(format!("... on {type_condition}")));
    }
    path.push(Value::from(selection.name.clone()));
    json!({
        "message": format!("Field '{}' doesn't exist on type '{type_name}'", selection.name),
        "locations": [{ "line": selection.location.line, "column": selection.location.column }],
        "path": path,
        "extensions": {
            "code": "undefinedField",
            "typeName": type_name,
            "fieldName": selection.name
        }
    })
}

fn apply_metafield_for_selection(record: &mut Value, metafield_selection: &SelectedField) {
    let namespace = metafield_selection
        .arguments
        .get("namespace")
        .and_then(|value| match value {
            ResolvedValue::String(value) => Some(value.as_str()),
            _ => None,
        });
    let key = metafield_selection
        .arguments
        .get("key")
        .and_then(|value| match value {
            ResolvedValue::String(value) => Some(value.as_str()),
            _ => None,
        });
    if let (Some(namespace), Some(key)) = (namespace, key) {
        let metafield = record["metafields"]["nodes"]
            .as_array()
            .and_then(|nodes| {
                nodes.iter().find(|node| {
                    node["namespace"].as_str() == Some(namespace)
                        && node["key"].as_str() == Some(key)
                })
            })
            .cloned()
            .unwrap_or(Value::Null);
        record["metafield"] = metafield;
    }
}

impl DraftProxy {
    pub(in crate::proxy) fn function_validation_create_payload(
        &mut self,
        request: &Request,
        field: &RootFieldSelection,
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

    pub(in crate::proxy) fn function_validation_update_payload(
        &mut self,
        request: &Request,
        field: &RootFieldSelection,
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
        upsert_validation_metafields(
            &mut validation,
            validation_metafields_from_input(input, &timestamp),
        );
        self.stage_function_validation(validation.clone());
        json!({ "validation": validation, "userErrors": [] })
    }

    pub(in crate::proxy) fn function_validation_delete_payload(
        &mut self,
        request: &Request,
        field: &RootFieldSelection,
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

    pub(in crate::proxy) fn function_cart_transform_create_payload(
        &mut self,
        request: &Request,
        field: &RootFieldSelection,
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

    pub(in crate::proxy) fn function_cart_transform_delete_payload(
        &mut self,
        request: &Request,
        field: &RootFieldSelection,
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

    pub(in crate::proxy) fn function_fulfillment_constraint_rule_create_payload(
        &mut self,
        request: &Request,
        field: &RootFieldSelection,
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

    pub(in crate::proxy) fn function_fulfillment_constraint_rule_update_payload(
        &mut self,
        request: &Request,
        field: &RootFieldSelection,
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

    pub(in crate::proxy) fn function_fulfillment_constraint_rule_delete_payload(
        &mut self,
        field: &RootFieldSelection,
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

    pub(in crate::proxy) fn function_tax_app_configure_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
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

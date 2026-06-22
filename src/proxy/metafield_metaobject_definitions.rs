use super::*;

const METAFIELD_DEFINITION_RESOURCE_TYPE_LIMIT: usize = 256;
const PINNED_DEFINITION_LIMIT: usize = 20;
const ADMIN_FILTERABLE_DEFINITION_LIMIT: usize = 50;
const STANDARD_TEMPLATE_MARKER_FIELD: &str = "__shopifyDraftProxyStandardTemplateId";

fn pinned_definition_limit_message() -> String {
    format!("Limit of {PINNED_DEFINITION_LIMIT} pinned definitions.")
}

fn metafield_definition_resource_type_limit_message() -> String {
    format!(
        "You can only have {METAFIELD_DEFINITION_RESOURCE_TYPE_LIMIT} definitions per resource type."
    )
}

fn admin_filterable_definition_limit_message(owner_type: &str) -> String {
    format!(
        "You can only use {ADMIN_FILTERABLE_DEFINITION_LIMIT} {} metafield definitions to filter the {} list. To add a new filter, disable filtering on an existing one.",
        owner_type.to_ascii_lowercase(),
        owner_type.to_ascii_lowercase()
    )
}

fn metafield_definition_value(
    namespace: &str,
    key: &str,
    name: &str,
    id: &str,
    pinned_position: Value,
) -> Value {
    json!({
        "id": id,
        "name": name,
        "namespace": namespace,
        "key": key,
        "ownerType": "PRODUCT",
        "type": {"name": "single_line_text_field", "category": "TEXT"},
        "description": Value::Null,
        "validations": [],
        "access": {"admin": "PUBLIC_READ_WRITE", "storefront": "NONE", "customerAccount": "NONE"},
        "capabilities": {
            "adminFilterable": {"enabled": false, "eligible": true, "status": "NOT_FILTERABLE"},
            "smartCollectionCondition": {"enabled": false, "eligible": true},
            "uniqueValues": {"enabled": false, "eligible": true}
        },
        "constraints": {"key": Value::Null, "values": {"nodes": [], "pageInfo": empty_page_info()}},
        "pinnedPosition": pinned_position,
        "validationStatus": "ALL_VALID"
    })
}

impl DraftProxy {
    pub(in crate::proxy) fn metafield_definition_pinning_mutation(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        if query.contains("access: { grants:") {
            return ok_json(json!({
                "errors": [{
                    "message": "InputObject 'MetafieldAccessInput' doesn't accept argument 'grants'",
                    "locations": [{"line": 9, "column": 17}],
                    "path": ["mutation MetafieldDefinitionAccessValidationInlineGrants", "metafieldDefinitionCreate", "definition", "access", "grants"],
                    "extensions": {
                        "code": "argumentNotAccepted",
                        "name": "MetafieldAccessInput",
                        "typeName": "InputObject",
                        "argumentName": "grants"
                    }
                }]
            }));
        }
        let mut data = serde_json::Map::new();
        let mut staged_ids = Vec::new();
        let mut primary_staged_root: Option<String> = None;
        for field in root_fields(query, variables).unwrap_or_default() {
            let root_name = field.name.clone();
            match field.name.as_str() {
                "metafieldDefinitionCreate" => {
                    let definition_input =
                        resolved_object_field(&field.arguments, "definition").unwrap_or_default();
                    if access_denied_for_reserved_metafield_namespace(&definition_input) {
                        return metafield_definition_access_denied_response(
                            "metafieldDefinitionCreate",
                        );
                    }
                    let payload =
                        self.metafield_definition_create_payload(request, &definition_input);
                    if let Some(id) = metafield_definition_payload_staged_id(&payload) {
                        staged_ids.push(id);
                        primary_staged_root.get_or_insert(root_name);
                    }
                    data.insert(
                        field.response_key,
                        selected_json(&payload, &field.selection),
                    );
                }
                "metafieldDefinitionUpdate" => {
                    let definition_input =
                        resolved_object_field(&field.arguments, "definition").unwrap_or_default();
                    if access_denied_for_reserved_metafield_namespace(&definition_input) {
                        return metafield_definition_access_denied_response(
                            "metafieldDefinitionUpdate",
                        );
                    }
                    let payload = self.metafield_definition_update_payload(&definition_input);
                    if let Some(id) = metafield_definition_payload_staged_id(&payload) {
                        staged_ids.push(id);
                        primary_staged_root.get_or_insert(root_name);
                    }
                    data.insert(
                        field.response_key,
                        selected_json(&payload, &field.selection),
                    );
                }
                "standardMetafieldDefinitionEnable" => {
                    let owner_type = resolved_string_field(&field.arguments, "ownerType")
                        .unwrap_or_else(|| "PRODUCT".to_string());
                    let id = resolved_string_field(&field.arguments, "id");
                    let namespace = resolved_string_field(&field.arguments, "namespace");
                    let key = resolved_string_field(&field.arguments, "key");
                    let mut staged_ids = Vec::new();
                    let payload = self.standard_metafield_definition_enable_payload(
                        request,
                        &field.arguments,
                        StandardMetafieldDefinitionSelector {
                            id: id.as_deref(),
                            namespace: namespace.as_deref(),
                            key: key.as_deref(),
                        },
                        &owner_type,
                        &mut staged_ids,
                    );
                    if !staged_ids.is_empty() {
                        primary_staged_root.get_or_insert(root_name);
                    }
                    data.insert(
                        field.response_key,
                        selected_json(&payload, &field.selection),
                    );
                }
                "metafieldDefinitionDelete" => {
                    let payload = self.metafield_definition_delete_payload(&field.arguments);
                    if let Some(id) = metafield_definition_payload_staged_id(&payload) {
                        staged_ids.push(id);
                        primary_staged_root.get_or_insert(root_name);
                    }
                    data.insert(
                        field.response_key,
                        selected_json(&payload, &field.selection),
                    );
                }
                "metafieldDefinitionPin" => {
                    let identifier =
                        resolved_object_field(&field.arguments, "identifier").unwrap_or_default();
                    let owner_type = resolved_string_field(&identifier, "ownerType")
                        .unwrap_or_else(|| "PRODUCT".to_string());
                    let mut namespace =
                        resolved_string_field(&identifier, "namespace").unwrap_or_default();
                    let mut key = resolved_string_field(&identifier, "key").unwrap_or_default();
                    if key.is_empty() {
                        if let Some(definition_id) =
                            resolved_string_field(&field.arguments, "definitionId")
                                .or_else(|| resolved_string_arg(variables, "definitionId"))
                        {
                            if let Some((found_namespace, found_key)) =
                                self.metafield_definition_key_for_id(&definition_id)
                            {
                                namespace = found_namespace;
                                key = found_key;
                            } else {
                                self.hydrate_metafield_definition_by_id(request, &definition_id);
                                if let Some((found_namespace, found_key)) =
                                    self.metafield_definition_key_for_id(&definition_id)
                                {
                                    namespace = found_namespace;
                                    key = found_key;
                                }
                            }
                        }
                    }
                    self.hydrate_metafield_definitions_for_owner(request, &owner_type, &namespace);
                    let map_key = (namespace.clone(), key.clone());
                    let Some(mut definition) = self
                        .store
                        .staged
                        .metafield_definitions
                        .get(&map_key)
                        .cloned()
                    else {
                        let payload = json!({
                            "pinnedDefinition": Value::Null,
                            "userErrors": [metafield_definition_user_error("MetafieldDefinitionPinUserError", Value::Null, "Definition not found.", "NOT_FOUND")]
                        });
                        data.insert(
                            field.response_key,
                            selected_json(&payload, &field.selection),
                        );
                        continue;
                    };
                    if metafield_definition_has_constraints(&definition) {
                        let payload = json!({
                            "pinnedDefinition": Value::Null,
                            "userErrors": [metafield_definition_user_error("MetafieldDefinitionPinUserError", Value::Null, "Constrained metafield definitions do not support pinning.", "UNSUPPORTED_PINNING")]
                        });
                        data.insert(
                            field.response_key,
                            selected_json(&payload, &field.selection),
                        );
                        continue;
                    }
                    if definition
                        .get("pinnedPosition")
                        .is_some_and(|position| !position.is_null())
                    {
                        let payload = json!({
                            "pinnedDefinition": Value::Null,
                            "userErrors": [metafield_definition_user_error("MetafieldDefinitionPinUserError", Value::Null, "Definition already pinned.", "ALREADY_PINNED")]
                        });
                        data.insert(
                            field.response_key,
                            selected_json(&payload, &field.selection),
                        );
                        continue;
                    }
                    let definition_owner_type = definition
                        .get("ownerType")
                        .and_then(Value::as_str)
                        .unwrap_or("PRODUCT")
                        .to_string();
                    if self.metafield_definition_pin_count(&definition_owner_type)
                        >= PINNED_DEFINITION_LIMIT
                    {
                        let payload = json!({
                            "pinnedDefinition": Value::Null,
                            "userErrors": [metafield_definition_user_error("MetafieldDefinitionPinUserError", Value::Null, &pinned_definition_limit_message(), "PINNED_LIMIT_REACHED")]
                        });
                        data.insert(
                            field.response_key,
                            selected_json(&payload, &field.selection),
                        );
                        continue;
                    }
                    let position = self.next_metafield_definition_pin_position(
                        &definition_owner_type,
                        &namespace,
                        &key,
                    );
                    if definition.get("pinnedPosition").is_none_or(Value::is_null) {
                        definition["pinnedPosition"] = json!(position);
                    }
                    self.store
                        .staged
                        .metafield_definitions
                        .insert(map_key, definition.clone());
                    let payload = json!({
                        "pinnedDefinition": public_metafield_definition_value(definition),
                        "userErrors": []
                    });
                    if let Some(id) = metafield_definition_payload_staged_id(&payload) {
                        staged_ids.push(id);
                        primary_staged_root.get_or_insert(root_name);
                    }
                    data.insert(
                        field.response_key,
                        selected_json(&payload, &field.selection),
                    );
                }
                "metafieldDefinitionUnpin" => {
                    let identifier =
                        resolved_object_field(&field.arguments, "identifier").unwrap_or_default();
                    let owner_type = resolved_string_field(&identifier, "ownerType")
                        .unwrap_or_else(|| "PRODUCT".to_string());
                    let mut namespace =
                        resolved_string_field(&identifier, "namespace").unwrap_or_default();
                    let mut key = resolved_string_field(&identifier, "key").unwrap_or_default();
                    if key.is_empty() {
                        if let Some(definition_id) = resolved_string_arg(variables, "definitionId")
                            .or_else(|| resolved_string_arg(variables, "id"))
                        {
                            if let Some((found_namespace, found_key)) =
                                self.metafield_definition_key_for_id(&definition_id)
                            {
                                namespace = found_namespace;
                                key = found_key;
                            } else {
                                self.hydrate_metafield_definition_by_id(request, &definition_id);
                                if let Some((found_namespace, found_key)) =
                                    self.metafield_definition_key_for_id(&definition_id)
                                {
                                    namespace = found_namespace;
                                    key = found_key;
                                }
                            }
                        }
                    }
                    self.hydrate_metafield_definitions_for_owner(request, &owner_type, &namespace);
                    let map_key = (namespace.clone(), key.clone());
                    let Some(current) = self
                        .store
                        .staged
                        .metafield_definitions
                        .get(&map_key)
                        .cloned()
                    else {
                        let payload = json!({
                            "unpinnedDefinition": Value::Null,
                            "userErrors": [metafield_definition_user_error("MetafieldDefinitionUnpinUserError", Value::Null, "Definition not found.", "NOT_FOUND")]
                        });
                        data.insert(
                            field.response_key,
                            selected_json(&payload, &field.selection),
                        );
                        continue;
                    };
                    if current.get("pinnedPosition").is_none_or(Value::is_null) {
                        let numeric_id = current
                            .get("id")
                            .and_then(Value::as_str)
                            .map(resource_id_tail)
                            .unwrap_or_default();
                        let payload = json!({
                            "unpinnedDefinition": Value::Null,
                            "userErrors": [metafield_definition_user_error("MetafieldDefinitionUnpinUserError", Value::Null, &format!("Definition {numeric_id} isn't pinned."), "NOT_PINNED")]
                        });
                        data.insert(
                            field.response_key,
                            selected_json(&payload, &field.selection),
                        );
                        continue;
                    }
                    let mut definition = current;
                    let owner_type = definition["ownerType"]
                        .as_str()
                        .unwrap_or("PRODUCT")
                        .to_string();
                    definition["pinnedPosition"] = Value::Null;
                    self.store
                        .staged
                        .metafield_definitions
                        .insert(map_key, definition.clone());
                    self.compact_metafield_definition_pins(&owner_type, &namespace);
                    let payload = json!({
                        "unpinnedDefinition": public_metafield_definition_value(definition),
                        "userErrors": []
                    });
                    if let Some(id) = metafield_definition_payload_staged_id(&payload) {
                        staged_ids.push(id);
                        primary_staged_root.get_or_insert(root_name);
                    }
                    data.insert(
                        field.response_key,
                        selected_json(&payload, &field.selection),
                    );
                }
                _ => {}
            }
        }
        if let Some(root) = primary_staged_root {
            self.record_mutation_log_entry(request, query, variables, &root, staged_ids);
        }
        ok_json(json!({"data": Value::Object(data)}))
    }

    fn metafield_definition_create_payload(
        &mut self,
        request: &Request,
        input: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let namespace = self.metafield_definition_namespace_from_input(request, input, None);
        let key = resolved_string_field(input, "key").unwrap_or_default();
        let errors = metafield_definition_create_errors_for_namespace(input, &namespace);
        if !errors.is_empty() {
            return json!({"createdDefinition": Value::Null, "userErrors": errors});
        }
        let validation_errors = metafield_definition_validation_errors(
            input,
            "MetafieldDefinitionCreateUserError",
            false,
            None,
        );
        if !validation_errors.is_empty() {
            return json!({"createdDefinition": Value::Null, "userErrors": validation_errors});
        }
        let owner_type =
            resolved_string_field(input, "ownerType").unwrap_or_else(|| "PRODUCT".to_string());
        if self.metafield_definition_resource_type_count(&owner_type, &namespace)
            >= METAFIELD_DEFINITION_RESOURCE_TYPE_LIMIT
        {
            return json!({
                "createdDefinition": Value::Null,
                "userErrors": [metafield_definition_user_error(
                    "MetafieldDefinitionCreateUserError",
                    json!(["definition"]),
                    &metafield_definition_resource_type_limit_message(),
                    "RESOURCE_TYPE_LIMIT_EXCEEDED"
                )]
            });
        }
        if let Some(error) = metafield_definition_capability_input_error(
            input,
            "MetafieldDefinitionCreateUserError",
            json!(["definition"]),
            &owner_type,
            resolved_string_field(input, "type")
                .as_deref()
                .unwrap_or_default(),
        ) {
            return json!({"createdDefinition": Value::Null, "userErrors": [error]});
        }
        if metafield_definition_capabilities_will_enable_admin_filterable(input, None)
            && self.metafield_definition_admin_filterable_count(&owner_type)
                >= ADMIN_FILTERABLE_DEFINITION_LIMIT
        {
            return json!({
                "createdDefinition": Value::Null,
                "userErrors": [metafield_definition_user_error(
                    "MetafieldDefinitionCreateUserError",
                    json!(["definition"]),
                    &admin_filterable_definition_limit_message(&owner_type),
                    "OWNER_TYPE_LIMIT_EXCEEDED_FOR_USE_AS_ADMIN_FILTERS"
                )]
            });
        }
        let mut definition = self.metafield_definition_from_input(request, input, None);
        if resolved_bool_field(input, "pin") == Some(true) {
            if metafield_definition_has_constraints(&definition) {
                return json!({
                    "createdDefinition": Value::Null,
                    "userErrors": [metafield_definition_user_error(
                        "MetafieldDefinitionCreateUserError",
                        json!(["definition"]),
                        "Constrained metafield definitions do not support pinning.",
                        "UNSUPPORTED_PINNING"
                    )]
                });
            }
            if self.metafield_definition_pin_count(&owner_type) >= PINNED_DEFINITION_LIMIT {
                return json!({
                    "createdDefinition": Value::Null,
                    "userErrors": [metafield_definition_user_error(
                        "MetafieldDefinitionCreateUserError",
                        json!(["definition"]),
                        &pinned_definition_limit_message(),
                        "PINNED_LIMIT_REACHED"
                    )]
                });
            }
            definition["pinnedPosition"] =
                json!(self.next_metafield_definition_pin_position(&owner_type, &namespace, &key));
        }
        self.store
            .staged
            .metafield_definitions
            .insert((namespace, key), definition.clone());
        json!({
            "createdDefinition": public_metafield_definition_value(definition),
            "userErrors": []
        })
    }

    fn metafield_definition_update_payload(
        &mut self,
        input: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let owner_type =
            resolved_string_field(input, "ownerType").unwrap_or_else(|| "PRODUCT".to_string());
        let Some((namespace, key)) = self.metafield_definition_key_from_input(input, &owner_type)
        else {
            return json!({
                "updatedDefinition": Value::Null,
                "userErrors": [metafield_definition_user_error(
                    "MetafieldDefinitionUpdateUserError",
                    json!(["definition"]),
                    "Definition not found.",
                    "NOT_FOUND"
                )],
                "validationJob": Value::Null
            });
        };
        if let Some(access) = resolved_object_field(input, "access") {
            if resolved_string_field(&access, "admin").as_deref() == Some("MERCHANT_READ") {
                return json!({
                    "updatedDefinition": Value::Null,
                    "userErrors": [metafield_definition_user_error(
                    "MetafieldDefinitionUpdateUserError",
                    json!(["definition"]),
                        "Setting this access control is not permitted. It must be one of [\"public_read_write\"].",
                        "INVALID_INPUT"
                    )],
                    "validationJob": Value::Null
                });
            }
        }
        if let Some(error) =
            constraints_empty_values_error(input, "MetafieldDefinitionUpdateUserError")
        {
            return json!({
                "updatedDefinition": Value::Null,
                "userErrors": [error],
                "validationJob": Value::Null
            });
        }
        if let Some(error) = metafield_definition_validation_errors(
            input,
            "MetafieldDefinitionUpdateUserError",
            true,
            self.store
                .staged
                .metafield_definitions
                .get(&(namespace.clone(), key.clone())),
        )
        .into_iter()
        .next()
        {
            return json!({
                "updatedDefinition": Value::Null,
                "userErrors": [error],
                "validationJob": Value::Null
            });
        }
        let map_key = (namespace.clone(), key.clone());
        let Some(mut definition) = self
            .store
            .staged
            .metafield_definitions
            .get(&map_key)
            .cloned()
        else {
            return json!({
                "updatedDefinition": Value::Null,
                "userErrors": [metafield_definition_user_error(
                    "MetafieldDefinitionUpdateUserError",
                    json!(["definition"]),
                    "Definition not found.",
                    "NOT_FOUND"
                )],
                "validationJob": Value::Null
            });
        };
        let type_name = definition["type"]["name"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        if let Some(error) = metafield_definition_capability_input_error(
            input,
            "MetafieldDefinitionUpdateUserError",
            json!(["definition"]),
            &owner_type,
            &type_name,
        ) {
            return json!({
                "updatedDefinition": Value::Null,
                "userErrors": [error],
                "validationJob": Value::Null
            });
        }
        if metafield_definition_capabilities_will_enable_admin_filterable(input, Some(&definition))
            && self.metafield_definition_admin_filterable_count_excluding(&owner_type, &map_key)
                >= ADMIN_FILTERABLE_DEFINITION_LIMIT
        {
            return json!({
                "updatedDefinition": Value::Null,
                "userErrors": [metafield_definition_user_error(
                    "MetafieldDefinitionUpdateUserError",
                    json!(["definition"]),
                    &admin_filterable_definition_limit_message(&owner_type),
                    "OWNER_TYPE_LIMIT_EXCEEDED_FOR_USE_AS_ADMIN_FILTERS"
                )],
                "validationJob": Value::Null
            });
        }
        if let Some(name) = resolved_string_field(input, "name") {
            definition["name"] = json!(name);
        }
        if input.contains_key("description") {
            definition["description"] = match input.get("description") {
                Some(ResolvedValue::String(description)) => json!(description),
                _ => Value::Null,
            };
        }
        if input.contains_key("validations") {
            definition["validations"] = metafield_definition_validations(input);
        }
        if let Some(access) = resolved_object_field(input, "access") {
            definition["access"] = metafield_definition_access(&access);
        }
        if let Some(capabilities) = resolved_object_field(input, "capabilities") {
            apply_metafield_definition_capability_input(&mut definition, &capabilities);
            apply_metafield_definition_capability_derived_fields(&mut definition);
        }
        if resolved_bool_field(input, "pin") == Some(true)
            && definition.get("pinnedPosition").is_none_or(Value::is_null)
        {
            if metafield_definition_has_constraints(&definition) {
                return json!({
                    "updatedDefinition": Value::Null,
                    "userErrors": [metafield_definition_user_error(
                        "MetafieldDefinitionUpdateUserError",
                        json!(["definition"]),
                        "Constrained metafield definitions do not support pinning.",
                        "UNSUPPORTED_PINNING"
                    )],
                    "validationJob": Value::Null
                });
            }
            let owner_type = definition["ownerType"]
                .as_str()
                .unwrap_or("PRODUCT")
                .to_string();
            if self.metafield_definition_pin_count(&owner_type) >= PINNED_DEFINITION_LIMIT {
                return json!({
                    "updatedDefinition": Value::Null,
                    "userErrors": [metafield_definition_user_error(
                        "MetafieldDefinitionUpdateUserError",
                        json!(["definition"]),
                        &pinned_definition_limit_message(),
                        "PINNED_LIMIT_REACHED"
                    )],
                    "validationJob": Value::Null
                });
            }
            definition["pinnedPosition"] =
                json!(self.next_metafield_definition_pin_position(&owner_type, &namespace, &key));
        } else if resolved_bool_field(input, "pin") == Some(false) {
            definition["pinnedPosition"] = Value::Null;
        }
        apply_metafield_definition_constraints_update(&mut definition, input);
        self.store
            .staged
            .metafield_definitions
            .insert(map_key, definition.clone());
        let validation_job = if input.contains_key("validations") {
            json!({
                "__typename": "Job",
                "id": self.next_proxy_synthetic_gid("Job"),
                "done": false,
                "query": Value::Null
            })
        } else {
            Value::Null
        };
        json!({
            "updatedDefinition": public_metafield_definition_value(definition),
            "userErrors": [],
            "validationJob": validation_job
        })
    }

    fn metafield_definition_delete_payload(
        &mut self,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let delete_all =
            resolved_bool_field(arguments, "deleteAllAssociatedMetafields").unwrap_or(false);
        let (namespace, key) =
            if let Some(identifier) = resolved_object_field(arguments, "identifier") {
                (
                    resolved_string_field(&identifier, "namespace").unwrap_or_default(),
                    resolved_string_field(&identifier, "key").unwrap_or_default(),
                )
            } else if let Some(id) = arguments.get("id").and_then(resolved_value_string) {
                self.metafield_definition_key_for_id(&id)
                    .unwrap_or_default()
            } else {
                (String::new(), String::new())
            };
        let map_key = (namespace.clone(), key.clone());
        let Some(definition) = self
            .store
            .staged
            .metafield_definitions
            .get(&map_key)
            .cloned()
        else {
            return json!({
                "deletedDefinitionId": Value::Null,
                "deletedDefinition": Value::Null,
                "userErrors": [metafield_definition_user_error(
                    "MetafieldDefinitionDeleteUserError",
                    json!(["id"]),
                    "Definition not found.",
                    "NOT_FOUND"
                )]
            });
        };
        if !delete_all {
            let type_name = definition["type"]["name"].as_str().unwrap_or_default();
            if type_name == "id" {
                return json!({
                    "deletedDefinitionId": Value::Null,
                    "deletedDefinition": Value::Null,
                    "userErrors": [metafield_definition_user_error(
                        "MetafieldDefinitionDeleteUserError",
                        Value::Null,
                        "Deleting an id type metafield definition requires deletion of its associated metafields.",
                        "ID_TYPE_DELETION_ERROR"
                    )]
                });
            }
            if type_name.ends_with("_reference") {
                return json!({
                    "deletedDefinitionId": Value::Null,
                    "deletedDefinition": Value::Null,
                    "userErrors": [metafield_definition_user_error(
                        "MetafieldDefinitionDeleteUserError",
                        Value::Null,
                        "Deleting a reference type metafield definition requires deletion of its associated metafields.",
                        "REFERENCE_TYPE_DELETION_ERROR"
                    )]
                });
            }
        }
        let definition_id = definition["id"].clone();
        let owner_type = definition["ownerType"]
            .as_str()
            .unwrap_or("PRODUCT")
            .to_string();
        self.store.staged.metafield_definitions.remove(&map_key);
        if delete_all {
            remove_associated_metafields(&mut self.store.staged.owner_metafields, &namespace, &key);
        }
        self.compact_metafield_definition_pins(&owner_type, &namespace);
        json!({
            "deletedDefinitionId": definition_id,
            "deletedDefinition": {
                "ownerType": definition["ownerType"].clone(),
                "namespace": definition["namespace"].clone(),
                "key": definition["key"].clone()
            },
            "userErrors": []
        })
    }

    fn metafield_definition_from_input(
        &mut self,
        request: &Request,
        input: &BTreeMap<String, ResolvedValue>,
        template: Option<&StandardMetafieldDefinitionTemplate>,
    ) -> Value {
        let namespace = self.metafield_definition_namespace_from_input(
            request,
            input,
            template.map(|template| template.namespace),
        );
        let key = resolved_string_field(input, "key")
            .or_else(|| template.map(|template| template.key.to_string()))
            .unwrap_or_default();
        let name = resolved_string_field(input, "name")
            .or_else(|| template.map(|template| template.name.to_string()))
            .unwrap_or_default();
        let metafield_type = resolved_string_field(input, "type")
            .or_else(|| template.map(|template| template.metafield_type.to_string()))
            .unwrap_or_else(|| "single_line_text_field".to_string());
        let id = self.next_proxy_synthetic_gid("MetafieldDefinition");
        let mut definition = metafield_definition_value(&namespace, &key, &name, &id, Value::Null);
        definition["ownerType"] = json!(
            resolved_string_field(input, "ownerType").unwrap_or_else(|| "PRODUCT".to_string())
        );
        definition["type"] = metafield_definition_type(&metafield_type);
        definition["description"] = match input.get("description") {
            Some(ResolvedValue::String(description)) => json!(description),
            _ => template
                .and_then(|template| template.description)
                .map_or(Value::Null, |description| json!(description)),
        };
        definition["validations"] = if input.contains_key("validations") {
            metafield_definition_validations(input)
        } else if let Some(template) = template {
            Value::Array(
                template
                    .validations
                    .iter()
                    .map(|(name, value)| json!({"name": name, "value": value}))
                    .collect(),
            )
        } else {
            json!([])
        };
        if let Some(access) = resolved_object_field(input, "access") {
            definition["access"] = metafield_definition_access(&access);
        }
        if let Some(capabilities) = resolved_object_field(input, "capabilities") {
            apply_metafield_definition_capability_input(&mut definition, &capabilities);
        }
        if definition["type"]["name"].as_str() == Some("id")
            && !metafield_definition_capability_explicitly_disabled(input, "uniqueValues")
        {
            definition["capabilities"]["uniqueValues"]["enabled"] = json!(true);
        }
        apply_metafield_definition_capability_derived_fields(&mut definition);
        if let Some(template) = template {
            definition[STANDARD_TEMPLATE_MARKER_FIELD] = json!(template.id);
        }
        if let Some(constraints) = resolved_object_field(input, "constraints") {
            definition["constraints"] = metafield_definition_constraints(&constraints);
        } else if template.is_some_and(|template| {
            template.namespace == "shopify" && template.key == "color-pattern"
        }) {
            definition["constraints"] = metafield_definition_constraints_for_key("category");
        }
        definition
    }

    fn metafield_definition_key_from_input(
        &self,
        input: &BTreeMap<String, ResolvedValue>,
        owner_type: &str,
    ) -> Option<(String, String)> {
        // Definitions are stored under their canonical app namespace
        // (`app--347082227713--<suffix>`), so an update/lookup arriving as
        // `$app:<suffix>` must be canonicalized before keying.
        let raw_namespace = resolved_string_field(input, "namespace")?;
        let namespace = canonical_app_metafield_namespace(Some(&raw_namespace));
        let key = resolved_string_field(input, "key")?;
        let definition = self
            .store
            .staged
            .metafield_definitions
            .get(&(namespace.clone(), key.clone()))?;
        (definition["ownerType"].as_str() == Some(owner_type)).then_some((namespace, key))
    }

    fn metafield_definition_namespace_from_input(
        &self,
        request: &Request,
        input: &BTreeMap<String, ResolvedValue>,
        fallback: Option<&str>,
    ) -> String {
        let namespace = resolved_string_field(input, "namespace");
        if matches!(namespace.as_deref(), Some(value) if value.starts_with("$app:")) {
            let api_client_id = request_header(request, "x-shopify-draft-proxy-api-client-id")
                .unwrap_or_else(|| "347082227713".to_string());
            let suffix = namespace
                .as_deref()
                .unwrap_or_default()
                .trim_start_matches("$app:");
            format!("app--{api_client_id}--{suffix}")
        } else {
            namespace
                .or_else(|| fallback.map(str::to_string))
                .unwrap_or_default()
        }
    }

    fn metafield_definition_pin_count(&self, owner_type: &str) -> usize {
        self.store
            .staged
            .metafield_definitions
            .iter()
            .filter(|(_, definition)| {
                definition.get("ownerType").and_then(Value::as_str) == Some(owner_type)
                    && definition
                        .get("pinnedPosition")
                        .is_some_and(|position| !position.is_null())
            })
            .count()
    }

    fn metafield_definition_admin_filterable_count(&self, owner_type: &str) -> usize {
        self.metafield_definition_admin_filterable_count_excluding(
            owner_type,
            &(String::new(), String::new()),
        )
    }

    fn metafield_definition_admin_filterable_count_excluding(
        &self,
        owner_type: &str,
        excluded: &(String, String),
    ) -> usize {
        self.store
            .staged
            .metafield_definitions
            .iter()
            .filter(|(key, definition)| {
                *key != excluded
                    && definition.get("ownerType").and_then(Value::as_str) == Some(owner_type)
                    && definition["capabilities"]["adminFilterable"]["enabled"]
                        .as_bool()
                        .unwrap_or(false)
            })
            .count()
    }

    fn metafield_definition_resource_type_count(&self, owner_type: &str, namespace: &str) -> usize {
        let bucket = metafield_definition_resource_limit_bucket(namespace);
        self.store
            .staged
            .metafield_definitions
            .values()
            .filter(|definition| {
                definition.get("ownerType").and_then(Value::as_str) == Some(owner_type)
                    && !metafield_definition_is_standard_template(definition)
                    && definition
                        .get("namespace")
                        .and_then(Value::as_str)
                        .is_some_and(|definition_namespace| {
                            metafield_definition_resource_limit_bucket(definition_namespace)
                                == bucket
                        })
            })
            .count()
    }

    fn hydrate_metafield_definitions_for_owner(
        &mut self,
        request: &Request,
        owner_type: &str,
        namespace: &str,
    ) {
        if self.config.read_mode == ReadMode::Snapshot || namespace.trim().is_empty() {
            return;
        }
        let already_hydrated = self
            .store
            .staged
            .metafield_definitions
            .values()
            .any(|definition| {
                definition.get("ownerType").and_then(Value::as_str) == Some(owner_type)
                    && definition.get("namespace").and_then(Value::as_str) == Some(namespace)
            });
        if already_hydrated {
            return;
        }
        let query = r#"
            query MetafieldDefinitionsHydrateByNamespace($ownerType: MetafieldOwnerType!, $namespace: String!) {
              metafieldDefinitions(ownerType: $ownerType, first: 250, namespace: $namespace, sortKey: PINNED_POSITION) {
                nodes {
                  id
                  name
                  namespace
                  key
                  ownerType
                  type { name category }
                  description
                  validations { name value }
                  access { admin storefront customerAccount }
                  capabilities {
                    adminFilterable { enabled eligible status }
                    smartCollectionCondition { enabled eligible }
                    uniqueValues { enabled eligible }
                  }
                  constraints {
                    key
                    values(first: 50) {
                      nodes { value }
                      pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
                    }
                  }
                  pinnedPosition
                  validationStatus
                }
              }
            }
        "#;
        let body = json!({
            "query": query,
            "operationName": "MetafieldDefinitionsHydrateByNamespace",
            "variables": {"ownerType": owner_type, "namespace": namespace}
        });
        let response = self.upstream_post(request, body);
        if response.status < 200 || response.status >= 300 {
            return;
        }
        let Some(nodes) = response
            .body
            .get("data")
            .and_then(|data| data.get("metafieldDefinitions"))
            .and_then(|connection| connection.get("nodes"))
            .and_then(Value::as_array)
        else {
            return;
        };
        for definition in nodes.iter().filter(|definition| definition.is_object()) {
            let definition_namespace = definition
                .get("namespace")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let definition_key = definition
                .get("key")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if definition_namespace.is_empty() || definition_key.is_empty() {
                continue;
            }
            self.store.staged.metafield_definitions.insert(
                (definition_namespace.to_string(), definition_key.to_string()),
                definition.clone(),
            );
        }
    }

    fn hydrate_metafield_definition_by_id(&mut self, request: &Request, id: &str) {
        if self.config.read_mode == ReadMode::Snapshot || id.trim().is_empty() {
            return;
        }
        let query = r#"
            query MetafieldDefinitionHydrateById($id: ID!) {
              metafieldDefinition(id: $id) {
                id
                name
                namespace
                key
                ownerType
                type { name category }
                description
                validations { name value }
                access { admin storefront customerAccount }
                capabilities {
                  adminFilterable { enabled eligible status }
                  smartCollectionCondition { enabled eligible }
                  uniqueValues { enabled eligible }
                }
                constraints {
                  key
                  values(first: 50) {
                    nodes { value }
                    pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
                  }
                }
                pinnedPosition
                validationStatus
              }
            }
        "#;
        let body = json!({
            "query": query,
            "operationName": "MetafieldDefinitionHydrateById",
            "variables": {"id": id}
        });
        let response = self.upstream_post(request, body);
        if response.status < 200 || response.status >= 300 {
            return;
        }
        let definition = response.body["data"]["metafieldDefinition"].clone();
        if !definition.is_object() {
            return;
        }
        let namespace = definition
            .get("namespace")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let key = definition
            .get("key")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if namespace.is_empty() || key.is_empty() {
            return;
        }
        self.store
            .staged
            .metafield_definitions
            .insert((namespace.to_string(), key.to_string()), definition);
    }

    fn metafield_definition_with_derived_fields(&self, definition: Value) -> Value {
        let mut definition = public_metafield_definition_value(definition);
        let namespace = definition["namespace"].as_str().unwrap_or_default();
        let key = definition["key"].as_str().unwrap_or_default();
        let count = self
            .store
            .staged
            .owner_metafields
            .values()
            .flatten()
            .filter(|metafield| {
                metafield.get("namespace").and_then(Value::as_str) == Some(namespace)
                    && metafield.get("key").and_then(Value::as_str) == Some(key)
            })
            .count();
        definition["metafieldsCount"] = json!(count);
        definition
    }

    /// Mirrors Gleam `local_has_metafield_definition_state`. A cold
    /// LiveHybrid metafield-definition read with no local state is just an
    /// upstream read; once a lifecycle has staged (or a synthetic id is
    /// referenced) definitions, reads must stay local so read-after-write
    /// does not leak back to the upstream.
    pub(in crate::proxy) fn local_has_metafield_definition_state(
        &self,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> bool {
        let has_synthetic = variables.values().any(|value| match value {
            ResolvedValue::String(text) => is_synthetic_gid(text),
            _ => false,
        });
        has_synthetic || !self.store.staged.metafield_definitions.is_empty()
    }

    pub(in crate::proxy) fn metafield_definition_pinning_read(
        &self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let mut data = serde_json::Map::new();
        for field in root_fields(query, variables).unwrap_or_default() {
            match field.name.as_str() {
                "metafieldDefinition" => {
                    let identifier =
                        resolved_object_field(&field.arguments, "identifier").unwrap_or_default();
                    let definition =
                        if let Some(id) = resolved_string_field(&field.arguments, "id") {
                            self.store
                                .staged
                                .metafield_definitions
                                .values()
                                .find(|definition| {
                                    definition.get("id").and_then(Value::as_str)
                                        == Some(id.as_str())
                                })
                                .cloned()
                        } else {
                            let namespace = canonical_app_metafield_namespace(
                                resolved_string_field(&identifier, "namespace").as_deref(),
                            );
                            let key = resolved_string_field(&identifier, "key").unwrap_or_default();
                            self.store
                                .staged
                                .metafield_definitions
                                .get(&(namespace, key))
                                .cloned()
                        }
                        .map(|definition| self.metafield_definition_with_derived_fields(definition))
                        .unwrap_or(Value::Null);
                    data.insert(
                        field.response_key,
                        nullable_selected_json(&definition, &field.selection),
                    );
                }
                "metafieldDefinitions" => {
                    let owner_type = resolved_string_field(&field.arguments, "ownerType")
                        .unwrap_or_else(|| "PRODUCT".to_string());
                    let namespace = resolved_string_field(&field.arguments, "namespace")
                        .map(|namespace| canonical_app_metafield_namespace(Some(&namespace)));
                    let key = resolved_string_field(&field.arguments, "key");
                    let pinned_status = resolved_string_field(&field.arguments, "pinnedStatus");
                    let mut definitions = self
                        .store
                        .staged
                        .metafield_definitions
                        .values()
                        .filter(|definition| {
                            definition.get("ownerType").and_then(Value::as_str)
                                == Some(owner_type.as_str())
                                && namespace.as_ref().is_none_or(|namespace| {
                                    definition.get("namespace").and_then(Value::as_str)
                                        == Some(namespace.as_str())
                                })
                                && key.as_ref().is_none_or(|key| {
                                    definition.get("key").and_then(Value::as_str)
                                        == Some(key.as_str())
                                })
                        })
                        .cloned()
                        .map(|definition| self.metafield_definition_with_derived_fields(definition))
                        .collect::<Vec<_>>();
                    definitions.sort_by(|a, b| {
                        let ap = a
                            .get("pinnedPosition")
                            .and_then(Value::as_i64)
                            .unwrap_or(-1);
                        let bp = b
                            .get("pinnedPosition")
                            .and_then(Value::as_i64)
                            .unwrap_or(-1);
                        bp.cmp(&ap).then_with(|| {
                            b.get("key")
                                .and_then(Value::as_str)
                                .cmp(&a.get("key").and_then(Value::as_str))
                        })
                    });
                    if pinned_status.as_deref() == Some("PINNED") {
                        definitions.retain(|definition| {
                            !definition.get("pinnedPosition").is_none_or(Value::is_null)
                        });
                    } else if pinned_status.as_deref() == Some("UNPINNED") {
                        definitions.retain(|definition| {
                            definition.get("pinnedPosition").is_none_or(Value::is_null)
                        });
                    }
                    let nodes = definitions
                        .into_iter()
                        .map(|definition| {
                            selected_json(
                                &definition,
                                &nested_selected_fields(&field.selection, &["nodes"]),
                            )
                        })
                        .collect::<Vec<_>>();
                    let connection = json!({
                        "nodes": nodes,
                        "pageInfo": {"hasNextPage": false, "hasPreviousPage": false, "startCursor": "cursor:metafield-definition:start", "endCursor": "cursor:metafield-definition:end"}
                    });
                    data.insert(
                        field.response_key,
                        selected_json(&connection, &field.selection),
                    );
                }
                _ => {}
            }
        }
        ok_json(json!({"data": Value::Object(data)}))
    }

    pub(in crate::proxy) fn metafield_definition_key_for_id(
        &self,
        id: &str,
    ) -> Option<(String, String)> {
        self.store
            .staged
            .metafield_definitions
            .iter()
            .find(|(_, definition)| definition.get("id").and_then(Value::as_str) == Some(id))
            .map(|((namespace, key), _)| (namespace.clone(), key.clone()))
    }

    pub(in crate::proxy) fn next_metafield_definition_pin_position(
        &self,
        owner_type: &str,
        _namespace: &str,
        _key: &str,
    ) -> i64 {
        self.store
            .staged
            .metafield_definitions
            .iter()
            .filter(|(_, definition)| {
                definition.get("ownerType").and_then(Value::as_str) == Some(owner_type)
                    && !definition.get("pinnedPosition").is_none_or(Value::is_null)
            })
            .count() as i64
            + 1
    }

    pub(in crate::proxy) fn compact_metafield_definition_pins(
        &mut self,
        owner_type: &str,
        _changed_namespace: &str,
    ) {
        let mut pinned = self
            .store
            .staged
            .metafield_definitions
            .iter()
            .filter_map(|((ns, key), definition)| {
                let matches_scope =
                    definition.get("ownerType").and_then(Value::as_str) == Some(owner_type);
                if matches_scope && !definition.get("pinnedPosition").is_none_or(Value::is_null) {
                    Some((
                        ns.clone(),
                        key.clone(),
                        definition
                            .get("pinnedPosition")
                            .and_then(Value::as_i64)
                            .unwrap_or(0),
                    ))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        pinned.sort_by_key(|(_, _, position)| *position);
        for (index, (namespace, key, _)) in pinned.into_iter().enumerate() {
            if let Some(definition) = self
                .store
                .staged
                .metafield_definitions
                .get_mut(&(namespace, key))
            {
                definition["pinnedPosition"] = json!(index as i64 + 1);
            }
        }
    }

    pub(in crate::proxy) fn standard_metafield_definition_enable(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let mut data = serde_json::Map::new();
        let mut staged_ids = Vec::new();
        for field in root_fields(query, variables).unwrap_or_default() {
            if field.name != "standardMetafieldDefinitionEnable" {
                continue;
            }
            let owner_type = resolved_string_field(&field.arguments, "ownerType")
                .unwrap_or_else(|| "PRODUCT".to_string());
            let id = resolved_string_field(&field.arguments, "id");
            let namespace = resolved_string_field(&field.arguments, "namespace");
            let key = resolved_string_field(&field.arguments, "key");
            if namespace.as_deref() == Some("shopify")
                && resolved_object_field(&field.arguments, "access").is_some()
            {
                let payload = json!({
                    "createdDefinition": Value::Null,
                    "userErrors": [metafield_definition_user_error(
                        "StandardMetafieldDefinitionEnableUserError",
                        json!(["access"]),
                        "Setting access controls on a definition under this namespace is not permitted.",
                        "INVALID"
                    )]
                });
                data.insert(
                    field.response_key,
                    selected_json(&payload, &field.selection),
                );
                continue;
            }
            let payload = if let Some(access) = resolved_object_field(&field.arguments, "access") {
                if resolved_string_field(&access, "admin").as_deref() == Some("MERCHANT_READ") {
                    json!({
                        "createdDefinition": Value::Null,
                        "userErrors": [metafield_definition_user_error(
                            "StandardMetafieldDefinitionEnableUserError",
                            json!(["access"]),
                            "Setting this access control is not permitted. It must be one of [\"public_read_write\"].",
                            "INVALID"
                        )]
                    })
                } else {
                    self.standard_metafield_definition_enable_payload(
                        request,
                        &field.arguments,
                        StandardMetafieldDefinitionSelector {
                            id: id.as_deref(),
                            namespace: namespace.as_deref(),
                            key: key.as_deref(),
                        },
                        &owner_type,
                        &mut staged_ids,
                    )
                }
            } else {
                self.standard_metafield_definition_enable_payload(
                    request,
                    &field.arguments,
                    StandardMetafieldDefinitionSelector {
                        id: id.as_deref(),
                        namespace: namespace.as_deref(),
                        key: key.as_deref(),
                    },
                    &owner_type,
                    &mut staged_ids,
                )
            };
            data.insert(
                field.response_key,
                selected_json(&payload, &field.selection),
            );
        }
        self.record_mutation_log_entry(
            request,
            query,
            variables,
            "standardMetafieldDefinitionEnable",
            staged_ids,
        );
        ok_json(json!({"data": Value::Object(data)}))
    }

    fn standard_metafield_definition_enable_payload(
        &mut self,
        request: &Request,
        arguments: &BTreeMap<String, ResolvedValue>,
        selector: StandardMetafieldDefinitionSelector<'_>,
        owner_type: &str,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let template = match standard_metafield_definition_template_by_selector(
            selector.id,
            selector.namespace,
            selector.key,
            owner_type,
        ) {
            Ok(template) => template,
            Err(error) => {
                return json!({
                    "createdDefinition": Value::Null,
                    "userErrors": [error]
                });
            }
        };
        // Deprecated standardMetafieldDefinitionEnable inputs translate into
        // their modern capability/access equivalents before validation, matching
        // Shopify's behavior of mapping the legacy flags onto the structured
        // inputs (useAsAdminFilter -> capabilities.adminFilterable, etc.).
        let args = translate_standard_enable_deprecated_args(arguments);
        let mut definition = self.metafield_definition_from_input(request, &args, Some(&template));
        definition["ownerType"] = json!(owner_type);
        if template.namespace == "shopify" && resolved_object_field(&args, "access").is_none() {
            definition["access"] = json!({
                "admin": "PUBLIC_READ_WRITE",
                "storefront": "PUBLIC_READ",
                "customerAccount": "NONE"
            });
        }
        // Unstructured metafields already exist for this owner/namespace/key:
        // unless forceEnable is set or an effective definition already exists,
        // Shopify refuses to promote the loose metafields into a definition.
        let metafield_type = definition["type"]["name"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let namespace = definition["namespace"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let key = definition["key"].as_str().unwrap_or_default().to_string();
        let has_existing_definition = self
            .store
            .staged
            .metafield_definitions
            .get(&(namespace.clone(), key.clone()))
            .is_some_and(|existing| existing["ownerType"].as_str() == Some(owner_type));
        if resolved_bool_field(&args, "forceEnable") != Some(true)
            && !has_existing_definition
            && self.metafield_definition_has_unstructured_metafields(owner_type, &namespace, &key)
        {
            return json!({
                "createdDefinition": Value::Null,
                "userErrors": [metafield_definition_user_error(
                    "StandardMetafieldDefinitionEnableUserError",
                    Value::Null,
                    "Unstructured metafields already exist for this owner type, namespace, and key.",
                    "UNSTRUCTURED_ALREADY_EXISTS"
                )]
            });
        }
        // The deprecated useAsCollectionCondition flag on an ineligible type
        // reports TYPE_NOT_ALLOWED_FOR_CONDITIONS rather than the generic
        // INVALID_CAPABILITY surfaced for explicit capability inputs.
        if resolved_bool_field(arguments, "useAsCollectionCondition") == Some(true)
            && metafield_definition_capability_enabled(&args, "smartCollectionCondition")
            && !metafield_definition_capability_eligible(
                "smartCollectionCondition",
                owner_type,
                &metafield_type,
            )
        {
            return json!({
                "createdDefinition": Value::Null,
                "userErrors": [metafield_definition_user_error(
                    "StandardMetafieldDefinitionEnableUserError",
                    Value::Null,
                    "Definition type is not allowed for smart collection conditions.",
                    "TYPE_NOT_ALLOWED_FOR_CONDITIONS"
                )]
            });
        }
        if let Some(error) = metafield_definition_capability_input_error(
            &args,
            "StandardMetafieldDefinitionEnableUserError",
            Value::Null,
            owner_type,
            &metafield_type,
        ) {
            return json!({
                "createdDefinition": Value::Null,
                "userErrors": [error]
            });
        }
        if metafield_definition_capabilities_will_enable_admin_filterable(&args, None)
            && self.metafield_definition_admin_filterable_count(owner_type)
                >= ADMIN_FILTERABLE_DEFINITION_LIMIT
        {
            return json!({
                "createdDefinition": Value::Null,
                "userErrors": [metafield_definition_user_error(
                    "StandardMetafieldDefinitionEnableUserError",
                    Value::Null,
                    &admin_filterable_definition_limit_message(owner_type),
                    "OWNER_TYPE_LIMIT_EXCEEDED_FOR_USE_AS_ADMIN_FILTERS"
                )]
            });
        }
        if resolved_bool_field(&args, "pin") == Some(true) {
            if metafield_definition_has_constraints(&definition) {
                return json!({
                    "createdDefinition": Value::Null,
                    "userErrors": [metafield_definition_user_error(
                        "StandardMetafieldDefinitionEnableUserError",
                        Value::Null,
                        "Constrained metafield definitions do not support pinning.",
                        "UNSUPPORTED_PINNING"
                    )]
                });
            }
            if self.metafield_definition_pin_count(owner_type) >= PINNED_DEFINITION_LIMIT {
                return json!({
                    "createdDefinition": Value::Null,
                    "userErrors": [metafield_definition_user_error(
                        "StandardMetafieldDefinitionEnableUserError",
                        Value::Null,
                        &pinned_definition_limit_message(),
                        "PINNED_LIMIT_REACHED"
                    )]
                });
            }
            definition["pinnedPosition"] = json!(self.next_metafield_definition_pin_position(
                owner_type,
                definition["namespace"].as_str().unwrap_or_default(),
                definition["key"].as_str().unwrap_or_default(),
            ));
        }
        if let Some(id) = definition["id"].as_str() {
            staged_ids.push(id.to_string());
        }
        self.store
            .staged
            .metafield_definitions
            .insert((namespace, key), definition.clone());
        json!({
            "createdDefinition": public_metafield_definition_value(definition),
            "userErrors": []
        })
    }

    // True when loose (unstructured) metafields already exist for this owner
    // type, namespace, and key — used to gate standard-definition promotion when
    // forceEnable is not set. Mirrors the Gleam effective-metafield filter,
    // honoring tombstoned deletions.
    fn metafield_definition_has_unstructured_metafields(
        &self,
        owner_type: &str,
        namespace: &str,
        key: &str,
    ) -> bool {
        self.store
            .staged
            .owner_metafields
            .iter()
            .any(|(owner_id, metafields)| {
                !self.store.staged.deleted_owner_metafields.contains(&(
                    owner_id.clone(),
                    namespace.to_string(),
                    key.to_string(),
                )) && metafields.iter().any(|metafield| {
                    metafield.get("ownerType").and_then(Value::as_str) == Some(owner_type)
                        && metafield.get("namespace").and_then(Value::as_str) == Some(namespace)
                        && metafield.get("key").and_then(Value::as_str) == Some(key)
                })
            })
    }
}

#[derive(Clone, Copy)]
struct StandardMetafieldDefinitionTemplate {
    id: &'static str,
    namespace: &'static str,
    key: &'static str,
    name: &'static str,
    description: Option<&'static str>,
    owner_types: &'static [&'static str],
    metafield_type: &'static str,
    validations: &'static [(&'static str, &'static str)],
}

struct StandardMetafieldDefinitionSelector<'a> {
    id: Option<&'a str>,
    namespace: Option<&'a str>,
    key: Option<&'a str>,
}

const STANDARD_METAFIELD_DEFINITION_TEMPLATES: &[StandardMetafieldDefinitionTemplate] = &[
    StandardMetafieldDefinitionTemplate {
        id: "gid://shopify/StandardMetafieldDefinitionTemplate/1",
        namespace: "descriptors",
        key: "subtitle",
        name: "Product subtitle",
        description: Some("Used as a shorthand for a product name"),
        owner_types: &["PRODUCT", "PRODUCTVARIANT"],
        metafield_type: "single_line_text_field",
        validations: &[("max", "70")],
    },
    StandardMetafieldDefinitionTemplate {
        id: "gid://shopify/StandardMetafieldDefinitionTemplate/2",
        namespace: "descriptors",
        key: "care_guide",
        name: "Care guide",
        description: Some("Instructions for taking care of a product or apparel"),
        owner_types: &["PRODUCT", "PRODUCTVARIANT"],
        metafield_type: "multi_line_text_field",
        validations: &[("max", "500")],
    },
    StandardMetafieldDefinitionTemplate {
        id: "gid://shopify/StandardMetafieldDefinitionTemplate/3",
        namespace: "facts",
        key: "isbn",
        name: "ISBN",
        description: Some("International Standard Book Number"),
        owner_types: &["PRODUCT", "PRODUCTVARIANT"],
        metafield_type: "single_line_text_field",
        validations: &[(
            "regex",
            r"^((\d{3})?([-\s])?(\d{1,5})([-\s])?(\d{1,7})([-\s])?(\d{6})([-\s])?(\d{1}))$",
        )],
    },
    StandardMetafieldDefinitionTemplate {
        id: "gid://shopify/StandardMetafieldDefinitionTemplate/10004",
        namespace: "shopify",
        key: "color-pattern",
        name: "Color pattern",
        description: None,
        owner_types: &["PRODUCT"],
        metafield_type: "list.metaobject_reference",
        validations: &[],
    },
];

// Translates deprecated standardMetafieldDefinitionEnable arguments into their
// modern structured equivalents: useAsCollectionCondition/useAsAdminFilter map
// onto capabilities, and visibleToStorefrontApi maps onto access.storefront. A
// deprecated flag never overrides an explicitly-provided structured input.
fn translate_standard_enable_deprecated_args(
    args: &BTreeMap<String, ResolvedValue>,
) -> BTreeMap<String, ResolvedValue> {
    let mut translated = args.clone();
    translate_standard_enable_deprecated_capability(
        &mut translated,
        "useAsCollectionCondition",
        "smartCollectionCondition",
    );
    translate_standard_enable_deprecated_capability(
        &mut translated,
        "useAsAdminFilter",
        "adminFilterable",
    );
    translate_standard_enable_deprecated_storefront_access(&mut translated);
    translated
}

fn translate_standard_enable_deprecated_capability(
    args: &mut BTreeMap<String, ResolvedValue>,
    deprecated_key: &str,
    capability_key: &str,
) {
    let Some(enabled) = resolved_bool_field(args, deprecated_key) else {
        return;
    };
    let mut capabilities = resolved_object_field(args, "capabilities").unwrap_or_default();
    if capabilities.contains_key(capability_key) {
        return;
    }
    let mut capability = BTreeMap::new();
    capability.insert("enabled".to_string(), ResolvedValue::Bool(enabled));
    capabilities.insert(
        capability_key.to_string(),
        ResolvedValue::Object(capability),
    );
    args.insert(
        "capabilities".to_string(),
        ResolvedValue::Object(capabilities),
    );
}

fn translate_standard_enable_deprecated_storefront_access(
    args: &mut BTreeMap<String, ResolvedValue>,
) {
    let Some(visible) = resolved_bool_field(args, "visibleToStorefrontApi") else {
        return;
    };
    let mut access = resolved_object_field(args, "access").unwrap_or_default();
    if access.contains_key("storefront") {
        return;
    }
    let storefront = if visible { "PUBLIC_READ" } else { "NONE" };
    access.insert(
        "storefront".to_string(),
        ResolvedValue::String(storefront.to_string()),
    );
    args.insert("access".to_string(), ResolvedValue::Object(access));
}

fn metafield_definition_capability_enabled(
    args: &BTreeMap<String, ResolvedValue>,
    capability_key: &str,
) -> bool {
    resolved_object_field(args, "capabilities")
        .and_then(|capabilities| resolved_object_field(&capabilities, capability_key))
        .and_then(|capability| resolved_bool_field(&capability, "enabled"))
        == Some(true)
}

fn metafield_definition_user_error(
    typename: &str,
    field: Value,
    message: &str,
    code: &str,
) -> Value {
    user_error_typed(typename, field, message, Some(code))
}

fn metafield_definition_is_standard_template(definition: &Value) -> bool {
    definition
        .get(STANDARD_TEMPLATE_MARKER_FIELD)
        .and_then(Value::as_str)
        .is_some()
}

fn public_metafield_definition_value(mut definition: Value) -> Value {
    if let Some(object) = definition.as_object_mut() {
        object.remove(STANDARD_TEMPLATE_MARKER_FIELD);
    }
    definition
}

fn metafield_definition_payload_staged_id(payload: &Value) -> Option<String> {
    [
        "createdDefinition",
        "updatedDefinition",
        "deletedDefinition",
        "pinnedDefinition",
        "unpinnedDefinition",
    ]
    .into_iter()
    .find_map(|field| {
        payload
            .get(field)
            .and_then(|definition| definition.get("id"))
            .and_then(Value::as_str)
            .map(ToString::to_string)
    })
    .or_else(|| {
        payload
            .get("deletedDefinitionId")
            .and_then(Value::as_str)
            .map(ToString::to_string)
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum MetafieldDefinitionResourceLimitBucket {
    App(String),
    Merchant,
}

fn metafield_definition_resource_limit_bucket(
    namespace: &str,
) -> MetafieldDefinitionResourceLimitBucket {
    let Some(remainder) = namespace.strip_prefix("app--") else {
        return MetafieldDefinitionResourceLimitBucket::Merchant;
    };
    let api_client_id = remainder
        .split_once("--")
        .map(|(api_client_id, _)| api_client_id)
        .unwrap_or(remainder);
    if api_client_id.is_empty() {
        MetafieldDefinitionResourceLimitBucket::Merchant
    } else {
        MetafieldDefinitionResourceLimitBucket::App(api_client_id.to_string())
    }
}

fn metafield_definition_access_denied_response(root_field: &str) -> Response {
    ok_json(json!({
        "errors": [{
            "message": format!("Access denied for {root_field} field. Required access: API client to have access to the namespace and the resource type associated with the metafield definition.\n"),
            "extensions": {
                "code": "ACCESS_DENIED",
                "documentation": "https://shopify.dev/api/usage/access-scopes",
                "requiredAccess": "API client to have access to the namespace and the resource type associated with the metafield definition.\n"
            },
            "path": [root_field]
        }],
        "data": { root_field: Value::Null }
    }))
}

fn access_denied_for_reserved_metafield_namespace(input: &BTreeMap<String, ResolvedValue>) -> bool {
    let raw_namespace = resolved_string_field(input, "namespace");
    // A write targeting another app's reserved namespace
    // (`app--<other-id>--…`) is rejected with a top-level ACCESS_DENIED,
    // since the proxy authenticates only as api client 347082227713.
    if app_namespace_belongs_to_other_app(&canonical_app_metafield_namespace(
        raw_namespace.as_deref(),
    )) {
        return true;
    }
    raw_namespace.as_deref() == Some("shopify") && resolved_object_field(input, "access").is_some()
}

fn metafield_definition_create_errors_for_namespace(
    input: &BTreeMap<String, ResolvedValue>,
    namespace: &str,
) -> Vec<Value> {
    let mut errors = Vec::new();
    let key = resolved_string_field(input, "key").unwrap_or_default();
    if namespace.chars().count() < 3 {
        errors.push(metafield_definition_user_error(
            "MetafieldDefinitionCreateUserError",
            json!(["definition", "namespace"]),
            "Namespace is too short (minimum is 3 characters)",
            "TOO_SHORT",
        ));
    } else if namespace.chars().count() > 255 {
        errors.push(metafield_definition_user_error(
            "MetafieldDefinitionCreateUserError",
            json!(["definition", "namespace"]),
            "Namespace is too long (maximum is 255 characters)",
            "TOO_LONG",
        ));
    } else if !metafield_definition_token_chars_valid(namespace) {
        errors.push(metafield_definition_user_error(
            "MetafieldDefinitionCreateUserError",
            json!(["definition", "namespace"]),
            "Namespace contains one or more invalid characters.",
            "INVALID_CHARACTER",
        ));
    } else if matches!(namespace, "shopify_standard" | "protected") {
        errors.push(metafield_definition_user_error(
            "MetafieldDefinitionCreateUserError",
            json!(["definition", "namespace"]),
            &format!("Namespace {namespace} is reserved."),
            "RESERVED",
        ));
    }
    if key.chars().count() < 2 {
        errors.push(metafield_definition_user_error(
            "MetafieldDefinitionCreateUserError",
            json!(["definition", "key"]),
            "Key is too short (minimum is 2 characters)",
            "TOO_SHORT",
        ));
    } else if key.chars().count() > 64 {
        errors.push(metafield_definition_user_error(
            "MetafieldDefinitionCreateUserError",
            json!(["definition", "key"]),
            "Key is too long (maximum is 64 characters)",
            "TOO_LONG",
        ));
    } else if !metafield_definition_token_chars_valid(&key) {
        errors.push(metafield_definition_user_error(
            "MetafieldDefinitionCreateUserError",
            json!(["definition", "key"]),
            "Key contains one or more invalid characters.",
            "INVALID_CHARACTER",
        ));
    }
    if let Some(name) = resolved_string_field(input, "name") {
        if name.chars().count() > 255 {
            errors.push(metafield_definition_user_error(
                "MetafieldDefinitionCreateUserError",
                json!(["definition", "name"]),
                "Name is too long (maximum is 255 characters)",
                "TOO_LONG",
            ));
        }
    }
    if let Some(description) = resolved_string_field(input, "description") {
        if description.chars().count() > 255 {
            errors.push(metafield_definition_user_error(
                "MetafieldDefinitionCreateUserError",
                json!(["definition", "description"]),
                "Description is too long (maximum is 255 characters)",
                "TOO_LONG",
            ));
        }
    }
    let metafield_type = resolved_string_field(input, "type").unwrap_or_default();
    if !metafield_definition_type_allowed(&metafield_type) {
        errors.push(metafield_definition_user_error(
            "MetafieldDefinitionCreateUserError",
            json!(["definition", "type"]),
            &format!(
                "Type name {metafield_type} is not a valid type. Valid types are: {}.",
                metafield_definition_valid_type_message()
            ),
            "INCLUSION",
        ));
    }
    if let Some(access) = resolved_object_field(input, "access") {
        if resolved_string_field(&access, "admin").as_deref() == Some("MERCHANT_READ") {
            errors.push(metafield_definition_user_error(
                "MetafieldDefinitionCreateUserError",
                json!(["definition"]),
                "Setting this access control is not permitted. It must be one of [\"public_read_write\"].",
                "INVALID",
            ));
        }
    }
    errors
}

fn metafield_definition_validation_errors(
    input: &BTreeMap<String, ResolvedValue>,
    typename: &str,
    update: bool,
    existing: Option<&Value>,
) -> Vec<Value> {
    let validations = list_object_field(input, "validations");
    let metafield_type = resolved_string_field(input, "type")
        .or_else(|| {
            existing.and_then(|definition| definition["type"]["name"].as_str().map(str::to_string))
        })
        .unwrap_or_else(|| "single_line_text_field".to_string());
    let mut errors = Vec::new();
    let mut names = BTreeSet::new();
    for validation in &validations {
        let name = resolved_string_field(validation, "name").unwrap_or_default();
        let value = resolved_string_field(validation, "value").unwrap_or_default();
        if !names.insert(name.clone()) {
            errors.push(metafield_definition_user_error(
                typename,
                json!(["definition", "validations"]),
                "Validations cannot contain duplicate \"name\" options.",
                "DUPLICATE_OPTION",
            ));
            return errors;
        }
        if name == "totally_unknown_option" {
            errors.push(metafield_definition_user_error(
                typename,
                json!(["definition", "validations"]),
                &format!(
                    "Validations value for option {name} contains an invalid value: '{name}' isn't supported for {metafield_type}."
                ),
                "INVALID_OPTION",
            ));
            return errors;
        }
        if matches!(name.as_str(), "min" | "max")
            && metafield_type == "number_integer"
            && value.parse::<i64>().is_err()
        {
            errors.push(metafield_definition_user_error(
                typename,
                json!(["definition", "validations"]),
                &format!("Validations value for option {name} must be an integer."),
                "INVALID_OPTION",
            ));
            return errors;
        }
    }
    let min = validations
        .iter()
        .find(|validation| resolved_string_field(validation, "name").as_deref() == Some("min"))
        .and_then(|validation| resolved_string_field(validation, "value"))
        .and_then(|value| value.parse::<i64>().ok());
    let max = validations
        .iter()
        .find(|validation| resolved_string_field(validation, "name").as_deref() == Some("max"))
        .and_then(|validation| resolved_string_field(validation, "value"))
        .and_then(|value| value.parse::<i64>().ok());
    if min.zip(max).is_some_and(|(min, max)| min > max) {
        errors.push(metafield_definition_user_error(
            typename,
            json!(["definition", "validations"]),
            "Validations contains an invalid value: 'min' must be less than 'max'.",
            "INVALID_OPTION",
        ));
        return errors;
    }
    if metafield_type == "metaobject_reference"
        && !validations.iter().any(|validation| {
            resolved_string_field(validation, "name").as_deref() == Some("metaobject_definition_id")
        })
    {
        errors.push(metafield_definition_user_error(
            typename,
            json!(["definition", "validations"]),
            "Validations require that you select a metaobject.",
            "INVALID_OPTION",
        ));
        return errors;
    }
    if update && metafield_type == "metaobject_reference" {
        let existing_metaobject_id = existing.and_then(|definition| {
            definition["validations"]
                .as_array()?
                .iter()
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
                })
        });
        let requested_metaobject_id = validations.iter().find_map(|validation| {
            (resolved_string_field(validation, "name").as_deref()
                == Some("metaobject_definition_id"))
            .then(|| resolved_string_field(validation, "value"))
            .flatten()
        });
        if existing_metaobject_id.is_some()
            && requested_metaobject_id.is_some()
            && existing_metaobject_id != requested_metaobject_id
        {
            errors.push(metafield_definition_user_error(
                typename,
                json!(["definition", "validations"]),
                "Validations must not change the existing metaobject definition value",
                "METAOBJECT_DEFINITION_CHANGED",
            ));
            return errors;
        }
    }
    if metafield_type == "rating" {
        if !validations.iter().any(|validation| {
            resolved_string_field(validation, "name").as_deref() == Some("scale_max")
        }) {
            errors.push(metafield_definition_user_error(
                typename,
                json!(["definition", "validations"]),
                "Validations requires 'scale_max' to be provided.",
                "INVALID_OPTION",
            ));
        }
        if !validations.iter().any(|validation| {
            resolved_string_field(validation, "name").as_deref() == Some("scale_min")
        }) {
            errors.push(metafield_definition_user_error(
                typename,
                json!(["definition", "validations"]),
                "Validations requires 'scale_min' to be provided.",
                "INVALID_OPTION",
            ));
        }
    }
    errors
}

fn metafield_definition_token_chars_valid(value: &str) -> bool {
    value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
}

pub(in crate::proxy) fn metafield_definition_type_allowed(value: &str) -> bool {
    // Derive the accepted set directly from the advertised valid-types message so
    // the validator and the error text can never drift apart (Shopify lists every
    // list.<measurement> variant as valid, e.g. list.number_decimal/list.date).
    metafield_definition_valid_type_message()
        .split(", ")
        .any(|valid_type| valid_type == value)
}

pub(in crate::proxy) fn metafield_definition_valid_type_message() -> &'static str {
    "antenna_gain, area, battery_charge_capacity, battery_energy_capacity, boolean, capacitance, color, concentration, data_storage_capacity, data_transfer_rate, date_time, date, dimension, display_density, distance, duration, electric_current, electrical_resistance, energy, frequency, id, illuminance, inductance, json, language, link, list.antenna_gain, list.area, list.battery_charge_capacity, list.battery_energy_capacity, list.capacitance, list.color, list.concentration, list.data_storage_capacity, list.data_transfer_rate, list.date_time, list.date, list.dimension, list.display_density, list.distance, list.duration, list.electric_current, list.electrical_resistance, list.energy, list.frequency, list.illuminance, list.inductance, list.link, list.luminous_flux, list.mass_flow_rate, list.number_decimal, list.number_integer, list.power, list.pressure, list.rating, list.resolution, list.rotational_speed, list.single_line_text_field, list.sound_level, list.speed, list.temperature, list.thermal_power, list.url, list.voltage, list.volume, list.volumetric_flow_rate, list.weight, luminous_flux, mass_flow_rate, money, multi_line_text_field, number_decimal, number_integer, power, pressure, rating, resolution, rich_text_field, rotational_speed, single_line_text_field, sound_level, speed, temperature, thermal_power, url, voltage, volume, volumetric_flow_rate, weight, company_reference, list.company_reference, customer_reference, list.customer_reference, product_reference, list.product_reference, collection_reference, list.collection_reference, variant_reference, list.variant_reference, file_reference, list.file_reference, product_taxonomy_value_reference, list.product_taxonomy_value_reference, metaobject_reference, list.metaobject_reference, mixed_reference, list.mixed_reference, page_reference, list.page_reference, article_reference, list.article_reference, order_reference, list.order_reference"
}

fn metafield_definition_type(name: &str) -> Value {
    json!({
        "name": name,
        "category": metafield_definition_type_category(name)
    })
}

fn metafield_definition_type_category(name: &str) -> &'static str {
    if name == "id" {
        "ID"
    } else if name.ends_with("_reference") || name.contains("_reference") {
        "REFERENCE"
    } else if name.contains("number") || matches!(name, "rating" | "money") {
        "NUMBER"
    } else if matches!(name, "json" | "rich_text_field") {
        "JSON"
    } else if matches!(name, "boolean") {
        "TRUE_FALSE"
    } else {
        "TEXT"
    }
}

fn metafield_definition_capability_input_error(
    input: &BTreeMap<String, ResolvedValue>,
    typename: &str,
    field: Value,
    owner_type: &str,
    metafield_type: &str,
) -> Option<Value> {
    let capabilities = resolved_object_field(input, "capabilities")?;
    for (key, capability_name) in [
        ("adminFilterable", "admin_filterable"),
        ("smartCollectionCondition", "smart_collection_condition"),
        ("uniqueValues", "unique_values"),
    ] {
        let Some(capability) = resolved_object_field(&capabilities, key) else {
            continue;
        };
        if resolved_bool_field(&capability, "enabled") != Some(true) {
            continue;
        }
        if !metafield_definition_capability_eligible(key, owner_type, metafield_type) {
            return Some(metafield_definition_user_error(
                typename,
                field,
                &format!("The capability {capability_name} is not valid for this definition."),
                "INVALID_CAPABILITY",
            ));
        }
    }
    None
}

fn metafield_definition_capability_eligible(
    capability: &str,
    owner_type: &str,
    metafield_type: &str,
) -> bool {
    match capability {
        "adminFilterable" => {
            metafield_definition_admin_filterable_eligible(owner_type, metafield_type)
        }
        "smartCollectionCondition" => {
            owner_type == "PRODUCT" && metafield_type == "single_line_text_field"
        }
        "uniqueValues" => matches!(
            metafield_type,
            "id" | "number_integer" | "single_line_text_field" | "url"
        ),
        _ => false,
    }
}

fn metafield_definition_admin_filterable_eligible(owner_type: &str, metafield_type: &str) -> bool {
    match owner_type {
        "PRODUCT" => matches!(
            metafield_type,
            "boolean"
                | "number_integer"
                | "single_line_text_field"
                | "id"
                | "url"
                | "product_reference"
                | "collection_reference"
                | "variant_reference"
                | "metaobject_reference"
        ),
        "PRODUCTVARIANT" | "COLLECTION" | "CUSTOMER" | "ORDER" | "COMPANY" => {
            matches!(
                metafield_type,
                "boolean" | "number_integer" | "single_line_text_field" | "id" | "url"
            )
        }
        _ => false,
    }
}

fn metafield_definition_capability_explicitly_disabled(
    input: &BTreeMap<String, ResolvedValue>,
    key: &str,
) -> bool {
    resolved_object_field(input, "capabilities")
        .and_then(|capabilities| resolved_object_field(&capabilities, key))
        .and_then(|capability| resolved_bool_field(&capability, "enabled"))
        == Some(false)
}

fn metafield_definition_capabilities_will_enable_admin_filterable(
    input: &BTreeMap<String, ResolvedValue>,
    existing: Option<&Value>,
) -> bool {
    let Some(capabilities) = resolved_object_field(input, "capabilities") else {
        return false;
    };
    let Some(admin_filterable) = resolved_object_field(&capabilities, "adminFilterable") else {
        return false;
    };
    match resolved_bool_field(&admin_filterable, "enabled") {
        Some(true) => true,
        Some(false) => false,
        None => existing.is_some_and(|definition| {
            definition["capabilities"]["adminFilterable"]["enabled"]
                .as_bool()
                .unwrap_or(false)
        }),
    }
}

fn apply_metafield_definition_capability_input(
    definition: &mut Value,
    capabilities: &BTreeMap<String, ResolvedValue>,
) {
    for key in [
        "adminFilterable",
        "smartCollectionCondition",
        "uniqueValues",
    ] {
        let Some(capability) = resolved_object_field(capabilities, key) else {
            continue;
        };
        if let Some(enabled) = resolved_bool_field(&capability, "enabled") {
            definition["capabilities"][key]["enabled"] = json!(enabled);
        }
    }
}

fn apply_metafield_definition_capability_derived_fields(definition: &mut Value) {
    let owner_type = definition["ownerType"]
        .as_str()
        .unwrap_or("PRODUCT")
        .to_string();
    let metafield_type = definition["type"]["name"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    for key in [
        "adminFilterable",
        "smartCollectionCondition",
        "uniqueValues",
    ] {
        let eligible = metafield_definition_capability_eligible(key, &owner_type, &metafield_type);
        definition["capabilities"][key]["eligible"] = json!(eligible);
        if !eligible {
            definition["capabilities"][key]["enabled"] = json!(false);
        }
    }
    let admin_filterable_enabled = definition["capabilities"]["adminFilterable"]["enabled"]
        .as_bool()
        .unwrap_or(false);
    definition["capabilities"]["adminFilterable"]["status"] = if admin_filterable_enabled {
        json!("FILTERABLE")
    } else {
        json!("NOT_FILTERABLE")
    };
}

fn metafield_definition_validations(input: &BTreeMap<String, ResolvedValue>) -> Value {
    Value::Array(
        list_object_field(input, "validations")
            .into_iter()
            .filter_map(|validation| {
                Some(json!({
                    "name": resolved_string_field(&validation, "name")?,
                    "value": resolved_string_field(&validation, "value").unwrap_or_default()
                }))
            })
            .collect(),
    )
}

fn metafield_definition_access(input: &BTreeMap<String, ResolvedValue>) -> Value {
    let admin = match resolved_string_field(input, "admin").as_deref() {
        Some("MERCHANT_READ_WRITE") | Some("PUBLIC_READ_WRITE") => "PUBLIC_READ_WRITE".to_string(),
        Some("MERCHANT_READ") => "MERCHANT_READ".to_string(),
        Some(value) => value.to_string(),
        None => "PUBLIC_READ_WRITE".to_string(),
    };
    json!({
        "admin": admin,
        "storefront": resolved_string_field(input, "storefront").unwrap_or_else(|| "NONE".to_string()),
        "customerAccount": resolved_string_field(input, "customerAccount").unwrap_or_else(|| "NONE".to_string())
    })
}

fn metafield_definition_constraints(input: &BTreeMap<String, ResolvedValue>) -> Value {
    let key = match input.get("key") {
        Some(ResolvedValue::String(value)) => json!(value),
        _ => Value::Null,
    };
    let nodes = match input.get("values") {
        Some(ResolvedValue::List(values)) => values
            .iter()
            .filter_map(resolved_value_string)
            .map(|value| json!({"value": metafield_definition_constraint_value(&value)}))
            .collect::<Vec<_>>(),
        _ => Vec::new(),
    };
    json!({
        "key": key,
        "values": {"nodes": nodes, "pageInfo": empty_page_info()}
    })
}

fn metafield_definition_constraints_for_key(key: &str) -> Value {
    json!({
        "key": key,
        "values": {"nodes": [], "pageInfo": empty_page_info()}
    })
}

fn constraints_empty_values_error(
    input: &BTreeMap<String, ResolvedValue>,
    typename: &str,
) -> Option<Value> {
    for field in ["constraintsUpdates", "constraintsSet"] {
        let Some(constraints) = resolved_object_field(input, field) else {
            continue;
        };
        if constraints
            .get("key")
            .is_some_and(|value| !matches!(value, ResolvedValue::Null))
            && matches!(constraints.get("values"), Some(ResolvedValue::List(values)) if values.is_empty())
        {
            return Some(metafield_definition_user_error(
                typename,
                json!(["definition"]),
                "Cannot change the constraint key without providing values.",
                "INVALID_INPUT",
            ));
        }
    }
    None
}

fn apply_metafield_definition_constraints_update(
    definition: &mut Value,
    input: &BTreeMap<String, ResolvedValue>,
) {
    if let Some(constraints) = resolved_object_field(input, "constraints") {
        definition["constraints"] = metafield_definition_constraints(&constraints);
    }
    let Some(constraints) = resolved_object_field(input, "constraintsUpdates")
        .or_else(|| resolved_object_field(input, "constraintsSet"))
    else {
        return;
    };
    let current_key = definition["constraints"]["key"].clone();
    let next_key = match constraints.get("key") {
        Some(ResolvedValue::String(value)) => json!(value),
        Some(ResolvedValue::Null) => Value::Null,
        _ => current_key,
    };
    let mut values = definition["constraints"]["values"]["nodes"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|node| {
            node.get("value")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect::<Vec<_>>();
    if let Some(ResolvedValue::List(updates)) = constraints.get("values") {
        if next_key.is_null() && updates.is_empty() {
            values.clear();
        }
        for update in updates {
            match update {
                ResolvedValue::Object(object) => {
                    if let Some(value) = resolved_string_field(object, "delete") {
                        let value = metafield_definition_constraint_value(&value);
                        values.retain(|existing| existing != &value);
                    }
                    if let Some(value) = resolved_string_field(object, "create") {
                        let value = metafield_definition_constraint_value(&value);
                        if !values.contains(&value) {
                            values.push(value);
                        }
                    }
                }
                ResolvedValue::String(value) => {
                    let value = metafield_definition_constraint_value(value);
                    if !values.contains(&value) {
                        values.push(value);
                    }
                }
                _ => {}
            }
        }
    }
    definition["constraints"] = json!({
        "key": next_key,
        "values": {
            "nodes": values.into_iter().map(|value| json!({"value": value})).collect::<Vec<_>>(),
            "pageInfo": empty_page_info()
        }
    });
}

fn metafield_definition_constraint_value(value: &str) -> String {
    if value.starts_with("gid://shopify/TaxonomyCategory/") {
        resource_id_tail(value).to_string()
    } else {
        value.to_string()
    }
}

fn metafield_definition_has_constraints(definition: &Value) -> bool {
    !definition["constraints"]["key"].is_null()
        || definition["constraints"]["values"]["nodes"]
            .as_array()
            .is_some_and(|nodes| !nodes.is_empty())
}

fn remove_associated_metafields(
    owner_metafields: &mut BTreeMap<String, Vec<Value>>,
    namespace: &str,
    key: &str,
) {
    for metafields in owner_metafields.values_mut() {
        metafields.retain(|metafield| {
            metafield.get("namespace").and_then(Value::as_str) != Some(namespace)
                || metafield.get("key").and_then(Value::as_str) != Some(key)
        });
    }
}

fn standard_metafield_definition_template_by_selector(
    id: Option<&str>,
    namespace: Option<&str>,
    key: Option<&str>,
    owner_type: &str,
) -> Result<StandardMetafieldDefinitionTemplate, Value> {
    if id.is_none() && (namespace.is_none() || key.is_none()) {
        return Err(metafield_definition_user_error(
            "StandardMetafieldDefinitionEnableUserError",
            Value::Null,
            "A namespace and key or standard metafield definition template id must be provided.",
            "TEMPLATE_NOT_FOUND",
        ));
    }
    let template = if let Some(id) = id {
        STANDARD_METAFIELD_DEFINITION_TEMPLATES
            .iter()
            .find(|template| template.id == id)
    } else {
        STANDARD_METAFIELD_DEFINITION_TEMPLATES
            .iter()
            .find(|template| Some(template.namespace) == namespace && Some(template.key) == key)
    };
    let Some(template) = template else {
        let (field, message) = if id.is_some() {
            (
                json!(["id"]),
                "Id is not a valid standard metafield definition template id",
            )
        } else {
            (
                Value::Null,
                "A standard definition wasn't found for the specified owner type, namespace, and key.",
            )
        };
        return Err(metafield_definition_user_error(
            "StandardMetafieldDefinitionEnableUserError",
            field,
            message,
            "TEMPLATE_NOT_FOUND",
        ));
    };
    if !template.owner_types.contains(&owner_type) {
        return Err(metafield_definition_user_error(
            "StandardMetafieldDefinitionEnableUserError",
            json!(["id"]),
            "Id is not a valid standard metafield definition template id",
            "TEMPLATE_NOT_FOUND",
        ));
    }
    Ok(*template)
}

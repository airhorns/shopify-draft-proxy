use super::*;

fn metaobject_create_duplicate_field_errors(input: &BTreeMap<String, ResolvedValue>) -> Vec<Value> {
    let mut seen = BTreeSet::new();
    let mut errors = Vec::new();
    if let Some(ResolvedValue::List(fields)) = input.get("fields") {
        for (index, field) in fields.iter().enumerate() {
            let ResolvedValue::Object(field) = field else {
                continue;
            };
            let Some(key) = resolved_string_field(field, "key") else {
                continue;
            };
            if seen.insert(key.clone()) {
                continue;
            }

            let field_index = index.to_string();
            let is_required_title = key == "title";
            errors.push(json!({
                "field": ["metaobject", "fields", field_index.clone()],
                "message": format!("Field \"{key}\" duplicates other inputs"),
                "code": "DUPLICATE_FIELD_INPUT",
                "elementKey": key.clone(),
                "elementIndex": null
            }));
            if is_required_title {
                errors.push(json!({
                    "field": ["metaobject", "fields", field_index],
                    "message": "Title can't be blank",
                    "code": "OBJECT_FIELD_REQUIRED",
                    "elementKey": key,
                    "elementIndex": null
                }));
            }
        }
    }
    errors
}

fn metaobject_field_record_from_definition(
    field_definition: &Value,
    value: Option<&String>,
) -> Value {
    let field_type = field_definition["type"]["name"]
        .as_str()
        .unwrap_or("single_line_text_field");
    let value = value.map(String::as_str).unwrap_or_default();
    json!({
        "key": field_definition.get("key").cloned().unwrap_or(Value::Null),
        "type": field_type,
        "value": value,
        "jsonValue": metaobject_field_json_value(field_type, Some(value)),
        "definition": field_definition
    })
}

fn metaobject_field_name(key: &str) -> String {
    key.split(['_', '-'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            let Some(first) = chars.next() else {
                return String::new();
            };
            format!("{}{}", first.to_uppercase(), chars.as_str())
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn metaobject_definition_record(
    id: &str,
    input: &BTreeMap<String, ResolvedValue>,
    meta_type: &str,
) -> Value {
    let name = resolved_string_field(input, "name").unwrap_or_else(|| meta_type.to_string());
    let display_name_key = resolved_string_field(input, "displayNameKey");
    let field_definitions = resolved_object_list_field(input, "fieldDefinitions")
        .into_iter()
        .map(metaobject_field_definition_record)
        .collect::<Vec<_>>();
    json!({
        "id": id,
        "type": meta_type,
        "name": name,
        "description": input.get("description").and_then(resolved_value_string).map_or(Value::Null, |description| json!(description)),
        "displayNameKey": display_name_key,
        "access": {"admin": "PUBLIC_READ_WRITE", "storefront": "NONE", "customerAccount": "NONE"},
        "capabilities": metaobject_definition_capabilities(input),
        "fieldDefinitions": field_definitions,
        "hasThumbnailField": false,
        "metaobjectsCount": 0,
        "standardTemplate": Value::Null,
        "createdAt": "2024-01-01T00:00:00.000Z",
        "updatedAt": "2024-01-01T00:00:00.000Z"
    })
}

fn metaobject_definition_capabilities(input: &BTreeMap<String, ResolvedValue>) -> Value {
    let capabilities = resolved_object_field(input, "capabilities").unwrap_or_default();
    let publishable = resolved_object_field(&capabilities, "publishable")
        .and_then(|publishable| resolved_bool_field(&publishable, "enabled"))
        .unwrap_or(false);
    let online_store = resolved_object_field(&capabilities, "onlineStore")
        .and_then(|online_store| resolved_bool_field(&online_store, "enabled"))
        .unwrap_or(false);
    let renderable = resolved_object_field(&capabilities, "renderable")
        .and_then(|renderable| resolved_bool_field(&renderable, "enabled"))
        .unwrap_or(false);
    let translatable = resolved_object_field(&capabilities, "translatable")
        .and_then(|translatable| resolved_bool_field(&translatable, "enabled"))
        .unwrap_or(false);
    json!({
        "publishable": {"enabled": publishable},
        "onlineStore": {"enabled": online_store, "data": Value::Null},
        "renderable": {"enabled": renderable},
        "translatable": {"enabled": translatable}
    })
}

fn metaobject_field_definition_record(input: BTreeMap<String, ResolvedValue>) -> Value {
    let key = resolved_string_field(&input, "key").unwrap_or_default();
    let name = resolved_string_field(&input, "name").unwrap_or_else(|| metaobject_field_name(&key));
    let field_type = metaobject_field_definition_type(&input);
    json!({
        "key": key,
        "name": name,
        "description": input.get("description").and_then(resolved_value_string).map_or(Value::Null, |description| json!(description)),
        "required": resolved_bool_field(&input, "required").unwrap_or(false),
        "type": {"name": field_type, "category": metaobject_field_type_category(&field_type)},
        "validations": resolved_object_list_field(&input, "validations")
            .into_iter()
            .map(|validation| {
                json!({
                    "name": resolved_string_field(&validation, "name").unwrap_or_default(),
                    "value": resolved_string_field(&validation, "value").unwrap_or_default()
                })
            })
            .collect::<Vec<_>>()
    })
}

fn metaobject_field_definition_type(input: &BTreeMap<String, ResolvedValue>) -> String {
    match input.get("type") {
        Some(ResolvedValue::String(value)) => value.clone(),
        Some(ResolvedValue::Object(value)) => resolved_string_field(value, "name")
            .unwrap_or_else(|| "single_line_text_field".to_string()),
        _ => "single_line_text_field".to_string(),
    }
}

fn metaobject_field_type_category(field_type: &str) -> &'static str {
    match field_type {
        "number_integer" | "number_decimal" => "NUMBER",
        "boolean" => "TRUE_FALSE",
        "date" | "date_time" => "DATE_TIME",
        "json" | "rich_text_field" => "JSON",
        value if value.ends_with("_reference") || value.starts_with("list.") => "REFERENCE",
        _ => "TEXT",
    }
}

fn metaobject_field_json_value(field_type: &str, value: Option<&str>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    match field_type {
        "number_integer" => value
            .parse::<i64>()
            .map_or(Value::Null, |number| json!(number)),
        "number_decimal" => value
            .parse::<f64>()
            .map_or(Value::Null, |number| json!(number)),
        "boolean" => match value {
            "true" => json!(true),
            "false" => json!(false),
            _ => Value::Null,
        },
        "json" | "rich_text_field" => serde_json::from_str(value).unwrap_or_else(|_| json!(value)),
        value_type if value_type.starts_with("list.") => {
            serde_json::from_str(value).unwrap_or_else(|_| json!([value]))
        }
        _ => json!(value),
    }
}

fn metaobject_value_matches_type(field_type: &str, value: &str) -> bool {
    match field_type {
        "number_integer" => value.parse::<i64>().is_ok(),
        "number_decimal" => value.parse::<f64>().is_ok(),
        "boolean" => matches!(value, "true" | "false"),
        "json" | "rich_text_field" => serde_json::from_str::<Value>(value).is_ok(),
        value_type if value_type.starts_with("list.") => serde_json::from_str::<Value>(value)
            .ok()
            .is_some_and(|value| value.is_array()),
        _ => true,
    }
}

fn metaobject_create_input_values(
    input: &BTreeMap<String, ResolvedValue>,
) -> BTreeMap<String, String> {
    let mut values = BTreeMap::new();
    if let Some(ResolvedValue::List(fields)) = input.get("fields") {
        for field in fields {
            if let ResolvedValue::Object(field) = field {
                if let (Some(key), Some(value)) = (
                    resolved_string_field(field, "key"),
                    resolved_string_field(field, "value"),
                ) {
                    values.insert(key, value);
                }
            }
        }
    }
    if let Some(ResolvedValue::Object(object)) = input.get("values") {
        for (key, value) in object {
            match value {
                ResolvedValue::String(value) => {
                    values.insert(key.clone(), value.clone());
                }
                ResolvedValue::Null => {
                    values.insert(key.clone(), String::new());
                }
                _ => {
                    values.insert(key.clone(), resolved_value_json(value).to_string());
                }
            }
        }
    }
    values
}

fn metaobject_create_validation_errors(
    input: &BTreeMap<String, ResolvedValue>,
    definition: &Value,
    input_values: &BTreeMap<String, String>,
) -> Vec<Value> {
    let mut errors = metaobject_create_duplicate_field_errors(input);
    let definition_keys = definition["fieldDefinitions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|field| field.get("key").and_then(Value::as_str))
        .collect::<BTreeSet<_>>();

    if let Some(ResolvedValue::List(fields)) = input.get("fields") {
        for (index, field) in fields.iter().enumerate() {
            let ResolvedValue::Object(field) = field else {
                continue;
            };
            let key = resolved_string_field(field, "key").unwrap_or_default();
            if !definition_keys.contains(key.as_str()) {
                errors.push(metaobject_user_error(
                    vec!["metaobject", "fields", &index.to_string()],
                    &format!("Field key \"{key}\" is not defined on this metaobject definition."),
                    "UNDEFINED_OBJECT_FIELD",
                    json!(key),
                    json!(index),
                ));
            } else if let Some(field_definition) = definition["fieldDefinitions"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|definition| {
                    definition.get("key").and_then(Value::as_str) == Some(key.as_str())
                })
            {
                let value = resolved_string_field(field, "value").unwrap_or_default();
                if !metaobject_value_matches_type(
                    field_definition["type"]["name"]
                        .as_str()
                        .unwrap_or_default(),
                    &value,
                ) {
                    errors.push(metaobject_user_error(
                        vec!["metaobject", "fields", &index.to_string()],
                        &format!("Value is invalid for field \"{key}\"."),
                        "INVALID_VALUE",
                        json!(key),
                        json!(index),
                    ));
                }
            }
        }
    }

    for key in input_values.keys() {
        if !definition_keys.contains(key.as_str()) {
            errors.push(metaobject_user_error(
                vec!["metaobject", "values", key],
                &format!("Field key \"{key}\" is not defined on this metaobject definition."),
                "UNDEFINED_OBJECT_FIELD",
                json!(key),
                Value::Null,
            ));
        }
    }

    for field_definition in definition["fieldDefinitions"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let key = field_definition
            .get("key")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if field_definition
            .get("required")
            .and_then(Value::as_bool)
            .unwrap_or(false)
            && input_values
                .get(key)
                .is_none_or(|value| value.trim().is_empty())
        {
            errors.push(metaobject_user_error(
                vec!["metaobject", "fields"],
                &format!("Field \"{key}\" is required."),
                "OBJECT_FIELD_REQUIRED",
                json!(key),
                Value::Null,
            ));
        }
    }

    if let Some(capabilities) = resolved_object_field(input, "capabilities") {
        for key in capabilities.keys() {
            let enabled = definition["capabilities"][key]["enabled"]
                .as_bool()
                .unwrap_or(false);
            if !enabled {
                errors.push(metaobject_user_error(
                    vec!["metaobject", "capabilities", key],
                    "Capability is not enabled for this metaobject definition.",
                    "CAPABILITY_NOT_ENABLED",
                    json!(key),
                    Value::Null,
                ));
            }
        }
    }

    errors
}

fn metaobject_user_error(
    field: Vec<&str>,
    message: &str,
    code: &str,
    element_key: Value,
    element_index: Value,
) -> Value {
    json!({
        "field": field,
        "message": message,
        "code": code,
        "elementKey": element_key,
        "elementIndex": element_index
    })
}

fn metaobject_display_name(definition: &Value, input_values: &BTreeMap<String, String>) -> String {
    definition
        .get("displayNameKey")
        .and_then(Value::as_str)
        .and_then(|key| input_values.get(key))
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .or_else(|| {
            input_values
                .values()
                .find(|value| !value.trim().is_empty())
                .cloned()
        })
        .unwrap_or_else(|| {
            definition["type"]
                .as_str()
                .unwrap_or("Metaobject")
                .to_string()
        })
}

fn metaobject_publishable_status(
    input: &BTreeMap<String, ResolvedValue>,
    definition: &Value,
) -> String {
    let publishable_enabled = definition["capabilities"]["publishable"]["enabled"]
        .as_bool()
        .unwrap_or(false);
    resolved_object_field(input, "capabilities")
        .and_then(|capabilities| resolved_object_field(&capabilities, "publishable"))
        .and_then(|publishable| resolved_string_field(&publishable, "status"))
        .unwrap_or_else(|| {
            if publishable_enabled {
                "DRAFT".to_string()
            } else {
                "ACTIVE".to_string()
            }
        })
}

fn metaobject_record_from_definition(
    id: &str,
    handle: &str,
    definition: &Value,
    input_values: &BTreeMap<String, String>,
    display_name: &str,
    publishable_status: &str,
) -> Value {
    let meta_type = definition["type"].as_str().unwrap_or_default();
    let fields = definition["fieldDefinitions"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|field_definition| {
            let key = field_definition
                .get("key")
                .and_then(Value::as_str)
                .unwrap_or_default();
            metaobject_field_record_from_definition(field_definition, input_values.get(key))
        })
        .collect::<Vec<_>>();
    let title_field = definition["displayNameKey"]
        .as_str()
        .and_then(|key| {
            fields
                .iter()
                .find(|field| field.get("key").and_then(Value::as_str) == Some(key))
                .cloned()
        })
        .or_else(|| fields.first().cloned());
    json!({
        "id": id,
        "handle": handle,
        "type": meta_type,
        "displayName": display_name,
        "updatedAt": "2026-01-01T00:00:00Z",
        "capabilities": {
            "publishable": {"status": publishable_status},
            "onlineStore": if definition["capabilities"]["onlineStore"]["enabled"].as_bool().unwrap_or(false) {
                json!({"templateSuffix": Value::Null})
            } else {
                Value::Null
            }
        },
        "fields": fields,
        "titleField": title_field
    })
}

fn selected_metaobject_value(value: &Value, selection: &[SelectedField]) -> Value {
    if let Some(values) = value.as_array() {
        Value::Array(
            values
                .iter()
                .map(|item| selected_json(item, selection))
                .collect(),
        )
    } else {
        nullable_selected_json(value, selection)
    }
}

fn metaobject_nodes_from_upstream_data(data: &serde_json::Map<String, Value>) -> Vec<Value> {
    let mut nodes = Vec::new();
    for value in data.values() {
        if let Some(connection_nodes) = value.get("nodes").and_then(Value::as_array) {
            nodes.extend(
                connection_nodes
                    .iter()
                    .filter(|node| node.is_object())
                    .cloned(),
            );
        }
        if let Some(edges) = value.get("edges").and_then(Value::as_array) {
            nodes.extend(
                edges
                    .iter()
                    .filter_map(|edge| edge.get("node").filter(|node| node.is_object()).cloned()),
            );
        }
    }
    nodes
}

pub(in crate::proxy) fn metaobject_cursor(record: &Value) -> String {
    format!(
        "cursor:{}",
        record
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("metaobject")
    )
}

impl DraftProxy {
    pub(in crate::proxy) fn has_local_metaobject_entry_state(&self) -> bool {
        !self.store.staged.metaobjects.is_empty()
            || !self.store.staged.deleted_metaobject_ids.is_empty()
    }

    pub(in crate::proxy) fn metaobject_query_data(&self, fields: &[RootFieldSelection]) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "metaobjects" => self.metaobject_connection(field),
                "metaobject" => {
                    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    self.metaobject_by_id(&id)
                        .map(|record| self.selected_metaobject(&record, &field.selection))
                        .unwrap_or(Value::Null)
                }
                "metaobjectByHandle" => self.metaobject_by_handle_arg(field).unwrap_or(Value::Null),
                "metaobjectDefinition" => {
                    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    self.metaobject_definition_by_id(&id)
                        .map(|definition| selected_json(&definition, &field.selection))
                        .unwrap_or(Value::Null)
                }
                "metaobjectDefinitionByType" => {
                    let meta_type =
                        resolved_string_arg(&field.arguments, "type").unwrap_or_default();
                    self.metaobject_definition_by_type(&meta_type)
                        .map(|definition| selected_json(&definition, &field.selection))
                        .unwrap_or(Value::Null)
                }
                "metaobjectDefinitions" => self.metaobject_definition_connection(field),
                _ => Value::Null,
            };
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    pub(in crate::proxy) fn metaobject_live_hybrid_read(
        &mut self,
        request: &Request,
        fields: &[RootFieldSelection],
    ) -> Response {
        let mut response = (self.upstream_transport)(request.clone());
        let Some(data) = response.body.get_mut("data").and_then(Value::as_object_mut) else {
            return response;
        };
        for field in fields {
            if data.contains_key(&field.response_key) {
                continue;
            }
            if let Some(value) = data.get(&field.name).cloned() {
                data.insert(field.response_key.clone(), value);
            }
        }
        let upstream_nodes = metaobject_nodes_from_upstream_data(data);
        for field in fields {
            if data.contains_key(&field.response_key) {
                continue;
            }
            let value = match field.name.as_str() {
                "metaobject" => {
                    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    upstream_nodes
                        .iter()
                        .find(|node| node.get("id").and_then(Value::as_str) == Some(&id))
                        .cloned()
                        .unwrap_or(Value::Null)
                }
                "metaobjectByHandle" => {
                    let Some(ResolvedValue::Object(handle)) = field.arguments.get("handle") else {
                        continue;
                    };
                    let meta_type = resolved_string_field(handle, "type").unwrap_or_default();
                    let meta_handle = resolved_string_field(handle, "handle").unwrap_or_default();
                    upstream_nodes
                        .iter()
                        .find(|node| {
                            node.get("type").and_then(Value::as_str) == Some(meta_type.as_str())
                                && node.get("handle").and_then(Value::as_str)
                                    == Some(meta_handle.as_str())
                        })
                        .cloned()
                        .unwrap_or(Value::Null)
                }
                _ => continue,
            };
            data.insert(field.response_key.clone(), value);
        }
        response
    }

    pub(in crate::proxy) fn metaobject_mutation(
        &mut self,
        fields: &[RootFieldSelection],
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let mut data = serde_json::Map::new();
        let mut staged_ids = Vec::new();
        for field in fields {
            let value = match field.name.as_str() {
                "metaobjectCreate" => self.metaobject_create(field, request, &mut staged_ids),
                "metaobjectDelete" => self.metaobject_delete(field, request, &mut staged_ids),
                "metaobjectDefinitionCreate" => {
                    self.metaobject_definition_create(field, &mut staged_ids)
                }
                "metaobjectDefinitionDelete" => {
                    self.metaobject_definition_delete(field, &mut staged_ids)
                }
                _ => Value::Null,
            };
            data.insert(field.response_key.clone(), value);
        }
        if !staged_ids.is_empty() {
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                fields
                    .first()
                    .map(|f| f.name.as_str())
                    .unwrap_or("metaobject"),
                staged_ids,
            );
        }
        ok_json(json!({"data": Value::Object(data)}))
    }

    pub(in crate::proxy) fn metaobject_by_id(&self, id: &str) -> Option<Value> {
        if self.store.staged.deleted_metaobject_ids.contains(id) {
            return None;
        }
        if let Some(record) = self.store.staged.metaobjects.get(id) {
            return Some(record.clone());
        }
        None
    }

    fn hydrate_metaobject_by_id(&mut self, request: &Request, id: &str) -> Option<Value> {
        if self.config.read_mode == ReadMode::Snapshot || id.is_empty() {
            return None;
        }
        let hydrate_request = Request {
            method: "POST".to_string(),
            path: request.path.clone(),
            headers: request.headers.clone(),
            body: json!({
                "query": r#"
                    query MetaobjectHydrateById($id: ID!) {
                      node(id: $id) { __typename }
                      metaobject(id: $id) {
                        id
                        handle
                        type
                        displayName
                        updatedAt
                        capabilities {
                          publishable { status }
                          onlineStore { templateSuffix }
                        }
                        fields {
                          key
                          type
                          value
                          jsonValue
                          definition {
                            key
                            name
                            required
                            type { name category }
                          }
                        }
                        titleField: field(key: "title") {
                          key
                          type
                          value
                          jsonValue
                          definition {
                            key
                            name
                            required
                            type { name category }
                          }
                        }
                      }
                    }
                "#,
                "variables": {"id": id}
            })
            .to_string(),
        };
        let response = (self.upstream_transport)(hydrate_request);
        let record = response.body["data"]["metaobject"].clone();
        if !record.is_object() {
            return None;
        }
        self.store
            .staged
            .metaobjects
            .insert(id.to_string(), record.clone());
        Some(record)
    }

    pub(in crate::proxy) fn metaobject_by_handle_arg(
        &self,
        field: &RootFieldSelection,
    ) -> Option<Value> {
        let Some(ResolvedValue::Object(handle)) = field.arguments.get("handle") else {
            return None;
        };
        let meta_type = resolved_string_field(handle, "type").unwrap_or_default();
        let meta_handle = resolved_string_field(handle, "handle").unwrap_or_default();
        self.metaobject_by_type_and_handle(&meta_type, &meta_handle)
            .map(|record| self.selected_metaobject(&record, &field.selection))
    }

    pub(in crate::proxy) fn metaobject_by_type_and_handle(
        &self,
        meta_type: &str,
        meta_handle: &str,
    ) -> Option<Value> {
        self.store
            .staged
            .metaobjects
            .values()
            .find(|record| {
                record.get("type").and_then(Value::as_str) == Some(meta_type)
                    && record.get("handle").and_then(Value::as_str) == Some(meta_handle)
                    && !self
                        .store
                        .staged
                        .deleted_metaobject_ids
                        .contains(record.get("id").and_then(Value::as_str).unwrap_or_default())
            })
            .cloned()
    }

    pub(in crate::proxy) fn metaobject_connection(&self, field: &RootFieldSelection) -> Value {
        let meta_type = resolved_string_arg(&field.arguments, "type").unwrap_or_default();
        let mut records: Vec<Value> = self
            .store
            .staged
            .metaobjects
            .values()
            .filter(|record| {
                record.get("type").and_then(Value::as_str) == Some(meta_type.as_str())
                    && !self
                        .store
                        .staged
                        .deleted_metaobject_ids
                        .contains(record.get("id").and_then(Value::as_str).unwrap_or_default())
            })
            .cloned()
            .collect();
        records.sort_by(|left, right| {
            left.get("id")
                .and_then(Value::as_str)
                .cmp(&right.get("id").and_then(Value::as_str))
        });
        selected_typed_connection_with_args(
            &records,
            &field.arguments,
            &field.selection,
            |record, selection| self.selected_metaobject(record, selection),
            metaobject_cursor,
        )
    }

    pub(in crate::proxy) fn metaobject_create(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let input = match field.arguments.get("metaobject") {
            Some(ResolvedValue::Object(input)) => input,
            _ => {
                return self.selected_metaobject_payload(
                    &json!({"metaobject": null, "userErrors": []}),
                    &field.selection,
                )
            }
        };
        let meta_type = resolved_string_field(input, "type").unwrap_or_default();
        let definition = self
            .metaobject_definition_by_type(&meta_type)
            .or_else(|| self.hydrate_metaobject_definition_by_type(request, &meta_type));
        let Some(definition) = definition else {
            let user_errors = metaobject_create_duplicate_field_errors(input);
            if !user_errors.is_empty() {
                return self.selected_metaobject_payload(
                    &json!({"metaobject": null, "userErrors": user_errors}),
                    &field.selection,
                );
            }
            return self.selected_metaobject_payload(
                &json!({
                    "metaobject": null,
                    "userErrors": [metaobject_user_error(
                        vec!["metaobject", "type"],
                        &format!("No metaobject definition exists for type \"{meta_type}\""),
                        "UNDEFINED_OBJECT_TYPE",
                        Value::Null,
                        Value::Null
                    )]
                }),
                &field.selection,
            );
        };
        if definition["access"]["admin"].as_str() == Some("MERCHANT_READ") {
            return self.selected_metaobject_payload(
                &json!({
                    "metaobject": null,
                    "userErrors": [metaobject_user_error(
                        vec!["metaobject", "type"],
                        "Not authorized to create metaobjects for this type.",
                        "NOT_AUTHORIZED",
                        Value::Null,
                        Value::Null
                    )]
                }),
                &field.selection,
            );
        }
        let input_values = metaobject_create_input_values(input);
        let validation_errors =
            metaobject_create_validation_errors(input, &definition, &input_values);
        if !validation_errors.is_empty() {
            return self.selected_metaobject_payload(
                &json!({"metaobject": null, "userErrors": validation_errors}),
                &field.selection,
            );
        }

        let id = self.next_proxy_synthetic_gid("Metaobject");
        let display_name = metaobject_display_name(&definition, &input_values);
        let fallback_handle = if display_name.trim().is_empty() {
            format!("{}-{}", slugify_handle(&meta_type), resource_id_tail(&id))
        } else {
            slugify_handle(&display_name)
        };
        let requested_handle = resolved_string_field(input, "handle").unwrap_or(fallback_handle);
        let handle = self.available_metaobject_handle(&meta_type, &requested_handle);
        let publishable_status = metaobject_publishable_status(input, &definition);
        let record = metaobject_record_from_definition(
            &id,
            &handle,
            &definition,
            &input_values,
            &display_name,
            &publishable_status,
        );
        self.store.staged.deleted_metaobject_ids.remove(&id);
        self.store
            .staged
            .metaobjects
            .insert(id.clone(), record.clone());
        self.increment_metaobject_definition_count(&meta_type, 1);
        staged_ids.push(id);
        self.selected_metaobject_payload(
            &json!({"metaobject": record, "userErrors": []}),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn metaobject_delete(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        if self.metaobject_by_id(&id).is_none()
            && self.hydrate_metaobject_by_id(request, &id).is_none()
        {
            return selected_json(
                &json!({
                    "deletedId": null,
                    "userErrors": [{
                        "field": ["id"],
                        "message": "Record not found",
                        "code": "RECORD_NOT_FOUND",
                        "elementKey": null,
                        "elementIndex": null
                    }]
                }),
                &field.selection,
            );
        }
        let record = self.metaobject_by_id(&id).unwrap_or(Value::Null);
        self.store.staged.metaobjects.remove(&id);
        self.store.staged.deleted_metaobject_ids.insert(id.clone());
        if let Some(meta_type) = record.get("type").and_then(Value::as_str) {
            self.increment_metaobject_definition_count(meta_type, -1);
        }
        staged_ids.push(id.clone());
        selected_json(
            &json!({"deletedId": id, "userErrors": []}),
            &field.selection,
        )
    }

    fn selected_metaobject(&self, record: &Value, selection: &[SelectedField]) -> Value {
        selected_payload_json(selection, |field| match field.name.as_str() {
            "field" => {
                let key = resolved_string_arg(&field.arguments, "key").unwrap_or_default();
                let value = record["fields"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|candidate| {
                        candidate.get("key").and_then(Value::as_str) == Some(key.as_str())
                    })
                    .cloned()
                    .unwrap_or(Value::Null);
                Some(nullable_selected_json(&value, &field.selection))
            }
            "definition" => {
                let meta_type = record
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                Some(
                    self.metaobject_definition_by_type(meta_type)
                        .map(|definition| selected_json(&definition, &field.selection))
                        .unwrap_or(Value::Null),
                )
            }
            _ => record
                .get(&field.name)
                .map(|value| selected_metaobject_value(value, &field.selection)),
        })
    }

    fn selected_metaobject_payload(&self, payload: &Value, selection: &[SelectedField]) -> Value {
        selected_payload_json(selection, |field| match field.name.as_str() {
            "metaobject" => {
                let metaobject = &payload["metaobject"];
                Some(if metaobject.is_null() {
                    Value::Null
                } else {
                    self.selected_metaobject(metaobject, &field.selection)
                })
            }
            _ => payload
                .get(&field.name)
                .map(|value| selected_metaobject_value(value, &field.selection)),
        })
    }

    fn metaobject_definition_create(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let definition_input = match field.arguments.get("definition") {
            Some(ResolvedValue::Object(input)) => input,
            _ => {
                return selected_json(
                    &json!({"metaobjectDefinition": null, "userErrors": []}),
                    &field.selection,
                )
            }
        };
        let meta_type = resolved_string_field(definition_input, "type")
            .unwrap_or_default()
            .to_lowercase();
        if meta_type.is_empty() {
            return selected_json(
                &json!({
                    "metaobjectDefinition": null,
                    "userErrors": [metaobject_user_error(vec!["definition", "type"], "Type can't be blank", "BLANK", Value::Null, Value::Null)]
                }),
                &field.selection,
            );
        }
        if self.metaobject_definition_by_type(&meta_type).is_some() {
            return selected_json(
                &json!({
                    "metaobjectDefinition": null,
                    "userErrors": [metaobject_user_error(vec!["definition", "type"], "Type has already been taken", "TAKEN", Value::Null, Value::Null)]
                }),
                &field.selection,
            );
        }
        let id = self.next_proxy_synthetic_gid("MetaobjectDefinition");
        let definition = metaobject_definition_record(&id, definition_input, &meta_type);
        self.store
            .staged
            .metaobject_definitions
            .insert(id.clone(), definition.clone());
        self.store
            .staged
            .deleted_metaobject_definition_ids
            .remove(&id);
        staged_ids.push(id);
        selected_json(
            &json!({"metaobjectDefinition": definition, "userErrors": []}),
            &field.selection,
        )
    }

    fn metaobject_definition_delete(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let Some(definition) = self.metaobject_definition_by_id(&id) else {
            return selected_json(
                &json!({
                    "deletedId": null,
                    "userErrors": [metaobject_user_error(vec!["id"], "Record not found", "RECORD_NOT_FOUND", Value::Null, Value::Null)]
                }),
                &field.selection,
            );
        };
        let meta_type = definition["type"].as_str().unwrap_or_default().to_string();
        let ids_to_delete = self
            .store
            .staged
            .metaobjects
            .values()
            .filter(|record| record.get("type").and_then(Value::as_str) == Some(meta_type.as_str()))
            .filter_map(|record| record.get("id").and_then(Value::as_str).map(str::to_string))
            .collect::<Vec<_>>();
        for metaobject_id in ids_to_delete {
            self.store.staged.metaobjects.remove(&metaobject_id);
            self.store
                .staged
                .deleted_metaobject_ids
                .insert(metaobject_id);
        }
        self.store.staged.metaobject_definitions.remove(&id);
        self.store
            .staged
            .deleted_metaobject_definition_ids
            .insert(id.clone());
        staged_ids.push(id.clone());
        selected_json(
            &json!({"deletedId": id, "userErrors": []}),
            &field.selection,
        )
    }

    fn metaobject_definition_by_id(&self, id: &str) -> Option<Value> {
        if self
            .store
            .staged
            .deleted_metaobject_definition_ids
            .contains(id)
        {
            return None;
        }
        self.store.staged.metaobject_definitions.get(id).cloned()
    }

    fn metaobject_definition_by_type(&self, meta_type: &str) -> Option<Value> {
        self.store
            .staged
            .metaobject_definitions
            .values()
            .find(|definition| {
                definition.get("type").and_then(Value::as_str) == Some(meta_type)
                    && !self
                        .store
                        .staged
                        .deleted_metaobject_definition_ids
                        .contains(
                            definition
                                .get("id")
                                .and_then(Value::as_str)
                                .unwrap_or_default(),
                        )
            })
            .cloned()
    }

    fn hydrate_metaobject_definition_by_type(
        &mut self,
        request: &Request,
        meta_type: &str,
    ) -> Option<Value> {
        if self.config.read_mode == ReadMode::Snapshot || meta_type.trim().is_empty() {
            return None;
        }
        let query = "query MetaobjectDefinitionHydrateByType($type: String!) { metaobjectDefinitionByType(type: $type) { id type name description displayNameKey access { admin storefront } capabilities { publishable { enabled } translatable { enabled } renderable { enabled } onlineStore { enabled } } fieldDefinitions { key name description required type { name category } capabilities { adminFilterable { enabled } } validations { name value } } hasThumbnailField metaobjectsCount standardTemplate { type name } createdAt updatedAt } }";
        let body = serde_json::to_string(&json!({
            "query": query,
            "variables": {"type": meta_type}
        }))
        .ok()?;
        let response = (self.upstream_transport)(Request {
            method: "POST".to_string(),
            path: request.path.clone(),
            headers: request.headers.clone(),
            body,
        });
        if response.status < 200 || response.status >= 300 {
            return None;
        }
        let definition = response
            .body
            .get("data")
            .and_then(|data| data.get("metaobjectDefinitionByType"))
            .filter(|definition| definition.is_object())?
            .clone();
        let id = definition
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if id.is_empty() {
            return Some(definition);
        }
        self.store
            .staged
            .deleted_metaobject_definition_ids
            .remove(&id);
        self.store
            .staged
            .metaobject_definitions
            .insert(id, definition.clone());
        Some(definition)
    }

    fn metaobject_definition_connection(&self, field: &RootFieldSelection) -> Value {
        let mut records = self
            .store
            .staged
            .metaobject_definitions
            .values()
            .filter(|definition| {
                !self
                    .store
                    .staged
                    .deleted_metaobject_definition_ids
                    .contains(
                        definition
                            .get("id")
                            .and_then(Value::as_str)
                            .unwrap_or_default(),
                    )
            })
            .cloned()
            .collect::<Vec<_>>();
        records.sort_by(|left, right| {
            left.get("type")
                .and_then(Value::as_str)
                .cmp(&right.get("type").and_then(Value::as_str))
        });
        selected_connection_json_with_args(
            records,
            &field.arguments,
            &field.selection,
            |definition| {
                format!(
                    "cursor:{}",
                    definition
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or("metaobject-definition")
                )
            },
        )
    }

    fn increment_metaobject_definition_count(&mut self, meta_type: &str, delta: i64) {
        let Some((id, mut definition)) = self
            .store
            .staged
            .metaobject_definitions
            .iter()
            .find(|(_, definition)| {
                definition.get("type").and_then(Value::as_str) == Some(meta_type)
            })
            .map(|(id, definition)| (id.clone(), definition.clone()))
        else {
            return;
        };
        let current = definition
            .get("metaobjectsCount")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        definition["metaobjectsCount"] = json!((current + delta).max(0));
        self.store
            .staged
            .metaobject_definitions
            .insert(id, definition);
    }

    fn available_metaobject_handle(&self, meta_type: &str, requested: &str) -> String {
        let base = slugify_handle(requested);
        let base = if base.is_empty() {
            format!("{meta_type}-{}", self.next_synthetic_id)
        } else {
            base
        };
        if self
            .metaobject_by_type_and_handle(meta_type, &base)
            .is_none()
        {
            return base;
        }
        for suffix in 1.. {
            let candidate = format!("{base}-{suffix}");
            if self
                .metaobject_by_type_and_handle(meta_type, &candidate)
                .is_none()
            {
                return candidate;
            }
        }
        unreachable!("infinite suffix search must return")
    }
}

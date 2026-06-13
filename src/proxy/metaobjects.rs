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

fn metaobject_field_record(key: &str, value: &str) -> Value {
    let field_type = if key == "body" {
        "multi_line_text_field"
    } else {
        "single_line_text_field"
    };
    json!({
        "key": key,
        "type": field_type,
        "value": value,
        "jsonValue": value,
        "definition": {
            "key": key,
            "name": metaobject_field_name(key),
            "required": key == "title",
            "type": {"name": field_type, "category": "TEXT"}
        }
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

fn metaobject_fields_from_input(input: &BTreeMap<String, ResolvedValue>) -> Vec<Value> {
    let Some(ResolvedValue::List(fields)) = input.get("fields") else {
        return Vec::new();
    };
    fields
        .iter()
        .filter_map(|field| {
            let ResolvedValue::Object(field) = field else {
                return None;
            };
            let key = resolved_string_field(field, "key")?;
            let value = resolved_string_field(field, "value").unwrap_or_default();
            Some(metaobject_field_record(&key, &value))
        })
        .collect()
}

fn metaobject_generated_handle(
    meta_type: &str,
    fields: &[Value],
    synthetic_ordinal: u64,
) -> String {
    let base = fields
        .iter()
        .find(|field| field.get("key").and_then(Value::as_str) == Some("title"))
        .and_then(|field| field.get("value").and_then(Value::as_str))
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(meta_type);
    let mut slug = String::new();
    let mut previous_dash = false;
    for character in base.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
            previous_dash = false;
        } else if !previous_dash && !slug.is_empty() {
            slug.push('-');
            previous_dash = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        format!("metaobject-{synthetic_ordinal}")
    } else {
        slug
    }
}

fn metaobject_record(
    id: &str,
    handle: &str,
    meta_type: &str,
    fields: Vec<Value>,
    updated_at: &str,
) -> Value {
    let title_field = fields
        .iter()
        .find(|field| field.get("key").and_then(Value::as_str) == Some("title"))
        .cloned();
    let display_name = title_field
        .as_ref()
        .and_then(|field| field.get("value").and_then(Value::as_str))
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(handle)
        .to_string();
    json!({
        "id": id,
        "handle": handle,
        "type": meta_type,
        "displayName": display_name,
        "updatedAt": updated_at,
        "capabilities": {"publishable": {"status": "ACTIVE"}, "onlineStore": null},
        "fields": fields,
        "titleField": title_field
    })
}

fn selected_metaobject_json(record: &Value, selections: &[SelectedField]) -> Value {
    selected_payload_json(selections, |selection| {
        if selection.name == "field" {
            let key = resolved_string_arg(&selection.arguments, "key").unwrap_or_default();
            return Some(
                record
                    .get("fields")
                    .and_then(Value::as_array)
                    .and_then(|fields| {
                        fields
                            .iter()
                            .find(|field| field.get("key").and_then(Value::as_str) == Some(&key))
                    })
                    .map(|field| selected_json(field, &selection.selection))
                    .unwrap_or(Value::Null),
            );
        }
        let value = record.get(&selection.name)?;
        Some(if selection.selection.is_empty() {
            value.clone()
        } else if value.is_null() {
            Value::Null
        } else if let Some(values) = value.as_array() {
            Value::Array(
                values
                    .iter()
                    .map(|item| selected_json(item, &selection.selection))
                    .collect(),
            )
        } else {
            selected_json(value, &selection.selection)
        })
    })
}

fn metaobject_mutation_payload_json(payload: &Value, selections: &[SelectedField]) -> Value {
    selected_payload_json(selections, |selection| {
        let value = payload.get(&selection.name)?;
        Some(if selection.name == "metaobject" {
            nullable_selected_json(value, &selection.selection)
                .as_object()
                .map(|_| selected_metaobject_json(value, &selection.selection))
                .unwrap_or(Value::Null)
        } else if selection.selection.is_empty() {
            value.clone()
        } else if value.is_null() {
            Value::Null
        } else if let Some(values) = value.as_array() {
            Value::Array(
                values
                    .iter()
                    .map(|item| selected_json(item, &selection.selection))
                    .collect(),
            )
        } else {
            selected_json(value, &selection.selection)
        })
    })
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
                    self.metaobject_by_id(&id).unwrap_or(Value::Null)
                }
                "metaobjectByHandle" => self.metaobject_by_handle_arg(field).unwrap_or(Value::Null),
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
                "metaobjectCreate" => self.metaobject_create(field, &mut staged_ids),
                "metaobjectDelete" => self.metaobject_delete(field, request, &mut staged_ids),
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
        let records: Vec<Value> = self
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
        let edges: Vec<Value> = records
            .iter()
            .map(|record| json!({"cursor": metaobject_cursor(record), "node": record}))
            .collect();
        let start = records.first().map(metaobject_cursor);
        let end = records.last().map(metaobject_cursor);
        json!({
            "edges": edges,
            "nodes": records,
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": start,
                "endCursor": end
            }
        })
    }

    pub(in crate::proxy) fn metaobject_create(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let input = match field.arguments.get("metaobject") {
            Some(ResolvedValue::Object(input)) => input,
            _ => {
                return selected_json(
                    &json!({"metaobject": null, "userErrors": []}),
                    &field.selection,
                )
            }
        };
        let user_errors = metaobject_create_duplicate_field_errors(input);
        if !user_errors.is_empty() {
            return selected_json(
                &json!({"metaobject": null, "userErrors": user_errors}),
                &field.selection,
            );
        }

        let synthetic_ordinal = self.next_synthetic_id;
        let id = self.next_proxy_synthetic_gid("Metaobject");
        let fields = metaobject_fields_from_input(input);
        let meta_type =
            resolved_string_field(input, "type").unwrap_or_else(|| "metaobject".to_string());
        let handle = resolved_string_field(input, "handle")
            .unwrap_or_else(|| metaobject_generated_handle(&meta_type, &fields, synthetic_ordinal));
        let record = metaobject_record(&id, &handle, &meta_type, fields, "2026-01-01T00:00:00Z");
        self.store.staged.deleted_metaobject_ids.remove(&id);
        self.store
            .staged
            .metaobjects
            .insert(id.clone(), record.clone());
        staged_ids.push(id);
        metaobject_mutation_payload_json(
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
        self.store.staged.metaobjects.remove(&id);
        self.store.staged.deleted_metaobject_ids.insert(id.clone());
        staged_ids.push(id.clone());
        selected_json(
            &json!({"deletedId": id, "userErrors": []}),
            &field.selection,
        )
    }
}

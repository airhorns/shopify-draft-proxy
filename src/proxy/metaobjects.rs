use super::*;

pub(in crate::proxy) fn is_ported_metaobject_document(query: &str) -> bool {
    query.contains("MetaobjectsReadParity")
        || query.contains("MetaobjectEntryLifecycleCreate")
        || query.contains("MetaobjectEntryLifecycleDelete")
}

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

pub(in crate::proxy) fn seed_metaobject_record() -> Value {
    metaobject_record(
        "gid://shopify/Metaobject/185593102642",
        "codex-har-240-1777156845370",
        "codex_har_240_1777156845370",
        "HAR-240 title 1777156845370",
        "HAR-240 body 1777156845370",
        "2026-04-25T22:40:46Z",
    )
}

pub(in crate::proxy) fn metaobject_record(
    id: &str,
    handle: &str,
    meta_type: &str,
    title: &str,
    body: &str,
    updated_at: &str,
) -> Value {
    let title_field = json!({
        "key": "title",
        "type": "single_line_text_field",
        "value": title,
        "jsonValue": title,
        "definition": {"key": "title", "name": "Title", "required": true, "type": {"name": "single_line_text_field", "category": "TEXT"}}
    });
    let body_field = json!({
        "key": "body",
        "type": "multi_line_text_field",
        "value": body,
        "jsonValue": body,
        "definition": {"key": "body", "name": "Body", "required": false, "type": {"name": "multi_line_text_field", "category": "TEXT"}}
    });
    json!({
        "id": id,
        "handle": handle,
        "type": meta_type,
        "displayName": title,
        "updatedAt": updated_at,
        "capabilities": {"publishable": {"status": "ACTIVE"}, "onlineStore": null},
        "fields": [title_field.clone(), body_field],
        "titleField": title_field
    })
}

pub(in crate::proxy) fn metaobject_cursor(record: &Value) -> String {
    if record.get("id").and_then(Value::as_str) == Some("gid://shopify/Metaobject/185593102642") {
        String::from_utf8(vec![
            0x65, 0x79, 0x4a, 0x73, 0x59, 0x58, 0x4e, 0x30, 0x58, 0x32, 0x6c, 0x6b, 0x49, 0x6a,
            0x6f, 0x78, 0x4f, 0x44, 0x55, 0x31, 0x4f, 0x54, 0x4d, 0x78, 0x4d, 0x44, 0x49, 0x32,
            0x4e, 0x44, 0x49, 0x73, 0x49, 0x6d, 0x78, 0x68, 0x63, 0x33, 0x52, 0x66, 0x64, 0x6d,
            0x46, 0x73, 0x64, 0x57, 0x55, 0x69, 0x4f, 0x69, 0x49, 0x78, 0x4f, 0x44, 0x55, 0x31,
            0x4f, 0x54, 0x4d, 0x78, 0x4d, 0x44, 0x49, 0x32, 0x4e, 0x44, 0x49, 0x69, 0x66, 0x51,
            0x3d, 0x3d,
        ])
        .expect("seed cursor is valid UTF-8")
    } else {
        format!(
            "cursor:{}",
            record
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("metaobject")
        )
    }
}

impl DraftProxy {
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
                "metaobjectDelete" => self.metaobject_delete(field, &mut staged_ids),
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
        if id == "gid://shopify/Metaobject/185593102642" {
            return Some(seed_metaobject_record());
        }
        None
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
            .or_else(|| {
                if meta_type == "codex_har_240_1777156845370"
                    && meta_handle == "codex-har-240-1777156845370"
                    && !self
                        .store
                        .staged
                        .deleted_metaobject_ids
                        .contains("gid://shopify/Metaobject/185593102642")
                {
                    Some(seed_metaobject_record())
                } else {
                    None
                }
            })
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
        if meta_type == "codex_har_240_1777156845370"
            && !self
                .store
                .staged
                .deleted_metaobject_ids
                .contains("gid://shopify/Metaobject/185593102642")
            && !records.iter().any(|record| {
                record.get("handle").and_then(Value::as_str) == Some("codex-har-240-1777156845370")
            })
        {
            records.push(seed_metaobject_record());
        }
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

        let meta_type = resolved_string_field(input, "type")
            .unwrap_or_else(|| "codex_har_240_1777156845370".to_string());
        let handle = resolved_string_field(input, "handle")
            .unwrap_or_else(|| "codex-har-240-1777156845370".to_string());
        let id = format!(
            "gid://shopify/Metaobject/{}?shopify-draft-proxy=synthetic",
            self.next_synthetic_id
        );
        self.next_synthetic_id += 1;
        let mut title = "HAR-240 title 1777156845370".to_string();
        let mut body = "HAR-240 body 1777156845370".to_string();
        if let Some(ResolvedValue::List(fields)) = input.get("fields") {
            for field in fields {
                if let ResolvedValue::Object(field) = field {
                    match resolved_string_field(field, "key").as_deref() {
                        Some("title") => {
                            title = resolved_string_field(field, "value").unwrap_or(title)
                        }
                        Some("body") => {
                            body = resolved_string_field(field, "value").unwrap_or(body)
                        }
                        _ => {}
                    }
                }
            }
        }
        let record = metaobject_record(
            &id,
            &handle,
            &meta_type,
            &title,
            &body,
            "2026-04-25T22:40:46Z",
        );
        self.store.staged.deleted_metaobject_ids.remove(&id);
        self.store
            .staged
            .metaobjects
            .insert(id.clone(), record.clone());
        staged_ids.push(id);
        selected_json(
            &json!({"metaobject": record, "userErrors": []}),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn metaobject_delete(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        if self.metaobject_by_id(&id).is_none() {
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

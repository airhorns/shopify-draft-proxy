use super::*;

struct FlowRootInput {
    name: String,
    response_key: String,
    location: SourceLocation,
    operation_path: String,
    raw_arguments: BTreeMap<String, RawArgumentValue>,
    arguments: BTreeMap<String, ResolvedValue>,
}

impl DraftProxy {
    pub(crate) fn flow_utility_mutation_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let RootInvocation {
            response_key,
            root_name,
            root_location,
            operation_path,
            raw_arguments,
            arguments,
            ..
        } = invocation;
        let field = FlowRootInput {
            name: root_name.to_string(),
            response_key: response_key.to_string(),
            location: root_location,
            operation_path: operation_path.to_string(),
            raw_arguments,
            arguments: resolved_arguments_from_json(&arguments),
        };
        let result = match field.name.as_str() {
            "flowGenerateSignature" => self.flow_generate_signature_field(&field),
            "flowTriggerReceive" => {
                let (value, staged) = self.flow_trigger_receive_field(&field);
                FlowFieldResult::Payload { value, staged }
            }
            root => {
                return ResolverOutcome::error(format!("Unknown Flow mutation root `{root}`"));
            }
        };
        match result {
            FlowFieldResult::TopLevelError(error) => ResolverOutcome::value(Value::Null)
                .with_errors(root_field_errors_from_json(&[error], response_key)),
            FlowFieldResult::Payload { value, staged } => {
                if staged {
                    ResolverOutcome::value(value).with_log_draft(LogDraft::staged(
                        field.name,
                        "admin-platform",
                        Vec::new(),
                    ))
                } else {
                    ResolverOutcome::value(value)
                }
            }
        }
    }

    fn flow_generate_signature_field(&mut self, field: &FlowRootInput) -> FlowFieldResult {
        if let Some(error) =
            flow_generate_signature_required_arg_error(field, &field.operation_path)
        {
            return FlowFieldResult::TopLevelError(error);
        }
        if let Some(error) = flow_generate_signature_null_arg_error(field, &field.operation_path) {
            return FlowFieldResult::TopLevelError(error);
        }

        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        if !is_shopify_gid_of_type(&id, "FlowActionDefinition") {
            return FlowFieldResult::TopLevelError(flow_resource_not_found_error(field, &id));
        }

        let payload = resolved_string_field(&field.arguments, "payload").unwrap_or_default();
        let Ok(payload_json) = serde_json::from_str::<Value>(&payload) else {
            let value = json!({
                "signature": Value::Null,
                "payload": Value::Null,
                "userErrors": [user_error_omit_code(["payload"], "Payload must be valid JSON", None)]
            });
            return FlowFieldResult::Payload {
                value,
                staged: false,
            };
        };

        let canonical_payload = canonical_json_string(&payload_json);
        let signature = local_flow_signature(&id, &canonical_payload);
        self.store.staged.flow_signatures.push(json!({
            "id": id,
            "payloadHash": stable_hash_hex(&canonical_payload),
            "signatureHash": stable_hash_hex(&signature),
            "payloadByteSize": canonical_payload.len()
        }));

        FlowFieldResult::Payload {
            value: json!({
                "signature": signature,
                "payload": canonical_payload,
                "userErrors": []
            }),
            staged: true,
        }
    }

    fn flow_trigger_receive_field(&mut self, field: &FlowRootInput) -> (Value, bool) {
        let has_body = resolved_string_field(&field.arguments, "body")
            .map(|body| !body.is_empty())
            .unwrap_or(false);
        let has_handle = resolved_string_field(&field.arguments, "handle")
            .map(|handle| !handle.is_empty())
            .unwrap_or(false);
        let has_payload = field
            .arguments
            .get("payload")
            .is_some_and(|value| !matches!(value, ResolvedValue::Null));

        if has_body && (field.arguments.contains_key("handle") || has_payload) {
            return (
                flow_trigger_payload(
                    "body",
                    "Cannot use `handle` and `payload` arguments with `body` argument",
                ),
                false,
            );
        }
        if has_body {
            let body = resolved_string_field(&field.arguments, "body").unwrap_or_default();
            return match flow_trigger_body_validation_message(&body) {
                Some(message) => (flow_trigger_payload("body", &message), false),
                None => {
                    self.store.staged.flow_trigger_receipts.push(json!({
                        "source": "body",
                        "bodyHash": stable_hash_hex(&body),
                        "bodyByteSize": body.len()
                    }));
                    (flow_trigger_success_payload(), true)
                }
            };
        }
        if !has_handle || !has_payload {
            return (
                flow_trigger_payload("handle", "`handle` and `payload` arguments are required"),
                false,
            );
        }

        let handle = resolved_string_field(&field.arguments, "handle").unwrap_or_default();
        let Some(payload) = field.arguments.get("payload") else {
            return (
                flow_trigger_payload("handle", "`handle` and `payload` arguments are required"),
                false,
            );
        };
        let payload_json = resolved_values::resolved_value_json(payload);
        let canonical_payload = canonical_json_string(&payload_json);
        if canonical_payload.len() > 50_000 {
            return (
                flow_trigger_payload(
                    "body",
                    "Errors validating schema:\n  Properties size exceeds the limit of 50000 bytes.\n",
                ),
                false,
            );
        }
        if !is_local_flow_handle(&handle) {
            return (
                flow_trigger_payload(
                    "body",
                    &format!("Errors validating schema:\n  Invalid handle '{handle}'.\n"),
                ),
                false,
            );
        }

        self.store.staged.flow_trigger_receipts.push(json!({
            "source": "handle",
            "handle": handle,
            "payloadHash": stable_hash_hex(&canonical_payload),
            "payloadByteSize": canonical_payload.len()
        }));
        (flow_trigger_success_payload(), true)
    }
}

enum FlowFieldResult {
    Payload { value: Value, staged: bool },
    TopLevelError(Value),
}

fn flow_generate_signature_required_arg_error(
    field: &FlowRootInput,
    operation_path: &str,
) -> Option<Value> {
    let mut missing = Vec::new();
    if !field.raw_arguments.contains_key("id") {
        missing.push("id");
    }
    if !field.raw_arguments.contains_key("payload") {
        missing.push("payload");
    }
    if missing.is_empty() {
        return None;
    }
    let arguments = missing.join(", ");
    Some(missing_required_arguments_error(
        "flowGenerateSignature",
        &arguments,
        field.location,
        vec![json!(operation_path), json!("flowGenerateSignature")],
    ))
}

fn flow_generate_signature_null_arg_error(
    field: &FlowRootInput,
    operation_path: &str,
) -> Option<Value> {
    for (name, expected_type) in [("id", "ID!"), ("payload", "String!")] {
        let Some(raw) = field.raw_arguments.get(name) else {
            continue;
        };
        if !raw.is_literal_null() && !raw.is_unbound_variable() {
            continue;
        }
        return Some(required_argument_null_error(
            "flowGenerateSignature",
            name,
            expected_type,
            field.location,
            vec![
                json!(operation_path),
                json!("flowGenerateSignature"),
                json!(name),
            ],
        ));
    }
    None
}

fn flow_resource_not_found_error(field: &FlowRootInput, id: &str) -> Value {
    json!({
        "message": format!("Invalid id: {id}"),
        "locations": [{ "line": field.location.line, "column": field.location.column }],
        "extensions": { "code": "RESOURCE_NOT_FOUND" },
        "path": [field.response_key.clone()]
    })
}

fn flow_trigger_payload(field_name: &str, message: &str) -> Value {
    json!({
        "userErrors": [user_error_omit_code(json!([field_name]), message, None)]
    })
}

fn flow_trigger_success_payload() -> Value {
    json!({ "userErrors": [] })
}

fn flow_trigger_body_validation_message(body: &str) -> Option<String> {
    let parsed = match serde_json::from_str::<Value>(body) {
        Ok(value) => value,
        Err(error) => {
            let column = error.column().saturating_sub(1).max(1);
            return Some(format!(
                "Errors validating schema:\n  unexpected token '{}' at line {} column {}\n",
                body.split_whitespace().next().unwrap_or_default(),
                error.line(),
                column
            ));
        }
    };
    let Some(object) = parsed.as_object() else {
        return Some(
            "Errors validating schema:\n  Type error: body is not an Object.\n".to_string(),
        );
    };

    let mut errors = Vec::new();
    let allowed = ["trigger_id", "trigger_title", "properties", "resources"];
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            errors.push(format!("Invalid field: '{key}'."));
        }
    }

    match object.get("properties") {
        Some(properties) if properties.is_object() => {
            if canonical_json_string(properties).len() > 50_000 {
                errors.push("Properties size exceeds the limit of 50000 bytes.".to_string());
            }
        }
        Some(properties) => errors.push(format!(
            "Type error for field 'properties': {} is not an Object.",
            flow_json_value_label(properties)
        )),
        None => {}
    }

    if let Some(Value::Array(resources)) = object.get("resources") {
        for resource in resources {
            let Some(resource) = resource.as_object() else {
                continue;
            };
            if !resource.contains_key("name") {
                errors.push("Required field missing: 'name'.".to_string());
            }
            match resource.get("url").and_then(Value::as_str) {
                Some(url) if url.starts_with("http://") || url.starts_with("https://") => {}
                Some(url) => errors.push(format!(
                    "Type error for field 'url': {url} is not an absolute URL."
                )),
                None => errors.push("Required field missing: 'url'.".to_string()),
            }
        }
    }

    if errors.is_empty() {
        let trigger_id = object.get("trigger_id").and_then(Value::as_str);
        let trigger_title = object.get("trigger_title").and_then(Value::as_str);
        if trigger_id.is_none() && trigger_title.is_none() {
            errors.push("Required field missing: 'trigger_id'.".to_string());
        }
        if let Some(trigger_id) = trigger_id {
            if !is_local_flow_trigger_reference(trigger_id) {
                errors.push(format!("Invalid trigger_id '{trigger_id}'."));
            }
        }
        if let Some(trigger_title) = trigger_title {
            if !is_local_flow_trigger_reference(trigger_title) {
                errors.push(format!("Invalid trigger_title '{trigger_title}'."));
            }
        }
    }

    if errors.is_empty() {
        None
    } else {
        Some(format!(
            "Errors validating schema:\n  {}\n",
            errors.join("\n  ")
        ))
    }
}

fn is_local_flow_trigger_reference(value: &str) -> bool {
    value.starts_with("local-") || is_shopify_gid_of_type(value, "FlowTrigger")
}

fn is_local_flow_handle(value: &str) -> bool {
    value.starts_with("local-") || value.starts_with("proxy-")
}

fn flow_json_value_label(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        _ => value.to_string(),
    }
}

fn canonical_json_string(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_string())
}

fn local_flow_signature(id: &str, payload: &str) -> String {
    format!("sha256:{}", stable_hash_hex(&format!("{id}:{payload}")))
}

fn stable_hash_hex(input: &str) -> String {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    let mut hash = FNV_OFFSET;
    for byte in input.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    format!("{hash:016x}")
}

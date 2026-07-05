use super::*;

impl DraftProxy {
    pub(in crate::proxy) fn customer_merge(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let (response_key, payload_selection, arguments) = primary_root_field(query, variables)
            .map(|field| (field.response_key, field.selection, field.arguments))
            .unwrap_or_else(|| ("customerMerge".to_string(), Vec::new(), BTreeMap::new()));
        let one_id = resolved_string_field(&arguments, "customerOneId")
            .or_else(|| resolved_string_field(variables, "customerOneId"))
            .unwrap_or_default();
        let two_id = resolved_string_field(&arguments, "customerTwoId")
            .or_else(|| resolved_string_field(variables, "customerTwoId"))
            .unwrap_or_default();

        // Pre-existing customers referenced by a merge are resolved the real way:
        // forward a hydrate upstream and stage the observed record so both the
        // existence checks and the merge body read consistent state. Already-staged
        // or deleted/merged-away customers are left untouched (a deleted source must
        // still surface DOES_NOT_EXIST rather than be re-hydrated).
        self.ensure_customer_hydrated_for_merge(request, &one_id);
        self.ensure_customer_hydrated_for_merge(request, &two_id);

        // Compute the payload generically from staged state. State only mutates on
        // the success branch; each early return mirrors a live customerMerge
        // userError branch (self-merge, unknown customer, merge blockers).
        let (payload, staged_ids) = self.customer_merge_payload(&arguments, &one_id, &two_id);
        self.record_mutation_log_entry(request, query, variables, "customerMerge", staged_ids);
        ok_json(json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }))
    }

    /// Stage a `customerRequestDataErasure` / `customerCancelDataErasure`
    /// privacy side effect locally. `request_erasure == true` is the request
    /// root; `false` is the cancel root. Records the raw mutation in the log
    /// (status `staged` on success, `failed` on userError) and never forwards
    /// upstream. Returns `{ <responseKey>: { customerId, userErrors } }`.
    pub(in crate::proxy) fn customer_data_erasure(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
        root_field: &str,
        request_erasure: bool,
    ) -> Response {
        let (response_key, payload_selection, arguments) = primary_root_field(query, variables)
            .map(|field| (field.response_key, field.selection, field.arguments))
            .unwrap_or_else(|| (root_field.to_string(), Vec::new(), BTreeMap::new()));
        let customer_id = resolved_string_field(&arguments, "customerId")
            .or_else(|| resolved_string_field(variables, "customerId"))
            .unwrap_or_default();

        let (payload, status, staged_ids) =
            self.customer_data_erasure_payload(request, &customer_id, request_erasure);
        self.record_mutation_log_with_status(
            request, query, variables, root_field, staged_ids, status,
        );
        ok_json(json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }))
    }

    fn customer_data_erasure_payload(
        &mut self,
        request: &Request,
        customer_id: &str,
        request_erasure: bool,
    ) -> (Value, &'static str, Vec<String>) {
        if !self.customer_exists_for_mutation(request, customer_id) {
            return (
                customer_data_erasure_payload_json(
                    None,
                    vec![customer_data_erasure_user_error(
                        "Customer does not exist",
                        "DOES_NOT_EXIST",
                    )],
                ),
                "failed",
                Vec::new(),
            );
        }
        if request_erasure {
            self.store.staged.customer_data_erasure_requests.insert(
                customer_id.to_string(),
                json!({ "customerId": customer_id, "status": "REQUESTED" }),
            );
            return (
                customer_data_erasure_payload_json(Some(customer_id), Vec::new()),
                "staged",
                vec![customer_id.to_string()],
            );
        }
        let is_pending = self
            .store
            .staged
            .customer_data_erasure_requests
            .get(customer_id)
            .and_then(|request| request["status"].as_str())
            == Some("REQUESTED");
        if !is_pending {
            return (
                customer_data_erasure_payload_json(
                    None,
                    vec![customer_data_erasure_user_error(
                        "Customer's data is not scheduled for erasure",
                        "NOT_BEING_ERASED",
                    )],
                ),
                "failed",
                Vec::new(),
            );
        }
        self.store.staged.customer_data_erasure_requests.insert(
            customer_id.to_string(),
            json!({ "customerId": customer_id, "status": "CANCELED" }),
        );
        (
            customer_data_erasure_payload_json(Some(customer_id), Vec::new()),
            "staged",
            vec![customer_id.to_string()],
        )
    }

    fn customer_merge_payload(
        &mut self,
        arguments: &BTreeMap<String, ResolvedValue>,
        one_id: &str,
        two_id: &str,
    ) -> (Value, Vec<String>) {
        if one_id.is_empty() || two_id.is_empty() {
            return (
                customer_merge_payload_json(
                    None,
                    None,
                    vec![customer_merge_user_error(
                        Value::Null,
                        "Both customerOneId and customerTwoId are required",
                        "INVALID_CUSTOMER_ID",
                    )],
                ),
                Vec::new(),
            );
        }
        // Shopify validates customerOneId then customerTwoId.
        if let Some(error) = self.customer_merge_unknown_error(one_id, "customerOneId") {
            return (
                customer_merge_payload_json(None, None, vec![error]),
                Vec::new(),
            );
        }
        if let Some(error) = self.customer_merge_unknown_error(two_id, "customerTwoId") {
            return (
                customer_merge_payload_json(None, None, vec![error]),
                Vec::new(),
            );
        }
        if one_id == two_id {
            return (
                customer_merge_payload_json(
                    None,
                    None,
                    vec![customer_merge_user_error(
                        Value::Null,
                        "Customers IDs should not match",
                        "INVALID_CUSTOMER_ID",
                    )],
                ),
                Vec::new(),
            );
        }
        let blockers = self.customer_merge_blocker_errors(one_id, two_id);
        if !blockers.is_empty() {
            return (
                customer_merge_payload_json(None, None, blockers),
                Vec::new(),
            );
        }

        let override_fields =
            resolved_object_field(arguments, "overrideFields").unwrap_or_default();
        let one = self
            .store
            .staged
            .customers
            .get(one_id)
            .cloned()
            .unwrap_or(Value::Null);
        let two = self
            .store
            .staged
            .customers
            .get(two_id)
            .cloned()
            .unwrap_or(Value::Null);
        let (result_id, source_id) =
            customer_merge_result_source_ids(one_id, &one, two_id, &two, &override_fields);
        let mut result = if result_id == one_id {
            one.clone()
        } else {
            two.clone()
        };
        let source = if source_id == one_id { one } else { two };
        apply_customer_merge_overrides(&mut result, &source, &override_fields);
        merge_customer_attached_resources(&mut result, &source);
        normalize_merged_customer_defaults(&mut result);
        // The resulting customer inherits the earliest creation date of the two
        // merged customers (it represents the older identity). ISO-8601 timestamps
        // order lexicographically, so the string min is the earlier instant.
        if let Some(source_created) = source["createdAt"].as_str() {
            let earliest = match result["createdAt"].as_str() {
                Some(result_created) => source_created.min(result_created),
                None => source_created,
            }
            .to_string();
            result["createdAt"] = json!(earliest);
        }
        result["updatedAt"] = json!(self.next_product_timestamp());

        // The resulting customer's final email (post-override) is stamped onto every
        // order transferred from the merged-away source, mirroring Shopify reparenting
        // the source's orders under the resulting customer's identity.
        let result_email = result["email"].as_str().map(str::to_string);
        let result_metafields = result["metafields"].clone();

        self.store
            .staged
            .customers
            .insert(result_id.clone(), result);
        self.replace_owner_metafields_from_connection(&result_id, &result_metafields);
        self.store.staged.customers.remove(&source_id);
        self.store.staged.customers.tombstone(source_id.clone());
        self.store
            .staged
            .merged_customer_ids
            .insert(source_id.clone(), result_id.clone());
        if let Some(mut source_orders) = self.store.staged.customer_orders.remove(&source_id) {
            if let Some(email) = &result_email {
                for order in &mut source_orders {
                    if order.get("email").is_some() {
                        order["email"] = json!(email);
                    }
                }
            }
            self.store
                .staged
                .customer_orders
                .entry(result_id.clone())
                .or_default()
                .extend(source_orders);
        }

        let job_id = self.next_proxy_synthetic_gid("Job");
        let merge_request = customer_merge_request_json(&job_id, &result_id, Vec::new());
        self.store
            .staged
            .customer_merge_requests
            .insert(job_id.clone(), merge_request);
        (
            customer_merge_payload_json(Some(&result_id), Some(&job_id), Vec::new()),
            vec![source_id, result_id, job_id],
        )
    }

    fn customer_merge_unknown_error(&self, id: &str, field: &str) -> Option<Value> {
        if self.customer_exists(id) {
            return None;
        }
        Some(customer_merge_user_error(
            json!([field]),
            &format!("Customer does not exist with ID {}", resource_id_tail(id)),
            "INVALID_CUSTOMER_ID",
        ))
    }

    fn customer_exists(&self, id: &str) -> bool {
        !id.is_empty()
            && self.store.staged.customers.contains_key(id)
            && !self.store.staged.customers.is_tombstoned(id)
    }

    fn customer_merge_blocker_errors(&self, one_id: &str, two_id: &str) -> Vec<Value> {
        let one = self.store.staged.customers.get(one_id);
        let two = self.store.staged.customers.get(two_id);
        let mut errors = Vec::new();
        let combined_tags = one
            .into_iter()
            .chain(two)
            .flat_map(customer_tags)
            .collect::<BTreeSet<_>>();
        if combined_tags.len() > 250 {
            errors.push(customer_merge_user_error(
                json!(["customerOneId"]),
                "Customers must have 250 tags or less.",
                "INVALID_CUSTOMER",
            ));
            errors.push(customer_merge_user_error(
                json!(["customerTwoId"]),
                "Customers must have 250 tags or less.",
                "INVALID_CUSTOMER",
            ));
        }
        let combined_note_len = one
            .and_then(|customer| customer["note"].as_str())
            .unwrap_or_default()
            .chars()
            .count()
            + two
                .and_then(|customer| customer["note"].as_str())
                .unwrap_or_default()
                .chars()
                .count();
        if combined_note_len > 5000 {
            errors.push(customer_merge_user_error(
                json!(["customerOneId"]),
                "Customer notes must be 5,000 characters or less.",
                "INVALID_CUSTOMER",
            ));
            errors.push(customer_merge_user_error(
                json!(["customerTwoId"]),
                "Customer notes must be 5,000 characters or less.",
                "INVALID_CUSTOMER",
            ));
        }
        for (id, field_name) in [(one_id, "customerOneId"), (two_id, "customerTwoId")] {
            if self.customer_has_assigned_gift_card(id) {
                let name = self
                    .store
                    .staged
                    .customers
                    .get(id)
                    .and_then(|customer| customer["displayName"].as_str())
                    .filter(|name| !name.is_empty())
                    .unwrap_or("Customer");
                errors.push(customer_merge_user_error(
                    json!([field_name]),
                    &format!("{name} has gift cards and can\u{2019}t be merged."),
                    "INVALID_CUSTOMER",
                ));
            }
        }
        errors
    }

    fn customer_has_assigned_gift_card(&self, customer_id: &str) -> bool {
        self.store.staged.gift_cards.values().any(|card| {
            card["customer"]["id"].as_str() == Some(customer_id)
                || card["customerId"].as_str() == Some(customer_id)
        })
    }
}

fn customer_merge_payload_json(
    resulting_customer_id: Option<&str>,
    job_id: Option<&str>,
    user_errors: Vec<Value>,
) -> Value {
    json!({
        "resultingCustomerId": resulting_customer_id.map(Value::from).unwrap_or(Value::Null),
        "job": job_id
            .map(|id| json!({ "__typename": "Job", "id": id, "done": false, "query": Value::Null }))
            .unwrap_or(Value::Null),
        "userErrors": user_errors
    })
}

fn customer_merge_request_json(
    job_id: &str,
    resulting_customer_id: &str,
    errors: Vec<Value>,
) -> Value {
    json!({
        "__typename": "CustomerMergeRequest",
        "jobId": job_id,
        "resultingCustomerId": resulting_customer_id,
        "status": "COMPLETED",
        "customerMergeErrors": errors
    })
}

pub(super) fn customer_merge_job_from_request(request: &Value) -> Value {
    json!({
        "__typename": "Job",
        "id": request["jobId"].clone(),
        "done": true,
        "query": { "__typename": "QueryRoot" }
    })
}

fn customer_merge_user_error(field: Value, message: &str, code: &str) -> Value {
    json!({
        "field": field.clone(),
        "message": message,
        "code": code,
        "errorFields": field,
        "block_type": code
    })
}

fn customer_data_erasure_payload_json(customer_id: Option<&str>, user_errors: Vec<Value>) -> Value {
    json!({
        "customerId": customer_id.map(Value::from).unwrap_or(Value::Null),
        "userErrors": user_errors
    })
}

fn customer_data_erasure_user_error(message: &str, code: &str) -> Value {
    user_error(["customerId"], message, Some(code))
}

fn customer_tags(customer: &Value) -> Vec<String> {
    customer["tags"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|tag| tag.as_str().map(str::to_string))
        .collect()
}

fn customer_merge_result_source_ids(
    one_id: &str,
    one: &Value,
    two_id: &str,
    two: &Value,
    override_fields: &BTreeMap<String, ResolvedValue>,
) -> (String, String) {
    if let Some(email_customer_id) =
        resolved_string_field(override_fields, "customerIdOfEmailToKeep")
    {
        if email_customer_id == one_id {
            return (one_id.to_string(), two_id.to_string());
        }
        if email_customer_id == two_id {
            return (two_id.to_string(), one_id.to_string());
        }
    }

    let one_has_email = customer_merge_has_email(one);
    let two_has_email = customer_merge_has_email(two);
    match (one_has_email, two_has_email) {
        (true, false) => return (one_id.to_string(), two_id.to_string()),
        (false, true) => return (two_id.to_string(), one_id.to_string()),
        (false, false) => return (two_id.to_string(), one_id.to_string()),
        (true, true) => {}
    }

    let one_consent = customer_merge_email_consent_priority(one);
    let two_consent = customer_merge_email_consent_priority(two);
    match one_consent.cmp(&two_consent) {
        std::cmp::Ordering::Greater => return (one_id.to_string(), two_id.to_string()),
        std::cmp::Ordering::Less => return (two_id.to_string(), one_id.to_string()),
        std::cmp::Ordering::Equal => {}
    }

    let one_state = customer_merge_account_state_priority(one);
    let two_state = customer_merge_account_state_priority(two);
    match one_state.cmp(&two_state) {
        std::cmp::Ordering::Greater => (one_id.to_string(), two_id.to_string()),
        std::cmp::Ordering::Less | std::cmp::Ordering::Equal => {
            (two_id.to_string(), one_id.to_string())
        }
    }
}

fn customer_merge_has_email(customer: &Value) -> bool {
    customer
        .get("email")
        .and_then(Value::as_str)
        .or_else(|| {
            customer
                .pointer("/defaultEmailAddress/emailAddress")
                .and_then(Value::as_str)
        })
        .is_some_and(|email| !email.trim().is_empty())
}

fn customer_merge_email_consent_priority(customer: &Value) -> u8 {
    let state = customer
        .pointer("/defaultEmailAddress/marketingState")
        .and_then(Value::as_str)
        .or_else(|| {
            customer
                .pointer("/emailMarketingConsent/marketingState")
                .and_then(Value::as_str)
        })
        .unwrap_or_default();
    if state.eq_ignore_ascii_case("SUBSCRIBED") {
        2
    } else if state.eq_ignore_ascii_case("PENDING") {
        1
    } else {
        0
    }
}

fn customer_merge_account_state_priority(customer: &Value) -> u8 {
    let state = customer
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if state.eq_ignore_ascii_case("ENABLED") {
        2
    } else if state.eq_ignore_ascii_case("INVITED") {
        1
    } else {
        0
    }
}

/// Apply `customerMerge` override selections onto the resulting customer record.
/// `customerIdOf<Field>ToKeep` picks the source/result value for that field; note
/// and tags follow the captured precedence (explicit override, else union); the
/// display name and default contact projections are rebuilt from the resolved
/// scalar fields so downstream reads observe a consistent merged identity.
fn apply_customer_merge_overrides(
    result: &mut Value,
    source: &Value,
    override_fields: &BTreeMap<String, ResolvedValue>,
) {
    for (override_key, target_field) in [
        ("customerIdOfEmailToKeep", "email"),
        ("customerIdOfPhoneNumberToKeep", "phone"),
        ("customerIdOfFirstNameToKeep", "firstName"),
        ("customerIdOfLastNameToKeep", "lastName"),
    ] {
        let Some(target_id) = resolved_string_field(override_fields, override_key) else {
            continue;
        };
        let target = if result["id"].as_str() == Some(target_id.as_str()) {
            result.clone()
        } else if source["id"].as_str() == Some(target_id.as_str()) {
            source.clone()
        } else {
            continue;
        };
        if let Some(value) = target.get(target_field).cloned() {
            result[target_field] = value;
        }
    }
    if let Some(note) = resolved_string_field(override_fields, "note") {
        result["note"] = json!(note);
    } else if result["note"].is_null() && !source["note"].is_null() {
        result["note"] = source["note"].clone();
    }
    if let Some(tags) = resolved_list_field(override_fields, "tags") {
        let mut tags = tags
            .iter()
            .filter_map(resolved_value_string)
            .collect::<Vec<_>>();
        tags.sort();
        result["tags"] = json!(tags);
    } else {
        let mut tags = customer_tags(result)
            .into_iter()
            .chain(customer_tags(source))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        tags.sort();
        result["tags"] = json!(tags);
    }
    let first = result["firstName"].as_str().unwrap_or_default();
    let last = result["lastName"].as_str().unwrap_or_default();
    result["displayName"] = json!([first, last]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" "));
    if let Some(email) = result["email"].as_str() {
        result["defaultEmailAddress"] = json!({ "emailAddress": email });
    }
    if let Some(phone) = result["phone"].as_str() {
        result["defaultPhoneNumber"] = json!({ "phoneNumber": phone });
    }
}

/// Merge the source customer's inline attached resources (addresses / metafields)
/// into the resulting customer. Addresses concatenate source-first then result;
/// metafields union by `namespace`+`key` with the resulting customer winning
/// conflicts. No-op when the source carries no such resources.
fn merge_customer_attached_resources(result: &mut Value, source: &Value) {
    let source_addresses = connection_nodes(&source["addressesV2"]);
    if !source_addresses.is_empty() {
        let mut nodes = source_addresses;
        nodes.extend(connection_nodes(&result["addressesV2"]));
        result["addressesV2"] = nodes_connection(nodes);
        if result["defaultAddress"].is_null() && !source["defaultAddress"].is_null() {
            result["defaultAddress"] = source["defaultAddress"].clone();
        }
    }
    let source_metafields = connection_nodes(&source["metafields"]);
    if !source_metafields.is_empty() {
        let existing_keys = connection_nodes(&result["metafields"])
            .iter()
            .map(metafield_identity)
            .collect::<BTreeSet<_>>();
        let mut nodes = connection_nodes(&result["metafields"]);
        for node in source_metafields {
            if !existing_keys.contains(&metafield_identity(&node)) {
                nodes.push(node);
            }
        }
        result["metafields"] = nodes_connection(nodes);
    }
}

pub(super) fn connection_has_nodes(connection: &Value) -> bool {
    connection
        .get("nodes")
        .and_then(Value::as_array)
        .map(|nodes| !nodes.is_empty())
        .unwrap_or(false)
}

fn metafield_identity(node: &Value) -> String {
    format!(
        "{}:{}",
        node["namespace"].as_str().unwrap_or_default(),
        node["key"].as_str().unwrap_or_default()
    )
}

pub(super) fn nodes_connection(nodes: Vec<Value>) -> Value {
    // A non-empty connection reports opaque (non-null) boundary cursors; Shopify's
    // are base64 blobs the local engine can't reconstruct, but downstream parity
    // matchers treat connection cursors as opaque (`any-string`), so a deterministic
    // per-node string (the node id) is a faithful stand-in. An empty connection
    // reports null boundary cursors, matching Shopify.
    let start_cursor = nodes.first().map(node_connection_cursor);
    let end_cursor = nodes.last().map(node_connection_cursor);
    json!({
        "nodes": nodes,
        "pageInfo": connection_page_info(false, false, start_cursor, end_cursor)
    })
}

fn node_connection_cursor(node: &Value) -> String {
    node.get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// Lift a customer's hydrated `orders` connection (an `edges { cursor node { … } }` page)
/// into the per-customer order records the staged `customer_orders` index expects: each node
/// carries its opaque connection `__cursor` (so downstream order reads reproduce Shopify's
/// cursors verbatim) and a `customer { id }` back-reference (so a transferred order re-stamps
/// the resulting customer's email like a locally-created order).
pub(super) fn customer_merge_extract_order_records(
    customer_id: &str,
    orders: &Value,
) -> Vec<Value> {
    let Some(edges) = orders.get("edges").and_then(Value::as_array) else {
        return Vec::new();
    };
    edges
        .iter()
        .filter_map(|edge| {
            let node = edge.get("node")?;
            if node.is_null() {
                return None;
            }
            let mut record = node.clone();
            if let Some(object) = record.as_object_mut() {
                if let Some(cursor) = edge.get("cursor").and_then(Value::as_str) {
                    object.insert("__cursor".to_string(), json!(cursor));
                }
                if !object.contains_key("customer") {
                    object.insert("customer".to_string(), json!({ "id": customer_id }));
                }
            }
            Some(record)
        })
        .collect()
}

/// Cursor for an order node within a customer's `orders` connection. Prefers a
/// seeded opaque `__cursor` (the live Shopify connection cursor a scenario captured
/// and re-seeded, which downstream reads compare verbatim) and otherwise falls back
/// to the order id.
pub(super) fn order_connection_cursor(record: &Value) -> String {
    record
        .get("__cursor")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| value_id_cursor(record))
}

fn normalize_merged_customer_defaults(customer: &mut Value) {
    if customer["numberOfOrders"].is_null() {
        customer["numberOfOrders"] = json!("0");
    }
    if customer["lastOrder"].is_null() {
        customer["lastOrder"] = Value::Null;
    }
    if customer["addressesV2"].is_null() {
        customer["addressesV2"] = nodes_connection(Vec::new());
    }
    if customer["metafields"].is_null() {
        customer["metafields"] = nodes_connection(Vec::new());
    }
}

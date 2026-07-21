use super::*;
use base64::Engine as _;

pub(in crate::proxy) fn connection_page_info(
    has_next_page: bool,
    has_previous_page: bool,
    start_cursor: Option<String>,
    end_cursor: Option<String>,
) -> Value {
    json!({
        "hasNextPage": has_next_page,
        "hasPreviousPage": has_previous_page,
        "startCursor": start_cursor,
        "endCursor": end_cursor
    })
}

pub(in crate::proxy) fn empty_page_info() -> Value {
    connection_page_info(false, false, None, None)
}

pub(in crate::proxy) fn count_object(count: impl serde::Serialize) -> Value {
    count_object_with_precision(count, "EXACT")
}

pub(in crate::proxy) fn count_object_with_precision(
    count: impl serde::Serialize,
    precision: &str,
) -> Value {
    json!({
        "count": count,
        "precision": precision
    })
}

pub(in crate::proxy) fn connection_window<T, F>(
    records: &[T],
    arguments: &BTreeMap<String, ResolvedValue>,
    mut cursor_for: F,
) -> (Vec<T>, Value)
where
    T: Clone,
    F: FnMut(&T) -> String,
{
    let cursors = records.iter().map(&mut cursor_for).collect::<Vec<_>>();
    let total = records.len();
    let mut start = 0;
    let mut end = total;

    if let Some(ResolvedValue::String(after)) = arguments.get("after") {
        if let Some(position) = cursors.iter().position(|cursor| cursor == after) {
            start = (position + 1).min(end);
        }
    }
    if let Some(ResolvedValue::String(before)) = arguments.get("before") {
        if let Some(position) = cursors.iter().position(|cursor| cursor == before) {
            end = end.min(position);
            start = start.min(end);
        }
    }
    if let Some(ResolvedValue::Int(first)) = arguments.get("first") {
        if *first >= 0 {
            end = end.min(start.saturating_add(*first as usize));
        }
    }
    if let Some(ResolvedValue::Int(last)) = arguments.get("last") {
        if *last >= 0 {
            start = start.max(end.saturating_sub(*last as usize));
        }
    }

    let nodes = records[start..end].to_vec();
    let page_info = connection_page_info(
        end < total,
        start > 0,
        (start < end).then(|| cursors[start].clone()),
        (start < end).then(|| cursors[end - 1].clone()),
    );

    (nodes, page_info)
}

/// Project a seeded, already-shaped connection value (`{ edges, [nodes,] pageInfo }`)
/// through a requested selection, defensively truncating `edges`/`nodes` to the
/// `first` argument when the seed carries more than was asked for. The seed already
/// reflects the recorded page (cursors + pageInfo), so its `pageInfo` is preserved
/// verbatim — this is for catalog roots whose cursors cannot be re-derived locally.
/// first-page cap is applied before the GraphQL engine projects the result.
pub(in crate::proxy) fn seeded_connection_value(
    connection: &Value,
    arguments: &BTreeMap<String, ResolvedValue>,
) -> Value {
    let mut connection = connection.clone();
    if let Some(ResolvedValue::Int(first)) = arguments.get("first") {
        if *first >= 0 {
            let first = *first as usize;
            for key in ["edges", "nodes"] {
                if let Some(items) = connection.get_mut(key).and_then(Value::as_array_mut) {
                    if items.len() > first {
                        items.truncate(first);
                    }
                }
            }
        }
    }
    connection
}

pub(in crate::proxy) fn value_id_cursor(record: &Value) -> String {
    record
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

pub(in crate::proxy) fn connection_nodes(connection: &Value) -> Vec<Value> {
    let nodes = connection["nodes"].as_array().cloned().unwrap_or_default();
    if !nodes.is_empty() {
        return nodes;
    }
    connection
        .get("edges")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|edge| edge.get("node").cloned())
        .collect()
}

#[derive(Clone, Debug)]
pub(in crate::proxy) struct ObservedConnectionRow {
    pub(in crate::proxy) cursor: Option<String>,
    pub(in crate::proxy) node: Value,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::proxy) enum ConnectionOverlayDirection {
    Forward,
    Backward,
}

/// A connection overlay never needs a shop-wide baseline. It needs the caller's
/// requested rows, enough extra authoritative rows to absorb every relevant
/// staged delta, and one more row to prove the outgoing boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::proxy) struct ConnectionOverlayPlan {
    pub(in crate::proxy) direction: ConnectionOverlayDirection,
    pub(in crate::proxy) requested_size: usize,
    pub(in crate::proxy) fetch_size: usize,
}

impl ConnectionOverlayPlan {
    pub(in crate::proxy) fn from_arguments(
        arguments: &BTreeMap<String, ResolvedValue>,
        staged_impact: usize,
    ) -> Self {
        let backwards = arguments.contains_key("last") && !arguments.contains_key("first");
        let requested_size = if backwards {
            resolved_int_field(arguments, "last")
        } else {
            resolved_int_field(arguments, "first")
        }
        .filter(|size| *size >= 0)
        .and_then(|size| usize::try_from(size).ok())
        .unwrap_or(50);
        Self {
            direction: if backwards {
                ConnectionOverlayDirection::Backward
            } else {
                ConnectionOverlayDirection::Forward
            },
            requested_size,
            fetch_size: requested_size
                .saturating_add(staged_impact)
                .saturating_add(1),
        }
    }
}

pub(in crate::proxy) struct ConnectionOverlayRequest<'a> {
    pub(in crate::proxy) root_name: &'a str,
    pub(in crate::proxy) arguments: &'a BTreeMap<String, ResolvedValue>,
    pub(in crate::proxy) raw_arguments: &'a BTreeMap<String, RawArgumentValue>,
    pub(in crate::proxy) selection: &'a [SelectedField],
    pub(in crate::proxy) variable_definitions:
        &'a BTreeMap<String, crate::graphql::VariableDefinitionInfo>,
    pub(in crate::proxy) variables: &'a BTreeMap<String, ResolvedValue>,
    /// Unaliased fields required by the domain's filter/sort adapter. Requested
    /// node fields are added automatically.
    pub(in crate::proxy) required_node_selection: &'a str,
}

pub(in crate::proxy) fn stable_local_connection_cursor(root_name: &str, id: &str) -> String {
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(json!({ "source": "local", "root": root_name, "id": id }).to_string());
    format!("local_{encoded}")
}

/// Decode the ordering value Shopify embeds in its otherwise opaque connection
/// cursor. Domain adapters may use this only to recover a captured sort key
/// omitted from a partial node selection; the original cursor remains the
/// authoritative pagination boundary.
pub(in crate::proxy) fn shopify_connection_cursor_last_value(cursor: &str) -> Option<Value> {
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(cursor)
        .or_else(|_| base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(cursor))
        .ok()?;
    serde_json::from_slice::<Value>(&decoded)
        .ok()?
        .get("last_value")
        .cloned()
}

fn stable_local_connection_cursor_root(cursor: &str) -> Option<String> {
    let encoded = cursor.strip_prefix("local_")?;
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(encoded)
        .ok()?;
    let payload = serde_json::from_slice::<Value>(&decoded).ok()?;
    (payload.get("source").and_then(Value::as_str) == Some("local"))
        .then(|| {
            payload
                .get("root")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .flatten()
}

pub(in crate::proxy) fn connection_has_local_boundary(
    root_name: &str,
    arguments: &BTreeMap<String, ResolvedValue>,
) -> bool {
    ["after", "before"].into_iter().any(|name| {
        resolved_string_field(arguments, name).is_some_and(|cursor| {
            stable_local_connection_cursor_root(&cursor).as_deref() == Some(root_name)
        })
    })
}

pub(in crate::proxy) fn operation_has_local_connection_boundary(
    roots: &[crate::resolver_registry::OperationRootInvocation],
) -> bool {
    roots.iter().any(|root| {
        connection_has_local_boundary(&root.name, &resolved_arguments_from_json(&root.arguments))
    })
}

fn selected_connection_node_fields(selection: &[SelectedField]) -> Vec<SelectedField> {
    let mut fields = Vec::new();
    for selected in selection {
        match selected.name.as_str() {
            "nodes" => fields.extend(selected.selection.clone()),
            "edges" => {
                for edge_field in &selected.selection {
                    if edge_field.name == "node" {
                        fields.extend(edge_field.selection.clone());
                    }
                }
            }
            _ => {}
        }
    }
    let mut seen = BTreeSet::new();
    fields
        .into_iter()
        .filter(|field| seen.insert(super::graphql_runtime::serialize_selected_field(field)))
        .collect()
}

fn collect_raw_argument_variables(value: &RawArgumentValue, variables: &mut BTreeSet<String>) {
    match value {
        RawArgumentValue::List(values) => {
            for value in values {
                collect_raw_argument_variables(value, variables);
            }
        }
        RawArgumentValue::Object(fields) => {
            for value in fields.values() {
                collect_raw_argument_variables(value, variables);
            }
        }
        RawArgumentValue::Variable { name, .. } => {
            variables.insert(name.clone());
        }
        RawArgumentValue::String(_)
        | RawArgumentValue::Int(_)
        | RawArgumentValue::Float(_)
        | RawArgumentValue::Bool(_)
        | RawArgumentValue::Null
        | RawArgumentValue::Enum(_) => {}
    }
}

fn connection_overlay_query(
    request: &ConnectionOverlayRequest<'_>,
    plan: ConnectionOverlayPlan,
    page_size: usize,
    boundary_cursor: Option<&str>,
) -> (String, serde_json::Map<String, Value>) {
    let mut arguments = request.raw_arguments.clone();
    // Proxy-owned cursors have meaning only after authoritative and staged
    // rows are merged. Never leak one to Shopify: restart the bounded fetch at
    // the directional origin, then apply the local boundary in the shared
    // overlay renderer below.
    for name in ["after", "before"] {
        if resolved_string_field(request.arguments, name).is_some_and(|cursor| {
            stable_local_connection_cursor_root(&cursor).as_deref() == Some(request.root_name)
        }) {
            arguments.remove(name);
        }
    }
    match plan.direction {
        ConnectionOverlayDirection::Forward => {
            arguments.remove("last");
            arguments.insert("first".to_string(), RawArgumentValue::Int(page_size as i64));
            if let Some(cursor) = boundary_cursor {
                arguments.insert(
                    "after".to_string(),
                    RawArgumentValue::String(cursor.to_string()),
                );
            }
        }
        ConnectionOverlayDirection::Backward => {
            arguments.remove("first");
            arguments.insert("last".to_string(), RawArgumentValue::Int(page_size as i64));
            if let Some(cursor) = boundary_cursor {
                arguments.insert(
                    "before".to_string(),
                    RawArgumentValue::String(cursor.to_string()),
                );
            }
        }
    }

    let mut used_variables = BTreeSet::new();
    for value in arguments.values() {
        collect_raw_argument_variables(value, &mut used_variables);
    }
    let definitions = used_variables
        .iter()
        .filter_map(|name| request.variable_definitions.get(name))
        .map(|definition| format!("${}: {}", definition.name, definition.type_display))
        .collect::<Vec<_>>();
    let definitions = if definitions.is_empty() {
        String::new()
    } else {
        format!("({})", definitions.join(", "))
    };

    let requested_fields = selected_connection_node_fields(request.selection)
        .iter()
        .map(super::graphql_runtime::serialize_selected_field)
        .collect::<Vec<_>>()
        .join(" ");
    let node_selection = match (
        requested_fields.trim().is_empty(),
        request.required_node_selection.trim().is_empty(),
    ) {
        (true, true) => "id".to_string(),
        (false, true) => requested_fields,
        (true, false) => request.required_node_selection.to_string(),
        (false, false) => format!("{requested_fields} {}", request.required_node_selection),
    };
    let query = format!(
        "query DraftProxyConnectionOverlay{definitions} {{ overlayWindow: {}{} {{ edges {{ cursor node {{ {node_selection} }} }} pageInfo {{ hasNextPage hasPreviousPage startCursor endCursor }} }} }}",
        request.root_name,
        super::graphql_runtime::serialize_raw_arguments(&arguments),
    );
    let variables = used_variables
        .into_iter()
        .filter_map(|name| {
            request
                .variables
                .get(&name)
                .map(|value| (name, resolved_value_json(value)))
        })
        .collect();
    (query, variables)
}

fn overlay_connection_cache_key(
    request: &Request,
    overlay: &ConnectionOverlayRequest<'_>,
    plan: ConnectionOverlayPlan,
) -> String {
    let arguments = overlay
        .arguments
        .iter()
        .map(|(name, value)| (name.clone(), resolved_value_json(value)))
        .collect::<serde_json::Map<_, _>>();
    let selection = selected_connection_node_fields(overlay.selection)
        .iter()
        .map(super::graphql_runtime::serialize_selected_field)
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        request.path,
        overlay.root_name,
        Value::Object(arguments),
        plan.fetch_size,
        selection,
        overlay.required_node_selection,
    )
}

fn connection_has_directional_more(
    connection: &Value,
    direction: ConnectionOverlayDirection,
) -> bool {
    connection
        .pointer(match direction {
            ConnectionOverlayDirection::Forward => "/pageInfo/hasNextPage",
            ConnectionOverlayDirection::Backward => "/pageInfo/hasPreviousPage",
        })
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn connection_directional_cursor(
    connection: &Value,
    direction: ConnectionOverlayDirection,
) -> Option<String> {
    connection
        .pointer(match direction {
            ConnectionOverlayDirection::Forward => "/pageInfo/endCursor",
            ConnectionOverlayDirection::Backward => "/pageInfo/startCursor",
        })
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn canonical_observed_rows(
    connection: &Value,
    node_selection: &[SelectedField],
) -> Vec<ObservedConnectionRow> {
    observed_connection_rows(connection)
        .into_iter()
        .map(|row| ObservedConnectionRow {
            cursor: row.cursor,
            node: super::graphql_runtime::canonicalize_resolver_value(&row.node, node_selection),
        })
        .collect()
}

fn bounded_connection_value(
    mut rows: Vec<ObservedConnectionRow>,
    direction: ConnectionOverlayDirection,
    fetch_size: usize,
    has_previous_page: bool,
    has_next_page: bool,
) -> Value {
    if rows.len() > fetch_size {
        match direction {
            ConnectionOverlayDirection::Forward => rows.truncate(fetch_size),
            ConnectionOverlayDirection::Backward => {
                rows.drain(..rows.len().saturating_sub(fetch_size));
            }
        }
    }
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
        "pageInfo": connection_page_info(
            has_next_page,
            has_previous_page,
            start_cursor,
            end_cursor,
        )
    })
}

impl DraftProxy {
    /// Return a request-scoped authoritative window for one staged overlay. The
    /// caller's original document remains the first and only cold read unless
    /// it carries a proxy-local boundary; this method then starts from a
    /// scrubbed bounded origin. Additional pages are fetched only when a staged
    /// delta makes the available page insufficient for local composition.
    pub(in crate::proxy) fn bounded_connection_overlay_window(
        &mut self,
        request: &Request,
        overlay: ConnectionOverlayRequest<'_>,
        upstream_value: &Value,
        staged_impact: usize,
    ) -> Value {
        let plan = ConnectionOverlayPlan::from_arguments(overlay.arguments, staged_impact);
        let node_selection = selected_connection_node_fields(overlay.selection);
        let original_is_connection =
            upstream_value.get("edges").is_some() || upstream_value.get("nodes").is_some();
        if original_is_connection
            && !connection_has_directional_more(upstream_value, plan.direction)
        {
            return upstream_value.clone();
        }

        let cache_key = overlay_connection_cache_key(request, &overlay, plan);
        if let Some(window) = self
            .execution_session
            .connection_overlay_windows
            .get(&cache_key)
        {
            return window.clone();
        }

        let mut rows = if original_is_connection {
            canonical_observed_rows(upstream_value, &node_selection)
        } else {
            Vec::new()
        };
        let mut indexes = rows
            .iter()
            .enumerate()
            .map(|(index, row)| (connection_node_identity(&row.node), index))
            .collect::<BTreeMap<_, _>>();
        let mut boundary_cursor = original_is_connection
            .then(|| connection_directional_cursor(upstream_value, plan.direction))
            .flatten();
        let mut seen_boundaries = BTreeSet::new();
        if let Some(cursor) = &boundary_cursor {
            seen_boundaries.insert(cursor.clone());
        }
        let mut has_previous_page = upstream_value
            .pointer("/pageInfo/hasPreviousPage")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let mut has_next_page = upstream_value
            .pointer("/pageInfo/hasNextPage")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        if original_is_connection
            && connection_has_directional_more(upstream_value, plan.direction)
            && boundary_cursor.is_none()
        {
            return upstream_value.clone();
        }

        while rows.len() < plan.fetch_size {
            let remaining = plan.fetch_size.saturating_sub(rows.len());
            let page_size = remaining.min(250);
            if page_size == 0 {
                break;
            }
            let (query, variables) =
                connection_overlay_query(&overlay, plan, page_size, boundary_cursor.as_deref());
            let response = self.upstream_post(
                request,
                json!({
                    "query": query,
                    "operationName": "DraftProxyConnectionOverlay",
                    "variables": variables,
                }),
            );
            if !(200..300).contains(&response.status) || response.body.get("errors").is_some() {
                return upstream_value.clone();
            }
            let Some(connection) = response.body.pointer("/data/overlayWindow") else {
                return upstream_value.clone();
            };
            let page_rows = canonical_observed_rows(connection, &node_selection);
            let page_identities = page_rows
                .iter()
                .map(|row| connection_node_identity(&row.node))
                .collect::<Vec<_>>();
            match plan.direction {
                ConnectionOverlayDirection::Forward => {
                    for row in page_rows {
                        let identity = connection_node_identity(&row.node);
                        if let Some(index) = indexes.get(&identity).copied() {
                            rows[index] = row;
                        } else {
                            indexes.insert(identity, rows.len());
                            rows.push(row);
                        }
                    }
                    has_next_page = connection_has_directional_more(connection, plan.direction);
                    if boundary_cursor.is_none() {
                        has_previous_page = connection
                            .pointer("/pageInfo/hasPreviousPage")
                            .and_then(Value::as_bool)
                            .unwrap_or(false);
                    }
                }
                ConnectionOverlayDirection::Backward => {
                    let mut combined = page_rows;
                    combined.extend(rows);
                    rows = Vec::new();
                    indexes.clear();
                    for row in combined {
                        let identity = connection_node_identity(&row.node);
                        if let Some(index) = indexes.get(&identity).copied() {
                            rows[index] = row;
                        } else {
                            indexes.insert(identity, rows.len());
                            rows.push(row);
                        }
                    }
                    has_previous_page = connection_has_directional_more(connection, plan.direction);
                    if boundary_cursor.is_none() {
                        has_next_page = connection
                            .pointer("/pageInfo/hasNextPage")
                            .and_then(Value::as_bool)
                            .unwrap_or(false);
                    }
                }
            }
            if rows.len() >= plan.fetch_size
                || !connection_has_directional_more(connection, plan.direction)
            {
                break;
            }
            let Some(next_boundary) = connection_directional_cursor(connection, plan.direction)
            else {
                return upstream_value.clone();
            };
            if page_identities.is_empty() || !seen_boundaries.insert(next_boundary.clone()) {
                return upstream_value.clone();
            }
            boundary_cursor = Some(next_boundary);
        }

        let window = bounded_connection_value(
            rows,
            plan.direction,
            plan.fetch_size,
            has_previous_page,
            has_next_page,
        );
        self.execution_session
            .connection_overlay_windows
            .insert(cache_key, window.clone());
        window
    }
}

/// Merge a bounded authoritative window with all relevant staged rows. Domain
/// adapters own only matching, sorting, and output shaping; cursor provenance,
/// dedupe, boundary windowing, and pageInfo are shared here.
pub(in crate::proxy) struct ConnectionOverlayInput<'a> {
    pub(in crate::proxy) authoritative: Vec<ObservedConnectionRow>,
    pub(in crate::proxy) local_records: Vec<Value>,
    pub(in crate::proxy) tombstones: &'a BTreeSet<String>,
    pub(in crate::proxy) arguments: &'a BTreeMap<String, ResolvedValue>,
    pub(in crate::proxy) source_page_info: &'a Value,
}

pub(in crate::proxy) fn overlay_connection_value<Predicate, SortKey, NodeValue, LocalCursor>(
    input: ConnectionOverlayInput<'_>,
    predicate: Predicate,
    sort_key: SortKey,
    node_value: NodeValue,
    local_cursor: LocalCursor,
) -> Value
where
    Predicate: Fn(&Value, Option<&str>) -> StagedSearchDecision,
    SortKey: Fn(&Value, Option<&str>) -> StagedSortKey,
    NodeValue: Fn(&Value) -> Value,
    LocalCursor: Fn(&Value) -> String,
{
    let ConnectionOverlayInput {
        authoritative,
        local_records,
        tombstones,
        arguments,
        source_page_info,
    } = input;
    let mut rows_by_id = BTreeMap::<String, ObservedConnectionRow>::new();
    for row in authoritative {
        let id = connection_node_identity(&row.node);
        if !tombstones.contains(&id) {
            rows_by_id.insert(id, row);
        }
    }
    for node in local_records {
        let id = connection_node_identity(&node);
        if tombstones.contains(&id) {
            rows_by_id.remove(&id);
            continue;
        }
        let cursor = rows_by_id
            .get(&id)
            .and_then(|row| row.cursor.clone())
            .unwrap_or_else(|| local_cursor(&node));
        rows_by_id.insert(
            id,
            ObservedConnectionRow {
                cursor: Some(cursor),
                node,
            },
        );
    }

    let query = resolved_string_field(arguments, "query");
    let sort_key_name = resolved_string_field(arguments, "sortKey");
    let mut rows = rows_by_id
        .into_values()
        .filter(|row| predicate(&row.node, query.as_deref()) == StagedSearchDecision::Match)
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        sort_key(&left.node, sort_key_name.as_deref())
            .cmp(&sort_key(&right.node, sort_key_name.as_deref()))
            .then_with(|| {
                connection_node_identity(&left.node).cmp(&connection_node_identity(&right.node))
            })
    });
    if resolved_bool_field(arguments, "reverse").unwrap_or(false) {
        rows.reverse();
    }

    if let Some(after) = resolved_string_field(arguments, "after") {
        if let Some(position) = rows
            .iter()
            .position(|row| row.cursor.as_deref() == Some(after.as_str()))
        {
            rows.drain(..=position);
        }
    }
    if let Some(before) = resolved_string_field(arguments, "before") {
        if let Some(position) = rows
            .iter()
            .position(|row| row.cursor.as_deref() == Some(before.as_str()))
        {
            rows.truncate(position);
        }
    }

    let plan = ConnectionOverlayPlan::from_arguments(arguments, 0);
    let source_has_previous = source_page_info
        .get("hasPreviousPage")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let source_has_next = source_page_info
        .get("hasNextPage")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let (window, has_previous_page, has_next_page) = match plan.direction {
        ConnectionOverlayDirection::Forward => {
            let has_next = rows.len() > plan.requested_size || source_has_next;
            let window = rows
                .into_iter()
                .take(plan.requested_size)
                .collect::<Vec<_>>();
            (
                window,
                source_has_previous || arguments.contains_key("after"),
                has_next,
            )
        }
        ConnectionOverlayDirection::Backward => {
            let has_previous = rows.len() > plan.requested_size || source_has_previous;
            let start = rows.len().saturating_sub(plan.requested_size);
            let window = rows.into_iter().skip(start).collect::<Vec<_>>();
            (
                window,
                has_previous,
                source_has_next || arguments.contains_key("before"),
            )
        }
    };
    let start_cursor = window.first().and_then(|row| row.cursor.clone());
    let end_cursor = window.last().and_then(|row| row.cursor.clone());
    let nodes = window
        .iter()
        .map(|row| node_value(&row.node))
        .collect::<Vec<_>>();
    let edges = window
        .iter()
        .zip(nodes.iter())
        .map(|(row, node)| json!({ "cursor": row.cursor, "node": node }))
        .collect::<Vec<_>>();
    json!({
        "nodes": nodes,
        "edges": edges,
        "pageInfo": connection_page_info(
            has_next_page,
            has_previous_page,
            start_cursor,
            end_cursor,
        )
    })
}

/// Read a GraphQL connection without deriving cursors from its nodes. Edges are
/// authoritative when present; `nodes` only fills records omitted from edges.
pub(in crate::proxy) fn observed_connection_rows(connection: &Value) -> Vec<ObservedConnectionRow> {
    let mut rows = Vec::<ObservedConnectionRow>::new();
    let mut indexes = BTreeMap::<String, usize>::new();
    for edge in connection
        .get("edges")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(node) = edge.get("node").filter(|node| node.is_object()) else {
            continue;
        };
        let identity = connection_node_identity(node);
        if let Some(index) = indexes.get(&identity).copied() {
            merge_connection_node_value(&mut rows[index].node, node);
        } else {
            indexes.insert(identity, rows.len());
            rows.push(ObservedConnectionRow {
                cursor: edge
                    .get("cursor")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                node: node.clone(),
            });
        }
    }
    let node_values = connection
        .get("nodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|node| node.is_object())
        .collect::<Vec<_>>();
    let last_node_index = node_values.len().saturating_sub(1);
    let start_cursor = connection
        .pointer("/pageInfo/startCursor")
        .and_then(Value::as_str);
    let end_cursor = connection
        .pointer("/pageInfo/endCursor")
        .and_then(Value::as_str);
    for (index, node) in node_values.into_iter().enumerate() {
        let identity = connection_node_identity(node);
        if let Some(index) = indexes.get(&identity).copied() {
            merge_connection_node_value(&mut rows[index].node, node);
        } else {
            indexes.insert(identity, rows.len());
            let cursor = match (index == 0, index == last_node_index) {
                (true, true) => start_cursor.or(end_cursor),
                (true, false) => start_cursor,
                (false, true) => end_cursor,
                (false, false) => None,
            };
            rows.push(ObservedConnectionRow {
                cursor: cursor.map(str::to_string),
                node: node.clone(),
            });
        }
    }
    rows
}

fn merge_connection_node_value(target: &mut Value, observed: &Value) {
    match (target, observed) {
        (Value::Object(target), Value::Object(observed)) => {
            for (key, value) in observed {
                match target.get_mut(key) {
                    Some(existing) => merge_connection_node_value(existing, value),
                    None => {
                        target.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        (target, observed) => *target = observed.clone(),
    }
}

fn connection_node_identity(node: &Value) -> String {
    node.get("id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| node.to_string())
}

pub(in crate::proxy) fn connection_has_next_page(connection: &Value) -> bool {
    connection
        .pointer("/pageInfo/hasNextPage")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

pub(in crate::proxy) fn connection_end_cursor(connection: &Value) -> Option<String> {
    connection
        .pointer("/pageInfo/endCursor")
        .and_then(Value::as_str)
        .map(str::to_string)
}

pub(in crate::proxy) fn upstream_page_is_complete_baseline(
    connection: &Value,
    arguments: &BTreeMap<String, ResolvedValue>,
) -> bool {
    !arguments.contains_key("after")
        && !arguments.contains_key("before")
        && !arguments.contains_key("last")
        && !resolved_bool_field(arguments, "reverse").unwrap_or(false)
        && connection
            .pointer("/pageInfo/hasNextPage")
            .and_then(Value::as_bool)
            == Some(false)
        && connection
            .pointer("/pageInfo/hasPreviousPage")
            .and_then(Value::as_bool)
            == Some(false)
}

pub(in crate::proxy) fn complete_connection_value(rows: Vec<ObservedConnectionRow>) -> Value {
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
        "pageInfo": connection_page_info(false, false, start_cursor, end_cursor)
    })
}

impl DraftProxy {
    /// Exhaust an upstream forward connection. Callers deliberately omit the
    /// original request's first/last/before/after/reverse window and pass only
    /// its filter/sort variables, so the returned rows are a proven-complete
    /// baseline that can safely receive a staged overlay and one local window.
    pub(in crate::proxy) fn complete_upstream_connection(
        &self,
        request: &Request,
        query: &str,
        operation_name: &str,
        mut variables: serde_json::Map<String, Value>,
        response_pointer: &str,
        initial_page: Option<&Value>,
    ) -> Option<Value> {
        let mut rows = Vec::<ObservedConnectionRow>::new();
        let mut row_indexes = BTreeMap::<String, usize>::new();
        let mut seen_end_cursors = BTreeSet::new();
        let mut page = initial_page.cloned();
        let mut after = initial_page.and_then(connection_end_cursor);

        for _ in 0..10_000 {
            if page.is_none() {
                variables.insert(
                    "after".to_string(),
                    after.clone().map_or(Value::Null, Value::String),
                );
                let response = self.upstream_post(
                    request,
                    json!({
                        "query": query,
                        "operationName": operation_name,
                        "variables": variables
                    }),
                );
                if !(200..300).contains(&response.status) || response.body.get("errors").is_some() {
                    return None;
                }
                page = response.body.pointer(response_pointer).cloned();
            }

            let current_page = page.take()?;
            for row in observed_connection_rows(&current_page) {
                let identity = connection_node_identity(&row.node);
                if let Some(index) = row_indexes.get(&identity).copied() {
                    rows[index] = row;
                } else {
                    row_indexes.insert(identity, rows.len());
                    rows.push(row);
                }
            }
            if !connection_has_next_page(&current_page) {
                return Some(complete_connection_value(rows));
            }
            let end_cursor = connection_end_cursor(&current_page)?;
            if !seen_end_cursors.insert(end_cursor.clone()) {
                return None;
            }
            after = Some(end_cursor);
        }
        None
    }
}

pub(in crate::proxy) fn connection_json_with_cursor<F>(
    nodes: Vec<Value>,
    mut cursor_for: F,
    page_info: Value,
) -> Value
where
    F: FnMut(usize, &Value) -> String,
{
    let edges = nodes
        .iter()
        .enumerate()
        .map(|(index, node)| {
            json!({
                "cursor": cursor_for(index, node),
                "node": node
            })
        })
        .collect::<Vec<_>>();
    json!({ "nodes": nodes, "edges": edges, "pageInfo": page_info })
}

pub(in crate::proxy) fn connection_json_with_empty_edges(nodes: Vec<Value>) -> Value {
    json!({ "nodes": nodes, "edges": [], "pageInfo": empty_page_info() })
}

pub(in crate::proxy) fn connection_json_with_boundary_cursors<F>(
    nodes: Vec<Value>,
    mut cursor_for: F,
) -> Value
where
    F: FnMut(&Value) -> Option<String>,
{
    let start_cursor = nodes.first().and_then(&mut cursor_for);
    let end_cursor = nodes.last().and_then(&mut cursor_for);
    json!({
        "nodes": nodes,
        "pageInfo": connection_page_info(false, false, start_cursor, end_cursor)
    })
}

pub(in crate::proxy) fn connection_json(nodes: Vec<Value>) -> Value {
    connection_json_with_cursor(
        nodes,
        |_, node| {
            node.get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        },
        empty_page_info(),
    )
}

/// Build a complete, selection-independent connection from typed records. The
/// GraphQL engine projects `nodes`, `edges`, and `pageInfo`; domain code only
/// supplies canonical node values and stable cursors.
pub(in crate::proxy) fn typed_connection_value<T, NodeValue, Cursor>(
    records: &[T],
    node_value: NodeValue,
    cursor: Cursor,
    page_info: Value,
) -> Value
where
    NodeValue: Fn(&T) -> Value,
    Cursor: Fn(&T) -> String,
{
    let nodes = records.iter().map(&node_value).collect::<Vec<_>>();
    let edges = records
        .iter()
        .zip(nodes.iter())
        .map(|(record, node)| {
            json!({
                "cursor": cursor(record),
                "node": node,
            })
        })
        .collect::<Vec<_>>();
    json!({ "nodes": nodes, "edges": edges, "pageInfo": page_info })
}

pub(in crate::proxy) fn connection_value_with_args<F>(
    mut nodes: Vec<Value>,
    arguments: &BTreeMap<String, ResolvedValue>,
    mut cursor_for: F,
) -> Value
where
    F: FnMut(&Value) -> String,
{
    if resolved_bool_field(arguments, "reverse").unwrap_or(false) {
        nodes.reverse();
    }
    let (nodes, page_info) = connection_window(&nodes, arguments, &mut cursor_for);
    connection_json_with_cursor(nodes, |_, node| cursor_for(node), page_info)
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(in crate::proxy) enum StagedSortValue {
    Null,
    I64(i64),
    String(String),
    ReverseString(std::cmp::Reverse<String>),
}

pub(in crate::proxy) type StagedSortKey = Vec<StagedSortValue>;

pub(in crate::proxy) fn sorted_indexed_records<T, SortKey, Cursor>(
    records: Vec<T>,
    reverse: bool,
    sort_key: SortKey,
    cursor: Cursor,
) -> Vec<T>
where
    SortKey: Fn(&T, usize) -> StagedSortKey,
    Cursor: Fn(&T) -> String,
{
    let mut indexed = records.into_iter().enumerate().collect::<Vec<_>>();
    indexed.sort_by(|left, right| {
        sort_key(&left.1, left.0)
            .cmp(&sort_key(&right.1, right.0))
            .then_with(|| cursor(&left.1).cmp(&cursor(&right.1)))
    });
    if reverse {
        indexed.reverse();
    }
    indexed.into_iter().map(|(_, record)| record).collect()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::proxy) enum StagedSearchDecision {
    Match,
    NoMatch,
    Unsupported,
}

impl StagedSearchDecision {
    pub(in crate::proxy) fn from_bool(matches: bool) -> Self {
        if matches {
            Self::Match
        } else {
            Self::NoMatch
        }
    }
}

pub(in crate::proxy) struct StagedConnectionResult<T> {
    pub(in crate::proxy) records: Vec<T>,
    pub(in crate::proxy) total_count: usize,
    pub(in crate::proxy) page_info: Value,
}

pub(in crate::proxy) fn staged_connection_query<T, Predicate, SortKey, Cursor>(
    records: Vec<T>,
    arguments: &BTreeMap<String, ResolvedValue>,
    predicate: Predicate,
    sort_key: SortKey,
    cursor: Cursor,
) -> StagedConnectionResult<T>
where
    T: Clone,
    Predicate: Fn(&T, Option<&str>) -> StagedSearchDecision,
    SortKey: Fn(&T, Option<&str>) -> StagedSortKey,
    Cursor: Fn(&T) -> String,
{
    let query = resolved_string_field(arguments, "query");
    let sort_key_name = resolved_string_field(arguments, "sortKey");
    let mut matched = records
        .into_iter()
        .filter(|record| match predicate(record, query.as_deref()) {
            StagedSearchDecision::Match => true,
            StagedSearchDecision::NoMatch | StagedSearchDecision::Unsupported => false,
        })
        .collect::<Vec<_>>();

    matched.sort_by(|left, right| {
        sort_key(left, sort_key_name.as_deref())
            .cmp(&sort_key(right, sort_key_name.as_deref()))
            .then_with(|| cursor(left).cmp(&cursor(right)))
    });

    if resolved_bool_field(arguments, "reverse").unwrap_or(false) {
        matched.reverse();
    }

    let total_count = matched.len();
    let (records, page_info) = connection_window(&matched, arguments, cursor);
    StagedConnectionResult {
        records,
        total_count,
        page_info,
    }
}

/// Filtering, sorting, reversing, and cursor windowing are shared, while output
/// projection is left entirely to the GraphQL executor.
pub(in crate::proxy) fn staged_connection_value_with_args<
    T,
    Predicate,
    SortKey,
    NodeValue,
    Cursor,
>(
    records: Vec<T>,
    arguments: &BTreeMap<String, ResolvedValue>,
    predicate: Predicate,
    sort_key: SortKey,
    node_value: NodeValue,
    cursor: Cursor,
) -> Value
where
    T: Clone,
    Predicate: Fn(&T, Option<&str>) -> StagedSearchDecision,
    SortKey: Fn(&T, Option<&str>) -> StagedSortKey,
    NodeValue: Fn(&T) -> Value,
    Cursor: Fn(&T) -> String + Copy,
{
    let result = staged_connection_query(records, arguments, predicate, sort_key, cursor);
    typed_connection_value(&result.records, node_value, cursor, result.page_info)
}

/// Canonical root-resolver counterpart to `upstream_count_with_staged_delta`.
/// The caller supplies the upstream Count value directly, so domain code does
/// not need a legacy root selection merely to find its response key.
pub(in crate::proxy) fn upstream_count_value_with_staged_delta(
    upstream_count: Option<&Value>,
    staged_delta: isize,
    arguments: &BTreeMap<String, ResolvedValue>,
) -> Option<Value> {
    let upstream_count = upstream_count?;
    let base_count = upstream_count
        .get("count")?
        .as_u64()
        .and_then(|count| usize::try_from(count).ok())?;
    let effective_total = if staged_delta.is_negative() {
        base_count.saturating_sub(staged_delta.unsigned_abs())
    } else {
        base_count.saturating_add(staged_delta as usize)
    };
    let upstream_precision = upstream_count
        .get("precision")
        .and_then(Value::as_str)
        .unwrap_or("EXACT");
    Some(count_with_limit_precision_from_upstream(
        effective_total,
        upstream_precision,
        arguments,
    ))
}

pub(in crate::proxy) fn upstream_count_value_with_effective_total(
    upstream_count: Option<&Value>,
    effective_total: usize,
    arguments: &BTreeMap<String, ResolvedValue>,
) -> Option<Value> {
    let upstream_count = upstream_count?;
    upstream_count.get("count")?.as_u64()?;
    let upstream_precision = upstream_count
        .get("precision")
        .and_then(Value::as_str)
        .unwrap_or("EXACT");
    Some(count_with_limit_precision_from_upstream(
        effective_total,
        upstream_precision,
        arguments,
    ))
}

pub(in crate::proxy) fn count_with_limit_precision_from_upstream(
    effective_total: usize,
    upstream_precision: &str,
    arguments: &BTreeMap<String, ResolvedValue>,
) -> Value {
    let limit = resolved_int_field(arguments, "limit").filter(|limit| *limit >= 0);
    if let Some(limit) = limit {
        let limit = limit as usize;
        if effective_total > limit
            || (effective_total == limit && upstream_precision.eq_ignore_ascii_case("AT_LEAST"))
        {
            return count_object_with_precision(limit, "AT_LEAST");
        }
        return count_object_with_precision(effective_total, "EXACT");
    }

    if upstream_precision.eq_ignore_ascii_case("AT_LEAST") {
        count_object_with_precision(effective_total, "AT_LEAST")
    } else {
        count_object_with_precision(effective_total, "EXACT")
    }
}

pub(in crate::proxy) fn snapshot_count_with_limit_precision(
    count: usize,
    arguments: &BTreeMap<String, ResolvedValue>,
) -> Value {
    match resolved_int_field(arguments, "limit") {
        Some(limit) if limit >= 0 && count as i64 > limit => {
            count_object_with_precision(limit as usize, "AT_LEAST")
        }
        _ => count_object_with_precision(count, "EXACT"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn row(id: &str, rank: i64, cursor: &str) -> ObservedConnectionRow {
        ObservedConnectionRow {
            cursor: Some(cursor.to_string()),
            node: json!({ "id": id, "rank": rank }),
        }
    }

    fn rank_sort(node: &Value, _sort_key: Option<&str>) -> StagedSortKey {
        vec![StagedSortValue::I64(
            node.get("rank").and_then(Value::as_i64).unwrap_or_default(),
        )]
    }

    #[test]
    fn overlay_plan_is_bounded_by_window_impact_and_one_boundary() {
        let forward = BTreeMap::from([("first".to_string(), ResolvedValue::Int(25))]);
        assert_eq!(
            ConnectionOverlayPlan::from_arguments(&forward, 4),
            ConnectionOverlayPlan {
                direction: ConnectionOverlayDirection::Forward,
                requested_size: 25,
                fetch_size: 30,
            }
        );

        let backward = BTreeMap::from([("last".to_string(), ResolvedValue::Int(10))]);
        assert_eq!(
            ConnectionOverlayPlan::from_arguments(&backward, 2),
            ConnectionOverlayPlan {
                direction: ConnectionOverlayDirection::Backward,
                requested_size: 10,
                fetch_size: 13,
            }
        );
    }

    #[test]
    fn local_boundary_is_recognized_and_removed_from_upstream_query() {
        let cursor = stable_local_connection_cursor("customers", "gid://shopify/Customer/1");
        let arguments = BTreeMap::from([
            ("first".to_string(), ResolvedValue::Int(2)),
            ("after".to_string(), ResolvedValue::String(cursor.clone())),
        ]);
        assert!(connection_has_local_boundary("customers", &arguments));
        assert!(!connection_has_local_boundary("markets", &arguments));

        let raw_arguments = BTreeMap::from([
            ("first".to_string(), RawArgumentValue::Int(2)),
            (
                "after".to_string(),
                RawArgumentValue::String(cursor.clone()),
            ),
        ]);
        let request = ConnectionOverlayRequest {
            root_name: "customers",
            arguments: &arguments,
            raw_arguments: &raw_arguments,
            selection: &[],
            variable_definitions: &BTreeMap::new(),
            variables: &BTreeMap::new(),
            required_node_selection: "id",
        };
        let (query, variables) = connection_overlay_query(
            &request,
            ConnectionOverlayPlan::from_arguments(&arguments, 1),
            4,
            None,
        );
        assert!(!query.contains(&cursor));
        assert!(!query.contains("after:"));
        assert!(query.contains("customers(first: 4)"));
        assert!(variables.is_empty());
    }

    #[test]
    fn shopify_cursor_exposes_captured_sort_value_without_replacing_cursor() {
        let cursor = base64::engine::general_purpose::STANDARD
            .encode(json!({ "last_id": 42, "last_value": "live, overlay" }).to_string());
        assert_eq!(
            shopify_connection_cursor_last_value(&cursor),
            Some(json!("live, overlay"))
        );
        assert_eq!(shopify_connection_cursor_last_value("opaque"), None);
    }

    #[test]
    fn migrated_families_refill_large_partial_windows_with_one_bounded_page() {
        for root_name in [
            "customers",
            "marketingActivities",
            "marketingEvents",
            "bulkOperations",
            "discountNodes",
            "automaticDiscountNodes",
            "codeDiscountNodes",
            "markets",
            "catalogs",
            "priceLists",
            "webPresences",
        ] {
            let calls = Arc::new(Mutex::new(Vec::<Value>::new()));
            let captured = Arc::clone(&calls);
            let mut proxy = DraftProxy::new(Config::default()).with_upstream_transport(
                move |request| {
                    let body: Value = serde_json::from_str(&request.body).unwrap();
                    captured.lock().unwrap().push(body);
                    Response {
                        status: 200,
                        headers: BTreeMap::new(),
                        body: json!({
                            "data": {
                                "overlayWindow": {
                                    "edges": [
                                        { "cursor": "opaque-3", "node": { "id": "gid://shopify/Test/3" } },
                                        { "cursor": "opaque-4", "node": { "id": "gid://shopify/Test/4" } }
                                    ],
                                    "pageInfo": {
                                        "hasNextPage": true,
                                        "hasPreviousPage": true,
                                        "startCursor": "opaque-3",
                                        "endCursor": "opaque-4"
                                    }
                                }
                            }
                        }),
                    }
                },
            );
            let arguments = BTreeMap::from([("first".to_string(), ResolvedValue::Int(2))]);
            let raw_arguments = BTreeMap::from([("first".to_string(), RawArgumentValue::Int(2))]);
            let request = Request {
                method: "POST".to_string(),
                path: "/admin/api/2026-04/graphql.json".to_string(),
                ..Request::default()
            };
            let source = json!({
                "edges": [
                    { "cursor": "opaque-1", "node": { "id": "gid://shopify/Test/1" } },
                    { "cursor": "opaque-2", "node": { "id": "gid://shopify/Test/2" } }
                ],
                "pageInfo": {
                    "hasNextPage": true,
                    "hasPreviousPage": false,
                    "startCursor": "opaque-1",
                    "endCursor": "opaque-2"
                }
            });

            let window = proxy.bounded_connection_overlay_window(
                &request,
                ConnectionOverlayRequest {
                    root_name,
                    arguments: &arguments,
                    raw_arguments: &raw_arguments,
                    selection: &[],
                    variable_definitions: &BTreeMap::new(),
                    variables: &BTreeMap::new(),
                    required_node_selection: "id",
                },
                &source,
                1,
            );

            assert_eq!(window["edges"].as_array().unwrap().len(), 4, "{root_name}");
            assert_eq!(window["edges"][0]["cursor"], json!("opaque-1"));
            assert_eq!(window["edges"][3]["cursor"], json!("opaque-4"));
            let calls = calls.lock().unwrap();
            assert_eq!(calls.len(), 1, "{root_name} should fetch one refill page");
            let query = calls[0]["query"].as_str().unwrap();
            assert!(query.contains(&format!("overlayWindow: {root_name}")));
            assert!(query.contains("first: 2"));
            assert!(query.contains("after: \"opaque-2\""));
        }
    }

    #[test]
    fn overlay_refills_tombstone_and_keeps_authoritative_cursor() {
        let arguments = BTreeMap::from([("first".to_string(), ResolvedValue::Int(2))]);
        let tombstones = BTreeSet::from(["gid://shopify/Test/1".to_string()]);
        let value = overlay_connection_value(
            ConnectionOverlayInput {
                authoritative: vec![
                    row("gid://shopify/Test/1", 1, "opaque-1"),
                    row("gid://shopify/Test/2", 2, "opaque-2"),
                    row("gid://shopify/Test/3", 3, "opaque-3"),
                ],
                local_records: vec![
                    json!({ "id": "gid://shopify/Test/2", "rank": 2, "title": "updated" }),
                    json!({ "id": "gid://shopify/Test/local", "rank": 4 }),
                ],
                tombstones: &tombstones,
                arguments: &arguments,
                source_page_info: &json!({ "hasNextPage": true, "hasPreviousPage": false }),
            },
            |_node, _query| StagedSearchDecision::Match,
            rank_sort,
            Value::clone,
            |node| {
                stable_local_connection_cursor(
                    "tests",
                    node.get("id").and_then(Value::as_str).unwrap_or_default(),
                )
            },
        );

        assert_eq!(
            value["edges"][0],
            json!({
                "cursor": "opaque-2",
                "node": {
                    "id": "gid://shopify/Test/2",
                    "rank": 2,
                    "title": "updated"
                }
            })
        );
        assert_eq!(value["edges"][1]["cursor"], json!("opaque-3"));
        assert_eq!(value["pageInfo"]["hasNextPage"], json!(true));
        assert_eq!(value["pageInfo"]["startCursor"], json!("opaque-2"));
    }

    #[test]
    fn backward_overlay_uses_last_window_and_proves_previous_boundary() {
        let arguments = BTreeMap::from([("last".to_string(), ResolvedValue::Int(2))]);
        let value = overlay_connection_value(
            ConnectionOverlayInput {
                authoritative: vec![
                    row("gid://shopify/Test/1", 1, "opaque-1"),
                    row("gid://shopify/Test/2", 2, "opaque-2"),
                    row("gid://shopify/Test/3", 3, "opaque-3"),
                ],
                local_records: Vec::new(),
                tombstones: &BTreeSet::new(),
                arguments: &arguments,
                source_page_info: &json!({ "hasNextPage": false, "hasPreviousPage": true }),
            },
            |_node, _query| StagedSearchDecision::Match,
            rank_sort,
            Value::clone,
            |_node| unreachable!("authoritative rows already have cursors"),
        );

        assert_eq!(
            value["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|node| node["id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            vec!["gid://shopify/Test/2", "gid://shopify/Test/3"]
        );
        assert_eq!(value["pageInfo"]["hasPreviousPage"], json!(true));
        assert_eq!(value["pageInfo"]["endCursor"], json!("opaque-3"));
    }
}

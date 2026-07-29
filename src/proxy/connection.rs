use super::*;

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

/// Read a GraphQL connection without deriving cursors from its nodes. Edges are
/// authoritative when present; `nodes` only fills records omitted from edges.
pub(in crate::proxy) fn observed_connection_rows(connection: &Value) -> Vec<ObservedConnectionRow> {
    let mut rows = Vec::new();
    let mut seen = BTreeSet::new();
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
        if seen.insert(identity) {
            rows.push(ObservedConnectionRow {
                cursor: edge
                    .get("cursor")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                node: node.clone(),
            });
        }
    }
    for node in connection
        .get("nodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|node| node.is_object())
    {
        let identity = connection_node_identity(node);
        if seen.insert(identity) {
            rows.push(ObservedConnectionRow {
                cursor: None,
                node: node.clone(),
            });
        }
    }
    rows
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
    Descending(std::cmp::Reverse<Box<StagedSortValue>>),
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

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

pub(in crate::proxy) fn selected_count_json(
    count: impl serde::Serialize,
    selections: &[SelectedField],
) -> Value {
    selected_json(&count_object(count), selections)
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
pub(in crate::proxy) fn project_seeded_connection(
    connection: &Value,
    arguments: &BTreeMap<String, ResolvedValue>,
    selections: &[SelectedField],
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
    selected_json(&connection, selections)
}

pub(in crate::proxy) fn value_id_cursor(record: &Value) -> String {
    record
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

pub(in crate::proxy) fn connection_nodes(connection: &Value) -> Vec<Value> {
    connection["nodes"].as_array().cloned().unwrap_or_default()
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

pub(in crate::proxy) fn selected_connection_json(
    nodes: Vec<Value>,
    selections: &[SelectedField],
) -> Value {
    selected_json(&connection_json(nodes), selections)
}

pub(in crate::proxy) fn selected_connection_json_with_args<F>(
    mut nodes: Vec<Value>,
    arguments: &BTreeMap<String, ResolvedValue>,
    selections: &[SelectedField],
    mut cursor_for: F,
) -> Value
where
    F: FnMut(&Value) -> String,
{
    if resolved_bool_field(arguments, "reverse").unwrap_or(false) {
        nodes.reverse();
    }
    let (nodes, page_info) = connection_window(&nodes, arguments, &mut cursor_for);
    selected_json(
        &connection_json_with_cursor(nodes, |_, node| cursor_for(node), page_info),
        selections,
    )
}

pub(in crate::proxy) fn selected_empty_connection_json(selections: &[SelectedField]) -> Value {
    selected_connection_json(Vec::new(), selections)
}

pub(in crate::proxy) fn selected_typed_connection<T, NodeJson, Cursor, PageInfo>(
    records: &[T],
    root_selection: &[SelectedField],
    node_json: NodeJson,
    cursor: Cursor,
    page_info: PageInfo,
) -> Value
where
    NodeJson: Fn(&T, &[SelectedField]) -> Value,
    Cursor: Fn(&T) -> String,
    PageInfo: Fn(&[SelectedField]) -> Value,
{
    let node_selection = nested_selected_fields(root_selection, &["nodes"]);
    let edge_node_selection = nested_selected_fields(root_selection, &["edges", "node"]);
    let page_info_selection = nested_selected_fields(root_selection, &["pageInfo"]);
    let mut connection = serde_json::Map::new();
    for selection in root_selection {
        let value = match selection.name.as_str() {
            "nodes" => Some(Value::Array(
                records
                    .iter()
                    .map(|record| node_json(record, &node_selection))
                    .collect(),
            )),
            "edges" => Some(Value::Array(
                records
                    .iter()
                    .map(|record| {
                        json!({
                            "cursor": cursor(record),
                            "node": node_json(record, &edge_node_selection)
                        })
                    })
                    .collect(),
            )),
            "pageInfo" => Some(page_info(&page_info_selection)),
            _ => None,
        };
        if let Some(value) = value {
            connection.insert(selection.response_key.clone(), value);
        }
    }
    Value::Object(connection)
}

pub(in crate::proxy) fn selected_typed_connection_with_page_info<T, NodeJson, Cursor>(
    records: &[T],
    root_selection: &[SelectedField],
    node_json: NodeJson,
    cursor: Cursor,
    page_info: Value,
) -> Value
where
    NodeJson: Fn(&T, &[SelectedField]) -> Value,
    Cursor: Fn(&T) -> String,
{
    selected_typed_connection(records, root_selection, node_json, cursor, |selections| {
        selected_json(&page_info, selections)
    })
}

pub(in crate::proxy) fn selected_typed_connection_with_args<T, NodeJson, Cursor>(
    records: &[T],
    arguments: &BTreeMap<String, ResolvedValue>,
    root_selection: &[SelectedField],
    node_json: NodeJson,
    cursor: Cursor,
) -> Value
where
    T: Clone,
    NodeJson: Fn(&T, &[SelectedField]) -> Value,
    Cursor: Fn(&T) -> String,
{
    let (records, page_info) = connection_window(records, arguments, |record| cursor(record));
    selected_typed_connection_with_page_info(&records, root_selection, node_json, cursor, page_info)
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(in crate::proxy) enum StagedSortValue {
    Null,
    I64(i64),
    String(String),
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

pub(in crate::proxy) fn selected_staged_connection_with_args<
    T,
    Predicate,
    SortKey,
    NodeJson,
    Cursor,
>(
    records: Vec<T>,
    arguments: &BTreeMap<String, ResolvedValue>,
    root_selection: &[SelectedField],
    predicate: Predicate,
    sort_key: SortKey,
    node_json: NodeJson,
    cursor: Cursor,
) -> Value
where
    T: Clone,
    Predicate: Fn(&T, Option<&str>) -> StagedSearchDecision,
    SortKey: Fn(&T, Option<&str>) -> StagedSortKey,
    NodeJson: Fn(&T, &[SelectedField]) -> Value,
    Cursor: Fn(&T) -> String + Copy,
{
    let result = staged_connection_query(records, arguments, predicate, sort_key, cursor);
    selected_typed_connection_with_page_info(
        &result.records,
        root_selection,
        node_json,
        cursor,
        result.page_info,
    )
}

pub(in crate::proxy) fn staged_count_with_limit_precision(
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

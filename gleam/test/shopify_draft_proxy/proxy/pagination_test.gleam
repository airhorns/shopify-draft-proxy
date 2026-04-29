import gleam/dict
import gleam/json
import gleam/option.{None, Some}
import shopify_draft_proxy/graphql/ast
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  type ConnectionWindow, ConnectionPageInfoOptions, SerializeConnectionConfig,
  default_connection_page_info_options, default_connection_window_options,
  default_selected_field_options, paginate_connection_items,
  serialize_connection,
}

fn first_root(document: String) -> ast.Selection {
  let assert Ok(field) = root_field.get_root_field(document)
  field
}

fn id_cursor(item: String, _index: Int) -> String {
  item
}

fn paginate_with(items: List(String), document: String) -> ConnectionWindow(String) {
  let field = first_root(document)
  paginate_connection_items(
    items,
    field,
    dict.new(),
    id_cursor,
    default_connection_window_options(),
  )
}

pub fn paginate_no_args_returns_all_test() {
  let window = paginate_with(["a", "b", "c"], "{ stuff { nodes { id } } }")
  assert window.items == ["a", "b", "c"]
  assert window.has_next_page == False
  assert window.has_previous_page == False
}

pub fn paginate_first_truncates_and_flags_next_test() {
  let window = paginate_with(["a", "b", "c"], "{ stuff(first: 2) { nodes { id } } }")
  assert window.items == ["a", "b"]
  assert window.has_next_page == True
  assert window.has_previous_page == False
}

pub fn paginate_first_equal_to_total_no_next_test() {
  let window = paginate_with(["a", "b"], "{ stuff(first: 2) { nodes { id } } }")
  assert window.items == ["a", "b"]
  assert window.has_next_page == False
}

pub fn paginate_after_skips_through_cursor_test() {
  let window =
    paginate_with(
      ["a", "b", "c", "d"],
      "{ stuff(after: \"cursor:b\") { nodes { id } } }",
    )
  assert window.items == ["c", "d"]
  assert window.has_next_page == False
  assert window.has_previous_page == True
}

pub fn paginate_before_truncates_test() {
  let window =
    paginate_with(
      ["a", "b", "c", "d"],
      "{ stuff(before: \"cursor:c\") { nodes { id } } }",
    )
  assert window.items == ["a", "b"]
  assert window.has_next_page == True
  assert window.has_previous_page == False
}

pub fn paginate_after_first_combine_test() {
  let window =
    paginate_with(
      ["a", "b", "c", "d", "e"],
      "{ stuff(after: \"cursor:a\", first: 2) { nodes { id } } }",
    )
  assert window.items == ["b", "c"]
  assert window.has_next_page == True
  assert window.has_previous_page == True
}

pub fn paginate_last_keeps_tail_test() {
  let window =
    paginate_with(
      ["a", "b", "c", "d"],
      "{ stuff(last: 2) { nodes { id } } }",
    )
  assert window.items == ["c", "d"]
  assert window.has_previous_page == True
}

pub fn paginate_unknown_after_falls_back_to_start_test() {
  // TS: findIndex returning -1 + 1 = 0 → window starts at 0.
  let window =
    paginate_with(
      ["a", "b"],
      "{ stuff(after: \"cursor:zzzz\") { nodes { id } } }",
    )
  assert window.items == ["a", "b"]
  assert window.has_previous_page == False
}

pub fn paginate_raw_string_cursor_unwrapped_test() {
  // No `cursor:` prefix → use the raw value.
  let window =
    paginate_with(
      ["a", "b", "c"],
      "{ stuff(after: \"a\") { nodes { id } } }",
    )
  assert window.items == ["b", "c"]
  assert window.has_previous_page == True
}

pub fn paginate_first_zero_returns_empty_test() {
  let window = paginate_with(["a", "b"], "{ stuff(first: 0) { nodes { id } } }")
  assert window.items == []
  assert window.has_next_page == True
}

// ---------------------------------------------------------------------------
// serialize_connection
// ---------------------------------------------------------------------------

fn node_as_id(item: String, _field: ast.Selection, _index: Int) -> json.Json {
  json.object([#("id", json.string(item))])
}

fn serialize_with(
  document: String,
  items: List(String),
  has_next: Bool,
  has_prev: Bool,
) -> String {
  let field = first_root(document)
  json.to_string(serialize_connection(
    field,
    SerializeConnectionConfig(
      items: items,
      has_next_page: has_next,
      has_previous_page: has_prev,
      get_cursor_value: id_cursor,
      serialize_node: node_as_id,
      selected_field_options: default_selected_field_options(),
      page_info_options: default_connection_page_info_options(),
    ),
  ))
}

pub fn serialize_nodes_only_test() {
  let result =
    serialize_with("{ root { nodes { id } } }", ["a", "b"], False, False)
  assert result == "{\"nodes\":[{\"id\":\"a\"},{\"id\":\"b\"}]}"
}

pub fn serialize_edges_emits_cursor_and_node_test() {
  let result =
    serialize_with(
      "{ root { edges { cursor node { id } } } }",
      ["a", "b"],
      False,
      False,
    )
  assert result
    == "{\"edges\":[{\"cursor\":\"cursor:a\",\"node\":{\"id\":\"a\"}},{\"cursor\":\"cursor:b\",\"node\":{\"id\":\"b\"}}]}"
}

pub fn serialize_page_info_with_cursors_test() {
  let result =
    serialize_with(
      "{ root { pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } }",
      ["a", "b", "c"],
      True,
      False,
    )
  assert result
    == "{\"pageInfo\":{\"hasNextPage\":true,\"hasPreviousPage\":false,\"startCursor\":\"cursor:a\",\"endCursor\":\"cursor:c\"}}"
}

pub fn serialize_page_info_empty_no_cursors_test() {
  let result =
    serialize_with(
      "{ root { pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } }",
      [],
      False,
      False,
    )
  assert result
    == "{\"pageInfo\":{\"hasNextPage\":false,\"hasPreviousPage\":false,\"startCursor\":null,\"endCursor\":null}}"
}

pub fn serialize_unknown_field_is_null_test() {
  let result =
    serialize_with("{ root { totalCount } }", ["a"], False, False)
  assert result == "{\"totalCount\":null}"
}

pub fn serialize_aliases_are_used_as_response_keys_test() {
  let result =
    serialize_with(
      "{ root { ns: nodes { id } } }",
      ["a"],
      False,
      False,
    )
  assert result == "{\"ns\":[{\"id\":\"a\"}]}"
}

pub fn serialize_page_info_no_cursors_when_disabled_test() {
  let field = first_root(
    "{ root { pageInfo { startCursor endCursor } } }",
  )
  let opts =
    ConnectionPageInfoOptions(
      ..default_connection_page_info_options(),
      include_cursors: False,
    )
  let result =
    json.to_string(serialize_connection(
      field,
      SerializeConnectionConfig(
        items: ["a"],
        has_next_page: False,
        has_previous_page: False,
        get_cursor_value: id_cursor,
        serialize_node: node_as_id,
        selected_field_options: default_selected_field_options(),
        page_info_options: opts,
      ),
    ))
  assert result == "{\"pageInfo\":{\"startCursor\":null,\"endCursor\":null}}"
}

pub fn serialize_page_info_falls_back_to_provided_cursors_test() {
  let field = first_root(
    "{ root { pageInfo { startCursor endCursor } } }",
  )
  let opts =
    ConnectionPageInfoOptions(
      ..default_connection_page_info_options(),
      fallback_start_cursor: Some("start_fallback"),
      fallback_end_cursor: Some("end_fallback"),
    )
  let result =
    json.to_string(serialize_connection(
      field,
      SerializeConnectionConfig(
        items: [],
        has_next_page: False,
        has_previous_page: False,
        get_cursor_value: id_cursor,
        serialize_node: node_as_id,
        selected_field_options: default_selected_field_options(),
        page_info_options: opts,
      ),
    ))
  assert result
    == "{\"pageInfo\":{\"startCursor\":\"start_fallback\",\"endCursor\":\"end_fallback\"}}"
}

pub fn serialize_unprefixed_cursor_when_disabled_test() {
  let field = first_root("{ root { edges { cursor } } }")
  let opts =
    ConnectionPageInfoOptions(
      ..default_connection_page_info_options(),
      prefix_cursors: False,
    )
  let result =
    json.to_string(serialize_connection(
      field,
      SerializeConnectionConfig(
        items: ["a", "b"],
        has_next_page: False,
        has_previous_page: False,
        get_cursor_value: id_cursor,
        serialize_node: node_as_id,
        selected_field_options: default_selected_field_options(),
        page_info_options: opts,
      ),
    ))
  assert result == "{\"edges\":[{\"cursor\":\"a\"},{\"cursor\":\"b\"}]}"
}

pub fn build_synthetic_cursor_format_test() {
  assert graphql_helpers.build_synthetic_cursor("xyz") == "cursor:xyz"
}

// Ensure the option stays exported and working.
pub fn no_options_used_in_simple_window_test() {
  // A regression check that ConnectionPageInfoOptions defaults match
  // the TS shape. Specifically: prefix_cursors default = True.
  let opts = default_connection_page_info_options()
  assert opts.prefix_cursors == True
  assert opts.include_cursors == True
  assert opts.fallback_start_cursor == None
  assert opts.fallback_end_cursor == None
}

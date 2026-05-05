import gleam/dict
import gleam/json
import gleam/option.{None, Some}
import shopify_draft_proxy/graphql/ast.{type Selection}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{default_selected_field_options}
import shopify_draft_proxy/proxy/metafields.{
  type MetafieldRecordCore, MetafieldRecordCore,
}

fn record() -> MetafieldRecordCore {
  MetafieldRecordCore(
    id: "gid://shopify/Metafield/1",
    namespace: "custom",
    key: "fabric",
    type_: Some("single_line_text_field"),
    value: Some("cotton"),
    compare_digest: None,
    json_value: None,
    created_at: Some("2024-01-01T00:00:00Z"),
    updated_at: Some("2024-01-02T00:00:00Z"),
    owner_type: Some("PRODUCT"),
  )
}

fn first_root_field(query: String) -> Selection {
  let assert Ok(field) = root_field.get_root_field(query)
  field
}

pub fn compare_digest_is_deterministic_test() {
  let r = record()
  let a = metafields.make_metafield_compare_digest(r)
  let b = metafields.make_metafield_compare_digest(r)
  assert a == b
}

pub fn compare_digest_changes_when_value_changes_test() {
  let r = record()
  let r2 = MetafieldRecordCore(..r, value: Some("linen"))
  assert metafields.make_metafield_compare_digest(r)
    != metafields.make_metafield_compare_digest(r2)
}

pub fn compare_digest_starts_with_draft_prefix_test() {
  let digest = metafields.make_metafield_compare_digest(record())
  assert digest != ""
  // Crude prefix check — can't easily slice here, so check length > 6
  // and the first 6 characters via concatenation.
  let prefix = "draft:"
  assert digest == prefix <> string_drop_first_6(digest)
}

fn string_drop_first_6(s: String) -> String {
  // Helper: reconstruct the suffix by matching a known prefix.
  case s {
    "draft:" <> rest -> rest
    _ -> s
  }
}

pub fn projects_id_namespace_key_test() {
  let field = first_root_field("{ root { id namespace key } }")
  let projection =
    metafields.serialize_metafield_selection(
      record(),
      field,
      default_selected_field_options(),
    )
  assert json.to_string(projection)
    == "{\"id\":\"gid://shopify/Metafield/1\",\"namespace\":\"custom\",\"key\":\"fabric\"}"
}

pub fn projects_typename_test() {
  let field = first_root_field("{ root { __typename id } }")
  let projection =
    metafields.serialize_metafield_selection(
      record(),
      field,
      default_selected_field_options(),
    )
  assert json.to_string(projection)
    == "{\"__typename\":\"Metafield\",\"id\":\"gid://shopify/Metafield/1\"}"
}

pub fn projects_value_and_type_with_nulls_test() {
  let r = MetafieldRecordCore(..record(), type_: None, value: None)
  let field = first_root_field("{ root { type value } }")
  let projection =
    metafields.serialize_metafield_selection(
      r,
      field,
      default_selected_field_options(),
    )
  assert json.to_string(projection) == "{\"type\":null,\"value\":null}"
}

pub fn projects_compare_digest_uses_provided_value_test() {
  let r = MetafieldRecordCore(..record(), compare_digest: Some("draft:cached"))
  let field = first_root_field("{ root { compareDigest } }")
  let projection =
    metafields.serialize_metafield_selection(
      r,
      field,
      default_selected_field_options(),
    )
  assert json.to_string(projection) == "{\"compareDigest\":\"draft:cached\"}"
}

pub fn updated_at_falls_back_to_created_at_test() {
  let r = MetafieldRecordCore(..record(), updated_at: None)
  let field = first_root_field("{ root { updatedAt } }")
  let projection =
    metafields.serialize_metafield_selection(
      r,
      field,
      default_selected_field_options(),
    )
  assert json.to_string(projection)
    == "{\"updatedAt\":\"2024-01-01T00:00:00Z\"}"
}

pub fn unknown_field_returns_null_test() {
  let field = first_root_field("{ root { definition jsonValue } }")
  let projection =
    metafields.serialize_metafield_selection(
      record(),
      field,
      default_selected_field_options(),
    )
  assert json.to_string(projection)
    == "{\"definition\":null,\"jsonValue\":null}"
}

pub fn connection_serializes_first_two_test() {
  let records = [
    record(),
    MetafieldRecordCore(
      ..record(),
      id: "gid://shopify/Metafield/2",
      key: "color",
    ),
  ]
  let field = first_root_field("{ ms(first: 2) { nodes { id key } } }")
  let envelope =
    metafields.serialize_metafields_connection(
      records,
      field,
      dict.new(),
      default_selected_field_options(),
    )
  assert json.to_string(envelope)
    == "{\"nodes\":[{\"id\":\"gid://shopify/Metafield/1\",\"key\":\"fabric\"},{\"id\":\"gid://shopify/Metafield/2\",\"key\":\"color\"}]}"
}

pub fn connection_paginates_test() {
  let records = [
    record(),
    MetafieldRecordCore(
      ..record(),
      id: "gid://shopify/Metafield/2",
      key: "color",
    ),
  ]
  let field =
    first_root_field(
      "{ ms(first: 1) { nodes { id } pageInfo { hasNextPage } } }",
    )
  let envelope =
    metafields.serialize_metafields_connection(
      records,
      field,
      dict.new(),
      default_selected_field_options(),
    )
  assert json.to_string(envelope)
    == "{\"nodes\":[{\"id\":\"gid://shopify/Metafield/1\"}],\"pageInfo\":{\"hasNextPage\":true}}"
}

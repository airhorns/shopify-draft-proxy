//// Draft order tag normalization and validation shared by draft-order roots.

import gleam/dict
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, Some}
import gleam/order
import gleam/string

import shopify_draft_proxy/proxy/orders/common.{
  captured_object_field, inferred_user_error, replace_captured_object_fields,
  user_error,
}
import shopify_draft_proxy/proxy/user_error_codes
import shopify_draft_proxy/state/types.{
  type CapturedJsonValue, type DraftOrderRecord, CapturedArray, CapturedString,
  DraftOrderRecord,
}

const draft_order_tag_limit = 250

const draft_order_bulk_tag_character_limit = 255

const draft_order_input_tag_character_limit = 40

@internal
pub fn draft_order_tags_exceed_graphql_input_limit(tags: List(String)) -> Bool {
  list.length(tags) > draft_order_tag_limit
}

@internal
pub fn draft_order_tags_max_input_size_error(
  root_name: String,
  actual_size: Int,
) -> Json {
  json.object([
    #(
      "message",
      json.string(
        "The input array size of "
        <> int.to_string(actual_size)
        <> " is greater than the maximum allowed of 250.",
      ),
    ),
    #("path", json.array([root_name, "input", "tags"], json.string)),
    #(
      "extensions",
      json.object([
        #("code", json.string("MAX_INPUT_SIZE_EXCEEDED")),
      ]),
    ),
  ])
}

@internal
pub fn draft_order_input_tag_user_errors(
  tags: List(String),
  root_name: String,
) -> List(#(List(String), String, Option(String))) {
  let normalized_tags = valid_draft_order_tag_values(tags)
  let long_tag_errors = draft_order_input_long_tag_errors(tags, root_name)
  let tag_count_errors = case
    list.length(normalized_tags) > draft_order_tag_limit
  {
    True -> [draft_order_too_many_tags_error()]
    False -> []
  }
  list.append(tag_count_errors, long_tag_errors)
}

@internal
pub fn nullable_draft_order_input_tag_user_errors(
  tags: List(String),
  root_name: String,
) -> List(#(Option(List(String)), String, Option(String))) {
  draft_order_input_tag_user_errors(tags, root_name)
  |> list.map(fn(error) {
    let #(field_path, message, code) = error
    #(Some(field_path), message, code)
  })
}

@internal
pub fn draft_order_bulk_tag_input(
  root_name: String,
  tags: List(String),
) -> #(List(String), List(#(List(String), String, Option(String))), Bool) {
  case root_name == "draftOrderBulkDelete" {
    True -> #([], [], False)
    False -> {
      let non_empty_tags = non_empty_draft_order_tag_values(tags)
      let normalized_tags = valid_draft_order_tag_values(tags)
      let long_tag_errors = draft_order_long_tag_errors(tags)
      let too_many_tags =
        list.length(non_empty_tags) > draft_order_tag_limit
        || list.length(normalized_tags) > draft_order_tag_limit
      let blank_errors = case list.is_empty(non_empty_tags) {
        True -> [inferred_user_error(["tags"], "Tags can't be blank")]
        False -> []
      }
      let tag_count_errors = case too_many_tags {
        True -> [draft_order_too_many_tags_error()]
        False -> []
      }
      let user_errors =
        list.append(
          blank_errors,
          list.append(tag_count_errors, long_tag_errors),
        )
      let request_blocked =
        !list.is_empty(blank_errors) || !list.is_empty(tag_count_errors)
      #(normalized_tags, user_errors, request_blocked)
    }
  }
}

@internal
pub fn draft_order_long_tag_errors(
  tags: List(String),
) -> List(#(List(String), String, Option(String))) {
  tags
  |> list.fold(#([], 0), fn(acc, tag) {
    let #(user_errors, index) = acc
    case trimmed_non_empty_draft_order_tag(tag) {
      Ok(trimmed) ->
        case string.length(trimmed) > draft_order_bulk_tag_character_limit {
          True -> #(
            [
              user_error(
                ["input", "tags", int.to_string(index)],
                "tag_too_long",
                Some(user_error_codes.invalid),
              ),
              ..user_errors
            ],
            index + 1,
          )
          False -> #(user_errors, index + 1)
        }
      _ -> #(user_errors, index + 1)
    }
  })
  |> fn(result) {
    let #(user_errors, _) = result
    list.reverse(user_errors)
  }
}

@internal
pub fn draft_order_input_long_tag_errors(
  tags: List(String),
  root_name: String,
) -> List(#(List(String), String, Option(String))) {
  tags
  |> list.fold(#([], 0), fn(acc, tag) {
    let #(user_errors, index) = acc
    case trimmed_non_empty_draft_order_tag(tag) {
      Ok(trimmed) ->
        case string.length(trimmed) > draft_order_input_tag_character_limit {
          True -> #(
            [
              user_error(
                draft_order_input_long_tag_field(root_name, index),
                "Title Tag exceeds the maximum length of 40 characters",
                Some(user_error_codes.invalid),
              ),
              ..user_errors
            ],
            index + 1,
          )
          False -> #(user_errors, index + 1)
        }
      _ -> #(user_errors, index + 1)
    }
  })
  |> fn(result) {
    let #(user_errors, _) = result
    list.reverse(user_errors)
  }
}

@internal
pub fn draft_order_input_long_tag_field(
  root_name: String,
  index: Int,
) -> List(String) {
  case root_name {
    "draftOrderUpdate" -> ["input", "tags", int.to_string(index + 1)]
    _ -> ["tags", int.to_string(index)]
  }
}

@internal
pub fn valid_draft_order_tag_values(tags: List(String)) -> List(String) {
  tags
  |> list.filter_map(fn(tag) {
    case trimmed_non_empty_draft_order_tag(tag) {
      Ok(trimmed) ->
        case string.length(trimmed) <= draft_order_bulk_tag_character_limit {
          True -> Ok(trimmed)
          False -> Error(Nil)
        }
      _ -> Error(Nil)
    }
  })
  |> normalize_draft_order_tags
}

@internal
pub fn non_empty_draft_order_tag_values(tags: List(String)) -> List(String) {
  tags
  |> list.filter_map(trimmed_non_empty_draft_order_tag)
}

@internal
pub fn normalize_draft_order_tags(tags: List(String)) -> List(String) {
  let #(reversed, _) =
    tags
    |> list.filter_map(trimmed_non_empty_draft_order_tag)
    |> list.fold(#([], dict.new()), fn(acc, tag) {
      let #(items, seen) = acc
      let key = draft_order_tag_identity_key(tag)
      case dict.has_key(seen, key) {
        True -> #(items, seen)
        False -> #([tag, ..items], dict.insert(seen, key, True))
      }
    })

  reversed
  |> list.reverse
  |> list.sort(compare_draft_order_tags)
}

@internal
pub fn remove_draft_order_tags_by_identity(
  current_tags: List(String),
  tags_to_remove: List(String),
) -> List(String) {
  let removal_keys = list.map(tags_to_remove, draft_order_tag_identity_key)
  current_tags
  |> normalize_draft_order_tags
  |> list.filter(fn(tag) {
    !list.contains(removal_keys, draft_order_tag_identity_key(tag))
  })
  |> list.sort(compare_draft_order_tags)
}

@internal
pub fn trimmed_non_empty_draft_order_tag(value: String) -> Result(String, Nil) {
  let trimmed = string.trim(value)
  case string.length(trimmed) > 0 {
    True -> Ok(trimmed)
    False -> Error(Nil)
  }
}

@internal
pub fn draft_order_tag_identity_key(tag: String) -> String {
  tag
  |> string.trim
  |> string.lowercase
}

@internal
pub fn compare_draft_order_tags(a: String, b: String) -> order.Order {
  let a_key = draft_order_tag_identity_key(a)
  let b_key = draft_order_tag_identity_key(b)
  case string.compare(a_key, b_key) {
    order.Eq -> string.compare(a, b)
    other -> other
  }
}

@internal
pub fn draft_order_too_many_tags_error() -> #(
  List(String),
  String,
  Option(String),
) {
  user_error(["input", "tags"], "too_many_tags", Some(user_error_codes.invalid))
}

@internal
pub fn append_too_many_draft_order_tags_error(
  user_errors: List(#(List(String), String, Option(String))),
) -> List(#(List(String), String, Option(String))) {
  case
    list.any(user_errors, fn(user_error) {
      let #(_, message, _) = user_error
      message == "too_many_tags"
    })
  {
    True -> user_errors
    False -> [draft_order_too_many_tags_error(), ..user_errors]
  }
}

@internal
pub fn draft_order_tag_count_exceeds_limit(tags: List(String)) -> Bool {
  list.length(tags) > draft_order_tag_limit
}

@internal
pub fn draft_order_tags(data: CapturedJsonValue) -> List(String) {
  case captured_object_field(data, "tags") {
    Some(CapturedArray(items)) ->
      items
      |> list.filter_map(fn(item) {
        case item {
          CapturedString(value) -> Ok(value)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

@internal
pub fn update_draft_order_tags(
  draft_order: DraftOrderRecord,
  tags: List(String),
  mode: String,
) -> DraftOrderRecord {
  let existing = draft_order_tags(draft_order.data)
  let next_tags = case mode {
    "add" -> normalize_draft_order_tags(list.append(existing, tags))
    "remove" -> remove_draft_order_tags_by_identity(existing, tags)
    _ -> existing
  }
  DraftOrderRecord(
    ..draft_order,
    data: replace_captured_object_fields(draft_order.data, [
      #("tags", CapturedArray(list.map(next_tags, CapturedString))),
    ]),
  )
}

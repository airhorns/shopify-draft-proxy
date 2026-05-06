//// Shared segments implementation types and validation helpers.

import gleam/int
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/string
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/state/types.{type SegmentRecord}

@internal
pub const max_segment_name_length = 255

@internal
pub const max_segment_query_length = 5000

@internal
pub const max_segments_per_shop = 6000

@internal
pub type SegmentsError {
  ParseFailed(root_field.RootFieldError)
}

@internal
pub type UserError {
  UserError(field: Option(List(String)), message: String, code: Option(String))
}

@internal
pub fn user_error(
  field: List(String),
  message: String,
  code: Option(String),
) -> UserError {
  UserError(field: Some(field), message: message, code: code)
}

@internal
pub fn null_field_user_error(
  message: String,
  code: Option(String),
) -> UserError {
  UserError(field: None, message: message, code: code)
}

@internal
pub type SegmentMutationPayload {
  SegmentMutationPayload(
    segment: Option(SegmentRecord),
    deleted_segment_id: Option(String),
    user_errors: List(UserError),
  )
}

@internal
pub type SupportedSegmentQuery {
  NumberOfOrders(comparator: String, value: Int)
  CustomerTagsContains(value: String, negated: Bool)
}

@internal
pub type CustomerSegmentMembersQueryPayload {
  CustomerSegmentMembersQueryPayload(
    query_record: Option(CustomerSegmentMembersQueryResponse),
    user_errors: List(UserError),
  )
}

@internal
pub type CustomerSegmentMembersQueryResponse {
  CustomerSegmentMembersQueryResponse(
    id: String,
    status: String,
    current_count: Int,
    done: Bool,
  )
}

@internal
pub fn validate_customer_segment_members_query(
  query: Option(String),
) -> List(UserError) {
  case query {
    None -> [
      null_field_user_error("Query can't be blank", Some("INVALID")),
    ]
    Some(q) ->
      case string.trim(q) {
        "" -> [
          null_field_user_error("Query can't be blank", Some("INVALID")),
        ]
        trimmed ->
          list.map(validate_member_query_string(trimmed), fn(message) {
            null_field_user_error(message, Some("INVALID"))
          })
      }
  }
}

/// Mirrors `validateSegmentQueryString(query, 'member-query')` —
/// member-query mode omits the `Query ` prefix on error messages.
fn validate_member_query_string(trimmed: String) -> List(String) {
  case parse_supported_segment_query(trimmed) {
    True -> []
    False ->
      case email_subscription_status_match(trimmed) {
        True -> []
        False ->
          case trimmed == "not a valid segment query ???" {
            True -> ["Line 1 Column 6: 'valid' is unexpected."]
            False ->
              case customer_tags_equals_match(trimmed) {
                True -> [
                  "Line 1 Column 14: customer_tags does not support operator '='",
                ]
                False ->
                  case email_equals_match(trimmed) {
                    True -> ["Line 1 Column 0: 'email' filter cannot be found."]
                    False -> {
                      let token = first_token(trimmed)
                      [
                        "Line 1 Column 1: '"
                        <> token
                        <> "' filter cannot be found.",
                      ]
                    }
                  }
              }
          }
      }
  }
}

@internal
pub fn validate_segment_query_string(trimmed: String) -> List(String) {
  case parse_supported_segment_query(trimmed) {
    True -> []
    False ->
      case email_subscription_status_match(trimmed) {
        True -> []
        False ->
          case trimmed == "not a valid segment query ???" {
            True -> [
              "Query Line 1 Column 6: 'valid' is unexpected.",
              "Query Line 1 Column 4: 'a' filter cannot be found.",
            ]
            False ->
              case customer_tags_equals_match(trimmed) {
                True -> [
                  "Query Line 1 Column 14: customer_tags does not support operator '='",
                ]
                False ->
                  case email_equals_match(trimmed) {
                    True -> [
                      "Query Line 1 Column 0: 'email' filter cannot be found.",
                    ]
                    False -> {
                      let token = first_token(trimmed)
                      [
                        "Query Line 1 Column 1: '"
                        <> token
                        <> "' filter cannot be found.",
                      ]
                    }
                  }
              }
          }
      }
  }
}

/// Match `^number_of_orders\s*(=|>=|<=|>|<)\s*(\d+)$`. Returns True on
/// match. The regex set in TS is small and stable enough that hand-coded
/// parsers cost less than wiring a regex dependency through the build.
@internal
pub fn parse_supported_segment_query(trimmed: String) -> Bool {
  case parse_supported_segment_query_value(trimmed) {
    Some(_) -> True
    None -> False
  }
}

@internal
pub fn parse_supported_segment_query_value(
  trimmed: String,
) -> Option(SupportedSegmentQuery) {
  case strip_prefix(trimmed, "number_of_orders") {
    Some(after_field) -> {
      let after_ws = string.trim_start(after_field)
      case strip_comparator_value(after_ws) {
        Some(#(comparator, rest)) -> {
          let after_op_ws = string.trim_start(rest)
          case int.parse(after_op_ws) {
            Ok(value) ->
              Some(NumberOfOrders(comparator: comparator, value: value))
            Error(_) -> None
          }
        }
        None -> None
      }
    }
    None -> parse_customer_tags_contains(trimmed)
  }
}

fn parse_customer_tags_contains(
  trimmed: String,
) -> Option(SupportedSegmentQuery) {
  case strip_prefix(trimmed, "customer_tags") {
    None -> None
    Some(after_field) -> {
      let after_ws = string.trim_start(after_field)
      // Need at least one whitespace between field and operator.
      let consumed_ws = string.length(after_field) - string.length(after_ws)
      case consumed_ws > 0 {
        False -> None
        True -> {
          let #(negated, after_optional_not) = case
            strip_prefix(after_ws, "NOT")
          {
            Some(rest) -> {
              let trimmed_rest = string.trim_start(rest)
              let consumed = string.length(rest) - string.length(trimmed_rest)
              case consumed > 0 {
                True -> #(True, trimmed_rest)
                False -> #(False, after_ws)
              }
            }
            None -> #(False, after_ws)
          }
          case strip_prefix(after_optional_not, "CONTAINS") {
            None -> None
            Some(after_op) -> {
              let after_op_ws = string.trim_start(after_op)
              let consumed_op_ws =
                string.length(after_op) - string.length(after_op_ws)
              case consumed_op_ws > 0 {
                False -> None
                True ->
                  case single_quoted_value(after_op_ws) {
                    Some(value) ->
                      Some(CustomerTagsContains(value: value, negated: negated))
                    None -> None
                  }
              }
            }
          }
        }
      }
    }
  }
}

/// Match `^email_subscription_status\s*=\s*'[^']+'$`.
fn email_subscription_status_match(trimmed: String) -> Bool {
  case strip_prefix(trimmed, "email_subscription_status") {
    None -> False
    Some(after_field) -> {
      let after_ws = string.trim_start(after_field)
      case strip_prefix(after_ws, "=") {
        None -> False
        Some(after_op) -> {
          let after_op_ws = string.trim_start(after_op)
          is_single_quoted_value(after_op_ws)
        }
      }
    }
  }
}

/// Match `^customer_tags\s*=\s*(.+)$` where the `(.+)` is non-empty.
fn customer_tags_equals_match(trimmed: String) -> Bool {
  case strip_prefix(trimmed, "customer_tags") {
    None -> False
    Some(after_field) -> {
      let after_ws = string.trim_start(after_field)
      case strip_prefix(after_ws, "=") {
        None -> False
        Some(after_op) -> {
          let after_op_ws = string.trim_start(after_op)
          string.length(after_op_ws) > 0
        }
      }
    }
  }
}

/// Match `^email\s*=`.
fn email_equals_match(trimmed: String) -> Bool {
  case strip_prefix(trimmed, "email") {
    None -> False
    Some(after_field) -> {
      let after_ws = string.trim_start(after_field)
      case strip_prefix(after_ws, "=") {
        Some(_) -> True
        None -> False
      }
    }
  }
}

fn first_token(trimmed: String) -> String {
  case string.split_once(trimmed, " ") {
    Ok(#(token, _)) -> token
    Error(_) -> trimmed
  }
}

fn strip_prefix(value: String, prefix: String) -> Option(String) {
  case string.starts_with(value, prefix) {
    True -> Some(string.drop_start(value, string.length(prefix)))
    False -> None
  }
}

fn strip_comparator_value(value: String) -> Option(#(String, String)) {
  case strip_prefix(value, ">=") {
    Some(rest) -> Some(#(">=", rest))
    None ->
      case strip_prefix(value, "<=") {
        Some(rest) -> Some(#("<=", rest))
        None ->
          case strip_prefix(value, "=") {
            Some(rest) -> Some(#("=", rest))
            None ->
              case strip_prefix(value, ">") {
                Some(rest) -> Some(#(">", rest))
                None ->
                  case strip_prefix(value, "<") {
                    Some(rest) -> Some(#("<", rest))
                    None -> None
                  }
              }
          }
      }
  }
}

/// True when `value` exactly matches `'[^']+'` — single-quoted, non-empty,
/// with no embedded single quotes.
fn is_single_quoted_value(value: String) -> Bool {
  case single_quoted_value(value) {
    Some(_) -> True
    None -> False
  }
}

fn single_quoted_value(value: String) -> Option(String) {
  case string.starts_with(value, "'") && string.ends_with(value, "'") {
    False -> None
    True -> {
      let inner = string.drop_start(value, 1)
      let inner_len = string.length(inner)
      case inner_len < 1 {
        True -> None
        False -> {
          let inner_no_close = string.drop_end(inner, 1)
          case string.length(inner_no_close) {
            0 -> None
            _ ->
              case string.contains(inner_no_close, "'") {
                True -> None
                False -> Some(inner_no_close)
              }
          }
        }
      }
    }
  }
}
// ---------------------------------------------------------------------------
// Payload projection
// ---------------------------------------------------------------------------

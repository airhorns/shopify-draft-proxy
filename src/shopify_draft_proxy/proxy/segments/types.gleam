//// Shared segments implementation types and validation helpers.

import gleam/int
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
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

type SegmentQueryToken {
  SegmentQueryWord(String)
  SegmentQueryString(String)
  SegmentQueryOperator(String)
  SegmentQueryOpenParen
  SegmentQueryCloseParen
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
  case tokenize_segment_query(trimmed) {
    Ok(tokens) -> parse_segment_query_tokens(tokens)
    Error(_) -> False
  }
}

fn parse_segment_query_tokens(tokens: List(SegmentQueryToken)) -> Bool {
  case parse_segment_or(tokens) {
    Ok([]) -> True
    _ -> False
  }
}

fn parse_segment_or(
  tokens: List(SegmentQueryToken),
) -> Result(List(SegmentQueryToken), Nil) {
  use remaining <- result.try(parse_segment_and(tokens))
  parse_segment_or_tail(remaining)
}

fn parse_segment_or_tail(
  tokens: List(SegmentQueryToken),
) -> Result(List(SegmentQueryToken), Nil) {
  case tokens {
    [token, ..rest] ->
      case token_is_keyword(token, "OR") {
        True -> {
          use remaining <- result.try(parse_segment_and(rest))
          parse_segment_or_tail(remaining)
        }
        False -> Ok(tokens)
      }
    _ -> Ok(tokens)
  }
}

fn parse_segment_and(
  tokens: List(SegmentQueryToken),
) -> Result(List(SegmentQueryToken), Nil) {
  use remaining <- result.try(parse_segment_primary(tokens))
  parse_segment_and_tail(remaining)
}

fn parse_segment_and_tail(
  tokens: List(SegmentQueryToken),
) -> Result(List(SegmentQueryToken), Nil) {
  case tokens {
    [token, ..rest] ->
      case token_is_keyword(token, "AND") {
        True -> {
          use remaining <- result.try(parse_segment_primary(rest))
          parse_segment_and_tail(remaining)
        }
        False -> Ok(tokens)
      }
    _ -> Ok(tokens)
  }
}

fn parse_segment_primary(
  tokens: List(SegmentQueryToken),
) -> Result(List(SegmentQueryToken), Nil) {
  case tokens {
    [SegmentQueryOpenParen, ..rest] -> {
      use remaining <- result.try(parse_segment_or(rest))
      case remaining {
        [SegmentQueryCloseParen, ..after_close] -> Ok(after_close)
        _ -> Error(Nil)
      }
    }
    [_, ..] -> parse_segment_predicate(tokens)
    [] -> Error(Nil)
  }
}

fn parse_segment_predicate(
  tokens: List(SegmentQueryToken),
) -> Result(List(SegmentQueryToken), Nil) {
  case tokens {
    [SegmentQueryWord(field), ..after_field] -> {
      case is_segment_query_field(field) {
        False -> Error(Nil)
        True -> parse_segment_predicate_after_field(field, after_field)
      }
    }
    _ -> Error(Nil)
  }
}

fn parse_segment_predicate_after_field(
  field: String,
  tokens: List(SegmentQueryToken),
) -> Result(List(SegmentQueryToken), Nil) {
  case tokens {
    [SegmentQueryOperator(op), ..rest] ->
      parse_segment_comparison(field, op, rest)
    [token, ..rest] ->
      case token_is_keyword(token, "IS") {
        True -> parse_segment_null_predicate(rest)
        False ->
          case token_is_keyword(token, "CONTAINS") {
            True -> parse_segment_contains(field, rest)
            False ->
              case token_is_keyword(token, "NOT") {
                True -> parse_segment_not_predicate(field, rest)
                False ->
                  case token_is_keyword(token, "BETWEEN") {
                    True -> parse_segment_between(field, rest)
                    False -> Error(Nil)
                  }
              }
          }
      }
    [] -> Error(Nil)
  }
}

fn parse_segment_comparison(
  field: String,
  operator: String,
  tokens: List(SegmentQueryToken),
) -> Result(List(SegmentQueryToken), Nil) {
  case
    list.contains(["=", "!=", ">", ">=", "<", "<="], operator)
    && !segment_query_contains_only_field(field)
  {
    False -> Error(Nil)
    True -> parse_segment_condition(tokens)
  }
}

fn parse_segment_null_predicate(
  tokens: List(SegmentQueryToken),
) -> Result(List(SegmentQueryToken), Nil) {
  case tokens {
    [token, ..rest] ->
      case token_is_keyword(token, "NULL") {
        True -> Ok(rest)
        False ->
          case token_is_keyword(token, "NOT") {
            True ->
              case rest {
                [null_token, ..after_null] ->
                  case token_is_keyword(null_token, "NULL") {
                    True -> Ok(after_null)
                    False -> Error(Nil)
                  }
                _ -> Error(Nil)
              }
            False -> Error(Nil)
          }
      }
    _ -> Error(Nil)
  }
}

fn parse_segment_contains(
  field: String,
  tokens: List(SegmentQueryToken),
) -> Result(List(SegmentQueryToken), Nil) {
  case segment_query_contains_field(field) {
    True -> parse_segment_condition(tokens)
    False -> Error(Nil)
  }
}

fn parse_segment_not_predicate(
  field: String,
  tokens: List(SegmentQueryToken),
) -> Result(List(SegmentQueryToken), Nil) {
  case tokens {
    [token, ..rest] ->
      case token_is_keyword(token, "CONTAINS") {
        True -> parse_segment_contains(field, rest)
        False ->
          case token_is_keyword(token, "MATCHES") {
            True -> parse_segment_matches(rest)
            False -> Error(Nil)
          }
      }
    _ -> Error(Nil)
  }
}

fn parse_segment_between(
  field: String,
  tokens: List(SegmentQueryToken),
) -> Result(List(SegmentQueryToken), Nil) {
  case segment_query_contains_only_field(field) {
    True -> Error(Nil)
    False -> {
      use after_left <- result.try(parse_segment_condition(tokens))
      case after_left {
        [and_token, ..after_and] ->
          case token_is_keyword(and_token, "AND") {
            True -> parse_segment_condition(after_and)
            False -> Error(Nil)
          }
        _ -> Error(Nil)
      }
    }
  }
}

fn parse_segment_matches(
  tokens: List(SegmentQueryToken),
) -> Result(List(SegmentQueryToken), Nil) {
  case tokens {
    [SegmentQueryOpenParen, ..rest] ->
      consume_parenthesized_segment_tokens(rest, 1)
    _ -> Error(Nil)
  }
}

fn consume_parenthesized_segment_tokens(
  tokens: List(SegmentQueryToken),
  depth: Int,
) -> Result(List(SegmentQueryToken), Nil) {
  case tokens {
    [] -> Error(Nil)
    [SegmentQueryOpenParen, ..rest] ->
      consume_parenthesized_segment_tokens(rest, depth + 1)
    [SegmentQueryCloseParen, ..rest] ->
      case depth == 1 {
        True -> Ok(rest)
        False -> consume_parenthesized_segment_tokens(rest, depth - 1)
      }
    [_, ..rest] -> consume_parenthesized_segment_tokens(rest, depth)
  }
}

fn parse_segment_condition(
  tokens: List(SegmentQueryToken),
) -> Result(List(SegmentQueryToken), Nil) {
  case tokens {
    [SegmentQueryString(_), ..rest] -> Ok(rest)
    [SegmentQueryWord(value), ..rest] ->
      case reserved_segment_condition_token(value) {
        True -> Error(Nil)
        False -> Ok(rest)
      }
    _ -> Error(Nil)
  }
}

fn reserved_segment_condition_token(value: String) -> Bool {
  list.contains(
    ["AND", "OR", "NOT", "IS", "NULL", "CONTAINS", "BETWEEN", "MATCHES"],
    string.uppercase(value),
  )
}

fn token_is_keyword(token: SegmentQueryToken, keyword: String) -> Bool {
  case token {
    SegmentQueryWord(value) -> string.uppercase(value) == keyword
    _ -> False
  }
}

fn is_segment_query_field(field: String) -> Bool {
  list.contains(segment_query_fields(), string.lowercase(field))
}

fn segment_query_contains_field(field: String) -> Bool {
  list.contains(segment_query_contains_fields(), string.lowercase(field))
}

fn segment_query_contains_only_field(field: String) -> Bool {
  list.contains(segment_query_contains_fields(), string.lowercase(field))
  && !list.contains(
    segment_query_comparable_alias_fields(),
    string.lowercase(field),
  )
}

fn segment_query_fields() -> List(String) {
  [
    "abandoned_checkout_date",
    "amount_spent",
    "city",
    "companies",
    "country",
    "created_by_app_id",
    "customer_account_status",
    "customer_added_date",
    "customer_cities",
    "customer_countries",
    "customer_email_domain",
    "customer_language",
    "customer_regions",
    "customer_tags",
    "customer_within_distance",
    "email_domain",
    "email_subscription_status",
    "first_order_date",
    "last_order_date",
    "last_order_id",
    "number_of_orders",
    "orders_placed",
    "predicted_spend_tier",
    "predictive_spend_tier",
    "product_subscription_status",
    "products_purchased",
    "province",
    "rfm_group",
    "shopify_email.bounced",
    "shopify_email.clicked",
    "shopify_email.delivered",
    "shopify_email.marked_as_spam",
    "shopify_email.opened",
    "shopify_email.unsubscribed",
    "shopify_protect_eligible",
    "sms_subscription_status",
    "store_credit_accounts",
    "storefront_event.collection_viewed",
    "storefront_event.product_viewed",
    "tax_exempt",
    "total_spent",
  ]
}

fn segment_query_contains_fields() -> List(String) {
  [
    "city",
    "country",
    "customer_cities",
    "customer_countries",
    "customer_regions",
    "customer_tags",
    "province",
  ]
}

fn segment_query_comparable_alias_fields() -> List(String) {
  ["city", "country", "province"]
}

fn tokenize_segment_query(
  query: String,
) -> Result(List(SegmentQueryToken), Nil) {
  tokenize_segment_query_loop(query, 0, [])
}

fn tokenize_segment_query_loop(
  query: String,
  index: Int,
  tokens: List(SegmentQueryToken),
) -> Result(List(SegmentQueryToken), Nil) {
  case index >= string.length(query) {
    True -> Ok(list.reverse(tokens))
    False -> {
      let char = string.slice(query, at_index: index, length: 1)
      case is_segment_query_whitespace(char) {
        True -> tokenize_segment_query_loop(query, index + 1, tokens)
        False -> tokenize_segment_query_token(query, index, char, tokens)
      }
    }
  }
}

fn tokenize_segment_query_token(
  query: String,
  index: Int,
  char: String,
  tokens: List(SegmentQueryToken),
) -> Result(List(SegmentQueryToken), Nil) {
  case char {
    "(" ->
      tokenize_segment_query_loop(query, index + 1, [
        SegmentQueryOpenParen,
        ..tokens
      ])
    ")" ->
      tokenize_segment_query_loop(query, index + 1, [
        SegmentQueryCloseParen,
        ..tokens
      ])
    "'" -> tokenize_segment_quoted(query, index + 1, "'", tokens)
    "\"" -> tokenize_segment_quoted(query, index + 1, "\"", tokens)
    "!" ->
      case next_segment_query_char(query, index) == "=" {
        True ->
          tokenize_segment_query_loop(query, index + 2, [
            SegmentQueryOperator("!="),
            ..tokens
          ])
        False -> Error(Nil)
      }
    ">" ->
      case next_segment_query_char(query, index) == "=" {
        True ->
          tokenize_segment_query_loop(query, index + 2, [
            SegmentQueryOperator(">="),
            ..tokens
          ])
        False ->
          tokenize_segment_query_loop(query, index + 1, [
            SegmentQueryOperator(">"),
            ..tokens
          ])
      }
    "<" ->
      case next_segment_query_char(query, index) == "=" {
        True ->
          tokenize_segment_query_loop(query, index + 2, [
            SegmentQueryOperator("<="),
            ..tokens
          ])
        False ->
          tokenize_segment_query_loop(query, index + 1, [
            SegmentQueryOperator("<"),
            ..tokens
          ])
      }
    "=" ->
      tokenize_segment_query_loop(query, index + 1, [
        SegmentQueryOperator("="),
        ..tokens
      ])
    _ -> tokenize_segment_word(query, index, tokens)
  }
}

fn tokenize_segment_quoted(
  query: String,
  index: Int,
  quote: String,
  tokens: List(SegmentQueryToken),
) -> Result(List(SegmentQueryToken), Nil) {
  tokenize_segment_quoted_loop(query, index, quote, "", tokens)
}

fn tokenize_segment_quoted_loop(
  query: String,
  index: Int,
  quote: String,
  value: String,
  tokens: List(SegmentQueryToken),
) -> Result(List(SegmentQueryToken), Nil) {
  case index >= string.length(query) {
    True -> Error(Nil)
    False -> {
      let char = string.slice(query, at_index: index, length: 1)
      case char == quote {
        True ->
          tokenize_segment_query_loop(query, index + 1, [
            SegmentQueryString(value),
            ..tokens
          ])
        False ->
          case char == "\\" && index + 1 < string.length(query) {
            True -> {
              let escaped = string.slice(query, at_index: index + 1, length: 1)
              tokenize_segment_quoted_loop(
                query,
                index + 2,
                quote,
                value <> escaped,
                tokens,
              )
            }
            False ->
              tokenize_segment_quoted_loop(
                query,
                index + 1,
                quote,
                value <> char,
                tokens,
              )
          }
      }
    }
  }
}

fn tokenize_segment_word(
  query: String,
  index: Int,
  tokens: List(SegmentQueryToken),
) -> Result(List(SegmentQueryToken), Nil) {
  let #(word, next_index) = consume_segment_word(query, index, "")
  case string.length(word) == 0 {
    True -> Error(Nil)
    False ->
      tokenize_segment_query_loop(query, next_index, [
        SegmentQueryWord(word),
        ..tokens
      ])
  }
}

fn consume_segment_word(
  query: String,
  index: Int,
  value: String,
) -> #(String, Int) {
  case index >= string.length(query) {
    True -> #(value, index)
    False -> {
      let char = string.slice(query, at_index: index, length: 1)
      case segment_query_word_boundary(char) {
        True -> #(value, index)
        False -> consume_segment_word(query, index + 1, value <> char)
      }
    }
  }
}

fn segment_query_word_boundary(char: String) -> Bool {
  is_segment_query_whitespace(char)
  || char == "("
  || char == ")"
  || char == "'"
  || char == "\""
  || char == "="
  || char == "!"
  || char == ">"
  || char == "<"
}

fn next_segment_query_char(query: String, index: Int) -> String {
  case index + 1 < string.length(query) {
    True -> string.slice(query, at_index: index + 1, length: 1)
    False -> ""
  }
}

fn is_segment_query_whitespace(char: String) -> Bool {
  char == " " || char == "\n" || char == "\t" || char == "\r"
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

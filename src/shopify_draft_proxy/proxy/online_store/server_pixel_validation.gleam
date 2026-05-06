//// Shared validation for Online Store ServerPixel endpoint arguments.

import gleam/list
import gleam/string

@internal
pub fn valid_eventbridge_arn(value: String) -> Bool {
  case string.trim(value) == value && value != "" {
    False -> False
    True ->
      case string.split(value, on: ":") {
        ["arn", "aws", _, region, account_id, ..resource] ->
          arn_region_valid(region)
          && string.length(account_id) == 12
          && all_digits(account_id)
          && resource != []
        _ -> False
      }
  }
}

@internal
pub fn non_blank(value: String) -> Bool {
  string.trim(value) != ""
}

fn arn_region_valid(value: String) -> Bool {
  value != ""
  && list.all(string.to_graphemes(value), fn(grapheme) {
    is_lowercase_alpha(grapheme) || is_digit(grapheme) || grapheme == "-"
  })
}

fn all_digits(value: String) -> Bool {
  value != "" && list.all(string.to_graphemes(value), is_digit)
}

fn is_lowercase_alpha(grapheme: String) -> Bool {
  string.contains("abcdefghijklmnopqrstuvwxyz", grapheme)
}

fn is_digit(grapheme: String) -> Bool {
  string.contains("0123456789", grapheme)
}

//// Shared saved-search types and query parser helpers.

import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/string
import shopify_draft_proxy/search_query_parser.{
  parse_search_query_term, search_query_term_value,
  strip_search_query_value_quotes,
}
import shopify_draft_proxy/state/types.{
  type SavedSearchFilter, SavedSearchFilter,
}

@internal
pub type UserError {
  UserError(field: Option(List(String)), message: String)
}

@internal
pub type ParsedSavedSearchQuery {
  ParsedSavedSearchQuery(
    filters: List(SavedSearchFilter),
    search_terms: String,
    canonical_query: String,
  )
}

/// Mirrors `parseSavedSearchQuery` in `src/proxy/saved-searches.ts`.
/// Splits a raw query into top-level tokens, classifies each as filter
/// vs search-term, and recomputes the canonical stored shape
/// (`<terms> <filters>`).
@internal
pub fn parse_saved_search_query(raw_query: String) -> ParsedSavedSearchQuery {
  let tokens = split_saved_search_top_level_tokens(raw_query)
  let has_boolean_expression =
    list.any(tokens, fn(t) { is_boolean_token(t) || is_grouped_token(t) })
  let #(filters_rev, search_terms_rev) =
    list.fold(tokens, #([], []), fn(acc, token) {
      let #(filters, terms) = acc
      let term = parse_search_query_term(token)
      let take_as_filter = case term.field, term.value {
        Some(_), v if v != "" ->
          is_filter_candidate(token)
          && { term.negated || !has_boolean_expression }
        _, _ -> False
      }
      case take_as_filter, term.field {
        True, Some(field) -> {
          let key = case term.negated {
            True -> field <> "_not"
            False -> field
          }
          let value = filter_value_for_term(token)
          #([SavedSearchFilter(key: key, value: value), ..filters], terms)
        }
        _, _ -> #(filters, [token, ..terms])
      }
    })
  let filters = list.reverse(filters_rev)
  let search_terms_tokens = list.reverse(search_terms_rev)
  let search_terms_text = string.join(search_terms_tokens, " ")
  let stored_search_terms_text =
    search_terms_tokens
    |> list.map(normalize_saved_search_term)
    |> string.join(" ")
  let rendered_filters = list.map(filters, render_saved_search_filter)
  let query_parts = case stored_search_terms_text {
    "" -> rendered_filters
    s -> [s, ..rendered_filters]
  }
  ParsedSavedSearchQuery(
    filters: filters,
    search_terms: normalize_saved_search_quoted_values(search_terms_text),
    canonical_query: string.join(query_parts, " "),
  )
}

@internal
pub fn split_saved_search_top_level_tokens(raw_query: String) -> List(String) {
  let chars = string.to_graphemes(string.trim(raw_query))
  let final_state =
    list.fold(
      chars,
      TokenizerState(current: "", quote: None, depth: 0, tokens: []),
      fn(state, ch) {
        case is_quote(ch) {
          True ->
            case state.quote {
              None ->
                TokenizerState(
                  ..state,
                  current: state.current <> ch,
                  quote: Some(ch),
                )
              Some(open) ->
                case open == ch {
                  True ->
                    TokenizerState(
                      ..state,
                      current: state.current <> ch,
                      quote: None,
                    )
                  False -> TokenizerState(..state, current: state.current <> ch)
                }
            }
          False ->
            case state.quote {
              Some(_) -> TokenizerState(..state, current: state.current <> ch)
              None ->
                case ch {
                  "(" ->
                    TokenizerState(
                      ..state,
                      current: state.current <> ch,
                      depth: state.depth + 1,
                    )
                  ")" ->
                    case state.depth > 0 {
                      True ->
                        TokenizerState(
                          ..state,
                          current: state.current <> ch,
                          depth: state.depth - 1,
                        )
                      False ->
                        TokenizerState(..state, current: state.current <> ch)
                    }
                  _ ->
                    case state.depth == 0 && is_whitespace(ch) {
                      True -> flush_current_token(state)
                      False ->
                        TokenizerState(..state, current: state.current <> ch)
                    }
                }
            }
        }
      },
    )
  let flushed = flush_current_token(final_state)
  list.reverse(flushed.tokens)
}

type TokenizerState {
  TokenizerState(
    current: String,
    quote: Option(String),
    depth: Int,
    tokens: List(String),
  )
}

fn flush_current_token(state: TokenizerState) -> TokenizerState {
  let value = string.trim(state.current)
  case value {
    "" -> TokenizerState(..state, current: "")
    _ -> TokenizerState(..state, current: "", tokens: [value, ..state.tokens])
  }
}

fn is_quote(ch: String) -> Bool {
  ch == "\"" || ch == "'"
}

fn is_whitespace(ch: String) -> Bool {
  ch == " " || ch == "\t" || ch == "\n" || ch == "\r"
}

fn is_grouped_token(token: String) -> Bool {
  string.contains(token, "(") || string.contains(token, ")")
}

fn is_boolean_token(token: String) -> Bool {
  string.uppercase(token) == "OR"
}

fn is_filter_candidate(token: String) -> Bool {
  !is_boolean_token(token) && !is_grouped_token(token)
}

fn filter_value_for_term(token: String) -> String {
  let term = parse_search_query_term(token)
  strip_search_query_value_quotes(search_query_term_value(term))
}

fn render_saved_search_filter(filter: SavedSearchFilter) -> String {
  let negated = string.ends_with(filter.key, "_not")
  let key = case negated {
    True -> string.drop_end(filter.key, 4)
    False -> filter.key
  }
  let value = case contains_whitespace(filter.value) {
    True -> "\"" <> filter.value <> "\""
    False -> filter.value
  }
  let prefix = case negated {
    True -> "-"
    False -> ""
  }
  prefix <> key <> ":" <> value
}

fn contains_whitespace(s: String) -> Bool {
  string.contains(s, " ")
  || string.contains(s, "\t")
  || string.contains(s, "\n")
  || string.contains(s, "\r")
}

fn normalize_saved_search_term(token: String) -> String {
  let normalized = normalize_saved_search_quoted_values(token)
  case
    string.contains(normalized, ":")
    || string.contains(normalized, "\"")
    || is_boolean_token(normalized)
    || is_grouped_token(normalized)
  {
    True -> normalized
    False -> escape_saved_search_term_for_stored_query(normalized)
  }
}

fn escape_saved_search_term_for_stored_query(token: String) -> String {
  string.replace(token, "-", "\\-")
}

fn normalize_saved_search_quoted_values(value: String) -> String {
  let chars = string.to_graphemes(value)
  let final_state =
    list.fold(chars, NormalizeQuotesState(out: "", quote: None), fn(state, ch) {
      case is_quote(ch) {
        True ->
          case state.quote {
            None ->
              NormalizeQuotesState(out: state.out <> "\"", quote: Some(ch))
            Some(open) ->
              case open == ch {
                True ->
                  NormalizeQuotesState(out: state.out <> "\"", quote: None)
                False -> NormalizeQuotesState(..state, out: state.out <> ch)
              }
          }
        False -> NormalizeQuotesState(..state, out: state.out <> ch)
      }
    })
  final_state.out
}

type NormalizeQuotesState {
  NormalizeQuotesState(out: String, quote: Option(String))
}

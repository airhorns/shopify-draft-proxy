//// Shared saved-search types and query parser helpers.

import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/string
import shopify_draft_proxy/search_query_parser.{
  type SearchQueryComparator, type SearchQueryTerm, Equal, GreaterThan,
  GreaterThanOrEqual, LessThan, LessThanOrEqual, parse_search_query_term,
  search_query_term_value, strip_search_query_value_quotes,
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
      let #(projected_filters, terms) = acc
      let term = parse_search_query_term(token)
      let filter_parts = case
        is_filter_candidate(token)
        && case term.negated {
          True -> True
          False -> !has_boolean_expression
        }
      {
        True -> projected_filters_for_term(term)
        False -> []
      }
      case filter_parts {
        [] -> #(projected_filters, [token, ..terms])
        _ -> #(
          list.fold(filter_parts, projected_filters, upsert_projected_filter),
          terms,
        )
      }
    })
  let projected_filters = list.reverse(filters_rev)
  let filters = list.map(projected_filters, fn(projected) { projected.filter })
  let search_terms_tokens = list.reverse(search_terms_rev)
  let search_terms_text = string.join(search_terms_tokens, " ")
  let stored_search_terms_text =
    search_terms_tokens
    |> list.map(normalize_saved_search_term)
    |> string.join(" ")
  let rendered_filters =
    list.map(projected_filters, fn(projected) { projected.canonical_token })
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

type ProjectedSavedSearchFilter {
  ProjectedSavedSearchFilter(filter: SavedSearchFilter, canonical_token: String)
}

fn upsert_projected_filter(
  projected_filters: List(ProjectedSavedSearchFilter),
  projected_filter: ProjectedSavedSearchFilter,
) -> List(ProjectedSavedSearchFilter) {
  let #(replaced, filters) =
    replace_projected_filter(projected_filters, projected_filter)
  case replaced {
    True -> filters
    False -> [projected_filter, ..projected_filters]
  }
}

fn replace_projected_filter(
  projected_filters: List(ProjectedSavedSearchFilter),
  projected_filter: ProjectedSavedSearchFilter,
) -> #(Bool, List(ProjectedSavedSearchFilter)) {
  case projected_filters {
    [] -> #(False, [])
    [existing, ..rest] -> {
      case existing.filter.key == projected_filter.filter.key {
        True -> #(True, [projected_filter, ..rest])
        False -> {
          let #(replaced, rest) =
            replace_projected_filter(rest, projected_filter)
          #(replaced, [existing, ..rest])
        }
      }
    }
  }
}

fn projected_filters_for_term(
  term: SearchQueryTerm,
) -> List(ProjectedSavedSearchFilter) {
  case term.field, term.value {
    Some(field), "*" -> [
      ProjectedSavedSearchFilter(
        filter: SavedSearchFilter(
          key: maybe_negated_filter_key(field, term.negated),
          value: "true",
        ),
        canonical_token: canonical_field_token(field, "*", term.negated),
      ),
    ]
    Some(field), v if v != "" ->
      case term.comparator {
        Some(comparator) ->
          projected_range_filters_for_term(
            field,
            comparator,
            term.value,
            term.negated,
          )
        None -> [
          ProjectedSavedSearchFilter(
            filter: SavedSearchFilter(
              key: maybe_negated_filter_key(field, term.negated),
              value: strip_search_query_value_quotes(search_query_term_value(
                term,
              )),
            ),
            canonical_token: canonical_field_token(
              field,
              strip_search_query_value_quotes(search_query_term_value(term)),
              term.negated,
            ),
          ),
        ]
      }
    _, _ -> []
  }
}

fn projected_range_filters_for_term(
  field: String,
  comparator: SearchQueryComparator,
  value: String,
  negated: Bool,
) -> List(ProjectedSavedSearchFilter) {
  let clean_value = strip_search_query_value_quotes(value)
  case comparator {
    GreaterThan | GreaterThanOrEqual ->
      case negated {
        True -> [
          projected_range_filter(
            field <> "_max",
            clean_value,
            canonical_field_token(
              field,
              negated_upper_comparator(comparator) <> clean_value,
              False,
            ),
          ),
        ]
        False -> [
          projected_range_filter(
            field <> "_min",
            clean_value,
            canonical_field_token(
              field,
              comparator_to_string(comparator) <> clean_value,
              False,
            ),
          ),
        ]
      }
    LessThan | LessThanOrEqual ->
      case negated {
        True -> [
          projected_range_filter(
            field <> "_min",
            clean_value,
            canonical_field_token(
              field,
              negated_lower_comparator(comparator) <> clean_value,
              False,
            ),
          ),
        ]
        False -> [
          projected_range_filter(
            field <> "_max",
            clean_value,
            canonical_field_token(
              field,
              comparator_to_string(comparator) <> clean_value,
              False,
            ),
          ),
        ]
      }
    Equal -> [
      ProjectedSavedSearchFilter(
        filter: SavedSearchFilter(
          key: maybe_negated_filter_key(field, negated),
          value: strip_search_query_value_quotes("=" <> clean_value),
        ),
        canonical_token: canonical_field_token(
          field,
          "=" <> clean_value,
          negated,
        ),
      ),
    ]
  }
}

fn projected_range_filter(
  key: String,
  value: String,
  canonical_token: String,
) -> ProjectedSavedSearchFilter {
  ProjectedSavedSearchFilter(
    filter: SavedSearchFilter(key: key, value: value),
    canonical_token: canonical_token,
  )
}

fn maybe_negated_filter_key(key: String, negated: Bool) -> String {
  case negated {
    True -> key <> "_not"
    False -> key
  }
}

fn canonical_field_token(
  field: String,
  raw_value: String,
  negated: Bool,
) -> String {
  let prefix = case negated {
    True -> "-"
    False -> ""
  }
  prefix <> field <> ":" <> quote_filter_value(raw_value)
}

fn quote_filter_value(value: String) -> String {
  case contains_whitespace(value) {
    True -> "\"" <> value <> "\""
    False -> value
  }
}

fn comparator_to_string(comparator: SearchQueryComparator) -> String {
  case comparator {
    LessThan -> "<"
    LessThanOrEqual -> "<="
    GreaterThan -> ">"
    GreaterThanOrEqual -> ">="
    Equal -> "="
  }
}

fn negated_lower_comparator(comparator: SearchQueryComparator) -> String {
  case comparator {
    LessThan -> ">="
    LessThanOrEqual -> ">"
    _ -> comparator_to_string(comparator)
  }
}

fn negated_upper_comparator(comparator: SearchQueryComparator) -> String {
  case comparator {
    GreaterThan -> "<="
    GreaterThanOrEqual -> "<"
    _ -> comparator_to_string(comparator)
  }
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

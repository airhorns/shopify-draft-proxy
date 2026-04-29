//// Mirrors `src/search-query-parser.ts`.
////
//// Shopify Admin's "search query" string is a small DSL: terms,
//// optional fields, comparators, parens for grouping, OR keyword,
//// optional NOT keyword, and prefix `-` for negation. Domain handlers
//// run user-supplied query strings against in-memory record lists, so
//// every domain that accepts a `query: "..."` argument depends on this
//// module.
////
//// The TS parser is a hand-rolled recursive-descent over a tiny
//// tokenizer. The Gleam port mirrors it: tokenizer first, then a
//// pratt-style cascade `parse_or → parse_and → parse_unary`. The
//// result type is a recursive `SearchQueryNode` ADT.
////
//// Generic `apply_*` and `matches_*` helpers stay parametric over the
//// item type and take a positive-term matcher callback, mirroring the
//// TS `SearchQueryTermMatcher<T>` signature.

import gleam/float
import gleam/int
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/string
import shopify_draft_proxy/state/iso_timestamp

/// Mirrors `SearchQueryComparator`.
pub type SearchQueryComparator {
  LessThanOrEqual
  GreaterThanOrEqual
  LessThan
  GreaterThan
  Equal
}

/// Mirrors `SearchQueryTerm`.
pub type SearchQueryTerm {
  SearchQueryTerm(
    raw: String,
    negated: Bool,
    field: Option(String),
    comparator: Option(SearchQueryComparator),
    value: String,
  )
}

/// Mirrors `SearchQueryNode`.
pub type SearchQueryNode {
  TermNode(term: SearchQueryTerm)
  AndNode(children: List(SearchQueryNode))
  OrNode(children: List(SearchQueryNode))
  NotNode(child: SearchQueryNode)
}

/// Mirrors `SearchQueryParseOptions`. Defaults via
/// `default_parse_options()`.
pub type SearchQueryParseOptions {
  SearchQueryParseOptions(
    quote_characters: List(String),
    recognize_not_keyword: Bool,
    preserve_quotes_in_terms: Bool,
  )
}

pub fn default_parse_options() -> SearchQueryParseOptions {
  SearchQueryParseOptions(
    quote_characters: ["\"", "'"],
    recognize_not_keyword: False,
    preserve_quotes_in_terms: False,
  )
}

/// Mirrors `SearchQueryTermListOptions` *and*
/// `SearchQueryTermListParseOptions`. The TS keeps them separate; in
/// Gleam we collapse them into one record with `drop_empty_values` so
/// callers don't have to construct two record types — when calling
/// `parse_search_query_terms` the field is ignored.
pub type SearchQueryTermListOptions {
  SearchQueryTermListOptions(
    quote_characters: List(String),
    preserve_quotes_in_terms: Bool,
    ignored_keywords: List(String),
    drop_empty_values: Bool,
  )
}

pub fn default_term_list_options() -> SearchQueryTermListOptions {
  SearchQueryTermListOptions(
    quote_characters: ["\"", "'"],
    preserve_quotes_in_terms: False,
    ignored_keywords: [],
    drop_empty_values: False,
  )
}

/// Mirrors `SearchQueryStringMatchMode`.
pub type SearchQueryStringMatchMode {
  ExactMatch
  IncludesMatch
}

/// Mirrors `SearchQueryStringMatchOptions`.
pub type SearchQueryStringMatchOptions {
  SearchQueryStringMatchOptions(word_prefix: Bool)
}

pub fn default_string_match_options() -> SearchQueryStringMatchOptions {
  SearchQueryStringMatchOptions(word_prefix: False)
}

const default_quote_chars: List(String) = ["\"", "'"]

// ----------- Term parsing -----------

/// Mirrors `parseSearchQueryTerm`. Trims the input, strips a leading
/// `-` for negation, and splits on the first `:` into field +
/// (optional comparator) + value.
pub fn parse_search_query_term(raw_value: String) -> SearchQueryTerm {
  let raw = string.trim(raw_value)
  let negated = case string.starts_with(raw, "-") && string.length(raw) > 1 {
    True -> True
    False -> False
  }
  let normalized_raw = case negated {
    True -> string.trim(string.drop_start(raw, 1))
    False -> raw
  }
  case string.split_once(normalized_raw, ":") {
    Error(_) ->
      SearchQueryTerm(
        raw: raw,
        negated: negated,
        field: None,
        comparator: None,
        value: normalized_raw,
      )
    Ok(#(field, after)) -> {
      let trimmed_after = trim_start(after)
      let #(comparator, value_part) = consume_comparator(trimmed_after)
      SearchQueryTerm(
        raw: raw,
        negated: negated,
        field: Some(field),
        comparator: comparator,
        value: trim_start(value_part),
      )
    }
  }
}

fn consume_comparator(s: String) -> #(Option(SearchQueryComparator), String) {
  // Order matters: longest prefixes first so "<=" / ">=" don't get
  // shadowed by "<" / ">".
  case string.starts_with(s, "<=") {
    True -> #(Some(LessThanOrEqual), string.drop_start(s, 2))
    False ->
      case string.starts_with(s, ">=") {
        True -> #(Some(GreaterThanOrEqual), string.drop_start(s, 2))
        False ->
          case string.starts_with(s, "<") {
            True -> #(Some(LessThan), string.drop_start(s, 1))
            False ->
              case string.starts_with(s, ">") {
                True -> #(Some(GreaterThan), string.drop_start(s, 1))
                False ->
                  case string.starts_with(s, "=") {
                    True -> #(Some(Equal), string.drop_start(s, 1))
                    False -> #(None, s)
                  }
              }
          }
      }
  }
}

/// Mirrors `normalizeSearchQueryValue`. Trim, strip leading/trailing
/// quote, lowercase. Note the TS regex `^['"]|['"]$` strips ONE char
/// from each end if it's a quote — not a balanced pair check.
pub fn normalize_search_query_value(value: String) -> String {
  value
  |> string.trim
  |> strip_one_leading_or_trailing_quote
  |> string.lowercase
}

fn strip_one_leading_or_trailing_quote(s: String) -> String {
  let s = case string.starts_with(s, "\"") || string.starts_with(s, "'") {
    True -> string.drop_start(s, 1)
    False -> s
  }
  case string.ends_with(s, "\"") || string.ends_with(s, "'") {
    True -> string.drop_end(s, 1)
    False -> s
  }
}

/// Mirrors `stripSearchQueryValueQuotes`. Strips a *balanced* pair of
/// matching quotes from a trimmed string.
pub fn strip_search_query_value_quotes(value: String) -> String {
  let trimmed = string.trim(value)
  case string.length(trimmed) >= 2 {
    False -> trimmed
    True -> {
      let first = string.slice(trimmed, 0, 1)
      let last = string.slice(trimmed, string.length(trimmed) - 1, 1)
      case { first == "\"" || first == "'" } && first == last {
        True -> string.slice(trimmed, 1, string.length(trimmed) - 2)
        False -> trimmed
      }
    }
  }
}

/// Mirrors `searchQueryTermValue`. Returns the value with the
/// comparator prefix re-attached.
pub fn search_query_term_value(term: SearchQueryTerm) -> String {
  case term.comparator {
    None -> term.value
    Some(c) -> comparator_to_string(c) <> term.value
  }
}

fn comparator_to_string(c: SearchQueryComparator) -> String {
  case c {
    LessThanOrEqual -> "<="
    GreaterThanOrEqual -> ">="
    LessThan -> "<"
    GreaterThan -> ">"
    Equal -> "="
  }
}

// ----------- Match helpers -----------

/// Mirrors `matchesSearchQueryString`.
pub fn matches_search_query_string(
  candidate: Option(String),
  raw_value: String,
  match_mode: SearchQueryStringMatchMode,
  options: SearchQueryStringMatchOptions,
) -> Bool {
  let value = string.lowercase(strip_search_query_value_quotes(raw_value))
  case value {
    "" -> True
    _ -> {
      let prefix_mode = string.ends_with(value, "*")
      let normalized_value = case prefix_mode {
        True -> string.drop_end(value, 1)
        False -> value
      }
      case normalized_value {
        "" -> True
        _ -> {
          let normalized_candidate = case candidate {
            Some(c) -> string.lowercase(c)
            None -> ""
          }
          case prefix_mode {
            True ->
              case string.starts_with(normalized_candidate, normalized_value) {
                True -> True
                False ->
                  case options.word_prefix {
                    True ->
                      list.any(
                        split_alphanumeric_words(normalized_candidate),
                        fn(part) { string.starts_with(part, normalized_value) },
                      )
                    False -> False
                  }
              }
            False ->
              case match_mode {
                ExactMatch -> normalized_candidate == normalized_value
                IncludesMatch ->
                  string.contains(normalized_candidate, normalized_value)
              }
          }
        }
      }
    }
  }
}

/// Split lowercase string on runs of non-`[a-z0-9]` characters.
/// Mirrors the JS `value.split(/[^a-z0-9]+/u)`.
fn split_alphanumeric_words(value: String) -> List(String) {
  // Walk graphemes: alphanumeric chars accumulate into the current
  // word; everything else flushes. Discard empty parts to match
  // `split` semantics on consecutive separators.
  let graphemes = string.to_graphemes(value)
  let #(parts, current) =
    list.fold(graphemes, #([], ""), fn(acc, g) {
      let #(parts, current) = acc
      case is_alnum_lower(g) {
        True -> #(parts, current <> g)
        False ->
          case current {
            "" -> #(parts, "")
            _ -> #([current, ..parts], "")
          }
      }
    })
  let parts = case current {
    "" -> parts
    _ -> [current, ..parts]
  }
  list.reverse(parts)
}

fn is_alnum_lower(g: String) -> Bool {
  case g {
    "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" -> True
    "a" | "b" | "c" | "d" | "e" | "f" | "g" | "h" | "i" | "j" -> True
    "k" | "l" | "m" | "n" | "o" | "p" | "q" | "r" | "s" | "t" -> True
    "u" | "v" | "w" | "x" | "y" | "z" -> True
    _ -> False
  }
}

/// Mirrors `matchesSearchQueryNumber`.
pub fn matches_search_query_number(
  value: Option(Float),
  term: SearchQueryTerm,
) -> Bool {
  let normalized = normalize_search_query_value(term.value)
  case parse_number(normalized), value {
    Ok(expected), Some(actual) ->
      compare_with_comparator(actual, expected, term.comparator)
    _, _ -> False
  }
}

fn compare_with_comparator(
  actual: Float,
  expected: Float,
  comparator: Option(SearchQueryComparator),
) -> Bool {
  case option.unwrap(comparator, Equal) {
    GreaterThan -> actual >. expected
    GreaterThanOrEqual -> actual >=. expected
    LessThan -> actual <. expected
    LessThanOrEqual -> actual <=. expected
    Equal -> actual == expected
  }
}

fn parse_number(raw: String) -> Result(Float, Nil) {
  case float.parse(raw) {
    Ok(f) -> Ok(f)
    Error(_) ->
      case int.parse(raw) {
        Ok(n) -> Ok(int.to_float(n))
        Error(_) -> Error(Nil)
      }
  }
}

/// Mirrors `matchesSearchQueryText`. Substring match on lowercased
/// value, ignoring the comparator entirely (TS does the same).
pub fn matches_search_query_text(
  value: Option(String),
  term: SearchQueryTerm,
) -> Bool {
  case value {
    None -> False
    Some("") -> False
    Some(v) -> {
      let needle = normalize_search_query_value(term.value)
      string.contains(string.lowercase(v), needle)
    }
  }
}

/// Mirrors `matchesSearchQueryDate`. The TS uses `Date.parse`; we use
/// the existing `iso_timestamp.parse_iso` FFI which wraps the same
/// platform native on JS and `calendar:rfc3339_to_system_time` on
/// Erlang.
pub fn matches_search_query_date(
  value: Option(String),
  term: SearchQueryTerm,
  now_ms: Int,
) -> Bool {
  case value {
    None -> False
    Some("") -> False
    Some(v) ->
      case iso_timestamp.parse_iso(v) {
        Error(_) -> False
        Ok(actual) -> {
          let expected_value = normalize_search_query_value(term.value)
          let expected_result = case expected_value {
            "now" -> Ok(now_ms)
            _ -> iso_timestamp.parse_iso(expected_value)
          }
          case expected_result {
            Error(_) -> False
            Ok(expected) ->
              compare_with_comparator_int(actual, expected, term.comparator)
          }
        }
      }
  }
}

fn compare_with_comparator_int(
  actual: Int,
  expected: Int,
  comparator: Option(SearchQueryComparator),
) -> Bool {
  case option.unwrap(comparator, Equal) {
    GreaterThan -> actual > expected
    GreaterThanOrEqual -> actual >= expected
    LessThan -> actual < expected
    LessThanOrEqual -> actual <= expected
    Equal -> actual == expected
  }
}

// ----------- Term list parsing (no boolean operators) -----------

/// Mirrors `parseSearchQueryTerms`. Splits on whitespace outside
/// quotes, dropping ignored keywords, returning a flat term list.
pub fn parse_search_query_terms(
  query: String,
  options: SearchQueryTermListOptions,
) -> List(SearchQueryTerm) {
  let quote_chars = case options.quote_characters {
    [] -> default_quote_chars
    qs -> qs
  }
  let ignored = list.map(options.ignored_keywords, string.uppercase)
  let graphemes = string.to_graphemes(query)
  let initial =
    TermListState(
      terms: [],
      current: "",
      quote: None,
      preserve_quotes: options.preserve_quotes_in_terms,
      quote_chars: quote_chars,
      ignored_upper: ignored,
    )
  let final = list.fold(graphemes, initial, term_list_step)
  let final = flush_term_list(final)
  list.reverse(final.terms)
}

type TermListState {
  TermListState(
    terms: List(SearchQueryTerm),
    current: String,
    quote: Option(String),
    preserve_quotes: Bool,
    quote_chars: List(String),
    ignored_upper: List(String),
  )
}

fn term_list_step(state: TermListState, g: String) -> TermListState {
  case is_quote(g, state.quote_chars) {
    True -> {
      let new_quote = case state.quote {
        Some(q) ->
          case q == g {
            True -> None
            False -> Some(q)
          }
        None -> Some(g)
      }
      let new_current = case state.preserve_quotes {
        True -> state.current <> g
        False -> state.current
      }
      TermListState(..state, quote: new_quote, current: new_current)
    }
    False ->
      case state.quote, is_whitespace(g) {
        None, True -> flush_term_list(state)
        _, _ -> TermListState(..state, current: state.current <> g)
      }
  }
}

fn flush_term_list(state: TermListState) -> TermListState {
  let trimmed = string.trim(state.current)
  case trimmed {
    "" -> TermListState(..state, current: "")
    _ -> {
      let upper = string.uppercase(trimmed)
      case list.contains(state.ignored_upper, upper) {
        True -> TermListState(..state, current: "")
        False ->
          TermListState(..state, current: "", terms: [
            parse_search_query_term(trimmed),
            ..state.terms
          ])
      }
    }
  }
}

fn is_whitespace(g: String) -> Bool {
  case g {
    " " | "\t" | "\n" | "\r" -> True
    _ -> False
  }
}

fn is_quote(g: String, quote_chars: List(String)) -> Bool {
  list.contains(quote_chars, g)
}

/// Mirrors `parseSearchQueryTermList`. Accepts an `Option(String)`
/// (TS uses `unknown`, but the only case we care about is "string or
/// not-a-string-which-is-treated-as-empty"). Empty / whitespace-only
/// inputs return `[]`.
pub fn parse_search_query_term_list(
  raw_query: Option(String),
  options: SearchQueryTermListOptions,
) -> List(SearchQueryTerm) {
  case raw_query {
    None -> []
    Some(q) ->
      case string.trim(q) {
        "" -> []
        trimmed -> {
          let terms = parse_search_query_terms(trimmed, options)
          case options.drop_empty_values {
            False -> terms
            True ->
              list.filter(terms, fn(term) {
                string.length(normalize_search_query_value(term.value)) > 0
              })
          }
        }
      }
  }
}

// ----------- Tokenizer + recursive-descent parser -----------

type SearchQueryToken {
  TermToken(value: String)
  OrToken
  LParenToken
  RParenToken
  NotToken
}

fn tokenize(
  query: String,
  options: SearchQueryParseOptions,
) -> List(SearchQueryToken) {
  let graphemes = string.to_graphemes(query)
  // We need lookahead for the `-(` → NOT token rewrite, so walk an
  // index over the grapheme list rather than using a fold.
  let tokens = tokenize_loop(graphemes, [], "", None, options)
  list.reverse(tokens)
}

fn tokenize_loop(
  graphemes: List(String),
  tokens: List(SearchQueryToken),
  current: String,
  quote: Option(String),
  options: SearchQueryParseOptions,
) -> List(SearchQueryToken) {
  case graphemes {
    [] -> flush_token(tokens, current, options)
    [g, ..rest] ->
      case is_quote(g, options.quote_characters) {
        True ->
          case
            quote_state_can_toggle(g, current, quote, options.quote_characters)
          {
            True -> {
              let new_quote = case quote {
                Some(q) ->
                  case q == g {
                    True -> None
                    False -> Some(q)
                  }
                None -> Some(g)
              }
              let new_current = case options.preserve_quotes_in_terms {
                True -> current <> g
                False -> current
              }
              tokenize_loop(rest, tokens, new_current, new_quote, options)
            }
            False -> tokenize_loop(rest, tokens, current <> g, quote, options)
          }
        False ->
          case quote {
            Some(_) -> tokenize_loop(rest, tokens, current <> g, quote, options)
            None -> tokenize_unquoted_step(g, rest, tokens, current, options)
          }
      }
  }
}

/// In the JS, a quote character can only START a quoted region when
/// the buffer is empty OR ends with `:` optionally followed by a
/// comparator. Closing a quote always works. This helper encapsulates
/// that asymmetry.
fn quote_state_can_toggle(
  g: String,
  current: String,
  quote: Option(String),
  quote_chars: List(String),
) -> Bool {
  case quote {
    Some(q) -> q == g
    None ->
      case can_start_quoted_value(current) {
        True -> True
        False -> False
      }
      // Avoid an unused-variable warning on `quote_chars`.
      |> fn(b) {
        let _ = quote_chars
        let _ = g
        b
      }
  }
}

/// Mirrors `canStartQuotedValue`. The JS regex is
/// `/:(?:<=|>=|<|>|=)?$/u` — i.e. ends with `:` optionally followed by
/// a comparator. Walk the suffixes manually to avoid pulling in regex.
fn can_start_quoted_value(current: String) -> Bool {
  case current {
    "" -> True
    _ ->
      string.ends_with(current, ":")
      || string.ends_with(current, ":<=")
      || string.ends_with(current, ":>=")
      || string.ends_with(current, ":<")
      || string.ends_with(current, ":>")
      || string.ends_with(current, ":=")
  }
}

fn tokenize_unquoted_step(
  g: String,
  rest: List(String),
  tokens: List(SearchQueryToken),
  current: String,
  options: SearchQueryParseOptions,
) -> List(SearchQueryToken) {
  case is_whitespace(g) {
    True -> {
      let tokens = flush_token(tokens, current, options)
      tokenize_loop(rest, tokens, "", None, options)
    }
    False ->
      case g {
        "(" -> {
          let tokens = flush_token(tokens, current, options)
          tokenize_loop(rest, [LParenToken, ..tokens], "", None, options)
        }
        ")" -> {
          let tokens = flush_token(tokens, current, options)
          tokenize_loop(rest, [RParenToken, ..tokens], "", None, options)
        }
        "-" ->
          case current {
            "" ->
              case list.first(rest) {
                Ok("(") ->
                  // Drop the `-`, emit a NOT token, leave the `(` for
                  // the next loop iteration.
                  tokenize_loop(rest, [NotToken, ..tokens], "", None, options)
                _ -> tokenize_loop(rest, tokens, current <> g, None, options)
              }
            _ -> tokenize_loop(rest, tokens, current <> g, None, options)
          }
        _ -> tokenize_loop(rest, tokens, current <> g, None, options)
      }
  }
}

fn flush_token(
  tokens: List(SearchQueryToken),
  current: String,
  options: SearchQueryParseOptions,
) -> List(SearchQueryToken) {
  let trimmed = string.trim(current)
  case trimmed {
    "" -> tokens
    _ -> {
      let upper = string.uppercase(trimmed)
      case upper {
        "OR" -> [OrToken, ..tokens]
        "NOT" ->
          case options.recognize_not_keyword {
            True -> [NotToken, ..tokens]
            False -> [TermToken(value: trimmed), ..tokens]
          }
        _ -> [TermToken(value: trimmed), ..tokens]
      }
    }
  }
}

/// Mirrors `parseSearchQuery`. Returns `None` for empty token streams.
pub fn parse_search_query(
  query: String,
  options: SearchQueryParseOptions,
) -> Option(SearchQueryNode) {
  let tokens = tokenize(query, options)
  case tokens {
    [] -> None
    _ -> {
      let #(node, _rest) = parse_or_expression(tokens)
      node
    }
  }
}

fn parse_or_expression(
  tokens: List(SearchQueryToken),
) -> #(Option(SearchQueryNode), List(SearchQueryToken)) {
  case parse_and_expression(tokens) {
    #(None, rest) -> #(None, rest)
    #(Some(first), rest) -> {
      let #(extra, rest) = parse_or_tail(rest, [])
      case extra {
        [] -> #(Some(first), rest)
        _ -> #(Some(OrNode(children: [first, ..list.reverse(extra)])), rest)
      }
    }
  }
}

fn parse_or_tail(
  tokens: List(SearchQueryToken),
  acc: List(SearchQueryNode),
) -> #(List(SearchQueryNode), List(SearchQueryToken)) {
  case tokens {
    [OrToken, ..rest] ->
      case parse_and_expression(rest) {
        #(None, rest2) -> #(acc, rest2)
        #(Some(node), rest2) -> parse_or_tail(rest2, [node, ..acc])
      }
    _ -> #(acc, tokens)
  }
}

fn parse_and_expression(
  tokens: List(SearchQueryToken),
) -> #(Option(SearchQueryNode), List(SearchQueryToken)) {
  let #(children, rest) = collect_and_children(tokens, [])
  case children {
    [] -> #(None, rest)
    [single] -> #(Some(single), rest)
    _ -> #(Some(AndNode(children: list.reverse(children))), rest)
  }
}

fn collect_and_children(
  tokens: List(SearchQueryToken),
  acc: List(SearchQueryNode),
) -> #(List(SearchQueryNode), List(SearchQueryToken)) {
  case tokens {
    [] -> #(acc, [])
    [OrToken, ..] -> #(acc, tokens)
    [RParenToken, ..] -> #(acc, tokens)
    _ ->
      case parse_unary_expression(tokens) {
        #(None, rest) -> #(acc, rest)
        #(Some(node), rest) -> collect_and_children(rest, [node, ..acc])
      }
  }
}

fn parse_unary_expression(
  tokens: List(SearchQueryToken),
) -> #(Option(SearchQueryNode), List(SearchQueryToken)) {
  case tokens {
    [] -> #(None, [])
    [NotToken, ..rest] ->
      case parse_unary_expression(rest) {
        #(None, rest2) -> #(None, rest2)
        #(Some(child), rest2) -> #(Some(NotNode(child: child)), rest2)
      }
    [TermToken(value: v), ..rest] -> #(
      Some(TermNode(term: parse_search_query_term(v))),
      rest,
    )
    [LParenToken, ..rest] -> {
      let #(child, rest2) = parse_or_expression(rest)
      let rest3 = case rest2 {
        [RParenToken, ..rest3] -> rest3
        _ -> rest2
      }
      #(child, rest3)
    }
    _ -> #(None, tokens)
  }
}

// ----------- Generic match/apply -----------

/// Mirrors `matchesSearchQueryTerm`. Empty raw or pure-negation
/// (negated + empty value + no field) trivially passes. Otherwise
/// delegate to the positive matcher, then flip if negated.
pub fn matches_search_query_term(
  item: a,
  term: SearchQueryTerm,
  matches_positive_term: fn(a, SearchQueryTerm) -> Bool,
) -> Bool {
  case term.raw {
    "" -> True
    _ ->
      case term.negated, term.value, term.field {
        True, "", None -> True
        _, _, _ -> {
          let result = matches_positive_term(item, term)
          case term.negated {
            True -> !result
            False -> result
          }
        }
      }
  }
}

/// Mirrors `matchesSearchQueryNode`.
pub fn matches_search_query_node(
  item: a,
  node: SearchQueryNode,
  matches_positive_term: fn(a, SearchQueryTerm) -> Bool,
) -> Bool {
  case node {
    TermNode(term: term) ->
      matches_search_query_term(item, term, matches_positive_term)
    AndNode(children: children) ->
      list.all(children, fn(child) {
        matches_search_query_node(item, child, matches_positive_term)
      })
    OrNode(children: children) ->
      list.any(children, fn(child) {
        matches_search_query_node(item, child, matches_positive_term)
      })
    NotNode(child: child) ->
      !matches_search_query_node(item, child, matches_positive_term)
  }
}

/// Mirrors `applySearchQuery`. Empty / whitespace-only / unparseable
/// queries leave the list untouched.
pub fn apply_search_query(
  items: List(a),
  raw_query: Option(String),
  options: SearchQueryParseOptions,
  matches_positive_term: fn(a, SearchQueryTerm) -> Bool,
) -> List(a) {
  case raw_query {
    None -> items
    Some(q) ->
      case string.trim(q) {
        "" -> items
        trimmed ->
          case parse_search_query(trimmed, options) {
            None -> items
            Some(parsed) ->
              list.filter(items, fn(item) {
                matches_search_query_node(item, parsed, matches_positive_term)
              })
          }
      }
  }
}

/// Mirrors `applySearchQueryTerms`. AND semantics across all terms.
pub fn apply_search_query_terms(
  items: List(a),
  raw_query: Option(String),
  options: SearchQueryTermListOptions,
  matches_positive_term: fn(a, SearchQueryTerm) -> Bool,
) -> List(a) {
  let terms = parse_search_query_term_list(raw_query, options)
  case terms {
    [] -> items
    _ ->
      list.filter(items, fn(item) {
        list.all(terms, fn(term) {
          matches_search_query_term(item, term, matches_positive_term)
        })
      })
  }
}

fn trim_start(s: String) -> String {
  case s {
    " " <> rest -> trim_start(rest)
    "\t" <> rest -> trim_start(rest)
    "\n" <> rest -> trim_start(rest)
    "\r" <> rest -> trim_start(rest)
    _ -> s
  }
}

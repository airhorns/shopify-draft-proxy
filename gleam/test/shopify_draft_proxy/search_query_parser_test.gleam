import gleam/option.{None, Some}
import shopify_draft_proxy/search_query_parser.{
  type SearchQueryTerm, AndNode, Equal, ExactMatch, GreaterThan,
  GreaterThanOrEqual, IncludesMatch, LessThan, LessThanOrEqual, NotNode, OrNode,
  SearchQueryParseOptions, SearchQueryStringMatchOptions, SearchQueryTerm,
  SearchQueryTermListOptions, TermNode,
}

// ----------- Term parsing -----------

pub fn parse_term_plain_test() {
  let term = search_query_parser.parse_search_query_term("hello")
  assert term
    == SearchQueryTerm(
      raw: "hello",
      negated: False,
      field: None,
      comparator: None,
      value: "hello",
    )
}

pub fn parse_term_field_value_test() {
  let term = search_query_parser.parse_search_query_term("title:hello")
  assert term.field == Some("title")
  assert term.value == "hello"
  assert term.comparator == None
  assert term.negated == False
}

pub fn parse_term_negated_test() {
  let term = search_query_parser.parse_search_query_term("-title:foo")
  assert term.negated == True
  assert term.field == Some("title")
  assert term.value == "foo"
}

pub fn parse_term_lone_dash_is_value_test() {
  // Just `-` shouldn't trigger negation (length must be > 1).
  let term = search_query_parser.parse_search_query_term("-")
  assert term.negated == False
  assert term.value == "-"
}

pub fn parse_term_comparator_lte_test() {
  let term = search_query_parser.parse_search_query_term("price:<=10")
  assert term.field == Some("price")
  assert term.comparator == Some(LessThanOrEqual)
  assert term.value == "10"
}

pub fn parse_term_comparator_gte_test() {
  let term = search_query_parser.parse_search_query_term("price:>=10")
  assert term.comparator == Some(GreaterThanOrEqual)
  assert term.value == "10"
}

pub fn parse_term_comparator_lt_test() {
  let term = search_query_parser.parse_search_query_term("price:<10")
  assert term.comparator == Some(LessThan)
  assert term.value == "10"
}

pub fn parse_term_comparator_gt_test() {
  let term = search_query_parser.parse_search_query_term("price:>10")
  assert term.comparator == Some(GreaterThan)
  assert term.value == "10"
}

pub fn parse_term_comparator_eq_test() {
  let term = search_query_parser.parse_search_query_term("price:=10")
  assert term.comparator == Some(Equal)
  assert term.value == "10"
}

pub fn parse_term_splits_only_first_colon_test() {
  let term = search_query_parser.parse_search_query_term("a:b:c")
  assert term.field == Some("a")
  assert term.value == "b:c"
}

pub fn search_query_term_value_round_trips_comparator_test() {
  let term = search_query_parser.parse_search_query_term("price:>=10")
  assert search_query_parser.search_query_term_value(term) == ">=10"
}

pub fn search_query_term_value_no_comparator_test() {
  let term = search_query_parser.parse_search_query_term("title:hello")
  assert search_query_parser.search_query_term_value(term) == "hello"
}

// ----------- Quote handling -----------

pub fn normalize_value_strips_one_leading_or_trailing_quote_test() {
  // Normalize is one-quote-per-side, not balanced.
  assert search_query_parser.normalize_search_query_value("\"hello\"")
    == "hello"
  assert search_query_parser.normalize_search_query_value("'hi'") == "hi"
  assert search_query_parser.normalize_search_query_value("\"unbalanced")
    == "unbalanced"
  assert search_query_parser.normalize_search_query_value("Hello") == "hello"
}

pub fn strip_value_quotes_only_balanced_pairs_test() {
  // Balanced quotes get stripped.
  assert search_query_parser.strip_search_query_value_quotes("\"hello\"")
    == "hello"
  assert search_query_parser.strip_search_query_value_quotes("'hi'") == "hi"
  // Mismatched quotes are left alone.
  assert search_query_parser.strip_search_query_value_quotes("\"hi'") == "\"hi'"
  assert search_query_parser.strip_search_query_value_quotes("hello") == "hello"
}

// ----------- String matching -----------

pub fn matches_string_exact_test() {
  let opts = search_query_parser.default_string_match_options()
  assert search_query_parser.matches_search_query_string(
      Some("hello"),
      "hello",
      ExactMatch,
      opts,
    )
    == True
  assert search_query_parser.matches_search_query_string(
      Some("hello world"),
      "hello",
      ExactMatch,
      opts,
    )
    == False
}

pub fn matches_string_includes_test() {
  let opts = search_query_parser.default_string_match_options()
  assert search_query_parser.matches_search_query_string(
      Some("hello world"),
      "world",
      IncludesMatch,
      opts,
    )
    == True
}

pub fn matches_string_case_insensitive_test() {
  let opts = search_query_parser.default_string_match_options()
  assert search_query_parser.matches_search_query_string(
      Some("HELLO"),
      "hello",
      ExactMatch,
      opts,
    )
    == True
}

pub fn matches_string_prefix_wildcard_test() {
  let opts = search_query_parser.default_string_match_options()
  assert search_query_parser.matches_search_query_string(
      Some("hello world"),
      "hel*",
      ExactMatch,
      opts,
    )
    == True
  assert search_query_parser.matches_search_query_string(
      Some("hello"),
      "world*",
      ExactMatch,
      opts,
    )
    == False
}

pub fn matches_string_word_prefix_wildcard_test() {
  let opts = SearchQueryStringMatchOptions(word_prefix: True)
  // Word-prefix matches a token start, not just the leading edge.
  assert search_query_parser.matches_search_query_string(
      Some("hello world"),
      "wor*",
      ExactMatch,
      opts,
    )
    == True
}

pub fn matches_string_empty_value_matches_test() {
  // Empty raw value is treated as "always matches" — used by
  // negation-only terms like `-`.
  let opts = search_query_parser.default_string_match_options()
  assert search_query_parser.matches_search_query_string(
      Some("hello"),
      "",
      ExactMatch,
      opts,
    )
    == True
}

pub fn matches_string_strips_balanced_quotes_test() {
  let opts = search_query_parser.default_string_match_options()
  assert search_query_parser.matches_search_query_string(
      Some("hello world"),
      "\"hello world\"",
      ExactMatch,
      opts,
    )
    == True
}

// ----------- Number matching -----------

pub fn matches_number_equal_default_test() {
  let term = search_query_parser.parse_search_query_term("price:10")
  assert search_query_parser.matches_search_query_number(Some(10.0), term)
    == True
  assert search_query_parser.matches_search_query_number(Some(11.0), term)
    == False
}

pub fn matches_number_lt_test() {
  let term = search_query_parser.parse_search_query_term("price:<10")
  assert search_query_parser.matches_search_query_number(Some(5.0), term)
    == True
  assert search_query_parser.matches_search_query_number(Some(10.0), term)
    == False
}

pub fn matches_number_gte_test() {
  let term = search_query_parser.parse_search_query_term("price:>=10")
  assert search_query_parser.matches_search_query_number(Some(10.0), term)
    == True
  assert search_query_parser.matches_search_query_number(Some(9.99), term)
    == False
}

pub fn matches_number_int_value_test() {
  let term = search_query_parser.parse_search_query_term("count:5")
  // Both int and float strings should parse.
  assert search_query_parser.matches_search_query_number(Some(5.0), term)
    == True
}

pub fn matches_number_none_test() {
  let term = search_query_parser.parse_search_query_term("price:10")
  assert search_query_parser.matches_search_query_number(None, term) == False
}

// ----------- Text matching -----------

pub fn matches_text_substring_test() {
  let term = search_query_parser.parse_search_query_term("body:foo")
  assert search_query_parser.matches_search_query_text(Some("FooBar"), term)
    == True
  assert search_query_parser.matches_search_query_text(Some("baz"), term)
    == False
}

pub fn matches_text_none_or_empty_test() {
  let term = search_query_parser.parse_search_query_term("body:foo")
  assert search_query_parser.matches_search_query_text(None, term) == False
  assert search_query_parser.matches_search_query_text(Some(""), term) == False
}

// ----------- Date matching -----------

pub fn matches_date_lte_now_test() {
  let term = search_query_parser.parse_search_query_term("created_at:<=now")
  // Pretend `now` is some absolute timestamp; the candidate is older.
  let now_ms = 1_700_000_000_000
  assert search_query_parser.matches_search_query_date(
      Some("2023-01-01T00:00:00Z"),
      term,
      now_ms,
    )
    == True
}

pub fn matches_date_gt_explicit_test() {
  let term =
    search_query_parser.parse_search_query_term(
      "created_at:>2023-01-01T00:00:00Z",
    )
  assert search_query_parser.matches_search_query_date(
      Some("2024-01-01T00:00:00Z"),
      term,
      0,
    )
    == True
  assert search_query_parser.matches_search_query_date(
      Some("2022-01-01T00:00:00Z"),
      term,
      0,
    )
    == False
}

pub fn matches_date_unparseable_test() {
  let term = search_query_parser.parse_search_query_term("created_at:>nonsense")
  assert search_query_parser.matches_search_query_date(
      Some("2024-01-01T00:00:00Z"),
      term,
      0,
    )
    == False
}

// ----------- Term list parsing -----------

pub fn parse_term_list_splits_on_whitespace_test() {
  let opts = search_query_parser.default_term_list_options()
  let terms = search_query_parser.parse_search_query_terms("a b c", opts)
  assert list_length(terms) == 3
}

pub fn parse_term_list_quoted_value_kept_together_test() {
  let opts = search_query_parser.default_term_list_options()
  let terms =
    search_query_parser.parse_search_query_terms(
      "title:\"hello world\" tag:foo",
      opts,
    )
  case terms {
    [first, second] -> {
      assert first.field == Some("title")
      assert first.value == "hello world"
      assert second.field == Some("tag")
      assert second.value == "foo"
    }
    _ -> panic as "expected 2 terms"
  }
}

pub fn parse_term_list_preserves_quotes_when_asked_test() {
  let opts =
    SearchQueryTermListOptions(
      quote_characters: ["\"", "'"],
      preserve_quotes_in_terms: True,
      ignored_keywords: [],
      drop_empty_values: False,
    )
  let terms =
    search_query_parser.parse_search_query_terms("title:\"hello world\"", opts)
  case terms {
    [first] -> {
      assert first.value == "\"hello world\""
    }
    _ -> panic as "expected 1 term"
  }
}

pub fn parse_term_list_drops_ignored_keywords_test() {
  let opts =
    SearchQueryTermListOptions(
      quote_characters: ["\"", "'"],
      preserve_quotes_in_terms: False,
      ignored_keywords: ["AND"],
      drop_empty_values: False,
    )
  let terms =
    search_query_parser.parse_search_query_terms("foo AND bar and baz", opts)
  // "AND" gets dropped (case-insensitive); "foo", "bar", "baz" remain.
  assert list_length(terms) == 3
}

pub fn parse_term_list_drop_empty_values_test() {
  let opts =
    SearchQueryTermListOptions(
      quote_characters: ["\"", "'"],
      preserve_quotes_in_terms: False,
      ignored_keywords: [],
      drop_empty_values: True,
    )
  let terms =
    search_query_parser.parse_search_query_term_list(
      Some("title:\"\" tag:foo"),
      opts,
    )
  case terms {
    [single] -> {
      assert single.field == Some("tag")
    }
    _ -> panic as "expected 1 term after drop_empty_values"
  }
}

pub fn parse_term_list_empty_input_test() {
  let opts = search_query_parser.default_term_list_options()
  assert search_query_parser.parse_search_query_term_list(None, opts) == []
  assert search_query_parser.parse_search_query_term_list(Some("   "), opts)
    == []
}

// ----------- Recursive descent parser -----------

pub fn parse_query_empty_test() {
  let opts = search_query_parser.default_parse_options()
  assert search_query_parser.parse_search_query("", opts) == None
}

pub fn parse_query_single_term_test() {
  let opts = search_query_parser.default_parse_options()
  case search_query_parser.parse_search_query("hello", opts) {
    Some(TermNode(term: term)) -> {
      assert term.value == "hello"
    }
    _ -> panic as "expected single TermNode"
  }
}

pub fn parse_query_implicit_and_test() {
  let opts = search_query_parser.default_parse_options()
  case search_query_parser.parse_search_query("a b c", opts) {
    Some(AndNode(children: children)) -> {
      assert list_length(children) == 3
    }
    _ -> panic as "expected AndNode"
  }
}

pub fn parse_query_or_test() {
  let opts = search_query_parser.default_parse_options()
  case search_query_parser.parse_search_query("a OR b", opts) {
    Some(OrNode(children: children)) -> {
      assert list_length(children) == 2
    }
    _ -> panic as "expected OrNode"
  }
}

pub fn parse_query_or_lower_precedence_than_and_test() {
  // `a b OR c d` → OR(AND(a,b), AND(c,d))
  let opts = search_query_parser.default_parse_options()
  case search_query_parser.parse_search_query("a b OR c d", opts) {
    Some(OrNode(children: [AndNode(children: left), AndNode(children: right)])) -> {
      assert list_length(left) == 2
      assert list_length(right) == 2
    }
    _ -> panic as "expected OR(AND, AND)"
  }
}

pub fn parse_query_parens_override_precedence_test() {
  // `a (b OR c)` → AND(a, OR(b, c))
  let opts = search_query_parser.default_parse_options()
  case search_query_parser.parse_search_query("a (b OR c)", opts) {
    Some(AndNode(children: [TermNode(term: _), OrNode(children: or_kids)])) -> {
      assert list_length(or_kids) == 2
    }
    _ -> panic as "expected AND(term, OR(b,c))"
  }
}

pub fn parse_query_negation_with_dash_paren_test() {
  // `-(a)` → NotNode(TermNode(a))
  let opts = search_query_parser.default_parse_options()
  case search_query_parser.parse_search_query("-(a)", opts) {
    Some(NotNode(child: TermNode(term: term))) -> {
      assert term.value == "a"
    }
    _ -> panic as "expected NotNode"
  }
}

pub fn parse_query_term_negation_test() {
  // `-foo` is a single negated TermNode (not a NotNode).
  let opts = search_query_parser.default_parse_options()
  case search_query_parser.parse_search_query("-foo", opts) {
    Some(TermNode(term: term)) -> {
      assert term.negated == True
      assert term.value == "foo"
    }
    _ -> panic as "expected negated TermNode"
  }
}

pub fn parse_query_not_keyword_disabled_by_default_test() {
  // Without recognize_not_keyword, "NOT" is just a term.
  let opts = search_query_parser.default_parse_options()
  case search_query_parser.parse_search_query("NOT foo", opts) {
    Some(AndNode(children: kids)) -> {
      assert list_length(kids) == 2
    }
    _ -> panic as "expected AND of two terms when NOT keyword disabled"
  }
}

pub fn parse_query_not_keyword_enabled_test() {
  let opts =
    SearchQueryParseOptions(
      quote_characters: ["\"", "'"],
      recognize_not_keyword: True,
      preserve_quotes_in_terms: False,
    )
  case search_query_parser.parse_search_query("NOT foo", opts) {
    Some(NotNode(child: TermNode(term: term))) -> {
      assert term.value == "foo"
    }
    _ -> panic as "expected NotNode when NOT keyword enabled"
  }
}

pub fn parse_query_quoted_value_after_field_test() {
  let opts = search_query_parser.default_parse_options()
  case search_query_parser.parse_search_query("title:\"hello world\"", opts) {
    Some(TermNode(term: term)) -> {
      assert term.field == Some("title")
      assert term.value == "hello world"
    }
    _ -> panic as "expected single TermNode with quoted value"
  }
}

pub fn parse_query_keeps_open_paren_after_field_colon_test() {
  let opts = search_query_parser.default_parse_options()
  case search_query_parser.parse_search_query("tag:(har-549", opts) {
    Some(TermNode(term: term)) -> {
      assert term.field == Some("tag")
      assert term.value == "(har-549"
    }
    _ -> panic as "expected field term with literal open paren"
  }
}

// ----------- Generic apply helpers -----------

type Item {
  Item(title: String, price: Float)
}

fn item_match(item: Item, term: SearchQueryTerm) -> Bool {
  case term.field {
    Some("title") -> {
      let opts = search_query_parser.default_string_match_options()
      search_query_parser.matches_search_query_string(
        Some(item.title),
        term.value,
        IncludesMatch,
        opts,
      )
    }
    Some("price") ->
      search_query_parser.matches_search_query_number(Some(item.price), term)
    _ -> {
      let opts = search_query_parser.default_string_match_options()
      search_query_parser.matches_search_query_string(
        Some(item.title),
        term.value,
        IncludesMatch,
        opts,
      )
    }
  }
}

pub fn apply_search_query_filters_items_test() {
  let items = [
    Item(title: "Apple", price: 1.0),
    Item(title: "Banana", price: 2.0),
    Item(title: "Cherry", price: 3.0),
  ]
  let opts = search_query_parser.default_parse_options()
  let result =
    search_query_parser.apply_search_query(
      items,
      Some("title:Banana OR price:>=3"),
      opts,
      item_match,
    )
  case result {
    [Item(title: "Banana", ..), Item(title: "Cherry", ..)] -> Nil
    _ -> panic as "expected Banana and Cherry"
  }
}

pub fn apply_search_query_empty_returns_all_test() {
  let items = [Item(title: "a", price: 1.0)]
  let opts = search_query_parser.default_parse_options()
  assert search_query_parser.apply_search_query(items, None, opts, item_match)
    == items
  assert search_query_parser.apply_search_query(
      items,
      Some("   "),
      opts,
      item_match,
    )
    == items
}

pub fn apply_search_query_negation_test() {
  let items = [
    Item(title: "Apple", price: 1.0),
    Item(title: "Banana", price: 2.0),
  ]
  let opts = search_query_parser.default_parse_options()
  let result =
    search_query_parser.apply_search_query(
      items,
      Some("-title:Apple"),
      opts,
      item_match,
    )
  case result {
    [Item(title: "Banana", ..)] -> Nil
    _ -> panic as "expected only Banana after negation"
  }
}

pub fn apply_search_query_terms_and_semantics_test() {
  let items = [
    Item(title: "Apple", price: 1.0),
    Item(title: "Apple Pie", price: 5.0),
  ]
  let opts = search_query_parser.default_term_list_options()
  let result =
    search_query_parser.apply_search_query_terms(
      items,
      Some("title:Apple price:>=5"),
      opts,
      item_match,
    )
  case result {
    [Item(title: "Apple Pie", ..)] -> Nil
    _ -> panic as "expected only Apple Pie"
  }
}

// Helpers (avoid pulling gleam/list into top-level imports we don't
// otherwise use).
fn list_length(xs: List(a)) -> Int {
  case xs {
    [] -> 0
    [_, ..rest] -> 1 + list_length(rest)
  }
}

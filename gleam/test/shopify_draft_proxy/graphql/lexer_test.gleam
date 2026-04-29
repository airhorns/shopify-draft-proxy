import gleam/list
import gleam/option.{None, Some}
import shopify_draft_proxy/graphql/lexer
import shopify_draft_proxy/graphql/source
import shopify_draft_proxy/graphql/token.{type Token}
import shopify_draft_proxy/graphql/token_kind as tk

fn lex_kinds(input: String) -> List(tk.TokenKind) {
  let assert Ok(tokens) = lexer.lex(source.new(input))
  list.map(tokens, fn(t: Token) { t.kind })
}

fn lex_or_panic(input: String) -> List(Token) {
  let assert Ok(tokens) = lexer.lex(source.new(input))
  tokens
}

pub fn empty_source_emits_only_eof_test() {
  assert lex_kinds("") == [tk.Eof]
}

pub fn whitespace_is_skipped_test() {
  assert lex_kinds("   \t  ,, ") == [tk.Eof]
}

pub fn single_punctuators_test() {
  assert lex_kinds("{}()[]:!@$&|=")
    == [
      tk.BraceL,
      tk.BraceR,
      tk.ParenL,
      tk.ParenR,
      tk.BracketL,
      tk.BracketR,
      tk.Colon,
      tk.Bang,
      tk.At,
      tk.Dollar,
      tk.Amp,
      tk.Pipe,
      tk.Equals,
      tk.Eof,
    ]
}

pub fn spread_token_test() {
  assert lex_kinds("...") == [tk.Spread, tk.Eof]
}

pub fn lone_dot_is_an_error_test() {
  let assert Error(_) = lexer.lex(source.new("."))
}

pub fn name_token_test() {
  let tokens = lex_or_panic("hello")
  let assert [name, _eof] = tokens
  assert name.kind == tk.Name
  assert name.value == Some("hello")
  assert name.start == 0
  assert name.end == 5
  assert name.line == 1
  assert name.column == 1
}

pub fn names_are_alphanumeric_with_underscores_test() {
  let tokens = lex_or_panic("_foo123 bar_baz")
  let assert [a, b, _eof] = tokens
  assert a.value == Some("_foo123")
  assert b.value == Some("bar_baz")
}

pub fn int_token_test() {
  let tokens = lex_or_panic("42 0 -7")
  let assert [a, b, c, _eof] = tokens
  assert a.kind == tk.Int
  assert a.value == Some("42")
  assert b.kind == tk.Int
  assert b.value == Some("0")
  assert c.kind == tk.Int
  assert c.value == Some("-7")
}

pub fn float_token_test() {
  let tokens = lex_or_panic("1.5 0.0 -3.14 1e10 2.5E-3")
  let assert [a, b, c, d, e, _eof] = tokens
  assert a.kind == tk.Float
  assert a.value == Some("1.5")
  assert b.kind == tk.Float
  assert b.value == Some("0.0")
  assert c.kind == tk.Float
  assert c.value == Some("-3.14")
  assert d.kind == tk.Float
  assert d.value == Some("1e10")
  assert e.kind == tk.Float
  assert e.value == Some("2.5E-3")
}

pub fn invalid_number_leading_zero_test() {
  let assert Error(_) = lexer.lex(source.new("01"))
}

pub fn string_token_test() {
  let tokens = lex_or_panic("\"hello\"")
  let assert [s, _eof] = tokens
  assert s.kind == tk.String
  assert s.value == Some("hello")
  assert s.start == 0
  assert s.end == 7
}

pub fn string_with_escapes_test() {
  let tokens = lex_or_panic("\"a\\nb\\\"c\\\\d\"")
  let assert [s, _eof] = tokens
  assert s.kind == tk.String
  assert s.value == Some("a\nb\"c\\d")
}

pub fn string_with_unicode_fixed_escape_test() {
  let tokens = lex_or_panic("\"\\u00e9\"")
  let assert [s, _eof] = tokens
  assert s.value == Some("é")
}

pub fn string_with_unicode_variable_escape_test() {
  let tokens = lex_or_panic("\"\\u{1F600}\"")
  let assert [s, _eof] = tokens
  assert s.value == Some("😀")
}

pub fn unterminated_string_is_an_error_test() {
  let assert Error(_) = lexer.lex(source.new("\"oops"))
}

pub fn comments_are_skipped_test() {
  // Comments are emitted as `Comment` tokens by `read_raw_token` but
  // `next_token` (used by `lex`) skips them, exactly like graphql-js.
  let tokens = lex_or_panic("# leading\nfoo # trailing\n")
  let assert [name, _eof] = tokens
  assert name.kind == tk.Name
  assert name.value == Some("foo")
}

pub fn line_and_column_track_newlines_test() {
  let tokens = lex_or_panic("a\n  b")
  let assert [a, b, _eof] = tokens
  assert a.line == 1
  assert a.column == 1
  assert b.line == 2
  assert b.column == 3
}

pub fn windows_newline_increments_line_once_test() {
  let tokens = lex_or_panic("a\r\nb")
  let assert [a, b, _eof] = tokens
  assert a.line == 1
  assert b.line == 2
  assert b.column == 1
}

pub fn block_string_currently_errors_test() {
  let assert Error(_) = lexer.lex(source.new("\"\"\"hi\"\"\""))
}

pub fn realistic_query_lexes_to_expected_kinds_test() {
  let kinds = lex_kinds("query Foo($id: ID!) { node(id: $id) { id } }")
  assert kinds
    == [
      tk.Name,
      tk.Name,
      tk.ParenL,
      tk.Dollar,
      tk.Name,
      tk.Colon,
      tk.Name,
      tk.Bang,
      tk.ParenR,
      tk.BraceL,
      tk.Name,
      tk.ParenL,
      tk.Name,
      tk.Colon,
      tk.Dollar,
      tk.Name,
      tk.ParenR,
      tk.BraceL,
      tk.Name,
      tk.BraceR,
      tk.BraceR,
      tk.Eof,
    ]
}

pub fn unused_punctuator_helper_test() {
  // Reference token.punctuator so the constructor stays exercised even when
  // tests above only consume tokens via `lex`.
  let t = token.punctuator(tk.Bang, 0, 1, 1, 1)
  assert t.kind == tk.Bang
  assert t.value == None
}

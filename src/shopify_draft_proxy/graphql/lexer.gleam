//// Mirrors `graphql-js` `language/lexer.ts`.
////
//// The lexer walks a `Source.body` codepoint-by-codepoint and produces a
//// stream of `Token` values. The Gleam port differs from graphql-js in
//// three intentional ways:
////
////   1. Lexer state is immutable. `next_token` returns the next token plus
////      a fresh `Lexer` to thread through the parser, instead of mutating
////      `this.line` / `this.token` like graphql-js does.
////   2. Errors are returned as `Result(_, LexError)` rather than thrown.
////   3. Position semantics use Unicode code points, not UTF-16 code units.
////      For ASCII Shopify GraphQL queries (which is all the proxy parses)
////      this is identical to graphql-js; supplementary-plane characters
////      would diverge by one position per surrogate pair.
////
//// Block strings (`"""..."""`) are intentionally not implemented in the
//// spike — operation documents under `config/parity-requests/**` do not
//// contain them. A clear error is raised if one is encountered.

import gleam/int
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/character_classes as cc
import shopify_draft_proxy/graphql/source.{type Source}
import shopify_draft_proxy/graphql/token.{type Token, Token}
import shopify_draft_proxy/graphql/token_kind as tk

/// A lex-time error. `position`/`line`/`column` use the same conventions as
/// `Token`: 0-indexed code-point offset, 1-indexed line and column.
pub type LexError {
  LexError(message: String, position: Int, line: Int, column: Int)
}

/// Immutable lexer state. Thread it through `next_token` to advance.
pub type Lexer {
  Lexer(
    source: Source,
    remaining: List(Int),
    position: Int,
    line: Int,
    line_start: Int,
  )
}

/// Construct a fresh lexer at the start of the source body.
pub fn new(source: Source) -> Lexer {
  let codes =
    source.body
    |> string.to_utf_codepoints
    |> list.map(string.utf_codepoint_to_int)
  Lexer(source: source, remaining: codes, position: 0, line: 1, line_start: 0)
}

/// Return the full sequence of tokens from a `Source`, terminated by an
/// `Eof` token. Comments are skipped. The returned list is stable and
/// position-ordered.
pub fn lex(source: Source) -> Result(List(Token), LexError) {
  collect_tokens(new(source), [])
}

fn collect_tokens(
  lexer: Lexer,
  acc: List(Token),
) -> Result(List(Token), LexError) {
  case next_token(lexer) {
    Error(e) -> Error(e)
    Ok(#(token, after)) ->
      case token.kind == tk.Eof {
        True -> Ok(list.reverse([token, ..acc]))
        False -> collect_tokens(after, [token, ..acc])
      }
  }
}

/// Read the next non-comment, non-ignored token from the lexer.
pub fn next_token(lexer: Lexer) -> Result(#(Token, Lexer), LexError) {
  let lexer = skip_whitespace(lexer)
  case read_raw_token(lexer) {
    Error(e) -> Error(e)
    Ok(#(token, after)) ->
      case token.kind == tk.Comment {
        True -> next_token(after)
        False -> Ok(#(token, after))
      }
  }
}

/// Read the next token from the source — *including* `Comment` tokens, but
/// after ignored whitespace has been skipped.
fn read_raw_token(lexer: Lexer) -> Result(#(Token, Lexer), LexError) {
  case lexer.remaining {
    [] -> Ok(#(eof_token(lexer), lexer))
    [c, ..] ->
      case c {
        0x0023 -> read_comment(lexer)

        // Single-character punctuators.
        0x0021 -> Ok(simple_punct(lexer, tk.Bang))
        0x0024 -> Ok(simple_punct(lexer, tk.Dollar))
        0x0026 -> Ok(simple_punct(lexer, tk.Amp))
        0x0028 -> Ok(simple_punct(lexer, tk.ParenL))
        0x0029 -> Ok(simple_punct(lexer, tk.ParenR))
        0x003a -> Ok(simple_punct(lexer, tk.Colon))
        0x003d -> Ok(simple_punct(lexer, tk.Equals))
        0x0040 -> Ok(simple_punct(lexer, tk.At))
        0x005b -> Ok(simple_punct(lexer, tk.BracketL))
        0x005d -> Ok(simple_punct(lexer, tk.BracketR))
        0x007b -> Ok(simple_punct(lexer, tk.BraceL))
        0x007c -> Ok(simple_punct(lexer, tk.Pipe))
        0x007d -> Ok(simple_punct(lexer, tk.BraceR))

        // `...` (spread) — `.` on its own is an error.
        0x002e -> read_spread(lexer)

        // String / block string.
        0x0022 ->
          case lexer.remaining {
            [_, 0x0022, 0x0022, ..] ->
              err(
                lexer,
                "Block strings (\"\"\"…\"\"\") are not supported by this lexer port yet.",
              )
            _ -> read_string(lexer)
          }

        _ ->
          case cc.is_digit(c) || c == 0x002d {
            True -> read_number(lexer)
            False ->
              case cc.is_name_start(c) {
                True -> read_name(lexer)
                False ->
                  err(
                    lexer,
                    "Unexpected character: " <> printable_code_point(c) <> ".",
                  )
              }
          }
      }
  }
}

fn eof_token(lexer: Lexer) -> Token {
  let column = 1 + lexer.position - lexer.line_start
  token.punctuator(tk.Eof, lexer.position, lexer.position, lexer.line, column)
}

fn simple_punct(lexer: Lexer, kind) -> #(Token, Lexer) {
  let start = lexer.position
  let column = 1 + start - lexer.line_start
  let line = lexer.line
  let after = advance_one(lexer)
  #(token.punctuator(kind, start, start + 1, line, column), after)
}

fn read_spread(lexer: Lexer) -> Result(#(Token, Lexer), LexError) {
  let start = lexer.position
  let column = 1 + start - lexer.line_start
  let line = lexer.line
  case lexer.remaining {
    [0x002e, 0x002e, 0x002e, ..rest] ->
      Ok(#(
        token.punctuator(tk.Spread, start, start + 3, line, column),
        Lexer(..lexer, remaining: rest, position: start + 3),
      ))
    _ -> err(lexer, "Unexpected character: \".\".")
  }
}

fn read_comment(lexer: Lexer) -> Result(#(Token, Lexer), LexError) {
  let start = lexer.position
  let line = lexer.line
  let column = 1 + start - lexer.line_start
  // Skip the leading `#`.
  let lexer = advance_one(lexer)
  let #(buf, after) = read_comment_chars(lexer, [])
  let value = codepoints_to_string(list.reverse(buf))
  Ok(#(
    Token(
      kind: tk.Comment,
      start: start,
      end: after.position,
      line: line,
      column: column,
      value: Some(value),
    ),
    after,
  ))
}

fn read_comment_chars(lexer: Lexer, buf: List(Int)) -> #(List(Int), Lexer) {
  case lexer.remaining {
    [] -> #(buf, lexer)
    [c, ..] ->
      case c == 0x000a || c == 0x000d {
        True -> #(buf, lexer)
        False ->
          case cc.is_unicode_scalar_value(c) {
            True -> read_comment_chars(advance_one(lexer), [c, ..buf])
            False -> #(buf, lexer)
          }
      }
  }
}

fn read_name(lexer: Lexer) -> Result(#(Token, Lexer), LexError) {
  let start = lexer.position
  let column = 1 + start - lexer.line_start
  let line = lexer.line
  let #(buf, after) = read_name_continue(lexer, [])
  let value = codepoints_to_string(list.reverse(buf))
  Ok(#(
    Token(
      kind: tk.Name,
      start: start,
      end: after.position,
      line: line,
      column: column,
      value: Some(value),
    ),
    after,
  ))
}

fn read_name_continue(lexer: Lexer, buf: List(Int)) -> #(List(Int), Lexer) {
  case lexer.remaining {
    [c, ..] ->
      case cc.is_name_continue(c) {
        True -> read_name_continue(advance_one(lexer), [c, ..buf])
        False -> #(buf, lexer)
      }
    _ -> #(buf, lexer)
  }
}

fn read_number(lexer: Lexer) -> Result(#(Token, Lexer), LexError) {
  let start = lexer.position
  let line = lexer.line
  let column = 1 + start - lexer.line_start

  // Optional negative sign.
  let #(lexer, buf) = case lexer.remaining {
    [0x002d, ..] -> #(advance_one(lexer), [0x002d])
    _ -> #(lexer, [])
  }

  // Integer part.
  use #(lexer, buf) <- result.try(read_integer_part(lexer, buf))

  // Optional fractional part.
  use #(lexer, buf, has_frac) <- result.try(case lexer.remaining {
    [0x002e, ..] -> {
      let lexer = advance_one(lexer)
      let buf = [0x002e, ..buf]
      use #(lexer, buf) <- result.try(read_digits_required(lexer, buf))
      Ok(#(lexer, buf, True))
    }
    _ -> Ok(#(lexer, buf, False))
  })

  // Optional exponent part.
  use #(lexer, buf, has_exp) <- result.try(case lexer.remaining {
    [c, ..] if c == 0x0045 || c == 0x0065 -> {
      let lexer = advance_one(lexer)
      let buf = [c, ..buf]
      let #(lexer, buf) = case lexer.remaining {
        [s, ..] if s == 0x002b || s == 0x002d -> #(advance_one(lexer), [
          s,
          ..buf
        ])
        _ -> #(lexer, buf)
      }
      use #(lexer, buf) <- result.try(read_digits_required(lexer, buf))
      Ok(#(lexer, buf, True))
    }
    _ -> Ok(#(lexer, buf, False))
  })

  // Numbers cannot be followed by `.` or NameStart.
  use _ <- result.try(case lexer.remaining {
    [c, ..] ->
      case c == 0x002e || cc.is_name_start(c) {
        True ->
          err(
            lexer,
            "Invalid number, expected digit but got: "
              <> printable_code_point(c)
              <> ".",
          )
        False -> Ok(Nil)
      }
    [] -> Ok(Nil)
  })

  let kind = case has_frac || has_exp {
    True -> tk.Float
    False -> tk.Int
  }
  let value = codepoints_to_string(list.reverse(buf))
  Ok(#(
    Token(
      kind: kind,
      start: start,
      end: lexer.position,
      line: line,
      column: column,
      value: Some(value),
    ),
    lexer,
  ))
}

fn read_integer_part(
  lexer: Lexer,
  buf: List(Int),
) -> Result(#(Lexer, List(Int)), LexError) {
  case lexer.remaining {
    [0x0030, ..] -> {
      // Leading zero — must not be followed by another digit.
      let lexer = advance_one(lexer)
      let buf = [0x0030, ..buf]
      case lexer.remaining {
        [c, ..] ->
          case cc.is_digit(c) {
            True ->
              err(
                lexer,
                "Invalid number, unexpected digit after 0: "
                  <> printable_code_point(c)
                  <> ".",
              )
            False -> Ok(#(lexer, buf))
          }
        [] -> Ok(#(lexer, buf))
      }
    }
    [c, ..] ->
      case cc.is_digit(c) {
        True -> Ok(read_digits(lexer, buf))
        False ->
          err(
            lexer,
            "Invalid number, expected digit but got: "
              <> printable_code_point(c)
              <> ".",
          )
      }
    [] -> err(lexer, "Invalid number, expected digit but got: <EOF>.")
  }
}

fn read_digits_required(
  lexer: Lexer,
  buf: List(Int),
) -> Result(#(Lexer, List(Int)), LexError) {
  case lexer.remaining {
    [c, ..] ->
      case cc.is_digit(c) {
        True -> Ok(read_digits(lexer, buf))
        False ->
          err(
            lexer,
            "Invalid number, expected digit but got: "
              <> printable_code_point(c)
              <> ".",
          )
      }
    [] -> err(lexer, "Invalid number, expected digit but got: <EOF>.")
  }
}

fn read_digits(lexer: Lexer, buf: List(Int)) -> #(Lexer, List(Int)) {
  case lexer.remaining {
    [c, ..] ->
      case cc.is_digit(c) {
        True -> read_digits(advance_one(lexer), [c, ..buf])
        False -> #(lexer, buf)
      }
    [] -> #(lexer, buf)
  }
}

fn read_string(lexer: Lexer) -> Result(#(Token, Lexer), LexError) {
  let start = lexer.position
  let line = lexer.line
  let column = 1 + start - lexer.line_start
  // Consume opening `"`.
  let lexer = advance_one(lexer)
  read_string_contents(lexer, [], start, line, column)
}

fn read_string_contents(
  lexer: Lexer,
  buf: List(Int),
  start: Int,
  line: Int,
  column: Int,
) -> Result(#(Token, Lexer), LexError) {
  case lexer.remaining {
    [] -> err(lexer, "Unterminated string.")
    [0x0022, ..] -> {
      // Closing `"`.
      let lexer = advance_one(lexer)
      let value = codepoints_to_string(list.reverse(buf))
      Ok(#(
        Token(
          kind: tk.String,
          start: start,
          end: lexer.position,
          line: line,
          column: column,
          value: Some(value),
        ),
        lexer,
      ))
    }
    [0x005c, ..] -> {
      use #(lexer, escaped) <- result.try(read_escape(lexer))
      let buf = list.fold(escaped, buf, fn(acc, c) { [c, ..acc] })
      read_string_contents(lexer, buf, start, line, column)
    }
    [c, ..] ->
      case c == 0x000a || c == 0x000d {
        True -> err(lexer, "Unterminated string.")
        False ->
          case cc.is_unicode_scalar_value(c) {
            True ->
              read_string_contents(
                advance_one(lexer),
                [c, ..buf],
                start,
                line,
                column,
              )
            False ->
              err(
                lexer,
                "Invalid character within String: "
                  <> printable_code_point(c)
                  <> ".",
              )
          }
      }
  }
}

fn read_escape(lexer: Lexer) -> Result(#(Lexer, List(Int)), LexError) {
  case lexer.remaining {
    [0x005c, 0x0075, 0x007b, ..] -> read_escaped_unicode_variable(lexer)
    [0x005c, 0x0075, ..] -> read_escaped_unicode_fixed(lexer)
    [0x005c, c, ..rest] -> {
      let after = Lexer(..lexer, remaining: rest, position: lexer.position + 2)
      case c {
        0x0022 -> Ok(#(after, [0x0022]))
        0x005c -> Ok(#(after, [0x005c]))
        0x002f -> Ok(#(after, [0x002f]))
        0x0062 -> Ok(#(after, [0x0008]))
        0x0066 -> Ok(#(after, [0x000c]))
        0x006e -> Ok(#(after, [0x000a]))
        0x0072 -> Ok(#(after, [0x000d]))
        0x0074 -> Ok(#(after, [0x0009]))
        _ ->
          err(
            lexer,
            "Invalid character escape sequence: \"\\"
              <> codepoints_to_string([c])
              <> "\".",
          )
      }
    }
    _ -> err(lexer, "Invalid character escape sequence.")
  }
}

fn read_escaped_unicode_fixed(
  lexer: Lexer,
) -> Result(#(Lexer, List(Int)), LexError) {
  case lexer.remaining {
    [_, _, c1, c2, c3, c4, ..rest] -> {
      let code = read_16bit_hex(c1, c2, c3, c4)
      case code >= 0 && cc.is_unicode_scalar_value(code) {
        True ->
          Ok(
            #(Lexer(..lexer, remaining: rest, position: lexer.position + 6), [
              code,
            ]),
          )
        False -> err(lexer, "Invalid Unicode escape sequence.")
      }
    }
    _ -> err(lexer, "Invalid Unicode escape sequence.")
  }
}

fn read_escaped_unicode_variable(
  lexer: Lexer,
) -> Result(#(Lexer, List(Int)), LexError) {
  // Lexer is positioned at `\`. The opening `\u{` occupies offsets 0..2.
  read_var_unicode_loop(lexer, 3, 0)
}

fn read_var_unicode_loop(
  lexer: Lexer,
  size: Int,
  point: Int,
) -> Result(#(Lexer, List(Int)), LexError) {
  case size >= 12 {
    True -> err(lexer, "Invalid Unicode escape sequence.")
    False -> {
      case nth(lexer.remaining, size) {
        None -> err(lexer, "Invalid Unicode escape sequence.")
        Some(c) ->
          case c == 0x007d {
            True ->
              case size >= 4 && cc.is_unicode_scalar_value(point) {
                True -> {
                  let total = size + 1
                  Ok(#(drop_n_lexer(lexer, total), [point]))
                }
                False -> err(lexer, "Invalid Unicode escape sequence.")
              }
            False -> {
              let digit = cc.read_hex_digit(c)
              case digit {
                -1 -> err(lexer, "Invalid Unicode escape sequence.")
                _ ->
                  read_var_unicode_loop(lexer, size + 1, { point * 16 } + digit)
              }
            }
          }
      }
    }
  }
}

fn read_16bit_hex(c1: Int, c2: Int, c3: Int, c4: Int) -> Int {
  let h1 = cc.read_hex_digit(c1)
  let h2 = cc.read_hex_digit(c2)
  let h3 = cc.read_hex_digit(c3)
  let h4 = cc.read_hex_digit(c4)
  case h1 < 0 || h2 < 0 || h3 < 0 || h4 < 0 {
    True -> -1
    False -> { h1 * 4096 } + { h2 * 256 } + { h3 * 16 } + h4
  }
}

fn skip_whitespace(lexer: Lexer) -> Lexer {
  case lexer.remaining {
    [] -> lexer
    [c, ..] ->
      case c {
        0xfeff -> skip_whitespace(advance_one(lexer))
        0x0009 -> skip_whitespace(advance_one(lexer))
        0x0020 -> skip_whitespace(advance_one(lexer))
        0x002c -> skip_whitespace(advance_one(lexer))
        0x000a -> skip_whitespace(advance_newline(advance_one(lexer)))
        0x000d -> {
          let after_cr = advance_one(lexer)
          let after = case after_cr.remaining {
            [0x000a, ..] -> advance_one(after_cr)
            _ -> after_cr
          }
          skip_whitespace(advance_newline(after))
        }
        _ -> lexer
      }
  }
}

fn advance_one(lexer: Lexer) -> Lexer {
  case lexer.remaining {
    [_, ..rest] -> Lexer(..lexer, remaining: rest, position: lexer.position + 1)
    [] -> lexer
  }
}

fn advance_newline(lexer: Lexer) -> Lexer {
  Lexer(..lexer, line: lexer.line + 1, line_start: lexer.position)
}

fn drop_n_lexer(lexer: Lexer, n: Int) -> Lexer {
  case n, lexer.remaining {
    0, _ -> lexer
    _, [] -> lexer
    _, [_, ..rest] ->
      drop_n_lexer(
        Lexer(..lexer, remaining: rest, position: lexer.position + 1),
        n - 1,
      )
  }
}

fn nth(items: List(a), n: Int) -> Option(a) {
  case items {
    [] -> None
    [x, ..rest] ->
      case n {
        0 -> Some(x)
        _ -> nth(rest, n - 1)
      }
  }
}

fn err(lexer: Lexer, message: String) -> Result(a, LexError) {
  let column = 1 + lexer.position - lexer.line_start
  Error(LexError(
    message: message,
    position: lexer.position,
    line: lexer.line,
    column: column,
  ))
}

fn codepoints_to_string(codes: List(Int)) -> String {
  codes
  |> list.filter_map(fn(c) {
    case string.utf_codepoint(c) {
      Ok(cp) -> Ok(cp)
      Error(_) -> Error(Nil)
    }
  })
  |> string.from_utf_codepoints
}

fn printable_code_point(c: Int) -> String {
  case c >= 0x0020 && c <= 0x007e {
    True ->
      case c == 0x0022 {
        True -> "'\"'"
        False -> "\"" <> codepoints_to_string([c]) <> "\""
      }
    False -> {
      let hex = int.to_base16(c)
      "U+" <> string.pad_start(hex, to: 4, with: "0")
    }
  }
}

//// Mirrors `graphql-js` `language/ast.ts` `Token` class.
////
//// In graphql-js the `Token` is a doubly-linked node whose `prev`/`next`
//// pointers are mutated by the parser. Gleam values are immutable, so the
//// Gleam port keeps `Token` as a flat record and lets the parser thread
//// tokens through by position. The substantive fields (`kind`, `start`,
//// `end`, `line`, `column`, `value`) are identical to graphql-js.

import gleam/option.{type Option, None}
import shopify_draft_proxy/graphql/token_kind.{type TokenKind}

/// A single lexed token. `start` and `end` are 0-indexed code-point offsets
/// into `Source.body`. `line` and `column` are 1-indexed. `value` is set for
/// `Name`, `Int`, `Float`, `String`, `BlockString`, and `Comment` tokens
/// (matching graphql-js); for other kinds it is `None`.
pub type Token {
  Token(
    kind: TokenKind,
    start: Int,
    end: Int,
    line: Int,
    column: Int,
    value: Option(String),
  )
}

/// Construct a token with a `None` value (used for punctuators and `Sof`/`Eof`).
pub fn punctuator(
  kind: TokenKind,
  start: Int,
  end: Int,
  line: Int,
  column: Int,
) -> Token {
  Token(
    kind: kind,
    start: start,
    end: end,
    line: line,
    column: column,
    value: None,
  )
}

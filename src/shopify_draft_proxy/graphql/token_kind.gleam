//// Mirrors `graphql-js` `language/tokenKind.ts`.
////
//// One variant per token kind the lexer emits. The string form (used in
//// graphql-js for `kind`) is intentionally not encoded as a runtime value;
//// callers pattern-match on the variant directly.

pub type TokenKind {
  Sof
  Eof
  Bang
  Dollar
  Amp
  ParenL
  ParenR
  Dot
  Spread
  Colon
  Equals
  At
  BracketL
  BracketR
  BraceL
  Pipe
  BraceR
  Name
  Int
  Float
  String
  BlockString
  Comment
}

/// True for punctuator kinds (graphql-js `isPunctuatorTokenKind`).
pub fn is_punctuator(kind: TokenKind) -> Bool {
  case kind {
    Bang
    | Dollar
    | Amp
    | ParenL
    | ParenR
    | Dot
    | Spread
    | Colon
    | Equals
    | At
    | BracketL
    | BracketR
    | BraceL
    | Pipe
    | BraceR -> True
    _ -> False
  }
}

/// Display string used by graphql-js error messages and printers.
pub fn display(kind: TokenKind) -> String {
  case kind {
    Sof -> "<SOF>"
    Eof -> "<EOF>"
    Bang -> "!"
    Dollar -> "$"
    Amp -> "&"
    ParenL -> "("
    ParenR -> ")"
    Dot -> "."
    Spread -> "..."
    Colon -> ":"
    Equals -> "="
    At -> "@"
    BracketL -> "["
    BracketR -> "]"
    BraceL -> "{"
    Pipe -> "|"
    BraceR -> "}"
    Name -> "Name"
    Int -> "Int"
    Float -> "Float"
    String -> "String"
    BlockString -> "BlockString"
    Comment -> "Comment"
  }
}

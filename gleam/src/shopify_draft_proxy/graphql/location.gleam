//// Mirrors `graphql-js` `language/location.ts`.
////
//// Computes the (line, column) for a UTF-8 character offset within a
//// `Source` body. `line` and `column` are 1-indexed.
////
//// Position semantics: `graphql-js` counts JS string positions (UTF-16
//// code units). Gleam strings are UTF-8 graphemes, so this implementation
//// counts Unicode code points. For the BMP characters that real Shopify
//// queries use this matches `graphql-js` exactly; supplementary-plane
//// characters (very rare in GraphQL source) would diverge by one position
//// per surrogate pair, and that gap is acceptable for the proxy use case.

import gleam/list
import gleam/string
import shopify_draft_proxy/graphql/source.{type Source}

/// 1-indexed line/column position inside a `Source.body`.
pub type SourceLocation {
  SourceLocation(line: Int, column: Int)
}

const cr = 0x0d

const lf = 0x0a

/// Translate a 0-indexed code point offset in `source.body` into a 1-indexed
/// line and column. Matches `graphql-js`'s `getLocation`.
pub fn get_location(source: Source, position position: Int) -> SourceLocation {
  let codepoints = string.to_utf_codepoints(source.body)
  scan(codepoints, 0, 0, 1, position)
}

fn scan(
  codepoints: List(UtfCodepoint),
  index: Int,
  last_line_start: Int,
  line: Int,
  position: Int,
) -> SourceLocation {
  case codepoints {
    [] -> SourceLocation(line: line, column: position + 1 - last_line_start)
    [head, ..rest] -> {
      let value = string.utf_codepoint_to_int(head)
      // \r\n consumes both code points and increments the line once.
      case value == cr && peek_lf(rest) {
        True ->
          case index + 1 >= position {
            True ->
              SourceLocation(
                line: line,
                column: position + 1 - last_line_start,
              )
            False ->
              scan(
                list.drop(rest, 1),
                index + 2,
                index + 2,
                line + 1,
                position,
              )
          }
        False ->
          case value == cr || value == lf {
            True ->
              case index >= position {
                True ->
                  SourceLocation(
                    line: line,
                    column: position + 1 - last_line_start,
                  )
                False ->
                  scan(rest, index + 1, index + 1, line + 1, position)
              }
            False -> scan(rest, index + 1, last_line_start, line, position)
          }
      }
    }
  }
}

fn peek_lf(codepoints: List(UtfCodepoint)) -> Bool {
  case codepoints {
    [head, ..] -> string.utf_codepoint_to_int(head) == lf
    [] -> False
  }
}

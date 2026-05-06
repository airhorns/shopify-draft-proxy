import gleam/list
import gleam/string

/// Mirror `normalizeFunctionHandle`. Lowercases, trims, replaces runs of
/// disallowed characters with `-`, strips leading/trailing `-`, and
/// returns `local-function` if the result is empty.
@internal
pub fn normalize_function_handle(handle: String) -> String {
  let lowered = string.lowercase(string.trim(handle))
  let mapped =
    string.to_graphemes(lowered)
    |> list.fold(#([], False), fn(acc, char) {
      let #(out, in_bad_run) = acc
      case is_handle_char(char) {
        True -> #(list.append(out, [char]), False)
        False ->
          case in_bad_run {
            True -> #(out, True)
            False -> #(list.append(out, ["-"]), True)
          }
      }
    })
  let #(chars, _) = mapped
  let joined = string.join(chars, "")
  let trimmed = trim_dashes(joined)
  case trimmed {
    "" -> "local-function"
    _ -> trimmed
  }
}

fn is_handle_char(char: String) -> Bool {
  case char {
    "a" | "b" | "c" | "d" | "e" | "f" | "g" | "h" | "i" | "j" -> True
    "k" | "l" | "m" | "n" | "o" | "p" | "q" | "r" | "s" | "t" -> True
    "u" | "v" | "w" | "x" | "y" | "z" -> True
    "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" -> True
    "_" | "-" -> True
    _ -> False
  }
}

fn trim_dashes(s: String) -> String {
  let chars = string.to_graphemes(s)
  let dropped_left = list.drop_while(chars, fn(c) { c == "-" })
  list.reverse(dropped_left)
  |> list.drop_while(fn(c) { c == "-" })
  |> list.reverse()
  |> string.join("")
}

/// Build a deterministic ShopifyFunction gid from a handle. Mirrors
/// `shopifyFunctionIdFromHandle`.
@internal
pub fn shopify_function_id_from_handle(handle: String) -> String {
  "gid://shopify/ShopifyFunction/" <> normalize_function_handle(handle)
}

/// Convert a handle to a human-readable title. Mirrors `titleFromHandle`
/// — splits on `-`, `_`, and whitespace; drops empty segments;
/// title-cases each segment; joins with a single space.
@internal
pub fn title_from_handle(handle: String) -> String {
  string.to_graphemes(handle)
  |> split_on_handle_separators([], [])
  |> list.filter(fn(seg) { seg != "" })
  |> list.map(capitalize_segment)
  |> string.join(" ")
}

fn split_on_handle_separators(
  remaining: List(String),
  current: List(String),
  acc: List(List(String)),
) -> List(String) {
  case remaining {
    [] ->
      list.append(acc, [list.reverse(current)])
      |> list.map(string.join(_, ""))
    [char, ..rest] ->
      case is_handle_separator(char) {
        True ->
          split_on_handle_separators(
            rest,
            [],
            list.append(acc, [list.reverse(current)]),
          )
        False -> split_on_handle_separators(rest, [char, ..current], acc)
      }
  }
}

fn is_handle_separator(char: String) -> Bool {
  case char {
    "-" | "_" | " " | "\t" | "\n" | "\r" -> True
    _ -> False
  }
}

fn capitalize_segment(segment: String) -> String {
  case string.to_graphemes(segment) {
    [] -> ""
    [first, ..rest] -> string.uppercase(first) <> string.join(rest, "")
  }
}

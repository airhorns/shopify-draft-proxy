//// Shared Shopify-like handle normalization helpers.

import gleam/list
import gleam/string

pub const generic_dynamic_content_translation_handle = "store-localization/generic-dynamic-content-translation"

pub const handle_max_length = 255

pub fn normalize(value: String, fallback: String) -> String {
  let normalized =
    value
    |> string.trim
    |> string.lowercase
    |> string.to_graphemes
    |> list.fold(#([], ""), fn(acc, grapheme) {
      let #(parts, current) = acc
      case is_handle_grapheme(grapheme) {
        True -> #(parts, current <> grapheme)
        False ->
          case current {
            "" -> #(parts, "")
            _ -> #([current, ..parts], "")
          }
      }
    })
    |> finish_handle_parts

  case normalized {
    "" -> fallback
    _ -> normalized
  }
}

pub fn normalize_translation_handle(value: String) -> String {
  let normalized =
    value
    |> transliterate_captured_latin
    |> normalize(generic_dynamic_content_translation_handle)

  case normalized {
    "/" -> generic_dynamic_content_translation_handle
    _ -> normalized
  }
}

pub fn finish_handle_parts(parts_state: #(List(String), String)) -> String {
  let #(parts, current) = parts_state
  let parts = case current {
    "" -> parts
    _ -> [current, ..parts]
  }
  parts
  |> list.reverse
  |> string.join("-")
}

pub fn is_handle_grapheme(grapheme: String) -> Bool {
  case grapheme {
    "_" -> True
    "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" -> True
    "a" | "b" | "c" | "d" | "e" | "f" | "g" | "h" | "i" | "j" -> True
    "k" | "l" | "m" | "n" | "o" | "p" | "q" | "r" | "s" | "t" -> True
    "u" | "v" | "w" | "x" | "y" | "z" -> True
    _ -> is_non_ascii_grapheme(grapheme)
  }
}

pub fn is_ascii_handle_grapheme(grapheme: String) -> Bool {
  case grapheme {
    "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" -> True
    "a" | "b" | "c" | "d" | "e" | "f" | "g" | "h" | "i" | "j" -> True
    "k" | "l" | "m" | "n" | "o" | "p" | "q" | "r" | "s" | "t" -> True
    "u" | "v" | "w" | "x" | "y" | "z" -> True
    _ -> False
  }
}

fn is_non_ascii_grapheme(grapheme: String) -> Bool {
  case grapheme |> string.to_utf_codepoints {
    [codepoint] -> {
      let code = string.utf_codepoint_to_int(codepoint)
      code > 127
    }
    _ -> False
  }
}

fn transliterate_captured_latin(value: String) -> String {
  value
  |> replace_any(["à", "á", "â", "ã", "ä", "å"], "a")
  |> replace_any(["æ"], "ae")
  |> replace_any(["ç"], "c")
  |> replace_any(["è", "é", "ê", "ë"], "e")
  |> replace_any(["ì", "í", "î", "ï"], "i")
  |> replace_any(["ñ"], "n")
  |> replace_any(["ò", "ó", "ô", "õ", "ö", "ø"], "o")
  |> replace_any(["œ"], "oe")
  |> replace_any(["ù", "ú", "û", "ü"], "u")
  |> replace_any(["ý", "ÿ"], "y")
  |> string.replace("ß", "ss")
}

fn replace_any(
  value: String,
  needles: List(String),
  replacement: String,
) -> String {
  list.fold(needles, value, fn(acc, needle) {
    string.replace(acc, needle, replacement)
  })
}

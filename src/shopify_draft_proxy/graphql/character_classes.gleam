//// Mirrors `graphql-js` `language/characterClasses.ts`.
////
//// All predicates take a Unicode code point as an `Int` and return whether
//// the code point participates in the named GraphQL lexical class. Inputs
//// outside the BMP are accepted but only ASCII-range answers are meaningful;
//// graphql-js itself only checks for ASCII letters/digits in these classes.

/// `WhiteSpace :: Horizontal Tab (U+0009) | Space (U+0020)`
pub fn is_white_space(code: Int) -> Bool {
  code == 0x0009 || code == 0x0020
}

/// `Digit :: 0..9`
pub fn is_digit(code: Int) -> Bool {
  code >= 0x0030 && code <= 0x0039
}

/// `Letter :: A..Z | a..z`
pub fn is_letter(code: Int) -> Bool {
  { code >= 0x0061 && code <= 0x007a } || { code >= 0x0041 && code <= 0x005a }
}

/// `NameStart :: Letter | "_"`
pub fn is_name_start(code: Int) -> Bool {
  is_letter(code) || code == 0x005f
}

/// `NameContinue :: Letter | Digit | "_"`
pub fn is_name_continue(code: Int) -> Bool {
  is_letter(code) || is_digit(code) || code == 0x005f
}

/// `HexDigit :: 0..9 | A..F | a..f`. Returns the integer value 0..15, or
/// `-1` when `code` is not a hex digit (matches graphql-js's sentinel).
pub fn read_hex_digit(code: Int) -> Int {
  case code {
    _ if code >= 0x0030 && code <= 0x0039 -> code - 0x0030
    _ if code >= 0x0041 && code <= 0x0046 -> code - 0x0037
    _ if code >= 0x0061 && code <= 0x0066 -> code - 0x0057
    _ -> -1
  }
}

/// Whether a code point is a valid Unicode scalar value (anything except
/// surrogate code points). graphql-js `isUnicodeScalarValue`.
pub fn is_unicode_scalar_value(code: Int) -> Bool {
  { code >= 0x0000 && code <= 0xd7ff } || { code >= 0xe000 && code <= 0x10ffff }
}

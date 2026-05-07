//// Shared Shopify-like phone number normalization.

import gleam/option.{type Option, None, Some}
import gleam/string
import shopify_draft_proxy/state/store.{type Store}

pub fn normalize_for_store(store: Store, phone: String) -> Result(String, Nil) {
  let trimmed = string.trim(phone)
  let digits = digits_only(trimmed)
  case phone_has_only_supported_characters(trimmed), starts_with_plus(trimmed) {
    False, _ -> Error(Nil)
    True, True -> validate_e164_digits(digits)
    True, False -> {
      let calling_code = country_calling_code(shop_country_code(store))
      let local_digits = case
        string.starts_with(digits, calling_code) && string.length(digits) > 10
      {
        True -> digits
        False -> calling_code <> digits
      }
      validate_e164_digits(local_digits)
    }
  }
}

pub fn validate_e164_digits(digits: String) -> Result(String, Nil) {
  let length = string.length(digits)
  case length >= 8 && length <= 15 && all_digits(digits) {
    True -> Ok("+" <> digits)
    False -> Error(Nil)
  }
}

pub fn shop_country_code(store: Store) -> String {
  case store.get_effective_shop(store) {
    Some(shop) ->
      shop.shop_address.country_code_v2
      |> option.map(string.uppercase)
      |> option.unwrap("US")
    None -> "US"
  }
}

pub fn country_calling_code(country_code: String) -> String {
  case country_code {
    "US" | "CA" -> "1"
    "GB" | "GG" | "IM" | "JE" -> "44"
    "AU" -> "61"
    "NZ" -> "64"
    "FR" -> "33"
    "DE" -> "49"
    "ES" -> "34"
    "IT" -> "39"
    "NL" -> "31"
    "BE" -> "32"
    "CH" -> "41"
    "AT" -> "43"
    "DK" -> "45"
    "FI" -> "358"
    "IE" -> "353"
    "NO" -> "47"
    "SE" -> "46"
    "BR" -> "55"
    "MX" -> "52"
    "JP" -> "81"
    "SG" -> "65"
    _ -> "1"
  }
}

pub fn digits_only(value: String) -> String {
  case string.pop_grapheme(value) {
    Error(_) -> ""
    Ok(#(grapheme, rest)) ->
      case normalized_digit(grapheme) {
        Some(digit) -> digit <> digits_only(rest)
        None -> digits_only(rest)
      }
  }
}

pub fn all_digits(value: String) -> Bool {
  case string.pop_grapheme(value) {
    Error(_) -> True
    Ok(#(grapheme, rest)) ->
      case normalized_digit(grapheme) {
        Some(_) -> all_digits(rest)
        None -> False
      }
  }
}

fn starts_with_plus(value: String) -> Bool {
  string.starts_with(value, "+") || string.starts_with(value, "\u{FF0B}")
}

fn phone_has_only_supported_characters(value: String) -> Bool {
  case string.pop_grapheme(value) {
    Error(_) -> True
    Ok(#(grapheme, rest)) ->
      case normalized_digit(grapheme), supported_format_character(grapheme) {
        Some(_), _ -> phone_has_only_supported_characters(rest)
        None, True -> phone_has_only_supported_characters(rest)
        None, False -> False
      }
  }
}

fn supported_format_character(grapheme: String) -> Bool {
  case grapheme {
    "+"
    | "\u{FF0B}"
    | " "
    | "\t"
    | "\n"
    | "\r"
    | "("
    | ")"
    | "-"
    | "\u{2010}"
    | "\u{2011}"
    | "\u{2012}"
    | "\u{2013}"
    | "\u{2014}"
    | "."
    | "\u{00A0}" -> True
    _ -> False
  }
}

fn normalized_digit(grapheme: String) -> Option(String) {
  case grapheme {
    "0" | "\u{FF10}" -> Some("0")
    "1" | "\u{FF11}" -> Some("1")
    "2" | "\u{FF12}" -> Some("2")
    "3" | "\u{FF13}" -> Some("3")
    "4" | "\u{FF14}" -> Some("4")
    "5" | "\u{FF15}" -> Some("5")
    "6" | "\u{FF16}" -> Some("6")
    "7" | "\u{FF17}" -> Some("7")
    "8" | "\u{FF18}" -> Some("8")
    "9" | "\u{FF19}" -> Some("9")
    _ -> None
  }
}

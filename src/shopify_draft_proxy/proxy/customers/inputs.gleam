//// Customer domain internals split from proxy/customers.gleam.

import gleam/dict.{type Dict}
import gleam/float
import gleam/int
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/customers/customer_types.{
  type AddressZoneResolution, type UserError, AddressZoneResolution, UserError,
}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/store/types as store_types
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type CustomerAddressRecord, type CustomerMetafieldRecord, type CustomerRecord,
  type Money, CustomerAddressRecord, CustomerEmailMarketingConsentRecord,
  CustomerMetafieldRecord, CustomerRecord, CustomerSmsMarketingConsentRecord,
  Money,
}

@internal
pub fn option_to_result(value: Option(a)) -> Result(a, Nil) {
  case value {
    Some(v) -> Ok(v)
    None -> Error(Nil)
  }
}

@internal
pub fn read_obj_string(
  obj: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(String) {
  case dict.get(obj, name) {
    Ok(root_field.StringVal(s)) ->
      case s {
        "" -> None
        _ -> Some(s)
      }
    Ok(root_field.IntVal(i)) -> Some(int.to_string(i))
    _ -> None
  }
}

@internal
pub fn read_customer_email(
  obj: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(String) {
  read_obj_string(obj, name)
  |> option.then(fn(value) {
    case sanitize_customer_email(value) {
      "" -> None
      sanitized -> Some(sanitized)
    }
  })
}

@internal
pub fn sanitize_customer_email(email: String) -> String {
  email
  |> string.to_graphemes
  |> list.filter(fn(grapheme) {
    !is_customer_email_removed_whitespace(grapheme)
  })
  |> string.concat
}

@internal
pub fn normalize_customer_email_for_comparison(email: String) -> String {
  email
  |> sanitize_customer_email
  |> string.lowercase
}

@internal
pub fn update_nullable_customer_email(
  existing: Option(String),
  input: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(String) {
  case dict.get(input, name) {
    Ok(root_field.StringVal(value)) -> {
      let sanitized = sanitize_customer_email(value)
      case sanitized {
        "" -> None
        _ -> Some(sanitized)
      }
    }
    Ok(root_field.NullVal) -> None
    _ -> existing
  }
}

@internal
pub fn customer_email_pattern_is_valid(email: String) -> Bool {
  string.length(email) <= 255
  && case string.split(email, "@") {
    [local, domain] ->
      // Approximation of Shopify's upstream
      // `EmailAddressValidator::EmailAddress#pattern_is_valid?`, after
      // `CustomerFoundations::EmailAddress::Valid` strips whitespace.
      local != ""
      && local_matches_customer_email_pattern(local)
      && domain_matches_customer_email_pattern(domain)
    _ -> False
  }
}

fn is_customer_email_removed_whitespace(grapheme: String) -> Bool {
  grapheme == " "
  || grapheme == "\n"
  || grapheme == "\r"
  || grapheme == "\t"
  || grapheme == "\u{000B}"
  || grapheme == "\u{000C}"
}

fn local_matches_customer_email_pattern(local: String) -> Bool {
  local
  |> string.to_utf_codepoints
  |> list.all(is_customer_email_local_codepoint)
}

fn domain_matches_customer_email_pattern(domain: String) -> Bool {
  let parts = string.split(domain, ".")
  list.length(parts) >= 2
  && list.all(parts, is_valid_customer_email_domain_label)
}

fn is_valid_customer_email_domain_label(label: String) -> Bool {
  case string.length(label) <= 63 {
    False -> False
    True ->
      case string.to_utf_codepoints(label) {
        [] -> False
        [first] -> is_ascii_alphanumeric_codepoint(first)
        [first, ..rest] ->
          case list.last(rest) {
            Ok(last) ->
              is_ascii_alphanumeric_codepoint(first)
              && is_ascii_alphanumeric_codepoint(last)
              && list.all(rest, is_customer_email_domain_codepoint)
            Error(_) -> False
          }
      }
  }
}

fn is_customer_email_local_codepoint(codepoint) -> Bool {
  let code = string.utf_codepoint_to_int(codepoint)
  is_ascii_alphanumeric(code)
  || list.contains(
    [
      33, 35, 36, 37, 38, 39, 42, 43, 45, 46, 47, 61, 63, 94, 95, 96, 123, 124,
      125, 126,
    ],
    code,
  )
}

fn is_customer_email_domain_codepoint(codepoint) -> Bool {
  let code = string.utf_codepoint_to_int(codepoint)
  is_ascii_alphanumeric(code) || code == 45
}

fn is_ascii_alphanumeric_codepoint(codepoint) -> Bool {
  codepoint
  |> string.utf_codepoint_to_int
  |> is_ascii_alphanumeric
}

fn is_ascii_alphanumeric(codepoint: Int) -> Bool {
  codepoint >= 48
  && codepoint <= 57
  || codepoint >= 65
  && codepoint <= 90
  || codepoint >= 97
  && codepoint <= 122
}

@internal
pub fn read_obj_raw_string(
  obj: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(String) {
  case dict.get(obj, name) {
    Ok(root_field.StringVal(s)) -> Some(s)
    Ok(root_field.IntVal(i)) -> Some(int.to_string(i))
    _ -> None
  }
}

@internal
pub fn read_normalized_optional_string(
  obj: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(String) {
  read_obj_string(obj, name)
  |> option.then(fn(value) {
    case string.trim(value) {
      "" -> None
      trimmed -> Some(trimmed)
    }
  })
}

@internal
pub fn read_normalized_string_with_blank(
  obj: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(String) {
  case dict.get(obj, name) {
    Ok(root_field.StringVal(value)) -> Some(string.trim(value))
    _ -> None
  }
}

@internal
pub fn read_obj_bool(
  obj: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(Bool) {
  case dict.get(obj, name) {
    Ok(root_field.BoolVal(b)) -> Some(b)
    _ -> None
  }
}

@internal
pub fn read_obj_array_strings(
  obj: Dict(String, root_field.ResolvedValue),
  name: String,
) -> List(String) {
  case dict.get(obj, name) {
    Ok(root_field.ListVal(items)) ->
      list.filter_map(items, fn(item) {
        case item {
          root_field.StringVal(s) -> Ok(s)
          _ -> Error(Nil)
        }
      })
    Ok(root_field.StringVal(s)) -> split_tags(s)
    _ -> []
  }
}

@internal
pub fn update_trimmed_nullable_string(
  existing: Option(String),
  input: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(String) {
  case dict.get(input, name) {
    Ok(root_field.StringVal(value)) -> {
      let trimmed = string.trim(value)
      case trimmed {
        "" -> None
        _ -> Some(trimmed)
      }
    }
    Ok(root_field.NullVal) -> None
    _ -> existing
  }
}

@internal
pub fn update_nullable_note(
  existing: Option(String),
  input: Dict(String, root_field.ResolvedValue),
) -> Option(String) {
  case dict.get(input, "note") {
    Ok(root_field.StringVal(value)) -> Some(value)
    Ok(root_field.NullVal) -> None
    _ -> existing
  }
}

@internal
pub fn json_get(
  value: commit.JsonValue,
  key: String,
) -> Option(commit.JsonValue) {
  case value {
    commit.JsonObject(fields) ->
      list.find_map(fields, fn(pair) {
        case pair {
          #(name, child) if name == key -> Ok(child)
          _ -> Error(Nil)
        }
      })
      |> option.from_result
    _ -> None
  }
}

@internal
pub fn non_null_json(
  value: Option(commit.JsonValue),
) -> Option(commit.JsonValue) {
  case value {
    Some(commit.JsonNull) -> None
    Some(v) -> Some(v)
    None -> None
  }
}

@internal
pub fn json_get_string(value: commit.JsonValue, key: String) -> Option(String) {
  json_get_scalar_string(value, key)
}

@internal
pub fn json_get_scalar_string(
  value: commit.JsonValue,
  key: String,
) -> Option(String) {
  json_get(value, key) |> option.then(json_scalar_string)
}

@internal
pub fn json_scalar_string(value: commit.JsonValue) -> Option(String) {
  case value {
    commit.JsonString(s) -> Some(s)
    commit.JsonInt(i) -> Some(int.to_string(i))
    commit.JsonFloat(f) -> Some(float.to_string(f))
    _ -> None
  }
}

@internal
pub fn json_get_bool(value: commit.JsonValue, key: String) -> Option(Bool) {
  case json_get(value, key) {
    Some(commit.JsonBool(b)) -> Some(b)
    _ -> None
  }
}

@internal
pub fn json_get_int(value: commit.JsonValue, key: String) -> Option(Int) {
  case json_get(value, key) {
    Some(commit.JsonInt(i)) -> Some(i)
    Some(commit.JsonString(s)) ->
      case int.parse(s) {
        Ok(i) -> Some(i)
        Error(_) -> None
      }
    _ -> None
  }
}

@internal
pub fn json_get_string_list(
  value: commit.JsonValue,
  key: String,
) -> List(String) {
  case json_get(value, key) {
    Some(commit.JsonArray(items)) ->
      list.filter_map(items, fn(item) {
        case json_scalar_string(item) {
          Some(s) -> Ok(s)
          None -> Error(Nil)
        }
      })
    _ -> []
  }
}

@internal
pub fn json_get_money(value: commit.JsonValue, key: String) -> Option(Money) {
  json_money_from_value(json_get(value, key))
}

@internal
pub fn json_money_from_value(value: Option(commit.JsonValue)) -> Option(Money) {
  use money <- option.then(non_null_json(value))
  use amount <- option.then(json_get_scalar_string(money, "amount"))
  let currency =
    json_get_string(money, "currencyCode")
    |> option.or(json_get_string(money, "currency_code"))
    |> option.unwrap("USD")
  Some(Money(amount: amount, currency_code: currency))
}

@internal
pub fn read_obj_addresses(
  input: Dict(String, root_field.ResolvedValue),
) -> List(Dict(String, root_field.ResolvedValue)) {
  read_obj_list_objects(input, "addresses")
}

@internal
pub fn read_obj_list_objects(
  input: Dict(String, root_field.ResolvedValue),
  key: String,
) -> List(Dict(String, root_field.ResolvedValue)) {
  case dict.get(input, key) {
    Ok(root_field.ListVal(items)) ->
      list.filter_map(items, fn(item) {
        case item {
          root_field.ObjectVal(d) -> Ok(d)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

@internal
pub fn dedupe_customer_addresses(
  addresses: List(CustomerAddressRecord),
  kept: List(CustomerAddressRecord),
) -> List(CustomerAddressRecord) {
  case addresses {
    [] -> list.reverse(kept)
    [address, ..rest] ->
      case
        list.any(kept, fn(existing) {
          customer_addresses_match(existing, address)
        })
      {
        True -> dedupe_customer_addresses(rest, kept)
        False -> dedupe_customer_addresses(rest, [address, ..kept])
      }
  }
}

@internal
pub fn build_address(
  id: String,
  customer_id: String,
  position: Int,
  input: Dict(String, root_field.ResolvedValue),
  fallback_first_name: Option(String),
  fallback_last_name: Option(String),
) -> CustomerAddressRecord {
  let first_name =
    read_obj_string(input, "firstName") |> option.or(fallback_first_name)
  let last_name =
    read_obj_string(input, "lastName") |> option.or(fallback_last_name)
  let #(zone, _) = resolve_address_zone(input, None, [])
  CustomerAddressRecord(
    id: id,
    customer_id: customer_id,
    cursor: None,
    position: position,
    first_name: first_name,
    last_name: last_name,
    address1: read_obj_string(input, "address1"),
    address2: read_obj_string(input, "address2"),
    city: read_obj_string(input, "city"),
    company: read_obj_string(input, "company"),
    province: zone.province,
    province_code: zone.province_code,
    country: zone.country,
    country_code_v2: zone.country_code,
    zip: read_obj_string(input, "zip"),
    phone: read_obj_string(input, "phone"),
    name: build_display_name(first_name, last_name, None),
    formatted_area: formatted_area(
      read_obj_string(input, "city"),
      zone.province_code,
      zone.country,
    ),
  )
}

@internal
pub fn merge_address(
  existing: CustomerAddressRecord,
  input: Dict(String, root_field.ResolvedValue),
) -> CustomerAddressRecord {
  let first_name =
    read_obj_string(input, "firstName") |> option.or(existing.first_name)
  let last_name =
    read_obj_string(input, "lastName") |> option.or(existing.last_name)
  let #(zone, _) = resolve_address_zone(input, Some(existing), [])
  CustomerAddressRecord(
    ..existing,
    first_name: first_name,
    last_name: last_name,
    address1: read_obj_string(input, "address1") |> option.or(existing.address1),
    address2: read_obj_string(input, "address2") |> option.or(existing.address2),
    city: read_obj_string(input, "city") |> option.or(existing.city),
    company: read_obj_string(input, "company") |> option.or(existing.company),
    province: zone.province,
    province_code: zone.province_code,
    country: zone.country,
    country_code_v2: zone.country_code,
    zip: read_obj_string(input, "zip") |> option.or(existing.zip),
    phone: read_obj_string(input, "phone") |> option.or(existing.phone),
    name: build_display_name(first_name, last_name, None),
    formatted_area: formatted_area(
      read_obj_string(input, "city") |> option.or(existing.city),
      zone.province_code,
      zone.country,
    ),
  )
}

@internal
pub fn read_customer_metafields(input, customer_id, _identity) {
  case dict.get(input, "metafields") {
    Ok(root_field.ListVal(items)) ->
      list.index_map(items, fn(item, index) {
        case item {
          root_field.ObjectVal(obj) -> {
            let namespace =
              read_obj_string(obj, "namespace") |> option.unwrap("")
            let key = read_obj_string(obj, "key") |> option.unwrap("")
            let id =
              "gid://shopify/Metafield/"
              <> gid_tail(customer_id) |> option.unwrap("0")
              <> "-"
              <> int.to_string(index + 1)
            Ok(CustomerMetafieldRecord(
              id: id,
              customer_id: customer_id,
              namespace: namespace,
              key: key,
              type_: read_obj_string(obj, "type")
                |> option.unwrap("single_line_text_field"),
              value: read_obj_string(obj, "value") |> option.unwrap(""),
              compare_digest: None,
              created_at: None,
              updated_at: None,
            ))
          }
          _ -> Error(Nil)
        }
      })
      |> list.filter_map(fn(x) { x })
    _ -> []
  }
}

@internal
pub fn read_nested_object(input, key) {
  case dict.get(input, key) {
    Ok(root_field.ObjectVal(d)) -> d
    _ -> dict.new()
  }
}

@internal
pub fn has_nested_object(input, key) {
  case dict.get(input, key) {
    Ok(root_field.ObjectVal(_)) -> True
    _ -> False
  }
}

@internal
pub fn make_email_consent(input) {
  make_email_consent_from(read_nested_object(input, "emailMarketingConsent"))
}

@internal
pub fn make_email_consent_from(consent) {
  case dict.is_empty(consent) {
    True -> None
    False ->
      Some(CustomerEmailMarketingConsentRecord(
        marketing_state: read_obj_string(consent, "marketingState"),
        marketing_opt_in_level: read_obj_string(consent, "marketingOptInLevel"),
        consent_updated_at: read_obj_string(consent, "consentUpdatedAt"),
      ))
  }
}

@internal
pub fn make_sms_consent(input) {
  make_sms_consent_from(read_nested_object(input, "smsMarketingConsent"))
}

@internal
pub fn make_sms_consent_from(consent) {
  case dict.is_empty(consent) {
    True -> None
    False ->
      Some(CustomerSmsMarketingConsentRecord(
        marketing_state: read_obj_string(consent, "marketingState"),
        marketing_opt_in_level: read_obj_string(consent, "marketingOptInLevel"),
        consent_updated_at: read_obj_string(consent, "consentUpdatedAt"),
        consent_collected_from: Some("OTHER"),
      ))
  }
}

@internal
pub fn consent_state_from_input(input, key) {
  read_obj_string(read_nested_object(input, key), "marketingState")
}

@internal
pub fn consent_level_from_input(input, key) {
  read_obj_string(read_nested_object(input, key), "marketingOptInLevel")
}

@internal
pub fn consent_updated_at_from_input(input, key) {
  read_obj_string(read_nested_object(input, key), "consentUpdatedAt")
}

@internal
pub fn consent_collected_from_input(input, key) {
  read_obj_string(read_nested_object(input, key), "consentCollectedFrom")
}

@internal
pub fn build_merged_customer(
  one: CustomerRecord,
  two: CustomerRecord,
  override: Dict(String, root_field.ResolvedValue),
  timestamp: String,
) -> CustomerRecord {
  let email_source =
    read_customer_id_override(override, "customerIdOfEmailToKeep", one, two)
    |> option.unwrap(two)
  let phone_source =
    read_customer_id_override(
      override,
      "customerIdOfPhoneNumberToKeep",
      one,
      two,
    )
    |> option.unwrap(two)
  let email =
    read_obj_string(override, "email")
    |> option.or(email_source.email)
  let first_name =
    read_obj_string(override, "firstName")
    |> option.or(
      select_customer_override_field(
        override,
        "customerIdOfFirstNameToKeep",
        one,
        two,
        fn(customer) { customer.first_name },
      ),
    )
  let last_name =
    read_obj_string(override, "lastName")
    |> option.or(
      select_customer_override_field(
        override,
        "customerIdOfLastNameToKeep",
        one,
        two,
        fn(customer) { customer.last_name },
      ),
    )
  let default_address =
    select_customer_override_field(
      override,
      "customerIdOfDefaultAddressToKeep",
      one,
      two,
      fn(customer) { customer.default_address },
    )
  CustomerRecord(
    ..two,
    first_name: first_name,
    last_name: last_name,
    display_name: build_display_name(first_name, last_name, email),
    email: email,
    note: read_obj_string(override, "note")
      |> option.or(two.note)
      |> option.or(one.note),
    tags: case read_obj_array_strings(override, "tags") {
      [] -> normalize_tags(list.append(one.tags, two.tags))
      tags -> normalize_tags(tags)
    },
    default_email_address: email_source.default_email_address,
    default_phone_number: phone_source.default_phone_number,
    email_marketing_consent: email_source.email_marketing_consent,
    sms_marketing_consent: phone_source.sms_marketing_consent,
    default_address: default_address,
    account_activation_token: None,
    created_at: one.created_at,
    updated_at: Some(timestamp),
  )
}

@internal
pub fn read_customer_id_override(
  override: Dict(String, root_field.ResolvedValue),
  field: String,
  one: CustomerRecord,
  two: CustomerRecord,
) -> Option(CustomerRecord) {
  case read_obj_string(override, field) {
    Some(id) ->
      case id == one.id, id == two.id {
        True, _ -> Some(one)
        _, True -> Some(two)
        _, _ -> None
      }
    None -> None
  }
}

@internal
pub fn select_customer_override_field(
  override: Dict(String, root_field.ResolvedValue),
  field: String,
  one: CustomerRecord,
  two: CustomerRecord,
  selector: fn(CustomerRecord) -> a,
) -> a {
  let source =
    read_customer_id_override(override, field, one, two)
    |> option.unwrap(two)
  selector(source)
}

@internal
pub fn read_money(input) -> Option(Money) {
  let amount_obj =
    first_non_empty_object([
      read_nested_object(input, "amount"),
      read_nested_object(input, "creditAmount"),
      read_nested_object(input, "debitAmount"),
    ])
  case
    read_obj_string(amount_obj, "amount"),
    read_obj_string(amount_obj, "currencyCode")
  {
    Some(amount), Some(currency) -> Some(Money(amount, currency))
    _, _ -> None
  }
}

@internal
pub fn first_non_empty_object(
  objects: List(Dict(String, root_field.ResolvedValue)),
) -> Dict(String, root_field.ResolvedValue) {
  case objects {
    [] -> dict.new()
    [first, ..rest] ->
      case dict.is_empty(first) {
        True -> first_non_empty_object(rest)
        False -> first
      }
  }
}

@internal
pub fn parse_cents(amount: String) -> Int {
  case float.parse(amount) {
    Ok(value) -> float.round(value *. 100.0)
    Error(_) ->
      case int.parse(amount) {
        Ok(value) -> value * 100
        Error(_) -> 0
      }
  }
}

@internal
pub fn format_cents(cents: Int) -> String {
  let whole = cents / 100
  let frac = int.absolute_value(cents % 100)
  case frac {
    0 -> int.to_string(whole) <> ".0"
    n if n < 10 -> int.to_string(whole) <> ".0" <> int.to_string(n)
    n -> int.to_string(whole) <> "." <> int.to_string(n)
  }
}

@internal
pub fn build_display_name(first_name, last_name, email) {
  let name =
    string.trim(string.join(
      [option.unwrap(first_name, ""), option.unwrap(last_name, "")],
      " ",
    ))
  case name {
    "" -> email
    _ -> Some(name)
  }
}

@internal
pub fn split_tags(raw: String) -> List(String) {
  raw
  |> string.split(",")
  |> list.map(string.trim)
  |> list.filter(fn(s) { s != "" })
}

@internal
pub fn normalize_tags(tags: List(String)) -> List(String) {
  tags
  |> list.map(string.trim)
  |> list.filter(fn(s) { s != "" })
  |> dedupe()
  |> list.sort(fn(a, b) {
    string.compare(string.lowercase(a), string.lowercase(b))
  })
}

@internal
pub fn dedupe(items: List(String)) -> List(String) {
  list.fold(items, [], fn(acc, item) {
    case list.contains(acc, item) {
      True -> acc
      False -> list.append(acc, [item])
    }
  })
}

@internal
pub fn validate_address_input(
  input: Dict(String, root_field.ResolvedValue),
  existing: Option(CustomerAddressRecord),
  field_prefix: List(String),
) -> List(UserError) {
  let #(_, errors) = resolve_address_zone(input, existing, field_prefix)
  errors
}

@internal
pub fn resolve_address_zone(
  input: Dict(String, root_field.ResolvedValue),
  existing: Option(CustomerAddressRecord),
  field_prefix: List(String),
) -> #(AddressZoneResolution, List(UserError)) {
  let code_input =
    first_non_empty_string([
      read_obj_string(input, "countryCode"),
      read_obj_string(input, "countryCodeV2"),
    ])
  let country_input = read_obj_string(input, "country") |> non_empty_string
  let existing_code =
    existing
    |> option.then(fn(address) { non_empty_string(address.country_code_v2) })
  let existing_country =
    existing |> option.then(fn(address) { non_empty_string(address.country) })
  let country_result =
    resolve_country(code_input, country_input, existing_code, existing_country)
  case country_result {
    AddressCountryInvalid -> #(AddressZoneResolution(None, None, None, None), [
      UserError(
        list.append(field_prefix, ["country"]),
        "Country is invalid",
        Some("INVALID"),
      ),
    ])
    AddressCountryResolved(country_code, country_name, zones) -> {
      let province_result =
        resolve_province(input, existing, zones, field_prefix)
      case province_result {
        Error(_) -> #(
          AddressZoneResolution(
            Some(country_name),
            Some(country_code),
            None,
            None,
          ),
          [
            UserError(
              list.append(field_prefix, ["province"]),
              "Province is invalid",
              Some("INVALID"),
            ),
          ],
        )
        Ok(#(province_code, province_name)) -> #(
          AddressZoneResolution(
            Some(country_name),
            Some(country_code),
            province_name,
            province_code,
          ),
          [],
        )
      }
    }
    AddressCountryAbsent -> {
      let province_code =
        first_non_empty_string([
          read_obj_string(input, "provinceCode"),
          existing
            |> option.then(fn(address) {
              non_empty_string(address.province_code)
            }),
        ])
      let province =
        legacy_province_name(
          province_code,
          first_non_empty_string([
            read_obj_string(input, "province"),
            existing
              |> option.then(fn(address) { non_empty_string(address.province) }),
          ]),
        )
      #(AddressZoneResolution(None, None, province, province_code), [])
    }
  }
}

@internal
pub type CountryResolution {
  AddressCountryResolved(String, String, List(#(String, String)))
  AddressCountryAbsent
  AddressCountryInvalid
}

@internal
pub fn resolve_country(
  code_input: Option(String),
  country_input: Option(String),
  existing_code: Option(String),
  existing_country: Option(String),
) -> CountryResolution {
  case code_input {
    Some(code) ->
      case country_catalog_by_code(code) {
        Some(#(catalog_code, catalog_name, zones)) ->
          AddressCountryResolved(catalog_code, catalog_name, zones)
        None -> AddressCountryInvalid
      }
    None -> {
      let country_name = country_input |> option.or(existing_country)
      case country_name {
        Some(name) ->
          case country_catalog_by_name(name) {
            Some(#(catalog_code, catalog_name, zones)) ->
              AddressCountryResolved(catalog_code, catalog_name, zones)
            None -> AddressCountryInvalid
          }
        None ->
          case existing_code {
            Some(code) ->
              case country_catalog_by_code(code) {
                Some(#(catalog_code, catalog_name, zones)) ->
                  AddressCountryResolved(catalog_code, catalog_name, zones)
                None -> AddressCountryInvalid
              }
            None -> AddressCountryAbsent
          }
      }
    }
  }
}

@internal
pub fn resolve_province(
  input: Dict(String, root_field.ResolvedValue),
  existing: Option(CustomerAddressRecord),
  zones: List(#(String, String)),
  _field_prefix: List(String),
) -> Result(#(Option(String), Option(String)), Nil) {
  case zones {
    [] -> Ok(#(None, None))
    [_, ..] -> {
      let province_code =
        first_non_empty_string([
          read_obj_string(input, "provinceCode"),
          existing
            |> option.then(fn(address) {
              non_empty_string(address.province_code)
            }),
        ])
      let province_name =
        first_non_empty_string([
          read_obj_string(input, "province"),
          existing
            |> option.then(fn(address) { non_empty_string(address.province) }),
        ])
      case province_code {
        Some(code) ->
          case zone_name_by_code(zones, code) {
            Some(name) ->
              Ok(#(Some(zone_code_by_input(zones, code)), Some(name)))
            None -> Error(Nil)
          }
        None ->
          case province_name {
            Some(name) ->
              case zone_by_name(zones, name) {
                Some(#(code, display_name)) ->
                  Ok(#(Some(code), Some(display_name)))
                None -> Error(Nil)
              }
            None -> Ok(#(None, None))
          }
      }
    }
  }
}

@internal
pub fn first_non_empty_string(values: List(Option(String))) -> Option(String) {
  case values {
    [] -> None
    [first, ..rest] ->
      case non_empty_string(first) {
        Some(value) -> Some(value)
        None -> first_non_empty_string(rest)
      }
  }
}

@internal
pub fn non_empty_string(value: Option(String)) -> Option(String) {
  case value {
    Some(raw) -> {
      let trimmed = string.trim(raw)
      case trimmed {
        "" -> None
        _ -> Some(trimmed)
      }
    }
    None -> None
  }
}

@internal
pub fn country_catalog_by_code(
  code: String,
) -> Option(#(String, String, List(#(String, String)))) {
  case string.uppercase(code) {
    "CA" -> Some(#("CA", "Canada", canada_zones()))
    "US" -> Some(#("US", "United States", united_states_zones()))
    "SG" -> Some(#("SG", "Singapore", []))
    _ -> None
  }
}

@internal
pub fn country_catalog_by_name(
  name: String,
) -> Option(#(String, String, List(#(String, String)))) {
  case string.lowercase(string.trim(name)) {
    "canada" -> country_catalog_by_code("CA")
    "united states" | "united states of america" | "usa" | "us" ->
      country_catalog_by_code("US")
    "singapore" -> country_catalog_by_code("SG")
    _ -> None
  }
}

@internal
pub fn zone_name_by_code(
  zones: List(#(String, String)),
  code: String,
) -> Option(String) {
  let normalized = string.uppercase(code)
  zones
  |> list.find(fn(zone) {
    let #(zone_code, _) = zone
    zone_code == normalized
  })
  |> result_to_option()
  |> option.map(fn(zone) {
    let #(_, name) = zone
    name
  })
}

@internal
pub fn zone_code_by_input(
  zones: List(#(String, String)),
  code: String,
) -> String {
  let normalized = string.uppercase(code)
  zones
  |> list.find(fn(zone) {
    let #(zone_code, _) = zone
    zone_code == normalized
  })
  |> result.map(fn(zone) {
    let #(zone_code, _) = zone
    zone_code
  })
  |> result.unwrap(normalized)
}

@internal
pub fn zone_by_name(
  zones: List(#(String, String)),
  name: String,
) -> Option(#(String, String)) {
  let normalized = string.lowercase(string.trim(name))
  zones
  |> list.find(fn(zone) {
    let #(_, zone_name) = zone
    string.lowercase(zone_name) == normalized
  })
  |> result_to_option()
}

@internal
pub fn legacy_province_name(code, fallback) {
  case code {
    Some("ON") -> Some("Ontario")
    Some("QC") -> Some("Quebec")
    Some("BC") -> Some("British Columbia")
    Some("CA") -> Some("California")
    Some("NY") -> Some("New York")
    _ -> fallback
  }
}

@internal
pub fn canada_zones() -> List(#(String, String)) {
  [
    #("AB", "Alberta"),
    #("BC", "British Columbia"),
    #("MB", "Manitoba"),
    #("NB", "New Brunswick"),
    #("NL", "Newfoundland and Labrador"),
    #("NT", "Northwest Territories"),
    #("NS", "Nova Scotia"),
    #("NU", "Nunavut"),
    #("ON", "Ontario"),
    #("PE", "Prince Edward Island"),
    #("QC", "Quebec"),
    #("SK", "Saskatchewan"),
    #("YT", "Yukon"),
  ]
}

@internal
pub fn united_states_zones() -> List(#(String, String)) {
  [
    #("AL", "Alabama"),
    #("AK", "Alaska"),
    #("AZ", "Arizona"),
    #("AR", "Arkansas"),
    #("CA", "California"),
    #("CO", "Colorado"),
    #("CT", "Connecticut"),
    #("DE", "Delaware"),
    #("DC", "District of Columbia"),
    #("FL", "Florida"),
    #("GA", "Georgia"),
    #("HI", "Hawaii"),
    #("ID", "Idaho"),
    #("IL", "Illinois"),
    #("IN", "Indiana"),
    #("IA", "Iowa"),
    #("KS", "Kansas"),
    #("KY", "Kentucky"),
    #("LA", "Louisiana"),
    #("ME", "Maine"),
    #("MD", "Maryland"),
    #("MA", "Massachusetts"),
    #("MI", "Michigan"),
    #("MN", "Minnesota"),
    #("MS", "Mississippi"),
    #("MO", "Missouri"),
    #("MT", "Montana"),
    #("NE", "Nebraska"),
    #("NV", "Nevada"),
    #("NH", "New Hampshire"),
    #("NJ", "New Jersey"),
    #("NM", "New Mexico"),
    #("NY", "New York"),
    #("NC", "North Carolina"),
    #("ND", "North Dakota"),
    #("OH", "Ohio"),
    #("OK", "Oklahoma"),
    #("OR", "Oregon"),
    #("PA", "Pennsylvania"),
    #("RI", "Rhode Island"),
    #("SC", "South Carolina"),
    #("SD", "South Dakota"),
    #("TN", "Tennessee"),
    #("TX", "Texas"),
    #("UT", "Utah"),
    #("VT", "Vermont"),
    #("VA", "Virginia"),
    #("WA", "Washington"),
    #("WV", "West Virginia"),
    #("WI", "Wisconsin"),
    #("WY", "Wyoming"),
  ]
}

@internal
pub fn formatted_area(city, province_code, country) {
  let city_region =
    [city, province_code]
    |> list.filter_map(non_empty_option_string)
    |> string.join(" ")
  case city_region, country {
    value, Some(country_name) if value == country_name -> Some(country_name)
    _, _ -> {
      let parts =
        [
          case city_region {
            "" -> None
            value -> Some(value)
          },
          country,
        ]
        |> list.filter_map(non_empty_option_string)
      case parts {
        [] -> None
        _ -> Some(string.join(parts, ", "))
      }
    }
  }
}

@internal
pub fn non_empty_option_string(value: Option(String)) -> Result(String, Nil) {
  case value {
    Some(s) if s != "" -> Ok(s)
    _ -> Error(Nil)
  }
}

@internal
pub fn find_duplicate_customer_address(
  store: Store,
  customer_id: String,
  candidate: CustomerAddressRecord,
  exclude_address_id: Option(String),
) -> Option(CustomerAddressRecord) {
  store.list_effective_customer_addresses(store, customer_id)
  |> list.find(fn(address) {
    address.id != option.unwrap(exclude_address_id, "")
    && customer_addresses_match(address, candidate)
  })
  |> result_to_option()
}

@internal
pub fn customer_addresses_match(
  left: CustomerAddressRecord,
  right: CustomerAddressRecord,
) -> Bool {
  left.first_name == right.first_name
  && left.last_name == right.last_name
  && left.address1 == right.address1
  && left.address2 == right.address2
  && left.city == right.city
  && left.company == right.company
  && left.province_code == right.province_code
  && left.country_code_v2 == right.country_code_v2
  && left.zip == right.zip
  && left.phone == right.phone
}

@internal
pub fn gid_tail(id: String) -> Option(String) {
  string.split(id, "/")
  |> list.last()
  |> result_to_option()
}

@internal
pub fn result_to_option(value: Result(a, b)) -> Option(a) {
  case value {
    Ok(v) -> Some(v)
    Error(_) -> None
  }
}

@internal
pub fn record_mutation_log(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  staged_ids: List(String),
  roots: List(String),
) -> #(Store, SyntheticIdentityRegistry) {
  let #(log_id, identity_after_log_id) =
    synthetic_identity.make_synthetic_gid(identity, "MutationLogEntry")
  let #(received_at, identity_final) =
    synthetic_identity.make_synthetic_timestamp(identity_after_log_id)
  let primary = list.first(roots) |> result_to_option()
  let entry =
    store_types.MutationLogEntry(
      id: log_id,
      received_at: received_at,
      operation_name: None,
      path: request_path,
      query: document,
      variables: dict.new(),
      staged_resource_ids: staged_ids,
      status: store_types.Staged,
      interpreted: store_types.InterpretedMetadata(
        operation_type: store_types.Mutation,
        operation_name: None,
        root_fields: roots,
        primary_root_field: primary,
        capability: store_types.Capability(
          operation_name: primary,
          domain: "customers",
          execution: "stage-locally",
        ),
      ),
      notes: Some(
        "Locally staged customer-domain mutation in shopify-draft-proxy.",
      ),
    )
  #(store.record_mutation_log_entry(store, entry), identity_final)
}

@internal
pub fn customer_metafield_key(metafield: CustomerMetafieldRecord) -> String {
  metafield.namespace <> "::" <> metafield.key
}

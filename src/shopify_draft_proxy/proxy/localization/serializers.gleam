//// Localization default catalog, resource reconstruction, and GraphQL projection helpers.

import gleam/dict.{type Dict}
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/order
import gleam/string
import shopify_draft_proxy/crypto
import shopify_draft_proxy/graphql/ast.{type Selection, Field, SelectionSet}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, ConnectionPageInfoOptions, ConnectionWindow,
  SelectedFieldOptions, SerializeConnectionConfig, SrcBool, SrcFloat, SrcInt,
  SrcList, SrcNull, SrcObject, SrcString, default_connection_page_info_options,
  default_connection_window_options, default_selected_field_options,
  get_field_response_key, paginate_connection_items, project_graphql_value,
  serialize_connection, serialize_empty_connection, src_object,
}
import shopify_draft_proxy/proxy/localization/types.{
  type AnyUserError, type TranslatableContent, type TranslatableResource,
  ShopLocaleError, TranslatableContent, TranslatableResource, TranslationError,
}
import shopify_draft_proxy/proxy/mutation_helpers.{read_optional_string_array}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/types as state_types

fn default_available_locales() -> List(state_types.LocaleRecord) {
  [
    state_types.LocaleRecord(iso_code: "af", name: "Afrikaans"),
    state_types.LocaleRecord(iso_code: "ak", name: "Akan"),
    state_types.LocaleRecord(iso_code: "sq", name: "Albanian"),
    state_types.LocaleRecord(iso_code: "am", name: "Amharic"),
    state_types.LocaleRecord(iso_code: "ar", name: "Arabic"),
    state_types.LocaleRecord(iso_code: "hy", name: "Armenian"),
    state_types.LocaleRecord(iso_code: "as", name: "Assamese"),
    state_types.LocaleRecord(iso_code: "az", name: "Azerbaijani"),
    state_types.LocaleRecord(iso_code: "bm", name: "Bambara"),
    state_types.LocaleRecord(iso_code: "bn", name: "Bangla"),
    state_types.LocaleRecord(iso_code: "eu", name: "Basque"),
    state_types.LocaleRecord(iso_code: "be", name: "Belarusian"),
    state_types.LocaleRecord(iso_code: "bs", name: "Bosnian"),
    state_types.LocaleRecord(iso_code: "br", name: "Breton"),
    state_types.LocaleRecord(iso_code: "bg", name: "Bulgarian"),
    state_types.LocaleRecord(iso_code: "my", name: "Burmese"),
    state_types.LocaleRecord(iso_code: "ca", name: "Catalan"),
    state_types.LocaleRecord(iso_code: "ckb", name: "Central Kurdish"),
    state_types.LocaleRecord(iso_code: "ce", name: "Chechen"),
    state_types.LocaleRecord(iso_code: "zh-CN", name: "Chinese (Simplified)"),
    state_types.LocaleRecord(iso_code: "zh-TW", name: "Chinese (Traditional)"),
    state_types.LocaleRecord(iso_code: "kw", name: "Cornish"),
    state_types.LocaleRecord(iso_code: "hr", name: "Croatian"),
    state_types.LocaleRecord(iso_code: "cs", name: "Czech"),
    state_types.LocaleRecord(iso_code: "da", name: "Danish"),
    state_types.LocaleRecord(iso_code: "nl", name: "Dutch"),
    state_types.LocaleRecord(iso_code: "dz", name: "Dzongkha"),
    state_types.LocaleRecord(iso_code: "en", name: "English"),
    state_types.LocaleRecord(iso_code: "eo", name: "Esperanto"),
    state_types.LocaleRecord(iso_code: "et", name: "Estonian"),
    state_types.LocaleRecord(iso_code: "ee", name: "Ewe"),
    state_types.LocaleRecord(iso_code: "fo", name: "Faroese"),
    state_types.LocaleRecord(iso_code: "fil", name: "Filipino"),
    state_types.LocaleRecord(iso_code: "fi", name: "Finnish"),
    state_types.LocaleRecord(iso_code: "fr", name: "French"),
    state_types.LocaleRecord(iso_code: "ff", name: "Fulah"),
    state_types.LocaleRecord(iso_code: "gl", name: "Galician"),
    state_types.LocaleRecord(iso_code: "lg", name: "Ganda"),
    state_types.LocaleRecord(iso_code: "ka", name: "Georgian"),
    state_types.LocaleRecord(iso_code: "de", name: "German"),
    state_types.LocaleRecord(iso_code: "el", name: "Greek"),
    state_types.LocaleRecord(iso_code: "gu", name: "Gujarati"),
    state_types.LocaleRecord(iso_code: "ha", name: "Hausa"),
    state_types.LocaleRecord(iso_code: "he", name: "Hebrew"),
    state_types.LocaleRecord(iso_code: "hi", name: "Hindi"),
    state_types.LocaleRecord(iso_code: "hu", name: "Hungarian"),
    state_types.LocaleRecord(iso_code: "is", name: "Icelandic"),
    state_types.LocaleRecord(iso_code: "ig", name: "Igbo"),
    state_types.LocaleRecord(iso_code: "id", name: "Indonesian"),
    state_types.LocaleRecord(iso_code: "ia", name: "Interlingua"),
    state_types.LocaleRecord(iso_code: "ga", name: "Irish"),
    state_types.LocaleRecord(iso_code: "it", name: "Italian"),
    state_types.LocaleRecord(iso_code: "ja", name: "Japanese"),
    state_types.LocaleRecord(iso_code: "jv", name: "Javanese"),
    state_types.LocaleRecord(iso_code: "kl", name: "Kalaallisut"),
    state_types.LocaleRecord(iso_code: "kn", name: "Kannada"),
    state_types.LocaleRecord(iso_code: "ks", name: "Kashmiri"),
    state_types.LocaleRecord(iso_code: "kk", name: "Kazakh"),
    state_types.LocaleRecord(iso_code: "km", name: "Khmer"),
    state_types.LocaleRecord(iso_code: "ki", name: "Kikuyu"),
    state_types.LocaleRecord(iso_code: "rw", name: "Kinyarwanda"),
    state_types.LocaleRecord(iso_code: "ko", name: "Korean"),
    state_types.LocaleRecord(iso_code: "ku", name: "Kurdish"),
    state_types.LocaleRecord(iso_code: "ky", name: "Kyrgyz"),
    state_types.LocaleRecord(iso_code: "lo", name: "Lao"),
    state_types.LocaleRecord(iso_code: "lv", name: "Latvian"),
    state_types.LocaleRecord(iso_code: "ln", name: "Lingala"),
    state_types.LocaleRecord(iso_code: "lt", name: "Lithuanian"),
    state_types.LocaleRecord(iso_code: "lu", name: "Luba-Katanga"),
    state_types.LocaleRecord(iso_code: "lb", name: "Luxembourgish"),
    state_types.LocaleRecord(iso_code: "mk", name: "Macedonian"),
    state_types.LocaleRecord(iso_code: "mg", name: "Malagasy"),
    state_types.LocaleRecord(iso_code: "ms", name: "Malay"),
    state_types.LocaleRecord(iso_code: "ml", name: "Malayalam"),
    state_types.LocaleRecord(iso_code: "mt", name: "Maltese"),
    state_types.LocaleRecord(iso_code: "gv", name: "Manx"),
    state_types.LocaleRecord(iso_code: "mr", name: "Marathi"),
    state_types.LocaleRecord(iso_code: "mn", name: "Mongolian"),
    state_types.LocaleRecord(iso_code: "mi", name: "Māori"),
    state_types.LocaleRecord(iso_code: "ne", name: "Nepali"),
    state_types.LocaleRecord(iso_code: "nd", name: "North Ndebele"),
    state_types.LocaleRecord(iso_code: "se", name: "Northern Sami"),
    state_types.LocaleRecord(iso_code: "no", name: "Norwegian"),
    state_types.LocaleRecord(iso_code: "nb", name: "Norwegian (Bokmål)"),
    state_types.LocaleRecord(iso_code: "nn", name: "Norwegian Nynorsk"),
    state_types.LocaleRecord(iso_code: "or", name: "Odia"),
    state_types.LocaleRecord(iso_code: "om", name: "Oromo"),
    state_types.LocaleRecord(iso_code: "os", name: "Ossetic"),
    state_types.LocaleRecord(iso_code: "ps", name: "Pashto"),
    state_types.LocaleRecord(iso_code: "fa", name: "Persian"),
    state_types.LocaleRecord(iso_code: "pl", name: "Polish"),
    state_types.LocaleRecord(iso_code: "pt-BR", name: "Portuguese (Brazil)"),
    state_types.LocaleRecord(iso_code: "pt-PT", name: "Portuguese (Portugal)"),
    state_types.LocaleRecord(iso_code: "pa", name: "Punjabi"),
    state_types.LocaleRecord(iso_code: "qu", name: "Quechua"),
    state_types.LocaleRecord(iso_code: "ro", name: "Romanian"),
    state_types.LocaleRecord(iso_code: "rm", name: "Romansh"),
    state_types.LocaleRecord(iso_code: "rn", name: "Rundi"),
    state_types.LocaleRecord(iso_code: "ru", name: "Russian"),
    state_types.LocaleRecord(iso_code: "sg", name: "Sango"),
    state_types.LocaleRecord(iso_code: "sa", name: "Sanskrit"),
    state_types.LocaleRecord(iso_code: "sc", name: "Sardinian"),
    state_types.LocaleRecord(iso_code: "gd", name: "Scottish Gaelic"),
    state_types.LocaleRecord(iso_code: "sr", name: "Serbian"),
    state_types.LocaleRecord(iso_code: "sn", name: "Shona"),
    state_types.LocaleRecord(iso_code: "ii", name: "Sichuan Yi"),
    state_types.LocaleRecord(iso_code: "sd", name: "Sindhi"),
    state_types.LocaleRecord(iso_code: "si", name: "Sinhala"),
    state_types.LocaleRecord(iso_code: "sk", name: "Slovak"),
    state_types.LocaleRecord(iso_code: "sl", name: "Slovenian"),
    state_types.LocaleRecord(iso_code: "so", name: "Somali"),
    state_types.LocaleRecord(iso_code: "es", name: "Spanish"),
    state_types.LocaleRecord(iso_code: "su", name: "Sundanese"),
    state_types.LocaleRecord(iso_code: "sw", name: "Swahili"),
    state_types.LocaleRecord(iso_code: "sv", name: "Swedish"),
    state_types.LocaleRecord(iso_code: "tg", name: "Tajik"),
    state_types.LocaleRecord(iso_code: "ta", name: "Tamil"),
    state_types.LocaleRecord(iso_code: "tt", name: "Tatar"),
    state_types.LocaleRecord(iso_code: "te", name: "Telugu"),
    state_types.LocaleRecord(iso_code: "th", name: "Thai"),
    state_types.LocaleRecord(iso_code: "bo", name: "Tibetan"),
    state_types.LocaleRecord(iso_code: "ti", name: "Tigrinya"),
    state_types.LocaleRecord(iso_code: "to", name: "Tongan"),
    state_types.LocaleRecord(iso_code: "tr", name: "Turkish"),
    state_types.LocaleRecord(iso_code: "tk", name: "Turkmen"),
    state_types.LocaleRecord(iso_code: "uk", name: "Ukrainian"),
    state_types.LocaleRecord(iso_code: "ur", name: "Urdu"),
    state_types.LocaleRecord(iso_code: "ug", name: "Uyghur"),
    state_types.LocaleRecord(iso_code: "uz", name: "Uzbek"),
    state_types.LocaleRecord(iso_code: "vi", name: "Vietnamese"),
    state_types.LocaleRecord(iso_code: "cy", name: "Welsh"),
    state_types.LocaleRecord(iso_code: "fy", name: "Western Frisian"),
    state_types.LocaleRecord(iso_code: "wo", name: "Wolof"),
    state_types.LocaleRecord(iso_code: "xh", name: "Xhosa"),
    state_types.LocaleRecord(iso_code: "yi", name: "Yiddish"),
    state_types.LocaleRecord(iso_code: "yo", name: "Yoruba"),
    state_types.LocaleRecord(iso_code: "zu", name: "Zulu"),
  ]
}

@internal
pub fn available_locales(store_in: Store) -> List(state_types.LocaleRecord) {
  case store.list_effective_available_locales(store_in) {
    [] -> default_available_locales()
    stored -> stored
  }
}

@internal
pub fn locale_name(store_in: Store, locale: String) -> Option(String) {
  available_locales(store_in)
  |> list.find(fn(candidate) { candidate.iso_code == locale })
  |> option.from_result
  |> option.map(fn(record) { record.name })
}

fn default_shop_locales(store_in: Store) -> List(state_types.ShopLocaleRecord) {
  let name = case locale_name(store_in, "en") {
    Some(value) -> value
    None -> "English"
  }
  [
    state_types.ShopLocaleRecord(
      locale: "en",
      name: name,
      primary: True,
      published: True,
      market_web_presence_ids: [],
    ),
  ]
}

@internal
pub fn list_shop_locales(
  store_in: Store,
  published: Option(Bool),
) -> List(state_types.ShopLocaleRecord) {
  case store.list_effective_shop_locales(store_in, published) {
    [] ->
      default_shop_locales(store_in)
      |> list.filter(fn(locale) {
        case published {
          Some(target) -> locale.published == target
          None -> True
        }
      })
    stored -> stored
  }
}

@internal
pub fn get_shop_locale(
  store_in: Store,
  locale: String,
) -> Option(state_types.ShopLocaleRecord) {
  store.get_effective_shop_locale(store_in, locale)
  |> option.lazy_or(fn() {
    default_shop_locales(store_in)
    |> list.find(fn(candidate) { candidate.locale == locale })
    |> option.from_result
  })
}

@internal
pub fn primary_locale_for(store_in: Store) -> String {
  list_shop_locales(store_in, None)
  |> list.find(fn(locale) { locale.primary })
  |> option.from_result
  |> option.map(fn(locale) { locale.locale })
  |> option.unwrap("en")
}

// ---------------------------------------------------------------------------
// Resource reconstruction
// ---------------------------------------------------------------------------

/// Find a translatable resource by id from effective Product and
/// product Metafield state, mirroring the TypeScript localization
/// runtime.
fn find_resource(
  store_in: Store,
  resource_id: String,
) -> Option(TranslatableResource) {
  case store.get_effective_product_by_id(store_in, resource_id) {
    Some(product) ->
      Some(TranslatableResource(
        resource_id: product.id,
        resource_type: "PRODUCT",
        content: product_content(product),
      ))
    None ->
      case
        list.find(list_product_metafields(store_in), fn(metafield) {
          metafield.id == resource_id
        })
      {
        Ok(metafield) ->
          Some(TranslatableResource(
            resource_id: metafield.id,
            resource_type: "METAFIELD",
            content: metafield_content(metafield),
          ))
        Error(_) -> None
      }
  }
}

/// Enumerate every translatable resource of a given type from effective
/// Product/Product Metafield state plus capture-backed source markers.
fn list_resources(
  store_in: Store,
  resource_type: Option(String),
) -> List(TranslatableResource) {
  let product_resources = case resource_matches(resource_type, "PRODUCT") {
    True ->
      store.list_effective_products(store_in)
      |> list.map(fn(product) {
        TranslatableResource(
          resource_id: product.id,
          resource_type: "PRODUCT",
          content: product_content(product),
        )
      })
    False -> []
  }
  let metafield_resources = case resource_matches(resource_type, "METAFIELD") {
    True ->
      list_product_metafields(store_in)
      |> list.map(fn(metafield) {
        TranslatableResource(
          resource_id: metafield.id,
          resource_type: "METAFIELD",
          content: metafield_content(metafield),
        )
      })
    False -> []
  }
  let seeded_resources =
    source_marker_resources(store_in)
    |> list.filter(fn(resource) {
      resource_matches(resource_type, resource.resource_type)
    })
  list.append(product_resources, metafield_resources)
  |> list.append(seeded_resources)
  |> dedupe_resources([])
  |> list.sort(fn(left, right) {
    string.compare(left.resource_id, right.resource_id)
  })
}

fn resource_matches(filter: Option(String), resource_type: String) -> Bool {
  case filter {
    Some(target) -> target == resource_type
    None -> False
  }
}

fn list_product_metafields(
  store_in: Store,
) -> List(state_types.ProductMetafieldRecord) {
  store.list_effective_products(store_in)
  |> list.flat_map(fn(product) {
    store.get_effective_metafields_by_owner_id(store_in, product.id)
  })
  |> dedupe_metafields([])
  |> list.sort(fn(left, right) { string.compare(left.id, right.id) })
}

fn dedupe_metafields(
  metafields: List(state_types.ProductMetafieldRecord),
  seen: List(String),
) -> List(state_types.ProductMetafieldRecord) {
  case metafields {
    [] -> []
    [metafield, ..rest] ->
      case list.contains(seen, metafield.id) {
        True -> dedupe_metafields(rest, seen)
        False -> [metafield, ..dedupe_metafields(rest, [metafield.id, ..seen])]
      }
  }
}

fn product_content(
  product: state_types.ProductRecord,
) -> List(TranslatableContent) {
  let base = [
    TranslatableContent(
      key: "title",
      value: Some(product.title),
      digest: Some(crypto.sha256_hex(product.title)),
      locale: "en",
      type_: "SINGLE_LINE_TEXT_FIELD",
    ),
    TranslatableContent(
      key: "handle",
      value: Some(product.handle),
      digest: Some(crypto.sha256_hex(product.handle)),
      locale: "en",
      type_: "URI",
    ),
  ]
  let with_body = case product.description_html {
    "" -> base
    value ->
      list.append(base, [
        TranslatableContent(
          key: "body_html",
          value: Some(value),
          digest: Some(crypto.sha256_hex(value)),
          locale: "en",
          type_: "HTML",
        ),
      ])
  }
  let with_type = case product.product_type {
    Some(value) ->
      list.append(with_body, [
        TranslatableContent(
          key: "product_type",
          value: Some(value),
          digest: Some(crypto.sha256_hex(value)),
          locale: "en",
          type_: "SINGLE_LINE_TEXT_FIELD",
        ),
      ])
    None -> with_body
  }
  let with_seo_title = case product.seo.title {
    Some(value) ->
      list.append(with_type, [
        TranslatableContent(
          key: "meta_title",
          value: Some(value),
          digest: Some(crypto.sha256_hex(value)),
          locale: "en",
          type_: "SINGLE_LINE_TEXT_FIELD",
        ),
      ])
    None -> with_type
  }
  case product.seo.description {
    Some(value) ->
      list.append(with_seo_title, [
        TranslatableContent(
          key: "meta_description",
          value: Some(value),
          digest: Some(crypto.sha256_hex(value)),
          locale: "en",
          type_: "MULTI_LINE_TEXT_FIELD",
        ),
      ])
    None -> with_seo_title
  }
}

fn metafield_content(
  metafield: state_types.ProductMetafieldRecord,
) -> List(TranslatableContent) {
  [
    TranslatableContent(
      key: "value",
      value: metafield.value,
      digest: case metafield.compare_digest, metafield.value {
        Some(digest), _ -> Some(digest)
        None, Some(value) -> Some(crypto.sha256_hex(value))
        None, None -> None
      },
      locale: "en",
      type_: localizable_content_type_for_metafield(metafield.type_),
    ),
  ]
}

fn localizable_content_type_for_metafield(type_: Option(String)) -> String {
  case type_ {
    Some("multi_line_text_field") -> "MULTI_LINE_TEXT_FIELD"
    Some("rich_text_field") -> "RICH_TEXT_FIELD"
    Some("url") -> "URL"
    Some("json") -> "JSON"
    _ -> "SINGLE_LINE_TEXT_FIELD"
  }
}

fn source_marker_resources(store_in: Store) -> List(TranslatableResource) {
  let source_markers =
    list.append(
      dict.values(store_in.base_state.translations),
      dict.values(store_in.staged_state.translations),
    )
    |> list.filter(fn(t) { t.locale == "__source" })
  let resource_ids =
    source_markers
    |> list.map(fn(t) { t.resource_id })
    |> dedupe_strings([])
  list.filter_map(resource_ids, fn(resource_id) {
    let content =
      source_markers
      |> list.filter(fn(t) { t.resource_id == resource_id })
      |> list.map(fn(t) {
        content_from_translation(t, t.translatable_content_digest)
      })
      |> dedupe_content_by_key([])
      |> list.sort(compare_content)
    case content {
      [] -> Error(Nil)
      _ ->
        Ok(TranslatableResource(
          resource_id: resource_id,
          resource_type: synthetic_resource_type(resource_id),
          content: content,
        ))
    }
  })
}

fn dedupe_strings(values: List(String), seen: List(String)) -> List(String) {
  case values {
    [] -> []
    [value, ..rest] ->
      case list.contains(seen, value) {
        True -> dedupe_strings(rest, seen)
        False -> [value, ..dedupe_strings(rest, [value, ..seen])]
      }
  }
}

fn dedupe_resources(
  resources: List(TranslatableResource),
  seen: List(String),
) -> List(TranslatableResource) {
  case resources {
    [] -> []
    [resource, ..rest] ->
      case list.contains(seen, resource.resource_id) {
        True -> dedupe_resources(rest, seen)
        False -> [
          resource,
          ..dedupe_resources(rest, [resource.resource_id, ..seen])
        ]
      }
  }
}

// ---------------------------------------------------------------------------
// Read-path serialization
// ---------------------------------------------------------------------------

@internal
pub fn serialize_root_fields(
  store_in: Store,
  fields: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let entries =
    list.map(fields, fn(field) {
      let key = get_field_response_key(field)
      let payload =
        root_payload_for_field(store_in, field, fragments, variables)
      #(key, payload)
    })
  json.object(entries)
}

fn root_payload_for_field(
  store_in: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  case field {
    Field(name: name, ..) ->
      case name.value {
        "availableLocales" -> serialize_available_locales(store_in, field)
        "shopLocales" ->
          serialize_shop_locales_root(store_in, field, fragments, variables)
        "translatableResource" ->
          serialize_translatable_resource_root(
            store_in,
            field,
            fragments,
            variables,
          )
        "translatableResources" ->
          serialize_translatable_resources_root(
            store_in,
            field,
            fragments,
            variables,
          )
        "translatableResourcesByIds" ->
          serialize_translatable_resources_by_ids_root(
            store_in,
            field,
            fragments,
            variables,
          )
        _ -> json.null()
      }
    _ -> json.null()
  }
}

fn selections_of(field: Selection) -> List(Selection) {
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      selections
    _ -> []
  }
}

// availableLocales
fn serialize_available_locales(store_in: Store, field: Selection) -> Json {
  let locales = available_locales(store_in)
  json.array(locales, fn(locale) { serialize_locale(locale, field) })
}

fn serialize_locale(
  locale: state_types.LocaleRecord,
  field: Selection,
) -> Json {
  let entries =
    list.map(selections_of(field), fn(selection) {
      let key = get_field_response_key(selection)
      case selection {
        Field(name: name, ..) ->
          case name.value {
            "isoCode" -> #(key, json.string(locale.iso_code))
            "name" -> #(key, json.string(locale.name))
            "__typename" -> #(key, json.string("Locale"))
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

// shopLocales
fn serialize_shop_locales_root(
  store_in: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  let published = graphql_helpers.read_arg_bool(args, "published")
  let locales = list_shop_locales(store_in, published)
  json.array(locales, fn(locale) {
    serialize_shop_locale(store_in, locale, field, fragments)
  })
}

fn serialize_shop_locale(
  store_in: Store,
  locale: state_types.ShopLocaleRecord,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let entries =
    list.map(selections_of(field), fn(selection) {
      let key = get_field_response_key(selection)
      case selection {
        Field(name: name, ..) ->
          case name.value {
            "locale" -> #(key, json.string(locale.locale))
            "name" -> #(key, json.string(locale.name))
            "primary" -> #(key, json.bool(locale.primary))
            "published" -> #(key, json.bool(locale.published))
            "__typename" -> #(key, json.string("ShopLocale"))
            "marketWebPresences" -> #(
              key,
              serialize_market_web_presences(
                store_in,
                locale,
                selection,
                fragments,
              ),
            )
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

fn serialize_market_web_presences(
  store_in: Store,
  locale: state_types.ShopLocaleRecord,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let sources =
    list.map(locale.market_web_presence_ids, fn(id) {
      market_web_presence_source(store_in, id)
    })
  project_graphql_value(SrcList(sources), selections_of(field), fragments)
}

fn market_web_presence_source(store_in: Store, id: String) {
  case store.get_effective_web_presence_by_id(store_in, id) {
    Some(record) -> captured_json_source(record.data)
    None ->
      src_object([
        #("id", SrcString(id)),
        #("__typename", SrcString("MarketWebPresence")),
        #(
          "defaultLocale",
          shop_locale_reference_source(store_in, primary_locale_for(store_in)),
        ),
      ])
  }
}

fn shop_locale_reference_source(store_in: Store, locale: String) {
  src_object([
    #("locale", SrcString(locale)),
    #("name", SrcString(locale_name(store_in, locale) |> option.unwrap(locale))),
    #("primary", SrcBool(locale == primary_locale_for(store_in))),
    #("published", SrcBool(True)),
  ])
}

// translatableResource
fn serialize_translatable_resource_root(
  store_in: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string(args, "resourceId") {
    Some(resource_id) -> {
      let resource = find_resource_or_synthesize(store_in, resource_id)
      serialize_resource(store_in, resource, field, fragments, variables)
    }
    None -> json.null()
  }
}

/// Synthesize a translatable resource from in-store translation/source
/// markers even when the underlying Product/Metafield record isn't
/// available. Parity seeding can provide a captured source digest as a
/// non-target-locale marker; ordinary unknown ids still return `None`.
fn find_resource_or_synthesize(
  store_in: Store,
  resource_id: String,
) -> Option(TranslatableResource) {
  find_resource(store_in, resource_id)
  |> option.lazy_or(fn() {
    let content = synthesized_content_from_translations(store_in, resource_id)
    case list.is_empty(content) {
      False ->
        Some(TranslatableResource(
          resource_id: resource_id,
          resource_type: synthetic_resource_type(resource_id),
          content: content,
        ))
      True -> None
    }
  })
}

fn synthesized_content_from_translations(
  store_in: Store,
  resource_id: String,
) -> List(TranslatableContent) {
  let translations =
    list.append(
      dict.values(store_in.base_state.translations),
      dict.values(store_in.staged_state.translations),
    )
    |> list.filter(fn(t) { t.resource_id == resource_id })
  translations
  |> list.map(fn(translation) {
    content_from_translation(
      translation,
      translation.translatable_content_digest,
    )
  })
  |> dedupe_content_by_key([])
  |> list.sort(compare_content)
}

fn compare_content(
  left: TranslatableContent,
  right: TranslatableContent,
) -> order.Order {
  case int.compare(content_order(left.key), content_order(right.key)) {
    order.Eq -> string.compare(left.key, right.key)
    other -> other
  }
}

fn content_order(key: String) -> Int {
  case key {
    "title" -> 0
    "handle" -> 1
    "body_html" -> 2
    "product_type" -> 3
    "meta_title" -> 4
    "meta_description" -> 5
    "value" -> 6
    _ -> 100
  }
}

fn dedupe_content_by_key(
  content: List(TranslatableContent),
  seen: List(String),
) -> List(TranslatableContent) {
  case content {
    [] -> []
    [entry, ..rest] ->
      case list.contains(seen, entry.key) {
        True -> dedupe_content_by_key(rest, seen)
        False -> [entry, ..dedupe_content_by_key(rest, [entry.key, ..seen])]
      }
  }
}

fn content_from_translation(
  translation: state_types.TranslationRecord,
  digest: String,
) -> TranslatableContent {
  let source_value = case translation.locale, translation.value {
    "__source", "" -> None
    "__source", value -> Some(value)
    _, _ -> None
  }
  TranslatableContent(
    key: translation.key,
    value: source_value,
    digest: case digest {
      "" -> None
      value -> Some(value)
    },
    locale: "en",
    type_: content_type_for_key(translation.key),
  )
}

fn content_type_for_key(key: String) -> String {
  case key {
    "body_html" -> "HTML"
    "handle" -> "URI"
    "meta_description" -> "MULTI_LINE_TEXT_FIELD"
    _ -> "SINGLE_LINE_TEXT_FIELD"
  }
}

@internal
pub fn resource_exists_for_validation(
  store_in: Store,
  resource_id: String,
) -> Option(TranslatableResource) {
  case find_resource(store_in, resource_id) {
    Some(resource) -> Some(resource)
    None -> find_resource_or_synthesize(store_in, resource_id)
  }
}

fn synthetic_resource_type(resource_id: String) -> String {
  case string.starts_with(resource_id, "gid://shopify/Metafield/") {
    True -> "METAFIELD"
    False -> "PRODUCT"
  }
}

fn serialize_resource(
  store_in: Store,
  resource: Option(TranslatableResource),
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  case resource {
    None -> json.null()
    Some(record) -> {
      let entries =
        list.map(selections_of(field), fn(selection) {
          let key = get_field_response_key(selection)
          case selection {
            Field(name: name, ..) ->
              case name.value {
                "resourceId" -> #(key, json.string(record.resource_id))
                "__typename" -> #(key, json.string("TranslatableResource"))
                "translatableContent" -> #(
                  key,
                  serialize_content(record.content, selection),
                )
                "translations" -> #(
                  key,
                  serialize_translations_for_resource(
                    store_in,
                    record,
                    selection,
                    fragments,
                    variables,
                  ),
                )
                "nestedTranslatableResources" -> #(
                  key,
                  serialize_empty_connection(
                    selection,
                    default_selected_field_options(),
                  ),
                )
                _ -> #(key, json.null())
              }
            _ -> #(key, json.null())
          }
        })
      json.object(entries)
    }
  }
}

fn serialize_content(
  content: List(TranslatableContent),
  field: Selection,
) -> Json {
  let inner = selections_of(field)
  json.array(content, fn(entry) {
    let entries =
      list.map(inner, fn(selection) {
        let key = get_field_response_key(selection)
        case selection {
          Field(name: name, ..) ->
            case name.value {
              "key" -> #(key, json.string(entry.key))
              "value" -> #(key, optional_string_json(entry.value))
              "digest" -> #(key, optional_string_json(entry.digest))
              "locale" -> #(key, json.string(entry.locale))
              "type" -> #(key, json.string(entry.type_))
              "__typename" -> #(key, json.string("TranslatableContent"))
              _ -> #(key, json.null())
            }
          _ -> #(key, json.null())
        }
      })
    json.object(entries)
  })
}

fn serialize_translations_for_resource(
  store_in: Store,
  resource: TranslatableResource,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  let locale = graphql_helpers.read_arg_string(args, "locale")
  let market_id = graphql_helpers.read_arg_string(args, "marketId")
  let outdated = graphql_helpers.read_arg_bool(args, "outdated")
  let translations = case locale {
    Some(loc) ->
      store.list_effective_translations(
        store_in,
        resource.resource_id,
        loc,
        market_id,
      )
    None -> []
  }
  let filtered = case outdated {
    Some(target) -> list.filter(translations, fn(t) { t.outdated == target })
    None -> translations
  }
  json.array(filtered, fn(t) {
    serialize_translation(store_in, t, field, fragments)
  })
}

@internal
pub fn serialize_translation(
  store_in: Store,
  translation: state_types.TranslationRecord,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let entries =
    list.map(selections_of(field), fn(selection) {
      let key = get_field_response_key(selection)
      case selection {
        Field(name: name, ..) ->
          case name.value {
            "key" -> #(key, json.string(translation.key))
            "value" -> #(key, json.string(translation.value))
            "locale" -> #(key, json.string(translation.locale))
            "outdated" -> #(key, json.bool(translation.outdated))
            "updatedAt" -> #(key, json.string(translation.updated_at))
            "__typename" -> #(key, json.string("Translation"))
            "market" -> #(
              key,
              serialize_translation_market(
                store_in,
                translation.market_id,
                selection,
                fragments,
              ),
            )
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

fn serialize_translation_market(
  store_in: Store,
  market_id: Option(String),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  case market_id {
    None -> json.null()
    Some(id) ->
      project_graphql_value(
        market_source(store_in, id),
        selections_of(field),
        fragments,
      )
  }
}

fn market_source(store_in: Store, id: String) {
  case store.get_effective_market_by_id(store_in, id) {
    Some(record) -> captured_json_source(record.data)
    None ->
      src_object([#("id", SrcString(id)), #("__typename", SrcString("Market"))])
  }
}

fn serialize_translatable_resources_root(
  store_in: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  let resource_type = graphql_helpers.read_arg_string(args, "resourceType")
  let reverse = graphql_helpers.read_arg_bool(args, "reverse")
  let resources = list_resources(store_in, resource_type)
  let resources = case reverse {
    Some(True) -> list.reverse(resources)
    _ -> resources
  }
  serialize_resource_connection(
    store_in,
    resources,
    field,
    fragments,
    variables,
  )
}

fn serialize_translatable_resources_by_ids_root(
  store_in: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  let ids = case read_optional_string_array(args, "resourceIds") {
    Some(ids) -> ids
    None -> []
  }
  let resources =
    list.filter_map(ids, fn(id) {
      case find_resource_or_synthesize(store_in, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  let reverse = graphql_helpers.read_arg_bool(args, "reverse")
  let resources = case reverse {
    Some(True) -> list.reverse(resources)
    _ -> resources
  }
  serialize_resource_connection(
    store_in,
    resources,
    field,
    fragments,
    variables,
  )
}

fn serialize_resource_connection(
  store_in: Store,
  resources: List(TranslatableResource),
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let cursor_value = fn(resource: TranslatableResource, _index: Int) {
    resource.resource_id
  }
  let window =
    paginate_connection_items(
      resources,
      field,
      variables,
      cursor_value,
      default_connection_window_options(),
    )
  let ConnectionWindow(
    items: paged,
    has_next_page: has_next,
    has_previous_page: has_prev,
  ) = window
  let selected_field_options =
    SelectedFieldOptions(include_inline_fragments: True)
  let page_info_options =
    ConnectionPageInfoOptions(
      ..default_connection_page_info_options(),
      include_inline_fragments: True,
    )
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: paged,
      has_next_page: has_next,
      has_previous_page: has_prev,
      get_cursor_value: cursor_value,
      serialize_node: fn(resource, node_field, _index) {
        serialize_resource(
          store_in,
          Some(resource),
          node_field,
          fragments,
          variables,
        )
      },
      selected_field_options: selected_field_options,
      page_info_options: page_info_options,
    ),
  )
}

// ---------------------------------------------------------------------------

@internal
pub fn shop_locale_does_not_exist_error() -> AnyUserError {
  ShopLocaleError(
    field: ["locale"],
    message: "The locale doesn't exist.",
    code: "SHOP_LOCALE_DOES_NOT_EXIST",
  )
}

@internal
pub fn invalid_locale_error() -> AnyUserError {
  ShopLocaleError(
    field: ["locale"],
    message: "Locale is invalid",
    code: "INVALID",
  )
}

@internal
pub fn shop_locale_taken_error() -> AnyUserError {
  ShopLocaleError(
    field: ["locale"],
    message: "Locale has already been taken",
    code: "TAKEN",
  )
}

@internal
pub fn shop_locale_limit_reached_error(locale_name: String) -> AnyUserError {
  ShopLocaleError(
    field: ["base"],
    message: "Your store has reached its 20 language limit. To add "
      <> locale_name
      <> ", delete one of your other languages.",
    code: "LIMIT_REACHED",
  )
}

@internal
pub fn primary_locale_error() -> AnyUserError {
  ShopLocaleError(
    field: ["locale"],
    message: "The primary locale of your store can't be changed through this endpoint.",
    code: "CAN_NOT_MUTATE_PRIMARY_LOCALE",
  )
}

@internal
pub fn shop_locale_staged_id(record: state_types.ShopLocaleRecord) -> String {
  "ShopLocale/" <> record.locale
}

@internal
pub fn read_input_object(
  args: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Dict(String, root_field.ResolvedValue) {
  case dict.get(args, key) {
    Ok(root_field.ObjectVal(d)) -> d
    _ -> dict.new()
  }
}

@internal
pub fn project_shop_locale_payload(
  store_in: Store,
  field: Selection,
  shop_locale: Option(state_types.ShopLocaleRecord),
  locale: Option(String),
  errors: List(AnyUserError),
  _typename: String,
  fragments: FragmentMap,
) -> Json {
  let entries =
    list.map(selections_of(field), fn(selection) {
      let key = get_field_response_key(selection)
      case selection {
        Field(name: name, ..) ->
          case name.value {
            "shopLocale" -> #(key, case shop_locale {
              Some(record) ->
                serialize_shop_locale(store_in, record, selection, fragments)
              None -> json.null()
            })
            "locale" -> #(key, case locale {
              Some(s) -> json.string(s)
              None -> json.null()
            })
            "userErrors" -> #(key, serialize_user_errors(errors, selection))
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

fn serialize_user_errors(
  errors: List(AnyUserError),
  selection: Selection,
) -> Json {
  let inner = selections_of(selection)
  json.array(errors, fn(error) {
    let entries =
      list.map(inner, fn(child) {
        let key = get_field_response_key(child)
        case child {
          Field(name: name, ..) ->
            case name.value, error {
              "field", TranslationError(field: parts, ..) -> #(
                key,
                json.array(parts, json.string),
              )
              "field", ShopLocaleError(field: parts, ..) -> #(
                key,
                json.array(parts, json.string),
              )
              "message", TranslationError(message: msg, ..) -> #(
                key,
                json.string(msg),
              )
              "message", ShopLocaleError(message: msg, ..) -> #(
                key,
                json.string(msg),
              )
              "code", TranslationError(code: code, ..) -> #(
                key,
                json.string(code),
              )
              "code", ShopLocaleError(code: code, ..) -> #(
                key,
                json.string(code),
              )
              _, _ -> #(key, json.null())
            }
          _ -> #(key, json.null())
        }
      })
    json.object(entries)
  })
}

@internal
pub fn project_translations_payload(
  store_in: Store,
  translations: Option(List(state_types.TranslationRecord)),
  errors: List(AnyUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let entries =
    list.map(selections_of(field), fn(selection) {
      let key = get_field_response_key(selection)
      case selection {
        Field(name: name, ..) ->
          case name.value {
            "translations" -> #(key, case translations {
              Some(records) ->
                json.array(records, fn(t) {
                  serialize_translation(store_in, t, selection, fragments)
                })
              None -> json.null()
            })
            "userErrors" -> #(key, serialize_user_errors(errors, selection))
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

fn captured_json_source(value: state_types.CapturedJsonValue) {
  case value {
    state_types.CapturedNull -> SrcNull
    state_types.CapturedBool(value) -> SrcBool(value)
    state_types.CapturedInt(value) -> SrcInt(value)
    state_types.CapturedFloat(value) -> SrcFloat(value)
    state_types.CapturedString(value) -> SrcString(value)
    state_types.CapturedArray(items) ->
      SrcList(list.map(items, captured_json_source))
    state_types.CapturedObject(fields) ->
      SrcObject(
        fields
        |> list.map(fn(pair) {
          let #(key, item) = pair
          #(key, captured_json_source(item))
        })
        |> dict.from_list,
      )
  }
}

fn optional_string_json(value: Option(String)) -> Json {
  case value {
    Some(s) -> json.string(s)
    None -> json.null()
  }
}

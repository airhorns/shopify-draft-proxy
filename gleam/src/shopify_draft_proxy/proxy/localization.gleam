//// Mirrors the localization slice of `src/proxy/localization.ts`.
////
//// Pass 23 ships:
//// - `availableLocales` and `shopLocales` reads, fully fed by the store
////   plus a hardcoded default catalog when the store hasn't been
////   hydrated.
//// - `translatableResource(s)` / `translatableResourcesByIds` reads.
////   Without a Products domain in the Gleam port the only resources
////   that can be discovered are translation entries already staged in
////   the store — the resource is reconstructed from those translations
////   so a register-then-read round-trip works end-to-end.
//// - `shopLocaleEnable/Update/Disable` mutations, including the
////   "disabling clears translations for that locale" cleanup.
//// - `translationsRegister/Remove` mutations with the same validation
////   structure as the TS handler. With no Products in the store,
////   `validateResource` always returns `RESOURCE_NOT_FOUND` for unknown
////   gids — matching the captured `unknown-resource-validation` parity
////   target. The success path that registers translations against a
////   real Product is deferred until the Products domain ports.

import gleam/dict.{type Dict}
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection, Field, SelectionSet}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, ConnectionPageInfoOptions, ConnectionWindow,
  SelectedFieldOptions, SerializeConnectionConfig,
  default_connection_page_info_options, default_connection_window_options,
  default_selected_field_options, get_document_fragments, get_field_response_key,
  paginate_connection_items, serialize_connection, serialize_empty_connection,
}
import shopify_draft_proxy/proxy/mutation_helpers.{read_optional_string_array}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type LocaleRecord, type ShopLocaleRecord, type TranslationRecord, LocaleRecord,
  ShopLocaleRecord, TranslationRecord,
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

pub type LocalizationError {
  ParseFailed(root_field.RootFieldError)
}

pub type MutationOutcome {
  MutationOutcome(
    data: Json,
    store: Store,
    identity: SyntheticIdentityRegistry,
    staged_resource_ids: List(String),
  )
}

/// Validation user-error variant. Translation register/remove emits
/// errors with `code`; shopLocale lifecycle emits errors without one.
type AnyUserError {
  TranslationError(field: List(String), message: String, code: String)
  ShopLocaleError(field: List(String), message: String)
}

/// One translatable content slot on a translatable resource. `digest`
/// is `None` for resources reconstructed from the store (no source
/// product available), which short-circuits the digest comparison
/// during validation. The constructor is currently unbuilt because the
/// Products domain hasn't ported — `find_resource` is always `None`,
/// so the type stays dormant until then. Marked `@internal` to silence
/// the unused-constructor warning while leaving the shape ready for
/// the Products pass.
@internal
pub type TranslatableContent {
  TranslatableContent(
    key: String,
    value: Option(String),
    digest: Option(String),
    locale: String,
    type_: String,
  )
}

@internal
pub type TranslatableResource {
  TranslatableResource(
    resource_id: String,
    resource_type: String,
    content: List(TranslatableContent),
  )
}

// ---------------------------------------------------------------------------
// Public surface
// ---------------------------------------------------------------------------

pub fn is_localization_query_root(name: String) -> Bool {
  case name {
    "availableLocales" -> True
    "shopLocales" -> True
    "translatableResource" -> True
    "translatableResources" -> True
    "translatableResourcesByIds" -> True
    _ -> False
  }
}

pub fn is_localization_mutation_root(name: String) -> Bool {
  case name {
    "shopLocaleEnable" -> True
    "shopLocaleUpdate" -> True
    "shopLocaleDisable" -> True
    "translationsRegister" -> True
    "translationsRemove" -> True
    _ -> False
  }
}

pub fn handle_localization_query(
  store_in: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, LocalizationError) {
  case root_field.get_root_fields(document) {
    Error(err) -> Error(ParseFailed(err))
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      Ok(serialize_root_fields(store_in, fields, fragments, variables))
    }
  }
}

pub fn wrap_data(data: Json) -> Json {
  json.object([#("data", data)])
}

pub fn process(
  store_in: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, LocalizationError) {
  use data <- result.try(handle_localization_query(
    store_in,
    document,
    variables,
  ))
  Ok(wrap_data(data))
}

pub fn process_mutation(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(MutationOutcome, LocalizationError) {
  case root_field.get_root_fields(document) {
    Error(err) -> Error(ParseFailed(err))
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      Ok(handle_mutation_fields(
        store_in,
        identity,
        fields,
        fragments,
        variables,
      ))
    }
  }
}

// ---------------------------------------------------------------------------
// Default catalog
// ---------------------------------------------------------------------------

fn default_available_locales() -> List(LocaleRecord) {
  [
    LocaleRecord(iso_code: "en", name: "English"),
    LocaleRecord(iso_code: "fr", name: "French"),
    LocaleRecord(iso_code: "de", name: "German"),
    LocaleRecord(iso_code: "es", name: "Spanish"),
    LocaleRecord(iso_code: "it", name: "Italian"),
    LocaleRecord(iso_code: "pt-BR", name: "Portuguese (Brazil)"),
    LocaleRecord(iso_code: "ja", name: "Japanese"),
    LocaleRecord(iso_code: "zh-CN", name: "Chinese (Simplified)"),
  ]
}

fn available_locales(store_in: Store) -> List(LocaleRecord) {
  case store.list_effective_available_locales(store_in) {
    [] -> default_available_locales()
    stored -> stored
  }
}

fn locale_name(store_in: Store, locale: String) -> Option(String) {
  available_locales(store_in)
  |> list.find(fn(candidate) { candidate.iso_code == locale })
  |> option.from_result
  |> option.map(fn(record) { record.name })
}

fn default_shop_locales(store_in: Store) -> List(ShopLocaleRecord) {
  let name = case locale_name(store_in, "en") {
    Some(value) -> value
    None -> "English"
  }
  [
    ShopLocaleRecord(
      locale: "en",
      name: name,
      primary: True,
      published: True,
      market_web_presence_ids: [],
    ),
  ]
}

fn list_shop_locales(
  store_in: Store,
  published: Option(Bool),
) -> List(ShopLocaleRecord) {
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

fn get_shop_locale(
  store_in: Store,
  locale: String,
) -> Option(ShopLocaleRecord) {
  case store.get_effective_shop_locale(store_in, locale) {
    Some(record) -> Some(record)
    None ->
      default_shop_locales(store_in)
      |> list.find(fn(candidate) { candidate.locale == locale })
      |> option.from_result
  }
}

// ---------------------------------------------------------------------------
// Resource reconstruction
// ---------------------------------------------------------------------------

/// Find a translatable resource by id. Without a Products domain the
/// proxy can't truly enumerate translatable resources, so this is
/// always `None` — every resourceId surfaces as RESOURCE_NOT_FOUND
/// during validation. Once the Products domain ports, this should
/// derive a `TranslatableResource` from the matching `ProductRecord`
/// (and `MetafieldRecord`) just like `findResource` in TS.
fn find_resource(
  _store_in: Store,
  _resource_id: String,
) -> Option(TranslatableResource) {
  None
}

/// Enumerate every translatable resource of a given type. Returns
/// `[]` for the same reason as `find_resource` — no Products in the
/// Gleam store yet.
fn list_resources(
  _store_in: Store,
  _resource_type: Option(String),
) -> List(TranslatableResource) {
  []
}

// ---------------------------------------------------------------------------
// Read-path serialization
// ---------------------------------------------------------------------------

fn serialize_root_fields(
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

fn field_args(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> Dict(String, root_field.ResolvedValue) {
  case root_field.get_field_arguments(field, variables) {
    Ok(d) -> d
    Error(_) -> dict.new()
  }
}

fn read_arg_string(
  args: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(String) {
  case dict.get(args, name) {
    Ok(root_field.StringVal(s)) -> Some(s)
    _ -> None
  }
}

fn read_arg_bool(
  args: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(Bool) {
  case dict.get(args, name) {
    Ok(root_field.BoolVal(b)) -> Some(b)
    _ -> None
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

fn serialize_locale(locale: LocaleRecord, field: Selection) -> Json {
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
  _fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = field_args(field, variables)
  let published = read_arg_bool(args, "published")
  let locales = list_shop_locales(store_in, published)
  json.array(locales, fn(locale) { serialize_shop_locale(locale, field) })
}

fn serialize_shop_locale(locale: ShopLocaleRecord, field: Selection) -> Json {
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
            "marketWebPresences" -> #(key, json.preprocessed_array([]))
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

// translatableResource
fn serialize_translatable_resource_root(
  store_in: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = field_args(field, variables)
  case read_arg_string(args, "resourceId") {
    Some(resource_id) -> {
      let resource = find_resource_or_synthesize(store_in, resource_id)
      serialize_resource(store_in, resource, field, fragments, variables)
    }
    None -> json.null()
  }
}

/// Synthesize a translatable resource from in-store translations even
/// when the underlying Product/Metafield record isn't available. This
/// lets register-then-read parity work without a Products domain — at
/// the cost of returning the registered translations only (no
/// translatable content slots, since the source content isn't in the
/// store). Returns `None` only when the resourceId has zero matching
/// translations and zero matching products/metafields.
fn find_resource_or_synthesize(
  store_in: Store,
  resource_id: String,
) -> Option(TranslatableResource) {
  case find_resource(store_in, resource_id) {
    Some(record) -> Some(record)
    None -> {
      let any_translation =
        list.any(dict.values(store_in.staged_state.translations), fn(t) {
          t.resource_id == resource_id
        })
        || list.any(dict.values(store_in.base_state.translations), fn(t) {
          t.resource_id == resource_id
        })
      case any_translation {
        True ->
          Some(
            TranslatableResource(
              resource_id: resource_id,
              resource_type: synthetic_resource_type(resource_id),
              content: [],
            ),
          )
        False -> None
      }
    }
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
  let args = field_args(field, variables)
  let locale = read_arg_string(args, "locale")
  let market_id = read_arg_string(args, "marketId")
  let outdated = read_arg_bool(args, "outdated")
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

fn serialize_translation(
  _store_in: Store,
  translation: TranslationRecord,
  field: Selection,
  _fragments: FragmentMap,
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
            "market" -> #(key, json.null())
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

fn serialize_translatable_resources_root(
  store_in: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = field_args(field, variables)
  let resource_type = read_arg_string(args, "resourceType")
  let reverse = read_arg_bool(args, "reverse")
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
  let args = field_args(field, variables)
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
  let reverse = read_arg_bool(args, "reverse")
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
// Mutation handlers
// ---------------------------------------------------------------------------

type MutationFieldResult {
  MutationFieldResult(
    key: String,
    payload: Json,
    staged_resource_ids: List(String),
  )
}

fn handle_mutation_fields(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  fields: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationOutcome {
  let initial = #([], store_in, identity, [])
  let #(data_entries, final_store, final_identity, all_staged) =
    list.fold(fields, initial, fn(acc, field) {
      let #(entries, current_store, current_identity, staged_ids) = acc
      case field {
        Field(name: name, ..) -> {
          let dispatch = case name.value {
            "shopLocaleEnable" ->
              Some(handle_shop_locale_enable(
                current_store,
                current_identity,
                field,
                variables,
              ))
            "shopLocaleUpdate" ->
              Some(handle_shop_locale_update(
                current_store,
                current_identity,
                field,
                variables,
              ))
            "shopLocaleDisable" ->
              Some(handle_shop_locale_disable(
                current_store,
                current_identity,
                field,
                variables,
              ))
            "translationsRegister" ->
              Some(handle_translations_register(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            "translationsRemove" ->
              Some(handle_translations_remove(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            _ -> None
          }
          case dispatch {
            None -> acc
            Some(#(result, next_store, next_identity)) -> #(
              list.append(entries, [#(result.key, result.payload)]),
              next_store,
              next_identity,
              list.append(staged_ids, result.staged_resource_ids),
            )
          }
        }
        _ -> acc
      }
    })
  MutationOutcome(
    data: json.object([#("data", json.object(data_entries))]),
    store: final_store,
    identity: final_identity,
    staged_resource_ids: all_staged,
  )
}

// shopLocaleEnable
fn handle_shop_locale_enable(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = field_args(field, variables)
  let locale = case read_arg_string(args, "locale") {
    Some(s) -> s
    None -> ""
  }
  case locale_name(store_in, locale) {
    None -> {
      let payload =
        project_shop_locale_payload(
          field,
          None,
          None,
          [invalid_locale_error()],
          "ShopLocaleEnablePayload",
        )
      #(
        MutationFieldResult(key: key, payload: payload, staged_resource_ids: []),
        store_in,
        identity,
      )
    }
    Some(name) -> {
      let market_web_presence_ids = case
        read_optional_string_array(args, "marketWebPresenceIds")
      {
        Some(ids) -> ids
        None -> []
      }
      let existing = get_shop_locale(store_in, locale)
      let existing_primary = case existing {
        Some(record) -> record.primary
        None -> False
      }
      let existing_published = case existing {
        Some(record) -> record.published
        None -> False
      }
      let record =
        ShopLocaleRecord(
          locale: locale,
          name: name,
          primary: existing_primary,
          published: existing_published,
          market_web_presence_ids: market_web_presence_ids,
        )
      let #(_, store_after) = store.stage_shop_locale(store_in, record)
      let payload =
        project_shop_locale_payload(
          field,
          Some(record),
          None,
          [],
          "ShopLocaleEnablePayload",
        )
      #(
        MutationFieldResult(key: key, payload: payload, staged_resource_ids: [
          shop_locale_staged_id(record),
        ]),
        store_after,
        identity,
      )
    }
  }
}

// shopLocaleUpdate
fn handle_shop_locale_update(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = field_args(field, variables)
  let locale = case read_arg_string(args, "locale") {
    Some(s) -> s
    None -> ""
  }
  let existing = get_shop_locale(store_in, locale)
  let has_locale_name = case locale_name(store_in, locale) {
    Some(_) -> True
    None -> False
  }
  case existing, has_locale_name {
    Some(current), True -> {
      let input = read_input_object(args, "shopLocale")
      let market_web_presence_ids = case
        read_optional_string_array(input, "marketWebPresenceIds")
      {
        Some(ids) -> ids
        None -> current.market_web_presence_ids
      }
      let published = case read_arg_bool(input, "published") {
        Some(b) -> b
        None -> current.published
      }
      let record =
        ShopLocaleRecord(
          locale: current.locale,
          name: current.name,
          primary: current.primary,
          published: published,
          market_web_presence_ids: market_web_presence_ids,
        )
      let #(_, store_after) = store.stage_shop_locale(store_in, record)
      let payload =
        project_shop_locale_payload(
          field,
          Some(record),
          None,
          [],
          "ShopLocaleUpdatePayload",
        )
      #(
        MutationFieldResult(key: key, payload: payload, staged_resource_ids: [
          shop_locale_staged_id(record),
        ]),
        store_after,
        identity,
      )
    }
    _, _ -> {
      let payload =
        project_shop_locale_payload(
          field,
          None,
          None,
          [invalid_locale_error()],
          "ShopLocaleUpdatePayload",
        )
      #(
        MutationFieldResult(key: key, payload: payload, staged_resource_ids: []),
        store_in,
        identity,
      )
    }
  }
}

// shopLocaleDisable
fn handle_shop_locale_disable(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = field_args(field, variables)
  let locale = case read_arg_string(args, "locale") {
    Some(s) -> s
    None -> ""
  }
  let existing = get_shop_locale(store_in, locale)
  let has_locale_name = case locale_name(store_in, locale) {
    Some(_) -> True
    None -> False
  }
  let primary = case existing {
    Some(record) -> record.primary
    None -> False
  }
  case existing, has_locale_name, primary {
    Some(_), True, False -> {
      let #(_, store_after_disable) =
        store.disable_shop_locale(store_in, locale)
      let #(_, store_after) =
        store.remove_translations_for_locale(store_after_disable, locale)
      let payload =
        project_shop_locale_payload(
          field,
          None,
          Some(locale),
          [],
          "ShopLocaleDisablePayload",
        )
      #(
        MutationFieldResult(key: key, payload: payload, staged_resource_ids: []),
        store_after,
        identity,
      )
    }
    _, _, _ -> {
      let payload =
        project_shop_locale_payload(
          field,
          None,
          Some(locale),
          [invalid_locale_error()],
          "ShopLocaleDisablePayload",
        )
      #(
        MutationFieldResult(key: key, payload: payload, staged_resource_ids: []),
        store_in,
        identity,
      )
    }
  }
}

fn invalid_locale_error() -> AnyUserError {
  ShopLocaleError(field: ["locale"], message: "Locale is invalid")
}

fn shop_locale_staged_id(record: ShopLocaleRecord) -> String {
  "ShopLocale/" <> record.locale
}

fn read_input_object(
  args: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Dict(String, root_field.ResolvedValue) {
  case dict.get(args, key) {
    Ok(root_field.ObjectVal(d)) -> d
    _ -> dict.new()
  }
}

fn project_shop_locale_payload(
  field: Selection,
  shop_locale: Option(ShopLocaleRecord),
  locale: Option(String),
  errors: List(AnyUserError),
  _typename: String,
) -> Json {
  let entries =
    list.map(selections_of(field), fn(selection) {
      let key = get_field_response_key(selection)
      case selection {
        Field(name: name, ..) ->
          case name.value {
            "shopLocale" -> #(key, case shop_locale {
              Some(record) -> serialize_shop_locale(record, selection)
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
              "code", ShopLocaleError(..) -> #(key, json.null())
              _, _ -> #(key, json.null())
            }
          _ -> #(key, json.null())
        }
      })
    json.object(entries)
  })
}

// translationsRegister
fn handle_translations_register(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = field_args(field, variables)
  let resource_validation = validate_resource(store_in, args)
  let initial_errors = resource_validation.1
  let inputs = read_translation_inputs(args)
  let blank_errors = case inputs {
    [] -> [
      TranslationError(
        field: ["translations"],
        message: "At least one translation is required",
        code: "BLANK",
      ),
    ]
    _ -> []
  }
  let errors = list.append(initial_errors, blank_errors)

  let #(translations, errors, identity_after) = case resource_validation.0 {
    Some(resource) ->
      validate_and_build_translations(
        store_in,
        identity,
        resource,
        inputs,
        errors,
      )
    None -> #([], errors, identity)
  }

  let #(store_after, staged_ids) = case errors {
    [] -> {
      let store_after =
        list.fold(translations, store_in, fn(acc, t) {
          let #(_, next) = store.stage_translation(acc, t)
          next
        })
      let ids =
        list.map(translations, fn(t) {
          store.translation_storage_key(
            t.resource_id,
            t.locale,
            t.key,
            t.market_id,
          )
        })
      #(store_after, ids)
    }
    _ -> #(store_in, [])
  }

  let translations_for_payload = case errors {
    [] -> Some(translations)
    _ -> None
  }
  let payload =
    project_translations_payload(
      store_after,
      translations_for_payload,
      errors,
      field,
      fragments,
    )
  #(
    MutationFieldResult(
      key: key,
      payload: payload,
      staged_resource_ids: staged_ids,
    ),
    store_after,
    identity_after,
  )
}

// translationsRemove
fn handle_translations_remove(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = field_args(field, variables)
  let resource_validation = validate_resource(store_in, args)
  let resource = resource_validation.0
  let initial_errors = resource_validation.1

  let keys = case read_optional_string_array(args, "translationKeys") {
    Some(ks) -> ks
    None -> []
  }
  let locales = case read_optional_string_array(args, "locales") {
    Some(ls) -> ls
    None -> []
  }
  let market_ids = case read_optional_string_array(args, "marketIds") {
    Some(m) -> m
    None -> []
  }

  let key_errors = case keys {
    [] -> [
      TranslationError(
        field: ["translationKeys"],
        message: "At least one translation key is required",
        code: "BLANK",
      ),
    ]
    _ -> []
  }
  let locale_errors = case locales {
    [] -> [
      TranslationError(
        field: ["locales"],
        message: "At least one locale is required",
        code: "BLANK",
      ),
    ]
    _ -> []
  }
  let market_errors = case market_ids {
    [] -> []
    _ -> [
      TranslationError(
        field: ["marketIds"],
        message: "Market-specific translations are not supported for this local resource branch",
        code: "MARKET_CUSTOM_CONTENT_NOT_ALLOWED",
      ),
    ]
  }

  let resource_errors = case resource {
    Some(record) -> {
      let key_validation =
        list.flat_map(keys, fn(k) {
          case content_has_key(record.content, k) {
            True -> []
            False -> [
              TranslationError(
                field: ["translationKeys"],
                message: "Key " <> k <> " is not translatable for this resource",
                code: "INVALID_KEY_FOR_MODEL",
              ),
            ]
          }
        })
      let locale_validation =
        list.flat_map(locales, fn(loc) {
          validate_locale_errors(store_in, loc, ["locales"])
        })
      list.append(key_validation, locale_validation)
    }
    None -> []
  }

  let errors =
    list.append(
      initial_errors,
      list.append(
        key_errors,
        list.append(locale_errors, list.append(market_errors, resource_errors)),
      ),
    )

  let #(removed, store_after) = case errors, resource {
    [], Some(record) -> {
      let #(removed, store_acc) =
        list.fold(locales, #([], store_in), fn(outer_acc, loc) {
          list.fold(keys, outer_acc, fn(inner_acc, k) {
            let #(removed_acc, store_step) = inner_acc
            let #(removed_record, store_next) =
              store.remove_translation(
                store_step,
                record.resource_id,
                loc,
                k,
                None,
              )
            case removed_record {
              Some(t) -> #(list.append(removed_acc, [t]), store_next)
              None -> #(removed_acc, store_next)
            }
          })
        })
      #(removed, store_acc)
    }
    _, _ -> #([], store_in)
  }

  let translations_for_payload = case errors {
    [] -> Some(removed)
    _ -> None
  }
  let payload =
    project_translations_payload(
      store_after,
      translations_for_payload,
      errors,
      field,
      fragments,
    )
  #(
    MutationFieldResult(key: key, payload: payload, staged_resource_ids: []),
    store_after,
    identity,
  )
}

fn validate_resource(
  store_in: Store,
  args: Dict(String, root_field.ResolvedValue),
) -> #(Option(TranslatableResource), List(AnyUserError)) {
  case read_arg_string(args, "resourceId") {
    None -> #(None, [
      TranslationError(
        field: ["resourceId"],
        message: "Resource does not exist",
        code: "RESOURCE_NOT_FOUND",
      ),
    ])
    Some("") -> #(None, [
      TranslationError(
        field: ["resourceId"],
        message: "Resource does not exist",
        code: "RESOURCE_NOT_FOUND",
      ),
    ])
    Some(resource_id) ->
      case find_resource(store_in, resource_id) {
        None -> #(None, [
          TranslationError(
            field: ["resourceId"],
            message: "Resource " <> resource_id <> " does not exist",
            code: "RESOURCE_NOT_FOUND",
          ),
        ])
        Some(record) -> #(Some(record), [])
      }
  }
}

fn validate_locale_errors(
  store_in: Store,
  locale: String,
  field_path: List(String),
) -> List(AnyUserError) {
  case get_shop_locale(store_in, locale) {
    Some(_) -> []
    None -> [
      TranslationError(
        field: field_path,
        message: "Locale is not enabled for this shop",
        code: "INVALID_LOCALE_FOR_SHOP",
      ),
    ]
  }
}

fn content_has_key(content: List(TranslatableContent), key: String) -> Bool {
  list.any(content, fn(entry) { entry.key == key })
}

fn read_translation_inputs(
  args: Dict(String, root_field.ResolvedValue),
) -> List(Dict(String, root_field.ResolvedValue)) {
  case dict.get(args, "translations") {
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

fn validate_and_build_translations(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  resource: TranslatableResource,
  inputs: List(Dict(String, root_field.ResolvedValue)),
  initial_errors: List(AnyUserError),
) -> #(List(TranslationRecord), List(AnyUserError), SyntheticIdentityRegistry) {
  let #(_, errors_after, translations_rev, identity_after) =
    list.fold(inputs, #(0, initial_errors, [], identity), fn(acc, input) {
      let #(index, errors_acc, translations_acc, identity_acc) = acc
      let prefix = ["translations", int.to_string(index)]
      let locale_validation = case read_arg_string(input, "locale") {
        Some(loc) ->
          case get_shop_locale(store_in, loc) {
            Some(_) -> #(Some(loc), [])
            None -> #(Some(loc), [
              TranslationError(
                field: list.append(prefix, ["locale"]),
                message: "Locale is not enabled for this shop",
                code: "INVALID_LOCALE_FOR_SHOP",
              ),
            ])
          }
        None -> #(None, [
          TranslationError(
            field: list.append(prefix, ["locale"]),
            message: "Locale is not enabled for this shop",
            code: "INVALID_LOCALE_FOR_SHOP",
          ),
        ])
      }
      let #(maybe_locale, locale_errs) = locale_validation
      let key = case read_arg_string(input, "key") {
        Some(k) -> k
        None -> ""
      }
      let content = list.find(resource.content, fn(c) { c.key == key })
      let key_errors = case content {
        Ok(_) -> []
        Error(_) -> [
          TranslationError(
            field: list.append(prefix, ["key"]),
            message: "Key " <> key <> " is not translatable for this resource",
            code: "INVALID_KEY_FOR_MODEL",
          ),
        ]
      }
      let value = read_arg_string(input, "value")
      let value_errors = case value {
        Some(v) ->
          case v {
            "" -> [
              TranslationError(
                field: list.append(prefix, ["value"]),
                message: "Value can't be blank",
                code: "BLANK",
              ),
            ]
            _ -> []
          }
        None -> [
          TranslationError(
            field: list.append(prefix, ["value"]),
            message: "Value can't be blank",
            code: "BLANK",
          ),
        ]
      }
      let supplied_digest = read_arg_string(input, "translatableContentDigest")
      let digest_errors = case content, supplied_digest {
        Ok(c), Some(supplied) ->
          case c.digest {
            Some(actual) ->
              case actual == supplied {
                True -> []
                False -> [
                  TranslationError(
                    field: list.append(prefix, ["translatableContentDigest"]),
                    message: "Translatable content digest does not match the resource content",
                    code: "INVALID_TRANSLATABLE_CONTENT",
                  ),
                ]
              }
            None -> []
          }
        _, _ -> []
      }
      let market_errors = case read_arg_string(input, "marketId") {
        Some(_) -> [
          TranslationError(
            field: list.append(prefix, ["marketId"]),
            message: "Market-specific translations are not supported for this local resource branch",
            code: "MARKET_CUSTOM_CONTENT_NOT_ALLOWED",
          ),
        ]
        None -> []
      }
      let row_errors =
        list.append(
          locale_errs,
          list.append(
            key_errors,
            list.append(value_errors, list.append(digest_errors, market_errors)),
          ),
        )
      let new_errors = list.append(errors_acc, row_errors)
      let can_record = case row_errors, maybe_locale, value, content {
        [], Some(_), Some(_), Ok(_) -> True
        _, _, _, _ -> False
      }
      case can_record, errors_acc, maybe_locale, value, content {
        True, [], Some(loc), Some(v), Ok(c) -> {
          let #(timestamp, identity_next) =
            synthetic_identity.make_synthetic_timestamp(identity_acc)
          let supplied_digest_value = case supplied_digest {
            Some(d) -> d
            None ->
              case c.digest {
                Some(d) -> d
                None -> ""
              }
          }
          let record =
            TranslationRecord(
              resource_id: resource.resource_id,
              key: key,
              locale: loc,
              value: v,
              translatable_content_digest: supplied_digest_value,
              market_id: None,
              updated_at: timestamp,
              outdated: False,
            )
          #(index + 1, new_errors, [record, ..translations_acc], identity_next)
        }
        _, _, _, _, _ -> #(
          index + 1,
          new_errors,
          translations_acc,
          identity_acc,
        )
      }
    })
  #(list.reverse(translations_rev), errors_after, identity_after)
}

fn project_translations_payload(
  store_in: Store,
  translations: Option(List(TranslationRecord)),
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

fn optional_string_json(value: Option(String)) -> Json {
  case value {
    Some(s) -> json.string(s)
    None -> json.null()
  }
}

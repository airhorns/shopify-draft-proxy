//// Localization mutation dispatch and local staging handlers.

import gleam/dict.{type Dict}
import gleam/int
import gleam/json
import gleam/list
import gleam/option.{type Option, None, Some}
import shopify_draft_proxy/graphql/ast.{type Selection, Field}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/admin_api_versions
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, get_document_fragments, get_field_response_key,
}
import shopify_draft_proxy/proxy/localization/serializers
import shopify_draft_proxy/proxy/localization/types.{
  type AnyUserError, type TranslatableResource, type TranslationErrorCode,
  FailsResourceValidation, InvalidKeyForModel, InvalidLocaleForShop,
  InvalidTranslatableContent, ResourceNotFound, SameLocaleAsShopPrimary,
  TooManyKeysForResource, max_keys_per_translation_mutation,
  proxy_translation_error, translation_error,
}
import shopify_draft_proxy/proxy/mutation_helpers.{
  type MutationFieldResult, type MutationOutcome, MutationFieldResult,
  MutationOutcome, read_optional_string_array, single_root_log_draft,
}
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/store/types as store_types
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types as state_types

const maximum_alternate_shop_locales = 20

@internal
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

@internal
pub fn process_mutation(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  _upstream: UpstreamContext,
) -> MutationOutcome {
  case root_field.get_root_fields(document) {
    Error(err) -> mutation_helpers.parse_failed_outcome(store_in, identity, err)
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      handle_mutation_fields(
        store_in,
        identity,
        request_path,
        fields,
        fragments,
        variables,
      )
    }
  }
}

// ---------------------------------------------------------------------------

fn handle_mutation_fields(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  fields: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationOutcome {
  let initial = #([], store_in, identity, [], [])
  let #(data_entries, final_store, final_identity, all_staged, all_drafts) =
    list.fold(fields, initial, fn(acc, field) {
      let #(entries, current_store, current_identity, staged_ids, drafts) = acc
      case field {
        Field(name: name, ..) -> {
          let dispatch = case name.value {
            "shopLocaleEnable" ->
              Some(handle_shop_locale_enable(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            "shopLocaleUpdate" ->
              Some(handle_shop_locale_update(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            "shopLocaleDisable" ->
              Some(handle_shop_locale_disable(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            "translationsRegister" ->
              Some(handle_translations_register(
                current_store,
                current_identity,
                request_path,
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
            Some(#(result, next_store, next_identity)) -> {
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  localization_status_for(
                    name.value,
                    result.staged_resource_ids,
                  ),
                  "localization",
                  "stage-locally",
                  Some(localization_notes_for(name.value)),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                next_store,
                next_identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
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
    log_drafts: all_drafts,
  )
}

/// Per-root-field log status for localization mutations. Default
/// rule: an empty `staged_resource_ids` means the validation path
/// rejected the request, so the entry logs `Failed`; otherwise
/// `Staged`.
fn localization_status_for(
  _root_field_name: String,
  staged_resource_ids: List(String),
) -> store.EntryStatus {
  case staged_resource_ids {
    [] -> store_types.Failed
    [_, ..] -> store_types.Staged
  }
}

/// Notes string mirroring the `localization` dispatcher in
/// `routes.ts`.
fn localization_notes_for(_root_field_name: String) -> String {
  "Staged locally in the in-memory localization draft store."
}

// shopLocaleEnable
fn handle_shop_locale_enable(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let locale = case graphql_helpers.read_arg_string(args, "locale") {
    Some(s) -> s
    None -> ""
  }
  case
    locale == serializers.primary_locale_for(store_in),
    serializers.locale_name(store_in, locale)
  {
    True, _ -> {
      let payload =
        serializers.project_shop_locale_payload(
          store_in,
          field,
          None,
          None,
          [serializers.primary_locale_error()],
          "ShopLocaleEnablePayload",
          fragments,
        )
      #(
        MutationFieldResult(key: key, payload: payload, staged_resource_ids: []),
        store_in,
        identity,
      )
    }
    False, None -> {
      let payload =
        serializers.project_shop_locale_payload(
          store_in,
          field,
          None,
          None,
          [serializers.invalid_locale_error()],
          "ShopLocaleEnablePayload",
          fragments,
        )
      #(
        MutationFieldResult(key: key, payload: payload, staged_resource_ids: []),
        store_in,
        identity,
      )
    }
    False, Some(name) -> {
      let market_web_presence_ids = case
        read_optional_string_array(args, "marketWebPresenceIds")
      {
        Some(ids) -> ids
        None -> []
      }
      let existing = serializers.get_shop_locale(store_in, locale)
      case
        existing,
        enabled_alternate_locale_count(store_in)
        >= maximum_alternate_shop_locales
      {
        Some(_), _ -> {
          let payload =
            serializers.project_shop_locale_payload(
              store_in,
              field,
              None,
              None,
              [serializers.shop_locale_taken_error()],
              "ShopLocaleEnablePayload",
              fragments,
            )
          #(
            MutationFieldResult(
              key: key,
              payload: payload,
              staged_resource_ids: [],
            ),
            store_in,
            identity,
          )
        }
        None, True -> {
          let payload =
            serializers.project_shop_locale_payload(
              store_in,
              field,
              None,
              None,
              [serializers.shop_locale_limit_reached_error(name)],
              "ShopLocaleEnablePayload",
              fragments,
            )
          #(
            MutationFieldResult(
              key: key,
              payload: payload,
              staged_resource_ids: [],
            ),
            store_in,
            identity,
          )
        }
        None, False -> {
          let record =
            state_types.ShopLocaleRecord(
              locale: locale,
              name: name,
              primary: False,
              published: False,
              market_web_presence_ids: market_web_presence_ids,
            )
          let #(_, store_after) = store.stage_shop_locale(store_in, record)
          let payload =
            serializers.project_shop_locale_payload(
              store_after,
              field,
              Some(record),
              None,
              [],
              "ShopLocaleEnablePayload",
              fragments,
            )
          #(
            MutationFieldResult(
              key: key,
              payload: payload,
              staged_resource_ids: [serializers.shop_locale_staged_id(record)],
            ),
            store_after,
            identity,
          )
        }
      }
    }
  }
}

fn enabled_alternate_locale_count(store_in: Store) -> Int {
  serializers.list_shop_locales(store_in, None)
  |> list.filter(fn(locale) { !locale.primary })
  |> list.length
}

// shopLocaleUpdate
fn handle_shop_locale_update(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let locale = case graphql_helpers.read_arg_string(args, "locale") {
    Some(s) -> s
    None -> ""
  }
  let existing = serializers.get_shop_locale(store_in, locale)
  let input = serializers.read_input_object(args, "shopLocale")
  let published_input = graphql_helpers.read_arg_bool(input, "published")
  case existing {
    Some(current) -> {
      let market_web_presence_ids = case
        read_optional_string_array(input, "marketWebPresenceIds")
      {
        Some(ids) -> ids
        None -> current.market_web_presence_ids
      }
      let published = case published_input {
        Some(b) -> b
        None -> current.published
      }
      case current.primary && !published {
        True -> {
          let payload =
            serializers.project_shop_locale_payload(
              store_in,
              field,
              None,
              None,
              [serializers.primary_locale_error()],
              "ShopLocaleUpdatePayload",
              fragments,
            )
          #(
            MutationFieldResult(
              key: key,
              payload: payload,
              staged_resource_ids: [],
            ),
            store_in,
            identity,
          )
        }
        False -> {
          let record =
            state_types.ShopLocaleRecord(
              locale: current.locale,
              name: current.name,
              primary: current.primary,
              published: published,
              market_web_presence_ids: market_web_presence_ids,
            )
          let #(_, store_after) = store.stage_shop_locale(store_in, record)
          let payload =
            serializers.project_shop_locale_payload(
              store_after,
              field,
              Some(record),
              None,
              [],
              "ShopLocaleUpdatePayload",
              fragments,
            )
          #(
            MutationFieldResult(
              key: key,
              payload: payload,
              staged_resource_ids: [
                serializers.shop_locale_staged_id(record),
              ],
            ),
            store_after,
            identity,
          )
        }
      }
    }
    None -> {
      case published_input {
        Some(_) -> {
          let payload =
            serializers.project_shop_locale_payload(
              store_in,
              field,
              None,
              None,
              [serializers.shop_locale_does_not_exist_error()],
              "ShopLocaleUpdatePayload",
              fragments,
            )
          #(
            MutationFieldResult(
              key: key,
              payload: payload,
              staged_resource_ids: [],
            ),
            store_in,
            identity,
          )
        }
        None -> {
          let market_web_presence_ids = case
            read_optional_string_array(input, "marketWebPresenceIds")
          {
            Some(ids) -> ids
            None -> []
          }
          let name =
            serializers.locale_name(store_in, locale)
            |> option.unwrap(locale)
          let record =
            state_types.ShopLocaleRecord(
              locale: locale,
              name: name,
              primary: False,
              published: False,
              market_web_presence_ids: market_web_presence_ids,
            )
          let #(_, store_after) = store.stage_shop_locale(store_in, record)
          let payload =
            serializers.project_shop_locale_payload(
              store_after,
              field,
              Some(record),
              None,
              [],
              "ShopLocaleUpdatePayload",
              fragments,
            )
          #(
            MutationFieldResult(
              key: key,
              payload: payload,
              staged_resource_ids: [serializers.shop_locale_staged_id(record)],
            ),
            store_after,
            identity,
          )
        }
      }
    }
  }
}

// shopLocaleDisable
fn handle_shop_locale_disable(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let locale = case graphql_helpers.read_arg_string(args, "locale") {
    Some(s) -> s
    None -> ""
  }
  let existing = serializers.get_shop_locale(store_in, locale)
  case existing {
    Some(record) if record.primary -> {
      let payload =
        serializers.project_shop_locale_payload(
          store_in,
          field,
          None,
          None,
          [serializers.primary_locale_error()],
          "ShopLocaleDisablePayload",
          fragments,
        )
      #(
        MutationFieldResult(key: key, payload: payload, staged_resource_ids: []),
        store_in,
        identity,
      )
    }
    Some(_) -> {
      let #(_, store_after_disable) =
        store.disable_shop_locale(store_in, locale)
      let #(_, store_after) =
        store.remove_translations_for_locale(store_after_disable, locale)
      let payload =
        serializers.project_shop_locale_payload(
          store_after,
          field,
          None,
          Some(locale),
          [],
          "ShopLocaleDisablePayload",
          fragments,
        )
      #(
        MutationFieldResult(key: key, payload: payload, staged_resource_ids: []),
        store_after,
        identity,
      )
    }
    None -> {
      let payload =
        serializers.project_shop_locale_payload(
          store_in,
          field,
          None,
          None,
          [serializers.shop_locale_does_not_exist_error()],
          "ShopLocaleDisablePayload",
          fragments,
        )
      #(
        MutationFieldResult(key: key, payload: payload, staged_resource_ids: []),
        store_in,
        identity,
      )
    }
  }
}

fn handle_translations_register(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let resource_validation = validate_resource(store_in, args)
  let initial_errors = resource_validation.1
  let inputs = read_translation_inputs(args)
  let has_too_many_keys =
    list.length(inputs) > max_keys_per_translation_mutation
  let length_errors = case has_too_many_keys {
    True -> [
      translation_error(
        ["resourceId"],
        "Too many keys for resource - maximum 100 per mutation",
        TooManyKeysForResource,
      ),
    ]
    False -> []
  }
  let errors = list.append(initial_errors, length_errors)

  let #(translations, errors, identity_after) = case resource_validation.0 {
    Some(resource) ->
      validate_and_build_translations(
        store_in,
        identity,
        request_path,
        resource,
        inputs,
        errors,
      )
    None -> #([], errors, identity)
  }

  let translations_for_payload = case errors {
    [] -> Some(translations)
    _ ->
      case resource_validation.0, has_too_many_keys {
        Some(_), False -> Some(translations)
        _, _ -> None
      }
  }
  let staged_translations = case translations_for_payload {
    Some(rows) -> rows
    None -> []
  }
  let store_after =
    list.fold(staged_translations, store_in, fn(acc, t) {
      let #(_, next) = store.stage_translation(acc, t)
      next
    })
  let staged_ids =
    list.map(staged_translations, fn(t) {
      store.translation_storage_key(t.resource_id, t.locale, t.key, t.market_id)
    })
  let payload =
    serializers.project_translations_payload(
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
  let args = graphql_helpers.field_args(field, variables)
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

  let has_empty_remove_target = list.is_empty(keys) || list.is_empty(locales)
  let errors = initial_errors

  let #(removed, store_after) = case errors, resource, has_empty_remove_target {
    [], Some(record), False -> {
      let market_targets = case market_ids {
        [] -> [None]
        _ -> list.map(market_ids, Some)
      }
      let #(removed, store_acc) =
        list.fold(locales, #([], store_in), fn(outer_acc, loc) {
          list.fold(keys, outer_acc, fn(inner_acc, k) {
            list.fold(market_targets, inner_acc, fn(market_acc, market_id) {
              let #(removed_acc, store_step) = market_acc
              let #(removed_record, store_next) =
                store.remove_translation(
                  store_step,
                  record.resource_id,
                  loc,
                  k,
                  market_id,
                )
              case removed_record {
                Some(t) -> #(list.append(removed_acc, [t]), store_next)
                None -> #(removed_acc, store_next)
              }
            })
          })
        })
      #(removed, store_acc)
    }
    _, _, _ -> #([], store_in)
  }

  let translations_for_payload = case errors, has_empty_remove_target, removed {
    [], False, [_, ..] -> Some(removed)
    _, _, _ -> None
  }
  let payload =
    serializers.project_translations_payload(
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
  case graphql_helpers.read_arg_string(args, "resourceId") {
    None -> #(None, [
      translation_error(
        ["resourceId"],
        "Resource does not exist",
        ResourceNotFound,
      ),
    ])
    Some("") -> #(None, [
      translation_error(
        ["resourceId"],
        "Resource does not exist",
        ResourceNotFound,
      ),
    ])
    Some(resource_id) ->
      case serializers.resource_exists_for_validation(store_in, resource_id) {
        None ->
          case serializers.unsupported_translatable_resource_type(resource_id) {
            Some(resource_type) -> #(None, [
              proxy_translation_error(
                ["resourceId"],
                "Translatable resource type "
                  <> resource_type
                  <> " is not supported by the draft proxy yet",
                "UNSUPPORTED_TRANSLATABLE_RESOURCE_TYPE",
              ),
            ])
            None -> #(None, [
              translation_error(
                ["resourceId"],
                "Resource " <> resource_id <> " does not exist",
                ResourceNotFound,
              ),
            ])
          }
        Some(record) -> #(Some(record), [])
      }
  }
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
  request_path: String,
  resource: TranslatableResource,
  inputs: List(Dict(String, root_field.ResolvedValue)),
  initial_errors: List(AnyUserError),
) -> #(
  List(state_types.TranslationRecord),
  List(AnyUserError),
  SyntheticIdentityRegistry,
) {
  let primary_locale_error_code =
    primary_locale_translation_error_code(request_path)
  let #(_, errors_after, translations_rev, identity_after) =
    list.fold(inputs, #(0, initial_errors, [], identity), fn(acc, input) {
      let #(index, errors_acc, translations_acc, identity_acc) = acc
      let prefix = ["translations", int.to_string(index)]
      let locale_validation = case
        graphql_helpers.read_arg_string(input, "locale")
      {
        Some(loc) ->
          case loc == serializers.primary_locale_for(store_in) {
            True -> #(Some(loc), [
              translation_error(
                list.append(prefix, ["locale"]),
                "Locale cannot be the same as the shop primary locale",
                primary_locale_error_code,
              ),
            ])
            False ->
              case serializers.get_shop_locale(store_in, loc) {
                Some(_) -> #(Some(loc), [])
                None -> #(Some(loc), [
                  translation_error(
                    list.append(prefix, ["locale"]),
                    "Locale is not enabled for this shop",
                    InvalidLocaleForShop,
                  ),
                ])
              }
          }
        None -> #(None, [
          translation_error(
            list.append(prefix, ["locale"]),
            "Locale is not enabled for this shop",
            InvalidLocaleForShop,
          ),
        ])
      }
      let #(maybe_locale, locale_errs) = locale_validation
      let key = case graphql_helpers.read_arg_string(input, "key") {
        Some(k) -> k
        None -> ""
      }
      let content = list.find(resource.content, fn(c) { c.key == key })
      let key_errors = case content {
        Ok(_) -> []
        Error(_) -> [
          translation_error(
            list.append(prefix, ["key"]),
            "Key " <> key <> " is not translatable for this resource",
            InvalidKeyForModel,
          ),
        ]
      }
      let value = graphql_helpers.read_arg_string(input, "value")
      let value_errors = case value {
        Some(v) ->
          case v {
            "" -> [
              translation_error(
                list.append(prefix, ["value"]),
                "Value can't be blank",
                FailsResourceValidation,
              ),
            ]
            _ -> []
          }
        None -> [
          translation_error(
            list.append(prefix, ["value"]),
            "Value can't be blank",
            FailsResourceValidation,
          ),
        ]
      }
      let supplied_digest =
        graphql_helpers.read_arg_string(input, "translatableContentDigest")
      let digest_errors = case content, supplied_digest {
        Ok(c), Some(supplied) ->
          case c.digest {
            Some(actual) ->
              case actual == supplied {
                True -> []
                False -> [
                  translation_error(
                    list.append(prefix, ["translatableContentDigest"]),
                    "Translatable content hash is invalid",
                    InvalidTranslatableContent,
                  ),
                ]
              }
            None -> []
          }
        _, _ -> []
      }
      let market_id = graphql_helpers.read_arg_string(input, "marketId")
      let row_errors =
        list.append(
          locale_errs,
          list.append(key_errors, list.append(value_errors, digest_errors)),
        )
      let new_errors = list.append(errors_acc, row_errors)
      let can_record = case row_errors, maybe_locale, value, content {
        [], Some(_), Some(_), Ok(_) -> True
        _, _, _, _ -> False
      }
      case can_record, maybe_locale, value, content {
        True, Some(loc), Some(v), Ok(c) -> {
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
            state_types.TranslationRecord(
              resource_id: resource.resource_id,
              key: key,
              locale: loc,
              value: v,
              translatable_content_digest: supplied_digest_value,
              market_id: market_id,
              updated_at: timestamp,
              outdated: False,
            )
          #(index + 1, new_errors, [record, ..translations_acc], identity_next)
        }
        _, _, _, _ -> #(index + 1, new_errors, translations_acc, identity_acc)
      }
    })
  #(list.reverse(translations_rev), errors_after, identity_after)
}

fn primary_locale_translation_error_code(
  request_path: String,
) -> TranslationErrorCode {
  case admin_api_versions.at_least(request_path, "2026-04") {
    True -> InvalidLocaleForShop
    False -> SameLocaleAsShopPrimary
  }
}

//// Markets mutation staging and validation.

import gleam/dict.{type Dict}
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection, Field}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, get_document_fragments, get_field_response_key,
}
import shopify_draft_proxy/proxy/markets/queries
import shopify_draft_proxy/proxy/markets/serializers.{
  append_unique_strings, assigned_market_country_codes, captured_field,
  captured_json_source, captured_object_upsert, captured_string_field,
  catalog_create_input_errors, catalog_data, catalog_update_input_errors,
  delete_fixed_price_nodes, delete_quantity_rule_nodes, enumerate_dicts,
  enumerate_strings, fixed_price_edge_variant_id, market_connection_from_ids,
  market_data, market_handle_in_use, market_localization_payload,
  market_name_in_use, markets_log_draft, mutation_variant_ids, option_to_result,
  optional_captured_string, price_edges, price_list_currency, price_list_data,
  price_list_input_errors, product_level_fixed_price_errors, product_payloads,
  project_record, quantity_pricing_input_errors, quantity_rule_delete_errors,
  quantity_rule_payloads, quantity_rule_user_error, quantity_rules_input_errors,
  read_arg_object_array, read_arg_string_allow_empty, read_arg_string_array,
  read_explicit_market_handle, read_market_region_inputs, read_price_list_id,
  result_to_option, string_array, translation_user_error,
  upsert_fixed_price_nodes, upsert_quantity_price_break_nodes,
  upsert_quantity_rule_nodes, user_error, user_error_with_typename,
  valid_currency, variant_payloads,
}
import shopify_draft_proxy/proxy/mutation_helpers.{
  type LogDraft, type MutationOutcome, MutationOutcome,
}
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type CapturedJsonValue, type MarketLocalizableContentRecord,
  type MarketLocalizationRecord, type PriceListRecord,
  type ProductMetafieldRecord, type ShopDomainRecord, CapturedArray,
  CapturedBool, CapturedNull, CapturedObject, CapturedString, CatalogRecord,
  MarketLocalizationRecord, MarketRecord, PriceListRecord, WebPresenceRecord,
}

@internal
pub fn is_markets_mutation_root(name: String) -> Bool {
  case name {
    "marketCreate"
    | "marketUpdate"
    | "marketDelete"
    | "catalogCreate"
    | "catalogUpdate"
    | "catalogContextUpdate"
    | "catalogDelete"
    | "priceListCreate"
    | "priceListUpdate"
    | "priceListDelete"
    | "priceListFixedPricesAdd"
    | "priceListFixedPricesUpdate"
    | "priceListFixedPricesDelete"
    | "priceListFixedPricesByProductUpdate"
    | "quantityPricingByVariantUpdate"
    | "quantityRulesAdd"
    | "quantityRulesDelete"
    | "webPresenceCreate"
    | "webPresenceUpdate"
    | "webPresenceDelete"
    | "marketLocalizationsRegister"
    | "marketLocalizationsRemove" -> True
    _ -> False
  }
}

@internal
pub fn process_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> MutationOutcome {
  case root_field.get_root_fields(document) {
    Error(err) -> mutation_helpers.parse_failed_outcome(store, identity, err)
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      let hydrated_store =
        queries.hydrate_mutation_preconditions(
          store,
          fields,
          variables,
          upstream,
        )
      handle_mutation_fields(
        hydrated_store,
        identity,
        fields,
        fragments,
        variables,
      )
    }
  }
}

type MutationFieldResult {
  MutationFieldResult(
    key: String,
    payload: Json,
    store: Store,
    identity: SyntheticIdentityRegistry,
    staged_resource_ids: List(String),
    log_drafts: List(LogDraft),
  )
}

fn handle_mutation_fields(
  store: Store,
  identity: SyntheticIdentityRegistry,
  fields: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationOutcome {
  let #(entries, final_store, final_identity, staged_ids, drafts) =
    list.fold(fields, #([], store, identity, [], []), fn(acc, field) {
      let #(current_entries, current_store, current_identity, ids, log_drafts) =
        acc
      case field {
        Field(name: name, ..) ->
          case
            handle_market_mutation(
              current_store,
              current_identity,
              field,
              name.value,
              fragments,
              variables,
            )
          {
            Some(result) -> #(
              list.append(current_entries, [#(result.key, result.payload)]),
              result.store,
              result.identity,
              list.append(ids, result.staged_resource_ids),
              list.append(log_drafts, result.log_drafts),
            )
            None -> acc
          }
        _ -> acc
      }
    })
  MutationOutcome(
    data: json.object([#("data", json.object(entries))]),
    store: final_store,
    identity: final_identity,
    staged_resource_ids: staged_ids,
    log_drafts: drafts,
  )
}

fn handle_market_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  name: String,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Option(MutationFieldResult) {
  case name {
    "marketCreate" ->
      Some(handle_market_create(store, identity, field, fragments, variables))
    "marketUpdate" ->
      Some(handle_market_update(store, identity, field, fragments, variables))
    "marketDelete" ->
      Some(handle_market_delete(store, identity, field, fragments, variables))
    "catalogCreate" ->
      Some(handle_catalog_create(store, identity, field, fragments, variables))
    "catalogUpdate" ->
      Some(handle_catalog_update(store, identity, field, fragments, variables))
    "catalogContextUpdate" ->
      Some(handle_catalog_context_update(
        store,
        identity,
        field,
        fragments,
        variables,
      ))
    "catalogDelete" ->
      Some(handle_catalog_delete(store, identity, field, fragments, variables))
    "priceListCreate" ->
      Some(handle_price_list_create(
        store,
        identity,
        field,
        fragments,
        variables,
      ))
    "priceListUpdate" ->
      Some(handle_price_list_update(
        store,
        identity,
        field,
        fragments,
        variables,
      ))
    "priceListDelete" ->
      Some(handle_price_list_delete(
        store,
        identity,
        field,
        fragments,
        variables,
      ))
    "priceListFixedPricesAdd" ->
      Some(handle_price_list_fixed_prices_add(
        store,
        identity,
        field,
        fragments,
        variables,
      ))
    "priceListFixedPricesUpdate" ->
      Some(handle_price_list_fixed_prices_update(
        store,
        identity,
        field,
        fragments,
        variables,
      ))
    "priceListFixedPricesDelete" ->
      Some(handle_price_list_fixed_prices_delete(
        store,
        identity,
        field,
        fragments,
        variables,
      ))
    "priceListFixedPricesByProductUpdate" ->
      Some(handle_price_list_fixed_prices_by_product_update(
        store,
        identity,
        field,
        fragments,
        variables,
      ))
    "quantityPricingByVariantUpdate" ->
      Some(handle_quantity_pricing_by_variant_update(
        store,
        identity,
        field,
        fragments,
        variables,
      ))
    "quantityRulesAdd" ->
      Some(handle_quantity_rules_add(
        store,
        identity,
        field,
        fragments,
        variables,
      ))
    "quantityRulesDelete" ->
      Some(handle_quantity_rules_delete(
        store,
        identity,
        field,
        fragments,
        variables,
      ))
    "webPresenceCreate" ->
      Some(handle_web_presence_create(
        store,
        identity,
        field,
        fragments,
        variables,
      ))
    "webPresenceUpdate" ->
      Some(handle_web_presence_update(
        store,
        identity,
        field,
        fragments,
        variables,
      ))
    "webPresenceDelete" ->
      Some(handle_web_presence_delete(
        store,
        identity,
        field,
        fragments,
        variables,
      ))
    "marketLocalizationsRegister" ->
      Some(handle_market_localizations_register(
        store,
        identity,
        field,
        fragments,
        variables,
      ))
    "marketLocalizationsRemove" ->
      Some(handle_market_localizations_remove(
        store,
        identity,
        field,
        fragments,
        variables,
      ))
    _ -> None
  }
}

fn handle_market_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  let name =
    graphql_helpers.read_arg_string_nonempty(input, "name") |> option.unwrap("")
  let errors = market_create_input_errors(store, input, name)
  case errors {
    [] -> {
      let #(id, next_identity) =
        synthetic_identity.make_synthetic_gid(identity, "Market")
      let data = market_data(store, id, input, None)
      let #(_, next_store) =
        store.upsert_staged_market(store, MarketRecord(id, Some(id), data))
      mutation_result(
        key,
        field,
        fragments,
        "marketCreate",
        "market",
        data,
        [],
        next_store,
        next_identity,
        [id],
      )
    }
    _ ->
      mutation_result(
        key,
        field,
        fragments,
        "marketCreate",
        "market",
        CapturedNull,
        errors,
        store,
        identity,
        [],
      )
  }
}

fn market_create_input_errors(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
  name: String,
) -> List(CapturedJsonValue) {
  market_create_name_errors(name)
  |> list.append(market_create_duplicate_name_errors(store, name))
  |> list.append(market_create_status_enabled_errors(input))
  |> list.append(market_create_handle_errors(store, input, None))
  |> list.append(market_create_plan_limit_errors(store))
  |> list.append(market_create_currency_errors(store, input))
  |> list.append(market_create_region_errors(store, input))
}

fn market_create_name_errors(name: String) -> List(CapturedJsonValue) {
  case string.trim(name) {
    "" -> [
      user_error(["input", "name"], "Name can't be blank", "BLANK"),
      user_error(
        ["input", "name"],
        "Name is too short (minimum is 2 characters)",
        "TOO_SHORT",
      ),
    ]
    trimmed ->
      case string.length(trimmed) < 2 {
        True -> [
          user_error(
            ["input", "name"],
            "Name is too short (minimum is 2 characters)",
            "TOO_SHORT",
          ),
        ]
        False -> []
      }
  }
}

fn market_create_duplicate_name_errors(
  store: Store,
  name: String,
) -> List(CapturedJsonValue) {
  case string.trim(name) {
    "" -> []
    trimmed ->
      case string.length(trimmed) < 2 {
        True -> []
        False ->
          case market_name_in_use(store, name) {
            True -> [
              user_error(
                ["input", "name"],
                "Name has already been taken",
                "TAKEN",
              ),
            ]
            False -> []
          }
      }
  }
}

fn market_create_status_enabled_errors(
  input: Dict(String, root_field.ResolvedValue),
) -> List(CapturedJsonValue) {
  let status =
    graphql_helpers.read_arg_string_nonempty(input, "status")
    |> option.unwrap("DRAFT")
  let enabled =
    graphql_helpers.read_arg_bool(input, "enabled")
    |> option.unwrap(status == "ACTIVE")
  case enabled == { status == "ACTIVE" } {
    True -> []
    False -> [
      user_error(
        ["input"],
        "Invalid status and enabled combination.",
        "INVALID_STATUS_AND_ENABLED_COMBINATION",
      ),
    ]
  }
}

fn market_create_handle_errors(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
  current_id: Option(String),
) -> List(CapturedJsonValue) {
  case read_explicit_market_handle(input) {
    Some(handle) ->
      case market_handle_in_use(store, handle, current_id) {
        True -> [
          user_error(
            ["input", "handle"],
            "Generated handle has already been taken",
            "GENERATED_DUPLICATED_HANDLE",
          ),
        ]
        False -> []
      }
    None -> []
  }
}

fn market_create_plan_limit_errors(store: Store) -> List(CapturedJsonValue) {
  let market_count = store.list_effective_markets(store) |> list.length
  case market_count >= default_market_plan_limit() {
    True -> [
      user_error(
        ["input"],
        "Shop has reached the maximum number of markets for the current plan.",
        "SHOP_REACHED_PLAN_MARKETS_LIMIT",
      ),
    ]
    False -> []
  }
}

fn default_market_plan_limit() -> Int {
  3
}

fn market_create_currency_errors(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
) -> List(CapturedJsonValue) {
  case graphql_helpers.read_arg_object(input, "currencySettings") {
    Some(currency_settings) ->
      case
        graphql_helpers.read_arg_string_nonempty(
          currency_settings,
          "baseCurrency",
        )
      {
        Some(currency) ->
          case valid_market_base_currency(store, currency) {
            True -> []
            False -> [
              user_error(
                ["input", "currencySettings", "baseCurrency"],
                "Base currency is invalid",
                "INVALID",
              ),
            ]
          }
        None -> []
      }
    None -> []
  }
}

fn valid_market_base_currency(store: Store, currency: String) -> Bool {
  valid_currency(currency) && market_base_currency_supported(store, currency)
}

fn market_base_currency_supported(store: Store, currency: String) -> Bool {
  let known =
    store.list_effective_markets(store)
    |> list.filter_map(fn(record) {
      captured_field(record.data, "currencySettings")
      |> option.then(captured_field(_, "baseCurrency"))
      |> option.then(captured_string_field(_, "currencyCode"))
      |> option_to_result
    })
  list.contains(known, currency)
  || list.contains(default_supported_market_base_currencies(), currency)
}

fn default_supported_market_base_currencies() -> List(String) {
  ["CAD", "DKK", "MXN", "USD"]
}

fn market_create_region_errors(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
) -> List(CapturedJsonValue) {
  let existing_codes = assigned_market_country_codes(store)
  read_market_region_inputs(input)
  |> list.filter_map(fn(region) {
    case list.contains(existing_codes, region.country_code) {
      True ->
        Ok(user_error(region.field, "Code has already been taken", "TAKEN"))
      False -> Error(Nil)
    }
  })
}

fn handle_market_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  case graphql_helpers.read_arg_string_nonempty(args, "id") {
    Some(id) ->
      case store.get_effective_market_by_id(store, id) {
        Some(existing) -> {
          let data = market_data(store, id, input, Some(existing.data))
          let #(_, next_store) =
            store.upsert_staged_market(
              store,
              MarketRecord(id, existing.cursor, data),
            )
          mutation_result(
            key,
            field,
            fragments,
            "marketUpdate",
            "market",
            data,
            [],
            next_store,
            identity,
            [id],
          )
        }
        None ->
          not_found_mutation_result(
            key,
            field,
            fragments,
            "marketUpdate",
            "market",
            ["id"],
            "Market does not exist",
            "MARKET_NOT_FOUND",
            store,
            identity,
          )
      }
    None ->
      not_found_mutation_result(
        key,
        field,
        fragments,
        "marketUpdate",
        "market",
        ["id"],
        "Market does not exist",
        "MARKET_NOT_FOUND",
        store,
        identity,
      )
  }
}

fn handle_market_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string_nonempty(args, "id") {
    Some(id) ->
      case store.get_effective_market_by_id(store, id) {
        Some(_) ->
          delete_result(
            key,
            field,
            fragments,
            "marketDelete",
            id,
            store.delete_staged_market(store, id),
            identity,
          )
        None ->
          delete_error_result(
            key,
            field,
            fragments,
            "marketDelete",
            ["id"],
            "Market does not exist",
            "MARKET_NOT_FOUND",
            store,
            identity,
          )
      }
    None ->
      delete_error_result(
        key,
        field,
        fragments,
        "marketDelete",
        ["id"],
        "Market does not exist",
        "MARKET_NOT_FOUND",
        store,
        identity,
      )
  }
}

fn handle_catalog_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  let title = read_arg_string_allow_empty(input, "title") |> option.unwrap("")
  let errors = catalog_create_input_errors(store, input, title)
  case errors {
    [] -> {
      let #(id, next_identity) =
        synthetic_identity.make_synthetic_gid(identity, "MarketCatalog")
      let data = catalog_data(store, id, input, None)
      let #(_, next_store) =
        store.upsert_staged_catalog(store, CatalogRecord(id, Some(id), data))
      mutation_result(
        key,
        field,
        fragments,
        "catalogCreate",
        "catalog",
        data,
        [],
        next_store,
        next_identity,
        [id],
      )
    }
    _ ->
      mutation_result(
        key,
        field,
        fragments,
        "catalogCreate",
        "catalog",
        CapturedNull,
        errors,
        store,
        identity,
        [],
      )
  }
}

fn handle_catalog_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  case graphql_helpers.read_arg_string_nonempty(args, "id") {
    Some(id) ->
      case store.get_effective_catalog_by_id(store, id) {
        Some(existing) -> {
          let errors = catalog_update_input_errors(store, input, id)
          case errors {
            [] -> {
              let data = catalog_data(store, id, input, Some(existing.data))
              let #(_, next_store) =
                store.upsert_staged_catalog(
                  store,
                  CatalogRecord(id, existing.cursor, data),
                )
              mutation_result(
                key,
                field,
                fragments,
                "catalogUpdate",
                "catalog",
                data,
                [],
                next_store,
                identity,
                [id],
              )
            }
            _ ->
              mutation_result(
                key,
                field,
                fragments,
                "catalogUpdate",
                "catalog",
                CapturedNull,
                errors,
                store,
                identity,
                [],
              )
          }
        }
        None ->
          not_found_mutation_result(
            key,
            field,
            fragments,
            "catalogUpdate",
            "catalog",
            ["id"],
            "Catalog does not exist",
            "CATALOG_NOT_FOUND",
            store,
            identity,
          )
      }
    None ->
      not_found_mutation_result(
        key,
        field,
        fragments,
        "catalogUpdate",
        "catalog",
        ["id"],
        "Catalog does not exist",
        "CATALOG_NOT_FOUND",
        store,
        identity,
      )
  }
}

fn handle_catalog_context_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string_nonempty(args, "catalogId") {
    Some(id) ->
      case store.get_effective_catalog_by_id(store, id) {
        Some(existing) -> {
          let market_ids =
            graphql_helpers.read_arg_object(args, "contextsToAdd")
            |> option.then(read_arg_string_array(_, "marketIds"))
            |> option.unwrap([])
          let data =
            captured_object_upsert(existing.data, [
              #("markets", market_connection_from_ids(store, market_ids)),
            ])
          let #(_, next_store) =
            store.upsert_staged_catalog(
              store,
              CatalogRecord(id, existing.cursor, data),
            )
          mutation_result(
            key,
            field,
            fragments,
            "catalogContextUpdate",
            "catalog",
            data,
            [],
            next_store,
            identity,
            [id],
          )
        }
        None ->
          not_found_mutation_result(
            key,
            field,
            fragments,
            "catalogContextUpdate",
            "catalog",
            ["catalogId"],
            "Catalog does not exist",
            "CATALOG_NOT_FOUND",
            store,
            identity,
          )
      }
    None ->
      not_found_mutation_result(
        key,
        field,
        fragments,
        "catalogContextUpdate",
        "catalog",
        ["catalogId"],
        "Catalog does not exist",
        "CATALOG_NOT_FOUND",
        store,
        identity,
      )
  }
}

fn handle_catalog_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string_nonempty(args, "id") {
    Some(id) ->
      case store.get_effective_catalog_by_id(store, id) {
        Some(_) ->
          delete_result(
            key,
            field,
            fragments,
            "catalogDelete",
            id,
            store.delete_staged_catalog(store, id),
            identity,
          )
        None ->
          delete_error_result(
            key,
            field,
            fragments,
            "catalogDelete",
            ["id"],
            "Catalog does not exist",
            "CATALOG_NOT_FOUND",
            store,
            identity,
          )
      }
    None ->
      delete_error_result(
        key,
        field,
        fragments,
        "catalogDelete",
        ["id"],
        "Catalog does not exist",
        "CATALOG_NOT_FOUND",
        store,
        identity,
      )
  }
}

fn handle_price_list_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  let errors = price_list_input_errors(input, None)
  case errors {
    [] -> {
      let #(id, next_identity) =
        synthetic_identity.make_synthetic_gid(identity, "PriceList")
      let data = price_list_data(id, input, None)
      let #(_, next_store) =
        store.upsert_staged_price_list(
          store,
          PriceListRecord(id, Some(id), data),
        )
      mutation_result(
        key,
        field,
        fragments,
        "priceListCreate",
        "priceList",
        data,
        [],
        next_store,
        next_identity,
        [id],
      )
    }
    _ ->
      mutation_result(
        key,
        field,
        fragments,
        "priceListCreate",
        "priceList",
        CapturedNull,
        errors,
        store,
        identity,
        [],
      )
  }
}

fn handle_price_list_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  case graphql_helpers.read_arg_string_nonempty(args, "id") {
    Some(id) ->
      case store.get_effective_price_list_by_id(store, id) {
        Some(existing) -> {
          let errors = price_list_input_errors(input, Some(existing.data))
          case errors {
            [] -> {
              let data = price_list_data(id, input, Some(existing.data))
              let #(_, next_store) =
                store.upsert_staged_price_list(
                  store,
                  PriceListRecord(id, existing.cursor, data),
                )
              mutation_result(
                key,
                field,
                fragments,
                "priceListUpdate",
                "priceList",
                data,
                [],
                next_store,
                identity,
                [id],
              )
            }
            _ ->
              mutation_result(
                key,
                field,
                fragments,
                "priceListUpdate",
                "priceList",
                CapturedNull,
                errors,
                store,
                identity,
                [],
              )
          }
        }
        None ->
          not_found_mutation_result(
            key,
            field,
            fragments,
            "priceListUpdate",
            "priceList",
            ["id"],
            "Price list does not exist",
            "PRICE_LIST_NOT_FOUND",
            store,
            identity,
          )
      }
    None ->
      not_found_mutation_result(
        key,
        field,
        fragments,
        "priceListUpdate",
        "priceList",
        ["id"],
        "Price list does not exist",
        "PRICE_LIST_NOT_FOUND",
        store,
        identity,
      )
  }
}

fn handle_price_list_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string_nonempty(args, "id") {
    Some(id) ->
      case store.get_effective_price_list_by_id(store, id) {
        Some(existing) -> {
          let next_store = store.delete_staged_price_list(store, id)
          let payload =
            CapturedObject([
              #("deletedId", CapturedString(id)),
              #("priceList", existing.data),
              #("userErrors", CapturedArray([])),
            ])
          MutationFieldResult(
            key: key,
            payload: project_record(
              field,
              fragments,
              captured_json_source(payload),
            ),
            store: next_store,
            identity: identity,
            staged_resource_ids: [],
            log_drafts: [markets_log_draft("priceListDelete", [id])],
          )
        }
        None ->
          delete_error_result(
            key,
            field,
            fragments,
            "priceListDelete",
            ["id"],
            "Price list does not exist",
            "PRICE_LIST_NOT_FOUND",
            store,
            identity,
          )
      }
    None ->
      delete_error_result(
        key,
        field,
        fragments,
        "priceListDelete",
        ["id"],
        "Price list does not exist",
        "PRICE_LIST_NOT_FOUND",
        store,
        identity,
      )
  }
}

fn handle_price_list_fixed_prices_add(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let price_list_id = read_price_list_id(args)
  let price_list =
    option.then(price_list_id, store.get_effective_price_list_by_id(store, _))
  let price_inputs = read_arg_object_array(args, "prices")
  let errors =
    price_list_fixed_price_target_errors(price_list_id, price_list)
    |> list.append(case price_list {
      Some(existing) ->
        fixed_price_input_errors(store, existing, price_inputs, "prices")
      None -> []
    })

  case price_list, errors {
    Some(existing), [] -> {
      let updated = upsert_fixed_price_nodes(existing, store, price_inputs)
      let #(_, next_store) = store.upsert_staged_price_list(store, updated)
      fixed_prices_payload_result(
        key,
        field,
        fragments,
        "priceListFixedPricesAdd",
        updated,
        mutation_variant_ids(price_inputs),
        [],
        next_store,
        identity,
      )
    }
    _, _ ->
      fixed_prices_error_result(
        key,
        field,
        fragments,
        "priceListFixedPricesAdd",
        errors,
        store,
        identity,
      )
  }
}

fn handle_price_list_fixed_prices_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let price_list_id = read_price_list_id(args)
  let price_list =
    option.then(price_list_id, store.get_effective_price_list_by_id(store, _))
  let #(price_inputs, price_input_field) = read_fixed_price_update_inputs(args)
  let delete_variant_ids =
    read_arg_string_array(args, "variantIdsToDelete") |> option.unwrap([])
  let errors =
    price_list_fixed_price_target_errors(price_list_id, price_list)
    |> list.append(case price_list {
      Some(existing) ->
        fixed_price_input_errors(
          store,
          existing,
          price_inputs,
          price_input_field,
        )
        |> list.append(fixed_price_not_fixed_errors(
          existing,
          price_inputs,
          price_input_field,
        ))
        |> list.append(fixed_price_delete_variant_errors(
          store,
          delete_variant_ids,
          "variantIdsToDelete",
        ))
      None -> []
    })

  case price_list, errors {
    Some(existing), [] -> {
      let deleted_variant_ids =
        fixed_price_variant_ids_in_request_order(existing, delete_variant_ids)
      let updated =
        existing
        |> upsert_fixed_price_nodes(store, price_inputs)
        |> delete_fixed_price_nodes(delete_variant_ids)
      let #(_, next_store) = store.upsert_staged_price_list(store, updated)
      let changed_variant_ids =
        mutation_variant_ids(price_inputs)
        |> append_unique_strings(deleted_variant_ids)
      fixed_prices_payload_result(
        key,
        field,
        fragments,
        "priceListFixedPricesUpdate",
        updated,
        changed_variant_ids,
        deleted_variant_ids,
        next_store,
        identity,
      )
    }
    _, _ ->
      fixed_prices_error_result(
        key,
        field,
        fragments,
        "priceListFixedPricesUpdate",
        errors,
        store,
        identity,
      )
  }
}

fn handle_price_list_fixed_prices_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let price_list_id = read_price_list_id(args)
  let price_list =
    option.then(price_list_id, store.get_effective_price_list_by_id(store, _))
  let variant_ids =
    read_arg_string_array(args, "variantIds") |> option.unwrap([])
  let errors =
    price_list_fixed_price_target_errors(price_list_id, price_list)
    |> list.append(case price_list {
      Some(_) ->
        fixed_price_delete_variant_errors(store, variant_ids, "variantIds")
      None -> []
    })

  case price_list, errors {
    Some(existing), [] -> {
      let deleted_variant_ids =
        fixed_price_variant_ids_in_request_order(existing, variant_ids)
      let updated = delete_fixed_price_nodes(existing, variant_ids)
      let #(_, next_store) = store.upsert_staged_price_list(store, updated)
      fixed_prices_payload_result(
        key,
        field,
        fragments,
        "priceListFixedPricesDelete",
        updated,
        [],
        deleted_variant_ids,
        next_store,
        identity,
      )
    }
    _, _ ->
      fixed_prices_error_result(
        key,
        field,
        fragments,
        "priceListFixedPricesDelete",
        errors,
        store,
        identity,
      )
  }
}

fn handle_price_list_fixed_prices_by_product_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let price_list_id = read_price_list_id(args)
  let price_list =
    option.then(price_list_id, store.get_effective_price_list_by_id(store, _))
  let price_inputs = read_arg_object_array(args, "pricesToAdd")
  let delete_product_ids =
    read_arg_string_array(args, "pricesToDeleteByProductIds")
    |> option.unwrap([])
  let errors =
    case price_list_id, price_list {
      Some(_), Some(_) -> []
      _, _ -> [
        user_error(
          ["priceListId"],
          "Price list does not exist.",
          "PRICE_LIST_DOES_NOT_EXIST",
        ),
      ]
    }
    |> list.append(product_level_fixed_price_errors(
      store,
      price_inputs,
      delete_product_ids,
    ))

  case price_list, errors {
    Some(existing), [] -> {
      let added_product_ids =
        list.filter_map(price_inputs, fn(input) {
          graphql_helpers.read_arg_string_nonempty(input, "productId")
          |> option_to_result
        })
      let fixed_inputs =
        list.flat_map(price_inputs, fn(input) {
          case graphql_helpers.read_arg_string_nonempty(input, "productId") {
            Some(product_id) ->
              store.get_effective_variants_by_product_id(store, product_id)
              |> list.map(fn(variant) {
                dict.insert(
                  input,
                  "variantId",
                  root_field.StringVal(variant.id),
                )
              })
            None -> []
          }
        })
      let delete_variant_ids =
        delete_product_ids
        |> list.flat_map(fn(product_id) {
          store.get_effective_variants_by_product_id(store, product_id)
          |> list.map(fn(variant) { variant.id })
        })
      let updated =
        existing
        |> upsert_fixed_price_nodes(store, fixed_inputs)
        |> delete_fixed_price_nodes(delete_variant_ids)
      let #(_, next_store) = store.upsert_staged_price_list(store, updated)
      let payload =
        CapturedObject([
          #("priceList", updated.data),
          #("pricesToAddProducts", product_payloads(store, added_product_ids)),
          #(
            "pricesToDeleteProducts",
            product_payloads(store, delete_product_ids),
          ),
          #("fixedPriceVariantIds", CapturedArray([])),
          #("deletedFixedPriceVariantIds", CapturedArray([])),
          #("userErrors", CapturedArray([])),
        ])
      MutationFieldResult(
        key: key,
        payload: project_record(field, fragments, captured_json_source(payload)),
        store: next_store,
        identity: identity,
        staged_resource_ids: [existing.id],
        log_drafts: [
          markets_log_draft("priceListFixedPricesByProductUpdate", [existing.id]),
        ],
      )
    }
    _, _ ->
      mutation_payload_result(
        key,
        field,
        fragments,
        "priceListFixedPricesByProductUpdate",
        CapturedObject([
          #("priceList", CapturedNull),
          #("pricesToAddProducts", CapturedNull),
          #("pricesToDeleteProducts", CapturedNull),
          #("userErrors", CapturedArray(errors)),
        ]),
        store,
        identity,
        [],
      )
  }
}

fn read_fixed_price_update_inputs(
  args: Dict(String, root_field.ResolvedValue),
) -> #(List(Dict(String, root_field.ResolvedValue)), String) {
  let prices = read_arg_object_array(args, "prices")
  case prices {
    [] -> #(read_arg_object_array(args, "pricesToAdd"), "pricesToAdd")
    _ -> #(prices, "prices")
  }
}

fn price_list_fixed_price_target_errors(
  price_list_id: Option(String),
  price_list: Option(PriceListRecord),
) -> List(CapturedJsonValue) {
  case price_list_id, price_list {
    Some(_), Some(_) -> []
    _, _ -> [
      price_list_price_user_error(
        ["priceListId"],
        "Price list not found.",
        "PRICE_LIST_NOT_FOUND",
      ),
    ]
  }
}

fn fixed_price_input_errors(
  store: Store,
  price_list: PriceListRecord,
  inputs: List(Dict(String, root_field.ResolvedValue)),
  field_name: String,
) -> List(CapturedJsonValue) {
  fixed_price_variant_errors(store, inputs, field_name)
  |> list.append(fixed_price_currency_errors(price_list, inputs, field_name))
  |> list.append(fixed_price_duplicate_errors(inputs, field_name))
}

fn fixed_price_variant_errors(
  store: Store,
  inputs: List(Dict(String, root_field.ResolvedValue)),
  field_name: String,
) -> List(CapturedJsonValue) {
  inputs
  |> enumerate_dicts
  |> list.filter_map(fn(entry) {
    let #(input, index) = entry
    let variant_id =
      graphql_helpers.read_arg_string_nonempty(input, "variantId")
      |> option.unwrap("")
    case store.get_effective_variant_by_id(store, variant_id) {
      Some(_) -> Error(Nil)
      None ->
        Ok(price_list_price_user_error(
          [field_name, int.to_string(index), "variantId"],
          "Variant not found.",
          "VARIANT_NOT_FOUND",
        ))
    }
  })
}

fn fixed_price_delete_variant_errors(
  store: Store,
  variant_ids: List(String),
  field_name: String,
) -> List(CapturedJsonValue) {
  variant_ids
  |> enumerate_strings
  |> list.filter_map(fn(entry) {
    let #(variant_id, index) = entry
    case store.get_effective_variant_by_id(store, variant_id) {
      Some(_) -> Error(Nil)
      None ->
        Ok(price_list_price_user_error(
          [field_name, int.to_string(index)],
          "Variant not found.",
          "VARIANT_NOT_FOUND",
        ))
    }
  })
}

fn fixed_price_currency_errors(
  price_list: PriceListRecord,
  inputs: List(Dict(String, root_field.ResolvedValue)),
  field_name: String,
) -> List(CapturedJsonValue) {
  let expected_currency = price_list_currency(price_list)
  inputs
  |> enumerate_dicts
  |> list.filter_map(fn(entry) {
    let #(input, index) = entry
    let currency =
      graphql_helpers.read_arg_object(input, "price")
      |> option.then(graphql_helpers.read_arg_string_nonempty(_, "currencyCode"))
    case currency {
      Some(actual) ->
        case actual == expected_currency {
          True -> Error(Nil)
          False ->
            Ok(price_list_price_user_error(
              [field_name, int.to_string(index), "price", "currencyCode"],
              "Currency must match price list currency.",
              "PRICES_TO_ADD_CURRENCY_MISMATCH",
            ))
        }
      None -> Error(Nil)
    }
  })
}

fn fixed_price_duplicate_errors(
  inputs: List(Dict(String, root_field.ResolvedValue)),
  field_name: String,
) -> List(CapturedJsonValue) {
  let #(_, errors) =
    inputs
    |> enumerate_dicts
    |> list.fold(#([], []), fn(acc, entry) {
      let #(seen_ids, current_errors) = acc
      let #(input, index) = entry
      case graphql_helpers.read_arg_string_nonempty(input, "variantId") {
        Some(variant_id) ->
          case list.contains(seen_ids, variant_id) {
            True -> #(
              seen_ids,
              list.append(current_errors, [
                price_list_price_user_error(
                  [field_name, int.to_string(index), "variantId"],
                  "Duplicate variant ID in input.",
                  "DUPLICATE_ID_IN_INPUT",
                ),
              ]),
            )
            False -> #(list.append(seen_ids, [variant_id]), current_errors)
          }
        None -> #(seen_ids, current_errors)
      }
    })
  errors
}

fn fixed_price_not_fixed_errors(
  price_list: PriceListRecord,
  inputs: List(Dict(String, root_field.ResolvedValue)),
  field_name: String,
) -> List(CapturedJsonValue) {
  let fixed_variant_ids = fixed_price_variant_ids(price_list)
  inputs
  |> enumerate_dicts
  |> list.filter_map(fn(entry) {
    let #(input, index) = entry
    case graphql_helpers.read_arg_string_nonempty(input, "variantId") {
      Some(variant_id) ->
        case list.contains(fixed_variant_ids, variant_id) {
          True -> Error(Nil)
          False ->
            Ok(price_list_price_user_error(
              [field_name, int.to_string(index), "variantId"],
              "Price is not fixed.",
              "PRICE_NOT_FIXED",
            ))
        }
      None -> Error(Nil)
    }
  })
}

fn fixed_price_variant_ids(price_list: PriceListRecord) -> List(String) {
  price_edges(price_list.data)
  |> list.filter_map(fn(edge) {
    fixed_price_edge_variant_id(edge) |> option_to_result
  })
}

fn fixed_price_variant_ids_in_request_order(
  price_list: PriceListRecord,
  variant_ids: List(String),
) -> List(String) {
  let fixed_variant_ids = fixed_price_variant_ids(price_list)
  variant_ids
  |> list.filter(fn(variant_id) { list.contains(fixed_variant_ids, variant_id) })
}

fn fixed_prices_payload_result(
  key: String,
  field: Selection,
  fragments: FragmentMap,
  root_name: String,
  price_list: PriceListRecord,
  fixed_variant_ids: List(String),
  deleted_variant_ids: List(String),
  store: Store,
  identity: SyntheticIdentityRegistry,
) -> MutationFieldResult {
  let payload = case root_name {
    "priceListFixedPricesAdd" ->
      CapturedObject([
        #(
          "prices",
          fixed_price_nodes_for_variant_ids(price_list, fixed_variant_ids),
        ),
        #("userErrors", CapturedArray([])),
      ])
    "priceListFixedPricesUpdate" ->
      CapturedObject([
        #("priceList", price_list.data),
        #(
          "pricesAdded",
          fixed_price_nodes_for_variant_ids(price_list, fixed_variant_ids),
        ),
        #("deletedFixedPriceVariantIds", string_array(deleted_variant_ids)),
        #("userErrors", CapturedArray([])),
      ])
    "priceListFixedPricesDelete" ->
      CapturedObject([
        #("deletedFixedPriceVariantIds", string_array(deleted_variant_ids)),
        #("userErrors", CapturedArray([])),
      ])
    _ ->
      CapturedObject([
        #("priceList", price_list.data),
        #("userErrors", CapturedArray([])),
      ])
  }
  MutationFieldResult(
    key: key,
    payload: project_record(field, fragments, captured_json_source(payload)),
    store: store,
    identity: identity,
    staged_resource_ids: [price_list.id],
    log_drafts: [markets_log_draft(root_name, [price_list.id])],
  )
}

fn fixed_price_nodes_for_variant_ids(
  price_list: PriceListRecord,
  variant_ids: List(String),
) -> CapturedJsonValue {
  CapturedArray(
    price_edges(price_list.data)
    |> list.filter_map(fn(edge) {
      use variant_id <- result.try(
        fixed_price_edge_variant_id(edge) |> option_to_result,
      )
      case list.contains(variant_ids, variant_id) {
        True -> captured_field(edge, "node") |> option_to_result
        False -> Error(Nil)
      }
    }),
  )
}

fn fixed_prices_error_result(
  key: String,
  field: Selection,
  fragments: FragmentMap,
  root_name: String,
  errors: List(CapturedJsonValue),
  store: Store,
  identity: SyntheticIdentityRegistry,
) -> MutationFieldResult {
  mutation_payload_result(
    key,
    field,
    fragments,
    root_name,
    fixed_prices_error_payload(root_name, errors),
    store,
    identity,
    [],
  )
}

fn fixed_prices_error_payload(
  root_name: String,
  errors: List(CapturedJsonValue),
) -> CapturedJsonValue {
  case root_name {
    "priceListFixedPricesAdd" ->
      CapturedObject([
        #("prices", CapturedNull),
        #("userErrors", CapturedArray(errors)),
      ])
    "priceListFixedPricesUpdate" ->
      CapturedObject([
        #("priceList", CapturedNull),
        #("pricesAdded", CapturedNull),
        #("deletedFixedPriceVariantIds", CapturedNull),
        #("userErrors", CapturedArray(errors)),
      ])
    "priceListFixedPricesDelete" ->
      CapturedObject([
        #("deletedFixedPriceVariantIds", CapturedNull),
        #("userErrors", CapturedArray(errors)),
      ])
    _ -> CapturedObject([#("userErrors", CapturedArray(errors))])
  }
}

fn price_list_price_user_error(
  field: List(String),
  message: String,
  code: String,
) -> CapturedJsonValue {
  user_error_with_typename(
    field,
    message,
    code,
    Some("PriceListPriceUserError"),
  )
}

fn handle_quantity_pricing_by_variant_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  let price_list_id = read_price_list_id(args)
  let price_list =
    option.then(price_list_id, store.get_effective_price_list_by_id(store, _))
  let errors = case price_list {
    Some(_) -> quantity_pricing_input_errors(store, input)
    None -> [
      user_error(
        ["priceListId"],
        "Price list not found.",
        "PRICE_LIST_NOT_FOUND",
      ),
    ]
  }
  case price_list, errors {
    Some(existing), [] -> {
      let fixed_inputs = read_arg_object_array(input, "pricesToAdd")
      let delete_variant_ids =
        read_arg_string_array(input, "pricesToDeleteByVariantId")
        |> option.unwrap([])
      let rule_inputs = read_arg_object_array(input, "quantityRulesToAdd")
      let rule_delete_ids =
        read_arg_string_array(input, "quantityRulesToDeleteByVariantId")
        |> option.unwrap([])
      let price_break_inputs =
        read_arg_object_array(input, "quantityPriceBreaksToAdd")
      let updated =
        existing
        |> upsert_fixed_price_nodes(store, fixed_inputs)
        |> delete_fixed_price_nodes(delete_variant_ids)
        |> upsert_quantity_rule_nodes(store, rule_inputs)
        |> delete_quantity_rule_nodes(rule_delete_ids)
        |> upsert_quantity_price_break_nodes(
          store,
          identity,
          price_break_inputs,
        )
      let #(_, next_store) = store.upsert_staged_price_list(store, updated)
      let changed_variant_ids =
        mutation_variant_ids(fixed_inputs)
        |> append_unique_strings(mutation_variant_ids(rule_inputs))
        |> append_unique_strings(mutation_variant_ids(price_break_inputs))
        |> append_unique_strings(delete_variant_ids)
        |> append_unique_strings(rule_delete_ids)
      let payload =
        CapturedObject([
          #("productVariants", variant_payloads(store, changed_variant_ids)),
          #("userErrors", CapturedArray([])),
        ])
      MutationFieldResult(
        key: key,
        payload: project_record(field, fragments, captured_json_source(payload)),
        store: next_store,
        identity: identity,
        staged_resource_ids: [existing.id],
        log_drafts: [
          markets_log_draft("quantityPricingByVariantUpdate", [existing.id]),
        ],
      )
    }
    _, _ ->
      mutation_payload_result(
        key,
        field,
        fragments,
        "quantityPricingByVariantUpdate",
        CapturedObject([
          #("productVariants", CapturedNull),
          #("userErrors", CapturedArray(errors)),
        ]),
        store,
        identity,
        [],
      )
  }
}

fn handle_quantity_rules_add(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let price_list =
    option.then(read_price_list_id(args), store.get_effective_price_list_by_id(
      store,
      _,
    ))
  let inputs = read_arg_object_array(args, "quantityRules")
  let errors = case price_list {
    Some(_) -> quantity_rules_input_errors(store, inputs)
    None -> [
      quantity_rule_user_error(
        ["priceListId"],
        "Price list does not exist.",
        "PRICE_LIST_DOES_NOT_EXIST",
      ),
    ]
  }
  case price_list, errors {
    Some(existing), [] -> {
      let updated = upsert_quantity_rule_nodes(existing, store, inputs)
      let #(_, next_store) = store.upsert_staged_price_list(store, updated)
      let payload =
        CapturedObject([
          #("quantityRules", quantity_rule_payloads(store, inputs)),
          #("userErrors", CapturedArray([])),
        ])
      mutation_payload_result(
        key,
        field,
        fragments,
        "quantityRulesAdd",
        payload,
        next_store,
        identity,
        [existing.id],
      )
    }
    _, _ ->
      mutation_payload_result(
        key,
        field,
        fragments,
        "quantityRulesAdd",
        CapturedObject([
          #("quantityRules", CapturedArray([])),
          #("userErrors", CapturedArray(errors)),
        ]),
        store,
        identity,
        [],
      )
  }
}

fn handle_quantity_rules_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let price_list =
    option.then(read_price_list_id(args), store.get_effective_price_list_by_id(
      store,
      _,
    ))
  let variant_ids =
    read_arg_string_array(args, "variantIds") |> option.unwrap([])
  let errors = case price_list {
    Some(_) -> quantity_rule_delete_errors(store, variant_ids)
    None -> [
      user_error(
        ["priceListId"],
        "Price list does not exist.",
        "PRICE_LIST_DOES_NOT_EXIST",
      ),
    ]
  }
  case price_list, errors {
    Some(existing), [] -> {
      let updated = delete_quantity_rule_nodes(existing, variant_ids)
      let #(_, next_store) = store.upsert_staged_price_list(store, updated)
      mutation_payload_result(
        key,
        field,
        fragments,
        "quantityRulesDelete",
        CapturedObject([
          #("deletedQuantityRulesVariantIds", string_array(variant_ids)),
          #("userErrors", CapturedArray([])),
        ]),
        next_store,
        identity,
        [existing.id],
      )
    }
    _, _ ->
      mutation_payload_result(
        key,
        field,
        fragments,
        "quantityRulesDelete",
        CapturedObject([
          #("deletedQuantityRulesVariantIds", CapturedArray([])),
          #("userErrors", CapturedArray(errors)),
        ]),
        store,
        identity,
        [],
      )
  }
}

fn handle_market_localizations_register(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let resource_id = graphql_helpers.read_arg_string_nonempty(args, "resourceId")
  let inputs = read_arg_object_array(args, "marketLocalizations")
  case list.length(inputs) > 100 {
    True ->
      mutation_payload_result(
        key,
        field,
        fragments,
        "marketLocalizationsRegister",
        CapturedObject([
          #("marketLocalizations", CapturedNull),
          #(
            "userErrors",
            CapturedArray([
              translation_user_error(
                ["resourceId"],
                "Too many keys for resource - maximum 100 per mutation",
                "TOO_MANY_KEYS_FOR_RESOURCE",
              ),
            ]),
          ),
        ]),
        store,
        identity,
        [],
      )
    False ->
      case resource_id {
        Some(id) ->
          case store.find_effective_metafield_by_id(store, id) {
            Some(metafield) -> {
              let errors =
                market_localizations_register_errors(store, metafield, inputs)
              case errors {
                [] -> {
                  let #(timestamp, next_identity) =
                    synthetic_identity.make_synthetic_timestamp(identity)
                  let records =
                    inputs
                    |> list.filter_map(fn(input) {
                      register_input_to_record(input, id, timestamp)
                    })
                  let next_store =
                    store.upsert_staged_market_localizations(store, records)
                  mutation_payload_result(
                    key,
                    field,
                    fragments,
                    "marketLocalizationsRegister",
                    CapturedObject([
                      #(
                        "marketLocalizations",
                        CapturedArray(
                          list.map(records, fn(record) {
                            market_localization_payload(store, record)
                          }),
                        ),
                      ),
                      #("userErrors", CapturedArray([])),
                    ]),
                    next_store,
                    next_identity,
                    [id],
                  )
                }
                _ ->
                  mutation_payload_result(
                    key,
                    field,
                    fragments,
                    "marketLocalizationsRegister",
                    CapturedObject([
                      #("marketLocalizations", CapturedNull),
                      #("userErrors", CapturedArray(errors)),
                    ]),
                    store,
                    identity,
                    [],
                  )
              }
            }
            None ->
              market_localizations_register_error_result(
                key,
                field,
                fragments,
                [translation_resource_not_found_error(id)],
                store,
                identity,
              )
          }
        None ->
          market_localizations_register_error_result(
            key,
            field,
            fragments,
            [translation_resource_not_found_error("")],
            store,
            identity,
          )
      }
  }
}

fn market_localizations_register_error_result(
  key: String,
  field: Selection,
  fragments: FragmentMap,
  errors: List(CapturedJsonValue),
  store: Store,
  identity: SyntheticIdentityRegistry,
) -> MutationFieldResult {
  mutation_payload_result(
    key,
    field,
    fragments,
    "marketLocalizationsRegister",
    CapturedObject([
      #("marketLocalizations", CapturedNull),
      #("userErrors", CapturedArray(errors)),
    ]),
    store,
    identity,
    [],
  )
}

fn market_localizations_register_errors(
  store: Store,
  metafield: ProductMetafieldRecord,
  inputs: List(Dict(String, root_field.ResolvedValue)),
) -> List(CapturedJsonValue) {
  inputs
  |> enumerate_dicts
  |> list.flat_map(fn(pair) {
    let #(input, index) = pair
    market_localization_register_input_errors(store, metafield, input, index)
  })
}

fn market_localization_register_input_errors(
  store: Store,
  metafield: ProductMetafieldRecord,
  input: Dict(String, root_field.ResolvedValue),
  index: Int,
) -> List(CapturedJsonValue) {
  let prefix = ["marketLocalizations", int.to_string(index)]
  let market_errors = market_localization_market_errors(store, input, prefix)
  let key =
    graphql_helpers.read_arg_string_nonempty(input, "key") |> option.unwrap("")
  let content = market_localizable_content_for_key(metafield, key)
  let key_errors = case content {
    Some(_) -> []
    None -> [
      translation_user_error(
        list.append(prefix, ["key"]),
        "Key " <> key <> " is not a valid market localizable field",
        "INVALID_KEY_FOR_MODEL",
      ),
    ]
  }
  let digest_errors = case content, key_errors {
    Some(item), [] -> market_localization_digest_errors(input, prefix, item)
    _, _ -> []
  }
  let value_errors = case content, key_errors, digest_errors {
    Some(_), [], [] -> market_localization_value_errors(input, prefix)
    _, _, _ -> []
  }
  market_errors
  |> list.append(key_errors)
  |> list.append(digest_errors)
  |> list.append(value_errors)
}

fn market_localization_market_errors(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
  prefix: List(String),
) -> List(CapturedJsonValue) {
  case graphql_helpers.read_arg_string_nonempty(input, "marketId") {
    Some(id) ->
      case store.list_effective_markets(store) {
        [] -> []
        _ ->
          case store.get_effective_market_by_id(store, id) {
            Some(_) -> []
            None -> [
              translation_user_error(
                list.append(prefix, ["marketId"]),
                "Market does not exist",
                "MARKET_DOES_NOT_EXIST",
              ),
            ]
          }
      }
    None -> [
      translation_user_error(
        list.append(prefix, ["marketId"]),
        "Market does not exist",
        "MARKET_DOES_NOT_EXIST",
      ),
    ]
  }
}

fn market_localizable_content_for_key(
  metafield: ProductMetafieldRecord,
  key: String,
) -> Option(MarketLocalizableContentRecord) {
  list.find(metafield.market_localizable_content, fn(content) {
    content.key == key
  })
  |> result_to_option
}

fn market_localization_digest_errors(
  input: Dict(String, root_field.ResolvedValue),
  prefix: List(String),
  content: MarketLocalizableContentRecord,
) -> List(CapturedJsonValue) {
  case read_market_localizable_content_digest(input) {
    Some(digest) if digest == content.digest -> []
    _ -> [
      translation_user_error(
        list.append(prefix, ["marketLocalizableContentDigest"]),
        "Market localizable content is invalid",
        "INVALID_MARKET_LOCALIZABLE_CONTENT",
      ),
    ]
  }
}

fn read_market_localizable_content_digest(
  input: Dict(String, root_field.ResolvedValue),
) -> Option(String) {
  case
    graphql_helpers.read_arg_string_nonempty(
      input,
      "marketLocalizableContentDigest",
    )
  {
    Some(digest) -> Some(digest)
    None ->
      graphql_helpers.read_arg_string_nonempty(
        input,
        "translatableContentDigest",
      )
  }
}

fn market_localization_value_errors(
  input: Dict(String, root_field.ResolvedValue),
  prefix: List(String),
) -> List(CapturedJsonValue) {
  case read_arg_string_allow_empty(input, "value") {
    Some(value) ->
      case string.trim(value) {
        "" -> [
          translation_user_error(
            list.append(prefix, ["value"]),
            "Value is invalid",
            "FAILS_RESOURCE_VALIDATION",
          ),
        ]
        _ -> []
      }
    _ -> [
      translation_user_error(
        list.append(prefix, ["value"]),
        "Value is invalid",
        "FAILS_RESOURCE_VALIDATION",
      ),
    ]
  }
}

fn register_input_to_record(
  input: Dict(String, root_field.ResolvedValue),
  resource_id: String,
  updated_at: String,
) -> Result(MarketLocalizationRecord, Nil) {
  use market_id <- result.try(
    graphql_helpers.read_arg_string_nonempty(input, "marketId")
    |> option_to_result,
  )
  use key <- result.try(
    graphql_helpers.read_arg_string_nonempty(input, "key") |> option_to_result,
  )
  use value <- result.try(
    read_arg_string_allow_empty(input, "value") |> option_to_result,
  )
  Ok(MarketLocalizationRecord(
    resource_id: resource_id,
    market_id: market_id,
    key: key,
    value: value,
    updated_at: updated_at,
    outdated: False,
  ))
}

fn handle_market_localizations_remove(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let resource_id = graphql_helpers.read_arg_string_nonempty(args, "resourceId")
  let errors = case resource_id {
    Some(id) ->
      case store.find_effective_metafield_by_id(store, id) {
        Some(_) -> []
        None -> [resource_not_found_error(id)]
      }
    None -> [resource_not_found_error("")]
  }
  mutation_payload_result(
    key,
    field,
    fragments,
    "marketLocalizationsRemove",
    CapturedObject([
      #("marketLocalizations", CapturedNull),
      #("userErrors", CapturedArray(errors)),
    ]),
    store,
    identity,
    [],
  )
}

fn resource_not_found_error(resource_id: String) -> CapturedJsonValue {
  user_error(
    ["resourceId"],
    "Resource " <> resource_id <> " does not exist",
    "RESOURCE_NOT_FOUND",
  )
}

fn translation_resource_not_found_error(
  resource_id: String,
) -> CapturedJsonValue {
  translation_user_error(
    ["resourceId"],
    "Resource " <> resource_id <> " does not exist",
    "RESOURCE_NOT_FOUND",
  )
}

fn handle_web_presence_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  let errors = web_presence_create_errors(store, input)
  case errors {
    [] -> {
      let #(id, next_identity) =
        synthetic_identity.make_synthetic_gid(identity, "MarketWebPresence")
      let data = web_presence_data(store, id, input)
      let record = WebPresenceRecord(id: id, cursor: None, data: data)
      let #(_, next_store) = store.upsert_staged_web_presence(store, record)
      mutation_result(
        key,
        field,
        fragments,
        "webPresenceCreate",
        "webPresence",
        data,
        [],
        next_store,
        next_identity,
        [id],
      )
    }
    _ ->
      mutation_result(
        key,
        field,
        fragments,
        "webPresenceCreate",
        "webPresence",
        CapturedNull,
        errors,
        store,
        identity,
        [],
      )
  }
}

fn handle_web_presence_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let id = graphql_helpers.read_arg_string_nonempty(args, "id")
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  case id {
    Some(id_value) ->
      case store.get_effective_web_presence_by_id(store, id_value) {
        Some(_) -> {
          let errors = web_presence_update_errors(store, input)
          case errors {
            [] -> {
              let data = web_presence_data(store, id_value, input)
              let record =
                WebPresenceRecord(id: id_value, cursor: None, data: data)
              let #(_, next_store) =
                store.upsert_staged_web_presence(store, record)
              mutation_result(
                key,
                field,
                fragments,
                "webPresenceUpdate",
                "webPresence",
                data,
                [],
                next_store,
                identity,
                [id_value],
              )
            }
            _ ->
              mutation_result(
                key,
                field,
                fragments,
                "webPresenceUpdate",
                "webPresence",
                CapturedNull,
                errors,
                store,
                identity,
                [],
              )
          }
        }
        None ->
          web_presence_not_found_result(
            key,
            field,
            fragments,
            "webPresenceUpdate",
            "webPresence",
            store,
            identity,
          )
      }
    None ->
      web_presence_not_found_result(
        key,
        field,
        fragments,
        "webPresenceUpdate",
        "webPresence",
        store,
        identity,
      )
  }
}

fn handle_web_presence_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string_nonempty(args, "id") {
    Some(id) ->
      case store.get_effective_web_presence_by_id(store, id) {
        Some(_) -> {
          let next_store = store.delete_staged_web_presence(store, id)
          let payload =
            CapturedObject([
              #("deletedId", CapturedString(id)),
              #("userErrors", CapturedArray([])),
            ])
          let staged_ids: List(String) = []
          MutationFieldResult(
            key: key,
            payload: project_record(
              field,
              fragments,
              captured_json_source(payload),
            ),
            store: next_store,
            identity: identity,
            staged_resource_ids: staged_ids,
            log_drafts: [markets_log_draft("webPresenceDelete", staged_ids)],
          )
        }
        None ->
          web_presence_delete_not_found_result(
            key,
            field,
            fragments,
            store,
            identity,
          )
      }
    None ->
      web_presence_delete_not_found_result(
        key,
        field,
        fragments,
        store,
        identity,
      )
  }
}

fn mutation_result(
  key: String,
  field: Selection,
  fragments: FragmentMap,
  root_name: String,
  resource_key: String,
  resource: CapturedJsonValue,
  user_errors: List(CapturedJsonValue),
  store: Store,
  identity: SyntheticIdentityRegistry,
  staged_ids: List(String),
) -> MutationFieldResult {
  let payload =
    CapturedObject([
      #(resource_key, resource),
      #("userErrors", CapturedArray(user_errors)),
    ])
  MutationFieldResult(
    key: key,
    payload: project_record(field, fragments, captured_json_source(payload)),
    store: store,
    identity: identity,
    staged_resource_ids: staged_ids,
    log_drafts: [markets_log_draft(root_name, staged_ids)],
  )
}

fn mutation_payload_result(
  key: String,
  field: Selection,
  fragments: FragmentMap,
  root_name: String,
  payload: CapturedJsonValue,
  store: Store,
  identity: SyntheticIdentityRegistry,
  staged_ids: List(String),
) -> MutationFieldResult {
  MutationFieldResult(
    key: key,
    payload: project_record(field, fragments, captured_json_source(payload)),
    store: store,
    identity: identity,
    staged_resource_ids: staged_ids,
    log_drafts: [markets_log_draft(root_name, staged_ids)],
  )
}

fn not_found_mutation_result(
  key: String,
  field: Selection,
  fragments: FragmentMap,
  root_name: String,
  resource_key: String,
  error_field: List(String),
  message: String,
  code: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
) -> MutationFieldResult {
  mutation_result(
    key,
    field,
    fragments,
    root_name,
    resource_key,
    CapturedNull,
    [user_error(error_field, message, code)],
    store,
    identity,
    [],
  )
}

fn delete_result(
  key: String,
  field: Selection,
  fragments: FragmentMap,
  root_name: String,
  id: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
) -> MutationFieldResult {
  mutation_payload_result(
    key,
    field,
    fragments,
    root_name,
    CapturedObject([
      #("deletedId", CapturedString(id)),
      #("userErrors", CapturedArray([])),
    ]),
    store,
    identity,
    [id],
  )
}

fn delete_error_result(
  key: String,
  field: Selection,
  fragments: FragmentMap,
  root_name: String,
  error_field: List(String),
  message: String,
  code: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
) -> MutationFieldResult {
  mutation_payload_result(
    key,
    field,
    fragments,
    root_name,
    CapturedObject([
      #("deletedId", CapturedNull),
      #("userErrors", CapturedArray([user_error(error_field, message, code)])),
    ]),
    store,
    identity,
    [],
  )
}

fn web_presence_not_found_result(
  key: String,
  field: Selection,
  fragments: FragmentMap,
  root_name: String,
  resource_key: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
) -> MutationFieldResult {
  mutation_result(
    key,
    field,
    fragments,
    root_name,
    resource_key,
    CapturedNull,
    [
      user_error(
        ["id"],
        "The market web presence wasn't found.",
        "WEB_PRESENCE_NOT_FOUND",
      ),
    ],
    store,
    identity,
    [],
  )
}

fn web_presence_delete_not_found_result(
  key: String,
  field: Selection,
  fragments: FragmentMap,
  store: Store,
  identity: SyntheticIdentityRegistry,
) -> MutationFieldResult {
  let staged_ids: List(String) = []
  let payload =
    CapturedObject([
      #("deletedId", CapturedNull),
      #(
        "userErrors",
        CapturedArray([
          user_error(
            ["id"],
            "The market web presence wasn't found.",
            "WEB_PRESENCE_NOT_FOUND",
          ),
        ]),
      ),
    ])
  MutationFieldResult(
    key: key,
    payload: project_record(field, fragments, captured_json_source(payload)),
    store: store,
    identity: identity,
    staged_resource_ids: staged_ids,
    log_drafts: [markets_log_draft("webPresenceDelete", staged_ids)],
  )
}

fn web_presence_create_errors(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
) -> List(CapturedJsonValue) {
  web_presence_input_errors(store, input, True)
}

fn web_presence_update_errors(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
) -> List(CapturedJsonValue) {
  web_presence_input_errors(store, input, False)
}

fn web_presence_input_errors(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
  require_route: Bool,
) -> List(CapturedJsonValue) {
  let domain_errors = case
    graphql_helpers.read_arg_string_nonempty(input, "domainId")
  {
    Some(domain_id) ->
      case web_presence_domain_for_id(store, domain_id) {
        Some(_) -> []
        None -> [
          user_error(
            ["input", "domainId"],
            "Domain does not exist",
            "DOMAIN_NOT_FOUND",
          ),
        ]
      }
    None -> []
  }
  let default_locale_errors = web_presence_default_locale_errors(input)
  let route_and_suffix_errors = case domain_errors, default_locale_errors {
    [], [] ->
      list.append(
        web_presence_route_errors(input, require_route),
        web_presence_subfolder_suffix_errors(input),
      )
    _, _ -> []
  }
  [
    domain_errors,
    default_locale_errors,
    web_presence_alternate_locale_errors(input),
    route_and_suffix_errors,
  ]
  |> list.flatten
}

fn web_presence_route_errors(
  input: Dict(String, root_field.ResolvedValue),
  require_route: Bool,
) -> List(CapturedJsonValue) {
  let domain_id = graphql_helpers.read_arg_string_nonempty(input, "domainId")
  let suffix =
    graphql_helpers.read_arg_string_nonempty(input, "subfolderSuffix")
  case domain_id, suffix {
    Some(_), Some(_) -> [
      user_error(
        ["input"],
        "Cannot have both subfolder suffix and domain",
        "CANNOT_HAVE_SUBFOLDER_AND_DOMAIN",
      ),
    ]
    None, None ->
      case require_route {
        True -> [
          user_error(
            ["input"],
            "Requires domain or subfolder",
            "REQUIRES_DOMAIN_OR_SUBFOLDER",
          ),
        ]
        False -> []
      }
    _, _ -> []
  }
}

fn web_presence_default_locale_errors(
  input: Dict(String, root_field.ResolvedValue),
) -> List(CapturedJsonValue) {
  case graphql_helpers.read_arg_string(input, "defaultLocale") {
    Some("") -> [
      user_error(
        ["input", "defaultLocale"],
        "Default locale can't be blank",
        "CANNOT_SET_DEFAULT_LOCALE_TO_NULL",
      ),
    ]
    Some(locale) ->
      case valid_web_presence_locale(locale) {
        True -> []
        False -> [
          user_error(
            ["input", "defaultLocale"],
            "Invalid locale codes: " <> locale,
            "INVALID",
          ),
        ]
      }
    None -> [
      user_error(
        ["input", "defaultLocale"],
        "Default locale can't be blank",
        "CANNOT_SET_DEFAULT_LOCALE_TO_NULL",
      ),
    ]
  }
}

fn web_presence_alternate_locale_errors(
  input: Dict(String, root_field.ResolvedValue),
) -> List(CapturedJsonValue) {
  read_arg_string_array(input, "alternateLocales")
  |> option.unwrap([])
  |> list.index_fold([], fn(errors, locale, index) {
    case valid_web_presence_locale(locale) {
      True -> errors
      False ->
        list.append(errors, [
          user_error(
            ["input", "alternateLocales", int.to_string(index)],
            "Invalid locale codes: " <> locale,
            "INVALID",
          ),
        ])
    }
  })
}

fn web_presence_subfolder_suffix_errors(
  input: Dict(String, root_field.ResolvedValue),
) -> List(CapturedJsonValue) {
  case graphql_helpers.read_arg_string_nonempty(input, "subfolderSuffix") {
    Some(suffix) -> {
      let length_errors = case subfolder_suffix_letter_count(suffix) < 2 {
        True -> [
          user_error(
            ["input", "subfolderSuffix"],
            "Subfolder suffix must be at least 2 letters",
            "SUBFOLDER_SUFFIX_MUST_BE_AT_LEAST_2_LETTERS",
          ),
        ]
        False -> []
      }
      let script_errors = case is_web_presence_script_code(suffix) {
        True -> [
          user_error(
            ["input", "subfolderSuffix"],
            "Subfolder suffix cannot be script code",
            "SUBFOLDER_SUFFIX_CANNOT_BE_SCRIPT_CODE",
          ),
        ]
        False -> []
      }
      list.append(length_errors, script_errors)
    }
    None -> []
  }
}

fn valid_web_presence_locale(locale: String) -> Bool {
  list.contains(shopify_i18n_language_codes(), locale)
}

fn shopify_i18n_language_codes() -> List(String) {
  [
    "af", "ak", "sq", "am", "ar", "hy", "as", "az", "bm", "bn", "eu", "be", "bs",
    "br", "bg", "my", "ca", "ckb", "ce", "zh-CN", "zh-TW", "kw", "hr", "cs",
    "da", "nl", "dz", "en", "eo", "et", "ee", "fo", "fil", "fi", "fr", "fr-CA",
    "ff", "gl", "lg", "ka", "de", "el", "gu", "ha", "he", "hi", "hu", "is", "ig",
    "id", "ia", "ga", "it", "ja", "jv", "kl", "kn", "ks", "kk", "km", "ki", "rw",
    "ko", "ku", "ky", "lo", "lv", "ln", "lt", "lu", "lb", "mk", "mg", "ms", "ml",
    "mt", "gv", "mr", "mn", "mi", "ne", "nd", "se", "no", "nb", "nn", "or", "om",
    "os", "ps", "fa", "pl", "pt-BR", "pt-PT", "pa", "qu", "ro", "rm", "rn", "ru",
    "sg", "sa", "sc", "gd", "sr", "sn", "ii", "sd", "si", "sk", "sl", "so", "es",
    "su", "sw", "sv", "tg", "ta", "tt", "te", "th", "bo", "ti", "to", "tr", "tk",
    "uk", "ur", "ug", "uz", "vi", "cy", "fy", "wo", "xh", "yi", "yo", "zu",
  ]
}

fn subfolder_suffix_letter_count(value: String) -> Int {
  value
  |> string.to_graphemes
  |> list.filter(is_ascii_alpha)
  |> list.length
}

fn is_ascii_alpha(grapheme: String) -> Bool {
  string.contains(
    "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ",
    grapheme,
  )
}

fn is_web_presence_script_code(value: String) -> Bool {
  list.contains(
    [
      "Arab", "Armn", "Beng", "Bopo", "Brai", "Cyrl", "Deva", "Ethi", "Geor",
      "Grek", "Gujr", "Guru", "Hang", "Hani", "Hans", "Hant", "Hebr", "Hira",
      "Jpan", "Kana", "Khmr", "Knda", "Kore", "Laoo", "Latn", "Mlym", "Mong",
      "Mymr", "Orya", "Sinh", "Taml", "Telu", "Tfng", "Thaa", "Thai", "Tibt",
      "Yiii",
    ],
    value,
  )
}

fn web_presence_data(
  store: Store,
  id: String,
  input: Dict(String, root_field.ResolvedValue),
) -> CapturedJsonValue {
  let default_locale =
    graphql_helpers.read_arg_string_nonempty(input, "defaultLocale")
    |> option.unwrap("en")
  let alternate_locales =
    read_arg_string_array(input, "alternateLocales") |> option.unwrap([])
  let suffix =
    graphql_helpers.read_arg_string_nonempty(input, "subfolderSuffix")
  let domain =
    web_presence_domain_for_input(
      store,
      graphql_helpers.read_arg_string_nonempty(input, "domainId"),
    )
  let has_alternate_locales = !list.is_empty(alternate_locales)
  let locales = [default_locale, ..alternate_locales]
  CapturedObject([
    #("__typename", CapturedString("MarketWebPresence")),
    #("id", CapturedString(id)),
    #("subfolderSuffix", optional_captured_string(suffix)),
    #("domain", optional_web_presence_domain(domain)),
    #(
      "rootUrls",
      CapturedArray(
        list.map(locales, fn(locale) {
          CapturedObject([
            #("locale", CapturedString(locale)),
            #(
              "url",
              CapturedString(web_presence_root_url(
                store,
                locale,
                default_locale,
                suffix,
                domain,
                has_alternate_locales,
              )),
            ),
          ])
        }),
      ),
    ),
    #("defaultLocale", locale_payload(default_locale, True)),
    #(
      "alternateLocales",
      CapturedArray(
        list.map(alternate_locales, fn(locale) { locale_payload(locale, False) }),
      ),
    ),
    #(
      "markets",
      CapturedObject([
        #("nodes", CapturedArray([])),
        #("edges", CapturedArray([])),
      ]),
    ),
  ])
}

fn web_presence_root_url(
  store: Store,
  locale: String,
  default_locale: String,
  suffix: Option(String),
  domain: Option(ShopDomainRecord),
  has_alternate_locales: Bool,
) -> String {
  let base_url = case domain {
    Some(domain) -> web_presence_domain_base_url(domain)
    None -> shop_primary_web_presence_base_url(store) |> option.unwrap("")
  }
  case domain {
    Some(_) -> localized_root_url(base_url, locale, default_locale)
    None ->
      case shop_primary_web_presence_base_url(store) {
        Some(base_url) ->
          documented_subfolder_root_url(
            base_url,
            locale,
            default_locale,
            suffix,
          )
        None ->
          case has_alternate_locales {
            True ->
              documented_subfolder_root_url(
                captured_web_presence_base_url(store) |> option.unwrap(""),
                locale,
                default_locale,
                suffix,
              )
            False ->
              legacy_captured_subfolder_root_url(
                captured_web_presence_base_url(store) |> option.unwrap(""),
                locale,
                suffix,
              )
          }
      }
  }
}

fn web_presence_domain_for_input(
  store: Store,
  domain_id: Option(String),
) -> Option(ShopDomainRecord) {
  case domain_id {
    Some(id) -> web_presence_domain_for_id(store, id)
    None -> None
  }
}

fn web_presence_domain_for_id(
  store: Store,
  domain_id: String,
) -> Option(ShopDomainRecord) {
  case store.get_effective_shop(store) {
    Some(shop) ->
      case shop.primary_domain.id == domain_id {
        True -> Some(shop.primary_domain)
        False -> None
      }
    None -> None
  }
}

fn optional_web_presence_domain(
  domain: Option(ShopDomainRecord),
) -> CapturedJsonValue {
  case domain {
    Some(domain) ->
      CapturedObject([
        #("__typename", CapturedString("Domain")),
        #("id", CapturedString(domain.id)),
        #("host", CapturedString(domain.host)),
        #("url", CapturedString(domain.url)),
        #("sslEnabled", CapturedBool(domain.ssl_enabled)),
      ])
    None -> CapturedNull
  }
}

fn localized_root_url(
  base_url: String,
  locale: String,
  default_locale: String,
) -> String {
  case locale == default_locale {
    True -> base_url <> "/"
    False -> base_url <> "/" <> locale <> "/"
  }
}

fn documented_subfolder_root_url(
  base_url: String,
  locale: String,
  default_locale: String,
  suffix: Option(String),
) -> String {
  case suffix {
    Some(s) ->
      case locale == default_locale {
        True -> base_url <> "/" <> s <> "/"
        False -> base_url <> "/" <> s <> "/" <> locale <> "/"
      }
    None -> localized_root_url(base_url, locale, default_locale)
  }
}

fn legacy_captured_subfolder_root_url(
  base_url: String,
  locale: String,
  suffix: Option(String),
) -> String {
  case suffix {
    Some(s) -> base_url <> "/" <> locale <> "-" <> s <> "/"
    None -> base_url <> "/"
  }
}

fn shop_primary_web_presence_base_url(store: Store) -> Option(String) {
  case store.get_effective_shop(store) {
    Some(shop) -> Some(web_presence_domain_base_url(shop.primary_domain))
    None -> None
  }
}

fn captured_web_presence_base_url(store: Store) -> Option(String) {
  case list.first(store.list_effective_web_presences(store)) {
    Ok(record) ->
      case captured_field(record.data, "domain") {
        Some(domain) -> captured_domain_base_url(domain)
        None -> None
      }
    Error(_) -> None
  }
}

fn captured_domain_base_url(domain: CapturedJsonValue) -> Option(String) {
  case captured_string_field(domain, "url") {
    Some(url) -> Some(trim_trailing_slash(url))
    None ->
      case captured_string_field(domain, "host") {
        Some(host) -> Some("https://" <> host)
        None -> None
      }
  }
}

fn web_presence_domain_base_url(domain: ShopDomainRecord) -> String {
  let raw = case domain.url == "" {
    True -> "https://" <> domain.host
    False -> domain.url
  }
  trim_trailing_slash(raw)
}

fn trim_trailing_slash(value: String) -> String {
  case string.ends_with(value, "/") {
    True -> string.drop_end(value, 1)
    False -> value
  }
}

fn locale_payload(locale: String, primary: Bool) -> CapturedJsonValue {
  CapturedObject([
    #("locale", CapturedString(locale)),
    #("name", CapturedString(locale_name(locale))),
    #("primary", CapturedBool(primary)),
    #("published", CapturedBool(True)),
  ])
}

fn locale_name(locale: String) -> String {
  case locale {
    "en" -> "English"
    "fr" -> "French"
    "de" -> "German"
    "es" -> "Spanish"
    _ -> string.uppercase(locale)
  }
}

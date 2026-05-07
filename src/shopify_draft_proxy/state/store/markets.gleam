//// Store operations for market and localization records.

import gleam/dict.{type Dict}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/order
import gleam/string
import shopify_draft_proxy/shopify/resource_ids
import shopify_draft_proxy/state/store/shared.{
  append_unique_id, dedupe_strings, dict_has, list_to_set, option_to_result,
}
import shopify_draft_proxy/state/store/types.{
  type Store, BaseState, StagedState, Store,
}
import shopify_draft_proxy/state/types.{
  type CapturedJsonValue, type CatalogRecord, type LocaleRecord,
  type MarketLocalizationRecord, type MarketRecord, type PriceListRecord,
  type ShopLocaleRecord, type TranslationRecord, type WebPresenceRecord,
  CapturedArray, CapturedBool, CapturedInt, CapturedNull, CapturedObject,
  CapturedString,
} as state_types

// ---------------------------------------------------------------------------
// Markets slice
// ---------------------------------------------------------------------------

fn upsert_base_ordered_record(ids: List(String), id: String) -> List(String) {
  append_unique_id(ids, id)
}

pub fn upsert_base_markets(store: Store, records: List(MarketRecord)) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    let staged = acc.staged_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        markets: dict.insert(base.markets, record.id, record),
        market_order: upsert_base_ordered_record(base.market_order, record.id),
        deleted_market_ids: dict.delete(base.deleted_market_ids, record.id),
      ),
      staged_state: StagedState(
        ..staged,
        deleted_market_ids: dict.delete(staged.deleted_market_ids, record.id),
      ),
    )
  })
}

pub fn get_effective_market_by_id(
  store: Store,
  id: String,
) -> Option(MarketRecord) {
  case dict_has(store.staged_state.deleted_market_ids, id) {
    True -> None
    False ->
      case dict.get(store.staged_state.markets, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.markets, id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

pub fn list_effective_markets(store: Store) -> List(MarketRecord) {
  list_effective_ordered_records(
    store.base_state.market_order,
    store.staged_state.market_order,
    dict.merge(store.base_state.markets, store.staged_state.markets),
    fn(id) { get_effective_market_by_id(store, id) },
  )
}

fn market_localization_key(
  resource_id: String,
  market_id: String,
  key: String,
) -> String {
  resource_id <> "::" <> market_id <> "::" <> key
}

pub fn upsert_staged_market_localizations(
  store: Store,
  records: List(MarketLocalizationRecord),
) -> Store {
  let staged = store.staged_state
  let next_bucket =
    list.fold(records, staged.market_localizations, fn(acc, record) {
      dict.insert(
        acc,
        market_localization_key(
          record.resource_id,
          record.market_id,
          record.key,
        ),
        record,
      )
    })
  Store(
    ..store,
    staged_state: StagedState(..staged, market_localizations: next_bucket),
  )
}

pub fn delete_staged_market_localizations(
  store: Store,
  resource_id: String,
  market_ids: List(String),
  localization_keys: List(String),
) -> #(List(MarketLocalizationRecord), Store) {
  let staged = store.staged_state
  let deleted =
    staged.market_localizations
    |> dict.values
    |> list.filter(fn(record) {
      record.resource_id == resource_id
      && list.contains(market_ids, record.market_id)
      && list.contains(localization_keys, record.key)
    })
    |> sort_market_localization_records
  let next_bucket =
    list.fold(deleted, staged.market_localizations, fn(acc, record) {
      dict.delete(
        acc,
        market_localization_key(
          record.resource_id,
          record.market_id,
          record.key,
        ),
      )
    })
  #(
    deleted,
    Store(
      ..store,
      staged_state: StagedState(..staged, market_localizations: next_bucket),
    ),
  )
}

pub fn list_effective_market_localizations(
  store: Store,
  resource_id: String,
) -> List(MarketLocalizationRecord) {
  dict.merge(
    store.base_state.market_localizations,
    store.staged_state.market_localizations,
  )
  |> dict.values
  |> list.filter(fn(record) {
    record.resource_id == resource_id
    && !dict_has(store.staged_state.deleted_market_ids, record.market_id)
  })
  |> sort_market_localization_records
}

fn sort_market_localization_records(
  records: List(MarketLocalizationRecord),
) -> List(MarketLocalizationRecord) {
  records
  |> list.sort(fn(left, right) {
    case string.compare(left.market_id, right.market_id) {
      order.Eq -> string.compare(left.key, right.key)
      other -> other
    }
  })
}

pub fn remove_staged_market_localizations(
  store: Store,
  resource_id: String,
  keys: List(String),
  market_ids: Option(List(String)),
) -> #(List(MarketLocalizationRecord), Store) {
  let staged = store.staged_state
  let removed =
    staged.market_localizations
    |> dict.values
    |> list.filter(fn(record) {
      record.resource_id == resource_id
      && list.contains(keys, record.key)
      && case market_ids {
        Some(ids) -> list.contains(ids, record.market_id)
        None -> True
      }
    })
    |> list.sort(fn(left, right) {
      case string.compare(left.market_id, right.market_id) {
        order.Eq -> string.compare(left.key, right.key)
        other -> other
      }
    })
  let next_bucket =
    list.fold(removed, staged.market_localizations, fn(acc, record) {
      dict.delete(
        acc,
        market_localization_key(
          record.resource_id,
          record.market_id,
          record.key,
        ),
      )
    })
  #(
    removed,
    Store(
      ..store,
      staged_state: StagedState(..staged, market_localizations: next_bucket),
    ),
  )
}

pub fn upsert_staged_market(
  store: Store,
  record: MarketRecord,
) -> #(MarketRecord, Store) {
  let staged = store.staged_state
  let base = store.base_state
  let already_known =
    list.contains(base.market_order, record.id)
    || list.contains(staged.market_order, record.id)
  let new_order = case already_known {
    True -> staged.market_order
    False -> list.append(staged.market_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      markets: dict.insert(staged.markets, record.id, record),
      market_order: new_order,
      deleted_market_ids: dict.delete(staged.deleted_market_ids, record.id),
    )
  #(record, Store(..store, staged_state: new_staged))
}

pub fn delete_staged_market(store: Store, id: String) -> Store {
  let staged = store.staged_state
  let new_staged =
    StagedState(
      ..staged,
      markets: dict.delete(staged.markets, id),
      deleted_market_ids: dict.insert(staged.deleted_market_ids, id, True),
    )
  Store(..store, staged_state: new_staged)
  |> cascade_market_delete(id)
}

pub fn upsert_base_catalogs(
  store: Store,
  records: List(CatalogRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    let staged = acc.staged_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        catalogs: dict.insert(base.catalogs, record.id, record),
        catalog_order: upsert_base_ordered_record(base.catalog_order, record.id),
        deleted_catalog_ids: dict.delete(base.deleted_catalog_ids, record.id),
      ),
      staged_state: StagedState(
        ..staged,
        deleted_catalog_ids: dict.delete(staged.deleted_catalog_ids, record.id),
      ),
    )
  })
}

pub fn get_effective_catalog_by_id(
  store: Store,
  id: String,
) -> Option(CatalogRecord) {
  case dict_has(store.staged_state.deleted_catalog_ids, id) {
    True -> None
    False ->
      case dict.get(store.staged_state.catalogs, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.catalogs, id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

pub fn list_effective_catalogs(store: Store) -> List(CatalogRecord) {
  list_effective_ordered_records(
    store.base_state.catalog_order,
    store.staged_state.catalog_order,
    dict.merge(store.base_state.catalogs, store.staged_state.catalogs),
    fn(id) { get_effective_catalog_by_id(store, id) },
  )
}

pub fn upsert_staged_catalog(
  store: Store,
  record: CatalogRecord,
) -> #(CatalogRecord, Store) {
  let staged = store.staged_state
  let base = store.base_state
  let already_known =
    list.contains(base.catalog_order, record.id)
    || list.contains(staged.catalog_order, record.id)
  let new_order = case already_known {
    True -> staged.catalog_order
    False -> list.append(staged.catalog_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      catalogs: dict.insert(staged.catalogs, record.id, record),
      catalog_order: new_order,
      deleted_catalog_ids: dict.delete(staged.deleted_catalog_ids, record.id),
    )
  #(record, Store(..store, staged_state: new_staged))
}

pub fn delete_staged_catalog(store: Store, id: String) -> Store {
  let staged = store.staged_state
  let new_staged =
    StagedState(
      ..staged,
      catalogs: dict.delete(staged.catalogs, id),
      deleted_catalog_ids: dict.insert(staged.deleted_catalog_ids, id, True),
    )
  Store(..store, staged_state: new_staged)
  |> detach_catalog_from_price_lists(id)
}

pub fn upsert_base_price_lists(
  store: Store,
  records: List(PriceListRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    let staged = acc.staged_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        price_lists: dict.insert(base.price_lists, record.id, record),
        price_list_order: upsert_base_ordered_record(
          base.price_list_order,
          record.id,
        ),
        deleted_price_list_ids: dict.delete(
          base.deleted_price_list_ids,
          record.id,
        ),
      ),
      staged_state: StagedState(
        ..staged,
        deleted_price_list_ids: dict.delete(
          staged.deleted_price_list_ids,
          record.id,
        ),
      ),
    )
  })
}

pub fn get_effective_price_list_by_id(
  store: Store,
  id: String,
) -> Option(PriceListRecord) {
  case dict_has(store.staged_state.deleted_price_list_ids, id) {
    True -> None
    False ->
      case dict.get(store.staged_state.price_lists, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.price_lists, id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

pub fn list_effective_price_lists(store: Store) -> List(PriceListRecord) {
  list_effective_ordered_records(
    store.base_state.price_list_order,
    store.staged_state.price_list_order,
    dict.merge(store.base_state.price_lists, store.staged_state.price_lists),
    fn(id) { get_effective_price_list_by_id(store, id) },
  )
}

pub fn upsert_staged_price_list(
  store: Store,
  record: PriceListRecord,
) -> #(PriceListRecord, Store) {
  let staged = store.staged_state
  let base = store.base_state
  let already_known =
    list.contains(base.price_list_order, record.id)
    || list.contains(staged.price_list_order, record.id)
  let new_order = case already_known {
    True -> staged.price_list_order
    False -> list.append(staged.price_list_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      price_lists: dict.insert(staged.price_lists, record.id, record),
      price_list_order: new_order,
      deleted_price_list_ids: dict.delete(
        staged.deleted_price_list_ids,
        record.id,
      ),
    )
  #(record, Store(..store, staged_state: new_staged))
}

pub fn delete_staged_price_list(store: Store, id: String) -> Store {
  let staged = store.staged_state
  let new_staged =
    StagedState(
      ..staged,
      price_lists: dict.delete(staged.price_lists, id),
      deleted_price_list_ids: dict.insert(
        staged.deleted_price_list_ids,
        id,
        True,
      ),
    )
  Store(..store, staged_state: new_staged)
  |> detach_price_list_from_catalogs(id)
}

pub fn upsert_base_web_presences(
  store: Store,
  records: List(WebPresenceRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    let staged = acc.staged_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        web_presences: dict.insert(base.web_presences, record.id, record),
        web_presence_order: upsert_base_ordered_record(
          base.web_presence_order,
          record.id,
        ),
        deleted_web_presence_ids: dict.delete(
          base.deleted_web_presence_ids,
          record.id,
        ),
      ),
      staged_state: StagedState(
        ..staged,
        deleted_web_presence_ids: dict.delete(
          staged.deleted_web_presence_ids,
          record.id,
        ),
      ),
    )
  })
}

pub fn get_effective_web_presence_by_id(
  store: Store,
  id: String,
) -> Option(WebPresenceRecord) {
  case dict_has(store.staged_state.deleted_web_presence_ids, id) {
    True -> None
    False ->
      case dict.get(store.staged_state.web_presences, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.web_presences, id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

pub fn list_effective_web_presences(store: Store) -> List(WebPresenceRecord) {
  list_effective_ordered_records(
    store.base_state.web_presence_order,
    store.staged_state.web_presence_order,
    dict.merge(store.base_state.web_presences, store.staged_state.web_presences),
    fn(id) { get_effective_web_presence_by_id(store, id) },
  )
}

pub fn upsert_staged_web_presence(
  store: Store,
  record: WebPresenceRecord,
) -> #(WebPresenceRecord, Store) {
  let staged = store.staged_state
  let base = store.base_state
  let already_known =
    list.contains(base.web_presence_order, record.id)
    || list.contains(staged.web_presence_order, record.id)
  let new_order = case already_known {
    True -> staged.web_presence_order
    False -> list.append(staged.web_presence_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      web_presences: dict.insert(staged.web_presences, record.id, record),
      web_presence_order: new_order,
      deleted_web_presence_ids: dict.delete(
        staged.deleted_web_presence_ids,
        record.id,
      ),
    )
  #(record, Store(..store, staged_state: new_staged))
}

pub fn delete_staged_web_presence(store: Store, id: String) -> Store {
  let staged = store.staged_state
  let new_staged =
    StagedState(
      ..staged,
      web_presences: dict.delete(staged.web_presences, id),
      deleted_web_presence_ids: dict.insert(
        staged.deleted_web_presence_ids,
        id,
        True,
      ),
    )
  Store(..store, staged_state: new_staged)
}

pub fn upsert_base_markets_root_payload(
  store: Store,
  key: String,
  payload: CapturedJsonValue,
) -> Store {
  let base = store.base_state
  Store(
    ..store,
    base_state: BaseState(
      ..base,
      markets_root_payloads: dict.insert(
        base.markets_root_payloads,
        key,
        payload,
      ),
    ),
  )
}

pub fn get_effective_markets_root_payload(
  store: Store,
  key: String,
) -> Option(CapturedJsonValue) {
  case dict.get(store.staged_state.markets_root_payloads, key) {
    Ok(payload) -> Some(payload)
    Error(_) ->
      case dict.get(store.base_state.markets_root_payloads, key) {
        Ok(payload) -> Some(payload)
        Error(_) -> None
      }
  }
}

fn cascade_market_delete(store: Store, market_id: String) -> Store {
  store
  |> delete_web_presences_for_market(market_id)
  |> remove_market_localizations_for_market(market_id)
  |> remove_market_from_catalog_contexts(market_id)
}

fn delete_web_presences_for_market(store: Store, market_id: String) -> Store {
  store
  |> list_effective_web_presences
  |> list.filter(fn(record) {
    web_presence_references_market(record, market_id)
  })
  |> list.fold(store, fn(acc, record) {
    delete_staged_web_presence(acc, record.id)
  })
}

fn remove_market_localizations_for_market(
  store: Store,
  market_id: String,
) -> Store {
  let staged = store.staged_state
  let next_localizations =
    staged.market_localizations
    |> dict.filter(fn(_key, record) { record.market_id != market_id })
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      market_localizations: next_localizations,
    ),
  )
}

fn remove_market_from_catalog_contexts(
  store: Store,
  market_id: String,
) -> Store {
  store
  |> list_effective_catalogs
  |> list.fold(store, fn(acc, record) {
    let next_record = remove_market_from_catalog(record, market_id)
    case next_record.data == record.data {
      True -> acc
      False -> {
        let #(_, next_store) = upsert_staged_catalog(acc, next_record)
        next_store
      }
    }
  })
}

fn remove_market_from_catalog(
  record: CatalogRecord,
  market_id: String,
) -> CatalogRecord {
  case captured_field(record.data, "markets") {
    Some(markets) -> {
      let next_markets = filter_connection_by_node_id(markets, market_id)
      state_types.CatalogRecord(
        ..record,
        data: captured_object_upsert(record.data, [
          #("markets", next_markets),
        ]),
      )
    }
    None -> record
  }
}

fn detach_catalog_from_price_lists(store: Store, catalog_id: String) -> Store {
  store
  |> list_effective_price_lists
  |> list.fold(store, fn(acc, record) {
    case price_list_catalog_id(record) == Some(catalog_id) {
      True -> {
        let #(_, next_store) =
          upsert_staged_price_list(
            acc,
            state_types.PriceListRecord(
              ..record,
              data: captured_object_upsert(record.data, [
                #("catalog", CapturedNull),
              ]),
            ),
          )
        next_store
      }
      False -> acc
    }
  })
}

fn detach_price_list_from_catalogs(
  store: Store,
  price_list_id: String,
) -> Store {
  store
  |> list_effective_catalogs
  |> list.fold(store, fn(acc, record) {
    case catalog_price_list_id(record) == Some(price_list_id) {
      True -> {
        let #(_, next_store) =
          upsert_staged_catalog(
            acc,
            state_types.CatalogRecord(
              ..record,
              data: captured_object_upsert(record.data, [
                #("priceList", CapturedNull),
              ]),
            ),
          )
        next_store
      }
      False -> acc
    }
  })
}

pub fn clear_price_list_fixed_prices(
  record: PriceListRecord,
) -> PriceListRecord {
  state_types.PriceListRecord(
    ..record,
    data: captured_object_upsert(record.data, [
      #("fixedPricesCount", CapturedInt(0)),
      #("prices", empty_connection()),
    ]),
  )
}

fn web_presence_references_market(
  record: WebPresenceRecord,
  market_id: String,
) -> Bool {
  captured_string_field(record.data, "marketId") == Some(market_id)
  || {
    case captured_field(record.data, "market") {
      Some(market) -> captured_string_field(market, "id") == Some(market_id)
      None -> False
    }
  }
  || connection_contains_node_id(record.data, "markets", market_id)
}

fn price_list_catalog_id(record: PriceListRecord) -> Option(String) {
  captured_field(record.data, "catalog")
  |> option.then(captured_string_field(_, "id"))
}

fn catalog_price_list_id(record: CatalogRecord) -> Option(String) {
  captured_field(record.data, "priceList")
  |> option.then(captured_string_field(_, "id"))
}

fn connection_contains_node_id(
  data: CapturedJsonValue,
  field_name: String,
  id: String,
) -> Bool {
  case captured_field(data, field_name) {
    Some(connection) ->
      connection_nodes(connection)
      |> list.any(fn(node) { captured_string_field(node, "id") == Some(id) })
    None -> False
  }
}

fn filter_connection_by_node_id(
  connection: CapturedJsonValue,
  removed_id: String,
) -> CapturedJsonValue {
  let nodes =
    connection_nodes(connection)
    |> list.filter(fn(node) {
      captured_string_field(node, "id") != Some(removed_id)
    })
  let edges =
    connection_edges(connection)
    |> list.filter(fn(edge) {
      captured_field(edge, "node")
      |> option.then(captured_string_field(_, "id"))
      != Some(removed_id)
    })
  connection_from_nodes_edges(nodes, edges)
}

fn connection_nodes(connection: CapturedJsonValue) -> List(CapturedJsonValue) {
  case captured_field(connection, "nodes") {
    Some(CapturedArray(nodes)) -> nodes
    _ ->
      connection_edges(connection)
      |> list.filter_map(fn(edge) {
        captured_field(edge, "node") |> option_to_result
      })
  }
}

fn connection_edges(connection: CapturedJsonValue) -> List(CapturedJsonValue) {
  case captured_field(connection, "edges") {
    Some(CapturedArray(edges)) -> edges
    _ -> []
  }
}

fn connection_from_nodes_edges(
  nodes: List(CapturedJsonValue),
  edges: List(CapturedJsonValue),
) -> CapturedJsonValue {
  let cursors = case edges {
    [] ->
      nodes
      |> list.filter_map(fn(node) {
        captured_string_field(node, "id") |> option_to_result
      })
    _ ->
      edges
      |> list.filter_map(fn(edge) {
        captured_string_field(edge, "cursor") |> option_to_result
      })
  }
  CapturedObject([
    #("nodes", CapturedArray(nodes)),
    #("edges", CapturedArray(edges)),
    #("pageInfo", page_info_for_cursors(cursors)),
  ])
}

fn empty_connection() -> CapturedJsonValue {
  connection_from_nodes_edges([], [])
}

fn page_info_for_cursors(cursors: List(String)) -> CapturedJsonValue {
  CapturedObject([
    #("hasNextPage", CapturedBool(False)),
    #("hasPreviousPage", CapturedBool(False)),
    #(
      "startCursor",
      optional_captured_string(list.first(cursors) |> result_to_option),
    ),
    #(
      "endCursor",
      optional_captured_string(list.last(cursors) |> result_to_option),
    ),
  ])
}

fn optional_captured_string(value: Option(String)) -> CapturedJsonValue {
  case value {
    Some(value) -> CapturedString(value)
    None -> CapturedNull
  }
}

fn result_to_option(value: Result(a, b)) -> Option(a) {
  case value {
    Ok(item) -> Some(item)
    Error(_) -> None
  }
}

fn captured_object_upsert(
  value: CapturedJsonValue,
  updates: List(#(String, CapturedJsonValue)),
) -> CapturedJsonValue {
  let base = case value {
    CapturedObject(fields) -> fields
    _ -> []
  }
  let retained =
    base
    |> list.filter(fn(pair) {
      let #(key, _) = pair
      !list.any(updates, fn(update) {
        let #(update_key, _) = update
        update_key == key
      })
    })
  CapturedObject(list.append(retained, updates))
}

fn captured_field(
  value: CapturedJsonValue,
  key: String,
) -> Option(CapturedJsonValue) {
  case value {
    CapturedObject(fields) -> captured_field_from_pairs(fields, key)
    _ -> None
  }
}

fn captured_field_from_pairs(
  fields: List(#(String, CapturedJsonValue)),
  key: String,
) -> Option(CapturedJsonValue) {
  case fields {
    [] -> None
    [first, ..rest] -> {
      let #(field_key, field_value) = first
      case field_key == key {
        True -> Some(field_value)
        False -> captured_field_from_pairs(rest, key)
      }
    }
  }
}

fn captured_string_field(
  value: CapturedJsonValue,
  key: String,
) -> Option(String) {
  case captured_field(value, key) {
    Some(CapturedString(value)) -> Some(value)
    _ -> None
  }
}

fn list_effective_ordered_records(
  base_order: List(String),
  staged_order: List(String),
  merged: Dict(String, a),
  by_id: fn(String) -> Option(a),
) -> List(a) {
  let ordered_ids = list.append(base_order, staged_order) |> dedupe_strings()
  let ordered =
    list.filter_map(ordered_ids, fn(id) { by_id(id) |> option_to_result })
  let ordered_set = list_to_set(ordered_ids)
  let unordered =
    dict.keys(merged)
    |> list.filter(fn(id) { !dict_has(ordered_set, id) })
    |> list.sort(resource_ids.compare_shopify_resource_ids)
    |> list.filter_map(fn(id) { by_id(id) |> option_to_result })
  list.append(ordered, unordered)
}

// ---------------------------------------------------------------------------
// Localization slice (Pass 23)
// ---------------------------------------------------------------------------

/// Replace the entire `availableLocales` catalog. Mirrors
/// `replaceBaseAvailableLocales`. The TS handler hydrates this from
/// upstream responses; the Gleam port only ever sees it via tests
/// today, but keeping the helper surface intact unblocks future
/// hydration work.
pub fn replace_base_available_locales(
  store: Store,
  locales: List(LocaleRecord),
) -> Store {
  let new_base = BaseState(..store.base_state, available_locales: locales)
  Store(..store, base_state: new_base)
}

/// Read the catalog of every locale Shopify recognises. Mirrors
/// `listEffectiveAvailableLocales`. Empty when no upstream response
/// has hydrated it; the localization handler falls back to its own
/// hardcoded default catalog in that case.
pub fn list_effective_available_locales(store: Store) -> List(LocaleRecord) {
  store.base_state.available_locales
}

/// Upsert one or more shop-locale records into the base state. Mirrors
/// `upsertBaseShopLocales`. Removes any existing "deleted" markers
/// (in either base or staged) for the same locale, since the upstream
/// answer wins.
pub fn upsert_base_shop_locales(
  store: Store,
  records: List(ShopLocaleRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    let staged = acc.staged_state
    let new_base =
      BaseState(
        ..base,
        shop_locales: dict.insert(base.shop_locales, record.locale, record),
      )
    let new_staged =
      StagedState(
        ..staged,
        deleted_shop_locales: dict.delete(
          staged.deleted_shop_locales,
          record.locale,
        ),
      )
    Store(..acc, base_state: new_base, staged_state: new_staged)
  })
}

/// Stage a shop-locale record. Mirrors `stageShopLocale`.
pub fn stage_shop_locale(
  store: Store,
  record: ShopLocaleRecord,
) -> #(ShopLocaleRecord, Store) {
  let staged = store.staged_state
  let new_staged =
    StagedState(
      ..staged,
      shop_locales: dict.insert(staged.shop_locales, record.locale, record),
      deleted_shop_locales: dict.delete(
        staged.deleted_shop_locales,
        record.locale,
      ),
    )
  #(record, Store(..store, staged_state: new_staged))
}

/// Mark a shop-locale as disabled. Mirrors `disableShopLocale`. Returns
/// the record that was previously effective (if any) so the caller can
/// build the mutation response payload.
pub fn disable_shop_locale(
  store: Store,
  locale: String,
) -> #(Option(ShopLocaleRecord), Store) {
  let staged = store.staged_state
  let base = store.base_state
  let existing = case dict.get(staged.shop_locales, locale) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(base.shop_locales, locale) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
  let new_staged = case existing {
    Some(_) ->
      StagedState(
        ..staged,
        shop_locales: dict.delete(staged.shop_locales, locale),
        deleted_shop_locales: dict.insert(
          staged.deleted_shop_locales,
          locale,
          True,
        ),
      )
    None ->
      StagedState(
        ..staged,
        shop_locales: dict.delete(staged.shop_locales, locale),
      )
  }
  #(existing, Store(..store, staged_state: new_staged))
}

/// Look up the effective shop-locale for a locale code. Staged wins
/// over base; any "deleted" marker on the staged side suppresses the
/// record. Mirrors `getEffectiveShopLocale`.
pub fn get_effective_shop_locale(
  store: Store,
  locale: String,
) -> Option(ShopLocaleRecord) {
  case dict_has(store.staged_state.deleted_shop_locales, locale) {
    True -> None
    False ->
      case dict.get(store.staged_state.shop_locales, locale) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.shop_locales, locale) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

/// List every effective shop locale. Optionally filter by `published`.
/// Sort: primary locale first, then by locale code. Mirrors
/// `listEffectiveShopLocales`.
pub fn list_effective_shop_locales(
  store: Store,
  published: Option(Bool),
) -> List(ShopLocaleRecord) {
  let base_records =
    dict.values(store.base_state.shop_locales)
    |> list.filter(fn(record) {
      !dict_has(store.staged_state.deleted_shop_locales, record.locale)
    })
  let staged_records =
    dict.values(store.staged_state.shop_locales)
    |> list.filter(fn(record) {
      !dict_has(store.staged_state.deleted_shop_locales, record.locale)
    })
  let merged_dict =
    list.fold(base_records, dict.new(), fn(acc, record) {
      dict.insert(acc, record.locale, record)
    })
  let merged_dict =
    list.fold(staged_records, merged_dict, fn(acc, record) {
      dict.insert(acc, record.locale, record)
    })
  let merged = dict.values(merged_dict)
  let filtered = case published {
    Some(target) -> list.filter(merged, fn(r) { r.published == target })
    None -> merged
  }
  list.sort(filtered, fn(left, right) {
    case left.primary, right.primary {
      True, False -> order.Lt
      False, True -> order.Gt
      _, _ -> string.compare(left.locale, right.locale)
    }
  })
}

/// Build the storage key used to address a translation:
/// `<resource_id>::<locale>::<market_id?>::<key>`. Mirrors
/// `translationStorageKey`.
pub fn translation_storage_key(
  resource_id: String,
  locale: String,
  key: String,
  market_id: Option(String),
) -> String {
  let market_part = option.unwrap(market_id, "")
  resource_id <> "::" <> locale <> "::" <> market_part <> "::" <> key
}

/// Stage a translation record. Mirrors `stageTranslation`.
pub fn stage_translation(
  store: Store,
  record: TranslationRecord,
) -> #(TranslationRecord, Store) {
  let storage_key =
    translation_storage_key(
      record.resource_id,
      record.locale,
      record.key,
      record.market_id,
    )
  let staged = store.staged_state
  let new_staged =
    StagedState(
      ..staged,
      translations: dict.insert(staged.translations, storage_key, record),
      deleted_translations: dict.delete(
        staged.deleted_translations,
        storage_key,
      ),
    )
  #(record, Store(..store, staged_state: new_staged))
}

/// Upsert a translation record into base state. Used by LiveHybrid
/// localization reads to remember upstream source-content markers
/// without treating that hydration as a staged mutation.
pub fn upsert_base_translation(
  store: Store,
  record: TranslationRecord,
) -> Store {
  let storage_key =
    translation_storage_key(
      record.resource_id,
      record.locale,
      record.key,
      record.market_id,
    )
  let base = store.base_state
  let staged = store.staged_state
  let new_base =
    BaseState(
      ..base,
      translations: dict.insert(base.translations, storage_key, record),
    )
  let new_staged =
    StagedState(
      ..staged,
      deleted_translations: dict.delete(
        staged.deleted_translations,
        storage_key,
      ),
    )
  Store(..store, base_state: new_base, staged_state: new_staged)
}

/// Remove a translation. Returns the record that was effective before
/// removal (if any). Mirrors `removeTranslation`.
pub fn remove_translation(
  store: Store,
  resource_id: String,
  locale: String,
  key: String,
  market_id: Option(String),
) -> #(Option(TranslationRecord), Store) {
  let storage_key = translation_storage_key(resource_id, locale, key, market_id)
  let staged = store.staged_state
  let base = store.base_state
  let existing = case dict.get(staged.translations, storage_key) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(base.translations, storage_key) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
  let new_staged = case existing {
    Some(_) ->
      StagedState(
        ..staged,
        translations: dict.delete(staged.translations, storage_key),
        deleted_translations: dict.insert(
          staged.deleted_translations,
          storage_key,
          True,
        ),
      )
    None ->
      StagedState(
        ..staged,
        translations: dict.delete(staged.translations, storage_key),
      )
  }
  #(existing, Store(..store, staged_state: new_staged))
}

/// Remove every translation registered against a given locale. Returns
/// the records that were effective before removal, sorted by
/// (resource_id, key, updated_at). Mirrors `removeTranslationsForLocale`.
pub fn remove_translations_for_locale(
  store: Store,
  locale: String,
) -> #(List(TranslationRecord), Store) {
  let base_matching =
    dict.values(store.base_state.translations)
    |> list.filter(fn(t) { t.locale == locale })
  let staged_matching =
    dict.values(store.staged_state.translations)
    |> list.filter(fn(t) { t.locale == locale })
  let merged_dict =
    list.fold(base_matching, dict.new(), fn(acc, t) {
      let k =
        translation_storage_key(t.resource_id, t.locale, t.key, t.market_id)
      dict.insert(acc, k, t)
    })
  let merged_dict =
    list.fold(staged_matching, merged_dict, fn(acc, t) {
      let k =
        translation_storage_key(t.resource_id, t.locale, t.key, t.market_id)
      dict.insert(acc, k, t)
    })
  let staged = store.staged_state
  let staged_after_removal =
    list.fold(dict.keys(merged_dict), staged, fn(acc, storage_key) {
      StagedState(
        ..acc,
        translations: dict.delete(acc.translations, storage_key),
        deleted_translations: dict.insert(
          acc.deleted_translations,
          storage_key,
          True,
        ),
      )
    })
  let removed =
    dict.values(merged_dict)
    |> list.sort(fn(left, right) {
      case string.compare(left.resource_id, right.resource_id) {
        order.Eq ->
          case string.compare(left.key, right.key) {
            order.Eq -> string.compare(left.updated_at, right.updated_at)
            other -> other
          }
        other -> other
      }
    })
  #(removed, Store(..store, staged_state: staged_after_removal))
}

/// List the effective translations for a `(resource_id, locale, market_id)`
/// triple. Mirrors `listEffectiveTranslations`. Sort: by `key`, then
/// `updated_at`.
pub fn list_effective_translations(
  store: Store,
  resource_id: String,
  locale: String,
  market_id: Option(String),
) -> List(TranslationRecord) {
  let base_matching =
    dict.values(store.base_state.translations)
    |> list.filter(fn(t) {
      t.resource_id == resource_id
      && t.locale == locale
      && t.market_id == market_id
      && {
        let storage_key =
          translation_storage_key(t.resource_id, t.locale, t.key, t.market_id)
        !dict_has(store.staged_state.deleted_translations, storage_key)
      }
    })
  let staged_matching =
    dict.values(store.staged_state.translations)
    |> list.filter(fn(t) {
      t.resource_id == resource_id
      && t.locale == locale
      && t.market_id == market_id
    })
  let merged_dict =
    list.fold(base_matching, dict.new(), fn(acc, t) {
      let k =
        translation_storage_key(t.resource_id, t.locale, t.key, t.market_id)
      dict.insert(acc, k, t)
    })
  let merged_dict =
    list.fold(staged_matching, merged_dict, fn(acc, t) {
      let k =
        translation_storage_key(t.resource_id, t.locale, t.key, t.market_id)
      dict.insert(acc, k, t)
    })
  dict.values(merged_dict)
  |> list.sort(fn(left, right) {
    case string.compare(left.key, right.key) {
      order.Eq -> string.compare(left.updated_at, right.updated_at)
      other -> other
    }
  })
}

/// True if the store contains any localization state. Mirrors
/// `hasLocalizationState`. Used by the meta-state serializer (not yet
/// ported on the Gleam side); kept here for parity.
pub fn has_localization_state(store: Store) -> Bool {
  let base = store.base_state
  let staged = store.staged_state
  !list.is_empty(base.available_locales)
  || !list.is_empty(dict.keys(base.shop_locales))
  || !list.is_empty(dict.keys(staged.shop_locales))
  || !list.is_empty(dict.keys(staged.deleted_shop_locales))
  || !list.is_empty(dict.keys(base.translations))
  || !list.is_empty(dict.keys(staged.translations))
  || !list.is_empty(dict.keys(staged.deleted_translations))
}

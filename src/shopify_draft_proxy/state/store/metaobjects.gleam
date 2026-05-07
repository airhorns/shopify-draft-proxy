//// Store operations for metaobject records.

import gleam/dict
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/order
import gleam/string
import shopify_draft_proxy/state/store/shared.{
  append_unique_id, dedupe_strings, dict_has, list_to_set, string_compare,
}
import shopify_draft_proxy/state/store/types.{
  type Store, BaseState, StagedState, Store,
}
import shopify_draft_proxy/state/types as state_types

// ---------------------------------------------------------------------------
// Metaobjects slice
// ---------------------------------------------------------------------------

pub fn upsert_base_metaobject_definitions(
  store: Store,
  records: List(state_types.MetaobjectDefinitionRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    let staged = acc.staged_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        metaobject_definitions: dict.insert(
          base.metaobject_definitions,
          record.id,
          record,
        ),
        metaobject_definition_order: append_unique_id(
          base.metaobject_definition_order,
          record.id,
        ),
        deleted_metaobject_definition_ids: dict.delete(
          base.deleted_metaobject_definition_ids,
          record.id,
        ),
      ),
      staged_state: StagedState(
        ..staged,
        deleted_metaobject_definition_ids: dict.delete(
          staged.deleted_metaobject_definition_ids,
          record.id,
        ),
      ),
    )
  })
}

pub fn upsert_staged_metaobject_definition(
  store: Store,
  record: state_types.MetaobjectDefinitionRecord,
) -> #(state_types.MetaobjectDefinitionRecord, Store) {
  let staged = store.staged_state
  let base = store.base_state
  let already_known =
    list.contains(base.metaobject_definition_order, record.id)
    || list.contains(staged.metaobject_definition_order, record.id)
  let new_order = case already_known {
    True -> staged.metaobject_definition_order
    False -> list.append(staged.metaobject_definition_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      metaobject_definitions: dict.insert(
        staged.metaobject_definitions,
        record.id,
        record,
      ),
      metaobject_definition_order: new_order,
      deleted_metaobject_definition_ids: dict.delete(
        staged.deleted_metaobject_definition_ids,
        record.id,
      ),
    )
  #(record, Store(..store, staged_state: new_staged))
}

pub fn delete_staged_metaobject_definition(store: Store, id: String) -> Store {
  let staged = store.staged_state
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      metaobject_definitions: dict.delete(staged.metaobject_definitions, id),
      deleted_metaobject_definition_ids: dict.insert(
        staged.deleted_metaobject_definition_ids,
        id,
        True,
      ),
    ),
  )
}

pub fn get_effective_metaobject_definition_by_id(
  store: Store,
  id: String,
) -> Option(state_types.MetaobjectDefinitionRecord) {
  case dict_has(store.staged_state.deleted_metaobject_definition_ids, id) {
    True -> None
    False ->
      case dict.get(store.staged_state.metaobject_definitions, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.metaobject_definitions, id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

pub fn find_effective_metaobject_definition_by_type(
  store: Store,
  type_: String,
) -> Option(state_types.MetaobjectDefinitionRecord) {
  list.find(list_effective_metaobject_definitions(store), fn(record) {
    record.type_ == type_
  })
  |> option.from_result
}

pub fn link_metaobject_definition_to_product_option(
  store: Store,
  definition_id: String,
  owner_type: String,
  linked_metafield: state_types.ProductOptionLinkedMetafieldRecord,
  product_id: String,
  product_option_id: String,
) -> Store {
  case get_effective_metaobject_definition_by_id(store, definition_id) {
    None -> store
    Some(definition) -> {
      let linked =
        state_types.MetaobjectDefinitionLinkedMetafieldRecord(
          owner_type: owner_type,
          namespace: linked_metafield.namespace,
          key: linked_metafield.key,
          metafield_definition_id: linked_metafield.metafield_definition_id,
          product_id: product_id,
          product_option_id: product_option_id,
        )
      let updated =
        state_types.MetaobjectDefinitionRecord(
          ..definition,
          linked_metafields: upsert_definition_linked_metafield(
            definition.linked_metafields,
            linked,
          ),
        )
      let #(_, next_store) = upsert_staged_metaobject_definition(store, updated)
      next_store
    }
  }
}

fn upsert_definition_linked_metafield(
  records: List(state_types.MetaobjectDefinitionLinkedMetafieldRecord),
  candidate: state_types.MetaobjectDefinitionLinkedMetafieldRecord,
) -> List(state_types.MetaobjectDefinitionLinkedMetafieldRecord) {
  let filtered =
    list.filter(records, fn(record) {
      !{
        record.owner_type == candidate.owner_type
        && record.namespace == candidate.namespace
        && record.key == candidate.key
        && record.product_option_id == candidate.product_option_id
      }
    })
  [candidate, ..filtered]
}

pub fn list_effective_metaobject_definitions(
  store: Store,
) -> List(state_types.MetaobjectDefinitionRecord) {
  let ordered_ids =
    list.append(
      store.base_state.metaobject_definition_order,
      store.staged_state.metaobject_definition_order,
    )
    |> dedupe_strings()
  let ordered_records =
    list.filter_map(ordered_ids, fn(id) {
      case get_effective_metaobject_definition_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  let ordered_set = list_to_set(ordered_ids)
  let merged =
    dict.merge(
      store.base_state.metaobject_definitions,
      store.staged_state.metaobject_definitions,
    )
  let unordered_ids =
    dict.keys(merged)
    |> list.filter(fn(id) { !dict_has(ordered_set, id) })
    |> list.sort(fn(left, right) {
      case dict.get(merged, left), dict.get(merged, right) {
        Ok(l), Ok(r) -> compare_metaobject_definitions(l, r)
        _, _ -> string_compare(left, right)
      }
    })
  let unordered_records =
    list.filter_map(unordered_ids, fn(id) {
      case get_effective_metaobject_definition_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  list.append(ordered_records, unordered_records)
}

pub fn upsert_base_metaobjects(
  store: Store,
  records: List(state_types.MetaobjectRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    let staged = acc.staged_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        metaobjects: dict.insert(base.metaobjects, record.id, record),
        metaobject_order: append_unique_id(base.metaobject_order, record.id),
        deleted_metaobject_ids: dict.delete(
          base.deleted_metaobject_ids,
          record.id,
        ),
      ),
      staged_state: StagedState(
        ..staged,
        deleted_metaobject_ids: dict.delete(
          staged.deleted_metaobject_ids,
          record.id,
        ),
      ),
    )
  })
}

pub fn upsert_staged_metaobject(
  store: Store,
  record: state_types.MetaobjectRecord,
) -> #(state_types.MetaobjectRecord, Store) {
  let staged = store.staged_state
  let base = store.base_state
  let already_known =
    list.contains(base.metaobject_order, record.id)
    || list.contains(staged.metaobject_order, record.id)
  let new_order = case already_known {
    True -> staged.metaobject_order
    False -> list.append(staged.metaobject_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      metaobjects: dict.insert(staged.metaobjects, record.id, record),
      metaobject_order: new_order,
      deleted_metaobject_ids: dict.delete(
        staged.deleted_metaobject_ids,
        record.id,
      ),
    )
  #(record, Store(..store, staged_state: new_staged))
}

pub fn delete_staged_metaobject(store: Store, id: String) -> Store {
  let staged = store.staged_state
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      metaobjects: dict.delete(staged.metaobjects, id),
      deleted_metaobject_ids: dict.insert(
        staged.deleted_metaobject_ids,
        id,
        True,
      ),
    ),
  )
}

pub fn get_effective_metaobject_by_id(
  store: Store,
  id: String,
) -> Option(state_types.MetaobjectRecord) {
  case dict_has(store.staged_state.deleted_metaobject_ids, id) {
    True -> None
    False ->
      case dict.get(store.staged_state.metaobjects, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.metaobjects, id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

pub fn find_effective_metaobject_by_handle(
  store: Store,
  type_: String,
  handle: String,
) -> Option(state_types.MetaobjectRecord) {
  list.find(list_effective_metaobjects(store), fn(record) {
    record.type_ == type_ && record.handle == handle
  })
  |> option.from_result
}

pub fn list_effective_metaobjects(
  store: Store,
) -> List(state_types.MetaobjectRecord) {
  let ordered_ids =
    list.append(
      store.base_state.metaobject_order,
      store.staged_state.metaobject_order,
    )
    |> dedupe_strings()
  let ordered_records =
    list.filter_map(ordered_ids, fn(id) {
      case get_effective_metaobject_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  let ordered_set = list_to_set(ordered_ids)
  let merged =
    dict.merge(store.base_state.metaobjects, store.staged_state.metaobjects)
  let unordered_ids =
    dict.keys(merged)
    |> list.filter(fn(id) { !dict_has(ordered_set, id) })
    |> list.sort(fn(left, right) {
      case dict.get(merged, left), dict.get(merged, right) {
        Ok(l), Ok(r) -> compare_metaobjects(l, r)
        _, _ -> string_compare(left, right)
      }
    })
  let unordered_records =
    list.filter_map(unordered_ids, fn(id) {
      case get_effective_metaobject_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  list.append(ordered_records, unordered_records)
}

pub fn list_effective_metaobjects_by_type(
  store: Store,
  type_: String,
) -> List(state_types.MetaobjectRecord) {
  list.filter(list_effective_metaobjects(store), fn(record) {
    record.type_ == type_
  })
}

pub fn has_effective_metaobjects(store: Store) -> Bool {
  !list.is_empty(dict.keys(store.base_state.metaobjects))
  || !list.is_empty(dict.keys(store.staged_state.metaobjects))
  || !list.is_empty(dict.keys(store.staged_state.deleted_metaobject_ids))
}

fn compare_metaobject_definitions(
  left: state_types.MetaobjectDefinitionRecord,
  right: state_types.MetaobjectDefinitionRecord,
) -> order.Order {
  case string.compare(left.type_, right.type_) {
    order.Eq -> string_compare(left.id, right.id)
    other -> other
  }
}

fn compare_metaobjects(
  left: state_types.MetaobjectRecord,
  right: state_types.MetaobjectRecord,
) -> order.Order {
  case string.compare(left.type_, right.type_) {
    order.Eq ->
      case string.compare(left.handle, right.handle) {
        order.Eq -> string_compare(left.id, right.id)
        other -> other
      }
    other -> other
  }
}
/// Stage a `ValidationRecord`. Mirrors `upsertStagedValidation`. Clears
/// any deletion marker the staged side may carry for the same id.

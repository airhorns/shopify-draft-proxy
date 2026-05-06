//// Store operations for metafield records.

import gleam/dict
import gleam/int
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/order
import gleam/string
import shopify_draft_proxy/shopify/resource_ids
import shopify_draft_proxy/state/store/shared.{
  bool_compare, dict_has, string_compare,
}
import shopify_draft_proxy/state/store/types.{
  type Store, BaseState, StagedState, Store,
}
import shopify_draft_proxy/state/types.{
  type MetafieldDefinitionRecord, type ProductMetafieldRecord,
} as _

// ---------------------------------------------------------------------------
// Metafields slice
// ---------------------------------------------------------------------------

pub fn replace_base_metafields_for_owner(
  store: Store,
  owner_id: String,
  metafields: List(ProductMetafieldRecord),
) -> Store {
  let base = store.base_state
  let retained =
    base.product_metafields
    |> dict.to_list
    |> list.filter(fn(pair) {
      let #(_, metafield) = pair
      metafield.owner_id != owner_id
    })
    |> dict.from_list
  let next_bucket =
    list.fold(metafields, retained, fn(acc, metafield) {
      dict.insert(acc, metafield.id, metafield)
    })
  Store(..store, base_state: BaseState(..base, product_metafields: next_bucket))
}

pub fn replace_staged_metafields_for_owner(
  store: Store,
  owner_id: String,
  metafields: List(ProductMetafieldRecord),
) -> Store {
  let staged = store.staged_state
  let retained =
    staged.product_metafields
    |> dict.to_list
    |> list.filter(fn(pair) {
      let #(_, metafield) = pair
      metafield.owner_id != owner_id
    })
    |> dict.from_list
  let next_bucket =
    list.fold(metafields, retained, fn(acc, metafield) {
      dict.insert(acc, metafield.id, metafield)
    })
  Store(
    ..store,
    staged_state: StagedState(..staged, product_metafields: next_bucket),
  )
}

pub fn upsert_base_metafield_definitions(
  store: Store,
  definitions: List(MetafieldDefinitionRecord),
) -> Store {
  list.fold(definitions, store, fn(acc, definition) {
    let base = acc.base_state
    let staged = acc.staged_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        metafield_definitions: dict.insert(
          base.metafield_definitions,
          definition.id,
          definition,
        ),
        deleted_metafield_definition_ids: dict.delete(
          base.deleted_metafield_definition_ids,
          definition.id,
        ),
      ),
      staged_state: StagedState(
        ..staged,
        deleted_metafield_definition_ids: dict.delete(
          staged.deleted_metafield_definition_ids,
          definition.id,
        ),
      ),
    )
  })
}

pub fn upsert_staged_metafield_definitions(
  store: Store,
  definitions: List(MetafieldDefinitionRecord),
) -> Store {
  list.fold(definitions, store, fn(acc, definition) {
    let staged = acc.staged_state
    Store(
      ..acc,
      staged_state: StagedState(
        ..staged,
        metafield_definitions: dict.insert(
          staged.metafield_definitions,
          definition.id,
          definition,
        ),
        deleted_metafield_definition_ids: dict.delete(
          staged.deleted_metafield_definition_ids,
          definition.id,
        ),
      ),
    )
  })
}

pub fn stage_delete_metafield_definition(
  store: Store,
  definition_id: String,
) -> Store {
  let staged = store.staged_state
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      metafield_definitions: dict.delete(
        staged.metafield_definitions,
        definition_id,
      ),
      deleted_metafield_definition_ids: dict.insert(
        staged.deleted_metafield_definition_ids,
        definition_id,
        True,
      ),
    ),
  )
}

pub fn delete_product_metafields_for_definition(
  store: Store,
  definition: MetafieldDefinitionRecord,
) -> Store {
  case definition.owner_type {
    "PRODUCT" -> {
      let keep = fn(metafield: ProductMetafieldRecord) {
        !{
          metafield.owner_type == Some("PRODUCT")
          && metafield.namespace == definition.namespace
          && metafield.key == definition.key
        }
      }
      let base = store.base_state
      let staged = store.staged_state
      let base_bucket =
        base.product_metafields
        |> dict.to_list
        |> list.filter(fn(pair) {
          let #(_, metafield) = pair
          keep(metafield)
        })
        |> dict.from_list
      let staged_bucket =
        staged.product_metafields
        |> dict.to_list
        |> list.filter(fn(pair) {
          let #(_, metafield) = pair
          keep(metafield)
        })
        |> dict.from_list
      Store(
        ..store,
        base_state: BaseState(..base, product_metafields: base_bucket),
        staged_state: StagedState(..staged, product_metafields: staged_bucket),
      )
    }
    _ -> store
  }
}

pub fn get_effective_metafields_by_owner_id(
  store: Store,
  owner_id: String,
) -> List(ProductMetafieldRecord) {
  let staged =
    dict.values(store.staged_state.product_metafields)
    |> list.filter(fn(metafield) { metafield.owner_id == owner_id })
  let source = case staged {
    [] ->
      dict.values(store.base_state.product_metafields)
      |> list.filter(fn(metafield) { metafield.owner_id == owner_id })
    _ -> staged
  }
  source
  |> list.sort(fn(left, right) {
    case
      bool_compare(
        string.starts_with(left.namespace, "app--"),
        string.starts_with(right.namespace, "app--"),
      )
    {
      order.Eq -> compare_product_metafield_ids(left, right)
      other -> other
    }
  })
}

fn compare_product_metafield_ids(
  left: ProductMetafieldRecord,
  right: ProductMetafieldRecord,
) -> order.Order {
  case is_low_local_metafield_id(left), is_low_local_metafield_id(right) {
    True, False -> order.Gt
    False, True -> order.Lt
    _, _ -> resource_ids.compare_shopify_resource_ids(left.id, right.id)
  }
}

fn is_low_local_metafield_id(record: ProductMetafieldRecord) -> Bool {
  let has_draft_digest = case record.compare_digest {
    Some(digest) -> string.starts_with(digest, "draft:")
    None -> False
  }
  case has_draft_digest, metafield_id_tail(record.id) {
    True, Some(id) -> id < 1_000_000
    _, _ -> False
  }
}

fn metafield_id_tail(id: String) -> Option(Int) {
  case list.last(string.split(id, "/")) {
    Ok(tail) ->
      case int.parse(tail) {
        Ok(value) -> Some(value)
        Error(_) -> None
      }
    Error(_) -> None
  }
}

pub fn find_effective_metafield_by_id(
  store: Store,
  metafield_id: String,
) -> Option(ProductMetafieldRecord) {
  case dict.get(store.staged_state.product_metafields, metafield_id) {
    Ok(metafield) -> Some(metafield)
    Error(_) ->
      case dict.get(store.base_state.product_metafields, metafield_id) {
        Ok(metafield) -> Some(metafield)
        Error(_) -> None
      }
  }
}

pub fn list_effective_metafield_definitions(
  store: Store,
) -> List(MetafieldDefinitionRecord) {
  let merged =
    dict.merge(
      store.base_state.metafield_definitions,
      store.staged_state.metafield_definitions,
    )
  dict.values(merged)
  |> list.filter(fn(definition) {
    !dict_has(
      store.staged_state.deleted_metafield_definition_ids,
      definition.id,
    )
  })
  |> list.sort(fn(left, right) {
    case string_compare(left.owner_type, right.owner_type) {
      order.Eq ->
        case string_compare(left.namespace, right.namespace) {
          order.Eq ->
            case string_compare(left.key, right.key) {
              order.Eq -> string_compare(left.id, right.id)
              other -> other
            }
          other -> other
        }
      other -> other
    }
  })
}

pub fn get_effective_metafield_definition_by_id(
  store: Store,
  definition_id: String,
) -> Option(MetafieldDefinitionRecord) {
  case
    dict_has(store.staged_state.deleted_metafield_definition_ids, definition_id)
  {
    True -> None
    False ->
      case dict.get(store.staged_state.metafield_definitions, definition_id) {
        Ok(definition) -> Some(definition)
        Error(_) ->
          case dict.get(store.base_state.metafield_definitions, definition_id) {
            Ok(definition) -> Some(definition)
            Error(_) -> None
          }
      }
  }
}

pub fn find_effective_metafield_definition(
  store: Store,
  owner_type: String,
  namespace: String,
  key: String,
) -> Option(MetafieldDefinitionRecord) {
  list.find(list_effective_metafield_definitions(store), fn(definition) {
    definition.owner_type == owner_type
    && definition.namespace == namespace
    && definition.key == key
  })
  |> option.from_result
}
/// Stage a local Flow signature audit record.

//// Store operations for Shopify function records.

import gleam/dict
import gleam/list
import gleam/option.{type Option, None, Some}
import shopify_draft_proxy/state/store/shared.{
  append_unique_id, dedupe_strings, dict_has, list_to_set, string_compare,
}
import shopify_draft_proxy/state/store/types.{
  type Store, BaseState, StagedState, Store,
}
import shopify_draft_proxy/state/types.{
  type CartTransformRecord, type ShopifyFunctionRecord,
  type TaxAppConfigurationRecord, type ValidationRecord,
} as _

// ---------------------------------------------------------------------------
// Functions domain (Pass 18)
// ---------------------------------------------------------------------------

/// Persist upstream-hydrated `ShopifyFunctionRecord` rows into base state.
/// Functions cannot be deleted in the proxy — once a record is staged or
/// hydrated upstream, it stays.
pub fn upsert_base_shopify_functions(
  store: Store,
  records: List(ShopifyFunctionRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        shopify_functions: dict.insert(
          base.shopify_functions,
          record.id,
          record,
        ),
        shopify_function_order: append_unique_id(
          base.shopify_function_order,
          record.id,
        ),
      ),
    )
  })
}

/// Stage a `ShopifyFunctionRecord`. Mirrors `upsertStagedShopifyFunction`.
/// Functions cannot be deleted in the proxy — once a record is staged or
/// hydrated upstream, it stays.
pub fn upsert_staged_shopify_function(
  store: Store,
  record: ShopifyFunctionRecord,
) -> #(ShopifyFunctionRecord, Store) {
  let staged = store.staged_state
  let already =
    dict_has(store.base_state.shopify_functions, record.id)
    || dict_has(staged.shopify_functions, record.id)
  let new_order = case already {
    True -> staged.shopify_function_order
    False -> list.append(staged.shopify_function_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      shopify_functions: dict.insert(
        staged.shopify_functions,
        record.id,
        record,
      ),
      shopify_function_order: new_order,
    )
  #(record, Store(..store, staged_state: new_staged))
}

/// Look up an effective `ShopifyFunctionRecord` (staged-over-base).
/// Mirrors `getEffectiveShopifyFunctionById`.
pub fn get_effective_shopify_function_by_id(
  store: Store,
  id: String,
) -> Option(ShopifyFunctionRecord) {
  case dict.get(store.staged_state.shopify_functions, id) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(store.base_state.shopify_functions, id) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

/// List every effective `ShopifyFunctionRecord`. Mirrors
/// `listEffectiveShopifyFunctions`. Ordered records first, then any
/// unordered ones sorted by id.
pub fn list_effective_shopify_functions(
  store: Store,
) -> List(ShopifyFunctionRecord) {
  let ordered_ids =
    list.append(
      store.base_state.shopify_function_order,
      store.staged_state.shopify_function_order,
    )
    |> dedupe_strings()
  let ordered_records =
    list.filter_map(ordered_ids, fn(id) {
      case get_effective_shopify_function_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  let ordered_set = list_to_set(ordered_ids)
  let merged =
    dict.merge(
      store.base_state.shopify_functions,
      store.staged_state.shopify_functions,
    )
  let unordered_ids =
    dict.keys(merged)
    |> list.filter(fn(id) { !dict_has(ordered_set, id) })
    |> list.sort(string_compare)
  let unordered_records =
    list.filter_map(unordered_ids, fn(id) {
      case get_effective_shopify_function_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  list.append(ordered_records, unordered_records)
}

pub fn upsert_staged_validation(
  store: Store,
  record: ValidationRecord,
) -> #(ValidationRecord, Store) {
  let staged = store.staged_state
  let base = store.base_state
  let already_known =
    list.contains(base.validation_order, record.id)
    || list.contains(staged.validation_order, record.id)
  let new_order = case already_known {
    True -> staged.validation_order
    False -> list.append(staged.validation_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      validations: dict.insert(staged.validations, record.id, record),
      validation_order: new_order,
      deleted_validation_ids: dict.delete(
        staged.deleted_validation_ids,
        record.id,
      ),
    )
  #(record, Store(..store, staged_state: new_staged))
}

/// Mark a validation id as deleted. Mirrors `deleteStagedValidation`.
pub fn delete_staged_validation(store: Store, id: String) -> Store {
  let staged = store.staged_state
  let new_staged =
    StagedState(
      ..staged,
      validations: dict.delete(staged.validations, id),
      deleted_validation_ids: dict.insert(
        staged.deleted_validation_ids,
        id,
        True,
      ),
    )
  Store(..store, staged_state: new_staged)
}

/// Look up an effective validation. Mirrors
/// `getEffectiveValidationById`.
pub fn get_effective_validation_by_id(
  store: Store,
  id: String,
) -> Option(ValidationRecord) {
  let deleted =
    dict_has(store.base_state.deleted_validation_ids, id)
    || dict_has(store.staged_state.deleted_validation_ids, id)
  case deleted {
    True -> None
    False ->
      case dict.get(store.staged_state.validations, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.validations, id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

/// List every effective validation. Mirrors `listEffectiveValidations`.
pub fn list_effective_validations(store: Store) -> List(ValidationRecord) {
  let ordered_ids =
    list.append(
      store.base_state.validation_order,
      store.staged_state.validation_order,
    )
    |> dedupe_strings()
  let ordered_records =
    list.filter_map(ordered_ids, fn(id) {
      case get_effective_validation_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  let ordered_set = list_to_set(ordered_ids)
  let merged =
    dict.merge(store.base_state.validations, store.staged_state.validations)
  let unordered_ids =
    dict.keys(merged)
    |> list.filter(fn(id) { !dict_has(ordered_set, id) })
    |> list.sort(string_compare)
  let unordered_records =
    list.filter_map(unordered_ids, fn(id) {
      case get_effective_validation_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  list.append(ordered_records, unordered_records)
}

/// Stage a `CartTransformRecord`. Mirrors `upsertStagedCartTransform`.
pub fn upsert_staged_cart_transform(
  store: Store,
  record: CartTransformRecord,
) -> #(CartTransformRecord, Store) {
  let staged = store.staged_state
  let base = store.base_state
  let already_known =
    list.contains(base.cart_transform_order, record.id)
    || list.contains(staged.cart_transform_order, record.id)
  let new_order = case already_known {
    True -> staged.cart_transform_order
    False -> list.append(staged.cart_transform_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      cart_transforms: dict.insert(staged.cart_transforms, record.id, record),
      cart_transform_order: new_order,
      deleted_cart_transform_ids: dict.delete(
        staged.deleted_cart_transform_ids,
        record.id,
      ),
    )
  #(record, Store(..store, staged_state: new_staged))
}

/// Mark a cart-transform id as deleted. Mirrors
/// `deleteStagedCartTransform`.
pub fn delete_staged_cart_transform(store: Store, id: String) -> Store {
  let staged = store.staged_state
  let new_staged =
    StagedState(
      ..staged,
      cart_transforms: dict.delete(staged.cart_transforms, id),
      deleted_cart_transform_ids: dict.insert(
        staged.deleted_cart_transform_ids,
        id,
        True,
      ),
    )
  Store(..store, staged_state: new_staged)
}

/// Look up an effective cart-transform. Mirrors
/// `getEffectiveCartTransformById`.
pub fn get_effective_cart_transform_by_id(
  store: Store,
  id: String,
) -> Option(CartTransformRecord) {
  let deleted =
    dict_has(store.base_state.deleted_cart_transform_ids, id)
    || dict_has(store.staged_state.deleted_cart_transform_ids, id)
  case deleted {
    True -> None
    False ->
      case dict.get(store.staged_state.cart_transforms, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.cart_transforms, id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

/// List every effective cart-transform. Mirrors
/// `listEffectiveCartTransforms`.
pub fn list_effective_cart_transforms(
  store: Store,
) -> List(CartTransformRecord) {
  let ordered_ids =
    list.append(
      store.base_state.cart_transform_order,
      store.staged_state.cart_transform_order,
    )
    |> dedupe_strings()
  let ordered_records =
    list.filter_map(ordered_ids, fn(id) {
      case get_effective_cart_transform_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  let ordered_set = list_to_set(ordered_ids)
  let merged =
    dict.merge(
      store.base_state.cart_transforms,
      store.staged_state.cart_transforms,
    )
  let unordered_ids =
    dict.keys(merged)
    |> list.filter(fn(id) { !dict_has(ordered_set, id) })
    |> list.sort(string_compare)
  let unordered_records =
    list.filter_map(unordered_ids, fn(id) {
      case get_effective_cart_transform_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  list.append(ordered_records, unordered_records)
}

/// Stage the singleton tax-app configuration. Mirrors
/// `setStagedTaxAppConfiguration`. The TS proxy permits one
/// configuration per shop; here it lives as `Option` on staged state.
pub fn set_staged_tax_app_configuration(
  store: Store,
  record: TaxAppConfigurationRecord,
) -> Store {
  let staged = store.staged_state
  let new_staged = StagedState(..staged, tax_app_configuration: Some(record))
  Store(..store, staged_state: new_staged)
}

/// Read the effective tax-app configuration (staged-over-base).
/// Mirrors `getEffectiveTaxAppConfiguration`.
pub fn get_effective_tax_app_configuration(
  store: Store,
) -> Option(TaxAppConfigurationRecord) {
  case store.staged_state.tax_app_configuration {
    Some(record) -> Some(record)
    None -> store.base_state.tax_app_configuration
  }
}

//// Store operations for order and draft order records.

import gleam/dict
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/order
import gleam/string
import shopify_draft_proxy/shopify/resource_ids
import shopify_draft_proxy/state/store/shared.{append_unique_id, dedupe_strings}
import shopify_draft_proxy/state/store/types.{
  type Store, BaseState, StagedState, Store,
}
import shopify_draft_proxy/state/types.{
  type AbandonedCheckoutRecord, type AbandonmentDeliveryActivityRecord,
  type AbandonmentRecord, type DraftOrderRecord,
  type DraftOrderVariantCatalogRecord, type OrderMandatePaymentRecord,
  type OrderRecord, AbandonmentRecord,
} as types_mod

// ---------------------------------------------------------------------------
// Orders / abandonments slice
// ---------------------------------------------------------------------------

pub fn upsert_base_abandoned_checkouts(
  store: Store,
  records: List(AbandonedCheckoutRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        abandoned_checkouts: dict.insert(
          base.abandoned_checkouts,
          record.id,
          record,
        ),
        abandoned_checkout_order: append_unique_id(
          base.abandoned_checkout_order,
          record.id,
        ),
      ),
    )
  })
}

pub fn upsert_base_abandonments(
  store: Store,
  records: List(AbandonmentRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        abandonments: dict.insert(base.abandonments, record.id, record),
        abandonment_order: append_unique_id(base.abandonment_order, record.id),
      ),
    )
  })
}

pub fn upsert_base_draft_orders(
  store: Store,
  records: List(DraftOrderRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    let staged = acc.staged_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        draft_orders: dict.insert(base.draft_orders, record.id, record),
        draft_order_order: append_unique_id(base.draft_order_order, record.id),
        deleted_draft_order_ids: dict.delete(
          base.deleted_draft_order_ids,
          record.id,
        ),
      ),
      staged_state: StagedState(
        ..staged,
        deleted_draft_order_ids: dict.delete(
          staged.deleted_draft_order_ids,
          record.id,
        ),
      ),
    )
  })
}

pub fn upsert_base_draft_order_variant_catalog(
  store: Store,
  records: List(DraftOrderVariantCatalogRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        draft_order_variant_catalog: dict.insert(
          base.draft_order_variant_catalog,
          record.variant_id,
          record,
        ),
      ),
    )
  })
}

pub fn upsert_base_orders(store: Store, records: List(OrderRecord)) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    let staged = acc.staged_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        orders: dict.insert(base.orders, record.id, record),
        order_order: append_unique_id(base.order_order, record.id),
        deleted_order_ids: dict.delete(base.deleted_order_ids, record.id),
      ),
      staged_state: StagedState(
        ..staged,
        deleted_order_ids: dict.delete(staged.deleted_order_ids, record.id),
      ),
    )
  })
}

pub fn stage_order(store: Store, record: OrderRecord) -> Store {
  let staged = store.staged_state
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      orders: dict.insert(staged.orders, record.id, record),
      order_order: append_unique_id(staged.order_order, record.id),
      deleted_order_ids: dict.delete(staged.deleted_order_ids, record.id),
    ),
  )
}

pub fn stage_draft_order(store: Store, record: DraftOrderRecord) -> Store {
  let staged = store.staged_state
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      draft_orders: dict.insert(staged.draft_orders, record.id, record),
      draft_order_order: append_unique_id(staged.draft_order_order, record.id),
      deleted_draft_order_ids: dict.delete(
        staged.deleted_draft_order_ids,
        record.id,
      ),
    ),
  )
}

pub fn delete_staged_draft_order(store: Store, id: String) -> Store {
  let staged = store.staged_state
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      draft_orders: dict.delete(staged.draft_orders, id),
      deleted_draft_order_ids: dict.insert(
        staged.deleted_draft_order_ids,
        id,
        True,
      ),
    ),
  )
}

pub fn delete_staged_order(store: Store, id: String) -> Store {
  let staged = store.staged_state
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      orders: dict.delete(staged.orders, id),
      deleted_order_ids: dict.insert(staged.deleted_order_ids, id, True),
    ),
  )
}

pub fn get_abandoned_checkout_by_id(
  store: Store,
  id: String,
) -> Option(AbandonedCheckoutRecord) {
  case dict.get(store.staged_state.abandoned_checkouts, id) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(store.base_state.abandoned_checkouts, id) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

pub fn get_draft_order_by_id(
  store: Store,
  id: String,
) -> Option(DraftOrderRecord) {
  case dict.get(store.staged_state.deleted_draft_order_ids, id) {
    Ok(True) -> None
    _ ->
      case dict.get(store.staged_state.draft_orders, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.deleted_draft_order_ids, id) {
            Ok(True) -> None
            _ ->
              case dict.get(store.base_state.draft_orders, id) {
                Ok(record) -> Some(record)
                Error(_) -> None
              }
          }
      }
  }
}

pub fn get_order_by_id(store: Store, id: String) -> Option(OrderRecord) {
  case dict.get(store.staged_state.deleted_order_ids, id) {
    Ok(True) -> None
    _ ->
      case dict.get(store.staged_state.orders, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.deleted_order_ids, id) {
            Ok(True) -> None
            _ ->
              case dict.get(store.base_state.orders, id) {
                Ok(record) -> Some(record)
                Error(_) -> None
              }
          }
      }
  }
}

pub fn list_effective_draft_orders(store: Store) -> List(DraftOrderRecord) {
  let ordered_ids =
    list.append(
      store.base_state.draft_order_order,
      store.staged_state.draft_order_order,
    )
  let ordered =
    ordered_ids
    |> dedupe_strings()
    |> list.filter_map(fn(id) {
      case get_draft_order_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  let unordered =
    dict.values(store.base_state.draft_orders)
    |> list.append(dict.values(store.staged_state.draft_orders))
    |> list.filter(fn(record) { !list.contains(ordered_ids, record.id) })
  list.append(ordered, unordered) |> dedupe_draft_orders()
}

pub fn list_effective_orders(store: Store) -> List(OrderRecord) {
  let ordered_ids =
    list.append(store.base_state.order_order, store.staged_state.order_order)
  let ordered =
    ordered_ids
    |> dedupe_strings()
    |> list.filter_map(fn(id) {
      case get_order_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  let unordered =
    dict.values(store.base_state.orders)
    |> list.append(dict.values(store.staged_state.orders))
    |> list.filter(fn(record) { !list.contains(ordered_ids, record.id) })
  list.append(ordered, unordered) |> dedupe_orders()
}

pub fn get_order_mandate_payment(
  store: Store,
  order_id: String,
  idempotency_key: String,
) -> Option(OrderMandatePaymentRecord) {
  let key = order_mandate_payment_key(order_id, idempotency_key)
  case dict.get(store.staged_state.order_mandate_payments, key) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(store.base_state.order_mandate_payments, key) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

pub fn upsert_staged_order_mandate_payment(
  store: Store,
  record: OrderMandatePaymentRecord,
) -> Store {
  let key = order_mandate_payment_key(record.order_id, record.idempotency_key)
  Store(
    ..store,
    staged_state: StagedState(
      ..store.staged_state,
      order_mandate_payments: dict.insert(
        store.staged_state.order_mandate_payments,
        key,
        record,
      ),
    ),
  )
}

fn order_mandate_payment_key(
  order_id: String,
  idempotency_key: String,
) -> String {
  order_id <> "::" <> idempotency_key
}

pub fn get_draft_order_variant_catalog_by_id(
  store: Store,
  variant_id: String,
) -> Option(DraftOrderVariantCatalogRecord) {
  case dict.get(store.staged_state.draft_order_variant_catalog, variant_id) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(store.base_state.draft_order_variant_catalog, variant_id) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

pub fn get_abandonment_by_id(
  store: Store,
  id: String,
) -> Option(AbandonmentRecord) {
  case dict.get(store.staged_state.abandonments, id) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(store.base_state.abandonments, id) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

pub fn get_abandonment_by_abandoned_checkout_id(
  store: Store,
  checkout_id: String,
) -> Option(AbandonmentRecord) {
  case
    list_effective_abandonments(store)
    |> list.find(fn(record) {
      record.abandoned_checkout_id == Some(checkout_id)
    })
  {
    Ok(record) -> Some(record)
    Error(_) -> None
  }
}

pub fn list_effective_abandoned_checkouts(
  store: Store,
) -> List(AbandonedCheckoutRecord) {
  let ordered_ids =
    list.append(
      store.base_state.abandoned_checkout_order,
      store.staged_state.abandoned_checkout_order,
    )
  let ordered =
    ordered_ids
    |> dedupe_strings()
    |> list.filter_map(fn(id) {
      case get_abandoned_checkout_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  let unordered =
    dict.values(store.base_state.abandoned_checkouts)
    |> list.append(dict.values(store.staged_state.abandoned_checkouts))
    |> list.filter(fn(record) { !list.contains(ordered_ids, record.id) })
  list.append(ordered, unordered)
  |> dedupe_abandoned_checkouts()
  |> list.sort(by: compare_abandoned_checkouts)
}

pub fn list_effective_abandonments(store: Store) -> List(AbandonmentRecord) {
  let ordered_ids =
    list.append(
      store.base_state.abandonment_order,
      store.staged_state.abandonment_order,
    )
  let ordered =
    ordered_ids
    |> dedupe_strings()
    |> list.filter_map(fn(id) {
      case get_abandonment_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  let unordered =
    dict.values(store.base_state.abandonments)
    |> list.append(dict.values(store.staged_state.abandonments))
    |> list.filter(fn(record) { !list.contains(ordered_ids, record.id) })
  list.append(ordered, unordered)
  |> dedupe_abandonments()
  |> list.sort(by: compare_abandonments)
}

pub fn unassociate_abandoned_checkouts_from_order(
  store: Store,
  order_id: String,
) -> Store {
  list_effective_abandoned_checkouts(store)
  |> list.filter(fn(record) {
    abandoned_checkout_references_order(record.data, order_id)
  })
  |> list.fold(store, fn(acc, record) {
    stage_abandoned_checkout(
      acc,
      types_mod.AbandonedCheckoutRecord(
        ..record,
        data: record.data
          |> captured_object_upsert("orderId", types_mod.CapturedNull)
          |> captured_object_upsert("order", types_mod.CapturedNull),
      ),
    )
  })
}

pub fn stage_abandonment_delivery_activity(
  store: Store,
  abandonment_id: String,
  activity: AbandonmentDeliveryActivityRecord,
) -> #(Store, Option(AbandonmentRecord)) {
  case get_abandonment_by_id(store, abandonment_id) {
    None -> #(store, None)
    Some(record) -> {
      let updated_data =
        captured_object_upsert(
          captured_object_upsert(
            record.data,
            "emailState",
            types_mod.CapturedString(activity.delivery_status),
          ),
          "emailSentAt",
          optional_captured_string(activity.delivered_at),
        )
      let updated =
        AbandonmentRecord(
          ..record,
          data: updated_data,
          delivery_activities: dict.insert(
            record.delivery_activities,
            activity.marketing_activity_id,
            activity,
          ),
        )
      let staged = store.staged_state
      #(
        Store(
          ..store,
          staged_state: StagedState(
            ..staged,
            abandonments: dict.insert(staged.abandonments, updated.id, updated),
            abandonment_order: append_unique_id(
              staged.abandonment_order,
              updated.id,
            ),
          ),
        ),
        Some(updated),
      )
    }
  }
}

fn stage_abandoned_checkout(
  store: Store,
  record: AbandonedCheckoutRecord,
) -> Store {
  let staged = store.staged_state
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      abandoned_checkouts: dict.insert(
        staged.abandoned_checkouts,
        record.id,
        record,
      ),
      abandoned_checkout_order: append_unique_id(
        staged.abandoned_checkout_order,
        record.id,
      ),
    ),
  )
}

fn abandoned_checkout_references_order(
  data: types_mod.CapturedJsonValue,
  order_id: String,
) -> Bool {
  captured_string_field(data, "orderId") == order_id
  || captured_nested_order_id(data, "order") == order_id
}

fn captured_nested_order_id(
  value: types_mod.CapturedJsonValue,
  key: String,
) -> String {
  case value {
    types_mod.CapturedObject(fields) -> {
      case list.find(fields, fn(pair) { pair.0 == key }) {
        Ok(#(_, nested)) -> captured_string_field(nested, "id")
        _ -> ""
      }
    }
    _ -> ""
  }
}

fn optional_captured_string(
  value: Option(String),
) -> types_mod.CapturedJsonValue {
  case value {
    Some(value) -> types_mod.CapturedString(value)
    None -> types_mod.CapturedNull
  }
}

fn captured_object_upsert(
  value: types_mod.CapturedJsonValue,
  key: String,
  field_value: types_mod.CapturedJsonValue,
) -> types_mod.CapturedJsonValue {
  case value {
    types_mod.CapturedObject(fields) ->
      types_mod.CapturedObject(upsert_captured_field(fields, key, field_value))
    _ -> value
  }
}

fn upsert_captured_field(
  fields: List(#(String, types_mod.CapturedJsonValue)),
  key: String,
  value: types_mod.CapturedJsonValue,
) -> List(#(String, types_mod.CapturedJsonValue)) {
  case fields {
    [] -> [#(key, value)]
    [first, ..rest] -> {
      let #(field_key, _) = first
      case field_key == key {
        True -> [#(key, value), ..rest]
        False -> [first, ..upsert_captured_field(rest, key, value)]
      }
    }
  }
}

fn captured_string_field(
  value: types_mod.CapturedJsonValue,
  key: String,
) -> String {
  case value {
    types_mod.CapturedObject(fields) -> {
      case list.find(fields, fn(pair) { pair.0 == key }) {
        Ok(#(_, types_mod.CapturedString(value))) -> value
        _ -> ""
      }
    }
    _ -> ""
  }
}

fn compare_abandoned_checkouts(
  left: AbandonedCheckoutRecord,
  right: AbandonedCheckoutRecord,
) -> order.Order {
  case
    string.compare(
      captured_string_field(right.data, "createdAt"),
      captured_string_field(left.data, "createdAt"),
    )
  {
    order.Eq -> resource_ids.compare_shopify_resource_ids(right.id, left.id)
    other -> other
  }
}

fn compare_abandonments(
  left: AbandonmentRecord,
  right: AbandonmentRecord,
) -> order.Order {
  case
    string.compare(
      captured_string_field(right.data, "createdAt"),
      captured_string_field(left.data, "createdAt"),
    )
  {
    order.Eq -> resource_ids.compare_shopify_resource_ids(right.id, left.id)
    other -> other
  }
}

fn dedupe_abandoned_checkouts(
  records: List(AbandonedCheckoutRecord),
) -> List(AbandonedCheckoutRecord) {
  let initial: List(AbandonedCheckoutRecord) = []
  records
  |> list.fold(initial, fn(acc, record) {
    case list.any(acc, fn(existing) { existing.id == record.id }) {
      True -> acc
      False -> list.append(acc, [record])
    }
  })
}

fn dedupe_abandonments(
  records: List(AbandonmentRecord),
) -> List(AbandonmentRecord) {
  let initial: List(AbandonmentRecord) = []
  records
  |> list.fold(initial, fn(acc, record) {
    case list.any(acc, fn(existing) { existing.id == record.id }) {
      True -> acc
      False -> list.append(acc, [record])
    }
  })
}

fn dedupe_draft_orders(
  records: List(DraftOrderRecord),
) -> List(DraftOrderRecord) {
  let initial: List(DraftOrderRecord) = []
  records
  |> list.fold(initial, fn(acc, record) {
    case list.any(acc, fn(existing) { existing.id == record.id }) {
      True -> acc
      False -> list.append(acc, [record])
    }
  })
}

fn dedupe_orders(records: List(OrderRecord)) -> List(OrderRecord) {
  let initial: List(OrderRecord) = []
  records
  |> list.fold(initial, fn(acc, record) {
    case list.any(acc, fn(existing) { existing.id == record.id }) {
      True -> acc
      False -> list.append(acc, [record])
    }
  })
}

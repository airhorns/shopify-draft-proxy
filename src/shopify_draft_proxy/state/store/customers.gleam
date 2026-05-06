//// Store operations for customer records.

import gleam/dict
import gleam/int
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/order
import gleam/result
import gleam/string
import shopify_draft_proxy/shopify/resource_ids
import shopify_draft_proxy/state/store/b2b
import shopify_draft_proxy/state/store/shared.{
  append_unique_id, append_unique_ids, dedupe_strings, dict_has, list_to_set,
  string_compare,
}
import shopify_draft_proxy/state/store/types.{
  type Store, BaseState, StagedState, Store,
}
import shopify_draft_proxy/state/types.{
  type CustomerAccountPageRecord, type CustomerAddressRecord,
  type CustomerCatalogConnectionRecord, type CustomerCatalogPageInfoRecord,
  type CustomerDataErasureRequestRecord, type CustomerEventSummaryRecord,
  type CustomerMergeRequestRecord, type CustomerMetafieldRecord,
  type CustomerOrderSummaryRecord, type CustomerPaymentMethodRecord,
  type CustomerPaymentMethodUpdateUrlRecord, type CustomerRecord,
  type PaymentCustomizationRecord, type PaymentReminderSendRecord,
  type PaymentScheduleRecord, type PaymentTermsRecord,
  type StoreCreditAccountRecord, type StoreCreditAccountTransactionRecord,
} as _

// ---------------------------------------------------------------------------
// Customers slice
// ---------------------------------------------------------------------------

pub fn upsert_base_customers(
  store: Store,
  records: List(CustomerRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        customers: dict.insert(base.customers, record.id, record),
        customer_order: append_unique_id(base.customer_order, record.id),
        deleted_customer_ids: dict.delete(base.deleted_customer_ids, record.id),
        merged_customer_ids: dict.delete(base.merged_customer_ids, record.id),
      ),
    )
  })
}

pub fn set_base_customer_catalog_connection(
  store: Store,
  key: String,
  connection: CustomerCatalogConnectionRecord,
) -> Store {
  let base = store.base_state
  Store(
    ..store,
    base_state: BaseState(
      ..base,
      customer_catalog_connections: dict.insert(
        base.customer_catalog_connections,
        key,
        connection,
      ),
    ),
  )
}

pub fn get_base_customer_catalog_connection(
  store: Store,
  key: String,
) -> Option(CustomerCatalogConnectionRecord) {
  case dict.get(store.base_state.customer_catalog_connections, key) {
    Ok(connection) -> Some(connection)
    Error(_) -> None
  }
}

pub fn stage_create_customer(
  store: Store,
  record: CustomerRecord,
) -> #(CustomerRecord, Store) {
  let staged = store.staged_state
  let already_known =
    list.contains(store.base_state.customer_order, record.id)
    || list.contains(staged.customer_order, record.id)
  let new_order = case already_known {
    True -> staged.customer_order
    False -> list.append(staged.customer_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      customers: dict.insert(staged.customers, record.id, record),
      customer_order: new_order,
      deleted_customer_ids: dict.delete(staged.deleted_customer_ids, record.id),
      merged_customer_ids: dict.delete(staged.merged_customer_ids, record.id),
    )
  #(record, Store(..store, staged_state: new_staged))
}

pub fn stage_update_customer(
  store: Store,
  record: CustomerRecord,
) -> #(CustomerRecord, Store) {
  stage_create_customer(store, record)
}

pub fn stage_delete_customer(store: Store, customer_id: String) -> Store {
  let staged = store.staged_state
  let staged_addresses =
    dict.filter(staged.customer_addresses, fn(_id, address) {
      address.customer_id != customer_id
    })
  let deleted_address_ids =
    dict.to_list(store.base_state.customer_addresses)
    |> list.fold(staged.deleted_customer_address_ids, fn(acc, pair) {
      let #(id, address) = pair
      case address.customer_id == customer_id {
        True -> dict.insert(acc, id, True)
        False -> acc
      }
    })
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      customers: dict.delete(staged.customers, customer_id),
      customer_addresses: staged_addresses,
      deleted_customer_address_ids: deleted_address_ids,
      deleted_customer_ids: dict.insert(
        staged.deleted_customer_ids,
        customer_id,
        True,
      ),
      merged_customer_ids: dict.delete(staged.merged_customer_ids, customer_id),
    ),
  )
}

pub fn get_effective_customer_by_id(
  store: Store,
  customer_id: String,
) -> Option(CustomerRecord) {
  case dict.get(store.staged_state.deleted_customer_ids, customer_id) {
    Ok(True) -> None
    _ ->
      case dict.get(store.staged_state.customers, customer_id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.customers, customer_id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

pub fn list_effective_customers(store: Store) -> List(CustomerRecord) {
  let ordered_ids =
    list.append(
      store.base_state.customer_order,
      store.staged_state.customer_order,
    )
    |> dedupe_strings()
  let ordered_records =
    list.filter_map(ordered_ids, fn(id) {
      case get_effective_customer_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  let ordered_set = list_to_set(ordered_ids)
  let merged =
    dict.merge(store.base_state.customers, store.staged_state.customers)
  let unordered_ids =
    dict.keys(merged)
    |> list.filter(fn(id) { !dict_has(ordered_set, id) })
    |> list.sort(string_compare)
  let unordered_records =
    list.filter_map(unordered_ids, fn(id) {
      case get_effective_customer_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  list.append(ordered_records, unordered_records)
}

pub fn upsert_base_customer_addresses(
  store: Store,
  records: List(CustomerAddressRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        customer_addresses: dict.insert(
          base.customer_addresses,
          record.id,
          record,
        ),
        customer_address_order: append_unique_id(
          base.customer_address_order,
          record.id,
        ),
        deleted_customer_address_ids: dict.delete(
          base.deleted_customer_address_ids,
          record.id,
        ),
      ),
    )
  })
}

pub fn stage_upsert_customer_address(
  store: Store,
  record: CustomerAddressRecord,
) -> #(CustomerAddressRecord, Store) {
  let staged = store.staged_state
  let already_known =
    list.contains(store.base_state.customer_address_order, record.id)
    || list.contains(staged.customer_address_order, record.id)
  let new_order = case already_known {
    True -> staged.customer_address_order
    False -> list.append(staged.customer_address_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      customer_addresses: dict.insert(
        staged.customer_addresses,
        record.id,
        record,
      ),
      customer_address_order: new_order,
      deleted_customer_address_ids: dict.delete(
        staged.deleted_customer_address_ids,
        record.id,
      ),
    )
  #(record, Store(..store, staged_state: new_staged))
}

pub fn stage_delete_customer_address(
  store: Store,
  address_id: String,
) -> Store {
  let staged = store.staged_state
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      customer_addresses: dict.delete(staged.customer_addresses, address_id),
      deleted_customer_address_ids: dict.insert(
        staged.deleted_customer_address_ids,
        address_id,
        True,
      ),
    ),
  )
}

pub fn get_effective_customer_address_by_id(
  store: Store,
  address_id: String,
) -> Option(CustomerAddressRecord) {
  case dict.get(store.staged_state.deleted_customer_address_ids, address_id) {
    Ok(True) -> None
    _ ->
      case dict.get(store.staged_state.customer_addresses, address_id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.customer_addresses, address_id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

pub fn list_effective_customer_addresses(
  store: Store,
  customer_id: String,
) -> List(CustomerAddressRecord) {
  case dict.get(store.staged_state.deleted_customer_ids, customer_id) {
    Ok(True) -> []
    _ -> {
      let ids =
        list.append(
          store.base_state.customer_address_order,
          store.staged_state.customer_address_order,
        )
        |> dedupe_strings()
      let from_order =
        list.filter_map(ids, fn(id) {
          case get_effective_customer_address_by_id(store, id) {
            Some(address) ->
              case address.customer_id == customer_id {
                True -> Ok(address)
                False -> Error(Nil)
              }
            None -> Error(Nil)
          }
        })
      let ordered_set = list_to_set(ids)
      let merged =
        dict.merge(
          store.base_state.customer_addresses,
          store.staged_state.customer_addresses,
        )
      let unordered =
        dict.keys(merged)
        |> list.filter(fn(id) { !dict_has(ordered_set, id) })
        |> list.sort(string_compare)
        |> list.filter_map(fn(id) {
          case get_effective_customer_address_by_id(store, id) {
            Some(address) ->
              case address.customer_id == customer_id {
                True -> Ok(address)
                False -> Error(Nil)
              }
            None -> Error(Nil)
          }
        })
      let effective = list.append(from_order, unordered)
      case list.any(effective, fn(address) { address.position < 0 }) {
        True ->
          list.sort(effective, fn(a, b) {
            case a.position < 0, b.position < 0 {
              True, True -> int.compare(a.position, b.position)
              True, False -> order.Lt
              False, True -> order.Gt
              False, False -> order.Eq
            }
          })
        False -> effective
      }
    }
  }
}

pub fn upsert_base_customer_order_summaries(
  store: Store,
  records: List(CustomerOrderSummaryRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        customer_order_summaries: dict.insert(
          base.customer_order_summaries,
          record.id,
          record,
        ),
      ),
    )
  })
}

pub fn stage_customer_order_summary(
  store: Store,
  record: CustomerOrderSummaryRecord,
) -> Store {
  let staged = store.staged_state
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      customer_order_summaries: dict.insert(
        staged.customer_order_summaries,
        record.id,
        record,
      ),
    ),
  )
}

pub fn get_effective_customer_order_summary_by_id(
  store: Store,
  order_id: String,
) -> Option(CustomerOrderSummaryRecord) {
  case dict.get(store.staged_state.customer_order_summaries, order_id) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(store.base_state.customer_order_summaries, order_id) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

pub fn list_effective_customer_order_summaries(
  store: Store,
  customer_id: String,
) -> List(CustomerOrderSummaryRecord) {
  dict.keys(dict.merge(
    store.base_state.customer_order_summaries,
    store.staged_state.customer_order_summaries,
  ))
  |> list.sort(string_compare)
  |> list.filter_map(fn(id) {
    case get_effective_customer_order_summary_by_id(store, id) {
      Some(record) ->
        case record.customer_id == Some(customer_id) {
          True -> Ok(record)
          False -> Error(Nil)
        }
      None -> Error(Nil)
    }
  })
}

pub fn set_base_customer_order_connection_page_info(
  store: Store,
  customer_id: String,
  page_info: CustomerCatalogPageInfoRecord,
) -> Store {
  let base = store.base_state
  Store(
    ..store,
    base_state: BaseState(
      ..base,
      customer_order_connection_page_infos: dict.insert(
        base.customer_order_connection_page_infos,
        customer_id,
        page_info,
      ),
    ),
  )
}

pub fn get_effective_customer_order_connection_page_info(
  store: Store,
  customer_id: String,
) -> Option(CustomerCatalogPageInfoRecord) {
  case
    dict.get(
      store.staged_state.customer_order_connection_page_infos,
      customer_id,
    )
  {
    Ok(info) -> Some(info)
    Error(_) ->
      case
        dict.get(
          store.base_state.customer_order_connection_page_infos,
          customer_id,
        )
      {
        Ok(info) -> Some(info)
        Error(_) -> None
      }
  }
}

pub fn upsert_base_customer_event_summaries(
  store: Store,
  records: List(CustomerEventSummaryRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        customer_event_summaries: dict.insert(
          base.customer_event_summaries,
          record.id,
          record,
        ),
      ),
    )
  })
}

pub fn list_effective_customer_event_summaries(
  store: Store,
  customer_id: String,
) -> List(CustomerEventSummaryRecord) {
  dict.values(dict.merge(
    store.base_state.customer_event_summaries,
    store.staged_state.customer_event_summaries,
  ))
  |> list.filter(fn(event) { event.customer_id == customer_id })
  |> list.sort(fn(a, b) { string.compare(a.id, b.id) })
}

pub fn set_base_customer_event_connection_page_info(
  store: Store,
  customer_id: String,
  page_info: CustomerCatalogPageInfoRecord,
) -> Store {
  let base = store.base_state
  Store(
    ..store,
    base_state: BaseState(
      ..base,
      customer_event_connection_page_infos: dict.insert(
        base.customer_event_connection_page_infos,
        customer_id,
        page_info,
      ),
    ),
  )
}

pub fn get_effective_customer_event_connection_page_info(
  store: Store,
  customer_id: String,
) -> Option(CustomerCatalogPageInfoRecord) {
  case
    dict.get(
      store.staged_state.customer_event_connection_page_infos,
      customer_id,
    )
  {
    Ok(info) -> Some(info)
    Error(_) ->
      case
        dict.get(
          store.base_state.customer_event_connection_page_infos,
          customer_id,
        )
      {
        Ok(info) -> Some(info)
        Error(_) -> None
      }
  }
}

pub fn upsert_base_customer_last_orders(
  store: Store,
  records: List(#(String, CustomerOrderSummaryRecord)),
) -> Store {
  list.fold(records, store, fn(acc, pair) {
    let #(customer_id, record) = pair
    let base = acc.base_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        customer_last_orders: dict.insert(
          base.customer_last_orders,
          customer_id,
          record,
        ),
      ),
    )
  })
}

pub fn get_effective_customer_last_order(
  store: Store,
  customer_id: String,
) -> Option(CustomerOrderSummaryRecord) {
  case dict.get(store.staged_state.customer_last_orders, customer_id) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(store.base_state.customer_last_orders, customer_id) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

pub fn stage_customer_metafields(
  store: Store,
  customer_id: String,
  records: List(CustomerMetafieldRecord),
) -> Store {
  let staged_without_customer =
    dict.filter(store.staged_state.customer_metafields, fn(_id, metafield) {
      metafield.customer_id != customer_id
    })
  let new_metafields =
    list.fold(records, staged_without_customer, fn(acc, metafield) {
      dict.insert(acc, metafield.id, metafield)
    })
  Store(
    ..store,
    staged_state: StagedState(
      ..store.staged_state,
      customer_metafields: new_metafields,
    ),
  )
}

pub fn get_effective_metafields_by_customer_id(
  store: Store,
  customer_id: String,
) -> List(CustomerMetafieldRecord) {
  let staged =
    dict.values(store.staged_state.customer_metafields)
    |> list.filter(fn(m) { m.customer_id == customer_id })
    |> list.sort(fn(a, b) { string.compare(a.id, b.id) })
  case staged {
    [] ->
      dict.values(store.base_state.customer_metafields)
      |> list.filter(fn(m) { m.customer_id == customer_id })
      |> list.sort(fn(a, b) { string.compare(a.id, b.id) })
    _ -> staged
  }
}

pub fn stage_customer_payment_method(
  store: Store,
  record: CustomerPaymentMethodRecord,
) -> Store {
  Store(
    ..store,
    staged_state: StagedState(
      ..store.staged_state,
      customer_payment_methods: dict.insert(
        store.staged_state.customer_payment_methods,
        record.id,
        record,
      ),
      deleted_customer_payment_method_ids: dict.delete(
        store.staged_state.deleted_customer_payment_method_ids,
        record.id,
      ),
    ),
  )
}

pub fn upsert_base_customer_payment_methods(
  store: Store,
  records: List(CustomerPaymentMethodRecord),
) -> Store {
  list.fold(records, store, fn(current, record) {
    Store(
      ..current,
      base_state: BaseState(
        ..current.base_state,
        customer_payment_methods: dict.insert(
          current.base_state.customer_payment_methods,
          record.id,
          record,
        ),
        deleted_customer_payment_method_ids: dict.delete(
          current.base_state.deleted_customer_payment_method_ids,
          record.id,
        ),
      ),
      staged_state: StagedState(
        ..current.staged_state,
        deleted_customer_payment_method_ids: dict.delete(
          current.staged_state.deleted_customer_payment_method_ids,
          record.id,
        ),
      ),
    )
  })
}

pub fn stage_customer_payment_method_update_url(
  store: Store,
  record: CustomerPaymentMethodUpdateUrlRecord,
) -> Store {
  Store(
    ..store,
    staged_state: StagedState(
      ..store.staged_state,
      customer_payment_method_update_urls: dict.insert(
        store.staged_state.customer_payment_method_update_urls,
        record.id,
        record,
      ),
    ),
  )
}

pub fn stage_payment_reminder_send(
  store: Store,
  record: PaymentReminderSendRecord,
) -> Store {
  Store(
    ..store,
    staged_state: StagedState(
      ..store.staged_state,
      payment_reminder_sends: dict.insert(
        store.staged_state.payment_reminder_sends,
        record.id,
        record,
      ),
    ),
  )
}

pub fn get_effective_customer_payment_method_by_id(
  store: Store,
  payment_method_id: String,
  show_revoked: Bool,
) -> Option(CustomerPaymentMethodRecord) {
  case
    dict.get(
      store.staged_state.deleted_customer_payment_method_ids,
      payment_method_id,
    )
  {
    Ok(True) -> None
    _ -> {
      let found = case
        dict.get(store.staged_state.customer_payment_methods, payment_method_id)
      {
        Ok(record) -> Some(record)
        Error(_) ->
          case
            dict.get(
              store.base_state.customer_payment_methods,
              payment_method_id,
            )
          {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
      case found {
        Some(record) ->
          case
            get_effective_customer_by_id(store, record.customer_id),
            record.revoked_at
          {
            None, _ -> None
            _, Some(_) if !show_revoked -> None
            _, _ -> Some(record)
          }
        None -> None
      }
    }
  }
}

pub fn list_effective_customer_payment_methods(
  store: Store,
  customer_id: String,
  show_revoked: Bool,
) -> List(CustomerPaymentMethodRecord) {
  let ids =
    dict.keys(dict.merge(
      store.base_state.customer_payment_methods,
      store.staged_state.customer_payment_methods,
    ))
    |> list.sort(string_compare)
  list.filter_map(ids, fn(id) {
    case get_effective_customer_payment_method_by_id(store, id, show_revoked) {
      Some(record) ->
        case record.customer_id == customer_id {
          True -> Ok(record)
          False -> Error(Nil)
        }
      None -> Error(Nil)
    }
  })
}

pub fn upsert_base_payment_customizations(
  store: Store,
  records: List(PaymentCustomizationRecord),
) -> Store {
  list.fold(records, store, fn(current, record) {
    Store(
      ..current,
      base_state: BaseState(
        ..current.base_state,
        payment_customizations: dict.insert(
          current.base_state.payment_customizations,
          record.id,
          record,
        ),
        payment_customization_order: append_unique_id(
          current.base_state.payment_customization_order,
          record.id,
        ),
        deleted_payment_customization_ids: dict.delete(
          current.base_state.deleted_payment_customization_ids,
          record.id,
        ),
      ),
      staged_state: StagedState(
        ..current.staged_state,
        deleted_payment_customization_ids: dict.delete(
          current.staged_state.deleted_payment_customization_ids,
          record.id,
        ),
      ),
    )
  })
}

pub fn upsert_staged_payment_customization(
  store: Store,
  record: PaymentCustomizationRecord,
) -> Store {
  let staged_order = case
    list.contains(store.base_state.payment_customization_order, record.id)
    || list.contains(store.staged_state.payment_customization_order, record.id)
  {
    True -> store.staged_state.payment_customization_order
    False ->
      list.append(store.staged_state.payment_customization_order, [record.id])
  }
  Store(
    ..store,
    staged_state: StagedState(
      ..store.staged_state,
      payment_customizations: dict.insert(
        store.staged_state.payment_customizations,
        record.id,
        record,
      ),
      payment_customization_order: staged_order,
      deleted_payment_customization_ids: dict.delete(
        store.staged_state.deleted_payment_customization_ids,
        record.id,
      ),
    ),
  )
}

pub fn delete_staged_payment_customization(store: Store, id: String) -> Store {
  Store(
    ..store,
    staged_state: StagedState(
      ..store.staged_state,
      payment_customizations: dict.delete(
        store.staged_state.payment_customizations,
        id,
      ),
      deleted_payment_customization_ids: dict.insert(
        store.staged_state.deleted_payment_customization_ids,
        id,
        True,
      ),
    ),
  )
}

pub fn get_effective_payment_customization_by_id(
  store: Store,
  id: String,
) -> Option(PaymentCustomizationRecord) {
  case dict.get(store.staged_state.deleted_payment_customization_ids, id) {
    Ok(True) -> None
    _ ->
      case dict.get(store.staged_state.payment_customizations, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.payment_customizations, id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

pub fn list_effective_payment_customizations(
  store: Store,
) -> List(PaymentCustomizationRecord) {
  let ordered_ids =
    append_unique_ids(
      store.base_state.payment_customization_order,
      store.staged_state.payment_customization_order,
    )
  let ordered =
    ordered_ids
    |> list.filter_map(fn(id) {
      case get_effective_payment_customization_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  let unordered =
    dict.values(dict.merge(
      store.base_state.payment_customizations,
      store.staged_state.payment_customizations,
    ))
    |> list.filter(fn(record) {
      !list.contains(ordered_ids, record.id)
      && case
        dict.get(
          store.staged_state.deleted_payment_customization_ids,
          record.id,
        )
      {
        Ok(True) -> False
        _ -> True
      }
    })
    |> list.sort(fn(a, b) {
      resource_ids.compare_shopify_resource_ids(a.id, b.id)
    })
  list.append(ordered, unordered)
}

pub fn has_payment_customizations(store: Store) -> Bool {
  dict.size(store.base_state.payment_customizations) > 0
  || dict.size(store.staged_state.payment_customizations) > 0
  || dict.size(store.staged_state.deleted_payment_customization_ids) > 0
}

pub fn register_payment_terms_owner(store: Store, owner_id: String) -> Store {
  Store(
    ..store,
    base_state: BaseState(
      ..store.base_state,
      payment_terms_owner_ids: dict.insert(
        store.base_state.payment_terms_owner_ids,
        owner_id,
        True,
      ),
    ),
  )
}

pub fn upsert_staged_payment_terms(
  store: Store,
  record: PaymentTermsRecord,
) -> Store {
  Store(
    ..store,
    staged_state: StagedState(
      ..store.staged_state,
      payment_terms: dict.insert(
        store.staged_state.payment_terms,
        record.id,
        record,
      ),
      payment_terms_owner_ids: dict.insert(
        store.staged_state.payment_terms_owner_ids,
        record.owner_id,
        True,
      ),
      payment_terms_by_owner_id: dict.insert(
        store.staged_state.payment_terms_by_owner_id,
        record.owner_id,
        record.id,
      ),
      deleted_payment_terms_ids: dict.delete(
        store.staged_state.deleted_payment_terms_ids,
        record.id,
      ),
    ),
  )
}

pub fn upsert_base_payment_terms(
  store: Store,
  record: PaymentTermsRecord,
) -> Store {
  Store(
    ..store,
    base_state: BaseState(
      ..store.base_state,
      payment_terms: dict.insert(
        store.base_state.payment_terms,
        record.id,
        record,
      ),
      payment_terms_owner_ids: dict.insert(
        store.base_state.payment_terms_owner_ids,
        record.owner_id,
        True,
      ),
      payment_terms_by_owner_id: dict.insert(
        store.base_state.payment_terms_by_owner_id,
        record.owner_id,
        record.id,
      ),
      deleted_payment_terms_ids: dict.delete(
        store.base_state.deleted_payment_terms_ids,
        record.id,
      ),
    ),
    staged_state: StagedState(
      ..store.staged_state,
      deleted_payment_terms_ids: dict.delete(
        store.staged_state.deleted_payment_terms_ids,
        record.id,
      ),
    ),
  )
}

pub fn delete_staged_payment_terms(store: Store, id: String) -> Store {
  let owner_id = case get_effective_payment_terms_by_id(store, id) {
    Some(record) -> Some(record.owner_id)
    None -> None
  }
  let by_owner = case owner_id {
    Some(owner) ->
      dict.delete(store.staged_state.payment_terms_by_owner_id, owner)
    None -> store.staged_state.payment_terms_by_owner_id
  }
  Store(
    ..store,
    staged_state: StagedState(
      ..store.staged_state,
      payment_terms: dict.delete(store.staged_state.payment_terms, id),
      payment_terms_by_owner_id: by_owner,
      deleted_payment_terms_ids: dict.insert(
        store.staged_state.deleted_payment_terms_ids,
        id,
        True,
      ),
    ),
  )
}

pub fn payment_terms_owner_exists(store: Store, owner_id: String) -> Bool {
  case dict.get(store.staged_state.payment_terms_owner_ids, owner_id) {
    Ok(True) -> True
    _ ->
      case dict.get(store.base_state.payment_terms_owner_ids, owner_id) {
        Ok(True) -> True
        _ -> False
      }
  }
}

pub fn get_effective_payment_terms_by_id(
  store: Store,
  id: String,
) -> Option(PaymentTermsRecord) {
  case dict.get(store.staged_state.deleted_payment_terms_ids, id) {
    Ok(True) -> None
    _ ->
      case dict.get(store.staged_state.payment_terms, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.payment_terms, id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

pub fn get_effective_payment_terms_by_owner_id(
  store: Store,
  owner_id: String,
) -> Option(PaymentTermsRecord) {
  let id = case
    dict.get(store.staged_state.payment_terms_by_owner_id, owner_id)
  {
    Ok(value) -> Some(value)
    Error(_) ->
      case dict.get(store.base_state.payment_terms_by_owner_id, owner_id) {
        Ok(value) -> Some(value)
        Error(_) -> None
      }
  }
  case id {
    Some(payment_terms_id) ->
      get_effective_payment_terms_by_id(store, payment_terms_id)
    None -> None
  }
}

pub fn get_effective_payment_schedule_by_id(
  store: Store,
  schedule_id: String,
) -> Option(#(PaymentTermsRecord, PaymentScheduleRecord)) {
  let candidates =
    list.append(
      dict.values(store.staged_state.payment_terms),
      dict.values(store.base_state.payment_terms),
    )
    |> list.filter_map(fn(record) {
      case get_effective_payment_terms_by_id(store, record.id) {
        Some(effective) -> Ok(effective)
        None -> Error(Nil)
      }
    })

  candidates
  |> list.find_map(fn(terms) {
    terms.payment_schedules
    |> list.find_map(fn(schedule) {
      case schedule.id == schedule_id {
        True -> Ok(#(terms, schedule))
        False -> Error(Nil)
      }
    })
    |> result.map_error(fn(_) { Nil })
  })
  |> option.from_result
}

pub fn stage_store_credit_account(
  store: Store,
  record: StoreCreditAccountRecord,
) -> Store {
  Store(
    ..store,
    staged_state: StagedState(
      ..store.staged_state,
      store_credit_accounts: dict.insert(
        store.staged_state.store_credit_accounts,
        record.id,
        record,
      ),
    ),
  )
}

pub fn stage_store_credit_account_transaction(
  store: Store,
  record: StoreCreditAccountTransactionRecord,
) -> Store {
  Store(
    ..store,
    staged_state: StagedState(
      ..store.staged_state,
      store_credit_account_transactions: dict.insert(
        store.staged_state.store_credit_account_transactions,
        record.id,
        record,
      ),
    ),
  )
}

pub fn get_effective_store_credit_account_by_id(
  store: Store,
  account_id: String,
) -> Option(StoreCreditAccountRecord) {
  let found = case
    dict.get(store.staged_state.store_credit_accounts, account_id)
  {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(store.base_state.store_credit_accounts, account_id) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
  case found {
    Some(account) ->
      case store_credit_account_owner_exists(store, account.customer_id) {
        True -> Some(account)
        False -> None
      }
    None -> None
  }
}

pub fn get_effective_store_credit_account_by_owner_id(
  store: Store,
  owner_id: String,
) -> Option(StoreCreditAccountRecord) {
  find_effective_store_credit_account(store, fn(account) {
    account.customer_id == owner_id
  })
}

pub fn get_effective_store_credit_account_by_owner_id_and_currency(
  store: Store,
  owner_id: String,
  currency_code: String,
) -> Option(StoreCreditAccountRecord) {
  find_effective_store_credit_account(store, fn(account) {
    account.customer_id == owner_id
    && account.balance.currency_code == currency_code
  })
}

fn find_effective_store_credit_account(
  store: Store,
  predicate: fn(StoreCreditAccountRecord) -> Bool,
) -> Option(StoreCreditAccountRecord) {
  dict.keys(dict.merge(
    store.base_state.store_credit_accounts,
    store.staged_state.store_credit_accounts,
  ))
  |> list.sort(string_compare)
  |> list.find_map(fn(id) {
    case get_effective_store_credit_account_by_id(store, id) {
      Some(account) ->
        case predicate(account) {
          True -> Ok(account)
          False -> Error(Nil)
        }
      None -> Error(Nil)
    }
  })
  |> option.from_result
}

pub fn list_effective_store_credit_accounts_for_customer(
  store: Store,
  customer_id: String,
) -> List(StoreCreditAccountRecord) {
  dict.keys(dict.merge(
    store.base_state.store_credit_accounts,
    store.staged_state.store_credit_accounts,
  ))
  |> list.sort(string_compare)
  |> list.filter_map(fn(id) {
    case get_effective_store_credit_account_by_id(store, id) {
      Some(account) ->
        case account.customer_id == customer_id {
          True -> Ok(account)
          False -> Error(Nil)
        }
      None -> Error(Nil)
    }
  })
}

pub fn list_effective_store_credit_account_transactions(
  store: Store,
  account_id: String,
) -> List(StoreCreditAccountTransactionRecord) {
  dict.values(dict.merge(
    store.base_state.store_credit_account_transactions,
    store.staged_state.store_credit_account_transactions,
  ))
  |> list.filter(fn(txn) { txn.account_id == account_id })
  |> list.sort(fn(a, b) {
    case string.compare(b.created_at, a.created_at) {
      order.Eq -> string.compare(b.id, a.id)
      other -> other
    }
  })
}

fn store_credit_account_owner_exists(store: Store, owner_id: String) -> Bool {
  case string.starts_with(owner_id, "gid://shopify/CompanyLocation/") {
    True ->
      case b2b.get_effective_b2b_company_location_by_id(store, owner_id) {
        Some(_) -> True
        None -> False
      }
    False ->
      case get_effective_customer_by_id(store, owner_id) {
        Some(_) -> True
        None -> False
      }
  }
}

pub fn upsert_base_customer_account_pages(
  store: Store,
  records: List(CustomerAccountPageRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        customer_account_pages: dict.insert(
          base.customer_account_pages,
          record.id,
          record,
        ),
        customer_account_page_order: append_unique_id(
          base.customer_account_page_order,
          record.id,
        ),
      ),
    )
  })
}

pub fn get_effective_customer_account_page_by_id(
  store: Store,
  page_id: String,
) -> Option(CustomerAccountPageRecord) {
  case dict.get(store.base_state.customer_account_pages, page_id) {
    Ok(record) -> Some(record)
    Error(_) -> None
  }
}

pub fn list_effective_customer_account_pages(
  store: Store,
) -> List(CustomerAccountPageRecord) {
  let ids =
    list.append(
      store.base_state.customer_account_page_order,
      store.staged_state.customer_account_page_order,
    )
    |> dedupe_strings()
  list.filter_map(ids, fn(id) {
    case get_effective_customer_account_page_by_id(store, id) {
      Some(record) -> Ok(record)
      None -> Error(Nil)
    }
  })
}

pub fn stage_customer_data_erasure_request(
  store: Store,
  request: CustomerDataErasureRequestRecord,
) -> Store {
  Store(
    ..store,
    staged_state: StagedState(
      ..store.staged_state,
      customer_data_erasure_requests: dict.insert(
        store.staged_state.customer_data_erasure_requests,
        request.customer_id,
        request,
      ),
    ),
  )
}

pub fn get_customer_data_erasure_request(
  store: Store,
  customer_id: String,
) -> Option(CustomerDataErasureRequestRecord) {
  case
    dict.get(store.staged_state.customer_data_erasure_requests, customer_id)
  {
    Ok(request) -> Some(request)
    Error(_) ->
      case
        dict.get(store.base_state.customer_data_erasure_requests, customer_id)
      {
        Ok(request) -> Some(request)
        Error(_) -> None
      }
  }
}

pub fn stage_merge_customers(
  store: Store,
  source_customer_id: String,
  resulting_customer: CustomerRecord,
  merge_request: CustomerMergeRequestRecord,
) -> Store {
  let after_delete = stage_delete_customer(store, source_customer_id)
  let #(stored, after_update) =
    stage_update_customer(after_delete, resulting_customer)
  let _ = stored
  Store(
    ..after_update,
    staged_state: StagedState(
      ..after_update.staged_state,
      merged_customer_ids: dict.insert(
        after_update.staged_state.merged_customer_ids,
        source_customer_id,
        resulting_customer.id,
      ),
      customer_merge_requests: dict.insert(
        after_update.staged_state.customer_merge_requests,
        merge_request.job_id,
        merge_request,
      ),
    ),
  )
}

pub fn get_customer_merge_request(
  store: Store,
  job_id: String,
) -> Option(CustomerMergeRequestRecord) {
  case dict.get(store.staged_state.customer_merge_requests, job_id) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(store.base_state.customer_merge_requests, job_id) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

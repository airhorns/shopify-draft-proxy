//// Store operations for fulfillment and shipping records.

import gleam/dict
import gleam/list
import gleam/option.{type Option, None, Some}
import shopify_draft_proxy/state/store/shared.{
  append_unique_id, dedupe_strings, string_compare,
}
import shopify_draft_proxy/state/store/types.{
  type Store, BaseState, StagedState, Store,
}
import shopify_draft_proxy/state/types.{
  type CalculatedOrderRecord, type CarrierServiceRecord,
  type DeliveryProfileRecord, type FulfillmentOrderRecord,
  type FulfillmentRecord, type FulfillmentServiceRecord,
  type InventoryShipmentRecord, type InventoryTransferRecord,
  type ReverseDeliveryRecord, type ReverseFulfillmentOrderRecord,
  type ShippingOrderRecord, type ShippingPackageRecord,
} as _

pub fn upsert_base_inventory_transfers(
  store: Store,
  transfers: List(InventoryTransferRecord),
) -> Store {
  list.fold(transfers, store, fn(acc, transfer) {
    let base = acc.base_state
    let staged = acc.staged_state
    let next_order = case
      list.contains(base.inventory_transfer_order, transfer.id)
    {
      True -> base.inventory_transfer_order
      False -> list.append(base.inventory_transfer_order, [transfer.id])
    }
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        inventory_transfers: dict.insert(
          base.inventory_transfers,
          transfer.id,
          transfer,
        ),
        inventory_transfer_order: next_order,
        deleted_inventory_transfer_ids: dict.delete(
          base.deleted_inventory_transfer_ids,
          transfer.id,
        ),
      ),
      staged_state: StagedState(
        ..staged,
        deleted_inventory_transfer_ids: dict.delete(
          staged.deleted_inventory_transfer_ids,
          transfer.id,
        ),
      ),
    )
  })
}

pub fn upsert_staged_inventory_transfer(
  store: Store,
  transfer: InventoryTransferRecord,
) -> #(InventoryTransferRecord, Store) {
  let staged = store.staged_state
  let next_order = case
    list.contains(store.base_state.inventory_transfer_order, transfer.id)
    || list.contains(staged.inventory_transfer_order, transfer.id)
  {
    True -> staged.inventory_transfer_order
    False -> list.append(staged.inventory_transfer_order, [transfer.id])
  }
  let next_staged =
    StagedState(
      ..staged,
      inventory_transfers: dict.insert(
        staged.inventory_transfers,
        transfer.id,
        transfer,
      ),
      inventory_transfer_order: next_order,
      deleted_inventory_transfer_ids: dict.delete(
        staged.deleted_inventory_transfer_ids,
        transfer.id,
      ),
    )
  #(transfer, Store(..store, staged_state: next_staged))
}

pub fn delete_staged_inventory_transfer(
  store: Store,
  transfer_id: String,
) -> Store {
  let staged = store.staged_state
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      inventory_transfers: dict.delete(staged.inventory_transfers, transfer_id),
      inventory_transfer_order: list.filter(
        staged.inventory_transfer_order,
        fn(id) { id != transfer_id },
      ),
      deleted_inventory_transfer_ids: dict.insert(
        staged.deleted_inventory_transfer_ids,
        transfer_id,
        True,
      ),
    ),
  )
}

pub fn get_effective_inventory_transfer_by_id(
  store: Store,
  transfer_id: String,
) -> Option(InventoryTransferRecord) {
  case
    dict.has_key(store.staged_state.deleted_inventory_transfer_ids, transfer_id)
    || dict.has_key(
      store.base_state.deleted_inventory_transfer_ids,
      transfer_id,
    )
  {
    True -> None
    False ->
      case dict.get(store.staged_state.inventory_transfers, transfer_id) {
        Ok(transfer) -> Some(transfer)
        Error(_) ->
          case dict.get(store.base_state.inventory_transfers, transfer_id) {
            Ok(transfer) -> Some(transfer)
            Error(_) -> None
          }
      }
  }
}

pub fn list_effective_inventory_transfers(
  store: Store,
) -> List(InventoryTransferRecord) {
  let ordered_ids =
    list.append(
      store.base_state.inventory_transfer_order,
      store.staged_state.inventory_transfer_order,
    )
    |> dedupe_strings
  let ordered_transfers =
    ordered_ids
    |> list.filter_map(fn(id) {
      get_effective_inventory_transfer_by_id(store, id)
      |> option.to_result(Nil)
    })
  let unordered_ids =
    dict.keys(store.base_state.inventory_transfers)
    |> list.append(dict.keys(store.staged_state.inventory_transfers))
    |> dedupe_strings
    |> list.filter(fn(id) { !list.contains(ordered_ids, id) })
    |> list.sort(string_compare)
  let unordered_transfers =
    unordered_ids
    |> list.filter_map(fn(id) {
      get_effective_inventory_transfer_by_id(store, id)
      |> option.to_result(Nil)
    })
  list.append(ordered_transfers, unordered_transfers)
}

pub fn upsert_base_inventory_shipments(
  store: Store,
  shipments: List(InventoryShipmentRecord),
) -> Store {
  list.fold(shipments, store, fn(acc, shipment) {
    let base = acc.base_state
    let staged = acc.staged_state
    let next_order = case
      list.contains(base.inventory_shipment_order, shipment.id)
    {
      True -> base.inventory_shipment_order
      False -> list.append(base.inventory_shipment_order, [shipment.id])
    }
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        inventory_shipments: dict.insert(
          base.inventory_shipments,
          shipment.id,
          shipment,
        ),
        inventory_shipment_order: next_order,
        deleted_inventory_shipment_ids: dict.delete(
          base.deleted_inventory_shipment_ids,
          shipment.id,
        ),
      ),
      staged_state: StagedState(
        ..staged,
        deleted_inventory_shipment_ids: dict.delete(
          staged.deleted_inventory_shipment_ids,
          shipment.id,
        ),
      ),
    )
  })
}

pub fn upsert_staged_inventory_shipment(
  store: Store,
  shipment: InventoryShipmentRecord,
) -> #(InventoryShipmentRecord, Store) {
  let staged = store.staged_state
  let next_order = case
    list.contains(store.base_state.inventory_shipment_order, shipment.id)
    || list.contains(staged.inventory_shipment_order, shipment.id)
  {
    True -> staged.inventory_shipment_order
    False -> list.append(staged.inventory_shipment_order, [shipment.id])
  }
  let next_staged =
    StagedState(
      ..staged,
      inventory_shipments: dict.insert(
        staged.inventory_shipments,
        shipment.id,
        shipment,
      ),
      inventory_shipment_order: next_order,
      deleted_inventory_shipment_ids: dict.delete(
        staged.deleted_inventory_shipment_ids,
        shipment.id,
      ),
    )
  #(shipment, Store(..store, staged_state: next_staged))
}

pub fn delete_staged_inventory_shipment(
  store: Store,
  shipment_id: String,
) -> Store {
  let staged = store.staged_state
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      inventory_shipments: dict.delete(staged.inventory_shipments, shipment_id),
      deleted_inventory_shipment_ids: dict.insert(
        staged.deleted_inventory_shipment_ids,
        shipment_id,
        True,
      ),
    ),
  )
}

pub fn get_effective_inventory_shipment_by_id(
  store: Store,
  shipment_id: String,
) -> Option(InventoryShipmentRecord) {
  case
    dict.has_key(store.staged_state.deleted_inventory_shipment_ids, shipment_id)
    || dict.has_key(
      store.base_state.deleted_inventory_shipment_ids,
      shipment_id,
    )
  {
    True -> None
    False ->
      case dict.get(store.staged_state.inventory_shipments, shipment_id) {
        Ok(shipment) -> Some(shipment)
        Error(_) ->
          case dict.get(store.base_state.inventory_shipments, shipment_id) {
            Ok(shipment) -> Some(shipment)
            Error(_) -> None
          }
      }
  }
}

pub fn list_effective_inventory_shipments(
  store: Store,
) -> List(InventoryShipmentRecord) {
  let ordered_ids =
    list.append(
      store.base_state.inventory_shipment_order,
      store.staged_state.inventory_shipment_order,
    )
    |> dedupe_strings
  let ordered_shipments =
    ordered_ids
    |> list.filter_map(fn(id) {
      get_effective_inventory_shipment_by_id(store, id)
      |> option.to_result(Nil)
    })
  let unordered_ids =
    dict.keys(store.base_state.inventory_shipments)
    |> list.append(dict.keys(store.staged_state.inventory_shipments))
    |> dedupe_strings
    |> list.filter(fn(id) { !list.contains(ordered_ids, id) })
    |> list.sort(string_compare)
  let unordered_shipments =
    unordered_ids
    |> list.filter_map(fn(id) {
      get_effective_inventory_shipment_by_id(store, id)
      |> option.to_result(Nil)
    })
  list.append(ordered_shipments, unordered_shipments)
}

pub fn upsert_base_carrier_services(
  store: Store,
  services: List(CarrierServiceRecord),
) -> Store {
  list.fold(services, store, fn(acc, service) {
    let base = acc.base_state
    let staged = acc.staged_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        carrier_services: dict.insert(
          base.carrier_services,
          service.id,
          service,
        ),
        carrier_service_order: append_unique_id(
          base.carrier_service_order,
          service.id,
        ),
        deleted_carrier_service_ids: dict.delete(
          base.deleted_carrier_service_ids,
          service.id,
        ),
      ),
      staged_state: StagedState(
        ..staged,
        deleted_carrier_service_ids: dict.delete(
          staged.deleted_carrier_service_ids,
          service.id,
        ),
      ),
    )
  })
}

pub fn stage_create_carrier_service(
  store: Store,
  service: CarrierServiceRecord,
) -> #(CarrierServiceRecord, Store) {
  stage_upsert_carrier_service(store, service)
}

pub fn stage_update_carrier_service(
  store: Store,
  service: CarrierServiceRecord,
) -> #(CarrierServiceRecord, Store) {
  stage_upsert_carrier_service(store, service)
}

fn stage_upsert_carrier_service(
  store: Store,
  service: CarrierServiceRecord,
) -> #(CarrierServiceRecord, Store) {
  let staged = store.staged_state
  let next_order = case
    list.contains(store.base_state.carrier_service_order, service.id)
    || list.contains(staged.carrier_service_order, service.id)
  {
    True -> staged.carrier_service_order
    False -> list.append(staged.carrier_service_order, [service.id])
  }
  let next_staged =
    StagedState(
      ..staged,
      carrier_services: dict.insert(
        staged.carrier_services,
        service.id,
        service,
      ),
      carrier_service_order: next_order,
      deleted_carrier_service_ids: dict.delete(
        staged.deleted_carrier_service_ids,
        service.id,
      ),
    )
  #(service, Store(..store, staged_state: next_staged))
}

pub fn delete_staged_carrier_service(store: Store, id: String) -> Store {
  let staged = store.staged_state
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      carrier_services: dict.delete(staged.carrier_services, id),
      deleted_carrier_service_ids: dict.insert(
        staged.deleted_carrier_service_ids,
        id,
        True,
      ),
    ),
  )
}

pub fn get_effective_carrier_service_by_id(
  store: Store,
  id: String,
) -> Option(CarrierServiceRecord) {
  case
    dict.has_key(store.staged_state.deleted_carrier_service_ids, id)
    || dict.has_key(store.base_state.deleted_carrier_service_ids, id)
  {
    True -> None
    False ->
      case dict.get(store.staged_state.carrier_services, id) {
        Ok(service) -> Some(service)
        Error(_) ->
          case dict.get(store.base_state.carrier_services, id) {
            Ok(service) -> Some(service)
            Error(_) -> None
          }
      }
  }
}

pub fn list_effective_carrier_services(
  store: Store,
) -> List(CarrierServiceRecord) {
  let ordered_ids =
    list.append(
      store.base_state.carrier_service_order,
      store.staged_state.carrier_service_order,
    )
    |> dedupe_strings
  let ordered_services =
    ordered_ids
    |> list.filter_map(fn(id) {
      get_effective_carrier_service_by_id(store, id)
      |> option.to_result(Nil)
    })
  let unordered_ids =
    dict.keys(store.base_state.carrier_services)
    |> list.append(dict.keys(store.staged_state.carrier_services))
    |> dedupe_strings
    |> list.filter(fn(id) { !list.contains(ordered_ids, id) })
    |> list.sort(string_compare)
  let unordered_services =
    unordered_ids
    |> list.filter_map(fn(id) {
      get_effective_carrier_service_by_id(store, id)
      |> option.to_result(Nil)
    })
  list.append(ordered_services, unordered_services)
}

pub fn upsert_base_fulfillment_services(
  store: Store,
  services: List(FulfillmentServiceRecord),
) -> Store {
  list.fold(services, store, fn(acc, service) {
    let base = acc.base_state
    let staged = acc.staged_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        fulfillment_services: dict.insert(
          base.fulfillment_services,
          service.id,
          service,
        ),
        fulfillment_service_order: append_unique_id(
          base.fulfillment_service_order,
          service.id,
        ),
        deleted_fulfillment_service_ids: dict.delete(
          base.deleted_fulfillment_service_ids,
          service.id,
        ),
      ),
      staged_state: StagedState(
        ..staged,
        deleted_fulfillment_service_ids: dict.delete(
          staged.deleted_fulfillment_service_ids,
          service.id,
        ),
      ),
    )
  })
}

pub fn stage_create_fulfillment_service(
  store: Store,
  service: FulfillmentServiceRecord,
) -> #(FulfillmentServiceRecord, Store) {
  stage_upsert_fulfillment_service(store, service)
}

pub fn stage_update_fulfillment_service(
  store: Store,
  service: FulfillmentServiceRecord,
) -> #(FulfillmentServiceRecord, Store) {
  stage_upsert_fulfillment_service(store, service)
}

fn stage_upsert_fulfillment_service(
  store: Store,
  service: FulfillmentServiceRecord,
) -> #(FulfillmentServiceRecord, Store) {
  let staged = store.staged_state
  let next_order = case
    list.contains(store.base_state.fulfillment_service_order, service.id)
    || list.contains(staged.fulfillment_service_order, service.id)
  {
    True -> staged.fulfillment_service_order
    False -> list.append(staged.fulfillment_service_order, [service.id])
  }
  let next_staged =
    StagedState(
      ..staged,
      fulfillment_services: dict.insert(
        staged.fulfillment_services,
        service.id,
        service,
      ),
      fulfillment_service_order: next_order,
      deleted_fulfillment_service_ids: dict.delete(
        staged.deleted_fulfillment_service_ids,
        service.id,
      ),
    )
  #(service, Store(..store, staged_state: next_staged))
}

pub fn delete_staged_fulfillment_service(store: Store, id: String) -> Store {
  let staged = store.staged_state
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      fulfillment_services: dict.delete(staged.fulfillment_services, id),
      deleted_fulfillment_service_ids: dict.insert(
        staged.deleted_fulfillment_service_ids,
        id,
        True,
      ),
    ),
  )
}

pub fn get_effective_fulfillment_service_by_id(
  store: Store,
  id: String,
) -> Option(FulfillmentServiceRecord) {
  case
    dict.has_key(store.staged_state.deleted_fulfillment_service_ids, id)
    || dict.has_key(store.base_state.deleted_fulfillment_service_ids, id)
  {
    True -> None
    False ->
      case dict.get(store.staged_state.fulfillment_services, id) {
        Ok(service) -> Some(service)
        Error(_) ->
          case dict.get(store.base_state.fulfillment_services, id) {
            Ok(service) -> Some(service)
            Error(_) -> None
          }
      }
  }
}

pub fn list_effective_fulfillment_services(
  store: Store,
) -> List(FulfillmentServiceRecord) {
  let ordered_ids =
    list.append(
      store.base_state.fulfillment_service_order,
      store.staged_state.fulfillment_service_order,
    )
    |> dedupe_strings
  let ordered_services =
    ordered_ids
    |> list.filter_map(fn(id) {
      get_effective_fulfillment_service_by_id(store, id)
      |> option.to_result(Nil)
    })
  let unordered_ids =
    dict.keys(store.base_state.fulfillment_services)
    |> list.append(dict.keys(store.staged_state.fulfillment_services))
    |> dedupe_strings
    |> list.filter(fn(id) { !list.contains(ordered_ids, id) })
    |> list.sort(string_compare)
  let unordered_services =
    unordered_ids
    |> list.filter_map(fn(id) {
      get_effective_fulfillment_service_by_id(store, id)
      |> option.to_result(Nil)
    })
  list.append(ordered_services, unordered_services)
}

pub fn upsert_base_fulfillments(
  store: Store,
  fulfillments: List(FulfillmentRecord),
) -> Store {
  list.fold(fulfillments, store, fn(acc, fulfillment) {
    let base = acc.base_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        fulfillments: dict.insert(
          base.fulfillments,
          fulfillment.id,
          fulfillment,
        ),
        fulfillment_order: append_unique_id(
          base.fulfillment_order,
          fulfillment.id,
        ),
      ),
    )
  })
}

pub fn stage_upsert_fulfillment(
  store: Store,
  fulfillment: FulfillmentRecord,
) -> #(FulfillmentRecord, Store) {
  let staged = store.staged_state
  let next_order = case
    list.contains(store.base_state.fulfillment_order, fulfillment.id)
    || list.contains(staged.fulfillment_order, fulfillment.id)
  {
    True -> staged.fulfillment_order
    False -> list.append(staged.fulfillment_order, [fulfillment.id])
  }
  let next_staged =
    StagedState(
      ..staged,
      fulfillments: dict.insert(
        staged.fulfillments,
        fulfillment.id,
        fulfillment,
      ),
      fulfillment_order: next_order,
    )
  #(fulfillment, Store(..store, staged_state: next_staged))
}

pub fn upsert_base_fulfillment_orders(
  store: Store,
  fulfillment_orders: List(FulfillmentOrderRecord),
) -> Store {
  list.fold(fulfillment_orders, store, fn(acc, fulfillment_order) {
    let base = acc.base_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        fulfillment_orders: dict.insert(
          base.fulfillment_orders,
          fulfillment_order.id,
          fulfillment_order,
        ),
        fulfillment_order_order: append_unique_id(
          base.fulfillment_order_order,
          fulfillment_order.id,
        ),
      ),
    )
  })
}

pub fn stage_upsert_fulfillment_order(
  store: Store,
  fulfillment_order: FulfillmentOrderRecord,
) -> #(FulfillmentOrderRecord, Store) {
  let staged = store.staged_state
  let next_order = case
    list.contains(
      store.base_state.fulfillment_order_order,
      fulfillment_order.id,
    )
    || list.contains(staged.fulfillment_order_order, fulfillment_order.id)
  {
    True -> staged.fulfillment_order_order
    False -> list.append(staged.fulfillment_order_order, [fulfillment_order.id])
  }
  let next_staged =
    StagedState(
      ..staged,
      fulfillment_orders: dict.insert(
        staged.fulfillment_orders,
        fulfillment_order.id,
        fulfillment_order,
      ),
      fulfillment_order_order: next_order,
    )
  #(fulfillment_order, Store(..store, staged_state: next_staged))
}

pub fn upsert_base_shipping_orders(
  store: Store,
  orders: List(ShippingOrderRecord),
) -> Store {
  list.fold(orders, store, fn(acc, order) {
    let base = acc.base_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        shipping_orders: dict.insert(base.shipping_orders, order.id, order),
      ),
    )
  })
}

pub fn stage_upsert_shipping_order(
  store: Store,
  order: ShippingOrderRecord,
) -> #(ShippingOrderRecord, Store) {
  let staged = store.staged_state
  let next_staged =
    StagedState(
      ..staged,
      shipping_orders: dict.insert(staged.shipping_orders, order.id, order),
    )
  #(order, Store(..store, staged_state: next_staged))
}

pub fn upsert_base_reverse_fulfillment_orders(
  store: Store,
  records: List(ReverseFulfillmentOrderRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        reverse_fulfillment_orders: dict.insert(
          base.reverse_fulfillment_orders,
          record.id,
          record,
        ),
        reverse_fulfillment_order_order: append_unique_id(
          base.reverse_fulfillment_order_order,
          record.id,
        ),
      ),
    )
  })
}

pub fn stage_upsert_reverse_fulfillment_order(
  store: Store,
  record: ReverseFulfillmentOrderRecord,
) -> #(ReverseFulfillmentOrderRecord, Store) {
  let staged = store.staged_state
  let next_order = case
    list.contains(store.base_state.reverse_fulfillment_order_order, record.id)
    || list.contains(staged.reverse_fulfillment_order_order, record.id)
  {
    True -> staged.reverse_fulfillment_order_order
    False -> list.append(staged.reverse_fulfillment_order_order, [record.id])
  }
  let next_staged =
    StagedState(
      ..staged,
      reverse_fulfillment_orders: dict.insert(
        staged.reverse_fulfillment_orders,
        record.id,
        record,
      ),
      reverse_fulfillment_order_order: next_order,
    )
  #(record, Store(..store, staged_state: next_staged))
}

pub fn get_effective_reverse_fulfillment_order_by_id(
  store: Store,
  id: String,
) -> Option(ReverseFulfillmentOrderRecord) {
  case dict.get(store.staged_state.reverse_fulfillment_orders, id) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(store.base_state.reverse_fulfillment_orders, id) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

pub fn list_effective_reverse_fulfillment_orders(
  store: Store,
) -> List(ReverseFulfillmentOrderRecord) {
  let ordered_ids =
    list.append(
      store.base_state.reverse_fulfillment_order_order,
      store.staged_state.reverse_fulfillment_order_order,
    )
    |> dedupe_strings
  let ordered =
    ordered_ids
    |> list.filter_map(fn(id) {
      get_effective_reverse_fulfillment_order_by_id(store, id)
      |> option.to_result(Nil)
    })
  let unordered_ids =
    dict.keys(store.base_state.reverse_fulfillment_orders)
    |> list.append(dict.keys(store.staged_state.reverse_fulfillment_orders))
    |> dedupe_strings
    |> list.filter(fn(id) { !list.contains(ordered_ids, id) })
    |> list.sort(string_compare)
  let unordered =
    unordered_ids
    |> list.filter_map(fn(id) {
      get_effective_reverse_fulfillment_order_by_id(store, id)
      |> option.to_result(Nil)
    })
  list.append(ordered, unordered)
}

pub fn upsert_base_reverse_deliveries(
  store: Store,
  records: List(ReverseDeliveryRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        reverse_deliveries: dict.insert(
          base.reverse_deliveries,
          record.id,
          record,
        ),
        reverse_delivery_order: append_unique_id(
          base.reverse_delivery_order,
          record.id,
        ),
      ),
    )
  })
}

pub fn stage_upsert_reverse_delivery(
  store: Store,
  record: ReverseDeliveryRecord,
) -> #(ReverseDeliveryRecord, Store) {
  let staged = store.staged_state
  let next_order = case
    list.contains(store.base_state.reverse_delivery_order, record.id)
    || list.contains(staged.reverse_delivery_order, record.id)
  {
    True -> staged.reverse_delivery_order
    False -> list.append(staged.reverse_delivery_order, [record.id])
  }
  let next_staged =
    StagedState(
      ..staged,
      reverse_deliveries: dict.insert(
        staged.reverse_deliveries,
        record.id,
        record,
      ),
      reverse_delivery_order: next_order,
    )
  #(record, Store(..store, staged_state: next_staged))
}

pub fn get_effective_reverse_delivery_by_id(
  store: Store,
  id: String,
) -> Option(ReverseDeliveryRecord) {
  case dict.get(store.staged_state.reverse_deliveries, id) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(store.base_state.reverse_deliveries, id) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

pub fn list_effective_reverse_deliveries(
  store: Store,
) -> List(ReverseDeliveryRecord) {
  let ordered_ids =
    list.append(
      store.base_state.reverse_delivery_order,
      store.staged_state.reverse_delivery_order,
    )
    |> dedupe_strings
  let ordered =
    ordered_ids
    |> list.filter_map(fn(id) {
      get_effective_reverse_delivery_by_id(store, id) |> option.to_result(Nil)
    })
  let unordered_ids =
    dict.keys(store.base_state.reverse_deliveries)
    |> list.append(dict.keys(store.staged_state.reverse_deliveries))
    |> dedupe_strings
    |> list.filter(fn(id) { !list.contains(ordered_ids, id) })
    |> list.sort(string_compare)
  let unordered =
    unordered_ids
    |> list.filter_map(fn(id) {
      get_effective_reverse_delivery_by_id(store, id) |> option.to_result(Nil)
    })
  list.append(ordered, unordered)
}

pub fn upsert_base_calculated_orders(
  store: Store,
  records: List(CalculatedOrderRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        calculated_orders: dict.insert(
          base.calculated_orders,
          record.id,
          record,
        ),
      ),
    )
  })
}

pub fn stage_upsert_calculated_order(
  store: Store,
  record: CalculatedOrderRecord,
) -> #(CalculatedOrderRecord, Store) {
  let staged = store.staged_state
  let next_staged =
    StagedState(
      ..staged,
      calculated_orders: dict.insert(
        staged.calculated_orders,
        record.id,
        record,
      ),
    )
  #(record, Store(..store, staged_state: next_staged))
}

pub fn get_effective_calculated_order_by_id(
  store: Store,
  id: String,
) -> Option(CalculatedOrderRecord) {
  case dict.get(store.staged_state.calculated_orders, id) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(store.base_state.calculated_orders, id) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

pub fn get_effective_fulfillment_by_id(
  store: Store,
  id: String,
) -> Option(FulfillmentRecord) {
  case dict.get(store.staged_state.fulfillments, id) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(store.base_state.fulfillments, id) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

pub fn get_effective_fulfillment_order_by_id(
  store: Store,
  id: String,
) -> Option(FulfillmentOrderRecord) {
  case dict.get(store.staged_state.fulfillment_orders, id) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(store.base_state.fulfillment_orders, id) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

pub fn get_effective_shipping_order_by_id(
  store: Store,
  id: String,
) -> Option(ShippingOrderRecord) {
  case dict.get(store.staged_state.shipping_orders, id) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(store.base_state.shipping_orders, id) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

pub fn list_effective_fulfillments(store: Store) -> List(FulfillmentRecord) {
  let ordered_ids =
    list.append(
      store.base_state.fulfillment_order,
      store.staged_state.fulfillment_order,
    )
    |> dedupe_strings
  let ordered =
    ordered_ids
    |> list.filter_map(fn(id) {
      get_effective_fulfillment_by_id(store, id) |> option.to_result(Nil)
    })
  let unordered_ids =
    dict.keys(store.base_state.fulfillments)
    |> list.append(dict.keys(store.staged_state.fulfillments))
    |> dedupe_strings
    |> list.filter(fn(id) { !list.contains(ordered_ids, id) })
    |> list.sort(string_compare)
  let unordered =
    unordered_ids
    |> list.filter_map(fn(id) {
      get_effective_fulfillment_by_id(store, id) |> option.to_result(Nil)
    })
  list.append(ordered, unordered)
}

pub fn list_effective_fulfillment_orders(
  store: Store,
) -> List(FulfillmentOrderRecord) {
  let ordered_ids =
    list.append(
      store.base_state.fulfillment_order_order,
      store.staged_state.fulfillment_order_order,
    )
    |> dedupe_strings
  let ordered =
    ordered_ids
    |> list.filter_map(fn(id) {
      get_effective_fulfillment_order_by_id(store, id)
      |> option.to_result(Nil)
    })
  let unordered_ids =
    dict.keys(store.base_state.fulfillment_orders)
    |> list.append(dict.keys(store.staged_state.fulfillment_orders))
    |> dedupe_strings
    |> list.filter(fn(id) { !list.contains(ordered_ids, id) })
    |> list.sort(string_compare)
  let unordered =
    unordered_ids
    |> list.filter_map(fn(id) {
      get_effective_fulfillment_order_by_id(store, id)
      |> option.to_result(Nil)
    })
  list.append(ordered, unordered)
}

pub fn upsert_base_delivery_profiles(
  store: Store,
  profiles: List(DeliveryProfileRecord),
) -> Store {
  list.fold(profiles, store, fn(acc, profile) {
    let base = acc.base_state
    let staged = acc.staged_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        delivery_profiles: dict.insert(
          base.delivery_profiles,
          profile.id,
          profile,
        ),
        delivery_profile_order: append_unique_id(
          base.delivery_profile_order,
          profile.id,
        ),
        deleted_delivery_profile_ids: dict.delete(
          base.deleted_delivery_profile_ids,
          profile.id,
        ),
      ),
      staged_state: StagedState(
        ..staged,
        deleted_delivery_profile_ids: dict.delete(
          staged.deleted_delivery_profile_ids,
          profile.id,
        ),
      ),
    )
  })
}

pub fn stage_create_delivery_profile(
  store: Store,
  profile: DeliveryProfileRecord,
) -> #(DeliveryProfileRecord, Store) {
  stage_upsert_delivery_profile(store, profile)
}

pub fn stage_update_delivery_profile(
  store: Store,
  profile: DeliveryProfileRecord,
) -> #(DeliveryProfileRecord, Store) {
  stage_upsert_delivery_profile(store, profile)
}

fn stage_upsert_delivery_profile(
  store: Store,
  profile: DeliveryProfileRecord,
) -> #(DeliveryProfileRecord, Store) {
  let staged = store.staged_state
  let next_order = case
    list.contains(store.base_state.delivery_profile_order, profile.id)
    || list.contains(staged.delivery_profile_order, profile.id)
  {
    True -> staged.delivery_profile_order
    False -> list.append(staged.delivery_profile_order, [profile.id])
  }
  let next_staged =
    StagedState(
      ..staged,
      delivery_profiles: dict.insert(
        staged.delivery_profiles,
        profile.id,
        profile,
      ),
      delivery_profile_order: next_order,
      deleted_delivery_profile_ids: dict.delete(
        staged.deleted_delivery_profile_ids,
        profile.id,
      ),
    )
  #(profile, Store(..store, staged_state: next_staged))
}

pub fn delete_staged_delivery_profile(store: Store, id: String) -> Store {
  let staged = store.staged_state
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      delivery_profiles: dict.delete(staged.delivery_profiles, id),
      deleted_delivery_profile_ids: dict.insert(
        staged.deleted_delivery_profile_ids,
        id,
        True,
      ),
    ),
  )
}

pub fn get_effective_delivery_profile_by_id(
  store: Store,
  id: String,
) -> Option(DeliveryProfileRecord) {
  case
    dict.has_key(store.staged_state.deleted_delivery_profile_ids, id)
    || dict.has_key(store.base_state.deleted_delivery_profile_ids, id)
  {
    True -> None
    False ->
      case dict.get(store.staged_state.delivery_profiles, id) {
        Ok(profile) -> Some(profile)
        Error(_) ->
          case dict.get(store.base_state.delivery_profiles, id) {
            Ok(profile) -> Some(profile)
            Error(_) -> None
          }
      }
  }
}

pub fn list_effective_delivery_profiles(
  store: Store,
) -> List(DeliveryProfileRecord) {
  let ordered_ids =
    list.append(
      store.base_state.delivery_profile_order,
      store.staged_state.delivery_profile_order,
    )
    |> dedupe_strings
  let ordered_profiles =
    ordered_ids
    |> list.filter_map(fn(id) {
      get_effective_delivery_profile_by_id(store, id)
      |> option.to_result(Nil)
    })
  let unordered_ids =
    dict.keys(store.base_state.delivery_profiles)
    |> list.append(dict.keys(store.staged_state.delivery_profiles))
    |> dedupe_strings
    |> list.filter(fn(id) { !list.contains(ordered_ids, id) })
    |> list.sort(string_compare)
  let unordered_profiles =
    unordered_ids
    |> list.filter_map(fn(id) {
      get_effective_delivery_profile_by_id(store, id)
      |> option.to_result(Nil)
    })
  list.append(ordered_profiles, unordered_profiles)
}

pub fn has_staged_delivery_profiles(store: Store) -> Bool {
  dict.size(store.staged_state.delivery_profiles) > 0
  || dict.size(store.staged_state.deleted_delivery_profile_ids) > 0
}

pub fn upsert_base_shipping_packages(
  store: Store,
  packages: List(ShippingPackageRecord),
) -> Store {
  list.fold(packages, store, fn(acc, shipping_package) {
    let base = acc.base_state
    let staged = acc.staged_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        shipping_packages: dict.insert(
          base.shipping_packages,
          shipping_package.id,
          shipping_package,
        ),
        shipping_package_order: append_unique_id(
          base.shipping_package_order,
          shipping_package.id,
        ),
        deleted_shipping_package_ids: dict.delete(
          base.deleted_shipping_package_ids,
          shipping_package.id,
        ),
      ),
      staged_state: StagedState(
        ..staged,
        deleted_shipping_package_ids: dict.delete(
          staged.deleted_shipping_package_ids,
          shipping_package.id,
        ),
      ),
    )
  })
}

pub fn stage_update_shipping_package(
  store: Store,
  shipping_package: ShippingPackageRecord,
) -> #(ShippingPackageRecord, Store) {
  let staged = store.staged_state
  let next_order = case
    list.contains(store.base_state.shipping_package_order, shipping_package.id)
    || list.contains(staged.shipping_package_order, shipping_package.id)
  {
    True -> staged.shipping_package_order
    False -> list.append(staged.shipping_package_order, [shipping_package.id])
  }
  let next_staged =
    StagedState(
      ..staged,
      shipping_packages: dict.insert(
        staged.shipping_packages,
        shipping_package.id,
        shipping_package,
      ),
      shipping_package_order: next_order,
      deleted_shipping_package_ids: dict.delete(
        staged.deleted_shipping_package_ids,
        shipping_package.id,
      ),
    )
  #(shipping_package, Store(..store, staged_state: next_staged))
}

pub fn delete_staged_shipping_package(store: Store, id: String) -> Store {
  let staged = store.staged_state
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      shipping_packages: dict.delete(staged.shipping_packages, id),
      deleted_shipping_package_ids: dict.insert(
        staged.deleted_shipping_package_ids,
        id,
        True,
      ),
    ),
  )
}

pub fn get_effective_shipping_package_by_id(
  store: Store,
  id: String,
) -> Option(ShippingPackageRecord) {
  case
    dict.has_key(store.staged_state.deleted_shipping_package_ids, id)
    || dict.has_key(store.base_state.deleted_shipping_package_ids, id)
  {
    True -> None
    False ->
      case dict.get(store.staged_state.shipping_packages, id) {
        Ok(shipping_package) -> Some(shipping_package)
        Error(_) ->
          case dict.get(store.base_state.shipping_packages, id) {
            Ok(shipping_package) -> Some(shipping_package)
            Error(_) -> None
          }
      }
  }
}

pub fn list_effective_shipping_packages(
  store: Store,
) -> List(ShippingPackageRecord) {
  let ordered_ids =
    list.append(
      store.base_state.shipping_package_order,
      store.staged_state.shipping_package_order,
    )
    |> dedupe_strings
  let ordered_packages =
    ordered_ids
    |> list.filter_map(fn(id) {
      get_effective_shipping_package_by_id(store, id)
      |> option.to_result(Nil)
    })
  let unordered_ids =
    dict.keys(store.base_state.shipping_packages)
    |> list.append(dict.keys(store.staged_state.shipping_packages))
    |> dedupe_strings
    |> list.filter(fn(id) { !list.contains(ordered_ids, id) })
    |> list.sort(string_compare)
  let unordered_packages =
    unordered_ids
    |> list.filter_map(fn(id) {
      get_effective_shipping_package_by_id(store, id)
      |> option.to_result(Nil)
    })
  list.append(ordered_packages, unordered_packages)
}

//// Store operations for B2B company records.

import gleam/dict
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/string
import shopify_draft_proxy/state/store/shared.{
  type EffectiveSlice, EffectiveSlice, append_unique_id, append_unique_ids,
  dict_has, effective_get, effective_list, effective_list_ordered,
}
import shopify_draft_proxy/state/store/types.{
  type Store, BaseState, StagedState, Store,
}
import shopify_draft_proxy/state/types.{
  type B2BCompanyContactRecord, type B2BCompanyContactRoleRecord,
  type B2BCompanyLocationRecord, type B2BCompanyRecord,
  type StorePropertyMutationPayloadRecord, type StorePropertyRecord,
} as _

// ---------------------------------------------------------------------------
// B2B company slice
// ---------------------------------------------------------------------------

pub fn upsert_base_b2b_company(
  store: Store,
  record: B2BCompanyRecord,
) -> Store {
  let base = store.base_state
  let staged = store.staged_state
  Store(
    ..store,
    base_state: BaseState(
      ..base,
      b2b_companies: dict.insert(base.b2b_companies, record.id, record),
      b2b_company_order: append_unique_id(base.b2b_company_order, record.id),
      deleted_b2b_company_ids: dict.delete(
        base.deleted_b2b_company_ids,
        record.id,
      ),
    ),
    staged_state: StagedState(
      ..staged,
      deleted_b2b_company_ids: dict.delete(
        staged.deleted_b2b_company_ids,
        record.id,
      ),
    ),
  )
}

pub fn upsert_staged_b2b_company(
  store: Store,
  record: B2BCompanyRecord,
) -> #(B2BCompanyRecord, Store) {
  let staged = store.staged_state
  let known =
    list.contains(store.base_state.b2b_company_order, record.id)
    || list.contains(staged.b2b_company_order, record.id)
    || dict_has(store.base_state.b2b_companies, record.id)
    || dict_has(staged.b2b_companies, record.id)
  let order = case known {
    True -> staged.b2b_company_order
    False -> list.append(staged.b2b_company_order, [record.id])
  }
  #(
    record,
    Store(
      ..store,
      staged_state: StagedState(
        ..staged,
        b2b_companies: dict.insert(staged.b2b_companies, record.id, record),
        b2b_company_order: order,
        deleted_b2b_company_ids: dict.delete(
          staged.deleted_b2b_company_ids,
          record.id,
        ),
      ),
    ),
  )
}

pub fn delete_staged_b2b_company(store: Store, id: String) -> Store {
  let staged = store.staged_state
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      b2b_companies: dict.delete(staged.b2b_companies, id),
      deleted_b2b_company_ids: dict.insert(
        staged.deleted_b2b_company_ids,
        id,
        True,
      ),
    ),
  )
}

fn b2b_company_slice(store: Store) -> EffectiveSlice(B2BCompanyRecord) {
  EffectiveSlice(
    base_records: store.base_state.b2b_companies,
    staged_records: store.staged_state.b2b_companies,
    base_deleted: store.base_state.deleted_b2b_company_ids,
    staged_deleted: store.staged_state.deleted_b2b_company_ids,
    base_order: store.base_state.b2b_company_order,
    staged_order: store.staged_state.b2b_company_order,
  )
}

pub fn get_effective_b2b_company_by_id(
  store: Store,
  id: String,
) -> Option(B2BCompanyRecord) {
  effective_get(b2b_company_slice(store), id)
}

pub fn list_effective_b2b_companies(store: Store) -> List(B2BCompanyRecord) {
  effective_list(b2b_company_slice(store), fn(record) { record.id })
}

pub fn upsert_base_b2b_company_contact(
  store: Store,
  record: B2BCompanyContactRecord,
) -> Store {
  let base = store.base_state
  let staged = store.staged_state
  Store(
    ..store,
    base_state: BaseState(
      ..base,
      b2b_company_contacts: dict.insert(
        base.b2b_company_contacts,
        record.id,
        record,
      ),
      b2b_company_contact_order: append_unique_id(
        base.b2b_company_contact_order,
        record.id,
      ),
      deleted_b2b_company_contact_ids: dict.delete(
        base.deleted_b2b_company_contact_ids,
        record.id,
      ),
    ),
    staged_state: StagedState(
      ..staged,
      deleted_b2b_company_contact_ids: dict.delete(
        staged.deleted_b2b_company_contact_ids,
        record.id,
      ),
    ),
  )
}

pub fn upsert_staged_b2b_company_contact(
  store: Store,
  record: B2BCompanyContactRecord,
) -> #(B2BCompanyContactRecord, Store) {
  let staged = store.staged_state
  let known =
    list.contains(store.base_state.b2b_company_contact_order, record.id)
    || list.contains(staged.b2b_company_contact_order, record.id)
    || dict_has(store.base_state.b2b_company_contacts, record.id)
    || dict_has(staged.b2b_company_contacts, record.id)
  let order = case known {
    True -> staged.b2b_company_contact_order
    False -> list.append(staged.b2b_company_contact_order, [record.id])
  }
  #(
    record,
    Store(
      ..store,
      staged_state: StagedState(
        ..staged,
        b2b_company_contacts: dict.insert(
          staged.b2b_company_contacts,
          record.id,
          record,
        ),
        b2b_company_contact_order: order,
        deleted_b2b_company_contact_ids: dict.delete(
          staged.deleted_b2b_company_contact_ids,
          record.id,
        ),
      ),
    ),
  )
}

pub fn delete_staged_b2b_company_contact(store: Store, id: String) -> Store {
  let staged = store.staged_state
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      b2b_company_contacts: dict.delete(staged.b2b_company_contacts, id),
      deleted_b2b_company_contact_ids: dict.insert(
        staged.deleted_b2b_company_contact_ids,
        id,
        True,
      ),
    ),
  )
}

fn b2b_company_contact_slice(
  store: Store,
) -> EffectiveSlice(B2BCompanyContactRecord) {
  EffectiveSlice(
    base_records: store.base_state.b2b_company_contacts,
    staged_records: store.staged_state.b2b_company_contacts,
    base_deleted: store.base_state.deleted_b2b_company_contact_ids,
    staged_deleted: store.staged_state.deleted_b2b_company_contact_ids,
    base_order: store.base_state.b2b_company_contact_order,
    staged_order: store.staged_state.b2b_company_contact_order,
  )
}

pub fn get_effective_b2b_company_contact_by_id(
  store: Store,
  id: String,
) -> Option(B2BCompanyContactRecord) {
  effective_get(b2b_company_contact_slice(store), id)
}

pub fn list_effective_b2b_company_contacts(
  store: Store,
) -> List(B2BCompanyContactRecord) {
  effective_list_ordered(b2b_company_contact_slice(store))
}

pub fn upsert_base_b2b_company_contact_role(
  store: Store,
  record: B2BCompanyContactRoleRecord,
) -> Store {
  let base = store.base_state
  let staged = store.staged_state
  Store(
    ..store,
    base_state: BaseState(
      ..base,
      b2b_company_contact_roles: dict.insert(
        base.b2b_company_contact_roles,
        record.id,
        record,
      ),
      b2b_company_contact_role_order: append_unique_id(
        base.b2b_company_contact_role_order,
        record.id,
      ),
      deleted_b2b_company_contact_role_ids: dict.delete(
        base.deleted_b2b_company_contact_role_ids,
        record.id,
      ),
    ),
    staged_state: StagedState(
      ..staged,
      deleted_b2b_company_contact_role_ids: dict.delete(
        staged.deleted_b2b_company_contact_role_ids,
        record.id,
      ),
    ),
  )
}

pub fn upsert_staged_b2b_company_contact_role(
  store: Store,
  record: B2BCompanyContactRoleRecord,
) -> #(B2BCompanyContactRoleRecord, Store) {
  let staged = store.staged_state
  let known =
    list.contains(store.base_state.b2b_company_contact_role_order, record.id)
    || list.contains(staged.b2b_company_contact_role_order, record.id)
    || dict_has(store.base_state.b2b_company_contact_roles, record.id)
    || dict_has(staged.b2b_company_contact_roles, record.id)
  let order = case known {
    True -> staged.b2b_company_contact_role_order
    False -> list.append(staged.b2b_company_contact_role_order, [record.id])
  }
  #(
    record,
    Store(
      ..store,
      staged_state: StagedState(
        ..staged,
        b2b_company_contact_roles: dict.insert(
          staged.b2b_company_contact_roles,
          record.id,
          record,
        ),
        b2b_company_contact_role_order: order,
        deleted_b2b_company_contact_role_ids: dict.delete(
          staged.deleted_b2b_company_contact_role_ids,
          record.id,
        ),
      ),
    ),
  )
}

pub fn delete_staged_b2b_company_contact_role(
  store: Store,
  id: String,
) -> Store {
  let staged = store.staged_state
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      b2b_company_contact_roles: dict.delete(
        staged.b2b_company_contact_roles,
        id,
      ),
      deleted_b2b_company_contact_role_ids: dict.insert(
        staged.deleted_b2b_company_contact_role_ids,
        id,
        True,
      ),
    ),
  )
}

fn b2b_company_contact_role_slice(
  store: Store,
) -> EffectiveSlice(B2BCompanyContactRoleRecord) {
  EffectiveSlice(
    base_records: store.base_state.b2b_company_contact_roles,
    staged_records: store.staged_state.b2b_company_contact_roles,
    base_deleted: store.base_state.deleted_b2b_company_contact_role_ids,
    staged_deleted: store.staged_state.deleted_b2b_company_contact_role_ids,
    base_order: store.base_state.b2b_company_contact_role_order,
    staged_order: store.staged_state.b2b_company_contact_role_order,
  )
}

pub fn get_effective_b2b_company_contact_role_by_id(
  store: Store,
  id: String,
) -> Option(B2BCompanyContactRoleRecord) {
  effective_get(b2b_company_contact_role_slice(store), id)
}

pub fn upsert_base_b2b_company_location(
  store: Store,
  record: B2BCompanyLocationRecord,
) -> Store {
  let base = store.base_state
  let staged = store.staged_state
  Store(
    ..store,
    base_state: BaseState(
      ..base,
      b2b_company_locations: dict.insert(
        base.b2b_company_locations,
        record.id,
        record,
      ),
      b2b_company_location_order: append_unique_id(
        base.b2b_company_location_order,
        record.id,
      ),
      deleted_b2b_company_location_ids: dict.delete(
        base.deleted_b2b_company_location_ids,
        record.id,
      ),
    ),
    staged_state: StagedState(
      ..staged,
      deleted_b2b_company_location_ids: dict.delete(
        staged.deleted_b2b_company_location_ids,
        record.id,
      ),
    ),
  )
}

pub fn upsert_staged_b2b_company_location(
  store: Store,
  record: B2BCompanyLocationRecord,
) -> #(B2BCompanyLocationRecord, Store) {
  let staged = store.staged_state
  let known =
    list.contains(store.base_state.b2b_company_location_order, record.id)
    || list.contains(staged.b2b_company_location_order, record.id)
    || dict_has(store.base_state.b2b_company_locations, record.id)
    || dict_has(staged.b2b_company_locations, record.id)
  let order = case known {
    True -> staged.b2b_company_location_order
    False -> list.append(staged.b2b_company_location_order, [record.id])
  }
  #(
    record,
    Store(
      ..store,
      staged_state: StagedState(
        ..staged,
        b2b_company_locations: dict.insert(
          staged.b2b_company_locations,
          record.id,
          record,
        ),
        b2b_company_location_order: order,
        deleted_b2b_company_location_ids: dict.delete(
          staged.deleted_b2b_company_location_ids,
          record.id,
        ),
      ),
    ),
  )
}

pub fn delete_staged_b2b_company_location(store: Store, id: String) -> Store {
  let staged = store.staged_state
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      b2b_company_locations: dict.delete(staged.b2b_company_locations, id),
      deleted_b2b_company_location_ids: dict.insert(
        staged.deleted_b2b_company_location_ids,
        id,
        True,
      ),
    ),
  )
}

fn b2b_company_location_slice(
  store: Store,
) -> EffectiveSlice(B2BCompanyLocationRecord) {
  EffectiveSlice(
    base_records: store.base_state.b2b_company_locations,
    staged_records: store.staged_state.b2b_company_locations,
    base_deleted: store.base_state.deleted_b2b_company_location_ids,
    staged_deleted: store.staged_state.deleted_b2b_company_location_ids,
    base_order: store.base_state.b2b_company_location_order,
    staged_order: store.staged_state.b2b_company_location_order,
  )
}

pub fn get_effective_b2b_company_location_by_id(
  store: Store,
  id: String,
) -> Option(B2BCompanyLocationRecord) {
  effective_get(b2b_company_location_slice(store), id)
}

pub fn list_effective_b2b_company_locations(
  store: Store,
) -> List(B2BCompanyLocationRecord) {
  effective_list(b2b_company_location_slice(store), fn(record) { record.id })
}

pub fn upsert_base_store_property_location(
  store: Store,
  record: StorePropertyRecord,
) -> Store {
  let base = store.base_state
  let staged = store.staged_state
  Store(
    ..store,
    base_state: BaseState(
      ..base,
      store_property_locations: dict.insert(
        base.store_property_locations,
        record.id,
        record,
      ),
      store_property_location_order: append_unique_id(
        base.store_property_location_order,
        record.id,
      ),
      deleted_store_property_location_ids: dict.delete(
        base.deleted_store_property_location_ids,
        record.id,
      ),
    ),
    staged_state: StagedState(
      ..staged,
      deleted_store_property_location_ids: dict.delete(
        staged.deleted_store_property_location_ids,
        record.id,
      ),
    ),
  )
}

pub fn upsert_staged_store_property_location(
  store: Store,
  record: StorePropertyRecord,
) -> #(StorePropertyRecord, Store) {
  let staged = store.staged_state
  let base = store.base_state
  let known =
    list.contains(base.store_property_location_order, record.id)
    || list.contains(staged.store_property_location_order, record.id)
    || dict_has(base.store_property_locations, record.id)
    || dict_has(staged.store_property_locations, record.id)
  let order = case known {
    True -> staged.store_property_location_order
    False -> list.append(staged.store_property_location_order, [record.id])
  }
  #(
    record,
    Store(
      ..store,
      staged_state: StagedState(
        ..staged,
        store_property_locations: dict.insert(
          staged.store_property_locations,
          record.id,
          record,
        ),
        store_property_location_order: order,
        deleted_store_property_location_ids: dict.delete(
          staged.deleted_store_property_location_ids,
          record.id,
        ),
      ),
    ),
  )
}

pub fn delete_staged_store_property_location(
  store: Store,
  id: String,
) -> Store {
  let staged = store.staged_state
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      store_property_locations: dict.delete(staged.store_property_locations, id),
      deleted_store_property_location_ids: dict.insert(
        staged.deleted_store_property_location_ids,
        id,
        True,
      ),
    ),
  )
}

fn store_property_location_slice(
  store: Store,
) -> EffectiveSlice(StorePropertyRecord) {
  EffectiveSlice(
    base_records: store.base_state.store_property_locations,
    staged_records: store.staged_state.store_property_locations,
    base_deleted: store.base_state.deleted_store_property_location_ids,
    staged_deleted: store.staged_state.deleted_store_property_location_ids,
    base_order: store.base_state.store_property_location_order,
    staged_order: store.staged_state.store_property_location_order,
  )
}

pub fn get_effective_store_property_location_by_id(
  store: Store,
  id: String,
) -> Option(StorePropertyRecord) {
  effective_get(store_property_location_slice(store), id)
}

pub fn list_effective_store_property_locations(
  store: Store,
) -> List(StorePropertyRecord) {
  effective_list(store_property_location_slice(store), fn(record) { record.id })
}

pub fn upsert_base_business_entity(
  store: Store,
  record: StorePropertyRecord,
) -> Store {
  let base = store.base_state
  Store(
    ..store,
    base_state: BaseState(
      ..base,
      business_entities: dict.insert(base.business_entities, record.id, record),
      business_entity_order: append_unique_id(
        base.business_entity_order,
        record.id,
      ),
    ),
  )
}

pub fn get_business_entity_by_id(
  store: Store,
  id: String,
) -> Option(StorePropertyRecord) {
  case dict.get(store.base_state.business_entities, id) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(store.staged_state.business_entities, id) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

pub fn list_effective_business_entities(
  store: Store,
) -> List(StorePropertyRecord) {
  let ordered_ids =
    append_unique_ids(
      store.base_state.business_entity_order,
      store.staged_state.business_entity_order,
    )
  let ordered =
    ordered_ids
    |> list.filter_map(fn(id) {
      case get_business_entity_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  let ordered_lookup =
    list.fold(ordered_ids, dict.new(), fn(acc, id) {
      dict.insert(acc, id, True)
    })
  let extras =
    dict.to_list(store.base_state.business_entities)
    |> list.append(dict.to_list(store.staged_state.business_entities))
    |> list.filter_map(fn(pair) {
      let #(id, _) = pair
      case dict_has(ordered_lookup, id) {
        True -> Error(Nil)
        False ->
          case get_business_entity_by_id(store, id) {
            Some(record) -> Ok(record)
            None -> Error(Nil)
          }
      }
    })
    |> sort_store_property_records
  list.append(ordered, extras)
}

pub fn upsert_base_publishable(
  store: Store,
  record: StorePropertyRecord,
) -> Store {
  let base = store.base_state
  Store(
    ..store,
    base_state: BaseState(
      ..base,
      publishables: dict.insert(base.publishables, record.id, record),
      publishable_order: append_unique_id(base.publishable_order, record.id),
    ),
  )
}

pub fn upsert_staged_publishable(
  store: Store,
  record: StorePropertyRecord,
) -> #(StorePropertyRecord, Store) {
  let staged = store.staged_state
  let base = store.base_state
  let known =
    list.contains(base.publishable_order, record.id)
    || list.contains(staged.publishable_order, record.id)
    || dict_has(base.publishables, record.id)
    || dict_has(staged.publishables, record.id)
  let order = case known {
    True -> staged.publishable_order
    False -> list.append(staged.publishable_order, [record.id])
  }
  #(
    record,
    Store(
      ..store,
      staged_state: StagedState(
        ..staged,
        publishables: dict.insert(staged.publishables, record.id, record),
        publishable_order: order,
      ),
    ),
  )
}

pub fn get_effective_publishable_by_id(
  store: Store,
  id: String,
) -> Option(StorePropertyRecord) {
  case dict.get(store.staged_state.publishables, id) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(store.base_state.publishables, id) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

pub fn upsert_base_store_property_mutation_payload(
  store: Store,
  record: StorePropertyMutationPayloadRecord,
) -> Store {
  let base = store.base_state
  Store(
    ..store,
    base_state: BaseState(
      ..base,
      store_property_mutation_payloads: dict.insert(
        base.store_property_mutation_payloads,
        record.key,
        record,
      ),
    ),
  )
}

pub fn get_store_property_mutation_payload(
  store: Store,
  key: String,
) -> Option(StorePropertyMutationPayloadRecord) {
  case dict.get(store.staged_state.store_property_mutation_payloads, key) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(store.base_state.store_property_mutation_payloads, key) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

fn sort_store_property_records(
  records: List(StorePropertyRecord),
) -> List(StorePropertyRecord) {
  list.sort(records, fn(a, b) { string.compare(a.id, b.id) })
}

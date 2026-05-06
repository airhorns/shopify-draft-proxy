//// Store operations for marketing records.

import gleam/dict.{type Dict}
import gleam/list
import gleam/option.{type Option, None, Some}
import shopify_draft_proxy/state/store/shared.{
  append_unique_id, dedupe_strings, dict_has, list_to_set, string_compare,
}
import shopify_draft_proxy/state/store/types.{
  type Store, BaseState, StagedState, Store,
}
import shopify_draft_proxy/state/types.{
  type MarketingChannelDefinitionRecord, type MarketingEngagementRecord,
  type MarketingRecord, type MarketingValue, MarketingObject, MarketingString,
} as types_mod

// ---------------------------------------------------------------------------
// Marketing slice
// ---------------------------------------------------------------------------

pub fn upsert_base_marketing_activities(
  store: Store,
  records: List(MarketingRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        marketing_activities: dict.insert(
          base.marketing_activities,
          record.id,
          record,
        ),
        marketing_activity_order: append_unique_id(
          base.marketing_activity_order,
          record.id,
        ),
        deleted_marketing_activity_ids: dict.delete(
          base.deleted_marketing_activity_ids,
          record.id,
        ),
      ),
    )
  })
}

pub fn upsert_base_marketing_events(
  store: Store,
  records: List(MarketingRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        marketing_events: dict.insert(base.marketing_events, record.id, record),
        marketing_event_order: append_unique_id(
          base.marketing_event_order,
          record.id,
        ),
        deleted_marketing_event_ids: dict.delete(
          base.deleted_marketing_event_ids,
          record.id,
        ),
      ),
    )
  })
}

pub fn upsert_base_marketing_channel_definitions(
  store: Store,
  records: List(MarketingChannelDefinitionRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        marketing_channel_definitions: dict.insert(
          base.marketing_channel_definitions,
          record.handle,
          record,
        ),
      ),
    )
  })
}

pub fn stage_marketing_activity(
  store: Store,
  record: MarketingRecord,
) -> #(MarketingRecord, Store) {
  let staged = store.staged_state
  let next =
    StagedState(
      ..staged,
      marketing_activities: dict.insert(
        staged.marketing_activities,
        record.id,
        record,
      ),
      marketing_activity_order: append_unique_id(
        staged.marketing_activity_order,
        record.id,
      ),
      deleted_marketing_activity_ids: dict.delete(
        staged.deleted_marketing_activity_ids,
        record.id,
      ),
    )
  #(record, Store(..store, staged_state: next))
}

pub fn stage_marketing_event(
  store: Store,
  record: MarketingRecord,
) -> #(MarketingRecord, Store) {
  let staged = store.staged_state
  let next =
    StagedState(
      ..staged,
      marketing_events: dict.insert(staged.marketing_events, record.id, record),
      marketing_event_order: append_unique_id(
        staged.marketing_event_order,
        record.id,
      ),
      deleted_marketing_event_ids: dict.delete(
        staged.deleted_marketing_event_ids,
        record.id,
      ),
    )
  #(record, Store(..store, staged_state: next))
}

pub fn stage_delete_marketing_activity(store: Store, id: String) -> Store {
  let event_id = case get_effective_marketing_activity_record_by_id(store, id) {
    Some(record) -> read_marketing_event_id(record.data)
    None -> None
  }
  let staged = store.staged_state
  let next =
    StagedState(
      ..staged,
      marketing_activities: dict.delete(staged.marketing_activities, id),
      deleted_marketing_activity_ids: dict.insert(
        staged.deleted_marketing_activity_ids,
        id,
        True,
      ),
    )
  let next = case event_id {
    None -> next
    Some(event_id) ->
      StagedState(
        ..next,
        marketing_events: dict.delete(next.marketing_events, event_id),
        deleted_marketing_event_ids: dict.insert(
          next.deleted_marketing_event_ids,
          event_id,
          True,
        ),
      )
  }
  Store(..store, staged_state: next)
}

pub fn stage_delete_all_external_marketing_activities(
  store: Store,
) -> #(List(String), Store) {
  stage_delete_all_external_marketing_activities_for_app(store, None)
}

pub fn stage_delete_all_external_marketing_activities_for_app(
  store: Store,
  requesting_api_client_id: Option(String),
) -> #(List(String), Store) {
  let #(ids, next_store) =
    list.fold(
      list_effective_marketing_activities(store),
      #([], store),
      fn(acc, record) {
        let #(deleted, current) = acc
        case
          marketing_bool_field(record.data, "isExternal")
          && record_visible_to_api_client(record, requesting_api_client_id)
        {
          True -> #(
            [record.id, ..deleted],
            stage_delete_marketing_activity(current, record.id),
          )
          False -> acc
        }
      },
    )
  let staged = next_store.staged_state
  let next_staged = case requesting_api_client_id {
    None -> StagedState(..staged, marketing_delete_all_external_in_flight: True)
    Some(api_client_id) ->
      StagedState(
        ..staged,
        marketing_delete_all_external_in_flight_api_client_ids: dict.insert(
          staged.marketing_delete_all_external_in_flight_api_client_ids,
          api_client_id,
          True,
        ),
      )
  }
  let next_store = Store(..next_store, staged_state: next_staged)
  #(list.reverse(ids), next_store)
}

pub fn get_effective_marketing_activity_record_by_id(
  store: Store,
  id: String,
) -> Option(MarketingRecord) {
  case dict.get(store.staged_state.deleted_marketing_activity_ids, id) {
    Ok(_) -> None
    Error(_) ->
      case dict.get(store.staged_state.marketing_activities, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.marketing_activities, id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

pub fn get_effective_marketing_activity_record_by_id_for_app(
  store: Store,
  id: String,
  requesting_api_client_id: Option(String),
) -> Option(MarketingRecord) {
  case get_effective_marketing_activity_record_by_id(store, id) {
    Some(record) ->
      case record_visible_to_api_client(record, requesting_api_client_id) {
        True -> Some(record)
        False -> None
      }
    _ -> None
  }
}

pub fn get_effective_marketing_event_record_by_id_for_app(
  store: Store,
  id: String,
  requesting_api_client_id: Option(String),
) -> Option(MarketingRecord) {
  case get_effective_marketing_event_record_by_id(store, id) {
    Some(record) ->
      case record_visible_to_api_client(record, requesting_api_client_id) {
        True -> Some(record)
        False -> None
      }
    _ -> None
  }
}

pub fn get_effective_marketing_event_record_by_id(
  store: Store,
  id: String,
) -> Option(MarketingRecord) {
  case dict.get(store.staged_state.deleted_marketing_event_ids, id) {
    Ok(_) -> None
    Error(_) ->
      case dict.get(store.staged_state.marketing_events, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.marketing_events, id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

pub fn get_effective_marketing_activity_by_remote_id(
  store: Store,
  remote_id: String,
) -> Option(MarketingRecord) {
  list.find(list_effective_marketing_activities(store), fn(record) {
    read_marketing_remote_id(record.data) == Some(remote_id)
  })
  |> option.from_result
}

pub fn get_effective_marketing_activity_by_remote_id_for_app(
  store: Store,
  remote_id: String,
  requesting_api_client_id: Option(String),
) -> Option(MarketingRecord) {
  list.find(
    list_effective_marketing_activities_for_app(store, requesting_api_client_id),
    fn(record) { read_marketing_remote_id(record.data) == Some(remote_id) },
  )
  |> option.from_result
}

pub fn list_effective_marketing_activities(
  store: Store,
) -> List(MarketingRecord) {
  list_effective_marketing_records(
    store.base_state.marketing_activities,
    store.base_state.marketing_activity_order,
    store.staged_state.marketing_activities,
    store.staged_state.marketing_activity_order,
    store.staged_state.deleted_marketing_activity_ids,
  )
}

pub fn list_effective_marketing_activities_for_app(
  store: Store,
  requesting_api_client_id: Option(String),
) -> List(MarketingRecord) {
  list_effective_marketing_activities(store)
  |> list.filter(fn(record) {
    record_visible_to_api_client(record, requesting_api_client_id)
  })
}

pub fn list_effective_marketing_events(store: Store) -> List(MarketingRecord) {
  list_effective_marketing_records(
    store.base_state.marketing_events,
    store.base_state.marketing_event_order,
    store.staged_state.marketing_events,
    store.staged_state.marketing_event_order,
    store.staged_state.deleted_marketing_event_ids,
  )
}

pub fn list_effective_marketing_events_for_app(
  store: Store,
  requesting_api_client_id: Option(String),
) -> List(MarketingRecord) {
  list_effective_marketing_events(store)
  |> list.filter(fn(record) {
    record_visible_to_api_client(record, requesting_api_client_id)
  })
}

pub fn has_staged_marketing_records(store: Store) -> Bool {
  !list.is_empty(dict.keys(store.staged_state.marketing_activities))
  || !list.is_empty(dict.keys(store.staged_state.marketing_events))
  || !list.is_empty(dict.keys(store.staged_state.marketing_engagements))
  || !list.is_empty(dict.keys(store.staged_state.deleted_marketing_activity_ids))
  || !list.is_empty(dict.keys(store.staged_state.deleted_marketing_event_ids))
  || !list.is_empty(dict.keys(
    store.staged_state.deleted_marketing_engagement_ids,
  ))
  || store.staged_state.marketing_delete_all_external_in_flight
  || !list.is_empty(dict.keys(
    store.staged_state.marketing_delete_all_external_in_flight_api_client_ids,
  ))
}

pub fn has_marketing_delete_all_external_in_flight(store: Store) -> Bool {
  store.staged_state.marketing_delete_all_external_in_flight
  || !list.is_empty(dict.keys(
    store.staged_state.marketing_delete_all_external_in_flight_api_client_ids,
  ))
}

pub fn has_marketing_delete_all_external_in_flight_for_app(
  store: Store,
  requesting_api_client_id: Option(String),
) -> Bool {
  case store.staged_state.marketing_delete_all_external_in_flight {
    True -> True
    False ->
      case requesting_api_client_id {
        None -> False
        Some(api_client_id) ->
          dict_has(
            store.staged_state.marketing_delete_all_external_in_flight_api_client_ids,
            api_client_id,
          )
      }
  }
}

pub fn stage_marketing_engagement(
  store: Store,
  record: MarketingEngagementRecord,
) -> #(MarketingEngagementRecord, Store) {
  let staged = store.staged_state
  let next =
    StagedState(
      ..staged,
      marketing_engagements: dict.insert(
        staged.marketing_engagements,
        record.id,
        record,
      ),
      marketing_engagement_order: append_unique_id(
        staged.marketing_engagement_order,
        record.id,
      ),
      deleted_marketing_engagement_ids: dict.delete(
        staged.deleted_marketing_engagement_ids,
        record.id,
      ),
    )
  #(record, Store(..store, staged_state: next))
}

pub fn stage_delete_marketing_engagement(store: Store, id: String) -> Store {
  let staged = store.staged_state
  let next =
    StagedState(
      ..staged,
      marketing_engagements: dict.delete(staged.marketing_engagements, id),
      deleted_marketing_engagement_ids: dict.insert(
        staged.deleted_marketing_engagement_ids,
        id,
        True,
      ),
    )
  Store(..store, staged_state: next)
}

pub fn stage_delete_marketing_engagements_by_channel_handle(
  store: Store,
  channel_handle: String,
) -> #(List(String), Store) {
  stage_delete_marketing_engagements_by_channel_handle_for_app(
    store,
    channel_handle,
    None,
  )
}

pub fn stage_delete_marketing_engagements_by_channel_handle_for_app(
  store: Store,
  channel_handle: String,
  requesting_api_client_id: Option(String),
) -> #(List(String), Store) {
  let #(ids, next_store) =
    list.fold(
      list_effective_marketing_engagements_for_app(
        store,
        requesting_api_client_id,
      ),
      #([], store),
      fn(acc, record) {
        let #(deleted, current) = acc
        case record.channel_handle == Some(channel_handle) {
          True -> #(
            [record.id, ..deleted],
            stage_delete_marketing_engagement(current, record.id),
          )
          False -> acc
        }
      },
    )
  #(list.reverse(ids), next_store)
}

pub fn stage_delete_all_channel_marketing_engagements(
  store: Store,
) -> #(List(String), Store) {
  stage_delete_all_channel_marketing_engagements_for_app(store, None)
}

pub fn stage_delete_all_channel_marketing_engagements_for_app(
  store: Store,
  requesting_api_client_id: Option(String),
) -> #(List(String), Store) {
  let #(ids, next_store) =
    list.fold(
      list_effective_marketing_engagements_for_app(
        store,
        requesting_api_client_id,
      ),
      #([], store),
      fn(acc, record) {
        let #(deleted, current) = acc
        case record.channel_handle {
          Some(_) -> #(
            [record.id, ..deleted],
            stage_delete_marketing_engagement(current, record.id),
          )
          None -> acc
        }
      },
    )
  #(list.reverse(ids), next_store)
}

pub fn list_effective_marketing_engagements(
  store: Store,
) -> List(MarketingEngagementRecord) {
  let ordered_ids =
    list.append(
      store.base_state.marketing_engagement_order,
      store.staged_state.marketing_engagement_order,
    )
    |> dedupe_strings()
  let merged =
    dict.merge(
      store.base_state.marketing_engagements,
      store.staged_state.marketing_engagements,
    )
  let ordered =
    list.filter_map(ordered_ids, fn(id) {
      case dict.get(store.staged_state.deleted_marketing_engagement_ids, id) {
        Ok(_) -> Error(Nil)
        Error(_) ->
          case dict.get(merged, id) {
            Ok(record) -> Ok(record)
            Error(_) -> Error(Nil)
          }
      }
    })
  let ordered_set = list_to_set(ordered_ids)
  let unordered =
    dict.values(merged)
    |> list.filter(fn(record) {
      !dict_has(ordered_set, record.id)
      && !dict_has(
        store.staged_state.deleted_marketing_engagement_ids,
        record.id,
      )
    })
    |> list.sort(fn(left, right) { string_compare(left.id, right.id) })
  list.append(ordered, unordered)
}

pub fn list_effective_marketing_engagements_for_app(
  store: Store,
  requesting_api_client_id: Option(String),
) -> List(MarketingEngagementRecord) {
  list_effective_marketing_engagements(store)
  |> list.filter(fn(record) {
    engagement_visible_to_api_client(record, requesting_api_client_id)
  })
}

pub fn record_visible_to_api_client(
  record: MarketingRecord,
  requesting_api_client_id: Option(String),
) -> Bool {
  case requesting_api_client_id, record.api_client_id {
    None, _ -> True
    Some(_), None -> True
    Some(requesting), Some(owner) -> requesting == owner
  }
}

fn engagement_visible_to_api_client(
  record: MarketingEngagementRecord,
  requesting_api_client_id: Option(String),
) -> Bool {
  case requesting_api_client_id, record.api_client_id {
    None, _ -> True
    Some(_), None -> True
    Some(requesting), Some(owner) -> requesting == owner
  }
}

pub fn has_known_marketing_channel_handle(
  store: Store,
  handle: String,
) -> Bool {
  has_registered_marketing_channel_handle(store, handle)
  || has_hydrated_marketing_channel_handle(store, handle)
}

pub fn has_known_marketing_channel_handle_for_app(
  store: Store,
  handle: String,
  requesting_api_client_id: Option(String),
) -> Bool {
  case dict.get(store.base_state.marketing_channel_definitions, handle) {
    Ok(record) -> {
      case record.api_client_ids, requesting_api_client_id {
        [], _ -> True
        ids, Some(api_client_id) -> list.contains(ids, api_client_id)
        _, None -> False
      }
    }
    Error(_) ->
      has_hydrated_marketing_channel_handle_for_app(
        store,
        handle,
        requesting_api_client_id,
      )
  }
}

fn has_registered_marketing_channel_handle(
  store: Store,
  handle: String,
) -> Bool {
  case dict.get(store.base_state.marketing_channel_definitions, handle) {
    Ok(_) -> True
    Error(_) -> False
  }
}

fn has_hydrated_marketing_channel_handle(store: Store, handle: String) -> Bool {
  list.any(list_effective_marketing_events(store), fn(event) {
    read_marketing_channel_handle(event.data) == Some(handle)
  })
}

fn has_hydrated_marketing_channel_handle_for_app(
  store: Store,
  handle: String,
  requesting_api_client_id: Option(String),
) -> Bool {
  list.any(
    list_effective_marketing_events_for_app(store, requesting_api_client_id),
    fn(event) { read_marketing_channel_handle(event.data) == Some(handle) },
  )
}

fn list_effective_marketing_records(
  base_bucket: Dict(String, MarketingRecord),
  base_order: List(String),
  staged_bucket: Dict(String, MarketingRecord),
  staged_order: List(String),
  deleted_bucket: Dict(String, Bool),
) -> List(MarketingRecord) {
  let ordered_ids = list.append(base_order, staged_order) |> dedupe_strings()
  let merged = dict.merge(base_bucket, staged_bucket)
  let ordered =
    list.filter_map(ordered_ids, fn(id) {
      case dict.get(deleted_bucket, id) {
        Ok(_) -> Error(Nil)
        Error(_) ->
          case dict.get(merged, id) {
            Ok(record) -> Ok(record)
            Error(_) -> Error(Nil)
          }
      }
    })
  let ordered_set = list_to_set(ordered_ids)
  let unordered =
    dict.values(merged)
    |> list.filter(fn(record) {
      !dict_has(ordered_set, record.id) && !dict_has(deleted_bucket, record.id)
    })
    |> list.sort(fn(left, right) { string_compare(left.id, right.id) })
  list.append(ordered, unordered)
}

fn read_marketing_event_id(
  data: Dict(String, MarketingValue),
) -> Option(String) {
  case dict.get(data, "marketingEvent") {
    Ok(MarketingObject(event)) -> marketing_string_field(event, "id")
    _ -> None
  }
}

fn read_marketing_remote_id(
  data: Dict(String, MarketingValue),
) -> Option(String) {
  case marketing_string_field(data, "remoteId") {
    Some(id) -> Some(id)
    None ->
      case dict.get(data, "marketingEvent") {
        Ok(MarketingObject(event)) -> marketing_string_field(event, "remoteId")
        _ -> None
      }
  }
}

fn read_marketing_channel_handle(
  data: Dict(String, MarketingValue),
) -> Option(String) {
  case marketing_string_field(data, "channelHandle") {
    Some(handle) -> Some(handle)
    None ->
      case dict.get(data, "marketingEvent") {
        Ok(MarketingObject(event)) ->
          marketing_string_field(event, "channelHandle")
        _ -> None
      }
  }
}

fn marketing_string_field(
  data: Dict(String, MarketingValue),
  field: String,
) -> Option(String) {
  case dict.get(data, field) {
    Ok(MarketingString(value)) -> Some(value)
    _ -> None
  }
}

fn marketing_bool_field(
  data: Dict(String, MarketingValue),
  field: String,
) -> Bool {
  case dict.get(data, field) {
    Ok(types_mod.MarketingBool(value)) -> value
    _ -> False
  }
}
/// Upsert BulkOperation records into base state. Mirrors
/// `upsertBaseBulkOperations`.
// ---------------------------------------------------------------------------
// Bulk-operations slice
// ---------------------------------------------------------------------------

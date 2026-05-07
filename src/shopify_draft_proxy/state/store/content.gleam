//// Store operations for online store and content records.

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
  type OnlineStoreContentRecord, type OnlineStoreIntegrationRecord,
  type SavedSearchRecord, type UrlRedirectRecord, type WebhookSubscriptionRecord,
} as _

// ---------------------------------------------------------------------------
// Saved-search slice
// ---------------------------------------------------------------------------

/// Upsert one or more saved-search records into the base state.
/// Mirrors `upsertBaseSavedSearches`. Removes any existing
/// "deleted" markers (in either base or staged) for the same id, since
/// the upstream answer wins.
pub fn upsert_base_saved_searches(
  store: Store,
  records: List(SavedSearchRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    let staged = acc.staged_state
    let new_base =
      BaseState(
        ..base,
        saved_searches: dict.insert(base.saved_searches, record.id, record),
        saved_search_order: append_unique_id(base.saved_search_order, record.id),
        deleted_saved_search_ids: dict.delete(
          base.deleted_saved_search_ids,
          record.id,
        ),
      )
    let new_staged =
      StagedState(
        ..staged,
        deleted_saved_search_ids: dict.delete(
          staged.deleted_saved_search_ids,
          record.id,
        ),
      )
    Store(..acc, base_state: new_base, staged_state: new_staged)
  })
}

/// Stage a saved-search record. Mirrors `upsertStagedSavedSearch`. The
/// TS version returns a fresh clone — Gleam values are already
/// immutable, so we return the record unchanged.
pub fn upsert_staged_saved_search(
  store: Store,
  record: SavedSearchRecord,
) -> #(SavedSearchRecord, Store) {
  let staged = store.staged_state
  let base = store.base_state
  let already_known =
    list.contains(base.saved_search_order, record.id)
    || list.contains(staged.saved_search_order, record.id)
  let new_order = case already_known {
    True -> staged.saved_search_order
    False -> list.append(staged.saved_search_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      saved_searches: dict.insert(staged.saved_searches, record.id, record),
      saved_search_order: new_order,
      deleted_saved_search_ids: dict.delete(
        staged.deleted_saved_search_ids,
        record.id,
      ),
    )
  #(record, Store(..store, staged_state: new_staged))
}

/// Mark a saved-search id as deleted. Mirrors
/// `deleteStagedSavedSearch`.
pub fn delete_staged_saved_search(store: Store, id: String) -> Store {
  let staged = store.staged_state
  let new_staged =
    StagedState(
      ..staged,
      saved_searches: dict.delete(staged.saved_searches, id),
      deleted_saved_search_ids: dict.insert(
        staged.deleted_saved_search_ids,
        id,
        True,
      ),
    )
  Store(..store, staged_state: new_staged)
}

/// Look up the effective saved search for an id. Staged wins over base;
/// any "deleted" marker on either side suppresses the record.
/// Mirrors `getEffectiveSavedSearchById`.
pub fn get_effective_saved_search_by_id(
  store: Store,
  id: String,
) -> Option(SavedSearchRecord) {
  let deleted =
    dict_has(store.base_state.deleted_saved_search_ids, id)
    || dict_has(store.staged_state.deleted_saved_search_ids, id)
  case deleted {
    True -> None
    False ->
      case dict.get(store.staged_state.saved_searches, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.saved_searches, id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

/// List every effective saved search the store knows about. Mirrors
/// `listEffectiveSavedSearches`. Ordered records (those tracked by the
/// `savedSearchOrder` arrays) come first, followed by any unordered
/// staged/base records sorted by id.
pub fn list_effective_saved_searches(store: Store) -> List(SavedSearchRecord) {
  let ordered_ids =
    list.append(
      store.base_state.saved_search_order,
      store.staged_state.saved_search_order,
    )
    |> dedupe_strings()
  let ordered_records =
    list.filter_map(ordered_ids, fn(id) {
      case get_effective_saved_search_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  let ordered_set = list_to_set(ordered_ids)
  let merged =
    dict.merge(
      store.base_state.saved_searches,
      store.staged_state.saved_searches,
    )
  let unordered_ids =
    dict.keys(merged)
    |> list.filter(fn(id) { !dict_has(ordered_set, id) })
    |> list.sort(string_compare)
  let unordered_records =
    list.filter_map(unordered_ids, fn(id) {
      case get_effective_saved_search_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  list.append(ordered_records, unordered_records)
}

// ---------------------------------------------------------------------------
// Webhook-subscription slice
// ---------------------------------------------------------------------------

/// Upsert one or more webhook-subscription records into the base state.
/// Mirrors `upsertBaseWebhookSubscriptions`. Removes any existing
/// "deleted" markers (in either base or staged) for the same id, since
/// the upstream answer wins.
pub fn upsert_base_webhook_subscriptions(
  store: Store,
  records: List(WebhookSubscriptionRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    let staged = acc.staged_state
    let new_base =
      BaseState(
        ..base,
        webhook_subscriptions: dict.insert(
          base.webhook_subscriptions,
          record.id,
          record,
        ),
        webhook_subscription_order: append_unique_id(
          base.webhook_subscription_order,
          record.id,
        ),
        deleted_webhook_subscription_ids: dict.delete(
          base.deleted_webhook_subscription_ids,
          record.id,
        ),
      )
    let new_staged =
      StagedState(
        ..staged,
        deleted_webhook_subscription_ids: dict.delete(
          staged.deleted_webhook_subscription_ids,
          record.id,
        ),
      )
    Store(..acc, base_state: new_base, staged_state: new_staged)
  })
}

/// Stage a webhook-subscription record. Mirrors
/// `upsertStagedWebhookSubscription`. The TS version returns a fresh
/// clone — Gleam values are already immutable, so we return the record
/// unchanged.
pub fn upsert_staged_webhook_subscription(
  store: Store,
  record: WebhookSubscriptionRecord,
) -> #(WebhookSubscriptionRecord, Store) {
  let staged = store.staged_state
  let base = store.base_state
  let already_known =
    list.contains(base.webhook_subscription_order, record.id)
    || list.contains(staged.webhook_subscription_order, record.id)
  let new_order = case already_known {
    True -> staged.webhook_subscription_order
    False -> list.append(staged.webhook_subscription_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      webhook_subscriptions: dict.insert(
        staged.webhook_subscriptions,
        record.id,
        record,
      ),
      webhook_subscription_order: new_order,
      deleted_webhook_subscription_ids: dict.delete(
        staged.deleted_webhook_subscription_ids,
        record.id,
      ),
    )
  #(record, Store(..store, staged_state: new_staged))
}

/// Mark a webhook-subscription id as deleted. Mirrors
/// `deleteStagedWebhookSubscription`.
pub fn delete_staged_webhook_subscription(store: Store, id: String) -> Store {
  let staged = store.staged_state
  let new_staged =
    StagedState(
      ..staged,
      webhook_subscriptions: dict.delete(staged.webhook_subscriptions, id),
      deleted_webhook_subscription_ids: dict.insert(
        staged.deleted_webhook_subscription_ids,
        id,
        True,
      ),
    )
  Store(..store, staged_state: new_staged)
}

/// Look up the effective webhook subscription for an id. Staged wins
/// over base; any "deleted" marker on either side suppresses the record.
/// Mirrors `getEffectiveWebhookSubscriptionById`.
pub fn get_effective_webhook_subscription_by_id(
  store: Store,
  id: String,
) -> Option(WebhookSubscriptionRecord) {
  let deleted =
    dict_has(store.base_state.deleted_webhook_subscription_ids, id)
    || dict_has(store.staged_state.deleted_webhook_subscription_ids, id)
  case deleted {
    True -> None
    False ->
      case dict.get(store.staged_state.webhook_subscriptions, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.webhook_subscriptions, id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

/// List every effective webhook subscription the store knows about.
/// Mirrors `listEffectiveWebhookSubscriptions`. Ordered records (those
/// tracked by the `webhookSubscriptionOrder` arrays) come first,
/// followed by any unordered staged/base records sorted by id.
pub fn list_effective_webhook_subscriptions(
  store: Store,
) -> List(WebhookSubscriptionRecord) {
  let ordered_ids =
    list.append(
      store.base_state.webhook_subscription_order,
      store.staged_state.webhook_subscription_order,
    )
    |> dedupe_strings()
  let ordered_records =
    list.filter_map(ordered_ids, fn(id) {
      case get_effective_webhook_subscription_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  let ordered_set = list_to_set(ordered_ids)
  let merged =
    dict.merge(
      store.base_state.webhook_subscriptions,
      store.staged_state.webhook_subscriptions,
    )
  let unordered_ids =
    dict.keys(merged)
    |> list.filter(fn(id) { !dict_has(ordered_set, id) })
    |> list.sort(string_compare)
  let unordered_records =
    list.filter_map(unordered_ids, fn(id) {
      case get_effective_webhook_subscription_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  list.append(ordered_records, unordered_records)
}

// ---------------------------------------------------------------------------
// Online-store slices
// ---------------------------------------------------------------------------

pub fn upsert_base_online_store_content(
  store: Store,
  records: List(OnlineStoreContentRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    let staged = acc.staged_state
    let new_base =
      BaseState(
        ..base,
        online_store_content: dict.insert(
          base.online_store_content,
          record.id,
          record,
        ),
        online_store_content_order: append_unique_id(
          base.online_store_content_order,
          record.id,
        ),
        deleted_online_store_content_ids: dict.delete(
          base.deleted_online_store_content_ids,
          record.id,
        ),
      )
    let new_staged =
      StagedState(
        ..staged,
        deleted_online_store_content_ids: dict.delete(
          staged.deleted_online_store_content_ids,
          record.id,
        ),
      )
    Store(..acc, base_state: new_base, staged_state: new_staged)
  })
}

pub fn upsert_staged_online_store_content(
  store: Store,
  record: OnlineStoreContentRecord,
) -> #(OnlineStoreContentRecord, Store) {
  let staged = store.staged_state
  let already_known =
    list.contains(store.base_state.online_store_content_order, record.id)
    || list.contains(staged.online_store_content_order, record.id)
  let new_order = case already_known {
    True -> staged.online_store_content_order
    False -> list.append(staged.online_store_content_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      online_store_content: dict.insert(
        staged.online_store_content,
        record.id,
        record,
      ),
      online_store_content_order: new_order,
      deleted_online_store_content_ids: dict.delete(
        staged.deleted_online_store_content_ids,
        record.id,
      ),
    )
  #(record, Store(..store, staged_state: new_staged))
}

pub fn delete_staged_online_store_content(store: Store, id: String) -> Store {
  let staged = store.staged_state
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      online_store_content: dict.delete(staged.online_store_content, id),
      deleted_online_store_content_ids: dict.insert(
        staged.deleted_online_store_content_ids,
        id,
        True,
      ),
    ),
  )
}

pub fn get_effective_online_store_content_by_id(
  store: Store,
  id: String,
) -> Option(OnlineStoreContentRecord) {
  let deleted =
    dict_has(store.base_state.deleted_online_store_content_ids, id)
    || dict_has(store.staged_state.deleted_online_store_content_ids, id)
  case deleted {
    True -> None
    False ->
      case dict.get(store.staged_state.online_store_content, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.online_store_content, id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

pub fn list_effective_online_store_content(
  store: Store,
  kind: String,
) -> List(OnlineStoreContentRecord) {
  let ordered_ids =
    list.append(
      store.base_state.online_store_content_order,
      store.staged_state.online_store_content_order,
    )
    |> dedupe_strings()
  let ordered_records =
    list.filter_map(ordered_ids, fn(id) {
      case get_effective_online_store_content_by_id(store, id) {
        Some(record) if record.kind == kind -> Ok(record)
        _ -> Error(Nil)
      }
    })
  let ordered_set = list_to_set(ordered_ids)
  let merged =
    dict.merge(
      store.base_state.online_store_content,
      store.staged_state.online_store_content,
    )
  let unordered_ids =
    dict.keys(merged)
    |> list.filter(fn(id) { !dict_has(ordered_set, id) })
    |> list.sort(string_compare)
  let unordered_records =
    list.filter_map(unordered_ids, fn(id) {
      case get_effective_online_store_content_by_id(store, id) {
        Some(record) if record.kind == kind -> Ok(record)
        _ -> Error(Nil)
      }
    })
  list.append(ordered_records, unordered_records)
}

pub fn upsert_base_online_store_integrations(
  store: Store,
  records: List(OnlineStoreIntegrationRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    let staged = acc.staged_state
    let new_base =
      BaseState(
        ..base,
        online_store_integrations: dict.insert(
          base.online_store_integrations,
          record.id,
          record,
        ),
        online_store_integration_order: append_unique_id(
          base.online_store_integration_order,
          record.id,
        ),
        deleted_online_store_integration_ids: dict.delete(
          base.deleted_online_store_integration_ids,
          record.id,
        ),
      )
    let new_staged =
      StagedState(
        ..staged,
        deleted_online_store_integration_ids: dict.delete(
          staged.deleted_online_store_integration_ids,
          record.id,
        ),
      )
    Store(..acc, base_state: new_base, staged_state: new_staged)
  })
}

pub fn upsert_staged_online_store_integration(
  store: Store,
  record: OnlineStoreIntegrationRecord,
) -> #(OnlineStoreIntegrationRecord, Store) {
  let staged = store.staged_state
  let already_known =
    list.contains(store.base_state.online_store_integration_order, record.id)
    || list.contains(staged.online_store_integration_order, record.id)
  let new_order = case already_known {
    True -> staged.online_store_integration_order
    False -> list.append(staged.online_store_integration_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      online_store_integrations: dict.insert(
        staged.online_store_integrations,
        record.id,
        record,
      ),
      online_store_integration_order: new_order,
      deleted_online_store_integration_ids: dict.delete(
        staged.deleted_online_store_integration_ids,
        record.id,
      ),
    )
  #(record, Store(..store, staged_state: new_staged))
}

pub fn delete_staged_online_store_integration(
  store: Store,
  id: String,
) -> Store {
  let staged = store.staged_state
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      online_store_integrations: dict.delete(
        staged.online_store_integrations,
        id,
      ),
      deleted_online_store_integration_ids: dict.insert(
        staged.deleted_online_store_integration_ids,
        id,
        True,
      ),
    ),
  )
}

pub fn get_effective_online_store_integration_by_id(
  store: Store,
  id: String,
) -> Option(OnlineStoreIntegrationRecord) {
  let deleted =
    dict_has(store.base_state.deleted_online_store_integration_ids, id)
    || dict_has(store.staged_state.deleted_online_store_integration_ids, id)
  case deleted {
    True -> None
    False ->
      case dict.get(store.staged_state.online_store_integrations, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.online_store_integrations, id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

pub fn list_effective_online_store_integrations(
  store: Store,
  kind: String,
) -> List(OnlineStoreIntegrationRecord) {
  let ordered_ids =
    list.append(
      store.base_state.online_store_integration_order,
      store.staged_state.online_store_integration_order,
    )
    |> dedupe_strings()
  let ordered_records =
    list.filter_map(ordered_ids, fn(id) {
      case get_effective_online_store_integration_by_id(store, id) {
        Some(record) if record.kind == kind -> Ok(record)
        _ -> Error(Nil)
      }
    })
  let ordered_set = list_to_set(ordered_ids)
  let merged =
    dict.merge(
      store.base_state.online_store_integrations,
      store.staged_state.online_store_integrations,
    )
  let unordered_ids =
    dict.keys(merged)
    |> list.filter(fn(id) { !dict_has(ordered_set, id) })
    |> list.sort(string_compare)
  let unordered_records =
    list.filter_map(unordered_ids, fn(id) {
      case get_effective_online_store_integration_by_id(store, id) {
        Some(record) if record.kind == kind -> Ok(record)
        _ -> Error(Nil)
      }
    })
  list.append(ordered_records, unordered_records)
}

pub fn upsert_base_url_redirects(
  store: Store,
  records: List(UrlRedirectRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    let staged = acc.staged_state
    let new_base =
      BaseState(
        ..base,
        url_redirects: dict.insert(base.url_redirects, record.id, record),
        url_redirect_order: append_unique_id(base.url_redirect_order, record.id),
        deleted_url_redirect_ids: dict.delete(
          base.deleted_url_redirect_ids,
          record.id,
        ),
      )
    let new_staged =
      StagedState(
        ..staged,
        deleted_url_redirect_ids: dict.delete(
          staged.deleted_url_redirect_ids,
          record.id,
        ),
      )
    Store(..acc, base_state: new_base, staged_state: new_staged)
  })
}

pub fn upsert_staged_url_redirect(
  store: Store,
  record: UrlRedirectRecord,
) -> #(UrlRedirectRecord, Store) {
  let staged = store.staged_state
  let already_known =
    list.contains(store.base_state.url_redirect_order, record.id)
    || list.contains(staged.url_redirect_order, record.id)
  let new_order = case already_known {
    True -> staged.url_redirect_order
    False -> list.append(staged.url_redirect_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      url_redirects: dict.insert(staged.url_redirects, record.id, record),
      url_redirect_order: new_order,
      deleted_url_redirect_ids: dict.delete(
        staged.deleted_url_redirect_ids,
        record.id,
      ),
    )
  #(record, Store(..store, staged_state: new_staged))
}

pub fn get_effective_url_redirect_by_id(
  store: Store,
  id: String,
) -> Option(UrlRedirectRecord) {
  let deleted =
    dict_has(store.base_state.deleted_url_redirect_ids, id)
    || dict_has(store.staged_state.deleted_url_redirect_ids, id)
  case deleted {
    True -> None
    False ->
      case dict.get(store.staged_state.url_redirects, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.url_redirects, id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

pub fn list_effective_url_redirects(store: Store) -> List(UrlRedirectRecord) {
  let ordered_ids =
    list.append(
      store.base_state.url_redirect_order,
      store.staged_state.url_redirect_order,
    )
    |> dedupe_strings()
  let ordered_records =
    list.filter_map(ordered_ids, fn(id) {
      case get_effective_url_redirect_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  let ordered_set = list_to_set(ordered_ids)
  let merged =
    dict.merge(store.base_state.url_redirects, store.staged_state.url_redirects)
  let unordered_ids =
    dict.keys(merged)
    |> list.filter(fn(id) { !dict_has(ordered_set, id) })
    |> list.sort(string_compare)
  let unordered_records =
    list.filter_map(unordered_ids, fn(id) {
      case get_effective_url_redirect_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  list.append(ordered_records, unordered_records)
}

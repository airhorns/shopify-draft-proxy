//// Store operations for app and access-token records.

import gleam/dict
import gleam/list
import gleam/option.{type Option, None, Some}
import shopify_draft_proxy/state/store/shared.{
  append_unique_id, dedupe_strings, dict_has, find_app_in_dict,
  find_token_in_dict,
}
import shopify_draft_proxy/state/store/types.{
  type Store, BaseState, StagedState, Store,
}
import shopify_draft_proxy/state/types.{
  type AppInstallationRecord, type AppOneTimePurchaseRecord, type AppRecord,
  type AppSubscriptionLineItemRecord, type AppSubscriptionRecord,
  type AppUsageRecord, type DelegatedAccessTokenRecord,
} as types_mod

// ---------------------------------------------------------------------------
// Apps slice (Pass 15)
// ---------------------------------------------------------------------------

/// Upsert an `AppRecord` into the base state. Used by hydration to seed
/// upstream-known apps. Mirrors `upsertBaseAppInstallation` (the app
/// half) and the implicit "stage app" the TS uses when the proxy mints
/// its own.
pub fn upsert_base_app(store: Store, record: AppRecord) -> Store {
  let base = store.base_state
  let new_base =
    BaseState(
      ..base,
      apps: dict.insert(base.apps, record.id, record),
      app_order: append_unique_id(base.app_order, record.id),
    )
  Store(..store, base_state: new_base)
}

/// Stage an `AppRecord`. The TS handler calls `stageApp` when it mints
/// a default app for a fresh proxy. Returns the record (unchanged in
/// Gleam since values are already immutable) alongside the new store.
pub fn stage_app(store: Store, record: AppRecord) -> #(AppRecord, Store) {
  let staged = store.staged_state
  let already =
    dict_has(store.base_state.apps, record.id)
    || dict_has(staged.apps, record.id)
  let new_order = case already {
    True -> staged.app_order
    False -> list.append(staged.app_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      apps: dict.insert(staged.apps, record.id, record),
      app_order: new_order,
    )
  #(record, Store(..store, staged_state: new_staged))
}

/// Look up an effective app (staged-over-base). Mirrors
/// `getEffectiveAppById`.
pub fn get_effective_app_by_id(store: Store, id: String) -> Option(AppRecord) {
  case dict.get(store.staged_state.apps, id) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(store.base_state.apps, id) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

/// Find an effective app whose `handle` matches the given value.
/// Mirrors `findEffectiveAppByHandle`. Staged wins on a tie.
pub fn find_effective_app_by_handle(
  store: Store,
  handle: String,
) -> Option(AppRecord) {
  case
    find_app_in_dict(store.staged_state.apps, fn(a) { a.handle == Some(handle) })
  {
    Some(record) -> Some(record)
    None ->
      find_app_in_dict(store.base_state.apps, fn(a) { a.handle == Some(handle) })
  }
}

/// Find an effective app whose `api_key` matches the given value.
/// Mirrors `findEffectiveAppByApiKey`.
pub fn find_effective_app_by_api_key(
  store: Store,
  api_key: String,
) -> Option(AppRecord) {
  case
    find_app_in_dict(store.staged_state.apps, fn(a) {
      a.api_key == Some(api_key)
    })
  {
    Some(record) -> Some(record)
    None ->
      find_app_in_dict(store.base_state.apps, fn(a) {
        a.api_key == Some(api_key)
      })
  }
}

/// List every effective app. Mirrors the implicit pattern of
/// `listEffectiveApps` (TS doesn't expose one but the same merge rules
/// apply).
pub fn list_effective_apps(store: Store) -> List(AppRecord) {
  let ordered_ids =
    list.append(store.base_state.app_order, store.staged_state.app_order)
    |> dedupe_strings()
  list.filter_map(ordered_ids, fn(id) {
    case get_effective_app_by_id(store, id) {
      Some(record) -> Ok(record)
      None -> Error(Nil)
    }
  })
}

/// Upsert an installation + its app together. Mirrors
/// `upsertBaseAppInstallation`, which atomically writes both to base.
pub fn upsert_base_app_installation(
  store: Store,
  installation: AppInstallationRecord,
  app: AppRecord,
) -> Store {
  let store = upsert_base_app(store, app)
  let base = store.base_state
  let new_base =
    BaseState(
      ..base,
      app_installations: dict.insert(
        base.app_installations,
        installation.id,
        installation,
      ),
      app_installation_order: append_unique_id(
        base.app_installation_order,
        installation.id,
      ),
      current_installation_id: case base.current_installation_id {
        None -> Some(installation.id)
        existing -> existing
      },
    )
  Store(..store, base_state: new_base)
}

/// Stage an installation. Mirrors `stageAppInstallation`. If no
/// installation is registered as current, the new one becomes current.
pub fn stage_app_installation(
  store: Store,
  record: AppInstallationRecord,
) -> #(AppInstallationRecord, Store) {
  let staged = store.staged_state
  let already =
    dict_has(store.base_state.app_installations, record.id)
    || dict_has(staged.app_installations, record.id)
  let new_order = case already {
    True -> staged.app_installation_order
    False -> list.append(staged.app_installation_order, [record.id])
  }
  let new_current = case
    staged.current_installation_id,
    store.base_state.current_installation_id
  {
    None, None -> Some(record.id)
    Some(_), _ -> staged.current_installation_id
    None, Some(_) -> staged.current_installation_id
  }
  let new_staged =
    StagedState(
      ..staged,
      app_installations: dict.insert(
        staged.app_installations,
        record.id,
        record,
      ),
      app_installation_order: new_order,
      current_installation_id: new_current,
    )
  #(record, Store(..store, staged_state: new_staged))
}

/// Look up an effective installation by id.
pub fn get_effective_app_installation_by_id(
  store: Store,
  id: String,
) -> Option(AppInstallationRecord) {
  case dict.get(store.staged_state.app_installations, id) {
    Ok(record) -> visible_app_installation(record)
    Error(_) ->
      case dict.get(store.base_state.app_installations, id) {
        Ok(record) -> visible_app_installation(record)
        Error(_) -> None
      }
  }
}

fn visible_app_installation(
  record: AppInstallationRecord,
) -> Option(AppInstallationRecord) {
  case record.uninstalled_at {
    Some(_) -> None
    None -> Some(record)
  }
}

/// Return the effective current installation, if one is registered.
/// Staged wins; falls back to base. Mirrors `getCurrentAppInstallation`.
pub fn get_current_app_installation(
  store: Store,
) -> Option(AppInstallationRecord) {
  case store.staged_state.current_installation_id {
    Some(id) -> get_effective_app_installation_by_id(store, id)
    None ->
      case store.base_state.current_installation_id {
        Some(id) -> get_effective_app_installation_by_id(store, id)
        None -> None
      }
  }
}

/// Stage an `AppSubscriptionRecord`. Mirrors `stageAppSubscription`.
pub fn stage_app_subscription(
  store: Store,
  record: AppSubscriptionRecord,
) -> #(AppSubscriptionRecord, Store) {
  let staged = store.staged_state
  let already =
    dict_has(store.base_state.app_subscriptions, record.id)
    || dict_has(staged.app_subscriptions, record.id)
  let new_order = case already {
    True -> staged.app_subscription_order
    False -> list.append(staged.app_subscription_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      app_subscriptions: dict.insert(
        staged.app_subscriptions,
        record.id,
        record,
      ),
      app_subscription_order: new_order,
    )
  #(record, Store(..store, staged_state: new_staged))
}

/// Look up an effective subscription by id.
pub fn get_effective_app_subscription_by_id(
  store: Store,
  id: String,
) -> Option(AppSubscriptionRecord) {
  case dict.get(store.staged_state.app_subscriptions, id) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(store.base_state.app_subscriptions, id) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

/// Stage an `AppSubscriptionLineItemRecord`. Mirrors
/// `stageAppSubscriptionLineItem`.
pub fn stage_app_subscription_line_item(
  store: Store,
  record: AppSubscriptionLineItemRecord,
) -> #(AppSubscriptionLineItemRecord, Store) {
  let staged = store.staged_state
  let already =
    dict_has(store.base_state.app_subscription_line_items, record.id)
    || dict_has(staged.app_subscription_line_items, record.id)
  let new_order = case already {
    True -> staged.app_subscription_line_item_order
    False -> list.append(staged.app_subscription_line_item_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      app_subscription_line_items: dict.insert(
        staged.app_subscription_line_items,
        record.id,
        record,
      ),
      app_subscription_line_item_order: new_order,
    )
  #(record, Store(..store, staged_state: new_staged))
}

/// Look up a line item by id.
pub fn get_effective_app_subscription_line_item_by_id(
  store: Store,
  id: String,
) -> Option(AppSubscriptionLineItemRecord) {
  case dict.get(store.staged_state.app_subscription_line_items, id) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(store.base_state.app_subscription_line_items, id) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

/// Stage an `AppOneTimePurchaseRecord`. Mirrors
/// `stageAppOneTimePurchase`.
pub fn stage_app_one_time_purchase(
  store: Store,
  record: AppOneTimePurchaseRecord,
) -> #(AppOneTimePurchaseRecord, Store) {
  let staged = store.staged_state
  let already =
    dict_has(store.base_state.app_one_time_purchases, record.id)
    || dict_has(staged.app_one_time_purchases, record.id)
  let new_order = case already {
    True -> staged.app_one_time_purchase_order
    False -> list.append(staged.app_one_time_purchase_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      app_one_time_purchases: dict.insert(
        staged.app_one_time_purchases,
        record.id,
        record,
      ),
      app_one_time_purchase_order: new_order,
    )
  #(record, Store(..store, staged_state: new_staged))
}

/// Look up a one-time purchase by id.
pub fn get_effective_app_one_time_purchase_by_id(
  store: Store,
  id: String,
) -> Option(AppOneTimePurchaseRecord) {
  case dict.get(store.staged_state.app_one_time_purchases, id) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(store.base_state.app_one_time_purchases, id) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

/// Stage an `AppUsageRecord`. Mirrors `stageAppUsageRecord`.
pub fn stage_app_usage_record(
  store: Store,
  record: AppUsageRecord,
) -> #(AppUsageRecord, Store) {
  let staged = store.staged_state
  let already =
    dict_has(store.base_state.app_usage_records, record.id)
    || dict_has(staged.app_usage_records, record.id)
  let new_order = case already {
    True -> staged.app_usage_record_order
    False -> list.append(staged.app_usage_record_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      app_usage_records: dict.insert(
        staged.app_usage_records,
        record.id,
        record,
      ),
      app_usage_record_order: new_order,
    )
  #(record, Store(..store, staged_state: new_staged))
}

/// Look up a usage record by id.
pub fn get_effective_app_usage_record_by_id(
  store: Store,
  id: String,
) -> Option(AppUsageRecord) {
  case dict.get(store.staged_state.app_usage_records, id) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(store.base_state.app_usage_records, id) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

/// List every effective usage record attached to a given line item.
/// Mirrors `listEffectiveAppUsageRecordsForLineItem`. Staged-over-base.
pub fn list_effective_app_usage_records_for_line_item(
  store: Store,
  line_item_id: String,
) -> List(AppUsageRecord) {
  let ordered_ids =
    list.append(
      store.base_state.app_usage_record_order,
      store.staged_state.app_usage_record_order,
    )
    |> dedupe_strings()
  list.filter_map(ordered_ids, fn(id) {
    case get_effective_app_usage_record_by_id(store, id) {
      Some(record) ->
        case record.subscription_line_item_id == line_item_id {
          True -> Ok(record)
          False -> Error(Nil)
        }
      None -> Error(Nil)
    }
  })
}

/// Stage a delegated access token. Mirrors `stageDelegatedAccessToken`.
pub fn stage_delegated_access_token(
  store: Store,
  record: DelegatedAccessTokenRecord,
) -> #(DelegatedAccessTokenRecord, Store) {
  let staged = store.staged_state
  let already =
    dict_has(store.base_state.delegated_access_tokens, record.id)
    || dict_has(staged.delegated_access_tokens, record.id)
  let new_order = case already {
    True -> staged.delegated_access_token_order
    False -> list.append(staged.delegated_access_token_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      delegated_access_tokens: dict.insert(
        staged.delegated_access_tokens,
        record.id,
        record,
      ),
      delegated_access_token_order: new_order,
    )
  #(record, Store(..store, staged_state: new_staged))
}

/// Find a delegated access token by sha256 hash. Mirrors
/// `findDelegatedAccessTokenByHash`. Searches staged before base.
pub fn find_delegated_access_token_by_hash(
  store: Store,
  hash: String,
) -> Option(DelegatedAccessTokenRecord) {
  case
    find_token_in_dict(store.staged_state.delegated_access_tokens, fn(t) {
      t.access_token_sha256 == hash && t.destroyed_at == None
    })
  {
    Some(record) -> Some(record)
    None ->
      find_token_in_dict(store.base_state.delegated_access_tokens, fn(t) {
        t.access_token_sha256 == hash && t.destroyed_at == None
      })
  }
}

/// Mark a delegated access token destroyed. Mirrors
/// `destroyDelegatedAccessToken`.
pub fn destroy_delegated_access_token(
  store: Store,
  id: String,
  destroyed_at: String,
) -> Store {
  case
    case dict.get(store.staged_state.delegated_access_tokens, id) {
      Ok(record) -> Some(record)
      Error(_) ->
        case dict.get(store.base_state.delegated_access_tokens, id) {
          Ok(record) -> Some(record)
          Error(_) -> None
        }
    }
  {
    None -> store
    Some(record) -> {
      let updated =
        types_mod.DelegatedAccessTokenRecord(
          ..record,
          destroyed_at: Some(destroyed_at),
        )
      let #(_, new_store) = stage_delegated_access_token(store, updated)
      new_store
    }
  }
}

//// Apps mutation handling.

import gleam/dict.{type Dict}
import gleam/float
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/order
import gleam/result
import gleam/string
import shopify_draft_proxy/crypto
import shopify_draft_proxy/graphql/ast.{type Selection, Field}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/apps/delegate_tokens
import shopify_draft_proxy/proxy/apps/serializers
import shopify_draft_proxy/proxy/apps/types as app_types
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, get_document_fragments, get_field_response_key,
}
import shopify_draft_proxy/proxy/mutation_helpers.{
  type LogDraft, type MutationOutcome, MutationOutcome, single_root_log_draft,
}
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/state/iso_timestamp
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/store/types as store_types
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type AppInstallationRecord, type AppRecord, type AppSubscriptionLineItemRecord,
  type AppSubscriptionRecord, type AppUsageRecord,
  type DelegatedAccessTokenRecord, type Money, AccessScopeRecord,
  AppInstallationRecord, AppOneTimePurchaseRecord, AppRecord,
  AppRecurringPricing, AppSubscriptionLineItemPlan,
  AppSubscriptionLineItemRecord, AppSubscriptionRecord, AppUsagePricing,
  AppUsageRecord, DelegatedAccessTokenRecord, Money,
}

/// Predicate matching the TS `APP_MUTATION_ROOTS` set.
@internal
pub fn is_app_mutation_root(name: String) -> Bool {
  case name {
    "appPurchaseOneTimeCreate" -> True
    "appSubscriptionCreate" -> True
    "appSubscriptionCancel" -> True
    "appSubscriptionLineItemUpdate" -> True
    "appSubscriptionTrialExtend" -> True
    "appUsageRecordCreate" -> True
    "appRevokeAccessScopes" -> True
    "appUninstall" -> True
    "delegateAccessTokenCreate" -> True
    "delegateAccessTokenDestroy" -> True
    _ -> False
  }
}

/// Outcome of an apps mutation. Mirrors the saved-search/webhook-subscription
/// outcome shape: a JSON envelope (`{"data": ...}` or `{"errors": ...}`),
/// the updated store and identity registry, and the staged GIDs.
/// User-error payload emitted on a mutation failure. Mirrors the apps
/// `app_types.UserError` shape in TS: an optional `code` and a path that defaults
/// to an empty list.
type DecimalAmount {
  DecimalAmount(sign: Int, whole: String, fraction: String)
}

type MutationFieldResult {
  MutationFieldResult(
    key: String,
    payload: Json,
    staged_resource_ids: List(String),
    log_drafts: List(LogDraft),
  )
}

/// Process an apps mutation document. Mirrors `handleAppMutation`. Each
/// mutation handler stages its records and returns a payload; the
/// outcomes are combined into a single `{"data": {...}}` envelope. Apps
/// mutations don't currently produce top-level error envelopes — every
/// failure mode is surfaced through `userErrors` instead.
@internal
pub fn process_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> MutationOutcome {
  case root_field.get_root_fields(document) {
    Error(err) -> mutation_helpers.parse_failed_outcome(store, identity, err)
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      handle_mutation_fields(
        store,
        identity,
        request_path,
        upstream.origin,
        upstream.headers,
        document,
        fields,
        fragments,
        variables,
      )
    }
  }
}

fn handle_mutation_fields(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  origin: String,
  headers: Dict(String, String),
  document: String,
  fields: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationOutcome {
  let initial = #([], store, identity, [], [])
  let #(data_entries, final_store, final_identity, all_staged, all_drafts) =
    list.fold(fields, initial, fn(acc, field) {
      let #(entries, current_store, current_identity, staged_ids, drafts) = acc
      case field {
        Field(name: name, ..) -> {
          let dispatch = case name.value {
            "appUninstall" ->
              Some(handle_uninstall(
                current_store,
                current_identity,
                request_path,
                origin,
                document,
                field,
                fragments,
                variables,
              ))
            "appRevokeAccessScopes" ->
              Some(handle_revoke_access_scopes(
                current_store,
                current_identity,
                request_path,
                origin,
                document,
                field,
                fragments,
                variables,
              ))
            "delegateAccessTokenCreate" ->
              Some(handle_delegate_create(
                current_store,
                current_identity,
                request_path,
                document,
                headers,
                field,
                fragments,
                variables,
              ))
            "delegateAccessTokenDestroy" ->
              Some(handle_delegate_destroy(
                current_store,
                current_identity,
                request_path,
                document,
                headers,
                field,
                fragments,
                variables,
              ))
            "appPurchaseOneTimeCreate" ->
              Some(handle_purchase_create(
                current_store,
                current_identity,
                request_path,
                origin,
                document,
                field,
                fragments,
                variables,
              ))
            "appSubscriptionCreate" ->
              Some(handle_subscription_create(
                current_store,
                current_identity,
                request_path,
                origin,
                document,
                field,
                fragments,
                variables,
              ))
            "appSubscriptionCancel" ->
              Some(handle_subscription_cancel(
                current_store,
                current_identity,
                request_path,
                document,
                field,
                fragments,
                variables,
              ))
            "appSubscriptionLineItemUpdate" ->
              Some(handle_line_item_update(
                current_store,
                current_identity,
                request_path,
                origin,
                document,
                field,
                fragments,
                variables,
              ))
            "appSubscriptionTrialExtend" ->
              Some(handle_trial_extend(
                current_store,
                current_identity,
                request_path,
                document,
                field,
                fragments,
                variables,
              ))
            "appUsageRecordCreate" ->
              Some(handle_usage_record_create(
                current_store,
                current_identity,
                request_path,
                document,
                field,
                fragments,
                variables,
              ))
            _ -> None
          }
          case dispatch {
            None -> acc
            Some(#(result, next_store, next_identity)) -> #(
              list.append(entries, [#(result.key, result.payload)]),
              next_store,
              next_identity,
              list.append(staged_ids, result.staged_resource_ids),
              list.append(drafts, result.log_drafts),
            )
          }
        }
        _ -> acc
      }
    })
  MutationOutcome(
    data: json.object([#("data", json.object(data_entries))]),
    store: final_store,
    identity: final_identity,
    staged_resource_ids: all_staged,
    log_drafts: all_drafts,
  )
}

// ---------------------------------------------------------------------------
// Per-mutation handlers
// ---------------------------------------------------------------------------

fn handle_uninstall(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
  _origin: String,
  _document: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input_id = uninstall_input_id(args)

  case resolve_uninstall_target(store, input_id) {
    Error(user_errors) ->
      failed_uninstall_result(
        key,
        store,
        identity,
        field,
        fragments,
        user_errors,
      )
    Ok(#(app, installation)) -> {
      let #(timestamp, identity_after_ts) =
        synthetic_identity.make_synthetic_timestamp(identity)
      let #(store_after_cascade, cascaded_ids) =
        cascade_app_uninstall(store, installation, timestamp)
      let updated =
        AppInstallationRecord(
          ..installation,
          access_scopes: [],
          active_subscription_ids: [],
          uninstalled_at: Some(timestamp),
        )
      let #(_, store_staged) =
        store.stage_app_installation(store_after_cascade, updated)
      let staged_ids = [installation.id, ..cascaded_ids]
      let payload =
        serializers.project_uninstall_payload(Some(app), [], field, fragments)
      let draft = make_log_draft("appUninstall", staged_ids, store_types.Staged)
      #(
        MutationFieldResult(
          key: key,
          payload: payload,
          staged_resource_ids: staged_ids,
          log_drafts: [draft],
        ),
        store_staged,
        identity_after_ts,
      )
    }
  }
}

fn uninstall_input_id(
  args: Dict(String, root_field.ResolvedValue),
) -> Option(String) {
  case graphql_helpers.read_arg_object(args, "input") {
    Some(input) -> graphql_helpers.read_arg_string(input, "id")
    None -> None
  }
}

fn resolve_uninstall_target(
  store: Store,
  input_id: Option(String),
) -> Result(#(AppRecord, AppInstallationRecord), List(app_types.UserError)) {
  case input_id {
    Some(app_id) ->
      case store.get_effective_app_by_id(store, app_id) {
        None -> Error([app_uninstall_app_not_found_error(["id"])])
        Some(app) ->
          case find_effective_app_installation_by_app_id(store, app.id) {
            Some(installation) ->
              case authorize_uninstall_target(store, app.id, ["id"]) {
                Ok(Nil) -> Ok(#(app, installation))
                Error(errors) -> Error(errors)
              }
            None -> Error([app_uninstall_not_installed_error(["id"])])
          }
      }
    None ->
      case store.get_current_app_installation(store) {
        None -> Error([app_uninstall_not_installed_error(["base"])])
        Some(installation) ->
          case store.get_effective_app_by_id(store, installation.app_id) {
            None -> Error([app_uninstall_app_not_found_error(["base"])])
            Some(app) -> Ok(#(app, installation))
          }
      }
  }
}

fn authorize_uninstall_target(
  store: Store,
  target_app_id: String,
  field: List(String),
) -> Result(Nil, List(app_types.UserError)) {
  case store.get_current_app_installation(store) {
    Some(current) if current.app_id == target_app_id -> Ok(Nil)
    Some(current) ->
      case installation_has_access_scope(current, "apps") {
        True -> Ok(Nil)
        False -> Error([app_uninstall_insufficient_permissions_error(field)])
      }
    None -> Error([app_uninstall_insufficient_permissions_error(field)])
  }
}

fn installation_has_access_scope(
  installation: AppInstallationRecord,
  handle: String,
) -> Bool {
  installation.access_scopes
  |> list.any(fn(scope) { scope.handle == handle })
}

fn find_effective_app_installation_by_app_id(
  store: Store,
  app_id: String,
) -> Option(AppInstallationRecord) {
  list.append(
    store.base_state.app_installation_order,
    store.staged_state.app_installation_order,
  )
  |> list.filter_map(fn(id) {
    case store.get_effective_app_installation_by_id(store, id) {
      Some(installation) -> Ok(installation)
      None -> Error(Nil)
    }
  })
  |> list.find(fn(installation) { installation.app_id == app_id })
  |> result_option
}

fn result_option(value: Result(a, b)) -> Option(a) {
  case value {
    Ok(item) -> Some(item)
    Error(_) -> None
  }
}

fn app_uninstall_app_not_found_error(
  field: List(String),
) -> app_types.UserError {
  app_types.UserError(
    field: field,
    message: "The app cannot be found.",
    code: Some("APP_NOT_FOUND"),
  )
}

fn app_uninstall_not_installed_error(
  field: List(String),
) -> app_types.UserError {
  app_types.UserError(
    field: field,
    message: "App is not installed on this shop.",
    code: Some("APP_NOT_INSTALLED"),
  )
}

fn app_uninstall_insufficient_permissions_error(
  field: List(String),
) -> app_types.UserError {
  app_types.UserError(
    field: field,
    message: "You do not have permission to uninstall this app.",
    code: Some("INSUFFICIENT_PERMISSIONS"),
  )
}

fn failed_uninstall_result(
  key: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  errors: List(app_types.UserError),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let payload =
    serializers.project_uninstall_payload(None, errors, field, fragments)
  let draft = make_log_draft("appUninstall", [], store_types.Failed)
  #(
    MutationFieldResult(
      key: key,
      payload: payload,
      staged_resource_ids: [],
      log_drafts: [draft],
    ),
    store,
    identity,
  )
}

fn cascade_app_uninstall(
  store: Store,
  installation: AppInstallationRecord,
  timestamp: String,
) -> #(Store, List(String)) {
  let subscription_ids =
    list.append(
      installation.active_subscription_ids,
      installation.all_subscription_ids,
    )
    |> unique_strings
  let #(store_after_subscriptions, subscription_staged_ids) =
    subscription_ids
    |> list.fold(#(store, []), fn(acc, id) {
      let #(current_store, staged_ids) = acc
      case store.get_effective_app_subscription_by_id(current_store, id) {
        Some(subscription) ->
          case is_cancellable_subscription_status(subscription.status) {
            True -> {
              let cancelled =
                AppSubscriptionRecord(..subscription, status: "CANCELLED")
              let #(_, next_store) =
                store.stage_app_subscription(current_store, cancelled)
              #(next_store, [cancelled.id, ..staged_ids])
            }
            False -> #(current_store, staged_ids)
          }
        _ -> #(current_store, staged_ids)
      }
    })
  let #(store_after_tokens, token_staged_ids) =
    list_effective_delegated_access_tokens(store_after_subscriptions)
    |> list.fold(#(store_after_subscriptions, []), fn(acc, token) {
      let #(current_store, staged_ids) = acc
      case token.destroyed_at {
        Some(_) -> #(current_store, staged_ids)
        None -> {
          let next_store =
            store.destroy_delegated_access_token(
              current_store,
              token.id,
              timestamp,
            )
          #(next_store, [token.id, ..staged_ids])
        }
      }
    })
  #(store_after_tokens, list.append(subscription_staged_ids, token_staged_ids))
}

fn unique_strings(values: List(String)) -> List(String) {
  values
  |> list.fold([], fn(acc, value) {
    case list.contains(acc, value) {
      True -> acc
      False -> list.append(acc, [value])
    }
  })
}

fn list_effective_delegated_access_tokens(
  store: Store,
) -> List(DelegatedAccessTokenRecord) {
  list.append(
    store.base_state.delegated_access_token_order,
    store.staged_state.delegated_access_token_order,
  )
  |> unique_strings
  |> list.filter_map(fn(id) {
    case get_effective_delegated_access_token_by_id(store, id) {
      Some(token) -> Ok(token)
      None -> Error(Nil)
    }
  })
}

fn get_effective_delegated_access_token_by_id(
  store: Store,
  id: String,
) -> Option(DelegatedAccessTokenRecord) {
  case dict.get(store.staged_state.delegated_access_tokens, id) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(store.base_state.delegated_access_tokens, id) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

fn handle_revoke_access_scopes(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
  _origin: String,
  _document: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let requested_scopes = case dict.get(args, "scopes") {
    Ok(root_field.ListVal(items)) ->
      list.filter_map(items, fn(item) {
        case item {
          root_field.StringVal(s) -> Ok(s)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }

  case current_revoke_context(store) {
    Error(errors) ->
      failed_revoke_access_scopes_result(
        key,
        store,
        identity,
        field,
        fragments,
        errors,
        [],
      )
    Ok(#(installation, app)) -> {
      let current_handles =
        list.map(installation.access_scopes, fn(s) { s.handle })
      let required_handles =
        list.map(app.requested_access_scopes, fn(s) { s.handle })
      let errors =
        revoke_access_scope_errors(
          requested_scopes,
          current_handles,
          required_handles,
        )

      case errors {
        [] -> {
          let revoked =
            list.filter(installation.access_scopes, fn(scope) {
              list.contains(requested_scopes, scope.handle)
            })
          let updated =
            AppInstallationRecord(
              ..installation,
              access_scopes: list.filter(installation.access_scopes, fn(scope) {
                !list.contains(requested_scopes, scope.handle)
              }),
            )
          let #(_, store_staged) = store.stage_app_installation(store, updated)
          let payload =
            serializers.project_revoke_payload(revoked, [], field, fragments)
          let draft =
            make_log_draft(
              "appRevokeAccessScopes",
              [installation.id],
              store_types.Staged,
            )
          #(
            MutationFieldResult(
              key: key,
              payload: payload,
              staged_resource_ids: [installation.id],
              log_drafts: [draft],
            ),
            store_staged,
            identity,
          )
        }
        _ ->
          failed_revoke_access_scopes_result(
            key,
            store,
            identity,
            field,
            fragments,
            errors,
            [installation.id],
          )
      }
    }
  }
}

fn current_revoke_context(
  store: Store,
) -> Result(#(AppInstallationRecord, AppRecord), List(app_types.UserError)) {
  case store.get_current_app_installation(store) {
    Some(installation) ->
      case store.get_effective_app_by_id(store, installation.app_id) {
        Some(app) -> Ok(#(installation, app))
        None ->
          Error([
            app_types.UserError(
              field: ["base"],
              message: "Application cannot be found.",
              code: Some("APPLICATION_CANNOT_BE_FOUND"),
            ),
          ])
      }
    None ->
      case store.list_effective_apps(store) {
        [] ->
          Error([
            app_types.UserError(
              field: ["base"],
              message: "Source app is missing.",
              code: Some("MISSING_SOURCE_APP"),
            ),
          ])
        _ ->
          Error([
            app_types.UserError(
              field: ["base"],
              message: "App is not installed on this shop.",
              code: Some("APP_NOT_INSTALLED"),
            ),
          ])
      }
  }
}

fn failed_revoke_access_scopes_result(
  key: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  errors: List(app_types.UserError),
  staged_resource_ids: List(String),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let payload = serializers.project_revoke_payload([], errors, field, fragments)
  let draft =
    make_log_draft(
      "appRevokeAccessScopes",
      staged_resource_ids,
      store_types.Failed,
    )
  #(
    MutationFieldResult(
      key: key,
      payload: payload,
      staged_resource_ids: staged_resource_ids,
      log_drafts: [draft],
    ),
    store,
    identity,
  )
}

fn revoke_access_scope_errors(
  requested_scopes: List(String),
  current_handles: List(String),
  required_handles: List(String),
) -> List(app_types.UserError) {
  requested_scopes
  |> list.filter_map(fn(scope) {
    case is_known_shopify_access_scope(scope) {
      False ->
        Ok(app_types.UserError(
          field: ["scopes"],
          message: "The requested list of scopes to revoke includes invalid handles.",
          code: Some("UNKNOWN_SCOPES"),
        ))
      True ->
        case list.contains(required_handles, scope) {
          True ->
            Ok(app_types.UserError(
              field: ["scopes"],
              message: "Scopes that are declared as required cannot be revoked.",
              code: Some("CANNOT_REVOKE_REQUIRED_SCOPES"),
            ))
          False ->
            case scope_implied_by_granted_scope(scope, current_handles) {
              True ->
                Ok(app_types.UserError(
                  field: ["scopes"],
                  message: "Scopes that are implied by other granted scopes cannot be revoked.",
                  code: Some("CANNOT_REVOKE_IMPLIED_SCOPES"),
                ))
              False ->
                case list.contains(current_handles, scope) {
                  True -> Error(Nil)
                  False ->
                    Ok(app_types.UserError(
                      field: ["scopes"],
                      message: "Scopes that are not declared cannot be revoked.",
                      code: Some("CANNOT_REVOKE_UNDECLARED_SCOPES"),
                    ))
                }
            }
        }
    }
  })
}

fn scope_implied_by_granted_scope(
  scope: String,
  current_handles: List(String),
) -> Bool {
  case string.starts_with(scope, "read_") {
    False -> False
    True -> {
      let write_scope = "write_" <> string.drop_start(scope, 5)
      list.contains(current_handles, write_scope)
    }
  }
}

fn is_known_shopify_access_scope(scope: String) -> Bool {
  list.contains(shopify_access_scope_catalog(), scope)
}

fn shopify_access_scope_catalog() -> List(String) {
  [
    "read_all_orders",
    "write_all_orders",
    "read_analytics",
    "read_apps",
    "write_apps",
    "read_assigned_fulfillment_orders",
    "write_assigned_fulfillment_orders",
    "read_cart_transforms",
    "write_cart_transforms",
    "read_cash_tracking",
    "write_cash_tracking",
    "read_checkouts",
    "write_checkouts",
    "read_companies",
    "write_companies",
    "read_content",
    "write_content",
    "read_custom_pixels",
    "write_custom_pixels",
    "read_customer_data_erasure",
    "write_customer_data_erasure",
    "read_customer_events",
    "read_customer_merge",
    "write_customer_merge",
    "read_customers",
    "write_customers",
    "read_delivery_customizations",
    "write_delivery_customizations",
    "read_delivery_promises",
    "write_delivery_promises",
    "read_discounts",
    "write_discounts",
    "read_discovery",
    "read_domains",
    "write_domains",
    "read_draft_orders",
    "write_draft_orders",
    "read_files",
    "write_files",
    "read_fulfillment_constraint_rules",
    "write_fulfillment_constraint_rules",
    "read_fulfillments",
    "write_fulfillments",
    "read_gift_card_transactions",
    "write_gift_card_transactions",
    "read_gift_cards",
    "write_gift_cards",
    "read_inventory",
    "write_inventory",
    "read_inventory_shipments",
    "write_inventory_shipments",
    "read_inventory_transfers",
    "write_inventory_transfers",
    "read_legal_policies",
    "write_legal_policies",
    "read_locales",
    "write_locales",
    "read_locations",
    "write_locations",
    "read_marketing_events",
    "write_marketing_events",
    "read_markets",
    "write_markets",
    "read_merchant_managed_fulfillment_orders",
    "write_merchant_managed_fulfillment_orders",
    "read_metaobject_definitions",
    "write_metaobject_definitions",
    "read_metaobjects",
    "write_metaobjects",
    "read_online_store_navigation",
    "write_online_store_navigation",
    "read_order_edits",
    "write_order_edits",
    "read_orders",
    "write_orders",
    "read_own_subscription_contracts",
    "write_own_subscription_contracts",
    "read_payment_customizations",
    "write_payment_customizations",
    "read_payment_terms",
    "write_payment_terms",
    "read_privacy_settings",
    "write_privacy_settings",
    "read_product_listings",
    "read_products",
    "write_products",
    "read_publications",
    "write_publications",
    "read_purchase_options",
    "write_purchase_options",
    "read_resource_feedbacks",
    "write_resource_feedbacks",
    "read_returns",
    "write_returns",
    "read_shipping",
    "write_shipping",
    "read_shopify_payments",
    "read_shopify_payments_accounts",
    "read_shopify_payments_dispute_evidences",
    "write_shopify_payments_dispute_evidences",
    "read_shopify_payments_disputes",
    "write_shopify_payments_disputes",
    "read_shopify_payments_payouts",
    "read_store_credit_account_transactions",
    "write_store_credit_account_transactions",
    "read_store_credit_accounts",
    "read_taxes",
    "write_taxes",
    "read_themes",
    "write_themes",
    "read_third_party_fulfillment_orders",
    "write_third_party_fulfillment_orders",
    "read_translations",
    "write_translations",
    "read_users",
    "read_validations",
    "write_validations",
    "unauthenticated_read_product_listings",
  ]
}

fn handle_delegate_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
  _document: String,
  request_headers: Dict(String, String),
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input = case dict.get(args, "input") {
    Ok(root_field.ObjectVal(d)) -> d
    _ -> dict.new()
  }
  let access_scopes = read_delegate_access_scopes(input)
  let expires_in = case dict.get(input, "expiresIn") {
    Ok(root_field.IntVal(n)) -> Some(n)
    _ -> None
  }
  case
    delegate_create_user_errors(
      store,
      identity,
      request_headers,
      access_scopes,
      expires_in,
    )
  {
    [_, ..] as errors ->
      failed_delegate_create_result(
        key,
        store,
        identity,
        field,
        fragments,
        errors,
      )
    [] ->
      stage_delegate_create(
        key,
        store,
        identity,
        field,
        fragments,
        access_scopes,
        expires_in,
        request_headers,
      )
  }
}

fn read_delegate_access_scopes(
  input: Dict(String, root_field.ResolvedValue),
) -> List(String) {
  case dict.get(input, "delegateAccessScope") {
    Ok(_) ->
      graphql_helpers.read_arg_string_list(input, "delegateAccessScope")
      |> option.unwrap([])
    Error(_) ->
      graphql_helpers.read_arg_string_list(input, "accessScopes")
      |> option.unwrap([])
  }
}

fn delegate_create_user_errors(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_headers: Dict(String, String),
  access_scopes: List(String),
  expires_in: Option(Int),
) -> List(app_types.DelegateAccessTokenUserError) {
  case access_scopes {
    [] -> [
      app_types.DelegateAccessTokenUserError(
        field: None,
        message: "The access scope can't be empty.",
        code: Some("EMPTY_ACCESS_SCOPE"),
      ),
    ]
    _ ->
      case delegate_tokens.active_parent_is_delegate(store, request_headers) {
        True -> [
          app_types.DelegateAccessTokenUserError(
            field: None,
            message: "The parent access token can't be a delegate token.",
            code: Some("DELEGATE_ACCESS_TOKEN"),
          ),
        ]
        False ->
          case expires_in {
            Some(n) ->
              case n <= 0 {
                True -> [
                  app_types.DelegateAccessTokenUserError(
                    field: None,
                    message: "The expires_in value must be greater than 0.",
                    code: Some("NEGATIVE_EXPIRES_IN"),
                  ),
                ]
                False ->
                  case
                    delegate_expires_after_parent_error(
                      identity,
                      request_headers,
                      n,
                    )
                  {
                    Some(error) -> [error]
                    None -> delegate_unknown_scope_errors(access_scopes)
                  }
              }
            None -> delegate_unknown_scope_errors(access_scopes)
          }
      }
  }
}

fn delegate_expires_after_parent_error(
  identity: SyntheticIdentityRegistry,
  request_headers: Dict(String, String),
  expires_in: Int,
) -> Option(app_types.DelegateAccessTokenUserError) {
  case delegate_tokens.active_access_token_expires_at(request_headers) {
    None -> None
    Some(parent_expires_at) ->
      case
        parent_expires_at_is_before_delegate_expiry(
          identity,
          parent_expires_at,
          expires_in,
        )
      {
        True ->
          Some(app_types.DelegateAccessTokenUserError(
            field: None,
            message: "The delegate token can't expire after the parent token.",
            code: Some("EXPIRES_AFTER_PARENT"),
          ))
        False -> None
      }
  }
}

fn parent_expires_at_is_before_delegate_expiry(
  identity: SyntheticIdentityRegistry,
  parent_expires_at: String,
  expires_in: Int,
) -> Bool {
  let #(now, _) = synthetic_identity.make_synthetic_timestamp(identity)
  case
    iso_timestamp.parse_iso(now),
    iso_timestamp.parse_iso(parent_expires_at)
  {
    Ok(now_ms), Ok(parent_ms) -> now_ms + expires_in * 1000 > parent_ms
    _, _ -> False
  }
}

fn delegate_unknown_scope_errors(
  access_scopes: List(String),
) -> List(app_types.DelegateAccessTokenUserError) {
  let unknown =
    list.filter(access_scopes, fn(scope) {
      !is_known_shopify_access_scope(scope)
    })
  case unknown {
    [] -> []
    _ -> [
      app_types.DelegateAccessTokenUserError(
        field: None,
        message: "The access scope is invalid: " <> string.join(unknown, ", "),
        code: Some("UNKNOWN_SCOPES"),
      ),
    ]
  }
}

fn failed_delegate_create_result(
  key: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  errors: List(app_types.DelegateAccessTokenUserError),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let payload =
    serializers.project_delegate_create_payload(
      store,
      None,
      [],
      None,
      None,
      errors,
      field,
      fragments,
    )
  let draft =
    make_log_draft("delegateAccessTokenCreate", [], store_types.Failed)
  #(
    MutationFieldResult(
      key: key,
      payload: payload,
      staged_resource_ids: [],
      log_drafts: [draft],
    ),
    store,
    identity,
  )
}

fn stage_delegate_create(
  key: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  access_scopes: List(String),
  expires_in: Option(Int),
  request_headers: Dict(String, String),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let #(token_gid, identity_after_id) =
    synthetic_identity.make_synthetic_gid(identity, "DelegateAccessToken")
  let #(timestamp, identity_after_ts) =
    synthetic_identity.make_synthetic_timestamp(identity_after_id)
  let raw_token = "shpat_delegate_proxy_" <> trailing_segment(token_gid)
  let record =
    DelegatedAccessTokenRecord(
      id: token_gid,
      api_client_id: delegate_tokens.caller_api_client_id(
        store,
        request_headers,
      ),
      parent_access_token_sha256: delegate_tokens.active_access_token(
        request_headers,
      )
        |> option.map(token_hash),
      access_token_sha256: token_hash(raw_token),
      access_token_preview: token_preview(raw_token),
      access_scopes: access_scopes,
      created_at: timestamp,
      expires_in: expires_in,
      destroyed_at: None,
    )
  let #(_, store_staged) = store.stage_delegated_access_token(store, record)
  let payload =
    serializers.project_delegate_create_payload(
      store_staged,
      Some(raw_token),
      access_scopes,
      Some(timestamp),
      expires_in,
      [],
      field,
      fragments,
    )
  let draft =
    make_log_draft("delegateAccessTokenCreate", [token_gid], store_types.Staged)
  #(
    MutationFieldResult(
      key: key,
      payload: payload,
      staged_resource_ids: [token_gid],
      log_drafts: [draft],
    ),
    store_staged,
    identity_after_ts,
  )
}

fn handle_delegate_destroy(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
  _document: String,
  request_headers: Dict(String, String),
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let access_token = graphql_helpers.read_arg_string(args, "accessToken")
  let active_token_hash =
    delegate_tokens.active_access_token(request_headers)
    |> option.map(token_hash)
  let token = case access_token {
    Some(raw) ->
      store.find_delegated_access_token_by_hash(store, token_hash(raw))
    None -> None
  }
  case token {
    None -> {
      let errors = case access_token, active_token_hash {
        Some(raw), Some(active_hash) -> {
          let supplied_hash = token_hash(raw)
          case
            supplied_hash == active_hash
            && !delegate_tokens.delegated_token_hash_exists(
              store,
              supplied_hash,
            )
          {
            True -> [
              delegate_tokens.destroy_error(
                "Can only delete delegate tokens.",
                "CAN_ONLY_DELETE_DELEGATE_TOKENS",
              ),
            ]
            False -> [
              delegate_tokens.destroy_error(
                "Access token does not exist.",
                "ACCESS_TOKEN_NOT_FOUND",
              ),
            ]
          }
        }
        _, _ -> [
          delegate_tokens.destroy_error(
            "Access token does not exist.",
            "ACCESS_TOKEN_NOT_FOUND",
          ),
        ]
      }
      let payload =
        serializers.project_delegate_destroy_payload(
          store,
          False,
          errors,
          field,
          fragments,
        )
      let draft =
        make_log_draft("delegateAccessTokenDestroy", [], store_types.Failed)
      #(
        MutationFieldResult(
          key: key,
          payload: payload,
          staged_resource_ids: [],
          log_drafts: [draft],
        ),
        store,
        identity,
      )
    }
    Some(record) -> {
      let allowed =
        record.api_client_id
        == delegate_tokens.caller_api_client_id(store, request_headers)
        && delegate_tokens.destroy_in_hierarchy(record, active_token_hash)
      case allowed {
        False -> {
          let payload =
            serializers.project_delegate_destroy_payload(
              store,
              False,
              [delegate_tokens.destroy_error("Access denied.", "ACCESS_DENIED")],
              field,
              fragments,
            )
          let draft =
            make_log_draft("delegateAccessTokenDestroy", [], store_types.Failed)
          #(
            MutationFieldResult(
              key: key,
              payload: payload,
              staged_resource_ids: [],
              log_drafts: [draft],
            ),
            store,
            identity,
          )
        }
        True -> {
          let #(timestamp, identity_after_ts) =
            synthetic_identity.make_synthetic_timestamp(identity)
          let store_after =
            store.destroy_delegated_access_token(store, record.id, timestamp)
          let payload =
            serializers.project_delegate_destroy_payload(
              store_after,
              True,
              [],
              field,
              fragments,
            )
          let draft =
            make_log_draft(
              "delegateAccessTokenDestroy",
              [record.id],
              store_types.Staged,
            )
          #(
            MutationFieldResult(
              key: key,
              payload: payload,
              staged_resource_ids: [record.id],
              log_drafts: [draft],
            ),
            store_after,
            identity_after_ts,
          )
        }
      }
    }
  }
}

fn handle_purchase_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
  origin: String,
  _document: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let name = graphql_helpers.read_arg_string(args, "name")
  let price = read_money_input(args, "price")
  let billing_currency = shop_billing_currency(store)
  let validation_errors =
    purchase_create_validation_errors(args, name, price, billing_currency)
  case validation_errors {
    [] ->
      stage_valid_purchase_create(
        store,
        identity,
        origin,
        key,
        name |> option.unwrap(""),
        price,
        graphql_helpers.read_arg_bool(args, "test") |> option.unwrap(False),
        field,
        fragments,
      )
    _ -> {
      let payload =
        serializers.project_purchase_create_payload(
          None,
          None,
          validation_errors,
          field,
          fragments,
        )
      let draft =
        make_log_draft("appPurchaseOneTimeCreate", [], store_types.Failed)
      #(
        MutationFieldResult(
          key: key,
          payload: payload,
          staged_resource_ids: [],
          log_drafts: [draft],
        ),
        store,
        identity,
      )
    }
  }
}

fn stage_valid_purchase_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  origin: String,
  key: String,
  name: String,
  price: Money,
  is_test: Bool,
  field: Selection,
  fragments: FragmentMap,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let #(installation, store_after_ensure, identity_after_ensure) =
    ensure_current_installation(store, identity, origin)
  let #(purchase_gid, identity_after_id) =
    synthetic_identity.make_synthetic_gid(
      identity_after_ensure,
      "AppPurchaseOneTime",
    )
  let #(timestamp, identity_after_ts) =
    synthetic_identity.make_synthetic_timestamp(identity_after_id)
  let status = case is_test {
    True -> "ACTIVE"
    False -> "PENDING"
  }
  let purchase =
    AppOneTimePurchaseRecord(
      id: purchase_gid,
      name: name,
      status: status,
      is_test: is_test,
      created_at: timestamp,
      price: price,
    )
  let #(_, store_with_purchase) =
    store.stage_app_one_time_purchase(store_after_ensure, purchase)
  let updated_installation =
    AppInstallationRecord(
      ..installation,
      one_time_purchase_ids: list.append(installation.one_time_purchase_ids, [
        purchase.id,
      ]),
    )
  let #(_, store_staged) =
    store.stage_app_installation(store_with_purchase, updated_installation)
  let payload =
    serializers.project_purchase_create_payload(
      Some(purchase),
      Some(confirmation_url(origin, "ApplicationCharge", purchase.id)),
      [],
      field,
      fragments,
    )
  let draft =
    make_log_draft(
      "appPurchaseOneTimeCreate",
      [purchase.id],
      store_types.Staged,
    )
  #(
    MutationFieldResult(
      key: key,
      payload: payload,
      staged_resource_ids: [purchase.id],
      log_drafts: [draft],
    ),
    store_staged,
    identity_after_ts,
  )
}

fn handle_subscription_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
  origin: String,
  _document: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let #(installation, store_after_ensure, identity_after_ensure) =
    ensure_current_installation(store, identity, origin)
  let #(sub_gid, identity_after_sub_id) =
    synthetic_identity.make_synthetic_gid(
      identity_after_ensure,
      "AppSubscription",
    )
  let line_item_inputs = case dict.get(args, "lineItems") {
    Ok(root_field.ListVal(items)) ->
      list.filter_map(items, fn(item) {
        case item {
          root_field.ObjectVal(d) -> Ok(d)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
  let #(line_items, store_after_lis, identity_after_lis) =
    list.index_fold(
      line_item_inputs,
      #([], store_after_ensure, identity_after_sub_id),
      fn(acc, input, index) {
        let #(records, current_store, current_identity) = acc
        let #(record, ident_next) =
          read_line_item_plan(current_identity, input, sub_gid, index + 1)
        let #(_, store_next) =
          store.stage_app_subscription_line_item(current_store, record)
        #(list.append(records, [record]), store_next, ident_next)
      },
    )
  let #(timestamp, identity_after_ts) =
    synthetic_identity.make_synthetic_timestamp(identity_after_lis)
  let is_test =
    graphql_helpers.read_arg_bool(args, "test")
    |> option.unwrap(False)
  let status = case is_test {
    True -> "ACTIVE"
    False -> "PENDING"
  }
  let current_period_end = case status {
    "ACTIVE" -> compute_current_period_end(timestamp, line_items, args)
    _ -> None
  }
  let subscription =
    AppSubscriptionRecord(
      id: sub_gid,
      name: option.unwrap(graphql_helpers.read_arg_string(args, "name"), ""),
      status: status,
      is_test: is_test,
      trial_days: graphql_helpers.read_arg_int(args, "trialDays"),
      current_period_end: current_period_end,
      created_at: timestamp,
      line_item_ids: list.map(line_items, fn(li) { li.id }),
    )
  let #(_, store_after_sub) =
    store.stage_app_subscription(store_after_lis, subscription)
  let updated_installation =
    AppInstallationRecord(
      ..installation,
      all_subscription_ids: list.append(installation.all_subscription_ids, [
        subscription.id,
      ]),
      active_subscription_ids: case subscription.status {
        "ACTIVE" ->
          append_unique(installation.active_subscription_ids, subscription.id)
        _ -> installation.active_subscription_ids
      },
    )
  let #(_, store_staged) =
    store.stage_app_installation(store_after_sub, updated_installation)
  let payload =
    serializers.project_subscription_create_payload(
      store_staged,
      Some(subscription),
      Some(confirmation_url(
        origin,
        "RecurringApplicationCharge",
        subscription.id,
      )),
      [],
      field,
      fragments,
    )
  let staged_ids =
    list.append([subscription.id], list.map(line_items, fn(li) { li.id }))
  let draft =
    make_log_draft("appSubscriptionCreate", staged_ids, store_types.Staged)
  #(
    MutationFieldResult(
      key: key,
      payload: payload,
      staged_resource_ids: staged_ids,
      log_drafts: [draft],
    ),
    store_staged,
    identity_after_ts,
  )
}

fn handle_subscription_cancel(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
  _document: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let subscription_id = graphql_helpers.read_arg_string(args, "id")
  let subscription = case subscription_id {
    Some(id) -> store.get_effective_app_subscription_by_id(store, id)
    None -> None
  }
  case subscription {
    None -> {
      let payload =
        serializers.project_subscription_payload(
          store,
          None,
          None,
          [
            app_types.UserError(
              field: ["id"],
              message: subscription_record_not_found_message(),
              code: None,
            ),
          ],
          field,
          fragments,
        )
      let draft =
        make_log_draft("appSubscriptionCancel", [], store_types.Failed)
      #(
        MutationFieldResult(
          key: key,
          payload: payload,
          staged_resource_ids: [],
          log_drafts: [draft],
        ),
        store,
        identity,
      )
    }
    Some(sub) -> {
      case is_cancellable_subscription_status(sub.status) {
        False -> {
          let payload =
            serializers.project_subscription_payload(
              store,
              None,
              None,
              [
                app_types.UserError(
                  field: ["id"],
                  message: subscription_invalid_cancel_transition_message(
                    sub.status,
                  ),
                  code: None,
                ),
              ],
              field,
              fragments,
            )
          let draft =
            make_log_draft("appSubscriptionCancel", [], store_types.Failed)
          #(
            MutationFieldResult(
              key: key,
              payload: payload,
              staged_resource_ids: [],
              log_drafts: [draft],
            ),
            store,
            identity,
          )
        }
        True -> {
          let cancelled = AppSubscriptionRecord(..sub, status: "CANCELLED")
          let #(_, store_after_sub) =
            store.stage_app_subscription(store, cancelled)
          let store_after_install = case
            store.get_current_app_installation(store_after_sub)
          {
            Some(install) -> {
              let updated =
                AppInstallationRecord(
                  ..install,
                  active_subscription_ids: list.filter(
                    install.active_subscription_ids,
                    fn(id) { id != cancelled.id },
                  ),
                )
              let #(_, store_next) =
                store.stage_app_installation(store_after_sub, updated)
              store_next
            }
            None -> store_after_sub
          }
          let payload =
            serializers.project_subscription_payload(
              store_after_install,
              Some(cancelled),
              None,
              [],
              field,
              fragments,
            )
          let draft =
            make_log_draft(
              "appSubscriptionCancel",
              [cancelled.id],
              store_types.Staged,
            )
          #(
            MutationFieldResult(
              key: key,
              payload: payload,
              staged_resource_ids: [cancelled.id],
              log_drafts: [draft],
            ),
            store_after_install,
            identity,
          )
        }
      }
    }
  }
}

fn is_cancellable_subscription_status(status: String) -> Bool {
  case status {
    "PENDING" | "ACCEPTED" | "ACTIVE" -> True
    _ -> False
  }
}

fn subscription_invalid_cancel_transition_message(status: String) -> String {
  "Cannot transition status via :cancel from :" <> string.lowercase(status)
}

fn subscription_record_not_found_message() -> String {
  "Couldn't find RecurringApplicationCharge"
}

fn handle_line_item_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
  origin: String,
  _document: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let line_item_id = graphql_helpers.read_arg_string(args, "id")
  case line_item_id {
    Some(id) -> {
      case valid_app_subscription_line_item_gid(id) {
        False ->
          line_item_update_failed(
            key,
            store,
            identity,
            field,
            fragments,
            app_types.UserError(
              field: ["id"],
              message: "Invalid app subscription line item id",
              code: None,
            ),
          )
        True ->
          handle_valid_line_item_update(
            key,
            store,
            identity,
            origin,
            field,
            fragments,
            args,
            id,
          )
      }
    }
    None ->
      line_item_update_failed(
        key,
        store,
        identity,
        field,
        fragments,
        app_types.UserError(
          field: ["id"],
          message: "Invalid app subscription line item id",
          code: None,
        ),
      )
  }
}

fn handle_valid_line_item_update(
  key: String,
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  origin: String,
  field: Selection,
  fragments: FragmentMap,
  args: Dict(String, root_field.ResolvedValue),
  line_item_id: String,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let line_item =
    store.get_effective_app_subscription_line_item_by_id(
      draft_store,
      line_item_id,
    )
  let subscription = case line_item {
    Some(li) ->
      store.get_effective_app_subscription_by_id(
        draft_store,
        li.subscription_id,
      )
    None -> None
  }
  case line_item, subscription {
    Some(li), Some(sub) -> {
      let capped = read_money_input(args, "cappedAmount")
      case li.plan.pricing_details {
        AppRecurringPricing(..) ->
          line_item_update_failed(
            key,
            draft_store,
            identity,
            field,
            fragments,
            app_types.UserError(
              field: ["cappedAmount"],
              message: "Only usage-pricing line items support cappedAmount updates",
              code: None,
            ),
          )
        AppUsagePricing(
          capped_amount: current_capped,
          balance_used: balance,
          interval: interval,
          terms: terms,
        ) ->
          case capped.currency_code != current_capped.currency_code {
            True ->
              line_item_update_failed(
                key,
                draft_store,
                identity,
                field,
                fragments,
                app_types.UserError(
                  field: ["cappedAmount"],
                  message: "Capped amount currency mismatch. Expected "
                    <> current_capped.currency_code,
                  code: None,
                ),
              )
            False ->
              case
                decimal_amount_greater_than(
                  capped.amount,
                  current_capped.amount,
                )
              {
                False ->
                  line_item_update_failed(
                    key,
                    draft_store,
                    identity,
                    field,
                    fragments,
                    app_types.UserError(
                      field: ["cappedAmount"],
                      message: "The capped amount must be greater than the existing capped amount",
                      code: None,
                    ),
                  )
                True -> {
                  let updated_pricing =
                    AppUsagePricing(
                      capped_amount: capped,
                      balance_used: balance,
                      interval: interval,
                      terms: terms,
                    )
                  let updated_line_item =
                    AppSubscriptionLineItemRecord(
                      ..li,
                      plan: AppSubscriptionLineItemPlan(
                        pricing_details: updated_pricing,
                      ),
                    )
                  let #(_, store_after_li) =
                    store.stage_app_subscription_line_item(
                      draft_store,
                      updated_line_item,
                    )
                  let payload =
                    serializers.project_subscription_payload(
                      store_after_li,
                      Some(sub),
                      Some(confirmation_url(
                        origin,
                        "RecurringApplicationCharge",
                        sub.id,
                      )),
                      [],
                      field,
                      fragments,
                    )
                  let draft =
                    make_log_draft(
                      "appSubscriptionLineItemUpdate",
                      [updated_line_item.id],
                      store_types.Staged,
                    )
                  #(
                    MutationFieldResult(
                      key: key,
                      payload: payload,
                      staged_resource_ids: [updated_line_item.id],
                      log_drafts: [draft],
                    ),
                    store_after_li,
                    identity,
                  )
                }
              }
          }
      }
    }
    _, _ ->
      line_item_update_failed(
        key,
        draft_store,
        identity,
        field,
        fragments,
        app_types.UserError(
          field: ["id"],
          message: "Subscription line item not found",
          code: None,
        ),
      )
  }
}

fn line_item_update_failed(
  key: String,
  draft_store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  user_error: app_types.UserError,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let payload =
    serializers.project_subscription_payload(
      draft_store,
      None,
      None,
      [user_error],
      field,
      fragments,
    )
  let draft =
    make_log_draft("appSubscriptionLineItemUpdate", [], store_types.Failed)
  #(
    MutationFieldResult(
      key: key,
      payload: payload,
      staged_resource_ids: [],
      log_drafts: [draft],
    ),
    draft_store,
    identity,
  )
}

fn valid_app_subscription_line_item_gid(id: String) -> Bool {
  let prefix = "gid://shopify/AppSubscriptionLineItem/"
  case string.starts_with(id, prefix) {
    False -> False
    True -> {
      let tail = string.drop_start(id, string.length(prefix))
      let resource_id = case string.split_once(tail, "?") {
        Ok(#(head, _)) -> head
        Error(_) -> tail
      }
      string.length(resource_id) > 0 && !string.contains(resource_id, "/")
    }
  }
}

fn decimal_amount_greater_than(left: String, right: String) -> Bool {
  case parse_decimal_amount(left), parse_decimal_amount(right) {
    Some(left), Some(right) -> compare_decimal_amounts(left, right) == order.Gt
    _, _ -> False
  }
}

fn parse_decimal_amount(value: String) -> Option(DecimalAmount) {
  let trimmed = string.trim(value)
  let #(sign, unsigned) = case string.starts_with(trimmed, "-") {
    True -> #(-1, string.drop_start(trimmed, 1))
    False ->
      case string.starts_with(trimmed, "+") {
        True -> #(1, string.drop_start(trimmed, 1))
        False -> #(1, trimmed)
      }
  }
  let #(whole_raw, fraction) = case string.split_once(unsigned, ".") {
    Ok(#(whole, fraction)) -> #(whole, fraction)
    Error(_) -> #(unsigned, "")
  }
  let whole = case whole_raw {
    "" -> "0"
    _ -> whole_raw
  }
  case
    string.length(unsigned) > 0
    && all_decimal_digits(whole)
    && all_decimal_digits(fraction)
  {
    True ->
      Some(DecimalAmount(
        sign: sign,
        whole: normalize_decimal_whole(whole),
        fraction: fraction,
      ))
    False -> None
  }
}

fn all_decimal_digits(value: String) -> Bool {
  list.all(string.to_graphemes(value), is_decimal_digit)
}

fn is_decimal_digit(grapheme: String) -> Bool {
  case grapheme {
    "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" -> True
    _ -> False
  }
}

fn normalize_decimal_whole(value: String) -> String {
  do_normalize_decimal_whole(string.to_graphemes(value))
}

fn do_normalize_decimal_whole(graphemes: List(String)) -> String {
  case graphemes {
    ["0", ..rest] -> do_normalize_decimal_whole(rest)
    [] -> "0"
    _ -> string.join(graphemes, "")
  }
}

fn compare_decimal_amounts(
  left: DecimalAmount,
  right: DecimalAmount,
) -> order.Order {
  case int.compare(left.sign, right.sign) {
    order.Eq ->
      case left.sign {
        -1 -> invert_order(compare_unsigned_decimal_amounts(left, right))
        _ -> compare_unsigned_decimal_amounts(left, right)
      }
    other -> other
  }
}

fn compare_unsigned_decimal_amounts(
  left: DecimalAmount,
  right: DecimalAmount,
) -> order.Order {
  case int.compare(string.length(left.whole), string.length(right.whole)) {
    order.Eq -> {
      let scale =
        int.max(string.length(left.fraction), string.length(right.fraction))
      string.compare(
        left.whole <> right_pad_fraction(left.fraction, scale),
        right.whole <> right_pad_fraction(right.fraction, scale),
      )
    }
    other -> other
  }
}

fn right_pad_fraction(value: String, scale: Int) -> String {
  case string.length(value) >= scale {
    True -> value
    False -> right_pad_fraction(value <> "0", scale)
  }
}

fn invert_order(value: order.Order) -> order.Order {
  case value {
    order.Lt -> order.Gt
    order.Eq -> order.Eq
    order.Gt -> order.Lt
  }
}

fn handle_trial_extend(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
  _document: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let subscription_id = graphql_helpers.read_arg_string(args, "id")
  let days = graphql_helpers.read_arg_int(args, "days")
  case validate_trial_extend_days(days) {
    Error(user_error) -> {
      let payload =
        serializers.project_subscription_payload(
          store,
          None,
          None,
          [user_error],
          field,
          fragments,
        )
      let draft =
        make_log_draft("appSubscriptionTrialExtend", [], store_types.Failed)
      #(
        MutationFieldResult(
          key: key,
          payload: payload,
          staged_resource_ids: [],
          log_drafts: [draft],
        ),
        store,
        identity,
      )
    }
    Ok(valid_days) -> {
      let subscription = case subscription_id {
        Some(id) -> store.get_effective_app_subscription_by_id(store, id)
        None -> None
      }
      case subscription {
        None -> {
          let payload =
            serializers.project_subscription_payload(
              store,
              None,
              None,
              [
                app_types.UserError(
                  field: ["id"],
                  message: "The app subscription wasn't found.",
                  code: Some("SUBSCRIPTION_NOT_FOUND"),
                ),
              ],
              field,
              fragments,
            )
          let draft =
            make_log_draft("appSubscriptionTrialExtend", [], store_types.Failed)
          #(
            MutationFieldResult(
              key: key,
              payload: payload,
              staged_resource_ids: [],
              log_drafts: [draft],
            ),
            store,
            identity,
          )
        }
        Some(sub) -> {
          case validate_trial_extend_subscription(sub) {
            Error(user_error) -> {
              let payload =
                serializers.project_subscription_payload(
                  store,
                  None,
                  None,
                  [user_error],
                  field,
                  fragments,
                )
              let draft =
                make_log_draft(
                  "appSubscriptionTrialExtend",
                  [],
                  store_types.Failed,
                )
              #(
                MutationFieldResult(
                  key: key,
                  payload: payload,
                  staged_resource_ids: [],
                  log_drafts: [draft],
                ),
                store,
                identity,
              )
            }
            Ok(Nil) -> {
              let extended_days = option.unwrap(sub.trial_days, 0) + valid_days
              let extended =
                AppSubscriptionRecord(..sub, trial_days: Some(extended_days))
              let #(_, store_after) =
                store.stage_app_subscription(store, extended)
              let payload =
                serializers.project_subscription_payload(
                  store_after,
                  Some(extended),
                  None,
                  [],
                  field,
                  fragments,
                )
              let draft =
                make_log_draft(
                  "appSubscriptionTrialExtend",
                  [extended.id],
                  store_types.Staged,
                )
              #(
                MutationFieldResult(
                  key: key,
                  payload: payload,
                  staged_resource_ids: [extended.id],
                  log_drafts: [draft],
                ),
                store_after,
                identity,
              )
            }
          }
        }
      }
    }
  }
}

fn validate_trial_extend_days(
  days: Option(Int),
) -> Result(Int, app_types.UserError) {
  case days {
    Some(value) if value > 0 && value <= 1000 -> Ok(value)
    Some(value) if value <= 0 ->
      Error(app_types.UserError(
        field: ["days"],
        message: "Days must be greater than 0",
        code: None,
      ))
    _ ->
      Error(app_types.UserError(
        field: ["days"],
        message: "Days must be less than or equal to 1000",
        code: None,
      ))
  }
}

fn validate_trial_extend_subscription(
  subscription: AppSubscriptionRecord,
) -> Result(Nil, app_types.UserError) {
  case subscription.status {
    "ACTIVE" -> {
      case trial_has_expired(subscription) {
        True ->
          Error(app_types.UserError(
            field: ["id"],
            message: "The trial can't be extended after expiration.",
            code: Some("TRIAL_NOT_ACTIVE"),
          ))
        False -> Ok(Nil)
      }
    }
    _ ->
      Error(app_types.UserError(
        field: ["id"],
        message: "The trial can't be extended on inactive app subscriptions.",
        code: Some("SUBSCRIPTION_NOT_ACTIVE"),
      ))
  }
}

fn trial_has_expired(subscription: AppSubscriptionRecord) -> Bool {
  case subscription.current_period_end {
    None -> False
    Some(current_period_end) -> {
      let trial_days = option.unwrap(subscription.trial_days, 0)
      case iso_timestamp.parse_iso(current_period_end) {
        Error(_) -> False
        Ok(period_end_ms) -> {
          case iso_timestamp.parse_iso(iso_timestamp.now_iso()) {
            Error(_) -> False
            Ok(now_ms) -> now_ms > period_end_ms + trial_days * 86_400_000
          }
        }
      }
    }
  }
}

fn handle_usage_record_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
  _document: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let line_item_id =
    graphql_helpers.read_arg_string(args, "subscriptionLineItemId")
  let line_item = case line_item_id {
    Some(id) -> store.get_effective_app_subscription_line_item_by_id(store, id)
    None -> None
  }
  case line_item {
    None -> {
      usage_record_create_failure(key, store, identity, field, fragments, [
        app_types.UserError(
          field: ["subscriptionLineItemId"],
          message: "Subscription line item not found",
          code: None,
        ),
      ])
    }
    Some(li) -> {
      let idempotency_key =
        graphql_helpers.read_arg_string(args, "idempotencyKey")
      case idempotency_key_too_long(idempotency_key) {
        True ->
          usage_record_create_failure(key, store, identity, field, fragments, [
            app_types.UserError(
              field: ["idempotencyKey"],
              message: "Idempotency key must be at most 255 characters",
              code: None,
            ),
          ])
        False ->
          case li.plan.pricing_details {
            AppRecurringPricing(..) ->
              usage_record_create_failure(
                key,
                store,
                identity,
                field,
                fragments,
                [
                  app_types.UserError(
                    field: ["subscriptionLineItemId"],
                    message: "Subscription line item must use usage pricing",
                    code: None,
                  ),
                ],
              )
            AppUsagePricing(
              capped_amount: capped,
              balance_used: balance,
              interval: interval,
              terms: terms,
            ) -> {
              case
                find_usage_record_by_idempotency_key(
                  store,
                  li.id,
                  idempotency_key,
                )
              {
                Some(record) -> {
                  let payload =
                    serializers.project_usage_record_payload(
                      store,
                      Some(record),
                      [],
                      field,
                      fragments,
                    )
                  let draft =
                    make_log_draft(
                      "appUsageRecordCreate",
                      [record.id],
                      store_types.Staged,
                    )
                  #(
                    MutationFieldResult(
                      key: key,
                      payload: payload,
                      staged_resource_ids: [record.id],
                      log_drafts: [draft],
                    ),
                    store,
                    identity,
                  )
                }
                None -> {
                  let price = read_money_input(args, "price")
                  case price.currency_code == capped.currency_code {
                    False ->
                      usage_record_create_failure(
                        key,
                        store,
                        identity,
                        field,
                        fragments,
                        [
                          app_types.UserError(
                            field: ["price", "currencyCode"],
                            message: "Currency code must match capped amount currency",
                            code: None,
                          ),
                        ],
                      )
                    True -> {
                      let proposed_balance = money_add(balance, price)
                      case money_amount_greater_than(proposed_balance, capped) {
                        True ->
                          usage_record_create_failure(
                            key,
                            store,
                            identity,
                            field,
                            fragments,
                            [
                              app_types.UserError(
                                field: [],
                                message: "Total price exceeds balance remaining",
                                code: None,
                              ),
                            ],
                          )
                        False -> {
                          let updated_pricing =
                            AppUsagePricing(
                              capped_amount: capped,
                              balance_used: proposed_balance,
                              interval: interval,
                              terms: terms,
                            )
                          let updated_line_item =
                            AppSubscriptionLineItemRecord(
                              ..li,
                              plan: AppSubscriptionLineItemPlan(
                                pricing_details: updated_pricing,
                              ),
                            )
                          let #(_, store_after_balance) =
                            store.stage_app_subscription_line_item(
                              store,
                              updated_line_item,
                            )
                          let #(record_gid, identity_after_id) =
                            synthetic_identity.make_synthetic_gid(
                              identity,
                              "AppUsageRecord",
                            )
                          let #(timestamp, identity_after_ts) =
                            synthetic_identity.make_synthetic_timestamp(
                              identity_after_id,
                            )
                          let record =
                            AppUsageRecord(
                              id: record_gid,
                              subscription_line_item_id: li.id,
                              description: option.unwrap(
                                graphql_helpers.read_arg_string(
                                  args,
                                  "description",
                                ),
                                "",
                              ),
                              price: price,
                              created_at: timestamp,
                              idempotency_key: idempotency_key,
                            )
                          let #(_, store_after) =
                            store.stage_app_usage_record(
                              store_after_balance,
                              record,
                            )
                          let payload =
                            serializers.project_usage_record_payload(
                              store_after,
                              Some(record),
                              [],
                              field,
                              fragments,
                            )
                          let draft =
                            make_log_draft(
                              "appUsageRecordCreate",
                              [record.id],
                              store_types.Staged,
                            )
                          #(
                            MutationFieldResult(
                              key: key,
                              payload: payload,
                              staged_resource_ids: [record.id],
                              log_drafts: [draft],
                            ),
                            store_after,
                            identity_after_ts,
                          )
                        }
                      }
                    }
                  }
                }
              }
            }
          }
      }
    }
  }
}

fn usage_record_create_failure(
  key: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  user_errors: List(app_types.UserError),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let payload =
    serializers.project_usage_record_payload(
      store,
      None,
      user_errors,
      field,
      fragments,
    )
  let draft = make_log_draft("appUsageRecordCreate", [], store_types.Failed)
  #(
    MutationFieldResult(
      key: key,
      payload: payload,
      staged_resource_ids: [],
      log_drafts: [draft],
    ),
    store,
    identity,
  )
}

fn idempotency_key_too_long(key: Option(String)) -> Bool {
  case key {
    Some(key) -> string.length(key) > 255
    None -> False
  }
}

fn find_usage_record_by_idempotency_key(
  store: Store,
  line_item_id: String,
  key: Option(String),
) -> Option(AppUsageRecord) {
  case key {
    None -> None
    Some(key) ->
      case
        store.list_effective_app_usage_records_for_line_item(
          store,
          line_item_id,
        )
        |> list.find(fn(record) { record.idempotency_key == Some(key) })
      {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

// ---------------------------------------------------------------------------
// Mutation helpers
// ---------------------------------------------------------------------------

fn ensure_current_installation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  origin: String,
) -> #(AppInstallationRecord, Store, SyntheticIdentityRegistry) {
  case store.get_current_app_installation(store) {
    Some(existing) -> #(existing, store, identity)
    None -> {
      let #(app, identity_after_app) = default_app(identity)
      let #(_, store_with_app) = store.stage_app(store, app)
      let #(install_gid, identity_after_install_id) =
        synthetic_identity.make_synthetic_gid(
          identity_after_app,
          "AppInstallation",
        )
      let installation =
        AppInstallationRecord(
          id: install_gid,
          app_id: app.id,
          launch_url: Some(
            origin
            <> "/admin/apps/"
            <> option.unwrap(app.handle, "shopify-draft-proxy"),
          ),
          uninstall_url: None,
          access_scopes: list.append(app.requested_access_scopes, [
            AccessScopeRecord(handle: "write_products", description: None),
          ]),
          active_subscription_ids: [],
          all_subscription_ids: [],
          one_time_purchase_ids: [],
          uninstalled_at: None,
        )
      let #(_, store_with_install) =
        store.stage_app_installation(store_with_app, installation)
      #(installation, store_with_install, identity_after_install_id)
    }
  }
}

fn default_app(
  identity: SyntheticIdentityRegistry,
) -> #(AppRecord, SyntheticIdentityRegistry) {
  let #(app_gid, identity_after) =
    synthetic_identity.make_synthetic_gid(identity, "App")
  let app =
    AppRecord(
      id: app_gid,
      api_key: Some("shopify-draft-proxy-local-app"),
      handle: Some("shopify-draft-proxy"),
      title: Some("shopify-draft-proxy"),
      developer_name: Some("shopify-draft-proxy"),
      embedded: Some(True),
      previously_installed: Some(False),
      requested_access_scopes: [
        AccessScopeRecord(handle: "read_products", description: None),
      ],
    )
  #(app, identity_after)
}

fn confirmation_url(origin: String, kind: String, id: String) -> String {
  origin
  <> "/admin/charges/shopify-draft-proxy/"
  <> trailing_segment(id)
  <> "/"
  <> kind
  <> "/confirm?signature=shopify-draft-proxy-local-redacted"
}

fn token_hash(raw: String) -> String {
  crypto.sha256_hex(raw)
}

fn token_preview(raw: String) -> String {
  case string.length(raw) <= 8 {
    True -> "[redacted]"
    False -> {
      let chars = string.to_graphemes(raw)
      let n = list.length(chars)
      let last_four =
        list.drop(chars, n - 4)
        |> string.join("")
      "[redacted]" <> last_four
    }
  }
}

fn trailing_segment(id: String) -> String {
  case list.last(string.split(id, "/")) {
    Ok(tail) ->
      case string.split_once(tail, "?") {
        Ok(#(head, _)) -> head
        Error(_) -> tail
      }
    Error(_) -> "local"
  }
}

fn read_money_input(
  args: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Money {
  case dict.get(args, name) {
    Ok(root_field.ObjectVal(d)) -> {
      let amount = case dict.get(d, "amount") {
        Ok(root_field.StringVal(s)) -> s
        Ok(root_field.IntVal(n)) -> int.to_string(n)
        Ok(root_field.FloatVal(f)) -> float.to_string(f)
        _ -> "0.0"
      }
      let currency = case dict.get(d, "currencyCode") {
        Ok(root_field.StringVal(s)) ->
          case s {
            "" -> "USD"
            _ -> s
          }
        _ -> "USD"
      }
      Money(amount: amount, currency_code: currency)
    }
    _ -> Money(amount: "0.0", currency_code: "USD")
  }
}

fn purchase_create_validation_errors(
  args: Dict(String, root_field.ResolvedValue),
  name: Option(String),
  price: Money,
  billing_currency: String,
) -> List(app_types.UserError) {
  let name_errors = case name {
    Some(raw) ->
      case string.trim(raw) {
        "" -> blank_purchase_name_error()
        _ -> []
      }
    _ -> [
      app_types.UserError(
        field: ["name"],
        message: "Name can't be blank",
        code: None,
      ),
    ]
  }
  let return_url_errors =
    purchase_return_url_errors(graphql_helpers.read_arg_string(
      args,
      "returnUrl",
    ))
  let price_errors = purchase_price_errors(price, billing_currency)
  list.append(name_errors, return_url_errors) |> list.append(price_errors)
}

fn blank_purchase_name_error() -> List(app_types.UserError) {
  [
    app_types.UserError(
      field: ["name"],
      message: "Name can't be blank",
      code: None,
    ),
  ]
}

fn purchase_return_url_errors(
  return_url: Option(String),
) -> List(app_types.UserError) {
  case return_url {
    Some(raw) -> {
      let trimmed = string.trim(raw)
      case
        trimmed != ""
        && {
          string.starts_with(trimmed, "https://")
          || string.starts_with(trimmed, "http://")
        }
      {
        True -> []
        False -> [
          app_types.UserError(
            field: ["returnUrl"],
            message: "Return URL must be a valid URL.",
            code: None,
          ),
        ]
      }
    }
    None -> [
      app_types.UserError(
        field: ["returnUrl"],
        message: "Return URL is required.",
        code: None,
      ),
    ]
  }
}

fn purchase_price_errors(
  price: Money,
  billing_currency: String,
) -> List(app_types.UserError) {
  let amount = parse_money_amount(price.amount)
  let amount_errors = case
    amount <. app_types.minimum_one_time_purchase_amount
  {
    True -> [
      app_types.UserError(
        field: ["price"],
        message: price_too_low_message(billing_currency),
        code: Some("PRICE_TOO_LOW"),
      ),
    ]
    False -> []
  }
  let currency_errors = case
    normalize_currency(price.currency_code)
    == normalize_currency(billing_currency)
  {
    True -> []
    False -> [
      app_types.UserError(
        field: ["price"],
        message: "Price currency must match shop billing currency "
          <> billing_currency
          <> ".",
        code: None,
      ),
    ]
  }
  list.append(amount_errors, currency_errors)
}

fn price_too_low_message(currency_code: String) -> String {
  "Price must be at least "
  <> app_types.minimum_one_time_purchase_amount_label
  <> " "
  <> currency_code
  <> "."
}

fn parse_money_amount(raw: String) -> Float {
  let trimmed = string.trim(raw)
  case float.parse(trimmed) {
    Ok(value) -> value
    Error(_) ->
      case int.parse(trimmed) {
        Ok(value) -> int.to_float(value)
        Error(_) -> 0.0
      }
  }
}

fn shop_billing_currency(store: Store) -> String {
  case store.get_effective_shop(store) {
    Some(shop) ->
      case string.trim(shop.currency_code) {
        "" -> app_types.default_billing_currency
        code -> normalize_currency(code)
      }
    None -> app_types.default_billing_currency
  }
}

fn normalize_currency(code: String) -> String {
  string.uppercase(string.trim(code))
}

fn money_add(left: Money, right: Money) -> Money {
  let scale = int.max(decimal_scale(left.amount), decimal_scale(right.amount))
  let amount =
    decimal_format(
      decimal_to_scaled(left.amount, scale)
        + decimal_to_scaled(right.amount, scale),
      scale,
    )
  Money(amount: amount, currency_code: left.currency_code)
}

fn money_amount_greater_than(left: Money, right: Money) -> Bool {
  let scale = int.max(decimal_scale(left.amount), decimal_scale(right.amount))
  decimal_to_scaled(left.amount, scale) > decimal_to_scaled(right.amount, scale)
}

fn decimal_scale(amount: String) -> Int {
  case string.split_once(string.trim(amount), ".") {
    Ok(#(_, fractional)) -> string.length(fractional)
    Error(_) -> 0
  }
}

fn decimal_to_scaled(amount: String, scale: Int) -> Int {
  let trimmed = string.trim(amount)
  let #(whole, fractional) = case string.split_once(trimmed, ".") {
    Ok(parts) -> parts
    Error(_) -> #(trimmed, "")
  }
  let whole_value = int.parse(whole) |> result.unwrap(0)
  let fractional_value = case scale {
    0 -> 0
    _ ->
      fractional
      |> string.pad_end(to: scale, with: "0")
      |> string.slice(at_index: 0, length: scale)
      |> int.parse
      |> result.unwrap(0)
  }
  whole_value * decimal_multiplier(scale) + fractional_value
}

fn decimal_format(value: Int, scale: Int) -> String {
  case scale {
    0 -> int.to_string(value)
    _ -> {
      let multiplier = decimal_multiplier(scale)
      let whole = int.divide(value, by: multiplier) |> result.unwrap(0)
      let fractional =
        int.remainder(value, by: multiplier)
        |> result.unwrap(0)
        |> int.absolute_value
      int.to_string(whole)
      <> "."
      <> string.pad_start(int.to_string(fractional), to: scale, with: "0")
    }
  }
}

fn decimal_multiplier(scale: Int) -> Int {
  case scale <= 0 {
    True -> 1
    False -> 10 * decimal_multiplier(scale - 1)
  }
}

fn read_line_item_plan(
  identity: SyntheticIdentityRegistry,
  input: Dict(String, root_field.ResolvedValue),
  subscription_id: String,
  index: Int,
) -> #(AppSubscriptionLineItemRecord, SyntheticIdentityRegistry) {
  let plan = case dict.get(input, "plan") {
    Ok(root_field.ObjectVal(d)) -> d
    _ -> dict.new()
  }
  let recurring = case dict.get(plan, "appRecurringPricingDetails") {
    Ok(root_field.ObjectVal(d)) -> Some(d)
    _ -> None
  }
  let usage = case dict.get(plan, "appUsagePricingDetails") {
    Ok(root_field.ObjectVal(d)) -> Some(d)
    _ -> None
  }
  let pricing = case recurring {
    Some(r) -> {
      let price = read_money_input(r, "price")
      let interval = case dict.get(r, "interval") {
        Ok(root_field.StringVal(s)) -> s
        _ -> "EVERY_30_DAYS"
      }
      let plan_handle = graphql_helpers.read_arg_string(r, "planHandle")
      AppRecurringPricing(
        price: price,
        interval: interval,
        plan_handle: plan_handle,
      )
    }
    None -> {
      let usage_dict = option.unwrap(usage, dict.new())
      let capped = read_money_input(usage_dict, "cappedAmount")
      let interval = case dict.get(usage_dict, "interval") {
        Ok(root_field.StringVal(s)) -> s
        _ -> "EVERY_30_DAYS"
      }
      let terms = graphql_helpers.read_arg_string(usage_dict, "terms")
      AppUsagePricing(
        capped_amount: capped,
        balance_used: Money(amount: "0.0", currency_code: capped.currency_code),
        interval: interval,
        terms: terms,
      )
    }
  }
  let #(base_gid, identity_after) =
    synthetic_identity.make_synthetic_gid(identity, "AppSubscriptionLineItem")
  let id = base_gid <> "?v=1&index=" <> int.to_string(index)
  let _ = subscription_id
  // subscription_id is used by the schema marker; the line item carries
  // it explicitly on the record.
  let record =
    AppSubscriptionLineItemRecord(
      id: id,
      subscription_id: subscription_id,
      plan: AppSubscriptionLineItemPlan(pricing_details: pricing),
    )
  #(record, identity_after)
}

fn compute_current_period_end(
  activated_at: String,
  line_items: List(AppSubscriptionLineItemRecord),
  args: Dict(String, root_field.ResolvedValue),
) -> Option(String) {
  let trial_days =
    graphql_helpers.read_arg_int(args, "trialDays")
    |> option.unwrap(0)
  let interval = subscription_interval(line_items)
  case iso_timestamp.parse_iso(activated_at) {
    Ok(ms) ->
      Some(iso_timestamp.format_iso(
        ms + days_to_ms(interval_days(interval) + trial_days),
      ))
    Error(_) -> None
  }
}

fn subscription_interval(
  line_items: List(AppSubscriptionLineItemRecord),
) -> String {
  case line_items {
    [first, ..] ->
      case first.plan.pricing_details {
        AppRecurringPricing(interval: interval, ..) -> interval
        AppUsagePricing(interval: interval, ..) -> interval
      }
    [] -> "EVERY_30_DAYS"
  }
}

fn interval_days(interval: String) -> Int {
  case interval {
    "ANNUAL" -> 365
    _ -> 30
  }
}

fn days_to_ms(days: Int) -> Int {
  days * 24 * 60 * 60 * 1000
}

fn append_unique(values: List(String), value: String) -> List(String) {
  case list.contains(values, value) {
    True -> values
    False -> list.append(values, [value])
  }
}

fn make_log_draft(
  root_field_name: String,
  staged_ids: List(String),
  status: store_types.EntryStatus,
) -> LogDraft {
  single_root_log_draft(
    root_field_name,
    staged_ids,
    status,
    "apps",
    "stage-locally",
    Some("Locally staged " <> root_field_name <> " in shopify-draft-proxy."),
  )
}

//// Apps billing/access draft runtime.
////
//// Pass 16 lands the six query roots (`app`, `appByHandle`, `appByKey`,
//// `appInstallation`, `appInstallations`, `currentAppInstallation`) plus
//// the per-record source projections needed to serve them.
////
//// Pass 17 lands the ten mutation roots (`appUninstall`,
//// `appRevokeAccessScopes`, `delegateAccessTokenCreate` / `Destroy`,
//// `appPurchaseOneTimeCreate`, `appSubscriptionCreate` /
//// `Cancel` / `LineItemUpdate` / `TrialExtend`,
//// `appUsageRecordCreate`) plus the supporting plumbing
//// (`MutationOutcome`, `process_mutation`, `ensure_current_installation`,
//// `confirmation_url`, `token_hash`, `token_preview`).
////
//// Note: the read path is pure of the store. Mutations thread
//// `(store, identity)` forward and may auto-create a default app
//// installation when one isn't registered yet.

import gleam/dict.{type Dict}
import gleam/float
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import shopify_draft_proxy/crypto
import shopify_draft_proxy/graphql/ast.{type Selection, Field, SelectionSet}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, ConnectionPageInfoOptions,
  ConnectionWindow, SelectedFieldOptions, SerializeConnectionConfig, SrcBool,
  SrcInt, SrcList, SrcNull, SrcString, default_connection_page_info_options,
  default_connection_window_options, get_document_fragments,
  get_field_response_key, paginate_connection_items, project_graphql_value,
  serialize_connection, src_object,
}
import shopify_draft_proxy/proxy/mutation_helpers.{
  type LogDraft, single_root_log_draft,
}
import shopify_draft_proxy/proxy/passthrough
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response, LiveHybrid, Response,
}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type AccessScopeRecord, type AppInstallationRecord,
  type AppOneTimePurchaseRecord, type AppRecord,
  type AppSubscriptionLineItemPlan, type AppSubscriptionLineItemRecord,
  type AppSubscriptionPricing, type AppSubscriptionRecord, type AppUsageRecord,
  type Money, AccessScopeRecord, AppInstallationRecord, AppOneTimePurchaseRecord,
  AppRecord, AppRecurringPricing, AppSubscriptionLineItemPlan,
  AppSubscriptionLineItemRecord, AppSubscriptionRecord, AppUsagePricing,
  AppUsageRecord, DelegatedAccessTokenRecord, Money,
}

// ---------------------------------------------------------------------------
// Public surface
// ---------------------------------------------------------------------------

/// Errors specific to the apps handler. Mirrors `WebhooksError`.
pub type AppsError {
  ParseFailed(root_field.RootFieldError)
}

/// Predicate matching the TS `APP_QUERY_ROOTS` set.
pub fn is_app_query_root(name: String) -> Bool {
  case name {
    "app" -> True
    "appByHandle" -> True
    "appByKey" -> True
    "appInstallation" -> True
    "appInstallations" -> True
    "currentAppInstallation" -> True
    _ -> False
  }
}

/// Predicate matching the TS `APP_MUTATION_ROOTS` set.
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

/// Process an apps query document and return a JSON `data` envelope.
/// Mirrors `handleAppQuery`. The store argument supplies effective
/// (base + staged) records.
pub fn handle_app_query(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, AppsError) {
  case root_field.get_root_fields(document) {
    Error(err) -> Error(ParseFailed(err))
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      Ok(serialize_root_fields(store, fields, fragments, variables))
    }
  }
}

/// Convenience: parse + handle + wrap, for the dispatcher.
pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, AppsError) {
  use data <- result.try(handle_app_query(store, document, variables))
  Ok(graphql_helpers.wrap_data(data))
}

/// True iff the app-domain store has any local app/installation/billing/access
/// records. LiveHybrid app reads pass through while cold, but once mutations
/// stage app state, downstream reads must stay local instead of forwarding
/// synthetic billing/install IDs upstream.
pub fn local_has_app_state(proxy: DraftProxy) -> Bool {
  let base = proxy.store.base_state
  let staged = proxy.store.staged_state
  dict.size(base.apps) > 0
  || dict.size(staged.apps) > 0
  || dict.size(base.app_installations) > 0
  || dict.size(staged.app_installations) > 0
  || has_option(base.current_installation_id)
  || has_option(staged.current_installation_id)
  || dict.size(base.app_subscriptions) > 0
  || dict.size(staged.app_subscriptions) > 0
  || dict.size(base.app_subscription_line_items) > 0
  || dict.size(staged.app_subscription_line_items) > 0
  || dict.size(base.app_one_time_purchases) > 0
  || dict.size(staged.app_one_time_purchases) > 0
  || dict.size(base.app_usage_records) > 0
  || dict.size(staged.app_usage_records) > 0
  || dict.size(base.delegated_access_tokens) > 0
  || dict.size(staged.delegated_access_tokens) > 0
}

fn has_option(value: Option(a)) -> Bool {
  case value {
    Some(_) -> True
    None -> False
  }
}

/// Pattern 1: app reads are transparent LiveHybrid passthroughs until
/// local app-domain state exists. After app billing/access mutations stage
/// state, the same roots must resolve locally so read-after-write and
/// read-after-uninstall behavior never consult upstream.
fn should_passthrough_in_live_hybrid(
  proxy: DraftProxy,
  type_: parse_operation.GraphQLOperationType,
  primary_root_field: String,
) -> Bool {
  case type_, primary_root_field {
    parse_operation.QueryOperation, "currentAppInstallation" ->
      !local_has_app_state(proxy)
    _, _ -> False
  }
}

/// Domain entrypoint for app queries. The registry now lets implemented app
/// reads reach this handler; LiveHybrid passthrough remains a domain decision
/// so staged billing/access scenarios stay local-only after their first write.
pub fn handle_query_request(
  proxy: DraftProxy,
  request: Request,
  parsed: parse_operation.ParsedOperation,
  primary_root_field: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Response, DraftProxy) {
  let want_passthrough = case proxy.config.read_mode {
    LiveHybrid ->
      should_passthrough_in_live_hybrid(proxy, parsed.type_, primary_root_field)
    _ -> False
  }
  case want_passthrough {
    True -> passthrough.passthrough_sync(proxy, request)
    False ->
      case process(proxy.store, document, variables) {
        Ok(envelope) -> #(
          Response(status: 200, body: envelope, headers: []),
          proxy,
        )
        Error(_) -> #(
          Response(
            status: 400,
            body: json.object([
              #(
                "errors",
                json.array(
                  [
                    json.object([
                      #("message", json.string("Failed to handle apps query")),
                    ]),
                  ],
                  fn(x) { x },
                ),
              ),
            ]),
            headers: [],
          ),
          proxy,
        )
      }
  }
}

// ---------------------------------------------------------------------------
// Root-field dispatch
// ---------------------------------------------------------------------------

fn serialize_root_fields(
  store: Store,
  fields: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let entries =
    list.map(fields, fn(field) {
      let key = get_field_response_key(field)
      let value = root_payload_for_field(store, field, fragments, variables)
      #(key, value)
    })
  json.object(entries)
}

fn root_payload_for_field(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  case field {
    Field(name: name, ..) ->
      case name.value {
        "currentAppInstallation" ->
          serialize_current_app_installation(store, field, fragments)
        "appInstallation" ->
          serialize_app_installation_by_id(store, field, fragments, variables)
        "app" -> serialize_app_by_id(store, field, fragments, variables)
        "appByHandle" ->
          serialize_app_by_handle(store, field, fragments, variables)
        "appByKey" -> serialize_app_by_key(store, field, fragments, variables)
        "appInstallations" ->
          serialize_app_installations_connection(store, field, fragments)
        _ -> json.null()
      }
    _ -> json.null()
  }
}

// ---------------------------------------------------------------------------
// Per-root serializers
// ---------------------------------------------------------------------------

fn serialize_current_app_installation(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  case store.get_current_app_installation(store) {
    Some(installation) ->
      project_app_installation(store, installation, field, fragments)
    None -> json.null()
  }
}

fn serialize_app_installation_by_id(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string(args, "id") {
    Some(id) ->
      case store.get_effective_app_installation_by_id(store, id) {
        Some(installation) ->
          project_app_installation(store, installation, field, fragments)
        None -> json.null()
      }
    None -> json.null()
  }
}

fn serialize_app_by_id(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string(args, "id") {
    Some(id) ->
      case store.get_effective_app_by_id(store, id) {
        Some(app) -> project_app(app, field, fragments)
        None -> json.null()
      }
    None -> json.null()
  }
}

fn serialize_app_by_handle(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string(args, "handle") {
    Some(handle) ->
      case store.find_effective_app_by_handle(store, handle) {
        Some(app) -> project_app(app, field, fragments)
        None -> json.null()
      }
    None -> json.null()
  }
}

fn serialize_app_by_key(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string(args, "apiKey") {
    Some(api_key) ->
      case store.find_effective_app_by_api_key(store, api_key) {
        Some(app) -> project_app(app, field, fragments)
        None -> json.null()
      }
    None -> json.null()
  }
}

fn serialize_app_installations_connection(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let installations = case store.get_current_app_installation(store) {
    Some(current) -> [current]
    None -> []
  }
  let window =
    paginate_connection_items(
      installations,
      field,
      dict.new(),
      installation_cursor_value,
      default_connection_window_options(),
    )
  let ConnectionWindow(
    items: items,
    has_next_page: has_next,
    has_previous_page: has_prev,
  ) = window
  let selected_field_options =
    SelectedFieldOptions(include_inline_fragments: True)
  let page_info_options =
    ConnectionPageInfoOptions(
      ..default_connection_page_info_options(),
      include_inline_fragments: True,
    )
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: items,
      has_next_page: has_next,
      has_previous_page: has_prev,
      get_cursor_value: installation_cursor_value,
      serialize_node: fn(installation, node_field, _index) {
        project_app_installation(store, installation, node_field, fragments)
      },
      selected_field_options: selected_field_options,
      page_info_options: page_info_options,
    ),
  )
}

fn installation_cursor_value(
  record: AppInstallationRecord,
  _index: Int,
) -> String {
  record.id
}

pub fn serialize_app_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  case store.get_effective_app_by_id(store, id) {
    Some(app) ->
      project_graphql_value(app_to_source(app), selections, fragments)
    None -> json.null()
  }
}

pub fn serialize_app_installation_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  case store.get_effective_app_installation_by_id(store, id) {
    Some(installation) ->
      project_graphql_value(
        app_installation_to_source(store, installation, fragments),
        selections,
        fragments,
      )
    None -> json.null()
  }
}

pub fn serialize_app_subscription_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  case store.get_effective_app_subscription_by_id(store, id) {
    Some(subscription) ->
      project_graphql_value(
        subscription_to_source(store, subscription, fragments),
        selections,
        fragments,
      )
    None -> json.null()
  }
}

pub fn serialize_app_one_time_purchase_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  case store.get_effective_app_one_time_purchase_by_id(store, id) {
    Some(purchase) ->
      project_graphql_value(
        one_time_purchase_to_source(purchase),
        selections,
        fragments,
      )
    None -> json.null()
  }
}

pub fn serialize_app_usage_record_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  case store.get_effective_app_usage_record_by_id(store, id) {
    Some(record) ->
      project_graphql_value(
        usage_record_to_source(store, record),
        selections,
        fragments,
      )
    None -> json.null()
  }
}

// ---------------------------------------------------------------------------
// Source projections — record → SourceValue → Json
// ---------------------------------------------------------------------------

fn project_app(
  app: AppRecord,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let source = app_to_source(app)
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      project_graphql_value(source, selections, fragments)
    _ -> json.object([])
  }
}

fn app_to_source(app: AppRecord) -> graphql_helpers.SourceValue {
  src_object([
    #("__typename", SrcString("App")),
    #("id", SrcString(app.id)),
    #("apiKey", graphql_helpers.option_string_source(app.api_key)),
    #("handle", graphql_helpers.option_string_source(app.handle)),
    #("title", graphql_helpers.option_string_source(app.title)),
    #("developerName", graphql_helpers.option_string_source(app.developer_name)),
    #("embedded", graphql_helpers.option_bool_source(app.embedded)),
    #(
      "previouslyInstalled",
      graphql_helpers.option_bool_source(app.previously_installed),
    ),
    #(
      "requestedAccessScopes",
      SrcList(list.map(app.requested_access_scopes, access_scope_to_source)),
    ),
  ])
}

fn access_scope_to_source(
  scope: AccessScopeRecord,
) -> graphql_helpers.SourceValue {
  src_object([
    #("__typename", SrcString("AccessScope")),
    #("handle", SrcString(scope.handle)),
    #("description", graphql_helpers.option_string_source(scope.description)),
  ])
}

fn project_app_installation(
  store: Store,
  installation: AppInstallationRecord,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let selections = case field {
    Field(selection_set: Some(SelectionSet(selections: ss, ..)), ..) -> ss
    _ -> []
  }
  let source = app_installation_to_source(store, installation, fragments)
  project_graphql_value(source, selections, fragments)
}

fn app_installation_to_source(
  store: Store,
  installation: AppInstallationRecord,
  fragments: FragmentMap,
) -> graphql_helpers.SourceValue {
  let app_source = case
    store.get_effective_app_by_id(store, installation.app_id)
  {
    Some(app) -> app_to_source(app)
    None -> SrcNull
  }
  let active_subscriptions =
    installation.active_subscription_ids
    |> list.filter_map(fn(id) {
      case store.get_effective_app_subscription_by_id(store, id) {
        Some(s) -> Ok(s)
        None -> Error(Nil)
      }
    })
    |> list.filter(fn(s) { s.status == "ACTIVE" })

  let all_subscriptions =
    installation.all_subscription_ids
    |> list.filter_map(fn(id) {
      case store.get_effective_app_subscription_by_id(store, id) {
        Some(s) -> Ok(s)
        None -> Error(Nil)
      }
    })

  let one_time_purchases =
    installation.one_time_purchase_ids
    |> list.filter_map(fn(id) {
      case store.get_effective_app_one_time_purchase_by_id(store, id) {
        Some(p) -> Ok(p)
        None -> Error(Nil)
      }
    })

  src_object([
    #("__typename", SrcString("AppInstallation")),
    #("id", SrcString(installation.id)),
    #("app", app_source),
    #(
      "launchUrl",
      graphql_helpers.option_string_source(installation.launch_url),
    ),
    #(
      "uninstallUrl",
      graphql_helpers.option_string_source(installation.uninstall_url),
    ),
    #(
      "accessScopes",
      SrcList(list.map(installation.access_scopes, access_scope_to_source)),
    ),
    #(
      "activeSubscriptions",
      SrcList(
        list.map(active_subscriptions, fn(s) {
          subscription_to_source(store, s, fragments)
        }),
      ),
    ),
    #(
      "allSubscriptions",
      subscription_connection_source(store, all_subscriptions, fragments),
    ),
    #(
      "oneTimePurchases",
      one_time_purchase_connection_source(one_time_purchases),
    ),
    #(
      "uninstalledAt",
      graphql_helpers.option_string_source(installation.uninstalled_at),
    ),
  ])
}

fn subscription_to_source(
  store: Store,
  subscription: AppSubscriptionRecord,
  fragments: FragmentMap,
) -> graphql_helpers.SourceValue {
  let line_items =
    subscription.line_item_ids
    |> list.filter_map(fn(id) {
      case store.get_effective_app_subscription_line_item_by_id(store, id) {
        Some(li) -> Ok(li)
        None -> Error(Nil)
      }
    })

  src_object([
    #("__typename", SrcString("AppSubscription")),
    #("id", SrcString(subscription.id)),
    #("name", SrcString(subscription.name)),
    #("status", SrcString(subscription.status)),
    #("test", SrcBool(subscription.is_test)),
    #("trialDays", graphql_helpers.option_int_source(subscription.trial_days)),
    #(
      "currentPeriodEnd",
      graphql_helpers.option_string_source(subscription.current_period_end),
    ),
    #("createdAt", SrcString(subscription.created_at)),
    #(
      "lineItems",
      SrcList(
        list.map(line_items, fn(li) {
          line_item_to_source(store, li, fragments)
        }),
      ),
    ),
  ])
}

fn line_item_to_source(
  store: Store,
  line_item: AppSubscriptionLineItemRecord,
  _fragments: FragmentMap,
) -> graphql_helpers.SourceValue {
  let usage_records =
    store.list_effective_app_usage_records_for_line_item(store, line_item.id)
  src_object([
    #("__typename", SrcString("AppSubscriptionLineItem")),
    #("id", SrcString(line_item.id)),
    #("plan", line_item_plan_to_source(line_item.plan)),
    #("usageRecords", usage_record_connection_source(store, usage_records)),
  ])
}

fn line_item_plan_to_source(
  plan: AppSubscriptionLineItemPlan,
) -> graphql_helpers.SourceValue {
  src_object([
    #("__typename", SrcString("AppPlan")),
    #("pricingDetails", pricing_to_source(plan.pricing_details)),
  ])
}

fn pricing_to_source(
  pricing: AppSubscriptionPricing,
) -> graphql_helpers.SourceValue {
  case pricing {
    AppRecurringPricing(price: price, interval: interval, plan_handle: handle) ->
      src_object([
        #("__typename", SrcString("AppRecurringPricing")),
        #("price", money_to_source(price)),
        #("interval", SrcString(interval)),
        #("planHandle", graphql_helpers.option_string_source(handle)),
      ])
    AppUsagePricing(
      capped_amount: capped,
      balance_used: balance,
      interval: interval,
      terms: terms,
    ) ->
      src_object([
        #("__typename", SrcString("AppUsagePricing")),
        #("cappedAmount", money_to_source(capped)),
        #("balanceUsed", money_to_source(balance)),
        #("interval", SrcString(interval)),
        #("terms", graphql_helpers.option_string_source(terms)),
      ])
  }
}

fn money_to_source(money: Money) -> graphql_helpers.SourceValue {
  src_object([
    #("__typename", SrcString("MoneyV2")),
    #("amount", SrcString(money.amount)),
    #("currencyCode", SrcString(money.currency_code)),
  ])
}

fn usage_record_to_source(
  store: Store,
  record: AppUsageRecord,
) -> graphql_helpers.SourceValue {
  let subscription_line_item_source = case
    store.get_effective_app_subscription_line_item_by_id(
      store,
      record.subscription_line_item_id,
    )
  {
    Some(li) ->
      src_object([
        #("__typename", SrcString("AppSubscriptionLineItem")),
        #("id", SrcString(li.id)),
        #("plan", line_item_plan_to_source(li.plan)),
      ])
    None -> SrcNull
  }
  src_object([
    #("__typename", SrcString("AppUsageRecord")),
    #("id", SrcString(record.id)),
    #("description", SrcString(record.description)),
    #("price", money_to_source(record.price)),
    #("createdAt", SrcString(record.created_at)),
    #(
      "idempotencyKey",
      graphql_helpers.option_string_source(record.idempotency_key),
    ),
    #("subscriptionLineItem", subscription_line_item_source),
  ])
}

fn one_time_purchase_to_source(
  purchase: AppOneTimePurchaseRecord,
) -> graphql_helpers.SourceValue {
  src_object([
    #("__typename", SrcString("AppPurchaseOneTime")),
    #("id", SrcString(purchase.id)),
    #("name", SrcString(purchase.name)),
    #("status", SrcString(purchase.status)),
    #("test", SrcBool(purchase.is_test)),
    #("createdAt", SrcString(purchase.created_at)),
    #("price", money_to_source(purchase.price)),
  ])
}

// ---------------------------------------------------------------------------
// Connection sources for the child connections on AppInstallation /
// AppSubscriptionLineItem.
//
// These build a SourceValue connection — `{ edges, nodes, pageInfo }` —
// rather than calling `serialize_connection` directly, because the
// outer `project_graphql_value` walk owns the field selection. That
// matches the way connections are projected through the source pipe in
// other domains.
// ---------------------------------------------------------------------------

fn subscription_connection_source(
  store: Store,
  subscriptions: List(AppSubscriptionRecord),
  fragments: FragmentMap,
) -> graphql_helpers.SourceValue {
  let nodes =
    SrcList(
      list.map(subscriptions, fn(s) {
        subscription_to_source(store, s, fragments)
      }),
    )
  let edges =
    SrcList(
      list.map(subscriptions, fn(s) {
        src_object([
          #("__typename", SrcString("AppSubscriptionEdge")),
          #("cursor", SrcString(s.id)),
          #("node", subscription_to_source(store, s, fragments)),
        ])
      }),
    )
  src_object([
    #("__typename", SrcString("AppSubscriptionConnection")),
    #("edges", edges),
    #("nodes", nodes),
    #("pageInfo", page_info_source(subscriptions, fn(s) { s.id })),
    #("totalCount", SrcInt(list.length(subscriptions))),
  ])
}

fn one_time_purchase_connection_source(
  purchases: List(AppOneTimePurchaseRecord),
) -> graphql_helpers.SourceValue {
  let nodes = SrcList(list.map(purchases, one_time_purchase_to_source))
  let edges =
    SrcList(
      list.map(purchases, fn(p) {
        src_object([
          #("__typename", SrcString("AppPurchaseOneTimeEdge")),
          #("cursor", SrcString(p.id)),
          #("node", one_time_purchase_to_source(p)),
        ])
      }),
    )
  src_object([
    #("__typename", SrcString("AppPurchaseOneTimeConnection")),
    #("edges", edges),
    #("nodes", nodes),
    #("pageInfo", page_info_source(purchases, fn(p) { p.id })),
    #("totalCount", SrcInt(list.length(purchases))),
  ])
}

fn usage_record_connection_source(
  store: Store,
  records: List(AppUsageRecord),
) -> graphql_helpers.SourceValue {
  let nodes =
    SrcList(list.map(records, fn(r) { usage_record_to_source(store, r) }))
  let edges =
    SrcList(
      list.map(records, fn(r) {
        src_object([
          #("__typename", SrcString("AppUsageRecordEdge")),
          #("cursor", SrcString(r.id)),
          #("node", usage_record_to_source(store, r)),
        ])
      }),
    )
  src_object([
    #("__typename", SrcString("AppUsageRecordConnection")),
    #("edges", edges),
    #("nodes", nodes),
    #("pageInfo", page_info_source(records, fn(r) { r.id })),
    #("totalCount", SrcInt(list.length(records))),
  ])
}

fn page_info_source(
  items: List(a),
  cursor: fn(a) -> String,
) -> graphql_helpers.SourceValue {
  let start_cursor = case items {
    [first, ..] -> SrcString(cursor(first))
    [] -> SrcNull
  }
  let end_cursor = case list.reverse(items) {
    [last, ..] -> SrcString(cursor(last))
    [] -> SrcNull
  }
  src_object([
    #("__typename", SrcString("PageInfo")),
    #("hasNextPage", SrcBool(False)),
    #("hasPreviousPage", SrcBool(False)),
    #("startCursor", start_cursor),
    #("endCursor", end_cursor),
  ])
}

// ===========================================================================
// Mutation path
// ===========================================================================

/// Outcome of an apps mutation. Mirrors the saved-search/webhook-subscription
/// outcome shape: a JSON envelope (`{"data": ...}` or `{"errors": ...}`),
/// the updated store and identity registry, and the staged GIDs.
pub type MutationOutcome {
  MutationOutcome(
    data: Json,
    store: Store,
    identity: SyntheticIdentityRegistry,
    staged_resource_ids: List(String),
    log_drafts: List(LogDraft),
  )
}

/// User-error payload emitted on a mutation failure. Mirrors the apps
/// `UserError` shape in TS: an optional `code` and a path that defaults
/// to an empty list.
pub type UserError {
  UserError(field: List(String), message: String, code: Option(String))
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
pub fn process_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  origin: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(MutationOutcome, AppsError) {
  case root_field.get_root_fields(document) {
    Error(err) -> Error(ParseFailed(err))
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      Ok(handle_mutation_fields(
        store,
        identity,
        request_path,
        origin,
        document,
        fields,
        fragments,
        variables,
      ))
    }
  }
}

fn handle_mutation_fields(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  origin: String,
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
  origin: String,
  _document: String,
  field: Selection,
  fragments: FragmentMap,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let #(installation, store_after_ensure, identity_after_ensure) =
    ensure_current_installation(store, identity, origin)
  let #(timestamp, identity_after_ts) =
    synthetic_identity.make_synthetic_timestamp(identity_after_ensure)
  let app =
    store.get_effective_app_by_id(store_after_ensure, installation.app_id)
  let updated =
    AppInstallationRecord(..installation, uninstalled_at: Some(timestamp))
  let #(_, store_staged) =
    store.stage_app_installation(store_after_ensure, updated)
  let payload = project_uninstall_payload(app, [], field, fragments)
  let draft = make_log_draft("appUninstall", [installation.id], store.Staged)
  #(
    MutationFieldResult(
      key: key,
      payload: payload,
      staged_resource_ids: [installation.id],
      log_drafts: [draft],
    ),
    store_staged,
    identity_after_ts,
  )
}

fn handle_revoke_access_scopes(
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
  let #(installation, store_after_ensure, identity_after_ensure) =
    ensure_current_installation(store, identity, origin)
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
  let current_handles = list.map(installation.access_scopes, fn(s) { s.handle })
  let revoked =
    list.filter(installation.access_scopes, fn(scope) {
      list.contains(requested_scopes, scope.handle)
    })
  let errors =
    list.filter(requested_scopes, fn(scope) {
      !list.contains(current_handles, scope)
    })
    |> list.map(fn(scope) {
      UserError(
        field: ["scopes"],
        message: "Access scope '" <> scope <> "' is not granted.",
        code: Some("UNKNOWN_SCOPES"),
      )
    })
  let updated =
    AppInstallationRecord(
      ..installation,
      access_scopes: list.filter(installation.access_scopes, fn(scope) {
        !list.contains(requested_scopes, scope.handle)
      }),
    )
  let #(_, store_staged) =
    store.stage_app_installation(store_after_ensure, updated)
  let payload = project_revoke_payload(revoked, errors, field, fragments)
  let status = case errors {
    [] -> store.Staged
    _ -> store.Failed
  }
  let draft = make_log_draft("appRevokeAccessScopes", [installation.id], status)
  #(
    MutationFieldResult(
      key: key,
      payload: payload,
      staged_resource_ids: [installation.id],
      log_drafts: [draft],
    ),
    store_staged,
    identity_after_ensure,
  )
}

fn handle_delegate_create(
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
  let input = case dict.get(args, "input") {
    Ok(root_field.ObjectVal(d)) -> d
    _ -> dict.new()
  }
  let delegate_scope =
    graphql_helpers.read_arg_string(input, "delegateAccessScope")
  let legacy_scopes = case dict.get(input, "accessScopes") {
    Ok(root_field.ListVal(items)) ->
      list.filter_map(items, fn(item) {
        case item {
          root_field.StringVal(s) -> Ok(s)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
  let access_scopes = case delegate_scope {
    Some(s) -> [s]
    None -> legacy_scopes
  }
  let expires_in = case dict.get(input, "expiresIn") {
    Ok(root_field.IntVal(n)) -> Some(n)
    _ -> None
  }
  let #(token_gid, identity_after_id) =
    synthetic_identity.make_synthetic_gid(identity, "DelegateAccessToken")
  let #(timestamp, identity_after_ts) =
    synthetic_identity.make_synthetic_timestamp(identity_after_id)
  let raw_token = "shpat_delegate_proxy_" <> trailing_segment(token_gid)
  let record =
    DelegatedAccessTokenRecord(
      id: token_gid,
      access_token_sha256: token_hash(raw_token),
      access_token_preview: token_preview(raw_token),
      access_scopes: access_scopes,
      created_at: timestamp,
      expires_in: expires_in,
      destroyed_at: None,
    )
  let #(_, store_staged) = store.stage_delegated_access_token(store, record)
  let payload =
    project_delegate_create_payload(
      raw_token,
      access_scopes,
      timestamp,
      expires_in,
      [],
      field,
      fragments,
    )
  let draft =
    make_log_draft("delegateAccessTokenCreate", [token_gid], store.Staged)
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
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let access_token = graphql_helpers.read_arg_string(args, "accessToken")
  let token = case access_token {
    Some(raw) ->
      store.find_delegated_access_token_by_hash(store, token_hash(raw))
    None -> None
  }
  case token {
    None -> {
      let payload =
        project_delegate_destroy_payload(
          False,
          [
            UserError(
              field: ["accessToken"],
              message: "Access token not found.",
              code: Some("ACCESS_TOKEN_NOT_FOUND"),
            ),
          ],
          field,
          fragments,
        )
      let draft = make_log_draft("delegateAccessTokenDestroy", [], store.Failed)
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
      let #(timestamp, identity_after_ts) =
        synthetic_identity.make_synthetic_timestamp(identity)
      let store_after =
        store.destroy_delegated_access_token(store, record.id, timestamp)
      let payload = project_delegate_destroy_payload(True, [], field, fragments)
      let draft =
        make_log_draft("delegateAccessTokenDestroy", [record.id], store.Staged)
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
  let #(installation, store_after_ensure, identity_after_ensure) =
    ensure_current_installation(store, identity, origin)
  let #(purchase_gid, identity_after_id) =
    synthetic_identity.make_synthetic_gid(
      identity_after_ensure,
      "AppPurchaseOneTime",
    )
  let #(timestamp, identity_after_ts) =
    synthetic_identity.make_synthetic_timestamp(identity_after_id)
  let purchase =
    AppOneTimePurchaseRecord(
      id: purchase_gid,
      name: option.unwrap(graphql_helpers.read_arg_string(args, "name"), ""),
      status: "PENDING",
      is_test: graphql_helpers.read_arg_bool(args, "test")
        |> option.unwrap(False),
      created_at: timestamp,
      price: read_money_input(args, "price"),
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
    project_purchase_create_payload(
      Some(purchase),
      Some(confirmation_url(origin, "ApplicationCharge", purchase.id)),
      [],
      field,
      fragments,
    )
  let draft =
    make_log_draft("appPurchaseOneTimeCreate", [purchase.id], store.Staged)
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
  let subscription =
    AppSubscriptionRecord(
      id: sub_gid,
      name: option.unwrap(graphql_helpers.read_arg_string(args, "name"), ""),
      status: "PENDING",
      is_test: graphql_helpers.read_arg_bool(args, "test")
        |> option.unwrap(False),
      trial_days: graphql_helpers.read_arg_int(args, "trialDays"),
      current_period_end: None,
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
    )
  let #(_, store_staged) =
    store.stage_app_installation(store_after_sub, updated_installation)
  let payload =
    project_subscription_create_payload(
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
  let draft = make_log_draft("appSubscriptionCreate", staged_ids, store.Staged)
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
        project_subscription_payload(
          store,
          None,
          None,
          [
            UserError(
              field: ["id"],
              message: subscription_record_not_found_message(),
              code: None,
            ),
          ],
          field,
          fragments,
        )
      let draft = make_log_draft("appSubscriptionCancel", [], store.Failed)
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
            project_subscription_payload(
              store,
              None,
              None,
              [
                UserError(
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
          let draft = make_log_draft("appSubscriptionCancel", [], store.Failed)
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
            project_subscription_payload(
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
              store.Staged,
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
  let line_item = case line_item_id {
    Some(id) -> store.get_effective_app_subscription_line_item_by_id(store, id)
    None -> None
  }
  let subscription = case line_item {
    Some(li) ->
      store.get_effective_app_subscription_by_id(store, li.subscription_id)
    None -> None
  }
  case line_item, subscription {
    Some(li), Some(sub) -> {
      let capped = read_money_input(args, "cappedAmount")
      let updated_pricing = case li.plan.pricing_details {
        AppRecurringPricing(..) ->
          // TS allows updating cappedAmount on a recurring line item by
          // shallow-merging onto the existing pricing details. The Gleam
          // model has no field for that on AppRecurringPricing, so we
          // fall through and leave it unchanged. (Realistic shape is
          // AppUsagePricing — that's what the TS shop emits.)
          li.plan.pricing_details
        AppUsagePricing(
          balance_used: balance,
          interval: interval,
          terms: terms,
          ..,
        ) ->
          AppUsagePricing(
            capped_amount: capped,
            balance_used: balance,
            interval: interval,
            terms: terms,
          )
      }
      let updated_line_item =
        AppSubscriptionLineItemRecord(
          ..li,
          plan: AppSubscriptionLineItemPlan(pricing_details: updated_pricing),
        )
      let #(_, store_after_li) =
        store.stage_app_subscription_line_item(store, updated_line_item)
      let payload =
        project_subscription_payload(
          store_after_li,
          Some(sub),
          Some(confirmation_url(origin, "RecurringApplicationCharge", sub.id)),
          [],
          field,
          fragments,
        )
      let draft =
        make_log_draft(
          "appSubscriptionLineItemUpdate",
          [updated_line_item.id],
          store.Staged,
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
    _, _ -> {
      let payload =
        project_subscription_payload(
          store,
          None,
          None,
          [
            UserError(
              field: ["id"],
              message: "Subscription line item not found",
              code: None,
            ),
          ],
          field,
          fragments,
        )
      let draft =
        make_log_draft("appSubscriptionLineItemUpdate", [], store.Failed)
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
  let days = option.unwrap(graphql_helpers.read_arg_int(args, "days"), 0)
  let subscription = case subscription_id {
    Some(id) -> store.get_effective_app_subscription_by_id(store, id)
    None -> None
  }
  case subscription {
    None -> {
      let payload =
        project_subscription_payload(
          store,
          None,
          None,
          [
            UserError(
              field: ["id"],
              message: "Subscription not found",
              code: None,
            ),
          ],
          field,
          fragments,
        )
      let draft = make_log_draft("appSubscriptionTrialExtend", [], store.Failed)
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
      let extended_days = option.unwrap(sub.trial_days, 0) + days
      let extended =
        AppSubscriptionRecord(..sub, trial_days: Some(extended_days))
      let #(_, store_after) = store.stage_app_subscription(store, extended)
      let payload =
        project_subscription_payload(
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
          store.Staged,
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
      let payload =
        project_usage_record_payload(
          store,
          None,
          [
            UserError(
              field: ["subscriptionLineItemId"],
              message: "Subscription line item not found",
              code: None,
            ),
          ],
          field,
          fragments,
        )
      let draft = make_log_draft("appUsageRecordCreate", [], store.Failed)
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
    Some(li) -> {
      let #(record_gid, identity_after_id) =
        synthetic_identity.make_synthetic_gid(identity, "AppUsageRecord")
      let #(timestamp, identity_after_ts) =
        synthetic_identity.make_synthetic_timestamp(identity_after_id)
      let record =
        AppUsageRecord(
          id: record_gid,
          subscription_line_item_id: li.id,
          description: option.unwrap(
            graphql_helpers.read_arg_string(args, "description"),
            "",
          ),
          price: read_money_input(args, "price"),
          created_at: timestamp,
          idempotency_key: graphql_helpers.read_arg_string(
            args,
            "idempotencyKey",
          ),
        )
      let #(_, store_after) = store.stage_app_usage_record(store, record)
      let payload =
        project_usage_record_payload(
          store_after,
          Some(record),
          [],
          field,
          fragments,
        )
      let draft =
        make_log_draft("appUsageRecordCreate", [record.id], store.Staged)
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
          access_scopes: app.requested_access_scopes,
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
        AccessScopeRecord(handle: "write_products", description: None),
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

fn make_log_draft(
  root_field_name: String,
  staged_ids: List(String),
  status: store.EntryStatus,
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

// ---------------------------------------------------------------------------
// Mutation projections
// ---------------------------------------------------------------------------

fn project_uninstall_payload(
  app: Option(AppRecord),
  user_errors: List(UserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let app_source = case app {
    Some(a) -> app_to_source(a)
    None -> SrcNull
  }
  let payload =
    src_object([
      #("app", app_source),
      #("userErrors", user_errors_source(user_errors)),
    ])
  project_payload(payload, field, fragments)
}

fn project_revoke_payload(
  revoked: List(AccessScopeRecord),
  user_errors: List(UserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let payload =
    src_object([
      #("revoked", SrcList(list.map(revoked, access_scope_to_source))),
      #("userErrors", user_errors_source(user_errors)),
    ])
  project_payload(payload, field, fragments)
}

fn project_delegate_create_payload(
  raw_token: String,
  access_scopes: List(String),
  created_at: String,
  expires_in: Option(Int),
  user_errors: List(UserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let token_source =
    src_object([
      #("__typename", SrcString("DelegateAccessToken")),
      #("accessToken", SrcString(raw_token)),
      #(
        "accessScopes",
        SrcList(list.map(access_scopes, fn(s) { SrcString(s) })),
      ),
      #("createdAt", SrcString(created_at)),
      #("expiresIn", graphql_helpers.option_int_source(expires_in)),
    ])
  let payload =
    src_object([
      #("delegateAccessToken", token_source),
      #("shop", SrcNull),
      #("userErrors", user_errors_source(user_errors)),
    ])
  project_payload(payload, field, fragments)
}

fn project_delegate_destroy_payload(
  status: Bool,
  user_errors: List(UserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let payload =
    src_object([
      #("status", SrcBool(status)),
      #("shop", SrcNull),
      #("userErrors", user_errors_source(user_errors)),
    ])
  project_payload(payload, field, fragments)
}

fn project_purchase_create_payload(
  purchase: Option(AppOneTimePurchaseRecord),
  confirmation: Option(String),
  user_errors: List(UserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let purchase_source = case purchase {
    Some(p) -> one_time_purchase_to_source(p)
    None -> SrcNull
  }
  let payload =
    src_object([
      #("appPurchaseOneTime", purchase_source),
      #("confirmationUrl", graphql_helpers.option_string_source(confirmation)),
      #("userErrors", user_errors_source(user_errors)),
    ])
  project_payload(payload, field, fragments)
}

fn project_subscription_create_payload(
  store: Store,
  subscription: Option(AppSubscriptionRecord),
  confirmation: Option(String),
  user_errors: List(UserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let sub_source = case subscription {
    Some(s) -> subscription_to_source(store, s, fragments)
    None -> SrcNull
  }
  let payload =
    src_object([
      #("appSubscription", sub_source),
      #("confirmationUrl", graphql_helpers.option_string_source(confirmation)),
      #("userErrors", user_errors_source(user_errors)),
    ])
  project_payload(payload, field, fragments)
}

fn project_subscription_payload(
  store: Store,
  subscription: Option(AppSubscriptionRecord),
  confirmation: Option(String),
  user_errors: List(UserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_subscription_create_payload(
    store,
    subscription,
    confirmation,
    user_errors,
    field,
    fragments,
  )
}

fn project_usage_record_payload(
  store: Store,
  record: Option(AppUsageRecord),
  user_errors: List(UserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let record_source = case record {
    Some(r) -> usage_record_to_source(store, r)
    None -> SrcNull
  }
  let payload =
    src_object([
      #("appUsageRecord", record_source),
      #("userErrors", user_errors_source(user_errors)),
    ])
  project_payload(payload, field, fragments)
}

fn project_payload(
  payload: SourceValue,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      project_graphql_value(payload, selections, fragments)
    _ -> json.object([])
  }
}

fn user_errors_source(errors: List(UserError)) -> SourceValue {
  SrcList(list.map(errors, user_error_to_source))
}

fn user_error_to_source(error: UserError) -> SourceValue {
  let base = [
    #("__typename", SrcString("UserError")),
    #("field", SrcList(list.map(error.field, fn(part) { SrcString(part) }))),
    #("message", SrcString(error.message)),
  ]
  let full = case error.code {
    Some(c) -> list.append(base, [#("code", SrcString(c))])
    None -> list.append(base, [#("code", SrcNull)])
  }
  src_object(full)
}

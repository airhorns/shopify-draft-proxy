//// Mirrors the read path of `src/proxy/apps.ts`.
////
//// Pass 16 lands the six query roots (`app`, `appByHandle`, `appByKey`,
//// `appInstallation`, `appInstallations`, `currentAppInstallation`) plus
//// the per-record source projections needed to serve them. The
//// mutation path lives in a later pass — this module only exposes
//// pure functions of the store.
////
//// Note: the TS read path does NOT auto-create the default app /
//// installation. Only mutations do. So the dispatcher can keep the same
//// pure shape as for `webhooks` / `saved_searches`.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import shopify_draft_proxy/graphql/ast.{type Selection, Field, SelectionSet}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, ConnectionPageInfoOptions, ConnectionWindow,
  SerializeConnectionConfig, SelectedFieldOptions, SrcBool, SrcInt, SrcList,
  SrcNull, SrcString, default_connection_page_info_options,
  default_connection_window_options, get_document_fragments,
  get_field_response_key, paginate_connection_items, project_graphql_value,
  serialize_connection, src_object,
}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/types.{
  type AccessScopeRecord, type AppInstallationRecord, type AppOneTimePurchaseRecord,
  type AppRecord, type AppSubscriptionLineItemPlan,
  type AppSubscriptionLineItemRecord, type AppSubscriptionPricing,
  type AppSubscriptionRecord, type AppUsageRecord, type Money,
  AppRecurringPricing, AppUsagePricing,
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

/// Wrap a successful apps response in the standard GraphQL envelope.
pub fn wrap_data(data: Json) -> Json {
  json.object([#("data", data)])
}

/// Convenience: parse + handle + wrap, for the dispatcher.
pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, AppsError) {
  use data <- result.try(handle_app_query(store, document, variables))
  Ok(wrap_data(data))
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
        "appByKey" ->
          serialize_app_by_key(store, field, fragments, variables)
        "appInstallations" ->
          serialize_app_installations_connection(store, field, fragments)
        _ -> json.null()
      }
    _ -> json.null()
  }
}

fn read_arg_string(
  args: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(String) {
  case dict.get(args, name) {
    Ok(root_field.StringVal(s)) -> Some(s)
    _ -> None
  }
}

fn field_args(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> Dict(String, root_field.ResolvedValue) {
  case root_field.get_field_arguments(field, variables) {
    Ok(d) -> d
    Error(_) -> dict.new()
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
    Some(installation) -> project_app_installation(store, installation, field, fragments)
    None -> json.null()
  }
}

fn serialize_app_installation_by_id(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = field_args(field, variables)
  case read_arg_string(args, "id") {
    Some(id) ->
      case store.get_effective_app_installation_by_id(store, id) {
        Some(installation) -> project_app_installation(store, installation, field, fragments)
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
  let args = field_args(field, variables)
  case read_arg_string(args, "id") {
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
  let args = field_args(field, variables)
  case read_arg_string(args, "handle") {
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
  let args = field_args(field, variables)
  case read_arg_string(args, "apiKey") {
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

fn installation_cursor_value(record: AppInstallationRecord, _index: Int) -> String {
  record.id
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
    #("apiKey", optional_string_to_source(app.api_key)),
    #("handle", optional_string_to_source(app.handle)),
    #("title", optional_string_to_source(app.title)),
    #("developerName", optional_string_to_source(app.developer_name)),
    #("embedded", optional_bool_to_source(app.embedded)),
    #(
      "previouslyInstalled",
      optional_bool_to_source(app.previously_installed),
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
    #("description", optional_string_to_source(scope.description)),
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
    #("launchUrl", optional_string_to_source(installation.launch_url)),
    #("uninstallUrl", optional_string_to_source(installation.uninstall_url)),
    #(
      "accessScopes",
      SrcList(list.map(installation.access_scopes, access_scope_to_source)),
    ),
    #(
      "activeSubscriptions",
      SrcList(list.map(active_subscriptions, fn(s) {
        subscription_to_source(store, s, fragments)
      })),
    ),
    #(
      "allSubscriptions",
      subscription_connection_source(store, all_subscriptions, fragments),
    ),
    #(
      "oneTimePurchases",
      one_time_purchase_connection_source(one_time_purchases),
    ),
    #("uninstalledAt", optional_string_to_source(installation.uninstalled_at)),
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
    #("trialDays", optional_int_to_source(subscription.trial_days)),
    #(
      "currentPeriodEnd",
      optional_string_to_source(subscription.current_period_end),
    ),
    #("createdAt", SrcString(subscription.created_at)),
    #(
      "lineItems",
      SrcList(list.map(line_items, fn(li) {
        line_item_to_source(store, li, fragments)
      })),
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
    #(
      "usageRecords",
      usage_record_connection_source(store, usage_records),
    ),
  ])
}

fn line_item_plan_to_source(
  plan: AppSubscriptionLineItemPlan,
) -> graphql_helpers.SourceValue {
  src_object([
    #("__typename", SrcString("AppPlan")),
    #(
      "pricingDetails",
      pricing_to_source(plan.pricing_details),
    ),
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
        #("planHandle", optional_string_to_source(handle)),
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
        #("terms", optional_string_to_source(terms)),
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
    #("idempotencyKey", optional_string_to_source(record.idempotency_key)),
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
    SrcList(list.map(subscriptions, fn(s) {
      subscription_to_source(store, s, fragments)
    }))
  let edges =
    SrcList(list.map(subscriptions, fn(s) {
      src_object([
        #("__typename", SrcString("AppSubscriptionEdge")),
        #("cursor", SrcString(s.id)),
        #("node", subscription_to_source(store, s, fragments)),
      ])
    }))
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
    SrcList(list.map(purchases, fn(p) {
      src_object([
        #("__typename", SrcString("AppPurchaseOneTimeEdge")),
        #("cursor", SrcString(p.id)),
        #("node", one_time_purchase_to_source(p)),
      ])
    }))
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
    SrcList(list.map(records, fn(r) {
      src_object([
        #("__typename", SrcString("AppUsageRecordEdge")),
        #("cursor", SrcString(r.id)),
        #("node", usage_record_to_source(store, r)),
      ])
    }))
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

// ---------------------------------------------------------------------------
// Small Option → SourceValue helpers
// ---------------------------------------------------------------------------

fn optional_string_to_source(value: Option(String)) -> graphql_helpers.SourceValue {
  case value {
    Some(s) -> SrcString(s)
    None -> SrcNull
  }
}

fn optional_bool_to_source(value: Option(Bool)) -> graphql_helpers.SourceValue {
  case value {
    Some(b) -> SrcBool(b)
    None -> SrcNull
  }
}

fn optional_int_to_source(value: Option(Int)) -> graphql_helpers.SourceValue {
  case value {
    Some(i) -> SrcInt(i)
    None -> SrcNull
  }
}

//// Apps GraphQL serializers and payload projectors.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import shopify_draft_proxy/graphql/ast.{type Selection, Field, SelectionSet}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/apps/types as app_types
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, ConnectionPageInfoOptions,
  ConnectionWindow, SelectedFieldOptions, SerializeConnectionConfig, SrcBool,
  SrcInt, SrcList, SrcNull, SrcString, default_connection_page_info_options,
  default_connection_window_options, get_field_response_key,
  paginate_connection_items, project_graphql_value, serialize_connection,
  src_object,
}
import shopify_draft_proxy/proxy/store_properties
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/types.{
  type AccessScopeRecord, type AppInstallationRecord,
  type AppOneTimePurchaseRecord, type AppRecord,
  type AppSubscriptionLineItemPlan, type AppSubscriptionLineItemRecord,
  type AppSubscriptionPricing, type AppSubscriptionRecord, type AppUsageRecord,
  type Money, AppRecurringPricing, AppUsagePricing,
}

@internal
pub fn serialize_root_fields(
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

@internal
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

@internal
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

@internal
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

@internal
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

@internal
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

// ---------------------------------------------------------------------------
// Mutation projections
// ---------------------------------------------------------------------------

@internal
pub fn project_uninstall_payload(
  app: Option(AppRecord),
  user_errors: List(app_types.UserError),
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

@internal
pub fn project_revoke_payload(
  revoked: List(AccessScopeRecord),
  user_errors: List(app_types.UserError),
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

@internal
pub fn project_delegate_create_payload(
  store: Store,
  raw_token: Option(String),
  access_scopes: List(String),
  created_at: Option(String),
  expires_in: Option(Int),
  user_errors: List(app_types.DelegateAccessTokenUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let token_source = case raw_token {
    Some(raw) ->
      case created_at {
        Some(timestamp) ->
          src_object([
            #("__typename", SrcString("DelegateAccessToken")),
            #("accessToken", SrcString(raw)),
            #(
              "accessScopes",
              SrcList(list.map(access_scopes, fn(s) { SrcString(s) })),
            ),
            #("createdAt", SrcString(timestamp)),
            #("expiresIn", graphql_helpers.option_int_source(expires_in)),
          ])
        None -> SrcNull
      }
    None -> SrcNull
  }
  let payload =
    src_object([
      #("delegateAccessToken", token_source),
      #("shop", current_shop_source(store)),
      #("userErrors", delegate_user_errors_source(user_errors)),
    ])
  project_payload(payload, field, fragments)
}

/// Return the Apps payload `Shop` source from hydrated store state when it is
/// available, otherwise use a stable local fallback for non-null payload fields.
fn current_shop_source(store: Store) -> SourceValue {
  case store.get_effective_shop(store) {
    Some(shop) -> store_properties.shop_source(shop)
    None -> synthetic_shop_source()
  }
}

fn synthetic_shop_source() -> SourceValue {
  src_object([
    #("__typename", SrcString("Shop")),
    #("id", SrcString(app_types.synthetic_shop_id)),
    #("name", SrcString("Shopify Draft Proxy")),
    #("myshopifyDomain", SrcString("shopify-draft-proxy.myshopify.com")),
    #("currencyCode", SrcString(app_types.default_billing_currency)),
  ])
}

fn delegate_user_errors_source(
  errors: List(app_types.DelegateAccessTokenUserError),
) -> SourceValue {
  SrcList(list.map(errors, delegate_user_error_to_source))
}

fn delegate_user_error_to_source(
  error: app_types.DelegateAccessTokenUserError,
) -> SourceValue {
  let app_types.DelegateAccessTokenUserError(
    field: field,
    message: message,
    code: code,
  ) = error
  let field_source = case field {
    Some(parts) -> SrcList(list.map(parts, fn(part) { SrcString(part) }))
    None -> SrcNull
  }
  let code_source = case code {
    Some(c) -> SrcString(c)
    None -> SrcNull
  }
  src_object([
    #("__typename", SrcString("app_types.UserError")),
    #("field", field_source),
    #("message", SrcString(message)),
    #("code", code_source),
  ])
}

@internal
pub fn project_delegate_destroy_payload(
  store: Store,
  status: Bool,
  user_errors: List(app_types.DelegateAccessTokenUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let payload =
    src_object([
      #("status", SrcBool(status)),
      #("shop", current_shop_source(store)),
      #("userErrors", delegate_user_errors_source(user_errors)),
    ])
  project_payload(payload, field, fragments)
}

@internal
pub fn project_purchase_create_payload(
  purchase: Option(AppOneTimePurchaseRecord),
  confirmation: Option(String),
  user_errors: List(app_types.UserError),
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

@internal
pub fn project_subscription_create_payload(
  store: Store,
  subscription: Option(AppSubscriptionRecord),
  confirmation: Option(String),
  user_errors: List(app_types.UserError),
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

@internal
pub fn project_subscription_payload(
  store: Store,
  subscription: Option(AppSubscriptionRecord),
  confirmation: Option(String),
  user_errors: List(app_types.UserError),
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

@internal
pub fn project_usage_record_payload(
  store: Store,
  record: Option(AppUsageRecord),
  user_errors: List(app_types.UserError),
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

fn user_errors_source(errors: List(app_types.UserError)) -> SourceValue {
  SrcList(list.map(errors, user_error_to_source))
}

fn user_error_to_source(error: app_types.UserError) -> SourceValue {
  let base = [
    #("__typename", SrcString("app_types.UserError")),
    #("field", SrcList(list.map(error.field, fn(part) { SrcString(part) }))),
    #("message", SrcString(error.message)),
  ]
  let full = case error.code {
    Some(c) -> list.append(base, [#("code", SrcString(c))])
    None -> list.append(base, [#("code", SrcNull)])
  }
  src_object(full)
}

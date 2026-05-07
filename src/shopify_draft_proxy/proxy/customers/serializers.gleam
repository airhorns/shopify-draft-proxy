//// Customer domain internals split from proxy/customers.gleam.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/order
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection, Field, SelectionSet}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/customers/customer_types.{
  type UserError, MutationFieldResult, UserError,
}
import shopify_draft_proxy/proxy/customers/hydration.{
  customer_account_pages_from_hydrate,
}
import shopify_draft_proxy/proxy/customers/inputs.{
  build_merged_customer, customer_metafield_key, json_get, json_get_int,
  json_get_string, non_null_json, option_to_result, read_obj_string,
}
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, ConnectionPageInfoOptions,
  ConnectionWindow, SelectedFieldOptions, SerializeConnectionConfig, SrcBool,
  SrcInt, SrcList, SrcNull, SrcObject, SrcString,
  default_connection_page_info_options, default_connection_window_options,
  get_field_response_key, paginate_connection_items, project_graphql_value,
  serialize_connection, src_object,
}
import shopify_draft_proxy/proxy/payments
import shopify_draft_proxy/proxy/phone_numbers
import shopify_draft_proxy/proxy/proxy_state.{type DraftProxy, LiveHybrid}
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/search_query_parser
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/types.{
  type CustomerAccountPageRecord, type CustomerAddressRecord,
  type CustomerCatalogConnectionRecord, type CustomerCatalogPageInfoRecord,
  type CustomerDefaultAddressRecord, type CustomerDefaultEmailAddressRecord,
  type CustomerDefaultPhoneNumberRecord,
  type CustomerEmailMarketingConsentRecord, type CustomerEventSummaryRecord,
  type CustomerMergeErrorRecord, type CustomerMergeRequestRecord,
  type CustomerMetafieldRecord, type CustomerOrderSummaryRecord,
  type CustomerPaymentMethodRecord, type CustomerRecord,
  type CustomerSmsMarketingConsentRecord, type Money,
  type ProductMetafieldRecord, type StoreCreditAccountRecord,
  type StoreCreditAccountTransactionRecord, CustomerCatalogPageInfoRecord,
  CustomerDefaultAddressRecord, CustomerMetafieldRecord,
}

@internal
pub fn wrap_query_payload(data: Json, search_extensions: List(Json)) -> Json {
  case search_extensions {
    [] -> graphql_helpers.wrap_data(data)
    [_, ..] ->
      json.object([
        #("data", data),
        #(
          "extensions",
          json.object([
            #(
              "search",
              json.array(search_extensions, fn(extension) { extension }),
            ),
          ]),
        ),
      ])
  }
}

@internal
pub fn customer_count_search_extensions(
  fields: List(Selection),
  variables: Dict(String, root_field.ResolvedValue),
) -> List(Json) {
  fields
  |> list.filter_map(fn(field) {
    case field {
      Field(name: name, ..) ->
        case name.value {
          "customersCount" ->
            build_customers_count_search_extension(
              graphql_helpers.read_arg_string_nonempty(
                graphql_helpers.field_args(field, variables),
                "query",
              ),
              get_field_response_key(field),
            )
          _ -> Error(Nil)
        }
      _ -> Error(Nil)
    }
  })
}

@internal
pub fn build_customers_count_search_extension(
  query: Option(String),
  response_key: String,
) -> Result(Json, Nil) {
  use raw <- result.try(option_to_result(query))
  let trimmed = string.trim(raw)
  case string.split_once(trimmed, ":") {
    Ok(#(raw_field, raw_value)) -> {
      let field = raw_field |> string.trim |> string.lowercase
      let match_all = string.trim(raw_value)
      case { field == "email" || field == "state" } && match_all != "" {
        True ->
          Ok(
            json.object([
              #("path", json.array([response_key], json.string)),
              #("query", json.string(trimmed)),
              #(
                "parsed",
                json.object([
                  #("field", json.string(field)),
                  #("match_all", json.string(match_all)),
                ]),
              ),
              #(
                "warnings",
                json.array([field], fn(warning_field) {
                  json.object([
                    #("field", json.string(warning_field)),
                    #(
                      "message",
                      json.string("Invalid search field for this query."),
                    ),
                    #("code", json.string("invalid_field")),
                  ])
                }),
              ),
            ]),
          )
        False -> Error(Nil)
      }
    }
    Error(_) -> Error(Nil)
  }
}

@internal
pub fn serialize_root_fields(
  proxy: DraftProxy,
  fields: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> Json {
  json.object(
    list.map(fields, fn(field) {
      #(
        get_field_response_key(field),
        root_payload_for_field(proxy, field, fragments, variables, upstream),
      )
    }),
  )
}

@internal
pub fn root_payload_for_field(
  proxy: DraftProxy,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> Json {
  let store = proxy.store
  case field {
    Field(name: name, ..) ->
      case name.value {
        "customer" ->
          serialize_customer_by_id(store, field, fragments, variables)
        "customerByIdentifier" ->
          serialize_customer_by_identifier(store, field, fragments, variables)
        "customers" ->
          serialize_customers_connection(store, field, fragments, variables)
        "customersCount" ->
          serialize_customers_count(proxy, field, variables, upstream)
        "customerAccountPage" ->
          serialize_customer_account_page(store, field, fragments, variables)
        "customerAccountPages" ->
          serialize_customer_account_pages(
            proxy,
            field,
            fragments,
            variables,
            upstream,
          )
        "customerMergePreview" ->
          serialize_customer_merge_preview(store, field, fragments, variables)
        "customerMergeJobStatus" ->
          serialize_customer_merge_job_status(
            store,
            field,
            fragments,
            variables,
          )
        "storeCreditAccount" ->
          serialize_store_credit_account_by_id(
            store,
            field,
            fragments,
            variables,
          )
        "customerPaymentMethod" ->
          serialize_customer_payment_method_by_id(
            store,
            field,
            fragments,
            variables,
          )
        _ -> json.null()
      }
    _ -> json.null()
  }
}

@internal
pub fn money_source(value: Money) -> SourceValue {
  src_object([
    #("__typename", SrcString("MoneyV2")),
    #("amount", SrcString(value.amount)),
    #("currencyCode", SrcString(value.currency_code)),
  ])
}

@internal
pub fn default_email_source(
  value: Option(CustomerDefaultEmailAddressRecord),
) -> SourceValue {
  case value {
    Some(v) ->
      src_object([
        #("emailAddress", graphql_helpers.option_string_source(v.email_address)),
        #(
          "marketingState",
          graphql_helpers.option_string_source(v.marketing_state),
        ),
        #(
          "marketingOptInLevel",
          graphql_helpers.option_string_source(v.marketing_opt_in_level),
        ),
        #(
          "marketingUpdatedAt",
          graphql_helpers.option_string_source(v.marketing_updated_at),
        ),
      ])
    None -> SrcNull
  }
}

@internal
pub fn default_phone_source(
  value: Option(CustomerDefaultPhoneNumberRecord),
) -> SourceValue {
  case value {
    Some(v) ->
      src_object([
        #("phoneNumber", graphql_helpers.option_string_source(v.phone_number)),
        #(
          "marketingState",
          graphql_helpers.option_string_source(v.marketing_state),
        ),
        #(
          "marketingOptInLevel",
          graphql_helpers.option_string_source(v.marketing_opt_in_level),
        ),
        #(
          "marketingUpdatedAt",
          graphql_helpers.option_string_source(v.marketing_updated_at),
        ),
        #(
          "marketingCollectedFrom",
          graphql_helpers.option_string_source(v.marketing_collected_from),
        ),
      ])
    None -> SrcNull
  }
}

@internal
pub fn email_consent_source(
  value: Option(CustomerEmailMarketingConsentRecord),
) -> SourceValue {
  case value {
    Some(v) ->
      src_object([
        #(
          "marketingState",
          graphql_helpers.option_string_source(v.marketing_state),
        ),
        #(
          "marketingOptInLevel",
          graphql_helpers.option_string_source(v.marketing_opt_in_level),
        ),
        #(
          "consentUpdatedAt",
          graphql_helpers.option_string_source(v.consent_updated_at),
        ),
      ])
    None -> SrcNull
  }
}

@internal
pub fn sms_consent_source(
  value: Option(CustomerSmsMarketingConsentRecord),
) -> SourceValue {
  case value {
    Some(v) ->
      src_object([
        #(
          "marketingState",
          graphql_helpers.option_string_source(v.marketing_state),
        ),
        #(
          "marketingOptInLevel",
          graphql_helpers.option_string_source(v.marketing_opt_in_level),
        ),
        #(
          "consentUpdatedAt",
          graphql_helpers.option_string_source(v.consent_updated_at),
        ),
        #(
          "consentCollectedFrom",
          graphql_helpers.option_string_source(v.consent_collected_from),
        ),
      ])
    None -> SrcNull
  }
}

@internal
pub fn address_to_default(
  address: CustomerAddressRecord,
) -> CustomerDefaultAddressRecord {
  CustomerDefaultAddressRecord(
    id: Some(address.id),
    first_name: address.first_name,
    last_name: address.last_name,
    address1: address.address1,
    address2: address.address2,
    city: address.city,
    company: address.company,
    province: address.province,
    province_code: address.province_code,
    country: address.country,
    country_code_v2: address.country_code_v2,
    zip: address.zip,
    phone: address.phone,
    name: address.name,
    formatted_area: address.formatted_area,
  )
}

@internal
pub fn default_address_source(
  value: Option(CustomerDefaultAddressRecord),
) -> SourceValue {
  case value {
    Some(v) ->
      src_object([
        #("__typename", SrcString("MailingAddress")),
        #("id", graphql_helpers.option_string_source(v.id)),
        #("firstName", graphql_helpers.option_string_source(v.first_name)),
        #("lastName", graphql_helpers.option_string_source(v.last_name)),
        #("address1", graphql_helpers.option_string_source(v.address1)),
        #("address2", graphql_helpers.option_string_source(v.address2)),
        #("city", graphql_helpers.option_string_source(v.city)),
        #("company", graphql_helpers.option_string_source(v.company)),
        #("province", graphql_helpers.option_string_source(v.province)),
        #("provinceCode", graphql_helpers.option_string_source(v.province_code)),
        #("country", graphql_helpers.option_string_source(v.country)),
        #(
          "countryCodeV2",
          graphql_helpers.option_string_source(v.country_code_v2),
        ),
        #("zip", graphql_helpers.option_string_source(v.zip)),
        #("phone", graphql_helpers.option_string_source(v.phone)),
        #("name", graphql_helpers.option_string_source(v.name)),
        #(
          "formattedArea",
          graphql_helpers.option_string_source(v.formatted_area),
        ),
      ])
    None -> SrcNull
  }
}

@internal
pub fn address_source(address: CustomerAddressRecord) -> SourceValue {
  default_address_source(Some(address_to_default(address)))
}

@internal
pub fn customer_to_source(
  store: Store,
  customer: CustomerRecord,
) -> SourceValue {
  let customer_metafields = customer_metafields_for_source(store, customer.id)
  src_object([
    #("__typename", SrcString("Customer")),
    #("id", SrcString(customer.id)),
    #("firstName", graphql_helpers.option_string_source(customer.first_name)),
    #("lastName", graphql_helpers.option_string_source(customer.last_name)),
    #(
      "displayName",
      graphql_helpers.option_string_source(customer.display_name),
    ),
    #("email", graphql_helpers.option_string_source(customer.email)),
    #(
      "phone",
      graphql_helpers.option_string_source(customer_phone_number(customer)),
    ),
    #(
      "legacyResourceId",
      graphql_helpers.option_string_source(customer.legacy_resource_id),
    ),
    #("locale", graphql_helpers.option_string_source(customer.locale)),
    #("note", graphql_helpers.option_string_source(customer.note)),
    #("canDelete", graphql_helpers.option_bool_source(customer.can_delete)),
    #(
      "verifiedEmail",
      graphql_helpers.option_bool_source(customer.verified_email),
    ),
    #("dataSaleOptOut", SrcBool(customer.data_sale_opt_out)),
    #("taxExempt", graphql_helpers.option_bool_source(customer.tax_exempt)),
    #("taxExemptions", SrcList(list.map(customer.tax_exemptions, SrcString))),
    #("state", graphql_helpers.option_string_source(customer.state)),
    #("tags", SrcList(list.map(customer.tags, SrcString))),
    #(
      "numberOfOrders",
      graphql_helpers.option_string_source(customer.number_of_orders),
    ),
    #("amountSpent", case customer.amount_spent {
      Some(m) -> money_source(m)
      None -> SrcNull
    }),
    #(
      "defaultEmailAddress",
      default_email_source(customer.default_email_address),
    ),
    #("defaultPhoneNumber", default_phone_source(customer.default_phone_number)),
    #(
      "emailMarketingConsent",
      email_consent_source(customer.email_marketing_consent),
    ),
    #("smsMarketingConsent", sms_consent_source(customer.sms_marketing_consent)),
    #("defaultAddress", default_address_source(customer.default_address)),
    #(
      "addresses",
      SrcList(list.map(
        store.list_effective_customer_addresses(store, customer.id),
        address_source,
      )),
    ),
    #("addressesV2", address_connection_source(store, customer.id)),
    #("metafield", case customer_metafields {
      [first, ..] -> metafield_source(first)
      [] -> SrcNull
    }),
    #("metafields", metafield_connection_source_from(customer_metafields)),
    #(
      "storeCreditAccounts",
      store_credit_accounts_connection_source(store, customer.id),
    ),
    #("paymentMethods", payment_methods_connection_source(store, customer.id)),
    #("subscriptionContracts", empty_connection_source()),
    #("orders", orders_connection_source(store, customer.id)),
    #("events", events_connection_source(store, customer.id)),
    #("companyContactProfiles", SrcList([])),
    #(
      "lastOrder",
      case store.get_effective_customer_last_order(store, customer.id) {
        Some(order) -> order_summary_source(store, order)
        None -> SrcNull
      },
    ),
    #("createdAt", graphql_helpers.option_string_source(customer.created_at)),
    #("updatedAt", graphql_helpers.option_string_source(customer.updated_at)),
  ])
}

@internal
pub fn customer_phone_number(customer: CustomerRecord) -> Option(String) {
  customer.default_phone_number
  |> option.then(fn(record) { record.phone_number })
}

@internal
pub fn customer_metafields_for_source(
  store: Store,
  customer_id: String,
) -> List(CustomerMetafieldRecord) {
  let generic_metafields =
    store.get_effective_metafields_by_owner_id(store, customer_id)
    |> list.filter_map(fn(metafield) {
      generic_customer_metafield_to_record(customer_id, metafield)
    })
  let generic_keys = list.map(generic_metafields, customer_metafield_key)
  let native_metafields =
    store.get_effective_metafields_by_customer_id(store, customer_id)
    |> list.filter(fn(metafield) {
      !list.contains(generic_keys, customer_metafield_key(metafield))
    })
  list.append(native_metafields, generic_metafields)
}

@internal
pub fn generic_customer_metafield_to_record(
  customer_id: String,
  metafield: ProductMetafieldRecord,
) -> Result(CustomerMetafieldRecord, Nil) {
  case metafield.type_, metafield.value {
    Some(type_), Some(value) ->
      Ok(CustomerMetafieldRecord(
        id: metafield.id,
        customer_id: customer_id,
        namespace: metafield.namespace,
        key: metafield.key,
        type_: type_,
        value: value,
        compare_digest: metafield.compare_digest,
        created_at: metafield.created_at,
        updated_at: metafield.updated_at,
      ))
    _, _ -> Error(Nil)
  }
}

@internal
pub fn connection_source(
  nodes: List(SourceValue),
  cursors: List(String),
) -> SourceValue {
  connection_source_with_page_info(nodes, cursors, None)
}

@internal
pub fn connection_source_with_page_info(
  nodes: List(SourceValue),
  cursors: List(String),
  captured_page_info: Option(CustomerCatalogPageInfoRecord),
) -> SourceValue {
  let edges =
    zip_nodes(nodes, cursors)
    |> list.map(fn(pair) {
      let #(node, cursor) = pair
      src_object([#("cursor", SrcString(cursor)), #("node", node)])
    })
  src_object([
    #("nodes", SrcList(nodes)),
    #("edges", SrcList(edges)),
    #("pageInfo", page_info_source(cursors, captured_page_info)),
  ])
}

@internal
pub fn empty_connection_source() -> SourceValue {
  connection_source([], [])
}

@internal
pub fn page_info_source(
  cursors: List(String),
  captured_page_info: Option(CustomerCatalogPageInfoRecord),
) -> SourceValue {
  let start = case cursors {
    [first, ..] -> SrcString(first)
    [] -> SrcNull
  }
  let end = case list.last(cursors) {
    Ok(last) -> SrcString(last)
    Error(_) -> SrcNull
  }
  case captured_page_info {
    Some(info) ->
      src_object([
        #("hasNextPage", SrcBool(info.has_next_page)),
        #("hasPreviousPage", SrcBool(info.has_previous_page)),
        #(
          "startCursor",
          graphql_helpers.option_string_source(
            info.start_cursor |> option.or(source_string_option(start)),
          ),
        ),
        #(
          "endCursor",
          graphql_helpers.option_string_source(
            info.end_cursor |> option.or(source_string_option(end)),
          ),
        ),
      ])
    None ->
      src_object([
        #("hasNextPage", SrcBool(False)),
        #("hasPreviousPage", SrcBool(False)),
        #("startCursor", start),
        #("endCursor", end),
      ])
  }
}

@internal
pub fn source_string_option(value: SourceValue) -> Option(String) {
  case value {
    SrcString(s) -> Some(s)
    _ -> None
  }
}

@internal
pub fn zip_nodes(
  nodes: List(SourceValue),
  cursors: List(String),
) -> List(#(SourceValue, String)) {
  case nodes, cursors {
    [node, ..rest_nodes], [cursor, ..rest_cursors] -> [
      #(node, cursor),
      ..zip_nodes(rest_nodes, rest_cursors)
    ]
    _, _ -> []
  }
}

@internal
pub fn address_connection_source(
  store: Store,
  customer_id: String,
) -> SourceValue {
  let all_addresses =
    store.list_effective_customer_addresses(store, customer_id)
  let captured_page =
    all_addresses
    |> list.filter(fn(address) {
      case address.cursor {
        Some(_) -> True
        None -> False
      }
    })
  let addresses = case captured_page {
    [] -> all_addresses
    _ -> captured_page
  }
  let captured_page_info = case captured_page {
    [] -> None
    _ ->
      Some(CustomerCatalogPageInfoRecord(
        has_next_page: list.length(all_addresses) > list.length(captured_page),
        has_previous_page: False,
        start_cursor: first_address_cursor(captured_page),
        end_cursor: last_address_cursor(captured_page),
      ))
  }
  let nodes = list.map(addresses, address_source)
  let cursors =
    list.map(addresses, fn(address) {
      case address.cursor {
        Some(c) -> c
        None -> "cursor:" <> address.id
      }
    })
  connection_source_with_page_info(nodes, cursors, captured_page_info)
}

@internal
pub fn first_address_cursor(
  addresses: List(CustomerAddressRecord),
) -> Option(String) {
  case addresses {
    [first, ..] -> first.cursor
    [] -> None
  }
}

@internal
pub fn last_address_cursor(
  addresses: List(CustomerAddressRecord),
) -> Option(String) {
  case list.last(addresses) {
    Ok(last) -> last.cursor
    Error(_) -> None
  }
}

@internal
pub fn order_summary_source(
  store: Store,
  order: CustomerOrderSummaryRecord,
) -> SourceValue {
  let customer = case order.customer_id {
    Some(customer_id) -> store.get_effective_customer_by_id(store, customer_id)
    None -> None
  }
  src_object([
    #("__typename", SrcString("Order")),
    #("id", SrcString(order.id)),
    #("name", graphql_helpers.option_string_source(order.name)),
    #("email", graphql_helpers.option_string_source(order.email)),
    #("createdAt", graphql_helpers.option_string_source(order.created_at)),
    #("currentTotalPriceSet", case order.current_total_price {
      Some(money) -> src_object([#("shopMoney", money_source(money))])
      None -> SrcNull
    }),
    #("customer", case customer {
      Some(c) -> customer_owner_source(c)
      None -> SrcNull
    }),
  ])
}

@internal
pub fn orders_connection_source(
  store: Store,
  customer_id: String,
) -> SourceValue {
  let orders = store.list_effective_customer_order_summaries(store, customer_id)
  connection_source_with_page_info(
    list.map(orders, fn(order) { order_summary_source(store, order) }),
    list.map(orders, fn(order) {
      order.cursor |> option.unwrap("cursor:" <> order.id)
    }),
    store.get_effective_customer_order_connection_page_info(store, customer_id),
  )
}

@internal
pub fn event_summary_source(event: CustomerEventSummaryRecord) -> SourceValue {
  src_object([
    #("__typename", SrcString("BasicEvent")),
    #("id", SrcString(event.id)),
  ])
}

@internal
pub fn events_connection_source(
  store: Store,
  customer_id: String,
) -> SourceValue {
  let events = store.list_effective_customer_event_summaries(store, customer_id)
  connection_source_with_page_info(
    list.map(events, event_summary_source),
    list.map(events, fn(event) {
      event.cursor |> option.unwrap("cursor:" <> event.id)
    }),
    store.get_effective_customer_event_connection_page_info(store, customer_id),
  )
}

@internal
pub fn metafield_source(metafield: CustomerMetafieldRecord) -> SourceValue {
  src_object([
    #("__typename", SrcString("Metafield")),
    #("id", SrcString(metafield.id)),
    #("namespace", SrcString(metafield.namespace)),
    #("key", SrcString(metafield.key)),
    #("type", SrcString(metafield.type_)),
    #("value", SrcString(metafield.value)),
    #(
      "compareDigest",
      graphql_helpers.option_string_source(metafield.compare_digest),
    ),
    #("createdAt", graphql_helpers.option_string_source(metafield.created_at)),
    #("updatedAt", graphql_helpers.option_string_source(metafield.updated_at)),
  ])
}

@internal
pub fn metafield_connection_source_from(
  metafields: List(CustomerMetafieldRecord),
) -> SourceValue {
  connection_source(
    list.map(metafields, metafield_source),
    list.map(metafields, fn(m) { "cursor:" <> m.id }),
  )
}

@internal
pub fn store_credit_account_source(
  store: Store,
  account: StoreCreditAccountRecord,
) -> SourceValue {
  let base = store_credit_account_shallow_source(store, account)
  case base {
    SrcObject(fields) ->
      SrcObject(dict.insert(
        fields,
        "transactions",
        store_credit_transactions_connection_source(store, account.id),
      ))
    _ -> base
  }
}

@internal
pub fn store_credit_account_shallow_source(
  store: Store,
  account: StoreCreditAccountRecord,
) -> SourceValue {
  let owner = store_credit_account_owner_source(store, account.customer_id)
  src_object([
    #("__typename", SrcString("StoreCreditAccount")),
    #("id", SrcString(account.id)),
    #("balance", money_source(account.balance)),
    #("owner", owner),
  ])
}

@internal
pub fn store_credit_account_owner_source(
  store: Store,
  owner_id: String,
) -> SourceValue {
  case string.starts_with(owner_id, "gid://shopify/CompanyLocation/") {
    True ->
      case store.get_effective_b2b_company_location_by_id(store, owner_id) {
        Some(location) ->
          src_object([
            #("__typename", SrcString("CompanyLocation")),
            #("id", SrcString(location.id)),
          ])
        None -> SrcNull
      }
    False ->
      case store.get_effective_customer_by_id(store, owner_id) {
        Some(customer) -> customer_owner_source(customer)
        None -> SrcNull
      }
  }
}

@internal
pub fn store_credit_accounts_connection_source(
  store: Store,
  customer_id: String,
) -> SourceValue {
  let accounts =
    store.list_effective_store_credit_accounts_for_customer(store, customer_id)
  connection_source(
    list.map(accounts, fn(a) { store_credit_account_source(store, a) }),
    list.map(accounts, fn(a) {
      case a.cursor {
        Some(c) -> c
        None -> "cursor:" <> a.id
      }
    }),
  )
}

@internal
pub fn store_credit_transaction_source(
  store: Store,
  transaction: StoreCreditAccountTransactionRecord,
) -> SourceValue {
  let account =
    store.get_effective_store_credit_account_by_id(
      store,
      transaction.account_id,
    )
  src_object([
    #("__typename", SrcString("StoreCreditAccountTransaction")),
    #("id", SrcString(transaction.id)),
    #("amount", money_source(transaction.amount)),
    #(
      "balanceAfterTransaction",
      money_source(transaction.balance_after_transaction),
    ),
    #("createdAt", SrcString(transaction.created_at)),
    #("event", SrcString(transaction.event)),
    #("origin", SrcNull),
    #("account", case account {
      Some(a) -> store_credit_account_shallow_source(store, a)
      None -> SrcNull
    }),
  ])
}

@internal
pub fn store_credit_transactions_connection_source(
  store: Store,
  account_id: String,
) -> SourceValue {
  let txns =
    store.list_effective_store_credit_account_transactions(store, account_id)
  connection_source(
    list.map(txns, fn(t) { store_credit_transaction_source(store, t) }),
    list.map(txns, fn(t) { "cursor:" <> t.id }),
  )
}

@internal
pub fn payment_method_source(
  store: Store,
  payment_method: CustomerPaymentMethodRecord,
) -> SourceValue {
  let customer =
    store.get_effective_customer_by_id(store, payment_method.customer_id)
  src_object([
    #("__typename", SrcString("CustomerPaymentMethod")),
    #("id", SrcString(payment_method.id)),
    #("customer", case customer {
      Some(c) -> customer_owner_source(c)
      None -> SrcNull
    }),
    #("instrument", case payment_method.instrument {
      Some(instrument) -> payments.instrument_source(instrument)
      None -> SrcNull
    }),
    #(
      "revokedAt",
      graphql_helpers.option_string_source(payment_method.revoked_at),
    ),
    #(
      "revokedReason",
      graphql_helpers.option_string_source(payment_method.revoked_reason),
    ),
    #("subscriptionContracts", empty_connection_source()),
  ])
}

@internal
pub fn customer_owner_source(customer: CustomerRecord) -> SourceValue {
  src_object([
    #("__typename", SrcString("Customer")),
    #("id", SrcString(customer.id)),
    #("firstName", graphql_helpers.option_string_source(customer.first_name)),
    #("lastName", graphql_helpers.option_string_source(customer.last_name)),
    #(
      "displayName",
      graphql_helpers.option_string_source(customer.display_name),
    ),
    #("email", graphql_helpers.option_string_source(customer.email)),
    #(
      "defaultEmailAddress",
      default_email_source(customer.default_email_address),
    ),
    #("defaultPhoneNumber", default_phone_source(customer.default_phone_number)),
  ])
}

@internal
pub fn payment_methods_connection_source(
  store: Store,
  customer_id: String,
) -> SourceValue {
  let methods =
    store.list_effective_customer_payment_methods(store, customer_id, False)
  connection_source(
    list.map(methods, fn(m) { payment_method_source(store, m) }),
    list.map(methods, fn(m) {
      case m.cursor {
        Some(c) -> c
        None -> "cursor:" <> m.id
      }
    }),
  )
}

@internal
pub fn project_customer(
  store: Store,
  customer: CustomerRecord,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      project_graphql_value(
        customer_to_source(store, customer),
        selections,
        fragments,
      )
    _ -> json.object([])
  }
}

@internal
pub fn serialize_customer_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  case store.get_effective_customer_by_id(store, id) {
    Some(customer) ->
      project_graphql_value(
        customer_to_source(store, customer),
        selections,
        fragments,
      )
    None -> json.null()
  }
}

@internal
pub fn serialize_customer_by_id(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string_nonempty(args, "id") {
    Some(id) ->
      case store.get_effective_customer_by_id(store, id) {
        Some(customer) -> project_customer(store, customer, field, fragments)
        None -> json.null()
      }
    None -> json.null()
  }
}

@internal
pub fn serialize_customer_by_identifier(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  let identifier =
    graphql_helpers.read_arg_object(args, "identifier")
    |> option.unwrap(dict.new())
  case find_customer_by_identifier(store, identifier) {
    Some(customer) -> project_customer(store, customer, field, fragments)
    None -> json.null()
  }
}

@internal
pub fn find_customer_by_identifier(
  store: Store,
  identifier: Dict(String, root_field.ResolvedValue),
) -> Option(CustomerRecord) {
  case read_obj_string(identifier, "id") {
    Some(id) -> store.get_effective_customer_by_id(store, id)
    None -> {
      let email = read_obj_string(identifier, "emailAddress")
      let phone =
        read_obj_string(identifier, "phoneNumber")
        |> option.map(fn(value) {
          phone_numbers.normalize_for_store(store, value)
          |> result.unwrap(value)
        })
      find_customer_by_email_or_phone(
        store.list_effective_customers(store),
        email,
        phone,
      )
    }
  }
}

@internal
pub fn find_customer_by_email_or_phone(
  customers: List(CustomerRecord),
  email: Option(String),
  phone: Option(String),
) -> Option(CustomerRecord) {
  case customers {
    [] -> None
    [customer, ..rest] -> {
      let email_match = case email, customer.email {
        Some(needle), Some(value) ->
          string.lowercase(needle) == string.lowercase(value)
        _, _ -> False
      }
      let phone_match = case phone, customer.default_phone_number {
        Some(needle), Some(record) ->
          case record.phone_number {
            Some(value) -> needle == value
            None -> False
          }
        _, _ -> False
      }
      case email_match || phone_match {
        True -> Some(customer)
        False -> find_customer_by_email_or_phone(rest, email, phone)
      }
    }
  }
}

@internal
pub fn serialize_customers_connection(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  let query = graphql_helpers.read_arg_string_nonempty(args, "query")
  let sort_key = graphql_helpers.read_arg_string_nonempty(args, "sortKey")
  let reverse =
    graphql_helpers.read_arg_bool(args, "reverse") |> option.unwrap(False)
  let seeded_connection =
    store.get_base_customer_catalog_connection(
      store,
      get_field_response_key(field),
    )
  let items = case seeded_connection {
    Some(connection) -> list_customers_for_connection(store, connection)
    None ->
      store.list_effective_customers(store)
      |> filter_customers(query)
      |> sort_customers(sort_key, reverse)
  }
  let cursor_value = fn(record: CustomerRecord, _index: Int) -> String {
    case seeded_connection {
      Some(connection) ->
        resolve_customer_connection_cursor(record.id, connection)
      None -> record.id
    }
  }
  let window =
    paginate_connection_items(
      items,
      field,
      variables,
      cursor_value,
      default_connection_window_options(),
    )
  let ConnectionWindow(
    items: page_items,
    has_next_page: has_next,
    has_previous_page: has_prev,
  ) = window
  let page_info_options = case seeded_connection {
    Some(connection) ->
      ConnectionPageInfoOptions(
        include_inline_fragments: False,
        prefix_cursors: False,
        include_cursors: True,
        fallback_start_cursor: connection.page_info.start_cursor,
        fallback_end_cursor: connection.page_info.end_cursor,
      )
    None -> default_connection_page_info_options()
  }
  let has_next_page = case seeded_connection {
    Some(connection) -> has_next || connection.page_info.has_next_page
    None -> has_next
  }
  let has_previous_page = case seeded_connection {
    Some(connection) -> has_prev || connection.page_info.has_previous_page
    None -> has_prev
  }
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: page_items,
      has_next_page: has_next_page,
      has_previous_page: has_previous_page,
      get_cursor_value: cursor_value,
      serialize_node: fn(customer, selection, _index) {
        project_customer(store, customer, selection, fragments)
      },
      selected_field_options: SelectedFieldOptions(
        include_inline_fragments: True,
      ),
      page_info_options: page_info_options,
    ),
  )
}

@internal
pub fn list_customers_for_connection(
  store: Store,
  connection: CustomerCatalogConnectionRecord,
) -> List(CustomerRecord) {
  connection.ordered_customer_ids
  |> list.filter_map(fn(id) {
    case store.get_effective_customer_by_id(store, id) {
      Some(customer) -> Ok(customer)
      None -> Error(Nil)
    }
  })
}

@internal
pub fn resolve_customer_connection_cursor(
  customer_id: String,
  connection: CustomerCatalogConnectionRecord,
) -> String {
  case dict.get(connection.cursor_by_customer_id, customer_id) {
    Ok(cursor) -> cursor
    Error(_) -> customer_id
  }
}

@internal
pub fn filter_customers(
  customers: List(CustomerRecord),
  query: Option(String),
) -> List(CustomerRecord) {
  case query {
    None -> customers
    Some(q) -> {
      let trimmed = string.trim(q)
      case
        trimmed == ""
        || string.starts_with(trimmed, "__customer_parity_no_match__")
      {
        True ->
          case string.starts_with(trimmed, "__customer_parity_no_match__") {
            True -> []
            False -> customers
          }
        False -> {
          search_query_parser.apply_search_query_terms(
            customers,
            Some(trimmed),
            search_query_parser.default_term_list_options(),
            customer_matches_search_term,
          )
        }
      }
    }
  }
}

@internal
pub fn customer_matches_search_term(
  customer: CustomerRecord,
  term: search_query_parser.SearchQueryTerm,
) -> Bool {
  case term.field {
    Some("email") ->
      search_query_parser.matches_search_query_string(
        customer.email,
        term.value,
        search_query_parser.IncludesMatch,
        search_query_parser.default_string_match_options(),
      )
    Some("tag") ->
      list.any(customer.tags, fn(tag) {
        search_query_parser.matches_search_query_string(
          Some(tag),
          term.value,
          search_query_parser.ExactMatch,
          search_query_parser.default_string_match_options(),
        )
      })
    Some("state") ->
      search_query_parser.matches_search_query_string(
        customer.state,
        term.value,
        search_query_parser.ExactMatch,
        search_query_parser.default_string_match_options(),
      )
    _ ->
      search_query_parser.matches_search_query_text(
        Some(customer_text(customer)),
        term,
      )
  }
}

@internal
pub fn customer_text(customer: CustomerRecord) -> String {
  string.join(
    [
      option.unwrap(customer.email, ""),
      option.unwrap(customer.first_name, ""),
      option.unwrap(customer.last_name, ""),
      option.unwrap(customer.display_name, ""),
      string.join(customer.tags, " "),
    ],
    " ",
  )
}

@internal
pub fn sort_customers(
  customers: List(CustomerRecord),
  sort_key: Option(String),
  reverse: Bool,
) -> List(CustomerRecord) {
  let sorted = case sort_key {
    Some("NAME") ->
      list.sort(customers, fn(a, b) {
        string.compare(
          option.unwrap(a.display_name, ""),
          option.unwrap(b.display_name, ""),
        )
      })
    Some("ID") -> list.sort(customers, fn(a, b) { string.compare(a.id, b.id) })
    _ ->
      list.sort(customers, fn(a, b) {
        case
          string.compare(
            option.unwrap(b.updated_at, ""),
            option.unwrap(a.updated_at, ""),
          )
        {
          order.Eq -> string.compare(b.id, a.id)
          other -> other
        }
      })
  }
  case reverse {
    True -> list.reverse(sorted)
    False -> sorted
  }
}

/// `customersCount` — in `LiveHybrid` mode the dispatcher passes the
/// request straight through to upstream (see
/// `live_hybrid_passthrough_target` in `draft_proxy.gleam`); the local
/// store can't know the real total. This handler only runs in
/// `Snapshot` mode and returns the count of effective local customers
/// (typically zero against an empty store).
@internal
pub fn serialize_customers_count(
  proxy: DraftProxy,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> Json {
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      case proxy.config.read_mode {
        LiveHybrid ->
          case fetch_customers_count_source(field, variables, upstream) {
            Some(source) ->
              project_graphql_value(source, selections, dict.new())
            None ->
              project_graphql_value(
                local_customers_count_source(proxy),
                selections,
                dict.new(),
              )
          }
        _ ->
          project_graphql_value(
            local_customers_count_source(proxy),
            selections,
            dict.new(),
          )
      }
    _ -> json.object([])
  }
}

@internal
pub fn local_customers_count_source(proxy: DraftProxy) -> SourceValue {
  let count = store.list_effective_customers(proxy.store) |> list.length()
  src_object([#("count", SrcInt(count)), #("precision", SrcString("EXACT"))])
}

@internal
pub fn fetch_customers_count_source(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> Option(SourceValue) {
  let args = graphql_helpers.field_args(field, variables)
  let query_arg = graphql_helpers.read_arg_string_nonempty(args, "query")
  let query =
    "query CustomerCountHydrate($query: String) {
  customersCount(query: $query) { count precision }
}
"
  let upstream_variables = case query_arg {
    Some(query) -> json.object([#("query", json.string(query))])
    None -> json.object([])
  }
  case
    upstream_query.fetch_sync(
      upstream.origin,
      upstream.transport,
      upstream.headers,
      "CustomerCountHydrate",
      query,
      upstream_variables,
    )
  {
    Ok(value) -> customers_count_source_from_hydrate(value)
    Error(_) -> None
  }
}

@internal
pub fn customers_count_source_from_hydrate(
  value: commit.JsonValue,
) -> Option(SourceValue) {
  use data <- option.then(json_get(value, "data"))
  use count_node <- option.then(non_null_json(json_get(data, "customersCount")))
  use count <- option.then(json_get_int(count_node, "count"))
  let precision =
    json_get_string(count_node, "precision")
    |> option.unwrap("EXACT")
  Some(
    src_object([
      #("count", SrcInt(count)),
      #("precision", SrcString(precision)),
    ]),
  )
}

@internal
pub fn serialize_customer_account_page(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string_nonempty(args, "id") {
    Some(id) ->
      case store.get_effective_customer_account_page_by_id(store, id) {
        Some(page) -> project_account_page(page, field, fragments)
        None -> json.null()
      }
    None -> json.null()
  }
}

@internal
pub fn serialize_customer_account_pages(
  proxy: DraftProxy,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> Json {
  let local_pages = store.list_effective_customer_account_pages(proxy.store)
  let pages = case local_pages, proxy.config.read_mode {
    [], LiveHybrid ->
      fetch_customer_account_pages(upstream) |> option.unwrap([])
    _, _ -> local_pages
  }
  let cursor_value = fn(page: CustomerAccountPageRecord, _index) {
    page.cursor |> option.unwrap(page.default_cursor)
  }
  let window =
    paginate_connection_items(
      pages,
      field,
      variables,
      cursor_value,
      default_connection_window_options(),
    )
  let ConnectionWindow(
    items: page_items,
    has_next_page: has_next,
    has_previous_page: has_prev,
  ) = window
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: page_items,
      has_next_page: has_next,
      has_previous_page: has_prev,
      get_cursor_value: cursor_value,
      serialize_node: fn(page, selection, _index) {
        project_account_page(page, selection, fragments)
      },
      selected_field_options: SelectedFieldOptions(
        include_inline_fragments: True,
      ),
      page_info_options: ConnectionPageInfoOptions(
        ..default_connection_page_info_options(),
        prefix_cursors: False,
      ),
    ),
  )
}

@internal
pub fn fetch_customer_account_pages(
  upstream: UpstreamContext,
) -> Option(List(CustomerAccountPageRecord)) {
  let query =
    "query CustomerAccountPagesHydrate {
  customerAccountPages(first: 250) {
    nodes { id title handle defaultCursor }
    pageInfo { startCursor endCursor }
  }
}
"
  case
    upstream_query.fetch_sync(
      upstream.origin,
      upstream.transport,
      upstream.headers,
      "CustomerAccountPagesHydrate",
      query,
      json.object([]),
    )
  {
    Ok(value) -> customer_account_pages_from_hydrate(value)
    Error(_) -> None
  }
}

@internal
pub fn account_page_source(page: CustomerAccountPageRecord) {
  src_object([
    #("__typename", SrcString("CustomerAccountPage")),
    #("id", SrcString(page.id)),
    #("title", SrcString(page.title)),
    #("handle", SrcString(page.handle)),
    #("defaultCursor", SrcString(page.default_cursor)),
  ])
}

@internal
pub fn project_account_page(
  page,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      project_graphql_value(account_page_source(page), selections, fragments)
    _ -> json.object([])
  }
}

@internal
pub fn serialize_store_credit_account_by_id(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string_nonempty(args, "id") {
    Some(id) ->
      case store.get_effective_store_credit_account_by_id(store, id) {
        Some(account) ->
          case field {
            Field(
              selection_set: Some(SelectionSet(selections: selections, ..)),
              ..,
            ) ->
              project_graphql_value(
                store_credit_account_source(store, account),
                selections,
                fragments,
              )
            _ -> json.object([])
          }
        None -> json.null()
      }
    None -> json.null()
  }
}

@internal
pub fn serialize_customer_payment_method_by_id(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  let show_revoked =
    graphql_helpers.read_arg_bool(args, "showRevoked") |> option.unwrap(False)
  case graphql_helpers.read_arg_string_nonempty(args, "id") {
    Some(id) ->
      case
        store.get_effective_customer_payment_method_by_id(
          store,
          id,
          show_revoked,
        )
      {
        Some(method) ->
          case field {
            Field(
              selection_set: Some(SelectionSet(selections: selections, ..)),
              ..,
            ) ->
              project_graphql_value(
                payment_method_source(store, method),
                selections,
                fragments,
              )
            _ -> json.object([])
          }
        None -> json.null()
      }
    None -> json.null()
  }
}

@internal
pub fn serialize_customer_merge_preview(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  let one = graphql_helpers.read_arg_string_nonempty(args, "customerOneId")
  let two = graphql_helpers.read_arg_string_nonempty(args, "customerTwoId")
  case one, two {
    Some(one_id), Some(two_id) ->
      case
        store.get_effective_customer_by_id(store, one_id),
        store.get_effective_customer_by_id(store, two_id)
      {
        Some(c1), Some(c2) -> {
          let merged =
            build_merged_customer(
              c1,
              c2,
              dict.new(),
              option.unwrap(c2.updated_at, ""),
            )
          project_merge_field_set(store, merged, field, fragments)
        }
        _, _ -> json.null()
      }
    _, _ -> json.null()
  }
}

@internal
pub fn serialize_customer_merge_job_status(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string_nonempty(args, "jobId") {
    Some(job_id) ->
      case store.get_customer_merge_request(store, job_id) {
        Some(request) -> project_merge_request(request, field, fragments)
        None -> json.null()
      }
    None -> json.null()
  }
}

@internal
pub fn project_merge_field_set(
  store: Store,
  customer: CustomerRecord,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      project_graphql_value(
        src_object([
          #("__typename", SrcString("CustomerMergePreview")),
          #("customer", customer_to_source(store, customer)),
          #("customerMergeErrors", SrcList([])),
        ]),
        selections,
        fragments,
      )
    _ -> json.object([])
  }
}

@internal
pub fn project_merge_request(
  request: CustomerMergeRequestRecord,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      project_graphql_value(
        merge_request_source(request),
        selections,
        fragments,
      )
    _ -> json.object([])
  }
}

@internal
pub fn merge_request_source(
  request: CustomerMergeRequestRecord,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("CustomerMergeRequest")),
    #("jobId", SrcString(request.job_id)),
    #("resultingCustomerId", SrcString(request.resulting_customer_id)),
    #("status", SrcString(request.status)),
    #("job", job_source(request.job_id, request.status)),
    #("userErrors", SrcList([])),
    #(
      "customerMergeErrors",
      SrcList(
        list.map(request.customer_merge_errors, fn(err) {
          src_object([
            #("errorFields", SrcList(list.map(err.error_fields, SrcString))),
            #("field", SrcList(list.map(err.error_fields, SrcString))),
            #("message", SrcString(err.message)),
            #("code", graphql_helpers.option_string_source(err.code)),
            #(
              "block_type",
              graphql_helpers.option_string_source(err.block_type),
            ),
            #("blockType", graphql_helpers.option_string_source(err.block_type)),
          ])
        }),
      ),
    ),
  ])
}

@internal
pub fn job_source(job_id: String, status: String) -> SourceValue {
  src_object([
    #("__typename", SrcString("Job")),
    #("id", SrcString(job_id)),
    #("done", SrcBool(status != "IN_PROGRESS")),
  ])
}

@internal
pub fn customer_payload_json(
  store: Store,
  typename: String,
  customer: Option(CustomerRecord),
  deleted_customer_id: Option(String),
  customer_address: Option(CustomerAddressRecord),
  user_errors: List(UserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      project_graphql_value(
        src_object([
          #("__typename", SrcString(typename)),
          #("customer", case customer {
            Some(c) -> customer_to_source(store, c)
            None -> SrcNull
          }),
          #(
            "deletedCustomerId",
            graphql_helpers.option_string_source(deleted_customer_id),
          ),
          #("address", case customer_address {
            Some(a) -> address_source(a)
            None -> SrcNull
          }),
          #("shop", src_object([#("id", SrcString("gid://shopify/Shop/1"))])),
          #("userErrors", SrcList(list.map(user_errors, user_error_source))),
        ]),
        selections,
        fragments,
      )
    _ -> json.object([])
  }
}

@internal
pub fn address_payload_json(
  _store: Store,
  typename: String,
  address: Option(CustomerAddressRecord),
  deleted_address_id: Option(String),
  user_errors: List(UserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      project_graphql_value(
        src_object([
          #("__typename", SrcString(typename)),
          #("address", case address {
            Some(a) -> address_source(a)
            None -> SrcNull
          }),
          #(
            "deletedAddressId",
            graphql_helpers.option_string_source(deleted_address_id),
          ),
          #("userErrors", SrcList(list.map(user_errors, user_error_source))),
        ]),
        selections,
        fragments,
      )
    _ -> json.object([])
  }
}

@internal
pub fn address_unknown_result(
  store,
  identity,
  field,
  fragments,
  typename,
  root,
) {
  let payload =
    address_payload_json(
      store,
      typename,
      None,
      None,
      [
        UserError(
          ["customerId"],
          "Customer does not exist",
          Some("CUSTOMER_DOES_NOT_EXIST"),
        ),
      ],
      field,
      fragments,
    )
  #(
    MutationFieldResult(get_field_response_key(field), payload, [], root),
    store,
    identity,
  )
}

@internal
pub fn address_customer_missing_result(
  store,
  identity,
  field,
  fragments,
  typename,
  root,
) {
  let payload =
    address_payload_json(
      store,
      typename,
      None,
      None,
      [
        UserError(
          ["customerId"],
          "Customer does not exist",
          Some("CUSTOMER_DOES_NOT_EXIST"),
        ),
      ],
      field,
      fragments,
    )
  #(
    MutationFieldResult(get_field_response_key(field), payload, [], root),
    store,
    identity,
  )
}

@internal
pub fn address_ownership_result(
  store,
  identity,
  field,
  fragments,
  typename,
  root,
) {
  let payload =
    address_payload_json(
      store,
      typename,
      None,
      None,
      [UserError(["addressId"], "Address does not exist", None)],
      field,
      fragments,
    )
  #(
    MutationFieldResult(get_field_response_key(field), payload, [], root),
    store,
    identity,
  )
}

@internal
pub fn address_id_mismatch_result(
  store,
  identity,
  field,
  fragments,
  typename,
  root,
) {
  let payload =
    address_payload_json(
      store,
      typename,
      None,
      None,
      [
        UserError(
          ["addressId"],
          "The id of the address does not match the id in the input",
          None,
        ),
      ],
      field,
      fragments,
    )
  #(
    MutationFieldResult(get_field_response_key(field), payload, [], root),
    store,
    identity,
  )
}

@internal
pub fn customer_address_ownership_result(
  store,
  identity,
  field,
  fragments,
  typename,
  root,
  customer,
) {
  let payload =
    customer_payload_json(
      store,
      typename,
      Some(customer),
      None,
      None,
      [UserError(["addressId"], "Address does not exist", None)],
      field,
      fragments,
    )
  #(
    MutationFieldResult(get_field_response_key(field), payload, [], root),
    store,
    identity,
  )
}

@internal
pub fn store_credit_payload_json(
  store,
  typename,
  transaction,
  user_errors,
  field,
  fragments,
) {
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      project_graphql_value(
        src_object([
          #("__typename", SrcString(typename)),
          #("storeCreditAccountTransaction", case transaction {
            Some(t) -> store_credit_transaction_source(store, t)
            None -> SrcNull
          }),
          #("userErrors", SrcList(list.map(user_errors, user_error_source))),
        ]),
        selections,
        fragments,
      )
    _ -> json.object([])
  }
}

@internal
pub fn order_customer_payload_json(
  store,
  typename,
  order,
  user_errors,
  field,
  fragments,
) {
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      project_graphql_value(
        src_object([
          #("__typename", SrcString(typename)),
          #("order", case order {
            Some(o) -> order_summary_source(store, o)
            None -> SrcNull
          }),
          #("userErrors", SrcList(list.map(user_errors, user_error_source))),
        ]),
        selections,
        fragments,
      )
    _ -> json.object([])
  }
}

@internal
pub fn merge_payload_json(request, field, fragments) {
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      project_graphql_value(
        merge_request_source(request),
        selections,
        fragments,
      )
    _ -> json.object([])
  }
}

@internal
pub fn merge_error_payload(field, fragments, error_field, message, code) {
  merge_errors_payload(
    field,
    fragments,
    [UserError(error_field, message, code)],
    [],
  )
}

@internal
pub fn merge_errors_payload(
  field: Selection,
  fragments: FragmentMap,
  user_errors: List(UserError),
  merge_errors: List(CustomerMergeErrorRecord),
) {
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      project_graphql_value(
        src_object([
          #("__typename", SrcString("CustomerMergePayload")),
          #("resultingCustomerId", SrcNull),
          #("job", SrcNull),
          #("userErrors", SrcList(list.map(user_errors, user_error_source))),
          #(
            "customerMergeErrors",
            SrcList(
              list.map(merge_errors, fn(err) {
                src_object([
                  #(
                    "errorFields",
                    SrcList(list.map(err.error_fields, SrcString)),
                  ),
                  #("field", SrcList(list.map(err.error_fields, SrcString))),
                  #("message", SrcString(err.message)),
                  #("code", graphql_helpers.option_string_source(err.code)),
                  #(
                    "block_type",
                    graphql_helpers.option_string_source(err.block_type),
                  ),
                  #(
                    "blockType",
                    graphql_helpers.option_string_source(err.block_type),
                  ),
                ])
              }),
            ),
          ),
        ]),
        selections,
        fragments,
      )
    _ -> json.object([])
  }
}

@internal
pub fn user_error_source(err: UserError) -> SourceValue {
  src_object([
    #("field", case err.field {
      [] -> SrcNull
      _ -> SrcList(list.map(err.field, SrcString))
    }),
    #("message", SrcString(err.message)),
    #("code", graphql_helpers.option_string_source(err.code)),
  ])
}

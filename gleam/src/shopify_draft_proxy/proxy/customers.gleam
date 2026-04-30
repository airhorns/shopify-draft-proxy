//// Customer domain port foundation.
////
//// Mirrors the normalized-state approach in `src/proxy/customers.ts`: customer
//// writes stage locally, downstream reads resolve from effective in-memory
//// state, and outbound/customer-visible side effects are represented as local
//// intent only.

import gleam/dict.{type Dict}
import gleam/float
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/order
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection, Field, SelectionSet}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, ConnectionPageInfoOptions,
  ConnectionWindow, SelectedFieldOptions, SerializeConnectionConfig, SrcBool,
  SrcInt, SrcList, SrcNull, SrcObject, SrcString,
  default_connection_page_info_options, default_connection_window_options,
  get_document_fragments, get_field_response_key, paginate_connection_items,
  project_graphql_value, serialize_connection, src_object,
}
import shopify_draft_proxy/proxy/payments
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type CustomerAccountPageRecord, type CustomerAddressRecord,
  type CustomerCatalogConnectionRecord, type CustomerCatalogPageInfoRecord,
  type CustomerDefaultAddressRecord, type CustomerDefaultEmailAddressRecord,
  type CustomerDefaultPhoneNumberRecord,
  type CustomerEmailMarketingConsentRecord, type CustomerEventSummaryRecord,
  type CustomerMergeRequestRecord, type CustomerMetafieldRecord,
  type CustomerOrderSummaryRecord, type CustomerPaymentMethodRecord,
  type CustomerRecord, type CustomerSmsMarketingConsentRecord, type Money,
  type StoreCreditAccountRecord, type StoreCreditAccountTransactionRecord,
  CustomerAddressRecord, CustomerCatalogPageInfoRecord,
  CustomerDefaultAddressRecord, CustomerDefaultEmailAddressRecord,
  CustomerDefaultPhoneNumberRecord, CustomerEmailMarketingConsentRecord,
  CustomerMergeRequestRecord, CustomerMetafieldRecord,
  CustomerOrderSummaryRecord, CustomerRecord, CustomerSmsMarketingConsentRecord,
  Money, StoreCreditAccountRecord, StoreCreditAccountTransactionRecord,
}

pub type CustomersError {
  ParseFailed(root_field.RootFieldError)
}

pub type MutationOutcome {
  MutationOutcome(
    data: Json,
    store: Store,
    identity: SyntheticIdentityRegistry,
    staged_resource_ids: List(String),
  )
}

type UserError {
  UserError(field: List(String), message: String, code: Option(String))
}

type MutationFieldResult {
  MutationFieldResult(
    key: String,
    payload: Json,
    staged_resource_ids: List(String),
    root_name: String,
  )
}

pub fn is_customer_query_root(name: String) -> Bool {
  case name {
    "customer"
    | "customers"
    | "customersCount"
    | "customerByIdentifier"
    | "customerAccountPage"
    | "customerAccountPages"
    | "customerMergePreview"
    | "customerMergeJobStatus"
    | "storeCreditAccount"
    | "customerPaymentMethod" -> True
    _ -> False
  }
}

pub fn is_customer_mutation_root(name: String) -> Bool {
  case name {
    "customerCreate"
    | "customerUpdate"
    | "customerDelete"
    | "customerSet"
    | "customerAddressCreate"
    | "customerAddressUpdate"
    | "customerAddressDelete"
    | "customerUpdateDefaultAddress"
    | "customerEmailMarketingConsentUpdate"
    | "customerSmsMarketingConsentUpdate"
    | "customerAddTaxExemptions"
    | "customerRemoveTaxExemptions"
    | "customerReplaceTaxExemptions"
    | "customerMerge"
    | "customerRequestDataErasure"
    | "customerCancelDataErasure"
    | "customerGenerateAccountActivationUrl"
    | "customerSendAccountInviteEmail"
    | "customerPaymentMethodSendUpdateEmail"
    | "storeCreditAccountCredit"
    | "storeCreditAccountDebit"
    | "orderCustomerSet"
    | "orderCustomerRemove"
    | "dataSaleOptOut" -> True
    _ -> False
  }
}

pub fn handle_customer_query(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, CustomersError) {
  case root_field.get_root_fields(document) {
    Error(err) -> Error(ParseFailed(err))
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      Ok(serialize_root_fields(store, fields, fragments, variables))
    }
  }
}

pub fn wrap_data(data: Json) -> Json {
  json.object([#("data", data)])
}

pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, CustomersError) {
  case root_field.get_root_fields(document) {
    Error(err) -> Error(ParseFailed(err))
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      let data = serialize_root_fields(store, fields, fragments, variables)
      let search_extensions =
        customer_count_search_extensions(fields, variables)
      Ok(wrap_query_payload(data, search_extensions))
    }
  }
}

fn wrap_query_payload(data: Json, search_extensions: List(Json)) -> Json {
  case search_extensions {
    [] -> wrap_data(data)
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

fn customer_count_search_extensions(
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
              read_arg_string(field_args(field, variables), "query"),
              get_field_response_key(field),
            )
          _ -> Error(Nil)
        }
      _ -> Error(Nil)
    }
  })
}

fn build_customers_count_search_extension(
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

fn option_to_result(value: Option(a)) -> Result(a, Nil) {
  case value {
    Some(v) -> Ok(v)
    None -> Error(Nil)
  }
}

fn serialize_root_fields(
  store: Store,
  fields: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  json.object(
    list.map(fields, fn(field) {
      #(
        get_field_response_key(field),
        root_payload_for_field(store, field, fragments, variables),
      )
    }),
  )
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
        "customer" ->
          serialize_customer_by_id(store, field, fragments, variables)
        "customerByIdentifier" ->
          serialize_customer_by_identifier(store, field, fragments, variables)
        "customers" ->
          serialize_customers_connection(store, field, fragments, variables)
        "customersCount" -> serialize_customers_count(store, field, variables)
        "customerAccountPage" ->
          serialize_customer_account_page(store, field, fragments, variables)
        "customerAccountPages" ->
          serialize_customer_account_pages(store, field, fragments, variables)
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

fn field_args(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> Dict(String, root_field.ResolvedValue) {
  case root_field.get_field_arguments(field, variables) {
    Ok(d) -> d
    Error(_) -> dict.new()
  }
}

fn read_arg_string(
  args: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(String) {
  case dict.get(args, name) {
    Ok(root_field.StringVal(s)) ->
      case s {
        "" -> None
        _ -> Some(s)
      }
    _ -> None
  }
}

fn read_arg_bool(
  args: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(Bool) {
  case dict.get(args, name) {
    Ok(root_field.BoolVal(b)) -> Some(b)
    _ -> None
  }
}

fn read_arg_object(
  args: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(Dict(String, root_field.ResolvedValue)) {
  case dict.get(args, name) {
    Ok(root_field.ObjectVal(d)) -> Some(d)
    _ -> None
  }
}

fn read_obj_string(
  obj: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(String) {
  case dict.get(obj, name) {
    Ok(root_field.StringVal(s)) ->
      case s {
        "" -> None
        _ -> Some(s)
      }
    Ok(root_field.IntVal(i)) -> Some(int.to_string(i))
    _ -> None
  }
}

fn read_normalized_optional_string(
  obj: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(String) {
  read_obj_string(obj, name)
  |> option.then(fn(value) {
    case string.trim(value) {
      "" -> None
      trimmed -> Some(trimmed)
    }
  })
}

fn read_normalized_string_with_blank(
  obj: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(String) {
  case dict.get(obj, name) {
    Ok(root_field.StringVal(value)) -> Some(string.trim(value))
    _ -> None
  }
}

fn read_obj_bool(
  obj: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(Bool) {
  case dict.get(obj, name) {
    Ok(root_field.BoolVal(b)) -> Some(b)
    _ -> None
  }
}

fn read_obj_array_strings(
  obj: Dict(String, root_field.ResolvedValue),
  name: String,
) -> List(String) {
  case dict.get(obj, name) {
    Ok(root_field.ListVal(items)) ->
      list.filter_map(items, fn(item) {
        case item {
          root_field.StringVal(s) -> Ok(s)
          _ -> Error(Nil)
        }
      })
    Ok(root_field.StringVal(s)) -> split_tags(s)
    _ -> []
  }
}

fn update_trimmed_nullable_string(
  existing: Option(String),
  input: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(String) {
  case dict.get(input, name) {
    Ok(root_field.StringVal(value)) -> {
      let trimmed = string.trim(value)
      case trimmed {
        "" -> None
        _ -> Some(trimmed)
      }
    }
    Ok(root_field.NullVal) -> None
    _ -> existing
  }
}

fn update_nullable_note(
  existing: Option(String),
  input: Dict(String, root_field.ResolvedValue),
) -> Option(String) {
  case dict.get(input, "note") {
    Ok(root_field.StringVal(value)) -> Some(value)
    Ok(root_field.NullVal) -> None
    _ -> existing
  }
}

fn input_object(
  args: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Dict(String, root_field.ResolvedValue) {
  read_arg_object(args, name) |> option.unwrap(dict.new())
}

fn optional_string_source(value: Option(String)) -> SourceValue {
  case value {
    Some(s) -> SrcString(s)
    None -> SrcNull
  }
}

fn optional_bool_source(value: Option(Bool)) -> SourceValue {
  case value {
    Some(b) -> SrcBool(b)
    None -> SrcNull
  }
}

fn money_source(value: Money) -> SourceValue {
  src_object([
    #("__typename", SrcString("MoneyV2")),
    #("amount", SrcString(value.amount)),
    #("currencyCode", SrcString(value.currency_code)),
  ])
}

fn default_email_source(
  value: Option(CustomerDefaultEmailAddressRecord),
) -> SourceValue {
  case value {
    Some(v) ->
      src_object([
        #("emailAddress", optional_string_source(v.email_address)),
        #("marketingState", optional_string_source(v.marketing_state)),
        #(
          "marketingOptInLevel",
          optional_string_source(v.marketing_opt_in_level),
        ),
        #("marketingUpdatedAt", optional_string_source(v.marketing_updated_at)),
      ])
    None -> SrcNull
  }
}

fn default_phone_source(
  value: Option(CustomerDefaultPhoneNumberRecord),
) -> SourceValue {
  case value {
    Some(v) ->
      src_object([
        #("phoneNumber", optional_string_source(v.phone_number)),
        #("marketingState", optional_string_source(v.marketing_state)),
        #(
          "marketingOptInLevel",
          optional_string_source(v.marketing_opt_in_level),
        ),
        #("marketingUpdatedAt", optional_string_source(v.marketing_updated_at)),
        #(
          "marketingCollectedFrom",
          optional_string_source(v.marketing_collected_from),
        ),
      ])
    None -> SrcNull
  }
}

fn email_consent_source(
  value: Option(CustomerEmailMarketingConsentRecord),
) -> SourceValue {
  case value {
    Some(v) ->
      src_object([
        #("marketingState", optional_string_source(v.marketing_state)),
        #(
          "marketingOptInLevel",
          optional_string_source(v.marketing_opt_in_level),
        ),
        #("consentUpdatedAt", optional_string_source(v.consent_updated_at)),
      ])
    None -> SrcNull
  }
}

fn sms_consent_source(
  value: Option(CustomerSmsMarketingConsentRecord),
) -> SourceValue {
  case value {
    Some(v) ->
      src_object([
        #("marketingState", optional_string_source(v.marketing_state)),
        #(
          "marketingOptInLevel",
          optional_string_source(v.marketing_opt_in_level),
        ),
        #("consentUpdatedAt", optional_string_source(v.consent_updated_at)),
        #(
          "consentCollectedFrom",
          optional_string_source(v.consent_collected_from),
        ),
      ])
    None -> SrcNull
  }
}

fn address_to_default(
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

fn default_address_source(
  value: Option(CustomerDefaultAddressRecord),
) -> SourceValue {
  case value {
    Some(v) ->
      src_object([
        #("__typename", SrcString("MailingAddress")),
        #("id", optional_string_source(v.id)),
        #("firstName", optional_string_source(v.first_name)),
        #("lastName", optional_string_source(v.last_name)),
        #("address1", optional_string_source(v.address1)),
        #("address2", optional_string_source(v.address2)),
        #("city", optional_string_source(v.city)),
        #("company", optional_string_source(v.company)),
        #("province", optional_string_source(v.province)),
        #("provinceCode", optional_string_source(v.province_code)),
        #("country", optional_string_source(v.country)),
        #("countryCodeV2", optional_string_source(v.country_code_v2)),
        #("zip", optional_string_source(v.zip)),
        #("phone", optional_string_source(v.phone)),
        #("name", optional_string_source(v.name)),
        #("formattedArea", optional_string_source(v.formatted_area)),
      ])
    None -> SrcNull
  }
}

fn address_source(address: CustomerAddressRecord) -> SourceValue {
  default_address_source(Some(address_to_default(address)))
}

fn customer_to_source(store: Store, customer: CustomerRecord) -> SourceValue {
  let customer_metafields =
    store.get_effective_metafields_by_customer_id(store, customer.id)
  src_object([
    #("__typename", SrcString("Customer")),
    #("id", SrcString(customer.id)),
    #("firstName", optional_string_source(customer.first_name)),
    #("lastName", optional_string_source(customer.last_name)),
    #("displayName", optional_string_source(customer.display_name)),
    #("email", optional_string_source(customer.email)),
    #("legacyResourceId", optional_string_source(customer.legacy_resource_id)),
    #("locale", optional_string_source(customer.locale)),
    #("note", optional_string_source(customer.note)),
    #("canDelete", optional_bool_source(customer.can_delete)),
    #("verifiedEmail", optional_bool_source(customer.verified_email)),
    #("dataSaleOptOut", SrcBool(customer.data_sale_opt_out)),
    #("taxExempt", optional_bool_source(customer.tax_exempt)),
    #("taxExemptions", SrcList(list.map(customer.tax_exemptions, SrcString))),
    #("state", optional_string_source(customer.state)),
    #("tags", SrcList(list.map(customer.tags, SrcString))),
    #("numberOfOrders", optional_string_source(customer.number_of_orders)),
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
    #("createdAt", optional_string_source(customer.created_at)),
    #("updatedAt", optional_string_source(customer.updated_at)),
  ])
}

fn connection_source(
  nodes: List(SourceValue),
  cursors: List(String),
) -> SourceValue {
  connection_source_with_page_info(nodes, cursors, None)
}

fn connection_source_with_page_info(
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

fn empty_connection_source() -> SourceValue {
  connection_source([], [])
}

fn page_info_source(
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
          optional_string_source(
            info.start_cursor |> option.or(source_string_option(start)),
          ),
        ),
        #(
          "endCursor",
          optional_string_source(
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

fn source_string_option(value: SourceValue) -> Option(String) {
  case value {
    SrcString(s) -> Some(s)
    _ -> None
  }
}

fn zip_nodes(
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

fn address_connection_source(store: Store, customer_id: String) -> SourceValue {
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

fn first_address_cursor(
  addresses: List(CustomerAddressRecord),
) -> Option(String) {
  case addresses {
    [first, ..] -> first.cursor
    [] -> None
  }
}

fn last_address_cursor(
  addresses: List(CustomerAddressRecord),
) -> Option(String) {
  case list.last(addresses) {
    Ok(last) -> last.cursor
    Error(_) -> None
  }
}

fn order_summary_source(
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
    #("name", optional_string_source(order.name)),
    #("email", optional_string_source(order.email)),
    #("createdAt", optional_string_source(order.created_at)),
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

fn orders_connection_source(store: Store, customer_id: String) -> SourceValue {
  let orders = store.list_effective_customer_order_summaries(store, customer_id)
  connection_source_with_page_info(
    list.map(orders, fn(order) { order_summary_source(store, order) }),
    list.map(orders, fn(order) {
      order.cursor |> option.unwrap("cursor:" <> order.id)
    }),
    store.get_effective_customer_order_connection_page_info(store, customer_id),
  )
}

fn event_summary_source(event: CustomerEventSummaryRecord) -> SourceValue {
  src_object([
    #("__typename", SrcString("BasicEvent")),
    #("id", SrcString(event.id)),
  ])
}

fn events_connection_source(store: Store, customer_id: String) -> SourceValue {
  let events = store.list_effective_customer_event_summaries(store, customer_id)
  connection_source_with_page_info(
    list.map(events, event_summary_source),
    list.map(events, fn(event) {
      event.cursor |> option.unwrap("cursor:" <> event.id)
    }),
    store.get_effective_customer_event_connection_page_info(store, customer_id),
  )
}

fn metafield_source(metafield: CustomerMetafieldRecord) -> SourceValue {
  src_object([
    #("__typename", SrcString("Metafield")),
    #("id", SrcString(metafield.id)),
    #("namespace", SrcString(metafield.namespace)),
    #("key", SrcString(metafield.key)),
    #("type", SrcString(metafield.type_)),
    #("value", SrcString(metafield.value)),
    #("compareDigest", optional_string_source(metafield.compare_digest)),
    #("createdAt", optional_string_source(metafield.created_at)),
    #("updatedAt", optional_string_source(metafield.updated_at)),
  ])
}

fn metafield_connection_source_from(
  metafields: List(CustomerMetafieldRecord),
) -> SourceValue {
  connection_source(
    list.map(metafields, metafield_source),
    list.map(metafields, fn(m) { "cursor:" <> m.id }),
  )
}

fn store_credit_account_source(
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

fn store_credit_account_shallow_source(
  store: Store,
  account: StoreCreditAccountRecord,
) -> SourceValue {
  let owner = case
    store.get_effective_customer_by_id(store, account.customer_id)
  {
    Some(customer) -> customer_owner_source(customer)
    None -> SrcNull
  }
  src_object([
    #("__typename", SrcString("StoreCreditAccount")),
    #("id", SrcString(account.id)),
    #("balance", money_source(account.balance)),
    #("owner", owner),
  ])
}

fn store_credit_accounts_connection_source(
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

fn store_credit_transaction_source(
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

fn store_credit_transactions_connection_source(
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

fn payment_method_source(
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
    #("revokedAt", optional_string_source(payment_method.revoked_at)),
    #("revokedReason", optional_string_source(payment_method.revoked_reason)),
    #("subscriptionContracts", empty_connection_source()),
  ])
}

fn customer_owner_source(customer: CustomerRecord) -> SourceValue {
  src_object([
    #("__typename", SrcString("Customer")),
    #("id", SrcString(customer.id)),
    #("firstName", optional_string_source(customer.first_name)),
    #("lastName", optional_string_source(customer.last_name)),
    #("displayName", optional_string_source(customer.display_name)),
    #("email", optional_string_source(customer.email)),
    #(
      "defaultEmailAddress",
      default_email_source(customer.default_email_address),
    ),
    #("defaultPhoneNumber", default_phone_source(customer.default_phone_number)),
  ])
}

fn payment_methods_connection_source(
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

fn project_customer(
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

fn serialize_customer_by_id(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = field_args(field, variables)
  case read_arg_string(args, "id") {
    Some(id) ->
      case store.get_effective_customer_by_id(store, id) {
        Some(customer) -> project_customer(store, customer, field, fragments)
        None -> json.null()
      }
    None -> json.null()
  }
}

fn serialize_customer_by_identifier(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = field_args(field, variables)
  let identifier = input_object(args, "identifier")
  case find_customer_by_identifier(store, identifier) {
    Some(customer) -> project_customer(store, customer, field, fragments)
    None -> json.null()
  }
}

fn find_customer_by_identifier(
  store: Store,
  identifier: Dict(String, root_field.ResolvedValue),
) -> Option(CustomerRecord) {
  case read_obj_string(identifier, "id") {
    Some(id) -> store.get_effective_customer_by_id(store, id)
    None -> {
      let email = read_obj_string(identifier, "emailAddress")
      let phone = read_obj_string(identifier, "phoneNumber")
      find_customer_by_email_or_phone(
        store.list_effective_customers(store),
        email,
        phone,
      )
    }
  }
}

fn find_customer_by_email_or_phone(
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

fn serialize_customers_connection(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = field_args(field, variables)
  let query = read_arg_string(args, "query")
  let sort_key = read_arg_string(args, "sortKey")
  let reverse = read_arg_bool(args, "reverse") |> option.unwrap(False)
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

fn list_customers_for_connection(
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

fn resolve_customer_connection_cursor(
  customer_id: String,
  connection: CustomerCatalogConnectionRecord,
) -> String {
  case dict.get(connection.cursor_by_customer_id, customer_id) {
    Ok(cursor) -> cursor
    Error(_) -> customer_id
  }
}

fn filter_customers(
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
          let needle =
            trimmed
            |> string.replace("email:", "")
            |> string.replace("\"", "")
            |> string.replace("'", "")
            |> string.lowercase()
          list.filter(customers, fn(customer) {
            customer_text(customer)
            |> string.lowercase()
            |> string.contains(needle)
          })
        }
      }
    }
  }
}

fn customer_text(customer: CustomerRecord) -> String {
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

fn sort_customers(
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

fn serialize_customers_count(
  store: Store,
  field: Selection,
  _variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let count =
    store.list_effective_customers(store)
    |> list.length()
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      project_graphql_value(
        src_object([
          #("count", SrcInt(count)),
          #("precision", SrcString("EXACT")),
        ]),
        selections,
        dict.new(),
      )
    _ -> json.object([])
  }
}

fn serialize_customer_account_page(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = field_args(field, variables)
  case read_arg_string(args, "id") {
    Some(id) ->
      case store.get_effective_customer_account_page_by_id(store, id) {
        Some(page) -> project_account_page(page, field, fragments)
        None -> json.null()
      }
    None -> json.null()
  }
}

fn serialize_customer_account_pages(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let pages = store.list_effective_customer_account_pages(store)
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

fn account_page_source(page: CustomerAccountPageRecord) {
  src_object([
    #("__typename", SrcString("CustomerAccountPage")),
    #("id", SrcString(page.id)),
    #("title", SrcString(page.title)),
    #("handle", SrcString(page.handle)),
    #("defaultCursor", SrcString(page.default_cursor)),
  ])
}

fn project_account_page(
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

fn serialize_store_credit_account_by_id(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = field_args(field, variables)
  case read_arg_string(args, "id") {
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

fn serialize_customer_payment_method_by_id(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = field_args(field, variables)
  let show_revoked = read_arg_bool(args, "showRevoked") |> option.unwrap(False)
  case read_arg_string(args, "id") {
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

fn serialize_customer_merge_preview(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = field_args(field, variables)
  let one = read_arg_string(args, "customerOneId")
  let two = read_arg_string(args, "customerTwoId")
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

fn serialize_customer_merge_job_status(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = field_args(field, variables)
  case read_arg_string(args, "jobId") {
    Some(job_id) ->
      case store.get_customer_merge_request(store, job_id) {
        Some(request) -> project_merge_request(request, field, fragments)
        None -> json.null()
      }
    None -> json.null()
  }
}

fn project_merge_field_set(
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

fn project_merge_request(
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

fn merge_request_source(request: CustomerMergeRequestRecord) -> SourceValue {
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
            #("message", SrcString(err.message)),
          ])
        }),
      ),
    ),
  ])
}

fn job_source(job_id: String, status: String) -> SourceValue {
  src_object([
    #("__typename", SrcString("Job")),
    #("id", SrcString(job_id)),
    #("done", SrcBool(status != "IN_PROGRESS")),
  ])
}

pub fn process_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(MutationOutcome, CustomersError) {
  case root_field.get_root_fields(document) {
    Error(err) -> Error(ParseFailed(err))
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      Ok(handle_mutation_fields(
        store,
        identity,
        request_path,
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
  document: String,
  fields: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationOutcome {
  case first_customer_merge_missing_argument_error(fields, variables) {
    Some(error_json) ->
      MutationOutcome(
        data: json.object([#("errors", json.array([error_json], fn(x) { x }))]),
        store: store,
        identity: identity,
        staged_resource_ids: [],
      )
    None ->
      case first_invalid_tax_exemption_error(fields, variables) {
        Some(error_json) ->
          MutationOutcome(
            data: json.object([
              #("errors", json.array([error_json], fn(x) { x })),
            ]),
            store: store,
            identity: identity,
            staged_resource_ids: [],
          )
        None ->
          handle_validated_mutation_fields(
            store,
            identity,
            request_path,
            document,
            fields,
            fragments,
            variables,
          )
      }
  }
}

fn first_customer_merge_missing_argument_error(
  fields: List(Selection),
  variables: Dict(String, root_field.ResolvedValue),
) -> Option(Json) {
  case fields {
    [] -> None
    [field, ..rest] ->
      case field {
        Field(name: name, ..) ->
          case name.value == "customerMerge" {
            True -> {
              let args = field_args(field, variables)
              let missing =
                [
                  #("customerOneId", read_arg_string(args, "customerOneId")),
                  #("customerTwoId", read_arg_string(args, "customerTwoId")),
                ]
                |> list.filter_map(fn(pair) {
                  case pair {
                    #(argument, None) -> Ok(argument)
                    #(_, Some(_)) -> Error(Nil)
                  }
                })
              case missing {
                [] ->
                  first_customer_merge_missing_argument_error(rest, variables)
                [_, ..] ->
                  Some(customer_merge_missing_argument_error(
                    name.value,
                    missing,
                  ))
              }
            }
            False ->
              first_customer_merge_missing_argument_error(rest, variables)
          }
        _ -> first_customer_merge_missing_argument_error(rest, variables)
      }
  }
}

fn customer_merge_missing_argument_error(
  field_name: String,
  missing_arguments: List(String),
) -> Json {
  let arguments = string.join(missing_arguments, ", ")
  json.object([
    #(
      "message",
      json.string(
        "Field '"
        <> field_name
        <> "' is missing required arguments: "
        <> arguments,
      ),
    ),
    #(
      "extensions",
      json.object([
        #("code", json.string("missingRequiredArguments")),
        #("className", json.string("Field")),
        #("name", json.string(field_name)),
        #("arguments", json.string(arguments)),
      ]),
    ),
  ])
}

fn handle_validated_mutation_fields(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  fields: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationOutcome {
  let initial = #([], store, identity, [], [])
  let #(entries, final_store, final_identity, staged_ids, roots) =
    list.fold(fields, initial, fn(acc, field) {
      let #(current_entries, current_store, current_identity, ids, root_names) =
        acc
      case field {
        Field(name: name, ..) -> {
          let handled =
            handle_mutation_field(
              current_store,
              current_identity,
              field,
              name.value,
              fragments,
              variables,
            )
          case handled {
            Some(#(result, next_store, next_identity)) -> #(
              list.append(current_entries, [#(result.key, result.payload)]),
              next_store,
              next_identity,
              list.append(ids, result.staged_resource_ids),
              list.append(root_names, [result.root_name]),
            )
            None -> acc
          }
        }
        _ -> acc
      }
    })
  let #(logged_store, logged_identity) = case staged_ids {
    [] -> #(final_store, final_identity)
    _ ->
      record_mutation_log(
        final_store,
        final_identity,
        request_path,
        document,
        staged_ids,
        roots,
      )
  }
  MutationOutcome(
    data: json.object([#("data", json.object(entries))]),
    store: logged_store,
    identity: logged_identity,
    staged_resource_ids: staged_ids,
  )
}

fn first_invalid_tax_exemption_error(
  fields: List(Selection),
  variables: Dict(String, root_field.ResolvedValue),
) -> Option(Json) {
  case fields {
    [] -> None
    [field, ..rest] ->
      case field {
        Field(name: name, ..) ->
          case is_tax_exemption_root(name.value) {
            True -> {
              let args = field_args(field, variables)
              case invalid_tax_exemption_value(args) {
                Some(value) ->
                  Some(invalid_tax_exemption_variable_error(value, 0, args))
                None -> first_invalid_tax_exemption_error(rest, variables)
              }
            }
            False -> first_invalid_tax_exemption_error(rest, variables)
          }
        _ -> first_invalid_tax_exemption_error(rest, variables)
      }
  }
}

fn is_tax_exemption_root(name: String) -> Bool {
  name == "customerAddTaxExemptions"
  || name == "customerRemoveTaxExemptions"
  || name == "customerReplaceTaxExemptions"
}

fn invalid_tax_exemption_value(
  args: Dict(String, root_field.ResolvedValue),
) -> Option(String) {
  case dict.get(args, "taxExemptions") {
    Ok(root_field.ListVal(items)) -> first_invalid_tax_exemption(items)
    _ -> None
  }
}

fn first_invalid_tax_exemption(
  items: List(root_field.ResolvedValue),
) -> Option(String) {
  case items {
    [] -> None
    [item, ..rest] ->
      case item {
        root_field.StringVal(value) ->
          case is_valid_tax_exemption(value) {
            True -> first_invalid_tax_exemption(rest)
            False -> Some(value)
          }
        _ -> Some("")
      }
  }
}

fn is_valid_tax_exemption(value: String) -> Bool {
  list.contains(valid_tax_exemptions(), value)
}

fn valid_tax_exemptions() -> List(String) {
  [
    "CA_STATUS_CARD_EXEMPTION",
    "CA_BC_RESELLER_EXEMPTION",
    "CA_MB_RESELLER_EXEMPTION",
    "CA_SK_RESELLER_EXEMPTION",
    "CA_DIPLOMAT_EXEMPTION",
    "CA_BC_COMMERCIAL_FISHERY_EXEMPTION",
    "CA_MB_COMMERCIAL_FISHERY_EXEMPTION",
    "CA_NS_COMMERCIAL_FISHERY_EXEMPTION",
    "CA_PE_COMMERCIAL_FISHERY_EXEMPTION",
    "CA_SK_COMMERCIAL_FISHERY_EXEMPTION",
    "CA_BC_PRODUCTION_AND_MACHINERY_EXEMPTION",
    "CA_SK_PRODUCTION_AND_MACHINERY_EXEMPTION",
    "CA_BC_SUB_CONTRACTOR_EXEMPTION",
    "CA_SK_SUB_CONTRACTOR_EXEMPTION",
    "CA_BC_CONTRACTOR_EXEMPTION",
    "CA_SK_CONTRACTOR_EXEMPTION",
    "CA_ON_PURCHASE_EXEMPTION",
    "CA_MB_FARMER_EXEMPTION",
    "CA_NS_FARMER_EXEMPTION",
    "CA_SK_FARMER_EXEMPTION",
    "EU_REVERSE_CHARGE_EXEMPTION_RULE",
    "US_AL_RESELLER_EXEMPTION",
    "US_AK_RESELLER_EXEMPTION",
    "US_AZ_RESELLER_EXEMPTION",
    "US_AR_RESELLER_EXEMPTION",
    "US_CA_RESELLER_EXEMPTION",
    "US_CO_RESELLER_EXEMPTION",
    "US_CT_RESELLER_EXEMPTION",
    "US_DE_RESELLER_EXEMPTION",
    "US_FL_RESELLER_EXEMPTION",
    "US_GA_RESELLER_EXEMPTION",
    "US_HI_RESELLER_EXEMPTION",
    "US_ID_RESELLER_EXEMPTION",
    "US_IL_RESELLER_EXEMPTION",
    "US_IN_RESELLER_EXEMPTION",
    "US_IA_RESELLER_EXEMPTION",
    "US_KS_RESELLER_EXEMPTION",
    "US_KY_RESELLER_EXEMPTION",
    "US_LA_RESELLER_EXEMPTION",
    "US_ME_RESELLER_EXEMPTION",
    "US_MD_RESELLER_EXEMPTION",
    "US_MA_RESELLER_EXEMPTION",
    "US_MI_RESELLER_EXEMPTION",
    "US_MN_RESELLER_EXEMPTION",
    "US_MS_RESELLER_EXEMPTION",
    "US_MO_RESELLER_EXEMPTION",
    "US_MT_RESELLER_EXEMPTION",
    "US_NE_RESELLER_EXEMPTION",
    "US_NV_RESELLER_EXEMPTION",
    "US_NH_RESELLER_EXEMPTION",
    "US_NJ_RESELLER_EXEMPTION",
    "US_NM_RESELLER_EXEMPTION",
    "US_NY_RESELLER_EXEMPTION",
    "US_NC_RESELLER_EXEMPTION",
    "US_ND_RESELLER_EXEMPTION",
    "US_OH_RESELLER_EXEMPTION",
    "US_OK_RESELLER_EXEMPTION",
    "US_OR_RESELLER_EXEMPTION",
    "US_PA_RESELLER_EXEMPTION",
    "US_RI_RESELLER_EXEMPTION",
    "US_SC_RESELLER_EXEMPTION",
    "US_SD_RESELLER_EXEMPTION",
    "US_TN_RESELLER_EXEMPTION",
    "US_TX_RESELLER_EXEMPTION",
    "US_UT_RESELLER_EXEMPTION",
    "US_VT_RESELLER_EXEMPTION",
    "US_VA_RESELLER_EXEMPTION",
    "US_WA_RESELLER_EXEMPTION",
    "US_WV_RESELLER_EXEMPTION",
    "US_WI_RESELLER_EXEMPTION",
    "US_WY_RESELLER_EXEMPTION",
    "US_DC_RESELLER_EXEMPTION",
  ]
}

fn invalid_tax_exemption_variable_error(
  value: String,
  index: Int,
  args: Dict(String, root_field.ResolvedValue),
) -> Json {
  let expected_values = string.join(valid_tax_exemptions(), ", ")
  let explanation =
    "Expected \"" <> value <> "\" to be one of: " <> expected_values
  let message =
    "Variable $taxExemptions of type [TaxExemption!]! was provided invalid value for "
    <> int.to_string(index)
    <> " ("
    <> explanation
    <> ")"
  json.object([
    #("message", json.string(message)),
    #(
      "extensions",
      json.object([
        #("code", json.string("INVALID_VARIABLE")),
        #("value", tax_exemptions_value_json(args)),
        #(
          "problems",
          json.array(
            [
              json.object([
                #("path", json.array([json.int(index)], fn(x) { x })),
                #("explanation", json.string(explanation)),
              ]),
            ],
            fn(x) { x },
          ),
        ),
      ]),
    ),
  ])
}

fn tax_exemptions_value_json(
  args: Dict(String, root_field.ResolvedValue),
) -> Json {
  case dict.get(args, "taxExemptions") {
    Ok(root_field.ListVal(items)) ->
      json.array(items, fn(item) {
        case item {
          root_field.StringVal(s) -> json.string(s)
          root_field.IntVal(i) -> json.int(i)
          root_field.BoolVal(b) -> json.bool(b)
          _ -> json.null()
        }
      })
    _ -> json.null()
  }
}

fn handle_mutation_field(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  root_name: String,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Option(#(MutationFieldResult, Store, SyntheticIdentityRegistry)) {
  case root_name {
    "customerCreate" ->
      Some(handle_customer_create(store, identity, field, fragments, variables))
    "customerUpdate" ->
      Some(handle_customer_update(store, identity, field, fragments, variables))
    "customerSet" ->
      Some(handle_customer_set(store, identity, field, fragments, variables))
    "customerDelete" ->
      Some(handle_customer_delete(store, identity, field, fragments, variables))
    "customerAddressCreate" ->
      Some(handle_customer_address_create(
        store,
        identity,
        field,
        fragments,
        variables,
      ))
    "customerAddressUpdate" ->
      Some(handle_customer_address_update(
        store,
        identity,
        field,
        fragments,
        variables,
      ))
    "customerAddressDelete" ->
      Some(handle_customer_address_delete(
        store,
        identity,
        field,
        fragments,
        variables,
      ))
    "customerUpdateDefaultAddress" ->
      Some(handle_customer_update_default_address(
        store,
        identity,
        field,
        fragments,
        variables,
      ))
    "customerAddTaxExemptions" ->
      Some(handle_customer_tax_exemptions(
        store,
        identity,
        field,
        fragments,
        variables,
        "add",
      ))
    "customerRemoveTaxExemptions" ->
      Some(handle_customer_tax_exemptions(
        store,
        identity,
        field,
        fragments,
        variables,
        "remove",
      ))
    "customerReplaceTaxExemptions" ->
      Some(handle_customer_tax_exemptions(
        store,
        identity,
        field,
        fragments,
        variables,
        "replace",
      ))
    "customerEmailMarketingConsentUpdate" ->
      Some(handle_email_consent(store, identity, field, fragments, variables))
    "customerSmsMarketingConsentUpdate" ->
      Some(handle_sms_consent(store, identity, field, fragments, variables))
    "customerRequestDataErasure" ->
      Some(handle_data_erasure(store, identity, field, variables, False))
    "customerCancelDataErasure" ->
      Some(handle_data_erasure(store, identity, field, variables, True))
    "customerGenerateAccountActivationUrl" ->
      Some(handle_activation_url(store, identity, field, variables))
    "customerSendAccountInviteEmail" ->
      Some(handle_account_invite(store, identity, field, fragments, variables))
    "customerPaymentMethodSendUpdateEmail" ->
      Some(handle_payment_method_update_email(
        store,
        identity,
        field,
        fragments,
        variables,
      ))
    "storeCreditAccountCredit" ->
      Some(handle_store_credit_adjustment(
        store,
        identity,
        field,
        fragments,
        variables,
        True,
      ))
    "storeCreditAccountDebit" ->
      Some(handle_store_credit_adjustment(
        store,
        identity,
        field,
        fragments,
        variables,
        False,
      ))
    "orderCustomerSet" ->
      Some(handle_order_customer_set(
        store,
        identity,
        field,
        fragments,
        variables,
      ))
    "orderCustomerRemove" ->
      Some(handle_order_customer_remove(
        store,
        identity,
        field,
        fragments,
        variables,
      ))
    "customerMerge" ->
      Some(handle_customer_merge(store, identity, field, fragments, variables))
    "dataSaleOptOut" ->
      Some(handle_data_sale_opt_out(store, identity, field, variables))
    _ -> None
  }
}

fn handle_customer_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let input = input_object(field_args(field, variables), "input")
  let errors = validate_customer_create(store, input)
  case errors {
    [] -> {
      let #(id, after_id) =
        synthetic_identity.make_synthetic_gid(identity, "Customer")
      let #(timestamp, after_ts) =
        synthetic_identity.make_synthetic_timestamp(after_id)
      let #(customer, address_records, metafields) =
        build_created_customer(id, timestamp, input, after_ts)
      let #(stored_customer, store_after_customer) =
        store.stage_create_customer(store, customer)
      let store_after_addresses =
        list.fold(address_records, store_after_customer, fn(acc, address) {
          let #(_, next_store) =
            store.stage_upsert_customer_address(acc, address)
          next_store
        })
      let store_after_metafields =
        store.stage_customer_metafields(
          store_after_addresses,
          stored_customer.id,
          metafields,
        )
      let payload =
        customer_payload_json(
          store_after_metafields,
          "CustomerCreatePayload",
          Some(stored_customer),
          None,
          None,
          [],
          field,
          fragments,
        )
      #(
        MutationFieldResult(
          key: get_field_response_key(field),
          payload: payload,
          staged_resource_ids: [stored_customer.id],
          root_name: "customerCreate",
        ),
        store_after_metafields,
        after_ts,
      )
    }
    _ -> {
      let payload =
        customer_payload_json(
          store,
          "CustomerCreatePayload",
          None,
          None,
          None,
          errors,
          field,
          fragments,
        )
      #(
        MutationFieldResult(
          key: get_field_response_key(field),
          payload: payload,
          staged_resource_ids: [],
          root_name: "customerCreate",
        ),
        store,
        identity,
      )
    }
  }
}

fn build_created_customer(
  id: String,
  timestamp: String,
  input: Dict(String, root_field.ResolvedValue),
  identity: SyntheticIdentityRegistry,
) -> #(
  CustomerRecord,
  List(CustomerAddressRecord),
  List(CustomerMetafieldRecord),
) {
  let email = read_obj_string(input, "email")
  let phone = read_obj_string(input, "phone")
  let first_name = read_normalized_optional_string(input, "firstName")
  let last_name = read_normalized_optional_string(input, "lastName")
  let tags = normalize_tags(read_obj_array_strings(input, "tags"))
  let tax_exempt = read_obj_bool(input, "taxExempt") |> option.unwrap(False)
  let tax_exemptions = read_obj_array_strings(input, "taxExemptions")
  let display = build_display_name(first_name, last_name, email)
  let addresses_input = read_obj_addresses(input)
  let addresses =
    list.index_map(addresses_input, fn(address_input, index) {
      build_address(
        "gid://shopify/MailingAddress/" <> int.to_string(index + 1),
        id,
        index,
        address_input,
        first_name,
        last_name,
      )
    })
  let default_address = case addresses {
    [first, ..] -> Some(address_to_default(first))
    [] -> None
  }
  let metafields = read_customer_metafields(input, id, identity)
  #(
    CustomerRecord(
      id: id,
      first_name: first_name,
      last_name: last_name,
      display_name: display,
      email: email,
      legacy_resource_id: gid_tail(id),
      locale: read_obj_string(input, "locale") |> option.or(Some("en")),
      note: read_normalized_string_with_blank(input, "note"),
      can_delete: Some(True),
      verified_email: Some(True),
      data_sale_opt_out: False,
      tax_exempt: Some(tax_exempt),
      tax_exemptions: tax_exemptions,
      state: Some("DISABLED"),
      tags: tags,
      number_of_orders: Some("0"),
      amount_spent: Some(Money(amount: "0.0", currency_code: "USD")),
      default_email_address: case email {
        Some(e) ->
          Some(CustomerDefaultEmailAddressRecord(
            email_address: Some(e),
            marketing_state: consent_state_from_input(
              input,
              "emailMarketingConsent",
            ),
            marketing_opt_in_level: consent_level_from_input(
              input,
              "emailMarketingConsent",
            ),
            marketing_updated_at: consent_updated_at_from_input(
              input,
              "emailMarketingConsent",
            ),
          ))
        None -> None
      },
      default_phone_number: case phone {
        Some(p) ->
          Some(
            CustomerDefaultPhoneNumberRecord(
              phone_number: Some(p),
              marketing_state: consent_state_from_input(
                input,
                "smsMarketingConsent",
              ),
              marketing_opt_in_level: consent_level_from_input(
                input,
                "smsMarketingConsent",
              ),
              marketing_updated_at: consent_updated_at_from_input(
                input,
                "smsMarketingConsent",
              ),
              marketing_collected_from: case
                has_nested_object(input, "smsMarketingConsent")
              {
                True -> Some("OTHER")
                False ->
                  consent_collected_from_input(input, "smsMarketingConsent")
              },
            ),
          )
        None -> None
      },
      email_marketing_consent: make_email_consent(input),
      sms_marketing_consent: make_sms_consent(input),
      default_address: default_address,
      created_at: Some(timestamp),
      updated_at: Some(timestamp),
    ),
    addresses,
    metafields,
  )
}

fn validate_customer_create(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
) -> List(UserError) {
  let email = read_obj_string(input, "email")
  let phone = read_obj_string(input, "phone")
  let presence_errors = case email, phone {
    None, None -> [
      UserError(
        field: [],
        message: "A name, phone number, or email address must be present",
        code: None,
      ),
    ]
    _, _ -> []
  }
  list.append(
    presence_errors,
    validate_customer_input_fields(store, input, None),
  )
}

fn validate_customer_input_fields(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
  exclude_customer_id: Option(String),
) -> List(UserError) {
  let scalar_errors =
    [
      validate_email(store, input, exclude_customer_id),
      validate_phone(store, input, exclude_customer_id),
      validate_locale(input),
    ]
    |> list.filter_map(fn(item) { item })
  let length_errors =
    list.flatten([
      validate_max_length(input, "firstName", "First name", 255),
      validate_max_length(input, "lastName", "Last name", 255),
      validate_max_length(input, "note", "Note", 5000),
      validate_tag_lengths(input),
    ])
  list.append(scalar_errors, length_errors)
}

fn validate_email(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
  exclude_customer_id: Option(String),
) -> Result(UserError, Nil) {
  use email <- result.try(read_obj_string(input, "email") |> option_to_result)
  case string.contains(email, "@") {
    False -> Ok(UserError(["email"], "Email is invalid", None))
    True ->
      case customer_email_exists(store, email, exclude_customer_id) {
        True -> Ok(UserError(["email"], "Email has already been taken", None))
        False -> Error(Nil)
      }
  }
}

fn validate_phone(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
  exclude_customer_id: Option(String),
) -> Result(UserError, Nil) {
  use phone <- result.try(read_obj_string(input, "phone") |> option_to_result)
  case valid_phone(phone) {
    False -> Ok(UserError(["phone"], "Phone is invalid", None))
    True ->
      case customer_phone_exists(store, phone, exclude_customer_id) {
        True -> Ok(UserError(["phone"], "Phone has already been taken", None))
        False -> Error(Nil)
      }
  }
}

fn validate_locale(
  input: Dict(String, root_field.ResolvedValue),
) -> Result(UserError, Nil) {
  use locale <- result.try(read_obj_string(input, "locale") |> option_to_result)
  case valid_locale(locale) {
    True -> Error(Nil)
    False -> Ok(UserError(["locale"], "Locale is invalid", None))
  }
}

fn validate_max_length(
  input: Dict(String, root_field.ResolvedValue),
  field: String,
  label: String,
  max: Int,
) -> List(UserError) {
  case read_obj_string(input, field) {
    Some(value) ->
      case string.length(value) > max {
        True -> [
          UserError(
            [field],
            label
              <> " is too long (maximum is "
              <> int.to_string(max)
              <> " characters)",
            None,
          ),
        ]
        False -> []
      }
    _ -> []
  }
}

fn validate_tag_lengths(
  input: Dict(String, root_field.ResolvedValue),
) -> List(UserError) {
  read_obj_array_strings(input, "tags")
  |> list.filter(fn(tag) { string.length(tag) > 255 })
  |> list.map(fn(_) {
    UserError(["tags"], "Tags is too long (maximum is 255 characters)", None)
  })
}

fn customer_email_exists(
  store: Store,
  email: String,
  exclude_customer_id: Option(String),
) -> Bool {
  store.list_effective_customers(store)
  |> list.any(fn(customer) {
    customer.id != option.unwrap(exclude_customer_id, "")
    && option.unwrap(customer.email, "") |> string.lowercase
    == string.lowercase(email)
  })
}

fn customer_phone_exists(
  store: Store,
  phone: String,
  exclude_customer_id: Option(String),
) -> Bool {
  store.list_effective_customers(store)
  |> list.any(fn(customer) {
    customer.id != option.unwrap(exclude_customer_id, "")
    && customer.default_phone_number
    |> option.then(fn(value) { value.phone_number })
    |> option.unwrap("")
    == phone
  })
}

fn valid_phone(phone: String) -> Bool {
  string.starts_with(phone, "+")
  && string.length(phone) > 1
  && all_digits(string.drop_start(phone, 1))
}

fn all_digits(value: String) -> Bool {
  case string.pop_grapheme(value) {
    Error(_) -> True
    Ok(#(grapheme, rest)) -> is_digit_string(grapheme) && all_digits(rest)
  }
}

fn is_digit_string(grapheme: String) -> Bool {
  case grapheme {
    "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" -> True
    _ -> False
  }
}

fn valid_locale(locale: String) -> Bool {
  case string.length(locale) {
    2 -> True
    5 -> string.contains(locale, "-")
    _ -> False
  }
}

fn handle_customer_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let input = input_object(field_args(field, variables), "input")
  let id = read_obj_string(input, "id")
  case id {
    Some(customer_id) ->
      case store.get_effective_customer_by_id(store, customer_id) {
        Some(existing) -> {
          let validation_errors =
            list.append(
              inline_consent_update_errors(input),
              validate_customer_input_fields(store, input, Some(customer_id)),
            )
          case validation_errors {
            [_, ..] as errors -> {
              let payload =
                customer_payload_json(
                  store,
                  "CustomerUpdatePayload",
                  None,
                  None,
                  None,
                  errors,
                  field,
                  fragments,
                )
              #(
                MutationFieldResult(
                  get_field_response_key(field),
                  payload,
                  [],
                  "customerUpdate",
                ),
                store,
                identity,
              )
            }
            [] -> {
              let #(timestamp, after_ts) =
                synthetic_identity.make_synthetic_timestamp(identity)
              let updated =
                update_customer_from_input(existing, input, timestamp)
              let #(stored, store_after_customer) =
                store.stage_update_customer(store, updated)
              let store_after_metafields = case
                read_customer_metafields(input, stored.id, after_ts)
              {
                [] -> store_after_customer
                metafields ->
                  store.stage_customer_metafields(
                    store_after_customer,
                    stored.id,
                    metafields,
                  )
              }
              let #(payload_customer, store_after_addresses) =
                replace_customer_input_addresses(
                  store_after_metafields,
                  stored,
                  input,
                )
              let payload =
                customer_payload_json(
                  store_after_addresses,
                  "CustomerUpdatePayload",
                  Some(payload_customer),
                  None,
                  None,
                  [],
                  field,
                  fragments,
                )
              #(
                MutationFieldResult(
                  get_field_response_key(field),
                  payload,
                  [payload_customer.id],
                  "customerUpdate",
                ),
                store_after_addresses,
                after_ts,
              )
            }
          }
        }
        None ->
          unknown_customer_result(
            store,
            identity,
            field,
            fragments,
            "CustomerUpdatePayload",
            "customerUpdate",
          )
      }
    None ->
      unknown_customer_result(
        store,
        identity,
        field,
        fragments,
        "CustomerUpdatePayload",
        "customerUpdate",
      )
  }
}

fn inline_consent_update_errors(
  input: Dict(String, root_field.ResolvedValue),
) -> List(UserError) {
  let email_errors = case has_nested_object(input, "emailMarketingConsent") {
    True -> [
      UserError(
        ["emailMarketingConsent"],
        "To update emailMarketingConsent, please use the customerEmailMarketingConsentUpdate Mutation instead",
        None,
      ),
    ]
    False -> []
  }
  let sms_errors = case has_nested_object(input, "smsMarketingConsent") {
    True -> [
      UserError(
        ["smsMarketingConsent"],
        "To update smsMarketingConsent, please use the customerSmsMarketingConsentUpdate Mutation instead",
        None,
      ),
    ]
    False -> []
  }
  list.append(email_errors, sms_errors)
}

fn handle_customer_set(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let args = field_args(field, variables)
  let input = input_object(args, "input")
  let identifier = input_object(args, "identifier")
  case
    find_customer_by_identifier(store, identifier),
    read_obj_string(input, "id")
  {
    Some(existing), _ -> {
      let #(timestamp, after_ts) =
        synthetic_identity.make_synthetic_timestamp(identity)
      let updated = update_customer_from_input(existing, input, timestamp)
      let #(stored, next_store) = store.stage_update_customer(store, updated)
      let payload =
        customer_payload_json(
          next_store,
          "CustomerSetPayload",
          Some(stored),
          None,
          None,
          [],
          field,
          fragments,
        )
      #(
        MutationFieldResult(
          get_field_response_key(field),
          payload,
          [stored.id],
          "customerSet",
        ),
        next_store,
        after_ts,
      )
    }
    None, Some(customer_id) ->
      case store.get_effective_customer_by_id(store, customer_id) {
        Some(existing) -> {
          let #(timestamp, after_ts) =
            synthetic_identity.make_synthetic_timestamp(identity)
          let updated = update_customer_from_input(existing, input, timestamp)
          let #(stored, next_store) =
            store.stage_update_customer(store, updated)
          let payload =
            customer_payload_json(
              next_store,
              "CustomerSetPayload",
              Some(stored),
              None,
              None,
              [],
              field,
              fragments,
            )
          #(
            MutationFieldResult(
              get_field_response_key(field),
              payload,
              [stored.id],
              "customerSet",
            ),
            next_store,
            after_ts,
          )
        }
        None -> create_from_set(store, identity, field, fragments, input)
      }
    None, None -> create_from_set(store, identity, field, fragments, input)
  }
}

fn create_from_set(store, identity, field, fragments, input) {
  let #(id, after_id) =
    synthetic_identity.make_synthetic_gid(identity, "Customer")
  let #(timestamp, after_ts) =
    synthetic_identity.make_synthetic_timestamp(after_id)
  let #(customer, addresses, metafields) =
    build_created_customer(id, timestamp, input, after_ts)
  let #(stored, store_after_customer) =
    store.stage_create_customer(store, customer)
  let store_after_addresses =
    list.fold(addresses, store_after_customer, fn(acc, address) {
      let #(_, next_store) = store.stage_upsert_customer_address(acc, address)
      next_store
    })
  let store_after_metafields =
    store.stage_customer_metafields(
      store_after_addresses,
      stored.id,
      metafields,
    )
  let payload =
    customer_payload_json(
      store_after_metafields,
      "CustomerSetPayload",
      Some(stored),
      None,
      None,
      [],
      field,
      fragments,
    )
  #(
    MutationFieldResult(
      get_field_response_key(field),
      payload,
      [stored.id],
      "customerSet",
    ),
    store_after_metafields,
    after_ts,
  )
}

fn update_customer_from_input(
  existing: CustomerRecord,
  input: Dict(String, root_field.ResolvedValue),
  timestamp: String,
) -> CustomerRecord {
  let first_name =
    update_trimmed_nullable_string(existing.first_name, input, "firstName")
  let last_name =
    update_trimmed_nullable_string(existing.last_name, input, "lastName")
  let email = update_trimmed_nullable_string(existing.email, input, "email")
  let phone =
    update_trimmed_nullable_string(
      existing.default_phone_number |> option.then(fn(v) { v.phone_number }),
      input,
      "phone",
    )
  CustomerRecord(
    ..existing,
    first_name: first_name,
    last_name: last_name,
    display_name: build_display_name(first_name, last_name, email),
    email: email,
    locale: update_trimmed_nullable_string(existing.locale, input, "locale"),
    note: update_nullable_note(existing.note, input),
    verified_email: read_obj_bool(input, "verifiedEmail")
      |> option.or(existing.verified_email),
    tax_exempt: read_obj_bool(input, "taxExempt")
      |> option.or(existing.tax_exempt),
    tax_exemptions: case read_obj_array_strings(input, "taxExemptions") {
      [] -> existing.tax_exemptions
      values -> values
    },
    tags: case read_obj_array_strings(input, "tags") {
      [] -> existing.tags
      values -> normalize_tags(values)
    },
    default_email_address: case email {
      Some(e) ->
        Some(CustomerDefaultEmailAddressRecord(
          email_address: Some(e),
          marketing_state: existing.default_email_address
            |> option.then(fn(v) { v.marketing_state })
            |> option.or(
              existing.email_marketing_consent
              |> option.then(fn(v) { v.marketing_state }),
            ),
          marketing_opt_in_level: existing.default_email_address
            |> option.then(fn(v) { v.marketing_opt_in_level })
            |> option.or(
              existing.email_marketing_consent
              |> option.then(fn(v) { v.marketing_opt_in_level }),
            ),
          marketing_updated_at: existing.default_email_address
            |> option.then(fn(v) { v.marketing_updated_at })
            |> option.or(
              existing.email_marketing_consent
              |> option.then(fn(v) { v.consent_updated_at }),
            ),
        ))
      None -> None
    },
    default_phone_number: case phone {
      Some(p) ->
        Some(CustomerDefaultPhoneNumberRecord(
          phone_number: Some(p),
          marketing_state: existing.default_phone_number
            |> option.then(fn(v) { v.marketing_state })
            |> option.or(
              existing.sms_marketing_consent
              |> option.then(fn(v) { v.marketing_state }),
            ),
          marketing_opt_in_level: existing.default_phone_number
            |> option.then(fn(v) { v.marketing_opt_in_level })
            |> option.or(
              existing.sms_marketing_consent
              |> option.then(fn(v) { v.marketing_opt_in_level }),
            ),
          marketing_updated_at: existing.default_phone_number
            |> option.then(fn(v) { v.marketing_updated_at })
            |> option.or(
              existing.sms_marketing_consent
              |> option.then(fn(v) { v.consent_updated_at }),
            ),
          marketing_collected_from: existing.default_phone_number
            |> option.then(fn(v) { v.marketing_collected_from })
            |> option.or(
              existing.sms_marketing_consent
              |> option.then(fn(v) { v.consent_collected_from }),
            ),
        ))
      None -> None
    },
    email_marketing_consent: case email {
      Some(_) -> existing.email_marketing_consent
      None -> None
    },
    sms_marketing_consent: case phone {
      Some(_) -> existing.sms_marketing_consent
      None -> None
    },
    updated_at: Some(timestamp),
  )
}

fn unknown_customer_result(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  typename: String,
  root_name: String,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let payload =
    customer_payload_json(
      store,
      typename,
      None,
      None,
      None,
      [
        UserError(
          field: ["id"],
          message: "Customer does not exist",
          code: Some("CUSTOMER_DOES_NOT_EXIST"),
        ),
      ],
      field,
      fragments,
    )
  #(
    MutationFieldResult(get_field_response_key(field), payload, [], root_name),
    store,
    identity,
  )
}

fn customer_missing_result(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  typename: String,
  root_name: String,
  error_field: List(String),
  message: String,
  code: Option(String),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let payload =
    customer_payload_json(
      store,
      typename,
      None,
      None,
      None,
      [UserError(error_field, message, code)],
      field,
      fragments,
    )
  #(
    MutationFieldResult(get_field_response_key(field), payload, [], root_name),
    store,
    identity,
  )
}

fn handle_customer_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let input = input_object(field_args(field, variables), "input")
  let id = read_obj_string(input, "id")
  case id {
    Some(customer_id) ->
      case store.get_effective_customer_by_id(store, customer_id) {
        Some(_) -> {
          let next_store = store.stage_delete_customer(store, customer_id)
          let payload =
            customer_payload_json(
              next_store,
              "CustomerDeletePayload",
              None,
              Some(customer_id),
              None,
              [],
              field,
              fragments,
            )
          #(
            MutationFieldResult(
              get_field_response_key(field),
              payload,
              [customer_id],
              "customerDelete",
            ),
            next_store,
            identity,
          )
        }
        None -> {
          let payload =
            customer_payload_json(
              store,
              "CustomerDeletePayload",
              None,
              None,
              None,
              [
                UserError(["id"], "Customer can't be found", None),
              ],
              field,
              fragments,
            )
          #(
            MutationFieldResult(
              get_field_response_key(field),
              payload,
              [],
              "customerDelete",
            ),
            store,
            identity,
          )
        }
      }
    None -> {
      let payload =
        customer_payload_json(
          store,
          "CustomerDeletePayload",
          None,
          None,
          None,
          [
            UserError(["id"], "Customer can't be found", None),
          ],
          field,
          fragments,
        )
      #(
        MutationFieldResult(
          get_field_response_key(field),
          payload,
          [],
          "customerDelete",
        ),
        store,
        identity,
      )
    }
  }
}

fn handle_customer_address_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let args = field_args(field, variables)
  let customer_id = read_arg_string(args, "customerId")
  let address_input = input_object(args, "address")
  let set_default = read_arg_bool(args, "setAsDefault") |> option.unwrap(False)
  case customer_id {
    Some(id) ->
      case store.get_effective_customer_by_id(store, id) {
        Some(customer) -> {
          let #(address_id, after_id) =
            synthetic_identity.make_synthetic_gid(identity, "MailingAddress")
          let existing_count =
            store.list_effective_customer_addresses(store, id) |> list.length()
          let address =
            build_address(
              address_id,
              id,
              existing_count,
              address_input,
              customer.first_name,
              customer.last_name,
            )
          case find_duplicate_customer_address(store, id, address, None) {
            Some(_) -> {
              let payload =
                address_payload_json(
                  store,
                  "CustomerAddressCreatePayload",
                  None,
                  None,
                  [UserError(["address"], "Address already exists", None)],
                  field,
                  fragments,
                )
              #(
                MutationFieldResult(
                  get_field_response_key(field),
                  payload,
                  [],
                  "customerAddressCreate",
                ),
                store,
                identity,
              )
            }
            None -> {
              let #(_, store_after_address) =
                store.stage_upsert_customer_address(store, address)
              let next_store = case
                set_default || customer.default_address == None
              {
                True -> {
                  let updated =
                    CustomerRecord(
                      ..customer,
                      default_address: Some(address_to_default(address)),
                    )
                  let #(_, s) =
                    store.stage_update_customer(store_after_address, updated)
                  s
                }
                False -> store_after_address
              }
              let payload =
                address_payload_json(
                  next_store,
                  "CustomerAddressCreatePayload",
                  Some(address),
                  None,
                  [],
                  field,
                  fragments,
                )
              #(
                MutationFieldResult(
                  get_field_response_key(field),
                  payload,
                  [address.id],
                  "customerAddressCreate",
                ),
                next_store,
                after_id,
              )
            }
          }
        }
        None ->
          address_unknown_result(
            store,
            identity,
            field,
            fragments,
            "CustomerAddressCreatePayload",
            "customerAddressCreate",
          )
      }
    None ->
      address_unknown_result(
        store,
        identity,
        field,
        fragments,
        "CustomerAddressCreatePayload",
        "customerAddressCreate",
      )
  }
}

fn handle_customer_address_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let args = field_args(field, variables)
  let address_id = read_arg_string(args, "addressId")
  let address_input = input_object(args, "address")
  let set_default = read_arg_bool(args, "setAsDefault") |> option.unwrap(False)
  case address_id {
    Some(id) ->
      case store.get_effective_customer_address_by_id(store, id) {
        Some(existing) -> {
          let updated = merge_address(existing, address_input)
          let #(_, store_after_address) =
            store.stage_upsert_customer_address(store, updated)
          let next_store = case set_default {
            True ->
              case
                store.get_effective_customer_by_id(
                  store_after_address,
                  updated.customer_id,
                )
              {
                Some(customer) -> {
                  let #(_, s) =
                    store.stage_update_customer(
                      store_after_address,
                      CustomerRecord(
                        ..customer,
                        default_address: Some(address_to_default(updated)),
                      ),
                    )
                  s
                }
                None -> store_after_address
              }
            False -> store_after_address
          }
          let payload =
            address_payload_json(
              next_store,
              "CustomerAddressUpdatePayload",
              Some(updated),
              None,
              [],
              field,
              fragments,
            )
          #(
            MutationFieldResult(
              get_field_response_key(field),
              payload,
              [updated.id],
              "customerAddressUpdate",
            ),
            next_store,
            identity,
          )
        }
        None ->
          address_unknown_result(
            store,
            identity,
            field,
            fragments,
            "CustomerAddressUpdatePayload",
            "customerAddressUpdate",
          )
      }
    None ->
      address_unknown_result(
        store,
        identity,
        field,
        fragments,
        "CustomerAddressUpdatePayload",
        "customerAddressUpdate",
      )
  }
}

fn handle_customer_address_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let args = field_args(field, variables)
  let address_id = read_arg_string(args, "addressId")
  case address_id {
    Some(id) ->
      case store.get_effective_customer_address_by_id(store, id) {
        Some(address) -> {
          let store_after_delete =
            store.stage_delete_customer_address(store, id)
          let next_store = case
            store.get_effective_customer_by_id(
              store_after_delete,
              address.customer_id,
            )
          {
            Some(customer) -> {
              let current_default =
                customer.default_address |> option.then(fn(a) { a.id })
              case current_default == Some(id) {
                True -> {
                  let replacement =
                    store.list_effective_customer_addresses(
                      store_after_delete,
                      address.customer_id,
                    )
                    |> list.first()
                    |> result_to_option()
                  let updated =
                    CustomerRecord(
                      ..customer,
                      default_address: replacement
                        |> option.map(address_to_default),
                    )
                  let #(_, s) =
                    store.stage_update_customer(store_after_delete, updated)
                  s
                }
                False -> store_after_delete
              }
            }
            None -> store_after_delete
          }
          let payload =
            address_payload_json(
              next_store,
              "CustomerAddressDeletePayload",
              None,
              Some(id),
              [],
              field,
              fragments,
            )
          #(
            MutationFieldResult(
              get_field_response_key(field),
              payload,
              [id],
              "customerAddressDelete",
            ),
            next_store,
            identity,
          )
        }
        None ->
          address_unknown_result(
            store,
            identity,
            field,
            fragments,
            "CustomerAddressDeletePayload",
            "customerAddressDelete",
          )
      }
    None ->
      address_unknown_result(
        store,
        identity,
        field,
        fragments,
        "CustomerAddressDeletePayload",
        "customerAddressDelete",
      )
  }
}

fn handle_customer_update_default_address(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let args = field_args(field, variables)
  let customer_id = read_arg_string(args, "customerId")
  let address_id = read_arg_string(args, "addressId")
  case customer_id, address_id {
    Some(cid), Some(aid) ->
      case
        store.get_effective_customer_by_id(store, cid),
        store.get_effective_customer_address_by_id(store, aid)
      {
        Some(customer), Some(address) -> {
          let updated =
            CustomerRecord(
              ..customer,
              default_address: Some(address_to_default(address)),
            )
          let #(_, next_store) = store.stage_update_customer(store, updated)
          let payload =
            customer_payload_json(
              next_store,
              "CustomerUpdateDefaultAddressPayload",
              Some(updated),
              None,
              None,
              [],
              field,
              fragments,
            )
          #(
            MutationFieldResult(
              get_field_response_key(field),
              payload,
              [cid, aid],
              "customerUpdateDefaultAddress",
            ),
            next_store,
            identity,
          )
        }
        _, _ ->
          unknown_customer_result(
            store,
            identity,
            field,
            fragments,
            "CustomerUpdateDefaultAddressPayload",
            "customerUpdateDefaultAddress",
          )
      }
    _, _ ->
      unknown_customer_result(
        store,
        identity,
        field,
        fragments,
        "CustomerUpdateDefaultAddressPayload",
        "customerUpdateDefaultAddress",
      )
  }
}

fn handle_customer_tax_exemptions(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  mode: String,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let args = field_args(field, variables)
  let customer_id = read_arg_string(args, "customerId")
  let exemptions = case dict.get(args, "taxExemptions") {
    Ok(root_field.ListVal(items)) ->
      list.filter_map(items, fn(item) {
        case item {
          root_field.StringVal(s) -> Ok(s)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
  let root = case mode {
    "add" -> "customerAddTaxExemptions"
    "remove" -> "customerRemoveTaxExemptions"
    _ -> "customerReplaceTaxExemptions"
  }
  let typename = case mode {
    "add" -> "CustomerAddTaxExemptionsPayload"
    "remove" -> "CustomerRemoveTaxExemptionsPayload"
    _ -> "CustomerReplaceTaxExemptionsPayload"
  }
  case customer_id {
    Some(id) ->
      case store.get_effective_customer_by_id(store, id) {
        Some(customer) -> {
          let next_exemptions = case mode {
            "add" ->
              normalize_tags(list.append(customer.tax_exemptions, exemptions))
            "remove" ->
              list.filter(customer.tax_exemptions, fn(e) {
                !list.contains(exemptions, e)
              })
            _ -> normalize_tags(exemptions)
          }
          let updated =
            CustomerRecord(..customer, tax_exemptions: next_exemptions)
          let #(_, next_store) = store.stage_update_customer(store, updated)
          let payload =
            customer_payload_json(
              next_store,
              typename,
              Some(updated),
              None,
              None,
              [],
              field,
              fragments,
            )
          #(
            MutationFieldResult(
              get_field_response_key(field),
              payload,
              [id],
              root,
            ),
            next_store,
            identity,
          )
        }
        None ->
          customer_missing_result(
            store,
            identity,
            field,
            fragments,
            typename,
            root,
            ["customerId"],
            "Customer does not exist.",
            None,
          )
      }
    None ->
      customer_missing_result(
        store,
        identity,
        field,
        fragments,
        typename,
        root,
        ["customerId"],
        "Customer does not exist.",
        None,
      )
  }
}

fn handle_email_consent(store, identity, field, fragments, variables) {
  let input = input_object(field_args(field, variables), "input")
  let customer_id = read_obj_string(input, "customerId")
  case customer_id {
    Some(id) ->
      case store.get_effective_customer_by_id(store, id) {
        Some(customer) -> {
          let consent = read_nested_object(input, "emailMarketingConsent")
          let updated =
            CustomerRecord(
              ..customer,
              default_email_address: Some(CustomerDefaultEmailAddressRecord(
                email_address: customer.email,
                marketing_state: read_obj_string(consent, "marketingState"),
                marketing_opt_in_level: read_obj_string(
                  consent,
                  "marketingOptInLevel",
                ),
                marketing_updated_at: read_obj_string(
                  consent,
                  "consentUpdatedAt",
                ),
              )),
              email_marketing_consent: make_email_consent_from(consent),
            )
          let #(_, next_store) = store.stage_update_customer(store, updated)
          let payload =
            customer_payload_json(
              next_store,
              "CustomerEmailMarketingConsentUpdatePayload",
              Some(updated),
              None,
              None,
              [],
              field,
              fragments,
            )
          #(
            MutationFieldResult(
              get_field_response_key(field),
              payload,
              [id],
              "customerEmailMarketingConsentUpdate",
            ),
            next_store,
            identity,
          )
        }
        None ->
          customer_missing_result(
            store,
            identity,
            field,
            fragments,
            "CustomerEmailMarketingConsentUpdatePayload",
            "customerEmailMarketingConsentUpdate",
            ["input", "customerId"],
            "Customer not found",
            Some("INVALID"),
          )
      }
    None ->
      customer_missing_result(
        store,
        identity,
        field,
        fragments,
        "CustomerEmailMarketingConsentUpdatePayload",
        "customerEmailMarketingConsentUpdate",
        ["input", "customerId"],
        "Customer not found",
        Some("INVALID"),
      )
  }
}

fn handle_sms_consent(store, identity, field, fragments, variables) {
  let input = input_object(field_args(field, variables), "input")
  let customer_id = read_obj_string(input, "customerId")
  case customer_id {
    Some(id) ->
      case store.get_effective_customer_by_id(store, id) {
        Some(customer) -> {
          let consent = read_nested_object(input, "smsMarketingConsent")
          let updated =
            CustomerRecord(
              ..customer,
              default_phone_number: Some(CustomerDefaultPhoneNumberRecord(
                phone_number: customer.default_phone_number
                  |> option.then(fn(v) { v.phone_number }),
                marketing_state: read_obj_string(consent, "marketingState"),
                marketing_opt_in_level: read_obj_string(
                  consent,
                  "marketingOptInLevel",
                ),
                marketing_updated_at: read_obj_string(
                  consent,
                  "consentUpdatedAt",
                ),
                marketing_collected_from: Some("OTHER"),
              )),
              sms_marketing_consent: make_sms_consent_from(consent),
            )
          let #(_, next_store) = store.stage_update_customer(store, updated)
          let payload =
            customer_payload_json(
              next_store,
              "CustomerSmsMarketingConsentUpdatePayload",
              Some(updated),
              None,
              None,
              [],
              field,
              fragments,
            )
          #(
            MutationFieldResult(
              get_field_response_key(field),
              payload,
              [id],
              "customerSmsMarketingConsentUpdate",
            ),
            next_store,
            identity,
          )
        }
        None ->
          customer_missing_result(
            store,
            identity,
            field,
            fragments,
            "CustomerSmsMarketingConsentUpdatePayload",
            "customerSmsMarketingConsentUpdate",
            [],
            "Customer not found",
            None,
          )
      }
    None ->
      customer_missing_result(
        store,
        identity,
        field,
        fragments,
        "CustomerSmsMarketingConsentUpdatePayload",
        "customerSmsMarketingConsentUpdate",
        [],
        "Customer not found",
        None,
      )
  }
}

fn handle_data_erasure(store, identity, field, variables, cancel) {
  let args = field_args(field, variables)
  let customer_id = read_arg_string(args, "customerId")
  let root = case cancel {
    True -> "customerCancelDataErasure"
    False -> "customerRequestDataErasure"
  }
  let typename = case cancel {
    True -> "CustomerCancelDataErasurePayload"
    False -> "CustomerRequestDataErasurePayload"
  }
  let errors = case customer_id {
    Some(id) ->
      case store.get_effective_customer_by_id(store, id) {
        Some(_) ->
          case cancel {
            False -> []
            True ->
              case store.get_customer_data_erasure_request(store, id) {
                Some(request) ->
                  case request.canceled_at {
                    None -> []
                    Some(_) -> [
                      UserError(
                        ["customerId"],
                        "Customer's data is not scheduled for erasure",
                        Some("NOT_BEING_ERASED"),
                      ),
                    ]
                  }
                None -> [
                  UserError(
                    ["customerId"],
                    "Customer's data is not scheduled for erasure",
                    Some("NOT_BEING_ERASED"),
                  ),
                ]
              }
          }
        None -> [
          UserError(
            ["customerId"],
            "Customer does not exist",
            Some("DOES_NOT_EXIST"),
          ),
        ]
      }
    None -> [
      UserError(
        ["customerId"],
        "Customer does not exist",
        Some("DOES_NOT_EXIST"),
      ),
    ]
  }
  let next_store = case errors, customer_id {
    [], Some(id) -> {
      let request =
        types.CustomerDataErasureRequestRecord(
          customer_id: id,
          requested_at: "",
          canceled_at: case cancel {
            True -> Some("")
            False -> None
          },
        )
      store.stage_customer_data_erasure_request(store, request)
    }
    _, _ -> store
  }
  let payload = case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      project_graphql_value(
        src_object([
          #("__typename", SrcString(typename)),
          #("customerId", case errors {
            [] -> optional_string_source(customer_id)
            _ -> SrcNull
          }),
          #("userErrors", SrcList(list.map(errors, user_error_source))),
        ]),
        selections,
        dict.new(),
      )
    _ -> json.object([])
  }
  #(
    MutationFieldResult(
      get_field_response_key(field),
      payload,
      case customer_id {
        Some(id) -> [id]
        None -> []
      },
      root,
    ),
    next_store,
    identity,
  )
}

fn handle_activation_url(store, identity, field, variables) {
  let args = field_args(field, variables)
  let customer_id = read_arg_string(args, "customerId")
  let errors = case customer_id {
    Some(id) ->
      case store.get_effective_customer_by_id(store, id) {
        Some(_) -> []
        None -> [
          UserError(
            ["customerId"],
            "The customer can't be found.",
            Some("CUSTOMER_DOES_NOT_EXIST"),
          ),
        ]
      }
    None -> [
      UserError(
        ["customerId"],
        "The customer can't be found.",
        Some("CUSTOMER_DOES_NOT_EXIST"),
      ),
    ]
  }
  let url = case errors, customer_id {
    [], Some(id) ->
      "https://shopify-draft-proxy.local/customer-account/activate?customer_id="
      <> id
    _, _ -> ""
  }
  let payload = case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      project_graphql_value(
        src_object([
          #(
            "__typename",
            SrcString("CustomerGenerateAccountActivationUrlPayload"),
          ),
          #("accountActivationUrl", case errors {
            [] -> SrcString(url)
            _ -> SrcNull
          }),
          #("userErrors", SrcList(list.map(errors, user_error_source))),
        ]),
        selections,
        dict.new(),
      )
    _ -> json.object([])
  }
  #(
    MutationFieldResult(
      get_field_response_key(field),
      payload,
      case customer_id {
        Some(id) -> [id]
        None -> []
      },
      "customerGenerateAccountActivationUrl",
    ),
    store,
    identity,
  )
}

fn handle_account_invite(store, identity, field, fragments, variables) {
  let args = field_args(field, variables)
  let customer_id = read_arg_string(args, "customerId")
  case customer_id {
    Some(id) ->
      case store.get_effective_customer_by_id(store, id) {
        Some(customer) -> {
          let updated = CustomerRecord(..customer, state: Some("INVITED"))
          let #(_, next_store) = store.stage_update_customer(store, updated)
          let payload =
            customer_payload_json(
              next_store,
              "CustomerSendAccountInviteEmailPayload",
              Some(updated),
              None,
              None,
              [],
              field,
              fragments,
            )
          #(
            MutationFieldResult(
              get_field_response_key(field),
              payload,
              [id],
              "customerSendAccountInviteEmail",
            ),
            next_store,
            identity,
          )
        }
        None ->
          customer_missing_result(
            store,
            identity,
            field,
            fragments,
            "CustomerSendAccountInviteEmailPayload",
            "customerSendAccountInviteEmail",
            ["customerId"],
            "Customer can't be found",
            None,
          )
      }
    None ->
      customer_missing_result(
        store,
        identity,
        field,
        fragments,
        "CustomerSendAccountInviteEmailPayload",
        "customerSendAccountInviteEmail",
        ["customerId"],
        "Customer can't be found",
        None,
      )
  }
}

fn handle_payment_method_update_email(
  store,
  identity,
  field,
  fragments,
  variables,
) {
  let args = field_args(field, variables)
  let id = read_arg_string(args, "customerPaymentMethodId")
  case id {
    Some(payment_id) ->
      case
        store.get_effective_customer_payment_method_by_id(
          store,
          payment_id,
          True,
        )
      {
        Some(method) ->
          case store.get_effective_customer_by_id(store, method.customer_id) {
            Some(customer) -> {
              let payload =
                customer_payload_json(
                  store,
                  "CustomerPaymentMethodSendUpdateEmailPayload",
                  Some(customer),
                  None,
                  None,
                  [],
                  field,
                  fragments,
                )
              #(
                MutationFieldResult(
                  get_field_response_key(field),
                  payload,
                  [payment_id],
                  "customerPaymentMethodSendUpdateEmail",
                ),
                store,
                identity,
              )
            }
            None ->
              customer_missing_result(
                store,
                identity,
                field,
                fragments,
                "CustomerPaymentMethodSendUpdateEmailPayload",
                "customerPaymentMethodSendUpdateEmail",
                ["customerPaymentMethodId"],
                "Customer payment method does not exist",
                None,
              )
          }
        None ->
          customer_missing_result(
            store,
            identity,
            field,
            fragments,
            "CustomerPaymentMethodSendUpdateEmailPayload",
            "customerPaymentMethodSendUpdateEmail",
            ["customerPaymentMethodId"],
            "Customer payment method does not exist",
            None,
          )
      }
    None ->
      customer_missing_result(
        store,
        identity,
        field,
        fragments,
        "CustomerPaymentMethodSendUpdateEmailPayload",
        "customerPaymentMethodSendUpdateEmail",
        ["customerPaymentMethodId"],
        "Customer payment method does not exist",
        None,
      )
  }
}

fn handle_store_credit_adjustment(
  store,
  identity,
  field,
  fragments,
  variables,
  is_credit,
) {
  let args = field_args(field, variables)
  let account_id = read_arg_string(args, "id")
  let input_name = case is_credit {
    True -> "creditInput"
    False -> "debitInput"
  }
  let input = input_object(args, input_name)
  let amount = read_money(input)
  let root = case is_credit {
    True -> "storeCreditAccountCredit"
    False -> "storeCreditAccountDebit"
  }
  let typename = case is_credit {
    True -> "StoreCreditAccountCreditPayload"
    False -> "StoreCreditAccountDebitPayload"
  }
  case account_id, amount {
    Some(id), Some(money) ->
      case store.get_effective_store_credit_account_by_id(store, id) {
        Some(account) -> {
          let balance_cents = parse_cents(account.balance.amount)
          let amount_cents = parse_cents(money.amount)
          let signed = case is_credit {
            True -> amount_cents
            False -> 0 - amount_cents
          }
          let new_balance = balance_cents + signed
          let errors = case !is_credit && new_balance < 0 {
            True -> [
              UserError(
                [input_name, "amount"],
                "Insufficient funds",
                Some("INSUFFICIENT_FUNDS"),
              ),
            ]
            False -> []
          }
          case errors {
            [] -> {
              let #(transaction_id, after_id) =
                synthetic_identity.make_synthetic_gid(
                  identity,
                  "StoreCreditAccountTransaction",
                )
              let #(timestamp, after_ts) =
                synthetic_identity.make_synthetic_timestamp(after_id)
              let new_account =
                StoreCreditAccountRecord(
                  ..account,
                  balance: Money(
                    amount: format_cents(new_balance),
                    currency_code: money.currency_code,
                  ),
                )
              let transaction =
                StoreCreditAccountTransactionRecord(
                  id: transaction_id,
                  account_id: account.id,
                  amount: Money(
                    amount: format_cents(signed),
                    currency_code: money.currency_code,
                  ),
                  balance_after_transaction: new_account.balance,
                  created_at: timestamp,
                  event: "ADJUSTMENT",
                )
              let next_store =
                store.stage_store_credit_account(store, new_account)
                |> store.stage_store_credit_account_transaction(transaction)
              let payload =
                store_credit_payload_json(
                  next_store,
                  typename,
                  Some(transaction),
                  [],
                  field,
                  fragments,
                )
              #(
                MutationFieldResult(
                  get_field_response_key(field),
                  payload,
                  [transaction.id, account.id],
                  root,
                ),
                next_store,
                after_ts,
              )
            }
            _ -> {
              let payload =
                store_credit_payload_json(
                  store,
                  typename,
                  None,
                  errors,
                  field,
                  fragments,
                )
              #(
                MutationFieldResult(
                  get_field_response_key(field),
                  payload,
                  [],
                  root,
                ),
                store,
                identity,
              )
            }
          }
        }
        None -> {
          let payload =
            store_credit_payload_json(
              store,
              typename,
              None,
              [
                UserError(
                  ["id"],
                  "Store credit account does not exist",
                  Some("NOT_FOUND"),
                ),
              ],
              field,
              fragments,
            )
          #(
            MutationFieldResult(
              get_field_response_key(field),
              payload,
              [],
              root,
            ),
            store,
            identity,
          )
        }
      }
    _, _ -> {
      let payload =
        store_credit_payload_json(
          store,
          typename,
          None,
          [
            UserError(
              [input_name, "amount"],
              "Amount is invalid",
              Some("INVALID"),
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
  }
}

fn handle_order_customer_set(store, identity, field, fragments, variables) {
  let args = field_args(field, variables)
  let order_id = read_arg_string(args, "orderId")
  let customer_id = read_arg_string(args, "customerId")
  case order_id, customer_id {
    Some(order_id), Some(customer_id) ->
      case
        store.get_effective_customer_order_summary_by_id(store, order_id),
        store.get_effective_customer_by_id(store, customer_id)
      {
        Some(order), Some(customer) -> {
          let linked =
            CustomerOrderSummaryRecord(..order, customer_id: Some(customer.id))
          let next_store = store.stage_customer_order_summary(store, linked)
          let payload =
            order_customer_payload_json(
              next_store,
              "OrderCustomerSetPayload",
              Some(linked),
              [],
              field,
              fragments,
            )
          #(
            MutationFieldResult(
              get_field_response_key(field),
              payload,
              [order_id],
              "orderCustomerSet",
            ),
            next_store,
            identity,
          )
        }
        _, _ -> {
          let payload =
            order_customer_payload_json(
              store,
              "OrderCustomerSetPayload",
              None,
              [UserError(["orderId"], "Order does not exist", Some("INVALID"))],
              field,
              fragments,
            )
          #(
            MutationFieldResult(
              get_field_response_key(field),
              payload,
              [],
              "orderCustomerSet",
            ),
            store,
            identity,
          )
        }
      }
    _, _ -> {
      let payload =
        order_customer_payload_json(
          store,
          "OrderCustomerSetPayload",
          None,
          [UserError(["orderId"], "Order does not exist", Some("INVALID"))],
          field,
          fragments,
        )
      #(
        MutationFieldResult(
          get_field_response_key(field),
          payload,
          [],
          "orderCustomerSet",
        ),
        store,
        identity,
      )
    }
  }
}

fn handle_order_customer_remove(store, identity, field, fragments, variables) {
  let args = field_args(field, variables)
  case read_arg_string(args, "orderId") {
    Some(order_id) ->
      case store.get_effective_customer_order_summary_by_id(store, order_id) {
        Some(order) -> {
          let unlinked = CustomerOrderSummaryRecord(..order, customer_id: None)
          let next_store = store.stage_customer_order_summary(store, unlinked)
          let payload =
            order_customer_payload_json(
              next_store,
              "OrderCustomerRemovePayload",
              Some(unlinked),
              [],
              field,
              fragments,
            )
          #(
            MutationFieldResult(
              get_field_response_key(field),
              payload,
              [order_id],
              "orderCustomerRemove",
            ),
            next_store,
            identity,
          )
        }
        None -> {
          let payload =
            order_customer_payload_json(
              store,
              "OrderCustomerRemovePayload",
              None,
              [UserError(["orderId"], "Order does not exist", Some("INVALID"))],
              field,
              fragments,
            )
          #(
            MutationFieldResult(
              get_field_response_key(field),
              payload,
              [],
              "orderCustomerRemove",
            ),
            store,
            identity,
          )
        }
      }
    None -> {
      let payload =
        order_customer_payload_json(
          store,
          "OrderCustomerRemovePayload",
          None,
          [UserError(["orderId"], "Order does not exist", Some("INVALID"))],
          field,
          fragments,
        )
      #(
        MutationFieldResult(
          get_field_response_key(field),
          payload,
          [],
          "orderCustomerRemove",
        ),
        store,
        identity,
      )
    }
  }
}

fn handle_customer_merge(store, identity, field, fragments, variables) {
  let args = field_args(field, variables)
  let one = read_arg_string(args, "customerOneId")
  let two = read_arg_string(args, "customerTwoId")
  let override = input_object(args, "overrideFields")
  case one, two {
    Some(one_id), Some(two_id) ->
      case one_id == two_id {
        True -> {
          let payload =
            merge_error_payload(
              field,
              fragments,
              [],
              "Customers IDs should not match",
              Some("INVALID_CUSTOMER_ID"),
            )
          #(
            MutationFieldResult(
              get_field_response_key(field),
              payload,
              [],
              "customerMerge",
            ),
            store,
            identity,
          )
        }
        False ->
          case
            store.get_effective_customer_by_id(store, one_id),
            store.get_effective_customer_by_id(store, two_id)
          {
            Some(c1), Some(c2) -> {
              let #(job_id, after_id) =
                synthetic_identity.make_synthetic_gid(identity, "Job")
              let #(timestamp, after_ts) =
                synthetic_identity.make_synthetic_timestamp(after_id)
              let merged = build_merged_customer(c1, c2, override, timestamp)
              let request =
                CustomerMergeRequestRecord(
                  job_id: job_id,
                  resulting_customer_id: merged.id,
                  status: "COMPLETED",
                  customer_merge_errors: [],
                )
              let payload_request =
                CustomerMergeRequestRecord(
                  job_id: job_id,
                  resulting_customer_id: merged.id,
                  status: "IN_PROGRESS",
                  customer_merge_errors: [],
                )
              let source_addresses =
                store.list_effective_customer_addresses(store, c1.id)
              let next_store =
                store.stage_merge_customers(store, c1.id, merged, request)
                |> stage_customer_merge_attached_resources(
                  c1,
                  c2,
                  merged,
                  source_addresses,
                )
              let payload =
                merge_payload_json(payload_request, field, fragments)
              #(
                MutationFieldResult(
                  get_field_response_key(field),
                  payload,
                  [c1.id, c2.id, job_id],
                  "customerMerge",
                ),
                next_store,
                after_ts,
              )
            }
            None, _ -> {
              let payload =
                merge_error_payload(
                  field,
                  fragments,
                  ["customerOneId"],
                  "Customer does not exist with ID "
                    <> option.unwrap(gid_tail(one_id), one_id),
                  Some("INVALID_CUSTOMER_ID"),
                )
              #(
                MutationFieldResult(
                  get_field_response_key(field),
                  payload,
                  [],
                  "customerMerge",
                ),
                store,
                identity,
              )
            }
            _, None -> {
              let payload =
                merge_error_payload(
                  field,
                  fragments,
                  ["customerTwoId"],
                  "Customer does not exist with ID "
                    <> option.unwrap(gid_tail(two_id), two_id),
                  Some("INVALID_CUSTOMER_ID"),
                )
              #(
                MutationFieldResult(
                  get_field_response_key(field),
                  payload,
                  [],
                  "customerMerge",
                ),
                store,
                identity,
              )
            }
          }
      }
    _, _ -> {
      let payload =
        merge_error_payload(
          field,
          fragments,
          ["customerId"],
          "Required argument missing",
          Some("CUSTOMER_DOES_NOT_EXIST"),
        )
      #(
        MutationFieldResult(
          get_field_response_key(field),
          payload,
          [],
          "customerMerge",
        ),
        store,
        identity,
      )
    }
  }
}

fn stage_customer_merge_attached_resources(
  store: Store,
  source: CustomerRecord,
  result: CustomerRecord,
  merged: CustomerRecord,
  source_addresses: List(CustomerAddressRecord),
) -> Store {
  let with_source_addresses =
    source_addresses
    |> list.index_map(fn(address, index) {
      CustomerAddressRecord(
        ..address,
        customer_id: merged.id,
        position: -1000 + index,
      )
    })
    |> list.fold(store, fn(acc, address) {
      let #(_, next_store) = store.stage_upsert_customer_address(acc, address)
      next_store
    })
  let result_metafields =
    store.get_effective_metafields_by_customer_id(
      with_source_addresses,
      result.id,
    )
  let result_keys =
    result_metafields
    |> list.map(customer_metafield_key)
  let copied_source_metafields =
    store.get_effective_metafields_by_customer_id(
      with_source_addresses,
      source.id,
    )
    |> list.filter(fn(metafield) {
      !list.contains(result_keys, customer_metafield_key(metafield))
    })
    |> list.map(fn(metafield) {
      CustomerMetafieldRecord(..metafield, customer_id: merged.id)
    })
  let with_source_orders =
    store.list_effective_customer_order_summaries(
      with_source_addresses,
      source.id,
    )
    |> list.fold(with_source_addresses, fn(acc, order) {
      store.stage_customer_order_summary(
        acc,
        CustomerOrderSummaryRecord(
          ..order,
          customer_id: Some(merged.id),
          email: merged.email |> option.or(order.email),
        ),
      )
    })
  case copied_source_metafields {
    [] -> with_source_orders
    [_, ..] ->
      store.stage_customer_metafields(
        with_source_orders,
        merged.id,
        list.append(result_metafields, copied_source_metafields),
      )
  }
}

fn customer_metafield_key(metafield: CustomerMetafieldRecord) -> String {
  metafield.namespace <> "::" <> metafield.key
}

fn handle_data_sale_opt_out(store, identity, field, variables) {
  let args = field_args(field, variables)
  let email = read_arg_string(args, "email")
  let existing = case email {
    Some(value) ->
      find_customer_by_email_or_phone(
        store.list_effective_customers(store),
        Some(value),
        None,
      )
    None -> None
  }
  case existing, email {
    Some(customer), _ -> {
      let updated = CustomerRecord(..customer, data_sale_opt_out: True)
      let #(_, next_store) = store.stage_update_customer(store, updated)
      let payload = data_sale_payload(field, Some(updated.id), [])
      #(
        MutationFieldResult(
          get_field_response_key(field),
          payload,
          [updated.id],
          "dataSaleOptOut",
        ),
        next_store,
        identity,
      )
    }
    None, Some(value) -> {
      let #(id, after_id) =
        synthetic_identity.make_synthetic_gid(identity, "Customer")
      let #(timestamp, after_ts) =
        synthetic_identity.make_synthetic_timestamp(after_id)
      let customer =
        CustomerRecord(
          id: id,
          first_name: None,
          last_name: None,
          display_name: Some(value),
          email: Some(value),
          legacy_resource_id: gid_tail(id),
          locale: None,
          note: None,
          can_delete: Some(True),
          verified_email: Some(True),
          data_sale_opt_out: True,
          tax_exempt: Some(False),
          tax_exemptions: [],
          state: Some("DISABLED"),
          tags: [],
          number_of_orders: Some("0"),
          amount_spent: Some(Money("0.0", "USD")),
          default_email_address: Some(CustomerDefaultEmailAddressRecord(
            Some(value),
            None,
            None,
            None,
          )),
          default_phone_number: None,
          email_marketing_consent: None,
          sms_marketing_consent: None,
          default_address: None,
          created_at: Some(timestamp),
          updated_at: Some(timestamp),
        )
      let #(_, next_store) = store.stage_create_customer(store, customer)
      let payload = data_sale_payload(field, Some(id), [])
      #(
        MutationFieldResult(
          get_field_response_key(field),
          payload,
          [id],
          "dataSaleOptOut",
        ),
        next_store,
        after_ts,
      )
    }
    None, None -> {
      let payload =
        data_sale_payload(field, None, [
          UserError(["email"], "Email is invalid", Some("INVALID")),
        ])
      #(
        MutationFieldResult(
          get_field_response_key(field),
          payload,
          [],
          "dataSaleOptOut",
        ),
        store,
        identity,
      )
    }
  }
}

fn customer_payload_json(
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
          #("deletedCustomerId", optional_string_source(deleted_customer_id)),
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

fn address_payload_json(
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
          #("deletedAddressId", optional_string_source(deleted_address_id)),
          #("userErrors", SrcList(list.map(user_errors, user_error_source))),
        ]),
        selections,
        fragments,
      )
    _ -> json.object([])
  }
}

fn address_unknown_result(store, identity, field, fragments, typename, root) {
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

fn store_credit_payload_json(
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

fn order_customer_payload_json(
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

fn merge_payload_json(request, field, fragments) {
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

fn merge_error_payload(field, fragments, error_field, message, code) {
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      project_graphql_value(
        src_object([
          #("__typename", SrcString("CustomerMergePayload")),
          #("resultingCustomerId", SrcNull),
          #("job", SrcNull),
          #(
            "userErrors",
            SrcList([
              user_error_source(UserError(error_field, message, code)),
            ]),
          ),
        ]),
        selections,
        fragments,
      )
    _ -> json.object([])
  }
}

fn data_sale_payload(field, customer_id, errors) {
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      project_graphql_value(
        src_object([
          #("__typename", SrcString("DataSaleOptOutPayload")),
          #("customerId", optional_string_source(customer_id)),
          #("userErrors", SrcList(list.map(errors, user_error_source))),
        ]),
        selections,
        dict.new(),
      )
    _ -> json.object([])
  }
}

fn user_error_source(err: UserError) -> SourceValue {
  src_object([
    #("field", case err.field {
      [] -> SrcNull
      _ -> SrcList(list.map(err.field, SrcString))
    }),
    #("message", SrcString(err.message)),
    #("code", optional_string_source(err.code)),
  ])
}

fn read_obj_addresses(
  input: Dict(String, root_field.ResolvedValue),
) -> List(Dict(String, root_field.ResolvedValue)) {
  case dict.get(input, "addresses") {
    Ok(root_field.ListVal(items)) ->
      list.filter_map(items, fn(item) {
        case item {
          root_field.ObjectVal(d) -> Ok(d)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

fn replace_customer_input_addresses(
  store: Store,
  customer: CustomerRecord,
  input: Dict(String, root_field.ResolvedValue),
) -> #(CustomerRecord, Store) {
  let address_inputs = read_obj_addresses(input)
  case address_inputs {
    [] -> #(customer, store)
    _ -> {
      let store_after_deletes =
        store.list_effective_customer_addresses(store, customer.id)
        |> list.fold(store, fn(acc, address) {
          store.stage_delete_customer_address(acc, address.id)
        })
      let addresses =
        address_inputs
        |> list.index_map(fn(address_input, index) {
          build_address(
            "gid://shopify/MailingAddress/" <> int.to_string(index + 1),
            customer.id,
            index,
            address_input,
            customer.first_name,
            customer.last_name,
          )
        })
        |> dedupe_customer_addresses([])
      let store_after_addresses =
        list.fold(addresses, store_after_deletes, fn(acc, address) {
          let #(_, next_store) =
            store.stage_upsert_customer_address(acc, address)
          next_store
        })
      let default_address =
        addresses
        |> list.first()
        |> result_to_option()
        |> option.map(address_to_default)
      let customer_after_addresses =
        CustomerRecord(..customer, default_address: default_address)
      let #(stored, final_store) =
        store.stage_update_customer(
          store_after_addresses,
          customer_after_addresses,
        )
      #(stored, final_store)
    }
  }
}

fn dedupe_customer_addresses(
  addresses: List(CustomerAddressRecord),
  kept: List(CustomerAddressRecord),
) -> List(CustomerAddressRecord) {
  case addresses {
    [] -> list.reverse(kept)
    [address, ..rest] ->
      case
        list.any(kept, fn(existing) {
          customer_addresses_match(existing, address)
        })
      {
        True -> dedupe_customer_addresses(rest, kept)
        False -> dedupe_customer_addresses(rest, [address, ..kept])
      }
  }
}

fn build_address(
  id: String,
  customer_id: String,
  position: Int,
  input: Dict(String, root_field.ResolvedValue),
  fallback_first_name: Option(String),
  fallback_last_name: Option(String),
) -> CustomerAddressRecord {
  let first_name =
    read_obj_string(input, "firstName") |> option.or(fallback_first_name)
  let last_name =
    read_obj_string(input, "lastName") |> option.or(fallback_last_name)
  let country_code =
    read_obj_string(input, "countryCode")
    |> option.or(read_obj_string(input, "countryCodeV2"))
  let province_code = read_obj_string(input, "provinceCode")
  let country = country_name(country_code, read_obj_string(input, "country"))
  let province =
    province_name(province_code, read_obj_string(input, "province"))
  CustomerAddressRecord(
    id: id,
    customer_id: customer_id,
    cursor: None,
    position: position,
    first_name: first_name,
    last_name: last_name,
    address1: read_obj_string(input, "address1"),
    address2: read_obj_string(input, "address2"),
    city: read_obj_string(input, "city"),
    company: read_obj_string(input, "company"),
    province: province,
    province_code: province_code,
    country: country,
    country_code_v2: country_code,
    zip: read_obj_string(input, "zip"),
    phone: read_obj_string(input, "phone"),
    name: build_display_name(first_name, last_name, None),
    formatted_area: formatted_area(
      read_obj_string(input, "city"),
      province_code,
      country,
    ),
  )
}

fn merge_address(
  existing: CustomerAddressRecord,
  input: Dict(String, root_field.ResolvedValue),
) -> CustomerAddressRecord {
  let first_name =
    read_obj_string(input, "firstName") |> option.or(existing.first_name)
  let last_name =
    read_obj_string(input, "lastName") |> option.or(existing.last_name)
  let country_code =
    read_obj_string(input, "countryCode")
    |> option.or(read_obj_string(input, "countryCodeV2"))
    |> option.or(existing.country_code_v2)
  let province_code =
    read_obj_string(input, "provinceCode") |> option.or(existing.province_code)
  let country =
    country_name(
      country_code,
      read_obj_string(input, "country") |> option.or(existing.country),
    )
  let province =
    province_name(
      province_code,
      read_obj_string(input, "province") |> option.or(existing.province),
    )
  CustomerAddressRecord(
    ..existing,
    first_name: first_name,
    last_name: last_name,
    address1: read_obj_string(input, "address1") |> option.or(existing.address1),
    address2: read_obj_string(input, "address2") |> option.or(existing.address2),
    city: read_obj_string(input, "city") |> option.or(existing.city),
    company: read_obj_string(input, "company") |> option.or(existing.company),
    province: province,
    province_code: province_code,
    country: country,
    country_code_v2: country_code,
    zip: read_obj_string(input, "zip") |> option.or(existing.zip),
    phone: read_obj_string(input, "phone") |> option.or(existing.phone),
    name: build_display_name(first_name, last_name, None),
    formatted_area: formatted_area(
      read_obj_string(input, "city") |> option.or(existing.city),
      province_code,
      country,
    ),
  )
}

fn read_customer_metafields(input, customer_id, _identity) {
  case dict.get(input, "metafields") {
    Ok(root_field.ListVal(items)) ->
      list.index_map(items, fn(item, index) {
        case item {
          root_field.ObjectVal(obj) -> {
            let namespace =
              read_obj_string(obj, "namespace") |> option.unwrap("")
            let key = read_obj_string(obj, "key") |> option.unwrap("")
            let id =
              "gid://shopify/Metafield/"
              <> gid_tail(customer_id) |> option.unwrap("0")
              <> "-"
              <> int.to_string(index + 1)
            Ok(CustomerMetafieldRecord(
              id: id,
              customer_id: customer_id,
              namespace: namespace,
              key: key,
              type_: read_obj_string(obj, "type")
                |> option.unwrap("single_line_text_field"),
              value: read_obj_string(obj, "value") |> option.unwrap(""),
              compare_digest: None,
              created_at: None,
              updated_at: None,
            ))
          }
          _ -> Error(Nil)
        }
      })
      |> list.filter_map(fn(x) { x })
    _ -> []
  }
}

fn read_nested_object(input, key) {
  case dict.get(input, key) {
    Ok(root_field.ObjectVal(d)) -> d
    _ -> dict.new()
  }
}

fn has_nested_object(input, key) {
  case dict.get(input, key) {
    Ok(root_field.ObjectVal(_)) -> True
    _ -> False
  }
}

fn make_email_consent(input) {
  make_email_consent_from(read_nested_object(input, "emailMarketingConsent"))
}

fn make_email_consent_from(consent) {
  case dict.is_empty(consent) {
    True -> None
    False ->
      Some(CustomerEmailMarketingConsentRecord(
        marketing_state: read_obj_string(consent, "marketingState"),
        marketing_opt_in_level: read_obj_string(consent, "marketingOptInLevel"),
        consent_updated_at: read_obj_string(consent, "consentUpdatedAt"),
      ))
  }
}

fn make_sms_consent(input) {
  make_sms_consent_from(read_nested_object(input, "smsMarketingConsent"))
}

fn make_sms_consent_from(consent) {
  case dict.is_empty(consent) {
    True -> None
    False ->
      Some(CustomerSmsMarketingConsentRecord(
        marketing_state: read_obj_string(consent, "marketingState"),
        marketing_opt_in_level: read_obj_string(consent, "marketingOptInLevel"),
        consent_updated_at: read_obj_string(consent, "consentUpdatedAt"),
        consent_collected_from: Some("OTHER"),
      ))
  }
}

fn consent_state_from_input(input, key) {
  read_obj_string(read_nested_object(input, key), "marketingState")
}

fn consent_level_from_input(input, key) {
  read_obj_string(read_nested_object(input, key), "marketingOptInLevel")
}

fn consent_updated_at_from_input(input, key) {
  read_obj_string(read_nested_object(input, key), "consentUpdatedAt")
}

fn consent_collected_from_input(input, key) {
  read_obj_string(read_nested_object(input, key), "consentCollectedFrom")
}

fn build_merged_customer(
  one: CustomerRecord,
  two: CustomerRecord,
  override: Dict(String, root_field.ResolvedValue),
  timestamp: String,
) -> CustomerRecord {
  let email_source =
    read_customer_id_override(override, "customerIdOfEmailToKeep", one, two)
    |> option.unwrap(two)
  let phone_source =
    read_customer_id_override(
      override,
      "customerIdOfPhoneNumberToKeep",
      one,
      two,
    )
    |> option.unwrap(two)
  let email =
    read_obj_string(override, "email")
    |> option.or(email_source.email)
  let first_name =
    read_obj_string(override, "firstName")
    |> option.or(
      select_customer_override_field(
        override,
        "customerIdOfFirstNameToKeep",
        one,
        two,
        fn(customer) { customer.first_name },
      ),
    )
  let last_name =
    read_obj_string(override, "lastName")
    |> option.or(
      select_customer_override_field(
        override,
        "customerIdOfLastNameToKeep",
        one,
        two,
        fn(customer) { customer.last_name },
      ),
    )
  let default_address =
    select_customer_override_field(
      override,
      "customerIdOfDefaultAddressToKeep",
      one,
      two,
      fn(customer) { customer.default_address },
    )
  CustomerRecord(
    ..two,
    first_name: first_name,
    last_name: last_name,
    display_name: build_display_name(first_name, last_name, email),
    email: email,
    note: read_obj_string(override, "note")
      |> option.or(two.note)
      |> option.or(one.note),
    tags: case read_obj_array_strings(override, "tags") {
      [] -> normalize_tags(list.append(one.tags, two.tags))
      tags -> normalize_tags(tags)
    },
    default_email_address: email_source.default_email_address,
    default_phone_number: phone_source.default_phone_number,
    email_marketing_consent: email_source.email_marketing_consent,
    sms_marketing_consent: phone_source.sms_marketing_consent,
    default_address: default_address,
    created_at: one.created_at,
    updated_at: Some(timestamp),
  )
}

fn read_customer_id_override(
  override: Dict(String, root_field.ResolvedValue),
  field: String,
  one: CustomerRecord,
  two: CustomerRecord,
) -> Option(CustomerRecord) {
  case read_obj_string(override, field) {
    Some(id) ->
      case id == one.id, id == two.id {
        True, _ -> Some(one)
        _, True -> Some(two)
        _, _ -> None
      }
    None -> None
  }
}

fn select_customer_override_field(
  override: Dict(String, root_field.ResolvedValue),
  field: String,
  one: CustomerRecord,
  two: CustomerRecord,
  selector: fn(CustomerRecord) -> a,
) -> a {
  let source =
    read_customer_id_override(override, field, one, two)
    |> option.unwrap(two)
  selector(source)
}

fn read_money(input) -> Option(Money) {
  let amount_obj =
    first_non_empty_object([
      read_nested_object(input, "amount"),
      read_nested_object(input, "creditAmount"),
      read_nested_object(input, "debitAmount"),
    ])
  case
    read_obj_string(amount_obj, "amount"),
    read_obj_string(amount_obj, "currencyCode")
  {
    Some(amount), Some(currency) -> Some(Money(amount, currency))
    _, _ -> None
  }
}

fn first_non_empty_object(
  objects: List(Dict(String, root_field.ResolvedValue)),
) -> Dict(String, root_field.ResolvedValue) {
  case objects {
    [] -> dict.new()
    [first, ..rest] ->
      case dict.is_empty(first) {
        True -> first_non_empty_object(rest)
        False -> first
      }
  }
}

fn parse_cents(amount: String) -> Int {
  case float.parse(amount) {
    Ok(value) -> float.round(value *. 100.0)
    Error(_) ->
      case int.parse(amount) {
        Ok(value) -> value * 100
        Error(_) -> 0
      }
  }
}

fn format_cents(cents: Int) -> String {
  let whole = cents / 100
  let frac = int.absolute_value(cents % 100)
  case frac {
    0 -> int.to_string(whole) <> ".0"
    n if n < 10 -> int.to_string(whole) <> ".0" <> int.to_string(n)
    n -> int.to_string(whole) <> "." <> int.to_string(n)
  }
}

fn build_display_name(first_name, last_name, email) {
  let name =
    string.trim(string.join(
      [option.unwrap(first_name, ""), option.unwrap(last_name, "")],
      " ",
    ))
  case name {
    "" -> email
    _ -> Some(name)
  }
}

fn split_tags(raw: String) -> List(String) {
  raw
  |> string.split(",")
  |> list.map(string.trim)
  |> list.filter(fn(s) { s != "" })
}

fn normalize_tags(tags: List(String)) -> List(String) {
  tags
  |> list.map(string.trim)
  |> list.filter(fn(s) { s != "" })
  |> dedupe()
  |> list.sort(fn(a, b) {
    string.compare(string.lowercase(a), string.lowercase(b))
  })
}

fn dedupe(items: List(String)) -> List(String) {
  list.fold(items, [], fn(acc, item) {
    case list.contains(acc, item) {
      True -> acc
      False -> list.append(acc, [item])
    }
  })
}

fn country_name(code, fallback) {
  case code {
    Some("CA") -> Some("Canada")
    Some("US") -> Some("United States")
    _ -> fallback
  }
}

fn province_name(code, fallback) {
  case code {
    Some("ON") -> Some("Ontario")
    Some("QC") -> Some("Quebec")
    Some("BC") -> Some("British Columbia")
    Some("CA") -> Some("California")
    Some("NY") -> Some("New York")
    _ -> fallback
  }
}

fn formatted_area(city, province_code, country) {
  let city_region =
    [city, province_code]
    |> list.filter_map(non_empty_option_string)
    |> string.join(" ")
  let parts =
    [
      case city_region {
        "" -> None
        value -> Some(value)
      },
      country,
    ]
    |> list.filter_map(non_empty_option_string)
  case parts {
    [] -> None
    _ -> Some(string.join(parts, ", "))
  }
}

fn non_empty_option_string(value: Option(String)) -> Result(String, Nil) {
  case value {
    Some(s) if s != "" -> Ok(s)
    _ -> Error(Nil)
  }
}

fn find_duplicate_customer_address(
  store: Store,
  customer_id: String,
  candidate: CustomerAddressRecord,
  exclude_address_id: Option(String),
) -> Option(CustomerAddressRecord) {
  store.list_effective_customer_addresses(store, customer_id)
  |> list.find(fn(address) {
    address.id != option.unwrap(exclude_address_id, "")
    && customer_addresses_match(address, candidate)
  })
  |> result_to_option()
}

fn customer_addresses_match(
  left: CustomerAddressRecord,
  right: CustomerAddressRecord,
) -> Bool {
  left.first_name == right.first_name
  && left.last_name == right.last_name
  && left.address1 == right.address1
  && left.address2 == right.address2
  && left.city == right.city
  && left.company == right.company
  && left.province_code == right.province_code
  && left.country_code_v2 == right.country_code_v2
  && left.zip == right.zip
  && left.phone == right.phone
}

fn gid_tail(id: String) -> Option(String) {
  string.split(id, "/")
  |> list.last()
  |> result_to_option()
}

fn result_to_option(value: Result(a, b)) -> Option(a) {
  case value {
    Ok(v) -> Some(v)
    Error(_) -> None
  }
}

fn record_mutation_log(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  staged_ids: List(String),
  roots: List(String),
) -> #(Store, SyntheticIdentityRegistry) {
  let #(log_id, identity_after_log_id) =
    synthetic_identity.make_synthetic_gid(identity, "MutationLogEntry")
  let #(received_at, identity_final) =
    synthetic_identity.make_synthetic_timestamp(identity_after_log_id)
  let primary = list.first(roots) |> result_to_option()
  let entry =
    store.MutationLogEntry(
      id: log_id,
      received_at: received_at,
      operation_name: None,
      path: request_path,
      query: document,
      variables: dict.new(),
      staged_resource_ids: staged_ids,
      status: store.Staged,
      interpreted: store.InterpretedMetadata(
        operation_type: store.Mutation,
        operation_name: None,
        root_fields: roots,
        primary_root_field: primary,
        capability: store.Capability(
          operation_name: primary,
          domain: "customers",
          execution: "stage-locally",
        ),
      ),
      notes: Some(
        "Locally staged customer-domain mutation in shopify-draft-proxy.",
      ),
    )
  #(store.record_mutation_log_entry(store, entry), identity_final)
}

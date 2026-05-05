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
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, ConnectionPageInfoOptions,
  ConnectionWindow, SelectedFieldOptions, SerializeConnectionConfig, SrcBool,
  SrcInt, SrcList, SrcNull, SrcObject, SrcString,
  default_connection_page_info_options, default_connection_window_options,
  get_document_fragments, get_field_response_key, paginate_connection_items,
  project_graphql_value, serialize_connection, src_object,
}
import shopify_draft_proxy/proxy/mutation_helpers.{
  type MutationOutcome, MutationOutcome,
}
import shopify_draft_proxy/proxy/passthrough
import shopify_draft_proxy/proxy/payments
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response, LiveHybrid, Response,
}
import shopify_draft_proxy/proxy/upstream_query.{
  type UpstreamContext, empty_upstream_context,
}
import shopify_draft_proxy/search_query_parser
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry, is_proxy_synthetic_gid,
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
  CustomerAccountPageRecord, CustomerAddressRecord,
  CustomerCatalogPageInfoRecord, CustomerDefaultAddressRecord,
  CustomerDefaultEmailAddressRecord, CustomerDefaultPhoneNumberRecord,
  CustomerEmailMarketingConsentRecord, CustomerMergeRequestRecord,
  CustomerMetafieldRecord, CustomerOrderSummaryRecord, CustomerRecord,
  CustomerSmsMarketingConsentRecord, Money, StoreCreditAccountRecord,
  StoreCreditAccountTransactionRecord,
}

pub type CustomersError {
  ParseFailed(root_field.RootFieldError)
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

type CustomerHydrateResult {
  CustomerHydrateResult(
    customer: CustomerRecord,
    addresses: List(CustomerAddressRecord),
    metafields: List(CustomerMetafieldRecord),
    orders: List(CustomerOrderSummaryRecord),
    store_credit_accounts: List(StoreCreditAccountRecord),
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

/// True iff the requested `customer(id:)` argument resolves to a
/// customer that's already in local state (base or staged). Used by
/// the dispatcher to skip `LiveHybrid` passthrough when a prior
/// staged mutation has already produced the record we'd otherwise
/// fetch — e.g. a `customerCreate` followed by `customer(id: <newly
/// staged synthetic gid>)` in the same scenario.
pub fn local_has_customer_id(
  proxy: DraftProxy,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  dict.values(variables)
  |> list.any(fn(value) {
    case value {
      root_field.StringVal(id) ->
        is_proxy_synthetic_gid(id) || local_customer_id_known(proxy.store, id)
      _ -> False
    }
  })
}

fn local_customer_id_known(store: Store, id: String) -> Bool {
  case store.get_effective_customer_by_id(store, id) {
    Some(_) -> True
    None ->
      case dict.get(store.staged_state.deleted_customer_ids, id) {
        Ok(True) -> True
        _ ->
          case dict.get(store.staged_state.merged_customer_ids, id) {
            Ok(_) -> True
            Error(_) -> False
          }
      }
  }
}

/// In `LiveHybrid` mode, decide whether this customer-domain
/// operation should be answered by reaching upstream verbatim instead
/// of from local state. Internal helper for `handle_query_request` —
/// the dispatcher does not consult this directly anymore.
///
/// The customer-domain operations on this list are aggregates and
/// catalog reads that the local handler can't compute the right
/// answer for without reaching upstream. `customer(id:)` only
/// passes through when the requested id isn't in local state — a
/// staged-create-then-read flow stays local end-to-end.
///
/// In `Snapshot` mode the same operations stay local (typically
/// with a degenerate empty answer that matches empty-snapshot
/// expectations).
fn should_passthrough_in_live_hybrid(
  proxy: DraftProxy,
  type_: parse_operation.GraphQLOperationType,
  primary_root_field: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  case type_, primary_root_field {
    parse_operation.QueryOperation, "customersCount" -> True
    parse_operation.QueryOperation, "customerByIdentifier" -> True
    parse_operation.QueryOperation, "customer" ->
      !local_has_customer_id(proxy, variables)
    parse_operation.QueryOperation, "customers" -> True
    _, _ -> False
  }
}

fn request_upstream_context(
  proxy: DraftProxy,
  request: Request,
) -> UpstreamContext {
  upstream_query.UpstreamContext(
    transport: proxy.upstream_transport,
    origin: proxy.config.shopify_admin_origin,
    headers: request.headers,
  )
}

/// Domain entrypoint for the customer query path. The dispatcher
/// always lands here for customer-domain reads regardless of
/// `read_mode`; the handler itself decides whether to compute the
/// answer from local state or to forward to upstream verbatim via
/// `passthrough.passthrough_sync` (when in `LiveHybrid` mode and the
/// operation is one we know we can't satisfy locally — see
/// `should_passthrough_in_live_hybrid`).
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
      should_passthrough_in_live_hybrid(
        proxy,
        parsed.type_,
        primary_root_field,
        variables,
      )
    _ -> False
  }
  case want_passthrough {
    True -> passthrough.passthrough_sync(proxy, request)
    False ->
      case
        process_with_upstream(
          proxy,
          document,
          variables,
          request_upstream_context(proxy, request),
        )
      {
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
                      #(
                        "message",
                        json.string("Failed to handle customers query"),
                      ),
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
    | "orderCustomerRemove" -> True
    _ -> False
  }
}

pub fn handle_customer_query(
  proxy: DraftProxy,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, CustomersError) {
  case root_field.get_root_fields(document) {
    Error(err) -> Error(ParseFailed(err))
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      Ok(serialize_root_fields(
        proxy,
        fields,
        fragments,
        variables,
        empty_upstream_context(),
      ))
    }
  }
}

pub fn process(
  proxy: DraftProxy,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, CustomersError) {
  process_with_upstream(proxy, document, variables, empty_upstream_context())
}

fn process_with_upstream(
  proxy: DraftProxy,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> Result(Json, CustomersError) {
  case root_field.get_root_fields(document) {
    Error(err) -> Error(ParseFailed(err))
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      let data =
        serialize_root_fields(proxy, fields, fragments, variables, upstream)
      let search_extensions =
        customer_count_search_extensions(fields, variables)
      Ok(wrap_query_payload(data, search_extensions))
    }
  }
}

fn wrap_query_payload(data: Json, search_extensions: List(Json)) -> Json {
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

fn root_payload_for_field(
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

fn default_phone_source(
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

fn email_consent_source(
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

fn sms_consent_source(
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

fn address_source(address: CustomerAddressRecord) -> SourceValue {
  default_address_source(Some(address_to_default(address)))
}

fn customer_to_source(store: Store, customer: CustomerRecord) -> SourceValue {
  let customer_metafields =
    store.get_effective_metafields_by_customer_id(store, customer.id)
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
    #(
      "compareDigest",
      graphql_helpers.option_string_source(metafield.compare_digest),
    ),
    #("createdAt", graphql_helpers.option_string_source(metafield.created_at)),
    #("updatedAt", graphql_helpers.option_string_source(metafield.updated_at)),
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

fn customer_owner_source(customer: CustomerRecord) -> SourceValue {
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

fn serialize_customer_by_id(
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

fn serialize_customer_by_identifier(
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

fn customer_matches_search_term(
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

/// `customersCount` — in `LiveHybrid` mode the dispatcher passes the
/// request straight through to upstream (see
/// `live_hybrid_passthrough_target` in `draft_proxy.gleam`); the local
/// store can't know the real total. This handler only runs in
/// `Snapshot` mode and returns the count of effective local customers
/// (typically zero against an empty store).
fn serialize_customers_count(
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

fn local_customers_count_source(proxy: DraftProxy) -> SourceValue {
  let count = store.list_effective_customers(proxy.store) |> list.length()
  src_object([#("count", SrcInt(count)), #("precision", SrcString("EXACT"))])
}

fn fetch_customers_count_source(
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

fn customers_count_source_from_hydrate(
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

fn serialize_customer_account_page(
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

fn serialize_customer_account_pages(
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

fn fetch_customer_account_pages(
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

fn serialize_customer_payment_method_by_id(
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

fn serialize_customer_merge_preview(
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

fn serialize_customer_merge_job_status(
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
  proxy: DraftProxy,
  request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> MutationOutcome {
  let store = proxy.store
  let identity = proxy.synthetic_identity
  case root_field.get_root_fields(document) {
    Error(err) ->
      mutation_helpers.parse_failed_outcome(
        proxy.store,
        proxy.synthetic_identity,
        err,
      )
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      handle_mutation_fields(
        store,
        identity,
        request_path,
        document,
        fields,
        fragments,
        variables,
        upstream,
      )
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
  upstream: UpstreamContext,
) -> MutationOutcome {
  case
    first_disallowed_marketing_consent_update_state_error(
      fields,
      variables,
      document,
    )
  {
    Some(#(root_name, error_json, include_null_data)) -> {
      let entries = case include_null_data {
        True -> [
          #("errors", json.array([error_json], fn(x) { x })),
          #("data", json.object([#(root_name, json.null())])),
        ]
        False -> [#("errors", json.array([error_json], fn(x) { x }))]
      }
      MutationOutcome(
        data: json.object(entries),
        store: store,
        identity: identity,
        staged_resource_ids: [],
        log_drafts: [],
      )
    }
    None ->
      case first_customer_merge_missing_argument_error(fields, variables) {
        Some(error_json) ->
          MutationOutcome(
            data: json.object([
              #("errors", json.array([error_json], fn(x) { x })),
            ]),
            store: store,
            identity: identity,
            staged_resource_ids: [],
            log_drafts: [],
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
                upstream,
              )
          }
      }
  }
}

fn first_disallowed_marketing_consent_update_state_error(
  fields: List(Selection),
  variables: Dict(String, root_field.ResolvedValue),
  document: String,
) -> Option(#(String, Json, Bool)) {
  case fields {
    [] -> None
    [field, ..rest] ->
      case field {
        Field(name: name, ..) as selected_field ->
          case marketing_consent_input_key(name.value) {
            Some(consent_key) -> {
              let args = graphql_helpers.field_args(selected_field, variables)
              let input =
                graphql_helpers.read_arg_object(args, "input")
                |> option.unwrap(dict.new())
              let consent = read_nested_object(input, consent_key)
              case read_obj_string(consent, "marketingState") {
                Some(value) ->
                  case is_allowed_marketing_consent_update_state(value) {
                    True ->
                      first_disallowed_marketing_consent_update_state_error(
                        rest,
                        variables,
                        document,
                      )
                    False -> {
                      let #(error_json, include_null_data) =
                        marketing_consent_update_state_error(
                          name.value,
                          consent_key,
                          value,
                          input,
                          selected_field,
                          document,
                        )
                      Some(#(name.value, error_json, include_null_data))
                    }
                  }
                None ->
                  first_disallowed_marketing_consent_update_state_error(
                    rest,
                    variables,
                    document,
                  )
              }
            }
            None ->
              first_disallowed_marketing_consent_update_state_error(
                rest,
                variables,
                document,
              )
          }
        _ ->
          first_disallowed_marketing_consent_update_state_error(
            rest,
            variables,
            document,
          )
      }
  }
}

fn marketing_consent_input_key(root_name: String) -> Option(String) {
  case root_name {
    "customerEmailMarketingConsentUpdate" -> Some("emailMarketingConsent")
    "customerSmsMarketingConsentUpdate" -> Some("smsMarketingConsent")
    _ -> None
  }
}

fn is_allowed_marketing_consent_update_state(state: String) -> Bool {
  case state {
    "SUBSCRIBED" | "UNSUBSCRIBED" | "PENDING" -> True
    _ -> False
  }
}

fn marketing_consent_update_state_error(
  root_name: String,
  consent_key: String,
  state: String,
  input: Dict(String, root_field.ResolvedValue),
  field: Selection,
  document: String,
) -> #(Json, Bool) {
  case root_name, state {
    "customerSmsMarketingConsentUpdate", "INVALID" -> #(
      invalid_sms_marketing_state_variable_error(
        consent_key,
        state,
        input,
        field,
        document,
      ),
      False,
    )
    _, _ -> #(
      json.object([
        #(
          "message",
          json.string(
            "Cannot specify " <> state <> " as a marketing state input",
          ),
        ),
        #("locations", graphql_helpers.field_locations_json(field, document)),
        #("extensions", json.object([#("code", json.string("INVALID"))])),
        #("path", json.array([json.string(root_name)], fn(x) { x })),
      ]),
      True,
    )
  }
}

fn invalid_sms_marketing_state_variable_error(
  consent_key: String,
  state: String,
  input: Dict(String, root_field.ResolvedValue),
  field: Selection,
  document: String,
) -> Json {
  let input_type = "CustomerSmsMarketingConsentUpdateInput"
  let allowed_values =
    "NOT_SUBSCRIBED, PENDING, SUBSCRIBED, UNSUBSCRIBED, REDACTED"
  let explanation =
    "Expected \"" <> state <> "\" to be one of: " <> allowed_values
  json.object([
    #(
      "message",
      json.string(
        "Variable $input of type "
        <> input_type
        <> "! was provided invalid value for "
        <> consent_key
        <> ".marketingState ("
        <> explanation
        <> ")",
      ),
    ),
    #("locations", graphql_helpers.field_locations_json(field, document)),
    #(
      "extensions",
      json.object([
        #("code", json.string("INVALID_VARIABLE")),
        #(
          "value",
          root_field.resolved_value_to_json(root_field.ObjectVal(input)),
        ),
        #(
          "problems",
          json.array(
            [
              json.object([
                #(
                  "path",
                  json.array(
                    [json.string(consent_key), json.string("marketingState")],
                    fn(x) { x },
                  ),
                ),
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
              let args = graphql_helpers.field_args(field, variables)
              let missing =
                [
                  #(
                    "customerOneId",
                    graphql_helpers.read_arg_string_nonempty(
                      args,
                      "customerOneId",
                    ),
                  ),
                  #(
                    "customerTwoId",
                    graphql_helpers.read_arg_string_nonempty(
                      args,
                      "customerTwoId",
                    ),
                  ),
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
  upstream: UpstreamContext,
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
              upstream,
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
    log_drafts: [],
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
              let args = graphql_helpers.field_args(field, variables)
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
  upstream: UpstreamContext,
) -> Option(#(MutationFieldResult, Store, SyntheticIdentityRegistry)) {
  let #(store, identity) =
    hydrate_before_customer_mutation(
      store,
      identity,
      field,
      root_name,
      variables,
      upstream,
    )
  case root_name {
    "customerCreate" ->
      Some(handle_customer_create(
        store,
        identity,
        field,
        fragments,
        variables,
        upstream,
      ))
    "customerUpdate" ->
      Some(handle_customer_update(
        store,
        identity,
        field,
        fragments,
        variables,
        upstream,
      ))
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
    _ -> None
  }
}

fn hydrate_before_customer_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  root_name: String,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> #(Store, SyntheticIdentityRegistry) {
  let args = graphql_helpers.field_args(field, variables)
  case root_name {
    "customerUpdate" | "customerDelete" ->
      hydrate_optional_customer_id(
        store,
        identity,
        graphql_helpers.read_arg_object(args, "input")
          |> option.unwrap(dict.new())
          |> read_obj_string("id"),
        upstream,
      )
    "customerAddTaxExemptions"
    | "customerRemoveTaxExemptions"
    | "customerReplaceTaxExemptions"
    | "customerRequestDataErasure"
    | "customerCancelDataErasure" ->
      hydrate_optional_customer_id(
        store,
        identity,
        graphql_helpers.read_arg_string_nonempty(args, "customerId"),
        upstream,
      )
    "customerEmailMarketingConsentUpdate"
    | "customerSmsMarketingConsentUpdate" ->
      hydrate_optional_customer_id(
        store,
        identity,
        graphql_helpers.read_arg_object(args, "input")
          |> option.unwrap(dict.new())
          |> read_obj_string("customerId"),
        upstream,
      )
    "customerMerge" -> {
      let ids = [
        graphql_helpers.read_arg_string_nonempty(args, "customerOneId"),
        graphql_helpers.read_arg_string_nonempty(args, "customerTwoId"),
      ]
      list.fold(ids, #(store, identity), fn(acc, id) {
        let #(current_store, current_identity) = acc
        hydrate_optional_customer_id(
          current_store,
          current_identity,
          id,
          upstream,
        )
      })
    }
    "storeCreditAccountCredit" | "storeCreditAccountDebit" ->
      hydrate_optional_store_credit_account_id(
        store,
        identity,
        graphql_helpers.read_arg_string_nonempty(args, "id"),
        upstream,
      )
    "orderCustomerSet" -> {
      let with_order =
        hydrate_optional_customer_order_id(
          store,
          graphql_helpers.read_arg_string_nonempty(args, "orderId"),
          upstream,
        )
      hydrate_optional_customer_id(
        with_order,
        identity,
        graphql_helpers.read_arg_string_nonempty(args, "customerId"),
        upstream,
      )
    }
    "orderCustomerRemove" -> #(
      hydrate_optional_customer_order_id(
        store,
        graphql_helpers.read_arg_string_nonempty(args, "orderId"),
        upstream,
      ),
      identity,
    )
    _ -> #(store, identity)
  }
}

fn hydrate_optional_customer_id(
  store: Store,
  identity: SyntheticIdentityRegistry,
  id: Option(String),
  upstream: UpstreamContext,
) -> #(Store, SyntheticIdentityRegistry) {
  case id {
    Some(customer_id) ->
      maybe_hydrate_customer(store, identity, customer_id, upstream)
    None -> #(store, identity)
  }
}

fn maybe_hydrate_customer(
  store: Store,
  identity: SyntheticIdentityRegistry,
  customer_id: String,
  upstream: UpstreamContext,
) -> #(Store, SyntheticIdentityRegistry) {
  case is_proxy_synthetic_gid(customer_id) {
    True -> #(store, identity)
    False ->
      case store.get_effective_customer_by_id(store, customer_id) {
        Some(_) -> #(store, identity)
        None -> {
          // Pattern 2: existing-customer mutations start cold in
          // LiveHybrid parity, so they fetch the prior customer and
          // persist it as base state before applying local-only staged
          // changes. Snapshot/no-transport mode keeps the legacy
          // unknown-customer behavior.
          let query = customer_hydrate_query()
          let variables = json.object([#("id", json.string(customer_id))])
          case
            upstream_query.fetch_sync(
              upstream.origin,
              upstream.transport,
              upstream.headers,
              "CustomerHydrate",
              query,
              variables,
            )
          {
            Ok(value) ->
              case customer_hydrate_result(value, customer_id) {
                Some(result) -> {
                  let with_customer =
                    store.upsert_base_customers(store, [result.customer])
                  let with_addresses =
                    store.upsert_base_customer_addresses(
                      with_customer,
                      result.addresses,
                    )
                  let with_metafields = case result.metafields {
                    [] -> with_addresses
                    metafields ->
                      store.stage_customer_metafields(
                        with_addresses,
                        result.customer.id,
                        metafields,
                      )
                  }
                  let with_accounts =
                    list.fold(
                      result.store_credit_accounts,
                      with_metafields,
                      fn(acc, account) {
                        store.stage_store_credit_account(acc, account)
                      },
                    )
                  let with_orders =
                    store.upsert_base_customer_order_summaries(
                      with_accounts,
                      result.orders,
                    )
                  #(with_orders, identity)
                }
                None -> #(store, identity)
              }
            Error(_) -> #(store, identity)
          }
        }
      }
  }
}

fn hydrate_optional_store_credit_account_id(
  store: Store,
  identity: SyntheticIdentityRegistry,
  id: Option(String),
  upstream: UpstreamContext,
) -> #(Store, SyntheticIdentityRegistry) {
  case id {
    Some(account_id) ->
      case store.get_effective_store_credit_account_by_id(store, account_id) {
        Some(_) -> #(store, identity)
        None -> {
          // Pattern 2: store-credit adjustments need the existing
          // account balance/owner before staging the local transaction.
          // In Snapshot or without a cassette this stays a no-op and
          // the handler returns Shopify-like NOT_FOUND.
          let query = store_credit_account_hydrate_query()
          let variables = json.object([#("id", json.string(account_id))])
          case
            upstream_query.fetch_sync(
              upstream.origin,
              upstream.transport,
              upstream.headers,
              "StoreCreditAccountHydrate",
              query,
              variables,
            )
          {
            Ok(value) ->
              case store_credit_account_hydrate_result(value, account_id) {
                Some(#(customer, account)) -> {
                  let with_customer =
                    store.upsert_base_customers(store, [customer])
                  let with_account =
                    store.stage_store_credit_account(with_customer, account)
                  #(with_account, identity)
                }
                None -> #(store, identity)
              }
            Error(_) -> #(store, identity)
          }
        }
      }
    None -> #(store, identity)
  }
}

fn hydrate_optional_customer_order_id(
  store: Store,
  id: Option(String),
  upstream: UpstreamContext,
) -> Store {
  case id {
    Some(order_id) ->
      case store.get_effective_customer_order_summary_by_id(store, order_id) {
        Some(_) -> store
        None -> {
          // Pattern 2: orderCustomerSet/orderCustomerRemove need the
          // existing order summary before staging a local customer
          // association change. In snapshot/no-cassette mode this stays
          // a no-op and the handler returns Shopify-like NOT_FOUND.
          let query = customer_order_summary_hydrate_query()
          let variables = json.object([#("id", json.string(order_id))])
          case
            upstream_query.fetch_sync(
              upstream.origin,
              upstream.transport,
              upstream.headers,
              "CustomerOrderSummaryHydrate",
              query,
              variables,
            )
          {
            Ok(value) ->
              case customer_order_summary_from_hydrate(value, order_id) {
                Some(order) -> store.stage_customer_order_summary(store, order)
                None -> store
              }
            Error(_) -> store
          }
        }
      }
    None -> store
  }
}

fn customer_hydrate_query() -> String {
  "query CustomerHydrate($id: ID!) {\n"
  <> "  customer(id: $id) {\n"
  <> "    id firstName lastName displayName email legacyResourceId locale note\n"
  <> "    canDelete verifiedEmail dataSaleOptOut taxExempt taxExemptions state tags\n"
  <> "    numberOfOrders createdAt updatedAt\n"
  <> "    amountSpent { amount currencyCode }\n"
  <> "    defaultEmailAddress { emailAddress marketingState marketingOptInLevel marketingUpdatedAt }\n"
  <> "    defaultPhoneNumber { phoneNumber marketingState marketingOptInLevel marketingUpdatedAt marketingCollectedFrom }\n"
  <> "    emailMarketingConsent { marketingState marketingOptInLevel consentUpdatedAt }\n"
  <> "    smsMarketingConsent { marketingState marketingOptInLevel consentUpdatedAt consentCollectedFrom }\n"
  <> "    defaultAddress { id firstName lastName address1 address2 city company province provinceCode country countryCodeV2 zip phone name formattedArea }\n"
  <> "    addressesV2(first: 250) { nodes { id firstName lastName address1 address2 city company province provinceCode country countryCodeV2 zip phone name formattedArea } }\n"
  <> "    metafields(first: 250) { nodes { id namespace key type value compareDigest createdAt updatedAt } }\n"
  <> "    orders(first: 10, sortKey: CREATED_AT, reverse: true) { nodes { id name email createdAt currentTotalPriceSet { shopMoney { amount currencyCode } } } pageInfo { startCursor endCursor } }\n"
  <> "    storeCreditAccounts(first: 50) { nodes { id balance { amount currencyCode } } }\n"
  <> "  }\n"
  <> "}\n"
}

fn customer_order_summary_hydrate_query() -> String {
  "query CustomerOrderSummaryHydrate($id: ID!) {\n"
  <> "  order(id: $id) {\n"
  <> "    id name email createdAt\n"
  <> "    currentTotalPriceSet { shopMoney { amount currencyCode } }\n"
  <> "    customer { id }\n"
  <> "  }\n"
  <> "}\n"
}

fn store_credit_account_hydrate_query() -> String {
  "query StoreCreditAccountHydrate($id: ID!) {\n"
  <> "  storeCreditAccount(id: $id) {\n"
  <> "    id\n"
  <> "    balance { amount currencyCode }\n"
  <> "    owner { ... on Customer { id firstName lastName displayName email legacyResourceId locale note canDelete verifiedEmail dataSaleOptOut taxExempt taxExemptions state tags numberOfOrders createdAt updatedAt amountSpent { amount currencyCode } defaultEmailAddress { emailAddress marketingState marketingOptInLevel marketingUpdatedAt } defaultPhoneNumber { phoneNumber marketingState marketingOptInLevel marketingUpdatedAt marketingCollectedFrom } emailMarketingConsent { marketingState marketingOptInLevel consentUpdatedAt } smsMarketingConsent { marketingState marketingOptInLevel consentUpdatedAt consentCollectedFrom } defaultAddress { id firstName lastName address1 address2 city company province provinceCode country countryCodeV2 zip phone name formattedArea } } }\n"
  <> "  }\n"
  <> "}\n"
}

fn customer_hydrate_result(
  value: commit.JsonValue,
  fallback_id: String,
) -> Option(CustomerHydrateResult) {
  use data <- option.then(json_get(value, "data"))
  use customer_node <- option.then(non_null_json(json_get(data, "customer")))
  use customer <- option.then(customer_record_from_node(
    customer_node,
    fallback_id,
  ))
  let addresses = address_records_from_customer_node(customer_node, customer.id)
  let metafields =
    metafield_records_from_customer_node(customer_node, customer.id)
  let orders = order_records_from_customer_node(customer_node, customer.id)
  let accounts =
    store_credit_accounts_from_customer_node(customer_node, customer.id)
  Some(CustomerHydrateResult(
    customer: customer,
    addresses: addresses,
    metafields: metafields,
    orders: orders,
    store_credit_accounts: accounts,
  ))
}

fn store_credit_account_hydrate_result(
  value: commit.JsonValue,
  fallback_account_id: String,
) -> Option(#(CustomerRecord, StoreCreditAccountRecord)) {
  use data <- option.then(json_get(value, "data"))
  use account_node <- option.then(
    non_null_json(json_get(data, "storeCreditAccount")),
  )
  use owner_node <- option.then(non_null_json(json_get(account_node, "owner")))
  let owner_id = json_get_string(owner_node, "id") |> option.unwrap("")
  use customer <- option.then(customer_record_from_node(owner_node, owner_id))
  let account =
    store_credit_account_from_node(
      account_node,
      fallback_account_id,
      customer.id,
    )
  Some(#(customer, account))
}

fn customer_order_summary_from_hydrate(
  value: commit.JsonValue,
  fallback_order_id: String,
) -> Option(CustomerOrderSummaryRecord) {
  use data <- option.then(json_get(value, "data"))
  use order_node <- option.then(non_null_json(json_get(data, "order")))
  let customer_id =
    json_get(order_node, "customer")
    |> option.then(fn(customer) { non_null_json(Some(customer)) })
    |> option.then(fn(customer) { json_get_string(customer, "id") })
  let total =
    json_get(order_node, "currentTotalPriceSet")
    |> option.then(fn(price_set) { non_null_json(Some(price_set)) })
    |> option.then(fn(price_set) { json_get_money(price_set, "shopMoney") })
  Some(CustomerOrderSummaryRecord(
    id: json_get_string(order_node, "id") |> option.unwrap(fallback_order_id),
    customer_id: customer_id,
    cursor: None,
    name: json_get_string(order_node, "name"),
    email: json_get_string(order_node, "email"),
    created_at: json_get_string(order_node, "createdAt"),
    current_total_price: total,
  ))
}

fn customer_account_pages_from_hydrate(
  value: commit.JsonValue,
) -> Option(List(CustomerAccountPageRecord)) {
  use data <- option.then(json_get(value, "data"))
  use connection <- option.then(
    non_null_json(json_get(data, "customerAccountPages")),
  )
  let start_cursor =
    json_get(connection, "pageInfo")
    |> option.then(fn(page_info) { json_get_string(page_info, "startCursor") })
  let end_cursor =
    json_get(connection, "pageInfo")
    |> option.then(fn(page_info) { json_get_string(page_info, "endCursor") })
  let nodes = connection_nodes(data, "customerAccountPages")
  let last_index = list.length(nodes) - 1
  Some(
    list.index_map(nodes, fn(node, index) {
      let cursor = case index {
        0 -> start_cursor
        i if i == last_index -> end_cursor
        _ -> None
      }
      CustomerAccountPageRecord(
        id: json_get_string(node, "id") |> option.unwrap(""),
        title: json_get_string(node, "title") |> option.unwrap(""),
        handle: json_get_string(node, "handle") |> option.unwrap(""),
        default_cursor: json_get_string(node, "defaultCursor")
          |> option.unwrap(""),
        cursor: cursor,
      )
    }),
  )
}

fn customer_record_from_node(
  node: commit.JsonValue,
  fallback_id: String,
) -> Option(CustomerRecord) {
  let id = json_get_string(node, "id") |> option.or(Some(fallback_id))
  use customer_id <- option.then(id)
  let email = json_get_string(node, "email")
  let first_name = json_get_string(node, "firstName")
  let last_name = json_get_string(node, "lastName")
  Some(CustomerRecord(
    id: customer_id,
    first_name: first_name,
    last_name: last_name,
    display_name: json_get_string(node, "displayName")
      |> option.or(build_display_name(first_name, last_name, email)),
    email: email,
    legacy_resource_id: json_get_string(node, "legacyResourceId")
      |> option.or(gid_tail(customer_id)),
    locale: json_get_string(node, "locale"),
    note: json_get_string(node, "note"),
    can_delete: json_get_bool(node, "canDelete"),
    verified_email: json_get_bool(node, "verifiedEmail"),
    data_sale_opt_out: json_get_bool(node, "dataSaleOptOut")
      |> option.unwrap(False),
    tax_exempt: json_get_bool(node, "taxExempt"),
    tax_exemptions: json_get_string_list(node, "taxExemptions"),
    state: json_get_string(node, "state"),
    tags: json_get_string_list(node, "tags"),
    number_of_orders: json_get_scalar_string(node, "numberOfOrders"),
    amount_spent: json_get_money(node, "amountSpent"),
    default_email_address: default_email_from_node(
      json_get(node, "defaultEmailAddress"),
      email,
    ),
    default_phone_number: default_phone_from_node(json_get(
      node,
      "defaultPhoneNumber",
    )),
    email_marketing_consent: email_consent_from_node(json_get(
      node,
      "emailMarketingConsent",
    )),
    sms_marketing_consent: sms_consent_from_node(json_get(
      node,
      "smsMarketingConsent",
    )),
    default_address: json_get(node, "defaultAddress")
      |> option.then(default_address_from_node),
    created_at: json_get_string(node, "createdAt"),
    updated_at: json_get_string(node, "updatedAt"),
  ))
}

fn default_email_from_node(
  value: Option(commit.JsonValue),
  fallback_email: Option(String),
) -> Option(CustomerDefaultEmailAddressRecord) {
  case non_null_json(value) {
    Some(node) ->
      Some(CustomerDefaultEmailAddressRecord(
        email_address: json_get_string(node, "emailAddress")
          |> option.or(fallback_email),
        marketing_state: json_get_string(node, "marketingState"),
        marketing_opt_in_level: json_get_string(node, "marketingOptInLevel"),
        marketing_updated_at: json_get_string(node, "marketingUpdatedAt"),
      ))
    None ->
      case fallback_email {
        Some(email) ->
          Some(CustomerDefaultEmailAddressRecord(
            email_address: Some(email),
            marketing_state: None,
            marketing_opt_in_level: None,
            marketing_updated_at: None,
          ))
        None -> None
      }
  }
}

fn default_phone_from_node(
  value: Option(commit.JsonValue),
) -> Option(CustomerDefaultPhoneNumberRecord) {
  use node <- option.then(non_null_json(value))
  Some(CustomerDefaultPhoneNumberRecord(
    phone_number: json_get_string(node, "phoneNumber"),
    marketing_state: json_get_string(node, "marketingState"),
    marketing_opt_in_level: json_get_string(node, "marketingOptInLevel"),
    marketing_updated_at: json_get_string(node, "marketingUpdatedAt"),
    marketing_collected_from: json_get_string(node, "marketingCollectedFrom"),
  ))
}

fn email_consent_from_node(
  value: Option(commit.JsonValue),
) -> Option(CustomerEmailMarketingConsentRecord) {
  use node <- option.then(non_null_json(value))
  Some(CustomerEmailMarketingConsentRecord(
    marketing_state: json_get_string(node, "marketingState"),
    marketing_opt_in_level: json_get_string(node, "marketingOptInLevel"),
    consent_updated_at: json_get_string(node, "consentUpdatedAt"),
  ))
}

fn sms_consent_from_node(
  value: Option(commit.JsonValue),
) -> Option(CustomerSmsMarketingConsentRecord) {
  use node <- option.then(non_null_json(value))
  Some(CustomerSmsMarketingConsentRecord(
    marketing_state: json_get_string(node, "marketingState"),
    marketing_opt_in_level: json_get_string(node, "marketingOptInLevel"),
    consent_updated_at: json_get_string(node, "consentUpdatedAt"),
    consent_collected_from: json_get_string(node, "consentCollectedFrom"),
  ))
}

fn default_address_from_node(
  node: commit.JsonValue,
) -> Option(CustomerDefaultAddressRecord) {
  case node {
    commit.JsonNull -> None
    _ ->
      Some(CustomerDefaultAddressRecord(
        id: json_get_string(node, "id"),
        first_name: json_get_string(node, "firstName"),
        last_name: json_get_string(node, "lastName"),
        address1: json_get_string(node, "address1"),
        address2: json_get_string(node, "address2"),
        city: json_get_string(node, "city"),
        company: json_get_string(node, "company"),
        province: json_get_string(node, "province"),
        province_code: json_get_string(node, "provinceCode"),
        country: json_get_string(node, "country"),
        country_code_v2: json_get_string(node, "countryCodeV2"),
        zip: json_get_string(node, "zip"),
        phone: json_get_string(node, "phone"),
        name: json_get_string(node, "name"),
        formatted_area: json_get_string(node, "formattedArea"),
      ))
  }
}

fn address_records_from_customer_node(
  customer_node: commit.JsonValue,
  customer_id: String,
) -> List(CustomerAddressRecord) {
  let nodes =
    connection_nodes(customer_node, "addressesV2")
    |> list.append(connection_nodes(customer_node, "addresses"))
  list.index_map(nodes, fn(node, index) {
    let default_address =
      default_address_from_node(node)
      |> option.unwrap(CustomerDefaultAddressRecord(
        id: None,
        first_name: None,
        last_name: None,
        address1: None,
        address2: None,
        city: None,
        company: None,
        province: None,
        province_code: None,
        country: None,
        country_code_v2: None,
        zip: None,
        phone: None,
        name: None,
        formatted_area: None,
      ))
    CustomerAddressRecord(
      id: default_address.id
        |> option.unwrap(
          "gid://shopify/MailingAddress/" <> int.to_string(index + 1),
        ),
      customer_id: customer_id,
      cursor: None,
      position: index,
      first_name: default_address.first_name,
      last_name: default_address.last_name,
      address1: default_address.address1,
      address2: default_address.address2,
      city: default_address.city,
      company: default_address.company,
      province: default_address.province,
      province_code: default_address.province_code,
      country: default_address.country,
      country_code_v2: default_address.country_code_v2,
      zip: default_address.zip,
      phone: default_address.phone,
      name: default_address.name,
      formatted_area: default_address.formatted_area,
    )
  })
}

fn metafield_records_from_customer_node(
  customer_node: commit.JsonValue,
  customer_id: String,
) -> List(CustomerMetafieldRecord) {
  connection_nodes(customer_node, "metafields")
  |> list.filter_map(fn(node) {
    case
      json_get_string(node, "id"),
      json_get_string(node, "namespace"),
      json_get_string(node, "key"),
      json_get_string(node, "type"),
      json_get_string(node, "value")
    {
      Some(id), Some(namespace), Some(key), Some(type_), Some(value) ->
        Ok(CustomerMetafieldRecord(
          id: id,
          customer_id: customer_id,
          namespace: namespace,
          key: key,
          type_: type_,
          value: value,
          compare_digest: json_get_string(node, "compareDigest"),
          created_at: json_get_string(node, "createdAt"),
          updated_at: json_get_string(node, "updatedAt"),
        ))
      _, _, _, _, _ -> Error(Nil)
    }
  })
}

fn store_credit_accounts_from_customer_node(
  customer_node: commit.JsonValue,
  customer_id: String,
) -> List(StoreCreditAccountRecord) {
  connection_nodes(customer_node, "storeCreditAccounts")
  |> list.filter_map(fn(node) {
    case json_get_string(node, "id") {
      Some(id) -> Ok(store_credit_account_from_node(node, id, customer_id))
      None -> Error(Nil)
    }
  })
}

fn order_records_from_customer_node(
  customer_node: commit.JsonValue,
  customer_id: String,
) -> List(CustomerOrderSummaryRecord) {
  let start_cursor =
    json_get(customer_node, "orders")
    |> option.then(fn(connection) { json_get(connection, "pageInfo") })
    |> option.then(fn(page_info) { json_get_string(page_info, "startCursor") })
  let end_cursor =
    json_get(customer_node, "orders")
    |> option.then(fn(connection) { json_get(connection, "pageInfo") })
    |> option.then(fn(page_info) { json_get_string(page_info, "endCursor") })
  let nodes = connection_nodes(customer_node, "orders")
  let last_index = list.length(nodes) - 1
  list.index_map(nodes, fn(node, index) {
    let cursor = case index {
      0 -> start_cursor
      i if i == last_index -> end_cursor
      _ -> None
    }
    CustomerOrderSummaryRecord(
      id: json_get_string(node, "id") |> option.unwrap(""),
      customer_id: Some(customer_id),
      cursor: cursor,
      name: json_get_string(node, "name"),
      email: json_get_string(node, "email"),
      created_at: json_get_string(node, "createdAt"),
      current_total_price: json_get(node, "currentTotalPriceSet")
        |> option.then(fn(price_set) { json_get_money(price_set, "shopMoney") }),
    )
  })
}

fn store_credit_account_from_node(
  node: commit.JsonValue,
  fallback_id: String,
  customer_id: String,
) -> StoreCreditAccountRecord {
  StoreCreditAccountRecord(
    id: json_get_string(node, "id") |> option.unwrap(fallback_id),
    customer_id: customer_id,
    cursor: None,
    balance: json_money_from_value(json_get(node, "balance"))
      |> option.unwrap(Money(amount: "0.0", currency_code: "USD")),
  )
}

fn connection_nodes(
  object: commit.JsonValue,
  key: String,
) -> List(commit.JsonValue) {
  case json_get(object, key) {
    Some(commit.JsonArray(items)) -> items
    Some(connection) ->
      case json_get(connection, "nodes") {
        Some(commit.JsonArray(nodes)) -> nodes
        _ ->
          case json_get(connection, "edges") {
            Some(commit.JsonArray(edges)) ->
              list.filter_map(edges, fn(edge) {
                case json_get(edge, "node") {
                  Some(node) -> Ok(node)
                  None -> Error(Nil)
                }
              })
            _ -> []
          }
      }
    None -> []
  }
}

fn json_get(value: commit.JsonValue, key: String) -> Option(commit.JsonValue) {
  case value {
    commit.JsonObject(fields) ->
      list.find_map(fields, fn(pair) {
        case pair {
          #(name, child) if name == key -> Ok(child)
          _ -> Error(Nil)
        }
      })
      |> option.from_result
    _ -> None
  }
}

fn non_null_json(value: Option(commit.JsonValue)) -> Option(commit.JsonValue) {
  case value {
    Some(commit.JsonNull) -> None
    Some(v) -> Some(v)
    None -> None
  }
}

fn json_get_string(value: commit.JsonValue, key: String) -> Option(String) {
  json_get_scalar_string(value, key)
}

fn json_get_scalar_string(
  value: commit.JsonValue,
  key: String,
) -> Option(String) {
  json_get(value, key) |> option.then(json_scalar_string)
}

fn json_scalar_string(value: commit.JsonValue) -> Option(String) {
  case value {
    commit.JsonString(s) -> Some(s)
    commit.JsonInt(i) -> Some(int.to_string(i))
    commit.JsonFloat(f) -> Some(float.to_string(f))
    _ -> None
  }
}

fn json_get_bool(value: commit.JsonValue, key: String) -> Option(Bool) {
  case json_get(value, key) {
    Some(commit.JsonBool(b)) -> Some(b)
    _ -> None
  }
}

fn json_get_int(value: commit.JsonValue, key: String) -> Option(Int) {
  case json_get(value, key) {
    Some(commit.JsonInt(i)) -> Some(i)
    Some(commit.JsonString(s)) ->
      case int.parse(s) {
        Ok(i) -> Some(i)
        Error(_) -> None
      }
    _ -> None
  }
}

fn json_get_string_list(value: commit.JsonValue, key: String) -> List(String) {
  case json_get(value, key) {
    Some(commit.JsonArray(items)) ->
      list.filter_map(items, fn(item) {
        case json_scalar_string(item) {
          Some(s) -> Ok(s)
          None -> Error(Nil)
        }
      })
    _ -> []
  }
}

fn json_get_money(value: commit.JsonValue, key: String) -> Option(Money) {
  json_money_from_value(json_get(value, key))
}

fn json_money_from_value(value: Option(commit.JsonValue)) -> Option(Money) {
  use money <- option.then(non_null_json(value))
  use amount <- option.then(json_get_scalar_string(money, "amount"))
  let currency =
    json_get_string(money, "currencyCode")
    |> option.or(json_get_string(money, "currency_code"))
    |> option.unwrap("USD")
  Some(Money(amount: amount, currency_code: currency))
}

fn handle_customer_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let input =
    graphql_helpers.read_arg_object(
      graphql_helpers.field_args(field, variables),
      "input",
    )
    |> option.unwrap(dict.new())
  let errors = validate_customer_create(store, input, upstream)
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
  upstream: UpstreamContext,
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
  let local_errors = validate_customer_input_fields(store, input, None)
  list.append(
    list.append(presence_errors, local_errors),
    validate_upstream_duplicate_customer(input, local_errors, None, upstream),
  )
}

fn validate_upstream_duplicate_customer(
  input: Dict(String, root_field.ResolvedValue),
  local_errors: List(UserError),
  exclude_customer_id: Option(String),
  upstream: UpstreamContext,
) -> List(UserError) {
  let has_email_error =
    local_errors |> list.any(fn(error) { error.field == ["email"] })
  let has_phone_error =
    local_errors |> list.any(fn(error) { error.field == ["phone"] })
  let email_error = case read_obj_string(input, "email"), has_email_error {
    Some(email), False ->
      case
        string.contains(email, "@")
        && upstream_customer_duplicate_exists(
          "email:" <> email,
          exclude_customer_id,
          upstream,
        )
      {
        True -> [UserError(["email"], "Email has already been taken", None)]
        False -> []
      }
    _, _ -> []
  }
  let phone_error = case read_obj_string(input, "phone"), has_phone_error {
    Some(phone), False ->
      case
        valid_phone(phone)
        && upstream_customer_duplicate_exists(
          "phone:" <> phone,
          exclude_customer_id,
          upstream,
        )
      {
        True -> [UserError(["phone"], "Phone has already been taken", None)]
        False -> []
      }
    _, _ -> []
  }
  list.append(email_error, phone_error)
}

fn upstream_customer_duplicate_exists(
  query_value: String,
  exclude_customer_id: Option(String),
  upstream: UpstreamContext,
) -> Bool {
  let query =
    "query CustomerDuplicateHydrate($query: String!) {
  customers(first: 1, query: $query) { nodes { id } }
}
"
  let variables = json.object([#("query", json.string(query_value))])
  case
    upstream_query.fetch_sync(
      upstream.origin,
      upstream.transport,
      upstream.headers,
      "CustomerDuplicateHydrate",
      query,
      variables,
    )
  {
    Ok(value) ->
      case upstream_customer_id_result(value) {
        Some(id) -> id != option.unwrap(exclude_customer_id, "")
        None -> False
      }
    Error(_) -> False
  }
}

fn upstream_customer_id_result(value: commit.JsonValue) -> Option(String) {
  case json_get(value, "data") {
    Some(data) ->
      case connection_nodes(data, "customers") {
        [first, ..] -> json_get_string(first, "id")
        [] -> None
      }
    None -> None
  }
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
  upstream: UpstreamContext,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let input =
    graphql_helpers.read_arg_object(
      graphql_helpers.field_args(field, variables),
      "input",
    )
    |> option.unwrap(dict.new())
  let id = read_obj_string(input, "id")
  case id {
    Some(customer_id) ->
      case store.get_effective_customer_by_id(store, customer_id) {
        Some(existing) -> {
          let input_errors =
            validate_customer_input_fields(store, input, Some(customer_id))
          let validation_errors =
            list.flatten([
              inline_consent_update_errors(input),
              input_errors,
              validate_upstream_duplicate_customer(
                input,
                input_errors,
                Some(customer_id),
                upstream,
              ),
            ])
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
  let args = graphql_helpers.field_args(field, variables)
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  let identifier =
    graphql_helpers.read_arg_object(args, "identifier")
    |> option.unwrap(dict.new())
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
  let input =
    graphql_helpers.read_arg_object(
      graphql_helpers.field_args(field, variables),
      "input",
    )
    |> option.unwrap(dict.new())
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
  let args = graphql_helpers.field_args(field, variables)
  let customer_id = graphql_helpers.read_arg_string_nonempty(args, "customerId")
  let address_input =
    graphql_helpers.read_arg_object(args, "address")
    |> option.unwrap(dict.new())
  let set_default =
    graphql_helpers.read_arg_bool(args, "setAsDefault") |> option.unwrap(False)
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
  let args = graphql_helpers.field_args(field, variables)
  let address_id = graphql_helpers.read_arg_string_nonempty(args, "addressId")
  let address_input =
    graphql_helpers.read_arg_object(args, "address")
    |> option.unwrap(dict.new())
  let set_default =
    graphql_helpers.read_arg_bool(args, "setAsDefault") |> option.unwrap(False)
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
  let args = graphql_helpers.field_args(field, variables)
  let address_id = graphql_helpers.read_arg_string_nonempty(args, "addressId")
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
  let args = graphql_helpers.field_args(field, variables)
  let customer_id = graphql_helpers.read_arg_string_nonempty(args, "customerId")
  let address_id = graphql_helpers.read_arg_string_nonempty(args, "addressId")
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
  let args = graphql_helpers.field_args(field, variables)
  let customer_id = graphql_helpers.read_arg_string_nonempty(args, "customerId")
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
          let #(updated_at, next_identity) = case
            next_exemptions == customer.tax_exemptions
          {
            True -> #(customer.updated_at, identity)
            False -> {
              let #(ts, after_ts) =
                synthetic_identity.make_synthetic_timestamp(identity)
              #(Some(ts), after_ts)
            }
          }
          let updated =
            CustomerRecord(
              ..customer,
              tax_exemptions: next_exemptions,
              updated_at: updated_at,
            )
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
            next_identity,
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
  let input =
    graphql_helpers.read_arg_object(
      graphql_helpers.field_args(field, variables),
      "input",
    )
    |> option.unwrap(dict.new())
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
  let input =
    graphql_helpers.read_arg_object(
      graphql_helpers.field_args(field, variables),
      "input",
    )
    |> option.unwrap(dict.new())
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
  let args = graphql_helpers.field_args(field, variables)
  let customer_id = graphql_helpers.read_arg_string_nonempty(args, "customerId")
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
            [] -> graphql_helpers.option_string_source(customer_id)
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
  let args = graphql_helpers.field_args(field, variables)
  let customer_id = graphql_helpers.read_arg_string_nonempty(args, "customerId")
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
  let args = graphql_helpers.field_args(field, variables)
  let customer_id = graphql_helpers.read_arg_string_nonempty(args, "customerId")
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
  let args = graphql_helpers.field_args(field, variables)
  let id =
    graphql_helpers.read_arg_string_nonempty(args, "customerPaymentMethodId")
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
  let args = graphql_helpers.field_args(field, variables)
  let account_id = graphql_helpers.read_arg_string_nonempty(args, "id")
  let input_name = case is_credit {
    True -> "creditInput"
    False -> "debitInput"
  }
  let input =
    graphql_helpers.read_arg_object(args, input_name)
    |> option.unwrap(dict.new())
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
  let args = graphql_helpers.field_args(field, variables)
  let order_id = graphql_helpers.read_arg_string_nonempty(args, "orderId")
  let customer_id = graphql_helpers.read_arg_string_nonempty(args, "customerId")
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
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string_nonempty(args, "orderId") {
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
  let args = graphql_helpers.field_args(field, variables)
  let one = graphql_helpers.read_arg_string_nonempty(args, "customerOneId")
  let two = graphql_helpers.read_arg_string_nonempty(args, "customerTwoId")
  let override =
    graphql_helpers.read_arg_object(args, "overrideFields")
    |> option.unwrap(dict.new())
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
    |> list.index_map(fn(metafield, index) {
      CustomerMetafieldRecord(
        ..metafield,
        id: "gid://shopify/Metafield/900000000000" <> int.to_string(index),
        customer_id: merged.id,
      )
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

fn user_error_source(err: UserError) -> SourceValue {
  src_object([
    #("field", case err.field {
      [] -> SrcNull
      _ -> SrcList(list.map(err.field, SrcString))
    }),
    #("message", SrcString(err.message)),
    #("code", graphql_helpers.option_string_source(err.code)),
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

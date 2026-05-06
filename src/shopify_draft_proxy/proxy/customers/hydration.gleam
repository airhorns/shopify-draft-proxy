//// Customer domain internals split from proxy/customers.gleam.

import gleam/dict.{type Dict}
import gleam/int
import gleam/json
import gleam/list
import gleam/option.{type Option, None, Some}
import shopify_draft_proxy/graphql/ast.{type Selection}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/customers/customer_types.{
  type CustomerHydrateResult, CustomerHydrateResult,
}
import shopify_draft_proxy/proxy/customers/inputs.{
  build_display_name, gid_tail, json_get, json_get_bool, json_get_money,
  json_get_scalar_string, json_get_string, json_get_string_list,
  json_money_from_value, non_null_json, read_obj_string,
}
import shopify_draft_proxy/proxy/graphql_helpers
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry, is_proxy_synthetic_gid,
}
import shopify_draft_proxy/state/types.{
  type CustomerAccountPageRecord, type CustomerAddressRecord,
  type CustomerDefaultAddressRecord, type CustomerDefaultEmailAddressRecord,
  type CustomerDefaultPhoneNumberRecord,
  type CustomerEmailMarketingConsentRecord, type CustomerMetafieldRecord,
  type CustomerOrderSummaryRecord, type CustomerRecord,
  type CustomerSmsMarketingConsentRecord, type StoreCreditAccountRecord,
  CustomerAccountPageRecord, CustomerAddressRecord, CustomerDefaultAddressRecord,
  CustomerDefaultEmailAddressRecord, CustomerDefaultPhoneNumberRecord,
  CustomerEmailMarketingConsentRecord, CustomerMetafieldRecord,
  CustomerOrderSummaryRecord, CustomerRecord, CustomerSmsMarketingConsentRecord,
  Money, StoreCreditAccountRecord,
}

@internal
pub fn hydrate_before_customer_mutation(
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

@internal
pub fn hydrate_optional_customer_id(
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

@internal
pub fn maybe_hydrate_customer(
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

@internal
pub fn hydrate_optional_store_credit_account_id(
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

@internal
pub fn hydrate_optional_customer_order_id(
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

@internal
pub fn customer_hydrate_query() -> String {
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

@internal
pub fn customer_order_summary_hydrate_query() -> String {
  "query CustomerOrderSummaryHydrate($id: ID!) {\n"
  <> "  order(id: $id) {\n"
  <> "    id name email createdAt\n"
  <> "    currentTotalPriceSet { shopMoney { amount currencyCode } }\n"
  <> "    customer { id }\n"
  <> "  }\n"
  <> "}\n"
}

@internal
pub fn store_credit_account_hydrate_query() -> String {
  "query StoreCreditAccountHydrate($id: ID!) {\n"
  <> "  storeCreditAccount(id: $id) {\n"
  <> "    id\n"
  <> "    balance { amount currencyCode }\n"
  <> "    owner { ... on Customer { id firstName lastName displayName email legacyResourceId locale note canDelete verifiedEmail dataSaleOptOut taxExempt taxExemptions state tags numberOfOrders createdAt updatedAt amountSpent { amount currencyCode } defaultEmailAddress { emailAddress marketingState marketingOptInLevel marketingUpdatedAt } defaultPhoneNumber { phoneNumber marketingState marketingOptInLevel marketingUpdatedAt marketingCollectedFrom } emailMarketingConsent { marketingState marketingOptInLevel consentUpdatedAt } smsMarketingConsent { marketingState marketingOptInLevel consentUpdatedAt consentCollectedFrom } defaultAddress { id firstName lastName address1 address2 city company province provinceCode country countryCodeV2 zip phone name formattedArea } } }\n"
  <> "  }\n"
  <> "}\n"
}

@internal
pub fn customer_hydrate_result(
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

@internal
pub fn store_credit_account_hydrate_result(
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

@internal
pub fn customer_order_summary_from_hydrate(
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

@internal
pub fn customer_account_pages_from_hydrate(
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

@internal
pub fn customer_record_from_node(
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
    account_activation_token: None,
    created_at: json_get_string(node, "createdAt"),
    updated_at: json_get_string(node, "updatedAt"),
  ))
}

@internal
pub fn default_email_from_node(
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

@internal
pub fn default_phone_from_node(
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

@internal
pub fn email_consent_from_node(
  value: Option(commit.JsonValue),
) -> Option(CustomerEmailMarketingConsentRecord) {
  use node <- option.then(non_null_json(value))
  Some(CustomerEmailMarketingConsentRecord(
    marketing_state: json_get_string(node, "marketingState"),
    marketing_opt_in_level: json_get_string(node, "marketingOptInLevel"),
    consent_updated_at: json_get_string(node, "consentUpdatedAt"),
  ))
}

@internal
pub fn sms_consent_from_node(
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

@internal
pub fn default_address_from_node(
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

@internal
pub fn address_records_from_customer_node(
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

@internal
pub fn metafield_records_from_customer_node(
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

@internal
pub fn store_credit_accounts_from_customer_node(
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

@internal
pub fn order_records_from_customer_node(
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

@internal
pub fn store_credit_account_from_node(
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

@internal
pub fn connection_nodes(
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

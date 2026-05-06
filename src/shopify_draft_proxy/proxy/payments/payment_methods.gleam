//// Customer payment-method mutation handling.

import gleam/dict.{type Dict}
import gleam/json
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, SrcBool, SrcNull, SrcString,
  get_field_response_key,
}
import shopify_draft_proxy/proxy/payments/serializers.{payment_method_source}
import shopify_draft_proxy/proxy/payments/types.{
  type MutationFieldResult, type UserError, MutationFieldResult, UserError,
  credit_card_processing_session_id, decode_duplication_data,
  encode_duplication_data, gid_tail, is_shopify_gid, json_array_items, json_get,
  json_get_bool, json_get_data_string, json_get_string, project_payload,
  read_string_field, uri_encode, user_errors_source,
}
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types as state_types

fn payment_method_error(
  field: String,
  message: String,
  code: String,
) -> UserError {
  UserError(field: Some([field]), message: message, code: Some(code))
}

fn payment_method_missing_error(field: String) -> UserError {
  payment_method_error(
    field,
    "Customer payment method does not exist",
    "PAYMENT_METHOD_DOES_NOT_EXIST",
  )
}

fn payment_method_active_contract_error(field: String) -> UserError {
  payment_method_error(
    field,
    "Customer payment method cannot be revoked because it has active subscription contracts",
    "ACTIVE_CONTRACT",
  )
}

fn customer_missing_error(field: String) -> UserError {
  payment_method_error(
    field,
    "Customer does not exist",
    "CUSTOMER_DOES_NOT_EXIST",
  )
}

fn active_payment_method(
  store: Store,
  id: Option(String),
  field_name: String,
) -> Result(state_types.CustomerPaymentMethodRecord, UserError) {
  payment_method_by_id(store, id, field_name, False)
}

fn payment_method_by_id(
  store: Store,
  id: Option(String),
  field_name: String,
  show_revoked: Bool,
) -> Result(state_types.CustomerPaymentMethodRecord, UserError) {
  case is_shopify_gid(id, "CustomerPaymentMethod"), id {
    True, Some(payment_id) ->
      case
        store.get_effective_customer_payment_method_by_id(
          store,
          payment_id,
          show_revoked,
        )
      {
        Some(record) -> Ok(record)
        None -> Error(payment_method_missing_error(field_name))
      }
    _, _ -> Error(payment_method_missing_error(field_name))
  }
}

fn customer_by_id(
  store: Store,
  id: Option(String),
  field_name: String,
) -> Result(state_types.CustomerRecord, UserError) {
  case is_shopify_gid(id, "Customer"), id {
    True, Some(customer_id) ->
      case store.get_effective_customer_by_id(store, customer_id) {
        Some(customer) -> Ok(customer)
        None -> Error(customer_missing_error(field_name))
      }
    _, _ -> Error(customer_missing_error(field_name))
  }
}

@internal
pub fn hydrate_customer_payment_method_context(
  store: Store,
  customer_ids: List(String),
  method_ids: List(String),
  upstream: UpstreamContext,
) -> Store {
  let missing_customer_ids =
    customer_ids
    |> list.filter(fn(id) {
      is_shopify_gid(Some(id), "Customer")
      && case store.get_effective_customer_by_id(store, id) {
        Some(_) -> False
        None -> True
      }
    })
  let missing_method_ids =
    method_ids
    |> list.filter(fn(id) {
      is_shopify_gid(Some(id), "CustomerPaymentMethod")
      && case
        store.get_effective_customer_payment_method_by_id(store, id, True)
      {
        Some(_) -> False
        None -> True
      }
    })
  case missing_customer_ids, missing_method_ids {
    [], [] -> store
    _, _ -> {
      // Pattern 2: local-runtime payment-method flows need existing
      // customers and vaulted-method shells before staging local-only
      // mutations. Snapshot/no-cassette mode keeps the unknown-resource
      // userErrors instead of inventing state.
      let variables =
        json.object([
          #("customerIds", json.array(missing_customer_ids, json.string)),
          #(
            "customerPaymentMethodIds",
            json.array(missing_method_ids, json.string),
          ),
        ])
      case
        upstream_query.fetch_sync(
          upstream.origin,
          upstream.transport,
          upstream.headers,
          "CustomerPaymentMethodHydrate",
          customer_payment_method_hydrate_query(),
          variables,
        )
      {
        Ok(value) ->
          hydrate_customer_payment_methods_from_response(store, value)
        Error(_) -> store
      }
    }
  }
}

fn customer_payment_method_hydrate_query() -> String {
  "query CustomerPaymentMethodHydrate($customerIds: [ID!]!, $customerPaymentMethodIds: [ID!]!) {\n"
  <> "  shop { id name myshopifyDomain url primaryDomain { id host url sslEnabled } }\n"
  <> "  customers: nodes(ids: $customerIds) { ... on Customer { id email displayName state } }\n"
  <> "  customerPaymentMethods: nodes(ids: $customerPaymentMethodIds) {\n"
  <> "    ... on CustomerPaymentMethod {\n"
  <> "      id revokedAt revokedReason customer { id }\n"
  <> "      instrument { __typename ... on CustomerCreditCard { lastDigits maskedNumber } ... on CustomerPaypalBillingAgreement { paypalAccountEmail inactive } }\n"
  <> "    }\n"
  <> "  }\n"
  <> "}\n"
}

fn hydrate_customer_payment_methods_from_response(
  store: Store,
  value: commit.JsonValue,
) -> Store {
  case json_get(value, "data") {
    Some(data) -> {
      let with_shop = case
        json_get(data, "shop") |> option.then(shop_from_hydrate_node)
      {
        Some(shop) -> store.upsert_base_shop(store, shop)
        None -> store
      }
      let customers =
        json_array_items(json_get(data, "customers"))
        |> list.filter_map(customer_from_hydrate_node)
      let methods =
        json_array_items(json_get(data, "customerPaymentMethods"))
        |> list.filter_map(customer_payment_method_from_hydrate_node)
      let with_customers = store.upsert_base_customers(with_shop, customers)
      store.upsert_base_customer_payment_methods(with_customers, methods)
    }
    None -> store
  }
}

fn shop_from_hydrate_node(
  node: commit.JsonValue,
) -> Option(state_types.ShopRecord) {
  use id <- option.then(json_get_string(node, "id"))
  let domain =
    json_get_string(node, "myshopifyDomain")
    |> option.unwrap("shopify-draft-proxy.myshopify.com")
  let url =
    json_get_string(node, "url")
    |> option.unwrap("https://" <> domain)
  Some(
    state_types.ShopRecord(
      id: id,
      name: json_get_string(node, "name")
        |> option.unwrap("Shopify Draft Proxy"),
      myshopify_domain: domain,
      url: url,
      primary_domain: hydrate_primary_domain(node, domain, url),
      contact_email: "",
      email: "",
      currency_code: "USD",
      enabled_presentment_currencies: ["USD"],
      iana_timezone: "UTC",
      timezone_abbreviation: "UTC",
      timezone_offset: "+0000",
      timezone_offset_minutes: 0,
      taxes_included: False,
      tax_shipping: False,
      unit_system: "IMPERIAL_SYSTEM",
      weight_unit: "POUNDS",
      shop_address: state_types.ShopAddressRecord(
        id: "gid://shopify/ShopAddress/" <> gid_tail(id),
        address1: None,
        address2: None,
        city: None,
        company: None,
        coordinates_validated: False,
        country: None,
        country_code_v2: None,
        formatted: [],
        formatted_area: None,
        latitude: None,
        longitude: None,
        phone: None,
        province: None,
        province_code: None,
        zip: None,
      ),
      plan: state_types.ShopPlanRecord(
        partner_development: False,
        public_display_name: "",
        shopify_plus: False,
      ),
      resource_limits: state_types.ShopResourceLimitsRecord(
        location_limit: 0,
        max_product_options: 0,
        max_product_variants: 0,
        redirect_limit_reached: False,
      ),
      features: state_types.ShopFeaturesRecord(
        avalara_avatax: False,
        branding: "SHOPIFY",
        bundles: state_types.ShopBundlesFeatureRecord(
          eligible_for_bundles: False,
          ineligibility_reason: None,
          sells_bundles: False,
        ),
        captcha: False,
        cart_transform: state_types.ShopCartTransformFeatureRecord(
          eligible_operations: state_types.ShopCartTransformEligibleOperationsRecord(
            expand_operation: False,
            merge_operation: False,
            update_operation: False,
          ),
        ),
        dynamic_remarketing: False,
        eligible_for_subscription_migration: False,
        eligible_for_subscriptions: False,
        gift_cards: False,
        harmonized_system_code: False,
        legacy_subscription_gateway_enabled: False,
        live_view: False,
        paypal_express_subscription_gateway_status: "DISABLED",
        reports: False,
        sells_subscriptions: False,
        show_metrics: False,
        storefront: False,
        unified_markets: False,
      ),
      payment_settings: state_types.PaymentSettingsRecord(
        supported_digital_wallets: [],
      ),
      shop_policies: [],
    ),
  )
}

fn hydrate_primary_domain(
  shop_node: commit.JsonValue,
  fallback_domain: String,
  fallback_url: String,
) -> state_types.ShopDomainRecord {
  let primary = json_get(shop_node, "primaryDomain")
  state_types.ShopDomainRecord(
    id: primary
      |> option.then(fn(domain) { json_get_string(domain, "id") })
      |> option.unwrap("gid://shopify/Domain/" <> fallback_domain),
    host: primary
      |> option.then(fn(domain) { json_get_string(domain, "host") })
      |> option.unwrap(fallback_domain),
    url: primary
      |> option.then(fn(domain) { json_get_string(domain, "url") })
      |> option.unwrap(fallback_url),
    ssl_enabled: primary
      |> option.then(fn(domain) { json_get_bool(domain, "sslEnabled") })
      |> option.unwrap(True),
  )
}

fn customer_from_hydrate_node(
  node: commit.JsonValue,
) -> Result(state_types.CustomerRecord, Nil) {
  case json_get_string(node, "id") {
    Some(id) ->
      Ok(state_types.CustomerRecord(
        id: id,
        first_name: None,
        last_name: None,
        display_name: json_get_string(node, "displayName"),
        email: json_get_string(node, "email"),
        legacy_resource_id: Some(gid_tail(id)),
        locale: None,
        note: None,
        can_delete: None,
        verified_email: None,
        data_sale_opt_out: False,
        tax_exempt: None,
        tax_exemptions: [],
        state: json_get_string(node, "state"),
        tags: [],
        number_of_orders: None,
        amount_spent: None,
        default_email_address: None,
        default_phone_number: None,
        email_marketing_consent: None,
        sms_marketing_consent: None,
        default_address: None,
        account_activation_token: None,
        created_at: None,
        updated_at: None,
      ))
    None -> Error(Nil)
  }
}

fn customer_payment_method_from_hydrate_node(
  node: commit.JsonValue,
) -> Result(state_types.CustomerPaymentMethodRecord, Nil) {
  let customer_id =
    json_get_string(node, "customerId")
    |> option.or(
      json_get(node, "customer")
      |> option.then(fn(customer) { json_get_string(customer, "id") }),
    )
  case json_get_string(node, "id"), customer_id {
    Some(id), Some(owner_id) ->
      Ok(
        state_types.CustomerPaymentMethodRecord(
          id: id,
          customer_id: owner_id,
          cursor: None,
          instrument: json_get(node, "instrument")
            |> option.then(instrument_from_hydrate_node),
          revoked_at: json_get_string(node, "revokedAt"),
          revoked_reason: json_get_string(node, "revokedReason"),
          subscription_contracts: [],
        ),
      )
    _, _ -> Error(Nil)
  }
}

fn instrument_from_hydrate_node(
  node: commit.JsonValue,
) -> Option(state_types.CustomerPaymentMethodInstrumentRecord) {
  let type_name =
    json_get_string(node, "typeName")
    |> option.or(json_get_string(node, "__typename"))
  use resolved_type <- option.then(type_name)
  let data_node = case json_get(node, "data") {
    Some(data) -> data
    None -> node
  }
  Some(state_types.CustomerPaymentMethodInstrumentRecord(
    type_name: resolved_type,
    data: dict.from_list(
      list.filter_map(
        [
          "lastDigits",
          "maskedNumber",
          "paypalAccountEmail",
          "inactive",
        ],
        fn(key) {
          case json_get_data_string(data_node, key) {
            Some(value) -> Ok(#(key, value))
            None -> Error(Nil)
          }
        },
      ),
    ),
  ))
}

fn scrubbed_credit_card_instrument(
  billing_address: Dict(String, root_field.ResolvedValue),
) -> state_types.CustomerPaymentMethodInstrumentRecord {
  state_types.CustomerPaymentMethodInstrumentRecord(
    type_name: "CustomerCreditCard",
    data: dict.from_list(list.append(
      [
        #("lastDigits", "__null"),
        #("maskedNumber", "__null"),
      ],
      billing_address_data(billing_address),
    )),
  )
}

fn billing_address_data(
  billing_address: Dict(String, root_field.ResolvedValue),
) -> List(#(String, String)) {
  [
    #("firstName", first_nonempty_string(billing_address, ["firstName"])),
    #("lastName", first_nonempty_string(billing_address, ["lastName"])),
    #("company", first_nonempty_string(billing_address, ["company"])),
    #("address1", first_nonempty_string(billing_address, ["address1"])),
    #("address2", first_nonempty_string(billing_address, ["address2"])),
    #("city", first_nonempty_string(billing_address, ["city"])),
    #("zip", first_nonempty_string(billing_address, ["zip"])),
    #("phone", first_nonempty_string(billing_address, ["phone"])),
    #("country", first_nonempty_string(billing_address, ["country"])),
    #(
      "countryCodeV2",
      first_nonempty_string(billing_address, [
        "countryCode",
        "country_code",
        "country",
      ]),
    ),
    #("province", first_nonempty_string(billing_address, ["province"])),
    #(
      "provinceCode",
      first_nonempty_string(billing_address, [
        "provinceCode",
        "province_code",
        "province",
      ]),
    ),
  ]
  |> list.flat_map(fn(entry) {
    let #(key, value) = entry
    case value {
      Some(value) -> [#("billingAddress." <> key, value)]
      None -> []
    }
  })
}

fn scrubbed_paypal_instrument(
  inactive: Bool,
) -> state_types.CustomerPaymentMethodInstrumentRecord {
  state_types.CustomerPaymentMethodInstrumentRecord(
    type_name: "CustomerPaypalBillingAgreement",
    data: dict.from_list([
      #("paypalAccountEmail", "__null"),
      #("inactive", case inactive {
        True -> "true"
        False -> "false"
      }),
    ]),
  )
}

fn create_payment_method_record(
  identity: SyntheticIdentityRegistry,
  customer_id: String,
  instrument: Option(state_types.CustomerPaymentMethodInstrumentRecord),
) -> #(state_types.CustomerPaymentMethodRecord, SyntheticIdentityRegistry) {
  let #(id, next_identity) =
    synthetic_identity.make_synthetic_gid(identity, "CustomerPaymentMethod")
  #(
    state_types.CustomerPaymentMethodRecord(
      id: id,
      customer_id: customer_id,
      cursor: None,
      instrument: instrument,
      revoked_at: None,
      revoked_reason: None,
      subscription_contracts: [],
    ),
    next_identity,
  )
}

@internal
pub fn create_credit_card_payment_method(
  store,
  identity,
  field,
  fragments,
  variables,
) {
  let args = graphql_helpers.field_args(field, variables)
  case
    customer_by_id(
      store,
      graphql_helpers.read_arg_string_nonempty(args, "customerId"),
      "customerId",
    )
  {
    Error(error) ->
      payment_method_result(
        store,
        identity,
        field,
        fragments,
        "customerPaymentMethodCreditCardCreate",
        None,
        [error],
        [#("processing", SrcBool(False))],
      )
    Ok(customer) -> {
      let errors = credit_card_input_errors(args)
      case errors {
        [_, ..] ->
          payment_method_result(
            store,
            identity,
            field,
            fragments,
            "customerPaymentMethodCreditCardCreate",
            None,
            errors,
            [#("processing", SrcBool(False))],
          )
        [] -> {
          case credit_card_processing(args) {
            True ->
              payment_method_result(
                store,
                identity,
                field,
                fragments,
                "customerPaymentMethodCreditCardCreate",
                None,
                [],
                [#("processing", SrcBool(True))],
              )
            False -> {
              let billing_address =
                graphql_helpers.read_arg_object(args, "billingAddress")
                |> option.unwrap(dict.new())
              let #(record, next_identity) =
                create_payment_method_record(
                  identity,
                  customer.id,
                  Some(scrubbed_credit_card_instrument(billing_address)),
                )
              payment_method_result(
                store.stage_customer_payment_method(store, record),
                next_identity,
                field,
                fragments,
                "customerPaymentMethodCreditCardCreate",
                Some(record),
                [],
                [#("processing", SrcBool(False))],
              )
            }
          }
        }
      }
    }
  }
}

@internal
pub fn update_credit_card_payment_method(
  store,
  identity,
  field,
  fragments,
  variables,
) {
  let args = graphql_helpers.field_args(field, variables)
  case
    active_payment_method(
      store,
      graphql_helpers.read_arg_string_nonempty(args, "id"),
      "id",
    )
  {
    Error(error) ->
      payment_method_result(
        store,
        identity,
        field,
        fragments,
        "customerPaymentMethodCreditCardUpdate",
        None,
        [error],
        [#("processing", SrcBool(False))],
      )
    Ok(current) -> {
      let errors = credit_card_input_errors(args)
      case errors {
        [_, ..] ->
          payment_method_result(
            store,
            identity,
            field,
            fragments,
            "customerPaymentMethodCreditCardUpdate",
            None,
            errors,
            [#("processing", SrcBool(False))],
          )
        [] -> {
          case credit_card_processing(args) {
            True ->
              payment_method_result(
                store,
                identity,
                field,
                fragments,
                "customerPaymentMethodCreditCardUpdate",
                None,
                [],
                [#("processing", SrcBool(True))],
              )
            False -> {
              let billing_address =
                graphql_helpers.read_arg_object(args, "billingAddress")
                |> option.unwrap(dict.new())
              let updated =
                state_types.CustomerPaymentMethodRecord(
                  ..current,
                  instrument: Some(scrubbed_credit_card_instrument(
                    billing_address,
                  )),
                )
              payment_method_result(
                store.stage_customer_payment_method(store, updated),
                identity,
                field,
                fragments,
                "customerPaymentMethodCreditCardUpdate",
                Some(updated),
                [],
                [#("processing", SrcBool(False))],
              )
            }
          }
        }
      }
    }
  }
}

fn credit_card_input_errors(
  args: Dict(String, root_field.ResolvedValue),
) -> List(UserError) {
  list.append(session_id_blank_errors(args), billing_address_blank_errors(args))
}

fn session_id_blank_errors(
  args: Dict(String, root_field.ResolvedValue),
) -> List(UserError) {
  case graphql_helpers.read_arg_string_nonempty(args, "sessionId") {
    Some(_) -> []
    None -> [
      payment_method_error(
        "sessionId",
        "Session ID can't be blank",
        "REQUIRED_INPUT_FIELD",
      ),
    ]
  }
}

fn credit_card_processing(
  args: Dict(String, root_field.ResolvedValue),
) -> Bool {
  case graphql_helpers.read_arg_string(args, "sessionId") {
    Some(session_id) ->
      session_id == credit_card_processing_session_id
      || string.starts_with(
        session_id,
        credit_card_processing_session_id <> ":",
      )
    None -> False
  }
}

fn count_object_values(input: Dict(String, root_field.ResolvedValue)) -> Int {
  input
  |> dict.values
  |> list.filter(fn(value) {
    case value {
      root_field.ObjectVal(_) -> True
      _ -> False
    }
  })
  |> list.length
}

fn remote_reference_gateway_errors(
  remote_reference: Dict(String, root_field.ResolvedValue),
) -> List(UserError) {
  case count_object_values(remote_reference) == 1 {
    False -> [
      UserError(
        field: Some(["remote_reference"]),
        message: "Remote reference must contain exactly one payment method.",
        code: Some("INVALID"),
      ),
    ]
    True -> {
      let gateway_errors =
        list.flatten([
          validate_remote_reference_gateway(
            remote_reference,
            "stripePaymentMethod",
            "stripe_payment_method",
            [
              #("customerId", "customer_id", "STRIPE_CUSTOMER_ID_BLANK"),
            ],
          ),
          validate_remote_reference_gateway(
            remote_reference,
            "paypalPaymentMethod",
            "paypal_payment_method",
            [
              #(
                "billingAgreementId",
                "billing_agreement_id",
                "BILLING_AGREEMENT_ID_BLANK",
              ),
            ],
          ),
          validate_remote_reference_gateway(
            remote_reference,
            "braintreePaymentMethod",
            "braintree_payment_method",
            [
              #("customerId", "customer_id", "BRAINTREE_CUSTOMER_ID_BLANK"),
              #(
                "paymentMethodToken",
                "payment_method_token",
                "PAYMENT_METHOD_TOKEN_BLANK",
              ),
            ],
          ),
          validate_remote_reference_gateway(
            remote_reference,
            "authorizeNetCustomerPaymentProfile",
            "authorize_net_customer_payment_profile",
            [
              #(
                "customerProfileId",
                "customer_profile_id",
                "AUTHORIZE_NET_CUSTOMER_PROFILE_ID_BLANK",
              ),
            ],
          ),
          validate_remote_reference_gateway(
            remote_reference,
            "adyenPaymentMethod",
            "adyen_payment_method",
            [
              #(
                "shopperReference",
                "shopper_reference",
                "ADYEN_SHOPPER_REFERENCE_BLANK",
              ),
              #(
                "storedPaymentMethodId",
                "stored_payment_method_id",
                "ADYEN_STORED_PAYMENT_METHOD_ID_BLANK",
              ),
            ],
          ),
        ])
      case gateway_errors {
        [] -> []
        _ -> gateway_errors
      }
    }
  }
}

fn validate_remote_reference_gateway(
  remote_reference: Dict(String, root_field.ResolvedValue),
  gateway_key: String,
  gateway_field: String,
  required_fields: List(#(String, String, String)),
) -> List(UserError) {
  case dict.get(remote_reference, gateway_key) {
    Ok(root_field.ObjectVal(gateway)) ->
      required_fields
      |> list.filter_map(fn(required) {
        let #(input_key, field_key, code) = required
        case has_nonblank_remote_reference_field(gateway, input_key) {
          True -> Error(Nil)
          False ->
            Ok(UserError(
              field: Some(["remote_reference", gateway_field, field_key]),
              message: remote_reference_blank_message(field_key),
              code: Some(code),
            ))
        }
      })
    _ -> []
  }
}

fn has_nonblank_remote_reference_field(
  input: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Bool {
  case dict.get(input, key) {
    Ok(root_field.StringVal(value)) -> string.trim(value) != ""
    _ -> False
  }
}

fn remote_reference_blank_message(field: String) -> String {
  field <> " can't be blank"
}

@internal
pub fn create_remote_payment_method(
  store,
  identity,
  field,
  fragments,
  variables,
) {
  let args = graphql_helpers.field_args(field, variables)
  case
    customer_by_id(
      store,
      graphql_helpers.read_arg_string_nonempty(args, "customerId"),
      "customerId",
    )
  {
    Error(error) ->
      payment_method_result(
        store,
        identity,
        field,
        fragments,
        "customerPaymentMethodRemoteCreate",
        None,
        [error],
        [],
      )
    Ok(customer) -> {
      let remote_reference =
        graphql_helpers.read_arg_object(args, "remoteReference")
        |> option.unwrap(dict.new())
      case remote_reference_gateway_errors(remote_reference) {
        [_, ..] as errors ->
          payment_method_result(
            store,
            identity,
            field,
            fragments,
            "customerPaymentMethodRemoteCreate",
            None,
            errors,
            [],
          )
        [] -> {
          let #(record, next_identity) =
            create_payment_method_record(identity, customer.id, None)
          payment_method_result(
            store.stage_customer_payment_method(store, record),
            next_identity,
            field,
            fragments,
            "customerPaymentMethodRemoteCreate",
            Some(record),
            [],
            [],
          )
        }
      }
    }
  }
}

@internal
pub fn create_paypal_payment_method(
  store,
  identity,
  field,
  fragments,
  variables,
) {
  let args = graphql_helpers.field_args(field, variables)
  case
    customer_by_id(
      store,
      graphql_helpers.read_arg_string_nonempty(args, "customerId"),
      "customerId",
    )
  {
    Error(error) ->
      payment_method_result(
        store,
        identity,
        field,
        fragments,
        "customerPaymentMethodPaypalBillingAgreementCreate",
        None,
        [error],
        [],
      )
    Ok(customer) -> {
      let inactive =
        graphql_helpers.read_arg_bool(args, "inactive") |> option.unwrap(False)
      let #(record, next_identity) =
        create_payment_method_record(
          identity,
          customer.id,
          Some(scrubbed_paypal_instrument(inactive)),
        )
      payment_method_result(
        store.stage_customer_payment_method(store, record),
        next_identity,
        field,
        fragments,
        "customerPaymentMethodPaypalBillingAgreementCreate",
        Some(record),
        [],
        [],
      )
    }
  }
}

@internal
pub fn update_paypal_payment_method(
  store,
  identity,
  field,
  fragments,
  variables,
) {
  let args = graphql_helpers.field_args(field, variables)
  case
    active_payment_method(
      store,
      graphql_helpers.read_arg_string_nonempty(args, "id"),
      "id",
    )
  {
    Error(error) ->
      payment_method_result(
        store,
        identity,
        field,
        fragments,
        "customerPaymentMethodPaypalBillingAgreementUpdate",
        None,
        [error],
        [],
      )
    Ok(current) -> {
      let inactive = case current.instrument {
        Some(instrument) ->
          case dict.get(instrument.data, "inactive") {
            Ok("true") -> True
            _ -> False
          }
        None -> False
      }
      let updated =
        state_types.CustomerPaymentMethodRecord(
          ..current,
          instrument: Some(scrubbed_paypal_instrument(inactive)),
        )
      payment_method_result(
        store.stage_customer_payment_method(store, updated),
        identity,
        field,
        fragments,
        "customerPaymentMethodPaypalBillingAgreementUpdate",
        Some(updated),
        [],
        [],
      )
    }
  }
}

@internal
pub fn get_payment_method_duplication_data(
  store,
  identity,
  field,
  fragments,
  variables,
) {
  let args = graphql_helpers.field_args(field, variables)
  case
    active_payment_method(
      store,
      graphql_helpers.read_arg_string_nonempty(args, "customerPaymentMethodId"),
      "customerPaymentMethodId",
    )
  {
    Error(error) ->
      payment_method_result(
        store,
        identity,
        field,
        fragments,
        "customerPaymentMethodGetDuplicationData",
        None,
        [error],
        [#("encryptedDuplicationData", SrcNull)],
      )
    Ok(method) ->
      case require_shop_pay_instrument(method, "customerPaymentMethodId") {
        Some(error) ->
          payment_method_result(
            store,
            identity,
            field,
            fragments,
            "customerPaymentMethodGetDuplicationData",
            None,
            [error],
            [#("encryptedDuplicationData", SrcNull)],
          )
        None -> {
          case
            customer_by_id(
              store,
              graphql_helpers.read_arg_string_nonempty(args, "targetCustomerId"),
              "targetCustomerId",
            )
          {
            Error(error) ->
              payment_method_result(
                store,
                identity,
                field,
                fragments,
                "customerPaymentMethodGetDuplicationData",
                None,
                [error],
                [#("encryptedDuplicationData", SrcNull)],
              )
            Ok(target_customer) -> {
              let target_shop_id =
                graphql_helpers.read_arg_string_nonempty(args, "targetShopId")
                |> option.unwrap("")
              case same_shop_duplication_error(store, target_shop_id) {
                Some(error) ->
                  payment_method_result(
                    store,
                    identity,
                    field,
                    fragments,
                    "customerPaymentMethodGetDuplicationData",
                    None,
                    [error],
                    [#("encryptedDuplicationData", SrcNull)],
                  )
                None -> {
                  let data =
                    encode_duplication_data(
                      method.id,
                      target_customer.id,
                      target_shop_id,
                    )
                  payment_method_result(
                    store,
                    identity,
                    field,
                    fragments,
                    "customerPaymentMethodGetDuplicationData",
                    None,
                    [],
                    [#("encryptedDuplicationData", SrcString(data))],
                  )
                }
              }
            }
          }
        }
      }
  }
}

@internal
pub fn require_shop_pay_instrument(
  method: state_types.CustomerPaymentMethodRecord,
  field_name: String,
) -> Option(UserError) {
  case method.instrument {
    Some(state_types.CustomerPaymentMethodInstrumentRecord(
      type_name: type_name,
      ..,
    )) -> {
      case is_shop_pay_agreement_type(type_name) {
        True -> None
        False ->
          Some(payment_method_error(
            field_name,
            "Invalid instrument",
            "INVALID_INSTRUMENT",
          ))
      }
    }
    _ ->
      Some(payment_method_error(
        field_name,
        "Invalid instrument",
        "INVALID_INSTRUMENT",
      ))
  }
}

fn is_shop_pay_agreement_type(type_name: String) -> Bool {
  type_name == "CustomerShopPayAgreement"
  || type_name == "CustomerShopPayBillingAgreement"
  || type_name == "ShopPayBillingAgreement"
  || type_name == "shop_pay_agreement"
}

fn same_shop_duplication_error(
  store: Store,
  target_shop_id: String,
) -> Option(UserError) {
  case store.get_effective_shop(store) {
    Some(shop) if shop.id == target_shop_id ->
      Some(UserError(
        field: Some(["targetShopId"]),
        message: "Target shop is not eligible for payment method duplication",
        code: Some("SAME_SHOP"),
      ))
    _ -> None
  }
}

fn billing_address_blank_errors(
  args: Dict(String, root_field.ResolvedValue),
) -> List(UserError) {
  let address =
    graphql_helpers.read_arg_object(args, "billingAddress")
    |> option.unwrap(dict.new())
  [
    #(["address1"], "address1"),
    #(["city"], "city"),
    #(["zip"], "zip"),
    #(["countryCode", "country_code", "country"], "country_code"),
    #(["provinceCode", "province_code", "province"], "province_code"),
  ]
  |> list.filter_map(fn(field_pair) {
    let #(input_names, field_name) = field_pair
    case first_nonempty_string(address, input_names) {
      Some(_) -> Error(Nil)
      None ->
        Ok(UserError(
          field: Some(["billing_address", field_name]),
          message: "can't be blank",
          code: Some("BLANK"),
        ))
    }
  })
}

fn first_nonempty_string(
  input: Dict(String, root_field.ResolvedValue),
  keys: List(String),
) -> Option(String) {
  list.find_map(keys, fn(key) {
    case read_string_field(input, key) {
      Some(value) -> Ok(value)
      None -> Error(Nil)
    }
  })
  |> option.from_result
}

@internal
pub fn create_payment_method_from_duplication_data(
  store,
  identity,
  field,
  fragments,
  variables,
) {
  let args = graphql_helpers.field_args(field, variables)
  let customer_id = graphql_helpers.read_arg_string_nonempty(args, "customerId")
  case customer_by_id(store, customer_id, "customerId") {
    Error(error) ->
      payment_method_result(
        store,
        identity,
        field,
        fragments,
        "customerPaymentMethodCreateFromDuplicationData",
        None,
        [error],
        [],
      )
    Ok(customer) -> {
      case billing_address_blank_errors(args) {
        [_, ..] as errors ->
          payment_method_result(
            store,
            identity,
            field,
            fragments,
            "customerPaymentMethodCreateFromDuplicationData",
            None,
            errors,
            [],
          )
        [] -> {
          case
            graphql_helpers.read_arg_string_nonempty(
              args,
              "encryptedDuplicationData",
            )
          {
            None ->
              invalid_duplication_result(store, identity, field, fragments)
            Some(raw) ->
              case decode_duplication_data(raw) {
                Error(_) ->
                  invalid_duplication_result(store, identity, field, fragments)
                Ok(payload) -> {
                  let source_id =
                    dict_string_to_option(payload, "customerPaymentMethodId")
                  let target_id =
                    dict_string_to_option(payload, "targetCustomerId")
                  case target_id == Some(customer.id), source_id {
                    True, Some(method_id) ->
                      case
                        store.get_effective_customer_payment_method_by_id(
                          store,
                          method_id,
                          True,
                        )
                      {
                        Some(source_method) -> {
                          let #(record, next_identity) =
                            create_payment_method_record(
                              identity,
                              customer.id,
                              source_method.instrument,
                            )
                          payment_method_result(
                            store.stage_customer_payment_method(store, record),
                            next_identity,
                            field,
                            fragments,
                            "customerPaymentMethodCreateFromDuplicationData",
                            Some(record),
                            [],
                            [],
                          )
                        }
                        None ->
                          invalid_duplication_result(
                            store,
                            identity,
                            field,
                            fragments,
                          )
                      }
                    _, _ ->
                      invalid_duplication_result(
                        store,
                        identity,
                        field,
                        fragments,
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

@internal
pub fn dict_string_to_option(
  data: Dict(String, String),
  key: String,
) -> Option(String) {
  case dict.get(data, key) {
    Ok(value) -> Some(value)
    Error(_) -> None
  }
}

@internal
pub fn invalid_duplication_result(store, identity, field, fragments) {
  payment_method_result(
    store,
    identity,
    field,
    fragments,
    "customerPaymentMethodCreateFromDuplicationData",
    None,
    [
      UserError(
        field: Some(["encryptedDuplicationData"]),
        message: "Encrypted duplication data is invalid",
        code: Some("INVALID_ENCRYPTED_DUPLICATION_DATA"),
      ),
    ],
    [],
  )
}

@internal
pub fn get_payment_method_update_url(
  store,
  identity,
  field,
  fragments,
  variables,
) {
  let args = graphql_helpers.field_args(field, variables)
  case
    active_payment_method(
      store,
      graphql_helpers.read_arg_string_nonempty(args, "customerPaymentMethodId"),
      "customerPaymentMethodId",
    )
  {
    Error(error) ->
      payment_method_result(
        store,
        identity,
        field,
        fragments,
        "customerPaymentMethodGetUpdateUrl",
        None,
        [error],
        [#("updatePaymentMethodUrl", SrcNull)],
      )
    Ok(method) -> {
      case require_shop_pay_instrument(method, "customerPaymentMethodId") {
        Some(error) ->
          payment_method_result(
            store,
            identity,
            field,
            fragments,
            "customerPaymentMethodGetUpdateUrl",
            None,
            [error],
            [#("updatePaymentMethodUrl", SrcNull)],
          )
        None -> {
          let #(id, identity_after_id) =
            synthetic_identity.make_synthetic_gid(
              identity,
              "CustomerPaymentMethodUpdateUrl",
            )
          let #(created_at, next_identity) =
            synthetic_identity.make_synthetic_timestamp(identity_after_id)
          let update_url =
            "https://shopify-draft-proxy.local/customer-payment-methods/"
            <> uri_encode(gid_tail(method.id))
            <> "/update?token=local-only"
          let record =
            state_types.CustomerPaymentMethodUpdateUrlRecord(
              id: id,
              customer_payment_method_id: method.id,
              update_payment_method_url: update_url,
              created_at: created_at,
            )
          payment_method_result(
            store.stage_customer_payment_method_update_url(store, record),
            next_identity,
            field,
            fragments,
            "customerPaymentMethodGetUpdateUrl",
            None,
            [],
            [#("updatePaymentMethodUrl", SrcString(update_url))],
          )
        }
      }
    }
  }
}

@internal
pub fn revoke_payment_method(store, identity, field, fragments, variables) {
  let args = graphql_helpers.field_args(field, variables)
  case
    payment_method_by_id(
      store,
      graphql_helpers.read_arg_string_nonempty(args, "customerPaymentMethodId"),
      "customerPaymentMethodId",
      True,
    )
  {
    Error(error) ->
      payment_method_result(
        store,
        identity,
        field,
        fragments,
        "customerPaymentMethodRevoke",
        None,
        [error],
        [#("revokedCustomerPaymentMethodId", SrcNull)],
      )
    Ok(method) -> {
      case method.subscription_contracts, method.revoked_at {
        [_, ..], _ ->
          payment_method_result(
            store,
            identity,
            field,
            fragments,
            "customerPaymentMethodRevoke",
            None,
            [payment_method_active_contract_error("customerPaymentMethodId")],
            [#("revokedCustomerPaymentMethodId", SrcNull)],
          )
        _, Some(_) ->
          payment_method_result(
            store,
            identity,
            field,
            fragments,
            "customerPaymentMethodRevoke",
            None,
            [],
            [
              #(
                "revokedCustomerPaymentMethodId",
                SrcString(customer_payment_method_gid_from_token(method.id)),
              ),
            ],
          )
        _, None -> {
          let #(_, identity_after_intent) =
            synthetic_identity.make_synthetic_timestamp(identity)
          let #(revoked_at, next_identity) =
            synthetic_identity.make_synthetic_timestamp(identity_after_intent)
          let revoked =
            state_types.CustomerPaymentMethodRecord(
              ..method,
              revoked_at: Some(revoked_at),
              revoked_reason: Some("CUSTOMER_REVOKED"),
            )
          payment_method_result(
            store.stage_customer_payment_method(store, revoked),
            next_identity,
            field,
            fragments,
            "customerPaymentMethodRevoke",
            None,
            [],
            [
              #(
                "revokedCustomerPaymentMethodId",
                SrcString(customer_payment_method_gid_from_token(method.id)),
              ),
            ],
          )
        }
      }
    }
  }
}

fn customer_payment_method_gid_from_token(id: String) -> String {
  "gid://shopify/CustomerPaymentMethod/" <> gid_tail(id)
}

fn payment_method_result(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  root_name: String,
  method: Option(state_types.CustomerPaymentMethodRecord),
  errors: List(UserError),
  extra_fields: List(#(String, SourceValue)),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let method_fields = [
    #("customerPaymentMethod", case method {
      Some(record) -> payment_method_source(store, record)
      None -> SrcNull
    }),
    #("userErrors", user_errors_source(errors)),
  ]
  let staged = case method {
    Some(record) -> [record.id]
    None -> []
  }
  #(
    MutationFieldResult(
      get_field_response_key(field),
      project_payload(
        field,
        fragments,
        list.append(method_fields, extra_fields),
      ),
      staged,
      root_name,
      Some(case root_name {
        "paymentReminderSend" ->
          "Staged a local payment reminder intent only; no customer email is sent at runtime."
        _ ->
          "Staged locally in the in-memory customer payment-method draft store; payment credentials, gateway secrets, and customer-facing update URLs are scrubbed or synthetic."
      }),
    ),
    store,
    identity,
  )
}

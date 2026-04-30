//// Pure-Gleam parity runner.
////
//// Replaces the legacy vitest harness in
//// `tests/unit/conformance-parity-scenarios.test.ts`. Reads a parity
//// spec, loads the capture and GraphQL document referenced by the
//// spec, drives them through `draft_proxy.process_request`, and
//// compares each target's `capturePath` slice of the capture against
//// the same `proxyPath` slice of the proxy response — applying the
//// spec's `expectedDifferences` matchers.
////
//// Per-target `proxyRequest` overrides are supported. State (store,
//// synthetic identity) is threaded forward across requests, so a
//// target can read back records the primary mutation created.
////
//// File-system paths in the spec are repo-root relative. Tests run
//// from the `gleam/` subdirectory; the runner resolves paths via `..`
//// (configurable via `RunnerConfig.repo_root`).

import gleam/dict
import gleam/int
import gleam/json
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import parity/diff.{type Mismatch}
import parity/json_value.{
  type JsonValue, JArray, JBool, JFloat, JInt, JObject, JString,
}
import parity/jsonpath
import parity/spec.{
  type Spec, type Target, NoVariables, OverrideRequest, ReusePrimary,
  VariablesFromCapture, VariablesFromFile, VariablesInline,
}
import shopify_draft_proxy/proxy/draft_proxy.{
  type DraftProxy, type Response, Request,
}
import shopify_draft_proxy/state/store as store_mod
import shopify_draft_proxy/state/synthetic_identity
import shopify_draft_proxy/state/types.{
  type GiftCardConfigurationRecord, type GiftCardRecipientAttributesRecord,
  type GiftCardRecord, type GiftCardTransactionRecord, type Money,
  type PaymentSettingsRecord, type ShopAddressRecord, type ShopDomainRecord,
  type ShopFeaturesRecord, type ShopPlanRecord, type ShopPolicyRecord,
  type ShopRecord, type ShopResourceLimitsRecord, type ShopifyFunctionAppRecord,
  type ShopifyFunctionRecord, GiftCardConfigurationRecord,
  GiftCardRecipientAttributesRecord, GiftCardRecord, GiftCardTransactionRecord,
  Money, PaymentSettingsRecord, ShopAddressRecord, ShopBundlesFeatureRecord,
  ShopCartTransformEligibleOperationsRecord, ShopCartTransformFeatureRecord,
  ShopDomainRecord, ShopFeaturesRecord, ShopPlanRecord, ShopPolicyRecord,
  ShopRecord, ShopResourceLimitsRecord, ShopifyFunctionAppRecord,
  ShopifyFunctionRecord,
}
import simplifile

pub type RunError {
  /// File could not be read off disk.
  FileError(path: String, reason: String)
  /// File contents could not be parsed as JSON.
  JsonError(path: String, reason: String)
  /// Spec was malformed.
  SpecError(reason: String)
  /// Variables JSONPath did not resolve.
  VariablesUnresolved(path: String)
  /// `fromPrimaryProxyPath` substitution path didn't resolve.
  PrimaryRefUnresolved(path: String)
  /// `fromCapturePath` substitution path didn't resolve.
  CaptureRefUnresolved(path: String)
  /// Capture JSONPath did not resolve for a target.
  CaptureUnresolved(target: String, path: String)
  /// Proxy response JSONPath did not resolve for a target.
  ProxyUnresolved(target: String, path: String)
  /// Proxy returned a non-200 status.
  ProxyStatus(target: String, status: Int, body: String)
}

pub type TargetReport {
  TargetReport(
    name: String,
    capture_path: String,
    proxy_path: String,
    mismatches: List(Mismatch),
  )
}

pub type Report {
  Report(scenario_id: String, targets: List(TargetReport))
}

pub type RunnerConfig {
  RunnerConfig(repo_root: String)
}

pub fn default_config() -> RunnerConfig {
  RunnerConfig(repo_root: "..")
}

pub fn run(spec_path: String) -> Result(Report, RunError) {
  run_with_config(default_config(), spec_path)
}

pub fn run_with_config(
  config: RunnerConfig,
  spec_path: String,
) -> Result(Report, RunError) {
  use spec_source <- result.try(read_file(resolve(config, spec_path)))
  use parsed <- result.try(parse_spec(spec_source))
  use capture <- result.try(load_capture(config, parsed))
  use primary_doc <- result.try(
    read_file(resolve(config, parsed.proxy_request.document_path)),
  )
  use primary_vars <- result.try(resolve_variables(
    config,
    parsed.proxy_request.variables,
    capture,
    None,
    "<primary>",
  ))
  let proxy = draft_proxy.new()
  let proxy = seed_capture_preconditions(parsed, capture, proxy)
  use #(primary_response, proxy) <- result.try(execute(
    proxy,
    primary_doc,
    primary_vars,
    "<primary>",
  ))
  use primary_value <- result.try(parse_response_body(primary_response))
  use #(_proxy, target_reports) <- result.try(run_targets(
    config,
    parsed,
    capture,
    primary_value,
    proxy,
  ))
  Ok(Report(scenario_id: parsed.scenario_id, targets: target_reports))
}

fn seed_capture_preconditions(
  parsed: Spec,
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case parsed.scenario_id {
    "gift-card-search-filters" ->
      seed_gift_card_lifecycle_preconditions(capture, proxy)
    "gift-card-lifecycle" ->
      seed_gift_card_lifecycle_preconditions(capture, proxy)
    "functions-owner-metadata-local-staging" ->
      seed_shopify_function_preconditions(capture, proxy)
    "shop-baseline-read"
    | "shop-policy-update-parity"
    | "admin-platform-store-property-node-reads" ->
      seed_shop_preconditions(capture, proxy)
    _ -> proxy
  }
}

fn seed_shop_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case jsonpath.lookup(capture, "$.readOnlyBaselines.shop.data.shop") {
    Some(shop_json) ->
      case make_seed_shop(shop_json) {
        Ok(shop) ->
          draft_proxy.DraftProxy(
            ..proxy,
            store: store_mod.upsert_base_shop(proxy.store, shop),
          )
        Error(_) -> proxy
      }
    None -> proxy
  }
}

fn make_seed_shop(source: JsonValue) -> Result(ShopRecord, Nil) {
  use id <- result.try(required_string_field(source, "id"))
  use name <- result.try(required_string_field(source, "name"))
  use myshopify_domain <- result.try(required_string_field(
    source,
    "myshopifyDomain",
  ))
  use url <- result.try(required_string_field(source, "url"))
  use primary_domain <- result.try(
    make_seed_shop_domain(read_object_field(source, "primaryDomain")),
  )
  use shop_address <- result.try(
    make_seed_shop_address(read_object_field(source, "shopAddress")),
  )
  use plan <- result.try(make_seed_shop_plan(read_object_field(source, "plan")))
  use resource_limits <- result.try(
    make_seed_resource_limits(read_object_field(source, "resourceLimits")),
  )
  use features <- result.try(
    make_seed_shop_features(read_object_field(source, "features")),
  )
  let payment_settings =
    make_seed_payment_settings(read_object_field(source, "paymentSettings"))
  let policies =
    read_array_field(source, "shopPolicies")
    |> option.unwrap([])
    |> list.filter_map(make_seed_shop_policy)
  Ok(ShopRecord(
    id: id,
    name: name,
    myshopify_domain: myshopify_domain,
    url: url,
    primary_domain: primary_domain,
    contact_email: read_string_field(source, "contactEmail")
      |> option.unwrap(""),
    email: read_string_field(source, "email") |> option.unwrap(""),
    currency_code: read_string_field(source, "currencyCode")
      |> option.unwrap(""),
    enabled_presentment_currencies: read_string_array_field(
      source,
      "enabledPresentmentCurrencies",
    ),
    iana_timezone: read_string_field(source, "ianaTimezone")
      |> option.unwrap(""),
    timezone_abbreviation: read_string_field(source, "timezoneAbbreviation")
      |> option.unwrap(""),
    timezone_offset: read_string_field(source, "timezoneOffset")
      |> option.unwrap(""),
    timezone_offset_minutes: read_int_field(source, "timezoneOffsetMinutes")
      |> option.unwrap(0),
    taxes_included: read_bool_field(source, "taxesIncluded")
      |> option.unwrap(False),
    tax_shipping: read_bool_field(source, "taxShipping")
      |> option.unwrap(False),
    unit_system: read_string_field(source, "unitSystem") |> option.unwrap(""),
    weight_unit: read_string_field(source, "weightUnit") |> option.unwrap(""),
    shop_address: shop_address,
    plan: plan,
    resource_limits: resource_limits,
    features: features,
    payment_settings: payment_settings,
    shop_policies: policies,
  ))
}

fn make_seed_shop_domain(
  source: Option(JsonValue),
) -> Result(ShopDomainRecord, Nil) {
  case source {
    Some(value) -> {
      use id <- result.try(required_string_field(value, "id"))
      use host <- result.try(required_string_field(value, "host"))
      use url <- result.try(required_string_field(value, "url"))
      Ok(ShopDomainRecord(
        id: id,
        host: host,
        url: url,
        ssl_enabled: read_bool_field(value, "sslEnabled")
          |> option.unwrap(False),
      ))
    }
    None -> Error(Nil)
  }
}

fn make_seed_shop_address(
  source: Option(JsonValue),
) -> Result(ShopAddressRecord, Nil) {
  case source {
    Some(value) -> {
      use id <- result.try(required_string_field(value, "id"))
      Ok(ShopAddressRecord(
        id: id,
        address1: read_string_field(value, "address1"),
        address2: read_string_field(value, "address2"),
        city: read_string_field(value, "city"),
        company: read_string_field(value, "company"),
        coordinates_validated: read_bool_field(value, "coordinatesValidated")
          |> option.unwrap(False),
        country: read_string_field(value, "country"),
        country_code_v2: read_string_field(value, "countryCodeV2"),
        formatted: read_string_array_field(value, "formatted"),
        formatted_area: read_string_field(value, "formattedArea"),
        latitude: read_float_field(value, "latitude"),
        longitude: read_float_field(value, "longitude"),
        phone: read_string_field(value, "phone"),
        province: read_string_field(value, "province"),
        province_code: read_string_field(value, "provinceCode"),
        zip: read_string_field(value, "zip"),
      ))
    }
    None -> Error(Nil)
  }
}

fn make_seed_shop_plan(
  source: Option(JsonValue),
) -> Result(ShopPlanRecord, Nil) {
  case source {
    Some(value) ->
      Ok(ShopPlanRecord(
        partner_development: read_bool_field(value, "partnerDevelopment")
          |> option.unwrap(False),
        public_display_name: read_string_field(value, "publicDisplayName")
          |> option.unwrap(""),
        shopify_plus: read_bool_field(value, "shopifyPlus")
          |> option.unwrap(False),
      ))
    None -> Error(Nil)
  }
}

fn make_seed_resource_limits(
  source: Option(JsonValue),
) -> Result(ShopResourceLimitsRecord, Nil) {
  case source {
    Some(value) ->
      Ok(ShopResourceLimitsRecord(
        location_limit: read_int_field(value, "locationLimit")
          |> option.unwrap(0),
        max_product_options: read_int_field(value, "maxProductOptions")
          |> option.unwrap(0),
        max_product_variants: read_int_field(value, "maxProductVariants")
          |> option.unwrap(0),
        redirect_limit_reached: read_bool_field(value, "redirectLimitReached")
          |> option.unwrap(False),
      ))
    None -> Error(Nil)
  }
}

fn make_seed_shop_features(
  source: Option(JsonValue),
) -> Result(ShopFeaturesRecord, Nil) {
  case source {
    Some(value) -> {
      let bundles = case read_object_field(value, "bundles") {
        Some(b) ->
          ShopBundlesFeatureRecord(
            eligible_for_bundles: read_bool_field(b, "eligibleForBundles")
              |> option.unwrap(False),
            ineligibility_reason: read_string_field(b, "ineligibilityReason"),
            sells_bundles: read_bool_field(b, "sellsBundles")
              |> option.unwrap(False),
          )
        None ->
          ShopBundlesFeatureRecord(
            eligible_for_bundles: False,
            ineligibility_reason: None,
            sells_bundles: False,
          )
      }
      let operations = case
        read_object_field(value, "cartTransform")
        |> option.then(fn(cart) {
          read_object_field(cart, "eligibleOperations")
        })
      {
        Some(op) ->
          ShopCartTransformEligibleOperationsRecord(
            expand_operation: read_bool_field(op, "expandOperation")
              |> option.unwrap(False),
            merge_operation: read_bool_field(op, "mergeOperation")
              |> option.unwrap(False),
            update_operation: read_bool_field(op, "updateOperation")
              |> option.unwrap(False),
          )
        None ->
          ShopCartTransformEligibleOperationsRecord(
            expand_operation: False,
            merge_operation: False,
            update_operation: False,
          )
      }
      Ok(ShopFeaturesRecord(
        avalara_avatax: read_bool_field(value, "avalaraAvatax")
          |> option.unwrap(False),
        branding: read_string_field(value, "branding") |> option.unwrap(""),
        bundles: bundles,
        captcha: read_bool_field(value, "captcha") |> option.unwrap(False),
        cart_transform: ShopCartTransformFeatureRecord(
          eligible_operations: operations,
        ),
        dynamic_remarketing: read_bool_field(value, "dynamicRemarketing")
          |> option.unwrap(False),
        eligible_for_subscription_migration: read_bool_field(
          value,
          "eligibleForSubscriptionMigration",
        )
          |> option.unwrap(False),
        eligible_for_subscriptions: read_bool_field(
          value,
          "eligibleForSubscriptions",
        )
          |> option.unwrap(False),
        gift_cards: read_bool_field(value, "giftCards") |> option.unwrap(False),
        harmonized_system_code: read_bool_field(value, "harmonizedSystemCode")
          |> option.unwrap(False),
        legacy_subscription_gateway_enabled: read_bool_field(
          value,
          "legacySubscriptionGatewayEnabled",
        )
          |> option.unwrap(False),
        live_view: read_bool_field(value, "liveView") |> option.unwrap(False),
        paypal_express_subscription_gateway_status: read_string_field(
          value,
          "paypalExpressSubscriptionGatewayStatus",
        )
          |> option.unwrap(""),
        reports: read_bool_field(value, "reports") |> option.unwrap(False),
        sells_subscriptions: read_bool_field(value, "sellsSubscriptions")
          |> option.unwrap(False),
        show_metrics: read_bool_field(value, "showMetrics")
          |> option.unwrap(False),
        storefront: read_bool_field(value, "storefront") |> option.unwrap(False),
        unified_markets: read_bool_field(value, "unifiedMarkets")
          |> option.unwrap(False),
      ))
    }
    None -> Error(Nil)
  }
}

fn make_seed_payment_settings(
  source: Option(JsonValue),
) -> PaymentSettingsRecord {
  PaymentSettingsRecord(supported_digital_wallets: case source {
    Some(value) -> read_string_array_field(value, "supportedDigitalWallets")
    None -> []
  })
}

fn make_seed_shop_policy(source: JsonValue) -> Result(ShopPolicyRecord, Nil) {
  use id <- result.try(required_string_field(source, "id"))
  use title <- result.try(required_string_field(source, "title"))
  use body <- result.try(required_string_field(source, "body"))
  use type_ <- result.try(required_string_field(source, "type"))
  use url <- result.try(required_string_field(source, "url"))
  use created_at <- result.try(required_string_field(source, "createdAt"))
  use updated_at <- result.try(required_string_field(source, "updatedAt"))
  Ok(ShopPolicyRecord(
    id: id,
    title: title,
    body: body,
    type_: type_,
    url: url,
    created_at: created_at,
    updated_at: updated_at,
  ))
}

fn seed_shopify_function_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let records = case jsonpath.lookup(capture, "$.seedShopifyFunctions") {
    Some(JArray(nodes)) -> list.filter_map(nodes, make_seed_shopify_function)
    _ -> []
  }

  let seeded_store =
    list.fold(records, proxy.store, fn(current_store, record) {
      let #(_, next_store) =
        store_mod.upsert_staged_shopify_function(current_store, record)
      next_store
    })

  // The local-runtime fixture was captured after the function metadata
  // seed step had advanced the synthetic counters once.
  let #(_, identity_after_id) =
    synthetic_identity.make_synthetic_gid(
      proxy.synthetic_identity,
      "MutationLogEntry",
    )
  let #(_, identity_after_seed) =
    synthetic_identity.make_synthetic_timestamp(identity_after_id)

  draft_proxy.DraftProxy(
    ..proxy,
    store: seeded_store,
    synthetic_identity: identity_after_seed,
  )
}

fn make_seed_shopify_function(
  source: JsonValue,
) -> Result(ShopifyFunctionRecord, Nil) {
  use id <- result.try(required_string_field(source, "id"))
  Ok(
    ShopifyFunctionRecord(
      id: id,
      title: read_string_field(source, "title"),
      handle: read_string_field(source, "handle"),
      api_type: read_string_field(source, "apiType"),
      description: read_string_field(source, "description"),
      app_key: read_string_field(source, "appKey"),
      app: case read_object_field(source, "app") {
        Some(app) -> Some(make_seed_shopify_function_app(app))
        None -> None
      },
    ),
  )
}

fn make_seed_shopify_function_app(
  source: JsonValue,
) -> ShopifyFunctionAppRecord {
  ShopifyFunctionAppRecord(
    typename: read_string_field(source, "__typename"),
    id: read_string_field(source, "id"),
    title: read_string_field(source, "title"),
    handle: read_string_field(source, "handle"),
    api_key: read_string_field(source, "apiKey"),
  )
}

fn seed_gift_card_lifecycle_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let records =
    [
      jsonpath.lookup(
        capture,
        "$.operations.create.response.payload.data.giftCardCreate.giftCard",
      ),
      jsonpath.lookup(
        capture,
        "$.create.response.payload.data.giftCardCreate.giftCard",
      ),
    ]
    |> list.filter_map(fn(candidate) {
      case candidate {
        Some(value) -> make_seed_gift_card(value, Some("api_client"))
        None -> Error(Nil)
      }
    })

  let empty_read_records = case
    jsonpath.lookup(
      capture,
      "$.operations.emptyRead.response.payload.data.giftCards.nodes",
    )
  {
    Some(JArray(nodes)) ->
      list.filter_map(nodes, fn(node) { make_seed_gift_card(node, None) })
    _ -> []
  }

  let records = list.append(records, empty_read_records)
  let seeded_store = case records {
    [] -> proxy.store
    _ -> store_mod.upsert_base_gift_cards(proxy.store, records)
  }
  let seeded_store = case seed_gift_card_configuration(capture) {
    Some(configuration) ->
      store_mod.upsert_base_gift_card_configuration(seeded_store, configuration)
    None -> seeded_store
  }
  draft_proxy.DraftProxy(..proxy, store: seeded_store)
}

fn make_seed_gift_card(
  source: JsonValue,
  source_override: Option(String),
) -> Result(GiftCardRecord, Nil) {
  use id <- result.try(required_string_field(source, "id"))
  case string.starts_with(id, "gid://shopify/GiftCard/") {
    False -> Error(Nil)
    True -> {
      let last_characters =
        read_string_field(source, "lastCharacters")
        |> option.unwrap(gift_card_tail(id))
      let initial_value =
        read_money_record(read_object_field(source, "initialValue"))
      let balance =
        read_money_record(
          read_object_field(source, "balance")
          |> option.or(read_object_field(source, "initialValue")),
        )
      let recipient_attributes_source =
        read_object_field(source, "recipientAttributes")
      let recipient_source =
        recipient_attributes_source
        |> option.then(read_object_field(_, "recipient"))
      let recipient_id =
        read_string_field_from_option(recipient_source, "id")
        |> option.or(read_string_field_from_option(
          read_object_field(source, "recipient"),
          "id",
        ))
      let transactions =
        read_transactions(read_object_field(source, "transactions"))
      Ok(GiftCardRecord(
        id: id,
        legacy_resource_id: read_string_field(source, "legacyResourceId")
          |> option.unwrap(gift_card_tail(id)),
        last_characters: last_characters,
        masked_code: read_string_field(source, "maskedCode")
          |> option.unwrap(masked_code(last_characters)),
        enabled: read_bool_field(source, "enabled") |> option.unwrap(True),
        deactivated_at: read_string_field(source, "deactivatedAt"),
        expires_on: read_string_field(source, "expiresOn"),
        note: read_string_field(source, "note"),
        template_suffix: read_string_field(source, "templateSuffix"),
        created_at: read_string_field(source, "createdAt")
          |> option.unwrap("2026-01-01T00:00:00Z"),
        updated_at: read_string_field(source, "updatedAt")
          |> option.unwrap("2026-01-01T00:00:00Z"),
        initial_value: initial_value,
        balance: balance,
        customer_id: read_string_field_from_option(
          read_object_field(source, "customer"),
          "id",
        ),
        recipient_id: recipient_id,
        source: case source_override {
          Some(_) -> source_override
          None -> read_string_field(source, "source")
        },
        recipient_attributes: make_seed_recipient_attributes(
          recipient_attributes_source,
          recipient_id,
        ),
        transactions: transactions,
      ))
    }
  }
}

fn seed_gift_card_configuration(
  capture: JsonValue,
) -> Option(GiftCardConfigurationRecord) {
  let primary =
    jsonpath.lookup(
      capture,
      "$.operations.configurationRead.response.payload.data.giftCardConfiguration",
    )
  let fallback =
    jsonpath.lookup(
      capture,
      "$.configurationRead.response.payload.data.giftCardConfiguration",
    )
  case primary |> option.or(fallback) {
    Some(value) ->
      Some(GiftCardConfigurationRecord(
        issue_limit: read_money_record(read_object_field(value, "issueLimit")),
        purchase_limit: read_money_record(read_object_field(
          value,
          "purchaseLimit",
        )),
      ))
    None -> None
  }
}

fn make_seed_recipient_attributes(
  source: Option(JsonValue),
  recipient_id: Option(String),
) -> Option(GiftCardRecipientAttributesRecord) {
  case source {
    None -> None
    Some(value) ->
      Some(GiftCardRecipientAttributesRecord(
        id: recipient_id,
        message: read_string_field(value, "message"),
        preferred_name: read_string_field(value, "preferredName"),
        send_notification_at: read_string_field(value, "sendNotificationAt"),
      ))
  }
}

fn read_transactions(
  source: Option(JsonValue),
) -> List(GiftCardTransactionRecord) {
  case source |> option.then(read_array_field(_, "nodes")) {
    Some(nodes) ->
      list.filter_map(nodes, fn(node) {
        let amount = read_money_record(read_object_field(node, "amount"))
        Ok(GiftCardTransactionRecord(
          id: read_string_field(node, "id")
            |> option.unwrap("gid://shopify/GiftCardTransaction/0"),
          kind: case string.starts_with(amount.amount, "-") {
            True -> "DEBIT"
            False -> "CREDIT"
          },
          amount: amount,
          processed_at: read_string_field(node, "processedAt")
            |> option.unwrap("2026-01-01T00:00:00Z"),
          note: read_string_field(node, "note"),
        ))
      })
    None -> []
  }
}

fn read_money_record(source: Option(JsonValue)) -> Money {
  case source {
    Some(value) ->
      Money(
        amount: read_string_field(value, "amount") |> option.unwrap("0.0"),
        currency_code: read_string_field(value, "currencyCode")
          |> option.unwrap("CAD"),
      )
    None -> Money(amount: "0.0", currency_code: "CAD")
  }
}

fn required_string_field(
  value: JsonValue,
  name: String,
) -> Result(String, Nil) {
  case read_string_field(value, name) {
    Some(s) -> Ok(s)
    None -> Error(Nil)
  }
}

fn read_string_field(value: JsonValue, name: String) -> Option(String) {
  case json_value.field(value, name) {
    Some(JString(s)) -> Some(s)
    _ -> None
  }
}

fn read_string_field_from_option(
  value: Option(JsonValue),
  name: String,
) -> Option(String) {
  case value {
    Some(v) -> read_string_field(v, name)
    None -> None
  }
}

fn read_bool_field(value: JsonValue, name: String) -> Option(Bool) {
  case json_value.field(value, name) {
    Some(JBool(b)) -> Some(b)
    _ -> None
  }
}

fn read_int_field(value: JsonValue, name: String) -> Option(Int) {
  case json_value.field(value, name) {
    Some(JInt(i)) -> Some(i)
    _ -> None
  }
}

fn read_float_field(value: JsonValue, name: String) -> Option(Float) {
  case json_value.field(value, name) {
    Some(JFloat(f)) -> Some(f)
    Some(JInt(i)) -> Some(int.to_float(i))
    _ -> None
  }
}

fn read_string_array_field(value: JsonValue, name: String) -> List(String) {
  case read_array_field(value, name) {
    Some(items) ->
      list.filter_map(items, fn(item) {
        case item {
          JString(s) -> Ok(s)
          _ -> Error(Nil)
        }
      })
    None -> []
  }
}

fn read_object_field(value: JsonValue, name: String) -> Option(JsonValue) {
  case json_value.field(value, name) {
    Some(JObject(_)) as object -> object
    _ -> None
  }
}

fn read_array_field(value: JsonValue, name: String) -> Option(List(JsonValue)) {
  case json_value.field(value, name) {
    Some(JArray(items)) -> Some(items)
    _ -> None
  }
}

fn gift_card_tail(id: String) -> String {
  case string.split(id, on: "/") |> list.last {
    Ok(tail_with_query) ->
      case string.split(tail_with_query, on: "?") {
        [tail, ..] -> tail
        [] -> id
      }
    Error(_) -> id
  }
}

fn masked_code(last_characters: String) -> String {
  "•••• •••• •••• " <> last_characters
}

fn run_targets(
  config: RunnerConfig,
  parsed: Spec,
  capture: JsonValue,
  primary_response: JsonValue,
  proxy: DraftProxy,
) -> Result(#(DraftProxy, List(TargetReport)), RunError) {
  list.try_fold(parsed.targets, #(proxy, []), fn(state, target) {
    let #(current_proxy, acc_reports) = state
    use #(next_proxy, report) <- result.try(run_target(
      config,
      parsed,
      target,
      capture,
      primary_response,
      current_proxy,
    ))
    Ok(#(next_proxy, [report, ..acc_reports]))
  })
  |> result.map(fn(state) {
    let #(final_proxy, reports) = state
    #(final_proxy, list.reverse(reports))
  })
}

fn run_target(
  config: RunnerConfig,
  parsed: Spec,
  target: Target,
  capture: JsonValue,
  primary_response: JsonValue,
  proxy: DraftProxy,
) -> Result(#(DraftProxy, TargetReport), RunError) {
  use #(actual_response, next_proxy) <- result.try(actual_response_for(
    config,
    target,
    capture,
    primary_response,
    proxy,
  ))
  let expected_opt = jsonpath.lookup(capture, target.capture_path)
  let actual_opt = jsonpath.lookup(actual_response, target.proxy_path)
  case expected_opt, actual_opt {
    None, _ ->
      Error(CaptureUnresolved(target: target.name, path: target.capture_path))
    _, None ->
      Error(ProxyUnresolved(target: target.name, path: target.proxy_path))
    Some(expected), Some(actual) -> {
      let rules = spec.rules_for(parsed, target)
      let mismatches = case target.selected_paths {
        [] -> diff.diff_with_expected(expected, actual, rules)
        selected_paths ->
          diff.diff_selected_paths(expected, actual, selected_paths, rules)
      }
      Ok(#(
        next_proxy,
        TargetReport(
          name: target.name,
          capture_path: target.capture_path,
          proxy_path: target.proxy_path,
          mismatches: mismatches,
        ),
      ))
    }
  }
}

/// Resolve which JsonValue tree to use as the proxy-side response for
/// a target. Targets without a per-target override reuse the primary
/// response (no extra HTTP call). Override targets execute their own
/// request, threading proxy state forward.
fn actual_response_for(
  config: RunnerConfig,
  target: Target,
  capture: JsonValue,
  primary_response: JsonValue,
  proxy: DraftProxy,
) -> Result(#(JsonValue, DraftProxy), RunError) {
  case target.request {
    ReusePrimary -> Ok(#(primary_response, proxy))
    OverrideRequest(request: request) -> {
      use document <- result.try(
        read_file(resolve(config, request.document_path)),
      )
      use variables <- result.try(resolve_variables(
        config,
        request.variables,
        capture,
        Some(primary_response),
        target.name,
      ))
      use #(response, next_proxy) <- result.try(execute(
        proxy,
        document,
        variables,
        target.name,
      ))
      use value <- result.try(parse_response_body(response))
      Ok(#(value, next_proxy))
    }
  }
}

fn parse_spec(source: String) -> Result(Spec, RunError) {
  case spec.decode(source) {
    Ok(s) -> Ok(s)
    Error(_) -> Error(SpecError(reason: "could not decode parity spec"))
  }
}

fn load_capture(
  config: RunnerConfig,
  parsed: Spec,
) -> Result(JsonValue, RunError) {
  let path = resolve(config, parsed.capture_file)
  use source <- result.try(read_file(path))
  parse_json(path, source)
}

fn resolve_variables(
  config: RunnerConfig,
  variables: spec.ParityVariables,
  capture: JsonValue,
  primary_response: Option(JsonValue),
  context: String,
) -> Result(JsonValue, RunError) {
  case variables {
    NoVariables -> Ok(JObject([]))
    VariablesFromCapture(path: path) ->
      case jsonpath.lookup(capture, path) {
        Some(value) -> Ok(value)
        None -> Error(VariablesUnresolved(path: path))
      }
    VariablesFromFile(path: path) -> {
      let resolved = resolve(config, path)
      use source <- result.try(read_file(resolved))
      parse_json(resolved, source)
    }
    VariablesInline(template: template) -> {
      let _ = context
      substitute(template, primary_response, capture)
    }
  }
}

/// Walk an inline variables template, substituting any
/// `{"fromPrimaryProxyPath": "$..."}` or `{"fromCapturePath": "$..."}`
/// markers with the corresponding value. Other nodes pass through.
fn substitute(
  template: JsonValue,
  primary: Option(JsonValue),
  capture: JsonValue,
) -> Result(JsonValue, RunError) {
  case as_primary_ref(template) {
    Some(path) ->
      case primary {
        None -> Error(PrimaryRefUnresolved(path: path))
        Some(root) ->
          case jsonpath.lookup(root, path) {
            Some(value) -> Ok(value)
            None -> Error(PrimaryRefUnresolved(path: path))
          }
      }
    None ->
      case as_capture_ref(template) {
        Some(path) ->
          case jsonpath.lookup(capture, path) {
            Some(value) -> Ok(value)
            None -> Error(CaptureRefUnresolved(path: path))
          }
        None ->
          case template {
            JObject(entries) ->
              entries
              |> list.try_map(fn(pair) {
                let #(k, v) = pair
                case substitute(v, primary, capture) {
                  Ok(v2) -> Ok(#(k, v2))
                  Error(e) -> Error(e)
                }
              })
              |> result.map(JObject)
            JArray(items) ->
              items
              |> list.try_map(fn(item) { substitute(item, primary, capture) })
              |> result.map(JArray)
            leaf -> Ok(leaf)
          }
      }
  }
}

/// If `value` is exactly `{"fromPrimaryProxyPath": "..."}` (one entry
/// with a string value), return the path. Otherwise None.
fn as_primary_ref(value: JsonValue) -> Option(String) {
  case value {
    JObject([#("fromPrimaryProxyPath", json_value.JString(path))]) -> Some(path)
    _ -> None
  }
}

/// If `value` is exactly `{"fromCapturePath": "..."}` (one entry with
/// a string value), return the path. Otherwise None.
fn as_capture_ref(value: JsonValue) -> Option(String) {
  case value {
    JObject([#("fromCapturePath", json_value.JString(path))]) -> Some(path)
    _ -> None
  }
}

fn execute(
  proxy: DraftProxy,
  document: String,
  variables: JsonValue,
  context: String,
) -> Result(#(Response, DraftProxy), RunError) {
  let body = build_graphql_body(document, variables)
  let request =
    Request(
      method: "POST",
      path: "/admin/api/2025-01/graphql.json",
      headers: dict.new(),
      body: body,
    )
  let #(response, next_proxy) = draft_proxy.process_request(proxy, request)
  case response.status {
    200 -> Ok(#(response, next_proxy))
    status ->
      Error(ProxyStatus(
        target: context,
        status: status,
        body: json.to_string(response.body),
      ))
  }
}

fn build_graphql_body(document: String, variables: JsonValue) -> String {
  let query = json.to_string(json.string(document))
  let vars = json_value.to_string(variables)
  "{\"query\":" <> query <> ",\"variables\":" <> vars <> "}"
}

fn parse_response_body(response: Response) -> Result(JsonValue, RunError) {
  let serialized = json.to_string(response.body)
  parse_json("<proxy-response>", serialized)
}

fn read_file(path: String) -> Result(String, RunError) {
  case simplifile.read(path) {
    Ok(s) -> Ok(s)
    Error(reason) ->
      Error(FileError(path: path, reason: simplifile.describe_error(reason)))
  }
}

fn parse_json(path: String, source: String) -> Result(JsonValue, RunError) {
  case json_value.parse(source) {
    Ok(v) -> Ok(v)
    Error(e) -> Error(JsonError(path: path, reason: e.message))
  }
}

fn resolve(config: RunnerConfig, path: String) -> String {
  case string.starts_with(path, "/") {
    True -> path
    False -> config.repo_root <> "/" <> path
  }
}

pub fn has_mismatches(report: Report) -> Bool {
  list.any(report.targets, fn(t) { t.mismatches != [] })
}

pub fn render(report: Report) -> String {
  case has_mismatches(report) {
    False -> "OK: " <> report.scenario_id
    True ->
      report.scenario_id
      <> "\n"
      <> string.join(list.map(report.targets, render_target), "\n")
  }
}

fn render_target(target: TargetReport) -> String {
  case target.mismatches {
    [] -> "  [" <> target.name <> "] OK"
    mismatches ->
      "  ["
      <> target.name
      <> "] "
      <> int.to_string(list.length(mismatches))
      <> " mismatch(es):\n"
      <> diff.render_mismatches(mismatches)
  }
}

pub fn into_assert(report: Report) -> Result(Nil, String) {
  case has_mismatches(report) {
    False -> Ok(Nil)
    True -> Error(render(report))
  }
}

pub fn render_error(error: RunError) -> String {
  case error {
    FileError(path, reason) -> "file error at " <> path <> ": " <> reason
    JsonError(path, reason) -> "json error at " <> path <> ": " <> reason
    SpecError(reason) -> "spec error: " <> reason
    VariablesUnresolved(path) -> "variables jsonpath did not resolve: " <> path
    PrimaryRefUnresolved(path) ->
      "fromPrimaryProxyPath did not resolve in primary response: " <> path
    CaptureRefUnresolved(path) ->
      "fromCapturePath did not resolve in capture: " <> path
    CaptureUnresolved(target, path) ->
      "capture jsonpath did not resolve for target '" <> target <> "': " <> path
    ProxyUnresolved(target, path) ->
      "proxy response jsonpath did not resolve for target '"
      <> target
      <> "': "
      <> path
    ProxyStatus(target, status, body) ->
      "proxy returned status "
      <> int.to_string(status)
      <> " for target '"
      <> target
      <> "': "
      <> body
  }
}

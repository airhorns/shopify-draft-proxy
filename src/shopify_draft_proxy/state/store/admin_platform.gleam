//// Store operations for Admin platform utility records.

import gleam/dict
import gleam/list
import gleam/option.{type Option, None, Some}
import shopify_draft_proxy/state/store/shared.{
  append_unique_id, dedupe_strings, dict_has, list_to_set, option_to_result,
  string_compare,
}
import shopify_draft_proxy/state/store/types.{
  type Store, BaseState, StagedState, Store,
}
import shopify_draft_proxy/state/types.{
  type AdminPlatformFlowSignatureRecord, type AdminPlatformFlowTriggerRecord,
  type AdminPlatformGenericNodeRecord, type AdminPlatformTaxonomyCategoryRecord,
  type BackupRegionRecord, type ShopRecord, AdminPlatformTaxonomyCategoryRecord,
} as types_mod

// ---------------------------------------------------------------------------
// Admin Platform utility slice
// ---------------------------------------------------------------------------

/// Seed or update the captured/effective backup region in base state.
pub fn upsert_base_backup_region(
  store: Store,
  record: BackupRegionRecord,
) -> Store {
  Store(
    ..store,
    base_state: BaseState(..store.base_state, backup_region: Some(record)),
  )
}

/// Stage the shop backup region. Mirrors `stageBackupRegion`.
pub fn stage_backup_region(
  store: Store,
  record: BackupRegionRecord,
) -> #(BackupRegionRecord, Store) {
  #(
    record,
    Store(
      ..store,
      staged_state: StagedState(
        ..store.staged_state,
        backup_region: Some(record),
      ),
    ),
  )
}

/// Return the staged backup region when present, otherwise the seeded base
/// region. The domain handler applies the no-shop captured fallback.
pub fn get_effective_backup_region(store: Store) -> Option(BackupRegionRecord) {
  case store.staged_state.backup_region {
    Some(region) -> Some(region)
    None -> store.base_state.backup_region
  }
}

pub fn upsert_base_admin_platform_generic_nodes(
  store: Store,
  records: List(AdminPlatformGenericNodeRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        admin_platform_generic_nodes: dict.insert(
          base.admin_platform_generic_nodes,
          record.id,
          record,
        ),
      ),
    )
  })
}

pub fn upsert_staged_admin_platform_generic_nodes(
  store: Store,
  records: List(AdminPlatformGenericNodeRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let staged = acc.staged_state
    Store(
      ..acc,
      staged_state: StagedState(
        ..staged,
        admin_platform_generic_nodes: dict.insert(
          staged.admin_platform_generic_nodes,
          record.id,
          record,
        ),
      ),
    )
  })
}

pub fn get_effective_admin_platform_generic_node_by_id(
  store: Store,
  id: String,
) -> Option(AdminPlatformGenericNodeRecord) {
  case dict.get(store.staged_state.admin_platform_generic_nodes, id) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(store.base_state.admin_platform_generic_nodes, id) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

pub fn upsert_base_admin_platform_taxonomy_categories(
  store: Store,
  records: List(AdminPlatformTaxonomyCategoryRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    let existing = dict.get(base.admin_platform_taxonomy_categories, record.id)
    let merged = case existing {
      Ok(current) ->
        AdminPlatformTaxonomyCategoryRecord(
          ..record,
          cursor: record.cursor |> option.or(current.cursor),
        )
      Error(_) -> record
    }
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        admin_platform_taxonomy_categories: dict.insert(
          base.admin_platform_taxonomy_categories,
          record.id,
          merged,
        ),
        admin_platform_taxonomy_category_order: append_unique_id(
          base.admin_platform_taxonomy_category_order,
          record.id,
        ),
      ),
    )
  })
}

pub fn get_effective_admin_platform_taxonomy_category_by_id(
  store: Store,
  id: String,
) -> Option(AdminPlatformTaxonomyCategoryRecord) {
  case dict.get(store.staged_state.admin_platform_taxonomy_categories, id) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(store.base_state.admin_platform_taxonomy_categories, id) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

pub fn list_effective_admin_platform_taxonomy_categories(
  store: Store,
) -> List(AdminPlatformTaxonomyCategoryRecord) {
  let ordered_ids =
    list.append(
      store.base_state.admin_platform_taxonomy_category_order,
      store.staged_state.admin_platform_taxonomy_category_order,
    )
    |> dedupe_strings()
  let ordered =
    ordered_ids
    |> list.filter_map(fn(id) {
      get_effective_admin_platform_taxonomy_category_by_id(store, id)
      |> option_to_result
    })
  let ordered_lookup = list_to_set(ordered_ids)
  let unordered =
    dict.merge(
      store.base_state.admin_platform_taxonomy_categories,
      store.staged_state.admin_platform_taxonomy_categories,
    )
    |> dict.keys()
    |> list.filter(fn(id) { !dict_has(ordered_lookup, id) })
    |> list.sort(string_compare)
    |> list.filter_map(fn(id) {
      get_effective_admin_platform_taxonomy_category_by_id(store, id)
      |> option_to_result
    })
  list.append(ordered, unordered)
}

// ---------------------------------------------------------------------------
// Store properties slice
// ---------------------------------------------------------------------------

pub fn upsert_base_shop(store: Store, record: ShopRecord) -> Store {
  Store(..store, base_state: BaseState(..store.base_state, shop: Some(record)))
}

pub fn stage_shop(store: Store, record: ShopRecord) -> #(ShopRecord, Store) {
  #(
    record,
    Store(
      ..store,
      staged_state: StagedState(..store.staged_state, shop: Some(record)),
    ),
  )
}

pub fn get_effective_shop(store: Store) -> Option(ShopRecord) {
  case store.staged_state.shop {
    Some(shop) -> Some(shop)
    None -> store.base_state.shop
  }
}

pub fn shop_sells_subscriptions(store: Store) -> Bool {
  case get_effective_shop(store) {
    Some(shop) -> shop.features.sells_subscriptions
    None -> False
  }
}

pub fn shop_discounts_by_market_enabled(store: Store) -> Bool {
  case get_effective_shop(store) {
    Some(shop) -> shop.features.discounts_by_market_enabled
    None -> False
  }
}

pub fn shop_markets_home_enabled(store: Store) -> Bool {
  case get_effective_shop(store) {
    Some(shop) -> shop.features.unified_markets
    None -> True
  }
}

pub fn shop_market_plan_limit(store: Store) -> Int {
  case get_effective_shop(store) {
    Some(shop) -> shop.features.markets_granted
    None -> default_market_plan_limit()
  }
}

fn default_market_plan_limit() -> Int {
  50
}

pub fn payment_gateway_by_id(
  store: Store,
  id: String,
) -> Option(types_mod.PaymentGatewayRecord) {
  case get_effective_shop(store) {
    Some(shop) ->
      case
        shop.payment_settings.payment_gateways
        |> list.find(fn(gateway) { gateway.id == id })
      {
        Ok(gateway) -> Some(gateway)
        Error(_) -> None
      }
    None -> None
  }
}

pub fn set_shop_payment_gateways(
  store: Store,
  payment_gateways: List(types_mod.PaymentGatewayRecord),
) -> Store {
  case store.staged_state.shop {
    Some(shop) -> {
      let shop = shop_with_payment_gateways(shop, payment_gateways)
      Store(
        ..store,
        staged_state: StagedState(..store.staged_state, shop: Some(shop)),
      )
    }
    None -> {
      let shop =
        store.base_state.shop
        |> option.unwrap(default_synthetic_shop())
        |> shop_with_payment_gateways(payment_gateways)
      Store(
        ..store,
        base_state: BaseState(..store.base_state, shop: Some(shop)),
      )
    }
  }
}

pub fn set_shop_sells_subscriptions(
  store: Store,
  sells_subscriptions: Bool,
) -> Store {
  case store.staged_state.shop {
    Some(shop) -> {
      let shop = shop_with_sells_subscriptions(shop, sells_subscriptions)
      Store(
        ..store,
        staged_state: StagedState(..store.staged_state, shop: Some(shop)),
      )
    }
    None -> {
      let shop =
        store.base_state.shop
        |> option.unwrap(default_synthetic_shop())
        |> shop_with_sells_subscriptions(sells_subscriptions)
      Store(
        ..store,
        base_state: BaseState(..store.base_state, shop: Some(shop)),
      )
    }
  }
}

pub fn set_shop_discounts_by_market_enabled(
  store: Store,
  discounts_by_market_enabled: Bool,
) -> Store {
  case store.staged_state.shop {
    Some(shop) -> {
      let shop =
        shop_with_discounts_by_market_enabled(shop, discounts_by_market_enabled)
      Store(
        ..store,
        staged_state: StagedState(..store.staged_state, shop: Some(shop)),
      )
    }
    None -> {
      let shop =
        store.base_state.shop
        |> option.unwrap(default_synthetic_shop())
        |> shop_with_discounts_by_market_enabled(discounts_by_market_enabled)
      Store(
        ..store,
        base_state: BaseState(..store.base_state, shop: Some(shop)),
      )
    }
  }
}

fn shop_with_sells_subscriptions(
  shop: ShopRecord,
  sells_subscriptions: Bool,
) -> ShopRecord {
  let features = shop.features
  types_mod.ShopRecord(
    ..shop,
    features: types_mod.ShopFeaturesRecord(
      ..features,
      sells_subscriptions: sells_subscriptions,
    ),
  )
}

fn shop_with_discounts_by_market_enabled(
  shop: ShopRecord,
  discounts_by_market_enabled: Bool,
) -> ShopRecord {
  let features = shop.features
  types_mod.ShopRecord(
    ..shop,
    features: types_mod.ShopFeaturesRecord(
      ..features,
      discounts_by_market_enabled: discounts_by_market_enabled,
    ),
  )
}

fn shop_with_payment_gateways(
  shop: ShopRecord,
  payment_gateways: List(types_mod.PaymentGatewayRecord),
) -> ShopRecord {
  let payment_settings = shop.payment_settings
  types_mod.ShopRecord(
    ..shop,
    payment_settings: types_mod.PaymentSettingsRecord(
      ..payment_settings,
      payment_gateways: payment_gateways,
    ),
  )
}

fn default_synthetic_shop() -> ShopRecord {
  types_mod.ShopRecord(
    id: "gid://shopify/Shop/1?shopify-draft-proxy=synthetic",
    name: "Shopify Draft Proxy",
    myshopify_domain: "shopify-draft-proxy.myshopify.com",
    url: "https://shopify-draft-proxy.myshopify.com",
    primary_domain: types_mod.ShopDomainRecord(
      id: "gid://shopify/Domain/1?shopify-draft-proxy=synthetic",
      host: "shopify-draft-proxy.myshopify.com",
      url: "https://shopify-draft-proxy.myshopify.com",
      ssl_enabled: True,
    ),
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
    shop_address: types_mod.ShopAddressRecord(
      id: "gid://shopify/ShopAddress/1?shopify-draft-proxy=synthetic",
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
    plan: types_mod.ShopPlanRecord(
      partner_development: False,
      public_display_name: "",
      shopify_plus: False,
    ),
    resource_limits: types_mod.ShopResourceLimitsRecord(
      location_limit: 0,
      max_product_options: 0,
      max_product_variants: 0,
      redirect_limit_reached: False,
    ),
    features: default_shop_features(),
    payment_settings: types_mod.PaymentSettingsRecord(
      supported_digital_wallets: [],
      payment_gateways: [],
    ),
    shop_policies: [],
  )
}

fn default_shop_features() -> types_mod.ShopFeaturesRecord {
  types_mod.ShopFeaturesRecord(
    avalara_avatax: False,
    branding: "SHOPIFY",
    bundles: types_mod.ShopBundlesFeatureRecord(
      eligible_for_bundles: False,
      ineligibility_reason: None,
      sells_bundles: False,
    ),
    captcha: False,
    cart_transform: types_mod.ShopCartTransformFeatureRecord(
      eligible_operations: types_mod.ShopCartTransformEligibleOperationsRecord(
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
    discounts_by_market_enabled: False,
    markets_granted: default_market_plan_limit(),
    sells_subscriptions: False,
    show_metrics: False,
    storefront: False,
    unified_markets: True,
  )
}

pub fn stage_admin_platform_flow_signature(
  store: Store,
  record: AdminPlatformFlowSignatureRecord,
) -> #(AdminPlatformFlowSignatureRecord, Store) {
  let staged = store.staged_state
  let known =
    list.contains(staged.admin_platform_flow_signature_order, record.id)
    || dict_has(staged.admin_platform_flow_signatures, record.id)
  let order = case known {
    True -> staged.admin_platform_flow_signature_order
    False ->
      list.append(staged.admin_platform_flow_signature_order, [record.id])
  }
  #(
    record,
    Store(
      ..store,
      staged_state: StagedState(
        ..staged,
        admin_platform_flow_signatures: dict.insert(
          staged.admin_platform_flow_signatures,
          record.id,
          record,
        ),
        admin_platform_flow_signature_order: order,
      ),
    ),
  )
}

/// Stage a local Flow trigger receipt audit record.
pub fn stage_admin_platform_flow_trigger(
  store: Store,
  record: AdminPlatformFlowTriggerRecord,
) -> #(AdminPlatformFlowTriggerRecord, Store) {
  let staged = store.staged_state
  let known =
    list.contains(staged.admin_platform_flow_trigger_order, record.id)
    || dict_has(staged.admin_platform_flow_triggers, record.id)
  let order = case known {
    True -> staged.admin_platform_flow_trigger_order
    False -> list.append(staged.admin_platform_flow_trigger_order, [record.id])
  }
  #(
    record,
    Store(
      ..store,
      staged_state: StagedState(
        ..staged,
        admin_platform_flow_triggers: dict.insert(
          staged.admin_platform_flow_triggers,
          record.id,
          record,
        ),
        admin_platform_flow_trigger_order: order,
      ),
    ),
  )
}
/// Upsert one or more saved-search records into the base state.
/// Mirrors `upsertBaseSavedSearches`. Removes any existing
/// "deleted" markers (in either base or staged) for the same id, since
/// the upstream answer wins.
// ---------------------------------------------------------------------------
// Saved-search slice
// ---------------------------------------------------------------------------

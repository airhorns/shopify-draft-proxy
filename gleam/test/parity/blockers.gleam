//// Explicit blockers for TS-ready parity specs that the Gleam proxy
//// runner executes but does not yet require to pass. The corpus test
//// still runs every blocked spec; a blocker is stale if its spec starts
//// passing, which keeps this list from hiding runnable coverage.

import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/string

pub fn known_passing_paths() -> List(String) {
  [
    "config/parity-specs/admin-platform/admin-platform-backup-region-update.json",
    "config/parity-specs/admin-platform/admin-platform-store-property-node-reads.json",
    "config/parity-specs/apps/delegate-access-token-current-input-local-staging.json",
    "config/parity-specs/events/event-empty-read.json",
    "config/parity-specs/functions/functions-owner-metadata-local-staging.json",
    "config/parity-specs/gift-cards/gift-card-search-filters.json",
    "config/parity-specs/marketing/marketing-activity-lifecycle.json",
    "config/parity-specs/marketing/marketing-engagement-lifecycle.json",
    "config/parity-specs/marketing/marketing-native-activity-lifecycle.json",
    "config/parity-specs/metafields/metafield-definitions-product-empty-read.json",
    "config/parity-specs/metafields/standard-metafield-definition-enable-validation.json",
    "config/parity-specs/saved-searches/saved-search-local-staging.json",
    "config/parity-specs/saved-searches/saved-search-query-grammar.json",
    "config/parity-specs/saved-searches/saved-search-resource-roots.json",
    "config/parity-specs/segments/customer-segment-members-query-lifecycle.json",
    "config/parity-specs/segments/segment-create-invalid-query-validation.json",
    "config/parity-specs/segments/segment-delete-unknown-id-validation.json",
    "config/parity-specs/segments/segment-query-grammar-not-contains.json",
    "config/parity-specs/segments/segment-update-unknown-id-validation.json",
    "config/parity-specs/shipping-fulfillments/delivery-settings-read.json",
    "config/parity-specs/store-properties/shop-baseline-read.json",
    "config/parity-specs/store-properties/shopPolicyUpdate-parity.json",
    "config/parity-specs/webhooks/webhook-subscription-catalog-read.json",
    "config/parity-specs/webhooks/webhook-subscription-conformance.json",
    "config/parity-specs/webhooks/webhook-subscription-required-argument-validation.json",
  ]
}

pub fn blocker_for(path: String) -> Option(String) {
  case list.contains(known_passing_paths(), path) {
    True -> None
    False -> blocker_reason(path)
  }
}

fn blocker_reason(path: String) -> Option(String) {
  let reasons = [
    #(
      "config/parity-specs/admin-platform/",
      "Admin Platform parity is still limited to utility, backup-region, and Store Properties-owned node reads; product/customer/location/market/taxonomy/delivery/payment node state is not ported.",
    ),
    #(
      "config/parity-specs/apps/",
      "The App billing graph still has known subscription line-item and installation read-after-write ID parity gaps.",
    ),
    #(
      "config/parity-specs/b2b/",
      "B2B company/contact/location lifecycle state and dispatch are not ported to Gleam.",
    ),
    #(
      "config/parity-specs/bulk-operations/",
      "Bulk Operation parity needs captured job/result seeding beyond the current local read/cancel foundation.",
    ),
    #(
      "config/parity-specs/customers/",
      "Customer state, mutations, search, consent, merge, address, and store-credit domains are not ported to Gleam.",
    ),
    #(
      "config/parity-specs/discounts/",
      "Discount lifecycle/read state and dispatch are not ported to Gleam.",
    ),
    #(
      "config/parity-specs/functions/",
      "Remaining Functions scenarios need fixture-correctness or metadata seeding beyond the owner-metadata local-staging path.",
    ),
    #(
      "config/parity-specs/gift-cards/",
      "Gift Card lifecycle parity needs full configuration/recipient/transaction state and selected-path contract coverage beyond search filters.",
    ),
    #(
      "config/parity-specs/localization/",
      "Localization read/mutation parity still needs capture seeding for locale and translation state.",
    ),
    #(
      "config/parity-specs/marketing/",
      "Remaining Marketing parity needs upstream/capture seeding for catalog reads and remote-id update flows.",
    ),
    #(
      "config/parity-specs/markets/",
      "Markets state, reads, and mutations are not ported to Gleam.",
    ),
    #(
      "config/parity-specs/media/",
      "Media state, staged upload, file, and product-media lifecycles are not ported to Gleam.",
    ),
    #(
      "config/parity-specs/metafields/",
      "Metafield definition/metafield lifecycle parity needs owner-resource seeding and unported mutation support.",
    ),
    #(
      "config/parity-specs/metaobjects/",
      "Metaobject and metaobject-definition state/mutation parity is not ported to Gleam.",
    ),
    #(
      "config/parity-specs/online-store/",
      "Online Store blogs/articles/menus/pages/redirects lifecycle state is not ported to Gleam.",
    ),
    #(
      "config/parity-specs/online-store-article-media-navigation-follow-through.json",
      "Online Store article/media/navigation follow-through depends on the unported Online Store domain.",
    ),
    #(
      "config/parity-specs/orders/",
      "Order, draft order, fulfillment, transaction, and edit lifecycle state is not ported to Gleam.",
    ),
    #(
      "config/parity-specs/payments/",
      "Payments, payment customizations, disputes, POS, and risk roots are not ported to Gleam.",
    ),
    #(
      "config/parity-specs/privacy/",
      "Privacy customer data-sale opt-out lifecycle depends on unported customer state.",
    ),
    #(
      "config/parity-specs/products/",
      "Product, variant, option, collection, inventory, publication, selling-plan, and media lifecycle parity is not ported to Gleam.",
    ),
    #(
      "config/parity-specs/shipping-fulfillments/",
      "Shipping/Fulfillments parity beyond delivery settings needs location, carrier-service, fulfillment-service, delivery-profile, package, and fulfillment-order state.",
    ),
    #(
      "config/parity-specs/store-properties/",
      "Store Properties parity is currently limited to shop and policy update; locations, carrier/fulfillment services, publications, business entities, and payment settings are unported.",
    ),
  ]
  case list.find(reasons, fn(reason) { string.starts_with(path, reason.0) }) {
    Ok(reason) -> Some(reason.1)
    Error(_) ->
      Some("No domain-specific Gleam parity blocker has been classified yet.")
  }
}

//// Shared local mutation dispatch for normal Admin mutations and
//// bulkOperationRunMutation inner-line replay.

import gleam/dict.{type Dict}
import gleam/option.{type Option, None, Some}
import gleam/string
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/admin_platform
import shopify_draft_proxy/proxy/apps
import shopify_draft_proxy/proxy/b2b
import shopify_draft_proxy/proxy/customers
import shopify_draft_proxy/proxy/discounts
import shopify_draft_proxy/proxy/functions
import shopify_draft_proxy/proxy/gift_cards
import shopify_draft_proxy/proxy/localization
import shopify_draft_proxy/proxy/marketing
import shopify_draft_proxy/proxy/markets
import shopify_draft_proxy/proxy/media
import shopify_draft_proxy/proxy/metafield_definitions
import shopify_draft_proxy/proxy/metaobject_definitions
import shopify_draft_proxy/proxy/mutation_helpers.{type MutationOutcome}
import shopify_draft_proxy/proxy/online_store
import shopify_draft_proxy/proxy/orders
import shopify_draft_proxy/proxy/payments
import shopify_draft_proxy/proxy/privacy
import shopify_draft_proxy/proxy/products
import shopify_draft_proxy/proxy/saved_searches
import shopify_draft_proxy/proxy/segments
import shopify_draft_proxy/proxy/shipping_fulfillments
import shopify_draft_proxy/proxy/store_properties
import shopify_draft_proxy/proxy/upstream_query
import shopify_draft_proxy/proxy/webhooks
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}

pub type MutationHandler =
  fn(
    Store,
    SyntheticIdentityRegistry,
    String,
    String,
    Dict(String, root_field.ResolvedValue),
    upstream_query.UpstreamContext,
  ) -> MutationOutcome

pub fn handler_for(name: String, query: String) -> Option(MutationHandler) {
  case publishable_mutation_requests_store_properties(name, query) {
    True -> Some(store_properties.process_mutation)
    False -> non_store_publishable_handler_for(name)
  }
}

pub fn has_handler(name: String, query: String) -> Bool {
  case handler_for(name, query) {
    Some(_) -> True
    None -> False
  }
}

pub fn domain_for(name: String, query: String) -> Option(String) {
  case publishable_mutation_requests_store_properties(name, query) {
    True -> Some("store-properties")
    False -> non_store_publishable_domain_for(name)
  }
}

fn non_store_publishable_handler_for(name: String) -> Option(MutationHandler) {
  first_matching_handler([
    #(payments.is_payments_mutation_root(name), payments.process_mutation),
    #(products.is_products_mutation_root(name), products.process_mutation),
    #(
      store_properties.is_store_properties_mutation_root(name),
      store_properties.process_mutation,
    ),
    #(
      saved_searches.is_saved_search_mutation_root(name),
      saved_searches.process_mutation,
    ),
    #(
      webhooks.is_webhook_subscription_mutation_root(name),
      webhooks.process_mutation,
    ),
    #(apps.is_app_mutation_root(name), apps.process_mutation),
    #(functions.is_function_mutation_root(name), functions.process_mutation),
    #(gift_cards.is_gift_card_mutation_root(name), gift_cards.process_mutation),
    #(discounts.is_discount_mutation_root(name), discounts.process_mutation),
    #(b2b.is_b2b_mutation_root(name), b2b.process_mutation),
    #(segments.is_segment_mutation_root(name), segments.process_mutation),
    #(
      metafield_definitions.is_metafield_definitions_mutation_root(name),
      metafield_definitions.process_mutation,
    ),
    #(
      localization.is_localization_mutation_root(name),
      localization.process_mutation,
    ),
    #(
      metaobject_definitions.is_metaobject_definitions_mutation_root(name),
      metaobject_definitions.process_mutation,
    ),
    #(marketing.is_marketing_mutation_root(name), marketing.process_mutation),
    #(media.is_media_mutation_root(name), media.process_mutation),
    #(markets.is_markets_mutation_root(name), markets.process_mutation),
    #(
      admin_platform.is_admin_platform_mutation_root(name),
      admin_platform.process_mutation,
    ),
    #(
      online_store.is_online_store_mutation_root(name),
      online_store.process_mutation,
    ),
    #(privacy.is_privacy_mutation_root(name), privacy.process_mutation),
    #(
      shipping_fulfillment_priority_mutation_root(name),
      shipping_fulfillments.process_mutation,
    ),
    #(orders.is_orders_mutation_root(name), orders.process_mutation),
    #(customers.is_customer_mutation_root(name), customers.process_mutation),
    #(
      shipping_fulfillments.is_shipping_fulfillment_mutation_root(name),
      shipping_fulfillments.process_mutation,
    ),
  ])
}

fn non_store_publishable_domain_for(name: String) -> Option(String) {
  case customers.is_customer_mutation_root(name) {
    True -> Some("customers")
    False ->
      case orders.is_orders_mutation_root(name) {
        True -> Some("orders")
        False ->
          case products.is_products_mutation_root(name) {
            True -> Some("products")
            False ->
              case discounts.is_discount_mutation_root(name) {
                True -> Some("discounts")
                False ->
                  case
                    metaobject_definitions.is_metaobject_definitions_mutation_root(
                      name,
                    )
                  {
                    True -> Some("metaobjects")
                    False ->
                      case
                        metafield_definitions.is_metafield_definitions_mutation_root(
                          name,
                        )
                      {
                        True -> Some("metafields")
                        False ->
                          case
                            shipping_fulfillment_priority_mutation_root(name)
                            || shipping_fulfillments.is_shipping_fulfillment_mutation_root(
                              name,
                            )
                          {
                            True -> Some("shipping-fulfillments")
                            False ->
                              case payments.is_payments_mutation_root(name) {
                                True -> Some("payments")
                                False ->
                                  case
                                    store_properties.is_store_properties_mutation_root(
                                      name,
                                    )
                                  {
                                    True -> Some("store-properties")
                                    False ->
                                      case
                                        saved_searches.is_saved_search_mutation_root(
                                          name,
                                        )
                                      {
                                        True -> Some("saved-searches")
                                        False ->
                                          case
                                            webhooks.is_webhook_subscription_mutation_root(
                                              name,
                                            )
                                          {
                                            True -> Some("webhooks")
                                            False ->
                                              case
                                                apps.is_app_mutation_root(name)
                                              {
                                                True -> Some("apps")
                                                False ->
                                                  case
                                                    functions.is_function_mutation_root(
                                                      name,
                                                    )
                                                  {
                                                    True -> Some("functions")
                                                    False ->
                                                      case
                                                        gift_cards.is_gift_card_mutation_root(
                                                          name,
                                                        )
                                                      {
                                                        True ->
                                                          Some("gift-cards")
                                                        False ->
                                                          case
                                                            b2b.is_b2b_mutation_root(
                                                              name,
                                                            )
                                                          {
                                                            True -> Some("b2b")
                                                            False ->
                                                              case
                                                                segments.is_segment_mutation_root(
                                                                  name,
                                                                )
                                                              {
                                                                True ->
                                                                  Some(
                                                                    "segments",
                                                                  )
                                                                False ->
                                                                  case
                                                                    localization.is_localization_mutation_root(
                                                                      name,
                                                                    )
                                                                  {
                                                                    True ->
                                                                      Some(
                                                                        "localization",
                                                                      )
                                                                    False ->
                                                                      case
                                                                        marketing.is_marketing_mutation_root(
                                                                          name,
                                                                        )
                                                                      {
                                                                        True ->
                                                                          Some(
                                                                            "marketing",
                                                                          )
                                                                        False ->
                                                                          case
                                                                            media.is_media_mutation_root(
                                                                              name,
                                                                            )
                                                                          {
                                                                            True ->
                                                                              Some(
                                                                                "media",
                                                                              )
                                                                            False ->
                                                                              case
                                                                                markets.is_markets_mutation_root(
                                                                                  name,
                                                                                )
                                                                              {
                                                                                True ->
                                                                                  Some(
                                                                                    "markets",
                                                                                  )
                                                                                False ->
                                                                                  case
                                                                                    admin_platform.is_admin_platform_mutation_root(
                                                                                      name,
                                                                                    )
                                                                                  {
                                                                                    True ->
                                                                                      Some(
                                                                                        "admin-platform",
                                                                                      )
                                                                                    False ->
                                                                                      case
                                                                                        online_store.is_online_store_mutation_root(
                                                                                          name,
                                                                                        )
                                                                                      {
                                                                                        True ->
                                                                                          Some(
                                                                                            "online-store",
                                                                                          )
                                                                                        False ->
                                                                                          case
                                                                                            privacy.is_privacy_mutation_root(
                                                                                              name,
                                                                                            )
                                                                                          {
                                                                                            True ->
                                                                                              Some(
                                                                                                "privacy",
                                                                                              )
                                                                                            False ->
                                                                                              None
                                                                                          }
                                                                                      }
                                                                                  }
                                                                              }
                                                                          }
                                                                      }
                                                                  }
                                                              }
                                                          }
                                                      }
                                                  }
                                              }
                                          }
                                      }
                                  }
                              }
                          }
                      }
                  }
              }
          }
      }
  }
}

fn first_matching_handler(
  candidates: List(#(Bool, handler)),
) -> Option(handler) {
  case candidates {
    [] -> None
    [#(True, handler), ..] -> Some(handler)
    [_, ..rest] -> first_matching_handler(rest)
  }
}

fn publishable_mutation_requests_store_properties(
  name: String,
  query: String,
) -> Bool {
  case name {
    "publishablePublish" | "publishableUnpublish" ->
      string.contains(query, "publishedOnCurrentPublication")
      || string.contains(query, "availablePublicationsCount")
      || string.contains(query, " shop ")
      || string.contains(query, "shop {")
    _ -> False
  }
}

fn shipping_fulfillment_priority_mutation_root(name: String) -> Bool {
  case name {
    "fulfillmentEventCreate"
    | "fulfillmentOrderSubmitFulfillmentRequest"
    | "fulfillmentOrderAcceptFulfillmentRequest"
    | "fulfillmentOrderRejectFulfillmentRequest"
    | "fulfillmentOrderSubmitCancellationRequest"
    | "fulfillmentOrderAcceptCancellationRequest"
    | "fulfillmentOrderRejectCancellationRequest"
    | "fulfillmentOrderHold"
    | "fulfillmentOrderReleaseHold"
    | "fulfillmentOrderMove"
    | "fulfillmentOrderReschedule"
    | "fulfillmentOrderReportProgress"
    | "fulfillmentOrderOpen"
    | "fulfillmentOrderClose"
    | "fulfillmentOrderCancel"
    | "fulfillmentOrderSplit"
    | "fulfillmentOrdersSetFulfillmentDeadline"
    | "fulfillmentOrderMerge" -> True
    _ -> False
  }
}

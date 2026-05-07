import gleam/dict.{type Dict}
import gleam/int
import gleam/json
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/mutation_helpers.{type MutationOutcome}
import shopify_draft_proxy/proxy/orders
import shopify_draft_proxy/proxy/upstream_query.{empty_upstream_context}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types

pub fn orders_abandoned_checkout_empty_read_test() {
  let query =
    "
    query {
      abandonedCheckouts(first: 2, sortKey: CREATED_AT, reverse: true) {
        nodes {
          id
          name
          abandonedCheckoutUrl
          completedAt
          createdAt
          updatedAt
          totalPriceSet {
            shopMoney {
              amount
              currencyCode
            }
          }
        }
        edges {
          cursor
          node {
            id
          }
        }
        pageInfo {
          hasNextPage
          hasPreviousPage
          startCursor
          endCursor
        }
      }
      abandonedCheckoutsCount {
        count
        precision
      }
      abandonment(id: \"gid://shopify/Abandonment/0\") {
        id
      }
      abandonmentByAbandonedCheckoutId(
        abandonedCheckoutId: \"gid://shopify/AbandonedCheckout/0\"
      ) {
        id
      }
    }
  "
  let assert Ok(result) = orders.process(store.new(), query, dict.new())
  assert json.to_string(result)
    == "{\"data\":{\"abandonedCheckouts\":{\"nodes\":[],\"edges\":[],\"pageInfo\":{\"hasNextPage\":false,\"hasPreviousPage\":false,\"startCursor\":null,\"endCursor\":null}},\"abandonedCheckoutsCount\":{\"count\":0,\"precision\":\"EXACT\"},\"abandonment\":null,\"abandonmentByAbandonedCheckoutId\":null}}"
}

pub fn orders_draft_orders_catalog_count_and_search_warning_test() {
  let first_id = "gid://shopify/DraftOrder/101"
  let second_id = "gid://shopify/DraftOrder/102"
  let seeded_store =
    store.new()
    |> store.upsert_base_draft_orders([
      types.DraftOrderRecord(
        id: first_id,
        cursor: Some("cursor-101"),
        data: types.CapturedObject([
          #("id", types.CapturedString(first_id)),
          #("name", types.CapturedString("#D101")),
          #("email", types.CapturedString("first@example.test")),
          #("status", types.CapturedString("OPEN")),
          #("ready", types.CapturedBool(True)),
        ]),
      ),
      types.DraftOrderRecord(
        id: second_id,
        cursor: Some("cursor-102"),
        data: types.CapturedObject([
          #("id", types.CapturedString(second_id)),
          #("name", types.CapturedString("#D102")),
          #("email", types.CapturedString("second@example.test")),
          #("status", types.CapturedString("COMPLETED")),
          #("ready", types.CapturedBool(True)),
        ]),
      ),
    ])
  let query =
    "
    query {
      draftOrders(first: 1, query: \"email:first@example.test\") {
        edges {
          cursor
          node {
            id
            name
            email
            status
            ready
          }
        }
        pageInfo {
          hasNextPage
          hasPreviousPage
          startCursor
          endCursor
        }
      }
      draftOrdersCount(query: \"email:first@example.test\") {
        count
        precision
      }
    }
  "
  let assert Ok(result) = orders.process(seeded_store, query, dict.new())
  assert json.to_string(result)
    == "{\"data\":{\"draftOrders\":{\"edges\":[{\"cursor\":\"cursor-101\",\"node\":{\"id\":\"gid://shopify/DraftOrder/101\",\"name\":\"#D101\",\"email\":\"first@example.test\",\"status\":\"OPEN\",\"ready\":true}}],\"pageInfo\":{\"hasNextPage\":true,\"hasPreviousPage\":false,\"startCursor\":\"cursor-101\",\"endCursor\":\"cursor-101\"}},\"draftOrdersCount\":{\"count\":2,\"precision\":\"EXACT\"}},\"extensions\":{\"search\":[{\"path\":[\"draftOrders\"],\"query\":\"email:first@example.test\",\"parsed\":{\"field\":\"email\",\"match_all\":\"first@example.test\"},\"warnings\":[{\"field\":\"email\",\"message\":\"Invalid search field for this query.\",\"code\":\"invalid_field\"}]},{\"path\":[\"draftOrdersCount\"],\"query\":\"email:first@example.test\",\"parsed\":{\"field\":\"email\",\"match_all\":\"first@example.test\"},\"warnings\":[{\"field\":\"email\",\"message\":\"Invalid search field for this query.\",\"code\":\"invalid_field\"}]}]}}"
}

pub fn orders_order_detail_read_and_missing_order_null_test() {
  let order_id = "gid://shopify/Order/6832000000000"
  let seeded_store =
    store.new()
    |> store.upsert_base_orders([
      types.OrderRecord(
        id: order_id,
        cursor: None,
        data: types.CapturedObject([
          #("id", types.CapturedString(order_id)),
          #("name", types.CapturedString("#1001")),
          #("email", types.CapturedString("merchant@example.test")),
          #("displayFinancialStatus", types.CapturedString("PAID")),
          #("displayFulfillmentStatus", types.CapturedString("UNFULFILLED")),
          #(
            "currentTotalPriceSet",
            types.CapturedObject([
              #(
                "shopMoney",
                types.CapturedObject([
                  #("amount", types.CapturedString("42.50")),
                  #("currencyCode", types.CapturedString("CAD")),
                ]),
              ),
            ]),
          ),
        ]),
      ),
    ])
  let query =
    "
    query {
      order(id: \"gid://shopify/Order/6832000000000\") {
        id
        name
        email
        displayFinancialStatus
        displayFulfillmentStatus
        currentTotalPriceSet {
          shopMoney {
            amount
            currencyCode
          }
        }
      }
      missing: order(id: \"gid://shopify/Order/0\") {
        id
      }
    }
  "
  let assert Ok(result) = orders.process(seeded_store, query, dict.new())
  assert json.to_string(result)
    == "{\"data\":{\"order\":{\"id\":\"gid://shopify/Order/6832000000000\",\"name\":\"#1001\",\"email\":\"merchant@example.test\",\"displayFinancialStatus\":\"PAID\",\"displayFulfillmentStatus\":\"UNFULFILLED\",\"currentTotalPriceSet\":{\"shopMoney\":{\"amount\":\"42.50\",\"currencyCode\":\"CAD\"}}},\"missing\":null}}"
}

pub fn orders_catalog_count_and_page_info_test() {
  let first_id = "gid://shopify/Order/101"
  let second_id = "gid://shopify/Order/102"
  let seeded_store =
    store.new()
    |> store.upsert_base_orders([
      types.OrderRecord(
        id: first_id,
        cursor: Some("cursor-101"),
        data: types.CapturedObject([
          #("id", types.CapturedString(first_id)),
          #("name", types.CapturedString("#101")),
          #("displayFinancialStatus", types.CapturedString("PAID")),
          #("displayFulfillmentStatus", types.CapturedString("UNFULFILLED")),
          #(
            "tags",
            types.CapturedArray([
              types.CapturedString("merchant-realistic"),
              types.CapturedString("parity-probe"),
            ]),
          ),
        ]),
      ),
      types.OrderRecord(
        id: second_id,
        cursor: Some("cursor-102"),
        data: types.CapturedObject([
          #("id", types.CapturedString(second_id)),
          #("name", types.CapturedString("#102")),
          #("displayFinancialStatus", types.CapturedString("PENDING")),
          #("displayFulfillmentStatus", types.CapturedString("FULFILLED")),
          #("tags", types.CapturedArray([types.CapturedString("other")])),
        ]),
      ),
    ])
  let query =
    "
    query {
      orders(first: 1, sortKey: CREATED_AT, reverse: true) {
        edges {
          cursor
          node {
            id
            name
            displayFinancialStatus
            displayFulfillmentStatus
          }
        }
        pageInfo {
          hasNextPage
          hasPreviousPage
          startCursor
          endCursor
        }
      }
      ordersCount {
        count
        precision
      }
      tagged: orders(first: 1, query: \"tag:merchant-realistic\") {
        nodes {
          id
          name
        }
      }
      limitedTaggedCount: ordersCount(query: \"tag:merchant-realistic\", limit: 0) {
        count
        precision
      }
    }
  "
  let assert Ok(result) = orders.process(seeded_store, query, dict.new())
  assert json.to_string(result)
    == "{\"data\":{\"orders\":{\"edges\":[{\"cursor\":\"cursor-101\",\"node\":{\"id\":\"gid://shopify/Order/101\",\"name\":\"#101\",\"displayFinancialStatus\":\"PAID\",\"displayFulfillmentStatus\":\"UNFULFILLED\"}}],\"pageInfo\":{\"hasNextPage\":true,\"hasPreviousPage\":false,\"startCursor\":\"cursor-101\",\"endCursor\":\"cursor-101\"}},\"ordersCount\":{\"count\":2,\"precision\":\"EXACT\"},\"tagged\":{\"nodes\":[{\"id\":\"gid://shopify/Order/101\",\"name\":\"#101\"}]},\"limitedTaggedCount\":{\"count\":0,\"precision\":\"AT_LEAST\"}}}"
}

pub fn orders_abandonment_delivery_status_unknown_test() {
  let query =
    "
    mutation {
      abandonmentUpdateActivitiesDeliveryStatuses(
        abandonmentId: \"gid://shopify/Abandonment/0\"
        marketingActivityId: \"gid://shopify/MarketingActivity/0\"
        deliveryStatus: SENT
        deliveredAt: \"2026-04-27T00:00:00Z\"
        deliveryStatusChangeReason: \"HAR-300 safe unknown-id probe\"
      ) {
        abandonment {
          id
          emailState
          emailSentAt
        }
        userErrors {
          field
          message
          code
        }
      }
    }
  "
  let outcome =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      query,
      dict.new(),
      empty_upstream_context(),
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"abandonmentUpdateActivitiesDeliveryStatuses\":{\"abandonment\":null,\"userErrors\":[{\"field\":[\"abandonmentId\"],\"message\":\"abandonment_not_found\",\"code\":\"NOT_FOUND\"}]}}}"
  assert outcome.staged_resource_ids == []
  assert list.length(outcome.log_drafts) == 1
}

pub fn orders_abandonment_delivery_status_edge_cases_test() {
  let abandonment_id = "gid://shopify/Abandonment/101"
  let marketing_activity_id = "gid://shopify/MarketingActivity/201"
  let unknown_activity_id = "gid://shopify/MarketingActivity/999"
  let seeded_store =
    seeded_abandonment_delivery_store(
      abandonment_id,
      marketing_activity_id,
      "DELIVERED",
      Some("2026-04-27T00:00:00Z"),
    )

  let unknown_outcome =
    run_abandonment_delivery_status_mutation(
      seeded_store,
      abandonment_id,
      unknown_activity_id,
      "SENDING",
      Some("2026-04-27T00:00:00Z"),
    )
  assert json.to_string(unknown_outcome.data)
    == "{\"data\":{\"abandonmentUpdateActivitiesDeliveryStatuses\":{\"abandonment\":{\"id\":\"gid://shopify/Abandonment/101\",\"emailState\":\"DELIVERED\",\"emailSentAt\":\"2026-04-27T00:00:00Z\"},\"userErrors\":[{\"field\":[\"deliveryStatuses\",\"0\",\"marketingActivityId\"],\"message\":\"invalid\",\"code\":\"NOT_FOUND\"}]}}}"
  assert unknown_outcome.staged_resource_ids == []

  let backwards_outcome =
    run_abandonment_delivery_status_mutation(
      seeded_store,
      abandonment_id,
      marketing_activity_id,
      "SENDING",
      Some("2026-04-27T00:00:00Z"),
    )
  assert json.to_string(backwards_outcome.data)
    == "{\"data\":{\"abandonmentUpdateActivitiesDeliveryStatuses\":{\"abandonment\":{\"id\":\"gid://shopify/Abandonment/101\",\"emailState\":\"DELIVERED\",\"emailSentAt\":\"2026-04-27T00:00:00Z\"},\"userErrors\":[{\"field\":[\"deliveryStatuses\",\"0\",\"deliveryStatus\"],\"message\":\"invalid_transition\",\"code\":\"INVALID\"}]}}}"
  assert backwards_outcome.staged_resource_ids == []

  let same_status_outcome =
    run_abandonment_delivery_status_mutation(
      seeded_store,
      abandonment_id,
      marketing_activity_id,
      "DELIVERED",
      Some("2026-04-27T00:00:00Z"),
    )
  assert json.to_string(same_status_outcome.data)
    == "{\"data\":{\"abandonmentUpdateActivitiesDeliveryStatuses\":{\"abandonment\":{\"id\":\"gid://shopify/Abandonment/101\",\"emailState\":\"DELIVERED\",\"emailSentAt\":\"2026-04-27T00:00:00Z\"},\"userErrors\":[]}}}"
  assert same_status_outcome.staged_resource_ids == []
  assert json.to_string(read_abandonment(seeded_store, abandonment_id))
    == "{\"data\":{\"abandonment\":{\"id\":\"gid://shopify/Abandonment/101\",\"emailState\":\"DELIVERED\",\"emailSentAt\":\"2026-04-27T00:00:00Z\"}}}"
  assert json.to_string(read_abandonment(
      same_status_outcome.store,
      abandonment_id,
    ))
    == "{\"data\":{\"abandonment\":{\"id\":\"gid://shopify/Abandonment/101\",\"emailState\":\"DELIVERED\",\"emailSentAt\":\"2026-04-27T00:00:00Z\"}}}"
}

pub fn orders_abandonment_delivery_status_future_delivered_at_test() {
  let abandonment_id = "gid://shopify/Abandonment/101"
  let marketing_activity_id = "gid://shopify/MarketingActivity/201"
  let seeded_store =
    seeded_abandonment_delivery_store(
      abandonment_id,
      marketing_activity_id,
      "SENDING",
      None,
    )

  let future_outcome =
    run_abandonment_delivery_status_mutation(
      seeded_store,
      abandonment_id,
      marketing_activity_id,
      "DELIVERED",
      Some("2099-01-01T00:00:00Z"),
    )
  assert json.to_string(future_outcome.data)
    == "{\"data\":{\"abandonmentUpdateActivitiesDeliveryStatuses\":{\"abandonment\":{\"id\":\"gid://shopify/Abandonment/101\",\"emailState\":\"SENDING\",\"emailSentAt\":null},\"userErrors\":[{\"field\":[\"deliveryStatuses\",\"0\",\"deliveredAt\"],\"message\":\"invalid\",\"code\":\"INVALID\"}]}}}"
  assert future_outcome.staged_resource_ids == []
  assert json.to_string(read_abandonment(future_outcome.store, abandonment_id))
    == "{\"data\":{\"abandonment\":{\"id\":\"gid://shopify/Abandonment/101\",\"emailState\":\"SENDING\",\"emailSentAt\":null}}}"
}

pub fn orders_abandonment_delivery_status_forward_transition_test() {
  let abandonment_id = "gid://shopify/Abandonment/101"
  let marketing_activity_id = "gid://shopify/MarketingActivity/201"
  let seeded_store =
    seeded_abandonment_delivery_store(
      abandonment_id,
      marketing_activity_id,
      "SENDING",
      None,
    )

  let forward_outcome =
    run_abandonment_delivery_status_mutation(
      seeded_store,
      abandonment_id,
      marketing_activity_id,
      "DELIVERED",
      Some("2026-04-27T00:00:00Z"),
    )
  assert json.to_string(forward_outcome.data)
    == "{\"data\":{\"abandonmentUpdateActivitiesDeliveryStatuses\":{\"abandonment\":{\"id\":\"gid://shopify/Abandonment/101\",\"emailState\":\"DELIVERED\",\"emailSentAt\":\"2026-04-27T00:00:00Z\"},\"userErrors\":[]}}}"
  assert forward_outcome.staged_resource_ids == [abandonment_id]
  assert json.to_string(read_abandonment(forward_outcome.store, abandonment_id))
    == "{\"data\":{\"abandonment\":{\"id\":\"gid://shopify/Abandonment/101\",\"emailState\":\"DELIVERED\",\"emailSentAt\":\"2026-04-27T00:00:00Z\"}}}"
}

fn seeded_abandonment_delivery_store(
  abandonment_id: String,
  marketing_activity_id: String,
  delivery_status: String,
  delivered_at: Option(String),
) {
  let delivery_activity =
    types.AbandonmentDeliveryActivityRecord(
      marketing_activity_id: marketing_activity_id,
      delivery_status: delivery_status,
      delivered_at: delivered_at,
      delivery_status_change_reason: None,
    )
  store.new()
  |> store.upsert_base_abandonments([
    types.AbandonmentRecord(
      id: abandonment_id,
      abandoned_checkout_id: None,
      cursor: None,
      data: types.CapturedObject([
        #("id", types.CapturedString(abandonment_id)),
        #("emailState", types.CapturedString(delivery_status)),
        #("emailSentAt", option_to_captured_string(delivered_at)),
      ]),
      delivery_activities: dict.from_list([
        #(marketing_activity_id, delivery_activity),
      ]),
    ),
  ])
}

fn run_abandonment_delivery_status_mutation(
  seeded_store,
  abandonment_id: String,
  marketing_activity_id: String,
  delivery_status: String,
  delivered_at: Option(String),
) {
  let delivered_at_arg = case delivered_at {
    Some(value) -> ", deliveredAt: \"" <> value <> "\""
    None -> ""
  }
  let query =
    "mutation { abandonmentUpdateActivitiesDeliveryStatuses(abandonmentId: \""
    <> abandonment_id
    <> "\", marketingActivityId: \""
    <> marketing_activity_id
    <> "\", deliveryStatus: "
    <> delivery_status
    <> delivered_at_arg
    <> ") { abandonment { id emailState emailSentAt } userErrors { field message code } } }"
  orders.process_mutation(
    seeded_store,
    synthetic_identity.new(),
    "/admin/api/2025-01/graphql.json",
    query,
    dict.new(),
    empty_upstream_context(),
  )
}

fn option_to_captured_string(value: Option(String)) {
  case value {
    Some(value) -> types.CapturedString(value)
    None -> types.CapturedNull
  }
}

fn read_abandonment(seed_store, abandonment_id: String) {
  let query =
    "query { abandonment(id: \""
    <> abandonment_id
    <> "\") { id emailState emailSentAt } }"
  let assert Ok(result) = orders.process(seed_store, query, dict.new())
  result
}

pub fn orders_access_denied_guardrails_test() {
  let query =
    "
    mutation {
      orderCreateManualPayment(
        id: \"gid://shopify/Order/6830646264041\"
        amount: { amount: \"14.00\", currencyCode: CAD }
        paymentMethodName: \"HAR-120 manual payment\"
        processedAt: \"2026-04-22T23:29:02.002Z\"
      ) {
        order {
          id
        }
        userErrors {
          field
          message
        }
      }
      taxSummaryCreate(
        orderId: \"gid://shopify/Order/6830646296809\"
        startTime: \"2026-04-22T22:29:03.293Z\"
        endTime: \"2026-04-22T23:29:03.293Z\"
      ) {
        enqueuedOrders {
          id
        }
        userErrors {
          field
          message
          code
        }
      }
    }
  "
  let outcome =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      query,
      dict.new(),
      empty_upstream_context(),
    )
  assert json.to_string(outcome.data)
    == "{\"errors\":[{\"message\":\"Access denied for orderCreateManualPayment field. Required access: `write_orders` access scope. Also: The user must have mark_orders_as_paid permission. The API client must be installed on a Shopify Plus store to use the amount field.\",\"extensions\":{\"code\":\"ACCESS_DENIED\",\"documentation\":\"https://shopify.dev/api/usage/access-scopes\",\"requiredAccess\":\"`write_orders` access scope. Also: The user must have mark_orders_as_paid permission. The API client must be installed on a Shopify Plus store to use the amount field.\"},\"path\":[\"orderCreateManualPayment\"]},{\"message\":\"Access denied for taxSummaryCreate field. Required access: `write_taxes` access scope. Also: The caller must be a tax calculations app and the relevant feature must be on.\",\"extensions\":{\"code\":\"ACCESS_DENIED\",\"documentation\":\"https://shopify.dev/api/usage/access-scopes\",\"requiredAccess\":\"`write_taxes` access scope. Also: The caller must be a tax calculations app and the relevant feature must be on.\"},\"path\":[\"taxSummaryCreate\"]}],\"data\":{\"orderCreateManualPayment\":null,\"taxSummaryCreate\":null}}"
  assert outcome.staged_resource_ids == []
  assert list.length(outcome.log_drafts) == 2
}

pub fn orders_order_edit_begin_existing_order_payload_test() {
  let order_id = "gid://shopify/Order/6834565087465"
  let seeded =
    store.new()
    |> store.upsert_base_orders([
      types.OrderRecord(
        id: order_id,
        cursor: None,
        data: types.CapturedObject([
          #("id", types.CapturedString(order_id)),
          #("name", types.CapturedString("#1331")),
          #(
            "lineItems",
            types.CapturedObject([
              #(
                "nodes",
                types.CapturedArray([
                  types.CapturedObject([
                    #(
                      "id",
                      types.CapturedString(
                        "gid://shopify/LineItem/16209970561257",
                      ),
                    ),
                    #(
                      "title",
                      types.CapturedString("Custom installation service"),
                    ),
                    #("quantity", types.CapturedInt(2)),
                    #("currentQuantity", types.CapturedInt(2)),
                    #(
                      "sku",
                      types.CapturedString(
                        "hermes-custom-service-1777076856718",
                      ),
                    ),
                    #("variant", types.CapturedNull),
                    #(
                      "originalUnitPriceSet",
                      types.CapturedObject([
                        #(
                          "shopMoney",
                          types.CapturedObject([
                            #("amount", types.CapturedString("20.0")),
                            #("currencyCode", types.CapturedString("CAD")),
                          ]),
                        ),
                      ]),
                    ),
                  ]),
                ]),
              ),
            ]),
          ),
        ]),
      ),
    ])
  let mutation =
    "
    mutation OrderEditExistingWorkflowBegin($id: ID!) {
      orderEditBegin(id: $id) {
        calculatedOrder {
          id
          originalOrder {
            id
            name
          }
          lineItems(first: 10) {
            nodes {
              id
              title
              quantity
              currentQuantity
              sku
              variant {
                id
              }
              originalUnitPriceSet {
                shopMoney {
                  amount
                  currencyCode
                }
              }
            }
          }
          addedLineItems(first: 10) {
            nodes {
              id
              title
              quantity
              sku
              variant {
                id
              }
            }
          }
        }
        orderEditSession {
          id
        }
        userErrors {
          field
          message
          code
        }
      }
    }
  "
  let variables = dict.from_list([#("id", root_field.StringVal(order_id))])
  let outcome =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      mutation,
      variables,
      empty_upstream_context(),
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"orderEditBegin\":{\"calculatedOrder\":{\"id\":\"gid://shopify/CalculatedOrder/1\",\"originalOrder\":{\"id\":\"gid://shopify/Order/6834565087465\",\"name\":\"#1331\"},\"lineItems\":{\"nodes\":[{\"id\":\"gid://shopify/CalculatedLineItem/2\",\"title\":\"Custom installation service\",\"quantity\":2,\"currentQuantity\":2,\"sku\":\"hermes-custom-service-1777076856718\",\"variant\":null,\"originalUnitPriceSet\":{\"shopMoney\":{\"amount\":\"20.0\",\"currencyCode\":\"CAD\"}}}]},\"addedLineItems\":{\"nodes\":[]}},\"orderEditSession\":{\"id\":\"gid://shopify/OrderEditSession/1\"},\"userErrors\":[]}}}"
  assert outcome.staged_resource_ids == []
  assert outcome.log_drafts == []
}

pub fn orders_order_edit_add_variant_payload_test() {
  let product_id = "gid://shopify/Product/8397254426857"
  let variant_id = "gid://shopify/ProductVariant/46789254021353"
  let seeded =
    store.new()
    |> store.upsert_base_products([
      types.ProductRecord(
        id: product_id,
        legacy_resource_id: None,
        title: "VANS |AUTHENTIC | LO PRO | BURGANDY/WHITE",
        handle: "",
        status: "ACTIVE",
        vendor: None,
        product_type: None,
        tags: [],
        price_range_min: None,
        price_range_max: None,
        total_variants: None,
        has_only_default_variant: None,
        has_out_of_stock_variants: None,
        total_inventory: None,
        tracks_inventory: None,
        created_at: None,
        updated_at: None,
        published_at: None,
        description_html: "",
        online_store_preview_url: None,
        template_suffix: None,
        seo: types.ProductSeoRecord(title: None, description: None),
        category: None,
        publication_ids: [],
        contextual_pricing: None,
        cursor: None,
        combined_listing_role: None,
        combined_listing_parent_id: None,
        combined_listing_child_ids: [],
      ),
    ])
    |> store.upsert_base_product_variants([
      types.ProductVariantRecord(
        id: variant_id,
        product_id: product_id,
        title: "4 / burgandy",
        sku: Some("VN-01-burgandy-4"),
        barcode: None,
        price: Some("29.00"),
        compare_at_price: None,
        taxable: None,
        inventory_policy: None,
        inventory_quantity: None,
        selected_options: [],
        media_ids: [],
        inventory_item: None,
        contextual_pricing: None,
        cursor: None,
      ),
    ])
    |> store.upsert_base_orders([
      order_edit_test_order("gid://shopify/Order/7012", "PAID", None, [
        order_edit_test_session(
          "gid://shopify/Order/7012",
          "gid://shopify/CalculatedOrder/10",
        ),
      ]),
    ])
  let mutation =
    "
    mutation OrderEditExistingWorkflowAddVariantPayload(
      $id: ID!
      $variantId: ID!
      $quantity: Int!
      $locationId: ID
      $allowDuplicates: Boolean
    ) {
      orderEditAddVariant(
        id: $id
        variantId: $variantId
        quantity: $quantity
        locationId: $locationId
        allowDuplicates: $allowDuplicates
      ) {
        calculatedLineItem {
          id
          title
          quantity
          currentQuantity
          sku
          variant {
            id
          }
          originalUnitPriceSet {
            shopMoney {
              amount
              currencyCode
            }
            presentmentMoney {
              amount
              currencyCode
            }
          }
        }
        orderEditSession {
          id
        }
        userErrors {
          field
          message
          code
        }
      }
    }
  "
  let variables =
    dict.from_list([
      #("id", root_field.StringVal("gid://shopify/CalculatedOrder/10")),
      #("variantId", root_field.StringVal(variant_id)),
      #("quantity", root_field.IntVal(1)),
      #(
        "locationId",
        root_field.StringVal("gid://shopify/Location/68509171945"),
      ),
      #("allowDuplicates", root_field.BoolVal(False)),
    ])
  let outcome =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      mutation,
      variables,
      empty_upstream_context(),
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"orderEditAddVariant\":{\"calculatedLineItem\":{\"id\":\"gid://shopify/CalculatedLineItem/1\",\"title\":\"VANS |AUTHENTIC | LO PRO | BURGANDY/WHITE\",\"quantity\":1,\"currentQuantity\":1,\"sku\":\"VN-01-burgandy-4\",\"variant\":{\"id\":\"gid://shopify/ProductVariant/46789254021353\"},\"originalUnitPriceSet\":{\"shopMoney\":{\"amount\":\"29.0\",\"currencyCode\":\"CAD\"},\"presentmentMoney\":{\"amount\":\"29.0\",\"currencyCode\":\"CAD\"}}},\"orderEditSession\":{\"id\":\"gid://shopify/OrderEditSession/10\"},\"userErrors\":[]}}}"
  assert outcome.staged_resource_ids == []
  assert outcome.log_drafts == []
}

pub fn orders_order_edit_add_variant_invalid_variant_payload_test() {
  let mutation =
    "
    mutation OrderEditExistingWorkflowAddVariant(
      $id: ID!
      $variantId: ID!
      $quantity: Int!
      $locationId: ID
      $allowDuplicates: Boolean
    ) {
      orderEditAddVariant(
        id: $id
        variantId: $variantId
        quantity: $quantity
        locationId: $locationId
        allowDuplicates: $allowDuplicates
      ) {
        calculatedOrder {
          id
        }
        calculatedLineItem {
          title
        }
        orderEditSession {
          id
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  let variables =
    dict.from_list([
      #("id", root_field.StringVal("gid://shopify/CalculatedOrder/1")),
      #("variantId", root_field.StringVal("gid://shopify/ProductVariant/0")),
      #("quantity", root_field.IntVal(1)),
      #(
        "locationId",
        root_field.StringVal("gid://shopify/Location/68509171945"),
      ),
      #("allowDuplicates", root_field.BoolVal(False)),
    ])
  let session_store =
    store.new()
    |> store.upsert_base_orders([
      order_edit_test_order("gid://shopify/Order/7010", "PAID", None, [
        order_edit_test_session(
          "gid://shopify/Order/7010",
          "gid://shopify/CalculatedOrder/1",
        ),
      ]),
    ])
  let outcome =
    orders.process_mutation(
      session_store,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      mutation,
      variables,
      empty_upstream_context(),
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"orderEditAddVariant\":{\"calculatedOrder\":null,\"calculatedLineItem\":null,\"orderEditSession\":null,\"userErrors\":[{\"field\":[\"variantId\"],\"message\":\"can't convert Integer[0] to a positive Integer to use as an untrusted id\"}]}}}"
  assert outcome.staged_resource_ids == []
  assert outcome.log_drafts == []
}

pub fn orders_order_edit_add_custom_item_validation_payloads_test() {
  let calculated_order_id = "gid://shopify/CalculatedOrder/928"
  let order_id = "gid://shopify/Order/928"
  let seeded =
    store.new()
    |> store.upsert_base_orders([
      order_edit_test_order_with_currency(
        order_id,
        "PAID",
        None,
        [order_edit_test_session(order_id, calculated_order_id)],
        "CAD",
      ),
    ])
  let identity = synthetic_identity.new()
  let mutation = order_edit_add_custom_item_validation_mutation()

  let blank_title =
    orders.process_mutation(
      seeded,
      identity,
      "/admin/api/2026-04/graphql.json",
      mutation,
      order_edit_add_custom_item_variables(
        calculated_order_id,
        "",
        1,
        "-1.00",
        "CAD",
      ),
      empty_upstream_context(),
    )
  assert json.to_string(blank_title.data)
    == "{\"data\":{\"orderEditAddCustomItem\":{\"calculatedOrder\":null,\"calculatedLineItem\":null,\"orderEditSession\":null,\"userErrors\":[{\"field\":[\"title\"],\"message\":\"can't be blank\"},{\"field\":[\"price\",\"amount\"],\"message\":\"must be greater than or equal to 0\"}]}}}"
  assert blank_title.store == seeded
  assert blank_title.identity == identity

  let oversized_title =
    orders.process_mutation(
      seeded,
      identity,
      "/admin/api/2026-04/graphql.json",
      mutation,
      order_edit_add_custom_item_variables(
        calculated_order_id,
        string.repeat("x", times: 256),
        1,
        "1.00",
        "CAD",
      ),
      empty_upstream_context(),
    )
  assert json.to_string(oversized_title.data)
    == "{\"data\":{\"orderEditAddCustomItem\":{\"calculatedOrder\":null,\"calculatedLineItem\":null,\"orderEditSession\":null,\"userErrors\":[{\"field\":[\"title\"],\"message\":\"is too long (maximum is 255 characters)\"}]}}}"
  assert oversized_title.store == seeded
  assert oversized_title.identity == identity

  let zero_quantity =
    orders.process_mutation(
      seeded,
      identity,
      "/admin/api/2026-04/graphql.json",
      mutation,
      order_edit_add_custom_item_variables(
        calculated_order_id,
        "Validation item",
        0,
        "1.00",
        "CAD",
      ),
      empty_upstream_context(),
    )
  assert json.to_string(zero_quantity.data)
    == "{\"data\":{\"orderEditAddCustomItem\":{\"calculatedOrder\":null,\"calculatedLineItem\":null,\"orderEditSession\":null,\"userErrors\":[{\"field\":[\"quantity\"],\"message\":\"must be greater than 0\"}]}}}"
  assert zero_quantity.store == seeded
  assert zero_quantity.identity == identity

  let negative_price =
    orders.process_mutation(
      seeded,
      identity,
      "/admin/api/2026-04/graphql.json",
      mutation,
      order_edit_add_custom_item_variables(
        calculated_order_id,
        "Validation item",
        1,
        "-5.00",
        "CAD",
      ),
      empty_upstream_context(),
    )
  assert json.to_string(negative_price.data)
    == "{\"data\":{\"orderEditAddCustomItem\":{\"calculatedOrder\":null,\"calculatedLineItem\":null,\"orderEditSession\":null,\"userErrors\":[{\"field\":[\"price\",\"amount\"],\"message\":\"must be greater than or equal to 0\"}]}}}"
  assert negative_price.store == seeded
  assert negative_price.identity == identity

  let currency_mismatch =
    orders.process_mutation(
      seeded,
      identity,
      "/admin/api/2026-04/graphql.json",
      mutation,
      order_edit_add_custom_item_variables(
        calculated_order_id,
        "Validation item",
        1,
        "1.00",
        "USD",
      ),
      empty_upstream_context(),
    )
  assert json.to_string(currency_mismatch.data)
    == "{\"data\":{\"orderEditAddCustomItem\":{\"calculatedOrder\":null,\"calculatedLineItem\":null,\"orderEditSession\":null,\"userErrors\":[{\"field\":[\"price\",\"amount\"],\"message\":\"Currency must be CAD.\"}]}}}"
  assert currency_mismatch.store == seeded
  assert currency_mismatch.identity == identity
}

pub fn orders_order_edit_add_custom_item_uses_order_currency_when_price_currency_missing_test() {
  let calculated_order_id = "gid://shopify/CalculatedOrder/929"
  let order_id = "gid://shopify/Order/929"
  let seeded =
    store.new()
    |> store.upsert_base_orders([
      order_edit_test_order_with_currency(
        order_id,
        "PAID",
        None,
        [order_edit_test_session(order_id, calculated_order_id)],
        "EUR",
      ),
    ])
  let mutation =
    "
    mutation AddCustomItemWithoutCurrency($id: ID!, $price: MoneyInput!) {
      orderEditAddCustomItem(
        id: $id
        title: \"EUR custom item\"
        quantity: 2
        price: $price
      ) {
        calculatedLineItem {
          id
          title
          quantity
          originalUnitPriceSet {
            shopMoney {
              amount
              currencyCode
            }
          }
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  let variables =
    dict.from_list([
      #("id", root_field.StringVal(calculated_order_id)),
      #(
        "price",
        root_field.ObjectVal(
          dict.from_list([
            #("amount", root_field.StringVal("4.00")),
          ]),
        ),
      ),
    ])
  let outcome =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      mutation,
      variables,
      empty_upstream_context(),
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"orderEditAddCustomItem\":{\"calculatedLineItem\":{\"id\":\"gid://shopify/CalculatedLineItem/1\",\"title\":\"EUR custom item\",\"quantity\":2,\"originalUnitPriceSet\":{\"shopMoney\":{\"amount\":\"4.0\",\"currencyCode\":\"EUR\"}}},\"userErrors\":[]}}}"
}

pub fn orders_order_edit_add_custom_item_line_item_count_limit_test() {
  let calculated_order_id = "gid://shopify/CalculatedOrder/930"
  let order_id = "gid://shopify/Order/930"
  let session =
    order_edit_test_session_with_line_items(
      order_id,
      calculated_order_id,
      order_edit_test_line_items(250, "CAD"),
    )
  let seeded =
    store.new()
    |> store.upsert_base_orders([
      order_edit_test_order_with_currency(
        order_id,
        "PAID",
        None,
        [session],
        "CAD",
      ),
    ])
  let identity = synthetic_identity.new()
  let outcome =
    orders.process_mutation(
      seeded,
      identity,
      "/admin/api/2026-04/graphql.json",
      order_edit_add_custom_item_validation_mutation(),
      order_edit_add_custom_item_variables(
        calculated_order_id,
        "Limit item",
        1,
        "1.00",
        "CAD",
      ),
      empty_upstream_context(),
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"orderEditAddCustomItem\":{\"calculatedOrder\":null,\"calculatedLineItem\":null,\"orderEditSession\":null,\"userErrors\":[{\"field\":[],\"message\":\"line_items_limit_exceeded\"}]}}}"
  assert outcome.store == seeded
  assert outcome.identity == identity
}

pub fn orders_order_edit_add_custom_item_channel_policy_guard_test() {
  let calculated_order_id = "gid://shopify/CalculatedOrder/931"
  let order_id = "gid://shopify/Order/931"
  let seeded =
    store.new()
    |> store.upsert_base_orders([
      order_edit_test_order_with_custom_item_policy(
        order_id,
        "PAID",
        None,
        [order_edit_test_session(order_id, calculated_order_id)],
        "CAD",
        False,
      ),
    ])
  let identity = synthetic_identity.new()
  let outcome =
    orders.process_mutation(
      seeded,
      identity,
      "/admin/api/2026-04/graphql.json",
      order_edit_add_custom_item_validation_mutation(),
      order_edit_add_custom_item_variables(
        calculated_order_id,
        "Policy disallowed item",
        1,
        "1.00",
        "CAD",
      ),
      empty_upstream_context(),
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"orderEditAddCustomItem\":{\"calculatedOrder\":null,\"calculatedLineItem\":null,\"orderEditSession\":null,\"userErrors\":[{\"field\":[\"customItem\"],\"message\":\"not_supported\"}]}}}"
  assert outcome.store == seeded
  assert outcome.identity == identity
}

pub fn orders_order_edit_begin_user_error_payload_shapes_test() {
  let begin =
    "
    mutation OrderEditBeginUserErrors($id: ID!) {
      orderEditBegin(id: $id) {
        calculatedOrder {
          id
        }
        orderEditSession {
          id
        }
        userErrors {
          field
          message
          code
        }
      }
    }
  "

  let missing_outcome =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      begin,
      dict.from_list([
        #("id", root_field.StringVal("gid://shopify/Order/0")),
      ]),
      empty_upstream_context(),
    )
  assert json.to_string(missing_outcome.data)
    == "{\"data\":{\"orderEditBegin\":{\"calculatedOrder\":null,\"orderEditSession\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"The order does not exist.\",\"code\":\"INVALID\"}]}}}"

  let refunded_id = "gid://shopify/Order/7001"
  let refunded_store =
    store.new()
    |> store.upsert_base_orders([
      order_edit_test_order(refunded_id, "REFUNDED", None, []),
    ])
  let refunded_outcome =
    orders.process_mutation(
      refunded_store,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      begin,
      dict.from_list([#("id", root_field.StringVal(refunded_id))]),
      empty_upstream_context(),
    )
  assert json.to_string(refunded_outcome.data)
    == "{\"data\":{\"orderEditBegin\":{\"calculatedOrder\":null,\"orderEditSession\":null,\"userErrors\":[{\"field\":[\"base\"],\"message\":\"The order cannot be edited.\",\"code\":\"INVALID\"}]}}}"

  let cancel_id = "gid://shopify/Order/7002"
  let cancel_store =
    store.new()
    |> store.upsert_base_orders([
      order_edit_test_order(cancel_id, "PAID", None, []),
    ])
  let cancel =
    "
    mutation CancelForOrderEdit($orderId: ID!, $reason: OrderCancelReason!, $restock: Boolean!) {
      orderCancel(orderId: $orderId, reason: $reason, restock: $restock) {
        userErrors {
          field
          message
        }
      }
    }
  "
  let cancel_outcome =
    orders.process_mutation(
      cancel_store,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      cancel,
      dict.from_list([
        #("orderId", root_field.StringVal(cancel_id)),
        #("reason", root_field.StringVal("OTHER")),
        #("restock", root_field.BoolVal(True)),
      ]),
      empty_upstream_context(),
    )
  let canceled_begin_outcome =
    orders.process_mutation(
      cancel_outcome.store,
      cancel_outcome.identity,
      "/admin/api/2026-04/graphql.json",
      begin,
      dict.from_list([#("id", root_field.StringVal(cancel_id))]),
      empty_upstream_context(),
    )
  assert json.to_string(canceled_begin_outcome.data)
    == "{\"data\":{\"orderEditBegin\":{\"calculatedOrder\":null,\"orderEditSession\":null,\"userErrors\":[{\"field\":[\"base\"],\"message\":\"The order cannot be edited.\",\"code\":\"INVALID\"}]}}}"

  let existing_session_id = "gid://shopify/CalculatedOrder/99"
  let existing_session_order_id = "gid://shopify/Order/7003"
  let existing_session_store =
    store.new()
    |> store.upsert_base_orders([
      order_edit_test_order(existing_session_order_id, "PAID", None, [
        order_edit_test_session(existing_session_order_id, existing_session_id),
      ]),
    ])
  let existing_session_outcome =
    orders.process_mutation(
      existing_session_store,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      begin,
      dict.from_list([
        #("id", root_field.StringVal(existing_session_order_id)),
      ]),
      empty_upstream_context(),
    )
  assert json.to_string(existing_session_outcome.data)
    == "{\"data\":{\"orderEditBegin\":{\"calculatedOrder\":null,\"orderEditSession\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"An edit is already in progress for this order\",\"code\":\"INVALID\"}]}}}"
  assert existing_session_outcome.store == existing_session_store
}

pub fn orders_order_edit_unknown_resource_user_error_payload_shapes_test() {
  let add_variant =
    "
    mutation AddMissingVariant($id: ID!, $variantId: ID!, $quantity: Int!) {
      orderEditAddVariant(id: $id, variantId: $variantId, quantity: $quantity) {
        calculatedOrder {
          id
        }
        calculatedLineItem {
          id
        }
        orderEditSession {
          id
        }
        userErrors {
          field
          message
          code
        }
      }
    }
  "
  let add_variant_outcome =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      add_variant,
      dict.from_list([
        #("id", root_field.StringVal("gid://shopify/CalculatedOrder/1")),
        #("variantId", root_field.StringVal("gid://shopify/ProductVariant/404")),
        #("quantity", root_field.IntVal(1)),
      ]),
      empty_upstream_context(),
    )
  assert json.to_string(add_variant_outcome.data)
    == "{\"data\":{\"orderEditAddVariant\":{\"calculatedOrder\":null,\"calculatedLineItem\":null,\"orderEditSession\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"The calculated order does not exist.\",\"code\":\"INVALID\"}]}}}"

  let session_store =
    store.new()
    |> store.upsert_base_orders([
      order_edit_test_order("gid://shopify/Order/7011", "PAID", None, [
        order_edit_test_session(
          "gid://shopify/Order/7011",
          "gid://shopify/CalculatedOrder/1",
        ),
      ]),
    ])
  let missing_variant_outcome =
    orders.process_mutation(
      session_store,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      add_variant,
      dict.from_list([
        #("id", root_field.StringVal("gid://shopify/CalculatedOrder/1")),
        #("variantId", root_field.StringVal("gid://shopify/ProductVariant/404")),
        #("quantity", root_field.IntVal(1)),
      ]),
      empty_upstream_context(),
    )
  assert json.to_string(missing_variant_outcome.data)
    == "{\"data\":{\"orderEditAddVariant\":{\"calculatedOrder\":null,\"calculatedLineItem\":null,\"orderEditSession\":null,\"userErrors\":[{\"field\":[\"variantId\"],\"message\":\"Variant does not exist\",\"code\":\"INVALID\"}]}}}"

  let set_quantity =
    "
    mutation SetMissingLine($id: ID!, $lineItemId: ID!, $quantity: Int!) {
      orderEditSetQuantity(id: $id, lineItemId: $lineItemId, quantity: $quantity) {
        calculatedOrder {
          id
        }
        calculatedLineItem {
          id
        }
        orderEditSession {
          id
        }
        userErrors {
          field
          message
          code
        }
      }
    }
  "
  let set_quantity_outcome =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      set_quantity,
      dict.from_list([
        #("id", root_field.StringVal("gid://shopify/CalculatedOrder/1")),
        #(
          "lineItemId",
          root_field.StringVal("gid://shopify/CalculatedLineItem/404"),
        ),
        #("quantity", root_field.IntVal(1)),
      ]),
      empty_upstream_context(),
    )
  assert json.to_string(set_quantity_outcome.data)
    == "{\"data\":{\"orderEditSetQuantity\":{\"calculatedOrder\":null,\"calculatedLineItem\":null,\"orderEditSession\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"The calculated order does not exist.\",\"code\":\"INVALID\"}]}}}"

  let missing_line_outcome =
    orders.process_mutation(
      session_store,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      set_quantity,
      dict.from_list([
        #("id", root_field.StringVal("gid://shopify/CalculatedOrder/1")),
        #(
          "lineItemId",
          root_field.StringVal("gid://shopify/CalculatedLineItem/404"),
        ),
        #("quantity", root_field.IntVal(1)),
      ]),
      empty_upstream_context(),
    )
  assert json.to_string(missing_line_outcome.data)
    == "{\"data\":{\"orderEditSetQuantity\":{\"calculatedOrder\":null,\"calculatedLineItem\":null,\"orderEditSession\":null,\"userErrors\":[{\"field\":[\"lineItemId\"],\"message\":\"Line item does not exist\",\"code\":\"INVALID\"}]}}}"

  let commit =
    "
    mutation CommitMissingCalculatedOrder($id: ID!) {
      orderEditCommit(id: $id, notifyCustomer: false) {
        order {
          id
        }
        successMessages
        userErrors {
          field
          message
          code
        }
      }
    }
  "
  let commit_outcome =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      commit,
      dict.from_list([
        #("id", root_field.StringVal("gid://shopify/CalculatedOrder/404")),
      ]),
      empty_upstream_context(),
    )
  assert json.to_string(commit_outcome.data)
    == "{\"data\":{\"orderEditCommit\":{\"order\":null,\"successMessages\":[],\"userErrors\":[{\"field\":[\"id\"],\"message\":\"The calculated order does not exist.\",\"code\":\"INVALID\"}]}}}"
}

fn order_edit_test_order(
  id: String,
  display_financial_status: String,
  cancelled_at: Option(String),
  order_edit_sessions: List(types.CapturedJsonValue),
) -> types.OrderRecord {
  order_edit_test_order_with_optional_currency(
    id,
    display_financial_status,
    cancelled_at,
    order_edit_sessions,
    None,
    None,
  )
}

fn order_edit_test_order_with_currency(
  id: String,
  display_financial_status: String,
  cancelled_at: Option(String),
  order_edit_sessions: List(types.CapturedJsonValue),
  currency_code: String,
) -> types.OrderRecord {
  order_edit_test_order_with_optional_currency(
    id,
    display_financial_status,
    cancelled_at,
    order_edit_sessions,
    Some(currency_code),
    None,
  )
}

fn order_edit_test_order_with_custom_item_policy(
  id: String,
  display_financial_status: String,
  cancelled_at: Option(String),
  order_edit_sessions: List(types.CapturedJsonValue),
  currency_code: String,
  add_custom_item_allowed: Bool,
) -> types.OrderRecord {
  order_edit_test_order_with_optional_currency(
    id,
    display_financial_status,
    cancelled_at,
    order_edit_sessions,
    Some(currency_code),
    Some(add_custom_item_allowed),
  )
}

fn order_edit_test_order_with_optional_currency(
  id: String,
  display_financial_status: String,
  cancelled_at: Option(String),
  order_edit_sessions: List(types.CapturedJsonValue),
  currency_code: Option(String),
  add_custom_item_allowed: Option(Bool),
) -> types.OrderRecord {
  let currency_fields = case currency_code {
    Some(code) -> [
      #("currentTotalPriceSet", order_edit_test_money_set("1.00", code)),
    ]
    None -> []
  }
  let policy_fields = case add_custom_item_allowed {
    Some(allowed) -> [
      #("__draftProxyAddCustomItemAllowed", types.CapturedBool(allowed)),
    ]
    None -> []
  }
  types.OrderRecord(
    id: id,
    cursor: None,
    data: types.CapturedObject(list.append(
      list.append(
        [
          #("id", types.CapturedString(id)),
          #("name", types.CapturedString("#7001")),
          #(
            "displayFinancialStatus",
            types.CapturedString(display_financial_status),
          ),
          #("cancelledAt", case cancelled_at {
            Some(timestamp) -> types.CapturedString(timestamp)
            None -> types.CapturedNull
          }),
          #(
            "lineItems",
            types.CapturedObject([#("nodes", types.CapturedArray([]))]),
          ),
          #("orderEditSessions", types.CapturedArray(order_edit_sessions)),
        ],
        currency_fields,
      ),
      policy_fields,
    )),
  )
}

fn order_edit_test_session(
  order_id: String,
  calculated_order_id: String,
) -> types.CapturedJsonValue {
  order_edit_test_session_with_line_items(order_id, calculated_order_id, [])
}

fn order_edit_test_session_with_line_items(
  order_id: String,
  calculated_order_id: String,
  line_items: List(types.CapturedJsonValue),
) -> types.CapturedJsonValue {
  types.CapturedObject([
    #("id", types.CapturedString(calculated_order_id)),
    #("originalOrderId", types.CapturedString(order_id)),
    #(
      "lineItems",
      types.CapturedObject([#("nodes", types.CapturedArray(line_items))]),
    ),
    #(
      "addedLineItems",
      types.CapturedObject([#("nodes", types.CapturedArray([]))]),
    ),
    #("shippingLines", types.CapturedArray([])),
  ])
}

fn order_edit_test_money_set(
  amount: String,
  currency_code: String,
) -> types.CapturedJsonValue {
  types.CapturedObject([
    #(
      "shopMoney",
      types.CapturedObject([
        #("amount", types.CapturedString(amount)),
        #("currencyCode", types.CapturedString(currency_code)),
      ]),
    ),
  ])
}

fn order_edit_add_custom_item_validation_mutation() -> String {
  "
    mutation OrderEditAddCustomItemValidation(
      $id: ID!
      $title: String!
      $quantity: Int!
      $price: MoneyInput!
    ) {
      orderEditAddCustomItem(
        id: $id
        title: $title
        quantity: $quantity
        price: $price
      ) {
        calculatedOrder {
          id
        }
        calculatedLineItem {
          id
        }
        orderEditSession {
          id
        }
        userErrors {
          field
          message
        }
      }
    }
  "
}

fn order_edit_add_custom_item_variables(
  calculated_order_id: String,
  title: String,
  quantity: Int,
  amount: String,
  currency_code: String,
) -> Dict(String, root_field.ResolvedValue) {
  dict.from_list([
    #("id", root_field.StringVal(calculated_order_id)),
    #("title", root_field.StringVal(title)),
    #("quantity", root_field.IntVal(quantity)),
    #(
      "price",
      root_field.ObjectVal(
        dict.from_list([
          #("amount", root_field.StringVal(amount)),
          #("currencyCode", root_field.StringVal(currency_code)),
        ]),
      ),
    ),
  ])
}

fn order_edit_test_line_items(
  count: Int,
  currency_code: String,
) -> List(types.CapturedJsonValue) {
  order_edit_test_line_items_loop(count, currency_code, [])
}

fn order_edit_test_line_items_loop(
  remaining: Int,
  currency_code: String,
  acc: List(types.CapturedJsonValue),
) -> List(types.CapturedJsonValue) {
  case remaining <= 0 {
    True -> acc
    False ->
      order_edit_test_line_items_loop(
        remaining - 1,
        currency_code,
        list.append(acc, [
          order_edit_test_line_item(remaining, currency_code),
        ]),
      )
  }
}

fn order_edit_test_line_item(
  index: Int,
  currency_code: String,
) -> types.CapturedJsonValue {
  types.CapturedObject([
    #(
      "id",
      types.CapturedString(
        "gid://shopify/CalculatedLineItem/" <> int.to_string(index),
      ),
    ),
    #("title", types.CapturedString("Existing item")),
    #("quantity", types.CapturedInt(1)),
    #("currentQuantity", types.CapturedInt(1)),
    #("sku", types.CapturedNull),
    #("variant", types.CapturedNull),
    #("originalUnitPriceSet", order_edit_test_money_set("1.00", currency_code)),
  ])
}

pub fn orders_order_edit_set_quantity_payload_test() {
  let order_id = "gid://shopify/Order/6834565087465"
  let seeded =
    store.new()
    |> store.upsert_base_orders([
      types.OrderRecord(
        id: order_id,
        cursor: None,
        data: types.CapturedObject([
          #("id", types.CapturedString(order_id)),
          #("name", types.CapturedString("#1331")),
          #(
            "lineItems",
            types.CapturedObject([
              #(
                "nodes",
                types.CapturedArray([
                  types.CapturedObject([
                    #(
                      "id",
                      types.CapturedString(
                        "gid://shopify/LineItem/16210048319721",
                      ),
                    ),
                    #(
                      "title",
                      types.CapturedString(
                        "VANS |AUTHENTIC | LO PRO | BURGANDY/WHITE",
                      ),
                    ),
                    #("quantity", types.CapturedInt(1)),
                    #("currentQuantity", types.CapturedInt(1)),
                    #("sku", types.CapturedString("VN-01-burgandy-4")),
                    #(
                      "variant",
                      types.CapturedObject([
                        #(
                          "id",
                          types.CapturedString(
                            "gid://shopify/ProductVariant/46789254021353",
                          ),
                        ),
                      ]),
                    ),
                    #(
                      "originalUnitPriceSet",
                      types.CapturedObject([
                        #(
                          "shopMoney",
                          types.CapturedObject([
                            #("amount", types.CapturedString("29.0")),
                            #("currencyCode", types.CapturedString("CAD")),
                          ]),
                        ),
                      ]),
                    ),
                  ]),
                ]),
              ),
            ]),
          ),
          #(
            "orderEditSessions",
            types.CapturedArray([
              order_edit_test_session(
                order_id,
                "gid://shopify/CalculatedOrder/1",
              ),
            ]),
          ),
        ]),
      ),
    ])
  let mutation =
    "
    mutation OrderEditExistingWorkflowSetQuantityPayload(
      $id: ID!
      $lineItemId: ID!
      $quantity: Int!
      $restock: Boolean
    ) {
      orderEditSetQuantity(
        id: $id
        lineItemId: $lineItemId
        quantity: $quantity
        restock: $restock
      ) {
        calculatedLineItem {
          title
          quantity
          currentQuantity
          sku
          variant {
            id
          }
          originalUnitPriceSet {
            shopMoney {
              amount
              currencyCode
            }
          }
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  let variables =
    dict.from_list([
      #("id", root_field.StringVal("gid://shopify/CalculatedOrder/1")),
      #(
        "lineItemId",
        root_field.StringVal("gid://shopify/CalculatedLineItem/2"),
      ),
      #("quantity", root_field.IntVal(0)),
      #("restock", root_field.BoolVal(True)),
    ])
  let outcome =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      mutation,
      variables,
      empty_upstream_context(),
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"orderEditSetQuantity\":{\"calculatedLineItem\":{\"title\":\"VANS |AUTHENTIC | LO PRO | BURGANDY/WHITE\",\"quantity\":0,\"currentQuantity\":0,\"sku\":\"VN-01-burgandy-4\",\"variant\":{\"id\":\"gid://shopify/ProductVariant/46789254021353\"},\"originalUnitPriceSet\":{\"shopMoney\":{\"amount\":\"29.0\",\"currencyCode\":\"CAD\"}}},\"userErrors\":[]}}}"
  assert outcome.staged_resource_ids == []
  assert outcome.log_drafts == []
}

pub fn orders_order_edit_quantity_validation_rejects_without_mutation_test() {
  let order_id = "gid://shopify/Order/quantity-validation"
  let product_id = "gid://shopify/Product/quantity-validation"
  let variant_id = "gid://shopify/ProductVariant/quantity-validation"
  let calculated_order_id = "gid://shopify/CalculatedOrder/1"
  let calculated_line_item_id = "gid://shopify/CalculatedLineItem/2"
  let line_item =
    types.CapturedObject([
      #("id", types.CapturedString(calculated_line_item_id)),
      #("title", types.CapturedString("Existing widget")),
      #("quantity", types.CapturedInt(3)),
      #("currentQuantity", types.CapturedInt(3)),
      #("sku", types.CapturedString("EX-3")),
      #("variant", types.CapturedNull),
      #(
        "originalUnitPriceSet",
        types.CapturedObject([
          #(
            "shopMoney",
            types.CapturedObject([
              #("amount", types.CapturedString("10.0")),
              #("currencyCode", types.CapturedString("CAD")),
            ]),
          ),
          #(
            "presentmentMoney",
            types.CapturedObject([
              #("amount", types.CapturedString("10.0")),
              #("currencyCode", types.CapturedString("CAD")),
            ]),
          ),
        ]),
      ),
    ])
  let seeded =
    store.new()
    |> store.upsert_base_products([
      types.ProductRecord(
        id: product_id,
        legacy_resource_id: None,
        title: "Quantity validation product",
        handle: "",
        status: "ACTIVE",
        vendor: None,
        product_type: None,
        tags: [],
        price_range_min: None,
        price_range_max: None,
        total_variants: None,
        has_only_default_variant: None,
        has_out_of_stock_variants: None,
        total_inventory: None,
        tracks_inventory: None,
        created_at: None,
        updated_at: None,
        published_at: None,
        description_html: "",
        online_store_preview_url: None,
        template_suffix: None,
        seo: types.ProductSeoRecord(title: None, description: None),
        category: None,
        publication_ids: [],
        contextual_pricing: None,
        cursor: None,
        combined_listing_role: None,
        combined_listing_parent_id: None,
        combined_listing_child_ids: [],
      ),
    ])
    |> store.upsert_base_product_variants([
      types.ProductVariantRecord(
        id: variant_id,
        product_id: product_id,
        title: "Default Title",
        sku: Some("QV-1"),
        barcode: None,
        price: Some("12.00"),
        compare_at_price: None,
        taxable: None,
        inventory_policy: None,
        inventory_quantity: None,
        selected_options: [],
        media_ids: [],
        inventory_item: None,
        contextual_pricing: None,
        cursor: None,
      ),
    ])
    |> store.upsert_base_orders([
      order_edit_test_order(order_id, "PAID", None, [
        order_edit_test_session_with_line_items(order_id, calculated_order_id, [
          line_item,
        ]),
      ]),
    ])
  let identity = synthetic_identity.new()
  let set_quantity =
    "
    mutation SetQuantity($id: ID!, $lineItemId: ID!, $quantity: Int!) {
      orderEditSetQuantity(id: $id, lineItemId: $lineItemId, quantity: $quantity) {
        calculatedOrder {
          id
        }
        calculatedLineItem {
          id
        }
        orderEditSession {
          id
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  let set_negative =
    orders.process_mutation(
      seeded,
      identity,
      "/admin/api/2026-04/graphql.json",
      set_quantity,
      dict.from_list([
        #("id", root_field.StringVal(calculated_order_id)),
        #("lineItemId", root_field.StringVal(calculated_line_item_id)),
        #("quantity", root_field.IntVal(-1)),
      ]),
      empty_upstream_context(),
    )
  assert json.to_string(set_negative.data)
    == "{\"data\":{\"orderEditSetQuantity\":{\"calculatedOrder\":null,\"calculatedLineItem\":null,\"orderEditSession\":null,\"userErrors\":[{\"field\":[\"quantity\"],\"message\":\"must be greater than or equal to 0\"}]}}}"
  assert set_negative.store == seeded
  assert set_negative.identity == identity

  let add_variant =
    "
    mutation AddVariant($id: ID!, $variantId: ID!, $quantity: Int!) {
      orderEditAddVariant(id: $id, variantId: $variantId, quantity: $quantity) {
        calculatedOrder {
          lineItems(first: 10) {
            nodes {
              title
              quantity
            }
          }
          addedLineItems(first: 10) {
            nodes {
              title
              quantity
              variant {
                id
              }
            }
          }
        }
        calculatedLineItem {
          title
          quantity
          variant {
            id
          }
        }
        orderEditSession {
          id
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  let add_zero =
    orders.process_mutation(
      seeded,
      identity,
      "/admin/api/2026-04/graphql.json",
      add_variant,
      dict.from_list([
        #("id", root_field.StringVal(calculated_order_id)),
        #("variantId", root_field.StringVal(variant_id)),
        #("quantity", root_field.IntVal(0)),
      ]),
      empty_upstream_context(),
    )
  assert json.to_string(add_zero.data)
    == "{\"data\":{\"orderEditAddVariant\":{\"calculatedOrder\":null,\"calculatedLineItem\":null,\"orderEditSession\":null,\"userErrors\":[{\"field\":[\"quantity\"],\"message\":\"must be greater than 0\"}]}}}"
  assert add_zero.store == seeded
  assert add_zero.identity == identity

  let add_negative =
    orders.process_mutation(
      seeded,
      identity,
      "/admin/api/2026-04/graphql.json",
      add_variant,
      dict.from_list([
        #("id", root_field.StringVal(calculated_order_id)),
        #("variantId", root_field.StringVal(variant_id)),
        #("quantity", root_field.IntVal(-3)),
      ]),
      empty_upstream_context(),
    )
  assert json.to_string(add_negative.data)
    == "{\"data\":{\"orderEditAddVariant\":{\"calculatedOrder\":null,\"calculatedLineItem\":null,\"orderEditSession\":null,\"userErrors\":[{\"field\":[\"quantity\"],\"message\":\"must be greater than 0\"},{\"field\":[\"quantity\"],\"message\":\"must be greater than or equal to 0\"}]}}}"
  assert add_negative.store == seeded
  assert add_negative.identity == identity

  let add_valid_after_rejections =
    orders.process_mutation(
      add_negative.store,
      add_negative.identity,
      "/admin/api/2026-04/graphql.json",
      add_variant,
      dict.from_list([
        #("id", root_field.StringVal(calculated_order_id)),
        #("variantId", root_field.StringVal(variant_id)),
        #("quantity", root_field.IntVal(2)),
      ]),
      empty_upstream_context(),
    )
  assert json.to_string(add_valid_after_rejections.data)
    == "{\"data\":{\"orderEditAddVariant\":{\"calculatedOrder\":{\"lineItems\":{\"nodes\":[{\"title\":\"Existing widget\",\"quantity\":3},{\"title\":\"Quantity validation product\",\"quantity\":2}]},\"addedLineItems\":{\"nodes\":[{\"title\":\"Quantity validation product\",\"quantity\":2,\"variant\":{\"id\":\"gid://shopify/ProductVariant/quantity-validation\"}}]}},\"calculatedLineItem\":{\"title\":\"Quantity validation product\",\"quantity\":2,\"variant\":{\"id\":\"gid://shopify/ProductVariant/quantity-validation\"}},\"orderEditSession\":{\"id\":\"gid://shopify/OrderEditSession/1\"},\"userErrors\":[]}}}"
}

pub fn orders_order_edit_commit_updates_history_fulfillment_orders_and_totals_test() {
  let order_id = "gid://shopify/Order/order-edit-commit-state"
  let existing_line_item_id = "gid://shopify/LineItem/order-edit-existing"
  let product_id = "gid://shopify/Product/order-edit-added"
  let variant_id = "gid://shopify/ProductVariant/order-edit-added"
  let seeded =
    store.new()
    |> store.upsert_base_products([
      types.ProductRecord(
        id: product_id,
        legacy_resource_id: None,
        title: "Added variant",
        handle: "",
        status: "ACTIVE",
        vendor: None,
        product_type: None,
        tags: [],
        price_range_min: None,
        price_range_max: None,
        total_variants: None,
        has_only_default_variant: None,
        has_out_of_stock_variants: None,
        total_inventory: None,
        tracks_inventory: None,
        created_at: None,
        updated_at: None,
        published_at: None,
        description_html: "",
        online_store_preview_url: None,
        template_suffix: None,
        seo: types.ProductSeoRecord(title: None, description: None),
        category: None,
        publication_ids: [],
        contextual_pricing: None,
        cursor: None,
        combined_listing_role: None,
        combined_listing_parent_id: None,
        combined_listing_child_ids: [],
      ),
    ])
    |> store.upsert_base_product_variants([
      types.ProductVariantRecord(
        id: variant_id,
        product_id: product_id,
        title: "Default Title",
        sku: Some("ADD-1"),
        barcode: None,
        price: Some("5.00"),
        compare_at_price: None,
        taxable: None,
        inventory_policy: None,
        inventory_quantity: None,
        selected_options: [],
        media_ids: [],
        inventory_item: None,
        contextual_pricing: None,
        cursor: None,
      ),
    ])
    |> store.upsert_base_orders([
      types.OrderRecord(
        id: order_id,
        cursor: None,
        data: types.CapturedObject([
          #("id", types.CapturedString(order_id)),
          #("name", types.CapturedString("#8001")),
          #("currentSubtotalLineItemsQuantity", types.CapturedInt(3)),
          #(
            "currentSubtotalPriceSet",
            types.CapturedObject([
              #(
                "shopMoney",
                types.CapturedObject([
                  #("amount", types.CapturedString("30.0")),
                  #("currencyCode", types.CapturedString("CAD")),
                ]),
              ),
            ]),
          ),
          #(
            "currentTotalPriceSet",
            types.CapturedObject([
              #(
                "shopMoney",
                types.CapturedObject([
                  #("amount", types.CapturedString("33.0")),
                  #("currencyCode", types.CapturedString("CAD")),
                ]),
              ),
            ]),
          ),
          #(
            "currentTaxLines",
            types.CapturedArray([
              types.CapturedObject([
                #("title", types.CapturedString("GST")),
                #("rate", types.CapturedFloat(0.1)),
                #(
                  "priceSet",
                  types.CapturedObject([
                    #(
                      "shopMoney",
                      types.CapturedObject([
                        #("amount", types.CapturedString("3.0")),
                        #("currencyCode", types.CapturedString("CAD")),
                      ]),
                    ),
                  ]),
                ),
              ]),
            ]),
          ),
          #(
            "lineItems",
            types.CapturedObject([
              #(
                "nodes",
                types.CapturedArray([
                  types.CapturedObject([
                    #("id", types.CapturedString(existing_line_item_id)),
                    #("title", types.CapturedString("Existing widget")),
                    #("quantity", types.CapturedInt(3)),
                    #("currentQuantity", types.CapturedInt(3)),
                    #("sku", types.CapturedString("EX-3")),
                    #("variant", types.CapturedNull),
                    #(
                      "originalUnitPriceSet",
                      types.CapturedObject([
                        #(
                          "shopMoney",
                          types.CapturedObject([
                            #("amount", types.CapturedString("10.0")),
                            #("currencyCode", types.CapturedString("CAD")),
                          ]),
                        ),
                      ]),
                    ),
                  ]),
                ]),
              ),
            ]),
          ),
          #(
            "fulfillmentOrders",
            types.CapturedObject([
              #(
                "nodes",
                types.CapturedArray([
                  types.CapturedObject([
                    #(
                      "id",
                      types.CapturedString(
                        "gid://shopify/FulfillmentOrder/order-edit",
                      ),
                    ),
                    #("status", types.CapturedString("OPEN")),
                    #(
                      "lineItems",
                      types.CapturedObject([
                        #(
                          "nodes",
                          types.CapturedArray([
                            types.CapturedObject([
                              #(
                                "id",
                                types.CapturedString(
                                  "gid://shopify/FulfillmentOrderLineItem/existing",
                                ),
                              ),
                              #("totalQuantity", types.CapturedInt(3)),
                              #("remainingQuantity", types.CapturedInt(3)),
                              #(
                                "lineItem",
                                types.CapturedObject([
                                  #(
                                    "id",
                                    types.CapturedString(existing_line_item_id),
                                  ),
                                  #(
                                    "title",
                                    types.CapturedString("Existing widget"),
                                  ),
                                  #("quantity", types.CapturedInt(3)),
                                  #("fulfillableQuantity", types.CapturedInt(3)),
                                ]),
                              ),
                            ]),
                          ]),
                        ),
                      ]),
                    ),
                  ]),
                ]),
              ),
            ]),
          ),
          #(
            "orderEditSessions",
            types.CapturedArray([
              types.CapturedObject([
                #("id", types.CapturedString("gid://shopify/CalculatedOrder/1")),
                #("originalOrderId", types.CapturedString(order_id)),
                #(
                  "lineItems",
                  types.CapturedObject([
                    #(
                      "nodes",
                      types.CapturedArray([
                        types.CapturedObject([
                          #(
                            "id",
                            types.CapturedString(
                              "gid://shopify/CalculatedLineItem/2",
                            ),
                          ),
                          #("title", types.CapturedString("Existing widget")),
                          #("quantity", types.CapturedInt(3)),
                          #("currentQuantity", types.CapturedInt(3)),
                          #("sku", types.CapturedString("EX-3")),
                          #("variant", types.CapturedNull),
                          #(
                            "originalUnitPriceSet",
                            types.CapturedObject([
                              #(
                                "shopMoney",
                                types.CapturedObject([
                                  #("amount", types.CapturedString("10.0")),
                                  #("currencyCode", types.CapturedString("CAD")),
                                ]),
                              ),
                            ]),
                          ),
                        ]),
                      ]),
                    ),
                  ]),
                ),
                #(
                  "addedLineItems",
                  types.CapturedObject([#("nodes", types.CapturedArray([]))]),
                ),
                #("shippingLines", types.CapturedArray([])),
              ]),
            ]),
          ),
        ]),
      ),
    ])

  let add_variant =
    "
    mutation AddVariant($id: ID!, $variantId: ID!, $quantity: Int!) {
      orderEditAddVariant(id: $id, variantId: $variantId, quantity: $quantity) {
        calculatedLineItem { id }
        userErrors { field message }
      }
    }
  "
  let add_outcome =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      add_variant,
      dict.from_list([
        #("id", root_field.StringVal("gid://shopify/CalculatedOrder/1")),
        #("variantId", root_field.StringVal(variant_id)),
        #("quantity", root_field.IntVal(1)),
      ]),
      empty_upstream_context(),
    )
  let set_quantity =
    "
    mutation SetQuantity($id: ID!, $lineItemId: ID!, $quantity: Int!) {
      orderEditSetQuantity(id: $id, lineItemId: $lineItemId, quantity: $quantity) {
        calculatedLineItem { currentQuantity }
        userErrors { field message }
      }
    }
  "
  let set_outcome =
    orders.process_mutation(
      add_outcome.store,
      add_outcome.identity,
      "/admin/api/2026-04/graphql.json",
      set_quantity,
      dict.from_list([
        #("id", root_field.StringVal("gid://shopify/CalculatedOrder/1")),
        #(
          "lineItemId",
          root_field.StringVal("gid://shopify/CalculatedLineItem/2"),
        ),
        #("quantity", root_field.IntVal(1)),
      ]),
      empty_upstream_context(),
    )
  let commit =
    "
    mutation Commit($id: ID!, $notifyCustomer: Boolean, $staffNote: String) {
      orderEditCommit(id: $id, notifyCustomer: $notifyCustomer, staffNote: $staffNote) {
        order { id }
        userErrors { field message }
      }
    }
  "
  let commit_outcome =
    orders.process_mutation(
      set_outcome.store,
      set_outcome.identity,
      "/admin/api/2026-04/graphql.json",
      commit,
      dict.from_list([
        #("id", root_field.StringVal("gid://shopify/CalculatedOrder/1")),
        #("notifyCustomer", root_field.BoolVal(False)),
        #("staffNote", root_field.StringVal("locally committed edit")),
      ]),
      empty_upstream_context(),
    )
  let read =
    "
    query ReadEditedOrder($id: ID!) {
      order(id: $id) {
        currentSubtotalLineItemsQuantity
        currentSubtotalPriceSet { shopMoney { amount currencyCode } }
        currentTotalPriceSet { shopMoney { amount currencyCode } }
        currentTaxLines { title rate priceSet { shopMoney { amount currencyCode } } }
        editHistory(first: 10) {
          nodes {
            id
            committedAt
            staffMemberId
            notifyCustomer
            staffNote
            changes(first: 10) {
              nodes {
                __typename
                lineItem { id title quantity currentQuantity }
                originalQuantity
                newQuantity
              }
            }
          }
        }
        fulfillmentOrders(first: 10) {
          nodes {
            lineItems(first: 10) {
              nodes {
                totalQuantity
                remainingQuantity
                lineItem { id title quantity fulfillableQuantity }
              }
            }
          }
        }
      }
    }
  "
  let assert Ok(read_result) =
    orders.process(
      commit_outcome.store,
      read,
      dict.from_list([#("id", root_field.StringVal(order_id))]),
    )
  let body = json.to_string(read_result)
  assert string.contains(body, "\"currentSubtotalLineItemsQuantity\":2")
  assert string.contains(
    body,
    "\"currentSubtotalPriceSet\":{\"shopMoney\":{\"amount\":\"15.0\",\"currencyCode\":\"CAD\"}}",
  )
  assert string.contains(
    body,
    "\"currentTotalPriceSet\":{\"shopMoney\":{\"amount\":\"16.5\",\"currencyCode\":\"CAD\"}}",
  )
  assert string.contains(
    body,
    "\"currentTaxLines\":[{\"title\":\"GST\",\"rate\":0.1,\"priceSet\":{\"shopMoney\":{\"amount\":\"1.5\",\"currencyCode\":\"CAD\"}}}]",
  )
  assert string.contains(body, "\"notifyCustomer\":false")
  assert string.contains(
    body,
    "\"staffMemberId\":\"gid://shopify/StaffMember/1\"",
  )
  assert string.contains(body, "\"staffNote\":\"locally committed edit\"")
  assert string.contains(body, "\"__typename\":\"OrderEditChangeAddLineItem\"")
  assert string.contains(body, "\"__typename\":\"OrderEditChangeSetQuantity\"")
  assert string.contains(
    body,
    "\"totalQuantity\":1,\"remainingQuantity\":1,\"lineItem\":{\"id\":\"gid://shopify/LineItem/order-edit-existing\",\"title\":\"Existing widget\",\"quantity\":3,\"fulfillableQuantity\":1}",
  )
  assert string.contains(
    body,
    "\"totalQuantity\":1,\"remainingQuantity\":1,\"lineItem\":{\"id\":\"gid://shopify/CalculatedLineItem/1\",\"title\":\"Added variant\",\"quantity\":1,\"fulfillableQuantity\":1}",
  )
}

pub fn orders_draft_order_not_found_read_test() {
  let query =
    "
    query {
      missingDraftOrder: draftOrder(
        id: \"gid://shopify/DraftOrder/999999999999999\"
      ) {
        id
        name
        status
        invoiceUrl
      }
    }
  "
  let assert Ok(result) = orders.process(store.new(), query, dict.new())
  assert json.to_string(result) == "{\"data\":{\"missingDraftOrder\":null}}"
}

pub fn orders_draft_order_create_validation_guardrails_test() {
  let missing_input =
    "
    mutation InlineMissingDraftOrderInput {
      draftOrderCreate {
        draftOrder {
          id
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  let missing_outcome =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      missing_input,
      dict.new(),
      empty_upstream_context(),
    )
  assert json.to_string(missing_outcome.data)
    == "{\"errors\":[{\"message\":\"Field 'draftOrderCreate' is missing required arguments: input\",\"locations\":[{\"line\":3,\"column\":7}],\"path\":[\"mutation InlineMissingDraftOrderInput\",\"draftOrderCreate\"],\"extensions\":{\"code\":\"missingRequiredArguments\",\"className\":\"Field\",\"name\":\"draftOrderCreate\",\"arguments\":\"input\"}}]}"

  let null_input =
    "
    mutation InlineNullDraftOrderInput {
      draftOrderCreate(input: null) {
        draftOrder {
          id
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  let null_outcome =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      null_input,
      dict.new(),
      empty_upstream_context(),
    )
  assert json.to_string(null_outcome.data)
    == "{\"errors\":[{\"message\":\"Argument 'input' on Field 'draftOrderCreate' has an invalid value (null). Expected type 'DraftOrderInput!'.\",\"locations\":[{\"line\":3,\"column\":7}],\"path\":[\"mutation InlineNullDraftOrderInput\",\"draftOrderCreate\",\"input\"],\"extensions\":{\"code\":\"argumentLiteralsIncompatible\",\"typeName\":\"Field\",\"argumentName\":\"input\"}}]}"
}

pub fn orders_draft_order_create_payload_validation_matrix_test() {
  let mutation =
    "
    mutation {
      noLineItems: draftOrderCreate(input: { lineItems: [] }) {
        draftOrder { id }
        userErrors { field message }
      }
      unknownVariant: draftOrderCreate(input: {
        lineItems: [{ variantId: \"gid://shopify/ProductVariant/999999999999999999\", quantity: 1 }]
      }) {
        draftOrder { id }
        userErrors { field message }
      }
      customMissingTitle: draftOrderCreate(input: {
        lineItems: [{ quantity: 1, originalUnitPrice: \"10.00\" }]
      }) {
        draftOrder { id }
        userErrors { field message }
      }
      zeroQuantity: draftOrderCreate(input: {
        lineItems: [{ title: \"Zero quantity\", quantity: 0, originalUnitPrice: \"10.00\" }]
      }) {
        draftOrder { id }
        userErrors { field message }
      }
      paymentTerms: draftOrderCreate(input: {
        paymentTerms: { paymentSchedules: [{ dueAt: \"2026-05-22T12:00:00Z\" }] }
        lineItems: [{ title: \"Payment terms\", quantity: 1, originalUnitPrice: \"10.00\" }]
      }) {
        draftOrder { id }
        userErrors { field message }
      }
      negativePrice: draftOrderCreate(input: {
        lineItems: [{ title: \"Negative price\", quantity: 1, originalUnitPrice: \"-1.00\" }]
      }) {
        draftOrder { id }
        userErrors { field message }
      }
      pastReserve: draftOrderCreate(input: {
        reserveInventoryUntil: \"2020-01-01T00:00:00Z\"
        lineItems: [{ title: \"Past reserve\", quantity: 1, originalUnitPrice: \"10.00\" }]
      }) {
        draftOrder { id }
        userErrors { field message }
      }
      badEmail: draftOrderCreate(input: {
        email: \"not-an-email\"
        lineItems: [{ title: \"Bad email\", quantity: 1, originalUnitPrice: \"10.00\" }]
      }) {
        draftOrder { id }
        userErrors { field message }
      }
    }
  "
  let outcome =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      mutation,
      dict.new(),
      empty_upstream_context(),
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"noLineItems\":{\"draftOrder\":null,\"userErrors\":[{\"field\":null,\"message\":\"Add at least 1 product\"}]},\"unknownVariant\":{\"draftOrder\":null,\"userErrors\":[{\"field\":null,\"message\":\"Product with ID 999999999999999999 is no longer available.\"}]},\"customMissingTitle\":{\"draftOrder\":null,\"userErrors\":[{\"field\":null,\"message\":\"Merchandise title is empty.\"}]},\"zeroQuantity\":{\"draftOrder\":null,\"userErrors\":[{\"field\":[\"lineItems\",\"0\",\"quantity\"],\"message\":\"Quantity must be greater than or equal to 1\"}]},\"paymentTerms\":{\"draftOrder\":null,\"userErrors\":[{\"field\":null,\"message\":\"Payment terms template id can not be empty.\"}]},\"negativePrice\":{\"draftOrder\":null,\"userErrors\":[{\"field\":null,\"message\":\"Cannot send negative price for line_item\"}]},\"pastReserve\":{\"draftOrder\":null,\"userErrors\":[{\"field\":null,\"message\":\"Reserve until can't be in the past\"}]},\"badEmail\":{\"draftOrder\":null,\"userErrors\":[{\"field\":[\"email\"],\"message\":\"Email is invalid\"}]}}}"
  assert list.length(outcome.log_drafts) == 8
  assert store.list_effective_draft_orders(outcome.store) == []
}

pub fn orders_draft_order_create_user_error_code_test() {
  let mutation =
    "
    mutation {
      draftOrderCreate(input: { lineItems: [] }) {
        draftOrder { id }
        userErrors {
          field
          message
          code
        }
      }
    }
  "
  let outcome =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      mutation,
      dict.new(),
      empty_upstream_context(),
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"draftOrderCreate\":{\"draftOrder\":null,\"userErrors\":[{\"field\":null,\"message\":\"Add at least 1 product\",\"code\":\"INVALID\"}]}}}"
}

pub fn orders_draft_order_complete_validation_guardrails_test() {
  let missing_id =
    "
    mutation DraftOrderCompleteInlineMissingIdParity {
      draftOrderComplete(
        paymentGatewayId: null
        sourceName: \"hermes-cron-orders\"
      ) {
        draftOrder {
          id
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  let missing_outcome =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      missing_id,
      dict.new(),
      empty_upstream_context(),
    )
  assert json.to_string(missing_outcome.data)
    == "{\"errors\":[{\"message\":\"Field 'draftOrderComplete' is missing required arguments: id\",\"locations\":[{\"line\":3,\"column\":7}],\"path\":[\"mutation DraftOrderCompleteInlineMissingIdParity\",\"draftOrderComplete\"],\"extensions\":{\"code\":\"missingRequiredArguments\",\"className\":\"Field\",\"name\":\"draftOrderComplete\",\"arguments\":\"id\"}}]}"

  let null_id =
    "
    mutation DraftOrderCompleteInlineNullIdParity {
      draftOrderComplete(
        id: null
        paymentGatewayId: null
        sourceName: \"hermes-cron-orders\"
      ) {
        draftOrder {
          id
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  let null_outcome =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      null_id,
      dict.new(),
      empty_upstream_context(),
    )
  assert json.to_string(null_outcome.data)
    == "{\"errors\":[{\"message\":\"Argument 'id' on Field 'draftOrderComplete' has an invalid value (null). Expected type 'ID!'.\",\"locations\":[{\"line\":3,\"column\":7}],\"path\":[\"mutation DraftOrderCompleteInlineNullIdParity\",\"draftOrderComplete\",\"id\"],\"extensions\":{\"code\":\"argumentLiteralsIncompatible\",\"typeName\":\"Field\",\"argumentName\":\"id\"}}]}"
}

pub fn orders_fulfillment_validation_guardrails_test() {
  let cancel_missing_id =
    "
    mutation FulfillmentCancelInlineMissingId {
      fulfillmentCancel {
        fulfillment {
          id
          status
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  let cancel_missing_outcome =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      cancel_missing_id,
      dict.new(),
      empty_upstream_context(),
    )
  assert json.to_string(cancel_missing_outcome.data)
    == "{\"errors\":[{\"message\":\"Field 'fulfillmentCancel' is missing required arguments: id\",\"locations\":[{\"line\":3,\"column\":7}],\"path\":[\"mutation FulfillmentCancelInlineMissingId\",\"fulfillmentCancel\"],\"extensions\":{\"code\":\"missingRequiredArguments\",\"className\":\"Field\",\"name\":\"fulfillmentCancel\",\"arguments\":\"id\"}}]}"

  let cancel_null_id =
    "
    mutation FulfillmentCancelInlineNullId {
      fulfillmentCancel(id: null) {
        fulfillment {
          id
          status
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  let cancel_null_outcome =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      cancel_null_id,
      dict.new(),
      empty_upstream_context(),
    )
  assert json.to_string(cancel_null_outcome.data)
    == "{\"errors\":[{\"message\":\"Argument 'id' on Field 'fulfillmentCancel' has an invalid value (null). Expected type 'ID!'.\",\"locations\":[{\"line\":3,\"column\":7}],\"path\":[\"mutation FulfillmentCancelInlineNullId\",\"fulfillmentCancel\",\"id\"],\"extensions\":{\"code\":\"argumentLiteralsIncompatible\",\"typeName\":\"Field\",\"argumentName\":\"id\"}}]}"

  let tracking_missing_id =
    "
    mutation FulfillmentTrackingInfoUpdateInlineMissingId(
      $trackingInfoInput: FulfillmentTrackingInput!
      $notifyCustomer: Boolean
    ) {
      fulfillmentTrackingInfoUpdate(
        trackingInfoInput: $trackingInfoInput
        notifyCustomer: $notifyCustomer
      ) {
        fulfillment {
          id
          status
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  let tracking_variables =
    dict.from_list([
      #(
        "trackingInfoInput",
        root_field.ObjectVal(
          dict.from_list([
            #("number", root_field.StringVal("HERMES-TRACK-UPDATE")),
            #(
              "url",
              root_field.StringVal(
                "https://example.com/track/HERMES-TRACK-UPDATE",
              ),
            ),
            #("company", root_field.StringVal("Hermes")),
          ]),
        ),
      ),
      #("notifyCustomer", root_field.BoolVal(False)),
    ])
  let tracking_missing_outcome =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      tracking_missing_id,
      tracking_variables,
      empty_upstream_context(),
    )
  assert json.to_string(tracking_missing_outcome.data)
    == "{\"errors\":[{\"message\":\"Field 'fulfillmentTrackingInfoUpdate' is missing required arguments: fulfillmentId\",\"locations\":[{\"line\":6,\"column\":7}],\"path\":[\"mutation FulfillmentTrackingInfoUpdateInlineMissingId\",\"fulfillmentTrackingInfoUpdate\"],\"extensions\":{\"code\":\"missingRequiredArguments\",\"className\":\"Field\",\"name\":\"fulfillmentTrackingInfoUpdate\",\"arguments\":\"fulfillmentId\"}}]}"

  let tracking_null_id =
    "
    mutation FulfillmentTrackingInfoUpdateInlineNullId(
      $trackingInfoInput: FulfillmentTrackingInput!
      $notifyCustomer: Boolean
    ) {
      fulfillmentTrackingInfoUpdate(
        fulfillmentId: null
        trackingInfoInput: $trackingInfoInput
        notifyCustomer: $notifyCustomer
      ) {
        fulfillment {
          id
          status
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  let tracking_null_outcome =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      tracking_null_id,
      tracking_variables,
      empty_upstream_context(),
    )
  assert json.to_string(tracking_null_outcome.data)
    == "{\"errors\":[{\"message\":\"Argument 'fulfillmentId' on Field 'fulfillmentTrackingInfoUpdate' has an invalid value (null). Expected type 'ID!'.\",\"locations\":[{\"line\":6,\"column\":7}],\"path\":[\"mutation FulfillmentTrackingInfoUpdateInlineNullId\",\"fulfillmentTrackingInfoUpdate\",\"fulfillmentId\"],\"extensions\":{\"code\":\"argumentLiteralsIncompatible\",\"typeName\":\"Field\",\"argumentName\":\"fulfillmentId\"}}]}"
}

pub fn orders_fulfillment_state_preconditions_test() {
  let order_id = "gid://shopify/Order/fulfillment-state-preconditions"
  let fulfillment_id =
    "gid://shopify/Fulfillment/fulfillment-state-preconditions"

  let cancelled_store =
    order_store_with_fulfillment(
      order_id,
      fulfillment_id,
      "CANCELLED",
      "CANCELED",
    )

  let cancel_mutation =
    "
    mutation FulfillmentCancelStatePrecondition($id: ID!) {
      fulfillmentCancel(id: $id) {
        fulfillment {
          id
          status
          displayStatus
        }
        userErrors {
          field
          message
          code
        }
      }
    }
  "
  let cancel_variables =
    dict.from_list([#("id", root_field.StringVal(fulfillment_id))])
  let cancel_on_cancelled =
    orders.process_mutation(
      cancelled_store,
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      cancel_mutation,
      cancel_variables,
      empty_upstream_context(),
    )
  assert json.to_string(cancel_on_cancelled.data)
    == "{\"data\":{\"fulfillmentCancel\":{\"fulfillment\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"fulfillment_cannot_be_cancelled\",\"code\":\"INVALID\"}]}}}"
  assert cancel_on_cancelled.staged_resource_ids == []
  assert cancel_on_cancelled.log_drafts == []

  let tracking_mutation =
    "
    mutation FulfillmentTrackingStatePrecondition(
      $fulfillmentId: ID!
      $trackingInfoInput: FulfillmentTrackingInput!
    ) {
      fulfillmentTrackingInfoUpdate(
        fulfillmentId: $fulfillmentId
        trackingInfoInput: $trackingInfoInput
      ) {
        fulfillment {
          id
          status
          trackingInfo {
            number
            url
            company
          }
        }
        userErrors {
          field
          message
          code
        }
      }
    }
  "
  let tracking_variables =
    dict.from_list([
      #("fulfillmentId", root_field.StringVal(fulfillment_id)),
      #(
        "trackingInfoInput",
        root_field.ObjectVal(
          dict.from_list([
            #("number", root_field.StringVal("PRECONDITION-TRACK")),
            #(
              "url",
              root_field.StringVal(
                "https://example.com/track/PRECONDITION-TRACK",
              ),
            ),
            #("company", root_field.StringVal("Hermes")),
          ]),
        ),
      ),
    ])
  let tracking_on_cancelled =
    orders.process_mutation(
      cancelled_store,
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      tracking_mutation,
      tracking_variables,
      empty_upstream_context(),
    )
  assert json.to_string(tracking_on_cancelled.data)
    == "{\"data\":{\"fulfillmentTrackingInfoUpdate\":{\"fulfillment\":null,\"userErrors\":[{\"field\":[\"fulfillmentId\"],\"message\":\"fulfillment_is_cancelled\",\"code\":\"INVALID\"}]}}}"
  assert tracking_on_cancelled.staged_resource_ids == []
  assert tracking_on_cancelled.log_drafts == []

  let delivered_store =
    order_store_with_fulfillment(
      order_id,
      fulfillment_id,
      "SUCCESS",
      "DELIVERED",
    )
  let cancel_on_delivered =
    orders.process_mutation(
      delivered_store,
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      cancel_mutation,
      cancel_variables,
      empty_upstream_context(),
    )
  assert json.to_string(cancel_on_delivered.data)
    == "{\"data\":{\"fulfillmentCancel\":{\"fulfillment\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"fulfillment_already_delivered\",\"code\":\"INVALID\"}]}}}"
  assert cancel_on_delivered.staged_resource_ids == []
  assert cancel_on_delivered.log_drafts == []
}

fn order_store_with_fulfillment(
  order_id: String,
  fulfillment_id: String,
  status: String,
  display_status: String,
) -> store.Store {
  store.new()
  |> store.upsert_base_orders([
    types.OrderRecord(
      id: order_id,
      cursor: None,
      data: types.CapturedObject([
        #("id", types.CapturedString(order_id)),
        #("name", types.CapturedString("#FULFILLMENT-PRECONDITION")),
        #("displayFulfillmentStatus", types.CapturedString(display_status)),
        #(
          "fulfillments",
          types.CapturedArray([
            types.CapturedObject([
              #("id", types.CapturedString(fulfillment_id)),
              #("status", types.CapturedString(status)),
              #("displayStatus", types.CapturedString(display_status)),
              #("createdAt", types.CapturedString("2026-04-25T00:06:31Z")),
              #("updatedAt", types.CapturedString("2026-04-25T00:06:31Z")),
              #("trackingInfo", types.CapturedArray([])),
            ]),
          ]),
        ),
      ]),
    ),
  ])
}

pub fn orders_fulfillment_cancel_tracking_read_after_write_test() {
  let order_id = "gid://shopify/Order/6834528944361"
  let fulfillment_id = "gid://shopify/Fulfillment/6189151518953"
  let seeded =
    store.new()
    |> store.upsert_base_orders([
      types.OrderRecord(
        id: order_id,
        cursor: None,
        data: types.CapturedObject([
          #("id", types.CapturedString(order_id)),
          #("name", types.CapturedString("#1330")),
          #(
            "displayFulfillmentStatus",
            types.CapturedString("PARTIALLY_FULFILLED"),
          ),
          #(
            "fulfillments",
            types.CapturedArray([
              types.CapturedObject([
                #("id", types.CapturedString(fulfillment_id)),
                #("status", types.CapturedString("SUCCESS")),
                #("displayStatus", types.CapturedString("FULFILLED")),
                #("createdAt", types.CapturedString("2026-04-25T00:06:31Z")),
                #("updatedAt", types.CapturedString("2026-04-25T00:06:31Z")),
                #("trackingInfo", types.CapturedArray([])),
              ]),
            ]),
          ),
        ]),
      ),
    ])
  let tracking_mutation =
    "
    mutation FulfillmentTrackingInfoUpdateParityPlan(
      $fulfillmentId: ID!
      $trackingInfoInput: FulfillmentTrackingInput!
      $notifyCustomer: Boolean
    ) {
      fulfillmentTrackingInfoUpdate(
        fulfillmentId: $fulfillmentId
        trackingInfoInput: $trackingInfoInput
        notifyCustomer: $notifyCustomer
      ) {
        fulfillment {
          id
          status
          trackingInfo {
            number
            url
            company
          }
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  let tracking_variables =
    dict.from_list([
      #("fulfillmentId", root_field.StringVal(fulfillment_id)),
      #("notifyCustomer", root_field.BoolVal(False)),
      #(
        "trackingInfoInput",
        root_field.ObjectVal(
          dict.from_list([
            #("number", root_field.StringVal("HERMES-UPDATE-20260425000631")),
            #(
              "url",
              root_field.StringVal(
                "https://example.com/track/HERMES-UPDATE-20260425000631",
              ),
            ),
            #("company", root_field.StringVal("Hermes")),
          ]),
        ),
      ),
    ])
  let tracking_outcome =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      tracking_mutation,
      tracking_variables,
      empty_upstream_context(),
    )
  assert json.to_string(tracking_outcome.data)
    == "{\"data\":{\"fulfillmentTrackingInfoUpdate\":{\"fulfillment\":{\"id\":\"gid://shopify/Fulfillment/6189151518953\",\"status\":\"SUCCESS\",\"trackingInfo\":[{\"number\":\"HERMES-UPDATE-20260425000631\",\"url\":\"https://example.com/track/HERMES-UPDATE-20260425000631\",\"company\":\"Hermes\"}]},\"userErrors\":[]}}}"
  assert tracking_outcome.staged_resource_ids == [order_id]
  assert list.length(tracking_outcome.log_drafts) == 1

  let cancel_mutation =
    "
    mutation FulfillmentCancelParityPlan($id: ID!) {
      fulfillmentCancel(id: $id) {
        fulfillment {
          id
          status
          displayStatus
          trackingInfo {
            number
            url
            company
          }
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  let cancel_variables =
    dict.from_list([#("id", root_field.StringVal(fulfillment_id))])
  let cancel_outcome =
    orders.process_mutation(
      tracking_outcome.store,
      tracking_outcome.identity,
      "/admin/api/2025-01/graphql.json",
      cancel_mutation,
      cancel_variables,
      empty_upstream_context(),
    )
  assert json.to_string(cancel_outcome.data)
    == "{\"data\":{\"fulfillmentCancel\":{\"fulfillment\":{\"id\":\"gid://shopify/Fulfillment/6189151518953\",\"status\":\"CANCELLED\",\"displayStatus\":\"CANCELED\",\"trackingInfo\":[{\"number\":\"HERMES-UPDATE-20260425000631\",\"url\":\"https://example.com/track/HERMES-UPDATE-20260425000631\",\"company\":\"Hermes\"}]},\"userErrors\":[]}}}"

  let read_query =
    "
    query OrderFulfillmentLifecycleRead($id: ID!) {
      order(id: $id) {
        id
        displayFulfillmentStatus
        fulfillments(first: 5) {
          id
          status
          displayStatus
          trackingInfo {
            number
            url
            company
          }
        }
      }
    }
  "
  let read_variables = dict.from_list([#("id", root_field.StringVal(order_id))])
  let assert Ok(read) =
    orders.process(cancel_outcome.store, read_query, read_variables)
  assert json.to_string(read)
    == "{\"data\":{\"order\":{\"id\":\"gid://shopify/Order/6834528944361\",\"displayFulfillmentStatus\":\"UNFULFILLED\",\"fulfillments\":[{\"id\":\"gid://shopify/Fulfillment/6189151518953\",\"status\":\"CANCELLED\",\"displayStatus\":\"CANCELED\",\"trackingInfo\":[{\"number\":\"HERMES-UPDATE-20260425000631\",\"url\":\"https://example.com/track/HERMES-UPDATE-20260425000631\",\"company\":\"Hermes\"}]}]}}}"
}

pub fn orders_fulfillment_create_invalid_id_guardrail_test() {
  let mutation =
    "
    mutation FulfillmentCreateInvalidIdParity($fulfillment: FulfillmentInput!, $message: String) {
      fulfillmentCreate(fulfillment: $fulfillment, message: $message) {
        fulfillment {
          id
          status
          trackingInfo(first: 5) {
            number
            url
            company
          }
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  let variables =
    dict.from_list([
      #(
        "fulfillment",
        root_field.ObjectVal(
          dict.from_list([
            #("notifyCustomer", root_field.BoolVal(False)),
            #(
              "trackingInfo",
              root_field.ObjectVal(
                dict.from_list([
                  #("number", root_field.StringVal("HERMES-PROBE")),
                  #(
                    "url",
                    root_field.StringVal(
                      "https://example.com/track/HERMES-PROBE",
                    ),
                  ),
                  #("company", root_field.StringVal("Hermes")),
                ]),
              ),
            ),
            #(
              "lineItemsByFulfillmentOrder",
              root_field.ListVal([
                root_field.ObjectVal(
                  dict.from_list([
                    #(
                      "fulfillmentOrderId",
                      root_field.StringVal("gid://shopify/FulfillmentOrder/0"),
                    ),
                  ]),
                ),
              ]),
            ),
          ]),
        ),
      ),
      #("message", root_field.StringVal("hermes fulfillment probe")),
    ])
  let outcome =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      mutation,
      variables,
      empty_upstream_context(),
    )
  assert json.to_string(outcome.data)
    == "{\"errors\":[{\"message\":\"invalid id\",\"extensions\":{\"code\":\"RESOURCE_NOT_FOUND\"},\"path\":[\"fulfillmentCreate\"]}],\"data\":{\"fulfillmentCreate\":null}}"
}

pub fn orders_fulfillment_create_precondition_validation_test() {
  let cancelled_order_id = "gid://shopify/Order/fulfillment-create-cancelled"
  let cancelled_fulfillment_order_id =
    "gid://shopify/FulfillmentOrder/fulfillment-create-cancelled"
  let cancelled_line_item_id =
    "gid://shopify/FulfillmentOrderLineItem/fulfillment-create-cancelled"
  let closed_order_id = "gid://shopify/Order/fulfillment-create-closed"
  let closed_fulfillment_order_id =
    "gid://shopify/FulfillmentOrder/fulfillment-create-closed"
  let closed_line_item_id =
    "gid://shopify/FulfillmentOrderLineItem/fulfillment-create-closed"
  let progress_order_id = "gid://shopify/Order/fulfillment-create-progress"
  let progress_fulfillment_order_id =
    "gid://shopify/FulfillmentOrder/fulfillment-create-progress"
  let progress_line_item_id =
    "gid://shopify/FulfillmentOrderLineItem/fulfillment-create-progress"
  let over_order_id = "gid://shopify/Order/fulfillment-create-over"
  let over_fulfillment_order_id =
    "gid://shopify/FulfillmentOrder/fulfillment-create-over"
  let over_line_item_id =
    "gid://shopify/FulfillmentOrderLineItem/fulfillment-create-over"
  let seeded =
    store.new()
    |> store.upsert_base_orders([
      fulfillment_create_precondition_order(
        cancelled_order_id,
        cancelled_fulfillment_order_id,
        cancelled_line_item_id,
        "CLOSED",
        0,
        Some("2026-05-06T00:00:00Z"),
      ),
      fulfillment_create_precondition_order(
        closed_order_id,
        closed_fulfillment_order_id,
        closed_line_item_id,
        "CLOSED",
        1,
        None,
      ),
      fulfillment_create_precondition_order(
        progress_order_id,
        progress_fulfillment_order_id,
        progress_line_item_id,
        "IN_PROGRESS",
        1,
        None,
      ),
      fulfillment_create_precondition_order(
        over_order_id,
        over_fulfillment_order_id,
        over_line_item_id,
        "OPEN",
        1,
        None,
      ),
    ])
  let mutation = fulfillment_create_precondition_mutation()

  let cancelled =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      mutation,
      fulfillment_create_precondition_variables(
        cancelled_fulfillment_order_id,
        cancelled_line_item_id,
        1,
      ),
      empty_upstream_context(),
    )
  assert json.to_string(cancelled.data)
    == "{\"data\":{\"fulfillmentCreate\":{\"fulfillment\":null,\"userErrors\":[{\"field\":[\"fulfillment\"],\"message\":\"Fulfillment order fulfillment-create-cancelled has an unfulfillable status= closed.\"}]}}}"
  assert cancelled.staged_resource_ids == []
  assert cancelled.log_drafts == []

  let closed =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      mutation,
      fulfillment_create_precondition_variables(
        closed_fulfillment_order_id,
        closed_line_item_id,
        1,
      ),
      empty_upstream_context(),
    )
  assert json.to_string(closed.data)
    == "{\"data\":{\"fulfillmentCreate\":{\"fulfillment\":null,\"userErrors\":[{\"field\":[\"fulfillment\"],\"message\":\"Fulfillment order fulfillment-create-closed has an unfulfillable status= closed.\"}]}}}"
  assert closed.staged_resource_ids == []
  assert closed.log_drafts == []

  let in_progress =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      mutation,
      fulfillment_create_precondition_variables(
        progress_fulfillment_order_id,
        progress_line_item_id,
        1,
      ),
      empty_upstream_context(),
    )
  assert json.to_string(in_progress.data)
    == "{\"data\":{\"fulfillmentCreate\":{\"fulfillment\":{\"id\":\"gid://shopify/Fulfillment/1\"},\"userErrors\":[]}}}"
  assert in_progress.staged_resource_ids == [progress_order_id]
  assert list.length(in_progress.log_drafts) == 1

  let over_fulfill =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      mutation,
      fulfillment_create_precondition_variables(
        over_fulfillment_order_id,
        over_line_item_id,
        2,
      ),
      empty_upstream_context(),
    )
  assert json.to_string(over_fulfill.data)
    == "{\"data\":{\"fulfillmentCreate\":{\"fulfillment\":null,\"userErrors\":[{\"field\":[\"fulfillment\"],\"message\":\"Invalid fulfillment order line item quantity requested.\"}]}}}"
  assert over_fulfill.staged_resource_ids == []
  assert over_fulfill.log_drafts == []
}

fn fulfillment_create_precondition_mutation() -> String {
  "
    mutation FulfillmentCreatePreconditions($fulfillment: FulfillmentInput!) {
      fulfillmentCreate(fulfillment: $fulfillment) {
        fulfillment {
          id
        }
        userErrors {
          field
          message
        }
      }
    }
  "
}

fn fulfillment_create_precondition_variables(
  fulfillment_order_id: String,
  fulfillment_order_line_item_id: String,
  quantity: Int,
) -> Dict(String, root_field.ResolvedValue) {
  dict.from_list([
    #(
      "fulfillment",
      root_field.ObjectVal(
        dict.from_list([
          #("notifyCustomer", root_field.BoolVal(False)),
          #(
            "lineItemsByFulfillmentOrder",
            root_field.ListVal([
              root_field.ObjectVal(
                dict.from_list([
                  #(
                    "fulfillmentOrderId",
                    root_field.StringVal(fulfillment_order_id),
                  ),
                  #(
                    "fulfillmentOrderLineItems",
                    root_field.ListVal([
                      root_field.ObjectVal(
                        dict.from_list([
                          #(
                            "id",
                            root_field.StringVal(fulfillment_order_line_item_id),
                          ),
                          #("quantity", root_field.IntVal(quantity)),
                        ]),
                      ),
                    ]),
                  ),
                ]),
              ),
            ]),
          ),
        ]),
      ),
    ),
  ])
}

fn fulfillment_create_precondition_order(
  order_id: String,
  fulfillment_order_id: String,
  fulfillment_order_line_item_id: String,
  status: String,
  remaining_quantity: Int,
  cancelled_at: Option(String),
) -> types.OrderRecord {
  types.OrderRecord(
    id: order_id,
    cursor: None,
    data: types.CapturedObject([
      #("id", types.CapturedString(order_id)),
      #("name", types.CapturedString("#FULFILL-PRECONDITION")),
      #("cancelledAt", case cancelled_at {
        Some(value) -> types.CapturedString(value)
        None -> types.CapturedNull
      }),
      #("displayFulfillmentStatus", types.CapturedString("UNFULFILLED")),
      #("fulfillments", types.CapturedArray([])),
      #(
        "fulfillmentOrders",
        types.CapturedArray([
          types.CapturedObject([
            #("id", types.CapturedString(fulfillment_order_id)),
            #("status", types.CapturedString(status)),
            #("requestStatus", types.CapturedString("UNSUBMITTED")),
            #(
              "lineItems",
              types.CapturedArray([
                types.CapturedObject([
                  #("id", types.CapturedString(fulfillment_order_line_item_id)),
                  #(
                    "lineItemId",
                    types.CapturedString(
                      "gid://shopify/LineItem/" <> string.lowercase(status),
                    ),
                  ),
                  #("title", types.CapturedString("Precondition item")),
                  #("totalQuantity", types.CapturedInt(remaining_quantity)),
                  #("remainingQuantity", types.CapturedInt(remaining_quantity)),
                ]),
              ]),
            ),
          ]),
        ]),
      ),
    ]),
  )
}

pub fn orders_fulfillment_create_event_and_detail_read_test() {
  let order_id = "gid://shopify/Order/fulfillment-create"
  let fulfillment_order_id = "gid://shopify/FulfillmentOrder/fulfillment-create"
  let fulfillment_order_line_item_id =
    "gid://shopify/FulfillmentOrderLineItem/fulfillment-create"
  let unrequested_fulfillment_order_line_item_id =
    "gid://shopify/FulfillmentOrderLineItem/fulfillment-create-unrequested"
  let second_fulfillment_order_id =
    "gid://shopify/FulfillmentOrder/fulfillment-create-second"
  let line_item_id = "gid://shopify/LineItem/fulfillment-create"
  let seeded =
    store.new()
    |> store.upsert_base_orders([
      types.OrderRecord(
        id: order_id,
        cursor: None,
        data: types.CapturedObject([
          #("id", types.CapturedString(order_id)),
          #("name", types.CapturedString("#FULFILL-CREATE")),
          #("displayFulfillmentStatus", types.CapturedString("UNFULFILLED")),
          #("fulfillments", types.CapturedArray([])),
          #(
            "fulfillmentOrders",
            types.CapturedArray([
              types.CapturedObject([
                #("id", types.CapturedString(fulfillment_order_id)),
                #("status", types.CapturedString("OPEN")),
                #("requestStatus", types.CapturedString("UNSUBMITTED")),
                #(
                  "lineItems",
                  types.CapturedArray([
                    types.CapturedObject([
                      #(
                        "id",
                        types.CapturedString(fulfillment_order_line_item_id),
                      ),
                      #(
                        "lineItem",
                        types.CapturedObject([
                          #("id", types.CapturedString(line_item_id)),
                          #("title", types.CapturedString("Fulfillment item")),
                        ]),
                      ),
                      #("totalQuantity", types.CapturedInt(2)),
                      #("remainingQuantity", types.CapturedInt(2)),
                    ]),
                    types.CapturedObject([
                      #(
                        "id",
                        types.CapturedString(
                          unrequested_fulfillment_order_line_item_id,
                        ),
                      ),
                      #(
                        "lineItem",
                        types.CapturedObject([
                          #(
                            "id",
                            types.CapturedString(
                              "gid://shopify/LineItem/fulfillment-create-unrequested",
                            ),
                          ),
                          #("title", types.CapturedString("Unrequested item")),
                        ]),
                      ),
                      #("totalQuantity", types.CapturedInt(5)),
                      #("remainingQuantity", types.CapturedInt(5)),
                    ]),
                  ]),
                ),
              ]),
              types.CapturedObject([
                #("id", types.CapturedString(second_fulfillment_order_id)),
                #("status", types.CapturedString("OPEN")),
                #("requestStatus", types.CapturedString("UNSUBMITTED")),
                #(
                  "lineItems",
                  types.CapturedArray([
                    types.CapturedObject([
                      #(
                        "id",
                        types.CapturedString(
                          "gid://shopify/FulfillmentOrderLineItem/fulfillment-create-second",
                        ),
                      ),
                      #(
                        "lineItemId",
                        types.CapturedString(
                          "gid://shopify/LineItem/fulfillment-create-second",
                        ),
                      ),
                      #("title", types.CapturedString("Second item")),
                      #("totalQuantity", types.CapturedInt(1)),
                      #("remainingQuantity", types.CapturedInt(1)),
                    ]),
                  ]),
                ),
              ]),
            ]),
          ),
        ]),
      ),
    ])
  let create_mutation =
    "
    mutation FulfillmentCreate($fulfillment: FulfillmentInput!, $message: String) {
      fulfillmentCreate(fulfillment: $fulfillment, message: $message) {
        fulfillment {
          id
          status
          displayStatus
          trackingInfo(first: 1) {
            number
            url
            company
          }
          fulfillmentLineItems(first: 5) {
            nodes {
              id
              quantity
              lineItem {
                id
                title
              }
            }
          }
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  let create_variables =
    dict.from_list([
      #(
        "fulfillment",
        root_field.ObjectVal(
          dict.from_list([
            #("notifyCustomer", root_field.BoolVal(False)),
            #(
              "trackingInfo",
              root_field.ObjectVal(
                dict.from_list([
                  #("number", root_field.StringVal("HAR159-CREATE")),
                  #(
                    "url",
                    root_field.StringVal(
                      "https://example.com/track/HAR159-CREATE",
                    ),
                  ),
                  #("company", root_field.StringVal("Hermes")),
                ]),
              ),
            ),
            #(
              "lineItemsByFulfillmentOrder",
              root_field.ListVal([
                root_field.ObjectVal(
                  dict.from_list([
                    #(
                      "fulfillmentOrderId",
                      root_field.StringVal(fulfillment_order_id),
                    ),
                    #(
                      "fulfillmentOrderLineItems",
                      root_field.ListVal([
                        root_field.ObjectVal(
                          dict.from_list([
                            #(
                              "id",
                              root_field.StringVal(
                                fulfillment_order_line_item_id,
                              ),
                            ),
                            #("quantity", root_field.IntVal(1)),
                          ]),
                        ),
                      ]),
                    ),
                  ]),
                ),
              ]),
            ),
          ]),
        ),
      ),
      #("message", root_field.StringVal("HAR-159 create")),
    ])
  let create_outcome =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      create_mutation,
      create_variables,
      empty_upstream_context(),
    )
  assert json.to_string(create_outcome.data)
    == "{\"data\":{\"fulfillmentCreate\":{\"fulfillment\":{\"id\":\"gid://shopify/Fulfillment/1\",\"status\":\"SUCCESS\",\"displayStatus\":\"FULFILLED\",\"trackingInfo\":[{\"number\":\"HAR159-CREATE\",\"url\":\"https://example.com/track/HAR159-CREATE\",\"company\":\"Hermes\"}],\"fulfillmentLineItems\":{\"nodes\":[{\"id\":\"gid://shopify/FulfillmentLineItem/2\",\"quantity\":1,\"lineItem\":{\"id\":\"gid://shopify/LineItem/fulfillment-create\",\"title\":\"Fulfillment item\"}}]}},\"userErrors\":[]}}}"

  let event_mutation =
    "
    mutation FulfillmentEventCreate($fulfillmentEvent: FulfillmentEventInput!) {
      fulfillmentEventCreate(fulfillmentEvent: $fulfillmentEvent) {
          fulfillmentEvent {
            id
            status
            message
            happenedAt
            estimatedDeliveryAt
            city
            province
            country
            zip
            address1
            latitude
            longitude
          }
        userErrors {
          field
          message
        }
      }
    }
  "
  let event_variables =
    dict.from_list([
      #(
        "fulfillmentEvent",
        root_field.ObjectVal(
          dict.from_list([
            #(
              "fulfillmentId",
              root_field.StringVal("gid://shopify/Fulfillment/1"),
            ),
            #("status", root_field.StringVal("IN_TRANSIT")),
            #("message", root_field.StringVal("HAR-159 package scanned")),
            #("happenedAt", root_field.StringVal("2026-04-25T22:25:00Z")),
            #(
              "estimatedDeliveryAt",
              root_field.StringVal("2026-04-27T18:00:00Z"),
            ),
            #("city", root_field.StringVal("Toronto")),
            #("province", root_field.StringVal("Ontario")),
            #("country", root_field.StringVal("Canada")),
            #("zip", root_field.StringVal("M5H 2M9")),
            #("address1", root_field.StringVal("123 Queen St W")),
            #("latitude", root_field.FloatVal(43.6532)),
            #("longitude", root_field.FloatVal(-79.3832)),
          ]),
        ),
      ),
    ])
  let event_outcome =
    orders.process_mutation(
      create_outcome.store,
      create_outcome.identity,
      "/admin/api/2026-04/graphql.json",
      event_mutation,
      event_variables,
      empty_upstream_context(),
    )
  assert json.to_string(event_outcome.data)
    == "{\"data\":{\"fulfillmentEventCreate\":{\"fulfillmentEvent\":{\"id\":\"gid://shopify/FulfillmentEvent/3\",\"status\":\"IN_TRANSIT\",\"message\":\"HAR-159 package scanned\",\"happenedAt\":\"2026-04-25T22:25:00Z\",\"estimatedDeliveryAt\":\"2026-04-27T18:00:00Z\",\"city\":\"Toronto\",\"province\":\"Ontario\",\"country\":\"Canada\",\"zip\":\"M5H 2M9\",\"address1\":\"123 Queen St W\",\"latitude\":43.6532,\"longitude\":-79.3832},\"userErrors\":[]}}}"

  let detail_query =
    "
    query FulfillmentDetail($orderId: ID!, $fulfillmentId: ID!) {
      fulfillment(id: $fulfillmentId) {
        id
        displayStatus
        estimatedDeliveryAt
        inTransitAt
        events(first: 5) {
          nodes {
            id
            status
            message
            happenedAt
            estimatedDeliveryAt
            city
            province
            country
            zip
            address1
            latitude
            longitude
          }
        }
      }
      order(id: $orderId) {
        id
        displayFulfillmentStatus
        fulfillments(first: 5) {
          id
          displayStatus
          events(first: 5) {
            nodes {
              id
              status
            }
          }
        }
        fulfillmentOrders(first: 5) {
          nodes {
            id
            status
            lineItems(first: 5) {
              nodes {
                id
                remainingQuantity
              }
            }
          }
        }
      }
    }
  "
  let detail_variables =
    dict.from_list([
      #("orderId", root_field.StringVal(order_id)),
      #("fulfillmentId", root_field.StringVal("gid://shopify/Fulfillment/1")),
    ])
  let assert Ok(detail) =
    orders.process(event_outcome.store, detail_query, detail_variables)
  assert json.to_string(detail)
    == "{\"data\":{\"fulfillment\":{\"id\":\"gid://shopify/Fulfillment/1\",\"displayStatus\":\"IN_TRANSIT\",\"estimatedDeliveryAt\":\"2026-04-27T18:00:00Z\",\"inTransitAt\":\"2026-04-25T22:25:00Z\",\"events\":{\"nodes\":[{\"id\":\"gid://shopify/FulfillmentEvent/3\",\"status\":\"IN_TRANSIT\",\"message\":\"HAR-159 package scanned\",\"happenedAt\":\"2026-04-25T22:25:00Z\",\"estimatedDeliveryAt\":\"2026-04-27T18:00:00Z\",\"city\":\"Toronto\",\"province\":\"Ontario\",\"country\":\"Canada\",\"zip\":\"M5H 2M9\",\"address1\":\"123 Queen St W\",\"latitude\":43.6532,\"longitude\":-79.3832}]}},\"order\":{\"id\":\"gid://shopify/Order/fulfillment-create\",\"displayFulfillmentStatus\":\"PARTIALLY_FULFILLED\",\"fulfillments\":[{\"id\":\"gid://shopify/Fulfillment/1\",\"displayStatus\":\"IN_TRANSIT\",\"events\":{\"nodes\":[{\"id\":\"gid://shopify/FulfillmentEvent/3\",\"status\":\"IN_TRANSIT\"}]}}],\"fulfillmentOrders\":{\"nodes\":[{\"id\":\"gid://shopify/FulfillmentOrder/fulfillment-create\",\"status\":\"CLOSED\",\"lineItems\":{\"nodes\":[{\"id\":\"gid://shopify/FulfillmentOrderLineItem/fulfillment-create\",\"remainingQuantity\":1},{\"id\":\"gid://shopify/FulfillmentOrderLineItem/fulfillment-create-unrequested\",\"remainingQuantity\":5}]}},{\"id\":\"gid://shopify/FulfillmentOrder/fulfillment-create-second\",\"status\":\"OPEN\",\"lineItems\":{\"nodes\":[{\"id\":\"gid://shopify/FulfillmentOrderLineItem/fulfillment-create-second\",\"remainingQuantity\":1}]}}]}}}}"
}

pub fn orders_fulfillment_order_hold_release_read_after_write_test() {
  let order_id = "gid://shopify/Order/fulfillment-order-hold"
  let fulfillment_order_id = "gid://shopify/FulfillmentOrder/hold-release"
  let fulfillment_order_line_item_id =
    "gid://shopify/FulfillmentOrderLineItem/hold-release"
  let line_item_id = "gid://shopify/LineItem/fulfillment-order-lifecycle"
  let seeded =
    fulfillment_order_lifecycle_store(
      order_id,
      fulfillment_order_id,
      fulfillment_order_line_item_id,
      line_item_id,
    )
  let hold_mutation =
    "
    mutation Hold($id: ID!, $fulfillmentHold: FulfillmentOrderHoldInput!) {
      fulfillmentOrderHold(id: $id, fulfillmentHold: $fulfillmentHold) {
        fulfillmentHold {
          handle
          reason
          reasonNotes
          heldByRequestingApp
        }
        fulfillmentOrder {
          id
          status
          requestStatus
          supportedActions {
            action
          }
          fulfillmentHolds {
            handle
            reason
            reasonNotes
            heldByRequestingApp
          }
          lineItems(first: 5) {
            nodes {
              totalQuantity
              remainingQuantity
              lineItem {
                id
                title
              }
            }
          }
        }
        remainingFulfillmentOrder {
          status
          lineItems(first: 5) {
            nodes {
              totalQuantity
              remainingQuantity
              lineItem {
                id
              }
            }
          }
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  let hold_variables =
    dict.from_list([
      #("id", root_field.StringVal(fulfillment_order_id)),
      #(
        "fulfillmentHold",
        root_field.ObjectVal(
          dict.from_list([
            #("reason", root_field.StringVal("OTHER")),
            #("reasonNotes", root_field.StringVal("Local lifecycle hold")),
            #("handle", root_field.StringVal("local-lifecycle-hold")),
            #("notifyMerchant", root_field.BoolVal(False)),
            #(
              "fulfillmentOrderLineItems",
              root_field.ListVal([
                root_field.ObjectVal(
                  dict.from_list([
                    #(
                      "id",
                      root_field.StringVal(fulfillment_order_line_item_id),
                    ),
                    #("quantity", root_field.IntVal(1)),
                  ]),
                ),
              ]),
            ),
          ]),
        ),
      ),
    ])
  let hold_outcome =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      hold_mutation,
      hold_variables,
      empty_upstream_context(),
    )
  assert json.to_string(hold_outcome.data)
    == "{\"data\":{\"fulfillmentOrderHold\":{\"fulfillmentHold\":{\"handle\":\"local-lifecycle-hold\",\"reason\":\"OTHER\",\"reasonNotes\":\"Local lifecycle hold\",\"heldByRequestingApp\":true},\"fulfillmentOrder\":{\"id\":\"gid://shopify/FulfillmentOrder/hold-release\",\"status\":\"ON_HOLD\",\"requestStatus\":\"UNSUBMITTED\",\"supportedActions\":[{\"action\":\"RELEASE_HOLD\"},{\"action\":\"HOLD\"},{\"action\":\"MOVE\"}],\"fulfillmentHolds\":[{\"handle\":\"local-lifecycle-hold\",\"reason\":\"OTHER\",\"reasonNotes\":\"Local lifecycle hold\",\"heldByRequestingApp\":true}],\"lineItems\":{\"nodes\":[{\"totalQuantity\":1,\"remainingQuantity\":1,\"lineItem\":{\"id\":\"gid://shopify/LineItem/fulfillment-order-lifecycle\",\"title\":\"Fulfillment order lifecycle item\"}}]}},\"remainingFulfillmentOrder\":{\"status\":\"OPEN\",\"lineItems\":{\"nodes\":[{\"totalQuantity\":1,\"remainingQuantity\":1,\"lineItem\":{\"id\":\"gid://shopify/LineItem/fulfillment-order-lifecycle\"}}]}},\"userErrors\":[]}}}"

  let held_read_query =
    "
    query HeldReads($id: ID!, $first: Int!) {
      order(id: $id) {
        fulfillmentOrders(first: $first) {
          nodes {
            status
            fulfillmentHolds {
              handle
            }
          }
        }
      }
      manualHoldsFulfillmentOrders(first: $first) {
        nodes {
          id
          status
          fulfillmentHolds {
            handle
          }
        }
      }
    }
  "
  let held_read_variables =
    dict.from_list([
      #("id", root_field.StringVal(order_id)),
      #("first", root_field.IntVal(5)),
    ])
  let assert Ok(held_read) =
    orders.process(hold_outcome.store, held_read_query, held_read_variables)
  assert json.to_string(held_read)
    == "{\"data\":{\"order\":{\"fulfillmentOrders\":{\"nodes\":[{\"status\":\"ON_HOLD\",\"fulfillmentHolds\":[{\"handle\":\"local-lifecycle-hold\"}]},{\"status\":\"OPEN\",\"fulfillmentHolds\":[]}]}},\"manualHoldsFulfillmentOrders\":{\"nodes\":[{\"id\":\"gid://shopify/FulfillmentOrder/hold-release\",\"status\":\"ON_HOLD\",\"fulfillmentHolds\":[{\"handle\":\"local-lifecycle-hold\"}]}]}}}"

  let release_mutation =
    "
    mutation ReleaseHold($id: ID!) {
      fulfillmentOrderReleaseHold(id: $id) {
        fulfillmentOrder {
          id
          status
          fulfillmentHolds {
            handle
          }
          supportedActions {
            action
          }
          lineItems(first: 5) {
            nodes {
              totalQuantity
              remainingQuantity
            }
          }
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  let release_outcome =
    orders.process_mutation(
      hold_outcome.store,
      hold_outcome.identity,
      "/admin/api/2026-04/graphql.json",
      release_mutation,
      dict.from_list([#("id", root_field.StringVal(fulfillment_order_id))]),
      empty_upstream_context(),
    )
  assert json.to_string(release_outcome.data)
    == "{\"data\":{\"fulfillmentOrderReleaseHold\":{\"fulfillmentOrder\":{\"id\":\"gid://shopify/FulfillmentOrder/hold-release\",\"status\":\"OPEN\",\"fulfillmentHolds\":[],\"supportedActions\":[{\"action\":\"CREATE_FULFILLMENT\"},{\"action\":\"REPORT_PROGRESS\"},{\"action\":\"MOVE\"},{\"action\":\"HOLD\"},{\"action\":\"SPLIT\"}],\"lineItems\":{\"nodes\":[{\"totalQuantity\":2,\"remainingQuantity\":2}]}},\"userErrors\":[]}}}"

  let assert Ok(released_read) =
    orders.process(release_outcome.store, held_read_query, held_read_variables)
  assert json.to_string(released_read)
    == "{\"data\":{\"order\":{\"fulfillmentOrders\":{\"nodes\":[{\"status\":\"OPEN\",\"fulfillmentHolds\":[]},{\"status\":\"CLOSED\",\"fulfillmentHolds\":[]}]}},\"manualHoldsFulfillmentOrders\":{\"nodes\":[]}}}"
}

pub fn orders_fulfillment_order_hold_persists_external_id_and_notify_test() {
  let order_id = "gid://shopify/Order/fulfillment-order-hold-inputs"
  let fulfillment_order_id = "gid://shopify/FulfillmentOrder/hold-inputs"
  let seeded =
    fulfillment_order_lifecycle_store(
      order_id,
      fulfillment_order_id,
      "gid://shopify/FulfillmentOrderLineItem/hold-inputs",
      "gid://shopify/LineItem/fulfillment-order-hold-inputs",
    )
  let hold_mutation =
    "
    mutation Hold($id: ID!, $fulfillmentHold: FulfillmentOrderHoldInput!) {
      fulfillmentOrderHold(id: $id, fulfillmentHold: $fulfillmentHold) {
        fulfillmentHold {
          handle
          externalId
          __draftProxyNotifyMerchant
        }
        fulfillmentOrder {
          status
          fulfillmentHolds {
            handle
            externalId
            __draftProxyNotifyMerchant
          }
        }
        userErrors {
          field
          message
          code
        }
      }
    }
  "
  let first_outcome =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      hold_mutation,
      dict.from_list([
        #("id", root_field.StringVal(fulfillment_order_id)),
        #(
          "fulfillmentHold",
          root_field.ObjectVal(
            dict.from_list([
              #("reason", root_field.StringVal("OTHER")),
              #("handle", root_field.StringVal("h1")),
              #("externalId", root_field.StringVal("abc-123")),
              #("notifyMerchant", root_field.BoolVal(True)),
            ]),
          ),
        ),
      ]),
      empty_upstream_context(),
    )
  assert json.to_string(first_outcome.data)
    == "{\"data\":{\"fulfillmentOrderHold\":{\"fulfillmentHold\":{\"handle\":\"h1\",\"externalId\":\"abc-123\",\"__draftProxyNotifyMerchant\":true},\"fulfillmentOrder\":{\"status\":\"ON_HOLD\",\"fulfillmentHolds\":[{\"handle\":\"h1\",\"externalId\":\"abc-123\",\"__draftProxyNotifyMerchant\":true}]},\"userErrors\":[]}}}"

  let second_outcome =
    orders.process_mutation(
      first_outcome.store,
      first_outcome.identity,
      "/admin/api/2026-04/graphql.json",
      hold_mutation,
      dict.from_list([
        #("id", root_field.StringVal(fulfillment_order_id)),
        #(
          "fulfillmentHold",
          root_field.ObjectVal(
            dict.from_list([
              #("reason", root_field.StringVal("OTHER")),
              #("handle", root_field.StringVal("h2")),
              #("externalId", root_field.NullVal),
              #("notifyMerchant", root_field.BoolVal(False)),
            ]),
          ),
        ),
      ]),
      empty_upstream_context(),
    )
  assert json.to_string(second_outcome.data)
    == "{\"data\":{\"fulfillmentOrderHold\":{\"fulfillmentHold\":{\"handle\":\"h2\",\"externalId\":null,\"__draftProxyNotifyMerchant\":false},\"fulfillmentOrder\":{\"status\":\"ON_HOLD\",\"fulfillmentHolds\":[{\"handle\":\"h1\",\"externalId\":\"abc-123\",\"__draftProxyNotifyMerchant\":true},{\"handle\":\"h2\",\"externalId\":null,\"__draftProxyNotifyMerchant\":false}]},\"userErrors\":[]}}}"

  let read_query =
    "
    query HeldReads($id: ID!, $first: Int!) {
      order(id: $id) {
        fulfillmentOrders(first: $first) {
          nodes {
            status
            fulfillmentHolds {
              handle
              externalId
              __draftProxyNotifyMerchant
            }
          }
        }
      }
      manualHoldsFulfillmentOrders(first: $first) {
        nodes {
          status
          fulfillmentHolds {
            handle
            externalId
            __draftProxyNotifyMerchant
          }
        }
      }
    }
  "
  let assert Ok(held_read) =
    orders.process(
      second_outcome.store,
      read_query,
      dict.from_list([
        #("id", root_field.StringVal(order_id)),
        #("first", root_field.IntVal(5)),
      ]),
    )
  assert json.to_string(held_read)
    == "{\"data\":{\"order\":{\"fulfillmentOrders\":{\"nodes\":[{\"status\":\"ON_HOLD\",\"fulfillmentHolds\":[{\"handle\":\"h1\",\"externalId\":\"abc-123\",\"__draftProxyNotifyMerchant\":true},{\"handle\":\"h2\",\"externalId\":null,\"__draftProxyNotifyMerchant\":false}]}]}},\"manualHoldsFulfillmentOrders\":{\"nodes\":[{\"status\":\"ON_HOLD\",\"fulfillmentHolds\":[{\"handle\":\"h1\",\"externalId\":\"abc-123\",\"__draftProxyNotifyMerchant\":true},{\"handle\":\"h2\",\"externalId\":null,\"__draftProxyNotifyMerchant\":false}]}]}}}"
}

pub fn orders_fulfillment_order_hold_validation_branches_test() {
  let order_id = "gid://shopify/Order/fulfillment-order-hold-validation"
  let fulfillment_order_id = "gid://shopify/FulfillmentOrder/hold-validation"
  let fulfillment_order_line_item_id =
    "gid://shopify/FulfillmentOrderLineItem/hold-validation"
  let line_item_id = "gid://shopify/LineItem/fulfillment-order-hold-validation"
  let seeded =
    fulfillment_order_lifecycle_store(
      order_id,
      fulfillment_order_id,
      fulfillment_order_line_item_id,
      line_item_id,
    )
  let hold_mutation =
    "
    mutation Hold($id: ID!, $fulfillmentHold: FulfillmentOrderHoldInput!) {
      fulfillmentOrderHold(id: $id, fulfillmentHold: $fulfillmentHold) {
        fulfillmentHold {
          handle
        }
        fulfillmentOrder {
          status
          fulfillmentHolds {
            handle
          }
        }
        remainingFulfillmentOrder {
          status
        }
        userErrors {
          field
          message
          code
        }
      }
    }
  "
  let first_outcome =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      hold_mutation,
      fulfillment_order_hold_variables(fulfillment_order_id, Some("appA-1"), [
        #(fulfillment_order_line_item_id, 1),
      ]),
      empty_upstream_context(),
    )
  assert json.to_string(first_outcome.data)
    == "{\"data\":{\"fulfillmentOrderHold\":{\"fulfillmentHold\":{\"handle\":\"appA-1\"},\"fulfillmentOrder\":{\"status\":\"ON_HOLD\",\"fulfillmentHolds\":[{\"handle\":\"appA-1\"}]},\"remainingFulfillmentOrder\":{\"status\":\"OPEN\"},\"userErrors\":[]}}}"

  let second_outcome =
    orders.process_mutation(
      first_outcome.store,
      first_outcome.identity,
      "/admin/api/2026-04/graphql.json",
      hold_mutation,
      fulfillment_order_hold_variables(fulfillment_order_id, Some("appA-1"), []),
      empty_upstream_context(),
    )
  assert json.to_string(second_outcome.data)
    == "{\"data\":{\"fulfillmentOrderHold\":{\"fulfillmentHold\":null,\"fulfillmentOrder\":null,\"remainingFulfillmentOrder\":null,\"userErrors\":[{\"field\":[\"fulfillmentHold\",\"handle\"],\"message\":\"The handle provided for the fulfillment hold is already in use by this app for another hold on this fulfillment order.\",\"code\":\"DUPLICATE_FULFILLMENT_HOLD_HANDLE\"}]}}}"

  let split_held_outcome =
    orders.process_mutation(
      first_outcome.store,
      first_outcome.identity,
      "/admin/api/2026-04/graphql.json",
      hold_mutation,
      fulfillment_order_hold_variables(fulfillment_order_id, Some("appA-2"), [
        #(fulfillment_order_line_item_id, 1),
      ]),
      empty_upstream_context(),
    )
  assert json.to_string(split_held_outcome.data)
    == "{\"data\":{\"fulfillmentOrderHold\":{\"fulfillmentHold\":null,\"fulfillmentOrder\":null,\"remainingFulfillmentOrder\":null,\"userErrors\":[{\"field\":[\"fulfillmentHold\",\"fulfillmentOrderLineItems\"],\"message\":\"The fulfillment order is not in a splittable state.\",\"code\":\"FULFILLMENT_ORDER_NOT_SPLITTABLE\"}]}}}"

  let append_outcome =
    orders.process_mutation(
      first_outcome.store,
      first_outcome.identity,
      "/admin/api/2026-04/graphql.json",
      hold_mutation,
      fulfillment_order_hold_variables(fulfillment_order_id, Some("appA-2"), []),
      empty_upstream_context(),
    )
  assert json.to_string(append_outcome.data)
    == "{\"data\":{\"fulfillmentOrderHold\":{\"fulfillmentHold\":{\"handle\":\"appA-2\"},\"fulfillmentOrder\":{\"status\":\"ON_HOLD\",\"fulfillmentHolds\":[{\"handle\":\"appA-1\"},{\"handle\":\"appA-2\"}]},\"remainingFulfillmentOrder\":null,\"userErrors\":[]}}}"

  let #(limit_ready_store, limit_ready_identity) =
    [
      "appA-4",
      "appA-5",
      "appA-6",
      "appA-7",
      "appA-8",
      "appA-9",
      "appA-10",
      "appA-11",
    ]
    |> list.fold(
      #(append_outcome.store, append_outcome.identity),
      fn(acc, handle) {
        let #(current_store, current_identity) = acc
        let outcome =
          orders.process_mutation(
            current_store,
            current_identity,
            "/admin/api/2026-04/graphql.json",
            hold_mutation,
            fulfillment_order_hold_variables(
              fulfillment_order_id,
              Some(handle),
              [],
            ),
            empty_upstream_context(),
          )
        #(outcome.store, outcome.identity)
      },
    )

  let limit_outcome =
    orders.process_mutation(
      limit_ready_store,
      limit_ready_identity,
      "/admin/api/2026-04/graphql.json",
      hold_mutation,
      fulfillment_order_hold_variables(
        fulfillment_order_id,
        Some("appA-12"),
        [],
      ),
      empty_upstream_context(),
    )
  assert json.to_string(limit_outcome.data)
    == "{\"data\":{\"fulfillmentOrderHold\":{\"fulfillmentHold\":null,\"fulfillmentOrder\":null,\"remainingFulfillmentOrder\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"The maximum number of fulfillment holds for this fulfillment order has been reached for this app. An app can only have up to 10 holds on a single fulfillment order at any one time.\",\"code\":\"FULFILLMENT_ORDER_HOLD_LIMIT_REACHED\"}]}}}"

  let zero_quantity_outcome =
    orders.process_mutation(
      append_outcome.store,
      append_outcome.identity,
      "/admin/api/2026-04/graphql.json",
      hold_mutation,
      fulfillment_order_hold_variables(fulfillment_order_id, Some("appA-4"), [
        #(fulfillment_order_line_item_id, 0),
      ]),
      empty_upstream_context(),
    )
  assert json.to_string(zero_quantity_outcome.data)
    == "{\"data\":{\"fulfillmentOrderHold\":{\"fulfillmentHold\":null,\"fulfillmentOrder\":null,\"remainingFulfillmentOrder\":null,\"userErrors\":[{\"field\":[\"fulfillmentHold\",\"fulfillmentOrderLineItems\",\"0\",\"quantity\"],\"message\":\"You must select at least one item to place on partial hold.\",\"code\":\"GREATER_THAN_ZERO\"}]}}}"

  let duplicate_line_item_outcome =
    orders.process_mutation(
      append_outcome.store,
      append_outcome.identity,
      "/admin/api/2026-04/graphql.json",
      hold_mutation,
      fulfillment_order_hold_variables(fulfillment_order_id, Some("appA-4"), [
        #(fulfillment_order_line_item_id, 1),
        #(fulfillment_order_line_item_id, 1),
      ]),
      empty_upstream_context(),
    )
  assert json.to_string(duplicate_line_item_outcome.data)
    == "{\"data\":{\"fulfillmentOrderHold\":{\"fulfillmentHold\":null,\"fulfillmentOrder\":null,\"remainingFulfillmentOrder\":null,\"userErrors\":[{\"field\":[\"fulfillmentHold\",\"fulfillmentOrderLineItems\"],\"message\":\"must contain unique line item ids\",\"code\":\"DUPLICATED_FULFILLMENT_ORDER_LINE_ITEMS\"}]}}}"

  let default_handle_order_id =
    "gid://shopify/Order/fulfillment-order-hold-default-handle"
  let default_handle_fulfillment_order_id =
    "gid://shopify/FulfillmentOrder/hold-default-handle"
  let default_handle_line_item_id =
    "gid://shopify/FulfillmentOrderLineItem/hold-default-handle"
  let default_seeded =
    fulfillment_order_lifecycle_store(
      default_handle_order_id,
      default_handle_fulfillment_order_id,
      default_handle_line_item_id,
      "gid://shopify/LineItem/fulfillment-order-hold-default-handle",
    )
  let default_handle_outcome =
    orders.process_mutation(
      default_seeded,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      hold_mutation,
      fulfillment_order_hold_variables(
        default_handle_fulfillment_order_id,
        None,
        [],
      ),
      empty_upstream_context(),
    )
  assert json.to_string(default_handle_outcome.data)
    == "{\"data\":{\"fulfillmentOrderHold\":{\"fulfillmentHold\":{\"handle\":\"\"},\"fulfillmentOrder\":{\"status\":\"ON_HOLD\",\"fulfillmentHolds\":[{\"handle\":\"\"}]},\"remainingFulfillmentOrder\":null,\"userErrors\":[]}}}"

  let too_long_handle_outcome =
    orders.process_mutation(
      default_seeded,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      hold_mutation,
      fulfillment_order_hold_variables(
        default_handle_fulfillment_order_id,
        Some(
          "hhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhh",
        ),
        [],
      ),
      empty_upstream_context(),
    )
  assert json.to_string(too_long_handle_outcome.data)
    == "{\"data\":{\"fulfillmentOrderHold\":{\"fulfillmentHold\":null,\"fulfillmentOrder\":null,\"remainingFulfillmentOrder\":null,\"userErrors\":[{\"field\":[\"fulfillmentHold\",\"handle\"],\"message\":\"Handle is too long (maximum is 64 characters)\",\"code\":\"TOO_LONG\"}]}}}"
}

fn fulfillment_order_hold_variables(
  fulfillment_order_id: String,
  handle: Option(String),
  line_items: List(#(String, Int)),
) -> Dict(String, root_field.ResolvedValue) {
  let hold_fields = [
    #("reason", root_field.StringVal("OTHER")),
    #("notifyMerchant", root_field.BoolVal(False)),
  ]
  let hold_fields = case handle {
    Some(handle) ->
      list.append(hold_fields, [#("handle", root_field.StringVal(handle))])
    None -> hold_fields
  }
  let hold_fields = case line_items {
    [] -> hold_fields
    [_, ..] ->
      list.append(hold_fields, [
        #(
          "fulfillmentOrderLineItems",
          root_field.ListVal(
            list.map(line_items, fn(line_item) {
              root_field.ObjectVal(
                dict.from_list([
                  #("id", root_field.StringVal(line_item.0)),
                  #("quantity", root_field.IntVal(line_item.1)),
                ]),
              )
            }),
          ),
        ),
      ])
  }
  dict.from_list([
    #("id", root_field.StringVal(fulfillment_order_id)),
    #("fulfillmentHold", root_field.ObjectVal(dict.from_list(hold_fields))),
  ])
}

pub fn orders_fulfillment_order_lifecycle_mutations_read_after_write_test() {
  let order_id = "gid://shopify/Order/fulfillment-order-lifecycle"
  let fulfillment_order_id = "gid://shopify/FulfillmentOrder/lifecycle"
  let fulfillment_order_line_item_id =
    "gid://shopify/FulfillmentOrderLineItem/lifecycle"
  let line_item_id = "gid://shopify/LineItem/fulfillment-order-lifecycle"
  let seeded =
    fulfillment_order_lifecycle_store(
      order_id,
      fulfillment_order_id,
      fulfillment_order_line_item_id,
      line_item_id,
    )
  let move_mutation =
    "
    mutation Move($id: ID!, $newLocationId: ID!, $fulfillmentOrderLineItems: [FulfillmentOrderLineItemInput!]) {
      fulfillmentOrderMove(id: $id, newLocationId: $newLocationId, fulfillmentOrderLineItems: $fulfillmentOrderLineItems) {
        movedFulfillmentOrder {
          id
          status
          assignedLocation {
            name
            location {
              id
              name
            }
          }
          lineItems(first: 5) {
            nodes {
              totalQuantity
              remainingQuantity
            }
          }
        }
        originalFulfillmentOrder {
          id
          status
          lineItems(first: 5) {
            nodes {
              totalQuantity
              remainingQuantity
            }
          }
        }
        remainingFulfillmentOrder {
          id
          status
          lineItems(first: 5) {
            nodes {
              totalQuantity
              remainingQuantity
            }
          }
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  let move_variables =
    dict.from_list([
      #("id", root_field.StringVal(fulfillment_order_id)),
      #(
        "newLocationId",
        root_field.StringVal("gid://shopify/Location/destination"),
      ),
      #(
        "fulfillmentOrderLineItems",
        root_field.ListVal([
          root_field.ObjectVal(
            dict.from_list([
              #("id", root_field.StringVal(fulfillment_order_line_item_id)),
              #("quantity", root_field.IntVal(1)),
            ]),
          ),
        ]),
      ),
    ])
  let move_outcome =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      move_mutation,
      move_variables,
      empty_upstream_context(),
    )
  assert json.to_string(move_outcome.data)
    == "{\"data\":{\"fulfillmentOrderMove\":{\"movedFulfillmentOrder\":{\"id\":\"gid://shopify/FulfillmentOrder/2\",\"status\":\"OPEN\",\"assignedLocation\":{\"name\":\"Shop location\",\"location\":{\"id\":\"gid://shopify/Location/destination\",\"name\":\"Shop location\"}},\"lineItems\":{\"nodes\":[{\"totalQuantity\":1,\"remainingQuantity\":1}]}},\"originalFulfillmentOrder\":{\"id\":\"gid://shopify/FulfillmentOrder/lifecycle\",\"status\":\"OPEN\",\"lineItems\":{\"nodes\":[{\"totalQuantity\":1,\"remainingQuantity\":1}]}},\"remainingFulfillmentOrder\":{\"id\":\"gid://shopify/FulfillmentOrder/lifecycle\",\"status\":\"OPEN\",\"lineItems\":{\"nodes\":[{\"totalQuantity\":1,\"remainingQuantity\":1}]}},\"userErrors\":[]}}}"

  let moved_fulfillment_order_id = "gid://shopify/FulfillmentOrder/2"
  let progress_mutation =
    "
    mutation Progress($id: ID!, $progressReport: FulfillmentOrderReportProgressInput) {
      fulfillmentOrderReportProgress(id: $id, progressReport: $progressReport) {
        fulfillmentOrder {
          id
          status
          supportedActions {
            action
          }
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  let progress_outcome =
    orders.process_mutation(
      move_outcome.store,
      move_outcome.identity,
      "/admin/api/2026-04/graphql.json",
      progress_mutation,
      dict.from_list([
        #("id", root_field.StringVal(moved_fulfillment_order_id)),
        #(
          "progressReport",
          root_field.ObjectVal(
            dict.from_list([
              #("reasonNotes", root_field.StringVal("Local progress")),
            ]),
          ),
        ),
      ]),
      empty_upstream_context(),
    )
  assert json.to_string(progress_outcome.data)
    == "{\"data\":{\"fulfillmentOrderReportProgress\":{\"fulfillmentOrder\":{\"id\":\"gid://shopify/FulfillmentOrder/2\",\"status\":\"IN_PROGRESS\",\"supportedActions\":[{\"action\":\"CREATE_FULFILLMENT\"},{\"action\":\"REPORT_PROGRESS\"},{\"action\":\"HOLD\"},{\"action\":\"MARK_AS_OPEN\"}]},\"userErrors\":[]}}}"

  let open_mutation =
    "
    mutation Open($id: ID!) {
      fulfillmentOrderOpen(id: $id) {
        fulfillmentOrder {
          id
          status
          supportedActions {
            action
          }
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  let open_outcome =
    orders.process_mutation(
      progress_outcome.store,
      progress_outcome.identity,
      "/admin/api/2026-04/graphql.json",
      open_mutation,
      dict.from_list([#("id", root_field.StringVal(moved_fulfillment_order_id))]),
      empty_upstream_context(),
    )
  assert json.to_string(open_outcome.data)
    == "{\"data\":{\"fulfillmentOrderOpen\":{\"fulfillmentOrder\":{\"id\":\"gid://shopify/FulfillmentOrder/2\",\"status\":\"OPEN\",\"supportedActions\":[{\"action\":\"CREATE_FULFILLMENT\"},{\"action\":\"REPORT_PROGRESS\"},{\"action\":\"MOVE\"},{\"action\":\"HOLD\"}]},\"userErrors\":[]}}}"

  let cancel_mutation =
    "
    mutation Cancel($id: ID!) {
      fulfillmentOrderCancel(id: $id) {
        fulfillmentOrder {
          id
          status
          lineItems(first: 5) {
            nodes {
              id
            }
          }
        }
        replacementFulfillmentOrder {
          status
          lineItems(first: 5) {
            nodes {
              totalQuantity
              remainingQuantity
            }
          }
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  let cancel_outcome =
    orders.process_mutation(
      open_outcome.store,
      open_outcome.identity,
      "/admin/api/2026-04/graphql.json",
      cancel_mutation,
      dict.from_list([#("id", root_field.StringVal(moved_fulfillment_order_id))]),
      empty_upstream_context(),
    )
  assert json.to_string(cancel_outcome.data)
    == "{\"data\":{\"fulfillmentOrderCancel\":{\"fulfillmentOrder\":{\"id\":\"gid://shopify/FulfillmentOrder/2\",\"status\":\"CLOSED\",\"lineItems\":{\"nodes\":[]}},\"replacementFulfillmentOrder\":{\"status\":\"OPEN\",\"lineItems\":{\"nodes\":[{\"totalQuantity\":1,\"remainingQuantity\":1}]}},\"userErrors\":[]}}}"

  let guardrail_mutation =
    "
    mutation Guardrails($id: ID!, $fulfillAt: DateTime!, $message: String) {
      fulfillmentOrderReschedule(id: $id, fulfillAt: $fulfillAt) {
        fulfillmentOrder {
          id
        }
        userErrors {
          field
          message
        }
      }
      fulfillmentOrderClose(id: $id, message: $message) {
        fulfillmentOrder {
          id
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  let guardrail_outcome =
    orders.process_mutation(
      cancel_outcome.store,
      cancel_outcome.identity,
      "/admin/api/2026-04/graphql.json",
      guardrail_mutation,
      dict.from_list([
        #("id", root_field.StringVal(fulfillment_order_id)),
        #("fulfillAt", root_field.StringVal("2026-04-28T00:00:00Z")),
        #("message", root_field.StringVal("close guardrail")),
      ]),
      empty_upstream_context(),
    )
  assert json.to_string(guardrail_outcome.data)
    == "{\"data\":{\"fulfillmentOrderReschedule\":{\"fulfillmentOrder\":null,\"userErrors\":[{\"field\":null,\"message\":\"Fulfillment order must be scheduled.\"}]},\"fulfillmentOrderClose\":{\"fulfillmentOrder\":null,\"userErrors\":[{\"field\":null,\"message\":\"The fulfillment order's assigned fulfillment service must be of api type\"}]}}}"

  let downstream_query =
    "
    query Downstream($orderId: ID!) {
      order(id: $orderId) {
        displayFulfillmentStatus
        fulfillmentOrders(first: 10, includeClosed: true) {
          nodes {
            status
            lineItems(first: 5) {
              nodes {
                totalQuantity
                remainingQuantity
              }
            }
          }
        }
      }
      fulfillmentOrders(first: 10) {
        nodes {
          status
        }
      }
    }
  "
  let assert Ok(downstream) =
    orders.process(
      guardrail_outcome.store,
      downstream_query,
      dict.from_list([#("orderId", root_field.StringVal(order_id))]),
    )
  assert json.to_string(downstream)
    == "{\"data\":{\"order\":{\"displayFulfillmentStatus\":\"UNFULFILLED\",\"fulfillmentOrders\":{\"nodes\":[{\"status\":\"OPEN\",\"lineItems\":{\"nodes\":[{\"totalQuantity\":1,\"remainingQuantity\":1}]}},{\"status\":\"CLOSED\",\"lineItems\":{\"nodes\":[]}},{\"status\":\"OPEN\",\"lineItems\":{\"nodes\":[{\"totalQuantity\":1,\"remainingQuantity\":1}]}}]}},\"fulfillmentOrders\":{\"nodes\":[{\"status\":\"OPEN\"},{\"status\":\"OPEN\"}]}}}"
}

pub fn orders_fulfillment_order_cancel_rejects_closed_orders_test() {
  let order_id = "gid://shopify/Order/fulfillment-order-cancel-closed"
  let fulfillment_order_id = "gid://shopify/FulfillmentOrder/cancel-closed"
  let fulfillment_order_line_item_id =
    "gid://shopify/FulfillmentOrderLineItem/cancel-closed"
  let line_item_id = "gid://shopify/LineItem/fulfillment-order-cancel-closed"
  let seeded =
    fulfillment_order_lifecycle_store(
      order_id,
      fulfillment_order_id,
      fulfillment_order_line_item_id,
      line_item_id,
    )

  let cancel_mutation =
    "
    mutation Cancel($id: ID!) {
      fulfillmentOrderCancel(id: $id) {
        fulfillmentOrder {
          id
          status
        }
        replacementFulfillmentOrder {
          id
        }
        userErrors {
          field
          message
          code
        }
      }
    }
  "
  let first_cancel =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      cancel_mutation,
      dict.from_list([#("id", root_field.StringVal(fulfillment_order_id))]),
      empty_upstream_context(),
    )
  let second_cancel =
    orders.process_mutation(
      first_cancel.store,
      first_cancel.identity,
      "/admin/api/2026-04/graphql.json",
      cancel_mutation,
      dict.from_list([#("id", root_field.StringVal(fulfillment_order_id))]),
      empty_upstream_context(),
    )
  assert json.to_string(second_cancel.data)
    == "{\"data\":{\"fulfillmentOrderCancel\":{\"fulfillmentOrder\":null,\"replacementFulfillmentOrder\":null,\"userErrors\":[{\"field\":null,\"message\":\"Fulfillment order is not in cancelable request state and can't be canceled.\",\"code\":\"fulfillment_order_cannot_be_cancelled\"}]}}}"
}

pub fn orders_fulfillment_order_cancel_rejects_manually_reported_progress_test() {
  let order_id = "gid://shopify/Order/fulfillment-order-cancel-progress"
  let fulfillment_order_id = "gid://shopify/FulfillmentOrder/cancel-progress"
  let fulfillment_order_line_item_id =
    "gid://shopify/FulfillmentOrderLineItem/cancel-progress"
  let line_item_id = "gid://shopify/LineItem/fulfillment-order-cancel-progress"
  let seeded =
    fulfillment_order_lifecycle_store(
      order_id,
      fulfillment_order_id,
      fulfillment_order_line_item_id,
      line_item_id,
    )

  let report_mutation =
    "
    mutation Progress($id: ID!) {
      fulfillmentOrderReportProgress(id: $id, progressReport: { reasonNotes: \"manual progress\" }) {
        fulfillmentOrder {
          id
          status
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  let progress_outcome =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      report_mutation,
      dict.from_list([#("id", root_field.StringVal(fulfillment_order_id))]),
      empty_upstream_context(),
    )

  let cancel_mutation =
    "
    mutation Cancel($id: ID!) {
      fulfillmentOrderCancel(id: $id) {
        fulfillmentOrder {
          id
        }
        replacementFulfillmentOrder {
          id
        }
        userErrors {
          field
          message
          code
        }
      }
    }
  "
  let cancel_outcome =
    orders.process_mutation(
      progress_outcome.store,
      progress_outcome.identity,
      "/admin/api/2026-04/graphql.json",
      cancel_mutation,
      dict.from_list([#("id", root_field.StringVal(fulfillment_order_id))]),
      empty_upstream_context(),
    )
  assert json.to_string(cancel_outcome.data)
    == "{\"data\":{\"fulfillmentOrderCancel\":{\"fulfillmentOrder\":null,\"replacementFulfillmentOrder\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"Cannot cancel fulfillment order that has had progress reported. Mark as unfulfilled first.\",\"code\":\"fulfillment_order_has_manually_reported_progress\"}]}}}"
}

pub fn orders_fulfillment_order_split_deadline_merge_read_after_write_test() {
  let order_id = "gid://shopify/Order/fulfillment-order-residual"
  let fulfillment_order_id = "gid://shopify/FulfillmentOrder/residual"
  let fulfillment_order_line_item_id =
    "gid://shopify/FulfillmentOrderLineItem/residual"
  let split_fulfillment_order_id = "gid://shopify/FulfillmentOrder/2"
  let line_item_id = "gid://shopify/LineItem/fulfillment-order-lifecycle"
  let seeded =
    store.upsert_base_orders(store.new(), [
      types.OrderRecord(
        id: order_id,
        cursor: None,
        data: types.CapturedObject([
          #("id", types.CapturedString(order_id)),
          #("name", types.CapturedString("#FULFILLMENT-ORDER-RESIDUAL")),
          #("displayFulfillmentStatus", types.CapturedString("UNFULFILLED")),
          #("fulfillments", types.CapturedArray([])),
          #(
            "fulfillmentOrders",
            types.CapturedArray([
              types.CapturedObject([
                #("id", types.CapturedString(fulfillment_order_id)),
                #("status", types.CapturedString("OPEN")),
                #("requestStatus", types.CapturedString("UNSUBMITTED")),
                #(
                  "assignedLocation",
                  types.CapturedObject([
                    #("name", types.CapturedString("My Custom Location")),
                    #(
                      "locationId",
                      types.CapturedString("gid://shopify/Location/source"),
                    ),
                  ]),
                ),
                #("fulfillmentHolds", types.CapturedArray([])),
                #(
                  "lineItems",
                  types.CapturedArray([
                    types.CapturedObject([
                      #(
                        "id",
                        types.CapturedString(fulfillment_order_line_item_id),
                      ),
                      #("lineItemId", types.CapturedString(line_item_id)),
                      #(
                        "title",
                        types.CapturedString("Fulfillment order lifecycle item"),
                      ),
                      #("lineItemQuantity", types.CapturedInt(3)),
                      #("lineItemFulfillableQuantity", types.CapturedInt(3)),
                      #("totalQuantity", types.CapturedInt(3)),
                      #("remainingQuantity", types.CapturedInt(3)),
                    ]),
                  ]),
                ),
              ]),
            ]),
          ),
        ]),
      ),
    ])
  let split_mutation =
    "
    mutation Split($fulfillmentOrderSplits: [FulfillmentOrderSplitInput!]!) {
      fulfillmentOrderSplit(fulfillmentOrderSplits: $fulfillmentOrderSplits) {
        fulfillmentOrderSplits {
          fulfillmentOrder {
            id
            status
            supportedActions {
              action
            }
            lineItems(first: 10) {
              nodes {
                id
                totalQuantity
                remainingQuantity
                lineItem {
                  id
                  quantity
                  fulfillableQuantity
                }
              }
            }
          }
          remainingFulfillmentOrder {
            id
            status
            supportedActions {
              action
            }
            lineItems(first: 10) {
              nodes {
                totalQuantity
                remainingQuantity
                lineItem {
                  id
                  quantity
                  fulfillableQuantity
                }
              }
            }
          }
          replacementFulfillmentOrder {
            id
          }
        }
        userErrors {
          field
          message
          code
        }
      }
    }
  "
  let split_variables =
    dict.from_list([
      #(
        "fulfillmentOrderSplits",
        root_field.ListVal([
          root_field.ObjectVal(
            dict.from_list([
              #(
                "fulfillmentOrderId",
                root_field.StringVal(fulfillment_order_id),
              ),
              #(
                "fulfillmentOrderLineItems",
                root_field.ListVal([
                  root_field.ObjectVal(
                    dict.from_list([
                      #(
                        "id",
                        root_field.StringVal(fulfillment_order_line_item_id),
                      ),
                      #("quantity", root_field.IntVal(1)),
                    ]),
                  ),
                ]),
              ),
            ]),
          ),
        ]),
      ),
    ])
  let split_outcome =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      split_mutation,
      split_variables,
      empty_upstream_context(),
    )
  assert json.to_string(split_outcome.data)
    == "{\"data\":{\"fulfillmentOrderSplit\":{\"fulfillmentOrderSplits\":[{\"fulfillmentOrder\":{\"id\":\"gid://shopify/FulfillmentOrder/residual\",\"status\":\"OPEN\",\"supportedActions\":[{\"action\":\"CREATE_FULFILLMENT\"},{\"action\":\"REPORT_PROGRESS\"},{\"action\":\"MOVE\"},{\"action\":\"HOLD\"},{\"action\":\"SPLIT\"},{\"action\":\"MERGE\"}],\"lineItems\":{\"nodes\":[{\"id\":\"gid://shopify/FulfillmentOrderLineItem/residual\",\"totalQuantity\":2,\"remainingQuantity\":2,\"lineItem\":{\"id\":\"gid://shopify/LineItem/fulfillment-order-lifecycle\",\"quantity\":3,\"fulfillableQuantity\":3}}]}},\"remainingFulfillmentOrder\":{\"id\":\"gid://shopify/FulfillmentOrder/2\",\"status\":\"OPEN\",\"supportedActions\":[{\"action\":\"CREATE_FULFILLMENT\"},{\"action\":\"REPORT_PROGRESS\"},{\"action\":\"MOVE\"},{\"action\":\"HOLD\"},{\"action\":\"MERGE\"}],\"lineItems\":{\"nodes\":[{\"totalQuantity\":1,\"remainingQuantity\":1,\"lineItem\":{\"id\":\"gid://shopify/LineItem/fulfillment-order-lifecycle\",\"quantity\":3,\"fulfillableQuantity\":3}}]}},\"replacementFulfillmentOrder\":null}],\"userErrors\":[]}}}"

  let deadline = "2026-05-02T02:16:59Z"
  let deadline_mutation =
    "
    mutation Deadline($fulfillmentOrderIds: [ID!]!, $fulfillmentDeadline: DateTime!) {
      fulfillmentOrdersSetFulfillmentDeadline(
        fulfillmentOrderIds: $fulfillmentOrderIds
        fulfillmentDeadline: $fulfillmentDeadline
      ) {
        success
        userErrors {
          field
          message
          code
        }
      }
    }
  "
  let deadline_outcome =
    orders.process_mutation(
      split_outcome.store,
      split_outcome.identity,
      "/admin/api/2026-04/graphql.json",
      deadline_mutation,
      dict.from_list([
        #(
          "fulfillmentOrderIds",
          root_field.ListVal([
            root_field.StringVal(fulfillment_order_id),
            root_field.StringVal(split_fulfillment_order_id),
          ]),
        ),
        #("fulfillmentDeadline", root_field.StringVal(deadline)),
      ]),
      empty_upstream_context(),
    )
  assert json.to_string(deadline_outcome.data)
    == "{\"data\":{\"fulfillmentOrdersSetFulfillmentDeadline\":{\"success\":true,\"userErrors\":[]}}}"

  let merge_mutation =
    "
    mutation Merge($fulfillmentOrderMergeInputs: [FulfillmentOrderMergeInput!]!) {
      fulfillmentOrderMerge(fulfillmentOrderMergeInputs: $fulfillmentOrderMergeInputs) {
        fulfillmentOrderMerges {
          fulfillmentOrder {
            id
            status
            fulfillBy
            supportedActions {
              action
            }
            lineItems(first: 10) {
              nodes {
                id
                totalQuantity
                remainingQuantity
                lineItem {
                  id
                  quantity
                  fulfillableQuantity
                }
              }
            }
          }
        }
        userErrors {
          field
          message
          code
        }
      }
    }
  "
  let merge_outcome =
    orders.process_mutation(
      deadline_outcome.store,
      deadline_outcome.identity,
      "/admin/api/2026-04/graphql.json",
      merge_mutation,
      dict.from_list([
        #(
          "fulfillmentOrderMergeInputs",
          root_field.ListVal([
            root_field.ObjectVal(
              dict.from_list([
                #(
                  "mergeIntents",
                  root_field.ListVal([
                    root_field.ObjectVal(
                      dict.from_list([
                        #(
                          "fulfillmentOrderId",
                          root_field.StringVal(fulfillment_order_id),
                        ),
                      ]),
                    ),
                    root_field.ObjectVal(
                      dict.from_list([
                        #(
                          "fulfillmentOrderId",
                          root_field.StringVal(split_fulfillment_order_id),
                        ),
                      ]),
                    ),
                  ]),
                ),
              ]),
            ),
          ]),
        ),
      ]),
      empty_upstream_context(),
    )
  assert json.to_string(merge_outcome.data)
    == "{\"data\":{\"fulfillmentOrderMerge\":{\"fulfillmentOrderMerges\":[{\"fulfillmentOrder\":{\"id\":\"gid://shopify/FulfillmentOrder/residual\",\"status\":\"OPEN\",\"fulfillBy\":\"2026-05-02T02:16:59Z\",\"supportedActions\":[{\"action\":\"CREATE_FULFILLMENT\"},{\"action\":\"REPORT_PROGRESS\"},{\"action\":\"MOVE\"},{\"action\":\"HOLD\"},{\"action\":\"SPLIT\"}],\"lineItems\":{\"nodes\":[{\"id\":\"gid://shopify/FulfillmentOrderLineItem/residual\",\"totalQuantity\":3,\"remainingQuantity\":3,\"lineItem\":{\"id\":\"gid://shopify/LineItem/fulfillment-order-lifecycle\",\"quantity\":3,\"fulfillableQuantity\":3}}]}}}],\"userErrors\":[]}}}"

  let downstream_query =
    "
    query MergedRead($id: ID!) {
      order(id: $id) {
        fulfillmentOrders(first: 10) {
          nodes {
            id
            fulfillBy
            lineItems(first: 5) {
              nodes {
                id
                totalQuantity
                remainingQuantity
              }
            }
          }
        }
      }
    }
  "
  let assert Ok(downstream) =
    orders.process(
      merge_outcome.store,
      downstream_query,
      dict.from_list([#("id", root_field.StringVal(order_id))]),
    )
  assert json.to_string(downstream)
    == "{\"data\":{\"order\":{\"fulfillmentOrders\":{\"nodes\":[{\"id\":\"gid://shopify/FulfillmentOrder/residual\",\"fulfillBy\":\"2026-05-02T02:16:59Z\",\"lineItems\":{\"nodes\":[{\"id\":\"gid://shopify/FulfillmentOrderLineItem/residual\",\"totalQuantity\":3,\"remainingQuantity\":3}]}},{\"id\":\"gid://shopify/FulfillmentOrder/2\",\"fulfillBy\":\"2026-05-02T02:16:59Z\",\"lineItems\":{\"nodes\":[{\"id\":\"gid://shopify/FulfillmentOrderLineItem/1\",\"totalQuantity\":0,\"remainingQuantity\":0}]}}]}}}}"
}

pub fn orders_fulfillment_order_request_cancellation_read_after_write_test() {
  let order_id = "gid://shopify/Order/fulfillment-order-requests"
  let partial_id = "gid://shopify/FulfillmentOrder/partial"
  let partial_line_item_id = "gid://shopify/FulfillmentOrderLineItem/partial"
  let reject_id = "gid://shopify/FulfillmentOrder/reject"
  let cancel_id = "gid://shopify/FulfillmentOrder/cancel"
  let unsubmitted_id = "gid://shopify/FulfillmentOrder/3"
  let seeded = fulfillment_request_store(order_id)
  let fields =
    "
      id
      status
      requestStatus
      merchantRequests(first: 10) {
        nodes { kind message requestOptions responseData }
      }
      lineItems(first: 5) {
        nodes { id totalQuantity remainingQuantity lineItem { id title } }
      }
    "
  let submit_mutation = "
    mutation SubmitRequest($id: ID!, $lineItems: [FulfillmentOrderLineItemInput!]) {
      fulfillmentOrderSubmitFulfillmentRequest(
        id: $id
        message: \"submit partial\"
        notifyCustomer: false
        fulfillmentOrderLineItems: $lineItems
      ) {
        submittedFulfillmentOrder { " <> fields <> " }
        unsubmittedFulfillmentOrder { " <> fields <> " }
        userErrors { field message }
      }
    }
  "
  let submit_outcome =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      submit_mutation,
      dict.from_list([
        #("id", root_field.StringVal(partial_id)),
        #(
          "lineItems",
          root_field.ListVal([
            root_field.ObjectVal(
              dict.from_list([
                #("id", root_field.StringVal(partial_line_item_id)),
                #("quantity", root_field.IntVal(1)),
              ]),
            ),
          ]),
        ),
      ]),
      empty_upstream_context(),
    )
  assert json.to_string(submit_outcome.data)
    == "{\"data\":{\"fulfillmentOrderSubmitFulfillmentRequest\":{\"submittedFulfillmentOrder\":{\"id\":\"gid://shopify/FulfillmentOrder/partial\",\"status\":\"OPEN\",\"requestStatus\":\"SUBMITTED\",\"merchantRequests\":{\"nodes\":[{\"kind\":\"FULFILLMENT_REQUEST\",\"message\":\"submit partial\",\"requestOptions\":{\"notify_customer\":false},\"responseData\":null}]},\"lineItems\":{\"nodes\":[{\"id\":\"gid://shopify/FulfillmentOrderLineItem/partial\",\"totalQuantity\":1,\"remainingQuantity\":1,\"lineItem\":{\"id\":\"gid://shopify/LineItem/partial\",\"title\":\"Partial request item\"}}]}},\"unsubmittedFulfillmentOrder\":{\"id\":\"gid://shopify/FulfillmentOrder/3\",\"status\":\"OPEN\",\"requestStatus\":\"UNSUBMITTED\",\"merchantRequests\":{\"nodes\":[]},\"lineItems\":{\"nodes\":[{\"id\":\"gid://shopify/FulfillmentOrderLineItem/1\",\"totalQuantity\":1,\"remainingQuantity\":1,\"lineItem\":{\"id\":\"gid://shopify/LineItem/partial\",\"title\":\"Partial request item\"}}]}},\"userErrors\":[]}}}"

  let accept_mutation =
    "
    mutation AcceptRequest($id: ID!) {
      fulfillmentOrderAcceptFulfillmentRequest(id: $id, message: \"accepted\") {
        fulfillmentOrder { id status requestStatus }
        userErrors { field message }
      }
    }
  "
  let accept_outcome =
    orders.process_mutation(
      submit_outcome.store,
      submit_outcome.identity,
      "/admin/api/2026-04/graphql.json",
      accept_mutation,
      dict.from_list([#("id", root_field.StringVal(partial_id))]),
      empty_upstream_context(),
    )
  assert json.to_string(accept_outcome.data)
    == "{\"data\":{\"fulfillmentOrderAcceptFulfillmentRequest\":{\"fulfillmentOrder\":{\"id\":\"gid://shopify/FulfillmentOrder/partial\",\"status\":\"IN_PROGRESS\",\"requestStatus\":\"ACCEPTED\"},\"userErrors\":[]}}}"

  let submit_cancel_mutation =
    "
    mutation SubmitCancellation($id: ID!) {
      fulfillmentOrderSubmitCancellationRequest(id: $id, message: \"cancel requested\") {
        fulfillmentOrder {
          id
          status
          requestStatus
          merchantRequests(first: 10) {
            nodes { kind message }
          }
        }
        userErrors { field message }
      }
    }
  "
  let submit_cancel_outcome =
    orders.process_mutation(
      accept_outcome.store,
      accept_outcome.identity,
      "/admin/api/2026-04/graphql.json",
      submit_cancel_mutation,
      dict.from_list([#("id", root_field.StringVal(partial_id))]),
      empty_upstream_context(),
    )
  assert json.to_string(submit_cancel_outcome.data)
    == "{\"data\":{\"fulfillmentOrderSubmitCancellationRequest\":{\"fulfillmentOrder\":{\"id\":\"gid://shopify/FulfillmentOrder/partial\",\"status\":\"IN_PROGRESS\",\"requestStatus\":\"ACCEPTED\",\"merchantRequests\":{\"nodes\":[{\"kind\":\"FULFILLMENT_REQUEST\",\"message\":\"submit partial\"},{\"kind\":\"CANCELLATION_REQUEST\",\"message\":\"cancel requested\"}]}},\"userErrors\":[]}}}"

  let reject_cancel_mutation =
    "
    mutation RejectCancellation($id: ID!) {
      fulfillmentOrderRejectCancellationRequest(id: $id, message: \"cancel rejected\") {
        fulfillmentOrder { id status requestStatus }
        userErrors { field message }
      }
    }
  "
  let reject_cancel_outcome =
    orders.process_mutation(
      submit_cancel_outcome.store,
      submit_cancel_outcome.identity,
      "/admin/api/2026-04/graphql.json",
      reject_cancel_mutation,
      dict.from_list([#("id", root_field.StringVal(partial_id))]),
      empty_upstream_context(),
    )
  assert json.to_string(reject_cancel_outcome.data)
    == "{\"data\":{\"fulfillmentOrderRejectCancellationRequest\":{\"fulfillmentOrder\":{\"id\":\"gid://shopify/FulfillmentOrder/partial\",\"status\":\"IN_PROGRESS\",\"requestStatus\":\"CANCELLATION_REJECTED\"},\"userErrors\":[]}}}"

  let submit_reject_outcome =
    orders.process_mutation(
      reject_cancel_outcome.store,
      reject_cancel_outcome.identity,
      "/admin/api/2026-04/graphql.json",
      submit_mutation,
      dict.from_list([
        #("id", root_field.StringVal(reject_id)),
        #("lineItems", root_field.NullVal),
      ]),
      empty_upstream_context(),
    )
  let reject_mutation =
    "
    mutation RejectRequest($id: ID!) {
      fulfillmentOrderRejectFulfillmentRequest(id: $id, reason: OTHER, message: \"rejected\") {
        fulfillmentOrder { id status requestStatus }
        userErrors { field message }
      }
    }
  "
  let reject_outcome =
    orders.process_mutation(
      submit_reject_outcome.store,
      submit_reject_outcome.identity,
      "/admin/api/2026-04/graphql.json",
      reject_mutation,
      dict.from_list([#("id", root_field.StringVal(reject_id))]),
      empty_upstream_context(),
    )
  assert json.to_string(reject_outcome.data)
    == "{\"data\":{\"fulfillmentOrderRejectFulfillmentRequest\":{\"fulfillmentOrder\":{\"id\":\"gid://shopify/FulfillmentOrder/reject\",\"status\":\"OPEN\",\"requestStatus\":\"REJECTED\"},\"userErrors\":[]}}}"

  let submit_cancel_request_outcome =
    orders.process_mutation(
      reject_outcome.store,
      reject_outcome.identity,
      "/admin/api/2026-04/graphql.json",
      submit_mutation,
      dict.from_list([
        #("id", root_field.StringVal(cancel_id)),
        #("lineItems", root_field.NullVal),
      ]),
      empty_upstream_context(),
    )
  let accept_cancel_request_outcome =
    orders.process_mutation(
      submit_cancel_request_outcome.store,
      submit_cancel_request_outcome.identity,
      "/admin/api/2026-04/graphql.json",
      accept_mutation,
      dict.from_list([#("id", root_field.StringVal(cancel_id))]),
      empty_upstream_context(),
    )
  let submit_cancel_accept_outcome =
    orders.process_mutation(
      accept_cancel_request_outcome.store,
      accept_cancel_request_outcome.identity,
      "/admin/api/2026-04/graphql.json",
      submit_cancel_mutation,
      dict.from_list([#("id", root_field.StringVal(cancel_id))]),
      empty_upstream_context(),
    )
  let accept_cancel_mutation =
    "
    mutation AcceptCancellation($id: ID!) {
      fulfillmentOrderAcceptCancellationRequest(id: $id, message: \"cancel accepted\") {
        fulfillmentOrder {
          id
          status
          requestStatus
          lineItems(first: 5) { nodes { totalQuantity remainingQuantity } }
        }
        userErrors { field message }
      }
    }
  "
  let accept_cancel_outcome =
    orders.process_mutation(
      submit_cancel_accept_outcome.store,
      submit_cancel_accept_outcome.identity,
      "/admin/api/2026-04/graphql.json",
      accept_cancel_mutation,
      dict.from_list([#("id", root_field.StringVal(cancel_id))]),
      empty_upstream_context(),
    )
  assert json.to_string(accept_cancel_outcome.data)
    == "{\"data\":{\"fulfillmentOrderAcceptCancellationRequest\":{\"fulfillmentOrder\":{\"id\":\"gid://shopify/FulfillmentOrder/cancel\",\"status\":\"CLOSED\",\"requestStatus\":\"CANCELLATION_ACCEPTED\",\"lineItems\":{\"nodes\":[{\"totalQuantity\":0,\"remainingQuantity\":0}]}},\"userErrors\":[]}}}"

  let downstream_query =
    "
    query Downstream($orderId: ID!, $submittedId: ID!, $unsubmittedId: ID!) {
      submitted: fulfillmentOrder(id: $submittedId) { id requestStatus }
      unsubmitted: fulfillmentOrder(id: $unsubmittedId) { id requestStatus }
      assignedFulfillmentOrders(first: 10) {
        nodes { id status requestStatus }
      }
      order(id: $orderId) {
        fulfillmentOrders(first: 10) {
          nodes { id status requestStatus }
        }
      }
    }
  "
  let assert Ok(downstream) =
    orders.process(
      accept_cancel_outcome.store,
      downstream_query,
      dict.from_list([
        #("orderId", root_field.StringVal(order_id)),
        #("submittedId", root_field.StringVal(partial_id)),
        #("unsubmittedId", root_field.StringVal(unsubmitted_id)),
      ]),
    )
  assert json.to_string(downstream)
    == "{\"data\":{\"submitted\":{\"id\":\"gid://shopify/FulfillmentOrder/partial\",\"requestStatus\":\"CANCELLATION_REJECTED\"},\"unsubmitted\":{\"id\":\"gid://shopify/FulfillmentOrder/3\",\"requestStatus\":\"UNSUBMITTED\"},\"assignedFulfillmentOrders\":{\"nodes\":[{\"id\":\"gid://shopify/FulfillmentOrder/partial\",\"status\":\"IN_PROGRESS\",\"requestStatus\":\"CANCELLATION_REJECTED\"},{\"id\":\"gid://shopify/FulfillmentOrder/3\",\"status\":\"OPEN\",\"requestStatus\":\"UNSUBMITTED\"},{\"id\":\"gid://shopify/FulfillmentOrder/reject\",\"status\":\"OPEN\",\"requestStatus\":\"REJECTED\"}]},\"order\":{\"fulfillmentOrders\":{\"nodes\":[{\"id\":\"gid://shopify/FulfillmentOrder/partial\",\"status\":\"IN_PROGRESS\",\"requestStatus\":\"CANCELLATION_REJECTED\"},{\"id\":\"gid://shopify/FulfillmentOrder/3\",\"status\":\"OPEN\",\"requestStatus\":\"UNSUBMITTED\"},{\"id\":\"gid://shopify/FulfillmentOrder/reject\",\"status\":\"OPEN\",\"requestStatus\":\"REJECTED\"},{\"id\":\"gid://shopify/FulfillmentOrder/cancel\",\"status\":\"CLOSED\",\"requestStatus\":\"CANCELLATION_ACCEPTED\"}]}}}}"
}

fn fulfillment_request_store(order_id: String) -> store.Store {
  let fulfillment_order = fn(suffix: String, title: String, quantity: Int) {
    types.CapturedObject([
      #("id", types.CapturedString("gid://shopify/FulfillmentOrder/" <> suffix)),
      #("status", types.CapturedString("OPEN")),
      #("requestStatus", types.CapturedString("UNSUBMITTED")),
      #(
        "assignedLocation",
        types.CapturedObject([
          #("name", types.CapturedString("HAR233 Local Service")),
          #("locationId", types.CapturedString("gid://shopify/Location/har233")),
        ]),
      ),
      #("merchantRequests", types.CapturedArray([])),
      #(
        "lineItems",
        types.CapturedArray([
          types.CapturedObject([
            #(
              "id",
              types.CapturedString(
                "gid://shopify/FulfillmentOrderLineItem/" <> suffix,
              ),
            ),
            #(
              "lineItemId",
              types.CapturedString("gid://shopify/LineItem/" <> suffix),
            ),
            #("title", types.CapturedString(title)),
            #("lineItemQuantity", types.CapturedInt(quantity)),
            #("lineItemFulfillableQuantity", types.CapturedInt(quantity)),
            #("totalQuantity", types.CapturedInt(quantity)),
            #("remainingQuantity", types.CapturedInt(quantity)),
          ]),
        ]),
      ),
    ])
  }
  store.new()
  |> store.upsert_base_orders([
    types.OrderRecord(
      id: order_id,
      cursor: None,
      data: types.CapturedObject([
        #("id", types.CapturedString(order_id)),
        #("name", types.CapturedString("#FO-REQUESTS")),
        #("displayFulfillmentStatus", types.CapturedString("UNFULFILLED")),
        #("fulfillments", types.CapturedArray([])),
        #(
          "fulfillmentOrders",
          types.CapturedArray([
            fulfillment_order("partial", "Partial request item", 2),
            fulfillment_order("reject", "Reject request item", 1),
            fulfillment_order("cancel", "Cancel request item", 1),
          ]),
        ),
      ]),
    ),
  ])
}

fn fulfillment_order_lifecycle_store(
  order_id: String,
  fulfillment_order_id: String,
  fulfillment_order_line_item_id: String,
  line_item_id: String,
) -> store.Store {
  store.new()
  |> store.upsert_base_orders([
    types.OrderRecord(
      id: order_id,
      cursor: None,
      data: types.CapturedObject([
        #("id", types.CapturedString(order_id)),
        #("name", types.CapturedString("#FULFILLMENT-ORDER")),
        #("displayFulfillmentStatus", types.CapturedString("UNFULFILLED")),
        #("fulfillments", types.CapturedArray([])),
        #(
          "fulfillmentOrders",
          types.CapturedArray([
            types.CapturedObject([
              #("id", types.CapturedString(fulfillment_order_id)),
              #("status", types.CapturedString("OPEN")),
              #("requestStatus", types.CapturedString("UNSUBMITTED")),
              #(
                "assignedLocation",
                types.CapturedObject([
                  #("name", types.CapturedString("My Custom Location")),
                  #(
                    "locationId",
                    types.CapturedString("gid://shopify/Location/source"),
                  ),
                ]),
              ),
              #("fulfillmentHolds", types.CapturedArray([])),
              #(
                "lineItems",
                types.CapturedArray([
                  types.CapturedObject([
                    #(
                      "id",
                      types.CapturedString(fulfillment_order_line_item_id),
                    ),
                    #("lineItemId", types.CapturedString(line_item_id)),
                    #(
                      "title",
                      types.CapturedString("Fulfillment order lifecycle item"),
                    ),
                    #("totalQuantity", types.CapturedInt(2)),
                    #("remainingQuantity", types.CapturedInt(2)),
                  ]),
                ]),
              ),
            ]),
          ]),
        ),
      ]),
    ),
  ])
}

pub fn orders_refund_create_over_refund_validation_keeps_order_unchanged_test() {
  let order_id = "gid://shopify/Order/6830465417449"
  let line_item_id = "gid://shopify/LineItem/16202166632681"
  let transaction_id = "gid://shopify/OrderTransaction/8194169077993"
  let seeded =
    store.new()
    |> store.upsert_base_orders([
      types.OrderRecord(
        id: order_id,
        cursor: None,
        data: types.CapturedObject([
          #("id", types.CapturedString(order_id)),
          #("name", types.CapturedString("#1320")),
          #("displayFinancialStatus", types.CapturedString("PAID")),
          #("displayFulfillmentStatus", types.CapturedString("UNFULFILLED")),
          #(
            "totalPriceSet",
            types.CapturedObject([
              #(
                "shopMoney",
                types.CapturedObject([
                  #("amount", types.CapturedString("15.0")),
                  #("currencyCode", types.CapturedString("CAD")),
                ]),
              ),
            ]),
          ),
          #(
            "totalRefundedSet",
            types.CapturedObject([
              #(
                "shopMoney",
                types.CapturedObject([
                  #("amount", types.CapturedString("0.0")),
                  #("currencyCode", types.CapturedString("CAD")),
                ]),
              ),
            ]),
          ),
          #(
            "totalReceivedSet",
            types.CapturedObject([
              #(
                "shopMoney",
                types.CapturedObject([
                  #("amount", types.CapturedString("15.0")),
                  #("currencyCode", types.CapturedString("CAD")),
                ]),
              ),
            ]),
          ),
          #(
            "shippingLines",
            types.CapturedObject([
              #(
                "nodes",
                types.CapturedArray([
                  types.CapturedObject([
                    #("title", types.CapturedString("Standard")),
                    #(
                      "originalPriceSet",
                      types.CapturedObject([
                        #(
                          "shopMoney",
                          types.CapturedObject([
                            #("amount", types.CapturedString("5.0")),
                            #("currencyCode", types.CapturedString("CAD")),
                          ]),
                        ),
                      ]),
                    ),
                  ]),
                ]),
              ),
            ]),
          ),
          #(
            "lineItems",
            types.CapturedObject([
              #(
                "nodes",
                types.CapturedArray([
                  types.CapturedObject([
                    #("id", types.CapturedString(line_item_id)),
                    #(
                      "title",
                      types.CapturedString("Hermes refundable over-refund item"),
                    ),
                    #("quantity", types.CapturedInt(1)),
                    #(
                      "originalUnitPriceSet",
                      types.CapturedObject([
                        #(
                          "shopMoney",
                          types.CapturedObject([
                            #("amount", types.CapturedString("10.0")),
                            #("currencyCode", types.CapturedString("CAD")),
                          ]),
                        ),
                      ]),
                    ),
                  ]),
                ]),
              ),
            ]),
          ),
          #(
            "transactions",
            types.CapturedArray([
              types.CapturedObject([
                #("id", types.CapturedString(transaction_id)),
                #("kind", types.CapturedString("SALE")),
                #("status", types.CapturedString("SUCCESS")),
                #("gateway", types.CapturedString("manual")),
                #(
                  "amountSet",
                  types.CapturedObject([
                    #(
                      "shopMoney",
                      types.CapturedObject([
                        #("amount", types.CapturedString("15.0")),
                        #("currencyCode", types.CapturedString("CAD")),
                      ]),
                    ),
                  ]),
                ),
              ]),
            ]),
          ),
          #("refunds", types.CapturedArray([])),
          #(
            "returns",
            types.CapturedObject([
              #("nodes", types.CapturedArray([])),
              #(
                "pageInfo",
                types.CapturedObject([
                  #("hasNextPage", types.CapturedBool(False)),
                  #("hasPreviousPage", types.CapturedBool(False)),
                  #("startCursor", types.CapturedNull),
                  #("endCursor", types.CapturedNull),
                ]),
              ),
            ]),
          ),
        ]),
      ),
    ])
  let mutation =
    "
    mutation RefundCreateParity($input: RefundInput!) {
      refundCreate(input: $input) {
        refund {
          id
        }
        order {
          id
          displayFinancialStatus
          totalRefundedSet {
            shopMoney {
              amount
              currencyCode
            }
          }
        }
        userErrors {
          field
          message
          code
        }
      }
    }
  "
  let variables =
    dict.from_list([
      #(
        "input",
        root_field.ObjectVal(
          dict.from_list([
            #("orderId", root_field.StringVal(order_id)),
            #("note", root_field.StringVal("invalid over refund")),
            #("notify", root_field.BoolVal(False)),
            #(
              "refundLineItems",
              root_field.ListVal([
                root_field.ObjectVal(
                  dict.from_list([
                    #("lineItemId", root_field.StringVal(line_item_id)),
                    #("quantity", root_field.IntVal(1)),
                    #("restockType", root_field.StringVal("NO_RESTOCK")),
                  ]),
                ),
              ]),
            ),
            #(
              "shipping",
              root_field.ObjectVal(
                dict.from_list([#("fullRefund", root_field.BoolVal(True))]),
              ),
            ),
            #(
              "transactions",
              root_field.ListVal([
                root_field.ObjectVal(
                  dict.from_list([
                    #("amount", root_field.StringVal("25.00")),
                    #("gateway", root_field.StringVal("manual")),
                    #("kind", root_field.StringVal("REFUND")),
                    #("orderId", root_field.StringVal(order_id)),
                    #("parentId", root_field.StringVal(transaction_id)),
                  ]),
                ),
              ]),
            ),
          ]),
        ),
      ),
    ])
  let outcome =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      mutation,
      variables,
      empty_upstream_context(),
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"refundCreate\":{\"refund\":null,\"order\":{\"id\":\"gid://shopify/Order/6830465417449\",\"displayFinancialStatus\":\"PAID\",\"totalRefundedSet\":{\"shopMoney\":{\"amount\":\"0.0\",\"currencyCode\":\"CAD\"}}},\"userErrors\":[{\"field\":[\"transactions\"],\"message\":\"Refund amount $25.00 is greater than net payment received $15.00\",\"code\":\"INVALID\"}]}}}"
  assert outcome.staged_resource_ids == []
  assert list.length(outcome.log_drafts) == 1

  let read_query =
    "
    query RefundCreateDownstreamRead($id: ID!) {
      order(id: $id) {
        id
        displayFinancialStatus
        displayFulfillmentStatus
        refunds {
          id
        }
        returns(first: 5) {
          nodes {
            id
          }
          pageInfo {
            hasNextPage
            hasPreviousPage
            startCursor
            endCursor
          }
        }
        transactions {
          id
          kind
          status
          gateway
          amountSet {
            shopMoney {
              amount
              currencyCode
            }
          }
        }
        totalRefundedSet {
          shopMoney {
            amount
            currencyCode
          }
        }
      }
    }
  "
  let read_variables = dict.from_list([#("id", root_field.StringVal(order_id))])
  let assert Ok(read) =
    orders.process(outcome.store, read_query, read_variables)
  assert json.to_string(read)
    == "{\"data\":{\"order\":{\"id\":\"gid://shopify/Order/6830465417449\",\"displayFinancialStatus\":\"PAID\",\"displayFulfillmentStatus\":\"UNFULFILLED\",\"refunds\":[],\"returns\":{\"nodes\":[],\"pageInfo\":{\"hasNextPage\":false,\"hasPreviousPage\":false,\"startCursor\":null,\"endCursor\":null}},\"transactions\":[{\"id\":\"gid://shopify/OrderTransaction/8194169077993\",\"kind\":\"SALE\",\"status\":\"SUCCESS\",\"gateway\":\"manual\",\"amountSet\":{\"shopMoney\":{\"amount\":\"15.0\",\"currencyCode\":\"CAD\"}}}],\"totalRefundedSet\":{\"shopMoney\":{\"amount\":\"0.0\",\"currencyCode\":\"CAD\"}}}}}"
}

pub fn orders_refund_create_unknown_order_uses_order_id_user_error_path_test() {
  let mutation =
    "
    mutation RefundCreateUnknownOrder($input: RefundInput!) {
      refundCreate(input: $input) {
        refund {
          id
        }
        order {
          id
        }
        userErrors {
          field
          message
          code
        }
      }
    }
  "
  let variables =
    dict.from_list([
      #(
        "input",
        root_field.ObjectVal(
          dict.from_list([
            #(
              "orderId",
              root_field.StringVal("gid://shopify/Order/9999999999999"),
            ),
          ]),
        ),
      ),
    ])
  let outcome =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      mutation,
      variables,
      empty_upstream_context(),
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"refundCreate\":{\"refund\":null,\"order\":null,\"userErrors\":[{\"field\":[\"orderId\"],\"message\":\"Order does not exist\",\"code\":\"NOT_FOUND\"}]}}}"
  assert outcome.staged_resource_ids == []
}

pub fn orders_refund_create_line_item_quantity_validation_uses_refundable_quantity_test() {
  let order_id = "gid://shopify/Order/6830465417550"
  let line_item_id = "gid://shopify/LineItem/16202166637700"
  let seeded =
    store.new()
    |> store.upsert_base_orders([
      types.OrderRecord(
        id: order_id,
        cursor: None,
        data: types.CapturedObject([
          #("id", types.CapturedString(order_id)),
          #(
            "displayFinancialStatus",
            types.CapturedString("PARTIALLY_REFUNDED"),
          ),
          #(
            "totalPriceSet",
            types.CapturedObject([
              #(
                "shopMoney",
                types.CapturedObject([
                  #("amount", types.CapturedString("50.0")),
                  #("currencyCode", types.CapturedString("CAD")),
                ]),
              ),
            ]),
          ),
          #(
            "totalReceivedSet",
            types.CapturedObject([
              #(
                "shopMoney",
                types.CapturedObject([
                  #("amount", types.CapturedString("50.0")),
                  #("currencyCode", types.CapturedString("CAD")),
                ]),
              ),
            ]),
          ),
          #(
            "totalRefundedSet",
            types.CapturedObject([
              #(
                "shopMoney",
                types.CapturedObject([
                  #("amount", types.CapturedString("10.0")),
                  #("currencyCode", types.CapturedString("CAD")),
                ]),
              ),
            ]),
          ),
          #(
            "lineItems",
            types.CapturedObject([
              #(
                "nodes",
                types.CapturedArray([
                  types.CapturedObject([
                    #("id", types.CapturedString(line_item_id)),
                    #("title", types.CapturedString("Partially refunded item")),
                    #("quantity", types.CapturedInt(2)),
                    #("currentQuantity", types.CapturedInt(2)),
                    #(
                      "originalUnitPriceSet",
                      types.CapturedObject([
                        #(
                          "shopMoney",
                          types.CapturedObject([
                            #("amount", types.CapturedString("10.0")),
                            #("currencyCode", types.CapturedString("CAD")),
                          ]),
                        ),
                      ]),
                    ),
                  ]),
                ]),
              ),
            ]),
          ),
          #(
            "transactions",
            types.CapturedArray([
              types.CapturedObject([
                #(
                  "id",
                  types.CapturedString("gid://shopify/OrderTransaction/sale"),
                ),
                #("kind", types.CapturedString("SALE")),
                #("status", types.CapturedString("SUCCESS")),
                #("gateway", types.CapturedString("manual")),
                #(
                  "amountSet",
                  types.CapturedObject([
                    #(
                      "shopMoney",
                      types.CapturedObject([
                        #("amount", types.CapturedString("50.0")),
                        #("currencyCode", types.CapturedString("CAD")),
                      ]),
                    ),
                  ]),
                ),
              ]),
            ]),
          ),
          #(
            "refunds",
            types.CapturedArray([
              types.CapturedObject([
                #("id", types.CapturedString("gid://shopify/Refund/previous")),
                #(
                  "totalRefundedSet",
                  types.CapturedObject([
                    #(
                      "shopMoney",
                      types.CapturedObject([
                        #("amount", types.CapturedString("10.0")),
                        #("currencyCode", types.CapturedString("CAD")),
                      ]),
                    ),
                  ]),
                ),
                #(
                  "refundLineItems",
                  types.CapturedObject([
                    #(
                      "nodes",
                      types.CapturedArray([
                        types.CapturedObject([
                          #("quantity", types.CapturedInt(1)),
                          #(
                            "lineItem",
                            types.CapturedObject([
                              #("id", types.CapturedString(line_item_id)),
                            ]),
                          ),
                        ]),
                      ]),
                    ),
                  ]),
                ),
              ]),
            ]),
          ),
        ]),
      ),
    ])
  let mutation =
    "
    mutation RefundCreateOverQuantity($input: RefundInput!) {
      refundCreate(input: $input) {
        refund {
          id
        }
        order {
          id
          totalRefundedSet {
            shopMoney {
              amount
              currencyCode
            }
          }
        }
        userErrors {
          field
          message
          code
        }
      }
    }
  "
  let variables =
    dict.from_list([
      #(
        "input",
        root_field.ObjectVal(
          dict.from_list([
            #("orderId", root_field.StringVal(order_id)),
            #("allowOverRefunding", root_field.BoolVal(True)),
            #(
              "refundLineItems",
              root_field.ListVal([
                root_field.ObjectVal(
                  dict.from_list([
                    #("lineItemId", root_field.StringVal(line_item_id)),
                    #("quantity", root_field.IntVal(2)),
                    #("restockType", root_field.StringVal("RETURN")),
                  ]),
                ),
              ]),
            ),
          ]),
        ),
      ),
    ])
  let outcome =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      mutation,
      variables,
      empty_upstream_context(),
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"refundCreate\":{\"refund\":null,\"order\":{\"id\":\"gid://shopify/Order/6830465417550\",\"totalRefundedSet\":{\"shopMoney\":{\"amount\":\"10.0\",\"currencyCode\":\"CAD\"}}},\"userErrors\":[{\"field\":[\"refundLineItems\",\"0\",\"quantity\"],\"message\":\"Quantity cannot refund more items than were purchased\",\"code\":\"INVALID\"}]}}}"
  assert outcome.staged_resource_ids == []
}

pub fn orders_refund_create_allow_over_refunding_stages_amount_over_refund_test() {
  let order_id = "gid://shopify/Order/6830465417660"
  let transaction_id = "gid://shopify/OrderTransaction/8194169077660"
  let seeded =
    store.new()
    |> store.upsert_base_orders([
      types.OrderRecord(
        id: order_id,
        cursor: None,
        data: types.CapturedObject([
          #("id", types.CapturedString(order_id)),
          #("displayFinancialStatus", types.CapturedString("PAID")),
          #(
            "totalPriceSet",
            types.CapturedObject([
              #(
                "shopMoney",
                types.CapturedObject([
                  #("amount", types.CapturedString("15.0")),
                  #("currencyCode", types.CapturedString("CAD")),
                ]),
              ),
            ]),
          ),
          #(
            "totalReceivedSet",
            types.CapturedObject([
              #(
                "shopMoney",
                types.CapturedObject([
                  #("amount", types.CapturedString("15.0")),
                  #("currencyCode", types.CapturedString("CAD")),
                ]),
              ),
            ]),
          ),
          #(
            "totalRefundedSet",
            types.CapturedObject([
              #(
                "shopMoney",
                types.CapturedObject([
                  #("amount", types.CapturedString("0.0")),
                  #("currencyCode", types.CapturedString("CAD")),
                ]),
              ),
            ]),
          ),
          #(
            "transactions",
            types.CapturedArray([
              types.CapturedObject([
                #("id", types.CapturedString(transaction_id)),
                #("kind", types.CapturedString("SALE")),
                #("status", types.CapturedString("SUCCESS")),
                #("gateway", types.CapturedString("manual")),
                #(
                  "amountSet",
                  types.CapturedObject([
                    #(
                      "shopMoney",
                      types.CapturedObject([
                        #("amount", types.CapturedString("15.0")),
                        #("currencyCode", types.CapturedString("CAD")),
                      ]),
                    ),
                  ]),
                ),
              ]),
            ]),
          ),
          #("refunds", types.CapturedArray([])),
        ]),
      ),
    ])
  let mutation =
    "
    mutation RefundCreateAllowOverRefunding($input: RefundInput!) {
      refundCreate(input: $input) {
        refund {
          id
          totalRefundedSet {
            shopMoney {
              amount
              currencyCode
            }
            presentmentMoney {
              amount
              currencyCode
            }
          }
        }
        order {
          id
          totalRefundedSet {
            shopMoney {
              amount
              currencyCode
            }
            presentmentMoney {
              amount
              currencyCode
            }
          }
        }
        userErrors {
          field
          message
          code
        }
      }
    }
  "
  let variables =
    dict.from_list([
      #(
        "input",
        root_field.ObjectVal(
          dict.from_list([
            #("orderId", root_field.StringVal(order_id)),
            #("allowOverRefunding", root_field.BoolVal(True)),
            #(
              "transactions",
              root_field.ListVal([
                root_field.ObjectVal(
                  dict.from_list([
                    #("amount", root_field.StringVal("25.00")),
                    #("gateway", root_field.StringVal("manual")),
                    #("kind", root_field.StringVal("REFUND")),
                    #("orderId", root_field.StringVal(order_id)),
                    #("parentId", root_field.StringVal(transaction_id)),
                  ]),
                ),
              ]),
            ),
          ]),
        ),
      ),
    ])
  let outcome =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      mutation,
      variables,
      empty_upstream_context(),
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"refundCreate\":{\"refund\":{\"id\":\"gid://shopify/Refund/1\",\"totalRefundedSet\":{\"shopMoney\":{\"amount\":\"25.0\",\"currencyCode\":\"CAD\"},\"presentmentMoney\":{\"amount\":\"25.0\",\"currencyCode\":\"CAD\"}}},\"order\":{\"id\":\"gid://shopify/Order/6830465417660\",\"totalRefundedSet\":{\"shopMoney\":{\"amount\":\"25.0\",\"currencyCode\":\"CAD\"},\"presentmentMoney\":{\"amount\":\"25.0\",\"currencyCode\":\"CAD\"}}},\"userErrors\":[]}}}"
  assert outcome.staged_resource_ids == [order_id]
}

pub fn orders_refund_create_partial_success_stages_refund_and_transaction_test() {
  let order_id = "gid://shopify/Order/6830465188073"
  let line_item_id = "gid://shopify/LineItem/16202166272233"
  let transaction_id = "gid://shopify/OrderTransaction/8194168750313"
  let seeded =
    store.new()
    |> store.upsert_base_orders([
      types.OrderRecord(
        id: order_id,
        cursor: None,
        data: types.CapturedObject([
          #("id", types.CapturedString(order_id)),
          #("name", types.CapturedString("#1318")),
          #("displayFinancialStatus", types.CapturedString("PAID")),
          #("displayFulfillmentStatus", types.CapturedString("UNFULFILLED")),
          #(
            "totalPriceSet",
            types.CapturedObject([
              #(
                "shopMoney",
                types.CapturedObject([
                  #("amount", types.CapturedString("25.0")),
                  #("currencyCode", types.CapturedString("CAD")),
                ]),
              ),
            ]),
          ),
          #(
            "totalRefundedSet",
            types.CapturedObject([
              #(
                "shopMoney",
                types.CapturedObject([
                  #("amount", types.CapturedString("0.0")),
                  #("currencyCode", types.CapturedString("CAD")),
                ]),
              ),
            ]),
          ),
          #(
            "shippingLines",
            types.CapturedObject([
              #(
                "nodes",
                types.CapturedArray([
                  types.CapturedObject([
                    #("title", types.CapturedString("Standard")),
                    #(
                      "originalPriceSet",
                      types.CapturedObject([
                        #(
                          "shopMoney",
                          types.CapturedObject([
                            #("amount", types.CapturedString("5.0")),
                            #("currencyCode", types.CapturedString("CAD")),
                          ]),
                        ),
                      ]),
                    ),
                  ]),
                ]),
              ),
            ]),
          ),
          #(
            "lineItems",
            types.CapturedObject([
              #(
                "nodes",
                types.CapturedArray([
                  types.CapturedObject([
                    #("id", types.CapturedString(line_item_id)),
                    #(
                      "title",
                      types.CapturedString(
                        "Hermes refundable partial-shipping-restock item",
                      ),
                    ),
                    #("quantity", types.CapturedInt(2)),
                    #(
                      "originalUnitPriceSet",
                      types.CapturedObject([
                        #(
                          "shopMoney",
                          types.CapturedObject([
                            #("amount", types.CapturedString("10.0")),
                            #("currencyCode", types.CapturedString("CAD")),
                          ]),
                        ),
                      ]),
                    ),
                  ]),
                ]),
              ),
            ]),
          ),
          #(
            "transactions",
            types.CapturedArray([
              types.CapturedObject([
                #("id", types.CapturedString(transaction_id)),
                #("kind", types.CapturedString("SALE")),
                #("status", types.CapturedString("SUCCESS")),
                #("gateway", types.CapturedString("manual")),
                #(
                  "amountSet",
                  types.CapturedObject([
                    #(
                      "shopMoney",
                      types.CapturedObject([
                        #("amount", types.CapturedString("25.0")),
                        #("currencyCode", types.CapturedString("CAD")),
                      ]),
                    ),
                  ]),
                ),
              ]),
            ]),
          ),
          #("refunds", types.CapturedArray([])),
          #(
            "returns",
            types.CapturedObject([
              #("nodes", types.CapturedArray([])),
              #(
                "pageInfo",
                types.CapturedObject([
                  #("hasNextPage", types.CapturedBool(False)),
                  #("hasPreviousPage", types.CapturedBool(False)),
                  #("startCursor", types.CapturedNull),
                  #("endCursor", types.CapturedNull),
                ]),
              ),
            ]),
          ),
        ]),
      ),
    ])
  let mutation =
    "
    mutation RefundCreateParity($input: RefundInput!) {
      refundCreate(input: $input) {
        refund {
          id
          note
          createdAt
          updatedAt
          totalRefundedSet {
            shopMoney {
              amount
              currencyCode
            }
          }
          refundLineItems(first: 5) {
            nodes {
              id
              quantity
              restockType
              lineItem {
                id
                title
              }
              subtotalSet {
                shopMoney {
                  amount
                  currencyCode
                }
              }
            }
          }
          transactions(first: 5) {
            nodes {
              id
              kind
              status
              gateway
              amountSet {
                shopMoney {
                  amount
                  currencyCode
                }
              }
            }
          }
        }
        order {
          id
          displayFinancialStatus
          totalRefundedSet {
            shopMoney {
              amount
              currencyCode
            }
          }
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  let variables =
    dict.from_list([
      #(
        "input",
        root_field.ObjectVal(
          dict.from_list([
            #("orderId", root_field.StringVal(order_id)),
            #(
              "note",
              root_field.StringVal("partial line item and shipping refund"),
            ),
            #("notify", root_field.BoolVal(False)),
            #(
              "refundLineItems",
              root_field.ListVal([
                root_field.ObjectVal(
                  dict.from_list([
                    #("lineItemId", root_field.StringVal(line_item_id)),
                    #("quantity", root_field.IntVal(1)),
                    #("restockType", root_field.StringVal("RETURN")),
                    #(
                      "locationId",
                      root_field.StringVal("gid://shopify/Location/68509171945"),
                    ),
                  ]),
                ),
              ]),
            ),
            #(
              "shipping",
              root_field.ObjectVal(
                dict.from_list([#("amount", root_field.StringVal("5.00"))]),
              ),
            ),
            #(
              "transactions",
              root_field.ListVal([
                root_field.ObjectVal(
                  dict.from_list([
                    #("amount", root_field.StringVal("15.00")),
                    #("gateway", root_field.StringVal("manual")),
                    #("kind", root_field.StringVal("REFUND")),
                    #("orderId", root_field.StringVal(order_id)),
                    #("parentId", root_field.StringVal(transaction_id)),
                  ]),
                ),
              ]),
            ),
          ]),
        ),
      ),
    ])
  let outcome =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      mutation,
      variables,
      empty_upstream_context(),
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"refundCreate\":{\"refund\":{\"id\":\"gid://shopify/Refund/1\",\"note\":\"partial line item and shipping refund\",\"createdAt\":\"2024-01-01T00:00:00.000Z\",\"updatedAt\":\"2024-01-01T00:00:00.000Z\",\"totalRefundedSet\":{\"shopMoney\":{\"amount\":\"15.0\",\"currencyCode\":\"CAD\"}},\"refundLineItems\":{\"nodes\":[{\"id\":\"gid://shopify/RefundLineItem/2\",\"quantity\":1,\"restockType\":\"RETURN\",\"lineItem\":{\"id\":\"gid://shopify/LineItem/16202166272233\",\"title\":\"Hermes refundable partial-shipping-restock item\"},\"subtotalSet\":{\"shopMoney\":{\"amount\":\"10.0\",\"currencyCode\":\"CAD\"}}}]},\"transactions\":{\"nodes\":[{\"id\":\"gid://shopify/OrderTransaction/3\",\"kind\":\"REFUND\",\"status\":\"SUCCESS\",\"gateway\":\"manual\",\"amountSet\":{\"shopMoney\":{\"amount\":\"15.0\",\"currencyCode\":\"CAD\"}}}]}},\"order\":{\"id\":\"gid://shopify/Order/6830465188073\",\"displayFinancialStatus\":\"PARTIALLY_REFUNDED\",\"totalRefundedSet\":{\"shopMoney\":{\"amount\":\"15.0\",\"currencyCode\":\"CAD\"}}},\"userErrors\":[]}}}"
  assert outcome.staged_resource_ids == [order_id]
  assert list.length(outcome.log_drafts) == 1

  let read_query =
    "
    query RefundCreateDownstreamRead($id: ID!) {
      order(id: $id) {
        id
        displayFinancialStatus
        refunds {
          id
          note
          totalRefundedSet {
            shopMoney {
              amount
              currencyCode
            }
          }
        }
        returns(first: 5) {
          nodes {
            id
          }
          pageInfo {
            hasNextPage
            hasPreviousPage
            startCursor
            endCursor
          }
        }
        transactions {
          id
          kind
          status
          gateway
          amountSet {
            shopMoney {
              amount
              currencyCode
            }
          }
        }
        totalRefundedSet {
          shopMoney {
            amount
            currencyCode
          }
        }
      }
    }
  "
  let read_variables = dict.from_list([#("id", root_field.StringVal(order_id))])
  let assert Ok(read) =
    orders.process(outcome.store, read_query, read_variables)
  assert json.to_string(read)
    == "{\"data\":{\"order\":{\"id\":\"gid://shopify/Order/6830465188073\",\"displayFinancialStatus\":\"PARTIALLY_REFUNDED\",\"refunds\":[{\"id\":\"gid://shopify/Refund/1\",\"note\":\"partial line item and shipping refund\",\"totalRefundedSet\":{\"shopMoney\":{\"amount\":\"15.0\",\"currencyCode\":\"CAD\"}}}],\"returns\":{\"nodes\":[],\"pageInfo\":{\"hasNextPage\":false,\"hasPreviousPage\":false,\"startCursor\":null,\"endCursor\":null}},\"transactions\":[{\"id\":\"gid://shopify/OrderTransaction/8194168750313\",\"kind\":\"SALE\",\"status\":\"SUCCESS\",\"gateway\":\"manual\",\"amountSet\":{\"shopMoney\":{\"amount\":\"25.0\",\"currencyCode\":\"CAD\"}}},{\"id\":\"gid://shopify/OrderTransaction/3\",\"kind\":\"REFUND\",\"status\":\"SUCCESS\",\"gateway\":\"manual\",\"amountSet\":{\"shopMoney\":{\"amount\":\"15.0\",\"currencyCode\":\"CAD\"}}}],\"totalRefundedSet\":{\"shopMoney\":{\"amount\":\"15.0\",\"currencyCode\":\"CAD\"}}}}}"
}

pub fn orders_draft_order_delete_read_after_write_test() {
  let draft_order_id = "gid://shopify/DraftOrder/10079785100"
  let seeded =
    store.new()
    |> store.stage_draft_order(types.DraftOrderRecord(
      id: draft_order_id,
      cursor: None,
      data: types.CapturedObject([
        #("id", types.CapturedString(draft_order_id)),
        #("name", types.CapturedString("#D1")),
      ]),
    ))
  let mutation =
    "
    mutation DraftOrderDeleteParityPlan($input: DraftOrderDeleteInput!) {
      draftOrderDelete(input: $input) {
        deletedId
        userErrors {
          field
          message
        }
      }
    }
  "
  let variables =
    dict.from_list([
      #(
        "input",
        root_field.ObjectVal(
          dict.from_list([#("id", root_field.StringVal(draft_order_id))]),
        ),
      ),
    ])
  let outcome =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      mutation,
      variables,
      empty_upstream_context(),
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"draftOrderDelete\":{\"deletedId\":\"gid://shopify/DraftOrder/10079785100\",\"userErrors\":[]}}}"
  assert store.get_draft_order_by_id(outcome.store, draft_order_id) == None
}

pub fn orders_order_delete_tombstone_read_after_write_test() {
  let order_id = "gid://shopify/Order/order-delete"
  let seeded =
    store.new()
    |> store.upsert_base_orders([
      types.OrderRecord(
        id: order_id,
        cursor: Some("cursor-order-delete"),
        data: types.CapturedObject([
          #("id", types.CapturedString(order_id)),
          #("name", types.CapturedString("#DELETE")),
          #("displayFinancialStatus", types.CapturedString("PENDING")),
          #("displayFulfillmentStatus", types.CapturedString("UNFULFILLED")),
        ]),
      ),
    ])
  let mutation =
    "
    mutation OrderDelete($orderId: ID!) {
      orderDelete(orderId: $orderId) {
        deletedId
        userErrors {
          field
          message
          code
        }
      }
    }
  "
  let variables = dict.from_list([#("orderId", root_field.StringVal(order_id))])
  let outcome =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      mutation,
      variables,
      empty_upstream_context(),
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"orderDelete\":{\"deletedId\":\"gid://shopify/Order/order-delete\",\"userErrors\":[]}}}"
  assert store.get_order_by_id(outcome.store, order_id) == None

  let read =
    "
    query OrderDeleteRead($id: ID!) {
      order(id: $id) {
        id
      }
      orders(first: 5) {
        nodes {
          id
        }
      }
      ordersCount {
        count
        precision
      }
    }
  "
  let read_variables = dict.from_list([#("id", root_field.StringVal(order_id))])
  let assert Ok(read_result) =
    orders.process(outcome.store, read, read_variables)
  assert json.to_string(read_result)
    == "{\"data\":{\"order\":null,\"orders\":{\"nodes\":[]},\"ordersCount\":{\"count\":0,\"precision\":\"EXACT\"}}}"

  let repeated =
    orders.process_mutation(
      outcome.store,
      outcome.identity,
      "/admin/api/2026-04/graphql.json",
      mutation,
      variables,
      empty_upstream_context(),
    )
  assert json.to_string(repeated.data)
    == "{\"data\":{\"orderDelete\":{\"deletedId\":null,\"userErrors\":[{\"field\":[\"orderId\"],\"message\":\"Order does not exist\",\"code\":\"NOT_FOUND\"}]}}}"
  assert repeated.staged_resource_ids == []
}

pub fn orders_order_delete_rejects_non_deletable_paid_orders_test() {
  let order_id = "gid://shopify/Order/order-delete-paid"
  let seeded =
    store.new()
    |> store.upsert_base_orders([
      types.OrderRecord(
        id: order_id,
        cursor: Some("cursor-order-delete-paid"),
        data: types.CapturedObject([
          #("id", types.CapturedString(order_id)),
          #("name", types.CapturedString("#DELETE-PAID")),
          #("displayFinancialStatus", types.CapturedString("PAID")),
          #("displayFulfillmentStatus", types.CapturedString("UNFULFILLED")),
          #("transactions", types.CapturedArray([])),
          #("refunds", types.CapturedArray([])),
          #("fulfillments", types.CapturedArray([])),
          #("fulfillmentOrders", types.CapturedArray([])),
          #("returns", types.CapturedArray([])),
        ]),
      ),
    ])
  let mutation =
    "
    mutation OrderDelete($orderId: ID!) {
      orderDelete(orderId: $orderId) {
        deletedId
        userErrors {
          field
          message
          code
        }
      }
    }
  "
  let variables = dict.from_list([#("orderId", root_field.StringVal(order_id))])
  let outcome =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      mutation,
      variables,
      empty_upstream_context(),
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"orderDelete\":{\"deletedId\":null,\"userErrors\":[{\"field\":[\"orderId\"],\"message\":\"order_cannot_be_deleted\",\"code\":\"INVALID\"}]}}}"
  assert store.get_order_by_id(outcome.store, order_id)
    == Some(types.OrderRecord(
      id: order_id,
      cursor: Some("cursor-order-delete-paid"),
      data: types.CapturedObject([
        #("id", types.CapturedString(order_id)),
        #("name", types.CapturedString("#DELETE-PAID")),
        #("displayFinancialStatus", types.CapturedString("PAID")),
        #("displayFulfillmentStatus", types.CapturedString("UNFULFILLED")),
        #("transactions", types.CapturedArray([])),
        #("refunds", types.CapturedArray([])),
        #("fulfillments", types.CapturedArray([])),
        #("fulfillmentOrders", types.CapturedArray([])),
        #("returns", types.CapturedArray([])),
      ]),
    ))
  assert outcome.staged_resource_ids == []
}

pub fn orders_order_delete_cascades_local_child_graph_test() {
  let order_id = "gid://shopify/Order/order-delete-cascade"
  let fulfillment_order_id =
    "gid://shopify/FulfillmentOrder/order-delete-cascade"
  let return_id = "gid://shopify/Return/order-delete-cascade"
  let payment_terms_id = "gid://shopify/PaymentTerms/order-delete-cascade"
  let checkout_id = "gid://shopify/AbandonedCheckout/order-delete-cascade"
  let seeded =
    store.new()
    |> store.upsert_base_orders([
      types.OrderRecord(
        id: order_id,
        cursor: Some("cursor-order-delete-cascade"),
        data: types.CapturedObject([
          #("id", types.CapturedString(order_id)),
          #("name", types.CapturedString("#DELETE-CASCADE")),
          #("displayFinancialStatus", types.CapturedString("PENDING")),
          #("displayFulfillmentStatus", types.CapturedString("UNFULFILLED")),
          #("transactions", types.CapturedArray([])),
          #("refunds", types.CapturedArray([])),
          #("fulfillments", types.CapturedArray([])),
          #(
            "fulfillmentOrders",
            types.CapturedArray([
              types.CapturedObject([
                #("id", types.CapturedString(fulfillment_order_id)),
                #("status", types.CapturedString("CLOSED")),
              ]),
            ]),
          ),
          #(
            "returns",
            types.CapturedArray([
              types.CapturedObject([
                #("id", types.CapturedString(return_id)),
                #("name", types.CapturedString("#R1")),
                #("status", types.CapturedString("CLOSED")),
              ]),
            ]),
          ),
        ]),
      ),
    ])
    |> store.upsert_base_abandoned_checkouts([
      types.AbandonedCheckoutRecord(
        id: checkout_id,
        cursor: Some("cursor-checkout-delete-cascade"),
        data: types.CapturedObject([
          #("id", types.CapturedString(checkout_id)),
          #("createdAt", types.CapturedString("2026-05-06T00:00:00Z")),
          #("orderId", types.CapturedString(order_id)),
          #(
            "order",
            types.CapturedObject([#("id", types.CapturedString(order_id))]),
          ),
        ]),
      ),
    ])
    |> store.upsert_base_payment_terms(
      types.PaymentTermsRecord(
        id: payment_terms_id,
        owner_id: order_id,
        due: True,
        overdue: False,
        due_in_days: Some(30),
        payment_terms_name: "Net 30",
        payment_terms_type: "NET",
        translated_name: "Net 30",
        payment_schedules: [],
      ),
    )
  let mutation =
    "
    mutation OrderDelete($orderId: ID!) {
      orderDelete(orderId: $orderId) {
        deletedId
        userErrors {
          field
          message
          code
        }
      }
    }
  "
  let variables = dict.from_list([#("orderId", root_field.StringVal(order_id))])
  let outcome =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      mutation,
      variables,
      empty_upstream_context(),
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"orderDelete\":{\"deletedId\":\"gid://shopify/Order/order-delete-cascade\",\"userErrors\":[]}}}"
  assert store.get_order_by_id(outcome.store, order_id) == None
  assert store.get_effective_payment_terms_by_owner_id(outcome.store, order_id)
    == None

  let read =
    "
    query OrderDeleteCascadeRead($orderId: ID!, $fulfillmentOrderId: ID!, $returnId: ID!) {
      order(id: $orderId) { id }
      fulfillmentOrder(id: $fulfillmentOrderId) { id }
      return(id: $returnId) { id }
      abandonedCheckouts(first: 5) {
        nodes {
          id
          orderId
          order { id }
        }
      }
    }
  "
  let read_variables =
    dict.from_list([
      #("orderId", root_field.StringVal(order_id)),
      #("fulfillmentOrderId", root_field.StringVal(fulfillment_order_id)),
      #("returnId", root_field.StringVal(return_id)),
    ])
  let assert Ok(read_result) =
    orders.process(outcome.store, read, read_variables)
  assert json.to_string(read_result)
    == "{\"data\":{\"order\":null,\"fulfillmentOrder\":null,\"return\":null,\"abandonedCheckouts\":{\"nodes\":[{\"id\":\"gid://shopify/AbandonedCheckout/order-delete-cascade\",\"orderId\":null,\"order\":null}]}}}"
}

pub fn orders_order_create_validation_guardrails_test() {
  let missing_order =
    "
    mutation InlineMissingOrderArg {
      orderCreate {
        order {
          id
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  let missing_outcome =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      missing_order,
      dict.new(),
      empty_upstream_context(),
    )
  assert json.to_string(missing_outcome.data)
    == "{\"errors\":[{\"message\":\"Field 'orderCreate' is missing required arguments: order\",\"locations\":[{\"line\":3,\"column\":7}],\"path\":[\"mutation InlineMissingOrderArg\",\"orderCreate\"],\"extensions\":{\"code\":\"missingRequiredArguments\",\"className\":\"Field\",\"name\":\"orderCreate\",\"arguments\":\"order\"}}]}"

  let null_order =
    "
    mutation InlineNullOrderArg {
      orderCreate(order: null) {
        order {
          id
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  let null_outcome =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      null_order,
      dict.new(),
      empty_upstream_context(),
    )
  assert json.to_string(null_outcome.data)
    == "{\"errors\":[{\"message\":\"Argument 'order' on Field 'orderCreate' has an invalid value (null). Expected type 'OrderCreateOrderInput!'.\",\"locations\":[{\"line\":3,\"column\":7}],\"path\":[\"mutation InlineNullOrderArg\",\"orderCreate\",\"order\"],\"extensions\":{\"code\":\"argumentLiteralsIncompatible\",\"typeName\":\"Field\",\"argumentName\":\"order\"}}]}"

  let no_line_items =
    "
    mutation OrderCreateValidationMatrix($order: OrderCreateOrderInput!) {
      orderCreate(order: $order) {
        order {
          id
        }
        userErrors {
          field
          message
          code
        }
      }
    }
  "
  let variables =
    dict.from_list([
      #(
        "order",
        root_field.ObjectVal(
          dict.from_list([
            #(
              "email",
              root_field.StringVal("hermes-order-no-line-items@example.com"),
            ),
            #("lineItems", root_field.ListVal([])),
          ]),
        ),
      ),
    ])
  let no_line_items_outcome =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      no_line_items,
      variables,
      empty_upstream_context(),
    )
  assert json.to_string(no_line_items_outcome.data)
    == "{\"data\":{\"orderCreate\":{\"order\":null,\"userErrors\":[{\"field\":[\"order\",\"lineItems\"],\"message\":\"Line items must have at least one line item\",\"code\":\"INVALID\"}]}}}"
  assert no_line_items_outcome.staged_resource_ids == []
  assert no_line_items_outcome.log_drafts == []
  assert store.list_effective_orders(no_line_items_outcome.store) == []

  let extended =
    "
    mutation OrderCreateValidationMatrixExtended(
      $futureProcessedAt: OrderCreateOrderInput!
      $redundantCustomer: OrderCreateOrderInput!
      $lineItemTaxLineMissingRate: OrderCreateOrderInput!
      $shippingLineTaxLineMissingRate: OrderCreateOrderInput!
    ) {
      futureProcessedAt: orderCreate(order: $futureProcessedAt) {
        order { id }
        userErrors { field message code }
      }
      redundantCustomer: orderCreate(order: $redundantCustomer) {
        order { id }
        userErrors { field message code }
      }
      lineItemTaxLineMissingRate: orderCreate(order: $lineItemTaxLineMissingRate) {
        order { id }
        userErrors { field message code }
      }
      shippingLineTaxLineMissingRate: orderCreate(order: $shippingLineTaxLineMissingRate) {
        order { id }
        userErrors { field message code }
      }
    }
  "
  let extended_variables =
    dict.from_list([
      #(
        "futureProcessedAt",
        order_create_test_order([
          #("processedAt", root_field.StringVal("2099-01-01T00:00:00Z")),
        ]),
      ),
      #(
        "redundantCustomer",
        order_create_test_order([
          #("customerId", root_field.StringVal("gid://shopify/Customer/1")),
          #(
            "customer",
            root_field.ObjectVal(
              dict.from_list([
                #(
                  "toUpsert",
                  root_field.ObjectVal(
                    dict.from_list([
                      #("email", root_field.StringVal("redundant@example.com")),
                    ]),
                  ),
                ),
              ]),
            ),
          ),
        ]),
      ),
      #(
        "lineItemTaxLineMissingRate",
        order_create_test_order([
          #(
            "lineItems",
            root_field.ListVal([
              order_create_test_line_item([
                #(
                  "taxLines",
                  root_field.ListVal([
                    root_field.ObjectVal(
                      dict.from_list([
                        #("priceSet", order_create_test_money("1.00")),
                      ]),
                    ),
                  ]),
                ),
              ]),
            ]),
          ),
        ]),
      ),
      #(
        "shippingLineTaxLineMissingRate",
        order_create_test_order([
          #(
            "shippingLines",
            root_field.ListVal([
              root_field.ObjectVal(
                dict.from_list([
                  #("title", root_field.StringVal("Standard")),
                  #("priceSet", order_create_test_money("5.00")),
                  #(
                    "taxLines",
                    root_field.ListVal([
                      root_field.ObjectVal(
                        dict.from_list([
                          #("priceSet", order_create_test_money("1.00")),
                        ]),
                      ),
                    ]),
                  ),
                ]),
              ),
            ]),
          ),
        ]),
      ),
    ])
  let extended_outcome =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      extended,
      extended_variables,
      empty_upstream_context(),
    )
  assert json.to_string(extended_outcome.data)
    == "{\"data\":{\"futureProcessedAt\":{\"order\":null,\"userErrors\":[{\"field\":[\"order\",\"processedAt\"],\"message\":\"Processed at must not be in the future\",\"code\":\"PROCESSED_AT_INVALID\"}]},\"redundantCustomer\":{\"order\":null,\"userErrors\":[{\"field\":[\"order\"],\"message\":\"Cannot specify both customerId and customer\",\"code\":\"REDUNDANT_CUSTOMER_FIELDS\"}]},\"lineItemTaxLineMissingRate\":{\"order\":null,\"userErrors\":[{\"field\":[\"order\",\"lineItems\",0,\"taxLines\",0,\"rate\"],\"message\":\"Tax line rate must be provided\",\"code\":\"TAX_LINE_RATE_MISSING\"}]},\"shippingLineTaxLineMissingRate\":{\"order\":null,\"userErrors\":[{\"field\":[\"order\",\"shippingLines\",0,\"taxLines\",0,\"rate\"],\"message\":\"Tax line rate must be provided\",\"code\":\"TAX_LINE_RATE_MISSING\"}]}}}"
  assert extended_outcome.staged_resource_ids == []
  assert extended_outcome.log_drafts == []
  assert store.list_effective_orders(extended_outcome.store) == []
}

fn order_create_test_order(
  overrides: List(#(String, root_field.ResolvedValue)),
) -> root_field.ResolvedValue {
  root_field.ObjectVal(
    dict.from_list(list.append(
      [
        #("email", root_field.StringVal("hermes-order-validation@example.com")),
        #("lineItems", root_field.ListVal([order_create_test_line_item([])])),
      ],
      overrides,
    )),
  )
}

fn order_create_test_line_item(
  overrides: List(#(String, root_field.ResolvedValue)),
) -> root_field.ResolvedValue {
  root_field.ObjectVal(
    dict.from_list(list.append(
      [
        #("title", root_field.StringVal("Validation custom item")),
        #("quantity", root_field.IntVal(1)),
        #("priceSet", order_create_test_money("1.00")),
      ],
      overrides,
    )),
  )
}

fn order_create_test_money(amount: String) -> root_field.ResolvedValue {
  root_field.ObjectVal(
    dict.from_list([
      #(
        "shopMoney",
        root_field.ObjectVal(
          dict.from_list([
            #("amount", root_field.StringVal(amount)),
            #("currencyCode", root_field.StringVal("USD")),
          ]),
        ),
      ),
    ]),
  )
}

fn order_payment_mutation_path() -> String {
  "/admin/api/2026-04/graphql.json"
}

fn order_payment_create_document() -> String {
  "
    mutation CreatePaymentOrder($order: OrderCreateOrderInput!) {
      orderCreate(order: $order) {
        order {
          id
          transactions {
            id
            kind
            status
          }
        }
        userErrors {
          field
          message
          code
        }
      }
    }
  "
}

fn order_payment_capture_document() -> String {
  "
    mutation CapturePaymentOrder($input: OrderCaptureInput!) {
      orderCapture(input: $input) {
        transaction {
          id
        }
        userErrors {
          field
          message
          code
        }
      }
    }
  "
}

fn transaction_void_error_document() -> String {
  "
    mutation VoidPaymentTransaction($id: ID!) {
      transactionVoid(parentTransactionId: $id) {
        transaction {
          id
        }
        userErrors {
          field
          message
          code
        }
      }
    }
  "
}

fn order_payment_transaction(
  kind: String,
  status: String,
) -> root_field.ResolvedValue {
  root_field.ObjectVal(
    dict.from_list([
      #("kind", root_field.StringVal(kind)),
      #("status", root_field.StringVal(status)),
      #("gateway", root_field.StringVal("manual")),
      #("amountSet", order_create_test_money("25.00")),
    ]),
  )
}

fn order_payment_order(transaction: root_field.ResolvedValue) {
  order_create_test_order([
    #("currency", root_field.StringVal("USD")),
    #("transactions", root_field.ListVal([transaction])),
    #(
      "lineItems",
      root_field.ListVal([
        order_create_test_line_item([
          #("title", root_field.StringVal("Payment test item")),
          #("priceSet", order_create_test_money("25.00")),
        ]),
      ]),
    ),
  ])
}

fn create_payment_order(transaction: root_field.ResolvedValue) {
  orders.process_mutation(
    store.new(),
    synthetic_identity.new(),
    order_payment_mutation_path(),
    order_payment_create_document(),
    dict.from_list([#("order", order_payment_order(transaction))]),
    empty_upstream_context(),
  )
}

fn transaction_void(source: store.Store, id: String) {
  orders.process_mutation(
    source,
    synthetic_identity.new(),
    order_payment_mutation_path(),
    transaction_void_error_document(),
    dict.from_list([#("id", root_field.StringVal(id))]),
    empty_upstream_context(),
  )
}

fn capture_payment_order(
  source: store.Store,
  order_id: String,
  auth_id: String,
) {
  orders.process_mutation(
    source,
    synthetic_identity.new(),
    order_payment_mutation_path(),
    order_payment_capture_document(),
    dict.from_list([
      #(
        "input",
        root_field.ObjectVal(
          dict.from_list([
            #("id", root_field.StringVal(order_id)),
            #("parentTransactionId", root_field.StringVal(auth_id)),
            #("amount", root_field.FloatVal(25.0)),
            #("currency", root_field.StringVal("USD")),
            #("finalCapture", root_field.BoolVal(True)),
          ]),
        ),
      ),
    ]),
    empty_upstream_context(),
  )
}

pub fn transaction_void_missing_transaction_uses_shopify_code_and_field_test() {
  let outcome =
    transaction_void(store.new(), "gid://shopify/OrderTransaction/missing")

  assert json.to_string(outcome.data)
    == "{\"data\":{\"transactionVoid\":{\"transaction\":null,\"userErrors\":[{\"field\":[\"parentTransactionId\"],\"message\":\"Transaction does not exist\",\"code\":\"TRANSACTION_NOT_FOUND\"}]}}}"
  assert outcome.staged_resource_ids == []
  assert store.list_effective_orders(outcome.store) == []
}

pub fn transaction_void_rejects_capture_with_auth_not_successful_code_test() {
  let create_outcome =
    create_payment_order(order_payment_transaction("CAPTURE", "SUCCESS"))
  let outcome =
    transaction_void(create_outcome.store, "gid://shopify/OrderTransaction/3")

  assert json.to_string(outcome.data)
    == "{\"data\":{\"transactionVoid\":{\"transaction\":null,\"userErrors\":[{\"field\":[\"parentTransactionId\"],\"message\":\"Parent transaction must be a successful authorization\",\"code\":\"AUTH_NOT_SUCCESSFUL\"}]}}}"
  assert outcome.staged_resource_ids == []
}

pub fn transaction_void_rejects_failed_auth_with_auth_not_successful_code_test() {
  let create_outcome =
    create_payment_order(order_payment_transaction("AUTHORIZATION", "FAILURE"))
  let outcome =
    transaction_void(create_outcome.store, "gid://shopify/OrderTransaction/3")

  assert json.to_string(outcome.data)
    == "{\"data\":{\"transactionVoid\":{\"transaction\":null,\"userErrors\":[{\"field\":[\"parentTransactionId\"],\"message\":\"Parent transaction must be a successful authorization\",\"code\":\"AUTH_NOT_SUCCESSFUL\"}]}}}"
  assert outcome.staged_resource_ids == []
}

pub fn transaction_void_rejects_captured_auth_with_auth_not_voidable_code_test() {
  let create_outcome =
    create_payment_order(order_payment_transaction("AUTHORIZATION", "SUCCESS"))
  let capture_outcome =
    capture_payment_order(
      create_outcome.store,
      "gid://shopify/Order/1",
      "gid://shopify/OrderTransaction/3",
    )
  let outcome =
    transaction_void(capture_outcome.store, "gid://shopify/OrderTransaction/3")

  assert json.to_string(outcome.data)
    == "{\"data\":{\"transactionVoid\":{\"transaction\":null,\"userErrors\":[{\"field\":[\"parentTransactionId\"],\"message\":\"Parent transaction require a parent_id referring to a voidable transaction\",\"code\":\"AUTH_NOT_VOIDABLE\"}]}}}"
  assert outcome.staged_resource_ids == []
}

pub fn transaction_void_rejects_already_voided_auth_with_auth_not_voidable_code_test() {
  let create_outcome =
    create_payment_order(order_payment_transaction("AUTHORIZATION", "SUCCESS"))
  let void_outcome =
    transaction_void(create_outcome.store, "gid://shopify/OrderTransaction/3")
  let outcome =
    transaction_void(void_outcome.store, "gid://shopify/OrderTransaction/3")

  assert json.to_string(outcome.data)
    == "{\"data\":{\"transactionVoid\":{\"transaction\":null,\"userErrors\":[{\"field\":[\"parentTransactionId\"],\"message\":\"Parent transaction require a parent_id referring to a voidable transaction\",\"code\":\"AUTH_NOT_VOIDABLE\"}]}}}"
  assert outcome.staged_resource_ids == []
}

pub fn transaction_void_rejects_expired_auth_with_auth_not_voidable_code_test() {
  let transaction_id = "gid://shopify/OrderTransaction/expired-auth"
  let order =
    types.OrderRecord(
      id: "gid://shopify/Order/expired-auth",
      cursor: None,
      data: types.CapturedObject([
        #("id", types.CapturedString("gid://shopify/Order/expired-auth")),
        #(
          "transactions",
          types.CapturedArray([
            types.CapturedObject([
              #("id", types.CapturedString(transaction_id)),
              #("kind", types.CapturedString("AUTHORIZATION")),
              #("status", types.CapturedString("SUCCESS")),
              #("gateway", types.CapturedString("manual")),
              #(
                "authorizationExpiresAt",
                types.CapturedString("2000-01-01T00:00:00.000Z"),
              ),
              #(
                "amountSet",
                types.CapturedObject([
                  #(
                    "shopMoney",
                    types.CapturedObject([
                      #("amount", types.CapturedString("25.0")),
                      #("currencyCode", types.CapturedString("CAD")),
                    ]),
                  ),
                ]),
              ),
            ]),
          ]),
        ),
      ]),
    )
  let outcome =
    transaction_void(
      store.new() |> store.upsert_base_orders([order]),
      transaction_id,
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"transactionVoid\":{\"transaction\":null,\"userErrors\":[{\"field\":[\"parentTransactionId\"],\"message\":\"Parent transaction require a parent_id referring to a voidable transaction\",\"code\":\"AUTH_NOT_VOIDABLE\"}]}}}"
  assert outcome.staged_resource_ids == []
}

pub fn orders_order_create_stages_selected_order_and_downstream_read_test() {
  let mutation =
    "
    mutation Create($order: OrderCreateOrderInput!) {
      orderCreate(order: $order) {
        order {
          id
          name
          email
          displayFinancialStatus
          displayFulfillmentStatus
          note
          tags
          currentTotalPriceSet {
            shopMoney {
              amount
              currencyCode
            }
          }
          totalTaxSet {
            shopMoney {
              amount
              currencyCode
            }
          }
          totalDiscountsSet {
            shopMoney {
              amount
              currencyCode
            }
          }
          discountCodes
          shippingLines(first: 5) {
            nodes {
              title
              originalPriceSet {
                shopMoney {
                  amount
                  currencyCode
                }
              }
            }
          }
          lineItems(first: 5) {
            nodes {
              id
              title
              quantity
              sku
              variant {
                id
              }
              originalUnitPriceSet {
                shopMoney {
                  amount
                  currencyCode
                }
                presentmentMoney {
                  amount
                  currencyCode
                }
              }
              taxLines {
                title
                rate
                priceSet {
                  shopMoney {
                    amount
                    currencyCode
                  }
                }
              }
            }
          }
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  let variables =
    dict.from_list([
      #(
        "order",
        root_field.ObjectVal(
          dict.from_list([
            #("email", root_field.StringVal("order-create@example.test")),
            #("note", root_field.StringVal("order create parity")),
            #(
              "tags",
              root_field.ListVal([
                root_field.StringVal("parity-plan"),
                root_field.StringVal("order-create"),
              ]),
            ),
            #("currency", root_field.StringVal("USD")),
            #("fulfillmentStatus", root_field.StringVal("FULFILLED")),
            #(
              "discountCode",
              root_field.ObjectVal(
                dict.from_list([
                  #(
                    "itemFixedDiscountCode",
                    root_field.ObjectVal(
                      dict.from_list([
                        #("code", root_field.StringVal("SAVE5")),
                        #(
                          "amountSet",
                          root_field.ObjectVal(
                            dict.from_list([
                              #(
                                "shopMoney",
                                root_field.ObjectVal(
                                  dict.from_list([
                                    #("amount", root_field.StringVal("5.00")),
                                    #(
                                      "currencyCode",
                                      root_field.StringVal("USD"),
                                    ),
                                  ]),
                                ),
                              ),
                            ]),
                          ),
                        ),
                      ]),
                    ),
                  ),
                ]),
              ),
            ),
            #(
              "shippingLines",
              root_field.ListVal([
                root_field.ObjectVal(
                  dict.from_list([
                    #("title", root_field.StringVal("Standard")),
                    #(
                      "priceSet",
                      root_field.ObjectVal(
                        dict.from_list([
                          #(
                            "shopMoney",
                            root_field.ObjectVal(
                              dict.from_list([
                                #("amount", root_field.StringVal("5.00")),
                                #("currencyCode", root_field.StringVal("USD")),
                              ]),
                            ),
                          ),
                        ]),
                      ),
                    ),
                    #(
                      "taxLines",
                      root_field.ListVal([
                        root_field.ObjectVal(
                          dict.from_list([
                            #("title", root_field.StringVal("Shipping tax")),
                            #("rate", root_field.FloatVal(0.1)),
                            #(
                              "priceSet",
                              root_field.ObjectVal(
                                dict.from_list([
                                  #(
                                    "shopMoney",
                                    root_field.ObjectVal(
                                      dict.from_list([
                                        #(
                                          "amount",
                                          root_field.StringVal("0.50"),
                                        ),
                                        #(
                                          "currencyCode",
                                          root_field.StringVal("USD"),
                                        ),
                                      ]),
                                    ),
                                  ),
                                ]),
                              ),
                            ),
                          ]),
                        ),
                      ]),
                    ),
                  ]),
                ),
              ]),
            ),
            #(
              "lineItems",
              root_field.ListVal([
                root_field.ObjectVal(
                  dict.from_list([
                    #(
                      "variantId",
                      root_field.StringVal("gid://shopify/ProductVariant/99"),
                    ),
                    #("title", root_field.StringVal("Inventory-backed line")),
                    #("quantity", root_field.IntVal(2)),
                    #("sku", root_field.StringVal("order-create-sku")),
                    #(
                      "priceSet",
                      root_field.ObjectVal(
                        dict.from_list([
                          #(
                            "shopMoney",
                            root_field.ObjectVal(
                              dict.from_list([
                                #("amount", root_field.StringVal("20.00")),
                                #("currencyCode", root_field.StringVal("USD")),
                              ]),
                            ),
                          ),
                          #(
                            "presentmentMoney",
                            root_field.ObjectVal(
                              dict.from_list([
                                #("amount", root_field.StringVal("27.00")),
                                #("currencyCode", root_field.StringVal("CAD")),
                              ]),
                            ),
                          ),
                        ]),
                      ),
                    ),
                    #(
                      "taxLines",
                      root_field.ListVal([
                        root_field.ObjectVal(
                          dict.from_list([
                            #("title", root_field.StringVal("Line tax")),
                            #("rate", root_field.FloatVal(0.05)),
                            #(
                              "priceSet",
                              root_field.ObjectVal(
                                dict.from_list([
                                  #(
                                    "shopMoney",
                                    root_field.ObjectVal(
                                      dict.from_list([
                                        #(
                                          "amount",
                                          root_field.StringVal("2.00"),
                                        ),
                                        #(
                                          "currencyCode",
                                          root_field.StringVal("USD"),
                                        ),
                                      ]),
                                    ),
                                  ),
                                ]),
                              ),
                            ),
                          ]),
                        ),
                      ]),
                    ),
                  ]),
                ),
              ]),
            ),
            #(
              "transactions",
              root_field.ListVal([
                root_field.ObjectVal(
                  dict.from_list([
                    #("kind", root_field.StringVal("SALE")),
                    #("status", root_field.StringVal("SUCCESS")),
                    #("gateway", root_field.StringVal("manual")),
                    #(
                      "amountSet",
                      root_field.ObjectVal(
                        dict.from_list([
                          #(
                            "shopMoney",
                            root_field.ObjectVal(
                              dict.from_list([
                                #("amount", root_field.StringVal("42.50")),
                                #("currencyCode", root_field.StringVal("USD")),
                              ]),
                            ),
                          ),
                        ]),
                      ),
                    ),
                  ]),
                ),
              ]),
            ),
          ]),
        ),
      ),
    ])
  let outcome =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      mutation,
      variables,
      empty_upstream_context(),
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"orderCreate\":{\"order\":{\"id\":\"gid://shopify/Order/1\",\"name\":\"#1\",\"email\":\"order-create@example.test\",\"displayFinancialStatus\":\"PAID\",\"displayFulfillmentStatus\":\"FULFILLED\",\"note\":\"order create parity\",\"tags\":[\"order-create\",\"parity-plan\"],\"currentTotalPriceSet\":{\"shopMoney\":{\"amount\":\"42.5\",\"currencyCode\":\"USD\"}},\"totalTaxSet\":{\"shopMoney\":{\"amount\":\"2.5\",\"currencyCode\":\"USD\"}},\"totalDiscountsSet\":{\"shopMoney\":{\"amount\":\"5.0\",\"currencyCode\":\"USD\"}},\"discountCodes\":[\"SAVE5\"],\"shippingLines\":{\"nodes\":[{\"title\":\"Standard\",\"originalPriceSet\":{\"shopMoney\":{\"amount\":\"5.0\",\"currencyCode\":\"USD\"}}}]},\"lineItems\":{\"nodes\":[{\"id\":\"gid://shopify/LineItem/2\",\"title\":\"Inventory-backed line\",\"quantity\":2,\"sku\":\"order-create-sku\",\"variant\":{\"id\":\"gid://shopify/ProductVariant/99\"},\"originalUnitPriceSet\":{\"shopMoney\":{\"amount\":\"20.0\",\"currencyCode\":\"USD\"},\"presentmentMoney\":{\"amount\":\"27.0\",\"currencyCode\":\"CAD\"}},\"taxLines\":[{\"title\":\"Line tax\",\"rate\":0.05,\"priceSet\":{\"shopMoney\":{\"amount\":\"2.0\",\"currencyCode\":\"USD\"}}}]}]}},\"userErrors\":[]}}}"
  assert outcome.staged_resource_ids == ["gid://shopify/Order/1"]

  let query =
    "
    query Read($id: ID!) {
      order(id: $id) {
        id
        displayFinancialStatus
        currentTotalPriceSet {
          shopMoney {
            amount
            currencyCode
          }
        }
        lineItems(first: 5) {
          nodes {
            title
            quantity
          }
        }
      }
    }
  "
  let assert Ok(read_result) =
    orders.process(
      outcome.store,
      query,
      dict.from_list([#("id", root_field.StringVal("gid://shopify/Order/1"))]),
    )
  assert json.to_string(read_result)
    == "{\"data\":{\"order\":{\"id\":\"gid://shopify/Order/1\",\"displayFinancialStatus\":\"PAID\",\"currentTotalPriceSet\":{\"shopMoney\":{\"amount\":\"42.5\",\"currencyCode\":\"USD\"}},\"lineItems\":{\"nodes\":[{\"title\":\"Inventory-backed line\",\"quantity\":2}]}}}}"
}

pub fn orders_order_create_money_bags_default_presentment_money_test() {
  let mutation =
    "
    mutation Create($order: OrderCreateOrderInput!) {
      orderCreate(order: $order) {
        order {
          currentTotalPriceSet {
            shopMoney {
              amount
              currencyCode
            }
            presentmentMoney {
              amount
              currencyCode
            }
          }
          totalTaxSet {
            shopMoney {
              amount
              currencyCode
            }
            presentmentMoney {
              amount
              currencyCode
            }
          }
          totalReceivedSet {
            shopMoney {
              amount
              currencyCode
            }
            presentmentMoney {
              amount
              currencyCode
            }
          }
          transactions {
            amountSet {
              shopMoney {
                amount
                currencyCode
              }
              presentmentMoney {
                amount
                currencyCode
              }
            }
          }
          lineItems(first: 5) {
            nodes {
              originalUnitPriceSet {
                shopMoney {
                  amount
                  currencyCode
                }
                presentmentMoney {
                  amount
                  currencyCode
                }
              }
            }
          }
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  let variables =
    dict.from_list([
      #(
        "order",
        root_field.ObjectVal(
          dict.from_list([
            #("currency", root_field.StringVal("USD")),
            #(
              "lineItems",
              root_field.ListVal([
                root_field.ObjectVal(
                  dict.from_list([
                    #("title", root_field.StringVal("MoneyBag line")),
                    #("quantity", root_field.IntVal(1)),
                    #(
                      "priceSet",
                      root_field.ObjectVal(
                        dict.from_list([
                          #(
                            "shopMoney",
                            root_field.ObjectVal(
                              dict.from_list([
                                #("amount", root_field.StringVal("12.00")),
                                #("currencyCode", root_field.StringVal("USD")),
                              ]),
                            ),
                          ),
                        ]),
                      ),
                    ),
                    #(
                      "taxLines",
                      root_field.ListVal([
                        root_field.ObjectVal(
                          dict.from_list([
                            #("title", root_field.StringVal("Line tax")),
                            #("rate", root_field.FloatVal(0.125)),
                            #(
                              "priceSet",
                              root_field.ObjectVal(
                                dict.from_list([
                                  #(
                                    "shopMoney",
                                    root_field.ObjectVal(
                                      dict.from_list([
                                        #(
                                          "amount",
                                          root_field.StringVal("1.50"),
                                        ),
                                        #(
                                          "currencyCode",
                                          root_field.StringVal("USD"),
                                        ),
                                      ]),
                                    ),
                                  ),
                                ]),
                              ),
                            ),
                          ]),
                        ),
                      ]),
                    ),
                  ]),
                ),
              ]),
            ),
            #(
              "transactions",
              root_field.ListVal([
                root_field.ObjectVal(
                  dict.from_list([
                    #("kind", root_field.StringVal("SALE")),
                    #("status", root_field.StringVal("SUCCESS")),
                    #("gateway", root_field.StringVal("manual")),
                    #(
                      "amountSet",
                      root_field.ObjectVal(
                        dict.from_list([
                          #(
                            "shopMoney",
                            root_field.ObjectVal(
                              dict.from_list([
                                #("amount", root_field.StringVal("13.50")),
                                #("currencyCode", root_field.StringVal("USD")),
                              ]),
                            ),
                          ),
                        ]),
                      ),
                    ),
                  ]),
                ),
              ]),
            ),
          ]),
        ),
      ),
    ])
  let outcome =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      mutation,
      variables,
      empty_upstream_context(),
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"orderCreate\":{\"order\":{\"currentTotalPriceSet\":{\"shopMoney\":{\"amount\":\"13.5\",\"currencyCode\":\"USD\"},\"presentmentMoney\":{\"amount\":\"13.5\",\"currencyCode\":\"USD\"}},\"totalTaxSet\":{\"shopMoney\":{\"amount\":\"1.5\",\"currencyCode\":\"USD\"},\"presentmentMoney\":{\"amount\":\"1.5\",\"currencyCode\":\"USD\"}},\"totalReceivedSet\":{\"shopMoney\":{\"amount\":\"13.5\",\"currencyCode\":\"USD\"},\"presentmentMoney\":{\"amount\":\"13.5\",\"currencyCode\":\"USD\"}},\"transactions\":[{\"amountSet\":{\"shopMoney\":{\"amount\":\"13.5\",\"currencyCode\":\"USD\"},\"presentmentMoney\":{\"amount\":\"13.5\",\"currencyCode\":\"USD\"}}}],\"lineItems\":{\"nodes\":[{\"originalUnitPriceSet\":{\"shopMoney\":{\"amount\":\"12.0\",\"currencyCode\":\"USD\"},\"presentmentMoney\":{\"amount\":\"12.0\",\"currencyCode\":\"USD\"}}}]}},\"userErrors\":[]}}}"
}

pub fn orders_order_create_money_bags_preserve_supplied_presentment_money_test() {
  let mutation =
    "
    mutation Create($order: OrderCreateOrderInput!) {
      orderCreate(order: $order) {
        order {
          id
          totalPriceSet {
            shopMoney {
              amount
              currencyCode
            }
            presentmentMoney {
              amount
              currencyCode
            }
          }
          lineItems(first: 5) {
            nodes {
              originalUnitPriceSet {
                shopMoney {
                  amount
                  currencyCode
                }
                presentmentMoney {
                  amount
                  currencyCode
                }
              }
            }
          }
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  let variables =
    dict.from_list([
      #(
        "order",
        root_field.ObjectVal(
          dict.from_list([
            #("currency", root_field.StringVal("CAD")),
            #(
              "lineItems",
              root_field.ListVal([
                root_field.ObjectVal(
                  dict.from_list([
                    #("title", root_field.StringVal("FX line")),
                    #("quantity", root_field.IntVal(1)),
                    #(
                      "priceSet",
                      root_field.ObjectVal(
                        dict.from_list([
                          #(
                            "shopMoney",
                            root_field.ObjectVal(
                              dict.from_list([
                                #("amount", root_field.StringVal("10.00")),
                                #("currencyCode", root_field.StringVal("CAD")),
                              ]),
                            ),
                          ),
                          #(
                            "presentmentMoney",
                            root_field.ObjectVal(
                              dict.from_list([
                                #("amount", root_field.StringVal("7.00")),
                                #("currencyCode", root_field.StringVal("USD")),
                              ]),
                            ),
                          ),
                        ]),
                      ),
                    ),
                  ]),
                ),
              ]),
            ),
          ]),
        ),
      ),
    ])
  let outcome =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      mutation,
      variables,
      empty_upstream_context(),
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"orderCreate\":{\"order\":{\"id\":\"gid://shopify/Order/1\",\"totalPriceSet\":{\"shopMoney\":{\"amount\":\"10.0\",\"currencyCode\":\"CAD\"},\"presentmentMoney\":{\"amount\":\"7.0\",\"currencyCode\":\"USD\"}},\"lineItems\":{\"nodes\":[{\"originalUnitPriceSet\":{\"shopMoney\":{\"amount\":\"10.0\",\"currencyCode\":\"CAD\"},\"presentmentMoney\":{\"amount\":\"7.0\",\"currencyCode\":\"USD\"}}}]}},\"userErrors\":[]}}}"

  let mark_as_paid_mutation =
    "
    mutation MarkPaid($input: OrderMarkAsPaidInput!) {
      orderMarkAsPaid(input: $input) {
        order {
          totalOutstandingSet {
            shopMoney {
              amount
              currencyCode
            }
            presentmentMoney {
              amount
              currencyCode
            }
          }
          totalReceivedSet {
            shopMoney {
              amount
              currencyCode
            }
            presentmentMoney {
              amount
              currencyCode
            }
          }
          transactions {
            amountSet {
              shopMoney {
                amount
                currencyCode
              }
              presentmentMoney {
                amount
                currencyCode
              }
            }
          }
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  let order_id = "gid://shopify/Order/1"
  let mark_as_paid_outcome =
    orders.process_mutation(
      outcome.store,
      outcome.identity,
      "/admin/api/2025-01/graphql.json",
      mark_as_paid_mutation,
      dict.from_list([
        #(
          "input",
          root_field.ObjectVal(
            dict.from_list([
              #("id", root_field.StringVal(order_id)),
            ]),
          ),
        ),
      ]),
      empty_upstream_context(),
    )

  assert json.to_string(mark_as_paid_outcome.data)
    == "{\"data\":{\"orderMarkAsPaid\":{\"order\":{\"totalOutstandingSet\":{\"shopMoney\":{\"amount\":\"0.0\",\"currencyCode\":\"CAD\"},\"presentmentMoney\":{\"amount\":\"0.0\",\"currencyCode\":\"USD\"}},\"totalReceivedSet\":{\"shopMoney\":{\"amount\":\"10.0\",\"currencyCode\":\"CAD\"},\"presentmentMoney\":{\"amount\":\"7.0\",\"currencyCode\":\"USD\"}},\"transactions\":[{\"amountSet\":{\"shopMoney\":{\"amount\":\"10.0\",\"currencyCode\":\"CAD\"},\"presentmentMoney\":{\"amount\":\"7.0\",\"currencyCode\":\"USD\"}}}]},\"userErrors\":[]}}}"

  let refund_mutation =
    "
    mutation Refund($input: RefundInput!) {
      refundCreate(input: $input) {
        refund {
          totalRefundedSet {
            shopMoney {
              amount
              currencyCode
            }
            presentmentMoney {
              amount
              currencyCode
            }
          }
        }
        order {
          totalRefundedSet {
            shopMoney {
              amount
              currencyCode
            }
            presentmentMoney {
              amount
              currencyCode
            }
          }
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  let refund_outcome =
    orders.process_mutation(
      mark_as_paid_outcome.store,
      mark_as_paid_outcome.identity,
      "/admin/api/2025-01/graphql.json",
      refund_mutation,
      dict.from_list([
        #(
          "input",
          root_field.ObjectVal(
            dict.from_list([
              #("orderId", root_field.StringVal(order_id)),
              #(
                "transactions",
                root_field.ListVal([
                  root_field.ObjectVal(
                    dict.from_list([
                      #("amount", root_field.StringVal("5.00")),
                      #("gateway", root_field.StringVal("manual")),
                      #("kind", root_field.StringVal("REFUND")),
                      #("orderId", root_field.StringVal(order_id)),
                    ]),
                  ),
                ]),
              ),
            ]),
          ),
        ),
      ]),
      empty_upstream_context(),
    )

  assert json.to_string(refund_outcome.data)
    == "{\"data\":{\"refundCreate\":{\"refund\":{\"totalRefundedSet\":{\"shopMoney\":{\"amount\":\"5.0\",\"currencyCode\":\"CAD\"},\"presentmentMoney\":{\"amount\":\"3.5\",\"currencyCode\":\"USD\"}}},\"order\":{\"totalRefundedSet\":{\"shopMoney\":{\"amount\":\"5.0\",\"currencyCode\":\"CAD\"},\"presentmentMoney\":{\"amount\":\"3.5\",\"currencyCode\":\"USD\"}}},\"userErrors\":[]}}}"
}

pub fn orders_order_update_validation_guardrails_test() {
  let missing_inline_id =
    "
    mutation OrderUpdateInlineMissingIdParityPlan {
      orderUpdate(
        input: {
          note: \"order update inline missing-id parity plan\"
          tags: [\"parity-plan\", \"order-update\", \"inline-missing-id\"]
        }
      ) {
        order {
          id
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  let missing_inline_outcome =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      missing_inline_id,
      dict.new(),
      empty_upstream_context(),
    )
  assert json.to_string(missing_inline_outcome.data)
    == "{\"errors\":[{\"message\":\"Argument 'id' on InputObject 'OrderInput' is required. Expected type ID!\",\"path\":[\"mutation OrderUpdateInlineMissingIdParityPlan\",\"orderUpdate\",\"input\",\"id\"],\"extensions\":{\"code\":\"missingRequiredInputObjectAttribute\",\"argumentName\":\"id\",\"argumentType\":\"ID!\",\"inputObjectType\":\"OrderInput\"}}]}"

  let null_inline_id =
    "
    mutation OrderUpdateInlineNullIdParityPlan {
      orderUpdate(
        input: {
          id: null
          note: \"order update inline null-id parity plan\"
          tags: [\"parity-plan\", \"order-update\", \"inline-null-id\"]
        }
      ) {
        order {
          id
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  let null_inline_outcome =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      null_inline_id,
      dict.new(),
      empty_upstream_context(),
    )
  assert json.to_string(null_inline_outcome.data)
    == "{\"errors\":[{\"message\":\"Argument 'id' on InputObject 'OrderInput' has an invalid value (null). Expected type 'ID!'.\",\"path\":[\"mutation OrderUpdateInlineNullIdParityPlan\",\"orderUpdate\",\"input\",\"id\"],\"extensions\":{\"code\":\"argumentLiteralsIncompatible\",\"typeName\":\"InputObject\",\"argumentName\":\"id\"}}]}"

  let missing_variable_id =
    "
    mutation OrderUpdateParityPlan($input: OrderInput!) {
      orderUpdate(input: $input) {
        order {
          id
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  let variables =
    dict.from_list([
      #(
        "input",
        root_field.ObjectVal(
          dict.from_list([
            #(
              "note",
              root_field.StringVal("order update missing-id parity probe"),
            ),
            #(
              "tags",
              root_field.ListVal([
                root_field.StringVal("parity-probe"),
                root_field.StringVal("order-update"),
                root_field.StringVal("missing-id"),
              ]),
            ),
          ]),
        ),
      ),
    ])
  let missing_variable_outcome =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      missing_variable_id,
      variables,
      empty_upstream_context(),
    )
  let missing_variable_json = json.to_string(missing_variable_outcome.data)
  assert string.contains(
    missing_variable_json,
    "Variable $input of type OrderInput! was provided invalid value for id (Expected value to not be null)",
  )
  assert string.contains(missing_variable_json, "\"code\":\"INVALID_VARIABLE\"")
  assert string.contains(
    missing_variable_json,
    "\"note\":\"order update missing-id parity probe\"",
  )
  assert string.contains(
    missing_variable_json,
    "\"tags\":[\"parity-probe\",\"order-update\",\"missing-id\"]",
  )

  let unknown_id =
    "
    mutation OrderUpdateParityPlan($input: OrderInput!) {
      orderUpdate(input: $input) {
        order {
          id
          name
          updatedAt
          note
          tags
        }
        userErrors {
          field
          message
          code
        }
      }
    }
  "
  let unknown_variables =
    dict.from_list([
      #(
        "input",
        root_field.ObjectVal(
          dict.from_list([
            #("id", root_field.StringVal("gid://shopify/Order/0")),
            #("note", root_field.StringVal("order update parity plan")),
            #(
              "tags",
              root_field.ListVal([
                root_field.StringVal("parity-plan"),
                root_field.StringVal("order-update"),
              ]),
            ),
          ]),
        ),
      ),
    ])
  let unknown_outcome =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      unknown_id,
      unknown_variables,
      empty_upstream_context(),
    )
  assert json.to_string(unknown_outcome.data)
    == "{\"data\":{\"orderUpdate\":{\"order\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"Order does not exist\",\"code\":\"NOT_FOUND\"}]}}}"
  assert unknown_outcome.staged_resource_ids == []
  assert unknown_outcome.log_drafts == []
  assert store.list_effective_orders(unknown_outcome.store) == []
}

pub fn orders_order_update_rejects_empty_phone_and_bad_address_test() {
  let order_id = "gid://shopify/Order/6830627356905"
  let seeded =
    store.new()
    |> store.upsert_base_orders([
      types.OrderRecord(
        id: order_id,
        cursor: None,
        data: types.CapturedObject([
          #("id", types.CapturedString(order_id)),
          #("name", types.CapturedString("#1323")),
          #("email", types.CapturedString("before@example.com")),
          #("phone", types.CapturedString("+16135550100")),
          #("note", types.CapturedString("before")),
          #("shippingAddress", types.CapturedNull),
        ]),
      ),
    ])
  let mutation =
    "
    mutation OrderUpdateValidation($input: OrderInput!) {
      orderUpdate(input: $input) {
        order {
          id
          note
          phone
          shippingAddress {
            countryCodeV2
            provinceCode
          }
        }
        userErrors {
          field
          message
          code
        }
      }
    }
  "

  let empty_outcome =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      mutation,
      dict.from_list([
        #(
          "input",
          root_field.ObjectVal(
            dict.from_list([
              #("id", root_field.StringVal(order_id)),
            ]),
          ),
        ),
      ]),
      empty_upstream_context(),
    )
  let empty_json = json.to_string(empty_outcome.data)
  assert string.contains(empty_json, "\"order\":{\"id\":\"" <> order_id)
  assert string.contains(empty_json, "\"field\":null")
  assert string.contains(empty_json, "\"code\":\"INVALID\"")
  assert string.contains(
    empty_json,
    "No valid update parameters have been provided",
  )
  assert empty_outcome.staged_resource_ids == []
  assert empty_outcome.log_drafts == []

  let phone_outcome =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      mutation,
      dict.from_list([
        #(
          "input",
          root_field.ObjectVal(
            dict.from_list([
              #("id", root_field.StringVal(order_id)),
              #("phone", root_field.StringVal("not a phone")),
            ]),
          ),
        ),
      ]),
      empty_upstream_context(),
    )
  let phone_json = json.to_string(phone_outcome.data)
  assert string.contains(phone_json, "\"order\":{\"id\":\"" <> order_id)
  assert string.contains(phone_json, "\"field\":[\"phone\"]")
  assert string.contains(phone_json, "\"code\":\"INVALID\"")
  assert phone_outcome.staged_resource_ids == []
  assert phone_outcome.log_drafts == []

  let address_outcome =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      mutation,
      dict.from_list([
        #(
          "input",
          root_field.ObjectVal(
            dict.from_list([
              #("id", root_field.StringVal(order_id)),
              #(
                "shippingAddress",
                root_field.ObjectVal(
                  dict.from_list([
                    #("address1", root_field.StringVal("3 Bad Province")),
                    #("city", root_field.StringVal("Chicago")),
                    #("countryCode", root_field.StringVal("US")),
                    #("provinceCode", root_field.StringVal("ON")),
                  ]),
                ),
              ),
            ]),
          ),
        ),
      ]),
      empty_upstream_context(),
    )
  let address_json = json.to_string(address_outcome.data)
  assert string.contains(address_json, "\"order\":{\"id\":\"" <> order_id)
  assert string.contains(
    address_json,
    "\"field\":[\"shippingAddress\",\"lastName\"]",
  )
  assert string.contains(address_json, "Enter a last name")
  assert string.contains(
    address_json,
    "\"field\":[\"shippingAddress\",\"zip\"]",
  )
  assert string.contains(address_json, "Enter a ZIP code")
  assert string.contains(
    address_json,
    "\"field\":[\"shippingAddress\",\"province\"]",
  )
  assert string.contains(
    address_json,
    "State is not a valid state in United States",
  )
  assert string.contains(address_json, "\"code\":\"INVALID\"")
  assert address_outcome.staged_resource_ids == []
  assert address_outcome.log_drafts == []
}

pub fn orders_order_update_existing_order_read_after_write_test() {
  let order_id = "gid://shopify/Order/6830627356905"
  let metafield_id = "gid://shopify/Metafield/35289666519273"
  let seeded =
    store.new()
    |> store.upsert_base_orders([
      types.OrderRecord(
        id: order_id,
        cursor: None,
        data: types.CapturedObject([
          #("id", types.CapturedString(order_id)),
          #("name", types.CapturedString("#1323")),
          #("email", types.CapturedString("before@example.com")),
          #("poNumber", types.CapturedNull),
          #("note", types.CapturedString("before")),
          #("tags", types.CapturedArray([types.CapturedString("before")])),
          #(
            "customer",
            types.CapturedObject([
              #(
                "id",
                types.CapturedString("gid://shopify/Customer/9096793751785"),
              ),
              #("email", types.CapturedString("operator@example.com")),
              #("displayName", types.CapturedString("Hermes Operator")),
            ]),
          ),
          #("customAttributes", types.CapturedArray([])),
          #("shippingAddress", types.CapturedNull),
          #(
            "metafields",
            types.CapturedObject([
              #(
                "nodes",
                types.CapturedArray([
                  types.CapturedObject([
                    #("id", types.CapturedString(metafield_id)),
                    #("namespace", types.CapturedString("custom")),
                    #("key", types.CapturedString("gift")),
                    #("type", types.CapturedString("single_line_text_field")),
                    #("value", types.CapturedString("no")),
                  ]),
                ]),
              ),
            ]),
          ),
        ]),
      ),
    ])
  let mutation =
    "
    mutation OrderUpdateExpandedParityPlan($input: OrderInput!) {
      orderUpdate(input: $input) {
        order {
          id
          name
          email
          poNumber
          note
          tags
          customer {
            id
            email
            displayName
          }
          customAttributes {
            key
            value
          }
          shippingAddress {
            firstName
            lastName
            address1
            address2
            company
            city
            province
            provinceCode
            country
            countryCodeV2
            zip
            phone
          }
          gift: metafield(namespace: \"custom\", key: \"gift\") {
            id
            namespace
            key
            type
            value
          }
          metafields(first: 10) {
            nodes {
              id
              namespace
              key
              type
              value
            }
          }
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  let variables =
    dict.from_list([
      #(
        "input",
        root_field.ObjectVal(
          dict.from_list([
            #("id", root_field.StringVal(order_id)),
            #(
              "email",
              root_field.StringVal("order-update-expanded@example.com"),
            ),
            #("poNumber", root_field.StringVal("PO-ORDER-UPDATE-PARITY")),
            #("note", root_field.StringVal("order update expanded parity plan")),
            #(
              "tags",
              root_field.ListVal([
                root_field.StringVal("order-update"),
                root_field.StringVal("expanded-parity"),
              ]),
            ),
            #(
              "customAttributes",
              root_field.ListVal([
                root_field.ObjectVal(
                  dict.from_list([
                    #("key", root_field.StringVal("source")),
                    #("value", root_field.StringVal("expanded-parity")),
                  ]),
                ),
              ]),
            ),
            #(
              "shippingAddress",
              root_field.ObjectVal(
                dict.from_list([
                  #("firstName", root_field.StringVal("Ada")),
                  #("lastName", root_field.StringVal("Lovelace")),
                  #("address1", root_field.StringVal("190 MacLaren")),
                  #("address2", root_field.StringVal("Suite 200")),
                  #("company", root_field.StringVal("Analytical Engines Ltd")),
                  #("city", root_field.StringVal("Sudbury")),
                  #("province", root_field.StringVal("Ontario")),
                  #("provinceCode", root_field.StringVal("ON")),
                  #("country", root_field.StringVal("Canada")),
                  #("countryCode", root_field.StringVal("CA")),
                  #("zip", root_field.StringVal("K2P0V6")),
                  #("phone", root_field.StringVal("+16135552222")),
                ]),
              ),
            ),
            #(
              "metafields",
              root_field.ListVal([
                root_field.ObjectVal(
                  dict.from_list([
                    #("namespace", root_field.StringVal("custom")),
                    #("key", root_field.StringVal("gift")),
                    #("type", root_field.StringVal("single_line_text_field")),
                    #("value", root_field.StringVal("yes")),
                  ]),
                ),
              ]),
            ),
          ]),
        ),
      ),
    ])
  let outcome =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      mutation,
      variables,
      empty_upstream_context(),
    )
  let mutation_json = json.to_string(outcome.data)
  assert string.contains(
    mutation_json,
    "\"email\":\"order-update-expanded@example.com\"",
  )
  assert string.contains(
    mutation_json,
    "\"poNumber\":\"PO-ORDER-UPDATE-PARITY\"",
  )
  assert string.contains(
    mutation_json,
    "\"note\":\"order update expanded parity plan\"",
  )
  assert string.contains(
    mutation_json,
    "\"tags\":[\"expanded-parity\",\"order-update\"]",
  )
  assert string.contains(
    mutation_json,
    "\"customAttributes\":[{\"key\":\"source\",\"value\":\"expanded-parity\"}]",
  )
  assert string.contains(mutation_json, "\"countryCodeV2\":\"CA\"")
  assert string.contains(
    mutation_json,
    "\"gift\":{\"id\":\"gid://shopify/Metafield/35289666519273\",\"namespace\":\"custom\",\"key\":\"gift\",\"type\":\"single_line_text_field\",\"value\":\"yes\"}",
  )
  assert outcome.staged_resource_ids == [order_id]
  assert list.length(outcome.log_drafts) == 1

  let read_query =
    "
    query OrderUpdateDownstreamRead($id: ID!) {
      order(id: $id) {
        id
        email
        poNumber
        note
        tags
        customAttributes {
          key
          value
        }
        gift: metafield(namespace: \"custom\", key: \"gift\") {
          id
          namespace
          key
          type
          value
        }
      }
    }
  "
  let read_variables = dict.from_list([#("id", root_field.StringVal(order_id))])
  let assert Ok(read) =
    orders.process(outcome.store, read_query, read_variables)
  assert json.to_string(read)
    == "{\"data\":{\"order\":{\"id\":\"gid://shopify/Order/6830627356905\",\"email\":\"order-update-expanded@example.com\",\"poNumber\":\"PO-ORDER-UPDATE-PARITY\",\"note\":\"order update expanded parity plan\",\"tags\":[\"expanded-parity\",\"order-update\"],\"customAttributes\":[{\"key\":\"source\",\"value\":\"expanded-parity\"}],\"gift\":{\"id\":\"gid://shopify/Metafield/35289666519273\",\"namespace\":\"custom\",\"key\":\"gift\",\"type\":\"single_line_text_field\",\"value\":\"yes\"}}}}"
}

pub fn orders_order_open_close_read_after_write_test() {
  let order_id = "gid://shopify/Order/6830646198505"
  let seeded =
    store.new()
    |> store.upsert_base_orders([
      types.OrderRecord(
        id: order_id,
        cursor: None,
        data: types.CapturedObject([
          #("id", types.CapturedString(order_id)),
          #("name", types.CapturedString("#1324")),
          #("closed", types.CapturedBool(False)),
          #("closedAt", types.CapturedNull),
          #("cancelledAt", types.CapturedNull),
          #("cancelReason", types.CapturedNull),
          #("displayFinancialStatus", types.CapturedNull),
          #("paymentGatewayNames", types.CapturedArray([])),
          #(
            "totalOutstandingSet",
            types.CapturedObject([
              #(
                "shopMoney",
                types.CapturedObject([
                  #("amount", types.CapturedString("12.0")),
                  #("currencyCode", types.CapturedString("CAD")),
                ]),
              ),
            ]),
          ),
          #(
            "currentTotalPriceSet",
            types.CapturedObject([
              #(
                "shopMoney",
                types.CapturedObject([
                  #("amount", types.CapturedString("12.0")),
                  #("currencyCode", types.CapturedString("CAD")),
                ]),
              ),
            ]),
          ),
          #("customer", types.CapturedNull),
          #("transactions", types.CapturedArray([])),
        ]),
      ),
    ])
  let selection =
    "
      {
        id
        name
        closed
        closedAt
        cancelledAt
        cancelReason
        displayFinancialStatus
        paymentGatewayNames
        totalOutstandingSet { shopMoney { amount currencyCode } }
        currentTotalPriceSet { shopMoney { amount currencyCode } }
        customer { id email displayName }
        transactions {
          kind
          status
          gateway
          amountSet { shopMoney { amount currencyCode } }
        }
      }
    "
  let close_mutation = "
    mutation {
      orderClose(input: { id: \"gid://shopify/Order/6830646198505\" }) {
        order " <> selection <> "
        userErrors { field message }
      }
    }
  "
  let close_outcome =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      close_mutation,
      dict.new(),
      empty_upstream_context(),
    )
  assert json.to_string(close_outcome.data)
    == "{\"data\":{\"orderClose\":{\"order\":{\"id\":\"gid://shopify/Order/6830646198505\",\"name\":\"#1324\",\"closed\":true,\"closedAt\":\"2024-01-01T00:00:00.000Z\",\"cancelledAt\":null,\"cancelReason\":null,\"displayFinancialStatus\":null,\"paymentGatewayNames\":[],\"totalOutstandingSet\":{\"shopMoney\":{\"amount\":\"12.0\",\"currencyCode\":\"CAD\"}},\"currentTotalPriceSet\":{\"shopMoney\":{\"amount\":\"12.0\",\"currencyCode\":\"CAD\"}},\"customer\":null,\"transactions\":[]},\"userErrors\":[]}}}"
  assert close_outcome.staged_resource_ids == [order_id]
  assert list.length(close_outcome.log_drafts) == 1

  let read_query =
    "
    query {
      order(id: \"gid://shopify/Order/6830646198505\") {
        id
        closed
        closedAt
      }
    }
  "
  let assert Ok(closed_read) =
    orders.process(close_outcome.store, read_query, dict.new())
  assert json.to_string(closed_read)
    == "{\"data\":{\"order\":{\"id\":\"gid://shopify/Order/6830646198505\",\"closed\":true,\"closedAt\":\"2024-01-01T00:00:00.000Z\"}}}"

  let open_mutation =
    "
    mutation {
      orderOpen(input: { id: \"gid://shopify/Order/6830646198505\" }) {
        order {
          id
          closed
          closedAt
        }
        userErrors { field message }
      }
    }
  "
  let open_outcome =
    orders.process_mutation(
      close_outcome.store,
      close_outcome.identity,
      "/admin/api/2026-04/graphql.json",
      open_mutation,
      dict.new(),
      empty_upstream_context(),
    )
  assert json.to_string(open_outcome.data)
    == "{\"data\":{\"orderOpen\":{\"order\":{\"id\":\"gid://shopify/Order/6830646198505\",\"closed\":false,\"closedAt\":null},\"userErrors\":[]}}}"
  assert open_outcome.staged_resource_ids == [order_id]
  assert list.length(open_outcome.log_drafts) == 1

  let assert Ok(open_read) =
    orders.process(open_outcome.store, read_query, dict.new())
  assert json.to_string(open_read)
    == "{\"data\":{\"order\":{\"id\":\"gid://shopify/Order/6830646198505\",\"closed\":false,\"closedAt\":null}}}"
}

pub fn order_close_noops_when_order_already_closed_test() {
  let order_id = "gid://shopify/Order/6830646198505"
  let closed_at = "2024-04-01T12:00:00.000Z"
  let updated_at = "2024-04-01T12:05:00.000Z"
  let seeded =
    store.new()
    |> store.upsert_base_orders([
      types.OrderRecord(
        id: order_id,
        cursor: None,
        data: types.CapturedObject([
          #("id", types.CapturedString(order_id)),
          #("closed", types.CapturedBool(True)),
          #("closedAt", types.CapturedString(closed_at)),
          #("updatedAt", types.CapturedString(updated_at)),
        ]),
      ),
    ])
  let mutation =
    "
    mutation {
      orderClose(input: { id: \"gid://shopify/Order/6830646198505\" }) {
        order {
          id
          closed
          closedAt
          updatedAt
        }
        userErrors { field message }
      }
    }
  "
  let outcome =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      mutation,
      dict.new(),
      empty_upstream_context(),
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"orderClose\":{\"order\":{\"id\":\"gid://shopify/Order/6830646198505\",\"closed\":true,\"closedAt\":\"2024-04-01T12:00:00.000Z\",\"updatedAt\":\"2024-04-01T12:05:00.000Z\"},\"userErrors\":[]}}}"
  assert outcome.staged_resource_ids == []
  assert outcome.log_drafts == []

  let identity_state = synthetic_identity.dump_state(outcome.identity)
  assert identity_state.next_synthetic_timestamp == "2024-01-01T00:00:00.000Z"

  let read_query =
    "
    query {
      order(id: \"gid://shopify/Order/6830646198505\") {
        id
        closed
        closedAt
        updatedAt
      }
    }
  "
  let assert Ok(read) = orders.process(outcome.store, read_query, dict.new())
  assert json.to_string(read)
    == "{\"data\":{\"order\":{\"id\":\"gid://shopify/Order/6830646198505\",\"closed\":true,\"closedAt\":\"2024-04-01T12:00:00.000Z\",\"updatedAt\":\"2024-04-01T12:05:00.000Z\"}}}"
}

pub fn order_open_noops_when_order_already_open_test() {
  let order_id = "gid://shopify/Order/6830646198505"
  let updated_at = "2024-04-01T12:05:00.000Z"
  let seeded =
    store.new()
    |> store.upsert_base_orders([
      types.OrderRecord(
        id: order_id,
        cursor: None,
        data: types.CapturedObject([
          #("id", types.CapturedString(order_id)),
          #("closed", types.CapturedBool(False)),
          #("closedAt", types.CapturedNull),
          #("updatedAt", types.CapturedString(updated_at)),
        ]),
      ),
    ])
  let mutation =
    "
    mutation {
      orderOpen(input: { id: \"gid://shopify/Order/6830646198505\" }) {
        order {
          id
          closed
          closedAt
          updatedAt
        }
        userErrors { field message }
      }
    }
  "
  let outcome =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      mutation,
      dict.new(),
      empty_upstream_context(),
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"orderOpen\":{\"order\":{\"id\":\"gid://shopify/Order/6830646198505\",\"closed\":false,\"closedAt\":null,\"updatedAt\":\"2024-04-01T12:05:00.000Z\"},\"userErrors\":[]}}}"
  assert outcome.staged_resource_ids == []
  assert outcome.log_drafts == []

  let identity_state = synthetic_identity.dump_state(outcome.identity)
  assert identity_state.next_synthetic_timestamp == "2024-01-01T00:00:00.000Z"

  let read_query =
    "
    query {
      order(id: \"gid://shopify/Order/6830646198505\") {
        id
        closed
        closedAt
        updatedAt
      }
    }
  "
  let assert Ok(read) = orders.process(outcome.store, read_query, dict.new())
  assert json.to_string(read)
    == "{\"data\":{\"order\":{\"id\":\"gid://shopify/Order/6830646198505\",\"closed\":false,\"closedAt\":null,\"updatedAt\":\"2024-04-01T12:05:00.000Z\"}}}"
}

pub fn orders_order_cancel_read_after_write_test() {
  let order_id = "gid://shopify/Order/6830646329577"
  let seeded =
    store.new()
    |> store.upsert_base_orders([
      types.OrderRecord(
        id: order_id,
        cursor: None,
        data: types.CapturedObject([
          #("id", types.CapturedString(order_id)),
          #("name", types.CapturedString("#1328")),
          #("closed", types.CapturedBool(False)),
          #("closedAt", types.CapturedNull),
          #("cancelledAt", types.CapturedNull),
          #("cancelReason", types.CapturedNull),
          #("displayFinancialStatus", types.CapturedNull),
        ]),
      ),
    ])
  let mutation =
    "
    mutation OrderCancelParity(
      $orderId: ID!
      $restock: Boolean!
      $reason: OrderCancelReason!
    ) {
      orderCancel(orderId: $orderId, restock: $restock, reason: $reason) {
        orderCancelUserErrors {
          field
          message
        }
      }
    }
  "
  let variables =
    dict.from_list([
      #("orderId", root_field.StringVal(order_id)),
      #("restock", root_field.BoolVal(False)),
      #("reason", root_field.StringVal("OTHER")),
    ])
  let outcome =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      mutation,
      variables,
      empty_upstream_context(),
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"orderCancel\":{\"orderCancelUserErrors\":[]}}}"
  assert outcome.staged_resource_ids == [order_id]
  assert list.length(outcome.log_drafts) == 1

  let read_query =
    "
    query {
      order(id: \"gid://shopify/Order/6830646329577\") {
        id
        closed
        closedAt
        cancelledAt
        cancelReason
      }
    }
  "
  let assert Ok(read) = orders.process(outcome.store, read_query, dict.new())
  assert json.to_string(read)
    == "{\"data\":{\"order\":{\"id\":\"gid://shopify/Order/6830646329577\",\"closed\":true,\"closedAt\":\"2024-01-01T00:00:00.000Z\",\"cancelledAt\":\"2024-01-01T00:00:00.000Z\",\"cancelReason\":\"OTHER\"}}}"
}

pub fn orders_order_cancel_rejects_uncancellable_states_test() {
  let cancelled_id = "gid://shopify/Order/6830646329578"
  let refunded_id = "gid://shopify/Order/6830646329579"
  let open_return_id = "gid://shopify/Order/6830646329580"
  let seeded =
    store.new()
    |> store.upsert_base_orders([
      order_cancel_test_order(
        cancelled_id,
        "PAID",
        Some("2024-02-01T00:00:00.000Z"),
        [],
      ),
      order_cancel_test_order(refunded_id, "REFUNDED", None, []),
      order_cancel_test_order(open_return_id, "PAID", None, [
        types.CapturedObject([
          #("id", types.CapturedString("gid://shopify/Return/1")),
          #("status", types.CapturedString("OPEN")),
        ]),
      ]),
    ])

  let cancelled_outcome = run_order_cancel(seed: seeded, order_id: cancelled_id)
  assert json.to_string(cancelled_outcome.data)
    == "{\"data\":{\"orderCancel\":{\"order\":null,\"job\":null,\"orderCancelUserErrors\":[{\"field\":[\"orderId\"],\"message\":\"Order has already been cancelled\",\"code\":\"INVALID\"}],\"userErrors\":[{\"field\":[\"orderId\"],\"message\":\"Order has already been cancelled\",\"code\":\"INVALID\"}]}}}"
  assert cancelled_outcome.staged_resource_ids == []
  assert cancelled_outcome.log_drafts == []

  let refunded_outcome = run_order_cancel(seed: seeded, order_id: refunded_id)
  assert json.to_string(refunded_outcome.data)
    == "{\"data\":{\"orderCancel\":{\"order\":null,\"job\":null,\"orderCancelUserErrors\":[{\"field\":[\"orderId\"],\"message\":\"Cannot cancel a refunded order\",\"code\":\"INVALID\"}],\"userErrors\":[{\"field\":[\"orderId\"],\"message\":\"Cannot cancel a refunded order\",\"code\":\"INVALID\"}]}}}"
  assert refunded_outcome.staged_resource_ids == []
  assert refunded_outcome.log_drafts == []

  let open_return_outcome =
    run_order_cancel(seed: seeded, order_id: open_return_id)
  assert json.to_string(open_return_outcome.data)
    == "{\"data\":{\"orderCancel\":{\"order\":null,\"job\":null,\"orderCancelUserErrors\":[{\"field\":[\"orderId\"],\"message\":\"Cannot cancel an order with open returns\",\"code\":\"INVALID\"}],\"userErrors\":[{\"field\":[\"orderId\"],\"message\":\"Cannot cancel an order with open returns\",\"code\":\"INVALID\"}]}}}"
  assert open_return_outcome.staged_resource_ids == []
  assert open_return_outcome.log_drafts == []
}

pub fn orders_order_cancel_rejects_invalid_arguments_test() {
  let order_id = "gid://shopify/Order/6830646329581"
  let seeded =
    store.new()
    |> store.upsert_base_orders([
      order_cancel_test_order(order_id, "PENDING", None, []),
    ])

  let refund_method_outcome =
    run_order_cancel_with_extra_args(
      seed: seeded,
      order_id: order_id,
      extra_args: ", refund: true, refundMethod: { originalPaymentMethodsRefund: true }",
    )
  assert json.to_string(refund_method_outcome.data)
    == "{\"data\":{\"orderCancel\":{\"order\":null,\"job\":null,\"orderCancelUserErrors\":[{\"field\":[\"refund\"],\"message\":\"Refund and refundMethod cannot both be present.\",\"code\":\"INVALID\"}],\"userErrors\":[{\"field\":[\"refund\"],\"message\":\"Refund and refundMethod cannot both be present.\",\"code\":\"INVALID\"}]}}}"
  assert refund_method_outcome.staged_resource_ids == []
  assert refund_method_outcome.log_drafts == []

  let staff_note_outcome =
    run_order_cancel_with_extra_args(
      seed: seeded,
      order_id: order_id,
      extra_args: ", staffNote: \"" <> string.repeat("x", times: 300) <> "\"",
    )
  assert json.to_string(staff_note_outcome.data)
    == "{\"data\":{\"orderCancel\":{\"order\":null,\"job\":null,\"orderCancelUserErrors\":[{\"field\":[\"staffNote\"],\"message\":\"Staff note is too long (maximum is 255 characters)\",\"code\":\"INVALID\"}],\"userErrors\":[{\"field\":[\"staffNote\"],\"message\":\"Staff note is too long (maximum is 255 characters)\",\"code\":\"INVALID\"}]}}}"
  assert staff_note_outcome.staged_resource_ids == []
  assert staff_note_outcome.log_drafts == []
}

pub fn orders_order_cancel_unknown_order_uses_not_found_code_test() {
  let outcome =
    run_order_cancel(seed: store.new(), order_id: "gid://shopify/Order/404")
  assert json.to_string(outcome.data)
    == "{\"data\":{\"orderCancel\":{\"order\":null,\"job\":null,\"orderCancelUserErrors\":[{\"field\":[\"orderId\"],\"message\":\"Order does not exist\",\"code\":\"NOT_FOUND\"}],\"userErrors\":[{\"field\":[\"orderId\"],\"message\":\"Order does not exist\",\"code\":\"NOT_FOUND\"}]}}}"
  assert outcome.staged_resource_ids == []
  assert outcome.log_drafts == []
}

pub fn orders_return_close_rejects_declined_return_test() {
  let outcome =
    run_return_status_mutation(
      seed: return_status_test_store("DECLINED", 0),
      root_name: "returnClose",
      return_id: "gid://shopify/Return/status-guard",
      selection: "return { id status closedAt }",
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"returnClose\":{\"return\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"Return status is invalid.\",\"code\":\"INVALID_STATE\"}]}}}"
  assert outcome.staged_resource_ids == []
  assert_return_status_read(outcome.store, "DECLINED", None)
}

pub fn orders_return_close_on_closed_return_is_idempotent_test() {
  let outcome =
    run_return_status_mutation(
      seed: return_status_test_store_with_closed_at(
        "CLOSED",
        0,
        "2026-05-01T01:00:00.000Z",
      ),
      root_name: "returnClose",
      return_id: "gid://shopify/Return/status-guard",
      selection: "return { id status closedAt order { updatedAt } }",
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"returnClose\":{\"return\":{\"id\":\"gid://shopify/Return/status-guard\",\"status\":\"CLOSED\",\"closedAt\":\"2026-05-01T01:00:00.000Z\",\"order\":{\"updatedAt\":\"2026-05-01T00:00:00.000Z\"}},\"userErrors\":[]}}}"
  assert outcome.staged_resource_ids == []
  assert outcome.log_drafts == []
}

pub fn orders_return_reopen_rejects_requested_return_test() {
  let outcome =
    run_return_status_mutation(
      seed: return_status_test_store("REQUESTED", 0),
      root_name: "returnReopen",
      return_id: "gid://shopify/Return/status-guard",
      selection: "return { id status closedAt }",
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"returnReopen\":{\"return\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"Return status is invalid.\",\"code\":\"INVALID_STATE\"}]}}}"
  assert outcome.staged_resource_ids == []
  assert_return_status_read(outcome.store, "REQUESTED", None)
}

pub fn orders_return_reopen_on_open_return_is_idempotent_test() {
  let outcome =
    run_return_status_mutation(
      seed: return_status_test_store("OPEN", 0),
      root_name: "returnReopen",
      return_id: "gid://shopify/Return/status-guard",
      selection: "return { id status closedAt order { updatedAt } }",
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"returnReopen\":{\"return\":{\"id\":\"gid://shopify/Return/status-guard\",\"status\":\"OPEN\",\"closedAt\":null,\"order\":{\"updatedAt\":\"2026-05-01T00:00:00.000Z\"}},\"userErrors\":[]}}}"
  assert outcome.staged_resource_ids == []
  assert outcome.log_drafts == []
}

pub fn orders_return_cancel_rejects_processed_return_test() {
  let outcome =
    run_return_status_mutation(
      seed: return_status_test_store("OPEN", 1),
      root_name: "returnCancel",
      return_id: "gid://shopify/Return/status-guard",
      selection: "return { id status }",
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"returnCancel\":{\"return\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"Return is not cancelable.\",\"code\":\"INVALID_STATE\"}]}}}"
  assert outcome.staged_resource_ids == []
  assert_return_status_read(outcome.store, "OPEN", None)
}

pub fn orders_return_cancel_rejects_refunded_return_test() {
  let outcome =
    run_return_status_mutation(
      seed: return_status_test_store_with_quantities("OPEN", 0, 1, ""),
      root_name: "returnCancel",
      return_id: "gid://shopify/Return/status-guard",
      selection: "return { id status }",
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"returnCancel\":{\"return\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"Return is not cancelable.\",\"code\":\"INVALID_STATE\"}]}}}"
  assert outcome.staged_resource_ids == []
  assert_return_status_read(outcome.store, "OPEN", None)
}

pub fn orders_return_cancel_on_canceled_return_is_idempotent_test() {
  let outcome =
    run_return_status_mutation(
      seed: return_status_test_store("CANCELED", 0),
      root_name: "returnCancel",
      return_id: "gid://shopify/Return/status-guard",
      selection: "return { id status order { updatedAt } }",
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"returnCancel\":{\"return\":{\"id\":\"gid://shopify/Return/status-guard\",\"status\":\"CANCELED\",\"order\":{\"updatedAt\":\"2026-05-01T00:00:00.000Z\"}},\"userErrors\":[]}}}"
  assert outcome.staged_resource_ids == []
  assert outcome.log_drafts == []
}

fn order_cancel_test_order(
  id: String,
  display_financial_status: String,
  cancelled_at: Option(String),
  returns: List(types.CapturedJsonValue),
) -> types.OrderRecord {
  types.OrderRecord(
    id: id,
    cursor: None,
    data: types.CapturedObject([
      #("id", types.CapturedString(id)),
      #("name", types.CapturedString("#1328")),
      #("closed", types.CapturedBool(False)),
      #("closedAt", types.CapturedNull),
      #("cancelledAt", case cancelled_at {
        Some(timestamp) -> types.CapturedString(timestamp)
        None -> types.CapturedNull
      }),
      #("cancelReason", types.CapturedNull),
      #(
        "displayFinancialStatus",
        types.CapturedString(display_financial_status),
      ),
      #("returns", types.CapturedArray(returns)),
    ]),
  )
}

fn run_order_cancel(seed seed: store.Store, order_id order_id: String) {
  run_order_cancel_with_extra_args(
    seed: seed,
    order_id: order_id,
    extra_args: "",
  )
}

fn run_order_cancel_with_extra_args(
  seed seed: store.Store,
  order_id order_id: String,
  extra_args extra_args: String,
) {
  let mutation = "
    mutation {
      orderCancel(orderId: \"" <> order_id <> "\", restock: true, reason: OTHER" <> extra_args <> ") {
        order {
          id
        }
        job {
          id
          done
        }
        orderCancelUserErrors {
          field
          message
          code
        }
        userErrors {
          field
          message
          code
        }
      }
    }
  "
  orders.process_mutation(
    seed,
    synthetic_identity.new(),
    "/admin/api/2026-04/graphql.json",
    mutation,
    dict.new(),
    empty_upstream_context(),
  )
}

fn return_status_test_store(
  status: String,
  processed_quantity: Int,
) -> store.Store {
  return_status_test_store_with_quantities(status, processed_quantity, 0, "")
}

fn return_status_test_store_with_closed_at(
  status: String,
  processed_quantity: Int,
  closed_at: String,
) -> store.Store {
  return_status_test_store_with_quantities(
    status,
    processed_quantity,
    0,
    closed_at,
  )
}

fn return_status_test_store_with_quantities(
  status: String,
  processed_quantity: Int,
  refunded_quantity: Int,
  closed_at: String,
) -> store.Store {
  let order_id = "gid://shopify/Order/status-guard"
  store.new()
  |> store.upsert_base_orders([
    types.OrderRecord(
      id: order_id,
      cursor: None,
      data: types.CapturedObject([
        #("id", types.CapturedString(order_id)),
        #("name", types.CapturedString("#STATUS-GUARD")),
        #("updatedAt", types.CapturedString("2026-05-01T00:00:00.000Z")),
        #(
          "returns",
          types.CapturedArray([
            types.CapturedObject([
              #("id", types.CapturedString("gid://shopify/Return/status-guard")),
              #("name", types.CapturedString("#STATUS-GUARD-R1")),
              #("status", types.CapturedString(status)),
              #("closedAt", case closed_at {
                "" -> types.CapturedNull
                value -> types.CapturedString(value)
              }),
              #("totalQuantity", types.CapturedInt(1)),
              #(
                "returnLineItems",
                types.CapturedObject([
                  #(
                    "nodes",
                    types.CapturedArray([
                      types.CapturedObject([
                        #(
                          "id",
                          types.CapturedString(
                            "gid://shopify/ReturnLineItem/status-guard",
                          ),
                        ),
                        #("quantity", types.CapturedInt(1)),
                        #(
                          "processedQuantity",
                          types.CapturedInt(processed_quantity),
                        ),
                        #(
                          "refundedQuantity",
                          types.CapturedInt(refunded_quantity),
                        ),
                      ]),
                    ]),
                  ),
                ]),
              ),
            ]),
          ]),
        ),
      ]),
    ),
  ])
}

fn run_return_status_mutation(
  seed seed: store.Store,
  root_name root_name: String,
  return_id return_id: String,
  selection selection: String,
) -> MutationOutcome {
  orders.process_mutation(
    seed,
    synthetic_identity.new(),
    "/admin/api/2025-01/graphql.json",
    "mutation {
      " <> root_name <> "(id: \"" <> return_id <> "\") {
        " <> selection <> "
        userErrors { field message code }
      }
    }",
    dict.new(),
    empty_upstream_context(),
  )
}

fn assert_return_status_read(
  seed: store.Store,
  status: String,
  closed_at: Option(String),
) {
  let assert Ok(result) =
    orders.process(
      seed,
      "
        query {
          return(id: \"gid://shopify/Return/status-guard\") {
            id
            status
            closedAt
            order { updatedAt }
          }
        }
      ",
      dict.new(),
    )
  let closed_at_json = case closed_at {
    Some(value) -> "\"" <> value <> "\""
    None -> "null"
  }
  assert json.to_string(result)
    == "{\"data\":{\"return\":{\"id\":\"gid://shopify/Return/status-guard\",\"status\":\""
    <> status
    <> "\",\"closedAt\":"
    <> closed_at_json
    <> ",\"order\":{\"updatedAt\":\"2026-05-01T00:00:00.000Z\"}}}}"
}

pub fn orders_order_invoice_send_payload_test() {
  let order_id = "gid://shopify/Order/6830646329577"
  let seeded =
    store.new()
    |> store.upsert_base_orders([
      types.OrderRecord(
        id: order_id,
        cursor: None,
        data: types.CapturedObject([
          #("id", types.CapturedString(order_id)),
          #("name", types.CapturedString("#1328")),
          #("email", types.CapturedString("order-recipient@example.test")),
          #("closed", types.CapturedBool(False)),
          #("closedAt", types.CapturedNull),
          #("cancelledAt", types.CapturedNull),
          #("cancelReason", types.CapturedNull),
          #("displayFinancialStatus", types.CapturedNull),
        ]),
      ),
    ])
  let mutation =
    "
    mutation OrderInvoiceSendParity($id: ID!) {
      orderInvoiceSend(id: $id) {
        order {
          id
          name
          closed
          closedAt
          cancelledAt
          cancelReason
          displayFinancialStatus
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  let variables = dict.from_list([#("id", root_field.StringVal(order_id))])
  let outcome =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      mutation,
      variables,
      empty_upstream_context(),
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"orderInvoiceSend\":{\"order\":{\"id\":\"gid://shopify/Order/6830646329577\",\"name\":\"#1328\",\"closed\":false,\"closedAt\":null,\"cancelledAt\":null,\"cancelReason\":null,\"displayFinancialStatus\":null},\"userErrors\":[]}}}"
  assert outcome.staged_resource_ids == []
  assert outcome.log_drafts == []
}

pub fn orders_order_invoice_send_rejects_empty_recipient_test() {
  let order_id = "gid://shopify/Order/6830646329580"
  let seeded =
    store.new()
    |> store.upsert_base_orders([
      types.OrderRecord(
        id: order_id,
        cursor: None,
        data: types.CapturedObject([
          #("id", types.CapturedString(order_id)),
          #("name", types.CapturedString("#1330")),
          #("email", types.CapturedNull),
          #("customer", types.CapturedNull),
        ]),
      ),
    ])
  let outcome =
    run_order_invoice_send_validation_case(seeded, order_id, dict.new())
  assert json.to_string(outcome.data)
    == "{\"data\":{\"orderInvoiceSend\":{\"order\":null,\"userErrors\":[{\"field\":null,\"message\":\"No recipient email address was provided\",\"code\":\"ORDER_INVOICE_SEND_UNSUCCESSFUL\"}]}}}"
  assert outcome.staged_resource_ids == []
  assert outcome.log_drafts == []
}

pub fn orders_order_invoice_send_rejects_invalid_explicit_email_test() {
  let order_id = "gid://shopify/Order/6830646329581"
  let seeded =
    store.new()
    |> store.upsert_base_orders([
      types.OrderRecord(
        id: order_id,
        cursor: None,
        data: types.CapturedObject([
          #("id", types.CapturedString(order_id)),
          #("name", types.CapturedString("#1331")),
          #("email", types.CapturedString("order-recipient@example.test")),
        ]),
      ),
    ])
  let variables =
    dict.from_list([
      #(
        "email",
        root_field.ObjectVal(
          dict.from_list([
            #("to", root_field.StringVal("not an email")),
          ]),
        ),
      ),
    ])
  let outcome =
    run_order_invoice_send_validation_case(seeded, order_id, variables)
  assert json.to_string(outcome.data)
    == "{\"data\":{\"orderInvoiceSend\":{\"order\":null,\"userErrors\":[{\"field\":null,\"message\":\"To is invalid\",\"code\":\"ORDER_INVOICE_SEND_UNSUCCESSFUL\"}]}}}"
  assert outcome.staged_resource_ids == []
  assert outcome.log_drafts == []
}

pub fn orders_order_invoice_send_customer_email_recipient_test() {
  let order_id = "gid://shopify/Order/6830646329582"
  let seeded =
    store.new()
    |> store.upsert_base_orders([
      types.OrderRecord(
        id: order_id,
        cursor: None,
        data: types.CapturedObject([
          #("id", types.CapturedString(order_id)),
          #("name", types.CapturedString("#1332")),
          #("email", types.CapturedNull),
          #(
            "customer",
            types.CapturedObject([
              #(
                "email",
                types.CapturedString("customer-recipient@example.test"),
              ),
            ]),
          ),
        ]),
      ),
    ])
  let outcome =
    run_order_invoice_send_validation_case(seeded, order_id, dict.new())
  assert json.to_string(outcome.data)
    == "{\"data\":{\"orderInvoiceSend\":{\"order\":{\"id\":\"gid://shopify/Order/6830646329582\",\"name\":\"#1332\"},\"userErrors\":[]}}}"
  assert outcome.staged_resource_ids == []
  assert outcome.log_drafts == []
}

fn run_order_invoice_send_validation_case(
  seeded: store.Store,
  order_id: String,
  variables: Dict(String, root_field.ResolvedValue),
) {
  let mutation = "
    mutation OrderInvoiceSendValidation($email: EmailInput) {
      orderInvoiceSend(id: \"" <> order_id <> "\", email: $email) {
        order {
          id
          name
        }
        userErrors {
          field
          message
          code
        }
      }
    }
  "
  orders.process_mutation(
    seeded,
    synthetic_identity.new(),
    "/admin/api/2026-04/graphql.json",
    mutation,
    variables,
    empty_upstream_context(),
  )
}

pub fn orders_order_mark_as_paid_read_after_write_test() {
  let order_id = "gid://shopify/Order/6830647771369"
  let seeded =
    store.new()
    |> store.upsert_base_orders([
      types.OrderRecord(
        id: order_id,
        cursor: None,
        data: types.CapturedObject([
          #("id", types.CapturedString(order_id)),
          #("name", types.CapturedString("#1329")),
          #("displayFinancialStatus", types.CapturedString("PENDING")),
          #("paymentGatewayNames", types.CapturedArray([])),
          #(
            "totalOutstandingSet",
            types.CapturedObject([
              #(
                "shopMoney",
                types.CapturedObject([
                  #("amount", types.CapturedString("19.0")),
                  #("currencyCode", types.CapturedString("CAD")),
                ]),
              ),
            ]),
          ),
          #(
            "currentTotalPriceSet",
            types.CapturedObject([
              #(
                "shopMoney",
                types.CapturedObject([
                  #("amount", types.CapturedString("19.0")),
                  #("currencyCode", types.CapturedString("CAD")),
                ]),
              ),
            ]),
          ),
          #("transactions", types.CapturedArray([])),
        ]),
      ),
    ])
  let mutation =
    "
    mutation OrderMarkAsPaidParity($input: OrderMarkAsPaidInput!) {
      orderMarkAsPaid(input: $input) {
        order {
          id
          displayFinancialStatus
          paymentGatewayNames
          totalOutstandingSet {
            shopMoney {
              amount
              currencyCode
            }
            presentmentMoney {
              amount
              currencyCode
            }
          }
          transactions {
            kind
            status
            gateway
            amountSet {
              shopMoney {
                amount
                currencyCode
              }
              presentmentMoney {
                amount
                currencyCode
              }
            }
          }
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  let variables =
    dict.from_list([
      #(
        "input",
        root_field.ObjectVal(
          dict.from_list([
            #("id", root_field.StringVal(order_id)),
          ]),
        ),
      ),
    ])
  let outcome =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      mutation,
      variables,
      empty_upstream_context(),
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"orderMarkAsPaid\":{\"order\":{\"id\":\"gid://shopify/Order/6830647771369\",\"displayFinancialStatus\":\"PAID\",\"paymentGatewayNames\":[\"manual\"],\"totalOutstandingSet\":{\"shopMoney\":{\"amount\":\"0.0\",\"currencyCode\":\"CAD\"},\"presentmentMoney\":{\"amount\":\"0.0\",\"currencyCode\":\"CAD\"}},\"transactions\":[{\"kind\":\"SALE\",\"status\":\"SUCCESS\",\"gateway\":\"manual\",\"amountSet\":{\"shopMoney\":{\"amount\":\"19.0\",\"currencyCode\":\"CAD\"},\"presentmentMoney\":{\"amount\":\"19.0\",\"currencyCode\":\"CAD\"}}}]},\"userErrors\":[]}}}"
  assert outcome.staged_resource_ids == [order_id]
  assert list.length(outcome.log_drafts) == 1

  let read_query =
    "
    query {
      order(id: \"gid://shopify/Order/6830647771369\") {
        id
        displayFinancialStatus
        paymentGatewayNames
        totalOutstandingSet {
          shopMoney {
            amount
            currencyCode
          }
          presentmentMoney {
            amount
            currencyCode
          }
        }
        transactions {
          kind
          status
          gateway
          amountSet {
            shopMoney {
              amount
              currencyCode
            }
            presentmentMoney {
              amount
              currencyCode
            }
          }
        }
      }
    }
  "
  let assert Ok(read) = orders.process(outcome.store, read_query, dict.new())
  assert json.to_string(read)
    == "{\"data\":{\"order\":{\"id\":\"gid://shopify/Order/6830647771369\",\"displayFinancialStatus\":\"PAID\",\"paymentGatewayNames\":[\"manual\"],\"totalOutstandingSet\":{\"shopMoney\":{\"amount\":\"0.0\",\"currencyCode\":\"CAD\"},\"presentmentMoney\":{\"amount\":\"0.0\",\"currencyCode\":\"CAD\"}},\"transactions\":[{\"kind\":\"SALE\",\"status\":\"SUCCESS\",\"gateway\":\"manual\",\"amountSet\":{\"shopMoney\":{\"amount\":\"19.0\",\"currencyCode\":\"CAD\"},\"presentmentMoney\":{\"amount\":\"19.0\",\"currencyCode\":\"CAD\"}}}]}}}"
}

pub fn orders_order_mark_as_paid_invalid_states_do_not_stage_test() {
  let cases = [
    #("PAID", "Order cannot be marked as paid."),
    #("REFUNDED", "Order cannot be marked as paid."),
    #("PARTIALLY_REFUNDED", "Order cannot be marked as paid."),
    #("VOIDED", "Order cannot be marked as paid."),
  ]
  list.each(cases, fn(item) {
    let #(status, message) = item
    let order_id = "gid://shopify/Order/mark-as-paid-" <> status
    let seeded =
      store.new()
      |> store.upsert_base_orders([
        mark_as_paid_order_record(
          order_id,
          status,
          types.CapturedNull,
          "19.0",
          "CAD",
          None,
        ),
      ])
    let outcome =
      orders.process_mutation(
        seeded,
        synthetic_identity.new(),
        "/admin/api/2026-04/graphql.json",
        mark_as_paid_validation_mutation(),
        mark_as_paid_variables(order_id),
        empty_upstream_context(),
      )
    assert json.to_string(outcome.data)
      == "{\"data\":{\"orderMarkAsPaid\":{\"order\":{\"id\":\""
      <> order_id
      <> "\",\"displayFinancialStatus\":\""
      <> status
      <> "\",\"paymentGatewayNames\":[\"previous\"],\"transactions\":[]},\"userErrors\":[{\"field\":[\"id\"],\"message\":\""
      <> message
      <> "\",\"code\":\"INVALID\"}]}}}"
    assert outcome.staged_resource_ids == []
    assert outcome.log_drafts == []
  })
}

pub fn orders_order_mark_as_paid_cancelled_order_does_not_stage_test() {
  let order_id = "gid://shopify/Order/mark-as-paid-cancelled"
  let seeded =
    store.new()
    |> store.upsert_base_orders([
      mark_as_paid_order_record(
        order_id,
        "PENDING",
        types.CapturedString("2024-01-02T00:00:00.000Z"),
        "19.0",
        "CAD",
        None,
      ),
    ])
  let outcome =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      mark_as_paid_validation_mutation(),
      mark_as_paid_variables(order_id),
      empty_upstream_context(),
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"orderMarkAsPaid\":{\"order\":{\"id\":\"gid://shopify/Order/mark-as-paid-cancelled\",\"displayFinancialStatus\":\"PENDING\",\"paymentGatewayNames\":[\"previous\"],\"transactions\":[]},\"userErrors\":[{\"field\":[\"id\"],\"message\":\"Order is cancelled and cannot be marked paid\",\"code\":\"INVALID\"}]}}}"
  assert outcome.staged_resource_ids == []
  assert outcome.log_drafts == []
}

pub fn orders_order_mark_as_paid_multi_currency_uses_presentment_currency_test() {
  let order_id = "gid://shopify/Order/mark-as-paid-multi-currency"
  let seeded =
    store.new()
    |> store.upsert_base_orders([
      mark_as_paid_order_record(
        order_id,
        "PENDING",
        types.CapturedNull,
        "12.5",
        "USD",
        Some("CAD"),
      ),
    ])
  let outcome =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      mark_as_paid_money_bag_mutation(),
      mark_as_paid_variables(order_id),
      empty_upstream_context(),
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"orderMarkAsPaid\":{\"order\":{\"id\":\"gid://shopify/Order/mark-as-paid-multi-currency\",\"presentmentCurrencyCode\":\"CAD\",\"displayFinancialStatus\":\"PAID\",\"totalOutstandingSet\":{\"shopMoney\":{\"amount\":\"0.0\",\"currencyCode\":\"USD\"},\"presentmentMoney\":{\"amount\":\"0.0\",\"currencyCode\":\"CAD\"}},\"transactions\":[{\"amountSet\":{\"shopMoney\":{\"amount\":\"12.5\",\"currencyCode\":\"USD\"},\"presentmentMoney\":{\"amount\":\"12.5\",\"currencyCode\":\"CAD\"}}}]},\"userErrors\":[]}}}"
  assert outcome.staged_resource_ids == [order_id]
  assert list.length(outcome.log_drafts) == 1
}

pub fn orders_order_capture_currency_validation_uses_presentment_currency_test() {
  let order_id = "gid://shopify/Order/capture-multi-currency"
  let authorization_id =
    "gid://shopify/OrderTransaction/capture-multi-currency-auth"
  let seeded =
    store.new()
    |> store.upsert_base_orders([
      order_capture_test_order(
        order_id,
        authorization_id,
        "AUTHORIZATION",
        "SUCCESS",
        "CAD",
        "USD",
      ),
    ])

  let missing_currency =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      order_capture_validation_mutation(),
      order_capture_variables(order_id, authorization_id, "10.00", None, None),
      empty_upstream_context(),
    )
  assert json.to_string(missing_currency.data)
    == "{\"data\":{\"orderCapture\":{\"userErrors\":[{\"field\":[\"currency\"],\"message\":\"Currency must be provided for multi-currency orders.\",\"code\":\"CURRENCY_REQUIRED\"}]}}}"
  assert missing_currency.staged_resource_ids == []
  assert list.length(missing_currency.log_drafts) == 1

  let mismatch =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      order_capture_validation_mutation(),
      order_capture_variables(
        order_id,
        authorization_id,
        "10.00",
        Some("CAD"),
        None,
      ),
      empty_upstream_context(),
    )
  assert json.to_string(mismatch.data)
    == "{\"data\":{\"orderCapture\":{\"userErrors\":[{\"field\":[\"currency\"],\"message\":\"Currency must match the order presentment currency.\",\"code\":\"CURRENCY_MISMATCH\"}]}}}"
  assert mismatch.staged_resource_ids == []
  assert list.length(mismatch.log_drafts) == 1
}

pub fn orders_order_capture_parent_and_amount_errors_have_codes_test() {
  let order_id = "gid://shopify/Order/capture-error-codes"
  let authorization_id = "gid://shopify/OrderTransaction/capture-error-auth"
  let settled_id = "gid://shopify/OrderTransaction/capture-error-settled"
  let seeded =
    store.new()
    |> store.upsert_base_orders([
      order_capture_test_order(
        order_id,
        authorization_id,
        "AUTHORIZATION",
        "SUCCESS",
        "CAD",
        "CAD",
      )
      |> order_capture_append_transaction(
        settled_id,
        "CAPTURE",
        "SUCCESS",
        "5.0",
        "CAD",
        "CAD",
      ),
    ])

  let missing_parent =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      order_capture_validation_mutation(),
      order_capture_variables(
        order_id,
        "gid://shopify/OrderTransaction/missing",
        "10.00",
        Some("CAD"),
        None,
      ),
      empty_upstream_context(),
    )
  assert json.to_string(missing_parent.data)
    == "{\"data\":{\"orderCapture\":{\"userErrors\":[{\"field\":[\"parent_transaction_id\"],\"message\":\"Transaction does not exist\",\"code\":\"TRANSACTION_NOT_FOUND\"}]}}}"

  let invalid_parent_state =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      order_capture_validation_mutation(),
      order_capture_variables(order_id, settled_id, "1.00", Some("CAD"), None),
      empty_upstream_context(),
    )
  assert json.to_string(invalid_parent_state.data)
    == "{\"data\":{\"orderCapture\":{\"userErrors\":[{\"field\":[\"parent_transaction_id\"],\"message\":\"Transaction is not capturable\",\"code\":\"INVALID_TRANSACTION_STATE\"}]}}}"

  let invalid_amount =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      order_capture_validation_mutation(),
      order_capture_variables(
        order_id,
        authorization_id,
        "0.00",
        Some("CAD"),
        None,
      ),
      empty_upstream_context(),
    )
  assert json.to_string(invalid_amount.data)
    == "{\"data\":{\"orderCapture\":{\"userErrors\":[{\"field\":[\"amount\"],\"message\":\"Amount must be greater than zero\",\"code\":\"INVALID_AMOUNT\"}]}}}"

  let over_capture =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      order_capture_validation_mutation(),
      order_capture_variables(
        order_id,
        authorization_id,
        "30.00",
        Some("CAD"),
        None,
      ),
      empty_upstream_context(),
    )
  assert json.to_string(over_capture.data)
    == "{\"data\":{\"orderCapture\":{\"userErrors\":[{\"field\":[\"amount\"],\"message\":\"Amount exceeds capturable amount\",\"code\":\"OVER_CAPTURE\"}]}}}"
}

pub fn orders_order_capture_final_capture_closes_authorization_test() {
  let order_id = "gid://shopify/Order/capture-final"
  let authorization_id = "gid://shopify/OrderTransaction/capture-final-auth"
  let seeded =
    store.new()
    |> store.upsert_base_orders([
      order_capture_test_order(
        order_id,
        authorization_id,
        "AUTHORIZATION",
        "SUCCESS",
        "CAD",
        "CAD",
      ),
    ])
  let first_capture =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      order_capture_validation_mutation(),
      order_capture_variables(
        order_id,
        authorization_id,
        "10.00",
        Some("CAD"),
        Some(True),
      ),
      empty_upstream_context(),
    )
  assert json.to_string(first_capture.data)
    == "{\"data\":{\"orderCapture\":{\"userErrors\":[]}}}"
  assert first_capture.staged_resource_ids == [order_id]
  assert list.length(first_capture.log_drafts) == 1

  let second_capture =
    orders.process_mutation(
      first_capture.store,
      first_capture.identity,
      "/admin/api/2026-04/graphql.json",
      order_capture_validation_mutation(),
      order_capture_variables(
        order_id,
        authorization_id,
        "1.00",
        Some("CAD"),
        None,
      ),
      empty_upstream_context(),
    )
  assert json.to_string(second_capture.data)
    == "{\"data\":{\"orderCapture\":{\"userErrors\":[{\"field\":[\"parent_transaction_id\"],\"message\":\"Transaction is not capturable\",\"code\":\"INVALID_TRANSACTION_STATE\"}]}}}"
  assert second_capture.staged_resource_ids == []
}

fn order_capture_validation_mutation() {
  "
  mutation Capture($input: OrderCaptureInput!) {
    orderCapture(input: $input) {
      userErrors {
        field
        message
        code
      }
    }
  }
  "
}

fn order_capture_variables(
  order_id: String,
  authorization_id: String,
  amount: String,
  currency: Option(String),
  final_capture: Option(Bool),
) -> Dict(String, root_field.ResolvedValue) {
  let currency_fields = case currency {
    Some(value) -> [#("currency", root_field.StringVal(value))]
    None -> []
  }
  let final_capture_fields = case final_capture {
    Some(value) -> [#("finalCapture", root_field.BoolVal(value))]
    None -> []
  }
  dict.from_list([
    #(
      "input",
      root_field.ObjectVal(
        dict.from_list(list.append(
          [
            #("id", root_field.StringVal(order_id)),
            #("parentTransactionId", root_field.StringVal(authorization_id)),
            #("amount", root_field.StringVal(amount)),
          ],
          list.append(currency_fields, final_capture_fields),
        )),
      ),
    ),
  ])
}

fn order_capture_test_order(
  order_id: String,
  authorization_id: String,
  authorization_kind: String,
  authorization_status: String,
  shop_currency: String,
  presentment_currency: String,
) -> types.OrderRecord {
  types.OrderRecord(
    id: order_id,
    cursor: None,
    data: types.CapturedObject([
      #("id", types.CapturedString(order_id)),
      #("presentmentCurrencyCode", types.CapturedString(presentment_currency)),
      #("displayFinancialStatus", types.CapturedString("AUTHORIZED")),
      #(
        "paymentGatewayNames",
        types.CapturedArray([types.CapturedString("manual")]),
      ),
      #(
        "currentTotalPriceSet",
        order_capture_money_set(
          "25.0",
          shop_currency,
          "25.0",
          presentment_currency,
        ),
      ),
      #(
        "totalPriceSet",
        order_capture_money_set(
          "25.0",
          shop_currency,
          "25.0",
          presentment_currency,
        ),
      ),
      #(
        "totalOutstandingSet",
        order_capture_money_set(
          "25.0",
          shop_currency,
          "25.0",
          presentment_currency,
        ),
      ),
      #(
        "transactions",
        types.CapturedArray([
          order_capture_transaction(
            authorization_id,
            authorization_kind,
            authorization_status,
            "25.0",
            shop_currency,
            presentment_currency,
          ),
        ]),
      ),
    ]),
  )
}

fn order_capture_append_transaction(
  order: types.OrderRecord,
  transaction_id: String,
  kind: String,
  status: String,
  amount: String,
  shop_currency: String,
  presentment_currency: String,
) -> types.OrderRecord {
  let transactions = case order.data {
    types.CapturedObject(fields) ->
      fields
      |> list.find_map(fn(field) {
        case field {
          #("transactions", types.CapturedArray(values)) -> Ok(values)
          _ -> Error(Nil)
        }
      })
      |> result.unwrap([])
    _ -> []
  }
  let updated_data = case order.data {
    types.CapturedObject(fields) ->
      types.CapturedObject(
        list.map(fields, fn(field) {
          case field {
            #("transactions", types.CapturedArray(_)) -> #(
              "transactions",
              types.CapturedArray(
                list.append(transactions, [
                  order_capture_transaction(
                    transaction_id,
                    kind,
                    status,
                    amount,
                    shop_currency,
                    presentment_currency,
                  ),
                ]),
              ),
            )
            other -> other
          }
        }),
      )
    other -> other
  }
  types.OrderRecord(..order, data: updated_data)
}

fn order_capture_transaction(
  transaction_id: String,
  kind: String,
  status: String,
  amount: String,
  shop_currency: String,
  presentment_currency: String,
) -> types.CapturedJsonValue {
  types.CapturedObject([
    #("id", types.CapturedString(transaction_id)),
    #("kind", types.CapturedString(kind)),
    #("status", types.CapturedString(status)),
    #("gateway", types.CapturedString("manual")),
    #(
      "amountSet",
      order_capture_money_set(
        amount,
        shop_currency,
        amount,
        presentment_currency,
      ),
    ),
    #("parentTransactionId", types.CapturedNull),
    #("parentTransaction", types.CapturedNull),
  ])
}

fn order_capture_money_set(
  amount: String,
  shop_currency: String,
  presentment_amount: String,
  presentment_currency: String,
) -> types.CapturedJsonValue {
  types.CapturedObject([
    #(
      "shopMoney",
      types.CapturedObject([
        #("amount", types.CapturedString(amount)),
        #("currencyCode", types.CapturedString(shop_currency)),
      ]),
    ),
    #(
      "presentmentMoney",
      types.CapturedObject([
        #("amount", types.CapturedString(presentment_amount)),
        #("currencyCode", types.CapturedString(presentment_currency)),
      ]),
    ),
  ])
}

fn mark_as_paid_order_record(
  order_id: String,
  display_financial_status: String,
  cancelled_at: types.CapturedJsonValue,
  amount: String,
  currency_code: String,
  presentment_currency_code: Option(String),
) -> types.OrderRecord {
  let presentment_fields = case presentment_currency_code {
    Some(value) -> [#("presentmentCurrencyCode", types.CapturedString(value))]
    None -> []
  }
  types.OrderRecord(
    id: order_id,
    cursor: None,
    data: types.CapturedObject(list.append(
      [
        #("id", types.CapturedString(order_id)),
        #(
          "displayFinancialStatus",
          types.CapturedString(display_financial_status),
        ),
        #("cancelledAt", cancelled_at),
        #(
          "paymentGatewayNames",
          types.CapturedArray([types.CapturedString("previous")]),
        ),
        #(
          "totalOutstandingSet",
          types.CapturedObject([
            #(
              "shopMoney",
              types.CapturedObject([
                #("amount", types.CapturedString(amount)),
                #("currencyCode", types.CapturedString(currency_code)),
              ]),
            ),
          ]),
        ),
        #(
          "currentTotalPriceSet",
          types.CapturedObject([
            #(
              "shopMoney",
              types.CapturedObject([
                #("amount", types.CapturedString(amount)),
                #("currencyCode", types.CapturedString(currency_code)),
              ]),
            ),
          ]),
        ),
        #("transactions", types.CapturedArray([])),
      ],
      presentment_fields,
    )),
  )
}

fn mark_as_paid_variables(
  order_id: String,
) -> Dict(String, root_field.ResolvedValue) {
  dict.from_list([
    #(
      "input",
      root_field.ObjectVal(
        dict.from_list([#("id", root_field.StringVal(order_id))]),
      ),
    ),
  ])
}

fn mark_as_paid_validation_mutation() -> String {
  "
    mutation OrderMarkAsPaidValidation($input: OrderMarkAsPaidInput!) {
      orderMarkAsPaid(input: $input) {
        order {
          id
          displayFinancialStatus
          paymentGatewayNames
          transactions {
            kind
          }
        }
        userErrors {
          field
          message
          code
        }
      }
    }
  "
}

fn mark_as_paid_money_bag_mutation() -> String {
  "
    mutation OrderMarkAsPaidMoneyBag($input: OrderMarkAsPaidInput!) {
      orderMarkAsPaid(input: $input) {
        order {
          id
          presentmentCurrencyCode
          displayFinancialStatus
          totalOutstandingSet {
            shopMoney {
              amount
              currencyCode
            }
            presentmentMoney {
              amount
              currencyCode
            }
          }
          transactions {
            amountSet {
              shopMoney {
                amount
                currencyCode
              }
              presentmentMoney {
                amount
                currencyCode
              }
            }
          }
        }
        userErrors {
          field
          message
          code
        }
      }
    }
  "
}

pub fn orders_order_edit_missing_id_validation_guardrails_test() {
  let expected =
    "{\"errors\":[{\"message\":\"Variable $id of type ID! was provided invalid value\",\"extensions\":{\"code\":\"INVALID_VARIABLE\",\"value\":null,\"problems\":[{\"path\":[],\"explanation\":\"Expected value to not be null\"}]}}]}"

  let begin =
    "
    mutation OrderEditBeginMissingId($id: ID!) {
      orderEditBegin(id: $id) {
        calculatedOrder {
          id
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  assert order_edit_missing_id_json(begin, dict.new()) == expected

  let add_variant =
    "
    mutation OrderEditAddVariantMissingId($id: ID!, $variantId: ID!, $quantity: Int!) {
      orderEditAddVariant(id: $id, variantId: $variantId, quantity: $quantity) {
        calculatedOrder {
          id
        }
        calculatedLineItem {
          id
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  assert order_edit_missing_id_json(
      add_variant,
      dict.from_list([
        #("variantId", root_field.StringVal("gid://shopify/ProductVariant/0")),
        #("quantity", root_field.IntVal(1)),
      ]),
    )
    == expected

  let set_quantity =
    "
    mutation OrderEditSetQuantityMissingId($id: ID!, $lineItemId: ID!, $quantity: Int!) {
      orderEditSetQuantity(id: $id, lineItemId: $lineItemId, quantity: $quantity) {
        calculatedOrder {
          id
        }
        calculatedLineItem {
          id
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  assert order_edit_missing_id_json(
      set_quantity,
      dict.from_list([
        #(
          "lineItemId",
          root_field.StringVal("gid://shopify/CalculatedLineItem/0"),
        ),
        #("quantity", root_field.IntVal(1)),
      ]),
    )
    == expected

  let commit =
    "
    mutation OrderEditCommitMissingId($id: ID!, $notifyCustomer: Boolean, $staffNote: String) {
      orderEditCommit(id: $id, notifyCustomer: $notifyCustomer, staffNote: $staffNote) {
        order {
          id
          name
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  assert order_edit_missing_id_json(
      commit,
      dict.from_list([
        #("notifyCustomer", root_field.BoolVal(False)),
        #("staffNote", root_field.StringVal("missing id probe")),
      ]),
    )
    == expected
}

fn order_edit_missing_id_json(
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> String {
  let outcome =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      document,
      variables,
      empty_upstream_context(),
    )
  json.to_string(outcome.data)
}

pub fn orders_draft_order_create_custom_item_read_after_write_test() {
  let mutation =
    "
    mutation {
      draftOrderCreate(input: {
        email: \"draft@example.test\"
        note: \"Phone order\"
        tags: [\"beta\", \"alpha\"]
        lineItems: [{
          title: \"Custom service\"
          quantity: 2
          originalUnitPrice: \"20.00\"
          requiresShipping: false
          taxable: false
          sku: \"CUSTOM\"
          appliedDiscount: {
            title: \"Service discount\"
            description: \"10 percent off\"
            value: 10
            amount: 4
            valueType: PERCENTAGE
          }
          customAttributes: [{ key: \"appointment\", value: \"morning\" }]
        }]
        shippingLine: {
          title: \"Courier\"
          priceWithCurrency: { amount: \"7.25\", currencyCode: CAD }
        }
      }) {
        draftOrder {
          id
          name
          status
          email
          note
          tags
          totalQuantityOfLineItems
          subtotalPriceSet { shopMoney { amount currencyCode } }
          totalDiscountsSet { shopMoney { amount currencyCode } }
          totalShippingPriceSet { shopMoney { amount currencyCode } }
          totalPriceSet { shopMoney { amount currencyCode } }
          lineItems {
            nodes {
              id
              title
              name
              quantity
              sku
              custom
              requiresShipping
              taxable
              variantTitle
              variant { id }
              originalUnitPriceSet { shopMoney { amount currencyCode } }
              originalTotalSet { shopMoney { amount currencyCode } }
              discountedTotalSet { shopMoney { amount currencyCode } }
              totalDiscountSet { shopMoney { amount currencyCode } }
            }
          }
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  let outcome =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      mutation,
      dict.new(),
      empty_upstream_context(),
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"draftOrderCreate\":{\"draftOrder\":{\"id\":\"gid://shopify/DraftOrder/1\",\"name\":\"#D1\",\"status\":\"OPEN\",\"email\":\"draft@example.test\",\"note\":\"Phone order\",\"tags\":[\"alpha\",\"beta\"],\"totalQuantityOfLineItems\":2,\"subtotalPriceSet\":{\"shopMoney\":{\"amount\":\"36.0\",\"currencyCode\":\"CAD\"}},\"totalDiscountsSet\":{\"shopMoney\":{\"amount\":\"4.0\",\"currencyCode\":\"CAD\"}},\"totalShippingPriceSet\":{\"shopMoney\":{\"amount\":\"7.25\",\"currencyCode\":\"CAD\"}},\"totalPriceSet\":{\"shopMoney\":{\"amount\":\"43.25\",\"currencyCode\":\"CAD\"}},\"lineItems\":{\"nodes\":[{\"id\":\"gid://shopify/DraftOrderLineItem/2\",\"title\":\"Custom service\",\"name\":\"Custom service\",\"quantity\":2,\"sku\":\"CUSTOM\",\"custom\":true,\"requiresShipping\":false,\"taxable\":false,\"variantTitle\":null,\"variant\":null,\"originalUnitPriceSet\":{\"shopMoney\":{\"amount\":\"20.0\",\"currencyCode\":\"CAD\"}},\"originalTotalSet\":{\"shopMoney\":{\"amount\":\"40.0\",\"currencyCode\":\"CAD\"}},\"discountedTotalSet\":{\"shopMoney\":{\"amount\":\"36.0\",\"currencyCode\":\"CAD\"}},\"totalDiscountSet\":{\"shopMoney\":{\"amount\":\"4.0\",\"currencyCode\":\"CAD\"}}}]}},\"userErrors\":[]}}}"
  assert outcome.staged_resource_ids == ["gid://shopify/DraftOrder/1"]
  assert list.length(outcome.log_drafts) == 1

  let read_query =
    "
    query {
      draftOrder(id: \"gid://shopify/DraftOrder/1\") {
        id
        name
        status
        email
        note
        tags
        totalQuantityOfLineItems
        totalPriceSet {
          shopMoney {
            amount
            currencyCode
          }
        }
      }
    }
  "
  let assert Ok(read_result) =
    orders.process(outcome.store, read_query, dict.new())
  assert json.to_string(read_result)
    == "{\"data\":{\"draftOrder\":{\"id\":\"gid://shopify/DraftOrder/1\",\"name\":\"#D1\",\"status\":\"OPEN\",\"email\":\"draft@example.test\",\"note\":\"Phone order\",\"tags\":[\"alpha\",\"beta\"],\"totalQuantityOfLineItems\":2,\"totalPriceSet\":{\"shopMoney\":{\"amount\":\"43.25\",\"currencyCode\":\"CAD\"}}}}}"
}

pub fn orders_draft_order_update_read_after_write_test() {
  let create_mutation =
    "
    mutation {
      draftOrderCreate(input: {
        email: \"draft@example.test\"
        note: \"Phone order\"
        tags: [\"initial\"]
        lineItems: [{
          title: \"Custom service\"
          quantity: 1
          originalUnitPrice: \"20.00\"
          sku: \"CUSTOM\"
        }]
        shippingLine: {
          title: \"Courier\"
          priceWithCurrency: { amount: \"7.25\", currencyCode: CAD }
        }
      }) {
        draftOrder {
          id
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  let create_outcome =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      create_mutation,
      dict.new(),
      empty_upstream_context(),
    )

  let update_mutation =
    "
    mutation DraftOrderUpdate($id: ID!, $input: DraftOrderInput!) {
      draftOrderUpdate(id: $id, input: $input) {
        draftOrder {
          id
          name
          status
          email
          note
          tags
          customAttributes {
            key
            value
          }
          shippingLine {
            title
            code
            originalPriceSet {
              shopMoney {
                amount
                currencyCode
              }
            }
          }
          subtotalPriceSet {
            shopMoney {
              amount
              currencyCode
            }
          }
          totalShippingPriceSet {
            shopMoney {
              amount
              currencyCode
            }
          }
          totalPriceSet {
            shopMoney {
              amount
              currencyCode
            }
            presentmentMoney {
              amount
              currencyCode
            }
          }
          totalQuantityOfLineItems
          lineItems {
            nodes {
              id
              title
              quantity
              sku
              originalUnitPriceSet {
                shopMoney {
                  amount
                  currencyCode
                }
              }
            }
          }
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  let update_variables =
    dict.from_list([
      #("id", root_field.StringVal("gid://shopify/DraftOrder/1")),
      #(
        "input",
        root_field.ObjectVal(
          dict.from_list([
            #("email", root_field.StringVal("updated-draft@example.test")),
            #("note", root_field.StringVal("Updated note")),
            #(
              "tags",
              root_field.ListVal([
                root_field.StringVal("updated"),
                root_field.StringVal("draft"),
              ]),
            ),
            #(
              "customAttributes",
              root_field.ListVal([
                root_field.ObjectVal(
                  dict.from_list([
                    #("key", root_field.StringVal("source")),
                    #("value", root_field.StringVal("har-492-update")),
                  ]),
                ),
              ]),
            ),
            #(
              "shippingLine",
              root_field.ObjectVal(
                dict.from_list([
                  #("title", root_field.StringVal("Standard")),
                  #(
                    "priceWithCurrency",
                    root_field.ObjectVal(
                      dict.from_list([
                        #("amount", root_field.StringVal("5.00")),
                        #("currencyCode", root_field.StringVal("CAD")),
                      ]),
                    ),
                  ),
                ]),
              ),
            ),
            #(
              "lineItems",
              root_field.ListVal([
                root_field.ObjectVal(
                  dict.from_list([
                    #("title", root_field.StringVal("Updated custom item")),
                    #("quantity", root_field.IntVal(2)),
                    #("originalUnitPrice", root_field.StringVal("12.50")),
                    #("sku", root_field.StringVal("HAR-492-UPDATED")),
                  ]),
                ),
              ]),
            ),
          ]),
        ),
      ),
    ])
  let update_outcome =
    orders.process_mutation(
      create_outcome.store,
      create_outcome.identity,
      "/admin/api/2025-01/graphql.json",
      update_mutation,
      update_variables,
      empty_upstream_context(),
    )

  assert json.to_string(update_outcome.data)
    == "{\"data\":{\"draftOrderUpdate\":{\"draftOrder\":{\"id\":\"gid://shopify/DraftOrder/1\",\"name\":\"#D1\",\"status\":\"OPEN\",\"email\":\"updated-draft@example.test\",\"note\":\"Updated note\",\"tags\":[\"draft\",\"updated\"],\"customAttributes\":[{\"key\":\"source\",\"value\":\"har-492-update\"}],\"shippingLine\":{\"title\":\"Standard\",\"code\":\"custom\",\"originalPriceSet\":{\"shopMoney\":{\"amount\":\"5.0\",\"currencyCode\":\"CAD\"}}},\"subtotalPriceSet\":{\"shopMoney\":{\"amount\":\"25.0\",\"currencyCode\":\"CAD\"}},\"totalShippingPriceSet\":{\"shopMoney\":{\"amount\":\"5.0\",\"currencyCode\":\"CAD\"}},\"totalPriceSet\":{\"shopMoney\":{\"amount\":\"30.0\",\"currencyCode\":\"CAD\"},\"presentmentMoney\":{\"amount\":\"30.0\",\"currencyCode\":\"CAD\"}},\"totalQuantityOfLineItems\":2,\"lineItems\":{\"nodes\":[{\"id\":\"gid://shopify/DraftOrderLineItem/3\",\"title\":\"Updated custom item\",\"quantity\":2,\"sku\":\"HAR-492-UPDATED\",\"originalUnitPriceSet\":{\"shopMoney\":{\"amount\":\"12.5\",\"currencyCode\":\"CAD\"}}}]}},\"userErrors\":[]}}}"
  assert update_outcome.staged_resource_ids == ["gid://shopify/DraftOrder/1"]
  assert list.length(update_outcome.log_drafts) == 1

  let read_query =
    "
    query {
      draftOrder(id: \"gid://shopify/DraftOrder/1\") {
        id
        email
        note
        tags
        totalQuantityOfLineItems
        totalPriceSet {
          shopMoney {
            amount
            currencyCode
          }
        }
      }
    }
  "
  let assert Ok(read_result) =
    orders.process(update_outcome.store, read_query, dict.new())
  assert json.to_string(read_result)
    == "{\"data\":{\"draftOrder\":{\"id\":\"gid://shopify/DraftOrder/1\",\"email\":\"updated-draft@example.test\",\"note\":\"Updated note\",\"tags\":[\"draft\",\"updated\"],\"totalQuantityOfLineItems\":2,\"totalPriceSet\":{\"shopMoney\":{\"amount\":\"30.0\",\"currencyCode\":\"CAD\"}}}}}"
}

pub fn orders_draft_order_update_business_validation_test() {
  let create_mutation =
    "
    mutation {
      draftOrderCreate(input: {
        email: \"validation-probe@example.test\"
        lineItems: [{
          title: \"Validation item\"
          quantity: 1
          originalUnitPrice: \"10.00\"
          sku: \"VALIDATION\"
        }]
        shippingLine: {
          title: \"Courier\"
          priceWithCurrency: { amount: \"5.00\", currencyCode: CAD }
        }
      }) {
        draftOrder {
          id
        }
        userErrors {
          field
          message
          code
        }
      }
    }
  "
  let create_outcome =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      create_mutation,
      dict.new(),
      empty_upstream_context(),
    )

  let complete_mutation =
    "
    mutation {
      draftOrderComplete(id: \"gid://shopify/DraftOrder/1\") {
        draftOrder {
          id
          status
        }
        userErrors {
          field
          message
          code
        }
      }
    }
  "
  let complete_outcome =
    orders.process_mutation(
      create_outcome.store,
      create_outcome.identity,
      "/admin/api/2025-01/graphql.json",
      complete_mutation,
      dict.new(),
      empty_upstream_context(),
    )
  let completed_update =
    orders.process_mutation(
      complete_outcome.store,
      complete_outcome.identity,
      "/admin/api/2025-01/graphql.json",
      "
      mutation {
        draftOrderUpdate(
          id: \"gid://shopify/DraftOrder/1\"
          input: { email: \"mutated-completed@example.test\" }
        ) {
          draftOrder {
            id
            status
            email
          }
          userErrors {
            field
            message
            code
          }
        }
      }
    ",
      dict.new(),
      empty_upstream_context(),
    )
  assert json.to_string(completed_update.data)
    == "{\"data\":{\"draftOrderUpdate\":{\"draftOrder\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"draft_order_already_completed\",\"code\":\"INVALID\"}]}}}"
  assert completed_update.staged_resource_ids == []
  assert completed_update.log_drafts == []
  let assert Ok(completed_read) =
    orders.process(
      completed_update.store,
      "query { draftOrder(id: \"gid://shopify/DraftOrder/1\") { id status email } }",
      dict.new(),
    )
  assert json.to_string(completed_read)
    == "{\"data\":{\"draftOrder\":{\"id\":\"gid://shopify/DraftOrder/1\",\"status\":\"COMPLETED\",\"email\":\"validation-probe@example.test\"}}}"

  let currency_update =
    orders.process_mutation(
      create_outcome.store,
      create_outcome.identity,
      "/admin/api/2025-01/graphql.json",
      "
      mutation {
        draftOrderUpdate(
          id: \"gid://shopify/DraftOrder/1\"
          input: { presentmentCurrencyCode: USD }
        ) {
          draftOrder {
            id
            totalPriceSet {
              shopMoney {
                currencyCode
              }
              presentmentMoney {
                currencyCode
              }
            }
          }
          userErrors {
            field
            message
            code
          }
        }
      }
    ",
      dict.new(),
      empty_upstream_context(),
    )
  assert json.to_string(currency_update.data)
    == "{\"data\":{\"draftOrderUpdate\":{\"draftOrder\":null,\"userErrors\":[{\"field\":[\"input\",\"presentmentCurrencyCode\"],\"message\":\"presentment_currency_code_cannot_be_changed\",\"code\":\"INVALID\"}]}}}"
  assert currency_update.staged_resource_ids == []
  assert currency_update.log_drafts == []

  let empty_line_items_update =
    orders.process_mutation(
      create_outcome.store,
      create_outcome.identity,
      "/admin/api/2025-01/graphql.json",
      "
      mutation {
        draftOrderUpdate(
          id: \"gid://shopify/DraftOrder/1\"
          input: { lineItems: [] }
        ) {
          draftOrder {
            id
            totalQuantityOfLineItems
            lineItems {
              nodes {
                id
              }
            }
          }
          userErrors {
            field
            message
            code
          }
        }
      }
    ",
      dict.new(),
      empty_upstream_context(),
    )
  assert json.to_string(empty_line_items_update.data)
    == "{\"data\":{\"draftOrderUpdate\":{\"draftOrder\":null,\"userErrors\":[{\"field\":[\"input\",\"lineItems\"],\"message\":\"line_items_must_not_be_empty\",\"code\":\"INVALID\"}]}}}"
  assert empty_line_items_update.staged_resource_ids == []
  assert empty_line_items_update.log_drafts == []
  let assert Ok(empty_line_items_read) =
    orders.process(
      empty_line_items_update.store,
      "query { draftOrder(id: \"gid://shopify/DraftOrder/1\") { id totalQuantityOfLineItems lineItems { nodes { title } } } }",
      dict.new(),
    )
  assert json.to_string(empty_line_items_read)
    == "{\"data\":{\"draftOrder\":{\"id\":\"gid://shopify/DraftOrder/1\",\"totalQuantityOfLineItems\":1,\"lineItems\":{\"nodes\":[{\"title\":\"Validation item\"}]}}}}"
}

pub fn orders_draft_order_duplicate_read_after_write_test() {
  let create_mutation =
    "
    mutation {
      draftOrderCreate(input: {
        email: \"duplicate-source@example.test\"
        tags: [\"duplicate\", \"source\"]
        lineItems: [{
          title: \"Custom service\"
          quantity: 2
          originalUnitPrice: \"20.00\"
          requiresShipping: false
          taxable: false
          sku: \"CUSTOM\"
          appliedDiscount: {
            title: \"Service discount\"
            description: \"10 percent off\"
            value: 10
            amount: 4
            valueType: PERCENTAGE
          }
        }]
        shippingLine: {
          title: \"Courier\"
          priceWithCurrency: { amount: \"7.25\", currencyCode: CAD }
        }
      }) {
        draftOrder {
          id
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  let create_outcome =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      create_mutation,
      dict.new(),
      empty_upstream_context(),
    )

  let duplicate_mutation =
    "
    mutation DraftOrderDuplicate($id: ID) {
      draftOrderDuplicate(id: $id) {
        draftOrder {
          id
          name
          invoiceUrl
          status
          ready
          email
          tags
          shippingLine {
            title
          }
          subtotalPriceSet {
            shopMoney {
              amount
              currencyCode
            }
          }
          totalDiscountsSet {
            shopMoney {
              amount
              currencyCode
            }
          }
          totalShippingPriceSet {
            shopMoney {
              amount
              currencyCode
            }
          }
          totalPriceSet {
            shopMoney {
              amount
              currencyCode
            }
          }
          lineItems {
            nodes {
              id
              title
              quantity
              sku
              appliedDiscount {
                title
              }
              discountedTotalSet {
                shopMoney {
                  amount
                  currencyCode
                }
              }
              totalDiscountSet {
                shopMoney {
                  amount
                  currencyCode
                }
              }
            }
          }
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  let duplicate_outcome =
    orders.process_mutation(
      create_outcome.store,
      create_outcome.identity,
      "/admin/api/2025-01/graphql.json",
      duplicate_mutation,
      dict.from_list([
        #("id", root_field.StringVal("gid://shopify/DraftOrder/1")),
      ]),
      empty_upstream_context(),
    )

  assert json.to_string(duplicate_outcome.data)
    == "{\"data\":{\"draftOrderDuplicate\":{\"draftOrder\":{\"id\":\"gid://shopify/DraftOrder/3\",\"name\":\"#D2\",\"invoiceUrl\":\"https://shopify-draft-proxy.local/draft_orders/gid://shopify/DraftOrder/3/invoice\",\"status\":\"OPEN\",\"ready\":true,\"email\":\"duplicate-source@example.test\",\"tags\":[\"duplicate\",\"source\"],\"shippingLine\":null,\"subtotalPriceSet\":{\"shopMoney\":{\"amount\":\"40.0\",\"currencyCode\":\"CAD\"}},\"totalDiscountsSet\":{\"shopMoney\":{\"amount\":\"0.0\",\"currencyCode\":\"CAD\"}},\"totalShippingPriceSet\":{\"shopMoney\":{\"amount\":\"0.0\",\"currencyCode\":\"CAD\"}},\"totalPriceSet\":{\"shopMoney\":{\"amount\":\"40.0\",\"currencyCode\":\"CAD\"}},\"lineItems\":{\"nodes\":[{\"id\":\"gid://shopify/DraftOrderLineItem/4\",\"title\":\"Custom service\",\"quantity\":2,\"sku\":\"CUSTOM\",\"appliedDiscount\":null,\"discountedTotalSet\":{\"shopMoney\":{\"amount\":\"40.0\",\"currencyCode\":\"CAD\"}},\"totalDiscountSet\":{\"shopMoney\":{\"amount\":\"0.0\",\"currencyCode\":\"CAD\"}}}]}},\"userErrors\":[]}}}"
  assert duplicate_outcome.staged_resource_ids == ["gid://shopify/DraftOrder/3"]
  assert list.length(duplicate_outcome.log_drafts) == 1

  let read_query =
    "
    query {
      draftOrder(id: \"gid://shopify/DraftOrder/3\") {
        id
        email
        shippingLine {
          title
        }
        totalPriceSet {
          shopMoney {
            amount
            currencyCode
          }
        }
      }
    }
  "
  let assert Ok(read_result) =
    orders.process(duplicate_outcome.store, read_query, dict.new())
  assert json.to_string(read_result)
    == "{\"data\":{\"draftOrder\":{\"id\":\"gid://shopify/DraftOrder/3\",\"email\":\"duplicate-source@example.test\",\"shippingLine\":null,\"totalPriceSet\":{\"shopMoney\":{\"amount\":\"40.0\",\"currencyCode\":\"CAD\"}}}}}"
}

pub fn orders_draft_order_duplicate_resets_lifecycle_fields_test() {
  let open_source_id = "gid://shopify/DraftOrder/101"
  let completed_source_id = "gid://shopify/DraftOrder/100"
  let source_order_id = "gid://shopify/Order/200"
  let seeded_store =
    store.new()
    |> store.upsert_base_draft_orders([
      types.DraftOrderRecord(
        id: open_source_id,
        cursor: None,
        data: types.CapturedObject([
          #("id", types.CapturedString(open_source_id)),
          #("name", types.CapturedString("#D101")),
          #("status", types.CapturedString("OPEN")),
          #("ready", types.CapturedBool(True)),
          #("completedAt", types.CapturedNull),
          #("createdAt", types.CapturedString("2024-03-01T12:00:00.000Z")),
          #("updatedAt", types.CapturedString("2024-03-10T12:00:00.000Z")),
          #("invoiceSentAt", types.CapturedString("2024-03-15T12:00:00.000Z")),
          #(
            "invoiceUrl",
            types.CapturedString("https://example.test/open-invoice"),
          ),
          #("orderId", types.CapturedNull),
          #("order", types.CapturedNull),
          #(
            "lineItems",
            types.CapturedObject([#("nodes", types.CapturedArray([]))]),
          ),
        ]),
      ),
      types.DraftOrderRecord(
        id: completed_source_id,
        cursor: None,
        data: types.CapturedObject([
          #("id", types.CapturedString(completed_source_id)),
          #("name", types.CapturedString("#D100")),
          #("status", types.CapturedString("COMPLETED")),
          #("ready", types.CapturedBool(True)),
          #("completedAt", types.CapturedString("2024-04-01T12:00:00.000Z")),
          #("createdAt", types.CapturedString("2024-03-01T12:00:00.000Z")),
          #("updatedAt", types.CapturedString("2024-04-01T12:00:00.000Z")),
          #("invoiceSentAt", types.CapturedString("2024-03-15T12:00:00.000Z")),
          #(
            "invoiceUrl",
            types.CapturedString("https://example.test/inherited-invoice"),
          ),
          #("orderId", types.CapturedString(source_order_id)),
          #(
            "order",
            types.CapturedObject([
              #("id", types.CapturedString(source_order_id)),
              #("name", types.CapturedString("#200")),
            ]),
          ),
          #(
            "lineItems",
            types.CapturedObject([#("nodes", types.CapturedArray([]))]),
          ),
        ]),
      ),
    ])

  let duplicate_mutation =
    "
    mutation DraftOrderDuplicate($id: ID) {
      draftOrderDuplicate(id: $id) {
        draftOrder {
          id
          name
          status
          ready
          completedAt
          createdAt
          updatedAt
          invoiceSentAt
          invoiceUrl
          order {
            id
            name
          }
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  let open_duplicate_outcome =
    orders.process_mutation(
      seeded_store,
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      duplicate_mutation,
      dict.from_list([#("id", root_field.StringVal(open_source_id))]),
      empty_upstream_context(),
    )

  assert json.to_string(open_duplicate_outcome.data)
    == "{\"data\":{\"draftOrderDuplicate\":{\"draftOrder\":{\"id\":\"gid://shopify/DraftOrder/1\",\"name\":\"#D3\",\"status\":\"OPEN\",\"ready\":true,\"completedAt\":null,\"createdAt\":\"2024-01-01T00:00:00.000Z\",\"updatedAt\":\"2024-01-01T00:00:00.000Z\",\"invoiceSentAt\":null,\"invoiceUrl\":\"https://shopify-draft-proxy.local/draft_orders/gid://shopify/DraftOrder/1/invoice\",\"order\":null},\"userErrors\":[]}}}"

  let completed_duplicate_outcome =
    orders.process_mutation(
      open_duplicate_outcome.store,
      open_duplicate_outcome.identity,
      "/admin/api/2025-01/graphql.json",
      duplicate_mutation,
      dict.from_list([#("id", root_field.StringVal(completed_source_id))]),
      empty_upstream_context(),
    )

  assert json.to_string(completed_duplicate_outcome.data)
    == "{\"data\":{\"draftOrderDuplicate\":{\"draftOrder\":{\"id\":\"gid://shopify/DraftOrder/2\",\"name\":\"#D4\",\"status\":\"OPEN\",\"ready\":true,\"completedAt\":null,\"createdAt\":\"2024-01-01T00:00:01.000Z\",\"updatedAt\":\"2024-01-01T00:00:01.000Z\",\"invoiceSentAt\":null,\"invoiceUrl\":\"https://shopify-draft-proxy.local/draft_orders/gid://shopify/DraftOrder/2/invoice\",\"order\":null},\"userErrors\":[]}}}"
}

pub fn orders_draft_order_complete_read_after_write_test() {
  let create_mutation =
    "
    mutation {
      draftOrderCreate(input: {
        email: \"complete-source@example.test\"
        note: \"complete this staged draft locally\"
        tags: [\"draft-complete\", \"gleam\"]
        customAttributes: [{ key: \"source\", value: \"direct-test\" }]
        billingAddress: {
          firstName: \"Hermes\"
          lastName: \"Closer\"
          address1: \"123 Queen St W\"
          city: \"Toronto\"
          provinceCode: \"ON\"
          countryCode: \"CA\"
          zip: \"M5H 2M9\"
        }
        lineItems: [{
          title: \"Completion service\"
          quantity: 2
          originalUnitPrice: \"12.50\"
          sku: \"COMPLETE\"
        }]
      }) {
        draftOrder {
          id
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  let create_outcome =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      create_mutation,
      dict.new(),
      empty_upstream_context(),
    )

  let complete_mutation =
    "
    mutation DraftOrderComplete($id: ID!, $sourceName: String, $paymentPending: Boolean) {
      draftOrderComplete(
        id: $id
        sourceName: $sourceName
        paymentPending: $paymentPending
      ) {
        draftOrder {
          id
          name
          status
          ready
          completedAt
          totalPriceSet {
            shopMoney {
              amount
              currencyCode
            }
            presentmentMoney {
              amount
              currencyCode
            }
          }
          lineItems {
            nodes {
              id
              title
              quantity
              sku
            }
          }
          order {
            id
            name
            sourceName
            paymentGatewayNames
            displayFinancialStatus
            displayFulfillmentStatus
            note
            tags
            customAttributes {
              key
              value
            }
            billingAddress {
              firstName
              lastName
              address1
              city
              provinceCode
              countryCodeV2
              zip
            }
            currentTotalPriceSet {
              shopMoney {
                amount
                currencyCode
              }
              presentmentMoney {
                amount
                currencyCode
              }
            }
            lineItems {
              nodes {
                id
                title
                quantity
                sku
                variantTitle
                originalUnitPriceSet {
                  shopMoney {
                    amount
                    currencyCode
                  }
                  presentmentMoney {
                    amount
                    currencyCode
                  }
                }
              }
            }
          }
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  let complete_outcome =
    orders.process_mutation(
      create_outcome.store,
      create_outcome.identity,
      "/admin/api/2025-01/graphql.json",
      complete_mutation,
      dict.from_list([
        #("id", root_field.StringVal("gid://shopify/DraftOrder/1")),
        #("sourceName", root_field.StringVal("hermes-cron-orders")),
        #("paymentPending", root_field.BoolVal(False)),
      ]),
      empty_upstream_context(),
    )

  assert json.to_string(complete_outcome.data)
    == "{\"data\":{\"draftOrderComplete\":{\"draftOrder\":{\"id\":\"gid://shopify/DraftOrder/1\",\"name\":\"#D1\",\"status\":\"COMPLETED\",\"ready\":true,\"completedAt\":\"2024-01-01T00:00:01.000Z\",\"totalPriceSet\":{\"shopMoney\":{\"amount\":\"25.0\",\"currencyCode\":\"CAD\"},\"presentmentMoney\":{\"amount\":\"25.0\",\"currencyCode\":\"CAD\"}},\"lineItems\":{\"nodes\":[{\"id\":\"gid://shopify/DraftOrderLineItem/2\",\"title\":\"Completion service\",\"quantity\":2,\"sku\":\"COMPLETE\"}]},\"order\":{\"id\":\"gid://shopify/Order/3\",\"name\":\"#1\",\"sourceName\":\"347082227713\",\"paymentGatewayNames\":[\"manual\"],\"displayFinancialStatus\":\"PAID\",\"displayFulfillmentStatus\":\"UNFULFILLED\",\"note\":\"complete this staged draft locally\",\"tags\":[\"draft-complete\",\"gleam\"],\"customAttributes\":[{\"key\":\"source\",\"value\":\"direct-test\"}],\"billingAddress\":{\"firstName\":\"Hermes\",\"lastName\":\"Closer\",\"address1\":\"123 Queen St W\",\"city\":\"Toronto\",\"provinceCode\":\"ON\",\"countryCodeV2\":\"CA\",\"zip\":\"M5H 2M9\"},\"currentTotalPriceSet\":{\"shopMoney\":{\"amount\":\"25.0\",\"currencyCode\":\"CAD\"},\"presentmentMoney\":{\"amount\":\"25.0\",\"currencyCode\":\"CAD\"}},\"lineItems\":{\"nodes\":[{\"id\":\"gid://shopify/LineItem/4\",\"title\":\"Completion service\",\"quantity\":2,\"sku\":\"COMPLETE\",\"variantTitle\":null,\"originalUnitPriceSet\":{\"shopMoney\":{\"amount\":\"12.5\",\"currencyCode\":\"CAD\"},\"presentmentMoney\":{\"amount\":\"12.5\",\"currencyCode\":\"CAD\"}}}]}}},\"userErrors\":[]}}}"
  assert complete_outcome.staged_resource_ids == ["gid://shopify/DraftOrder/1"]
  assert list.length(complete_outcome.log_drafts) == 1

  let read_query =
    "
    query {
      draftOrder(id: \"gid://shopify/DraftOrder/1\") {
        id
        status
        completedAt
        order {
          id
          name
          sourceName
          displayFinancialStatus
        }
      }
    }
  "
  let assert Ok(read_result) =
    orders.process(complete_outcome.store, read_query, dict.new())
  assert json.to_string(read_result)
    == "{\"data\":{\"draftOrder\":{\"id\":\"gid://shopify/DraftOrder/1\",\"status\":\"COMPLETED\",\"completedAt\":\"2024-01-01T00:00:01.000Z\",\"order\":{\"id\":\"gid://shopify/Order/3\",\"name\":\"#1\",\"sourceName\":\"347082227713\",\"displayFinancialStatus\":\"PAID\"}}}}"

  let order_read_query =
    "
    query {
      completedOrder: order(id: \"gid://shopify/Order/3\") {
        id
        name
        sourceName
        displayFinancialStatus
        displayFulfillmentStatus
        currentTotalPriceSet {
          shopMoney {
            amount
            currencyCode
          }
        }
        lineItems {
          nodes {
            id
            title
            quantity
            sku
          }
        }
      }
      completedOrders: orders(first: 10, query: \"name:#1\") {
        nodes {
          id
          name
          sourceName
          displayFinancialStatus
        }
        pageInfo {
          hasNextPage
          hasPreviousPage
          startCursor
          endCursor
        }
      }
    }
  "
  let assert Ok(order_read_result) =
    orders.process(complete_outcome.store, order_read_query, dict.new())
  assert json.to_string(order_read_result)
    == "{\"data\":{\"completedOrder\":{\"id\":\"gid://shopify/Order/3\",\"name\":\"#1\",\"sourceName\":\"347082227713\",\"displayFinancialStatus\":\"PAID\",\"displayFulfillmentStatus\":\"UNFULFILLED\",\"currentTotalPriceSet\":{\"shopMoney\":{\"amount\":\"25.0\",\"currencyCode\":\"CAD\"}},\"lineItems\":{\"nodes\":[{\"id\":\"gid://shopify/LineItem/4\",\"title\":\"Completion service\",\"quantity\":2,\"sku\":\"COMPLETE\"}]}},\"completedOrders\":{\"nodes\":[{\"id\":\"gid://shopify/Order/3\",\"name\":\"#1\",\"sourceName\":\"347082227713\",\"displayFinancialStatus\":\"PAID\"}],\"pageInfo\":{\"hasNextPage\":false,\"hasPreviousPage\":false,\"startCursor\":\"gid://shopify/Order/3\",\"endCursor\":\"gid://shopify/Order/3\"}}}}"
}

pub fn orders_draft_order_complete_payment_gateway_paths_test() {
  let active_gateway_id = "gid://shopify/PaymentGateway/active-local"
  let disabled_gateway_id = "gid://shopify/PaymentGateway/disabled-local"
  let initial_store =
    store.new()
    |> store.set_shop_payment_gateways([
      types.PaymentGatewayRecord(
        id: active_gateway_id,
        name: "bogus-payments",
        active: True,
      ),
      types.PaymentGatewayRecord(
        id: disabled_gateway_id,
        name: "disabled-payments",
        active: False,
      ),
    ])

  let active_create =
    create_gateway_completion_draft(initial_store, synthetic_identity.new())
  let assert [active_draft_id] = active_create.staged_resource_ids
  let active_complete =
    complete_gateway_completion_draft(
      active_create.store,
      active_create.identity,
      active_draft_id,
      root_field.StringVal(active_gateway_id),
      False,
    )
  let active_json = json.to_string(active_complete.data)
  assert string.contains(active_json, "\"displayFinancialStatus\":\"PAID\"")
  assert string.contains(
    active_json,
    "\"paymentGatewayNames\":[\"bogus-payments\"]",
  )
  assert string.contains(active_json, "\"kind\":\"SALE\"")
  assert string.contains(active_json, "\"gateway\":\"bogus-payments\"")
  assert active_complete.staged_resource_ids == [active_draft_id]
  assert list.length(active_complete.log_drafts) == 1

  let pending_create =
    create_gateway_completion_draft(
      active_complete.store,
      active_complete.identity,
    )
  let assert [pending_draft_id] = pending_create.staged_resource_ids
  let pending_complete =
    complete_gateway_completion_draft(
      pending_create.store,
      pending_create.identity,
      pending_draft_id,
      root_field.StringVal(active_gateway_id),
      True,
    )
  let pending_json = json.to_string(pending_complete.data)
  assert string.contains(
    pending_json,
    "\"displayFinancialStatus\":\"AUTHORIZED\"",
  )
  assert string.contains(
    pending_json,
    "\"paymentGatewayNames\":[\"bogus-payments\"]",
  )
  assert string.contains(pending_json, "\"kind\":\"AUTHORIZATION\"")
  assert string.contains(pending_json, "\"gateway\":\"bogus-payments\"")

  let unknown_create =
    create_gateway_completion_draft(
      pending_complete.store,
      pending_complete.identity,
    )
  let assert [unknown_draft_id] = unknown_create.staged_resource_ids
  let unknown_complete =
    complete_gateway_completion_draft(
      unknown_create.store,
      unknown_create.identity,
      unknown_draft_id,
      root_field.StringVal("gid://shopify/PaymentGateway/not-installed"),
      False,
    )
  assert json.to_string(unknown_complete.data)
    == "{\"data\":{\"draftOrderComplete\":{\"draftOrder\":null,\"userErrors\":[{\"field\":[\"paymentGatewayId\"],\"message\":\"payment_gateway_not_found\",\"code\":\"INVALID\"}]}}}"
  assert unknown_complete.staged_resource_ids == []
  assert unknown_complete.log_drafts == []

  let disabled_create =
    create_gateway_completion_draft(
      unknown_complete.store,
      unknown_complete.identity,
    )
  let assert [disabled_draft_id] = disabled_create.staged_resource_ids
  let disabled_complete =
    complete_gateway_completion_draft(
      disabled_create.store,
      disabled_create.identity,
      disabled_draft_id,
      root_field.StringVal(disabled_gateway_id),
      False,
    )
  assert json.to_string(disabled_complete.data)
    == "{\"data\":{\"draftOrderComplete\":{\"draftOrder\":null,\"userErrors\":[{\"field\":[\"paymentGatewayId\"],\"message\":\"payment_gateway_disabled\",\"code\":\"INVALID\"}]}}}"
  assert disabled_complete.staged_resource_ids == []
  assert disabled_complete.log_drafts == []

  let no_gateway_create =
    create_gateway_completion_draft(
      disabled_complete.store,
      disabled_complete.identity,
    )
  let assert [no_gateway_draft_id] = no_gateway_create.staged_resource_ids
  let no_gateway_complete =
    complete_gateway_completion_draft(
      no_gateway_create.store,
      no_gateway_create.identity,
      no_gateway_draft_id,
      root_field.NullVal,
      True,
    )
  let no_gateway_json = json.to_string(no_gateway_complete.data)
  assert string.contains(
    no_gateway_json,
    "\"displayFinancialStatus\":\"PENDING\"",
  )
  assert string.contains(no_gateway_json, "\"paymentGatewayNames\":[]")
  assert string.contains(no_gateway_json, "\"transactions\":[]")
}

fn create_gateway_completion_draft(current_store, identity) {
  let mutation =
    "
    mutation {
      draftOrderCreate(input: {
        email: \"gateway-complete@example.test\"
        lineItems: [{
          title: \"Gateway completion service\"
          quantity: 1
          originalUnitPrice: \"10.00\"
          sku: \"GATEWAY-COMPLETE\"
        }]
      }) {
        draftOrder {
          id
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  orders.process_mutation(
    current_store,
    identity,
    "/admin/api/2025-01/graphql.json",
    mutation,
    dict.new(),
    empty_upstream_context(),
  )
}

fn complete_gateway_completion_draft(
  current_store,
  identity,
  draft_order_id,
  payment_gateway_id,
  payment_pending,
) {
  let mutation =
    "
    mutation DraftOrderCompleteGateway(
      $id: ID!
      $paymentGatewayId: ID
      $paymentPending: Boolean
    ) {
      draftOrderComplete(
        id: $id
        paymentGatewayId: $paymentGatewayId
        paymentPending: $paymentPending
      ) {
        draftOrder {
          id
          status
          order {
            id
            displayFinancialStatus
            paymentGatewayNames
            transactions {
              kind
              status
              gateway
              amountSet {
                shopMoney {
                  amount
                  currencyCode
                }
              }
            }
          }
        }
        userErrors {
          field
          message
          code
        }
      }
    }
  "
  orders.process_mutation(
    current_store,
    identity,
    "/admin/api/2025-01/graphql.json",
    mutation,
    dict.from_list([
      #("id", root_field.StringVal(draft_order_id)),
      #("paymentGatewayId", payment_gateway_id),
      #("paymentPending", root_field.BoolVal(payment_pending)),
    ]),
    empty_upstream_context(),
  )
}

pub fn orders_draft_order_create_from_order_read_after_write_test() {
  let create_mutation =
    "
    mutation {
      draftOrderCreate(input: {
        email: \"from-order-source@example.test\"
        note: \"complete before cloning to draft\"
        tags: [\"from-order\", \"source\"]
        lineItems: [{
          title: \"From order service\"
          quantity: 2
          originalUnitPrice: \"12.50\"
          sku: \"FROM-ORDER\"
        }]
      }) {
        draftOrder {
          id
        }
      }
    }
  "
  let create_outcome =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      create_mutation,
      dict.new(),
      empty_upstream_context(),
    )

  let complete_mutation =
    "
    mutation DraftOrderComplete($id: ID!) {
      draftOrderComplete(id: $id, sourceName: \"hermes-cron-orders\") {
        draftOrder {
          id
          order {
            id
          }
        }
      }
    }
  "
  let complete_outcome =
    orders.process_mutation(
      create_outcome.store,
      create_outcome.identity,
      "/admin/api/2025-01/graphql.json",
      complete_mutation,
      dict.from_list([
        #("id", root_field.StringVal("gid://shopify/DraftOrder/1")),
      ]),
      empty_upstream_context(),
    )

  let mutation =
    "
    mutation DraftOrderCreateFromOrder($orderId: ID!) {
      draftOrderCreateFromOrder(orderId: $orderId) {
        draftOrder {
          id
          name
          status
          ready
          email
          tags
          subtotalPriceSet {
            shopMoney {
              amount
              currencyCode
            }
          }
          totalPriceSet {
            shopMoney {
              amount
              currencyCode
            }
          }
          lineItems {
            nodes {
              id
              title
              quantity
              sku
            }
          }
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  let outcome =
    orders.process_mutation(
      complete_outcome.store,
      complete_outcome.identity,
      "/admin/api/2025-01/graphql.json",
      mutation,
      dict.from_list([
        #("orderId", root_field.StringVal("gid://shopify/Order/3")),
      ]),
      empty_upstream_context(),
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"draftOrderCreateFromOrder\":{\"draftOrder\":{\"id\":\"gid://shopify/DraftOrder/5\",\"name\":\"#D2\",\"status\":\"OPEN\",\"ready\":true,\"email\":\"from-order-source@example.test\",\"tags\":[\"from-order\",\"source\"],\"subtotalPriceSet\":{\"shopMoney\":{\"amount\":\"25.0\",\"currencyCode\":\"CAD\"}},\"totalPriceSet\":{\"shopMoney\":{\"amount\":\"25.0\",\"currencyCode\":\"CAD\"}},\"lineItems\":{\"nodes\":[{\"id\":\"gid://shopify/DraftOrderLineItem/6\",\"title\":\"From order service\",\"quantity\":2,\"sku\":\"FROM-ORDER\"}]}},\"userErrors\":[]}}}"
  assert outcome.staged_resource_ids == ["gid://shopify/DraftOrder/5"]
  assert list.length(outcome.log_drafts) == 1

  let read_query =
    "
    query {
      draftOrder(id: \"gid://shopify/DraftOrder/5\") {
        id
        email
        totalPriceSet {
          shopMoney {
            amount
            currencyCode
          }
        }
      }
    }
  "
  let assert Ok(read_result) =
    orders.process(outcome.store, read_query, dict.new())
  assert json.to_string(read_result)
    == "{\"data\":{\"draftOrder\":{\"id\":\"gid://shopify/DraftOrder/5\",\"email\":\"from-order-source@example.test\",\"totalPriceSet\":{\"shopMoney\":{\"amount\":\"25.0\",\"currencyCode\":\"CAD\"}}}}}"

  let missing_outcome =
    orders.process_mutation(
      outcome.store,
      outcome.identity,
      "/admin/api/2025-01/graphql.json",
      mutation,
      dict.from_list([
        #("orderId", root_field.StringVal("gid://shopify/Order/404")),
      ]),
      empty_upstream_context(),
    )
  assert json.to_string(missing_outcome.data)
    == "{\"data\":{\"draftOrderCreateFromOrder\":{\"draftOrder\":null,\"userErrors\":[{\"field\":[\"orderId\"],\"message\":\"Order does not exist\"}]}}}"
}

pub fn orders_draft_order_invoice_send_safety_validation_test() {
  let mutation =
    "
    mutation DraftOrderInvoiceSend($id: ID!, $email: EmailInput) {
      draftOrderInvoiceSend(id: $id, email: $email) {
        draftOrder {
          id
          status
          email
          invoiceUrl
        }
        userErrors {
          field
          message
        }
      }
    }
  "
  let unknown_outcome =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      mutation,
      dict.from_list([
        #("id", root_field.StringVal("gid://shopify/DraftOrder/999")),
      ]),
      empty_upstream_context(),
    )
  assert json.to_string(unknown_outcome.data)
    == "{\"data\":{\"draftOrderInvoiceSend\":{\"draftOrder\":null,\"userErrors\":[{\"field\":null,\"message\":\"Draft order not found\"}]}}}"

  let create_mutation =
    "
    mutation {
      draftOrderCreate(input: {
        lineItems: [{ title: \"Invoice safety item\", quantity: 1, originalUnitPrice: \"1.00\" }]
      }) {
        draftOrder {
          id
        }
      }
    }
  "
  let create_outcome =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      create_mutation,
      dict.new(),
      empty_upstream_context(),
    )
  let open_outcome =
    orders.process_mutation(
      create_outcome.store,
      create_outcome.identity,
      "/admin/api/2025-01/graphql.json",
      mutation,
      dict.from_list([
        #("id", root_field.StringVal("gid://shopify/DraftOrder/1")),
      ]),
      empty_upstream_context(),
    )
  assert json.to_string(open_outcome.data)
    == "{\"data\":{\"draftOrderInvoiceSend\":{\"draftOrder\":{\"id\":\"gid://shopify/DraftOrder/1\",\"status\":\"OPEN\",\"email\":null,\"invoiceUrl\":\"https://shopify-draft-proxy.local/draft_orders/gid://shopify/DraftOrder/1/invoice\"},\"userErrors\":[{\"field\":null,\"message\":\"To can't be blank\"}]}}}"
}

pub fn orders_draft_order_residual_helper_roots_test() {
  let assert Ok(delivery_result) =
    orders.process(
      store.new(),
      "
      query {
        draftOrderAvailableDeliveryOptions(input: {
          lineItems: [{ title: \"Local delivery probe\", quantity: 1, originalUnitPrice: \"4.00\" }]
        }) {
          availableShippingRates { handle title }
          availableLocalDeliveryRates { handle title }
          availableLocalPickupOptions { handle title }
          pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
        }
      }
    ",
      dict.new(),
    )
  assert json.to_string(delivery_result)
    == "{\"data\":{\"draftOrderAvailableDeliveryOptions\":{\"availableShippingRates\":[],\"availableLocalDeliveryRates\":[],\"availableLocalPickupOptions\":[],\"pageInfo\":{\"hasNextPage\":false,\"hasPreviousPage\":false,\"startCursor\":null,\"endCursor\":null}}}}"

  let calculate_outcome =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      "
      mutation {
        draftOrderCalculate(input: {
          lineItems: [{ title: \"Calculated custom item\", quantity: 2, originalUnitPrice: \"6.25\" }]
        }) {
          calculatedDraftOrder {
            currencyCode
            totalQuantityOfLineItems
            subtotalPriceSet { shopMoney { amount currencyCode } }
            totalPriceSet { shopMoney { amount currencyCode } }
            lineItems {
              title
              quantity
              custom
              originalTotalSet { shopMoney { amount currencyCode } }
            }
            availableShippingRates { handle title }
          }
          userErrors { field message }
        }
      }
    ",
      dict.new(),
      empty_upstream_context(),
    )
  assert json.to_string(calculate_outcome.data)
    == "{\"data\":{\"draftOrderCalculate\":{\"calculatedDraftOrder\":{\"currencyCode\":\"CAD\",\"totalQuantityOfLineItems\":2,\"subtotalPriceSet\":{\"shopMoney\":{\"amount\":\"12.5\",\"currencyCode\":\"CAD\"}},\"totalPriceSet\":{\"shopMoney\":{\"amount\":\"12.5\",\"currencyCode\":\"CAD\"}},\"lineItems\":[{\"title\":\"Calculated custom item\",\"quantity\":2,\"custom\":true,\"originalTotalSet\":{\"shopMoney\":{\"amount\":\"12.5\",\"currencyCode\":\"CAD\"}}}],\"availableShippingRates\":[]},\"userErrors\":[]}}}"

  let create_mutation =
    "
    mutation {
      draftOrderCreate(input: {
        email: \"helper@example.test\"
        tags: [\"initial\"]
        lineItems: [{ title: \"Bulk helper item\", quantity: 1, originalUnitPrice: \"2.00\" }]
      }) {
        draftOrder { id tags }
        userErrors { field message }
      }
    }
  "
  let create_outcome =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      create_mutation,
      dict.new(),
      empty_upstream_context(),
    )

  let invoice_preview_mutation =
    "
    mutation PreviewDraftInvoice($id: ID!, $email: EmailInput) {
      draftOrderInvoicePreview(id: $id, email: $email) {
        previewSubject
        userErrors { field message }
      }
    }
  "
  let preview_outcome =
    orders.process_mutation(
      create_outcome.store,
      create_outcome.identity,
      "/admin/api/2025-01/graphql.json",
      invoice_preview_mutation,
      dict.from_list([
        #("id", root_field.StringVal("gid://shopify/DraftOrder/1")),
        #(
          "email",
          root_field.ObjectVal(
            dict.from_list([
              #("subject", root_field.StringVal("Custom invoice subject")),
            ]),
          ),
        ),
      ]),
      empty_upstream_context(),
    )
  assert json.to_string(preview_outcome.data)
    == "{\"data\":{\"draftOrderInvoicePreview\":{\"previewSubject\":\"Custom invoice subject\",\"userErrors\":[]}}}"

  let bulk_add_mutation =
    "
    mutation BulkAdd($ids: [ID!], $tags: [String!]!) {
      draftOrderBulkAddTags(ids: $ids, tags: $tags) {
        job { id done }
        userErrors { field message }
      }
    }
  "
  let add_outcome =
    orders.process_mutation(
      preview_outcome.store,
      preview_outcome.identity,
      "/admin/api/2025-01/graphql.json",
      bulk_add_mutation,
      dict.from_list([
        #(
          "ids",
          root_field.ListVal([
            root_field.StringVal("gid://shopify/DraftOrder/1"),
          ]),
        ),
        #("tags", root_field.ListVal([root_field.StringVal("added")])),
      ]),
      empty_upstream_context(),
    )
  assert json.to_string(add_outcome.data)
    == "{\"data\":{\"draftOrderBulkAddTags\":{\"job\":{\"id\":\"gid://shopify/Job/3\",\"done\":false},\"userErrors\":[]}}}"

  let assert Ok(after_add_read) =
    orders.process(
      add_outcome.store,
      "query { draftOrder(id: \"gid://shopify/DraftOrder/1\") { id tags } }",
      dict.new(),
    )
  assert json.to_string(after_add_read)
    == "{\"data\":{\"draftOrder\":{\"id\":\"gid://shopify/DraftOrder/1\",\"tags\":[\"added\",\"initial\"]}}}"

  let bulk_remove_mutation =
    "
    mutation BulkRemove($ids: [ID!], $tags: [String!]!) {
      draftOrderBulkRemoveTags(ids: $ids, tags: $tags) {
        job { id done }
        userErrors { field message }
      }
    }
  "
  let remove_outcome =
    orders.process_mutation(
      add_outcome.store,
      add_outcome.identity,
      "/admin/api/2025-01/graphql.json",
      bulk_remove_mutation,
      dict.from_list([
        #(
          "ids",
          root_field.ListVal([
            root_field.StringVal("gid://shopify/DraftOrder/1"),
          ]),
        ),
        #("tags", root_field.ListVal([root_field.StringVal("initial")])),
      ]),
      empty_upstream_context(),
    )
  assert json.to_string(remove_outcome.data)
    == "{\"data\":{\"draftOrderBulkRemoveTags\":{\"job\":{\"id\":\"gid://shopify/Job/4\",\"done\":false},\"userErrors\":[]}}}"

  let assert Ok(after_remove_read) =
    orders.process(
      remove_outcome.store,
      "query { draftOrder(id: \"gid://shopify/DraftOrder/1\") { id tags } }",
      dict.new(),
    )
  assert json.to_string(after_remove_read)
    == "{\"data\":{\"draftOrder\":{\"id\":\"gid://shopify/DraftOrder/1\",\"tags\":[\"added\"]}}}"

  let bulk_delete_mutation =
    "
    mutation BulkDelete($ids: [ID!]) {
      draftOrderBulkDelete(ids: $ids) {
        job { id done }
        userErrors { field message }
      }
    }
  "
  let delete_outcome =
    orders.process_mutation(
      remove_outcome.store,
      remove_outcome.identity,
      "/admin/api/2025-01/graphql.json",
      bulk_delete_mutation,
      dict.from_list([
        #(
          "ids",
          root_field.ListVal([
            root_field.StringVal("gid://shopify/DraftOrder/1"),
          ]),
        ),
      ]),
      empty_upstream_context(),
    )
  assert json.to_string(delete_outcome.data)
    == "{\"data\":{\"draftOrderBulkDelete\":{\"job\":{\"id\":\"gid://shopify/Job/5\",\"done\":false},\"userErrors\":[]}}}"

  let assert Ok(after_delete_read) =
    orders.process(
      delete_outcome.store,
      "query { draftOrder(id: \"gid://shopify/DraftOrder/1\") { id tags } }",
      dict.new(),
    )
  assert json.to_string(after_delete_read) == "{\"data\":{\"draftOrder\":null}}"
}

pub fn orders_draft_order_bulk_tags_validation_partial_success_test() {
  let create_outcome =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      "
      mutation {
        draftOrderCreate(input: {
          email: \"bulk-validation@example.test\"
          tags: [\"Initial\"]
          lineItems: [{ title: \"Bulk validation item\", quantity: 1, originalUnitPrice: \"2.00\" }]
        }) {
          draftOrder { id tags }
          userErrors { field message code }
        }
      }
    ",
      dict.new(),
      empty_upstream_context(),
    )

  let long_tag = string.repeat("x", times: 256)
  let bulk_add_mutation =
    "
    mutation BulkAdd($ids: [ID!], $tags: [String!]!) {
      draftOrderBulkAddTags(ids: $ids, tags: $tags) {
        job { id done }
        userErrors { field message code }
      }
    }
  "
  let add_outcome =
    orders.process_mutation(
      create_outcome.store,
      create_outcome.identity,
      "/admin/api/2025-01/graphql.json",
      bulk_add_mutation,
      dict.from_list([
        #(
          "ids",
          root_field.ListVal([
            root_field.StringVal("gid://shopify/DraftOrder/1"),
            root_field.StringVal("gid://shopify/DraftOrder/999999"),
          ]),
        ),
        #(
          "tags",
          root_field.ListVal([
            root_field.StringVal(" added "),
            root_field.StringVal("ADDED"),
            root_field.StringVal(long_tag),
          ]),
        ),
      ]),
      empty_upstream_context(),
    )

  assert json.to_string(add_outcome.data)
    == "{\"data\":{\"draftOrderBulkAddTags\":{\"job\":{\"id\":\"gid://shopify/Job/3\",\"done\":false},\"userErrors\":[{\"field\":[\"input\",\"tags\",\"2\"],\"message\":\"tag_too_long\",\"code\":\"INVALID\"},{\"field\":[\"input\",\"ids\",\"1\"],\"message\":\"Draft order does not exist\",\"code\":\"NOT_FOUND\"}]}}}"

  let assert Ok(after_add_read) =
    orders.process(
      add_outcome.store,
      "query { draftOrder(id: \"gid://shopify/DraftOrder/1\") { id tags } }",
      dict.new(),
    )
  assert json.to_string(after_add_read)
    == "{\"data\":{\"draftOrder\":{\"id\":\"gid://shopify/DraftOrder/1\",\"tags\":[\"added\",\"Initial\"]}}}"
}

pub fn orders_draft_order_bulk_tags_count_validation_test() {
  let draft_order_id = "gid://shopify/DraftOrder/250"
  let existing_tags = numbered_draft_order_tags(250)
  let seeded_store =
    store.new()
    |> store.stage_draft_order(types.DraftOrderRecord(
      id: draft_order_id,
      cursor: None,
      data: types.CapturedObject([
        #("id", types.CapturedString(draft_order_id)),
        #(
          "tags",
          types.CapturedArray(list.map(existing_tags, types.CapturedString)),
        ),
      ]),
    ))

  let bulk_add_mutation =
    "
    mutation BulkAdd($ids: [ID!], $tags: [String!]!) {
      draftOrderBulkAddTags(ids: $ids, tags: $tags) {
        job { id done }
        userErrors { field message code }
      }
    }
  "
  let add_outcome =
    orders.process_mutation(
      seeded_store,
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      bulk_add_mutation,
      dict.from_list([
        #("ids", root_field.ListVal([root_field.StringVal(draft_order_id)])),
        #("tags", root_field.ListVal([root_field.StringVal("bulk-extra")])),
      ]),
      empty_upstream_context(),
    )

  assert json.to_string(add_outcome.data)
    == "{\"data\":{\"draftOrderBulkAddTags\":{\"job\":null,\"userErrors\":[{\"field\":[\"input\",\"tags\"],\"message\":\"too_many_tags\",\"code\":\"INVALID\"}]}}}"

  let assert Ok(after_add_read) =
    orders.process(
      add_outcome.store,
      "query { draftOrder(id: \"gid://shopify/DraftOrder/250\") { id tags } }",
      dict.new(),
    )
  let read_json = json.to_string(after_add_read)
  assert string.contains(read_json, "\"tag-250\"")
  assert !string.contains(read_json, "bulk-extra")
}

pub fn orders_draft_order_bulk_remove_tags_normalizes_identity_test() {
  let draft_order_id = "gid://shopify/DraftOrder/normalization"
  let seeded_store =
    store.new()
    |> store.stage_draft_order(types.DraftOrderRecord(
      id: draft_order_id,
      cursor: None,
      data: types.CapturedObject([
        #("id", types.CapturedString(draft_order_id)),
        #(
          "tags",
          types.CapturedArray([
            types.CapturedString("Initial"),
            types.CapturedString("vip"),
          ]),
        ),
      ]),
    ))

  let bulk_remove_mutation =
    "
    mutation BulkRemove($ids: [ID!], $tags: [String!]!) {
      draftOrderBulkRemoveTags(ids: $ids, tags: $tags) {
        job { id done }
        userErrors { field message code }
      }
    }
  "
  let remove_outcome =
    orders.process_mutation(
      seeded_store,
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      bulk_remove_mutation,
      dict.from_list([
        #("ids", root_field.ListVal([root_field.StringVal(draft_order_id)])),
        #("tags", root_field.ListVal([root_field.StringVal(" initial ")])),
      ]),
      empty_upstream_context(),
    )

  assert json.to_string(remove_outcome.data)
    == "{\"data\":{\"draftOrderBulkRemoveTags\":{\"job\":{\"id\":\"gid://shopify/Job/1\",\"done\":false},\"userErrors\":[]}}}"

  let assert Ok(after_remove_read) =
    orders.process(
      remove_outcome.store,
      "query { draftOrder(id: \"gid://shopify/DraftOrder/normalization\") { id tags } }",
      dict.new(),
    )
  assert json.to_string(after_remove_read)
    == "{\"data\":{\"draftOrder\":{\"id\":\"gid://shopify/DraftOrder/normalization\",\"tags\":[\"vip\"]}}}"
}

fn numbered_draft_order_tags(count: Int) -> List(String) {
  int.range(from: 1, to: count + 1, with: [], run: fn(acc, index) {
    ["tag-" <> int.to_string(index), ..acc]
  })
  |> list.reverse
}

pub fn orders_draft_order_calculate_validation_and_shipping_rates_test() {
  let outcome =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      "
      mutation {
        emptyLineItems: draftOrderCalculate(input: { lineItems: [] }) {
          calculatedDraftOrder { currencyCode }
          userErrors { field message }
        }
        invalidEmail: draftOrderCalculate(input: {
          email: \"bad email\"
          lineItems: [{ title: \"Bad email\", quantity: 1, originalUnitPrice: \"1.00\" }]
        }) {
          calculatedDraftOrder { currencyCode }
          userErrors { field message }
        }
        availableShippingRatesEmpty: draftOrderCalculate(input: {
          lineItems: [{ title: \"Needs shipping\", quantity: 1, originalUnitPrice: \"1.00\" }]
        }) {
          calculatedDraftOrder {
            availableShippingRates { handle title }
          }
          userErrors { field message }
        }
        paymentTermsTemplateId: draftOrderCalculate(input: {
          lineItems: [{ title: \"Payment terms\", quantity: 1, originalUnitPrice: \"1.00\" }]
          paymentTerms: {
            paymentTermsTemplateId: \"gid://shopify/PaymentTermsTemplate/4\"
            paymentSchedules: [{ issuedAt: \"2026-01-01T00:00:00Z\" }]
          }
        }) {
          calculatedDraftOrder { currencyCode }
          userErrors { field message }
        }
      }
    ",
      dict.new(),
      empty_upstream_context(),
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"emptyLineItems\":{\"calculatedDraftOrder\":null,\"userErrors\":[{\"field\":null,\"message\":\"Add at least 1 product\"}]},\"invalidEmail\":{\"calculatedDraftOrder\":null,\"userErrors\":[{\"field\":[\"email\"],\"message\":\"Email is invalid\"}]},\"availableShippingRatesEmpty\":{\"calculatedDraftOrder\":{\"availableShippingRates\":[]},\"userErrors\":[]},\"paymentTermsTemplateId\":{\"calculatedDraftOrder\":{\"currencyCode\":\"CAD\"},\"userErrors\":[]}}}"
}

pub fn orders_order_create_mandate_payment_uses_composite_reference_and_sale_test() {
  let create_outcome = create_mandate_payment_test_order()
  let order_id = "gid://shopify/Order/1"

  let outcome =
    create_mandate_payment_for_order(
      create_outcome.store,
      create_outcome.identity,
      order_id,
      "abc123",
      None,
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"orderCreateMandatePayment\":{\"job\":{\"id\":\"gid://shopify/Job/5\",\"done\":true},\"paymentReferenceId\":\"gid://shopify/Order/1/abc123\",\"order\":{\"id\":\"gid://shopify/Order/1\",\"displayFinancialStatus\":\"PAID\",\"capturable\":false,\"totalCapturable\":\"0.0\",\"transactions\":[{\"kind\":\"SALE\",\"status\":\"SUCCESS\",\"gateway\":\"mandate\",\"paymentReferenceId\":\"gid://shopify/Order/1/abc123\"}]},\"userErrors\":[]}}}"
}

pub fn orders_order_create_mandate_payment_repeat_is_idempotent_test() {
  let create_outcome = create_mandate_payment_test_order()
  let order_id = "gid://shopify/Order/1"

  let first =
    create_mandate_payment_for_order(
      create_outcome.store,
      create_outcome.identity,
      order_id,
      "abc123",
      None,
    )
  let repeat =
    create_mandate_payment_for_order(
      first.store,
      first.identity,
      order_id,
      "abc123",
      None,
    )

  assert json.to_string(repeat.data)
    == "{\"data\":{\"orderCreateMandatePayment\":{\"job\":{\"id\":\"gid://shopify/Job/5\",\"done\":true},\"paymentReferenceId\":\"gid://shopify/Order/1/abc123\",\"order\":{\"id\":\"gid://shopify/Order/1\",\"displayFinancialStatus\":\"PAID\",\"capturable\":false,\"totalCapturable\":\"0.0\",\"transactions\":[{\"kind\":\"SALE\",\"status\":\"SUCCESS\",\"gateway\":\"mandate\",\"paymentReferenceId\":\"gid://shopify/Order/1/abc123\"}]},\"userErrors\":[]}}}"
}

pub fn orders_order_create_mandate_payment_auto_capture_false_authorizes_test() {
  let create_outcome = create_mandate_payment_test_order()
  let order_id = "gid://shopify/Order/1"

  let outcome =
    create_mandate_payment_for_order(
      create_outcome.store,
      create_outcome.identity,
      order_id,
      "auth-only",
      Some(False),
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"orderCreateMandatePayment\":{\"job\":{\"id\":\"gid://shopify/Job/5\",\"done\":true},\"paymentReferenceId\":\"gid://shopify/Order/1/auth-only\",\"order\":{\"id\":\"gid://shopify/Order/1\",\"displayFinancialStatus\":\"AUTHORIZED\",\"capturable\":true,\"totalCapturable\":\"25.0\",\"transactions\":[{\"kind\":\"AUTHORIZATION\",\"status\":\"SUCCESS\",\"gateway\":\"mandate\",\"paymentReferenceId\":\"gid://shopify/Order/1/auth-only\"}]},\"userErrors\":[]}}}"
}

fn create_mandate_payment_test_order() -> MutationOutcome {
  orders.process_mutation(
    store.new(),
    synthetic_identity.new(),
    "/admin/api/2025-01/graphql.json",
    "
      mutation Create($order: OrderCreateOrderInput!) {
        orderCreate(order: $order) {
          order { id }
          userErrors { field message }
        }
      }
    ",
    dict.from_list([
      #(
        "order",
        root_field.ObjectVal(
          dict.from_list([
            #("currency", root_field.StringVal("CAD")),
            #("transactions", root_field.ListVal([])),
            #(
              "lineItems",
              root_field.ListVal([
                root_field.ObjectVal(
                  dict.from_list([
                    #("title", root_field.StringVal("Mandate item")),
                    #("quantity", root_field.IntVal(1)),
                    #(
                      "priceSet",
                      root_field.ObjectVal(
                        dict.from_list([
                          #(
                            "shopMoney",
                            root_field.ObjectVal(
                              dict.from_list([
                                #("amount", root_field.StringVal("25.00")),
                                #("currencyCode", root_field.StringVal("CAD")),
                              ]),
                            ),
                          ),
                        ]),
                      ),
                    ),
                  ]),
                ),
              ]),
            ),
          ]),
        ),
      ),
    ]),
    empty_upstream_context(),
  )
}

fn create_mandate_payment_for_order(
  store: Store,
  identity: SyntheticIdentityRegistry,
  order_id: String,
  idempotency_key: String,
  auto_capture: Option(Bool),
) -> MutationOutcome {
  let variables =
    dict.from_list([
      #("id", root_field.StringVal(order_id)),
      #("mandateId", root_field.StringVal("gid://shopify/PaymentMandate/test")),
      #("idempotencyKey", root_field.StringVal(idempotency_key)),
      #("autoCapture", case auto_capture {
        Some(value) -> root_field.BoolVal(value)
        None -> root_field.NullVal
      }),
    ])
  orders.process_mutation(
    store,
    identity,
    "/admin/api/2025-01/graphql.json",
    "
      mutation Mandate($id: ID!, $mandateId: ID!, $idempotencyKey: String!, $autoCapture: Boolean) {
        orderCreateMandatePayment(id: $id, mandateId: $mandateId, idempotencyKey: $idempotencyKey, autoCapture: $autoCapture) {
          job { id done }
          paymentReferenceId
          order {
            id
            displayFinancialStatus
            capturable
            totalCapturable
            transactions {
              kind
              status
              gateway
              paymentReferenceId
            }
          }
          userErrors { field message }
        }
      }
    ",
    variables,
    empty_upstream_context(),
  )
}

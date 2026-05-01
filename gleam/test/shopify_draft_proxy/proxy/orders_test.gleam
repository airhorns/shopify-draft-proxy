import gleam/dict.{type Dict}
import gleam/json
import gleam/list
import gleam/option.{None, Some}
import gleam/string
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/orders
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/synthetic_identity
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
        }
      }
    }
  "
  let assert Ok(outcome) =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      query,
      dict.new(),
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"abandonmentUpdateActivitiesDeliveryStatuses\":{\"abandonment\":null,\"userErrors\":[{\"field\":[\"abandonmentId\"],\"message\":\"abandonment_not_found\"}]}}}"
  assert outcome.staged_resource_ids == []
  assert list.length(outcome.log_drafts) == 1
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
        }
      }
    }
  "
  let assert Ok(outcome) =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      query,
      dict.new(),
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
        }
      }
    }
  "
  let variables = dict.from_list([#("id", root_field.StringVal(order_id))])
  let assert Ok(outcome) =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      mutation,
      variables,
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
  let assert Ok(outcome) =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      mutation,
      variables,
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"orderEditAddVariant\":{\"calculatedLineItem\":{\"id\":\"gid://shopify/CalculatedLineItem/1\",\"title\":\"VANS |AUTHENTIC | LO PRO | BURGANDY/WHITE\",\"quantity\":1,\"currentQuantity\":1,\"sku\":\"VN-01-burgandy-4\",\"variant\":{\"id\":\"gid://shopify/ProductVariant/46789254021353\"},\"originalUnitPriceSet\":{\"shopMoney\":{\"amount\":\"29.0\",\"currencyCode\":\"CAD\"}}},\"orderEditSession\":{\"id\":\"gid://shopify/OrderEditSession/10\"},\"userErrors\":[]}}}"
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
  let assert Ok(outcome) =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      mutation,
      variables,
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"orderEditAddVariant\":{\"calculatedOrder\":null,\"calculatedLineItem\":null,\"orderEditSession\":null,\"userErrors\":[{\"field\":[\"variantId\"],\"message\":\"can't convert Integer[0] to a positive Integer to use as an untrusted id\"}]}}}"
  assert outcome.staged_resource_ids == []
  assert outcome.log_drafts == []
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
  let assert Ok(outcome) =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      mutation,
      variables,
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"orderEditSetQuantity\":{\"calculatedLineItem\":{\"title\":\"VANS |AUTHENTIC | LO PRO | BURGANDY/WHITE\",\"quantity\":0,\"currentQuantity\":0,\"sku\":\"VN-01-burgandy-4\",\"variant\":{\"id\":\"gid://shopify/ProductVariant/46789254021353\"},\"originalUnitPriceSet\":{\"shopMoney\":{\"amount\":\"29.0\",\"currencyCode\":\"CAD\"}}},\"userErrors\":[]}}}"
  assert outcome.staged_resource_ids == []
  assert outcome.log_drafts == []
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
  let assert Ok(missing_outcome) =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      missing_input,
      dict.new(),
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
  let assert Ok(null_outcome) =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      null_input,
      dict.new(),
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
  let assert Ok(outcome) =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      mutation,
      dict.new(),
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"noLineItems\":{\"draftOrder\":null,\"userErrors\":[{\"field\":null,\"message\":\"Add at least 1 product\"}]},\"unknownVariant\":{\"draftOrder\":null,\"userErrors\":[{\"field\":null,\"message\":\"Product with ID 999999999999999999 is no longer available.\"}]},\"customMissingTitle\":{\"draftOrder\":null,\"userErrors\":[{\"field\":null,\"message\":\"Merchandise title is empty.\"}]},\"zeroQuantity\":{\"draftOrder\":null,\"userErrors\":[{\"field\":[\"lineItems\",\"0\",\"quantity\"],\"message\":\"Quantity must be greater than or equal to 1\"}]},\"paymentTerms\":{\"draftOrder\":null,\"userErrors\":[{\"field\":null,\"message\":\"Payment terms template id can not be empty.\"}]},\"negativePrice\":{\"draftOrder\":null,\"userErrors\":[{\"field\":null,\"message\":\"Cannot send negative price for line_item\"}]},\"pastReserve\":{\"draftOrder\":null,\"userErrors\":[{\"field\":null,\"message\":\"Reserve until can't be in the past\"}]},\"badEmail\":{\"draftOrder\":null,\"userErrors\":[{\"field\":[\"email\"],\"message\":\"Email is invalid\"}]}}}"
  assert list.length(outcome.log_drafts) == 8
  assert store.list_effective_draft_orders(outcome.store) == []
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
  let assert Ok(missing_outcome) =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      missing_id,
      dict.new(),
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
  let assert Ok(null_outcome) =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      null_id,
      dict.new(),
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
  let assert Ok(cancel_missing_outcome) =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      cancel_missing_id,
      dict.new(),
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
  let assert Ok(cancel_null_outcome) =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      cancel_null_id,
      dict.new(),
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
  let assert Ok(tracking_missing_outcome) =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      tracking_missing_id,
      tracking_variables,
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
  let assert Ok(tracking_null_outcome) =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      tracking_null_id,
      tracking_variables,
    )
  assert json.to_string(tracking_null_outcome.data)
    == "{\"errors\":[{\"message\":\"Argument 'fulfillmentId' on Field 'fulfillmentTrackingInfoUpdate' has an invalid value (null). Expected type 'ID!'.\",\"locations\":[{\"line\":6,\"column\":7}],\"path\":[\"mutation FulfillmentTrackingInfoUpdateInlineNullId\",\"fulfillmentTrackingInfoUpdate\",\"fulfillmentId\"],\"extensions\":{\"code\":\"argumentLiteralsIncompatible\",\"typeName\":\"Field\",\"argumentName\":\"fulfillmentId\"}}]}"
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
  let assert Ok(tracking_outcome) =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      tracking_mutation,
      tracking_variables,
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
  let assert Ok(cancel_outcome) =
    orders.process_mutation(
      tracking_outcome.store,
      tracking_outcome.identity,
      "/admin/api/2025-01/graphql.json",
      cancel_mutation,
      cancel_variables,
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
  let assert Ok(outcome) =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      mutation,
      variables,
    )
  assert json.to_string(outcome.data)
    == "{\"errors\":[{\"message\":\"invalid id\",\"extensions\":{\"code\":\"RESOURCE_NOT_FOUND\"},\"path\":[\"fulfillmentCreate\"]}],\"data\":{\"fulfillmentCreate\":null}}"
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
  let assert Ok(create_outcome) =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      create_mutation,
      create_variables,
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
  let assert Ok(event_outcome) =
    orders.process_mutation(
      create_outcome.store,
      create_outcome.identity,
      "/admin/api/2026-04/graphql.json",
      event_mutation,
      event_variables,
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
  let assert Ok(outcome) =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      mutation,
      variables,
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"refundCreate\":{\"refund\":null,\"order\":{\"id\":\"gid://shopify/Order/6830465417449\",\"displayFinancialStatus\":\"PAID\",\"totalRefundedSet\":{\"shopMoney\":{\"amount\":\"0.0\",\"currencyCode\":\"CAD\"}}},\"userErrors\":[{\"field\":null,\"message\":\"Refund amount $25.00 is greater than net payment received $15.00\"}]}}}"
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
  let assert Ok(outcome) =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      mutation,
      variables,
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
  let assert Ok(outcome) =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      mutation,
      variables,
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
          #("displayFinancialStatus", types.CapturedString("PAID")),
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
        }
      }
    }
  "
  let variables = dict.from_list([#("orderId", root_field.StringVal(order_id))])
  let assert Ok(outcome) =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      mutation,
      variables,
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

  let assert Ok(repeated) =
    orders.process_mutation(
      outcome.store,
      outcome.identity,
      "/admin/api/2026-04/graphql.json",
      mutation,
      variables,
    )
  assert json.to_string(repeated.data)
    == "{\"data\":{\"orderDelete\":{\"deletedId\":null,\"userErrors\":[{\"field\":[\"orderId\"],\"message\":\"Order does not exist\"}]}}}"
  assert repeated.staged_resource_ids == []
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
  let assert Ok(missing_outcome) =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      missing_order,
      dict.new(),
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
  let assert Ok(null_outcome) =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      null_order,
      dict.new(),
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
  let assert Ok(no_line_items_outcome) =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      no_line_items,
      variables,
    )
  assert json.to_string(no_line_items_outcome.data)
    == "{\"data\":{\"orderCreate\":{\"order\":null,\"userErrors\":[{\"field\":[\"order\",\"lineItems\"],\"message\":\"Line items must have at least one line item\"}]}}}"
  assert no_line_items_outcome.staged_resource_ids == []
  assert no_line_items_outcome.log_drafts == []
  assert store.list_effective_orders(no_line_items_outcome.store) == []
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
  let assert Ok(outcome) =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      mutation,
      variables,
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
  let assert Ok(missing_inline_outcome) =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      missing_inline_id,
      dict.new(),
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
  let assert Ok(null_inline_outcome) =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      null_inline_id,
      dict.new(),
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
  let assert Ok(missing_variable_outcome) =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      missing_variable_id,
      variables,
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
  let assert Ok(unknown_outcome) =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      unknown_id,
      unknown_variables,
    )
  assert json.to_string(unknown_outcome.data)
    == "{\"data\":{\"orderUpdate\":{\"order\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"Order does not exist\"}]}}}"
  assert unknown_outcome.staged_resource_ids == []
  assert unknown_outcome.log_drafts == []
  assert store.list_effective_orders(unknown_outcome.store) == []
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
  let assert Ok(outcome) =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      mutation,
      variables,
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
  let assert Ok(close_outcome) =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      close_mutation,
      dict.new(),
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
  let assert Ok(open_outcome) =
    orders.process_mutation(
      close_outcome.store,
      close_outcome.identity,
      "/admin/api/2026-04/graphql.json",
      open_mutation,
      dict.new(),
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
  let assert Ok(outcome) =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      mutation,
      variables,
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
  let assert Ok(outcome) =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      mutation,
      variables,
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"orderInvoiceSend\":{\"order\":{\"id\":\"gid://shopify/Order/6830646329577\",\"name\":\"#1328\",\"closed\":false,\"closedAt\":null,\"cancelledAt\":null,\"cancelReason\":null,\"displayFinancialStatus\":null},\"userErrors\":[]}}}"
  assert outcome.staged_resource_ids == []
  assert outcome.log_drafts == []
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
  let assert Ok(outcome) =
    orders.process_mutation(
      seeded,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      mutation,
      variables,
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"orderMarkAsPaid\":{\"order\":{\"id\":\"gid://shopify/Order/6830647771369\",\"displayFinancialStatus\":\"PAID\",\"paymentGatewayNames\":[\"manual\"],\"totalOutstandingSet\":{\"shopMoney\":{\"amount\":\"0.0\",\"currencyCode\":\"CAD\"}},\"transactions\":[{\"kind\":\"SALE\",\"status\":\"SUCCESS\",\"gateway\":\"manual\",\"amountSet\":{\"shopMoney\":{\"amount\":\"19.0\",\"currencyCode\":\"CAD\"}}}]},\"userErrors\":[]}}}"
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
          }
        }
      }
    }
  "
  let assert Ok(read) = orders.process(outcome.store, read_query, dict.new())
  assert json.to_string(read)
    == "{\"data\":{\"order\":{\"id\":\"gid://shopify/Order/6830647771369\",\"displayFinancialStatus\":\"PAID\",\"paymentGatewayNames\":[\"manual\"],\"totalOutstandingSet\":{\"shopMoney\":{\"amount\":\"0.0\",\"currencyCode\":\"CAD\"}},\"transactions\":[{\"kind\":\"SALE\",\"status\":\"SUCCESS\",\"gateway\":\"manual\",\"amountSet\":{\"shopMoney\":{\"amount\":\"19.0\",\"currencyCode\":\"CAD\"}}}]}}}"
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
  let assert Ok(outcome) =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      document,
      variables,
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
  let assert Ok(outcome) =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      mutation,
      dict.new(),
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
  let assert Ok(create_outcome) =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      create_mutation,
      dict.new(),
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
  let assert Ok(update_outcome) =
    orders.process_mutation(
      create_outcome.store,
      create_outcome.identity,
      "/admin/api/2025-01/graphql.json",
      update_mutation,
      update_variables,
    )

  assert json.to_string(update_outcome.data)
    == "{\"data\":{\"draftOrderUpdate\":{\"draftOrder\":{\"id\":\"gid://shopify/DraftOrder/1\",\"name\":\"#D1\",\"status\":\"OPEN\",\"email\":\"updated-draft@example.test\",\"note\":\"Updated note\",\"tags\":[\"draft\",\"updated\"],\"customAttributes\":[{\"key\":\"source\",\"value\":\"har-492-update\"}],\"shippingLine\":{\"title\":\"Standard\",\"code\":\"custom\",\"originalPriceSet\":{\"shopMoney\":{\"amount\":\"5.0\",\"currencyCode\":\"CAD\"}}},\"subtotalPriceSet\":{\"shopMoney\":{\"amount\":\"25.0\",\"currencyCode\":\"CAD\"}},\"totalShippingPriceSet\":{\"shopMoney\":{\"amount\":\"5.0\",\"currencyCode\":\"CAD\"}},\"totalPriceSet\":{\"shopMoney\":{\"amount\":\"30.0\",\"currencyCode\":\"CAD\"}},\"totalQuantityOfLineItems\":2,\"lineItems\":{\"nodes\":[{\"id\":\"gid://shopify/DraftOrderLineItem/3\",\"title\":\"Updated custom item\",\"quantity\":2,\"sku\":\"HAR-492-UPDATED\",\"originalUnitPriceSet\":{\"shopMoney\":{\"amount\":\"12.5\",\"currencyCode\":\"CAD\"}}}]}},\"userErrors\":[]}}}"
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
  let assert Ok(create_outcome) =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      create_mutation,
      dict.new(),
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
  let assert Ok(duplicate_outcome) =
    orders.process_mutation(
      create_outcome.store,
      create_outcome.identity,
      "/admin/api/2025-01/graphql.json",
      duplicate_mutation,
      dict.from_list([
        #("id", root_field.StringVal("gid://shopify/DraftOrder/1")),
      ]),
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
  let assert Ok(create_outcome) =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      create_mutation,
      dict.new(),
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
  let assert Ok(complete_outcome) =
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
    )

  assert json.to_string(complete_outcome.data)
    == "{\"data\":{\"draftOrderComplete\":{\"draftOrder\":{\"id\":\"gid://shopify/DraftOrder/1\",\"name\":\"#D1\",\"status\":\"COMPLETED\",\"ready\":true,\"completedAt\":\"2024-01-01T00:00:01.000Z\",\"totalPriceSet\":{\"shopMoney\":{\"amount\":\"25.0\",\"currencyCode\":\"CAD\"}},\"lineItems\":{\"nodes\":[{\"id\":\"gid://shopify/DraftOrderLineItem/2\",\"title\":\"Completion service\",\"quantity\":2,\"sku\":\"COMPLETE\"}]},\"order\":{\"id\":\"gid://shopify/Order/3\",\"name\":\"#1\",\"sourceName\":\"347082227713\",\"paymentGatewayNames\":[\"manual\"],\"displayFinancialStatus\":\"PAID\",\"displayFulfillmentStatus\":\"UNFULFILLED\",\"note\":\"complete this staged draft locally\",\"tags\":[\"draft-complete\",\"gleam\"],\"customAttributes\":[{\"key\":\"source\",\"value\":\"direct-test\"}],\"billingAddress\":{\"firstName\":\"Hermes\",\"lastName\":\"Closer\",\"address1\":\"123 Queen St W\",\"city\":\"Toronto\",\"provinceCode\":\"ON\",\"countryCodeV2\":\"CA\",\"zip\":\"M5H 2M9\"},\"currentTotalPriceSet\":{\"shopMoney\":{\"amount\":\"25.0\",\"currencyCode\":\"CAD\"}},\"lineItems\":{\"nodes\":[{\"id\":\"gid://shopify/LineItem/4\",\"title\":\"Completion service\",\"quantity\":2,\"sku\":\"COMPLETE\",\"variantTitle\":null,\"originalUnitPriceSet\":{\"shopMoney\":{\"amount\":\"12.5\",\"currencyCode\":\"CAD\"}}}]}}},\"userErrors\":[]}}}"
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
  let assert Ok(create_outcome) =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      create_mutation,
      dict.new(),
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
  let assert Ok(complete_outcome) =
    orders.process_mutation(
      create_outcome.store,
      create_outcome.identity,
      "/admin/api/2025-01/graphql.json",
      complete_mutation,
      dict.from_list([
        #("id", root_field.StringVal("gid://shopify/DraftOrder/1")),
      ]),
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
  let assert Ok(outcome) =
    orders.process_mutation(
      complete_outcome.store,
      complete_outcome.identity,
      "/admin/api/2025-01/graphql.json",
      mutation,
      dict.from_list([
        #("orderId", root_field.StringVal("gid://shopify/Order/3")),
      ]),
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

  let assert Ok(missing_outcome) =
    orders.process_mutation(
      outcome.store,
      outcome.identity,
      "/admin/api/2025-01/graphql.json",
      mutation,
      dict.from_list([
        #("orderId", root_field.StringVal("gid://shopify/Order/404")),
      ]),
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
  let assert Ok(unknown_outcome) =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      mutation,
      dict.from_list([
        #("id", root_field.StringVal("gid://shopify/DraftOrder/999")),
      ]),
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
  let assert Ok(create_outcome) =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      create_mutation,
      dict.new(),
    )
  let assert Ok(open_outcome) =
    orders.process_mutation(
      create_outcome.store,
      create_outcome.identity,
      "/admin/api/2025-01/graphql.json",
      mutation,
      dict.from_list([
        #("id", root_field.StringVal("gid://shopify/DraftOrder/1")),
      ]),
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

  let assert Ok(calculate_outcome) =
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
  let assert Ok(create_outcome) =
    orders.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2025-01/graphql.json",
      create_mutation,
      dict.new(),
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
  let assert Ok(preview_outcome) =
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
  let assert Ok(add_outcome) =
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
  let assert Ok(remove_outcome) =
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
  let assert Ok(delete_outcome) =
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

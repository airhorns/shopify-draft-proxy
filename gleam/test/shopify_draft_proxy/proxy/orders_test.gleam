import gleam/dict
import gleam/json
import gleam/list
import shopify_draft_proxy/proxy/orders
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/synthetic_identity

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

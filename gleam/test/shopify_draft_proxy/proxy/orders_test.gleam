import gleam/dict.{type Dict}
import gleam/json
import gleam/list
import gleam/option.{None}
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

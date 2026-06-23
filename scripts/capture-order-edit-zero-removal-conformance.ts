/* oxlint-disable no-console -- CLI capture script intentionally writes progress to stdio. */
import {
  createConformanceCapture,
  readArray,
  readRecord,
  requireString,
  type JsonRecord,
} from './conformance-capture-lib.js';

// De-seeded re-record of the existing-order zero-removal order-edit fixture (HAR-115), re-homed
// from very-big-test-store onto harry-test-heelo.
//
// The very-big fixture seeded the precondition order via a top-level `seedOrder` key and carried a
// hand-synthesized order hydrate cassette (`query: "hand-synthesized from checked-in seedOrder ..."`)
// that could never byte-match the proxy's real ORDER_EDIT_HYDRATE_QUERY. Both are /__meta/seed
// affordances being removed. This re-record resolves the precondition the real way: begin forwards a
// cold `OrdersOrderEditHydrate` order hydrate, so the single upstreamCall is a byte-matching forward
// cassette rather than a seed source.
//
// Three custom line items (no variant lines) keep the scenario free of any variant hydrate: the
// edit zeros the third line (`lineItems.nodes[2]`) with `restock: true`, commits, and reads the
// order back — the zeroed line keeps its historical `quantity` while `currentQuantity` drops to 0.
// The order is a disposable test order, cancelled afterward to leave the store clean.
const cap = await createConformanceCapture();

// The exact order hydrate query the proxy forwards, byte-identical to ORDER_EDIT_HYDRATE_QUERY
// (`include_str!` of order-edit-hydrate.graphql) so the recorded cassette replays verbatim.
const orderEditHydrateQuery = await cap.readRequestRaw('orders', 'order-edit-hydrate.graphql');

// Proxy request documents under test — run live for the ground-truth responses.
const beginDocument = await cap.readRequest('orders', 'orderEditExistingWorkflow-begin.graphql');
const setQuantityPayloadDocument = await cap.readRequest(
  'orders',
  'orderEditExistingWorkflow-setQuantity-payload.graphql',
);
const commitDocument = await cap.readRequest('orders', 'orderEditExistingWorkflow-commit.graphql');
const downstreamReadDocument = await cap.readRequest('orders', 'orderEditExistingWorkflow-downstream-read.graphql');

const orderCreateMutation = `#graphql
  mutation OrderEditZeroCaptureCreate($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
    orderCreate(order: $order, options: $options) {
      order { id name merchantEditable }
      userErrors { field message }
    }
  }
`;
const cancelMutation = `#graphql
  mutation OrderEditZeroCaptureCancel($orderId: ID!, $reason: OrderCancelReason!, $notifyCustomer: Boolean!, $restock: Boolean!, $refund: Boolean!) {
    orderCancel(orderId: $orderId, reason: $reason, notifyCustomer: $notifyCustomer, restock: $restock, refund: $refund) {
      job { id }
      userErrors { field message }
    }
  }
`;

function orderHydrateCassette(orderId: string, body: JsonRecord): JsonRecord {
  return {
    operationName: 'OrdersOrderEditHydrate',
    variables: { id: orderId },
    query: orderEditHydrateQuery,
    response: { status: 200, body },
  };
}

const shopCurrencyPayload = await cap.run(
  `#graphql
  query OrderEditZeroCaptureShop { shop { currencyCode } }
`,
  {},
  'discover shop currency',
);
const shopCurrency = requireString(
  readRecord(readRecord(shopCurrencyPayload['data'])?.['shop'])?.['currencyCode'],
  'shop currencyCode',
);

// ── Create a 3-line custom-line order, begin, zero the third line, commit, read back ──────────
const order = await (async () => {
  const payload = await cap.run(
    orderCreateMutation,
    {
      order: {
        email: `har-115-order-edit-zero-${cap.stamp}@example.com`,
        note: 'HAR-115 order edit existing-order zero-removal capture',
        tags: ['har-115', 'order-edit', 'deseed', 'zero-removal'],
        test: true,
        currency: shopCurrency,
        lineItems: [
          {
            title: `HAR-115 zero line A ${cap.stamp}`,
            quantity: 2,
            priceSet: { shopMoney: { amount: '20.00', currencyCode: shopCurrency } },
            requiresShipping: false,
            taxable: false,
          },
          {
            title: `HAR-115 zero line B ${cap.stamp}`,
            quantity: 1,
            priceSet: { shopMoney: { amount: '12.00', currencyCode: shopCurrency } },
            requiresShipping: false,
            taxable: false,
          },
          {
            title: `HAR-115 zero line C ${cap.stamp}`,
            quantity: 1,
            priceSet: { shopMoney: { amount: '29.00', currencyCode: shopCurrency } },
            requiresShipping: false,
            taxable: false,
          },
        ],
      },
      options: { inventoryBehaviour: 'BYPASS', sendReceipt: false, sendFulfillmentReceipt: false },
    },
    'create zero-removal order',
  );
  const root = cap.mutationRoot(payload, 'orderCreate', 'create zero-removal order');
  return {
    id: requireString(readRecord(root['order'])?.['id'], 'zero-removal order id'),
    name: readRecord(root['order'])?.['name'] ?? null,
  };
})();

const orderHydrate = orderHydrateCassette(
  order.id,
  await cap.run(orderEditHydrateQuery, { id: order.id }, 'order hydrate'),
);

const begin = await cap.run(beginDocument, { id: order.id }, 'begin');
const beginCalc = readRecord(cap.mutationRoot(begin, 'orderEditBegin', 'begin')['calculatedOrder']);
const calcId = requireString(beginCalc?.['id'], 'calculatedOrder id');
const beginNodes = readArray(readRecord(beginCalc?.['lineItems'])?.['nodes']).map(readRecord);
if (beginNodes.length < 3) {
  throw new Error(`expected >=3 calculated line items to zero nodes[2], got ${beginNodes.length}`);
}
const targetLineItemId = requireString(beginNodes[2]?.['id'], 'calculatedOrder lineItems nodes[2] id');

const setZero = await cap.run(
  setQuantityPayloadDocument,
  { id: calcId, lineItemId: targetLineItemId, quantity: 0, restock: true },
  'set quantity zero',
);
cap.mutationRoot(setZero, 'orderEditSetQuantity', 'set quantity zero');

const commitRemove = await cap.run(
  commitDocument,
  { id: calcId, notifyCustomer: false, staffNote: 'HAR-115 order edit cleanup capture' },
  'commit removal',
);
cap.mutationRoot(commitRemove, 'orderEditCommit', 'commit removal');

const downstreamRead = await cap.run(downstreamReadDocument, { id: order.id }, 'downstream read');

// Locate the zeroed line (the one whose title matches line C) in the live downstream read so the
// spec's capturePath index can be set deterministically against real Shopify ordering.
const downstreamNodes = readArray(
  readRecord(readRecord(readRecord(downstreamRead['data'])?.['order'])?.['lineItems'])?.['nodes'],
).map(readRecord);
const zeroedTitle = beginNodes[2]?.['title'];
const downstreamZeroIndex = downstreamNodes.findIndex((node) => node?.['title'] === zeroedTitle);

const fixturePath = cap.fixturePath('orders', 'order-edit-existing-order-zero-removal.json');
await cap.writeJson(fixturePath, {
  scenarioId: 'order-edit-existing-order-zero-removal',
  apiVersion: cap.apiVersion,
  storeDomain: cap.storeDomain,
  recordedAt: new Date().toISOString(),
  source: 'live-shopify-admin-graphql',
  variables: { id: order.id },
  mutation: { response: begin },
  setZero: { variables: { quantity: 0, restock: true }, response: setZero },
  commitRemove: {
    variables: { notifyCustomer: false, staffNote: 'HAR-115 order edit cleanup capture' },
    response: commitRemove,
  },
  downstreamRead: { response: downstreamRead },
  upstreamCalls: [orderHydrate],
});
const cancelStatus = (
  await cap.runGraphqlRequest(cancelMutation, {
    orderId: order.id,
    reason: 'OTHER',
    notifyCustomer: false,
    restock: false,
    refund: false,
  })
).status;

console.log(
  JSON.stringify(
    {
      phase: 'done',
      fixturePath,
      shopCurrency,
      orderId: order.id,
      orderName: order.name,
      calcId,
      targetLineItemId,
      zeroedTitle,
      downstreamZeroIndex,
      downstreamNodes: downstreamNodes.map((node) => ({
        title: node?.['title'],
        quantity: node?.['quantity'],
        currentQuantity: node?.['currentQuantity'],
      })),
      setZeroPayload: readRecord(readRecord(setZero['data'])?.['orderEditSetQuantity']),
      cancelStatus,
    } satisfies JsonRecord,
    null,
    2,
  ),
);

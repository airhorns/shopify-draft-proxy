/* oxlint-disable no-console -- CLI capture script intentionally writes progress to stdio. */
import {
  createConformanceCapture,
  readArray,
  readRecord,
  requireString,
  type JsonRecord,
} from './conformance-capture-lib.js';

// De-seeded re-record of the existing-order order-edit workflow fixtures (HAR-115 happy-path +
// validation), re-homed from very-big-test-store onto harry-test-heelo.
//
// The very-big fixtures seeded the precondition order via a top-level `seedOrder` key and
// auto-derived `seedOrderEditVariants` from a hand-synthesized `productVariant` hydrate. Both are
// /__meta/seed affordances being removed. This re-record resolves the precondition the real way:
//   - begin forwards a cold `OrdersOrderEditHydrate` order hydrate (ORDER_EDIT_HYDRATE_QUERY)
//   - addVariant forwards a cold `OrdersDraftOrderVariantHydrate` variant hydrate on a catalog miss
// so the upstreamCalls are byte-matching forward cassettes, not seed sources. (The validation
// fixture needs only the order hydrate: its duplicate-variant add short-circuits to the existing
// session line and the invalid-variant probe errors before any hydrate — so no productVariant
// hydrate is recorded there, keeping it free of the auto-derived variant seed.)
//
// All orders are disposable test orders, cancelled afterward to leave the store clean.
const cap = await createConformanceCapture();

// The exact hydrate queries the proxy forwards, byte-identical to the Rust constants so the
// recorded cassettes replay verbatim (the matcher trims trailing ws). ORDER_EDIT_HYDRATE_QUERY is
// `include_str!` of order-edit-hydrate.graphql; the variant hydrate is an inline Rust const.
const orderEditHydrateQuery = await cap.readRequestRaw('orders', 'order-edit-hydrate.graphql');
const variantHydrateQuery =
  'query OrdersDraftOrderVariantHydrate($id: ID!) {\n  productVariant(id: $id) { id title sku taxable price inventoryItem { requiresShipping } product { title } }\n}\n';

// Proxy request documents under test — run live for the ground-truth responses.
const beginDocument = await cap.readRequest('orders', 'orderEditExistingWorkflow-begin.graphql');
const addVariantDocument = await cap.readRequest('orders', 'orderEditExistingWorkflow-addVariant.graphql');
const addVariantPayloadDocument = await cap.readRequest(
  'orders',
  'orderEditExistingWorkflow-addVariant-payload.graphql',
);
const commitDocument = await cap.readRequest('orders', 'orderEditExistingWorkflow-commit.graphql');

const orderCreateMutation = `#graphql
  mutation OrderEditCaptureCreate($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
    orderCreate(order: $order, options: $options) {
      order { id name merchantEditable }
      userErrors { field message }
    }
  }
`;
const cancelMutation = `#graphql
  mutation OrderEditCaptureCancel($orderId: ID!, $reason: OrderCancelReason!, $notifyCustomer: Boolean!, $restock: Boolean!, $refund: Boolean!) {
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
function variantHydrateCassette(variantId: string, body: JsonRecord): JsonRecord {
  return {
    operationName: 'OrdersDraftOrderVariantHydrate',
    variables: { id: variantId },
    query: variantHydrateQuery,
    response: { status: 200, body },
  };
}
async function createOrder(input: JsonRecord, label: string): Promise<{ id: string; name: unknown }> {
  const payload = await cap.run(
    orderCreateMutation,
    { order: input, options: { inventoryBehaviour: 'BYPASS', sendReceipt: false, sendFulfillmentReceipt: false } },
    label,
  );
  const root = cap.mutationRoot(payload, 'orderCreate', label);
  return {
    id: requireString(readRecord(root['order'])?.['id'], `${label} order id`),
    name: readRecord(root['order'])?.['name'] ?? null,
  };
}
async function cancelOrder(orderId: string): Promise<number> {
  const cancel = await cap.runGraphqlRequest(cancelMutation, {
    orderId,
    reason: 'OTHER',
    notifyCustomer: false,
    restock: false,
    refund: false,
  });
  return cancel.status;
}

// ── Discover live store state: currency, and a variant that can be added at a location ───────
// `orderEditAddVariant` with a `locationId` rejects a tracked variant that is not stocked there
// ("This variant is not stocked at this location."), so resolve a (variant, location) pair that
// can actually take the add: an untracked variant (any active location) or a tracked variant with
// available stock at a specific location.
const discovery = await cap.run(
  `#graphql
    query OrderEditCaptureDiscover {
      shop { currencyCode }
      locations(first: 10) { nodes { id isActive } }
      products(first: 50) {
        nodes {
          variants(first: 10) {
            nodes {
              id
              price
              inventoryItem {
                tracked
                inventoryLevels(first: 10) {
                  nodes { location { id } quantities(names: ["available"]) { name quantity } }
                }
              }
            }
          }
        }
      }
    }
  `,
  {},
  'discover store state',
);
const discoveryData = readRecord(discovery['data']) ?? {};
const shopCurrency = requireString(readRecord(discoveryData['shop'])?.['currencyCode'], 'shop currencyCode');
const locationNodes = readArray(readRecord(discoveryData['locations'])?.['nodes']).map(readRecord);
const fallbackLocationId = (locationNodes.find((node) => node?.['isActive'] === true) ?? locationNodes.find(Boolean))?.[
  'id'
];

interface VariantPick {
  id: string;
  price: string;
  locationId: string;
}
let untrackedPick: VariantPick | undefined;
let stockedPick: VariantPick | undefined;
for (const rawProduct of readArray(readRecord(discoveryData['products'])?.['nodes'])) {
  for (const rawVariant of readArray(readRecord(readRecord(rawProduct)?.['variants'])?.['nodes'])) {
    const variant = readRecord(rawVariant);
    const id = variant?.['id'];
    const price = variant?.['price'];
    if (typeof id !== 'string' || typeof price !== 'string') continue;
    const numeric = Number.parseFloat(price);
    if (!Number.isFinite(numeric) || numeric <= 0) continue;
    const inventoryItem = readRecord(variant?.['inventoryItem']);
    if (inventoryItem?.['tracked'] === false) {
      if (!untrackedPick && typeof fallbackLocationId === 'string') {
        untrackedPick = { id, price, locationId: fallbackLocationId };
      }
      continue;
    }
    for (const rawLevel of readArray(readRecord(inventoryItem?.['inventoryLevels'])?.['nodes'])) {
      const level = readRecord(rawLevel);
      const available = readArray(level?.['quantities'])
        .map(readRecord)
        .find((quantity) => quantity?.['name'] === 'available')?.['quantity'];
      const levelLocation = readRecord(level?.['location'])?.['id'];
      if (typeof available === 'number' && available >= 2 && typeof levelLocation === 'string' && !stockedPick) {
        stockedPick = { id, price, locationId: levelLocation };
      }
    }
  }
}
// Prefer a stocked (variant, location) pair: `orderEditAddVariant` enforces location stocking even
// for variants whose inventory item reports untracked, so a real stocked level is the only reliably
// addable choice; the untracked pick is a last resort.
const variantPick = stockedPick ?? untrackedPick;
if (!variantPick) {
  throw new Error('No untracked or sufficiently-stocked priced variant found to drive the order-edit captures.');
}
const variantId = variantPick.id;
const locationId = variantPick.locationId;
console.log(
  JSON.stringify(
    { phase: 'discovered', shopCurrency, locationId, variantId, variantPrice: variantPick.price },
    null,
    2,
  ),
);

// ── Fixture 1: happy-path — two custom lines, add a real variant, commit ──────────────────────
const happyLineAmount = '12.00';
const happy = await createOrder(
  {
    email: `har-115-order-edit-${cap.stamp}@example.com`,
    note: 'HAR-115 order edit existing-order happy-path capture',
    tags: ['har-115', 'order-edit', 'deseed'],
    test: true,
    currency: shopCurrency,
    lineItems: [
      {
        title: `HAR-115 base line A ${cap.stamp}`,
        quantity: 1,
        priceSet: { shopMoney: { amount: happyLineAmount, currencyCode: shopCurrency } },
        requiresShipping: false,
        taxable: false,
      },
      {
        title: `HAR-115 base line B ${cap.stamp}`,
        quantity: 1,
        priceSet: { shopMoney: { amount: happyLineAmount, currencyCode: shopCurrency } },
        requiresShipping: false,
        taxable: false,
      },
    ],
  },
  'create happy-path order',
);
const happyOrderHydrate = orderHydrateCassette(
  happy.id,
  await cap.run(orderEditHydrateQuery, { id: happy.id }, 'happy order hydrate'),
);
const happyVariantHydrate = variantHydrateCassette(
  variantId,
  await cap.run(variantHydrateQuery, { id: variantId }, 'happy variant hydrate'),
);

const happyBegin = await cap.run(beginDocument, { id: happy.id }, 'happy begin');
const happyCalcId = requireString(
  readRecord(cap.mutationRoot(happyBegin, 'orderEditBegin', 'happy begin')['calculatedOrder'])?.['id'],
  'happy calculatedOrder id',
);
const happyAddVariant = await cap.run(
  addVariantPayloadDocument,
  { id: happyCalcId, variantId, quantity: 1, locationId, allowDuplicates: false },
  'happy add variant',
);
cap.mutationRoot(happyAddVariant, 'orderEditAddVariant', 'happy add variant');
const happyCommit = await cap.run(
  commitDocument,
  { id: happyCalcId, notifyCustomer: false, staffNote: 'HAR-115 order edit add capture' },
  'happy commit',
);
cap.mutationRoot(happyCommit, 'orderEditCommit', 'happy commit');

const happyPath = cap.fixturePath('orders', 'order-edit-existing-order-happy-path.json');
await cap.writeJson(happyPath, {
  scenarioId: 'order-edit-existing-order-happy-path',
  apiVersion: cap.apiVersion,
  storeDomain: cap.storeDomain,
  recordedAt: new Date().toISOString(),
  source: 'live-shopify-admin-graphql',
  variables: { id: happy.id },
  mutation: { response: happyBegin },
  addVariant: { variables: { variantId, quantity: 1, locationId, allowDuplicates: false }, response: happyAddVariant },
  commitAdd: {
    variables: { notifyCustomer: false, staffNote: 'HAR-115 order edit add capture' },
    response: happyCommit,
  },
  upstreamCalls: [happyOrderHydrate, happyVariantHydrate],
});
const happyCancel = await cancelOrder(happy.id);

// ── Fixture 2: validation — invalid variant id + duplicate-existing-variant (no commit) ───────
const validation = await createOrder(
  {
    email: `har-115-order-edit-validation-${cap.stamp}@example.com`,
    note: 'HAR-115 order edit existing-order validation capture',
    tags: ['har-115', 'order-edit', 'deseed', 'validation'],
    test: true,
    currency: shopCurrency,
    lineItems: [{ variantId, quantity: 1 }],
  },
  'create validation order',
);
const validationOrderHydrate = orderHydrateCassette(
  validation.id,
  await cap.run(orderEditHydrateQuery, { id: validation.id }, 'validation order hydrate'),
);
const validationBegin = await cap.run(beginDocument, { id: validation.id }, 'validation begin');
const validationCalcId = requireString(
  readRecord(cap.mutationRoot(validationBegin, 'orderEditBegin', 'validation begin')['calculatedOrder'])?.['id'],
  'validation calculatedOrder id',
);
// Invalid variant id (.../0) — tolerate userErrors (not a thrown top-level error).
const invalidVariant = (
  await cap.runGraphqlRequest(addVariantDocument, {
    id: validationCalcId,
    variantId: 'gid://shopify/ProductVariant/0',
    quantity: 1,
    locationId,
    allowDuplicates: false,
  })
).payload as JsonRecord;
// Duplicate existing variant with allowDuplicates:false — Shopify returns the existing calc line.
const duplicateVariant = await cap.run(
  addVariantDocument,
  { id: validationCalcId, variantId, quantity: 1, locationId, allowDuplicates: false },
  'validation duplicate variant',
);

const validationPath = cap.fixturePath('orders', 'order-edit-existing-order-validation.json');
await cap.writeJson(validationPath, {
  scenarioId: 'order-edit-existing-order-validation',
  apiVersion: cap.apiVersion,
  storeDomain: cap.storeDomain,
  recordedAt: new Date().toISOString(),
  source: 'live-shopify-admin-graphql',
  variables: { id: validation.id },
  mutation: { response: validationBegin },
  invalidVariant: { response: invalidVariant },
  duplicateVariant: {
    variables: { variantId, quantity: 1, locationId, allowDuplicates: false },
    response: duplicateVariant,
  },
  upstreamCalls: [validationOrderHydrate],
});
const validationCancel = await cancelOrder(validation.id);

console.log(
  JSON.stringify(
    {
      phase: 'done',
      happyPath,
      validationPath,
      shopCurrency,
      locationId,
      variantId,
      happyOrderId: happy.id,
      validationOrderId: validation.id,
      happyCancel,
      validationCancel,
      happyAddedUnitAmount: readRecord(
        readRecord(
          readRecord(
            readRecord(readRecord(happyAddVariant['data'])?.['orderEditAddVariant'])?.['calculatedLineItem'],
          )?.['originalUnitPriceSet'],
        )?.['shopMoney'],
      )?.['amount'],
      happyCommitLineCount: readArray(
        readRecord(
          readRecord(readRecord(readRecord(happyCommit['data'])?.['orderEditCommit'])?.['order'])?.['lineItems'],
        )?.['nodes'],
      ).length,
      invalidVariantUserErrors: readArray(
        readRecord(readRecord(readRecord(invalidVariant['data'])?.['orderEditAddVariant']))?.['userErrors'],
      ),
      duplicateVariantTitle: readRecord(
        readRecord(readRecord(duplicateVariant['data'])?.['orderEditAddVariant'])?.['calculatedLineItem'],
      )?.['title'],
    } satisfies JsonRecord,
    null,
    2,
  ),
);

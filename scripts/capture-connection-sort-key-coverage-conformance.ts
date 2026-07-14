/* oxlint-disable no-console -- CLI capture scripts intentionally write progress to stdio. */
import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import {
  createConformanceCapture,
  readArray,
  readRecord,
  requireString,
  type JsonRecord,
} from './conformance-capture-lib.js';
import type { ConformanceGraphqlResult } from './conformance-graphql-client.js';
import {
  captureDraftProxyShopPricingHydrate,
  captureRuntimeHydrationCall,
  STORE_PROPERTIES_LOCATION_HYDRATE_QUERY,
} from './support/shopify/runtime-hydration-capture.js';

type CaptureStep = {
  query: string;
  variables: JsonRecord;
  response: ConformanceGraphqlResult<JsonRecord>;
};

const scenarioId = 'connection-sort-key-coverage';

const productSetDocumentPath = 'config/parity-requests/products/products-sort-key-inventory-total-set.graphql';
const productReadDocumentPath = 'config/parity-requests/products/products-sort-key-inventory-total-read.graphql';
const productSpecPath = 'config/parity-specs/products/products-sort-key-inventory-total-staged.json';
const orderDraftCreateDocumentPath = 'config/parity-requests/orders/orders-sort-key-total-price-draft-create.graphql';
const orderDraftCompleteDocumentPath =
  'config/parity-requests/orders/orders-sort-key-total-price-draft-complete.graphql';
const orderReadDocumentPath = 'config/parity-requests/orders/orders-sort-key-total-price-read.graphql';
const orderSpecPath = 'config/parity-specs/orders/orders-sort-key-total-price-staged.json';
const segmentCreateDocumentPath = 'config/parity-requests/segments/segments-sort-key-create.graphql';
const segmentUpdateDocumentPath = 'config/parity-requests/segments/segments-sort-key-update.graphql';
const segmentReadDocumentPath = 'config/parity-requests/segments/segments-sort-key-read.graphql';
const segmentSpecPath = 'config/parity-specs/segments/segments-sort-key-staged.json';

const productSetDocument = `#graphql
  mutation ProductsSortKeyInventoryTotalSet($input: ProductSetInput!, $synchronous: Boolean!) {
    productSet(input: $input, synchronous: $synchronous) {
      product {
        id
        title
        tags
        totalInventory
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const productReadDocument = `#graphql
  query ProductsSortKeyInventoryTotalRead($query: String!) {
    inventoryTotalOrder: products(first: 10, query: $query, sortKey: INVENTORY_TOTAL) {
      nodes {
        title
        totalInventory
      }
    }
    reverseWindow: products(first: 1, query: $query, sortKey: INVENTORY_TOTAL, reverse: true) {
      nodes {
        title
        totalInventory
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
      }
    }
  }
`;

const productDeleteDocument = `#graphql
  mutation ProductsSortKeyCleanupDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

const locationDocument = `#graphql
  query ConnectionSortKeyCoverageLocations {
    shop {
      currencyCode
    }
    locations(first: 1) {
      nodes {
        id
        name
        isActive
      }
    }
  }
`;

const orderDraftCreateDocument = `#graphql
  mutation OrdersSortKeyTotalPriceDraftCreate($input: DraftOrderInput!) {
    draftOrderCreate(input: $input) {
      draftOrder {
        id
        email
        tags
        totalPriceSet {
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
`;

const orderDraftCompleteDocument = `#graphql
  mutation OrdersSortKeyTotalPriceDraftComplete($id: ID!, $paymentPending: Boolean!) {
    draftOrderComplete(id: $id, paymentPending: $paymentPending) {
      draftOrder {
        id
        email
        tags
        status
        order {
          id
          email
          tags
          totalPriceSet {
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
`;

const orderReadDocument = `#graphql
  query OrdersSortKeyTotalPriceRead($query: String!) {
    totalPriceOrder: orders(first: 10, query: $query, sortKey: TOTAL_PRICE) {
      nodes {
        email
        totalPriceSet {
          shopMoney {
            amount
            currencyCode
          }
        }
      }
    }
    reverseWindow: orders(first: 1, query: $query, sortKey: TOTAL_PRICE, reverse: true) {
      nodes {
        email
        totalPriceSet {
          shopMoney {
            amount
            currencyCode
          }
        }
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
      }
    }
  }
`;

const orderCancelDocument = `#graphql
  mutation OrdersSortKeyCleanupCancel(
    $orderId: ID!
    $reason: OrderCancelReason!
    $notifyCustomer: Boolean!
    $restock: Boolean!
  ) {
    orderCancel(orderId: $orderId, reason: $reason, notifyCustomer: $notifyCustomer, restock: $restock) {
      job {
        id
        done
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const segmentCreateDocument = `#graphql
  mutation SegmentsSortKeyCreate($name: String!, $query: String!) {
    segmentCreate(name: $name, query: $query) {
      segment {
        id
        name
        query
        creationDate
        lastEditDate
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const segmentUpdateDocument = `#graphql
  mutation SegmentsSortKeyUpdate($id: ID!, $name: String!, $query: String!) {
    segmentUpdate(id: $id, name: $name, query: $query) {
      segment {
        id
        name
        query
        creationDate
        lastEditDate
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const segmentReadDocument = `#graphql
  query SegmentsSortKeyRead($first: Int!) {
    byLastEditReverse: segments(first: $first, sortKey: LAST_EDIT_DATE, reverse: true) {
      nodes {
        name
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
      }
    }
    byCreationDateReverse: segments(first: $first, sortKey: CREATION_DATE, reverse: true) {
      nodes {
        name
      }
    }
    byIdReverse: segments(first: $first, sortKey: ID, reverse: true) {
      nodes {
        name
      }
    }
  }
`;

const segmentDeleteDocument = `#graphql
  mutation SegmentsSortKeyCleanupDelete($id: ID!) {
    segmentDelete(id: $id) {
      deletedSegmentId
      userErrors {
        field
        message
      }
    }
  }
`;

function trimGraphql(document: string): string {
  return document.replace(/^#graphql\n/u, '').trim();
}

async function writeText(filePath: string, payload: string): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${payload.trim()}\n`, 'utf8');
}

async function sleep(milliseconds: number): Promise<void> {
  await new Promise((resolve) => {
    setTimeout(resolve, milliseconds);
  });
}

function data(step: CaptureStep): JsonRecord {
  const value = readRecord(step.response.payload['data']);
  if (!value) {
    throw new Error(`Capture step is missing data: ${JSON.stringify(step.response.payload, null, 2)}`);
  }
  return value;
}

function nodeFieldValues(step: CaptureStep, connectionName: string, fieldName: string): string[] {
  const connection = readRecord(data(step)[connectionName]);
  return readArray(connection?.['nodes']).map((node) => requireString(readRecord(node)?.[fieldName], fieldName));
}

function assertValues(label: string, actual: string[], expected: string[]): void {
  if (JSON.stringify(actual) !== JSON.stringify(expected)) {
    throw new Error(`${label}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`);
  }
}

function assertNoTopLevelErrors(step: CaptureStep, label: string): void {
  if (step.response.status < 200 || step.response.status >= 300 || step.response.payload['errors']) {
    throw new Error(`${label} failed: ${JSON.stringify(step.response, null, 2)}`);
  }
}

async function main(): Promise<void> {
  const capture = await createConformanceCapture();
  const fixturePath = capture.fixturePath('admin-platform', 'connection-sort-key-coverage.json');

  async function record(document: string, variables: JsonRecord, label: string): Promise<CaptureStep> {
    const query = trimGraphql(document);
    const response = await capture.runGraphqlRequest<JsonRecord>(query, variables);
    const step = { query, variables, response };
    assertNoTopLevelErrors(step, label);
    return step;
  }

  async function recordUntil(
    document: string,
    variables: JsonRecord,
    label: string,
    predicate: (step: CaptureStep) => boolean,
  ): Promise<CaptureStep> {
    let latest: CaptureStep | null = null;
    for (let attempt = 1; attempt <= 15; attempt += 1) {
      latest = await record(document, variables, `${label} attempt ${attempt}`);
      if (predicate(latest)) return latest;
      await sleep(2_000);
    }
    throw new Error(`${label} did not reach expected state: ${JSON.stringify(latest?.response.payload, null, 2)}`);
  }

  async function cleanupProduct(id: string): Promise<CaptureStep | null> {
    try {
      return await record(productDeleteDocument, { input: { id } }, `cleanup product ${id}`);
    } catch (error) {
      console.error(`Failed to clean up product ${id}:`, error);
      return null;
    }
  }

  async function cleanupOrder(orderId: string): Promise<CaptureStep | null> {
    try {
      return await record(
        orderCancelDocument,
        { orderId, reason: 'OTHER', notifyCustomer: false, restock: true },
        `cleanup order ${orderId}`,
      );
    } catch (error) {
      console.error(`Failed to clean up order ${orderId}:`, error);
      return null;
    }
  }

  async function cleanupSegment(id: string): Promise<CaptureStep | null> {
    try {
      return await record(segmentDeleteDocument, { id }, `cleanup segment ${id}`);
    } catch (error) {
      console.error(`Failed to clean up segment ${id}:`, error);
      return null;
    }
  }

  const productIds: string[] = [];
  const orderIds: string[] = [];
  const segmentIds: string[] = [];
  const cleanup: JsonRecord = { products: [], orders: [], segments: [] };

  try {
    const tag = `connection-sort-key-${capture.stamp}`;
    const locationProbe = await record(locationDocument, {}, 'location probe');
    const locationId = requireString(
      readRecord(readArray(readRecord(data(locationProbe)['locations'])?.['nodes'])[0])?.['id'],
      'locations.nodes[0].id',
    );
    const shopCurrencyCode = requireString(
      readRecord(data(locationProbe)['shop'])?.['currencyCode'],
      'shop.currencyCode',
    );
    const locationHydrate = await captureRuntimeHydrationCall({
      operationName: 'StorePropertiesLocationHydrate',
      query: STORE_PROPERTIES_LOCATION_HYDRATE_QUERY,
      variables: { id: locationId },
      runGraphqlRequest: (query, variables) => capture.runGraphqlRequest(query, variables),
    });
    const shopPricingHydrate = await captureDraftProxyShopPricingHydrate((query, variables) =>
      capture.runGraphqlRequest(query, variables),
    );

    function productSetVariables(name: string, inventoryQuantity: number): JsonRecord {
      return {
        synchronous: true,
        input: {
          title: `${name} ${capture.stamp}`,
          status: 'DRAFT',
          vendor: 'Connection Sort Probe',
          productType: 'SortKey Probe',
          tags: [tag],
          productOptions: [{ name: 'Title', position: 1, values: [{ name: 'Default Title' }] }],
          variants: [
            {
              optionValues: [{ optionName: 'Title', name: 'Default Title' }],
              sku: `${name.toUpperCase().replace(/[^A-Z0-9]+/gu, '-')}-${capture.stamp}`,
              inventoryItem: { tracked: true, requiresShipping: false },
              inventoryQuantities: [{ locationId, name: 'available', quantity: inventoryQuantity }],
            },
          ],
        },
      };
    }

    const productAlpha = await record(
      productSetDocument,
      productSetVariables('Alpha Inventory Sort', 9),
      'alpha productSet',
    );
    const productMiddle = await record(
      productSetDocument,
      productSetVariables('Middle Inventory Sort', 4),
      'middle productSet',
    );
    const productZulu = await record(
      productSetDocument,
      productSetVariables('Zulu Inventory Sort', 1),
      'zulu productSet',
    );
    for (const step of [productAlpha, productMiddle, productZulu]) {
      capture.mutationRoot(step.response.payload, 'productSet', 'productSet');
      productIds.push(
        requireString(readRecord(readRecord(data(step)['productSet'])?.['product'])?.['id'], 'product.id'),
      );
    }

    const productReadVariables = { query: `tag:${tag}` };
    const productRead = await recordUntil(
      productReadDocument,
      productReadVariables,
      'products inventory sort read',
      (step) => {
        const titles = nodeFieldValues(step, 'inventoryTotalOrder', 'title');
        return titles.length === 3;
      },
    );
    assertValues('products INVENTORY_TOTAL order', nodeFieldValues(productRead, 'inventoryTotalOrder', 'title'), [
      `Zulu Inventory Sort ${capture.stamp}`,
      `Middle Inventory Sort ${capture.stamp}`,
      `Alpha Inventory Sort ${capture.stamp}`,
    ]);
    assertValues('products INVENTORY_TOTAL reverse window', nodeFieldValues(productRead, 'reverseWindow', 'title'), [
      `Alpha Inventory Sort ${capture.stamp}`,
    ]);

    function orderDraftCreateVariables(label: string, amount: string): JsonRecord {
      const email = `${label}-${capture.stamp}@example.com`;
      return {
        input: {
          email,
          tags: [tag],
          lineItems: [
            {
              title: `${label} total price sort`,
              quantity: 1,
              originalUnitPriceWithCurrency: { amount, currencyCode: shopCurrencyCode },
              sku: `${label}-${capture.stamp}`,
            },
          ],
        },
      };
    }

    async function completeDraft(step: CaptureStep, label: string): Promise<CaptureStep> {
      const draftOrder = readRecord(readRecord(data(step)['draftOrderCreate'])?.['draftOrder']);
      const id = requireString(draftOrder?.['id'], 'draftOrder.id');
      return await record(orderDraftCompleteDocument, { id, paymentPending: false }, `${label} draftOrderComplete`);
    }

    const orderExpensiveDraft = await record(
      orderDraftCreateDocument,
      orderDraftCreateVariables('expensive-sort', '30.00'),
      'expensive draftOrderCreate',
    );
    capture.mutationRoot(orderExpensiveDraft.response.payload, 'draftOrderCreate', 'draftOrderCreate');
    const orderExpensive = await completeDraft(orderExpensiveDraft, 'expensive');
    const orderCheapDraft = await record(
      orderDraftCreateDocument,
      orderDraftCreateVariables('cheap-sort', '10.00'),
      'cheap draftOrderCreate',
    );
    capture.mutationRoot(orderCheapDraft.response.payload, 'draftOrderCreate', 'draftOrderCreate');
    const orderCheap = await completeDraft(orderCheapDraft, 'cheap');
    const orderMiddleDraft = await record(
      orderDraftCreateDocument,
      orderDraftCreateVariables('middle-sort', '20.00'),
      'middle draftOrderCreate',
    );
    capture.mutationRoot(orderMiddleDraft.response.payload, 'draftOrderCreate', 'draftOrderCreate');
    const orderMiddle = await completeDraft(orderMiddleDraft, 'middle');
    for (const step of [orderExpensive, orderCheap, orderMiddle]) {
      capture.mutationRoot(step.response.payload, 'draftOrderComplete', 'draftOrderComplete');
      orderIds.push(
        requireString(
          readRecord(readRecord(readRecord(data(step)['draftOrderComplete'])?.['draftOrder'])?.['order'])?.['id'],
          'draftOrder.order.id',
        ),
      );
    }

    const orderReadVariables = { query: `tag:${tag}` };
    const orderRead = await recordUntil(
      orderReadDocument,
      orderReadVariables,
      'orders total price sort read',
      (step) => {
        const emails = nodeFieldValues(step, 'totalPriceOrder', 'email');
        return emails.length === 3;
      },
    );
    assertValues('orders TOTAL_PRICE order', nodeFieldValues(orderRead, 'totalPriceOrder', 'email'), [
      `cheap-sort-${capture.stamp}@example.com`,
      `middle-sort-${capture.stamp}@example.com`,
      `expensive-sort-${capture.stamp}@example.com`,
    ]);
    assertValues('orders TOTAL_PRICE reverse window', nodeFieldValues(orderRead, 'reverseWindow', 'email'), [
      `expensive-sort-${capture.stamp}@example.com`,
    ]);

    const segmentAlpha = await record(
      segmentCreateDocument,
      { name: `Alpha Segment Sort ${capture.stamp}`, query: 'number_of_orders >= 1' },
      'alpha segmentCreate',
    );
    const segmentBeta = await record(
      segmentCreateDocument,
      { name: `Beta Segment Sort ${capture.stamp}`, query: 'number_of_orders >= 2' },
      'beta segmentCreate',
    );
    const segmentGamma = await record(
      segmentCreateDocument,
      { name: `Gamma Segment Sort ${capture.stamp}`, query: 'number_of_orders >= 3' },
      'gamma segmentCreate',
    );
    for (const step of [segmentAlpha, segmentBeta, segmentGamma]) {
      capture.mutationRoot(step.response.payload, 'segmentCreate', 'segmentCreate');
      segmentIds.push(
        requireString(readRecord(readRecord(data(step)['segmentCreate'])?.['segment'])?.['id'], 'segment.id'),
      );
    }

    await sleep(1_100);
    const segmentUpdate = await record(
      segmentUpdateDocument,
      {
        id: segmentIds[0],
        name: `Alpha Segment Sort Updated ${capture.stamp}`,
        query: 'number_of_orders >= 4',
      },
      'alpha segmentUpdate',
    );
    capture.mutationRoot(segmentUpdate.response.payload, 'segmentUpdate', 'segmentUpdate');

    const segmentReadVariables = { first: 3 };
    const segmentRead = await recordUntil(segmentReadDocument, segmentReadVariables, 'segments sort read', (step) => {
      const names = nodeFieldValues(step, 'byLastEditReverse', 'name');
      return (
        JSON.stringify(names) ===
        JSON.stringify([
          `Alpha Segment Sort Updated ${capture.stamp}`,
          `Gamma Segment Sort ${capture.stamp}`,
          `Beta Segment Sort ${capture.stamp}`,
        ])
      );
    });
    assertValues('segments LAST_EDIT_DATE reverse order', nodeFieldValues(segmentRead, 'byLastEditReverse', 'name'), [
      `Alpha Segment Sort Updated ${capture.stamp}`,
      `Gamma Segment Sort ${capture.stamp}`,
      `Beta Segment Sort ${capture.stamp}`,
    ]);
    assertValues(
      'segments CREATION_DATE reverse order',
      nodeFieldValues(segmentRead, 'byCreationDateReverse', 'name'),
      [
        `Gamma Segment Sort ${capture.stamp}`,
        `Beta Segment Sort ${capture.stamp}`,
        `Alpha Segment Sort Updated ${capture.stamp}`,
      ],
    );
    assertValues('segments ID reverse order', nodeFieldValues(segmentRead, 'byIdReverse', 'name'), [
      `Gamma Segment Sort ${capture.stamp}`,
      `Beta Segment Sort ${capture.stamp}`,
      `Alpha Segment Sort Updated ${capture.stamp}`,
    ]);

    for (const productId of productIds) {
      const result = await cleanupProduct(productId);
      if (result) readArray(cleanup['products']).push(result);
    }
    for (const orderId of orderIds) {
      const result = await cleanupOrder(orderId);
      if (result) readArray(cleanup['orders']).push(result);
    }
    for (const segmentId of segmentIds) {
      const result = await cleanupSegment(segmentId);
      if (result) readArray(cleanup['segments']).push(result);
    }

    await capture.writeJson(fixturePath, {
      scenarioId,
      capturedAt: new Date().toISOString(),
      storeDomain: capture.storeDomain,
      apiVersion: capture.apiVersion,
      notes:
        'Live Shopify Admin GraphQL capture for staged connection sort keys. Setup uses public productSet, draftOrderCreate, draftOrderComplete, segmentCreate, and segmentUpdate requests with unique tags/names; sorted reads are captured after Shopify search/catalog visibility is observed.',
      operations: {
        products: {
          locationProbe,
          alphaSet: productAlpha,
          middleSet: productMiddle,
          zuluSet: productZulu,
          read: productRead,
        },
        orders: {
          expensiveDraftCreate: orderExpensiveDraft,
          expensiveComplete: orderExpensive,
          cheapDraftCreate: orderCheapDraft,
          cheapComplete: orderCheap,
          middleDraftCreate: orderMiddleDraft,
          middleComplete: orderMiddle,
          read: orderRead,
        },
        segments: {
          alphaCreate: segmentAlpha,
          betaCreate: segmentBeta,
          gammaCreate: segmentGamma,
          alphaUpdate: segmentUpdate,
          read: segmentRead,
        },
        cleanup,
      },
      upstreamCalls: [locationHydrate, shopPricingHydrate],
    });

    await writeText(productSetDocumentPath, trimGraphql(productSetDocument));
    await writeText(productReadDocumentPath, trimGraphql(productReadDocument));
    await writeText(orderDraftCreateDocumentPath, trimGraphql(orderDraftCreateDocument));
    await writeText(orderDraftCompleteDocumentPath, trimGraphql(orderDraftCompleteDocument));
    await writeText(orderReadDocumentPath, trimGraphql(orderReadDocument));
    await writeText(segmentCreateDocumentPath, trimGraphql(segmentCreateDocument));
    await writeText(segmentUpdateDocumentPath, trimGraphql(segmentUpdateDocument));
    await writeText(segmentReadDocumentPath, trimGraphql(segmentReadDocument));

    await capture.writeJson(productSpecPath, productSpec(fixturePath, capture.apiVersion));
    await capture.writeJson(orderSpecPath, orderSpec(fixturePath, capture.apiVersion));
    await capture.writeJson(segmentSpecPath, segmentSpec(fixturePath, capture.apiVersion));

    console.log(JSON.stringify({ ok: true, scenarioId, fixturePath }, null, 2));
  } catch (error) {
    for (const productId of productIds) await cleanupProduct(productId);
    for (const orderId of orderIds) await cleanupOrder(orderId);
    for (const segmentId of segmentIds) await cleanupSegment(segmentId);
    throw error;
  }
}

function productSpec(fixturePath: string, apiVersion: string): JsonRecord {
  const createSelectedPaths = [
    '$.productSet.product.title',
    '$.productSet.product.totalInventory',
    '$.productSet.userErrors',
  ];
  return {
    scenarioId: 'products-sort-key-inventory-total-staged',
    operationNames: ['productSet', 'products'],
    scenarioStatus: 'captured',
    assertionKinds: ['sort-order-semantics', 'runtime-staging', 'pagination-shape'],
    liveCaptureFiles: [fixturePath],
    runtimeTestFiles: ['tests/graphql_routes/products_saved_searches.rs'],
    proxyRequest: {
      documentPath: productSetDocumentPath,
      variablesCapturePath: '$.operations.products.alphaSet.variables',
      apiVersion,
    },
    comparisonMode: 'captured-vs-proxy-request',
    notes:
      'Live Shopify evidence for local staged products(sortKey: INVENTORY_TOTAL) ordering and reverse windowing. Replay stages three products through public productSet requests with distinct tracked inventory quantities, then reads the connection through the public products root.',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        {
          name: 'alpha-product-set-setup',
          capturePath: '$.operations.products.alphaSet.response.payload.data',
          proxyPath: '$.data',
          selectedPaths: createSelectedPaths,
        },
        {
          name: 'middle-product-set-setup',
          capturePath: '$.operations.products.middleSet.response.payload.data',
          proxyPath: '$.data',
          proxyRequest: {
            documentPath: productSetDocumentPath,
            variablesCapturePath: '$.operations.products.middleSet.variables',
            apiVersion,
          },
          selectedPaths: createSelectedPaths,
        },
        {
          name: 'zulu-product-set-setup',
          capturePath: '$.operations.products.zuluSet.response.payload.data',
          proxyPath: '$.data',
          proxyRequest: {
            documentPath: productSetDocumentPath,
            variablesCapturePath: '$.operations.products.zuluSet.variables',
            apiVersion,
          },
          selectedPaths: createSelectedPaths,
        },
        {
          name: 'products-inventory-total-order-and-reverse-window',
          capturePath: '$.operations.products.read.response.payload.data',
          proxyPath: '$.data',
          proxyRequest: {
            documentPath: productReadDocumentPath,
            variablesCapturePath: '$.operations.products.read.variables',
            apiVersion,
          },
        },
      ],
    },
  };
}

function orderSpec(fixturePath: string, apiVersion: string): JsonRecord {
  const createSelectedPaths = [
    '$.draftOrderCreate.draftOrder.email',
    '$.draftOrderCreate.draftOrder.totalPriceSet',
    '$.draftOrderCreate.userErrors',
  ];
  const completeSelectedPaths = [
    '$.draftOrderComplete.draftOrder.order.email',
    '$.draftOrderComplete.draftOrder.order.totalPriceSet',
    '$.draftOrderComplete.userErrors',
  ];
  return {
    scenarioId: 'orders-sort-key-total-price-staged',
    operationNames: ['draftOrderCreate', 'draftOrderComplete', 'orders'],
    scenarioStatus: 'captured',
    assertionKinds: ['sort-order-semantics', 'runtime-staging', 'pagination-shape'],
    liveCaptureFiles: [fixturePath],
    runtimeTestFiles: ['tests/graphql_routes/orders.rs'],
    proxyRequest: {
      documentPath: orderDraftCreateDocumentPath,
      variablesCapturePath: '$.operations.orders.expensiveDraftCreate.variables',
      apiVersion,
    },
    comparisonMode: 'captured-vs-proxy-request',
    notes:
      'Live Shopify evidence for local staged orders(sortKey: TOTAL_PRICE) ordering and reverse windowing. Replay stages three orders through public draftOrderCreate and draftOrderComplete requests with distinct total prices, then reads the connection through the public orders root.',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        {
          name: 'expensive-draft-create-setup',
          capturePath: '$.operations.orders.expensiveDraftCreate.response.payload.data',
          proxyPath: '$.data',
          selectedPaths: createSelectedPaths,
        },
        {
          name: 'expensive-draft-complete-setup',
          capturePath: '$.operations.orders.expensiveComplete.response.payload.data',
          proxyPath: '$.data',
          proxyRequest: {
            documentPath: orderDraftCompleteDocumentPath,
            variables: {
              id: { fromPrimaryProxyPath: '$.data.draftOrderCreate.draftOrder.id' },
              paymentPending: { fromCapturePath: '$.operations.orders.expensiveComplete.variables.paymentPending' },
            },
            apiVersion,
          },
          selectedPaths: completeSelectedPaths,
        },
        {
          name: 'cheap-draft-create-setup',
          capturePath: '$.operations.orders.cheapDraftCreate.response.payload.data',
          proxyPath: '$.data',
          proxyRequest: {
            documentPath: orderDraftCreateDocumentPath,
            variablesCapturePath: '$.operations.orders.cheapDraftCreate.variables',
            apiVersion,
          },
          selectedPaths: createSelectedPaths,
        },
        {
          name: 'cheap-draft-complete-setup',
          capturePath: '$.operations.orders.cheapComplete.response.payload.data',
          proxyPath: '$.data',
          proxyRequest: {
            documentPath: orderDraftCompleteDocumentPath,
            variables: {
              id: { fromProxyResponse: 'cheap-draft-create-setup', path: '$.data.draftOrderCreate.draftOrder.id' },
              paymentPending: { fromCapturePath: '$.operations.orders.cheapComplete.variables.paymentPending' },
            },
            apiVersion,
          },
          selectedPaths: completeSelectedPaths,
        },
        {
          name: 'middle-draft-create-setup',
          capturePath: '$.operations.orders.middleDraftCreate.response.payload.data',
          proxyPath: '$.data',
          proxyRequest: {
            documentPath: orderDraftCreateDocumentPath,
            variablesCapturePath: '$.operations.orders.middleDraftCreate.variables',
            apiVersion,
          },
          selectedPaths: createSelectedPaths,
        },
        {
          name: 'middle-draft-complete-setup',
          capturePath: '$.operations.orders.middleComplete.response.payload.data',
          proxyPath: '$.data',
          proxyRequest: {
            documentPath: orderDraftCompleteDocumentPath,
            variables: {
              id: { fromProxyResponse: 'middle-draft-create-setup', path: '$.data.draftOrderCreate.draftOrder.id' },
              paymentPending: { fromCapturePath: '$.operations.orders.middleComplete.variables.paymentPending' },
            },
            apiVersion,
          },
          selectedPaths: completeSelectedPaths,
        },
        {
          name: 'orders-total-price-order-and-reverse-window',
          capturePath: '$.operations.orders.read.response.payload.data',
          proxyPath: '$.data',
          proxyRequest: {
            documentPath: orderReadDocumentPath,
            variablesCapturePath: '$.operations.orders.read.variables',
            apiVersion,
          },
        },
      ],
    },
  };
}

function segmentSpec(fixturePath: string, apiVersion: string): JsonRecord {
  const createSelectedPaths = [
    '$.segmentCreate.segment.name',
    '$.segmentCreate.segment.query',
    '$.segmentCreate.userErrors',
  ];
  return {
    scenarioId: 'segments-sort-key-staged',
    operationNames: ['segmentCreate', 'segmentUpdate', 'segments'],
    scenarioStatus: 'captured',
    assertionKinds: ['sort-order-semantics', 'runtime-staging', 'pagination-shape'],
    liveCaptureFiles: [fixturePath],
    runtimeTestFiles: ['tests/graphql_routes/products_saved_searches.rs'],
    proxyRequest: {
      documentPath: segmentCreateDocumentPath,
      variablesCapturePath: '$.operations.segments.alphaCreate.variables',
      apiVersion,
    },
    comparisonMode: 'captured-vs-proxy-request',
    notes:
      'Live Shopify evidence for local staged segments(sortKey:) ordering. Replay stages three segments through public segmentCreate requests, updates one segment to move lastEditDate, then reads LAST_EDIT_DATE, CREATION_DATE, and ID ordering through the public segments root.',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        {
          name: 'alpha-segment-create-setup',
          capturePath: '$.operations.segments.alphaCreate.response.payload.data',
          proxyPath: '$.data',
          selectedPaths: createSelectedPaths,
        },
        {
          name: 'beta-segment-create-setup',
          capturePath: '$.operations.segments.betaCreate.response.payload.data',
          proxyPath: '$.data',
          proxyRequest: {
            documentPath: segmentCreateDocumentPath,
            variablesCapturePath: '$.operations.segments.betaCreate.variables',
            apiVersion,
          },
          selectedPaths: createSelectedPaths,
        },
        {
          name: 'gamma-segment-create-setup',
          capturePath: '$.operations.segments.gammaCreate.response.payload.data',
          proxyPath: '$.data',
          proxyRequest: {
            documentPath: segmentCreateDocumentPath,
            variablesCapturePath: '$.operations.segments.gammaCreate.variables',
            apiVersion,
          },
          selectedPaths: createSelectedPaths,
        },
        {
          name: 'alpha-segment-update-last-edit',
          capturePath: '$.operations.segments.alphaUpdate.response.payload.data',
          proxyPath: '$.data',
          proxyRequest: {
            documentPath: segmentUpdateDocumentPath,
            variables: {
              id: { fromPrimaryProxyPath: '$.data.segmentCreate.segment.id' },
              name: { fromCapturePath: '$.operations.segments.alphaUpdate.variables.name' },
              query: { fromCapturePath: '$.operations.segments.alphaUpdate.variables.query' },
            },
            apiVersion,
          },
          selectedPaths: [
            '$.segmentUpdate.segment.name',
            '$.segmentUpdate.segment.query',
            '$.segmentUpdate.userErrors',
          ],
        },
        {
          name: 'segments-sort-key-orderings',
          capturePath: '$.operations.segments.read.response.payload.data',
          proxyPath: '$.data',
          proxyRequest: {
            documentPath: segmentReadDocumentPath,
            variablesCapturePath: '$.operations.segments.read.variables',
            apiVersion,
          },
        },
      ],
    },
  };
}

await main();

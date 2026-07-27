/* oxlint-disable no-console -- CLI capture scripts intentionally report progress. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlPayload } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;
type GraphqlVariables = Record<string, unknown>;

type CapturedOperation = {
  query: string;
  variables: GraphqlVariables;
  response: ConformanceGraphqlPayload<unknown>;
};

type UpstreamCall = {
  operationName: string;
  variables: GraphqlVariables;
  query: string;
  response: {
    status: number;
    body: ConformanceGraphqlPayload<unknown>;
  };
};

const scenarioId = 'inventory-node-read-after-write';
const requestDir = path.join('config', 'parity-requests', 'products');
const requestPaths = {
  locationAdd: path.join(requestDir, 'inventory-connection-location-add.graphql'),
  productCreate: path.join(requestDir, 'inventory-node-product-create.graphql'),
  track: path.join(requestDir, 'inventory-node-track.graphql'),
  activate: path.join(requestDir, 'inventory-node-activate.graphql'),
  inventorySet: path.join(requestDir, 'inventory-node-set.graphql'),
  dedicatedRead: path.join(requestDir, 'inventory-node-dedicated-read.graphql'),
  coreRead: path.join(requestDir, 'inventory-node-core-read.graphql'),
  transferCreateReady: path.join(requestDir, 'inventory-node-transfer-create-ready.graphql'),
  transferRead: path.join(requestDir, 'inventory-node-transfer-read.graphql'),
  transferCreateDraft: path.join(requestDir, 'inventory-node-transfer-create-draft.graphql'),
  transferDelete: path.join(requestDir, 'inventory-node-transfer-delete.graphql'),
  shipmentCreate: path.join(requestDir, 'inventory-node-shipment-create.graphql'),
  shipmentSetTracking: path.join(requestDir, 'inventory-node-shipment-set-tracking.graphql'),
  shipmentRead: path.join(requestDir, 'inventory-node-shipment-read.graphql'),
  shipmentDetail: path.join(requestDir, 'inventory-shipment-detail.graphql'),
  shipmentHydrate: path.join(requestDir, 'inventory-shipment-mutation-hydrate.graphql'),
  transferHydrate: path.join(requestDir, 'inventory-transfer-mutation-hydrate.graphql'),
  referenceHydrate: path.join(requestDir, 'inventory-transfer-reference-hydrate.graphql'),
  shipmentDelete: path.join(requestDir, 'inventory-node-shipment-delete.graphql'),
} as const;
type RequestKey = keyof typeof requestPaths;

const locationDeactivateMutation = `#graphql
  mutation InventoryNodeLocationDeactivate($locationId: ID!, $destinationLocationId: ID) {
    locationDeactivate(locationId: $locationId, destinationLocationId: $destinationLocationId) {
      location {
        id
        isActive
      }
      locationDeactivateUserErrors {
        field
        message
        code
      }
    }
  }
`;

const locationDeleteMutation = `#graphql
  mutation InventoryNodeLocationDelete($locationId: ID!) {
    locationDelete(locationId: $locationId) {
      deletedLocationId
      locationDeleteUserErrors {
        field
        message
        code
      }
    }
  }
`;

const inventoryTransferCancelMutation = `#graphql
  mutation InventoryNodeCleanupTransferCancel($id: ID!) {
    inventoryTransferCancel(id: $id) {
      inventoryTransfer {
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
`;

const activeLocationsQuery = `#graphql
  query InventoryNodeCleanupLocations {
    locations(first: 20, includeInactive: false) {
      nodes {
        id
        name
        isActive
      }
    }
  }
`;

const productDeleteMutation = `#graphql
  mutation InventoryNodeCleanupProductDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2025-01',
  exitOnMissing: true,
});
if (apiVersion !== '2025-01') {
  throw new Error(`inventory Node capture requires SHOPIFY_CONFORMANCE_API_VERSION=2025-01, got ${apiVersion}`);
}

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const outputPath = path.join(outputDir, 'inventory-node-read-after-write.json');
const mutationFirstOutputPath = path.join(outputDir, 'inventory-shipment-mutation-first-hydration.json');

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const documents = Object.fromEntries(
  await Promise.all(
    Object.entries(requestPaths).map(async ([key, requestPath]) => [key, await readFile(requestPath, 'utf8')]),
  ),
) as Record<RequestKey, string>;

function asRecord(value: unknown, label: string): JsonRecord {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    throw new Error(`${label} was not an object: ${JSON.stringify(value)}`);
  }
  return value as JsonRecord;
}

function asArray(value: unknown, label: string): unknown[] {
  if (!Array.isArray(value)) {
    throw new Error(`${label} was not an array: ${JSON.stringify(value)}`);
  }
  return value;
}

function valueAt(value: unknown, pathSegments: Array<string | number>, label: string): unknown {
  let current = value;
  for (const segment of pathSegments) {
    if (Array.isArray(current)) {
      const index = typeof segment === 'number' ? segment : Number.parseInt(segment, 10);
      current = current[index];
      continue;
    }
    current = asRecord(current, label)[String(segment)];
  }
  return current;
}

function stringAt(value: unknown, pathSegments: Array<string | number>, label: string): string {
  const candidate = valueAt(value, pathSegments, label);
  if (typeof candidate !== 'string' || candidate.length === 0) {
    throw new Error(`${label} missing string at ${pathSegments.join('.')}: ${JSON.stringify(value)}`);
  }
  return candidate;
}

function userErrorsAt(value: unknown, pathSegments: Array<string | number>): unknown[] {
  const candidate = valueAt(value, pathSegments, 'GraphQL payload');
  return Array.isArray(candidate) ? candidate : [];
}

function expectNoUserErrors(value: unknown, pathSegments: Array<string | number>, label: string): void {
  const errors = userErrorsAt(value, pathSegments);
  if (errors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(errors, null, 2)}`);
  }
}

async function runOperation(key: RequestKey, variables: GraphqlVariables, label: string): Promise<CapturedOperation> {
  const query = documents[key];
  const result = await runGraphqlRequest(query, variables);
  if (result.status < 200 || result.status >= 300) {
    throw new Error(`${label} returned HTTP ${result.status}: ${JSON.stringify(result.payload, null, 2)}`);
  }
  const payload = result.payload as JsonRecord;
  if (Array.isArray(payload['errors']) && payload['errors'].length > 0) {
    throw new Error(`${label} returned GraphQL errors: ${JSON.stringify(result.payload, null, 2)}`);
  }
  return {
    query,
    variables,
    response: result.payload as ConformanceGraphqlPayload<unknown>,
  };
}

async function runHydrationOperation(
  key: RequestKey,
  operationName: string,
  variables: GraphqlVariables,
  label: string,
): Promise<UpstreamCall> {
  const query = documents[key];
  const result = await runGraphqlRequest(query, variables);
  if (result.status < 200 || result.status >= 300) {
    throw new Error(`${label} returned HTTP ${result.status}: ${JSON.stringify(result.payload, null, 2)}`);
  }
  const payload = result.payload as ConformanceGraphqlPayload<unknown>;
  if (Array.isArray(payload.errors) && payload.errors.length > 0) {
    throw new Error(`${label} returned GraphQL errors: ${JSON.stringify(result.payload, null, 2)}`);
  }
  return {
    operationName,
    variables,
    query,
    response: {
      status: result.status,
      body: payload,
    },
  };
}

async function runCleanup(query: string, variables: GraphqlVariables): Promise<unknown> {
  try {
    const result = await runGraphqlRequest(query, variables);
    return {
      status: result.status,
      body: result.payload,
    };
  } catch (error) {
    return {
      error: error instanceof Error ? error.message : String(error),
    };
  }
}

async function cleanupDestinationLocationId(excludedLocationIds: string[]): Promise<string | null> {
  const result = await runGraphqlRequest(activeLocationsQuery, {});
  if (result.status < 200 || result.status >= 300) {
    return null;
  }
  const nodes = valueAt(result.payload, ['data', 'locations', 'nodes'], 'locations cleanup query');
  if (!Array.isArray(nodes)) {
    return null;
  }
  const excluded = new Set(excludedLocationIds);
  for (const node of nodes) {
    const id = asRecord(node, 'cleanup location')['id'];
    if (typeof id === 'string' && !excluded.has(id)) {
      return id;
    }
  }
  return null;
}

function readLocationId(operation: CapturedOperation): string {
  return stringAt(operation.response, ['data', 'locationAdd', 'location', 'id'], 'locationAdd');
}

function readProductIds(operation: CapturedOperation): {
  productId: string;
  variantId: string;
  inventoryItemId: string;
} {
  return {
    productId: stringAt(operation.response, ['data', 'productCreate', 'product', 'id'], 'productCreate'),
    variantId: stringAt(
      operation.response,
      ['data', 'productCreate', 'product', 'variants', 'nodes', 0, 'id'],
      'productCreate',
    ),
    inventoryItemId: stringAt(
      operation.response,
      ['data', 'productCreate', 'product', 'variants', 'nodes', 0, 'inventoryItem', 'id'],
      'productCreate',
    ),
  };
}

function inventoryLevelForLocation(operation: CapturedOperation, locationId: string): JsonRecord {
  const nodes = asArray(
    valueAt(operation.response, ['data', 'inventoryItem', 'inventoryLevels', 'nodes'], 'inventoryItem read'),
    'inventoryItem.inventoryLevels.nodes',
  );
  const level = nodes.find(
    (candidate) => valueAt(candidate, ['location', 'id'], 'inventory level location') === locationId,
  );
  if (!level) {
    throw new Error(`Inventory item read did not include level for ${locationId}: ${JSON.stringify(nodes)}`);
  }
  return asRecord(level, 'inventory level');
}

function quantityId(level: JsonRecord, name: string): string {
  const quantities = asArray(level['quantities'], 'InventoryLevel.quantities');
  const quantity = quantities.find((candidate) => asRecord(candidate, 'InventoryQuantity')['name'] === name);
  if (!quantity) {
    throw new Error(`Inventory level did not include ${name} quantity: ${JSON.stringify(level)}`);
  }
  return stringAt(quantity, ['id'], `InventoryQuantity ${name}`);
}

function transferIds(
  operation: CapturedOperation,
  root: 'inventoryTransferCreateAsReadyToShip' | 'inventoryTransferCreate',
): {
  transferId: string;
  lineItemId: string;
} {
  return {
    transferId: stringAt(operation.response, ['data', root, 'inventoryTransfer', 'id'], root),
    lineItemId: stringAt(operation.response, ['data', root, 'inventoryTransfer', 'lineItems', 'nodes', 0, 'id'], root),
  };
}

function shipmentIds(operation: CapturedOperation): { shipmentId: string; lineItemId: string } {
  return {
    shipmentId: stringAt(
      operation.response,
      ['data', 'inventoryShipmentCreate', 'inventoryShipment', 'id'],
      'inventoryShipmentCreate',
    ),
    lineItemId: stringAt(
      operation.response,
      ['data', 'inventoryShipmentCreate', 'inventoryShipment', 'lineItems', 'nodes', 0, 'id'],
      'inventoryShipmentCreate',
    ),
  };
}

function locationInput(name: string, address1: string, zip: string): GraphqlVariables {
  return {
    input: {
      name,
      fulfillsOnlineOrders: true,
      address: {
        address1,
        city: 'Boston',
        provinceCode: 'MA',
        countryCode: 'US',
        zip,
      },
    },
  };
}

await mkdir(outputDir, { recursive: true });

const runId = `${Date.now()}`;
const missingIds = {
  item: 'gid://shopify/InventoryItem/999999999999',
  level: 'gid://shopify/InventoryLevel/999999999999?inventory_item_id=999999999999',
  quantity: 'gid://shopify/InventoryQuantity/999999999999?inventory_item_id=999999999999&name=available',
  adjustmentGroup: 'gid://shopify/InventoryAdjustmentGroup/999999999999',
  transfer: 'gid://shopify/InventoryTransfer/999999999999',
  transferLineItem: 'gid://shopify/InventoryTransferLineItem/999999999999',
  shipment: 'gid://shopify/InventoryShipment/999999999999',
  shipmentLineItem: 'gid://shopify/InventoryShipmentLineItem/999999999999',
};

const cleanup: JsonRecord = {};
let productIdForCleanup: string | null = null;
let readyTransferIdForCleanup: string | null = null;
let shipmentIdForCleanup: string | null = null;
let shipmentMutationFirstFixture: JsonRecord | null = null;
const locationIdsForCleanup: string[] = [];

try {
  const originLocationAdd = await runOperation(
    'locationAdd',
    locationInput(`Inventory Node origin ${runId}`, '10 Inventory Node Origin St', '02110'),
    'origin locationAdd',
  );
  expectNoUserErrors(originLocationAdd.response, ['data', 'locationAdd', 'userErrors'], 'origin locationAdd');
  const originLocationId = readLocationId(originLocationAdd);
  locationIdsForCleanup.push(originLocationId);

  const destinationLocationAdd = await runOperation(
    'locationAdd',
    locationInput(`Inventory Node destination ${runId}`, '20 Inventory Node Destination St', '02111'),
    'destination locationAdd',
  );
  expectNoUserErrors(destinationLocationAdd.response, ['data', 'locationAdd', 'userErrors'], 'destination locationAdd');
  const destinationLocationId = readLocationId(destinationLocationAdd);
  locationIdsForCleanup.push(destinationLocationId);

  const productCreate = await runOperation(
    'productCreate',
    {
      product: {
        title: `Inventory Node Conformance ${runId}`,
        status: 'ACTIVE',
      },
    },
    'productCreate',
  );
  expectNoUserErrors(productCreate.response, ['data', 'productCreate', 'userErrors'], 'productCreate');
  const product = readProductIds(productCreate);
  productIdForCleanup = product.productId;

  const trackInventory = await runOperation(
    'track',
    {
      productId: product.productId,
      variants: [
        {
          id: product.variantId,
          inventoryItem: {
            sku: `INV-NODE-${runId}`,
            tracked: true,
            requiresShipping: true,
          },
        },
      ],
    },
    'productVariantsBulkUpdate',
  );
  expectNoUserErrors(
    trackInventory.response,
    ['data', 'productVariantsBulkUpdate', 'userErrors'],
    'productVariantsBulkUpdate',
  );

  const originActivate = await runOperation(
    'activate',
    {
      inventoryItemId: product.inventoryItemId,
      locationId: originLocationId,
      available: 0,
    },
    'origin inventoryActivate',
  );
  expectNoUserErrors(originActivate.response, ['data', 'inventoryActivate', 'userErrors'], 'origin inventoryActivate');

  const destinationActivate = await runOperation(
    'activate',
    {
      inventoryItemId: product.inventoryItemId,
      locationId: destinationLocationId,
      available: 0,
    },
    'destination inventoryActivate',
  );
  expectNoUserErrors(
    destinationActivate.response,
    ['data', 'inventoryActivate', 'userErrors'],
    'destination inventoryActivate',
  );

  const inventorySet = await runOperation(
    'inventorySet',
    {
      input: {
        name: 'available',
        reason: 'correction',
        referenceDocumentUri: `logistics://inventory-node-conformance/${apiVersion}/${runId}`,
        ignoreCompareQuantity: true,
        quantities: [
          {
            inventoryItemId: product.inventoryItemId,
            locationId: originLocationId,
            quantity: 5,
          },
          {
            inventoryItemId: product.inventoryItemId,
            locationId: destinationLocationId,
            quantity: 0,
          },
        ],
      },
    },
    'inventorySetQuantities',
  );
  expectNoUserErrors(inventorySet.response, ['data', 'inventorySetQuantities', 'userErrors'], 'inventorySetQuantities');
  const adjustmentGroupId = stringAt(
    inventorySet.response,
    ['data', 'inventorySetQuantities', 'inventoryAdjustmentGroup', 'id'],
    'inventorySetQuantities',
  );

  const dedicatedRead = await runOperation(
    'dedicatedRead',
    {
      inventoryItemId: product.inventoryItemId,
    },
    'inventoryItem dedicated read',
  );
  const firstLevel = inventoryLevelForLocation(dedicatedRead, originLocationId);
  const firstLevelId = stringAt(firstLevel, ['id'], 'first inventory level');
  const availableQuantityId = quantityId(firstLevel, 'available');

  const coreNodeRead = await runOperation(
    'coreRead',
    {
      levelId: firstLevelId,
      ids: [
        product.inventoryItemId,
        firstLevelId,
        availableQuantityId,
        adjustmentGroupId,
        missingIds.item,
        missingIds.level,
        missingIds.quantity,
        missingIds.adjustmentGroup,
      ],
    },
    'generic inventory item/level/quantity/adjustment Node read',
  );

  const readyTransferCreate = await runOperation(
    'transferCreateReady',
    {
      input: {
        originLocationId,
        destinationLocationId,
        referenceName: `inventory-node-ready-${runId}`,
        note: 'inventory Node ready transfer conformance',
        tags: ['inventory-node-conformance'],
        lineItems: [
          {
            inventoryItemId: product.inventoryItemId,
            quantity: 2,
          },
        ],
      },
    },
    'inventoryTransferCreateAsReadyToShip',
  );
  expectNoUserErrors(
    readyTransferCreate.response,
    ['data', 'inventoryTransferCreateAsReadyToShip', 'userErrors'],
    'inventoryTransferCreateAsReadyToShip',
  );
  const readyTransfer = transferIds(readyTransferCreate, 'inventoryTransferCreateAsReadyToShip');
  readyTransferIdForCleanup = readyTransfer.transferId;

  const transferNodeRead = await runOperation(
    'transferRead',
    {
      ids: [readyTransfer.transferId, readyTransfer.lineItemId, missingIds.transfer, missingIds.transferLineItem],
    },
    'generic inventory transfer Node read',
  );

  const shipmentCreate = await runOperation(
    'shipmentCreate',
    {
      input: {
        movementId: readyTransfer.transferId,
        trackingInput: {
          trackingNumber: `NODE-SHIP-${runId}`,
          company: 'Japan Post',
          trackingUrl: 'https://www.post.japanpost.jp/',
          arrivesAt: '2026-07-01T12:00:00Z',
        },
        lineItems: [
          {
            inventoryItemId: product.inventoryItemId,
            quantity: 1,
          },
        ],
      },
    },
    'inventoryShipmentCreate',
  );
  expectNoUserErrors(
    shipmentCreate.response,
    ['data', 'inventoryShipmentCreate', 'userErrors'],
    'inventoryShipmentCreate',
  );
  const shipment = shipmentIds(shipmentCreate);
  shipmentIdForCleanup = shipment.shipmentId;

  const shipmentTracking = await runOperation(
    'shipmentSetTracking',
    {
      id: shipment.shipmentId,
      tracking: {
        trackingNumber: `NODE-TRACK-${runId}`,
        company: 'Japan Post',
        trackingUrl: 'https://www.post.japanpost.jp/',
        arrivesAt: '2026-07-02T12:00:00Z',
      },
    },
    'inventoryShipmentSetTracking',
  );
  expectNoUserErrors(
    shipmentTracking.response,
    ['data', 'inventoryShipmentSetTracking', 'userErrors'],
    'inventoryShipmentSetTracking',
  );

  const shipmentNodeRead = await runOperation(
    'shipmentRead',
    {
      ids: [shipment.shipmentId, shipment.lineItemId, missingIds.shipment, missingIds.shipmentLineItem],
    },
    'generic inventory shipment Node read',
  );

  const shipmentDelete = await runOperation(
    'shipmentDelete',
    {
      id: shipment.shipmentId,
    },
    'inventoryShipmentDelete',
  );
  expectNoUserErrors(
    shipmentDelete.response,
    ['data', 'inventoryShipmentDelete', 'userErrors'],
    'inventoryShipmentDelete',
  );
  shipmentIdForCleanup = null;

  const shipmentDeletedNodeRead = await runOperation(
    'shipmentRead',
    {
      ids: [shipment.shipmentId, shipment.lineItemId],
    },
    'generic inventory shipment Node read after delete',
  );

  const mutationFirstShipmentCreate = await runOperation(
    'shipmentCreate',
    {
      input: {
        movementId: readyTransfer.transferId,
        trackingInput: {
          trackingNumber: `COLD-SHIP-BEFORE-${runId}`,
          company: 'Japan Post',
          trackingUrl: 'https://www.post.japanpost.jp/',
          arrivesAt: '2026-07-03T12:00:00Z',
        },
        lineItems: [{ inventoryItemId: product.inventoryItemId, quantity: 1 }],
      },
    },
    'inventoryShipmentCreate mutation-first target',
  );
  expectNoUserErrors(
    mutationFirstShipmentCreate.response,
    ['data', 'inventoryShipmentCreate', 'userErrors'],
    'inventoryShipmentCreate mutation-first target',
  );
  const mutationFirstShipment = shipmentIds(mutationFirstShipmentCreate);
  shipmentIdForCleanup = mutationFirstShipment.shipmentId;

  const shipmentHydrate = await runHydrationOperation(
    'shipmentHydrate',
    'InventoryShipmentMutationHydrate',
    { id: mutationFirstShipment.shipmentId, after: null },
    'inventory shipment mutation hydrate',
  );
  stringAt(shipmentHydrate.response.body, ['data', 'inventoryShipment', 'id'], 'inventory shipment mutation hydrate');
  const transferHydrate = await runHydrationOperation(
    'transferHydrate',
    'InventoryTransferMutationHydrate',
    { id: readyTransfer.transferId },
    'inventory shipment owning transfer hydrate',
  );
  const referenceHydrate = await runHydrationOperation(
    'referenceHydrate',
    'ProductsHydrateNodes',
    { ids: [product.inventoryItemId] },
    'inventory shipment reference hydrate',
  );

  const mutationFirstTrackingVariables = {
    id: mutationFirstShipment.shipmentId,
    tracking: {
      trackingNumber: `COLD-SHIP-AFTER-${runId}`,
      company: 'Japan Post',
      trackingUrl: 'https://www.post.japanpost.jp/',
      arrivesAt: '2026-07-04T12:00:00Z',
    },
  };
  const mutationFirstShipmentTracking = await runOperation(
    'shipmentSetTracking',
    mutationFirstTrackingVariables,
    'inventoryShipmentSetTracking mutation-first',
  );
  expectNoUserErrors(
    mutationFirstShipmentTracking.response,
    ['data', 'inventoryShipmentSetTracking', 'userErrors'],
    'inventoryShipmentSetTracking mutation-first',
  );
  const mutationFirstShipmentRead = await runOperation(
    'shipmentDetail',
    { id: mutationFirstShipment.shipmentId },
    'inventoryShipment read after mutation-first tracking',
  );
  const mutationFirstShipmentDelete = await runOperation(
    'shipmentDelete',
    { id: mutationFirstShipment.shipmentId },
    'inventoryShipmentDelete mutation-first cleanup',
  );
  expectNoUserErrors(
    mutationFirstShipmentDelete.response,
    ['data', 'inventoryShipmentDelete', 'userErrors'],
    'inventoryShipmentDelete mutation-first cleanup',
  );
  shipmentIdForCleanup = null;
  shipmentMutationFirstFixture = {
    scenario: 'inventory-shipment-mutation-first-hydration',
    storeDomain,
    apiVersion,
    capturedAt: new Date().toISOString(),
    setup: {
      inventoryItemId: product.inventoryItemId,
      transferId: readyTransfer.transferId,
      shipmentCreate: mutationFirstShipmentCreate,
    },
    operation: {
      query: documents.shipmentSetTracking,
      variables: mutationFirstTrackingVariables,
      response: mutationFirstShipmentTracking.response,
    },
    reads: {
      afterTracking: mutationFirstShipmentRead,
    },
    cleanup: {
      shipmentDelete: mutationFirstShipmentDelete,
      shipmentDeleted: true,
    },
    upstreamCalls: [shipmentHydrate, transferHydrate, referenceHydrate],
    notes:
      'Creates a disposable real draft shipment, records the exact query-only shipment, owning-transfer, and inventory-node hydration calls used by the proxy, then runs inventoryShipmentSetTracking as the primary mutation from a cold proxy session and verifies downstream readback.',
  };

  const draftTransferCreate = await runOperation(
    'transferCreateDraft',
    {
      input: {
        originLocationId,
        destinationLocationId,
        referenceName: `inventory-node-draft-${runId}`,
        note: 'inventory Node draft transfer delete conformance',
        tags: ['inventory-node-conformance', 'draft-delete'],
        lineItems: [
          {
            inventoryItemId: product.inventoryItemId,
            quantity: 0,
          },
        ],
      },
    },
    'inventoryTransferCreate',
  );
  expectNoUserErrors(
    draftTransferCreate.response,
    ['data', 'inventoryTransferCreate', 'userErrors'],
    'inventoryTransferCreate draft',
  );
  const draftTransfer = transferIds(draftTransferCreate, 'inventoryTransferCreate');

  const draftTransferDelete = await runOperation(
    'transferDelete',
    {
      id: draftTransfer.transferId,
    },
    'inventoryTransferDelete',
  );
  expectNoUserErrors(
    draftTransferDelete.response,
    ['data', 'inventoryTransferDelete', 'userErrors'],
    'inventoryTransferDelete draft',
  );

  const transferDeletedNodeRead = await runOperation(
    'transferRead',
    {
      ids: [draftTransfer.transferId, draftTransfer.lineItemId],
    },
    'generic inventory transfer Node read after delete',
  );

  cleanup['readyTransferCancelBeforeProductDelete'] = await runCleanup(inventoryTransferCancelMutation, {
    id: readyTransfer.transferId,
  });
  readyTransferIdForCleanup = null;

  cleanup['productDelete'] = await runCleanup(productDeleteMutation, {
    input: {
      id: product.productId,
    },
  });
  productIdForCleanup = null;

  const cleanupDestination = await cleanupDestinationLocationId(locationIdsForCleanup);
  for (const locationId of [...locationIdsForCleanup].reverse()) {
    cleanup[`location:${locationId}:deactivate`] = await runCleanup(locationDeactivateMutation, {
      locationId,
      destinationLocationId: cleanupDestination,
    });
    cleanup[`location:${locationId}:delete`] = await runCleanup(locationDeleteMutation, { locationId });
  }
  locationIdsForCleanup.length = 0;

  const fixture = {
    scenario: scenarioId,
    storeDomain,
    apiVersion,
    capturedAt: new Date().toISOString(),
    missingIds,
    derived: {
      originLevel: firstLevel,
    },
    workflow: {
      originLocationAdd,
      destinationLocationAdd,
      productCreate,
      trackInventory,
      originActivate,
      destinationActivate,
      inventorySet,
      dedicatedRead,
      coreNodeRead,
      readyTransferCreate,
      transferNodeRead,
      shipmentCreate,
      shipmentTracking,
      shipmentNodeRead,
      shipmentDelete,
      shipmentDeletedNodeRead,
      draftTransferCreate,
      draftTransferDelete,
      transferDeletedNodeRead,
    },
    cleanup,
  };

  if (!shipmentMutationFirstFixture) {
    throw new Error('inventory shipment mutation-first capture did not produce a fixture');
  }
  await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  await writeFile(mutationFirstOutputPath, `${JSON.stringify(shipmentMutationFirstFixture, null, 2)}\n`, 'utf8');
  console.log(
    JSON.stringify({ ok: true, storeDomain, apiVersion, outputs: [outputPath, mutationFirstOutputPath] }, null, 2),
  );
} finally {
  if (shipmentIdForCleanup) {
    cleanup['unhandledShipmentDelete'] = await runCleanup(documents.shipmentDelete, { id: shipmentIdForCleanup });
  }
  if (readyTransferIdForCleanup) {
    cleanup['unhandledReadyTransferCancel'] = await runCleanup(inventoryTransferCancelMutation, {
      id: readyTransferIdForCleanup,
    });
  }
  if (productIdForCleanup) {
    cleanup['unhandledProductDelete'] = await runCleanup(productDeleteMutation, { input: { id: productIdForCleanup } });
  }
  const cleanupDestination = await cleanupDestinationLocationId(locationIdsForCleanup);
  for (const locationId of [...locationIdsForCleanup].reverse()) {
    cleanup[`unhandledLocation:${locationId}:deactivate`] = await runCleanup(locationDeactivateMutation, {
      locationId,
      destinationLocationId: cleanupDestination,
    });
    cleanup[`unhandledLocation:${locationId}:delete`] = await runCleanup(locationDeleteMutation, { locationId });
  }
}

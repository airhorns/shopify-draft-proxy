/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;
type GraphqlPayload = JsonRecord;
type GraphqlVariables = Record<string, unknown>;

type LocationSummary = {
  id: string;
  name: string;
};

type ProductSetup = {
  productId: string;
  variantId: string;
  inventoryItemId: string;
  originLocation: LocationSummary;
  destinationLocation: LocationSummary;
  originLocationCreate: GraphqlPayload;
  destinationLocationCreate: GraphqlPayload;
  create: GraphqlPayload;
  track: GraphqlPayload;
  originActivation: GraphqlPayload;
  destinationActivation: GraphqlPayload;
  inventorySet: GraphqlPayload;
  hydratedItem: GraphqlPayload;
};

type CaseCapture = {
  variables: GraphqlVariables;
  response: GraphqlPayload;
};

type UpstreamCall = {
  operationName: string;
  variables: { ids: string[] };
  query: string;
  response: {
    status: number;
    body: unknown;
  };
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2025-01',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const validationOutputPath = path.join(outputDir, 'inventory-transfer-create-validation.json');
const lifecycleOutputPath = path.join(outputDir, 'inventory-transfer-lifecycle-local-staging.json');

const { runGraphql, runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function runGraphqlAllowGraphqlErrors(query: string, variables: GraphqlVariables = {}): Promise<GraphqlPayload> {
  const result = await runGraphqlRequest(query, variables);
  if (result.status < 200 || result.status >= 300) {
    throw new Error(JSON.stringify(result, null, 2));
  }

  return result.payload as GraphqlPayload;
}

const locationAddMutation = `#graphql
  mutation InventoryTransferConformanceLocationAdd($input: LocationAddInput!) {
    locationAdd(input: $input) {
      location {
        id
        name
        isActive
      }
      userErrors { field message code }
    }
  }
`;

const locationDeactivateMutation = `#graphql
  mutation InventoryTransferConformanceLocationDeactivate($locationId: ID!, $idempotencyKey: String!) {
    locationDeactivate(locationId: $locationId) @idempotent(key: $idempotencyKey) {
      location {
        id
        isActive
      }
      locationDeactivateUserErrors { field message code }
    }
  }
`;

const locationDeleteMutation = `#graphql
  mutation InventoryTransferConformanceLocationDelete($locationId: ID!) {
    locationDelete(locationId: $locationId) {
      deletedLocationId
      locationDeleteUserErrors { field message code }
    }
  }
`;

const createProductMutation = `#graphql
  mutation InventoryTransferConformanceProductCreate($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
        id
        title
        handle
        status
        totalInventory
        tracksInventory
        variants(first: 1) {
          nodes {
            id
            title
            inventoryQuantity
            selectedOptions { name value }
            inventoryItem {
              id
              tracked
              requiresShipping
            }
          }
        }
      }
      userErrors { field message }
    }
  }
`;

const trackInventoryMutation = `#graphql
  mutation InventoryTransferConformanceTrackInventory(
    $productId: ID!
    $variants: [ProductVariantsBulkInput!]!
  ) {
    productVariantsBulkUpdate(productId: $productId, variants: $variants) {
      product {
        id
        totalInventory
        tracksInventory
      }
      productVariants {
        id
        inventoryQuantity
        inventoryItem {
          id
          tracked
          requiresShipping
        }
      }
      userErrors { field message }
    }
  }
`;

const inventoryActivateMutation = `#graphql
  mutation InventoryTransferConformanceInventoryActivate(
    $inventoryItemId: ID!
    $locationId: ID!
    $available: Int
  ) {
    inventoryActivate(inventoryItemId: $inventoryItemId, locationId: $locationId, available: $available) {
      inventoryLevel {
        id
        location { id name }
        item { id }
      }
      userErrors { field message }
    }
  }
`;

const inventorySetQuantitiesMutation = `#graphql
  mutation InventoryTransferConformanceInventorySet($input: InventorySetQuantitiesInput!) {
    inventorySetQuantities(input: $input) {
      inventoryAdjustmentGroup {
        id
        reason
        referenceDocumentUri
        changes {
          name
          delta
          quantityAfterChange
          item { id }
          location { id name }
        }
      }
      userErrors { field message code }
    }
  }
`;

const inventoryItemReadQuery = `#graphql
  query InventoryTransferConformanceInventoryItem($id: ID!) {
    inventoryItem(id: $id) {
      id
      tracked
      requiresShipping
      variant {
        id
        title
        inventoryQuantity
        selectedOptions { name value }
        product {
          id
          title
          handle
          status
          totalInventory
          tracksInventory
        }
      }
      inventoryLevels(first: 50) {
        nodes {
          id
          location { id name }
          quantities(names: ["available", "reserved", "on_hand"]) {
            name
            quantity
            updatedAt
          }
        }
      }
    }
  }
`;

const productHydrateNodesQuery = `#graphql
  query ProductsHydrateNodes($ids: [ID!]!) {
    nodes(ids: $ids) {
      __typename
      id
      ... on InventoryItem {
        tracked
        requiresShipping
        measurement { weight { unit value } }
        variant {
          id
          title
          inventoryQuantity
          selectedOptions { name value }
          product {
            id
            title
            handle
            status
            totalInventory
            tracksInventory
          }
        }
        inventoryLevels(first: 50) {
          nodes {
            id
            location { id name }
            quantities(names: ["available", "on_hand", "committed", "incoming", "reserved", "damaged", "quality_control", "safety_stock"]) {
              name
              quantity
              updatedAt
            }
          }
        }
      }
      ... on Location {
        id
        name
        isActive
      }
    }
  }
`;

const inventoryTransferCreateValidationMutation = `#graphql
  mutation InventoryTransferCreateValidationParity($input: InventoryTransferCreateInput!) {
    inventoryTransferCreate(input: $input) {
      inventoryTransfer {
        id
        name
        status
        totalQuantity
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const inventoryTransferCreateMutation = `#graphql
  mutation InventoryTransferCreateParity($input: InventoryTransferCreateInput!) {
    inventoryTransferCreate(input: $input) {
      inventoryTransfer {
        id
        name
        status
        totalQuantity
        lineItems(first: 10) {
          nodes {
            totalQuantity
            shippableQuantity
            shippedQuantity
            processableQuantity
            pickedForShipmentQuantity
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

const inventoryTransferMarkReadyMutation = `#graphql
  mutation InventoryTransferMarkReadyParity($id: ID!) {
    inventoryTransferMarkAsReadyToShip(id: $id) {
      inventoryTransfer {
        status
        totalQuantity
        lineItems(first: 10) {
          nodes {
            totalQuantity
            shippableQuantity
            shippedQuantity
            processableQuantity
            pickedForShipmentQuantity
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

const inventoryTransferInventoryReadQuery = `#graphql
  query InventoryTransferInventoryReadParity($id: ID!) {
    inventoryItem(id: $id) {
      variant {
        inventoryQuantity
      }
      inventoryLevels(first: 50) {
        nodes {
          location {
            id
          }
          quantities(names: ["available", "reserved", "on_hand"]) {
            name
            quantity
          }
        }
      }
    }
  }
`;

const inventoryTransferCancelMutation = `#graphql
  mutation InventoryTransferCancelParity($id: ID!) {
    inventoryTransferCancel(id: $id) {
      inventoryTransfer {
        status
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const inventoryTransferDeleteMutation = `#graphql
  mutation InventoryTransferDeleteParity($id: ID!) {
    inventoryTransferDelete(id: $id) {
      deletedId
      userErrors {
        field
        message
      }
    }
  }
`;

const productDeleteMutation = `#graphql
  mutation InventoryTransferConformanceProductDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors { field message }
    }
  }
`;

function readRecord(value: unknown, label: string): JsonRecord {
  if (typeof value === 'object' && value !== null && !Array.isArray(value)) {
    return value as JsonRecord;
  }
  throw new Error(`${label} was not an object: ${JSON.stringify(value)}`);
}

function readPath(value: unknown, pathSegments: string[], label: string): unknown {
  let current = value;
  for (const segment of pathSegments) {
    if (Array.isArray(current)) {
      const index = Number.parseInt(segment, 10);
      if (Number.isInteger(index) && index >= 0 && index < current.length) {
        current = current[index];
        continue;
      }
      throw new Error(`${label} was missing array index ${segment}: ${JSON.stringify(value)}`);
    }
    const record = readRecord(current, label);
    current = record[segment];
  }
  return current;
}

function readStringPath(value: unknown, pathSegments: string[], label: string): string {
  const candidate = readPath(value, pathSegments, label);
  if (typeof candidate === 'string') {
    return candidate;
  }
  throw new Error(`${label} was missing string path ${pathSegments.join('.')}: ${JSON.stringify(value)}`);
}

function readUserErrors(payload: unknown, pathSegments: string[]): unknown[] {
  const candidate = readPath(payload, pathSegments, 'GraphQL payload');
  return Array.isArray(candidate) ? candidate : [];
}

function expectNoUserErrors(payload: unknown, pathSegments: string[], label: string): void {
  const userErrors = readUserErrors(payload, pathSegments);
  if (userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function readCreatedProduct(payload: GraphqlPayload): {
  productId: string;
  variantId: string;
  inventoryItemId: string;
} {
  return {
    productId: readStringPath(payload, ['data', 'productCreate', 'product', 'id'], 'productCreate'),
    variantId: readStringPath(
      payload,
      ['data', 'productCreate', 'product', 'variants', 'nodes', '0', 'id'],
      'productCreate',
    ),
    inventoryItemId: readStringPath(
      payload,
      ['data', 'productCreate', 'product', 'variants', 'nodes', '0', 'inventoryItem', 'id'],
      'productCreate',
    ),
  };
}

function readCreatedLocation(payload: GraphqlPayload): LocationSummary {
  return {
    id: readStringPath(payload, ['data', 'locationAdd', 'location', 'id'], 'locationAdd'),
    name: readStringPath(payload, ['data', 'locationAdd', 'location', 'name'], 'locationAdd'),
  };
}

function readTransferId(payload: GraphqlPayload, pathSegments: string[]): string {
  return readStringPath(payload, pathSegments, 'inventory transfer mutation');
}

async function deleteProduct(productId: string | null): Promise<GraphqlPayload | null> {
  if (!productId) {
    return null;
  }

  try {
    return await runGraphqlAllowGraphqlErrors(productDeleteMutation, { input: { id: productId } });
  } catch (error) {
    console.warn(`Product cleanup failed for ${productId}: ${error instanceof Error ? error.message : String(error)}`);
    return null;
  }
}

async function createLocation(
  runId: string,
  role: 'origin' | 'destination',
): Promise<{ payload: GraphqlPayload; location: LocationSummary }> {
  const payload = await runGraphqlAllowGraphqlErrors(locationAddMutation, {
    input: {
      name: `Inventory transfer conformance ${role} ${runId}`,
      fulfillsOnlineOrders: true,
      address: {
        address1: role === 'origin' ? '10 Origin Test St' : '20 Destination Test St',
        city: 'Boston',
        provinceCode: 'MA',
        countryCode: 'US',
        zip: role === 'origin' ? '02110' : '02111',
      },
    },
  });
  expectNoUserErrors(payload, ['data', 'locationAdd', 'userErrors'], 'locationAdd');

  return {
    payload,
    location: readCreatedLocation(payload),
  };
}

async function cleanupLocation(locationId: string, runId: string): Promise<JsonRecord> {
  const cleanup: JsonRecord = {};
  try {
    cleanup['deactivate'] = await runGraphqlAllowGraphqlErrors(locationDeactivateMutation, {
      locationId,
      idempotencyKey: `inventory-transfer-conformance-deactivate-${runId}-${locationId.split('/').at(-1) ?? 'location'}`,
    });
  } catch (error) {
    cleanup['deactivateError'] = error instanceof Error ? error.message : String(error);
  }

  try {
    cleanup['delete'] = await runGraphqlAllowGraphqlErrors(locationDeleteMutation, { locationId });
  } catch (error) {
    cleanup['deleteError'] = error instanceof Error ? error.message : String(error);
  }

  return cleanup;
}

async function createSetup(runId: string): Promise<ProductSetup> {
  const origin = await createLocation(runId, 'origin');
  const destination = await createLocation(runId, 'destination');

  const create = (await runGraphql(createProductMutation, {
    product: {
      title: `Inventory transfer conformance ${runId}`,
      status: 'ACTIVE',
    },
  })) as GraphqlPayload;
  expectNoUserErrors(create, ['data', 'productCreate', 'userErrors'], 'productCreate');
  const product = readCreatedProduct(create);

  const track = (await runGraphql(trackInventoryMutation, {
    productId: product.productId,
    variants: [
      {
        id: product.variantId,
        inventoryItem: {
          tracked: true,
          requiresShipping: true,
        },
      },
    ],
  })) as GraphqlPayload;
  expectNoUserErrors(track, ['data', 'productVariantsBulkUpdate', 'userErrors'], 'productVariantsBulkUpdate');

  const originActivation = await runGraphqlAllowGraphqlErrors(inventoryActivateMutation, {
    inventoryItemId: product.inventoryItemId,
    locationId: origin.location.id,
    available: 5,
  });
  expectNoUserErrors(originActivation, ['data', 'inventoryActivate', 'userErrors'], 'origin inventoryActivate');
  const destinationActivation = await runGraphqlAllowGraphqlErrors(inventoryActivateMutation, {
    inventoryItemId: product.inventoryItemId,
    locationId: destination.location.id,
    available: 0,
  });
  expectNoUserErrors(
    destinationActivation,
    ['data', 'inventoryActivate', 'userErrors'],
    'destination inventoryActivate',
  );

  const inventorySet = await runGraphqlAllowGraphqlErrors(inventorySetQuantitiesMutation, {
    input: {
      name: 'available',
      reason: 'correction',
      referenceDocumentUri: `logistics://inventory-transfer-conformance/${apiVersion}/${runId}`,
      ignoreCompareQuantity: true,
      quantities: [
        {
          inventoryItemId: product.inventoryItemId,
          locationId: origin.location.id,
          quantity: 5,
        },
        {
          inventoryItemId: product.inventoryItemId,
          locationId: destination.location.id,
          quantity: 0,
        },
      ],
    },
  });
  expectNoUserErrors(inventorySet, ['data', 'inventorySetQuantities', 'userErrors'], 'inventorySetQuantities');

  const hydratedItem = (await runGraphql(inventoryItemReadQuery, { id: product.inventoryItemId })) as GraphqlPayload;

  return {
    ...product,
    originLocation: origin.location,
    destinationLocation: destination.location,
    originLocationCreate: origin.payload,
    destinationLocationCreate: destination.payload,
    create,
    track,
    originActivation,
    destinationActivation,
    inventorySet,
    hydratedItem,
  };
}

function transferInput(setup: ProductSetup, quantity: number): JsonRecord {
  return {
    originLocationId: setup.originLocation.id,
    destinationLocationId: setup.destinationLocation.id,
    referenceName: `inventory-transfer-conformance-${Date.now()}`,
    note: 'inventory transfer conformance',
    tags: ['inventory-transfer-conformance'],
    lineItems: [
      {
        inventoryItemId: setup.inventoryItemId,
        quantity,
      },
    ],
  };
}

async function captureValidationCases(setup: ProductSetup): Promise<Record<string, CaseCapture>> {
  const unknownLocationId = 'gid://shopify/Location/999999999999';
  const unknownInventoryItemId = 'gid://shopify/InventoryItem/999999999999';
  const caseVariables: Record<string, GraphqlVariables> = {
    sameLocation: {
      input: {
        originLocationId: setup.originLocation.id,
        destinationLocationId: setup.originLocation.id,
        lineItems: [{ inventoryItemId: setup.inventoryItemId, quantity: 1 }],
      },
    },
    unknownOrigin: {
      input: {
        originLocationId: unknownLocationId,
        destinationLocationId: setup.destinationLocation.id,
        lineItems: [{ inventoryItemId: setup.inventoryItemId, quantity: 1 }],
      },
    },
    unknownItem: {
      input: {
        originLocationId: setup.originLocation.id,
        destinationLocationId: setup.destinationLocation.id,
        lineItems: [{ inventoryItemId: unknownInventoryItemId, quantity: 1 }],
      },
    },
    duplicateItem: {
      input: {
        originLocationId: setup.originLocation.id,
        destinationLocationId: setup.destinationLocation.id,
        lineItems: [
          { inventoryItemId: setup.inventoryItemId, quantity: 1 },
          { inventoryItemId: setup.inventoryItemId, quantity: 2 },
        ],
      },
    },
    negativeQuantity: {
      input: {
        originLocationId: setup.originLocation.id,
        destinationLocationId: setup.destinationLocation.id,
        lineItems: [{ inventoryItemId: setup.inventoryItemId, quantity: -1 }],
      },
    },
  };

  const cases: Record<string, CaseCapture> = {};
  for (const [caseId, variables] of Object.entries(caseVariables)) {
    cases[caseId] = {
      variables,
      response: await runGraphqlAllowGraphqlErrors(inventoryTransferCreateValidationMutation, variables),
    };
  }

  return cases;
}

async function captureLifecycle(setup: ProductSetup): Promise<{
  workflow: JsonRecord;
  beforeReadyInventoryRead: GraphqlPayload;
  draftCreate: GraphqlPayload;
  readyTransition: GraphqlPayload;
  readyInventoryReadAfterWriteGraphql: GraphqlPayload;
  cancelReadyTransfer: GraphqlPayload;
  deleteNonDraftGuardrail: GraphqlPayload;
  cleanup: JsonRecord;
}> {
  let transferId: string | null = null;
  const createVariables = {
    input: transferInput(setup, 2),
  };
  const beforeReadyInventoryRead = (await runGraphql(inventoryTransferInventoryReadQuery, {
    id: setup.inventoryItemId,
  })) as GraphqlPayload;
  const draftCreate = await runGraphqlAllowGraphqlErrors(inventoryTransferCreateMutation, createVariables);
  transferId = readTransferId(draftCreate, ['data', 'inventoryTransferCreate', 'inventoryTransfer', 'id']);
  const readyTransition = await runGraphqlAllowGraphqlErrors(inventoryTransferMarkReadyMutation, { id: transferId });
  const readyInventoryReadAfterWriteGraphql = (await runGraphql(inventoryTransferInventoryReadQuery, {
    id: setup.inventoryItemId,
  })) as GraphqlPayload;
  const cancelReadyTransfer = await runGraphqlAllowGraphqlErrors(inventoryTransferCancelMutation, { id: transferId });
  const deleteNonDraftGuardrail = await runGraphqlAllowGraphqlErrors(inventoryTransferDeleteMutation, {
    id: transferId,
  });

  return {
    workflow: {
      createDraft: {
        variables: createVariables,
      },
      afterReadyInventoryRead: {
        variables: {
          id: setup.inventoryItemId,
        },
      },
    },
    beforeReadyInventoryRead,
    draftCreate,
    readyTransition,
    readyInventoryReadAfterWriteGraphql,
    cancelReadyTransfer,
    deleteNonDraftGuardrail,
    cleanup: {
      readyTransferCanceled:
        readUserErrors(cancelReadyTransfer, ['data', 'inventoryTransferCancel', 'userErrors']).length === 0,
      canceledTransfersRemainBecauseShopifyRejectsNonDraftDelete: true,
    },
  };
}

function uniqueIdLists(lists: string[][]): string[][] {
  const seen = new Set<string>();
  const unique: string[][] = [];
  for (const ids of lists) {
    const key = JSON.stringify(ids);
    if (!seen.has(key)) {
      seen.add(key);
      unique.push(ids);
    }
  }
  return unique;
}

async function hydrateCall(ids: string[]): Promise<UpstreamCall> {
  const response = await runGraphqlRequest(productHydrateNodesQuery, { ids });
  if (response.status < 200 || response.status >= 300) {
    throw new Error(`Hydration call failed: ${JSON.stringify(response, null, 2)}`);
  }

  return {
    operationName: 'ProductsHydrateNodes',
    variables: { ids },
    query: productHydrateNodesQuery,
    response: {
      status: response.status,
      body: response.payload,
    },
  };
}

async function buildValidationUpstreamCalls(setup: ProductSetup): Promise<UpstreamCall[]> {
  const origin = setup.originLocation.id;
  const destination = setup.destinationLocation.id;
  const item = setup.inventoryItemId;
  const unknownLocation = 'gid://shopify/Location/999999999999';
  const unknownItem = 'gid://shopify/InventoryItem/999999999999';
  const idLists = uniqueIdLists([
    [origin, item],
    [destination, unknownLocation],
    [unknownLocation, destination],
    [destination, unknownLocation, item],
    [unknownLocation, destination, item],
    [destination, item, unknownLocation],
    [origin, destination, unknownItem],
    [destination, unknownItem],
    [unknownItem],
    [origin, destination, item],
    [unknownLocation],
  ]);

  return Promise.all(idLists.map(hydrateCall));
}

async function buildLifecycleUpstreamCalls(setup: ProductSetup): Promise<UpstreamCall[]> {
  return [await hydrateCall([setup.originLocation.id, setup.destinationLocation.id, setup.inventoryItemId])];
}

await mkdir(outputDir, { recursive: true });

const runId = `${Date.now()}`;
let productIdForCleanup: string | null = null;
let locationIdsForCleanup: string[] = [];
const setup = await createSetup(runId);
productIdForCleanup = setup.productId;
locationIdsForCleanup = [setup.originLocation.id, setup.destinationLocation.id];

try {
  const validationFixture = {
    scenario: 'inventory-transfer-create-validation',
    storeDomain,
    apiVersion,
    capturedAt: new Date().toISOString(),
    setup: {
      productId: setup.productId,
      variantId: setup.variantId,
      inventoryItemId: setup.inventoryItemId,
      originLocation: setup.originLocation,
      destinationLocation: setup.destinationLocation,
      originLocationCreate: setup.originLocationCreate,
      destinationLocationCreate: setup.destinationLocationCreate,
      create: setup.create,
      track: setup.track,
      originActivation: setup.originActivation,
      destinationActivation: setup.destinationActivation,
      inventorySet: setup.inventorySet,
      hydratedItem: setup.hydratedItem,
    },
    cases: await captureValidationCases(setup),
    upstreamCalls: await buildValidationUpstreamCalls(setup),
  };

  const lifecycle = await captureLifecycle(setup);
  const lifecycleFixture = {
    scenario: 'inventory-transfer-lifecycle-local-staging',
    storeDomain,
    apiVersion,
    capturedAt: new Date().toISOString(),
    setup: {
      productId: setup.productId,
      variantId: setup.variantId,
      inventoryItemId: setup.inventoryItemId,
      originLocation: setup.originLocation,
      destinationLocation: setup.destinationLocation,
    },
    ...lifecycle,
    upstreamCalls: await buildLifecycleUpstreamCalls(setup),
  };

  const cleanup = await deleteProduct(setup.productId);
  productIdForCleanup = null;
  const locationCleanup = {
    origin: await cleanupLocation(setup.originLocation.id, runId),
    destination: await cleanupLocation(setup.destinationLocation.id, runId),
  };
  locationIdsForCleanup = [];
  lifecycleFixture.cleanup = {
    ...lifecycleFixture.cleanup,
    productDelete: cleanup,
    locationCleanup,
    productsDeleted: cleanup !== null && readUserErrors(cleanup, ['data', 'productDelete', 'userErrors']).length === 0,
  };

  await writeFile(validationOutputPath, `${JSON.stringify(validationFixture, null, 2)}\n`, 'utf8');
  await writeFile(lifecycleOutputPath, `${JSON.stringify(lifecycleFixture, null, 2)}\n`, 'utf8');

  console.log(
    JSON.stringify(
      {
        ok: true,
        storeDomain,
        apiVersion,
        outputs: [validationOutputPath, lifecycleOutputPath],
      },
      null,
      2,
    ),
  );
} finally {
  await deleteProduct(productIdForCleanup);
  for (const locationId of locationIdsForCleanup) {
    await cleanupLocation(locationId, runId);
  }
}

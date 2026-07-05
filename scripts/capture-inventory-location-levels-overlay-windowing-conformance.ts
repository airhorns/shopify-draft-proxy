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

const scenarioId = 'inventory-location-levels-overlay-windowing';
const requestDir = path.join('config', 'parity-requests', 'products');
const requestPaths = {
  locationAdd: path.join(requestDir, 'inventory-connection-location-add.graphql'),
  productSet: path.join(requestDir, 'inventory-connection-product-set.graphql'),
  inventorySet: path.join(requestDir, 'inventory-location-levels-set.graphql'),
  inventoryAdjust: path.join(requestDir, 'inventory-location-levels-adjust.graphql'),
  overlayRead: path.join(requestDir, 'inventory-location-levels-read.graphql'),
  itemRead: path.join(requestDir, 'inventory-location-levels-item-read.graphql'),
  windowRead: path.join(requestDir, 'inventory-location-levels-window.graphql'),
} as const;

const productDeleteMutation = `#graphql
  mutation InventoryLocationLevelsCleanupProductDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors { field message }
    }
  }
`;

const locationDeactivateMutation = `#graphql
  mutation InventoryLocationLevelsCleanupLocationDeactivate($locationId: ID!, $destinationLocationId: ID) {
    locationDeactivate(locationId: $locationId, destinationLocationId: $destinationLocationId) {
      location { id isActive }
      locationDeactivateUserErrors { field message code }
    }
  }
`;

const locationDeleteMutation = `#graphql
  mutation InventoryLocationLevelsCleanupLocationDelete($locationId: ID!) {
    locationDelete(locationId: $locationId) {
      deletedLocationId
      locationDeleteUserErrors { field message code }
    }
  }
`;

const activeLocationsQuery = `#graphql
  query InventoryLocationLevelsCleanupLocations {
    locations(first: 20, includeInactive: false) {
      nodes { id name isActive }
    }
  }
`;

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

function stringValue(value: unknown, label: string): string {
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`${label} was not a non-empty string: ${JSON.stringify(value)}`);
  }
  return value;
}

function resourceIdTail(id: string): string {
  const pathPart = id.split('?')[0] ?? id;
  return pathPart.split('/').pop() ?? id;
}

function userErrorsAt(payload: ConformanceGraphqlPayload<unknown>, root: string): unknown[] {
  const data = asRecord(payload.data, 'payload.data');
  const rootPayload = asRecord(data[root], `payload.data.${root}`);
  return Array.isArray(rootPayload['userErrors']) ? rootPayload['userErrors'] : [];
}

function assertNoUserErrors(payload: ConformanceGraphqlPayload<unknown>, root: string): void {
  const userErrors = userErrorsAt(payload, root);
  if (userErrors.length > 0) {
    throw new Error(`${root} returned userErrors: ${JSON.stringify(userErrors)}`);
  }
}

function operationData(operation: CapturedOperation, root: string): JsonRecord {
  return asRecord(asRecord(operation.response.data, `${root}.data`)[root], `${root} payload`);
}

function quantityAt(operation: CapturedOperation, pathLabel: string): number {
  const quantity = operation.response.data as unknown;
  const value = pathLabel.split('.').reduce<unknown>((cursor, part) => {
    if (cursor === undefined || cursor === null) return undefined;
    if (Array.isArray(cursor)) return cursor[Number(part)];
    if (typeof cursor === 'object') return (cursor as JsonRecord)[part];
    return undefined;
  }, quantity);
  if (typeof value !== 'number') {
    throw new Error(`${pathLabel} was not a number: ${JSON.stringify(value)}`);
  }
  return value;
}

function connectionEdgeCursor(operation: CapturedOperation, edgeIndex: number): string {
  const data = asRecord(operation.response.data, 'overlayRead.data');
  const location = asRecord(data['location'], 'overlayRead.data.location');
  const connection = asRecord(location['inventoryLevels'], 'overlayRead.location.inventoryLevels');
  const edges = asArray(connection['edges'], 'overlayRead.location.inventoryLevels.edges');
  return stringValue(asRecord(edges[edgeIndex], `edge ${edgeIndex}`)['cursor'], `edge ${edgeIndex}.cursor`);
}

async function readRequest(key: keyof typeof requestPaths): Promise<string> {
  return readFile(requestPaths[key], 'utf8');
}

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2025-01',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const outputPath = path.join(outputDir, `${scenarioId}.json`);
const runId = Date.now().toString();
const skuAlpha = `INV-LOC-LEVEL-${runId}-ALPHA`;
const skuBeta = `INV-LOC-LEVEL-${runId}-BETA`;

async function runOperation(
  key: keyof typeof requestPaths,
  variables: GraphqlVariables,
  label: string,
): Promise<CapturedOperation> {
  const query = await readRequest(key);
  const result = await runGraphqlRequest(query, variables);
  if (result.status < 200 || result.status >= 300) {
    throw new Error(`${label} returned HTTP ${result.status}: ${JSON.stringify(result.payload)}`);
  }
  if (Array.isArray(result.payload.errors) && result.payload.errors.length > 0) {
    throw new Error(`${label} returned GraphQL errors: ${JSON.stringify(result.payload.errors)}`);
  }
  return { query, variables, response: result.payload };
}

async function cleanup(productId: string | null, locationId: string | null): Promise<JsonRecord> {
  const cleanupResult: JsonRecord = {};
  if (productId !== null) {
    try {
      cleanupResult['productDelete'] = (
        await runGraphqlRequest(productDeleteMutation, { input: { id: productId } })
      ).payload;
    } catch (error) {
      cleanupResult['productDelete'] = { error: String(error) };
    }
  }
  if (locationId !== null) {
    let destinationLocationId: string | null = null;
    try {
      const locations = (await runGraphqlRequest(activeLocationsQuery, {})).payload;
      const nodes = asArray(
        asRecord(asRecord(locations.data, 'cleanup.locations.data')['locations'], 'cleanup.locations')['nodes'],
        'cleanup.locations.nodes',
      );
      destinationLocationId =
        nodes
          .map((node) => asRecord(node, 'cleanup.location'))
          .map((node) => node['id'])
          .find((id): id is string => typeof id === 'string' && id !== locationId) ?? null;
      cleanupResult['relocationDestinationLocationId'] = destinationLocationId;
    } catch (error) {
      cleanupResult['relocationDestinationLocationId'] = { error: String(error) };
    }
    try {
      cleanupResult['locationDeactivate'] = (
        await runGraphqlRequest(locationDeactivateMutation, {
          locationId,
          destinationLocationId,
        })
      ).payload;
    } catch (error) {
      cleanupResult['locationDeactivate'] = { error: String(error) };
    }
    try {
      cleanupResult['locationDelete'] = (await runGraphqlRequest(locationDeleteMutation, { locationId })).payload;
    } catch (error) {
      cleanupResult['locationDelete'] = { error: String(error) };
    }
  }
  return cleanupResult;
}

let productId: string | null = null;
let locationId: string | null = null;

try {
  const locationAdd = await runOperation(
    'locationAdd',
    {
      input: {
        name: `Inventory Location Levels ${runId}`,
        address: { countryCode: 'US' },
      },
    },
    'locationAdd',
  );
  assertNoUserErrors(locationAdd.response, 'locationAdd');
  locationId = stringValue(
    asRecord(operationData(locationAdd, 'locationAdd')['location'], 'locationAdd.location')['id'],
    'locationAdd.location.id',
  );

  const productSet = await runOperation(
    'productSet',
    {
      synchronous: true,
      input: {
        title: `Inventory location levels ${runId}`,
        status: 'ACTIVE',
        productOptions: [{ name: 'Title', position: 1, values: [{ name: 'Alpha' }, { name: 'Beta' }] }],
        variants: [
          {
            optionValues: [{ optionName: 'Title', name: 'Alpha' }],
            price: '10.00',
            sku: skuAlpha,
            inventoryItem: { tracked: true, requiresShipping: true },
            inventoryQuantities: [{ locationId, name: 'available', quantity: 0 }],
          },
          {
            optionValues: [{ optionName: 'Title', name: 'Beta' }],
            price: '11.00',
            sku: skuBeta,
            inventoryItem: { tracked: true, requiresShipping: true },
            inventoryQuantities: [{ locationId, name: 'available', quantity: 0 }],
          },
        ],
      },
    },
    'productSet',
  );
  assertNoUserErrors(productSet.response, 'productSet');
  const productSetData = operationData(productSet, 'productSet');
  const product = asRecord(productSetData['product'], 'productSet.product');
  productId = stringValue(product['id'], 'productSet.product.id');
  const variants = asArray(asRecord(product['variants'], 'product.variants')['nodes'], 'product.variants.nodes').map(
    (entry) => asRecord(entry, 'variant'),
  );
  const alphaVariant = variants.find((variant) => variant['sku'] === skuAlpha);
  const betaVariant = variants.find((variant) => variant['sku'] === skuBeta);
  if (!alphaVariant || !betaVariant) {
    throw new Error(`Could not find both setup variants: ${JSON.stringify(variants)}`);
  }
  const alphaInventoryItemId = stringValue(
    asRecord(alphaVariant['inventoryItem'], 'alpha.inventoryItem')['id'],
    'alpha.inventoryItem.id',
  );
  const betaInventoryItemId = stringValue(
    asRecord(betaVariant['inventoryItem'], 'beta.inventoryItem')['id'],
    'beta.inventoryItem.id',
  );

  const inventorySet = await runOperation(
    'inventorySet',
    {
      input: {
        name: 'available',
        reason: 'correction',
        ignoreCompareQuantity: true,
        quantities: [
          { inventoryItemId: alphaInventoryItemId, locationId, quantity: 4 },
          { inventoryItemId: betaInventoryItemId, locationId, quantity: 8 },
        ],
      },
    },
    'inventorySetQuantities',
  );
  assertNoUserErrors(inventorySet.response, 'inventorySetQuantities');

  const inventoryAdjust = await runOperation(
    'inventoryAdjust',
    {
      input: {
        name: 'available',
        reason: 'correction',
        changes: [{ inventoryItemId: alphaInventoryItemId, locationId, delta: 3 }],
      },
    },
    'inventoryAdjustQuantities',
  );
  assertNoUserErrors(inventoryAdjust.response, 'inventoryAdjustQuantities');

  const overlayRead = await runOperation(
    'overlayRead',
    {
      locationId,
      inventoryItemQuery: `inventory_item_id:${resourceIdTail(alphaInventoryItemId)}`,
    },
    'location inventoryLevels overlay read',
  );
  if (quantityAt(overlayRead, 'location.inventoryLevels.nodes.0.quantities.0.quantity') !== 7) {
    throw new Error(
      `Adjusted location inventory level did not read back as 7: ${JSON.stringify(overlayRead.response)}`,
    );
  }
  const overlayLocation = asRecord(
    asRecord(overlayRead.response.data, 'overlayRead.data')['location'],
    'overlayRead.data.location',
  );
  const filteredConnection = asRecord(overlayLocation['filtered'], 'overlayRead.data.location.filtered');
  const filteredNodes = asArray(filteredConnection['nodes'], 'overlayRead.data.location.filtered.nodes');
  if (filteredNodes.length !== 1) {
    throw new Error(
      `Inventory item query did not narrow Location.inventoryLevels: ${JSON.stringify(overlayRead.response)}`,
    );
  }
  const filteredItemId = stringValue(
    asRecord(asRecord(filteredNodes[0], 'filtered node')['item'], 'filtered node.item')['id'],
    'filtered node.item.id',
  );
  if (filteredItemId !== alphaInventoryItemId) {
    throw new Error(`Filtered Location.inventoryLevels returned ${filteredItemId}, expected ${alphaInventoryItemId}`);
  }

  const itemRead = await runOperation(
    'itemRead',
    {
      locationId,
      firstInventoryItemId: alphaInventoryItemId,
    },
    'inventoryItem inventoryLevel read',
  );
  if (quantityAt(itemRead, 'firstItem.inventoryLevel.quantities.0.quantity') !== 7) {
    throw new Error(`Item-level inventoryLevel did not agree with location read: ${JSON.stringify(itemRead.response)}`);
  }

  const windowRead = await runOperation(
    'windowRead',
    {
      locationId,
      after: connectionEdgeCursor(overlayRead, 0),
      before: connectionEdgeCursor(overlayRead, 1),
    },
    'location inventoryLevels cursor window read',
  );

  const cleanupResult = await cleanup(productId, locationId);
  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        scenario: scenarioId,
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        setup: {
          skuAlpha,
          skuBeta,
          productId,
          locationId,
          alphaInventoryItemId,
          betaInventoryItemId,
        },
        operations: {
          locationAdd,
          productSet,
          inventorySet,
          inventoryAdjust,
          overlayRead,
          itemRead,
          windowRead,
        },
        cleanup: cleanupResult,
      },
      null,
      2,
    )}\n`,
  );
  console.log(`Wrote ${outputPath}`);
} catch (error) {
  const cleanupResult = await cleanup(productId, locationId);
  console.error(JSON.stringify({ error: String(error), cleanup: cleanupResult }, null, 2));
  process.exitCode = 1;
}

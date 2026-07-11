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

const scenarioId = 'inventory-item-levels-connection-windowing-2026-04';
const requestDir = path.join('config', 'parity-requests', 'products');
const requestPaths = {
  locationAdd: path.join(requestDir, 'inventory-connection-location-add.graphql'),
  productSet: path.join(requestDir, 'inventory-connection-product-set.graphql'),
  inventorySet: path.join(requestDir, 'inventory-quantity-contracts-2026-set.graphql'),
  inventoryActivate: path.join(requestDir, 'inventory-item-levels-activate.graphql'),
  inventoryAdjust: path.join(requestDir, 'inventory-quantity-contracts-2026-adjust.graphql'),
  connectionRead: path.join(requestDir, 'inventory-item-levels-read.graphql'),
  inventoryDeactivate: path.join(requestDir, 'inventory-inactive-lifecycle-deactivate.graphql'),
  windowRead: path.join(requestDir, 'inventory-item-levels-window.graphql'),
} as const;

const productDeleteMutation = `#graphql
  mutation InventoryItemLevelsCleanupProductDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors { field message }
    }
  }
`;

const locationDeactivateMutation = `#graphql
  mutation InventoryItemLevelsCleanupLocationDeactivate(
    $locationId: ID!
    $destinationLocationId: ID
    $idempotencyKey: String!
  ) {
    locationDeactivate(
      locationId: $locationId
      destinationLocationId: $destinationLocationId
    ) @idempotent(key: $idempotencyKey) {
      location { id isActive }
      locationDeactivateUserErrors { field message code }
    }
  }
`;

const locationDeleteMutation = `#graphql
  mutation InventoryItemLevelsCleanupLocationDelete($locationId: ID!) {
    locationDelete(locationId: $locationId) {
      deletedLocationId
      locationDeleteUserErrors { field message code }
    }
  }
`;

const activeLocationsQuery = `#graphql
  query InventoryItemLevelsCleanupLocations {
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

function valueAt<T>(values: readonly T[], index: number, label: string): T {
  const value = values[index];
  if (value === undefined) {
    throw new Error(`${label} was missing index ${index}: ${JSON.stringify(values)}`);
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

function itemConnection(operation: CapturedOperation, alias: string): JsonRecord {
  const data = asRecord(operation.response.data, `${alias}.data`);
  const item = asRecord(data['inventoryItem'], `${alias}.data.inventoryItem`);
  return asRecord(item[alias], `${alias}.data.inventoryItem.${alias}`);
}

function connectionNodes(operation: CapturedOperation, alias: string): JsonRecord[] {
  return asArray(itemConnection(operation, alias)['nodes'], `${alias}.nodes`).map((node, index) =>
    asRecord(node, `${alias}.nodes.${index}`),
  );
}

function connectionEdges(operation: CapturedOperation, alias: string): JsonRecord[] {
  return asArray(itemConnection(operation, alias)['edges'], `${alias}.edges`).map((edge, index) =>
    asRecord(edge, `${alias}.edges.${index}`),
  );
}

function nodeLocationId(node: JsonRecord, label: string): string {
  return stringValue(asRecord(node['location'], `${label}.location`)['id'], `${label}.location.id`);
}

function nodeQuantity(node: JsonRecord, name: string, label: string): number {
  const quantities = asArray(node['quantities'], `${label}.quantities`).map((row, index) =>
    asRecord(row, `${label}.quantities.${index}`),
  );
  const row = quantities.find((quantity) => quantity['name'] === name);
  const value = row?.['quantity'];
  if (typeof value !== 'number') {
    throw new Error(`${label}.quantities.${name} was not a number: ${JSON.stringify(row)}`);
  }
  return value;
}

function edgeNode(edge: JsonRecord, label: string): JsonRecord {
  return asRecord(edge['node'], `${label}.node`);
}

function searchWarnings(operation: CapturedOperation, alias: string): JsonRecord[] {
  const extensions = operation.response.extensions;
  if (typeof extensions !== 'object' || extensions === null || Array.isArray(extensions)) {
    return [];
  }
  const search = (extensions as JsonRecord)['search'];
  if (!Array.isArray(search)) {
    return [];
  }
  const entry = search
    .map((row) => asRecord(row, 'search entry'))
    .find((row) => {
      const pathValue = row['path'];
      return (
        Array.isArray(pathValue) && pathValue.length === 2 && pathValue[0] === 'inventoryItem' && pathValue[1] === alias
      );
    });
  if (!entry) {
    return [];
  }
  return asArray(entry['warnings'], `${alias}.search.warnings`).map((warning, index) =>
    asRecord(warning, `${alias}.search.warnings.${index}`),
  );
}

function assertNoSearchWarnings(operation: CapturedOperation, alias: string): void {
  const warnings = searchWarnings(operation, alias);
  if (warnings.length > 0) {
    throw new Error(`${alias} returned unexpected search warnings: ${JSON.stringify(warnings)}`);
  }
}

function assertSearchWarning(operation: CapturedOperation, alias: string, field: string, code: string): void {
  const warnings = searchWarnings(operation, alias);
  if (!warnings.some((warning) => warning['field'] === field && warning['code'] === code)) {
    throw new Error(`${alias} did not return ${field}/${code} warning: ${JSON.stringify(operation.response)}`);
  }
}

async function readRequest(key: keyof typeof requestPaths): Promise<string> {
  return readFile(requestPaths[key], 'utf8');
}

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
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
const sku = `INV-ITEM-LEVEL-${runId}`;

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

async function cleanupLocation(locationId: string, cleanupResult: JsonRecord): Promise<void> {
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
    cleanupResult[`relocationDestination:${locationId}`] = destinationLocationId;
  } catch (error) {
    cleanupResult[`relocationDestination:${locationId}`] = { error: String(error) };
  }
  try {
    cleanupResult[`locationDeactivate:${locationId}`] = (
      await runGraphqlRequest(locationDeactivateMutation, {
        locationId,
        destinationLocationId,
        idempotencyKey: `inventory-item-levels-cleanup-${resourceIdTail(locationId)}`,
      })
    ).payload;
  } catch (error) {
    cleanupResult[`locationDeactivate:${locationId}`] = { error: String(error) };
  }
  try {
    cleanupResult[`locationDelete:${locationId}`] = (
      await runGraphqlRequest(locationDeleteMutation, { locationId })
    ).payload;
  } catch (error) {
    cleanupResult[`locationDelete:${locationId}`] = { error: String(error) };
  }
}

async function cleanup(productId: string | null, locationIds: string[]): Promise<JsonRecord> {
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
  for (const locationId of locationIds) {
    await cleanupLocation(locationId, cleanupResult);
  }
  return cleanupResult;
}

let productId: string | null = null;
const locationIds: string[] = [];

try {
  const sourceLocationAdd = await runOperation(
    'locationAdd',
    {
      input: {
        name: `Inventory Item Levels Source ${runId}`,
        address: { countryCode: 'US' },
      },
    },
    'source locationAdd',
  );
  assertNoUserErrors(sourceLocationAdd.response, 'locationAdd');
  const sourceLocationId = stringValue(
    asRecord(operationData(sourceLocationAdd, 'locationAdd')['location'], 'source locationAdd.location')['id'],
    'source locationAdd.location.id',
  );
  locationIds.push(sourceLocationId);

  const destinationLocationAdd = await runOperation(
    'locationAdd',
    {
      input: {
        name: `Inventory Item Levels Destination ${runId}`,
        address: { countryCode: 'US' },
      },
    },
    'destination locationAdd',
  );
  assertNoUserErrors(destinationLocationAdd.response, 'locationAdd');
  const destinationLocationId = stringValue(
    asRecord(operationData(destinationLocationAdd, 'locationAdd')['location'], 'destination locationAdd.location')[
      'id'
    ],
    'destination locationAdd.location.id',
  );
  locationIds.push(destinationLocationId);

  const productSet = await runOperation(
    'productSet',
    {
      synchronous: true,
      input: {
        title: `Inventory item levels ${runId}`,
        status: 'ACTIVE',
        productOptions: [{ name: 'Title', position: 1, values: [{ name: 'Default Title' }] }],
        variants: [
          {
            optionValues: [{ optionName: 'Title', name: 'Default Title' }],
            price: '10.00',
            sku,
            inventoryItem: { tracked: true, requiresShipping: true },
            inventoryQuantities: [{ locationId: sourceLocationId, name: 'available', quantity: 0 }],
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
  const variant = variants.find((candidate) => candidate['sku'] === sku);
  if (!variant) {
    throw new Error(`Could not find setup variant: ${JSON.stringify(variants)}`);
  }
  const inventoryItemId = stringValue(
    asRecord(variant['inventoryItem'], 'variant.inventoryItem')['id'],
    'variant.inventoryItem.id',
  );

  const inventorySet = await runOperation(
    'inventorySet',
    {
      input: {
        name: 'available',
        reason: 'correction',
        referenceDocumentUri: `logistics://inventory-item-levels/set/${runId}`,
        quantities: [{ inventoryItemId, locationId: sourceLocationId, quantity: 4, changeFromQuantity: 0 }],
      },
      idempotencyKey: `inventory-item-levels-set-${runId}`,
    },
    'inventorySetQuantities',
  );
  assertNoUserErrors(inventorySet.response, 'inventorySetQuantities');

  const inventoryActivate = await runOperation(
    'inventoryActivate',
    {
      inventoryItemId,
      locationId: destinationLocationId,
      available: 9,
      idempotencyKey: `inventory-item-levels-activate-${runId}`,
    },
    'inventoryActivate',
  );
  assertNoUserErrors(inventoryActivate.response, 'inventoryActivate');

  const inventoryAdjust = await runOperation(
    'inventoryAdjust',
    {
      input: {
        name: 'available',
        reason: 'correction',
        referenceDocumentUri: `logistics://inventory-item-levels/adjust/${runId}`,
        changes: [{ inventoryItemId, locationId: destinationLocationId, delta: 1, changeFromQuantity: 9 }],
      },
      idempotencyKey: `inventory-item-levels-adjust-${runId}`,
    },
    'inventoryAdjustQuantities',
  );
  assertNoUserErrors(inventoryAdjust.response, 'inventoryAdjustQuantities');

  const activeRead = await runOperation(
    'connectionRead',
    {
      inventoryItemId,
      itemQuery: `inventory_item_id:${resourceIdTail(inventoryItemId)}`,
      missingItemQuery: 'inventory_item_id:1',
      invalidLocationQuery: `location_id:${resourceIdTail(destinationLocationId)}`,
    },
    'active inventoryItem.inventoryLevels read',
  );
  const activeEdges = connectionEdges(activeRead, 'allLevels');
  if (activeEdges.length !== 2) {
    throw new Error(`Expected two active item inventory levels: ${JSON.stringify(activeRead.response)}`);
  }
  assertNoSearchWarnings(activeRead, 'filtered');
  assertNoSearchWarnings(activeRead, 'emptyFilter');
  assertSearchWarning(activeRead, 'invalidLocationFilter', 'location_id', 'invalid_field');
  const activeFilteredNodes = connectionNodes(activeRead, 'filtered');
  if (activeFilteredNodes.length !== 2) {
    throw new Error(`Inventory item query did not return both item levels: ${JSON.stringify(activeRead.response)}`);
  }
  const activeEmptyNodes = connectionNodes(activeRead, 'emptyFilter');
  if (activeEmptyNodes.length !== 0) {
    throw new Error(`Mismatched inventory item query returned levels: ${JSON.stringify(activeRead.response)}`);
  }
  const activeInvalidLocationNodes = connectionNodes(activeRead, 'invalidLocationFilter');
  if (activeInvalidLocationNodes.length !== 2) {
    throw new Error(`Invalid location_id query narrowed item levels: ${JSON.stringify(activeRead.response)}`);
  }
  const activeDestinationNodes = activeFilteredNodes.filter(
    (node) => nodeLocationId(node, 'filtered node') === destinationLocationId,
  );
  if (activeDestinationNodes.length !== 1) {
    throw new Error(
      `Adjusted destination level did not read back exactly once: ${JSON.stringify(activeRead.response)}`,
    );
  }
  const activeDestinationNode = valueAt(activeDestinationNodes, 0, 'activeRead.filtered destination');
  if (nodeQuantity(activeDestinationNode, 'available', 'filtered destination') !== 10) {
    throw new Error(`Adjusted destination level did not read back as 10: ${JSON.stringify(activeRead.response)}`);
  }
  const firstEdge = valueAt(activeEdges, 0, 'activeRead.allLevels.edges');
  const secondEdge = valueAt(activeEdges, 1, 'activeRead.allLevels.edges');
  const firstCursor = stringValue(firstEdge['cursor'], 'activeRead.allLevels.edges.0.cursor');
  const secondCursor = stringValue(secondEdge['cursor'], 'activeRead.allLevels.edges.1.cursor');
  const firstNode = edgeNode(firstEdge, 'activeRead.allLevels.edges.0');
  const secondNode = edgeNode(secondEdge, 'activeRead.allLevels.edges.1');

  const firstLevelId = stringValue(firstNode['id'], 'activeRead.firstLevel.id');
  const inventoryDeactivate = await runOperation(
    'inventoryDeactivate',
    {
      inventoryLevelId: firstLevelId,
      idempotencyKey: `inventory-item-levels-deactivate-${runId}`,
    },
    'inventoryDeactivate',
  );
  assertNoUserErrors(inventoryDeactivate.response, 'inventoryDeactivate');

  const inactiveRead = await runOperation(
    'connectionRead',
    {
      inventoryItemId,
      itemQuery: `inventory_item_id:${resourceIdTail(inventoryItemId)}`,
      missingItemQuery: 'inventory_item_id:1',
      invalidLocationQuery: `location_id:${resourceIdTail(destinationLocationId)}`,
    },
    'inactive inventoryItem.inventoryLevels read',
  );
  assertNoSearchWarnings(inactiveRead, 'filtered');
  assertNoSearchWarnings(inactiveRead, 'emptyFilter');
  assertSearchWarning(inactiveRead, 'invalidLocationFilter', 'location_id', 'invalid_field');
  if (connectionNodes(inactiveRead, 'emptyFilter').length !== 0) {
    throw new Error(
      `Mismatched inventory item query returned inactive levels: ${JSON.stringify(inactiveRead.response)}`,
    );
  }
  if (connectionNodes(inactiveRead, 'invalidLocationFilter').length !== 2) {
    throw new Error(
      `Invalid location_id query narrowed inactive item levels: ${JSON.stringify(inactiveRead.response)}`,
    );
  }
  const inactiveActiveNodes = connectionEdges(inactiveRead, 'activeLevels').map((edge, index) =>
    edgeNode(edge, `inactiveRead.activeLevels.edges.${index}`),
  );
  if (inactiveActiveNodes.some((node) => node['isActive'] !== true)) {
    throw new Error(`Active item levels included an inactive node: ${JSON.stringify(inactiveRead.response)}`);
  }
  const inactiveAllNodes = connectionEdges(inactiveRead, 'allLevels').map((edge, index) =>
    edgeNode(edge, `inactiveRead.allLevels.edges.${index}`),
  );
  const inactiveFirstNode = inactiveAllNodes.find((node) => node['id'] === firstLevelId);
  if (!inactiveFirstNode || inactiveFirstNode['isActive'] !== false) {
    throw new Error(`Deactivated item level was not visible as inactive: ${JSON.stringify(inactiveRead.response)}`);
  }

  const windowRead = await runOperation(
    'windowRead',
    {
      inventoryItemId,
      after: firstCursor,
      before: secondCursor,
    },
    'inactive inventoryItem.inventoryLevels window read',
  );
  const afterNodes = connectionNodes(windowRead, 'afterFirst');
  const beforeNodes = connectionNodes(windowRead, 'beforeSecond');
  if (afterNodes.length !== 1) {
    throw new Error(`After cursor window did not return exactly one level: ${JSON.stringify(windowRead.response)}`);
  }
  const afterNode = valueAt(afterNodes, 0, 'windowRead.afterFirst.nodes');
  if (stringValue(afterNode['id'], 'afterFirst.nodes.0.id') !== stringValue(secondNode['id'], 'second node id')) {
    throw new Error(`After cursor window did not return the second level: ${JSON.stringify(windowRead.response)}`);
  }
  if (beforeNodes.length !== 1) {
    throw new Error(`Before cursor window did not return exactly one level: ${JSON.stringify(windowRead.response)}`);
  }
  const beforeNode = valueAt(beforeNodes, 0, 'windowRead.beforeSecond.nodes');
  if (stringValue(beforeNode['id'], 'beforeSecond.nodes.0.id') !== firstLevelId) {
    throw new Error(`Before cursor window did not return the first level: ${JSON.stringify(windowRead.response)}`);
  }

  const cleanupResult = await cleanup(productId, locationIds);
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
          sku,
          productId,
          inventoryItemId,
          sourceLocationId,
          destinationLocationId,
          firstConnectionLevelId: firstLevelId,
          secondConnectionLevelId: stringValue(secondNode['id'], 'second node id'),
        },
        operations: {
          sourceLocationAdd,
          destinationLocationAdd,
          productSet,
          inventorySet,
          inventoryActivate,
          inventoryAdjust,
          activeRead,
          inventoryDeactivate,
          inactiveRead,
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
  const cleanupResult = await cleanup(productId, locationIds);
  console.error(JSON.stringify({ error: String(error), cleanup: cleanupResult }, null, 2));
  process.exitCode = 1;
}

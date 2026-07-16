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

const requestDir = path.join('config', 'parity-requests', 'products');
const requestPaths = {
  locationAdd: path.join(requestDir, 'inventory-connection-location-add.graphql'),
  productSet: path.join(requestDir, 'inventory-connection-product-set.graphql'),
  itemUpdate: path.join(requestDir, 'inventory-connection-item-update.graphql'),
  itemsQuery: path.join(requestDir, 'inventory-connection-items-query.graphql'),
  transferCreate: path.join(requestDir, 'inventory-transfer-create.graphql'),
  transferCreateReady: path.join(requestDir, 'inventory-transfer-create-ready.graphql'),
  transfersWindow: path.join(requestDir, 'inventory-connection-transfers-window.graphql'),
  transfersPage: path.join(requestDir, 'inventory-connection-transfers-page.graphql'),
  transfersReverseStatus: path.join(requestDir, 'inventory-connection-transfers-reverse-status.graphql'),
} as const;

const productDeleteMutation = `#graphql
  mutation InventoryConnectionCleanupProductDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors { field message }
    }
  }
`;

const transferCancelMutation = `#graphql
  mutation InventoryConnectionCleanupTransferCancel($id: ID!) {
    inventoryTransferCancel(id: $id) {
      inventoryTransfer { id status }
      userErrors { field message code }
    }
  }
`;

const transferDeleteMutation = `#graphql
  mutation InventoryConnectionCleanupTransferDelete($id: ID!) {
    inventoryTransferDelete(id: $id) {
      deletedId
      userErrors { field message }
    }
  }
`;

const locationDeactivateMutation = `#graphql
  mutation InventoryConnectionCleanupLocationDeactivate($locationId: ID!, $destinationLocationId: ID) {
    locationDeactivate(locationId: $locationId, destinationLocationId: $destinationLocationId) {
      location { id isActive }
      locationDeactivateUserErrors { field message code }
    }
  }
`;

const locationDeleteMutation = `#graphql
  mutation InventoryConnectionCleanupLocationDelete($locationId: ID!) {
    locationDelete(locationId: $locationId) {
      deletedLocationId
      locationDeleteUserErrors { field message code }
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

function resourceTail(id: string): string {
  return id.split('/').at(-1)?.split('?')[0] ?? id;
}

function connectionEndCursor(payload: ConformanceGraphqlPayload<unknown>, root: string): string {
  const data = asRecord(payload.data, 'payload.data');
  const connection = asRecord(data[root], `payload.data.${root}`);
  const pageInfo = asRecord(connection['pageInfo'], `${root}.pageInfo`);
  return stringValue(pageInfo['endCursor'], `${root}.pageInfo.endCursor`);
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
const outputPath = path.join(outputDir, 'inventory-connection-query-windowing.json');
const runId = Date.now().toString();
const skuAlpha = `INV-CONN-${runId}-ALPHA`;
const skuBeta = `INV-CONN-${runId}-BETA`;
const transferTag = `icqw-${runId}`;
const runTimestamp = Number(runId);
const transferBaselineCreatedAt = isoSecond(runTimestamp);
const transferCreateCreatedAt = isoSecond(runTimestamp + 1_000);
const transferReadyCreatedAt = isoSecond(runTimestamp + 2_000);

function isoSecond(timestamp: number): string {
  return new Date(timestamp).toISOString().replace(/\.\d{3}Z$/u, 'Z');
}

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

function recordedAdminUpstreamCall(operationName: string, operation: CapturedOperation): JsonRecord {
  return {
    method: 'POST',
    apiSurface: 'admin',
    apiVersion,
    path: `/admin/api/${apiVersion}/graphql.json`,
    operationName,
    variables: operation.variables,
    query: operation.query,
    response: {
      status: 200,
      body: operation.response,
    },
  };
}

async function cleanup(
  productId: string | null,
  transferIds: string[],
  originLocationId: string | null,
  destinationLocationId: string | null,
): Promise<JsonRecord> {
  const cleanup: JsonRecord = {};
  for (const transferId of transferIds) {
    try {
      cleanup[`transferCancel:${transferId}`] = (
        await runGraphqlRequest(transferCancelMutation, { id: transferId })
      ).payload;
    } catch (error) {
      cleanup[`transferCancel:${transferId}`] = { error: String(error) };
    }
    try {
      cleanup[`transferDelete:${transferId}`] = (
        await runGraphqlRequest(transferDeleteMutation, { id: transferId })
      ).payload;
    } catch (error) {
      cleanup[`transferDelete:${transferId}`] = { error: String(error) };
    }
  }
  if (productId !== null) {
    try {
      cleanup['productDelete'] = (await runGraphqlRequest(productDeleteMutation, { input: { id: productId } })).payload;
    } catch (error) {
      cleanup['productDelete'] = { error: String(error) };
    }
  }
  if (originLocationId !== null) {
    try {
      cleanup['locationDeactivate'] = (
        await runGraphqlRequest(locationDeactivateMutation, {
          locationId: originLocationId,
          destinationLocationId,
        })
      ).payload;
    } catch (error) {
      cleanup['locationDeactivate'] = { error: String(error) };
    }
    try {
      cleanup['locationDelete'] = (
        await runGraphqlRequest(locationDeleteMutation, { locationId: originLocationId })
      ).payload;
    } catch (error) {
      cleanup['locationDelete'] = { error: String(error) };
    }
  }
  if (destinationLocationId !== null) {
    try {
      cleanup['destinationLocationDeactivate'] = (
        await runGraphqlRequest(locationDeactivateMutation, {
          locationId: destinationLocationId,
          destinationLocationId: null,
        })
      ).payload;
    } catch (error) {
      cleanup['destinationLocationDeactivate'] = { error: String(error) };
    }
    try {
      cleanup['destinationLocationDelete'] = (
        await runGraphqlRequest(locationDeleteMutation, { locationId: destinationLocationId })
      ).payload;
    } catch (error) {
      cleanup['destinationLocationDelete'] = { error: String(error) };
    }
  }
  return cleanup;
}

let productId: string | null = null;
let originLocationId: string | null = null;
let destinationLocationId: string | null = null;
const transferIds: string[] = [];

try {
  const originLocationAdd = await runOperation(
    'locationAdd',
    {
      input: {
        name: `Inventory Connection Origin ${runId}`,
        address: { countryCode: 'US' },
      },
    },
    'origin locationAdd',
  );
  assertNoUserErrors(originLocationAdd.response, 'locationAdd');
  originLocationId = stringValue(
    asRecord(
      asRecord(asRecord(originLocationAdd.response.data, 'originLocationAdd.data')['locationAdd'], 'locationAdd')[
        'location'
      ],
      'location',
    )['id'],
    'originLocationAdd.location.id',
  );

  const destinationLocationAdd = await runOperation(
    'locationAdd',
    {
      input: {
        name: `Inventory Connection Destination ${runId}`,
        address: { countryCode: 'US' },
      },
    },
    'destination locationAdd',
  );
  assertNoUserErrors(destinationLocationAdd.response, 'locationAdd');
  destinationLocationId = stringValue(
    asRecord(
      asRecord(
        asRecord(destinationLocationAdd.response.data, 'destinationLocationAdd.data')['locationAdd'],
        'locationAdd',
      )['location'],
      'location',
    )['id'],
    'destinationLocationAdd.location.id',
  );

  const productSet = await runOperation(
    'productSet',
    {
      synchronous: true,
      input: {
        title: `Inventory connection ${runId}`,
        status: 'ACTIVE',
        productOptions: [{ name: 'Title', position: 1, values: [{ name: 'Alpha' }, { name: 'Beta' }] }],
        variants: [
          {
            optionValues: [{ optionName: 'Title', name: 'Alpha' }],
            price: '10.00',
            sku: skuAlpha,
            inventoryItem: { tracked: true, requiresShipping: true },
            inventoryQuantities: [{ locationId: originLocationId, name: 'available', quantity: 6 }],
          },
          {
            optionValues: [{ optionName: 'Title', name: 'Beta' }],
            price: '11.00',
            sku: skuBeta,
            inventoryItem: { tracked: true, requiresShipping: true },
            inventoryQuantities: [{ locationId: originLocationId, name: 'available', quantity: 8 }],
          },
        ],
      },
    },
    'productSet',
  );
  assertNoUserErrors(productSet.response, 'productSet');
  const productSetData = asRecord(productSet.response.data, 'productSet.data');
  const product = asRecord(asRecord(productSetData['productSet'], 'productSet')['product'], 'productSet.product');
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

  const itemUpdate = await runOperation(
    'itemUpdate',
    { id: betaInventoryItemId, input: { tracked: false } },
    'inventoryItemUpdate',
  );
  assertNoUserErrors(itemUpdate.response, 'inventoryItemUpdate');

  const itemsQuery = await runOperation(
    'itemsQuery',
    {
      skuQuery: `sku:'${skuAlpha}'`,
      trackedFalseQuery: `tracked:false sku:'${skuBeta}'`,
      idRangeQuery: `id:>=${resourceTail(alphaInventoryItemId)} id:<=${resourceTail(betaInventoryItemId)}`,
      updatedBeforeQuery: `updated_at:<2000-01-01T00:00:00Z sku:'${skuAlpha}'`,
    },
    'inventoryItems query',
  );

  const baselineTransferCreate = await runOperation(
    'transferCreate',
    {
      input: {
        originLocationId,
        destinationLocationId,
        dateCreated: transferBaselineCreatedAt,
        tags: [transferTag, 'baseline'],
        lineItems: [{ inventoryItemId: alphaInventoryItemId, quantity: 1 }],
      },
    },
    'baseline inventoryTransferCreate',
  );
  assertNoUserErrors(baselineTransferCreate.response, 'inventoryTransferCreate');
  transferIds.push(
    stringValue(
      asRecord(
        asRecord(
          asRecord(baselineTransferCreate.response.data, 'baselineTransferCreate.data')['inventoryTransferCreate'],
          'payload',
        )['inventoryTransfer'],
        'inventoryTransfer',
      )['id'],
      'baselineInventoryTransferCreate.inventoryTransfer.id',
    ),
  );

  const transferOriginQuery = `origin_id:${resourceTail(originLocationId)}`;
  const transferAfterCreateProxyQuery = `date_created:>=${transferBaselineCreatedAt} date_created:<=${transferCreateCreatedAt}`;
  const transferPageProxyQuery = `date_created:>=${transferBaselineCreatedAt} date_created:<=${transferReadyCreatedAt}`;
  const transferStatusProxyQuery = `status:READY_TO_SHIP ${transferPageProxyQuery}`;
  const transfersColdWindow = await runOperation(
    'transfersWindow',
    { query: transferOriginQuery },
    'inventoryTransfers cold live-hybrid window',
  );

  const transferCreate = await runOperation(
    'transferCreate',
    {
      input: {
        originLocationId,
        destinationLocationId,
        dateCreated: transferCreateCreatedAt,
        tags: [transferTag, 'alpha'],
        lineItems: [{ inventoryItemId: alphaInventoryItemId, quantity: 2 }],
      },
    },
    'inventoryTransferCreate',
  );
  assertNoUserErrors(transferCreate.response, 'inventoryTransferCreate');
  transferIds.push(
    stringValue(
      asRecord(
        asRecord(asRecord(transferCreate.response.data, 'transferCreate.data')['inventoryTransferCreate'], 'payload')[
          'inventoryTransfer'
        ],
        'inventoryTransfer',
      )['id'],
      'inventoryTransferCreate.inventoryTransfer.id',
    ),
  );

  const transfersAfterCreateWindow = await runOperation(
    'transfersWindow',
    { query: transferOriginQuery },
    'inventoryTransfers after inventoryTransferCreate window',
  );

  const transferCreateReady = await runOperation(
    'transferCreateReady',
    {
      input: {
        originLocationId,
        destinationLocationId,
        dateCreated: transferReadyCreatedAt,
        tags: [transferTag, 'beta'],
        lineItems: [{ inventoryItemId: alphaInventoryItemId, quantity: 3 }],
      },
    },
    'inventoryTransferCreateAsReadyToShip',
  );
  assertNoUserErrors(transferCreateReady.response, 'inventoryTransferCreateAsReadyToShip');
  transferIds.push(
    stringValue(
      asRecord(
        asRecord(
          asRecord(transferCreateReady.response.data, 'transferCreateReady.data')[
            'inventoryTransferCreateAsReadyToShip'
          ],
          'payload',
        )['inventoryTransfer'],
        'inventoryTransfer',
      )['id'],
      'inventoryTransferCreateAsReadyToShip.inventoryTransfer.id',
    ),
  );

  const transferPageQuery = transferOriginQuery;
  const transfersPage1 = await runOperation(
    'transfersPage',
    { query: transferPageQuery, after: null },
    'inventoryTransfers page 1',
  );
  const transfersPage2 = await runOperation(
    'transfersPage',
    { query: transferPageQuery, after: connectionEndCursor(transfersPage1.response, 'inventoryTransfers') },
    'inventoryTransfers page 2',
  );
  const transfersReverseStatus = await runOperation(
    'transfersReverseStatus',
    {
      query: transferPageQuery,
      statusQuery: `status:READY_TO_SHIP ${transferOriginQuery}`,
    },
    'inventoryTransfers reverse/status',
  );

  const cleanupResult = await cleanup(productId, transferIds, originLocationId, destinationLocationId);
  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        scenario: 'inventory-connection-query-windowing',
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        setup: {
          skuAlpha,
          skuBeta,
          transferTag,
          productId,
          originLocationId,
          destinationLocationId,
          alphaInventoryItemId,
          betaInventoryItemId,
          alphaInventoryItemTail: resourceTail(alphaInventoryItemId),
          betaInventoryItemTail: resourceTail(betaInventoryItemId),
          transferBaselineCreatedAt,
          transferCreateCreatedAt,
          transferReadyCreatedAt,
          transferAfterCreateProxyQuery,
          transferPageProxyQuery,
          transferStatusProxyQuery,
        },
        operations: {
          originLocationAdd,
          destinationLocationAdd,
          productSet,
          itemUpdate,
          itemsQuery,
          baselineTransferCreate,
          transfersColdWindow,
          transferCreate,
          transfersAfterCreateWindow,
          transferCreateReady,
          transfersPage1,
          transfersPage2,
          transfersReverseStatus,
        },
        upstreamCalls: [recordedAdminUpstreamCall('InventoryConnectionTransfersWindow', transfersColdWindow)],
        cleanup: cleanupResult,
      },
      null,
      2,
    )}\n`,
  );
  console.log(`Wrote ${outputPath}`);
} catch (error) {
  const cleanupResult = await cleanup(productId, transferIds, originLocationId, destinationLocationId);
  console.error(JSON.stringify({ error: String(error), cleanup: cleanupResult }, null, 2));
  process.exitCode = 1;
}

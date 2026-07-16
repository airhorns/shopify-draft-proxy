/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

import { createAdminGraphqlClient, runStorefrontGraphqlRequest } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import {
  buildAdminAuthHeaders,
  buildStorefrontRequestHeaders,
  getStoredStorefrontAccessToken,
  getValidConformanceAccessToken,
} from './shopify-conformance-auth.mjs';

type Capture = {
  name: string;
  request: {
    query: string;
    variables: Record<string, unknown>;
  };
  status: number;
  response: unknown;
};

type GraphqlUpstreamCapture = {
  name: string;
  method: 'POST';
  apiSurface: 'admin' | 'storefront';
  apiVersion: string;
  path: string;
  endpoint: string;
  authMode: 'admin-access-token' | 'storefront-access-token';
  headers?: Record<string, string>;
  operationName: string;
  query: string;
  variables: Record<string, unknown>;
  response: {
    status: number;
    body: unknown;
  };
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const storedStorefrontAuth = await getStoredStorefrontAccessToken();
if (storedStorefrontAuth.shop && storedStorefrontAuth.shop !== storeDomain) {
  throw new Error(
    `Stored Storefront token is for ${storedStorefrontAuth.shop}, but SHOPIFY_CONFORMANCE_STORE_DOMAIN is ${storeDomain}. ` +
      'Run `corepack pnpm conformance:grant-storefront-token` for the target store.',
  );
}

const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const adminEndpoint = `${adminOrigin}/admin/api/${apiVersion}/graphql.json`;
const adminPath = `/admin/api/${apiVersion}/graphql.json`;
const storefrontEndpoint = `https://${storeDomain}/api/${apiVersion}/graphql.json`;
const storefrontPath = `/api/${apiVersion}/graphql.json`;
const storefrontOptions = {
  storeOrigin: `https://${storeDomain}`,
  apiVersion,
  storefrontAccessToken: storedStorefrontAuth.storefront_access_token,
};
const storefrontRedactedHeaders = Object.fromEntries(
  Object.keys(buildStorefrontRequestHeaders(storedStorefrontAuth.storefront_access_token)).map((name) => [
    name,
    '<redacted:storefront-access-token>',
  ]),
);

const documents = {
  productCreate: 'config/parity-requests/storefront/storefront-catalog-product-create-admin.graphql',
  variantUpdate: 'config/parity-requests/storefront/storefront-catalog-variant-update-admin.graphql',
  locationAdd: 'config/parity-requests/storefront/storefront-catalog-location-add-admin.graphql',
  stockLocationHydrate: 'config/parity-requests/storefront/storefront-catalog-stock-location-hydrate-admin.graphql',
  inventorySet: 'config/parity-requests/storefront/storefront-catalog-inventory-set-admin.graphql',
  publicationHydrate: 'config/parity-requests/storefront/storefront-catalog-publications-hydrate-admin.graphql',
  publishStorefront: 'config/parity-requests/storefront/storefront-catalog-publish-admin.graphql',
  productUpdate: 'config/parity-requests/storefront/storefront-catalog-product-update-admin.graphql',
  unpublishStorefront: 'config/parity-requests/storefront/storefront-catalog-unpublish-admin.graphql',
  productDelete: 'config/parity-requests/storefront/storefront-catalog-product-delete-admin.graphql',
  storefrontRead: 'config/parity-requests/storefront/storefront-catalog-read-after-admin-setup.graphql',
  storefrontUpdatedRead: 'config/parity-requests/storefront/storefront-catalog-updated-read.graphql',
  storefrontHiddenRead: 'config/parity-requests/storefront/storefront-catalog-hidden-read.graphql',
} as const;

const documentText = Object.fromEntries(
  await Promise.all(
    Object.entries(documents).map(async ([key, documentPath]) => [key, await readFile(documentPath, 'utf8')]),
  ),
) as Record<keyof typeof documents, string>;

const suffix = new Date().toISOString().replace(/\D/gu, '').slice(0, 14);
const tag = `storefront-catalog-${suffix}`;
const initialHandle = `storefront-catalog-product-${suffix}`;
const updatedHandle = `storefront-catalog-updated-${suffix}`;
const productCreateVariables = {
  product: {
    title: `Storefront Catalog Product ${suffix}`,
    handle: initialHandle,
    status: 'ACTIVE',
    vendor: 'Hermes',
    productType: 'Catalog Fixture',
    tags: ['storefront-catalog', tag],
    descriptionHtml: `<p>Storefront catalog body ${suffix}</p>`,
    seo: {
      title: `Storefront Catalog SEO ${suffix}`,
      description: `Storefront catalog SEO description ${suffix}`,
    },
    productOptions: [{ name: 'Color', values: [{ name: 'Red' }] }],
  },
} satisfies Record<string, unknown>;
const variantUpdateInput = {
  barcode: `sfc-barcode-${suffix}`,
  price: '14.25',
  compareAtPrice: '18.00',
  inventoryItem: {
    sku: `SFC-${suffix}`,
    tracked: true,
    requiresShipping: false,
  },
} satisfies Record<string, unknown>;
const locationAddVariables = {
  input: {
    name: `Storefront Catalog ${suffix}`,
    address: { countryCode: 'US' },
  },
} satisfies Record<string, unknown>;
const selectedOptions = [{ name: 'Color', value: 'Red' }];

const adminCaptures: Capture[] = [];
const cleanupCaptures: Capture[] = [];
let currentPublicationHydrateCapture: GraphqlUpstreamCapture | null = null;
let stockLocationHydrateCapture: GraphqlUpstreamCapture | null = null;
let storefrontReadCapture: GraphqlUpstreamCapture | null = null;
let storefrontUpdatedReadCapture: GraphqlUpstreamCapture | null = null;
let storefrontUnpublishedReadCapture: GraphqlUpstreamCapture | null = null;
let storefrontDeletedReadCapture: GraphqlUpstreamCapture | null = null;
let productId: string | null = null;
let locationId: string | null = null;
let productDeleted = false;

async function captureAdmin(name: string, query: string, variables: Record<string, unknown>): Promise<unknown> {
  const result = await runGraphqlRaw(query, variables);
  const capture = {
    name,
    request: { query, variables },
    status: result.status,
    response: result.payload,
  };
  adminCaptures.push(capture);
  return result.payload;
}

async function captureAdminCleanup(name: string, query: string, variables: Record<string, unknown>): Promise<void> {
  const result = await runGraphqlRaw(query, variables);
  cleanupCaptures.push({
    name,
    request: { query, variables },
    status: result.status,
    response: result.payload,
  });
}

async function captureAdminUpstream(
  name: string,
  operationName: string,
  query: string,
  variables: Record<string, unknown>,
): Promise<GraphqlUpstreamCapture> {
  const result = await runGraphqlRaw(query, variables);
  return {
    name,
    method: 'POST',
    apiSurface: 'admin',
    apiVersion,
    path: adminPath,
    endpoint: adminEndpoint,
    authMode: 'admin-access-token',
    operationName,
    query,
    variables,
    response: {
      status: result.status,
      body: result.payload,
    },
  };
}

async function storefrontRequest(
  name: string,
  operationName: string,
  query: string,
  variables: Record<string, unknown>,
): Promise<GraphqlUpstreamCapture> {
  const result = await runStorefrontGraphqlRequest(storefrontOptions, query, variables);
  return {
    name,
    method: 'POST',
    apiSurface: 'storefront',
    apiVersion,
    path: storefrontPath,
    endpoint: storefrontEndpoint,
    authMode: 'storefront-access-token',
    headers: storefrontRedactedHeaders,
    operationName,
    query,
    variables,
    response: {
      status: result.status,
      body: result.payload,
    },
  };
}

function readPath(value: unknown, pathSegments: string[]): unknown {
  return pathSegments.reduce<unknown>((current, segment) => {
    if (typeof current !== 'object' || current === null) return null;
    return (current as Record<string, unknown>)[segment] ?? null;
  }, value);
}

function readRequiredString(value: unknown, pathSegments: string[], label: string): string {
  const result = readPath(value, pathSegments);
  if (typeof result !== 'string' || result.length === 0) {
    throw new Error(`${label} did not return a string at ${pathSegments.join('.')}: ${JSON.stringify(value)}`);
  }
  return result;
}

function readRequiredPublicationIdByName(payload: unknown, name: string): string {
  const nodes = readPath(payload, ['data', 'publications', 'nodes']);
  if (!Array.isArray(nodes)) {
    throw new Error(`publication hydrate did not return publications.nodes: ${JSON.stringify(payload)}`);
  }
  const match = nodes.find((node) => {
    return typeof node === 'object' && node !== null && (node as Record<string, unknown>)['name'] === name;
  });
  if (typeof match !== 'object' || match === null) {
    throw new Error(`publication hydrate did not include ${name}: ${JSON.stringify(nodes, null, 2)}`);
  }
  const id = (match as Record<string, unknown>)['id'];
  if (typeof id !== 'string' || id.length === 0) {
    throw new Error(`publication hydrate ${name} entry did not include an id: ${JSON.stringify(match)}`);
  }
  return id;
}

function readRequiredStockLocationId(payload: unknown): string {
  const nodes = readPath(payload, ['data', 'locations', 'nodes']);
  if (!Array.isArray(nodes)) {
    throw new Error(`stock location hydrate did not return locations.nodes: ${JSON.stringify(payload)}`);
  }
  const match = nodes.find((node) => {
    if (typeof node !== 'object' || node === null) return false;
    const record = node as Record<string, unknown>;
    return record['isActive'] === true && record['fulfillsOnlineOrders'] === true && record['shipsInventory'] === true;
  });
  if (typeof match !== 'object' || match === null) {
    throw new Error(
      `stock location hydrate did not include an active shipping location: ${JSON.stringify(nodes, null, 2)}`,
    );
  }
  const id = (match as Record<string, unknown>)['id'];
  if (typeof id !== 'string' || id.length === 0) {
    throw new Error(`stock location entry did not include an id: ${JSON.stringify(match)}`);
  }
  return id;
}

function assertNoTopLevelErrors(payload: unknown, label: string): void {
  const errors = readPath(payload, ['errors']);
  if (Array.isArray(errors) && errors.length > 0) {
    throw new Error(`${label} returned top-level errors: ${JSON.stringify(errors, null, 2)}`);
  }
}

function assertNoUserErrors(payload: unknown, pathSegments: string[], label: string): void {
  const userErrors = readPath(payload, pathSegments);
  if (Array.isArray(userErrors) && userErrors.length === 0) return;
  throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
}

function visibleStorefrontCatalog(payload: unknown, handle: string): boolean {
  return (
    readPath(payload, ['data', 'byId', 'handle']) === handle &&
    readPath(payload, ['data', 'byHandle', 'handle']) === handle &&
    readPath(payload, ['data', 'products', 'edges', '0', 'node', 'handle']) === handle &&
    readPath(payload, ['data', 'byId', 'variants', 'edges', '0', 'node', 'quantityAvailable']) === 5
  );
}

function updatedStorefrontCatalog(payload: unknown, oldHandle: string, newHandle: string): boolean {
  return (
    readPath(payload, ['data', 'oldHandle']) === null &&
    readPath(payload, ['data', 'byId', 'handle']) === newHandle &&
    readPath(payload, ['data', 'byNewHandle', 'handle']) === newHandle &&
    readPath(payload, ['data', 'products', 'nodes', '0', 'handle']) === newHandle &&
    oldHandle !== newHandle
  );
}

function hiddenStorefrontCatalog(payload: unknown): boolean {
  const nodes = readPath(payload, ['data', 'products', 'nodes']);
  return (
    readPath(payload, ['data', 'byId']) === null &&
    readPath(payload, ['data', 'byHandle']) === null &&
    Array.isArray(nodes) &&
    nodes.length === 0
  );
}

async function waitForVisibleCatalog(variables: Record<string, unknown>): Promise<GraphqlUpstreamCapture> {
  const handle = variables['handle'];
  if (typeof handle !== 'string') throw new Error('visible Storefront variables must include a string handle');
  let lastCapture: GraphqlUpstreamCapture | null = null;
  for (let attempt = 1; attempt <= 45; attempt += 1) {
    lastCapture = await storefrontRequest(
      'storefront-catalog-read-after-admin-setup',
      'StorefrontCatalogReadAfterAdminSetup',
      documentText.storefrontRead,
      variables,
    );
    assertNoTopLevelErrors(lastCapture.response.body, `storefront catalog visible read attempt ${attempt}`);
    if (visibleStorefrontCatalog(lastCapture.response.body, handle)) return lastCapture;
    await delay(2000);
  }
  throw new Error(`Storefront catalog did not become visible: ${JSON.stringify(lastCapture?.response.body, null, 2)}`);
}

async function waitForUpdatedCatalog(variables: Record<string, unknown>): Promise<GraphqlUpstreamCapture> {
  const oldHandle = variables['oldHandle'];
  const newHandle = variables['newHandle'];
  if (typeof oldHandle !== 'string' || typeof newHandle !== 'string') {
    throw new Error('updated Storefront variables must include string handles');
  }
  let lastCapture: GraphqlUpstreamCapture | null = null;
  for (let attempt = 1; attempt <= 45; attempt += 1) {
    lastCapture = await storefrontRequest(
      'storefront-catalog-updated-read',
      'StorefrontCatalogUpdatedRead',
      documentText.storefrontUpdatedRead,
      variables,
    );
    assertNoTopLevelErrors(lastCapture.response.body, `storefront catalog updated read attempt ${attempt}`);
    if (updatedStorefrontCatalog(lastCapture.response.body, oldHandle, newHandle)) return lastCapture;
    await delay(2000);
  }
  throw new Error(
    `Storefront catalog update did not become visible: ${JSON.stringify(lastCapture?.response.body, null, 2)}`,
  );
}

async function waitForHiddenCatalog(name: string, variables: Record<string, unknown>): Promise<GraphqlUpstreamCapture> {
  let lastCapture: GraphqlUpstreamCapture | null = null;
  for (let attempt = 1; attempt <= 45; attempt += 1) {
    lastCapture = await storefrontRequest(
      name,
      'StorefrontCatalogHiddenRead',
      documentText.storefrontHiddenRead,
      variables,
    );
    assertNoTopLevelErrors(lastCapture.response.body, `${name} attempt ${attempt}`);
    if (hiddenStorefrontCatalog(lastCapture.response.body)) return lastCapture;
    await delay(2000);
  }
  throw new Error(`${name} did not become hidden: ${JSON.stringify(lastCapture?.response.body, null, 2)}`);
}

const locationDeactivateMutation = `#graphql
  mutation StorefrontCatalogLocationDeactivateCleanup($locationId: ID!, $idempotencyKey: String!) {
    locationDeactivate(locationId: $locationId) @idempotent(key: $idempotencyKey) {
      location { id isActive }
      locationDeactivateUserErrors { field message code }
    }
  }
`;

const locationDeleteMutation = `#graphql
  mutation StorefrontCatalogLocationDeleteCleanup($locationId: ID!) {
    locationDelete(locationId: $locationId) {
      deletedLocationId
      locationDeleteUserErrors { field message code }
    }
  }
`;

try {
  const adminProductCreate = await captureAdmin(
    'admin-product-create',
    documentText.productCreate,
    productCreateVariables,
  );
  assertNoTopLevelErrors(adminProductCreate, 'admin productCreate');
  assertNoUserErrors(adminProductCreate, ['data', 'productCreate', 'userErrors'], 'productCreate');
  productId = readRequiredString(adminProductCreate, ['data', 'productCreate', 'product', 'id'], 'product id');
  const variantId = readRequiredString(
    adminProductCreate,
    ['data', 'productCreate', 'product', 'variants', 'nodes', '0', 'id'],
    'default variant id',
  );

  const adminVariantUpdate = await captureAdmin('admin-variant-update', documentText.variantUpdate, {
    productId,
    variants: [{ id: variantId, ...variantUpdateInput }],
  });
  assertNoTopLevelErrors(adminVariantUpdate, 'admin productVariantsBulkUpdate');
  assertNoUserErrors(adminVariantUpdate, ['data', 'updateVariant', 'userErrors'], 'productVariantsBulkUpdate');
  const inventoryItemId = readRequiredString(
    adminVariantUpdate,
    ['data', 'updateVariant', 'productVariants', '0', 'inventoryItem', 'id'],
    'variant inventory item id',
  );

  const adminLocationAdd = await captureAdmin('admin-location-add', documentText.locationAdd, locationAddVariables);
  assertNoTopLevelErrors(adminLocationAdd, 'admin locationAdd');
  assertNoUserErrors(adminLocationAdd, ['data', 'locationAdd', 'userErrors'], 'locationAdd');
  locationId = readRequiredString(adminLocationAdd, ['data', 'locationAdd', 'location', 'id'], 'location id');

  stockLocationHydrateCapture = await captureAdminUpstream(
    'admin-stock-location-hydrate',
    'StorefrontCatalogStockLocationHydrate',
    documentText.stockLocationHydrate,
    {},
  );
  assertNoTopLevelErrors(stockLocationHydrateCapture.response.body, 'admin stock location hydrate');
  const stockLocationId = readRequiredStockLocationId(stockLocationHydrateCapture.response.body);

  const adminInventorySet = await captureAdmin('admin-inventory-set', documentText.inventorySet, {
    input: {
      name: 'available',
      reason: 'correction',
      referenceDocumentUri: `logistics://storefront-catalog/${suffix}`,
      quantities: [{ inventoryItemId, locationId: stockLocationId, quantity: 5, changeFromQuantity: 0 }],
    },
    idempotencyKey: `storefront-catalog-inventory-set-${suffix}`,
  });
  assertNoTopLevelErrors(adminInventorySet, 'admin inventorySetQuantities');
  assertNoUserErrors(adminInventorySet, ['data', 'inventorySetQuantities', 'userErrors'], 'inventorySetQuantities');

  currentPublicationHydrateCapture = await captureAdminUpstream(
    'admin-storefront-publications-hydrate',
    'StorePropertiesPublishableInputValidationHydrate',
    documentText.publicationHydrate,
    {},
  );
  assertNoTopLevelErrors(currentPublicationHydrateCapture.response.body, 'admin publication hydrate');
  const storefrontPublicationId = readRequiredPublicationIdByName(
    currentPublicationHydrateCapture.response.body,
    'Online Store',
  );

  const adminPublishStorefront = await captureAdmin('admin-publish-storefront', documentText.publishStorefront, {
    id: productId,
    input: [{ publicationId: storefrontPublicationId }],
    publicationId: storefrontPublicationId,
  });
  assertNoTopLevelErrors(adminPublishStorefront, 'admin publishablePublish');
  assertNoUserErrors(adminPublishStorefront, ['data', 'publishablePublish', 'userErrors'], 'publishablePublish');

  const storefrontVariables = {
    id: productId,
    handle: initialHandle,
    query: `tag:${tag}`,
    selectedOptions,
  };
  storefrontReadCapture = await waitForVisibleCatalog(storefrontVariables);

  const adminProductUpdate = await captureAdmin('admin-product-update', documentText.productUpdate, {
    product: {
      id: productId,
      title: `Storefront Catalog Updated ${suffix}`,
      handle: updatedHandle,
    },
  });
  assertNoTopLevelErrors(adminProductUpdate, 'admin productUpdate');
  assertNoUserErrors(adminProductUpdate, ['data', 'productUpdate', 'userErrors'], 'productUpdate');

  storefrontUpdatedReadCapture = await waitForUpdatedCatalog({
    id: productId,
    oldHandle: initialHandle,
    newHandle: updatedHandle,
    query: `tag:${tag}`,
  });

  const adminUnpublishStorefront = await captureAdmin('admin-unpublish-storefront', documentText.unpublishStorefront, {
    id: productId,
    input: [{ publicationId: storefrontPublicationId }],
    publicationId: storefrontPublicationId,
  });
  assertNoTopLevelErrors(adminUnpublishStorefront, 'admin publishableUnpublish');
  assertNoUserErrors(adminUnpublishStorefront, ['data', 'publishableUnpublish', 'userErrors'], 'publishableUnpublish');

  const hiddenVariables = {
    id: productId,
    handle: updatedHandle,
    query: `tag:${tag}`,
  };
  storefrontUnpublishedReadCapture = await waitForHiddenCatalog('storefront-catalog-unpublished-read', hiddenVariables);

  const adminProductDelete = await captureAdmin('admin-product-delete', documentText.productDelete, {
    input: { id: productId },
  });
  assertNoTopLevelErrors(adminProductDelete, 'admin productDelete');
  assertNoUserErrors(adminProductDelete, ['data', 'productDelete', 'userErrors'], 'productDelete');
  productDeleted = true;
  storefrontDeletedReadCapture = await waitForHiddenCatalog('storefront-catalog-deleted-read', hiddenVariables);
} finally {
  if (!productDeleted && productId !== null) {
    await captureAdminCleanup('productDelete-cleanup', documentText.productDelete, { input: { id: productId } });
  }
  if (locationId !== null) {
    await captureAdminCleanup('locationDeactivate-cleanup', locationDeactivateMutation, {
      locationId,
      idempotencyKey: `storefront-catalog-location-deactivate-${suffix}`,
    });
    await captureAdminCleanup('locationDelete-cleanup', locationDeleteMutation, { locationId });
  }
}

if (
  currentPublicationHydrateCapture === null ||
  stockLocationHydrateCapture === null ||
  storefrontReadCapture === null ||
  storefrontUpdatedReadCapture === null ||
  storefrontUnpublishedReadCapture === null ||
  storefrontDeletedReadCapture === null
) {
  throw new Error('Storefront catalog capture did not complete.');
}

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'storefront');
await mkdir(outputDir, { recursive: true });
const outputPath = path.join(outputDir, 'storefront-catalog-read-after-admin-setup.json');
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      scenarioId: 'storefront-catalog-read-after-admin-setup',
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      apiSurface: 'storefront',
      endpoint: storefrontEndpoint,
      authMode: 'storefront-access-token',
      storefrontToken: {
        id: storedStorefrontAuth.storefront_token_id || '<unknown>',
        title: storedStorefrontAuth.storefront_token_title || '<unknown>',
        accessScopes: storedStorefrontAuth.storefront_access_scopes,
        obtainedAt: storedStorefrontAuth.obtained_at || '<unknown>',
      },
      adminProductCreate: adminCaptures.find((capture) => capture.name === 'admin-product-create'),
      adminVariantUpdate: adminCaptures.find((capture) => capture.name === 'admin-variant-update'),
      adminLocationAdd: adminCaptures.find((capture) => capture.name === 'admin-location-add'),
      adminStockLocationHydrate: stockLocationHydrateCapture,
      adminInventorySet: adminCaptures.find((capture) => capture.name === 'admin-inventory-set'),
      adminPublicationHydrate: currentPublicationHydrateCapture,
      adminPublishStorefront: adminCaptures.find((capture) => capture.name === 'admin-publish-storefront'),
      storefrontRead: storefrontReadCapture,
      adminProductUpdate: adminCaptures.find((capture) => capture.name === 'admin-product-update'),
      storefrontUpdatedRead: storefrontUpdatedReadCapture,
      adminUnpublishStorefront: adminCaptures.find((capture) => capture.name === 'admin-unpublish-storefront'),
      storefrontUnpublishedRead: storefrontUnpublishedReadCapture,
      adminProductDelete: adminCaptures.find((capture) => capture.name === 'admin-product-delete'),
      storefrontDeletedRead: storefrontDeletedReadCapture,
      cleanup: cleanupCaptures,
      upstreamCalls: [currentPublicationHydrateCapture],
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(`Wrote ${outputPath}`);
console.log(`Captured authenticated Storefront catalog status ${storefrontReadCapture.response.status}`);

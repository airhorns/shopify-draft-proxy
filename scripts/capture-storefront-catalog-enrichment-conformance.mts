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

type JsonRecord = Record<string, unknown>;

type Capture = {
  name: string;
  request: {
    query: string;
    variables: JsonRecord;
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
  variables: JsonRecord;
  response: {
    status: number;
    body: unknown;
  };
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
if (apiVersion !== '2026-04') {
  throw new Error(`Storefront catalog enrichment capture requires API version 2026-04, received ${apiVersion}.`);
}

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const storedStorefrontAuth = await getStoredStorefrontAccessToken();
if (storedStorefrontAuth.shop && storedStorefrontAuth.shop !== storeDomain) {
  throw new Error(
    `Stored Storefront token is for ${storedStorefrontAuth.shop}, but the configured store is ${storeDomain}.`,
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
const sensitiveRedaction = '<redacted:storefront-buyer-context>';

function redactSensitive(value: unknown, key?: string): unknown {
  if (
    typeof value === 'string' &&
    key !== undefined &&
    ['accessToken', 'customerAccessToken', 'deletedAccessToken', 'password', 'token'].includes(key)
  ) {
    return sensitiveRedaction;
  }
  if (Array.isArray(value)) return value.map((entry) => redactSensitive(entry));
  if (typeof value === 'object' && value !== null) {
    return Object.fromEntries(
      Object.entries(value).map(([childKey, child]) => [childKey, redactSensitive(child, childKey)]),
    );
  }
  return value;
}

const documents = {
  productCreate: 'config/parity-requests/storefront/storefront-enrichment-product-create-admin.graphql',
  variantUpdate: 'config/parity-requests/storefront/storefront-catalog-variant-update-admin.graphql',
  locationAdd: 'config/parity-requests/storefront/storefront-catalog-location-add-admin.graphql',
  inventoryActivate: 'config/parity-requests/products/inventory-item-levels-activate.graphql',
  publicationHydrate: 'config/parity-requests/storefront/storefront-catalog-publications-hydrate-admin.graphql',
  publish: 'config/parity-requests/storefront/storefront-catalog-publish-admin.graphql',
  productDelete: 'config/parity-requests/storefront/storefront-catalog-product-delete-admin.graphql',
  definitionCreate: 'config/parity-requests/storefront/storefront-enrichment-metafield-definition-create-admin.graphql',
  metafieldsSet: 'config/parity-requests/storefront/storefront-enrichment-metafields-set-admin.graphql',
  sellingPlanCreate: 'config/parity-requests/storefront/storefront-enrichment-selling-plan-group-create-admin.graphql',
  marketCreate: 'config/parity-requests/storefront/storefront-enrichment-market-create-admin.graphql',
  priceListCreate: 'config/parity-requests/storefront/storefront-enrichment-price-list-create-admin.graphql',
  catalogCreate: 'config/parity-requests/storefront/storefront-enrichment-catalog-create-admin.graphql',
  quantityPricing: 'config/parity-requests/storefront/storefront-enrichment-quantity-pricing-admin.graphql',
  taxonomyHydrate: 'config/parity-requests/storefront/storefront-enrichment-taxonomy-hydrate.graphql',
  merchandisingRead: 'config/parity-requests/storefront/storefront-enrichment-merchandising-read.graphql',
  contextHydrate: 'config/parity-requests/storefront/storefront-enrichment-context-hydrate.graphql',
  contextRead: 'config/parity-requests/storefront/storefront-enrichment-context-read.graphql',
  customerCreate: 'config/parity-requests/storefront/storefront-customer-auth-create.graphql',
  customerTokenCreate: 'config/parity-requests/storefront/storefront-customer-auth-token-create.graphql',
  customerDelete: 'config/parity-requests/storefront/storefront-customer-auth-admin-delete.graphql',
  companyCreate: 'config/parity-requests/b2b/b2b-contact-business-rules-company-create.graphql',
  companyAssignCustomer: 'config/parity-requests/b2b/assign-customer-as-contact-error-branch-assign.graphql',
  companyAssignRole: 'config/parity-requests/b2b/b2b-contact-business-rules-assign-role.graphql',
  companyDelete: 'config/parity-requests/b2b/company-delete-check-company-delete.graphql',
} as const;

const documentText = Object.fromEntries(
  await Promise.all(
    Object.entries(documents).map(async ([key, documentPath]) => [key, await readFile(documentPath, 'utf8')]),
  ),
) as Record<keyof typeof documents, string>;

function readPath(value: unknown, segments: string[]): unknown {
  return segments.reduce<unknown>((current, segment) => {
    if (typeof current !== 'object' || current === null) return undefined;
    return (current as JsonRecord)[segment];
  }, value);
}

function requireString(value: unknown, segments: string[], label: string): string {
  const result = readPath(value, segments);
  if (typeof result !== 'string' || result.length === 0) {
    throw new Error(`${label} did not return a string at ${segments.join('.')}: ${JSON.stringify(value, null, 2)}`);
  }
  return result;
}

function assertNoTopLevelErrors(value: unknown, label: string): void {
  const errors = readPath(value, ['errors']);
  if (Array.isArray(errors) && errors.length > 0) {
    throw new Error(`${label} returned top-level errors: ${JSON.stringify(errors, null, 2)}`);
  }
}

function assertNoUserErrors(value: unknown, segments: string[], label: string): void {
  const userErrors = readPath(value, segments);
  if (Array.isArray(userErrors) && userErrors.length === 0) return;
  throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
}

function connectionNodes(value: unknown): unknown[] {
  if (typeof value !== 'object' || value === null) return [];
  const nodes = (value as JsonRecord)['nodes'];
  if (Array.isArray(nodes)) return nodes;
  const edges = (value as JsonRecord)['edges'];
  return Array.isArray(edges)
    ? edges.flatMap((edge) => {
        if (typeof edge !== 'object' || edge === null) return [];
        return [(edge as JsonRecord)['node']];
      })
    : [];
}

async function captureAdmin(name: string, query: string, variables: JsonRecord): Promise<Capture> {
  const result = await runGraphqlRaw(query, variables);
  return {
    name,
    request: { query, variables },
    status: result.status,
    response: result.payload,
  };
}

async function captureAdminUpstream(
  name: string,
  operationName: string,
  query: string,
  variables: JsonRecord,
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
    response: { status: result.status, body: result.payload },
  };
}

async function captureStorefront(
  name: string,
  operationName: string,
  query: string,
  variables: JsonRecord,
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
    response: { status: result.status, body: result.payload },
  };
}

async function captureSensitiveStorefront(
  name: string,
  operationName: string,
  query: string,
  variables: JsonRecord,
): Promise<{ capture: GraphqlUpstreamCapture; rawBody: unknown }> {
  const result = await runStorefrontGraphqlRequest(storefrontOptions, query, variables);
  return {
    capture: {
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
      variables: redactSensitive(variables) as JsonRecord,
      response: { status: result.status, body: redactSensitive(result.payload) },
    },
    rawBody: result.payload,
  };
}

async function bestEffortAdminCleanup(name: string, query: string, variables: JsonRecord): Promise<Capture> {
  return captureAdmin(name, query, variables);
}

function merchandisingReady(
  body: unknown,
  expected: {
    id: string;
    handle: string;
    visibleProductKey: string;
    visibleProductValue: string;
    hiddenProductKey: string;
    visibleVariantKey: string;
    visibleVariantValue: string;
  },
): boolean {
  const product = readPath(body, ['data', 'aliasedProduct']);
  if (typeof product !== 'object' || product === null) return false;
  const record = product as JsonRecord;
  if (record['id'] !== expected.id || record['handle'] !== expected.handle) return false;
  if (readPath(record, ['visibleProductMetafield', 'key']) !== expected.visibleProductKey) return false;
  if (readPath(record, ['visibleProductMetafield', 'value']) !== expected.visibleProductValue) return false;
  if (readPath(record, ['hiddenProductMetafield']) !== null) return false;
  const selectedMetafields = record['selectedProductMetafields'];
  if (!Array.isArray(selectedMetafields) || selectedMetafields.length !== 2 || selectedMetafields[1] !== null) {
    return false;
  }
  const mediaNodes = connectionNodes(record['media']);
  if (mediaNodes.length === 0 || readPath(mediaNodes[0], ['previewImage', 'url']) === undefined) return false;
  if (connectionNodes(record['sellingPlanGroups']).length === 0) return false;
  const variants = connectionNodes(record['variants']);
  const variant = variants[0];
  if (typeof variant !== 'object' || variant === null) return false;
  if (readPath(variant, ['visibleVariantMetafield', 'key']) !== expected.visibleVariantKey) return false;
  if (readPath(variant, ['visibleVariantMetafield', 'value']) !== expected.visibleVariantValue) return false;
  return connectionNodes((variant as JsonRecord)['sellingPlanAllocations']).length > 0;
}

async function waitForMerchandising(
  variables: JsonRecord,
  expected: Parameters<typeof merchandisingReady>[1],
): Promise<GraphqlUpstreamCapture> {
  let lastCapture: GraphqlUpstreamCapture | null = null;
  for (let attempt = 1; attempt <= 60; attempt += 1) {
    lastCapture = await captureStorefront(
      'storefront-enrichment-merchandising-read',
      'StorefrontEnrichmentMerchandising',
      documentText.merchandisingRead,
      variables,
    );
    assertNoTopLevelErrors(lastCapture.response.body, `merchandising read attempt ${attempt}`);
    if (merchandisingReady(lastCapture.response.body, expected)) return lastCapture;
    await delay(2000);
  }
  throw new Error(
    `Storefront merchandising state did not converge: ${JSON.stringify(lastCapture?.response.body, null, 2)}`,
  );
}

function contextCurrency(body: unknown): string | undefined {
  return readPath(body, ['data', 'product', 'variants', 'nodes', '0', 'price', 'currencyCode']) as string | undefined;
}

async function waitForContext(
  name: string,
  variables: JsonRecord,
  expectedCurrency: string,
): Promise<GraphqlUpstreamCapture> {
  let lastCapture: GraphqlUpstreamCapture | null = null;
  for (let attempt = 1; attempt <= 60; attempt += 1) {
    lastCapture = await captureStorefront(name, 'StorefrontEnrichmentContext', documentText.contextRead, variables);
    assertNoTopLevelErrors(lastCapture.response.body, `${name} attempt ${attempt}`);
    if (contextCurrency(lastCapture.response.body) === expectedCurrency) return lastCapture;
    await delay(2000);
  }
  throw new Error(
    `${name} did not converge on ${expectedCurrency}: ${JSON.stringify(lastCapture?.response.body, null, 2)}`,
  );
}

function buyerContextReady(body: unknown, expectedPrice: string, expectedQuantity: number): boolean {
  return (
    readPath(body, ['data', 'product', 'variants', 'nodes', '0', 'price', 'amount']) === expectedPrice &&
    readPath(body, ['data', 'product', 'variants', 'nodes', '0', 'quantityAvailable']) === expectedQuantity &&
    readPath(body, ['data', 'product', 'variants', 'nodes', '0', 'quantityRule', 'minimum']) === 4 &&
    readPath(body, [
      'data',
      'product',
      'variants',
      'nodes',
      '0',
      'quantityPriceBreaks',
      'nodes',
      '0',
      'minimumQuantity',
    ]) === 8
  );
}

async function waitForBuyerContext(
  variables: JsonRecord,
  expectedPrice: string,
  expectedQuantity: number,
): Promise<GraphqlUpstreamCapture> {
  let lastCapture: GraphqlUpstreamCapture | null = null;
  let lastRawBody: unknown;
  for (let attempt = 1; attempt <= 60; attempt += 1) {
    const result = await captureSensitiveStorefront(
      'storefront-enrichment-authorized-buyer-context-read',
      'StorefrontEnrichmentContext',
      documentText.contextRead,
      variables,
    );
    lastCapture = result.capture;
    lastRawBody = result.rawBody;
    const errors = readPath(lastRawBody, ['errors']);
    if (!Array.isArray(errors) || errors.length === 0) {
      if (buyerContextReady(lastRawBody, expectedPrice, expectedQuantity)) return lastCapture;
    } else if (
      errors.some(
        (error) =>
          typeof error !== 'object' ||
          error === null ||
          readPath(error, ['message']) !== 'The token provided is not valid',
      )
    ) {
      assertNoTopLevelErrors(lastRawBody, `authorized buyer context attempt ${attempt}`);
    }
    await delay(2000);
  }
  throw new Error(
    `authorized buyer context did not converge: ${JSON.stringify(redactSensitive(lastRawBody), null, 2)}`,
  );
}

function publicationIdByName(payload: unknown, name: string): string {
  const publications = connectionNodes(readPath(payload, ['data', 'publications']));
  const publication = publications.find(
    (value) => typeof value === 'object' && value !== null && (value as JsonRecord)['name'] === name,
  );
  return requireString(publication, ['id'], `publication ${name}`);
}

const sellingPlanDeleteMutation = `#graphql
  mutation StorefrontEnrichmentSellingPlanDelete($id: ID!) {
    sellingPlanGroupDelete(id: $id) {
      deletedSellingPlanGroupId
      userErrors { field message code }
    }
  }
`;
const definitionDeleteMutation = `#graphql
  mutation StorefrontEnrichmentDefinitionDelete($id: ID!) {
    metafieldDefinitionDelete(id: $id, deleteAllAssociatedMetafields: true) {
      deletedDefinitionId
      userErrors { field message code }
    }
  }
`;
const catalogDeleteMutation = `#graphql
  mutation StorefrontEnrichmentCatalogDelete($id: ID!) {
    catalogDelete(id: $id) {
      deletedId
      userErrors { field message code }
    }
  }
`;
const priceListDeleteMutation = `#graphql
  mutation StorefrontEnrichmentPriceListDelete($id: ID!) {
    priceListDelete(id: $id) {
      deletedId
      userErrors { field message code }
    }
  }
`;
const marketDeleteMutation = `#graphql
  mutation StorefrontEnrichmentMarketDelete($id: ID!) {
    marketDelete(id: $id) {
      deletedId
      userErrors { field message code }
    }
  }
`;
const locationDeactivateMutation = `#graphql
  mutation StorefrontEnrichmentLocationDeactivate($locationId: ID!, $idempotencyKey: String!) {
    locationDeactivate(locationId: $locationId) @idempotent(key: $idempotencyKey) {
      location { id isActive }
      locationDeactivateUserErrors { field message code }
    }
  }
`;
const locationDeleteMutation = `#graphql
  mutation StorefrontEnrichmentLocationDelete($locationId: ID!) {
    locationDelete(locationId: $locationId) {
      deletedLocationId
      locationDeleteUserErrors { field message code }
    }
  }
`;

const suffix = new Date().toISOString().replace(/\D/gu, '').slice(0, 14);
const taxonomyTag = `000-enrichment-${suffix}`;
const productType = `000 Enrichment ${suffix}`;
const primaryHandle = `storefront-enrichment-primary-${suffix}`;
const candidateHandle = `storefront-enrichment-candidate-${suffix}`;
const visibleProductKey = `sf_visible_${suffix}`;
const hiddenProductKey = `sf_hidden_${suffix}`;
const visibleVariantKey = `sf_variant_${suffix}`;
const visibleProductValue = `visible product ${suffix}`;
const hiddenProductValue = `hidden product ${suffix}`;
const visibleVariantValue = `visible variant ${suffix}`;
const missingProductId = 'gid://shopify/Product/0';
const marketCountry = 'DK';
const marketCurrency = 'DKK';
const defaultLanguage = 'EN';

const captures: Record<string, unknown> = {};
const cleanup: Capture[] = [];
const upstreamCalls: GraphqlUpstreamCapture[] = [];
const productIds: string[] = [];
const definitionIds: string[] = [];
let primaryProductId: string | null = null;
let primaryVariantId: string | null = null;
let primaryInventoryItemId: string | null = null;
let locationId: string | null = null;
let emptyLocationId: string | null = null;
let sellingPlanGroupId: string | null = null;
let marketId: string | null = null;
let priceListId: string | null = null;
let catalogId: string | null = null;
let buyerCustomerId: string | null = null;
let buyerCompanyId: string | null = null;
let buyerPriceListId: string | null = null;
let buyerCatalogId: string | null = null;

try {
  const taxonomyBaseline = await captureStorefront(
    'storefront-enrichment-taxonomy-baseline',
    'StorefrontEnrichmentTaxonomyHydrate',
    documentText.taxonomyHydrate,
    {},
  );
  assertNoTopLevelErrors(taxonomyBaseline.response.body, 'taxonomy baseline');
  captures['taxonomyBaseline'] = taxonomyBaseline;

  const primaryProductCreate = await captureAdmin('admin-primary-product-create', documentText.productCreate, {
    product: {
      title: `Storefront Enrichment Primary ${suffix}`,
      handle: primaryHandle,
      status: 'ACTIVE',
      vendor: 'Hermes Enrichment',
      productType,
      tags: [taxonomyTag, 'enrichment-shared', 'merchandising'],
      descriptionHtml: `<p>Storefront enrichment primary ${suffix}</p>`,
      productOptions: [{ name: 'Color', values: [{ name: 'Blue' }] }],
    },
    media: [
      {
        alt: `Storefront enrichment image ${suffix}`,
        mediaContentType: 'IMAGE',
        originalSource: `https://placehold.co/640x480/png?text=storefront-enrichment-${suffix}`,
      },
    ],
  });
  assertNoTopLevelErrors(primaryProductCreate.response, 'primary productCreate');
  assertNoUserErrors(primaryProductCreate.response, ['data', 'productCreate', 'userErrors'], 'primary productCreate');
  captures['primaryProductCreate'] = primaryProductCreate;
  primaryProductId = requireString(
    primaryProductCreate.response,
    ['data', 'productCreate', 'product', 'id'],
    'primary product id',
  );
  primaryVariantId = requireString(
    primaryProductCreate.response,
    ['data', 'productCreate', 'product', 'variants', 'nodes', '0', 'id'],
    'primary variant id',
  );
  primaryInventoryItemId = requireString(
    primaryProductCreate.response,
    ['data', 'productCreate', 'product', 'variants', 'nodes', '0', 'inventoryItem', 'id'],
    'primary inventory item id',
  );
  productIds.push(primaryProductId);

  const candidateProductCreate = await captureAdmin('admin-candidate-product-create', documentText.productCreate, {
    product: {
      title: `Storefront Enrichment Candidate ${suffix}`,
      handle: candidateHandle,
      status: 'ACTIVE',
      vendor: 'Hermes Enrichment',
      productType,
      tags: [taxonomyTag, 'enrichment-shared', 'recommendation-candidate'],
      descriptionHtml: `<p>Storefront enrichment candidate ${suffix}</p>`,
    },
    media: [],
  });
  assertNoTopLevelErrors(candidateProductCreate.response, 'candidate productCreate');
  assertNoUserErrors(
    candidateProductCreate.response,
    ['data', 'productCreate', 'userErrors'],
    'candidate productCreate',
  );
  captures['candidateProductCreate'] = candidateProductCreate;
  const candidateProductId = requireString(
    candidateProductCreate.response,
    ['data', 'productCreate', 'product', 'id'],
    'candidate product id',
  );
  productIds.push(candidateProductId);

  const variantUpdate = await captureAdmin('admin-primary-variant-update', documentText.variantUpdate, {
    productId: primaryProductId,
    variants: [
      {
        id: primaryVariantId,
        price: '149.00',
        compareAtPrice: '179.00',
        barcode: `sf-enrichment-${suffix}`,
        inventoryItem: {
          sku: `SF-ENRICH-${suffix}`,
          tracked: true,
          requiresShipping: true,
        },
      },
    ],
  });
  assertNoTopLevelErrors(variantUpdate.response, 'variant update');
  assertNoUserErrors(variantUpdate.response, ['data', 'updateVariant', 'userErrors'], 'variant update');
  captures['variantUpdate'] = variantUpdate;

  const locationAdd = await captureAdmin('admin-location-add', documentText.locationAdd, {
    input: {
      name: `Storefront Enrichment ${suffix}`,
      fulfillsOnlineOrders: true,
      address: { countryCode: 'DK', city: 'Copenhagen', address1: '1 Enrichment Way' },
    },
  });
  assertNoTopLevelErrors(locationAdd.response, 'location add');
  assertNoUserErrors(locationAdd.response, ['data', 'locationAdd', 'userErrors'], 'location add');
  captures['locationAdd'] = locationAdd;
  locationId = requireString(locationAdd.response, ['data', 'locationAdd', 'location', 'id'], 'location id');

  const emptyLocationAdd = await captureAdmin('admin-empty-location-add', documentText.locationAdd, {
    input: {
      name: `Storefront Enrichment Empty ${suffix}`,
      fulfillsOnlineOrders: true,
      address: { countryCode: 'DK', city: 'Copenhagen', address1: '2 Enrichment Way' },
    },
  });
  assertNoTopLevelErrors(emptyLocationAdd.response, 'empty location add');
  assertNoUserErrors(emptyLocationAdd.response, ['data', 'locationAdd', 'userErrors'], 'empty location add');
  captures['emptyLocationAdd'] = emptyLocationAdd;
  emptyLocationId = requireString(
    emptyLocationAdd.response,
    ['data', 'locationAdd', 'location', 'id'],
    'empty location id',
  );

  const stockedInventoryActivate = await captureAdmin(
    'admin-stocked-location-inventory-activate',
    documentText.inventoryActivate,
    {
      inventoryItemId: primaryInventoryItemId,
      locationId,
      available: 37,
      idempotencyKey: `storefront-enrichment-inventory-stocked-${suffix}`,
    },
  );
  assertNoTopLevelErrors(stockedInventoryActivate.response, 'stocked inventory activate');
  assertNoUserErrors(
    stockedInventoryActivate.response,
    ['data', 'inventoryActivate', 'userErrors'],
    'stocked inventory activate',
  );
  captures['stockedInventoryActivate'] = stockedInventoryActivate;

  const emptyInventoryActivate = await captureAdmin(
    'admin-empty-location-inventory-activate',
    documentText.inventoryActivate,
    {
      inventoryItemId: primaryInventoryItemId,
      locationId: emptyLocationId,
      available: 0,
      idempotencyKey: `storefront-enrichment-inventory-empty-${suffix}`,
    },
  );
  assertNoTopLevelErrors(emptyInventoryActivate.response, 'empty inventory activate');
  assertNoUserErrors(
    emptyInventoryActivate.response,
    ['data', 'inventoryActivate', 'userErrors'],
    'empty inventory activate',
  );
  captures['emptyInventoryActivate'] = emptyInventoryActivate;

  const publicationHydrate = await captureAdminUpstream(
    'admin-storefront-publications-hydrate',
    'StorePropertiesPublishableInputValidationHydrate',
    documentText.publicationHydrate,
    {},
  );
  assertNoTopLevelErrors(publicationHydrate.response.body, 'publication hydrate');
  captures['publicationHydrate'] = publicationHydrate;
  upstreamCalls.push(publicationHydrate);
  const onlineStorePublicationId = publicationIdByName(publicationHydrate.response.body, 'Online Store');

  for (const [name, productId] of [
    ['primary', primaryProductId],
    ['candidate', candidateProductId],
  ] as const) {
    const publish = await captureAdmin(`admin-${name}-online-store-publish`, documentText.publish, {
      id: productId,
      input: [{ publicationId: onlineStorePublicationId }],
      publicationId: onlineStorePublicationId,
    });
    assertNoTopLevelErrors(publish.response, `${name} online store publish`);
    assertNoUserErrors(publish.response, ['data', 'publishablePublish', 'userErrors'], `${name} online store publish`);
    captures[`${name}OnlineStorePublish`] = publish;
  }

  const definitionInputs = [
    {
      label: 'visibleProductDefinition',
      definition: {
        name: `Storefront visible product ${suffix}`,
        namespace: 'custom',
        key: visibleProductKey,
        type: 'single_line_text_field',
        ownerType: 'PRODUCT',
        access: { storefront: 'PUBLIC_READ' },
      },
    },
    {
      label: 'hiddenProductDefinition',
      definition: {
        name: `Storefront hidden product ${suffix}`,
        namespace: 'custom',
        key: hiddenProductKey,
        type: 'single_line_text_field',
        ownerType: 'PRODUCT',
        access: { storefront: 'NONE' },
      },
    },
    {
      label: 'visibleVariantDefinition',
      definition: {
        name: `Storefront visible variant ${suffix}`,
        namespace: 'custom',
        key: visibleVariantKey,
        type: 'single_line_text_field',
        ownerType: 'PRODUCTVARIANT',
        access: { storefront: 'PUBLIC_READ' },
      },
    },
  ];
  for (const { label, definition } of definitionInputs) {
    const create = await captureAdmin(`admin-${label}`, documentText.definitionCreate, { definition });
    assertNoTopLevelErrors(create.response, label);
    assertNoUserErrors(create.response, ['data', 'metafieldDefinitionCreate', 'userErrors'], label);
    captures[label] = create;
    definitionIds.push(
      requireString(create.response, ['data', 'metafieldDefinitionCreate', 'createdDefinition', 'id'], `${label} id`),
    );
  }

  const metafieldsSet = await captureAdmin('admin-owner-metafields-set', documentText.metafieldsSet, {
    metafields: [
      {
        ownerId: primaryProductId,
        namespace: 'custom',
        key: visibleProductKey,
        type: 'single_line_text_field',
        value: visibleProductValue,
      },
      {
        ownerId: primaryProductId,
        namespace: 'custom',
        key: hiddenProductKey,
        type: 'single_line_text_field',
        value: hiddenProductValue,
      },
      {
        ownerId: primaryVariantId,
        namespace: 'custom',
        key: visibleVariantKey,
        type: 'single_line_text_field',
        value: visibleVariantValue,
      },
    ],
  });
  assertNoTopLevelErrors(metafieldsSet.response, 'metafields set');
  assertNoUserErrors(metafieldsSet.response, ['data', 'metafieldsSet', 'userErrors'], 'metafields set');
  captures['metafieldsSet'] = metafieldsSet;

  const sellingPlanCreate = await captureAdmin('admin-selling-plan-group-create', documentText.sellingPlanCreate, {
    input: {
      name: `Storefront Enrichment Subscription ${suffix}`,
      merchantCode: `sf-enrichment-${suffix}`,
      options: ['Delivery frequency'],
      position: 1,
      sellingPlansToCreate: [
        {
          name: 'Monthly enrichment delivery',
          description: `Monthly Storefront enrichment ${suffix}`,
          options: ['Every month'],
          position: 1,
          category: 'SUBSCRIPTION',
          billingPolicy: { recurring: { interval: 'MONTH', intervalCount: 1 } },
          deliveryPolicy: { recurring: { interval: 'MONTH', intervalCount: 1 } },
          pricingPolicies: [
            {
              fixed: {
                adjustmentType: 'PERCENTAGE',
                adjustmentValue: { percentage: 15 },
              },
            },
          ],
        },
      ],
    },
    resources: { productIds: [primaryProductId] },
  });
  assertNoTopLevelErrors(sellingPlanCreate.response, 'selling plan create');
  assertNoUserErrors(
    sellingPlanCreate.response,
    ['data', 'sellingPlanGroupCreate', 'userErrors'],
    'selling plan create',
  );
  captures['sellingPlanCreate'] = sellingPlanCreate;
  sellingPlanGroupId = requireString(
    sellingPlanCreate.response,
    ['data', 'sellingPlanGroupCreate', 'sellingPlanGroup', 'id'],
    'selling plan group id',
  );

  const marketCreate = await captureAdmin('admin-market-create', documentText.marketCreate, {
    input: {
      name: `Storefront Enrichment Denmark ${suffix}`,
      status: 'ACTIVE',
      conditions: { regionsCondition: { regions: [{ countryCode: marketCountry }] } },
      currencySettings: { localCurrencies: true },
    },
  });
  assertNoTopLevelErrors(marketCreate.response, 'market create');
  assertNoUserErrors(marketCreate.response, ['data', 'marketCreate', 'userErrors'], 'market create');
  captures['marketCreate'] = marketCreate;
  marketId = requireString(marketCreate.response, ['data', 'marketCreate', 'market', 'id'], 'market id');

  const priceListCreate = await captureAdmin('admin-price-list-create', documentText.priceListCreate, {
    input: {
      name: `Storefront Enrichment DKK ${suffix}`,
      currency: marketCurrency,
      parent: { adjustment: { type: 'PERCENTAGE_DECREASE', value: 5 } },
    },
  });
  assertNoTopLevelErrors(priceListCreate.response, 'price list create');
  assertNoUserErrors(priceListCreate.response, ['data', 'priceListCreate', 'userErrors'], 'price list create');
  captures['priceListCreate'] = priceListCreate;
  priceListId = requireString(
    priceListCreate.response,
    ['data', 'priceListCreate', 'priceList', 'id'],
    'price list id',
  );

  const catalogCreate = await captureAdmin('admin-catalog-create', documentText.catalogCreate, {
    input: {
      title: `Storefront Enrichment Denmark Catalog ${suffix}`,
      status: 'ACTIVE',
      context: { marketIds: [marketId] },
      priceListId,
    },
  });
  assertNoTopLevelErrors(catalogCreate.response, 'catalog create');
  assertNoUserErrors(catalogCreate.response, ['data', 'catalogCreate', 'userErrors'], 'catalog create');
  captures['catalogCreate'] = catalogCreate;
  catalogId = requireString(catalogCreate.response, ['data', 'catalogCreate', 'catalog', 'id'], 'catalog id');

  const unsupportedMarketQuantityPricing = await captureAdmin(
    'admin-market-quantity-pricing-unsupported',
    documentText.quantityPricing,
    {
      priceListId,
      input: {
        pricesToAdd: [],
        pricesToDeleteByVariantId: [],
        quantityRulesToAdd: [{ variantId: primaryVariantId, minimum: 5, maximum: 50, increment: 5 }],
        quantityRulesToDeleteByVariantId: [],
        quantityPriceBreaksToAdd: [
          {
            variantId: primaryVariantId,
            minimumQuantity: 10,
            price: { amount: '749.00', currencyCode: marketCurrency },
          },
        ],
        quantityPriceBreaksToDelete: [],
        quantityPriceBreaksToDeleteByVariantId: [],
      },
    },
  );
  assertNoTopLevelErrors(unsupportedMarketQuantityPricing.response, 'unsupported market quantity pricing');
  const unsupportedErrors = readPath(unsupportedMarketQuantityPricing.response, [
    'data',
    'quantityPricingByVariantUpdate',
    'userErrors',
  ]);
  if (
    !Array.isArray(unsupportedErrors) ||
    unsupportedErrors.length === 0 ||
    readPath(unsupportedErrors[0], ['code']) !== 'QUANTITY_RULE_ADD_CATALOG_CONTEXT_NOT_SUPPORTED'
  ) {
    throw new Error(
      `market quantity pricing did not return the captured unsupported-context error: ${JSON.stringify(unsupportedMarketQuantityPricing.response, null, 2)}`,
    );
  }
  captures['unsupportedMarketQuantityPricing'] = unsupportedMarketQuantityPricing;

  const quantityPricing = await captureAdmin('admin-market-fixed-price', documentText.quantityPricing, {
    priceListId,
    input: {
      pricesToAdd: [
        {
          variantId: primaryVariantId,
          price: { amount: '799.00', currencyCode: marketCurrency },
          compareAtPrice: { amount: '999.00', currencyCode: marketCurrency },
        },
      ],
      pricesToDeleteByVariantId: [],
      quantityRulesToAdd: [],
      quantityRulesToDeleteByVariantId: [],
      quantityPriceBreaksToAdd: [],
      quantityPriceBreaksToDelete: [],
      quantityPriceBreaksToDeleteByVariantId: [],
    },
  });
  assertNoTopLevelErrors(quantityPricing.response, 'quantity pricing');
  assertNoUserErrors(
    quantityPricing.response,
    ['data', 'quantityPricingByVariantUpdate', 'userErrors'],
    'quantity pricing',
  );
  captures['quantityPricing'] = quantityPricing;

  const buyerEmail = `storefront-enrichment-buyer-${suffix}@example.com`;
  const buyerPassword = 'CodexPass123!';
  const buyerCustomerCreate = await captureSensitiveStorefront(
    'storefront-enrichment-buyer-customer-create',
    'StorefrontCustomerAuthCreate',
    documentText.customerCreate,
    {
      input: {
        email: buyerEmail,
        password: buyerPassword,
        firstName: 'Storefront',
        lastName: 'Enrichment Buyer',
      },
    },
  );
  assertNoTopLevelErrors(buyerCustomerCreate.rawBody, 'buyer customer create');
  assertNoUserErrors(
    buyerCustomerCreate.rawBody,
    ['data', 'customerCreate', 'customerUserErrors'],
    'buyer customer create',
  );
  captures['buyerCustomerCreate'] = buyerCustomerCreate.capture;
  buyerCustomerId = requireString(
    buyerCustomerCreate.rawBody,
    ['data', 'customerCreate', 'customer', 'id'],
    'buyer customer id',
  );

  const buyerCompanyCreate = await captureAdmin('admin-buyer-company-create', documentText.companyCreate, {
    input: {
      company: { name: `Storefront Enrichment Buyer ${suffix}` },
      companyLocation: {
        name: `Storefront Enrichment Buyer HQ ${suffix}`,
        billingAddress: {
          address1: '3 Enrichment Way',
          city: 'Toronto',
          countryCode: 'CA',
        },
      },
    },
  });
  assertNoTopLevelErrors(buyerCompanyCreate.response, 'buyer company create');
  assertNoUserErrors(buyerCompanyCreate.response, ['data', 'companyCreate', 'userErrors'], 'buyer company create');
  captures['buyerCompanyCreate'] = buyerCompanyCreate;
  buyerCompanyId = requireString(
    buyerCompanyCreate.response,
    ['data', 'companyCreate', 'company', 'id'],
    'buyer company id',
  );
  const buyerCompanyLocationId = requireString(
    buyerCompanyCreate.response,
    ['data', 'companyCreate', 'company', 'locations', 'nodes', '0', 'id'],
    'buyer company location id',
  );
  const buyerCompanyRoleId = requireString(
    buyerCompanyCreate.response,
    ['data', 'companyCreate', 'company', 'contactRoles', 'nodes', '0', 'id'],
    'buyer company role id',
  );

  const buyerCompanyAssignCustomer = await captureAdmin(
    'admin-buyer-company-assign-customer',
    documentText.companyAssignCustomer,
    { companyId: buyerCompanyId, customerId: buyerCustomerId },
  );
  assertNoTopLevelErrors(buyerCompanyAssignCustomer.response, 'buyer company assign customer');
  assertNoUserErrors(
    buyerCompanyAssignCustomer.response,
    ['data', 'companyAssignCustomerAsContact', 'userErrors'],
    'buyer company assign customer',
  );
  captures['buyerCompanyAssignCustomer'] = buyerCompanyAssignCustomer;
  const buyerCompanyContactId = requireString(
    buyerCompanyAssignCustomer.response,
    ['data', 'companyAssignCustomerAsContact', 'companyContact', 'id'],
    'buyer company contact id',
  );

  const buyerCompanyAssignRole = await captureAdmin('admin-buyer-company-assign-role', documentText.companyAssignRole, {
    companyContactId: buyerCompanyContactId,
    companyContactRoleId: buyerCompanyRoleId,
    companyLocationId: buyerCompanyLocationId,
  });
  assertNoTopLevelErrors(buyerCompanyAssignRole.response, 'buyer company assign role');
  assertNoUserErrors(
    buyerCompanyAssignRole.response,
    ['data', 'companyContactAssignRole', 'userErrors'],
    'buyer company assign role',
  );
  captures['buyerCompanyAssignRole'] = buyerCompanyAssignRole;

  const buyerTokenCreate = await captureSensitiveStorefront(
    'storefront-enrichment-buyer-token-create',
    'StorefrontCustomerAuthTokenCreate',
    documentText.customerTokenCreate,
    { input: { email: buyerEmail, password: buyerPassword } },
  );
  assertNoTopLevelErrors(buyerTokenCreate.rawBody, 'buyer token create');
  assertNoUserErrors(
    buyerTokenCreate.rawBody,
    ['data', 'customerAccessTokenCreate', 'customerUserErrors'],
    'buyer token create',
  );
  captures['buyerTokenCreate'] = buyerTokenCreate.capture;
  const buyerCustomerAccessToken = requireString(
    buyerTokenCreate.rawBody,
    ['data', 'customerAccessTokenCreate', 'customerAccessToken', 'accessToken'],
    'buyer customer access token',
  );

  const buyerPriceListCreate = await captureAdmin('admin-buyer-price-list-create', documentText.priceListCreate, {
    input: {
      name: `Storefront Enrichment Buyer CAD ${suffix}`,
      currency: 'CAD',
      parent: { adjustment: { type: 'PERCENTAGE_DECREASE', value: 0 } },
    },
  });
  assertNoTopLevelErrors(buyerPriceListCreate.response, 'buyer price list create');
  assertNoUserErrors(
    buyerPriceListCreate.response,
    ['data', 'priceListCreate', 'userErrors'],
    'buyer price list create',
  );
  captures['buyerPriceListCreate'] = buyerPriceListCreate;
  buyerPriceListId = requireString(
    buyerPriceListCreate.response,
    ['data', 'priceListCreate', 'priceList', 'id'],
    'buyer price list id',
  );

  const buyerCatalogCreate = await captureAdmin('admin-buyer-catalog-create', documentText.catalogCreate, {
    input: {
      title: `Storefront Enrichment Buyer Catalog ${suffix}`,
      status: 'ACTIVE',
      context: { companyLocationIds: [buyerCompanyLocationId] },
      priceListId: buyerPriceListId,
    },
  });
  assertNoTopLevelErrors(buyerCatalogCreate.response, 'buyer catalog create');
  assertNoUserErrors(buyerCatalogCreate.response, ['data', 'catalogCreate', 'userErrors'], 'buyer catalog create');
  captures['buyerCatalogCreate'] = buyerCatalogCreate;
  buyerCatalogId = requireString(
    buyerCatalogCreate.response,
    ['data', 'catalogCreate', 'catalog', 'id'],
    'buyer catalog id',
  );

  const buyerQuantityPricing = await captureAdmin('admin-buyer-quantity-pricing', documentText.quantityPricing, {
    priceListId: buyerPriceListId,
    input: {
      pricesToAdd: [
        {
          variantId: primaryVariantId,
          price: { amount: '89.00', currencyCode: 'CAD' },
          compareAtPrice: { amount: '99.00', currencyCode: 'CAD' },
        },
      ],
      pricesToDeleteByVariantId: [],
      quantityRulesToAdd: [{ variantId: primaryVariantId, minimum: 4, maximum: 20, increment: 2 }],
      quantityRulesToDeleteByVariantId: [],
      quantityPriceBreaksToAdd: [
        {
          variantId: primaryVariantId,
          minimumQuantity: 8,
          price: { amount: '79.00', currencyCode: 'CAD' },
        },
      ],
      quantityPriceBreaksToDelete: [],
      quantityPriceBreaksToDeleteByVariantId: [],
    },
  });
  assertNoTopLevelErrors(buyerQuantityPricing.response, 'buyer quantity pricing');
  assertNoUserErrors(
    buyerQuantityPricing.response,
    ['data', 'quantityPricingByVariantUpdate', 'userErrors'],
    'buyer quantity pricing',
  );
  captures['buyerQuantityPricing'] = buyerQuantityPricing;

  const taxonomyHydrate = await captureStorefront(
    'storefront-enrichment-taxonomy-hydrate',
    'StorefrontEnrichmentTaxonomyHydrate',
    documentText.taxonomyHydrate,
    {},
  );
  assertNoTopLevelErrors(taxonomyHydrate.response.body, 'taxonomy hydrate');
  captures['taxonomyHydrate'] = taxonomyHydrate;
  upstreamCalls.push(taxonomyHydrate);

  const merchandisingVariables = {
    id: primaryProductId,
    handle: primaryHandle,
    missingId: missingProductId,
    visibleProductKey,
    hiddenProductKey,
    visibleVariantKey,
  };
  const merchandisingRead = await waitForMerchandising(merchandisingVariables, {
    id: primaryProductId,
    handle: primaryHandle,
    visibleProductKey,
    visibleProductValue,
    hiddenProductKey,
    visibleVariantKey,
    visibleVariantValue,
  });
  captures['merchandisingRead'] = merchandisingRead;

  const defaultContextVariables = {
    id: primaryProductId,
    country: null,
    language: null,
    preferredLocationId: null,
    buyer: null,
  };
  const defaultContextHydrate = await captureStorefront(
    'storefront-enrichment-default-context-hydrate',
    'StorefrontEnrichmentContextHydrate',
    documentText.contextHydrate,
    {
      country: null,
      language: null,
      preferredLocationId: null,
      buyer: null,
    },
  );
  assertNoTopLevelErrors(defaultContextHydrate.response.body, 'default context hydrate');
  upstreamCalls.push(defaultContextHydrate);
  captures['defaultContextHydrate'] = defaultContextHydrate;
  const defaultCurrency = requireString(
    defaultContextHydrate.response.body,
    ['data', 'localization', 'country', 'currency', 'isoCode'],
    'default context currency',
  );
  captures['defaultContextRead'] = await waitForContext(
    'storefront-enrichment-default-context-read',
    defaultContextVariables,
    defaultCurrency,
  );

  const countryContextVariables = {
    id: primaryProductId,
    country: marketCountry,
    language: defaultLanguage,
    preferredLocationId: null,
    buyer: null,
  };
  const countryContextHydrate = await captureStorefront(
    'storefront-enrichment-country-context-hydrate',
    'StorefrontEnrichmentContextHydrate',
    documentText.contextHydrate,
    {
      country: marketCountry,
      language: defaultLanguage,
      preferredLocationId: null,
      buyer: null,
    },
  );
  assertNoTopLevelErrors(countryContextHydrate.response.body, 'country context hydrate');
  upstreamCalls.push(countryContextHydrate);
  captures['countryContextHydrate'] = countryContextHydrate;
  captures['countryContextRead'] = await waitForContext(
    'storefront-enrichment-country-context-read',
    countryContextVariables,
    marketCurrency,
  );

  const preferredContextVariables = {
    id: primaryProductId,
    country: marketCountry,
    language: defaultLanguage,
    preferredLocationId: locationId,
    buyer: null,
  };
  const preferredContextHydrate = await captureStorefront(
    'storefront-enrichment-preferred-location-context-hydrate',
    'StorefrontEnrichmentContextHydrate',
    documentText.contextHydrate,
    {
      country: marketCountry,
      language: defaultLanguage,
      preferredLocationId: locationId,
      buyer: null,
    },
  );
  assertNoTopLevelErrors(preferredContextHydrate.response.body, 'preferred location context hydrate');
  upstreamCalls.push(preferredContextHydrate);
  captures['preferredContextHydrate'] = preferredContextHydrate;
  captures['preferredContextRead'] = await waitForContext(
    'storefront-enrichment-preferred-location-context-read',
    preferredContextVariables,
    marketCurrency,
  );

  const emptyPreferredContextVariables = {
    id: primaryProductId,
    country: marketCountry,
    language: defaultLanguage,
    preferredLocationId: emptyLocationId,
    buyer: null,
  };
  const emptyPreferredContextHydrate = await captureStorefront(
    'storefront-enrichment-empty-preferred-location-context-hydrate',
    'StorefrontEnrichmentContextHydrate',
    documentText.contextHydrate,
    {
      country: marketCountry,
      language: defaultLanguage,
      preferredLocationId: emptyLocationId,
      buyer: null,
    },
  );
  assertNoTopLevelErrors(emptyPreferredContextHydrate.response.body, 'empty preferred location context hydrate');
  upstreamCalls.push(emptyPreferredContextHydrate);
  captures['emptyPreferredContextHydrate'] = emptyPreferredContextHydrate;
  captures['emptyPreferredContextRead'] = await waitForContext(
    'storefront-enrichment-empty-preferred-location-context-read',
    emptyPreferredContextVariables,
    marketCurrency,
  );

  const authorizedBuyerContextVariables = {
    id: primaryProductId,
    country: null,
    language: null,
    preferredLocationId: locationId,
    buyer: {
      customerAccessToken: buyerCustomerAccessToken,
      companyLocationId: buyerCompanyLocationId,
    },
  };
  captures['authorizedBuyerContextRead'] = await waitForBuyerContext(authorizedBuyerContextVariables, '89.0', 37);

  const invalidBuyerVariables = {
    id: primaryProductId,
    country: marketCountry,
    language: defaultLanguage,
    preferredLocationId: locationId,
    buyer: {
      customerAccessToken: 'invalid-storefront-enrichment-buyer-token',
      companyLocationId: 'gid://shopify/CompanyLocation/0',
    },
  };
  const invalidBuyerHydrate = await captureStorefront(
    'storefront-enrichment-invalid-buyer-context-hydrate',
    'StorefrontEnrichmentContextHydrate',
    documentText.contextHydrate,
    {
      country: invalidBuyerVariables.country,
      language: invalidBuyerVariables.language,
      preferredLocationId: invalidBuyerVariables.preferredLocationId,
      buyer: invalidBuyerVariables.buyer,
    },
  );
  const invalidBuyerErrors = readPath(invalidBuyerHydrate.response.body, ['errors']);
  if (!Array.isArray(invalidBuyerErrors) || invalidBuyerErrors.length === 0) {
    throw new Error(
      `invalid buyer context hydrate did not return a top-level error: ${JSON.stringify(invalidBuyerHydrate.response.body, null, 2)}`,
    );
  }
  upstreamCalls.push(invalidBuyerHydrate);
  captures['invalidBuyerHydrate'] = invalidBuyerHydrate;
} finally {
  if (buyerCatalogId) {
    cleanup.push(await bestEffortAdminCleanup('cleanup-buyer-catalog', catalogDeleteMutation, { id: buyerCatalogId }));
  }
  if (buyerPriceListId) {
    cleanup.push(
      await bestEffortAdminCleanup('cleanup-buyer-price-list', priceListDeleteMutation, { id: buyerPriceListId }),
    );
  }
  if (buyerCompanyId) {
    cleanup.push(
      await bestEffortAdminCleanup('cleanup-buyer-company', documentText.companyDelete, { id: buyerCompanyId }),
    );
  }
  if (buyerCustomerId) {
    cleanup.push(
      await bestEffortAdminCleanup('cleanup-buyer-customer', documentText.customerDelete, {
        input: { id: buyerCustomerId },
      }),
    );
  }
  if (sellingPlanGroupId) {
    cleanup.push(
      await bestEffortAdminCleanup('cleanup-selling-plan-group', sellingPlanDeleteMutation, {
        id: sellingPlanGroupId,
      }),
    );
  }
  for (const productId of productIds.reverse()) {
    cleanup.push(
      await bestEffortAdminCleanup('cleanup-product', documentText.productDelete, {
        input: { id: productId },
      }),
    );
  }
  for (const definitionId of definitionIds.reverse()) {
    cleanup.push(
      await bestEffortAdminCleanup('cleanup-metafield-definition', definitionDeleteMutation, {
        id: definitionId,
      }),
    );
  }
  if (catalogId) {
    cleanup.push(await bestEffortAdminCleanup('cleanup-catalog', catalogDeleteMutation, { id: catalogId }));
  }
  if (priceListId) {
    cleanup.push(await bestEffortAdminCleanup('cleanup-price-list', priceListDeleteMutation, { id: priceListId }));
  }
  if (marketId) {
    cleanup.push(await bestEffortAdminCleanup('cleanup-market', marketDeleteMutation, { id: marketId }));
  }
  for (const [name, cleanupLocationId] of [
    ['stocked', locationId],
    ['empty', emptyLocationId],
  ] as const) {
    if (!cleanupLocationId) continue;
    cleanup.push(
      await bestEffortAdminCleanup(`cleanup-${name}-location-deactivate`, locationDeactivateMutation, {
        locationId: cleanupLocationId,
        idempotencyKey: `storefront-enrichment-${name}-location-deactivate-${suffix}`,
      }),
    );
    cleanup.push(
      await bestEffortAdminCleanup(`cleanup-${name}-location-delete`, locationDeleteMutation, {
        locationId: cleanupLocationId,
      }),
    );
  }
}

const fixturePath = path.join(
  'fixtures',
  'conformance',
  storeDomain,
  apiVersion,
  'storefront',
  'storefront-catalog-enrichment.json',
);
await mkdir(path.dirname(fixturePath), { recursive: true });
await writeFile(
  fixturePath,
  `${JSON.stringify(
    {
      scenarioId: 'storefront-catalog-enrichment',
      apiSurface: 'storefront',
      authMode: 'storefront-access-token',
      storeDomain,
      apiVersion,
      endpoint: storefrontEndpoint,
      capturedAt: new Date().toISOString(),
      storefrontToken: '<redacted:storefront-access-token>',
      setup: captures,
      upstreamCalls,
      cleanup,
    },
    null,
    2,
  )}\n`,
);

console.log(
  JSON.stringify(
    {
      ok: true,
      fixturePath,
      primaryProductId,
      primaryVariantId,
      locationId,
      marketId,
      catalogId,
      priceListId,
      sellingPlanGroupId,
    },
    null,
    2,
  ),
);

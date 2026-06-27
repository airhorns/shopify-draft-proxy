/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type GraphqlCapture = {
  label: string;
  request: {
    query: string;
    variables: JsonRecord;
  };
  status: number;
  response: unknown;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2025-01',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metafields');
const outputPath = path.join(outputDir, 'metafields-set-validation-gaps.json');
const paritySpecPath = path.join('config', 'parity-specs', 'metafields', 'metafields-set-validation-gaps.json');

const requestPaths = {
  createDefinitions:
    'config/parity-requests/metafields/metafields-set-definition-validation-create-definitions.graphql',
  metaobjectDefinitions:
    'config/parity-requests/metafields/metafields-set-definition-validation-metaobject-definitions.graphql',
  metaobjectCreate: 'config/parity-requests/metafields/metafields-set-definition-validation-metaobject-create.graphql',
  referenceDefinition:
    'config/parity-requests/metafields/metafields-set-definition-validation-reference-definition.graphql',
  listScalarDefinition: 'config/parity-requests/metafields/metafields-set-list-scalar-category.graphql',
  setDefinitionValue: 'config/parity-requests/metafields/metafields-set-definition-validation-set.graphql',
  ownerTypesSet: 'config/parity-requests/metafields/metafields-set-non-product-owner-types.graphql',
  onlineStoreContentCreate: 'config/parity-requests/online-store/online-store-content-create.graphql',
  onlineStoreArticleCreate: 'config/parity-requests/online-store/online-store-content-article-create.graphql',
};

const documents = Object.fromEntries(
  await Promise.all(
    Object.entries(requestPaths).map(async ([key, filePath]) => [key, await readFile(filePath, 'utf8')]),
  ),
) as Record<keyof typeof requestPaths, string>;

const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const productCreateMutation = `#graphql
mutation MetafieldsSetValidationGapsProductCreate($product: ProductCreateInput!) {
  productCreate(product: $product) {
    product {
      id
      title
      handle
      status
    }
    userErrors {
      field
      message
    }
  }
}
`;

const productDeleteMutation = `#graphql
mutation MetafieldsSetValidationGapsProductDelete($input: ProductDeleteInput!) {
  productDelete(input: $input) {
    deletedProductId
    userErrors {
      field
      message
    }
  }
}
`;

const metaobjectDeleteMutation = `#graphql
mutation MetafieldsSetValidationGapsMetaobjectDelete($id: ID!) {
  metaobjectDelete(id: $id) {
    deletedId
    userErrors {
      field
      message
      code
    }
  }
}
`;

const metaobjectDefinitionDeleteMutation = `#graphql
mutation MetafieldsSetValidationGapsMetaobjectDefinitionDelete($id: ID!) {
  metaobjectDefinitionDelete(id: $id) {
    deletedId
    userErrors {
      field
      message
      code
    }
  }
}
`;

const metafieldDefinitionDeleteMutation = `#graphql
mutation MetafieldsSetValidationGapsMetafieldDefinitionDelete($id: ID!) {
  metafieldDefinitionDelete(id: $id, deleteAllAssociatedMetafields: true) {
    deletedDefinitionId
    userErrors {
      field
      message
      code
    }
  }
}
`;

const metafieldsDeleteMutation = `#graphql
mutation MetafieldsSetValidationGapsMetafieldsDelete($metafields: [MetafieldIdentifierInput!]!) {
  metafieldsDelete(metafields: $metafields) {
    deletedMetafields {
      ownerId
      namespace
      key
    }
    userErrors {
      field
      message
    }
  }
}
`;

const articleDeleteMutation = `#graphql
mutation MetafieldsSetValidationGapsArticleDelete($id: ID!) {
  articleDelete(id: $id) {
    deletedArticleId
    userErrors {
      field
      message
    }
  }
}
`;

const pageDeleteMutation = `#graphql
mutation MetafieldsSetValidationGapsPageDelete($id: ID!) {
  pageDelete(id: $id) {
    deletedPageId
    userErrors {
      field
      message
    }
  }
}
`;

const blogDeleteMutation = `#graphql
mutation MetafieldsSetValidationGapsBlogDelete($id: ID!) {
  blogDelete(id: $id) {
    deletedBlogId
    userErrors {
      field
      message
    }
  }
}
`;

const locationsReadQuery = `#graphql
query MetafieldsSetValidationGapsLocationsRead {
  locations(first: 1) {
    nodes {
      id
      name
    }
  }
}
`;

const marketsReadQuery = `#graphql
query MetafieldsSetValidationGapsMarketsRead {
  markets(first: 1) {
    nodes {
      id
      name
    }
  }
}
`;

const marketCreateMutation = `#graphql
mutation MetafieldsSetValidationGapsMarketCreate($input: MarketCreateInput!) {
  marketCreate(input: $input) {
    market {
      id
      name
      handle
      status
      enabled
    }
    userErrors {
      field
      message
      code
    }
  }
}
`;

const marketDeleteMutation = `#graphql
mutation MetafieldsSetValidationGapsMarketDelete($id: ID!) {
  marketDelete(id: $id) {
    deletedId
    userErrors {
      field
      message
      code
    }
  }
}
`;

const ownerMetafieldsHydrateQuery =
  'query OwnerMetafieldsHydrateNodes($ids: [ID!]!) { nodes(ids: $ids) { __typename id ... on Product { id title handle status totalInventory tracksInventory createdAt updatedAt metafields(first: 250) { nodes { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } variants(first: 10) { nodes { id title sku barcode price compareAtPrice taxable inventoryPolicy inventoryQuantity selectedOptions { name value } inventoryItem { id tracked requiresShipping } } } } ... on ProductVariant { id title sku barcode price compareAtPrice taxable inventoryPolicy inventoryQuantity selectedOptions { name value } inventoryItem { id tracked requiresShipping } product { id title handle status totalInventory tracksInventory createdAt updatedAt } metafields(first: 250) { nodes { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } ... on Collection { id title handle metafields(first: 250) { nodes { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } ... on Customer { id displayName email metafields(first: 250) { nodes { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } ... on Order { id name metafields(first: 250) { nodes { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } ... on Company { id name metafields(first: 250) { nodes { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } } }';

const suffix = Date.now().toString(36);
const namespace = `validation_values_${suffix}`;
const ownerNamespace = `owner_type_values_${suffix}`;
const targetMetaobjectType = `validation_target_${suffix}`;
const replacementMetaobjectType = `validation_replacement_${suffix}`;
const listScalarMetaobjectType = `validation_list_category_${suffix}`;
const createdMetafieldDefinitionIds = new Set<string>();
const createdMetaobjectDefinitionIds = new Set<string>();
const cleanup: GraphqlCapture[] = [];
const upstreamCalls: unknown[] = [];

let productId: string | null = null;
let metaobjectId: string | null = null;
let pageId: string | null = null;
let blogId: string | null = null;
let articleId: string | null = null;
let createdMarketId: string | null = null;

function readObject(value: unknown): JsonRecord | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readPath(value: unknown, pathSegments: Array<string | number>): unknown {
  let current: unknown = value;
  for (const segment of pathSegments) {
    if (typeof segment === 'number') {
      current = Array.isArray(current) ? current[segment] : undefined;
    } else {
      current = readObject(current)?.[segment];
    }
  }
  return current;
}

function readRequiredString(value: unknown, pathSegments: Array<string | number>, label: string): string {
  const result = readPath(value, pathSegments);
  if (typeof result !== 'string' || result.length === 0) {
    throw new Error(`${label} missing string at ${pathSegments.join('.')}: ${JSON.stringify(value, null, 2)}`);
  }
  return result;
}

function readUserErrors(value: unknown): unknown[] {
  const userErrors = readObject(value)?.['userErrors'];
  return Array.isArray(userErrors) ? userErrors : [];
}

function assertHttpOk(capture: GraphqlCapture, label: string): void {
  if (capture.status < 200 || capture.status >= 300 || readObject(capture.response)?.['errors']) {
    throw new Error(`${label} failed: ${JSON.stringify(capture, null, 2)}`);
  }
}

function assertNoUserErrors(value: unknown, label: string): void {
  const userErrors = readUserErrors(value);
  if (userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function definitionInput(name: string, key: string, type: string, validations: Array<{ name: string; value: string }>) {
  return {
    name,
    namespace,
    key,
    ownerType: 'PRODUCT',
    type,
    validations,
  };
}

async function capture(label: string, query: string, variables: JsonRecord): Promise<GraphqlCapture> {
  const result = await runGraphqlRaw(query, variables);
  const entry = {
    label,
    request: { query, variables },
    status: result.status,
    response: result.payload,
  };
  assertHttpOk(entry, label);
  return entry;
}

async function cleanupCapture(label: string, query: string, variables: JsonRecord): Promise<void> {
  try {
    cleanup.push(await capture(label, query, variables));
  } catch (error) {
    cleanup.push({
      label,
      request: { query, variables },
      status: 0,
      response: { error: String(error) },
    });
  }
}

async function captureOwnerHydration(ids: string[]): Promise<void> {
  const sortedIds = [...ids].sort();
  const result = await runGraphqlRaw(ownerMetafieldsHydrateQuery, { ids: sortedIds });
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`Owner hydration cassette capture failed: ${JSON.stringify(result, null, 2)}`);
  }
  upstreamCalls.push({
    operationName: 'OwnerMetafieldsHydrateNodes',
    variables: { ids: sortedIds },
    query: ownerMetafieldsHydrateQuery,
    response: {
      status: result.status,
      body: result.payload,
    },
  });
}

function assertMetafieldDefinitionIds(createDefinitions: GraphqlCapture): void {
  for (const alias of ['quantityMin', 'quantityMax', 'sku', 'color', 'rating', 'launchDate', 'startsAt']) {
    const payload = readPath(createDefinitions.response, ['data', alias]);
    assertNoUserErrors(payload, `metafieldDefinitionCreate ${alias}`);
    createdMetafieldDefinitionIds.add(
      readRequiredString(payload, ['createdDefinition', 'id'], `metafieldDefinitionCreate ${alias}`),
    );
  }
}

function assertInvalidSetShape(invalidSet: GraphqlCapture, expectedErrors: number): void {
  const payload = readPath(invalidSet.response, ['data', 'metafieldsSet']);
  const metafields = readObject(payload)?.['metafields'];
  const userErrors = readUserErrors(payload);
  if (!Array.isArray(metafields) || metafields.length !== 0 || userErrors.length !== expectedErrors) {
    throw new Error(`Expected ${expectedErrors} metafieldsSet userErrors: ${JSON.stringify(payload, null, 2)}`);
  }
}

function assertAcceptedMetafieldsSet(set: GraphqlCapture, expectedCount: number, label: string): void {
  const payload = readPath(set.response, ['data', 'metafieldsSet']);
  assertNoUserErrors(payload, label);
  const metafields = readObject(payload)?.['metafields'];
  if (!Array.isArray(metafields) || metafields.length !== expectedCount) {
    throw new Error(`${label} expected ${expectedCount} metafields: ${JSON.stringify(payload, null, 2)}`);
  }
}

async function setupOnlineStoreOwners(): Promise<{ pageId: string; articleId: string }> {
  const contentCreate = await capture('online store owner setup', documents.onlineStoreContentCreate, {
    blog: {
      title: `Metafields owner type blog ${suffix}`,
      commentPolicy: 'MODERATED',
    },
    page: {
      title: `Metafields owner type page ${suffix}`,
      body: '<p>Metafields owner type parity page</p>',
      isPublished: false,
    },
  });
  assertNoUserErrors(readPath(contentCreate.response, ['data', 'blogCreate']), 'blogCreate owner setup');
  assertNoUserErrors(readPath(contentCreate.response, ['data', 'pageCreate']), 'pageCreate owner setup');
  blogId = readRequiredString(contentCreate.response, ['data', 'blogCreate', 'blog', 'id'], 'blogCreate owner setup');
  pageId = readRequiredString(contentCreate.response, ['data', 'pageCreate', 'page', 'id'], 'pageCreate owner setup');

  const articleCreate = await capture('article owner setup', documents.onlineStoreArticleCreate, {
    article: {
      blogId,
      title: `Metafields owner type article ${suffix}`,
      body: '<p>Metafields owner type parity article</p>',
      summary: '<p>Owner type parity summary</p>',
      isPublished: false,
      tags: [`metafields-owner-type-${suffix}`],
      author: { name: 'Metafields Owner Type Capture' },
    },
  });
  assertNoUserErrors(readPath(articleCreate.response, ['data', 'articleCreate']), 'articleCreate owner setup');
  articleId = readRequiredString(
    articleCreate.response,
    ['data', 'articleCreate', 'article', 'id'],
    'articleCreate owner setup',
  );
  return { pageId, articleId };
}

async function readLocationId(): Promise<string> {
  const locations = await capture('location owner setup read', locationsReadQuery, {});
  return readRequiredString(locations.response, ['data', 'locations', 'nodes', 0, 'id'], 'locations(first: 1)');
}

async function readOrCreateMarketId(): Promise<string> {
  const markets = await capture('market owner setup read', marketsReadQuery, {});
  const existing = readPath(markets.response, ['data', 'markets', 'nodes', 0, 'id']);
  if (typeof existing === 'string' && existing.length > 0) {
    return existing;
  }
  const market = await capture('market owner setup create', marketCreateMutation, {
    input: {
      name: `Metafields Owner Type ${suffix}`,
      enabled: true,
    },
  });
  assertNoUserErrors(readPath(market.response, ['data', 'marketCreate']), 'marketCreate owner setup');
  createdMarketId = readRequiredString(market.response, ['data', 'marketCreate', 'market', 'id'], 'marketCreate');
  return createdMarketId;
}

const createDefinitionsVariables = {
  quantityMin: definitionInput('Quantity minimum', 'quantity_min', 'number_integer', [{ name: 'min', value: '2' }]),
  quantityMax: definitionInput('Quantity maximum', 'quantity_max', 'number_integer', [{ name: 'max', value: '5' }]),
  sku: definitionInput('SKU pattern', 'sku', 'single_line_text_field', [{ name: 'regex', value: '^[A-Z]{3}$' }]),
  color: definitionInput('Color choices', 'color', 'single_line_text_field', [
    { name: 'choices', value: '["red","blue"]' },
  ]),
  rating: definitionInput('Rating scale', 'rating', 'rating', [
    { name: 'scale_min', value: '1.0' },
    { name: 'scale_max', value: '5.0' },
  ]),
  launchDate: definitionInput('Launch date', 'launch_date', 'date', [
    { name: 'min', value: '2026-01-01' },
    { name: 'max', value: '2026-12-31' },
  ]),
  startsAt: definitionInput('Starts at', 'starts_at', 'date_time', [
    { name: 'min', value: '2026-01-01T00:00:00+00:00' },
    { name: 'max', value: '2026-12-31T23:59:59+00:00' },
  ]),
};

const fixture: JsonRecord = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  namespace,
  ownerNamespace,
};

try {
  const productCreate = await capture('product owner setup', productCreateMutation, {
    product: {
      title: `Metafields validation owner ${suffix}`,
      status: 'DRAFT',
    },
  });
  assertNoUserErrors(readPath(productCreate.response, ['data', 'productCreate']), 'productCreate owner setup');
  productId = readRequiredString(productCreate.response, ['data', 'productCreate', 'product', 'id'], 'productCreate');
  await captureOwnerHydration([productId]);

  const createDefinitions = await capture(
    'metafield definition validation rule setup',
    documents.createDefinitions,
    createDefinitionsVariables,
  );
  assertMetafieldDefinitionIds(createDefinitions);

  const metaobjectDefinitions = await capture(
    'metaobject definition setup for reference validation',
    documents.metaobjectDefinitions,
    {
      target: {
        type: targetMetaobjectType,
        name: `Metafields target ${suffix}`,
        displayNameKey: 'title',
        fieldDefinitions: [{ key: 'title', name: 'Title', type: 'single_line_text_field' }],
      },
      replacement: {
        type: replacementMetaobjectType,
        name: `Metafields replacement ${suffix}`,
        displayNameKey: 'title',
        fieldDefinitions: [{ key: 'title', name: 'Title', type: 'single_line_text_field' }],
      },
    },
  );
  assertNoUserErrors(readPath(metaobjectDefinitions.response, ['data', 'target']), 'target metaobjectDefinitionCreate');
  assertNoUserErrors(
    readPath(metaobjectDefinitions.response, ['data', 'replacement']),
    'replacement metaobjectDefinitionCreate',
  );
  const targetDefinitionId = readRequiredString(
    metaobjectDefinitions.response,
    ['data', 'target', 'metaobjectDefinition', 'id'],
    'target metaobjectDefinitionCreate',
  );
  const replacementDefinitionId = readRequiredString(
    metaobjectDefinitions.response,
    ['data', 'replacement', 'metaobjectDefinition', 'id'],
    'replacement metaobjectDefinitionCreate',
  );
  createdMetaobjectDefinitionIds.add(targetDefinitionId);
  createdMetaobjectDefinitionIds.add(replacementDefinitionId);

  const replacementMetaobject = await capture('replacement metaobject setup', documents.metaobjectCreate, {
    metaobject: {
      type: replacementMetaobjectType,
      handle: `replacement-${suffix}`,
      fields: [{ key: 'title', value: `Replacement ${suffix}` }],
    },
  });
  assertNoUserErrors(readPath(replacementMetaobject.response, ['data', 'metaobjectCreate']), 'metaobjectCreate');
  metaobjectId = readRequiredString(
    replacementMetaobject.response,
    ['data', 'metaobjectCreate', 'metaobject', 'id'],
    'metaobjectCreate',
  );

  const referenceDefinition = await capture(
    'metaobject reference metafield definition setup',
    documents.referenceDefinition,
    {
      definition: {
        name: 'Linked metaobject',
        namespace,
        key: 'linked',
        ownerType: 'PRODUCT',
        type: 'metaobject_reference',
        validations: [{ name: 'metaobject_definition_id', value: targetDefinitionId }],
      },
    },
  );
  assertNoUserErrors(
    readPath(referenceDefinition.response, ['data', 'metafieldDefinitionCreate']),
    'reference metafieldDefinitionCreate',
  );
  createdMetafieldDefinitionIds.add(
    readRequiredString(
      referenceDefinition.response,
      ['data', 'metafieldDefinitionCreate', 'createdDefinition', 'id'],
      'reference metafieldDefinitionCreate',
    ),
  );

  const listScalarDefinition = await capture(
    'list scalar metaobject field category setup',
    documents.listScalarDefinition,
    {
      definition: {
        type: listScalarMetaobjectType,
        name: `Metafields list category ${suffix}`,
        fieldDefinitions: [
          { key: 'text_values', name: 'Text values', type: 'list.single_line_text_field' },
          { key: 'numbers', name: 'Numbers', type: 'list.number_integer' },
          { key: 'dates', name: 'Dates', type: 'list.date' },
        ],
      },
    },
  );
  assertNoUserErrors(
    readPath(listScalarDefinition.response, ['data', 'metaobjectDefinitionCreate']),
    'list scalar metaobjectDefinitionCreate',
  );
  createdMetaobjectDefinitionIds.add(
    readRequiredString(
      listScalarDefinition.response,
      ['data', 'metaobjectDefinitionCreate', 'metaobjectDefinition', 'id'],
      'list scalar metaobjectDefinitionCreate',
    ),
  );

  const invalidSet = await capture('metafieldsSet rejected by definition validations', documents.setDefinitionValue, {
    metafields: [
      { ownerId: productId, namespace, key: 'quantity_min', type: 'number_integer', value: '1' },
      { ownerId: productId, namespace, key: 'quantity_max', type: 'number_integer', value: '6' },
      { ownerId: productId, namespace, key: 'sku', type: 'single_line_text_field', value: 'abc' },
      { ownerId: productId, namespace, key: 'color', type: 'single_line_text_field', value: 'green' },
      {
        ownerId: productId,
        namespace,
        key: 'rating',
        type: 'rating',
        value: JSON.stringify({ value: '6.0', scale_min: '1.0', scale_max: '10.0' }),
      },
      { ownerId: productId, namespace, key: 'launch_date', type: 'date', value: '2027-01-01' },
      { ownerId: productId, namespace, key: 'starts_at', type: 'date_time', value: '2027-01-01T00:00:00+00:00' },
      { ownerId: productId, namespace, key: 'linked', type: 'metaobject_reference', value: metaobjectId },
    ],
  });
  assertInvalidSetShape(invalidSet, 8);

  const validDateTimeSet = await capture('metafieldsSet accepts date_time offset', documents.setDefinitionValue, {
    metafields: [
      {
        ownerId: productId,
        namespace,
        key: 'starts_at',
        type: 'date_time',
        value: '2026-06-25T10:11:12.123+05:30',
      },
    ],
  });
  assertAcceptedMetafieldsSet(validDateTimeSet, 1, 'date_time offset metafieldsSet');

  const onlineStoreOwners = await setupOnlineStoreOwners();
  const locationId = await readLocationId();
  const marketId = await readOrCreateMarketId();
  await captureOwnerHydration([onlineStoreOwners.pageId, locationId, marketId, onlineStoreOwners.articleId]);

  const ownerTypeSet = await capture('metafieldsSet non-product owner types', documents.ownerTypesSet, {
    metafields: [
      {
        ownerId: onlineStoreOwners.pageId,
        namespace: ownerNamespace,
        key: 'page',
        type: 'single_line_text_field',
        value: 'Page subtitle',
      },
      {
        ownerId: locationId,
        namespace: ownerNamespace,
        key: 'location',
        type: 'single_line_text_field',
        value: 'Location label',
      },
      {
        ownerId: marketId,
        namespace: ownerNamespace,
        key: 'market',
        type: 'single_line_text_field',
        value: 'Market label',
      },
      {
        ownerId: onlineStoreOwners.articleId,
        namespace: ownerNamespace,
        key: 'article',
        type: 'single_line_text_field',
        value: 'Article label',
      },
    ],
  });
  assertAcceptedMetafieldsSet(ownerTypeSet, 4, 'non-product owner type metafieldsSet');

  fixture['definitionRules'] = {
    productCreate,
    createDefinitions,
    metaobjectDefinitions,
    replacementMetaobject,
    referenceDefinition,
    listScalarDefinition,
    invalidSet,
    validDateTimeSet,
  };
  fixture['ownerTypes'] = {
    onlineStoreOwners,
    locationId,
    marketId,
    metafieldsSet: ownerTypeSet,
  };
} finally {
  if (productId) {
    await cleanupCapture('cleanup product metafields', metafieldsDeleteMutation, {
      metafields: [{ ownerId: productId, namespace, key: 'starts_at' }],
    });
  }
  if (pageId || articleId || productId) {
    const metafields = [
      pageId ? { ownerId: pageId, namespace: ownerNamespace, key: 'page' } : null,
      articleId ? { ownerId: articleId, namespace: ownerNamespace, key: 'article' } : null,
    ].filter(Boolean);
    if (metafields.length > 0) {
      await cleanupCapture('cleanup online store owner metafields', metafieldsDeleteMutation, {
        metafields: metafields as JsonRecord[],
      });
    }
  }
  const ownerMetafields = [];
  const ownerTypes = readObject(fixture['ownerTypes']);
  const locationId = ownerTypes?.['locationId'];
  const marketId = ownerTypes?.['marketId'];
  if (typeof locationId === 'string') {
    ownerMetafields.push({ ownerId: locationId, namespace: ownerNamespace, key: 'location' });
  }
  if (typeof marketId === 'string') {
    ownerMetafields.push({ ownerId: marketId, namespace: ownerNamespace, key: 'market' });
  }
  if (ownerMetafields.length > 0) {
    await cleanupCapture('cleanup location and market owner metafields', metafieldsDeleteMutation, {
      metafields: ownerMetafields,
    });
  }
  for (const id of createdMetafieldDefinitionIds) {
    await cleanupCapture('cleanup metafieldDefinitionDelete', metafieldDefinitionDeleteMutation, { id });
  }
  if (metaobjectId) {
    await cleanupCapture('cleanup metaobjectDelete', metaobjectDeleteMutation, { id: metaobjectId });
  }
  for (const id of createdMetaobjectDefinitionIds) {
    await cleanupCapture('cleanup metaobjectDefinitionDelete', metaobjectDefinitionDeleteMutation, { id });
  }
  if (articleId) {
    await cleanupCapture('cleanup articleDelete', articleDeleteMutation, { id: articleId });
  }
  if (pageId) {
    await cleanupCapture('cleanup pageDelete', pageDeleteMutation, { id: pageId });
  }
  if (blogId) {
    await cleanupCapture('cleanup blogDelete', blogDeleteMutation, { id: blogId });
  }
  if (createdMarketId) {
    await cleanupCapture('cleanup marketDelete', marketDeleteMutation, { id: createdMarketId });
  }
  if (productId) {
    await cleanupCapture('cleanup productDelete', productDeleteMutation, { input: { id: productId } });
  }
}

fixture['cleanup'] = cleanup;
fixture['upstreamCalls'] = upstreamCalls;

const idDifference = (pathValue: string, resourceType: string, reason: string) => ({
  path: pathValue,
  matcher: `shopify-gid:${resourceType}`,
  reason,
});

const timestampDifference = (pathValue: string) => ({
  path: pathValue,
  matcher: 'iso-timestamp',
  reason: 'The local proxy timestamps staged metafields at replay time.',
});

const spec = {
  scenarioId: 'metafields-set-validation-gaps',
  operationNames: ['metafieldDefinitionCreate', 'metaobjectDefinitionCreate', 'metaobjectCreate', 'metafieldsSet'],
  scenarioStatus: 'captured',
  assertionKinds: [
    'definition-validation-effects',
    'field-category-parity',
    'user-errors-parity',
    'payload-shape',
    'owner-type-parity',
  ],
  liveCaptureFiles: [outputPath],
  proxyRequest: {
    documentPath: requestPaths.createDefinitions,
    variablesCapturePath: '$.definitionRules.createDefinitions.request.variables',
    apiVersion,
  },
  comparisonMode: 'captured-vs-proxy-request',
  comparison: {
    mode: 'strict-json',
    expectedDifferences: [],
    targets: [
      {
        name: 'create-definition-validation-rules',
        capturePath: '$.definitionRules.createDefinitions.response.data',
        proxyPath: '$.data',
        expectedDifferences: [
          idDifference(
            '$.quantityMin.createdDefinition.id',
            'MetafieldDefinition',
            'The proxy stages a deterministic definition ID while Shopify returned the live-store definition ID.',
          ),
          idDifference(
            '$.quantityMax.createdDefinition.id',
            'MetafieldDefinition',
            'The proxy stages a deterministic definition ID while Shopify returned the live-store definition ID.',
          ),
          idDifference(
            '$.sku.createdDefinition.id',
            'MetafieldDefinition',
            'The proxy stages a deterministic definition ID while Shopify returned the live-store definition ID.',
          ),
          idDifference(
            '$.color.createdDefinition.id',
            'MetafieldDefinition',
            'The proxy stages a deterministic definition ID while Shopify returned the live-store definition ID.',
          ),
          idDifference(
            '$.rating.createdDefinition.id',
            'MetafieldDefinition',
            'The proxy stages a deterministic definition ID while Shopify returned the live-store definition ID.',
          ),
          idDifference(
            '$.launchDate.createdDefinition.id',
            'MetafieldDefinition',
            'The proxy stages a deterministic definition ID while Shopify returned the live-store definition ID.',
          ),
          idDifference(
            '$.startsAt.createdDefinition.id',
            'MetafieldDefinition',
            'The proxy stages a deterministic definition ID while Shopify returned the live-store definition ID.',
          ),
        ],
      },
      {
        name: 'create-metaobject-reference-definitions',
        capturePath: '$.definitionRules.metaobjectDefinitions.response.data',
        proxyPath: '$.data',
        proxyRequest: {
          documentPath: requestPaths.metaobjectDefinitions,
          variablesCapturePath: '$.definitionRules.metaobjectDefinitions.request.variables',
          apiVersion,
        },
        expectedDifferences: [
          idDifference(
            '$.target.metaobjectDefinition.id',
            'MetaobjectDefinition',
            'The proxy stages a deterministic metaobject definition ID while Shopify returned the live-store definition ID.',
          ),
          idDifference(
            '$.replacement.metaobjectDefinition.id',
            'MetaobjectDefinition',
            'The proxy stages a deterministic metaobject definition ID while Shopify returned the live-store definition ID.',
          ),
        ],
      },
      {
        name: 'create-disallowed-reference-metaobject',
        capturePath: '$.definitionRules.replacementMetaobject.response.data.metaobjectCreate',
        proxyPath: '$.data.metaobjectCreate',
        proxyRequest: {
          documentPath: requestPaths.metaobjectCreate,
          variablesCapturePath: '$.definitionRules.replacementMetaobject.request.variables',
          apiVersion,
        },
        expectedDifferences: [
          idDifference(
            '$.metaobject.id',
            'Metaobject',
            'The proxy stages a deterministic metaobject ID while Shopify returned the live-store metaobject ID.',
          ),
        ],
      },
      {
        name: 'create-reference-target-definition-rule',
        capturePath: '$.definitionRules.referenceDefinition.response.data.metafieldDefinitionCreate',
        proxyPath: '$.data.metafieldDefinitionCreate',
        proxyRequest: {
          documentPath: requestPaths.referenceDefinition,
          variables: {
            definition: {
              name: { fromCapturePath: '$.definitionRules.referenceDefinition.request.variables.definition.name' },
              namespace: {
                fromCapturePath: '$.definitionRules.referenceDefinition.request.variables.definition.namespace',
              },
              key: { fromCapturePath: '$.definitionRules.referenceDefinition.request.variables.definition.key' },
              ownerType: {
                fromCapturePath: '$.definitionRules.referenceDefinition.request.variables.definition.ownerType',
              },
              type: { fromCapturePath: '$.definitionRules.referenceDefinition.request.variables.definition.type' },
              validations: [
                {
                  name: 'metaobject_definition_id',
                  value: {
                    fromProxyResponse: 'create-metaobject-reference-definitions',
                    path: '$.data.target.metaobjectDefinition.id',
                  },
                },
              ],
            },
          },
          apiVersion,
        },
        expectedDifferences: [
          idDifference(
            '$.createdDefinition.id',
            'MetafieldDefinition',
            'The proxy stages a deterministic definition ID while Shopify returned the live-store definition ID.',
          ),
          idDifference(
            '$.createdDefinition.validations[0].value',
            'MetaobjectDefinition',
            'The reference validation points at the local replay metaobject definition ID instead of the live-store ID.',
          ),
        ],
      },
      {
        name: 'list-scalar-metaobject-field-categories',
        capturePath: '$.definitionRules.listScalarDefinition.response.data.metaobjectDefinitionCreate',
        proxyPath: '$.data.metaobjectDefinitionCreate',
        proxyRequest: {
          documentPath: requestPaths.listScalarDefinition,
          variablesCapturePath: '$.definitionRules.listScalarDefinition.request.variables',
          apiVersion,
        },
        expectedDifferences: [
          idDifference(
            '$.metaobjectDefinition.id',
            'MetaobjectDefinition',
            'The proxy stages a deterministic metaobject definition ID while Shopify returned the live-store definition ID.',
          ),
        ],
      },
      {
        name: 'reject-definition-rule-violations',
        capturePath: '$.definitionRules.invalidSet.response.data.metafieldsSet',
        proxyPath: '$.data.metafieldsSet',
        proxyRequest: {
          documentPath: requestPaths.setDefinitionValue,
          variables: {
            metafields: [
              { fromCapturePath: '$.definitionRules.invalidSet.request.variables.metafields[0]' },
              { fromCapturePath: '$.definitionRules.invalidSet.request.variables.metafields[1]' },
              { fromCapturePath: '$.definitionRules.invalidSet.request.variables.metafields[2]' },
              { fromCapturePath: '$.definitionRules.invalidSet.request.variables.metafields[3]' },
              { fromCapturePath: '$.definitionRules.invalidSet.request.variables.metafields[4]' },
              { fromCapturePath: '$.definitionRules.invalidSet.request.variables.metafields[5]' },
              { fromCapturePath: '$.definitionRules.invalidSet.request.variables.metafields[6]' },
              {
                ownerId: { fromCapturePath: '$.definitionRules.invalidSet.request.variables.metafields[7].ownerId' },
                namespace: {
                  fromCapturePath: '$.definitionRules.invalidSet.request.variables.metafields[7].namespace',
                },
                key: { fromCapturePath: '$.definitionRules.invalidSet.request.variables.metafields[7].key' },
                type: { fromCapturePath: '$.definitionRules.invalidSet.request.variables.metafields[7].type' },
                value: {
                  fromProxyResponse: 'create-disallowed-reference-metaobject',
                  path: '$.data.metaobjectCreate.metaobject.id',
                },
              },
            ],
          },
          apiVersion,
        },
        expectedDifferences: [
          {
            path: '$.userErrors[*].message',
            matcher: 'any-string',
            reason:
              'Shopify emits validator-specific wording; this target compares reject/accept shape, field paths, codes, elementIndex, and empty metafields payload.',
          },
        ],
      },
      {
        name: 'accept-date-time-offset-value',
        capturePath: '$.definitionRules.validDateTimeSet.response.data.metafieldsSet',
        proxyPath: '$.data.metafieldsSet',
        proxyRequest: {
          documentPath: requestPaths.setDefinitionValue,
          variablesCapturePath: '$.definitionRules.validDateTimeSet.request.variables',
          apiVersion,
        },
        expectedDifferences: [
          idDifference(
            '$.metafields[0].id',
            'Metafield',
            'The proxy stages a deterministic metafield ID while Shopify returned the live-store metafield ID.',
          ),
          {
            path: '$.metafields[0].compareDigest',
            matcher: 'any-string',
            reason: 'The proxy computes deterministic compare digests instead of Shopify opaque digests.',
          },
          timestampDifference('$.metafields[0].createdAt'),
          timestampDifference('$.metafields[0].updatedAt'),
        ],
      },
      {
        name: 'non-product-owner-type-payloads',
        capturePath: '$.ownerTypes.metafieldsSet.response.data.metafieldsSet',
        proxyPath: '$.data.metafieldsSet',
        proxyRequest: {
          documentPath: requestPaths.ownerTypesSet,
          variablesCapturePath: '$.ownerTypes.metafieldsSet.request.variables',
          apiVersion,
        },
        expectedDifferences: [
          idDifference(
            '$.metafields[*].id',
            'Metafield',
            'The proxy stages deterministic metafield IDs while Shopify returned live-store metafield IDs.',
          ),
          {
            path: '$.metafields[*].compareDigest',
            matcher: 'any-string',
            reason: 'The proxy computes deterministic compare digests instead of Shopify opaque digests.',
          },
          timestampDifference('$.metafields[*].createdAt'),
          timestampDifference('$.metafields[*].updatedAt'),
        ],
      },
    ],
  },
  notes:
    'Live Shopify capture proves metafieldsSet rejects definition-backed numeric range, regex, choices, rating scale, date/date_time bounds, and metaobject reference target-definition violations; returns list scalar metaobject field type categories by element type; accepts a date_time value with fractional seconds and an explicit offset; and returns ownerType/owner payloads for Page, Location, Market, and Article owners.',
};

await mkdir(outputDir, { recursive: true });
await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
await mkdir(path.dirname(paritySpecPath), { recursive: true });
await writeFile(paritySpecPath, `${JSON.stringify(spec, null, 2)}\n`, 'utf8');

console.log(
  JSON.stringify(
    {
      ok: true,
      outputPath,
      paritySpecPath,
      apiVersion,
      namespace,
      ownerNamespace,
      cleanupCount: cleanup.length,
      upstreamCallCount: upstreamCalls.length,
    },
    null,
    2,
  ),
);

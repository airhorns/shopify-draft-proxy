/* oxlint-disable no-console -- CLI capture script intentionally writes progress to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type Capture = {
  query: string;
  variables: JsonRecord;
  status: number;
  response: unknown;
};

type Owner = {
  id: string;
  resourceType: 'Location' | 'Page' | 'Article' | 'Market';
  ownerType: 'LOCATION' | 'PAGE' | 'ARTICLE' | 'MARKET';
  value: string;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2025-01',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metafields');
const outputPath = path.join(outputDir, 'owner-metafield-mixed-owner-hydration.json');
const requestPaths = {
  set: 'config/parity-requests/metafields/owner-metafield-mixed-owner-set.graphql',
  read: 'config/parity-requests/metafields/owner-metafield-mixed-owner-read.graphql',
  delete: 'config/parity-requests/metafields/owner-metafield-mixed-owner-delete.graphql',
  contentCreate: 'config/parity-requests/online-store/online-store-content-create.graphql',
  articleCreate: 'config/parity-requests/online-store/online-store-content-article-create.graphql',
} as const;

const documents = Object.fromEntries(
  await Promise.all(
    Object.entries(requestPaths).map(async ([key, filePath]) => [key, await readFile(filePath, 'utf8')]),
  ),
) as Record<keyof typeof requestPaths, string>;

const locationsReadQuery = `#graphql
query OwnerMetafieldHydrationLocationsRead {
  locations(first: 1) {
    nodes { id name }
  }
}
`;

const marketsReadQuery = `#graphql
query OwnerMetafieldHydrationMarketsRead {
  markets(first: 1) {
    nodes { id name }
  }
}
`;

const definitionsCreateMutation = `#graphql
mutation OwnerMetafieldHydrationDefinitionsCreate(
  $location: MetafieldDefinitionInput!
  $page: MetafieldDefinitionInput!
  $article: MetafieldDefinitionInput!
  $market: MetafieldDefinitionInput!
) {
  location: metafieldDefinitionCreate(definition: $location) {
    createdDefinition { id }
    userErrors { field message code }
  }
  page: metafieldDefinitionCreate(definition: $page) {
    createdDefinition { id }
    userErrors { field message code }
  }
  article: metafieldDefinitionCreate(definition: $article) {
    createdDefinition { id }
    userErrors { field message code }
  }
  market: metafieldDefinitionCreate(definition: $market) {
    createdDefinition { id }
    userErrors { field message code }
  }
}
`;

const metafieldDefinitionDeleteMutation = `#graphql
mutation OwnerMetafieldHydrationDefinitionDelete($id: ID!) {
  metafieldDefinitionDelete(id: $id, deleteAllAssociatedMetafields: true) {
    deletedDefinitionId
    userErrors { field message code }
  }
}
`;

const articleDeleteMutation = `#graphql
mutation OwnerMetafieldHydrationArticleDelete($id: ID!) {
  articleDelete(id: $id) {
    deletedArticleId
    userErrors { field message }
  }
}
`;

const pageDeleteMutation = `#graphql
mutation OwnerMetafieldHydrationPageDelete($id: ID!) {
  pageDelete(id: $id) {
    deletedPageId
    userErrors { field message }
  }
}
`;

const blogDeleteMutation = `#graphql
mutation OwnerMetafieldHydrationBlogDelete($id: ID!) {
  blogDelete(id: $id) {
    deletedBlogId
    userErrors { field message }
  }
}
`;

const ownerMetafieldsHydrateQuery =
  'query OwnerMetafieldsHydrateNodes($ids: [ID!]!, $metafield0Namespace: String!, $metafield0Key: String!) { nodes(ids: $ids) { __typename id ... on HasMetafields { metafield0: metafield(namespace: $metafield0Namespace, key: $metafield0Key) { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType definition { id name namespace key ownerType type { name category } description validations { name value } pinnedPosition validationStatus } } } ... on Product { id title handle status totalInventory tracksInventory createdAt updatedAt  } ... on ProductVariant { id title sku barcode price compareAtPrice taxable inventoryPolicy inventoryQuantity selectedOptions { name value } inventoryItem { id tracked requiresShipping } product { id title handle status totalInventory tracksInventory createdAt updatedAt } } ... on Collection { id title handle } ... on Customer { id displayName email } ... on Order { id name } ... on Company { id name } ... on Page { id title } ... on Article { id title } } }';

function asRecord(value: unknown, label: string): JsonRecord {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`${label} was not an object: ${JSON.stringify(value)}`);
  }
  return value as JsonRecord;
}

function atPath(value: unknown, pathSegments: Array<string | number>): unknown {
  let current = value;
  for (const segment of pathSegments) {
    current =
      typeof segment === 'number'
        ? Array.isArray(current)
          ? current[segment]
          : undefined
        : current && typeof current === 'object' && !Array.isArray(current)
          ? (current as JsonRecord)[segment]
          : undefined;
  }
  return current;
}

function requiredString(value: unknown, pathSegments: Array<string | number>, label: string): string {
  const result = atPath(value, pathSegments);
  if (typeof result !== 'string' || result.length === 0) {
    throw new Error(`${label} missing ${pathSegments.join('.')}: ${JSON.stringify(value)}`);
  }
  return result;
}

function assertNoUserErrors(value: unknown, pathSegments: Array<string | number>, label: string): void {
  const userErrors = atPath(value, pathSegments);
  if (!Array.isArray(userErrors) || userErrors.length !== 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors)}`);
  }
}

function assertNoTopLevelErrors(capture: Capture, label: string): void {
  if (capture.status < 200 || capture.status >= 300 || asRecord(capture.response, label)['errors']) {
    throw new Error(`${label} failed: ${JSON.stringify(capture, null, 2)}`);
  }
}

async function capture(query: string, variables: JsonRecord, label: string): Promise<Capture> {
  const result = await runGraphqlRaw(query, variables);
  const captured = { query, variables, status: result.status, response: result.payload };
  assertNoTopLevelErrors(captured, label);
  return captured;
}

async function cleanupCapture(cleanup: Capture[], query: string, variables: JsonRecord, label: string): Promise<void> {
  try {
    cleanup.push(await capture(query, variables, label));
  } catch (error) {
    console.warn(JSON.stringify({ ok: false, cleanup: label, error: String(error) }));
  }
}

function metafieldsSetInputs(owners: Owner[], namespace: string, digests?: Map<string, string>): JsonRecord[] {
  return owners.map((owner) => ({
    ownerId: owner.id,
    namespace,
    key: 'owner_state',
    type: 'single_line_text_field',
    value: owner.value,
    ...(digests ? { compareDigest: digests.get(owner.id) } : {}),
  }));
}

function metafieldIdentifiers(owners: Owner[], namespace: string): JsonRecord[] {
  return owners.map((owner) => ({ ownerId: owner.id, namespace, key: 'owner_state' }));
}

function readVariables(owners: Owner[], namespace: string): JsonRecord {
  const byType = new Map(owners.map((owner) => [owner.resourceType, owner.id]));
  return {
    locationId: byType.get('Location'),
    pageId: byType.get('Page'),
    articleId: byType.get('Article'),
    marketId: byType.get('Market'),
    namespace,
    key: 'owner_state',
  };
}

function metafieldDigestsFromNodes(captureValue: Capture): Map<string, string> {
  const nodes = atPath(captureValue.response, ['data', 'nodes']);
  if (!Array.isArray(nodes)) throw new Error(`hydration nodes missing: ${JSON.stringify(captureValue.response)}`);
  return new Map(
    nodes.map((node) => [
      requiredString(node, ['id'], 'hydrated owner id'),
      requiredString(node, ['metafield0', 'compareDigest'], 'hydrated metafield compareDigest'),
    ]),
  );
}

function metafieldDigestsFromSet(captureValue: Capture): Map<string, string> {
  const inputs = captureValue.variables['metafields'];
  const metafields = atPath(captureValue.response, ['data', 'metafieldsSet', 'metafields']);
  if (!Array.isArray(inputs) || !Array.isArray(metafields) || inputs.length !== metafields.length) {
    throw new Error(`metafieldsSet response did not match inputs: ${JSON.stringify(captureValue)}`);
  }
  return new Map(
    metafields.map((metafield, index) => [
      requiredString(inputs[index], ['ownerId'], 'metafieldsSet input ownerId'),
      requiredString(metafield, ['compareDigest'], 'metafieldsSet compareDigest'),
    ]),
  );
}

const suffix = Date.now().toString(36);
const namespace = `owner_hydration_${suffix}`;
const cleanup: Capture[] = [];
const definitionIds: string[] = [];
let pageId: string | null = null;
let articleId: string | null = null;
let blogId: string | null = null;
let owners: Owner[] = [];
let fixture: JsonRecord = {};

try {
  const definitions = await capture(
    definitionsCreateMutation,
    Object.fromEntries(
      [
        ['location', 'LOCATION'],
        ['page', 'PAGE'],
        ['article', 'ARTICLE'],
        ['market', 'MARKET'],
      ].map(([alias, ownerType]) => [
        alias,
        {
          name: `Owner hydration ${alias} ${suffix}`,
          namespace,
          key: 'owner_state',
          description: 'Authoritative owner metafield hydration parity',
          ownerType,
          type: 'single_line_text_field',
        },
      ]),
    ),
    'metafield definition setup',
  );
  for (const alias of ['location', 'page', 'article', 'market']) {
    assertNoUserErrors(definitions.response, ['data', alias, 'userErrors'], `${alias} definition setup`);
    definitionIds.push(requiredString(definitions.response, ['data', alias, 'createdDefinition', 'id'], alias));
  }

  const contentCreateVariables = {
    blog: { title: `Owner hydration blog ${suffix}`, commentPolicy: 'MODERATED' },
    page: {
      title: `Owner hydration page ${suffix}`,
      body: '<p>Owner metafield hydration capture</p>',
      isPublished: false,
    },
  };
  const contentCreate = await capture(documents.contentCreate, contentCreateVariables, 'page and blog owner setup');
  assertNoUserErrors(contentCreate.response, ['data', 'blogCreate', 'userErrors'], 'blogCreate');
  assertNoUserErrors(contentCreate.response, ['data', 'pageCreate', 'userErrors'], 'pageCreate');
  blogId = requiredString(contentCreate.response, ['data', 'blogCreate', 'blog', 'id'], 'blogCreate');
  pageId = requiredString(contentCreate.response, ['data', 'pageCreate', 'page', 'id'], 'pageCreate');

  const articleCreateVariables = {
    article: {
      blogId,
      title: `Owner hydration article ${suffix}`,
      body: '<p>Owner metafield hydration capture</p>',
      summary: '<p>Mixed owner CAS capture</p>',
      isPublished: false,
      author: { name: 'Owner Metafield Capture' },
    },
  };
  const articleCreate = await capture(documents.articleCreate, articleCreateVariables, 'article owner setup');
  assertNoUserErrors(articleCreate.response, ['data', 'articleCreate', 'userErrors'], 'articleCreate');
  articleId = requiredString(articleCreate.response, ['data', 'articleCreate', 'article', 'id'], 'articleCreate');

  const locationRead = await capture(locationsReadQuery, {}, 'location owner read');
  const locationId = requiredString(locationRead.response, ['data', 'locations', 'nodes', 0, 'id'], 'location');
  const marketRead = await capture(marketsReadQuery, {}, 'market owner read');
  const marketId = requiredString(marketRead.response, ['data', 'markets', 'nodes', 0, 'id'], 'market');

  owners = [
    { id: locationId, resourceType: 'Location', ownerType: 'LOCATION', value: `Location ${suffix}` },
    { id: pageId, resourceType: 'Page', ownerType: 'PAGE', value: `Page ${suffix}` },
    { id: articleId, resourceType: 'Article', ownerType: 'ARTICLE', value: `Article ${suffix}` },
    { id: marketId, resourceType: 'Market', ownerType: 'MARKET', value: `Market ${suffix}` },
  ];

  const baselineSet = await capture(
    documents.set,
    { metafields: metafieldsSetInputs(owners, namespace) },
    'baseline mixed-owner metafieldsSet',
  );
  assertNoUserErrors(baselineSet.response, ['data', 'metafieldsSet', 'userErrors'], 'baseline metafieldsSet');

  const sortedIds = owners.map((owner) => owner.id).sort();
  const hydration = await capture(
    ownerMetafieldsHydrateQuery,
    { ids: sortedIds, metafield0Namespace: namespace, metafield0Key: 'owner_state' },
    'owner metafield hydration cassette',
  );
  const hydratedDigests = metafieldDigestsFromNodes(hydration);
  const validSet = await capture(
    documents.set,
    { metafields: metafieldsSetInputs(owners, namespace, hydratedDigests) },
    'valid mixed-owner CAS',
  );
  assertNoUserErrors(validSet.response, ['data', 'metafieldsSet', 'userErrors'], 'valid mixed-owner CAS');

  const ownerReadVariables = readVariables(owners, namespace);
  const readAfterValid = await capture(documents.read, ownerReadVariables, 'read after valid mixed-owner CAS');
  for (const root of ['location', 'page', 'article', 'market']) {
    requiredString(readAfterValid.response, ['data', root, 'metafield', 'id'], `${root} metafield read`);
  }

  const currentDigests = metafieldDigestsFromSet(validSet);
  const marketOwner = owners.find((owner) => owner.resourceType === 'Market');
  if (!marketOwner) throw new Error('Market owner missing after setup');
  const marketAdvanceOwner: Owner = {
    ...marketOwner,
    value: `Market advanced ${suffix}`,
  };
  const marketAdvanceSet = await capture(
    documents.set,
    {
      metafields: metafieldsSetInputs(
        [marketAdvanceOwner],
        namespace,
        new Map([[marketId, requiredString(currentDigests.get(marketId), [], 'current Market digest')]]),
      ),
    },
    'valid Market CAS update',
  );
  assertNoUserErrors(marketAdvanceSet.response, ['data', 'metafieldsSet', 'userErrors'], 'valid Market CAS update');
  const readAfterMarketAdvance = await capture(
    documents.read,
    ownerReadVariables,
    'read after valid Market CAS update',
  );

  const staleDigests = new Map(currentDigests);
  staleDigests.set(marketId, requiredString(currentDigests.get(marketId), [], 'stale Market digest'));
  const staleSet = await capture(
    documents.set,
    { metafields: metafieldsSetInputs(owners, namespace, staleDigests) },
    'stale mixed-owner CAS',
  );
  const staleMetafields = atPath(staleSet.response, ['data', 'metafieldsSet', 'metafields']);
  const staleErrors = atPath(staleSet.response, ['data', 'metafieldsSet', 'userErrors']);
  if (
    !Array.isArray(staleMetafields) ||
    staleMetafields.length !== 0 ||
    !Array.isArray(staleErrors) ||
    staleErrors.length !== 1 ||
    atPath(staleErrors, [0, 'code']) !== 'STALE_OBJECT' ||
    JSON.stringify(atPath(staleErrors, [0, 'field'])) !== JSON.stringify(['metafields', '3']) ||
    atPath(staleErrors, [0, 'elementIndex']) !== null
  ) {
    throw new Error(`unexpected stale mixed-owner CAS payload: ${JSON.stringify(staleSet.response, null, 2)}`);
  }

  const readAfterStale = await capture(documents.read, ownerReadVariables, 'read after stale mixed-owner CAS');
  if (
    JSON.stringify(atPath(readAfterMarketAdvance.response, ['data'])) !==
    JSON.stringify(atPath(readAfterStale.response, ['data']))
  ) {
    throw new Error('stale mixed-owner CAS changed owner metafield state');
  }

  const deleteResult = await capture(
    documents.delete,
    { metafields: metafieldIdentifiers(owners, namespace) },
    'mixed-owner metafieldsDelete',
  );
  assertNoUserErrors(deleteResult.response, ['data', 'metafieldsDelete', 'userErrors'], 'metafieldsDelete');
  const deleted = atPath(deleteResult.response, ['data', 'metafieldsDelete', 'deletedMetafields']);
  if (!Array.isArray(deleted) || deleted.length !== owners.length) {
    throw new Error(`metafieldsDelete did not delete every owner metafield: ${JSON.stringify(deleteResult.response)}`);
  }

  const readAfterDelete = await capture(documents.read, ownerReadVariables, 'read after mixed-owner delete');
  for (const root of ['location', 'page', 'article', 'market']) {
    if (atPath(readAfterDelete.response, ['data', root, 'metafield']) !== null) {
      throw new Error(`${root} metafield remained after delete: ${JSON.stringify(readAfterDelete.response)}`);
    }
  }

  fixture = {
    storeDomain,
    apiVersion,
    capturedAt: new Date().toISOString(),
    namespace,
    setup: { definitions, contentCreate, articleCreate, locationRead, marketRead, baselineSet },
    owners,
    validSet,
    readAfterValid,
    marketAdvanceSet,
    readAfterMarketAdvance,
    staleSet,
    readAfterStale,
    delete: deleteResult,
    readAfterDelete,
    upstreamCalls: [
      {
        operationName: 'OwnerMetafieldsHydrateNodes',
        variables: hydration.variables,
        query: hydration.query,
        response: { status: hydration.status, body: hydration.response },
      },
    ],
  };
} finally {
  if (owners.length > 0) {
    await cleanupCapture(
      cleanup,
      documents.delete,
      { metafields: metafieldIdentifiers(owners, namespace) },
      'cleanup owner metafields',
    );
  }
  for (const id of definitionIds) {
    await cleanupCapture(cleanup, metafieldDefinitionDeleteMutation, { id }, 'cleanup metafield definition');
  }
  if (articleId) await cleanupCapture(cleanup, articleDeleteMutation, { id: articleId }, 'cleanup article');
  if (pageId) await cleanupCapture(cleanup, pageDeleteMutation, { id: pageId }, 'cleanup page');
  if (blogId) await cleanupCapture(cleanup, blogDeleteMutation, { id: blogId }, 'cleanup blog');
}

fixture['cleanup'] = cleanup;
await mkdir(outputDir, { recursive: true });
await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
console.log(JSON.stringify({ ok: true, outputPath, ownerCount: owners.length, cleanupCount: cleanup.length }, null, 2));

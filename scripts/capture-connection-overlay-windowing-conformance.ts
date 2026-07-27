/* oxlint-disable no-console -- CLI capture scripts intentionally report status. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;
type Capture = {
  request: { query: string; variables: JsonRecord };
  status: number;
  response: unknown;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const client = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const savedSearchCreateDocument = `mutation ConnectionOverlaySavedSearchCreate(
  $first: SavedSearchCreateInput!
  $second: SavedSearchCreateInput!
) {
  first: savedSearchCreate(input: $first) {
    savedSearch { id name }
    userErrors { field message }
  }
  second: savedSearchCreate(input: $second) {
    savedSearch { id name }
    userErrors { field message }
  }
}
`;

const fileCreateDocument = `mutation ConnectionOverlayFileCreate($files: [FileCreateInput!]!) {
  fileCreate(files: $files) {
    files { __typename id alt }
    userErrors { field message code }
  }
}
`;

const sellingPlanGroupCreateDocument = `mutation ConnectionOverlaySellingPlanGroupCreate(
  $first: SellingPlanGroupInput!
  $second: SellingPlanGroupInput!
) {
  first: sellingPlanGroupCreate(input: $first) {
    sellingPlanGroup { id name merchantCode }
    userErrors { field message code }
  }
  second: sellingPlanGroupCreate(input: $second) {
    sellingPlanGroup { id name merchantCode }
    userErrors { field message code }
  }
}
`;

const savedSearchBaselineDocument = `query SavedSearchConnectionBaseline($first: Int!, $after: String) {
  savedSearchBaseline: productSavedSearches(first: $first, after: $after) {
    edges { cursor node { id name query resourceType } }
    pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
  }
}`;

const mediaFilesBaselineDocument = `
query MediaFilesConnectionBaseline(
  $first: Int!
  $after: String
  $query: String
  $sortKey: FileSortKeys
) {
  filesBaseline: files(first: $first, after: $after, query: $query, sortKey: $sortKey) {
    edges {
      cursor
      node {
        __typename
        id
        alt
        createdAt
        updatedAt
        fileStatus
        ... on MediaImage {
          image { url width height }
          preview { image { url width height } }
        }
        ... on GenericFile { url }
        ... on Video {
          preview { image { url width height } }
          sources { url mimeType format height width }
        }
        ... on ExternalVideo {
          embeddedUrl
          host
          originUrl
          preview { image { url width height } }
        }
        ... on Model3d {
          preview { image { url width height } }
          sources { url mimeType format filesize }
        }
      }
    }
    pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
  }
}
`;

const sellingPlanGroupsWindowDocument = `
query SellingPlanGroupsConnectionWindow(
  $first: Int
  $after: String
  $last: Int
  $before: String
  $query: String
  $sortKey: SellingPlanGroupSortKeys
  $reverse: Boolean!
) {
  sellingPlanGroupsWindow: sellingPlanGroups(
    first: $first
    after: $after
    last: $last
    before: $before
    query: $query
    sortKey: $sortKey
    reverse: $reverse
  ) {
    edges {
      cursor
      node {
        __typename
        id
        appId
        name
        merchantCode
        description
        options
        position
        createdAt
        productsCount { count precision }
        productVariantsCount { count precision }
        sellingPlans(first: 31) {
          edges {
            cursor
            node {
              __typename
              id
              name
              description
              options
              position
              category
              createdAt
              billingPolicy {
                __typename
                ... on SellingPlanRecurringBillingPolicy { interval intervalCount minCycles maxCycles }
              }
              deliveryPolicy {
                __typename
                ... on SellingPlanRecurringDeliveryPolicy { interval intervalCount cutoff intent preAnchorBehavior }
              }
              inventoryPolicy { reserve }
              pricingPolicies {
                __typename
                ... on SellingPlanFixedPricingPolicy {
                  adjustmentType
                  adjustmentValue {
                    __typename
                    ... on SellingPlanPricingPolicyPercentageValue { percentage }
                    ... on MoneyV2 { amount currencyCode }
                  }
                }
                ... on SellingPlanRecurringPricingPolicy {
                  afterCycle
                  createdAt
                  adjustmentType
                  adjustmentValue {
                    __typename
                    ... on SellingPlanPricingPolicyPercentageValue { percentage }
                    ... on MoneyV2 { amount currencyCode }
                  }
                }
              }
            }
          }
          pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
        }
      }
    }
    pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
  }
}
`;

const fullReadDocument = `query ConnectionOverlayFullRead($fileQuery: String!, $groupQuery: String!) {
  saved: productSavedSearches(first: 10) {
    edges { cursor node { id name } }
    pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
  }
  savedReverse: productSavedSearches(first: 1, reverse: true) {
    edges { cursor node { id name } }
    pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
  }
  files: files(first: 10, query: $fileQuery, sortKey: ID) {
    edges { cursor node { __typename id alt } }
    pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
  }
  filesReverse: files(first: 1, query: $fileQuery, sortKey: ID, reverse: true) {
    edges { cursor node { __typename id alt } }
    pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
  }
  groups: sellingPlanGroups(first: 10, query: $groupQuery, sortKey: ID) {
    edges { cursor node { id name merchantCode } }
    pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
  }
  groupsReverse: sellingPlanGroups(first: 1, query: $groupQuery, sortKey: ID, reverse: true) {
    edges { cursor node { id name merchantCode } }
    pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
  }
}
`;

const afterDeleteReadDocument = `query ConnectionOverlayAfterDeleteRead($fileQuery: String!, $groupQuery: String!) {
  saved: productSavedSearches(first: 10) {
    edges { cursor node { id name } }
    pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
  }
  files: files(first: 10, query: $fileQuery, sortKey: ID) {
    edges { cursor node { __typename id alt } }
    pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
  }
  groups: sellingPlanGroups(first: 10, query: $groupQuery, sortKey: ID) {
    edges { cursor node { id name merchantCode } }
    pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
  }
}
`;

const afterReadDocument = `query ConnectionOverlayAfterRead(
  $savedAfter: String!
  $fileAfter: String!
  $groupAfter: String!
  $fileQuery: String!
  $groupQuery: String!
) {
  saved: productSavedSearches(first: 1, after: $savedAfter) {
    edges { cursor node { id name } }
    pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
  }
  files: files(first: 1, after: $fileAfter, query: $fileQuery, sortKey: ID) {
    edges { cursor node { __typename id alt } }
    pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
  }
  groups: sellingPlanGroups(first: 1, after: $groupAfter, query: $groupQuery, sortKey: ID) {
    edges { cursor node { id name merchantCode } }
    pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
  }
}
`;

const beforeReadDocument = `query ConnectionOverlayBeforeRead(
  $savedBefore: String!
  $fileBefore: String!
  $groupBefore: String!
  $fileQuery: String!
  $groupQuery: String!
) {
  saved: productSavedSearches(last: 1, before: $savedBefore) {
    edges { cursor node { id name } }
    pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
  }
  files: files(last: 1, before: $fileBefore, query: $fileQuery, sortKey: ID) {
    edges { cursor node { __typename id alt } }
    pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
  }
  groups: sellingPlanGroups(last: 1, before: $groupBefore, query: $groupQuery, sortKey: ID) {
    edges { cursor node { id name merchantCode } }
    pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
  }
}
`;

const deleteDocument = `mutation ConnectionOverlayDelete(
  $savedSearch: SavedSearchDeleteInput!
  $fileIds: [ID!]!
  $sellingPlanGroupId: ID!
) {
  savedSearchDelete(input: $savedSearch) {
    deletedSavedSearchId
    userErrors { field message }
  }
  fileDelete(fileIds: $fileIds) {
    deletedFileIds
    userErrors { field message code }
  }
  sellingPlanGroupDelete(id: $sellingPlanGroupId) {
    deletedSellingPlanGroupId
    userErrors { field message code }
  }
}
`;

const savedSearchCatalogDocument = `query ConnectionOverlaySavedSearchCleanup {
  productSavedSearches(first: 100) { nodes { id } }
}
`;

function asObject(value: unknown, label: string): JsonRecord {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`Expected ${label} to be an object.`);
  }
  return value as JsonRecord;
}

function asArray(value: unknown, label: string): unknown[] {
  if (!Array.isArray(value)) {
    throw new Error(`Expected ${label} to be an array.`);
  }
  return value;
}

function data(capture: Capture): JsonRecord {
  return asObject(asObject(capture.response, 'response')['data'], 'response.data');
}

function root(capture: Capture, name: string): JsonRecord {
  return asObject(data(capture)[name], `response.data.${name}`);
}

function nestedId(capture: Capture, rootName: string, resourceName: string): string {
  const value = asObject(root(capture, rootName)[resourceName], resourceName)['id'];
  if (typeof value !== 'string') {
    throw new Error(`Expected ${rootName}.${resourceName}.id.`);
  }
  return value;
}

function edgeCursor(capture: Capture, rootName: string, index: number): string {
  const edges = asArray(root(capture, rootName)['edges'], `${rootName}.edges`);
  const cursor = asObject(edges[index], `${rootName}.edges[${index}]`)['cursor'];
  if (typeof cursor !== 'string' || cursor.length === 0) {
    throw new Error(`Expected ${rootName}.edges[${index}].cursor.`);
  }
  return cursor;
}

function assertNoErrors(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertEmptyUserErrors(capture: Capture, rootNames: string[]): void {
  for (const rootName of rootNames) {
    const errors = root(capture, rootName)['userErrors'];
    if (!Array.isArray(errors) || errors.length !== 0) {
      throw new Error(`${rootName} returned userErrors: ${JSON.stringify(errors)}`);
    }
  }
}

function upstreamCall(capture: Capture, operationName: string): JsonRecord {
  return {
    operationName,
    variables: capture.request.variables,
    query: capture.request.query,
    response: { status: capture.status, body: capture.response },
  };
}

async function capture(query: string, variables: JsonRecord, label: string): Promise<Capture> {
  const result = await client.runGraphqlRequest(query, variables);
  assertNoErrors(result, label);
  return {
    request: { query, variables },
    status: result.status,
    response: result.payload,
  };
}

async function cleanupSavedSearches(): Promise<void> {
  const catalog = await capture(savedSearchCatalogDocument, {}, 'saved-search cleanup catalog');
  const nodes = asArray(root(catalog, 'productSavedSearches')['nodes'], 'productSavedSearches.nodes');
  for (const node of nodes) {
    const id = asObject(node, 'saved-search cleanup node')['id'];
    if (typeof id === 'string') {
      await client.runGraphqlRequest(
        `mutation ConnectionOverlaySavedSearchCleanup($input: SavedSearchDeleteInput!) {
          savedSearchDelete(input: $input) { deletedSavedSearchId userErrors { field message } }
        }`,
        { input: { id } },
      );
    }
  }
}

function sellingPlanGroupInput(name: string, merchantCode: string): JsonRecord {
  return {
    name,
    merchantCode,
    description: 'Disposable connection overlay pagination evidence',
    options: ['Delivery frequency'],
    position: 1,
    sellingPlansToCreate: [
      {
        name: `${name} monthly`,
        description: 'Monthly delivery',
        options: ['Monthly'],
        position: 1,
        category: 'SUBSCRIPTION',
        billingPolicy: { recurring: { interval: 'MONTH', intervalCount: 1 } },
        deliveryPolicy: { recurring: { interval: 'MONTH', intervalCount: 1, cutoff: 0 } },
        inventoryPolicy: { reserve: 'ON_FULFILLMENT' },
        pricingPolicies: [],
      },
    ],
  };
}

const suffix = Date.now().toString(36);
const firstSavedSearchName = `Connection overlay ${suffix} A`;
const secondSavedSearchName = `Connection overlay ${suffix} B`;
const firstFileAlt = `Connection overlay ${suffix} A`;
const secondFileAlt = `Connection overlay ${suffix} B`;
const filePrefix = `connection-overlay-${suffix}`;
const groupPrefix = `connection-overlay-${suffix}`;
const firstGroupName = `${groupPrefix}-a`;
const secondGroupName = `${groupPrefix}-b`;
const fileQuery = `filename:${filePrefix}*`;
const groupQuery = `name:${groupPrefix}*`;

let firstSavedSearchId: string | null = null;
let secondSavedSearchId: string | null = null;
let firstFileId: string | null = null;
let secondFileId: string | null = null;
let firstGroupId: string | null = null;
let secondGroupId: string | null = null;
const cleanup: Array<{ label: string; response: unknown }> = [];

try {
  await cleanupSavedSearches();
  const preCreateBaselines: Array<{ operationName: string; capture: Capture }> = [];
  const sellingPlanGroupWindowVariables: JsonRecord[] = [
    {
      first: 12,
      after: null,
      last: null,
      before: null,
      query: groupQuery,
      sortKey: 'ID',
      reverse: false,
    },
    {
      first: 3,
      after: null,
      last: null,
      before: null,
      query: groupQuery,
      sortKey: 'ID',
      reverse: true,
    },
    {
      first: 3,
      after: null,
      last: null,
      before: null,
      query: groupQuery,
      sortKey: 'ID',
      reverse: false,
    },
    {
      first: 3,
      after: null,
      last: null,
      before: null,
      query: groupQuery,
      sortKey: 'ID',
      reverse: false,
    },
    {
      first: 12,
      after: null,
      last: null,
      before: null,
      query: groupQuery,
      sortKey: 'ID',
      reverse: false,
    },
  ];
  for (let round = 1; round <= 5; round += 1) {
    preCreateBaselines.push(
      {
        operationName: 'SavedSearchConnectionBaseline',
        capture: await capture(
          savedSearchBaselineDocument,
          { first: 250, after: null },
          `saved-search pre-create baseline ${round}`,
        ),
      },
      {
        operationName: 'MediaFilesConnectionBaseline',
        capture: await capture(
          mediaFilesBaselineDocument,
          { first: 250, after: null, query: fileQuery, sortKey: 'ID' },
          `files pre-create baseline ${round}`,
        ),
      },
      {
        operationName: 'SellingPlanGroupsConnectionWindow',
        capture: await capture(
          sellingPlanGroupsWindowDocument,
          sellingPlanGroupWindowVariables[round - 1]!,
          `selling-plan-groups bounded overlay window ${round}`,
        ),
      },
    );
  }
  const savedSearchCreate = await capture(
    savedSearchCreateDocument,
    {
      first: { resourceType: 'PRODUCT', name: firstSavedSearchName, query: `title:${suffix}-a` },
      second: { resourceType: 'PRODUCT', name: secondSavedSearchName, query: `title:${suffix}-b` },
    },
    'saved-search create',
  );
  assertEmptyUserErrors(savedSearchCreate, ['first', 'second']);
  firstSavedSearchId = nestedId(savedSearchCreate, 'first', 'savedSearch');
  secondSavedSearchId = nestedId(savedSearchCreate, 'second', 'savedSearch');

  const fileCreate = await capture(
    fileCreateDocument,
    {
      files: [
        {
          alt: firstFileAlt,
          contentType: 'FILE',
          filename: `${filePrefix}-a.pdf`,
          originalSource: 'https://www.w3.org/WAI/ER/tests/xhtml/testfiles/resources/pdf/dummy.pdf',
        },
        {
          alt: secondFileAlt,
          contentType: 'FILE',
          filename: `${filePrefix}-b.pdf`,
          originalSource: 'https://www.w3.org/WAI/ER/tests/xhtml/testfiles/resources/pdf/dummy.pdf',
        },
      ],
    },
    'file create',
  );
  assertEmptyUserErrors(fileCreate, ['fileCreate']);
  const createdFiles = asArray(root(fileCreate, 'fileCreate')['files'], 'fileCreate.files');
  firstFileId = asObject(createdFiles[0], 'fileCreate.files[0]')['id'] as string;
  secondFileId = asObject(createdFiles[1], 'fileCreate.files[1]')['id'] as string;

  const sellingPlanGroupCreate = await capture(
    sellingPlanGroupCreateDocument,
    {
      first: sellingPlanGroupInput(firstGroupName, `${groupPrefix}-a`),
      second: sellingPlanGroupInput(secondGroupName, `${groupPrefix}-b`),
    },
    'selling-plan-group create',
  );
  assertEmptyUserErrors(sellingPlanGroupCreate, ['first', 'second']);
  firstGroupId = nestedId(sellingPlanGroupCreate, 'first', 'sellingPlanGroup');
  secondGroupId = nestedId(sellingPlanGroupCreate, 'second', 'sellingPlanGroup');

  let fullRead: Capture | null = null;
  for (let attempt = 1; attempt <= 24; attempt += 1) {
    fullRead = await capture(fullReadDocument, { fileQuery, groupQuery }, `full read attempt ${attempt}`);
    const hasTwo = ['saved', 'files', 'groups'].every(
      (name) => asArray(root(fullRead!, name)['edges'], `${name}.edges`).length === 2,
    );
    if (hasTwo) break;
    await sleep(5000);
  }
  if (
    !fullRead ||
    !['saved', 'files', 'groups'].every((name) => asArray(root(fullRead!, name)['edges'], `${name}.edges`).length === 2)
  ) {
    throw new Error(`Connections did not stabilize with two rows: ${JSON.stringify(fullRead, null, 2)}`);
  }

  const afterRead = await capture(
    afterReadDocument,
    {
      savedAfter: edgeCursor(fullRead, 'saved', 0),
      fileAfter: edgeCursor(fullRead, 'files', 0),
      groupAfter: edgeCursor(fullRead, 'groups', 0),
      fileQuery,
      groupQuery,
    },
    'after read',
  );
  const beforeRead = await capture(
    beforeReadDocument,
    {
      savedBefore: edgeCursor(fullRead, 'saved', 1),
      fileBefore: edgeCursor(fullRead, 'files', 1),
      groupBefore: edgeCursor(fullRead, 'groups', 1),
      fileQuery,
      groupQuery,
    },
    'before read',
  );

  const deleteSecond = await capture(
    deleteDocument,
    {
      savedSearch: { id: secondSavedSearchId },
      fileIds: [secondFileId],
      sellingPlanGroupId: secondGroupId,
    },
    'delete second records',
  );
  assertEmptyUserErrors(deleteSecond, ['savedSearchDelete', 'fileDelete', 'sellingPlanGroupDelete']);
  secondSavedSearchId = null;
  secondFileId = null;
  secondGroupId = null;

  const afterDeleteRead = await capture(afterDeleteReadDocument, { fileQuery, groupQuery }, 'read after delete');

  const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'admin-platform');
  const requestDir = path.join('config', 'parity-requests', 'admin-platform');
  const specDir = path.join('config', 'parity-specs', 'admin-platform');
  await Promise.all([
    mkdir(fixtureDir, { recursive: true }),
    mkdir(requestDir, { recursive: true }),
    mkdir(specDir, { recursive: true }),
  ]);

  const fixturePath = path.join(fixtureDir, 'connection-overlay-windowing.json');
  const requestPaths = {
    savedSearchCreate: path.join(requestDir, 'connection-overlay-windowing-saved-search-create.graphql'),
    fileCreate: path.join(requestDir, 'connection-overlay-windowing-file-create.graphql'),
    sellingPlanGroupCreate: path.join(requestDir, 'connection-overlay-windowing-selling-plan-group-create.graphql'),
    fullRead: path.join(requestDir, 'connection-overlay-windowing-full-read.graphql'),
    afterDeleteRead: path.join(requestDir, 'connection-overlay-windowing-after-delete-read.graphql'),
    afterRead: path.join(requestDir, 'connection-overlay-windowing-after-read.graphql'),
    beforeRead: path.join(requestDir, 'connection-overlay-windowing-before-read.graphql'),
    delete: path.join(requestDir, 'connection-overlay-windowing-delete.graphql'),
  };
  const fixture = {
    metadata: {
      source: 'live-shopify-admin-graphql',
      storeDomain,
      apiVersion,
      capturedAt: new Date().toISOString(),
      scenario: 'connection-overlay-windowing',
    },
    savedSearchCreate,
    fileCreate,
    sellingPlanGroupCreate,
    fullRead,
    afterRead,
    beforeRead,
    deleteSecond,
    afterDeleteRead,
    cleanup,
    preCreateBaselines,
    upstreamCalls: preCreateBaselines.map(({ operationName, capture: baseline }) =>
      upstreamCall(baseline, operationName),
    ),
  };
  await writeFile(fixturePath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  await Promise.all([
    writeFile(requestPaths.savedSearchCreate, savedSearchCreateDocument, 'utf8'),
    writeFile(requestPaths.fileCreate, fileCreateDocument, 'utf8'),
    writeFile(requestPaths.sellingPlanGroupCreate, sellingPlanGroupCreateDocument, 'utf8'),
    writeFile(requestPaths.fullRead, fullReadDocument, 'utf8'),
    writeFile(requestPaths.afterDeleteRead, afterDeleteReadDocument, 'utf8'),
    writeFile(requestPaths.afterRead, afterReadDocument, 'utf8'),
    writeFile(requestPaths.beforeRead, beforeReadDocument, 'utf8'),
    writeFile(requestPaths.delete, deleteDocument, 'utf8'),
  ]);

  const cursorDifferences = (rootName: string) => [
    {
      path: `$.${rootName}.edges[*].cursor`,
      ignore: true,
      regrettable: true,
      reason: 'Shopify connection cursors are opaque and resource-instance specific.',
    },
    {
      path: `$.${rootName}.pageInfo.startCursor`,
      ignore: true,
      regrettable: true,
      reason: 'Shopify connection cursors are opaque and resource-instance specific.',
    },
    {
      path: `$.${rootName}.pageInfo.endCursor`,
      ignore: true,
      regrettable: true,
      reason: 'Shopify connection cursors are opaque and resource-instance specific.',
    },
  ];
  const connectionDifferences = (rootNames: string[]) => [
    ...rootNames.flatMap(cursorDifferences),
    ...rootNames.map((rootName) => {
      const matcher = rootName.startsWith('saved')
        ? 'shopify-gid:SavedSearch'
        : rootName.startsWith('files')
          ? 'shopify-gid:GenericFile'
          : 'shopify-gid:SellingPlanGroup';
      return {
        path: `$.${rootName}.edges[*].node.id`,
        matcher,
        reason: 'The proxy and live store create different resources of the same Shopify type.',
      };
    }),
  ];
  const specPath = path.join(specDir, 'connection-overlay-windowing.json');
  const spec = {
    scenarioId: 'connection-overlay-windowing',
    operationNames: [
      'productSavedSearches',
      'savedSearchCreate',
      'savedSearchDelete',
      'files',
      'fileCreate',
      'fileDelete',
      'sellingPlanGroups',
      'sellingPlanGroupCreate',
      'sellingPlanGroupDelete',
    ],
    scenarioStatus: 'captured',
    assertionKinds: ['pagination-shape', 'downstream-read-parity', 'mutation-lifecycle'],
    liveCaptureFiles: [fixturePath],
    runtimeTestFiles: [
      'tests/graphql_routes/products_saved_searches.rs',
      'tests/graphql_routes/marketing_inventory_online_store.rs',
      'tests/graphql_routes/selling_plans.rs',
    ],
    proxyRequest: {
      documentPath: requestPaths.savedSearchCreate,
      variablesCapturePath: '$.savedSearchCreate.request.variables',
      apiVersion,
    },
    comparisonMode: 'captured-vs-proxy-request',
    notes:
      'Live Shopify evidence for complete saved-search, Files API, and selling-plan-group connection payloads across first/reverse, after, last/before, and post-delete reads. Only resource IDs and opaque cursor values are volatile.',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        {
          name: 'saved-search-create-setup',
          capturePath: '$.savedSearchCreate.response.data',
          proxyPath: '$.data',
          expectedDifferences: [
            {
              path: '$.first.savedSearch.id',
              matcher: 'shopify-gid:SavedSearch',
              reason: 'Different created resource.',
            },
            {
              path: '$.second.savedSearch.id',
              matcher: 'shopify-gid:SavedSearch',
              reason: 'Different created resource.',
            },
          ],
        },
        {
          name: 'file-create-setup',
          capturePath: '$.fileCreate.response.data.fileCreate',
          proxyPath: '$.data.fileCreate',
          proxyRequest: {
            documentPath: requestPaths.fileCreate,
            variablesCapturePath: '$.fileCreate.request.variables',
            apiVersion,
          },
          expectedDifferences: [
            { path: '$.files[*].id', matcher: 'shopify-gid:GenericFile', reason: 'Different created resources.' },
          ],
        },
        {
          name: 'selling-plan-group-create-setup',
          capturePath: '$.sellingPlanGroupCreate.response.data',
          proxyPath: '$.data',
          proxyRequest: {
            documentPath: requestPaths.sellingPlanGroupCreate,
            variablesCapturePath: '$.sellingPlanGroupCreate.request.variables',
            apiVersion,
          },
          expectedDifferences: [
            {
              path: '$.first.sellingPlanGroup.id',
              matcher: 'shopify-gid:SellingPlanGroup',
              reason: 'Different created resource.',
            },
            {
              path: '$.second.sellingPlanGroup.id',
              matcher: 'shopify-gid:SellingPlanGroup',
              reason: 'Different created resource.',
            },
          ],
        },
        {
          name: 'complete-connections-first-and-reverse',
          capturePath: '$.fullRead.response.data',
          proxyPath: '$.data',
          proxyRequest: {
            documentPath: requestPaths.fullRead,
            variablesCapturePath: '$.fullRead.request.variables',
            apiVersion,
          },
          expectedDifferences: connectionDifferences([
            'saved',
            'savedReverse',
            'files',
            'filesReverse',
            'groups',
            'groupsReverse',
          ]),
        },
        {
          name: 'complete-connections-after',
          capturePath: '$.afterRead.response.data',
          proxyPath: '$.data',
          proxyRequest: {
            documentPath: requestPaths.afterRead,
            variables: {
              savedAfter: {
                fromProxyResponse: 'complete-connections-first-and-reverse',
                path: '$.data.saved.edges[0].cursor',
              },
              fileAfter: {
                fromProxyResponse: 'complete-connections-first-and-reverse',
                path: '$.data.files.edges[0].cursor',
              },
              groupAfter: {
                fromProxyResponse: 'complete-connections-first-and-reverse',
                path: '$.data.groups.edges[0].cursor',
              },
              fileQuery: { fromCapturePath: '$.afterRead.request.variables.fileQuery' },
              groupQuery: { fromCapturePath: '$.afterRead.request.variables.groupQuery' },
            },
            apiVersion,
          },
          expectedDifferences: connectionDifferences(['saved', 'files', 'groups']),
        },
        {
          name: 'complete-connections-last-before',
          capturePath: '$.beforeRead.response.data',
          proxyPath: '$.data',
          proxyRequest: {
            documentPath: requestPaths.beforeRead,
            variables: {
              savedBefore: {
                fromProxyResponse: 'complete-connections-first-and-reverse',
                path: '$.data.saved.edges[1].cursor',
              },
              fileBefore: {
                fromProxyResponse: 'complete-connections-first-and-reverse',
                path: '$.data.files.edges[1].cursor',
              },
              groupBefore: {
                fromProxyResponse: 'complete-connections-first-and-reverse',
                path: '$.data.groups.edges[1].cursor',
              },
              fileQuery: { fromCapturePath: '$.beforeRead.request.variables.fileQuery' },
              groupQuery: { fromCapturePath: '$.beforeRead.request.variables.groupQuery' },
            },
            apiVersion,
          },
          expectedDifferences: connectionDifferences(['saved', 'files', 'groups']),
        },
        {
          name: 'delete-second-resources',
          capturePath: '$.deleteSecond.response.data',
          proxyPath: '$.data',
          proxyRequest: {
            documentPath: requestPaths.delete,
            variables: {
              savedSearch: { id: { fromPrimaryProxyPath: '$.data.second.savedSearch.id' } },
              fileIds: [{ fromProxyResponse: 'file-create-setup', path: '$.data.fileCreate.files[1].id' }],
              sellingPlanGroupId: {
                fromProxyResponse: 'selling-plan-group-create-setup',
                path: '$.data.second.sellingPlanGroup.id',
              },
            },
            apiVersion,
          },
          expectedDifferences: [
            {
              path: '$.savedSearchDelete.deletedSavedSearchId',
              matcher: 'shopify-gid:SavedSearch',
              reason: 'Different deleted resource.',
            },
            {
              path: '$.fileDelete.deletedFileIds[*]',
              matcher: 'shopify-gid:GenericFile',
              reason: 'Different deleted resource.',
            },
            {
              path: '$.sellingPlanGroupDelete.deletedSellingPlanGroupId',
              matcher: 'shopify-gid:SellingPlanGroup',
              reason: 'Different deleted resource.',
            },
          ],
        },
        {
          name: 'complete-connections-after-delete',
          capturePath: '$.afterDeleteRead.response.data',
          proxyPath: '$.data',
          proxyRequest: {
            documentPath: requestPaths.afterDeleteRead,
            variablesCapturePath: '$.afterDeleteRead.request.variables',
            apiVersion,
          },
          expectedDifferences: connectionDifferences(['saved', 'files', 'groups']),
        },
      ],
    },
  };
  await writeFile(specPath, `${JSON.stringify(spec, null, 2)}\n`, 'utf8');
  console.log(JSON.stringify({ ok: true, fixturePath, specPath }, null, 2));
} finally {
  if (firstSavedSearchId || firstFileId || firstGroupId) {
    const result = await client.runGraphqlRequest(deleteDocument, {
      savedSearch: { id: firstSavedSearchId ?? secondSavedSearchId ?? 'gid://shopify/SavedSearch/0' },
      fileIds: [firstFileId ?? secondFileId ?? 'gid://shopify/GenericFile/0'],
      sellingPlanGroupId: firstGroupId ?? secondGroupId ?? 'gid://shopify/SellingPlanGroup/0',
    });
    cleanup.push({ label: 'delete first records', response: result.payload });
  }
  if (secondSavedSearchId || secondFileId || secondGroupId) {
    const result = await client.runGraphqlRequest(deleteDocument, {
      savedSearch: { id: secondSavedSearchId ?? 'gid://shopify/SavedSearch/0' },
      fileIds: [secondFileId ?? 'gid://shopify/GenericFile/0'],
      sellingPlanGroupId: secondGroupId ?? 'gid://shopify/SellingPlanGroup/0',
    });
    cleanup.push({ label: 'delete second records', response: result.payload });
  }
}

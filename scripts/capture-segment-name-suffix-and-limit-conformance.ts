/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type CapturedCase = {
  name: string;
  request: {
    query: string;
    variables: Record<string, unknown>;
  };
  response: ConformanceGraphqlResult;
};

type SegmentNode = {
  id: string;
  name: string;
};

const SEGMENT_LIMIT = 6000;
const CHUNK_SIZE = 25;
const SETUP_CHUNKS = SEGMENT_LIMIT / CHUNK_SIZE;
const LIMIT_SETUP_PREFIX = 'Segment limit setup conformance';
const SUFFIX_PREFIX = 'Segment suffix conformance';
const marker = `${Date.now()}`;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'segments');
const outputPath = path.join(outputDir, 'segment-name-suffix-and-limit.json');
const setupDocumentPath = path.join(
  'config',
  'parity-requests',
  'segments',
  'segment-create-limit-setup-chunk.graphql',
);
const suffixDocumentPath = path.join('config', 'parity-requests', 'segments', 'segment-name-suffix-duplicate.graphql');
const specPath = path.join('config', 'parity-specs', 'segments', 'segment-name-suffix-and-limit.json');
let adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
let graphqlClient = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

function throttleStatus(result: ConformanceGraphqlResult): { currentlyAvailable: number; restoreRate: number } | null {
  const status = readPath(result.payload, ['extensions', 'cost', 'throttleStatus']);
  if (
    typeof status === 'object' &&
    status !== null &&
    typeof (status as Record<string, unknown>)['currentlyAvailable'] === 'number' &&
    typeof (status as Record<string, unknown>)['restoreRate'] === 'number'
  ) {
    const throttle = status as { currentlyAvailable: number; restoreRate: number };
    return {
      currentlyAvailable: throttle.currentlyAvailable,
      restoreRate: throttle.restoreRate,
    };
  }
  return null;
}

function isThrottled(result: ConformanceGraphqlResult): boolean {
  const errors = result.payload.errors;
  return (
    Array.isArray(errors) &&
    errors.some(
      (error) => typeof error === 'object' && error !== null && readPath(error, ['extensions', 'code']) === 'THROTTLED',
    )
  );
}

function isUnauthorized(result: ConformanceGraphqlResult): boolean {
  return result.status === 401;
}

async function refreshGraphqlClient(context: string): Promise<void> {
  console.log(`${context} received 401; refreshing conformance access token`);
  adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
  graphqlClient = createAdminGraphqlClient({
    adminOrigin,
    apiVersion,
    headers: buildAdminAuthHeaders(adminAccessToken),
  });
}

async function runGraphqlWithRetry(
  query: string,
  variables: Record<string, unknown>,
  context: string,
): Promise<ConformanceGraphqlResult> {
  for (let attempt = 0; attempt < 6; attempt += 1) {
    const result = await graphqlClient.runGraphqlRequest(query, variables);
    if (isUnauthorized(result)) {
      await refreshGraphqlClient(context);
      await sleep(1000);
      continue;
    }
    if (!isThrottled(result)) {
      const status = throttleStatus(result);
      if (status && status.currentlyAvailable < 500) {
        await sleep(Math.ceil(((500 - status.currentlyAvailable) / Math.max(status.restoreRate, 1)) * 1000));
      }
      return result;
    }
    const status = throttleStatus(result);
    const waitMs = status
      ? Math.ceil(((1000 - status.currentlyAvailable) / Math.max(status.restoreRate, 1)) * 1000) + 1000
      : 10_000;
    console.log(`${context} throttled; waiting ${waitMs}ms before retry ${attempt + 1}`);
    await sleep(waitMs);
  }
  return await graphqlClient.runGraphqlRequest(query, variables);
}

function assertGraphqlOk(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function readPath(value: unknown, pathSegments: string[]): unknown {
  let cursor = value;
  for (const segment of pathSegments) {
    if (!cursor || typeof cursor !== 'object') return undefined;
    cursor = (cursor as Record<string, unknown>)[segment];
  }
  return cursor;
}

function readRequiredString(result: ConformanceGraphqlResult, pathSegments: string[], context: string): string {
  const value = readPath(result.payload, pathSegments);
  if (typeof value !== 'string') {
    throw new Error(`${context} did not return ${pathSegments.join('.')}: ${JSON.stringify(result.payload, null, 2)}`);
  }
  return value;
}

function readUserErrors(result: ConformanceGraphqlResult, root: string): unknown[] {
  const value = readPath(result.payload, ['data', root, 'userErrors']);
  if (!Array.isArray(value)) {
    throw new Error(`${root}.userErrors missing from response: ${JSON.stringify(result.payload, null, 2)}`);
  }
  return value;
}

function assertNoUserErrors(result: ConformanceGraphqlResult, root: string, context: string): void {
  const userErrors = readUserErrors(result, root);
  if (userErrors.length > 0) {
    throw new Error(`${context} returned userErrors: ${JSON.stringify(result.payload, null, 2)}`);
  }
}

async function captureCase(
  cases: CapturedCase[],
  name: string,
  query: string,
  variables: Record<string, unknown>,
  assert: (result: ConformanceGraphqlResult) => void,
): Promise<ConformanceGraphqlResult> {
  const response = await runGraphqlWithRetry(query, variables, name);
  assertGraphqlOk(response, name);
  assert(response);
  cases.push({
    name,
    request: { query, variables },
    response,
  });
  return response;
}

function buildSetupDocument(): string {
  const variableDefinitions = [
    ...Array.from({ length: CHUNK_SIZE }, (_, index) => `$name${index}: String!`),
    '$query: String!',
  ].join(', ');
  const fields = Array.from(
    { length: CHUNK_SIZE },
    (_, index) => `    c${index.toString().padStart(3, '0')}: segmentCreate(name: $name${index}, query: $query) {
      segment {
        id
        name
      }
      userErrors {
        field
        message
      }
    }`,
  ).join('\n');

  return `mutation SegmentCreateLimitSetupChunk(${variableDefinitions}) {
${fields}
}
`;
}

const suffixDocument = `mutation SegmentNameSuffixDuplicate($name: String!, $query: String!) {
  original: segmentCreate(name: $name, query: $query) {
    segment {
      id
      name
    }
    userErrors {
      field
      message
    }
  }
  duplicate: segmentCreate(name: $name, query: $query) {
    segment {
      id
      name
    }
    userErrors {
      field
      message
    }
  }
}
`;

const limitDocument = `mutation SegmentCreateLimitValidation($name: String!, $query: String!) {
  segmentCreate(name: $name, query: $query) {
    segment {
      id
      name
      query
      creationDate
      lastEditDate
    }
    userErrors {
      field
      message
    }
  }
}
`;

const segmentCatalogQuery = `query SegmentCatalogForLimitSetup($first: Int!, $after: String) {
  segments(first: $first, after: $after) {
    nodes {
      id
      name
    }
    pageInfo {
      hasNextPage
      endCursor
    }
  }
}
`;

async function listSegments(): Promise<SegmentNode[]> {
  const segments: SegmentNode[] = [];
  let after: string | null = null;
  for (;;) {
    const response = await runGraphqlWithRetry(segmentCatalogQuery, { first: 250, after }, 'segment catalog');
    assertGraphqlOk(response, 'segment catalog before limit setup');
    const nodes = readPath(response.payload, ['data', 'segments', 'nodes']);
    if (!Array.isArray(nodes)) {
      throw new Error(`segments.nodes missing from catalog response: ${JSON.stringify(response.payload, null, 2)}`);
    }
    for (const node of nodes) {
      if (
        typeof node === 'object' &&
        node !== null &&
        typeof (node as Record<string, unknown>)['id'] === 'string' &&
        typeof (node as Record<string, unknown>)['name'] === 'string'
      ) {
        const segment = node as SegmentNode;
        segments.push({
          id: segment.id,
          name: segment.name,
        });
      }
    }
    const hasNextPage = readPath(response.payload, ['data', 'segments', 'pageInfo', 'hasNextPage']);
    const endCursor = readPath(response.payload, ['data', 'segments', 'pageInfo', 'endCursor']);
    if (hasNextPage !== true) break;
    after = typeof endCursor === 'string' ? endCursor : null;
    if (after === null) {
      throw new Error(`segments.pageInfo.endCursor missing while hasNextPage is true`);
    }
  }
  return segments;
}

function buildDeleteDocument(count: number): string {
  const variableDefinitions = Array.from({ length: count }, (_, index) => `$id${index}: ID!`).join(', ');
  const fields = Array.from(
    { length: count },
    (_, index) => `    d${index.toString().padStart(3, '0')}: segmentDelete(id: $id${index}) {
      deletedSegmentId
      userErrors {
        field
        message
      }
    }`,
  ).join('\n');

  return `mutation SegmentNameSuffixAndLimitCleanupChunk(${variableDefinitions}) {
${fields}
}
`;
}

function assertDeleteChunk(result: ConformanceGraphqlResult, count: number, context: string): void {
  for (let index = 0; index < count; index += 1) {
    const root = `d${index.toString().padStart(3, '0')}`;
    const userErrors = readUserErrors(result, root);
    if (userErrors.length > 0) {
      throw new Error(`${context} returned userErrors: ${JSON.stringify(result.payload, null, 2)}`);
    }
  }
}

async function deleteSegments(ids: Iterable<string>): Promise<number> {
  const idList = [...ids];
  let count = 0;
  for (let start = 0; start < idList.length; start += CHUNK_SIZE) {
    const chunk = idList.slice(start, start + CHUNK_SIZE);
    const variables = Object.fromEntries(chunk.map((id, index) => [`id${index}`, id]));
    const response = await runGraphqlWithRetry(buildDeleteDocument(chunk.length), variables, 'segment cleanup chunk');
    assertGraphqlOk(response, 'segment cleanup chunk');
    assertDeleteChunk(response, chunk.length, 'segment cleanup chunk');
    count += chunk.length;
    if (count % 500 === 0 || count === idList.length) console.log(`Deleted ${count} segment(s)`);
  }
  return count;
}

function setupVariablesForChunk(chunkIndex: number): Record<string, unknown> {
  return Object.fromEntries([
    ...Array.from({ length: CHUNK_SIZE }, (_, offset) => {
      const segmentIndex = chunkIndex * CHUNK_SIZE + offset;
      return [`name${offset}`, `${LIMIT_SETUP_PREFIX} ${marker} ${segmentIndex.toString().padStart(4, '0')}`];
    }),
    ['query', 'number_of_orders >= 1'],
  ]);
}

function assertSetupChunk(result: ConformanceGraphqlResult, context: string): void {
  for (let index = 0; index < CHUNK_SIZE; index += 1) {
    assertNoUserErrors(result, `c${index.toString().padStart(3, '0')}`, context);
    readRequiredString(result, ['data', `c${index.toString().padStart(3, '0')}`, 'segment', 'id'], context);
  }
}

function readSetupIds(result: ConformanceGraphqlResult): string[] {
  return Array.from({ length: CHUNK_SIZE }, (_, index) =>
    readRequiredString(result, ['data', `c${index.toString().padStart(3, '0')}`, 'segment', 'id'], 'setup id'),
  );
}

function assertDuplicateName(result: ConformanceGraphqlResult, expectedDuplicateName: string, context: string): void {
  assertNoUserErrors(result, 'original', `${context} original`);
  assertNoUserErrors(result, 'duplicate', `${context} duplicate`);
  const actual = readRequiredString(result, ['data', 'duplicate', 'segment', 'name'], context);
  if (actual !== expectedDuplicateName) {
    throw new Error(`${context} expected duplicate name ${JSON.stringify(expectedDuplicateName)}, got ${actual}`);
  }
}

function assertLimitReached(result: ConformanceGraphqlResult): void {
  const segment = readPath(result.payload, ['data', 'segmentCreate', 'segment']);
  if (segment !== null) {
    throw new Error(`segment limit branch returned a segment: ${JSON.stringify(result.payload, null, 2)}`);
  }
  const userErrors = readUserErrors(result, 'segmentCreate');
  const expected = [
    {
      field: null,
      message: 'Segment limit reached. Delete an existing segment to create more.',
    },
  ];
  if (JSON.stringify(userErrors) !== JSON.stringify(expected)) {
    throw new Error(`segment limit userErrors mismatch: ${JSON.stringify(result.payload, null, 2)}`);
  }
}

function setupSelectedPaths(): string[] {
  const lastAlias = `c${(CHUNK_SIZE - 1).toString().padStart(3, '0')}`;
  return ['$.c000.segment.name', '$.c000.userErrors', `$.${lastAlias}.segment.name`, `$.${lastAlias}.userErrors`];
}

function buildSpec() {
  const limitCaseIndex = SETUP_CHUNKS;
  const suffixOneCaseIndex = SETUP_CHUNKS + 1;
  const suffixZeroCaseIndex = SETUP_CHUNKS + 2;
  const setupTargets = Array.from({ length: SETUP_CHUNKS }, (_, index) => ({
    name: `segment-limit-setup-chunk-${index.toString().padStart(2, '0')}`,
    capturePath: `$.cases[${index}].response.payload.data`,
    proxyPath: '$.data',
    selectedPaths: setupSelectedPaths(),
    ...(index === 0
      ? {}
      : {
          proxyRequest: {
            documentPath: setupDocumentPath,
            variablesCapturePath: `$.cases[${index}].request.variables`,
          },
          preserveProxyState: true,
        }),
  }));

  return {
    scenarioId: 'segment-name-suffix-and-limit',
    operationNames: ['segmentCreate'],
    scenarioStatus: 'captured',
    assertionKinds: ['payload-shape', 'user-errors-parity', 'mutation-read-after-write'],
    liveCaptureFiles: [outputPath],
    runtimeTestFiles: ['tests/graphql_routes/products_saved_searches.rs'],
    proxyRequest: {
      documentPath: setupDocumentPath,
      variablesCapturePath: '$.cases[0].request.variables',
    },
    comparisonMode: 'captured-vs-proxy-request',
    notes: `Live Shopify evidence for segmentCreate duplicate-name suffix counters \`(0)\`/\`(1)\` plus the 6000 segment limit userError. The limit setup uses public segmentCreate requests in ${CHUNK_SIZE}-segment chunks; the proxy replay stages those same public requests before asserting the overflow response.`,
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        ...setupTargets,
        {
          name: 'segment-limit-reached-message',
          capturePath: `$.cases[${limitCaseIndex}].response.payload.data.segmentCreate`,
          proxyPath: '$.data.segmentCreate',
          proxyRequest: {
            documentPath: 'config/parity-requests/segments/segment-create-limit-validation.graphql',
            variablesCapturePath: `$.cases[${limitCaseIndex}].request.variables`,
          },
          preserveProxyState: true,
        },
        {
          name: 'segment-name-suffix-one',
          capturePath: `$.cases[${suffixOneCaseIndex}].response.payload.data.duplicate`,
          proxyPath: '$.data.duplicate',
          isolatedProxy: true,
          selectedPaths: ['$.segment.name', '$.userErrors'],
          proxyRequest: {
            documentPath: suffixDocumentPath,
            variablesCapturePath: `$.cases[${suffixOneCaseIndex}].request.variables`,
          },
        },
        {
          name: 'segment-name-suffix-zero',
          capturePath: `$.cases[${suffixZeroCaseIndex}].response.payload.data.duplicate`,
          proxyPath: '$.data.duplicate',
          isolatedProxy: true,
          selectedPaths: ['$.segment.name', '$.userErrors'],
          proxyRequest: {
            documentPath: suffixDocumentPath,
            variablesCapturePath: `$.cases[${suffixZeroCaseIndex}].request.variables`,
          },
        },
      ],
    },
  };
}

const setupDocument = buildSetupDocument();
const cases: CapturedCase[] = [];
const createdIds: string[] = [];

try {
  const existingSegments = await listSegments();
  if (existingSegments.length > 0) {
    console.log(`Deleting ${existingSegments.length} existing segment(s) before limit capture setup`);
    await deleteSegments(existingSegments.map((segment) => segment.id));
  }

  for (let chunkIndex = 0; chunkIndex < SETUP_CHUNKS; chunkIndex += 1) {
    const setup = await captureCase(
      cases,
      `segmentLimitSetupChunk${chunkIndex.toString().padStart(2, '0')}`,
      setupDocument,
      setupVariablesForChunk(chunkIndex),
      (result) => assertSetupChunk(result, `segment limit setup chunk ${chunkIndex}`),
    );
    createdIds.push(...readSetupIds(setup));
    console.log(`Captured segment limit setup chunk ${chunkIndex + 1}/${SETUP_CHUNKS}`);
  }

  await captureCase(
    cases,
    'segmentLimitReached',
    limitDocument,
    {
      name: `${LIMIT_SETUP_PREFIX} ${marker} overflow`,
      query: 'number_of_orders >= 1',
    },
    assertLimitReached,
  );

  console.log(`Cleaning up ${createdIds.length} limit setup segment(s) before suffix captures`);
  await deleteSegments(createdIds.reverse());
  createdIds.length = 0;

  const suffixOne = await captureCase(
    cases,
    'segmentNameSuffixOne',
    suffixDocument,
    {
      name: `${SUFFIX_PREFIX} ${marker} One Foo (1)`,
      query: 'number_of_orders >= 1',
    },
    (result) => assertDuplicateName(result, `${SUFFIX_PREFIX} ${marker} One Foo (2)`, 'suffix one'),
  );
  createdIds.push(
    readRequiredString(suffixOne, ['data', 'original', 'segment', 'id'], 'suffix one original id'),
    readRequiredString(suffixOne, ['data', 'duplicate', 'segment', 'id'], 'suffix one duplicate id'),
  );

  const suffixZero = await captureCase(
    cases,
    'segmentNameSuffixZero',
    suffixDocument,
    {
      name: `${SUFFIX_PREFIX} ${marker} Zero Foo (0)`,
      query: 'number_of_orders >= 1',
    },
    (result) => assertDuplicateName(result, `${SUFFIX_PREFIX} ${marker} Zero Foo (1)`, 'suffix zero'),
  );
  createdIds.push(
    readRequiredString(suffixZero, ['data', 'original', 'segment', 'id'], 'suffix zero original id'),
    readRequiredString(suffixZero, ['data', 'duplicate', 'segment', 'id'], 'suffix zero duplicate id'),
  );
} finally {
  if (createdIds.length > 0) {
    console.log(`Cleaning up ${createdIds.length} created segment(s)`);
    await deleteSegments(createdIds.reverse());
  }
}

await mkdir(outputDir, { recursive: true });
await mkdir(path.dirname(setupDocumentPath), { recursive: true });
await mkdir(path.dirname(specPath), { recursive: true });
await writeFile(setupDocumentPath, setupDocument, 'utf8');
await writeFile(suffixDocumentPath, suffixDocument, 'utf8');
await writeFile(specPath, `${JSON.stringify(buildSpec(), null, 2)}\n`, 'utf8');
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      cases,
      notes: [
        'Live Shopify evidence that segmentCreate increments duplicate names ending in `(0)` and `(1)` by treating the trailing non-negative integer as a counter.',
        'Live Shopify evidence that segmentCreate at the 6000 segment cap returns field:null and message `Segment limit reached. Delete an existing segment to create more.`.',
        'The script clears existing segments in the disposable conformance shop before capture, creates 6000 setup segments through public segmentCreate mutations, captures the overflow branch, deletes those setup segments, captures suffix probes, then deletes every suffix probe segment it created.',
      ],
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
  'utf8',
);
console.log(`Wrote ${outputPath}`);
console.log(`Wrote ${specPath}`);
console.log(`Wrote ${setupDocumentPath}`);
console.log(`Wrote ${suffixDocumentPath}`);

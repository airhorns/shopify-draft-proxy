/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type Capture = {
  name: string;
  request: {
    query: string;
    variables: Record<string, unknown>;
  };
  status: number;
  response: unknown;
};

type DefinitionNode = {
  id: string;
  namespace?: string | null;
  key?: string | null;
  ownerType?: string | null;
  standardTemplate?: { id?: string | null } | null;
};

type UpstreamCall = {
  method: 'POST';
  apiSurface: 'admin';
  path: string;
  operationName: string;
  variables: Record<string, unknown>;
  query: string;
  response: { status: number; body: unknown };
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metafields');
const outputPath = path.join(outputDir, 'metafield-definition-resource-type-limit.json');
const runId = Date.now().toString(36);
const primaryNamespace = `resource_limit_${runId}`;
const secondaryNamespace = `resource_limit_secondary_${runId}`;
const maxProbeCreations = 260;
const createDefinitionDocumentPath =
  'config/parity-requests/metafields/metafield-definition-resource-type-limit.graphql';
const hydrateByIdentifierDocumentPath =
  'config/parity-requests/metafields/metafield-definition-hydrate-by-identifier.graphql';
const hydrateResourceScopeDocumentPath =
  'config/parity-requests/metafields/metafield-definitions-hydrate-resource-scope.graphql';
const createDefinitionMutation = await readFile(createDefinitionDocumentPath, 'utf8');
const hydrateByIdentifierDocument = await readFile(hydrateByIdentifierDocumentPath, 'utf8');
const hydrateResourceScopeDocument = await readFile(hydrateResourceScopeDocumentPath, 'utf8');

const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const readDefinitionsQuery = `#graphql
  query ProductMetafieldDefinitionsForResourceLimit($first: Int!, $after: String) {
    metafieldDefinitions(ownerType: PRODUCT, first: $first, after: $after) {
      nodes {
        id
        namespace
        key
        ownerType
      }
      pageInfo {
        hasNextPage
        endCursor
      }
    }
  }
`;

const readNamespaceDefinitionsQuery = `#graphql
  query ProductMetafieldDefinitionsForResourceLimitNamespace($namespace: String!, $first: Int!, $after: String) {
    metafieldDefinitions(ownerType: PRODUCT, namespace: $namespace, first: $first, after: $after) {
      nodes {
        id
        namespace
        key
        ownerType
      }
      pageInfo {
        hasNextPage
        endCursor
      }
    }
  }
`;

const deleteDefinitionMutation = `#graphql
  mutation DeleteProductMetafieldDefinitionForResourceLimit($id: ID!) {
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

function readObject(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as Record<string, unknown>) : null;
}

function readPath(value: unknown, pathParts: string[]): unknown {
  let current = value;
  for (const part of pathParts) {
    const object = readObject(current);
    if (object === null) {
      return undefined;
    }
    current = object[part];
  }

  return current;
}

function readString(value: unknown): string | null {
  return typeof value === 'string' ? value : null;
}

function readNumber(value: unknown): number | null {
  return typeof value === 'number' ? value : null;
}

function captureFromResult(
  name: string,
  query: string,
  variables: Record<string, unknown>,
  result: ConformanceGraphqlResult,
): Capture {
  return {
    name,
    request: {
      query,
      variables,
    },
    status: result.status,
    response: result.payload,
  };
}

function assertGraphqlOk(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || readPath(result.payload, ['errors'])) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function userErrorCodes(capture: Capture): string[] {
  const errors = readPath(capture.response, ['data', 'metafieldDefinitionCreate', 'userErrors']);
  if (!Array.isArray(errors)) {
    return [];
  }

  return errors.map((error) => readString(readPath(error, ['code']))).filter((code): code is string => code !== null);
}

function createdDefinitionId(capture: Capture): string | null {
  return readString(readPath(capture.response, ['data', 'metafieldDefinitionCreate', 'createdDefinition', 'id']));
}

function definitionNodes(capture: Capture, root = 'metafieldDefinitions'): DefinitionNode[] {
  const nodes = readPath(capture.response, ['data', root, 'nodes']);
  return Array.isArray(nodes) ? (nodes as DefinitionNode[]) : [];
}

function merchantResourceLimitDefinition(definition: DefinitionNode): boolean {
  const namespace = definition.namespace ?? '';
  return namespace !== 'shopify' && !namespace.startsWith('app--');
}

function hasNextPage(capture: Capture, root = 'metafieldDefinitions'): boolean {
  return readPath(capture.response, ['data', root, 'pageInfo', 'hasNextPage']) === true;
}

function endCursor(capture: Capture, root = 'metafieldDefinitions'): string | null {
  return readString(readPath(capture.response, ['data', root, 'pageInfo', 'endCursor']));
}

async function waitForThrottle(result: ConformanceGraphqlResult): Promise<void> {
  const currentlyAvailable = readNumber(
    readPath(result.payload, ['extensions', 'cost', 'throttleStatus', 'currentlyAvailable']),
  );
  const restoreRate = readNumber(readPath(result.payload, ['extensions', 'cost', 'throttleStatus', 'restoreRate']));
  if (currentlyAvailable === null || restoreRate === null || restoreRate <= 0 || currentlyAvailable >= 100) {
    return;
  }

  await sleep(Math.ceil(((100 - currentlyAvailable) / restoreRate) * 1000));
}

async function captureGraphql(name: string, query: string, variables: Record<string, unknown>): Promise<Capture> {
  const result = await runGraphqlRaw(query, variables);
  await waitForThrottle(result);
  assertGraphqlOk(result, name);
  return captureFromResult(name, query, variables, result);
}

async function recordUpstreamCall(
  operationName: string,
  query: string,
  variables: Record<string, unknown>,
): Promise<UpstreamCall> {
  const capture = await captureGraphql(operationName, query, variables);
  return {
    method: 'POST',
    apiSurface: 'admin',
    path: `/admin/api/${apiVersion}/graphql.json`,
    operationName,
    variables,
    query,
    response: { status: capture.status, body: capture.response },
  };
}

async function recordResourceLimitPrerequisites(limitVariables: Record<string, unknown>): Promise<UpstreamCall[]> {
  const definition = readObject(limitVariables['definition']);
  const namespace = readString(definition?.['namespace']);
  const key = readString(definition?.['key']);
  if (namespace === null || key === null) {
    throw new Error(`Limit variables do not contain a namespace/key: ${JSON.stringify(limitVariables)}`);
  }
  const identityCall = await recordUpstreamCall('MetafieldDefinitionHydrateByIdentifier', hydrateByIdentifierDocument, {
    identifier: { ownerType: 'PRODUCT', namespace, key },
  });
  let lastObservedBucketDefinitions = 0;
  for (let attempt = 0; attempt < 60; attempt += 1) {
    const calls: UpstreamCall[] = [];
    let after: string | null = null;
    let observedBucketDefinitions = 0;
    for (let page = 0; page < 3; page += 1) {
      const variables = { ownerType: 'PRODUCT', query: '-namespace:app--*', first: 250, after };
      const call = await recordUpstreamCall(
        'MetafieldDefinitionsHydrateResourceScope',
        hydrateResourceScopeDocument,
        variables,
      );
      calls.push(call);
      const nodes = readPath(call.response.body, ['data', 'metafieldDefinitions', 'nodes']);
      if (!Array.isArray(nodes)) {
        throw new Error(`Resource-scope hydrate page ${page + 1} did not return nodes`);
      }
      observedBucketDefinitions += (nodes as DefinitionNode[]).filter(merchantResourceLimitDefinition).length;
      const pageInfo = readObject(readPath(call.response.body, ['data', 'metafieldDefinitions', 'pageInfo']));
      if (observedBucketDefinitions >= 256 || pageInfo?.['hasNextPage'] !== true) break;
      const endCursor = readString(pageInfo['endCursor']);
      if (endCursor === null) {
        throw new Error(`Resource-scope hydrate page ${page + 1} did not return endCursor`);
      }
      after = endCursor;
    }
    if (observedBucketDefinitions >= 256) return [identityCall, ...calls];
    lastObservedBucketDefinitions = observedBucketDefinitions;
    await sleep(1_000);
  }
  throw new Error(
    `Resource-scope hydrate observed only ${lastObservedBucketDefinitions} merchant-bucket definitions at the cap after waiting for Shopify's search index`,
  );
}

async function readAllDefinitions(query: string, variables: Record<string, unknown>, name: string): Promise<Capture[]> {
  const captures: Capture[] = [];
  let after: string | null = null;

  for (;;) {
    const capture = await captureGraphql(`${name}-${captures.length + 1}`, query, {
      ...variables,
      first: 250,
      after,
    });
    captures.push(capture);
    if (!hasNextPage(capture)) {
      return captures;
    }
    after = endCursor(capture);
  }
}

function createDefinitionVariables(namespace: string, key: string): Record<string, unknown> {
  return {
    definition: {
      namespace,
      key,
      ownerType: 'PRODUCT',
      name: key,
      type: 'single_line_text_field',
    },
  };
}

async function deleteDefinition(id: string): Promise<Capture> {
  const result = await runGraphqlRaw(deleteDefinitionMutation, { id });
  await waitForThrottle(result);
  return captureFromResult(`cleanup-${id.split('/').at(-1) ?? id}`, deleteDefinitionMutation, { id }, result);
}

async function deleteCreatedDefinitions(ids: string[]): Promise<Capture[]> {
  const cleanup: Capture[] = [];
  for (const id of ids) {
    cleanup.push(await deleteDefinition(id));
  }
  return cleanup;
}

const preflightCatalog = await readAllDefinitions(readDefinitionsQuery, {}, 'preflight-product-definitions');
const createdDefinitionIds: string[] = [];
const createAttempts: Capture[] = [];
let limitAttempt: Capture | null = null;
let secondaryNamespaceAttempt: Capture | null = null;
let cleanup: Capture[] = [];
let postCleanupPrimaryNamespace: Capture[] = [];
let postCleanupSecondaryNamespace: Capture[] = [];
let upstreamCalls: UpstreamCall[] = [];

try {
  for (let index = 0; index < maxProbeCreations; index += 1) {
    const key = `key_${index.toString().padStart(3, '0')}`;
    const capture = await captureGraphql(
      `primary-namespace-create-${index + 1}`,
      createDefinitionMutation,
      createDefinitionVariables(primaryNamespace, key),
    );
    createAttempts.push(capture);

    const id = createdDefinitionId(capture);
    if (id !== null) {
      createdDefinitionIds.push(id);
      continue;
    }

    if (userErrorCodes(capture).includes('RESOURCE_TYPE_LIMIT_EXCEEDED')) {
      limitAttempt = capture;
      break;
    }

    throw new Error(`Unexpected metafield definition create response: ${JSON.stringify(capture.response, null, 2)}`);
  }

  if (limitAttempt === null) {
    throw new Error(`Did not observe RESOURCE_TYPE_LIMIT_EXCEEDED after ${maxProbeCreations} create attempts.`);
  }

  upstreamCalls = await recordResourceLimitPrerequisites(limitAttempt.request.variables);

  secondaryNamespaceAttempt = await captureGraphql(
    'secondary-namespace-create-after-primary-limit',
    createDefinitionMutation,
    createDefinitionVariables(secondaryNamespace, 'key_000'),
  );
  const secondaryId = createdDefinitionId(secondaryNamespaceAttempt);
  if (secondaryId !== null) {
    createdDefinitionIds.push(secondaryId);
  }
} finally {
  cleanup = await deleteCreatedDefinitions([...createdDefinitionIds].reverse());
  postCleanupPrimaryNamespace = await readAllDefinitions(
    readNamespaceDefinitionsQuery,
    { namespace: primaryNamespace },
    'post-cleanup-primary-namespace',
  );
  postCleanupSecondaryNamespace = await readAllDefinitions(
    readNamespaceDefinitionsQuery,
    { namespace: secondaryNamespace },
    'post-cleanup-secondary-namespace',
  );
}

const preflightCount = preflightCatalog.flatMap((capture) => definitionNodes(capture)).length;
const preflightMerchantResourceLimitCount = preflightCatalog
  .flatMap((capture) => definitionNodes(capture))
  .filter(merchantResourceLimitDefinition).length;
const primarySuccessCount = createAttempts.filter((capture) => createdDefinitionId(capture) !== null).length;
const observedOwnerTypeBoundary = preflightMerchantResourceLimitCount + primarySuccessCount;
const secondarySucceeded =
  secondaryNamespaceAttempt !== null && createdDefinitionId(secondaryNamespaceAttempt) !== null;

if (observedOwnerTypeBoundary !== 256 || secondarySucceeded) {
  throw new Error(
    `Expected PRODUCT ownerType resource limit at 256 with secondary namespace rejected; observed ${JSON.stringify({
      preflightCount,
      preflightMerchantResourceLimitCount,
      primarySuccessCount,
      observedOwnerTypeBoundary,
      secondarySucceeded,
      limitCodes: limitAttempt ? userErrorCodes(limitAttempt) : [],
      secondaryCodes: secondaryNamespaceAttempt ? userErrorCodes(secondaryNamespaceAttempt) : [],
    })}`,
  );
}

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      summary:
        'MetafieldDefinitionCreate PRODUCT ownerType resource limit capture. The script records the preflight catalog, creates disposable PRODUCT definitions until Shopify returns RESOURCE_TYPE_LIMIT_EXCEEDED, probes a second namespace after the limit, then deletes every created definition.',
      setup: {
        runId,
        primaryNamespace,
        secondaryNamespace,
        maxProbeCreations,
      },
      observed: {
        preflightProductDefinitionCount: preflightCount,
        preflightMerchantResourceLimitDefinitionCount: preflightMerchantResourceLimitCount,
        primaryNamespaceAcceptedCreates: primarySuccessCount,
        observedOwnerTypeBoundary,
        secondaryNamespaceAcceptedAfterPrimaryLimit: secondarySucceeded,
      },
      preflightCatalog,
      createAttempts,
      limitAttempt,
      secondaryNamespaceAttempt,
      cleanup,
      postCleanupPrimaryNamespace,
      postCleanupSecondaryNamespace,
      upstreamCalls,
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(
  JSON.stringify(
    {
      ok: true,
      outputPath,
      storeDomain,
      apiVersion,
      preflightCount,
      preflightMerchantResourceLimitCount,
      primarySuccessCount,
      observedOwnerTypeBoundary,
      cleanupCount: cleanup.length,
    },
    null,
    2,
  ),
);

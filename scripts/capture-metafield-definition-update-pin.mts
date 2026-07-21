/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type DefinitionNode = {
  id: string;
  key?: string;
  namespace?: string;
  pinnedPosition?: number | null;
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

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metafields');
const outputPath = path.join(outputDir, 'metafield-definition-update-pin.json');
const primaryDocumentPath = 'config/parity-requests/metafield-definitions/update-pin.graphql';
const readAfterDocumentPath = 'config/parity-requests/metafield-definitions/update-pin-read.graphql';

const { runGraphql, runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const readPinnedDefinitionsQuery = `#graphql
  query ExistingPinnedMetafieldDefinitions {
    metafieldDefinitions(ownerType: PRODUCT, first: 50, pinnedStatus: PINNED, sortKey: PINNED_POSITION) {
      nodes { id key namespace pinnedPosition }
    }
  }
`;

const readNamespaceDefinitionsQuery = `#graphql
  query TemporaryUpdatePinDefinitions($namespace: String!) {
    metafieldDefinitions(ownerType: PRODUCT, first: 100, namespace: $namespace) {
      nodes { id key }
    }
  }
`;

const pinByIdMutation = `#graphql
  mutation RestorePinnedMetafieldDefinition($definitionId: ID!) {
    metafieldDefinitionPin(definitionId: $definitionId) {
      pinnedDefinition { id pinnedPosition }
      userErrors { field message code }
    }
  }
`;

const unpinByIdMutation = `#graphql
  mutation TemporarilyUnpinMetafieldDefinition($definitionId: ID!) {
    metafieldDefinitionUnpin(definitionId: $definitionId) {
      unpinnedDefinition { id }
      userErrors { field message code }
    }
  }
`;

const deleteDefinitionMutation = `#graphql
  mutation DeleteTemporaryMetafieldDefinition($id: ID!) {
    metafieldDefinitionDelete(id: $id, deleteAllAssociatedMetafields: true) {
      deletedDefinitionId
      userErrors { field message code }
    }
  }
`;

const runId = Date.now().toString(36);
const namespace = `update_pin_${runId}`;
const fillerNamespace = `update_pin_baseline_${runId}`;
const variables = {
  namespace,
  categoryId: 'gid://shopify/TaxonomyCategory/ap-2',
};

const primaryDocument = await readFile(primaryDocumentPath, 'utf8');
const readAfterDocument = await readFile(readAfterDocumentPath, 'utf8');
const hydrateByIdentifierDocument = await readFile(
  'config/parity-requests/metafields/metafield-definition-hydrate-by-identifier.graphql',
  'utf8',
);
const hydrateResourceScopeDocument = await readFile(
  'config/parity-requests/metafields/metafield-definitions-hydrate-resource-scope.graphql',
  'utf8',
);
const hydratePinnedOwnerDocument = await readFile(
  'config/parity-requests/metafields/metafield-definitions-hydrate-pinned-owner.graphql',
  'utf8',
);
const hydrateWindowDocument = await readFile(
  'config/parity-requests/metafields/metafield-definitions-hydrate-window.graphql',
  'utf8',
);

function readObject(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as Record<string, unknown>) : null;
}

function readPath(value: unknown, parts: string[]): unknown {
  let current = value;
  for (const part of parts) current = readObject(current)?.[part];
  return current;
}

function createAndPinBatchDocument(start: number, end: number): string {
  const fields: string[] = [];
  for (let index = start; index <= end; index += 1) {
    const suffix = String(index).padStart(2, '0');
    fields.push(`
      create${suffix}: metafieldDefinitionCreate(
        definition: {
          ownerType: PRODUCT
          namespace: $namespace
          key: "baseline_${suffix}"
          name: "Update pin baseline ${suffix}"
          type: "single_line_text_field"
        }
      ) { createdDefinition { id } userErrors { field message code } }
      pin${suffix}: metafieldDefinitionPin(
        identifier: { ownerType: PRODUCT, namespace: $namespace, key: "baseline_${suffix}" }
      ) { pinnedDefinition { id pinnedPosition } userErrors { field message code } }
    `);
  }
  return `#graphql
    mutation MetafieldDefinitionUpdatePinSetupBatch($namespace: String!) {
      ${fields.join('\n')}
    }
  `;
}

async function createFillerPins(): Promise<void> {
  for (const [start, end] of [
    [1, 10],
    [11, 20],
    [21, 30],
    [31, 40],
    [41, 49],
  ] as const) {
    const response = await runGraphql(createAndPinBatchDocument(start, end), { namespace: fillerNamespace });
    for (let index = start; index <= end; index += 1) {
      const suffix = String(index).padStart(2, '0');
      const createErrors = readPath(response, ['data', `create${suffix}`, 'userErrors']);
      const pinErrors = readPath(response, ['data', `pin${suffix}`, 'userErrors']);
      if (
        !Array.isArray(createErrors) ||
        createErrors.length > 0 ||
        !Array.isArray(pinErrors) ||
        pinErrors.length > 0
      ) {
        throw new Error(`Failed to create update-pin baseline ${suffix}: ${JSON.stringify(response, null, 2)}`);
      }
    }
  }
}

async function readPinnedDefinitions(): Promise<DefinitionNode[]> {
  const response = await runGraphql(readPinnedDefinitionsQuery);
  return (readPath(response, ['data', 'metafieldDefinitions', 'nodes']) as DefinitionNode[] | undefined) ?? [];
}

async function waitForPinnedDefinitionCount(expected: number): Promise<DefinitionNode[]> {
  let definitions: DefinitionNode[] = [];
  for (let attempt = 0; attempt < 60; attempt += 1) {
    definitions = await readPinnedDefinitions();
    if (definitions.length === expected) return definitions;
    await sleep(1_000);
  }
  throw new Error(`Expected ${expected} setup pins, received ${definitions.length}`);
}

async function deleteNamespaceDefinitions(targetNamespace: string): Promise<DefinitionNode[]> {
  const read = await runGraphql(readNamespaceDefinitionsQuery, { namespace: targetNamespace });
  const definitions = (readPath(read, ['data', 'metafieldDefinitions', 'nodes']) as DefinitionNode[] | undefined) ?? [];
  for (const definition of definitions) {
    try {
      await runGraphql(deleteDefinitionMutation, { id: definition.id });
    } catch (error) {
      console.warn(`Failed to delete temporary metafield definition ${definition.id}: ${String(error)}`);
    }
  }
  return definitions;
}

async function restoreBaselinePins(baselinePinned: DefinitionNode[]): Promise<void> {
  const ascending = [...baselinePinned].sort((left, right) => (left.pinnedPosition ?? 0) - (right.pinnedPosition ?? 0));
  for (const definition of ascending) {
    try {
      await runGraphql(pinByIdMutation, { definitionId: definition.id });
    } catch (error) {
      console.warn(`Failed to restore pinned metafield definition ${definition.id}: ${String(error)}`);
    }
  }
}

async function recordUpstreamCall(
  operationName: string,
  query: string,
  callVariables: Record<string, unknown>,
): Promise<UpstreamCall> {
  const result = await runGraphqlRaw(query, callVariables);
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${operationName} failed: ${JSON.stringify(result, null, 2)}`);
  }
  return {
    method: 'POST',
    apiSurface: 'admin',
    path: `/admin/api/${apiVersion}/graphql.json`,
    operationName,
    variables: callVariables,
    query,
    response: { status: result.status, body: result.payload },
  };
}

async function recordIdentity(key: string): Promise<UpstreamCall> {
  return await recordUpstreamCall('MetafieldDefinitionHydrateByIdentifier', hydrateByIdentifierDocument, {
    identifier: { ownerType: 'PRODUCT', namespace, key },
  });
}

async function recordResourceScopeHydrate(): Promise<UpstreamCall[]> {
  const calls: UpstreamCall[] = [];
  let after: string | null = null;
  let observedBucketDefinitions = 0;
  for (let page = 0; page < 3; page += 1) {
    const callVariables = { ownerType: 'PRODUCT', query: '-namespace:app--*', first: 250, after };
    const call = await recordUpstreamCall(
      'MetafieldDefinitionsHydrateResourceScope',
      hydrateResourceScopeDocument,
      callVariables,
    );
    calls.push(call);
    const nodes = readPath(call.response.body, ['data', 'metafieldDefinitions', 'nodes']);
    if (!Array.isArray(nodes)) throw new Error(`Resource-scope page ${page + 1} did not return nodes`);
    observedBucketDefinitions += nodes.filter((node) => readObject(node)?.['namespace'] !== 'shopify').length;
    const pageInfo = readObject(readPath(call.response.body, ['data', 'metafieldDefinitions', 'pageInfo']));
    if (observedBucketDefinitions >= 256 || pageInfo?.['hasNextPage'] !== true) break;
    const endCursor = pageInfo?.['endCursor'];
    if (typeof endCursor !== 'string') throw new Error(`Resource-scope page ${page + 1} omitted endCursor`);
    after = endCursor;
  }
  return calls;
}

async function recordReadWindow(): Promise<UpstreamCall> {
  return await recordUpstreamCall('MetafieldDefinitionsHydrateWindow', hydrateWindowDocument, {
    ownerType: 'PRODUCT',
    key: null,
    namespace,
    pinnedStatus: 'PINNED',
    constraintSubtype: null,
    constraintStatus: null,
    first: 25,
    after: null,
    last: null,
    before: null,
    reverse: false,
    sortKey: 'PINNED_POSITION',
    query: null,
  });
}

const baselinePinned = await readPinnedDefinitions();
let setupPinnedDefinitions: DefinitionNode[] = [];
let primaryResponse: unknown = null;
let readAfterResponse: unknown = null;
let deletedTargetDefinitions: DefinitionNode[] = [];
let deletedFillerDefinitions: DefinitionNode[] = [];
let upstreamCalls: UpstreamCall[] = [];

try {
  await mkdir(outputDir, { recursive: true });
  for (const definition of baselinePinned) {
    await runGraphql(unpinByIdMutation, { definitionId: definition.id });
  }
  await createFillerPins();
  setupPinnedDefinitions = await waitForPinnedDefinitionCount(49);

  upstreamCalls.push(await recordIdentity('pin_true'));
  upstreamCalls.push(...(await recordResourceScopeHydrate()));
  upstreamCalls.push(await recordIdentity('pin_false'));
  upstreamCalls.push(
    await recordUpstreamCall('MetafieldDefinitionsHydratePinnedOwner', hydratePinnedOwnerDocument, {
      ownerType: 'PRODUCT',
    }),
  );
  upstreamCalls.push(await recordIdentity('constrained'));
  upstreamCalls.push(await recordIdentity('over_cap'));

  primaryResponse = await runGraphql(primaryDocument, variables);
  upstreamCalls.push(await recordReadWindow());
  readAfterResponse = await runGraphql(readAfterDocument, variables);
} finally {
  deletedTargetDefinitions = await deleteNamespaceDefinitions(namespace);
  deletedFillerDefinitions = await deleteNamespaceDefinitions(fillerNamespace);
  await restoreBaselinePins(baselinePinned);
}

await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      setup: { runId, namespace, fillerNamespace, targetPinnedBaseline: 49 },
      variables,
      baselinePinnedDefinitions: baselinePinned,
      setupPinnedDefinitions,
      primary: {
        request: { documentPath: primaryDocumentPath, variables },
        response: primaryResponse,
      },
      readAfter: {
        request: { documentPath: readAfterDocumentPath, variables },
        response: readAfterResponse,
      },
      cleanup: {
        deletedTargetDefinitions,
        deletedFillerDefinitions,
        restoredPinnedDefinitions: baselinePinned,
      },
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
      namespace,
      baselinePinnedCount: baselinePinned.length,
      setupPinnedCount: setupPinnedDefinitions.length,
      deletedTargetDefinitionCount: deletedTargetDefinitions.length,
      deletedFillerDefinitionCount: deletedFillerDefinitions.length,
    },
    null,
    2,
  ),
);

/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metafields');
const outputPath = path.join(outputDir, 'standard-metafield-definition-enable-reenable-idempotent.json');
const primaryDocumentPath =
  'config/parity-requests/metafields/standard-metafield-definition-enable-reenable-idempotent.graphql';
const readAfterDocumentPath =
  'config/parity-requests/metafields/standard-metafield-definition-enable-reenable-read.graphql';

const { runGraphql, runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const readPinnedDefinitionsQuery = `#graphql
  query ExistingPinnedMetafieldDefinitions {
    metafieldDefinitions(ownerType: PRODUCT, first: 50, pinnedStatus: PINNED, sortKey: PINNED_POSITION) {
      nodes {
        id
        namespace
        key
        pinnedPosition
      }
    }
  }
`;

const readNamespaceDefinitionsQuery = `#graphql
  query TemporaryNamespaceDefinitions($namespace: String!) {
    metafieldDefinitions(ownerType: PRODUCT, first: 100, namespace: $namespace) {
      nodes {
        id
        namespace
        key
        pinnedPosition
      }
    }
  }
`;

const readStandardDefinitionQuery = `#graphql
  query ExistingStandardSubtitleDefinition {
    metafieldDefinition(identifier: { ownerType: PRODUCT, namespace: "descriptors", key: "subtitle" }) {
      id
      namespace
      key
      pinnedPosition
    }
  }
`;

const pinByIdMutation = `#graphql
  mutation RestorePinnedMetafieldDefinition($definitionId: ID!) {
    metafieldDefinitionPin(definitionId: $definitionId) {
      pinnedDefinition {
        id
        pinnedPosition
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const unpinByIdMutation = `#graphql
  mutation TemporarilyUnpinMetafieldDefinition($definitionId: ID!) {
    metafieldDefinitionUnpin(definitionId: $definitionId) {
      unpinnedDefinition {
        id
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const deleteDefinitionMutation = `#graphql
  mutation DeleteTemporaryMetafieldDefinition($id: ID!) {
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

type DefinitionNode = {
  id: string;
  namespace?: string;
  key?: string;
  pinnedPosition?: number | null;
};

type GraphqlResponse = {
  data?: Record<string, unknown>;
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

const namespace = `standard_reenable_${Date.now().toString(36)}`;
const variables = {
  namespace,
  standardTemplateId: 'gid://shopify/StandardMetafieldDefinitionTemplate/1',
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
const baselinePinned =
  ((await runGraphql(readPinnedDefinitionsQuery)).data?.metafieldDefinitions?.nodes as DefinitionNode[] | undefined) ??
  [];
const baselineStandardDefinition =
  ((await runGraphql(readStandardDefinitionQuery)).data?.metafieldDefinition as DefinitionNode | null | undefined) ??
  null;

function readStandardInitialId(response: GraphqlResponse): string {
  const data = response.data;
  const standardInitial = data?.['standardInitial'];
  if (typeof standardInitial !== 'object' || standardInitial === null || Array.isArray(standardInitial)) {
    throw new Error(`standardInitial payload missing from response: ${JSON.stringify(response, null, 2)}`);
  }
  const standardInitialRecord = standardInitial as Record<string, unknown>;
  const createdDefinition = standardInitialRecord['createdDefinition'];
  if (typeof createdDefinition !== 'object' || createdDefinition === null || Array.isArray(createdDefinition)) {
    throw new Error(`standardInitial.createdDefinition missing from response: ${JSON.stringify(response, null, 2)}`);
  }
  const createdDefinitionRecord = createdDefinition as Record<string, unknown>;
  const id = createdDefinitionRecord['id'];
  if (typeof id !== 'string') {
    throw new Error(`standardInitial.createdDefinition.id missing from response: ${JSON.stringify(response, null, 2)}`);
  }
  return id;
}

function readObject(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as Record<string, unknown>) : null;
}

function readPath(value: unknown, parts: string[]): unknown {
  let current = value;
  for (const part of parts) current = readObject(current)?.[part];
  return current;
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

async function recordIdentity(targetNamespace: string, key: string): Promise<UpstreamCall> {
  return await recordUpstreamCall('MetafieldDefinitionHydrateByIdentifier', hydrateByIdentifierDocument, {
    identifier: { ownerType: 'PRODUCT', namespace: targetNamespace, key },
  });
}

async function recordResourceScope(): Promise<UpstreamCall[]> {
  const calls: UpstreamCall[] = [];
  let after: string | null = null;
  let observed = 0;
  for (let page = 0; page < 3; page += 1) {
    const callVariables = { ownerType: 'PRODUCT', query: '-namespace:app--*', first: 250, after };
    const call = await recordUpstreamCall(
      'MetafieldDefinitionsHydrateResourceScope',
      hydrateResourceScopeDocument,
      callVariables,
    );
    calls.push(call);
    const nodes = readPath(call.response.body, ['data', 'metafieldDefinitions', 'nodes']);
    if (!Array.isArray(nodes)) throw new Error(`resource-scope page ${page + 1} omitted nodes`);
    observed += nodes.filter((node) => readObject(node)?.['namespace'] !== 'shopify').length;
    const pageInfo = readObject(readPath(call.response.body, ['data', 'metafieldDefinitions', 'pageInfo']));
    if (observed >= 256 || pageInfo?.['hasNextPage'] !== true) break;
    const endCursor = pageInfo?.['endCursor'];
    if (typeof endCursor !== 'string') throw new Error(`resource-scope page ${page + 1} omitted endCursor`);
    after = endCursor;
  }
  return calls;
}

async function deleteNamespaceDefinitions(): Promise<DefinitionNode[]> {
  const read = await runGraphql(readNamespaceDefinitionsQuery, { namespace });
  const definitions = (read.data?.metafieldDefinitions?.nodes as DefinitionNode[] | undefined) ?? [];
  for (const definition of definitions) {
    try {
      await runGraphql(deleteDefinitionMutation, { id: definition.id });
    } catch (error) {
      console.warn(`Failed to delete temporary metafield definition ${definition.id}: ${String(error)}`);
    }
  }
  return definitions;
}

async function deleteCreatedStandardDefinition(): Promise<DefinitionNode | null> {
  if (baselineStandardDefinition) {
    return null;
  }
  const read = await runGraphql(readStandardDefinitionQuery);
  const definition = (read.data?.metafieldDefinition as DefinitionNode | null | undefined) ?? null;
  if (!definition) {
    return null;
  }
  try {
    await runGraphql(deleteDefinitionMutation, { id: definition.id });
  } catch (error) {
    console.warn(`Failed to delete created standard metafield definition ${definition.id}: ${String(error)}`);
  }
  return definition;
}

async function restoreBaselinePins(): Promise<void> {
  const ascending = [...baselinePinned].sort((left, right) => (left.pinnedPosition ?? 0) - (right.pinnedPosition ?? 0));
  for (const definition of ascending) {
    try {
      await runGraphql(pinByIdMutation, { definitionId: definition.id });
    } catch (error) {
      console.warn(`Failed to restore pinned metafield definition ${definition.id}: ${String(error)}`);
    }
  }
}

let primaryResponse: GraphqlResponse | null = null;
let readAfterResponse: GraphqlResponse | null = null;
let readAfterVariables: { definitionId: string } | null = null;
let deletedDefinitions: DefinitionNode[] = [];
let deletedStandardDefinition: DefinitionNode | null = null;
const upstreamCalls: UpstreamCall[] = [];

try {
  await mkdir(outputDir, { recursive: true });

  for (const definition of baselinePinned) {
    await runGraphql(unpinByIdMutation, { definitionId: definition.id });
  }

  upstreamCalls.push(await recordIdentity(namespace, 'pin_01'));
  upstreamCalls.push(...(await recordResourceScope()));
  upstreamCalls.push(
    await recordUpstreamCall('MetafieldDefinitionsHydratePinnedOwner', hydratePinnedOwnerDocument, {
      ownerType: 'PRODUCT',
    }),
  );
  for (let index = 2; index <= 20; index += 1) {
    upstreamCalls.push(await recordIdentity(namespace, `pin_${String(index).padStart(2, '0')}`));
  }
  upstreamCalls.push(await recordIdentity('descriptors', 'subtitle'));

  primaryResponse = (await runGraphql(primaryDocument, variables)) as GraphqlResponse;
  readAfterVariables = { definitionId: readStandardInitialId(primaryResponse) };
  readAfterResponse = (await runGraphql(readAfterDocument, readAfterVariables)) as GraphqlResponse;
} finally {
  deletedDefinitions = await deleteNamespaceDefinitions();
  deletedStandardDefinition = await deleteCreatedStandardDefinition();
  await restoreBaselinePins();
}

await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      storeDomain,
      apiVersion,
      variables,
      baselinePinnedDefinitions: baselinePinned,
      baselineStandardDefinition,
      primary: {
        request: {
          documentPath: primaryDocumentPath,
          variables,
        },
        response: primaryResponse,
      },
      readAfter: {
        request: {
          documentPath: readAfterDocumentPath,
          variables: readAfterVariables,
        },
        response: readAfterResponse,
      },
      cleanup: {
        deletedDefinitions,
        deletedStandardDefinition,
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
      deletedDefinitionCount: deletedDefinitions.length,
      deletedStandardDefinitionId: deletedStandardDefinition?.id ?? null,
    },
    null,
    2,
  ),
);

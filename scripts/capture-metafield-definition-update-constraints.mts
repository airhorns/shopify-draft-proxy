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
const outputPath = path.join(outputDir, 'metafield-definition-update-constraints.json');
const primaryDocumentPath = 'config/parity-requests/metafields/metafield-definition-update-constraints.graphql';
const readAfterDocumentPath = 'config/parity-requests/metafields/metafield-definition-update-constraints-read.graphql';

const { runGraphql, runGraphqlRaw, runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

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

const namespace = `constraint_update_${Date.now().toString(36)}`;
const variables = {
  namespace,
  categoryId: 'gid://shopify/TaxonomyCategory/ap-2',
  alternateCategoryId: 'gid://shopify/TaxonomyCategory/ap-2-1',
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

const constraintsSetProbeDocument = `#graphql
  mutation MetafieldDefinitionUpdateConstraintsSetProbe($namespace: String!) {
    constraintsSetProbe: metafieldDefinitionUpdate(
      definition: {
        namespace: $namespace
        key: "tier"
        ownerType: PRODUCT
        constraintsSet: { key: "category", values: [] }
      }
    ) {
      updatedDefinition {
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

const readNamespaceDefinitionsQuery = `#graphql
  query TemporaryNamespaceDefinitions($namespace: String!) {
    metafieldDefinitions(ownerType: PRODUCT, first: 50, namespace: $namespace) {
      nodes {
        id
        key
      }
    }
  }
`;

const readPinnedDefinitionsQuery = `#graphql
  query ExistingPinnedMetafieldDefinitions {
    metafieldDefinitions(ownerType: PRODUCT, first: 50, pinnedStatus: PINNED, sortKey: PINNED_POSITION) {
      nodes { id namespace key pinnedPosition }
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
      userErrors {
        field
        message
        code
      }
    }
  }
`;

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

async function restoreBaselinePins(definitions: DefinitionNode[]): Promise<void> {
  const ascending = [...definitions].sort((left, right) => (left.pinnedPosition ?? 0) - (right.pinnedPosition ?? 0));
  for (const definition of ascending) {
    try {
      await runGraphql(pinByIdMutation, { definitionId: definition.id });
    } catch (error) {
      console.warn(`Failed to restore pinned metafield definition ${definition.id}: ${String(error)}`);
    }
  }
}

const baselinePinned =
  ((await runGraphql(readPinnedDefinitionsQuery)).data?.metafieldDefinitions?.nodes as DefinitionNode[] | undefined) ??
  [];

let primaryResponse: unknown = null;
let readAfterResponse: unknown = null;
let constraintsSetProbeResponse: unknown = null;
let deletedDefinitions: DefinitionNode[] = [];
const upstreamCalls: UpstreamCall[] = [];

try {
  await mkdir(outputDir, { recursive: true });
  for (const definition of baselinePinned) {
    await runGraphql(unpinByIdMutation, { definitionId: definition.id });
  }
  upstreamCalls.push(
    await recordUpstreamCall('MetafieldDefinitionHydrateByIdentifier', hydrateByIdentifierDocument, {
      identifier: { ownerType: 'PRODUCT', namespace, key: 'tier' },
    }),
  );
  upstreamCalls.push(...(await recordResourceScope()));
  upstreamCalls.push(
    await recordUpstreamCall('MetafieldDefinitionsHydratePinnedOwner', hydratePinnedOwnerDocument, {
      ownerType: 'PRODUCT',
    }),
  );
  primaryResponse = await runGraphql(primaryDocument, variables);
  readAfterResponse = await runGraphql(readAfterDocument, variables);
  constraintsSetProbeResponse = await runGraphqlRequest(constraintsSetProbeDocument, { namespace });
} finally {
  deletedDefinitions = await deleteNamespaceDefinitions();
  await restoreBaselinePins(baselinePinned);
}

await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      storeDomain,
      apiVersion,
      variables,
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
          variables,
        },
        response: readAfterResponse,
      },
      constraintsSetProbe: {
        request: {
          document: constraintsSetProbeDocument,
          variables: { namespace },
        },
        response: constraintsSetProbeResponse,
      },
      cleanup: {
        deletedDefinitions,
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
      deletedDefinitionCount: deletedDefinitions.length,
    },
    null,
    2,
  ),
);

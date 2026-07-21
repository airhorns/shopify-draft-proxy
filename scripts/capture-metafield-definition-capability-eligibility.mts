/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type DefinitionNode = {
  id: string;
  namespace?: string;
  key?: string;
  capabilities?: {
    adminFilterable?: {
      enabled?: boolean;
    };
  };
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metafields');
const outputPath = path.join(outputDir, 'metafield-definition-capability-eligibility.json');
const primaryDocumentPath = 'config/parity-requests/metafields/metafield-definition-capability-eligibility.graphql';

const { runGraphql, runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const namespace = `capability_eligibility_${Date.now().toString(36)}`;
const variables = { namespace };
const primaryDocument = await readFile(primaryDocumentPath, 'utf8');
const hydrateByIdentifierDocument = await readFile(
  'config/parity-requests/metafields/metafield-definition-hydrate-by-identifier.graphql',
  'utf8',
);
const hydrateResourceScopeDocument = await readFile(
  'config/parity-requests/metafields/metafield-definitions-hydrate-resource-scope.graphql',
  'utf8',
);

const readNamespaceDefinitionsQuery = `#graphql
  query TemporaryMetafieldDefinitions($ownerType: MetafieldOwnerType!, $namespace: String!) {
    metafieldDefinitions(ownerType: $ownerType, first: 100, namespace: $namespace) {
      nodes {
        id
        namespace
        key
        capabilities {
          adminFilterable { enabled }
        }
      }
    }
  }
`;

const readProductDefinitionsQuery = `#graphql
  query ProductMetafieldDefinitionsForAdminFilterableLimit {
    metafieldDefinitions(ownerType: PRODUCT, first: 250) {
      nodes {
        id
        namespace
        key
        capabilities {
          adminFilterable { enabled }
        }
      }
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

function definitionNodes(value: unknown): DefinitionNode[] {
  const data = value && typeof value === 'object' ? (value as Record<string, unknown>)['data'] : null;
  const root = data && typeof data === 'object' ? (data as Record<string, unknown>)['metafieldDefinitions'] : null;
  const nodes = root && typeof root === 'object' ? (root as Record<string, unknown>)['nodes'] : null;
  return Array.isArray(nodes) ? (nodes as DefinitionNode[]) : [];
}

async function readNamespaceDefinitions(ownerType: 'PRODUCT' | 'CUSTOMER'): Promise<DefinitionNode[]> {
  return definitionNodes(await runGraphql(readNamespaceDefinitionsQuery, { ownerType, namespace }));
}

async function deleteDefinitions(definitions: DefinitionNode[]): Promise<unknown[]> {
  const results: unknown[] = [];
  for (const definition of definitions) {
    try {
      results.push(await runGraphql(deleteDefinitionMutation, { id: definition.id }));
    } catch (error) {
      results.push({ id: definition.id, error: String(error) });
    }
  }
  return results;
}

async function deleteNamespaceDefinitions(): Promise<unknown[]> {
  const productDefinitions = await readNamespaceDefinitions('PRODUCT');
  const customerDefinitions = await readNamespaceDefinitions('CUSTOMER');
  return deleteDefinitions([...productDefinitions, ...customerDefinitions]);
}

function collectCreatedDefinitions(response: unknown): DefinitionNode[] {
  const data = response && typeof response === 'object' ? (response as Record<string, unknown>)['data'] : null;
  if (!data || typeof data !== 'object') {
    return [];
  }
  return Object.values(data as Record<string, unknown>).flatMap((payload) => {
    if (!payload || typeof payload !== 'object') {
      return [];
    }
    const definition = (payload as Record<string, unknown>)['createdDefinition'];
    return definition && typeof definition === 'object' ? [definition as DefinitionNode] : [];
  });
}

function readObject(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as Record<string, unknown>) : null;
}

function readPath(value: unknown, parts: string[]): unknown {
  let current = value;
  for (const part of parts) current = readObject(current)?.[part];
  return current;
}

async function recordUpstreamCall(operationName: string, query: string, variables: Record<string, unknown>) {
  const result = await runGraphqlRaw(query, variables);
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${operationName} failed: ${JSON.stringify(result, null, 2)}`);
  }
  return {
    method: 'POST',
    apiSurface: 'admin',
    path: `/admin/api/${apiVersion}/graphql.json`,
    operationName,
    variables,
    query,
    response: { status: result.status, body: result.payload },
  };
}

async function recordResourceScopeHydrate() {
  const calls = [];
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

async function recordPrimaryPrerequisites() {
  const identities = [
    { ownerType: 'PRODUCT', namespace, key: 'external_id' },
    { ownerType: 'PRODUCT', namespace, key: 'json_payload' },
    ...Array.from({ length: 51 }, (_, index) => ({
      ownerType: 'PRODUCT',
      namespace,
      key: `admin_${String(index + 1).padStart(2, '0')}`,
    })),
    { ownerType: 'PRODUCT', namespace: 'shopify', key: 'material' },
  ];
  const calls = [];
  for (const identifier of identities) {
    calls.push(
      await recordUpstreamCall('MetafieldDefinitionHydrateByIdentifier', hydrateByIdentifierDocument, {
        identifier,
      }),
    );
  }
  calls.push(...(await recordResourceScopeHydrate()));
  return calls;
}

let primaryResponse: unknown = null;
let preflightAdminFilterableDefinitions: DefinitionNode[] = [];
let cleanup: unknown[] = [];
let upstreamCalls: unknown[] = [];

try {
  await mkdir(outputDir, { recursive: true });
  await deleteNamespaceDefinitions();

  const preflight = await runGraphql(readProductDefinitionsQuery, {});
  preflightAdminFilterableDefinitions = definitionNodes(preflight).filter(
    (definition) => definition.capabilities?.adminFilterable?.enabled === true,
  );
  if (preflightAdminFilterableDefinitions.length > 0) {
    throw new Error(
      `Expected no pre-existing PRODUCT admin-filterable definitions; found ${preflightAdminFilterableDefinitions.length}: ${JSON.stringify(
        preflightAdminFilterableDefinitions,
      )}`,
    );
  }

  upstreamCalls = await recordPrimaryPrerequisites();
  primaryResponse = await runGraphql(primaryDocument, variables);
} finally {
  cleanup = await deleteNamespaceDefinitions();
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
      evidence: {
        preflightAdminFilterableDefinitions,
        createdDefinitions: collectCreatedDefinitions(primaryResponse),
      },
      cleanup,
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
      createdDefinitionCount: collectCreatedDefinitions(primaryResponse).length,
      cleanupCount: cleanup.length,
    },
    null,
    2,
  ),
);

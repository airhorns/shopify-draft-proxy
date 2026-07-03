/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metafields');
const outputPath = path.join(outputDir, 'metafield-definition-catalog-connection.json');
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

type GraphqlCapture = {
  label: string;
  request: {
    query: string;
    variables: Record<string, unknown>;
  };
  status: number;
  response: unknown;
};

const createDefinitionsDocument = await readRequestDocument(
  'config/parity-requests/metafields/metafield-definition-catalog-connection-create.graphql',
);
const firstPageDocument = await readRequestDocument(
  'config/parity-requests/metafields/metafield-definition-catalog-connection-first-page.graphql',
);
const afterDocument = await readRequestDocument(
  'config/parity-requests/metafields/metafield-definition-catalog-connection-after.graphql',
);
const sortQueryDocument = await readRequestDocument(
  'config/parity-requests/metafields/metafield-definition-catalog-connection-sort-query.graphql',
);

const deleteDefinitionMutation = `#graphql
  mutation DeleteCatalogConnectionDefinition($id: ID!) {
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

const suffix = Date.now().toString(36);
const namespace = `catalog_connection_${suffix}`;
const createVariables = {
  firstDefinition: definitionInput(namespace, 'beta', 'Zulu'),
  secondDefinition: definitionInput(namespace, 'alpha', 'Alpha'),
  thirdDefinition: definitionInput(namespace, 'gamma', 'Mike'),
};

const captures: Record<string, GraphqlCapture> = {};
const cleanup: GraphqlCapture[] = [];
const createdDefinitionIds: string[] = [];

try {
  captures.createDefinitions = await capture('createDefinitions', createDefinitionsDocument, createVariables);
  requireNoUserErrors(captures.createDefinitions.response, ['data', 'first', 'userErrors'], 'first definition create');
  requireNoUserErrors(
    captures.createDefinitions.response,
    ['data', 'second', 'userErrors'],
    'second definition create',
  );
  requireNoUserErrors(captures.createDefinitions.response, ['data', 'third', 'userErrors'], 'third definition create');
  for (const alias of ['first', 'second', 'third']) {
    createdDefinitionIds.push(
      requireString(
        readPath(captures.createDefinitions.response, ['data', alias, 'createdDefinition', 'id']),
        `${alias} created definition id`,
      ),
    );
  }

  captures.firstPage = await capture('firstPage', firstPageDocument, { namespace });
  const firstPageCursor = requireString(
    readPath(captures.firstPage.response, ['data', 'firstPage', 'pageInfo', 'endCursor']),
    'firstPage endCursor',
  );

  captures.secondPage = await capture('secondPage', afterDocument, {
    namespace,
    after: firstPageCursor,
  });
  captures.sortAndQuery = await captureIndexedSortAndQuery(namespace);
} finally {
  for (const id of createdDefinitionIds.reverse()) {
    cleanup.push(await capture('cleanupDefinition', deleteDefinitionMutation, { id }));
  }
}

if (!captures.createDefinitions || !captures.firstPage || !captures.secondPage || !captures.sortAndQuery) {
  throw new Error('Metafield definition catalog connection capture did not produce all required captures.');
}

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      scenarioId: 'metafield-definition-catalog-connection',
      storeDomain,
      apiVersion,
      namespace,
      captures,
      cleanup,
      upstreamCalls: [],
      notes:
        'Live Shopify capture for metafieldDefinitions default ID ordering, first/after cursor windows, query filtering, sortKey NAME, reverse, and the local unrecognized-filter policy target.',
    },
    null,
    2,
  )}\n`,
);
console.log(`Wrote ${outputPath}`);

async function readRequestDocument(documentPath: string): Promise<string> {
  return await readFile(documentPath, 'utf8');
}

function definitionInput(namespace: string, key: string, name: string): Record<string, unknown> {
  return {
    ownerType: 'PRODUCT',
    namespace,
    key,
    name,
    type: 'single_line_text_field',
  };
}

async function capture(label: string, query: string, variables: Record<string, unknown>): Promise<GraphqlCapture> {
  const result = await runGraphqlRaw(query, variables);
  assertHttpOk(result, label);
  return {
    label,
    request: { query, variables },
    status: result.status,
    response: result.payload,
  };
}

async function captureIndexedSortAndQuery(namespace: string): Promise<GraphqlCapture> {
  const variables = { namespace };
  let lastCapture: GraphqlCapture | null = null;
  for (let attempt = 1; attempt <= 15; attempt += 1) {
    lastCapture = await capture('sortAndQuery', sortQueryDocument, variables);
    if (sortAndQueryIndexed(lastCapture.response)) return lastCapture;
    await sleep(2000);
  }
  throw new Error(
    `sort/query capture did not observe indexed definition rows after polling: ${JSON.stringify(
      lastCapture?.response,
      null,
      2,
    )}`,
  );
}

function sortAndQueryIndexed(response: unknown): boolean {
  return (
    readNodeKeys(response, ['data', 'matching']).join(',') === 'alpha' &&
    readNodeKeys(response, ['data', 'unknownFilter']).join(',') === 'beta,alpha,gamma' &&
    readNodeKeys(response, ['data', 'nameDescending']).join(',') === 'beta,gamma,alpha'
  );
}

function readNodeKeys(value: unknown, pathParts: string[]): string[] {
  const nodes = readPath(value, [...pathParts, 'nodes']);
  if (!Array.isArray(nodes)) return [];
  return nodes
    .map((node) =>
      node && typeof node === 'object' && !Array.isArray(node) ? (node as Record<string, unknown>)['key'] : undefined,
    )
    .filter((key): key is string => typeof key === 'string');
}

async function sleep(ms: number): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, ms));
}

function assertHttpOk(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function readPath(value: unknown, pathParts: string[]): unknown {
  let cursor = value;
  for (const pathPart of pathParts) {
    if (!cursor || typeof cursor !== 'object' || Array.isArray(cursor)) return undefined;
    cursor = (cursor as Record<string, unknown>)[pathPart];
  }
  return cursor;
}

function requireString(value: unknown, label: string): string {
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`${label} was missing: ${JSON.stringify(value)}`);
  }
  return value;
}

function requireNoUserErrors(value: unknown, pathParts: string[], label: string): void {
  const userErrors = readPath(value, pathParts);
  if (Array.isArray(userErrors) && userErrors.length === 0) return;
  throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors)}`);
}

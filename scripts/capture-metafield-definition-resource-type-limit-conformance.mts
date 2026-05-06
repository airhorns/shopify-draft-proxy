/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type CreatedDefinition = {
  id: string;
  namespace?: string;
  key?: string;
};

type Capture = {
  label: string;
  request: {
    documentPath: string;
    query: string;
    variables: Record<string, unknown>;
  };
  status: number;
  response: unknown;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metafields');
const outputPath = path.join(outputDir, 'metafield-definition-resource-type-limit.json');
const batchDocumentPaths = [1, 2, 3, 4, 5, 6].map(
  (batch) => `config/parity-requests/metafields/metafield-definition-resource-type-limit-batch-${batch}.graphql`,
);

const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const namespace = `resource_type_limit_${Date.now().toString(36)}`;
const variables = { namespace };
const deleteDefinitionMutation = `#graphql
  mutation DeleteTemporaryMetafieldDefinition($id: ID!) {
    metafieldDefinitionDelete(id: $id, deleteAllAssociatedMetafields: true) {
      deletedDefinitionId
      userErrors { field message code }
    }
  }
`;

function assertHttpOk(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function readObject(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as Record<string, unknown>) : null;
}

function collectCreatedDefinitions(response: unknown): CreatedDefinition[] {
  const data = readObject(readObject(response)?.['data']);
  if (!data) return [];

  return Object.values(data).flatMap((payload) => {
    const createdDefinition = readObject(readObject(payload)?.['createdDefinition']);
    const id = createdDefinition?.['id'];
    return typeof id === 'string'
      ? [
          {
            id,
            namespace:
              typeof createdDefinition['namespace'] === 'string' ? String(createdDefinition['namespace']) : undefined,
            key: typeof createdDefinition['key'] === 'string' ? String(createdDefinition['key']) : undefined,
          },
        ]
      : [];
  });
}

async function sleep(ms: number): Promise<void> {
  await new Promise((resolve) => {
    setTimeout(resolve, ms);
  });
}

async function capture(documentPath: string, label: string): Promise<Capture> {
  const query = await readFile(documentPath, 'utf8');
  const result = await runGraphqlRaw(query, variables);
  assertHttpOk(result, label);
  return {
    label,
    request: {
      documentPath,
      query,
      variables,
    },
    status: result.status,
    response: result.payload,
  };
}

async function cleanupDefinitions(definitions: CreatedDefinition[]): Promise<unknown[]> {
  const cleanup: unknown[] = [];
  for (const definition of definitions.reverse()) {
    try {
      cleanup.push({
        id: definition.id,
        response: (await runGraphqlRaw(deleteDefinitionMutation, { id: definition.id })).payload,
      });
    } catch (error) {
      cleanup.push({ id: definition.id, error: String(error) });
    }
    await sleep(100);
  }
  return cleanup;
}

const batches: Capture[] = [];
let cleanup: unknown[] = [];

try {
  await mkdir(outputDir, { recursive: true });

  for (const [index, documentPath] of batchDocumentPaths.entries()) {
    batches.push(await capture(documentPath, `metafieldDefinitionCreate resource type limit batch ${index + 1}`));
    await sleep(1500);
  }
} finally {
  cleanup = await cleanupDefinitions(batches.flatMap((batch) => collectCreatedDefinitions(batch.response)));
}

const finalPayload = readObject(readObject(readObject(batches.at(-1)?.response)?.['data'])?.['create257']);

await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      storeDomain,
      apiVersion,
      namespace,
      variables,
      batches,
      evidence: {
        finalPayload,
        createdDefinitionCount: batches.flatMap((batch) => collectCreatedDefinitions(batch.response)).length,
      },
      cleanup,
      upstreamCalls: [],
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
      createdDefinitionCount: batches.flatMap((batch) => collectCreatedDefinitions(batch.response)).length,
      finalPayload,
      cleanupCount: cleanup.length,
    },
    null,
    2,
  ),
);

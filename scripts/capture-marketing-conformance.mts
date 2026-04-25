/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion);
const baselineOutputPath = path.join(outputDir, 'marketing-baseline-read.json');
const invalidIdOutputPath = path.join(outputDir, 'marketing-invalid-id-read.json');
const schemaOutputPath = path.join(outputDir, 'marketing-schema-inventory.json');
const documentPath = path.join('config', 'parity-requests', 'marketing-baseline-read.graphql');
const variablesPath = path.join('config', 'parity-requests', 'marketing-baseline-read.variables.json');

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const seedDocument = `#graphql
  query MarketingSeedIds($first: Int!) {
    marketingActivities(first: $first, sortKey: CREATED_AT, reverse: true) {
      edges {
        node {
          id
        }
      }
    }
    marketingEvents(first: $first, sortKey: ID, reverse: true) {
      edges {
        node {
          id
        }
      }
    }
  }
`;

const schemaInventoryDocument = `#graphql
  query MarketingSchemaInventory {
    queryRoot: __type(name: "QueryRoot") {
      fields {
        name
        args {
          name
          type {
            ...TypeRef
          }
        }
      }
    }
    marketingActivityType: __type(name: "MarketingActivity") {
      fields {
        name
        type {
          ...TypeRef
        }
      }
    }
    marketingEventType: __type(name: "MarketingEvent") {
      fields {
        name
        type {
          ...TypeRef
        }
      }
    }
    marketingActivitySortKeys: __type(name: "MarketingActivitySortKeys") {
      enumValues {
        name
      }
    }
    marketingEventSortKeys: __type(name: "MarketingEventSortKeys") {
      enumValues {
        name
      }
    }
  }

  fragment TypeRef on __Type {
    kind
    name
    ofType {
      kind
      name
      ofType {
        kind
        name
        ofType {
          kind
          name
        }
      }
    }
  }
`;

const invalidIdDocument = `#graphql
  query MarketingInvalidIdRead($activityId: ID!, $eventId: ID!) {
    invalidActivity: marketingActivity(id: $activityId) {
      id
    }
    invalidEvent: marketingEvent(id: $eventId) {
      id
    }
  }
`;

function readFirstId(payload: unknown, rootField: 'marketingActivities' | 'marketingEvents'): string | null {
  if (!payload || typeof payload !== 'object' || !('data' in payload)) {
    return null;
  }

  const data = (payload as { data?: unknown }).data;
  if (!data || typeof data !== 'object') {
    return null;
  }

  const connection = (data as Record<string, unknown>)[rootField];
  if (!connection || typeof connection !== 'object' || !('edges' in connection)) {
    return null;
  }

  const edges = (connection as { edges?: unknown }).edges;
  if (!Array.isArray(edges)) {
    return null;
  }

  for (const edge of edges) {
    if (!edge || typeof edge !== 'object' || !('node' in edge)) {
      continue;
    }

    const node = (edge as { node?: unknown }).node;
    if (!node || typeof node !== 'object' || !('id' in node)) {
      continue;
    }

    const id = (node as { id?: unknown }).id;
    if (typeof id === 'string' && id.length > 0) {
      return id;
    }
  }

  return null;
}

function filterSchemaInventory(payload: unknown): unknown {
  if (!payload || typeof payload !== 'object' || !('data' in payload)) {
    return payload;
  }

  const cloned = structuredClone(payload) as { data?: { queryRoot?: { fields?: unknown } } };
  const fields = cloned.data?.queryRoot?.fields;
  if (Array.isArray(fields)) {
    cloned.data!.queryRoot!.fields = fields.filter((field) => {
      return (
        field &&
        typeof field === 'object' &&
        'name' in field &&
        typeof field.name === 'string' &&
        ['marketingActivities', 'marketingActivity', 'marketingEvents', 'marketingEvent'].includes(field.name)
      );
    });
  }

  return cloned;
}

async function assertHttpOk(label: string, result: { status: number; payload: unknown }): Promise<void> {
  if (result.status >= 200 && result.status < 300) {
    return;
  }

  console.error(JSON.stringify(result.payload, null, 2));
  throw new Error(`${label} failed with HTTP ${result.status}`);
}

await mkdir(outputDir, { recursive: true });

const document = await readFile(documentPath, 'utf8');
const variables = JSON.parse(await readFile(variablesPath, 'utf8')) as Record<string, unknown>;
const first = typeof variables['first'] === 'number' ? variables['first'] : 3;

const seedResult = await runGraphqlRequest(seedDocument, { first });
await assertHttpOk('Marketing seed capture', seedResult);

const activityId = readFirstId(seedResult.payload, 'marketingActivities') ?? variables['activityId'];
const eventId = readFirstId(seedResult.payload, 'marketingEvents') ?? variables['eventId'];
const captureVariables = {
  ...variables,
  activityId,
  eventId,
};

const baselineResult = await runGraphqlRequest(document, captureVariables);
await assertHttpOk('Marketing baseline capture', baselineResult);
await writeFile(variablesPath, `${JSON.stringify(captureVariables, null, 2)}\n`, 'utf8');
await writeFile(baselineOutputPath, `${JSON.stringify(baselineResult.payload, null, 2)}\n`, 'utf8');

const invalidIdResult = await runGraphqlRequest(invalidIdDocument, {
  activityId: 'not-a-shopify-marketing-activity-gid',
  eventId: 'not-a-shopify-marketing-event-gid',
});
await assertHttpOk('Marketing invalid-id capture', invalidIdResult);
await writeFile(invalidIdOutputPath, `${JSON.stringify(invalidIdResult.payload, null, 2)}\n`, 'utf8');

const schemaResult = await runGraphqlRequest(schemaInventoryDocument);
await assertHttpOk('Marketing schema inventory capture', schemaResult);
const filteredSchemaInventory = filterSchemaInventory(schemaResult.payload);
await writeFile(schemaOutputPath, `${JSON.stringify(filteredSchemaInventory, null, 2)}\n`, 'utf8');

console.log(
  JSON.stringify(
    {
      ok: true,
      outputDir,
      apiVersion,
      files: ['marketing-baseline-read.json', 'marketing-invalid-id-read.json', 'marketing-schema-inventory.json'],
      first,
      activityId,
      eventId,
      baselineErrors: Array.isArray(baselineResult.payload.errors) ? baselineResult.payload.errors.length : 0,
      invalidIdErrors: Array.isArray(invalidIdResult.payload.errors) ? invalidIdResult.payload.errors.length : 0,
    },
    null,
    2,
  ),
);

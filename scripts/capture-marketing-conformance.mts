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
const nativeActivityValidationOutputPath = path.join(outputDir, 'marketing-native-activity-validation.json');
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

const nativeActivityInventoryDocument = `#graphql
  query NativeMarketingActivityInventory {
    currentAppInstallation {
      accessScopes {
        handle
      }
    }
    createInput: __type(name: "MarketingActivityCreateInput") {
      inputFields {
        name
      }
    }
    updateInput: __type(name: "MarketingActivityUpdateInput") {
      inputFields {
        name
      }
    }
    createPayload: __type(name: "MarketingActivityCreatePayload") {
      fields {
        name
      }
    }
    updatePayload: __type(name: "MarketingActivityUpdatePayload") {
      fields {
        name
      }
    }
  }
`;

const nativeActivityCreateValidationDocument = `#graphql
  mutation NativeCreateValidation($input: MarketingActivityCreateInput!) {
    marketingActivityCreate(input: $input) {
      userErrors {
        field
        message
      }
    }
  }
`;

const nativeActivityUpdateProbeDocument = `#graphql
  mutation NativeUpdateMissing($input: MarketingActivityUpdateInput!) {
    marketingActivityUpdate(input: $input) {
      marketingActivity {
        id
      }
      redirectPath
      userErrors {
        field
        message
      }
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

function readStringList(value: unknown, key: string): string[] {
  if (!value || typeof value !== 'object' || !(key in value)) {
    return [];
  }

  const items = (value as Record<string, unknown>)[key];
  if (!Array.isArray(items)) {
    return [];
  }

  return items.flatMap((item): string[] => {
    if (item && typeof item === 'object' && 'name' in item && typeof item.name === 'string') {
      return [item.name];
    }
    return [];
  });
}

function buildNativeActivityValidationFixture({
  inventoryPayload,
  invalidExtensionResult,
  updateProbeResult,
}: {
  inventoryPayload: unknown;
  invalidExtensionResult: { status: number; payload: unknown };
  updateProbeResult: { status: number; payload: unknown };
}): unknown {
  const data =
    inventoryPayload && typeof inventoryPayload === 'object' && 'data' in inventoryPayload
      ? (inventoryPayload as { data?: Record<string, unknown> }).data
      : {};
  const scopesRaw = data?.['currentAppInstallation'];
  const accessScopes =
    scopesRaw && typeof scopesRaw === 'object' && 'accessScopes' in scopesRaw
      ? (scopesRaw as { accessScopes?: unknown }).accessScopes
      : [];
  const scopeHandles = Array.isArray(accessScopes)
    ? accessScopes.flatMap((scope): string[] => {
        return scope && typeof scope === 'object' && 'handle' in scope && typeof scope.handle === 'string'
          ? [scope.handle]
          : [];
      })
    : [];

  return {
    scenarioId: 'marketing-native-activity-lifecycle',
    apiVersion,
    storeDomain,
    capturedAt: new Date().toISOString(),
    accessScopes: scopeHandles.filter(
      (scope) => scope === 'read_marketing_events' || scope === 'write_marketing_events',
    ),
    schema: {
      marketingActivityCreateInputFields: readStringList(data?.['createInput'], 'inputFields'),
      marketingActivityUpdateInputFields: readStringList(data?.['updateInput'], 'inputFields'),
      marketingActivityCreatePayloadFields: readStringList(data?.['createPayload'], 'fields'),
      marketingActivityUpdatePayloadFields: readStringList(data?.['updatePayload'], 'fields'),
    },
    operations: {
      validation: {
        invalidExtension: {
          request: {
            query: nativeActivityCreateValidationDocument,
            variables: {
              input: {
                marketingActivityExtensionId:
                  'gid://shopify/MarketingActivityExtension/00000000-0000-0000-0000-000000000000',
                status: 'DRAFT',
              },
            },
          },
          response: invalidExtensionResult.payload,
        },
        updateOutsideExtensionContext: {
          request: {
            query: nativeActivityUpdateProbeDocument,
            variables: {
              input: {
                id: 'gid://shopify/MarketingActivity/999999999999',
              },
            },
          },
          response: updateProbeResult.payload,
        },
      },
    },
    blockers: {
      successPath:
        'The conformance app/store has read/write marketing scopes, but no deprecated MarketingActivityExtension is installed or discoverable through Admin GraphQL. Shopify returns `Could not find the marketing extension` for arbitrary extension IDs, and update probes outside extension context return ACCESS_DENIED, so live success-path capture is blocked until the conformance app includes a deprecated marketing activity app extension.',
    },
    localRuntimeExpectations: {
      updatedActivity: {
        id: 'gid://shopify/MarketingActivity/1',
        title: 'HAR-373 Native Activity Active',
        status: 'ACTIVE',
        statusLabel: 'Sending',
        isExternal: false,
        inMainWorkflowVersion: true,
        marketingEvent: null,
      },
    },
  };
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

const nativeInventoryResult = await runGraphqlRequest(nativeActivityInventoryDocument);
await assertHttpOk('Native marketing activity inventory capture', nativeInventoryResult);
const nativeInvalidExtensionVariables = {
  input: {
    marketingActivityExtensionId: 'gid://shopify/MarketingActivityExtension/00000000-0000-0000-0000-000000000000',
    status: 'DRAFT',
  },
};
const nativeInvalidExtensionResult = await runGraphqlRequest(
  nativeActivityCreateValidationDocument,
  nativeInvalidExtensionVariables,
);
await assertHttpOk('Native marketing activity invalid extension capture', nativeInvalidExtensionResult);
const nativeUpdateProbeVariables = {
  input: {
    id: 'gid://shopify/MarketingActivity/999999999999',
  },
};
const nativeUpdateProbeResult = await runGraphqlRequest(nativeActivityUpdateProbeDocument, nativeUpdateProbeVariables);
await assertHttpOk('Native marketing activity update probe capture', nativeUpdateProbeResult);
await writeFile(
  nativeActivityValidationOutputPath,
  `${JSON.stringify(
    buildNativeActivityValidationFixture({
      inventoryPayload: nativeInventoryResult.payload,
      invalidExtensionResult: nativeInvalidExtensionResult,
      updateProbeResult: nativeUpdateProbeResult,
    }),
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(
  JSON.stringify(
    {
      ok: true,
      outputDir,
      apiVersion,
      files: [
        'marketing-baseline-read.json',
        'marketing-invalid-id-read.json',
        'marketing-schema-inventory.json',
        'marketing-native-activity-validation.json',
      ],
      first,
      activityId,
      eventId,
      baselineErrors: Array.isArray(baselineResult.payload.errors) ? baselineResult.payload.errors.length : 0,
      invalidIdErrors: Array.isArray(invalidIdResult.payload.errors) ? invalidIdResult.payload.errors.length : 0,
      nativeInvalidExtensionErrors: Array.isArray(nativeInvalidExtensionResult.payload.errors)
        ? nativeInvalidExtensionResult.payload.errors.length
        : 0,
      nativeUpdateProbeErrors: Array.isArray(nativeUpdateProbeResult.payload.errors)
        ? nativeUpdateProbeResult.payload.errors.length
        : 0,
    },
    null,
    2,
  ),
);

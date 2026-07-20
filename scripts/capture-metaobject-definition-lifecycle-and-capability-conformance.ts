/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

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

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metaobjects');
const runId = Date.now().toString();
const lifecycleType = `codex_definition_lifecycle_${runId}`;
const capabilityType = `codex_capability_disabled_${runId}`;

const requestPaths = {
  lifecycleCreate: 'config/parity-requests/metaobjects/metaobject-definition-lifecycle-create.graphql',
  lifecycleUpdate: 'config/parity-requests/metaobjects/metaobject-definition-lifecycle-update.graphql',
  lifecycleRead: 'config/parity-requests/metaobjects/metaobject-definition-lifecycle-read.graphql',
  lifecycleDelete: 'config/parity-requests/metaobjects/metaobject-definition-lifecycle-delete.graphql',
  lifecycleReadDeleted: 'config/parity-requests/metaobjects/metaobject-definition-lifecycle-read-deleted.graphql',
  standardEnable: 'config/parity-requests/metaobjects/metaobject-definition-standard-enable.graphql',
  standardEnableUnknown: 'config/parity-requests/metaobjects/metaobject-definition-standard-enable-unknown.graphql',
  capabilityDefinitionCreate: 'config/parity-requests/metaobjects/metaobject-capability-definition-create.graphql',
  capabilityCreate: 'config/parity-requests/metaobjects/metaobject-capability-create.graphql',
  capabilityUpdate: 'config/parity-requests/metaobjects/metaobject-capability-update.graphql',
  capabilityUpsert: 'config/parity-requests/metaobjects/metaobject-capability-upsert.graphql',
};

const queries = Object.fromEntries(
  await Promise.all(
    Object.entries(requestPaths).map(async ([name, requestPath]) => [name, await readFile(requestPath, 'utf8')]),
  ),
) as Record<keyof typeof requestPaths, string>;

const metaobjectDeleteMutation = `#graphql
  mutation MetaobjectCapabilityCleanupDelete($id: ID!) {
    metaobjectDelete(id: $id) {
      deletedId
      userErrors {
        field
        message
        code
        elementKey
        elementIndex
      }
    }
  }
`;

const definitionDeleteMutation = `#graphql
  mutation MetaobjectDefinitionCleanupDelete($id: ID!) {
    metaobjectDefinitionDelete(id: $id) {
      deletedId
      userErrors {
        field
        message
        code
        elementKey
        elementIndex
      }
    }
  }
`;

const definitionByTypeQuery = `#graphql
  query MetaobjectDefinitionCleanupByType($type: String!) {
    metaobjectDefinitionByType(type: $type) {
      id
      type
      metaobjectsCount
    }
  }
`;

function readObject(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as Record<string, unknown>) : null;
}

function readPath(value: unknown, pathParts: string[]): unknown {
  let current = value;
  for (const part of pathParts) {
    if (Array.isArray(current)) {
      const index = Number.parseInt(part, 10);
      current = Number.isInteger(index) ? current[index] : undefined;
      continue;
    }
    const object = readObject(current);
    if (!object) {
      return undefined;
    }
    current = object[part];
  }
  return current;
}

function extractString(payload: unknown, pathParts: string[], label: string): string {
  const value = readPath(payload, pathParts);
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`${label} did not return a string at ${pathParts.join('.')}: ${JSON.stringify(payload, null, 2)}`);
  }
  return value;
}

function readUserErrors(payload: unknown, pathParts: string[]): unknown[] {
  const value = readPath(payload, pathParts);
  return Array.isArray(value) ? value : [];
}

function assertGraphqlOk(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || readPath(result.payload, ['errors'])) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertNoUserErrors(payload: unknown, pathParts: string[], label: string): void {
  const errors = readUserErrors(payload, pathParts);
  if (errors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(errors, null, 2)}`);
  }
}

function assertHasUserErrorCode(payload: unknown, pathParts: string[], code: string, label: string): void {
  const errors = readUserErrors(payload, pathParts);
  if (!errors.some((error) => readObject(error)?.['code'] === code)) {
    throw new Error(`${label} did not return ${code}: ${JSON.stringify(errors, null, 2)}`);
  }
}

function captureFromResult(
  name: string,
  query: string,
  variables: Record<string, unknown>,
  result: ConformanceGraphqlResult,
): Capture {
  return {
    name,
    request: { query, variables },
    status: result.status,
    response: result.payload,
  };
}

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const client = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function runCapture(name: string, query: string, variables: Record<string, unknown>): Promise<Capture> {
  const result = await client.runGraphqlRequest(query, variables);
  assertGraphqlOk(result, name);
  return captureFromResult(name, query, variables, result);
}

async function cleanupDefinitionById(id: string | null, label: string): Promise<void> {
  if (!id) {
    return;
  }
  try {
    const result = await client.runGraphqlRequest(definitionDeleteMutation, { id });
    assertGraphqlOk(result, label);
  } catch (error) {
    console.warn(`Failed to cleanup metaobject definition ${id}:`, error);
  }
}

async function cleanupMetaobjectById(id: string | null, label: string): Promise<void> {
  if (!id) {
    return;
  }
  try {
    const result = await client.runGraphqlRequest(metaobjectDeleteMutation, { id });
    assertGraphqlOk(result, label);
  } catch (error) {
    console.warn(`Failed to cleanup metaobject ${id}:`, error);
  }
}

async function cleanupDefinitionByType(type: string, label: string): Promise<void> {
  const existing = await client.runGraphqlRequest(definitionByTypeQuery, { type });
  assertGraphqlOk(existing, label);
  const id = readPath(existing.payload, ['data', 'metaobjectDefinitionByType', 'id']);
  if (typeof id !== 'string' || id.length === 0) {
    return;
  }
  const count = readPath(existing.payload, ['data', 'metaobjectDefinitionByType', 'metaobjectsCount']);
  if (typeof count === 'number' && count > 0) {
    throw new Error(`Existing definition for ${type} has metaobjectsCount > 0; refusing to delete test data.`);
  }
  await cleanupDefinitionById(id, `${label}-delete`);
}

async function captureDefinitionLifecycle(): Promise<void> {
  const outputPath = path.join(outputDir, 'metaobject-definition-draft-flow.json');
  let lifecycleDefinitionId: string | null = null;
  let standardDefinitionId: string | null = null;

  try {
    const createVariables = {
      definition: {
        type: lifecycleType,
        name: `Definition Lifecycle ${runId}`,
        description: 'A live captured metaobject definition.',
        capabilities: {
          publishable: {
            enabled: true,
          },
          translatable: {
            enabled: false,
          },
        },
        displayNameKey: 'title',
        fieldDefinitions: [
          {
            key: 'title',
            name: 'Title',
            description: 'Title field.',
            type: 'single_line_text_field',
            required: true,
          },
          {
            key: 'body',
            name: 'Body',
            description: 'Body field.',
            type: 'multi_line_text_field',
            required: false,
            validations: [
              {
                name: 'max',
                value: '500',
              },
            ],
          },
        ],
      },
    };
    const create = await runCapture('create-definition', queries.lifecycleCreate, createVariables);
    assertNoUserErrors(create.response, ['data', 'metaobjectDefinitionCreate', 'userErrors'], 'create-definition');
    lifecycleDefinitionId = extractString(
      create.response,
      ['data', 'metaobjectDefinitionCreate', 'metaobjectDefinition', 'id'],
      'create-definition',
    );

    const updateVariables = {
      id: lifecycleDefinitionId,
      definition: {
        name: `Updated Definition ${runId}`,
        description: 'Updated live capture.',
        displayNameKey: 'body',
        resetFieldOrder: true,
        access: {
          storefront: 'PUBLIC_READ',
        },
        capabilities: {
          translatable: {
            enabled: true,
          },
          renderable: {
            enabled: true,
          },
        },
        fieldDefinitions: [
          {
            update: {
              key: 'body',
              name: 'Body Copy',
              description: 'Updated body.',
              required: true,
              validations: [
                {
                  name: 'max',
                  value: '250',
                },
              ],
            },
          },
          {
            create: {
              key: 'summary',
              name: 'Summary',
              description: 'Summary field.',
              type: 'single_line_text_field',
              required: false,
            },
          },
          {
            delete: {
              key: 'title',
            },
          },
        ],
      },
    };
    const update = await runCapture('update-definition', queries.lifecycleUpdate, updateVariables);
    assertNoUserErrors(update.response, ['data', 'metaobjectDefinitionUpdate', 'userErrors'], 'update-definition');

    const readAfterUpdate = await runCapture('read-after-update', queries.lifecycleRead, {
      id: lifecycleDefinitionId,
      type: lifecycleType,
    });

    const deleted = await runCapture('delete-definition', queries.lifecycleDelete, { id: lifecycleDefinitionId });
    assertNoUserErrors(deleted.response, ['data', 'metaobjectDefinitionDelete', 'userErrors'], 'delete-definition');
    lifecycleDefinitionId = null;

    const readAfterDelete = await runCapture('read-after-delete', queries.lifecycleReadDeleted, {
      id: extractString(deleted.response, ['data', 'metaobjectDefinitionDelete', 'deletedId'], 'delete-definition'),
      type: lifecycleType,
    });

    const standardType = 'shopify--qa-pair';
    await cleanupDefinitionByType(standardType, 'preclean-standard-definition');
    const standardEnable = await runCapture('standard-enable', queries.standardEnable, { type: standardType });
    assertNoUserErrors(
      standardEnable.response,
      ['data', 'standardMetaobjectDefinitionEnable', 'userErrors'],
      'standard-enable',
    );
    standardDefinitionId = extractString(
      standardEnable.response,
      ['data', 'standardMetaobjectDefinitionEnable', 'metaobjectDefinition', 'id'],
      'standard-enable',
    );

    const standardEnableUnknown = await runCapture('standard-enable-unknown', queries.standardEnableUnknown, {
      type: 'shopify--unknown-template',
    });
    assertHasUserErrorCode(
      standardEnableUnknown.response,
      ['data', 'standardMetaobjectDefinitionEnable', 'userErrors'],
      'RECORD_NOT_FOUND',
      'standard-enable-unknown',
    );

    await cleanupDefinitionById(standardDefinitionId, 'standard-enable-cleanup');
    standardDefinitionId = null;

    await writeFile(
      outputPath,
      `${JSON.stringify(
        {
          apiVersion,
          storeDomain,
          capturedAt: new Date().toISOString(),
          scenarioId: 'metaobject-definition-lifecycle-local-staging',
          notes:
            'Live Shopify capture for metaobject definition create/update/delete/read-after-write and bounded standard template enablement.',
          create,
          update,
          readAfterUpdate,
          delete: deleted,
          readAfterDelete,
          standardEnable,
          standardEnableUnknown,
          upstreamCalls: [],
        },
        null,
        2,
      )}\n`,
    );
    console.log(`Wrote ${outputPath}`);
  } finally {
    await cleanupDefinitionById(lifecycleDefinitionId, 'lifecycle-definition-cleanup');
    await cleanupDefinitionById(standardDefinitionId, 'standard-definition-cleanup');
  }
}

async function captureCapabilityNotEnabled(): Promise<void> {
  const outputPath = path.join(outputDir, 'metaobject-capability-not-enabled.json');
  let definitionId: string | null = null;
  let metaobjectId: string | null = null;

  try {
    const definitionCreate = await runCapture('definition-create', queries.capabilityDefinitionCreate, {
      definition: {
        type: capabilityType,
        name: `Capability Disabled ${runId}`,
        displayNameKey: 'title',
        fieldDefinitions: [
          {
            key: 'title',
            name: 'Title',
            type: 'single_line_text_field',
            required: true,
          },
        ],
      },
    });
    assertNoUserErrors(
      definitionCreate.response,
      ['data', 'metaobjectDefinitionCreate', 'userErrors'],
      'definition-create',
    );
    definitionId = extractString(
      definitionCreate.response,
      ['data', 'metaobjectDefinitionCreate', 'metaobjectDefinition', 'id'],
      'definition-create',
    );

    const validCreate = await runCapture('valid-create', queries.capabilityCreate, {
      metaobject: {
        type: capabilityType,
        handle: `existing-${runId}`,
        fields: [{ key: 'title', value: 'Original' }],
      },
    });
    assertNoUserErrors(validCreate.response, ['data', 'metaobjectCreate', 'userErrors'], 'valid-create');
    metaobjectId = extractString(
      validCreate.response,
      ['data', 'metaobjectCreate', 'metaobject', 'id'],
      'valid-create',
    );

    const updateOnlineStore = await runCapture('update-disabled-online-store', queries.capabilityUpdate, {
      id: metaobjectId,
      metaobject: {
        capabilities: {
          onlineStore: {
            templateSuffix: 'landing',
          },
        },
        fields: [{ key: 'title', value: 'Changed' }],
      },
    });
    assertHasUserErrorCode(
      updateOnlineStore.response,
      ['data', 'metaobjectUpdate', 'userErrors'],
      'CAPABILITY_NOT_ENABLED',
      'update-disabled-online-store',
    );

    const createPublishable = await runCapture('create-disabled-publishable', queries.capabilityCreate, {
      metaobject: {
        type: capabilityType,
        handle: `rejected-publishable-${runId}`,
        capabilities: {
          publishable: {
            status: 'ACTIVE',
          },
        },
        fields: [{ key: 'title', value: 'Rejected' }],
      },
    });
    assertHasUserErrorCode(
      createPublishable.response,
      ['data', 'metaobjectCreate', 'userErrors'],
      'CAPABILITY_NOT_ENABLED',
      'create-disabled-publishable',
    );

    const upsertBoth = await runCapture('upsert-disabled-both-capabilities', queries.capabilityUpsert, {
      handle: {
        type: capabilityType,
        handle: `upserted-${runId}`,
      },
      metaobject: {
        capabilities: {
          publishable: {
            status: 'ACTIVE',
          },
          onlineStore: {
            templateSuffix: 'landing',
          },
        },
        fields: [{ key: 'title', value: 'Upserted' }],
      },
    });
    assertHasUserErrorCode(
      upsertBoth.response,
      ['data', 'metaobjectUpsert', 'userErrors'],
      'CAPABILITY_NOT_ENABLED',
      'upsert-disabled-both-capabilities',
    );

    await writeFile(
      outputPath,
      `${JSON.stringify(
        {
          apiVersion,
          storeDomain,
          capturedAt: new Date().toISOString(),
          scenarioId: 'metaobject-capability-not-enabled',
          notes:
            'Live Shopify capture for disabled publishable and onlineStore capability guardrails on metaobject create/update/upsert.',
          definitionCreate,
          validCreate,
          updateOnlineStore,
          createPublishable,
          upsertBoth,
          upstreamCalls: [],
        },
        null,
        2,
      )}\n`,
    );
    console.log(`Wrote ${outputPath}`);
  } finally {
    await cleanupMetaobjectById(metaobjectId, 'capability-metaobject-cleanup');
    await cleanupDefinitionById(definitionId, 'capability-definition-cleanup');
  }
}

await mkdir(outputDir, { recursive: true });
const captureSelection = process.argv[2] ?? 'all';
if (!['all', 'lifecycle', 'capability'].includes(captureSelection)) {
  throw new Error(`Unknown capture selection ${captureSelection}; expected all, lifecycle, or capability.`);
}
if (captureSelection === 'all' || captureSelection === 'lifecycle') {
  await captureDefinitionLifecycle();
}
if (captureSelection === 'all' || captureSelection === 'capability') {
  await captureCapabilityNotEnabled();
}

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

type ScenarioCaptures = {
  type: string;
  definitionCreate: Capture;
  createOmitted: Capture;
  createEmpty: Capture;
  createCustom: Capture;
  readAfterCustomCreate: Capture;
  unrelatedUpdate: Capture;
  readAfterUnrelatedUpdate: Capture;
  explicitUpdate: Capture;
  upsertCreate: Capture;
  upsertUpdatePreserve: Capture;
  upsertUpdateEmpty: Capture;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metaobjects');
const outputPath = path.join(outputDir, 'metaobject-online-store-template-suffix.json');
const requestDir = path.join('config', 'parity-requests', 'metaobjects');
const requestPaths = {
  definitionCreate: path.join(requestDir, 'metaobject-online-store-template-suffix-definition-create.graphql'),
  create: path.join(requestDir, 'metaobject-online-store-template-suffix-create.graphql'),
  update: path.join(requestDir, 'metaobject-online-store-template-suffix-update.graphql'),
  upsert: path.join(requestDir, 'metaobject-online-store-template-suffix-upsert.graphql'),
  read: path.join(requestDir, 'metaobject-online-store-template-suffix-read.graphql'),
};

const queries = Object.fromEntries(
  await Promise.all(
    Object.entries(requestPaths).map(async ([name, requestPath]) => [name, await readFile(requestPath, 'utf8')]),
  ),
) as Record<keyof typeof requestPaths, string>;

const metaobjectDeleteMutation = `#graphql
  mutation MetaobjectOnlineStoreTemplateSuffixCleanupMetaobject($id: ID!) {
    metaobjectDelete(id: $id) {
      deletedId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const definitionDeleteMutation = `#graphql
  mutation MetaobjectOnlineStoreTemplateSuffixCleanupDefinition($id: ID!) {
    metaobjectDefinitionDelete(id: $id) {
      deletedId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const runId = Date.now().toString();
const createdDefinitionIds: string[] = [];
const createdMetaobjectIds: string[] = [];

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function readPath(value: unknown, parts: string[]): unknown {
  let current = value;
  for (const part of parts) {
    if (!isRecord(current)) {
      return undefined;
    }
    current = current[part];
  }
  return current;
}

function readStringPath(value: unknown, parts: string[], label: string): string {
  const found = readPath(value, parts);
  if (typeof found !== 'string' || found.length === 0) {
    throw new Error(`${label} did not return a string at ${parts.join('.')}: ${JSON.stringify(value, null, 2)}`);
  }
  return found;
}

function readUserErrors(value: unknown, parts: string[]): unknown[] {
  const found = readPath(value, parts);
  return Array.isArray(found) ? found : [];
}

function assertGraphqlOk(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || (isRecord(result.payload) && result.payload['errors'])) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertNoUserErrors(payload: unknown, parts: string[], label: string): void {
  const userErrors = readUserErrors(payload, parts);
  if (userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function assertTemplateSuffix(payload: unknown, parts: string[], expected: string | null, label: string): void {
  const actual = readPath(payload, parts);
  if (actual !== expected) {
    throw new Error(`${label} expected templateSuffix ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`);
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

async function captureGraphql(name: string, query: string, variables: Record<string, unknown>): Promise<Capture> {
  const result = await client.runGraphqlRequest(query, variables);
  assertGraphqlOk(result, name);
  return captureFromResult(name, query, variables, result);
}

function definitionInput(type: string): Record<string, unknown> {
  return {
    type,
    name: `Template suffix ${runId}`,
    displayNameKey: 'title',
    access: { storefront: 'PUBLIC_READ' },
    capabilities: {
      publishable: { enabled: true },
      renderable: { enabled: true, data: { metaTitleKey: 'title' } },
      onlineStore: { enabled: true, data: { urlHandle: `template-suffix-${runId}` } },
    },
    fieldDefinitions: [
      { key: 'title', name: 'Title', type: 'single_line_text_field', required: true },
      { key: 'body', name: 'Body', type: 'single_line_text_field', required: false },
    ],
  };
}

function metaobjectInput(
  type: string,
  handle: string,
  title: string,
  body: string,
  templateSuffix?: string,
): Record<string, unknown> {
  const capabilities: Record<string, unknown> = {
    publishable: { status: 'ACTIVE' },
  };
  if (templateSuffix !== undefined) {
    capabilities['onlineStore'] = { templateSuffix };
  }
  return {
    type,
    handle,
    capabilities,
    fields: [
      { key: 'title', value: title },
      { key: 'body', value: body },
    ],
  };
}

function upsertInput(title: string, body: string, templateSuffix?: string): Record<string, unknown> {
  const input: Record<string, unknown> = {
    fields: [
      { key: 'title', value: title },
      { key: 'body', value: body },
    ],
  };
  if (templateSuffix !== undefined) {
    input['capabilities'] = { onlineStore: { templateSuffix } };
  }
  return input;
}

function updateBodyInput(body: string): Record<string, unknown> {
  return {
    fields: [{ key: 'body', value: body }],
  };
}

function updateTemplateSuffixInput(templateSuffix: string): Record<string, unknown> {
  return {
    capabilities: { onlineStore: { templateSuffix } },
  };
}

function readVariables(id: string, type: string, handle: string): Record<string, unknown> {
  return {
    id,
    handle: { type, handle },
  };
}

async function captureScenario(): Promise<ScenarioCaptures> {
  const type = `codex_template_suffix_${runId}`;

  const definitionCreate = await captureGraphql('definition-create', queries.definitionCreate, {
    definition: definitionInput(type),
  });
  assertNoUserErrors(
    definitionCreate.response,
    ['data', 'metaobjectDefinitionCreate', 'userErrors'],
    'definition create',
  );
  createdDefinitionIds.push(
    readStringPath(
      definitionCreate.response,
      ['data', 'metaobjectDefinitionCreate', 'metaobjectDefinition', 'id'],
      'definition create',
    ),
  );

  const createOmitted = await captureGraphql('create-omitted', queries.create, {
    metaobject: metaobjectInput(type, `omitted-${runId}`, 'Omitted', 'Omitted body'),
  });
  assertNoUserErrors(createOmitted.response, ['data', 'metaobjectCreate', 'userErrors'], 'create omitted');
  assertTemplateSuffix(
    createOmitted.response,
    ['data', 'metaobjectCreate', 'metaobject', 'capabilities', 'onlineStore', 'templateSuffix'],
    null,
    'create omitted',
  );
  createdMetaobjectIds.push(
    readStringPath(createOmitted.response, ['data', 'metaobjectCreate', 'metaobject', 'id'], 'create omitted'),
  );

  const createEmpty = await captureGraphql('create-empty', queries.create, {
    metaobject: metaobjectInput(type, `empty-${runId}`, 'Empty', 'Empty body', ''),
  });
  assertNoUserErrors(createEmpty.response, ['data', 'metaobjectCreate', 'userErrors'], 'create empty');
  assertTemplateSuffix(
    createEmpty.response,
    ['data', 'metaobjectCreate', 'metaobject', 'capabilities', 'onlineStore', 'templateSuffix'],
    '',
    'create empty',
  );
  createdMetaobjectIds.push(
    readStringPath(createEmpty.response, ['data', 'metaobjectCreate', 'metaobject', 'id'], 'create empty'),
  );

  const customHandle = `custom-${runId}`;
  const createCustom = await captureGraphql('create-custom', queries.create, {
    metaobject: metaobjectInput(type, customHandle, 'Custom', 'Original body', 'custom'),
  });
  assertNoUserErrors(createCustom.response, ['data', 'metaobjectCreate', 'userErrors'], 'create custom');
  assertTemplateSuffix(
    createCustom.response,
    ['data', 'metaobjectCreate', 'metaobject', 'capabilities', 'onlineStore', 'templateSuffix'],
    'custom',
    'create custom',
  );
  const customId = readStringPath(
    createCustom.response,
    ['data', 'metaobjectCreate', 'metaobject', 'id'],
    'create custom',
  );
  createdMetaobjectIds.push(customId);

  const readAfterCustomCreate = await captureGraphql(
    'read-after-custom-create',
    queries.read,
    readVariables(customId, type, customHandle),
  );
  assertTemplateSuffix(
    readAfterCustomCreate.response,
    ['data', 'detail', 'capabilities', 'onlineStore', 'templateSuffix'],
    'custom',
    'detail read after custom create',
  );
  assertTemplateSuffix(
    readAfterCustomCreate.response,
    ['data', 'byHandle', 'capabilities', 'onlineStore', 'templateSuffix'],
    'custom',
    'handle read after custom create',
  );

  const unrelatedUpdate = await captureGraphql('unrelated-update', queries.update, {
    id: customId,
    metaobject: updateBodyInput('Changed body'),
  });
  assertNoUserErrors(unrelatedUpdate.response, ['data', 'metaobjectUpdate', 'userErrors'], 'unrelated update');
  assertTemplateSuffix(
    unrelatedUpdate.response,
    ['data', 'metaobjectUpdate', 'metaobject', 'capabilities', 'onlineStore', 'templateSuffix'],
    'custom',
    'unrelated update',
  );

  const readAfterUnrelatedUpdate = await captureGraphql(
    'read-after-unrelated-update',
    queries.read,
    readVariables(customId, type, customHandle),
  );
  assertTemplateSuffix(
    readAfterUnrelatedUpdate.response,
    ['data', 'detail', 'capabilities', 'onlineStore', 'templateSuffix'],
    'custom',
    'detail read after unrelated update',
  );
  assertTemplateSuffix(
    readAfterUnrelatedUpdate.response,
    ['data', 'byHandle', 'capabilities', 'onlineStore', 'templateSuffix'],
    'custom',
    'handle read after unrelated update',
  );

  const explicitUpdate = await captureGraphql('explicit-update', queries.update, {
    id: customId,
    metaobject: updateTemplateSuffixInput('updated'),
  });
  assertNoUserErrors(explicitUpdate.response, ['data', 'metaobjectUpdate', 'userErrors'], 'explicit update');
  assertTemplateSuffix(
    explicitUpdate.response,
    ['data', 'metaobjectUpdate', 'metaobject', 'capabilities', 'onlineStore', 'templateSuffix'],
    'updated',
    'explicit update',
  );

  const upsertHandle = `upserted-${runId}`;
  const upsertCreate = await captureGraphql('upsert-create', queries.upsert, {
    handle: { type, handle: upsertHandle },
    metaobject: upsertInput('Upserted', 'Original upsert body', 'upserted'),
  });
  assertNoUserErrors(upsertCreate.response, ['data', 'metaobjectUpsert', 'userErrors'], 'upsert create');
  assertTemplateSuffix(
    upsertCreate.response,
    ['data', 'metaobjectUpsert', 'metaobject', 'capabilities', 'onlineStore', 'templateSuffix'],
    'upserted',
    'upsert create',
  );
  createdMetaobjectIds.push(
    readStringPath(upsertCreate.response, ['data', 'metaobjectUpsert', 'metaobject', 'id'], 'upsert create'),
  );

  const upsertUpdatePreserve = await captureGraphql('upsert-update-preserve', queries.upsert, {
    handle: { type, handle: upsertHandle },
    metaobject: upsertInput('Upserted', 'Changed upsert body'),
  });
  assertNoUserErrors(
    upsertUpdatePreserve.response,
    ['data', 'metaobjectUpsert', 'userErrors'],
    'upsert update preserve',
  );
  assertTemplateSuffix(
    upsertUpdatePreserve.response,
    ['data', 'metaobjectUpsert', 'metaobject', 'capabilities', 'onlineStore', 'templateSuffix'],
    'upserted',
    'upsert update preserve',
  );

  const upsertUpdateEmpty = await captureGraphql('upsert-update-empty', queries.upsert, {
    handle: { type, handle: upsertHandle },
    metaobject: upsertInput('Upserted', 'Changed upsert body', ''),
  });
  assertNoUserErrors(upsertUpdateEmpty.response, ['data', 'metaobjectUpsert', 'userErrors'], 'upsert update empty');
  assertTemplateSuffix(
    upsertUpdateEmpty.response,
    ['data', 'metaobjectUpsert', 'metaobject', 'capabilities', 'onlineStore', 'templateSuffix'],
    '',
    'upsert update empty',
  );

  return {
    type,
    definitionCreate,
    createOmitted,
    createEmpty,
    createCustom,
    readAfterCustomCreate,
    unrelatedUpdate,
    readAfterUnrelatedUpdate,
    explicitUpdate,
    upsertCreate,
    upsertUpdatePreserve,
    upsertUpdateEmpty,
  };
}

async function cleanup(): Promise<void> {
  for (const id of [...createdMetaobjectIds].reverse()) {
    try {
      await client.runGraphqlRequest(metaobjectDeleteMutation, { id });
    } catch (error) {
      console.warn(`Failed to cleanup metaobject ${id}:`, error);
    }
  }
  for (const id of [...createdDefinitionIds].reverse()) {
    try {
      await client.runGraphqlRequest(definitionDeleteMutation, { id });
    } catch (error) {
      console.warn(`Failed to cleanup metaobject definition ${id}:`, error);
    }
  }
}

try {
  const scenario = await captureScenario();

  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        apiVersion,
        storeDomain,
        capturedAt: new Date().toISOString(),
        scenarioId: 'metaobject-online-store-template-suffix',
        notes:
          'Captured live 2026-04 evidence that entry capabilities.onlineStore.templateSuffix is persisted for metaobjectCreate, preserved by unrelated metaobjectUpdate/metaobjectUpsert updates, and updated when supplied explicitly.',
        ...scenario,
        upstreamCalls: [],
      },
      null,
      2,
    )}\n`,
  );
  console.log(`Wrote ${outputPath}`);
} finally {
  await cleanup();
}

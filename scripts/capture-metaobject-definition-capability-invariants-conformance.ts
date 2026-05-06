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

type Seed = {
  runId: string;
  locale: string;
  definitions: Record<string, string | undefined>;
  metaobjects: Record<string, string | undefined>;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metaobjects');
const outputPath = path.join(outputDir, 'metaobjectDefinitionUpdate-capability-invariants.json');
const runId = Date.now().toString();
const seed: Seed = {
  runId,
  locale: 'fr',
  definitions: {},
  metaobjects: {},
};

const requestPaths = {
  definitionCreate:
    'config/parity-requests/metaobjects/metaobjectDefinitionUpdate-capability-invariants-definition-create.graphql',
  entryCreate:
    'config/parity-requests/metaobjects/metaobjectDefinitionUpdate-capability-invariants-entry-create.graphql',
  definitionUpdate:
    'config/parity-requests/metaobjects/metaobjectDefinitionUpdate-capability-invariants-update.graphql',
  definitionRead: 'config/parity-requests/metaobjects/metaobjectDefinitionUpdate-capability-invariants-read.graphql',
};

const queries = Object.fromEntries(
  await Promise.all(
    Object.entries(requestPaths).map(async ([name, requestPath]) => [name, await readFile(requestPath, 'utf8')]),
  ),
) as Record<keyof typeof requestPaths, string>;

const shopLocalesQuery = `#graphql
  query MetaobjectDefinitionUpdateCapabilityInvariantsShopLocales {
    shopLocales {
      locale
      primary
      published
    }
  }
`;

const shopLocaleEnableMutation = `#graphql
  mutation MetaobjectDefinitionUpdateCapabilityInvariantsShopLocaleEnable($locale: String!) {
    shopLocaleEnable(locale: $locale) {
      shopLocale {
        locale
        primary
        published
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const translatableResourceQuery = `#graphql
  query MetaobjectDefinitionUpdateCapabilityInvariantsTranslatableResource($resourceId: ID!) {
    translatableResource(resourceId: $resourceId) {
      resourceId
      translatableContent {
        key
        value
        digest
        locale
        type
      }
    }
  }
`;

const translationsRegisterMutation = `#graphql
  mutation MetaobjectDefinitionUpdateCapabilityInvariantsTranslationsRegister(
    $resourceId: ID!
    $translations: [TranslationInput!]!
  ) {
    translationsRegister(resourceId: $resourceId, translations: $translations) {
      translations {
        key
        value
        locale
        outdated
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const deleteMetaobjectMutation = `#graphql
  mutation MetaobjectDefinitionUpdateCapabilityInvariantsDeleteMetaobject($id: ID!) {
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

const deleteDefinitionMutation = `#graphql
  mutation MetaobjectDefinitionUpdateCapabilityInvariantsDeleteDefinition($id: ID!) {
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

function assertHasUserErrors(payload: unknown, parts: string[], label: string): void {
  const userErrors = readUserErrors(payload, parts);
  if (userErrors.length === 0) {
    throw new Error(`${label} did not return userErrors: ${JSON.stringify(payload, null, 2)}`);
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
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function captureGraphql(name: string, query: string, variables: Record<string, unknown>): Promise<Capture> {
  const result = await runGraphqlRaw(query, variables);
  assertGraphqlOk(result, name);
  return captureFromResult(name, query, variables, result);
}

function baseDefinition(type: string, capabilities: Record<string, unknown>): Record<string, unknown> {
  return {
    type,
    name: `Capability Invariants ${type}`,
    displayNameKey: 'title',
    access: { storefront: 'PUBLIC_READ' },
    capabilities,
    fieldDefinitions: [
      { key: 'title', name: 'Title', type: 'single_line_text_field', required: true },
      { key: 'count', name: 'Count', type: 'number_integer', required: false },
    ],
  };
}

async function createDefinition(name: string, definition: Record<string, unknown>): Promise<Capture> {
  const capture = await captureGraphql(name, queries.definitionCreate, { definition });
  assertNoUserErrors(capture.response, ['data', 'metaobjectDefinitionCreate', 'userErrors'], name);
  seed.definitions[name] = readStringPath(
    capture.response,
    ['data', 'metaobjectDefinitionCreate', 'metaobjectDefinition', 'id'],
    name,
  );
  return capture;
}

async function createMetaobject(name: string, metaobject: Record<string, unknown>): Promise<Capture> {
  const capture = await captureGraphql(name, queries.entryCreate, { metaobject });
  assertNoUserErrors(capture.response, ['data', 'metaobjectCreate', 'userErrors'], name);
  seed.metaobjects[name] = readStringPath(capture.response, ['data', 'metaobjectCreate', 'metaobject', 'id'], name);
  return capture;
}

async function ensureLocale(locale: string): Promise<Capture[]> {
  const captures: Capture[] = [];
  const before = await captureGraphql('shop-locales-before', shopLocalesQuery, {});
  captures.push(before);
  const locales = readPath(before.response, ['data', 'shopLocales']);
  const alreadyEnabled =
    Array.isArray(locales) && locales.some((entry) => isRecord(entry) && entry['locale'] === locale);
  if (!alreadyEnabled) {
    const enabled = await captureGraphql('shop-locale-enable', shopLocaleEnableMutation, { locale });
    assertNoUserErrors(enabled.response, ['data', 'shopLocaleEnable', 'userErrors'], 'shop-locale-enable');
    captures.push(enabled);
  }
  return captures;
}

function translatableDigest(payload: unknown): string {
  const content = readPath(payload, ['data', 'translatableResource', 'translatableContent']);
  if (!Array.isArray(content)) {
    throw new Error(`translatableResource did not return content: ${JSON.stringify(payload, null, 2)}`);
  }
  const title = content.find((entry) => isRecord(entry) && entry['key'] === 'title');
  if (!isRecord(title) || typeof title['digest'] !== 'string') {
    throw new Error(`translatableResource did not return a title digest: ${JSON.stringify(payload, null, 2)}`);
  }
  return title['digest'];
}

async function cleanup(cleanupCaptures: Capture[]): Promise<void> {
  for (const id of Object.values(seed.metaobjects).filter((value): value is string => typeof value === 'string')) {
    cleanupCaptures.push(await captureGraphql('cleanup-metaobject-delete', deleteMetaobjectMutation, { id }));
  }
  for (const id of Object.values(seed.definitions).filter((value): value is string => typeof value === 'string')) {
    cleanupCaptures.push(await captureGraphql('cleanup-definition-delete', deleteDefinitionMutation, { id }));
  }
}

async function writeBlocker(stage: string, error: unknown, captures: Capture[]): Promise<void> {
  await mkdir(outputDir, { recursive: true });
  const blockerPath = path.join(outputDir, `metaobjectDefinitionUpdate-capability-invariants-blocker-${runId}.json`);
  const message = error instanceof Error ? error.message : String(error);
  await writeFile(
    blockerPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        command: 'corepack pnpm tsx scripts/capture-metaobject-definition-capability-invariants-conformance.ts',
        blocker: { stage, message },
        partialCaptures: captures,
        seed,
      },
      null,
      2,
    )}\n`,
  );
  console.error(`Wrote blocker details to ${blockerPath}`);
}

const captures: Capture[] = [];
const cleanupCaptures: Capture[] = [];

try {
  captures.push(...(await ensureLocale(seed.locale)));

  const publishableType = `capability_publishable_${runId}`;
  const publishableDefinition = await createDefinition(
    'publishable-definition-create',
    baseDefinition(publishableType, { publishable: { enabled: true } }),
  );
  captures.push(publishableDefinition);
  const publishableEntry = await createMetaobject('publishable-draft-entry-create', {
    type: publishableType,
    handle: `publishable-draft-${runId}`,
    fields: [{ key: 'title', value: 'Draft row' }],
  });
  captures.push(publishableEntry);
  const publishableDisable = await captureGraphql('publishable-disable-public-allowed', queries.definitionUpdate, {
    id: seed.definitions['publishable-definition-create'],
    definition: { capabilities: { publishable: { enabled: false } } },
  });
  captures.push(publishableDisable);
  captures.push(
    await captureGraphql('publishable-read-after-update', queries.definitionRead, {
      id: seed.definitions['publishable-definition-create'],
    }),
  );

  const onlineStoreType = `capability_online_store_${runId}`;
  const onlineStoreDefinition = await createDefinition(
    'online-store-definition-create',
    baseDefinition(onlineStoreType, {
      publishable: { enabled: true },
      onlineStore: { enabled: true, data: { urlHandle: 'title' } },
    }),
  );
  captures.push(onlineStoreDefinition);
  const onlineStoreEntry = await createMetaobject('online-store-active-entry-create', {
    type: onlineStoreType,
    handle: `online-store-active-${runId}`,
    capabilities: { publishable: { status: 'ACTIVE' }, onlineStore: { templateSuffix: '' } },
    fields: [{ key: 'title', value: 'Published row' }],
  });
  captures.push(onlineStoreEntry);
  const onlineStoreDisable = await captureGraphql('online-store-disable-public-allowed', queries.definitionUpdate, {
    id: seed.definitions['online-store-definition-create'],
    definition: { capabilities: { onlineStore: { enabled: false } } },
  });
  captures.push(onlineStoreDisable);

  const renderableType = `capability_renderable_${runId}`;
  const renderableDefinition = await createDefinition(
    'renderable-definition-create',
    baseDefinition(renderableType, {
      renderable: { enabled: true, data: { metaTitleKey: 'title', metaDescriptionKey: 'title' } },
    }),
  );
  captures.push(renderableDefinition);
  captures.push(
    await createMetaobject('renderable-entry-create', {
      type: renderableType,
      handle: `renderable-${runId}`,
      fields: [{ key: 'title', value: 'Renderable row' }],
    }),
  );
  const renderableDisable = await captureGraphql('renderable-disable-public-allowed', queries.definitionUpdate, {
    id: seed.definitions['renderable-definition-create'],
    definition: { capabilities: { renderable: { enabled: false } } },
  });
  captures.push(renderableDisable);

  const translatableType = `capability_translatable_${runId}`;
  const translatableDefinition = await createDefinition(
    'translatable-definition-create',
    baseDefinition(translatableType, { translatable: { enabled: true } }),
  );
  captures.push(translatableDefinition);
  const translatableEntry = await createMetaobject('translatable-entry-create', {
    type: translatableType,
    handle: `translatable-${runId}`,
    fields: [{ key: 'title', value: 'Translatable row' }],
  });
  captures.push(translatableEntry);
  const translatableResource = await captureGraphql('translatable-resource-read', translatableResourceQuery, {
    resourceId: seed.metaobjects['translatable-entry-create'],
  });
  captures.push(translatableResource);
  const digest = translatableDigest(translatableResource.response);
  const translationRegister = await captureGraphql('translation-register', translationsRegisterMutation, {
    resourceId: seed.metaobjects['translatable-entry-create'],
    translations: [{ key: 'title', locale: seed.locale, value: 'Ligne traduite', translatableContentDigest: digest }],
  });
  assertNoUserErrors(
    translationRegister.response,
    ['data', 'translationsRegister', 'userErrors'],
    translationRegister.name,
  );
  captures.push(translationRegister);
  const translatableDisable = await captureGraphql('translatable-disable-public-allowed', queries.definitionUpdate, {
    id: seed.definitions['translatable-definition-create'],
    definition: { capabilities: { translatable: { enabled: false } } },
  });
  captures.push(translatableDisable);

  const renderableMissingType = `capability_render_missing_${runId}`;
  captures.push(
    await createDefinition('renderable-missing-definition-create', baseDefinition(renderableMissingType, {})),
  );
  const renderableMissing = await captureGraphql('renderable-enable-missing-field-rejected', queries.definitionUpdate, {
    id: seed.definitions['renderable-missing-definition-create'],
    definition: { capabilities: { renderable: { enabled: true, data: { metaTitleKey: 'missing' } } } },
  });
  assertHasUserErrors(
    renderableMissing.response,
    ['data', 'metaobjectDefinitionUpdate', 'userErrors'],
    renderableMissing.name,
  );
  captures.push(renderableMissing);

  const renderableTypeInvalidType = `capability_render_type_${runId}`;
  captures.push(
    await createDefinition('renderable-type-definition-create', baseDefinition(renderableTypeInvalidType, {})),
  );
  const renderableTypeInvalid = await captureGraphql('renderable-enable-type-rejected', queries.definitionUpdate, {
    id: seed.definitions['renderable-type-definition-create'],
    definition: { capabilities: { renderable: { enabled: true, data: { metaDescriptionKey: 'count' } } } },
  });
  assertHasUserErrors(
    renderableTypeInvalid.response,
    ['data', 'metaobjectDefinitionUpdate', 'userErrors'],
    renderableTypeInvalid.name,
  );
  captures.push(renderableTypeInvalid);

  await cleanup(cleanupCaptures);

  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        summary: 'MetaobjectDefinitionUpdate public capability-disable behavior and renderable enable data validation.',
        seed,
        captures: Object.fromEntries(captures.map((capture) => [capture.name, capture])),
        cleanup: cleanupCaptures,
        upstreamCalls: [],
      },
      null,
      2,
    )}\n`,
  );
  console.log(`Wrote ${outputPath}`);
} catch (error) {
  try {
    await cleanup(cleanupCaptures);
  } catch (cleanupError) {
    console.error(`Cleanup failed after capture error: ${String(cleanupError)}`);
  }
  await writeBlocker('capture', error, captures);
  throw error;
}

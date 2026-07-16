/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

import { createAdminGraphqlClient, runStorefrontGraphqlRequest } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import {
  buildAdminAuthHeaders,
  buildStorefrontRequestHeaders,
  getStoredStorefrontAccessToken,
  getValidConformanceAccessToken,
} from './shopify-conformance-auth.mjs';

type Capture = {
  name: string;
  request: {
    query: string;
    variables: Record<string, unknown>;
  };
  status: number;
  response: unknown;
};

type StorefrontCapture = {
  name: string;
  method: 'POST';
  apiSurface: 'storefront';
  apiVersion: string;
  path: string;
  endpoint: string;
  authMode: 'storefront-access-token';
  headers: Record<string, string>;
  operationName: string;
  query: string;
  variables: Record<string, unknown>;
  response: {
    status: number;
    body: unknown;
  };
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const storedStorefrontAuth = await getStoredStorefrontAccessToken();
if (storedStorefrontAuth.shop && storedStorefrontAuth.shop !== storeDomain) {
  throw new Error(
    `Stored Storefront token is for ${storedStorefrontAuth.shop}, but SHOPIFY_CONFORMANCE_STORE_DOMAIN is ${storeDomain}. ` +
      'Run `corepack pnpm conformance:grant-storefront-token` for the target store.',
  );
}

const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const storefrontOptions = {
  storeOrigin: `https://${storeDomain}`,
  apiVersion,
  storefrontAccessToken: storedStorefrontAuth.storefront_access_token,
};
const storefrontEndpoint = `https://${storeDomain}/api/${apiVersion}/graphql.json`;
const storefrontPath = `/api/${apiVersion}/graphql.json`;
const storefrontRedactedHeaders = Object.fromEntries(
  Object.keys(buildStorefrontRequestHeaders(storedStorefrontAuth.storefront_access_token)).map((name) => [
    name,
    '<redacted:storefront-access-token>',
  ]),
);

const documentPaths = {
  primarySetup: 'config/parity-requests/storefront/storefront-custom-data-primary-setup-admin.graphql',
  targetEntriesSetup: 'config/parity-requests/storefront/storefront-custom-data-target-entries-setup-admin.graphql',
  sourceDefinitionSetup:
    'config/parity-requests/storefront/storefront-custom-data-source-definition-setup-admin.graphql',
  sourceEntrySetup: 'config/parity-requests/storefront/storefront-custom-data-source-entry-setup-admin.graphql',
  shopMetafieldsSet: 'config/parity-requests/storefront/storefront-custom-data-shop-metafields-set-admin.graphql',
  storefrontRead: 'config/parity-requests/storefront/storefront-custom-data-read-after-admin-setup.graphql',
};

const documents = {
  primarySetup: await readFile(documentPaths.primarySetup, 'utf8'),
  targetEntriesSetup: await readFile(documentPaths.targetEntriesSetup, 'utf8'),
  sourceDefinitionSetup: await readFile(documentPaths.sourceDefinitionSetup, 'utf8'),
  sourceEntrySetup: await readFile(documentPaths.sourceEntrySetup, 'utf8'),
  shopMetafieldsSet: await readFile(documentPaths.shopMetafieldsSet, 'utf8'),
  storefrontRead: await readFile(documentPaths.storefrontRead, 'utf8'),
};

const suffix = new Date().toISOString().replace(/\D/gu, '').slice(0, 14);
const targetType = `codex_sfc_target_${suffix}`;
const sourceType = `codex_sfc_source_${suffix}`;
const privateType = `codex_sfc_private_${suffix}`;
const visibleHandle = `visible-target-${suffix}`;
const draftHandle = `draft-target-${suffix}`;
const sourceHandle = `source-entry-${suffix}`;
const privateHandle = `private-entry-${suffix}`;
const metafieldNamespace = `sfc_${suffix}`;

const adminCaptures: Capture[] = [];
const cleanupCaptures: Capture[] = [];
const createdMetaobjectIds: string[] = [];
const createdMetaobjectDefinitionIds: string[] = [];
const createdMetafieldDefinitionIds: string[] = [];
let adminShopCapture: Capture | null = null;
let primarySetupCapture: Capture | null = null;
let targetEntriesCapture: Capture | null = null;
let sourceDefinitionCapture: Capture | null = null;
let sourceEntryCapture: Capture | null = null;
let shopMetafieldsSetCapture: Capture | null = null;
let storefrontReadCapture: StorefrontCapture | null = null;

async function captureAdmin(name: string, query: string, variables: Record<string, unknown>): Promise<Capture> {
  const result = await runGraphqlRaw(query, variables);
  const capture = {
    name,
    request: { query, variables },
    status: result.status,
    response: result.payload,
  };
  adminCaptures.push(capture);
  return capture;
}

async function captureAdminCleanup(name: string, query: string, variables: Record<string, unknown>): Promise<void> {
  const result = await runGraphqlRaw(query, variables);
  cleanupCaptures.push({
    name,
    request: { query, variables },
    status: result.status,
    response: result.payload,
  });
}

async function storefrontRequest(
  name: string,
  operationName: string,
  query: string,
  variables: Record<string, unknown>,
): Promise<StorefrontCapture> {
  const result = await runStorefrontGraphqlRequest(storefrontOptions, query, variables);
  return {
    name,
    method: 'POST',
    apiSurface: 'storefront',
    apiVersion,
    path: storefrontPath,
    endpoint: storefrontEndpoint,
    authMode: 'storefront-access-token',
    headers: storefrontRedactedHeaders,
    operationName,
    query,
    variables,
    response: {
      status: result.status,
      body: result.payload,
    },
  };
}

function readPath(value: unknown, pathSegments: string[]): unknown {
  return pathSegments.reduce<unknown>((current, segment) => {
    if (typeof current !== 'object' || current === null) return null;
    return (current as Record<string, unknown>)[segment] ?? null;
  }, value);
}

function readRequiredString(value: unknown, pathSegments: string[], label: string): string {
  const result = readPath(value, pathSegments);
  if (typeof result !== 'string' || result.length === 0) {
    throw new Error(`${label} did not return a string at ${pathSegments.join('.')}: ${JSON.stringify(value)}`);
  }
  return result;
}

function assertNoTopLevelErrors(payload: unknown, label: string): void {
  const errors = readPath(payload, ['errors']);
  if (Array.isArray(errors) && errors.length > 0) {
    throw new Error(`${label} returned top-level errors: ${JSON.stringify(errors, null, 2)}`);
  }
}

function assertNoUserErrors(payload: unknown, pathSegments: string[], label: string): void {
  const userErrors = readPath(payload, pathSegments);
  if (Array.isArray(userErrors) && userErrors.length === 0) return;
  throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
}

function rememberString(target: string[], value: unknown): void {
  if (typeof value === 'string' && value.length > 0) target.push(value);
}

function storefrontCustomDataVisible(payload: unknown): boolean {
  return (
    readPath(payload, ['data', 'byHandle', 'handle']) === visibleHandle &&
    readPath(payload, ['data', 'entries', 'nodes', '0', 'handle']) === visibleHandle &&
    readPath(payload, ['data', 'source', 'featured', 'reference', 'handle']) === visibleHandle &&
    readPath(payload, ['data', 'source', 'draftFeatured', 'reference']) === null &&
    readPath(payload, ['data', 'source', 'related', 'references', 'nodes', '0', 'handle']) === visibleHandle &&
    readPath(payload, ['data', 'draft']) === null &&
    readPath(payload, ['data', 'privateEntry']) === null &&
    readPath(payload, ['data', 'shop', 'visible', 'value']) === `Visible tagline ${suffix}` &&
    readPath(payload, ['data', 'shop', 'hidden']) === null
  );
}

async function waitForStorefrontCustomData(variables: Record<string, unknown>): Promise<StorefrontCapture> {
  let lastCapture: StorefrontCapture | null = null;
  for (let attempt = 1; attempt <= 30; attempt += 1) {
    lastCapture = await storefrontRequest(
      'storefront-custom-data-read-after-admin-setup',
      'StorefrontCustomDataReadAfterAdminSetup',
      documents.storefrontRead,
      variables,
    );
    assertNoTopLevelErrors(lastCapture.response.body, `storefront custom-data read attempt ${attempt}`);
    if (storefrontCustomDataVisible(lastCapture.response.body)) return lastCapture;
    await delay(2000);
  }
  throw new Error(
    `Storefront custom data did not become visible after polling: ${JSON.stringify(
      lastCapture?.response.body,
      null,
      2,
    )}`,
  );
}

const adminShopQuery = `#graphql
  query StorefrontCustomDataAdminShop {
    shop {
      id
      name
    }
  }
`;

const metaobjectDeleteMutation = `#graphql
  mutation StorefrontCustomDataMetaobjectCleanup($id: ID!) {
    metaobjectDelete(id: $id) {
      deletedId
      userErrors { field message code elementKey elementIndex }
    }
  }
`;

const metaobjectDefinitionDeleteMutation = `#graphql
  mutation StorefrontCustomDataMetaobjectDefinitionCleanup($id: ID!) {
    metaobjectDefinitionDelete(id: $id) {
      deletedId
      userErrors { field message code elementKey elementIndex }
    }
  }
`;

const metafieldDefinitionDeleteMutation = `#graphql
  mutation StorefrontCustomDataMetafieldDefinitionCleanup($id: ID!) {
    metafieldDefinitionDelete(id: $id, deleteAllAssociatedMetafields: true) {
      deletedDefinitionId
      userErrors { field message code }
    }
  }
`;

try {
  adminShopCapture = await captureAdmin('admin-shop', adminShopQuery, {});
  assertNoTopLevelErrors(adminShopCapture.response, 'admin shop');
  const shopId = readRequiredString(adminShopCapture.response, ['data', 'shop', 'id'], 'admin shop id');

  primarySetupCapture = await captureAdmin('admin-primary-setup', documents.primarySetup, {
    targetDefinition: {
      type: targetType,
      name: `Storefront custom data target ${suffix}`,
      access: { storefront: 'PUBLIC_READ' },
      capabilities: { publishable: { enabled: true } },
      displayNameKey: 'title',
      fieldDefinitions: [
        {
          key: 'title',
          name: 'Title',
          type: 'single_line_text_field',
          required: true,
        },
        {
          key: 'body',
          name: 'Body',
          type: 'multi_line_text_field',
          required: false,
        },
      ],
    },
    privateDefinition: {
      type: privateType,
      name: `Storefront custom data private ${suffix}`,
      access: { storefront: 'NONE' },
      capabilities: { publishable: { enabled: true } },
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
    visibleMetafieldDefinition: {
      ownerType: 'SHOP',
      namespace: metafieldNamespace,
      key: 'visible',
      name: 'Visible Storefront tagline',
      description: 'Visible Storefront custom data tagline.',
      type: 'single_line_text_field',
      access: { storefront: 'PUBLIC_READ' },
    },
    hiddenMetafieldDefinition: {
      ownerType: 'SHOP',
      namespace: metafieldNamespace,
      key: 'hidden',
      name: 'Hidden Storefront tagline',
      description: 'Hidden Storefront custom data tagline.',
      type: 'single_line_text_field',
      access: { storefront: 'NONE' },
    },
  });
  assertNoTopLevelErrors(primarySetupCapture.response, 'admin primary setup');
  assertNoUserErrors(primarySetupCapture.response, ['data', 'targetDefinition', 'userErrors'], 'target definition');
  assertNoUserErrors(primarySetupCapture.response, ['data', 'privateDefinition', 'userErrors'], 'private definition');
  assertNoUserErrors(
    primarySetupCapture.response,
    ['data', 'visibleMetafieldDefinition', 'userErrors'],
    'visible metafield definition',
  );
  assertNoUserErrors(
    primarySetupCapture.response,
    ['data', 'hiddenMetafieldDefinition', 'userErrors'],
    'hidden metafield definition',
  );

  const targetDefinitionId = readRequiredString(
    primarySetupCapture.response,
    ['data', 'targetDefinition', 'metaobjectDefinition', 'id'],
    'target definition id',
  );
  rememberString(createdMetaobjectDefinitionIds, targetDefinitionId);
  rememberString(
    createdMetaobjectDefinitionIds,
    readPath(primarySetupCapture.response, ['data', 'privateDefinition', 'metaobjectDefinition', 'id']),
  );
  rememberString(
    createdMetafieldDefinitionIds,
    readPath(primarySetupCapture.response, ['data', 'visibleMetafieldDefinition', 'createdDefinition', 'id']),
  );
  rememberString(
    createdMetafieldDefinitionIds,
    readPath(primarySetupCapture.response, ['data', 'hiddenMetafieldDefinition', 'createdDefinition', 'id']),
  );

  targetEntriesCapture = await captureAdmin('admin-target-entries-setup', documents.targetEntriesSetup, {
    visibleTarget: {
      type: targetType,
      handle: visibleHandle,
      capabilities: { publishable: { status: 'ACTIVE' } },
      fields: [
        { key: 'title', value: `Visible Storefront Entry ${suffix}` },
        { key: 'body', value: `Body for visible Storefront Entry ${suffix}` },
      ],
    },
    draftTarget: {
      type: targetType,
      handle: draftHandle,
      capabilities: { publishable: { status: 'DRAFT' } },
      fields: [
        { key: 'title', value: `Draft Storefront Entry ${suffix}` },
        { key: 'body', value: `Body for draft Storefront Entry ${suffix}` },
      ],
    },
    privateEntry: {
      type: privateType,
      handle: privateHandle,
      capabilities: { publishable: { status: 'ACTIVE' } },
      fields: [{ key: 'title', value: `Private Storefront Entry ${suffix}` }],
    },
  });
  assertNoTopLevelErrors(targetEntriesCapture.response, 'admin target entries setup');
  assertNoUserErrors(targetEntriesCapture.response, ['data', 'visibleTarget', 'userErrors'], 'visible target');
  assertNoUserErrors(targetEntriesCapture.response, ['data', 'draftTarget', 'userErrors'], 'draft target');
  assertNoUserErrors(targetEntriesCapture.response, ['data', 'privateEntry', 'userErrors'], 'private entry');
  const visibleTargetId = readRequiredString(
    targetEntriesCapture.response,
    ['data', 'visibleTarget', 'metaobject', 'id'],
    'visible target id',
  );
  const draftTargetId = readRequiredString(
    targetEntriesCapture.response,
    ['data', 'draftTarget', 'metaobject', 'id'],
    'draft target id',
  );
  rememberString(createdMetaobjectIds, visibleTargetId);
  rememberString(createdMetaobjectIds, draftTargetId);
  rememberString(
    createdMetaobjectIds,
    readPath(targetEntriesCapture.response, ['data', 'privateEntry', 'metaobject', 'id']),
  );

  sourceDefinitionCapture = await captureAdmin('admin-source-definition-setup', documents.sourceDefinitionSetup, {
    sourceDefinition: {
      type: sourceType,
      name: `Storefront custom data source ${suffix}`,
      access: { storefront: 'PUBLIC_READ' },
      capabilities: { publishable: { enabled: true } },
      displayNameKey: 'title',
      fieldDefinitions: [
        {
          key: 'title',
          name: 'Title',
          type: 'single_line_text_field',
          required: true,
        },
        {
          key: 'featured',
          name: 'Featured',
          type: 'metaobject_reference',
          required: false,
          validations: [{ name: 'metaobject_definition_id', value: targetDefinitionId }],
        },
        {
          key: 'draft_featured',
          name: 'Draft featured',
          type: 'metaobject_reference',
          required: false,
          validations: [{ name: 'metaobject_definition_id', value: targetDefinitionId }],
        },
        {
          key: 'related',
          name: 'Related',
          type: 'list.metaobject_reference',
          required: false,
          validations: [{ name: 'metaobject_definition_id', value: targetDefinitionId }],
        },
      ],
    },
  });
  assertNoTopLevelErrors(sourceDefinitionCapture.response, 'admin source definition setup');
  assertNoUserErrors(sourceDefinitionCapture.response, ['data', 'sourceDefinition', 'userErrors'], 'source definition');
  rememberString(
    createdMetaobjectDefinitionIds,
    readPath(sourceDefinitionCapture.response, ['data', 'sourceDefinition', 'metaobjectDefinition', 'id']),
  );

  sourceEntryCapture = await captureAdmin('admin-source-entry-setup', documents.sourceEntrySetup, {
    sourceEntry: {
      type: sourceType,
      handle: sourceHandle,
      capabilities: { publishable: { status: 'ACTIVE' } },
      fields: [
        { key: 'title', value: `Source Storefront Entry ${suffix}` },
        { key: 'featured', value: visibleTargetId },
        { key: 'draft_featured', value: draftTargetId },
        { key: 'related', value: JSON.stringify([visibleTargetId]) },
      ],
    },
  });
  assertNoTopLevelErrors(sourceEntryCapture.response, 'admin source entry setup');
  assertNoUserErrors(sourceEntryCapture.response, ['data', 'sourceEntry', 'userErrors'], 'source entry');
  rememberString(
    createdMetaobjectIds,
    readPath(sourceEntryCapture.response, ['data', 'sourceEntry', 'metaobject', 'id']),
  );

  shopMetafieldsSetCapture = await captureAdmin('admin-shop-metafields-set', documents.shopMetafieldsSet, {
    metafields: [
      {
        ownerId: shopId,
        namespace: metafieldNamespace,
        key: 'visible',
        type: 'single_line_text_field',
        value: `Visible tagline ${suffix}`,
      },
      {
        ownerId: shopId,
        namespace: metafieldNamespace,
        key: 'hidden',
        type: 'single_line_text_field',
        value: `Hidden tagline ${suffix}`,
      },
    ],
  });
  assertNoTopLevelErrors(shopMetafieldsSetCapture.response, 'admin shop metafields set');
  assertNoUserErrors(shopMetafieldsSetCapture.response, ['data', 'metafieldsSet', 'userErrors'], 'shop metafieldsSet');

  const storefrontVariables = {
    targetHandle: { type: targetType, handle: visibleHandle },
    sourceHandle: { type: sourceType, handle: sourceHandle },
    draftHandle: { type: targetType, handle: draftHandle },
    privateHandle: { type: privateType, handle: privateHandle },
    targetType,
    metafieldNamespace,
  };
  storefrontReadCapture = await waitForStorefrontCustomData(storefrontVariables);
} finally {
  for (const id of createdMetaobjectIds.reverse()) {
    await captureAdminCleanup('metaobjectDelete-cleanup', metaobjectDeleteMutation, { id });
  }
  for (const id of createdMetafieldDefinitionIds.reverse()) {
    await captureAdminCleanup('metafieldDefinitionDelete-cleanup', metafieldDefinitionDeleteMutation, { id });
  }
  for (const id of createdMetaobjectDefinitionIds.reverse()) {
    await captureAdminCleanup('metaobjectDefinitionDelete-cleanup', metaobjectDefinitionDeleteMutation, { id });
  }
}

if (
  adminShopCapture === null ||
  primarySetupCapture === null ||
  targetEntriesCapture === null ||
  sourceDefinitionCapture === null ||
  sourceEntryCapture === null ||
  shopMetafieldsSetCapture === null ||
  storefrontReadCapture === null
) {
  throw new Error('Storefront custom-data capture did not complete.');
}

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'storefront');
await mkdir(outputDir, { recursive: true });
const outputPath = path.join(outputDir, 'storefront-custom-data-read-after-admin-setup.json');
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      scenarioId: 'storefront-custom-data-read-after-admin-setup',
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      apiSurface: 'storefront',
      endpoint: storefrontEndpoint,
      authMode: 'storefront-access-token',
      storefrontToken: {
        id: storedStorefrontAuth.storefront_token_id || '<unknown>',
        title: storedStorefrontAuth.storefront_token_title || '<unknown>',
        accessScopes: storedStorefrontAuth.storefront_access_scopes,
        obtainedAt: storedStorefrontAuth.obtained_at || '<unknown>',
      },
      adminShop: adminShopCapture,
      primarySetup: primarySetupCapture,
      targetEntriesSetup: targetEntriesCapture,
      sourceDefinitionSetup: sourceDefinitionCapture,
      sourceEntrySetup: sourceEntryCapture,
      shopMetafieldsSet: shopMetafieldsSetCapture,
      storefrontRead: storefrontReadCapture,
      cleanup: cleanupCaptures,
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(`Wrote ${outputPath}`);
console.log(`Captured authenticated Storefront custom-data status ${storefrontReadCapture.response.status}`);

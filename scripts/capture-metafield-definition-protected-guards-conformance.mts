/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type CapturedInteraction = {
  request: {
    documentPath?: string;
    query?: string;
    variables: Record<string, unknown>;
  };
  status: number;
  response: unknown;
};

const requestPaths = {
  standardEnable: 'config/parity-requests/metafields/metafield-definition-protected-guards-standard-enable.graphql',
  standardUpdate: 'config/parity-requests/metafields/metafield-definition-protected-guards-standard-update.graphql',
  read: 'config/parity-requests/metafields/metafield-definition-protected-guards-read.graphql',
  appCreate: 'config/parity-requests/metafields/metafield-definition-app-namespace-create.graphql',
  metafieldsSet:
    'config/parity-requests/metafields/metafield-definition-update-delete-preconditions-metafields-set.graphql',
  deleteNoFlag: 'config/parity-requests/metafields/metafield-definition-protected-guards-delete-no-flag.graphql',
  deleteWithFlag: 'config/parity-requests/metafields/metafield-definition-protected-guards-delete-with-flag.graphql',
};

const productCreateMutation = `#graphql
  mutation MetafieldDefinitionProtectedGuardsProductCreate($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product { id title }
      userErrors { field message }
    }
  }
`;

const productDeleteMutation = `#graphql
  mutation MetafieldDefinitionProtectedGuardsProductDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors { field message }
    }
  }
`;

const definitionDeleteByIdMutation = `#graphql
  mutation MetafieldDefinitionProtectedGuardsCleanupDefinition($id: ID!) {
    metafieldDefinitionDelete(id: $id, deleteAllAssociatedMetafields: true) {
      deletedDefinitionId
      userErrors { field message code }
    }
  }
`;

function readObject(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as Record<string, unknown>) : null;
}

function readPath(value: unknown, pathParts: string[]): unknown {
  let current = value;
  for (const part of pathParts) {
    const object = readObject(current);
    if (object === null) return undefined;
    current = object[part];
  }
  return current;
}

function readArrayPath(value: unknown, pathParts: string[]): unknown[] {
  const found = readPath(value, pathParts);
  return Array.isArray(found) ? found : [];
}

function readStringPath(value: unknown, pathParts: string[], label: string): string {
  const found = readPath(value, pathParts);
  if (typeof found !== 'string' || found.length === 0) {
    throw new Error(`${label} did not return a string at ${pathParts.join('.')}: ${JSON.stringify(value, null, 2)}`);
  }
  return found;
}

function assertHttpGraphqlOk(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || readPath(result.payload, ['errors']) !== undefined) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertNoUserErrors(payload: unknown, pathParts: string[], label: string): void {
  const userErrors = readArrayPath(payload, pathParts);
  if (userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function assertUserErrorCode(payload: unknown, pathParts: string[], code: string, label: string): void {
  const userErrors = readArrayPath(payload, pathParts);
  if (!userErrors.some((error) => readObject(error)?.['code'] === code)) {
    throw new Error(`${label} did not return ${code}: ${JSON.stringify(payload, null, 2)}`);
  }
}

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metafields');
const outputPath = path.join(outputDir, 'metafield-definition-protected-guards.json');
const requestingApiClientId = process.env.SHOPIFY_CONFORMANCE_API_CLIENT_ID ?? '347082227713';
const runId = Date.now().toString(36);
const appNamespaceSuffix = `protected_guards_${runId}`;
const appNamespaceInput = `$app:${appNamespaceSuffix}`;
const appNamespaceResolved = `app--${requestingApiClientId}--${appNamespaceSuffix}`;
const standardIdentifier = { ownerType: 'PRODUCT', namespace: 'descriptors', key: 'subtitle' };

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function captureDocument(
  label: string,
  documentPath: string,
  variables: Record<string, unknown>,
): Promise<CapturedInteraction> {
  const query = await readFile(documentPath, 'utf8');
  const result = await runGraphqlRaw(query, variables);
  assertHttpGraphqlOk(result, label);
  return { request: { documentPath, variables }, status: result.status, response: result.payload };
}

async function captureQuery(
  label: string,
  query: string,
  variables: Record<string, unknown>,
): Promise<CapturedInteraction> {
  const result = await runGraphqlRaw(query, variables);
  assertHttpGraphqlOk(result, label);
  return { request: { query, variables }, status: result.status, response: result.payload };
}

async function cleanupDefinition(id: string): Promise<CapturedInteraction> {
  return captureQuery('cleanup metafieldDefinitionDelete', definitionDeleteByIdMutation, { id }).catch(
    (error: unknown) => ({
      request: { query: definitionDeleteByIdMutation, variables: { id } },
      status: 0,
      response: { error: String(error) },
    }),
  );
}

async function cleanupProduct(id: string): Promise<CapturedInteraction> {
  return captureQuery('cleanup productDelete', productDeleteMutation, { input: { id } }).catch((error: unknown) => ({
    request: { query: productDeleteMutation, variables: { input: { id } } },
    status: 0,
    response: { error: String(error) },
  }));
}

const cleanup: CapturedInteraction[] = [];
let standardDefinitionId: string | undefined;
let standardCreatedByCapture = false;
let appDefinitionId: string | undefined;
let productId: string | undefined;
let standardEnable: CapturedInteraction | undefined;
let standardImmutableUpdate: CapturedInteraction | undefined;
let standardReadAfterRejectedUpdate: CapturedInteraction | undefined;
let productCreate: CapturedInteraction | undefined;
let appDefinitionCreate: CapturedInteraction | undefined;
let appMetafieldsSet: CapturedInteraction | undefined;
let appReservedDeleteNoFlag: CapturedInteraction | undefined;
let appDefinitionReadAfterGuard: CapturedInteraction | undefined;
let appReservedDeleteWithFlag: CapturedInteraction | undefined;

const standardBeforeRead = await captureDocument('standard definition pre-read', requestPaths.read, {
  identifier: standardIdentifier,
});

try {
  standardCreatedByCapture = readPath(standardBeforeRead.response, ['data', 'metafieldDefinition']) === null;
  standardEnable = await captureDocument('standardMetafieldDefinitionEnable', requestPaths.standardEnable, {
    ownerType: 'PRODUCT',
    id: 'gid://shopify/StandardMetafieldDefinitionTemplate/1',
  });
  assertNoUserErrors(
    standardEnable.response,
    ['data', 'standardMetafieldDefinitionEnable', 'userErrors'],
    'standardMetafieldDefinitionEnable',
  );
  standardDefinitionId = readStringPath(
    standardEnable.response,
    ['data', 'standardMetafieldDefinitionEnable', 'createdDefinition', 'id'],
    'standardMetafieldDefinitionEnable',
  );

  standardImmutableUpdate = await captureDocument(
    'standard template immutable metafieldDefinitionUpdate',
    requestPaths.standardUpdate,
    {
      definition: {
        ownerType: 'PRODUCT',
        namespace: 'descriptors',
        key: 'subtitle',
        name: `Renamed product subtitle ${runId}`,
        description: `Changed standard description ${runId}`,
        validations: [{ name: 'max', value: '80' }],
      },
    },
  );
  assertUserErrorCode(
    standardImmutableUpdate.response,
    ['data', 'metafieldDefinitionUpdate', 'userErrors'],
    'INVALID_INPUT',
    'standard template immutable metafieldDefinitionUpdate',
  );

  standardReadAfterRejectedUpdate = await captureDocument(
    'standard definition read after rejected update',
    requestPaths.read,
    { identifier: standardIdentifier },
  );

  productCreate = await captureQuery('productCreate', productCreateMutation, {
    product: { title: `Metafield definition protected guards ${runId}` },
  });
  assertNoUserErrors(productCreate.response, ['data', 'productCreate', 'userErrors'], 'productCreate');
  productId = readStringPath(productCreate.response, ['data', 'productCreate', 'product', 'id'], 'productCreate');

  appDefinitionCreate = await captureDocument('app-reserved metafieldDefinitionCreate', requestPaths.appCreate, {
    definition: {
      ownerType: 'PRODUCT',
      namespace: appNamespaceInput,
      key: 'config',
      name: `Protected config ${runId}`,
      type: 'single_line_text_field',
    },
  });
  assertNoUserErrors(
    appDefinitionCreate.response,
    ['data', 'metafieldDefinitionCreate', 'userErrors'],
    'app-reserved metafieldDefinitionCreate',
  );
  appDefinitionId = readStringPath(
    appDefinitionCreate.response,
    ['data', 'metafieldDefinitionCreate', 'createdDefinition', 'id'],
    'app-reserved metafieldDefinitionCreate',
  );

  appMetafieldsSet = await captureDocument('app-reserved metafieldsSet', requestPaths.metafieldsSet, {
    metafields: [
      {
        ownerId: productId,
        namespace: appNamespaceInput,
        key: 'config',
        type: 'single_line_text_field',
        value: 'enabled',
      },
    ],
  });
  assertNoUserErrors(appMetafieldsSet.response, ['data', 'metafieldsSet', 'userErrors'], 'app-reserved metafieldsSet');

  appReservedDeleteNoFlag = await captureDocument(
    'app-reserved metafieldDefinitionDelete without flag',
    requestPaths.deleteNoFlag,
    { namespace: appNamespaceInput, key: 'config' },
  );
  assertUserErrorCode(
    appReservedDeleteNoFlag.response,
    ['data', 'metafieldDefinitionDelete', 'userErrors'],
    'RESERVED_NAMESPACE_ORPHANED_METAFIELDS',
    'app-reserved metafieldDefinitionDelete without flag',
  );

  appDefinitionReadAfterGuard = await captureDocument(
    'app-reserved definition read after guarded delete',
    requestPaths.read,
    { identifier: { ownerType: 'PRODUCT', namespace: appNamespaceInput, key: 'config' } },
  );

  appReservedDeleteWithFlag = await captureDocument(
    'app-reserved metafieldDefinitionDelete with flag',
    requestPaths.deleteWithFlag,
    { namespace: appNamespaceInput, key: 'config', deleteAllAssociatedMetafields: true },
  );
  assertNoUserErrors(
    appReservedDeleteWithFlag.response,
    ['data', 'metafieldDefinitionDelete', 'userErrors'],
    'app-reserved metafieldDefinitionDelete with flag',
  );
  appDefinitionId = undefined;
} finally {
  if (appDefinitionId) {
    cleanup.push(await cleanupDefinition(appDefinitionId));
  }
  if (productId) {
    cleanup.push(await cleanupProduct(productId));
  }
  if (standardCreatedByCapture && standardDefinitionId) {
    cleanup.push(await cleanupDefinition(standardDefinitionId));
  }
}

if (
  !standardEnable ||
  !standardImmutableUpdate ||
  !standardReadAfterRejectedUpdate ||
  !productCreate ||
  !appDefinitionCreate ||
  !appMetafieldsSet ||
  !appReservedDeleteNoFlag ||
  !appDefinitionReadAfterGuard ||
  !appReservedDeleteWithFlag
) {
  throw new Error('Capture did not complete all required interactions; cleanup was attempted.');
}

const fixture = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  seed: {
    runId,
    requestingApiClientId,
    appNamespaceInput,
    appNamespaceResolved,
    standardTemplateId: 'gid://shopify/StandardMetafieldDefinitionTemplate/1',
    standardCreatedByCapture,
  },
  standard: {
    beforeRead: standardBeforeRead,
    enable: standardEnable,
    immutableUpdate: standardImmutableUpdate,
    readAfterRejectedUpdate: standardReadAfterRejectedUpdate,
  },
  appReserved: {
    productCreate,
    definitionCreate: appDefinitionCreate,
    metafieldsSet: appMetafieldsSet,
    deleteNoFlag: appReservedDeleteNoFlag,
    readAfterGuard: appDefinitionReadAfterGuard,
    deleteWithFlag: appReservedDeleteWithFlag,
  },
  cleanup,
  upstreamCalls: [],
};

await mkdir(outputDir, { recursive: true });
await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
console.log(`Wrote ${outputPath}`);

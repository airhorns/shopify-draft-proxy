/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
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
  requestingApiClientId: string;
  appNamespaceInput: string;
  appNamespaceResolved: string;
  defaultNamespaceResolved: string;
  crossAppNamespace: string;
  key: string;
  defaultKey: string;
  productId?: string;
};

const requestPaths = {
  set: 'config/parity-requests/metafield-definitions/metafields-set-app-namespace-resolution.graphql',
  delete: 'config/parity-requests/metafield-definitions/metafields-delete-app-namespace-resolution.graphql',
  read: 'config/parity-requests/metafield-definitions/metafields-app-namespace-product-read.graphql',
};

const queries = Object.fromEntries(
  await Promise.all(
    Object.entries(requestPaths).map(async ([name, requestPath]) => [name, await readFile(requestPath, 'utf8')]),
  ),
) as Record<keyof typeof requestPaths, string>;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metafield-definitions');
const outputPath = path.join(outputDir, 'metafields-set-delete-app-namespace-resolution.json');
const runId = Date.now().toString(36);
const requestingApiClientId = process.env.SHOPIFY_CONFORMANCE_API_CLIENT_ID ?? '347082227713';
const suffix = `value_namespace_${runId}`;
const seed: Seed = {
  runId,
  requestingApiClientId,
  appNamespaceInput: `$app:${suffix}`,
  appNamespaceResolved: `app--${requestingApiClientId}--${suffix}`,
  defaultNamespaceResolved: `app--${requestingApiClientId}`,
  crossAppNamespace: `app--999999999999--${suffix}`,
  key: 'tier',
  defaultKey: `default_${runId}`,
};

const productCreateMutation = `#graphql
  mutation CreateProduct($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product { id title }
      userErrors { field message }
    }
  }
`;

const productDeleteMutation = `#graphql
  mutation DeleteProduct($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors { field message }
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
    if (object === null) {
      return undefined;
    }
    current = object[part];
  }

  return current;
}

function readUserErrors(payload: unknown, pathParts: string[]): unknown[] {
  const value = readPath(payload, pathParts);
  return Array.isArray(value) ? value : [];
}

function readTopErrors(payload: unknown): unknown[] {
  const value = readPath(payload, ['errors']);
  return Array.isArray(value) ? value : [];
}

function assertHttpOk(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300) {
    throw new Error(`${label} failed HTTP status: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertNoGraphqlErrors(result: ConformanceGraphqlResult, label: string): void {
  assertHttpOk(result, label);
  if (readTopErrors(result.payload).length > 0) {
    throw new Error(`${label} returned GraphQL errors: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertNoUserErrors(payload: unknown, pathParts: string[], label: string): void {
  const userErrors = readUserErrors(payload, pathParts);
  if (userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function readStringPath(payload: unknown, pathParts: string[], label: string): string {
  const value = readPath(payload, pathParts);
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`${label} did not return a string at ${pathParts.join('.')}: ${JSON.stringify(payload, null, 2)}`);
  }

  return value;
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
  assertNoGraphqlErrors(result, name);
  return captureFromResult(name, query, variables, result);
}

async function captureCrossAppSet(name: string, query: string, variables: Record<string, unknown>): Promise<Capture> {
  const result = await runGraphqlRaw(query, variables);
  assertNoGraphqlErrors(result, name);
  const userErrors = readUserErrors(result.payload, ['data', 'metafieldsSet', 'userErrors']);
  if (
    !userErrors.some((error) => {
      const object = readObject(error);
      return object?.['code'] === 'APP_NOT_AUTHORIZED';
    })
  ) {
    throw new Error(`${name} did not return APP_NOT_AUTHORIZED userErrors: ${JSON.stringify(result.payload)}`);
  }
  return captureFromResult(name, query, variables, result);
}

async function captureCrossAppDelete(
  name: string,
  query: string,
  variables: Record<string, unknown>,
): Promise<Capture> {
  const result = await runGraphqlRaw(query, variables);
  assertNoGraphqlErrors(result, name);
  const userErrors = readUserErrors(result.payload, ['data', 'metafieldsDelete', 'userErrors']);
  if (
    !userErrors.some((error) => {
      const object = readObject(error);
      return (
        object?.['message'] === 'Access to this namespace and key on Metafields for this resource type is not allowed.'
      );
    })
  ) {
    throw new Error(`${name} did not return app namespace authorization userErrors: ${JSON.stringify(result.payload)}`);
  }
  return captureFromResult(name, query, variables, result);
}

function productReadVariables(): Record<string, unknown> {
  return {
    productId: seed.productId,
    canonicalNamespace: seed.appNamespaceResolved,
    defaultNamespace: seed.defaultNamespaceResolved,
    key: seed.key,
    defaultKey: seed.defaultKey,
  };
}

async function cleanupProduct(cleanup: Capture[]): Promise<void> {
  if (!seed.productId) {
    return;
  }

  const cleanupCapture = await captureGraphql('cleanup-product-delete', productDeleteMutation, {
    input: { id: seed.productId },
  });
  cleanup.push(cleanupCapture);
  seed.productId = undefined;
}

async function writeBlocker(stage: string, error: unknown, captures: Capture[], cleanup: Capture[]): Promise<void> {
  await mkdir(outputDir, { recursive: true });
  const blockerPath = path.join(outputDir, `metafields-set-delete-app-namespace-resolution-blocker-${runId}.json`);
  const message = error instanceof Error ? error.message : String(error);
  await writeFile(
    blockerPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        command: 'corepack pnpm conformance:capture -- --run metafields-set-delete-app-namespace-resolution',
        blocker: { stage, message },
        seed,
        partialCaptures: captures,
        cleanup,
      },
      null,
      2,
    )}\n`,
  );
  console.error(`Wrote blocker evidence to ${blockerPath}`);
}

const cleanup: Capture[] = [];
const captures: Capture[] = [];

try {
  const productCreate = await captureGraphql('product-create-setup', productCreateMutation, {
    product: {
      title: `Metafields app namespace ${runId}`,
      status: 'DRAFT',
    },
  });
  captures.push(productCreate);
  seed.productId = readStringPath(productCreate.response, ['data', 'productCreate', 'product', 'id'], 'product-create');
  assertNoUserErrors(productCreate.response, ['data', 'productCreate', 'userErrors'], 'product-create');

  const appPrefixedSet = await captureGraphql('app-prefixed-set', queries.set, {
    metafields: [
      {
        ownerId: seed.productId,
        namespace: seed.appNamespaceInput,
        key: seed.key,
        type: 'single_line_text_field',
        value: 'gold',
      },
    ],
  });
  captures.push(appPrefixedSet);
  assertNoUserErrors(appPrefixedSet.response, ['data', 'metafieldsSet', 'userErrors'], 'app-prefixed-set');

  const postAppPrefixedSetRead = await captureGraphql(
    'post-app-prefixed-set-read',
    queries.read,
    productReadVariables(),
  );
  captures.push(postAppPrefixedSetRead);

  const missingNamespaceSet = await captureGraphql('missing-namespace-set', queries.set, {
    metafields: [
      {
        ownerId: seed.productId,
        key: seed.defaultKey,
        type: 'single_line_text_field',
        value: 'silver',
      },
    ],
  });
  captures.push(missingNamespaceSet);
  assertNoUserErrors(missingNamespaceSet.response, ['data', 'metafieldsSet', 'userErrors'], 'missing-namespace-set');

  const postMissingNamespaceSetRead = await captureGraphql(
    'post-missing-namespace-set-read',
    queries.read,
    productReadVariables(),
  );
  captures.push(postMissingNamespaceSetRead);

  const appPrefixedDelete = await captureGraphql('app-prefixed-delete', queries.delete, {
    metafields: [
      {
        ownerId: seed.productId,
        namespace: seed.appNamespaceInput,
        key: seed.key,
      },
    ],
  });
  captures.push(appPrefixedDelete);
  assertNoUserErrors(appPrefixedDelete.response, ['data', 'metafieldsDelete', 'userErrors'], 'app-prefixed-delete');

  const postDeleteRead = await captureGraphql('post-delete-read', queries.read, productReadVariables());
  captures.push(postDeleteRead);

  const crossAppSet = await captureCrossAppSet('cross-app-set', queries.set, {
    metafields: [
      {
        ownerId: seed.productId,
        namespace: seed.crossAppNamespace,
        key: seed.key,
        type: 'single_line_text_field',
        value: 'blocked',
      },
    ],
  });
  captures.push(crossAppSet);

  const crossAppDelete = await captureCrossAppDelete('cross-app-delete', queries.delete, {
    metafields: [
      {
        ownerId: seed.productId,
        namespace: seed.crossAppNamespace,
        key: seed.key,
      },
    ],
  });
  captures.push(crossAppDelete);

  await cleanupProduct(cleanup);

  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        summary:
          'metafieldsSet and metafieldsDelete app namespace resolution for value mutations, including omitted namespace defaulting and cross-app access denial.',
        seed,
        productCreate,
        appPrefixedSet,
        postAppPrefixedSetRead,
        missingNamespaceSet,
        postMissingNamespaceSetRead,
        appPrefixedDelete,
        postDeleteRead,
        crossAppSet,
        crossAppDelete,
        cleanup,
        upstreamCalls: [],
      },
      null,
      2,
    )}\n`,
  );
  console.log(`Wrote ${outputPath}`);
} catch (error) {
  try {
    await cleanupProduct(cleanup);
  } finally {
    await writeBlocker('capture', error, captures, cleanup);
  }
  throw error;
}

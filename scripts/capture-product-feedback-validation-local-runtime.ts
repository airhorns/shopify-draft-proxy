import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

type JsonRecord = Record<string, unknown>;
type ProxyResponse = {
  status: number;
  body: unknown;
};
type DraftProxyInstance = {
  processGraphQLRequest: (
    body: { query: string; variables?: JsonRecord },
    options?: { apiVersion?: string },
  ) => Promise<ProxyResponse>;
  dispose: () => void;
};

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, '..');
const apiVersion = '2026-04';
const fixturePath = path.join(
  repoRoot,
  'fixtures',
  'conformance',
  'local-runtime',
  apiVersion,
  'products',
  'product-feedback-validation-local-runtime.json',
);
const productFeedbackRequestPath = path.join(
  repoRoot,
  'config',
  'parity-requests',
  'products',
  'product-feedback-create-local-runtime.graphql',
);
const productInvalidStateRequestPath = path.join(
  repoRoot,
  'config',
  'parity-requests',
  'products',
  'product-feedback-invalid-state.graphql',
);
const shopFeedbackRequestPath = path.join(
  repoRoot,
  'config',
  'parity-requests',
  'products',
  'shop-feedback-create-local-runtime.graphql',
);
const shopInvalidStateRequestPath = path.join(
  repoRoot,
  'config',
  'parity-requests',
  'products',
  'shop-feedback-invalid-state.graphql',
);

function readObject(value: unknown, context: string): JsonRecord {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`${context} was not an object: ${JSON.stringify(value)}`);
  }
  return value as JsonRecord;
}

function assertHttpOk(response: ProxyResponse, context: string): JsonRecord {
  if (response.status !== 200) {
    throw new Error(`${context} returned HTTP ${response.status}: ${JSON.stringify(response.body)}`);
  }
  return readObject(response.body, `${context} response body`);
}

function fixtureVariables(fixture: JsonRecord, key: string): JsonRecord {
  return readObject(readObject(fixture[key], `fixture.${key}`)['variables'], `fixture.${key}.variables`);
}

async function graphql(
  proxy: DraftProxyInstance,
  query: string,
  variables: JsonRecord,
  context: string,
): Promise<JsonRecord> {
  return assertHttpOk(await proxy.processGraphQLRequest({ query, variables }, { apiVersion }), context);
}

const existingFixture = readObject(JSON.parse(await readFile(fixturePath, 'utf8')) as unknown, 'existing fixture');
const productFeedbackQuery = await readFile(productFeedbackRequestPath, 'utf8');
const productInvalidStateQuery = await readFile(productInvalidStateRequestPath, 'utf8');
const shopFeedbackQuery = await readFile(shopFeedbackRequestPath, 'utf8');
const shopInvalidStateQuery = await readFile(shopInvalidStateRequestPath, 'utf8');
const { createDraftProxy } = (await import('../js/src/index.js')) as {
  createDraftProxy: (options: { readMode: string; port: number; shopifyAdminOrigin: string }) => DraftProxyInstance;
};

const proxy = createDraftProxy({
  readMode: 'live-hybrid',
  port: 0,
  shopifyAdminOrigin: 'https://local-runtime.invalid',
});

try {
  const fixture = {
    fixtureKind: 'local-runtime-product-feedback-validation',
    apiVersion,
    productInvalidState: {
      response: await graphql(proxy, productInvalidStateQuery, {}, 'product invalid state'),
    },
    productBlankMessages: {
      variables: fixtureVariables(existingFixture, 'productBlankMessages'),
      response: await graphql(
        proxy,
        productFeedbackQuery,
        fixtureVariables(existingFixture, 'productBlankMessages'),
        'product blank messages',
      ),
    },
    productFutureGeneratedAt: {
      variables: fixtureVariables(existingFixture, 'productFutureGeneratedAt'),
      response: await graphql(
        proxy,
        productFeedbackQuery,
        fixtureVariables(existingFixture, 'productFutureGeneratedAt'),
        'product future generated at',
      ),
    },
    productMixedBatchPerEntryValidation: {
      variables: fixtureVariables(existingFixture, 'productMixedBatchPerEntryValidation'),
      response: await graphql(
        proxy,
        productFeedbackQuery,
        fixtureVariables(existingFixture, 'productMixedBatchPerEntryValidation'),
        'product mixed batch per-entry validation',
      ),
    },
    productTooLongMessage: {
      variables: fixtureVariables(existingFixture, 'productTooLongMessage'),
      response: await graphql(
        proxy,
        productFeedbackQuery,
        fixtureVariables(existingFixture, 'productTooLongMessage'),
        'product too-long message',
      ),
    },
    productBatchTooLong: {
      variables: fixtureVariables(existingFixture, 'productBatchTooLong'),
      response: await graphql(
        proxy,
        productFeedbackQuery,
        fixtureVariables(existingFixture, 'productBatchTooLong'),
        'product batch too long',
      ),
    },
    shopInvalidState: {
      response: await graphql(proxy, shopInvalidStateQuery, {}, 'shop invalid state'),
    },
    shopBlankMessages: {
      variables: fixtureVariables(existingFixture, 'shopBlankMessages'),
      response: await graphql(
        proxy,
        shopFeedbackQuery,
        fixtureVariables(existingFixture, 'shopBlankMessages'),
        'shop blank messages',
      ),
    },
    upstreamCalls: [],
  };

  await mkdir(path.dirname(fixturePath), { recursive: true });
  await writeFile(fixturePath, `${JSON.stringify(fixture, null, 2)}\n`);
} finally {
  proxy.dispose();
}

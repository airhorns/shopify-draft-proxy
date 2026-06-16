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
const apiVersion = '2025-01';
const giftCardsDir = path.join(repoRoot, 'fixtures', 'conformance', 'local-runtime', apiVersion, 'gift-cards');
const entitlementFixturePath = path.join(giftCardsDir, 'gift-card-entitlement-disabled.json');
const notifyFixturePath = path.join(giftCardsDir, 'gift-card-create-notify.json');
const entitlementRequestPath = path.join(
  repoRoot,
  'config',
  'parity-requests',
  'gift-cards',
  'gift-card-entitlement-disabled.graphql',
);
const notifyRequestPath = path.join(
  repoRoot,
  'config',
  'parity-requests',
  'gift-cards',
  'gift-card-create-notify.graphql',
);

const entitlementMessage = 'Gift cards are unavailable on your plan.';
const notifyDisabledMessage = 'Notifications for this gift card are disabled.';

function readObject(value: unknown, context: string): JsonRecord {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`${context} was not an object: ${JSON.stringify(value)}`);
  }

  return value as JsonRecord;
}

function assertResponseOk(response: ProxyResponse, context: string): JsonRecord {
  if (response.status !== 200) {
    throw new Error(`${context} returned HTTP ${response.status}: ${JSON.stringify(response.body)}`);
  }

  const body = readObject(response.body, `${context} response body`);
  if ('errors' in body) {
    throw new Error(`${context} returned top-level GraphQL errors: ${JSON.stringify(body['errors'])}`);
  }

  return body;
}

function assertEveryUserErrorMessage(data: JsonRecord, expected: string, context: string): void {
  for (const [key, payload] of Object.entries(data)) {
    const userErrors = readObject(payload, `${context}.${key}`)['userErrors'];
    if (!Array.isArray(userErrors) || userErrors.length !== 1) {
      throw new Error(`${context}.${key}.userErrors was not a single error: ${JSON.stringify(userErrors)}`);
    }

    const error = readObject(userErrors[0], `${context}.${key}.userErrors[0]`);
    if (error['message'] !== expected) {
      throw new Error(`${context}.${key} message diverged: ${JSON.stringify(error['message'])}`);
    }
  }
}

const [entitlementQuery, notifyQuery, entitlementFixture, notifyFixture] = await Promise.all([
  readFile(entitlementRequestPath, 'utf8'),
  readFile(notifyRequestPath, 'utf8'),
  readFile(entitlementFixturePath, 'utf8').then((contents) =>
    readObject(JSON.parse(contents) as unknown, 'entitlement fixture'),
  ),
  readFile(notifyFixturePath, 'utf8').then((contents) => readObject(JSON.parse(contents) as unknown, 'notify fixture')),
]);
const { createDraftProxy } = (await import('../js/src/index.js')) as {
  createDraftProxy: (options: { readMode: string; port: number; shopifyAdminOrigin: string }) => DraftProxyInstance;
};

const proxy = createDraftProxy({
  readMode: 'snapshot',
  port: 0,
  shopifyAdminOrigin: 'https://local-runtime.invalid',
});

try {
  const entitlementBody = assertResponseOk(
    await proxy.processGraphQLRequest({ query: entitlementQuery, variables: {} }, { apiVersion }),
    'entitlement-disabled',
  );
  const entitlementData = readObject(entitlementBody['data'], 'entitlement-disabled data');
  assertEveryUserErrorMessage(entitlementData, entitlementMessage, 'entitlement-disabled');

  const notifyBody = assertResponseOk(
    await proxy.processGraphQLRequest({ query: notifyQuery, variables: {} }, { apiVersion }),
    'create-notify',
  );
  const notifyData = readObject(notifyBody['data'], 'create-notify data');
  const notifyDisabled = readObject(notifyData['notifyDisabled'], 'create-notify notifyDisabled');
  const notifyErrors = notifyDisabled['userErrors'];
  if (!Array.isArray(notifyErrors) || notifyErrors.length !== 1) {
    throw new Error(`notify-disabled userErrors was not a single error: ${JSON.stringify(notifyErrors)}`);
  }

  const notifyError = readObject(notifyErrors[0], 'notify-disabled userErrors[0]');
  if (notifyError['message'] !== notifyDisabledMessage) {
    throw new Error(`notify-disabled message diverged: ${JSON.stringify(notifyError['message'])}`);
  }

  await mkdir(giftCardsDir, { recursive: true });
  await writeFile(
    entitlementFixturePath,
    `${JSON.stringify(
      {
        ...entitlementFixture,
        expected: entitlementBody,
      },
      null,
      2,
    )}\n`,
  );
  await writeFile(
    notifyFixturePath,
    `${JSON.stringify(
      {
        ...notifyFixture,
        expected: notifyBody,
      },
      null,
      2,
    )}\n`,
  );
} finally {
  proxy.dispose();
}

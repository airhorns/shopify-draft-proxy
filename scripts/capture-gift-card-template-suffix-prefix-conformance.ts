/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { setTimeout } from 'node:timers/promises';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type CapturedRequest = {
  label: string;
  query: string;
  variables: Record<string, unknown>;
  response: ConformanceGraphqlResult;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2025-01',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'gift-cards');
const outputPath = path.join(outputDir, 'gift-card-template-suffix-prefix.json');

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function readPath(value: unknown, pathParts: string[]): unknown {
  return pathParts.reduce<unknown>((cursor, part) => {
    if (!isObject(cursor)) {
      return undefined;
    }
    return cursor[part];
  }, value);
}

function readStringPath(value: unknown, pathParts: string[]): string | null {
  const found = readPath(value, pathParts);
  return typeof found === 'string' ? found : null;
}

function readCreatedGiftCardId(capture: CapturedRequest): string | null {
  return readStringPath(capture.response.payload, ['data', 'giftCardCreate', 'giftCard', 'id']);
}

function giftCardTail(id: string): string {
  return id.split('/').at(-1)?.split('?')[0] ?? id;
}

function readArrayPath(value: unknown, pathParts: string[]): unknown[] {
  const found = readPath(value, pathParts);
  return Array.isArray(found) ? found : [];
}

function assertStringPath(capture: CapturedRequest, pathParts: string[], expected: string): void {
  const actual = readStringPath(capture.response.payload, pathParts);
  if (actual !== expected) {
    throw new Error(
      `${capture.label} expected ${pathParts.join('.')} to be ${JSON.stringify(expected)}; got ${JSON.stringify(actual)}.`,
    );
  }
}

function assertNoTopLevelErrors(capture: CapturedRequest): void {
  if (capture.response.payload.errors) {
    throw new Error(`${capture.label} returned top-level errors: ${JSON.stringify(capture.response.payload.errors)}`);
  }
}

async function capture(
  label: string,
  query: string,
  variables: Record<string, unknown> = {},
): Promise<CapturedRequest> {
  const response = await runGraphqlRequest(query, variables);
  return { label, query, variables, response };
}

async function deactivateGiftCard(label: string, id: string): Promise<CapturedRequest> {
  const captured = await capture(
    label,
    `#graphql
      mutation GiftCardTemplateSuffixPrefixDeactivate($id: ID!) {
        giftCardDeactivate(id: $id) {
          giftCard {
            id
            enabled
            deactivatedAt
          }
          userErrors {
            field
            message
          }
        }
      }
    `,
    { id },
  );
  assertNoTopLevelErrors(captured);
  return captured;
}

async function captureReadAfterUpdateWithRetry(
  variables: Record<string, unknown>,
  expectedId: string,
): Promise<CapturedRequest> {
  const maxAttempts = 8;
  const delayMs = 5_000;
  let lastCapture: CapturedRequest | null = null;

  for (let attempt = 1; attempt <= maxAttempts; attempt += 1) {
    const captured = await capture('readAfterTemplateSuffixPrefixUpdate', readAfterUpdateQuery, variables);
    assertNoTopLevelErrors(captured);
    lastCapture = captured;

    const nodes = readArrayPath(captured.response.payload, ['data', 'giftCards', 'nodes']);
    const matchingNodes = nodes.filter((node) => readStringPath(node, ['id']) === expectedId);
    const topLevelSuffix = readStringPath(captured.response.payload, ['data', 'giftCard', 'templateSuffix']);
    const listSuffix = matchingNodes.length === 1 ? readStringPath(matchingNodes[0], ['templateSuffix']) : null;

    if (nodes.length === 1 && matchingNodes.length === 1 && topLevelSuffix === 'foo' && listSuffix === 'foo') {
      return captured;
    }

    if (attempt < maxAttempts) {
      await setTimeout(delayMs);
    }
  }

  const lastNodes =
    lastCapture === null ? [] : readArrayPath(lastCapture.response.payload, ['data', 'giftCards', 'nodes']);
  const lastNodeIds = lastNodes.map((node) => readStringPath(node, ['id']));
  throw new Error(
    `Expected code-fragment giftCards readback to return exactly one setup card with stripped templateSuffix after ${maxAttempts} attempts; last node ids: ${JSON.stringify(lastNodeIds)}.`,
  );
}

const createMutation = `#graphql
  mutation GiftCardTemplateSuffixPrefixCreate($input: GiftCardCreateInput!) {
    giftCardCreate(input: $input) {
      giftCard {
        id
        templateSuffix
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const updateMutation = `#graphql
  mutation GiftCardTemplateSuffixPrefixUpdate($id: ID!, $templateSuffix: String!) {
    giftCardUpdate(id: $id, input: { templateSuffix: $templateSuffix }) {
      giftCard {
        id
        templateSuffix
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const readAfterUpdateQuery = `#graphql
  query GiftCardTemplateSuffixPrefixRead($id: ID!, $query: String!) {
    giftCard(id: $id) {
      id
      templateSuffix
    }
    giftCards(first: 2, query: $query, sortKey: ID) {
      nodes {
        id
        templateSuffix
      }
    }
  }
`;

const runSuffix = Date.now().toString(36).toUpperCase();
const setup: CapturedRequest[] = [];
const cleanup: CapturedRequest[] = [];
const createdGiftCardIds: string[] = [];

try {
  const createInput = {
    initialValue: '5.00',
    code: `PFX${runSuffix}`,
    templateSuffix: 'gift_card.birthday',
  };
  const updateVariables = {
    templateSuffix: 'gift_card.foo',
  };

  const create = await capture('templateSuffixPrefixCreate', createMutation, { input: createInput });
  assertNoTopLevelErrors(create);
  assertStringPath(create, ['data', 'giftCardCreate', 'giftCard', 'templateSuffix'], 'birthday');
  setup.push(create);

  const createdId = readCreatedGiftCardId(create);
  if (createdId === null) {
    throw new Error('Unable to create disposable gift card for template suffix prefix capture.');
  }
  createdGiftCardIds.push(createdId);

  const update = await capture('templateSuffixPrefixUpdate', updateMutation, {
    id: createdId,
    ...updateVariables,
  });
  assertNoTopLevelErrors(update);
  assertStringPath(update, ['data', 'giftCardUpdate', 'giftCard', 'templateSuffix'], 'foo');

  const readAfterUpdateVariables = {
    id: createdId,
    query: createInput.code.slice(-4),
  };
  const readAfterUpdate = await captureReadAfterUpdateWithRetry(readAfterUpdateVariables, createdId);

  cleanup.push(await deactivateGiftCard('cleanupDeactivate:templateSuffixPrefixCard', createdId));

  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        notes: [
          'Captures live giftCardCreate and giftCardUpdate behavior for templateSuffix values with a leading gift_card. prefix.',
          'Shopify strips exactly the literal leading gift_card. prefix, returns the stripped suffix in mutation payloads, and downstream reads expose the stripped value.',
          'Setup creates one disposable gift card with a run-unique code; cleanup deactivates the setup gift card.',
        ],
        proxyVariables: {
          create: { input: createInput },
          update: updateVariables,
          readAfterUpdate: {
            idQuery: `id:${giftCardTail(createdId)}`,
            query: readAfterUpdateVariables.query,
          },
        },
        setup,
        operations: {
          update,
          readAfterUpdate,
        },
        cleanup,
        upstreamCalls: [],
      },
      null,
      2,
    )}\n`,
    'utf8',
  );

  console.log(`wrote ${outputPath}`);
} catch (error) {
  for (const id of createdGiftCardIds) {
    try {
      cleanup.push(await deactivateGiftCard(`cleanupAfterError:giftCard:${id}`, id));
    } catch {
      // best-effort cleanup
    }
  }
  throw error;
}

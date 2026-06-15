/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { assertDiscountConformanceScopes, probeDiscountConformanceScopes } from './discount-conformance-lib.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

function assertSuccess(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function readPath(value: unknown, pathSegments: string[]): unknown {
  let current = value;
  for (const segment of pathSegments) {
    if (current === null || typeof current !== 'object') {
      return undefined;
    }
    current = (current as JsonRecord)[segment];
  }
  return current;
}

function readRequiredString(result: ConformanceGraphqlResult, pathSegments: string[], context: string): string {
  const value = readPath(result.payload, pathSegments);
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`${context} did not return ${pathSegments.join('.')}: ${JSON.stringify(result.payload, null, 2)}`);
  }
  return value;
}

function assertNoUserErrors(result: ConformanceGraphqlResult, pathSegments: string[], context: string): void {
  const value = readPath(result.payload, pathSegments);
  if (!Array.isArray(value) || value.length !== 0) {
    throw new Error(`${context} returned userErrors: ${JSON.stringify(value, null, 2)}`);
  }
}

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'discounts');
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const adminOptions = {
  adminOrigin,
  apiVersion,
  accessToken: adminAccessToken,
  headers: buildAdminAuthHeaders(adminAccessToken),
};
const { runGraphqlRaw } = createAdminGraphqlClient(adminOptions);

await mkdir(outputDir, { recursive: true });

const scopeProbe = await probeDiscountConformanceScopes(adminOptions);
assertDiscountConformanceScopes(scopeProbe);

const createDocument = await readFile(
  'config/parity-requests/discounts/discount-activate-deactivate-edge-create.graphql',
  'utf8',
);
const deactivateDocument = await readFile(
  'config/parity-requests/discounts/discount-activate-deactivate-edge-deactivate.graphql',
  'utf8',
);
const activateDocument = await readFile(
  'config/parity-requests/discounts/discount-activate-deactivate-edge-activate.graphql',
  'utf8',
);
const readDocument = await readFile(
  'config/parity-requests/discounts/discount-activate-deactivate-edge-read.graphql',
  'utf8',
);
const unknownDocument = await readFile(
  'config/parity-requests/discounts/discount-activate-deactivate-edge-unknown.graphql',
  'utf8',
);
const deleteDocument = await readFile(
  'config/parity-requests/discounts/discount-delete-unknown-id-code.graphql',
  'utf8',
);

const runId = Date.now();
const code = `HAREDGE${runId}`;
const createVariables = {
  input: {
    title: `HAR activate edge ${runId}`,
    code,
    startsAt: '2030-01-01T00:00:00.000Z',
    combinesWith: {
      productDiscounts: false,
      orderDiscounts: true,
      shippingDiscounts: false,
    },
    context: {
      all: 'ALL',
    },
    customerGets: {
      value: {
        percentage: 0.1,
      },
      items: {
        all: true,
      },
    },
  },
};

let discountId: string | null = null;
let deleted = false;

try {
  const create = await runGraphqlRaw(createDocument, createVariables);
  assertSuccess(create, 'discount activate/deactivate edge create');
  assertNoUserErrors(create, ['data', 'discountCodeBasicCreate', 'userErrors'], 'discount edge create');
  discountId = readRequiredString(
    create,
    ['data', 'discountCodeBasicCreate', 'codeDiscountNode', 'id'],
    'discount activate/deactivate edge create',
  );

  const deactivate = await runGraphqlRaw(deactivateDocument, { id: discountId });
  assertSuccess(deactivate, 'discountCodeDeactivate edge');
  assertNoUserErrors(deactivate, ['data', 'discountCodeDeactivate', 'userErrors'], 'discountCodeDeactivate edge');

  const activate = await runGraphqlRaw(activateDocument, { id: discountId });
  assertSuccess(activate, 'discountCodeActivate edge');
  assertNoUserErrors(activate, ['data', 'discountCodeActivate', 'userErrors'], 'discountCodeActivate edge');

  const readAfterActivate = await runGraphqlRaw(readDocument, { id: discountId, code });
  assertSuccess(readAfterActivate, 'discount activate/deactivate edge read');

  const unknown = await runGraphqlRaw(unknownDocument, {});
  assertSuccess(unknown, 'discount activate/deactivate unknown ids');

  const cleanup = await runGraphqlRaw(deleteDocument, { id: discountId });
  assertSuccess(cleanup, 'discount activate/deactivate edge cleanup');
  assertNoUserErrors(cleanup, ['data', 'discountCodeDelete', 'userErrors'], 'discount edge cleanup');
  deleted = true;

  const output = {
    scenarioId: 'discount-activate-deactivate-edge-cases',
    storeDomain,
    apiVersion,
    runId,
    variables: {
      id: discountId,
      code,
    },
    requests: {
      create: { query: createDocument, variables: createVariables },
      deactivate: { query: deactivateDocument, variables: { id: discountId } },
      activate: { query: activateDocument, variables: { id: discountId } },
      readAfterActivate: { query: readDocument, variables: { id: discountId, code } },
      unknown: { query: unknownDocument, variables: {} },
      cleanup: { query: deleteDocument, variables: { id: discountId } },
    },
    scopeProbe,
    create: { response: create },
    deactivate: { response: deactivate },
    activate: { response: activate },
    readAfterActivate: { response: readAfterActivate },
    unknown: { response: unknown },
    cleanup: { response: cleanup },
    upstreamCalls: [],
  };

  const outputPath = path.join(outputDir, 'discount-activate-deactivate-edge-cases.json');
  await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');

  console.log(
    JSON.stringify(
      {
        ok: true,
        apiVersion,
        outputPath,
        discountId,
        code,
      },
      null,
      2,
    ),
  );
} finally {
  if (!deleted && discountId !== null) {
    await runGraphqlRaw(deleteDocument, { id: discountId });
  }
}

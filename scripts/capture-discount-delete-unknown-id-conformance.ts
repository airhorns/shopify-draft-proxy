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

const setupDocument = await readFile(
  'config/parity-requests/discounts/discount-delete-unknown-id-setup.graphql',
  'utf8',
);
const codeDeleteDocument = await readFile(
  'config/parity-requests/discounts/discount-delete-unknown-id-code.graphql',
  'utf8',
);
const automaticDeleteDocument = await readFile(
  'config/parity-requests/discounts/discount-delete-unknown-id-automatic.graphql',
  'utf8',
);

const runId = Date.now();
const startsAt = new Date(Date.now() - 60_000).toISOString();
const setupVariables = {
  codeInput: {
    title: `HAR delete unknown code ${runId}`,
    code: `HARDEL${runId}`,
    startsAt,
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
  automaticInput: {
    title: `HAR delete unknown automatic ${runId}`,
    startsAt,
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

let codeDiscountId: string | null = null;
let automaticDiscountId: string | null = null;
let codeDeleted = false;
let automaticDeleted = false;

const unknownCodeDelete = await runGraphqlRaw(codeDeleteDocument, {
  id: 'gid://shopify/DiscountCodeNode/0',
});
assertSuccess(unknownCodeDelete, 'discountCodeDelete unknown id');

const unknownAutomaticDelete = await runGraphqlRaw(automaticDeleteDocument, {
  id: 'gid://shopify/DiscountAutomaticNode/0',
});
assertSuccess(unknownAutomaticDelete, 'discountAutomaticDelete unknown id');

try {
  const setup = await runGraphqlRaw(setupDocument, setupVariables);
  assertSuccess(setup, 'discount delete unknown setup');
  assertNoUserErrors(setup, ['data', 'codeSetup', 'userErrors'], 'discountCodeBasicCreate setup');
  assertNoUserErrors(setup, ['data', 'automaticSetup', 'userErrors'], 'discountAutomaticBasicCreate setup');

  codeDiscountId = readRequiredString(
    setup,
    ['data', 'codeSetup', 'codeDiscountNode', 'id'],
    'discountCodeBasicCreate setup',
  );
  automaticDiscountId = readRequiredString(
    setup,
    ['data', 'automaticSetup', 'automaticDiscountNode', 'id'],
    'discountAutomaticBasicCreate setup',
  );

  const codeDelete = await runGraphqlRaw(codeDeleteDocument, { id: codeDiscountId });
  assertSuccess(codeDelete, 'discountCodeDelete setup discount');
  assertNoUserErrors(codeDelete, ['data', 'discountCodeDelete', 'userErrors'], 'discountCodeDelete setup discount');
  codeDeleted = true;

  const automaticDelete = await runGraphqlRaw(automaticDeleteDocument, { id: automaticDiscountId });
  assertSuccess(automaticDelete, 'discountAutomaticDelete setup discount');
  assertNoUserErrors(
    automaticDelete,
    ['data', 'discountAutomaticDelete', 'userErrors'],
    'discountAutomaticDelete setup discount',
  );
  automaticDeleted = true;

  const output = {
    scenarioId: 'discount-delete-unknown-id',
    storeDomain,
    apiVersion,
    runId,
    variables: {
      codeDiscountId,
      automaticDiscountId,
    },
    requests: {
      setup: { query: setupDocument, variables: setupVariables },
      unknownCodeDelete: {
        query: codeDeleteDocument,
        variables: { id: 'gid://shopify/DiscountCodeNode/0' },
      },
      unknownAutomaticDelete: {
        query: automaticDeleteDocument,
        variables: { id: 'gid://shopify/DiscountAutomaticNode/0' },
      },
      codeDelete: { query: codeDeleteDocument, variables: { id: codeDiscountId } },
      automaticDelete: {
        query: automaticDeleteDocument,
        variables: { id: automaticDiscountId },
      },
    },
    scopeProbe,
    unknownCodeDelete: { response: unknownCodeDelete },
    unknownAutomaticDelete: { response: unknownAutomaticDelete },
    setup: { response: setup },
    codeDelete: { response: codeDelete },
    automaticDelete: { response: automaticDelete },
    upstreamCalls: [],
  };

  const outputPath = path.join(outputDir, 'discount-delete-unknown-id.json');
  await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');

  console.log(
    JSON.stringify(
      {
        ok: true,
        apiVersion,
        outputPath,
        codeDiscountId,
        automaticDiscountId,
      },
      null,
      2,
    ),
  );
} finally {
  if (!codeDeleted && codeDiscountId !== null) {
    await runGraphqlRaw(codeDeleteDocument, { id: codeDiscountId });
  }
  if (!automaticDeleted && automaticDiscountId !== null) {
    await runGraphqlRaw(automaticDeleteDocument, { id: automaticDiscountId });
  }
}

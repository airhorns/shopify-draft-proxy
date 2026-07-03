/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { assertDiscountConformanceScopes, probeDiscountConformanceScopes } from './discount-conformance-lib.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

const setupDocumentPath = 'config/parity-requests/discounts/discount-lifecycle-cross-kind-id-setup.graphql';
const crossKindDocumentPath = 'config/parity-requests/discounts/discount-lifecycle-cross-kind-id-mutations.graphql';
const readDocumentPath = 'config/parity-requests/discounts/discount-lifecycle-cross-kind-id-read.graphql';

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

async function readText(filePath: string): Promise<string> {
  return readFile(filePath, 'utf8');
}

function asRecord(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function pathValue(value: unknown, segments: string[]): unknown {
  return segments.reduce<unknown>((current, segment) => asRecord(current)?.[segment], value);
}

function requiredString(value: unknown, segments: string[], label: string): string {
  const candidate = pathValue(value, segments);
  if (typeof candidate !== 'string' || candidate.length === 0) {
    throw new Error(`${label} missing at ${segments.join('.')}: ${JSON.stringify(value)}`);
  }
  return candidate;
}

function rewriteShopifyGidType(id: string, resourceType: 'DiscountAutomaticNode' | 'DiscountCodeNode'): string {
  const match = /^gid:\/\/shopify\/[^/]+\/(.+)$/u.exec(id);
  if (!match) {
    throw new Error(`Cannot rewrite non-Shopify GID ${id}`);
  }
  return `gid://shopify/${resourceType}/${match[1]}`;
}

function assertNoTopLevelErrors(response: ConformanceGraphqlResult, label: string): void {
  if (response.status < 200 || response.status >= 300 || response.payload.errors) {
    throw new Error(`${label} returned errors: ${JSON.stringify(response, null, 2)}`);
  }
}

const setupDocument = await readText(setupDocumentPath);
const crossKindDocument = await readText(crossKindDocumentPath);
const readDocument = await readText(readDocumentPath);
const cleanupDocument = `#graphql
  mutation DiscountLifecycleCrossKindCleanup($codeId: ID!, $automaticId: ID!) {
    codeCleanup: discountCodeDelete(id: $codeId) {
      deletedCodeDiscountId
      userErrors {
        field
        message
        code
        extraInfo
      }
    }
    automaticCleanup: discountAutomaticDelete(id: $automaticId) {
      deletedAutomaticDiscountId
      userErrors {
        field
        message
        code
        extraInfo
      }
    }
  }
`;

const runId = Date.now();
const seedCode = `SDPXKBASE${runId}`;
const rejectedCode = `SDPXKREJECT${runId}`;
const startsAt = '2026-04-01T00:00:00Z';
const setupVariables = {
  codeInput: {
    title: `Cross-kind code ${runId}`,
    code: seedCode,
    startsAt,
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
  automaticInput: {
    title: `Cross-kind automatic ${runId}`,
    startsAt,
    combinesWith: {
      productDiscounts: false,
      orderDiscounts: true,
      shippingDiscounts: false,
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

let codeId: string | null = null;
let automaticId: string | null = null;
let cleanup: ConformanceGraphqlResult | null = null;

try {
  const setup = await runGraphqlRaw(setupDocument, setupVariables);
  assertNoTopLevelErrors(setup, 'setup');
  codeId = requiredString(setup, ['payload', 'data', 'codeSetup', 'codeDiscountNode', 'id'], 'code discount id');
  automaticId = requiredString(
    setup,
    ['payload', 'data', 'automaticSetup', 'automaticDiscountNode', 'id'],
    'automatic discount id',
  );
  const codeRootAutomaticId = rewriteShopifyGidType(automaticId, 'DiscountCodeNode');
  const automaticRootCodeId = rewriteShopifyGidType(codeId, 'DiscountAutomaticNode');

  const crossKindVariables = {
    codeRootAutomaticId,
    automaticRootCodeId,
    rejectedCode,
  };
  const crossKind = await runGraphqlRaw(crossKindDocument, crossKindVariables);
  assertNoTopLevelErrors(crossKind, 'cross-kind mutations');

  const readVariables = {
    codeId,
    automaticId,
    rejectedCode,
  };
  const readAfter = await runGraphqlRaw(readDocument, readVariables);
  assertNoTopLevelErrors(readAfter, 'read after cross-kind mutations');

  const cleanupVariables = {
    codeId,
    automaticId,
  };
  cleanup = await runGraphqlRaw(cleanupDocument, cleanupVariables);

  const output = {
    variables: {
      codeId,
      automaticId,
      codeRootAutomaticId,
      automaticRootCodeId,
      seedCode,
      rejectedCode,
      runId,
    },
    requests: {
      setup: { query: setupDocument, variables: setupVariables },
      crossKind: { query: crossKindDocument, variables: crossKindVariables },
      readAfter: { query: readDocument, variables: readVariables },
      cleanup: { query: cleanupDocument, variables: cleanupVariables },
    },
    scopeProbe,
    setup,
    crossKind,
    readAfter,
    cleanup,
  };

  const outputPath = path.join(outputDir, 'discount-lifecycle-cross-kind-id.json');
  await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');

  console.log(
    JSON.stringify(
      {
        ok: true,
        apiVersion,
        outputPath,
        codeId,
        automaticId,
        rejectedCode,
      },
      null,
      2,
    ),
  );
} catch (error) {
  if ((codeId || automaticId) && !cleanup) {
    try {
      cleanup = await runGraphqlRaw(cleanupDocument, {
        codeId: codeId ?? 'gid://shopify/DiscountCodeNode/0',
        automaticId: automaticId ?? 'gid://shopify/DiscountAutomaticNode/0',
      });
      console.error(`Cleaned up after capture failure: ${JSON.stringify(cleanup)}`);
    } catch (cleanupError) {
      console.error(`Failed to clean up after capture failure: ${String(cleanupError)}`);
    }
  }
  throw error;
}

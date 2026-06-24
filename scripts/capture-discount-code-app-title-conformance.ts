/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { readConformanceScriptConfig } from './conformance-script-config.js';
import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { assertDiscountConformanceScopes, probeDiscountConformanceScopes } from './discount-conformance-lib.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;
type CaptureCall = {
  operationName: string;
  variables: Record<string, unknown>;
  query: string;
  response: {
    status: number;
    body: unknown;
  };
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'discounts');
const outputPath = path.join(outputDir, 'discount-code-app-title.json');

const setupDocumentPath = 'config/parity-requests/discounts/discount-code-app-title-setup.graphql';
const createDocumentPath = 'config/parity-requests/discounts/discount-code-app-title-create.graphql';
const updateDocumentPath = 'config/parity-requests/discounts/discount-code-app-title-update.graphql';

const setupDocument = await readFile(setupDocumentPath, 'utf8');
const createDocument = await readFile(createDocumentPath, 'utf8');
const updateDocument = await readFile(updateDocumentPath, 'utf8');

const functionCatalogDocument = `#graphql
  query DiscountCodeAppTitleFunctionCatalog {
    shopifyFunctions(first: 50) {
      nodes {
        id
        title
        handle
        apiType
        description
        appKey
        app {
          id
          title
          handle
          apiKey
        }
      }
    }
  }
`;

const functionHydrateByHandleDocument = `query ShopifyFunctionByHandle($handle: String!) {
  shopifyFunctions(first: 1, handle: $handle) {
    nodes {
      id
      title
      handle
      apiType
      description
      appKey
      app {
        id
        title
        handle
        apiKey
      }
    }
  }
}
`;

const deleteCodeDocument = `#graphql
  mutation DiscountCodeAppTitleDeleteCode($id: ID!) {
    discountCodeDelete(id: $id) {
      deletedCodeDiscountId
      userErrors {
        field
        message
        code
        extraInfo
      }
    }
  }
`;

const deleteAutomaticDocument = `#graphql
  mutation DiscountCodeAppTitleDeleteAutomatic($id: ID!) {
    discountAutomaticDelete(id: $id) {
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

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const adminOptions = {
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
};
const { runGraphqlRequest, runGraphqlRaw } = createAdminGraphqlClient(adminOptions);

function readRecord(value: unknown): JsonRecord | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function readString(value: unknown): string | null {
  return typeof value === 'string' && value.length > 0 ? value : null;
}

function readPath(value: unknown, pathSegments: string[]): unknown {
  let current: unknown = value;
  for (const segment of pathSegments) {
    const record = readRecord(current);
    if (!record) {
      return null;
    }
    current = record[segment];
  }

  return current;
}

function assertHttpOk(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300) {
    throw new Error(`${context} returned HTTP ${result.status}: ${JSON.stringify(result.payload, null, 2)}`);
  }
}

function readFunctionNodes(catalog: ConformanceGraphqlResult): JsonRecord[] {
  const connection = readRecord(readRecord(catalog.payload.data)?.['shopifyFunctions']);
  return readArray(connection?.['nodes']).flatMap((node) => {
    const record = readRecord(node);
    return record ? [record] : [];
  });
}

function isDiscountApi(apiType: unknown): boolean {
  return (
    apiType === 'discount' ||
    apiType === 'product_discounts' ||
    apiType === 'order_discounts' ||
    apiType === 'shipping_discounts'
  );
}

function findDiscountFunction(nodes: JsonRecord[]): JsonRecord {
  const deployed = nodes.find(
    (node) => readString(node['handle']) === 'conformance-discount' && isDiscountApi(node['apiType']),
  );
  if (!deployed) {
    throw new Error(`Expected deployed conformance-discount Function in catalog: ${JSON.stringify(nodes, null, 2)}`);
  }

  return deployed;
}

function captureHydrateCall(handle: string, node: JsonRecord): CaptureCall {
  return {
    operationName: 'ShopifyFunctionByHandle',
    variables: { handle },
    query: functionHydrateByHandleDocument,
    response: {
      status: 200,
      body: {
        data: {
          shopifyFunctions: {
            nodes: [node],
          },
        },
      },
    },
  };
}

function validCodeInput(stamp: number, functionHandle: string): Record<string, unknown> {
  return {
    title: `Conformance code setup ${stamp}`,
    code: `APPCTSETUP${stamp}`,
    startsAt: '2026-05-05T00:00:00Z',
    functionHandle,
    discountClasses: ['ORDER'],
  };
}

function validAutomaticInput(stamp: number, functionHandle: string): Record<string, unknown> {
  return {
    title: `Conformance automatic setup ${stamp}`,
    startsAt: '2026-05-05T00:00:00Z',
    functionHandle,
    discountClasses: ['ORDER'],
  };
}

function baseCodeInput(stamp: number, suffix: string, functionHandle: string): Record<string, unknown> {
  return {
    title: `Conformance code ${suffix} ${stamp}`,
    code: `APPCT${suffix}${stamp}`,
    startsAt: '2026-05-05T00:00:00Z',
    functionHandle,
    discountClasses: ['ORDER'],
  };
}

function baseAutomaticInput(stamp: number, suffix: string, functionHandle: string): Record<string, unknown> {
  return {
    title: `Conformance automatic ${suffix} ${stamp}`,
    startsAt: '2026-05-05T00:00:00Z',
    functionHandle,
    discountClasses: ['ORDER'],
  };
}

function withInput(base: Record<string, unknown>, patch: Record<string, unknown>): Record<string, unknown> {
  return { ...base, ...patch };
}

async function runCase(
  documentPath: string,
  document: string,
  variables: Record<string, unknown>,
): Promise<{ request: { documentPath: string; variables: Record<string, unknown> }; response: unknown }> {
  const response = await runGraphqlRaw(document, variables);
  assertHttpOk(response, documentPath);
  return {
    request: {
      documentPath,
      variables,
    },
    response: response.payload,
  };
}

async function cleanupDiscounts(codeIds: string[], automaticIds: string[]): Promise<unknown[]> {
  const cleanup: unknown[] = [];
  for (const codeId of new Set(codeIds)) {
    const result = await runGraphqlRequest(deleteCodeDocument, { id: codeId });
    cleanup.push({ kind: 'code', id: codeId, response: result.payload });
  }
  for (const automaticId of new Set(automaticIds)) {
    const result = await runGraphqlRequest(deleteAutomaticDocument, { id: automaticId });
    cleanup.push({ kind: 'automatic', id: automaticId, response: result.payload });
  }
  return cleanup;
}

function readCodeDiscountId(response: unknown, alias: string): string | null {
  return readString(readPath(response, ['data', alias, 'codeAppDiscount', 'discountId']));
}

function readAutomaticDiscountId(response: unknown, alias: string): string | null {
  return readString(readPath(response, ['data', alias, 'automaticAppDiscount', 'discountId']));
}

await mkdir(outputDir, { recursive: true });

const scopeProbe = await probeDiscountConformanceScopes(adminOptions);
assertDiscountConformanceScopes(scopeProbe);

const functionCatalog = await runGraphqlRequest(functionCatalogDocument, {});
assertHttpOk(functionCatalog, 'shopifyFunctions catalog');
const discountFunction = findDiscountFunction(readFunctionNodes(functionCatalog));
const functionHandle = readString(discountFunction['handle']);
if (!functionHandle) {
  throw new Error(`Discount Function is missing a handle: ${JSON.stringify(discountFunction, null, 2)}`);
}

const stamp = Date.now();
const longTitle = 'x'.repeat(256);
const setupVariables = {
  codeInput: validCodeInput(stamp, functionHandle),
  automaticInput: validAutomaticInput(stamp, functionHandle),
};
const setup = await runCase(setupDocumentPath, setupDocument, setupVariables);
const codeSetupId = readCodeDiscountId(setup.response, 'codeSetup');
const automaticSetupId = readAutomaticDiscountId(setup.response, 'automaticSetup');
if (!codeSetupId || !automaticSetupId) {
  throw new Error(`Setup did not create both app discounts: ${JSON.stringify(setup.response, null, 2)}`);
}

const cleanupCodeIds = [codeSetupId];
const cleanupAutomaticIds = [automaticSetupId];
let create: { request: { documentPath: string; variables: Record<string, unknown> }; response: unknown } | null = null;
let update: { request: { documentPath: string; variables: Record<string, unknown> }; response: unknown } | null = null;
let cleanup: unknown[] = [];

try {
  create = await runCase(createDocumentPath, createDocument, {
    blank: withInput(baseCodeInput(stamp, 'BLANK', functionHandle), { title: '' }),
    missing: withInput(baseCodeInput(stamp, 'MISSING', functionHandle), { title: undefined }),
    tooLong: withInput(baseCodeInput(stamp, 'TOOLONG', functionHandle), { title: longTitle }),
    automaticBlank: withInput(baseAutomaticInput(stamp, 'AUTOBLANK', functionHandle), { title: '' }),
  });
  for (const alias of ['blankTitle', 'missingTitle', 'tooLongTitle']) {
    const id = readCodeDiscountId(create.response, alias);
    if (id) {
      cleanupCodeIds.push(id);
    }
  }
  const automaticCreateId = readAutomaticDiscountId(create.response, 'automaticBlankTitle');
  if (automaticCreateId) {
    cleanupAutomaticIds.push(automaticCreateId);
  }

  update = await runCase(updateDocumentPath, updateDocument, {
    codeId: codeSetupId,
    automaticId: automaticSetupId,
    blank: withInput(baseCodeInput(stamp, 'UPBLANK', functionHandle), { title: '' }),
    missing: withInput(baseCodeInput(stamp, 'UPMISSING', functionHandle), { title: undefined }),
    tooLong: withInput(baseCodeInput(stamp, 'UPTOOLONG', functionHandle), { title: longTitle }),
    automaticBlank: withInput(baseAutomaticInput(stamp, 'UPAUTOBLANK', functionHandle), { title: '' }),
  });
} finally {
  cleanup = await cleanupDiscounts(cleanupCodeIds, cleanupAutomaticIds);
}

if (!create || !update) {
  throw new Error('Capture did not complete create and update probes.');
}

const output = {
  scenarioId: 'discount-code-app-title',
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  scopeProbe,
  functionCatalog: {
    query: functionCatalogDocument,
    variables: {},
    response: functionCatalog,
  },
  discountFunction,
  setup,
  create,
  update,
  cleanup,
  upstreamCalls: [captureHydrateCall(functionHandle, discountFunction)],
  notes:
    'Live Shopify app-managed code discount title capture using a deployed disposable conformance-discount Function. Code-app create rejects blank, omitted, and 256-character titles; code-app update accepts omitted title only. Automatic-app blank title is retained as the control validation error.',
};

await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');
console.log(`Wrote ${outputPath}`);

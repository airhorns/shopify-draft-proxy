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
const outputPath = path.join(outputDir, 'discount-app-function-validation.json');
const documentPath = 'config/parity-requests/discounts/discount-app-function-validation.graphql';
const document = await readFile(documentPath, 'utf8');
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const adminOptions = {
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
};
const { runGraphqlRequest } = createAdminGraphqlClient(adminOptions);

const functionCatalogDocument = `#graphql
  query DiscountAppFunctionValidationCatalog {
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

function readRecord(value: unknown): JsonRecord | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function readString(value: unknown): string | null {
  return typeof value === 'string' && value.length > 0 ? value : null;
}

function assertNoTopLevelErrors(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300 || readRecord(result.payload)?.['errors']) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function readFunctionNodes(catalog: ConformanceGraphqlResult): JsonRecord[] {
  const data = readRecord(readRecord(catalog.payload)?.['data']);
  const connection = readRecord(data?.['shopifyFunctions']);
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

function findWrongApiFunction(nodes: JsonRecord[]): JsonRecord {
  const wrongApiNodes = nodes.filter((candidate) => {
    const handle = readString(candidate['handle']);
    const apiType = readString(candidate['apiType']);
    return Boolean(handle && apiType && !isDiscountApi(apiType));
  });
  const node = wrongApiNodes.find((candidate) => candidate['apiType'] === 'cart_transform') ?? wrongApiNodes[0];
  if (!node) {
    throw new Error(
      `Expected at least one released non-discount Shopify Function for wrong-API validation: ${JSON.stringify(
        nodes,
        null,
        2,
      )}`,
    );
  }

  return node;
}

function captureHydrateCall(handle: string, nodes: JsonRecord[]): CaptureCall {
  return {
    operationName: 'ShopifyFunctionByHandle',
    variables: { handle },
    query: functionHydrateByHandleDocument,
    response: {
      status: 200,
      body: {
        data: {
          shopifyFunctions: {
            nodes,
          },
        },
      },
    },
  };
}

function assertValidationOnlyResponse(result: ConformanceGraphqlResult): void {
  const data = readRecord(readRecord(result.payload)?.['data']);
  if (!data) {
    throw new Error(`Validation capture returned no data: ${JSON.stringify(result, null, 2)}`);
  }

  for (const [alias, payload] of Object.entries(data)) {
    const record = readRecord(payload);
    const userErrors = readArray(record?.['userErrors']);
    if (!record || userErrors.length !== 1) {
      throw new Error(`${alias} did not return exactly one userError: ${JSON.stringify(payload, null, 2)}`);
    }
  }
}

await mkdir(outputDir, { recursive: true });

const scopeProbe = await probeDiscountConformanceScopes(adminOptions);
assertDiscountConformanceScopes(scopeProbe);

const functionCatalog = await runGraphqlRequest(functionCatalogDocument, {});
assertNoTopLevelErrors(functionCatalog, 'shopifyFunctions catalog probe');
const functionNodes = readFunctionNodes(functionCatalog);
const wrongApiFunction = findWrongApiFunction(functionNodes);
const wrongApiHandle = readString(wrongApiFunction['handle']);
if (!wrongApiHandle) {
  throw new Error(`Wrong-API Function is missing handle: ${JSON.stringify(wrongApiFunction, null, 2)}`);
}

const stamp = Date.now();
const unknownFunctionId = '00000000-0000-0000-0000-000000000000';
const unknownFunctionHandle = 'har-783-uninstalled-handle';
const variables = {
  missingCode: {
    title: 'HAR-783 Missing Code',
    code: `HAR783MISS${stamp}`,
    startsAt: '2026-04-25T00:00:00Z',
  },
  multipleCode: {
    title: 'HAR-783 Multiple Code',
    code: `HAR783MULT${stamp}`,
    startsAt: '2026-04-25T00:00:00Z',
    functionId: unknownFunctionId,
    functionHandle: wrongApiHandle,
  },
  unknownIdCode: {
    title: 'HAR-783 Unknown Code',
    code: `HAR783UNK${stamp}`,
    startsAt: '2026-04-25T00:00:00Z',
    functionId: unknownFunctionId,
  },
  wrongApiCode: {
    title: 'HAR-783 Wrong API Code',
    code: `HAR783WRONG${stamp}`,
    startsAt: '2026-04-25T00:00:00Z',
    functionHandle: wrongApiHandle,
  },
  missingAutomatic: {
    title: 'HAR-783 Missing Auto',
    startsAt: '2026-04-25T00:00:00Z',
  },
  unknownHandleAutomatic: {
    title: 'HAR-783 Unknown Auto',
    startsAt: '2026-04-25T00:00:00Z',
    functionHandle: unknownFunctionHandle,
  },
};

const validation = await runGraphqlRequest(document, variables);
assertNoTopLevelErrors(validation, 'app-discount Function validation capture');
assertValidationOnlyResponse(validation);

const output = {
  scenarioId: 'discount-app-function-validation',
  capturedAt: new Date().toISOString(),
  source: storeDomain,
  apiVersion,
  scopeProbe,
  functionCatalog: {
    query: functionCatalogDocument,
    variables: {},
    response: functionCatalog,
  },
  validation: {
    documentPath,
    variables,
    response: validation,
  },
  upstreamCalls: [
    captureHydrateCall(unknownFunctionId, []),
    captureHydrateCall(wrongApiHandle, [wrongApiFunction]),
    captureHydrateCall(unknownFunctionHandle, []),
  ],
  notes:
    'HAR-783 validation-only capture. The conformance app has no released discount Function, but its released cart-transform Function proves the wrong-API branch without creating app discounts.',
};

await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');
console.log(`Wrote ${outputPath}`);

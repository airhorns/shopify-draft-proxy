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
const outputPath = path.join(outputDir, 'discount-title-validation.json');

const setupDocumentPath = 'config/parity-requests/discounts/discount-title-validation-setup.graphql';
const createDocumentPath = 'config/parity-requests/discounts/discount-title-validation-create.graphql';
const updateDocumentPath = 'config/parity-requests/discounts/discount-title-validation-update.graphql';

const setupDocument = await readFile(setupDocumentPath, 'utf8');
const createDocument = await readFile(createDocumentPath, 'utf8');
const updateDocument = await readFile(updateDocumentPath, 'utf8');

const productProbeDocument = `#graphql
  query DiscountTitleValidationProducts {
    products(first: 1) {
      nodes {
        id
      }
    }
  }
`;

const discountUniquenessQuery = `#graphql
  query DiscountUniquenessCheck($code: String!) {
    codeDiscountNodeByCode(code: $code) {
      id
    }
  }
`;

const functionCatalogDocument = `#graphql
  query DiscountTitleValidationFunctionCatalog {
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
  mutation DiscountTitleValidationDeleteCode($id: ID!) {
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
  mutation DiscountTitleValidationDeleteAutomatic($id: ID!) {
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
    if (Array.isArray(current)) {
      const index = Number.parseInt(segment, 10);
      if (!Number.isInteger(index) || index < 0) {
        return null;
      }
      current = current[index];
      continue;
    }
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

function readProductId(response: ConformanceGraphqlResult): string {
  const id = readString(readPath(response.payload, ['data', 'products', 'nodes', '0', 'id']));
  if (!id) {
    throw new Error(
      `Discount title validation capture requires at least one product in the test shop: ${JSON.stringify(response.payload, null, 2)}`,
    );
  }

  return id;
}

async function readProductsHydrateNodesQuery(): Promise<string> {
  const source = await readFile('src/shopify_draft_proxy/proxy/products/products_core.gleam', 'utf8');
  const match = source.match(/pub const product_hydrate_nodes_query: String = "\n([\s\S]*?)\n"\n\n@internal/);
  if (!match) {
    throw new Error('Unable to read product_hydrate_nodes_query from products_core.gleam');
  }
  const query = match[1];
  if (query === undefined) {
    throw new Error('Unable to read product_hydrate_nodes_query body from products_core.gleam');
  }

  return query.replace(/\\"/g, '"');
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

function captureRawCall(
  operationName: string,
  variables: Record<string, unknown>,
  query: string,
  response: ConformanceGraphqlResult,
): CaptureCall {
  return {
    operationName,
    variables,
    query,
    response: {
      status: response.status,
      body: response.payload,
    },
  };
}

function withTitle(input: Record<string, unknown>, title: string): Record<string, unknown> {
  return { ...input, title };
}

function basicInput(title: string, code: string): Record<string, unknown> {
  return {
    title,
    code,
    startsAt: '2026-05-05T00:00:00Z',
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
  };
}

function automaticBasicInput(title: string): Record<string, unknown> {
  return {
    title,
    startsAt: '2026-05-05T00:00:00Z',
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
  };
}

function bxgyInput(title: string, code: string | null, productId: string): Record<string, unknown> {
  return {
    title,
    ...(code === null ? {} : { code }),
    startsAt: '2026-05-05T00:00:00Z',
    combinesWith: {
      productDiscounts: true,
      orderDiscounts: false,
      shippingDiscounts: false,
    },
    context: {
      all: 'ALL',
    },
    customerBuys: {
      value: {
        quantity: '1',
      },
      items: {
        products: {
          productsToAdd: [productId],
        },
      },
    },
    customerGets: {
      value: {
        discountOnQuantity: {
          quantity: '1',
          effect: {
            percentage: 1,
          },
        },
      },
      items: {
        products: {
          productsToAdd: [productId],
        },
      },
    },
  };
}

function freeShippingInput(title: string, code: string | null): Record<string, unknown> {
  return {
    title,
    ...(code === null ? {} : { code }),
    startsAt: '2026-05-05T00:00:00Z',
    combinesWith: {
      productDiscounts: false,
      orderDiscounts: false,
      shippingDiscounts: false,
    },
    context: {
      all: 'ALL',
    },
    destination: {
      all: true,
    },
  };
}

function appInput(title: string, code: string | null, functionHandle: string): Record<string, unknown> {
  return {
    title,
    ...(code === null ? {} : { code }),
    startsAt: '2026-05-05T00:00:00Z',
    functionHandle,
    discountClasses: ['ORDER'],
  };
}

function setupVariables(stamp: number, productId: string, functionHandle: string): Record<string, unknown> {
  return {
    codeBasic: basicInput(`Conformance title code basic setup ${stamp}`, `TITLEBASIC${stamp}`),
    codeBxgy: bxgyInput(`Conformance title code bxgy setup ${stamp}`, `TITLEBXGY${stamp}`, productId),
    codeFreeShipping: freeShippingInput(`Conformance title code shipping setup ${stamp}`, `TITLESHIP${stamp}`),
    automaticBasic: automaticBasicInput(`Conformance title automatic basic setup ${stamp}`),
    automaticBxgy: bxgyInput(`Conformance title automatic bxgy setup ${stamp}`, null, productId),
    automaticFreeShipping: freeShippingInput(`Conformance title automatic shipping setup ${stamp}`, null),
    codeApp: appInput(`Conformance title code app setup ${stamp}`, `TITLEAPP${stamp}`, functionHandle),
    automaticApp: appInput(`Conformance title automatic app setup ${stamp}`, null, functionHandle),
  };
}

function createVariables(
  stamp: number,
  productId: string,
  functionHandle: string,
  tooLongTitle: string,
): Record<string, unknown> {
  const base = setupVariables(stamp, productId, functionHandle);

  return {
    codeBasicBlank: withTitle(base['codeBasic'] as Record<string, unknown>, ''),
    codeBasicTooLong: withTitle(base['codeBasic'] as Record<string, unknown>, tooLongTitle),
    codeBxgyBlank: withTitle(base['codeBxgy'] as Record<string, unknown>, ''),
    codeBxgyTooLong: withTitle(base['codeBxgy'] as Record<string, unknown>, tooLongTitle),
    codeFreeShippingBlank: withTitle(base['codeFreeShipping'] as Record<string, unknown>, ''),
    codeFreeShippingTooLong: withTitle(base['codeFreeShipping'] as Record<string, unknown>, tooLongTitle),
    automaticBasicBlank: withTitle(base['automaticBasic'] as Record<string, unknown>, ''),
    automaticBasicTooLong: withTitle(base['automaticBasic'] as Record<string, unknown>, tooLongTitle),
    automaticBxgyBlank: withTitle(base['automaticBxgy'] as Record<string, unknown>, ''),
    automaticBxgyTooLong: withTitle(base['automaticBxgy'] as Record<string, unknown>, tooLongTitle),
    automaticFreeShippingBlank: withTitle(base['automaticFreeShipping'] as Record<string, unknown>, ''),
    automaticFreeShippingTooLong: withTitle(base['automaticFreeShipping'] as Record<string, unknown>, tooLongTitle),
    codeAppBlank: withTitle(base['codeApp'] as Record<string, unknown>, ''),
    codeAppTooLong: withTitle(base['codeApp'] as Record<string, unknown>, tooLongTitle),
    automaticAppBlank: withTitle(base['automaticApp'] as Record<string, unknown>, ''),
    automaticAppTooLong: withTitle(base['automaticApp'] as Record<string, unknown>, tooLongTitle),
  };
}

function updateVariables(ids: Record<string, string>, invalidInputs: Record<string, unknown>): Record<string, unknown> {
  return {
    codeBasicId: ids['codeBasic'],
    codeBxgyId: ids['codeBxgy'],
    codeFreeShippingId: ids['codeFreeShipping'],
    automaticBasicId: ids['automaticBasic'],
    automaticBxgyId: ids['automaticBxgy'],
    automaticFreeShippingId: ids['automaticFreeShipping'],
    codeAppId: ids['codeApp'],
    automaticAppId: ids['automaticApp'],
    ...invalidInputs,
  };
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

function readSetupIds(response: unknown): Record<string, string> {
  const ids: Record<string, string> = {};
  const codePaths: Array<[string, string, string]> = [
    ['codeBasic', 'codeDiscountNode', 'id'],
    ['codeBxgy', 'codeDiscountNode', 'id'],
    ['codeFreeShipping', 'codeDiscountNode', 'id'],
  ];
  const automaticPaths: Array<[string, string, string]> = [
    ['automaticBasic', 'automaticDiscountNode', 'id'],
    ['automaticBxgy', 'automaticDiscountNode', 'id'],
    ['automaticFreeShipping', 'automaticDiscountNode', 'id'],
  ];
  for (const [alias, nodeKey, idKey] of codePaths) {
    const id = readString(readPath(response, ['data', alias, nodeKey, idKey]));
    if (id) {
      ids[alias] = id;
    }
  }
  for (const [alias, nodeKey, idKey] of automaticPaths) {
    const id = readString(readPath(response, ['data', alias, nodeKey, idKey]));
    if (id) {
      ids[alias] = id;
    }
  }
  const codeAppId = readString(readPath(response, ['data', 'codeApp', 'codeAppDiscount', 'discountId']));
  const automaticAppId = readString(readPath(response, ['data', 'automaticApp', 'automaticAppDiscount', 'discountId']));
  if (codeAppId) {
    ids['codeApp'] = codeAppId;
  }
  if (automaticAppId) {
    ids['automaticApp'] = automaticAppId;
  }

  return ids;
}

function assertAllSetupIds(ids: Record<string, string>, response: unknown): void {
  const expected = [
    'codeBasic',
    'codeBxgy',
    'codeFreeShipping',
    'automaticBasic',
    'automaticBxgy',
    'automaticFreeShipping',
    'codeApp',
    'automaticApp',
  ];
  const missing = expected.filter((key) => !ids[key]);
  if (missing.length > 0) {
    throw new Error(`Setup did not create all discounts (${missing.join(', ')}): ${JSON.stringify(response, null, 2)}`);
  }
}

function collectCreatedDiscounts(call: unknown, codeIds: string[], automaticIds: string[]): void {
  const response = readRecord(call)?.['response'] ?? call;
  const codeAliases = [
    'codeBasic',
    'codeBxgy',
    'codeFreeShipping',
    'codeApp',
    'codeBasicBlank',
    'codeBasicTooLong',
    'codeBxgyBlank',
    'codeBxgyTooLong',
    'codeFreeShippingBlank',
    'codeFreeShippingTooLong',
    'codeAppBlank',
    'codeAppTooLong',
  ];
  const automaticAliases = [
    'automaticBasic',
    'automaticBxgy',
    'automaticFreeShipping',
    'automaticApp',
    'automaticBasicBlank',
    'automaticBasicTooLong',
    'automaticBxgyBlank',
    'automaticBxgyTooLong',
    'automaticFreeShippingBlank',
    'automaticFreeShippingTooLong',
    'automaticAppBlank',
    'automaticAppTooLong',
  ];

  for (const alias of codeAliases) {
    const nodeId =
      readString(readPath(response, ['data', alias, 'codeDiscountNode', 'id'])) ||
      readString(readPath(response, ['data', alias, 'codeAppDiscount', 'discountId']));
    if (nodeId) {
      codeIds.push(nodeId);
    }
  }
  for (const alias of automaticAliases) {
    const nodeId =
      readString(readPath(response, ['data', alias, 'automaticDiscountNode', 'id'])) ||
      readString(readPath(response, ['data', alias, 'automaticAppDiscount', 'discountId']));
    if (nodeId) {
      automaticIds.push(nodeId);
    }
  }
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

await mkdir(outputDir, { recursive: true });

const scopeProbe = await probeDiscountConformanceScopes(adminOptions);
assertDiscountConformanceScopes(scopeProbe);

const productProbe = await runGraphqlRaw(productProbeDocument, {});
assertHttpOk(productProbe, 'products probe');
const productId = readProductId(productProbe);

const functionCatalog = await runGraphqlRequest(functionCatalogDocument, {});
assertHttpOk(functionCatalog, 'shopifyFunctions catalog');
const discountFunction = findDiscountFunction(readFunctionNodes(functionCatalog));
const functionHandle = readString(discountFunction['handle']);
if (!functionHandle) {
  throw new Error(`Discount Function is missing a handle: ${JSON.stringify(discountFunction, null, 2)}`);
}

const stamp = Date.now();
const setupInputVariables = setupVariables(stamp, productId, functionHandle);
const invalidInputs = createVariables(stamp + 1, productId, functionHandle, 'T'.repeat(256));
const productsHydrateNodesQuery = await readProductsHydrateNodesQuery();
const productHydrateVariables = { ids: [productId] };
const productHydrateResponse = await runGraphqlRaw(productsHydrateNodesQuery, productHydrateVariables);
assertHttpOk(productHydrateResponse, 'ProductsHydrateNodes cassette');
const setupCodeInputs = [
  setupInputVariables['codeBasic'],
  setupInputVariables['codeBxgy'],
  setupInputVariables['codeFreeShipping'],
  setupInputVariables['codeApp'],
].flatMap((input) => {
  const code = readString(readRecord(input)?.['code']);
  return code ? [code] : [];
});
const uniquenessResponses = await Promise.all(
  setupCodeInputs.map(async (code) => ({
    code,
    response: await runGraphqlRaw(discountUniquenessQuery, { code }),
  })),
);
const cleanupCodeIds: string[] = [];
const cleanupAutomaticIds: string[] = [];
let setup: unknown = null;
let create: unknown = null;
let update: unknown = null;
let cleanup: unknown[] = [];

try {
  setup = await runCase(setupDocumentPath, setupDocument, setupInputVariables);
  collectCreatedDiscounts(setup, cleanupCodeIds, cleanupAutomaticIds);
  const ids = readSetupIds(readRecord(setup)?.['response']);
  assertAllSetupIds(ids, readRecord(setup)?.['response']);

  create = await runCase(createDocumentPath, createDocument, invalidInputs);
  collectCreatedDiscounts(create, cleanupCodeIds, cleanupAutomaticIds);
  update = await runCase(updateDocumentPath, updateDocument, updateVariables(ids, invalidInputs));
  collectCreatedDiscounts(update, cleanupCodeIds, cleanupAutomaticIds);
} finally {
  cleanup = await cleanupDiscounts(cleanupCodeIds, cleanupAutomaticIds);
}

const output = {
  scenarioId: 'discount-title-validation',
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  scopeProbe,
  productProbe: {
    query: productProbeDocument,
    variables: {},
    response: productProbe,
  },
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
  upstreamCalls: [
    captureRawCall('ProductsHydrateNodes', productHydrateVariables, productsHydrateNodesQuery, productHydrateResponse),
    ...uniquenessResponses.map(({ code, response }) =>
      captureRawCall('DiscountUniquenessCheck', { code }, discountUniquenessQuery, response),
    ),
    captureHydrateCall(functionHandle, discountFunction),
  ],
  notes:
    'Live Shopify title validation capture for discount create and update roots. Setup creates disposable code, automatic, and app-managed discounts, then cleanup deletes every created discount after blank-title and overlong-title validation probes.',
};

await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');
console.log(`Wrote ${outputPath}`);

/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
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
const outputPath = path.join(outputDir, 'discount-app-bulk-live-parity.json');
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const adminOptions = {
  adminOrigin,
  apiVersion,
  accessToken: adminAccessToken,
  headers: buildAdminAuthHeaders(adminAccessToken),
};
const { runGraphqlRaw } = createAdminGraphqlClient(adminOptions);

const requestPaths = {
  create: 'config/parity-requests/discounts/discount-app-bulk-live-create.graphql',
  update: 'config/parity-requests/discounts/discount-app-bulk-live-update.graphql',
  preconditions: 'config/parity-requests/discounts/discount-app-bulk-live-preconditions.graphql',
  bulkJobs: 'config/parity-requests/discounts/discount-app-bulk-live-jobs.graphql',
  downstreamRead: 'config/parity-requests/discounts/discount-app-bulk-live-read.graphql',
  deactivate: 'config/parity-requests/discounts/discount-app-automatic-live-deactivate.graphql',
  activate: 'config/parity-requests/discounts/discount-app-automatic-live-activate.graphql',
  deleteAutomatic: 'config/parity-requests/discounts/discount-app-automatic-live-delete.graphql',
  readAfterDelete: 'config/parity-requests/discounts/discount-app-automatic-live-read-after-delete.graphql',
} as const;

const documents = Object.fromEntries(
  await Promise.all(
    Object.entries(requestPaths).map(async ([key, requestPath]) => [key, await readFile(requestPath, 'utf8')] as const),
  ),
) as Record<keyof typeof requestPaths, string>;

const functionCatalogDocument = `#graphql
  query DiscountAppBulkLiveFunctionCatalog {
    shopifyFunctions(first: 50) {
      nodes {
        id
        title
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

const functionHydrateByIdDocument = `query ShopifyFunctionById($id: String!) {
  shopifyFunction(id: $id) {
    id
    title
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
`;

const cleanupCodeDocument = `#graphql
  mutation DiscountAppBulkLiveCleanupCode($id: ID!) {
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

const cleanupAutomaticDocument = `#graphql
  mutation DiscountAppBulkLiveCleanupAutomatic($id: ID!) {
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

function readRunId(): number {
  const raw = process.env['SHOPIFY_CONFORMANCE_RUN_ID'];
  if (!raw) {
    return Date.now();
  }

  const parsed = Number.parseInt(raw, 10);
  if (!Number.isFinite(parsed) || parsed <= 0) {
    throw new Error(`SHOPIFY_CONFORMANCE_RUN_ID must be a positive integer, got ${JSON.stringify(raw)}`);
  }
  return parsed;
}

function readRecord(value: unknown): JsonRecord | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function readString(value: unknown): string | null {
  return typeof value === 'string' && value.length > 0 ? value : null;
}

function readData(result: ConformanceGraphqlResult): JsonRecord {
  const payload = readRecord(result.payload);
  const data = readRecord(payload?.['data']);
  if (!data) {
    throw new Error(`GraphQL result did not include data: ${JSON.stringify(result, null, 2)}`);
  }
  return data;
}

function assertNoTopLevelErrors(result: ConformanceGraphqlResult, context: string): void {
  const payload = readRecord(result.payload);
  if (result.status < 200 || result.status >= 300 || payload?.['errors']) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertNoUserErrors(result: ConformanceGraphqlResult, context: string): void {
  const data = readData(result);
  for (const [alias, payload] of Object.entries(data)) {
    const record = readRecord(payload);
    const userErrors = readArray(record?.['userErrors']);
    if (userErrors.length > 0) {
      throw new Error(`${context}.${alias} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
    }
  }
}

function readFunctionNodes(catalog: ConformanceGraphqlResult): JsonRecord[] {
  const connection = readRecord(readData(catalog)['shopifyFunctions']);
  return readArray(connection?.['nodes']).flatMap((node) => {
    const record = readRecord(node);
    return record ? [record] : [];
  });
}

function findDiscountFunction(nodes: JsonRecord[]): JsonRecord {
  const node = nodes.find((candidate) => readString(candidate['apiType']) === 'discount');
  if (!node) {
    throw new Error(`Expected a released discount Shopify Function in the conformance app: ${JSON.stringify(nodes)}`);
  }
  return node;
}

function readPath(value: unknown, pathParts: string[]): unknown {
  let cursor = value;
  for (const part of pathParts) {
    if (cursor === null || typeof cursor !== 'object') {
      return undefined;
    }
    cursor = (cursor as Record<string, unknown>)[part];
  }
  return cursor;
}

function requireStringPath(value: unknown, pathParts: string[], context: string): string {
  const candidate = readPath(value, pathParts);
  if (typeof candidate !== 'string' || candidate.length === 0) {
    throw new Error(`${context} missing string at ${pathParts.join('.')}: ${JSON.stringify(value, null, 2)}`);
  }
  return candidate;
}

function captureHydrateCall(functionId: string, functionNode: JsonRecord): CaptureCall {
  return {
    operationName: 'ShopifyFunctionById',
    variables: { id: functionId },
    query: functionHydrateByIdDocument,
    response: {
      status: 200,
      body: {
        data: {
          shopifyFunction: functionNode,
        },
      },
    },
  };
}

function captureRecordedCall(
  operationName: string,
  query: string,
  variables: Record<string, unknown>,
  result: ConformanceGraphqlResult,
): CaptureCall {
  return {
    operationName,
    variables,
    query,
    response: {
      status: result.status,
      body: result.payload,
    },
  };
}

async function captureRequest(
  query: string,
  variables: Record<string, unknown>,
  context: string,
): Promise<ConformanceGraphqlResult> {
  const result = await runGraphqlRaw(query, variables);
  assertNoTopLevelErrors(result, context);
  return result;
}

async function cleanupCodeDiscount(id: string): Promise<ConformanceGraphqlResult> {
  return await runGraphqlRaw(cleanupCodeDocument, { id });
}

async function cleanupAutomaticDiscount(id: string): Promise<ConformanceGraphqlResult> {
  return await runGraphqlRaw(cleanupAutomaticDocument, { id });
}

await mkdir(outputDir, { recursive: true });

const scopeProbe = await probeDiscountConformanceScopes(adminOptions);
assertDiscountConformanceScopes(scopeProbe);

const functionCatalog = await captureRequest(functionCatalogDocument, {}, 'shopifyFunctions catalog');
const discountFunction = findDiscountFunction(readFunctionNodes(functionCatalog));
const functionId = requireStringPath(discountFunction, ['id'], 'discount Function');
const runId = readRunId();
const startsAt = new Date(Date.now() - 60_000).toISOString();
const codeDiscountIds = new Set<string>();
const automaticDiscountIds = new Set<string>();

function basicInput(title: string, code: string): JsonRecord {
  return {
    title,
    code,
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
    customerSelection: {
      all: true,
    },
  };
}

function automaticBasicInput(title: string): JsonRecord {
  return {
    title,
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
  };
}

const createVariables = {
  codeInput: {
    title: `SDP app bulk code ${runId}`,
    code: `SDPAPPCODE${runId}`,
    startsAt,
    functionId,
    discountClasses: ['ORDER'],
    usageLimit: 10,
    combinesWith: {
      orderDiscounts: true,
      productDiscounts: false,
      shippingDiscounts: true,
    },
  },
  automaticInput: {
    title: `SDP app bulk automatic ${runId}`,
    startsAt,
    functionId,
    discountClasses: ['ORDER'],
    recurringCycleLimit: 0,
  },
};

const preconditionVariables = {
  bulkInput: basicInput(`SDP bulk lifecycle ${runId}`, `SDPBULK${runId}`),
  redeemInput: basicInput(`SDP redeem target ${runId}`, `SDPREDEEM${runId}`),
  automaticInput: automaticBasicInput(`SDP automatic bulk ${runId}`),
};

let cleanup: unknown[] = [];

try {
  const create = await captureRequest(documents.create, createVariables, 'discount app bulk create');
  assertNoUserErrors(create, 'discount app bulk create');
  const createData = readData(create);
  const codeId = requireStringPath(
    createData,
    ['discountCodeAppCreate', 'codeAppDiscount', 'discountId'],
    'code app create',
  );
  const automaticId = requireStringPath(
    createData,
    ['discountAutomaticAppCreate', 'automaticAppDiscount', 'discountId'],
    'automatic app create',
  );
  codeDiscountIds.add(codeId);
  automaticDiscountIds.add(automaticId);

  const updateVariables = {
    codeId,
    codeInput: {
      title: `SDP app bulk code updated ${runId}`,
      code: `SDPAPPUP${runId}`,
      startsAt,
      functionId,
      discountClasses: ['ORDER'],
    },
    automaticId,
    automaticInput: {
      title: `SDP app bulk automatic updated ${runId}`,
      startsAt,
      functionId,
      discountClasses: ['ORDER'],
      recurringCycleLimit: 2,
    },
  };
  const update = await captureRequest(documents.update, updateVariables, 'discount app bulk update');
  assertNoUserErrors(update, 'discount app bulk update');

  const preconditions = await captureRequest(
    documents.preconditions,
    preconditionVariables,
    'discount app bulk preconditions',
  );
  assertNoUserErrors(preconditions, 'discount app bulk preconditions');
  const preconditionData = readData(preconditions);
  const bulkTargetId = requireStringPath(
    preconditionData,
    ['bulkTarget', 'codeDiscountNode', 'id'],
    'bulk target create',
  );
  const redeemTargetId = requireStringPath(
    preconditionData,
    ['redeemTarget', 'codeDiscountNode', 'id'],
    'redeem target create',
  );
  const automaticBulkTargetId = requireStringPath(
    preconditionData,
    ['automaticBulkTarget', 'automaticDiscountNode', 'id'],
    'automatic bulk target create',
  );
  codeDiscountIds.add(bulkTargetId);
  codeDiscountIds.add(redeemTargetId);
  automaticDiscountIds.add(automaticBulkTargetId);

  const downstreamReadVariables = {
    codeId,
    automaticId,
    deletedCodeId: 'gid://shopify/DiscountCodeNode/1',
    deletedAutomaticId: 'gid://shopify/DiscountAutomaticNode/1',
    redeemDiscountId: redeemTargetId,
    removedCode: `SDPUNKNOWN${runId}`,
  };
  const downstreamRead = await captureRequest(
    documents.downstreamRead,
    downstreamReadVariables,
    'discount app bulk downstream read',
  );

  const deactivateVariables = { id: automaticId };
  const deactivate = await captureRequest(documents.deactivate, deactivateVariables, 'automatic app deactivate');
  assertNoUserErrors(deactivate, 'automatic app deactivate');

  const activateVariables = { id: automaticId };
  const activate = await captureRequest(documents.activate, activateVariables, 'automatic app activate');
  assertNoUserErrors(activate, 'automatic app activate');

  const bulkJobVariables = {
    activateIds: [bulkTargetId],
    deactivateIds: [bulkTargetId],
    deleteCodeIds: [bulkTargetId],
    deleteAutomaticIds: [automaticBulkTargetId],
  };
  const bulkJobs = await captureRequest(documents.bulkJobs, bulkJobVariables, 'discount bulk jobs');
  assertNoUserErrors(bulkJobs, 'discount bulk jobs');

  const deleteAutomaticVariables = { id: automaticId };
  const deleteAutomatic = await captureRequest(
    documents.deleteAutomatic,
    deleteAutomaticVariables,
    'automatic app delete',
  );
  assertNoUserErrors(deleteAutomatic, 'automatic app delete');
  automaticDiscountIds.delete(automaticId);

  const readAfterDeleteVariables = { id: automaticId };
  const readAfterDelete = await captureRequest(
    documents.readAfterDelete,
    readAfterDeleteVariables,
    'automatic app read after delete',
  );

  for (const id of codeDiscountIds) cleanup.push(await cleanupCodeDiscount(id));
  for (const id of automaticDiscountIds) cleanup.push(await cleanupAutomaticDiscount(id));

  const output = {
    scenarioId: 'discount-app-bulk-live-parity',
    capturedAt: new Date().toISOString(),
    source: storeDomain,
    apiVersion,
    scopeProbe,
    functionCatalog: {
      query: functionCatalogDocument,
      variables: {},
      response: functionCatalog,
    },
    functionId,
    requests: {
      create: { query: documents.create, variables: createVariables },
      update: { query: documents.update, variables: updateVariables },
      preconditions: { query: documents.preconditions, variables: preconditionVariables },
      downstreamRead: { query: documents.downstreamRead, variables: downstreamReadVariables },
      deactivate: { query: documents.deactivate, variables: deactivateVariables },
      activate: { query: documents.activate, variables: activateVariables },
      bulkJobs: { query: documents.bulkJobs, variables: bulkJobVariables },
      deleteAutomatic: { query: documents.deleteAutomatic, variables: deleteAutomaticVariables },
      readAfterDelete: { query: documents.readAfterDelete, variables: readAfterDeleteVariables },
      cleanupCode: { query: cleanupCodeDocument },
      cleanupAutomatic: { query: cleanupAutomaticDocument },
    },
    create,
    update,
    preconditions,
    downstreamRead,
    deactivate,
    activate,
    bulkJobs,
    deleteAutomatic,
    readAfterDelete,
    cleanup,
    upstreamCalls: [
      captureHydrateCall(functionId, discountFunction),
      captureHydrateCall(functionId, discountFunction),
      captureRecordedCall('DiscountAppBulkLiveJobs', documents.bulkJobs, bulkJobVariables, bulkJobs),
    ],
    notes:
      'Live Shopify Admin evidence for app-managed code/automatic discount create, update, automatic lifecycle, downstream reads, and bulk job payloads using the released conformance discount Function. The non-subscription shop-gating and revoked-Function activation-failure branches remain covered by Rust route tests because this conformance shop sells subscriptions and the installed app exposes an available discount Function.',
  };

  await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');
  console.log(
    JSON.stringify(
      {
        ok: true,
        outputPath,
        functionId,
        codeId,
        automaticId,
        bulkTargetId,
        redeemTargetId,
        automaticBulkTargetId,
      },
      null,
      2,
    ),
  );
} catch (error) {
  for (const id of codeDiscountIds) {
    try {
      cleanup.push(await cleanupCodeDiscount(id));
    } catch (cleanupError) {
      cleanup.push({ cleanupError: cleanupError instanceof Error ? cleanupError.message : String(cleanupError), id });
    }
  }
  for (const id of automaticDiscountIds) {
    try {
      cleanup.push(await cleanupAutomaticDiscount(id));
    } catch (cleanupError) {
      cleanup.push({ cleanupError: cleanupError instanceof Error ? cleanupError.message : String(cleanupError), id });
    }
  }
  console.error(JSON.stringify({ ok: false, cleanup }, null, 2));
  throw error;
}

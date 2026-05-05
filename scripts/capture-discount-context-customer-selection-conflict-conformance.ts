/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { assertDiscountConformanceScopes, probeDiscountConformanceScopes } from './discount-conformance-lib.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

function assertNoTopLevelErrors(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function readPath(value: unknown, pathSegments: string[]): unknown {
  let current = value;
  for (const segment of pathSegments) {
    if (Array.isArray(current)) {
      const index = Number.parseInt(segment, 10);
      if (!Number.isInteger(index)) return undefined;
      current = current[index];
      continue;
    }
    if (current === null || typeof current !== 'object') return undefined;
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

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'discounts');
const outputPath = path.join(outputDir, 'discount-context-customer-selection-conflict.json');
const setupDocument = await readFile(
  'config/parity-requests/discounts/discount-context-customer-selection-conflict-setup.graphql',
  'utf8',
);
const conflictCreateDocument = await readFile(
  'config/parity-requests/discounts/discount-context-customer-selection-conflict.graphql',
  'utf8',
);
const basicUpdateDocument = await readFile(
  'config/parity-requests/discounts/discount-context-customer-selection-conflict-basic-update.graphql',
  'utf8',
);
const bxgyUpdateDocument = await readFile(
  'config/parity-requests/discounts/discount-context-customer-selection-conflict-bxgy-update.graphql',
  'utf8',
);
const freeShippingUpdateDocument = await readFile(
  'config/parity-requests/discounts/discount-context-customer-selection-conflict-free-shipping-update.graphql',
  'utf8',
);
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

const runId = Date.now();
const startsAt = '2026-04-25T00:00:00Z';
const cleanup: JsonRecord = {};
const setupDiscountIds: string[] = [];
const setupCustomerIds: string[] = [];

const productProbeDocument = `#graphql
  query DiscountConflictProductProbe {
    products(first: 1) {
      nodes {
        id
      }
    }
  }
`;
const customerCreateDocument = `#graphql
  mutation DiscountConflictCustomerCreate($input: CustomerInput!) {
    customerCreate(input: $input) {
      customer {
        id
        displayName
        email
      }
      userErrors {
        field
        message
      }
    }
  }
`;
const customerDeleteDocument = `#graphql
  mutation DiscountConflictCustomerDelete($input: CustomerDeleteInput!) {
    customerDelete(input: $input) {
      deletedCustomerId
      userErrors {
        field
        message
      }
    }
  }
`;
const discountDeleteDocument = `#graphql
  mutation DiscountConflictDiscountDelete($id: ID!) {
    discountCodeDelete(id: $id) {
      deletedCodeDiscountId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

function customerInput(index: number): JsonRecord {
  return {
    firstName: 'HAR-780',
    lastName: `Conflict ${index}`,
    email: `har780-discount-conflict-${runId}-${index}@example.com`,
    tags: ['har-780', `har-780-${runId}`],
  };
}

function buyerConflict(customerAId: string, customerBId: string): JsonRecord {
  return {
    context: {
      customers: {
        add: [customerAId],
      },
    },
    customerSelection: {
      customers: {
        add: [customerBId],
      },
    },
  };
}

function basicInput(code: string, customerAId?: string, customerBId?: string): JsonRecord {
  return {
    title: `HAR-780 basic ${code}`,
    code,
    startsAt,
    ...(customerAId && customerBId ? buyerConflict(customerAId, customerBId) : { context: { all: 'ALL' } }),
    customerGets: {
      value: { percentage: 0.1 },
      items: { all: true },
    },
  };
}

function bxgyInput(code: string, productId: string, customerAId?: string, customerBId?: string): JsonRecord {
  return {
    title: `HAR-780 bxgy ${code}`,
    code,
    startsAt,
    ...(customerAId && customerBId ? buyerConflict(customerAId, customerBId) : { context: { all: 'ALL' } }),
    customerBuys: {
      value: { quantity: '1' },
      items: { products: { productsToAdd: [productId] } },
    },
    customerGets: {
      value: {
        discountOnQuantity: {
          quantity: '1',
          effect: { percentage: 1 },
        },
      },
      items: { products: { productsToAdd: [productId] } },
    },
  };
}

function freeShippingInput(code: string, customerAId?: string, customerBId?: string): JsonRecord {
  return {
    title: `HAR-780 shipping ${code}`,
    code,
    startsAt,
    ...(customerAId && customerBId ? buyerConflict(customerAId, customerBId) : { context: { all: 'ALL' } }),
    destination: { all: true },
  };
}

function appCodeInput(code: string, customerAId: string, customerBId: string): JsonRecord {
  return {
    title: `HAR-780 app ${code}`,
    code,
    startsAt,
    ...buyerConflict(customerAId, customerBId),
    functionHandle: 'discount-local',
  };
}

function readSetupDiscountId(response: ConformanceGraphqlResult, alias: string): string {
  return readRequiredString(response, ['data', alias, 'codeDiscountNode', 'id'], `${alias} setup`);
}

try {
  const productProbe = await runGraphqlRaw(productProbeDocument, {});
  assertNoTopLevelErrors(productProbe, 'product probe');
  const productId = readRequiredString(productProbe, ['data', 'products', 'nodes', '0', 'id'], 'product probe');

  const customerACreate = await runGraphqlRaw(customerCreateDocument, { input: customerInput(1) });
  assertNoTopLevelErrors(customerACreate, 'customerCreate A setup');
  const customerAId = readRequiredString(customerACreate, ['data', 'customerCreate', 'customer', 'id'], 'customer A');
  setupCustomerIds.push(customerAId);

  const customerBCreate = await runGraphqlRaw(customerCreateDocument, { input: customerInput(2) });
  assertNoTopLevelErrors(customerBCreate, 'customerCreate B setup');
  const customerBId = readRequiredString(customerBCreate, ['data', 'customerCreate', 'customer', 'id'], 'customer B');
  setupCustomerIds.push(customerBId);

  const setupBasicCode = `HAR780SETB${runId}`;
  const setupBxgyCode = `HAR780SETX${runId}`;
  const setupFreeShippingCode = `HAR780SETS${runId}`;
  const setupVariables = {
    basicCode: basicInput(setupBasicCode),
    bxgyCode: bxgyInput(setupBxgyCode, productId),
    freeShippingCode: freeShippingInput(setupFreeShippingCode),
  };
  const setup = await runGraphqlRaw(setupDocument, setupVariables);
  assertNoTopLevelErrors(setup, 'setup discount create');
  setupDiscountIds.push(readSetupDiscountId(setup, 'basicCode'));
  setupDiscountIds.push(readSetupDiscountId(setup, 'bxgyCode'));
  setupDiscountIds.push(readSetupDiscountId(setup, 'freeShippingCode'));

  const conflictCreateVariables = {
    basicCode: basicInput(`HAR780BASIC${runId}`, customerAId, customerBId),
    bxgyCode: bxgyInput(`HAR780BXGY${runId}`, productId, customerAId, customerBId),
    freeShippingCode: freeShippingInput(`HAR780SHIP${runId}`, customerAId, customerBId),
    appCode: appCodeInput(`HAR780APP${runId}`, customerAId, customerBId),
  };
  const conflictCreate = await runGraphqlRaw(conflictCreateDocument, conflictCreateVariables);
  assertNoTopLevelErrors(conflictCreate, 'context/customerSelection conflict create capture');

  const basicUpdateVariables = {
    id: setupDiscountIds[0],
    input: basicInput(setupBasicCode, customerAId, customerBId),
  };
  const bxgyUpdateVariables = {
    id: setupDiscountIds[1],
    input: bxgyInput(setupBxgyCode, productId, customerAId, customerBId),
  };
  const freeShippingUpdateVariables = {
    id: setupDiscountIds[2],
    input: freeShippingInput(setupFreeShippingCode, customerAId, customerBId),
  };
  const basicUpdate = await runGraphqlRaw(basicUpdateDocument, basicUpdateVariables);
  const bxgyUpdate = await runGraphqlRaw(bxgyUpdateDocument, bxgyUpdateVariables);
  const freeShippingUpdate = await runGraphqlRaw(freeShippingUpdateDocument, freeShippingUpdateVariables);
  assertNoTopLevelErrors(basicUpdate, 'basic conflict update capture');
  assertNoTopLevelErrors(bxgyUpdate, 'bxgy conflict update capture');
  assertNoTopLevelErrors(freeShippingUpdate, 'free-shipping conflict update capture');

  for (const [index, id] of setupDiscountIds.entries()) {
    cleanup[`discountDelete${index + 1}`] = await runGraphqlRaw(discountDeleteDocument, { id });
  }
  for (const [index, id] of [...setupCustomerIds].reverse().entries()) {
    cleanup[`customerDelete${index + 1}`] = await runGraphqlRaw(customerDeleteDocument, { input: { id } });
  }

  const fixture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    accessScopes: scopeProbe,
    setup: {
      query: setupDocument,
      variables: setupVariables,
      response: setup.payload,
      discountIds: setupDiscountIds,
      customerIds: setupCustomerIds,
      productId,
    },
    conflictCreate: {
      query: conflictCreateDocument,
      variables: conflictCreateVariables,
      response: conflictCreate.payload,
    },
    basicUpdate: {
      query: basicUpdateDocument,
      variables: basicUpdateVariables,
      response: basicUpdate.payload,
    },
    bxgyUpdate: {
      query: bxgyUpdateDocument,
      variables: bxgyUpdateVariables,
      response: bxgyUpdate.payload,
    },
    freeShippingUpdate: {
      query: freeShippingUpdateDocument,
      variables: freeShippingUpdateVariables,
      response: freeShippingUpdate.payload,
    },
    cleanup,
    upstreamCalls: [],
  };

  await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');

  console.log(
    JSON.stringify(
      {
        ok: true,
        apiVersion,
        outputPath,
        setupDiscountIds,
        setupCustomerIds,
      },
      null,
      2,
    ),
  );
} finally {
  for (const [index, id] of setupDiscountIds.entries()) {
    const key = `discountDelete${index + 1}`;
    if (!cleanup[key]) cleanup[`${key}AfterFailure`] = await runGraphqlRaw(discountDeleteDocument, { id });
  }
  for (const [index, id] of [...setupCustomerIds].reverse().entries()) {
    const key = `customerDelete${index + 1}`;
    if (!cleanup[key]) {
      cleanup[`${key}AfterFailure`] = await runGraphqlRaw(customerDeleteDocument, { input: { id } });
    }
  }
}

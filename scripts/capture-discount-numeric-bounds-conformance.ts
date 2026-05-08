/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { assertDiscountConformanceScopes, probeDiscountConformanceScopes } from './discount-conformance-lib.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type RawCase = {
  request: {
    documentPath: string;
    query: string;
    variables: Record<string, unknown>;
  };
  response: {
    status: number;
    payload: unknown;
  };
};

type CleanupStep = {
  label: string;
  run: () => Promise<ConformanceGraphqlResult<unknown>>;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'discounts');
const outputPath = path.join(outputDir, 'discount-numeric-bounds.json');

const setupDocumentPath = 'config/parity-requests/discounts/discount-numeric-bounds-setup.graphql';
const codeCreateDocumentPath = 'config/parity-requests/discounts/discount-numeric-bounds-code-basic-create.graphql';
const codeUpdateDocumentPath = 'config/parity-requests/discounts/discount-numeric-bounds-code-basic-update.graphql';
const automaticCreateDocumentPath =
  'config/parity-requests/discounts/discount-numeric-bounds-automatic-basic-create.graphql';
const automaticUpdateDocumentPath =
  'config/parity-requests/discounts/discount-numeric-bounds-automatic-basic-update.graphql';
const recurringFloatVariableDocumentPath =
  'config/parity-requests/discounts/discount-numeric-bounds-recurring-float-variable.graphql';
const recurringFloatLiteralDocumentPath =
  'config/parity-requests/discounts/discount-numeric-bounds-recurring-float-literal.graphql';

const deleteCodeDocument = `#graphql
  mutation DiscountNumericBoundsDeleteCode($id: ID!) {
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
  mutation DiscountNumericBoundsDeleteAutomatic($id: ID!) {
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
const { runGraphqlRaw } = createAdminGraphqlClient(adminOptions);

function percentageCustomerGets(): Record<string, unknown> {
  return {
    value: {
      percentage: 0.1,
    },
    items: {
      all: true,
    },
  };
}

function amountCustomerGets(amount: string): Record<string, unknown> {
  return {
    value: {
      discountAmount: {
        amount,
        appliesOnEachItem: false,
      },
    },
    items: {
      all: true,
    },
  };
}

function subscriptionPercentageCustomerGets(): Record<string, unknown> {
  return {
    appliesOnSubscription: true,
    appliesOnOneTimePurchase: false,
    ...percentageCustomerGets(),
  };
}

function codeBasicInput(stamp: number, suffix: string, extra: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    title: `Numeric bounds code ${suffix} ${stamp}`,
    code: `NUMBOUNDS${suffix}${stamp}`,
    startsAt: '2026-04-25T00:00:00Z',
    combinesWith: {
      productDiscounts: false,
      orderDiscounts: true,
      shippingDiscounts: false,
    },
    context: {
      all: 'ALL',
    },
    customerGets: percentageCustomerGets(),
    ...extra,
  };
}

function automaticBasicInput(
  stamp: number,
  suffix: string,
  extra: Record<string, unknown> = {},
): Record<string, unknown> {
  return {
    title: `Numeric bounds automatic ${suffix} ${stamp}`,
    startsAt: '2026-04-25T00:00:00Z',
    combinesWith: {
      productDiscounts: false,
      orderDiscounts: true,
      shippingDiscounts: false,
    },
    context: {
      all: 'ALL',
    },
    customerGets: {
      appliesOnSubscription: true,
      appliesOnOneTimePurchase: false,
      ...percentageCustomerGets(),
    },
    ...extra,
  };
}

function readRecord(value: unknown, pathParts: string[]): Record<string, unknown> | undefined {
  let current = value;
  for (const part of pathParts) {
    if (typeof current !== 'object' || current === null || Array.isArray(current)) {
      return undefined;
    }
    current = (current as Record<string, unknown>)[part];
  }
  return typeof current === 'object' && current !== null && !Array.isArray(current)
    ? (current as Record<string, unknown>)
    : undefined;
}

function readString(value: unknown, pathParts: string[]): string | undefined {
  const parent = readRecord(value, pathParts.slice(0, -1));
  const leaf = parent?.[pathParts[pathParts.length - 1] ?? ''];
  return typeof leaf === 'string' ? leaf : undefined;
}

function assertNoUserErrors(label: string, payload: unknown, root: string): void {
  const rootPayload = readRecord(payload, ['data', root]);
  const userErrors = rootPayload?.['userErrors'];
  if (Array.isArray(userErrors) && userErrors.length === 0) {
    return;
  }
  throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors)} payload=${JSON.stringify(payload)}`);
}

function assertPayloadPresent(label: string, payload: unknown, root: string): void {
  if (readRecord(payload, ['data', root]) !== undefined || readRecord(payload, ['errors', '0']) !== undefined) {
    return;
  }
  if (typeof payload === 'object' && payload !== null && 'errors' in payload) {
    return;
  }
  throw new Error(`${label} missing expected payload root/errors: ${JSON.stringify(payload)}`);
}

async function runCase(documentPath: string, variables: Record<string, unknown>): Promise<RawCase> {
  const query = await readFile(documentPath, 'utf8');
  const response = await runGraphqlRaw(query, variables);
  return {
    request: {
      documentPath,
      query,
      variables,
    },
    response: {
      status: response.status,
      payload: response.payload,
    },
  };
}

await mkdir(outputDir, { recursive: true });

const scopeProbe = await probeDiscountConformanceScopes(adminOptions);
assertDiscountConformanceScopes(scopeProbe);

const stamp = Date.now();
const cleanup: CleanupStep[] = [];
const cleanupResponses: Array<{ label: string; payload?: unknown; error?: string }> = [];
let setup: RawCase | undefined;

try {
  setup = await runCase(setupDocumentPath, {
    codeInput: codeBasicInput(stamp, 'SETUP'),
    automaticInput: automaticBasicInput(stamp, 'SETUP', { recurringCycleLimit: 2 }),
  });
  assertNoUserErrors('code setup create', setup.response.payload, 'codeSetup');
  assertNoUserErrors('automatic setup create', setup.response.payload, 'automaticSetup');

  const codeId = readString(setup.response.payload, ['data', 'codeSetup', 'codeDiscountNode', 'id']);
  if (codeId === undefined) {
    throw new Error(`code setup create did not return an id: ${JSON.stringify(setup.response.payload)}`);
  }
  cleanup.push({
    label: 'discountCodeDelete',
    run: () => runGraphqlRaw(deleteCodeDocument, { id: codeId }),
  });

  const automaticId = readString(setup.response.payload, ['data', 'automaticSetup', 'automaticDiscountNode', 'id']);
  if (automaticId === undefined) {
    throw new Error(`automatic setup create did not return an id: ${JSON.stringify(setup.response.payload)}`);
  }
  cleanup.push({
    label: 'discountAutomaticDelete',
    run: () => runGraphqlRaw(deleteAutomaticDocument, { id: automaticId }),
  });
} finally {
  // Validation cases are run after setup below; cleanup remains in the outer finally.
}

if (setup === undefined) {
  throw new Error('Capture did not complete setup.');
}

const codeId = readString(setup.response.payload, ['data', 'codeSetup', 'codeDiscountNode', 'id']);
const automaticId = readString(setup.response.payload, ['data', 'automaticSetup', 'automaticDiscountNode', 'id']);
if (codeId === undefined || automaticId === undefined) {
  throw new Error(`Setup ids missing: ${JSON.stringify(setup.response.payload)}`);
}

let codeCreate: RawCase | undefined;
let codeUpdate: RawCase | undefined;
let automaticCreate: RawCase | undefined;
let automaticUpdate: RawCase | undefined;
let recurringFloatVariable: RawCase | undefined;
let recurringFloatLiteral: RawCase | undefined;

try {
  codeCreate = await runCase(codeCreateDocumentPath, {
    usageHigh: codeBasicInput(stamp, 'CREATEUSAGEHIGH', { usageLimit: 2147483648 }),
    usageLow: codeBasicInput(stamp, 'CREATEUSAGELOW', { usageLimit: -2147483649 }),
    recurringHigh: codeBasicInput(stamp, 'CREATERECURRINGHIGH', {
      recurringCycleLimit: 2147483648,
      customerGets: subscriptionPercentageCustomerGets(),
    }),
    amountHigh: codeBasicInput(stamp, 'CREATEAMOUNTHIGH', {
      customerGets: amountCustomerGets('1000000000000000000'),
    }),
  });
  assertPayloadPresent('code create validation', codeCreate.response.payload, 'usageHigh');

  codeUpdate = await runCase(codeUpdateDocumentPath, {
    id: codeId,
    usageHigh: codeBasicInput(stamp, 'UPDATEUSAGEHIGH', { usageLimit: 2147483648 }),
    recurringHigh: codeBasicInput(stamp, 'UPDATERECURRINGHIGH', {
      recurringCycleLimit: 2147483648,
      customerGets: subscriptionPercentageCustomerGets(),
    }),
    amountHigh: codeBasicInput(stamp, 'UPDATEAMOUNTHIGH', {
      customerGets: amountCustomerGets('1000000000000000000'),
    }),
  });
  assertPayloadPresent('code update validation', codeUpdate.response.payload, 'usageHigh');

  automaticCreate = await runCase(automaticCreateDocumentPath, {
    recurringHigh: automaticBasicInput(stamp, 'CREATERECURRINGHIGH', { recurringCycleLimit: 2147483648 }),
    amountHigh: automaticBasicInput(stamp, 'CREATEAMOUNTHIGH', {
      customerGets: {
        appliesOnSubscription: true,
        appliesOnOneTimePurchase: false,
        ...amountCustomerGets('1000000000000000000'),
      },
    }),
  });
  assertPayloadPresent('automatic create validation', automaticCreate.response.payload, 'recurringHigh');

  automaticUpdate = await runCase(automaticUpdateDocumentPath, {
    id: automaticId,
    recurringHigh: automaticBasicInput(stamp, 'UPDATERECURRINGHIGH', { recurringCycleLimit: 2147483648 }),
    amountHigh: automaticBasicInput(stamp, 'UPDATEAMOUNTHIGH', {
      customerGets: {
        appliesOnSubscription: true,
        appliesOnOneTimePurchase: false,
        ...amountCustomerGets('1000000000000000000'),
      },
    }),
  });
  assertPayloadPresent('automatic update validation', automaticUpdate.response.payload, 'recurringHigh');

  recurringFloatVariable = await runCase(recurringFloatVariableDocumentPath, {
    input: automaticBasicInput(stamp, 'FLOATVARIABLE', { recurringCycleLimit: 1.5 }),
  });
  assertPayloadPresent('recurring float variable validation', recurringFloatVariable.response.payload, 'errors');

  recurringFloatLiteral = await runCase(recurringFloatLiteralDocumentPath, {});
  assertPayloadPresent('recurring float literal validation', recurringFloatLiteral.response.payload, 'errors');
} finally {
  for (const cleanupStep of cleanup.reverse()) {
    try {
      const response = await cleanupStep.run();
      cleanupResponses.push({ label: cleanupStep.label, payload: response.payload });
    } catch (error) {
      cleanupResponses.push({
        label: cleanupStep.label,
        error: error instanceof Error ? error.message : String(error),
      });
    }
  }
}

if (
  codeCreate === undefined ||
  codeUpdate === undefined ||
  automaticCreate === undefined ||
  automaticUpdate === undefined ||
  recurringFloatVariable === undefined ||
  recurringFloatLiteral === undefined
) {
  throw new Error('Capture did not complete all validation cases.');
}

const fixture = {
  scenarioId: 'discount-numeric-bounds',
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  accessScopes: scopeProbe,
  setup,
  cases: {
    codeCreate,
    codeUpdate,
    automaticCreate,
    automaticUpdate,
    recurringFloatVariable,
    recurringFloatLiteral,
  },
  cleanup: cleanupResponses,
  upstreamCalls: [],
};

await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');

console.log(
  JSON.stringify(
    {
      ok: true,
      apiVersion,
      outputPath,
      setupIds: {
        codeId,
        automaticId,
      },
      cleanup: cleanupResponses.map((item) => ({ label: item.label, ok: item.error === undefined })),
    },
    null,
    2,
  ),
);

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
const outputPath = path.join(outputDir, 'discount-automatic-value-bounds.json');

const setupDocumentPath = 'config/parity-requests/discounts/discount-automatic-value-bounds-setup.graphql';
const createDocumentPath = 'config/parity-requests/discounts/discount-automatic-value-bounds-create.graphql';
const updateDocumentPath = 'config/parity-requests/discounts/discount-automatic-value-bounds-update.graphql';

const deleteAutomaticDocument = `#graphql
  mutation DiscountAutomaticValueBoundsDelete($id: ID!) {
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

function automaticBasicInput(stamp: number, suffix: string, value: Record<string, unknown>): Record<string, unknown> {
  return {
    title: `Automatic value bounds ${suffix} ${stamp}`,
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
      value,
      items: {
        all: true,
      },
    },
  };
}

function amountValue(amount: string): Record<string, unknown> {
  return {
    discountAmount: {
      amount,
      appliesOnEachItem: false,
    },
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

function automaticIdsFromPayload(payload: unknown, roots: string[]): string[] {
  return roots.flatMap((root) => {
    const id = readString(payload, ['data', root, 'automaticDiscountNode', 'id']);
    return id === undefined ? [] : [id];
  });
}

await mkdir(outputDir, { recursive: true });

const scopeProbe = await probeDiscountConformanceScopes(adminOptions);
assertDiscountConformanceScopes(scopeProbe);

const stamp = Date.now();
const cleanup: CleanupStep[] = [];
const cleanupResponses: Array<{ label: string; payload?: unknown; error?: string }> = [];

let setup: RawCase | undefined;
let create: RawCase | undefined;
let update: RawCase | undefined;

try {
  setup = await runCase(setupDocumentPath, {
    input: automaticBasicInput(stamp, 'SETUP', { percentage: 0.1 }),
  });
  assertNoUserErrors('automatic setup create', setup.response.payload, 'discountAutomaticBasicCreate');

  const setupId = readString(setup.response.payload, [
    'data',
    'discountAutomaticBasicCreate',
    'automaticDiscountNode',
    'id',
  ]);
  if (setupId === undefined) {
    throw new Error(`automatic setup create did not return an id: ${JSON.stringify(setup.response.payload)}`);
  }

  cleanup.push({
    label: 'discountAutomaticDelete:setup',
    run: () => runGraphqlRaw(deleteAutomaticDocument, { id: setupId }),
  });

  create = await runCase(createDocumentPath, {
    percentageHigh: automaticBasicInput(stamp, 'CREATEPERCENTAGEHIGH', { percentage: 1.5 }),
    percentageNegative: automaticBasicInput(stamp, 'CREATEPERCENTAGENEGATIVE', { percentage: -0.1 }),
    percentageZero: automaticBasicInput(stamp, 'CREATEPERCENTAGEZERO', { percentage: 0 }),
    amountNegative: automaticBasicInput(stamp, 'CREATEAMOUNTNEGATIVE', amountValue('-1')),
    amountZero: automaticBasicInput(stamp, 'CREATEAMOUNTZERO', amountValue('0')),
  });
  assertPayloadPresent('automatic create value bounds', create.response.payload, 'percentageHigh');

  for (const id of automaticIdsFromPayload(create.response.payload, [
    'percentageHigh',
    'percentageNegative',
    'percentageZero',
    'amountNegative',
    'amountZero',
  ])) {
    cleanup.push({
      label: `discountAutomaticDelete:create:${id}`,
      run: () => runGraphqlRaw(deleteAutomaticDocument, { id }),
    });
  }

  update = await runCase(updateDocumentPath, {
    id: setupId,
    percentageHigh: automaticBasicInput(stamp, 'UPDATEPERCENTAGEHIGH', { percentage: 1.5 }),
    percentageNegative: automaticBasicInput(stamp, 'UPDATEPERCENTAGENEGATIVE', { percentage: -0.1 }),
    percentageZero: automaticBasicInput(stamp, 'UPDATEPERCENTAGEZERO', { percentage: 0 }),
    amountNegative: automaticBasicInput(stamp, 'UPDATEAMOUNTNEGATIVE', amountValue('-1')),
    amountZero: automaticBasicInput(stamp, 'UPDATEAMOUNTZERO', amountValue('0')),
  });
  assertPayloadPresent('automatic update value bounds', update.response.payload, 'percentageHigh');
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

if (setup === undefined || create === undefined || update === undefined) {
  throw new Error('Capture did not complete automatic value-bounds cases.');
}

const fixture = {
  scenarioId: 'discount-automatic-value-bounds',
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  accessScopes: scopeProbe,
  setup,
  cases: {
    create,
    update,
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
      cleanup: cleanupResponses.map((item) => ({ label: item.label, ok: item.error === undefined })),
    },
    null,
    2,
  ),
);

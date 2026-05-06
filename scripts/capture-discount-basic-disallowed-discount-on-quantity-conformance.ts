/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { assertDiscountConformanceScopes, probeDiscountConformanceScopes } from './discount-conformance-lib.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type RawCase = {
  query: string;
  variables: Record<string, unknown>;
  payload: unknown;
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
const outputPath = path.join(outputDir, 'discount-basic-disallowed-discount-on-quantity.json');
const codeCreateRequestPath =
  'config/parity-requests/discounts/discount-basic-disallowed-discount-on-quantity-code-create.graphql';
const codeUpdateRequestPath =
  'config/parity-requests/discounts/discount-basic-disallowed-discount-on-quantity-code-update.graphql';
const automaticCreateRequestPath =
  'config/parity-requests/discounts/discount-basic-disallowed-discount-on-quantity-automatic-create.graphql';
const automaticUpdateRequestPath =
  'config/parity-requests/discounts/discount-basic-disallowed-discount-on-quantity-automatic-update.graphql';

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const adminOptions = {
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
};
const { runGraphqlRaw } = createAdminGraphqlClient(adminOptions);

const codeDeleteMutation = `#graphql
  mutation DiscountBasicDisallowedQuantityCodeCleanup($id: ID!) {
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

const automaticDeleteMutation = `#graphql
  mutation DiscountBasicDisallowedQuantityAutomaticCleanup($id: ID!) {
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

function percentageValue(): Record<string, unknown> {
  return {
    percentage: 0.1,
  };
}

function discountOnQuantityValue(): Record<string, unknown> {
  return {
    discountOnQuantity: {
      quantity: '2',
      effect: {
        percentage: 0.5,
      },
    },
  };
}

function baseCustomerGets(value: Record<string, unknown>): Record<string, unknown> {
  return {
    value,
    items: {
      all: true,
    },
  };
}

function codeInput(stamp: number, suffix: string, value: Record<string, unknown>): Record<string, unknown> {
  return {
    title: `Basic disallowed quantity code ${suffix} ${stamp}`,
    code: `BASICQTY${suffix}${stamp}`,
    startsAt: '2026-04-25T00:00:00Z',
    customerSelection: {
      all: true,
    },
    customerGets: baseCustomerGets(value),
  };
}

function automaticInput(stamp: number, suffix: string, value: Record<string, unknown>): Record<string, unknown> {
  return {
    title: `Basic disallowed quantity automatic ${suffix} ${stamp}`,
    startsAt: '2026-04-25T00:00:00Z',
    customerGets: baseCustomerGets(value),
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

  if (typeof current === 'object' && current !== null && !Array.isArray(current)) {
    return current as Record<string, unknown>;
  }

  return undefined;
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

function assertDiscountOnQuantityRejected(
  label: string,
  payload: unknown,
  root: string,
  inputName: string,
  nodeField: string,
): void {
  const rootPayload = readRecord(payload, ['data', root]);
  if (rootPayload === undefined) {
    throw new Error(`${label} missing data.${root}: ${JSON.stringify(payload)}`);
  }

  if (rootPayload[nodeField] !== null) {
    throw new Error(`${label} unexpectedly returned ${nodeField}: ${JSON.stringify(rootPayload[nodeField])}`);
  }

  const expected = [
    {
      field: [inputName, 'customerGets', 'value', 'discountOnQuantity'],
      message: 'discountOnQuantity field is only permitted with bxgy discounts.',
      code: 'INVALID',
      extraInfo: null,
    },
  ];
  if (JSON.stringify(rootPayload['userErrors']) !== JSON.stringify(expected)) {
    throw new Error(`${label} unexpected userErrors: ${JSON.stringify(rootPayload['userErrors'])}`);
  }
}

async function runCase(query: string, variables: Record<string, unknown>): Promise<RawCase> {
  const response = await runGraphqlRaw(query, variables);
  return {
    query,
    variables,
    payload: response.payload,
  };
}

await mkdir(outputDir, { recursive: true });

const scopeProbe = await probeDiscountConformanceScopes(adminOptions);
assertDiscountConformanceScopes(scopeProbe);

const codeCreateQuery = await readFile(codeCreateRequestPath, 'utf8');
const codeUpdateQuery = await readFile(codeUpdateRequestPath, 'utf8');
const automaticCreateQuery = await readFile(automaticCreateRequestPath, 'utf8');
const automaticUpdateQuery = await readFile(automaticUpdateRequestPath, 'utf8');

const stamp = Date.now();
const cleanup: CleanupStep[] = [];
const cleanupResponses: Array<{ label: string; payload?: unknown; error?: string }> = [];
let codeCreate: RawCase | undefined;
let automaticCreate: RawCase | undefined;
let invalidCodeCreate: RawCase | undefined;
let invalidCodeUpdate: RawCase | undefined;
let invalidAutomaticCreate: RawCase | undefined;
let invalidAutomaticUpdate: RawCase | undefined;

try {
  codeCreate = await runCase(codeCreateQuery, {
    input: codeInput(stamp, 'SETUP', percentageValue()),
  });
  assertNoUserErrors('code setup create', codeCreate.payload, 'discountCodeBasicCreate');
  const codeId = readString(codeCreate.payload, ['data', 'discountCodeBasicCreate', 'codeDiscountNode', 'id']);
  if (codeId === undefined) {
    throw new Error(`code setup create did not return an id: ${JSON.stringify(codeCreate.payload)}`);
  }
  cleanup.push({
    label: 'discountCodeDelete',
    run: () => runGraphqlRaw(codeDeleteMutation, { id: codeId }),
  });

  automaticCreate = await runCase(automaticCreateQuery, {
    input: automaticInput(stamp, 'SETUP', percentageValue()),
  });
  assertNoUserErrors('automatic setup create', automaticCreate.payload, 'discountAutomaticBasicCreate');
  const automaticId = readString(automaticCreate.payload, [
    'data',
    'discountAutomaticBasicCreate',
    'automaticDiscountNode',
    'id',
  ]);
  if (automaticId === undefined) {
    throw new Error(`automatic setup create did not return an id: ${JSON.stringify(automaticCreate.payload)}`);
  }
  cleanup.push({
    label: 'discountAutomaticDelete',
    run: () => runGraphqlRaw(automaticDeleteMutation, { id: automaticId }),
  });

  invalidCodeCreate = await runCase(codeCreateQuery, {
    input: codeInput(stamp, 'CREATE', discountOnQuantityValue()),
  });
  assertDiscountOnQuantityRejected(
    'code basic create validation',
    invalidCodeCreate.payload,
    'discountCodeBasicCreate',
    'basicCodeDiscount',
    'codeDiscountNode',
  );

  invalidCodeUpdate = await runCase(codeUpdateQuery, {
    id: codeId,
    input: codeInput(stamp, 'UPDATE', discountOnQuantityValue()),
  });
  assertDiscountOnQuantityRejected(
    'code basic update validation',
    invalidCodeUpdate.payload,
    'discountCodeBasicUpdate',
    'basicCodeDiscount',
    'codeDiscountNode',
  );

  invalidAutomaticCreate = await runCase(automaticCreateQuery, {
    input: automaticInput(stamp, 'CREATE', discountOnQuantityValue()),
  });
  assertDiscountOnQuantityRejected(
    'automatic basic create validation',
    invalidAutomaticCreate.payload,
    'discountAutomaticBasicCreate',
    'automaticBasicDiscount',
    'automaticDiscountNode',
  );

  invalidAutomaticUpdate = await runCase(automaticUpdateQuery, {
    id: automaticId,
    input: automaticInput(stamp, 'UPDATE', discountOnQuantityValue()),
  });
  assertDiscountOnQuantityRejected(
    'automatic basic update validation',
    invalidAutomaticUpdate.payload,
    'discountAutomaticBasicUpdate',
    'automaticBasicDiscount',
    'automaticDiscountNode',
  );
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
  automaticCreate === undefined ||
  invalidCodeCreate === undefined ||
  invalidCodeUpdate === undefined ||
  invalidAutomaticCreate === undefined ||
  invalidAutomaticUpdate === undefined
) {
  throw new Error('Capture did not complete all setup and validation cases.');
}

const fixture = {
  storeDomain,
  apiVersion,
  accessScopes: scopeProbe,
  setup: {
    codeCreate,
    automaticCreate,
  },
  validation: {
    codeCreate: invalidCodeCreate,
    codeUpdate: invalidCodeUpdate,
    automaticCreate: invalidAutomaticCreate,
    automaticUpdate: invalidAutomaticUpdate,
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
    },
    null,
    2,
  ),
);

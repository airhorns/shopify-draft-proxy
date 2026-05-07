/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type UserError = {
  field?: string[] | null;
  message?: string;
  code?: string | null;
};

type CapturedCase<TData = unknown> = {
  name: string;
  query: string;
  variables: Record<string, unknown>;
  response: ConformanceGraphqlResult<TData>;
};

type MutationPayload = {
  userErrors?: UserError[];
  priceList?: { id?: string; name?: string; currency?: string } | null;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'markets');
const outputPath = path.join(outputDir, 'price-list-name-validation.json');

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const priceListCreateMutation = `#graphql
mutation PriceListNameValidationCreate($input: PriceListCreateInput!) {
  priceListCreate(input: $input) {
    priceList {
      id
      name
      currency
      parent {
        adjustment {
          type
          value
        }
      }
    }
    userErrors {
      field
      message
      code
    }
  }
}
`;

const priceListUpdateMutation = `#graphql
mutation PriceListNameValidationUpdate($id: ID!, $input: PriceListUpdateInput!) {
  priceListUpdate(id: $id, input: $input) {
    priceList {
      id
      name
      currency
      parent {
        adjustment {
          type
          value
        }
      }
    }
    userErrors {
      field
      message
      code
    }
  }
}
`;

const priceListDeleteMutation = `#graphql
mutation PriceListNameValidationCleanup($id: ID!) {
  priceListDelete(id: $id) {
    deletedId
    userErrors {
      field
      message
      code
    }
  }
}
`;

function payloadUserErrors<TData>(result: ConformanceGraphqlResult<TData>, root: string): UserError[] {
  const data = result.payload.data;
  if (typeof data !== 'object' || data === null) return [];
  const payload = (data as Record<string, unknown>)[root];
  if (typeof payload !== 'object' || payload === null) return [];
  const maybeErrors = (payload as MutationPayload).userErrors;
  return Array.isArray(maybeErrors) ? maybeErrors : [];
}

function mutationPayload<TData>(result: ConformanceGraphqlResult<TData>, root: string): MutationPayload {
  const data = result.payload.data;
  if (typeof data !== 'object' || data === null) return {};
  const payload = (data as Record<string, unknown>)[root];
  return typeof payload === 'object' && payload !== null ? (payload as MutationPayload) : {};
}

function assertNoGraphqlErrors<TData>(result: ConformanceGraphqlResult<TData>, label: string): void {
  if (result.status !== 200 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertNoUserErrors<TData>(result: ConformanceGraphqlResult<TData>, root: string, label: string): void {
  assertNoGraphqlErrors(result, label);
  const errors = payloadUserErrors(result, root);
  if (errors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(errors)}`);
  }
}

function assertUserError<TData>(
  result: ConformanceGraphqlResult<TData>,
  root: string,
  expectedCode: string,
  expectedField: string[],
  label: string,
): void {
  assertNoGraphqlErrors(result, label);
  const errors = payloadUserErrors(result, root);
  const matched = errors.some(
    (error) => error.code === expectedCode && JSON.stringify(error.field ?? null) === JSON.stringify(expectedField),
  );
  if (!matched) {
    throw new Error(`${label} missing ${expectedCode}: ${JSON.stringify(errors)}`);
  }
}

function priceListId<TData>(result: ConformanceGraphqlResult<TData>, root: string): string {
  const id = mutationPayload(result, root).priceList?.id;
  if (typeof id !== 'string') {
    throw new Error(`${root} did not return a price list id: ${JSON.stringify(result.payload)}`);
  }
  return id;
}

async function captureCase<TData>(
  name: string,
  query: string,
  variables: Record<string, unknown>,
): Promise<CapturedCase<TData>> {
  return {
    name,
    query,
    variables,
    response: await runGraphqlRequest<TData>(query, variables),
  };
}

function createInput(name: string, currency: string): Record<string, unknown> {
  return {
    input: {
      name,
      currency,
      parent: {
        adjustment: {
          type: 'PERCENTAGE_DECREASE',
          value: 10,
        },
      },
    },
  };
}

const unique = Date.now().toString(36);
const duplicateName = `Price list duplicate name ${unique}`;
const updateOriginalName = `Price list update original ${unique}`;
const tooLongName = 'L'.repeat(256);
const createdPriceListIds: string[] = [];
const cleanup: Array<{
  type: 'priceList';
  id: string;
  response: ConformanceGraphqlResult<unknown>;
}> = [];
const cases: Array<CapturedCase<unknown>> = [];
const nameField = ['input', 'name'];

try {
  const baseline = await captureCase(
    'priceListCreate baseline for duplicate-name validation',
    priceListCreateMutation,
    createInput(duplicateName, 'USD'),
  );
  assertNoUserErrors(baseline.response, 'priceListCreate', 'baseline price-list create');
  const baselineId = priceListId(baseline.response, 'priceListCreate');
  createdPriceListIds.push(baselineId);
  cases.push(baseline);

  const duplicateCreate = await captureCase(
    'priceListCreate duplicate name invalid',
    priceListCreateMutation,
    createInput(duplicateName, 'CAD'),
  );
  assertUserError(duplicateCreate.response, 'priceListCreate', 'TAKEN', nameField, 'duplicate name create');
  cases.push(duplicateCreate);

  const tooLongCreate = await captureCase(
    'priceListCreate name above two hundred fifty five invalid',
    priceListCreateMutation,
    createInput(tooLongName, 'EUR'),
  );
  assertUserError(tooLongCreate.response, 'priceListCreate', 'TOO_LONG', nameField, 'too-long name create');
  cases.push(tooLongCreate);

  const updateSubject = await captureCase(
    'priceListCreate subject for update name validation',
    priceListCreateMutation,
    createInput(updateOriginalName, 'GBP'),
  );
  assertNoUserErrors(updateSubject.response, 'priceListCreate', 'update-subject price-list create');
  const updateSubjectId = priceListId(updateSubject.response, 'priceListCreate');
  createdPriceListIds.push(updateSubjectId);
  cases.push(updateSubject);

  const duplicateUpdate = await captureCase('priceListUpdate duplicate name invalid', priceListUpdateMutation, {
    id: updateSubjectId,
    input: {
      name: duplicateName,
    },
  });
  assertUserError(duplicateUpdate.response, 'priceListUpdate', 'TAKEN', nameField, 'duplicate name update');
  cases.push(duplicateUpdate);

  const tooLongUpdate = await captureCase(
    'priceListUpdate name above two hundred fifty five invalid',
    priceListUpdateMutation,
    {
      id: updateSubjectId,
      input: {
        name: tooLongName,
      },
    },
  );
  assertUserError(tooLongUpdate.response, 'priceListUpdate', 'TOO_LONG', nameField, 'too-long name update');
  cases.push(tooLongUpdate);
} finally {
  for (const id of [...createdPriceListIds].reverse()) {
    cleanup.push({
      type: 'priceList',
      id,
      response: await runGraphqlRequest(priceListDeleteMutation, { id }),
    });
  }
}

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      scope: 'price-list name length and uniqueness validation',
      setup: {
        duplicateName,
        updateOriginalName,
        tooLongNameLength: tooLongName.length,
        note: 'The recorder creates a baseline price list, attempts duplicate and over-length create/update branches, then deletes every successfully created price list.',
      },
      cases,
      cleanup,
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(
  JSON.stringify(
    {
      ok: true,
      outputPath,
      storeDomain,
      apiVersion,
      cases: cases.map((entry) => ({
        name: entry.name,
        status: entry.response.status,
      })),
      cleanup: cleanup.map((entry) => ({
        type: entry.type,
        id: entry.id,
        status: entry.response.status,
      })),
    },
    null,
    2,
  ),
);

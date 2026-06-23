/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type Scenario = {
  label: string;
  variables: JsonRecord;
  status: number;
  response: unknown;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'selling-plans');
const outputPath = path.join(outputDir, 'selling-plan-group-create-active-model-validation.json');
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const createMutation = `#graphql
  mutation SellingPlanGroupCreateActiveModelValidation($input: SellingPlanGroupInput!) {
    sellingPlanGroupCreate(input: $input) {
      sellingPlanGroup {
        id
        sellingPlans(first: 5) {
          nodes {
            id
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

const updateMutation = `#graphql
  mutation SellingPlanGroupUpdateEmptyCreateList($id: ID!, $input: SellingPlanGroupInput!) {
    sellingPlanGroupUpdate(id: $id, input: $input) {
      deletedSellingPlanIds
      sellingPlanGroup {
        id
        sellingPlans(first: 5) {
          nodes {
            id
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

const deleteMutation = `#graphql
  mutation DeleteSellingPlanGroup($id: ID!) {
    sellingPlanGroupDelete(id: $id) {
      deletedSellingPlanGroupId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

function readRecord(value: unknown): JsonRecord {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`Expected object, got ${JSON.stringify(value)}`);
  }
  return value as JsonRecord;
}

function readArray(value: unknown): unknown[] {
  if (!Array.isArray(value)) throw new Error(`Expected array, got ${JSON.stringify(value)}`);
  return value;
}

function data(payload: unknown): JsonRecord {
  return readRecord(readRecord(payload)['data']);
}

function root(payload: unknown, rootName: string): JsonRecord {
  return readRecord(data(payload)[rootName]);
}

function userErrors(payload: unknown, rootName: string): unknown[] {
  return readArray(root(payload, rootName)['userErrors']);
}

function assertNoTopLevelErrors(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || readRecord(result.payload)['errors']) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertUserError(result: ConformanceGraphqlResult, label: string): void {
  assertNoTopLevelErrors(result, label);
  if (userErrors(result.payload, 'sellingPlanGroupCreate').length === 0) {
    throw new Error(`${label} did not return userErrors: ${JSON.stringify(result.payload, null, 2)}`);
  }
  if (root(result.payload, 'sellingPlanGroupCreate')['sellingPlanGroup'] !== null) {
    throw new Error(`${label} returned a group despite userErrors: ${JSON.stringify(result.payload, null, 2)}`);
  }
}

function assertNoUserErrorsPayload(payload: unknown, rootName: string, label: string): void {
  const errors = userErrors(payload, rootName);
  if (errors.length !== 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(errors, null, 2)}`);
  }
}

async function capture(
  label: string,
  query: string,
  variables: JsonRecord,
  expectUserError: boolean,
): Promise<Scenario> {
  const result = await runGraphqlRaw(query, variables);
  if (expectUserError) assertUserError(result, label);
  else assertNoTopLevelErrors(result, label);
  return {
    label,
    variables,
    status: result.status,
    response: result.payload,
  };
}

function recurringBillingPolicy(): JsonRecord {
  return {
    recurring: {
      interval: 'MONTH',
      intervalCount: 1,
    },
  };
}

function recurringDeliveryPolicy(): JsonRecord {
  return {
    recurring: {
      interval: 'MONTH',
      intervalCount: 1,
    },
  };
}

function validSellingPlanInput(name = 'Monthly delivery'): JsonRecord {
  return {
    name,
    options: [name],
    category: 'SUBSCRIPTION',
    billingPolicy: recurringBillingPolicy(),
    deliveryPolicy: recurringDeliveryPolicy(),
  };
}

function validGroupInput(overrides: JsonRecord = {}): JsonRecord {
  return {
    name: 'Create validation group',
    options: ['Delivery frequency'],
    sellingPlansToCreate: [validSellingPlanInput()],
    ...overrides,
  };
}

const cleanup: Scenario[] = [];
let groupId: string | null = null;
let planId: string | null = null;
const scenarios: Record<string, Scenario> = {};

try {
  scenarios['validSeed'] = await capture(
    'validSeed',
    createMutation,
    { input: validGroupInput({ name: `Create validation seed ${Date.now()}` }) },
    false,
  );
  assertNoUserErrorsPayload(scenarios['validSeed'].response, 'sellingPlanGroupCreate', 'validSeed');
  const createdGroup = readRecord(root(scenarios['validSeed'].response, 'sellingPlanGroupCreate')['sellingPlanGroup']);
  groupId = createdGroup['id'] as string;
  const planNodes = readArray(readRecord(createdGroup['sellingPlans'])['nodes']);
  planId = readRecord(planNodes[0])['id'] as string;

  scenarios['blankName'] = await capture(
    'blankName',
    createMutation,
    { input: validGroupInput({ name: '   ' }) },
    true,
  );

  const absentNameInput = validGroupInput();
  delete absentNameInput['name'];
  scenarios['absentName'] = await capture('absentName', createMutation, { input: absentNameInput }, true);

  scenarios['zeroPlans'] = await capture(
    'zeroPlans',
    createMutation,
    { input: validGroupInput({ name: 'Zero plans', sellingPlansToCreate: [] }) },
    true,
  );

  const absentPlansInput = validGroupInput({ name: 'Absent plans' });
  delete absentPlansInput['sellingPlansToCreate'];
  scenarios['absentPlans'] = await capture('absentPlans', createMutation, { input: absentPlansInput }, true);

  scenarios['tooManyPlans'] = await capture(
    'tooManyPlans',
    createMutation,
    {
      input: validGroupInput({
        name: 'Too many plans',
        sellingPlansToCreate: Array.from({ length: 32 }, (_, index) => validSellingPlanInput(`Monthly ${index + 1}`)),
      }),
    },
    true,
  );

  scenarios['missingBillingPolicy'] = await capture(
    'missingBillingPolicy',
    createMutation,
    {
      input: validGroupInput({
        name: 'Missing billing',
        sellingPlansToCreate: [
          {
            name: 'Monthly',
            options: ['Monthly'],
            category: 'SUBSCRIPTION',
            deliveryPolicy: recurringDeliveryPolicy(),
          },
        ],
      }),
    },
    true,
  );

  scenarios['missingDeliveryPolicy'] = await capture(
    'missingDeliveryPolicy',
    createMutation,
    {
      input: validGroupInput({
        name: 'Missing delivery',
        sellingPlansToCreate: [
          {
            name: 'Monthly',
            options: ['Monthly'],
            category: 'SUBSCRIPTION',
            billingPolicy: recurringBillingPolicy(),
          },
        ],
      }),
    },
    true,
  );

  scenarios['missingBothPolicies'] = await capture(
    'missingBothPolicies',
    createMutation,
    {
      input: validGroupInput({
        name: 'Missing both',
        sellingPlansToCreate: [
          {
            name: 'Monthly',
            options: ['Monthly'],
            category: 'SUBSCRIPTION',
          },
        ],
      }),
    },
    true,
  );

  scenarios['updateEmptyCreateList'] = await capture(
    'updateEmptyCreateList',
    updateMutation,
    { id: groupId, input: { sellingPlansToCreate: [] } },
    false,
  );
  assertNoUserErrorsPayload(
    scenarios['updateEmptyCreateList'].response,
    'sellingPlanGroupUpdate',
    'updateEmptyCreateList',
  );
} finally {
  if (groupId) {
    const result = await runGraphqlRaw(deleteMutation, { id: groupId });
    cleanup.push({
      label: 'cleanup sellingPlanGroupDelete',
      variables: { id: groupId },
      status: result.status,
      response: result.payload,
    });
  }
}

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      apiVersion,
      storeDomain,
      notes: [
        'Captures Admin 2026-04 sellingPlanGroupCreate model-backed validation errors after pure input validation passes.',
        'The script creates one disposable valid group to prove sellingPlanGroupUpdate does not apply the create-only lower bound to an empty sellingPlansToCreate list, then deletes the group during cleanup.',
      ],
      groupId,
      planId,
      scenarios,
      cleanup,
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(`Wrote selling-plan group create ActiveModel validation fixture to ${outputPath}`);

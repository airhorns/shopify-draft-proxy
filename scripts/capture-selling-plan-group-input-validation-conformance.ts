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
  defaultApiVersion: '2025-01',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const outputPath = path.join(outputDir, 'selling-plan-group-input-validation.json');
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const createMutation = `#graphql
  mutation SellingPlanGroupCreateInputValidation(
    $input: SellingPlanGroupInput!
    $resources: SellingPlanGroupResourceInput
  ) {
    sellingPlanGroupCreate(input: $input, resources: $resources) {
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
  mutation SellingPlanGroupUpdateInputValidation($id: ID!, $input: SellingPlanGroupInput!) {
    sellingPlanGroupUpdate(id: $id, input: $input) {
      deletedSellingPlanIds
      sellingPlanGroup {
        id
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
  if (!Array.isArray(value)) {
    throw new Error(`Expected array, got ${JSON.stringify(value)}`);
  }
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
  const createErrors = data(result.payload)['sellingPlanGroupCreate']
    ? userErrors(result.payload, 'sellingPlanGroupCreate')
    : [];
  const updateErrors = data(result.payload)['sellingPlanGroupUpdate']
    ? userErrors(result.payload, 'sellingPlanGroupUpdate')
    : [];
  if (createErrors.length === 0 && updateErrors.length === 0) {
    throw new Error(`${label} did not return userErrors: ${JSON.stringify(result.payload, null, 2)}`);
  }
}

async function capture(
  label: string,
  query: string,
  variables: JsonRecord,
  expectUserError: boolean,
): Promise<Scenario> {
  const result = await runGraphqlRaw(query, variables);
  if (expectUserError) {
    assertUserError(result, label);
  } else {
    assertNoTopLevelErrors(result, label);
  }
  return {
    label,
    variables,
    status: result.status,
    response: result.payload,
  };
}

function fixedPricingPolicy(): JsonRecord {
  return {
    fixed: {
      adjustmentType: 'PERCENTAGE',
      adjustmentValue: { percentage: 10 },
    },
  };
}

function recurringPricingPolicy(afterCycle: number): JsonRecord {
  return {
    recurring: {
      adjustmentType: 'PERCENTAGE',
      adjustmentValue: { percentage: 5 },
      afterCycle,
    },
  };
}

function recurringBillingPolicy(): JsonRecord {
  return {
    recurring: {
      interval: 'MONTH',
      intervalCount: 1,
      minCycles: 1,
      maxCycles: 12,
    },
  };
}

function recurringDeliveryPolicy(): JsonRecord {
  return {
    recurring: {
      interval: 'MONTH',
      intervalCount: 1,
      cutoff: 0,
    },
  };
}

function fixedDeliveryPolicy(): JsonRecord {
  return {
    fixed: {
      fulfillmentTrigger: 'ASAP',
    },
  };
}

function validSellingPlanInput(overrides: JsonRecord = {}): JsonRecord {
  return {
    name: 'Monthly delivery',
    options: ['Monthly'],
    position: 1,
    category: 'SUBSCRIPTION',
    billingPolicy: recurringBillingPolicy(),
    deliveryPolicy: recurringDeliveryPolicy(),
    inventoryPolicy: { reserve: 'ON_FULFILLMENT' },
    pricingPolicies: [fixedPricingPolicy()],
    ...overrides,
  };
}

function validGroupInput(overrides: JsonRecord = {}): JsonRecord {
  return {
    name: 'Input validation group',
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
  scenarios.validSeed = await capture(
    'validSeed',
    createMutation,
    { input: validGroupInput({ name: `Input validation seed ${Date.now()}` }), resources: {} },
    false,
  );
  const createdGroup = readRecord(root(scenarios.validSeed.response, 'sellingPlanGroupCreate')['sellingPlanGroup']);
  groupId = createdGroup['id'] as string;
  const planNodes = readArray(readRecord(createdGroup['sellingPlans'])['nodes']);
  planId = readRecord(planNodes[0])['id'] as string;

  scenarios.groupOptionsTooLong = await capture(
    'groupOptionsTooLong',
    createMutation,
    {
      input: {
        name: 'Too many group options',
        options: ['a', 'b', 'c', 'd'],
      },
      resources: {},
    },
    true,
  );
  scenarios.groupPositionInvalid = await capture(
    'groupPositionInvalid',
    createMutation,
    {
      input: {
        name: 'Bad group position',
        position: 9_999_999_999,
      },
      resources: {},
    },
    true,
  );
  scenarios.planCreateOptionsTooLong = await capture(
    'planCreateOptionsTooLong',
    createMutation,
    {
      input: validGroupInput({
        name: 'Too many plan options',
        sellingPlansToCreate: [validSellingPlanInput({ options: ['a', 'b', 'c', 'd'] })],
      }),
      resources: {},
    },
    true,
  );
  scenarios.planCreatePricingPoliciesTooLong = await capture(
    'planCreatePricingPoliciesTooLong',
    createMutation,
    {
      input: validGroupInput({
        name: 'Too many pricing policies',
        sellingPlansToCreate: [
          validSellingPlanInput({
            pricingPolicies: [fixedPricingPolicy(), recurringPricingPolicy(2), recurringPricingPolicy(3)],
          }),
        ],
      }),
      resources: {},
    },
    true,
  );
  scenarios.planCreatePositionInvalid = await capture(
    'planCreatePositionInvalid',
    createMutation,
    {
      input: validGroupInput({
        name: 'Bad plan position',
        sellingPlansToCreate: [validSellingPlanInput({ position: 9_999_999_999 })],
      }),
      resources: {},
    },
    true,
  );
  scenarios.planCreatePolicyMismatch = await capture(
    'planCreatePolicyMismatch',
    createMutation,
    {
      input: validGroupInput({
        name: 'Policy mismatch',
        sellingPlansToCreate: [validSellingPlanInput({ deliveryPolicy: fixedDeliveryPolicy() })],
      }),
      resources: {},
    },
    true,
  );
  scenarios.groupUpdateOptionsTooLong = await capture(
    'groupUpdateOptionsTooLong',
    updateMutation,
    {
      id: groupId,
      input: {
        options: ['a', 'b', 'c', 'd'],
      },
    },
    true,
  );
  scenarios.planUpdateMissingId = await capture(
    'planUpdateMissingId',
    updateMutation,
    {
      id: groupId,
      input: {
        sellingPlansToUpdate: [{ name: 'Missing id' }],
      },
    },
    true,
  );
  scenarios.planUpdateOptionsTooLong = await capture(
    'planUpdateOptionsTooLong',
    updateMutation,
    {
      id: groupId,
      input: {
        sellingPlansToUpdate: [{ id: planId, options: ['a', 'b', 'c', 'd'] }],
      },
    },
    true,
  );
  scenarios.planUpdatePricingPoliciesTooLong = await capture(
    'planUpdatePricingPoliciesTooLong',
    updateMutation,
    {
      id: groupId,
      input: {
        sellingPlansToUpdate: [
          { id: planId, pricingPolicies: [fixedPricingPolicy(), recurringPricingPolicy(2), recurringPricingPolicy(3)] },
        ],
      },
    },
    true,
  );
  scenarios.planUpdatePositionInvalid = await capture(
    'planUpdatePositionInvalid',
    updateMutation,
    {
      id: groupId,
      input: {
        sellingPlansToUpdate: [{ id: planId, position: 9_999_999_999 }],
      },
    },
    true,
  );
  scenarios.planUpdatePolicyMismatch = await capture(
    'planUpdatePolicyMismatch',
    updateMutation,
    {
      id: groupId,
      input: {
        sellingPlansToUpdate: [
          { id: planId, billingPolicy: recurringBillingPolicy(), deliveryPolicy: fixedDeliveryPolicy() },
        ],
      },
    },
    true,
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
        'Captures selling-plan group input validation userErrors for group options, positions, nested selling plan limits, required update ids, and billing/delivery policy kind compatibility.',
        'The script creates one disposable selling-plan group so update validation branches can target an existing group and deletes it during cleanup.',
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

console.log(`Wrote selling-plan group input validation fixture to ${outputPath}`);

/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import {
  createConformanceCapture,
  readArray,
  readRecord,
  requireString,
  type JsonRecord,
} from './conformance-capture-lib.js';

type Scenario = {
  label: string;
  variables: JsonRecord;
  status: number;
  response: unknown;
};

const expectedSummary = '4 delivery frequencies, 10-20%·$5-$8 discount';

const capture = await createConformanceCapture();
const createMutation = await capture.readRequest('selling-plans', 'sellingPlanGroupCreate-summary.graphql');
const readQuery = await capture.readRequest('selling-plans', 'sellingPlanGroupSummary-read.graphql');

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

function recurringPolicy(): JsonRecord {
  return {
    recurring: {
      interval: 'MONTH',
      intervalCount: 1,
    },
  };
}

function sellingPlan(
  name: string,
  secondOption: string,
  adjustmentType: string,
  adjustmentValue: JsonRecord,
): JsonRecord {
  return {
    name,
    options: [name, secondOption],
    category: 'SUBSCRIPTION',
    billingPolicy: recurringPolicy(),
    deliveryPolicy: recurringPolicy(),
    pricingPolicies: [
      {
        fixed: {
          adjustmentType,
          adjustmentValue,
        },
      },
    ],
  };
}

async function runScenario(label: string, query: string, variables: JsonRecord): Promise<Scenario> {
  const result = await capture.runGraphqlRequest(query, variables);
  if (result.status < 200 || result.status >= 300 || readRecord(result.payload)?.['errors']) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
  return {
    label,
    variables,
    status: result.status,
    response: result.payload,
  };
}

function scenarioData(scenario: Scenario, rootName: string): JsonRecord {
  const data = readRecord(readRecord(scenario.response)?.['data']);
  const root = readRecord(data?.[rootName]);
  if (!root) {
    throw new Error(`${scenario.label} missing ${rootName}: ${JSON.stringify(scenario.response, null, 2)}`);
  }
  return root;
}

function assertNoUserErrors(root: JsonRecord, label: string): void {
  const userErrors = readArray(root['userErrors']);
  if (userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function assertSummary(value: unknown, label: string): void {
  if (value !== expectedSummary) {
    throw new Error(`${label} summary mismatch: expected ${expectedSummary}, got ${JSON.stringify(value)}`);
  }
}

const scenarios: Record<string, Scenario> = {};
let groupId: string | null = null;

try {
  scenarios['createSummary'] = await runScenario('createSummary', createMutation, {
    input: {
      name: `Summary parity ${capture.stamp}`,
      options: ['Delivery frequency', 'Billing cadence'],
      sellingPlansToCreate: [
        sellingPlan('Monthly percentage', 'Monthly billing', 'PERCENTAGE', { percentage: 10 }),
        sellingPlan('Annual percentage', 'Annual billing', 'PERCENTAGE', { percentage: 20 }),
        sellingPlan('Monthly fixed', 'Monthly fixed billing', 'FIXED_AMOUNT', { fixedValue: '5.0' }),
        sellingPlan('Annual fixed', 'Annual fixed billing', 'FIXED_AMOUNT', { fixedValue: '7.5' }),
      ],
    },
  });

  const createRoot = scenarioData(scenarios['createSummary'], 'sellingPlanGroupCreate');
  assertNoUserErrors(createRoot, 'createSummary');
  const createdGroup = readRecord(createRoot['sellingPlanGroup']);
  if (!createdGroup) {
    throw new Error(`createSummary did not return a sellingPlanGroup: ${JSON.stringify(createRoot, null, 2)}`);
  }
  groupId = requireString(createdGroup['id'], 'createSummary.sellingPlanGroup.id');
  assertSummary(createdGroup['summary'], 'createSummary');

  scenarios['readSummary'] = await runScenario('readSummary', readQuery, { id: groupId });
  const readGroup = readRecord(
    readRecord(readRecord(scenarios['readSummary'].response)?.['data'])?.['sellingPlanGroup'],
  );
  if (!readGroup) {
    throw new Error(
      `readSummary did not return a sellingPlanGroup: ${JSON.stringify(scenarios['readSummary'].response, null, 2)}`,
    );
  }
  assertSummary(readGroup['summary'], 'readSummary');
} finally {
  if (groupId) {
    scenarios['cleanupDelete'] = await runScenario('cleanupDelete', deleteMutation, { id: groupId });
  }
}

const outputPath = capture.fixturePath('selling-plans', 'selling-plan-group-summary.json');
await capture.writeJson(outputPath, {
  metadata: {
    scenario: 'sellingPlanGroup.summary computed discount range',
    storeDomain: capture.storeDomain,
    apiVersion: capture.apiVersion,
    capturedAt: new Date().toISOString(),
    expectedSummary,
  },
  scenarios,
});

console.log(JSON.stringify({ ok: true, outputPath, expectedSummary }, null, 2));

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
  request: {
    query: string;
    variables: JsonRecord;
  };
  status: number;
  response: unknown;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const outputPath = path.join(outputDir, 'selling-plan-group-update-delete-to-zero.json');
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const createMutation = `#graphql
  mutation CreateSellingPlanGroupForPlanCount($input: SellingPlanGroupInput!) {
    sellingPlanGroupCreate(input: $input) {
      sellingPlanGroup {
        id
        sellingPlans(first: 5) {
          nodes {
            id
            name
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
  mutation UpdateSellingPlanGroupForPlanCount($id: ID!, $input: SellingPlanGroupInput!) {
    sellingPlanGroupUpdate(id: $id, input: $input) {
      deletedSellingPlanIds
      sellingPlanGroup {
        id
        sellingPlans(first: 5) {
          nodes {
            id
            name
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

const readQuery = `#graphql
  query ReadSellingPlanGroupForPlanCount($id: ID!) {
    sellingPlanGroup(id: $id) {
      id
      sellingPlans(first: 5) {
        nodes {
          id
          name
        }
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

function sellingPlanNodes(group: unknown): JsonRecord[] {
  return readArray(readRecord(readRecord(group)['sellingPlans'])['nodes']).map(readRecord);
}

function singlePlanNode(nodes: JsonRecord[], label: string): JsonRecord {
  const [node] = nodes;
  if (nodes.length !== 1 || node === undefined) {
    throw new Error(`${label} returned unexpected plan nodes: ${JSON.stringify(nodes, null, 2)}`);
  }
  return node;
}

function userErrors(payload: unknown, rootName: string): unknown[] {
  return readArray(root(payload, rootName)['userErrors']);
}

function assertNoTopLevelErrors(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || readRecord(result.payload)['errors']) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertNoUserErrors(payload: unknown, rootName: string, label: string): void {
  const errors = userErrors(payload, rootName);
  if (errors.length !== 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(errors, null, 2)}`);
  }
}

function assertUserErrorsEqual(payload: unknown, rootName: string, expected: unknown[], label: string): void {
  const actual = userErrors(payload, rootName);
  if (JSON.stringify(actual) !== JSON.stringify(expected)) {
    throw new Error(`${label} returned unexpected userErrors: ${JSON.stringify(actual, null, 2)}`);
  }
}

async function capture(label: string, query: string, variables: JsonRecord): Promise<Scenario> {
  const result = await runGraphqlRaw(query, variables);
  assertNoTopLevelErrors(result, label);
  return {
    label,
    request: { query, variables },
    status: result.status,
    response: result.payload,
  };
}

function validSellingPlanInput(name: string): JsonRecord {
  return {
    name,
    options: [name],
    category: 'SUBSCRIPTION',
    billingPolicy: { recurring: { interval: 'MONTH', intervalCount: 1 } },
    deliveryPolicy: { recurring: { interval: 'MONTH', intervalCount: 1 } },
  };
}

function validGroupInput(name: string): JsonRecord {
  return {
    name,
    options: ['Delivery frequency'],
    sellingPlansToCreate: [validSellingPlanInput('Monthly delivery')],
  };
}

const suffix = Date.now().toString(36);
const scenarios: Record<string, Scenario> = {};
const cleanup: Scenario[] = [];
let groupId: string | null = null;
let originalPlanId: string | null = null;
let replacementPlanId: string | null = null;

try {
  scenarios['create'] = await capture('create', createMutation, {
    input: validGroupInput(`Delete-to-zero seed ${suffix}`),
  });
  assertNoUserErrors(scenarios['create'].response, 'sellingPlanGroupCreate', 'create');
  const createdGroup = root(scenarios['create'].response, 'sellingPlanGroupCreate')['sellingPlanGroup'];
  groupId = readRecord(createdGroup)['id'] as string;
  const createdPlanNodes = sellingPlanNodes(createdGroup);
  originalPlanId = singlePlanNode(createdPlanNodes, 'create')['id'] as string;

  scenarios['deleteFinalRejected'] = await capture('deleteFinalRejected', updateMutation, {
    id: groupId,
    input: { sellingPlansToDelete: [originalPlanId] },
  });
  assertUserErrorsEqual(
    scenarios['deleteFinalRejected'].response,
    'sellingPlanGroupUpdate',
    [
      {
        field: ['input', 'sellingPlansToDelete'],
        message: "Selling plans to delete can't result in a selling plan group with no selling plan.",
        code: 'SELLING_PLAN_COUNT_LOWER_BOUND',
      },
    ],
    'deleteFinalRejected',
  );
  const rejectedGroup = root(scenarios['deleteFinalRejected'].response, 'sellingPlanGroupUpdate')['sellingPlanGroup'];
  if (rejectedGroup !== null) {
    throw new Error(`deleteFinalRejected returned a group payload: ${JSON.stringify(rejectedGroup, null, 2)}`);
  }

  scenarios['readAfterRejectedDelete'] = await capture('readAfterRejectedDelete', readQuery, { id: groupId });
  const readbackPlanNodes = sellingPlanNodes(data(scenarios['readAfterRejectedDelete'].response)['sellingPlanGroup']);
  const readbackPlanNode = singlePlanNode(readbackPlanNodes, 'readAfterRejectedDelete');
  if (readbackPlanNode['id'] !== originalPlanId) {
    throw new Error(`readAfterRejectedDelete lost the original plan: ${JSON.stringify(readbackPlanNodes, null, 2)}`);
  }

  scenarios['deleteFinalWithReplacement'] = await capture('deleteFinalWithReplacement', updateMutation, {
    id: groupId,
    input: {
      sellingPlansToDelete: [originalPlanId],
      sellingPlansToCreate: [validSellingPlanInput('Replacement delivery')],
    },
  });
  assertNoUserErrors(
    scenarios['deleteFinalWithReplacement'].response,
    'sellingPlanGroupUpdate',
    'deleteFinalWithReplacement',
  );
  const replacementGroup = root(scenarios['deleteFinalWithReplacement'].response, 'sellingPlanGroupUpdate')[
    'sellingPlanGroup'
  ];
  const replacementPlanNodes = sellingPlanNodes(replacementGroup);
  const replacementPlanNode = singlePlanNode(replacementPlanNodes, 'deleteFinalWithReplacement');
  if (replacementPlanNode['id'] === originalPlanId) {
    throw new Error(
      `deleteFinalWithReplacement returned unexpected plan nodes: ${JSON.stringify(replacementPlanNodes, null, 2)}`,
    );
  }
  replacementPlanId = replacementPlanNode['id'] as string;
} finally {
  if (groupId) {
    cleanup.push(await capture('cleanup sellingPlanGroupDelete', deleteMutation, { id: groupId }));
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
        'Captures Admin 2026-04 sellingPlanGroupUpdate delete-to-zero rejection, unchanged readback, and delete-with-replacement success.',
        'The script creates one disposable selling-plan group and deletes it during cleanup.',
      ],
      groupId,
      originalPlanId,
      replacementPlanId,
      scenarios,
      cleanup,
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(`Wrote selling-plan group update delete-to-zero fixture to ${outputPath}`);

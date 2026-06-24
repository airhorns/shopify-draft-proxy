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
  query: string;
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
const outputPath = path.join(outputDir, 'selling-plan-group-app-id-readback.json');
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const createMutation = `#graphql
  mutation SellingPlanGroupCreateAppIdReadback($input: SellingPlanGroupInput!) {
    sellingPlanGroupCreate(input: $input) {
      sellingPlanGroup {
        id
        appId
        name
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
  mutation SellingPlanGroupUpdateAppIdReadback($id: ID!, $input: SellingPlanGroupInput!) {
    sellingPlanGroupUpdate(id: $id, input: $input) {
      deletedSellingPlanIds
      sellingPlanGroup {
        id
        appId
        name
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
  query SellingPlanGroupAppIdReadback($id: ID!) {
    sellingPlanGroup(id: $id) {
      id
      appId
      name
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

function assertNoTopLevelErrors(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || readRecord(result.payload)['errors']) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertNoUserErrorsPayload(payload: unknown, rootName: string, label: string): void {
  const errors = readArray(root(payload, rootName)['userErrors']);
  if (errors.length !== 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(errors, null, 2)}`);
  }
}

function assertGroupAppId(payload: unknown, rootName: string, expectedAppId: string | null, label: string): string {
  const group = readRecord(root(payload, rootName)['sellingPlanGroup']);
  if (group['appId'] !== expectedAppId) {
    throw new Error(`${label} appId mismatch: ${JSON.stringify(group, null, 2)}`);
  }
  const id = group['id'];
  if (typeof id !== 'string') {
    throw new Error(`${label} missing group id: ${JSON.stringify(group, null, 2)}`);
  }
  return id;
}

function assertReadAppId(payload: unknown, expectedAppId: string | null, label: string): void {
  const group = readRecord(data(payload)['sellingPlanGroup']);
  if (group['appId'] !== expectedAppId) {
    throw new Error(`${label} appId mismatch: ${JSON.stringify(group, null, 2)}`);
  }
}

async function capture(label: string, query: string, variables: JsonRecord): Promise<Scenario> {
  const result = await runGraphqlRaw(query, variables);
  assertNoTopLevelErrors(result, label);
  return {
    label,
    query,
    variables,
    status: result.status,
    response: result.payload,
  };
}

function recurringPolicy(): JsonRecord {
  return {
    recurring: {
      interval: 'MONTH',
      intervalCount: 1,
    },
  };
}

function validGroupInput(suffix: string, appId: string): JsonRecord {
  return {
    name: `App ID readback group ${suffix}`,
    appId,
    options: ['Delivery frequency'],
    sellingPlansToCreate: [
      {
        name: 'Monthly delivery',
        options: ['Monthly'],
        category: 'SUBSCRIPTION',
        billingPolicy: recurringPolicy(),
        deliveryPolicy: recurringPolicy(),
      },
    ],
  };
}

const suffix = Date.now().toString(36);
const createAppId = `draft-proxy-create-${suffix}`;
const updateAppId = `draft-proxy-update-${suffix}`;
const scenarios: Record<string, Scenario> = {};
const cleanup: Scenario[] = [];
let groupId: string | null = null;

try {
  scenarios['createAppId'] = await capture('createAppId', createMutation, {
    input: validGroupInput(suffix, createAppId),
  });
  assertNoUserErrorsPayload(scenarios['createAppId'].response, 'sellingPlanGroupCreate', 'createAppId');
  groupId = assertGroupAppId(scenarios['createAppId'].response, 'sellingPlanGroupCreate', createAppId, 'createAppId');

  scenarios['readAfterCreate'] = await capture('readAfterCreate', readQuery, { id: groupId });
  assertReadAppId(scenarios['readAfterCreate'].response, createAppId, 'readAfterCreate');

  scenarios['updateAppId'] = await capture('updateAppId', updateMutation, {
    id: groupId,
    input: { appId: updateAppId },
  });
  assertNoUserErrorsPayload(scenarios['updateAppId'].response, 'sellingPlanGroupUpdate', 'updateAppId');
  assertGroupAppId(scenarios['updateAppId'].response, 'sellingPlanGroupUpdate', updateAppId, 'updateAppId');

  scenarios['readAfterUpdate'] = await capture('readAfterUpdate', readQuery, { id: groupId });
  assertReadAppId(scenarios['readAfterUpdate'].response, updateAppId, 'readAfterUpdate');

  scenarios['clearAppId'] = await capture('clearAppId', updateMutation, {
    id: groupId,
    input: { appId: null },
  });
  assertNoUserErrorsPayload(scenarios['clearAppId'].response, 'sellingPlanGroupUpdate', 'clearAppId');
  assertGroupAppId(scenarios['clearAppId'].response, 'sellingPlanGroupUpdate', null, 'clearAppId');

  scenarios['readAfterClear'] = await capture('readAfterClear', readQuery, { id: groupId });
  assertReadAppId(scenarios['readAfterClear'].response, null, 'readAfterClear');
} finally {
  if (groupId) {
    const result = await runGraphqlRaw(deleteMutation, { id: groupId });
    cleanup.push({
      label: 'cleanup sellingPlanGroupDelete',
      query: deleteMutation,
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
        'Captures Admin 2026-04 sellingPlanGroupCreate and sellingPlanGroupUpdate appId persistence.',
        'The script creates one disposable selling-plan group, reads appId after create, updates appId, clears appId with explicit null, reads after each mutation, then deletes the group during cleanup.',
      ],
      groupId,
      createAppId,
      updateAppId,
      scenarios,
      cleanup,
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(`Wrote selling-plan group appId readback fixture to ${outputPath}`);

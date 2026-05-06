/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'store-properties');
const specDir = path.join('config', 'parity-specs', 'store-properties');
const requestDir = path.join('config', 'parity-requests', 'store-properties');
const fixturePath = path.join(outputDir, 'shop-policy-update-subscription-blank-body.json');
const specPath = path.join(specDir, 'shop-policy-update-subscription-blank-body.json');
const mutationRequestPath = path.join(requestDir, 'shopPolicyUpdate-subscription-blank-body.graphql');
const downstreamRequestPath = path.join(requestDir, 'shopPolicyUpdate-subscription-blank-body-downstream-read.graphql');

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

function assertNoTopLevelErrors(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function readObject(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as Record<string, unknown>) : null;
}

function readShopPolicyUpdate(result: ConformanceGraphqlResult, context: string): Record<string, unknown> {
  const payload = readObject(result.payload);
  const data = readObject(payload?.['data']);
  const update = readObject(data?.['shopPolicyUpdate']);
  if (!update) {
    throw new Error(`${context} did not return shopPolicyUpdate data: ${JSON.stringify(result, null, 2)}`);
  }
  return update;
}

function readUserErrors(update: Record<string, unknown>, context: string): Array<Record<string, unknown>> {
  const userErrors = update['userErrors'];
  if (!Array.isArray(userErrors)) {
    throw new Error(`${context} did not return userErrors: ${JSON.stringify(update, null, 2)}`);
  }
  return userErrors.filter((error): error is Record<string, unknown> => !!readObject(error));
}

function assertSubscriptionBodyRequired(result: ConformanceGraphqlResult, context: string): void {
  const update = readShopPolicyUpdate(result, context);
  if (update['shopPolicy'] !== null) {
    throw new Error(`${context} returned shopPolicy instead of null: ${JSON.stringify(update, null, 2)}`);
  }

  const userErrors = readUserErrors(update, context);
  const [firstError, ...extraErrors] = userErrors;
  if (
    !firstError ||
    extraErrors.length > 0 ||
    JSON.stringify(firstError['field']) !== JSON.stringify(['shopPolicy', 'body']) ||
    firstError['message'] !== 'Purchase options cancellation policy required' ||
    firstError['code'] !== null
  ) {
    throw new Error(`${context} returned unexpected userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function readPolicies(result: ConformanceGraphqlResult): Array<Record<string, unknown>> {
  const payload = readObject(result.payload);
  const data = readObject(payload?.['data']);
  const shop = readObject(data?.['shop']);
  const policies = shop?.['shopPolicies'];
  return Array.isArray(policies)
    ? policies.filter((policy): policy is Record<string, unknown> => !!readObject(policy))
    : [];
}

function policyTypeBodies(result: ConformanceGraphqlResult): Array<{ type: string; body: string }> {
  return readPolicies(result).map((policy) => ({
    type: typeof policy['type'] === 'string' ? policy['type'] : '',
    body: typeof policy['body'] === 'string' ? policy['body'] : '',
  }));
}

function assertNoBlankSubscriptionPolicy(result: ConformanceGraphqlResult, context: string): void {
  const blankSubscriptionPolicies = policyTypeBodies(result).filter(
    (policy) => policy.type === 'SUBSCRIPTION_POLICY' && policy.body.trim() === '',
  );
  if (blankSubscriptionPolicies.length > 0) {
    throw new Error(`${context} included a blank subscription policy: ${JSON.stringify(blankSubscriptionPolicies)}`);
  }
}

const mutationDocument = `mutation ShopPolicyUpdate($shopPolicy: ShopPolicyInput!) {
  shopPolicyUpdate(shopPolicy: $shopPolicy) {
    shopPolicy {
      id
      type
      body
    }
    userErrors {
      field
      message
      code
    }
  }
}
`;

const downstreamReadDocument = `query ShopPolicySubscriptionBlankBodyDownstreamRead {
  shop {
    shopPolicies {
      type
      body
    }
  }
}
`;

const blankVariables = {
  shopPolicy: {
    type: 'SUBSCRIPTION_POLICY',
    body: '',
  },
};

const whitespaceVariables = {
  shopPolicy: {
    type: 'SUBSCRIPTION_POLICY',
    body: '   ',
  },
};

const beforeRead = await runGraphqlRequest(downstreamReadDocument, {});
assertNoTopLevelErrors(beforeRead, 'baseline shop policy downstream read');
assertNoBlankSubscriptionPolicy(beforeRead, 'baseline shop policy downstream read');

const blankMutation = await runGraphqlRequest(mutationDocument, blankVariables);
assertNoTopLevelErrors(blankMutation, 'blank subscription policy validation');
assertSubscriptionBodyRequired(blankMutation, 'blank subscription policy validation');

const whitespaceMutation = await runGraphqlRequest(mutationDocument, whitespaceVariables);
assertNoTopLevelErrors(whitespaceMutation, 'whitespace subscription policy validation');
assertSubscriptionBodyRequired(whitespaceMutation, 'whitespace subscription policy validation');

const downstreamRead = await runGraphqlRequest(downstreamReadDocument, {});
assertNoTopLevelErrors(downstreamRead, 'post-validation shop policy downstream read');
assertNoBlankSubscriptionPolicy(downstreamRead, 'post-validation shop policy downstream read');

const beforePolicies = policyTypeBodies(beforeRead);
const downstreamPolicies = policyTypeBodies(downstreamRead);
if (JSON.stringify(beforePolicies) !== JSON.stringify(downstreamPolicies)) {
  throw new Error(
    `Rejected subscription policy validations changed shopPolicies: before=${JSON.stringify(
      beforePolicies,
    )} after=${JSON.stringify(downstreamPolicies)}`,
  );
}

const fixture = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  beforeRead: {
    query: downstreamReadDocument,
    variables: {},
    response: beforeRead.payload,
  },
  blankMutation: {
    operationName: 'ShopPolicyUpdate',
    query: mutationDocument,
    variables: blankVariables,
    response: blankMutation.payload,
  },
  whitespaceMutation: {
    operationName: 'ShopPolicyUpdate',
    query: mutationDocument,
    variables: whitespaceVariables,
    response: whitespaceMutation.payload,
  },
  downstreamRead: {
    operationName: 'ShopPolicySubscriptionBlankBodyDownstreamRead',
    query: downstreamReadDocument,
    variables: {},
    response: downstreamRead.payload,
  },
  assertions: {
    policyTypeBodiesUnchanged: true,
    noBlankSubscriptionPolicyInDownstreamRead: true,
  },
  upstreamCalls: [],
};

const spec = {
  scenarioId: 'shop-policy-update-subscription-blank-body',
  operationNames: ['shopPolicyUpdate', 'shop'],
  scenarioStatus: 'captured',
  assertionKinds: ['user-errors-parity', 'downstream-read-parity'],
  liveCaptureFiles: [fixturePath],
  proxyRequest: {
    documentPath: mutationRequestPath,
    variablesCapturePath: '$.blankMutation.variables',
  },
  comparisonMode: 'captured-vs-proxy-request',
  notes:
    'Strict parity for Shopify subscription policy blank-body validation. The capture records empty and whitespace-only SUBSCRIPTION_POLICY bodies returning shopPolicy:null with the Purchase options cancellation policy required userError, plus an immediate downstream shop.shopPolicies read proving the rejected attempts did not create a blank subscription policy.',
  comparison: {
    mode: 'strict-json',
    expectedDifferences: [],
    targets: [
      {
        name: 'blank-body-validation-data',
        capturePath: '$.blankMutation.response.data',
        proxyPath: '$.data',
      },
      {
        name: 'whitespace-body-validation-data',
        capturePath: '$.whitespaceMutation.response.data',
        proxyPath: '$.data',
        proxyRequest: {
          documentPath: mutationRequestPath,
          variablesCapturePath: '$.whitespaceMutation.variables',
        },
      },
      {
        name: 'downstream-read-data',
        capturePath: '$.downstreamRead.response.data',
        proxyPath: '$.data',
        proxyRequest: {
          documentPath: downstreamRequestPath,
          variables: {},
        },
      },
    ],
  },
};

await mkdir(outputDir, { recursive: true });
await mkdir(specDir, { recursive: true });
await mkdir(requestDir, { recursive: true });
await writeFile(mutationRequestPath, mutationDocument, 'utf8');
await writeFile(downstreamRequestPath, downstreamReadDocument, 'utf8');
await writeFile(fixturePath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
await writeFile(specPath, `${JSON.stringify(spec, null, 2)}\n`, 'utf8');

console.log(`Wrote ${mutationRequestPath}`);
console.log(`Wrote ${downstreamRequestPath}`);
console.log(`Wrote ${fixturePath}`);
console.log(`Wrote ${specPath}`);

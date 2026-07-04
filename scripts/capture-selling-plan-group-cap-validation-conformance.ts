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

type CleanupRecord = {
  label: string;
  status: number;
  response: unknown;
};

const BASELINE_RESOURCE_GROUP_COUNT = 31;
const SELLING_PLANS_PER_GROUP_CAP = 31;
const CAP_REJECTION_COUNT = 32;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'selling-plans');
const outputPath = path.join(outputDir, 'selling-plan-group-cap-validation.json');
const paritySpecPath = path.join('config', 'parity-specs', 'selling-plans', 'sellingPlanGroup-cap-validation.json');
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const productCreateMutation = `#graphql
  mutation ProductCreateParityPlan($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
        id
        title
        handle
        status
        vendor
        productType
        tags
        descriptionHtml
        templateSuffix
        seo {
          title
          description
        }
      }
      shop {
        id
        name
        myshopifyDomain
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const sellingPlanGroupCreateMutation = `#graphql
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

const productJoinSellingPlanGroupsMutation = `#graphql
  mutation ProductRelationshipProductJoinSellingPlanGroups($id: ID!, $sellingPlanGroupIds: [ID!]!) {
    productJoinSellingPlanGroups(id: $id, sellingPlanGroupIds: $sellingPlanGroupIds) {
      product {
        id
        sellingPlanGroups(first: 5) {
          nodes {
            id
            name
            merchantCode
          }
        }
        sellingPlanGroupsCount {
          count
          precision
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

const sellingPlanGroupDeleteMutation = `#graphql
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

const productDeleteMutation = `#graphql
  mutation DeleteProduct($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
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

function payloadRoot(payload: unknown, rootName: string): JsonRecord {
  return readRecord(data(payload)[rootName]);
}

function userErrors(payload: unknown, rootName: string): unknown[] {
  return readArray(payloadRoot(payload, rootName)['userErrors']);
}

function assertNoTopLevelErrors(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || readRecord(result.payload)['errors']) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertUserErrors(result: ConformanceGraphqlResult, rootName: string, label: string): void {
  assertNoTopLevelErrors(result, label);
  if (userErrors(result.payload, rootName).length === 0) {
    throw new Error(`${label} did not return userErrors: ${JSON.stringify(result.payload, null, 2)}`);
  }
}

function assertNoUserErrors(result: ConformanceGraphqlResult, rootName: string, label: string): void {
  assertNoTopLevelErrors(result, label);
  const errors = userErrors(result.payload, rootName);
  if (errors.length !== 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(errors, null, 2)}`);
  }
}

async function capture(
  label: string,
  query: string,
  variables: JsonRecord,
  validate: (result: ConformanceGraphqlResult) => void,
): Promise<Scenario> {
  const result = await runGraphqlRaw(query, variables);
  validate(result);
  return {
    label,
    request: { query, variables },
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

function sellingPlanInput(index: number): JsonRecord {
  return {
    name: `Monthly delivery ${index}`,
    options: [`Monthly ${index}`],
    category: 'SUBSCRIPTION',
    billingPolicy: recurringPolicy(),
    deliveryPolicy: recurringPolicy(),
  };
}

function sellingPlanGroupInput(label: string, planCount = 1): JsonRecord {
  const merchantCode = label
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-|-$/g, '');
  return {
    name: label,
    merchantCode,
    options: ['Delivery frequency'],
    sellingPlansToCreate: Array.from({ length: planCount }, (_value, index) => sellingPlanInput(index + 1)),
  };
}

function createdProductId(payload: unknown): string {
  const product = readRecord(payloadRoot(payload, 'productCreate')['product']);
  const id = product['id'];
  if (typeof id !== 'string') {
    throw new Error(`Expected product id, got ${JSON.stringify(product)}`);
  }
  return id;
}

function createdGroupId(payload: unknown): string {
  const group = readRecord(payloadRoot(payload, 'sellingPlanGroupCreate')['sellingPlanGroup']);
  const id = group['id'];
  if (typeof id !== 'string') {
    throw new Error(`Expected selling plan group id, got ${JSON.stringify(group)}`);
  }
  return id;
}

function countFromProductJoin(payload: unknown): number {
  const product = readRecord(payloadRoot(payload, 'productJoinSellingPlanGroups')['product']);
  const count = readRecord(product['sellingPlanGroupsCount'])['count'];
  if (typeof count !== 'number') {
    throw new Error(`Expected numeric sellingPlanGroupsCount, got ${JSON.stringify(product)}`);
  }
  return count;
}

function proxyApiVersion(): string {
  return apiVersion;
}

function generatedGroupCreateTarget(index: number): JsonRecord {
  const targetName = `group-${String(index + 1).padStart(2, '0')}-create-user-errors`;
  return {
    name: targetName,
    capturePath: `$.scenarios.groupCreates[${index}].response.data.sellingPlanGroupCreate.userErrors`,
    proxyPath: '$.data.sellingPlanGroupCreate.userErrors',
    proxyRequest: {
      documentPath: 'config/parity-requests/selling-plans/sellingPlanGroupCreate-active-model-validation.graphql',
      variablesCapturePath: `$.scenarios.groupCreates[${index}].request.variables`,
      apiVersion: proxyApiVersion(),
    },
  };
}

function groupIdReference(index: number): JsonRecord {
  return {
    fromProxyResponse: `group-${String(index + 1).padStart(2, '0')}-create-user-errors`,
    path: '$.data.sellingPlanGroupCreate.sellingPlanGroup.id',
  };
}

function paritySpec(): JsonRecord {
  const groupCreateTargets = Array.from({ length: CAP_REJECTION_COUNT }, (_value, index) =>
    generatedGroupCreateTarget(index),
  );
  return {
    scenarioId: 'sellingPlanGroup-cap-validation',
    operationNames: ['productCreate', 'sellingPlanGroupCreate', 'productJoinSellingPlanGroups'],
    scenarioStatus: 'captured',
    assertionKinds: ['user-errors-parity', 'mutation-lifecycle'],
    liveCaptureFiles: [outputPath],
    runtimeTestFiles: ['tests/graphql_routes/selling_plans.rs'],
    proxyRequest: {
      documentPath: 'config/parity-requests/products/productCreate-parity-plan.graphql',
      variablesCapturePath: '$.scenarios.productSetup.request.variables',
      apiVersion: proxyApiVersion(),
    },
    comparisonMode: 'captured-vs-proxy-request',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        {
          name: 'setup-product-user-errors',
          capturePath: '$.scenarios.productSetup.response.data.productCreate.userErrors',
          proxyPath: '$.data.productCreate.userErrors',
        },
        {
          name: 'thirty-one-plans-create-user-errors',
          capturePath: '$.scenarios.thirtyOnePlansCreate.response.data.sellingPlanGroupCreate.userErrors',
          proxyPath: '$.data.sellingPlanGroupCreate.userErrors',
          proxyRequest: {
            documentPath: 'config/parity-requests/selling-plans/sellingPlanGroupCreate-active-model-validation.graphql',
            variablesCapturePath: '$.scenarios.thirtyOnePlansCreate.request.variables',
            apiVersion: proxyApiVersion(),
          },
        },
        {
          name: 'too-many-plans-create',
          capturePath: '$.scenarios.tooManyPlansCreate.response.data.sellingPlanGroupCreate',
          proxyPath: '$.data.sellingPlanGroupCreate',
          proxyRequest: {
            documentPath: 'config/parity-requests/selling-plans/sellingPlanGroupCreate-active-model-validation.graphql',
            variablesCapturePath: '$.scenarios.tooManyPlansCreate.request.variables',
            apiVersion: proxyApiVersion(),
          },
        },
        ...groupCreateTargets,
        {
          name: 'join-thirty-one-groups',
          capturePath: '$.scenarios.joinThirtyOneGroups.response.data.productJoinSellingPlanGroups',
          proxyPath: '$.data.productJoinSellingPlanGroups',
          proxyRequest: {
            documentPath:
              'config/parity-requests/products/product-relationship-product-join-selling-plan-groups.graphql',
            apiVersion: proxyApiVersion(),
            variables: {
              id: { fromPrimaryProxyPath: '$.data.productCreate.product.id' },
              sellingPlanGroupIds: Array.from({ length: BASELINE_RESOURCE_GROUP_COUNT }, (_value, index) =>
                groupIdReference(index),
              ),
            },
          },
          expectedDifferences: [
            {
              path: '$.product.id',
              matcher: 'shopify-gid:Product',
              reason:
                'The proxy creates a synthetic setup product ID while Shopify captured a real disposable product ID.',
            },
            {
              path: '$.product.sellingPlanGroups.nodes[*].id',
              matcher: 'shopify-gid:SellingPlanGroup',
              reason:
                'The proxy creates synthetic selling-plan group IDs while Shopify captured real disposable group IDs.',
            },
          ],
        },
        {
          name: 'join-thirty-two-groups',
          capturePath: '$.scenarios.joinThirtyTwoGroups.response.data.productJoinSellingPlanGroups',
          proxyPath: '$.data.productJoinSellingPlanGroups',
          proxyRequest: {
            documentPath:
              'config/parity-requests/products/product-relationship-product-join-selling-plan-groups.graphql',
            apiVersion: proxyApiVersion(),
            variables: {
              id: { fromPrimaryProxyPath: '$.data.productCreate.product.id' },
              sellingPlanGroupIds: [groupIdReference(BASELINE_RESOURCE_GROUP_COUNT)],
            },
          },
          expectedDifferences: [
            {
              path: '$.product.id',
              matcher: 'shopify-gid:Product',
              reason:
                'The proxy creates a synthetic setup product ID while Shopify captured a real disposable product ID.',
            },
            {
              path: '$.product.sellingPlanGroups.nodes[*].id',
              matcher: 'shopify-gid:SellingPlanGroup',
              reason:
                'The proxy creates synthetic selling-plan group IDs while Shopify captured real disposable group IDs.',
            },
          ],
        },
      ],
    },
    notes:
      'Captured 2026-04 Shopify responses confirm sellingPlanGroupCreate accepts 31 selling plans and rejects 32, while productJoinSellingPlanGroups accepts at least 32 selling-plan groups for one product with empty userErrors.',
  };
}

const suffix = Date.now().toString(36);
const cleanup: CleanupRecord[] = [];
const cleanupGroupIds: string[] = [];
const resourceGroupIds: string[] = [];
const groupCreates: Scenario[] = [];
let productId: string | null = null;
let thirtyOnePlansCreate: Scenario | null = null;
let tooManyPlansCreate: Scenario | null = null;
let productSetup: Scenario | null = null;
let joinThirtyOneGroups: Scenario | null = null;
let joinThirtyTwoGroups: Scenario | null = null;

try {
  thirtyOnePlansCreate = await capture(
    'sellingPlanGroupCreate accepts 31 selling plans',
    sellingPlanGroupCreateMutation,
    {
      input: sellingPlanGroupInput(`Cap validation 31 plans ${suffix}`, SELLING_PLANS_PER_GROUP_CAP),
    },
    (result) => assertNoUserErrors(result, 'sellingPlanGroupCreate', 'sellingPlanGroupCreate accepts 31 selling plans'),
  );
  cleanupGroupIds.push(createdGroupId(thirtyOnePlansCreate.response));

  tooManyPlansCreate = await capture(
    'sellingPlanGroupCreate rejects 32 selling plans',
    sellingPlanGroupCreateMutation,
    {
      input: sellingPlanGroupInput(`Cap validation too many plans ${suffix}`, CAP_REJECTION_COUNT),
    },
    (result) => assertUserErrors(result, 'sellingPlanGroupCreate', 'sellingPlanGroupCreate rejects 32 selling plans'),
  );

  productSetup = await capture(
    'productCreate resource cap setup',
    productCreateMutation,
    { product: { title: `Selling plan cap product ${suffix}`, status: 'DRAFT' } },
    (result) => assertNoUserErrors(result, 'productCreate', 'productCreate resource cap setup'),
  );
  productId = createdProductId(productSetup.response);

  for (let index = 0; index < CAP_REJECTION_COUNT; index += 1) {
    const label = `Cap validation group ${String(index + 1).padStart(2, '0')} ${suffix}`;
    const groupCreate = await capture(
      `sellingPlanGroupCreate setup group ${index + 1}`,
      sellingPlanGroupCreateMutation,
      { input: sellingPlanGroupInput(label) },
      (result) =>
        assertNoUserErrors(result, 'sellingPlanGroupCreate', `sellingPlanGroupCreate setup group ${index + 1}`),
    );
    const groupId = createdGroupId(groupCreate.response);
    resourceGroupIds.push(groupId);
    cleanupGroupIds.push(groupId);
    groupCreates.push(groupCreate);
  }

  joinThirtyOneGroups = await capture(
    'productJoinSellingPlanGroups accepts 31 groups',
    productJoinSellingPlanGroupsMutation,
    { id: productId, sellingPlanGroupIds: resourceGroupIds.slice(0, BASELINE_RESOURCE_GROUP_COUNT) },
    (result) => {
      assertNoUserErrors(result, 'productJoinSellingPlanGroups', 'productJoinSellingPlanGroups accepts 31 groups');
      const count = countFromProductJoin(result.payload);
      if (count !== BASELINE_RESOURCE_GROUP_COUNT) {
        throw new Error(`Expected 31 joined groups, got ${count}: ${JSON.stringify(result.payload, null, 2)}`);
      }
    },
  );

  joinThirtyTwoGroups = await capture(
    'productJoinSellingPlanGroups accepts 32nd group',
    productJoinSellingPlanGroupsMutation,
    { id: productId, sellingPlanGroupIds: [resourceGroupIds[BASELINE_RESOURCE_GROUP_COUNT]] },
    (result) => {
      assertNoUserErrors(result, 'productJoinSellingPlanGroups', 'productJoinSellingPlanGroups accepts 32nd group');
      const count = countFromProductJoin(result.payload);
      if (count !== CAP_REJECTION_COUNT) {
        throw new Error(`Expected 32 joined groups, got ${count}: ${JSON.stringify(result.payload, null, 2)}`);
      }
    },
  );
} finally {
  for (const id of [...cleanupGroupIds].reverse()) {
    const result = await runGraphqlRaw(sellingPlanGroupDeleteMutation, { id });
    cleanup.push({ label: `cleanup sellingPlanGroupDelete ${id}`, status: result.status, response: result.payload });
  }
  if (productId) {
    const result = await runGraphqlRaw(productDeleteMutation, { input: { id: productId } });
    cleanup.push({ label: `cleanup productDelete ${productId}`, status: result.status, response: result.payload });
  }
}

if (!thirtyOnePlansCreate || !tooManyPlansCreate || !productSetup || !joinThirtyOneGroups || !joinThirtyTwoGroups) {
  throw new Error('Capture did not complete all required selling-plan cap scenarios.');
}

await mkdir(outputDir, { recursive: true });
await mkdir(path.dirname(paritySpecPath), { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      notes:
        'Real Shopify Admin GraphQL cap capture for selling-plan group plan count and selling-plan groups per product resource.',
      confirmedCaps: {
        sellingPlansPerGroup: {
          acceptedCount: SELLING_PLANS_PER_GROUP_CAP,
          rejectedCount: CAP_REJECTION_COUNT,
          acceptedScenario: 'thirtyOnePlansCreate',
          rejectedScenario: 'tooManyPlansCreate',
        },
        sellingPlanGroupsPerProductResource: {
          observedAcceptedCount: CAP_REJECTION_COUNT,
          rejectedCount: null,
          acceptedScenario: 'joinThirtyOneGroups',
          extendedAcceptedScenario: 'joinThirtyTwoGroups',
          notes:
            'Public Admin GraphQL accepted 32 groups for one product; the old local 31-group cap is not a public Shopify cap.',
        },
      },
      scenarios: {
        thirtyOnePlansCreate,
        tooManyPlansCreate,
        productSetup,
        groupCreates,
        joinThirtyOneGroups,
        joinThirtyTwoGroups,
      },
      cleanup,
    },
    null,
    2,
  )}\n`,
);
await writeFile(paritySpecPath, `${JSON.stringify(paritySpec(), null, 2)}\n`);

console.log(`Wrote ${outputPath}`);
console.log(`Wrote ${paritySpecPath}`);

/* oxlint-disable no-console -- CLI capture scripts intentionally write status output. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;
type Capture = {
  query: string;
  variables: JsonRecord;
  response: ConformanceGraphqlResult;
};

const validationFunctionHandle = 'conformance-validation';
const cartFunctionHandle = 'conformance-cart-transform';
const secondCartFunctionHandle = 'conformance-cart-transform-secondary';
const disposableTitlePrefix = 'Authoritative lifecycle preflight';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'functions');
const outputPath = path.join(outputDir, 'functions-authoritative-lifecycle-preflight.json');
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function capture(query: string, variables: JsonRecord = {}): Promise<Capture> {
  return { query, variables, response: await runGraphqlRequest(query, variables) };
}

function readRecord(value: unknown): JsonRecord {
  return value !== null && typeof value === 'object' && !Array.isArray(value) ? (value as JsonRecord) : {};
}

function readArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function readString(value: unknown): string | null {
  return typeof value === 'string' && value.length > 0 ? value : null;
}

function readPath(value: unknown, segments: string[]): unknown {
  let current = value;
  for (const segment of segments) current = readRecord(current)[segment];
  return current;
}

function assertNoTopLevelErrors(result: Capture, context: string): void {
  if (result.response.status < 200 || result.response.status >= 300 || readRecord(result.response.payload)['errors']) {
    throw new Error(`${context} failed: ${JSON.stringify(result.response, null, 2)}`);
  }
}

function assertNoUserErrors(result: Capture, root: string, context: string): void {
  assertNoTopLevelErrors(result, context);
  const errors = readArray(readPath(result.response.payload, ['data', root, 'userErrors']));
  if (errors.length > 0) throw new Error(`${context} returned userErrors: ${JSON.stringify(errors, null, 2)}`);
}

function requireFunction(nodes: JsonRecord[], handle: string): JsonRecord {
  const node = nodes.find((candidate) => candidate['handle'] === handle);
  if (!readString(node?.['id'])) {
    throw new Error(`Missing released Function ${handle}: ${JSON.stringify(nodes, null, 2)}`);
  }
  return node ?? {};
}

function assertUserError(
  result: Capture,
  root: string,
  expected: { code: string | null; field: string[] | null; message: string },
): void {
  assertNoTopLevelErrors(result, root);
  const payload = readRecord(readPath(result.response.payload, ['data', root]));
  const errors = readArray(payload['userErrors']);
  const actual = readRecord(errors[0]);
  if (
    errors.length !== 1 ||
    actual['code'] !== expected.code ||
    actual['message'] !== expected.message ||
    JSON.stringify(actual['field']) !== JSON.stringify(expected.field)
  ) {
    throw new Error(`${root} mismatch: ${JSON.stringify({ expected, payload }, null, 2)}`);
  }
}

const functionCatalogDocument = `query AuthoritativeLifecycleFunctionCatalog {
  shopifyFunctions(first: 100) {
    nodes {
      id
      title
      handle
      apiType
      description
      appKey
      app {
        __typename
        id
        title
        handle
        apiKey
      }
    }
  }
}
`;

const inventoryDocument = `query AuthoritativeLifecycleInventory {
  validations(first: 100) {
    nodes { id title }
  }
  cartTransforms(first: 100) {
    nodes { id functionId }
  }
}
`;

const validationCreateDocument = `mutation AuthoritativeLifecycleValidationSetup($validation: ValidationCreateInput!) {
  validationCreate(validation: $validation) {
    validation { id }
    userErrors { field message code }
  }
}
`;

const validationDeleteDocument = `mutation AuthoritativeLifecycleValidationCleanup($id: ID!) {
  validationDelete(id: $id) {
    deletedId
    userErrors { field message code }
  }
}
`;

const cartCreateDocument = `mutation AuthoritativeLifecycleCartSetup($functionId: String!) {
  cartTransformCreate(functionId: $functionId) {
    cartTransform { id }
    userErrors { field message code }
  }
}
`;

const cartDeleteDocument = `mutation AuthoritativeLifecycleCartCleanup($id: ID!) {
  cartTransformDelete(id: $id) {
    deletedId
    userErrors { field message code }
  }
}
`;

const preflightDocument = `mutation AuthoritativeLifecyclePreflight(
  $validationFunctionId: String!
  $secondCartFunctionId: String!
) {
  validationLimit: validationCreate(
    validation: {
      functionId: $validationFunctionId
      title: "Twenty sixth active validation"
      enable: true
    }
  ) {
    validation { id }
    userErrors { field message code }
  }
  reusedValidationFunction: cartTransformCreate(functionId: $validationFunctionId) {
    cartTransform { id }
    userErrors { field message code }
  }
  secondCartTransform: cartTransformCreate(functionId: $secondCartFunctionId) {
    cartTransform { id }
    userErrors { field message code }
  }
}
`;

const functionValidationDecisionPreflightDocument = `query FunctionValidationDecisionPreflight($after: String) {
  validations(first: 250, after: $after) {
    nodes {
      id
      enabled
      shopifyFunction {
        id
      }
    }
    pageInfo {
      hasNextPage
      endCursor
    }
  }
}
`;

const functionCartTransformDecisionPreflightDocument = `query FunctionCartTransformDecisionPreflight {
  cartTransforms(first: 1) {
    nodes {
      id
      functionId
    }
  }
}
`;

const functionHydrateByIdDocument = `query FunctionHydrateById($id: String!) {
  shopifyFunction(id: $id) {
    id
    title
    apiType
    description
    appKey
    app {
      __typename
      id
      title
      apiKey
    }
  }
}
`;

const functionCatalog = await capture(functionCatalogDocument);
assertNoTopLevelErrors(functionCatalog, 'Function catalog');
const functionNodes = readArray(readPath(functionCatalog.response.payload, ['data', 'shopifyFunctions', 'nodes'])).map(
  readRecord,
);
const validationFunction = requireFunction(functionNodes, validationFunctionHandle);
const cartFunction = requireFunction(functionNodes, cartFunctionHandle);
const secondCartFunction = requireFunction(functionNodes, secondCartFunctionHandle);
const validationFunctionId = readString(validationFunction['id'])!;
const cartFunctionId = readString(cartFunction['id'])!;
const secondCartFunctionId = readString(secondCartFunction['id'])!;

const cleanupBefore: Capture[] = [];
const initialInventory = await capture(inventoryDocument);
assertNoTopLevelErrors(initialInventory, 'Initial lifecycle inventory');
for (const validation of readArray(readPath(initialInventory.response.payload, ['data', 'validations', 'nodes']))) {
  const id = readString(readRecord(validation)['id']);
  if (id) cleanupBefore.push(await capture(validationDeleteDocument, { id }));
}
for (const transform of readArray(readPath(initialInventory.response.payload, ['data', 'cartTransforms', 'nodes']))) {
  const id = readString(readRecord(transform)['id']);
  if (id) cleanupBefore.push(await capture(cartDeleteDocument, { id }));
}

const validationIds: string[] = [];
let cartTransformId: string | null = null;
const cleanupAfter: Capture[] = [];
let fixture: JsonRecord | null = null;

try {
  const validationSetup: Capture[] = [];
  for (let index = 1; index <= 25; index += 1) {
    const created = await capture(validationCreateDocument, {
      validation: {
        functionId: validationFunctionId,
        title: `${disposableTitlePrefix} ${index}`,
        enable: true,
        blockOnFailure: false,
      },
    });
    assertNoUserErrors(created, 'validationCreate', `Validation setup ${index}`);
    const id = readString(readPath(created.response.payload, ['data', 'validationCreate', 'validation', 'id']));
    if (!id) throw new Error(`Validation setup ${index} returned no id`);
    validationIds.push(id);
    validationSetup.push(created);
  }

  const cartSetup = await capture(cartCreateDocument, { functionId: cartFunctionId });
  assertNoUserErrors(cartSetup, 'cartTransformCreate', 'Cart transform setup');
  cartTransformId = readString(
    readPath(cartSetup.response.payload, ['data', 'cartTransformCreate', 'cartTransform', 'id']),
  );
  if (!cartTransformId) throw new Error('Cart transform setup returned no id');

  const functionValidationDecisionPreflight = await capture(functionValidationDecisionPreflightDocument, {
    after: null,
  });
  assertNoTopLevelErrors(functionValidationDecisionPreflight, 'Validation decision cassette');
  const validationNodes = readArray(
    readPath(functionValidationDecisionPreflight.response.payload, ['data', 'validations', 'nodes']),
  );
  if (validationNodes.length !== 25) throw new Error(`Expected 25 validation nodes, got ${validationNodes.length}`);

  const validationFunctionHydrate = await capture(functionHydrateByIdDocument, { id: validationFunctionId });
  assertNoTopLevelErrors(validationFunctionHydrate, 'Validation Function cassette');

  const functionCartTransformDecisionPreflight = await capture(functionCartTransformDecisionPreflightDocument);
  assertNoTopLevelErrors(functionCartTransformDecisionPreflight, 'Cart-transform decision cassette');
  const cartNodes = readArray(
    readPath(functionCartTransformDecisionPreflight.response.payload, ['data', 'cartTransforms', 'nodes']),
  );
  if (cartNodes.length !== 1) throw new Error(`Expected one cart-transform node, got ${cartNodes.length}`);

  const secondCartFunctionHydrate = await capture(functionHydrateByIdDocument, { id: secondCartFunctionId });
  assertNoTopLevelErrors(secondCartFunctionHydrate, 'Second cart Function cassette');

  const preflight = await capture(preflightDocument, { validationFunctionId, secondCartFunctionId });
  assertUserError(preflight, 'validationLimit', {
    code: 'MAX_VALIDATIONS_ACTIVATED',
    field: null,
    message: 'Cannot have more than 25 active validation functions.',
  });
  assertUserError(preflight, 'reusedValidationFunction', {
    code: 'FUNCTION_ALREADY_REGISTERED',
    field: ['functionId'],
    message: 'Could not enable cart transform because it is already registered',
  });
  assertUserError(preflight, 'secondCartTransform', {
    code: null,
    field: null,
    message: 'An API client cannot have more than 1 cart transform functions per shop',
  });

  fixture = {
    scenarioId: 'functions-authoritative-lifecycle-preflight',
    capturedAt: new Date().toISOString(),
    source: 'live-shopify',
    storeDomain,
    apiVersion,
    summary:
      'Live global lifecycle limits and registered-Function precedence with 25 active validations and one existing cart transform.',
    functions: { validationFunction, cartFunction, secondCartFunction },
    initialInventory,
    cleanupBefore,
    validationSetup,
    cartSetup,
    functionValidationDecisionPreflight,
    validationFunctionHydrate,
    functionCartTransformDecisionPreflight,
    secondCartFunctionHydrate,
    preflight,
    cleanupAfter,
    upstreamCalls: [
      {
        operationName: 'FunctionValidationDecisionPreflight',
        variables: functionValidationDecisionPreflight.variables,
        query: functionValidationDecisionPreflight.query,
        response: {
          status: functionValidationDecisionPreflight.response.status,
          body: functionValidationDecisionPreflight.response.payload,
        },
      },
      {
        operationName: 'FunctionHydrateById',
        variables: validationFunctionHydrate.variables,
        query: validationFunctionHydrate.query,
        response: {
          status: validationFunctionHydrate.response.status,
          body: validationFunctionHydrate.response.payload,
        },
      },
      {
        operationName: 'FunctionHydrateById',
        variables: { id: secondCartFunctionId },
        query: secondCartFunctionHydrate.query,
        response: {
          status: secondCartFunctionHydrate.response.status,
          body: secondCartFunctionHydrate.response.payload,
        },
      },
      {
        operationName: 'FunctionCartTransformDecisionPreflight',
        variables: functionCartTransformDecisionPreflight.variables,
        query: functionCartTransformDecisionPreflight.query,
        response: {
          status: functionCartTransformDecisionPreflight.response.status,
          body: functionCartTransformDecisionPreflight.response.payload,
        },
      },
    ],
    notes: {
      authoritativeDecisions:
        'The replay starts cold and reaches the three Shopify payloads through a 25-active threshold page, two exact Function lookups, and a first-cart-transform existence probe without hydrating lifecycle objects.',
      cleanup:
        'The script clears the disposable lifecycle catalogs before setup and deletes every created Validation and CartTransform in finally.',
    },
  };
} finally {
  if (cartTransformId) cleanupAfter.push(await capture(cartDeleteDocument, { id: cartTransformId }));
  for (const id of validationIds.reverse()) cleanupAfter.push(await capture(validationDeleteDocument, { id }));
  for (const cleanup of cleanupAfter) assertNoTopLevelErrors(cleanup, 'Lifecycle cleanup');
}

if (!fixture) throw new Error('Lifecycle capture did not produce a fixture');
await mkdir(outputDir, { recursive: true });
await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
console.log(`Wrote ${outputPath}`);

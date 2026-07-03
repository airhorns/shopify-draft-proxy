/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'functions');
const outputPath = path.join(outputDir, 'functions-non-catalog-hydrate-validation-create.json');
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const functionCatalogDocument = `#graphql
  query FunctionsNonCatalogHydrateCatalog {
    shopifyFunctions(first: 100) {
      nodes {
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
      pageInfo {
        hasNextPage
        endCursor
      }
    }
  }
`;

const validationCreateDocument = `#graphql
  mutation FunctionsNonCatalogHydrateValidationCreate($validation: ValidationCreateInput!) {
    validationCreate(validation: $validation) {
      validation {
        id
        title
        enabled
        blockOnFailure
        shopifyFunction {
          id
          title
          apiType
          appKey
          app {
            __typename
            id
            title
            apiKey
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

const functionHydrateByIdDocument =
  'query FunctionHydrateById($id: String!) {\n  shopifyFunction(id: $id) {\n    id\n    title\n    apiType\n    description\n    appKey\n    app {\n      __typename\n      id\n      title\n      apiKey\n    }\n  }\n}\n';

function readRecord(value: unknown): JsonRecord | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function readString(value: unknown): string | null {
  return typeof value === 'string' && value.length > 0 ? value : null;
}

function assertNoTopLevelErrors(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300 || readRecord(result.payload)?.['errors']) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function readFunctionNodes(result: ConformanceGraphqlResult): JsonRecord[] {
  const data = readRecord(readRecord(result.payload)?.['data']);
  const connection = readRecord(data?.['shopifyFunctions']);
  return readArray(connection?.['nodes']).flatMap((node) => {
    const record = readRecord(node);
    return record ? [record] : [];
  });
}

function findNonCatalogWrongApiFunction(nodes: JsonRecord[]): JsonRecord {
  const removedCatalogHandles = new Set([
    'validation-local',
    'validation-alpha',
    'validation-beta',
    'conformance-validation',
    'validation-owned',
    'cart-transform-local',
    'cart-beta',
    'conformance-cart-transform',
    'cart-transform-delete-shape',
    'cart-owned',
    'fulfillment-constraint-local',
  ]);
  const candidates = nodes.filter((candidate) => {
    const handle = readString(candidate['handle']) ?? readString(candidate['title']);
    const apiType = readString(candidate['apiType']);
    return Boolean(handle && apiType && apiType !== 'cart_checkout_validation' && !removedCatalogHandles.has(handle));
  });
  const node = candidates.find((candidate) => candidate['title'] === 'conformance-discount') ?? candidates[0];
  if (!node) {
    throw new Error(
      `Expected at least one released non-validation ShopifyFunction outside the removed local catalog; saw ${JSON.stringify(
        nodes,
        null,
        2,
      )}`,
    );
  }
  return node;
}

function validationCreatePayload(result: ConformanceGraphqlResult): JsonRecord {
  const data = readRecord(readRecord(result.payload)?.['data']);
  const payload = readRecord(data?.['validationCreate']);
  if (!payload) {
    throw new Error(`validationCreate returned no payload: ${JSON.stringify(result, null, 2)}`);
  }
  return payload;
}

function assertWrongApiValidationPayload(result: ConformanceGraphqlResult): void {
  const payload = validationCreatePayload(result);
  if (payload['validation'] !== null) {
    throw new Error(`Expected validationCreate validation:null, got ${JSON.stringify(payload, null, 2)}`);
  }
  const errors = readArray(payload['userErrors']);
  const first = readRecord(errors[0]);
  if (errors.length !== 1 || first?.['code'] !== 'FUNCTION_DOES_NOT_IMPLEMENT') {
    throw new Error(`Expected FUNCTION_DOES_NOT_IMPLEMENT userError, got ${JSON.stringify(payload, null, 2)}`);
  }
}

async function capture(
  query: string,
  variables: JsonRecord = {},
): Promise<{
  query: string;
  variables: JsonRecord;
  response: ConformanceGraphqlResult;
}> {
  return {
    query,
    variables,
    response: await runGraphqlRequest(query, variables),
  };
}

const functionCatalog = await capture(functionCatalogDocument);
assertNoTopLevelErrors(functionCatalog.response, 'Function catalog');
const selectedFunction = findNonCatalogWrongApiFunction(readFunctionNodes(functionCatalog.response));
const selectedFunctionId = readString(selectedFunction['id']);
if (!selectedFunctionId) {
  throw new Error(`Selected Function has no id: ${JSON.stringify(selectedFunction, null, 2)}`);
}

const validationWrongApi = await capture(validationCreateDocument, {
  validation: {
    functionId: selectedFunctionId,
    title: 'Non-catalog wrong API validation probe',
  },
});
assertNoTopLevelErrors(validationWrongApi.response, 'Non-catalog wrong API validationCreate');
assertWrongApiValidationPayload(validationWrongApi.response);

const fixture = {
  scenarioId: 'functions-non-catalog-hydrate-validation-create',
  capturedAt: new Date().toISOString(),
  source: 'live-shopify',
  storeDomain,
  apiVersion,
  summary:
    'Live validationCreate evidence for a released ShopifyFunction id outside the removed local Functions catalog. Proxy replay must hydrate the Function through the upstreamCalls cassette before returning Shopify-compatible wrong-API userErrors.',
  selectedFunction,
  functionCatalog,
  validationWrongApi,
  upstreamCalls: [
    {
      operationName: 'FunctionHydrateById',
      variables: {
        id: selectedFunctionId,
      },
      query: functionHydrateByIdDocument,
      response: {
        status: 200,
        body: {
          data: {
            shopifyFunction: selectedFunction,
          },
        },
      },
    },
  ],
};

await mkdir(outputDir, { recursive: true });
await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
console.log(`Wrote ${outputPath}`);

import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlPayload } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type CapturedResponse = {
  status: number;
  payload: ConformanceGraphqlPayload;
};

type ValidationCase = {
  query: string;
  variables?: Record<string, unknown>;
  response: CapturedResponse;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2025-01',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const fixturePath = path.join(outputDir, 'product-status-enum-validation.json');

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function readRequestDocument(fileName: string): Promise<string> {
  return await readFile(path.join('config', 'parity-requests', 'products', fileName), 'utf8');
}

async function captureCase(query: string, variables: Record<string, unknown> = {}): Promise<ValidationCase> {
  const response = await runGraphqlRequest(query, variables);
  return {
    query,
    ...(Object.keys(variables).length > 0 ? { variables } : {}),
    response: {
      status: response.status,
      payload: response.payload,
    },
  };
}

const productChangeStatusInvalidLiteralQuery = await readRequestDocument(
  'productChangeStatus-invalid-status-literal.graphql',
);
const productChangeStatusInvalidVariableQuery = await readRequestDocument(
  'productChangeStatus-invalid-status-variable.graphql',
);
const productCreateInvalidLiteralQuery = await readRequestDocument('productCreate-invalid-status-literal.graphql');
const productCreateInvalidVariableQuery = await readRequestDocument('productCreate-invalid-status-variable.graphql');

const validation = {
  productChangeStatusInvalidLiteral: await captureCase(productChangeStatusInvalidLiteralQuery),
  productChangeStatusInvalidVariable: await captureCase(productChangeStatusInvalidVariableQuery, {
    productId: 'gid://shopify/Product/999999999999999',
    status: 'ENABLED',
  }),
  productCreateInvalidLiteral: await captureCase(productCreateInvalidLiteralQuery),
  productCreateInvalidVariable: await captureCase(productCreateInvalidVariableQuery, {
    product: {
      title: 'Invalid status probe',
      status: 'ENABLED',
    },
  }),
};

await mkdir(outputDir, { recursive: true });
await writeFile(
  fixturePath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      apiVersion,
      storeDomain,
      validation,
      notes: [
        'Captured against real Shopify Admin GraphQL. Invalid ProductStatus values fail schema validation before productChangeStatus/productCreate resolver execution, so no products are created or status changes staged upstream.',
      ],
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
  'utf8',
);

// oxlint-disable-next-line no-console -- CLI capture result is intentionally written to stdout.
console.log(JSON.stringify({ ok: true, storeDomain, apiVersion, fixturePath }, null, 2));

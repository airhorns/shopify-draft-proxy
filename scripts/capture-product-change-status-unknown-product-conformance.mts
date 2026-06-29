import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlPayload } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type ProductChangeStatusUnknownData = {
  productChangeStatus?: {
    product?: unknown;
    userErrors?: Array<{
      field?: string[] | null;
      message?: string | null;
      code?: string | null;
    }>;
  } | null;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const fixturePath = path.join(outputDir, 'product-change-status-unknown-product-parity.json');
const documentPath = path.join('config', 'parity-requests', 'products', 'productChangeStatus-parity-plan.graphql');
const query = await readFile(documentPath, 'utf8');
const variables = {
  productId: 'gid://shopify/Product/999999999999999',
  status: 'ARCHIVED',
};

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const response = await runGraphqlRequest<ProductChangeStatusUnknownData>(query, variables);
if (response.status < 200 || response.status >= 300 || response.payload.errors) {
  throw new Error(`productChangeStatus unknown-product capture failed: ${JSON.stringify(response, null, 2)}`);
}

const userErrors = response.payload.data?.productChangeStatus?.userErrors ?? [];
const [notFoundError] = userErrors;
if (
  response.payload.data?.productChangeStatus?.product !== null ||
  userErrors.length !== 1 ||
  JSON.stringify(notFoundError?.field) !== JSON.stringify(['productId']) ||
  notFoundError?.message !== 'Product does not exist' ||
  notFoundError?.code !== 'PRODUCT_NOT_FOUND'
) {
  throw new Error(
    `Unexpected productChangeStatus unknown-product response: ${JSON.stringify(response.payload, null, 2)}`,
  );
}

const fixture: {
  capturedAt: string;
  apiVersion: string;
  storeDomain: string;
  mutation: {
    query: string;
    variables: typeof variables;
    response: ConformanceGraphqlPayload<ProductChangeStatusUnknownData>;
  };
  upstreamCalls: [];
  notes: string[];
} = {
  capturedAt: new Date().toISOString(),
  apiVersion,
  storeDomain,
  mutation: {
    query,
    variables,
    response: response.payload,
  },
  upstreamCalls: [],
  notes: [
    'Captured against real Shopify Admin GraphQL. Unknown productChangeStatus product ids return a resolver-level Product does not exist userError with code PRODUCT_NOT_FOUND and no product payload.',
  ],
};

await mkdir(outputDir, { recursive: true });
await writeFile(fixturePath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');

// oxlint-disable-next-line no-console -- CLI capture result is intentionally written to stdout.
console.log(JSON.stringify({ ok: true, storeDomain, apiVersion, fixturePath }, null, 2));

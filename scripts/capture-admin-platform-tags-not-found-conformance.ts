/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type CaptureCase = {
  request: {
    documentPath: string;
    query: string;
    variables: Record<string, unknown>;
  };
  response: {
    status: number;
    payload: unknown;
  };
};

const scenarioId = 'admin-platform-tags-not-found';
const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2025-01',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'admin-platform');
const outputPath = path.join(outputDir, `${scenarioId}.json`);

const addDocumentPath = 'config/parity-requests/admin-platform/admin-platform-tags-add-not-found.graphql';
const removeDocumentPath = 'config/parity-requests/admin-platform/admin-platform-tags-remove-not-found.graphql';
const addDocument = await readFile(addDocumentPath, 'utf8');
const removeDocument = await readFile(removeDocumentPath, 'utf8');

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function capture(documentPath: string, query: string, variables: Record<string, unknown>): Promise<CaptureCase> {
  const result: ConformanceGraphqlResult = await runGraphqlRequest(query, variables);
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`Unexpected Shopify response for ${documentPath}: ${JSON.stringify(result, null, 2)}`);
  }

  return {
    request: { documentPath, query, variables },
    response: {
      status: result.status,
      payload: result.payload,
    },
  };
}

const productId = 'gid://shopify/Product/999999999999999';
const customerId = 'gid://shopify/Customer/999999999999999';
const tags = ['vip'];

const cases = {
  productAdd: await capture(addDocumentPath, addDocument, { id: productId, tags }),
  productRemove: await capture(removeDocumentPath, removeDocument, { id: productId, tags }),
  customerAdd: await capture(addDocumentPath, addDocument, { id: customerId, tags }),
  customerRemove: await capture(removeDocumentPath, removeDocument, { id: customerId, tags }),
};

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      scenarioId,
      capturedAt: new Date().toISOString(),
      source: 'live-shopify-admin-graphql',
      storeDomain,
      apiVersion,
      liveGatewaySideEffects: false,
      notes:
        'Well-formed Product and Customer GIDs that do not resolve return payload-level tag userErrors instead of top-level transport errors. Requests select the public field/message UserError shape exposed by Admin GraphQL 2025-01.',
      cases,
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(`Wrote ${outputPath}`);

/* oxlint-disable no-console -- CLI capture script intentionally reports progress. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metafields');
const outputPath = path.join(outputDir, 'standard-metafield-definition-enable-material.json');
const documentPath = 'config/parity-requests/metafields/standard-metafield-definition-enable-material.graphql';

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const document = await readFile(documentPath, 'utf8');
const result = await runGraphqlRequest(document);

if (result.status < 200 || result.status >= 300 || result.payload.errors) {
  throw new Error(
    `standardMetafieldDefinitionEnable material capture failed: ${JSON.stringify(
      { status: result.status, payload: result.payload },
      null,
      2,
    )}`,
  );
}

const payload = {
  storeDomain,
  apiVersion,
  capturedAt: new Date().toISOString(),
  materialEnable: {
    request: {
      query: document,
      variables: {},
    },
    status: result.status,
    response: result.payload,
  },
};

await mkdir(outputDir, { recursive: true });
await writeFile(outputPath, `${JSON.stringify(payload, null, 2)}\n`, 'utf8');
console.log(`Wrote ${outputPath}`);

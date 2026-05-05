/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { readConformanceScriptConfig } from './conformance-script-config.js';
import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { assertDiscountConformanceScopes, probeDiscountConformanceScopes } from './discount-conformance-lib.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'discounts');
const outputPath = path.join(outputDir, 'discount-bulk-selector-validation.json');
const document = await readFile('config/parity-requests/discounts/discount-bulk-selector-validation.graphql', 'utf8');

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const adminOptions = {
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
};
const { runGraphqlRaw } = createAdminGraphqlClient(adminOptions);

const scopeProbe = await probeDiscountConformanceScopes(adminOptions);
assertDiscountConformanceScopes(scopeProbe);

const variables = {
  codeIds: ['gid://shopify/DiscountCodeNode/0'],
  automaticIds: ['gid://shopify/DiscountAutomaticNode/0'],
  search: 'status:active',
  savedSearchId: 'gid://shopify/SavedSearch/0',
};

const response = await runGraphqlRaw(document, variables);

const fixture = {
  storeDomain,
  apiVersion,
  accessScopes: scopeProbe,
  validation: {
    bulkSelector: {
      query: document,
      variables,
      response: response.payload,
    },
  },
  cleanup: null,
  upstreamCalls: [],
};

await mkdir(outputDir, { recursive: true });
await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');

console.log(
  JSON.stringify(
    {
      ok: true,
      apiVersion,
      output: outputPath,
    },
    null,
    2,
  ),
);

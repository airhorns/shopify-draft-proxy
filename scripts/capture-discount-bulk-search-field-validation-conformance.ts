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
const outputPath = path.join(outputDir, 'discount-bulk-search-field-validation.json');
const document = await readFile(
  'config/parity-requests/discounts/discount-bulk-search-field-validation.graphql',
  'utf8',
);

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
  codeFieldSearch: 'code:DRAFT_PROXY_NO_MATCH',
  codeStatusSearch: 'status:DRAFT_PROXY_NO_MATCH',
  codeClassSearch: 'discount_class:DRAFT_PROXY_NO_MATCH',
  codeTimesUsedSearch: 'times_used:>9999999',
  codeTypeSearch: 'discount_type:DRAFT_PROXY_NO_MATCH',
  unknownFieldSearch: 'frobnicate:DRAFT_PROXY_NO_MATCH',
  automaticStatusSearch: 'status:DRAFT_PROXY_NO_MATCH',
  automaticClassSearch: 'discount_class:DRAFT_PROXY_NO_MATCH',
  automaticCodeSearch: 'code:DRAFT_PROXY_NO_MATCH',
};

const response = await runGraphqlRaw(document, variables);

const fixture = {
  storeDomain,
  apiVersion,
  accessScopes: scopeProbe,
  validation: {
    bulkSearchField: {
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

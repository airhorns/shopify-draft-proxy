/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';
import {
  assertDiscountConformanceScopes,
  captureDiscountReadEvidence,
  probeDiscountConformanceScopes,
} from './discount-conformance-lib.js';

const requiredVars = ['SHOPIFY_CONFORMANCE_STORE_DOMAIN', 'SHOPIFY_CONFORMANCE_ADMIN_ORIGIN'];
const missingVars = requiredVars.filter((name) => !process.env[name]);
if (missingVars.length > 0) {
  console.error(`Missing required environment variables: ${missingVars.join(', ')}`);
  process.exit(1);
}

const storeDomain = process.env['SHOPIFY_CONFORMANCE_STORE_DOMAIN'];
const adminOrigin = process.env['SHOPIFY_CONFORMANCE_ADMIN_ORIGIN'];
const apiVersion = process.env['SHOPIFY_CONFORMANCE_API_VERSION'] || '2026-04';

if (!storeDomain || !adminOrigin) {
  console.error(`Missing required environment variables: ${missingVars.join(', ')}`);
  process.exit(1);
}

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion);
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const adminOptions = {
  adminOrigin,
  apiVersion,
  accessToken: adminAccessToken,
  headers: buildAdminAuthHeaders(adminAccessToken),
};

await mkdir(outputDir, { recursive: true });

const scopeProbe = await probeDiscountConformanceScopes(adminOptions);
await writeFile(path.join(outputDir, 'discount-scope-probe.json'), `${JSON.stringify(scopeProbe, null, 2)}\n`, 'utf8');

try {
  assertDiscountConformanceScopes(scopeProbe);
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
}

const first = Number.parseInt(process.env['SHOPIFY_CONFORMANCE_DISCOUNTS_FIRST'] ?? '10', 10);
const query = process.env['SHOPIFY_CONFORMANCE_DISCOUNTS_QUERY'] || null;
const captures = await captureDiscountReadEvidence(adminOptions, { first, query });

await writeFile(
  path.join(outputDir, 'discount-nodes-count.json'),
  `${JSON.stringify(captures.discountNodesCount, null, 2)}\n`,
  'utf8',
);
await writeFile(
  path.join(outputDir, 'discount-nodes-catalog.json'),
  `${JSON.stringify(captures.discountNodesCatalog, null, 2)}\n`,
  'utf8',
);

console.log(
  JSON.stringify(
    {
      ok: true,
      apiVersion,
      outputDir,
      files: ['discount-scope-probe.json', 'discount-nodes-count.json', 'discount-nodes-catalog.json'],
      first,
      query,
    },
    null,
    2,
  ),
);

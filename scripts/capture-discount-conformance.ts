/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { readConformanceScriptConfig } from './conformance-script-config.js';

import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';
import {
  assertDiscountConformanceScopes,
  captureDiscountDetailEvidence,
  captureDiscountReadEvidence,
  probeDiscountConformanceScopes,
} from './discount-conformance-lib.js';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});

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
const detailCaptures = await captureDiscountDetailEvidence(adminOptions);

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
await writeFile(
  path.join(outputDir, 'discount-code-basic-detail-read.json'),
  `${JSON.stringify(detailCaptures.codeDetail, null, 2)}\n`,
  'utf8',
);
await writeFile(
  path.join(outputDir, 'discount-automatic-basic-detail-read.json'),
  `${JSON.stringify(detailCaptures.automaticDetail, null, 2)}\n`,
  'utf8',
);

console.log(
  JSON.stringify(
    {
      ok: true,
      apiVersion,
      outputDir,
      files: [
        'discount-scope-probe.json',
        'discount-nodes-count.json',
        'discount-nodes-catalog.json',
        'discount-code-basic-detail-read.json',
        'discount-automatic-basic-detail-read.json',
      ],
      first,
      query,
    },
    null,
    2,
  ),
);

/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2025-01',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const requestPath = path.join(
  'config',
  'parity-requests',
  'online-store',
  'comment-moderation-not-found-codes.graphql',
);
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'online-store');
const outputPath = path.join(outputDir, 'comment-moderation-not-found-codes.json');
const query = await readFile(requestPath, 'utf8');
const variables = {
  id: 'gid://shopify/Comment/0',
};
const result = await runGraphqlRaw(query, variables);

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      scenarioId: 'comment-moderation-not-found-codes',
      storeDomain,
      apiVersion,
      capturedAt: new Date().toISOString(),
      variables,
      capture: {
        request: {
          query,
          variables,
        },
        response: {
          status: result.status,
          payload: result.payload,
        },
      },
      evidence: {
        source: 'live-shopify',
        notes: [
          `Captured against ${storeDomain} using API ${apiVersion}.`,
          'The probe uses a harmless non-existent Comment GID to exercise resolver-level not-found userErrors without setup or cleanup state.',
        ],
      },
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(`Wrote ${outputPath}`);

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

const requestPath = path.join('config', 'parity-requests', 'online-store', 'theme-update-role-not-an-input.graphql');
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'online-store');
const outputPath = path.join(outputDir, 'theme-update-role-not-an-input.json');
const query = await readFile(requestPath, 'utf8');

const variables = {
  id: 'gid://shopify/OnlineStoreTheme/0',
};
const result = await runGraphqlRaw(query, variables);

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      scenarioId: 'online-store/theme-update-role-not-an-input',
      storeDomain,
      apiVersion,
      capturedAt: new Date().toISOString(),
      captures: {
        roleInput: {
          variables,
          response: {
            status: result.status,
            payload: result.payload,
          },
        },
      },
      evidence: {
        source: 'live-shopify',
        notes: [
          `Captured against ${storeDomain} using API ${apiVersion}.`,
          'The invalid input-object field fails at GraphQL schema validation before resolver auth or theme state is consulted, so the probe uses a harmless dummy theme ID.',
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

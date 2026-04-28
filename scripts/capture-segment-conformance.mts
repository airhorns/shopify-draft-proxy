/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'segments');
const outputPath = path.join(outputDir, 'segments-baseline.json');
const documentPath = path.join('config', 'parity-requests', 'segments-baseline-read.graphql');
const variablesPath = path.join('config', 'parity-requests', 'segments-baseline-read.variables.json');

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const document = await readFile(documentPath, 'utf8');
const variables = JSON.parse(await readFile(variablesPath, 'utf8')) as Record<string, unknown>;
const result = await runGraphqlRequest(document, variables);

if (result.status < 200 || result.status >= 300) {
  console.error(JSON.stringify(result.payload, null, 2));
  throw new Error(`Segment conformance capture failed with HTTP ${result.status}`);
}

await mkdir(outputDir, { recursive: true });
await writeFile(outputPath, `${JSON.stringify(result.payload, null, 2)}\n`);
console.log(`Wrote ${outputPath}`);

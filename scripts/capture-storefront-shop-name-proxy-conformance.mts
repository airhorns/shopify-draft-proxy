/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { readConformanceScriptConfig } from './conformance-script-config.js';

const scenarioId = 'storefront-shop-name-proxy-parity';
const operationName = 'StorefrontShopNameProxyParity';
const { storeDomain, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'online-store');
const outputPath = path.join(outputDir, `${scenarioId}.json`);
const documentPath = path.join('config', 'parity-requests', 'online-store', 'storefront-shop-name.graphql');
const document = await readFile(documentPath, 'utf8');
const variables: Record<string, never> = {};
const endpoint = `https://${storeDomain}/api/${apiVersion}/graphql.json`;

const response = await fetch(endpoint, {
  method: 'POST',
  headers: {
    'Content-Type': 'application/json',
  },
  body: JSON.stringify({ query: document, variables }),
});
const responseText = await response.text();
let body: unknown = responseText;
try {
  body = responseText.length > 0 ? (JSON.parse(responseText) as unknown) : null;
} catch {
  // Keep the raw text body; the fixture should represent the exact Storefront response shape.
}

const recordedRequest = {
  operationName,
  query: document,
  variables,
  response: {
    status: response.status,
    body,
  },
};

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      scenarioId,
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      apiSurface: 'storefront',
      endpoint,
      authMode: 'no-storefront-token',
      primary: recordedRequest,
      upstreamCalls: [recordedRequest],
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(`Wrote ${outputPath}`);
console.log(`Captured Storefront status ${response.status} for ${operationName}`);

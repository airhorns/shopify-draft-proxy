/* oxlint-disable no-console -- CLI capture scripts intentionally write status output. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'admin-platform');
const outputPath = path.join(outputDir, 'admin-platform-flow-trigger-receive-property-size-boundary.json');
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const flowTriggerPropertySizeBoundaryDocument = `#graphql
  mutation FlowTriggerReceivePropertySizeBoundary(
    $bloatedBody: String
    $oversizedBody: String
    $nearLimitPayload: JSON
  ) {
    bloatedResources: flowTriggerReceive(body: $bloatedBody) {
      userErrors {
        field
        message
      }
    }
    oversizedBodyProperties: flowTriggerReceive(body: $oversizedBody) {
      userErrors {
        field
        message
      }
    }
    nearLimitHandlePayload: flowTriggerReceive(handle: "missing-flow-trigger-handle", payload: $nearLimitPayload) {
      userErrors {
        field
        message
      }
    }
  }
`;

function assertNoTopLevelErrors(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

const variables = {
  bloatedBody: JSON.stringify({
    trigger_id: 'abc',
    resources: [{ url: `https://example.com/${'x'.repeat(60_000)}`, name: 'resource' }],
    properties: { a: 1 },
  }),
  oversizedBody: JSON.stringify({
    trigger_id: 'abc',
    properties: { value: 'x'.repeat(49_989) },
  }),
  nearLimitPayload: { value: 'x'.repeat(49_988) },
};

await mkdir(outputDir, { recursive: true });

const result = await runGraphqlRequest(flowTriggerPropertySizeBoundaryDocument, variables);
assertNoTopLevelErrors(result, 'flowTriggerReceive property-size boundary');

const fixture = {
  scenarioId: 'admin-platform-flow-trigger-receive-property-size-boundary',
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  query: flowTriggerPropertySizeBoundaryDocument,
  variables,
  result: {
    status: result.status,
    payload: result.payload,
  },
  upstreamCalls: [],
  notes:
    'Validation-only Flow trigger property-size boundary branches captured against Shopify before external Flow trigger delivery.',
};

await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
console.log(`Wrote ${outputPath}`);

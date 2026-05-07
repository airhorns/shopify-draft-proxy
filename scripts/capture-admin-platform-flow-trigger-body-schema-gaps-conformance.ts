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
const outputPath = path.join(outputDir, 'admin-platform-flow-trigger-receive-body-schema-gaps.json');
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const flowTriggerBodySchemaGapsDocument = `#graphql
  mutation FlowTriggerReceiveBodySchemaGaps {
    missingTriggerReference: flowTriggerReceive(body: "{\\"properties\\":{}}") {
      userErrors {
        field
        message
      }
    }
    unknownTriggerId: flowTriggerReceive(body: "{\\"trigger_id\\":\\"abc\\",\\"properties\\":{}}") {
      userErrors {
        field
        message
      }
    }
    unknownTriggerTitle: flowTriggerReceive(body: "{\\"trigger_title\\":\\"foo\\",\\"properties\\":{}}") {
      userErrors {
        field
        message
      }
    }
    nonAbsoluteResourceUrl: flowTriggerReceive(body: "{\\"trigger_id\\":\\"abc\\",\\"properties\\":{},\\"resources\\":[{\\"url\\":\\"not-a-url\\",\\"name\\":\\"x\\"}]}") {
      userErrors {
        field
        message
      }
    }
    unknownRootField: flowTriggerReceive(body: "{\\"trigger_id\\":\\"abc\\",\\"properties\\":{},\\"unknown_root\\":1}") {
      userErrors {
        field
        message
      }
    }
    multipleSchemaErrors: flowTriggerReceive(body: "{\\"properties\\":{},\\"resources\\":[{\\"url\\":\\"not-a-url\\"}],\\"unknown_root\\":1}") {
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

await mkdir(outputDir, { recursive: true });

const result = await runGraphqlRequest(flowTriggerBodySchemaGapsDocument);
assertNoTopLevelErrors(result, 'flowTriggerReceive body schema gaps');

const fixture = {
  scenarioId: 'admin-platform-flow-trigger-receive-body-schema-gaps',
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  query: flowTriggerBodySchemaGapsDocument,
  result: {
    status: result.status,
    payload: result.payload,
  },
  upstreamCalls: [],
  notes:
    'Validation-only Flow trigger body-schema branches captured against Shopify before external Flow trigger delivery.',
};

await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
console.log(`Wrote ${outputPath}`);

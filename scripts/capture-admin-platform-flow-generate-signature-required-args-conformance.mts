/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type CapturedInteraction = {
  operationName: string;
  query: string;
  variables: Record<string, unknown>;
  status: number;
  response: unknown;
};

const scenarioId = 'admin-platform-flow-generate-signature-required-args';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'admin-platform');
const outputPath = path.join(outputDir, `${scenarioId}.json`);

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const cases = {
  missingBoth: {
    operationName: 'FlowGenerateSignatureMissingBothRequiredArgs',
    query: `mutation FlowGenerateSignatureMissingBothRequiredArgs {
  flowGenerateSignature {
    signature
  }
}
`,
    variables: {},
  },
  missingId: {
    operationName: 'FlowGenerateSignatureMissingIdRequiredArg',
    query: `mutation FlowGenerateSignatureMissingIdRequiredArg {
  flowGenerateSignature(payload: "{}") {
    signature
  }
}
`,
    variables: {},
  },
  missingPayload: {
    operationName: 'FlowGenerateSignatureMissingPayloadRequiredArg',
    query: `mutation FlowGenerateSignatureMissingPayloadRequiredArg {
  flowGenerateSignature(id: "gid://shopify/FlowActionDefinition/0") {
    signature
    payload
    userErrors {
      field
      message
    }
  }
}
`,
    variables: {},
  },
  nullId: {
    operationName: 'FlowGenerateSignatureNullIdRequiredArg',
    query: `mutation FlowGenerateSignatureNullIdRequiredArg {
  flowGenerateSignature(id: null, payload: "{}") {
    signature
  }
}
`,
    variables: {},
  },
  nullPayload: {
    operationName: 'FlowGenerateSignatureNullPayloadRequiredArg',
    query: `mutation FlowGenerateSignatureNullPayloadRequiredArg {
  flowGenerateSignature(id: "gid://shopify/FlowActionDefinition/0", payload: null) {
    signature
  }
}
`,
    variables: {},
  },
};

function captureResult(
  operationName: string,
  query: string,
  variables: Record<string, unknown>,
  result: ConformanceGraphqlResult,
): CapturedInteraction {
  return {
    operationName,
    query,
    variables,
    status: result.status,
    response: result.payload,
  };
}

async function capture(caseConfig: {
  operationName: string;
  query: string;
  variables: Record<string, unknown>;
}): Promise<CapturedInteraction> {
  const result = await runGraphqlRequest(caseConfig.query, caseConfig.variables);
  return captureResult(caseConfig.operationName, caseConfig.query, caseConfig.variables, result);
}

const capturedCases: Record<string, CapturedInteraction> = {};
for (const [name, caseConfig] of Object.entries(cases)) {
  capturedCases[name] = await capture(caseConfig);
}

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      scenarioId,
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      cases: capturedCases,
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
);

console.log(`Wrote ${outputPath}`);

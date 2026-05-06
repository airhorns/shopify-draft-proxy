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

const scenarioId = 'bulk-operations-read-arg-validation';
const configEnv = {
  ...process.env,
  SHOPIFY_CONFORMANCE_API_VERSION: process.env['SHOPIFY_CONFORMANCE_BULK_API_VERSION'] ?? '2026-04',
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  env: configEnv,
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'bulk-operations');
const outputPath = path.join(outputDir, `${scenarioId}.json`);

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const cases = {
  missingWindow: {
    operationName: 'BulkOperationsMissingWindowValidation',
    query: `#graphql
query BulkOperationsMissingWindowValidation {
  bulkOperations {
    nodes {
      id
    }
  }
}
`,
    variables: {},
  },
  firstAndLast: {
    operationName: 'BulkOperationsFirstAndLastValidation',
    query: `#graphql
query BulkOperationsFirstAndLastValidation {
  bulkOperations(first: 1, last: 1) {
    nodes {
      id
    }
  }
}
`,
    variables: {},
  },
  invalidCreatedAt: {
    operationName: 'BulkOperationsInvalidCreatedAtValidation',
    query: `#graphql
query BulkOperationsInvalidCreatedAtValidation {
  bulkOperations(first: 1, query: "created_at:not-a-date") {
    nodes {
      id
    }
  }
}
`,
    variables: {},
  },
  invalidStatusExpired: {
    operationName: 'BulkOperationsInvalidStatusExpiredValidation',
    query: `#graphql
query BulkOperationsInvalidStatusExpiredValidation {
  bulkOperations(first: 1, query: "status:EXPIRED") {
    nodes {
      id
    }
  }
}
`,
    variables: {},
  },
  invalidStatusUnknown: {
    operationName: 'BulkOperationsInvalidStatusUnknownValidation',
    query: `#graphql
query BulkOperationsInvalidStatusUnknownValidation {
  bulkOperations(first: 1, query: "status:NOT_A_STATUS") {
    nodes {
      id
    }
  }
}
`,
    variables: {},
  },
  invalidOperationType: {
    operationName: 'BulkOperationsInvalidOperationTypeValidation',
    query: `#graphql
query BulkOperationsInvalidOperationTypeValidation {
  bulkOperations(first: 1, query: "operation_type:EXPORT") {
    nodes {
      id
    }
  }
}
`,
    variables: {},
  },
  malformedInlineId: {
    operationName: 'BulkOperationMalformedInlineIdValidation',
    query: `#graphql
query BulkOperationMalformedInlineIdValidation {
  bulkOperation(id: "not-a-gid") {
    id
  }
}
`,
    variables: {},
  },
  nonBulkOperationGid: {
    operationName: 'BulkOperationNonBulkOperationGidValidation',
    query: `#graphql
query BulkOperationNonBulkOperationGidValidation {
  bulkOperation(id: "gid://shopify/Product/1") {
    id
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

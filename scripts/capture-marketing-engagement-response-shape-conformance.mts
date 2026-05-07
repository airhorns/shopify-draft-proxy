/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type CapturedRequest = {
  documentPath: string;
  variables: Record<string, unknown>;
  response: ConformanceGraphqlResult;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'marketing');
const outputPath = path.join(outputDir, 'marketing-engagement-create-response-shape.json');

const requestDir = path.join('config', 'parity-requests', 'marketing');
const createActivityRequestPath = path.join(requestDir, 'marketing-engagement-response-shape-create-activity.graphql');
const fullInputRequestPath = path.join(requestDir, 'marketing-engagement-response-shape-full.graphql');
const sparseInputRequestPath = path.join(requestDir, 'marketing-engagement-response-shape-sparse.graphql');
const missingOccurredOnRequestPath = path.join(
  requestDir,
  'marketing-engagement-response-shape-missing-occurred-on.graphql',
);

const deleteActivityDocument = `#graphql
  mutation DeleteMarketingActivity($remoteId: String) {
    marketingActivityDeleteExternal(remoteId: $remoteId) {
      deletedMarketingActivityId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const schemaDocument = `#graphql
  query MarketingEngagementResponseShapeSchema {
    currentAppInstallation {
      app {
        id
        handle
        title
      }
      accessScopes {
        handle
      }
    }
    engagementInput: __type(name: "MarketingEngagementInput") {
      inputFields {
        name
        type {
          kind
          name
          ofType {
            kind
            name
          }
        }
      }
    }
  }
`;

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function readText(relativePath: string): Promise<string> {
  return readFile(path.join(process.cwd(), relativePath), 'utf8');
}

async function capture(documentPath: string, variables: Record<string, unknown>): Promise<CapturedRequest> {
  const document = await readText(documentPath);
  const response = await runGraphqlRequest(document, variables);

  return {
    documentPath,
    variables,
    response,
  };
}

async function assertHttpOk(label: string, result: { status: number; payload: unknown }): Promise<void> {
  if (result.status >= 200 && result.status < 300) {
    return;
  }

  console.error(JSON.stringify(result.payload, null, 2));
  throw new Error(`${label} failed with HTTP ${result.status}`);
}

function readRecord(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as Record<string, unknown>) : null;
}

function readPath(value: unknown, pathParts: string[]): unknown {
  let current: unknown = value;
  for (const part of pathParts) {
    const record = readRecord(current);
    if (!record) {
      return undefined;
    }
    current = record[part];
  }
  return current;
}

function readString(value: unknown): string | null {
  return typeof value === 'string' && value.length > 0 ? value : null;
}

function readAccessScopes(schemaPayload: unknown): string[] {
  const scopes = readPath(schemaPayload, ['data', 'currentAppInstallation', 'accessScopes']);
  if (!Array.isArray(scopes)) {
    return [];
  }

  return scopes.flatMap((scope): string[] => {
    const handle = readString(readRecord(scope)?.['handle']);
    return handle ? [handle] : [];
  });
}

function hasUserErrors(payload: unknown, pathParts: string[]): boolean {
  const userErrors = readPath(payload, pathParts);
  return Array.isArray(userErrors) && userErrors.length > 0;
}

function hasTopLevelErrors(payload: unknown): boolean {
  return Array.isArray(readRecord(payload)?.['errors']);
}

function readCreatedActivityId(payload: unknown): string | null {
  return readString(readPath(payload, ['data', 'marketingActivityCreateExternal', 'marketingActivity', 'id']));
}

function requireSuccessfulEngagement(capture: CapturedRequest, label: string): void {
  if (capture.response.status < 200 || capture.response.status >= 300 || capture.response.payload.errors) {
    throw new Error(`${label} returned top-level errors: ${JSON.stringify(capture.response.payload)}`);
  }
  if (hasUserErrors(capture.response.payload, ['data', 'marketingEngagementCreate', 'userErrors'])) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(capture.response.payload)}`);
  }
}

function requireValidationEvidence(capture: CapturedRequest, label: string): void {
  if (capture.response.status < 200 || capture.response.status >= 300) {
    throw new Error(
      `${label} failed with HTTP ${capture.response.status}: ${JSON.stringify(capture.response.payload)}`,
    );
  }
  if (hasTopLevelErrors(capture.response.payload)) {
    return;
  }
  if (hasUserErrors(capture.response.payload, ['data', 'marketingEngagementCreate', 'userErrors'])) {
    return;
  }
  throw new Error(
    `${label} did not produce top-level errors or mutation userErrors: ${JSON.stringify(capture.response.payload)}`,
  );
}

await mkdir(outputDir, { recursive: true });

const schemaResult = await runGraphqlRequest(schemaDocument);
await assertHttpOk('Marketing engagement response-shape schema capture', schemaResult);

const runId = Date.now().toString(36);
const remoteId = `marketing-engagement-response-shape-${runId}`;
const activityInput = {
  title: 'Marketing Engagement Response Shape',
  remoteId,
  status: 'ACTIVE',
  remoteUrl: `https://example.com/${remoteId}`,
  tactic: 'NEWSLETTER',
  marketingChannelType: 'EMAIL',
  utm: {
    campaign: remoteId,
    source: 'newsletter',
    medium: 'email',
  },
};

let createActivity: CapturedRequest | null = null;
let fullInputEcho: CapturedRequest | null = null;
let sparseInput: CapturedRequest | null = null;
let missingOccurredOn: CapturedRequest | null = null;
let cleanup: CapturedRequest | null = null;
let createdActivityId: string | null = null;

try {
  createActivity = await capture(createActivityRequestPath, { input: activityInput });
  await assertHttpOk('Marketing engagement response-shape activity setup', createActivity.response);
  if (hasUserErrors(createActivity.response.payload, ['data', 'marketingActivityCreateExternal', 'userErrors'])) {
    console.error(JSON.stringify(createActivity.response.payload, null, 2));
    throw new Error('Marketing engagement response-shape activity setup returned userErrors');
  }
  createdActivityId = readCreatedActivityId(createActivity.response.payload);
  if (!createdActivityId) {
    console.error(JSON.stringify(createActivity.response.payload, null, 2));
    throw new Error('Marketing engagement response-shape activity setup did not return an activity id');
  }

  fullInputEcho = await capture(fullInputRequestPath, { remoteId });
  requireSuccessfulEngagement(fullInputEcho, 'Marketing engagement full-input echo');

  sparseInput = await capture(sparseInputRequestPath, { remoteId });
  requireValidationEvidence(sparseInput, 'Marketing engagement sparse-input echo');

  missingOccurredOn = await capture(missingOccurredOnRequestPath, { remoteId });
  requireValidationEvidence(missingOccurredOn, 'Marketing engagement missing occurredOn');
} finally {
  if (createdActivityId) {
    const cleanupResponse = await runGraphqlRequest(deleteActivityDocument, { remoteId });
    cleanup = {
      documentPath: '<inline:delete-marketing-activity>',
      variables: { remoteId },
      response: cleanupResponse,
    };
    await assertHttpOk('Marketing engagement response-shape activity cleanup', cleanupResponse);
  }
}

const fixture = {
  capturedAt: new Date().toISOString(),
  apiVersion,
  storeDomain,
  scopeEvidence: {
    app: readPath(schemaResult.payload, ['data', 'currentAppInstallation', 'app']),
    accessScopes: readAccessScopes(schemaResult.payload).filter(
      (scope) => scope === 'read_marketing_events' || scope === 'write_marketing_events',
    ),
  },
  schema: {
    marketingEngagementInput: readPath(schemaResult.payload, ['data', 'engagementInput']),
  },
  cases: {
    createActivity,
    fullInputEcho,
    sparseInput,
    missingOccurredOn,
    cleanup,
  },
  upstreamCalls: [],
  notes: [
    'Captures marketingEngagementCreate immediate response shape against Admin GraphQL 2026-04.',
    'The full-input case proves Shopify echoes supplied engagement fields without synthesizing extra defaults.',
    'The sparse and missing-occurredOn cases prove Shopify behavior for omitted required MarketingEngagementInput fields before the proxy stages local state.',
    'A disposable external marketing activity is created for identifier resolution and deleted during cleanup.',
  ],
};

await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');

console.log(
  JSON.stringify(
    {
      ok: true,
      outputPath,
      apiVersion,
      storeDomain,
      activityId: createdActivityId,
      sparseHasTopLevelErrors: sparseInput ? hasTopLevelErrors(sparseInput.response.payload) : null,
      missingOccurredOnHasTopLevelErrors: missingOccurredOn
        ? hasTopLevelErrors(missingOccurredOn.response.payload)
        : null,
    },
    null,
    2,
  ),
);

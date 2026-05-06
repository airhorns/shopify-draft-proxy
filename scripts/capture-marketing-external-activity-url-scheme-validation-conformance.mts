/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type GraphqlResult = { status: number; payload: unknown };
type CaseCapture = {
  operation: 'create' | 'update' | 'upsert' | 'read';
  variables: Record<string, unknown>;
  response: GraphqlResult;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'marketing');
const outputPath = path.join(outputDir, 'marketing-external-activity-url-scheme-validation.json');

const createDocument = await readFile(
  path.join('config', 'parity-requests', 'marketing', 'marketing-external-activity-url-scheme-create.graphql'),
  'utf8',
);
const updateDocument = await readFile(
  path.join('config', 'parity-requests', 'marketing', 'marketing-external-activity-url-scheme-update.graphql'),
  'utf8',
);
const upsertDocument = await readFile(
  path.join('config', 'parity-requests', 'marketing', 'marketing-external-activity-url-scheme-upsert.graphql'),
  'utf8',
);
const readDocument = await readFile(
  path.join('config', 'parity-requests', 'marketing', 'marketing-external-activity-url-scheme-read.graphql'),
  'utf8',
);

const deleteDocument = `#graphql
  mutation MarketingExternalActivityUrlSchemeCleanup($remoteId: String) {
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

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

function randomSuffix(): string {
  return `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

function readRecord(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as Record<string, unknown>) : null;
}

function readPath(value: unknown, parts: string[]): unknown {
  let current: unknown = value;
  for (const part of parts) {
    const record = readRecord(current);
    if (!record) return undefined;
    current = record[part];
  }
  return current;
}

function readUserErrors(payload: unknown, root: string): unknown[] {
  const value = readPath(payload, ['data', root, 'userErrors']);
  return Array.isArray(value) ? value : [];
}

function readTopLevelErrors(payload: unknown): unknown[] {
  const value = readRecord(payload)?.errors;
  return Array.isArray(value) ? value : [];
}

function firstTopLevelError(payload: unknown): Record<string, unknown> | null {
  const [first] = readTopLevelErrors(payload);
  return readRecord(first);
}

function assertNoTopLevelErrors(label: string, result: GraphqlResult): void {
  if (result.status >= 200 && result.status < 300 && readTopLevelErrors(result.payload).length === 0) {
    return;
  }
  console.error(JSON.stringify(result.payload, null, 2));
  throw new Error(`${label} failed with HTTP ${result.status}`);
}

function assertNoUserErrors(label: string, payload: unknown, root: string): void {
  const userErrors = readUserErrors(payload, root);
  if (userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors)}`);
  }
}

function assertInvalidFieldArguments(label: string, payload: unknown, root: string): void {
  const error = firstTopLevelError(payload);
  const dataRoot = readPath(payload, ['data', root]);
  const code = readRecord(error?.extensions)?.code;
  const pathValue = error?.path;
  if (
    error?.message !== 'The URL scheme must be one of the following: https,http' ||
    code !== 'INVALID_FIELD_ARGUMENTS' ||
    !Array.isArray(pathValue) ||
    pathValue[0] !== root ||
    dataRoot !== null
  ) {
    throw new Error(`${label} expected INVALID_FIELD_ARGUMENTS with null ${root}, got ${JSON.stringify(payload)}`);
  }
}

function assertInvalidVariable(label: string, payload: unknown, variablePath: string[]): void {
  const error = firstTopLevelError(payload);
  const code = readRecord(error?.extensions)?.code;
  const problems = readRecord(error?.extensions)?.problems;
  const firstProblem = Array.isArray(problems) ? readRecord(problems[0]) : null;
  const pathValue = firstProblem?.path;
  if (
    code !== 'INVALID_VARIABLE' ||
    !Array.isArray(pathValue) ||
    JSON.stringify(pathValue) !== JSON.stringify(variablePath)
  ) {
    throw new Error(`${label} expected INVALID_VARIABLE at ${variablePath.join('.')}, got ${JSON.stringify(payload)}`);
  }
}

function assertEmptyRead(label: string, payload: unknown): void {
  const nodes = readPath(payload, ['data', 'marketingActivities', 'nodes']);
  if (!Array.isArray(nodes) || nodes.length !== 0) {
    throw new Error(`${label} expected no marketingActivities nodes, got ${JSON.stringify(payload)}`);
  }
}

function baseInput(label: string, suffix: string, remoteUrl: string): Record<string, unknown> {
  return {
    title: `URL scheme ${label} ${suffix}`,
    remoteId: `url-scheme-${label}-${suffix}`,
    status: 'ACTIVE',
    remoteUrl,
    tactic: 'NEWSLETTER',
    marketingChannelType: 'EMAIL',
    utm: {
      campaign: `url-scheme-${label}-${suffix}`,
      source: 'email',
      medium: 'newsletter',
    },
  };
}

async function runCase(
  cases: Record<string, CaseCapture>,
  name: string,
  operation: CaseCapture['operation'],
  query: string,
  variables: Record<string, unknown>,
): Promise<GraphqlResult> {
  const response = await runGraphqlRequest(query, variables);
  cases[name] = { operation, variables, response };
  return response;
}

const suffix = randomSuffix();
const seedInput = baseInput('valid-https', suffix, `https://example.com/url-scheme/${suffix}/valid-https`);
const invalidCreateFtpInput = baseInput('create-ftp', suffix, 'ftp://example.com/url-scheme');
const invalidCreateDataInput = baseInput('create-data', suffix, 'data:text/plain,hello');
const invalidCreatePreviewFileInput = {
  ...baseInput('create-preview-file', suffix, `https://example.com/url-scheme/${suffix}/preview-file`),
  remotePreviewImageUrl: 'file://example.com/preview.png',
};
const invalidUpsertMailtoInput = baseInput('upsert-mailto', suffix, 'mailto:marketing@example.com');
const invalidUpsertPreviewJavascriptInput = {
  ...baseInput('upsert-preview-javascript', suffix, `https://example.com/url-scheme/${suffix}/upsert-preview-js`),
  remotePreviewImageUrl: 'javascript:alert(1)',
};

const cases: Record<string, CaseCapture> = {};
const cleanupResponses: Record<string, GraphqlResult> = {};

try {
  const validCreate = await runCase(cases, 'validHttpsCreate', 'create', createDocument, { input: seedInput });
  assertNoTopLevelErrors('valid-https-create', validCreate);
  assertNoUserErrors('valid-https-create', validCreate.payload, 'marketingActivityCreateExternal');

  const validHttpUpdate = await runCase(cases, 'validHttpUpdate', 'update', updateDocument, {
    remoteId: seedInput.remoteId,
    input: {
      title: `URL scheme valid http ${suffix}`,
      remoteUrl: `http://example.com/url-scheme/${suffix}/valid-http`,
    },
  });
  assertNoTopLevelErrors('valid-http-update', validHttpUpdate);
  assertNoUserErrors('valid-http-update', validHttpUpdate.payload, 'marketingActivityUpdateExternal');

  const invalidUpdateFtp = await runCase(cases, 'invalidUpdateRemoteUrlFtp', 'update', updateDocument, {
    remoteId: seedInput.remoteId,
    input: {
      title: `URL scheme rejected update ftp ${suffix}`,
      remoteUrl: 'ftp://example.com/url-scheme-update',
    },
  });
  assertInvalidFieldArguments('invalid-update-remote-url-ftp', invalidUpdateFtp.payload, 'marketingActivityUpdateExternal');

  const invalidUpdatePreviewJavascript = await runCase(
    cases,
    'invalidUpdatePreviewJavascript',
    'update',
    updateDocument,
    {
      remoteId: seedInput.remoteId,
      input: {
        title: `URL scheme rejected update preview javascript ${suffix}`,
        remotePreviewImageUrl: 'javascript:alert(1)',
      },
    },
  );
  assertInvalidVariable('invalid-update-preview-javascript', invalidUpdatePreviewJavascript.payload, [
    'remotePreviewImageUrl',
  ]);

  const readAfterInvalidUpdate = await runCase(cases, 'readAfterInvalidUpdate', 'read', readDocument, {
    remoteIds: [seedInput.remoteId],
  });
  assertNoTopLevelErrors('read-after-invalid-update', readAfterInvalidUpdate);

  const invalidCreateFtp = await runCase(cases, 'invalidCreateRemoteUrlFtp', 'create', createDocument, {
    input: invalidCreateFtpInput,
  });
  assertInvalidFieldArguments('invalid-create-remote-url-ftp', invalidCreateFtp.payload, 'marketingActivityCreateExternal');

  const readAfterInvalidCreate = await runCase(cases, 'readAfterInvalidCreate', 'read', readDocument, {
    remoteIds: [invalidCreateFtpInput.remoteId],
  });
  assertNoTopLevelErrors('read-after-invalid-create', readAfterInvalidCreate);
  assertEmptyRead('read-after-invalid-create', readAfterInvalidCreate.payload);

  const invalidCreatePreviewFile = await runCase(cases, 'invalidCreatePreviewFile', 'create', createDocument, {
    input: invalidCreatePreviewFileInput,
  });
  assertInvalidFieldArguments(
    'invalid-create-preview-file',
    invalidCreatePreviewFile.payload,
    'marketingActivityCreateExternal',
  );

  const invalidCreateData = await runCase(cases, 'invalidCreateRemoteUrlData', 'create', createDocument, {
    input: invalidCreateDataInput,
  });
  assertInvalidVariable('invalid-create-remote-url-data', invalidCreateData.payload, ['remoteUrl']);

  const invalidUpsertMailto = await runCase(cases, 'invalidUpsertRemoteUrlMailto', 'upsert', upsertDocument, {
    input: invalidUpsertMailtoInput,
  });
  assertInvalidFieldArguments('invalid-upsert-remote-url-mailto', invalidUpsertMailto.payload, 'marketingActivityUpsertExternal');

  const invalidUpsertPreviewJavascript = await runCase(
    cases,
    'invalidUpsertPreviewJavascript',
    'upsert',
    upsertDocument,
    {
      input: invalidUpsertPreviewJavascriptInput,
    },
  );
  assertInvalidVariable('invalid-upsert-preview-javascript', invalidUpsertPreviewJavascript.payload, [
    'remotePreviewImageUrl',
  ]);
} finally {
  const remoteIds = [
    seedInput.remoteId,
    invalidCreateFtpInput.remoteId,
    invalidCreateDataInput.remoteId,
    invalidCreatePreviewFileInput.remoteId,
    invalidUpsertMailtoInput.remoteId,
    invalidUpsertPreviewJavascriptInput.remoteId,
  ];
  for (const remoteId of remoteIds) {
    cleanupResponses[remoteId] = await runGraphqlRequest(deleteDocument, { remoteId });
  }
}

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      scenarioId: 'marketing-external-activity-url-scheme-validation',
      apiVersion,
      storeDomain,
      capturedAt: new Date().toISOString(),
      setup: {
        suffix,
        remoteIds: {
          seed: seedInput.remoteId,
          invalidCreateFtp: invalidCreateFtpInput.remoteId,
          invalidCreateData: invalidCreateDataInput.remoteId,
          invalidCreatePreviewFile: invalidCreatePreviewFileInput.remoteId,
          invalidUpsertMailto: invalidUpsertMailtoInput.remoteId,
          invalidUpsertPreviewJavascript: invalidUpsertPreviewJavascriptInput.remoteId,
        },
      },
      cases,
      cleanup: cleanupResponses,
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(
  JSON.stringify(
    {
      ok: true,
      outputPath,
      cases: Object.keys(cases),
    },
    null,
    2,
  ),
);

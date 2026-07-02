import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type RecordedGraphqlRequest = {
  query: string;
  status: number;
  payload: unknown;
};

type GraphqlPayload = {
  data?: unknown;
  errors?: unknown;
  extensions?: unknown;
};

const captureDocuments = [
  {
    key: 'unknownScope',
    documentPath: 'config/parity-requests/apps/appRevokeAccessScopes-fake-scope.graphql',
  },
  {
    key: 'mixedUnknownScope',
    documentPath: 'config/parity-requests/apps/appRevokeAccessScopes-mixed-fake-scope.graphql',
  },
  {
    key: 'requiredScope',
    documentPath: 'config/parity-requests/apps/appRevokeAccessScopes-required-read-products.graphql',
  },
] as const;

function parityEnvelope(payload: unknown): GraphqlPayload {
  const graphqlPayload = payload as GraphqlPayload;
  return {
    ...(graphqlPayload.data === undefined ? {} : { data: graphqlPayload.data }),
    ...(graphqlPayload.errors === undefined ? {} : { errors: graphqlPayload.errors }),
    ...(graphqlPayload.extensions === undefined ? {} : { extensions: graphqlPayload.extensions }),
  };
}

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function record(documentPath: string): Promise<RecordedGraphqlRequest> {
  const query = await readFile(path.join(process.cwd(), documentPath), 'utf8');
  const { status, payload } = await runGraphqlRequest(query, {});
  return { query, status, payload };
}

const safeProbes: Record<string, RecordedGraphqlRequest & { documentPath: string }> = {};
const expected: Record<string, GraphqlPayload> = {};

for (const document of captureDocuments) {
  const recorded = await record(document.documentPath);
  safeProbes[document.key] = { documentPath: document.documentPath, ...recorded };
  expected[document.key] = parityEnvelope(recorded.payload);
}

const fixture = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  scenario: 'app-revoke-access-scopes-validation',
  notes: [
    'Captured live Admin GraphQL appRevokeAccessScopes validation branches that do not revoke real app grants.',
    'The optional-grant success branch is intentionally not captured by this script because recording it would revoke a real access scope from the active conformance app.',
    'The missing-source-app branch is not live-recordable through a valid public Admin request because valid requests carry source app context and unauthenticated requests fail before the mutation resolver.',
  ],
  operationNames: ['appRevokeAccessScopes'],
  upstreamCalls: [],
  evidence: {
    live: {
      safeProbes,
    },
    parity: {
      expected,
    },
  },
};

const outputPath = path.join(
  process.cwd(),
  'fixtures',
  'conformance',
  storeDomain,
  apiVersion,
  'apps',
  'app-revoke-access-scopes-validation.json',
);

await mkdir(path.dirname(outputPath), { recursive: true });
await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');

// oxlint-disable-next-line no-console -- CLI capture scripts intentionally write the generated fixture path.
console.log(outputPath);

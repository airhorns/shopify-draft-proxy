/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type MarketUserError = {
  code?: string | null;
  field?: string[] | null;
  message?: string | null;
};

type WebPresenceCreatePayload = {
  webPresence?: unknown;
  userErrors?: MarketUserError[];
};

type CapturedCase = {
  name: string;
  query: string;
  variables: Record<string, unknown>;
  response: ConformanceGraphqlResult;
};

const fixtureStoreDomain = 'harry-test-heelo.myshopify.com';
const fixtureApiVersion = '2026-04';
const documentPath = path.join(
  'config',
  'parity-requests',
  'markets',
  'web-presence-create-requires-domain-or-subfolder.graphql',
);

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: fixtureApiVersion,
  env: { ...process.env, SHOPIFY_CONFORMANCE_API_VERSION: fixtureApiVersion },
  exitOnMissing: true,
});

if (storeDomain !== fixtureStoreDomain) {
  throw new Error(
    `This recorder writes checked-in ${fixtureStoreDomain} fixtures; got SHOPIFY_CONFORMANCE_STORE_DOMAIN=${storeDomain}.`,
  );
}

if (apiVersion !== fixtureApiVersion) {
  throw new Error(`This recorder writes ${fixtureApiVersion} fixtures; got ${apiVersion}.`);
}

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const query = await readFile(documentPath, 'utf8');
const variables = {
  input: {
    defaultLocale: 'en',
  },
};

function objectValue(value: unknown): Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as Record<string, unknown>) : {};
}

function webPresenceCreatePayload(response: ConformanceGraphqlResult): WebPresenceCreatePayload {
  const data = objectValue(response.payload.data);
  return objectValue(data.webPresenceCreate) as WebPresenceCreatePayload;
}

function assertExpectedUserError(response: ConformanceGraphqlResult): void {
  if (response.status !== 200 || response.payload.errors) {
    throw new Error(`webPresenceCreate validation capture failed: ${JSON.stringify(response, null, 2)}`);
  }

  const payload = webPresenceCreatePayload(response);
  if (payload.webPresence !== null) {
    throw new Error(`Expected null webPresence: ${JSON.stringify(response.payload, null, 2)}`);
  }

  const errors = Array.isArray(payload.userErrors) ? payload.userErrors : [];
  const expected = {
    field: ['input'],
    message: 'One of `subfolderSuffix` or `domainId` is required.',
    code: 'REQUIRES_DOMAIN_OR_SUBFOLDER',
  };
  if (errors.length !== 1 || JSON.stringify(errors[0]) !== JSON.stringify(expected)) {
    throw new Error(`Unexpected userErrors: ${JSON.stringify(errors, null, 2)}`);
  }
}

const response = await runGraphqlRequest(query, variables);
assertExpectedUserError(response);

const cases: CapturedCase[] = [
  {
    name: 'webPresenceCreateRequiresDomainOrSubfolder',
    query,
    variables,
    response,
  },
];

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'markets');
const outputPath = path.join(outputDir, 'web-presence-create-requires-domain-or-subfolder.json');

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      scope: 'Market webPresenceCreate requires domain or subfolder validation',
      cases,
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
      storeDomain,
      apiVersion,
      caseCount: cases.length,
    },
    null,
    2,
  ),
);

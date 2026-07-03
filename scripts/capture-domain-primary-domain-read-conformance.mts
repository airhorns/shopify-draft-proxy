/* oxlint-disable no-console -- CLI scripts intentionally write capture status to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type GraphqlCapture = {
  status: number;
  payload: Record<string, unknown>;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'admin-platform');
const outputPath = path.join(outputDir, 'domain-primary-domain-read.json');
const variablesPath = path.join(
  'config',
  'parity-requests',
  'admin-platform',
  'domain-primary-domain-read.variables.json',
);
const domainReadQuery = await readFile(
  path.join('config', 'parity-requests', 'admin-platform', 'domain-primary-domain-read.graphql'),
  'utf8',
);
const primaryDomainSeedQuery = `query AdminPlatformDomainPrimaryDomainSeed {
  shop {
    primaryDomain {
      id
      host
      url
      sslEnabled
    }
  }
}
`;

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function runGraphqlCapture(query: string, variables: Record<string, unknown> = {}): Promise<GraphqlCapture> {
  const result = await runGraphqlRequest(query, variables);
  return {
    status: result.status,
    payload: result.payload as Record<string, unknown>,
  };
}

function objectField(value: unknown, field: string): Record<string, unknown> | undefined {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return undefined;
  const child = (value as Record<string, unknown>)[field];
  if (typeof child !== 'object' || child === null || Array.isArray(child)) return undefined;
  return child as Record<string, unknown>;
}

function stringField(value: Record<string, unknown> | undefined, field: string): string | undefined {
  const fieldValue = value?.[field];
  return typeof fieldValue === 'string' && fieldValue.length > 0 ? fieldValue : undefined;
}

function assertStatusOk(name: string, capture: GraphqlCapture): void {
  if (capture.status !== 200) {
    throw new Error(`${name} expected HTTP 200, got ${JSON.stringify(capture)}`);
  }
}

const primaryDomainSeed = await runGraphqlCapture(primaryDomainSeedQuery);
assertStatusOk('primaryDomainSeed', primaryDomainSeed);
const seedData = objectField(primaryDomainSeed.payload, 'data');
const primaryDomain = objectField(objectField(seedData, 'shop'), 'primaryDomain');
const primaryDomainId = stringField(primaryDomain, 'id');
if (primaryDomainId === undefined) {
  throw new Error(`Expected shop.primaryDomain.id in seed response: ${JSON.stringify(primaryDomainSeed.payload)}`);
}
if (primaryDomainId === 'gid://shopify/Domain/1000') {
  throw new Error(`Expected a non-Domain/1000 primary domain id, got ${primaryDomainId}`);
}

const variables = { id: primaryDomainId };
const primaryDomainRead = await runGraphqlCapture(domainReadQuery, variables);
assertStatusOk('primaryDomainRead', primaryDomainRead);
const readDomain = objectField(objectField(primaryDomainRead.payload, 'data'), 'domain');
if (stringField(readDomain, 'id') !== primaryDomainId) {
  throw new Error(`domain(id:) did not return the seeded primary domain: ${JSON.stringify(primaryDomainRead.payload)}`);
}

const captureOutput = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  notes:
    'Read-only evidence for direct domain(id:) resolution of the connected shop primary domain. The captured id is deliberately not gid://shopify/Domain/1000.',
  captures: {
    primaryDomainSeed: {
      query: primaryDomainSeedQuery,
      variables: {},
      result: primaryDomainSeed,
    },
    primaryDomainRead: {
      query: domainReadQuery,
      variables,
      result: primaryDomainRead,
    },
  },
  upstreamCalls: [
    {
      operationName: 'AdminPlatformDomainPrimaryDomainRead',
      variables,
      query: domainReadQuery,
      response: {
        status: primaryDomainRead.status,
        body: primaryDomainRead.payload,
      },
    },
  ],
};

await mkdir(outputDir, { recursive: true });
await mkdir(path.dirname(variablesPath), { recursive: true });
await writeFile(outputPath, `${JSON.stringify(captureOutput, null, 2)}\n`, 'utf8');
await writeFile(variablesPath, `${JSON.stringify(variables, null, 2)}\n`, 'utf8');

console.log(JSON.stringify({ ok: true, storeDomain, apiVersion, fixturePath: outputPath, variablesPath }, null, 2));

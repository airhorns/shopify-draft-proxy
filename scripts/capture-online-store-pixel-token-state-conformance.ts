/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type GraphqlCapture = {
  requestPath: string;
  request: {
    query: string;
    variables: JsonRecord;
  };
  status: number;
  response: unknown;
};

type RecordedCall = {
  operationName: string;
  variables: JsonRecord;
  query: string;
  response: {
    status: number;
    body: unknown;
  };
};

const scenarioId = 'online-store-pixel-token-state';
const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2025-01',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'online-store');
const outputPath = path.join(outputDir, 'online-store-pixel-token-state.json');
const captures: Record<string, GraphqlCapture> = {};
const upstreamCalls: RecordedCall[] = [];

async function readGraphql(relativePath: string): Promise<string> {
  return await readFile(relativePath, 'utf8');
}

function isRecord(value: unknown): value is JsonRecord {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function readPath(value: unknown, pathSegments: string[]): unknown {
  return pathSegments.reduce<unknown>((cursor, segment) => {
    if (!isRecord(cursor)) {
      return undefined;
    }
    return cursor[segment];
  }, value);
}

function stringFieldArray(value: unknown, pathSegments: string[]): string[] {
  const entries = readPath(value, pathSegments);
  if (!Array.isArray(entries)) {
    return [];
  }
  return entries
    .map((entry) => (isRecord(entry) && typeof entry['name'] === 'string' ? entry['name'] : null))
    .filter((entry): entry is string => entry !== null);
}

function accessScopeNodes(accessScopesCapture: GraphqlCapture): Array<{ handle: string }> {
  const scopes = readPath(accessScopesCapture.response, ['data', 'currentAppInstallation', 'accessScopes']);
  if (!Array.isArray(scopes)) {
    return [];
  }
  return scopes.flatMap((scope): Array<{ handle: string }> => {
    if (!isRecord(scope) || typeof scope['handle'] !== 'string') {
      return [];
    }
    return [{ handle: scope['handle'] }];
  });
}

async function captureGraphql(name: string, requestPath: string, variables: JsonRecord = {}): Promise<GraphqlCapture> {
  const query = await readGraphql(requestPath);
  const result = await runGraphqlRaw(query, variables);
  const capture: GraphqlCapture = {
    requestPath,
    request: { query, variables },
    status: result.status,
    response: result.payload,
  };
  captures[name] = capture;
  upstreamCalls.push({
    operationName: name,
    variables,
    query,
    response: {
      status: result.status,
      body: result.payload,
    },
  });
  return capture;
}

const suffix = new Date().toISOString().replace(/\D/gu, '').slice(0, 14);
const schema = await captureGraphql(
  'schema',
  path.join('config', 'parity-requests', 'online-store', 'online-store-pixel-token-schema.graphql'),
);
await captureGraphql(
  'webPixelStatusUndefinedField',
  path.join('config', 'parity-requests', 'online-store', 'web-pixel-status-is-not-a-field.graphql'),
  { webPixel: { settings: JSON.stringify({ accountID: `pixel-token-state-${suffix}` }) } },
);
const accessScopes = await captureGraphql(
  'currentAppAccessScopes',
  path.join('config', 'parity-requests', 'apps', 'app-access-scopes-local-read.graphql'),
);

const observedAccessScopes = accessScopeNodes(accessScopes);
const observedUnauthenticatedAccessScopes = observedAccessScopes.filter(({ handle }) =>
  handle.startsWith('unauthenticated_'),
);

const output = {
  scenarioId,
  storeDomain,
  apiVersion,
  capturedAt: new Date().toISOString(),
  captures,
  evidence: {
    source: 'live-shopify',
    serverPixelStatusEnumValues: stringFieldArray(schema.response, ['data', 'serverPixelStatus', 'enumValues']),
    webPixelFieldNames: stringFieldArray(schema.response, ['data', 'webPixel', 'fields']),
    serverPixelFieldNames: stringFieldArray(schema.response, ['data', 'serverPixel', 'fields']),
    observedAccessScopes,
    observedUnauthenticatedAccessScopes,
    notes: [
      'WebPixel.status is captured as a Shopify schema error; WebPixel does not expose server-pixel status fields.',
      'StorefrontAccessToken local replay compares accessScopes against unauthenticated grants observed on the active app installation. This conformance app currently has no unauthenticated grants.',
      'Successful serverPixelCreate and storefrontAccessTokenCreate live mutation captures remain unavailable with this app grant; focused Rust runtime tests cover those local success payloads.',
    ],
  },
  upstreamCalls,
};

await mkdir(outputDir, { recursive: true });
await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');

console.log(JSON.stringify({ ok: true, scenarioId, outputPath }, null, 2));

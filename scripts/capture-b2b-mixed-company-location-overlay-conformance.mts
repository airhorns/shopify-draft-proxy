/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;
type UserError = {
  field?: string[] | null;
  message?: string;
  code?: string | null;
};
type CapturedCase = {
  name: string;
  query: string;
  variables: JsonRecord;
  response: ConformanceGraphqlResult<unknown>;
};

const scenarioId = 'b2b-mixed-company-location-overlay';
const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const outputPath = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'b2b', `${scenarioId}.json`);

async function readDocument(name: string): Promise<string> {
  return await readFile(path.join('config', 'parity-requests', 'b2b', name), 'utf8');
}

const companyCreateDocument = await readDocument('b2b-mixed-overlay-company-create.graphql');
const locationCreateDocument = await readDocument('b2b-mixed-overlay-location-create.graphql');
const companyUpdateDocument = await readDocument('b2b-mixed-overlay-company-update.graphql');
const locationUpdateDocument = await readDocument('b2b-mixed-overlay-location-update.graphql');
const locationDeleteDocument = await readDocument('b2b-mixed-overlay-location-delete.graphql');
const companyDeleteDocument = await readDocument('b2b-mixed-overlay-company-delete.graphql');
const mixedReadDocument = await readDocument('b2b-mixed-overlay-read.graphql');

const cases: CapturedCase[] = [];
const cleanup: Array<{ type: string; id: string; response: ConformanceGraphqlResult<unknown> }> = [];
const createdCompanyIds: string[] = [];
const deletedCompanyIds = new Set<string>();
const unique = new Date().toISOString().replace(/\D/gu, '').slice(0, 14);
const token = `ZZZB2BMIX${unique}`;

const alphaCompanyName = `${token} Alpha Buyer`;
const alphaCompanyUpdatedName = `${token} Alpha Buyer Updated`;
const betaCompanyName = `${token} Beta Buyer`;
const gammaCompanyName = `${token} Gamma Buyer`;
const alphaHqName = `${token} Alpha HQ`;
const alphaHqUpdatedName = `${token} Alpha HQ Updated`;
const alphaAnnexName = `${token} Alpha Annex`;
const betaHqName = `${token} Beta HQ`;
const gammaHqName = `${token} Gamma HQ`;
const gammaRemoteName = `${token} Gamma Remote`;

const mixedReadVariables = {
  companyQuery: `name:${token}`,
  locationQuery: `name:${token}`,
  companiesFirst: 10,
  locationsFirst: 10,
  nestedLocationsFirst: 10,
};

function isRecord(value: unknown): value is JsonRecord {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function dataObject(result: ConformanceGraphqlResult<unknown>): JsonRecord {
  const data = result.payload.data;
  if (!isRecord(data)) {
    throw new Error(`Missing response data: ${JSON.stringify(result.payload, null, 2)}`);
  }
  return data;
}

function rootPayload(result: ConformanceGraphqlResult<unknown>, root: string): JsonRecord {
  const payload = dataObject(result)[root];
  if (!isRecord(payload)) {
    throw new Error(`Missing root payload ${root}: ${JSON.stringify(result.payload, null, 2)}`);
  }
  return payload;
}

function userErrors(result: ConformanceGraphqlResult<unknown>, root: string): UserError[] {
  const errors = rootPayload(result, root)['userErrors'];
  return Array.isArray(errors) ? (errors as UserError[]) : [];
}

function assertNoGraphqlErrors(result: ConformanceGraphqlResult<unknown>, label: string): void {
  if (result.status !== 200 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertNoUserErrors(result: ConformanceGraphqlResult<unknown>, root: string, label: string): void {
  assertNoGraphqlErrors(result, label);
  const errors = userErrors(result, root);
  if (errors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(errors, null, 2)}`);
  }
}

function readNestedId(result: ConformanceGraphqlResult<unknown>, root: string, nodeField: string): string {
  const node = rootPayload(result, root)[nodeField];
  if (!isRecord(node)) {
    throw new Error(`Missing ${root}.${nodeField}: ${JSON.stringify(result.payload, null, 2)}`);
  }
  const id = node['id'];
  if (typeof id !== 'string' || id.length === 0) {
    throw new Error(`Missing ${root}.${nodeField}.id: ${JSON.stringify(result.payload, null, 2)}`);
  }
  return id;
}

function connectionNodes(result: ConformanceGraphqlResult<unknown>, root: string): JsonRecord[] {
  const connection = dataObject(result)[root];
  if (!isRecord(connection) || !Array.isArray(connection['nodes'])) {
    throw new Error(`Missing ${root}.nodes: ${JSON.stringify(result.payload, null, 2)}`);
  }
  return connection['nodes'].filter(isRecord);
}

function locationIdByName(result: ConformanceGraphqlResult<unknown>, name: string): string {
  const match = connectionNodes(result, 'companyLocations').find((node) => node['name'] === name);
  const id = match?.['id'];
  if (typeof id !== 'string' || id.length === 0) {
    throw new Error(`Readback did not include location ${name}: ${JSON.stringify(result.payload, null, 2)}`);
  }
  return id;
}

function nodeStrings(nodes: JsonRecord[], key: string): string[] {
  return nodes.flatMap((node) => (typeof node[key] === 'string' ? [node[key] as string] : []));
}

function countValue(result: ConformanceGraphqlResult<unknown>): number | null {
  const count = rootPayload(result, 'companiesCount')['count'];
  return typeof count === 'number' ? count : null;
}

function mixedReadHasExpectedRows(
  result: ConformanceGraphqlResult<unknown>,
  expected: { companyNames: string[]; locationNames: string[]; companyCount: number },
): boolean {
  if (result.status !== 200 || result.payload.errors) return false;
  const companyNames = nodeStrings(connectionNodes(result, 'companies'), 'name');
  const locationNames = nodeStrings(connectionNodes(result, 'companyLocations'), 'name');
  return (
    expected.companyNames.every((name) => companyNames.includes(name)) &&
    expected.locationNames.every((name) => locationNames.includes(name)) &&
    countValue(result) === expected.companyCount
  );
}

async function captureCase(name: string, query: string, variables: JsonRecord): Promise<CapturedCase> {
  return {
    name,
    query,
    variables,
    response: await runGraphqlRequest(query, variables),
  };
}

async function captureMutation(
  name: string,
  query: string,
  variables: JsonRecord,
  root: string,
): Promise<CapturedCase> {
  const capture = await captureCase(name, query, variables);
  assertNoUserErrors(capture.response, root, name);
  cases.push(capture);
  return capture;
}

async function captureWhen(
  name: string,
  expected: { companyNames: string[]; locationNames: string[]; companyCount: number },
): Promise<CapturedCase> {
  let latest: CapturedCase | undefined;
  for (let attempt = 1; attempt <= 18; attempt += 1) {
    latest = await captureCase(name, mixedReadDocument, mixedReadVariables);
    assertNoGraphqlErrors(latest.response, `${name} attempt ${attempt}`);
    if (mixedReadHasExpectedRows(latest.response, expected)) {
      cases.push(latest);
      return latest;
    }
    await sleep(5_000);
  }
  throw new Error(`${name} never reached expected rows: ${JSON.stringify(latest?.response, null, 2)}`);
}

async function createCompany(
  name: string,
  externalId: string,
  locationName: string,
): Promise<{
  capture: CapturedCase;
  companyId: string;
}> {
  const capture = await captureMutation(
    `companyCreate ${name}`,
    companyCreateDocument,
    {
      input: {
        company: {
          name,
          externalId,
          note: 'B2B mixed overlay parity',
        },
        companyLocation: {
          name: locationName,
          externalId: `${externalId}-HQ`,
        },
      },
    },
    'companyCreate',
  );
  const companyId = readNestedId(capture.response, 'companyCreate', 'company');
  createdCompanyIds.push(companyId);
  return { capture, companyId };
}

async function createLocation(
  companyId: string,
  name: string,
  externalId: string,
): Promise<{ capture: CapturedCase; id: string }> {
  const capture = await captureMutation(
    `companyLocationCreate ${name}`,
    locationCreateDocument,
    {
      companyId,
      input: {
        name,
        externalId,
      },
    },
    'companyLocationCreate',
  );
  return {
    capture,
    id: readNestedId(capture.response, 'companyLocationCreate', 'companyLocation'),
  };
}

async function cleanupCompany(id: string): Promise<void> {
  cleanup.push({
    type: 'company',
    id,
    response: await runGraphqlRequest(companyDeleteDocument, { id }),
  });
}

let captureFailure: unknown = null;
let baselineRead: CapturedCase | null = null;

try {
  const alpha = await createCompany(alphaCompanyName, `${token}-ALPHA`, alphaHqName);
  const alphaAnnex = await createLocation(alpha.companyId, alphaAnnexName, `${token}-ALPHA-ANNEX`);
  const beta = await createCompany(betaCompanyName, `${token}-BETA`, betaHqName);

  baselineRead = await captureWhen('baseline B2B mixed read before staged overlay', {
    companyNames: [alphaCompanyName, betaCompanyName],
    locationNames: [alphaAnnexName, alphaHqName, betaHqName],
    companyCount: 1,
  });
  const alphaDefaultLocationId = locationIdByName(baselineRead.response, alphaHqName);

  const gamma = await createCompany(gammaCompanyName, `${token}-GAMMA`, gammaHqName);
  await createLocation(gamma.companyId, gammaRemoteName, `${token}-GAMMA-REMOTE`);

  await captureWhen('B2B mixed read after staged company and location create', {
    companyNames: [alphaCompanyName, betaCompanyName, gammaCompanyName],
    locationNames: [alphaAnnexName, alphaHqName, betaHqName, gammaHqName, gammaRemoteName],
    companyCount: 1,
  });

  await captureMutation(
    'companyUpdate baseline Alpha',
    companyUpdateDocument,
    {
      companyId: alpha.companyId,
      input: {
        name: alphaCompanyUpdatedName,
        externalId: `${token}-ALPHA-UPDATED`,
      },
    },
    'companyUpdate',
  );
  await captureWhen('B2B mixed read after baseline company update', {
    companyNames: [alphaCompanyUpdatedName, betaCompanyName, gammaCompanyName],
    locationNames: [alphaAnnexName, alphaHqName, betaHqName, gammaHqName, gammaRemoteName],
    companyCount: 1,
  });

  await captureMutation(
    'companyLocationUpdate baseline Alpha HQ',
    locationUpdateDocument,
    {
      companyLocationId: alphaDefaultLocationId,
      input: {
        name: alphaHqUpdatedName,
        externalId: `${token}-ALPHA-HQ-UPDATED`,
      },
    },
    'companyLocationUpdate',
  );
  await captureWhen('B2B mixed read after baseline location update', {
    companyNames: [alphaCompanyUpdatedName, betaCompanyName, gammaCompanyName],
    locationNames: [alphaAnnexName, alphaHqUpdatedName, betaHqName, gammaHqName, gammaRemoteName],
    companyCount: 1,
  });

  await captureMutation(
    'companyLocationDelete baseline Alpha Annex',
    locationDeleteDocument,
    { companyLocationId: alphaAnnex.id },
    'companyLocationDelete',
  );
  await captureWhen('B2B mixed read after baseline location delete', {
    companyNames: [alphaCompanyUpdatedName, betaCompanyName, gammaCompanyName],
    locationNames: [alphaHqUpdatedName, betaHqName, gammaHqName, gammaRemoteName],
    companyCount: 1,
  });

  await captureMutation('companyDelete baseline Beta', companyDeleteDocument, { id: beta.companyId }, 'companyDelete');
  deletedCompanyIds.add(beta.companyId);
  await captureWhen('B2B mixed read after baseline company delete', {
    companyNames: [alphaCompanyUpdatedName, gammaCompanyName],
    locationNames: [alphaHqUpdatedName, gammaHqName, gammaRemoteName],
    companyCount: 1,
  });
} catch (error) {
  captureFailure = error;
} finally {
  for (const id of createdCompanyIds.slice().reverse()) {
    if (deletedCompanyIds.has(id)) continue;
    await cleanupCompany(id);
  }
}

if (captureFailure) {
  throw captureFailure;
}
if (!baselineRead) {
  throw new Error('Missing baseline B2B mixed read for upstreamCalls cassette.');
}

const baselineUpstreamCall = {
  operationName: 'B2BMixedOverlayRead',
  variables: baselineRead.variables,
  query: mixedReadDocument,
  response: {
    status: baselineRead.response.status,
    body: baselineRead.response.payload,
  },
};

await mkdir(path.dirname(outputPath), { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      scope: 'B2B LiveHybrid baseline company/location catalog plus staged lifecycle overlay',
      token,
      cases,
      cleanup,
      upstreamCalls: Array.from({ length: 5 }, () => baselineUpstreamCall),
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
      token,
      cases: cases.map((entry) => ({ name: entry.name, status: entry.response.status })),
      cleanup: cleanup.map((entry) => ({ type: entry.type, id: entry.id, status: entry.response.status })),
      upstreamCalls: 5,
    },
    null,
    2,
  ),
);

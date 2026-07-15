/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonObject = Record<string, unknown>;

type CapturedStep = {
  operationName: string;
  query: string;
  variables: JsonObject;
  response: {
    status: number;
    body: unknown;
  };
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({
  adminOrigin,
  apiVersion,
});
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'customers');
const outputPath = path.join(outputDir, 'customer-live-hybrid-overlay.json');
const parityRequestDir = path.join('config', 'parity-requests', 'customers');
const createDocument = await readFile(
  path.join(parityRequestDir, 'customer-live-hybrid-overlay-create.graphql'),
  'utf8',
);
const readDocument = await readFile(path.join(parityRequestDir, 'customer-live-hybrid-overlay-read.graphql'), 'utf8');
const overlayHydrateDocument = await readFile(
  path.join(parityRequestDir, 'customer-live-hybrid-overlay-hydrate.graphql'),
  'utf8',
);
const duplicateHydrateDocument = await readFile(
  path.join(parityRequestDir, 'customer-duplicate-hydrate.graphql'),
  'utf8',
);
const countHydrateDocument = await readFile(path.join(parityRequestDir, 'customer-count-hydrate.graphql'), 'utf8');

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const cleanupDocument = `#graphql
mutation CustomerLiveHybridOverlayCleanup($input: CustomerDeleteInput!) {
  customerDelete(input: $input) {
    deletedCustomerId
    userErrors {
      field
      message
    }
  }
}
`;

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function assertGraphqlOk(label: string, result: ConformanceGraphqlResult): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

async function captureStep(operationName: string, query: string, variables: JsonObject): Promise<CapturedStep> {
  const result = await runGraphqlRequest(query, variables);
  assertGraphqlOk(operationName, result);
  return {
    operationName,
    query,
    variables,
    response: {
      status: result.status,
      body: result.payload,
    },
  };
}

function responseBody(step: CapturedStep): JsonObject {
  if (typeof step.response.body === 'object' && step.response.body !== null) {
    return step.response.body as JsonObject;
  }
  throw new Error(`${step.operationName} response body was not an object`);
}

function responseData(step: CapturedStep): JsonObject {
  const body = responseBody(step);
  if (typeof body['data'] === 'object' && body['data'] !== null) {
    return body['data'] as JsonObject;
  }
  throw new Error(`${step.operationName} response body did not contain data`);
}

function readCreatedCustomerId(step: CapturedStep): string {
  const data = responseData(step);
  const customerCreate = data['customerCreate'];
  if (typeof customerCreate !== 'object' || customerCreate === null) {
    throw new Error(`${step.operationName} did not return customerCreate`);
  }
  const customer = (customerCreate as JsonObject)['customer'];
  if (typeof customer !== 'object' || customer === null) {
    throw new Error(`${step.operationName} did not return customer`);
  }
  const id = (customer as JsonObject)['id'];
  if (typeof id !== 'string' || id.length === 0) {
    throw new Error(`${step.operationName} did not return a customer id`);
  }
  return id;
}

function assertNoCreateUserErrors(step: CapturedStep): void {
  const customerCreate = responseData(step)['customerCreate'] as JsonObject | undefined;
  const userErrors = customerCreate?.['userErrors'];
  if (!Array.isArray(userErrors) || userErrors.length > 0) {
    throw new Error(`${step.operationName} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function customerEmails(value: unknown): string[] {
  if (!Array.isArray(value)) return [];
  return value
    .map((node) => (typeof node === 'object' && node !== null ? (node as JsonObject)['email'] : null))
    .filter((email): email is string => typeof email === 'string');
}

function readFieldEmail(data: JsonObject, key: string): string | null {
  const field = data[key];
  if (field === null) return null;
  if (typeof field !== 'object' || field === undefined) return null;
  const email = (field as JsonObject)['email'];
  return typeof email === 'string' ? email : null;
}

function readCatalogEmails(data: JsonObject, key: string): string[] {
  const catalog = data[key];
  if (typeof catalog !== 'object' || catalog === null) return [];
  return customerEmails((catalog as JsonObject)['nodes']);
}

function readFirstPageEmails(data: JsonObject): string[] {
  const firstPage = data['firstPage'];
  if (typeof firstPage !== 'object' || firstPage === null) return [];
  const edges = (firstPage as JsonObject)['edges'];
  if (!Array.isArray(edges)) return [];
  return edges
    .map((edge) => {
      if (typeof edge !== 'object' || edge === null) return null;
      const node = (edge as JsonObject)['node'];
      if (typeof node !== 'object' || node === null) return null;
      const email = (node as JsonObject)['email'];
      return typeof email === 'string' ? email : null;
    })
    .filter((email): email is string => email !== null);
}

function readHasNextPage(data: JsonObject): boolean | null {
  const firstPage = data['firstPage'];
  if (typeof firstPage !== 'object' || firstPage === null) return null;
  const pageInfo = (firstPage as JsonObject)['pageInfo'];
  if (typeof pageInfo !== 'object' || pageInfo === null) return null;
  const hasNextPage = (pageInfo as JsonObject)['hasNextPage'];
  return typeof hasNextPage === 'boolean' ? hasNextPage : null;
}

function baseReadMatches(step: CapturedStep, baseEmail: string): boolean {
  const data = responseData(step);
  return (
    readFieldEmail(data, 'byRealId') === baseEmail &&
    readFieldEmail(data, 'byRealEmail') === baseEmail &&
    readFieldEmail(data, 'byStagedEmail') === null &&
    data['byMissingEmail'] === null &&
    readCatalogEmails(data, 'catalog').join('|') === baseEmail &&
    readFirstPageEmails(data).join('|') === baseEmail &&
    readHasNextPage(data) === false &&
    readCatalogEmails(data, 'matchingCatalog').length === 0
  );
}

function finalReadMatches(step: CapturedStep, baseEmail: string, stagedEmail: string): boolean {
  const data = responseData(step);
  return (
    readFieldEmail(data, 'byRealId') === baseEmail &&
    readFieldEmail(data, 'byRealEmail') === baseEmail &&
    readFieldEmail(data, 'byStagedEmail') === stagedEmail &&
    data['byMissingEmail'] === null &&
    readCatalogEmails(data, 'catalog').join('|') === `${baseEmail}|${stagedEmail}` &&
    readFirstPageEmails(data).join('|') === baseEmail &&
    readHasNextPage(data) === true &&
    readCatalogEmails(data, 'matchingCatalog').join('|') === stagedEmail
  );
}

function hydrateMatches(step: CapturedStep, emails: string[]): boolean {
  const data = responseData(step);
  const customers = data['customers'];
  if (typeof customers !== 'object' || customers === null) return false;
  return customerEmails((customers as JsonObject)['nodes']).join('|') === emails.join('|');
}

async function captureUntil(
  label: string,
  operationName: string,
  query: string,
  variables: JsonObject,
  matches: (step: CapturedStep) => boolean,
): Promise<CapturedStep> {
  let lastStep: CapturedStep | null = null;
  for (let attempt = 1; attempt <= 20; attempt += 1) {
    lastStep = await captureStep(`${operationName}Attempt${attempt}`, query, variables);
    lastStep.operationName = operationName;
    if (matches(lastStep)) return lastStep;
    await sleep(1500);
  }
  throw new Error(`${label} did not observe expected indexed state: ${JSON.stringify(lastStep, null, 2)}`);
}

async function cleanupCustomer(id: string | null): Promise<unknown> {
  if (!id) return null;
  try {
    const result = await runGraphqlRequest(cleanupDocument, { input: { id } });
    return {
      query: cleanupDocument,
      variables: { input: { id } },
      response: {
        status: result.status,
        body: result.payload,
      },
    };
  } catch (error) {
    return { error: error instanceof Error ? error.message : String(error) };
  }
}

function upstreamCall(step: CapturedStep): JsonObject {
  return {
    method: 'POST',
    path: `/admin/api/${apiVersion}/graphql.json`,
    apiSurface: 'admin',
    apiVersion,
    operationName: step.operationName,
    variables: step.variables,
    query: step.query,
    response: step.response,
  };
}

const stamp = Date.now().toString(36);
const commonTag = `livehybridoverlay${stamp}`;
const baseEmail = `live-overlay-base-${stamp}@example.com`;
const stagedEmail = `live-overlay-staged-${stamp}@example.com`;
const missingEmail = `live-overlay-missing-${stamp}@example.com`;
const baseCreateVariables = {
  input: {
    email: baseEmail,
    firstName: `OverlayBase${stamp}`,
    lastName: 'Live',
    tags: [commonTag],
  },
};
const stagedCreateVariables = {
  input: {
    email: stagedEmail,
    firstName: `OverlayStaged${stamp}`,
    lastName: 'Live',
    tags: [commonTag],
  },
};

let baseCustomerId: string | null = null;
let liveStagedCustomerId: string | null = null;

try {
  const baseCreate = await captureStep('CustomerLiveHybridOverlayBaseCreate', createDocument, baseCreateVariables);
  assertNoCreateUserErrors(baseCreate);
  baseCustomerId = readCreatedCustomerId(baseCreate);

  const readVariables = {
    realId: baseCustomerId,
    realEmail: baseEmail,
    stagedEmail,
    missingEmail,
    catalogQuery: `tag:${commonTag}`,
    stagedQuery: stagedEmail,
  };

  const duplicateHydrate = await captureStep('CustomerDuplicateHydrate', duplicateHydrateDocument, {
    query: `email:${stagedEmail}`,
  });
  const countHydrate = await captureStep('CustomerCountHydrate', countHydrateDocument, {});
  const baseRead = await captureUntil(
    'base overlay read',
    'CustomerLiveHybridOverlayRead',
    readDocument,
    readVariables,
    (step) => baseReadMatches(step, baseEmail),
  );
  const catalogHydrate = await captureUntil(
    'base catalog hydrate',
    'CustomerOverlayCatalogHydrate',
    overlayHydrateDocument,
    { query: `tag:${commonTag}` },
    (step) => hydrateMatches(step, [baseEmail]),
  );
  const firstPageHydrate = await captureStep('CustomerOverlayCatalogHydrate', overlayHydrateDocument, {
    query: `tag:${commonTag}`,
  });
  if (!hydrateMatches(firstPageHydrate, [baseEmail])) {
    throw new Error('firstPage hydrate did not return the base customer only');
  }
  const matchingCatalogHydrate = await captureStep('CustomerOverlayCatalogHydrate', overlayHydrateDocument, {
    query: stagedEmail,
  });
  if (!hydrateMatches(matchingCatalogHydrate, [])) {
    throw new Error('matchingCatalog hydrate unexpectedly returned customers before staged create');
  }
  const matchingCountHydrate = await captureStep('CustomerOverlayCatalogHydrate', overlayHydrateDocument, {
    query: stagedEmail,
  });
  if (!hydrateMatches(matchingCountHydrate, [])) {
    throw new Error('matchingCount hydrate unexpectedly returned customers before staged create');
  }

  const liveStagedCreate = await captureStep('CustomerLiveHybridOverlayCreate', createDocument, stagedCreateVariables);
  assertNoCreateUserErrors(liveStagedCreate);
  liveStagedCustomerId = readCreatedCustomerId(liveStagedCreate);

  const finalRead = await captureUntil(
    'final overlay read',
    'CustomerLiveHybridOverlayRead',
    readDocument,
    readVariables,
    (step) => finalReadMatches(step, baseEmail, stagedEmail),
  );

  const cleanup = {
    staged: await cleanupCustomer(liveStagedCustomerId),
    base: await cleanupCustomer(baseCustomerId),
  };

  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        scenarioId: 'customer-live-hybrid-overlay',
        storeDomain,
        apiVersion,
        proxyVariables: {
          create: stagedCreateVariables,
          read: readVariables,
        },
        setup: {
          baseCreate,
          duplicateHydrate,
          countHydrate,
          baseRead,
          catalogHydrate,
          firstPageHydrate,
          matchingCatalogHydrate,
          matchingCountHydrate,
        },
        liveStagedCreate,
        finalRead,
        cleanup,
        upstreamCalls: [
          duplicateHydrate,
          countHydrate,
          baseRead,
          catalogHydrate,
          firstPageHydrate,
          matchingCatalogHydrate,
          matchingCountHydrate,
        ].map(upstreamCall),
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
        baseEmail,
        stagedEmail,
      },
      null,
      2,
    ),
  );
} catch (error) {
  const cleanup = {
    staged: await cleanupCustomer(liveStagedCustomerId),
    base: await cleanupCustomer(baseCustomerId),
  };
  console.error(JSON.stringify({ ok: false, cleanup }, null, 2));
  throw error;
}

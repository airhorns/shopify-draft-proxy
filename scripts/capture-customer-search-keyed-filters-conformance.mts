// @ts-nocheck
/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'customers');
const parityRequestDir = path.join('config', 'parity-requests', 'customers');
const createDocument = await readFile(path.join(parityRequestDir, 'customers-keyed-search-create.graphql'), 'utf8');
const readDocument = await readFile(path.join(parityRequestDir, 'customers-keyed-search-read.graphql'), 'utf8');
const outputPath = path.join(outputDir, 'customers-keyed-search-filters.json');
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const cleanupDocument = `#graphql
  mutation CustomersKeyedSearchCleanup($input: CustomerDeleteInput!) {
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

function assertGraphqlOk(label: string, result: unknown): void {
  if (result.status < 200 || result.status >= 300 || result.payload?.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function readCustomerId(step: unknown, label: string): string {
  const id = step.response?.data?.customerCreate?.customer?.id;
  if (typeof id !== 'string' || id.length === 0) {
    throw new Error(`${label} did not return a customer id: ${JSON.stringify(step.response, null, 2)}`);
  }
  return id;
}

function assertNoCreateUserErrors(step: unknown, label: string): void {
  const userErrors = step.response?.data?.customerCreate?.userErrors;
  if (!Array.isArray(userErrors) || userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

async function captureStep(label: string, query: string, variables: Record<string, unknown>): Promise<unknown> {
  const result = await runGraphqlRequest(query, variables);
  assertGraphqlOk(label, result);
  return { query, variables, response: result.payload };
}

function searchReadMatches(response: unknown, expectedCanadaEmail: string, expectedUsEmail: string): boolean {
  const data = response?.data;
  const byCountry = data?.byCountry?.nodes?.map((node: unknown) => node.email) ?? [];
  const byState = data?.byState?.nodes?.map((node: unknown) => node.email).sort() ?? [];
  const byDefault = data?.byDefault?.nodes?.map((node: unknown) => node.email) ?? [];
  const byGroupedOr = data?.byGroupedOr?.nodes?.map((node: unknown) => node.email).sort() ?? [];
  const byGroupedExclusion = data?.byGroupedExclusion?.nodes?.map((node: unknown) => node.email) ?? [];
  return (
    byCountry.length === 1 &&
    byCountry[0] === expectedCanadaEmail &&
    byState.join('|') === [expectedCanadaEmail, expectedUsEmail].sort().join('|') &&
    byDefault.length === 1 &&
    byDefault[0] === expectedCanadaEmail &&
    byGroupedOr.join('|') === [expectedCanadaEmail, expectedUsEmail].sort().join('|') &&
    byGroupedExclusion.length === 1 &&
    byGroupedExclusion[0] === expectedUsEmail
  );
}

async function captureSearchRead(
  variables: Record<string, unknown>,
  expectedCanadaEmail: string,
  expectedUsEmail: string,
): Promise<unknown> {
  let lastStep: unknown = null;
  for (let attempt = 1; attempt <= 20; attempt += 1) {
    lastStep = await captureStep(`customers keyed search read attempt ${attempt}`, readDocument, variables);
    if (searchReadMatches(lastStep.response, expectedCanadaEmail, expectedUsEmail)) {
      return lastStep;
    }
    await sleep(1500);
  }
  throw new Error(
    `customers keyed search read did not observe expected indexed customers: ${JSON.stringify(lastStep, null, 2)}`,
  );
}

async function cleanupCustomer(id: string): Promise<unknown> {
  return captureStep('customers keyed search cleanup', cleanupDocument, { input: { id } }).catch((error) => ({
    error: error instanceof Error ? error.message : String(error),
  }));
}

const stamp = Date.now().toString(36);
const commonTag = `keyedsearch${stamp}`;
const canadaTag = `keyedcanada${stamp}`;
const usTag = `keyedus${stamp}`;
const canadaEmail = `keyed-ca-${stamp}@example.com`;
const usEmail = `keyed-us-${stamp}@example.com`;
const canadianCreateVariables = {
  input: {
    email: canadaEmail,
    firstName: `KeyedCanada${stamp}`,
    lastName: 'Search',
    tags: [commonTag, canadaTag, 'VIP'],
    addresses: [
      {
        address1: '1 King St W',
        city: 'Toronto',
        provinceCode: 'ON',
        countryCode: 'CA',
        zip: 'M5H 1A1',
      },
    ],
  },
};
const usCreateVariables = {
  input: {
    email: usEmail,
    firstName: `KeyedUs${stamp}`,
    lastName: 'Search',
    tags: [commonTag, usTag, 'standard'],
    addresses: [
      {
        address1: '600 4th Ave',
        city: 'Seattle',
        provinceCode: 'WA',
        countryCode: 'US',
        zip: '98104',
      },
    ],
  },
};
const searchReadVariables = {
  countryQuery: `country:Canada tag:${canadaTag}`,
  stateQuery: `state:DISABLED tag:${commonTag}`,
  defaultQuery: canadaEmail,
  orQuery: `(tag:${canadaTag} OR tag:${usTag}) state:DISABLED`,
  exclusionQuery: `tag:${commonTag} -tag:${canadaTag}`,
};

let canadianCustomerId: string | null = null;
let usCustomerId: string | null = null;
let canadianCreate: unknown = null;
let usCreate: unknown = null;
let searchRead: unknown = null;
let cleanup: unknown = null;

try {
  canadianCreate = await captureStep(
    'customers keyed search Canada customerCreate',
    createDocument,
    canadianCreateVariables,
  );
  assertNoCreateUserErrors(canadianCreate, 'Canada customerCreate');
  canadianCustomerId = readCustomerId(canadianCreate, 'Canada customerCreate');

  usCreate = await captureStep('customers keyed search US customerCreate', createDocument, usCreateVariables);
  assertNoCreateUserErrors(usCreate, 'US customerCreate');
  usCustomerId = readCustomerId(usCreate, 'US customerCreate');

  searchRead = await captureSearchRead(searchReadVariables, canadaEmail, usEmail);

  cleanup = {
    canada: await cleanupCustomer(canadianCustomerId),
    us: await cleanupCustomer(usCustomerId),
  };

  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        proxyVariables: canadianCreateVariables,
        canadianCreate,
        usCreate,
        searchRead,
        cleanup,
        upstreamCalls: [],
      },
      null,
      2,
    )}\n`,
    'utf8',
  );

  console.log(JSON.stringify({ ok: true, outputPath, canadaEmail, usEmail }, null, 2));
} catch (error) {
  cleanup = {
    canada: canadianCustomerId ? await cleanupCustomer(canadianCustomerId) : null,
    us: usCustomerId ? await cleanupCustomer(usCustomerId) : null,
  };
  console.error(JSON.stringify({ ok: false, cleanup }, null, 2));
  throw error;
}

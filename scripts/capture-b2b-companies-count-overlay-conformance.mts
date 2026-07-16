/* oxlint-disable no-console -- CLI capture scripts intentionally write status output. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import {
  createAdminGraphqlClient,
  type AdminGraphqlClient,
  type ConformanceGraphqlResult,
} from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;
type Capture = {
  request: {
    query: string;
    variables: JsonRecord;
  };
  response: ConformanceGraphqlResult;
};

const apiVersion = '2026-04';
const scenarioId = 'b2b-companies-count-overlay';
const { storeDomain, adminOrigin } = readConformanceScriptConfig({
  defaultApiVersion: apiVersion,
  exitOnMissing: true,
});
const outputPath = path.join('fixtures/conformance', storeDomain, apiVersion, 'b2b', `${scenarioId}.json`);

async function readText(filePath: string): Promise<string> {
  return await readFile(filePath, 'utf8');
}

async function sleep(ms: number): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, ms));
}

function isRecord(value: unknown): value is JsonRecord {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function readPath(value: unknown, segments: Array<string | number>): unknown {
  let current = value;
  for (const segment of segments) {
    if (Array.isArray(current) && typeof segment === 'number') {
      current = current[segment];
    } else if (isRecord(current) && typeof segment === 'string') {
      current = current[segment];
    } else {
      return undefined;
    }
  }
  return current;
}

function readStringPath(value: unknown, segments: Array<string | number>, context: string): string {
  const pathValue = readPath(value, segments);
  if (typeof pathValue !== 'string' || pathValue.length === 0) {
    throw new Error(`${context} did not return a string at ${segments.join('.')}: ${JSON.stringify(value, null, 2)}`);
  }
  return pathValue;
}

function readCount(capture: Capture, context: string): number {
  const count = readPath(capture.response.payload, ['data', 'companiesCount', 'count']);
  if (typeof count !== 'number') {
    throw new Error(`${context} did not return companiesCount.count: ${JSON.stringify(capture.response, null, 2)}`);
  }
  return count;
}

function assertGraphqlOk(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertNoUserErrors(payload: unknown, segments: Array<string | number>, context: string): void {
  const userErrors = readPath(payload, segments);
  if (!Array.isArray(userErrors) || userErrors.length !== 0) {
    throw new Error(`${context} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

async function makeClient(): Promise<AdminGraphqlClient> {
  const token = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
  return createAdminGraphqlClient({
    adminOrigin,
    apiVersion,
    headers: buildAdminAuthHeaders(token),
  });
}

async function captureGraphql(
  client: AdminGraphqlClient,
  query: string,
  variables: JsonRecord,
  context: string,
): Promise<Capture> {
  const response = await client.runGraphqlRequest(query, variables);
  assertGraphqlOk(response, context);
  return {
    request: { query, variables },
    response,
  };
}

async function cleanupCompany(
  client: AdminGraphqlClient,
  query: string,
  id: string,
): Promise<{ id: string; response: ConformanceGraphqlResult }> {
  const response = await client.runGraphqlRequest(query, { id });
  return { id, response };
}

async function captureCountWithRetry(
  client: AdminGraphqlClient,
  query: string,
  variables: JsonRecord,
  expectedCount: number,
  context: string,
): Promise<Capture> {
  let last: Capture | null = null;
  for (let attempt = 1; attempt <= 10; attempt += 1) {
    last = await captureGraphql(client, query, variables, `${context} attempt ${attempt}`);
    if (readCount(last, context) === expectedCount) {
      return last;
    }
    await sleep(3_000);
  }
  throw new Error(`${context} did not reach count ${expectedCount}: ${JSON.stringify(last?.response, null, 2)}`);
}

async function main(): Promise<void> {
  const client = await makeClient();
  const companyCreateQuery = await readText('config/parity-requests/b2b/b2b-companies-count-overlay-create.graphql');
  const countQuery = await readText('config/parity-requests/b2b/b2b-companies-count-overlay-count.graphql');
  const smallPageQuery = await readText('config/parity-requests/b2b/b2b-companies-count-overlay-small-page.graphql');
  const cleanupQuery = `#graphql
    mutation B2BCompaniesCountOverlayCleanup($id: ID!) {
      companyDelete(id: $id) {
        deletedCompanyId
        userErrors { field message code }
      }
    }
  `;

  const token = `ZZZB2BCOUNT${new Date()
    .toISOString()
    .replace(/[-:.TZ]/gu, '')
    .slice(0, 14)}`;
  const createdCompanyIds: string[] = [];
  const cleanup: Array<{ id: string; response: ConformanceGraphqlResult }> = [];
  let captureFailure: unknown = null;
  let baselineAlphaCreate: Capture | null = null;
  let baselineBetaCreate: Capture | null = null;
  let baselineCountOnly: Capture | null = null;
  let baselineSmallPageAndCount: Capture | null = null;
  let companyCreate: Capture | null = null;
  let countOnlyAfterCreate: Capture | null = null;
  let smallPageAndCountAfterCreate: Capture | null = null;

  try {
    const baselineAlphaVariables = {
      input: {
        company: {
          name: `${token} Alpha Buyer`,
          externalId: `${token}-ALPHA`,
          note: 'B2B companiesCount overlay parity baseline',
        },
      },
    };
    baselineAlphaCreate = await captureGraphql(
      client,
      companyCreateQuery,
      baselineAlphaVariables,
      'B2B companiesCount baseline alpha companyCreate',
    );
    assertNoUserErrors(
      baselineAlphaCreate.response.payload,
      ['data', 'companyCreate', 'userErrors'],
      'baseline alpha companyCreate',
    );
    createdCompanyIds.push(
      readStringPath(
        baselineAlphaCreate.response.payload,
        ['data', 'companyCreate', 'company', 'id'],
        'baseline alpha companyCreate',
      ),
    );

    const baselineBetaVariables = {
      input: {
        company: {
          name: `${token} Beta Buyer`,
          externalId: `${token}-BETA`,
          note: 'B2B companiesCount overlay parity baseline',
        },
      },
    };
    baselineBetaCreate = await captureGraphql(
      client,
      companyCreateQuery,
      baselineBetaVariables,
      'B2B companiesCount baseline beta companyCreate',
    );
    assertNoUserErrors(
      baselineBetaCreate.response.payload,
      ['data', 'companyCreate', 'userErrors'],
      'baseline beta companyCreate',
    );
    createdCompanyIds.push(
      readStringPath(
        baselineBetaCreate.response.payload,
        ['data', 'companyCreate', 'company', 'id'],
        'baseline beta companyCreate',
      ),
    );

    baselineCountOnly = await captureGraphql(client, countQuery, {}, 'B2B companiesCount baseline count-only');
    const baselineCount = readCount(baselineCountOnly, 'baseline count-only');
    const smallPageVariables = { first: 1 };
    baselineSmallPageAndCount = await captureGraphql(
      client,
      smallPageQuery,
      smallPageVariables,
      'B2B companiesCount baseline small-page-and-count',
    );
    if (readCount(baselineSmallPageAndCount, 'baseline small-page-and-count') !== baselineCount) {
      throw new Error('Baseline count-only and small-page count responses disagreed.');
    }
    if (baselineCount <= smallPageVariables.first) {
      throw new Error(`Expected baseline company count > page size ${smallPageVariables.first}, got ${baselineCount}.`);
    }

    const stagedCreateVariables = {
      input: {
        company: {
          name: `${token} Gamma Buyer`,
          externalId: `${token}-GAMMA`,
          note: 'B2B companiesCount overlay parity staged delta',
        },
      },
    };
    companyCreate = await captureGraphql(
      client,
      companyCreateQuery,
      stagedCreateVariables,
      'B2B companiesCount staged companyCreate',
    );
    assertNoUserErrors(companyCreate.response.payload, ['data', 'companyCreate', 'userErrors'], 'staged companyCreate');
    createdCompanyIds.push(
      readStringPath(
        companyCreate.response.payload,
        ['data', 'companyCreate', 'company', 'id'],
        'staged companyCreate',
      ),
    );

    const expectedAfterCreateCount = baselineCount + 1;
    countOnlyAfterCreate = await captureCountWithRetry(
      client,
      countQuery,
      {},
      expectedAfterCreateCount,
      'B2B companiesCount count-only after create',
    );
    smallPageAndCountAfterCreate = await captureCountWithRetry(
      client,
      smallPageQuery,
      smallPageVariables,
      expectedAfterCreateCount,
      'B2B companiesCount small-page-and-count after create',
    );
  } catch (error) {
    captureFailure = error;
  } finally {
    for (const id of createdCompanyIds.slice().reverse()) {
      cleanup.push(await cleanupCompany(client, cleanupQuery, id));
    }
  }

  if (captureFailure) {
    throw captureFailure;
  }
  if (
    !baselineCountOnly ||
    !baselineSmallPageAndCount ||
    !companyCreate ||
    !countOnlyAfterCreate ||
    !smallPageAndCountAfterCreate
  ) {
    throw new Error('B2B companiesCount overlay capture did not collect every required case.');
  }

  const upstreamCalls = [
    {
      operationName: 'B2BCompaniesCountOverlayCountOnly',
      variables: baselineCountOnly.request.variables,
      query: baselineCountOnly.request.query,
      response: {
        status: baselineCountOnly.response.status,
        body: baselineCountOnly.response.payload,
      },
    },
    {
      operationName: 'B2BCompaniesCountOverlaySmallPage',
      variables: baselineSmallPageAndCount.request.variables,
      query: baselineSmallPageAndCount.request.query,
      response: {
        status: baselineSmallPageAndCount.response.status,
        body: baselineSmallPageAndCount.response.payload,
      },
    },
  ];

  await mkdir(path.dirname(outputPath), { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        scenarioId,
        scope: 'B2B companiesCount count-only and small-page overlay over a larger live company catalog',
        token,
        baselineAlphaCreate,
        baselineBetaCreate,
        baselineCountOnly,
        baselineSmallPageAndCount,
        companyCreate,
        countOnlyAfterCreate,
        smallPageAndCountAfterCreate,
        cleanup,
        upstreamCalls,
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
        baselineCount: readCount(baselineCountOnly, 'baseline count-only'),
        afterCreateCount: readCount(countOnlyAfterCreate, 'count-only after create'),
        upstreamCalls: upstreamCalls.length,
      },
      null,
      2,
    ),
  );
}

main().catch((error: unknown) => {
  console.error(error);
  process.exit(1);
});

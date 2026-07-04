/* oxlint-disable no-console -- CLI capture scripts intentionally write status output. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import {
  createAdminGraphqlClient,
  type AdminGraphqlClient,
  type ConformanceGraphqlPayload,
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
  response: ConformanceGraphqlPayload;
};

const apiVersion = '2025-01';
const scenarioId = 'b2b-connection-arguments';
const { storeDomain, adminOrigin } = readConformanceScriptConfig({
  defaultApiVersion: apiVersion,
  exitOnMissing: true,
});

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

function assertEqual(actual: unknown, expected: unknown, context: string): void {
  if (actual !== expected) {
    throw new Error(`${context}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`);
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
    response: response.payload,
  };
}

async function writeJson(outputPath: string, body: JsonRecord): Promise<void> {
  await mkdir(path.dirname(outputPath), { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(body, null, 2)}\n`, 'utf8');
}

async function main(): Promise<void> {
  const client = await makeClient();
  const companyCreateQuery = await readText('config/parity-requests/b2b/b2b-connection-args-company-create.graphql');
  const contactCreateQuery = await readText('config/parity-requests/b2b/b2b-connection-args-contact-create.graphql');
  const locationCreateQuery = await readText('config/parity-requests/b2b/b2b-connection-args-location-create.graphql');
  const assignRoleQuery = await readText('config/parity-requests/b2b/b2b-connection-args-assign-role.graphql');
  const readQuery = await readText('config/parity-requests/b2b/b2b-connection-args-read.graphql');
  const cleanupQuery = `#graphql
    mutation B2BConnectionArgsCleanup($id: ID!) {
      companyDelete(id: $id) {
        deletedCompanyId
        userErrors { field message code }
      }
    }
  `;

  const timestamp = Date.now();
  const companySearchToken = `har1975b2bcompany${timestamp}`;
  const locationSearchToken = `har1975b2blocation${timestamp}`;
  const acmeName = `Zzzzy ${companySearchToken} ${locationSearchToken} Acme`;
  const zetaName = `Zzzzx ${companySearchToken} Zeta`;
  const remoteLocationName = `Zzzzz ${locationSearchToken} Remote`;
  const primaryTitle = 'Primary buyer';
  const secondaryTitle = 'Secondary buyer';
  const companyCreateVariables = {
    acmeInput: {
      company: {
        name: acmeName,
        note: 'Connection argument parity fixture',
        externalId: `connection-args-acme-${timestamp}`,
      },
      companyContact: {
        title: primaryTitle,
        email: `connection-args-primary-${timestamp}@example.com`,
      },
    },
    zetaInput: {
      company: {
        name: zetaName,
        note: 'Connection argument parity fixture',
        externalId: `connection-args-zeta-${timestamp}`,
      },
    },
  };

  let acmeCompanyId: string | null = null;
  let zetaCompanyId: string | null = null;
  try {
    const companyCreate = await captureGraphql(
      client,
      companyCreateQuery,
      companyCreateVariables,
      'B2B connection args companyCreate',
    );
    assertNoUserErrors(companyCreate.response, ['data', 'acme', 'userErrors'], 'acme companyCreate');
    assertNoUserErrors(companyCreate.response, ['data', 'zeta', 'userErrors'], 'zeta companyCreate');
    acmeCompanyId = readStringPath(companyCreate.response, ['data', 'acme', 'company', 'id'], 'acme companyCreate');
    zetaCompanyId = readStringPath(companyCreate.response, ['data', 'zeta', 'company', 'id'], 'zeta companyCreate');
    const mainContactId = readStringPath(
      companyCreate.response,
      ['data', 'acme', 'company', 'mainContact', 'id'],
      'acme main contact',
    );
    const roleId = readStringPath(
      companyCreate.response,
      ['data', 'acme', 'company', 'contactRoles', 'nodes', 0, 'id'],
      'acme role',
    );
    const defaultLocationName = readStringPath(
      companyCreate.response,
      ['data', 'acme', 'company', 'locations', 'nodes', 0, 'name'],
      'acme default location',
    );

    const contactCreate = await captureGraphql(
      client,
      contactCreateQuery,
      {
        companyId: acmeCompanyId,
        input: {
          title: secondaryTitle,
          email: `connection-args-secondary-${timestamp}@example.com`,
        },
      },
      'B2B connection args contactCreate',
    );
    assertNoUserErrors(contactCreate.response, ['data', 'companyContactCreate', 'userErrors'], 'contactCreate');
    const secondaryContactId = readStringPath(
      contactCreate.response,
      ['data', 'companyContactCreate', 'companyContact', 'id'],
      'secondary contactCreate',
    );

    const locationCreate = await captureGraphql(
      client,
      locationCreateQuery,
      {
        companyId: acmeCompanyId,
        input: { name: remoteLocationName },
      },
      'B2B connection args locationCreate',
    );
    assertNoUserErrors(locationCreate.response, ['data', 'companyLocationCreate', 'userErrors'], 'locationCreate');
    const remoteLocationId = readStringPath(
      locationCreate.response,
      ['data', 'companyLocationCreate', 'companyLocation', 'id'],
      'remote locationCreate',
    );

    const secondaryRoleAssign = await captureGraphql(
      client,
      assignRoleQuery,
      {
        companyContactId: secondaryContactId,
        companyContactRoleId: roleId,
        companyLocationId: remoteLocationId,
      },
      'B2B connection args secondary assign role',
    );
    assertNoUserErrors(
      secondaryRoleAssign.response,
      ['data', 'companyContactAssignRole', 'userErrors'],
      'secondary role assignment',
    );
    readStringPath(
      secondaryRoleAssign.response,
      ['data', 'companyContactAssignRole', 'companyContactRoleAssignment', 'id'],
      'secondary role assignment',
    );

    const mainRoleAssign = await captureGraphql(
      client,
      assignRoleQuery,
      {
        companyContactId: mainContactId,
        companyContactRoleId: roleId,
        companyLocationId: remoteLocationId,
      },
      'B2B connection args main assign role',
    );
    assertNoUserErrors(
      mainRoleAssign.response,
      ['data', 'companyContactAssignRole', 'userErrors'],
      'main role assignment',
    );

    const readPageOneVariables = {
      companyQuery: `name:${companySearchToken}`,
      locationQuery: `name:${locationSearchToken}`,
      companyId: acmeCompanyId,
      contactId: mainContactId,
      locationId: remoteLocationId,
    };
    let readPageOne: Capture | null = null;
    for (let attempt = 1; attempt <= 12; attempt += 1) {
      const candidate = await captureGraphql(
        client,
        readQuery,
        readPageOneVariables,
        `B2B connection args read page one attempt ${attempt}`,
      );
      if (
        readPath(candidate.response, ['data', 'companies', 'nodes', 0, 'name']) === acmeName &&
        readPath(candidate.response, ['data', 'companies', 'pageInfo', 'hasNextPage']) === true &&
        readPath(candidate.response, ['data', 'companyLocations', 'nodes', 0, 'name']) === remoteLocationName &&
        readPath(candidate.response, ['data', 'companyLocations', 'pageInfo', 'hasNextPage']) === true
      ) {
        readPageOne = candidate;
        break;
      }
      await sleep(5_000);
    }
    if (!readPageOne) {
      throw new Error('B2B connection args read page one did not become searchable before timeout');
    }
    assertEqual(
      readPath(readPageOne.response, ['data', 'companies', 'nodes', 0, 'name']),
      acmeName,
      'companies sort/reverse page one',
    );
    assertEqual(readPath(readPageOne.response, ['data', 'companiesCount', 'count']), 1, 'companiesCount limited count');
    assertEqual(
      readPath(readPageOne.response, ['data', 'companiesCount', 'precision']),
      'AT_LEAST',
      'companiesCount limited precision',
    );
    assertEqual(
      readPath(readPageOne.response, ['data', 'companyLocations', 'nodes', 0, 'name']),
      remoteLocationName,
      'companyLocations sort/reverse page one',
    );
    assertEqual(
      readPath(readPageOne.response, ['data', 'company', 'contacts', 'nodes', 0, 'title']),
      secondaryTitle,
      'Company.contacts sort/reverse page one',
    );
    assertEqual(
      readPath(readPageOne.response, ['data', 'company', 'locations', 'nodes', 0, 'name']),
      remoteLocationName,
      'Company.locations sort/reverse page one',
    );
    assertEqual(
      readPath(readPageOne.response, [
        'data',
        'companyContact',
        'roleAssignments',
        'nodes',
        0,
        'companyLocation',
        'name',
      ]),
      remoteLocationName,
      'CompanyContact.roleAssignments sort/reverse page one',
    );
    assertEqual(
      readPath(readPageOne.response, [
        'data',
        'companyLocation',
        'roleAssignments',
        'nodes',
        0,
        'companyContact',
        'title',
      ]),
      primaryTitle,
      'CompanyLocation.roleAssignments sort/reverse page one',
    );

    const readPageTwoVariables = {
      ...readPageOneVariables,
      afterCompany: readStringPath(
        readPageOne.response,
        ['data', 'companies', 'pageInfo', 'endCursor'],
        'companies page one cursor',
      ),
      afterLocation: readStringPath(
        readPageOne.response,
        ['data', 'companyLocations', 'pageInfo', 'endCursor'],
        'companyLocations page one cursor',
      ),
      afterCompanyLocation: readStringPath(
        readPageOne.response,
        ['data', 'company', 'locations', 'pageInfo', 'endCursor'],
        'Company.locations page one cursor',
      ),
      afterContact: readStringPath(
        readPageOne.response,
        ['data', 'company', 'contacts', 'pageInfo', 'endCursor'],
        'Company.contacts page one cursor',
      ),
      afterContactAssignment: readStringPath(
        readPageOne.response,
        ['data', 'companyContact', 'roleAssignments', 'pageInfo', 'endCursor'],
        'CompanyContact.roleAssignments page one cursor',
      ),
      afterLocationAssignment: readStringPath(
        readPageOne.response,
        ['data', 'companyLocation', 'roleAssignments', 'pageInfo', 'endCursor'],
        'CompanyLocation.roleAssignments page one cursor',
      ),
    };
    let readPageTwo: Capture | null = null;
    for (let attempt = 1; attempt <= 12; attempt += 1) {
      const candidate = await captureGraphql(
        client,
        readQuery,
        readPageTwoVariables,
        `B2B connection args read page two attempt ${attempt}`,
      );
      if (
        readPath(candidate.response, ['data', 'companies', 'nodes', 0, 'name']) === zetaName &&
        readPath(candidate.response, ['data', 'companies', 'pageInfo', 'hasNextPage']) === false &&
        readPath(candidate.response, ['data', 'companyLocations', 'nodes', 0, 'name']) === defaultLocationName
      ) {
        readPageTwo = candidate;
        break;
      }
      await sleep(5_000);
    }
    if (!readPageTwo) {
      throw new Error('B2B connection args read page two did not settle before timeout');
    }
    assertEqual(
      readPath(readPageTwo.response, ['data', 'companies', 'nodes', 0, 'name']),
      zetaName,
      'companies cursor window page two',
    );
    assertEqual(
      readPath(readPageTwo.response, ['data', 'companyLocations', 'nodes', 0, 'name']),
      defaultLocationName,
      'companyLocations cursor window page two',
    );
    assertEqual(
      readPath(readPageTwo.response, ['data', 'company', 'contacts', 'nodes', 0, 'title']),
      primaryTitle,
      'Company.contacts cursor window page two',
    );
    assertEqual(
      readPath(readPageTwo.response, ['data', 'company', 'locations', 'nodes', 0, 'name']),
      defaultLocationName,
      'Company.locations cursor window page two',
    );
    assertEqual(
      readPath(readPageTwo.response, [
        'data',
        'companyContact',
        'roleAssignments',
        'nodes',
        0,
        'companyLocation',
        'name',
      ]),
      defaultLocationName,
      'CompanyContact.roleAssignments cursor window page two',
    );
    assertEqual(
      readPath(readPageTwo.response, [
        'data',
        'companyLocation',
        'roleAssignments',
        'nodes',
        0,
        'companyContact',
        'title',
      ]),
      secondaryTitle,
      'CompanyLocation.roleAssignments cursor window page two',
    );

    const cleanupAcme = await captureGraphql(
      client,
      cleanupQuery,
      { id: acmeCompanyId },
      'B2B connection args cleanup acme',
    );
    acmeCompanyId = null;
    const cleanupZeta = await captureGraphql(
      client,
      cleanupQuery,
      { id: zetaCompanyId },
      'B2B connection args cleanup zeta',
    );
    zetaCompanyId = null;

    const outputPath = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'b2b', `${scenarioId}.json`);
    await writeJson(outputPath, {
      scenarioId,
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      companyCreate,
      contactCreate,
      locationCreate,
      secondaryRoleAssign,
      mainRoleAssign,
      readPageOne,
      readPageTwo,
      cleanup: {
        acme: cleanupAcme,
        zeta: cleanupZeta,
      },
      upstreamCalls: [],
    });
    console.log(JSON.stringify({ ok: true, fixturePath: outputPath }, null, 2));
  } finally {
    if (acmeCompanyId) {
      await client.runGraphqlRequest(cleanupQuery, { id: acmeCompanyId });
    }
    if (zetaCompanyId) {
      await client.runGraphqlRequest(cleanupQuery, { id: zetaCompanyId });
    }
  }
}

await main();

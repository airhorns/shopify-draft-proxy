import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;
type RecordedOperation = {
  request: {
    query: string;
    variables: JsonRecord;
  };
  response: JsonRecord;
};

const scenarioId = 'b2b-revoke-role-scope-preconditions';
const timestamp = Date.now();
const companyName = `B2B revoke role scope ${timestamp}`;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const companyCreateDocument = `#graphql
  mutation B2BRevokeRoleScopeCompanyCreate($input: CompanyCreateInput!) {
    companyCreate(input: $input) {
      company {
        id
        name
        mainContact {
          id
          roleAssignments(first: 5) { nodes { id } }
        }
        locations(first: 5) { nodes { id name } }
        contactRoles(first: 5) { nodes { id name } }
      }
      userErrors { field message code }
    }
  }
`;

const contactCreateDocument = `#graphql
  mutation B2BRevokeRoleScopeContactCreate($companyId: ID!, $input: CompanyContactInput!) {
    companyContactCreate(companyId: $companyId, input: $input) {
      companyContact { id company { id } }
      userErrors { field message code }
    }
  }
`;

const locationCreateDocument = `#graphql
  mutation B2BRevokeRoleScopeLocationCreate($companyId: ID!, $input: CompanyLocationInput!) {
    companyLocationCreate(companyId: $companyId, input: $input) {
      companyLocation { id name company { id } }
      userErrors { field message code }
    }
  }
`;

const contactAssignRolesDocument = `#graphql
  mutation B2BRevokeRoleScopeContactAssignRoles($companyContactId: ID!, $rolesToAssign: [CompanyContactRoleAssign!]!) {
    companyContactAssignRoles(companyContactId: $companyContactId, rolesToAssign: $rolesToAssign) {
      roleAssignments { id }
      userErrors { field message code }
    }
  }
`;

const locationAssignRolesDocument = `#graphql
  mutation B2BRevokeRoleScopeLocationAssignRoles($companyLocationId: ID!, $rolesToAssign: [CompanyLocationRoleAssign!]!) {
    companyLocationAssignRoles(companyLocationId: $companyLocationId, rolesToAssign: $rolesToAssign) {
      roleAssignments { id }
      userErrors { field message code }
    }
  }
`;

const contactRevokeRoleDocument = `#graphql
  mutation B2BRevokeRoleScopeContactRevokeRole($companyContactId: ID!, $companyContactRoleAssignmentId: ID!) {
    companyContactRevokeRole(
      companyContactId: $companyContactId
      companyContactRoleAssignmentId: $companyContactRoleAssignmentId
    ) {
      revokedCompanyContactRoleAssignmentId
      userErrors { field message code }
    }
  }
`;

const contactRevokeRolesDocument = `#graphql
  mutation B2BRevokeRoleScopeContactRevokeRoles(
    $companyContactId: ID!
    $roleAssignmentIds: [ID!]!
    $revokeAll: Boolean
  ) {
    companyContactRevokeRoles(
      companyContactId: $companyContactId
      roleAssignmentIds: $roleAssignmentIds
      revokeAll: $revokeAll
    ) {
      revokedRoleAssignmentIds
      userErrors { field message code }
    }
  }
`;

const locationRevokeRolesDocument = `#graphql
  mutation B2BRevokeRoleScopeLocationRevokeRoles($companyLocationId: ID!, $rolesToRevoke: [ID!]!) {
    companyLocationRevokeRoles(companyLocationId: $companyLocationId, rolesToRevoke: $rolesToRevoke) {
      revokedRoleAssignmentIds
      userErrors { field message code }
    }
  }
`;

const companyDeleteDocument = `#graphql
  mutation B2BRevokeRoleScopeCompanyDelete($id: ID!) {
    companyDelete(id: $id) {
      deletedCompanyId
      userErrors { field message code }
    }
  }
`;

function readRecord(value: unknown): JsonRecord | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readPath(value: unknown, pathSegments: string[]): unknown {
  let current = value;
  for (const segment of pathSegments) {
    if (Array.isArray(current)) {
      current = current[Number.parseInt(segment, 10)];
    } else {
      const record = readRecord(current);
      if (!record) {
        return undefined;
      }
      current = record[segment];
    }
  }
  return current;
}

function readStringAtPath(value: unknown, pathSegments: string[], label: string): string {
  const pathValue = readPath(value, pathSegments);
  if (typeof pathValue !== 'string' || pathValue.length === 0) {
    throw new Error(`${label} did not return a string at ${pathSegments.join('.')}: ${JSON.stringify(value, null, 2)}`);
  }
  return pathValue;
}

function readArrayAtPath(value: unknown, pathSegments: string[], label: string): unknown[] {
  const pathValue = readPath(value, pathSegments);
  if (!Array.isArray(pathValue)) {
    throw new Error(`${label} did not return an array at ${pathSegments.join('.')}: ${JSON.stringify(value, null, 2)}`);
  }
  return pathValue;
}

function readUserErrors(payload: unknown, root: string): JsonRecord[] {
  const value = readPath(payload, ['data', root, 'userErrors']);
  return Array.isArray(value) ? value.filter((item): item is JsonRecord => readRecord(item) !== null) : [];
}

function assertNoTopLevelErrors(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} returned top-level errors: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertSuccessful(result: ConformanceGraphqlResult, root: string, label: string): void {
  assertNoTopLevelErrors(result, label);
  const userErrors = readUserErrors(result.payload, root);
  if (userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function assertUserError(
  result: ConformanceGraphqlResult,
  root: string,
  expected: {
    field: string[] | null;
    code: string;
  },
  label: string,
): void {
  assertNoTopLevelErrors(result, label);
  const userErrors = readUserErrors(result.payload, root);
  const found = userErrors.some((error) => {
    const fieldMatches = JSON.stringify(error['field']) === JSON.stringify(expected.field);
    const codeMatches = error['code'] === expected.code;
    return fieldMatches && codeMatches;
  });
  if (!found) {
    throw new Error(
      `${label} did not include expected userError ${JSON.stringify(expected)}: ${JSON.stringify(
        result.payload,
        null,
        2,
      )}`,
    );
  }
}

function assertPathEquals(result: unknown, pathSegments: string[], expected: unknown, label: string): void {
  const actual = readPath(result, pathSegments);
  if (JSON.stringify(actual) !== JSON.stringify(expected)) {
    throw new Error(
      `${label} expected ${pathSegments.join('.')} to equal ${JSON.stringify(expected)}, got ${JSON.stringify(
        actual,
      )}: ${JSON.stringify(result, null, 2)}`,
    );
  }
}

function recordOperation(query: string, variables: JsonRecord, result: ConformanceGraphqlResult): RecordedOperation {
  return {
    request: { query, variables },
    response: {
      status: result.status,
      ...result.payload,
    },
  };
}

async function runRequired(
  query: string,
  variables: JsonRecord,
  root: string,
  label: string,
): Promise<RecordedOperation> {
  const result = await runGraphqlRequest(query, variables);
  assertSuccessful(result, root, label);
  return recordOperation(query, variables, result);
}

async function runValidation(
  query: string,
  variables: JsonRecord,
  root: string,
  expected: {
    field: string[] | null;
    code: string;
    detail?: string;
  },
  label: string,
): Promise<RecordedOperation> {
  const result = await runGraphqlRequest(query, variables);
  assertUserError(result, root, expected, label);
  return recordOperation(query, variables, result);
}

async function runCleanup(
  query: string,
  variables: JsonRecord,
  root: string,
  label: string,
): Promise<RecordedOperation> {
  const result = await runGraphqlRequest(query, variables);
  if (result.status >= 200 && result.status < 300 && !result.payload.errors) {
    return recordOperation(query, variables, result);
  }
  return recordOperation(query, variables, {
    status: result.status,
    payload: {
      ...result.payload,
      cleanupWarning: `${label} cleanup failed before userErrors could be inspected.`,
      cleanupRoot: root,
    },
  });
}

function companyInput(): JsonRecord {
  return {
    company: {
      name: companyName,
      note: 'HAR-762 B2B revoke-role scope parity',
      externalId: `har-762-revoke-scope-${timestamp}`,
    },
    companyContact: {
      firstName: 'Har',
      lastName: 'Revoke',
      email: `har-762-main-${timestamp}@example.com`,
      title: 'Main buyer',
    },
    companyLocation: {
      name: `${companyName} HQ`,
      phone: '+16135550762',
      billingAddress: {
        address1: '762 Scope Way',
        city: 'Toronto',
        countryCode: 'CA',
      },
    },
  };
}

let companyId: string | null = null;
let companyDeleted = false;
const cleanup: Record<string, RecordedOperation> = {};

try {
  const companyCreate = await runRequired(
    companyCreateDocument,
    { input: companyInput() },
    'companyCreate',
    'companyCreate setup',
  );
  companyId = readStringAtPath(companyCreate.response, ['data', 'companyCreate', 'company', 'id'], 'companyCreate');
  const mainContactId = readStringAtPath(
    companyCreate.response,
    ['data', 'companyCreate', 'company', 'mainContact', 'id'],
    'companyCreate main contact',
  );
  const mainLocationId = readStringAtPath(
    companyCreate.response,
    ['data', 'companyCreate', 'company', 'locations', 'nodes', '0', 'id'],
    'companyCreate main location',
  );
  const mainAssignmentId = readStringAtPath(
    companyCreate.response,
    ['data', 'companyCreate', 'company', 'mainContact', 'roleAssignments', 'nodes', '0', 'id'],
    'companyCreate automatic role assignment',
  );
  const locationAdminRoleId = readStringAtPath(
    companyCreate.response,
    ['data', 'companyCreate', 'company', 'contactRoles', 'nodes', '0', 'id'],
    'companyCreate first contact role',
  );
  const orderingOnlyRoleId = readStringAtPath(
    companyCreate.response,
    ['data', 'companyCreate', 'company', 'contactRoles', 'nodes', '1', 'id'],
    'companyCreate second contact role',
  );

  const secondaryContactCreate = await runRequired(
    contactCreateDocument,
    {
      companyId,
      input: {
        firstName: 'Har',
        lastName: 'Other Contact',
        email: `har-762-other-contact-${timestamp}@example.com`,
        title: 'Other buyer',
      },
    },
    'companyContactCreate',
    'secondary companyContactCreate setup',
  );
  const secondaryContactId = readStringAtPath(
    secondaryContactCreate.response,
    ['data', 'companyContactCreate', 'companyContact', 'id'],
    'secondary companyContactCreate setup',
  );

  const contactAssignmentLocationCreate = await runRequired(
    locationCreateDocument,
    {
      companyId,
      input: { name: `${companyName} contact assignment location`, phone: '+16135550763' },
    },
    'companyLocationCreate',
    'contact assignment location setup',
  );
  const contactAssignmentLocationId = readStringAtPath(
    contactAssignmentLocationCreate.response,
    ['data', 'companyLocationCreate', 'companyLocation', 'id'],
    'contact assignment location setup',
  );

  const contactAssignmentCreate = await runRequired(
    contactAssignRolesDocument,
    {
      companyContactId: secondaryContactId,
      rolesToAssign: [{ companyLocationId: contactAssignmentLocationId, companyContactRoleId: locationAdminRoleId }],
    },
    'companyContactAssignRoles',
    'secondary contact role assignment setup',
  );
  const secondaryContactAssignmentId = readStringAtPath(
    contactAssignmentCreate.response,
    ['data', 'companyContactAssignRoles', 'roleAssignments', '0', 'id'],
    'secondary contact role assignment setup',
  );

  const locationAssignmentLocationCreate = await runRequired(
    locationCreateDocument,
    {
      companyId,
      input: { name: `${companyName} location assignment location`, phone: '+16135550764' },
    },
    'companyLocationCreate',
    'location assignment location setup',
  );
  const locationAssignmentLocationId = readStringAtPath(
    locationAssignmentLocationCreate.response,
    ['data', 'companyLocationCreate', 'companyLocation', 'id'],
    'location assignment location setup',
  );

  const locationAssignmentCreate = await runRequired(
    locationAssignRolesDocument,
    {
      companyLocationId: locationAssignmentLocationId,
      rolesToAssign: [{ companyContactId: mainContactId, companyContactRoleId: orderingOnlyRoleId }],
    },
    'companyLocationAssignRoles',
    'secondary location role assignment setup',
  );
  const secondaryLocationAssignmentId = readStringAtPath(
    locationAssignmentCreate.response,
    ['data', 'companyLocationAssignRoles', 'roleAssignments', '0', 'id'],
    'secondary location role assignment setup',
  );

  const contactRevokeRoleWrongContact = await runValidation(
    contactRevokeRoleDocument,
    { companyContactId: mainContactId, companyContactRoleAssignmentId: secondaryContactAssignmentId },
    'companyContactRevokeRole',
    {
      field: ['companyContactRoleAssignmentId'],
      code: 'RESOURCE_NOT_FOUND',
    },
    'companyContactRevokeRole wrong-contact assignment validation',
  );
  assertPathEquals(
    contactRevokeRoleWrongContact.response,
    ['data', 'companyContactRevokeRole', 'revokedCompanyContactRoleAssignmentId'],
    null,
    'companyContactRevokeRole wrong-contact assignment validation',
  );

  const contactRevokeRolesRequiresIds = await runValidation(
    contactRevokeRolesDocument,
    { companyContactId: mainContactId, roleAssignmentIds: [], revokeAll: false },
    'companyContactRevokeRoles',
    { field: null, code: 'INVALID_INPUT' },
    'companyContactRevokeRoles requires ids or revokeAll validation',
  );
  assertPathEquals(
    contactRevokeRolesRequiresIds.response,
    ['data', 'companyContactRevokeRoles', 'revokedRoleAssignmentIds'],
    null,
    'companyContactRevokeRoles requires ids or revokeAll validation',
  );

  const locationRevokeRolesWrongLocation = await runValidation(
    locationRevokeRolesDocument,
    { companyLocationId: mainLocationId, rolesToRevoke: [secondaryLocationAssignmentId] },
    'companyLocationRevokeRoles',
    { field: ['rolesToRevoke', '0'], code: 'RESOURCE_NOT_FOUND' },
    'companyLocationRevokeRoles wrong-location assignment validation',
  );
  assertPathEquals(
    locationRevokeRolesWrongLocation.response,
    ['data', 'companyLocationRevokeRoles', 'revokedRoleAssignmentIds'],
    [],
    'companyLocationRevokeRoles wrong-location assignment validation',
  );

  const contactRevokeRolesWrongContactPartial = await runValidation(
    contactRevokeRolesDocument,
    {
      companyContactId: mainContactId,
      roleAssignmentIds: [mainAssignmentId, secondaryContactAssignmentId],
      revokeAll: false,
    },
    'companyContactRevokeRoles',
    { field: ['roleAssignmentIds', '1'], code: 'RESOURCE_NOT_FOUND' },
    'companyContactRevokeRoles wrong-contact partial validation',
  );
  assertPathEquals(
    contactRevokeRolesWrongContactPartial.response,
    ['data', 'companyContactRevokeRoles', 'revokedRoleAssignmentIds'],
    [mainAssignmentId],
    'companyContactRevokeRoles wrong-contact partial validation',
  );

  const companyDelete = await runRequired(companyDeleteDocument, { id: companyId }, 'companyDelete', 'company cleanup');
  companyDeleted = true;

  const output = {
    scenarioId,
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    intent: {
      ticket: 'HAR-762',
      plan: 'Create disposable B2B company/contact/location role assignments, capture Shopify revoke-role parent/scope preconditions, and delete the company.',
    },
    companyCreate,
    secondaryContactCreate,
    contactAssignmentLocationCreate,
    contactAssignmentCreate,
    locationAssignmentLocationCreate,
    locationAssignmentCreate,
    contactRevokeRoleWrongContact,
    contactRevokeRolesRequiresIds,
    locationRevokeRolesWrongLocation,
    contactRevokeRolesWrongContactPartial,
    companyDelete,
    cleanup,
    upstreamCalls: [],
  };

  readArrayAtPath(
    output,
    ['contactRevokeRoleWrongContact', 'response', 'data', 'companyContactRevokeRole', 'userErrors'],
    'contactRevokeRoleWrongContact',
  );
  readArrayAtPath(
    output,
    ['contactRevokeRolesWrongContactPartial', 'response', 'data', 'companyContactRevokeRoles', 'userErrors'],
    'contactRevokeRolesWrongContactPartial',
  );

  const outputPath = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'b2b', `${scenarioId}.json`);
  await mkdir(path.dirname(outputPath), { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');

  // oxlint-disable-next-line no-console -- capture scripts report their output path.
  console.log(JSON.stringify({ ok: true, outputPath }, null, 2));
} finally {
  if (companyId && !companyDeleted) {
    cleanup['companyDelete'] = await runCleanup(companyDeleteDocument, { id: companyId }, 'companyDelete', 'cleanup');
  }
}

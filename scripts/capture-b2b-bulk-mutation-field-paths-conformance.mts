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

const scenarioId = 'b2b-bulk-mutation-field-paths';
const timestamp = Date.now();
const companyName = `HAR-754 B2B bulk fields ${timestamp}`;
const missingCompanyId = 'gid://shopify/Company/999999999999';
const missingContactId = 'gid://shopify/CompanyContact/999999999999';
const missingLocationId = 'gid://shopify/CompanyLocation/999999999999';
const missingRoleId = 'gid://shopify/CompanyContactRole/999999999999';
const missingAssignmentId = 'gid://shopify/CompanyContactRoleAssignment/999999999999';
const missingStaffMemberId = 'gid://shopify/StaffMember/999999999999';
const missingStaffAssignmentId = 'gid://shopify/CompanyLocationStaffMemberAssignment/999999999999';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const companyCreateDocument = `#graphql
  mutation HAR754CompanyCreate($input: CompanyCreateInput!) {
    companyCreate(input: $input) {
      company {
        id
        name
        mainContact {
          id
          title
          isMainContact
          roleAssignments(first: 5) { nodes { id role { id name } companyLocation { id name } } }
        }
        contacts(first: 5) { nodes { id title isMainContact } }
        locations(first: 5) { nodes { id name } }
        contactRoles(first: 5) { nodes { id name } }
      }
      userErrors { field message code }
    }
  }
`;

const contactCreateDocument = `#graphql
  mutation HAR754ContactCreate($companyId: ID!, $input: CompanyContactInput!) {
    companyContactCreate(companyId: $companyId, input: $input) {
      companyContact { id title company { id } }
      userErrors { field message code }
    }
  }
`;

const locationCreateDocument = `#graphql
  mutation HAR754LocationCreate($companyId: ID!, $input: CompanyLocationInput!) {
    companyLocationCreate(companyId: $companyId, input: $input) {
      companyLocation { id name company { id } }
      userErrors { field message code }
    }
  }
`;

const companiesDeleteDocument = `#graphql
  mutation HAR754CompaniesDelete($companyIds: [ID!]!) {
    companiesDelete(companyIds: $companyIds) {
      deletedCompanyIds
      userErrors { field message code }
    }
  }
`;

const contactsDeleteDocument = `#graphql
  mutation HAR754ContactsDelete($companyContactIds: [ID!]!) {
    companyContactsDelete(companyContactIds: $companyContactIds) {
      deletedCompanyContactIds
      userErrors { field message code }
    }
  }
`;

const locationsDeleteDocument = `#graphql
  mutation HAR754LocationsDelete($companyLocationIds: [ID!]!) {
    companyLocationsDelete(companyLocationIds: $companyLocationIds) {
      deletedCompanyLocationIds
      userErrors { field message code }
    }
  }
`;

const contactAssignRolesDocument = `#graphql
  mutation HAR754ContactAssignRoles($companyContactId: ID!, $rolesToAssign: [CompanyContactRoleAssign!]!) {
    companyContactAssignRoles(companyContactId: $companyContactId, rolesToAssign: $rolesToAssign) {
      roleAssignments { id }
      userErrors { field message code }
    }
  }
`;

const locationAssignRolesDocument = `#graphql
  mutation HAR754LocationAssignRoles($companyLocationId: ID!, $rolesToAssign: [CompanyLocationRoleAssign!]!) {
    companyLocationAssignRoles(companyLocationId: $companyLocationId, rolesToAssign: $rolesToAssign) {
      roleAssignments { id }
      userErrors { field message code }
    }
  }
`;

const contactRevokeRolesDocument = `#graphql
  mutation HAR754ContactRevokeRoles($companyContactId: ID!, $roleAssignmentIds: [ID!]!) {
    companyContactRevokeRoles(companyContactId: $companyContactId, roleAssignmentIds: $roleAssignmentIds) {
      revokedRoleAssignmentIds
      userErrors { field message code }
    }
  }
`;

const locationRevokeRolesDocument = `#graphql
  mutation HAR754LocationRevokeRoles($companyLocationId: ID!, $rolesToRevoke: [ID!]!) {
    companyLocationRevokeRoles(companyLocationId: $companyLocationId, rolesToRevoke: $rolesToRevoke) {
      revokedRoleAssignmentIds
      userErrors { field message code }
    }
  }
`;

const staffAssignDocument = `#graphql
  mutation HAR754AssignStaff($companyLocationId: ID!, $staffMemberIds: [ID!]!) {
    companyLocationAssignStaffMembers(companyLocationId: $companyLocationId, staffMemberIds: $staffMemberIds) {
      companyLocationStaffMemberAssignments { id }
      userErrors { field message code }
    }
  }
`;

const staffRemoveDocument = `#graphql
  mutation HAR754RemoveStaff($companyLocationStaffMemberAssignmentIds: [ID!]!) {
    companyLocationRemoveStaffMembers(
      companyLocationStaffMemberAssignmentIds: $companyLocationStaffMemberAssignmentIds
    ) {
      deletedCompanyLocationStaffMemberAssignmentIds
      userErrors { field message code }
    }
  }
`;

const companyDeleteDocument = `#graphql
  mutation HAR754CompanyDelete($id: ID!) {
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

function assertUserErrorFields(
  result: ConformanceGraphqlResult,
  root: string,
  expectedFields: string[][],
  label: string,
): void {
  assertNoTopLevelErrors(result, label);
  const actualFields = readUserErrors(result.payload, root).map((error) => error['field']);
  for (const expected of expectedFields) {
    const found = actualFields.some((field) => JSON.stringify(field) === JSON.stringify(expected));
    if (!found) {
      throw new Error(
        `${label} did not include field ${JSON.stringify(expected)}: ${JSON.stringify(result.payload, null, 2)}`,
      );
    }
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

async function runFieldValidation(
  query: string,
  variables: JsonRecord,
  root: string,
  expectedFields: string[][],
  label: string,
): Promise<RecordedOperation> {
  const result = await runGraphqlRequest(query, variables);
  assertUserErrorFields(result, root, expectedFields, label);
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

function companyInput(name: string, emailPrefix: string): JsonRecord {
  return {
    company: {
      name,
      note: 'HAR-754 B2B bulk mutation field path parity',
      externalId: `${emailPrefix}-${timestamp}`,
    },
    companyContact: {
      firstName: 'Har',
      lastName: 'Bulk',
      email: `${emailPrefix}-${timestamp}@example.com`,
      title: 'Buyer',
    },
    companyLocation: {
      name: `${name} HQ`,
      phone: '+16135550101',
      billingAddress: {
        address1: '754 Bulk Way',
        city: 'Toronto',
        countryCode: 'CA',
      },
    },
  };
}

let primaryCompanyId: string | null = null;
let primaryCompanyDeleted = false;
const cleanup: Record<string, RecordedOperation> = {};

try {
  const primaryCompanyCreate = await runRequired(
    companyCreateDocument,
    { input: companyInput(companyName, 'har-754-primary') },
    'companyCreate',
    'primary companyCreate setup',
  );
  primaryCompanyId = readStringAtPath(
    primaryCompanyCreate.response,
    ['data', 'companyCreate', 'company', 'id'],
    'primary companyCreate setup',
  );
  const contactId = readStringAtPath(
    primaryCompanyCreate.response,
    ['data', 'companyCreate', 'company', 'mainContact', 'id'],
    'primary companyCreate main contact',
  );
  const mainLocationId = readStringAtPath(
    primaryCompanyCreate.response,
    ['data', 'companyCreate', 'company', 'locations', 'nodes', '0', 'id'],
    'primary companyCreate main location',
  );
  const locationAdminRoleId = readStringAtPath(
    primaryCompanyCreate.response,
    ['data', 'companyCreate', 'company', 'contactRoles', 'nodes', '0', 'id'],
    'primary companyCreate first contact role',
  );
  const orderingOnlyRoleId = readStringAtPath(
    primaryCompanyCreate.response,
    ['data', 'companyCreate', 'company', 'contactRoles', 'nodes', '1', 'id'],
    'primary companyCreate second contact role',
  );

  const secondCompanyCreate = await runRequired(
    companyCreateDocument,
    { input: companyInput(`${companyName} delete`, 'har-754-delete-company') },
    'companyCreate',
    'secondary companyCreate setup for companiesDelete',
  );
  const secondCompanyId = readStringAtPath(
    secondCompanyCreate.response,
    ['data', 'companyCreate', 'company', 'id'],
    'secondary companyCreate setup for companiesDelete',
  );
  const companiesDelete = await runFieldValidation(
    companiesDeleteDocument,
    { companyIds: [secondCompanyId, missingCompanyId] },
    'companiesDelete',
    [['companyIds', '1']],
    'companiesDelete indexed field path validation',
  );

  const contactCreate = await runRequired(
    contactCreateDocument,
    {
      companyId: primaryCompanyId,
      input: {
        firstName: 'Har',
        lastName: 'Deleted',
        email: `har-754-delete-contact-${timestamp}@example.com`,
        title: 'Delete candidate',
      },
    },
    'companyContactCreate',
    'companyContactCreate setup for companyContactsDelete',
  );
  const contactToDeleteId = readStringAtPath(
    contactCreate.response,
    ['data', 'companyContactCreate', 'companyContact', 'id'],
    'companyContactCreate setup for companyContactsDelete',
  );
  const contactsDelete = await runFieldValidation(
    contactsDeleteDocument,
    { companyContactIds: [contactToDeleteId, missingContactId] },
    'companyContactsDelete',
    [['companyContactIds', '1']],
    'companyContactsDelete indexed field path validation',
  );

  const locationCreateForDelete = await runRequired(
    locationCreateDocument,
    {
      companyId: primaryCompanyId,
      input: { name: `${companyName} delete location`, phone: '+16135550102' },
    },
    'companyLocationCreate',
    'companyLocationCreate setup for companyLocationsDelete',
  );
  const locationToDeleteId = readStringAtPath(
    locationCreateForDelete.response,
    ['data', 'companyLocationCreate', 'companyLocation', 'id'],
    'companyLocationCreate setup for companyLocationsDelete',
  );
  const locationsDelete = await runFieldValidation(
    locationsDeleteDocument,
    { companyLocationIds: [locationToDeleteId, missingLocationId] },
    'companyLocationsDelete',
    [['companyLocationIds', '1']],
    'companyLocationsDelete indexed field path validation',
  );

  const contactAssignRoles = await runFieldValidation(
    contactAssignRolesDocument,
    {
      companyContactId: contactId,
      rolesToAssign: [
        { companyLocationId: missingLocationId, companyContactRoleId: locationAdminRoleId },
        { companyLocationId: mainLocationId, companyContactRoleId: missingRoleId },
        { companyLocationId: missingLocationId, companyContactRoleId: missingRoleId },
      ],
    },
    'companyContactAssignRoles',
    [
      ['rolesToAssign', '0', 'companyLocationId'],
      ['rolesToAssign', '1', 'companyContactRoleId'],
      ['rolesToAssign', '2', 'companyLocationId'],
    ],
    'companyContactAssignRoles indexed field path validation',
  );

  const locationAssignRoles = await runFieldValidation(
    locationAssignRolesDocument,
    {
      companyLocationId: mainLocationId,
      rolesToAssign: [
        { companyContactId: missingContactId, companyContactRoleId: orderingOnlyRoleId },
        { companyContactId: contactId, companyContactRoleId: missingRoleId },
        { companyContactId: missingContactId, companyContactRoleId: missingRoleId },
      ],
    },
    'companyLocationAssignRoles',
    [
      ['rolesToAssign', '0'],
      ['rolesToAssign', '1'],
      ['rolesToAssign', '2'],
    ],
    'companyLocationAssignRoles indexed field path validation',
  );

  const contactRevokeLocationCreate = await runRequired(
    locationCreateDocument,
    {
      companyId: primaryCompanyId,
      input: { name: `${companyName} contact revoke location`, phone: '+16135550103' },
    },
    'companyLocationCreate',
    'companyLocationCreate setup for companyContactRevokeRoles',
  );
  const contactRevokeLocationId = readStringAtPath(
    contactRevokeLocationCreate.response,
    ['data', 'companyLocationCreate', 'companyLocation', 'id'],
    'companyLocationCreate setup for companyContactRevokeRoles',
  );
  const contactRevokeAssignmentCreate = await runRequired(
    contactAssignRolesDocument,
    {
      companyContactId: contactId,
      rolesToAssign: [{ companyLocationId: contactRevokeLocationId, companyContactRoleId: locationAdminRoleId }],
    },
    'companyContactAssignRoles',
    'companyContactAssignRoles setup for revoke',
  );
  const contactRevokeAssignmentId = readStringAtPath(
    contactRevokeAssignmentCreate.response,
    ['data', 'companyContactAssignRoles', 'roleAssignments', '0', 'id'],
    'companyContactAssignRoles setup for revoke',
  );
  const contactRevokeRoles = await runFieldValidation(
    contactRevokeRolesDocument,
    { companyContactId: contactId, roleAssignmentIds: [contactRevokeAssignmentId, missingAssignmentId] },
    'companyContactRevokeRoles',
    [['roleAssignmentIds', '1']],
    'companyContactRevokeRoles indexed field path validation',
  );

  const locationRevokeLocationCreate = await runRequired(
    locationCreateDocument,
    {
      companyId: primaryCompanyId,
      input: { name: `${companyName} location revoke location`, phone: '+16135550104' },
    },
    'companyLocationCreate',
    'companyLocationCreate setup for companyLocationRevokeRoles',
  );
  const locationRevokeLocationId = readStringAtPath(
    locationRevokeLocationCreate.response,
    ['data', 'companyLocationCreate', 'companyLocation', 'id'],
    'companyLocationCreate setup for companyLocationRevokeRoles',
  );
  const locationRevokeAssignmentCreate = await runRequired(
    locationAssignRolesDocument,
    {
      companyLocationId: locationRevokeLocationId,
      rolesToAssign: [{ companyContactId: contactId, companyContactRoleId: orderingOnlyRoleId }],
    },
    'companyLocationAssignRoles',
    'companyLocationAssignRoles setup for revoke',
  );
  const locationRevokeAssignmentId = readStringAtPath(
    locationRevokeAssignmentCreate.response,
    ['data', 'companyLocationAssignRoles', 'roleAssignments', '0', 'id'],
    'companyLocationAssignRoles setup for revoke',
  );
  const locationRevokeRoles = await runFieldValidation(
    locationRevokeRolesDocument,
    { companyLocationId: locationRevokeLocationId, rolesToRevoke: [locationRevokeAssignmentId, missingAssignmentId] },
    'companyLocationRevokeRoles',
    [['rolesToRevoke', '1']],
    'companyLocationRevokeRoles indexed field path validation',
  );

  const staffAssign = await runFieldValidation(
    staffAssignDocument,
    { companyLocationId: mainLocationId, staffMemberIds: [missingStaffMemberId] },
    'companyLocationAssignStaffMembers',
    [['staffMemberIds', '0']],
    'companyLocationAssignStaffMembers indexed field path validation',
  );

  const staffRemove = await runFieldValidation(
    staffRemoveDocument,
    { companyLocationStaffMemberAssignmentIds: [missingStaffAssignmentId] },
    'companyLocationRemoveStaffMembers',
    [['companyLocationStaffMemberAssignmentIds', '0']],
    'companyLocationRemoveStaffMembers indexed field path validation',
  );

  const companyDelete = await runRequired(
    companyDeleteDocument,
    { id: primaryCompanyId },
    'companyDelete',
    'companyDelete cleanup',
  );
  primaryCompanyDeleted = true;

  const output = {
    scenarioId,
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    intent: {
      ticket: 'HAR-754',
      plan: 'Create disposable B2B records, record Shopify userErrors[].field paths for bulk B2B list inputs, and clean up created companies.',
    },
    primaryCompanyCreate,
    secondCompanyCreate,
    companiesDelete,
    contactCreate,
    contactsDelete,
    locationCreateForDelete,
    locationsDelete,
    contactAssignRoles,
    locationAssignRoles,
    contactRevokeLocationCreate,
    contactRevokeAssignmentCreate,
    contactRevokeRoles,
    locationRevokeLocationCreate,
    locationRevokeAssignmentCreate,
    locationRevokeRoles,
    staffAssign,
    staffRemove,
    companyDelete,
    cleanup,
    upstreamCalls: [],
  };

  // Ensure the shape is useful to future maintainers inspecting the fixture.
  readArrayAtPath(output, ['companiesDelete', 'response', 'data', 'companiesDelete', 'userErrors'], 'companiesDelete');
  readArrayAtPath(
    output,
    ['contactAssignRoles', 'response', 'data', 'companyContactAssignRoles', 'userErrors'],
    'contactAssignRoles',
  );

  const outputPath = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'b2b', `${scenarioId}.json`);
  await mkdir(path.dirname(outputPath), { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');

  // oxlint-disable-next-line no-console -- capture scripts report their output path.
  console.log(JSON.stringify({ ok: true, outputPath }, null, 2));
} finally {
  if (primaryCompanyId && !primaryCompanyDeleted) {
    cleanup['companyDelete'] = await runCleanup(
      companyDeleteDocument,
      { id: primaryCompanyId },
      'companyDelete',
      'companyDelete finally cleanup',
    );
  }
}

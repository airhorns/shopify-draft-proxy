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

const scenarioId = 'b2b-contact-location-assignments-tax';
const timestamp = Date.now();
const companyName = `HAR-446 B2B assignment ${timestamp}`;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const companyCreateDocument = `#graphql
  mutation B2BContactLocationAssignmentsCreate($input: CompanyCreateInput!) {
    companyCreate(input: $input) {
      company {
        id
        name
        mainContact {
          id
          title
          isMainContact
          roleAssignments(first: 5) {
            nodes {
              id
              role { id name }
              companyLocation { id name }
            }
          }
        }
        contacts(first: 5) {
          nodes {
            id
            title
            isMainContact
            roleAssignments(first: 5) {
              nodes {
                id
                role { id name }
                companyLocation { id name }
              }
            }
          }
        }
        locations(first: 5) { nodes { id name } }
        contactRoles(first: 5) { nodes { id name } }
      }
      userErrors { field message code }
    }
  }
`;

const locationCreateDocument = `#graphql
  mutation B2BContactLocationAssignmentsLocationCreate($companyId: ID!, $input: CompanyLocationInput!) {
    companyLocationCreate(companyId: $companyId, input: $input) {
      companyLocation { id name company { id name } }
      userErrors { field message code }
    }
  }
`;

const contactAssignRoleDocument = `#graphql
  mutation B2BContactLocationAssignmentsAssignRole(
    $companyContactId: ID!
    $companyContactRoleId: ID!
    $companyLocationId: ID!
  ) {
    companyContactAssignRole(
      companyContactId: $companyContactId
      companyContactRoleId: $companyContactRoleId
      companyLocationId: $companyLocationId
    ) {
      companyContactRoleAssignment {
        id
        companyContact { id title }
        role { id name }
        companyLocation { id name }
      }
      userErrors { field message code }
    }
  }
`;

const contactAssignRolesDocument = `#graphql
  mutation B2BContactLocationAssignmentsAssignRoles(
    $companyContactId: ID!
    $rolesToAssign: [CompanyContactRoleAssign!]!
  ) {
    companyContactAssignRoles(companyContactId: $companyContactId, rolesToAssign: $rolesToAssign) {
      roleAssignments {
        id
        companyContact { id title }
        role { id name }
        companyLocation { id name }
      }
      userErrors { field message code }
    }
  }
`;

const locationAssignRolesDocument = `#graphql
  mutation B2BContactLocationAssignmentsLocationAssignRoles(
    $companyLocationId: ID!
    $rolesToAssign: [CompanyLocationRoleAssign!]!
  ) {
    companyLocationAssignRoles(companyLocationId: $companyLocationId, rolesToAssign: $rolesToAssign) {
      roleAssignments {
        id
        companyContact { id title }
        role { id name }
        companyLocation { id name }
      }
      userErrors { field message code }
    }
  }
`;

const addressAssignDocument = `#graphql
  mutation B2BContactLocationAssignmentsAddress(
    $locationId: ID!
    $address: CompanyAddressInput!
    $addressTypes: [CompanyAddressType!]!
  ) {
    companyLocationAssignAddress(locationId: $locationId, address: $address, addressTypes: $addressTypes) {
      addresses { id address1 city countryCode }
      userErrors { field message code }
    }
  }
`;

const taxUpdateDocument = `#graphql
  mutation B2BContactLocationAssignmentsTax(
    $companyLocationId: ID!
    $taxRegistrationId: String
    $taxExempt: Boolean
    $exemptionsToAssign: [TaxExemption!]
    $exemptionsToRemove: [TaxExemption!]
  ) {
    companyLocationTaxSettingsUpdate(
      companyLocationId: $companyLocationId
      taxRegistrationId: $taxRegistrationId
      taxExempt: $taxExempt
      exemptionsToAssign: $exemptionsToAssign
      exemptionsToRemove: $exemptionsToRemove
    ) {
      companyLocation { id taxSettings { taxRegistrationId taxExempt taxExemptions } }
      userErrors { field message code }
    }
  }
`;

const contactUpdateDocument = `#graphql
  mutation B2BContactLocationAssignmentsContactUpdate($companyContactId: ID!, $input: CompanyContactInput!) {
    companyContactUpdate(companyContactId: $companyContactId, input: $input) {
      companyContact { id title }
      userErrors { field message code }
    }
  }
`;

const locationUpdateDocument = `#graphql
  mutation B2BContactLocationAssignmentsLocationUpdate($companyLocationId: ID!, $input: CompanyLocationUpdateInput!) {
    companyLocationUpdate(companyLocationId: $companyLocationId, input: $input) {
      companyLocation { id name }
      userErrors { field message code }
    }
  }
`;

const readAfterAssignmentsDocument = `#graphql
  query B2BContactLocationAssignmentsRead(
    $companyContactId: ID!
    $companyLocationId: ID!
    $singleAssignmentLocationId: ID!
    $contactBulkLocationId: ID!
    $locationBulkLocationId: ID!
  ) {
    companyContact(id: $companyContactId) {
      id
      title
      roleAssignments(first: 5) {
        nodes {
          id
          companyContact { id title }
          role { id name }
          companyLocation { id name }
        }
      }
    }
    companyLocation(id: $companyLocationId) {
      id
      name
      billingAddress { id address1 city countryCode }
      taxSettings { taxRegistrationId taxExempt taxExemptions }
      roleAssignments(first: 5) {
        nodes {
          id
          companyContact { id title }
          role { id name }
          companyLocation { id name }
        }
      }
    }
    singleAssignmentLocation: companyLocation(id: $singleAssignmentLocationId) {
      id
      name
      roleAssignments(first: 5) {
        nodes {
          id
          companyContact { id title }
          role { id name }
          companyLocation { id name }
        }
      }
    }
    contactBulkLocation: companyLocation(id: $contactBulkLocationId) {
      id
      name
      roleAssignments(first: 5) {
        nodes {
          id
          companyContact { id title }
          role { id name }
          companyLocation { id name }
        }
      }
    }
    locationBulkLocation: companyLocation(id: $locationBulkLocationId) {
      id
      name
      roleAssignments(first: 5) {
        nodes {
          id
          companyContact { id title }
          role { id name }
          companyLocation { id name }
        }
      }
    }
  }
`;

const contactRevokeRoleDocument = `#graphql
  mutation B2BContactLocationAssignmentsRevokeRole(
    $companyContactId: ID!
    $companyContactRoleAssignmentId: ID!
  ) {
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
  mutation B2BContactLocationAssignmentsContactRevokeRoles(
    $companyContactId: ID!
    $roleAssignmentIds: [ID!]!
  ) {
    companyContactRevokeRoles(companyContactId: $companyContactId, roleAssignmentIds: $roleAssignmentIds) {
      revokedRoleAssignmentIds
      userErrors { field message code }
    }
  }
`;

const locationRevokeRolesDocument = `#graphql
  mutation B2BContactLocationAssignmentsLocationRevokeRoles(
    $companyLocationId: ID!
    $rolesToRevoke: [ID!]!
  ) {
    companyLocationRevokeRoles(companyLocationId: $companyLocationId, rolesToRevoke: $rolesToRevoke) {
      revokedRoleAssignmentIds
      userErrors { field message code }
    }
  }
`;

const addressDeleteDocument = `#graphql
  mutation B2BContactLocationAssignmentsAddressDelete($addressId: ID!) {
    companyAddressDelete(addressId: $addressId) {
      deletedAddressId
      userErrors { field message code }
    }
  }
`;

const readAfterRevokeDocument = `#graphql
  query B2BContactLocationAssignmentsReadAfterRevoke($companyContactId: ID!, $companyLocationId: ID!) {
    companyContact(id: $companyContactId) {
      id
      roleAssignments(first: 5) { nodes { id } }
    }
    companyLocation(id: $companyLocationId) {
      id
      billingAddress { id }
      roleAssignments(first: 5) { nodes { id } }
    }
  }
`;

const companyDeleteDocument = `#graphql
  mutation B2BContactLocationAssignmentsCompanyDelete($id: ID!) {
    companyDelete(id: $id) {
      deletedCompanyId
      userErrors { field message code }
    }
  }
`;

const staffAccessProbeDocument = `#graphql
  query B2BContactLocationAssignmentsStaffProbe {
    staffMembers(first: 1) { nodes { id name } }
  }
`;

function readRecord(value: unknown): JsonRecord | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readPath(value: unknown, pathSegments: string[]): unknown {
  let current = value;
  for (const segment of pathSegments) {
    if (Array.isArray(current)) {
      const index = Number(segment);
      if (!Number.isInteger(index) || index < 0) {
        return undefined;
      }
      current = current[index];
      continue;
    }
    const record = readRecord(current);
    if (!record) {
      return undefined;
    }
    current = record[segment];
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

function readUserErrors(payload: unknown, root: string): unknown[] {
  const value = readPath(payload, ['data', root, 'userErrors']);
  return Array.isArray(value) ? value : [];
}

function assertSuccessful(result: ConformanceGraphqlResult, root: string, label: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} returned top-level errors: ${JSON.stringify(result, null, 2)}`);
  }
  const userErrors = readUserErrors(result.payload, root);
  if (userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
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

let companyId: string | null = null;
let companyDeleted = false;
const cleanup: Record<string, RecordedOperation> = {};

try {
  const staffAccessProbe = recordOperation(
    staffAccessProbeDocument,
    {},
    await runGraphqlRequest(staffAccessProbeDocument, {}),
  );

  const companyCreateVariables = {
    input: {
      company: {
        name: companyName,
        note: 'HAR-446 B2B assignment parity',
        externalId: `har-446-assignment-${timestamp}`,
      },
      companyContact: {
        firstName: 'Har',
        lastName: 'Assign',
        email: `har-446-assignment-${timestamp}@example.com`,
        title: 'Buyer',
      },
      companyLocation: {
        name: `${companyName} HQ`,
        phone: '+16135550101',
        billingAddress: {
          address1: '1 B2B Way',
          city: 'Ottawa',
          countryCode: 'CA',
        },
      },
    },
  };
  const companyCreate = await runRequired(
    companyCreateDocument,
    companyCreateVariables,
    'companyCreate',
    'companyCreate setup',
  );
  companyId = readStringAtPath(companyCreate.response, ['data', 'companyCreate', 'company', 'id'], 'companyCreate');
  const contactId = readStringAtPath(
    companyCreate.response,
    ['data', 'companyCreate', 'company', 'mainContact', 'id'],
    'companyCreate main contact',
  );
  const mainLocationId = readStringAtPath(
    companyCreate.response,
    ['data', 'companyCreate', 'company', 'locations', 'nodes', '0', 'id'],
    'companyCreate main location',
  );
  const locationAdminRoleId = readStringAtPath(
    companyCreate.response,
    ['data', 'companyCreate', 'company', 'contactRoles', 'nodes', '0', 'id'],
    'companyCreate first role',
  );
  const orderingOnlyRoleId = readStringAtPath(
    companyCreate.response,
    ['data', 'companyCreate', 'company', 'contactRoles', 'nodes', '1', 'id'],
    'companyCreate second role',
  );

  const extraLocationCreate = await runRequired(
    locationCreateDocument,
    {
      companyId,
      input: {
        name: `${companyName} Single assignment`,
        phone: '+16135550102',
      },
    },
    'companyLocationCreate',
    'companyLocationCreate setup',
  );
  const extraLocationId = readStringAtPath(
    extraLocationCreate.response,
    ['data', 'companyLocationCreate', 'companyLocation', 'id'],
    'companyLocationCreate setup',
  );
  const contactBulkLocationCreate = await runRequired(
    locationCreateDocument,
    {
      companyId,
      input: {
        name: `${companyName} Contact bulk`,
        phone: '+16135550103',
      },
    },
    'companyLocationCreate',
    'companyLocationCreate setup for contact bulk assignment',
  );
  const contactBulkLocationId = readStringAtPath(
    contactBulkLocationCreate.response,
    ['data', 'companyLocationCreate', 'companyLocation', 'id'],
    'companyLocationCreate setup for contact bulk assignment',
  );
  const locationBulkLocationCreate = await runRequired(
    locationCreateDocument,
    {
      companyId,
      input: {
        name: `${companyName} Location bulk`,
        phone: '+16135550104',
      },
    },
    'companyLocationCreate',
    'companyLocationCreate setup for location bulk assignment',
  );
  const locationBulkLocationId = readStringAtPath(
    locationBulkLocationCreate.response,
    ['data', 'companyLocationCreate', 'companyLocation', 'id'],
    'companyLocationCreate setup for location bulk assignment',
  );

  const assignSingle = await runRequired(
    contactAssignRoleDocument,
    { companyContactId: contactId, companyContactRoleId: locationAdminRoleId, companyLocationId: extraLocationId },
    'companyContactAssignRole',
    'companyContactAssignRole assignment',
  );
  const singleAssignmentId = readStringAtPath(
    assignSingle.response,
    ['data', 'companyContactAssignRole', 'companyContactRoleAssignment', 'id'],
    'companyContactAssignRole assignment',
  );

  const assignContactRoles = await runRequired(
    contactAssignRolesDocument,
    {
      companyContactId: contactId,
      rolesToAssign: [{ companyContactRoleId: locationAdminRoleId, companyLocationId: contactBulkLocationId }],
    },
    'companyContactAssignRoles',
    'companyContactAssignRoles assignment',
  );
  const contactBulkAssignmentId = readStringAtPath(
    assignContactRoles.response,
    ['data', 'companyContactAssignRoles', 'roleAssignments', '0', 'id'],
    'companyContactAssignRoles assignment',
  );

  const assignLocationRoles = await runRequired(
    locationAssignRolesDocument,
    {
      companyLocationId: locationBulkLocationId,
      rolesToAssign: [{ companyContactId: contactId, companyContactRoleId: orderingOnlyRoleId }],
    },
    'companyLocationAssignRoles',
    'companyLocationAssignRoles assignment',
  );
  const locationBulkAssignmentId = readStringAtPath(
    assignLocationRoles.response,
    ['data', 'companyLocationAssignRoles', 'roleAssignments', '0', 'id'],
    'companyLocationAssignRoles assignment',
  );

  const assignAddress = await runRequired(
    addressAssignDocument,
    {
      locationId: mainLocationId,
      address: {
        address1: '446 Assignment Way',
        city: 'Toronto',
        countryCode: 'CA',
      },
      addressTypes: ['BILLING'],
    },
    'companyLocationAssignAddress',
    'companyLocationAssignAddress assignment',
  );
  const addressId = readStringAtPath(
    assignAddress.response,
    ['data', 'companyLocationAssignAddress', 'addresses', '0', 'id'],
    'companyLocationAssignAddress assignment',
  );

  const taxUpdate = await runRequired(
    taxUpdateDocument,
    {
      companyLocationId: mainLocationId,
      taxRegistrationId: 'HAR446-TAX',
      taxExempt: true,
      exemptionsToAssign: ['CA_STATUS_CARD_EXEMPTION'],
      exemptionsToRemove: [],
    },
    'companyLocationTaxSettingsUpdate',
    'companyLocationTaxSettingsUpdate assignment',
  );

  const contactUpdate = await runRequired(
    contactUpdateDocument,
    { companyContactId: contactId, input: { title: 'Lead buyer' } },
    'companyContactUpdate',
    'companyContactUpdate after assignment',
  );

  const locationUpdate = await runRequired(
    locationUpdateDocument,
    { companyLocationId: extraLocationId, input: { name: `${companyName} Single assignment updated` } },
    'companyLocationUpdate',
    'companyLocationUpdate after assignment',
  );

  const readAfterAssignments = await runRequired(
    readAfterAssignmentsDocument,
    {
      companyContactId: contactId,
      companyLocationId: mainLocationId,
      singleAssignmentLocationId: extraLocationId,
      contactBulkLocationId,
      locationBulkLocationId,
    },
    'companyContact',
    'downstream read after assignment/address/tax updates',
  );

  const revokeSingle = await runRequired(
    contactRevokeRoleDocument,
    { companyContactId: contactId, companyContactRoleAssignmentId: singleAssignmentId },
    'companyContactRevokeRole',
    'companyContactRevokeRole cleanup',
  );

  const revokeContactRoles = await runRequired(
    contactRevokeRolesDocument,
    { companyContactId: contactId, roleAssignmentIds: [contactBulkAssignmentId] },
    'companyContactRevokeRoles',
    'companyContactRevokeRoles cleanup',
  );

  const revokeLocationRoles = await runRequired(
    locationRevokeRolesDocument,
    { companyLocationId: locationBulkLocationId, rolesToRevoke: [locationBulkAssignmentId] },
    'companyLocationRevokeRoles',
    'companyLocationRevokeRoles cleanup',
  );

  const addressDelete = await runRequired(
    addressDeleteDocument,
    { addressId },
    'companyAddressDelete',
    'companyAddressDelete cleanup',
  );

  const readAfterRevoke = await runRequired(
    readAfterRevokeDocument,
    { companyContactId: contactId, companyLocationId: mainLocationId },
    'companyContact',
    'downstream read after revoke/address delete',
  );

  const companyDelete = await runRequired(
    companyDeleteDocument,
    { id: companyId },
    'companyDelete',
    'companyDelete cleanup',
  );
  companyDeleted = true;

  const output = {
    scenarioId,
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    intent: {
      ticket: 'HAR-446',
      plan: 'Create a disposable B2B company, record role assignment, address/tax settings, relationship reads after contact/location updates, revoke/delete cleanup, and delete the company.',
    },
    staffAccessProbe,
    companyCreate,
    extraLocationCreate,
    contactBulkLocationCreate,
    locationBulkLocationCreate,
    assignSingle,
    assignContactRoles,
    assignLocationRoles,
    assignAddress,
    taxUpdate,
    contactUpdate,
    locationUpdate,
    readAfterAssignments,
    revokeSingle,
    revokeContactRoles,
    revokeLocationRoles,
    addressDelete,
    readAfterRevoke,
    companyDelete,
    cleanup,
  };

  const outputPath = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'b2b', `${scenarioId}.json`);
  await mkdir(path.dirname(outputPath), { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');

  // oxlint-disable-next-line no-console -- capture scripts report their output path.
  console.log(JSON.stringify({ ok: true, outputPath }, null, 2));
} finally {
  if (companyId && !companyDeleted) {
    cleanup['companyDelete'] = await runCleanup(
      companyDeleteDocument,
      { id: companyId },
      'companyDelete',
      'companyDelete finally cleanup',
    );
  }
}

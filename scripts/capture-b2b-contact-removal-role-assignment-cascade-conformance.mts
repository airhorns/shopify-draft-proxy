/* oxlint-disable no-console -- CLI capture script intentionally reports progress/output. */
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
type RemovalScenario = {
  key: string;
  scenarioId: string;
  outputName: string;
  ticketPlan: string;
  removalRoot: string;
  removalDocument: string;
  removalVariables: (contactId: string) => JsonRecord;
};

const timestamp = Date.now();
const runKey = `har-758-${timestamp}`;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const companyCreateDocument = `#graphql
  mutation ContactCascadeCompanyCreate($input: CompanyCreateInput!) {
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
        locations(first: 5) { nodes { id name } }
        contactRoles(first: 5) { nodes { id name } }
      }
      userErrors { field message code }
    }
  }
`;

const locationCreateDocument = `#graphql
  mutation ContactCascadeLocationCreate($companyId: ID!, $input: CompanyLocationInput!) {
    companyLocationCreate(companyId: $companyId, input: $input) {
      companyLocation { id name company { id } }
      userErrors { field message code }
    }
  }
`;

const assignRoleDocument = `#graphql
  mutation ContactCascadeAssignRole(
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

const contactDeleteDocument = `#graphql
  mutation ContactDeleteCleansRoleAssignments($companyContactId: ID!) {
    companyContactDelete(companyContactId: $companyContactId) {
      deletedCompanyContactId
      userErrors { field message code }
    }
  }
`;

const contactsDeleteDocument = `#graphql
  mutation ContactsDeleteCleansRoleAssignments($companyContactIds: [ID!]!) {
    companyContactsDelete(companyContactIds: $companyContactIds) {
      deletedCompanyContactIds
      userErrors { field message code }
    }
  }
`;

const contactRemoveFromCompanyDocument = `#graphql
  mutation ContactRemoveFromCompanyCleansRoleAssignments($companyContactId: ID!) {
    companyContactRemoveFromCompany(companyContactId: $companyContactId) {
      removedCompanyContactId
      userErrors { field message code }
    }
  }
`;

const locationsReadDocument = `#graphql
  query ContactCascadeLocationsRead($mainLocationId: ID!, $extraLocationId: ID!) {
    mainLocation: companyLocation(id: $mainLocationId) {
      id
      roleAssignments(first: 5) {
        nodes {
          id
          companyContact { id title }
          role { id name }
          companyLocation { id name }
        }
      }
    }
    extraLocation: companyLocation(id: $extraLocationId) {
      id
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

const companyDeleteDocument = `#graphql
  mutation ContactCascadeCompanyDelete($id: ID!) {
    companyDelete(id: $id) {
      deletedCompanyId
      userErrors { field message code }
    }
  }
`;

const scenarios: RemovalScenario[] = [
  {
    key: 'contactDelete',
    scenarioId: 'b2b-contact-delete-cleans-role-assignments',
    outputName: 'contact-delete-cleans-role-assignments',
    ticketPlan: 'Record companyContactDelete removing location-side role assignments for the deleted contact.',
    removalRoot: 'companyContactDelete',
    removalDocument: contactDeleteDocument,
    removalVariables: (contactId) => ({ companyContactId: contactId }),
  },
  {
    key: 'contactsDelete',
    scenarioId: 'b2b-contacts-delete-cleans-role-assignments',
    outputName: 'contacts-delete-cleans-role-assignments',
    ticketPlan: 'Record companyContactsDelete removing location-side role assignments for the deleted contact.',
    removalRoot: 'companyContactsDelete',
    removalDocument: contactsDeleteDocument,
    removalVariables: (contactId) => ({ companyContactIds: [contactId] }),
  },
  {
    key: 'contactRemoveFromCompany',
    scenarioId: 'b2b-contact-remove-from-company-cleans-role-assignments',
    outputName: 'contact-remove-from-company-cleans-role-assignments',
    ticketPlan:
      'Record companyContactRemoveFromCompany removing location-side role assignments for the removed contact.',
    removalRoot: 'companyContactRemoveFromCompany',
    removalDocument: contactRemoveFromCompanyDocument,
    removalVariables: (contactId) => ({ companyContactId: contactId }),
  },
];

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

function assertHttpGraphqlOk(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} returned top-level errors: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertSuccessful(result: ConformanceGraphqlResult, root: string, label: string): void {
  assertHttpGraphqlOk(result, label);
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

async function runRead(query: string, variables: JsonRecord, label: string): Promise<RecordedOperation> {
  const result = await runGraphqlRequest(query, variables);
  assertHttpGraphqlOk(result, label);
  return recordOperation(query, variables, result);
}

async function runCleanup(query: string, variables: JsonRecord): Promise<RecordedOperation> {
  const result = await runGraphqlRequest(query, variables);
  return recordOperation(query, variables, result);
}

function companyCreateVariables(label: string, externalIdSuffix: string): JsonRecord {
  const name = `HAR-758 ${label} ${timestamp}`;
  return {
    input: {
      company: {
        name,
        note: `HAR-758 contact-removal role-assignment cascade ${label}`,
        externalId: `${runKey}-${externalIdSuffix}`,
      },
      companyContact: {
        firstName: 'Har',
        lastName: `Cascade ${label}`,
        email: `${runKey}-${label}@example.com`,
        title: 'Buyer',
      },
      companyLocation: {
        name: `${name} HQ`,
        phone: '+16135550758',
        billingAddress: {
          address1: '758 B2B Way',
          city: 'Ottawa',
          countryCode: 'CA',
        },
      },
    },
  };
}

function assertLocationAssignmentsCleared(operation: RecordedOperation, label: string): void {
  for (const alias of ['mainLocation', 'extraLocation']) {
    const nodes = readPath(operation.response, ['data', alias, 'roleAssignments', 'nodes']);
    if (!Array.isArray(nodes) || nodes.length !== 0) {
      throw new Error(`${label} left ${alias} role assignments: ${JSON.stringify(operation.response, null, 2)}`);
    }
  }
}

async function captureScenario(scenario: RemovalScenario): Promise<string> {
  let companyId: string | null = null;
  let companyDeleted = false;
  const cleanup: Record<string, RecordedOperation> = {};
  const label = scenario.outputName;

  try {
    const companyCreate = await runRequired(
      companyCreateDocument,
      companyCreateVariables(label, scenario.key),
      'companyCreate',
      `${label} companyCreate setup`,
    );
    companyId = readStringAtPath(companyCreate.response, ['data', 'companyCreate', 'company', 'id'], 'company id');
    const contactId = readStringAtPath(
      companyCreate.response,
      ['data', 'companyCreate', 'company', 'mainContact', 'id'],
      'main contact id',
    );
    const mainLocationId = readStringAtPath(
      companyCreate.response,
      ['data', 'companyCreate', 'company', 'locations', 'nodes', '0', 'id'],
      'main location id',
    );
    const roleId = readStringAtPath(
      companyCreate.response,
      ['data', 'companyCreate', 'company', 'contactRoles', 'nodes', '0', 'id'],
      'role id',
    );

    const extraLocationCreate = await runRequired(
      locationCreateDocument,
      {
        companyId,
        input: {
          name: `HAR-758 ${label} Branch ${timestamp}`,
          phone: '+16135550759',
        },
      },
      'companyLocationCreate',
      `${label} extra companyLocationCreate setup`,
    );
    const extraLocationId = readStringAtPath(
      extraLocationCreate.response,
      ['data', 'companyLocationCreate', 'companyLocation', 'id'],
      'extra location id',
    );

    const assignExtraLocationRole = await runRequired(
      assignRoleDocument,
      { companyContactId: contactId, companyContactRoleId: roleId, companyLocationId: extraLocationId },
      'companyContactAssignRole',
      `${label} extra location role assignment setup`,
    );

    const contactRemoval = await runRequired(
      scenario.removalDocument,
      scenario.removalVariables(contactId),
      scenario.removalRoot,
      `${label} contact removal`,
    );

    const readAfterRemoval = await runRead(
      locationsReadDocument,
      { mainLocationId, extraLocationId },
      `${label} read after contact removal`,
    );
    assertLocationAssignmentsCleared(readAfterRemoval, label);

    cleanup['companyDelete'] = await runCleanup(companyDeleteDocument, { id: companyId });
    companyDeleted = true;

    const output = {
      scenarioId: scenario.scenarioId,
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      intent: {
        ticket: 'HAR-758',
        plan: scenario.ticketPlan,
      },
      companyCreate,
      extraLocationCreate,
      assignExtraLocationRole,
      contactRemoval,
      readAfterRemoval,
      cleanup,
      upstreamCalls: [],
    };

    const outputPath = path.join(
      'fixtures',
      'conformance',
      storeDomain,
      apiVersion,
      'b2b',
      `${scenario.outputName}.json`,
    );
    await mkdir(path.dirname(outputPath), { recursive: true });
    await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');
    return outputPath;
  } finally {
    if (companyId && !companyDeleted) {
      cleanup['companyDelete'] = await runCleanup(companyDeleteDocument, { id: companyId });
    }
  }
}

const outputPaths: string[] = [];
for (const scenario of scenarios) {
  outputPaths.push(await captureScenario(scenario));
}

console.log(JSON.stringify({ ok: true, outputPaths }, null, 2));

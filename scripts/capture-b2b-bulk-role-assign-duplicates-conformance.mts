import 'dotenv/config';

import {
  createConformanceCapture,
  readArray,
  readRecord,
  requireString,
  type JsonRecord,
} from './conformance-capture-lib.js';
import type { ConformanceGraphqlResult } from './conformance-graphql-client.js';

type RecordedOperation = {
  request: {
    query: string;
    variables: JsonRecord;
  };
  response: JsonRecord;
};

const scenarioId = 'b2b-bulk-role-assign-duplicates';
const missingTransportLabel = 'GraphQL transport returned top-level errors';

function readPath(value: unknown, pathSegments: string[]): unknown {
  let current = value;
  for (const segment of pathSegments) {
    if (Array.isArray(current)) {
      const index = Number(segment);
      if (!Number.isInteger(index) || index < 0) return undefined;
      current = current[index];
      continue;
    }
    const record = readRecord(current);
    if (!record) return undefined;
    current = record[segment];
  }
  return current;
}

function readStringAtPath(value: unknown, pathSegments: string[], label: string): string {
  return requireString(readPath(value, pathSegments), `${label} at ${pathSegments.join('.')}`);
}

function recordOperation(
  query: string,
  variables: JsonRecord,
  result: ConformanceGraphqlResult<JsonRecord>,
): RecordedOperation {
  return {
    request: { query, variables },
    response: {
      status: result.status,
      ...(result.payload as JsonRecord),
    },
  };
}

function mutationUserErrors(operation: RecordedOperation, rootName: string): unknown[] {
  return readArray(readRecord(readRecord(operation.response['data'])?.[rootName])?.['userErrors']);
}

function assertNoTopLevelErrors(operation: RecordedOperation, label: string): void {
  if (
    typeof operation.response['status'] !== 'number' ||
    operation.response['status'] < 200 ||
    operation.response['status'] >= 300 ||
    operation.response['errors'] !== undefined
  ) {
    throw new Error(`${label} ${missingTransportLabel}: ${JSON.stringify(operation.response, null, 2)}`);
  }
}

function assertSuccessful(operation: RecordedOperation, rootName: string, label: string): void {
  assertNoTopLevelErrors(operation, label);
  const userErrors = mutationUserErrors(operation, rootName);
  if (userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function assertBulkDuplicateLimitReached(operation: RecordedOperation, rootName: string, label: string): void {
  assertNoTopLevelErrors(operation, label);
  const root = readRecord(readRecord(operation.response['data'])?.[rootName]);
  const roleAssignments = readArray(root?.['roleAssignments']);
  const userErrors = readArray(root?.['userErrors']);
  const expectedUserErrors = [
    {
      field: ['rolesToAssign', '0'],
      message: 'Company contact has already been assigned a role in that company location.',
      code: 'LIMIT_REACHED',
    },
  ];
  if (roleAssignments.length !== 1 || JSON.stringify(userErrors) !== JSON.stringify(expectedUserErrors)) {
    throw new Error(`${label} did not capture Shopify duplicate-role behavior: ${JSON.stringify(root, null, 2)}`);
  }
}

async function runRecorded(
  capture: Awaited<ReturnType<typeof createConformanceCapture>>,
  query: string,
  variables: JsonRecord,
): Promise<RecordedOperation> {
  const result = await capture.runGraphqlRequest<JsonRecord>(query, variables);
  return recordOperation(query, variables, result);
}

const capture = await createConformanceCapture();
const companyCreateDocument = await capture.readRequest('b2b', 'b2b-contact-location-assignments-tax-create.graphql');
const locationCreateDocument = await capture.readRequest(
  'b2b',
  'b2b-contact-location-assignments-tax-location-create.graphql',
);
const contactCreateDocument = await capture.readRequest('b2b', 'b2b-bulk-role-assign-duplicate-contact-create.graphql');
const assignRoleDocument = await capture.readRequest('b2b', 'b2b-contact-location-assignments-tax-assign-role.graphql');
const contactAssignRolesDocument = await capture.readRequest(
  'b2b',
  'b2b-contact-location-assignments-tax-contact-assign-roles.graphql',
);
const locationAssignRolesDocument = await capture.readRequest(
  'b2b',
  'b2b-contact-location-assignments-tax-location-assign-roles.graphql',
);
const companyDeleteDocument = await capture.readRequest(
  'b2b',
  'b2b-company-contact-main-delete-company-delete.graphql',
);

let companyId: string | null = null;
let companyDeleted = false;
const cleanup: Record<string, RecordedOperation> = {};

try {
  const companyCreateVariables = {
    input: {
      company: {
        name: `B2B bulk role duplicate ${capture.stamp}`,
        note: 'Bulk role duplicate conformance',
        externalId: `bulk-role-duplicate-${capture.stamp}`,
      },
      companyContact: {
        firstName: 'Bulk',
        lastName: 'Duplicate',
        email: `bulk-role-duplicate-${capture.stamp}@example.com`,
        title: 'Buyer',
      },
      companyLocation: {
        name: `B2B bulk role duplicate HQ ${capture.stamp}`,
        phone: '+16135550111',
      },
    },
  };
  const companyCreate = await runRecorded(capture, companyCreateDocument, companyCreateVariables);
  assertSuccessful(companyCreate, 'companyCreate', 'companyCreate setup');
  companyId = readStringAtPath(companyCreate.response, ['data', 'companyCreate', 'company', 'id'], 'company id');
  const contactId = readStringAtPath(
    companyCreate.response,
    ['data', 'companyCreate', 'company', 'mainContact', 'id'],
    'main contact id',
  );
  const roleId = readStringAtPath(
    companyCreate.response,
    ['data', 'companyCreate', 'company', 'contactRoles', 'nodes', '0', 'id'],
    'role id',
  );

  const duplicateLocationCreate = await runRecorded(capture, locationCreateDocument, {
    companyId,
    input: {
      name: `B2B duplicate location ${capture.stamp}`,
      phone: '+16135550112',
    },
  });
  assertSuccessful(duplicateLocationCreate, 'companyLocationCreate', 'duplicate location setup');
  const duplicateLocationId = readStringAtPath(
    duplicateLocationCreate.response,
    ['data', 'companyLocationCreate', 'companyLocation', 'id'],
    'duplicate location id',
  );

  const validLocationCreate = await runRecorded(capture, locationCreateDocument, {
    companyId,
    input: {
      name: `B2B valid sibling location ${capture.stamp}`,
      phone: '+16135550113',
    },
  });
  assertSuccessful(validLocationCreate, 'companyLocationCreate', 'valid sibling location setup');
  const validLocationId = readStringAtPath(
    validLocationCreate.response,
    ['data', 'companyLocationCreate', 'companyLocation', 'id'],
    'valid sibling location id',
  );

  const secondContactCreate = await runRecorded(capture, contactCreateDocument, {
    companyId,
    input: {
      firstName: 'Second',
      lastName: 'Buyer',
      email: `bulk-role-duplicate-second-${capture.stamp}@example.com`,
      title: 'Second Buyer',
    },
  });
  assertSuccessful(secondContactCreate, 'companyContactCreate', 'second contact setup');
  const secondContactId = readStringAtPath(
    secondContactCreate.response,
    ['data', 'companyContactCreate', 'companyContact', 'id'],
    'second contact id',
  );

  const seedAssign = await runRecorded(capture, assignRoleDocument, {
    companyContactId: contactId,
    companyContactRoleId: roleId,
    companyLocationId: duplicateLocationId,
  });
  assertSuccessful(seedAssign, 'companyContactAssignRole', 'seed role assignment');

  const duplicateContactAssignRoles = await runRecorded(capture, contactAssignRolesDocument, {
    companyContactId: contactId,
    rolesToAssign: [
      { companyContactRoleId: roleId, companyLocationId: duplicateLocationId },
      { companyContactRoleId: roleId, companyLocationId: validLocationId },
    ],
  });
  assertBulkDuplicateLimitReached(
    duplicateContactAssignRoles,
    'companyContactAssignRoles',
    'companyContactAssignRoles duplicate branch',
  );

  const duplicateLocationAssignRoles = await runRecorded(capture, locationAssignRolesDocument, {
    companyLocationId: validLocationId,
    rolesToAssign: [
      { companyContactId: contactId, companyContactRoleId: roleId },
      { companyContactId: secondContactId, companyContactRoleId: roleId },
    ],
  });
  assertBulkDuplicateLimitReached(
    duplicateLocationAssignRoles,
    'companyLocationAssignRoles',
    'companyLocationAssignRoles duplicate branch',
  );

  const companyDelete = await runRecorded(capture, companyDeleteDocument, { id: companyId });
  assertSuccessful(companyDelete, 'companyDelete', 'companyDelete cleanup');
  companyDeleted = true;

  const output = {
    scenarioId,
    capturedAt: new Date().toISOString(),
    storeDomain: capture.storeDomain,
    apiVersion: capture.apiVersion,
    intent: {
      plan: 'Create a disposable company with a contact, role, and locations; seed one contact/location role assignment; prove both bulk role-assign mutations reject a duplicate contact+location entry while accepting a valid sibling entry; then delete the company.',
      setupConditions:
        'The duplicate entry targets a contact that already holds a role at the location. The sibling entry targets a location/contact pair without an existing role assignment.',
    },
    companyCreate,
    duplicateLocationCreate,
    validLocationCreate,
    secondContactCreate,
    seedAssign,
    duplicateContactAssignRoles,
    duplicateLocationAssignRoles,
    companyDelete,
    cleanup,
    upstreamCalls: [],
  };

  const outputPath = capture.fixturePath('b2b', `${scenarioId}.json`);
  await capture.writeJson(outputPath, output);

  // oxlint-disable-next-line no-console -- capture scripts report their output path.
  console.log(JSON.stringify({ ok: true, outputPath }, null, 2));
} finally {
  if (companyId && !companyDeleted) {
    cleanup['companyDelete'] = await runRecorded(capture, companyDeleteDocument, { id: companyId });
  }
}

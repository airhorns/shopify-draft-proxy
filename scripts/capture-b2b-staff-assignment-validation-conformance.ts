import 'dotenv/config';

/* oxlint-disable no-console -- CLI capture script writes status to stdout/stderr. */

import { mkdirSync, writeFileSync } from 'node:fs';
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

const scenarioId = 'b2b-staff-assignment-validation';

const companyCreateDocument = `#graphql
  mutation B2BStaffAssignmentValidationCreate($input: CompanyCreateInput!) {
    companyCreate(input: $input) {
      company {
        id
        name
        locations(first: 1) {
          nodes { id name }
        }
      }
      userErrors { field message code }
    }
  }
`;

const assignUnknownStaffDocument = `#graphql
  mutation B2BStaffAssignmentValidationAssignUnknown(
    $companyLocationId: ID!
    $staffMemberIds: [ID!]!
  ) {
    companyLocationAssignStaffMembers(
      companyLocationId: $companyLocationId
      staffMemberIds: $staffMemberIds
    ) {
      companyLocationStaffMemberAssignments {
        id
        staffMember { id }
        companyLocation { id }
      }
      userErrors { field message code }
    }
  }
`;

const removeUnknownAssignmentDocument = `#graphql
  mutation B2BStaffAssignmentValidationRemoveUnknown(
    $companyLocationStaffMemberAssignmentIds: [ID!]!
  ) {
    companyLocationRemoveStaffMembers(
      companyLocationStaffMemberAssignmentIds: $companyLocationStaffMemberAssignmentIds
    ) {
      deletedCompanyLocationStaffMemberAssignmentIds
      userErrors { field message code }
    }
  }
`;

const readAfterUnknownAssignDocument = `#graphql
  query B2BStaffAssignmentValidationReadAfterUnknown($companyLocationId: ID!) {
    companyLocation(id: $companyLocationId) {
      id
      staffMemberAssignments(first: 5) {
        nodes {
          id
          staffMember { id }
          companyLocation { id }
        }
      }
    }
  }
`;

const companyDeleteDocument = `#graphql
  mutation B2BStaffAssignmentValidationCompanyDelete($id: ID!) {
    companyDelete(id: $id) {
      deletedCompanyId
      userErrors { field message code }
    }
  }
`;

const staffAccessProbeDocument = `#graphql
  query B2BStaffAssignmentValidationStaffProbe {
    staffMembers(first: 1) { nodes { id name } }
  }
`;

const existingAssignmentsProbeDocument = `#graphql
  query B2BStaffAssignmentValidationExistingAssignments {
    companyLocations(first: 25) {
      nodes {
        id
        name
        staffMemberAssignments(first: 10) {
          nodes {
            id
            staffMember { id }
            companyLocation { id }
          }
        }
      }
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

function assertOnlyUserErrors(result: ConformanceGraphqlResult, root: string, label: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} returned top-level errors: ${JSON.stringify(result, null, 2)}`);
  }
  const userErrors = readUserErrors(result.payload, root);
  if (userErrors.length === 0) {
    throw new Error(`${label} did not return userErrors: ${JSON.stringify(result, null, 2)}`);
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

async function main(): Promise<void> {
  const timestamp = Date.now();
  const config = readConformanceScriptConfig({ exitOnMissing: true });
  const adminAccessToken = await getValidConformanceAccessToken({
    adminOrigin: config.adminOrigin,
    apiVersion: config.apiVersion,
  });
  const { runGraphqlRequest } = createAdminGraphqlClient({
    adminOrigin: config.adminOrigin,
    apiVersion: config.apiVersion,
    headers: buildAdminAuthHeaders(adminAccessToken),
  });

  let companyId: string | null = null;
  let companyDeleted = false;
  const cleanup: Record<string, RecordedOperation> = {};

  try {
    const staffAccessProbe = recordOperation(
      staffAccessProbeDocument,
      {},
      await runGraphqlRequest(staffAccessProbeDocument, {}),
    );
    const existingAssignmentsProbe = recordOperation(
      existingAssignmentsProbeDocument,
      {},
      await runGraphqlRequest(existingAssignmentsProbeDocument, {}),
    );

    const companyCreateVariables = {
      input: {
        company: {
          name: `HAR-761 staff validation ${timestamp}`,
          externalId: `har-761-staff-validation-${timestamp}`,
        },
        companyLocation: {
          name: `HAR-761 staff validation ${timestamp} HQ`,
        },
      },
    };
    const companyCreateResult = await runGraphqlRequest(companyCreateDocument, companyCreateVariables);
    assertSuccessful(companyCreateResult, 'companyCreate', 'companyCreate setup');
    const companyCreate = recordOperation(companyCreateDocument, companyCreateVariables, companyCreateResult);
    companyId = readStringAtPath(companyCreate.response, ['data', 'companyCreate', 'company', 'id'], 'companyCreate');
    const companyLocationId = readStringAtPath(
      companyCreate.response,
      ['data', 'companyCreate', 'company', 'locations', 'nodes', '0', 'id'],
      'companyCreate location',
    );

    const assignUnknownStaffVariables = {
      companyLocationId,
      staffMemberIds: ['gid://shopify/StaffMember/999999999'],
    };
    const assignUnknownStaffResult = await runGraphqlRequest(assignUnknownStaffDocument, assignUnknownStaffVariables);
    assertOnlyUserErrors(
      assignUnknownStaffResult,
      'companyLocationAssignStaffMembers',
      'companyLocationAssignStaffMembers unknown staff',
    );
    const assignUnknownStaff = recordOperation(
      assignUnknownStaffDocument,
      assignUnknownStaffVariables,
      assignUnknownStaffResult,
    );

    const removeUnknownAssignmentVariables = {
      companyLocationStaffMemberAssignmentIds: ['gid://shopify/CompanyLocationStaffMemberAssignment/999999999'],
    };
    const removeUnknownAssignmentResult = await runGraphqlRequest(
      removeUnknownAssignmentDocument,
      removeUnknownAssignmentVariables,
    );
    assertOnlyUserErrors(
      removeUnknownAssignmentResult,
      'companyLocationRemoveStaffMembers',
      'companyLocationRemoveStaffMembers unknown assignment',
    );
    const removeUnknownAssignment = recordOperation(
      removeUnknownAssignmentDocument,
      removeUnknownAssignmentVariables,
      removeUnknownAssignmentResult,
    );

    const readAfterUnknownAssignVariables = { companyLocationId };
    const readAfterUnknownAssignResult = await runGraphqlRequest(
      readAfterUnknownAssignDocument,
      readAfterUnknownAssignVariables,
    );
    assertSuccessful(readAfterUnknownAssignResult, 'companyLocation', 'companyLocation read after unknown staff');
    const readAfterUnknownAssign = recordOperation(
      readAfterUnknownAssignDocument,
      readAfterUnknownAssignVariables,
      readAfterUnknownAssignResult,
    );

    const companyDeleteVariables = { id: companyId };
    const companyDeleteResult = await runGraphqlRequest(companyDeleteDocument, companyDeleteVariables);
    assertSuccessful(companyDeleteResult, 'companyDelete', 'companyDelete cleanup');
    const companyDelete = recordOperation(companyDeleteDocument, companyDeleteVariables, companyDeleteResult);
    companyDeleted = true;

    const output = {
      scenarioId,
      capturedAt: new Date().toISOString(),
      storeDomain: config.storeDomain,
      apiVersion: config.apiVersion,
      intent: {
        ticket: 'HAR-761',
        plan: 'Create a disposable B2B company location, capture unknown staff-member and unknown staff-assignment validation, confirm no staff assignment was staged, and delete the company.',
      },
      staffAccessProbe,
      existingAssignmentsProbe,
      companyCreate,
      assignUnknownStaff,
      removeUnknownAssignment,
      readAfterUnknownAssign,
      companyDelete,
      cleanup,
      upstreamCalls: [],
    };

    const outputDir = path.join('fixtures', 'conformance', config.storeDomain, config.apiVersion, 'b2b');
    mkdirSync(outputDir, { recursive: true });
    const outputPath = path.join(outputDir, `${scenarioId}.json`);
    writeFileSync(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');

    console.log(JSON.stringify({ ok: true, outputPath }, null, 2));
  } finally {
    if (companyId && !companyDeleted) {
      const result = await runGraphqlRequest(companyDeleteDocument, { id: companyId });
      cleanup['companyDelete'] = recordOperation(companyDeleteDocument, { id: companyId }, result);
    }
  }
}

main().catch((error: unknown) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
});

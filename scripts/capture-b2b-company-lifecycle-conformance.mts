import 'dotenv/config';

import { spawnSync } from 'node:child_process';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
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

const scenarioId = 'b2b-company-contact-main-delete';
const timestamp = Date.now();
const mainCompanyName = `B2B lifecycle main company ${timestamp}`;
const bulkCompanyName = `B2B lifecycle bulk company ${timestamp}`;
const customerEmail = `b2b-lifecycle-${timestamp}@example.com`;
const mainCompanyQuery = `name:"${mainCompanyName}"`;
const missingCompanyId = `gid://shopify/Company/${timestamp}`;
const missingCustomerId = `gid://shopify/Customer/${timestamp}`;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const companyCreateDocument = `#graphql
  mutation B2BCompanyLifecycleCreate($input: CompanyCreateInput!) {
    companyCreate(input: $input) {
      company {
        id
        name
        note
        externalId
        contactsCount { count }
        locationsCount { count }
        mainContact { id title isMainContact }
        contacts(first: 5) { nodes { id title isMainContact } }
        locations(first: 5) { nodes { id name } }
        contactRoles(first: 5) { nodes { id name } }
      }
      userErrors { field message code }
    }
  }
`;

const companyUpdateDocument = `#graphql
  mutation B2BCompanyLifecycleUpdate($companyId: ID!, $input: CompanyInput!) {
    companyUpdate(companyId: $companyId, input: $input) {
      company { id name note externalId }
      userErrors { field message code }
    }
  }
`;

const customerCreateDocument = `#graphql
  mutation B2BCompanyLifecycleCustomerCreate($input: CustomerInput!) {
    customerCreate(input: $input) {
      customer { id email }
      userErrors { field message }
    }
  }
`;

const customerDeleteDocument = `#graphql
  mutation B2BCompanyLifecycleCustomerDelete($input: CustomerDeleteInput!) {
    customerDelete(input: $input) {
      deletedCustomerId
      userErrors { field message }
    }
  }
`;

const contactCreateDocument = `#graphql
  mutation B2BCompanyLifecycleContactCreate($companyId: ID!, $input: CompanyContactInput!) {
    companyContactCreate(companyId: $companyId, input: $input) {
      companyContact { id title isMainContact company { id name } }
      userErrors { field message code }
    }
  }
`;

const assignCustomerDocument = `#graphql
  mutation B2BCompanyLifecycleAssignCustomer($companyId: ID!, $customerId: ID!) {
    companyAssignCustomerAsContact(companyId: $companyId, customerId: $customerId) {
      companyContact { id isMainContact customer { id } company { id name } }
      userErrors { field message code }
    }
  }
`;

const assignMainContactDocument = `#graphql
  mutation B2BCompanyLifecycleAssignMainContact($companyId: ID!, $companyContactId: ID!) {
    companyAssignMainContact(companyId: $companyId, companyContactId: $companyContactId) {
      company {
        id
        mainContact { id title isMainContact }
        contacts(first: 5) { nodes { id title isMainContact } }
      }
      userErrors { field message code }
    }
  }
`;

const revokeMainContactDocument = `#graphql
  mutation B2BCompanyLifecycleRevokeMainContact($companyId: ID!) {
    companyRevokeMainContact(companyId: $companyId) {
      company {
        id
        mainContact { id title isMainContact }
        contacts(first: 5) { nodes { id title isMainContact } }
      }
      userErrors { field message code }
    }
  }
`;

const contactDeleteDocument = `#graphql
  mutation B2BCompanyLifecycleContactDelete($companyContactId: ID!) {
    companyContactDelete(companyContactId: $companyContactId) {
      deletedCompanyContactId
      userErrors { field message code }
    }
  }
`;

const readAfterMainContactDocument = `#graphql
  query B2BCompanyLifecycleReadAfterMainContact($companyId: ID!) {
    company(id: $companyId) {
      id
      name
      note
      externalId
      contactsCount { count }
      locationsCount { count }
      mainContact { id title isMainContact }
      contacts(first: 5) {
        nodes {
          id
          title
          isMainContact
          customer { id }
        }
      }
      locations(first: 5) { nodes { id name } }
    }
  }
`;

const assignCustomerColdReadbackDocument = await readFile(
  'config/parity-requests/b2b/b2b-assign-customer-as-contact-cold-readback.graphql',
  'utf8',
);

async function readRustQuery(name: string): Promise<string> {
  const source = await readFile('src/proxy/b2b_customers/companies.rs', 'utf8');
  const match = source.match(new RegExp(`const ${name}: &str = r#"([\\s\\S]*?)"#;`, 'u'));
  if (!match?.[1]) {
    throw new Error(`${name} was not found in the B2B runtime`);
  }
  return match[1];
}

const mutationTargetsHydrateDocument = await readRustQuery('B2B_MUTATION_TARGETS_HYDRATE_QUERY');
const companyContactsWindowHydrateDocument = await readRustQuery('B2B_COMPANY_CONTACTS_WINDOW_HYDRATE_QUERY');

function mutationTargetsHydrateVariables(ids: string[]): JsonRecord {
  return {
    ids,
    includeCompanyLocationCardinality: false,
    includeCompanyDeleteBlockers: false,
    includeLocationDeleteBlockers: false,
    includeContactCustomer: false,
    includeCustomerContactProfiles: true,
    includeContactRoleMembership: false,
    includeLocationRoleMembership: false,
    includeStaffAssignments: false,
    includeAllContactAssignments: false,
    roleMembershipLocationQuery: null,
    roleMembershipContactQuery: null,
  };
}

const companiesDeleteDocument = `#graphql
  mutation B2BCompanyLifecycleCompaniesDelete($companyIds: [ID!]!) {
    companiesDelete(companyIds: $companyIds) {
      deletedCompanyIds
      userErrors { field message code }
    }
  }
`;

const companyDeleteDocument = `#graphql
  mutation B2BCompanyLifecycleCompanyDelete($id: ID!) {
    companyDelete(id: $id) {
      deletedCompanyId
      userErrors { field message code }
    }
  }
`;

const readAfterDeleteDocument = `#graphql
  query B2BCompanyLifecycleReadAfterDelete($companyId: ID!, $companyQuery: String!) {
    company(id: $companyId) { id }
    companies(first: 5, query: $companyQuery) {
      nodes { id name }
      pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
    }
  }
`;

function readRecord(value: unknown): JsonRecord | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readPath(value: unknown, pathSegments: string[]): unknown {
  let current = value;
  for (const segment of pathSegments) {
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

function assertUserError(result: ConformanceGraphqlResult, root: string, label: string): void {
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
    request: {
      query,
      variables,
    },
    response: {
      status: result.status,
      ...result.payload,
    },
  };
}

function recordUpstreamCall(operation: RecordedOperation, operationName = 'B2BMutationTargetsHydrate'): JsonRecord {
  return {
    operationName,
    variables: operation.request.variables,
    query: operation.request.query,
    response: {
      status: operation.response.status,
      body: {
        data: operation.response.data,
        extensions: operation.response.extensions,
      },
    },
  };
}

function formatGeneratedJson(filePath: string): void {
  const result = spawnSync('corepack', ['pnpm', 'exec', 'oxfmt', filePath], { stdio: 'inherit' });
  if (result.status !== 0) {
    throw new Error(`Failed to format generated JSON file: ${filePath}`);
  }
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

async function runExpectedUserError(
  query: string,
  variables: JsonRecord,
  root: string,
  label: string,
): Promise<RecordedOperation> {
  const result = await runGraphqlRequest(query, variables);
  assertUserError(result, root, label);
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

let mainCompanyId: string | null = null;
let bulkCompanyId: string | null = null;
let customerId: string | null = null;
let mainCompanyDeleted = false;
let bulkCompanyDeleted = false;
let customerDeleted = false;
const cleanup: Record<string, RecordedOperation> = {};

try {
  const companyCreateVariables = {
    input: {
      company: {
        name: mainCompanyName,
        note: 'B2B lifecycle parity',
        externalId: `b2b-lifecycle-main-${timestamp}`,
      },
      companyContact: {
        firstName: 'Har',
        lastName: 'Main',
        email: `b2b-lifecycle-main-${timestamp}@example.com`,
        title: 'Buyer',
      },
      companyLocation: {
        name: `${mainCompanyName} HQ`,
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
    'companyCreate lifecycle setup',
  );
  mainCompanyId = readStringAtPath(
    companyCreate.response,
    ['data', 'companyCreate', 'company', 'id'],
    'companyCreate lifecycle setup',
  );

  const companyUpdateVariables = {
    companyId: mainCompanyId,
    input: {
      name: `${mainCompanyName} updated`,
      note: 'B2B lifecycle parity updated',
      externalId: `b2b-lifecycle-main-updated-${timestamp}`,
    },
  };
  const companyUpdate = await runRequired(
    companyUpdateDocument,
    companyUpdateVariables,
    'companyUpdate',
    'companyUpdate lifecycle',
  );

  const customerCreateVariables = {
    input: {
      email: customerEmail,
      firstName: 'Har',
      lastName: 'Assigned',
      tags: ['b2b-lifecycle'],
    },
  };
  const customerCreate = await runRequired(
    customerCreateDocument,
    customerCreateVariables,
    'customerCreate',
    'customerCreate setup for companyAssignCustomerAsContact',
  );
  customerId = readStringAtPath(
    customerCreate.response,
    ['data', 'customerCreate', 'customer', 'id'],
    'customerCreate setup for companyAssignCustomerAsContact',
  );

  const hydrateAssignPrerequisites = await runRequired(
    mutationTargetsHydrateDocument,
    mutationTargetsHydrateVariables([mainCompanyId, customerId]),
    'nodes',
    'companyAssignCustomerAsContact batched prerequisite hydrate',
  );
  const hydrateMissingCompany = await runRequired(
    mutationTargetsHydrateDocument,
    mutationTargetsHydrateVariables([missingCompanyId, customerId]),
    'nodes',
    'companyAssignCustomerAsContact missing company prerequisite hydrate',
  );
  const hydrateMissingCustomer = await runRequired(
    mutationTargetsHydrateDocument,
    mutationTargetsHydrateVariables([mainCompanyId, missingCustomerId]),
    'nodes',
    'companyAssignCustomerAsContact missing customer prerequisite hydrate',
  );
  const hydrateBothMissing = await runRequired(
    mutationTargetsHydrateDocument,
    mutationTargetsHydrateVariables([missingCompanyId, missingCustomerId]),
    'nodes',
    'companyAssignCustomerAsContact both missing prerequisites hydrate',
  );
  const hydrateAssignCompanyContactsWindow = await runRequired(
    companyContactsWindowHydrateDocument,
    { id: mainCompanyId },
    'company',
    'companyAssignCustomerAsContact downstream company contacts window hydrate',
  );

  const missingCompanyAssign = await runExpectedUserError(
    assignCustomerDocument,
    { companyId: missingCompanyId, customerId },
    'companyAssignCustomerAsContact',
    'companyAssignCustomerAsContact missing company validation',
  );
  const missingCustomerAssign = await runExpectedUserError(
    assignCustomerDocument,
    { companyId: mainCompanyId, customerId: missingCustomerId },
    'companyAssignCustomerAsContact',
    'companyAssignCustomerAsContact missing customer validation',
  );
  const bothMissingAssign = await runExpectedUserError(
    assignCustomerDocument,
    { companyId: missingCompanyId, customerId: missingCustomerId },
    'companyAssignCustomerAsContact',
    'companyAssignCustomerAsContact company-before-customer validation ordering',
  );

  const assignCustomerVariables = { companyId: mainCompanyId, customerId };
  const assignCustomer = await runRequired(
    assignCustomerDocument,
    assignCustomerVariables,
    'companyAssignCustomerAsContact',
    'companyAssignCustomerAsContact lifecycle',
  );
  const assignedCustomerContactId = readStringAtPath(
    assignCustomer.response,
    ['data', 'companyAssignCustomerAsContact', 'companyContact', 'id'],
    'companyAssignCustomerAsContact lifecycle',
  );
  const readAfterAssignCustomer = await runRequired(
    assignCustomerColdReadbackDocument,
    { companyId: mainCompanyId, companyContactId: assignedCustomerContactId, customerId },
    'company',
    'company/contact/customer downstream read after companyAssignCustomerAsContact',
  );

  const contactCreateVariables = {
    companyId: mainCompanyId,
    input: {
      email: `b2b-lifecycle-secondary-${timestamp}@example.com`,
      firstName: 'Har',
      lastName: 'Secondary',
      title: 'Approver',
    },
  };
  const contactCreate = await runRequired(
    contactCreateDocument,
    contactCreateVariables,
    'companyContactCreate',
    'companyContactCreate lifecycle',
  );
  const secondaryContactId = readStringAtPath(
    contactCreate.response,
    ['data', 'companyContactCreate', 'companyContact', 'id'],
    'companyContactCreate lifecycle',
  );

  const assignMainContactVariables = { companyId: mainCompanyId, companyContactId: secondaryContactId };
  const assignMainContact = await runRequired(
    assignMainContactDocument,
    assignMainContactVariables,
    'companyAssignMainContact',
    'companyAssignMainContact lifecycle',
  );

  const revokeMainContactVariables = { companyId: mainCompanyId };
  const revokeMainContact = await runRequired(
    revokeMainContactDocument,
    revokeMainContactVariables,
    'companyRevokeMainContact',
    'companyRevokeMainContact lifecycle',
  );

  const readAfterMainContactVariables = { companyId: mainCompanyId };
  const readAfterMainContact = await runRequired(
    readAfterMainContactDocument,
    readAfterMainContactVariables,
    'company',
    'company downstream read after main-contact lifecycle',
  );

  const assignMainContactBeforeDelete = await runRequired(
    assignMainContactDocument,
    assignMainContactVariables,
    'companyAssignMainContact',
    'companyAssignMainContact before main-contact delete',
  );

  const deleteMainContactVariables = { companyContactId: secondaryContactId };
  const deleteMainContact = await runRequired(
    contactDeleteDocument,
    deleteMainContactVariables,
    'companyContactDelete',
    'companyContactDelete clears main contact',
  );

  const readAfterMainContactDelete = await runRequired(
    readAfterMainContactDocument,
    readAfterMainContactVariables,
    'company',
    'company downstream read after deleting main contact',
  );

  const bulkCompanyCreateVariables = {
    input: {
      company: {
        name: bulkCompanyName,
        externalId: `b2b-lifecycle-bulk-${timestamp}`,
      },
    },
  };
  const bulkCompanyCreate = await runRequired(
    companyCreateDocument,
    bulkCompanyCreateVariables,
    'companyCreate',
    'companyCreate setup for companiesDelete',
  );
  bulkCompanyId = readStringAtPath(
    bulkCompanyCreate.response,
    ['data', 'companyCreate', 'company', 'id'],
    'companyCreate setup for companiesDelete',
  );

  const wrongCompanyAssignMainContactVariables = {
    companyId: bulkCompanyId,
    companyContactId: readStringAtPath(
      companyCreate.response,
      ['data', 'companyCreate', 'company', 'mainContact', 'id'],
      'companyCreate main contact for wrong-company assignment',
    ),
  };
  const wrongCompanyAssignMainContact = await runExpectedUserError(
    assignMainContactDocument,
    wrongCompanyAssignMainContactVariables,
    'companyAssignMainContact',
    'companyAssignMainContact wrong-company validation',
  );

  const companiesDeleteVariables = { companyIds: [bulkCompanyId] };
  const companiesDelete = await runRequired(
    companiesDeleteDocument,
    companiesDeleteVariables,
    'companiesDelete',
    'companiesDelete lifecycle',
  );
  bulkCompanyDeleted = true;

  const bulkReadAfterDeleteVariables = {
    companyId: bulkCompanyId,
    companyQuery: `name:"${bulkCompanyName}"`,
  };
  const bulkReadAfterDelete = await runRequired(
    readAfterDeleteDocument,
    bulkReadAfterDeleteVariables,
    'company',
    'company downstream read after companiesDelete',
  );

  const companyDeleteVariables = { id: mainCompanyId };
  const companyDelete = await runRequired(
    companyDeleteDocument,
    companyDeleteVariables,
    'companyDelete',
    'companyDelete lifecycle',
  );
  mainCompanyDeleted = true;

  const readAfterDeleteVariables = {
    companyId: mainCompanyId,
    companyQuery: mainCompanyQuery,
  };
  const readAfterDelete = await runRequired(
    readAfterDeleteDocument,
    readAfterDeleteVariables,
    'company',
    'company downstream read after companyDelete',
  );

  if (customerId) {
    cleanup['customerDelete'] = await runCleanup(
      customerDeleteDocument,
      { input: { id: customerId } },
      'customerDelete',
      'customerDelete setup cleanup',
    );
    customerDeleted = true;
  }

  const output = {
    scenarioId,
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    upstreamCalls: [
      recordUpstreamCall(hydrateAssignPrerequisites),
      recordUpstreamCall(hydrateMissingCompany),
      recordUpstreamCall(hydrateMissingCustomer),
      recordUpstreamCall(hydrateBothMissing),
      recordUpstreamCall(hydrateAssignCompanyContactsWindow, 'B2BCompanyContactsWindowHydrate'),
    ],
    intent: {
      plan: 'Create disposable B2B companies and a customer; record batched cold company/customer prerequisite reads, missing-resource controls, customer-as-contact assignment and downstream reads, company update, main-contact assignment/revocation, wrong-company main-contact validation, main-contact delete clearing, bulk delete, explicit delete, and post-delete empty reads.',
    },
    proxyVariables: {
      mainCompanyQuery,
    },
    companyCreate,
    companyUpdate,
    customerCreate,
    hydrateAssignPrerequisites,
    hydrateMissingCompany,
    hydrateMissingCustomer,
    hydrateBothMissing,
    hydrateAssignCompanyContactsWindow,
    missingCompanyAssign,
    missingCustomerAssign,
    bothMissingAssign,
    contactCreate,
    assignCustomer,
    readAfterAssignCustomer,
    assignMainContact,
    revokeMainContact,
    readAfterMainContact,
    assignMainContactBeforeDelete,
    deleteMainContact,
    readAfterMainContactDelete,
    bulkCompanyCreate,
    wrongCompanyAssignMainContact,
    companiesDelete,
    bulkReadAfterDelete,
    companyDelete,
    readAfterDelete,
    cleanup,
  };

  const outputPath = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'b2b', `${scenarioId}.json`);
  await mkdir(path.dirname(outputPath), { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');
  formatGeneratedJson(outputPath);

  // oxlint-disable-next-line no-console -- capture scripts report their output path.
  console.log(JSON.stringify({ ok: true, outputPath }, null, 2));
} finally {
  if (mainCompanyId && !mainCompanyDeleted) {
    cleanup['companyDelete'] = await runCleanup(
      companyDeleteDocument,
      { id: mainCompanyId },
      'companyDelete',
      'companyDelete finally cleanup',
    );
  }
  if (bulkCompanyId && !bulkCompanyDeleted) {
    cleanup['bulkCompanyDelete'] = await runCleanup(
      companyDeleteDocument,
      { id: bulkCompanyId },
      'companyDelete',
      'bulk companyDelete finally cleanup',
    );
  }
  if (customerId && !customerDeleted) {
    cleanup['customerDelete'] = await runCleanup(
      customerDeleteDocument,
      { input: { id: customerId } },
      'customerDelete',
      'customerDelete finally cleanup',
    );
  }
}

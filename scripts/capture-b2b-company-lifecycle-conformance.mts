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

const scenarioId = 'b2b-company-contact-main-delete';
const timestamp = Date.now();
const mainCompanyName = `HAR-445 main company ${timestamp}`;
const bulkCompanyName = `HAR-445 bulk company ${timestamp}`;
const customerEmail = `har-445-b2b-${timestamp}@example.com`;
const mainCompanyQuery = `name:"${mainCompanyName}"`;

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
        note: 'HAR-445 B2B lifecycle parity',
        externalId: `har-445-main-${timestamp}`,
      },
      companyContact: {
        firstName: 'Har',
        lastName: 'Main',
        email: `har-445-main-${timestamp}@example.com`,
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
      note: 'HAR-445 B2B lifecycle parity updated',
      externalId: `har-445-main-updated-${timestamp}`,
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
      tags: ['har-445-b2b-lifecycle'],
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

  const contactCreateVariables = {
    companyId: mainCompanyId,
    input: {
      email: `har-445-secondary-${timestamp}@example.com`,
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

  const assignCustomerVariables = { companyId: mainCompanyId, customerId };
  const assignCustomer = await runRequired(
    assignCustomerDocument,
    assignCustomerVariables,
    'companyAssignCustomerAsContact',
    'companyAssignCustomerAsContact lifecycle',
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

  const bulkCompanyCreateVariables = {
    input: {
      company: {
        name: bulkCompanyName,
        externalId: `har-445-bulk-${timestamp}`,
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
    intent: {
      ticket: 'HAR-445',
      plan: 'Create disposable B2B companies and a customer; record company update, customer-as-contact assignment, main-contact assignment/revocation, bulk delete, explicit delete, and post-delete empty reads.',
    },
    proxyVariables: {
      mainCompanyQuery,
    },
    companyCreate,
    companyUpdate,
    customerCreate,
    contactCreate,
    assignCustomer,
    assignMainContact,
    revokeMainContact,
    readAfterMainContact,
    bulkCompanyCreate,
    companiesDelete,
    bulkReadAfterDelete,
    companyDelete,
    readAfterDelete,
    cleanup,
  };

  const outputPath = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'b2b', `${scenarioId}.json`);
  await mkdir(path.dirname(outputPath), { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');

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

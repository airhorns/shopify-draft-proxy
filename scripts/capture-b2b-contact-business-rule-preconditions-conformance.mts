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

const scenarioId = 'b2b-contact-business-rule-preconditions';
const timestamp = Date.now();
const runKey = `har-620-${timestamp}`;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const companyCreateDocument = `#graphql
  mutation B2BContactBusinessRulesCompanyCreate($input: CompanyCreateInput!) {
    companyCreate(input: $input) {
      company {
        id
        name
        contactsCount {
          count
        }
        mainContact {
          id
          title
          isMainContact
          roleAssignments(first: 5) {
            nodes {
              id
              role {
                id
                name
              }
              companyLocation {
                id
                name
              }
            }
          }
        }
        locations(first: 5) {
          nodes {
            id
            name
          }
        }
        contactRoles(first: 5) {
          nodes {
            id
            name
          }
        }
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const contactAssignRoleDocument = `#graphql
  mutation B2BContactBusinessRulesAssignRole(
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
        companyContact {
          id
        }
        role {
          id
          name
        }
        companyLocation {
          id
          name
        }
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const contactDeleteDocument = `#graphql
  mutation B2BContactBusinessRulesContactDelete($companyContactId: ID!) {
    companyContactDelete(companyContactId: $companyContactId) {
      deletedCompanyContactId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const companyReadDocument = `#graphql
  query B2BContactBusinessRulesCompanyRead($companyId: ID!, $companyContactId: ID!) {
    company(id: $companyId) {
      id
      mainContact {
        id
        isMainContact
      }
      contactsCount {
        count
      }
    }
    companyContact(id: $companyContactId) {
      id
      isMainContact
    }
  }
`;

const draftOrderCreateDocument = `#graphql
  mutation B2BContactBusinessRulesDraftOrderCreate($input: DraftOrderInput!) {
    draftOrderCreate(input: $input) {
      draftOrder {
        id
        status
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const draftOrderCompleteDocument = `#graphql
  mutation B2BContactBusinessRulesDraftOrderComplete($id: ID!, $paymentPending: Boolean!) {
    draftOrderComplete(id: $id, paymentPending: $paymentPending) {
      draftOrder {
        id
        status
        order {
          id
          name
          purchasingEntity {
            ... on PurchasingCompany {
              company {
                id
              }
              contact {
                id
              }
              location {
                id
              }
            }
          }
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const orderCancelDocument = `#graphql
  mutation B2BContactBusinessRulesOrderCancel(
    $orderId: ID!
    $reason: OrderCancelReason!
    $notifyCustomer: Boolean!
    $restock: Boolean!
  ) {
    orderCancel(orderId: $orderId, reason: $reason, notifyCustomer: $notifyCustomer, restock: $restock) {
      job {
        id
        done
      }
      orderCancelUserErrors {
        field
        message
        code
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const companyDeleteDocument = `#graphql
  mutation B2BContactBusinessRulesCompanyDelete($id: ID!) {
    companyDelete(id: $id) {
      deletedCompanyId
      userErrors {
        field
        message
        code
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

function assertUserError(
  operation: RecordedOperation,
  root: string,
  expected: { field: string[] | null; message: string; code: string },
  label: string,
): void {
  const userErrors = readUserErrors(operation.response, root);
  const matchingError = userErrors.find((error) => {
    const record = readRecord(error);
    return (
      record !== null &&
      JSON.stringify(record['field'] ?? null) === JSON.stringify(expected.field) &&
      record['message'] === expected.message &&
      record['code'] === expected.code
    );
  });

  if (!matchingError) {
    throw new Error(
      `${label} did not return expected userError ${JSON.stringify(expected)}: ${JSON.stringify(userErrors, null, 2)}`,
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

async function runOperation(query: string, variables: JsonRecord): Promise<RecordedOperation> {
  return recordOperation(query, variables, await runGraphqlRequest(query, variables));
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

async function runCleanup(query: string, variables: JsonRecord): Promise<RecordedOperation> {
  return runOperation(query, variables);
}

function companyCreateVariables(label: string): JsonRecord {
  const name = `HAR-620 ${label} ${timestamp}`;
  return {
    input: {
      company: {
        name,
        note: `HAR-620 business-rule preconditions ${label}`,
        externalId: `${runKey}-${label}`,
      },
      companyContact: {
        firstName: 'Har',
        lastName: `Business ${label}`,
        email: `${runKey}-${label}@example.com`,
        title: 'Buyer',
      },
      companyLocation: {
        name: `${name} HQ`,
        phone: '+16135550620',
        billingAddress: {
          address1: '620 B2B Way',
          city: 'Ottawa',
          countryCode: 'CA',
        },
      },
    },
  };
}

function draftOrderVariables(companyId: string, companyContactId: string, companyLocationId: string): JsonRecord {
  return {
    input: {
      purchasingEntity: {
        purchasingCompany: {
          companyId,
          companyContactId,
          companyLocationId,
        },
      },
      email: `${runKey}-orders@example.com`,
      note: 'HAR-620 B2B contact delete order-history precondition',
      tags: ['har-620', 'b2b-contact-delete-order-precondition'],
      visibleToCustomer: false,
      lineItems: [
        {
          title: 'HAR-620 B2B order-history custom item',
          quantity: 1,
          originalUnitPrice: '1.00',
          requiresShipping: false,
          taxable: false,
        },
      ],
    },
  };
}

function companyIdsFromCreate(operation: RecordedOperation): {
  companyId: string;
  contactId: string;
  locationId: string;
  firstRoleId: string;
} {
  return {
    companyId: readStringAtPath(operation.response, ['data', 'companyCreate', 'company', 'id'], 'companyCreate id'),
    contactId: readStringAtPath(
      operation.response,
      ['data', 'companyCreate', 'company', 'mainContact', 'id'],
      'companyCreate mainContact id',
    ),
    locationId: readStringAtPath(
      operation.response,
      ['data', 'companyCreate', 'company', 'locations', 'nodes', '0', 'id'],
      'companyCreate location id',
    ),
    firstRoleId: readStringAtPath(
      operation.response,
      ['data', 'companyCreate', 'company', 'contactRoles', 'nodes', '0', 'id'],
      'companyCreate first role id',
    ),
  };
}

const createdCompanyIds: string[] = [];
let completedOrderId: string | null = null;
const cleanup: Record<string, RecordedOperation> = {};

try {
  const roleCompanyCreate = await runRequired(
    companyCreateDocument,
    companyCreateVariables('role-company'),
    'companyCreate',
    'role companyCreate',
  );
  const roleCompany = companyIdsFromCreate(roleCompanyCreate);
  createdCompanyIds.push(roleCompany.companyId);

  const duplicateAssign = await runOperation(contactAssignRoleDocument, {
    companyContactId: roleCompany.contactId,
    companyContactRoleId: roleCompany.firstRoleId,
    companyLocationId: roleCompany.locationId,
  });
  assertHttpGraphqlOk(
    { status: duplicateAssign.response.status as number, payload: duplicateAssign.response },
    'duplicate assign',
  );
  assertUserError(
    duplicateAssign,
    'companyContactAssignRole',
    {
      field: null,
      message: 'Company contact has already been assigned a role in that company location.',
      code: 'LIMIT_REACHED',
    },
    'duplicate companyContactAssignRole',
  );

  const foreignCompanyCreate = await runRequired(
    companyCreateDocument,
    companyCreateVariables('foreign-company'),
    'companyCreate',
    'foreign companyCreate',
  );
  const foreignCompany = companyIdsFromCreate(foreignCompanyCreate);
  createdCompanyIds.push(foreignCompany.companyId);

  const foreignRoleAssign = await runOperation(contactAssignRoleDocument, {
    companyContactId: roleCompany.contactId,
    companyContactRoleId: foreignCompany.firstRoleId,
    companyLocationId: roleCompany.locationId,
  });
  assertHttpGraphqlOk(
    { status: foreignRoleAssign.response.status as number, payload: foreignRoleAssign.response },
    'foreign role assign',
  );
  assertUserError(
    foreignRoleAssign,
    'companyContactAssignRole',
    {
      field: ['companyContactRoleId'],
      message: "The company contact role doesn't exist.",
      code: 'RESOURCE_NOT_FOUND',
    },
    'foreign role companyContactAssignRole',
  );

  const foreignLocationAssign = await runOperation(contactAssignRoleDocument, {
    companyContactId: roleCompany.contactId,
    companyContactRoleId: roleCompany.firstRoleId,
    companyLocationId: foreignCompany.locationId,
  });
  assertHttpGraphqlOk(
    { status: foreignLocationAssign.response.status as number, payload: foreignLocationAssign.response },
    'foreign location assign',
  );
  assertUserError(
    foreignLocationAssign,
    'companyContactAssignRole',
    {
      field: ['companyLocationId'],
      message: "The company location doesn't exist.",
      code: 'RESOURCE_NOT_FOUND',
    },
    'foreign location companyContactAssignRole',
  );

  const missingRoleAssign = await runOperation(contactAssignRoleDocument, {
    companyContactId: roleCompany.contactId,
    companyContactRoleId: 'gid://shopify/CompanyContactRole/999999999999999',
    companyLocationId: roleCompany.locationId,
  });
  assertHttpGraphqlOk(
    { status: missingRoleAssign.response.status as number, payload: missingRoleAssign.response },
    'missing role assign',
  );
  assertUserError(
    missingRoleAssign,
    'companyContactAssignRole',
    {
      field: ['companyContactRoleId'],
      message: "The company contact role doesn't exist.",
      code: 'RESOURCE_NOT_FOUND',
    },
    'missing role companyContactAssignRole',
  );

  const missingLocationAssign = await runOperation(contactAssignRoleDocument, {
    companyContactId: roleCompany.contactId,
    companyContactRoleId: roleCompany.firstRoleId,
    companyLocationId: 'gid://shopify/CompanyLocation/999999999999999',
  });
  assertHttpGraphqlOk(
    { status: missingLocationAssign.response.status as number, payload: missingLocationAssign.response },
    'missing location assign',
  );
  assertUserError(
    missingLocationAssign,
    'companyContactAssignRole',
    {
      field: ['companyLocationId'],
      message: "The company location doesn't exist.",
      code: 'RESOURCE_NOT_FOUND',
    },
    'missing location companyContactAssignRole',
  );

  const deleteSuccessCompanyCreate = await runRequired(
    companyCreateDocument,
    companyCreateVariables('delete-success'),
    'companyCreate',
    'delete success companyCreate',
  );
  const deleteSuccessCompany = companyIdsFromCreate(deleteSuccessCompanyCreate);
  createdCompanyIds.push(deleteSuccessCompany.companyId);

  const deleteSuccess = await runRequired(
    contactDeleteDocument,
    { companyContactId: deleteSuccessCompany.contactId },
    'companyContactDelete',
    'companyContactDelete success',
  );

  const readAfterDeleteSuccess = await runRequired(
    companyReadDocument,
    {
      companyId: deleteSuccessCompany.companyId,
      companyContactId: deleteSuccessCompany.contactId,
    },
    'company',
    'read after successful companyContactDelete',
  );

  const ordersCompanyCreate = await runRequired(
    companyCreateDocument,
    companyCreateVariables('orders-company'),
    'companyCreate',
    'orders companyCreate',
  );
  const ordersCompany = companyIdsFromCreate(ordersCompanyCreate);
  createdCompanyIds.push(ordersCompany.companyId);

  const draftOrderCreate = await runRequired(
    draftOrderCreateDocument,
    draftOrderVariables(ordersCompany.companyId, ordersCompany.contactId, ordersCompany.locationId),
    'draftOrderCreate',
    'draftOrderCreate for B2B contact',
  );
  const draftOrderId = readStringAtPath(
    draftOrderCreate.response,
    ['data', 'draftOrderCreate', 'draftOrder', 'id'],
    'draftOrderCreate id',
  );

  const draftOrderComplete = await runRequired(
    draftOrderCompleteDocument,
    { id: draftOrderId, paymentPending: true },
    'draftOrderComplete',
    'draftOrderComplete for B2B contact',
  );
  completedOrderId = readStringAtPath(
    draftOrderComplete.response,
    ['data', 'draftOrderComplete', 'draftOrder', 'order', 'id'],
    'draftOrderComplete order id',
  );

  const deleteWithOrders = await runOperation(contactDeleteDocument, { companyContactId: ordersCompany.contactId });
  assertHttpGraphqlOk(
    { status: deleteWithOrders.response.status as number, payload: deleteWithOrders.response },
    'delete contact with orders',
  );
  assertUserError(
    deleteWithOrders,
    'companyContactDelete',
    {
      field: ['companyContactId'],
      message: 'Cannot delete a company contact with existing orders or draft orders.',
      code: 'FAILED_TO_DELETE',
    },
    'companyContactDelete with orders',
  );

  const readAfterDeleteWithOrders = await runRequired(
    companyReadDocument,
    {
      companyId: ordersCompany.companyId,
      companyContactId: ordersCompany.contactId,
    },
    'company',
    'read after rejected companyContactDelete',
  );

  cleanup['orderCancel'] = await runCleanup(orderCancelDocument, {
    orderId: completedOrderId,
    reason: 'OTHER',
    notifyCustomer: false,
    restock: false,
  });

  for (const [index, companyId] of createdCompanyIds.entries()) {
    cleanup[`companyDelete${index + 1}`] = await runCleanup(companyDeleteDocument, { id: companyId });
  }

  const output = {
    scenarioId,
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    intent: {
      ticket: 'HAR-620',
      plan: 'Record B2B contact delete/order-history and contact role assignment business-rule preconditions against a disposable Shopify dev store.',
    },
    roleCompanyCreate,
    duplicateAssign,
    foreignCompanyCreate,
    foreignRoleAssign,
    foreignLocationAssign,
    missingRoleAssign,
    missingLocationAssign,
    deleteSuccessCompanyCreate,
    deleteSuccess,
    readAfterDeleteSuccess,
    ordersCompanyCreate,
    draftOrderCreate,
    draftOrderComplete,
    deleteWithOrders,
    readAfterDeleteWithOrders,
    cleanup,
    upstreamCalls: [],
  };

  const outputPath = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'b2b', `${scenarioId}.json`);
  await mkdir(path.dirname(outputPath), { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');

  console.log(JSON.stringify({ ok: true, outputPath }, null, 2));
} finally {
  if (completedOrderId && !cleanup['orderCancel']) {
    cleanup['orderCancel'] = await runCleanup(orderCancelDocument, {
      orderId: completedOrderId,
      reason: 'OTHER',
      notifyCustomer: false,
      restock: false,
    });
  }

  for (const [index, companyId] of createdCompanyIds.entries()) {
    const cleanupKey = `companyDelete${index + 1}`;
    if (!cleanup[cleanupKey]) {
      cleanup[cleanupKey] = await runCleanup(companyDeleteDocument, { id: companyId });
    }
  }
}

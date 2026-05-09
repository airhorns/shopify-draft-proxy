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

const scenarioId = 'company-delete-failed-deletable-check';
const timestamp = Date.now();
const runKey = `b2b-company-delete-check-${timestamp}`;
const missingCompanyId = 'gid://shopify/Company/999999999999999';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const companyCreateDocument = `#graphql
  mutation B2BCompanyDeleteCheckCompanyCreate($input: CompanyCreateInput!) {
    companyCreate(input: $input) {
      company {
        id
        name
        mainContact {
          id
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

const companyDeleteDocument = `#graphql
  mutation B2BCompanyDeleteCheckCompanyDelete($id: ID!) {
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

const companiesDeleteDocument = `#graphql
  mutation B2BCompanyDeleteCheckCompaniesDelete($companyIds: [ID!]!) {
    companiesDelete(companyIds: $companyIds) {
      deletedCompanyIds
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const singleCompanyReadDocument = `#graphql
  query B2BCompanyDeleteCheckSingleRead($companyId: ID!) {
    company(id: $companyId) {
      id
      name
      locations(first: 5) {
        nodes {
          id
          name
        }
      }
    }
  }
`;

const bulkCompanyReadDocument = `#graphql
  query B2BCompanyDeleteCheckBulkRead(
    $orderCompanyId: ID!
    $draftOrderCompanyId: ID!
    $storeCreditCompanyId: ID!
    $deletedCompanyId: ID!
  ) {
    orderBlocked: company(id: $orderCompanyId) {
      id
      name
    }
    draftOrderBlocked: company(id: $draftOrderCompanyId) {
      id
      name
    }
    storeCreditBlocked: company(id: $storeCreditCompanyId) {
      id
      name
    }
    deleted: company(id: $deletedCompanyId) {
      id
      name
    }
  }
`;

const draftOrderCreateDocument = `#graphql
  mutation B2BCompanyDeleteCheckDraftOrderCreate($input: DraftOrderInput!) {
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
  mutation B2BCompanyDeleteCheckDraftOrderComplete($id: ID!, $paymentPending: Boolean!) {
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

const draftOrderDeleteDocument = `#graphql
  mutation B2BCompanyDeleteCheckDraftOrderDelete($input: DraftOrderDeleteInput!) {
    draftOrderDelete(input: $input) {
      deletedId
      userErrors {
        field
        message
      }
    }
  }
`;

const orderCancelDocument = `#graphql
  mutation B2BCompanyDeleteCheckOrderCancel(
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

const orderDeleteDocument = `#graphql
  mutation B2BCompanyDeleteCheckOrderDelete($orderId: ID!) {
    orderDelete(orderId: $orderId) {
      deletedId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const storeCreditAccountCreditDocument = `#graphql
  mutation B2BCompanyDeleteCheckStoreCreditCredit($id: ID!, $creditInput: StoreCreditAccountCreditInput!) {
    storeCreditAccountCredit(id: $id, creditInput: $creditInput) {
      storeCreditAccountTransaction {
        amount {
          amount
          currencyCode
        }
        balanceAfterTransaction {
          amount
          currencyCode
        }
        event
        origin
        account {
          id
          balance {
            amount
            currencyCode
          }
          owner {
            ... on CompanyLocation {
              id
            }
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

const storeCreditAccountDebitDocument = `#graphql
  mutation B2BCompanyDeleteCheckStoreCreditDebit($id: ID!, $debitInput: StoreCreditAccountDebitInput!) {
    storeCreditAccountDebit(id: $id, debitInput: $debitInput) {
      storeCreditAccountTransaction {
        amount {
          amount
          currencyCode
        }
        balanceAfterTransaction {
          amount
          currencyCode
        }
        account {
          id
          balance {
            amount
            currencyCode
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

function readUserErrors(payload: unknown, root: string): JsonRecord[] {
  const value = readPath(payload, ['data', root, 'userErrors']);
  return Array.isArray(value) ? value.filter((item): item is JsonRecord => readRecord(item) !== null) : [];
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
  expected: { field: string[] | null; code: string; messageIncludes: string },
  label: string,
): void {
  const userErrors = readUserErrors(operation.response, root);
  const matchingError = userErrors.find((error) => {
    const message = error['message'];
    return (
      JSON.stringify(error['field'] ?? null) === JSON.stringify(expected.field) &&
      error['code'] === expected.code &&
      typeof message === 'string' &&
      message.includes(expected.messageIncludes)
    );
  });

  if (!matchingError) {
    throw new Error(
      `${label} did not return expected userError ${JSON.stringify(expected)}: ${JSON.stringify(userErrors, null, 2)}`,
    );
  }
}

function assertNullAtPath(operation: RecordedOperation, pathSegments: string[], label: string): void {
  const value = readPath(operation.response, pathSegments);
  if (value !== null) {
    throw new Error(
      `${label} expected null at ${pathSegments.join('.')}: ${JSON.stringify(operation.response, null, 2)}`,
    );
  }
}

function assertDeletedIds(operation: RecordedOperation, expectedIds: string[], label: string): void {
  const value = readPath(operation.response, ['data', 'companiesDelete', 'deletedCompanyIds']);
  if (JSON.stringify(value) !== JSON.stringify(expectedIds)) {
    throw new Error(`${label} unexpected deletedCompanyIds: ${JSON.stringify(operation.response, null, 2)}`);
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

function companyCreateVariables(label: string, withContact = false): JsonRecord {
  const input: JsonRecord = {
    company: {
      name: `B2B Company Delete Check ${label} ${timestamp}`,
      note: `B2B company delete deletable-check ${label}`,
      externalId: `${runKey}-${label}`,
    },
    companyLocation: {
      name: `${label} HQ`,
      phone: '+16135550814',
      billingAddress: {
        address1: '814 B2B Way',
        city: 'Ottawa',
        countryCode: 'CA',
      },
    },
  };

  if (withContact) {
    input['companyContact'] = {
      firstName: 'Company',
      lastName: 'Delete',
      email: `${runKey}-${label}@example.com`,
      title: 'Buyer',
    };
  }

  return { input };
}

function draftOrderVariables(
  companyId: string,
  companyContactId: string,
  companyLocationId: string,
  label: string,
): JsonRecord {
  return {
    input: {
      purchasingEntity: {
        purchasingCompany: {
          companyId,
          companyContactId,
          companyLocationId,
        },
      },
      email: `${runKey}-${label}@example.com`,
      note: `B2B company delete ${label} precondition`,
      tags: ['b2b-company-delete-check'],
      visibleToCustomer: false,
      lineItems: [
        {
          title: `B2B company delete ${label} custom item`,
          quantity: 1,
          originalUnitPrice: '1.00',
          requiresShipping: false,
          taxable: false,
        },
      ],
    },
  };
}

function companyIdsFromCreate(
  operation: RecordedOperation,
  label: string,
): {
  companyId: string;
  locationId: string;
  contactId: string | null;
} {
  const contactId = readPath(operation.response, ['data', 'companyCreate', 'company', 'mainContact', 'id']);
  return {
    companyId: readStringAtPath(operation.response, ['data', 'companyCreate', 'company', 'id'], `${label} company id`),
    locationId: readStringAtPath(
      operation.response,
      ['data', 'companyCreate', 'company', 'locations', 'nodes', '0', 'id'],
      `${label} location id`,
    ),
    contactId: typeof contactId === 'string' && contactId.length > 0 ? contactId : null,
  };
}

const createdCompanyIds: string[] = [];
const draftOrderIds: string[] = [];
const completedOrderIds: string[] = [];
const storeCreditAccounts: Array<{ id: string; amount: string }> = [];
const cleanup: Record<string, RecordedOperation> = {};

try {
  const singleOrderCompanyCreate = await runRequired(
    companyCreateDocument,
    companyCreateVariables('single-order', true),
    'companyCreate',
    'single order companyCreate',
  );
  const singleOrderCompany = companyIdsFromCreate(singleOrderCompanyCreate, 'single order');
  if (!singleOrderCompany.contactId) {
    throw new Error(
      `single order companyCreate did not return a main contact: ${JSON.stringify(singleOrderCompanyCreate)}`,
    );
  }
  createdCompanyIds.push(singleOrderCompany.companyId);

  const singleOrderDraftOrderCreate = await runRequired(
    draftOrderCreateDocument,
    draftOrderVariables(
      singleOrderCompany.companyId,
      singleOrderCompany.contactId,
      singleOrderCompany.locationId,
      'single-order',
    ),
    'draftOrderCreate',
    'single order draftOrderCreate',
  );
  const singleOrderDraftOrderId = readStringAtPath(
    singleOrderDraftOrderCreate.response,
    ['data', 'draftOrderCreate', 'draftOrder', 'id'],
    'single order draftOrderCreate id',
  );

  const singleOrderDraftOrderComplete = await runRequired(
    draftOrderCompleteDocument,
    { id: singleOrderDraftOrderId, paymentPending: true },
    'draftOrderComplete',
    'single order draftOrderComplete',
  );
  const singleOrderId = readStringAtPath(
    singleOrderDraftOrderComplete.response,
    ['data', 'draftOrderComplete', 'draftOrder', 'order', 'id'],
    'single order id',
  );
  completedOrderIds.push(singleOrderId);

  const singleOrderCompanyDelete = await runOperation(companyDeleteDocument, {
    id: singleOrderCompany.companyId,
  });
  assertUserError(
    singleOrderCompanyDelete,
    'companyDelete',
    {
      field: ['id'],
      code: 'FAILED_TO_DELETE',
      messageIncludes: 'Failed to delete the company.',
    },
    'single order companyDelete',
  );
  assertNullAtPath(
    singleOrderCompanyDelete,
    ['data', 'companyDelete', 'deletedCompanyId'],
    'single order companyDelete',
  );

  const readAfterSingleOrderCompanyDelete = await runRequired(
    singleCompanyReadDocument,
    { companyId: singleOrderCompany.companyId },
    'company',
    'read after single order companyDelete',
  );

  const singleDraftOrderCompanyCreate = await runRequired(
    companyCreateDocument,
    companyCreateVariables('single-draft-order', true),
    'companyCreate',
    'single draft-order companyCreate',
  );
  const singleDraftOrderCompany = companyIdsFromCreate(singleDraftOrderCompanyCreate, 'single draft-order');
  if (!singleDraftOrderCompany.contactId) {
    throw new Error(
      `single draft-order companyCreate did not return a main contact: ${JSON.stringify(singleDraftOrderCompanyCreate)}`,
    );
  }
  createdCompanyIds.push(singleDraftOrderCompany.companyId);

  const singleDraftOrderCreate = await runRequired(
    draftOrderCreateDocument,
    draftOrderVariables(
      singleDraftOrderCompany.companyId,
      singleDraftOrderCompany.contactId,
      singleDraftOrderCompany.locationId,
      'single-draft-order',
    ),
    'draftOrderCreate',
    'single draft-order draftOrderCreate',
  );
  const singleDraftOrderId = readStringAtPath(
    singleDraftOrderCreate.response,
    ['data', 'draftOrderCreate', 'draftOrder', 'id'],
    'single draftOrderCreate id',
  );
  draftOrderIds.push(singleDraftOrderId);

  const singleDraftOrderCompanyDelete = await runOperation(companyDeleteDocument, {
    id: singleDraftOrderCompany.companyId,
  });
  assertUserError(
    singleDraftOrderCompanyDelete,
    'companyDelete',
    {
      field: ['id'],
      code: 'FAILED_TO_DELETE',
      messageIncludes: 'Failed to delete the company.',
    },
    'single draft-order companyDelete',
  );
  assertNullAtPath(
    singleDraftOrderCompanyDelete,
    ['data', 'companyDelete', 'deletedCompanyId'],
    'single draft-order companyDelete',
  );

  const readAfterSingleDraftOrderCompanyDelete = await runRequired(
    singleCompanyReadDocument,
    { companyId: singleDraftOrderCompany.companyId },
    'company',
    'read after single draft-order companyDelete',
  );

  const singleStoreCreditCompanyCreate = await runRequired(
    companyCreateDocument,
    companyCreateVariables('single-store-credit'),
    'companyCreate',
    'single store-credit companyCreate',
  );
  const singleStoreCreditCompany = companyIdsFromCreate(singleStoreCreditCompanyCreate, 'single store-credit');
  createdCompanyIds.push(singleStoreCreditCompany.companyId);

  const singleStoreCreditAccountCredit = await runRequired(
    storeCreditAccountCreditDocument,
    {
      id: singleStoreCreditCompany.locationId,
      creditInput: {
        creditAmount: {
          amount: '7.00',
          currencyCode: 'USD',
        },
      },
    },
    'storeCreditAccountCredit',
    'single store-credit account credit',
  );
  storeCreditAccounts.push({
    id: readStringAtPath(
      singleStoreCreditAccountCredit.response,
      ['data', 'storeCreditAccountCredit', 'storeCreditAccountTransaction', 'account', 'id'],
      'single store-credit account id',
    ),
    amount: readStringAtPath(
      singleStoreCreditAccountCredit.response,
      ['data', 'storeCreditAccountCredit', 'storeCreditAccountTransaction', 'balanceAfterTransaction', 'amount'],
      'single store-credit cleanup amount',
    ),
  });

  const singleStoreCreditCompanyDelete = await runOperation(companyDeleteDocument, {
    id: singleStoreCreditCompany.companyId,
  });
  assertUserError(
    singleStoreCreditCompanyDelete,
    'companyDelete',
    {
      field: ['id'],
      code: 'FAILED_TO_DELETE',
      messageIncludes: 'Failed to delete the company.',
    },
    'single store-credit companyDelete',
  );
  assertNullAtPath(
    singleStoreCreditCompanyDelete,
    ['data', 'companyDelete', 'deletedCompanyId'],
    'single store-credit companyDelete',
  );

  const readAfterSingleStoreCreditCompanyDelete = await runRequired(
    singleCompanyReadDocument,
    { companyId: singleStoreCreditCompany.companyId },
    'company',
    'read after single store-credit companyDelete',
  );

  const bulkOrderCompanyCreate = await runRequired(
    companyCreateDocument,
    companyCreateVariables('bulk-order', true),
    'companyCreate',
    'bulk order companyCreate',
  );
  const bulkOrderCompany = companyIdsFromCreate(bulkOrderCompanyCreate, 'bulk order');
  if (!bulkOrderCompany.contactId) {
    throw new Error(
      `bulk order companyCreate did not return a main contact: ${JSON.stringify(bulkOrderCompanyCreate)}`,
    );
  }
  createdCompanyIds.push(bulkOrderCompany.companyId);

  const bulkOrderDraftOrderCreate = await runRequired(
    draftOrderCreateDocument,
    draftOrderVariables(
      bulkOrderCompany.companyId,
      bulkOrderCompany.contactId,
      bulkOrderCompany.locationId,
      'bulk-order',
    ),
    'draftOrderCreate',
    'bulk order draftOrderCreate',
  );
  const bulkOrderDraftOrderId = readStringAtPath(
    bulkOrderDraftOrderCreate.response,
    ['data', 'draftOrderCreate', 'draftOrder', 'id'],
    'bulk order draftOrderCreate id',
  );

  const bulkOrderDraftOrderComplete = await runRequired(
    draftOrderCompleteDocument,
    { id: bulkOrderDraftOrderId, paymentPending: true },
    'draftOrderComplete',
    'bulk order draftOrderComplete',
  );
  completedOrderIds.push(
    readStringAtPath(
      bulkOrderDraftOrderComplete.response,
      ['data', 'draftOrderComplete', 'draftOrder', 'order', 'id'],
      'bulk order id',
    ),
  );

  const bulkDraftOrderCompanyCreate = await runRequired(
    companyCreateDocument,
    companyCreateVariables('bulk-draft-order', true),
    'companyCreate',
    'bulk draft-order companyCreate',
  );
  const bulkDraftOrderCompany = companyIdsFromCreate(bulkDraftOrderCompanyCreate, 'bulk draft-order');
  if (!bulkDraftOrderCompany.contactId) {
    throw new Error(
      `bulk draft-order companyCreate did not return a main contact: ${JSON.stringify(bulkDraftOrderCompanyCreate)}`,
    );
  }
  createdCompanyIds.push(bulkDraftOrderCompany.companyId);

  const bulkDraftOrderCreate = await runRequired(
    draftOrderCreateDocument,
    draftOrderVariables(
      bulkDraftOrderCompany.companyId,
      bulkDraftOrderCompany.contactId,
      bulkDraftOrderCompany.locationId,
      'bulk-draft-order',
    ),
    'draftOrderCreate',
    'bulk draft-order draftOrderCreate',
  );
  draftOrderIds.push(
    readStringAtPath(
      bulkDraftOrderCreate.response,
      ['data', 'draftOrderCreate', 'draftOrder', 'id'],
      'bulk draftOrderCreate id',
    ),
  );

  const bulkStoreCreditCompanyCreate = await runRequired(
    companyCreateDocument,
    companyCreateVariables('bulk-store-credit'),
    'companyCreate',
    'bulk store-credit companyCreate',
  );
  const bulkStoreCreditCompany = companyIdsFromCreate(bulkStoreCreditCompanyCreate, 'bulk store-credit');
  createdCompanyIds.push(bulkStoreCreditCompany.companyId);

  const bulkStoreCreditAccountCredit = await runRequired(
    storeCreditAccountCreditDocument,
    {
      id: bulkStoreCreditCompany.locationId,
      creditInput: {
        creditAmount: {
          amount: '8.00',
          currencyCode: 'USD',
        },
      },
    },
    'storeCreditAccountCredit',
    'bulk store-credit account credit',
  );
  storeCreditAccounts.push({
    id: readStringAtPath(
      bulkStoreCreditAccountCredit.response,
      ['data', 'storeCreditAccountCredit', 'storeCreditAccountTransaction', 'account', 'id'],
      'bulk store-credit account id',
    ),
    amount: readStringAtPath(
      bulkStoreCreditAccountCredit.response,
      ['data', 'storeCreditAccountCredit', 'storeCreditAccountTransaction', 'balanceAfterTransaction', 'amount'],
      'bulk store-credit cleanup amount',
    ),
  });

  const bulkNormalCompanyCreate = await runRequired(
    companyCreateDocument,
    companyCreateVariables('bulk-normal'),
    'companyCreate',
    'bulk normal companyCreate',
  );
  const bulkNormalCompany = companyIdsFromCreate(bulkNormalCompanyCreate, 'bulk normal');
  createdCompanyIds.push(bulkNormalCompany.companyId);

  const bulkCompaniesDelete = await runOperation(companiesDeleteDocument, {
    companyIds: [
      bulkOrderCompany.companyId,
      bulkDraftOrderCompany.companyId,
      bulkStoreCreditCompany.companyId,
      bulkNormalCompany.companyId,
      missingCompanyId,
    ],
  });
  assertDeletedIds(bulkCompaniesDelete, [bulkNormalCompany.companyId], 'bulk companiesDelete');
  assertUserError(
    bulkCompaniesDelete,
    'companiesDelete',
    {
      field: ['companyIds', '0'],
      code: 'FAILED_TO_DELETE',
      messageIncludes: 'Failed to delete the company.',
    },
    'bulk order companiesDelete',
  );
  assertUserError(
    bulkCompaniesDelete,
    'companiesDelete',
    {
      field: ['companyIds', '1'],
      code: 'FAILED_TO_DELETE',
      messageIncludes: 'Failed to delete the company.',
    },
    'bulk draft-order companiesDelete',
  );
  assertUserError(
    bulkCompaniesDelete,
    'companiesDelete',
    {
      field: ['companyIds', '2'],
      code: 'FAILED_TO_DELETE',
      messageIncludes: 'Failed to delete the company.',
    },
    'bulk store-credit companiesDelete',
  );
  assertUserError(
    bulkCompaniesDelete,
    'companiesDelete',
    {
      field: ['companyIds', '4'],
      code: 'RESOURCE_NOT_FOUND',
      messageIncludes: 'Resource requested does not exist.',
    },
    'bulk missing companiesDelete',
  );

  const readAfterBulkCompaniesDelete = await runRequired(
    bulkCompanyReadDocument,
    {
      orderCompanyId: bulkOrderCompany.companyId,
      draftOrderCompanyId: bulkDraftOrderCompany.companyId,
      storeCreditCompanyId: bulkStoreCreditCompany.companyId,
      deletedCompanyId: bulkNormalCompany.companyId,
    },
    'orderBlocked',
    'read after bulk companiesDelete',
  );
  assertNullAtPath(readAfterBulkCompaniesDelete, ['data', 'deleted'], 'read after bulk companiesDelete');

  for (const draftOrderId of draftOrderIds) {
    cleanup[`draftOrderDelete${draftOrderId}`] = await runCleanup(draftOrderDeleteDocument, {
      input: { id: draftOrderId },
    });
  }

  for (const account of storeCreditAccounts) {
    cleanup[`storeCreditAccountDebit${account.id}`] = await runCleanup(storeCreditAccountDebitDocument, {
      id: account.id,
      debitInput: {
        debitAmount: {
          amount: account.amount,
          currencyCode: 'USD',
        },
      },
    });
  }

  for (const orderId of completedOrderIds) {
    cleanup[`orderCancel${orderId}`] = await runCleanup(orderCancelDocument, {
      orderId,
      reason: 'OTHER',
      notifyCustomer: false,
      restock: false,
    });
    cleanup[`orderDelete${orderId}`] = await runCleanup(orderDeleteDocument, { orderId });
  }

  for (const [index, companyId] of createdCompanyIds.entries()) {
    cleanup[`companyDelete${index + 1}`] = await runCleanup(companyDeleteDocument, { id: companyId });
  }

  const output = {
    scenarioId,
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    intent: {
      plan: 'Record B2B companyDelete and companiesDelete failed-deletable checks for order, draft-order, and store-credit blockers against a disposable Shopify dev store.',
      cleanup:
        'Draft orders are deleted, store-credit balances are debited to zero, completed orders are cancelled and deleted when Shopify accepts it, then all disposable companies are deleted. Shopify may retain completed-order company history, so order-blocked cleanup companyDelete responses are recorded.',
    },
    singleOrderCompanyCreate,
    singleOrderDraftOrderCreate,
    singleOrderDraftOrderComplete,
    singleOrderCompanyDelete,
    readAfterSingleOrderCompanyDelete,
    singleDraftOrderCompanyCreate,
    singleDraftOrderCreate,
    singleDraftOrderCompanyDelete,
    readAfterSingleDraftOrderCompanyDelete,
    singleStoreCreditCompanyCreate,
    singleStoreCreditAccountCredit,
    singleStoreCreditCompanyDelete,
    readAfterSingleStoreCreditCompanyDelete,
    bulkOrderCompanyCreate,
    bulkOrderDraftOrderCreate,
    bulkOrderDraftOrderComplete,
    bulkDraftOrderCompanyCreate,
    bulkDraftOrderCreate,
    bulkStoreCreditCompanyCreate,
    bulkStoreCreditAccountCredit,
    bulkNormalCompanyCreate,
    bulkCompaniesDelete,
    readAfterBulkCompaniesDelete,
    cleanup,
    upstreamCalls: [],
  };

  const outputPath = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'b2b', `${scenarioId}.json`);
  await mkdir(path.dirname(outputPath), { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');

  console.log(JSON.stringify({ ok: true, outputPath }, null, 2));
} finally {
  for (const draftOrderId of draftOrderIds) {
    const cleanupKey = `draftOrderDelete${draftOrderId}`;
    if (!cleanup[cleanupKey]) {
      cleanup[cleanupKey] = await runCleanup(draftOrderDeleteDocument, { input: { id: draftOrderId } });
    }
  }

  for (const account of storeCreditAccounts) {
    const cleanupKey = `storeCreditAccountDebit${account.id}`;
    if (!cleanup[cleanupKey]) {
      cleanup[cleanupKey] = await runCleanup(storeCreditAccountDebitDocument, {
        id: account.id,
        debitInput: {
          debitAmount: {
            amount: account.amount,
            currencyCode: 'USD',
          },
        },
      });
    }
  }

  for (const orderId of completedOrderIds) {
    const cancelKey = `orderCancel${orderId}`;
    if (!cleanup[cancelKey]) {
      cleanup[cancelKey] = await runCleanup(orderCancelDocument, {
        orderId,
        reason: 'OTHER',
        notifyCustomer: false,
        restock: false,
      });
    }
    const deleteKey = `orderDelete${orderId}`;
    if (!cleanup[deleteKey]) {
      cleanup[deleteKey] = await runCleanup(orderDeleteDocument, { orderId });
    }
  }

  for (const [index, companyId] of createdCompanyIds.entries()) {
    const cleanupKey = `companyDelete${index + 1}`;
    if (!cleanup[cleanupKey]) {
      cleanup[cleanupKey] = await runCleanup(companyDeleteDocument, { id: companyId });
    }
  }
}

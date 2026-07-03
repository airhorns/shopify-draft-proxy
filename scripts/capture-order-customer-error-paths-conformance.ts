/* oxlint-disable no-console -- CLI capture script intentionally writes progress to stdio. */
import 'dotenv/config';

import {
  createConformanceCapture,
  readArray,
  readRecord,
  requireString,
  type JsonRecord,
} from './conformance-capture-lib.js';

type RecordedOperation = {
  query: string;
  variables: JsonRecord;
  response: {
    status: number;
    body: JsonRecord;
  };
};

const capture = await createConformanceCapture();
const runKey = `order-customer-error-paths-${capture.stamp}`;
const cleanup: Record<string, unknown> = {};
const createdOrderIds: string[] = [];
const createdCustomerIds: string[] = [];
let companyId: string | null = null;

const customerCreateDocument = await capture.readRequest('orders', 'orderCustomer-error-paths-customer-create.graphql');
const companyCreateDocument = await capture.readRequest('b2b', 'b2b-contact-business-rules-company-create.graphql');
const assignCustomerDocument = await capture.readRequest(
  'b2b',
  'b2b-company-contact-main-delete-assign-customer.graphql',
);
const orderCreateDocument = await capture.readRequest('orders', 'orderCancel-state-transitions-order-create.graphql');
const orderCancelDocument = await capture.readRequest('orders', 'orderCancel-parity.graphql');
const b2bDraftOrderCreateDocument = await capture.readRequest(
  'b2b',
  'b2b-contact-business-rules-draft-order-create.graphql',
);
const b2bDraftOrderCompleteDocument = await capture.readRequest(
  'b2b',
  'b2b-contact-business-rules-draft-order-complete.graphql',
);
const orderCustomerSetDocument = await capture.readRequest('orders', 'orderCustomerSet-error-paths.graphql');
const orderCustomerRemoveDocument = await capture.readRequest('orders', 'orderCustomerRemove-error-paths.graphql');

const customerDeleteDocument = `#graphql
  mutation OrderCustomerErrorPathsCustomerDelete($input: CustomerDeleteInput!) {
    customerDelete(input: $input) {
      deletedCustomerId
      userErrors { field message }
    }
  }
`;

const companyDeleteDocument = `#graphql
  mutation OrderCustomerErrorPathsCompanyDelete($id: ID!) {
    companyDelete(id: $id) {
      deletedCompanyId
      userErrors { field message code }
    }
  }
`;

async function runOperation(query: string, variables: JsonRecord): Promise<RecordedOperation> {
  const result = await capture.runGraphqlRequest<JsonRecord>(query, variables);
  return {
    query: query.replace(/^#graphql\n/u, '').trim(),
    variables,
    response: {
      status: result.status,
      body: result.payload as JsonRecord,
    },
  };
}

function readPath(value: unknown, pathSegments: string[]): unknown {
  let current = value;
  for (const segment of pathSegments) {
    if (Array.isArray(current)) {
      current = current[Number(segment)];
      continue;
    }
    const record = readRecord(current);
    if (!record) return undefined;
    current = record[segment];
  }
  return current;
}

function userErrors(operation: RecordedOperation, root: string): unknown[] {
  return readArray(readPath(operation.response.body, ['data', root, 'userErrors']));
}

function assertNoUserErrors(operation: RecordedOperation, root: string, label: string): void {
  if (operation.response.status < 200 || operation.response.status >= 300 || operation.response.body['errors']) {
    throw new Error(`${label} failed: ${JSON.stringify(operation.response.body, null, 2)}`);
  }
  const errors = userErrors(operation, root);
  if (errors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(errors, null, 2)}`);
  }
}

function assertNoOrderCancelUserErrors(operation: RecordedOperation, label: string): void {
  if (operation.response.status < 200 || operation.response.status >= 300 || operation.response.body['errors']) {
    throw new Error(`${label} failed: ${JSON.stringify(operation.response.body, null, 2)}`);
  }
  const errors = readArray(readPath(operation.response.body, ['data', 'orderCancel', 'orderCancelUserErrors']));
  if (errors.length > 0) {
    throw new Error(`${label} returned orderCancelUserErrors: ${JSON.stringify(errors, null, 2)}`);
  }
}

function assertUserErrors(operation: RecordedOperation, root: string, expected: unknown[], label: string): void {
  if (operation.response.status < 200 || operation.response.status >= 300 || operation.response.body['errors']) {
    throw new Error(`${label} failed: ${JSON.stringify(operation.response.body, null, 2)}`);
  }
  const actual = userErrors(operation, root);
  if (JSON.stringify(actual) !== JSON.stringify(expected)) {
    throw new Error(
      `${label} userErrors mismatch: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`,
    );
  }
}

async function cleanupOrder(orderId: string): Promise<unknown> {
  return await runOperation(orderCancelDocument, {
    orderId,
    restock: false,
    reason: 'OTHER',
  });
}

async function cleanupCustomer(customerId: string): Promise<unknown> {
  return await runOperation(customerDeleteDocument, { input: { id: customerId } });
}

async function cleanupCompany(id: string): Promise<unknown> {
  return await runOperation(companyDeleteDocument, { id });
}

try {
  const noRoleCustomerCreate = await runOperation(customerCreateDocument, {
    input: {
      email: `buyer-alpha-${capture.stamp}@example.com`,
      firstName: 'Avery',
      lastName: 'Atlas',
    },
  });
  assertNoUserErrors(noRoleCustomerCreate, 'customerCreate', 'no-role customerCreate');
  const noRoleCustomerId = requireString(
    readPath(noRoleCustomerCreate.response.body, ['data', 'customerCreate', 'customer', 'id']),
    'no-role customer id',
  );
  createdCustomerIds.push(noRoleCustomerId);

  const removableCustomerCreate = await runOperation(customerCreateDocument, {
    input: {
      email: `buyer-beta-${capture.stamp}@example.com`,
      firstName: 'Blair',
      lastName: 'Benton',
    },
  });
  assertNoUserErrors(removableCustomerCreate, 'customerCreate', 'removable customerCreate');
  const removableCustomerId = requireString(
    readPath(removableCustomerCreate.response.body, ['data', 'customerCreate', 'customer', 'id']),
    'removable customer id',
  );
  createdCustomerIds.push(removableCustomerId);

  const companyCreate = await runOperation(companyCreateDocument, {
    input: {
      company: {
        name: `Atlas Procurement ${capture.stamp}`,
        note: 'Varied order-customer error-path conformance setup',
        externalId: runKey,
      },
      companyContact: {
        firstName: 'Casey',
        lastName: 'Contact',
        email: `atlas-procurement-${capture.stamp}-main@example.com`,
        title: 'Buyer',
      },
      companyLocation: {
        name: `Atlas Procurement HQ ${capture.stamp}`,
        billingAddress: {
          address1: '1 Error Path Way',
          city: 'Ottawa',
          countryCode: 'CA',
        },
      },
    },
  });
  assertNoUserErrors(companyCreate, 'companyCreate', 'companyCreate');
  companyId = requireString(
    readPath(companyCreate.response.body, ['data', 'companyCreate', 'company', 'id']),
    'company id',
  );
  const companyContactId = requireString(
    readPath(companyCreate.response.body, ['data', 'companyCreate', 'company', 'mainContact', 'id']),
    'company main contact id',
  );
  const companyLocationId = requireString(
    readPath(companyCreate.response.body, ['data', 'companyCreate', 'company', 'locations', 'nodes', '0', 'id']),
    'company location id',
  );

  const assignNoRoleContact = await runOperation(assignCustomerDocument, {
    companyId,
    customerId: noRoleCustomerId,
  });
  assertNoUserErrors(assignNoRoleContact, 'companyAssignCustomerAsContact', 'companyAssignCustomerAsContact');

  const happyOrderCreate = await runOperation(orderCreateDocument, {
    order: {
      email: `customer-set-happy-${capture.stamp}@example.com`,
      test: true,
      currency: 'USD',
      financialStatus: 'PENDING',
      lineItems: [
        {
          title: 'Order customer happy item',
          quantity: 1,
          priceSet: {
            shopMoney: {
              amount: '10.00',
              currencyCode: 'USD',
            },
          },
          requiresShipping: false,
          taxable: false,
        },
      ],
    },
  });
  assertNoUserErrors(happyOrderCreate, 'orderCreate', 'happy orderCreate');
  const happyOrderId = requireString(
    readPath(happyOrderCreate.response.body, ['data', 'orderCreate', 'order', 'id']),
    'happy order id',
  );
  createdOrderIds.push(happyOrderId);

  const happySet = await runOperation(orderCustomerSetDocument, {
    orderId: happyOrderId,
    customerId: removableCustomerId,
  });
  assertNoUserErrors(happySet, 'orderCustomerSet', 'orderCustomerSet happy path');

  const happyRemove = await runOperation(orderCustomerRemoveDocument, {
    orderId: happyOrderId,
  });
  assertNoUserErrors(happyRemove, 'orderCustomerRemove', 'orderCustomerRemove happy path');

  const unknownOrder = await runOperation(orderCustomerSetDocument, {
    orderId: 'gid://shopify/Order/999999999999999',
    customerId: removableCustomerId,
  });
  assertUserErrors(
    unknownOrder,
    'orderCustomerSet',
    [
      {
        field: ['orderId'],
        message: 'Order does not exist',
        code: 'NOT_FOUND',
      },
    ],
    'orderCustomerSet unknown order',
  );

  const unknownCustomer = await runOperation(orderCustomerSetDocument, {
    orderId: happyOrderId,
    customerId: 'gid://shopify/Customer/999999999999999',
  });
  assertUserErrors(
    unknownCustomer,
    'orderCustomerSet',
    [
      {
        field: ['customerId'],
        message: 'Customer does not exist',
        code: 'NOT_FOUND',
      },
    ],
    'orderCustomerSet unknown customer',
  );

  const b2bDraftOrderCreate = await runOperation(b2bDraftOrderCreateDocument, {
    input: {
      email: `b2b-purchase-${capture.stamp}@example.com`,
      purchasingEntity: {
        purchasingCompany: {
          companyId,
          companyContactId,
          companyLocationId,
        },
      },
      lineItems: [
        {
          title: 'Order customer B2B item',
          quantity: 1,
          originalUnitPrice: '10.00',
          requiresShipping: false,
          taxable: false,
        },
      ],
    },
  });
  assertNoUserErrors(b2bDraftOrderCreate, 'draftOrderCreate', 'B2B draftOrderCreate');
  const b2bDraftOrderId = requireString(
    readPath(b2bDraftOrderCreate.response.body, ['data', 'draftOrderCreate', 'draftOrder', 'id']),
    'B2B draft order id',
  );

  const b2bDraftOrderComplete = await runOperation(b2bDraftOrderCompleteDocument, {
    id: b2bDraftOrderId,
    paymentPending: true,
  });
  assertNoUserErrors(b2bDraftOrderComplete, 'draftOrderComplete', 'B2B draftOrderComplete');
  const b2bOrderId = requireString(
    readPath(b2bDraftOrderComplete.response.body, ['data', 'draftOrderComplete', 'draftOrder', 'order', 'id']),
    'B2B order id',
  );
  createdOrderIds.push(b2bOrderId);

  const setB2bNoRole = await runOperation(orderCustomerSetDocument, {
    orderId: b2bOrderId,
    customerId: noRoleCustomerId,
  });
  assertUserErrors(
    setB2bNoRole,
    'orderCustomerSet',
    [
      {
        field: ['customerId'],
        message: 'Customer does not have the permissions to place this order',
        code: 'NOT_PERMITTED',
      },
    ],
    'orderCustomerSet B2B no-role',
  );

  const removeB2bOrder = await runOperation(orderCustomerRemoveDocument, {
    orderId: b2bOrderId,
  });
  assertUserErrors(
    removeB2bOrder,
    'orderCustomerRemove',
    [
      {
        field: ['orderId'],
        message: 'Action not permitted on B2B Orders',
        code: 'INVALID',
      },
    ],
    'orderCustomerRemove B2B order',
  );

  const cancelledOrderCreate = await runOperation(orderCreateDocument, {
    order: {
      email: `customer-set-cancelled-${capture.stamp}@example.com`,
      test: true,
      currency: 'USD',
      financialStatus: 'PENDING',
      customerId: removableCustomerId,
      lineItems: [
        {
          title: 'Order customer cancelled item',
          quantity: 1,
          priceSet: {
            shopMoney: {
              amount: '10.00',
              currencyCode: 'USD',
            },
          },
          requiresShipping: false,
          taxable: false,
        },
      ],
    },
  });
  assertNoUserErrors(cancelledOrderCreate, 'orderCreate', 'cancelled orderCreate');
  const cancelledOrderId = requireString(
    readPath(cancelledOrderCreate.response.body, ['data', 'orderCreate', 'order', 'id']),
    'cancelled order id',
  );
  createdOrderIds.push(cancelledOrderId);

  const cancelOrder = await runOperation(orderCancelDocument, {
    orderId: cancelledOrderId,
    restock: false,
    reason: 'OTHER',
  });
  assertNoOrderCancelUserErrors(cancelOrder, 'orderCancel setup');

  const removeCancelledOrder = await runOperation(orderCustomerRemoveDocument, {
    orderId: cancelledOrderId,
  });
  assertNoUserErrors(removeCancelledOrder, 'orderCustomerRemove', 'orderCustomerRemove cancelled order');

  await capture.writeJson(capture.fixturePath('orders', 'orderCustomerSet-and-Remove-error-paths.json'), {
    scenarioId: 'orderCustomerSet-and-Remove-error-paths',
    recordedAt: new Date().toISOString(),
    source: 'shopify',
    storeDomain: capture.storeDomain,
    apiVersion: capture.apiVersion,
    setup: {
      noRoleCustomerCreate,
      removableCustomerCreate,
      companyCreate,
      assignNoRoleContact,
      happyOrderCreate,
      b2bDraftOrderCreate,
      b2bDraftOrderComplete,
      cancelledOrderCreate,
      cancelOrder,
    },
    cases: {
      happySet,
      happyRemove,
      unknownOrder,
      unknownCustomer,
      setB2bNoRole,
      removeB2bOrder,
      removeCancelledOrder,
    },
    upstreamCalls: [],
  });
} finally {
  for (const orderId of createdOrderIds) {
    try {
      cleanup[`order:${orderId}`] = await cleanupOrder(orderId);
    } catch (error) {
      cleanup[`order:${orderId}`] = { error: error instanceof Error ? error.message : String(error) };
    }
  }
  for (const customerId of createdCustomerIds) {
    try {
      cleanup[`customer:${customerId}`] = await cleanupCustomer(customerId);
    } catch (error) {
      cleanup[`customer:${customerId}`] = { error: error instanceof Error ? error.message : String(error) };
    }
  }
  if (companyId) {
    try {
      cleanup[`company:${companyId}`] = await cleanupCompany(companyId);
    } catch (error) {
      cleanup[`company:${companyId}`] = { error: error instanceof Error ? error.message : String(error) };
    }
  }
  if (createdOrderIds.length > 0 || createdCustomerIds.length > 0 || companyId) {
    await capture.writeJson(capture.fixturePath('orders', 'orderCustomerSet-and-Remove-error-paths-cleanup.json'), {
      capturedAt: new Date().toISOString(),
      storeDomain: capture.storeDomain,
      apiVersion: capture.apiVersion,
      cleanup,
    });
  }
}

console.log(
  JSON.stringify(
    {
      ok: true,
      fixture: capture.fixturePath('orders', 'orderCustomerSet-and-Remove-error-paths.json'),
      cleanup: capture.fixturePath('orders', 'orderCustomerSet-and-Remove-error-paths-cleanup.json'),
    },
    null,
    2,
  ),
);

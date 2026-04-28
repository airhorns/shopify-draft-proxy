/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion);
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

function assertNoTopLevelErrors(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function readRecord(value: unknown): Record<string, unknown> | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function readCustomerId(result: ConformanceGraphqlResult, context: string): string {
  const data = readRecord(result.payload.data);
  const customerCreate = readRecord(data?.['customerCreate']);
  const customer = readRecord(customerCreate?.['customer']);
  const customerId = customer?.['id'];
  if (typeof customerId !== 'string' || customerId.length === 0) {
    throw new Error(`${context} did not return a customer id: ${JSON.stringify(result.payload, null, 2)}`);
  }
  return customerId;
}

const customerSlice = `#graphql
  id
  email
  firstName
  lastName
  tags
  defaultEmailAddress {
    emailAddress
  }
`;

const accountPagesQuery = `#graphql
  query CustomerAccountPages($unknownId: ID!) {
    customerAccountPages(first: 10) {
      nodes {
        id
        title
        handle
        defaultCursor
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
    missing: customerAccountPage(id: $unknownId) {
      id
      title
      handle
      defaultCursor
    }
  }
`;

const createCustomerMutation = `#graphql
  mutation CreateCustomerForDataErasure($input: CustomerInput!) {
    customerCreate(input: $input) {
      customer {
        ${customerSlice}
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const requestDataErasureMutation = `#graphql
  mutation CustomerRequestDataErasure($customerId: ID!) {
    customerRequestDataErasure(customerId: $customerId) {
      customerId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const cancelDataErasureMutation = `#graphql
  mutation CustomerCancelDataErasure($customerId: ID!) {
    customerCancelDataErasure(customerId: $customerId) {
      customerId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const customerReadQuery = `#graphql
  query CustomerDataErasureCustomerRead($id: ID!) {
    customer(id: $id) {
      ${customerSlice}
    }
  }
`;

const deleteCustomerMutation = `#graphql
  mutation DeleteCustomerForDataErasure($input: CustomerDeleteInput!) {
    customerDelete(input: $input) {
      deletedCustomerId
      userErrors {
        field
        message
      }
    }
  }
`;

await mkdir(outputDir, { recursive: true });

const stamp = Date.now();
const emailAddress = `hermes-data-erasure-${stamp}@example.com`;
const customerCreateVariables = {
  input: {
    email: emailAddress,
    firstName: 'Hermes',
    lastName: 'DataErasure',
    tags: ['parity', `data-erasure-${stamp}`],
  },
};
const accountPagesVariables = {
  unknownId: 'gid://shopify/CustomerAccountPage/999999999999999',
};
const unknownCustomerVariables = {
  customerId: 'gid://shopify/Customer/999999999999999',
};

const accountPages = await runGraphqlRequest(accountPagesQuery, accountPagesVariables);
assertNoTopLevelErrors(accountPages, 'customerAccountPages read');

const createCustomer = await runGraphqlRequest(createCustomerMutation, customerCreateVariables);
assertNoTopLevelErrors(createCustomer, 'customerCreate precondition');
const customerId = readCustomerId(createCustomer, 'customerCreate precondition');
const mutationVariables = { customerId };

let requestDataErasure: ConformanceGraphqlResult | null = null;
let afterRequestRead: ConformanceGraphqlResult | null = null;
let cancelDataErasure: ConformanceGraphqlResult | null = null;
let afterCancelRead: ConformanceGraphqlResult | null = null;
let unknownRequest: ConformanceGraphqlResult | null = null;
let unknownCancel: ConformanceGraphqlResult | null = null;
const cleanup: Record<string, unknown> = {};

try {
  requestDataErasure = await runGraphqlRequest(requestDataErasureMutation, mutationVariables);
  assertNoTopLevelErrors(requestDataErasure, 'customerRequestDataErasure success');

  afterRequestRead = await runGraphqlRequest(customerReadQuery, { id: customerId });
  assertNoTopLevelErrors(afterRequestRead, 'customer read after customerRequestDataErasure');

  cancelDataErasure = await runGraphqlRequest(cancelDataErasureMutation, mutationVariables);
  assertNoTopLevelErrors(cancelDataErasure, 'customerCancelDataErasure success');

  afterCancelRead = await runGraphqlRequest(customerReadQuery, { id: customerId });
  assertNoTopLevelErrors(afterCancelRead, 'customer read after customerCancelDataErasure');

  unknownRequest = await runGraphqlRequest(requestDataErasureMutation, unknownCustomerVariables);
  assertNoTopLevelErrors(unknownRequest, 'customerRequestDataErasure unknown customer');

  unknownCancel = await runGraphqlRequest(cancelDataErasureMutation, unknownCustomerVariables);
  assertNoTopLevelErrors(unknownCancel, 'customerCancelDataErasure unknown customer');
} finally {
  const cleanupCancel = await runGraphqlRequest(cancelDataErasureMutation, mutationVariables);
  cleanup['cancelAfterCapture'] = {
    operationName: 'CustomerCancelDataErasure',
    query: cancelDataErasureMutation,
    variables: mutationVariables,
    response: cleanupCancel.payload,
  };

  const deleteCustomer = await runGraphqlRequest(deleteCustomerMutation, { input: { id: customerId } });
  cleanup['customerDelete'] = {
    operationName: 'DeleteCustomerForDataErasure',
    query: deleteCustomerMutation,
    variables: { input: { id: customerId } },
    response: deleteCustomer.payload,
  };
}

const fixture = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  reads: {
    customerAccountPages: {
      operationName: 'CustomerAccountPages',
      query: accountPagesQuery,
      variables: accountPagesVariables,
      response: accountPages.payload,
    },
  },
  precondition: {
    customerCreate: {
      operationName: 'CreateCustomerForDataErasure',
      query: createCustomerMutation,
      variables: customerCreateVariables,
      response: createCustomer.payload,
    },
  },
  mutation: {
    operationName: 'CustomerRequestDataErasure',
    query: requestDataErasureMutation,
    variables: mutationVariables,
    response: requestDataErasure?.payload ?? null,
  },
  mutations: {
    customerRequestDataErasure: {
      operationName: 'CustomerRequestDataErasure',
      query: requestDataErasureMutation,
      variables: mutationVariables,
      response: requestDataErasure?.payload ?? null,
      downstreamRead: {
        operationName: 'CustomerDataErasureCustomerRead',
        query: customerReadQuery,
        variables: { id: customerId },
        response: afterRequestRead?.payload ?? null,
      },
    },
    customerCancelDataErasure: {
      operationName: 'CustomerCancelDataErasure',
      query: cancelDataErasureMutation,
      variables: mutationVariables,
      response: cancelDataErasure?.payload ?? null,
      downstreamRead: {
        operationName: 'CustomerDataErasureCustomerRead',
        query: customerReadQuery,
        variables: { id: customerId },
        response: afterCancelRead?.payload ?? null,
      },
    },
  },
  validation: {
    unknownCustomerRequest: {
      operationName: 'CustomerRequestDataErasure',
      query: requestDataErasureMutation,
      variables: unknownCustomerVariables,
      response: unknownRequest?.payload ?? null,
    },
    unknownCustomerCancel: {
      operationName: 'CustomerCancelDataErasure',
      query: cancelDataErasureMutation,
      variables: unknownCustomerVariables,
      response: unknownCancel?.payload ?? null,
    },
  },
  schemaEvidence: {
    customerRequestDataErasure: {
      args: ['customerId: ID!'],
      payloadFields: ['customerId: ID', 'userErrors: [CustomerRequestDataErasureUserError!]!'],
    },
    customerCancelDataErasure: {
      args: ['customerId: ID!'],
      payloadFields: ['customerId: ID', 'userErrors: [CustomerCancelDataErasureUserError!]!'],
    },
  },
  cleanup,
};

const outputPath = path.join(outputDir, 'customer-account-page-data-erasure.json');
await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');

console.log(
  JSON.stringify(
    {
      ok: true,
      outputPath,
      customerId,
    },
    null,
    2,
  ),
);

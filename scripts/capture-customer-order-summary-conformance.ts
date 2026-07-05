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
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'customers');
const fixturePath = path.join(outputDir, 'customer-order-summary-read-effects.json');
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

function assertHttpOk(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300) {
    throw new Error(`${label} returned HTTP ${result.status}: ${JSON.stringify(result.payload, null, 2)}`);
  }
}

function assertNoTopLevelErrors(result: ConformanceGraphqlResult, label: string): void {
  assertHttpOk(result, label);
  if (result.payload.errors) {
    throw new Error(`${label} returned GraphQL errors: ${JSON.stringify(result.payload.errors, null, 2)}`);
  }
}

function readPayloadPath<T>(source: unknown, pathSegments: string[], label: string): T {
  let current = source;
  for (const segment of pathSegments) {
    current =
      typeof current === 'object' && current !== null ? (current as Record<string, unknown>)[segment] : undefined;
  }

  if (current === undefined || current === null) {
    throw new Error(`${label} missing payload path ${pathSegments.join('.')}`);
  }

  return current as T;
}

const latestOrderQuery = `#graphql
  query CustomerOrderSummaryLatestOrder {
    orders(first: 1, sortKey: CREATED_AT, reverse: true) {
      nodes {
        id
        name
        displayFinancialStatus
        currentTotalPriceSet { shopMoney { amount currencyCode } }
        totalReceivedSet { shopMoney { amount currencyCode } }
        customer { id email displayName }
      }
    }
  }
`;

// Byte-for-byte copy of the proxy's ORDER_LIFECYCLE_HYDRATE_QUERY
// (orders_payments_fulfillment.rs `OrderManagementDownstreamRead`). The proxy
// forwards this verbatim for a cold `orderCustomerSet` to earn the order from
// the backend instead of a precondition seed, so the recorded cassette must
// match the emitted query exactly (cassette matching is byte-exact on the
// query text + variables).
const orderLifecycleHydrateQuery = `query OrderManagementDownstreamRead($id: ID!) {
  order(id: $id) {
    id
    name
    closed
    closedAt
    cancelledAt
    cancelReason
    displayFinancialStatus
    paymentGatewayNames
    totalOutstandingSet {
      shopMoney {
        amount
        currencyCode
      }
    }
    currentTotalPriceSet {
      shopMoney {
        amount
        currencyCode
      }
    }
    customer {
      id
      email
      displayName
    }
    transactions {
      kind
      status
      gateway
      amountSet {
        shopMoney {
          amount
          currencyCode
        }
      }
    }
  }
}`;

// Byte-for-byte copy of the proxy's CUSTOMER_COUNT_HYDRATE_QUERY
// (config/parity-requests/customers/customer-count-hydrate.graphql). The proxy
// forwards this once (variables: {}) the first time a `customersCount` read is
// served from the staged overlay, caching the store-wide baseline. Recording it
// replaces the former `seedCustomersCount` precondition with the real forward.
const customerCountHydrateQuery = `query CustomerCountHydrate { customersCount { count precision } }`;

const customerCreateMutation = `#graphql
  mutation CustomerOrderSummaryCreateCustomer($input: CustomerInput!) {
    customerCreate(input: $input) {
      customer {
        id
        displayName
        email
        numberOfOrders
        amountSpent { amount currencyCode }
        lastOrder { id }
        orders(first: 1) {
          nodes { id }
          pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
        }
      }
      userErrors { field message }
    }
  }
`;

const customerSummaryQuery = `#graphql
  query CustomerOrderSummaryRead($id: ID!, $emailQuery: String!) {
    customer(id: $id) {
      id
      displayName
      email
      numberOfOrders
      amountSpent { amount currencyCode }
      lastOrder {
        id
        name
        currentTotalPriceSet { shopMoney { amount currencyCode } }
      }
      orders(first: 5) {
        nodes {
          id
          name
          currentTotalPriceSet { shopMoney { amount currencyCode } }
          customer { id email displayName }
        }
        edges {
          cursor
          node { id }
        }
        pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
      }
    }
    customers(first: 5, query: $emailQuery) {
      nodes {
        id
        email
        numberOfOrders
        amountSpent { amount currencyCode }
      }
    }
    customersCount(query: $emailQuery) {
      count
      precision
    }
  }
`;

const orderCustomerSetMutation = `#graphql
  mutation CustomerOrderSummarySet($orderId: ID!, $customerId: ID!) {
    orderCustomerSet(orderId: $orderId, customerId: $customerId) {
      order {
        id
        name
        customer { id email displayName }
      }
      userErrors { field message }
    }
  }
`;

const orderCustomerRemoveMutation = `#graphql
  mutation CustomerOrderSummaryRemove($orderId: ID!) {
    orderCustomerRemove(orderId: $orderId) {
      order {
        id
        name
        customer { id email displayName }
      }
      userErrors { field message }
    }
  }
`;

const customerDeleteMutation = `#graphql
  mutation CustomerOrderSummaryDeleteCustomer($input: CustomerDeleteInput!) {
    customerDelete(input: $input) {
      deletedCustomerId
      userErrors { field message }
    }
  }
`;

async function main(): Promise<void> {
  const stamp = Date.now();
  const email = `har-288-customer-order-summary-${stamp}@example.com`;
  const emailQuery = `email:${email}`;
  const cleanup: Record<string, unknown> = {};
  let orderId: string | null = null;
  let customerId: string | null = null;
  let originalCustomerId: string | null = null;

  const latestOrder = await runGraphqlRequest(latestOrderQuery);
  assertNoTopLevelErrors(latestOrder, 'latest order query');
  const latestOrderNodes = readPayloadPath<Record<string, unknown>[]>(
    latestOrder.payload,
    ['data', 'orders', 'nodes'],
    'latest order query',
  );
  const firstOrder = latestOrderNodes[0];
  if (!firstOrder || typeof firstOrder['id'] !== 'string') {
    throw new Error('latest order query did not return an order to mutate for customer summary capture');
  }

  orderId = firstOrder['id'];
  originalCustomerId =
    typeof firstOrder['customer'] === 'object' &&
    firstOrder['customer'] !== null &&
    typeof (firstOrder['customer'] as Record<string, unknown>)['id'] === 'string'
      ? ((firstOrder['customer'] as Record<string, unknown>)['id'] as string)
      : null;

  const createCustomer = await runGraphqlRequest(customerCreateMutation, {
    input: {
      email,
      firstName: 'HAR-288',
      lastName: 'Order Summary',
      tags: ['har-288', 'customer-order-summary'],
    },
  });
  assertNoTopLevelErrors(createCustomer, 'customerCreate setup');
  const createPayload = readPayloadPath<Record<string, unknown>>(
    createCustomer.payload,
    ['data', 'customerCreate'],
    'customerCreate setup',
  );
  const createErrors = createPayload['userErrors'];
  if (Array.isArray(createErrors) && createErrors.length > 0) {
    throw new Error(`customerCreate setup returned userErrors: ${JSON.stringify(createErrors, null, 2)}`);
  }

  const createdCustomer = readPayloadPath<Record<string, unknown>>(
    createCustomer.payload,
    ['data', 'customerCreate', 'customer'],
    'customerCreate setup',
  );
  if (typeof createdCustomer['id'] !== 'string') {
    throw new Error(`customerCreate setup did not return customer id: ${JSON.stringify(createCustomer.payload)}`);
  }
  customerId = createdCustomer['id'];

  try {
    // Record the store-wide customersCount baseline the proxy forwards
    // (CustomerCountHydrate, variables {}) the first time it serves a
    // `customersCount` read from the staged overlay. Replaces seedCustomersCount.
    const customerCountHydrate = await runGraphqlRequest(customerCountHydrateQuery);
    assertNoTopLevelErrors(customerCountHydrate, 'customer count hydrate');

    const beforeSet = await runGraphqlRequest(customerSummaryQuery, { id: customerId, emailQuery });
    assertNoTopLevelErrors(beforeSet, 'before-set customer summary read');

    // Record the order projection the proxy forwards (OrderManagementDownstreamRead)
    // for the cold orderCustomerSet, captured here while the order still carries its
    // original customer — exactly the state the proxy observes before applying the
    // set. Replaces the former seedOrder/seedOrders precondition.
    const orderLifecycleHydrate = await runGraphqlRequest(orderLifecycleHydrateQuery, { id: orderId });
    assertNoTopLevelErrors(orderLifecycleHydrate, 'order lifecycle hydrate');

    const setCustomer = await runGraphqlRequest(orderCustomerSetMutation, { orderId, customerId });
    assertNoTopLevelErrors(setCustomer, 'orderCustomerSet');

    const afterSet = await runGraphqlRequest(customerSummaryQuery, { id: customerId, emailQuery });
    assertNoTopLevelErrors(afterSet, 'after-set customer summary read');

    const removeCustomer = await runGraphqlRequest(orderCustomerRemoveMutation, { orderId });
    assertNoTopLevelErrors(removeCustomer, 'orderCustomerRemove');

    const afterRemove = await runGraphqlRequest(customerSummaryQuery, { id: customerId, emailQuery });
    assertNoTopLevelErrors(afterRemove, 'after-remove customer summary read');

    if (originalCustomerId) {
      const restoreOriginalCustomer = await runGraphqlRequest(orderCustomerSetMutation, {
        orderId,
        customerId: originalCustomerId,
      });
      cleanup['restoreOriginalCustomer'] = restoreOriginalCustomer.payload;
      assertNoTopLevelErrors(restoreOriginalCustomer, 'restore original customer');
    }

    const deleteCustomer = await runGraphqlRequest(customerDeleteMutation, { input: { id: customerId } });
    cleanup['deleteCustomer'] = deleteCustomer.payload;
    assertNoTopLevelErrors(deleteCustomer, 'customerDelete cleanup');

    await mkdir(outputDir, { recursive: true });
    await writeFile(
      fixturePath,
      `${JSON.stringify(
        {
          metadata: {
            capturedAt: new Date().toISOString(),
            storeDomain,
            apiVersion,
            scenario: 'customer order summary read effects after orderCustomerSet/orderCustomerRemove',
          },
          variables: { orderId, customerId, email, emailQuery, originalCustomerId },
          setup: { query: customerCreateMutation.trim(), response: createCustomer.payload },
          beforeSet: { query: customerSummaryQuery.trim(), response: beforeSet.payload },
          orderCustomerSet: {
            query: orderCustomerSetMutation.trim(),
            variables: { orderId, customerId },
            response: setCustomer.payload,
          },
          afterSet: { query: customerSummaryQuery.trim(), response: afterSet.payload },
          orderCustomerRemove: {
            query: orderCustomerRemoveMutation.trim(),
            variables: { orderId },
            response: removeCustomer.payload,
          },
          afterRemove: { query: customerSummaryQuery.trim(), response: afterRemove.payload },
          // Real upstream forwards the proxy makes when no precondition seed exists:
          // the store-wide customersCount baseline and the cold order projection.
          upstreamCalls: [
            {
              operationName: 'CustomerCountHydrate',
              query: customerCountHydrateQuery,
              variables: {},
              response: { status: customerCountHydrate.status, body: customerCountHydrate.payload },
            },
            {
              operationName: 'OrderManagementDownstreamRead',
              query: orderLifecycleHydrateQuery,
              variables: { id: orderId },
              response: { status: orderLifecycleHydrate.status, body: orderLifecycleHydrate.payload },
            },
          ],
          cleanup,
        },
        null,
        2,
      )}\n`,
    );

    console.log(`Wrote ${fixturePath}`);
  } catch (error) {
    if (orderId && originalCustomerId) {
      cleanup['restoreOriginalCustomerAfterError'] = (
        await runGraphqlRequest(orderCustomerSetMutation, { orderId, customerId: originalCustomerId })
      ).payload;
    }
    if (customerId) {
      cleanup['deleteCustomerAfterError'] = (
        await runGraphqlRequest(customerDeleteMutation, { input: { id: customerId } })
      ).payload;
    }
    console.error(JSON.stringify({ cleanup }, null, 2));
    throw error;
  }
}

await main();

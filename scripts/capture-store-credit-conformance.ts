/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type RecordedGraphqlRequest = {
  request: {
    query: string;
    variables: Record<string, unknown>;
  };
  status: number;
  response: unknown;
};

const CUSTOMER_ACCOUNT_SLICE = `
  id
  email
  displayName
  storeCreditAccounts(first: 5) {
    nodes {
      id
      balance {
        amount
        currencyCode
      }
    }
    pageInfo {
      hasNextPage
      hasPreviousPage
    }
  }
`;

const TRANSACTION_SLICE = `
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
      ... on Customer {
        id
        email
      }
    }
  }
`;

const CREATE_CUSTOMER_MUTATION = `#graphql
  mutation StoreCreditConformanceCustomerCreate($input: CustomerInput!) {
    customerCreate(input: $input) {
      customer {
        ${CUSTOMER_ACCOUNT_SLICE}
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const STORE_CREDIT_ACCOUNT_CREDIT_MUTATION = `#graphql
  mutation StoreCreditAccountCreditParity($id: ID!, $creditInput: StoreCreditAccountCreditInput!) {
    storeCreditAccountCredit(id: $id, creditInput: $creditInput) {
      storeCreditAccountTransaction {
        ${TRANSACTION_SLICE}
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const STORE_CREDIT_ACCOUNT_DEBIT_MUTATION = `#graphql
  mutation StoreCreditAccountDebitParity($id: ID!, $debitInput: StoreCreditAccountDebitInput!) {
    storeCreditAccountDebit(id: $id, debitInput: $debitInput) {
      storeCreditAccountTransaction {
        ${TRANSACTION_SLICE}
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const STORE_CREDIT_ACCOUNT_READBACK_QUERY = `#graphql
  query StoreCreditAccountReadbackParity($customerId: ID!, $accountId: ID!) {
    customer(id: $customerId) {
      ${CUSTOMER_ACCOUNT_SLICE}
    }
    storeCreditAccount(id: $accountId) {
      id
      balance {
        amount
        currencyCode
      }
      owner {
        ... on Customer {
          id
          email
        }
      }
    }
  }
`;

const DELETE_CUSTOMER_MUTATION = `#graphql
  mutation StoreCreditConformanceCustomerDelete($input: CustomerDeleteInput!) {
    customerDelete(input: $input) {
      deletedCustomerId
      userErrors {
        field
        message
      }
    }
  }
`;

function readResponseData(result: ConformanceGraphqlResult): Record<string, unknown> | null {
  return result.payload.data && typeof result.payload.data === 'object'
    ? (result.payload.data as Record<string, unknown>)
    : null;
}

function assertNoGraphqlFailure(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertNoUserErrors(result: ConformanceGraphqlResult, rootName: string, context: string): void {
  assertNoGraphqlFailure(result, context);
  const payload = readResponseData(result)?.[rootName];
  const userErrors =
    payload && typeof payload === 'object' && Array.isArray((payload as { userErrors?: unknown }).userErrors)
      ? (payload as { userErrors: unknown[] }).userErrors
      : [];
  if (userErrors.length > 0) {
    throw new Error(`${context} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function record(
  query: string,
  variables: Record<string, unknown>,
  result: ConformanceGraphqlResult,
): RecordedGraphqlRequest {
  return {
    request: {
      query,
      variables,
    },
    status: result.status,
    response: result.payload,
  };
}

function readStringAtPath(value: unknown, pathSegments: string[]): string | null {
  let current = value;
  for (const segment of pathSegments) {
    if (!current || typeof current !== 'object' || !(segment in current)) {
      return null;
    }
    current = (current as Record<string, unknown>)[segment];
  }
  return typeof current === 'string' && current.length > 0 ? current : null;
}

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion);
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

await mkdir(outputDir, { recursive: true });

const stamp = Date.now();
const email = `hermes-store-credit-${stamp}@example.com`;
const createdCustomerIds = new Set<string>();
const cleanupRecords: Record<string, RecordedGraphqlRequest> = {};
let fixtureCore: Record<string, unknown> | null = null;

const createCustomerVariables = {
  input: {
    email,
    firstName: 'Hermes',
    lastName: 'StoreCredit',
    tags: ['store-credit-parity', String(stamp)],
  },
};
const createCustomer = await runGraphqlRequest(CREATE_CUSTOMER_MUTATION, createCustomerVariables);
assertNoUserErrors(createCustomer, 'customerCreate', 'customerCreate store-credit precondition');
const customerId = readStringAtPath(createCustomer.payload, ['data', 'customerCreate', 'customer', 'id']);
if (!customerId) {
  throw new Error(`customerCreate did not return a customer id: ${JSON.stringify(createCustomer.payload, null, 2)}`);
}
createdCustomerIds.add(customerId);

let accountId: string | null = null;
let cleanupDebitAmount = '0.00';

try {
  const setupCreditVariables = {
    id: customerId,
    creditInput: {
      creditAmount: {
        amount: '7.23',
        currencyCode: 'USD',
      },
    },
  };
  const setupCredit = await runGraphqlRequest(STORE_CREDIT_ACCOUNT_CREDIT_MUTATION, setupCreditVariables);
  assertNoUserErrors(setupCredit, 'storeCreditAccountCredit', 'storeCreditAccountCredit customer-id setup');
  accountId = readStringAtPath(setupCredit.payload, [
    'data',
    'storeCreditAccountCredit',
    'storeCreditAccountTransaction',
    'account',
    'id',
  ]);
  if (!accountId) {
    throw new Error(
      `storeCreditAccountCredit did not create/return an account id: ${JSON.stringify(setupCredit.payload, null, 2)}`,
    );
  }

  const creditVariables = {
    id: accountId,
    creditInput: {
      creditAmount: {
        amount: '1.11',
        currencyCode: 'USD',
      },
    },
  };
  const credit = await runGraphqlRequest(STORE_CREDIT_ACCOUNT_CREDIT_MUTATION, creditVariables);
  assertNoUserErrors(credit, 'storeCreditAccountCredit', 'storeCreditAccountCredit account-id mutation');

  const debitVariables = {
    id: accountId,
    debitInput: {
      debitAmount: {
        amount: '2.22',
        currencyCode: 'USD',
      },
    },
  };
  const debit = await runGraphqlRequest(STORE_CREDIT_ACCOUNT_DEBIT_MUTATION, debitVariables);
  assertNoUserErrors(debit, 'storeCreditAccountDebit', 'storeCreditAccountDebit account-id mutation');

  const readbackVariables = { customerId, accountId };
  const readback = await runGraphqlRequest(STORE_CREDIT_ACCOUNT_READBACK_QUERY, readbackVariables);
  assertNoGraphqlFailure(readback, 'storeCreditAccount downstream readback');
  cleanupDebitAmount =
    readStringAtPath(readback.payload, ['data', 'storeCreditAccount', 'balance', 'amount']) ?? cleanupDebitAmount;

  fixtureCore = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    notes: [
      'HAR-317 store-credit success-path capture creates a disposable customer, uses storeCreditAccountCredit with the customer id to create the store credit account, then replays account-id credit/debit mutations and downstream reads.',
      'Cleanup debits the remaining captured balance back to zero and deletes the disposable customer. Store credit account identifiers may remain visible in Shopify audit/history even after balance neutralization.',
    ],
    setup: {
      createCustomer: record(CREATE_CUSTOMER_MUTATION, createCustomerVariables, createCustomer),
      createAccountCredit: record(STORE_CREDIT_ACCOUNT_CREDIT_MUTATION, setupCreditVariables, setupCredit),
    },
    mutation: record(STORE_CREDIT_ACCOUNT_CREDIT_MUTATION, creditVariables, credit),
    debit: record(STORE_CREDIT_ACCOUNT_DEBIT_MUTATION, debitVariables, debit),
    downstreamRead: record(STORE_CREDIT_ACCOUNT_READBACK_QUERY, readbackVariables, readback),
  };

  const cleanupDebitVariables = {
    id: accountId,
    debitInput: {
      debitAmount: {
        amount: cleanupDebitAmount,
        currencyCode: 'USD',
      },
    },
  };
  const cleanupDebit = await runGraphqlRequest(STORE_CREDIT_ACCOUNT_DEBIT_MUTATION, cleanupDebitVariables);
  cleanupRecords['debitRemainingBalance'] = record(
    STORE_CREDIT_ACCOUNT_DEBIT_MUTATION,
    cleanupDebitVariables,
    cleanupDebit,
  );
  assertNoUserErrors(cleanupDebit, 'storeCreditAccountDebit', 'storeCreditAccountDebit cleanup debit');
} finally {
  for (const customerIdToDelete of createdCustomerIds) {
    const deleteVariables = { input: { id: customerIdToDelete } };
    const deleteResult = await runGraphqlRequest(DELETE_CUSTOMER_MUTATION, deleteVariables);
    cleanupRecords['customerDelete'] = record(DELETE_CUSTOMER_MUTATION, deleteVariables, deleteResult);
    assertNoUserErrors(deleteResult, 'customerDelete', 'customerDelete store-credit cleanup');
  }
}

if (!fixtureCore) {
  throw new Error('Store credit capture did not complete; no fixture was written.');
}

const outputPath = path.join(outputDir, 'store-credit-account-parity.json');
await writeFile(outputPath, `${JSON.stringify({ ...fixtureCore, cleanup: cleanupRecords }, null, 2)}\n`, 'utf8');
console.log(`Wrote ${outputPath}`);

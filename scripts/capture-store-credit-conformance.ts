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
  queriedStoreCreditAccounts: storeCreditAccounts(first: 5, query: "currency_code:CHF") {
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
      startCursor
      endCursor
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

const NEVER_CREATED_STORE_CREDIT_ACCOUNT_ID = 'gid://shopify/StoreCreditAccount/999999999999999';

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
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'customers');
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
let secondaryAccountId: string | null = null;
let secondaryCleanupDebitAmount = '0.00';

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

  const missingAccountZeroCreditVariables = {
    id: NEVER_CREATED_STORE_CREDIT_ACCOUNT_ID,
    creditInput: {
      creditAmount: {
        amount: '0.00',
        currencyCode: 'USD',
      },
    },
  };
  const missingAccountZeroCredit = await runGraphqlRequest(
    STORE_CREDIT_ACCOUNT_CREDIT_MUTATION,
    missingAccountZeroCreditVariables,
  );
  assertNoGraphqlFailure(
    missingAccountZeroCredit,
    'storeCreditAccountCredit missing account plus zero amount validation',
  );

  const missingAccountZeroDebitVariables = {
    id: NEVER_CREATED_STORE_CREDIT_ACCOUNT_ID,
    debitInput: {
      debitAmount: {
        amount: '0.00',
        currencyCode: 'USD',
      },
    },
  };
  const missingAccountZeroDebit = await runGraphqlRequest(
    STORE_CREDIT_ACCOUNT_DEBIT_MUTATION,
    missingAccountZeroDebitVariables,
  );
  assertNoGraphqlFailure(
    missingAccountZeroDebit,
    'storeCreditAccountDebit missing account plus zero amount validation',
  );

  const pastExpiryNegativeCreditVariables = {
    id: accountId,
    creditInput: {
      creditAmount: {
        amount: '-5.00',
        currencyCode: 'USD',
      },
      expiresAt: '2000-01-01T00:00:00Z',
    },
  };
  const pastExpiryNegativeCredit = await runGraphqlRequest(
    STORE_CREDIT_ACCOUNT_CREDIT_MUTATION,
    pastExpiryNegativeCreditVariables,
  );
  assertNoGraphqlFailure(
    pastExpiryNegativeCredit,
    'storeCreditAccountCredit past expiresAt plus negative amount validation',
  );

  const accountCurrencyMismatchVariables = {
    id: accountId,
    creditInput: {
      creditAmount: {
        amount: '2.00',
        currencyCode: 'CAD',
      },
    },
  };
  const accountCurrencyMismatch = await runGraphqlRequest(
    STORE_CREDIT_ACCOUNT_CREDIT_MUTATION,
    accountCurrencyMismatchVariables,
  );
  assertNoGraphqlFailure(accountCurrencyMismatch, 'storeCreditAccountCredit account-id currency mismatch');

  const unsupportedDebitCurrencyVariables = {
    id: accountId,
    debitInput: {
      debitAmount: {
        amount: '1.00',
        currencyCode: 'CHF',
      },
    },
  };
  const unsupportedDebitCurrency = await runGraphqlRequest(
    STORE_CREDIT_ACCOUNT_DEBIT_MUTATION,
    unsupportedDebitCurrencyVariables,
  );
  assertNoGraphqlFailure(unsupportedDebitCurrency, 'storeCreditAccountDebit unsupported currency validation');

  const pastExpiryVariables = {
    id: customerId,
    creditInput: {
      creditAmount: {
        amount: '1.00',
        currencyCode: 'USD',
      },
      expiresAt: '2000-01-01T00:00:00Z',
    },
  };
  const pastExpiry = await runGraphqlRequest(STORE_CREDIT_ACCOUNT_CREDIT_MUTATION, pastExpiryVariables);
  assertNoGraphqlFailure(pastExpiry, 'storeCreditAccountCredit past expiresAt validation');

  const zeroCreditVariables = {
    id: customerId,
    creditInput: {
      creditAmount: {
        amount: '0.00',
        currencyCode: 'USD',
      },
    },
  };
  const zeroCredit = await runGraphqlRequest(STORE_CREDIT_ACCOUNT_CREDIT_MUTATION, zeroCreditVariables);
  assertNoGraphqlFailure(zeroCredit, 'storeCreditAccountCredit zero amount validation');

  const zeroDebitVariables = {
    id: customerId,
    debitInput: {
      debitAmount: {
        amount: '0.00',
        currencyCode: 'USD',
      },
    },
  };
  const zeroDebit = await runGraphqlRequest(STORE_CREDIT_ACCOUNT_DEBIT_MUTATION, zeroDebitVariables);
  assertNoGraphqlFailure(zeroDebit, 'storeCreditAccountDebit zero amount validation');

  const negativeDebitVariables = {
    id: customerId,
    debitInput: {
      debitAmount: {
        amount: '-1.00',
        currencyCode: 'USD',
      },
    },
  };
  const negativeDebit = await runGraphqlRequest(STORE_CREDIT_ACCOUNT_DEBIT_MUTATION, negativeDebitVariables);
  assertNoGraphqlFailure(negativeDebit, 'storeCreditAccountDebit negative amount validation');

  const ownerSecondCurrencyCreditVariables = {
    id: customerId,
    creditInput: {
      creditAmount: {
        amount: '2.00',
        currencyCode: 'CHF',
      },
    },
  };
  const ownerSecondCurrencyCredit = await runGraphqlRequest(
    STORE_CREDIT_ACCOUNT_CREDIT_MUTATION,
    ownerSecondCurrencyCreditVariables,
  );
  assertNoUserErrors(
    ownerSecondCurrencyCredit,
    'storeCreditAccountCredit',
    'storeCreditAccountCredit owner-id second-currency mutation',
  );
  secondaryAccountId = readStringAtPath(ownerSecondCurrencyCredit.payload, [
    'data',
    'storeCreditAccountCredit',
    'storeCreditAccountTransaction',
    'account',
    'id',
  ]);
  secondaryCleanupDebitAmount =
    readStringAtPath(ownerSecondCurrencyCredit.payload, [
      'data',
      'storeCreditAccountCredit',
      'storeCreditAccountTransaction',
      'balanceAfterTransaction',
      'amount',
    ]) ?? secondaryCleanupDebitAmount;

  const resultOnlyUsdcCreditVariables = {
    id: customerId,
    creditInput: {
      creditAmount: {
        amount: '1.00',
        currencyCode: 'USDC',
      },
    },
  };
  const resultOnlyUsdcCredit = await runGraphqlRequest(
    STORE_CREDIT_ACCOUNT_CREDIT_MUTATION,
    resultOnlyUsdcCreditVariables,
  );
  if (!resultOnlyUsdcCredit.payload.errors) {
    throw new Error(
      `storeCreditAccountCredit USDC validation unexpectedly succeeded: ${JSON.stringify(
        resultOnlyUsdcCredit.payload,
        null,
        2,
      )}`,
    );
  }

  const resultOnlyXxxCreditVariables = {
    id: customerId,
    creditInput: {
      creditAmount: {
        amount: '1.00',
        currencyCode: 'XXX',
      },
    },
  };
  const resultOnlyXxxCredit = await runGraphqlRequest(
    STORE_CREDIT_ACCOUNT_CREDIT_MUTATION,
    resultOnlyXxxCreditVariables,
  );
  if (!resultOnlyXxxCredit.payload.errors) {
    throw new Error(
      `storeCreditAccountCredit XXX validation unexpectedly succeeded: ${JSON.stringify(
        resultOnlyXxxCredit.payload,
        null,
        2,
      )}`,
    );
  }

  const overLimitDebitVariables = {
    id: customerId,
    debitInput: {
      debitAmount: {
        amount: '9999.00',
        currencyCode: 'USD',
      },
    },
  };
  const overLimitDebit = await runGraphqlRequest(STORE_CREDIT_ACCOUNT_DEBIT_MUTATION, overLimitDebitVariables);
  assertNoGraphqlFailure(overLimitDebit, 'storeCreditAccountDebit over-limit validation');

  const overLimitCreditVariables = {
    id: customerId,
    creditInput: {
      creditAmount: {
        amount: '100000.00',
        currencyCode: 'USD',
      },
    },
  };
  const overLimitCredit = await runGraphqlRequest(STORE_CREDIT_ACCOUNT_CREDIT_MUTATION, overLimitCreditVariables);
  assertNoGraphqlFailure(overLimitCredit, 'storeCreditAccountCredit credit-limit validation');

  const unsupportedAttributionCreditVariables = {
    id: customerId,
    creditInput: {
      creditAmount: {
        amount: '1.00',
        currencyCode: 'USD',
      },
      notify: true,
      attribution: {
        userId: 'gid://shopify/User/1',
        locationId: 'gid://shopify/Location/1',
      },
    },
  };
  const unsupportedAttributionCredit = await runGraphqlRequest(
    STORE_CREDIT_ACCOUNT_CREDIT_MUTATION,
    unsupportedAttributionCreditVariables,
  );
  if (!unsupportedAttributionCredit.payload.errors) {
    throw new Error(
      `storeCreditAccountCredit attribution validation unexpectedly succeeded: ${JSON.stringify(
        unsupportedAttributionCredit.payload,
        null,
        2,
      )}`,
    );
  }

  const unsupportedDebitFieldsVariables = {
    id: customerId,
    debitInput: {
      debitAmount: {
        amount: '1.00',
        currencyCode: 'USD',
      },
      notify: false,
      attribution: {
        userId: 'gid://shopify/User/1',
      },
    },
  };
  const unsupportedDebitFields = await runGraphqlRequest(
    STORE_CREDIT_ACCOUNT_DEBIT_MUTATION,
    unsupportedDebitFieldsVariables,
  );
  if (!unsupportedDebitFields.payload.errors) {
    throw new Error(
      `storeCreditAccountDebit unsupported fields validation unexpectedly succeeded: ${JSON.stringify(
        unsupportedDebitFields.payload,
        null,
        2,
      )}`,
    );
  }

  const creditVariables = {
    id: accountId,
    creditInput: {
      creditAmount: {
        amount: '1.11',
        currencyCode: 'USD',
      },
      notify: true,
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
      'Store-credit success-path capture creates a disposable customer, uses storeCreditAccountCredit with the customer id to create the store credit account, then replays account-id credit/debit mutations and downstream reads.',
      'Cleanup debits the remaining captured balance back to zero and deletes the disposable customer. Store credit account identifiers may remain visible in Shopify audit/history even after balance neutralization.',
    ],
    setup: {
      createCustomer: record(CREATE_CUSTOMER_MUTATION, createCustomerVariables, createCustomer),
      createAccountCredit: record(STORE_CREDIT_ACCOUNT_CREDIT_MUTATION, setupCreditVariables, setupCredit),
    },
    validations: {
      missingAccountZeroCredit: record(
        STORE_CREDIT_ACCOUNT_CREDIT_MUTATION,
        missingAccountZeroCreditVariables,
        missingAccountZeroCredit,
      ),
      missingAccountZeroDebit: record(
        STORE_CREDIT_ACCOUNT_DEBIT_MUTATION,
        missingAccountZeroDebitVariables,
        missingAccountZeroDebit,
      ),
      pastExpiryNegativeCredit: record(
        STORE_CREDIT_ACCOUNT_CREDIT_MUTATION,
        pastExpiryNegativeCreditVariables,
        pastExpiryNegativeCredit,
      ),
      accountCurrencyMismatch: record(
        STORE_CREDIT_ACCOUNT_CREDIT_MUTATION,
        accountCurrencyMismatchVariables,
        accountCurrencyMismatch,
      ),
      unsupportedDebitCurrency: record(
        STORE_CREDIT_ACCOUNT_DEBIT_MUTATION,
        unsupportedDebitCurrencyVariables,
        unsupportedDebitCurrency,
      ),
      pastExpiry: record(STORE_CREDIT_ACCOUNT_CREDIT_MUTATION, pastExpiryVariables, pastExpiry),
      zeroCredit: record(STORE_CREDIT_ACCOUNT_CREDIT_MUTATION, zeroCreditVariables, zeroCredit),
      zeroDebit: record(STORE_CREDIT_ACCOUNT_DEBIT_MUTATION, zeroDebitVariables, zeroDebit),
      negativeDebit: record(STORE_CREDIT_ACCOUNT_DEBIT_MUTATION, negativeDebitVariables, negativeDebit),
      ownerSecondCurrencyCredit: record(
        STORE_CREDIT_ACCOUNT_CREDIT_MUTATION,
        ownerSecondCurrencyCreditVariables,
        ownerSecondCurrencyCredit,
      ),
      resultOnlyUsdcCredit: record(
        STORE_CREDIT_ACCOUNT_CREDIT_MUTATION,
        resultOnlyUsdcCreditVariables,
        resultOnlyUsdcCredit,
      ),
      resultOnlyXxxCredit: record(
        STORE_CREDIT_ACCOUNT_CREDIT_MUTATION,
        resultOnlyXxxCreditVariables,
        resultOnlyXxxCredit,
      ),
      overLimitDebit: record(STORE_CREDIT_ACCOUNT_DEBIT_MUTATION, overLimitDebitVariables, overLimitDebit),
      overLimitCredit: record(STORE_CREDIT_ACCOUNT_CREDIT_MUTATION, overLimitCreditVariables, overLimitCredit),
      unsupportedAttributionCredit: record(
        STORE_CREDIT_ACCOUNT_CREDIT_MUTATION,
        unsupportedAttributionCreditVariables,
        unsupportedAttributionCredit,
      ),
      unsupportedDebitFields: record(
        STORE_CREDIT_ACCOUNT_DEBIT_MUTATION,
        unsupportedDebitFieldsVariables,
        unsupportedDebitFields,
      ),
    },
    mutation: record(STORE_CREDIT_ACCOUNT_CREDIT_MUTATION, creditVariables, credit),
    debit: record(STORE_CREDIT_ACCOUNT_DEBIT_MUTATION, debitVariables, debit),
    downstreamRead: record(STORE_CREDIT_ACCOUNT_READBACK_QUERY, readbackVariables, readback),
  };

  if (secondaryAccountId) {
    const cleanupSecondaryDebitVariables = {
      id: secondaryAccountId,
      debitInput: {
        debitAmount: {
          amount: secondaryCleanupDebitAmount,
          currencyCode: 'CHF',
        },
      },
    };
    const cleanupSecondaryDebit = await runGraphqlRequest(
      STORE_CREDIT_ACCOUNT_DEBIT_MUTATION,
      cleanupSecondaryDebitVariables,
    );
    cleanupRecords['debitSecondaryCurrencyBalance'] = record(
      STORE_CREDIT_ACCOUNT_DEBIT_MUTATION,
      cleanupSecondaryDebitVariables,
      cleanupSecondaryDebit,
    );
    assertNoUserErrors(
      cleanupSecondaryDebit,
      'storeCreditAccountDebit',
      'storeCreditAccountDebit secondary currency cleanup debit',
    );
  }

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

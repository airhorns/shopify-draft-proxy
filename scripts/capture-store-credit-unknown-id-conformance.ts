/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
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

const STORE_CREDIT_ACCOUNT_CREDIT_MUTATION = `#graphql
  mutation StoreCreditAccountCreditParity($id: ID!, $creditInput: StoreCreditAccountCreditInput!) {
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
            ... on Customer {
              id
              email
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

const STORE_CREDIT_ACCOUNT_DEBIT_MUTATION = `#graphql
  mutation StoreCreditAccountDebitParity($id: ID!, $debitInput: StoreCreditAccountDebitInput!) {
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
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const UNKNOWN_IDS = {
  customer: 'gid://shopify/Customer/999999999999999',
  storeCreditAccount: 'gid://shopify/StoreCreditAccount/999999999999999',
  companyLocation: 'gid://shopify/CompanyLocation/999999999999999',
} as const;

const CREDIT_INPUT = {
  creditAmount: {
    amount: '1.00',
    currencyCode: 'USD',
  },
};

const DEBIT_INPUT = {
  debitAmount: {
    amount: '1.00',
    currencyCode: 'USD',
  },
};

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

function objectValue(value: unknown): Record<string, unknown> | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

type ExpectedUserError = {
  message: string;
  code: string;
};

function creditExpectedUserError(id: string): ExpectedUserError {
  if (id.startsWith('gid://shopify/Customer/') || id.startsWith('gid://shopify/CompanyLocation/')) {
    return {
      message: 'Owner does not exist',
      code: 'OWNER_NOT_FOUND',
    };
  }

  return {
    message: 'Store credit account does not exist',
    code: 'ACCOUNT_NOT_FOUND',
  };
}

function debitExpectedUserError(): ExpectedUserError {
  return {
    message: 'Store credit account does not exist',
    code: 'ACCOUNT_NOT_FOUND',
  };
}

function assertMissingIdUserError(
  result: ConformanceGraphqlResult,
  root: string,
  expectedUserError: ExpectedUserError,
  context: string,
): void {
  const payload = objectValue(result.payload);
  const data = objectValue(payload?.['data']);
  const rootPayload = objectValue(data?.[root]);
  const userErrors = Array.isArray(rootPayload?.['userErrors']) ? rootPayload['userErrors'] : [];
  const firstUserError = objectValue(userErrors[0]);

  if (
    result.status !== 200 ||
    payload?.['errors'] !== undefined ||
    rootPayload?.['storeCreditAccountTransaction'] !== null ||
    firstUserError?.['message'] !== expectedUserError.message ||
    firstUserError?.['code'] !== expectedUserError.code ||
    JSON.stringify(firstUserError?.['field']) !== JSON.stringify(['id'])
  ) {
    throw new Error(`${context} did not return the expected missing-id userError: ${JSON.stringify(result, null, 2)}`);
  }
}

async function captureCreditUnknownId(
  runGraphqlRequest: (query: string, variables?: Record<string, unknown>) => Promise<ConformanceGraphqlResult>,
  id: string,
  context: string,
): Promise<RecordedGraphqlRequest> {
  const variables = {
    id,
    creditInput: CREDIT_INPUT,
  };
  const result = await runGraphqlRequest(STORE_CREDIT_ACCOUNT_CREDIT_MUTATION, variables);
  assertMissingIdUserError(result, 'storeCreditAccountCredit', creditExpectedUserError(id), context);
  return record(STORE_CREDIT_ACCOUNT_CREDIT_MUTATION, variables, result);
}

async function captureDebitUnknownId(
  runGraphqlRequest: (query: string, variables?: Record<string, unknown>) => Promise<ConformanceGraphqlResult>,
  id: string,
  context: string,
): Promise<RecordedGraphqlRequest> {
  const variables = {
    id,
    debitInput: DEBIT_INPUT,
  };
  const result = await runGraphqlRequest(STORE_CREDIT_ACCOUNT_DEBIT_MUTATION, variables);
  assertMissingIdUserError(result, 'storeCreditAccountDebit', debitExpectedUserError(), context);
  return record(STORE_CREDIT_ACCOUNT_DEBIT_MUTATION, variables, result);
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

const fixture = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  notes: [
    'Records StoreCreditAccountCredit and StoreCreditAccountDebit missing-id userErrors for well-formed but nonexistent StoreCreditAccount, Customer, and CompanyLocation GIDs.',
    'Shopify returns payload userErrors for these mutation roots: credit by an unknown Customer or CompanyLocation returns OWNER_NOT_FOUND, while unknown StoreCreditAccount and debit missing-id branches return ACCOUNT_NOT_FOUND.',
  ],
  unknownIds: UNKNOWN_IDS,
  creditUnknownCustomer: await captureCreditUnknownId(
    runGraphqlRequest,
    UNKNOWN_IDS.customer,
    'storeCreditAccountCredit unknown Customer id',
  ),
  creditUnknownStoreCreditAccount: await captureCreditUnknownId(
    runGraphqlRequest,
    UNKNOWN_IDS.storeCreditAccount,
    'storeCreditAccountCredit unknown StoreCreditAccount id',
  ),
  creditUnknownCompanyLocation: await captureCreditUnknownId(
    runGraphqlRequest,
    UNKNOWN_IDS.companyLocation,
    'storeCreditAccountCredit unknown CompanyLocation id',
  ),
  debitUnknownCustomer: await captureDebitUnknownId(
    runGraphqlRequest,
    UNKNOWN_IDS.customer,
    'storeCreditAccountDebit unknown Customer id',
  ),
  debitUnknownStoreCreditAccount: await captureDebitUnknownId(
    runGraphqlRequest,
    UNKNOWN_IDS.storeCreditAccount,
    'storeCreditAccountDebit unknown StoreCreditAccount id',
  ),
  debitUnknownCompanyLocation: await captureDebitUnknownId(
    runGraphqlRequest,
    UNKNOWN_IDS.companyLocation,
    'storeCreditAccountDebit unknown CompanyLocation id',
  ),
  upstreamCalls: [],
};

const outputPath = path.join(outputDir, 'store-credit-account-unknown-id-user-errors.json');
await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
console.log(`Wrote ${outputPath}`);

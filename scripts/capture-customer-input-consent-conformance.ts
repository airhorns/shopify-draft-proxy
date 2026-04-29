/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'customers');
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

function readCreatedCustomerId(result: ConformanceGraphqlResult): string {
  const data = readRecord(result.payload.data);
  const customerCreate = readRecord(data?.['customerCreate']);
  const customer = readRecord(customerCreate?.['customer']);
  const customerId = customer?.['id'];
  if (typeof customerId !== 'string' || customerId.length === 0) {
    throw new Error(`customerCreate did not return a customer id: ${JSON.stringify(result.payload, null, 2)}`);
  }
  return customerId;
}

const customerConsentSlice = `#graphql
  id
  email
  defaultEmailAddress {
    emailAddress
    marketingState
    marketingOptInLevel
    marketingUpdatedAt
  }
  defaultPhoneNumber {
    phoneNumber
    marketingState
    marketingOptInLevel
    marketingUpdatedAt
    marketingCollectedFrom
  }
  emailMarketingConsent {
    marketingState
    marketingOptInLevel
    consentUpdatedAt
  }
  smsMarketingConsent {
    marketingState
    marketingOptInLevel
    consentUpdatedAt
    consentCollectedFrom
  }
`;

const customerCreateMutation = `#graphql
  mutation CustomerInputInlineConsentCreate($input: CustomerInput!) {
    customerCreate(input: $input) {
      customer {
        ${customerConsentSlice}
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const customerUpdateMutation = `#graphql
  mutation CustomerInputInlineConsentUpdate($input: CustomerInput!) {
    customerUpdate(input: $input) {
      customer {
        ${customerConsentSlice}
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const downstreamReadQuery = `#graphql
  query CustomerInputInlineConsentRead($id: ID!, $identifier: CustomerIdentifierInput!, $query: String!) {
    customer(id: $id) {
      ${customerConsentSlice}
    }
    customerByIdentifier(identifier: $identifier) {
      ${customerConsentSlice}
    }
    customers(first: 5, query: $query, sortKey: UPDATED_AT, reverse: true) {
      nodes {
        ${customerConsentSlice}
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
      }
    }
  }
`;

const customerDeleteMutation = `#graphql
  mutation CustomerInputInlineConsentDelete($input: CustomerDeleteInput!) {
    customerDelete(input: $input) {
      deletedCustomerId
      userErrors {
        field
        message
      }
    }
  }
`;

const schemaEvidenceQuery = `#graphql
  query CustomerInputInlineConsentSchema {
    customerInput: __type(name: "CustomerInput") {
      inputFields {
        name
        type {
          kind
          name
          ofType {
            kind
            name
          }
        }
      }
    }
    emailConsentInput: __type(name: "CustomerEmailMarketingConsentInput") {
      inputFields {
        name
      }
    }
    smsConsentInput: __type(name: "CustomerSmsMarketingConsentInput") {
      inputFields {
        name
      }
    }
  }
`;

await mkdir(outputDir, { recursive: true });

const stamp = Date.now();
const emailAddress = `hermes-inline-consent-${stamp}@example.com`;
const phoneNumber = `+1415${String(stamp).slice(-7).padStart(7, '0')}`;
const createdCustomerIds = new Set<string>();
const cleanup: Record<string, unknown> = {};

const schemaEvidence = await runGraphqlRequest(schemaEvidenceQuery, {});
assertNoTopLevelErrors(schemaEvidence, 'customer input inline consent schema evidence');

const createVariables = {
  input: {
    email: emailAddress,
    phone: phoneNumber,
    firstName: 'Hermes',
    lastName: 'InlineConsent',
    tags: ['parity', `inline-consent-${stamp}`],
    emailMarketingConsent: {
      marketingState: 'SUBSCRIBED',
      marketingOptInLevel: 'SINGLE_OPT_IN',
      consentUpdatedAt: '2026-04-25T01:00:00Z',
    },
    smsMarketingConsent: {
      marketingState: 'SUBSCRIBED',
      marketingOptInLevel: 'SINGLE_OPT_IN',
      consentUpdatedAt: '2026-04-25T01:05:00Z',
    },
  },
};

let createResult: ConformanceGraphqlResult | null = null;
let downstreamRead: ConformanceGraphqlResult | null = null;
let updateEmailConsent: ConformanceGraphqlResult | null = null;
let updateSmsConsent: ConformanceGraphqlResult | null = null;
let downstreamAfterRejectedUpdates: ConformanceGraphqlResult | null = null;
let customerId: string | null = null;

try {
  createResult = await runGraphqlRequest(customerCreateMutation, createVariables);
  assertNoTopLevelErrors(createResult, 'customerCreate inline consent');
  customerId = readCreatedCustomerId(createResult);
  createdCustomerIds.add(customerId);

  const downstreamVariables = {
    id: customerId,
    identifier: { id: customerId },
    query: `email:${emailAddress}`,
  };
  downstreamRead = await runGraphqlRequest(downstreamReadQuery, downstreamVariables);
  assertNoTopLevelErrors(downstreamRead, 'customerCreate inline consent downstream read');

  const updateEmailVariables = {
    input: {
      id: customerId,
      emailMarketingConsent: {
        marketingState: 'UNSUBSCRIBED',
        marketingOptInLevel: 'SINGLE_OPT_IN',
        consentUpdatedAt: '2026-04-25T02:00:00Z',
      },
    },
  };
  updateEmailConsent = await runGraphqlRequest(customerUpdateMutation, updateEmailVariables);
  assertNoTopLevelErrors(updateEmailConsent, 'customerUpdate inline email consent rejection');

  const updateSmsVariables = {
    input: {
      id: customerId,
      smsMarketingConsent: {
        marketingState: 'UNSUBSCRIBED',
        marketingOptInLevel: 'SINGLE_OPT_IN',
        consentUpdatedAt: '2026-04-25T02:05:00Z',
      },
    },
  };
  updateSmsConsent = await runGraphqlRequest(customerUpdateMutation, updateSmsVariables);
  assertNoTopLevelErrors(updateSmsConsent, 'customerUpdate inline sms consent rejection');

  downstreamAfterRejectedUpdates = await runGraphqlRequest(downstreamReadQuery, downstreamVariables);
  assertNoTopLevelErrors(downstreamAfterRejectedUpdates, 'customerUpdate inline consent rejection downstream read');
} finally {
  for (const disposableCustomerId of [...createdCustomerIds].reverse()) {
    const deleteVariables = { input: { id: disposableCustomerId } };
    const deleteCustomer = await runGraphqlRequest(customerDeleteMutation, deleteVariables);
    cleanup[disposableCustomerId] = {
      operationName: 'CustomerInputInlineConsentDelete',
      query: customerDeleteMutation,
      variables: deleteVariables,
      response: deleteCustomer.payload,
      status: deleteCustomer.status,
    };
  }
}

const fixture = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  schemaEvidence: {
    operationName: 'CustomerInputInlineConsentSchema',
    query: schemaEvidenceQuery,
    variables: {},
    response: schemaEvidence.payload,
  },
  mutation: {
    operationName: 'CustomerInputInlineConsentCreate',
    query: customerCreateMutation,
    variables: createVariables,
    response: createResult?.payload ?? null,
    downstreamRead: {
      operationName: 'CustomerInputInlineConsentRead',
      query: downstreamReadQuery,
      variables: customerId
        ? {
            id: customerId,
            identifier: { id: customerId },
            query: `email:${emailAddress}`,
          }
        : null,
      response: downstreamRead?.payload ?? null,
    },
  },
  validation: {
    updateEmailConsent: {
      operationName: 'CustomerInputInlineConsentUpdate',
      query: customerUpdateMutation,
      variables: customerId
        ? {
            input: {
              id: customerId,
              emailMarketingConsent: {
                marketingState: 'UNSUBSCRIBED',
                marketingOptInLevel: 'SINGLE_OPT_IN',
                consentUpdatedAt: '2026-04-25T02:00:00Z',
              },
            },
          }
        : null,
      response: updateEmailConsent?.payload ?? null,
    },
    updateSmsConsent: {
      operationName: 'CustomerInputInlineConsentUpdate',
      query: customerUpdateMutation,
      variables: customerId
        ? {
            input: {
              id: customerId,
              smsMarketingConsent: {
                marketingState: 'UNSUBSCRIBED',
                marketingOptInLevel: 'SINGLE_OPT_IN',
                consentUpdatedAt: '2026-04-25T02:05:00Z',
              },
            },
          }
        : null,
      response: updateSmsConsent?.payload ?? null,
    },
    downstreamAfterRejectedUpdates: {
      operationName: 'CustomerInputInlineConsentRead',
      query: downstreamReadQuery,
      variables: customerId
        ? {
            id: customerId,
            identifier: { id: customerId },
            query: `email:${emailAddress}`,
          }
        : null,
      response: downstreamAfterRejectedUpdates?.payload ?? null,
    },
  },
  cleanup,
};

const outputPath = path.join(outputDir, 'customer-input-inline-consent-parity.json');
await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');

console.log(
  JSON.stringify(
    {
      ok: true,
      outputPath,
      customerId,
      apiVersion,
      storeDomain,
    },
    null,
    2,
  ),
);

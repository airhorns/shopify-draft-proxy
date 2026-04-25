// @ts-nocheck
/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion);
const { runGraphqlRequest: runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

function assertNoTopLevelErrors(result, context) {
  if (result.status < 200 || result.status >= 300 || result.payload?.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

const consentCustomerSlice = `
  id
  firstName
  lastName
  displayName
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
`;

const createMutation = `#graphql
  mutation CustomerConsentCreate($input: CustomerInput!) {
    customerCreate(input: $input) {
      customer {
        ${consentCustomerSlice}
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const emailConsentMutation = `#graphql
  mutation CustomerEmailMarketingConsentUpdate($input: CustomerEmailMarketingConsentUpdateInput!) {
    customerEmailMarketingConsentUpdate(input: $input) {
      customer {
        ${consentCustomerSlice}
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const smsConsentMutation = `#graphql
  mutation CustomerSmsMarketingConsentUpdate($input: CustomerSmsMarketingConsentUpdateInput!) {
    customerSmsMarketingConsentUpdate(input: $input) {
      customer {
        ${consentCustomerSlice}
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const downstreamReadQuery = `#graphql
  query CustomerConsentDownstream($id: ID!, $identifier: CustomerIdentifierInput!, $query: String!, $first: Int!) {
    customer(id: $id) {
      ${consentCustomerSlice}
    }
    customerByIdentifier(identifier: $identifier) {
      ${consentCustomerSlice}
    }
    customers(first: $first, query: $query, sortKey: UPDATED_AT, reverse: true) {
      nodes {
        ${consentCustomerSlice}
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
      }
    }
  }
`;

const deleteMutation = `#graphql
  mutation CustomerConsentDelete($input: CustomerDeleteInput!) {
    customerDelete(input: $input) {
      deletedCustomerId
      userErrors {
        field
        message
      }
    }
  }
`;

async function main() {
  await mkdir(outputDir, { recursive: true });

  const stamp = Date.now();
  const emailAddress = `hermes-consent-${stamp}@example.com`;
  const phoneNumber = `+1415555${String(stamp).slice(-4).padStart(4, '0')}`;
  const createVariables = {
    input: {
      email: emailAddress,
      firstName: 'Hermes',
      lastName: 'Consent',
      phone: phoneNumber,
      tags: ['parity', `consent-${stamp}`],
    },
  };

  const createResult = await runGraphql(createMutation, createVariables);
  assertNoTopLevelErrors(createResult, 'customerCreate for consent parity');
  const customerId = createResult.payload?.data?.customerCreate?.customer?.id;
  if (typeof customerId !== 'string' || !customerId) {
    throw new Error(`customerCreate did not return a customer id: ${JSON.stringify(createResult.payload, null, 2)}`);
  }

  const emailConsentVariables = {
    input: {
      customerId,
      emailMarketingConsent: {
        marketingState: 'SUBSCRIBED',
        marketingOptInLevel: 'SINGLE_OPT_IN',
        consentUpdatedAt: '2026-04-25T01:00:00Z',
      },
    },
  };
  const emailConsentResult = await runGraphql(emailConsentMutation, emailConsentVariables);
  assertNoTopLevelErrors(emailConsentResult, 'customerEmailMarketingConsentUpdate');

  const emailDownstreamReadVariables = {
    id: customerId,
    identifier: { id: customerId },
    query: `email:${emailAddress}`,
    first: 5,
  };
  const emailDownstreamReadResult = await runGraphql(downstreamReadQuery, emailDownstreamReadVariables);
  assertNoTopLevelErrors(emailDownstreamReadResult, 'customer email consent downstream read');

  const smsConsentVariables = {
    input: {
      customerId,
      smsMarketingConsent: {
        marketingState: 'SUBSCRIBED',
        marketingOptInLevel: 'SINGLE_OPT_IN',
        consentUpdatedAt: '2026-04-25T01:05:00Z',
      },
    },
  };
  const smsConsentResult = await runGraphql(smsConsentMutation, smsConsentVariables);
  assertNoTopLevelErrors(smsConsentResult, 'customerSmsMarketingConsentUpdate');

  const smsDownstreamReadVariables = {
    id: customerId,
    identifier: { id: customerId },
    query: `email:${emailAddress}`,
    first: 5,
  };
  const smsDownstreamReadResult = await runGraphql(downstreamReadQuery, smsDownstreamReadVariables);
  assertNoTopLevelErrors(smsDownstreamReadResult, 'customer sms consent downstream read');

  const unknownEmailConsentVariables = {
    input: {
      customerId: 'gid://shopify/Customer/999999999999999',
      emailMarketingConsent: {
        marketingState: 'SUBSCRIBED',
        marketingOptInLevel: 'SINGLE_OPT_IN',
      },
    },
  };
  const unknownEmailConsentResult = await runGraphql(emailConsentMutation, unknownEmailConsentVariables);
  assertNoTopLevelErrors(unknownEmailConsentResult, 'customerEmailMarketingConsentUpdate unknown customer');

  const unknownSmsConsentVariables = {
    input: {
      customerId: 'gid://shopify/Customer/999999999999999',
      smsMarketingConsent: {
        marketingState: 'SUBSCRIBED',
        marketingOptInLevel: 'SINGLE_OPT_IN',
      },
    },
  };
  const unknownSmsConsentResult = await runGraphql(smsConsentMutation, unknownSmsConsentVariables);
  assertNoTopLevelErrors(unknownSmsConsentResult, 'customerSmsMarketingConsentUpdate unknown customer');

  const deleteResult = await runGraphql(deleteMutation, { input: { id: customerId } });
  assertNoTopLevelErrors(deleteResult, 'customerDelete cleanup for consent parity');

  const emailCapture = {
    precondition: {
      response: createResult.payload,
    },
    mutation: {
      variables: emailConsentVariables,
      response: emailConsentResult.payload,
    },
    downstreamRead: {
      variables: emailDownstreamReadVariables,
      response: emailDownstreamReadResult.payload,
    },
    validation: {
      unknownCustomer: {
        variables: unknownEmailConsentVariables,
        response: unknownEmailConsentResult.payload,
      },
    },
  };

  const smsCapture = {
    precondition: {
      response: emailConsentResult.payload,
    },
    mutation: {
      variables: smsConsentVariables,
      response: smsConsentResult.payload,
    },
    downstreamRead: {
      variables: smsDownstreamReadVariables,
      response: smsDownstreamReadResult.payload,
    },
    validation: {
      unknownCustomer: {
        variables: unknownSmsConsentVariables,
        response: unknownSmsConsentResult.payload,
      },
    },
    cleanup: {
      response: deleteResult.payload,
    },
  };

  await writeFile(
    path.join(outputDir, 'customer-email-marketing-consent-update-parity.json'),
    `${JSON.stringify(emailCapture, null, 2)}\n`,
    'utf8',
  );
  await writeFile(
    path.join(outputDir, 'customer-sms-marketing-consent-update-parity.json'),
    `${JSON.stringify(smsCapture, null, 2)}\n`,
    'utf8',
  );

  console.log(
    JSON.stringify(
      {
        ok: true,
        outputDir,
        files: [
          'customer-email-marketing-consent-update-parity.json',
          'customer-sms-marketing-consent-update-parity.json',
        ],
        customerId,
      },
      null,
      2,
    ),
  );
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : error);
  process.exit(1);
});

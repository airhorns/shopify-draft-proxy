// @ts-nocheck
/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'customers');
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

async function runCaptureCase(name, query, variables) {
  const result = await runGraphql(query, variables);
  if (result.status < 200 || result.status >= 300) {
    throw new Error(`${name} failed with HTTP ${result.status}: ${JSON.stringify(result, null, 2)}`);
  }

  return {
    name,
    variables,
    response: result.payload,
  };
}

async function main() {
  await mkdir(outputDir, { recursive: true });

  const stamp = Date.now();
  const emailAddress = `hermes-consent-${stamp}@example.com`;
  const phoneNumber = `+1415555${String(stamp).slice(-4).padStart(4, '0')}`;
  const transitionEmailAddress = `hermes-consent-transition-${stamp}@example.com`;
  const transitionPhoneNumber = `+1415666${String(stamp).slice(-4).padStart(4, '0')}`;
  const emailOnlyAddress = `hermes-consent-email-only-${stamp}@example.com`;
  const phoneOnlyNumber = `+1415777${String(stamp).slice(-4).padStart(4, '0')}`;
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

  const transitionCreateResult = await runGraphql(createMutation, {
    input: {
      email: transitionEmailAddress,
      firstName: 'Hermes',
      lastName: 'ConsentTransition',
      phone: transitionPhoneNumber,
      tags: ['parity', `consent-transition-${stamp}`],
    },
  });
  assertNoTopLevelErrors(transitionCreateResult, 'customerCreate for consent transition matrix');
  const transitionCustomerId = transitionCreateResult.payload?.data?.customerCreate?.customer?.id;
  if (typeof transitionCustomerId !== 'string' || !transitionCustomerId) {
    throw new Error(
      `transition customerCreate did not return a customer id: ${JSON.stringify(transitionCreateResult.payload, null, 2)}`,
    );
  }

  const emailOnlyCreateResult = await runGraphql(createMutation, {
    input: {
      email: emailOnlyAddress,
      firstName: 'Hermes',
      lastName: 'ConsentEmailOnly',
      tags: ['parity', `consent-email-only-${stamp}`],
    },
  });
  assertNoTopLevelErrors(emailOnlyCreateResult, 'customerCreate for consent email-only matrix');
  const emailOnlyCustomerId = emailOnlyCreateResult.payload?.data?.customerCreate?.customer?.id;
  if (typeof emailOnlyCustomerId !== 'string' || !emailOnlyCustomerId) {
    throw new Error(
      `email-only customerCreate did not return a customer id: ${JSON.stringify(emailOnlyCreateResult.payload, null, 2)}`,
    );
  }

  const phoneOnlyCreateResult = await runGraphql(createMutation, {
    input: {
      phone: phoneOnlyNumber,
      firstName: 'Hermes',
      lastName: 'ConsentPhoneOnly',
      tags: ['parity', `consent-phone-only-${stamp}`],
    },
  });
  assertNoTopLevelErrors(phoneOnlyCreateResult, 'customerCreate for consent phone-only matrix');
  const phoneOnlyCustomerId = phoneOnlyCreateResult.payload?.data?.customerCreate?.customer?.id;
  if (typeof phoneOnlyCustomerId !== 'string' || !phoneOnlyCustomerId) {
    throw new Error(
      `phone-only customerCreate did not return a customer id: ${JSON.stringify(phoneOnlyCreateResult.payload, null, 2)}`,
    );
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

  const emailValidationMatrix = [
    await runCaptureCase('missing consent payload', emailConsentMutation, {
      input: {
        customerId,
      },
    }),
    await runCaptureCase('null consent payload', emailConsentMutation, {
      input: {
        customerId,
        emailMarketingConsent: null,
      },
    }),
    await runCaptureCase('null marketingState', emailConsentMutation, {
      input: {
        customerId,
        emailMarketingConsent: {
          marketingState: null,
          marketingOptInLevel: 'SINGLE_OPT_IN',
        },
      },
    }),
    await runCaptureCase('invalid marketingOptInLevel', emailConsentMutation, {
      input: {
        customerId,
        emailMarketingConsent: {
          marketingState: 'SUBSCRIBED',
          marketingOptInLevel: 'BOGUS',
        },
      },
    }),
    await runCaptureCase('invalid consentUpdatedAt', emailConsentMutation, {
      input: {
        customerId,
        emailMarketingConsent: {
          marketingState: 'SUBSCRIBED',
          marketingOptInLevel: 'SINGLE_OPT_IN',
          consentUpdatedAt: 'not-a-date',
        },
      },
    }),
    await runCaptureCase('future consentUpdatedAt', emailConsentMutation, {
      input: {
        customerId,
        emailMarketingConsent: {
          marketingState: 'SUBSCRIBED',
          marketingOptInLevel: 'SINGLE_OPT_IN',
          consentUpdatedAt: '2999-01-01T00:00:00Z',
        },
      },
    }),
    await runCaptureCase('customer without email contact method', emailConsentMutation, {
      input: {
        customerId: phoneOnlyCustomerId,
        emailMarketingConsent: {
          marketingState: 'SUBSCRIBED',
          marketingOptInLevel: 'SINGLE_OPT_IN',
          consentUpdatedAt: '2026-04-25T02:02:00Z',
        },
      },
    }),
    await runCaptureCase('pending requires confirmed opt-in', emailConsentMutation, {
      input: {
        customerId: transitionCustomerId,
        emailMarketingConsent: {
          marketingState: 'PENDING',
          marketingOptInLevel: 'SINGLE_OPT_IN',
          consentUpdatedAt: '2026-04-25T02:03:00Z',
        },
      },
    }),
    await runCaptureCase('pending confirmed opt-in transition', emailConsentMutation, {
      input: {
        customerId: transitionCustomerId,
        emailMarketingConsent: {
          marketingState: 'PENDING',
          marketingOptInLevel: 'CONFIRMED_OPT_IN',
          consentUpdatedAt: '2026-04-25T02:04:00Z',
        },
      },
    }),
    await runCaptureCase('subscribed unknown opt-in transition', emailConsentMutation, {
      input: {
        customerId: transitionCustomerId,
        emailMarketingConsent: {
          marketingState: 'SUBSCRIBED',
          marketingOptInLevel: 'UNKNOWN',
          consentUpdatedAt: '2026-04-25T02:05:00Z',
        },
      },
    }),
    await runCaptureCase('unsubscribed transition', emailConsentMutation, {
      input: {
        customerId: transitionCustomerId,
        emailMarketingConsent: {
          marketingState: 'UNSUBSCRIBED',
          marketingOptInLevel: 'SINGLE_OPT_IN',
          consentUpdatedAt: '2026-04-25T02:06:00Z',
        },
      },
    }),
    await runCaptureCase('idempotent repeated update', emailConsentMutation, {
      input: {
        customerId: transitionCustomerId,
        emailMarketingConsent: {
          marketingState: 'UNSUBSCRIBED',
          marketingOptInLevel: 'SINGLE_OPT_IN',
          consentUpdatedAt: '2026-04-25T02:06:00Z',
        },
      },
    }),
    await runCaptureCase('not subscribed input state', emailConsentMutation, {
      input: {
        customerId: transitionCustomerId,
        emailMarketingConsent: {
          marketingState: 'NOT_SUBSCRIBED',
          marketingOptInLevel: 'SINGLE_OPT_IN',
          consentUpdatedAt: '2026-04-25T02:07:00Z',
        },
      },
    }),
    await runCaptureCase('redacted input state', emailConsentMutation, {
      input: {
        customerId: transitionCustomerId,
        emailMarketingConsent: {
          marketingState: 'REDACTED',
          marketingOptInLevel: 'SINGLE_OPT_IN',
          consentUpdatedAt: '2026-04-25T02:08:00Z',
        },
      },
    }),
    await runCaptureCase('invalid input state', emailConsentMutation, {
      input: {
        customerId: transitionCustomerId,
        emailMarketingConsent: {
          marketingState: 'INVALID',
          marketingOptInLevel: 'SINGLE_OPT_IN',
          consentUpdatedAt: '2026-04-25T02:09:00Z',
        },
      },
    }),
  ];

  const smsValidationMatrix = [
    await runCaptureCase('missing consent payload', smsConsentMutation, {
      input: {
        customerId,
      },
    }),
    await runCaptureCase('null consent payload', smsConsentMutation, {
      input: {
        customerId,
        smsMarketingConsent: null,
      },
    }),
    await runCaptureCase('null marketingState', smsConsentMutation, {
      input: {
        customerId,
        smsMarketingConsent: {
          marketingState: null,
          marketingOptInLevel: 'SINGLE_OPT_IN',
        },
      },
    }),
    await runCaptureCase('invalid marketingOptInLevel', smsConsentMutation, {
      input: {
        customerId,
        smsMarketingConsent: {
          marketingState: 'SUBSCRIBED',
          marketingOptInLevel: 'BOGUS',
        },
      },
    }),
    await runCaptureCase('invalid consentUpdatedAt', smsConsentMutation, {
      input: {
        customerId,
        smsMarketingConsent: {
          marketingState: 'SUBSCRIBED',
          marketingOptInLevel: 'SINGLE_OPT_IN',
          consentUpdatedAt: 'not-a-date',
        },
      },
    }),
    await runCaptureCase('future consentUpdatedAt', smsConsentMutation, {
      input: {
        customerId,
        smsMarketingConsent: {
          marketingState: 'SUBSCRIBED',
          marketingOptInLevel: 'SINGLE_OPT_IN',
          consentUpdatedAt: '2999-01-01T00:00:00Z',
        },
      },
    }),
    await runCaptureCase('customer without phone contact method', smsConsentMutation, {
      input: {
        customerId: emailOnlyCustomerId,
        smsMarketingConsent: {
          marketingState: 'SUBSCRIBED',
          marketingOptInLevel: 'SINGLE_OPT_IN',
          consentUpdatedAt: '2026-04-25T03:02:00Z',
        },
      },
    }),
    await runCaptureCase('unsupported consentCollectedFrom input field', smsConsentMutation, {
      input: {
        customerId,
        smsMarketingConsent: {
          marketingState: 'SUBSCRIBED',
          marketingOptInLevel: 'SINGLE_OPT_IN',
          consentCollectedFrom: 'SHOPIFY',
        },
      },
    }),
    await runCaptureCase('pending requires confirmed opt-in', smsConsentMutation, {
      input: {
        customerId: transitionCustomerId,
        smsMarketingConsent: {
          marketingState: 'PENDING',
          marketingOptInLevel: 'SINGLE_OPT_IN',
          consentUpdatedAt: '2026-04-25T03:03:00Z',
        },
      },
    }),
    await runCaptureCase('pending confirmed opt-in transition', smsConsentMutation, {
      input: {
        customerId: transitionCustomerId,
        smsMarketingConsent: {
          marketingState: 'PENDING',
          marketingOptInLevel: 'CONFIRMED_OPT_IN',
          consentUpdatedAt: '2026-04-25T03:04:00Z',
        },
      },
    }),
    await runCaptureCase('subscribed unknown opt-in transition', smsConsentMutation, {
      input: {
        customerId: transitionCustomerId,
        smsMarketingConsent: {
          marketingState: 'SUBSCRIBED',
          marketingOptInLevel: 'UNKNOWN',
          consentUpdatedAt: '2026-04-25T03:05:00Z',
        },
      },
    }),
    await runCaptureCase('unsubscribed transition', smsConsentMutation, {
      input: {
        customerId: transitionCustomerId,
        smsMarketingConsent: {
          marketingState: 'UNSUBSCRIBED',
          marketingOptInLevel: 'SINGLE_OPT_IN',
          consentUpdatedAt: '2026-04-25T03:06:00Z',
        },
      },
    }),
    await runCaptureCase('idempotent repeated update', smsConsentMutation, {
      input: {
        customerId: transitionCustomerId,
        smsMarketingConsent: {
          marketingState: 'UNSUBSCRIBED',
          marketingOptInLevel: 'SINGLE_OPT_IN',
          consentUpdatedAt: '2026-04-25T03:06:00Z',
        },
      },
    }),
    await runCaptureCase('not subscribed input state', smsConsentMutation, {
      input: {
        customerId: transitionCustomerId,
        smsMarketingConsent: {
          marketingState: 'NOT_SUBSCRIBED',
          marketingOptInLevel: 'SINGLE_OPT_IN',
          consentUpdatedAt: '2026-04-25T03:07:00Z',
        },
      },
    }),
    await runCaptureCase('redacted input state', smsConsentMutation, {
      input: {
        customerId: transitionCustomerId,
        smsMarketingConsent: {
          marketingState: 'REDACTED',
          marketingOptInLevel: 'SINGLE_OPT_IN',
          consentUpdatedAt: '2026-04-25T03:08:00Z',
        },
      },
    }),
  ];

  const deleteResult = await runGraphql(deleteMutation, { input: { id: customerId } });
  assertNoTopLevelErrors(deleteResult, 'customerDelete cleanup for consent parity');
  const transitionDeleteResult = await runGraphql(deleteMutation, { input: { id: transitionCustomerId } });
  assertNoTopLevelErrors(transitionDeleteResult, 'customerDelete cleanup for consent transition matrix');
  const emailOnlyDeleteResult = await runGraphql(deleteMutation, { input: { id: emailOnlyCustomerId } });
  assertNoTopLevelErrors(emailOnlyDeleteResult, 'customerDelete cleanup for consent email-only matrix');
  const phoneOnlyDeleteResult = await runGraphql(deleteMutation, { input: { id: phoneOnlyCustomerId } });
  assertNoTopLevelErrors(phoneOnlyDeleteResult, 'customerDelete cleanup for consent phone-only matrix');

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
      matrixPreconditions: {
        transitionCustomer: transitionCreateResult.payload,
        phoneOnlyCustomer: phoneOnlyCreateResult.payload,
      },
      matrix: emailValidationMatrix,
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
      matrixPreconditions: {
        transitionCustomer: transitionCreateResult.payload,
        emailOnlyCustomer: emailOnlyCreateResult.payload,
      },
      matrix: smsValidationMatrix,
    },
    cleanup: {
      response: deleteResult.payload,
      transitionCustomerResponse: transitionDeleteResult.payload,
      emailOnlyCustomerResponse: emailOnlyDeleteResult.payload,
      phoneOnlyCustomerResponse: phoneOnlyDeleteResult.payload,
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

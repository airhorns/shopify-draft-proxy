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

const customerSlice = `
  id
  email
  dataSaleOptOut
  defaultEmailAddress {
    emailAddress
  }
`;

const createMutation = `#graphql
  mutation DataSaleCustomerCreate($input: CustomerInput!) {
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

const dataSaleOptOutMutation = `#graphql
  mutation DataSaleOptOut($email: String!) {
    dataSaleOptOut(email: $email) {
      customerId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const downstreamReadQuery = `#graphql
  query DataSaleOptOutDownstream($id: ID!, $identifier: CustomerIdentifierInput!, $query: String!, $first: Int!) {
    customer(id: $id) {
      ${customerSlice}
    }
    customerByIdentifier(identifier: $identifier) {
      ${customerSlice}
    }
    customers(first: $first, query: $query, sortKey: UPDATED_AT, reverse: true) {
      nodes {
        ${customerSlice}
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
      }
    }
  }
`;

const deleteMutation = `#graphql
  mutation DataSaleCustomerDelete($input: CustomerDeleteInput!) {
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
  const emailAddress = `hermes-data-sale-${stamp}@example.com`;
  const unknownEmailAddress = `hermes-data-sale-new-${stamp}@example.com`;
  const createVariables = {
    input: {
      email: emailAddress,
      firstName: 'Hermes',
      lastName: 'DataSale',
      tags: ['parity', `data-sale-${stamp}`],
    },
  };

  const createResult = await runGraphql(createMutation, createVariables);
  assertNoTopLevelErrors(createResult, 'customerCreate for dataSaleOptOut parity');
  const customerId = createResult.payload?.data?.customerCreate?.customer?.id;
  if (typeof customerId !== 'string' || !customerId) {
    throw new Error(`customerCreate did not return a customer id: ${JSON.stringify(createResult.payload, null, 2)}`);
  }

  let unknownCustomerId = null;
  try {
    const mutationVariables = { email: emailAddress };
    const mutationResult = await runGraphql(dataSaleOptOutMutation, mutationVariables);
    assertNoTopLevelErrors(mutationResult, 'dataSaleOptOut existing customer');

    const downstreamReadVariables = {
      id: customerId,
      identifier: { id: customerId },
      query: `email:${emailAddress}`,
      first: 5,
    };
    const downstreamReadResult = await runGraphql(downstreamReadQuery, downstreamReadVariables);
    assertNoTopLevelErrors(downstreamReadResult, 'dataSaleOptOut downstream read');

    const repeatMutationResult = await runGraphql(dataSaleOptOutMutation, mutationVariables);
    assertNoTopLevelErrors(repeatMutationResult, 'dataSaleOptOut repeat');

    const invalidEmailVariables = { email: 'not-an-email' };
    const invalidEmailResult = await runGraphql(dataSaleOptOutMutation, invalidEmailVariables);
    assertNoTopLevelErrors(invalidEmailResult, 'dataSaleOptOut invalid email');

    const unknownEmailVariables = { email: unknownEmailAddress };
    const unknownEmailResult = await runGraphql(dataSaleOptOutMutation, unknownEmailVariables);
    assertNoTopLevelErrors(unknownEmailResult, 'dataSaleOptOut unknown email');
    unknownCustomerId = unknownEmailResult.payload?.data?.dataSaleOptOut?.customerId;
    if (typeof unknownCustomerId !== 'string' || !unknownCustomerId) {
      throw new Error(
        `dataSaleOptOut unknown email did not return a customer id: ${JSON.stringify(unknownEmailResult.payload, null, 2)}`,
      );
    }

    const unknownDownstreamReadVariables = {
      id: unknownCustomerId,
      identifier: { id: unknownCustomerId },
      query: `email:${unknownEmailAddress}`,
      first: 5,
    };
    const unknownDownstreamReadResult = await runGraphql(downstreamReadQuery, unknownDownstreamReadVariables);
    assertNoTopLevelErrors(unknownDownstreamReadResult, 'dataSaleOptOut unknown email downstream read');

    const cleanupExisting = await runGraphql(deleteMutation, { input: { id: customerId } });
    assertNoTopLevelErrors(cleanupExisting, 'dataSaleOptOut existing customer cleanup');
    const cleanupUnknown = await runGraphql(deleteMutation, { input: { id: unknownCustomerId } });
    assertNoTopLevelErrors(cleanupUnknown, 'dataSaleOptOut unknown customer cleanup');

    const capture = {
      precondition: {
        variables: createVariables,
        response: createResult.payload,
      },
      mutation: {
        variables: mutationVariables,
        response: mutationResult.payload,
      },
      downstreamRead: {
        variables: downstreamReadVariables,
        response: downstreamReadResult.payload,
      },
      validation: {
        repeat: {
          variables: mutationVariables,
          response: repeatMutationResult.payload,
        },
        invalidEmail: {
          variables: invalidEmailVariables,
          response: invalidEmailResult.payload,
        },
        unknownEmailCreatesCustomer: {
          variables: unknownEmailVariables,
          response: unknownEmailResult.payload,
          downstreamRead: {
            variables: unknownDownstreamReadVariables,
            response: unknownDownstreamReadResult.payload,
          },
        },
      },
      cleanup: {
        existingCustomer: {
          response: cleanupExisting.payload,
        },
        unknownEmailCustomer: {
          response: cleanupUnknown.payload,
        },
      },
    };

    await writeFile(
      path.join(outputDir, 'data-sale-opt-out-parity.json'),
      `${JSON.stringify(capture, null, 2)}\n`,
      'utf8',
    );

    console.log(
      JSON.stringify(
        {
          ok: true,
          outputDir,
          files: ['data-sale-opt-out-parity.json'],
          customerId,
          unknownCustomerId,
        },
        null,
        2,
      ),
    );
  } catch (error) {
    if (unknownCustomerId) {
      await runGraphql(deleteMutation, { input: { id: unknownCustomerId } });
    }
    await runGraphql(deleteMutation, { input: { id: customerId } });
    throw error;
  }
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : error);
  process.exit(1);
});

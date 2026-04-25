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
  firstName
  lastName
  displayName
  email
  locale
  note
  verifiedEmail
  taxExempt
  taxExemptions
  tags
  state
  canDelete
  loyalty: metafield(namespace: "custom", key: "loyalty") {
    id
    namespace
    key
    type
    value
  }
  metafields(first: 5) {
    nodes {
      id
      namespace
      key
      type
      value
    }
    pageInfo {
      hasNextPage
      hasPreviousPage
      startCursor
      endCursor
    }
  }
  defaultEmailAddress { emailAddress }
  defaultPhoneNumber { phoneNumber }
  defaultAddress { address1 city province country zip formattedArea }
  createdAt
  updatedAt
`;

const createMutation = `#graphql
  mutation CustomerCreateConformance($input: CustomerInput!) {
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

const updateMutation = `#graphql
  mutation CustomerUpdateConformance($input: CustomerInput!) {
    customerUpdate(input: $input) {
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

const deleteMutation = `#graphql
  mutation CustomerDeleteConformance($input: CustomerDeleteInput!) {
    customerDelete(input: $input) {
      deletedCustomerId
      shop {
        id
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const customerReadQuery = `#graphql
  query CustomerMutationDownstream($id: ID!, $query: String!, $first: Int!) {
    customer(id: $id) {
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
    customersCount {
      count
      precision
    }
  }
`;

async function main() {
  await mkdir(outputDir, { recursive: true });

  const stamp = Date.now();
  const createVariables = {
    input: {
      email: `hermes-customer-create-${stamp}@example.com`,
      firstName: 'Hermes',
      lastName: 'Create',
      locale: 'en',
      note: 'customer create parity probe',
      phone: '+14155550123',
      tags: ['parity', `create-${stamp}`],
      taxExempt: true,
    },
  };

  const createResult = await runGraphql(createMutation, createVariables);
  assertNoTopLevelErrors(createResult, 'customerCreate');
  const createdCustomer = createResult.payload?.data?.customerCreate?.customer;
  const createdCustomerId = createdCustomer?.id;
  if (typeof createdCustomerId !== 'string' || !createdCustomerId) {
    throw new Error(`customerCreate did not return a customer id: ${JSON.stringify(createResult.payload, null, 2)}`);
  }

  const createReadResult = await runGraphql(customerReadQuery, {
    id: createdCustomerId,
    query: `email:${createVariables.input.email}`,
    first: 5,
  });
  assertNoTopLevelErrors(createReadResult, 'customerCreate downstream read');

  const updateVariables = {
    input: {
      id: createdCustomerId,
      firstName: 'Hermes',
      lastName: 'Updated',
      note: 'customer update parity probe',
      tags: ['parity', 'updated'],
      taxExempt: false,
      taxExemptions: ['CA_BC_RESELLER_EXEMPTION'],
      metafields: [
        {
          namespace: 'custom',
          key: 'loyalty',
          type: 'single_line_text_field',
          value: `gold-${stamp}`,
        },
      ],
    },
  };

  const updateResult = await runGraphql(updateMutation, updateVariables);
  assertNoTopLevelErrors(updateResult, 'customerUpdate');
  const updateReadResult = await runGraphql(customerReadQuery, {
    id: createdCustomerId,
    query: 'tag:updated',
    first: 5,
  });
  assertNoTopLevelErrors(updateReadResult, 'customerUpdate downstream read');

  const createValidation = await runGraphql(createMutation, { input: { email: '' } });
  assertNoTopLevelErrors(createValidation, 'customerCreate validation');
  const updateValidation = await runGraphql(updateMutation, {
    input: { id: 'gid://shopify/Customer/999999999999999', firstName: 'Ghost' },
  });
  assertNoTopLevelErrors(updateValidation, 'customerUpdate validation');
  const updateMetafieldValidation = await runGraphql(updateMutation, {
    input: {
      id: createdCustomerId,
      metafields: [{ namespace: 'custom', key: 'bad_type', type: 'not_a_type', value: 'bad' }],
    },
  });
  assertNoTopLevelErrors(updateMetafieldValidation, 'customerUpdate metafield validation');
  const updateTaxExemptionValidation = await runGraphql(updateMutation, {
    input: {
      id: createdCustomerId,
      taxExemptions: ['NOT_A_TAX_EXEMPTION'],
    },
  });
  const deleteVariables = {
    input: {
      id: createdCustomerId,
    },
  };

  const deleteResult = await runGraphql(deleteMutation, deleteVariables);
  assertNoTopLevelErrors(deleteResult, 'customerDelete');
  const deleteReadResult = await runGraphql(customerReadQuery, {
    id: createdCustomerId,
    query: `email:${createVariables.input.email}`,
    first: 5,
  });
  assertNoTopLevelErrors(deleteReadResult, 'customerDelete downstream read');

  const deleteValidation = await runGraphql(deleteMutation, {
    input: { id: 'gid://shopify/Customer/999999999999999' },
  });
  assertNoTopLevelErrors(deleteValidation, 'customerDelete validation');

  const createCapture = {
    mutation: {
      variables: createVariables,
      response: createResult.payload,
    },
    downstreamRead: createReadResult.payload,
    validation: {
      variables: { input: { email: '' } },
      response: createValidation.payload,
    },
  };

  const updateCapture = {
    mutation: {
      variables: updateVariables,
      response: updateResult.payload,
    },
    downstreamRead: updateReadResult.payload,
    validation: {
      variables: { input: { id: 'gid://shopify/Customer/999999999999999', firstName: 'Ghost' } },
      response: updateValidation.payload,
    },
    metafieldValidation: {
      variables: {
        input: {
          id: createdCustomerId,
          metafields: [{ namespace: 'custom', key: 'bad_type', type: 'not_a_type', value: 'bad' }],
        },
      },
      response: updateMetafieldValidation.payload,
    },
    taxExemptionValidation: {
      variables: {
        input: {
          id: createdCustomerId,
          taxExemptions: ['NOT_A_TAX_EXEMPTION'],
        },
      },
      response: updateTaxExemptionValidation.payload,
    },
  };

  const deleteCapture = {
    mutation: {
      variables: deleteVariables,
      response: deleteResult.payload,
    },
    downstreamRead: deleteReadResult.payload,
    validation: {
      variables: { input: { id: 'gid://shopify/Customer/999999999999999' } },
      response: deleteValidation.payload,
    },
  };

  await Promise.all([
    writeFile(
      path.join(outputDir, 'customer-create-parity.json'),
      `${JSON.stringify(createCapture, null, 2)}\n`,
      'utf8',
    ),
    writeFile(
      path.join(outputDir, 'customer-update-parity.json'),
      `${JSON.stringify(updateCapture, null, 2)}\n`,
      'utf8',
    ),
    writeFile(
      path.join(outputDir, 'customer-delete-parity.json'),
      `${JSON.stringify(deleteCapture, null, 2)}\n`,
      'utf8',
    ),
  ]);

  console.log(
    JSON.stringify(
      {
        ok: true,
        outputDir,
        files: ['customer-create-parity.json', 'customer-update-parity.json', 'customer-delete-parity.json'],
        customerId: createdCustomerId,
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

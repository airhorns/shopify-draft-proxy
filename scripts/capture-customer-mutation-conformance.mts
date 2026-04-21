// @ts-nocheck
/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const requiredVars = ['SHOPIFY_CONFORMANCE_STORE_DOMAIN', 'SHOPIFY_CONFORMANCE_ADMIN_ORIGIN'];

const missingVars = requiredVars.filter((name) => !process.env[name]);
if (missingVars.length > 0) {
  console.error(`Missing required environment variables: ${missingVars.join(', ')}`);
  process.exit(1);
}

const storeDomain = process.env['SHOPIFY_CONFORMANCE_STORE_DOMAIN'];
const adminOrigin = process.env['SHOPIFY_CONFORMANCE_ADMIN_ORIGIN'];
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const apiVersion = process.env['SHOPIFY_CONFORMANCE_API_VERSION'] || '2025-01';
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion);

async function runGraphql(query, variables = {}) {
  const response = await fetch(`${adminOrigin}/admin/api/${apiVersion}/graphql.json`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      ...buildAdminAuthHeaders(adminAccessToken),
    },
    body: JSON.stringify({ query, variables }),
  });

  return {
    status: response.status,
    payload: await response.json(),
  };
}

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
  tags
  state
  canDelete
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

  const createValidation = await runGraphql(createMutation, { input: { email: '' } });
  assertNoTopLevelErrors(createValidation, 'customerCreate validation');
  const updateValidation = await runGraphql(updateMutation, {
    input: { id: 'gid://shopify/Customer/999999999999999', firstName: 'Ghost' },
  });
  assertNoTopLevelErrors(updateValidation, 'customerUpdate validation');
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

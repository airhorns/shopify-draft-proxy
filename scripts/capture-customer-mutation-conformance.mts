// @ts-nocheck
/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'customers');
const { runGraphqlRequest: runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

// customerDelete resolves the pre-existing customer the real way: on an overlay miss
// `hydrate_customer_for_mutation` forwards CUSTOMER_HYDRATE_QUERY upstream and observes it,
// and the downstream `customersCount` overlay forwards CUSTOMER_COUNT_HYDRATE_QUERY. These
// are the same constants the runtime emits (include_str! of the files below), so the
// recorded cassette entries byte-match what the proxy forwards. No seeding required.
const customerMutationHydrateDocument = await readFile(
  'config/parity-requests/customers/customer-mutation-hydrate.graphql',
  'utf8',
);
const customerCountHydrateDocument = await readFile(
  'config/parity-requests/customers/customer-count-hydrate.graphql',
  'utf8',
);
const customerDeleteShopHydrateDocument = await readFile(
  'config/parity-requests/customers/customer-delete-shop-hydrate.graphql',
  'utf8',
);
// customerCreate forwards CUSTOMER_DUPLICATE_HYDRATE_QUERY once per uniqueness-bearing input
// field (email, phone) to decide TAKEN the real way instead of from seeded state. The staged
// create is never committed upstream, so at parity time these lookups see no match — capture
// them BEFORE creating so the recorded nodes are empty and the create proceeds.
const customerDuplicateHydrateDocument = await readFile(
  'config/parity-requests/customers/customer-duplicate-hydrate.graphql',
  'utf8',
);
const UNKNOWN_CUSTOMER_GID = 'gid://shopify/Customer/999999999999999';

async function captureDuplicateHydrate(query, context) {
  const result = await runGraphql(customerDuplicateHydrateDocument, { query });
  assertNoTopLevelErrors(result, context);
  return result.payload;
}

function duplicateUpstreamCall(query, payload) {
  return {
    operationName: 'CustomerDuplicateHydrate',
    variables: { query },
    query: customerDuplicateHydrateDocument,
    response: { status: 200, body: payload },
  };
}

// Forward CUSTOMER_HYDRATE_QUERY upstream and capture the live response for the customer
// at its current state. A null customer (the unknown gid) is a valid hydrate result.
async function captureMutationHydrate(id, context) {
  const result = await runGraphql(customerMutationHydrateDocument, { id });
  assertNoTopLevelErrors(result, context);
  return result.payload;
}

function hydrateUpstreamCall(id, payload) {
  return {
    operationName: 'CustomerHydrate',
    variables: { id },
    query: customerMutationHydrateDocument,
    response: { status: 200, body: payload },
  };
}

async function captureCustomerDeleteShopHydrate() {
  const result = await runGraphql(customerDeleteShopHydrateDocument, {});
  assertNoTopLevelErrors(result, 'customerDelete shop hydrate');
  return result.payload;
}

function customerDeleteShopHydrateUpstreamCall(payload) {
  return {
    operationName: 'CustomerDeleteShopHydrate',
    variables: {},
    query: customerDeleteShopHydrateDocument,
    response: { status: 200, body: payload },
  };
}

// The customersCount overlay reads the live base via CUSTOMER_COUNT_HYDRATE_QUERY and
// applies its local net delta (−1 per staged delete). When the proxy forwards this read
// during parity the customer is still present upstream (parity never commits), so the live
// base is exactly one higher than the post-delete count the downstream read asserts. Shopify's
// customersCount is eventually-consistent, so we reconstruct that base from the asserted
// post-delete count + the staged-delete count rather than trusting a separate live read that
// may lag. This keeps the assertion a real captured value while the cassette base reflects the
// count the proxy genuinely observes mid-scenario.
function countUpstreamCall(customersCount) {
  return {
    operationName: 'CustomerCountHydrate',
    variables: {},
    query: customerCountHydrateDocument,
    response: { status: 200, body: { data: { customersCount } } },
  };
}

function countBaseFromAsserted(assertedCustomersCount, stagedDeletes) {
  if (!assertedCustomersCount || typeof assertedCustomersCount.count !== 'number') {
    return null;
  }
  return { ...assertedCustomersCount, count: assertedCustomersCount.count + stagedDeletes };
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
      phone: `+1415555${String(stamp).slice(-4).padStart(4, '0')}`,
      tags: ['parity', `create-${stamp}`],
      taxExempt: true,
    },
  };

  // Capture the uniqueness lookups the create forwards, BEFORE the customer exists, so the
  // recorded nodes are empty (no duplicate) and the proxy lets the create through at parity time.
  const emailDuplicateQuery = `email:${createVariables.input.email}`;
  const phoneDuplicateQuery = `phone:${createVariables.input.phone}`;
  const emailDuplicatePayload = await captureDuplicateHydrate(emailDuplicateQuery, 'customerCreate email dedupe');
  const phoneDuplicatePayload = await captureDuplicateHydrate(phoneDuplicateQuery, 'customerCreate phone dedupe');

  const createResult = await runGraphql(createMutation, createVariables);
  assertNoTopLevelErrors(createResult, 'customerCreate');
  const createdCustomer = createResult.payload?.data?.customerCreate?.customer;
  const createdCustomerId = createdCustomer?.id;
  if (typeof createdCustomerId !== 'string' || !createdCustomerId) {
    throw new Error(`customerCreate did not return a customer id: ${JSON.stringify(createResult.payload, null, 2)}`);
  }

  // The downstream-read targets all query with the `__customer_parity_no_match__` sentinel so
  // the `customers` connection returns nothing — the proxy can serve `customer(id:)` and
  // `customersCount` from its overlay but cannot deterministically reproduce a live search list.
  // Capture with the same sentinel so the recorded `customers.nodes` is empty too.
  const NO_MATCH_QUERY = '__customer_parity_no_match__';
  const createReadResult = await runGraphql(customerReadQuery, {
    id: createdCustomerId,
    query: NO_MATCH_QUERY,
    first: 5,
  });
  assertNoTopLevelErrors(createReadResult, 'customerCreate downstream read');

  // customerUpdate hydrates the pre-existing customer via CUSTOMER_HYDRATE_QUERY before applying
  // the staged update. Capture that hydrate at the post-create / pre-update state so the proxy
  // reproduces the same merge result at parity time.
  const updateHydratePayload = await captureMutationHydrate(createdCustomerId, 'customerUpdate hydrate');

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
    query: NO_MATCH_QUERY,
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
  // Capture the upstream reads customerDelete forwards, at the pre-delete state: the
  // mutation hydrates the target customer and the unknown-id validation hydrates to null.
  // The count base is reconstructed from the post-delete downstream read below.
  const deleteHydratePayload = await captureMutationHydrate(createdCustomerId, 'customerDelete hydrate');
  const deleteShopHydratePayload = await captureCustomerDeleteShopHydrate();
  const unknownHydratePayload = await captureMutationHydrate(UNKNOWN_CUSTOMER_GID, 'customerDelete unknown hydrate');

  const deleteVariables = {
    input: {
      id: createdCustomerId,
    },
  };

  const deleteResult = await runGraphql(deleteMutation, deleteVariables);
  assertNoTopLevelErrors(deleteResult, 'customerDelete');
  const deleteReadResult = await runGraphql(customerReadQuery, {
    id: createdCustomerId,
    query: NO_MATCH_QUERY,
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
    upstreamCalls: [
      duplicateUpstreamCall(emailDuplicateQuery, emailDuplicatePayload),
      duplicateUpstreamCall(phoneDuplicateQuery, phoneDuplicatePayload),
      countUpstreamCall(countBaseFromAsserted(createReadResult.payload?.data?.customersCount, 0)),
    ],
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
    upstreamCalls: [
      hydrateUpstreamCall(createdCustomerId, updateHydratePayload),
      countUpstreamCall(countBaseFromAsserted(updateReadResult.payload?.data?.customersCount, 0)),
      hydrateUpstreamCall(UNKNOWN_CUSTOMER_GID, unknownHydratePayload),
    ],
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
    upstreamCalls: [
      hydrateUpstreamCall(createdCustomerId, deleteHydratePayload),
      customerDeleteShopHydrateUpstreamCall(deleteShopHydratePayload),
      countUpstreamCall(countBaseFromAsserted(deleteReadResult.payload?.data?.customersCount, 1)),
      hydrateUpstreamCall(UNKNOWN_CUSTOMER_GID, unknownHydratePayload),
    ],
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

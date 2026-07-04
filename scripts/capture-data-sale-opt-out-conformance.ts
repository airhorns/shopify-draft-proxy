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
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'privacy');
const { runGraphqlRequest: runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

// dataSaleOptOut resolves a pre-existing customer by email the real way: the proxy forwards
// this exact DataSaleOptOutCustomerLookup query (include_str! of the same file in privacy.rs)
// and observes the result. Recording the live forward here lets the cassette byte-match the
// proxy's request, so no seeded customer is required.
const customerLookupDocument = await readFile(
  'config/parity-requests/privacy/data-sale-opt-out-customer-lookup.graphql',
  'utf8',
);

// When re-recording the 2025-01 privacy captures to de-seed data-sale-opt-out-parity, skip the
// new-customer-defaults capture (it lives at 2026-04 and depends on a slow tag-search indexing
// poll), so this run regenerates only the missing-email / parity / whitespace 2025-01 fixtures.
const skipNewCustomerDefaults = process.env.SHOPIFY_CONFORMANCE_CAPTURE_DATA_SALE_OPT_OUT_SKIP_NEW_DEFAULTS === 'true';
const newCustomerDefaultsOnly = process.env.SHOPIFY_CONFORMANCE_CAPTURE_DATA_SALE_OPT_OUT_NEW_DEFAULTS_ONLY === 'true';
const invalidFormatOnly = process.env.SHOPIFY_CONFORMANCE_CAPTURE_DATA_SALE_OPT_OUT_INVALID_FORMAT_ONLY === 'true';
const strictFormatResidualOnly =
  process.env.SHOPIFY_CONFORMANCE_CAPTURE_DATA_SALE_OPT_OUT_STRICT_FORMAT_RESIDUAL_ONLY === 'true';
const unicodeLetterOnly = process.env.SHOPIFY_CONFORMANCE_CAPTURE_DATA_SALE_OPT_OUT_UNICODE_LETTER_ONLY === 'true';

async function captureCustomerLookup(emailAddress, context) {
  const result = await runGraphql(customerLookupDocument, { identifier: { emailAddress } });
  assertNoTopLevelErrors(result, context);
  return result.payload;
}

function customerLookupUpstreamCall(emailAddress, payload) {
  return {
    operationName: 'DataSaleOptOutCustomerLookup',
    variables: { identifier: { emailAddress } },
    query: customerLookupDocument,
    response: { status: 200, body: payload },
  };
}

function assertNoTopLevelErrors(result, context) {
  if (result.status < 200 || result.status >= 300 || result.payload?.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertDataSaleOptOutFailed(payload, context) {
  const expected = {
    customerId: null,
    userErrors: [
      {
        field: null,
        message: 'Data sale opt out failed.',
        code: 'FAILED',
      },
    ],
  };
  if (JSON.stringify(payload) !== JSON.stringify(expected)) {
    throw new Error(`${context} did not return FAILED payload: ${JSON.stringify(payload, null, 2)}`);
  }
}

function assertDataSaleOptOutSucceeded(payload, context) {
  if (
    typeof payload?.customerId !== 'string' ||
    payload.customerId.length === 0 ||
    !Array.isArray(payload?.userErrors) ||
    payload.userErrors.length !== 0
  ) {
    throw new Error(`${context} did not return SUCCESS payload: ${JSON.stringify(payload, null, 2)}`);
  }
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function customerNodes(result) {
  const nodes = result.payload?.data?.customers?.nodes;
  return Array.isArray(nodes) ? nodes : [];
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

const dataSaleOptOutMissingEmailMutation = `#graphql
  mutation DataSaleOptOutMissingEmail {
    dataSaleOptOut {
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

const whitespaceDownstreamReadQuery = `#graphql
  query DataSaleOptOutWhitespaceDownstream($id: ID!, $identifier: CustomerIdentifierInput!) {
    customer(id: $id) {
      ${customerSlice}
    }
    customerByIdentifier(identifier: $identifier) {
      ${customerSlice}
    }
  }
`;

const newCustomerDefaultsReadQuery = `#graphql
  query DataSaleOptOutNewCustomerDefaultsRead($id: ID!) {
    customer(id: $id) {
      id
      email
      tags
      locale
      verifiedEmail
      state
      createdAt
      updatedAt
      defaultEmailAddress {
        emailAddress
      }
    }
  }
`;

const dnsTagSearchQuery = `#graphql
  query DataSaleOptOutDnsTagSearch($query: String!, $first: Int!) {
    customers(query: $query, first: $first) {
      nodes {
        id
        email
        tags
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
  }
`;

async function runDnsTagSearchUntilCustomer(customerId, variables) {
  let lastResult = null;
  for (let attempt = 0; attempt < 10; attempt += 1) {
    const result = await runGraphql(dnsTagSearchQuery, variables);
    assertNoTopLevelErrors(result, 'dataSaleOptOut new customer defaults tag search');
    lastResult = result;
    if (customerNodes(result).some((node) => node?.id === customerId)) {
      return result;
    }
    await sleep(1000);
  }
  throw new Error(
    `dataSaleOptOut tag search did not find ${customerId}: ${JSON.stringify(lastResult?.payload, null, 2)}`,
  );
}

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

const invalidFormatCases = [
  {
    name: 'leadingDotLocal',
    variables: { email: '.me@example.com' },
  },
  {
    name: 'trailingDotLocal',
    variables: { email: 'me.@example.com' },
  },
  {
    name: 'consecutiveDotLocal',
    variables: { email: 'me..example@example.com' },
  },
  {
    name: 'consecutiveDotDomain',
    variables: { email: 'me@example..com' },
  },
  {
    name: 'leadingDashDomainLabel',
    variables: { email: 'me@-example.com' },
  },
  {
    name: 'trailingDashDomainLabel',
    variables: { email: 'me@example-.com' },
  },
  {
    name: 'ipv4LiteralDomain',
    variables: { email: 'me@8.8.8.8' },
  },
  {
    name: 'emojiLocal',
    variables: { email: '💩💩💩@example.com' },
  },
  {
    name: 'invalidDomainChars',
    variables: { email: '#@%^%#.com' },
  },
  {
    name: 'displayNameComment',
    variables: { email: 'me@example.com (First Name)' },
  },
  {
    name: 'over255Length',
    variables: { email: `${'a'.repeat(244)}@example.com` },
  },
];

async function captureInvalidFormatCases() {
  const invalidFormats = {};
  const accidentalCustomerIds = [];
  try {
    for (const testCase of invalidFormatCases) {
      const result = await runGraphql(dataSaleOptOutMutation, testCase.variables);
      assertNoTopLevelErrors(result, `dataSaleOptOut invalid format ${testCase.name}`);
      const payload = result.payload?.data?.dataSaleOptOut;
      const customerId = payload?.customerId;
      if (typeof customerId === 'string' && customerId) {
        accidentalCustomerIds.push(customerId);
      }
      assertDataSaleOptOutFailed(payload, `dataSaleOptOut invalid format ${testCase.name}`);
      invalidFormats[testCase.name] = {
        variables: testCase.variables,
        response: result.payload,
      };
    }
  } finally {
    for (const customerId of accidentalCustomerIds) {
      await runGraphql(deleteMutation, { input: { id: customerId } });
    }
  }
  return {
    setup: {
      seededCustomers: false,
      note: 'Format-validation-only capture; Core rejects before creating or updating a customer.',
    },
    mutation: invalidFormats.leadingDotLocal,
    validation: {
      invalidFormats,
    },
    cleanup: {
      accidentalCustomerIds,
    },
    upstreamCalls: [],
  };
}

async function writeInvalidFormatCapture() {
  const invalidFormatCapture = await captureInvalidFormatCases();
  await writeFile(
    path.join(outputDir, 'data-sale-opt-out-invalid-format.json'),
    `${JSON.stringify(invalidFormatCapture, null, 2)}\n`,
    'utf8',
  );
  return invalidFormatCapture;
}

const strictFormatResidualCases = [
  {
    name: 'digitTld',
    variables: { email: 'foo@bar.co2' },
    expected: 'failed',
  },
  {
    name: 'digitInTld',
    variables: { email: 'user@example.c0m' },
    expected: 'failed',
  },
  {
    name: 'hyphenInTld',
    variables: { email: 'user@example.c-o' },
    expected: 'failed',
  },
  {
    name: 'localOver128',
    variables: { email: `${'a'.repeat(200)}@e.co` },
    expected: 'failed',
  },
  {
    name: 'quotedLocalAtom',
    variables: { email: 'ab"cd@example.com' },
    expected: 'success',
  },
];

async function deleteCustomerIfPresent(customerId, context) {
  if (typeof customerId !== 'string' || customerId.length === 0) {
    return null;
  }
  const result = await runGraphql(deleteMutation, { input: { id: customerId } });
  assertNoTopLevelErrors(result, context);
  return result.payload;
}

async function captureStrictFormatResidualCases() {
  const residualFormats = {};
  const cleanup = {};
  const upstreamCalls = [];
  let createdCustomerId = null;

  for (const testCase of strictFormatResidualCases) {
    if (testCase.expected === 'success') {
      const setupLookup = await captureCustomerLookup(
        testCase.variables.email,
        `dataSaleOptOut strict format residual ${testCase.name} setup lookup`,
      );
      const setupCustomerId = setupLookup?.data?.customerByIdentifier?.id;
      const setupCleanup = await deleteCustomerIfPresent(
        setupCustomerId,
        `dataSaleOptOut strict format residual ${testCase.name} setup cleanup`,
      );
      if (setupCleanup) {
        cleanup[`${testCase.name}PreexistingCustomer`] = { response: setupCleanup };
      }

      const lookupPayload = await captureCustomerLookup(
        testCase.variables.email,
        `dataSaleOptOut strict format residual ${testCase.name} lookup`,
      );
      upstreamCalls.push(customerLookupUpstreamCall(testCase.variables.email, lookupPayload));
      const result = await runGraphql(dataSaleOptOutMutation, testCase.variables);
      assertNoTopLevelErrors(result, `dataSaleOptOut strict format residual ${testCase.name}`);
      const payload = result.payload?.data?.dataSaleOptOut;
      assertDataSaleOptOutSucceeded(payload, `dataSaleOptOut strict format residual ${testCase.name}`);
      createdCustomerId = payload.customerId;
      residualFormats[testCase.name] = {
        variables: testCase.variables,
        response: result.payload,
      };
      continue;
    }

    const result = await runGraphql(dataSaleOptOutMutation, testCase.variables);
    assertNoTopLevelErrors(result, `dataSaleOptOut strict format residual ${testCase.name}`);
    const payload = result.payload?.data?.dataSaleOptOut;
    assertDataSaleOptOutFailed(payload, `dataSaleOptOut strict format residual ${testCase.name}`);
    residualFormats[testCase.name] = {
      variables: testCase.variables,
      response: result.payload,
    };
  }

  try {
    const response = await deleteCustomerIfPresent(
      createdCustomerId,
      'dataSaleOptOut strict format residual quoted local customer cleanup',
    );
    if (response) {
      cleanup.quotedLocalAtomCustomer = { response };
    }
  } finally {
    createdCustomerId = null;
  }

  return {
    setup: {
      seededCustomers: false,
      note: 'Strict-format residual capture. Rejected branches require no setup; the quoted local atom success creates a disposable opted-out customer and deletes it during cleanup.',
    },
    mutation: residualFormats.digitTld,
    validation: {
      residualFormats,
    },
    cleanup,
    upstreamCalls,
  };
}

async function writeStrictFormatResidualCapture() {
  const strictFormatResidualCapture = await captureStrictFormatResidualCases();
  await writeFile(
    path.join(outputDir, 'data-sale-opt-out-strict-format-residual.json'),
    `${JSON.stringify(strictFormatResidualCapture, null, 2)}\n`,
    'utf8',
  );
  return strictFormatResidualCapture;
}

async function captureNewCustomerDefaults(stamp) {
  const newDefaultsEmailAddress = `hermes-data-sale-defaults-${stamp}@example.com`;
  const newDefaultsMutationVariables = { email: newDefaultsEmailAddress };
  let newDefaultsCustomerId = null;
  try {
    // The proxy forwards the lookup with this fresh email before creating the customer, so the
    // recorded lookup returns null and the proxy stages a new synthetic customer to match.
    const newDefaultsLookupPayload = await captureCustomerLookup(
      newDefaultsEmailAddress,
      'dataSaleOptOut new customer defaults lookup',
    );
    const newDefaultsMutationResult = await runGraphql(dataSaleOptOutMutation, newDefaultsMutationVariables);
    assertNoTopLevelErrors(newDefaultsMutationResult, 'dataSaleOptOut new customer defaults mutation');
    newDefaultsCustomerId = newDefaultsMutationResult.payload?.data?.dataSaleOptOut?.customerId;
    if (typeof newDefaultsCustomerId !== 'string' || !newDefaultsCustomerId) {
      throw new Error(
        `dataSaleOptOut new customer defaults did not return a customer id: ${JSON.stringify(
          newDefaultsMutationResult.payload,
          null,
          2,
        )}`,
      );
    }

    const newDefaultsDownstreamReadVariables = { id: newDefaultsCustomerId };
    const newDefaultsDownstreamReadResult = await runGraphql(
      newCustomerDefaultsReadQuery,
      newDefaultsDownstreamReadVariables,
    );
    assertNoTopLevelErrors(newDefaultsDownstreamReadResult, 'dataSaleOptOut new customer defaults downstream read');

    const newDefaultsTagSearchVariables = {
      query: 'tag:created-by-dns-form',
      first: 5,
    };
    const newDefaultsTagSearchResult = await runDnsTagSearchUntilCustomer(
      newDefaultsCustomerId,
      newDefaultsTagSearchVariables,
    );

    const cleanupNewDefaults = await runGraphql(deleteMutation, { input: { id: newDefaultsCustomerId } });
    assertNoTopLevelErrors(cleanupNewDefaults, 'dataSaleOptOut new customer defaults cleanup');

    return {
      mutation: {
        variables: newDefaultsMutationVariables,
        response: newDefaultsMutationResult.payload,
      },
      downstreamRead: {
        variables: newDefaultsDownstreamReadVariables,
        response: newDefaultsDownstreamReadResult.payload,
      },
      tagSearchRead: {
        variables: newDefaultsTagSearchVariables,
        response: newDefaultsTagSearchResult.payload,
      },
      cleanup: {
        unknownEmailCustomer: {
          response: cleanupNewDefaults.payload,
        },
      },
      upstreamCalls: [customerLookupUpstreamCall(newDefaultsEmailAddress, newDefaultsLookupPayload)],
    };
  } catch (error) {
    await deleteCustomerIfPresent(newDefaultsCustomerId, 'dataSaleOptOut new customer defaults cleanup after failure');
    throw error;
  }
}

async function writeNewCustomerDefaultsCapture(stamp) {
  const newCustomerDefaultsCapture = await captureNewCustomerDefaults(stamp);
  await writeFile(
    path.join(outputDir, 'data-sale-opt-out-new-customer-defaults.json'),
    `${JSON.stringify(newCustomerDefaultsCapture, null, 2)}\n`,
    'utf8',
  );
  return newCustomerDefaultsCapture;
}

async function deletePreexistingCustomerByEmail(emailAddress, context) {
  const lookupPayload = await captureCustomerLookup(emailAddress, `${context} preexisting lookup`);
  const customerId = lookupPayload?.data?.customerByIdentifier?.id;
  const response = await deleteCustomerIfPresent(customerId, `${context} preexisting customer cleanup`);
  return response ? { customerId, response } : null;
}

async function captureUnicodeLetterEmailCase({ name, emailAddress }) {
  const preexistingCleanup = await deletePreexistingCustomerByEmail(
    emailAddress,
    `dataSaleOptOut unicode letter ${name}`,
  );
  const lookupPayload = await captureCustomerLookup(emailAddress, `dataSaleOptOut unicode letter ${name} lookup`);
  const mutationVariables = { email: emailAddress };
  const mutationResult = await runGraphql(dataSaleOptOutMutation, mutationVariables);
  assertNoTopLevelErrors(mutationResult, `dataSaleOptOut unicode letter ${name} mutation`);
  const payload = mutationResult.payload?.data?.dataSaleOptOut;
  assertDataSaleOptOutSucceeded(payload, `dataSaleOptOut unicode letter ${name}`);
  const customerId = payload.customerId;

  try {
    const downstreamReadVariables = {
      id: customerId,
      identifier: { id: customerId },
      query: '__customer_parity_no_match__',
      first: 5,
    };
    const downstreamReadResult = await runGraphql(downstreamReadQuery, downstreamReadVariables);
    assertNoTopLevelErrors(downstreamReadResult, `dataSaleOptOut unicode letter ${name} downstream read`);
    const customer = downstreamReadResult.payload?.data?.customer;
    const byIdentifier = downstreamReadResult.payload?.data?.customerByIdentifier;
    if (customer?.dataSaleOptOut !== true || byIdentifier?.dataSaleOptOut !== true) {
      throw new Error(
        `dataSaleOptOut unicode letter ${name} downstream read did not show opt-out: ${JSON.stringify(
          downstreamReadResult.payload,
          null,
          2,
        )}`,
      );
    }

    const cleanup = await deleteCustomerIfPresent(customerId, `dataSaleOptOut unicode letter ${name} cleanup`);
    return {
      setup: {
        preexistingCleanup,
      },
      mutation: {
        variables: mutationVariables,
        response: mutationResult.payload,
      },
      downstreamRead: {
        variables: downstreamReadVariables,
        response: downstreamReadResult.payload,
      },
      cleanup: {
        customer: {
          response: cleanup,
        },
      },
      upstreamCall: customerLookupUpstreamCall(emailAddress, lookupPayload),
    };
  } catch (error) {
    await deleteCustomerIfPresent(customerId, `dataSaleOptOut unicode letter ${name} cleanup after failure`);
    throw error;
  }
}

async function captureUnicodeLetterEmailCases() {
  const accentedLatin = await captureUnicodeLetterEmailCase({
    name: 'accented latin',
    emailAddress: 'héllo@example.com',
  });
  const cjk = await captureUnicodeLetterEmailCase({
    name: 'cjk local',
    emailAddress: '日本@example.com',
  });
  return {
    setup: {
      seededCustomers: false,
      note: 'Unicode-letter email capture. Fixed test emails are cleaned up before mutation, Shopify creates disposable opted-out customers, and the script deletes them after downstream reads.',
      accentedLatin: accentedLatin.setup,
      cjk: cjk.setup,
    },
    mutation: accentedLatin.mutation,
    downstreamRead: accentedLatin.downstreamRead,
    validation: {
      cjk: {
        mutation: cjk.mutation,
        downstreamRead: cjk.downstreamRead,
      },
    },
    cleanup: {
      accentedLatin: accentedLatin.cleanup,
      cjk: cjk.cleanup,
    },
    upstreamCalls: [accentedLatin.upstreamCall, cjk.upstreamCall],
  };
}

async function writeUnicodeLetterEmailCapture() {
  const unicodeLetterEmailCapture = await captureUnicodeLetterEmailCases();
  await writeFile(
    path.join(outputDir, 'data-sale-opt-out-unicode-letter-email.json'),
    `${JSON.stringify(unicodeLetterEmailCapture, null, 2)}\n`,
    'utf8',
  );
  return unicodeLetterEmailCapture;
}

async function main() {
  await mkdir(outputDir, { recursive: true });

  if (newCustomerDefaultsOnly) {
    await writeNewCustomerDefaultsCapture(Date.now());
    console.log(
      JSON.stringify(
        {
          ok: true,
          outputDir,
          files: ['data-sale-opt-out-new-customer-defaults.json'],
        },
        null,
        2,
      ),
    );
    return;
  }

  if (unicodeLetterOnly) {
    await writeUnicodeLetterEmailCapture();
    console.log(
      JSON.stringify(
        {
          ok: true,
          outputDir,
          files: ['data-sale-opt-out-unicode-letter-email.json'],
        },
        null,
        2,
      ),
    );
    return;
  }

  if (invalidFormatOnly) {
    await writeInvalidFormatCapture();
    console.log(
      JSON.stringify(
        {
          ok: true,
          outputDir,
          files: ['data-sale-opt-out-invalid-format.json'],
        },
        null,
        2,
      ),
    );
    return;
  }
  if (strictFormatResidualOnly) {
    await writeStrictFormatResidualCapture();
    console.log(
      JSON.stringify(
        {
          ok: true,
          outputDir,
          files: ['data-sale-opt-out-strict-format-residual.json'],
        },
        null,
        2,
      ),
    );
    return;
  }

  const missingEmailResult = await runGraphql(dataSaleOptOutMissingEmailMutation);
  if (
    missingEmailResult.status < 200 ||
    missingEmailResult.status >= 300 ||
    !Array.isArray(missingEmailResult.payload?.errors)
  ) {
    throw new Error(
      `dataSaleOptOut missing email did not return top-level errors: ${JSON.stringify(missingEmailResult, null, 2)}`,
    );
  }
  const missingEmailCapture = {
    mutation: {
      query: dataSaleOptOutMissingEmailMutation,
      variables: {},
      response: missingEmailResult.payload,
    },
    upstreamCalls: [],
  };
  await writeFile(
    path.join(outputDir, 'data-sale-opt-out-missing-email.json'),
    `${JSON.stringify(missingEmailCapture, null, 2)}\n`,
    'utf8',
  );
  if (process.env.SHOPIFY_CONFORMANCE_CAPTURE_DATA_SALE_OPT_OUT_MISSING_EMAIL_ONLY === 'true') {
    console.log(
      JSON.stringify(
        {
          ok: true,
          outputDir,
          files: ['data-sale-opt-out-missing-email.json'],
        },
        null,
        2,
      ),
    );
    return;
  }
  await writeInvalidFormatCapture();
  await writeStrictFormatResidualCapture();

  const stamp = Date.now();
  const emailAddress = `hermes-data-sale-${stamp}@example.com`;
  const unknownEmailAddress = `hermes-data-sale-new-${stamp}@example.com`;
  const whitespaceEmailAddress = `hermes data sale whitespace ${stamp}@example.com`;
  const whitespaceSanitizedEmailAddress = whitespaceEmailAddress.replace(/[ \n\r]/g, '');
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
  let whitespaceCustomerId = null;
  let newDefaultsCustomerId = null;
  try {
    // Capture the live customerByIdentifier lookup the proxy forwards to resolve this existing
    // customer (pre-opt-out state) so the recorded cassette byte-matches the proxy's request.
    const existingLookupPayload = await captureCustomerLookup(emailAddress, 'dataSaleOptOut existing customer lookup');

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

    // The proxy forwards the lookup with the *sanitized* email; capture it before the whitespace
    // opt-out creates the customer upstream, so the recorded lookup returns null (no pre-existing
    // customer) and the proxy stages a new synthetic customer to match Shopify's create-new path.
    const whitespaceLookupPayload = await captureCustomerLookup(
      whitespaceSanitizedEmailAddress,
      'dataSaleOptOut whitespace email lookup',
    );

    const whitespaceEmailVariables = { email: whitespaceEmailAddress };
    const whitespaceEmailResult = await runGraphql(dataSaleOptOutMutation, whitespaceEmailVariables);
    assertNoTopLevelErrors(whitespaceEmailResult, 'dataSaleOptOut whitespace email');
    whitespaceCustomerId = whitespaceEmailResult.payload?.data?.dataSaleOptOut?.customerId;
    if (typeof whitespaceCustomerId !== 'string' || !whitespaceCustomerId) {
      throw new Error(
        `dataSaleOptOut whitespace email did not return a customer id: ${JSON.stringify(
          whitespaceEmailResult.payload,
          null,
          2,
        )}`,
      );
    }

    const whitespaceDownstreamReadVariables = {
      id: whitespaceCustomerId,
      identifier: { id: whitespaceCustomerId },
    };
    const whitespaceDownstreamReadResult = await runGraphql(
      whitespaceDownstreamReadQuery,
      whitespaceDownstreamReadVariables,
    );
    assertNoTopLevelErrors(whitespaceDownstreamReadResult, 'dataSaleOptOut whitespace email downstream read');

    const cleanupExisting = await runGraphql(deleteMutation, { input: { id: customerId } });
    assertNoTopLevelErrors(cleanupExisting, 'dataSaleOptOut existing customer cleanup');
    const cleanupUnknown = await runGraphql(deleteMutation, { input: { id: unknownCustomerId } });
    assertNoTopLevelErrors(cleanupUnknown, 'dataSaleOptOut unknown customer cleanup');
    const cleanupWhitespace = await runGraphql(deleteMutation, { input: { id: whitespaceCustomerId } });
    assertNoTopLevelErrors(cleanupWhitespace, 'dataSaleOptOut whitespace customer cleanup');

    // The new-customer-defaults fixture lives at 2026-04 and depends on a slow tag-search indexing
    // poll; skip it when re-recording the 2025-01 privacy captures to de-seed parity.
    let newCustomerDefaultsCapture = null;
    if (!skipNewCustomerDefaults) {
      newCustomerDefaultsCapture = await captureNewCustomerDefaults(stamp);
      newDefaultsCustomerId = newCustomerDefaultsCapture.mutation.response.data.dataSaleOptOut.customerId;
    }

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
      upstreamCalls: [customerLookupUpstreamCall(emailAddress, existingLookupPayload)],
    };

    const whitespaceCapture = {
      mutation: {
        variables: whitespaceEmailVariables,
        response: whitespaceEmailResult.payload,
      },
      downstreamRead: {
        variables: whitespaceDownstreamReadVariables,
        response: whitespaceDownstreamReadResult.payload,
      },
      cleanup: {
        whitespaceEmailCustomer: {
          response: cleanupWhitespace.payload,
        },
      },
      upstreamCalls: [customerLookupUpstreamCall(whitespaceSanitizedEmailAddress, whitespaceLookupPayload)],
    };

    await writeFile(
      path.join(outputDir, 'data-sale-opt-out-parity.json'),
      `${JSON.stringify(capture, null, 2)}\n`,
      'utf8',
    );

    await writeFile(
      path.join(outputDir, 'data-sale-opt-out-whitespace-email.json'),
      `${JSON.stringify(whitespaceCapture, null, 2)}\n`,
      'utf8',
    );

    if (newCustomerDefaultsCapture) {
      await writeFile(
        path.join(outputDir, 'data-sale-opt-out-new-customer-defaults.json'),
        `${JSON.stringify(newCustomerDefaultsCapture, null, 2)}\n`,
        'utf8',
      );
    }

    console.log(
      JSON.stringify(
        {
          ok: true,
          outputDir,
          files: [
            'data-sale-opt-out-invalid-format.json',
            'data-sale-opt-out-strict-format-residual.json',
            'data-sale-opt-out-parity.json',
            'data-sale-opt-out-whitespace-email.json',
            ...(newCustomerDefaultsCapture ? ['data-sale-opt-out-new-customer-defaults.json'] : []),
          ],
          customerId,
          unknownCustomerId,
          whitespaceCustomerId,
          newDefaultsCustomerId,
        },
        null,
        2,
      ),
    );
  } catch (error) {
    if (newDefaultsCustomerId) {
      await runGraphql(deleteMutation, { input: { id: newDefaultsCustomerId } });
    }
    if (whitespaceCustomerId) {
      await runGraphql(deleteMutation, { input: { id: whitespaceCustomerId } });
    }
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

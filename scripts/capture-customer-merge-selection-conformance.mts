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
const outputPath = path.join(outputDir, 'customer-merge-selection-rules.json');

const mergeDocument = await readFile('config/parity-requests/customers/customer-merge-selection-merge.graphql', 'utf8');
const readWithEmailDocument = await readFile(
  'config/parity-requests/customers/customer-merge-selection-read-with-email.graphql',
  'utf8',
);
const readDocument = await readFile('config/parity-requests/customers/customer-merge-selection-read.graphql', 'utf8');
// The merge resolves each referenced customer the real way instead of from seeded records:
// customerMerge forwards CUSTOMER_MERGE_HYDRATE_QUERY per id (ensure_customer_hydrated_for_merge) and
// stages the observed record, so selection (email presence / account-state tiebreak) and the
// downstream readback both read real upstream state. Record that exact constant (include_str! of
// the file below) so the cassette byte-matches the proxy's forward. No seeding required.
const customerMergeHydrateDocument = await readFile(
  'config/parity-requests/customers/customer-merge-hydrate.graphql',
  'utf8',
);

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

// Forward CUSTOMER_MERGE_HYDRATE_QUERY upstream and capture the live response at the customer's
// current (post-invite, pre-merge) state, so the proxy reproduces the same selection + readback.
async function captureMutationHydrate(id, context) {
  const result = await runGraphql(customerMergeHydrateDocument, { id });
  assertNoTopLevelErrors(result, context);
  return result.payload;
}

function hydrateUpstreamCall(id, payload) {
  return {
    operationName: 'CustomerMergeHydrate',
    variables: { id },
    query: customerMergeHydrateDocument,
    response: { status: 200, body: payload },
  };
}

const customerSlice = `
  id
  firstName
  lastName
  displayName
  email
  state
  note
  tags
  numberOfOrders
  defaultEmailAddress { emailAddress marketingState }
  emailMarketingConsent { marketingState marketingOptInLevel consentUpdatedAt }
  defaultPhoneNumber { phoneNumber }
  defaultAddress { id address1 city provinceCode countryCodeV2 zip }
  addressesV2(first: 10) {
    nodes { id address1 city provinceCode countryCodeV2 zip }
    pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
  }
  metafields(first: 10) {
    nodes { id namespace key type value }
    pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
  }
  orders(first: 10, sortKey: CREATED_AT, reverse: true) {
    nodes { id name email createdAt }
    pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
  }
  lastOrder { id name email createdAt }
  createdAt
  updatedAt
`;

const createCustomerMutation = `#graphql
  mutation CustomerMergeSelectionCreate($input: CustomerInput!) {
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

const inviteMutation = `#graphql
  mutation CustomerMergeSelectionInvite($customerId: ID!) {
    customerSendAccountInviteEmail(customerId: $customerId) {
      customer {
        ${customerSlice}
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const jobStatusQuery = `#graphql
  query CustomerMergeSelectionJobStatus($jobId: ID!) {
    customerMergeJobStatus(jobId: $jobId) {
      resultingCustomerId
      status
      customerMergeErrors {
        errorFields
        message
      }
    }
  }
`;

const deleteCustomerMutation = `#graphql
  mutation CustomerMergeSelectionCleanup($input: CustomerDeleteInput!) {
    customerDelete(input: $input) {
      deletedCustomerId
      userErrors {
        field
        message
      }
    }
  }
`;

async function createCustomer(label, input) {
  const variables = { input };
  const response = await runGraphql(createCustomerMutation, variables);
  assertNoTopLevelErrors(response, `${label} customerCreate`);
  const payload = response.payload?.data?.customerCreate;
  if (payload?.userErrors?.length) {
    throw new Error(`${label} customerCreate userErrors: ${JSON.stringify(payload.userErrors, null, 2)}`);
  }
  const customer = payload?.customer;
  if (!customer?.id) {
    throw new Error(`${label} customerCreate did not return a customer: ${JSON.stringify(response.payload, null, 2)}`);
  }
  return { variables, response: response.payload, customer };
}

async function inviteCustomer(label, customerId) {
  const variables = { customerId };
  const response = await runGraphql(inviteMutation, variables);
  assertNoTopLevelErrors(response, `${label} customerSendAccountInviteEmail`);
  const payload = response.payload?.data?.customerSendAccountInviteEmail;
  if (payload?.userErrors?.length) {
    throw new Error(
      `${label} customerSendAccountInviteEmail userErrors: ${JSON.stringify(payload.userErrors, null, 2)}`,
    );
  }
  const customer = payload?.customer;
  if (customer?.state !== 'INVITED') {
    throw new Error(`${label} invite did not produce INVITED state: ${JSON.stringify(response.payload, null, 2)}`);
  }
  return { variables, response: response.payload, customer };
}

async function pollMergeStatus(jobId) {
  const polls = [];
  for (let attempt = 0; attempt < 10; attempt += 1) {
    const response = await runGraphql(jobStatusQuery, { jobId });
    assertNoTopLevelErrors(response, 'customerMergeJobStatus');
    polls.push({ attempt, variables: { jobId }, response: response.payload });
    if (response.payload?.data?.customerMergeJobStatus?.status !== 'IN_PROGRESS') break;
    await new Promise((resolve) => setTimeout(resolve, 1000));
  }
  return polls;
}

async function deleteCustomer(id) {
  const variables = { input: { id } };
  const response = await runGraphql(deleteCustomerMutation, variables);
  return { variables, response: response.payload };
}

async function captureCase(key, setup, upstreamCalls, cleanup) {
  const one = await createCustomer(`${key} one`, setup.oneInput);
  let oneCustomer = one.customer;
  let invite = null;
  if (setup.inviteOne === true) {
    invite = await inviteCustomer(`${key} one`, one.customer.id);
    oneCustomer = invite.customer;
  }
  const two = await createCustomer(`${key} two`, setup.twoInput);
  const twoCustomer = two.customer;

  // Capture the upstream hydrates the merge forwards for this pair at the pre-merge state
  // (customerOne already in its INVITED state when this case invites it).
  const hydrateOne = await captureMutationHydrate(oneCustomer.id, `${key} hydrate one`);
  const hydrateTwo = await captureMutationHydrate(twoCustomer.id, `${key} hydrate two`);
  upstreamCalls.push(hydrateUpstreamCall(oneCustomer.id, hydrateOne), hydrateUpstreamCall(twoCustomer.id, hydrateTwo));

  const mergeVariables = {
    one: oneCustomer.id,
    two: twoCustomer.id,
    override: setup.overrideFor ? setup.overrideFor(oneCustomer, twoCustomer) : null,
  };
  const merge = await runGraphql(mergeDocument, mergeVariables);
  assertNoTopLevelErrors(merge, `${key} customerMerge`);
  const mergePayload = merge.payload?.data?.customerMerge;
  if (mergePayload?.userErrors?.length) {
    throw new Error(`${key} customerMerge userErrors: ${JSON.stringify(mergePayload.userErrors, null, 2)}`);
  }
  const resultingCustomerId = mergePayload?.resultingCustomerId;
  const jobId = mergePayload?.job?.id;
  if (typeof resultingCustomerId !== 'string' || typeof jobId !== 'string') {
    throw new Error(`${key} customerMerge missing result/job: ${JSON.stringify(merge.payload, null, 2)}`);
  }

  const resultCustomer = resultingCustomerId === oneCustomer.id ? oneCustomer : twoCustomer;
  const sourceCustomer = resultingCustomerId === oneCustomer.id ? twoCustomer : oneCustomer;
  const resultEmail = resultCustomer.email ?? resultCustomer.defaultEmailAddress?.emailAddress ?? null;
  const statusPolls = await pollMergeStatus(jobId);

  const readVariables = resultEmail
    ? { result: resultCustomer.id, source: sourceCustomer.id, email: resultEmail, jobId }
    : { result: resultCustomer.id, source: sourceCustomer.id, jobId };
  const read = await runGraphql(resultEmail ? readWithEmailDocument : readDocument, readVariables);
  assertNoTopLevelErrors(read, `${key} downstream read`);

  cleanup.push({ key, result: await deleteCustomer(resultingCustomerId) });

  return {
    setup: {
      createOne: one,
      inviteOne: invite,
      createTwo: two,
    },
    resultCustomerId: resultCustomer.id,
    sourceCustomerId: sourceCustomer.id,
    resultEmail,
    merge: {
      variables: mergeVariables,
      response: merge.payload,
    },
    statusPolls,
    read: {
      variables: readVariables,
      response: read.payload,
    },
  };
}

async function main() {
  await mkdir(outputDir, { recursive: true });
  const stamp = Date.now();
  const upstreamCalls = [];
  const cleanup = [];

  const cases = {
    overrideCustomerOne: await captureCase(
      'overrideCustomerOne',
      {
        oneInput: {
          email: `hermes-merge-select-override-one-${stamp}@example.com`,
          firstName: 'Selection',
          lastName: 'OverrideOne',
        },
        twoInput: {
          email: `hermes-merge-select-override-two-${stamp}@example.com`,
          firstName: 'Selection',
          lastName: 'OverrideTwo',
        },
        overrideFor: (one) => ({ customerIdOfEmailToKeep: one.id }),
      },
      upstreamCalls,
      cleanup,
    ),
    onlyCustomerOneHasEmail: await captureCase(
      'onlyCustomerOneHasEmail',
      {
        oneInput: {
          email: `hermes-merge-select-single-one-${stamp}@example.com`,
          firstName: 'Selection',
          lastName: 'SingleEmailOne',
        },
        twoInput: {
          firstName: 'Selection',
          lastName: 'SingleEmailTwo',
        },
      },
      upstreamCalls,
      cleanup,
    ),
    accountStateCustomerOne: await captureCase(
      'accountStateCustomerOne',
      {
        oneInput: {
          email: `hermes-merge-select-invited-one-${stamp}@example.com`,
          firstName: 'Selection',
          lastName: 'InvitedOne',
        },
        inviteOne: true,
        twoInput: {
          email: `hermes-merge-select-disabled-two-${stamp}@example.com`,
          firstName: 'Selection',
          lastName: 'DisabledTwo',
        },
      },
      upstreamCalls,
      cleanup,
    ),
    neitherHasEmail: await captureCase(
      'neitherHasEmail',
      {
        oneInput: {
          firstName: 'Selection',
          lastName: 'NoEmailOne',
        },
        twoInput: {
          firstName: 'Selection',
          lastName: 'NoEmailTwo',
        },
      },
      upstreamCalls,
      cleanup,
    ),
  };

  const capture = {
    cases,
    upstreamCalls,
    cleanup,
  };
  await writeFile(outputPath, `${JSON.stringify(capture, null, 2)}\n`);
  console.log(`Wrote ${outputPath}`);
}

await main();

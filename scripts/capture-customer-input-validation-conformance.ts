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
  createdAt
  updatedAt
`;

const createMutation = `#graphql
  mutation CustomerInputValidationCreate($input: CustomerInput!) {
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
  mutation CustomerInputValidationUpdate($input: CustomerInput!) {
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

const mergeMutation = `#graphql
  mutation CustomerInputValidationMerge($customerOneId: ID!, $customerTwoId: ID!) {
    customerMerge(customerOneId: $customerOneId, customerTwoId: $customerTwoId) {
      resultingCustomerId
      job {
        id
        done
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const deleteMutation = `#graphql
  mutation CustomerInputValidationDelete($input: CustomerDeleteInput!) {
    customerDelete(input: $input) {
      deletedCustomerId
      userErrors {
        field
        message
      }
    }
  }
`;

const downstreamReadQuery = `#graphql
  query CustomerInputValidationDownstream($id: ID!, $identifier: CustomerIdentifierInput!, $query: String!, $first: Int!) {
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
        startCursor
        endCursor
      }
    }
    customersCount {
      count
      precision
    }
  }
`;

const customerHydrateQuery = `
query CustomerHydrate($id: ID!) {
  customer(id: $id) {
    id
    firstName
    lastName
    displayName
    email
    phone
    locale
    note
    canDelete
    verifiedEmail
    dataSaleOptOut
    taxExempt
    taxExemptions
    state
    tags
    createdAt
    updatedAt
    defaultEmailAddress { emailAddress }
    defaultPhoneNumber { phoneNumber }
    defaultAddress { id firstName lastName address1 address2 city company province provinceCode country countryCodeV2 zip phone name formattedArea }
    addressesV2(first: 250) { nodes { id firstName lastName address1 address2 city company province provinceCode country countryCodeV2 zip phone name formattedArea } }
  }
}
`;

const customerDuplicateHydrateQuery = `
query CustomerDuplicateHydrate($query: String!) {
  customers(first: 1, query: $query) {
    nodes { id }
  }
}
`;

// customerMerge forwards the richer CustomerMergeHydrate per referenced customer (it needs
// the orders/metafields the lighter CustomerHydrate omits). Read the exact shared constant
// the proxy include_str!s so the synthesized cassette byte-matches the proxy's forward.
const customerMergeHydrateQuery = await readFile(
  'config/parity-requests/customers/customer-merge-hydrate.graphql',
  'utf8',
);

const createdCustomerIds = new Set<string>();
const deletedCustomerIds = new Set<string>();

function assertNoGraphqlFailure(result, context) {
  if (result.status < 200 || result.status >= 300 || result.payload?.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function recordCreatedCustomerId(payload) {
  const id =
    payload?.data?.customerCreate?.customer?.id ??
    payload?.data?.customerUpdate?.customer?.id ??
    payload?.data?.customerMerge?.resultingCustomerId;
  if (typeof id === 'string' && id) {
    createdCustomerIds.add(id);
  }
  return typeof id === 'string' && id ? id : null;
}

function emailFor(stamp, label) {
  return `hermes-input-validation-${label}-${stamp}@example.com`;
}

function phoneFor(stamp, offset) {
  const tail = String((offset % 9000) + 1000).padStart(4, '0');
  return `+1415555${tail}`;
}

async function runCase(document, variables) {
  const result = await runGraphql(document, variables);
  recordCreatedCustomerId(result.payload);
  return {
    variables,
    response: result.payload,
    status: result.status,
  };
}

async function createRequiredCustomer(stamp, label, input = {}) {
  const variables = {
    input: {
      email: emailFor(stamp, label),
      firstName: 'Hermes',
      lastName: label,
      phone: phoneFor(stamp, createdCustomerIds.size + 1),
      tags: ['input-validation', label],
      ...input,
    },
  };
  const result = await runGraphql(createMutation, variables);
  assertNoGraphqlFailure(result, `create precondition customer ${label}`);
  const customerId = result.payload?.data?.customerCreate?.customer?.id;
  if (typeof customerId !== 'string' || !customerId) {
    throw new Error(`Precondition customer ${label} did not return an id: ${JSON.stringify(result.payload, null, 2)}`);
  }
  createdCustomerIds.add(customerId);
  return {
    id: customerId,
    email: variables.input.email,
    phone: variables.input.phone,
    variables,
    response: result.payload,
  };
}

async function readDownstream(customerId, email) {
  const queryEmail = typeof email === 'string' && email.trim() ? email : '__customer_input_validation_no_match__';
  const variables = {
    id: customerId,
    identifier: { id: customerId },
    query: `email:${queryEmail}`,
    first: 5,
  };
  const result = await runGraphql(downstreamReadQuery, variables);
  return {
    variables,
    response: result.payload,
    status: result.status,
  };
}

function customerFromCase(rootName, result) {
  return result.response?.data?.[rootName]?.customer ?? null;
}

function capturedCustomerFromCreate(record) {
  return record?.response?.data?.customerCreate?.customer ?? null;
}

function customerHydrateCall(customer) {
  return {
    operationName: 'CustomerHydrate',
    variables: { id: customer.id },
    query: customerHydrateQuery,
    response: {
      status: 200,
      body: { data: { customer } },
    },
  };
}

// Shape a CustomerMergeHydrate response from the live-created customer record. The merge
// customers carry no orders/metafields, so those connections are empty; the remaining fields
// the merge query selects (phone, dataSaleOptOut, taxExemptions, numberOfOrders, defaultAddress,
// addressesV2, lastOrder) default to their no-data forms. Only the query text + variables are
// matched, so this richer body simply lets the merge stage a well-formed customer.
function customerMergeHydrateCall(customer) {
  return {
    operationName: 'CustomerMergeHydrate',
    variables: { id: customer.id },
    query: customerMergeHydrateQuery,
    response: {
      status: 200,
      body: {
        data: {
          customer: {
            ...customer,
            phone: customer.defaultPhoneNumber?.phoneNumber ?? null,
            dataSaleOptOut: false,
            taxExemptions: [],
            numberOfOrders: '0',
            defaultAddress: null,
            addressesV2: { nodes: [] },
            metafields: { nodes: [] },
            orders: { edges: [] },
            lastOrder: null,
          },
        },
      },
    },
  };
}

function emptyDuplicateHydrateCall(field, value) {
  return {
    operationName: 'CustomerDuplicateHydrate',
    variables: { query: `${field}:${value}` },
    query: customerDuplicateHydrateQuery,
    response: {
      status: 200,
      body: { data: { customers: { nodes: [] } } },
    },
  };
}

function inputEmail(record) {
  return record?.variables?.input?.email ?? null;
}

function inputPhone(record) {
  return record?.variables?.input?.phone ?? null;
}

function buildUpstreamCalls(capture) {
  const calls = [];
  const primaryCustomer = capturedCustomerFromCreate(capture.preconditions.primary);
  const duplicateTargetCustomer = capturedCustomerFromCreate(capture.preconditions.duplicateTarget);
  // The two precondition customers (primary, duplicate-target) are staged by their
  // own setup-create targets, which the proxy validates by forwarding a uniqueness
  // lookup upstream. At precondition time no such customer exists upstream, so these
  // lookups must come back EMPTY — otherwise the setup creates fail "already taken"
  // and never stage, and the later duplicate scenarios (which detect the dup against
  // the *staged* precondition customers, short-circuiting before any forward) never
  // see them. Emitting a MATCH here is the seeding-era bug this de-seed removes.
  if (primaryCustomer) {
    calls.push(emptyDuplicateHydrateCall('email', capture.preconditions.primary.email));
    calls.push(emptyDuplicateHydrateCall('phone', capture.preconditions.primary.phone));
  }
  if (duplicateTargetCustomer) {
    calls.push(emptyDuplicateHydrateCall('email', capture.preconditions.duplicateTarget.email));
    calls.push(emptyDuplicateHydrateCall('phone', capture.preconditions.duplicateTarget.phone));
  }
  for (const scenario of Object.values(capture.createScenarios)) {
    const email = inputEmail(scenario);
    if (typeof email === 'string' && email.includes('@') && email !== capture.preconditions.primary.email) {
      calls.push(emptyDuplicateHydrateCall('email', email));
    }
    const phone = inputPhone(scenario);
    if (typeof phone === 'string' && phone.startsWith('+') && phone !== capture.preconditions.primary.phone) {
      calls.push(emptyDuplicateHydrateCall('phone', phone));
    }
  }
  for (const scenario of Object.values(capture.updateScenarios)) {
    // Each update targets a pre-existing customer that no setup-create staged, so
    // the proxy forwards a CustomerHydrate to resolve it. The duplicate-email/phone
    // update scenarios detect the collision against the *staged* duplicate-target
    // customer (short-circuiting before any uniqueness forward), so we emit no
    // dedupe cassette here — a MATCH would also collide with the EMPTY dup-target
    // uniqueness lookups the setup-create targets consume.
    const baseCustomer = capturedCustomerFromCreate({
      response: { data: { customerCreate: { customer: scenario.baseCustomer } } },
    });
    if (baseCustomer) {
      calls.push(customerHydrateCall(baseCustomer));
    }
  }
  // The delete target forwards a CustomerHydrate to confirm the customer exists,
  // then stages the deletion. The follow-up update of that now-deleted customer is
  // answered from staged `deleted_customer_ids` (no forward), so no missing-hydrate
  // cassette is needed — the delete's own hydrate is the only forward.
  const deletedCustomer = capturedCustomerFromCreate(capture.deletedCustomerUpdate.precondition);
  if (deletedCustomer) {
    calls.push(customerHydrateCall(deletedCustomer));
  }
  // customerMerge resolves both pre-existing customers the real way: the proxy
  // forwards a CustomerMergeHydrate for each and stages the observed record, so both
  // must hydrate to a real customer. After the merge the source is in
  // `deleted_customer_ids`, so the follow-up update of the merged-away source is
  // answered locally (no forward) as "Customer does not exist".
  const mergeSource = capturedCustomerFromCreate(capture.mergedCustomerUpdate.mergeSource);
  const mergeTarget = capturedCustomerFromCreate(capture.mergedCustomerUpdate.mergeTarget);
  if (mergeSource) {
    calls.push(customerMergeHydrateCall(mergeSource));
  }
  if (mergeTarget) {
    calls.push(customerMergeHydrateCall(mergeTarget));
  }
  return calls;
}

async function runCreateScenario(input, options = {}) {
  const result = await runCase(createMutation, { input });
  const customer = customerFromCase('customerCreate', result);
  const downstreamRead = customer?.id
    ? await readDownstream(customer.id, customer.email ?? options.downstreamEmail)
    : null;
  return {
    ...result,
    downstreamRead,
  };
}

async function runUpdateScenario(baseCustomer, input) {
  const result = await runCase(updateMutation, { input: { id: baseCustomer.id, ...input } });
  const customer = customerFromCase('customerUpdate', result);
  const downstreamRead = customer?.id ? await readDownstream(customer.id, customer.email ?? baseCustomer.email) : null;
  return {
    baseCustomer: {
      id: baseCustomer.id,
      email: baseCustomer.email,
      phone: baseCustomer.phone,
    },
    ...result,
    downstreamRead,
  };
}

async function cleanupCustomers() {
  const cleanup = [];
  for (const customerId of [...createdCustomerIds].reverse()) {
    if (deletedCustomerIds.has(customerId)) {
      continue;
    }
    const result = await runGraphql(deleteMutation, { input: { id: customerId } });
    if (!result.payload?.errors && result.payload?.data?.customerDelete?.deletedCustomerId) {
      deletedCustomerIds.add(customerId);
    }
    cleanup.push({
      variables: { input: { id: customerId } },
      status: result.status,
      response: result.payload,
    });
  }
  return cleanup;
}

async function main() {
  await mkdir(outputDir, { recursive: true });

  const stamp = Date.now();
  const primary = await createRequiredCustomer(stamp, 'primary');
  const duplicateTarget = await createRequiredCustomer(stamp, 'duplicate-target');

  const createScenarios = {
    invalidEmail: await runCreateScenario({
      email: 'not-an-email',
    }),
    invalidPhone: await runCreateScenario({
      phone: 'abc',
    }),
    duplicateEmail: await runCreateScenario({
      email: primary.email,
      firstName: 'Duplicate',
    }),
    duplicatePhone: await runCreateScenario({
      phone: primary.phone,
      firstName: 'Duplicate',
    }),
    invalidLocale: await runCreateScenario({
      email: emailFor(stamp, 'create-invalid-locale'),
      locale: 'not-a-locale',
    }),
    blankScalarNormalization: await runCreateScenario({
      email: emailFor(stamp, 'create-blank-scalars'),
      firstName: '   ',
      lastName: '',
      note: '',
      phone: '',
    }),
    nullScalarNormalization: await runCreateScenario({
      email: emailFor(stamp, 'create-null-scalars'),
      firstName: null,
      lastName: null,
      note: null,
      phone: null,
    }),
    tagNormalization: await runCreateScenario({
      email: emailFor(stamp, 'create-tags'),
      tags: ['Zulu', 'alpha', 'alpha', ' spaced tag ', ''],
    }),
    oversizedTag: await runCreateScenario({
      email: emailFor(stamp, 'create-oversized-tag'),
      tags: ['T'.repeat(256)],
    }),
    oversizedNameAndNote: await runCreateScenario({
      email: emailFor(stamp, 'create-oversized-name-note'),
      firstName: 'F'.repeat(300),
      lastName: 'L'.repeat(300),
      note: 'N'.repeat(70000),
    }),
  };

  const updateScenarios = {
    invalidEmail: await runUpdateScenario(await createRequiredCustomer(stamp, 'update-invalid-email'), {
      email: 'not-an-email',
    }),
    invalidPhone: await runUpdateScenario(await createRequiredCustomer(stamp, 'update-invalid-phone'), {
      phone: 'abc',
    }),
    duplicateEmail: await runUpdateScenario(await createRequiredCustomer(stamp, 'update-duplicate-email'), {
      email: duplicateTarget.email,
    }),
    duplicatePhone: await runUpdateScenario(await createRequiredCustomer(stamp, 'update-duplicate-phone'), {
      phone: duplicateTarget.phone,
    }),
    invalidLocale: await runUpdateScenario(await createRequiredCustomer(stamp, 'update-invalid-locale'), {
      locale: 'not-a-locale',
    }),
    blankScalarNormalization: await runUpdateScenario(await createRequiredCustomer(stamp, 'update-blank-scalars'), {
      firstName: '   ',
      lastName: '',
      note: '',
      phone: '',
    }),
    nullScalarNormalization: await runUpdateScenario(await createRequiredCustomer(stamp, 'update-null-scalars'), {
      firstName: null,
      lastName: null,
      note: null,
      phone: null,
    }),
    tagNormalization: await runUpdateScenario(await createRequiredCustomer(stamp, 'update-tags'), {
      tags: ['Zulu', 'alpha', 'alpha', ' spaced tag ', ''],
    }),
    oversizedTag: await runUpdateScenario(await createRequiredCustomer(stamp, 'update-oversized-tag'), {
      tags: ['T'.repeat(256)],
    }),
    oversizedNameAndNote: await runUpdateScenario(await createRequiredCustomer(stamp, 'update-oversized-name-note'), {
      firstName: 'F'.repeat(300),
      lastName: 'L'.repeat(300),
      note: 'N'.repeat(70000),
    }),
  };

  const deletedUpdateCustomer = await createRequiredCustomer(stamp, 'update-deleted');
  const deleteBeforeUpdate = await runCase(deleteMutation, { input: { id: deletedUpdateCustomer.id } });
  if (deleteBeforeUpdate.response?.data?.customerDelete?.deletedCustomerId) {
    deletedCustomerIds.add(deletedUpdateCustomer.id);
  }
  const updateDeletedCustomer = await runCase(updateMutation, {
    input: {
      id: deletedUpdateCustomer.id,
      firstName: 'AfterDelete',
    },
  });

  const mergeSource = await createRequiredCustomer(stamp, 'merge-source');
  const mergeTarget = await createRequiredCustomer(stamp, 'merge-target');
  const mergeResult = await runCase(mergeMutation, {
    customerOneId: mergeSource.id,
    customerTwoId: mergeTarget.id,
  });
  if (mergeResult.response?.data?.customerMerge?.resultingCustomerId === mergeTarget.id) {
    deletedCustomerIds.add(mergeSource.id);
  }
  const updateMergedSourceCustomer = await runCase(updateMutation, {
    input: {
      id: mergeSource.id,
      firstName: 'AfterMerge',
    },
  });

  const cleanup = await cleanupCustomers();

  const capture = {
    metadata: {
      storeDomain,
      apiVersion,
      capturedAt: new Date().toISOString(),
      stamp,
    },
    preconditions: {
      primary,
      duplicateTarget,
    },
    createScenarios,
    updateScenarios,
    deletedCustomerUpdate: {
      precondition: deletedUpdateCustomer,
      deleteBeforeUpdate,
      updateDeletedCustomer,
    },
    mergedCustomerUpdate: {
      mergeSource,
      mergeTarget,
      mergeResult,
      updateMergedSourceCustomer,
    },
    cleanup,
  };
  capture.upstreamCalls = buildUpstreamCalls(capture);

  const outputFile = path.join(outputDir, 'customer-input-validation-parity.json');
  await writeFile(outputFile, `${JSON.stringify(capture, null, 2)}\n`, 'utf8');

  console.log(
    JSON.stringify(
      {
        ok: true,
        outputFile,
        createdCustomers: createdCustomerIds.size,
        cleanupAttempts: cleanup.length,
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

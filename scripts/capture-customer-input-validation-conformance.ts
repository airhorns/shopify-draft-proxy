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
  const tail = String(stamp + offset)
    .slice(-7)
    .padStart(7, '0');
  return `+1415${tail}`;
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

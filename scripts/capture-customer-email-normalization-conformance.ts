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
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'customers');
const { runGraphqlRequest: runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const customerEmailSlice = `
  id
  email
  defaultEmailAddress {
    emailAddress
  }
`;

const createMutation = `#graphql
  mutation CustomerEmailNormalizationCreate($input: CustomerInput!) {
    customerCreate(input: $input) {
      customer {
        ${customerEmailSlice}
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const updateMutation = `#graphql
  mutation CustomerEmailNormalizationUpdate($input: CustomerInput!) {
    customerUpdate(input: $input) {
      customer {
        ${customerEmailSlice}
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const customerSetMutation = `#graphql
  mutation CustomerEmailNormalizationSet($input: CustomerSetInput!, $identifier: CustomerSetIdentifiers) {
    customerSet(input: $input, identifier: $identifier) {
      customer {
        ${customerEmailSlice}
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const downstreamReadQuery = `#graphql
  query CustomerEmailNormalizationRead($identifier: CustomerIdentifierInput!, $query: String!, $first: Int!) {
    customerByIdentifier(identifier: $identifier) {
      email
      defaultEmailAddress {
        emailAddress
      }
    }
    customers(first: $first, query: $query, sortKey: UPDATED_AT, reverse: true) {
      nodes {
        email
        defaultEmailAddress {
          emailAddress
        }
      }
    }
  }
`;

const deleteMutation = `#graphql
  mutation CustomerEmailNormalizationDelete($input: CustomerDeleteInput!) {
    customerDelete(input: $input) {
      deletedCustomerId
      userErrors {
        field
        message
      }
    }
  }
`;

const createdCustomerIds = new Set<string>();
const deletedCustomerIds = new Set<string>();

function assertNoTopLevelErrors(result, context) {
  if (result.status < 200 || result.status >= 300 || result.payload?.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function recordCreatedCustomerId(payload) {
  const id =
    payload?.data?.customerCreate?.customer?.id ??
    payload?.data?.customerUpdate?.customer?.id ??
    payload?.data?.customerSet?.customer?.id;
  if (typeof id === 'string' && id) {
    createdCustomerIds.add(id);
  }
  return typeof id === 'string' && id ? id : null;
}

async function runCase(document, variables, context) {
  const result = await runGraphql(document, variables);
  assertNoTopLevelErrors(result, context);
  recordCreatedCustomerId(result.payload);
  return {
    variables,
    response: result.payload,
    status: result.status,
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

function stampEmail(stamp, label) {
  return `hermes-email-normalization-${label}-${stamp}@example.com`;
}

function tooLongEmail() {
  return `${'a'.repeat(244)}@example.com`;
}

async function main() {
  await mkdir(outputDir, { recursive: true });

  const stamp = Date.now();
  const normalizedCreateEmail = stampEmail(stamp, 'create');
  const spacedCreateEmail = normalizedCreateEmail.replace('normalization', 'normal ization');
  const duplicateCreateEmail = normalizedCreateEmail
    .replace('hermes', 'Hermes')
    .replace('normalization', 'normal iz ation') + ' ';
  const updateEmail = stampEmail(stamp, 'updated');
  const spacedUpdateEmail = updateEmail.replace('updated', 'up dated') + ' ';
  const setEmail = stampEmail(stamp, 'set');
  const spacedSetEmail = setEmail.replace('set', 's et');
  const duplicateSetEmail = setEmail.replace('hermes', 'Hermes').replace('set', 's e t') + ' ';

  const createSanitized = await runCase(
    createMutation,
    { input: { email: spacedCreateEmail } },
    'customerCreate whitespace normalization',
  );
  const createCustomerId = createSanitized.response?.data?.customerCreate?.customer?.id;
  if (typeof createCustomerId !== 'string' || !createCustomerId) {
    throw new Error(`customerCreate normalization did not return an id: ${JSON.stringify(createSanitized.response, null, 2)}`);
  }

  const downstreamRead = await runCase(
    downstreamReadQuery,
    {
      identifier: { emailAddress: duplicateCreateEmail },
      query: `email:${normalizedCreateEmail}`,
      first: 5,
    },
    'customer email normalization downstream read',
  );

  const duplicateCreate = await runCase(
    createMutation,
    { input: { email: duplicateCreateEmail } },
    'customerCreate duplicate normalized email',
  );

  const invalidFooAt = await runCase(createMutation, { input: { email: 'foo@' } }, 'customerCreate invalid foo@');
  const invalidAtDomain = await runCase(
    createMutation,
    { input: { email: '@bar.com' } },
    'customerCreate invalid @bar.com',
  );
  const invalidNoDotDomain = await runCase(
    createMutation,
    { input: { email: 'foo@bar' } },
    'customerCreate invalid foo@bar',
  );
  const invalidDoubleAt = await runCase(
    createMutation,
    { input: { email: 'foo@@bar.com' } },
    'customerCreate invalid double at',
  );
  const tooLongCreate = await runCase(
    createMutation,
    { input: { email: tooLongEmail() } },
    'customerCreate too-long email',
  );

  const updateSanitized = await runCase(
    updateMutation,
    { input: { id: createCustomerId, email: spacedUpdateEmail } },
    'customerUpdate whitespace normalization',
  );

  const customerSetSanitized = await runCase(
    customerSetMutation,
    { input: { email: spacedSetEmail } },
    'customerSet whitespace normalization',
  );
  const customerSetId = customerSetSanitized.response?.data?.customerSet?.customer?.id;
  if (typeof customerSetId !== 'string' || !customerSetId) {
    throw new Error(`customerSet normalization did not return an id: ${JSON.stringify(customerSetSanitized.response, null, 2)}`);
  }

  const customerSetDuplicate = await runCase(
    customerSetMutation,
    { input: { email: duplicateSetEmail } },
    'customerSet duplicate normalized email',
  );
  const customerSetInvalid = await runCase(
    customerSetMutation,
    { input: { email: 'foo@' } },
    'customerSet invalid email',
  );
  const customerSetTooLong = await runCase(
    customerSetMutation,
    { input: { email: tooLongEmail() } },
    'customerSet too-long email',
  );

  const cleanup = await cleanupCustomers();

  const capture = {
    storeDomain,
    apiVersion,
    metadata: {
      storeDomain,
      apiVersion,
      capturedAt: new Date().toISOString(),
      stamp,
    },
    cases: {
      createSanitized,
      downstreamRead,
      duplicateCreate,
      invalidFooAt,
      invalidAtDomain,
      invalidNoDotDomain,
      invalidDoubleAt,
      tooLongCreate,
      updateSanitized,
      customerSetSanitized,
      customerSetDuplicate,
      customerSetInvalid,
      customerSetTooLong,
    },
    cleanup,
    upstreamCalls: [],
  };

  const outputFile = path.join(outputDir, 'customer-email-normalization.json');
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

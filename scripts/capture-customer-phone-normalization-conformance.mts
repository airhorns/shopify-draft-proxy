/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type CaptureCase = {
  variables: Record<string, unknown>;
  status: number;
  response: Record<string, any>;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'customers');
const { runGraphqlRequest: runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const customerPhoneSlice = `
  id
  phone
  defaultPhoneNumber {
    phoneNumber
  }
  createdAt
  updatedAt
`;

const customerCreateMutation = `#graphql
  mutation CustomerPhoneNormalizationCreate($input: CustomerInput!) {
    customerCreate(input: $input) {
      customer {
        ${customerPhoneSlice}
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const customerUpdateMutation = `#graphql
  mutation CustomerPhoneNormalizationUpdate($input: CustomerInput!) {
    customerUpdate(input: $input) {
      customer {
        ${customerPhoneSlice}
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const customerSetMutation = `#graphql
  mutation CustomerPhoneNormalizationSet($input: CustomerSetInput!, $identifier: CustomerSetIdentifiers) {
    customerSet(input: $input, identifier: $identifier) {
      customer {
        ${customerPhoneSlice}
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const customerDeleteMutation = `#graphql
  mutation CustomerPhoneNormalizationCleanup($input: CustomerDeleteInput!) {
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

function assertNoTopLevelErrors(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload?.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function captureCase(variables: Record<string, unknown>, result: ConformanceGraphqlResult): CaptureCase {
  return {
    variables,
    status: result.status,
    response: result.payload as Record<string, any>,
  };
}

function readCustomerId(result: ConformanceGraphqlResult, rootName: string): string | null {
  const payload = result.payload as Record<string, any>;
  const id = payload?.data?.[rootName]?.customer?.id;
  return typeof id === 'string' && id ? id : null;
}

async function runCreate(variables: Record<string, unknown>, context: string): Promise<CaptureCase> {
  const result = await runGraphql(customerCreateMutation, variables);
  assertNoTopLevelErrors(result, context);
  const id = readCustomerId(result, 'customerCreate');
  if (id) {
    createdCustomerIds.add(id);
  }
  return captureCase(variables, result);
}

async function runUpdate(variables: Record<string, unknown>, context: string): Promise<CaptureCase> {
  const result = await runGraphql(customerUpdateMutation, variables);
  assertNoTopLevelErrors(result, context);
  const id = readCustomerId(result, 'customerUpdate');
  if (id) {
    createdCustomerIds.add(id);
  }
  return captureCase(variables, result);
}

async function runCustomerSet(variables: Record<string, unknown>, context: string): Promise<CaptureCase> {
  const result = await runGraphql(customerSetMutation, variables);
  assertNoTopLevelErrors(result, context);
  const id = readCustomerId(result, 'customerSet');
  if (id) {
    createdCustomerIds.add(id);
  }
  return captureCase(variables, result);
}

async function cleanupCustomers(): Promise<Array<CaptureCase>> {
  const cleanup: Array<CaptureCase> = [];
  for (const id of [...createdCustomerIds].reverse()) {
    if (deletedCustomerIds.has(id)) {
      continue;
    }
    const variables = { input: { id } };
    const result = await runGraphql(customerDeleteMutation, variables);
    const payload = result.payload as Record<string, any>;
    if (!payload?.errors && payload?.data?.customerDelete?.deletedCustomerId) {
      deletedCustomerIds.add(id);
    }
    cleanup.push(captureCase(variables, result));
  }
  return cleanup;
}

async function main(): Promise<void> {
  await mkdir(outputDir, { recursive: true });

  const stamp = Date.now();
  const formattedCreateSuffix = String(stamp).slice(-4).padStart(4, '0');
  const updateSetupSuffix = String(stamp + 1)
    .slice(-4)
    .padStart(4, '0');
  const updateFormattedSuffix = String(stamp + 2)
    .slice(-4)
    .padStart(4, '0');
  const createFormattedVariables = {
    input: {
      firstName: 'Hermes',
      lastName: 'PhoneNormalization',
      phone: `+1 (613) 450-${formattedCreateSuffix}`,
      tags: [`phone-normalization-${stamp}`],
    },
  };
  const createFormatted = await runCreate(createFormattedVariables, 'customerCreate formatted phone');

  const duplicateNormalizedVariables = {
    input: {
      firstName: 'Hermes',
      lastName: 'PhoneDuplicate',
      phone: `+1613450${formattedCreateSuffix}`,
    },
  };
  const duplicateNormalized = await runCreate(
    duplicateNormalizedVariables,
    'customerCreate duplicate normalized phone',
  );

  const updateSetupVariables = {
    input: {
      firstName: 'Hermes',
      lastName: 'PhoneUpdateSetup',
      phone: `+1415555${updateSetupSuffix}`,
      tags: [`phone-update-${stamp}`],
    },
  };
  const updateSetup = await runCreate(updateSetupVariables, 'customerUpdate setup');
  const updateSetupId = updateSetup.response?.data?.customerCreate?.customer?.id;
  if (typeof updateSetupId !== 'string' || !updateSetupId) {
    throw new Error(`customerUpdate setup did not return an id: ${JSON.stringify(updateSetup.response, null, 2)}`);
  }

  const updateFormattedVariables = {
    input: {
      id: updateSetupId,
      phone: `+1-613-450-${updateFormattedSuffix}`,
    },
  };
  const updateFormatted = await runUpdate(updateFormattedVariables, 'customerUpdate formatted phone');

  const invalidVariables = {
    input: {
      phone: '+1234abcd',
    },
  };
  const invalid = await runCreate(invalidVariables, 'customerCreate invalid phone');

  const tooLongVariables = {
    input: {
      phone: `+${'1'.repeat(255)}`,
    },
  };
  const tooLong = await runCreate(tooLongVariables, 'customerCreate too-long phone');

  const setInvalidVariables = {
    identifier: {
      email: `hermes-phone-set-invalid-${stamp}@example.com`,
    },
    input: {
      email: `hermes-phone-set-invalid-${stamp}@example.com`,
      phone: '+1234abcd',
    },
  };
  const setInvalid = await runCustomerSet(setInvalidVariables, 'customerSet invalid phone');

  const cleanup = await cleanupCustomers();

  const capture = {
    storeDomain,
    apiVersion,
    createFormatted,
    duplicateNormalized,
    updateSetup,
    updateFormatted,
    invalid,
    tooLong,
    setInvalid,
    cleanup,
  };

  const outputPath = path.join(outputDir, 'customer-phone-normalization.json');
  await writeFile(outputPath, `${JSON.stringify(capture, null, 2)}\n`);
  console.log(`Wrote ${outputPath}`);
}

await main();

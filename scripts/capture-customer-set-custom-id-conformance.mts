/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'customers');
const parityRequestDir = path.join('config', 'parity-requests', 'customers');
const definitionCreateDocument = await readFile(
  path.join(parityRequestDir, 'customer-set-custom-id-definition-create.graphql'),
  'utf8',
);
const definitionDeleteDocument = await readFile(
  path.join(parityRequestDir, 'customer-set-custom-id-definition-delete.graphql'),
  'utf8',
);
const customerSetDocument = await readFile(path.join(parityRequestDir, 'customer-set-custom-id.graphql'), 'utf8');
const readDocument = await readFile(path.join(parityRequestDir, 'customer-set-custom-id-read.graphql'), 'utf8');
const outputPath = path.join(outputDir, 'customer-set-custom-id.json');
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const customerDeleteDocument = `#graphql
  mutation CustomerSetCustomIdCustomerDelete($input: CustomerDeleteInput!) {
    customerDelete(input: $input) {
      deletedCustomerId
      userErrors {
        field
        message
      }
    }
  }
`;

function assertHttpOk(result, context) {
  if (result.status < 200 || result.status >= 300) {
    throw new Error(`${context} returned HTTP ${result.status}: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertNoTopLevelErrors(step, context) {
  if (step.response?.errors) {
    throw new Error(`${context} returned top-level errors: ${JSON.stringify(step.response, null, 2)}`);
  }
}

function assertNoUserErrors(step, rootPath, context) {
  const userErrors = readPath(step.response, rootPath)?.userErrors;
  if (!Array.isArray(userErrors) || userErrors.length > 0) {
    throw new Error(`${context} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function readPath(value, segments) {
  let cursor = value;
  for (const segment of segments) {
    if (cursor === null || typeof cursor !== 'object') {
      return undefined;
    }
    cursor = cursor[segment];
  }
  return cursor;
}

function readStringPath(value, segments, context) {
  const result = readPath(value, segments);
  if (typeof result !== 'string' || result.length === 0) {
    throw new Error(`${context} missing string at ${segments.join('.')}: ${JSON.stringify(value, null, 2)}`);
  }
  return result;
}

async function captureStep(label, query, variables) {
  const result = await runGraphqlRequest(query, variables);
  assertHttpOk(result, label);
  return {
    label,
    request: { query, variables },
    response: result.payload,
  };
}

function definitionVariables(namespace, key, name, type, capabilities = undefined) {
  const definition = {
    ownerType: 'CUSTOMER',
    namespace,
    key,
    name,
    type,
  };
  if (capabilities) {
    definition.capabilities = capabilities;
  }
  return { definition };
}

function customerSetVariables(namespace, key, identifierValue, input) {
  return {
    namespace,
    key,
    identifier: { customId: { namespace, key, value: identifierValue } },
    input,
  };
}

async function cleanupCustomer(id) {
  return captureStep('cleanup customerDelete', customerDeleteDocument, { input: { id } }).catch((error) => ({
    label: 'cleanup customerDelete',
    id,
    error: error instanceof Error ? error.message : String(error),
  }));
}

async function cleanupDefinition(id) {
  return captureStep('cleanup metafieldDefinitionDelete', definitionDeleteDocument, {
    id,
    deleteAllAssociatedMetafields: true,
  }).catch((error) => ({
    label: 'cleanup metafieldDefinitionDelete',
    id,
    error: error instanceof Error ? error.message : String(error),
  }));
}

const stamp = Date.now().toString(36);
const namespace = `custom_set_${stamp}`;
const customIdKey = 'external_id';
const disabledKey = 'disabled_external_id';
const missingKey = 'missing_external_id';
const createValue = `created-${stamp}`;
const mismatchIdentifierValue = `mismatch-${stamp}`;
const mismatchInputValue = `input-${stamp}`;
const duplicateIdentifierValue = `duplicate-${stamp}`;
const createdCustomerIds = new Set();
const createdDefinitionIds = new Set();

let validDefinition = null;
let disabledDefinition = null;
let createNoMatch = null;
let updateMatching = null;
let readBack = null;
let missingDefinition = null;
let disabledUnique = null;
let mismatch = null;
let malformed = null;
let duplicateAssignment = null;
let cleanup = null;

try {
  validDefinition = await captureStep(
    'metafieldDefinitionCreate valid CUSTOMER id customId definition',
    definitionCreateDocument,
    definitionVariables(namespace, customIdKey, `Customer Custom ID ${stamp}`, 'id'),
  );
  assertNoTopLevelErrors(validDefinition, 'valid metafieldDefinitionCreate');
  assertNoUserErrors(validDefinition, ['data', 'metafieldDefinitionCreate'], 'valid metafieldDefinitionCreate');
  createdDefinitionIds.add(
    readStringPath(
      validDefinition.response,
      ['data', 'metafieldDefinitionCreate', 'createdDefinition', 'id'],
      'valid metafieldDefinitionCreate',
    ),
  );

  disabledDefinition = await captureStep(
    'metafieldDefinitionCreate disabled CUSTOMER id customId definition',
    definitionCreateDocument,
    definitionVariables(namespace, disabledKey, `Customer Disabled Custom ID ${stamp}`, 'id', {
      uniqueValues: { enabled: false },
    }),
  );
  assertNoTopLevelErrors(disabledDefinition, 'disabled metafieldDefinitionCreate');
  const disabledDefinitionId = readPath(disabledDefinition.response, [
    'data',
    'metafieldDefinitionCreate',
    'createdDefinition',
    'id',
  ]);
  if (typeof disabledDefinitionId === 'string' && disabledDefinitionId.length > 0) {
    createdDefinitionIds.add(disabledDefinitionId);
  }

  createNoMatch = await captureStep(
    'customerSet customId no-match create',
    customerSetDocument,
    customerSetVariables(namespace, customIdKey, createValue, {
      firstName: 'CustomId',
      lastName: 'Created',
    }),
  );
  assertNoTopLevelErrors(createNoMatch, 'customerSet customId no-match create');
  assertNoUserErrors(createNoMatch, ['data', 'customerSet'], 'customerSet customId no-match create');
  const createdCustomerId = readStringPath(
    createNoMatch.response,
    ['data', 'customerSet', 'customer', 'id'],
    'customerSet customId no-match create',
  );
  createdCustomerIds.add(createdCustomerId);

  updateMatching = await captureStep(
    'customerSet customId matching update',
    customerSetDocument,
    customerSetVariables(namespace, customIdKey, createValue, {
      firstName: 'CustomId',
      lastName: 'Updated',
    }),
  );
  assertNoTopLevelErrors(updateMatching, 'customerSet customId matching update');
  assertNoUserErrors(updateMatching, ['data', 'customerSet'], 'customerSet customId matching update');

  readBack = await captureStep('customerSet customId readback', readDocument, {
    identifier: { customId: { namespace, key: customIdKey, value: createValue } },
    customerId: createdCustomerId,
    namespace,
    key: customIdKey,
  });
  assertNoTopLevelErrors(readBack, 'customerSet customId readback');

  missingDefinition = await captureStep(
    'customerSet customId missing definition',
    customerSetDocument,
    customerSetVariables(namespace, missingKey, `missing-${stamp}`, {
      firstName: 'CustomId',
      lastName: 'MissingDefinition',
    }),
  );

  disabledUnique = await captureStep(
    'customerSet customId disabled unique definition',
    customerSetDocument,
    customerSetVariables(namespace, disabledKey, `disabled-${stamp}`, {
      firstName: 'CustomId',
      lastName: 'DisabledUnique',
    }),
  );

  mismatch = await captureStep(
    'customerSet customId input mismatch',
    customerSetDocument,
    customerSetVariables(namespace, customIdKey, mismatchIdentifierValue, {
      firstName: 'CustomId',
      lastName: 'Mismatch',
      metafields: [
        {
          namespace,
          key: customIdKey,
          type: 'id',
          value: mismatchInputValue,
        },
      ],
    }),
  );

  malformed = await captureStep('customerSet customId malformed identifier', customerSetDocument, {
    namespace,
    key: customIdKey,
    identifier: { customId: { namespace, value: `malformed-${stamp}` } },
    input: {
      firstName: 'CustomId',
      lastName: 'Malformed',
    },
  });

  duplicateAssignment = await captureStep(
    'customerSet customId duplicate assignment probe',
    customerSetDocument,
    customerSetVariables(namespace, customIdKey, duplicateIdentifierValue, {
      firstName: 'CustomId',
      lastName: 'DuplicateAssignment',
      metafields: [
        {
          namespace,
          key: customIdKey,
          type: 'id',
          value: createValue,
        },
      ],
    }),
  );

  cleanup = {
    customers: await Promise.all([...createdCustomerIds].map((id) => cleanupCustomer(id))),
    definitions: await Promise.all([...createdDefinitionIds].map((id) => cleanupDefinition(id))),
  };

  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        scenarioId: 'customer-set-custom-id',
        storeDomain,
        apiVersion,
        capturedAt: new Date().toISOString(),
        notes:
          'Live Shopify Admin GraphQL customerSet(identifier.customId) evidence. Setup creates CUSTOMER id metafield definitions, records no-match create, matching update, downstream customerByIdentifier/metafield readback, missing/disabled custom-id definition, input mismatch, malformed identifier, and a duplicate-assignment probe. Cleanup deletes created customers and definitions.',
        cases: {
          validDefinition,
          disabledDefinition,
          createNoMatch,
          updateMatching,
          readBack,
          validations: {
            missingDefinition,
            disabledUnique,
            mismatch,
            malformed,
            duplicateAssignment,
          },
        },
        cleanup,
        upstreamCalls: [],
      },
      null,
      2,
    )}\n`,
    'utf8',
  );
  console.log(JSON.stringify({ ok: true, outputPath, namespace, customerId: createdCustomerId }, null, 2));
} catch (error) {
  cleanup = {
    customers: await Promise.all([...createdCustomerIds].map((id) => cleanupCustomer(id))),
    definitions: await Promise.all([...createdDefinitionIds].map((id) => cleanupDefinition(id))),
  };
  console.error(JSON.stringify({ ok: false, cleanup }, null, 2));
  throw error;
}

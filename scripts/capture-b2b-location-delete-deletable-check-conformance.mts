/* oxlint-disable no-console -- CLI capture script intentionally reports progress/output. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;
type RecordedOperation = {
  request: {
    query: string;
    variables: JsonRecord;
  };
  response: JsonRecord;
};

const scenarioId = 'location-delete-failed-deletable-check';
const timestamp = Date.now();
const runKey = `b2b-location-delete-check-${timestamp}`;
const missingLocationId = 'gid://shopify/CompanyLocation/999999999999999';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const companyCreateDocument = `#graphql
  mutation B2BLocationDeleteCheckCompanyCreate($input: CompanyCreateInput!) {
    companyCreate(input: $input) {
      company {
        id
        name
        mainContact {
          id
        }
        locations(first: 5) {
          nodes {
            id
            name
          }
        }
        contactRoles(first: 5) {
          nodes {
            id
            name
          }
        }
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const locationCreateDocument = `#graphql
  mutation B2BLocationDeleteCheckLocationCreate($companyId: ID!, $input: CompanyLocationInput!) {
    companyLocationCreate(companyId: $companyId, input: $input) {
      companyLocation {
        id
        name
        company {
          id
          name
        }
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const locationDeleteDocument = `#graphql
  mutation B2BLocationDeleteCheckLocationDelete($companyLocationId: ID!) {
    companyLocationDelete(companyLocationId: $companyLocationId) {
      deletedCompanyLocationId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const locationsDeleteDocument = `#graphql
  mutation B2BLocationDeleteCheckLocationsDelete($companyLocationIds: [ID!]!) {
    companyLocationsDelete(companyLocationIds: $companyLocationIds) {
      deletedCompanyLocationIds
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const locationReadDocument = `#graphql
  query B2BLocationDeleteCheckRead($companyId: ID!, $companyLocationId: ID!) {
    company(id: $companyId) {
      id
      locations(first: 5) {
        nodes {
          id
          name
        }
      }
    }
    companyLocation(id: $companyLocationId) {
      id
      name
    }
  }
`;

const bulkReadDocument = `#graphql
  query B2BLocationDeleteCheckBulkRead($blockedCompanyLocationId: ID!, $deletedCompanyLocationId: ID!) {
    blocked: companyLocation(id: $blockedCompanyLocationId) {
      id
      name
    }
    deleted: companyLocation(id: $deletedCompanyLocationId) {
      id
      name
    }
  }
`;

const draftOrderCreateDocument = `#graphql
  mutation B2BLocationDeleteCheckDraftOrderCreate($input: DraftOrderInput!) {
    draftOrderCreate(input: $input) {
      draftOrder {
        id
        status
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const draftOrderDeleteDocument = `#graphql
  mutation B2BLocationDeleteCheckDraftOrderDelete($input: DraftOrderDeleteInput!) {
    draftOrderDelete(input: $input) {
      deletedId
      userErrors {
        field
        message
      }
    }
  }
`;

const storeCreditAccountCreditDocument = `#graphql
  mutation B2BLocationDeleteCheckStoreCreditCredit($id: ID!, $creditInput: StoreCreditAccountCreditInput!) {
    storeCreditAccountCredit(id: $id, creditInput: $creditInput) {
      storeCreditAccountTransaction {
        amount {
          amount
          currencyCode
        }
        balanceAfterTransaction {
          amount
          currencyCode
        }
        event
        origin
        account {
          id
          balance {
            amount
            currencyCode
          }
          owner {
            ... on CompanyLocation {
              id
            }
          }
        }
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const storeCreditAccountDebitDocument = `#graphql
  mutation B2BLocationDeleteCheckStoreCreditDebit($id: ID!, $debitInput: StoreCreditAccountDebitInput!) {
    storeCreditAccountDebit(id: $id, debitInput: $debitInput) {
      storeCreditAccountTransaction {
        amount {
          amount
          currencyCode
        }
        balanceAfterTransaction {
          amount
          currencyCode
        }
        account {
          id
          balance {
            amount
            currencyCode
          }
        }
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const companyDeleteDocument = `#graphql
  mutation B2BLocationDeleteCheckCompanyDelete($id: ID!) {
    companyDelete(id: $id) {
      deletedCompanyId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

function readRecord(value: unknown): JsonRecord | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readPath(value: unknown, pathSegments: string[]): unknown {
  let current = value;
  for (const segment of pathSegments) {
    if (Array.isArray(current)) {
      const index = Number(segment);
      if (!Number.isInteger(index) || index < 0) {
        return undefined;
      }
      current = current[index];
      continue;
    }

    const record = readRecord(current);
    if (!record) {
      return undefined;
    }
    current = record[segment];
  }
  return current;
}

function readStringAtPath(value: unknown, pathSegments: string[], label: string): string {
  const pathValue = readPath(value, pathSegments);
  if (typeof pathValue !== 'string' || pathValue.length === 0) {
    throw new Error(`${label} did not return a string at ${pathSegments.join('.')}: ${JSON.stringify(value, null, 2)}`);
  }
  return pathValue;
}

function readOptionalStringAtPath(value: unknown, pathSegments: string[]): string | null {
  const pathValue = readPath(value, pathSegments);
  return typeof pathValue === 'string' && pathValue.length > 0 ? pathValue : null;
}

function readUserErrors(payload: unknown, root: string): JsonRecord[] {
  const value = readPath(payload, ['data', root, 'userErrors']);
  return Array.isArray(value) ? value.filter((item): item is JsonRecord => readRecord(item) !== null) : [];
}

function assertHttpGraphqlOk(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} returned top-level errors: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertSuccessful(result: ConformanceGraphqlResult, root: string, label: string): void {
  assertHttpGraphqlOk(result, label);
  const userErrors = readUserErrors(result.payload, root);
  if (userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function assertUserError(
  operation: RecordedOperation,
  root: string,
  expected: { field: string[] | null; code: string; messageIncludes: string },
  label: string,
): void {
  const userErrors = readUserErrors(operation.response, root);
  const matchingError = userErrors.find((error) => {
    const message = error['message'];
    return (
      JSON.stringify(error['field'] ?? null) === JSON.stringify(expected.field) &&
      error['code'] === expected.code &&
      typeof message === 'string' &&
      message.includes(expected.messageIncludes)
    );
  });

  if (!matchingError) {
    throw new Error(
      `${label} did not return expected userError ${JSON.stringify(expected)}: ${JSON.stringify(userErrors, null, 2)}`,
    );
  }
}

function assertNullAtPath(operation: RecordedOperation, pathSegments: string[], label: string): void {
  const value = readPath(operation.response, pathSegments);
  if (value !== null) {
    throw new Error(
      `${label} expected null at ${pathSegments.join('.')}: ${JSON.stringify(operation.response, null, 2)}`,
    );
  }
}

function recordOperation(query: string, variables: JsonRecord, result: ConformanceGraphqlResult): RecordedOperation {
  return {
    request: { query, variables },
    response: {
      status: result.status,
      ...result.payload,
    },
  };
}

async function runOperation(query: string, variables: JsonRecord): Promise<RecordedOperation> {
  return recordOperation(query, variables, await runGraphqlRequest(query, variables));
}

async function runRequired(
  query: string,
  variables: JsonRecord,
  root: string,
  label: string,
): Promise<RecordedOperation> {
  const result = await runGraphqlRequest(query, variables);
  assertSuccessful(result, root, label);
  return recordOperation(query, variables, result);
}

async function runCleanup(query: string, variables: JsonRecord): Promise<RecordedOperation> {
  return runOperation(query, variables);
}

function companyCreateVariables(label: string, withContact = false): JsonRecord {
  const input: JsonRecord = {
    company: {
      name: `B2B Location Delete Check ${label} ${timestamp}`,
      note: `B2B location delete deletable-check ${label}`,
      externalId: `${runKey}-${label}`,
    },
    companyLocation: {
      name: `${label} HQ`,
      phone: '+16135550717',
      billingAddress: {
        address1: '717 B2B Way',
        city: 'Ottawa',
        countryCode: 'CA',
      },
    },
  };

  if (withContact) {
    input['companyContact'] = {
      firstName: 'Location',
      lastName: 'Delete',
      email: `${runKey}-${label}@example.com`,
      title: 'Buyer',
    };
  }

  return { input };
}

function locationCreateVariables(label: string): JsonRecord {
  return {
    input: {
      name: `${label} Branch`,
      phone: '+16135550718',
      billingAddress: {
        address1: '718 B2B Way',
        city: 'Ottawa',
        countryCode: 'CA',
      },
    },
  };
}

function draftOrderVariables(companyId: string, companyContactId: string, companyLocationId: string): JsonRecord {
  return {
    input: {
      purchasingEntity: {
        purchasingCompany: {
          companyId,
          companyContactId,
          companyLocationId,
        },
      },
      email: `${runKey}-draft-order@example.com`,
      note: 'B2B location delete draft-order precondition',
      tags: ['b2b-location-delete-check'],
      visibleToCustomer: false,
      lineItems: [
        {
          title: 'B2B location delete custom item',
          quantity: 1,
          originalUnitPrice: '1.00',
          requiresShipping: false,
          taxable: false,
        },
      ],
    },
  };
}

function companyIdsFromCreate(
  operation: RecordedOperation,
  label: string,
): {
  companyId: string;
  locationId: string;
  contactId: string | null;
} {
  return {
    companyId: readStringAtPath(operation.response, ['data', 'companyCreate', 'company', 'id'], `${label} company id`),
    locationId: readStringAtPath(
      operation.response,
      ['data', 'companyCreate', 'company', 'locations', 'nodes', '0', 'id'],
      `${label} location id`,
    ),
    contactId: readOptionalStringAtPath(operation.response, ['data', 'companyCreate', 'company', 'mainContact', 'id']),
  };
}

const createdCompanyIds: string[] = [];
const cleanup: Record<string, RecordedOperation> = {};
let draftOrderId: string | null = null;
let storeCreditAccountId: string | null = null;
let storeCreditCleanupAmount = '6.00';

try {
  const onlyLocationCompanyCreate = await runRequired(
    companyCreateDocument,
    companyCreateVariables('only-location'),
    'companyCreate',
    'only-location companyCreate',
  );
  const onlyLocationCompany = companyIdsFromCreate(onlyLocationCompanyCreate, 'only-location');
  createdCompanyIds.push(onlyLocationCompany.companyId);

  const onlyLocationDelete = await runOperation(locationDeleteDocument, {
    companyLocationId: onlyLocationCompany.locationId,
  });
  assertUserError(
    onlyLocationDelete,
    'companyLocationDelete',
    {
      field: ['companyLocationId'],
      code: 'FAILED_TO_DELETE',
      messageIncludes: 'Failed to delete the company location.',
    },
    'only-location companyLocationDelete',
  );
  assertNullAtPath(
    onlyLocationDelete,
    ['data', 'companyLocationDelete', 'deletedCompanyLocationId'],
    'only-location companyLocationDelete',
  );

  const readAfterOnlyLocationDelete = await runRequired(
    locationReadDocument,
    {
      companyId: onlyLocationCompany.companyId,
      companyLocationId: onlyLocationCompany.locationId,
    },
    'company',
    'read after only-location companyLocationDelete',
  );

  const draftOrderCompanyCreate = await runRequired(
    companyCreateDocument,
    companyCreateVariables('draft-order', true),
    'companyCreate',
    'draft-order companyCreate',
  );
  const draftOrderCompany = companyIdsFromCreate(draftOrderCompanyCreate, 'draft-order');
  if (!draftOrderCompany.contactId) {
    throw new Error(
      `draft-order companyCreate did not return a main contact: ${JSON.stringify(draftOrderCompanyCreate)}`,
    );
  }
  createdCompanyIds.push(draftOrderCompany.companyId);

  const draftOrderExtraLocationCreate = await runRequired(
    locationCreateDocument,
    {
      companyId: draftOrderCompany.companyId,
      ...locationCreateVariables('draft-order'),
    },
    'companyLocationCreate',
    'draft-order extra companyLocationCreate',
  );

  const draftOrderCreate = await runRequired(
    draftOrderCreateDocument,
    draftOrderVariables(draftOrderCompany.companyId, draftOrderCompany.contactId, draftOrderCompany.locationId),
    'draftOrderCreate',
    'draftOrderCreate location delete precondition',
  );
  draftOrderId = readStringAtPath(
    draftOrderCreate.response,
    ['data', 'draftOrderCreate', 'draftOrder', 'id'],
    'draftOrderCreate id',
  );

  const draftOrderLocationDelete = await runOperation(locationDeleteDocument, {
    companyLocationId: draftOrderCompany.locationId,
  });
  assertUserError(
    draftOrderLocationDelete,
    'companyLocationDelete',
    {
      field: ['companyLocationId'],
      code: 'FAILED_TO_DELETE',
      messageIncludes: 'Failed to delete the company location.',
    },
    'draft-order companyLocationDelete',
  );
  assertNullAtPath(
    draftOrderLocationDelete,
    ['data', 'companyLocationDelete', 'deletedCompanyLocationId'],
    'draft-order companyLocationDelete',
  );

  const readAfterDraftOrderLocationDelete = await runRequired(
    locationReadDocument,
    {
      companyId: draftOrderCompany.companyId,
      companyLocationId: draftOrderCompany.locationId,
    },
    'company',
    'read after draft-order companyLocationDelete',
  );

  const storeCreditCompanyCreate = await runRequired(
    companyCreateDocument,
    companyCreateVariables('store-credit'),
    'companyCreate',
    'store-credit companyCreate',
  );
  const storeCreditCompany = companyIdsFromCreate(storeCreditCompanyCreate, 'store-credit');
  createdCompanyIds.push(storeCreditCompany.companyId);

  const storeCreditExtraLocationCreate = await runRequired(
    locationCreateDocument,
    {
      companyId: storeCreditCompany.companyId,
      ...locationCreateVariables('store-credit'),
    },
    'companyLocationCreate',
    'store-credit extra companyLocationCreate',
  );

  const storeCreditAccountCredit = await runRequired(
    storeCreditAccountCreditDocument,
    {
      id: storeCreditCompany.locationId,
      creditInput: {
        creditAmount: {
          amount: '6.00',
          currencyCode: 'USD',
        },
      },
    },
    'storeCreditAccountCredit',
    'storeCreditAccountCredit company-location owner setup',
  );
  storeCreditAccountId = readStringAtPath(
    storeCreditAccountCredit.response,
    ['data', 'storeCreditAccountCredit', 'storeCreditAccountTransaction', 'account', 'id'],
    'storeCreditAccountCredit account id',
  );
  storeCreditCleanupAmount = readStringAtPath(
    storeCreditAccountCredit.response,
    ['data', 'storeCreditAccountCredit', 'storeCreditAccountTransaction', 'balanceAfterTransaction', 'amount'],
    'storeCreditAccountCredit cleanup amount',
  );

  const storeCreditLocationDelete = await runOperation(locationDeleteDocument, {
    companyLocationId: storeCreditCompany.locationId,
  });
  assertUserError(
    storeCreditLocationDelete,
    'companyLocationDelete',
    {
      field: ['companyLocationId'],
      code: 'FAILED_TO_DELETE',
      messageIncludes: 'Failed to delete the company location.',
    },
    'store-credit companyLocationDelete',
  );
  assertNullAtPath(
    storeCreditLocationDelete,
    ['data', 'companyLocationDelete', 'deletedCompanyLocationId'],
    'store-credit companyLocationDelete',
  );

  const readAfterStoreCreditLocationDelete = await runRequired(
    locationReadDocument,
    {
      companyId: storeCreditCompany.companyId,
      companyLocationId: storeCreditCompany.locationId,
    },
    'company',
    'read after store-credit companyLocationDelete',
  );

  const bulkNormalCompanyCreate = await runRequired(
    companyCreateDocument,
    companyCreateVariables('bulk-normal'),
    'companyCreate',
    'bulk normal companyCreate',
  );
  const bulkNormalCompany = companyIdsFromCreate(bulkNormalCompanyCreate, 'bulk normal');
  createdCompanyIds.push(bulkNormalCompany.companyId);

  const bulkNormalExtraLocationCreate = await runRequired(
    locationCreateDocument,
    {
      companyId: bulkNormalCompany.companyId,
      ...locationCreateVariables('bulk-normal'),
    },
    'companyLocationCreate',
    'bulk normal extra companyLocationCreate',
  );
  const bulkDeletedLocationId = readStringAtPath(
    bulkNormalExtraLocationCreate.response,
    ['data', 'companyLocationCreate', 'companyLocation', 'id'],
    'bulk extra location id',
  );

  const bulkLocationsDelete = await runOperation(locationsDeleteDocument, {
    companyLocationIds: [storeCreditCompany.locationId, bulkDeletedLocationId, missingLocationId],
  });
  assertUserError(
    bulkLocationsDelete,
    'companyLocationsDelete',
    {
      field: ['companyLocationIds', '0'],
      code: 'INTERNAL_ERROR',
      messageIncludes: 'CompanyLocation has non-zero store credit balance',
    },
    'bulk store-credit companyLocationsDelete',
  );
  assertUserError(
    bulkLocationsDelete,
    'companyLocationsDelete',
    {
      field: ['companyLocationIds', '2'],
      code: 'RESOURCE_NOT_FOUND',
      messageIncludes: 'Resource requested does not exist.',
    },
    'bulk missing companyLocationsDelete',
  );

  const readAfterBulkLocationsDelete = await runRequired(
    bulkReadDocument,
    {
      blockedCompanyLocationId: storeCreditCompany.locationId,
      deletedCompanyLocationId: bulkDeletedLocationId,
    },
    'blocked',
    'read after bulk companyLocationsDelete',
  );
  assertNullAtPath(readAfterBulkLocationsDelete, ['data', 'deleted'], 'read after bulk companyLocationsDelete');

  if (draftOrderId) {
    cleanup['draftOrderDelete'] = await runCleanup(draftOrderDeleteDocument, { input: { id: draftOrderId } });
  }

  if (storeCreditAccountId) {
    cleanup['storeCreditAccountDebit'] = await runCleanup(storeCreditAccountDebitDocument, {
      id: storeCreditAccountId,
      debitInput: {
        debitAmount: {
          amount: storeCreditCleanupAmount,
          currencyCode: 'USD',
        },
      },
    });
  }

  for (const [index, companyId] of createdCompanyIds.entries()) {
    cleanup[`companyDelete${index + 1}`] = await runCleanup(companyDeleteDocument, { id: companyId });
  }

  const output = {
    scenarioId,
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    intent: {
      plan: 'Record B2B company location delete failed-deletable checks for only-location, draft-order, store-credit, and bulk partial-success branches against a disposable Shopify dev store.',
    },
    onlyLocationCompanyCreate,
    onlyLocationDelete,
    readAfterOnlyLocationDelete,
    draftOrderCompanyCreate,
    draftOrderExtraLocationCreate,
    draftOrderCreate,
    draftOrderLocationDelete,
    readAfterDraftOrderLocationDelete,
    storeCreditCompanyCreate,
    storeCreditExtraLocationCreate,
    storeCreditAccountCredit,
    storeCreditLocationDelete,
    readAfterStoreCreditLocationDelete,
    bulkNormalCompanyCreate,
    bulkNormalExtraLocationCreate,
    bulkLocationsDelete,
    readAfterBulkLocationsDelete,
    cleanup,
    upstreamCalls: [],
  };

  const outputPath = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'b2b', `${scenarioId}.json`);
  await mkdir(path.dirname(outputPath), { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');

  console.log(JSON.stringify({ ok: true, outputPath }, null, 2));
} finally {
  if (draftOrderId && !cleanup['draftOrderDelete']) {
    cleanup['draftOrderDelete'] = await runCleanup(draftOrderDeleteDocument, { input: { id: draftOrderId } });
  }

  if (storeCreditAccountId && !cleanup['storeCreditAccountDebit']) {
    cleanup['storeCreditAccountDebit'] = await runCleanup(storeCreditAccountDebitDocument, {
      id: storeCreditAccountId,
      debitInput: {
        debitAmount: {
          amount: storeCreditCleanupAmount,
          currencyCode: 'USD',
        },
      },
    });
  }

  for (const [index, companyId] of createdCompanyIds.entries()) {
    const cleanupKey = `companyDelete${index + 1}`;
    if (!cleanup[cleanupKey]) {
      cleanup[cleanupKey] = await runCleanup(companyDeleteDocument, { id: companyId });
    }
  }
}

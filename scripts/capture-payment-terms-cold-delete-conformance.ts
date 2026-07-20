/* oxlint-disable no-console -- CLI capture scripts intentionally write status output. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type GraphqlCapture = {
  query: string;
  variables: JsonRecord;
  response: ConformanceGraphqlResult<JsonRecord>;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const outputPath = path.join(
  'fixtures',
  'conformance',
  storeDomain,
  apiVersion,
  'payments',
  'payment-terms-cold-delete.json',
);

const draftOrderCreateDocument = `#graphql
  mutation PaymentTermsColdDeleteDraftOrderCreate($input: DraftOrderInput!) {
    draftOrderCreate(input: $input) {
      draftOrder {
        id
        name
        status
        completedAt
        subtotalPriceSet {
          shopMoney { amount currencyCode }
          presentmentMoney { amount currencyCode }
        }
        totalPriceSet {
          shopMoney { amount currencyCode }
          presentmentMoney { amount currencyCode }
        }
      }
      userErrors { field message }
    }
  }
`;

const draftOrderDeleteDocument = `#graphql
  mutation PaymentTermsColdDeleteDraftOrderCleanup($input: DraftOrderDeleteInput!) {
    draftOrderDelete(input: $input) {
      deletedId
      userErrors { field message }
    }
  }
`;

const paymentTermsCreateDocument = `#graphql
  mutation PaymentTermsColdDeleteSetup($referenceId: ID!, $attrs: PaymentTermsCreateInput!) {
    paymentTermsCreate(referenceId: $referenceId, paymentTermsAttributes: $attrs) {
      paymentTerms {
        id
        due
        overdue
        dueInDays
        paymentTermsName
        paymentTermsType
        translatedName
        paymentSchedules(first: 10) {
          nodes {
            id
            dueAt
            issuedAt
            completedAt
            due
            amount { amount currencyCode }
            balanceDue { amount currencyCode }
            totalBalance { amount currencyCode }
          }
        }
      }
      userErrors { field message code }
    }
  }
`;

// This is the production prerequisite query issued by the proxy. Keep it byte-for-byte
// aligned with PAYMENT_TERMS_DELETE_HYDRATE_QUERY after trimGraphql removes the marker.
const paymentTermsHydrateDocument = `#graphql
  query PaymentTermsDeleteHydrate($id: ID!) {
    paymentTerms: node(id: $id) {
      ... on PaymentTerms {
        id
        due
        overdue
        dueInDays
        paymentTermsName
        paymentTermsType
        translatedName
        order {
          id
          name
          email
          closed
          closedAt
          cancelledAt
          displayFinancialStatus
          totalOutstandingSet {
            shopMoney { amount currencyCode }
            presentmentMoney { amount currencyCode }
          }
          currentTotalPriceSet {
            shopMoney { amount currencyCode }
            presentmentMoney { amount currencyCode }
          }
          totalPriceSet {
            shopMoney { amount currencyCode }
            presentmentMoney { amount currencyCode }
          }
          lineItems(first: 1) {
            nodes {
              sellingPlan {
                name
              }
            }
          }
        }
        draftOrder {
          id
          name
          status
          completedAt
          subtotalPriceSet {
            shopMoney { amount currencyCode }
            presentmentMoney { amount currencyCode }
          }
          totalPriceSet {
            shopMoney { amount currencyCode }
            presentmentMoney { amount currencyCode }
          }
        }
        paymentSchedules(first: 10) {
          nodes {
            id
            dueAt
            issuedAt
            completedAt
            due
            amount { amount currencyCode }
            balanceDue { amount currencyCode }
            totalBalance { amount currencyCode }
          }
        }
      }
    }
  }
`;

function trimGraphql(query: string): string {
  return query.replace(/^#graphql\n/u, '').trim();
}

function asRecord(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readRecord(value: unknown, key: string): JsonRecord | null {
  return asRecord(asRecord(value)?.[key]);
}

function readArray(value: unknown, key: string): unknown[] {
  const result = asRecord(value)?.[key];
  return Array.isArray(result) ? result : [];
}

function readString(value: unknown, key: string): string | null {
  const result = asRecord(value)?.[key];
  return typeof result === 'string' && result.length > 0 ? result : null;
}

async function run(query: string, variables: JsonRecord): Promise<GraphqlCapture> {
  const cleanQuery = trimGraphql(query);
  return {
    query: cleanQuery,
    variables,
    response: await runGraphqlRequest<JsonRecord>(cleanQuery, variables),
  };
}

function assertOk(label: string, capture: GraphqlCapture): void {
  if (capture.response.status < 200 || capture.response.status >= 300 || capture.response.payload['errors']) {
    throw new Error(`${label} failed: ${JSON.stringify(capture.response, null, 2)}`);
  }
}

function assertWrongTypeDelete(label: string, capture: GraphqlCapture, submittedId: string): void {
  if (capture.response.status < 200 || capture.response.status >= 300) {
    throw new Error(`${label} failed transport: ${JSON.stringify(capture.response, null, 2)}`);
  }
  const errors = Array.isArray(capture.response.payload['errors'])
    ? capture.response.payload['errors'].map(asRecord)
    : [];
  const error = errors[0];
  if (
    errors.length !== 1 ||
    error?.['message'] !== `Invalid id: ${submittedId}` ||
    readRecord(error, 'extensions')?.['code'] !== 'RESOURCE_NOT_FOUND' ||
    JSON.stringify(error?.['path']) !== JSON.stringify(['paymentTermsDelete']) ||
    readRecord(capture.response.payload['data'], 'paymentTermsDelete') !== null
  ) {
    throw new Error(`${label} did not match the captured resolver error: ${JSON.stringify(capture.response, null, 2)}`);
  }
}

function assertNoUserErrors(label: string, capture: GraphqlCapture, root: string): void {
  assertOk(label, capture);
  const payload = readRecord(capture.response.payload['data'], root);
  const errors = readArray(payload, 'userErrors');
  if (errors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(errors, null, 2)}`);
  }
}

function capturePayload(capture: GraphqlCapture): JsonRecord {
  return {
    query: capture.query,
    variables: capture.variables,
    response: capture.response.payload,
  };
}

function requireId(value: unknown, label: string): string {
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`${label} did not return an id: ${JSON.stringify(value)}`);
  }
  return value;
}

const deleteDocument = await readFile('config/parity-requests/payments/payment-terms-lifecycle-delete.graphql', 'utf8');
const draftReadDocument = await readFile(
  'config/parity-requests/payments/payment-terms-owner-cascade-draft-read.graphql',
  'utf8',
);
const nodesReadDocument = await readFile(
  'config/parity-requests/payments/payment-terms-cold-delete-nodes-read.graphql',
  'utf8',
);

const stamp = Date.now();
const draftVariables = {
  input: {
    email: `payment-terms-cold-delete-${stamp}@example.com`,
    note: 'payment terms cold delete conformance',
    tags: ['shopify-draft-proxy', 'payment-terms-cold-delete'],
    presentmentCurrencyCode: 'CAD',
    lineItems: [
      {
        title: 'Payment terms cold delete item',
        quantity: 1,
        originalUnitPriceWithCurrency: { amount: '18.50', currencyCode: 'CAD' },
        requiresShipping: false,
        taxable: false,
        sku: `payment-terms-cold-delete-${stamp}`,
      },
    ],
  },
};
const paymentTermsAttributes = {
  paymentTermsTemplateId: 'gid://shopify/PaymentTermsTemplate/4',
  paymentSchedules: [{ issuedAt: '2026-07-20T00:00:00Z' }],
};
const cleanup: JsonRecord = {};
let draftOrderId: string | null = null;
let paymentTermsId: string | null = null;
let fixture: JsonRecord | null = null;

try {
  const draftOrderCreate = await run(draftOrderCreateDocument, draftVariables);
  assertNoUserErrors('draftOrderCreate setup', draftOrderCreate, 'draftOrderCreate');
  const draftOrder = readRecord(
    readRecord(draftOrderCreate.response.payload['data'], 'draftOrderCreate'),
    'draftOrder',
  );
  draftOrderId = requireId(readString(draftOrder, 'id'), 'draftOrderCreate setup');

  const paymentTermsCreate = await run(paymentTermsCreateDocument, {
    referenceId: draftOrderId,
    attrs: paymentTermsAttributes,
  });
  assertNoUserErrors('paymentTermsCreate setup', paymentTermsCreate, 'paymentTermsCreate');
  const createdTerms = readRecord(
    readRecord(paymentTermsCreate.response.payload['data'], 'paymentTermsCreate'),
    'paymentTerms',
  );
  paymentTermsId = requireId(readString(createdTerms, 'id'), 'paymentTermsCreate setup');
  const schedule = asRecord(readArray(readRecord(createdTerms, 'paymentSchedules'), 'nodes')[0]);
  const paymentScheduleId = requireId(readString(schedule, 'id'), 'paymentTermsCreate schedule');

  const hydrate = await run(paymentTermsHydrateDocument, { id: paymentTermsId });
  assertOk('PaymentTermsHydrate prerequisite', hydrate);
  const hydratedTerms = readRecord(hydrate.response.payload['data'], 'paymentTerms');
  if (readString(hydratedTerms, 'id') !== paymentTermsId) {
    throw new Error(
      `PaymentTermsHydrate did not return the created target: ${JSON.stringify(hydrate.response.payload)}`,
    );
  }

  const wrongTypeDelete = await run(deleteDocument, {
    input: { paymentTermsId: draftOrderId },
  });
  assertWrongTypeDelete('wrong-type paymentTermsDelete', wrongTypeDelete, draftOrderId);

  const unknownPaymentTermsId = 'gid://shopify/PaymentTerms/999999999999999';
  const unknownHydrate = await run(paymentTermsHydrateDocument, { id: unknownPaymentTermsId });
  assertOk('unknown PaymentTermsHydrate prerequisite', unknownHydrate);
  if (readRecord(unknownHydrate.response.payload['data'], 'paymentTerms') !== null) {
    throw new Error(
      `unknown PaymentTermsHydrate unexpectedly resolved a target: ${JSON.stringify(unknownHydrate.response.payload)}`,
    );
  }
  const unknownDelete = await run(deleteDocument, {
    input: { paymentTermsId: unknownPaymentTermsId },
  });
  assertOk('unknown paymentTermsDelete', unknownDelete);

  const coldDelete = await run(deleteDocument, {
    input: { paymentTermsId },
  });
  assertNoUserErrors('cold paymentTermsDelete', coldDelete, 'paymentTermsDelete');

  const ownerReadAfterDelete = await run(draftReadDocument, { id: draftOrderId });
  assertOk('owner read after delete', ownerReadAfterDelete);
  const nodesReadAfterDelete = await run(nodesReadDocument, {
    paymentTermsId,
    paymentScheduleId,
  });
  assertOk('terms/schedule reads after delete', nodesReadAfterDelete);

  fixture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    scenarioId: 'payment-terms-cold-delete',
    setup: {
      draftOrderCreate: capturePayload(draftOrderCreate),
      paymentTermsCreate: capturePayload(paymentTermsCreate),
    },
    target: {
      draftOrderId,
      paymentTermsId,
      paymentScheduleId,
    },
    hydrate: capturePayload(hydrate),
    cases: {
      wrongType: capturePayload(wrongTypeDelete),
      unknown: {
        hydrate: capturePayload(unknownHydrate),
        delete: capturePayload(unknownDelete),
      },
      delete: capturePayload(coldDelete),
      ownerReadAfterDelete: capturePayload(ownerReadAfterDelete),
      nodesReadAfterDelete: capturePayload(nodesReadAfterDelete),
    },
    upstreamCalls: [
      {
        operationName: 'PaymentTermsDeleteHydrate',
        variables: hydrate.variables,
        query: hydrate.query,
        response: {
          status: hydrate.response.status,
          body: hydrate.response.payload,
        },
      },
      {
        operationName: 'PaymentTermsDeleteHydrate',
        variables: unknownHydrate.variables,
        query: unknownHydrate.query,
        response: {
          status: unknownHydrate.response.status,
          body: unknownHydrate.response.payload,
        },
      },
    ],
    cleanup,
    notes:
      'Live Shopify cold-start delete evidence. The replay starts with only the persisted PaymentTerms ID, uses the exact query-only hydration response, stages paymentTermsDelete locally, then verifies the DraftOrder owner and deleted PaymentTerms/PaymentSchedule nodes.',
  };
} finally {
  if (draftOrderId) {
    try {
      const draftDelete = await run(draftOrderDeleteDocument, { input: { id: draftOrderId } });
      cleanup['draftOrderDelete'] = capturePayload(draftDelete);
    } catch (error) {
      cleanup['draftOrderDelete'] = { error: error instanceof Error ? error.message : String(error) };
    }
  }
}

if (!fixture) {
  throw new Error('payment-terms cold-delete fixture was not captured');
}
await mkdir(path.dirname(outputPath), { recursive: true });
await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
console.log(JSON.stringify({ ok: true, outputPath }, null, 2));

/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'payments');
const outputPath = path.join(outputDir, 'payment-terms-due-state.json');
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

function assertNoTopLevelErrors(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function readRecord(value: unknown): Record<string, unknown> | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function readArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function userErrors(payload: Record<string, unknown> | null): unknown[] {
  return readArray(payload?.['userErrors']);
}

function assertNoUserErrors(payload: Record<string, unknown> | null, context: string): void {
  const errors = userErrors(payload);
  if (errors.length > 0) {
    throw new Error(`${context} returned userErrors: ${JSON.stringify(errors, null, 2)}`);
  }
}

function paymentTermsFromMutation(
  result: ConformanceGraphqlResult,
  root: 'paymentTermsCreate' | 'paymentTermsUpdate',
  context: string,
): Record<string, unknown> {
  assertNoTopLevelErrors(result, context);
  const data = readRecord(result.payload.data);
  const payload = readRecord(data?.[root]);
  assertNoUserErrors(payload, context);
  const terms = readRecord(payload?.['paymentTerms']);
  if (!terms) {
    throw new Error(`${context} did not return paymentTerms: ${JSON.stringify(result, null, 2)}`);
  }
  return terms;
}

function paymentTermsFromDraftRead(result: ConformanceGraphqlResult, context: string): Record<string, unknown> {
  assertNoTopLevelErrors(result, context);
  const data = readRecord(result.payload.data);
  const draftOrder = readRecord(data?.['draftOrder']);
  const terms = readRecord(draftOrder?.['paymentTerms']);
  if (!terms) {
    throw new Error(`${context} did not return draftOrder.paymentTerms: ${JSON.stringify(result, null, 2)}`);
  }
  return terms;
}

function assertDueState(
  terms: Record<string, unknown>,
  expectedDue: boolean,
  expectedDueAt: string,
  context: string,
): void {
  const schedules = readRecord(terms['paymentSchedules']);
  const nodes = readArray(schedules?.['nodes']);
  const firstSchedule = readRecord(nodes[0]);
  if (!firstSchedule) {
    throw new Error(`${context} did not return a payment schedule: ${JSON.stringify(terms, null, 2)}`);
  }

  const actual = {
    termsDue: terms['due'],
    termsOverdue: terms['overdue'],
    scheduleDueAt: firstSchedule['dueAt'],
    scheduleCompletedAt: firstSchedule['completedAt'],
    scheduleDue: firstSchedule['due'],
  };
  const expected = {
    termsDue: expectedDue,
    termsOverdue: expectedDue,
    scheduleDueAt: expectedDueAt,
    scheduleCompletedAt: null,
    scheduleDue: expectedDue,
  };
  if (JSON.stringify(actual) !== JSON.stringify(expected)) {
    throw new Error(`${context} due-state mismatch: ${JSON.stringify({ expected, actual }, null, 2)}`);
  }
}

const paymentTermsSelection = `#graphql
  paymentTerms {
    id
    due
    overdue
    paymentSchedules(first: 10) {
      nodes {
        dueAt
        completedAt
        due
      }
    }
  }
  userErrors {
    field
    message
    code
  }
`;

const draftOrderCreateDocument = `#graphql
  mutation PaymentTermsDueStateDraftCreate($input: DraftOrderInput!) {
    draftOrderCreate(input: $input) {
      draftOrder {
        id
        name
        subtotalPriceSet {
          shopMoney {
            amount
            currencyCode
          }
        }
        totalPriceSet {
          shopMoney {
            amount
            currencyCode
          }
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const paymentTermsCreateDocument = `#graphql
  mutation PaymentTermsDueStateCreate($referenceId: ID!, $attrs: PaymentTermsCreateInput!) {
    paymentTermsCreate(referenceId: $referenceId, paymentTermsAttributes: $attrs) {
      ${paymentTermsSelection}
    }
  }
`;

const paymentTermsUpdateDocument = `#graphql
  mutation PaymentTermsDueStateUpdate($input: PaymentTermsUpdateInput!) {
    paymentTermsUpdate(input: $input) {
      ${paymentTermsSelection}
    }
  }
`;

const draftOrderReadDocument = `#graphql
  query PaymentTermsDueStateDraftRead($id: ID!) {
    draftOrder(id: $id) {
      paymentTerms {
        id
        due
        overdue
        paymentSchedules(first: 10) {
          nodes {
            dueAt
            completedAt
            due
          }
        }
      }
    }
  }
`;

const paymentTermsDeleteDocument = `#graphql
  mutation PaymentTermsDueStateDelete($input: PaymentTermsDeleteInput!) {
    paymentTermsDelete(input: $input) {
      deletedId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const draftOrderDeleteDocument = `#graphql
  mutation PaymentTermsDueStateDraftDelete($input: DraftOrderDeleteInput!) {
    draftOrderDelete(input: $input) {
      deletedId
      userErrors {
        field
        message
      }
    }
  }
`;

await mkdir(outputDir, { recursive: true });

const runId = Date.now();
const pastDueAt = '2020-01-01T00:00:00Z';
const futureDueAt = '2099-01-01T00:00:00Z';
const pastAttrs = {
  paymentTermsTemplateId: 'gid://shopify/PaymentTermsTemplate/7',
  paymentSchedules: [{ dueAt: pastDueAt }],
};
const futureAttrs = {
  paymentTermsTemplateId: 'gid://shopify/PaymentTermsTemplate/7',
  paymentSchedules: [{ dueAt: futureDueAt }],
};

function draftOrderCreateVariables(label: string): Record<string, unknown> {
  return {
    input: {
      email: `payment-terms-due-state-${label}-${runId}@example.com`,
      lineItems: [
        {
          title: `Payment terms due state ${label}`,
          quantity: 1,
          originalUnitPrice: '18.50',
        },
      ],
    },
  };
}

let pastDraftOrderId: string | null = null;
let pastPaymentTermsId: string | null = null;
let futureDraftOrderId: string | null = null;
let futurePaymentTermsId: string | null = null;
const cleanup: Record<string, unknown> = {};

try {
  const pastDraftOrderCreateVariables = draftOrderCreateVariables('past');
  const pastDraftOrderCreate = await runGraphqlRequest(draftOrderCreateDocument, pastDraftOrderCreateVariables);
  assertNoTopLevelErrors(pastDraftOrderCreate, 'past draftOrderCreate setup');
  const pastDraftOrderCreateData = readRecord(pastDraftOrderCreate.payload.data);
  const pastDraftOrderCreatePayload = readRecord(pastDraftOrderCreateData?.['draftOrderCreate']);
  assertNoUserErrors(pastDraftOrderCreatePayload, 'past draftOrderCreate setup');
  const pastDraftOrder = readRecord(pastDraftOrderCreatePayload?.['draftOrder']);
  pastDraftOrderId = typeof pastDraftOrder?.['id'] === 'string' ? pastDraftOrder['id'] : null;
  if (!pastDraftOrderId) {
    throw new Error(`past draftOrderCreate did not return an id: ${JSON.stringify(pastDraftOrderCreate, null, 2)}`);
  }

  const pastPaymentTermsCreateVariables = { referenceId: pastDraftOrderId, attrs: pastAttrs };
  const pastPaymentTermsCreate = await runGraphqlRequest(paymentTermsCreateDocument, pastPaymentTermsCreateVariables);
  const pastCreatedTerms = paymentTermsFromMutation(pastPaymentTermsCreate, 'paymentTermsCreate', 'past create');
  assertDueState(pastCreatedTerms, true, pastDueAt, 'past create');
  pastPaymentTermsId = typeof pastCreatedTerms['id'] === 'string' ? pastCreatedTerms['id'] : null;
  if (!pastPaymentTermsId) {
    throw new Error(`past create did not return payment terms id: ${JSON.stringify(pastPaymentTermsCreate, null, 2)}`);
  }

  const pastReadAfterCreate = await runGraphqlRequest(draftOrderReadDocument, { id: pastDraftOrderId });
  assertDueState(
    paymentTermsFromDraftRead(pastReadAfterCreate, 'past read after create'),
    true,
    pastDueAt,
    'past read after create',
  );

  const futureDraftOrderCreateVariables = draftOrderCreateVariables('future');
  const futureDraftOrderCreate = await runGraphqlRequest(draftOrderCreateDocument, futureDraftOrderCreateVariables);
  assertNoTopLevelErrors(futureDraftOrderCreate, 'future draftOrderCreate setup');
  const futureDraftOrderCreateData = readRecord(futureDraftOrderCreate.payload.data);
  const futureDraftOrderCreatePayload = readRecord(futureDraftOrderCreateData?.['draftOrderCreate']);
  assertNoUserErrors(futureDraftOrderCreatePayload, 'future draftOrderCreate setup');
  const futureDraftOrder = readRecord(futureDraftOrderCreatePayload?.['draftOrder']);
  futureDraftOrderId = typeof futureDraftOrder?.['id'] === 'string' ? futureDraftOrder['id'] : null;
  if (!futureDraftOrderId) {
    throw new Error(`future draftOrderCreate did not return an id: ${JSON.stringify(futureDraftOrderCreate, null, 2)}`);
  }

  const futurePaymentTermsCreateVariables = { referenceId: futureDraftOrderId, attrs: futureAttrs };
  const futurePaymentTermsCreate = await runGraphqlRequest(
    paymentTermsCreateDocument,
    futurePaymentTermsCreateVariables,
  );
  const futureCreatedTerms = paymentTermsFromMutation(futurePaymentTermsCreate, 'paymentTermsCreate', 'future create');
  assertDueState(futureCreatedTerms, false, futureDueAt, 'future create');
  futurePaymentTermsId = typeof futureCreatedTerms['id'] === 'string' ? futureCreatedTerms['id'] : null;
  if (!futurePaymentTermsId) {
    throw new Error(
      `future create did not return payment terms id: ${JSON.stringify(futurePaymentTermsCreate, null, 2)}`,
    );
  }

  const futureReadAfterCreate = await runGraphqlRequest(draftOrderReadDocument, { id: futureDraftOrderId });
  assertDueState(
    paymentTermsFromDraftRead(futureReadAfterCreate, 'future read after create'),
    false,
    futureDueAt,
    'future read after create',
  );

  const paymentTermsUpdatePastVariables = {
    input: {
      paymentTermsId: futurePaymentTermsId,
      paymentTermsAttributes: pastAttrs,
    },
  };
  const paymentTermsUpdatePast = await runGraphqlRequest(paymentTermsUpdateDocument, paymentTermsUpdatePastVariables);
  assertDueState(
    paymentTermsFromMutation(paymentTermsUpdatePast, 'paymentTermsUpdate', 'past update'),
    true,
    pastDueAt,
    'past update',
  );

  const readAfterPastUpdate = await runGraphqlRequest(draftOrderReadDocument, { id: futureDraftOrderId });
  assertDueState(
    paymentTermsFromDraftRead(readAfterPastUpdate, 'read after past update'),
    true,
    pastDueAt,
    'read after past update',
  );

  const paymentTermsUpdateFutureVariables = {
    input: {
      paymentTermsId: futurePaymentTermsId,
      paymentTermsAttributes: futureAttrs,
    },
  };
  const paymentTermsUpdateFuture = await runGraphqlRequest(
    paymentTermsUpdateDocument,
    paymentTermsUpdateFutureVariables,
  );
  assertDueState(
    paymentTermsFromMutation(paymentTermsUpdateFuture, 'paymentTermsUpdate', 'future update'),
    false,
    futureDueAt,
    'future update',
  );

  const readAfterFutureUpdate = await runGraphqlRequest(draftOrderReadDocument, { id: futureDraftOrderId });
  assertDueState(
    paymentTermsFromDraftRead(readAfterFutureUpdate, 'read after future update'),
    false,
    futureDueAt,
    'read after future update',
  );

  if (pastPaymentTermsId) {
    const pastPaymentTermsDelete = await runGraphqlRequest(paymentTermsDeleteDocument, {
      input: { paymentTermsId: pastPaymentTermsId },
    });
    cleanup['pastPaymentTermsDelete'] = pastPaymentTermsDelete.payload;
    assertNoTopLevelErrors(pastPaymentTermsDelete, 'past paymentTermsDelete cleanup');
    pastPaymentTermsId = null;
  }
  if (futurePaymentTermsId) {
    const futurePaymentTermsDelete = await runGraphqlRequest(paymentTermsDeleteDocument, {
      input: { paymentTermsId: futurePaymentTermsId },
    });
    cleanup['futurePaymentTermsDelete'] = futurePaymentTermsDelete.payload;
    assertNoTopLevelErrors(futurePaymentTermsDelete, 'future paymentTermsDelete cleanup');
    futurePaymentTermsId = null;
  }
  if (pastDraftOrderId) {
    const pastDraftOrderDelete = await runGraphqlRequest(draftOrderDeleteDocument, {
      input: { id: pastDraftOrderId },
    });
    cleanup['pastDraftOrderDelete'] = pastDraftOrderDelete.payload;
    assertNoTopLevelErrors(pastDraftOrderDelete, 'past draftOrderDelete cleanup');
    pastDraftOrderId = null;
  }
  if (futureDraftOrderId) {
    const futureDraftOrderDelete = await runGraphqlRequest(draftOrderDeleteDocument, {
      input: { id: futureDraftOrderId },
    });
    cleanup['futureDraftOrderDelete'] = futureDraftOrderDelete.payload;
    assertNoTopLevelErrors(futureDraftOrderDelete, 'future draftOrderDelete cleanup');
    futureDraftOrderId = null;
  }

  const fixture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    cases: {
      pastCreate: {
        setup: {
          draftOrderCreate: {
            query: draftOrderCreateDocument,
            variables: pastDraftOrderCreateVariables,
            response: pastDraftOrderCreate.payload,
          },
        },
        paymentTermsCreate: {
          query: paymentTermsCreateDocument,
          variables: pastPaymentTermsCreateVariables,
          response: pastPaymentTermsCreate.payload,
        },
        readAfterCreate: {
          query: draftOrderReadDocument,
          variables: { id: pastPaymentTermsCreateVariables.referenceId },
          response: pastReadAfterCreate.payload,
        },
      },
      futureUpdate: {
        setup: {
          draftOrderCreate: {
            query: draftOrderCreateDocument,
            variables: futureDraftOrderCreateVariables,
            response: futureDraftOrderCreate.payload,
          },
        },
        paymentTermsCreate: {
          query: paymentTermsCreateDocument,
          variables: futurePaymentTermsCreateVariables,
          response: futurePaymentTermsCreate.payload,
        },
        readAfterCreate: {
          query: draftOrderReadDocument,
          variables: { id: futurePaymentTermsCreateVariables.referenceId },
          response: futureReadAfterCreate.payload,
        },
        paymentTermsUpdatePast: {
          query: paymentTermsUpdateDocument,
          variables: paymentTermsUpdatePastVariables,
          response: paymentTermsUpdatePast.payload,
        },
        readAfterPastUpdate: {
          query: draftOrderReadDocument,
          variables: { id: futurePaymentTermsCreateVariables.referenceId },
          response: readAfterPastUpdate.payload,
        },
        paymentTermsUpdateFuture: {
          query: paymentTermsUpdateDocument,
          variables: paymentTermsUpdateFutureVariables,
          response: paymentTermsUpdateFuture.payload,
        },
        readAfterFutureUpdate: {
          query: draftOrderReadDocument,
          variables: { id: futurePaymentTermsCreateVariables.referenceId },
          response: readAfterFutureUpdate.payload,
        },
      },
    },
    upstreamCalls: [],
    cleanup,
    notes:
      'Captured on disposable draft orders. Shopify reports due/overdue true for non-completed fixed schedules with dueAt in the past, false for future schedules, and downstream draftOrder.paymentTerms reads preserve the same booleans after create and update.',
  };

  await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  console.log(`Wrote ${outputPath}`);
} finally {
  if (pastPaymentTermsId) {
    cleanup['pastPaymentTermsDeleteAfterFailure'] = (
      await runGraphqlRequest(paymentTermsDeleteDocument, { input: { paymentTermsId: pastPaymentTermsId } })
    ).payload;
  }
  if (futurePaymentTermsId) {
    cleanup['futurePaymentTermsDeleteAfterFailure'] = (
      await runGraphqlRequest(paymentTermsDeleteDocument, { input: { paymentTermsId: futurePaymentTermsId } })
    ).payload;
  }
  if (pastDraftOrderId) {
    cleanup['pastDraftOrderDeleteAfterFailure'] = (
      await runGraphqlRequest(draftOrderDeleteDocument, { input: { id: pastDraftOrderId } })
    ).payload;
  }
  if (futureDraftOrderId) {
    cleanup['futureDraftOrderDeleteAfterFailure'] = (
      await runGraphqlRequest(draftOrderDeleteDocument, { input: { id: futureDraftOrderId } })
    ).payload;
  }
}

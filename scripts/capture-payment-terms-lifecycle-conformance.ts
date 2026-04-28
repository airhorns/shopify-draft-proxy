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
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion);
const outputPath = path.join(outputDir, 'payment-terms-lifecycle.json');
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

const paymentTermsSelection = `#graphql
  paymentTerms {
    id
    due
    overdue
    dueInDays
    paymentTermsName
    paymentTermsType
    translatedName
    paymentSchedules(first: 2) {
      nodes {
        id
        issuedAt
        dueAt
        completedAt
        due
        amount {
          amount
          currencyCode
        }
        balanceDue {
          amount
          currencyCode
        }
        totalBalance {
          amount
          currencyCode
        }
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
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
  mutation PaymentTermsLifecycleDraftCreate($input: DraftOrderInput!) {
    draftOrderCreate(input: $input) {
      draftOrder {
        id
        name
        paymentTerms {
          id
        }
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
  mutation PaymentTermsLifecycleCreate($referenceId: ID!, $attrs: PaymentTermsCreateInput!) {
    paymentTermsCreate(referenceId: $referenceId, paymentTermsAttributes: $attrs) {
      ${paymentTermsSelection}
    }
  }
`;

const paymentTermsUpdateDocument = `#graphql
  mutation PaymentTermsLifecycleUpdate($input: PaymentTermsUpdateInput!) {
    paymentTermsUpdate(input: $input) {
      ${paymentTermsSelection}
    }
  }
`;

const paymentTermsDeleteDocument = `#graphql
  mutation PaymentTermsLifecycleDelete($input: PaymentTermsDeleteInput!) {
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

const draftOrderReadDocument = `#graphql
  query PaymentTermsLifecycleDraftRead($id: ID!) {
    draftOrder(id: $id) {
      id
      paymentTerms {
        id
        due
        overdue
        dueInDays
        paymentTermsName
        paymentTermsType
        translatedName
        paymentSchedules(first: 2) {
          nodes {
            id
            issuedAt
            dueAt
            completedAt
            due
            amount {
              amount
              currencyCode
            }
            balanceDue {
              amount
              currencyCode
            }
            totalBalance {
              amount
              currencyCode
            }
          }
          pageInfo {
            hasNextPage
            hasPreviousPage
            startCursor
            endCursor
          }
        }
      }
    }
  }
`;

const draftOrderDeleteDocument = `#graphql
  mutation PaymentTermsLifecycleDraftCleanup($input: DraftOrderDeleteInput!) {
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
const draftOrderCreateVariables = {
  input: {
    email: `har222-payment-terms-${runId}@example.com`,
    lineItems: [
      {
        title: 'HAR-222 payment terms lifecycle',
        quantity: 1,
        originalUnitPrice: '18.50',
      },
    ],
  },
};
const paymentTermsCreateVariables = {
  referenceId: '',
  attrs: {
    paymentTermsTemplateId: 'gid://shopify/PaymentTermsTemplate/4',
    paymentSchedules: [{ issuedAt: '2026-04-27T12:00:00Z' }],
  },
};
const paymentTermsUpdateVariables = {
  input: {
    paymentTermsId: '',
    paymentTermsAttributes: {
      paymentTermsTemplateId: 'gid://shopify/PaymentTermsTemplate/7',
      paymentSchedules: [{ dueAt: '2026-05-27T12:00:00Z' }],
    },
  },
};

let draftOrderId: string | null = null;
let paymentTermsId: string | null = null;
const cleanup: Record<string, unknown> = {};

try {
  const draftOrderCreate = await runGraphqlRequest(draftOrderCreateDocument, draftOrderCreateVariables);
  assertNoTopLevelErrors(draftOrderCreate, 'draftOrderCreate setup');
  const draftOrderCreateData = readRecord(draftOrderCreate.payload.data);
  const draftOrderCreatePayload = readRecord(draftOrderCreateData?.['draftOrderCreate']);
  const draftOrder = readRecord(draftOrderCreatePayload?.['draftOrder']);
  draftOrderId = typeof draftOrder?.['id'] === 'string' ? draftOrder['id'] : null;
  if (!draftOrderId) {
    throw new Error(`draftOrderCreate did not return a draft order id: ${JSON.stringify(draftOrderCreate, null, 2)}`);
  }

  paymentTermsCreateVariables.referenceId = draftOrderId;
  const paymentTermsCreate = await runGraphqlRequest(paymentTermsCreateDocument, paymentTermsCreateVariables);
  assertNoTopLevelErrors(paymentTermsCreate, 'paymentTermsCreate');
  const paymentTermsCreateData = readRecord(paymentTermsCreate.payload.data);
  const paymentTermsCreatePayload = readRecord(paymentTermsCreateData?.['paymentTermsCreate']);
  const createdPaymentTerms = readRecord(paymentTermsCreatePayload?.['paymentTerms']);
  paymentTermsId = typeof createdPaymentTerms?.['id'] === 'string' ? createdPaymentTerms['id'] : null;
  if (!paymentTermsId) {
    throw new Error(`paymentTermsCreate did not return payment terms: ${JSON.stringify(paymentTermsCreate, null, 2)}`);
  }

  const readAfterCreate = await runGraphqlRequest(draftOrderReadDocument, { id: draftOrderId });
  assertNoTopLevelErrors(readAfterCreate, 'draftOrder read after paymentTermsCreate');

  paymentTermsUpdateVariables.input.paymentTermsId = paymentTermsId;
  const paymentTermsUpdate = await runGraphqlRequest(paymentTermsUpdateDocument, paymentTermsUpdateVariables);
  assertNoTopLevelErrors(paymentTermsUpdate, 'paymentTermsUpdate');

  const readAfterUpdate = await runGraphqlRequest(draftOrderReadDocument, { id: draftOrderId });
  assertNoTopLevelErrors(readAfterUpdate, 'draftOrder read after paymentTermsUpdate');

  const paymentTermsDeleteVariables = { input: { paymentTermsId } };
  const paymentTermsDelete = await runGraphqlRequest(paymentTermsDeleteDocument, paymentTermsDeleteVariables);
  assertNoTopLevelErrors(paymentTermsDelete, 'paymentTermsDelete');
  paymentTermsId = null;

  const readAfterDelete = await runGraphqlRequest(draftOrderReadDocument, { id: draftOrderId });
  assertNoTopLevelErrors(readAfterDelete, 'draftOrder read after paymentTermsDelete');

  const capturedDraftOrderId = draftOrderId;
  const draftOrderDelete = await runGraphqlRequest(draftOrderDeleteDocument, { input: { id: draftOrderId } });
  cleanup['draftOrderDelete'] = draftOrderDelete.payload;
  assertNoTopLevelErrors(draftOrderDelete, 'draftOrderDelete cleanup');
  draftOrderId = null;

  const fixture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    seedDraftOrder: draftOrder,
    setup: {
      draftOrderCreate: {
        query: draftOrderCreateDocument,
        variables: draftOrderCreateVariables,
        response: draftOrderCreate.payload,
      },
    },
    operations: {
      paymentTermsCreate: {
        query: paymentTermsCreateDocument,
        variables: paymentTermsCreateVariables,
        response: paymentTermsCreate.payload,
      },
      paymentTermsUpdate: {
        query: paymentTermsUpdateDocument,
        variables: paymentTermsUpdateVariables,
        response: paymentTermsUpdate.payload,
      },
      paymentTermsDelete: {
        query: paymentTermsDeleteDocument,
        variables: paymentTermsDeleteVariables,
        response: paymentTermsDelete.payload,
      },
    },
    reads: {
      afterCreate: {
        query: draftOrderReadDocument,
        variables: { id: capturedDraftOrderId },
        response: readAfterCreate.payload,
      },
      afterUpdate: {
        query: draftOrderReadDocument,
        variables: { id: capturedDraftOrderId },
        response: readAfterUpdate.payload,
      },
      afterDelete: {
        query: draftOrderReadDocument,
        variables: { id: capturedDraftOrderId },
        response: readAfterDelete.payload,
      },
    },
    cleanup,
    notes:
      'Captured on a disposable draft order. Shopify computes NET dueAt from issuedAt plus template due days, creates a new PaymentSchedule id on FIXED update, and returns null paymentTerms after delete.',
  };

  await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  console.log(`Wrote ${outputPath}`);
} finally {
  if (paymentTermsId) {
    cleanup['paymentTermsDelete'] = (
      await runGraphqlRequest(paymentTermsDeleteDocument, { input: { paymentTermsId } })
    ).payload;
  }
  if (draftOrderId) {
    cleanup['draftOrderDelete'] = (
      await runGraphqlRequest(draftOrderDeleteDocument, { input: { id: draftOrderId } })
    ).payload;
  }
}

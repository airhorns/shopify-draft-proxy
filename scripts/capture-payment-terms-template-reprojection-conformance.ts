/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const scenarioId = 'payment-terms-create-template-reprojection';
const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'payments');
const outputPath = path.join(outputDir, `${scenarioId}.json`);
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

type PaymentTermsCase = {
  emailSuffix: string;
  attrs: Record<string, unknown>;
  purpose: string;
};

type CapturedCase = {
  purpose: string;
  setup: {
    orderCreate: {
      query: string;
      variables: Record<string, unknown>;
      response: unknown;
    };
  };
  paymentTermsCreate: {
    query: string;
    variables: Record<string, unknown>;
    response: unknown;
  };
  cleanup: Record<string, unknown>;
};

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

const orderCreateDocument = `#graphql
  mutation PaymentTermsTemplateReprojectionOrderCreate($order: OrderCreateOrderInput!) {
    orderCreate(order: $order) {
      order {
        id
        currentTotalPriceSet {
          shopMoney {
            amount
            currencyCode
          }
          presentmentMoney {
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

const paymentTermsCreateDocument = `#graphql
  mutation PaymentTermsCreateTemplateReprojection($referenceId: ID!, $attrs: PaymentTermsCreateInput!) {
    paymentTermsCreate(referenceId: $referenceId, paymentTermsAttributes: $attrs) {
      paymentTerms {
        id
        paymentTermsName
        paymentTermsType
        dueInDays
        translatedName
        paymentSchedules(first: 2) {
          nodes {
            id
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

const paymentTermsDeleteDocument = `#graphql
  mutation PaymentTermsTemplateReprojectionTermsCleanup($input: PaymentTermsDeleteInput!) {
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

const orderCancelDocument = `#graphql
  mutation PaymentTermsTemplateReprojectionOrderCleanup($orderId: ID!, $reason: OrderCancelReason!, $notifyCustomer: Boolean!, $restock: Boolean!) {
    orderCancel(orderId: $orderId, reason: $reason, notifyCustomer: $notifyCustomer, restock: $restock) {
      orderCancelUserErrors {
        field
        message
        code
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const cases: Record<string, PaymentTermsCase> = {
  fixed: {
    purpose: 'FIXED template reprojects to Fixed/FIXED/null and keeps the materialized due-date schedule.',
    emailSuffix: 'fixed',
    attrs: {
      paymentTermsTemplateId: 'gid://shopify/PaymentTermsTemplate/7',
      paymentSchedules: [{ dueAt: '2026-07-01T00:00:00Z' }],
    },
  },
  net7: {
    purpose: 'Non-30 NET template reprojects to Net 7/NET/7 and keeps the materialized schedule.',
    emailSuffix: 'net7',
    attrs: {
      paymentTermsTemplateId: 'gid://shopify/PaymentTermsTemplate/2',
      paymentSchedules: [{ issuedAt: '2026-07-01T00:00:00Z' }],
    },
  },
  fulfillment: {
    purpose:
      'FULFILLMENT event template reprojects to Due on fulfillment/FULFILLMENT/null and returns no schedule nodes.',
    emailSuffix: 'fulfillment',
    attrs: {
      paymentTermsTemplateId: 'gid://shopify/PaymentTermsTemplate/9',
    },
  },
};

async function captureCase(name: string, spec: PaymentTermsCase, runId: number) {
  let orderId: string | null = null;
  let paymentTermsId: string | null = null;
  const cleanup: Record<string, unknown> = {};

  const orderVariables = {
    order: {
      email: `payment-terms-template-reprojection-${spec.emailSuffix}-${runId}@example.com`,
      currency: 'USD',
      presentmentCurrency: 'CAD',
      lineItems: [
        {
          title: `Payment terms template reprojection ${name} first`,
          quantity: 2,
          priceSet: {
            shopMoney: {
              amount: '3.25',
              currencyCode: 'USD',
            },
            presentmentMoney: {
              amount: '4.50',
              currencyCode: 'CAD',
            },
          },
        },
        {
          title: `Payment terms template reprojection ${name} second`,
          quantity: 3,
          priceSet: {
            shopMoney: {
              amount: '4.50',
              currencyCode: 'USD',
            },
            presentmentMoney: {
              amount: '6.00',
              currencyCode: 'CAD',
            },
          },
        },
      ],
    },
  };

  try {
    const orderCreate = await runGraphqlRequest(orderCreateDocument, orderVariables);
    assertNoTopLevelErrors(orderCreate, `${name} orderCreate setup`);
    const orderCreateData = readRecord(orderCreate.payload.data);
    const orderCreatePayload = readRecord(orderCreateData?.['orderCreate']);
    const order = readRecord(orderCreatePayload?.['order']);
    orderId = typeof order?.['id'] === 'string' ? order['id'] : null;
    if (!orderId) {
      throw new Error(`${name} orderCreate did not return an id: ${JSON.stringify(orderCreate, null, 2)}`);
    }

    const paymentTermsVariables = { referenceId: orderId, attrs: spec.attrs };
    const paymentTermsCreate = await runGraphqlRequest(paymentTermsCreateDocument, paymentTermsVariables);
    assertNoTopLevelErrors(paymentTermsCreate, `${name} paymentTermsCreate`);
    const paymentTermsCreateData = readRecord(paymentTermsCreate.payload.data);
    const paymentTermsCreatePayload = readRecord(paymentTermsCreateData?.['paymentTermsCreate']);
    const paymentTerms = readRecord(paymentTermsCreatePayload?.['paymentTerms']);
    paymentTermsId = typeof paymentTerms?.['id'] === 'string' ? paymentTerms['id'] : null;
    if (!paymentTermsId) {
      throw new Error(
        `${name} paymentTermsCreate did not return payment terms: ${JSON.stringify(paymentTermsCreate, null, 2)}`,
      );
    }

    const paymentTermsDelete = await runGraphqlRequest(paymentTermsDeleteDocument, {
      input: { paymentTermsId },
    });
    cleanup['paymentTermsDelete'] = paymentTermsDelete.payload;
    assertNoTopLevelErrors(paymentTermsDelete, `${name} paymentTermsDelete cleanup`);
    paymentTermsId = null;

    const orderCancel = await runGraphqlRequest(orderCancelDocument, {
      orderId,
      reason: 'OTHER',
      notifyCustomer: false,
      restock: false,
    });
    cleanup['orderCancel'] = orderCancel.payload;
    assertNoTopLevelErrors(orderCancel, `${name} orderCancel cleanup`);
    orderId = null;

    return {
      purpose: spec.purpose,
      setup: {
        orderCreate: {
          query: orderCreateDocument,
          variables: orderVariables,
          response: orderCreate.payload,
        },
      },
      paymentTermsCreate: {
        query: paymentTermsCreateDocument,
        variables: paymentTermsVariables,
        response: paymentTermsCreate.payload,
      },
      cleanup,
    };
  } finally {
    if (paymentTermsId) {
      cleanup['paymentTermsDelete'] = (
        await runGraphqlRequest(paymentTermsDeleteDocument, { input: { paymentTermsId } })
      ).payload;
    }
    if (orderId) {
      cleanup['orderCancel'] = (
        await runGraphqlRequest(orderCancelDocument, {
          orderId,
          reason: 'OTHER',
          notifyCustomer: false,
          restock: false,
        })
      ).payload;
    }
  }
}

await mkdir(outputDir, { recursive: true });

const runId = Date.now();
const capturedCases: Record<string, CapturedCase> = {};
for (const [name, spec] of Object.entries(cases)) {
  capturedCases[name] = await captureCase(name, spec, runId);
}
const fixedCase = capturedCases['fixed'];
const net7Case = capturedCases['net7'];
const fulfillmentCase = capturedCases['fulfillment'];
if (!fixedCase || !net7Case || !fulfillmentCase) {
  throw new Error('Expected fixed, net7, and fulfillment captures before writing fixture.');
}

const fixture = {
  scenarioId,
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  upstreamCalls: [],
  cases: capturedCases,
  expected: {
    localMutationLogVariables: {
      '1': {
        variables: {
          attrs: fixedCase.paymentTermsCreate.variables['attrs'],
        },
      },
      '3': {
        variables: {
          attrs: net7Case.paymentTermsCreate.variables['attrs'],
        },
      },
      '5': {
        variables: {
          attrs: fulfillmentCase.paymentTermsCreate.variables['attrs'],
        },
      },
    },
  },
  notes:
    'Captured against disposable Orders. The scenario verifies successful paymentTermsCreate reprojects name/type/dueInDays/translatedName from FIXED, Net 7, and FULFILLMENT templates, preserves the captured schedule-node cardinality, and records multi-line order totals used by Order-owned payment terms.',
};

await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
console.log(`Wrote ${outputPath}`);

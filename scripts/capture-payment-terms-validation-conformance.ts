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
const outputPath = path.join(outputDir, 'payment-terms-create-template-and-schedule-validation.json');
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

const paymentTermsTemplatesDocument = `#graphql
  query PaymentTermsTemplatesRead($type: PaymentTermsType) {
    all: paymentTermsTemplates {
      id
      name
      description
      dueInDays
      paymentTermsType
      translatedName
    }
    filtered: paymentTermsTemplates(paymentTermsType: $type) {
      id
      name
      description
      dueInDays
      paymentTermsType
      translatedName
    }
  }
`;

const draftOrderCreateDocument = `#graphql
  mutation PaymentTermsValidationDraftCreate($input: DraftOrderInput!) {
    draftOrderCreate(input: $input) {
      draftOrder {
        id
        name
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const paymentTermsCreateDocument = `#graphql
  mutation PaymentTermsValidationCreate($referenceId: ID!, $attrs: PaymentTermsCreateInput!) {
    paymentTermsCreate(referenceId: $referenceId, paymentTermsAttributes: $attrs) {
      ${paymentTermsSelection}
    }
  }
`;

const paymentTermsDeleteDocument = `#graphql
  mutation PaymentTermsValidationTermsCleanup($input: PaymentTermsDeleteInput!) {
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
  mutation PaymentTermsValidationDraftCleanup($input: DraftOrderDeleteInput!) {
    draftOrderDelete(input: $input) {
      deletedId
      userErrors {
        field
        message
      }
    }
  }
`;

type PaymentTermsCase = {
  attrs: Record<string, unknown>;
  purpose: string;
};

const cases: Record<string, PaymentTermsCase> = {
  unknownTemplate: {
    purpose: 'Unknown supplied paymentTermsTemplateId is rejected instead of defaulting to the first template.',
    attrs: {
      paymentTermsTemplateId: 'gid://shopify/PaymentTermsTemplate/9999',
      paymentSchedules: [{ issuedAt: '2026-01-01T00:00:00Z' }],
    },
  },
  fixedMissingDueAt: {
    purpose: 'FIXED template requires paymentSchedules[0].dueAt.',
    attrs: {
      paymentTermsTemplateId: 'gid://shopify/PaymentTermsTemplate/7',
      paymentSchedules: [{}],
    },
  },
  receiptExtraIssuedAt: {
    purpose: 'RECEIPT template accepts supplied issuedAt and returns no schedule nodes.',
    attrs: {
      paymentTermsTemplateId: 'gid://shopify/PaymentTermsTemplate/1',
      paymentSchedules: [{ issuedAt: '2026-01-01T00:00:00Z' }],
    },
  },
  receiptExtraDueAt: {
    purpose: 'RECEIPT template rejects supplied dueAt.',
    attrs: {
      paymentTermsTemplateId: 'gid://shopify/PaymentTermsTemplate/1',
      paymentSchedules: [{ dueAt: '2026-01-01T00:00:00Z' }],
    },
  },
  happyPath: {
    purpose: 'Sanity success path; primary lifecycle parity reuses the existing payment-terms lifecycle cassette.',
    attrs: {
      paymentTermsTemplateId: 'gid://shopify/PaymentTermsTemplate/4',
      paymentSchedules: [{ issuedAt: '2026-01-01T00:00:00Z' }],
    },
  },
};

async function captureCreateCase(name: string, spec: PaymentTermsCase, runId: number) {
  let draftOrderId: string | null = null;
  let paymentTermsId: string | null = null;
  const cleanup: Record<string, unknown> = {};

  const draftOrderVariables = {
    input: {
      email: `har674-payment-terms-${name}-${runId}@example.com`,
      lineItems: [
        {
          title: `HAR-674 payment terms ${name}`,
          quantity: 1,
          originalUnitPrice: '18.50',
        },
      ],
    },
  };

  try {
    const draftOrderCreate = await runGraphqlRequest(draftOrderCreateDocument, draftOrderVariables);
    assertNoTopLevelErrors(draftOrderCreate, `${name} draftOrderCreate setup`);
    const draftOrderCreateData = readRecord(draftOrderCreate.payload.data);
    const draftOrderCreatePayload = readRecord(draftOrderCreateData?.['draftOrderCreate']);
    const draftOrder = readRecord(draftOrderCreatePayload?.['draftOrder']);
    draftOrderId = typeof draftOrder?.['id'] === 'string' ? draftOrder['id'] : null;
    if (!draftOrderId) {
      throw new Error(`${name} draftOrderCreate did not return an id: ${JSON.stringify(draftOrderCreate, null, 2)}`);
    }

    const variables = { referenceId: draftOrderId, attrs: spec.attrs };
    const response = await runGraphqlRequest(paymentTermsCreateDocument, variables);
    assertNoTopLevelErrors(response, `${name} paymentTermsCreate`);
    const responseData = readRecord(response.payload.data);
    const payload = readRecord(responseData?.['paymentTermsCreate']);
    const paymentTerms = readRecord(payload?.['paymentTerms']);
    paymentTermsId = typeof paymentTerms?.['id'] === 'string' ? paymentTerms['id'] : null;

    if (paymentTermsId) {
      const paymentTermsDelete = await runGraphqlRequest(paymentTermsDeleteDocument, {
        input: { paymentTermsId },
      });
      cleanup['paymentTermsDelete'] = paymentTermsDelete.payload;
      assertNoTopLevelErrors(paymentTermsDelete, `${name} paymentTermsDelete cleanup`);
      paymentTermsId = null;
    }

    const draftOrderDelete = await runGraphqlRequest(draftOrderDeleteDocument, { input: { id: draftOrderId } });
    cleanup['draftOrderDelete'] = draftOrderDelete.payload;
    assertNoTopLevelErrors(draftOrderDelete, `${name} draftOrderDelete cleanup`);
    draftOrderId = null;

    return {
      purpose: spec.purpose,
      setup: {
        draftOrderCreate: {
          query: draftOrderCreateDocument,
          variables: draftOrderVariables,
          response: draftOrderCreate.payload,
        },
      },
      query: paymentTermsCreateDocument,
      variables,
      response: response.payload,
      cleanup,
    };
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
}

await mkdir(outputDir, { recursive: true });

const templatesVariables = { type: 'NET' };
const templates = await runGraphqlRequest(paymentTermsTemplatesDocument, templatesVariables);
assertNoTopLevelErrors(templates, 'paymentTermsTemplates lookup');

const runId = Date.now();
const capturedCases: Record<string, unknown> = {};
for (const [name, spec] of Object.entries(cases)) {
  capturedCases[name] = await captureCreateCase(name, spec, runId);
}

const fixture = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  upstreamCalls: [],
  templates: {
    query: paymentTermsTemplatesDocument,
    variables: templatesVariables,
    response: templates.payload,
  },
  cases: capturedCases,
  notes:
    'Captured on disposable draft orders. Validation cases assert Shopify paymentTermsCreate userErrors for unknown template ids and template/schedule mismatches; the happy path is retained as a sanity capture while executable lifecycle parity continues to cover success behavior.',
};

await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
console.log(`Wrote ${outputPath}`);

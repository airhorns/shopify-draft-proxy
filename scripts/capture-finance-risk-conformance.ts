import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type RecordedGraphqlRequest = {
  query: string;
  variables?: Record<string, unknown>;
  status: number;
  payload: unknown;
};

const ROOT_INTROSPECTION_QUERY = `#graphql
  query FinanceRiskRootIntrospection {
    queryRoot: __type(name: "QueryRoot") {
      fields {
        name
        args {
          name
          type {
            kind
            name
            ofType {
              kind
              name
              ofType {
                kind
                name
              }
            }
          }
        }
        type {
          kind
          name
          ofType {
            kind
            name
            ofType {
              kind
              name
            }
          }
        }
      }
    }
    mutationRoot: __type(name: "Mutation") {
      fields {
        name
        args {
          name
          type {
            kind
            name
            ofType {
              kind
              name
              ofType {
                kind
                name
              }
            }
          }
        }
        type {
          kind
          name
          ofType {
            kind
            name
            ofType {
              kind
              name
            }
          }
        }
      }
    }
  }
`;

const FINANCE_APP_ACCESS_POLICY_QUERY = `#graphql
  query FinanceAppAccessPolicyProbe {
    financeAppAccessPolicy {
      access
    }
  }
`;

const FINANCE_KYC_INFORMATION_QUERY = `#graphql
  query FinanceKycInformationAccessProbe {
    financeKycInformation {
      __typename
    }
  }
`;

const CASH_TRACKING_QUERY = `#graphql
  query CashTrackingSafeRead($id: ID!, $first: Int!) {
    cashTrackingSession(id: $id) {
      __typename
    }
    cashTrackingSessions(first: $first) {
      nodes {
        __typename
      }
      edges {
        node {
          __typename
        }
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
      }
    }
  }
`;

const POINT_OF_SALE_DEVICE_QUERY = `#graphql
  query PointOfSaleDeviceSafeRead($id: ID!) {
    pointOfSaleDevice(id: $id) {
      __typename
    }
  }
`;

const DISPUTE_SAFE_READ_QUERY = `#graphql
  query DisputeSafeRead($disputeId: ID!, $evidenceId: ID!, $first: Int!) {
    dispute(id: $disputeId) {
      __typename
    }
    disputeEvidence(id: $evidenceId) {
      __typename
    }
    disputes(first: $first) {
      nodes {
        __typename
      }
      edges {
        node {
          __typename
        }
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
      }
    }
  }
`;

const SHOP_PAY_RECEIPT_QUERY = `#graphql
  query ShopPayPaymentRequestReceiptSafeRead($token: String!, $first: Int!) {
    shopPayPaymentRequestReceipt(token: $token) {
      __typename
    }
    shopPayPaymentRequestReceipts(first: $first) {
      nodes {
        __typename
      }
      edges {
        node {
          __typename
        }
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
      }
    }
  }
`;

const TENDER_TRANSACTIONS_QUERY = `#graphql
  query TenderTransactionsSafeRead($first: Int!) {
    tenderTransactions(first: $first) {
      nodes {
        __typename
      }
      edges {
        node {
          __typename
        }
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
      }
    }
  }
`;

const DISPUTE_EVIDENCE_UPDATE_UNKNOWN_ID_MUTATION = `#graphql
  mutation DisputeEvidenceUpdateUnknownId($id: ID!, $input: ShopifyPaymentsDisputeEvidenceUpdateInput!) {
    disputeEvidenceUpdate(id: $id, input: $input) {
      disputeEvidence {
        __typename
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const ORDER_RISK_ASSESSMENT_UNKNOWN_ORDER_MUTATION = `#graphql
  mutation OrderRiskAssessmentCreateUnknownOrder($input: OrderRiskAssessmentCreateInput!) {
    orderRiskAssessmentCreate(orderRiskAssessmentInput: $input) {
      orderRiskAssessment {
        __typename
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const PAYOUT_ALTERNATE_CURRENCY_MISSING_CURRENCY_MUTATION = `#graphql
  mutation ShopifyPaymentsPayoutAlternateCurrencyCreateMissingCurrency {
    shopifyPaymentsPayoutAlternateCurrencyCreate {
      success
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const FINANCE_RISK_ROOT_NAMES = new Set([
  'cashTrackingSession',
  'cashTrackingSessions',
  'financeAppAccessPolicy',
  'financeKycInformation',
  'pointOfSaleDevice',
  'dispute',
  'disputes',
  'disputeEvidence',
  'disputeEvidenceUpdate',
  'orderRiskAssessmentCreate',
  'shopifyPaymentsPayoutAlternateCurrencyCreate',
  'shopPayPaymentRequestReceipt',
  'shopPayPaymentRequestReceipts',
  'tenderTransactions',
]);

function pickRelevantRootFields(payload: unknown) {
  const data = (
    payload as {
      data?: {
        queryRoot?: { fields?: unknown[] };
        mutationRoot?: { fields?: unknown[] };
      };
    }
  ).data;
  const queryFields = Array.isArray(data?.queryRoot?.fields) ? data.queryRoot.fields : [];
  const mutationFields = Array.isArray(data?.mutationRoot?.fields) ? data.mutationRoot.fields : [];

  return {
    queryRoots: queryFields.filter((field) => FINANCE_RISK_ROOT_NAMES.has((field as { name?: string }).name ?? '')),
    mutationRoots: mutationFields.filter((field) =>
      FINANCE_RISK_ROOT_NAMES.has((field as { name?: string }).name ?? ''),
    ),
  };
}

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function record(query: string, variables: Record<string, unknown> = {}): Promise<RecordedGraphqlRequest> {
  const { status, payload } = await runGraphqlRequest(query, variables);
  return {
    query,
    ...(Object.keys(variables).length > 0 ? { variables } : {}),
    status,
    payload,
  };
}

const rootIntrospection = await record(ROOT_INTROSPECTION_QUERY);
const financeAppAccessPolicy = await record(FINANCE_APP_ACCESS_POLICY_QUERY);
const financeKycInformation = await record(FINANCE_KYC_INFORMATION_QUERY);
const cashTracking = await record(CASH_TRACKING_QUERY, {
  id: 'gid://shopify/CashTrackingSession/0',
  first: 1,
});
const pointOfSaleDevice = await record(POINT_OF_SALE_DEVICE_QUERY, {
  id: 'gid://shopify/PointOfSaleDevice/0',
});
const disputes = await record(DISPUTE_SAFE_READ_QUERY, {
  disputeId: 'gid://shopify/ShopifyPaymentsDispute/0',
  evidenceId: 'gid://shopify/ShopifyPaymentsDisputeEvidence/0',
  first: 1,
});
const shopPayPaymentRequestReceipts = await record(SHOP_PAY_RECEIPT_QUERY, {
  token: 'codex-missing-shop-pay-payment-request-receipt-token',
  first: 1,
});
const tenderTransactions = await record(TENDER_TRANSACTIONS_QUERY, { first: 1 });
const disputeEvidenceUpdateUnknownId = await record(DISPUTE_EVIDENCE_UPDATE_UNKNOWN_ID_MUTATION, {
  id: 'gid://shopify/ShopifyPaymentsDisputeEvidence/0',
  input: {},
});
const orderRiskAssessmentCreateUnknownOrder = await record(ORDER_RISK_ASSESSMENT_UNKNOWN_ORDER_MUTATION, {
  input: {
    orderId: 'gid://shopify/Order/0',
    riskLevel: 'HIGH',
    facts: [],
  },
});
const payoutAlternateCurrencyMissingCurrency = await record(PAYOUT_ALTERNATE_CURRENCY_MISSING_CURRENCY_MUTATION);

const fixture = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  notes: [
    'HAR-316 finance/risk/POS safe capture. No real financial, dispute, POS cash, Shop Pay receipt, tender transaction, payout, or order-risk records are created.',
    'Read probes use unknown ids/tokens or type-only connection nodes to avoid inventing or exposing sensitive records. Non-empty future captures must use disposable stores and scrub/limit sensitive payment and KYC details.',
    'The payout alternate-currency mutation capture intentionally omits the required currency argument, so Shopify returns GraphQL validation without executing a money-movement resolver.',
  ],
  rootIntrospection: {
    query: rootIntrospection.query,
    status: rootIntrospection.status,
    errors:
      (rootIntrospection.payload as { errors?: unknown }).errors === undefined
        ? null
        : (rootIntrospection.payload as { errors?: unknown }).errors,
    relevantRoots: pickRelevantRootFields(rootIntrospection.payload),
  },
  safeReads: {
    financeAppAccessPolicy,
    financeKycInformation,
    cashTracking,
    pointOfSaleDevice,
    disputes,
    shopPayPaymentRequestReceipts,
    tenderTransactions,
  },
  safeMutationProbes: {
    disputeEvidenceUpdateUnknownId,
    orderRiskAssessmentCreateUnknownOrder,
    payoutAlternateCurrencyMissingCurrency,
  },
};

const outputPath = path.join(
  process.cwd(),
  'fixtures',
  'conformance',
  storeDomain,
  apiVersion,
  'finance-risk-access-read.json',
);

await mkdir(path.dirname(outputPath), { recursive: true });
await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');

// oxlint-disable-next-line no-console -- CLI capture scripts intentionally write the generated fixture path.
console.log(outputPath);

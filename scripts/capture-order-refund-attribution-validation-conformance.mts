/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type GraphqlResult = {
  status: number;
  payload: JsonRecord;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
}) as {
  runGraphqlRequest: (query: string, variables?: Record<string, unknown>) => Promise<GraphqlResult>;
};

const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'orders');
const fixturePath = path.join(fixtureDir, 'refund-create-attribution-validation.json');

const schemaProbeQuery = `#graphql
  query RefundCreateAttributionSchemaProbe {
    refundInput: __type(name: "RefundInput") {
      inputFields {
        name
      }
    }
    userError: __type(name: "UserError") {
      fields {
        name
      }
    }
  }
`;

const orderCreateMutation = `#graphql
  mutation RefundAttributionValidationOrderCreate($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
    orderCreate(order: $order, options: $options) {
      order {
        id
        name
        displayFinancialStatus
        displayFulfillmentStatus
        totalPriceSet { shopMoney { amount currencyCode } }
        totalReceivedSet { shopMoney { amount currencyCode } }
        totalRefundedSet { shopMoney { amount currencyCode } }
        lineItems(first: 5) {
          nodes {
            id
            title
            quantity
            currentQuantity
            originalUnitPriceSet { shopMoney { amount currencyCode } }
          }
        }
        transactions {
          id
          kind
          status
          gateway
          amountSet { shopMoney { amount currencyCode } }
        }
        refunds {
          id
          note
          totalRefundedSet { shopMoney { amount currencyCode } }
        }
        returns(first: 5) {
          nodes { id status }
          pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
        }
      }
      userErrors { field message }
    }
  }
`;

const refundCreateAttributionValidationMutation = `#graphql
mutation RefundCreateAttributionValidation($input: RefundInput!) {
  refundCreate(input: $input) {
    refund {
      id
    }
    order {
      id
    }
    userErrors {
      field
      message
    }
  }
}
`;

const orderReadAfterRefundAttributionRejection = `#graphql
  query OrderRefundReadParity($id: ID!) {
    order(id: $id) {
      id
      name
      displayFinancialStatus
      displayFulfillmentStatus
      refunds {
        id
        note
        totalRefundedSet {
          shopMoney {
            amount
            currencyCode
          }
        }
      }
      returns(first: 5) {
        nodes {
          id
          status
        }
        pageInfo {
          hasNextPage
          hasPreviousPage
          startCursor
          endCursor
        }
      }
      transactions {
        id
        kind
        status
        gateway
        amountSet {
          shopMoney {
            amount
            currencyCode
          }
        }
      }
      totalRefundedSet {
        shopMoney {
          amount
          currencyCode
        }
      }
    }
  }
`;

const orderCancelMutation = `#graphql
  mutation RefundAttributionValidationOrderCancel(
    $orderId: ID!
    $reason: OrderCancelReason!
    $notifyCustomer: Boolean!
    $restock: Boolean!
  ) {
    orderCancel(orderId: $orderId, reason: $reason, notifyCustomer: $notifyCustomer, restock: $restock) {
      job { id done }
      orderCancelUserErrors { field message code }
      userErrors { field message }
    }
  }
`;

function asRecord(value: unknown): JsonRecord {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonRecord) : {};
}

function readPath(value: unknown, pathParts: string[]): unknown {
  return pathParts.reduce<unknown>((current, key) => asRecord(current)[key], value);
}

function requireString(value: unknown, label: string): string {
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`Missing required capture value: ${label}`);
  }
  return value;
}

function assertHttpOk(label: string, result: GraphqlResult): void {
  if (result.status < 200 || result.status >= 300) {
    throw new Error(`${label} returned HTTP ${result.status}: ${JSON.stringify(result.payload, null, 2)}`);
  }
}

function assertNoGraphqlErrors(label: string, result: GraphqlResult): void {
  assertHttpOk(label, result);
  if (Array.isArray(result.payload['errors']) && result.payload['errors'].length > 0) {
    throw new Error(`${label} returned GraphQL errors: ${JSON.stringify(result.payload['errors'], null, 2)}`);
  }
}

function assertInvalidVariableAttributionErrors(result: GraphqlResult): void {
  assertHttpOk('refundCreate invalid attribution variable probe', result);
  const errors = Array.isArray(result.payload['errors']) ? result.payload['errors'] : [];
  const first = asRecord(errors[0]);
  const extensions = asRecord(first['extensions']);
  const problems = Array.isArray(extensions['problems']) ? extensions['problems'] : [];
  const problemPaths = problems.map((problem) => JSON.stringify(asRecord(problem)['path']));

  if (
    extensions['code'] !== 'INVALID_VARIABLE' ||
    !problemPaths.includes(JSON.stringify(['pointOfSaleDeviceId'])) ||
    !problemPaths.includes(JSON.stringify(['locationId'])) ||
    !problemPaths.includes(JSON.stringify(['userId'])) ||
    !problemPaths.includes(JSON.stringify(['transactionGroupId']))
  ) {
    throw new Error(`Expected INVALID_VARIABLE attribution problems, got ${JSON.stringify(result.payload, null, 2)}`);
  }
}

function assertInlineAttributionErrors(result: GraphqlResult): void {
  assertHttpOk('refundCreate invalid attribution inline probe', result);
  const errors = Array.isArray(result.payload['errors']) ? result.payload['errors'] : [];
  const argumentNames = errors.map((error) => asRecord(asRecord(error)['extensions'])['argumentName']);

  for (const name of ['pointOfSaleDeviceId', 'locationId', 'userId', 'transactionGroupId']) {
    if (!argumentNames.includes(name)) {
      throw new Error(`Expected inline attribution error for ${name}, got ${JSON.stringify(result.payload, null, 2)}`);
    }
  }
}

function gqlString(value: string): string {
  return JSON.stringify(value);
}

function inlineAttributionDocument(orderId: string): string {
  return `mutation RefundCreateAttributionInlineProbe {
  refundCreate(input: {
    orderId: ${gqlString(orderId)}
    pointOfSaleDeviceId: "9999999"
    locationId: "gid://shopify/Location/0"
    userId: 0
    transactionGroupId: "0"
  }) {
    refund {
      id
    }
    order {
      id
    }
    userErrors {
      field
      message
    }
  }
}`;
}

async function writeJson(filePath: string, payload: unknown): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${JSON.stringify(payload, null, 2)}\n`);
}

async function cleanupOrder(orderId: string): Promise<GraphqlResult> {
  return runGraphqlRequest(orderCancelMutation, {
    orderId,
    reason: 'OTHER',
    notifyCustomer: false,
    restock: false,
  });
}

const stamp = Date.now();
const schemaProbe = await runGraphqlRequest(schemaProbeQuery, {});
assertNoGraphqlErrors('schema probe', schemaProbe);

const orderVariables = {
  order: {
    email: `hermes-refund-attribution-${stamp}@example.com`,
    note: 'refundCreate attribution validation parity seed order',
    tags: ['parity-probe', 'refund-create', 'attribution-validation'],
    test: true,
    lineItems: [
      {
        title: `Hermes refundable attribution validation item ${stamp}`,
        quantity: 1,
        priceSet: {
          shopMoney: {
            amount: '10.00',
            currencyCode: 'CAD',
          },
        },
        requiresShipping: true,
        taxable: false,
        sku: `hermes-refund-attribution-${stamp}`,
      },
    ],
    transactions: [
      {
        kind: 'SALE',
        status: 'SUCCESS',
        gateway: 'manual',
        test: true,
        amountSet: {
          shopMoney: {
            amount: '10.00',
            currencyCode: 'CAD',
          },
        },
      },
    ],
  },
  options: null,
};

const orderCreate = await runGraphqlRequest(orderCreateMutation, orderVariables);
assertNoGraphqlErrors('orderCreate setup', orderCreate);
const order = readPath(orderCreate.payload, ['data', 'orderCreate', 'order']);
const orderId = requireString(readPath(order, ['id']), 'order.id');

const invalidVariables = {
  input: {
    orderId,
    pointOfSaleDeviceId: '9999999',
    locationId: 'gid://shopify/Location/0',
    userId: 0,
    transactionGroupId: '0',
  },
};
const invalidVariableResult = await runGraphqlRequest(refundCreateAttributionValidationMutation, invalidVariables);
assertInvalidVariableAttributionErrors(invalidVariableResult);

const inlineDocument = inlineAttributionDocument(orderId);
const invalidInlineResult = await runGraphqlRequest(inlineDocument, {});
assertInlineAttributionErrors(invalidInlineResult);

const downstreamReadVariables = { id: orderId };
const downstreamRead = await runGraphqlRequest(orderReadAfterRefundAttributionRejection, downstreamReadVariables);
assertNoGraphqlErrors('downstream order read after invalid refundCreate', downstreamRead);

let cleanup: GraphqlResult | { error: string };
try {
  cleanup = await cleanupOrder(orderId);
} catch (error) {
  cleanup = { error: error instanceof Error ? error.message : String(error) };
}

await writeJson(fixturePath, {
  storeDomain,
  apiVersion,
  schemaProbe: {
    query: schemaProbeQuery,
    response: schemaProbe.payload,
  },
  setup: {
    orderCreate: {
      query: orderCreateMutation,
      variables: orderVariables,
      response: orderCreate.payload,
    },
  },
  invalidVariable: {
    query: refundCreateAttributionValidationMutation,
    variables: invalidVariables,
    response: invalidVariableResult.payload,
  },
  invalidInline: {
    document: inlineDocument,
    variables: {},
    response: invalidInlineResult.payload,
  },
  downstreamRead: {
    query: orderReadAfterRefundAttributionRejection,
    variables: downstreamReadVariables,
    response: downstreamRead.payload,
  },
  cleanup: {
    orderCancel: cleanup,
  },
  upstreamCalls: [
    {
      operationName: 'OrderRefundReadParity',
      variables: downstreamReadVariables,
      query: 'captured downstream order read after refundCreate attribution validation rejection',
      response: {
        status: 200,
        body: downstreamRead.payload,
      },
    },
  ],
});

console.log(
  JSON.stringify(
    {
      ok: true,
      fixturePath,
      storeDomain,
      apiVersion,
      orderId,
      invalidVariableErrors: invalidVariableResult.payload['errors'] ?? null,
      invalidInlineErrors: invalidInlineResult.payload['errors'] ?? null,
      downstreamRefundCount: Array.isArray(readPath(downstreamRead.payload, ['data', 'order', 'refunds']))
        ? (readPath(downstreamRead.payload, ['data', 'order', 'refunds']) as unknown[]).length
        : null,
    },
    null,
    2,
  ),
);

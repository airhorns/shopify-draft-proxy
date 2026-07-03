/* oxlint-disable no-console -- Capture scripts intentionally write status output. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig, type ConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type CaptureClient = {
  config: ConformanceScriptConfig;
  runGraphqlRequest: (query: string, variables?: JsonRecord) => Promise<ConformanceGraphqlResult<JsonRecord>>;
};

const reminderRequestDocument = `mutation PaymentReminderSendAdditionalGuards($paymentScheduleId: ID!) {
  paymentReminderSend(paymentScheduleId: $paymentScheduleId) {
    success
    userErrors {
      field
      code
      message
    }
  }
}`;

const reminderHydrateDocument = `query PaymentScheduleReminderHydrate($id: ID!) {
  paymentSchedule: node(id: $id) {
    ... on PaymentSchedule {
      id
      dueAt
      issuedAt
      completedAt
      paymentTerms {
        id
        overdue
        dueInDays
        paymentTermsName
        paymentTermsType
        translatedName
        order {
          id
          email
          closed
          closedAt
          cancelledAt
          displayFinancialStatus
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
          status
          completedAt
        }
        paymentSchedules(first: 10) {
          nodes {
            id
            dueAt
            issuedAt
            completedAt
          }
        }
      }
    }
  }
}`;

const draftOrderCreateDocument = `mutation PaymentReminderGuardDraftOrderCreate($input: DraftOrderInput!) {
  draftOrderCreate(input: $input) {
    draftOrder {
      id
      name
      status
      email
    }
    userErrors {
      field
      message
    }
  }
}`;

const draftOrderCompleteDocument = `mutation PaymentReminderGuardDraftOrderComplete($id: ID!) {
  draftOrderComplete(id: $id, paymentPending: true) {
    draftOrder {
      id
      status
      order {
        id
        name
        email
        displayFinancialStatus
        cancelledAt
        closedAt
        closed
      }
    }
    userErrors {
      field
      message
    }
  }
}`;

const paymentTermsCreateDocument = `mutation PaymentReminderGuardTermsCreate(
  $referenceId: ID!
  $attrs: PaymentTermsCreateInput!
) {
  paymentTermsCreate(referenceId: $referenceId, paymentTermsAttributes: $attrs) {
    paymentTerms {
      id
      overdue
      dueInDays
      paymentTermsName
      paymentTermsType
      translatedName
      order {
        id
        email
        displayFinancialStatus
      }
      paymentSchedules(first: 10) {
        nodes {
          id
          dueAt
          issuedAt
          completedAt
        }
      }
    }
    userErrors {
      field
      message
      code
    }
  }
}`;

const orderCancelDocument = `mutation PaymentReminderGuardOrderCancel(
  $orderId: ID!
  $reason: OrderCancelReason!
  $notifyCustomer: Boolean!
  $restock: Boolean!
) {
  orderCancel(orderId: $orderId, reason: $reason, notifyCustomer: $notifyCustomer, restock: $restock) {
    job {
      id
    }
    userErrors {
      field
      message
    }
  }
}`;

function readRecord(value: unknown): JsonRecord {
  return value !== null && typeof value === 'object' && !Array.isArray(value) ? (value as JsonRecord) : {};
}

function readArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function readString(value: unknown): string | null {
  return typeof value === 'string' && value.length > 0 ? value : null;
}

function readPath(value: unknown, segments: string[]): unknown {
  let current = value;
  for (const segment of segments) {
    if (Array.isArray(current) && /^\d+$/u.test(segment)) {
      current = current[Number(segment)];
      continue;
    }
    const record = readRecord(current);
    if (!(segment in record)) return null;
    current = record[segment];
  }
  return current;
}

function payloadRoot(result: ConformanceGraphqlResult<JsonRecord>, root: string): JsonRecord {
  return readRecord(readPath(result.payload, ['data', root]));
}

function assertNoTopLevelErrors(result: ConformanceGraphqlResult<JsonRecord>, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertNoUserErrors(result: ConformanceGraphqlResult<JsonRecord>, root: string, context: string): void {
  assertNoTopLevelErrors(result, context);
  const errors = readArray(payloadRoot(result, root)['userErrors']);
  if (errors.length > 0) {
    throw new Error(`${context} returned userErrors: ${JSON.stringify(errors, null, 2)}`);
  }
}

async function clientFor(apiVersion: string): Promise<CaptureClient> {
  const config = readConformanceScriptConfig({
    defaultApiVersion: apiVersion,
    env: { ...process.env, SHOPIFY_CONFORMANCE_API_VERSION: apiVersion },
    exitOnMissing: true,
  });
  const accessToken = await getValidConformanceAccessToken({
    adminOrigin: config.adminOrigin,
    apiVersion: config.apiVersion,
  });
  const client = createAdminGraphqlClient({
    adminOrigin: config.adminOrigin,
    apiVersion: config.apiVersion,
    headers: buildAdminAuthHeaders(accessToken),
  });
  return { config, runGraphqlRequest: client.runGraphqlRequest };
}

function outputPath(config: ConformanceScriptConfig, domain: string, filename: string): string {
  return path.join('fixtures', 'conformance', config.storeDomain, config.apiVersion, domain, filename);
}

async function writeJson(filePath: string, value: unknown): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${JSON.stringify(value, null, 2)}\n`, 'utf8');
}

async function setupReminderOrder(
  client: CaptureClient,
  stamp: number,
  label: string,
  email: string | null,
): Promise<{ setup: JsonRecord; orderId: string; scheduleId: string }> {
  const draftOrderInput: JsonRecord = {
    note: `payment reminder ${label} additional guard capture`,
    tags: ['payment-reminder', 'additional-guard', label],
    lineItems: [
      {
        title: `Payment reminder ${label}`,
        quantity: 1,
        originalUnitPrice: '18.50',
      },
    ],
  };
  if (email !== null) draftOrderInput['email'] = email;

  const draftOrderCreateVariables = { input: draftOrderInput };
  const draftOrderCreate = await client.runGraphqlRequest(draftOrderCreateDocument, draftOrderCreateVariables);
  assertNoUserErrors(draftOrderCreate, 'draftOrderCreate', `${label} draftOrderCreate`);
  const draftOrderId = readString(readPath(draftOrderCreate.payload, ['data', 'draftOrderCreate', 'draftOrder', 'id']));
  if (!draftOrderId) throw new Error(`${label} draftOrderCreate did not return an id.`);

  const draftOrderCompleteVariables = { id: draftOrderId };
  const draftOrderComplete = await client.runGraphqlRequest(draftOrderCompleteDocument, draftOrderCompleteVariables);
  assertNoUserErrors(draftOrderComplete, 'draftOrderComplete', `${label} draftOrderComplete`);
  const orderId = readString(
    readPath(draftOrderComplete.payload, ['data', 'draftOrderComplete', 'draftOrder', 'order', 'id']),
  );
  if (!orderId) throw new Error(`${label} draftOrderComplete did not return an order id.`);

  const paymentTermsCreateVariables = {
    referenceId: orderId,
    attrs: {
      paymentTermsTemplateId: 'gid://shopify/PaymentTermsTemplate/4',
      paymentSchedules: [{ issuedAt: '2025-01-01T00:00:00Z' }],
    },
  };
  const paymentTermsCreate = await client.runGraphqlRequest(paymentTermsCreateDocument, paymentTermsCreateVariables);
  assertNoUserErrors(paymentTermsCreate, 'paymentTermsCreate', `${label} paymentTermsCreate`);
  const scheduleId = readString(
    readPath(paymentTermsCreate.payload, [
      'data',
      'paymentTermsCreate',
      'paymentTerms',
      'paymentSchedules',
      'nodes',
      '0',
      'id',
    ]),
  );
  if (!scheduleId) throw new Error(`${label} paymentTermsCreate did not return a payment schedule id.`);

  return {
    setup: {
      draftOrderCreate: {
        variables: draftOrderCreateVariables,
        response: draftOrderCreate.payload,
      },
      draftOrderComplete: {
        variables: draftOrderCompleteVariables,
        response: draftOrderComplete.payload,
      },
      paymentTermsCreate: {
        variables: paymentTermsCreateVariables,
        response: paymentTermsCreate.payload,
      },
    },
    orderId,
    scheduleId,
  };
}

async function captureReminderCase(client: CaptureClient, paymentScheduleId: string): Promise<JsonRecord> {
  const variables = { paymentScheduleId };
  const response = await client.runGraphqlRequest(reminderRequestDocument, variables);
  assertNoTopLevelErrors(response, `paymentReminderSend ${paymentScheduleId}`);
  return {
    request: { variables },
    response: response.payload,
  };
}

async function captureReminderHydrate(
  client: CaptureClient,
  paymentScheduleId: string,
  label: string,
): Promise<JsonRecord> {
  const response = await client.runGraphqlRequest(reminderHydrateDocument, { id: paymentScheduleId });
  assertNoTopLevelErrors(response, `payment schedule hydrate ${label}`);
  return {
    operationName: 'PaymentScheduleReminderHydrate',
    variables: { id: paymentScheduleId },
    query: reminderHydrateDocument,
    response: {
      status: response.status,
      body: response.payload,
    },
  };
}

async function cancelReminderOrder(client: CaptureClient, orderId: string): Promise<unknown> {
  const result = await client.runGraphqlRequest(orderCancelDocument, {
    orderId,
    reason: 'OTHER',
    notifyCustomer: false,
    restock: false,
  });
  return result.payload;
}

async function capturePaymentReminderSendAdditionalGuards(): Promise<string> {
  const client = await clientFor('2025-01');
  const stamp = Date.now();
  const orderIds: { missingEmail?: string; rateLimit?: string } = {};
  const cleanup: JsonRecord = {};
  const filePath = outputPath(client.config, 'payments', 'payment-reminder-send-additional-guards.json');
  let fixture: JsonRecord | null = null;

  try {
    const missingEmail = await setupReminderOrder(client, stamp, 'missing-email', null);
    orderIds.missingEmail = missingEmail.orderId;
    const rateLimit = await setupReminderOrder(
      client,
      stamp,
      'rate-limit',
      `payment-reminder-rate-limit-${stamp}@example.com`,
    );
    orderIds.rateLimit = rateLimit.orderId;

    const cases = {
      missingEmail: await captureReminderCase(client, missingEmail.scheduleId),
      rateFirst: await captureReminderCase(client, rateLimit.scheduleId),
      rateSecond: await captureReminderCase(client, rateLimit.scheduleId),
    };

    const upstreamCalls = [
      await captureReminderHydrate(client, missingEmail.scheduleId, 'missing contact email order schedule'),
      await captureReminderHydrate(client, rateLimit.scheduleId, 'rate-limit order schedule'),
    ];

    fixture = {
      capturedAt: new Date().toISOString(),
      storeDomain: client.config.storeDomain,
      apiVersion: client.config.apiVersion,
      scenarioId: 'payment-reminder-send-additional-guards',
      notes:
        'Live capture on disposable payment-terms orders for public Admin-reproducible paymentReminderSend guards: blank order email and one reminder per order per 24 hours. Selling-plan, capture-at-fulfillment, and unsent PaymentCollection branches depend on internal order/payment state not currently constructible through this public conformance harness; local runtime tests cover those guardrails with explicit order-side state hints.',
      requestDocument: reminderRequestDocument.replace(/\s+/gu, ' ').trim(),
      hydrateDocument: reminderHydrateDocument.replace(/\s+/gu, ' ').trim(),
      setup: {
        missingEmail: missingEmail.setup,
        rateLimit: rateLimit.setup,
      },
      cases,
      cleanup,
      upstreamCalls,
    };
  } finally {
    if (orderIds.missingEmail)
      cleanup['missingEmailOrderCancel'] = await cancelReminderOrder(client, orderIds.missingEmail);
    if (orderIds.rateLimit) cleanup['rateLimitOrderCancel'] = await cancelReminderOrder(client, orderIds.rateLimit);
  }

  if (!fixture) throw new Error('Payment reminder additional guard fixture was not captured.');
  await writeJson(filePath, fixture);
  return filePath;
}

capturePaymentReminderSendAdditionalGuards()
  .then((filePath) => {
    console.log(`Captured ${filePath}`);
  })
  .catch((error) => {
    console.error(error);
    process.exitCode = 1;
  });

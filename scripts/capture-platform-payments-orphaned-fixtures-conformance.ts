/* oxlint-disable no-console -- CLI capture scripts intentionally write status output. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig, type ConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;
type FixtureGroup = 'events' | 'payments' | 'apps' | 'bulk-operations' | 'functions';

type Capture = {
  query: string;
  variables: JsonRecord;
  response: ConformanceGraphqlResult<JsonRecord>;
};

type CaptureClient = {
  config: ConformanceScriptConfig;
  runGraphqlRequest: (query: string, variables?: JsonRecord) => Promise<ConformanceGraphqlResult<JsonRecord>>;
};

type FunctionNode = {
  id: string;
  title: string | null;
  handle: string | null;
  apiType: string | null;
  description: string | null;
  appKey: string | null;
  app: JsonRecord | null;
};

const allGroups: FixtureGroup[] = ['events', 'payments', 'apps', 'bulk-operations', 'functions'];
const requestedGroup = process.env['ORPHAN_FIXTURE_GROUP'];
const groupsToCapture =
  requestedGroup === undefined || requestedGroup === ''
    ? allGroups
    : allGroups.includes(requestedGroup as FixtureGroup)
      ? [requestedGroup as FixtureGroup]
      : null;

if (groupsToCapture === null) {
  throw new Error(`Unknown ORPHAN_FIXTURE_GROUP: ${requestedGroup}`);
}

const clientCache = new Map<string, CaptureClient>();

async function clientFor(apiVersion: string): Promise<CaptureClient> {
  const cached = clientCache.get(apiVersion);
  if (cached) return cached;

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
  const result = { config, runGraphqlRequest: client.runGraphqlRequest };
  clientCache.set(apiVersion, result);
  return result;
}

async function readText(filePath: string): Promise<string> {
  return readFile(filePath, 'utf8');
}

async function readJson<T>(filePath: string): Promise<T> {
  return JSON.parse(await readText(filePath)) as T;
}

async function writeJson(filePath: string, value: unknown): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${JSON.stringify(value, null, 2)}\n`, 'utf8');
}

function outputPath(config: ConformanceScriptConfig, domain: string, filename: string): string {
  return path.join('fixtures', 'conformance', config.storeDomain, config.apiVersion, domain, filename);
}

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

async function capture(client: CaptureClient, query: string, variables: JsonRecord = {}): Promise<Capture> {
  return {
    query,
    variables,
    response: await client.runGraphqlRequest(query, variables),
  };
}

function redactSecrets(value: unknown): unknown {
  return JSON.parse(
    JSON.stringify(value, (_key, entry) =>
      typeof entry === 'string' && /^shp[a-z]+_/u.test(entry) ? '[redacted-live-delegate-token]' : entry,
    ),
  ) as unknown;
}

async function captureEvents(): Promise<string[]> {
  const client = await clientFor('2025-01');
  const query = await readText('config/parity-requests/events/event-empty-read.graphql');
  const variables = await readJson<JsonRecord>('config/parity-requests/events/event-empty-read.variables.json');
  const result = await client.runGraphqlRequest(query, variables);
  assertNoTopLevelErrors(result, 'event empty read');

  const filePath = outputPath(client.config, 'events', 'event-empty-read.json');
  await writeJson(filePath, {
    variables,
    response: result.payload,
    upstreamCalls: [],
  });
  return [filePath];
}

async function capturePaymentReads(): Promise<string[]> {
  const client = await clientFor('2025-01');
  const written: string[] = [];

  const paymentCustomizationEmptyReadQuery = await readText(
    'config/parity-requests/payments/payment-customization-empty-read.graphql',
  );
  const paymentCustomizationEmptyReadVariables = {
    first: 2,
    query: 'enabled:true',
    unknownId: 'gid://shopify/PaymentCustomization/999999999999',
  };
  const paymentCustomizationEmptyRead = await client.runGraphqlRequest(
    paymentCustomizationEmptyReadQuery,
    paymentCustomizationEmptyReadVariables,
  );
  assertNoTopLevelErrors(paymentCustomizationEmptyRead, 'payment customization empty read');
  written.push(outputPath(client.config, 'payments', 'payment-customization-empty-read.json'));
  await writeJson(written.at(-1) as string, {
    variables: paymentCustomizationEmptyReadVariables,
    response: paymentCustomizationEmptyRead.payload,
    upstreamCalls: [],
  });

  const paymentTermsTemplatesQuery = await readText(
    'config/parity-requests/payments/payment-terms-templates-read.graphql',
  );
  const paymentTermsTemplatesVariables = { type: 'NET' };
  const paymentTermsTemplates = await client.runGraphqlRequest(
    paymentTermsTemplatesQuery,
    paymentTermsTemplatesVariables,
  );
  assertNoTopLevelErrors(paymentTermsTemplates, 'payment terms templates read');
  written.push(outputPath(client.config, 'payments', 'payment-terms-templates-read.json'));
  await writeJson(written.at(-1) as string, {
    variables: paymentTermsTemplatesVariables,
    response: paymentTermsTemplates.payload,
    upstreamCalls: [],
  });

  const shopifyPaymentsAccountQuery = await readText(
    'config/parity-requests/payments/shopify-payments-account-read.graphql',
  );
  const shopifyPaymentsAccount = await client.runGraphqlRequest(shopifyPaymentsAccountQuery, {});
  if (shopifyPaymentsAccount.status < 200 || shopifyPaymentsAccount.status >= 300) {
    throw new Error(`shopifyPaymentsAccount access probe failed: ${JSON.stringify(shopifyPaymentsAccount, null, 2)}`);
  }
  written.push(outputPath(client.config, 'payments', 'shopify-payments-account-access-denied.json'));
  await writeJson(written.at(-1) as string, {
    storeDomain: client.config.storeDomain,
    apiVersion: client.config.apiVersion,
    capturedAt: new Date().toISOString(),
    request: { query: shopifyPaymentsAccountQuery.replace(/\s+/gu, ' ').trim() },
    status: shopifyPaymentsAccount.status,
    errors: shopifyPaymentsAccount.payload.errors,
    data: shopifyPaymentsAccount.payload.data,
    extensions: shopifyPaymentsAccount.payload.extensions,
    upstreamCalls: [],
  });

  return written;
}

async function capturePaymentCustomizationValidation(): Promise<string> {
  const client = await clientFor('2025-01');
  const query = await readText('config/parity-requests/payments/payment-customization-validation.graphql');
  const functionCatalogQuery = `query PaymentCustomizationValidationFunctionCatalog {
    shopifyFunctions(first: 50) {
      nodes {
        id
        title
        apiType
        description
      }
    }
  }`;
  const functionCatalog = await client.runGraphqlRequest(functionCatalogQuery, {});
  assertNoTopLevelErrors(functionCatalog, 'payment customization function catalog');
  const variables = {
    badCreate: {
      title: 'Invalid payment customization',
      enabled: true,
      functionId: 'gid://shopify/ShopifyFunction/0',
    },
    missingFunction: {
      title: 'Missing function',
      enabled: true,
    },
    missingTitle: {
      enabled: true,
      functionId: 'gid://shopify/ShopifyFunction/0',
    },
    missingEnabled: {
      title: 'Missing enabled',
      functionId: 'gid://shopify/ShopifyFunction/0',
    },
    unknownId: 'gid://shopify/PaymentCustomization/0',
    badUpdate: {
      title: 'Unknown update',
      enabled: false,
      functionId: 'gid://shopify/ShopifyFunction/0',
    },
    activationIds: ['gid://shopify/PaymentCustomization/0'],
    emptyActivationIds: [],
    enabled: true,
  };
  const response = await client.runGraphqlRequest(query, variables);
  assertNoTopLevelErrors(response, 'payment customization validation');

  const filePath = outputPath(client.config, 'payments', 'payment-customization-validation.json');
  await writeJson(filePath, {
    capturedAt: new Date().toISOString(),
    storeDomain: client.config.storeDomain,
    apiVersion: client.config.apiVersion,
    functionCatalog: readPath(functionCatalog.payload, ['data', 'shopifyFunctions']),
    variables,
    response: {
      status: response.status,
      payload: response.payload,
    },
    upstreamCalls: [],
  });
  return filePath;
}

const reminderRequestDocument = `mutation PaymentReminderSendEligibility($paymentScheduleId: ID!) {
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
          closed
          closedAt
          cancelledAt
          displayFinancialStatus
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

const draftOrderCreateDocument = `mutation PaymentReminderDraftOrderCreate($input: DraftOrderInput!) {
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

const draftOrderCompleteDocument = `mutation PaymentReminderDraftOrderComplete($id: ID!) {
  draftOrderComplete(id: $id, paymentPending: true) {
    draftOrder {
      id
      status
      order {
        id
        name
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

const paymentTermsCreateDocument = `mutation PaymentReminderTermsCreate($referenceId: ID!, $attrs: PaymentTermsCreateInput!) {
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
        displayFinancialStatus
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
    userErrors {
      field
      message
      code
    }
  }
}`;

const orderMarkAsPaidDocument = `mutation PaymentReminderOrderMarkAsPaid($input: OrderMarkAsPaidInput!) {
  orderMarkAsPaid(input: $input) {
    order {
      id
      displayFinancialStatus
    }
    userErrors {
      field
      message
    }
  }
}`;

const orderCancelDocument = `mutation PaymentReminderOrderCancel(
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

async function setupReminderOrder(
  client: CaptureClient,
  stamp: number,
  label: string,
): Promise<{ setup: JsonRecord; orderId: string; scheduleId: string }> {
  const draftOrderCreateVariables = {
    input: {
      email: `payment-reminder-${label}-${stamp}@example.com`,
      note: `payment reminder ${label} capture`,
      tags: ['payment-reminder', label],
      lineItems: [
        {
          title: `Payment reminder ${label}`,
          quantity: 1,
          originalUnitPrice: '18.50',
        },
      ],
    },
  };
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
  const nodes = readArray(
    readPath(paymentTermsCreate.payload, ['data', 'paymentTermsCreate', 'paymentTerms', 'paymentSchedules', 'nodes']),
  );
  const firstScheduleId = readString(readRecord(nodes[0])['id']);
  if (!firstScheduleId) throw new Error(`${label} paymentTermsCreate did not return a payment schedule id.`);

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
    scheduleId: scheduleId ?? firstScheduleId,
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
    query: `captured live hydrate for ${label}`,
    response: {
      status: response.status,
      body: response.payload,
    },
  };
}

async function capturePaymentReminderSend(): Promise<string> {
  const client = await clientFor('2025-01');
  const stamp = Date.now();
  const orderIds: { success?: string; paid?: string } = {};
  const cleanup: JsonRecord = {};
  const filePath = outputPath(client.config, 'payments', 'payment-reminder-send-eligibility.json');
  let fixture: JsonRecord | null = null;

  try {
    const success = await setupReminderOrder(client, stamp, 'success');
    orderIds.success = success.orderId;
    const paid = await setupReminderOrder(client, stamp, 'paid');
    orderIds.paid = paid.orderId;

    const markPaidVariables = { input: { id: paid.orderId } };
    const orderMarkAsPaid = await client.runGraphqlRequest(orderMarkAsPaidDocument, markPaidVariables);
    assertNoUserErrors(orderMarkAsPaid, 'orderMarkAsPaid', 'paid orderMarkAsPaid');
    paid.setup['orderMarkAsPaid'] = {
      variables: markPaidVariables,
      response: orderMarkAsPaid.payload,
    };

    const unknownScheduleId = 'gid://shopify/PaymentSchedule/9999999999';
    const cases = {
      success: await captureReminderCase(client, success.scheduleId),
      unknown: await captureReminderCase(client, unknownScheduleId),
      paid: await captureReminderCase(client, paid.scheduleId),
    };

    const upstreamCalls = [
      await captureReminderHydrate(client, success.scheduleId, 'eligible overdue order schedule'),
      await captureReminderHydrate(client, unknownScheduleId, 'unknown schedule'),
      await captureReminderHydrate(client, paid.scheduleId, 'paid order schedule'),
    ];

    fixture = {
      capturedAt: new Date().toISOString(),
      storeDomain: client.config.storeDomain,
      apiVersion: client.config.apiVersion,
      scenarioId: 'payment-reminder-send-eligibility',
      notes:
        'Live capture on disposable payment-terms orders. Success uses an overdue payment-pending order schedule; unknown and paid branches confirm Shopify userErrors for ineligible schedules.',
      requestDocument: reminderRequestDocument.replace(/\s+/gu, ' ').trim(),
      hydrateDocument: reminderHydrateDocument.replace(/\s+/gu, ' ').trim(),
      setup: {
        success: success.setup,
        paid: paid.setup,
      },
      cases,
      cleanup,
      upstreamCalls,
    };
  } finally {
    if (orderIds.success) cleanup['successOrderCancel'] = await cancelReminderOrder(client, orderIds.success);
    if (orderIds.paid) cleanup['paidOrderCancel'] = await cancelReminderOrder(client, orderIds.paid);
  }

  if (!fixture) throw new Error('Payment reminder fixture was not captured.');
  await writeJson(filePath, fixture);
  return filePath;
}

async function capturePayments(): Promise<string[]> {
  const written = await capturePaymentReads();
  written.push(await capturePaymentCustomizationValidation());
  written.push(await capturePaymentReminderSend());
  return written;
}

async function captureDelegateAccessTokenCreateValidation(): Promise<string> {
  const client = await clientFor('2026-04');
  const emptyScopeQuery = await readText(
    'config/parity-requests/apps/delegateAccessTokenCreate-empty-scope-validation.graphql',
  );
  const negativeExpiresQuery = await readText(
    'config/parity-requests/apps/delegateAccessTokenCreate-negative-expires-validation.graphql',
  );
  const unknownScopeQuery = await readText(
    'config/parity-requests/apps/delegateAccessTokenCreate-unknown-scope-validation.graphql',
  );
  const happyPathQuery = await readText(
    'config/parity-requests/apps/delegateAccessTokenCreate-happy-validation.graphql',
  );
  const destroyQuery = await readText('config/parity-requests/apps/delegateAccessTokenDestroy-codes.graphql');

  const emptyScope = await client.runGraphqlRequest(emptyScopeQuery, {});
  const negativeExpires = await client.runGraphqlRequest(negativeExpiresQuery, {});
  const unknownScope = await client.runGraphqlRequest(unknownScopeQuery, {});
  const happyPath = await client.runGraphqlRequest(happyPathQuery, {});
  for (const [label, result] of Object.entries({ emptyScope, negativeExpires, unknownScope, happyPath })) {
    assertNoTopLevelErrors(result, `delegateAccessTokenCreate ${label}`);
  }

  const token = readString(
    readPath(happyPath.payload, ['data', 'delegateAccessTokenCreate', 'delegateAccessToken', 'accessToken']),
  );
  if (!token) throw new Error('delegateAccessTokenCreate happy path did not return a token.');
  const cleanup = await client.runGraphqlRequest(destroyQuery, { token });

  const filePath = outputPath(client.config, 'apps', 'delegate-access-token-create-validation.json');
  await writeJson(filePath, {
    capturedAt: new Date().toISOString(),
    storeDomain: client.config.storeDomain,
    apiVersion: client.config.apiVersion,
    scenario: 'delegate-access-token-create-validation',
    notes: [
      'Captured live Admin GraphQL delegateAccessTokenCreate validation with the conformance credential.',
      'The happy-path branch used a short expiry and the returned raw delegate token was destroyed immediately.',
      'The raw live delegate token is redacted in this fixture.',
    ],
    operationNames: ['delegateAccessTokenCreate'],
    upstreamCalls: [],
    evidence: {
      live: {
        cleanup: {
          status: cleanup.status,
          payload: cleanup.payload,
        },
      },
      parity: {
        expected: {
          emptyScope: readRecord(redactSecrets(emptyScope.payload)),
          negativeExpires: readRecord(redactSecrets(negativeExpires.payload)),
          unknownScope: readRecord(redactSecrets(unknownScope.payload)),
          happyPath: readRecord(redactSecrets(happyPath.payload)),
        },
      },
    },
  });
  return filePath;
}

async function captureDelegateAccessTokenCreateExpiresAfterParent(): Promise<string> {
  const client = await clientFor('2026-04');
  const query = await readText('config/parity-requests/apps/delegateAccessTokenCreate-expires-after-parent.graphql');
  const result = await client.runGraphqlRequest(query, {});
  assertNoTopLevelErrors(result, 'delegateAccessTokenCreate expires-after-parent');

  const token = readPath(result.payload, ['data', 'delegateAccessTokenCreate', 'delegateAccessToken']);
  const userErrors = readArray(readPath(result.payload, ['data', 'delegateAccessTokenCreate', 'userErrors']));
  const firstError = readRecord(userErrors[0]);
  if (
    token !== null ||
    userErrors.length !== 1 ||
    firstError['code'] !== 'EXPIRES_AFTER_PARENT' ||
    firstError['message'] !== "The delegate token can't expire after the parent token."
  ) {
    throw new Error(`Unexpected expires-after-parent response: ${JSON.stringify(result.payload, null, 2)}`);
  }

  const filePath = outputPath(client.config, 'apps', 'delegate-access-token-create-expires-after-parent.json');
  await writeJson(filePath, {
    capturedAt: new Date().toISOString(),
    storeDomain: client.config.storeDomain,
    apiVersion: client.config.apiVersion,
    scenario: 'delegate-access-token-create-expires-after-parent',
    notes: [
      'Captured live Admin GraphQL delegateAccessTokenCreate EXPIRES_AFTER_PARENT validation with the expiring conformance credential.',
      'The request used a very large expiresIn so the delegate would outlive the active parent token.',
      'Shopify returned field null for this public GraphQL userError path.',
    ],
    operationNames: ['delegateAccessTokenCreate'],
    upstreamCalls: [],
    evidence: {
      parity: {
        expected: {
          expiresAfterParent: readRecord(redactSecrets(result.payload)),
        },
      },
      localRuntime: {
        expected: {
          failedLog: {
            status: 'failed',
            stagedResourceIds: [],
          },
          emptyDelegatedAccessTokens: {},
        },
      },
    },
  });
  return filePath;
}

async function captureDelegateAccessTokenDestroyCodes(): Promise<string> {
  const parentClient = await clientFor('2026-04');
  const createQuery = await readText('config/parity-requests/apps/delegateAccessTokenCreate-happy-validation.graphql');
  const destroyQuery = await readText('config/parity-requests/apps/delegateAccessTokenDestroy-codes.graphql');
  const parentAccessToken = await getValidConformanceAccessToken({
    adminOrigin: parentClient.config.adminOrigin,
    apiVersion: parentClient.config.apiVersion,
  });

  const missing = await parentClient.runGraphqlRequest(destroyQuery, { token: 'shpat_does_not_exist' });
  assertNoTopLevelErrors(missing, 'delegateAccessTokenDestroy missing');

  const createForSuccess = await parentClient.runGraphqlRequest(createQuery, {});
  assertNoTopLevelErrors(createForSuccess, 'delegateAccessTokenCreate success setup');
  const successToken = readString(
    readPath(createForSuccess.payload, ['data', 'delegateAccessTokenCreate', 'delegateAccessToken', 'accessToken']),
  );
  if (!successToken) throw new Error('delegateAccessTokenCreate success setup did not return a token.');
  const success = await parentClient.runGraphqlRequest(destroyQuery, { token: successToken });
  const repeat = await parentClient.runGraphqlRequest(destroyQuery, { token: successToken });
  assertNoTopLevelErrors(success, 'delegateAccessTokenDestroy success');
  assertNoTopLevelErrors(repeat, 'delegateAccessTokenDestroy repeat');

  const parentSelf = await parentClient.runGraphqlRequest(destroyQuery, { token: parentAccessToken });
  assertNoTopLevelErrors(parentSelf, 'delegateAccessTokenDestroy parent self');

  let siblingTargetToken: string | null = null;
  let siblingCallerToken: string | null = null;
  try {
    const createSiblingTarget = await parentClient.runGraphqlRequest(createQuery, {});
    const createSiblingCaller = await parentClient.runGraphqlRequest(createQuery, {});
    assertNoTopLevelErrors(createSiblingTarget, 'delegateAccessTokenCreate sibling target');
    assertNoTopLevelErrors(createSiblingCaller, 'delegateAccessTokenCreate sibling caller');
    siblingTargetToken = readString(
      readPath(createSiblingTarget.payload, [
        'data',
        'delegateAccessTokenCreate',
        'delegateAccessToken',
        'accessToken',
      ]),
    );
    siblingCallerToken = readString(
      readPath(createSiblingCaller.payload, [
        'data',
        'delegateAccessTokenCreate',
        'delegateAccessToken',
        'accessToken',
      ]),
    );
    if (!siblingTargetToken || !siblingCallerToken) {
      throw new Error('delegateAccessTokenCreate sibling setup did not return both tokens.');
    }
    const siblingClient = createAdminGraphqlClient({
      adminOrigin: parentClient.config.adminOrigin,
      apiVersion: parentClient.config.apiVersion,
      headers: buildAdminAuthHeaders(siblingCallerToken),
    });
    const siblingHierarchy = await siblingClient.runGraphqlRequest<JsonRecord>(destroyQuery, {
      token: siblingTargetToken,
    });
    assertNoTopLevelErrors(siblingHierarchy, 'delegateAccessTokenDestroy sibling hierarchy');

    const filePath = outputPath(parentClient.config, 'apps', 'delegate-access-token-destroy-codes.json');
    await writeJson(filePath, {
      capturedAt: new Date().toISOString(),
      storeDomain: parentClient.config.storeDomain,
      apiVersion: parentClient.config.apiVersion,
      scenario: 'delegate-access-token-destroy-codes',
      notes: [
        'Live probes captured the public Admin GraphQL response shape for missing token, parent-token self destroy, sibling hierarchy denial, successful destroy, and repeat destroy.',
        'Raw parent and delegate tokens are intentionally omitted; parity replay creates synthetic delegate tokens locally and uses request headers to model caller token identity.',
      ],
      evidence: {
        live: {
          expected: {
            missing: readRecord(redactSecrets(missing.payload)),
            success: readRecord(redactSecrets(success.payload)),
            repeat: readRecord(redactSecrets(repeat.payload)),
            parentSelf: readRecord(redactSecrets(parentSelf.payload)),
            siblingHierarchy: readRecord(redactSecrets(siblingHierarchy.payload)),
          },
        },
        localRuntime: {
          expected: {
            delegateCreate: readRecord(redactSecrets(createForSuccess.payload)),
            crossApp: readRecord(redactSecrets(siblingHierarchy.payload)),
          },
        },
      },
      upstreamCalls: [],
    });
    return filePath;
  } finally {
    if (siblingTargetToken) await parentClient.runGraphqlRequest(destroyQuery, { token: siblingTargetToken });
    if (siblingCallerToken) await parentClient.runGraphqlRequest(destroyQuery, { token: siblingCallerToken });
  }
}

async function captureApps(): Promise<string[]> {
  return [
    await captureDelegateAccessTokenCreateValidation(),
    await captureDelegateAccessTokenCreateExpiresAfterParent(),
    await captureDelegateAccessTokenDestroyCodes(),
  ];
}

async function captureBulkOperations(): Promise<string[]> {
  const client = await clientFor('2026-04');
  const query = await readText('config/parity-requests/bulk-operations/bulk-operation-run-mutation-validators.graphql');
  const validations: Record<string, JsonRecord> = {};
  const cases = {
    parserError: {
      mutation: 'mutation { not parseable',
      path: 'valid',
    },
    queryInsteadOfMutation: {
      mutation: 'query { products { edges { node { id } } } }',
      path: 'valid',
    },
    multipleTopLevelMutations: {
      mutation:
        'mutation BulkProducts($product: ProductCreateInput!, $update: ProductUpdateInput!) { productCreate(product: $product) { product { id } } productUpdate(product: $update) { product { id } } }',
      path: 'valid',
    },
    disallowedMutationName: {
      mutation:
        'mutation Probe($mutation: String!, $stagedUploadPath: String!, $clientIdentifier: String) { bulkOperationRunMutation(mutation: $mutation, stagedUploadPath: $stagedUploadPath, clientIdentifier: $clientIdentifier) { bulkOperation { id } userErrors { field message } } }',
      path: 'valid',
    },
  };

  for (const [key, variables] of Object.entries(cases)) {
    const response = await client.runGraphqlRequest(query, variables);
    assertNoTopLevelErrors(response, `bulkOperationRunMutation ${key}`);
    validations[key] = {
      operationName: 'BulkOperationRunMutationValidators',
      query,
      variables,
      status: response.status,
      response: response.payload,
    };
  }

  const filePath = outputPath(client.config, 'bulk-operations', 'bulk-operation-run-mutation-validators.json');
  await writeJson(filePath, {
    capturedAt: new Date().toISOString(),
    storeDomain: client.config.storeDomain,
    apiVersion: client.config.apiVersion,
    request: {
      operationName: 'BulkOperationRunMutationValidators',
      query,
    },
    validations,
    upstreamCalls: [],
  });
  return [filePath];
}

function normalizeFunctionNode(node: FunctionNode): JsonRecord {
  return {
    id: node.id,
    title: node.title,
    handle: node.handle,
    apiType: node.apiType,
    description: node.description,
    appKey: node.appKey,
    app: node.app,
  };
}

function readFunctionNodes(captureResult: Capture): FunctionNode[] {
  return readArray(readPath(captureResult.response.payload, ['data', 'shopifyFunctions', 'nodes'])).map(
    (node) => readRecord(node) as FunctionNode,
  );
}

function requireFunction(nodes: FunctionNode[], handle: string, apiType: string): FunctionNode {
  const node = nodes.find((candidate) => candidate.handle === handle || candidate.apiType === apiType);
  if (!node?.id || !node.handle) {
    throw new Error(`Expected released Function ${handle} (${apiType}) in live shopifyFunctions response.`);
  }
  return node;
}

async function captureFunctions(): Promise<string[]> {
  const client = await clientFor('2026-04');
  const requestDir = path.join('config', 'parity-requests', 'functions');
  const setupQuery = await readText(path.join(requestDir, 'functions-cart-transform-create-validation-setup.graphql'));
  const conflictQuery = await readText(
    path.join(requestDir, 'functions-cart-transform-create-validation-conflict.graphql'),
  );
  const apiMismatchQuery = await readText(
    path.join(requestDir, 'functions-cart-transform-create-validation-api-mismatch.graphql'),
  );
  const bothQuery = await readText(path.join(requestDir, 'functions-cart-transform-create-validation-both.graphql'));
  const readQuery = await readText(path.join(requestDir, 'functions-cart-transform-create-validation-read.graphql'));
  const deleteQuery = `mutation DeleteCapturedCartTransform($id: ID!) {
    cartTransformDelete(id: $id) {
      deletedId
      userErrors {
        field
        message
        code
      }
    }
  }`;
  const functionReadQuery = `query ReadCartTransformValidationFunctions {
    shopifyFunctions(first: 50) {
      nodes {
        id
        title
        handle
        apiType
        description
        appKey
        app {
          __typename
          id
          title
          handle
          apiKey
        }
      }
    }
  }`;
  const functionHydrateQuery = `query FunctionHydrateById($id: String!) {
    shopifyFunction(id: $id) {
      id
      title
      handle
      apiType
      description
      appKey
      app {
        __typename
        id
        title
        handle
        apiKey
      }
    }
  }`;

  const functionRead = await capture(client, functionReadQuery, {});
  assertNoTopLevelErrors(functionRead.response, 'Function inventory read');
  const functions = readFunctionNodes(functionRead);
  const cartTransformFunction = requireFunction(functions, 'conformance-cart-transform', 'cart_transform');
  const validationFunction = requireFunction(functions, 'conformance-validation', 'cart_checkout_validation');

  const existingCartTransforms = await capture(client, readQuery, { first: 50 });
  assertNoTopLevelErrors(existingCartTransforms.response, 'Existing cartTransforms read');
  for (const node of readArray(
    readPath(existingCartTransforms.response.payload, ['data', 'cartTransforms', 'nodes']),
  )) {
    const id = readString(readRecord(node)['id']);
    if (id) await client.runGraphqlRequest(deleteQuery, { id });
  }

  let createdCartTransformId: string | null = null;
  try {
    const cartTransformCreateSetup = await capture(client, setupQuery, {
      functionId: cartTransformFunction.id,
      blockOnFailure: false,
    });
    assertNoUserErrors(cartTransformCreateSetup.response, 'cartTransformCreate', 'cartTransformCreate setup');
    createdCartTransformId = readString(
      readPath(cartTransformCreateSetup.response.payload, ['data', 'cartTransformCreate', 'cartTransform', 'id']),
    );

    const cartTransformCreateConflict = await capture(client, conflictQuery, {
      functionId: cartTransformFunction.id,
      blockOnFailure: false,
    });
    const cartTransformCreateApiMismatch = await capture(client, apiMismatchQuery, {
      functionId: validationFunction.id,
      blockOnFailure: false,
    });
    const cartTransformCreateBoth = await capture(client, bothQuery, {
      functionId: cartTransformFunction.id,
      functionHandle: cartTransformFunction.handle,
      blockOnFailure: false,
    });
    const cartTransformsAfterValidation = await capture(client, readQuery, { first: 5 });
    const cartTransformHydrate = await client.runGraphqlRequest(functionHydrateQuery, { id: cartTransformFunction.id });
    const validationHydrate = await client.runGraphqlRequest(functionHydrateQuery, { id: validationFunction.id });

    for (const [label, result] of Object.entries({
      cartTransformCreateConflict,
      cartTransformCreateApiMismatch,
      cartTransformCreateBoth,
      cartTransformsAfterValidation,
    })) {
      assertNoTopLevelErrors(result.response, label);
    }
    assertNoTopLevelErrors(cartTransformHydrate, 'cart transform Function hydrate');
    assertNoTopLevelErrors(validationHydrate, 'validation Function hydrate');

    const filePath = outputPath(client.config, 'functions', 'functions-cart-transform-create-validation.json');
    await writeJson(filePath, {
      scenarioId: 'functions-cart-transform-create-validation',
      capturedAt: new Date().toISOString(),
      source: 'live-shopify',
      storeDomain: client.config.storeDomain,
      apiVersion: client.config.apiVersion,
      summary:
        'Cart transform create Function identifier validation, duplicate registration, multiple identifiers, and downstream read evidence.',
      shopifyFunctions: {
        cartTransform: normalizeFunctionNode(cartTransformFunction),
        validation: normalizeFunctionNode(validationFunction),
      },
      cartTransformCreateSetup,
      cartTransformCreateConflict,
      cartTransformCreateApiMismatch,
      cartTransformCreateBoth,
      cartTransformsAfterValidation,
      captureNotes: [
        'The setup branch creates one disposable cart transform from the released cart-transform Function.',
        'The conflict branch reuses the same Function id and records duplicate-registration behavior.',
        'The both-identifiers branch sends both Function id and handle for the same released Function.',
      ],
      upstreamCalls: [
        {
          operationName: 'FunctionHydrateById',
          variables: { id: cartTransformFunction.id },
          query: functionHydrateQuery,
          response: {
            status: cartTransformHydrate.status,
            body: cartTransformHydrate.payload,
          },
        },
        {
          operationName: 'FunctionHydrateById',
          variables: { id: validationFunction.id },
          query: functionHydrateQuery,
          response: {
            status: validationHydrate.status,
            body: validationHydrate.payload,
          },
        },
      ],
    });
    return [filePath];
  } finally {
    if (createdCartTransformId) {
      await client.runGraphqlRequest(deleteQuery, { id: createdCartTransformId });
    }
  }
}

const written: string[] = [];
for (const group of groupsToCapture) {
  if (group === 'events') written.push(...(await captureEvents()));
  if (group === 'payments') written.push(...(await capturePayments()));
  if (group === 'apps') written.push(...(await captureApps()));
  if (group === 'bulk-operations') written.push(...(await captureBulkOperations()));
  if (group === 'functions') written.push(...(await captureFunctions()));
}

console.log(JSON.stringify({ ok: true, groups: groupsToCapture, written }, null, 2));

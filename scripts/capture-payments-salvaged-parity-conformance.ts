/* oxlint-disable no-console -- CLI capture scripts intentionally write status output. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig, type ConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type CaptureClient = {
  config: ConformanceScriptConfig;
  runGraphqlRequest: <T = JsonRecord>(query: string, variables?: JsonRecord) => Promise<ConformanceGraphqlResult<T>>;
};

type GraphqlCapture = {
  query: string;
  variables: JsonRecord;
  response: ConformanceGraphqlResult<JsonRecord>;
};

const clientCache = new Map<string, CaptureClient>();

async function clientFor(apiVersion: string): Promise<CaptureClient> {
  const cached = clientCache.get(apiVersion);
  if (cached) return cached;

  const config = readConformanceScriptConfig({
    defaultApiVersion: apiVersion,
    env: { ...process.env, SHOPIFY_CONFORMANCE_API_VERSION: apiVersion },
    exitOnMissing: true,
  });
  const token = await getValidConformanceAccessToken({
    adminOrigin: config.adminOrigin,
    apiVersion: config.apiVersion,
  });
  const client = createAdminGraphqlClient({
    adminOrigin: config.adminOrigin,
    apiVersion: config.apiVersion,
    headers: buildAdminAuthHeaders(token),
  });
  const result = { config, runGraphqlRequest: client.runGraphqlRequest };
  clientCache.set(apiVersion, result);
  return result;
}

async function readText(filePath: string): Promise<string> {
  return readFile(filePath, 'utf8');
}

async function writeJson(filePath: string, value: unknown): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${JSON.stringify(value, null, 2)}\n`, 'utf8');
}

function outputPath(config: ConformanceScriptConfig, filename: string): string {
  return path.join('fixtures', 'conformance', config.storeDomain, config.apiVersion, 'payments', filename);
}

function trimGraphql(query: string): string {
  return query.replace(/^#graphql\n/u, '').trim();
}

function readRecord(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function readString(value: unknown): string | null {
  return typeof value === 'string' && value.length > 0 ? value : null;
}

function payloadRoot(capture: GraphqlCapture, root: string): JsonRecord | null {
  return readRecord(readRecord(capture.response.payload['data'])?.[root]);
}

function userErrors(capture: GraphqlCapture, root: string): unknown[] {
  return readArray(payloadRoot(capture, root)?.['userErrors']);
}

function assertNoTopLevelErrors(capture: GraphqlCapture, context: string): void {
  if (capture.response.status < 200 || capture.response.status >= 300 || capture.response.payload['errors']) {
    throw new Error(`${context} failed: ${JSON.stringify(capture.response, null, 2)}`);
  }
}

function assertNoUserErrors(capture: GraphqlCapture, root: string, context: string): void {
  assertNoTopLevelErrors(capture, context);
  const errors = userErrors(capture, root);
  const cancelErrors = readArray(payloadRoot(capture, root)?.['orderCancelUserErrors']);
  if (errors.length > 0 || cancelErrors.length > 0) {
    throw new Error(`${context} returned userErrors: ${JSON.stringify({ errors, cancelErrors }, null, 2)}`);
  }
}

function requireId(value: unknown, context: string): string {
  const id = readString(value);
  if (!id) throw new Error(`${context} did not return an id: ${JSON.stringify(value, null, 2)}`);
  return id;
}

async function run(client: CaptureClient, query: string, variables: JsonRecord = {}): Promise<GraphqlCapture> {
  const cleanQuery = trimGraphql(query);
  return {
    query: cleanQuery,
    variables,
    response: await client.runGraphqlRequest<JsonRecord>(cleanQuery, variables),
  };
}

function capturePayload(capture: GraphqlCapture): JsonRecord {
  return {
    query: capture.query,
    variables: capture.variables,
    response: capture.response.payload,
  };
}

const orderCancelDocument = `#graphql
  mutation PaymentsSalvageOrderCancel(
    $orderId: ID!
    $reason: OrderCancelReason!
    $notifyCustomer: Boolean!
    $restock: Boolean!
  ) {
    orderCancel(orderId: $orderId, reason: $reason, notifyCustomer: $notifyCustomer, restock: $restock) {
      job {
        id
        done
      }
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

const draftOrderCreateDocument = `#graphql
  mutation PaymentsSalvageDraftOrderCreate($input: DraftOrderInput!) {
    draftOrderCreate(input: $input) {
      draftOrder {
        id
        name
        subtotalPriceSet {
          shopMoney {
            amount
            currencyCode
          }
          presentmentMoney {
            amount
            currencyCode
          }
        }
        totalPriceSet {
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
      }
    }
  }
`;

const draftOrderDeleteDocument = `#graphql
  mutation PaymentsSalvageDraftOrderDelete($input: DraftOrderDeleteInput!) {
    draftOrderDelete(input: $input) {
      deletedId
      userErrors {
        field
        message
      }
    }
  }
`;

const paymentTermsDraftHydrateDocument = `#graphql
  query PaymentTermsDraftHydrate($id: ID!) {
    draftOrder(id: $id) {
      id
      name
      paymentTerms {
        id
      }
      subtotalPriceSet {
        shopMoney { amount currencyCode }
        presentmentMoney { amount currencyCode }
      }
      totalPriceSet {
        shopMoney { amount currencyCode }
        presentmentMoney { amount currencyCode }
      }
    }
  }
`;

const paymentTermsOwnerHydrateDocument = `#graphql
  query PaymentTermsOwnerHydrate($id: ID!) {
    order(id: $id) {
      id
      displayFinancialStatus
      closed
      closedAt
      cancelledAt
      paymentTerms {
        id
      }
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
    }
  }
`;

function paymentTermsAttrs(): JsonRecord {
  return {
    paymentTermsTemplateId: 'gid://shopify/PaymentTermsTemplate/4',
    paymentSchedules: [{ issuedAt: '2026-05-05T00:00:00Z' }],
  };
}

function multiplePaymentTermsAttrs(): JsonRecord {
  return {
    paymentTermsTemplateId: 'gid://shopify/PaymentTermsTemplate/4',
    paymentSchedules: [{ issuedAt: '2026-05-05T00:00:00Z' }, { issuedAt: '2026-05-06T00:00:00Z' }],
  };
}

function orderCreateVariables(label: string, stamp: number): JsonRecord {
  const amount = '42.50';
  const priceSet = {
    shopMoney: { amount, currencyCode: 'USD' },
    presentmentMoney: { amount: '57.00', currencyCode: 'CAD' },
  };
  return {
    order: {
      email: `payments-salvage-${label}-${stamp}@example.com`,
      currency: 'USD',
      presentmentCurrency: 'CAD',
      test: true,
      lineItems: [
        {
          title: `Payments salvage ${label}`,
          quantity: 1,
          priceSet,
          requiresShipping: false,
          taxable: false,
          sku: `payments-salvage-${label}-${stamp}`,
        },
      ],
    },
  };
}

function draftOrderCreateVariables(label: string, stamp: number): JsonRecord {
  return {
    input: {
      email: `payments-salvage-draft-${label}-${stamp}@example.com`,
      note: `payments salvage draft ${label}`,
      tags: ['shopify-draft-proxy', 'payments-salvage', label],
      presentmentCurrencyCode: 'CAD',
      lineItems: [
        {
          title: `Payments salvage draft ${label}`,
          quantity: 1,
          originalUnitPriceWithCurrency: {
            amount: '18.50',
            currencyCode: 'CAD',
          },
          requiresShipping: false,
          taxable: false,
          sku: `payments-salvage-draft-${label}-${stamp}`,
        },
      ],
    },
  };
}

function paymentTermsIdFromCreate(capture: GraphqlCapture, context: string): string {
  assertNoUserErrors(capture, 'paymentTermsCreate', context);
  const terms = readRecord(payloadRoot(capture, 'paymentTermsCreate')?.['paymentTerms']);
  return requireId(terms?.['id'], context);
}

async function cancelOrder(client: CaptureClient, orderId: string): Promise<JsonRecord> {
  const capture = await run(client, orderCancelDocument, {
    orderId,
    reason: 'OTHER',
    notifyCustomer: false,
    restock: false,
  });
  return capturePayload(capture);
}

async function deleteDraftOrder(client: CaptureClient, draftOrderId: string): Promise<JsonRecord> {
  const capture = await run(client, draftOrderDeleteDocument, { input: { id: draftOrderId } });
  return capturePayload(capture);
}

function upstreamCall(operationName: string, capture: GraphqlCapture): JsonRecord {
  return {
    operationName,
    variables: capture.variables,
    query: capture.query,
    response: {
      status: capture.response.status,
      body: capture.response.payload,
    },
  };
}

async function capturePaymentTermsCreateOnOrder(): Promise<string> {
  const client = await clientFor('2026-04');
  const orderCreateDocument = await readText(
    'config/parity-requests/payments/payment-terms-create-on-order-create.graphql',
  );
  const lifecycleCreateDocument = await readText(
    'config/parity-requests/payments/payment-terms-lifecycle-create.graphql',
  );
  const multipleDocument = await readText(
    'config/parity-requests/payments/payment-terms-create-on-order-multiple.graphql',
  );
  const updateDocument = await readText('config/parity-requests/payments/payment-terms-lifecycle-update.graphql');
  const deleteDocument = await readText('config/parity-requests/payments/payment-terms-lifecycle-delete.graphql');
  const stamp = Date.now();
  const cleanup: JsonRecord = {};
  let orderId: string | null = null;
  let fixture: JsonRecord | null = null;

  try {
    const orderCreate = await run(client, orderCreateDocument, orderCreateVariables('create-on-order', stamp));
    assertNoUserErrors(orderCreate, 'orderCreate', 'payment terms create-on-order orderCreate');
    orderId = requireId(
      readRecord(payloadRoot(orderCreate, 'orderCreate')?.['order'])?.['id'],
      'payment terms create-on-order orderCreate',
    );

    const create = await run(client, lifecycleCreateDocument, {
      referenceId: orderId,
      attrs: paymentTermsAttrs(),
    });
    paymentTermsIdFromCreate(create, 'payment terms create-on-order create');

    const multiple = await run(client, multipleDocument, {
      referenceId: orderId,
      attrs: multiplePaymentTermsAttrs(),
    });
    assertNoTopLevelErrors(multiple, 'payment terms create-on-order multiple schedules');

    const missingUpdateVariables = {
      input: {
        paymentTermsId: 'gid://shopify/PaymentTerms/999999999999999',
        paymentTermsAttributes: paymentTermsAttrs(),
      },
    };
    const update = await run(client, updateDocument, missingUpdateVariables);
    assertNoTopLevelErrors(update, 'payment terms create-on-order missing update');

    const missingDeleteVariables = {
      input: {
        paymentTermsId: 'gid://shopify/PaymentTerms/999999999999999',
      },
    };
    const missingDelete = await run(client, deleteDocument, missingDeleteVariables);
    assertNoTopLevelErrors(missingDelete, 'payment terms create-on-order missing delete');

    fixture = {
      capturedAt: new Date().toISOString(),
      storeDomain: client.config.storeDomain,
      apiVersion: client.config.apiVersion,
      notes:
        'Live Shopify capture replacing the former local-runtime payment-terms-create-on-order parity fixture. The disposable Order is cancelled in cleanup after recording create, multiple-schedule validation, missing update, and missing delete branches.',
      paymentTermsCreateOnOrder: {
        orderCreate: {
          variables: orderCreate.variables,
        },
        paymentTermsCreate: {
          variables: {
            attrs: paymentTermsAttrs(),
          },
        },
        multipleSchedules: {
          variables: {
            attrs: multiplePaymentTermsAttrs(),
          },
        },
        missingUpdate: {
          variables: missingUpdateVariables,
        },
        missingDelete: {
          variables: missingDeleteVariables,
        },
        expected: {
          orderCreate: orderCreate.response.payload,
          create: create.response.payload,
          multiple: multiple.response.payload,
          update: update.response.payload,
          delete: missingDelete.response.payload,
        },
      },
      cleanup,
      upstreamCalls: [],
    };
  } finally {
    if (orderId) cleanup['orderCancel'] = await cancelOrder(client, orderId);
  }

  if (!fixture) throw new Error('payment-terms-create-on-order fixture was not captured.');
  const filePath = outputPath(client.config, 'payment-terms-create-on-order.json');
  await writeJson(filePath, fixture);
  return filePath;
}

async function capturePaymentTermsDeleteOwnerCascade(): Promise<string> {
  const client = await clientFor('2026-04');
  const orderCreateDocument = await readText(
    'config/parity-requests/payments/payment-terms-create-on-order-create.graphql',
  );
  const lifecycleCreateDocument = await readText(
    'config/parity-requests/payments/payment-terms-lifecycle-create.graphql',
  );
  const deleteDocument = await readText('config/parity-requests/payments/payment-terms-lifecycle-delete.graphql');
  const draftReadDocument = await readText(
    'config/parity-requests/payments/payment-terms-owner-cascade-draft-read.graphql',
  );
  const orderReadDocument = await readText(
    'config/parity-requests/payments/payment-terms-owner-cascade-order-read.graphql',
  );
  const stamp = Date.now();
  const cleanup: JsonRecord = {};
  let orderId: string | null = null;
  let draftOrderId: string | null = null;
  let fixture: JsonRecord | null = null;

  try {
    const draftOrderCreate = await run(
      client,
      draftOrderCreateDocument,
      draftOrderCreateVariables('delete-cascade', stamp),
    );
    assertNoUserErrors(draftOrderCreate, 'draftOrderCreate', 'payment terms delete cascade draftOrderCreate');
    const draftOrder = readRecord(payloadRoot(draftOrderCreate, 'draftOrderCreate')?.['draftOrder']);
    draftOrderId = requireId(draftOrder?.['id'], 'payment terms delete cascade draftOrderCreate');

    const draftHydrate = await run(client, paymentTermsDraftHydrateDocument, { id: draftOrderId });
    assertNoTopLevelErrors(draftHydrate, 'payment terms delete cascade draft hydrate');
    const draftCreate = await run(client, lifecycleCreateDocument, {
      referenceId: draftOrderId,
      attrs: paymentTermsAttrs(),
    });
    const draftTermsId = paymentTermsIdFromCreate(draftCreate, 'payment terms delete cascade draft create');
    const draftDelete = await run(client, deleteDocument, { input: { paymentTermsId: draftTermsId } });
    assertNoUserErrors(draftDelete, 'paymentTermsDelete', 'payment terms delete cascade draft delete');
    const draftReadAfterDelete = await run(client, draftReadDocument, { id: draftOrderId });
    assertNoTopLevelErrors(draftReadAfterDelete, 'payment terms delete cascade draft read after delete');

    const orderCreate = await run(client, orderCreateDocument, orderCreateVariables('delete-cascade', stamp));
    assertNoUserErrors(orderCreate, 'orderCreate', 'payment terms delete cascade orderCreate');
    orderId = requireId(
      readRecord(payloadRoot(orderCreate, 'orderCreate')?.['order'])?.['id'],
      'payment terms delete cascade orderCreate',
    );
    const orderHydrate = await run(client, paymentTermsOwnerHydrateDocument, { id: orderId });
    assertNoTopLevelErrors(orderHydrate, 'payment terms delete cascade order hydrate');
    const orderCreateTerms = await run(client, lifecycleCreateDocument, {
      referenceId: orderId,
      attrs: paymentTermsAttrs(),
    });
    const orderTermsId = paymentTermsIdFromCreate(orderCreateTerms, 'payment terms delete cascade order create');
    const orderDelete = await run(client, deleteDocument, { input: { paymentTermsId: orderTermsId } });
    assertNoUserErrors(orderDelete, 'paymentTermsDelete', 'payment terms delete cascade order delete');
    const orderReadAfterDelete = await run(client, orderReadDocument, { id: orderId });
    assertNoTopLevelErrors(orderReadAfterDelete, 'payment terms delete cascade order read after delete');
    const missingDeleteVariables = {
      input: {
        paymentTermsId: 'gid://shopify/PaymentTerms/999999999999999',
      },
    };
    const missingDelete = await run(client, deleteDocument, missingDeleteVariables);
    assertNoTopLevelErrors(missingDelete, 'payment terms delete cascade missing delete');

    fixture = {
      capturedAt: new Date().toISOString(),
      storeDomain: client.config.storeDomain,
      apiVersion: client.config.apiVersion,
      notes:
        'Live Shopify capture replacing the former local-runtime paymentTermsDelete owner-cascade parity fixture. Disposable DraftOrder and Order owners are created through public Admin GraphQL, payment terms are created and deleted, and owner reads confirm paymentTerms is null after delete.',
      draft: {
        setup: {
          draftOrderCreate: capturePayload(draftOrderCreate),
        },
        owner: {
          id: draftOrderId,
          name: readString(draftOrder?.['name']),
        },
        paymentTermsCreate: {
          variables: {
            attrs: paymentTermsAttrs(),
          },
        },
        expected: {
          create: draftCreate.response.payload,
          delete: draftDelete.response.payload,
          readAfterDelete: draftReadAfterDelete.response.payload,
        },
      },
      order: {
        orderCreate: {
          variables: orderCreate.variables,
        },
        paymentTermsCreate: {
          variables: {
            attrs: paymentTermsAttrs(),
          },
        },
        missingDelete: {
          variables: missingDeleteVariables,
        },
        expected: {
          orderCreate: orderCreate.response.payload,
          create: orderCreateTerms.response.payload,
          delete: orderDelete.response.payload,
          readAfterDelete: orderReadAfterDelete.response.payload,
          missingDelete: missingDelete.response.payload,
        },
      },
      cleanup,
      upstreamCalls: [
        upstreamCall('PaymentTermsDraftHydrate', draftHydrate),
        upstreamCall('PaymentTermsOwnerHydrate', orderHydrate),
      ],
    };
  } finally {
    if (draftOrderId) cleanup['draftOrderDelete'] = await deleteDraftOrder(client, draftOrderId);
    if (orderId) cleanup['orderCancel'] = await cancelOrder(client, orderId);
  }

  if (!fixture) throw new Error('payment-terms-delete-owner-cascade fixture was not captured.');
  const filePath = outputPath(client.config, 'payment-terms-delete-owner-cascade.json');
  await writeJson(filePath, fixture);
  return filePath;
}

async function capturePaymentReminderSendShape(): Promise<string> {
  const client = await clientFor('2025-01');
  const query = await readText('config/parity-requests/payments/payment-reminder-send-invalid-field.graphql');
  const variables = { paymentScheduleId: 'gid://shopify/PaymentSchedule/999999999999999' };
  const response = await client.runGraphqlRequest<JsonRecord>(query, variables);
  const errors = readArray(response.payload['errors']);
  if (response.status < 200 || response.status >= 300 || errors.length === 0) {
    throw new Error(
      `paymentReminderSend invalid selection did not return a schema error: ${JSON.stringify(response, null, 2)}`,
    );
  }

  const filePath = outputPath(client.config, 'payment-reminder-send-shape.json');
  await writeJson(filePath, {
    capturedAt: new Date().toISOString(),
    scenarioId: 'payment-reminder-send-shape',
    storeDomain: client.config.storeDomain,
    apiVersion: client.config.apiVersion,
    notes:
      'Live Shopify schema-validation capture replacing the former local-runtime paymentReminderSend payload-shape fixture. The invalid customerPaymentMethod selection is rejected before mutation execution.',
    cases: {
      invalidSelection: {
        request: {
          query,
          variables,
        },
        response: response.payload,
      },
    },
    upstreamCalls: [],
  });
  return filePath;
}

async function captureCustomerPaymentMethodAccessProbe(): Promise<string> {
  const client = await clientFor('2026-04');
  const scopesQuery = `query CustomerPaymentMethodScopeProbe {
    currentAppInstallation {
      accessScopes {
        handle
      }
    }
  }`;
  const readQuery = `query CustomerPaymentMethodAccessProbe($id: ID!) {
    customerPaymentMethod(id: $id) {
      id
      instrument {
        __typename
      }
    }
  }`;
  const remoteCreateQuery = `mutation CustomerPaymentMethodRemoteCreateAccessProbe {
    customerPaymentMethodRemoteCreate(
      customerId: "gid://shopify/Customer/1"
      remoteReference: { stripePaymentMethod: { customerId: "cus_x", paymentMethodId: "pm_x" } }
    ) {
      customerPaymentMethod {
        id
      }
      userErrors {
        field
        code
        message
      }
    }
  }`;

  const scopes = await client.runGraphqlRequest<JsonRecord>(scopesQuery, {});
  const read = await client.runGraphqlRequest<JsonRecord>(readQuery, {
    id: 'gid://shopify/CustomerPaymentMethod/999999999999999',
  });
  const remoteCreate = await client.runGraphqlRequest<JsonRecord>(remoteCreateQuery, {});
  const scopeHandles = readArray(
    readRecord(readRecord(scopes.payload['data'])?.['currentAppInstallation'])?.['accessScopes'],
  )
    .map((scope) => readRecord(scope)?.['handle'])
    .filter((handle): handle is string => typeof handle === 'string');

  const filePath = outputPath(client.config, 'customer-payment-method-access-probe.json');
  await writeJson(filePath, {
    capturedAt: new Date().toISOString(),
    storeDomain: client.config.storeDomain,
    apiVersion: client.config.apiVersion,
    scenarioId: 'customer-payment-method-access-probe',
    notes:
      'Live access probe for the removed customer-payment-method local-runtime parity fixtures. The checked-in conformance app config now requests read_customer_payment_methods/write_customer_payment_methods, but unattended `shopify app deploy --allow-updates` was blocked by interactive Shopify device-code login, so the active token is probed here before deciding whether real vaulted payment-method fixtures can be recorded.',
    scopeDeploymentAttempt: {
      appConfigPath: 'shopify-conformance-app/hermes-conformance-products/shopify.app.toml',
      requestedScopesAdded: ['read_customer_payment_methods', 'write_customer_payment_methods'],
      command: 'corepack pnpm exec shopify app deploy --allow-updates',
      result: 'blocked by interactive Shopify device-code login in unattended workspace',
    },
    activeAccessScopes: scopeHandles,
    probes: {
      currentAppInstallationAccessScopes: {
        query: scopesQuery,
        variables: {},
        response: scopes.payload,
      },
      customerPaymentMethodRead: {
        query: readQuery,
        variables: { id: 'gid://shopify/CustomerPaymentMethod/999999999999999' },
        response: read.payload,
      },
      customerPaymentMethodRemoteCreate: {
        query: remoteCreateQuery,
        variables: {},
        response: remoteCreate.payload,
      },
    },
    upstreamCalls: [],
  });
  return filePath;
}

const written = [
  await captureCustomerPaymentMethodAccessProbe(),
  await capturePaymentTermsCreateOnOrder(),
  await capturePaymentTermsDeleteOwnerCascade(),
  await capturePaymentReminderSendShape(),
];

console.log(JSON.stringify({ ok: true, written }, null, 2));

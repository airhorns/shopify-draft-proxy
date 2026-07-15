/* oxlint-disable no-console -- CLI capture script intentionally writes progress to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonObject = Record<string, unknown>;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const fixturePath = path.join(
  'fixtures',
  'conformance',
  storeDomain,
  apiVersion,
  'orders',
  'orderUpdate-localization-and-staff.json',
);
const orderHydrateQuery = await readFile('config/parity-requests/orders/order-hydrate-pageable.graphql', 'utf8');

const orderSelection = `
  id
  name
  email
  note
  tags
  localizedFields(first: 5) {
    nodes {
      key
      value
    }
  }
  localizationExtensions(first: 5) {
    nodes {
      key
      value
    }
  }
`;

const orderCreateMutation = `#graphql
  mutation OrderUpdateLocalizationAndStaffCreate($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
    orderCreate(order: $order, options: $options) {
      order {
        ${orderSelection}
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const orderReadQuery = `#graphql
  query OrderUpdateLocalizationAndStaffRead($id: ID!) {
    order(id: $id) {
      ${orderSelection}
    }
  }
`;

const orderParityReadQuery = `#graphql
  query OrderUpdateLocalizationAndStaffRead($id: ID!) {
    order(id: $id) {
      id
      localizedFields(first: 5) {
        nodes {
          key
          value
        }
      }
      localizationExtensions(first: 5) {
        nodes {
          key
          value
        }
      }
    }
  }
`;

const orderUpdateLocalizationMutation = `#graphql
  mutation OrderUpdateLocalizationAndStaff($input: OrderInput!) {
    orderUpdate(input: $input) {
      order {
        id
        localizedFields(first: 5) {
          nodes {
            key
            value
          }
        }
        localizationExtensions(first: 5) {
          nodes {
            key
            value
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

const orderUpdateStaffMemberIdSchemaProbeMutation = `#graphql
  mutation OrderUpdateStaffMemberIdSchemaProbe($input: OrderInput!) {
    orderUpdate(input: $input) {
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

const orderCancelMutation = `#graphql
  mutation OrderUpdateLocalizationAndStaffCleanup(
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

function stripGraphqlTag(query: string): string {
  return query.replace(/^#graphql\n/u, '').trim();
}

async function writeJson(filePath: string, payload: unknown): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${JSON.stringify(payload, null, 2)}\n`);
}

function readObject(value: unknown): JsonObject | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonObject) : null;
}

function readArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function assertNoTopLevelErrors(label: string, result: ConformanceGraphqlResult): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result.payload, null, 2)}`);
  }
}

function assertTopLevelError(label: string, result: ConformanceGraphqlResult, messageFragment: string): void {
  const errors = readArray(result.payload.errors);
  const found = errors.some((entry) => {
    const message = readObject(entry)?.['message'];
    return typeof message === 'string' && message.includes(messageFragment);
  });
  if (!found) {
    throw new Error(`${label} did not return expected top-level error: ${JSON.stringify(result.payload, null, 2)}`);
  }
}

function dataRoot(result: ConformanceGraphqlResult, rootName: string): JsonObject {
  const data = readObject(result.payload.data);
  const root = data?.[rootName];
  const rootObject = readObject(root);
  if (rootObject === null) {
    throw new Error(`Missing ${rootName} payload: ${JSON.stringify(result.payload, null, 2)}`);
  }
  return rootObject;
}

function orderIdFromCreate(result: ConformanceGraphqlResult): string {
  const order = readObject(dataRoot(result, 'orderCreate')['order']);
  const orderId = order?.['id'];
  if (typeof orderId !== 'string') {
    throw new Error(`Missing created order id: ${JSON.stringify(result.payload, null, 2)}`);
  }
  return orderId;
}

function assertEmptyUserErrors(label: string, result: ConformanceGraphqlResult, rootName: string): void {
  const userErrors = dataRoot(result, rootName)['userErrors'];
  if (!Array.isArray(userErrors) || userErrors.length !== 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function assertConnectionValue(
  label: string,
  order: JsonObject | null,
  connectionName: string,
  expectedKey: string,
  expectedValue: string,
): void {
  const connection = readObject(order?.[connectionName]);
  const nodes = readArray(connection?.['nodes']);
  const found = nodes.some((node) => {
    const nodeObject = readObject(node);
    return nodeObject?.['key'] === expectedKey && nodeObject['value'] === expectedValue;
  });
  if (!found) {
    throw new Error(`${label} did not persist ${connectionName}: ${JSON.stringify(order, null, 2)}`);
  }
}

function orderCreateVariables(stamp: number): JsonObject {
  return {
    order: {
      email: `order-update-localization-${stamp}@example.com`,
      note: 'orderUpdate localization baseline',
      tags: ['order-update-localization', String(stamp)],
      test: true,
      currency: 'USD',
      lineItems: [
        {
          title: `orderUpdate localization item ${stamp}`,
          quantity: 1,
          priceSet: {
            shopMoney: {
              amount: '1.00',
              currencyCode: 'USD',
            },
          },
          requiresShipping: false,
          taxable: false,
          sku: `order-update-localization-${stamp}`,
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
              amount: '1.00',
              currencyCode: 'USD',
            },
          },
        },
      ],
    },
    options: {
      inventoryBehaviour: 'BYPASS',
      sendReceipt: false,
      sendFulfillmentReceipt: false,
    },
  };
}

function localizedFieldsVariables(orderId: string): JsonObject {
  return {
    input: {
      id: orderId,
      localizedFields: [
        {
          key: 'TAX_CREDENTIAL_BR',
          value: '52998224725',
        },
      ],
    },
  };
}

function localizationExtensionVariables(orderId: string): JsonObject {
  return {
    input: {
      id: orderId,
      localizationExtensions: [
        {
          key: 'SHIPPING_CREDENTIAL_BR',
          value: '52998224725',
        },
      ],
    },
  };
}

function staffSchemaProbeVariables(orderId: string): JsonObject {
  return {
    input: {
      id: orderId,
      staffMemberId: 'gid://shopify/StaffMember/999999999999',
    },
  };
}

async function cleanupOrder(orderId: string): Promise<unknown> {
  const variables = {
    orderId,
    reason: 'OTHER',
    notifyCustomer: false,
    restock: false,
  };
  const result = await runGraphqlRequest(orderCancelMutation, variables);
  return {
    document: stripGraphqlTag(orderCancelMutation),
    variables,
    response: result.payload,
  };
}

const stamp = Date.now();
const createVariables = orderCreateVariables(stamp);
const createResult = await runGraphqlRequest(orderCreateMutation, createVariables);
assertNoTopLevelErrors('orderCreate setup', createResult);
assertEmptyUserErrors('orderCreate setup', createResult, 'orderCreate');

const createdOrderId = orderIdFromCreate(createResult);
const beforeRead = await runGraphqlRequest(orderReadQuery, { id: createdOrderId });
assertNoTopLevelErrors('pre-update order read', beforeRead);
const hydrateBeforeUpdateVariables = { id: createdOrderId, lineItemsAfter: null };
const hydrateBeforeUpdate = await runGraphqlRequest(orderHydrateQuery, hydrateBeforeUpdateVariables);
assertNoTopLevelErrors('orderUpdate runtime hydrate', hydrateBeforeUpdate);

const variables = localizedFieldsVariables(createdOrderId);
const mutationResult = await runGraphqlRequest(orderUpdateLocalizationMutation, variables);
assertNoTopLevelErrors('orderUpdate localizedFields', mutationResult);
assertEmptyUserErrors('orderUpdate localizedFields', mutationResult, 'orderUpdate');

const mutationOrder = readObject(dataRoot(mutationResult, 'orderUpdate')['order']);
assertConnectionValue(
  'orderUpdate localizedFields mutation',
  mutationOrder,
  'localizedFields',
  'TAX_CREDENTIAL_BR',
  '52998224725',
);
assertConnectionValue(
  'orderUpdate localizedFields mutation alias',
  mutationOrder,
  'localizationExtensions',
  'TAX_CREDENTIAL_BR',
  '52998224725',
);

const localizationExtensionMutationVariables = localizationExtensionVariables(createdOrderId);
const localizationExtensionMutationResult = await runGraphqlRequest(
  orderUpdateLocalizationMutation,
  localizationExtensionMutationVariables,
);
assertNoTopLevelErrors('orderUpdate localizationExtensions', localizationExtensionMutationResult);
assertEmptyUserErrors('orderUpdate localizationExtensions', localizationExtensionMutationResult, 'orderUpdate');

const localizationExtensionMutationOrder = readObject(
  dataRoot(localizationExtensionMutationResult, 'orderUpdate')['order'],
);
assertConnectionValue(
  'orderUpdate localizationExtensions mutation alias',
  localizationExtensionMutationOrder,
  'localizedFields',
  'SHIPPING_CREDENTIAL_BR',
  '52998224725',
);
assertConnectionValue(
  'orderUpdate localizationExtensions mutation',
  localizationExtensionMutationOrder,
  'localizationExtensions',
  'SHIPPING_CREDENTIAL_BR',
  '52998224725',
);

const downstreamRead = await runGraphqlRequest(orderParityReadQuery, { id: createdOrderId });
assertNoTopLevelErrors('post-update order read', downstreamRead);
const downstreamOrder = readObject(readObject(downstreamRead.payload.data)?.['order']);
assertConnectionValue(
  'orderUpdate localization downstream read',
  downstreamOrder,
  'localizedFields',
  'TAX_CREDENTIAL_BR',
  '52998224725',
);
assertConnectionValue(
  'orderUpdate localization downstream read',
  downstreamOrder,
  'localizationExtensions',
  'SHIPPING_CREDENTIAL_BR',
  '52998224725',
);

const staffMemberIdSchemaProbeVariables = staffSchemaProbeVariables(createdOrderId);
const staffMemberIdSchemaProbe = await runGraphqlRequest(
  orderUpdateStaffMemberIdSchemaProbeMutation,
  staffMemberIdSchemaProbeVariables,
);
assertTopLevelError('orderUpdate staffMemberId schema probe', staffMemberIdSchemaProbe, 'staffMemberId');

const cleanup = await cleanupOrder(createdOrderId);

await writeJson(fixturePath, {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  document: stripGraphqlTag(orderUpdateLocalizationMutation),
  variables,
  setup: {
    orderCreate: {
      document: stripGraphqlTag(orderCreateMutation),
      variables: createVariables,
      response: createResult.payload,
    },
    beforeRead: {
      document: stripGraphqlTag(orderReadQuery),
      variables: { id: createdOrderId },
      response: beforeRead.payload,
    },
  },
  mutation: {
    response: mutationResult.payload,
  },
  localizationExtensionMutation: {
    document: stripGraphqlTag(orderUpdateLocalizationMutation),
    variables: localizationExtensionMutationVariables,
    response: localizationExtensionMutationResult.payload,
  },
  downstreamRead: {
    document: stripGraphqlTag(orderParityReadQuery),
    variables: { id: createdOrderId },
    response: downstreamRead.payload,
  },
  staffMemberIdSchemaProbe: {
    document: stripGraphqlTag(orderUpdateStaffMemberIdSchemaProbeMutation),
    variables: staffMemberIdSchemaProbeVariables,
    response: staffMemberIdSchemaProbe.payload,
    notes:
      'The configured public 2026-04 Admin schema rejects OrderInput.staffMemberId before resolver execution; focused runtime tests cover the internal staffMemberId NOT_FOUND branch.',
  },
  cleanup,
  upstreamCalls: [
    {
      operationName: 'OrdersOrderHydrate',
      variables: hydrateBeforeUpdateVariables,
      query: orderHydrateQuery,
      response: {
        status: hydrateBeforeUpdate.status,
        body: hydrateBeforeUpdate.payload,
      },
    },
  ],
});

console.log(`Wrote ${fixturePath}`);

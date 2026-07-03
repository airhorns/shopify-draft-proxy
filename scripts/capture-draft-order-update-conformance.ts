/* oxlint-disable no-console -- CLI capture script intentionally writes progress to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'orders');
const fixturePath = path.join(fixtureDir, 'draft-order-update-parity.json');
const requestDir = path.join('config', 'parity-requests', 'orders');

async function readRequest(name: string): Promise<string> {
  return readFile(path.join(requestDir, name), 'utf8');
}

function readRecord(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function requireString(value: unknown, label: string): string {
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`Missing required capture value: ${label}`);
  }
  return value;
}

async function writeJson(filePath: string, payload: unknown): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${JSON.stringify(payload, null, 2)}\n`);
}

async function run(query: string, variables: JsonRecord, label: string): Promise<JsonRecord> {
  const result: ConformanceGraphqlResult<JsonRecord> = await runGraphqlRequest<JsonRecord>(query, variables);
  if (result.status < 200 || result.status >= 300 || result.payload?.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
  return result.payload as JsonRecord;
}

function mutationRoot(payload: JsonRecord, rootName: string, label: string): JsonRecord {
  const root = readRecord(readRecord(payload['data'])?.[rootName]);
  if (!root) {
    throw new Error(`${label} missing ${rootName}: ${JSON.stringify(payload, null, 2)}`);
  }
  const userErrors = readArray(root['userErrors']);
  if (userErrors.length > 0) {
    throw new Error(`${label} ${rootName} userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
  return root;
}

const now = new Date();
const stamp = now
  .toISOString()
  .replace(/[-:.TZ]/gu, '')
  .slice(0, 14);

// A safely-future reserve-until instant (~90 days out) so draftOrderCreate accepts
// it regardless of when this capture is replayed.
const reserveInventoryUntil = `${new Date(now.getTime() + 90 * 24 * 60 * 60 * 1000)
  .toISOString()
  .slice(0, 10)}T12:00:00Z`;

// The exact draft-order hydrate query the proxy forwards on a cold draftOrderUpdate.
// Read from the shared .graphql file (include_str! into DRAFT_ORDER_HYDRATE_QUERY)
// so the recorded cassette byte-matches the forward under the strict matcher.
const draftOrderHydrateQuery = await readFile(path.join(requestDir, 'draft-order-hydrate.graphql'), 'utf8');
const updateDocument = await readRequest('draftOrderUpdate-parity-plan.graphql');
const downstreamReadDocument = await readRequest('draftOrderCreate-downstream-read.graphql');

// Disposable draft mirroring the original setup precondition (real customer +
// untracked variant on harry-test-heelo so the proxy's cold hydrate reflects a
// real merchant draft, not a seed). Untracked/non-taxable lines keep the proxy's
// tax-free draft math aligned with Shopify's calculated totals.
const draftOrderCreateMutation = `#graphql
  mutation DraftOrderUpdateCaptureCreate($input: DraftOrderInput!) {
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

const draftOrderDeleteMutation = `#graphql
  mutation DraftOrderUpdateCaptureDelete($input: DraftOrderDeleteInput!) {
    draftOrderDelete(input: $input) {
      deletedId
      userErrors {
        field
        message
      }
    }
  }
`;

const createInput = {
  purchasingEntity: { customerId: 'gid://shopify/Customer/9522206933298' },
  email: `hermes-draft-order-update-${stamp}@example.com`,
  note: 'draft order update setup',
  taxExempt: true,
  reserveInventoryUntil,
  tags: ['parity-capture', 'draft-order-family', 'update'],
  customAttributes: [
    { key: 'source', value: 'phone-order' },
    { key: 'purchase-order', value: 'PO-117' },
  ],
  appliedDiscount: {
    title: 'Loyalty credit',
    description: 'merchant order-level discount',
    value: 5,
    amount: 5,
    valueType: 'FIXED_AMOUNT',
  },
  billingAddress: {
    firstName: 'Hermes',
    lastName: 'Buyer',
    address1: '123 Queen St W',
    city: 'Toronto',
    provinceCode: 'ON',
    countryCode: 'CA',
    zip: 'M5H 2M9',
    phone: '+14165550101',
  },
  shippingAddress: {
    firstName: 'Hermes',
    lastName: 'Buyer',
    address1: '500 King St W',
    city: 'Toronto',
    provinceCode: 'ON',
    countryCode: 'CA',
    zip: 'M5V 1L9',
    phone: '+14165550102',
  },
  shippingLine: { title: 'Merchant Courier', priceWithCurrency: { amount: '7.25', currencyCode: 'CAD' } },
  lineItems: [
    {
      title: 'Custom installation service',
      quantity: 2,
      originalUnitPrice: '20.00',
      requiresShipping: false,
      taxable: false,
      sku: 'CUSTOM-INSTALL',
      appliedDiscount: {
        title: 'Service discount',
        description: '10 percent off service',
        value: 10,
        amount: 4,
        valueType: 'PERCENTAGE',
      },
      customAttributes: [{ key: 'appointment', value: 'morning' }],
    },
    { variantId: 'gid://shopify/ProductVariant/49875425296690', quantity: 1 },
  ],
};

// The supported draftOrderUpdate the parity scenario replays against the cold draft.
const updateInput = {
  email: `hermes-draft-order-update-${stamp}@example.com`,
  note: 'draft order update live parity capture',
  tags: ['draft-order', 'update-parity'],
  customAttributes: [{ key: 'source', value: 'har-118' }],
  shippingLine: { title: 'Standard', priceWithCurrency: { amount: '5.00', currencyCode: 'CAD' } },
};

const createPayload = await run(draftOrderCreateMutation, { input: createInput }, 'draftOrderCreate');
const createdDraft = mutationRoot(createPayload, 'draftOrderCreate', 'draftOrderCreate');
const draftOrderId = requireString(readRecord(createdDraft['draftOrder'])?.['id'], 'created draft order id');

// Capture the exact hydrate the proxy forwards on a cold draftOrderUpdate, before
// the update mutation mutates the draft.
const hydratePayload = await run(draftOrderHydrateQuery, { id: draftOrderId }, 'draftOrderHydrate');

const updatePayload = await run(updateDocument, { id: draftOrderId, input: updateInput }, 'draftOrderUpdate');
mutationRoot(updatePayload, 'draftOrderUpdate', 'draftOrderUpdate');

const downstreamPayload = await run(downstreamReadDocument, { id: draftOrderId }, 'draftOrderDownstreamRead');

// Best-effort cleanup of the disposable draft. Errors reported, not fatal.
const cleanup = await runGraphqlRequest<JsonRecord>(draftOrderDeleteMutation, {
  input: { id: draftOrderId },
});

await writeJson(fixturePath, {
  scenarioId: 'draft-order-update-live-parity',
  apiVersion,
  storeDomain,
  recordedAt: new Date().toISOString(),
  source: 'live-shopify-admin-graphql',
  variables: { id: draftOrderId, input: updateInput },
  mutation: { response: updatePayload },
  downstreamRead: { response: downstreamPayload },
  upstreamCalls: [
    {
      operationName: 'OrdersDraftOrderHydrate',
      variables: { id: draftOrderId },
      query: draftOrderHydrateQuery,
      response: {
        status: 200,
        body: hydratePayload,
      },
    },
  ],
});

console.log(
  JSON.stringify(
    {
      fixturePath,
      draftOrderId,
      cleanupStatus: cleanup.status,
      cleanupDeletedId: readRecord(readRecord(cleanup.payload?.['data'])?.['draftOrderDelete'])?.['deletedId'],
    },
    null,
    2,
  ),
);

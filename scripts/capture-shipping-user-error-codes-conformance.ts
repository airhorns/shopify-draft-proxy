/* oxlint-disable no-console -- CLI scripts intentionally write capture status to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type GraphqlCapture = {
  query: string;
  variables: JsonRecord;
  response: ConformanceGraphqlResult;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'shipping-fulfillments');
const outputPath = path.join(outputDir, 'shipping-user-error-codes.json');

const carrierServiceCreateBlankMutation = `#graphql
  mutation ShippingUserErrorCodesCarrierCreate($input: DeliveryCarrierServiceCreateInput!) {
    carrierServiceCreate(input: $input) {
      carrierService {
        id
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const carrierServiceUpdateUnknownMutation = `#graphql
  mutation ShippingUserErrorCodesCarrierUpdate($input: DeliveryCarrierServiceUpdateInput!) {
    carrierServiceUpdate(input: $input) {
      carrierService {
        id
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const carrierServiceDeleteUnknownMutation = `#graphql
  mutation ShippingUserErrorCodesCarrierDelete($id: ID!) {
    carrierServiceDelete(id: $id) {
      deletedId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

function trimGraphql(query: string): string {
  return query.replace(/^#graphql\n/u, '').trim();
}

async function capture(query: string, variables: JsonRecord): Promise<GraphqlCapture> {
  return {
    query: trimGraphql(query),
    variables,
    response: await runGraphqlRequest(query, variables),
  };
}

function readRecord(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function userErrors(captureResult: GraphqlCapture, root: string): JsonRecord[] {
  const payload = readRecord(captureResult.response.payload);
  const data = readRecord(payload?.['data']);
  const rootPayload = readRecord(data?.[root]);
  const errors = rootPayload?.['userErrors'];
  return Array.isArray(errors) ? errors.filter((error): error is JsonRecord => readRecord(error) !== null) : [];
}

function assertFirstCode(captureResult: GraphqlCapture, root: string, expectedCode: string): void {
  const errors = userErrors(captureResult, root);
  const firstCode = errors[0]?.['code'];
  if (firstCode !== expectedCode) {
    throw new Error(`${root} expected first userError code ${expectedCode}, got ${JSON.stringify(errors)}`);
  }
}

const unknownCarrierServiceId = 'gid://shopify/DeliveryCarrierService/999999999999';

const blankCarrierCreate = await capture(carrierServiceCreateBlankMutation, {
  input: {
    name: '',
    callbackUrl: 'https://mock.shop/carrier-service-rates',
    supportsServiceDiscovery: false,
    active: true,
  },
});
assertFirstCode(blankCarrierCreate, 'carrierServiceCreate', 'CARRIER_SERVICE_CREATE_FAILED');

const unknownCarrierUpdate = await capture(carrierServiceUpdateUnknownMutation, {
  input: {
    id: unknownCarrierServiceId,
    name: 'HAR-578 unknown carrier',
  },
});
assertFirstCode(unknownCarrierUpdate, 'carrierServiceUpdate', 'CARRIER_SERVICE_UPDATE_FAILED');

const unknownCarrierDelete = await capture(carrierServiceDeleteUnknownMutation, {
  id: unknownCarrierServiceId,
});
assertFirstCode(unknownCarrierDelete, 'carrierServiceDelete', 'CARRIER_SERVICE_DELETE_FAILED');

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      storeDomain,
      apiVersion,
      capturedAt: new Date().toISOString(),
      captureType: 'shipping-user-error-code-parity',
      scopedRoots: ['carrierServiceCreate', 'carrierServiceUpdate', 'carrierServiceDelete'],
      notes: [
        'HAR-578 captures typed carrier-service userError.code parity for blank-create and unknown-id update/delete validation branches.',
        'Other shipping-fulfillment payloads that expose generic UserError in Admin 2026-04 cannot be code-selected in live conformance and are intentionally excluded from this code-specific capture.',
      ],
      captures: {
        blankCarrierCreate,
        unknownCarrierUpdate,
        unknownCarrierDelete,
      },
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
);

console.log(JSON.stringify({ ok: true, outputPath }, null, 2));

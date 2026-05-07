/* oxlint-disable no-console -- CLI capture script intentionally writes progress to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type TypeRef = {
  kind?: string;
  name?: string | null;
  ofType?: TypeRef | null;
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

const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'orders');
const outputPath = path.join(fixtureDir, 'return-reverse-logistics-introspection.json');

const introspectionQuery = `#graphql
  query ReturnReverseLogisticsIntrospection {
    __schema {
      queryType {
        fields {
          name
          args {
            name
            type { ...TypeRef }
          }
          type { ...TypeRef }
        }
      }
      mutationType {
        fields {
          name
          args {
            name
            type { ...TypeRef }
          }
          type { ...TypeRef }
        }
      }
    }
    returnCreatePayload: __type(name: "ReturnCreatePayload") {
      fields { name }
    }
    returnRequestPayload: __type(name: "ReturnRequestPayload") {
      fields { name }
    }
    returnCancelPayload: __type(name: "ReturnCancelPayload") {
      fields { name }
    }
    returnClosePayload: __type(name: "ReturnClosePayload") {
      fields { name }
    }
    returnReopenPayload: __type(name: "ReturnReopenPayload") {
      fields { name }
    }
    returnStatus: __type(name: "ReturnStatus") {
      enumValues(includeDeprecated: true) { name }
    }
    returnInput: __type(name: "ReturnInput") {
      inputFields { name }
    }
    returnLineItemInput: __type(name: "ReturnLineItemInput") {
      inputFields { name }
    }
    returnRequestInput: __type(name: "ReturnRequestInput") {
      inputFields { name }
    }
    returnProcessInput: __type(name: "ReturnProcessInput") {
      inputFields { name }
    }
  }

  fragment TypeRef on __Type {
    kind
    name
    ofType {
      kind
      name
      ofType {
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
`;

async function writeJson(filePath: string, payload: unknown): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${JSON.stringify(payload, null, 2)}\n`);
}

function assertCaptured(result: ConformanceGraphqlResult): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`Return/reverse-logistics introspection failed: ${JSON.stringify(result.payload, null, 2)}`);
  }
}

function asRecord(value: unknown): JsonRecord {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    return {};
  }
  return value as JsonRecord;
}

function readArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function typeName(typeRef: unknown): string {
  const type = asRecord(typeRef) as TypeRef;
  if (type.kind === 'NON_NULL') {
    return `${typeName(type.ofType)}!`;
  }
  if (type.kind === 'LIST') {
    return `[${typeName(type.ofType)}]`;
  }
  return type.name ?? 'Unknown';
}

function rootFields(data: JsonRecord, rootKind: 'queryType' | 'mutationType'): JsonRecord[] {
  return readArray(asRecord(asRecord(data['__schema'])[rootKind])['fields']).map(asRecord);
}

function fieldByName(fields: JsonRecord[], name: string): JsonRecord | null {
  return fields.find((field) => field['name'] === name) ?? null;
}

function argsFor(field: JsonRecord | null): string[] {
  return readArray(field?.['args']).map((arg) => {
    const record = asRecord(arg);
    return `${record['name']}: ${typeName(record['type'])}`;
  });
}

function inputFields(data: JsonRecord, key: string): string[] {
  return readArray(asRecord(data[key])['inputFields']).map((field) => String(asRecord(field)['name']));
}

function payloadFields(data: JsonRecord, key: string): string[] {
  return readArray(asRecord(data[key])['fields']).map((field) => String(asRecord(field)['name']));
}

const result = await runGraphqlRequest(introspectionQuery, {});
assertCaptured(result);

const data = asRecord(result.payload['data']);
const queryFields = rootFields(data, 'queryType');
const mutationFields = rootFields(data, 'mutationType');
const queryRootNames = [
  'return',
  'returnCalculate',
  'returnableFulfillment',
  'returnableFulfillments',
  'reverseDelivery',
  'reverseFulfillmentOrder',
];
const mutationRootNames = [
  'returnCreate',
  'returnRequest',
  'returnCancel',
  'returnClose',
  'returnReopen',
  'removeFromReturn',
  'returnProcess',
  'reverseDeliveryCreateWithShipping',
  'reverseDeliveryShippingUpdate',
  'reverseFulfillmentOrderDispose',
];

const queryRoots = Object.fromEntries(
  queryRootNames.map((name) => {
    const field = fieldByName(queryFields, name);
    const blockers: Record<string, string> = {
      returnCalculate:
        'Requires calculation parity for restocking fees, exchanges, shipping, tax, discount, and validation behavior.',
      returnableFulfillment: 'Requires returnability eligibility and line-item quantity parity over fulfilled orders.',
      returnableFulfillments:
        'Requires order eligibility, pagination, reverse ordering, and returnable line-item quantity behavior.',
      reverseDelivery: 'Requires a normalized reverse-delivery graph linked to reverse fulfillment orders.',
      reverseFulfillmentOrder:
        'Requires reverse fulfillment order lifecycle, line items, reverse deliveries, and order linkage parity.',
    };
    return [
      name,
      {
        args: argsFor(field),
        type: typeName(field?.['type']),
        ...(blockers[name] ? { blocker: blockers[name] } : {}),
      },
    ];
  }),
);

const payloadByRoot: Record<string, string> = {
  returnCreate: 'returnCreatePayload',
  returnRequest: 'returnRequestPayload',
  returnCancel: 'returnCancelPayload',
  returnClose: 'returnClosePayload',
  returnReopen: 'returnReopenPayload',
};

const mutationRoots = Object.fromEntries(
  mutationRootNames.map((name) => {
    const field = fieldByName(mutationFields, name);
    const blockers: Record<string, string> = {
      removeFromReturn: 'Requires removal semantics for return and exchange line items plus downstream read effects.',
      returnProcess:
        'Requires processing semantics for returned/exchanged items, refunds, duties, shipping, financial transfers, and notifications.',
      reverseDeliveryCreateWithShipping:
        'Requires normalized reverse fulfillment order and reverse delivery state plus tracking/label behavior.',
      reverseDeliveryShippingUpdate: 'Requires captured reverse-delivery tracking/label update semantics.',
      reverseFulfillmentOrderDispose:
        'Requires disposition, inventory/location effects, status updates, and downstream reads.',
    };
    return [
      name,
      {
        args: argsFor(field),
        ...(payloadByRoot[name] ? { payloadFields: payloadFields(data, payloadByRoot[name]) } : {}),
        ...(blockers[name] ? { blocker: blockers[name] } : {}),
      },
    ];
  }),
);

await writeJson(outputPath, {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  captureType: 'no-side-effect-introspection',
  queryRoots,
  mutationRoots,
  typeEvidence: {
    ReturnStatus: readArray(asRecord(data['returnStatus'])['enumValues']).map((field) =>
      String(asRecord(field)['name']),
    ),
    ReturnInput: inputFields(data, 'returnInput'),
    ReturnLineItemInput: inputFields(data, 'returnLineItemInput'),
    ReturnRequestInput: inputFields(data, 'returnRequestInput'),
    ReturnProcessInput: inputFields(data, 'returnProcessInput'),
    absentRoots: ['returnApprove', 'returnDecline'].filter((name) => !fieldByName(mutationFields, name)),
  },
  upstreamCalls: [],
});

console.log(JSON.stringify({ ok: true, outputPath }, null, 2));

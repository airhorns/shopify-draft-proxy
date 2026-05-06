/* oxlint-disable no-console -- CLI scripts intentionally write capture status to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type CapturedRequest = {
  documentPath: string;
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
const outputPath = path.join(outputDir, 'carrier-service-callback-url-validation.json');
const primaryDocumentPath = path.join(
  'config',
  'parity-requests',
  'shipping-fulfillments',
  'carrier-service-callback-url-validation.graphql',
);
const updateHttpDocumentPath = path.join(
  'config',
  'parity-requests',
  'shipping-fulfillments',
  'carrier-service-callback-url-validation-update-http.graphql',
);
const updateBannedDocumentPath = path.join(
  'config',
  'parity-requests',
  'shipping-fulfillments',
  'carrier-service-callback-url-validation-update-banned.graphql',
);

const deleteDocument = `#graphql
  mutation CarrierServiceCallbackUrlValidationCleanup($id: ID!) {
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

const createDocument = `#graphql
  mutation CarrierServiceCallbackUrlValidationInvalidCreate($input: DeliveryCarrierServiceCreateInput!) {
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

async function readText(relativePath: string): Promise<string> {
  return readFile(path.join(process.cwd(), relativePath), 'utf8');
}

async function capture(documentPath: string, variables: JsonRecord): Promise<CapturedRequest> {
  const document = await readText(documentPath);
  return {
    documentPath,
    variables,
    response: await runGraphqlRequest(document, variables),
  };
}

async function captureAdHoc(
  query: string,
  variables: JsonRecord,
): Promise<{
  query: string;
  variables: JsonRecord;
  response: ConformanceGraphqlResult;
}> {
  const trimmed = query.replace(/^#graphql\n/u, '').trim();
  return {
    query: trimmed,
    variables,
    response: await runGraphqlRequest(trimmed, variables),
  };
}

function isObject(value: unknown): value is JsonRecord {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function readValidCarrierServiceId(primary: CapturedRequest): string | null {
  const payload = primary.response.payload;
  if (!isObject(payload.data)) return null;
  const validCreate = payload.data['validCreate'];
  if (!isObject(validCreate)) return null;
  const carrierService = validCreate['carrierService'];
  if (!isObject(carrierService)) return null;
  const id = carrierService['id'];
  return typeof id === 'string' ? id : null;
}

function userErrors(captureResult: CapturedRequest, root: string): JsonRecord[] {
  const payload = captureResult.response.payload;
  const data = payload.data;
  if (!isObject(data)) return [];
  const rootPayload = data[root];
  if (!isObject(rootPayload)) return [];
  const errors = rootPayload['userErrors'];
  return Array.isArray(errors) ? errors.filter(isObject) : [];
}

function assertFirstUserError(
  captureResult: CapturedRequest,
  root: string,
  expectedMessage: string,
  expectedCode: string,
): void {
  const errors = userErrors(captureResult, root);
  const first = errors[0];
  if (first?.['message'] !== expectedMessage || first?.['code'] !== expectedCode) {
    throw new Error(`${root} expected ${expectedMessage} / ${expectedCode}, got ${JSON.stringify(errors)}`);
  }
}

function assertInvalidVariable(
  captureResult: {
    response: ConformanceGraphqlResult;
  },
  expectedProblemPath: string,
): void {
  const errors = captureResult.response.payload.errors;
  if (!Array.isArray(errors) || errors.length === 0) {
    throw new Error(`Expected INVALID_VARIABLE errors, got ${JSON.stringify(captureResult.response.payload)}`);
  }
  const first = errors[0];
  if (!isObject(first)) {
    throw new Error(`Malformed GraphQL error: ${JSON.stringify(first)}`);
  }
  const extensions = first['extensions'];
  if (!isObject(extensions) || extensions['code'] !== 'INVALID_VARIABLE') {
    throw new Error(`Expected INVALID_VARIABLE, got ${JSON.stringify(first)}`);
  }
  const problems = extensions['problems'];
  if (
    !Array.isArray(problems) ||
    !problems.some((problem) => {
      if (!isObject(problem) || !Array.isArray(problem['path'])) return false;
      return JSON.stringify(problem['path']) === expectedProblemPath;
    })
  ) {
    throw new Error(`Expected problem path ${expectedProblemPath}, got ${JSON.stringify(problems)}`);
  }
}

const suffix = `${Date.now()}`;
const primary = await capture(primaryDocumentPath, {
  validInput: {
    name: `Hermes Callback Validation ${suffix}`,
    callbackUrl: 'https://mock.shop/carrier-service-rates',
    supportsServiceDiscovery: false,
    active: false,
  },
  httpCreateInput: {
    name: `Hermes HTTP Callback ${suffix}`,
    callbackUrl: 'http://example.com/rates',
    supportsServiceDiscovery: false,
    active: false,
  },
  bannedCreateInput: {
    name: `Hermes Banned Callback ${suffix}`,
    callbackUrl: 'https://shopify.com/rates',
    supportsServiceDiscovery: false,
    active: false,
  },
});

assertFirstUserError(
  primary,
  'httpCreate',
  'Shipping rate provider callback url must use HTTPS',
  'CARRIER_SERVICE_CREATE_FAILED',
);
assertFirstUserError(
  primary,
  'bannedCreate',
  'Shipping rate provider callback url invalid host',
  'CARRIER_SERVICE_CREATE_FAILED',
);

const validCarrierServiceId = readValidCarrierServiceId(primary);
if (validCarrierServiceId === null) {
  throw new Error(`validCreate did not return a carrier service id: ${JSON.stringify(primary.response.payload)}`);
}

let cleanup: Awaited<ReturnType<typeof captureAdHoc>> | null = null;
let updateHttp: CapturedRequest | null = null;
let updateBanned: CapturedRequest | null = null;
try {
  updateHttp = await capture(updateHttpDocumentPath, {
    input: {
      id: validCarrierServiceId,
      callbackUrl: 'http://example.com/rates',
    },
  });
  assertFirstUserError(
    updateHttp,
    'carrierServiceUpdate',
    'Shipping rate provider callback url must use HTTPS',
    'CARRIER_SERVICE_UPDATE_FAILED',
  );

  updateBanned = await capture(updateBannedDocumentPath, {
    input: {
      id: validCarrierServiceId,
      callbackUrl: 'https://shopify.com/rates',
    },
  });
  assertFirstUserError(
    updateBanned,
    'carrierServiceUpdate',
    'Shipping rate provider callback url invalid host',
    'CARRIER_SERVICE_UPDATE_FAILED',
  );
} finally {
  cleanup = await captureAdHoc(deleteDocument, { id: validCarrierServiceId });
}

if (updateHttp === null || updateBanned === null) {
  throw new Error('Expected update validation captures to be present.');
}

const missingCreate = await captureAdHoc(createDocument, {
  input: {
    name: `Hermes Missing Callback ${suffix}`,
    supportsServiceDiscovery: false,
    active: false,
  },
});
assertInvalidVariable(missingCreate, JSON.stringify(['callbackUrl']));

const blankCreate = await captureAdHoc(createDocument, {
  input: {
    name: `Hermes Blank Callback ${suffix}`,
    callbackUrl: '',
    supportsServiceDiscovery: false,
    active: false,
  },
});
assertInvalidVariable(blankCreate, JSON.stringify(['callbackUrl']));

const unparseableCreate = await captureAdHoc(createDocument, {
  input: {
    name: `Hermes Bad Callback ${suffix}`,
    callbackUrl: 'not-a-url',
    supportsServiceDiscovery: false,
    active: false,
  },
});
assertInvalidVariable(unparseableCreate, JSON.stringify(['callbackUrl']));

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      notes: [
        'Captures DeliveryCarrierService callbackUrl validation against live Shopify Admin GraphQL.',
        'Missing, blank, and unparseable callbackUrl inputs fail at GraphQL variable coercion before resolver execution.',
        'http:// and banned-host callbackUrl inputs reach carrier service resolver userErrors with typed create/update codes.',
        'The successful create exists only to target update validation and is deleted during cleanup.',
      ],
      primary,
      updateHttp,
      updateBanned,
      invalidVariableCreates: {
        missingCreate,
        blankCreate,
        unparseableCreate,
      },
      cleanup,
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(JSON.stringify({ ok: true, outputPath }, null, 2));

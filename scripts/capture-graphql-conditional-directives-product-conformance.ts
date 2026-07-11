/* oxlint-disable no-console -- CLI recorder intentionally writes capture status to stdout. */
import {
  createConformanceCapture,
  readArray,
  readRecord,
  requireString,
  type JsonRecord,
} from './conformance-capture-lib.js';

const domain = 'products';
const scenarioId = 'graphql-conditional-directives-product-execution';
const createDocumentFile = 'graphql-conditional-directives-product-create.graphql';
const readDocumentFile = 'graphql-conditional-directives-product-read.graphql';
const skippedCreateDocumentFile = 'graphql-conditional-directives-product-create-skipped.graphql';

type CaptureEntry = {
  query: string;
  variables: JsonRecord;
  response: {
    status: number;
    payload: JsonRecord;
  };
};

function documentPath(documentFile: string): string {
  return `config/parity-requests/${domain}/${documentFile}`;
}

const cleanupProductDeleteDocument = `mutation ConditionalDirectiveProductCleanup($input: ProductDeleteInput!) {
  productDelete(input: $input) {
    deletedProductId
    userErrors {
      field
      message
    }
  }
}
`;

function payloadData(entry: CaptureEntry, label: string): JsonRecord {
  const data = readRecord(entry.response.payload['data']);
  if (!data) {
    throw new Error(`${label} missing data object: ${JSON.stringify(entry.response.payload, null, 2)}`);
  }
  return data;
}

function captureHasTopLevelErrors(entry: CaptureEntry): boolean {
  return readArray(entry.response.payload['errors']).length > 0;
}

function assertNoTopLevelErrors(entry: CaptureEntry, label: string): void {
  if (entry.response.status < 200 || entry.response.status >= 300 || captureHasTopLevelErrors(entry)) {
    throw new Error(`${label} returned top-level errors: ${JSON.stringify(entry.response, null, 2)}`);
  }
}

function assertNoUserErrors(entry: CaptureEntry, rootName: string, label: string): void {
  assertNoTopLevelErrors(entry, label);
  const root = readRecord(payloadData(entry, label)[rootName]);
  if (!root) {
    throw new Error(`${label} missing ${rootName}: ${JSON.stringify(entry.response.payload, null, 2)}`);
  }
  const userErrors = readArray(root['userErrors']);
  if (userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function productIdFromCreate(entry: CaptureEntry): string {
  const root = readRecord(payloadData(entry, 'product create')['productCreate']);
  const product = readRecord(root?.['product']);
  return requireString(product?.['id'], 'productCreate.product.id');
}

function assertOnlyKeys(value: JsonRecord, expectedKeys: string[], label: string): void {
  const actualKeys = Object.keys(value).sort();
  const sortedExpected = [...expectedKeys].sort();
  if (JSON.stringify(actualKeys) !== JSON.stringify(sortedExpected)) {
    throw new Error(`${label} keys mismatch. Expected ${sortedExpected.join(', ')}, got ${actualKeys.join(', ')}`);
  }
}

function assertConditionalReadShape(entry: CaptureEntry): void {
  assertNoTopLevelErrors(entry, 'conditional product read');
  const data = payloadData(entry, 'conditional product read');
  if (Object.prototype.hasOwnProperty.call(data, 'skippedRoot')) {
    throw new Error(`conditional product read unexpectedly included skippedRoot: ${JSON.stringify(data, null, 2)}`);
  }
  assertOnlyKeys(data, ['includedProduct'], 'conditional product read data');

  const includedProduct = readRecord(data['includedProduct']);
  if (!includedProduct) {
    throw new Error(`conditional product read missing includedProduct: ${JSON.stringify(data, null, 2)}`);
  }
  assertOnlyKeys(includedProduct, ['aliasStatus', 'id'], 'conditional product read includedProduct');
}

function assertSkippedMutationShape(entry: CaptureEntry): void {
  assertNoTopLevelErrors(entry, 'skipped productCreate mutation');
  assertOnlyKeys(payloadData(entry, 'skipped productCreate mutation'), [], 'skipped productCreate data');
}

async function runCaptureEntry(query: string, variables: JsonRecord, label: string): Promise<CaptureEntry> {
  const response = await capture.runGraphqlRequest<JsonRecord>(query, variables);
  const entry = {
    query,
    variables,
    response: {
      status: response.status,
      payload: response.payload,
    },
  };
  assertNoTopLevelErrors(entry, label);
  return entry;
}

async function cleanupProduct(productId: string | null): Promise<CaptureEntry | null> {
  if (productId === null) {
    return null;
  }
  const variables = { input: { id: productId } };
  const response = await capture.runGraphqlRequest<JsonRecord>(cleanupProductDeleteDocument, variables);
  return {
    query: cleanupProductDeleteDocument,
    variables,
    response: {
      status: response.status,
      payload: response.payload,
    },
  };
}

function productIdDifference(path: string): JsonRecord {
  return {
    path,
    matcher: 'shopify-gid:Product',
    reason: 'Shopify and the local staging registry allocate product ids independently.',
  };
}

function buildSpec(apiVersion: string, fixturePath: string): JsonRecord {
  const createDocumentPath = documentPath(createDocumentFile);
  const readDocumentPath = documentPath(readDocumentFile);
  const skippedCreateDocumentPath = documentPath(skippedCreateDocumentFile);

  return {
    scenarioId,
    operationNames: ['productCreate', 'product'],
    scenarioStatus: 'captured',
    assertionKinds: ['graphql-directive-parity', 'payload-shape', 'downstream-read-parity'],
    liveCaptureFiles: [fixturePath],
    runtimeTestFiles: [
      'tests/graphql_arguments.rs',
      'tests/graphql_routes/store_state.rs',
      'tests/graphql_routes/admin_graphql_webhooks.rs',
    ],
    proxyRequest: {
      documentPath: createDocumentPath,
      apiVersion,
      variablesCapturePath: '$.setup.create.variables',
    },
    comparisonMode: 'captured-vs-proxy-request',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [productIdDifference('$.productCreate.product.id')],
      targets: [
        {
          name: 'setup-product-create-data',
          capturePath: '$.setup.create.response.payload.data',
          proxyPath: '$.data',
        },
        {
          name: 'conditional-product-read-data',
          capturePath: '$.conditionalRead.response.payload.data',
          proxyRequest: {
            documentPath: readDocumentPath,
            apiVersion,
            variables: {
              id: { fromPrimaryProxyPath: '$.data.productCreate.product.id' },
              includeTitle: { fromCapturePath: '$.conditionalRead.variables.includeTitle' },
              skipInline: { fromCapturePath: '$.conditionalRead.variables.skipInline' },
              includeSpread: { fromCapturePath: '$.conditionalRead.variables.includeSpread' },
            },
          },
          proxyPath: '$.data',
          expectedDifferences: [productIdDifference('$.includedProduct.id')],
        },
        {
          name: 'skipped-product-create-empty-data',
          capturePath: '$.skippedCreate.response.payload.data',
          proxyRequest: {
            documentPath: skippedCreateDocumentPath,
            apiVersion,
            variablesCapturePath: '$.skippedCreate.variables',
          },
          proxyPath: '$.data',
        },
      ],
    },
    notes:
      'Captured live Admin GraphQL behavior for standard @skip and @include on product query roots, nested selected fields, inline fragments, named fragment spreads, and a skipped productCreate mutation root. The parity replay earns product state through the public productCreate request and compares response data only because Shopify cost extensions are volatile and not part of the local proxy contract.',
  };
}

const capture = await createConformanceCapture();
const createQuery = await capture.readRequestRaw(domain, createDocumentFile);
const readQuery = await capture.readRequestRaw(domain, readDocumentFile);
const skippedCreateQuery = await capture.readRequestRaw(domain, skippedCreateDocumentFile);

const createVariables = {
  product: {
    title: `Hermes Conditional Directive Product ${capture.stamp}`,
    status: 'DRAFT',
  },
};
const skippedCreateVariables = {
  product: {
    title: `Hermes Skipped Conditional Directive Product ${capture.stamp}`,
    status: 'DRAFT',
  },
  skipMutation: true,
};

let productId: string | null = null;
let createEntry: CaptureEntry | null = null;
let conditionalReadEntry: CaptureEntry | null = null;
let skippedCreateEntry: CaptureEntry | null = null;
let cleanupEntry: CaptureEntry | null = null;

try {
  createEntry = await runCaptureEntry(createQuery, createVariables, 'setup productCreate');
  assertNoUserErrors(createEntry, 'productCreate', 'setup productCreate');
  productId = productIdFromCreate(createEntry);

  conditionalReadEntry = await runCaptureEntry(
    readQuery,
    {
      id: productId,
      includeTitle: false,
      skipInline: true,
      includeSpread: true,
    },
    'conditional product read',
  );
  assertConditionalReadShape(conditionalReadEntry);

  skippedCreateEntry = await runCaptureEntry(skippedCreateQuery, skippedCreateVariables, 'skipped productCreate');
  assertSkippedMutationShape(skippedCreateEntry);
} finally {
  cleanupEntry = await cleanupProduct(productId);
}

if (!createEntry || !conditionalReadEntry || !skippedCreateEntry) {
  throw new Error('Capture did not complete all required conditional directive requests.');
}

const fixturePath = capture.fixturePath(domain, `${scenarioId}.json`);
const specPath = `config/parity-specs/${domain}/${scenarioId}.json`;

await capture.writeJson(fixturePath, {
  scenarioId,
  capturedAt: new Date().toISOString(),
  source: 'live-shopify-admin-graphql',
  storeDomain: capture.storeDomain,
  apiVersion: capture.apiVersion,
  liveGatewaySideEffects: true,
  notes:
    'Creates one disposable draft product, records conditional directive response projection, records a skipped productCreate root, and deletes the setup product in best-effort cleanup.',
  setup: {
    create: createEntry,
  },
  conditionalRead: conditionalReadEntry,
  skippedCreate: skippedCreateEntry,
  upstreamCalls: [],
  cleanup: cleanupEntry,
});

await capture.writeJson(specPath, buildSpec(capture.apiVersion, fixturePath));

console.log(
  JSON.stringify(
    {
      ok: true,
      scenarioId,
      fixturePath,
      specPath,
      cleanupRecorded: cleanupEntry !== null,
    },
    null,
    2,
  ),
);

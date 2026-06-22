/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
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

const productsDir = path.join('config', 'parity-requests', 'products');
const specsDir = path.join('config', 'parity-specs', 'products');
const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');

const documentPath = path.join(productsDir, 'collectionCreate-rejects-id.graphql');
const specPath = path.join(specsDir, 'collectionCreate-rejects-id.json');
const fixturePath = path.join(fixtureDir, 'collection-create-rejects-id.json');

const document = `mutation CollectionCreateRejectsId($input: CollectionInput!) {
  collectionCreate(input: $input) {
    collection {
      id
      title
    }
    userErrors {
      field
      message
    }
  }
}
`;

const variables = {
  input: {
    id: 'gid://shopify/Collection/123',
    title: 'Reject ID Collection',
  },
};

const expectedUserErrors = [
  {
    field: ['id'],
    message: 'id cannot be specified on collection creation',
  },
];

function readRecord(value: unknown): JsonRecord | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readPath(value: unknown, pathSegments: readonly string[]): unknown {
  let current: unknown = value;
  for (const segment of pathSegments) {
    if (Array.isArray(current)) {
      current = current[Number(segment)];
      continue;
    }
    const record = readRecord(current);
    if (!record) {
      return undefined;
    }
    current = record[segment];
  }
  return current;
}

function assertJsonEqual(actual: unknown, expected: unknown, label: string): void {
  const actualJson = JSON.stringify(actual);
  const expectedJson = JSON.stringify(expected);
  if (actualJson !== expectedJson) {
    throw new Error(`${label} mismatch: expected ${expectedJson}, received ${actualJson}`);
  }
}

const response = await runGraphqlRequest(document, variables);
if (response.status < 200 || response.status >= 300) {
  throw new Error(
    `collectionCreate input.id capture failed with HTTP ${response.status}: ${JSON.stringify(response.payload)}`,
  );
}

assertJsonEqual(
  readPath(response.payload, ['data', 'collectionCreate', 'collection']),
  null,
  'collectionCreate.collection',
);
assertJsonEqual(
  readPath(response.payload, ['data', 'collectionCreate', 'userErrors']),
  expectedUserErrors,
  'collectionCreate.userErrors',
);

await mkdir(productsDir, { recursive: true });
await mkdir(specsDir, { recursive: true });
await mkdir(fixtureDir, { recursive: true });

await writeFile(documentPath, document);
await writeFile(
  fixturePath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      apiVersion,
      storeDomain,
      scenarios: {
        inputId: {
          document,
          variables,
          response: response.payload,
        },
      },
      notes: [
        'Live public Admin GraphQL 2026-04 rejects collectionCreate with caller-supplied CollectionInput.id before creating a collection.',
        'The mutation payload returns collection: null and a generic UserError with field ["id"].',
      ],
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
);
await writeFile(
  specPath,
  `${JSON.stringify(
    {
      scenarioId: 'collection-create-rejects-id',
      operationNames: ['collectionCreate'],
      scenarioStatus: 'captured',
      assertionKinds: ['payload-shape', 'user-errors-parity', 'validation-parity', 'state-invariance'],
      liveCaptureFiles: [fixturePath],
      proxyRequest: {
        documentPath,
        variablesCapturePath: '$.scenarios.inputId.variables',
      },
      comparisonMode: 'captured-vs-proxy-request',
      notes:
        'Live public Admin GraphQL 2026-04 rejects caller-supplied CollectionInput.id on collectionCreate with field ["id"] and message "id cannot be specified on collection creation"; the local runtime must return collection: null and avoid staging a collection.',
      comparison: {
        mode: 'strict-json',
        expectedDifferences: [],
        targets: [
          {
            name: 'collection-create-input-id-user-error',
            capturePath: '$.scenarios.inputId.response.data.collectionCreate',
            proxyPath: '$.data.collectionCreate',
          },
        ],
      },
    },
    null,
    2,
  )}\n`,
);

console.log(`Wrote ${documentPath}`);
console.log(`Wrote ${fixturePath}`);
console.log(`Wrote ${specPath}`);

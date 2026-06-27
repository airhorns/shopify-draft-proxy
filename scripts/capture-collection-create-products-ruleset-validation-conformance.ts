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

const documentPath = path.join(productsDir, 'collectionCreate-products-ruleset-validation.graphql');
const unknownProductsSpecPath = path.join(specsDir, 'collectionCreate-unknown-products.json');
const emptyRulesSpecPath = path.join(specsDir, 'collectionCreate-empty-ruleset-rules.json');
const fixturePath = path.join(fixtureDir, 'collection-create-products-ruleset-validation.json');

const document = `mutation CollectionCreateProductsRuleSetValidation($input: CollectionInput!) {
  collectionCreate(input: $input) {
    collection {
      id
      title
      products(first: 10) {
        nodes {
          id
        }
      }
      ruleSet {
        appliedDisjunctively
        rules {
          column
          relation
          condition
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

const cleanupDocument = `mutation CleanupCollection($input: CollectionDeleteInput!) {
  collectionDelete(input: $input) {
    deletedCollectionId
    userErrors {
      field
      message
    }
  }
}
`;

const scenarios = {
  unknownProducts: {
    variables: {
      input: {
        title: 'Reject Unknown Product Collection',
        products: ['gid://shopify/Product/999999999999999'],
      },
    },
    expectedUserErrors: [
      {
        field: ['products', '0'],
        message: 'Product does not exist',
      },
    ],
  },
  emptyRuleSetRules: {
    variables: {
      input: {
        title: 'Reject Empty Rule Set Collection',
        ruleSet: {
          appliedDisjunctively: false,
          rules: [],
        },
      },
    },
    expectedUserErrors: [],
  },
  missingRuleSetRules: {
    variables: {
      input: {
        title: 'Reject Missing Rule Set Rules Collection',
        ruleSet: {
          appliedDisjunctively: false,
        },
      },
    },
    expectedUserErrors: [
      {
        field: ['ruleSet', 'rules'],
        message: 'Rules cannot be an empty set',
      },
    ],
  },
} as const;

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

async function captureScenario(name: keyof typeof scenarios): Promise<JsonRecord> {
  const scenario = scenarios[name];
  const response = await runGraphqlRequest(document, scenario.variables);
  if (response.status < 200 || response.status >= 300) {
    throw new Error(`${name} capture failed with HTTP ${response.status}: ${JSON.stringify(response.payload)}`);
  }

  const result: JsonRecord = {
    document,
    variables: scenario.variables,
    response: response.payload,
  };

  if (name === 'emptyRuleSetRules') {
    const collection = readPath(response.payload, ['data', 'collectionCreate', 'collection']);
    const collectionRecord = readRecord(collection);
    if (!collectionRecord) {
      throw new Error(`${name}.collectionCreate.collection should be an object: ${JSON.stringify(collection)}`);
    }
    assertJsonEqual(
      readPath(response.payload, ['data', 'collectionCreate', 'collection', 'title']),
      'Reject Empty Rule Set Collection',
      `${name}.collectionCreate.collection.title`,
    );
    assertJsonEqual(
      readPath(response.payload, ['data', 'collectionCreate', 'collection', 'products', 'nodes']),
      [],
      `${name}.collectionCreate.collection.products.nodes`,
    );
    assertJsonEqual(
      readPath(response.payload, ['data', 'collectionCreate', 'collection', 'ruleSet']),
      null,
      `${name}.collectionCreate.collection.ruleSet`,
    );
  } else {
    assertJsonEqual(
      readPath(response.payload, ['data', 'collectionCreate', 'collection']),
      null,
      `${name}.collectionCreate.collection`,
    );
  }
  assertJsonEqual(
    readPath(response.payload, ['data', 'collectionCreate', 'userErrors']),
    scenario.expectedUserErrors,
    `${name}.collectionCreate.userErrors`,
  );

  const createdCollectionId = readPath(response.payload, ['data', 'collectionCreate', 'collection', 'id']);
  if (typeof createdCollectionId === 'string') {
    const cleanup = await runGraphqlRequest(cleanupDocument, { input: { id: createdCollectionId } });
    if (cleanup.status < 200 || cleanup.status >= 300) {
      throw new Error(`${name} cleanup failed with HTTP ${cleanup.status}: ${JSON.stringify(cleanup.payload)}`);
    }
    assertJsonEqual(
      readPath(cleanup.payload, ['data', 'collectionDelete', 'userErrors']),
      [],
      `${name}.cleanup.collectionDelete.userErrors`,
    );
    result['cleanup'] = {
      document: cleanupDocument,
      variables: { input: { id: createdCollectionId } },
      response: cleanup.payload,
    };
  }

  return result;
}

const capturedScenarios = {
  unknownProducts: await captureScenario('unknownProducts'),
  emptyRuleSetRules: await captureScenario('emptyRuleSetRules'),
  missingRuleSetRules: await captureScenario('missingRuleSetRules'),
};

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
      scenarios: capturedScenarios,
      notes: [
        'Live public Admin GraphQL rejects collectionCreate with unknown input.products before creating a collection.',
        'Live public Admin GraphQL accepts collectionCreate with present ruleSet and an empty rules list as a custom collection with ruleSet: null.',
        'Live public Admin GraphQL rejects collectionCreate with present ruleSet and omitted rules before creating a collection.',
        'The validation failure payloads return collection: null and generic UserError objects with field/message only.',
      ],
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
);
await writeFile(
  unknownProductsSpecPath,
  `${JSON.stringify(
    {
      scenarioId: 'collectionCreate-unknown-products',
      operationNames: ['collectionCreate'],
      scenarioStatus: 'captured',
      assertionKinds: ['payload-shape', 'user-errors-parity', 'validation-parity', 'state-invariance'],
      liveCaptureFiles: [fixturePath],
      proxyRequest: {
        documentPath,
        variablesCapturePath: '$.scenarios.unknownProducts.variables',
      },
      comparisonMode: 'captured-vs-proxy-request',
      notes:
        'Live public Admin GraphQL rejects collectionCreate with an unknown input.products id using field ["products", "0"] and message "Product does not exist"; the local runtime must return collection: null and avoid staging a collection.',
      comparison: {
        mode: 'strict-json',
        expectedDifferences: [],
        targets: [
          {
            name: 'collection-create-unknown-products-user-error',
            capturePath: '$.scenarios.unknownProducts.response.data.collectionCreate',
            proxyPath: '$.data.collectionCreate',
          },
        ],
      },
    },
    null,
    2,
  )}\n`,
);
await writeFile(
  emptyRulesSpecPath,
  `${JSON.stringify(
    {
      scenarioId: 'collectionCreate-empty-ruleset-rules',
      operationNames: ['collectionCreate'],
      scenarioStatus: 'captured',
      assertionKinds: ['payload-shape', 'user-errors-parity', 'validation-parity', 'state-invariance'],
      liveCaptureFiles: [fixturePath],
      proxyRequest: {
        documentPath,
        variablesCapturePath: '$.scenarios.emptyRuleSetRules.variables',
      },
      comparisonMode: 'captured-vs-proxy-request',
      notes:
        'Live public Admin GraphQL accepts collectionCreate when ruleSet.rules is an empty list as a custom collection with ruleSet: null, but rejects omitted ruleSet.rules using field ["ruleSet", "rules"] and message "Rules cannot be an empty set"; the local runtime must match both branches.',
      comparison: {
        mode: 'strict-json',
        expectedDifferences: [],
        targets: [
          {
            name: 'collection-create-empty-ruleset-rules-custom-success',
            capturePath: '$.scenarios.emptyRuleSetRules.response.data',
            proxyPath: '$.data',
            expectedDifferences: [
              {
                path: '$.collectionCreate.collection.id',
                matcher: 'shopify-gid:Collection',
                reason: 'Shopify and the local parity harness allocate collection identifiers independently.',
              },
            ],
          },
          {
            name: 'collection-create-missing-ruleset-rules-user-error',
            capturePath: '$.scenarios.missingRuleSetRules.response.data.collectionCreate',
            proxyRequest: {
              documentPath,
              variablesCapturePath: '$.scenarios.missingRuleSetRules.variables',
            },
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
console.log(`Wrote ${unknownProductsSpecPath}`);
console.log(`Wrote ${emptyRulesSpecPath}`);

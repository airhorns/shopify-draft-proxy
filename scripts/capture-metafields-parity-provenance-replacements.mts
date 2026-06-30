/* oxlint-disable no-console -- CLI capture scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type GraphqlCapture = {
  request: {
    query: string;
    variables: Record<string, unknown>;
  };
  status: number;
  response: ConformanceGraphqlResult['payload'];
};

type CaptureClient = {
  apiVersion: string;
  runGraphqlRaw: (query: string, variables?: Record<string, unknown>) => Promise<ConformanceGraphqlResult>;
};

const inputValidationApiVersion = '2025-01';
const standardEnableApiVersion = '2026-04';
const metafieldsRequestDir = path.join('config', 'parity-requests', 'metafields');
const metafieldsSpecDir = path.join('config', 'parity-specs', 'metafields');

const { storeDomain, adminOrigin } = readConformanceScriptConfig({
  defaultApiVersion: inputValidationApiVersion,
  exitOnMissing: true,
});

async function createClient(apiVersion: string): Promise<CaptureClient> {
  const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
  const { runGraphqlRaw } = createAdminGraphqlClient({
    adminOrigin,
    apiVersion,
    headers: buildAdminAuthHeaders(adminAccessToken),
  });
  return { apiVersion, runGraphqlRaw };
}

function fixturePath(apiVersion: string, fileName: string): string {
  return path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metafields', fileName);
}

async function readMetafieldsRequest(fileName: string): Promise<string> {
  return readFile(path.join(metafieldsRequestDir, fileName), 'utf8');
}

async function writeJson(filePath: string, payload: unknown): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${JSON.stringify(payload, null, 2)}\n`);
}

async function capture(
  client: CaptureClient,
  query: string,
  variables: Record<string, unknown> = {},
): Promise<GraphqlCapture> {
  const result = await client.runGraphqlRaw(query, variables);
  return {
    request: { query, variables },
    status: result.status,
    response: result.payload,
  };
}

function readObject(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as Record<string, unknown>) : null;
}

function readPath(value: unknown, pathParts: string[]): unknown {
  let current = value;
  for (const part of pathParts) {
    const object = readObject(current);
    if (!object) return undefined;
    current = object[part];
  }
  return current;
}

function createdDefinitionId(captureResult: GraphqlCapture, rootName: string): string | undefined {
  const id = readPath(captureResult.response, ['data', rootName, 'createdDefinition', 'id']);
  return typeof id === 'string' ? id : undefined;
}

async function cleanupDefinition(client: CaptureClient, id: string): Promise<GraphqlCapture> {
  return capture(
    client,
    `#graphql
      mutation CleanupMetafieldDefinitionProvenanceReplacement($id: ID!) {
        metafieldDefinitionDelete(id: $id, deleteAllAssociatedMetafields: true) {
          deletedDefinitionId
          userErrors {
            field
            message
            code
          }
        }
      }
    `,
    { id },
  );
}

async function captureMetafieldDefinitionCreateInputValidation(): Promise<void> {
  const client = await createClient(inputValidationApiVersion);
  const document = await readMetafieldsRequest('metafield-definition-create-input-validation.graphql');
  const outputPath = fixturePath(inputValidationApiVersion, 'metafield-definition-create-input-validation.json');
  const paritySpecPath = path.join(metafieldsSpecDir, 'metafield-definition-create-input-validation.json');
  const cases: Record<string, Record<string, unknown>> = {
    shortNamespace: {
      definition: {
        namespace: 'ab',
        key: 'x',
        ownerType: 'PRODUCT',
        name: 'X',
        type: 'single_line_text_field',
      },
    },
    invalidCharacterNamespace: {
      definition: {
        namespace: 'my space',
        key: 'valid_key',
        ownerType: 'PRODUCT',
        name: 'X',
        type: 'single_line_text_field',
      },
    },
    invalidCharacterKey: {
      definition: {
        namespace: 'loyalty',
        key: 'bad.key!',
        ownerType: 'PRODUCT',
        name: 'X',
        type: 'single_line_text_field',
      },
    },
    unknownType: {
      definition: {
        namespace: 'loyalty',
        key: 'tier',
        ownerType: 'PRODUCT',
        name: 'Tier',
        type: 'totally_made_up_type',
      },
    },
    reservedNamespaceShopifyStandard: {
      definition: {
        namespace: 'shopify_standard',
        key: 'xx',
        ownerType: 'PRODUCT',
        name: 'X',
        type: 'single_line_text_field',
      },
    },
    reservedNamespaceProtected: {
      definition: {
        namespace: 'protected',
        key: 'xx',
        ownerType: 'PRODUCT',
        name: 'X',
        type: 'single_line_text_field',
      },
    },
    nameTooLong: {
      definition: {
        namespace: 'loyalty',
        key: 'longname',
        ownerType: 'PRODUCT',
        name: 'N'.repeat(256),
        type: 'single_line_text_field',
      },
    },
  };
  const capturedCases: Record<string, GraphqlCapture> = {};
  const cleanup: GraphqlCapture[] = [];

  for (const [name, variables] of Object.entries(cases)) {
    const captured = await capture(client, document, variables);
    capturedCases[name] = captured;
    const id = createdDefinitionId(captured, 'metafieldDefinitionCreate');
    if (id) cleanup.push(await cleanupDefinition(client, id));
  }

  await writeJson(outputPath, {
    scenarioId: 'metafield-definition-create-input-validation',
    storeDomain,
    apiVersion: client.apiVersion,
    capturedAt: new Date().toISOString(),
    cases: capturedCases,
    cleanup,
    upstreamCalls: [],
    notes:
      'Live Shopify Admin API 2025-01 metafieldDefinitionCreate input-validation capture. The current conformance app is permitted to create `shopify_standard` and `protected` namespace definitions, so those branches are recorded as successful creates and cleaned up immediately.',
  });

  await writeJson(paritySpecPath, {
    scenarioId: 'metafield-definition-create-input-validation',
    operationNames: ['metafieldDefinitionCreate'],
    scenarioStatus: 'captured',
    assertionKinds: ['user-errors-parity', 'input-validation', 'payload-shape'],
    liveCaptureFiles: [outputPath],
    runtimeTestFiles: ['tests/graphql_routes/metafield_definitions.rs'],
    proxyRequest: {
      documentPath: 'config/parity-requests/metafields/metafield-definition-create-input-validation.graphql',
      variablesCapturePath: '$.cases.shortNamespace.request.variables',
      apiVersion: client.apiVersion,
    },
    comparisonMode: 'captured-vs-proxy-request',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        {
          name: 'short-namespace-userErrors',
          capturePath: '$.cases.shortNamespace.response.data.metafieldDefinitionCreate',
          proxyPath: '$.data.metafieldDefinitionCreate',
        },
        {
          name: 'invalid-character-namespace-userErrors',
          capturePath: '$.cases.invalidCharacterNamespace.response.data.metafieldDefinitionCreate',
          proxyPath: '$.data.metafieldDefinitionCreate',
          proxyRequest: {
            documentPath: 'config/parity-requests/metafields/metafield-definition-create-input-validation.graphql',
            variablesCapturePath: '$.cases.invalidCharacterNamespace.request.variables',
            apiVersion: client.apiVersion,
          },
        },
        {
          name: 'invalid-character-key-userErrors',
          capturePath: '$.cases.invalidCharacterKey.response.data.metafieldDefinitionCreate',
          proxyPath: '$.data.metafieldDefinitionCreate',
          proxyRequest: {
            documentPath: 'config/parity-requests/metafields/metafield-definition-create-input-validation.graphql',
            variablesCapturePath: '$.cases.invalidCharacterKey.request.variables',
            apiVersion: client.apiVersion,
          },
        },
        {
          name: 'unknown-type-userErrors',
          capturePath: '$.cases.unknownType.response.data.metafieldDefinitionCreate',
          proxyPath: '$.data.metafieldDefinitionCreate',
          proxyRequest: {
            documentPath: 'config/parity-requests/metafields/metafield-definition-create-input-validation.graphql',
            variablesCapturePath: '$.cases.unknownType.request.variables',
            apiVersion: client.apiVersion,
          },
        },
        {
          name: 'shopify-standard-namespace-create',
          capturePath: '$.cases.reservedNamespaceShopifyStandard.response.data.metafieldDefinitionCreate',
          proxyPath: '$.data.metafieldDefinitionCreate',
          proxyRequest: {
            documentPath: 'config/parity-requests/metafields/metafield-definition-create-input-validation.graphql',
            variablesCapturePath: '$.cases.reservedNamespaceShopifyStandard.request.variables',
            apiVersion: client.apiVersion,
          },
          expectedDifferences: [
            {
              path: '$.createdDefinition.id',
              matcher: 'shopify-gid:MetafieldDefinition',
              reason:
                'The proxy stages a deterministic synthetic definition ID while Shopify returned the live-store definition ID.',
            },
          ],
        },
        {
          name: 'protected-namespace-create',
          capturePath: '$.cases.reservedNamespaceProtected.response.data.metafieldDefinitionCreate',
          proxyPath: '$.data.metafieldDefinitionCreate',
          proxyRequest: {
            documentPath: 'config/parity-requests/metafields/metafield-definition-create-input-validation.graphql',
            variablesCapturePath: '$.cases.reservedNamespaceProtected.request.variables',
            apiVersion: client.apiVersion,
          },
          expectedDifferences: [
            {
              path: '$.createdDefinition.id',
              matcher: 'shopify-gid:MetafieldDefinition',
              reason:
                'The proxy stages a deterministic synthetic definition ID while Shopify returned the live-store definition ID.',
            },
          ],
        },
        {
          name: 'name-too-long-userErrors',
          capturePath: '$.cases.nameTooLong.response.data.metafieldDefinitionCreate',
          proxyPath: '$.data.metafieldDefinitionCreate',
          proxyRequest: {
            documentPath: 'config/parity-requests/metafields/metafield-definition-create-input-validation.graphql',
            variablesCapturePath: '$.cases.nameTooLong.request.variables',
            apiVersion: client.apiVersion,
          },
        },
      ],
    },
    notes:
      'Live Shopify Admin API 2025-01 input validation for metafieldDefinitionCreate. Unlike the retired synthetic fixture, the current conformance app is allowed to create `shopify_standard` and `protected` namespace definitions; the recorder deletes those successful validation-probe creates during cleanup.',
  });

  console.log(`Wrote ${outputPath}`);
  console.log(`Wrote ${paritySpecPath}`);
}

async function captureStandardMetafieldDefinitionEnableBranches(): Promise<void> {
  const client = await createClient(standardEnableApiVersion);
  const outputPath = fixturePath(standardEnableApiVersion, 'standard-metafield-definition-enable-error-branches.json');
  const paritySpecPath = path.join(metafieldsSpecDir, 'standard-metafield-definition-enable-error-branches.json');
  const requestPaths = {
    smartCapability: 'standard-metafield-definition-enable-smart-capability.graphql',
    uniqueCapability: 'standard-metafield-definition-enable-unique-capability.graphql',
    adminAccess: 'standard-metafield-definition-enable-admin-access.graphql',
    visibleStorefront: 'standard-metafield-definition-enable-visible-storefront.graphql',
    collectionCondition: 'standard-metafield-definition-enable-deprecated-condition.graphql',
    forceEnable: 'standard-metafield-definition-enable-force-false.graphql',
    adminFilter: 'standard-metafield-definition-enable-deprecated-access-filter.graphql',
  };
  const documents = Object.fromEntries(
    await Promise.all(
      Object.entries(requestPaths).map(async ([key, fileName]) => [key, await readMetafieldsRequest(fileName)]),
    ),
  ) as Record<keyof typeof requestPaths, string>;
  const beforeVisible = await capture(
    client,
    `#graphql
      query ReadVisibleStorefrontProbeDefinition {
        metafieldDefinition(identifier: { ownerType: PRODUCT, namespace: "facts", key: "isbn" }) {
          id
          namespace
          key
        }
      }
    `,
  );
  const captures = {
    invalidSmartCollectionCapability: await capture(client, documents.smartCapability),
    invalidUniqueValuesCapability: await capture(client, documents.uniqueCapability),
    adminAccessInputNotAllowed: await capture(client, documents.adminAccess),
    visibleStorefrontTranslation: await capture(client, documents.visibleStorefront),
    deprecatedCollectionConditionHiddenArgument: await capture(client, documents.collectionCondition),
  };
  const hardAttempts = {
    forceEnableSchemaRejection: await capture(client, documents.forceEnable),
    adminFilterSchemaRejection: await capture(client, documents.adminFilter),
  };
  const afterVisible = await capture(
    client,
    `#graphql
      query ReadVisibleStorefrontProbeDefinitionAfter {
        metafieldDefinition(identifier: { ownerType: PRODUCT, namespace: "facts", key: "isbn" }) {
          id
          namespace
          key
        }
      }
    `,
  );
  const beforeVisibleId = readPath(beforeVisible.response, ['data', 'metafieldDefinition', 'id']);
  const afterVisibleId = readPath(afterVisible.response, ['data', 'metafieldDefinition', 'id']);
  const cleanup: GraphqlCapture[] = [];
  if (typeof beforeVisibleId !== 'string' && typeof afterVisibleId === 'string') {
    cleanup.push(await cleanupDefinition(client, afterVisibleId));
  }

  await writeJson(outputPath, {
    scenarioId: 'standard-metafield-definition-enable-error-branches',
    storeDomain,
    apiVersion: client.apiVersion,
    capturedAt: new Date().toISOString(),
    captures,
    hardAttempts,
    cleanup,
    upstreamCalls: [],
    notes: {
      publicSchema:
        'Admin API 2026-04 introspection lists ownerType, id, namespace, key, pin, capabilities, and access for standardMetafieldDefinitionEnable. Live execution still accepts hidden visibleToStorefrontApi and useAsCollectionCondition arguments, but forceEnable and useAsAdminFilter are rejected by public schema validation.',
      cleanup:
        'The visibleToStorefrontApi probe can create the PRODUCT facts/isbn standard definition. The recorder reads before and after the probe and deletes the definition only when this capture created it.',
    },
  });

  await writeJson(paritySpecPath, {
    scenarioId: 'standard-metafield-definition-enable-error-branches',
    operationNames: ['standardMetafieldDefinitionEnable'],
    scenarioStatus: 'captured',
    assertionKinds: ['payload-shape', 'user-errors-parity', 'hidden-argument-parity'],
    liveCaptureFiles: [outputPath],
    runtimeTestFiles: ['tests/graphql_routes/metafield_definitions.rs'],
    proxyRequest: {
      documentPath: 'config/parity-requests/metafields/standard-metafield-definition-enable-smart-capability.graphql',
      apiVersion: client.apiVersion,
    },
    comparisonMode: 'captured-vs-proxy-request',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        {
          name: 'invalid-smart-collection-capability',
          capturePath: '$.captures.invalidSmartCollectionCapability.response.data.standardMetafieldDefinitionEnable',
          proxyPath: '$.data.standardMetafieldDefinitionEnable',
        },
        {
          name: 'invalid-unique-values-capability',
          capturePath: '$.captures.invalidUniqueValuesCapability.response.data.standardMetafieldDefinitionEnable',
          proxyPath: '$.data.standardMetafieldDefinitionEnable',
          proxyRequest: {
            documentPath:
              'config/parity-requests/metafields/standard-metafield-definition-enable-unique-capability.graphql',
            apiVersion: client.apiVersion,
          },
        },
        {
          name: 'admin-access-input-not-allowed',
          capturePath: '$.captures.adminAccessInputNotAllowed.response.data.standardMetafieldDefinitionEnable',
          proxyPath: '$.data.standardMetafieldDefinitionEnable',
          proxyRequest: {
            documentPath: 'config/parity-requests/metafields/standard-metafield-definition-enable-admin-access.graphql',
            apiVersion: client.apiVersion,
          },
        },
        {
          name: 'visible-storefront-translation',
          capturePath: '$.captures.visibleStorefrontTranslation.response.data.standardMetafieldDefinitionEnable',
          proxyPath: '$.data.standardMetafieldDefinitionEnable',
          proxyRequest: {
            documentPath:
              'config/parity-requests/metafields/standard-metafield-definition-enable-visible-storefront.graphql',
            apiVersion: client.apiVersion,
          },
        },
        {
          name: 'deprecated-collection-condition-hidden-argument',
          capturePath:
            '$.captures.deprecatedCollectionConditionHiddenArgument.response.data.standardMetafieldDefinitionEnable',
          proxyPath: '$.data.standardMetafieldDefinitionEnable',
          proxyRequest: {
            documentPath:
              'config/parity-requests/metafields/standard-metafield-definition-enable-deprecated-condition.graphql',
            apiVersion: client.apiVersion,
          },
        },
      ],
    },
    notes:
      'Live Shopify Admin API 2026-04 error-branch capture for standardMetafieldDefinitionEnable. The retired synthetic fixture claimed payload parity for forceEnable and useAsAdminFilter paths that the public schema rejects; those exact hard-attempt responses are preserved in the fixture but are not strict payload targets.',
  });

  console.log(`Wrote ${outputPath}`);
  console.log(`Wrote ${paritySpecPath}`);
}

await captureMetafieldDefinitionCreateInputValidation();
await captureStandardMetafieldDefinitionEnableBranches();

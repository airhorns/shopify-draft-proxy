/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metafields');
const outputPath = path.join(outputDir, 'metafield-definition-automated-collection-dependency.json');
const requestRoot = 'config/parity-requests/metafields/metafield-definition-automated-collection-dependency';
const productCreateDocumentPath = `${requestRoot}-product-create.graphql`;
const definitionCreateDocumentPath = `${requestRoot}-definition-create.graphql`;
const metafieldsSetDocumentPath = `${requestRoot}-metafields-set.graphql`;
const collectionCreateDocumentPath = `${requestRoot}-collection-create.graphql`;
const deleteDocumentPath = `${requestRoot}-delete.graphql`;
const readDocumentPath = `${requestRoot}-read.graphql`;

const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const collectionDeleteMutation = `#graphql
  mutation MetafieldDefinitionAutomatedCollectionCleanupCollection($input: CollectionDeleteInput!) {
    collectionDelete(input: $input) {
      deletedCollectionId
      userErrors { field message }
    }
  }
`;

const definitionDeleteMutation = `#graphql
  mutation MetafieldDefinitionAutomatedCollectionCleanupDefinition($id: ID!) {
    metafieldDefinitionDelete(id: $id, deleteAllAssociatedMetafields: true) {
      deletedDefinitionId
      userErrors { field message code }
    }
  }
`;

const productDeleteMutation = `#graphql
  mutation MetafieldDefinitionAutomatedCollectionCleanupProduct($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors { field message }
    }
  }
`;

type CapturedInteraction = {
  request: {
    documentPath?: string;
    query?: string;
    variables: Record<string, unknown>;
  };
  status: number;
  response: unknown;
};

type UpstreamCall = {
  method: 'POST';
  apiSurface: 'admin';
  path: string;
  operationName: string;
  variables: Record<string, unknown>;
  query: string;
  response: { status: number; body: unknown };
};

function assertHttpOk(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function readObject(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as Record<string, unknown>) : null;
}

function readPath(value: unknown, pathParts: string[]): unknown {
  let current: unknown = value;
  for (const part of pathParts) {
    current = readObject(current)?.[part];
  }
  return current;
}

function requiredId(capture: CapturedInteraction, pathParts: string[], label: string): string {
  const id = readPath(capture.response, pathParts);
  if (typeof id !== 'string') {
    throw new Error(`${label} did not return an id: ${JSON.stringify(capture.response)}`);
  }
  return id;
}

async function captureDocument(
  label: string,
  documentPath: string,
  variables: Record<string, unknown>,
): Promise<CapturedInteraction> {
  const query = await readFile(documentPath, 'utf8');
  const result = await runGraphqlRaw(query, variables);
  assertHttpOk(result, label);
  return {
    request: { documentPath, variables },
    status: result.status,
    response: result.payload,
  };
}

async function captureQuery(
  label: string,
  query: string,
  variables: Record<string, unknown>,
): Promise<CapturedInteraction> {
  const result = await runGraphqlRaw(query, variables);
  assertHttpOk(result, label);
  return {
    request: { query, variables },
    status: result.status,
    response: result.payload,
  };
}

const hydrateByIdentifierDocument = await readFile(
  'src/runtime_graphql/metafields/metafield-definition-hydrate-by-identifier.graphql',
  'utf8',
);
const hydrateResourceScopeDocument = await readFile(
  'src/runtime_graphql/metafields/metafield-definitions-hydrate-resource-scope.graphql',
  'utf8',
);
const upstreamCalls: UpstreamCall[] = [];

async function recordUpstreamCall(
  operationName: string,
  query: string,
  variables: Record<string, unknown>,
): Promise<UpstreamCall> {
  const result = await runGraphqlRaw(query, variables);
  assertHttpOk(result, operationName);
  return {
    method: 'POST',
    apiSurface: 'admin',
    path: `/admin/api/${apiVersion}/graphql.json`,
    operationName,
    variables,
    query,
    response: { status: result.status, body: result.payload },
  };
}

async function recordResourceScope(): Promise<void> {
  let after: string | null = null;
  for (let page = 0; page < 3; page += 1) {
    const call = await recordUpstreamCall('MetafieldDefinitionsHydrateResourceScope', hydrateResourceScopeDocument, {
      ownerType: 'PRODUCT',
      query: '-namespace:app--*',
      first: 250,
      after,
    });
    upstreamCalls.push(call);
    const pageInfo = readObject(readPath(call.response.body, ['data', 'metafieldDefinitions', 'pageInfo']));
    if (pageInfo?.['hasNextPage'] !== true) return;
    const endCursor = pageInfo['endCursor'];
    if (typeof endCursor !== 'string') throw new Error(`resource-scope page ${page + 1} omitted endCursor`);
    after = endCursor;
  }
}

const suffix = `definition_collection_dependency_${Date.now().toString(36)}`;
const namespace = suffix;
const key = 'shade';
let productId: string | undefined;
let definitionId: string | undefined;
let collectionId: string | undefined;
const cleanup: CapturedInteraction[] = [];

let productCreate: CapturedInteraction;
let definitionCreate: CapturedInteraction;
let metafieldsSet: CapturedInteraction;
let collectionCreate: CapturedInteraction;
let readBeforeDelete: CapturedInteraction;
let deleteFalse: CapturedInteraction;
let readAfterFalse: CapturedInteraction;
let deleteTrue: CapturedInteraction;
let readAfterTrue: CapturedInteraction;

try {
  productCreate = await captureDocument('productCreate', productCreateDocumentPath, {
    product: { title: `Metafield definition collection dependency ${suffix}` },
  });
  productId = requiredId(productCreate, ['data', 'productCreate', 'product', 'id'], 'productCreate');

  upstreamCalls.push(
    await recordUpstreamCall('MetafieldDefinitionHydrateByIdentifier', hydrateByIdentifierDocument, {
      identifier: { ownerType: 'PRODUCT', namespace, key },
    }),
  );
  await recordResourceScope();

  definitionCreate = await captureDocument('metafieldDefinitionCreate', definitionCreateDocumentPath, {
    definition: {
      name: `Collection dependency ${suffix}`,
      namespace,
      key,
      ownerType: 'PRODUCT',
      type: 'single_line_text_field',
      capabilities: { smartCollectionCondition: { enabled: true } },
    },
  });
  definitionId = requiredId(
    definitionCreate,
    ['data', 'metafieldDefinitionCreate', 'createdDefinition', 'id'],
    'metafieldDefinitionCreate',
  );

  metafieldsSet = await captureDocument('metafieldsSet', metafieldsSetDocumentPath, {
    metafields: [
      {
        ownerId: productId,
        namespace,
        key,
        type: 'single_line_text_field',
        value: 'blue',
      },
    ],
  });

  collectionCreate = await captureDocument('collectionCreate', collectionCreateDocumentPath, {
    input: {
      title: `Metafield definition collection dependency ${suffix}`,
      ruleSet: {
        appliedDisjunctively: false,
        rules: [
          {
            column: 'PRODUCT_METAFIELD_DEFINITION',
            relation: 'EQUALS',
            condition: 'blue',
            conditionObjectId: definitionId,
          },
        ],
      },
    },
  });
  collectionId = requiredId(collectionCreate, ['data', 'collectionCreate', 'collection', 'id'], 'collectionCreate');

  const readVariables = { definitionId, productId, collectionId, namespace, key };
  readBeforeDelete = await captureDocument('read before delete', readDocumentPath, readVariables);
  deleteFalse = await captureDocument('delete with false flag', deleteDocumentPath, {
    id: definitionId,
    deleteAllAssociatedMetafields: false,
  });
  readAfterFalse = await captureDocument('read after false flag', readDocumentPath, readVariables);
  deleteTrue = await captureDocument('delete with true flag', deleteDocumentPath, {
    id: definitionId,
    deleteAllAssociatedMetafields: true,
  });
  readAfterTrue = await captureDocument('read after true flag', readDocumentPath, readVariables);
} finally {
  if (collectionId) {
    cleanup.push(
      await captureQuery('cleanup collectionDelete', collectionDeleteMutation, { input: { id: collectionId } }).catch(
        (error: unknown) => ({
          request: { query: collectionDeleteMutation, variables: { input: { id: collectionId } } },
          status: 0,
          response: { error: String(error) },
        }),
      ),
    );
  }
  if (definitionId) {
    cleanup.push(
      await captureQuery('cleanup metafieldDefinitionDelete', definitionDeleteMutation, { id: definitionId }).catch(
        (error: unknown) => ({
          request: { query: definitionDeleteMutation, variables: { id: definitionId } },
          status: 0,
          response: { error: String(error) },
        }),
      ),
    );
  }
  if (productId) {
    cleanup.push(
      await captureQuery('cleanup productDelete', productDeleteMutation, { input: { id: productId } }).catch(
        (error: unknown) => ({
          request: { query: productDeleteMutation, variables: { input: { id: productId } } },
          status: 0,
          response: { error: String(error) },
        }),
      ),
    );
  }
}

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      suffix,
      namespace,
      key,
      productCreate,
      definitionCreate,
      metafieldsSet,
      collectionCreate,
      readBeforeDelete,
      deleteFalse,
      readAfterFalse,
      deleteTrue,
      readAfterTrue,
      cleanup,
      upstreamCalls,
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(JSON.stringify({ ok: true, outputPath, apiVersion, suffix, cleanupCount: cleanup.length }, null, 2));

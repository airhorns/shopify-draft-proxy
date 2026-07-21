/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

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

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const requestDir = path.join('config', 'parity-requests', 'metafield-definitions');
const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metafields');
const createDocumentPath = path.join(requestDir, 'validation-affects-values-create.graphql');
const updateDocumentPath = path.join(requestDir, 'validation-affects-values-update.graphql');
const setDocumentPath = path.join(requestDir, 'validation-affects-values-set.graphql');
const readDocumentPath = path.join(requestDir, 'validation-affects-values-read.graphql');
const fixturePath = path.join(fixtureDir, 'metafield-definition-validation-affects-values.json');
const hydrateByIdentifierDocument = await readFile(
  'config/parity-requests/metafields/metafield-definition-hydrate-by-identifier.graphql',
  'utf8',
);
const hydrateResourceScopeDocument = await readFile(
  'config/parity-requests/metafields/metafield-definitions-hydrate-resource-scope.graphql',
  'utf8',
);

const productCreateMutation = `#graphql
mutation ValidationAffectsValuesProductCreate($product: ProductCreateInput!) {
  productCreate(product: $product) {
    product { id title }
    userErrors { field message }
  }
}
`;

const productDeleteMutation = `#graphql
mutation ValidationAffectsValuesProductDelete($input: ProductDeleteInput!) {
  productDelete(input: $input) {
    deletedProductId
    userErrors { field message }
  }
}
`;

const deleteDefinitionMutation = `#graphql
mutation ValidationAffectsValuesDefinitionDelete($id: ID!) {
  metafieldDefinitionDelete(id: $id, deleteAllAssociatedMetafields: true) {
    deletedDefinitionId
    userErrors { field message code }
  }
}
`;

const ownerMetafieldHydrateQuery =
  'query OwnerMetafieldsHydrateNodes($ids: [ID!]!, $metafield0Namespace: String!, $metafield0Key: String!) { nodes(ids: $ids) { __typename id ... on Product { id title handle status totalInventory tracksInventory createdAt updatedAt metafield0: metafield(namespace: $metafield0Namespace, key: $metafield0Key) { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType }  } ... on ProductVariant { id title sku barcode price compareAtPrice taxable inventoryPolicy inventoryQuantity selectedOptions { name value } inventoryItem { id tracked requiresShipping } product { id title handle status totalInventory tracksInventory createdAt updatedAt } metafield0: metafield(namespace: $metafield0Namespace, key: $metafield0Key) { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } } ... on Collection { id title handle metafield0: metafield(namespace: $metafield0Namespace, key: $metafield0Key) { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } } ... on Customer { id displayName email metafield0: metafield(namespace: $metafield0Namespace, key: $metafield0Key) { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } } ... on Order { id name metafield0: metafield(namespace: $metafield0Namespace, key: $metafield0Key) { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } } ... on Company { id name metafield0: metafield(namespace: $metafield0Namespace, key: $metafield0Key) { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } } ... on Shop { id metafield0: metafield(namespace: $metafield0Namespace, key: $metafield0Key) { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } } } }';

function assertHttpOk(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function readObject(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as Record<string, unknown>) : null;
}

function readPath(value: unknown, parts: string[]): unknown {
  let cursor: unknown = value;
  for (const part of parts) cursor = readObject(cursor)?.[part];
  return cursor;
}

function requireString(value: unknown, label: string): string {
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`${label} was not returned: ${JSON.stringify(value)}`);
  }
  return value;
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

function requireCapture(value: CapturedInteraction | null, label: string): CapturedInteraction {
  if (!value) throw new Error(`${label} was not captured`);
  return value;
}

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

async function recordResourceScope(): Promise<UpstreamCall[]> {
  const calls: UpstreamCall[] = [];
  let after: string | null = null;
  let observed = 0;
  for (let page = 0; page < 3; page += 1) {
    const variables = { ownerType: 'PRODUCT', query: '-namespace:app--*', first: 250, after };
    const call = await recordUpstreamCall(
      'MetafieldDefinitionsHydrateResourceScope',
      hydrateResourceScopeDocument,
      variables,
    );
    calls.push(call);
    const nodes = readPath(call.response.body, ['data', 'metafieldDefinitions', 'nodes']);
    if (!Array.isArray(nodes)) throw new Error(`resource-scope page ${page + 1} omitted nodes`);
    observed += nodes.filter((node) => readObject(node)?.['namespace'] !== 'shopify').length;
    const pageInfo = readObject(readPath(call.response.body, ['data', 'metafieldDefinitions', 'pageInfo']));
    if (observed >= 256 || pageInfo?.['hasNextPage'] !== true) break;
    const endCursor = pageInfo?.['endCursor'];
    if (typeof endCursor !== 'string') throw new Error(`resource-scope page ${page + 1} omitted endCursor`);
    after = endCursor;
  }
  return calls;
}

const suffix = Date.now().toString(36);
const namespace = `validation_affects_${suffix}`;
const key = 'headline';
let productId: string | null = null;
let definitionId: string | null = null;
const cleanup: CapturedInteraction[] = [];
const upstreamCalls: UpstreamCall[] = [];
let productCreate: CapturedInteraction | null = null;
let create: CapturedInteraction | null = null;
let setBeforeUpdate: CapturedInteraction | null = null;
let validationUpdate: CapturedInteraction | null = null;
let setTooLongAfterUpdate: CapturedInteraction | null = null;
let setShortAfterUpdate: CapturedInteraction | null = null;
let readAfterShortSet: CapturedInteraction | null = null;

try {
  productCreate = await captureQuery('productCreate setup', productCreateMutation, {
    product: { title: `validation affects values ${suffix}` },
  });
  productId = requireString(readPath(productCreate.response, ['data', 'productCreate', 'product', 'id']), 'product id');

  upstreamCalls.push(
    await recordUpstreamCall('MetafieldDefinitionHydrateByIdentifier', hydrateByIdentifierDocument, {
      identifier: { ownerType: 'PRODUCT', namespace, key },
    }),
  );
  upstreamCalls.push(...(await recordResourceScope()));

  create = await captureDocument('metafieldDefinitionCreate setup', createDocumentPath, {
    definition: {
      name: 'Validation Affects Values',
      namespace,
      key,
      ownerType: 'PRODUCT',
      type: 'single_line_text_field',
    },
  });
  definitionId = requireString(
    readPath(create.response, ['data', 'metafieldDefinitionCreate', 'createdDefinition', 'id']),
    'definition id',
  );

  setBeforeUpdate = await captureDocument('metafieldsSet before validation update', setDocumentPath, {
    metafields: [
      {
        ownerId: productId,
        namespace,
        key,
        type: 'single_line_text_field',
        value: 'unbounded headline',
      },
    ],
  });

  validationUpdate = await captureDocument('metafieldDefinitionUpdate validation max', updateDocumentPath, {
    definition: {
      namespace,
      key,
      ownerType: 'PRODUCT',
      validations: [{ name: 'max', value: '5' }],
    },
  });

  setTooLongAfterUpdate = await captureDocument('metafieldsSet too long after validation update', setDocumentPath, {
    metafields: [
      {
        ownerId: productId,
        namespace,
        key,
        type: 'single_line_text_field',
        value: 'too long',
      },
    ],
  });

  setShortAfterUpdate = await captureDocument('metafieldsSet short after validation update', setDocumentPath, {
    metafields: [
      {
        ownerId: productId,
        namespace,
        key,
        type: 'single_line_text_field',
        value: 'short',
      },
    ],
  });

  upstreamCalls.push(
    await recordUpstreamCall('OwnerMetafieldsHydrateNodes', ownerMetafieldHydrateQuery, {
      ids: [productId],
      metafield0Namespace: namespace,
      metafield0Key: key,
    }),
  );

  readAfterShortSet = await captureDocument('product metafield read after short set', readDocumentPath, {
    id: productId,
    namespace,
    key,
  });
} finally {
  if (definitionId) {
    cleanup.push(
      await captureQuery('cleanup metafieldDefinitionDelete', deleteDefinitionMutation, { id: definitionId }).catch(
        (error: unknown) => ({
          request: { query: deleteDefinitionMutation, variables: { id: definitionId } },
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

await mkdir(fixtureDir, { recursive: true });
await writeFile(
  fixturePath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      namespace,
      key,
      productCreate: requireCapture(productCreate, 'productCreate'),
      create: requireCapture(create, 'create'),
      setBeforeUpdate: requireCapture(setBeforeUpdate, 'setBeforeUpdate'),
      validationUpdate: requireCapture(validationUpdate, 'validationUpdate'),
      setTooLongAfterUpdate: requireCapture(setTooLongAfterUpdate, 'setTooLongAfterUpdate'),
      setShortAfterUpdate: requireCapture(setShortAfterUpdate, 'setShortAfterUpdate'),
      readAfterShortSet: requireCapture(readAfterShortSet, 'readAfterShortSet'),
      cleanup,
      upstreamCalls,
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(
  JSON.stringify(
    {
      ok: true,
      outputPath: fixturePath,
      namespace,
      productId,
      definitionId,
    },
    null,
    2,
  ),
);

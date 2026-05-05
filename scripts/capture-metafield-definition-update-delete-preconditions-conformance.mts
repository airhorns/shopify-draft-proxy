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
const outputPath = path.join(outputDir, 'metafield-definition-update-delete-preconditions.json');
const createDocumentPath =
  'config/parity-requests/metafields/metafield-definition-update-delete-preconditions-create.graphql';
const metafieldsSetDocumentPath =
  'config/parity-requests/metafields/metafield-definition-update-delete-preconditions-metafields-set.graphql';
const deleteNoFlagDocumentPath =
  'config/parity-requests/metafields/metafield-definition-update-delete-preconditions-delete-no-flag.graphql';
const deleteWithFlagDocumentPath =
  'config/parity-requests/metafields/metafield-definition-update-delete-preconditions-delete-with-flag.graphql';
const updateDocumentPath =
  'config/parity-requests/metafields/metafield-definition-update-delete-preconditions-update.graphql';

const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const productCreateMutation = `#graphql
  mutation HAR697CreateProduct($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product { id title }
      userErrors { field message }
    }
  }
`;

const productDeleteMutation = `#graphql
  mutation HAR697DeleteProduct($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors { field message }
    }
  }
`;

const deleteDefinitionMutation = `#graphql
  mutation HAR697CleanupDefinition($id: ID!) {
    metafieldDefinitionDelete(id: $id, deleteAllAssociatedMetafields: true) {
      deletedDefinitionId
      userErrors { field message code }
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

type FlowSetup = {
  namespace: string;
  key: string;
  product?: CapturedInteraction;
  productId?: string;
  create: CapturedInteraction;
  definitionId: string;
  metafieldsSet?: CapturedInteraction;
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

async function createProduct(label: string, suffix: string): Promise<CapturedInteraction> {
  return captureQuery(`${label} productCreate`, productCreateMutation, {
    product: { title: `HAR-697 ${label} ${suffix}` },
  });
}

function createdProductId(capture: CapturedInteraction): string {
  const id = readPath(capture.response, ['data', 'productCreate', 'product', 'id']);
  if (typeof id !== 'string') {
    throw new Error(`productCreate did not return a product id: ${JSON.stringify(capture.response)}`);
  }
  return id;
}

function createdDefinitionId(capture: CapturedInteraction): string {
  const id = readPath(capture.response, ['data', 'metafieldDefinitionCreate', 'createdDefinition', 'id']);
  if (typeof id !== 'string') {
    throw new Error(`metafieldDefinitionCreate did not return a definition id: ${JSON.stringify(capture.response)}`);
  }
  return id;
}

function deleteSucceeded(capture: CapturedInteraction): boolean {
  return typeof readPath(capture.response, ['data', 'metafieldDefinitionDelete', 'deletedDefinitionId']) === 'string';
}

async function setupFlow(label: string, suffix: string, withMetafield: boolean): Promise<FlowSetup> {
  const namespace = `${suffix}_${label}`;
  const key = 'tier';
  const product = withMetafield ? await createProduct(label, suffix) : undefined;
  const productId = product ? createdProductId(product) : undefined;
  const create = await captureDocument(`${label} metafieldDefinitionCreate`, createDocumentPath, {
    definition: {
      name: `HAR-697 ${label}`,
      namespace,
      key,
      ownerType: 'PRODUCT',
      type: 'single_line_text_field',
    },
  });
  const definitionId = createdDefinitionId(create);
  const metafieldsSet =
    productId === undefined
      ? undefined
      : await captureDocument(`${label} metafieldsSet`, metafieldsSetDocumentPath, {
          metafields: [
            {
              ownerId: productId,
              namespace,
              key,
              type: 'single_line_text_field',
              value: 'gold',
            },
          ],
        });

  return { namespace, key, product, productId, create, definitionId, metafieldsSet };
}

async function cleanup(productId?: string, definitionId?: string): Promise<CapturedInteraction[]> {
  const cleanupCaptures: CapturedInteraction[] = [];
  if (definitionId) {
    cleanupCaptures.push(
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
    cleanupCaptures.push(
      await captureQuery('cleanup productDelete', productDeleteMutation, { input: { id: productId } }).catch(
        (error: unknown) => ({
          request: { query: productDeleteMutation, variables: { input: { id: productId } } },
          status: 0,
          response: { error: String(error) },
        }),
      ),
    );
  }
  return cleanupCaptures;
}

const suffix = `har697_${Date.now().toString(36)}`;
const cleanupCaptures: CapturedInteraction[] = [];

const deleteWithoutFlag = await setupFlow('delete_without_flag', suffix, true);
const deleteWithoutFlagDelete = await captureDocument('delete without flag', deleteNoFlagDocumentPath, {
  id: deleteWithoutFlag.definitionId,
});
cleanupCaptures.push(
  ...(await cleanup(
    deleteWithoutFlag.productId,
    deleteSucceeded(deleteWithoutFlagDelete) ? undefined : deleteWithoutFlag.definitionId,
  )),
);

const deleteFalseFlag = await setupFlow('delete_false_flag', suffix, true);
const deleteFalseFlagDelete = await captureDocument(
  'delete false associated metafields flag',
  deleteWithFlagDocumentPath,
  {
    id: deleteFalseFlag.definitionId,
    deleteAllAssociatedMetafields: false,
  },
);
cleanupCaptures.push(
  ...(await cleanup(
    deleteFalseFlag.productId,
    deleteSucceeded(deleteFalseFlagDelete) ? undefined : deleteFalseFlag.definitionId,
  )),
);

const deleteWithFlag = await setupFlow('delete_with_flag', suffix, true);
const deleteWithFlagDelete = await captureDocument(
  'delete with associated metafields flag',
  deleteWithFlagDocumentPath,
  {
    id: deleteWithFlag.definitionId,
    deleteAllAssociatedMetafields: true,
  },
);
cleanupCaptures.push(
  ...(await cleanup(
    deleteWithFlag.productId,
    deleteSucceeded(deleteWithFlagDelete) ? undefined : deleteWithFlag.definitionId,
  )),
);

const updateNamespace = await setupFlow('update_namespace', suffix, false);
const updateNamespaceUpdate = await captureDocument('update namespace precondition', updateDocumentPath, {
  definition: {
    namespace: `${updateNamespace.namespace}_changed`,
    key: updateNamespace.key,
    ownerType: 'PRODUCT',
    name: 'HAR-697 namespace changed',
  },
});
cleanupCaptures.push(...(await cleanup(undefined, updateNamespace.definitionId)));

const updateKey = await setupFlow('update_key', suffix, false);
const updateKeyUpdate = await captureDocument('update key precondition', updateDocumentPath, {
  definition: {
    namespace: updateKey.namespace,
    key: 'tier_changed',
    ownerType: 'PRODUCT',
    name: 'HAR-697 key changed',
  },
});
cleanupCaptures.push(...(await cleanup(undefined, updateKey.definitionId)));

const updateOwnerType = await setupFlow('update_owner_type', suffix, false);
const updateOwnerTypeUpdate = await captureDocument('update owner type precondition', updateDocumentPath, {
  definition: {
    namespace: updateOwnerType.namespace,
    key: updateOwnerType.key,
    ownerType: 'CUSTOMER',
    name: 'HAR-697 owner type changed',
  },
});
cleanupCaptures.push(...(await cleanup(undefined, updateOwnerType.definitionId)));

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      suffix,
      deleteWithoutFlag: {
        namespace: deleteWithoutFlag.namespace,
        key: deleteWithoutFlag.key,
        product: deleteWithoutFlag.product,
        create: deleteWithoutFlag.create,
        metafieldsSet: deleteWithoutFlag.metafieldsSet,
        delete: deleteWithoutFlagDelete,
      },
      deleteFalseFlag: {
        namespace: deleteFalseFlag.namespace,
        key: deleteFalseFlag.key,
        product: deleteFalseFlag.product,
        create: deleteFalseFlag.create,
        metafieldsSet: deleteFalseFlag.metafieldsSet,
        delete: deleteFalseFlagDelete,
      },
      deleteWithFlag: {
        namespace: deleteWithFlag.namespace,
        key: deleteWithFlag.key,
        product: deleteWithFlag.product,
        create: deleteWithFlag.create,
        metafieldsSet: deleteWithFlag.metafieldsSet,
        delete: deleteWithFlagDelete,
      },
      updateNamespace: {
        namespace: updateNamespace.namespace,
        key: updateNamespace.key,
        create: updateNamespace.create,
        update: updateNamespaceUpdate,
      },
      updateKey: {
        namespace: updateKey.namespace,
        key: updateKey.key,
        create: updateKey.create,
        update: updateKeyUpdate,
      },
      updateOwnerType: {
        namespace: updateOwnerType.namespace,
        key: updateOwnerType.key,
        create: updateOwnerType.create,
        update: updateOwnerTypeUpdate,
      },
      cleanup: cleanupCaptures,
      upstreamCalls: [],
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
      outputPath,
      apiVersion,
      suffix,
      cleanupCount: cleanupCaptures.length,
    },
    null,
    2,
  ),
);

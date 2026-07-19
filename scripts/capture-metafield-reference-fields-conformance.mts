/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlPayload } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type Capture = {
  name: string;
  request: {
    query: string;
    variables: Record<string, unknown>;
  };
  status: number;
  response: ConformanceGraphqlPayload;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metafields');
const outputPath = path.join(outputDir, 'metafield-reference-fields-lifecycle.json');
const runId = Date.now().toString();
const metaobjectTargetType = `reference_target_${runId}`;
const metaobjectParentType = `reference_parent_${runId}`;

const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const documents = {
  productCreate: await readFile(
    'config/parity-requests/metafields/metafield-reference-fields-product-create.graphql',
    'utf8',
  ),
  collectionCreate: await readFile(
    'config/parity-requests/metafields/metafield-reference-fields-collection-create.graphql',
    'utf8',
  ),
  fileCreate: await readFile(
    'config/parity-requests/metafields/metafield-reference-fields-file-create.graphql',
    'utf8',
  ),
  pageCreate: await readFile(
    'config/parity-requests/metafields/metafield-reference-fields-page-create.graphql',
    'utf8',
  ),
  productDelete: await readFile(
    'config/parity-requests/metafields/metafield-reference-fields-product-delete.graphql',
    'utf8',
  ),
  metafieldsSet: await readFile('config/parity-requests/metafields/metafield-reference-fields-set.graphql', 'utf8'),
  read: await readFile('config/parity-requests/metafields/metafield-reference-fields-read.graphql', 'utf8'),
  metaobjectDefinitionCreate: await readFile(
    'config/parity-requests/metafields/metafield-reference-fields-metaobject-definition-create.graphql',
    'utf8',
  ),
  metaobjectCreate: await readFile(
    'config/parity-requests/metafields/metafield-reference-fields-metaobject-create.graphql',
    'utf8',
  ),
  metaobjectRead: await readFile(
    'config/parity-requests/metafields/metafield-reference-fields-metaobject-read.graphql',
    'utf8',
  ),
};

const cleanupDocuments = {
  collectionDelete: `#graphql
    mutation MetafieldReferenceFieldsCleanupCollection($input: CollectionDeleteInput!) {
      collectionDelete(input: $input) {
        deletedCollectionId
        userErrors {
          field
          message
        }
      }
    }
  `,
  fileDelete: `#graphql
    mutation MetafieldReferenceFieldsCleanupFile($fileIds: [ID!]!) {
      fileDelete(fileIds: $fileIds) {
        deletedFileIds
        userErrors {
          field
          message
          code
        }
      }
    }
  `,
  pageDelete: `#graphql
    mutation MetafieldReferenceFieldsCleanupPage($id: ID!) {
      pageDelete(id: $id) {
        deletedPageId
        userErrors {
          field
          message
          code
        }
      }
    }
  `,
  metaobjectDelete: `#graphql
    mutation MetafieldReferenceFieldsCleanupMetaobject($id: ID!) {
      metaobjectDelete(id: $id) {
        deletedId
        userErrors {
          field
          message
          code
          elementKey
          elementIndex
        }
      }
    }
  `,
  metaobjectDefinitionDelete: `#graphql
    mutation MetafieldReferenceFieldsCleanupMetaobjectDefinition($id: ID!) {
      metaobjectDefinitionDelete(id: $id) {
        deletedId
        userErrors {
          field
          message
          code
          elementKey
          elementIndex
        }
      }
    }
  `,
};

function readPath(value: unknown, pathSegments: Array<string | number>, context: string): unknown {
  let current = value;
  for (const segment of pathSegments) {
    if (typeof segment === 'number') {
      if (!Array.isArray(current)) {
        throw new Error(`${context}: expected array before segment ${segment}`);
      }
      current = current[segment];
    } else {
      if (typeof current !== 'object' || current === null || !(segment in current)) {
        throw new Error(`${context}: missing path segment ${segment}`);
      }
      current = (current as Record<string, unknown>)[segment];
    }
  }
  return current;
}

function readStringPath(value: unknown, pathSegments: Array<string | number>, context: string): string {
  const found = readPath(value, pathSegments, context);
  if (typeof found !== 'string' || found.length === 0) {
    throw new Error(`${context}: expected non-empty string at ${pathSegments.join('.')}`);
  }
  return found;
}

function assertNoTopLevelErrors(capture: Capture): void {
  if (capture.status < 200 || capture.status >= 300 || capture.response.errors) {
    throw new Error(`${capture.name} failed: ${JSON.stringify(capture.response, null, 2)}`);
  }
}

function assertNoUserErrors(capture: Capture, pathSegments: Array<string | number>): void {
  const userErrors = readPath(capture.response, pathSegments, capture.name);
  if (!Array.isArray(userErrors) || userErrors.length > 0) {
    throw new Error(`${capture.name} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

async function captureGraphql(name: string, query: string, variables: Record<string, unknown>): Promise<Capture> {
  const result = await runGraphqlRaw(query, variables);
  const capture = {
    name,
    request: { query, variables },
    status: result.status,
    response: result.payload,
  };
  assertNoTopLevelErrors(capture);
  return capture;
}

async function cleanupGraphql(name: string, query: string, variables: Record<string, unknown>): Promise<Capture> {
  const result = await runGraphqlRaw(query, variables);
  return {
    name,
    request: { query, variables },
    status: result.status,
    response: result.payload,
  };
}

function buildProductVariables(label: string): Record<string, unknown> {
  return {
    product: {
      title: `Metafield reference ${label} ${runId}`,
      status: 'DRAFT',
    },
  };
}

const cleanup: Capture[] = [];
const deletedProductIds = new Set<string>();
let cleanupComplete = false;
const created = {
  existingTargetProductId: null as string | null,
  existingTargetCollectionId: null as string | null,
  existingOwnerProductId: null as string | null,
  stagedTargetProductId: null as string | null,
  stagedOwnerProductId: null as string | null,
  stagedCollectionId: null as string | null,
  fileId: null as string | null,
  pageId: null as string | null,
  metaobjectTargetDefinitionId: null as string | null,
  metaobjectParentDefinitionId: null as string | null,
  metaobjectTargetAId: null as string | null,
  metaobjectTargetBId: null as string | null,
  metaobjectParentId: null as string | null,
};

async function captureCleanup(): Promise<void> {
  if (cleanupComplete) {
    return;
  }
  cleanupComplete = true;

  for (const productId of [
    created.existingOwnerProductId,
    created.stagedOwnerProductId,
    created.stagedTargetProductId,
    created.existingTargetProductId,
  ]) {
    if (productId && !deletedProductIds.has(productId)) {
      cleanup.push(
        await cleanupGraphql('cleanup-product-delete', documents.productDelete, {
          input: { id: productId },
        }),
      );
    }
  }
  for (const collectionId of [created.stagedCollectionId, created.existingTargetCollectionId]) {
    if (collectionId) {
      cleanup.push(
        await cleanupGraphql('cleanup-collection-delete', cleanupDocuments.collectionDelete, {
          input: { id: collectionId },
        }),
      );
    }
  }
  if (created.fileId) {
    cleanup.push(
      await cleanupGraphql('cleanup-file-delete', cleanupDocuments.fileDelete, {
        fileIds: [created.fileId],
      }),
    );
  }
  if (created.pageId) {
    cleanup.push(
      await cleanupGraphql('cleanup-page-delete', cleanupDocuments.pageDelete, {
        id: created.pageId,
      }),
    );
  }
  for (const metaobjectId of [created.metaobjectParentId, created.metaobjectTargetBId, created.metaobjectTargetAId]) {
    if (metaobjectId) {
      cleanup.push(
        await cleanupGraphql('cleanup-metaobject-delete', cleanupDocuments.metaobjectDelete, {
          id: metaobjectId,
        }),
      );
    }
  }
  for (const definitionId of [created.metaobjectParentDefinitionId, created.metaobjectTargetDefinitionId]) {
    if (definitionId) {
      cleanup.push(
        await cleanupGraphql('cleanup-metaobject-definition-delete', cleanupDocuments.metaobjectDefinitionDelete, {
          id: definitionId,
        }),
      );
    }
  }
}

try {
  const existingTargetProduct = await captureGraphql(
    'existing-target-product-create',
    documents.productCreate,
    buildProductVariables('existing target'),
  );
  assertNoUserErrors(existingTargetProduct, ['data', 'productCreate', 'userErrors']);
  created.existingTargetProductId = readStringPath(
    existingTargetProduct.response,
    ['data', 'productCreate', 'product', 'id'],
    'existing target product id',
  );
  const existingTargetVariantId = readStringPath(
    existingTargetProduct.response,
    ['data', 'productCreate', 'product', 'variants', 'nodes', 0, 'id'],
    'existing target default variant id',
  );

  const existingTargetCollection = await captureGraphql('existing-target-collection-create', documents.collectionCreate, {
    input: {
      title: `Metafield reference existing collection ${runId}`,
    },
  });
  assertNoUserErrors(existingTargetCollection, ['data', 'collectionCreate', 'userErrors']);
  created.existingTargetCollectionId = readStringPath(
    existingTargetCollection.response,
    ['data', 'collectionCreate', 'collection', 'id'],
    'existing target collection id',
  );

  const existingOwnerProduct = await captureGraphql(
    'existing-reference-owner-product-create',
    documents.productCreate,
    buildProductVariables('existing owner'),
  );
  assertNoUserErrors(existingOwnerProduct, ['data', 'productCreate', 'userErrors']);
  created.existingOwnerProductId = readStringPath(
    existingOwnerProduct.response,
    ['data', 'productCreate', 'product', 'id'],
    'existing owner product id',
  );

  const setExistingReferences = await captureGraphql('existing-reference-metafields-set', documents.metafieldsSet, {
    metafields: [
      {
        ownerId: created.existingOwnerProductId,
        namespace: 'reference_fields',
        key: 'product_ref',
        type: 'product_reference',
        value: created.existingTargetProductId,
      },
      {
        ownerId: created.existingOwnerProductId,
        namespace: 'reference_fields',
        key: 'product_refs',
        type: 'list.product_reference',
        value: JSON.stringify([created.existingTargetProductId]),
      },
      {
        ownerId: created.existingOwnerProductId,
        namespace: 'reference_fields',
        key: 'variant_ref',
        type: 'variant_reference',
        value: existingTargetVariantId,
      },
      {
        ownerId: created.existingOwnerProductId,
        namespace: 'reference_fields',
        key: 'collection_ref',
        type: 'collection_reference',
        value: created.existingTargetCollectionId,
      },
      {
        ownerId: created.existingOwnerProductId,
        namespace: 'reference_fields',
        key: 'plain',
        type: 'single_line_text_field',
        value: 'not a reference',
      },
    ],
  });
  assertNoUserErrors(setExistingReferences, ['data', 'metafieldsSet', 'userErrors']);

  const existingReferenceRead = await captureGraphql('existing-reference-fields-read', documents.read, {
    ownerId: created.existingOwnerProductId,
  });

  const stagedTargetProduct = await captureGraphql(
    'staged-target-product-create',
    documents.productCreate,
    buildProductVariables('staged target'),
  );
  assertNoUserErrors(stagedTargetProduct, ['data', 'productCreate', 'userErrors']);
  created.stagedTargetProductId = readStringPath(
    stagedTargetProduct.response,
    ['data', 'productCreate', 'product', 'id'],
    'staged target product id',
  );
  const stagedTargetVariantId = readStringPath(
    stagedTargetProduct.response,
    ['data', 'productCreate', 'product', 'variants', 'nodes', 0, 'id'],
    'staged target default variant id',
  );

  const stagedOwnerProduct = await captureGraphql(
    'staged-reference-owner-product-create',
    documents.productCreate,
    buildProductVariables('staged owner'),
  );
  assertNoUserErrors(stagedOwnerProduct, ['data', 'productCreate', 'userErrors']);
  created.stagedOwnerProductId = readStringPath(
    stagedOwnerProduct.response,
    ['data', 'productCreate', 'product', 'id'],
    'staged owner product id',
  );

  const stagedCollection = await captureGraphql('staged-target-collection-create', documents.collectionCreate, {
    input: {
      title: `Metafield reference staged collection ${runId}`,
    },
  });
  assertNoUserErrors(stagedCollection, ['data', 'collectionCreate', 'userErrors']);
  created.stagedCollectionId = readStringPath(
    stagedCollection.response,
    ['data', 'collectionCreate', 'collection', 'id'],
    'staged collection id',
  );

  const file = await captureGraphql('staged-target-file-create', documents.fileCreate, {
    files: [
      {
        alt: `Metafield reference image ${runId}`,
        contentType: 'IMAGE',
        filename: `metafield-reference-${runId}.png`,
        originalSource: `https://placehold.co/600x400.png?text=metafield-reference-${runId}`,
      },
    ],
  });
  assertNoUserErrors(file, ['data', 'fileCreate', 'userErrors']);
  created.fileId = readStringPath(file.response, ['data', 'fileCreate', 'files', 0, 'id'], 'file id');

  const page = await captureGraphql('staged-target-page-create', documents.pageCreate, {
    page: {
      title: `Metafield reference page ${runId}`,
      body: '<p>Metafield reference page body.</p>',
    },
  });
  assertNoUserErrors(page, ['data', 'pageCreate', 'userErrors']);
  created.pageId = readStringPath(page.response, ['data', 'pageCreate', 'page', 'id'], 'page id');

  const setStagedReferences = await captureGraphql('staged-reference-metafields-set', documents.metafieldsSet, {
    metafields: [
      {
        ownerId: created.stagedOwnerProductId,
        namespace: 'reference_fields',
        key: 'product_ref',
        type: 'product_reference',
        value: created.stagedTargetProductId,
      },
      {
        ownerId: created.stagedOwnerProductId,
        namespace: 'reference_fields',
        key: 'product_refs',
        type: 'list.product_reference',
        value: JSON.stringify([created.stagedTargetProductId]),
      },
      {
        ownerId: created.stagedOwnerProductId,
        namespace: 'reference_fields',
        key: 'variant_ref',
        type: 'variant_reference',
        value: stagedTargetVariantId,
      },
      {
        ownerId: created.stagedOwnerProductId,
        namespace: 'reference_fields',
        key: 'collection_ref',
        type: 'collection_reference',
        value: created.stagedCollectionId,
      },
      {
        ownerId: created.stagedOwnerProductId,
        namespace: 'reference_fields',
        key: 'file_ref',
        type: 'file_reference',
        value: created.fileId,
      },
      {
        ownerId: created.stagedOwnerProductId,
        namespace: 'reference_fields',
        key: 'page_ref',
        type: 'page_reference',
        value: created.pageId,
      },
      {
        ownerId: created.stagedOwnerProductId,
        namespace: 'reference_fields',
        key: 'plain',
        type: 'single_line_text_field',
        value: 'not a reference',
      },
    ],
  });
  assertNoUserErrors(setStagedReferences, ['data', 'metafieldsSet', 'userErrors']);

  const stagedReferenceRead = await captureGraphql('staged-reference-fields-read', documents.read, {
    ownerId: created.stagedOwnerProductId,
  });

  const deleteStagedTargetProduct = await captureGraphql('staged-reference-target-product-delete', documents.productDelete, {
    input: { id: created.stagedTargetProductId },
  });
  assertNoUserErrors(deleteStagedTargetProduct, ['data', 'productDelete', 'userErrors']);
  if (created.stagedTargetProductId) {
    deletedProductIds.add(created.stagedTargetProductId);
  }

  const stagedReferencePostDeleteRead = await captureGraphql(
    'staged-reference-fields-read-after-product-delete',
    documents.read,
    {
      ownerId: created.stagedOwnerProductId,
    },
  );

  const metaobjectTargetDefinition = await captureGraphql(
    'metaobject-target-definition-create',
    documents.metaobjectDefinitionCreate,
    {
      definition: {
        type: metaobjectTargetType,
        name: `Reference Target ${runId}`,
        displayNameKey: 'title',
        fieldDefinitions: [
          {
            key: 'title',
            name: 'Title',
            type: 'single_line_text_field',
            required: true,
          },
        ],
      },
    },
  );
  assertNoUserErrors(metaobjectTargetDefinition, ['data', 'metaobjectDefinitionCreate', 'userErrors']);
  created.metaobjectTargetDefinitionId = readStringPath(
    metaobjectTargetDefinition.response,
    ['data', 'metaobjectDefinitionCreate', 'metaobjectDefinition', 'id'],
    'metaobject target definition id',
  );

  const metaobjectParentDefinition = await captureGraphql(
    'metaobject-parent-definition-create',
    documents.metaobjectDefinitionCreate,
    {
      definition: {
        type: metaobjectParentType,
        name: `Reference Parent ${runId}`,
        displayNameKey: 'title',
        fieldDefinitions: [
          {
            key: 'title',
            name: 'Title',
            type: 'single_line_text_field',
            required: true,
          },
          {
            key: 'single_ref',
            name: 'Single Ref',
            type: 'metaobject_reference',
            required: false,
            validations: [{ name: 'metaobject_definition_id', value: created.metaobjectTargetDefinitionId }],
          },
          {
            key: 'list_ref',
            name: 'List Ref',
            type: 'list.metaobject_reference',
            required: false,
            validations: [{ name: 'metaobject_definition_id', value: created.metaobjectTargetDefinitionId }],
          },
          {
            key: 'plain',
            name: 'Plain',
            type: 'single_line_text_field',
            required: false,
          },
        ],
      },
    },
  );
  assertNoUserErrors(metaobjectParentDefinition, ['data', 'metaobjectDefinitionCreate', 'userErrors']);
  created.metaobjectParentDefinitionId = readStringPath(
    metaobjectParentDefinition.response,
    ['data', 'metaobjectDefinitionCreate', 'metaobjectDefinition', 'id'],
    'metaobject parent definition id',
  );

  const metaobjectTargetA = await captureGraphql('metaobject-target-a-create', documents.metaobjectCreate, {
    metaobject: {
      type: metaobjectTargetType,
      handle: `target-a-${runId}`,
      fields: [{ key: 'title', value: `Target A ${runId}` }],
    },
  });
  assertNoUserErrors(metaobjectTargetA, ['data', 'metaobjectCreate', 'userErrors']);
  created.metaobjectTargetAId = readStringPath(
    metaobjectTargetA.response,
    ['data', 'metaobjectCreate', 'metaobject', 'id'],
    'metaobject target A id',
  );

  const metaobjectTargetB = await captureGraphql('metaobject-target-b-create', documents.metaobjectCreate, {
    metaobject: {
      type: metaobjectTargetType,
      handle: `target-b-${runId}`,
      fields: [{ key: 'title', value: `Target B ${runId}` }],
    },
  });
  assertNoUserErrors(metaobjectTargetB, ['data', 'metaobjectCreate', 'userErrors']);
  created.metaobjectTargetBId = readStringPath(
    metaobjectTargetB.response,
    ['data', 'metaobjectCreate', 'metaobject', 'id'],
    'metaobject target B id',
  );

  const metaobjectParent = await captureGraphql('metaobject-parent-create', documents.metaobjectCreate, {
    metaobject: {
      type: metaobjectParentType,
      handle: `parent-${runId}`,
      fields: [
        { key: 'title', value: `Reference Parent ${runId}` },
        { key: 'single_ref', value: created.metaobjectTargetAId },
        { key: 'list_ref', value: JSON.stringify([created.metaobjectTargetAId, created.metaobjectTargetBId]) },
        { key: 'plain', value: created.metaobjectTargetAId },
      ],
    },
  });
  assertNoUserErrors(metaobjectParent, ['data', 'metaobjectCreate', 'userErrors']);
  created.metaobjectParentId = readStringPath(
    metaobjectParent.response,
    ['data', 'metaobjectCreate', 'metaobject', 'id'],
    'metaobject parent id',
  );

  const metaobjectReferenceRead = await captureGraphql('metaobject-reference-fields-read', documents.metaobjectRead, {
    targetAId: created.metaobjectTargetAId,
    targetBId: created.metaobjectTargetBId,
    parentId: created.metaobjectParentId,
  });

  await captureCleanup();

  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        summary:
          'Live Shopify capture for Metafield.reference/references and MetaobjectField.reference/references resolving existing, staged, and deleted reference targets.',
        setup: {
          existingTargetProduct,
          existingTargetCollection,
          existingOwnerProduct,
          stagedTargetProduct,
          stagedOwnerProduct,
          stagedCollection,
          file,
          page,
          metaobjectTargetDefinition,
          metaobjectParentDefinition,
          metaobjectTargetA,
          metaobjectTargetB,
          metaobjectParent,
        },
        mutation: {
          setExistingReferences,
          setStagedReferences,
          deleteStagedTargetProduct,
        },
        read: {
          existingReferenceRead,
          stagedReferenceRead,
          stagedReferencePostDeleteRead,
          metaobjectReferenceRead,
        },
        cleanup,
      },
      null,
      2,
    )}\n`,
    'utf8',
  );
  console.log(`Wrote ${outputPath}`);
} finally {
  await captureCleanup();
}

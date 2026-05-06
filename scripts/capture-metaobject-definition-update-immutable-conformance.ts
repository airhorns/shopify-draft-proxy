/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type Capture = {
  name: string;
  request: {
    query: string;
    variables: Record<string, unknown>;
  };
  status: number;
  response: unknown;
};

type Seed = {
  runId: string;
  standardType: string;
  standardDefinitionPreexisting: boolean;
  standardDefinitionId?: string;
  linkedDefinitionId?: string;
  linkedMetaobjectIds: string[];
  linkedMetafieldDefinitionId?: string;
  linkedProductId?: string;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metaobjects');
const outputPath = path.join(outputDir, 'metaobjectDefinitionUpdate-immutable.json');
const runId = Date.now().toString();
const seed: Seed = {
  runId,
  standardType: 'shopify--qa-pair',
  standardDefinitionPreexisting: false,
  linkedMetaobjectIds: [],
};

const requestPaths = {
  standardEnable: 'config/parity-requests/metaobjects/standard-metaobject-definition-enable-catalog.graphql',
  immutableUpdate: 'config/parity-requests/metaobjects/metaobjectDefinitionUpdate-immutable-update.graphql',
  immutableRead: 'config/parity-requests/metaobjects/metaobjectDefinitionUpdate-immutable-read.graphql',
};

const queries = Object.fromEntries(
  await Promise.all(
    Object.entries(requestPaths).map(async ([name, requestPath]) => [name, await readFile(requestPath, 'utf8')]),
  ),
) as Record<keyof typeof requestPaths, string>;

const definitionByTypeQuery = `#graphql
  query MetaobjectDefinitionUpdateImmutableByType($type: String!) {
    metaobjectDefinitionByType(type: $type) {
      id
      type
      name
      standardTemplate {
        type
        name
      }
    }
  }
`;

const deleteDefinitionMutation = `#graphql
  mutation DeleteMetaobjectDefinition($id: ID!) {
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
`;

const createDefinitionMutation = `#graphql
  mutation MetaobjectDefinitionUpdateImmutableCreateDefinition($definition: MetaobjectDefinitionCreateInput!) {
    metaobjectDefinitionCreate(definition: $definition) {
      metaobjectDefinition {
        id
        type
        name
        displayNameKey
      }
      userErrors {
        field
        message
        code
        elementKey
        elementIndex
      }
    }
  }
`;

const createMetaobjectMutation = `#graphql
  mutation MetaobjectDefinitionUpdateImmutableCreateMetaobject($metaobject: MetaobjectCreateInput!) {
    metaobjectCreate(metaobject: $metaobject) {
      metaobject {
        id
        handle
      }
      userErrors {
        field
        message
        code
        elementKey
        elementIndex
      }
    }
  }
`;

const deleteMetaobjectMutation = `#graphql
  mutation MetaobjectDefinitionUpdateImmutableDeleteMetaobject($id: ID!) {
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
`;

const createMetafieldDefinitionMutation = `#graphql
  mutation MetaobjectDefinitionUpdateImmutableCreateMetafieldDefinition($definition: MetafieldDefinitionInput!) {
    metafieldDefinitionCreate(definition: $definition) {
      createdDefinition {
        id
        namespace
        key
        type {
          name
        }
        validations {
          name
          value
        }
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const deleteMetafieldDefinitionMutation = `#graphql
  mutation MetaobjectDefinitionUpdateImmutableDeleteMetafieldDefinition(
    $id: ID!
    $deleteAllAssociatedMetafields: Boolean!
  ) {
    metafieldDefinitionDelete(id: $id, deleteAllAssociatedMetafields: $deleteAllAssociatedMetafields) {
      deletedDefinitionId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const createProductMutation = `#graphql
  mutation MetaobjectDefinitionUpdateImmutableCreateProduct($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
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

const deleteProductMutation = `#graphql
  mutation MetaobjectDefinitionUpdateImmutableDeleteProduct($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

const createProductOptionsMutation = `#graphql
  mutation MetaobjectDefinitionUpdateImmutableCreateProductOptions(
    $productId: ID!
    $options: [OptionCreateInput!]!
    $variantStrategy: ProductOptionCreateVariantStrategy
  ) {
    productOptionsCreate(productId: $productId, options: $options, variantStrategy: $variantStrategy) {
      product {
        id
        options {
          id
          name
          linkedMetafield {
            namespace
            key
          }
          optionValues {
            id
            name
            linkedMetafieldValue
          }
        }
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const createReservedDefinitionProbeMutation = `#graphql
  mutation MetaobjectDefinitionUpdateImmutableReservedProbe($definition: MetaobjectDefinitionCreateInput!) {
    metaobjectDefinitionCreate(definition: $definition) {
      metaobjectDefinition {
        id
        type
        name
      }
      userErrors {
        field
        message
        code
        elementKey
        elementIndex
      }
    }
  }
`;

function readObject(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as Record<string, unknown>) : null;
}

function readPath(value: unknown, pathParts: string[]): unknown {
  let current = value;
  for (const part of pathParts) {
    const object = readObject(current);
    if (object === null) {
      return undefined;
    }
    current = object[part];
  }

  return current;
}

function readUserErrors(payload: unknown, pathParts: string[]): unknown[] {
  const value = readPath(payload, pathParts);
  return Array.isArray(value) ? value : [];
}

function assertGraphqlOk(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || readPath(result.payload, ['errors'])) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertNoUserErrors(payload: unknown, pathParts: string[], label: string): void {
  const userErrors = readUserErrors(payload, pathParts);
  if (userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function assertHasUserErrorCode(payload: unknown, pathParts: string[], code: string, label: string): void {
  const userErrors = readUserErrors(payload, pathParts);
  if (!userErrors.some((error) => readPath(error, ['code']) === code)) {
    throw new Error(`${label} did not return ${code}: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function readStringPath(payload: unknown, pathParts: string[], label: string): string {
  const value = readPath(payload, pathParts);
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`${label} did not return a string at ${pathParts.join('.')}: ${JSON.stringify(payload, null, 2)}`);
  }

  return value;
}

function captureFromResult(
  name: string,
  query: string,
  variables: Record<string, unknown>,
  result: ConformanceGraphqlResult,
): Capture {
  return {
    name,
    request: {
      query,
      variables,
    },
    status: result.status,
    response: result.payload,
  };
}

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function captureGraphql(name: string, query: string, variables: Record<string, unknown>): Promise<Capture> {
  const result = await runGraphqlRaw(query, variables);
  assertGraphqlOk(result, name);
  return captureFromResult(name, query, variables, result);
}

async function cleanup(cleanupCaptures: Capture[]): Promise<void> {
  if (seed.linkedProductId) {
    cleanupCaptures.push(
      await captureGraphql('cleanup-linked-product-delete', deleteProductMutation, {
        input: { id: seed.linkedProductId },
      }),
    );
  }

  if (seed.linkedMetafieldDefinitionId) {
    cleanupCaptures.push(
      await captureGraphql('cleanup-linked-metafield-definition-delete', deleteMetafieldDefinitionMutation, {
        id: seed.linkedMetafieldDefinitionId,
        deleteAllAssociatedMetafields: true,
      }),
    );
  }

  for (const id of seed.linkedMetaobjectIds) {
    cleanupCaptures.push(await captureGraphql('cleanup-linked-metaobject-delete', deleteMetaobjectMutation, { id }));
  }

  if (seed.linkedDefinitionId) {
    cleanupCaptures.push(
      await captureGraphql('cleanup-linked-metaobject-definition-delete', deleteDefinitionMutation, {
        id: seed.linkedDefinitionId,
      }),
    );
  }

  if (seed.standardDefinitionId && !seed.standardDefinitionPreexisting) {
    cleanupCaptures.push(
      await captureGraphql('cleanup-standard-definition-delete', deleteDefinitionMutation, {
        id: seed.standardDefinitionId,
      }),
    );
  }
}

async function writeBlocker(stage: string, error: unknown, captures: Capture[]): Promise<void> {
  await mkdir(outputDir, { recursive: true });
  const blockerPath = path.join(outputDir, `metaobjectDefinitionUpdate-immutable-blocker-${runId}.json`);
  const message = error instanceof Error ? error.message : String(error);
  await writeFile(
    blockerPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        command: 'corepack pnpm conformance:capture -- --run metaobject-definition-update-immutable',
        blocker: {
          stage,
          message,
        },
        seed,
        partialCaptures: captures,
      },
      null,
      2,
    )}\n`,
  );
  console.error(`Wrote blocker evidence to ${blockerPath}`);
}

const captures: Capture[] = [];
const cleanupCaptures: Capture[] = [];

try {
  const beforeStandard = await captureGraphql('before-standard-definition-by-type', definitionByTypeQuery, {
    type: seed.standardType,
  });
  seed.standardDefinitionPreexisting =
    readPath(beforeStandard.response, ['data', 'metaobjectDefinitionByType', 'id']) !== undefined;
  captures.push(beforeStandard);

  const standardEnable = await captureGraphql('standard-enable', queries.standardEnable, {
    type: seed.standardType,
  });
  assertNoUserErrors(
    standardEnable.response,
    ['data', 'standardMetaobjectDefinitionEnable', 'userErrors'],
    'standard-enable',
  );
  seed.standardDefinitionId = readStringPath(
    standardEnable.response,
    ['data', 'standardMetaobjectDefinitionEnable', 'metaobjectDefinition', 'id'],
    'standard-enable',
  );
  captures.push(standardEnable);

  const standardUpdate = await captureGraphql('standard-update-immutable', queries.immutableUpdate, {
    id: seed.standardDefinitionId,
    definition: {
      name: `Immutable Rename ${runId}`,
      fieldDefinitions: [{ delete: { key: 'answer' } }],
    },
  });
  assertHasUserErrorCode(
    standardUpdate.response,
    ['data', 'metaobjectDefinitionUpdate', 'userErrors'],
    'IMMUTABLE',
    'standard-update-immutable',
  );
  captures.push(standardUpdate);

  const standardReadAfter = await captureGraphql('standard-read-after-reject', queries.immutableRead, {
    id: seed.standardDefinitionId,
  });
  captures.push(standardReadAfter);

  const reservedCreateProbe = await captureGraphql(
    'reserved-prefix-create-probe',
    createReservedDefinitionProbeMutation,
    {
      definition: {
        type: `shopify--immutable-probe-${runId}`,
        name: 'Reserved Probe',
        displayNameKey: 'title',
        fieldDefinitions: [{ key: 'title', name: 'Title', type: 'single_line_text_field', required: true }],
      },
    },
  );
  assertHasUserErrorCode(
    reservedCreateProbe.response,
    ['data', 'metaobjectDefinitionCreate', 'userErrors'],
    'NOT_AUTHORIZED',
    'reserved-prefix-create-probe',
  );
  captures.push(reservedCreateProbe);

  const linkedType = `immutable_linked_${runId}`;
  const linkedDefinition = await captureGraphql('linked-definition-create', createDefinitionMutation, {
    definition: {
      type: linkedType,
      name: 'Linked Product Option Definition',
      displayNameKey: 'label',
      fieldDefinitions: [
        { key: 'label', name: 'Label', type: 'single_line_text_field', required: true },
        { key: 'alt', name: 'Alt', type: 'single_line_text_field', required: false },
      ],
    },
  });
  assertNoUserErrors(
    linkedDefinition.response,
    ['data', 'metaobjectDefinitionCreate', 'userErrors'],
    'linked-definition-create',
  );
  seed.linkedDefinitionId = readStringPath(
    linkedDefinition.response,
    ['data', 'metaobjectDefinitionCreate', 'metaobjectDefinition', 'id'],
    'linked-definition-create',
  );
  captures.push(linkedDefinition);

  for (const label of ['One', 'Two']) {
    const metaobject = await captureGraphql('linked-metaobject-create', createMetaobjectMutation, {
      metaobject: {
        type: linkedType,
        handle: `${label.toLowerCase()}-${runId}`,
        fields: [
          { key: 'label', value: label },
          { key: 'alt', value: `Alt ${label}` },
        ],
      },
    });
    assertNoUserErrors(metaobject.response, ['data', 'metaobjectCreate', 'userErrors'], 'linked-metaobject-create');
    seed.linkedMetaobjectIds.push(
      readStringPath(metaobject.response, ['data', 'metaobjectCreate', 'metaobject', 'id'], 'linked-metaobject-create'),
    );
    captures.push(metaobject);
  }

  const linkedMetafieldNamespace = `linked_option_${runId}`;
  const linkedMetafieldDefinition = await captureGraphql(
    'linked-metafield-definition-create',
    createMetafieldDefinitionMutation,
    {
      definition: {
        ownerType: 'PRODUCT',
        namespace: linkedMetafieldNamespace,
        key: 'choice',
        name: 'Linked option choice',
        type: 'list.metaobject_reference',
        validations: [{ name: 'metaobject_definition_id', value: seed.linkedDefinitionId }],
      },
    },
  );
  assertNoUserErrors(
    linkedMetafieldDefinition.response,
    ['data', 'metafieldDefinitionCreate', 'userErrors'],
    'linked-metafield-definition-create',
  );
  seed.linkedMetafieldDefinitionId = readStringPath(
    linkedMetafieldDefinition.response,
    ['data', 'metafieldDefinitionCreate', 'createdDefinition', 'id'],
    'linked-metafield-definition-create',
  );
  captures.push(linkedMetafieldDefinition);

  const linkedProduct = await captureGraphql('linked-product-create', createProductMutation, {
    product: { title: `Linked Product Option ${runId}`, status: 'DRAFT' },
  });
  assertNoUserErrors(linkedProduct.response, ['data', 'productCreate', 'userErrors'], 'linked-product-create');
  seed.linkedProductId = readStringPath(
    linkedProduct.response,
    ['data', 'productCreate', 'product', 'id'],
    'linked-product-create',
  );
  captures.push(linkedProduct);

  const linkedProductOptions = await captureGraphql('linked-product-options-create', createProductOptionsMutation, {
    productId: seed.linkedProductId,
    variantStrategy: 'LEAVE_AS_IS',
    options: [
      {
        name: 'Linked Choice',
        linkedMetafield: {
          namespace: linkedMetafieldNamespace,
          key: 'choice',
          values: seed.linkedMetaobjectIds,
        },
      },
    ],
  });
  assertNoUserErrors(
    linkedProductOptions.response,
    ['data', 'productOptionsCreate', 'userErrors'],
    'linked-product-options-create',
  );
  captures.push(linkedProductOptions);

  const linkedDisplayNameUpdate = await captureGraphql(
    'linked-display-name-update-immutable',
    queries.immutableUpdate,
    {
      id: seed.linkedDefinitionId,
      definition: { displayNameKey: 'alt' },
    },
  );
  assertHasUserErrorCode(
    linkedDisplayNameUpdate.response,
    ['data', 'metaobjectDefinitionUpdate', 'userErrors'],
    'IMMUTABLE',
    'linked-display-name-update-immutable',
  );
  captures.push(linkedDisplayNameUpdate);

  await cleanup(cleanupCaptures);

  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        summary:
          'MetaobjectDefinitionUpdate immutability capture for standard definitions, reserved Shopify prefixes, and display-name changes on definitions linked to product options.',
        seed,
        beforeStandard,
        standardEnable,
        standardUpdate,
        standardReadAfter,
        reservedCreateProbe,
        linkedDefinition,
        linkedMetaobjects: captures.filter((capture) => capture.name === 'linked-metaobject-create'),
        linkedMetafieldDefinition,
        linkedProduct,
        linkedProductOptions,
        linkedDisplayNameUpdate,
        cleanup: cleanupCaptures,
        upstreamCalls: [],
      },
      null,
      2,
    )}\n`,
  );
  console.log(`Wrote ${outputPath}`);
} catch (error) {
  try {
    await cleanup(cleanupCaptures);
  } finally {
    await writeBlocker('capture', error, captures);
  }
  throw error;
}

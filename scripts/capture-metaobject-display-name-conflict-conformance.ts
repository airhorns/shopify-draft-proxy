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
  type: string;
  namespace: string;
  definitionId?: string;
  oneId?: string;
  twoId?: string;
  metafieldDefinitionId?: string;
  productId?: string;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metaobjects');
const outputPath = path.join(outputDir, 'metaobject-display-name-conflict.json');
const requestDir = path.join('config', 'parity-requests', 'metaobjects');
const runId = Date.now().toString();
const seed: Seed = {
  runId,
  type: `display_name_conflict_${runId}`,
  namespace: `linked_option_${runId}`,
};

const requestPaths = {
  definitionCreate: 'metaobject-display-name-conflict-definition-create.graphql',
  entryCreate: 'metaobject-display-name-conflict-entry-create.graphql',
  metafieldDefinitionCreate: 'metaobject-display-name-conflict-metafield-definition-create.graphql',
  productCreate: 'metaobject-display-name-conflict-product-create.graphql',
  productOptionsCreate: 'metaobject-display-name-conflict-product-options-create.graphql',
  update: 'metaobject-display-name-conflict-update.graphql',
  upsert: 'metaobject-display-name-conflict-upsert.graphql',
};

const documents = Object.fromEntries(
  await Promise.all(
    Object.entries(requestPaths).map(async ([name, fileName]) => [
      name,
      await readFile(path.join(requestDir, fileName), 'utf8'),
    ]),
  ),
) as Record<keyof typeof requestPaths, string>;

const metaobjectDeleteMutation = `#graphql
  mutation MetaobjectDisplayNameConflictDeleteMetaobject($id: ID!) {
    metaobjectDelete(id: $id) {
      deletedId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const definitionDeleteMutation = `#graphql
  mutation MetaobjectDisplayNameConflictDeleteDefinition($id: ID!) {
    metaobjectDefinitionDelete(id: $id) {
      deletedId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const metafieldDefinitionDeleteMutation = `#graphql
  mutation MetaobjectDisplayNameConflictDeleteMetafieldDefinition(
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

const productDeleteMutation = `#graphql
  mutation MetaobjectDisplayNameConflictDeleteProduct($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
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
    if (!object) {
      return undefined;
    }
    current = object[part];
  }
  return current;
}

function extractString(payload: unknown, pathParts: string[], label: string): string {
  const value = readPath(payload, pathParts);
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`${label} did not return a string at ${pathParts.join('.')}: ${JSON.stringify(payload, null, 2)}`);
  }
  return value;
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

function captureFromResult(
  name: string,
  query: string,
  variables: Record<string, unknown>,
  result: ConformanceGraphqlResult,
): Capture {
  return {
    name,
    request: { query, variables },
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
  if (seed.productId) {
    cleanupCaptures.push(
      await captureGraphql('cleanup-product-delete', productDeleteMutation, {
        input: { id: seed.productId },
      }),
    );
  }

  if (seed.metafieldDefinitionId) {
    cleanupCaptures.push(
      await captureGraphql('cleanup-metafield-definition-delete', metafieldDefinitionDeleteMutation, {
        id: seed.metafieldDefinitionId,
        deleteAllAssociatedMetafields: true,
      }),
    );
  }

  for (const metaobjectId of [seed.twoId, seed.oneId]) {
    if (metaobjectId) {
      cleanupCaptures.push(
        await captureGraphql('cleanup-metaobject-delete', metaobjectDeleteMutation, { id: metaobjectId }),
      );
    }
  }

  if (seed.definitionId) {
    cleanupCaptures.push(
      await captureGraphql('cleanup-definition-delete', definitionDeleteMutation, {
        id: seed.definitionId,
      }),
    );
  }
}

async function writeBlocker(stage: string, error: unknown, captures: Capture[]): Promise<void> {
  await mkdir(outputDir, { recursive: true });
  const blockerPath = path.join(outputDir, `metaobject-display-name-conflict-blocker-${runId}.json`);
  const message = error instanceof Error ? error.message : String(error);
  await writeFile(
    blockerPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        command:
          'SHOPIFY_CONFORMANCE_API_VERSION=2026-04 corepack pnpm tsx scripts/capture-metaobject-display-name-conflict-conformance.ts',
        blocker: { stage, message },
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
  const definitionCreate = await captureGraphql('definition-create', documents.definitionCreate, {
    definition: {
      type: seed.type,
      name: `Display name conflict ${runId}`,
      displayNameKey: 'label',
      fieldDefinitions: [
        { key: 'label', name: 'Label', type: 'single_line_text_field', required: true },
        { key: 'alt', name: 'Alt', type: 'single_line_text_field', required: false },
      ],
    },
  });
  assertNoUserErrors(
    definitionCreate.response,
    ['data', 'metaobjectDefinitionCreate', 'userErrors'],
    'definition-create',
  );
  seed.definitionId = extractString(
    definitionCreate.response,
    ['data', 'metaobjectDefinitionCreate', 'metaobjectDefinition', 'id'],
    'definition-create',
  );
  captures.push(definitionCreate);

  const entryCaptures: Capture[] = [];
  for (const [key, label] of [
    ['one', 'One'],
    ['two', 'Two'],
  ] as const) {
    const entry = await captureGraphql(`${key}-entry-create`, documents.entryCreate, {
      metaobject: {
        type: seed.type,
        handle: `${key}-${runId}`,
        fields: [
          { key: 'label', value: label },
          { key: 'alt', value: `Alt ${label}` },
        ],
      },
    });
    assertNoUserErrors(entry.response, ['data', 'metaobjectCreate', 'userErrors'], `${key}-entry-create`);
    const entryId = extractString(
      entry.response,
      ['data', 'metaobjectCreate', 'metaobject', 'id'],
      `${key}-entry-create`,
    );
    if (key === 'one') {
      seed.oneId = entryId;
    } else {
      seed.twoId = entryId;
    }
    entryCaptures.push(entry);
    captures.push(entry);
  }

  const metafieldDefinitionCreate = await captureGraphql(
    'metafield-definition-create',
    documents.metafieldDefinitionCreate,
    {
      definition: {
        ownerType: 'PRODUCT',
        namespace: seed.namespace,
        key: 'choice',
        name: 'Linked option choice',
        type: 'list.metaobject_reference',
        validations: [{ name: 'metaobject_definition_id', value: seed.definitionId }],
      },
    },
  );
  assertNoUserErrors(
    metafieldDefinitionCreate.response,
    ['data', 'metafieldDefinitionCreate', 'userErrors'],
    'metafield-definition-create',
  );
  seed.metafieldDefinitionId = extractString(
    metafieldDefinitionCreate.response,
    ['data', 'metafieldDefinitionCreate', 'createdDefinition', 'id'],
    'metafield-definition-create',
  );
  captures.push(metafieldDefinitionCreate);

  const productCreate = await captureGraphql('product-create', documents.productCreate, {
    product: { title: `Display name conflict ${runId}`, status: 'DRAFT' },
  });
  assertNoUserErrors(productCreate.response, ['data', 'productCreate', 'userErrors'], 'product-create');
  seed.productId = extractString(productCreate.response, ['data', 'productCreate', 'product', 'id'], 'product-create');
  captures.push(productCreate);

  const productOptionsCreate = await captureGraphql('product-options-create', documents.productOptionsCreate, {
    productId: seed.productId,
    variantStrategy: 'LEAVE_AS_IS',
    options: [
      {
        name: 'Linked Choice',
        linkedMetafield: {
          namespace: seed.namespace,
          key: 'choice',
          values: [seed.oneId, seed.twoId],
        },
      },
    ],
  });
  assertNoUserErrors(
    productOptionsCreate.response,
    ['data', 'productOptionsCreate', 'userErrors'],
    'product-options-create',
  );
  captures.push(productOptionsCreate);

  const updateConflict = await captureGraphql('update-display-name-conflict', documents.update, {
    id: seed.twoId,
    metaobject: { fields: [{ key: 'label', value: 'One' }] },
  });
  assertHasUserErrorCode(
    updateConflict.response,
    ['data', 'metaobjectUpdate', 'userErrors'],
    'DISPLAY_NAME_CONFLICT',
    'update-display-name-conflict',
  );
  captures.push(updateConflict);

  const upsertConflict = await captureGraphql('upsert-display-name-conflict', documents.upsert, {
    handle: { type: seed.type, handle: `two-${runId}` },
    metaobject: { fields: [{ key: 'label', value: 'One' }] },
  });
  assertHasUserErrorCode(
    upsertConflict.response,
    ['data', 'metaobjectUpsert', 'userErrors'],
    'DISPLAY_NAME_CONFLICT',
    'upsert-display-name-conflict',
  );
  captures.push(upsertConflict);

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
          'Metaobject display-name conflict validation for update/upsert when a competing row is referenced by a product option linked through a product metafield definition.',
        seed,
        setup: {
          definitionCreate,
          entries: entryCaptures,
          metafieldDefinitionCreate,
          productCreate,
          productOptionsCreate,
        },
        cases: {
          updateConflict,
          upsertConflict,
        },
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

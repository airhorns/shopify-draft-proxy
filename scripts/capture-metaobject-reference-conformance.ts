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

type SeedState = {
  targetType: string;
  parentType: string;
  targetDefinitionId?: string;
  parentDefinitionId?: string;
  targetAId?: string;
  targetBId?: string;
  parentId?: string;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metaobjects');
const outputPath = path.join(outputDir, 'metaobject-reference-lifecycle.json');
const runId = Date.now().toString();
const seed: SeedState = {
  targetType: `codex_har_384_ref_target_${runId}`,
  parentType: `codex_har_384_ref_parent_${runId}`,
};

const referenceReadQuery = await readFile(
  'config/parity-requests/metaobjects/metaobject-reference-read.graphql',
  'utf8',
);

const definitionReadFields = `#graphql
  fragment MetaobjectReferenceDefinitionFields on MetaobjectDefinition {
    id
    type
    name
    description
    displayNameKey
    access {
      admin
      storefront
    }
    capabilities {
      publishable {
        enabled
      }
      translatable {
        enabled
      }
      renderable {
        enabled
      }
      onlineStore {
        enabled
      }
    }
    fieldDefinitions {
      key
      name
      description
      required
      type {
        name
        category
      }
      validations {
        name
        value
      }
    }
    hasThumbnailField
    metaobjectsCount
    standardTemplate {
      type
      name
    }
  }
`;

const entryReadFields = `#graphql
  fragment MetaobjectReferenceEntryFields on Metaobject {
    id
    handle
    type
    displayName
    updatedAt
    capabilities {
      publishable {
        status
      }
      onlineStore {
        templateSuffix
      }
    }
    fields {
      key
      type
      value
      jsonValue
      definition {
        key
        name
        required
        type {
          name
          category
        }
      }
    }
  }
`;

const definitionCreateMutation = `#graphql
  ${definitionReadFields}
  mutation MetaobjectReferenceDefinitionCreate($definition: MetaobjectDefinitionCreateInput!) {
    metaobjectDefinitionCreate(definition: $definition) {
      metaobjectDefinition {
        ...MetaobjectReferenceDefinitionFields
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

const entryCreateMutation = `#graphql
  ${entryReadFields}
  mutation MetaobjectReferenceEntryCreate($metaobject: MetaobjectCreateInput!) {
    metaobjectCreate(metaobject: $metaobject) {
      metaobject {
        ...MetaobjectReferenceEntryFields
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

const entryDeleteMutation = `#graphql
  mutation MetaobjectReferenceEntryDelete($id: ID!) {
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

const definitionDeleteMutation = `#graphql
  mutation MetaobjectReferenceDefinitionDelete($id: ID!) {
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

const seedReadQuery = `#graphql
  ${definitionReadFields}
  ${entryReadFields}
  query MetaobjectReferenceSeedRead(
    $targetDefinitionId: ID!
    $parentDefinitionId: ID!
    $targetAId: ID!
    $targetBId: ID!
    $parentId: ID!
  ) {
    targetDefinition: metaobjectDefinition(id: $targetDefinitionId) {
      ...MetaobjectReferenceDefinitionFields
    }
    parentDefinition: metaobjectDefinition(id: $parentDefinitionId) {
      ...MetaobjectReferenceDefinitionFields
    }
    targetA: metaobject(id: $targetAId) {
      ...MetaobjectReferenceEntryFields
    }
    targetB: metaobject(id: $targetBId) {
      ...MetaobjectReferenceEntryFields
    }
    parent: metaobject(id: $parentId) {
      ...MetaobjectReferenceEntryFields
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
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertNoUserErrors(payload: unknown, pathParts: string[], label: string): void {
  const userErrors = readUserErrors(payload, pathParts);
  if (userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function extractId(payload: unknown, pathParts: string[], label: string): string {
  const id = readPath(payload, pathParts);
  if (typeof id !== 'string' || id.length === 0) {
    throw new Error(`${label} did not return an id: ${JSON.stringify(payload, null, 2)}`);
  }

  return id;
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

async function runSuccessMutation(
  name: string,
  query: string,
  variables: Record<string, unknown>,
  userErrorPath: string[],
): Promise<Capture> {
  const capture = await captureGraphql(name, query, variables);
  assertNoUserErrors(capture.response, userErrorPath, name);
  return capture;
}

async function writeBlocker(stage: string, error: unknown, captures: Capture[]): Promise<void> {
  await mkdir(outputDir, { recursive: true });
  const blockerPath = path.join(outputDir, `metaobject-reference-lifecycle-blocker-${runId}.json`);
  const message = error instanceof Error ? error.message : String(error);
  await writeFile(
    blockerPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        command: 'corepack pnpm conformance:capture-metaobject-references',
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

async function captureCleanup(cleanup: Capture[]): Promise<void> {
  for (const entryId of [seed.parentId, seed.targetBId, seed.targetAId]) {
    if (!entryId) {
      continue;
    }

    cleanup.push(await captureGraphql('cleanup-metaobject-delete', entryDeleteMutation, { id: entryId }));
  }

  for (const definitionId of [seed.parentDefinitionId, seed.targetDefinitionId]) {
    if (!definitionId) {
      continue;
    }

    cleanup.push(
      await captureGraphql('cleanup-metaobject-definition-delete', definitionDeleteMutation, { id: definitionId }),
    );
  }
}

const setupCaptures: Capture[] = [];
const seededReadCaptures: Capture[] = [];
const referenceReadCaptures: Capture[] = [];
const cleanupCaptures: Capture[] = [];

try {
  const targetDefinitionCreate = await runSuccessMutation(
    'setup-target-definition-create',
    definitionCreateMutation,
    {
      definition: {
        type: seed.targetType,
        name: `Codex HAR-384 Target ${runId}`,
        description: 'Temporary HAR-384 conformance definition for metaobject reference targets.',
        displayNameKey: 'title',
        fieldDefinitions: [
          {
            key: 'title',
            name: 'Title',
            description: 'Reference target title.',
            type: 'single_line_text_field',
            required: true,
          },
        ],
      },
    },
    ['data', 'metaobjectDefinitionCreate', 'userErrors'],
  );
  seed.targetDefinitionId = extractId(
    targetDefinitionCreate.response,
    ['data', 'metaobjectDefinitionCreate', 'metaobjectDefinition', 'id'],
    'target metaobjectDefinitionCreate',
  );
  setupCaptures.push(targetDefinitionCreate);

  const parentDefinitionCreate = await runSuccessMutation(
    'setup-parent-definition-create',
    definitionCreateMutation,
    {
      definition: {
        type: seed.parentType,
        name: `Codex HAR-384 Parent ${runId}`,
        description: 'Temporary HAR-384 conformance definition for metaobject reference fields.',
        displayNameKey: 'title',
        fieldDefinitions: [
          {
            key: 'title',
            name: 'Title',
            description: 'Reference parent title.',
            type: 'single_line_text_field',
            required: true,
          },
          {
            key: 'single_ref',
            name: 'Single Ref',
            description: 'Single metaobject reference field.',
            type: 'metaobject_reference',
            required: false,
            validations: [{ name: 'metaobject_definition_id', value: seed.targetDefinitionId }],
          },
          {
            key: 'list_ref',
            name: 'List Ref',
            description: 'List metaobject reference field.',
            type: 'list.metaobject_reference',
            required: false,
            validations: [{ name: 'metaobject_definition_id', value: seed.targetDefinitionId }],
          },
        ],
      },
    },
    ['data', 'metaobjectDefinitionCreate', 'userErrors'],
  );
  seed.parentDefinitionId = extractId(
    parentDefinitionCreate.response,
    ['data', 'metaobjectDefinitionCreate', 'metaobjectDefinition', 'id'],
    'parent metaobjectDefinitionCreate',
  );
  setupCaptures.push(parentDefinitionCreate);

  const targetACreate = await runSuccessMutation(
    'setup-target-a-create',
    entryCreateMutation,
    {
      metaobject: {
        type: seed.targetType,
        handle: 'target-a',
        fields: [{ key: 'title', value: `Target A ${runId}` }],
      },
    },
    ['data', 'metaobjectCreate', 'userErrors'],
  );
  seed.targetAId = extractId(
    targetACreate.response,
    ['data', 'metaobjectCreate', 'metaobject', 'id'],
    'target A create',
  );
  setupCaptures.push(targetACreate);

  const targetBCreate = await runSuccessMutation(
    'setup-target-b-create',
    entryCreateMutation,
    {
      metaobject: {
        type: seed.targetType,
        handle: 'target-b',
        fields: [{ key: 'title', value: `Target B ${runId}` }],
      },
    },
    ['data', 'metaobjectCreate', 'userErrors'],
  );
  seed.targetBId = extractId(
    targetBCreate.response,
    ['data', 'metaobjectCreate', 'metaobject', 'id'],
    'target B create',
  );
  setupCaptures.push(targetBCreate);

  const parentCreate = await runSuccessMutation(
    'setup-reference-parent-create',
    entryCreateMutation,
    {
      metaobject: {
        type: seed.parentType,
        handle: 'reference-parent',
        fields: [
          { key: 'title', value: `Reference parent ${runId}` },
          { key: 'single_ref', value: seed.targetAId },
          { key: 'list_ref', value: JSON.stringify([seed.targetAId, seed.targetBId]) },
        ],
      },
    },
    ['data', 'metaobjectCreate', 'userErrors'],
  );
  seed.parentId = extractId(parentCreate.response, ['data', 'metaobjectCreate', 'metaobject', 'id'], 'parent create');
  setupCaptures.push(parentCreate);

  const referenceReadVariables = {
    targetAId: seed.targetAId,
    targetBId: seed.targetBId,
    parentId: seed.parentId,
  };
  seededReadCaptures.push(
    await captureGraphql('seeded-reference-read-preconditions', seedReadQuery, {
      targetDefinitionId: seed.targetDefinitionId,
      parentDefinitionId: seed.parentDefinitionId,
      ...referenceReadVariables,
    }),
  );
  referenceReadCaptures.push(
    await captureGraphql('reference-field-and-referenced-by-read', referenceReadQuery, referenceReadVariables),
  );

  await captureCleanup(cleanupCaptures);

  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        seed,
        setup: setupCaptures,
        seededReads: seededReadCaptures,
        referenceReads: referenceReadCaptures,
        cleanup: cleanupCaptures,
      },
      null,
      2,
    )}\n`,
  );
  console.log(`Wrote ${outputPath}`);
} catch (error) {
  await writeBlocker('metaobject-reference-lifecycle', error, [
    ...setupCaptures,
    ...seededReadCaptures,
    ...referenceReadCaptures,
    ...cleanupCaptures,
  ]);
  try {
    await captureCleanup(cleanupCaptures);
  } catch (cleanupError) {
    console.error(cleanupError);
  }
  throw error;
}

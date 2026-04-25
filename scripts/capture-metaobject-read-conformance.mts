/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
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
  type: string;
  handle: string;
  definitionId?: string;
  entryId?: string;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion);
const outputPath = path.join(outputDir, 'metaobjects-read.json');
const runId = Date.now().toString();
const seed: SeedState = {
  type: `codex_har_240_${runId}`,
  handle: `codex-har-240-${runId}`,
};

const definitionReadFields = `#graphql
  fragment MetaobjectDefinitionReadFields on MetaobjectDefinition {
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
  fragment MetaobjectReadFields on Metaobject {
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
    titleField: field(key: "title") {
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

const schemaQuery = `#graphql
  query MetaobjectsReadSchema {
    queryRoot: __type(name: "QueryRoot") {
      fields {
        name
        args {
          name
          type {
            kind
            name
            ofType {
              kind
              name
              ofType {
                kind
                name
              }
            }
          }
        }
        type {
          kind
          name
          ofType {
            kind
            name
          }
        }
      }
    }
    metaobjectDefinition: __type(name: "MetaobjectDefinition") {
      fields {
        name
      }
    }
    metaobject: __type(name: "Metaobject") {
      fields {
        name
      }
    }
  }
`;

const missingDefinitionByTypeQuery = `#graphql
  ${definitionReadFields}
  query MetaobjectDefinitionByMissingType($type: String!) {
    metaobjectDefinitionByType(type: $type) {
      ...MetaobjectDefinitionReadFields
    }
  }
`;

const missingDefinitionByIdQuery = `#graphql
  ${definitionReadFields}
  query MetaobjectDefinitionByMissingId($id: ID!) {
    metaobjectDefinition(id: $id) {
      ...MetaobjectDefinitionReadFields
    }
  }
`;

const missingEntriesByTypeQuery = `#graphql
  ${entryReadFields}
  query MetaobjectsByMissingType($type: String!) {
    metaobjects(type: $type, first: 5) {
      edges {
        cursor
        node {
          ...MetaobjectReadFields
        }
      }
      nodes {
        ...MetaobjectReadFields
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
  }
`;

const missingEntryByHandleQuery = `#graphql
  ${entryReadFields}
  query MetaobjectByMissingHandle($handle: MetaobjectHandleInput!) {
    metaobjectByHandle(handle: $handle) {
      ...MetaobjectReadFields
    }
  }
`;

const missingEntryByIdQuery = `#graphql
  ${entryReadFields}
  query MetaobjectByMissingId($id: ID!) {
    metaobject(id: $id) {
      ...MetaobjectReadFields
    }
  }
`;

const definitionCreateMutation = `#graphql
  ${definitionReadFields}
  mutation CreateMetaobjectDefinition($definition: MetaobjectDefinitionCreateInput!) {
    metaobjectDefinitionCreate(definition: $definition) {
      metaobjectDefinition {
        ...MetaobjectDefinitionReadFields
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
  mutation CreateMetaobject($metaobject: MetaobjectCreateInput!) {
    metaobjectCreate(metaobject: $metaobject) {
      metaobject {
        ...MetaobjectReadFields
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

const definitionCatalogQuery = `#graphql
  ${definitionReadFields}
  query MetaobjectDefinitionCatalog {
    metaobjectDefinitions(first: 50, reverse: true) {
      edges {
        cursor
        node {
          ...MetaobjectDefinitionReadFields
        }
      }
      nodes {
        ...MetaobjectDefinitionReadFields
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
  }
`;

const definitionDetailQuery = `#graphql
  ${definitionReadFields}
  query MetaobjectDefinitionDetail($id: ID!) {
    metaobjectDefinition(id: $id) {
      ...MetaobjectDefinitionReadFields
    }
  }
`;

const definitionByTypeQuery = `#graphql
  ${definitionReadFields}
  query MetaobjectDefinitionByType($type: String!) {
    metaobjectDefinitionByType(type: $type) {
      ...MetaobjectDefinitionReadFields
    }
  }
`;

const entriesByTypeQuery = `#graphql
  ${entryReadFields}
  query MetaobjectsByType($type: String!) {
    metaobjects(type: $type, first: 10) {
      edges {
        cursor
        node {
          ...MetaobjectReadFields
        }
      }
      nodes {
        ...MetaobjectReadFields
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
  }
`;

const entryDetailQuery = `#graphql
  ${entryReadFields}
  query MetaobjectDetail($id: ID!) {
    metaobject(id: $id) {
      ...MetaobjectReadFields
    }
  }
`;

const entryByHandleQuery = `#graphql
  ${entryReadFields}
  query MetaobjectByHandle($handle: MetaobjectHandleInput!) {
    metaobjectByHandle(handle: $handle) {
      ...MetaobjectReadFields
    }
  }
`;

const entryDeleteMutation = `#graphql
  mutation DeleteMetaobject($id: ID!) {
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

function filterMetaobjectSchema(payload: unknown): unknown {
  const root = readObject(readPath(payload, ['data', 'queryRoot']));
  const fields = root?.['fields'];
  const metaobjectRootNames = new Set([
    'metaobject',
    'metaobjectByHandle',
    'metaobjectDefinition',
    'metaobjectDefinitionByType',
    'metaobjectDefinitions',
    'metaobjects',
  ]);

  return {
    ...readObject(payload),
    data: {
      ...readObject(readPath(payload, ['data'])),
      queryRoot: {
        ...root,
        fields: Array.isArray(fields)
          ? fields.filter((field) => metaobjectRootNames.has(String(readObject(field)?.['name'] ?? '')))
          : fields,
      },
    },
  };
}

function captureFromResult(
  name: string,
  query: string,
  variables: Record<string, unknown>,
  result: ConformanceGraphqlResult,
  response: unknown = result.payload,
): Capture {
  return {
    name,
    request: {
      query,
      variables,
    },
    status: result.status,
    response,
  };
}

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function captureGraphql(name: string, query: string, variables: Record<string, unknown> = {}): Promise<Capture> {
  const result = await runGraphqlRaw(query, variables);
  assertGraphqlOk(result, name);
  return captureFromResult(name, query, variables, result);
}

async function runSetupMutation(
  name: string,
  query: string,
  variables: Record<string, unknown>,
  userErrorPath: string[],
): Promise<Capture> {
  const result = await runGraphqlRaw(query, variables);
  assertGraphqlOk(result, name);
  assertNoUserErrors(result.payload, userErrorPath, name);
  return captureFromResult(name, query, variables, result);
}

async function writeBlocker(stage: string, error: unknown, captures: Capture[]): Promise<void> {
  await mkdir(outputDir, { recursive: true });
  const blockerPath = path.join(outputDir, `metaobjects-read-blocker-${runId}.json`);
  const message = error instanceof Error ? error.message : String(error);
  await writeFile(
    blockerPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        command: 'corepack pnpm conformance:capture-metaobjects',
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
  if (seed.entryId) {
    cleanup.push(await captureGraphql('cleanup-metaobject-delete', entryDeleteMutation, { id: seed.entryId }));
  }

  if (seed.definitionId) {
    cleanup.push(
      await captureGraphql('cleanup-metaobject-definition-delete', definitionDeleteMutation, {
        id: seed.definitionId,
      }),
    );
  }
}

const schemaResult = await runGraphqlRaw(schemaQuery, {});
assertGraphqlOk(schemaResult, 'metaobjects read schema');
const schema = captureFromResult('schema', schemaQuery, {}, schemaResult, filterMetaobjectSchema(schemaResult.payload));

const noDataCaptures: Capture[] = [];
const setupCaptures: Capture[] = [];
const seededReadCaptures: Capture[] = [];
const cleanupCaptures: Capture[] = [];

try {
  noDataCaptures.push(
    await captureGraphql('missing-definition-by-type', missingDefinitionByTypeQuery, { type: seed.type }),
  );
  noDataCaptures.push(
    await captureGraphql('missing-definition-by-id', missingDefinitionByIdQuery, {
      id: 'gid://shopify/MetaobjectDefinition/0',
    }),
  );
  noDataCaptures.push(await captureGraphql('empty-entries-by-type', missingEntriesByTypeQuery, { type: seed.type }));
  noDataCaptures.push(
    await captureGraphql('missing-entry-by-handle', missingEntryByHandleQuery, {
      handle: {
        type: seed.type,
        handle: `${seed.handle}-missing`,
      },
    }),
  );
  noDataCaptures.push(
    await captureGraphql('missing-entry-by-id', missingEntryByIdQuery, {
      id: 'gid://shopify/Metaobject/0',
    }),
  );

  const definitionCreate = await runSetupMutation(
    'setup-metaobject-definition-create',
    definitionCreateMutation,
    {
      definition: {
        type: seed.type,
        name: `Codex HAR-240 ${runId}`,
        description: 'Temporary HAR-240 conformance definition for metaobject read fixture capture.',
        capabilities: {
          publishable: {
            enabled: true,
          },
          translatable: {
            enabled: false,
          },
        },
        displayNameKey: 'title',
        fieldDefinitions: [
          {
            key: 'title',
            name: 'Title',
            description: 'Display title for HAR-240 fixture capture.',
            type: 'single_line_text_field',
            required: true,
          },
          {
            key: 'body',
            name: 'Body',
            description: 'Body text for HAR-240 fixture capture.',
            type: 'multi_line_text_field',
            required: false,
          },
        ],
      },
    },
    ['data', 'metaobjectDefinitionCreate', 'userErrors'],
  );
  seed.definitionId = extractId(
    definitionCreate.response,
    ['data', 'metaobjectDefinitionCreate', 'metaobjectDefinition', 'id'],
    'metaobjectDefinitionCreate',
  );
  setupCaptures.push(definitionCreate);

  const entryCreate = await runSetupMutation(
    'setup-metaobject-create',
    entryCreateMutation,
    {
      metaobject: {
        type: seed.type,
        handle: seed.handle,
        capabilities: {
          publishable: {
            status: 'ACTIVE',
          },
        },
        fields: [
          {
            key: 'title',
            value: `HAR-240 title ${runId}`,
          },
          {
            key: 'body',
            value: `HAR-240 body ${runId}`,
          },
        ],
      },
    },
    ['data', 'metaobjectCreate', 'userErrors'],
  );
  seed.entryId = extractId(entryCreate.response, ['data', 'metaobjectCreate', 'metaobject', 'id'], 'metaobjectCreate');
  setupCaptures.push(entryCreate);

  seededReadCaptures.push(await captureGraphql('definition-catalog', definitionCatalogQuery));
  seededReadCaptures.push(await captureGraphql('definition-detail', definitionDetailQuery, { id: seed.definitionId }));
  seededReadCaptures.push(await captureGraphql('definition-by-type', definitionByTypeQuery, { type: seed.type }));
  seededReadCaptures.push(await captureGraphql('entries-by-type', entriesByTypeQuery, { type: seed.type }));
  seededReadCaptures.push(await captureGraphql('entry-detail', entryDetailQuery, { id: seed.entryId }));
  seededReadCaptures.push(
    await captureGraphql('entry-by-handle', entryByHandleQuery, {
      handle: {
        type: seed.type,
        handle: seed.handle,
      },
    }),
  );

  await captureCleanup(cleanupCaptures);

  const fixture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    seed,
    safety: {
      setup:
        'Creates one temporary metaobject definition and one temporary metaobject entry on the disposable conformance shop, then deletes both before writing the successful fixture.',
      paritySpecs:
        'No parity spec is checked in for this fixture yet because the local proxy has no executable metaobject snapshot/read model to compare against without inventing data.',
    },
    schema,
    noData: noDataCaptures,
    setup: setupCaptures,
    seededReads: seededReadCaptures,
    cleanup: cleanupCaptures,
  };

  await mkdir(outputDir, { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`);
  console.log(`Wrote ${outputPath}`);
} catch (error) {
  try {
    await captureCleanup(cleanupCaptures);
  } catch (cleanupError) {
    cleanupCaptures.push({
      name: 'cleanup-failure',
      request: {
        query: '',
        variables: {},
      },
      status: 0,
      response: cleanupError instanceof Error ? cleanupError.message : String(cleanupError),
    });
  }

  await writeBlocker('metaobjects read capture', error, [
    ...noDataCaptures,
    ...setupCaptures,
    ...seededReadCaptures,
    ...cleanupCaptures,
  ]);
  throw error;
}

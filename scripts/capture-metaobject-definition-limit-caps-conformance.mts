/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';

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

type DefinitionNode = {
  id: string;
  type?: string | null;
  name?: string | null;
  standardTemplate?: {
    type?: string | null;
    name?: string | null;
  } | null;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metaobjects');
const outputPath = path.join(outputDir, 'metaobject-definition-limit-caps.json');
const runId = Date.now().toString(36);
const maxDefinitionProbeCreates = 160;

const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const readDefinitionsQuery = `#graphql
  query MetaobjectDefinitionsForLimitCaps($first: Int!, $after: String) {
    metaobjectDefinitions(first: $first, after: $after) {
      nodes {
        id
        type
        name
        standardTemplate {
          type
          name
        }
      }
      pageInfo {
        hasNextPage
        endCursor
      }
    }
  }
`;

const createDefinitionMutation = `#graphql
  mutation CreateMetaobjectDefinitionForLimitCaps($definition: MetaobjectDefinitionCreateInput!) {
    metaobjectDefinitionCreate(definition: $definition) {
      metaobjectDefinition {
        id
        type
        name
        fieldDefinitions {
          key
          capabilities {
            adminFilterable {
              enabled
            }
          }
        }
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

const deleteDefinitionMutation = `#graphql
  mutation DeleteMetaobjectDefinitionForLimitCaps($id: ID!) {
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

function readString(value: unknown): string | null {
  return typeof value === 'string' ? value : null;
}

function readNumber(value: unknown): number | null {
  return typeof value === 'number' ? value : null;
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

function assertGraphqlOk(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || readPath(result.payload, ['errors'])) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function userErrorCodes(capture: Capture): string[] {
  const errors = readPath(capture.response, ['data', 'metaobjectDefinitionCreate', 'userErrors']);
  if (!Array.isArray(errors)) {
    return [];
  }

  return errors.map((error) => readString(readPath(error, ['code']))).filter((code): code is string => code !== null);
}

function createdDefinitionId(capture: Capture): string | null {
  return readString(readPath(capture.response, ['data', 'metaobjectDefinitionCreate', 'metaobjectDefinition', 'id']));
}

function definitionNodes(capture: Capture): DefinitionNode[] {
  const nodes = readPath(capture.response, ['data', 'metaobjectDefinitions', 'nodes']);
  return Array.isArray(nodes) ? (nodes as DefinitionNode[]) : [];
}

function hasNextPage(capture: Capture): boolean {
  return readPath(capture.response, ['data', 'metaobjectDefinitions', 'pageInfo', 'hasNextPage']) === true;
}

function endCursor(capture: Capture): string | null {
  return readString(readPath(capture.response, ['data', 'metaobjectDefinitions', 'pageInfo', 'endCursor']));
}

async function waitForThrottle(result: ConformanceGraphqlResult): Promise<void> {
  const currentlyAvailable = readNumber(
    readPath(result.payload, ['extensions', 'cost', 'throttleStatus', 'currentlyAvailable']),
  );
  const restoreRate = readNumber(readPath(result.payload, ['extensions', 'cost', 'throttleStatus', 'restoreRate']));
  if (currentlyAvailable === null || restoreRate === null || restoreRate <= 0 || currentlyAvailable >= 100) {
    return;
  }

  await sleep(Math.ceil(((100 - currentlyAvailable) / restoreRate) * 1000));
}

async function captureGraphql(name: string, query: string, variables: Record<string, unknown>): Promise<Capture> {
  const result = await runGraphqlRaw(query, variables);
  await waitForThrottle(result);
  assertGraphqlOk(result, name);
  return captureFromResult(name, query, variables, result);
}

async function readAllDefinitions(name: string): Promise<Capture[]> {
  const captures: Capture[] = [];
  let after: string | null = null;

  for (;;) {
    const capture = await captureGraphql(`${name}-${captures.length + 1}`, readDefinitionsQuery, {
      first: 250,
      after,
    });
    captures.push(capture);
    if (!hasNextPage(capture)) {
      return captures;
    }
    after = endCursor(capture);
  }
}

function fieldDefinition(index: number, adminFilterable: boolean): Record<string, unknown> {
  return {
    key: `field_${index.toString().padStart(3, '0')}`,
    name: `Field ${index}`,
    type: 'single_line_text_field',
    capabilities: {
      adminFilterable: {
        enabled: adminFilterable,
      },
    },
  };
}

function createDefinitionVariables(
  type: string,
  fieldDefinitions: Array<Record<string, unknown>>,
): Record<string, unknown> {
  return {
    definition: {
      type,
      name: type,
      displayNameKey: 'field_000',
      fieldDefinitions,
    },
  };
}

function singleFieldDefinitionVariables(type: string): Record<string, unknown> {
  return createDefinitionVariables(type, [fieldDefinition(0, false)]);
}

async function deleteDefinition(id: string): Promise<Capture> {
  const result = await runGraphqlRaw(deleteDefinitionMutation, { id });
  await waitForThrottle(result);
  return captureFromResult(`cleanup-${id.split('/').at(-1) ?? id}`, deleteDefinitionMutation, { id }, result);
}

async function deleteCreatedDefinitions(ids: string[]): Promise<Capture[]> {
  const cleanup: Capture[] = [];
  for (const id of ids) {
    cleanup.push(await deleteDefinition(id));
  }
  return cleanup;
}

const fieldProbeIds: string[] = [];
const shopLimitDefinitionIds: string[] = [];
const fieldProbeCleanup: Capture[] = [];
let fortyAdminFilterableFields: Capture | null = null;
let fortyOneAdminFilterableFields: Capture | null = null;
let reservedShopifyFormFields: Capture | null = null;
let preflightCatalog: Capture[] = [];
let shopLimitAttempts: Capture[] = [];
let limitAttempt: Capture | null = null;
let shopLimitCleanup: Capture[] = [];
let postCleanupCatalog: Capture[] = [];

try {
  fortyAdminFilterableFields = await captureGraphql(
    'forty-admin-filterable-fields',
    createDefinitionMutation,
    createDefinitionVariables(
      `definition_limit_admin_filterable_40_${runId}`,
      Array.from({ length: 40 }, (_, index) => fieldDefinition(index, true)),
    ),
  );
  const fortyId = createdDefinitionId(fortyAdminFilterableFields);
  if (fortyId === null || userErrorCodes(fortyAdminFilterableFields).length > 0) {
    throw new Error(
      `Expected 40 admin-filterable fields to be accepted: ${JSON.stringify(
        fortyAdminFilterableFields.response,
        null,
        2,
      )}`,
    );
  }
  fieldProbeIds.push(fortyId);

  fortyOneAdminFilterableFields = await captureGraphql(
    'forty-one-admin-filterable-fields',
    createDefinitionMutation,
    createDefinitionVariables(
      `definition_limit_admin_filterable_41_${runId}`,
      Array.from({ length: 41 }, (_, index) => fieldDefinition(index, true)),
    ),
  );
  if (
    createdDefinitionId(fortyOneAdminFilterableFields) !== null ||
    !userErrorCodes(fortyOneAdminFilterableFields).includes('INVALID')
  ) {
    throw new Error(
      `Expected 41 admin-filterable fields to be rejected by Shopify: ${JSON.stringify(
        fortyOneAdminFilterableFields.response,
        null,
        2,
      )}`,
    );
  }

  reservedShopifyFormFields = await captureGraphql(
    'reserved-shopify-form-fields',
    createDefinitionMutation,
    createDefinitionVariables(
      `shopify--form-definition-limit-${runId}`,
      Array.from({ length: 101 }, (_, index) => fieldDefinition(index, false)),
    ),
  );
  if (
    createdDefinitionId(reservedShopifyFormFields) !== null ||
    !userErrorCodes(reservedShopifyFormFields).includes('NOT_AUTHORIZED')
  ) {
    throw new Error(
      `Expected shopify--form create to be rejected before field-limit validation: ${JSON.stringify(
        reservedShopifyFormFields.response,
        null,
        2,
      )}`,
    );
  }
} finally {
  fieldProbeCleanup.push(...(await deleteCreatedDefinitions([...fieldProbeIds].reverse())));
}

try {
  preflightCatalog = await readAllDefinitions('preflight-metaobject-definitions');

  for (let index = 0; index < maxDefinitionProbeCreates; index += 1) {
    const capture = await captureGraphql(
      `definition-shop-limit-create-${index + 1}`,
      createDefinitionMutation,
      singleFieldDefinitionVariables(`definition_limit_shop_${runId}_${index.toString().padStart(3, '0')}`),
    );
    shopLimitAttempts.push(capture);

    const id = createdDefinitionId(capture);
    if (id !== null) {
      shopLimitDefinitionIds.push(id);
      continue;
    }

    if (userErrorCodes(capture).includes('MAX_DEFINITIONS_EXCEEDED')) {
      limitAttempt = capture;
      break;
    }

    throw new Error(`Unexpected metaobject definition create response: ${JSON.stringify(capture.response, null, 2)}`);
  }

  if (limitAttempt === null) {
    throw new Error(`Did not observe MAX_DEFINITIONS_EXCEEDED after ${maxDefinitionProbeCreates} create attempts.`);
  }
} finally {
  shopLimitCleanup = await deleteCreatedDefinitions([...shopLimitDefinitionIds].reverse());
  postCleanupCatalog = await readAllDefinitions('post-cleanup-metaobject-definitions');
}

const preflightCount = preflightCatalog.flatMap((capture) => definitionNodes(capture)).length;
const preflightStandardCount = preflightCatalog
  .flatMap((capture) => definitionNodes(capture))
  .filter((definition) => definition.standardTemplate !== null && definition.standardTemplate !== undefined).length;
const preflightCountedDefinitionCount = preflightCount - preflightStandardCount;
const shopLimitSuccessCount = shopLimitAttempts.filter((capture) => createdDefinitionId(capture) !== null).length;
const observedShopDefinitionBoundary = preflightCountedDefinitionCount + shopLimitSuccessCount;

if (observedShopDefinitionBoundary !== 128) {
  throw new Error(
    `Expected metaobject definition shop limit at 128; observed ${JSON.stringify({
      preflightCount,
      preflightStandardCount,
      preflightCountedDefinitionCount,
      shopLimitSuccessCount,
      observedShopDefinitionBoundary,
      limitCodes: limitAttempt ? userErrorCodes(limitAttempt) : [],
    })}`,
  );
}

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      summary:
        'MetaobjectDefinitionCreate limit capture for public Admin 2026-04. The script proves 40 admin-filterable fields are accepted, 41 field definitions are rejected, shopify--form public creates are rejected before field-count validation, and the live shop definition boundary returns MAX_DEFINITIONS_EXCEEDED at 128 definitions.',
      seed: {
        runId,
        maxDefinitionProbeCreates,
      },
      fieldCaps: {
        fortyAdminFilterableFields,
        fortyOneAdminFilterableFields,
        reservedShopifyFormFields,
        cleanup: fieldProbeCleanup,
      },
      definitionShopLimit: {
        observed: {
          preflightDefinitionCount: preflightCount,
          preflightStandardDefinitionCount: preflightStandardCount,
          preflightCountedDefinitionCount,
          acceptedCreatesBeforeLimit: shopLimitSuccessCount,
          observedShopDefinitionBoundary,
        },
        preflightCatalog,
        createAttempts: shopLimitAttempts,
        limitAttempt,
        cleanup: shopLimitCleanup,
        postCleanupCatalog,
      },
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
      storeDomain,
      apiVersion,
      fieldProbeCleanupCount: fieldProbeCleanup.length,
      preflightCount,
      preflightStandardCount,
      preflightCountedDefinitionCount,
      shopLimitSuccessCount,
      observedShopDefinitionBoundary,
      shopLimitCleanupCount: shopLimitCleanup.length,
    },
    null,
    2,
  ),
);

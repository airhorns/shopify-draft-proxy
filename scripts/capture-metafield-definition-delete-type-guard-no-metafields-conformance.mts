/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const scenarioId = 'metafield-definition-delete-type-guard-no-metafields';
const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metafields');
const fixturePath = path.join(outputDir, `${scenarioId}.json`);
const specPath = path.join('config', 'parity-specs', 'metafields', `${scenarioId}.json`);
const createDocumentPath = `config/parity-requests/metafields/${scenarioId}-create.graphql`;
const deleteNoFlagDocumentPath = `config/parity-requests/metafields/${scenarioId}-delete-no-flag.graphql`;
const deleteWithFlagDocumentPath = `config/parity-requests/metafields/${scenarioId}-delete-with-flag.graphql`;

const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

type CapturedInteraction = {
  request: {
    documentPath?: string;
    query?: string;
    variables: Record<string, unknown>;
  };
  status: number;
  response: unknown;
};

type CapturedCase = {
  namespace: string;
  key: string;
  type: string;
  create: CapturedInteraction;
  delete: CapturedInteraction;
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

const cleanupDefinitionMutation = `#graphql
  mutation MetafieldDefinitionDeleteTypeGuardNoMetafieldsCleanup($id: ID!) {
    metafieldDefinitionDelete(id: $id, deleteAllAssociatedMetafields: true) {
      deletedDefinitionId
      userErrors { field message code }
    }
  }
`;

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

async function captureCase(
  label: string,
  suffix: string,
  type: string,
  deleteDocumentPath: string,
  deleteVariables: (definitionId: string) => Record<string, unknown>,
): Promise<CapturedCase> {
  const namespace = `${suffix}_${label}`;
  const key = label === 'list_reference_no_values' ? 'targets' : label === 'id_no_values' ? 'uid' : 'target';
  const create = await captureDocument(`${label} metafieldDefinitionCreate`, createDocumentPath, {
    definition: {
      name: `Delete type guard no metafields ${label}`,
      namespace,
      key,
      ownerType: 'PRODUCT',
      type,
    },
  });
  const definitionId = createdDefinitionId(create);
  const deleteCapture = await captureDocument(
    `${label} metafieldDefinitionDelete`,
    deleteDocumentPath,
    deleteVariables(definitionId),
  );
  return { namespace, key, type, create, delete: deleteCapture };
}

async function cleanup(definitionId: string | null): Promise<CapturedInteraction[]> {
  if (definitionId === null) {
    return [];
  }
  return [
    await captureQuery('cleanup metafieldDefinitionDelete', cleanupDefinitionMutation, { id: definitionId }).catch(
      (error: unknown) => ({
        request: { query: cleanupDefinitionMutation, variables: { id: definitionId } },
        status: 0,
        response: { error: String(error) },
      }),
    ),
  ];
}

const suffix = `metafield_type_guard_${Date.now().toString(36)}`;
const cleanupCaptures: CapturedInteraction[] = [];

const idNoValues = await captureCase('id_no_values', suffix, 'id', deleteNoFlagDocumentPath, (definitionId) => ({
  id: definitionId,
}));
cleanupCaptures.push(
  ...(await cleanup(deleteSucceeded(idNoValues.delete) ? null : createdDefinitionId(idNoValues.create))),
);

const referenceNoValues = await captureCase(
  'reference_no_values',
  suffix,
  'product_reference',
  deleteWithFlagDocumentPath,
  (definitionId) => ({ id: definitionId, deleteAllAssociatedMetafields: false }),
);
cleanupCaptures.push(
  ...(await cleanup(deleteSucceeded(referenceNoValues.delete) ? null : createdDefinitionId(referenceNoValues.create))),
);

const listReferenceNoValues = await captureCase(
  'list_reference_no_values',
  suffix,
  'list.product_reference',
  deleteNoFlagDocumentPath,
  (definitionId) => ({ id: definitionId }),
);
cleanupCaptures.push(
  ...(await cleanup(
    deleteSucceeded(listReferenceNoValues.delete) ? null : createdDefinitionId(listReferenceNoValues.create),
  )),
);

const referenceWithFlag = await captureCase(
  'reference_with_flag',
  suffix,
  'product_reference',
  deleteWithFlagDocumentPath,
  (definitionId) => ({ id: definitionId, deleteAllAssociatedMetafields: true }),
);
cleanupCaptures.push(
  ...(await cleanup(deleteSucceeded(referenceWithFlag.delete) ? null : createdDefinitionId(referenceWithFlag.create))),
);

const fixture = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  suffix,
  cases: {
    idNoValues,
    referenceNoValues,
    listReferenceNoValues,
    referenceWithFlag,
  },
  cleanup: cleanupCaptures,
  upstreamCalls: [],
};

const spec = {
  scenarioId,
  operationNames: ['metafieldDefinitionCreate', 'metafieldDefinitionDelete'],
  scenarioStatus: 'captured',
  assertionKinds: ['payload-shape', 'validation-errors', 'runtime-staging', 'no-upstream-passthrough'],
  liveCaptureFiles: [fixturePath],
  proxyRequest: {
    documentPath: createDocumentPath,
    apiVersion,
    variablesCapturePath: '$.cases.idNoValues.create.request.variables',
  },
  comparisonMode: 'captured-vs-proxy-request',
  comparison: {
    mode: 'strict-json',
    expectedDifferences: [],
    targets: [
      {
        name: 'id-no-values-create-setup',
        capturePath: '$.cases.idNoValues.create.response.data.metafieldDefinitionCreate',
        proxyPath: '$.data.metafieldDefinitionCreate',
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
        name: 'id-no-values-delete-userErrors',
        capturePath: '$.cases.idNoValues.delete.response.data.metafieldDefinitionDelete',
        proxyPath: '$.data.metafieldDefinitionDelete',
        proxyRequest: {
          documentPath: deleteNoFlagDocumentPath,
          apiVersion,
          variables: {
            id: {
              fromProxyResponse: 'id-no-values-create-setup',
              path: '$.data.metafieldDefinitionCreate.createdDefinition.id',
            },
          },
        },
      },
      {
        name: 'reference-no-values-create-setup',
        capturePath: '$.cases.referenceNoValues.create.response.data.metafieldDefinitionCreate',
        proxyPath: '$.data.metafieldDefinitionCreate',
        proxyRequest: {
          documentPath: createDocumentPath,
          apiVersion,
          variablesCapturePath: '$.cases.referenceNoValues.create.request.variables',
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
        name: 'reference-no-values-false-flag-userErrors',
        capturePath: '$.cases.referenceNoValues.delete.response.data.metafieldDefinitionDelete',
        proxyPath: '$.data.metafieldDefinitionDelete',
        proxyRequest: {
          documentPath: deleteWithFlagDocumentPath,
          apiVersion,
          variables: {
            id: {
              fromProxyResponse: 'reference-no-values-create-setup',
              path: '$.data.metafieldDefinitionCreate.createdDefinition.id',
            },
            deleteAllAssociatedMetafields: false,
          },
        },
      },
      {
        name: 'list-reference-no-values-create-setup',
        capturePath: '$.cases.listReferenceNoValues.create.response.data.metafieldDefinitionCreate',
        proxyPath: '$.data.metafieldDefinitionCreate',
        proxyRequest: {
          documentPath: createDocumentPath,
          apiVersion,
          variablesCapturePath: '$.cases.listReferenceNoValues.create.request.variables',
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
        name: 'list-reference-no-values-delete-userErrors',
        capturePath: '$.cases.listReferenceNoValues.delete.response.data.metafieldDefinitionDelete',
        proxyPath: '$.data.metafieldDefinitionDelete',
        proxyRequest: {
          documentPath: deleteNoFlagDocumentPath,
          apiVersion,
          variables: {
            id: {
              fromProxyResponse: 'list-reference-no-values-create-setup',
              path: '$.data.metafieldDefinitionCreate.createdDefinition.id',
            },
          },
        },
      },
      {
        name: 'reference-with-flag-create-setup',
        capturePath: '$.cases.referenceWithFlag.create.response.data.metafieldDefinitionCreate',
        proxyPath: '$.data.metafieldDefinitionCreate',
        proxyRequest: {
          documentPath: createDocumentPath,
          apiVersion,
          variablesCapturePath: '$.cases.referenceWithFlag.create.request.variables',
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
        name: 'reference-with-flag-delete-success',
        capturePath: '$.cases.referenceWithFlag.delete.response.data.metafieldDefinitionDelete',
        proxyPath: '$.data.metafieldDefinitionDelete',
        proxyRequest: {
          documentPath: deleteWithFlagDocumentPath,
          apiVersion,
          variables: {
            id: {
              fromProxyResponse: 'reference-with-flag-create-setup',
              path: '$.data.metafieldDefinitionCreate.createdDefinition.id',
            },
            deleteAllAssociatedMetafields: true,
          },
        },
        expectedDifferences: [
          {
            path: '$.deletedDefinitionId',
            matcher: 'shopify-gid:MetafieldDefinition',
            reason:
              'The proxy deletes the locally staged synthetic definition ID while Shopify deleted the live-store definition ID.',
          },
        ],
      },
    ],
  },
  runtimeTestFiles: ['tests/graphql_routes/metafield_definitions.rs'],
  notes:
    'Recorded Shopify Admin behavior for PRODUCT id, product_reference, and list.product_reference definitions with no associated metafields. When deleteAllAssociatedMetafields is omitted or false, Shopify returns ID_TYPE_DELETION_ERROR or REFERENCE_TYPE_DELETION_ERROR and leaves the definition in place. With deleteAllAssociatedMetafields true, Shopify deletes the definition successfully.',
};

await mkdir(outputDir, { recursive: true });
await mkdir(path.dirname(specPath), { recursive: true });
await writeFile(fixturePath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
await writeFile(specPath, `${JSON.stringify(spec, null, 2)}\n`, 'utf8');

console.log(
  JSON.stringify(
    {
      ok: true,
      fixturePath,
      specPath,
      apiVersion,
      suffix,
      cleanupCount: cleanupCaptures.length,
    },
    null,
    2,
  ),
);

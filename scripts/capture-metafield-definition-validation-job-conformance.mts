/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
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

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const requestDir = path.join('config', 'parity-requests', 'metafield-definitions');
const specDir = path.join('config', 'parity-specs', 'metafield-definitions');
const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metafields');

const createDocumentPath = path.join(requestDir, 'validation-job-create.graphql');
const metafieldsSetDocumentPath = path.join(requestDir, 'validation-job-metafields-set.graphql');
const updateDocumentPath = path.join(requestDir, 'validation-job-update.graphql');
const jobReadDocumentPath = path.join(requestDir, 'validation-job-read.graphql');
const renameDocumentPath = path.join(requestDir, 'validation-job-rename.graphql');
const specPath = path.join(specDir, 'validation-job.json');
const fixturePath = path.join(fixtureDir, 'metafield-definition-validation-job.json');

const productCreateMutation = `#graphql
mutation ValidationJobProductCreate($product: ProductCreateInput!) {
  productCreate(product: $product) {
    product { id title }
    userErrors { field message }
  }
}
`;

const productDeleteMutation = `#graphql
mutation ValidationJobProductDelete($input: ProductDeleteInput!) {
  productDelete(input: $input) {
    deletedProductId
    userErrors { field message }
  }
}
`;

const deleteDefinitionMutation = `#graphql
mutation ValidationJobDefinitionDelete($id: ID!) {
  metafieldDefinitionDelete(id: $id, deleteAllAssociatedMetafields: true) {
    deletedDefinitionId
    userErrors { field message code }
  }
}
`;

const createDocument = `mutation MetafieldDefinitionValidationJobCreate($definition: MetafieldDefinitionInput!) {
  metafieldDefinitionCreate(definition: $definition) {
    createdDefinition {
      id
      namespace
      key
      ownerType
      validations { name value }
      validationStatus
    }
    userErrors { field message code }
  }
}
`;

const metafieldsSetDocument = `mutation MetafieldDefinitionValidationJobMetafieldsSet($metafields: [MetafieldsSetInput!]!) {
  metafieldsSet(metafields: $metafields) {
    metafields {
      id
      ownerType
      namespace
      key
      type
      value
    }
    userErrors { field message code }
  }
}
`;

const updateDocument = `mutation MetafieldDefinitionValidationJobUpdate($definition: MetafieldDefinitionUpdateInput!) {
  metafieldDefinitionUpdate(definition: $definition) {
    updatedDefinition {
      id
      namespace
      key
      ownerType
      validations { name value }
      validationStatus
    }
    validationJob { __typename id done query { __typename } }
    userErrors { field message code }
  }
}
`;

const jobReadDocument = `query MetafieldDefinitionValidationJobRead($id: ID!) {
  job(id: $id) {
    __typename
    id
    done
    query { __typename }
  }
}
`;

const renameDocument = `mutation MetafieldDefinitionValidationJobRename($definition: MetafieldDefinitionUpdateInput!) {
  metafieldDefinitionUpdate(definition: $definition) {
    updatedDefinition {
      id
      name
      namespace
      key
      ownerType
      validations { name value }
      validationStatus
    }
    validationJob { __typename id done query { __typename } }
    userErrors { field message code }
  }
}
`;

const payloadSchemaQuery = `#graphql
query MetafieldDefinitionValidationJobPayloadSchema {
  createPayload: __type(name: "MetafieldDefinitionCreatePayload") { fields { name } }
  updatePayload: __type(name: "MetafieldDefinitionUpdatePayload") { fields { name } }
  definition: __type(name: "MetafieldDefinition") { fields { name } }
}
`;

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
  for (const part of parts) {
    cursor = readObject(cursor)?.[part];
  }
  return cursor;
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
  query: string,
  variables: Record<string, unknown>,
): Promise<CapturedInteraction> {
  const result = await runGraphqlRaw(query, variables);
  assertHttpOk(result, label);
  return {
    request: { documentPath, variables },
    status: result.status,
    response: result.payload,
  };
}

function requireString(value: unknown, label: string): string {
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`${label} was not returned: ${JSON.stringify(value)}`);
  }
  return value;
}

const suffix = Date.now().toString(36);
const namespace = `validation_job_${suffix}`;
const key = 'probe';
let productId: string | null = null;
let definitionId: string | null = null;
const cleanup: CapturedInteraction[] = [];
let schema: CapturedInteraction | null = null;
let productCreate: CapturedInteraction | null = null;
let create: CapturedInteraction | null = null;
let metafieldsSet: CapturedInteraction | null = null;
let validationUpdate: CapturedInteraction | null = null;
let jobRead: CapturedInteraction | null = null;
let rename: CapturedInteraction | null = null;

try {
  schema = await captureQuery('payload schema', payloadSchemaQuery, {});

  productCreate = await captureQuery('productCreate setup', productCreateMutation, {
    product: { title: `validation job ${suffix}` },
  });
  productId = requireString(readPath(productCreate.response, ['data', 'productCreate', 'product', 'id']), 'product id');

  create = await captureDocument('metafieldDefinitionCreate setup', createDocumentPath, createDocument, {
    definition: {
      name: 'Validation Job Definition',
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

  metafieldsSet = await captureDocument(
    'metafieldsSet matching metafield',
    metafieldsSetDocumentPath,
    metafieldsSetDocument,
    {
      metafields: [
        {
          ownerId: productId,
          namespace,
          key,
          type: 'single_line_text_field',
          value: 'ABCDE',
        },
      ],
    },
  );

  validationUpdate = await captureDocument(
    'metafieldDefinitionUpdate validation change',
    updateDocumentPath,
    updateDocument,
    {
      definition: {
        namespace,
        key,
        ownerType: 'PRODUCT',
        validations: [{ name: 'max', value: '8' }],
      },
    },
  );
  const jobId = requireString(
    readPath(validationUpdate.response, ['data', 'metafieldDefinitionUpdate', 'validationJob', 'id']),
    'validation job id',
  );

  jobRead = await captureDocument('job readback', jobReadDocumentPath, jobReadDocument, { id: jobId });

  rename = await captureDocument('metafieldDefinitionUpdate no validation change', renameDocumentPath, renameDocument, {
    definition: {
      namespace,
      key,
      ownerType: 'PRODUCT',
      name: 'Validation Job Definition Renamed',
    },
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

function requireCapture(value: CapturedInteraction | null, label: string): CapturedInteraction {
  if (!value) {
    throw new Error(`${label} was not captured`);
  }
  return value;
}

const fixture = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  namespace,
  key,
  schema: requireCapture(schema, 'schema'),
  productCreate: requireCapture(productCreate, 'productCreate'),
  create: requireCapture(create, 'create'),
  metafieldsSet: requireCapture(metafieldsSet, 'metafieldsSet'),
  validationUpdate: requireCapture(validationUpdate, 'validationUpdate'),
  jobRead: requireCapture(jobRead, 'jobRead'),
  rename: requireCapture(rename, 'rename'),
  cleanup,
  upstreamCalls: [],
};

const spec = {
  scenarioId: 'metafield-definition-validation-job',
  operationNames: ['metafieldDefinitionCreate', 'metafieldsSet', 'metafieldDefinitionUpdate', 'job'],
  scenarioStatus: 'captured',
  assertionKinds: ['payload-shape', 'validation-backfill-job', 'async-job-readback', 'null-noop-job'],
  liveCaptureFiles: [fixturePath],
  proxyRequest: {
    documentPath: createDocumentPath,
    variablesCapturePath: '$.create.request.variables',
    apiVersion,
  },
  comparisonMode: 'captured-vs-proxy-request',
  notes:
    'Executable parity for metafieldDefinitionUpdate validation backfill jobs. The live schema exposes validationJob on MetafieldDefinitionUpdatePayload, not on MetafieldDefinitionCreatePayload or MetafieldDefinition; a validation change over an existing matching metafield returns a pending payload Job, job(id:) readback returns the same pending shape in this capture, and a subsequent non-validation update returns validationJob null.',
  comparison: {
    mode: 'strict-json',
    expectedDifferences: [
      {
        path: '$.metafieldDefinitionCreate.createdDefinition.id',
        matcher: 'shopify-gid:MetafieldDefinition',
        reason: 'Shopify and the local parity harness allocate definition identifiers independently.',
      },
      {
        path: '$.metafieldsSet.metafields[0].id',
        matcher: 'shopify-gid:Metafield',
        reason: 'Shopify and the local parity harness allocate metafield identifiers independently.',
      },
      {
        path: '$.metafieldDefinitionUpdate.updatedDefinition.id',
        matcher: 'shopify-gid:MetafieldDefinition',
        reason:
          'The proxy updates its staged synthetic definition while Shopify returned the live-store definition ID.',
      },
      {
        path: '$.metafieldDefinitionUpdate.validationJob.id',
        matcher: 'shopify-gid:Job',
        reason: 'Shopify and the local parity harness allocate async job identifiers independently.',
      },
      {
        path: '$.job.id',
        matcher: 'shopify-gid:Job',
        reason: 'The job readback targets the job ID allocated by the current proxy run.',
      },
    ],
    targets: [
      {
        name: 'create-definition',
        capturePath: '$.create.response.data',
        proxyPath: '$.data',
      },
      {
        name: 'stage-existing-metafield',
        capturePath: '$.metafieldsSet.response.data',
        proxyRequest: {
          documentPath: metafieldsSetDocumentPath,
          variablesCapturePath: '$.metafieldsSet.request.variables',
          apiVersion,
        },
        proxyPath: '$.data',
      },
      {
        name: 'validation-update-job',
        capturePath: '$.validationUpdate.response.data',
        proxyRequest: {
          documentPath: updateDocumentPath,
          variablesCapturePath: '$.validationUpdate.request.variables',
          apiVersion,
        },
        proxyPath: '$.data',
      },
      {
        name: 'job-readback',
        capturePath: '$.jobRead.response.data',
        proxyRequest: {
          documentPath: jobReadDocumentPath,
          apiVersion,
          variables: {
            id: {
              fromProxyResponse: 'validation-update-job',
              path: '$.data.metafieldDefinitionUpdate.validationJob.id',
            },
          },
        },
        proxyPath: '$.data',
      },
      {
        name: 'rename-no-validation-change',
        capturePath: '$.rename.response.data',
        proxyRequest: {
          documentPath: renameDocumentPath,
          variablesCapturePath: '$.rename.request.variables',
          apiVersion,
        },
        proxyPath: '$.data',
      },
    ],
  },
};

await mkdir(requestDir, { recursive: true });
await mkdir(specDir, { recursive: true });
await mkdir(fixtureDir, { recursive: true });

await writeFile(createDocumentPath, createDocument, 'utf8');
await writeFile(metafieldsSetDocumentPath, metafieldsSetDocument, 'utf8');
await writeFile(updateDocumentPath, updateDocument, 'utf8');
await writeFile(jobReadDocumentPath, jobReadDocument, 'utf8');
await writeFile(renameDocumentPath, renameDocument, 'utf8');
await writeFile(fixturePath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
await writeFile(specPath, `${JSON.stringify(spec, null, 2)}\n`, 'utf8');

console.log(
  JSON.stringify(
    {
      ok: true,
      fixturePath,
      specPath,
      requestFiles: [
        createDocumentPath,
        metafieldsSetDocumentPath,
        updateDocumentPath,
        jobReadDocumentPath,
        renameDocumentPath,
      ],
    },
    null,
    2,
  ),
);

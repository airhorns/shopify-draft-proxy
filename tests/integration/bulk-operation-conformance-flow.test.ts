import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path, { resolve } from 'node:path';

import request from 'supertest';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { classifyParityScenarioState, type ParitySpec } from '../../scripts/conformance-parity-lib.js';
import { loadConformanceScenarios } from '../../scripts/conformance-scenario-registry.js';
import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { parseOperation } from '../../src/graphql/parse-operation.js';
import { getOperationCapability } from '../../src/proxy/capabilities.js';
import { findOperationRegistryEntry } from '../../src/proxy/operation-registry.js';
import { resetSyntheticIdentity } from '../../src/state/synthetic-identity.js';
import { store } from '../../src/state/store.js';
import type { BulkOperationRecord } from '../../src/state/types.js';

const repoRoot = resolve(import.meta.dirname, '../..');
const fixturePath =
  'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/bulk-operation-status-catalog-cancel.json';
const specPath = 'config/parity-specs/bulk-operation-status-catalog-cancel.json';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

type CapturedInteraction = {
  operationName: string;
  query: string;
  variables: Record<string, unknown>;
  status: number;
  response: Record<string, unknown>;
};

type BulkOperationFixture = {
  apiVersion: string;
  reads: Record<string, CapturedInteraction>;
  validations: Record<string, CapturedInteraction>;
  lifecycle: Record<string, unknown>;
};

type BulkImportLogEntryBody = {
  operationName: string;
  variables: Record<string, unknown>;
  interpreted: {
    bulkOperationImport?: {
      lineNumber: number;
      outerRequestBody: unknown;
    };
  };
};

function readText(relativePath: string): string {
  return readFileSync(resolve(repoRoot, relativePath), 'utf8');
}

function readJson<T>(relativePath: string): T {
  return JSON.parse(readText(relativePath)) as T;
}

const fixture = readJson<BulkOperationFixture>(fixturePath);

function readInteraction(section: 'reads' | 'validations', key: string): CapturedInteraction {
  const interaction = fixture[section][key];
  if (!interaction) {
    throw new Error(`Missing BulkOperation fixture interaction ${section}.${key}`);
  }

  return interaction;
}

function makeBulkOperation(id: string, overrides: Partial<BulkOperationRecord> = {}): BulkOperationRecord {
  return {
    id,
    status: 'RUNNING',
    type: 'QUERY',
    errorCode: null,
    createdAt: '2026-04-27T00:00:00Z',
    completedAt: null,
    objectCount: '0',
    rootObjectCount: '0',
    fileSize: null,
    url: null,
    partialDataUrl: null,
    query: '#graphql\n{ products { edges { node { id title } } } }',
    ...overrides,
  };
}

function writeSnapshotFile(tempDir: string, operation: BulkOperationRecord): string {
  const snapshotPath = path.join(tempDir, 'bulk-operation-snapshot.json');
  writeFileSync(
    snapshotPath,
    JSON.stringify(
      {
        kind: 'normalized-state-snapshot',
        baseState: {
          products: {},
          productVariants: {},
          productOptions: {},
          collections: {},
          customers: {},
          productCollections: {},
          productMedia: {},
          productMetafields: {},
          deletedProductIds: {},
          deletedCollectionIds: {},
          deletedCustomerIds: {},
          bulkOperations: {
            [operation.id]: operation,
          },
          bulkOperationOrder: [operation.id],
        },
      },
      null,
      2,
    ),
  );
  return snapshotPath;
}

const bulkOperationSelection = `
  id
  status
  type
  errorCode
  createdAt
  completedAt
  objectCount
  rootObjectCount
  fileSize
  url
  partialDataUrl
  query
`;

describe('BulkOperation conformance fixture and local model', () => {
  let tempDir: string | null = null;

  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  afterEach(() => {
    if (tempDir) {
      rmSync(tempDir, { recursive: true, force: true });
      tempDir = null;
    }
  });

  it('discovers captured fixture evidence for locally implemented read and cancel foundations', () => {
    const scenarios = loadConformanceScenarios(repoRoot);
    const scenario = scenarios.find((candidate) => candidate.id === 'bulk-operation-status-catalog-cancel');
    const paritySpec = readJson<ParitySpec>(specPath);

    expect(scenario).toMatchObject({
      status: 'captured',
      operationNames: [
        'bulkOperation',
        'bulkOperations',
        'currentBulkOperation',
        'bulkOperationRunQuery',
        'bulkOperationRunMutation',
        'bulkOperationCancel',
      ],
      captureFiles: [fixturePath],
    });
    expect(classifyParityScenarioState(scenario!, paritySpec)).toBe('enforced-by-fixture');

    for (const [operationType, rootField, implemented] of [
      ['query', 'bulkOperation', true],
      ['query', 'bulkOperations', true],
      ['query', 'currentBulkOperation', true],
      ['mutation', 'bulkOperationRunQuery', false],
      ['mutation', 'bulkOperationRunMutation', true],
      ['mutation', 'bulkOperationCancel', true],
    ] as const) {
      expect(findOperationRegistryEntry(operationType, [rootField])).toMatchObject({
        domain: 'bulk-operations',
        implemented,
      });
    }

    expect(
      getOperationCapability(parseOperation(readInteraction('reads', 'catalogEmptyRunningMutation').query)),
    ).toEqual({
      type: 'query',
      operationName: 'bulkOperations',
      domain: 'bulk-operations',
      execution: 'overlay-read',
    });
    expect(
      getOperationCapability(parseOperation(readInteraction('validations', 'bulkOperationCancelUnknownId').query)),
    ).toEqual({
      type: 'mutation',
      operationName: 'bulkOperationCancel',
      domain: 'bulk-operations',
      execution: 'stage-locally',
    });
  });

  it('serves missing reads and empty listings in snapshot mode without upstream access', async () => {
    const fetchSpy = vi
      .spyOn(globalThis, 'fetch')
      .mockRejectedValue(new Error('BulkOperation reads should stay local'));
    const app = createApp(config).callback();

    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({
        query: `query EmptyBulkOperations {
          bulkOperation(id: "gid://shopify/BulkOperation/0") {
            ${bulkOperationSelection}
          }
          bulkOperations(first: 2) {
            edges {
              cursor
              node {
                ${bulkOperationSelection}
              }
            }
            nodes {
              id
            }
            pageInfo {
              hasNextPage
              hasPreviousPage
              startCursor
              endCursor
            }
          }
          currentBulkOperation(type: MUTATION) {
            id
          }
        }`,
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        bulkOperation: null,
        bulkOperations: {
          edges: [],
          nodes: [],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: false,
            startCursor: null,
            endCursor: null,
          },
        },
        currentBulkOperation: null,
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('reads, lists, filters, paginates, and derives currentBulkOperation from effective local state', async () => {
    const completedQuery = makeBulkOperation('gid://shopify/BulkOperation/101', {
      status: 'COMPLETED',
      type: 'QUERY',
      createdAt: '2026-04-27T00:00:01Z',
      completedAt: '2026-04-27T00:00:02Z',
      objectCount: '2',
      rootObjectCount: '1',
      fileSize: '25',
      url: 'https://example.test/completed.jsonl',
    });
    const runningMutation = makeBulkOperation('gid://shopify/BulkOperation/202', {
      type: 'MUTATION',
      createdAt: '2026-04-27T00:00:03Z',
    });
    const runningQuery = makeBulkOperation('gid://shopify/BulkOperation/303', {
      createdAt: '2026-04-27T00:00:04Z',
    });

    store.upsertBaseBulkOperations([completedQuery]);
    store.stageBulkOperation(runningMutation);
    store.stageBulkOperation(runningQuery);

    const app = createApp(config).callback();
    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query LocalBulkOperations {
        byId: bulkOperation(id: "gid://shopify/BulkOperation/202") {
          id
          status
          type
        }
        firstPage: bulkOperations(first: 1) {
          edges {
            cursor
            node {
              id
              createdAt
            }
          }
          nodes {
            id
          }
          pageInfo {
            hasNextPage
            hasPreviousPage
            startCursor
            endCursor
          }
        }
        secondPage: bulkOperations(first: 1, after: "cursor:gid://shopify/BulkOperation/303") {
          nodes {
            id
          }
          pageInfo {
            hasNextPage
            hasPreviousPage
            startCursor
            endCursor
          }
        }
        runningMutations: bulkOperations(first: 5, query: "status:RUNNING operation_type:MUTATION") {
          nodes {
            id
            type
            status
          }
        }
        reversedById: bulkOperations(first: 5, sortKey: ID, reverse: true) {
          nodes {
            id
          }
        }
        currentQuery: currentBulkOperation(type: QUERY) {
          id
        }
        currentMutation: currentBulkOperation(type: MUTATION) {
          id
        }
      }`,
      });

    expect(response.status).toBe(200);
    expect(response.body.data).toMatchObject({
      byId: {
        id: 'gid://shopify/BulkOperation/202',
        status: 'RUNNING',
        type: 'MUTATION',
      },
      firstPage: {
        edges: [
          {
            cursor: 'cursor:gid://shopify/BulkOperation/303',
            node: {
              id: 'gid://shopify/BulkOperation/303',
              createdAt: '2026-04-27T00:00:04Z',
            },
          },
        ],
        nodes: [{ id: 'gid://shopify/BulkOperation/303' }],
        pageInfo: {
          hasNextPage: true,
          hasPreviousPage: false,
          startCursor: 'cursor:gid://shopify/BulkOperation/303',
          endCursor: 'cursor:gid://shopify/BulkOperation/303',
        },
      },
      secondPage: {
        nodes: [{ id: 'gid://shopify/BulkOperation/202' }],
        pageInfo: {
          hasNextPage: true,
          hasPreviousPage: true,
          startCursor: 'cursor:gid://shopify/BulkOperation/202',
          endCursor: 'cursor:gid://shopify/BulkOperation/202',
        },
      },
      runningMutations: {
        nodes: [
          {
            id: 'gid://shopify/BulkOperation/202',
            type: 'MUTATION',
            status: 'RUNNING',
          },
        ],
      },
      reversedById: {
        nodes: [
          { id: 'gid://shopify/BulkOperation/303' },
          { id: 'gid://shopify/BulkOperation/202' },
          { id: 'gid://shopify/BulkOperation/101' },
        ],
      },
      currentQuery: {
        id: 'gid://shopify/BulkOperation/303',
      },
      currentMutation: {
        id: 'gid://shopify/BulkOperation/202',
      },
    });
  });

  it('cancels staged jobs locally and returns captured userErrors for unknown or terminal jobs', async () => {
    const fetchSpy = vi
      .spyOn(globalThis, 'fetch')
      .mockRejectedValue(new Error('bulkOperationCancel should stay local'));
    store.stageBulkOperation(
      makeBulkOperation('gid://shopify/BulkOperation/401', {
        status: 'RUNNING',
      }),
    );
    store.stageBulkOperation(
      makeBulkOperation('gid://shopify/BulkOperation/402', {
        status: 'COMPLETED',
        completedAt: '2026-04-27T00:01:00Z',
      }),
    );
    const app = createApp(config).callback();

    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation CancelBulkOperations {
        running: bulkOperationCancel(id: "gid://shopify/BulkOperation/401") {
          bulkOperation {
            id
            status
            completedAt
          }
          userErrors {
            field
            message
          }
        }
        terminal: bulkOperationCancel(id: "gid://shopify/BulkOperation/402") {
          bulkOperation {
            id
            status
          }
          userErrors {
            field
            message
          }
        }
        missing: bulkOperationCancel(id: "gid://shopify/BulkOperation/0") {
          bulkOperation {
            id
          }
          userErrors {
            field
            message
          }
        }
      }`,
      });

    expect(response.status).toBe(200);
    expect(response.body.data).toEqual({
      running: {
        bulkOperation: {
          id: 'gid://shopify/BulkOperation/401',
          status: 'CANCELING',
          completedAt: null,
        },
        userErrors: [],
      },
      terminal: {
        bulkOperation: {
          id: 'gid://shopify/BulkOperation/402',
          status: 'COMPLETED',
        },
        userErrors: [
          {
            field: null,
            message: 'A bulk operation cannot be canceled when it is completed',
          },
        ],
      },
      missing: {
        bulkOperation: null,
        userErrors: [
          {
            field: ['id'],
            message: 'Bulk operation does not exist',
          },
        ],
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();

    const readAfterCancel = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query ReadCanceled {
        bulkOperation(id: "gid://shopify/BulkOperation/401") {
          id
          status
        }
      }`,
      });
    const logResponse = await request(app).get('/__meta/log');

    expect(readAfterCancel.body.data.bulkOperation).toEqual({
      id: 'gid://shopify/BulkOperation/401',
      status: 'CANCELING',
    });
    expect(logResponse.body.entries).toHaveLength(1);
    expect(logResponse.body.entries[0]).toMatchObject({
      operationName: 'bulkOperationCancel',
      status: 'staged',
      stagedResourceIds: ['gid://shopify/BulkOperation/401', 'gid://shopify/BulkOperation/402'],
      interpreted: {
        operationType: 'mutation',
        rootFields: ['bulkOperationCancel', 'bulkOperationCancel', 'bulkOperationCancel'],
        capability: {
          domain: 'bulk-operations',
          execution: 'stage-locally',
        },
      },
    });
  });

  it('stages supported product bulk mutation imports from uploaded JSONL and preserves line-order commit logs', async () => {
    const fetchSpy = vi
      .spyOn(globalThis, 'fetch')
      .mockRejectedValue(new Error('bulkOperationRunMutation product imports should stay local'));
    const app = createApp(config).callback();

    const stagedUploadResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation PrepareBulkImport($input: [StagedUploadInput!]!) {
          stagedUploadsCreate(input: $input) {
            stagedTargets {
              resourceUrl
              parameters {
                name
                value
              }
            }
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          input: [
            {
              resource: 'BULK_MUTATION_VARIABLES',
              filename: 'product-create.jsonl',
              mimeType: 'text/jsonl',
              httpMethod: 'POST',
            },
          ],
        },
      });
    const target = stagedUploadResponse.body.data.stagedUploadsCreate.stagedTargets[0] as {
      resourceUrl: string;
      parameters: Array<{ name: string; value: string }>;
    };
    const stagedUploadPath = target.parameters.find((parameter) => parameter.name === 'key')?.value;

    expect(stagedUploadPath).toEqual(expect.stringContaining('/product-create.jsonl'));
    expect(stagedUploadResponse.body.data.stagedUploadsCreate.userErrors).toEqual([]);
    if (!stagedUploadPath) {
      throw new Error('stagedUploadsCreate did not return a key parameter');
    }

    const jsonl = [
      JSON.stringify({ product: { title: 'Bulk Hat One', status: 'DRAFT' } }),
      JSON.stringify({ product: { title: '', status: 'DRAFT' } }),
      JSON.stringify({ product: { title: 'Bulk Hat Two', status: 'ACTIVE' } }),
    ].join('\n');
    const uploadResponse = await request(app)
      .post(new URL(target.resourceUrl).pathname)
      .set('content-type', 'text/jsonl')
      .send(`${jsonl}\n`);

    expect(uploadResponse.status).toBe(201);

    const innerMutation = `mutation ProductCreate($product: ProductCreateInput!) {
      productCreate(product: $product) {
        product {
          id
          title
          handle
          status
        }
        userErrors {
          field
          message
        }
      }
    }`;
    const bulkRequestBody = {
      query: `mutation BulkImport($mutation: String!, $stagedUploadPath: String!) {
        bulkOperationRunMutation(mutation: $mutation, stagedUploadPath: $stagedUploadPath) {
          bulkOperation {
            id
            status
            type
            objectCount
            rootObjectCount
            fileSize
            url
            query
          }
          userErrors {
            field
            message
          }
        }
      }`,
      variables: {
        mutation: innerMutation,
        stagedUploadPath,
      },
    };
    const bulkResponse = await request(app).post('/admin/api/2026-04/graphql.json').send(bulkRequestBody);

    expect(bulkResponse.status).toBe(200);
    expect(fetchSpy).not.toHaveBeenCalled();
    expect(bulkResponse.body.data.bulkOperationRunMutation.userErrors).toEqual([]);
    expect(bulkResponse.body.data.bulkOperationRunMutation.bulkOperation).toMatchObject({
      status: 'COMPLETED',
      type: 'MUTATION',
      objectCount: '2',
      rootObjectCount: '2',
      query: innerMutation,
    });
    const operationId = bulkResponse.body.data.bulkOperationRunMutation.bulkOperation.id as string;
    const resultResponse = await request(app).get(
      `/__meta/bulk-operations/${encodeURIComponent(operationId)}/result.jsonl`,
    );
    const resultRows = resultResponse.text
      .trim()
      .split('\n')
      .map((line) => JSON.parse(line) as Record<string, unknown>);

    expect(resultResponse.status).toBe(200);
    expect(resultRows).toHaveLength(3);
    expect(resultRows[0]).toMatchObject({
      line: 1,
      response: {
        data: {
          productCreate: {
            product: {
              title: 'Bulk Hat One',
              handle: 'bulk-hat-one',
              status: 'DRAFT',
            },
            userErrors: [],
          },
        },
      },
    });
    expect(resultRows[1]).toMatchObject({
      line: 2,
      response: {
        data: {
          productCreate: {
            product: null,
            userErrors: [{ field: ['title'], message: "Title can't be blank" }],
          },
        },
      },
    });
    expect(resultRows[2]).toMatchObject({
      line: 3,
      response: {
        data: {
          productCreate: {
            product: {
              title: 'Bulk Hat Two',
              handle: 'bulk-hat-two',
              status: 'ACTIVE',
            },
            userErrors: [],
          },
        },
      },
    });

    const firstResultResponse = resultRows[0]?.['response'] as {
      data: { productCreate: { product: { id: string } } };
    };
    const secondResultResponse = resultRows[2]?.['response'] as {
      data: { productCreate: { product: { id: string } } };
    };
    const firstProductId = firstResultResponse.data.productCreate.product.id;
    const secondProductId = secondResultResponse.data.productCreate.product.id;
    const readAfterWriteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query ReadBulkImportedProducts($firstId: ID!, $secondId: ID!) {
          first: product(id: $firstId) {
            id
            title
            handle
            status
          }
          second: product(id: $secondId) {
            id
            title
            handle
            status
          }
        }`,
        variables: {
          firstId: firstProductId,
          secondId: secondProductId,
        },
      });
    const logResponse = await request(app).get('/__meta/log');
    const stateResponse = await request(app).get('/__meta/state');

    expect(readAfterWriteResponse.body.data).toEqual({
      first: {
        id: firstProductId,
        title: 'Bulk Hat One',
        handle: 'bulk-hat-one',
        status: 'DRAFT',
      },
      second: {
        id: secondProductId,
        title: 'Bulk Hat Two',
        handle: 'bulk-hat-two',
        status: 'ACTIVE',
      },
    });
    expect(stateResponse.body.stagedState.bulkOperations[operationId]).toMatchObject({
      status: 'COMPLETED',
      resultJsonl: resultResponse.text,
    });
    const bulkImportLogEntries = (logResponse.body.entries as BulkImportLogEntryBody[]).filter(
      (
        entry,
      ): entry is BulkImportLogEntryBody & {
        interpreted: { bulkOperationImport: { lineNumber: number; outerRequestBody: unknown } };
      } => Boolean(entry.interpreted.bulkOperationImport),
    );

    expect(logResponse.body.entries).toHaveLength(4);
    expect(logResponse.body.entries[0]).toMatchObject({
      operationName: 'stagedUploadsCreate',
      status: 'staged',
    });
    expect(bulkImportLogEntries.map((entry) => entry.operationName)).toEqual([
      'ProductCreate',
      'ProductCreate',
      'ProductCreate',
    ]);
    expect(
      bulkImportLogEntries.map((entry) => ({
        lineNumber: entry.interpreted.bulkOperationImport.lineNumber,
        outerRequestBody: entry.interpreted.bulkOperationImport.outerRequestBody,
      })),
    ).toEqual([
      {
        lineNumber: 1,
        outerRequestBody: bulkRequestBody,
      },
      {
        lineNumber: 2,
        outerRequestBody: bulkRequestBody,
      },
      {
        lineNumber: 3,
        outerRequestBody: bulkRequestBody,
      },
    ]);
    expect(bulkImportLogEntries.map((entry) => entry.variables)).toEqual([
      { product: { title: 'Bulk Hat One', status: 'DRAFT' } },
      { product: { title: '', status: 'DRAFT' } },
      { product: { title: 'Bulk Hat Two', status: 'ACTIVE' } },
    ]);
  });

  it('fails unsupported bulk mutation import roots locally without upstream passthrough', async () => {
    const fetchSpy = vi
      .spyOn(globalThis, 'fetch')
      .mockRejectedValue(new Error('unsupported bulkOperationRunMutation imports should not proxy upstream'));
    const app = createApp(config).callback();

    store.stageUploadContent(
      ['shopify-draft-proxy/gid://shopify/StagedUploadTarget0/unsupported.jsonl'],
      `${JSON.stringify({ input: { email: 'bulk@example.com' } })}\n`,
    );

    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation BulkImport($mutation: String!, $stagedUploadPath: String!) {
          bulkOperationRunMutation(mutation: $mutation, stagedUploadPath: $stagedUploadPath) {
            bulkOperation {
              id
              status
              type
              objectCount
              rootObjectCount
              url
            }
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          mutation: `mutation CustomerCreate($input: CustomerInput!) {
            customerCreate(input: $input) {
              customer {
                id
              }
              userErrors {
                field
                message
              }
            }
          }`,
          stagedUploadPath: 'shopify-draft-proxy/gid://shopify/StagedUploadTarget0/unsupported.jsonl',
        },
      });
    const logResponse = await request(app).get('/__meta/log');
    const operationId = response.body.data.bulkOperationRunMutation.bulkOperation.id as string;
    const resultResponse = await request(app).get(
      `/__meta/bulk-operations/${encodeURIComponent(operationId)}/result.jsonl`,
    );

    expect(response.status).toBe(200);
    expect(fetchSpy).not.toHaveBeenCalled();
    expect(response.body.data.bulkOperationRunMutation.bulkOperation).toMatchObject({
      status: 'FAILED',
      type: 'MUTATION',
      objectCount: '0',
      rootObjectCount: '0',
    });
    expect(response.body.data.bulkOperationRunMutation.userErrors).toEqual([
      {
        field: ['mutation'],
        message: 'Unsupported bulk mutation import root. The proxy did not send this bulk import upstream at runtime.',
      },
    ]);
    expect(JSON.parse(resultResponse.text.trim())).toMatchObject({
      line: null,
      errors: [
        {
          message:
            'bulkOperationRunMutation locally supports only single-root product mutations that are already staged by the proxy.',
        },
      ],
    });
    expect(logResponse.body.entries).toHaveLength(1);
    expect(logResponse.body.entries[0]).toMatchObject({
      operationName: 'bulkOperationRunMutation',
      status: 'failed',
      interpreted: {
        capability: {
          domain: 'bulk-operations',
          execution: 'stage-locally',
        },
      },
    });
  });

  it('restores snapshot BulkOperation state and clears staged jobs on meta reset', async () => {
    tempDir = mkdtempSync(path.join(tmpdir(), 'shopify-draft-proxy-bulk-operation-'));
    const baseOperation = makeBulkOperation('gid://shopify/BulkOperation/501', {
      status: 'COMPLETED',
      createdAt: '2026-04-27T00:02:00Z',
      completedAt: '2026-04-27T00:02:05Z',
    });
    const snapshotPath = writeSnapshotFile(tempDir, baseOperation);
    const app = createApp({ ...config, snapshotPath }).callback();

    store.stageBulkOperation(makeBulkOperation('gid://shopify/BulkOperation/502'));

    const stateBeforeReset = await request(app).get('/__meta/state');
    const resetResponse = await request(app).post('/__meta/reset');
    const stateAfterReset = await request(app).get('/__meta/state');

    expect(stateBeforeReset.body.baseState.bulkOperations['gid://shopify/BulkOperation/501']).toMatchObject({
      status: 'COMPLETED',
    });
    expect(stateBeforeReset.body.stagedState.bulkOperations['gid://shopify/BulkOperation/502']).toMatchObject({
      status: 'RUNNING',
    });
    expect(resetResponse.status).toBe(200);
    expect(stateAfterReset.body.baseState.bulkOperations).toEqual({
      'gid://shopify/BulkOperation/501': baseOperation,
    });
    expect(stateAfterReset.body.baseState.bulkOperationOrder).toEqual(['gid://shopify/BulkOperation/501']);
    expect(stateAfterReset.body.stagedState.bulkOperations).toEqual({});
    expect(stateAfterReset.body.stagedState.bulkOperationOrder).toEqual([]);
  });
});

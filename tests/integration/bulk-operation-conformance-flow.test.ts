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
import type { BulkOperationRecord, ProductRecord, ProductVariantRecord } from '../../src/state/types.js';

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

function makeBaseProduct(id: string, title: string, handle: string): ProductRecord {
  return {
    id,
    legacyResourceId: id.split('/').at(-1) ?? id,
    title,
    handle,
    status: 'ACTIVE',
    publicationIds: [],
    createdAt: '2024-01-01T00:00:00.000Z',
    updatedAt: '2024-01-02T00:00:00.000Z',
    vendor: null,
    productType: null,
    tags: [],
    totalInventory: null,
    tracksInventory: null,
    descriptionHtml: null,
    onlineStorePreviewUrl: null,
    templateSuffix: null,
    seo: { title: null, description: null },
    category: null,
  };
}

function makeBaseVariant(productId: string, id: string, title: string, sku: string): ProductVariantRecord {
  return {
    id,
    productId,
    title,
    sku,
    barcode: null,
    price: null,
    compareAtPrice: null,
    taxable: null,
    inventoryPolicy: null,
    inventoryQuantity: 0,
    selectedOptions: [{ name: 'Title', value: title }],
    inventoryItem: null,
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

  it('discovers captured proxy-comparison evidence for locally implemented read and cancel foundations', () => {
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
        'bulkOperationCancel',
      ],
      captureFiles: [fixturePath],
    });
    expect(classifyParityScenarioState(scenario!, paritySpec)).toBe('ready-for-comparison');

    for (const [operationType, rootField, implemented] of [
      ['query', 'bulkOperation', true],
      ['query', 'bulkOperations', true],
      ['query', 'currentBulkOperation', true],
      ['mutation', 'bulkOperationRunQuery', true],
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

  it('stages supported product bulk query exports locally and serves JSONL result records', async () => {
    const fetchSpy = vi
      .spyOn(globalThis, 'fetch')
      .mockRejectedValue(new Error('bulkOperationRunQuery should stay local'));
    const app = createApp(config).callback();
    const productOne = makeBaseProduct('gid://shopify/Product/1001', 'Alpha Hat', 'alpha-hat');
    const productTwo = makeBaseProduct('gid://shopify/Product/1002', 'Beta Hat', 'beta-hat');
    store.upsertBaseProducts([productOne, productTwo]);
    store.replaceBaseVariantsForProduct(productOne.id, [
      makeBaseVariant(productOne.id, 'gid://shopify/ProductVariant/2001', 'Small', 'ALPHA-S'),
      makeBaseVariant(productOne.id, 'gid://shopify/ProductVariant/2002', 'Large', 'ALPHA-L'),
    ]);
    store.replaceBaseVariantsForProduct(productTwo.id, [
      makeBaseVariant(productTwo.id, 'gid://shopify/ProductVariant/2003', 'One Size', 'BETA-OS'),
    ]);

    const bulkQuery = `#graphql
      {
        products(sortKey: TITLE) {
          edges {
            node {
              id
              title
              handle
              variants {
                edges {
                  node {
                    id
                    title
                    sku
                  }
                }
              }
            }
          }
        }
      }
    `;
    const runResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation RunBulkProductExport($query: String!, $groupObjects: Boolean!) {
            bulkOperationRunQuery(query: $query, groupObjects: $groupObjects) {
              bulkOperation {
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
              }
              userErrors {
                field
                message
              }
            }
          }
        `,
        variables: { query: bulkQuery, groupObjects: false },
      });

    expect(runResponse.status).toBe(200);
    expect(fetchSpy).not.toHaveBeenCalled();
    const payload = runResponse.body.data.bulkOperationRunQuery;
    expect(payload.userErrors).toEqual([]);
    expect(payload.bulkOperation).toMatchObject({
      id: 'gid://shopify/BulkOperation/1',
      status: 'COMPLETED',
      type: 'QUERY',
      errorCode: null,
      objectCount: '5',
      rootObjectCount: '2',
      partialDataUrl: null,
      query: bulkQuery,
    });
    expect(payload.bulkOperation.url).toBe('https://shopify-draft-proxy.local/__bulk_operations/1/result.jsonl');
    expect(payload.bulkOperation.fileSize).toEqual(expect.stringMatching(/^\d+$/));

    const resultResponse = await request(app).get(new URL(payload.bulkOperation.url).pathname);
    expect(resultResponse.status).toBe(200);
    expect(resultResponse.headers['content-type']).toContain('application/jsonl');
    const records = resultResponse.text
      .trim()
      .split('\n')
      .map((line) => JSON.parse(line) as Record<string, unknown>);
    expect(records).toEqual([
      { id: productOne.id, title: 'Alpha Hat', handle: 'alpha-hat' },
      { id: 'gid://shopify/ProductVariant/2001', title: 'Small', sku: 'ALPHA-S', __parentId: productOne.id },
      { id: 'gid://shopify/ProductVariant/2002', title: 'Large', sku: 'ALPHA-L', __parentId: productOne.id },
      { id: productTwo.id, title: 'Beta Hat', handle: 'beta-hat' },
      { id: 'gid://shopify/ProductVariant/2003', title: 'One Size', sku: 'BETA-OS', __parentId: productTwo.id },
    ]);

    const readResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          query ReadBulkOperation($id: ID!) {
            bulkOperation(id: $id) {
              id
              status
              objectCount
              rootObjectCount
              url
            }
            bulkOperations(first: 5) {
              nodes {
                id
                status
                url
              }
            }
          }
        `,
        variables: { id: payload.bulkOperation.id },
      });
    expect(readResponse.body.data.bulkOperation).toMatchObject({
      id: payload.bulkOperation.id,
      status: 'COMPLETED',
      objectCount: '5',
      rootObjectCount: '2',
      url: payload.bulkOperation.url,
    });
    expect(readResponse.body.data.bulkOperations.nodes).toEqual([
      { id: payload.bulkOperation.id, status: 'COMPLETED', url: payload.bulkOperation.url },
    ]);

    const logResponse = await request(app).get('/__meta/log');
    expect(logResponse.body.entries).toHaveLength(1);
    expect(logResponse.body.entries[0]).toMatchObject({
      operationName: 'bulkOperationRunQuery',
      path: '/admin/api/2026-04/graphql.json',
      stagedResourceIds: [payload.bulkOperation.id],
      requestBody: {
        variables: { query: bulkQuery, groupObjects: false },
      },
    });
    expect(logResponse.body.entries[0].requestBody.query).toContain('bulkOperationRunQuery');

    const stateResponse = await request(app).get('/__meta/state');
    expect(stateResponse.body.stagedState.bulkOperations[payload.bulkOperation.id]).toMatchObject({
      status: 'COMPLETED',
      url: payload.bulkOperation.url,
    });
    expect(stateResponse.body.stagedState.bulkOperationResults[payload.bulkOperation.id]).toBe(resultResponse.text);
  });

  it('returns local userErrors for malformed, unsupported, and unsupported-shape bulk query exports', async () => {
    const fetchSpy = vi
      .spyOn(globalThis, 'fetch')
      .mockRejectedValue(new Error('bulk query validations should stay local'));
    const app = createApp(config).callback();

    async function runBulkQuery(query: string, groupObjects = false) {
      const response = await request(app)
        .post('/admin/api/2026-04/graphql.json')
        .send({
          query: `#graphql
            mutation RunBulkProductExport($query: String!, $groupObjects: Boolean!) {
              bulkOperationRunQuery(query: $query, groupObjects: $groupObjects) {
                bulkOperation { id }
                userErrors { field message }
              }
            }
          `,
          variables: { query, groupObjects },
        });
      expect(response.status).toBe(200);
      return response.body.data.bulkOperationRunQuery;
    }

    await expect(runBulkQuery('#graphql\n{ shop { id } }')).resolves.toEqual({
      bulkOperation: null,
      userErrors: [{ field: ['query'], message: 'Bulk queries must contain at least one connection.' }],
    });
    await expect(runBulkQuery('#graphql\n{ orders { edges { node { id } } } }')).resolves.toEqual({
      bulkOperation: null,
      userErrors: [{ field: ['query'], message: "Bulk query root 'orders' is not supported locally." }],
    });
    await expect(
      runBulkQuery('#graphql\n{ products { edges { node { id media { edges { node { id } } } } } } }'),
    ).resolves.toEqual({
      bulkOperation: null,
      userErrors: [
        {
          field: ['query'],
          message: "Nested connection 'media' is not supported by the local bulk query executor.",
        },
      ],
    });
    await expect(runBulkQuery('#graphql\n{ products { edges { node { id } } }', false)).resolves.toEqual({
      bulkOperation: null,
      userErrors: [
        {
          field: ['query'],
          message: expect.stringContaining('Invalid bulk query: Syntax Error:'),
        },
      ],
    });
    await expect(runBulkQuery('#graphql\n{ products { edges { node { id } } } }', true)).resolves.toEqual({
      bulkOperation: null,
      userErrors: [
        {
          field: ['groupObjects'],
          message: 'groupObjects is not supported by the local bulk query executor.',
        },
      ],
    });

    expect(fetchSpy).not.toHaveBeenCalled();
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

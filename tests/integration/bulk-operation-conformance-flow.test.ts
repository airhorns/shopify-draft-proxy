import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path, { resolve } from 'node:path';

import request from 'supertest';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { classifyParityScenarioState, type ParitySpec } from '../../scripts/conformance-parity-lib.js';
import { loadConformanceScenarios } from '../../scripts/conformance-scenario-registry.js';
import { createApp } from '../support/runtime.js';
import type { AppConfig } from '../../src/config.js';
import { parseOperation } from '../../src/graphql/parse-operation.js';
import { getOperationCapability } from '../../src/proxy/capabilities.js';
import { findOperationRegistryEntry } from '../../src/proxy/operation-registry.js';
import { resetSyntheticIdentity } from '../support/runtime.js';
import { store } from '../support/runtime.js';
import type { BulkOperationRecord, ProductRecord, ProductVariantRecord } from '../../src/state/types.js';

const repoRoot = resolve(import.meta.dirname, '../..');
const fixturePath =
  'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/bulk-operations/bulk-operation-status-catalog-cancel.json';
const specPath = 'config/parity-specs/bulk-operations/bulk-operation-status-catalog-cancel.json';

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
    capability: {
      domain: string;
    };
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

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function asRecord(value: unknown): Record<string, unknown> | null {
  return isRecord(value) ? value : null;
}

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

function readFixtureBulkQueryResultRecords(): Array<Record<string, unknown>> {
  const terminalLifecycle = asRecord(fixture.lifecycle['queryExportToTerminal']);
  const result = asRecord(terminalLifecycle?.['result']);
  const records = Array.isArray(result?.['records']) ? result['records'].filter(isRecord) : [];
  if (records.length === 0) {
    throw new Error('BulkOperation fixture is missing captured query export result records.');
  }

  return records;
}

function readFixtureTerminalOperation(): Record<string, unknown> {
  const terminalLifecycle = asRecord(fixture.lifecycle['queryExportToTerminal']);
  const terminalOperation = asRecord(terminalLifecycle?.['terminalOperation']);
  if (!terminalOperation) {
    throw new Error('BulkOperation fixture is missing the terminal query export operation.');
  }

  return terminalOperation;
}

function readFixtureRunQueryVariables(): Record<string, unknown> {
  const terminalLifecycle = asRecord(fixture.lifecycle['queryExportToTerminal']);
  const run = asRecord(terminalLifecycle?.['run']);
  const variables = asRecord(run?.['variables']);
  if (typeof variables?.['query'] !== 'string') {
    throw new Error('BulkOperation fixture is missing run-query variables.');
  }

  return variables;
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
        'bulkOperationCancel',
        'bulkOperationRunQuery',
      ],
      runtimeTestFiles: ['tests/integration/bulk-operation-conformance-flow.test.ts'],
      captureFiles: [fixturePath],
    });
    expect(classifyParityScenarioState(scenario!, paritySpec)).toBe('ready-for-comparison');

    for (const [operationType, rootField, implemented] of [
      ['query', 'bulkOperation', true],
      ['query', 'bulkOperations', true],
      ['query', 'currentBulkOperation', true],
      ['mutation', 'bulkOperationRunQuery', true],
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

  it('replays captured Shopify product query export result records through the local bulk runner', async () => {
    const app = createApp(config).callback();
    const capturedRecords = readFixtureBulkQueryResultRecords();
    for (const [index, record] of capturedRecords.entries()) {
      const id = typeof record['id'] === 'string' ? record['id'] : null;
      if (!id?.startsWith('gid://shopify/Product/')) {
        continue;
      }

      const product = makeBaseProduct(
        id,
        typeof record['title'] === 'string' ? record['title'] : 'Captured product',
        '',
      );
      const orderedTimestamp = new Date(Date.UTC(2026, 3, 27, 0, 0, 0, 0) - index).toISOString();
      product.createdAt = orderedTimestamp;
      product.updatedAt = orderedTimestamp;
      store.upsertBaseProducts([product]);
    }

    const terminalOperation = readFixtureTerminalOperation();
    const runResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: readInteraction('validations', 'bulkOperationRunQueryWithoutConnection').query,
        variables: readFixtureRunQueryVariables(),
      });

    expect(runResponse.status).toBe(200);
    const payload = runResponse.body.data.bulkOperationRunQuery;
    expect(payload.userErrors).toEqual([]);
    expect(payload.bulkOperation).toMatchObject({
      status: terminalOperation['status'],
      type: terminalOperation['type'],
      errorCode: terminalOperation['errorCode'],
      objectCount: terminalOperation['objectCount'],
      rootObjectCount: terminalOperation['rootObjectCount'],
      partialDataUrl: terminalOperation['partialDataUrl'],
      query: terminalOperation['query'],
    });

    const resultResponse = await request(app).get(new URL(payload.bulkOperation.url).pathname);
    expect(resultResponse.status).toBe(200);
    const localRecords = resultResponse.text
      .trim()
      .split('\n')
      .filter((line) => line.length > 0)
      .map((line) => JSON.parse(line) as Record<string, unknown>);
    expect(localRecords).toEqual(capturedRecords);
    expect(payload.bulkOperation.fileSize).toBe(String(Buffer.byteLength(resultResponse.text, 'utf8')));
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

  it('stages supported product-variant bulk mutation imports from uploaded JSONL', async () => {
    const fetchSpy = vi
      .spyOn(globalThis, 'fetch')
      .mockRejectedValue(new Error('bulkOperationRunMutation product-variant imports should stay local'));
    const app = createApp(config).callback();
    const product = makeBaseProduct('gid://shopify/Product/8801', 'Variant Import Hat', 'variant-import-hat');
    store.upsertBaseProducts([product]);
    store.replaceBaseVariantsForProduct(product.id, [
      makeBaseVariant(product.id, 'gid://shopify/ProductVariant/9901', 'Default Title', 'BASE-SKU'),
    ]);

    const stagedUploadPath = 'shopify-draft-proxy/gid://shopify/StagedUploadTarget0/product-variant-create.jsonl';
    const firstVariables = {
      input: {
        productId: product.id,
        title: 'Bulk Variant One',
        sku: 'BULK-VARIANT-ONE',
      },
    };
    const secondVariables = {
      input: {
        productId: 'gid://shopify/Product/404404',
        title: 'Missing Product Variant',
        sku: 'BULK-VARIANT-MISSING',
      },
    };
    store.stageUploadContent(
      [stagedUploadPath],
      `${JSON.stringify(firstVariables)}\n${JSON.stringify(secondVariables)}\n`,
    );

    const innerMutation = `mutation ProductVariantCreate($input: ProductVariantInput!) {
      productVariantCreate(input: $input) {
        product {
          id
          title
        }
        productVariant {
          id
          title
          sku
          selectedOptions {
            name
            value
          }
        }
        userErrors {
          field
          message
        }
      }
    }`;
    const bulkResponse = await request(app)
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
      });

    expect(bulkResponse.status).toBe(200);
    expect(fetchSpy).not.toHaveBeenCalled();
    expect(bulkResponse.body.data.bulkOperationRunMutation.userErrors).toEqual([]);
    expect(bulkResponse.body.data.bulkOperationRunMutation.bulkOperation).toMatchObject({
      status: 'COMPLETED',
      type: 'MUTATION',
      objectCount: '1',
      rootObjectCount: '1',
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
    expect(resultRows).toHaveLength(2);
    expect(resultRows[0]).toMatchObject({
      line: 1,
      response: {
        data: {
          productVariantCreate: {
            product: {
              id: product.id,
              title: 'Variant Import Hat',
            },
            productVariant: {
              title: 'Bulk Variant One',
              sku: 'BULK-VARIANT-ONE',
              selectedOptions: [],
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
          productVariantCreate: {
            product: null,
            productVariant: null,
            userErrors: [{ field: ['input', 'productId'], message: 'Product not found' }],
          },
        },
      },
    });

    const firstResultResponse = resultRows[0]?.['response'] as {
      data: { productVariantCreate: { productVariant: { id: string } } };
    };
    const createdVariantId = firstResultResponse.data.productVariantCreate.productVariant.id;
    const readAfterWriteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query ReadBulkImportedVariant($id: ID!) {
          productVariant(id: $id) {
            id
            title
            sku
            product {
              id
              title
            }
          }
        }`,
        variables: {
          id: createdVariantId,
        },
      });
    const logResponse = await request(app).get('/__meta/log');
    const bulkImportLogEntries = (logResponse.body.entries as BulkImportLogEntryBody[]).filter(
      (
        entry,
      ): entry is BulkImportLogEntryBody & {
        interpreted: { bulkOperationImport: { lineNumber: number; outerRequestBody: unknown } };
      } => Boolean(entry.interpreted.bulkOperationImport),
    );

    expect(readAfterWriteResponse.body.data.productVariant).toEqual({
      id: createdVariantId,
      title: 'Bulk Variant One',
      sku: 'BULK-VARIANT-ONE',
      product: {
        id: product.id,
        title: 'Variant Import Hat',
      },
    });
    expect(logResponse.body.entries).toHaveLength(2);
    expect(bulkImportLogEntries.map((entry) => entry.operationName)).toEqual([
      'ProductVariantCreate',
      'ProductVariantCreate',
    ]);
    expect(bulkImportLogEntries.map((entry) => entry.variables)).toEqual([firstVariables, secondVariables]);
    expect(bulkImportLogEntries.map((entry) => entry.interpreted.bulkOperationImport.lineNumber)).toEqual([1, 2]);
  });

  it('stages supported customer bulk mutation imports from uploaded JSONL', async () => {
    const fetchSpy = vi
      .spyOn(globalThis, 'fetch')
      .mockRejectedValue(new Error('bulkOperationRunMutation customer imports should stay local'));
    const app = createApp(config).callback();
    const stagedUploadPath = 'shopify-draft-proxy/gid://shopify/StagedUploadTarget0/customer-create.jsonl';
    const firstVariables = {
      input: {
        email: 'bulk-customer-one@example.com',
        firstName: 'Bulk',
        lastName: 'Customer One',
        tags: ['bulk-import'],
      },
    };
    const secondVariables = {
      input: {
        email: 'not an email',
        firstName: 'Invalid',
        lastName: 'Customer',
      },
    };
    store.stageUploadContent(
      [stagedUploadPath],
      `${JSON.stringify(firstVariables)}\n${JSON.stringify(secondVariables)}\n`,
    );

    const innerMutation = `mutation CustomerCreate($input: CustomerInput!) {
      customerCreate(input: $input) {
        customer {
          id
          displayName
          email
          tags
        }
        userErrors {
          field
          message
        }
      }
    }`;
    const bulkResponse = await request(app)
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
      });

    expect(bulkResponse.status).toBe(200);
    expect(fetchSpy).not.toHaveBeenCalled();
    expect(bulkResponse.body.data.bulkOperationRunMutation.userErrors).toEqual([]);
    expect(bulkResponse.body.data.bulkOperationRunMutation.bulkOperation).toMatchObject({
      status: 'COMPLETED',
      type: 'MUTATION',
      objectCount: '1',
      rootObjectCount: '1',
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
    expect(resultRows).toHaveLength(2);
    expect(resultRows[0]).toMatchObject({
      line: 1,
      response: {
        data: {
          customerCreate: {
            customer: {
              displayName: 'Bulk Customer One',
              email: 'bulk-customer-one@example.com',
              tags: ['bulk-import'],
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
          customerCreate: {
            customer: null,
            userErrors: [{ field: ['email'], message: 'Email is invalid' }],
          },
        },
      },
    });

    const firstResultResponse = resultRows[0]?.['response'] as {
      data: { customerCreate: { customer: { id: string } } };
    };
    const createdCustomerId = firstResultResponse.data.customerCreate.customer.id;
    const readAfterWriteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query ReadBulkImportedCustomer($id: ID!) {
          customer(id: $id) {
            id
            displayName
            email
            tags
          }
        }`,
        variables: {
          id: createdCustomerId,
        },
      });
    const logResponse = await request(app).get('/__meta/log');
    const bulkImportLogEntries = (logResponse.body.entries as BulkImportLogEntryBody[]).filter(
      (
        entry,
      ): entry is BulkImportLogEntryBody & {
        interpreted: { bulkOperationImport: { lineNumber: number; outerRequestBody: unknown } };
      } => Boolean(entry.interpreted.bulkOperationImport),
    );

    expect(readAfterWriteResponse.body.data.customer).toEqual({
      id: createdCustomerId,
      displayName: 'Bulk Customer One',
      email: 'bulk-customer-one@example.com',
      tags: ['bulk-import'],
    });
    expect(logResponse.body.entries).toHaveLength(2);
    expect(bulkImportLogEntries.map((entry) => entry.operationName)).toEqual(['CustomerCreate', 'CustomerCreate']);
    expect(bulkImportLogEntries.map((entry) => entry.variables)).toEqual([firstVariables, secondVariables]);
    expect(
      bulkImportLogEntries.map((entry) => ({
        domain: entry.interpreted.capability.domain,
        lineNumber: entry.interpreted.bulkOperationImport.lineNumber,
      })),
    ).toEqual([
      { domain: 'customers', lineNumber: 1 },
      { domain: 'customers', lineNumber: 2 },
    ]);
  });

  it('stages supported non-product/customer bulk mutation imports from uploaded JSONL', async () => {
    const fetchSpy = vi
      .spyOn(globalThis, 'fetch')
      .mockRejectedValue(new Error('bulkOperationRunMutation location imports should stay local'));
    const app = createApp(config).callback();
    const stagedUploadPath = 'shopify-draft-proxy/gid://shopify/StagedUploadTarget0/location-add.jsonl';
    const variables = {
      input: {
        name: 'Bulk Warehouse',
        address: { countryCode: 'US', zip: '10006' },
        fulfillsOnlineOrders: false,
      },
    };
    store.stageUploadContent([stagedUploadPath], `${JSON.stringify(variables)}\n`);

    const innerMutation = `mutation LocationAdd($input: LocationAddInput!) {
      locationAdd(input: $input) {
        location {
          id
          name
          fulfillsOnlineOrders
          address {
            countryCode
            zip
          }
        }
        userErrors {
          field
          message
        }
      }
    }`;
    const bulkResponse = await request(app)
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
      });

    expect(bulkResponse.status).toBe(200);
    expect(fetchSpy).not.toHaveBeenCalled();
    expect(bulkResponse.body.data.bulkOperationRunMutation.userErrors).toEqual([]);
    expect(bulkResponse.body.data.bulkOperationRunMutation.bulkOperation).toMatchObject({
      status: 'COMPLETED',
      type: 'MUTATION',
      objectCount: '1',
      rootObjectCount: '1',
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
    expect(resultRows).toHaveLength(1);
    expect(resultRows[0]).toMatchObject({
      line: 1,
      response: {
        data: {
          locationAdd: {
            location: {
              name: 'Bulk Warehouse',
              fulfillsOnlineOrders: false,
              address: { countryCode: 'US', zip: '10006' },
            },
            userErrors: [],
          },
        },
      },
    });

    const firstResultResponse = resultRows[0]?.['response'] as {
      data: { locationAdd: { location: { id: string } } };
    };
    const createdLocationId = firstResultResponse.data.locationAdd.location.id;
    const readAfterWriteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query ReadBulkImportedLocation($id: ID!) {
          location(id: $id) {
            id
            name
            fulfillsOnlineOrders
            address {
              countryCode
              zip
            }
          }
        }`,
        variables: {
          id: createdLocationId,
        },
      });
    const logResponse = await request(app).get('/__meta/log');
    const bulkImportLogEntries = (logResponse.body.entries as BulkImportLogEntryBody[]).filter(
      (
        entry,
      ): entry is BulkImportLogEntryBody & {
        interpreted: { bulkOperationImport: { lineNumber: number; outerRequestBody: unknown } };
      } => Boolean(entry.interpreted.bulkOperationImport),
    );

    expect(readAfterWriteResponse.body.data.location).toEqual({
      id: createdLocationId,
      name: 'Bulk Warehouse',
      fulfillsOnlineOrders: false,
      address: { countryCode: 'US', zip: '10006' },
    });
    expect(logResponse.body.entries).toHaveLength(1);
    expect(bulkImportLogEntries).toHaveLength(1);
    expect(bulkImportLogEntries[0]).toMatchObject({
      operationName: 'LocationAdd',
      variables,
      interpreted: {
        capability: { domain: 'store-properties' },
        bulkOperationImport: { lineNumber: 1 },
      },
    });
  });

  it('replays bulk import inner mutations through meta commit in JSONL line order', async () => {
    const upstreamBodies: unknown[] = [];
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async (_url, init) => {
      const body = JSON.parse(String(init?.body)) as Record<string, unknown>;
      upstreamBodies.push(body);

      return new Response(
        JSON.stringify({
          data: {
            productCreate: {
              product: {
                id: `gid://shopify/Product/${9000 + upstreamBodies.length}`,
              },
              userErrors: [],
            },
          },
        }),
        {
          status: 200,
          headers: { 'content-type': 'application/json' },
        },
      );
    });
    const app = createApp(config).callback();
    const stagedUploadPath = 'shopify-draft-proxy/gid://shopify/StagedUploadTarget0/commit-order.jsonl';
    const innerMutation = `mutation ProductCreate($product: ProductCreateInput!) {
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
    }`;
    const firstVariables = { product: { title: 'Bulk Commit One', status: 'DRAFT' } };
    const secondVariables = { product: { title: 'Bulk Commit Two', status: 'ACTIVE' } };
    store.stageUploadContent(
      [stagedUploadPath],
      `${JSON.stringify(firstVariables)}\n${JSON.stringify(secondVariables)}\n`,
    );

    const bulkResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation BulkImport($mutation: String!, $stagedUploadPath: String!) {
          bulkOperationRunMutation(mutation: $mutation, stagedUploadPath: $stagedUploadPath) {
            bulkOperation {
              id
              status
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
      });
    const logBeforeCommit = await request(app).get('/__meta/log');

    expect(bulkResponse.status).toBe(200);
    expect(bulkResponse.body.data.bulkOperationRunMutation.userErrors).toEqual([]);
    expect(logBeforeCommit.body.entries.map((entry: { operationName: string }) => entry.operationName)).toEqual([
      'ProductCreate',
      'ProductCreate',
    ]);
    expect(logBeforeCommit.body.entries.map((entry: { variables: unknown }) => entry.variables)).toEqual([
      firstVariables,
      secondVariables,
    ]);

    const commitResponse = await request(app)
      .post('/__meta/commit')
      .set('x-shopify-access-token', 'shpat_bulk_commit')
      .set('authorization', 'Bearer bulk_commit_authorization');
    const logAfterCommit = await request(app).get('/__meta/log');

    expect(commitResponse.status).toBe(200);
    expect(commitResponse.body).toMatchObject({
      ok: true,
      stopIndex: null,
      attempts: [
        { operationName: 'ProductCreate', path: '/admin/api/2026-04/graphql.json', success: true },
        { operationName: 'ProductCreate', path: '/admin/api/2026-04/graphql.json', success: true },
      ],
    });
    expect(fetchSpy).toHaveBeenCalledTimes(2);
    expect(upstreamBodies).toEqual([
      {
        query: innerMutation,
        variables: firstVariables,
      },
      {
        query: innerMutation,
        variables: secondVariables,
      },
    ]);
    expect(JSON.stringify(upstreamBodies)).not.toContain('bulkOperationRunMutation');
    expect(logAfterCommit.body.entries.map((entry: { status: string }) => entry.status)).toEqual([
      'committed',
      'committed',
    ]);
  });

  it('marks malformed JSONL mutation imports as failed jobs with inspectable result rows', async () => {
    const fetchSpy = vi
      .spyOn(globalThis, 'fetch')
      .mockRejectedValue(new Error('malformed bulkOperationRunMutation imports should stay local'));
    const app = createApp(config).callback();
    const stagedUploadPath = 'shopify-draft-proxy/gid://shopify/StagedUploadTarget0/malformed-product-create.jsonl';
    const validVariables = { product: { title: 'Bulk Valid Before Malformed', status: 'ACTIVE' } };
    store.stageUploadContent([stagedUploadPath], `${JSON.stringify(validVariables)}\n{"product":\n`);

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
    const bulkResponse = await request(app)
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
        }`,
        variables: {
          mutation: innerMutation,
          stagedUploadPath,
        },
      });

    expect(bulkResponse.status).toBe(200);
    expect(fetchSpy).not.toHaveBeenCalled();
    expect(bulkResponse.body.data.bulkOperationRunMutation.userErrors).toEqual([]);
    expect(bulkResponse.body.data.bulkOperationRunMutation.bulkOperation).toMatchObject({
      status: 'FAILED',
      type: 'MUTATION',
      objectCount: '1',
      rootObjectCount: '1',
      partialDataUrl: null,
      query: innerMutation,
    });

    const operation = bulkResponse.body.data.bulkOperationRunMutation.bulkOperation as {
      id: string;
      url: string;
      fileSize: string;
    };
    const resultResponse = await request(app).get(new URL(operation.url).pathname);
    const resultRows = resultResponse.text
      .trim()
      .split('\n')
      .map((line) => JSON.parse(line) as Record<string, unknown>);

    expect(resultResponse.status).toBe(200);
    expect(operation.fileSize).toBe(String(Buffer.byteLength(resultResponse.text, 'utf8')));
    expect(resultRows).toHaveLength(2);
    expect(resultRows[0]).toMatchObject({
      line: 1,
      response: {
        data: {
          productCreate: {
            product: {
              title: 'Bulk Valid Before Malformed',
              handle: 'bulk-valid-before-malformed',
              status: 'ACTIVE',
            },
            userErrors: [],
          },
        },
      },
    });
    expect(resultRows[1]).toMatchObject({
      line: 2,
      errors: [{ message: expect.stringContaining('Unexpected end of JSON input') }],
    });

    const readResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query ReadFailedBulkMutation($id: ID!) {
          byId: bulkOperation(id: $id) {
            id
            status
            type
            objectCount
            rootObjectCount
            url
            partialDataUrl
          }
          failedMutations: bulkOperations(first: 5, query: "status:FAILED operation_type:MUTATION") {
            nodes {
              id
              status
              type
            }
          }
          currentMutation: currentBulkOperation(type: MUTATION) {
            id
            status
            type
          }
        }`,
        variables: { id: operation.id },
      });
    const logResponse = await request(app).get('/__meta/log');

    expect(readResponse.status).toBe(200);
    expect(readResponse.body.data).toEqual({
      byId: {
        id: operation.id,
        status: 'FAILED',
        type: 'MUTATION',
        objectCount: '1',
        rootObjectCount: '1',
        url: operation.url,
        partialDataUrl: null,
      },
      failedMutations: {
        nodes: [{ id: operation.id, status: 'FAILED', type: 'MUTATION' }],
      },
      currentMutation: {
        id: operation.id,
        status: 'FAILED',
        type: 'MUTATION',
      },
    });
    expect(logResponse.body.entries).toHaveLength(1);
    expect(logResponse.body.entries[0]).toMatchObject({
      operationName: 'ProductCreate',
      variables: validVariables,
      interpreted: {
        bulkOperationImport: {
          bulkOperationId: operation.id,
          lineNumber: 1,
          stagedUploadPath,
        },
      },
    });
  });

  it('fails unsupported bulk mutation import roots locally without upstream passthrough', async () => {
    const fetchSpy = vi
      .spyOn(globalThis, 'fetch')
      .mockRejectedValue(new Error('unsupported bulkOperationRunMutation imports should not proxy upstream'));
    const app = createApp(config).callback();

    store.stageUploadContent(
      ['shopify-draft-proxy/gid://shopify/StagedUploadTarget0/unsupported.jsonl'],
      `${JSON.stringify({ id: 'gid://shopify/CompanyContact/404404' })}\n`,
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
          mutation: `mutation CompanyContactSendWelcomeEmail($id: ID!) {
            companyContactSendWelcomeEmail(companyContactId: $id) {
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
            'bulkOperationRunMutation locally supports only single-root Admin mutations with local staging support in the proxy.',
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

import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { classifyParityScenarioState, type ParitySpec } from '../../scripts/conformance-parity-lib.js';
import { loadConformanceScenarios } from '../../scripts/conformance-scenario-registry.js';
import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { parseOperation } from '../../src/graphql/parse-operation.js';
import { getOperationCapability } from '../../src/proxy/capabilities.js';
import { findOperationRegistryEntry } from '../../src/proxy/operation-registry.js';
import { resetSyntheticIdentity } from '../../src/state/synthetic-identity.js';
import { store } from '../../src/state/store.js';

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

function mockCapturedUpstream(interaction: CapturedInteraction) {
  return vi.spyOn(globalThis, 'fetch').mockImplementation(async (input, init) => {
    expect(String(input)).toBe('https://example.myshopify.com/admin/api/2026-04/graphql.json');
    expect(init?.method).toBe('POST');

    const headers = init?.headers as Record<string, string>;
    expect(headers['x-shopify-access-token']).toBe('shpat_test');

    expect(typeof init?.body).toBe('string');
    expect(JSON.parse(init?.body as string)).toEqual({
      query: interaction.query,
      variables: interaction.variables,
    });

    return new Response(JSON.stringify(interaction.response), {
      status: interaction.status,
      headers: { 'content-type': 'application/json' },
    });
  });
}

describe('BulkOperation conformance fixture', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('discovers captured fixture evidence while keeping BulkOperation roots as unsupported proxy gaps', () => {
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
    expect(classifyParityScenarioState(scenario!, paritySpec)).toBe('enforced-by-fixture');

    for (const [operationType, rootField] of [
      ['query', 'bulkOperation'],
      ['query', 'bulkOperations'],
      ['query', 'currentBulkOperation'],
      ['mutation', 'bulkOperationRunQuery'],
      ['mutation', 'bulkOperationCancel'],
    ] as const) {
      expect(findOperationRegistryEntry(operationType, [rootField])).toMatchObject({
        domain: 'bulk-operations',
        implemented: false,
      });
    }

    expect(
      getOperationCapability(parseOperation(readInteraction('reads', 'catalogEmptyRunningMutation').query)),
    ).toEqual({
      type: 'query',
      operationName: 'BulkOperationsCatalogCapture',
      domain: 'unknown',
      execution: 'passthrough',
    });
    expect(
      getOperationCapability(parseOperation(readInteraction('validations', 'bulkOperationCancelUnknownId').query)),
    ).toEqual({
      type: 'mutation',
      operationName: 'BulkOperationCancelCapture',
      domain: 'unknown',
      execution: 'passthrough',
    });
  });

  it('proxies recorded BulkOperation reads unchanged in snapshot mode until local job modeling exists', async () => {
    const interaction = readInteraction('reads', 'catalogEmptyRunningMutation');
    const fetchSpy = mockCapturedUpstream(interaction);
    const app = createApp(config).callback();

    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({
        query: interaction.query,
        variables: interaction.variables,
        operationName: interaction.operationName,
      });

    expect(response.status).toBe(interaction.status);
    expect(response.body).toEqual(interaction.response);
    expect(fetchSpy).toHaveBeenCalledTimes(1);
  });

  it('logs recorded bulkOperationCancel as a registered unsupported passthrough boundary', async () => {
    const interaction = readInteraction('validations', 'bulkOperationCancelUnknownId');
    const fetchSpy = mockCapturedUpstream(interaction);
    const app = createApp(config).callback();

    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({
        query: interaction.query,
        variables: interaction.variables,
        operationName: interaction.operationName,
      });

    expect(response.status).toBe(interaction.status);
    expect(response.body).toEqual(interaction.response);
    expect(fetchSpy).toHaveBeenCalledTimes(1);

    const logResponse = await request(app).get('/__meta/log');
    expect(logResponse.body.entries).toHaveLength(1);
    expect(logResponse.body.entries[0]).toMatchObject({
      operationName: 'BulkOperationCancelCapture',
      status: 'proxied',
      interpreted: {
        operationType: 'mutation',
        rootFields: ['bulkOperationCancel'],
        capability: {
          domain: 'unknown',
          execution: 'passthrough',
        },
        registeredOperation: {
          name: 'bulkOperationCancel',
          domain: 'bulk-operations',
          execution: 'stage-locally',
          implemented: false,
        },
      },
      notes: 'Mutation passthrough placeholder until supported local staging is implemented.',
    });
  });
});

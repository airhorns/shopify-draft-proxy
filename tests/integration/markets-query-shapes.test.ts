import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { hydrateMarketsFromUpstreamResponse } from '../../src/proxy/markets.js';
import { resetSyntheticIdentity } from '../../src/state/synthetic-identity.js';
import { store } from '../../src/state/store.js';

const repoRoot = process.cwd();
const fixtureRoot = 'fixtures/conformance/very-big-test-store.myshopify.com/2026-04';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

function readText(relativePath: string): string {
  return readFileSync(resolve(repoRoot, relativePath), 'utf8');
}

function readJson<T>(relativePath: string): T {
  return JSON.parse(readText(relativePath)) as T;
}

describe('Markets query shapes', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it.each([
    {
      name: 'markets catalog',
      documentPath: 'config/parity-requests/markets-catalog-read.graphql',
      variablesPath: 'config/parity-requests/markets-catalog-read.variables.json',
      fixturePath: `${fixtureRoot}/markets-catalog.json`,
    },
    {
      name: 'market detail',
      documentPath: 'config/parity-requests/market-detail-read.graphql',
      variablesPath: 'config/parity-requests/market-detail-read.variables.json',
      fixturePath: `${fixtureRoot}/market-detail.json`,
    },
    {
      name: 'market catalogs',
      documentPath: 'config/parity-requests/market-catalogs-read.graphql',
      variablesPath: 'config/parity-requests/market-catalogs-read.variables.json',
      fixturePath: `${fixtureRoot}/market-catalogs.json`,
    },
    {
      name: 'market web presences',
      documentPath: 'config/parity-requests/market-web-presences-read.graphql',
      variablesPath: 'config/parity-requests/market-web-presences-read.variables.json',
      fixturePath: `${fixtureRoot}/market-web-presences.json`,
    },
    {
      name: 'markets resolved values',
      documentPath: 'config/parity-requests/markets-resolved-values-read.graphql',
      variablesPath: 'config/parity-requests/markets-resolved-values-read.variables.json',
      fixturePath: `${fixtureRoot}/markets-resolved-values.json`,
    },
  ])('serves captured $name from local snapshot state', async ({ documentPath, variablesPath, fixturePath }) => {
    const document = readText(documentPath);
    const variables = readJson<Record<string, unknown>>(variablesPath);
    const fixture = readJson<{ data: Record<string, unknown> }>(fixturePath);
    hydrateMarketsFromUpstreamResponse(document, variables, fixture);

    vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('snapshot Markets reads must not fetch upstream'));
    const app = createApp(config);

    const response = await request(app.callback()).post('/admin/api/2026-04/graphql.json').send({
      query: document,
      variables,
    });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({ data: fixture.data });
    expect(globalThis.fetch).not.toHaveBeenCalled();
  });

  it('returns Shopify-like null and empty connections for absent Markets snapshot data', async () => {
    vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('snapshot Markets reads must not fetch upstream'));
    const app = createApp(config);

    const response = await request(app.callback())
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          query EmptyMarkets($id: ID!) {
            market(id: $id) {
              id
            }
            markets(first: 3) {
              edges {
                cursor
              }
              pageInfo {
                hasNextPage
                hasPreviousPage
                startCursor
                endCursor
              }
            }
            webPresences(first: 3) {
              edges {
                cursor
              }
              pageInfo {
                hasNextPage
                hasPreviousPage
                startCursor
                endCursor
              }
            }
          }
        `,
        variables: {
          id: 'gid://shopify/Market/0',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        market: null,
        markets: {
          edges: [],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: false,
            startCursor: null,
            endCursor: null,
          },
        },
        webPresences: {
          edges: [],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: false,
            startCursor: null,
            endCursor: null,
          },
        },
      },
    });
    expect(globalThis.fetch).not.toHaveBeenCalled();
  });
});

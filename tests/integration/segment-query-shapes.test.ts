import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../support/runtime.js';
import type { AppConfig } from '../../src/config.js';
import { hydrateSegmentsFromUpstreamResponse } from '../../src/proxy/segments.js';
import { resetSyntheticIdentity } from '../support/runtime.js';
import { store } from '../support/runtime.js';
import { withRuntimeContext } from '../support/runtime.js';

const repoRoot = process.cwd();
const fixtureRoot = 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01';

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

describe('segment query shapes', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('serves captured segment catalog, detail, count, and metadata reads from local snapshot state', async () => {
    const document = readText('config/parity-requests/segments-baseline-read.graphql');
    const variables = readJson<Record<string, unknown>>('config/parity-requests/segments-baseline-read.variables.json');
    const fixture = readJson<{ data: Record<string, unknown>; errors: Array<Record<string, unknown>> }>(
      `${fixtureRoot}/segments-baseline.json`,
    );
    withRuntimeContext((runtime) => hydrateSegmentsFromUpstreamResponse(runtime, document, variables, fixture));

    vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('snapshot segment reads must not fetch upstream'));
    const app = createApp(config);

    const response = await request(app.callback()).post('/admin/api/2025-01/graphql.json').send({
      query: document,
      variables,
    });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: fixture.data,
      errors: fixture.errors,
    });
    expect(globalThis.fetch).not.toHaveBeenCalled();
  });

  it('returns Shopify-like empty segment shapes in snapshot mode when no segment data is hydrated', async () => {
    vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('empty snapshot segment reads must not fetch upstream'));
    const app = createApp(config);

    const response = await request(app.callback())
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query EmptySegments($id: ID!, $search: String!, $filter: String!) {
          missing: segment(id: $id) {
            id
            name
            query
            creationDate
            lastEditDate
          }
          segments(first: 5) {
            nodes {
              id
            }
            edges {
              cursor
              node {
                id
              }
            }
            pageInfo {
              hasNextPage
              hasPreviousPage
              startCursor
              endCursor
            }
          }
          segmentsCount {
            count
            precision
          }
          segmentFilters(first: 2) {
            nodes {
              queryName
              localizedName
              multiValue
            }
            edges {
              cursor
              node {
                queryName
              }
            }
            pageInfo {
              hasNextPage
              hasPreviousPage
              startCursor
              endCursor
            }
          }
          segmentFilterSuggestions(first: 2, search: $search) {
            edges {
              cursor
              node {
                queryName
              }
            }
            pageInfo {
              hasNextPage
              hasPreviousPage
              startCursor
              endCursor
            }
          }
          segmentValueSuggestions(first: 2, search: $search, filterQueryName: $filter) {
            edges {
              cursor
              node {
                queryName
                localizedValue
              }
            }
            pageInfo {
              hasNextPage
              hasPreviousPage
              startCursor
              endCursor
            }
          }
          segmentMigrations(first: 2) {
            edges {
              cursor
              node {
                id
                savedSearchId
                segmentId
              }
            }
            pageInfo {
              hasNextPage
              hasPreviousPage
              startCursor
              endCursor
            }
          }
        }`,
        variables: {
          id: 'gid://shopify/Segment/999999999999',
          search: 'email',
          filter: 'customer_tags',
        },
      });

    const emptyConnection = {
      edges: [],
      pageInfo: {
        hasNextPage: false,
        hasPreviousPage: false,
        startCursor: null,
        endCursor: null,
      },
    };

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        missing: null,
        segments: {
          nodes: [],
          ...emptyConnection,
        },
        segmentsCount: {
          count: 0,
          precision: 'EXACT',
        },
        segmentFilters: {
          nodes: [],
          ...emptyConnection,
        },
        segmentFilterSuggestions: emptyConnection,
        segmentValueSuggestions: emptyConnection,
        segmentMigrations: emptyConnection,
      },
      errors: [
        {
          message: 'Segment does not exist',
          locations: [
            {
              line: 2,
              column: 11,
            },
          ],
          path: ['missing'],
          extensions: {
            code: 'NOT_FOUND',
          },
        },
      ],
    });
    expect(globalThis.fetch).not.toHaveBeenCalled();
  });
});

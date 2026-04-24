import { describe, expect, it, vi } from 'vitest';

import {
  ConformanceGraphqlError,
  formatGraphqlError,
  runAdminGraphql,
  runAdminGraphqlRequest,
} from '../../scripts/conformance-graphql-client.js';

const adminOptions = {
  adminOrigin: 'https://very-big-test-store.myshopify.com',
  apiVersion: '2026-04',
  headers: {
    'X-Shopify-Access-Token': 'shpca_test',
  },
};

describe('conformance GraphQL client', () => {
  it('sends Admin GraphQL requests with shared headers and variables', async () => {
    const fetchMock = vi.fn().mockResolvedValueOnce(
      new Response(JSON.stringify({ data: { shop: { id: 'gid://shopify/Shop/1' } } }), {
        status: 200,
        headers: { 'Content-Type': 'application/json' },
      }),
    );

    const payload = await runAdminGraphql(
      {
        ...adminOptions,
        fetchImpl: fetchMock,
      },
      'query ShopProbe($first: Int!) { shop { id } }',
      { first: 1 },
    );

    expect(payload).toEqual({ data: { shop: { id: 'gid://shopify/Shop/1' } } });
    expect(fetchMock).toHaveBeenCalledWith('https://very-big-test-store.myshopify.com/admin/api/2026-04/graphql.json', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'X-Shopify-Access-Token': 'shpca_test',
      },
      body: JSON.stringify({ query: 'query ShopProbe($first: Int!) { shop { id } }', variables: { first: 1 } }),
    });
  });

  it('throws a result-bearing error for GraphQL errors', async () => {
    const fetchMock = vi.fn().mockResolvedValueOnce(
      new Response(JSON.stringify({ errors: [{ message: 'Access denied' }] }), {
        status: 200,
        headers: { 'Content-Type': 'application/json' },
      }),
    );

    await expect(
      runAdminGraphql(
        {
          ...adminOptions,
          fetchImpl: fetchMock,
        },
        'query AccessProbe { shop { id } }',
      ),
    ).rejects.toMatchObject({
      name: 'ConformanceGraphqlError',
      message: 'Access denied',
      result: {
        status: 200,
        payload: { errors: [{ message: 'Access denied' }] },
      },
    } satisfies Partial<ConformanceGraphqlError>);
  });

  it('can return raw results for scripts that intentionally inspect error payloads', async () => {
    const fetchMock = vi.fn().mockResolvedValueOnce(
      new Response(JSON.stringify({ errors: 'Invalid API key or access token' }), {
        status: 401,
        headers: { 'Content-Type': 'application/json' },
      }),
    );

    await expect(
      runAdminGraphqlRequest(
        {
          ...adminOptions,
          fetchImpl: fetchMock,
        },
        'query RawProbe { shop { id } }',
      ),
    ).resolves.toEqual({
      status: 401,
      payload: { errors: 'Invalid API key or access token' },
    });
  });

  it('formats Shopify error payloads without requiring script-local duplication', () => {
    expect(formatGraphqlError({ errors: [{ message: 'First' }, { message: 'Second' }] }, 200)).toBe('First; Second');
    expect(formatGraphqlError({ errors: 'Unauthorized' }, 401)).toBe('Unauthorized');
    expect(formatGraphqlError({ data: null }, 500)).toBe('HTTP 500');
  });
});

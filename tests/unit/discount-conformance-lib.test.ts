import { describe, expect, it, vi } from 'vitest';

import {
  assertDiscountConformanceScopes,
  captureDiscountReadEvidence,
  probeDiscountConformanceScopes,
} from '../../scripts/discount-conformance-lib.js';

const adminOptions = {
  adminOrigin: 'https://very-big-test-store.myshopify.com',
  apiVersion: '2026-04',
  accessToken: 'shpca_test',
  headers: {
    'X-Shopify-Access-Token': 'shpca_test',
  },
};

describe('discount conformance helpers', () => {
  it('records read_discounts and write_discounts scope availability', async () => {
    const fetchMock = vi.fn().mockResolvedValueOnce(
      new Response(
        JSON.stringify({
          data: {
            currentAppInstallation: {
              accessScopes: [{ handle: 'write_products' }, { handle: 'read_discounts' }],
            },
          },
        }),
        {
          status: 200,
          headers: { 'Content-Type': 'application/json' },
        },
      ),
    );

    const probe = await probeDiscountConformanceScopes({
      ...adminOptions,
      fetchImpl: fetchMock,
    });

    expect(probe).toEqual({
      availableScopes: ['read_discounts', 'write_products'],
      requiredScopes: [
        { handle: 'read_discounts', present: true },
        { handle: 'write_discounts', present: false },
      ],
      missingScopes: ['write_discounts'],
      hasRequiredScopes: false,
    });
  });

  it('fails clearly before capture when required discount scopes are missing', () => {
    expect(() =>
      assertDiscountConformanceScopes({
        availableScopes: ['write_products'],
        requiredScopes: [
          { handle: 'read_discounts', present: false },
          { handle: 'write_discounts', present: false },
        ],
        missingScopes: ['read_discounts', 'write_discounts'],
        hasRequiredScopes: false,
      }),
    ).toThrow(
      'Discount conformance capture requires Shopify Admin scopes read_discounts and write_discounts. Missing: read_discounts, write_discounts.',
    );
  });

  it('keeps discount read capture behind the explicit scope probe', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify({
            data: {
              discountNodesCount: {
                count: 0,
                precision: 'EXACT',
              },
            },
          }),
          {
            status: 200,
            headers: { 'Content-Type': 'application/json' },
          },
        ),
      )
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify({
            data: {
              discountNodes: {
                edges: [],
                pageInfo: {
                  hasNextPage: false,
                  hasPreviousPage: false,
                  startCursor: null,
                  endCursor: null,
                },
              },
            },
          }),
          {
            status: 200,
            headers: { 'Content-Type': 'application/json' },
          },
        ),
      );

    const capture = await captureDiscountReadEvidence(
      {
        ...adminOptions,
        fetchImpl: fetchMock,
      },
      { first: 10 },
    );

    expect(capture).toEqual({
      discountNodesCount: {
        data: {
          discountNodesCount: {
            count: 0,
            precision: 'EXACT',
          },
        },
      },
      discountNodesCatalog: {
        data: {
          discountNodes: {
            edges: [],
            pageInfo: {
              hasNextPage: false,
              hasPreviousPage: false,
              startCursor: null,
              endCursor: null,
            },
          },
        },
      },
    });
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });
});

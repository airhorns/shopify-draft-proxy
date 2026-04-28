import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../support/runtime.js';
import type { AppConfig } from '../../src/config.js';
import { resetSyntheticIdentity } from '../support/runtime.js';
import { store } from '../support/runtime.js';
import type { DiscountRecord } from '../../src/state/types.js';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'live-hybrid',
};

function buildCodeDiscount(overrides: Partial<DiscountRecord> = {}): DiscountRecord {
  return {
    id: 'gid://shopify/DiscountCodeNode/197001',
    typeName: 'DiscountCodeBasic',
    method: 'code',
    title: 'HAR-197 redeem code bulk fixture',
    status: 'ACTIVE',
    summary: '10% off entire order',
    startsAt: '2026-04-25T00:00:00Z',
    endsAt: null,
    createdAt: '2026-04-25T20:00:00Z',
    updatedAt: '2026-04-25T20:00:00Z',
    asyncUsageCount: 0,
    discountClasses: ['ORDER'],
    combinesWith: {
      productDiscounts: false,
      orderDiscounts: true,
      shippingDiscounts: false,
    },
    codes: ['HAR197BASE'],
    redeemCodes: [
      {
        id: 'gid://shopify/DiscountRedeemCode/197001',
        code: 'HAR197BASE',
        asyncUsageCount: 0,
      },
    ],
    context: { typeName: 'DiscountBuyerSelectionAll', all: 'ALL' },
    customerGets: {
      value: { typeName: 'DiscountPercentage', percentage: 0.1 },
      items: { typeName: 'AllDiscountItems', allItems: true },
      appliesOnOneTimePurchase: true,
      appliesOnSubscription: false,
    },
    minimumRequirement: null,
    metafields: [],
    events: [],
    discountType: 'percentage',
    appId: null,
    ...overrides,
  };
}

describe('discount redeem-code bulk staging', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('stages redeem-code bulk add and id-scoped delete locally with stable job payloads and downstream reads', async () => {
    store.upsertBaseDiscounts([buildCodeDiscount()]);
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('redeem-code bulk runtime staging should not hit upstream fetch');
    });
    const app = createApp(config).callback();

    const addResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation AddRedeemCodes($discountId: ID!, $codes: [String!]!) {
          discountRedeemCodeBulkAdd(discountId: $discountId, codes: $codes) {
            bulkCreation {
              id
              done
              codesCount
              importedCount
              failedCount
            }
            userErrors { field message code extraInfo }
          }
        }`,
        variables: {
          discountId: 'gid://shopify/DiscountCodeNode/197001',
          codes: ['HAR197ADD1', 'HAR197ADD2'],
        },
      });

    expect(addResponse.status).toBe(200);
    expect(addResponse.body.data.discountRedeemCodeBulkAdd).toEqual({
      bulkCreation: {
        id: 'gid://shopify/DiscountRedeemCodeBulkCreation/1?shopify-draft-proxy=synthetic',
        done: true,
        codesCount: 2,
        importedCount: 2,
        failedCount: 0,
      },
      userErrors: [],
    });

    const readAfterAdd = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query ReadCodes($id: ID!, $code: String!, $lowerCode: String!) {
          codeDiscountNode(id: $id) {
            codeDiscount {
              ... on DiscountCodeBasic {
                codes(first: 10) {
                  nodes { id code asyncUsageCount }
                }
                codesCount { count precision }
              }
            }
          }
          codeDiscountNodeByCode(code: $code) { id }
          lowerCaseLookup: codeDiscountNodeByCode(code: $lowerCode) { id }
          discountNodes(first: 10, query: "status:active") { nodes { id } }
        }`,
        variables: {
          id: 'gid://shopify/DiscountCodeNode/197001',
          code: 'HAR197ADD1',
          lowerCode: 'har197add1',
        },
      });

    expect(readAfterAdd.status).toBe(200);
    expect(readAfterAdd.body.data.codeDiscountNode.codeDiscount.codes.nodes).toEqual([
      { id: 'gid://shopify/DiscountRedeemCode/197001', code: 'HAR197BASE', asyncUsageCount: 0 },
      {
        id: 'gid://shopify/DiscountRedeemCode/2?shopify-draft-proxy=synthetic',
        code: 'HAR197ADD1',
        asyncUsageCount: 0,
      },
      {
        id: 'gid://shopify/DiscountRedeemCode/3?shopify-draft-proxy=synthetic',
        code: 'HAR197ADD2',
        asyncUsageCount: 0,
      },
    ]);
    expect(readAfterAdd.body.data.codeDiscountNode.codeDiscount.codesCount).toEqual({
      count: 3,
      precision: 'EXACT',
    });
    expect(readAfterAdd.body.data.codeDiscountNodeByCode).toEqual({
      id: 'gid://shopify/DiscountCodeNode/197001',
    });
    expect(readAfterAdd.body.data.lowerCaseLookup).toEqual({
      id: 'gid://shopify/DiscountCodeNode/197001',
    });
    expect(readAfterAdd.body.data.discountNodes.nodes).toEqual([{ id: 'gid://shopify/DiscountCodeNode/197001' }]);

    const deleteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation DeleteRedeemCodes($discountId: ID!, $ids: [ID!]!) {
          discountCodeRedeemCodeBulkDelete(discountId: $discountId, ids: $ids) {
            job { id done query }
            userErrors { field message code extraInfo }
          }
        }`,
        variables: {
          discountId: 'gid://shopify/DiscountCodeNode/197001',
          ids: ['gid://shopify/DiscountRedeemCode/197001'],
        },
      });

    expect(deleteResponse.status).toBe(200);
    expect(deleteResponse.body.data.discountCodeRedeemCodeBulkDelete).toEqual({
      job: {
        id: 'gid://shopify/Job/5?shopify-draft-proxy=synthetic',
        done: true,
        query: null,
      },
      userErrors: [],
    });

    const readAfterDelete = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query ReadCodes($id: ID!, $removedCode: String!) {
          codeDiscountNode(id: $id) {
            codeDiscount {
              ... on DiscountCodeBasic {
                codes(first: 10) { nodes { id code } }
                codesCount { count precision }
              }
            }
          }
          removed: codeDiscountNodeByCode(code: $removedCode) { id }
        }`,
        variables: {
          id: 'gid://shopify/DiscountCodeNode/197001',
          removedCode: 'HAR197BASE',
        },
      });

    expect(readAfterDelete.body.data.codeDiscountNode.codeDiscount.codes.nodes).toEqual([
      { id: 'gid://shopify/DiscountRedeemCode/2?shopify-draft-proxy=synthetic', code: 'HAR197ADD1' },
      { id: 'gid://shopify/DiscountRedeemCode/3?shopify-draft-proxy=synthetic', code: 'HAR197ADD2' },
    ]);
    expect(readAfterDelete.body.data.codeDiscountNode.codeDiscount.codesCount).toEqual({
      count: 2,
      precision: 'EXACT',
    });
    expect(readAfterDelete.body.data.removed).toBeNull();

    const stateResponse = await request(app).get('/__meta/state');
    expect(Object.values(stateResponse.body.stagedState.discountBulkOperations)).toEqual([
      expect.objectContaining({
        id: 'gid://shopify/DiscountRedeemCodeBulkCreation/1?shopify-draft-proxy=synthetic',
        operation: 'discountRedeemCodeBulkAdd',
        status: 'COMPLETED',
        done: true,
        codesCount: 2,
        importedCount: 2,
        failedCount: 0,
      }),
      expect.objectContaining({
        id: 'gid://shopify/Job/5?shopify-draft-proxy=synthetic',
        operation: 'discountCodeRedeemCodeBulkDelete',
        status: 'COMPLETED',
        done: true,
        redeemCodeIds: ['gid://shopify/DiscountRedeemCode/197001'],
      }),
    ]);

    const logBeforeCommit = await request(app).get('/__meta/log');
    expect(logBeforeCommit.body.entries.map((entry: { operationName: string }) => entry.operationName)).toEqual([
      'discountRedeemCodeBulkAdd',
      'discountCodeRedeemCodeBulkDelete',
    ]);
    expect(logBeforeCommit.body.entries.every((entry: { status: string }) => entry.status === 'staged')).toBe(true);
    expect(fetchSpy).not.toHaveBeenCalled();

    const replayQueries: string[] = [];
    fetchSpy.mockImplementation(async (_input: Parameters<typeof fetch>[0], init?: Parameters<typeof fetch>[1]) => {
      const body = JSON.parse(String(init?.body ?? '{}')) as { query?: string };
      replayQueries.push(String(body.query ?? ''));
      if (body.query?.includes('discountRedeemCodeBulkAdd')) {
        return new Response(
          JSON.stringify({
            data: {
              discountRedeemCodeBulkAdd: {
                bulkCreation: { id: 'gid://shopify/DiscountRedeemCodeBulkCreation/9001' },
                userErrors: [],
              },
            },
          }),
          { status: 200, headers: { 'content-type': 'application/json' } },
        );
      }

      return new Response(
        JSON.stringify({
          data: {
            discountCodeRedeemCodeBulkDelete: {
              job: { id: 'gid://shopify/Job/9002' },
              userErrors: [],
            },
          },
        }),
        { status: 200, headers: { 'content-type': 'application/json' } },
      );
    });

    const commitResponse = await request(app).post('/__meta/commit').set('x-shopify-access-token', 'shpat_commit');

    expect(commitResponse.status).toBe(200);
    expect(commitResponse.body.attempts).toEqual([
      expect.objectContaining({ operationName: 'discountRedeemCodeBulkAdd', status: 'committed', success: true }),
      expect.objectContaining({
        operationName: 'discountCodeRedeemCodeBulkDelete',
        status: 'committed',
        success: true,
      }),
    ]);
    expect(replayQueries[0]).toContain('discountRedeemCodeBulkAdd');
    expect(replayQueries[1]).toContain('discountCodeRedeemCodeBulkDelete');
    expect(fetchSpy).toHaveBeenCalledTimes(2);
  });

  it('refuses broad destructive bulk inputs locally and stages id-scoped bulk paths without passthrough', async () => {
    store.upsertBaseDiscounts([buildCodeDiscount({ status: 'EXPIRED' })]);
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          data: {
            discountCodeBulkActivate: {
              job: { id: 'gid://shopify/Job/upstream' },
              userErrors: [],
            },
          },
        }),
        { status: 200, headers: { 'content-type': 'application/json' } },
      ),
    );
    const app = createApp(config).callback();

    const refusedResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation RefuseBroadBulk {
          blankCodeSearch: discountCodeBulkDelete(search: "") {
            job { id }
            userErrors { field message code extraInfo }
          }
          allAutomatic: discountAutomaticBulkDelete {
            job { id }
            userErrors { field message code extraInfo }
          }
        }`,
      });

    expect(refusedResponse.status).toBe(200);
    expect(refusedResponse.body.data.blankCodeSearch).toEqual({
      job: null,
      userErrors: [
        {
          field: ['search'],
          message: 'Local proxy refuses blank bulk search selectors to avoid broad destructive discount writes.',
          code: 'INVALID',
          extraInfo: null,
        },
      ],
    });
    expect(refusedResponse.body.data.allAutomatic).toEqual({
      job: null,
      userErrors: [
        {
          field: null,
          message:
            'Local proxy refuses discount bulk mutations without ids, search, or savedSearchId to avoid broad destructive writes.',
          code: 'INVALID',
          extraInfo: null,
        },
      ],
    });
    expect(fetchSpy).not.toHaveBeenCalled();

    const passthroughResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation UnsupportedIdScopedBulk($ids: [ID!]) {
          discountCodeBulkActivate(ids: $ids) {
            job { id }
            userErrors { field message code extraInfo }
          }
        }`,
        variables: {
          ids: ['gid://shopify/DiscountCodeNode/197001'],
        },
      });

    expect(passthroughResponse.status).toBe(200);
    expect(passthroughResponse.body.data.discountCodeBulkActivate).toEqual({
      job: {
        id: 'gid://shopify/Job/1?shopify-draft-proxy=synthetic',
      },
      userErrors: [],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
    const logResponse = await request(app).get('/__meta/log');
    expect(logResponse.body.entries).toEqual([
      expect.objectContaining({
        operationName: 'discountCodeBulkActivate',
        status: 'staged',
        interpreted: expect.objectContaining({
          capability: expect.objectContaining({
            operationName: 'discountCodeBulkActivate',
            domain: 'discounts',
            execution: 'stage-locally',
          }),
        }),
        notes: 'Staged locally in the in-memory discount draft store.',
      }),
    ]);
  });
});

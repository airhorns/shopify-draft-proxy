import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { AppConfig } from '../../src/config.js';
import type { DiscountRecord } from '../../src/state/types.js';
import { createApp, resetSyntheticIdentity, store } from '../support/runtime.js';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'live-hybrid',
};

function buildDiscount(overrides: Partial<DiscountRecord> = {}): DiscountRecord {
  return {
    id: 'gid://shopify/DiscountCodeNode/366001',
    typeName: 'DiscountCodeBasic',
    method: 'code',
    title: 'HAR-366 code fixture',
    status: 'EXPIRED',
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
    codes: ['HAR366BASE'],
    redeemCodes: [
      {
        id: 'gid://shopify/DiscountRedeemCode/366001',
        code: 'HAR366BASE',
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

describe('discount app and broad bulk staging', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('stages app-managed code and automatic discounts when Function evidence is present', async () => {
    store.upsertStagedShopifyFunction({
      id: 'gid://shopify/ShopifyFunction/discount-local',
      title: 'Local volume discount',
      handle: 'discount-local',
      apiType: 'DISCOUNT',
      description: 'Captured local Function metadata',
      appKey: 'app-key-366',
    });
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('app-discount local staging must not hit upstream fetch');
    });
    const app = createApp(config).callback();

    const createResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation CreateAppDiscounts($codeInput: DiscountCodeAppInput!, $automaticInput: DiscountAutomaticAppInput!) {
          discountCodeAppCreate(codeAppDiscount: $codeInput) {
            codeAppDiscount {
              __typename
              discountId
              title
              status
              usageLimit
              combinesWith { orderDiscounts productDiscounts shippingDiscounts }
              codes(first: 5) { nodes { code } }
              appDiscountType { appKey functionId title description }
            }
            userErrors { field message code extraInfo }
          }
          discountAutomaticAppCreate(automaticAppDiscount: $automaticInput) {
            automaticAppDiscount {
              __typename
              discountId
              title
              status
              recurringCycleLimit
              appDiscountType { appKey functionId title description }
            }
            userErrors { field message code extraInfo }
          }
        }`,
        variables: {
          codeInput: {
            title: 'HAR-366 code app',
            code: 'HAR366APP',
            startsAt: '2024-01-01T00:00:00.000Z',
            functionHandle: 'discount-local',
            usageLimit: 10,
            combinesWith: { orderDiscounts: true, productDiscounts: false, shippingDiscounts: true },
          },
          automaticInput: {
            title: 'HAR-366 automatic app',
            startsAt: '2024-01-01T00:00:00.000Z',
            functionHandle: 'discount-local',
            recurringCycleLimit: 0,
          },
        },
      });

    expect(createResponse.status).toBe(200);
    expect(createResponse.body.data.discountCodeAppCreate).toEqual({
      codeAppDiscount: {
        __typename: 'DiscountCodeApp',
        discountId: 'gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic',
        title: 'HAR-366 code app',
        status: 'ACTIVE',
        usageLimit: 10,
        combinesWith: { orderDiscounts: true, productDiscounts: false, shippingDiscounts: true },
        codes: { nodes: [{ code: 'HAR366APP' }] },
        appDiscountType: {
          appKey: 'app-key-366',
          functionId: 'discount-local',
          title: 'Local volume discount',
          description: 'Captured local Function metadata',
        },
      },
      userErrors: [],
    });
    expect(createResponse.body.data.discountAutomaticAppCreate).toEqual({
      automaticAppDiscount: {
        __typename: 'DiscountAutomaticApp',
        discountId: 'gid://shopify/DiscountAutomaticNode/3?shopify-draft-proxy=synthetic',
        title: 'HAR-366 automatic app',
        status: 'ACTIVE',
        recurringCycleLimit: 0,
        appDiscountType: {
          appKey: 'app-key-366',
          functionId: 'discount-local',
          title: 'Local volume discount',
          description: 'Captured local Function metadata',
        },
      },
      userErrors: [],
    });

    const codeId = createResponse.body.data.discountCodeAppCreate.codeAppDiscount.discountId as string;
    const automaticId = createResponse.body.data.discountAutomaticAppCreate.automaticAppDiscount.discountId as string;
    const updateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation UpdateAppDiscounts($codeId: ID!, $codeInput: DiscountCodeAppInput!, $automaticId: ID!, $automaticInput: DiscountAutomaticAppInput!) {
          discountCodeAppUpdate(id: $codeId, codeAppDiscount: $codeInput) {
            codeAppDiscount { discountId title codes(first: 5) { nodes { code } } }
            userErrors { field message code extraInfo }
          }
          discountAutomaticAppUpdate(id: $automaticId, automaticAppDiscount: $automaticInput) {
            automaticAppDiscount { discountId title recurringCycleLimit }
            userErrors { field message code extraInfo }
          }
        }`,
        variables: {
          codeId,
          codeInput: {
            title: 'HAR-366 code app updated',
            code: 'HAR366UP',
            startsAt: '2024-01-01T00:00:00.000Z',
            functionHandle: 'discount-local',
          },
          automaticId,
          automaticInput: {
            title: 'HAR-366 automatic app updated',
            startsAt: '2024-01-01T00:00:00.000Z',
            functionHandle: 'discount-local',
            recurringCycleLimit: 2,
          },
        },
      });

    expect(updateResponse.body.data.discountCodeAppUpdate).toEqual({
      codeAppDiscount: {
        discountId: codeId,
        title: 'HAR-366 code app updated',
        codes: { nodes: [{ code: 'HAR366UP' }] },
      },
      userErrors: [],
    });
    expect(updateResponse.body.data.discountAutomaticAppUpdate).toEqual({
      automaticAppDiscount: {
        discountId: automaticId,
        title: 'HAR-366 automatic app updated',
        recurringCycleLimit: 2,
      },
      userErrors: [],
    });

    const readResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query ReadAppDiscounts($codeId: ID!, $automaticId: ID!) {
          codeDiscountNode(id: $codeId) {
            codeDiscount { ... on DiscountCodeApp { title appDiscountType { functionId } codes(first: 5) { nodes { code } } } }
          }
          automaticDiscountNode(id: $automaticId) {
            automaticDiscount { ... on DiscountAutomaticApp { title recurringCycleLimit appDiscountType { functionId } } }
          }
          discountNodesCount(query: "type:app") { count precision }
        }`,
        variables: { codeId, automaticId },
      });

    expect(readResponse.body.data).toEqual({
      codeDiscountNode: {
        codeDiscount: {
          title: 'HAR-366 code app updated',
          appDiscountType: { functionId: 'discount-local' },
          codes: { nodes: [{ code: 'HAR366UP' }] },
        },
      },
      automaticDiscountNode: {
        automaticDiscount: {
          title: 'HAR-366 automatic app updated',
          recurringCycleLimit: 2,
          appDiscountType: { functionId: 'discount-local' },
        },
      },
      discountNodesCount: { count: 2, precision: 'EXACT' },
    });

    const appLifecycleSelection = `#graphql
      automaticDiscountNode {
        id
        automaticDiscount {
          __typename
          ... on DiscountAutomaticApp {
            title
            status
            endsAt
            appDiscountType {
              functionId
            }
          }
        }
      }
      userErrors {
        field
        message
        code
        extraInfo
      }
    `;

    const deactivateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation DeactivateAutomaticApp($id: ID!) {
          discountAutomaticDeactivate(id: $id) {
            ${appLifecycleSelection}
          }
        }`,
        variables: { id: automaticId },
      });

    expect(deactivateResponse.body.data.discountAutomaticDeactivate).toMatchObject({
      automaticDiscountNode: {
        id: automaticId,
        automaticDiscount: {
          __typename: 'DiscountAutomaticApp',
          title: 'HAR-366 automatic app updated',
          status: 'EXPIRED',
          appDiscountType: { functionId: 'discount-local' },
        },
      },
      userErrors: [],
    });
    expect(
      deactivateResponse.body.data.discountAutomaticDeactivate.automaticDiscountNode.automaticDiscount.endsAt,
    ).toEqual(expect.any(String));

    const expiredAppCount = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query ExpiredAppCount {
          discountNodesCount(query: "type:app status:expired") { count precision }
        }`,
      });
    expect(expiredAppCount.body.data.discountNodesCount).toEqual({ count: 1, precision: 'EXACT' });

    const activateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation ActivateAutomaticApp($id: ID!) {
          discountAutomaticActivate(id: $id) {
            ${appLifecycleSelection}
          }
        }`,
        variables: { id: automaticId },
      });

    expect(activateResponse.body.data.discountAutomaticActivate).toEqual({
      automaticDiscountNode: {
        id: automaticId,
        automaticDiscount: {
          __typename: 'DiscountAutomaticApp',
          title: 'HAR-366 automatic app updated',
          status: 'ACTIVE',
          endsAt: null,
          appDiscountType: { functionId: 'discount-local' },
        },
      },
      userErrors: [],
    });

    const activeAppCount = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query ActiveAppCount {
          discountNodesCount(query: "type:app status:active") { count precision }
        }`,
      });
    expect(activeAppCount.body.data.discountNodesCount).toEqual({ count: 2, precision: 'EXACT' });

    const deleteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation DeleteAutomaticApp($id: ID!) {
          discountAutomaticDelete(id: $id) {
            deletedAutomaticDiscountId
            userErrors {
              field
              message
              code
              extraInfo
            }
          }
        }`,
        variables: { id: automaticId },
      });

    expect(deleteResponse.body.data.discountAutomaticDelete).toEqual({
      deletedAutomaticDiscountId: automaticId,
      userErrors: [],
    });

    const readAfterDelete = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query ReadAutomaticAppAfterDelete($id: ID!) {
          automaticDiscountNode(id: $id) { id }
          discountNodesCount(query: "type:app") { count precision }
        }`,
        variables: { id: automaticId },
      });
    expect(readAfterDelete.body.data).toEqual({
      automaticDiscountNode: null,
      discountNodesCount: { count: 1, precision: 'EXACT' },
    });

    const stateResponse = await request(app).get('/__meta/state');
    expect(Object.keys(stateResponse.body.stagedState.discounts)).toEqual([codeId]);
    expect(stateResponse.body.stagedState.deletedDiscountIds[automaticId]).toBe(true);
    const logResponse = await request(app).get('/__meta/log');
    expect(logResponse.body.entries.map((entry: { operationName: string }) => entry.operationName)).toEqual([
      'discountCodeAppCreate',
      'discountCodeAppUpdate',
      'discountAutomaticDeactivate',
      'discountAutomaticActivate',
      'discountAutomaticDelete',
    ]);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages code and automatic bulk jobs for safe selectors and keeps broad refusals local', async () => {
    store.upsertBaseDiscounts([
      buildDiscount(),
      buildDiscount({
        id: 'gid://shopify/DiscountCodeNode/366002',
        title: 'HAR-366 code fixture active',
        status: 'ACTIVE',
        codes: ['HAR366ACTIVE'],
        redeemCodes: [{ id: 'gid://shopify/DiscountRedeemCode/366002', code: 'HAR366ACTIVE', asyncUsageCount: 0 }],
      }),
      buildDiscount({
        id: 'gid://shopify/DiscountAutomaticNode/366003',
        typeName: 'DiscountAutomaticBasic',
        method: 'automatic',
        title: 'HAR-366 automatic fixture',
        status: 'ACTIVE',
        codes: [],
        redeemCodes: [],
      }),
    ]);
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('discount bulk staging must not hit upstream fetch');
    });
    const app = createApp(config).callback();

    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation BulkDiscounts($ids: [ID!], $deleteSearch: String) {
          activate: discountCodeBulkActivate(ids: $ids) {
            job { id done query }
            userErrors { field message code extraInfo }
          }
          deactivate: discountCodeBulkDeactivate(search: "status:active") {
            job { id done query }
            userErrors { field message code extraInfo }
          }
          deleteCode: discountCodeBulkDelete(search: $deleteSearch) {
            job { id done query }
            userErrors { field message code extraInfo }
          }
          deleteAutomatic: discountAutomaticBulkDelete(search: "status:active") {
            job { id done query }
            userErrors { field message code extraInfo }
          }
        }`,
        variables: {
          ids: ['gid://shopify/DiscountCodeNode/366001'],
          deleteSearch: 'title:"HAR-366 code fixture active"',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body.data).toEqual({
      activate: {
        job: { id: 'gid://shopify/Job/1?shopify-draft-proxy=synthetic', done: true, query: null },
        userErrors: [],
      },
      deactivate: {
        job: { id: 'gid://shopify/Job/2?shopify-draft-proxy=synthetic', done: true, query: 'status:active' },
        userErrors: [],
      },
      deleteCode: {
        job: {
          id: 'gid://shopify/Job/3?shopify-draft-proxy=synthetic',
          done: true,
          query: 'title:"HAR-366 code fixture active"',
        },
        userErrors: [],
      },
      deleteAutomatic: {
        job: { id: 'gid://shopify/Job/4?shopify-draft-proxy=synthetic', done: true, query: 'status:active' },
        userErrors: [],
      },
    });

    const readResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query ReadBulkEffects {
          active: discountNodesCount(query: "status:active") { count precision }
          expired: discountNodesCount(query: "status:expired") { count precision }
          deletedCode: codeDiscountNode(id: "gid://shopify/DiscountCodeNode/366002") { id }
          deletedAutomatic: automaticDiscountNode(id: "gid://shopify/DiscountAutomaticNode/366003") { id }
        }`,
      });

    expect(readResponse.body.data).toEqual({
      active: { count: 0, precision: 'EXACT' },
      expired: { count: 1, precision: 'EXACT' },
      deletedCode: null,
      deletedAutomatic: null,
    });

    const refusedResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation RefuseBulk {
          discountCodeBulkDelete(search: "") {
            job { id }
            userErrors { field message code extraInfo }
          }
        }`,
      });

    expect(refusedResponse.body.data.discountCodeBulkDelete).toEqual({
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

    const stateResponse = await request(app).get('/__meta/state');
    expect(Object.values(stateResponse.body.stagedState.discountBulkOperations)).toEqual([
      expect.objectContaining({
        operation: 'discountCodeBulkActivate',
        discountIds: ['gid://shopify/DiscountCodeNode/366001'],
      }),
      expect.objectContaining({
        operation: 'discountCodeBulkDeactivate',
        discountIds: ['gid://shopify/DiscountCodeNode/366001', 'gid://shopify/DiscountCodeNode/366002'],
      }),
      expect.objectContaining({
        operation: 'discountCodeBulkDelete',
        discountIds: ['gid://shopify/DiscountCodeNode/366002'],
      }),
      expect.objectContaining({
        operation: 'discountAutomaticBulkDelete',
        discountIds: ['gid://shopify/DiscountAutomaticNode/366003'],
      }),
    ]);
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});

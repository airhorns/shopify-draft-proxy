import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { resetSyntheticIdentity } from '../../src/state/synthetic-identity.js';
import { store } from '../../src/state/store.js';
import type { CustomerRecord, SegmentRecord } from '../../src/state/types.js';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'live-hybrid',
};

const discountFields = `#graphql
  id
  codeDiscount {
    __typename
    ... on DiscountCodeBasic {
      title
      status
      summary
      startsAt
      endsAt
      createdAt
      updatedAt
      asyncUsageCount
      discountClasses
      combinesWith {
        productDiscounts
        orderDiscounts
        shippingDiscounts
      }
      codes(first: 2) {
        nodes {
          id
          code
          asyncUsageCount
        }
      }
      context {
        __typename
        ... on DiscountBuyerSelectionAll {
          all
        }
        ... on DiscountCustomers {
          customers {
            __typename
            id
            displayName
          }
        }
        ... on DiscountCustomerSegments {
          segments {
            __typename
            id
            name
          }
        }
      }
      customerGets {
        value {
          __typename
          ... on DiscountPercentage {
            percentage
          }
          ... on DiscountAmount {
            amount {
              amount
              currencyCode
            }
            appliesOnEachItem
          }
        }
        items {
          __typename
          ... on AllDiscountItems {
            allItems
          }
        }
        appliesOnOneTimePurchase
        appliesOnSubscription
      }
      minimumRequirement {
        __typename
        ... on DiscountMinimumSubtotal {
          greaterThanOrEqualToSubtotal {
            amount
            currencyCode
          }
        }
        ... on DiscountMinimumQuantity {
          greaterThanOrEqualToQuantity
        }
      }
    }
  }
`;

function basicPercentageInput(code: string): Record<string, unknown> {
  return {
    title: 'HAR-193 ten percent',
    code,
    startsAt: '2023-12-31T00:00:00Z',
    combinesWith: {
      productDiscounts: false,
      orderDiscounts: true,
      shippingDiscounts: false,
    },
    context: {
      all: 'ALL',
    },
    minimumRequirement: {
      subtotal: {
        greaterThanOrEqualToSubtotal: '1.00',
      },
    },
    customerGets: {
      value: {
        percentage: 0.1,
      },
      items: {
        all: true,
      },
    },
  };
}

function makeContextCustomer(): CustomerRecord {
  return {
    id: 'gid://shopify/Customer/390001',
    firstName: 'Har',
    lastName: 'Discount',
    displayName: 'Har Discount',
    email: 'har-discount@example.com',
    legacyResourceId: '390001',
    locale: 'en',
    note: null,
    canDelete: true,
    verifiedEmail: true,
    taxExempt: false,
    taxExemptions: [],
    state: 'ENABLED',
    tags: [],
    numberOfOrders: '0',
    amountSpent: { amount: '0.00', currencyCode: 'USD' },
    defaultEmailAddress: null,
    defaultPhoneNumber: null,
    defaultAddress: null,
    createdAt: '2024-01-01T00:00:00Z',
    updatedAt: '2024-01-01T00:00:00Z',
  };
}

function makeContextSegment(): SegmentRecord {
  return {
    id: 'gid://shopify/Segment/390002',
    name: 'HAR-390 Discount Segment',
    query: "email_subscription_status = 'SUBSCRIBED'",
    creationDate: '2024-01-01T00:00:00Z',
    lastEditDate: '2024-01-01T00:00:00Z',
  };
}

describe('discount code-basic lifecycle staging', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('stages create, update, activation, deactivation, deletion, downstream reads, meta state, and commit replay locally', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('discount code-basic lifecycle should not hit upstream during runtime staging or reads');
    });
    const app = createApp(config).callback();

    const createResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation CreateCodeBasic($input: DiscountCodeBasicInput!) {
          discountCodeBasicCreate(basicCodeDiscount: $input) {
            codeDiscountNode {
              ${discountFields}
            }
            userErrors { field message code extraInfo }
          }
        }`,
        variables: {
          input: basicPercentageInput('HAR193TEN'),
        },
      });

    expect(createResponse.status).toBe(200);
    expect(createResponse.body.data.discountCodeBasicCreate.userErrors).toEqual([]);
    const createdNode = createResponse.body.data.discountCodeBasicCreate.codeDiscountNode;
    const stagedDiscountId = createdNode.id;
    expect(stagedDiscountId).toMatch(/^gid:\/\/shopify\/DiscountCodeNode\/\d+\?shopify-draft-proxy=synthetic$/u);
    expect(createdNode.codeDiscount).toMatchObject({
      __typename: 'DiscountCodeBasic',
      title: 'HAR-193 ten percent',
      status: 'ACTIVE',
      summary: '10% off entire order - Minimum purchase of $1.00',
      startsAt: '2023-12-31T00:00:00Z',
      endsAt: null,
      asyncUsageCount: 0,
      discountClasses: ['ORDER'],
      combinesWith: {
        productDiscounts: false,
        orderDiscounts: true,
        shippingDiscounts: false,
      },
      codes: {
        nodes: [
          {
            code: 'HAR193TEN',
            asyncUsageCount: 0,
          },
        ],
      },
      context: {
        __typename: 'DiscountBuyerSelectionAll',
        all: 'ALL',
      },
      customerGets: {
        value: {
          __typename: 'DiscountPercentage',
          percentage: 0.1,
        },
        items: {
          __typename: 'AllDiscountItems',
          allItems: true,
        },
        appliesOnOneTimePurchase: true,
        appliesOnSubscription: false,
      },
      minimumRequirement: {
        __typename: 'DiscountMinimumSubtotal',
        greaterThanOrEqualToSubtotal: {
          amount: '1.00',
          currencyCode: 'USD',
        },
      },
    });

    const createdReadResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query CreatedDiscount($id: ID!, $code: String!) {
          discountNode(id: $id) {
            id
            discount {
              __typename
              ... on DiscountCodeBasic {
                title
                status
              }
            }
          }
          codeDiscountNodeByCode(code: $code) {
            id
          }
          discountNodes(first: 10, query: "status:active") {
            nodes { id }
          }
          discountNodesCount(query: "status:active") {
            count
            precision
          }
        }`,
        variables: {
          id: stagedDiscountId,
          code: 'HAR193TEN',
        },
      });

    expect(createdReadResponse.body.data).toEqual({
      discountNode: {
        id: stagedDiscountId,
        discount: {
          __typename: 'DiscountCodeBasic',
          title: 'HAR-193 ten percent',
          status: 'ACTIVE',
        },
      },
      codeDiscountNodeByCode: {
        id: stagedDiscountId,
      },
      discountNodes: {
        nodes: [{ id: stagedDiscountId }],
      },
      discountNodesCount: {
        count: 1,
        precision: 'EXACT',
      },
    });

    const updateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation UpdateCodeBasic($id: ID!, $input: DiscountCodeBasicInput!) {
          discountCodeBasicUpdate(id: $id, basicCodeDiscount: $input) {
            codeDiscountNode {
              ${discountFields}
            }
            userErrors { field message code extraInfo }
          }
        }`,
        variables: {
          id: stagedDiscountId,
          input: {
            ...basicPercentageInput('HAR193FIVE'),
            title: 'HAR-193 five dollars',
            customerGets: {
              value: {
                discountAmount: {
                  amount: '5.00',
                  appliesOnEachItem: false,
                },
              },
              items: {
                all: true,
              },
            },
            minimumRequirement: {
              subtotal: {
                greaterThanOrEqualToSubtotal: '2.00',
              },
            },
          },
        },
      });

    expect(updateResponse.status).toBe(200);
    expect(updateResponse.body.data.discountCodeBasicUpdate).toMatchObject({
      userErrors: [],
      codeDiscountNode: {
        id: stagedDiscountId,
        codeDiscount: {
          title: 'HAR-193 five dollars',
          status: 'ACTIVE',
          summary: '5.00 USD off entire order - Minimum purchase of $2.00',
          codes: {
            nodes: [
              {
                code: 'HAR193FIVE',
              },
            ],
          },
          customerGets: {
            value: {
              __typename: 'DiscountAmount',
              amount: {
                amount: '5.00',
                currencyCode: 'USD',
              },
              appliesOnEachItem: false,
            },
          },
          minimumRequirement: {
            __typename: 'DiscountMinimumSubtotal',
            greaterThanOrEqualToSubtotal: {
              amount: '2.00',
              currencyCode: 'USD',
            },
          },
        },
      },
    });

    const deactivatedResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation DeactivateCodeBasic($id: ID!) {
          discountCodeDeactivate(id: $id) {
            codeDiscountNode {
              id
              codeDiscount {
                __typename
                ... on DiscountCodeBasic {
                  status
                }
              }
            }
            userErrors { field message code extraInfo }
          }
        }`,
        variables: { id: stagedDiscountId },
      });

    expect(deactivatedResponse.body.data.discountCodeDeactivate).toEqual({
      codeDiscountNode: {
        id: stagedDiscountId,
        codeDiscount: {
          __typename: 'DiscountCodeBasic',
          status: 'EXPIRED',
        },
      },
      userErrors: [],
    });

    const activatedResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation ActivateCodeBasic($id: ID!) {
          discountCodeActivate(id: $id) {
            codeDiscountNode {
              id
              codeDiscount {
                __typename
                ... on DiscountCodeBasic {
                  status
                }
              }
            }
            userErrors { field message code extraInfo }
          }
        }`,
        variables: { id: stagedDiscountId },
      });

    expect(activatedResponse.body.data.discountCodeActivate).toEqual({
      codeDiscountNode: {
        id: stagedDiscountId,
        codeDiscount: {
          __typename: 'DiscountCodeBasic',
          status: 'ACTIVE',
        },
      },
      userErrors: [],
    });

    const deleteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation DeleteCodeBasic($id: ID!) {
          discountCodeDelete(id: $id) {
            deletedCodeDiscountId
            userErrors { field message code extraInfo }
          }
        }`,
        variables: { id: stagedDiscountId },
      });

    expect(deleteResponse.body.data.discountCodeDelete).toEqual({
      deletedCodeDiscountId: stagedDiscountId,
      userErrors: [],
    });

    const deletedReadResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query DeletedDiscount($id: ID!, $code: String!) {
          discountNode(id: $id) { id }
          codeDiscountNode(id: $id) { id }
          codeDiscountNodeByCode(code: $code) { id }
          discountNodes(first: 10) { nodes { id } }
          discountNodesCount { count precision }
        }`,
        variables: {
          id: stagedDiscountId,
          code: 'HAR193FIVE',
        },
      });

    expect(deletedReadResponse.body.data).toEqual({
      discountNode: null,
      codeDiscountNode: null,
      codeDiscountNodeByCode: null,
      discountNodes: {
        nodes: [],
      },
      discountNodesCount: {
        count: 0,
        precision: 'EXACT',
      },
    });

    const stateResponse = await request(app).get('/__meta/state');
    expect(stateResponse.body.stagedState.deletedDiscountIds).toEqual({
      [stagedDiscountId]: true,
    });
    expect(stateResponse.body.stagedState.discounts).toEqual({});

    const logResponse = await request(app).get('/__meta/log');
    expect(logResponse.body.entries.map((entry: { operationName: string }) => entry.operationName)).toEqual([
      'discountCodeBasicCreate',
      'discountCodeBasicUpdate',
      'discountCodeDeactivate',
      'discountCodeActivate',
      'discountCodeDelete',
    ]);
    expect(logResponse.body.entries.every((entry: { status: string }) => entry.status === 'staged')).toBe(true);
    expect(logResponse.body.entries[0].stagedResourceIds).toContain(stagedDiscountId);
    expect(fetchSpy).not.toHaveBeenCalled();

    const replayBodies: Record<string, unknown>[] = [];
    fetchSpy.mockImplementation(async (_input: Parameters<typeof fetch>[0], init?: Parameters<typeof fetch>[1]) => {
      const body = JSON.parse(String(init?.body ?? '{}')) as Record<string, unknown>;
      replayBodies.push(body);
      const query = String(body['query'] ?? '');
      const variables = (body['variables'] ?? {}) as Record<string, unknown>;

      if (query.includes('discountCodeBasicCreate')) {
        return new Response(
          JSON.stringify({
            data: {
              discountCodeBasicCreate: {
                codeDiscountNode: {
                  id: 'gid://shopify/DiscountCodeNode/9001',
                },
                userErrors: [],
              },
            },
          }),
          { status: 200, headers: { 'content-type': 'application/json' } },
        );
      }

      const id = variables['id'];
      const rootName =
        query.match(/discountCode(?:BasicUpdate|Deactivate|Activate|Delete)/u)?.[0] ?? 'discountCodeBasicUpdate';
      return new Response(
        JSON.stringify({
          data: {
            [rootName]:
              rootName === 'discountCodeDelete'
                ? { deletedCodeDiscountId: id, userErrors: [] }
                : {
                    codeDiscountNode: { id },
                    userErrors: [],
                  },
          },
        }),
        { status: 200, headers: { 'content-type': 'application/json' } },
      );
    });

    const commitResponse = await request(app).post('/__meta/commit').set('x-shopify-access-token', 'shpat_test');
    expect(commitResponse.body.ok).toBe(true);
    expect(commitResponse.body.attempts).toHaveLength(5);
    expect(replayBodies.map((body) => String(body['query']).match(/discountCode[A-Za-z]+/u)?.[0])).toEqual([
      'discountCodeBasicCreate',
      'discountCodeBasicUpdate',
      'discountCodeDeactivate',
      'discountCodeActivate',
      'discountCodeDelete',
    ]);
    const replayVariables = replayBodies.map((body) => body['variables'] as Record<string, unknown>);
    expect(replayVariables[1]?.['id']).toBe('gid://shopify/DiscountCodeNode/9001');
    expect(replayVariables[2]?.['id']).toBe('gid://shopify/DiscountCodeNode/9001');
    expect(replayVariables[3]?.['id']).toBe('gid://shopify/DiscountCodeNode/9001');
    expect(replayVariables[4]?.['id']).toBe('gid://shopify/DiscountCodeNode/9001');
  });

  it('stages customer and segment buyer context links with downstream detail reads', async () => {
    const customer = makeContextCustomer();
    const segment = makeContextSegment();
    store.upsertBaseCustomers([customer]);
    store.upsertBaseSegments([segment]);
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('discount customer/segment context should not hit upstream fetch');
    });
    const app = createApp(config).callback();

    const create = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation CreateCustomerContextCodeBasic($input: DiscountCodeBasicInput!) {
          discountCodeBasicCreate(basicCodeDiscount: $input) {
            codeDiscountNode {
              ${discountFields}
            }
            userErrors { field message code extraInfo }
          }
        }`,
        variables: {
          input: {
            ...basicPercentageInput('HAR390CUSTOMER'),
            title: 'HAR-390 customer context',
            context: {
              customers: {
                add: [customer.id],
              },
            },
          },
        },
      });

    expect(create.status).toBe(200);
    expect(create.body.data.discountCodeBasicCreate.userErrors).toEqual([]);
    const discountId = create.body.data.discountCodeBasicCreate.codeDiscountNode.id as string;
    expect(create.body.data.discountCodeBasicCreate.codeDiscountNode.codeDiscount.context).toEqual({
      __typename: 'DiscountCustomers',
      customers: [
        {
          __typename: 'Customer',
          id: customer.id,
          displayName: 'Har Discount',
        },
      ],
    });

    const update = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation UpdateSegmentContextCodeBasic($id: ID!, $input: DiscountCodeBasicInput!) {
          discountCodeBasicUpdate(id: $id, basicCodeDiscount: $input) {
            codeDiscountNode {
              ${discountFields}
            }
            userErrors { field message code extraInfo }
          }
        }`,
        variables: {
          id: discountId,
          input: {
            ...basicPercentageInput('HAR390SEGMENT'),
            title: 'HAR-390 segment context',
            context: {
              customerSegments: {
                add: [segment.id],
              },
            },
          },
        },
      });

    expect(update.status).toBe(200);
    expect(update.body.data.discountCodeBasicUpdate.userErrors).toEqual([]);
    expect(update.body.data.discountCodeBasicUpdate.codeDiscountNode.codeDiscount.context).toEqual({
      __typename: 'DiscountCustomerSegments',
      segments: [
        {
          __typename: 'Segment',
          id: segment.id,
          name: 'HAR-390 Discount Segment',
        },
      ],
    });

    const read = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query ReadSegmentContextCodeBasic($id: ID!) {
          discountNode(id: $id) {
            discount {
              __typename
              ... on DiscountCodeBasic {
                context {
                  __typename
                  ... on DiscountCustomerSegments {
                    segments {
                      id
                      name
                    }
                  }
                }
              }
            }
          }
        }`,
        variables: {
          id: discountId,
        },
      });

    expect(read.body.data.discountNode.discount.context).toEqual({
      __typename: 'DiscountCustomerSegments',
      segments: [
        {
          id: segment.id,
          name: 'HAR-390 Discount Segment',
        },
      ],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});

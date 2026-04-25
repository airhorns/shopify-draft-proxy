import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { resetSyntheticIdentity } from '../../src/state/synthetic-identity.js';
import { store } from '../../src/state/store.js';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

function percentageInput(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    title: 'HAR-194 automatic percentage',
    startsAt: '2023-01-01T00:00:00Z',
    endsAt: null,
    combinesWith: {
      productDiscounts: false,
      orderDiscounts: true,
      shippingDiscounts: false,
    },
    context: {
      all: 'ALL',
    },
    minimumRequirement: {
      quantity: {
        greaterThanOrEqualToQuantity: '2',
      },
    },
    customerGets: {
      value: {
        percentage: 0.15,
      },
      items: {
        all: true,
      },
    },
    ...overrides,
  };
}

const userErrorsSelection = `#graphql
  userErrors {
    field
    message
    code
    extraInfo
  }
`;

const automaticDetailSelection = `#graphql
  id
  automaticDiscount {
    __typename
    ... on DiscountAutomaticBasic {
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
      context {
        __typename
        ... on DiscountBuyerSelectionAll {
          all
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
        ... on DiscountMinimumQuantity {
          greaterThanOrEqualToQuantity
        }
        ... on DiscountMinimumSubtotal {
          greaterThanOrEqualToSubtotal {
            amount
            currencyCode
          }
        }
      }
    }
  }
`;

describe('automatic basic discount lifecycle staging', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('stages create-update-deactivate-activate-delete locally with downstream reads and meta visibility', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('automatic discount lifecycle should not hit upstream fetch');
    });
    const app = createApp(config).callback();

    const createMutation = `#graphql
      mutation CreateAutomaticBasic($input: DiscountAutomaticBasicInput!) {
        discountAutomaticBasicCreate(automaticBasicDiscount: $input) {
          automaticDiscountNode {
            ${automaticDetailSelection}
          }
          ${userErrorsSelection}
        }
      }
    `;
    const create = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: createMutation,
        variables: {
          input: percentageInput(),
        },
      });

    expect(create.status).toBe(200);
    expect(create.body.data.discountAutomaticBasicCreate.userErrors).toEqual([]);
    const discountId = create.body.data.discountAutomaticBasicCreate.automaticDiscountNode.id as string;
    expect(discountId).toMatch(/^gid:\/\/shopify\/DiscountAutomaticNode\/[0-9]+\?shopify-draft-proxy=synthetic$/u);
    expect(create.body.data.discountAutomaticBasicCreate.automaticDiscountNode.automaticDiscount).toMatchObject({
      __typename: 'DiscountAutomaticBasic',
      title: 'HAR-194 automatic percentage',
      status: 'ACTIVE',
      startsAt: '2023-01-01T00:00:00Z',
      endsAt: null,
      customerGets: {
        value: {
          __typename: 'DiscountPercentage',
          percentage: 0.15,
        },
      },
      minimumRequirement: {
        __typename: 'DiscountMinimumQuantity',
        greaterThanOrEqualToQuantity: '2',
      },
    });

    const readAfterCreate = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          query AutomaticReads($id: ID!) {
            automaticDiscountNode(id: $id) {
              ${automaticDetailSelection}
            }
            automaticDiscountNodes(first: 5, query: "status:active") {
              nodes {
                id
                automaticDiscount {
                  ... on DiscountAutomaticBasic {
                    title
                    status
                  }
                }
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
            discountNodes(first: 5, query: "method:automatic") {
              nodes {
                id
                discount {
                  __typename
                  ... on DiscountAutomaticBasic {
                    title
                    status
                  }
                }
              }
            }
            discountNodesCount(query: "method:automatic") {
              count
              precision
            }
          }
        `,
        variables: {
          id: discountId,
        },
      });

    expect(readAfterCreate.status).toBe(200);
    expect(readAfterCreate.body.data.automaticDiscountNode.id).toBe(discountId);
    expect(readAfterCreate.body.data.automaticDiscountNodes.nodes).toEqual([
      {
        id: discountId,
        automaticDiscount: {
          title: 'HAR-194 automatic percentage',
          status: 'ACTIVE',
        },
      },
    ]);
    expect(readAfterCreate.body.data.automaticDiscountNodes.edges).toEqual([
      {
        cursor: `cursor:${discountId}`,
        node: {
          id: discountId,
        },
      },
    ]);
    expect(readAfterCreate.body.data.automaticDiscountNodes.pageInfo).toEqual({
      hasNextPage: false,
      hasPreviousPage: false,
      startCursor: `cursor:${discountId}`,
      endCursor: `cursor:${discountId}`,
    });
    expect(readAfterCreate.body.data.discountNodes.nodes).toEqual([
      {
        id: discountId,
        discount: {
          __typename: 'DiscountAutomaticBasic',
          title: 'HAR-194 automatic percentage',
          status: 'ACTIVE',
        },
      },
    ]);
    expect(readAfterCreate.body.data.discountNodesCount).toEqual({
      count: 1,
      precision: 'EXACT',
    });

    const updateMutation = `#graphql
      mutation UpdateAutomaticBasic($id: ID!, $input: DiscountAutomaticBasicInput!) {
        discountAutomaticBasicUpdate(id: $id, automaticBasicDiscount: $input) {
          automaticDiscountNode {
            ${automaticDetailSelection}
          }
          ${userErrorsSelection}
        }
      }
    `;
    const update = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: updateMutation,
        variables: {
          id: discountId,
          input: percentageInput({
            title: 'HAR-194 automatic fixed amount',
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
                greaterThanOrEqualToSubtotal: '10.00',
              },
            },
          }),
        },
      });

    expect(update.status).toBe(200);
    expect(update.body.data.discountAutomaticBasicUpdate.userErrors).toEqual([]);
    expect(update.body.data.discountAutomaticBasicUpdate.automaticDiscountNode.automaticDiscount).toMatchObject({
      title: 'HAR-194 automatic fixed amount',
      status: 'ACTIVE',
      customerGets: {
        value: {
          __typename: 'DiscountAmount',
          amount: {
            amount: '5.0',
            currencyCode: 'CAD',
          },
          appliesOnEachItem: false,
        },
      },
      minimumRequirement: {
        __typename: 'DiscountMinimumSubtotal',
        greaterThanOrEqualToSubtotal: {
          amount: '10.0',
          currencyCode: 'CAD',
        },
      },
    });

    const metaStateAfterUpdate = await request(app).get('/__meta/state');
    expect(metaStateAfterUpdate.status).toBe(200);
    expect(metaStateAfterUpdate.body.stagedState.discounts[discountId]).toMatchObject({
      id: discountId,
      method: 'automatic',
      typeName: 'DiscountAutomaticBasic',
      title: 'HAR-194 automatic fixed amount',
      customerGets: {
        value: {
          typeName: 'DiscountAmount',
          amount: {
            amount: '5.0',
            currencyCode: 'CAD',
          },
        },
      },
    });

    const deactivate = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation DeactivateAutomatic($id: ID!) {
            discountAutomaticDeactivate(id: $id) {
              automaticDiscountNode {
                ${automaticDetailSelection}
              }
              ${userErrorsSelection}
            }
          }
        `,
        variables: {
          id: discountId,
        },
      });

    expect(deactivate.status).toBe(200);
    expect(deactivate.body.data.discountAutomaticDeactivate.userErrors).toEqual([]);
    expect(deactivate.body.data.discountAutomaticDeactivate.automaticDiscountNode.automaticDiscount.status).toBe(
      'EXPIRED',
    );
    expect(deactivate.body.data.discountAutomaticDeactivate.automaticDiscountNode.automaticDiscount.endsAt).toEqual(
      expect.any(String),
    );

    const activeCountAfterDeactivate = await request(app).post('/admin/api/2026-04/graphql.json').send({
      query: `query { discountNodesCount(query: "status:active method:automatic") { count precision } }`,
    });
    expect(activeCountAfterDeactivate.body.data.discountNodesCount).toEqual({
      count: 0,
      precision: 'EXACT',
    });

    const activate = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation ActivateAutomatic($id: ID!) {
            discountAutomaticActivate(id: $id) {
              automaticDiscountNode {
                ${automaticDetailSelection}
              }
              ${userErrorsSelection}
            }
          }
        `,
        variables: {
          id: discountId,
        },
      });

    expect(activate.status).toBe(200);
    expect(activate.body.data.discountAutomaticActivate.userErrors).toEqual([]);
    expect(activate.body.data.discountAutomaticActivate.automaticDiscountNode.automaticDiscount).toMatchObject({
      status: 'ACTIVE',
      endsAt: null,
    });

    const deleteMutation = `#graphql
      mutation DeleteAutomatic($id: ID!) {
        discountAutomaticDelete(id: $id) {
          deletedAutomaticDiscountId
          ${userErrorsSelection}
        }
      }
    `;
    const deleted = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: deleteMutation,
        variables: {
          id: discountId,
        },
      });

    expect(deleted.status).toBe(200);
    expect(deleted.body.data.discountAutomaticDelete).toEqual({
      deletedAutomaticDiscountId: discountId,
      userErrors: [],
    });

    const readAfterDelete = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          query($id: ID!) {
            automaticDiscountNode(id: $id) {
              id
            }
            discountNodesCount(query: "method:automatic") {
              count
              precision
            }
          }
        `,
        variables: {
          id: discountId,
        },
      });

    expect(readAfterDelete.body.data).toEqual({
      automaticDiscountNode: null,
      discountNodesCount: {
        count: 0,
        precision: 'EXACT',
      },
    });

    const metaLog = await request(app).get('/__meta/log');
    expect(metaLog.status).toBe(200);
    expect(metaLog.body.entries).toHaveLength(5);
    expect(metaLog.body.entries.map((entry: { status: string }) => entry.status)).toEqual([
      'staged',
      'staged',
      'staged',
      'staged',
      'staged',
    ]);
    expect(
      metaLog.body.entries.map((entry: { interpreted: { rootFields: string[] } }) => entry.interpreted.rootFields[0]),
    ).toEqual([
      'discountAutomaticBasicCreate',
      'discountAutomaticBasicUpdate',
      'discountAutomaticDeactivate',
      'discountAutomaticActivate',
      'discountAutomaticDelete',
    ]);
    expect(metaLog.body.entries.map((entry: { query: string }) => entry.query)).toEqual([
      createMutation,
      updateMutation,
      expect.stringContaining('discountAutomaticDeactivate'),
      expect.stringContaining('discountAutomaticActivate'),
      deleteMutation,
    ]);

    const metaStateAfterDelete = await request(app).get('/__meta/state');
    expect(metaStateAfterDelete.body.stagedState.discounts[discountId]).toBeUndefined();
    expect(metaStateAfterDelete.body.stagedState.deletedDiscountIds[discountId]).toBe(true);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('derives scheduled and expired statuses and preserves empty snapshot read behavior', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('automatic discount snapshot flow should not hit upstream fetch');
    });
    const app = createApp(config).callback();

    const emptyRead = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          query($id: ID!) {
            automaticDiscountNode(id: $id) {
              id
            }
            automaticDiscountNodes(first: 2, query: "title:__missing_automatic_discount__") {
              nodes {
                id
              }
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
          id: 'gid://shopify/DiscountAutomaticNode/404',
        },
      });

    expect(emptyRead.status).toBe(200);
    expect(emptyRead.body.data).toEqual({
      automaticDiscountNode: null,
      automaticDiscountNodes: {
        nodes: [],
        edges: [],
        pageInfo: {
          hasNextPage: false,
          hasPreviousPage: false,
          startCursor: null,
          endCursor: null,
        },
      },
    });

    const createMutation = `#graphql
      mutation CreateAutomaticBasic($scheduled: DiscountAutomaticBasicInput!, $expired: DiscountAutomaticBasicInput!) {
        scheduled: discountAutomaticBasicCreate(automaticBasicDiscount: $scheduled) {
          automaticDiscountNode {
            id
            automaticDiscount {
              ... on DiscountAutomaticBasic {
                title
                status
              }
            }
          }
          ${userErrorsSelection}
        }
        expired: discountAutomaticBasicCreate(automaticBasicDiscount: $expired) {
          automaticDiscountNode {
            id
            automaticDiscount {
              ... on DiscountAutomaticBasic {
                title
                status
              }
            }
          }
          ${userErrorsSelection}
        }
      }
    `;
    const create = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: createMutation,
        variables: {
          scheduled: percentageInput({
            title: 'HAR-194 scheduled automatic',
            startsAt: '2099-01-01T00:00:00Z',
            endsAt: null,
          }),
          expired: percentageInput({
            title: 'HAR-194 expired automatic',
            startsAt: '2023-01-01T00:00:00Z',
            endsAt: '2023-02-01T00:00:00Z',
          }),
        },
      });

    expect(create.body.data.scheduled.automaticDiscountNode.automaticDiscount.status).toBe('SCHEDULED');
    expect(create.body.data.expired.automaticDiscountNode.automaticDiscount.status).toBe('EXPIRED');

    const filtered = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          query {
            scheduled: automaticDiscountNodes(first: 5, query: "status:scheduled") {
              nodes {
                automaticDiscount {
                  ... on DiscountAutomaticBasic {
                    title
                    status
                  }
                }
              }
            }
            expired: automaticDiscountNodes(first: 5, query: "status:expired") {
              nodes {
                automaticDiscount {
                  ... on DiscountAutomaticBasic {
                    title
                    status
                  }
                }
              }
            }
          }
        `,
      });

    expect(filtered.body.data.scheduled.nodes).toEqual([
      {
        automaticDiscount: {
          title: 'HAR-194 scheduled automatic',
          status: 'SCHEDULED',
        },
      },
    ]);
    expect(filtered.body.data.expired.nodes).toEqual([
      {
        automaticDiscount: {
          title: 'HAR-194 expired automatic',
          status: 'EXPIRED',
        },
      },
    ]);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('returns local userErrors for unknown automatic lifecycle IDs without upstream access', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('automatic discount validation should not hit upstream fetch');
    });
    const app = createApp(config).callback();
    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation MissingAutomatic($id: ID!, $input: DiscountAutomaticBasicInput!) {
            updateMissing: discountAutomaticBasicUpdate(id: $id, automaticBasicDiscount: $input) {
              automaticDiscountNode { id }
              ${userErrorsSelection}
            }
            activateMissing: discountAutomaticActivate(id: $id) {
              automaticDiscountNode { id }
              ${userErrorsSelection}
            }
            deactivateMissing: discountAutomaticDeactivate(id: $id) {
              automaticDiscountNode { id }
              ${userErrorsSelection}
            }
            deleteMissing: discountAutomaticDelete(id: $id) {
              deletedAutomaticDiscountId
              ${userErrorsSelection}
            }
          }
        `,
        variables: {
          id: 'gid://shopify/DiscountAutomaticNode/0',
          input: percentageInput(),
        },
      });

    const expectedUserErrors = [
      {
        field: ['id'],
        message: 'Discount does not exist',
        code: null,
        extraInfo: null,
      },
    ];
    expect(response.status).toBe(200);
    expect(response.body.data.updateMissing).toEqual({
      automaticDiscountNode: null,
      userErrors: expectedUserErrors,
    });
    expect(response.body.data.activateMissing).toEqual({
      automaticDiscountNode: null,
      userErrors: expectedUserErrors,
    });
    expect(response.body.data.deactivateMissing).toEqual({
      automaticDiscountNode: null,
      userErrors: expectedUserErrors,
    });
    expect(response.body.data.deleteMissing).toEqual({
      deletedAutomaticDiscountId: null,
      userErrors: expectedUserErrors,
    });
    expect(store.getLog()).toEqual([]);
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});

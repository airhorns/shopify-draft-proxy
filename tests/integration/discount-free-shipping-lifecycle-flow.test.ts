import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../support/runtime.js';
import type { AppConfig } from '../../src/config.js';
import { resetSyntheticIdentity } from '../support/runtime.js';
import { store } from '../support/runtime.js';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

const userErrorsSelection = `#graphql
  userErrors {
    field
    message
    code
    extraInfo
  }
`;

const codeFreeShippingSelection = `#graphql
  id
  codeDiscount {
    __typename
    ... on DiscountCodeFreeShipping {
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
        pageInfo {
          hasNextPage
          hasPreviousPage
          startCursor
          endCursor
        }
      }
      context {
        __typename
        ... on DiscountBuyerSelectionAll {
          all
        }
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
      destinationSelection {
        __typename
        ... on DiscountCountryAll {
          allCountries
        }
        ... on DiscountCountries {
          countries
          includeRestOfWorld
        }
      }
      maximumShippingPrice {
        amount
        currencyCode
      }
      appliesOncePerCustomer
      appliesOnOneTimePurchase
      appliesOnSubscription
      recurringCycleLimit
      usageLimit
    }
  }
`;

const automaticFreeShippingSelection = `#graphql
  id
  automaticDiscount {
    __typename
    ... on DiscountAutomaticFreeShipping {
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
      minimumRequirement {
        __typename
        ... on DiscountMinimumSubtotal {
          greaterThanOrEqualToSubtotal {
            amount
            currencyCode
          }
        }
      }
      destinationSelection {
        __typename
        ... on DiscountCountryAll {
          allCountries
        }
        ... on DiscountCountries {
          countries
          includeRestOfWorld
        }
      }
      maximumShippingPrice {
        amount
        currencyCode
      }
      appliesOnOneTimePurchase
      appliesOnSubscription
      recurringCycleLimit
    }
  }
`;

function codeFreeShippingInput(code = 'HAR196FREE'): Record<string, unknown> {
  return {
    title: 'HAR-196 code free shipping',
    code,
    startsAt: '2023-04-25T00:00:00Z',
    combinesWith: {
      productDiscounts: true,
      orderDiscounts: false,
      shippingDiscounts: false,
    },
    context: {
      all: 'ALL',
    },
    minimumRequirement: {
      subtotal: {
        greaterThanOrEqualToSubtotal: '10.00',
      },
    },
    destination: {
      all: true,
    },
    maximumShippingPrice: '25.00',
    appliesOncePerCustomer: true,
    appliesOnOneTimePurchase: true,
    appliesOnSubscription: false,
    recurringCycleLimit: 1,
    usageLimit: 5,
  };
}

function automaticFreeShippingInput(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    title: 'HAR-196 automatic free shipping',
    startsAt: '2023-04-25T00:00:00Z',
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
      subtotal: {
        greaterThanOrEqualToSubtotal: '15.00',
      },
    },
    destination: {
      all: true,
    },
    maximumShippingPrice: '20.00',
    appliesOnOneTimePurchase: true,
    appliesOnSubscription: false,
    recurringCycleLimit: 1,
    ...overrides,
  };
}

describe('free-shipping discount lifecycle staging', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('stages code free-shipping create-update-status-delete locally with downstream reads', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('code free-shipping discount staging should not hit upstream fetch');
    });
    const app = createApp(config).callback();

    const create = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation CreateCodeFreeShipping($input: DiscountCodeFreeShippingInput!) {
            discountCodeFreeShippingCreate(freeShippingCodeDiscount: $input) {
              codeDiscountNode {
                ${codeFreeShippingSelection}
              }
              ${userErrorsSelection}
            }
          }
        `,
        variables: {
          input: codeFreeShippingInput(),
        },
      });

    expect(create.status).toBe(200);
    expect(create.body.data.discountCodeFreeShippingCreate.userErrors).toEqual([]);
    const discountId = create.body.data.discountCodeFreeShippingCreate.codeDiscountNode.id as string;
    expect(discountId).toMatch(/^gid:\/\/shopify\/DiscountCodeNode\/[0-9]+\?shopify-draft-proxy=synthetic$/u);
    expect(create.body.data.discountCodeFreeShippingCreate.codeDiscountNode.codeDiscount).toMatchObject({
      __typename: 'DiscountCodeFreeShipping',
      title: 'HAR-196 code free shipping',
      status: 'ACTIVE',
      summary:
        'Free shipping on one-time purchase products • Minimum purchase of $10.00 • For all countries • Applies to shipping rates under $25.00 • One use per customer',
      discountClasses: ['SHIPPING'],
      combinesWith: {
        productDiscounts: true,
        orderDiscounts: false,
        shippingDiscounts: false,
      },
      codes: {
        nodes: [
          {
            code: 'HAR196FREE',
            asyncUsageCount: 0,
          },
        ],
      },
      minimumRequirement: {
        __typename: 'DiscountMinimumSubtotal',
        greaterThanOrEqualToSubtotal: {
          amount: '10.0',
          currencyCode: 'CAD',
        },
      },
      destinationSelection: {
        __typename: 'DiscountCountryAll',
        allCountries: true,
      },
      maximumShippingPrice: {
        amount: '25.0',
        currencyCode: 'CAD',
      },
      appliesOncePerCustomer: true,
      appliesOnOneTimePurchase: true,
      appliesOnSubscription: false,
      recurringCycleLimit: 1,
      usageLimit: 5,
    });

    const update = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation UpdateCodeFreeShipping($id: ID!, $input: DiscountCodeFreeShippingInput!) {
            discountCodeFreeShippingUpdate(id: $id, freeShippingCodeDiscount: $input) {
              codeDiscountNode {
                ${codeFreeShippingSelection}
              }
              ${userErrorsSelection}
            }
          }
        `,
        variables: {
          id: discountId,
          input: {
            ...codeFreeShippingInput('HAR196SHIP'),
            title: 'HAR-196 code free shipping updated',
            combinesWith: {
              productDiscounts: false,
              orderDiscounts: true,
              shippingDiscounts: false,
            },
            minimumRequirement: {
              subtotal: {
                greaterThanOrEqualToSubtotal: '12.00',
              },
            },
            destination: {
              countries: {
                add: ['US', 'CA'],
                includeRestOfWorld: false,
              },
            },
            maximumShippingPrice: '30.00',
            appliesOncePerCustomer: false,
            appliesOnOneTimePurchase: false,
            appliesOnSubscription: true,
            recurringCycleLimit: 2,
            usageLimit: 10,
          },
        },
      });

    expect(update.status).toBe(200);
    expect(update.body.data.discountCodeFreeShippingUpdate.userErrors).toEqual([]);
    expect(update.body.data.discountCodeFreeShippingUpdate.codeDiscountNode.codeDiscount).toMatchObject({
      title: 'HAR-196 code free shipping updated',
      summary:
        'Free shipping on subscription products • Minimum purchase of $12.00 • For 2 countries • Applies to shipping rates under $30.00',
      codes: {
        nodes: [
          {
            code: 'HAR196SHIP',
          },
        ],
      },
      destinationSelection: {
        __typename: 'DiscountCountries',
        countries: ['CA', 'US'],
        includeRestOfWorld: false,
      },
      appliesOncePerCustomer: false,
      appliesOnOneTimePurchase: false,
      appliesOnSubscription: true,
      recurringCycleLimit: 2,
      usageLimit: 10,
    });

    const readAfterUpdate = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          query CodeFreeShippingReads($id: ID!, $code: String!) {
            discountNode(id: $id) {
              id
              discount {
                __typename
                ... on DiscountCodeFreeShipping {
                  title
                  status
                }
              }
            }
            codeDiscountNodeByCode(code: $code) {
              id
            }
            discountNodes(first: 5, query: "type:free_shipping") {
              nodes {
                id
                discount {
                  __typename
                }
              }
            }
          }
        `,
        variables: {
          id: discountId,
          code: 'HAR196SHIP',
        },
      });

    expect(readAfterUpdate.body.data).toEqual({
      discountNode: {
        id: discountId,
        discount: {
          __typename: 'DiscountCodeFreeShipping',
          title: 'HAR-196 code free shipping updated',
          status: 'ACTIVE',
        },
      },
      codeDiscountNodeByCode: {
        id: discountId,
      },
      discountNodes: {
        nodes: [
          {
            id: discountId,
            discount: {
              __typename: 'DiscountCodeFreeShipping',
            },
          },
        ],
      },
    });

    const deactivate = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation DeactivateCodeFreeShipping($id: ID!) {
            discountCodeDeactivate(id: $id) {
              codeDiscountNode {
                id
                codeDiscount {
                  ... on DiscountCodeFreeShipping {
                    status
                  }
                }
              }
              ${userErrorsSelection}
            }
          }
        `,
        variables: {
          id: discountId,
        },
      });

    expect(deactivate.body.data.discountCodeDeactivate.userErrors).toEqual([]);
    expect(deactivate.body.data.discountCodeDeactivate.codeDiscountNode.codeDiscount.status).toBe('EXPIRED');

    const activate = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation ActivateCodeFreeShipping($id: ID!) {
            discountCodeActivate(id: $id) {
              codeDiscountNode {
                id
                codeDiscount {
                  ... on DiscountCodeFreeShipping {
                    status
                  }
                }
              }
              ${userErrorsSelection}
            }
          }
        `,
        variables: {
          id: discountId,
        },
      });

    expect(activate.body.data.discountCodeActivate.userErrors).toEqual([]);
    expect(activate.body.data.discountCodeActivate.codeDiscountNode.codeDiscount.status).toBe('ACTIVE');

    const deleted = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation DeleteCodeFreeShipping($id: ID!) {
            discountCodeDelete(id: $id) {
              deletedCodeDiscountId
              ${userErrorsSelection}
            }
          }
        `,
        variables: {
          id: discountId,
        },
      });

    expect(deleted.body.data.discountCodeDelete).toEqual({
      deletedCodeDiscountId: discountId,
      userErrors: [],
    });
    expect(store.getLog().map((entry) => entry.operationName)).toEqual([
      'discountCodeFreeShippingCreate',
      'discountCodeFreeShippingUpdate',
      'discountCodeDeactivate',
      'discountCodeActivate',
      'discountCodeDelete',
    ]);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages automatic free-shipping create-update-status-delete locally with downstream reads', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('automatic free-shipping discount staging should not hit upstream fetch');
    });
    const app = createApp(config).callback();

    const create = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation CreateAutomaticFreeShipping($input: DiscountAutomaticFreeShippingInput!) {
            discountAutomaticFreeShippingCreate(freeShippingAutomaticDiscount: $input) {
              automaticDiscountNode {
                ${automaticFreeShippingSelection}
              }
              ${userErrorsSelection}
            }
          }
        `,
        variables: {
          input: automaticFreeShippingInput(),
        },
      });

    expect(create.status).toBe(200);
    expect(create.body.data.discountAutomaticFreeShippingCreate.userErrors).toEqual([]);
    const discountId = create.body.data.discountAutomaticFreeShippingCreate.automaticDiscountNode.id as string;
    expect(discountId).toMatch(/^gid:\/\/shopify\/DiscountAutomaticNode\/[0-9]+\?shopify-draft-proxy=synthetic$/u);
    expect(create.body.data.discountAutomaticFreeShippingCreate.automaticDiscountNode.automaticDiscount).toMatchObject({
      __typename: 'DiscountAutomaticFreeShipping',
      title: 'HAR-196 automatic free shipping',
      status: 'ACTIVE',
      summary:
        'Free shipping on all products • Minimum purchase of $15.00 • For all countries • Applies to shipping rates under $20.00',
      discountClasses: ['SHIPPING'],
      maximumShippingPrice: {
        amount: '20.0',
        currencyCode: 'CAD',
      },
      appliesOnOneTimePurchase: true,
      appliesOnSubscription: false,
      recurringCycleLimit: 1,
    });

    const update = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation UpdateAutomaticFreeShipping($id: ID!, $input: DiscountAutomaticFreeShippingInput!) {
            discountAutomaticFreeShippingUpdate(id: $id, freeShippingAutomaticDiscount: $input) {
              automaticDiscountNode {
                ${automaticFreeShippingSelection}
              }
              ${userErrorsSelection}
            }
          }
        `,
        variables: {
          id: discountId,
          input: automaticFreeShippingInput({
            title: 'HAR-196 automatic free shipping updated',
            combinesWith: {
              productDiscounts: true,
              orderDiscounts: false,
              shippingDiscounts: false,
            },
            minimumRequirement: {
              subtotal: {
                greaterThanOrEqualToSubtotal: '18.00',
              },
            },
            destination: {
              countries: {
                add: ['US'],
                includeRestOfWorld: false,
              },
            },
            maximumShippingPrice: '22.00',
            appliesOnOneTimePurchase: false,
            appliesOnSubscription: true,
            recurringCycleLimit: 3,
          }),
        },
      });

    expect(update.status).toBe(200);
    expect(update.body.data.discountAutomaticFreeShippingUpdate.userErrors).toEqual([]);
    expect(update.body.data.discountAutomaticFreeShippingUpdate.automaticDiscountNode.automaticDiscount).toMatchObject({
      title: 'HAR-196 automatic free shipping updated',
      summary:
        'Free shipping on subscription products • Minimum purchase of $18.00 • For United States • Applies to shipping rates under $22.00',
      destinationSelection: {
        __typename: 'DiscountCountries',
        countries: ['US'],
        includeRestOfWorld: false,
      },
      appliesOnOneTimePurchase: false,
      appliesOnSubscription: true,
      recurringCycleLimit: 3,
    });

    const readAfterUpdate = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          query AutomaticFreeShippingReads($id: ID!) {
            automaticDiscountNode(id: $id) {
              id
              automaticDiscount {
                __typename
                ... on DiscountAutomaticFreeShipping {
                  title
                  status
                }
              }
            }
            automaticDiscountNodes(first: 5, query: "type:free_shipping") {
              nodes {
                id
                automaticDiscount {
                  __typename
                }
              }
            }
            discountNodesCount(query: "method:automatic type:free_shipping") {
              count
              precision
            }
          }
        `,
        variables: {
          id: discountId,
        },
      });

    expect(readAfterUpdate.body.data).toEqual({
      automaticDiscountNode: {
        id: discountId,
        automaticDiscount: {
          __typename: 'DiscountAutomaticFreeShipping',
          title: 'HAR-196 automatic free shipping updated',
          status: 'ACTIVE',
        },
      },
      automaticDiscountNodes: {
        nodes: [
          {
            id: discountId,
            automaticDiscount: {
              __typename: 'DiscountAutomaticFreeShipping',
            },
          },
        ],
      },
      discountNodesCount: {
        count: 1,
        precision: 'EXACT',
      },
    });

    const deactivate = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation DeactivateAutomaticFreeShipping($id: ID!) {
            discountAutomaticDeactivate(id: $id) {
              automaticDiscountNode {
                id
                automaticDiscount {
                  ... on DiscountAutomaticFreeShipping {
                    status
                    endsAt
                  }
                }
              }
              ${userErrorsSelection}
            }
          }
        `,
        variables: {
          id: discountId,
        },
      });

    expect(deactivate.body.data.discountAutomaticDeactivate.userErrors).toEqual([]);
    expect(deactivate.body.data.discountAutomaticDeactivate.automaticDiscountNode.automaticDiscount.status).toBe(
      'EXPIRED',
    );
    expect(deactivate.body.data.discountAutomaticDeactivate.automaticDiscountNode.automaticDiscount.endsAt).toEqual(
      expect.any(String),
    );

    const activate = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation ActivateAutomaticFreeShipping($id: ID!) {
            discountAutomaticActivate(id: $id) {
              automaticDiscountNode {
                id
                automaticDiscount {
                  ... on DiscountAutomaticFreeShipping {
                    status
                  }
                }
              }
              ${userErrorsSelection}
            }
          }
        `,
        variables: {
          id: discountId,
        },
      });

    expect(activate.body.data.discountAutomaticActivate.userErrors).toEqual([]);
    expect(activate.body.data.discountAutomaticActivate.automaticDiscountNode.automaticDiscount.status).toBe('ACTIVE');

    const deleted = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation DeleteAutomaticFreeShipping($id: ID!) {
            discountAutomaticDelete(id: $id) {
              deletedAutomaticDiscountId
              ${userErrorsSelection}
            }
          }
        `,
        variables: {
          id: discountId,
        },
      });

    expect(deleted.body.data.discountAutomaticDelete).toEqual({
      deletedAutomaticDiscountId: discountId,
      userErrors: [],
    });
    expect(store.getLog().map((entry) => entry.operationName)).toEqual([
      'discountAutomaticFreeShippingCreate',
      'discountAutomaticFreeShippingUpdate',
      'discountAutomaticDeactivate',
      'discountAutomaticActivate',
      'discountAutomaticDelete',
    ]);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('returns captured requirement-conflict userErrors without staging invalid free-shipping inputs', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('invalid free-shipping discount inputs should not hit upstream fetch');
    });
    const app = createApp(config).callback();
    const conflictingRequirement = {
      subtotal: {
        greaterThanOrEqualToSubtotal: '10.00',
      },
      quantity: {
        greaterThanOrEqualToQuantity: '2',
      },
    };

    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation InvalidFreeShipping(
            $code: DiscountCodeFreeShippingInput!
            $automatic: DiscountAutomaticFreeShippingInput!
          ) {
            code: discountCodeFreeShippingCreate(freeShippingCodeDiscount: $code) {
              codeDiscountNode {
                id
              }
              ${userErrorsSelection}
            }
            automatic: discountAutomaticFreeShippingCreate(freeShippingAutomaticDiscount: $automatic) {
              automaticDiscountNode {
                id
              }
              ${userErrorsSelection}
            }
          }
        `,
        variables: {
          code: {
            ...codeFreeShippingInput(),
            minimumRequirement: conflictingRequirement,
          },
          automatic: automaticFreeShippingInput({
            minimumRequirement: conflictingRequirement,
          }),
        },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.code).toEqual({
      codeDiscountNode: null,
      userErrors: [
        {
          field: ['freeShippingCodeDiscount', 'minimumRequirement', 'subtotal', 'greaterThanOrEqualToSubtotal'],
          message: 'Minimum subtotal cannot be defined when minimum quantity is.',
          code: 'CONFLICT',
          extraInfo: null,
        },
        {
          field: ['freeShippingCodeDiscount', 'minimumRequirement', 'quantity', 'greaterThanOrEqualToQuantity'],
          message: 'Minimum quantity cannot be defined when minimum subtotal is.',
          code: 'CONFLICT',
          extraInfo: null,
        },
      ],
    });
    expect(response.body.data.automatic).toEqual({
      automaticDiscountNode: null,
      userErrors: [
        {
          field: ['freeShippingAutomaticDiscount', 'minimumRequirement', 'subtotal', 'greaterThanOrEqualToSubtotal'],
          message: 'Minimum subtotal cannot be defined when minimum quantity is.',
          code: 'CONFLICT',
          extraInfo: null,
        },
        {
          field: ['freeShippingAutomaticDiscount', 'minimumRequirement', 'quantity', 'greaterThanOrEqualToQuantity'],
          message: 'Minimum quantity cannot be defined when minimum subtotal is.',
          code: 'CONFLICT',
          extraInfo: null,
        },
      ],
    });
    expect(store.listEffectiveDiscounts()).toEqual([]);
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});

import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { resetSyntheticIdentity } from '../../src/state/synthetic-identity.js';
import { store } from '../../src/state/store.js';
import type { GiftCardRecord } from '../../src/state/types.js';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

const giftCardSelection = `#graphql
    id
    lastCharacters
    maskedCode
    enabled
    deactivatedAt
    expiresOn
    note
    templateSuffix
    createdAt
    updatedAt
    initialValue {
      amount
      currencyCode
    }
    balance {
      amount
      currencyCode
    }
    transactions(first: 5) {
      nodes {
        id
        note
        processedAt
        amount {
          amount
          currencyCode
        }
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
`;

function baseGiftCard(overrides: Partial<GiftCardRecord> = {}): GiftCardRecord {
  return {
    id: 'gid://shopify/GiftCard/1001',
    legacyResourceId: '1001',
    lastCharacters: 'BASE',
    maskedCode: '**** **** **** BASE',
    enabled: true,
    deactivatedAt: null,
    expiresOn: '2027-04-26',
    note: 'Existing gift card',
    templateSuffix: null,
    createdAt: '2026-04-20T12:00:00.000Z',
    updatedAt: '2026-04-20T12:00:00.000Z',
    initialValue: {
      amount: '25.0',
      currencyCode: 'CAD',
    },
    balance: {
      amount: '25.0',
      currencyCode: 'CAD',
    },
    customerId: null,
    recipientId: null,
    transactions: [],
    ...overrides,
  };
}

describe('gift-card local staging', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('serves empty and seeded gift-card reads locally in snapshot mode', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('gift-card reads must stay local'));
    store.upsertBaseGiftCards([baseGiftCard()]);
    store.upsertBaseGiftCardConfiguration({
      issueLimit: {
        amount: '1000.0',
        currencyCode: 'CAD',
      },
      purchaseLimit: {
        amount: '500.0',
        currencyCode: 'CAD',
      },
    });
    const app = createApp(config).callback();

    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query GiftCardReads($id: ID!, $missingId: ID!) {
          giftCard(id: $id) {
            ${giftCardSelection}
          }
          missingGiftCard: giftCard(id: $missingId) {
            id
          }
          giftCards(first: 2, sortKey: ID) {
            nodes {
              id
              lastCharacters
              balance {
                amount
                currencyCode
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
          giftCardsCount {
            count
            precision
          }
          filteredEmptyGiftCards: giftCards(first: 2, query: "id:999999999999", sortKey: ID) {
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
          filteredEmptyGiftCardsCount: giftCardsCount(query: "id:999999999999") {
            count
            precision
          }
          giftCardConfiguration {
            issueLimit {
              amount
              currencyCode
            }
            purchaseLimit {
              amount
              currencyCode
            }
          }
        }`,
        variables: {
          id: 'gid://shopify/GiftCard/1001',
          missingId: 'gid://shopify/GiftCard/999999999999',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.giftCard).toMatchObject({
      id: 'gid://shopify/GiftCard/1001',
      lastCharacters: 'BASE',
      maskedCode: '**** **** **** BASE',
      enabled: true,
      balance: {
        amount: '25.0',
        currencyCode: 'CAD',
      },
      transactions: {
        nodes: [],
        pageInfo: {
          hasNextPage: false,
          hasPreviousPage: false,
          startCursor: null,
          endCursor: null,
        },
      },
    });
    expect(response.body.data.missingGiftCard).toBeNull();
    expect(response.body.data.giftCards).toEqual({
      nodes: [
        {
          id: 'gid://shopify/GiftCard/1001',
          lastCharacters: 'BASE',
          balance: {
            amount: '25.0',
            currencyCode: 'CAD',
          },
        },
      ],
      edges: [
        {
          cursor: 'cursor:gid://shopify/GiftCard/1001',
          node: {
            id: 'gid://shopify/GiftCard/1001',
          },
        },
      ],
      pageInfo: {
        hasNextPage: false,
        hasPreviousPage: false,
        startCursor: 'cursor:gid://shopify/GiftCard/1001',
        endCursor: 'cursor:gid://shopify/GiftCard/1001',
      },
    });
    expect(response.body.data.giftCardsCount).toEqual({ count: 1, precision: 'EXACT' });
    expect(response.body.data.filteredEmptyGiftCards).toEqual({
      nodes: [],
      edges: [],
      pageInfo: {
        hasNextPage: false,
        hasPreviousPage: false,
        startCursor: null,
        endCursor: null,
      },
    });
    expect(response.body.data.filteredEmptyGiftCardsCount).toEqual({ count: 0, precision: 'EXACT' });
    expect(response.body.data.giftCardConfiguration).toEqual({
      issueLimit: {
        amount: '1000.0',
        currencyCode: 'CAD',
      },
      purchaseLimit: {
        amount: '500.0',
        currencyCode: 'CAD',
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages gift-card create, update, credit, debit, deactivate, and notification roots locally', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('gift-card mutations must stay local'));
    const app = createApp(config).callback();

    const createResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation CreateGiftCard($input: GiftCardCreateInput!) {
          giftCardCreate(input: $input) {
            giftCard {
              ${giftCardSelection}
            }
            giftCardCode
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          input: {
            initialValue: '50.00',
            code: 'HAR310LOCALCARD',
            note: 'Local create',
            expiresOn: '2027-04-26',
          },
        },
      });

    expect(createResponse.status).toBe(200);
    const createdGiftCard = createResponse.body.data.giftCardCreate.giftCard;
    expect(createResponse.body.data.giftCardCreate.userErrors).toEqual([]);
    expect(createResponse.body.data.giftCardCreate.giftCardCode).toBe('HAR310LOCALCARD');
    expect(createdGiftCard).toMatchObject({
      lastCharacters: 'CARD',
      maskedCode: '**** **** **** CARD',
      enabled: true,
      note: 'Local create',
      expiresOn: '2027-04-26',
      initialValue: {
        amount: '50.0',
        currencyCode: 'CAD',
      },
      balance: {
        amount: '50.0',
        currencyCode: 'CAD',
      },
    });

    const updateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation UpdateGiftCard($id: ID!, $input: GiftCardUpdateInput!) {
          giftCardUpdate(id: $id, input: $input) {
            giftCard {
              id
              note
              templateSuffix
              expiresOn
              balance {
                amount
                currencyCode
              }
            }
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          id: createdGiftCard.id,
          input: {
            note: 'Updated local card',
            templateSuffix: 'birthday',
            expiresOn: '2028-04-26',
          },
        },
      });

    expect(updateResponse.body.data.giftCardUpdate.userErrors).toEqual([]);
    expect(updateResponse.body.data.giftCardUpdate.giftCard).toMatchObject({
      id: createdGiftCard.id,
      note: 'Updated local card',
      templateSuffix: 'birthday',
      expiresOn: '2028-04-26',
      balance: {
        amount: '50.0',
        currencyCode: 'CAD',
      },
    });

    const creditResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation CreditGiftCard($id: ID!, $amount: MoneyInput!) {
          giftCardCredit(id: $id, creditInput: { creditAmount: $amount, note: "Manual credit" }) {
            giftCardCreditTransaction {
              note
              amount {
                amount
                currencyCode
              }
              giftCard {
                id
                balance {
                  amount
                  currencyCode
                }
              }
            }
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          id: createdGiftCard.id,
          amount: {
            amount: '15.00',
            currencyCode: 'CAD',
          },
        },
      });

    expect(creditResponse.body.data.giftCardCredit.userErrors).toEqual([]);
    expect(creditResponse.body.data.giftCardCredit.giftCardCreditTransaction.giftCard.balance).toEqual({
      amount: '65.0',
      currencyCode: 'CAD',
    });
    expect(creditResponse.body.data.giftCardCredit.giftCardCreditTransaction).toMatchObject({
      note: 'Manual credit',
      amount: {
        amount: '15.0',
        currencyCode: 'CAD',
      },
    });

    const debitResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation DebitGiftCard($id: ID!, $input: GiftCardDebitInput!) {
          giftCardDebit(id: $id, debitInput: $input) {
            giftCardDebitTransaction {
              amount {
                amount
                currencyCode
              }
              giftCard {
                id
                balance {
                  amount
                  currencyCode
                }
              }
            }
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          id: createdGiftCard.id,
          input: {
            debitAmount: {
              amount: '20.00',
              currencyCode: 'CAD',
            },
          },
        },
      });

    expect(debitResponse.body.data.giftCardDebit.userErrors).toEqual([]);
    expect(debitResponse.body.data.giftCardDebit.giftCardDebitTransaction.giftCard.balance).toEqual({
      amount: '45.0',
      currencyCode: 'CAD',
    });
    expect(debitResponse.body.data.giftCardDebit.giftCardDebitTransaction).toMatchObject({
      amount: {
        amount: '-20.0',
        currencyCode: 'CAD',
      },
    });

    const notificationResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation NotifyGiftCard($id: ID!) {
          customerNotification: giftCardSendNotificationToCustomer(id: $id) {
            giftCard {
              id
            }
            userErrors {
              field
              message
            }
          }
          recipientNotification: giftCardSendNotificationToRecipient(id: $id) {
            giftCard {
              id
            }
            userErrors {
              field
              message
            }
          }
        }`,
        variables: { id: createdGiftCard.id },
      });

    expect(notificationResponse.body.data.customerNotification.userErrors).toEqual([]);
    expect(notificationResponse.body.data.recipientNotification.userErrors).toEqual([]);
    expect(notificationResponse.body.data.customerNotification.giftCard.id).toBe(createdGiftCard.id);
    expect(notificationResponse.body.data.recipientNotification.giftCard.id).toBe(createdGiftCard.id);

    const deactivateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation DeactivateGiftCard($id: ID!) {
          giftCardDeactivate(id: $id) {
            giftCard {
              id
              enabled
              deactivatedAt
            }
            userErrors {
              field
              message
            }
          }
        }`,
        variables: { id: createdGiftCard.id },
      });

    expect(deactivateResponse.body.data.giftCardDeactivate.userErrors).toEqual([]);
    expect(deactivateResponse.body.data.giftCardDeactivate.giftCard).toMatchObject({
      id: createdGiftCard.id,
      enabled: false,
    });
    expect(deactivateResponse.body.data.giftCardDeactivate.giftCard.deactivatedAt).toEqual(expect.any(String));

    const readAfterWriteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query GiftCardReadAfterWrite($id: ID!) {
          giftCard(id: $id) {
            ${giftCardSelection}
          }
          giftCardsCount {
            count
            precision
          }
        }`,
        variables: { id: createdGiftCard.id },
      });

    expect(readAfterWriteResponse.body.data.giftCard).toMatchObject({
      id: createdGiftCard.id,
      enabled: false,
      note: 'Updated local card',
      templateSuffix: 'birthday',
      balance: {
        amount: '45.0',
        currencyCode: 'CAD',
      },
    });
    expect(readAfterWriteResponse.body.data.giftCard.transactions.nodes).toHaveLength(2);
    expect(readAfterWriteResponse.body.data.giftCardsCount).toEqual({ count: 1, precision: 'EXACT' });

    const metaLogResponse = await request(app).get('/__meta/log');
    expect(metaLogResponse.body.entries.map((entry: { status: string }) => entry.status)).toEqual([
      'staged',
      'staged',
      'staged',
      'staged',
      'staged',
      'staged',
    ]);
    expect(
      metaLogResponse.body.entries.map(
        (entry: { interpreted: { rootFields: string[] } }) => entry.interpreted.rootFields[0],
      ),
    ).toEqual([
      'giftCardCreate',
      'giftCardUpdate',
      'giftCardCredit',
      'giftCardDebit',
      'giftCardSendNotificationToCustomer',
      'giftCardDeactivate',
    ]);
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});

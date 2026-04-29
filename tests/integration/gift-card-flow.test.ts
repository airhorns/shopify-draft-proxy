import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../support/runtime.js';
import type { AppConfig } from '../../src/config.js';
import { resetSyntheticIdentity } from '../support/runtime.js';
import { store } from '../support/runtime.js';
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
    recipientAttributes {
      message
      preferredName
      sendNotificationAt
      recipient {
        id
      }
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
    store.upsertBaseGiftCards([
      baseGiftCard(),
      baseGiftCard({
        id: 'gid://shopify/GiftCard/1002',
        legacyResourceId: '1002',
        lastCharacters: 'STOP',
        maskedCode: '**** **** **** STOP',
        enabled: false,
        deactivatedAt: '2026-04-21T12:00:00.000Z',
        balance: {
          amount: '0.0',
          currencyCode: 'CAD',
        },
      }),
      baseGiftCard({
        id: 'gid://shopify/GiftCard/1003',
        legacyResourceId: '1003',
        lastCharacters: 'PART',
        maskedCode: '**** **** **** PART',
        balance: {
          amount: '10.0',
          currencyCode: 'CAD',
        },
      }),
    ]);
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
          enabledGiftCards: giftCards(first: 5, query: "status:enabled", sortKey: ID) {
            nodes {
              id
              enabled
            }
          }
          disabledGiftCards: giftCards(first: 5, query: "status:disabled", sortKey: ID) {
            nodes {
              id
              enabled
            }
          }
          disabledGiftCardsCount: giftCardsCount(query: "status:disabled") {
            count
            precision
          }
          fullBalanceGiftCards: giftCards(first: 5, query: "balance_status:full", sortKey: ID) {
            nodes {
              id
            }
          }
          partialBalanceGiftCards: giftCards(first: 5, query: "balance_status:partial", sortKey: ID) {
            nodes {
              id
            }
          }
          emptyBalanceGiftCards: giftCards(first: 5, query: "balance_status:empty", sortKey: ID) {
            nodes {
              id
            }
          }
          nonEmptyBalanceGiftCardsCount: giftCardsCount(query: "balance_status:full_or_partial") {
            count
            precision
          }
          codeSearchGiftCards: giftCards(first: 5, query: "PART", sortKey: ID) {
            nodes {
              id
              lastCharacters
            }
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
        {
          id: 'gid://shopify/GiftCard/1002',
          lastCharacters: 'STOP',
          balance: {
            amount: '0.0',
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
        {
          cursor: 'cursor:gid://shopify/GiftCard/1002',
          node: {
            id: 'gid://shopify/GiftCard/1002',
          },
        },
      ],
      pageInfo: {
        hasNextPage: true,
        hasPreviousPage: false,
        startCursor: 'cursor:gid://shopify/GiftCard/1001',
        endCursor: 'cursor:gid://shopify/GiftCard/1002',
      },
    });
    expect(response.body.data.giftCardsCount).toEqual({ count: 3, precision: 'EXACT' });
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
    expect(response.body.data.enabledGiftCards.nodes).toEqual([
      { id: 'gid://shopify/GiftCard/1001', enabled: true },
      { id: 'gid://shopify/GiftCard/1003', enabled: true },
    ]);
    expect(response.body.data.disabledGiftCards.nodes).toEqual([{ id: 'gid://shopify/GiftCard/1002', enabled: false }]);
    expect(response.body.data.disabledGiftCardsCount).toEqual({ count: 1, precision: 'EXACT' });
    expect(response.body.data.fullBalanceGiftCards.nodes).toEqual([{ id: 'gid://shopify/GiftCard/1001' }]);
    expect(response.body.data.partialBalanceGiftCards.nodes).toEqual([{ id: 'gid://shopify/GiftCard/1003' }]);
    expect(response.body.data.emptyBalanceGiftCards.nodes).toEqual([{ id: 'gid://shopify/GiftCard/1002' }]);
    expect(response.body.data.nonEmptyBalanceGiftCardsCount).toEqual({ count: 2, precision: 'EXACT' });
    expect(response.body.data.codeSearchGiftCards.nodes).toEqual([
      { id: 'gid://shopify/GiftCard/1003', lastCharacters: 'PART' },
    ]);
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

  it('filters seeded gift cards by captured advanced search fields locally', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('gift-card reads must stay local'));
    const customerId = 'gid://shopify/Customer/2001';
    store.upsertBaseGiftCards([
      baseGiftCard({
        id: 'gid://shopify/GiftCard/1001',
        legacyResourceId: '1001',
        lastCharacters: 'OLD1',
        maskedCode: '**** **** **** OLD1',
        createdAt: '2026-04-20T12:00:00.000Z',
        updatedAt: '2026-04-20T12:00:00.000Z',
        expiresOn: '2027-04-26',
        source: 'manual',
      }),
      baseGiftCard({
        id: 'gid://shopify/GiftCard/1004',
        legacyResourceId: '1004',
        lastCharacters: 'RICH',
        maskedCode: '**** **** **** RICH',
        createdAt: '2026-04-24T12:00:00.000Z',
        updatedAt: '2026-04-25T12:00:00.000Z',
        expiresOn: '2028-04-26',
        initialValue: {
          amount: '50.0',
          currencyCode: 'CAD',
        },
        balance: {
          amount: '50.0',
          currencyCode: 'CAD',
        },
        customerId,
        recipientId: customerId,
        source: 'api_client',
        recipientAttributes: {
          id: customerId,
          message: 'Advanced search recipient',
          preferredName: 'Search Recipient',
          sendNotificationAt: null,
        },
      }),
    ]);
    const app = createApp(config).callback();

    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query GiftCardAdvancedSearch($customerQuery: String!, $recipientQuery: String!) {
          createdAfterGiftCards: giftCards(first: 5, query: "created_at:>=2026-04-22", sortKey: ID) {
            nodes {
              id
              createdAt
            }
          }
          expiresAfterGiftCards: giftCards(first: 5, query: "expires_on:>=2028-01-01", sortKey: ID) {
            nodes {
              id
              expiresOn
            }
          }
          futureCreatedGiftCards: giftCards(first: 5, query: "created_at:>=2099-01-01", sortKey: ID) {
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
          futureCreatedGiftCardsCount: giftCardsCount(query: "created_at:>=2099-01-01") {
            count
            precision
          }
          customerGiftCards: giftCards(first: 5, query: $customerQuery, sortKey: ID) {
            nodes {
              id
              customer {
                id
              }
            }
          }
          recipientGiftCards: giftCards(first: 5, query: $recipientQuery, sortKey: ID) {
            nodes {
              id
              recipientAttributes {
                recipient {
                  id
                }
              }
            }
          }
          sourceGiftCards: giftCards(first: 5, query: "source:api_client", sortKey: ID) {
            nodes {
              id
            }
          }
          initialValueGiftCards: giftCards(first: 5, query: "initial_value:>=50", sortKey: ID) {
            nodes {
              id
              initialValue {
                amount
                currencyCode
              }
            }
          }
        }`,
        variables: {
          customerQuery: 'customer_id:2001',
          recipientQuery: 'recipient_id:2001',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.createdAfterGiftCards.nodes).toEqual([
      { id: 'gid://shopify/GiftCard/1004', createdAt: '2026-04-24T12:00:00.000Z' },
    ]);
    expect(response.body.data.expiresAfterGiftCards.nodes).toEqual([
      { id: 'gid://shopify/GiftCard/1004', expiresOn: '2028-04-26' },
    ]);
    expect(response.body.data.futureCreatedGiftCards).toEqual({
      nodes: [],
      edges: [],
      pageInfo: {
        hasNextPage: false,
        hasPreviousPage: false,
        startCursor: null,
        endCursor: null,
      },
    });
    expect(response.body.data.futureCreatedGiftCardsCount).toEqual({ count: 0, precision: 'EXACT' });
    expect(response.body.data.customerGiftCards.nodes).toEqual([
      { id: 'gid://shopify/GiftCard/1004', customer: { id: customerId } },
    ]);
    expect(response.body.data.recipientGiftCards.nodes).toEqual([
      { id: 'gid://shopify/GiftCard/1004', recipientAttributes: { recipient: { id: customerId } } },
    ]);
    expect(response.body.data.sourceGiftCards.nodes).toEqual([{ id: 'gid://shopify/GiftCard/1004' }]);
    expect(response.body.data.initialValueGiftCards.nodes).toEqual([
      { id: 'gid://shopify/GiftCard/1004', initialValue: { amount: '50.0', currencyCode: 'CAD' } },
    ]);
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
            recipientAttributes: {
              id: 'gid://shopify/Customer/9001',
              message: 'Enjoy this local card',
              preferredName: 'Local Recipient',
              sendNotificationAt: '2026-05-01T10:00:00Z',
            },
          },
        },
      });

    expect(createResponse.status).toBe(200);
    const createdGiftCard = createResponse.body.data.giftCardCreate.giftCard;
    expect(createResponse.body.data.giftCardCreate.userErrors).toEqual([]);
    expect(createResponse.body.data.giftCardCreate.giftCardCode).toBe('har310localcard');
    expect(createdGiftCard).toMatchObject({
      lastCharacters: 'CARD',
      maskedCode: '\u2022\u2022\u2022\u2022 \u2022\u2022\u2022\u2022 \u2022\u2022\u2022\u2022 CARD',
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
      recipientAttributes: {
        message: 'Enjoy this local card',
        preferredName: 'Local Recipient',
        sendNotificationAt: '2026-05-01T10:00:00Z',
        recipient: {
          id: 'gid://shopify/Customer/9001',
        },
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
              recipientAttributes {
                message
                preferredName
                sendNotificationAt
                recipient {
                  id
                }
              }
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
            recipientAttributes: {
              id: 'gid://shopify/Customer/9001',
              message: 'Updated gift-card message',
              preferredName: 'Updated Recipient',
              sendNotificationAt: '2026-05-02T10:00:00Z',
            },
          },
        },
      });

    expect(updateResponse.body.data.giftCardUpdate.userErrors).toEqual([]);
    expect(updateResponse.body.data.giftCardUpdate.giftCard).toMatchObject({
      id: createdGiftCard.id,
      note: 'Updated local card',
      templateSuffix: 'birthday',
      expiresOn: '2028-04-26',
      recipientAttributes: {
        message: 'Updated gift-card message',
        preferredName: 'Updated Recipient',
        sendNotificationAt: '2026-05-02T10:00:00Z',
        recipient: {
          id: 'gid://shopify/Customer/9001',
        },
      },
      balance: {
        amount: '50.0',
        currencyCode: 'CAD',
      },
    });

    const creditResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation CreditGiftCard($id: ID!, $amount: MoneyInput!, $processedAt: DateTime!) {
          giftCardCredit(id: $id, creditInput: { creditAmount: $amount, note: "Manual credit", processedAt: $processedAt }) {
            giftCardCreditTransaction {
              note
              processedAt
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
          processedAt: '2026-04-21T12:34:56Z',
        },
      });

    expect(creditResponse.body.data.giftCardCredit.userErrors).toEqual([]);
    expect(creditResponse.body.data.giftCardCredit.giftCardCreditTransaction.giftCard.balance).toEqual({
      amount: '65.0',
      currencyCode: 'CAD',
    });
    expect(creditResponse.body.data.giftCardCredit.giftCardCreditTransaction).toMatchObject({
      note: 'Manual credit',
      processedAt: '2026-04-21T12:34:56Z',
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
              processedAt
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
            processedAt: '2026-04-21T13:34:56Z',
          },
        },
      });

    expect(debitResponse.body.data.giftCardDebit.userErrors).toEqual([]);
    expect(debitResponse.body.data.giftCardDebit.giftCardDebitTransaction.giftCard.balance).toEqual({
      amount: '45.0',
      currencyCode: 'CAD',
    });
    expect(debitResponse.body.data.giftCardDebit.giftCardDebitTransaction).toMatchObject({
      processedAt: '2026-04-21T13:34:56Z',
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
          enabledGiftCards: giftCards(first: 2, query: "status:enabled", sortKey: ID) {
            nodes {
              id
            }
          }
          disabledGiftCards: giftCards(first: 2, query: "status:disabled", sortKey: ID) {
            nodes {
              id
              enabled
            }
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
    expect(readAfterWriteResponse.body.data.enabledGiftCards.nodes).toEqual([]);
    expect(readAfterWriteResponse.body.data.disabledGiftCards.nodes).toEqual([
      { id: createdGiftCard.id, enabled: false },
    ]);

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
        (entry: { interpreted: { rootFields: string[] } }) => entry.interpreted.rootFields,
      ),
    ).toEqual([
      ['giftCardCreate'],
      ['giftCardUpdate'],
      ['giftCardCredit'],
      ['giftCardDebit'],
      ['giftCardSendNotificationToCustomer', 'giftCardSendNotificationToRecipient'],
      ['giftCardDeactivate'],
    ]);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('returns local userErrors for unsupported gift-card lifecycle inputs', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('gift-card errors must stay local'));
    store.upsertBaseGiftCards([baseGiftCard()]);
    const app = createApp(config).callback();

    const invalidCreateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation InvalidGiftCardCreate($input: GiftCardCreateInput!) {
          giftCardCreate(input: $input) {
            giftCard {
              id
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
            initialValue: '0.00',
          },
        },
      });

    expect(invalidCreateResponse.status).toBe(200);
    expect(invalidCreateResponse.body.data.giftCardCreate).toEqual({
      giftCard: null,
      giftCardCode: null,
      userErrors: [
        {
          field: ['input', 'initialValue'],
          message: 'Initial value must be greater than zero',
        },
      ],
    });

    const debitTooHighResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation DebitTooHigh($id: ID!) {
          giftCardDebit(id: $id, debitInput: { debitAmount: { amount: "30.00", currencyCode: CAD } }) {
            giftCardDebitTransaction {
              id
            }
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          id: 'gid://shopify/GiftCard/1001',
        },
      });

    expect(debitTooHighResponse.status).toBe(200);
    expect(debitTooHighResponse.body.data.giftCardDebit).toEqual({
      giftCardDebitTransaction: null,
      userErrors: [
        {
          field: ['debitAmount'],
          message: 'Insufficient balance',
        },
      ],
    });

    const missingNotificationResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation MissingGiftCardNotification($id: ID!) {
          giftCardSendNotificationToRecipient(id: $id) {
            giftCard {
              id
            }
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          id: 'gid://shopify/GiftCard/999999999999',
        },
      });

    expect(missingNotificationResponse.status).toBe(200);
    expect(missingNotificationResponse.body.data.giftCardSendNotificationToRecipient).toEqual({
      giftCard: null,
      userErrors: [
        {
          field: ['id'],
          message: 'Gift card does not exist',
        },
      ],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});

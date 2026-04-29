import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { AppConfig } from '../../src/config.js';
import { createApp, resetSyntheticIdentity, store } from '../support/runtime.js';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://billing-test.myshopify.com',
  readMode: 'passthrough',
};

describe('app billing, access, and delegated token local staging', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('stages billing purchases, subscriptions, line item updates, usage records, cancellation, reads, and meta state locally', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('app billing roots must not proxy'));
    const app = createApp(config);
    const agent = request(app.callback());

    const createResponse = await agent
      .post('/admin/api/2026-04/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({
        query: `#graphql
          mutation CreateBilling($lineItems: [AppSubscriptionLineItemInput!]!) {
            appSubscriptionCreate(
              name: "Local plan"
              returnUrl: "https://app.example.test/return"
              trialDays: 7
              test: true
              lineItems: $lineItems
            ) {
              confirmationUrl
              appSubscription {
                id
                name
                status
                test
                trialDays
                lineItems {
                  id
                  plan {
                    pricingDetails {
                      __typename
                      ... on AppUsagePricing {
                        cappedAmount { amount currencyCode }
                        balanceUsed { amount currencyCode }
                        interval
                        terms
                      }
                    }
                  }
                }
              }
              userErrors { field message }
            }
          }
        `,
        variables: {
          lineItems: [
            {
              plan: {
                appUsagePricingDetails: {
                  cappedAmount: { amount: 100, currencyCode: 'USD' },
                  terms: 'usage terms',
                },
              },
            },
          ],
        },
      });

    expect(createResponse.status).toBe(200);
    expect(fetchSpy).not.toHaveBeenCalled();
    const subscription = createResponse.body.data.appSubscriptionCreate.appSubscription;
    expect(createResponse.body.data.appSubscriptionCreate.confirmationUrl).toContain(
      'signature=shopify-draft-proxy-local-redacted',
    );
    expect(subscription).toMatchObject({
      name: 'Local plan',
      status: 'PENDING',
      test: true,
      trialDays: 7,
    });
    expect(subscription.lineItems[0].plan.pricingDetails).toMatchObject({
      __typename: 'AppUsagePricing',
      cappedAmount: { amount: '100', currencyCode: 'USD' },
      balanceUsed: { amount: '0.0', currencyCode: 'USD' },
      terms: 'usage terms',
    });

    const lineItemId = subscription.lineItems[0].id;
    const oneTimeResponse = await agent
      .post('/admin/api/2026-04/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({
        query: `#graphql
          mutation CreateOneTime {
            appPurchaseOneTimeCreate(
              name: "Import package"
              returnUrl: "https://app.example.test/return"
              price: { amount: 10, currencyCode: USD }
              test: true
            ) {
              confirmationUrl
              appPurchaseOneTime { id name status test price { amount currencyCode } }
              userErrors { field message }
            }
          }
        `,
      });
    expect(oneTimeResponse.status).toBe(200);
    expect(oneTimeResponse.body.data.appPurchaseOneTimeCreate.appPurchaseOneTime).toMatchObject({
      name: 'Import package',
      status: 'PENDING',
      test: true,
      price: { amount: '10', currencyCode: 'USD' },
    });

    const updateResponse = await agent
      .post('/admin/api/2026-04/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({
        query: `#graphql
          mutation UpdateLineItem($id: ID!) {
            appSubscriptionLineItemUpdate(id: $id, cappedAmount: { amount: 150, currencyCode: USD }) {
              confirmationUrl
              appSubscription { id }
              userErrors { field message }
            }
          }
        `,
        variables: { id: lineItemId },
      });
    expect(updateResponse.body.data.appSubscriptionLineItemUpdate).toMatchObject({
      appSubscription: { id: subscription.id },
      userErrors: [],
    });

    const usageResponse = await agent
      .post('/admin/api/2026-04/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({
        query: `#graphql
          mutation Usage($id: ID!) {
            appUsageRecordCreate(
              subscriptionLineItemId: $id
              price: { amount: 12.5, currencyCode: USD }
              description: "metered import"
              idempotencyKey: "usage-key-1"
            ) {
              appUsageRecord { id description price { amount currencyCode } subscriptionLineItem { id } }
              userErrors { field message }
            }
          }
        `,
        variables: { id: lineItemId },
      });
    expect(usageResponse.body.data.appUsageRecordCreate.appUsageRecord).toMatchObject({
      description: 'metered import',
      price: { amount: '12.5', currencyCode: 'USD' },
      subscriptionLineItem: { id: lineItemId },
    });

    await agent
      .post('/admin/api/2026-04/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({
        query: `mutation Extend($id: ID!) {
          appSubscriptionTrialExtend(id: $id, days: 3) {
            appSubscription { id trialDays }
            userErrors { field message }
          }
        }`,
        variables: { id: subscription.id },
      })
      .expect(200);

    const cancelResponse = await agent
      .post('/admin/api/2026-04/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({
        query: `mutation Cancel($id: ID!) {
          appSubscriptionCancel(id: $id, prorate: true) {
            appSubscription { id status trialDays }
            userErrors { field message }
          }
        }`,
        variables: { id: subscription.id },
      });
    expect(cancelResponse.body.data.appSubscriptionCancel.appSubscription).toMatchObject({
      id: subscription.id,
      status: 'CANCELLED',
      trialDays: 10,
    });

    const readResponse = await agent
      .post('/admin/api/2026-04/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({
        query: `#graphql
          query CurrentBilling {
            currentAppInstallation {
              id
              activeSubscriptions { id }
              allSubscriptions(first: 5) {
                nodes {
                  id
                  status
                  trialDays
                  lineItems {
                    id
                    usageRecords(first: 5) {
                      nodes { description price { amount currencyCode } }
                    }
                  }
                }
              }
              oneTimePurchases(first: 5) {
                nodes { name status price { amount currencyCode } }
              }
            }
          }
        `,
      });
    expect(readResponse.body.data.currentAppInstallation.activeSubscriptions).toEqual([]);
    expect(readResponse.body.data.currentAppInstallation.allSubscriptions.nodes[0]).toMatchObject({
      id: subscription.id,
      status: 'CANCELLED',
      trialDays: 10,
      lineItems: [
        {
          id: lineItemId,
          usageRecords: {
            nodes: [{ description: 'metered import', price: { amount: '12.5', currencyCode: 'USD' } }],
          },
        },
      ],
    });
    expect(readResponse.body.data.currentAppInstallation.oneTimePurchases.nodes[0]).toMatchObject({
      name: 'Import package',
      status: 'PENDING',
    });

    const metaState = await agent.get('/__meta/state').expect(200);
    expect(Object.keys(metaState.body.stagedState.appSubscriptions)).toContain(subscription.id);
    expect(metaState.body.stagedState.appUsageRecords).toMatchObject({
      [usageResponse.body.data.appUsageRecordCreate.appUsageRecord.id]: {
        idempotencyKey: 'usage-key-1',
      },
    });

    const metaLog = await agent.get('/__meta/log').expect(200);
    expect(metaLog.body.entries).toHaveLength(6);
    expect(metaLog.body.entries.every((entry: { status: string }) => entry.status === 'staged')).toBe(true);
    expect(metaLog.body.entries[0].requestBody.query).toContain('appSubscriptionCreate');
  });

  it('stages access-scope revocation and uninstall read effects without upstream writes', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('app access roots must not proxy'));
    const app = createApp(config);
    const agent = request(app.callback());

    const revokeResponse = await agent
      .post('/admin/api/2026-04/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({
        query: `#graphql
          mutation Revoke {
            appRevokeAccessScopes(scopes: ["write_products", "write_orders"]) {
              revoked { handle description }
              userErrors { field message code }
            }
          }
        `,
      });
    expect(fetchSpy).not.toHaveBeenCalled();
    expect(revokeResponse.body.data.appRevokeAccessScopes.revoked).toEqual([
      { handle: 'write_products', description: null },
    ]);
    expect(revokeResponse.body.data.appRevokeAccessScopes.userErrors).toEqual([
      {
        field: ['scopes'],
        message: "Access scope 'write_orders' is not granted.",
        code: 'UNKNOWN_SCOPES',
      },
    ]);

    const scopeRead = await agent
      .post('/admin/api/2026-04/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({
        query: `query Scopes { currentAppInstallation { accessScopes { handle } } }`,
      });
    expect(
      scopeRead.body.data.currentAppInstallation.accessScopes.map((scope: { handle: string }) => scope.handle),
    ).toEqual(['read_products']);

    const uninstallResponse = await agent
      .post('/admin/api/2026-04/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({
        query: `mutation Uninstall { appUninstall { app { id handle } userErrors { field message } } }`,
      });
    expect(uninstallResponse.body.data.appUninstall.app.handle).toBe('shopify-draft-proxy');

    const afterUninstallRead = await agent
      .post('/admin/api/2026-04/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({
        query: `query AfterUninstall { currentAppInstallation { id } }`,
      });
    expect(afterUninstallRead.body.data.currentAppInstallation).toBeNull();
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('returns delegated tokens once while storing only hashes/previews in meta-visible state', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('delegate token roots must not proxy'));
    const app = createApp(config);
    const agent = request(app.callback());

    const createResponse = await agent
      .post('/admin/api/2026-04/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({
        query: `#graphql
          mutation Delegate {
            delegateAccessTokenCreate(input: { delegateAccessScope: "read_products", expiresIn: 3600 }) {
              delegateAccessToken {
                accessToken
                accessScopes
                createdAt
                expiresIn
              }
              userErrors { field message code }
            }
          }
        `,
      });
    expect(fetchSpy).not.toHaveBeenCalled();
    const accessToken = createResponse.body.data.delegateAccessTokenCreate.delegateAccessToken.accessToken;
    expect(accessToken).toMatch(/^shpat_delegate_proxy_/u);
    expect(createResponse.body.data.delegateAccessTokenCreate.delegateAccessToken).toMatchObject({
      accessScopes: ['read_products'],
      expiresIn: 3600,
    });

    const metaState = await agent.get('/__meta/state').expect(200);
    expect(JSON.stringify(metaState.body)).not.toContain(accessToken);
    expect(JSON.stringify(metaState.body)).toContain('[redacted]');

    const destroyResponse = await agent
      .post('/admin/api/2026-04/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({
        query: `mutation Destroy($token: String!) {
          delegateAccessTokenDestroy(accessToken: $token) {
            status
            userErrors { field message code }
          }
        }`,
        variables: { token: accessToken },
      });
    expect(destroyResponse.body.data.delegateAccessTokenDestroy).toEqual({
      status: true,
      userErrors: [],
    });

    const secondDestroyResponse = await agent
      .post('/admin/api/2026-04/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({
        query: `mutation Destroy($token: String!) {
          delegateAccessTokenDestroy(accessToken: $token) {
            status
            userErrors { field message code }
          }
        }`,
        variables: { token: accessToken },
      });
    expect(secondDestroyResponse.body.data.delegateAccessTokenDestroy).toEqual({
      status: false,
      userErrors: [
        {
          field: ['accessToken'],
          message: 'Access token not found.',
          code: 'ACCESS_TOKEN_NOT_FOUND',
        },
      ],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});

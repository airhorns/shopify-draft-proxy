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

const query = `#graphql
  query CashManagementLocationSummaryProbe($locationId: ID!, $startDate: Date!, $endDate: Date!) {
    cashManagementLocationSummary(locationId: $locationId, startDate: $startDate, endDate: $endDate) {
      cashBalanceAtStart {
        amount
        currencyCode
      }
      cashBalanceAtEnd {
        amount
        currencyCode
      }
      netCash {
        amount
        currencyCode
      }
      totalDiscrepancies {
        amount
        currencyCode
      }
      sessionsOpened
      sessionsClosed
    }
  }
`;

describe('cashManagementLocationSummary query shapes', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('mirrors the captured access-denied branch in snapshot mode without inventing cash totals', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('cashManagementLocationSummary should resolve locally in snapshot mode');
    });

    const app = createApp(config).callback();
    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query,
        variables: {
          locationId: 'gid://shopify/Location/106318463282',
          startDate: '2026-04-01',
          endDate: '2026-04-25',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: null,
      errors: [
        {
          message:
            'Access denied for cashManagementLocationSummary field. Required access: `read_cash_tracking` access scope. Also: User must have `view_payment_tracking_sessions_pos_channel` or `payments_cash_session_history` retail role permission.',
          locations: [
            {
              line: 3,
              column: 5,
            },
          ],
          extensions: {
            code: 'ACCESS_DENIED',
            documentation: 'https://shopify.dev/api/usage/access-scopes',
            requiredAccess:
              '`read_cash_tracking` access scope. Also: User must have `view_payment_tracking_sessions_pos_channel` or `payments_cash_session_history` retail role permission.',
          },
          path: ['cashManagementLocationSummary'],
        },
      ],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});

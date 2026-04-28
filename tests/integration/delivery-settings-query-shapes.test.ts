import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../support/runtime.js';
import type { AppConfig } from '../../src/config.js';
import { store } from '../support/runtime.js';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

describe('delivery settings query shapes', () => {
  beforeEach(() => {
    store.reset();
    vi.restoreAllMocks();
  });

  it('serves captured empty delivery settings in snapshot mode without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('delivery settings snapshot reads must stay local');
    });

    const app = createApp(config).callback();

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `#graphql
          query DeliverySettingsRead {
            deliverySettings {
              __typename
              legacyModeProfiles
              legacyModeBlocked {
                __typename
                blocked
                reasons
              }
            }
            deliveryPromiseSettings {
              __typename
              deliveryDatesEnabled
              processingTime
            }
          }
        `,
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        deliverySettings: {
          __typename: 'DeliverySetting',
          legacyModeProfiles: false,
          legacyModeBlocked: {
            __typename: 'DeliveryLegacyModeBlocked',
            blocked: false,
            reasons: null,
          },
        },
        deliveryPromiseSettings: {
          __typename: 'DeliveryPromiseSetting',
          deliveryDatesEnabled: false,
          processingTime: null,
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});

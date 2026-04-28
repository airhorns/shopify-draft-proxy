import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { resetSyntheticIdentity } from '../../src/state/synthetic-identity.js';
import { store } from '../../src/state/store.js';
import type { CarrierServiceRecord, LocationRecord, ShippingPackageRecord } from '../../src/state/types.js';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

function makeLocation(overrides: Partial<LocationRecord>): LocationRecord {
  return {
    id: 'gid://shopify/Location/1',
    name: 'Shop location',
    isActive: true,
    isFulfillmentService: false,
    ...overrides,
  };
}

function makeCarrierService(overrides: Partial<CarrierServiceRecord>): CarrierServiceRecord {
  return {
    id: 'gid://shopify/DeliveryCarrierService/1',
    name: 'Hermes Carrier',
    formattedName: 'Hermes Carrier (Rates provided by app)',
    callbackUrl: 'https://mock.shop/rates',
    active: true,
    supportsServiceDiscovery: true,
    createdAt: '2026-04-27T00:00:00.000Z',
    updatedAt: '2026-04-27T00:00:00.000Z',
    ...overrides,
  };
}

function makeShippingPackage(overrides: Partial<ShippingPackageRecord>): ShippingPackageRecord {
  return {
    id: 'gid://shopify/ShippingPackage/1',
    name: 'Starter box',
    type: 'BOX',
    default: false,
    weight: { value: 1, unit: 'KILOGRAMS' },
    dimensions: { length: 10, width: 8, height: 4, unit: 'CENTIMETERS' },
    createdAt: '2026-04-27T00:00:00.000Z',
    updatedAt: '2026-04-27T00:00:00.000Z',
    ...overrides,
  };
}

describe('shipping settings local staging', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('serves carrier/location availability reads and stages local-pickup read-after-write locally', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('shipping settings should resolve locally in snapshot mode');
    });
    store.upsertBaseLocations([
      makeLocation({ id: 'gid://shopify/Location/10', name: 'Alpha Warehouse' }),
      makeLocation({ id: 'gid://shopify/Location/20', name: 'Beta Retail' }),
      makeLocation({ id: 'gid://shopify/Location/30', name: 'Inactive Warehouse', isActive: false }),
    ]);
    store.upsertBaseCarrierServices([
      makeCarrierService({ id: 'gid://shopify/DeliveryCarrierService/10', name: 'Active Carrier' }),
      makeCarrierService({ id: 'gid://shopify/DeliveryCarrierService/20', name: 'Inactive Carrier', active: false }),
    ]);
    const app = createApp(config).callback();

    const availabilityResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `
          query ShippingAvailability {
            availableCarrierServices {
              carrierService { id name active }
              locations { id name localPickupSettingsV2 { pickupTime instructions } }
            }
            locationsAvailableForDeliveryProfilesConnection(first: 2) {
              nodes { id name localPickupSettingsV2 { pickupTime instructions } }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
          }
        `,
      });

    expect(availabilityResponse.status).toBe(200);
    expect(availabilityResponse.body.data.availableCarrierServices).toEqual([
      {
        carrierService: {
          id: 'gid://shopify/DeliveryCarrierService/10',
          name: 'Active Carrier',
          active: true,
        },
        locations: [
          { id: 'gid://shopify/Location/10', name: 'Alpha Warehouse', localPickupSettingsV2: null },
          { id: 'gid://shopify/Location/20', name: 'Beta Retail', localPickupSettingsV2: null },
        ],
      },
    ]);
    expect(availabilityResponse.body.data.locationsAvailableForDeliveryProfilesConnection.nodes).toEqual([
      { id: 'gid://shopify/Location/10', name: 'Alpha Warehouse', localPickupSettingsV2: null },
      { id: 'gid://shopify/Location/20', name: 'Beta Retail', localPickupSettingsV2: null },
    ]);
    expect(availabilityResponse.body.data.locationsAvailableForDeliveryProfilesConnection.pageInfo).toMatchObject({
      hasNextPage: false,
      hasPreviousPage: false,
      startCursor: 'cursor:gid://shopify/Location/10',
      endCursor: 'cursor:gid://shopify/Location/20',
    });

    const enableResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `
          mutation EnablePickup($settings: DeliveryLocationLocalPickupEnableInput!) {
            locationLocalPickupEnable(localPickupSettings: $settings) {
              localPickupSettings { pickupTime instructions }
              userErrors { field message code }
            }
          }
        `,
        variables: {
          settings: {
            locationId: 'gid://shopify/Location/20',
            pickupTime: 'TWO_HOURS',
            instructions: 'Bring photo ID.',
          },
        },
      });

    expect(enableResponse.status).toBe(200);
    expect(enableResponse.body.data.locationLocalPickupEnable).toEqual({
      localPickupSettings: {
        pickupTime: 'TWO_HOURS',
        instructions: 'Bring photo ID.',
      },
      userErrors: [],
    });

    const readAfterEnable = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `
          query PickupRead($id: ID!) {
            location(id: $id) { id localPickupSettingsV2 { pickupTime instructions } }
          }
        `,
        variables: { id: 'gid://shopify/Location/20' },
      });

    expect(readAfterEnable.body.data.location).toEqual({
      id: 'gid://shopify/Location/20',
      localPickupSettingsV2: {
        pickupTime: 'TWO_HOURS',
        instructions: 'Bring photo ID.',
      },
    });

    const downstreamAvailabilityResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `
          query PickupDownstream {
            availableCarrierServices {
              carrierService { id }
              locations { id localPickupSettingsV2 { pickupTime instructions } }
            }
            locationsAvailableForDeliveryProfilesConnection(first: 5) {
              nodes { id localPickupSettingsV2 { pickupTime instructions } }
            }
          }
        `,
      });

    expect(downstreamAvailabilityResponse.body.data.availableCarrierServices).toEqual([
      {
        carrierService: { id: 'gid://shopify/DeliveryCarrierService/10' },
        locations: [
          { id: 'gid://shopify/Location/10', localPickupSettingsV2: null },
          {
            id: 'gid://shopify/Location/20',
            localPickupSettingsV2: {
              pickupTime: 'TWO_HOURS',
              instructions: 'Bring photo ID.',
            },
          },
        ],
      },
    ]);
    expect(downstreamAvailabilityResponse.body.data.locationsAvailableForDeliveryProfilesConnection.nodes).toEqual([
      { id: 'gid://shopify/Location/10', localPickupSettingsV2: null },
      {
        id: 'gid://shopify/Location/20',
        localPickupSettingsV2: {
          pickupTime: 'TWO_HOURS',
          instructions: 'Bring photo ID.',
        },
      },
    ]);

    const disableResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `
          mutation DisablePickup($id: ID!) {
            locationLocalPickupDisable(locationId: $id) {
              locationId
              userErrors { field message code }
            }
          }
        `,
        variables: { id: 'gid://shopify/Location/20' },
      });

    expect(disableResponse.body.data.locationLocalPickupDisable).toEqual({
      locationId: 'gid://shopify/Location/20',
      userErrors: [],
    });

    const readAfterDisableResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `
          query PickupDownstreamAfterDisable {
            availableCarrierServices {
              locations { id localPickupSettingsV2 { pickupTime instructions } }
            }
            locationsAvailableForDeliveryProfilesConnection(first: 5) {
              nodes { id localPickupSettingsV2 { pickupTime instructions } }
            }
          }
        `,
      });

    expect(readAfterDisableResponse.body.data.availableCarrierServices[0].locations).toEqual([
      { id: 'gid://shopify/Location/10', localPickupSettingsV2: null },
      { id: 'gid://shopify/Location/20', localPickupSettingsV2: null },
    ]);
    expect(readAfterDisableResponse.body.data.locationsAvailableForDeliveryProfilesConnection.nodes).toEqual([
      { id: 'gid://shopify/Location/10', localPickupSettingsV2: null },
      { id: 'gid://shopify/Location/20', localPickupSettingsV2: null },
    ]);

    const unknownEnableResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `
          mutation EnableMissingPickup($settings: DeliveryLocationLocalPickupEnableInput!) {
            locationLocalPickupEnable(localPickupSettings: $settings) {
              localPickupSettings { pickupTime instructions }
              userErrors { field message code }
            }
          }
        `,
        variables: {
          settings: {
            locationId: 'gid://shopify/Location/999999999999',
            pickupTime: 'ONE_HOUR',
          },
        },
      });

    expect(unknownEnableResponse.body.data.locationLocalPickupEnable).toEqual({
      localPickupSettings: null,
      userErrors: [
        {
          field: ['localPickupSettings'],
          message: 'Unable to find an active location for location ID 999999999999',
          code: 'ACTIVE_LOCATION_NOT_FOUND',
        },
      ],
    });

    expect(fetchSpy).not.toHaveBeenCalled();
    expect(store.getLog().map((entry) => entry.operationName)).toEqual([
      'locationLocalPickupEnable',
      'locationLocalPickupDisable',
    ]);
  });

  it('stages shipping package update, default selection, and deletion in local meta state', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('shipping package mutations should resolve locally in snapshot mode');
    });
    store.upsertBaseShippingPackages([
      makeShippingPackage({ id: 'gid://shopify/ShippingPackage/1', default: true }),
      makeShippingPackage({ id: 'gid://shopify/ShippingPackage/2', name: 'Backup mailer', default: false }),
    ]);
    const app = createApp(config).callback();

    const updateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `
          mutation UpdatePackage($id: ID!, $package: CustomShippingPackageInput!) {
            shippingPackageUpdate(id: $id, shippingPackage: $package) {
              userErrors { field message }
            }
          }
        `,
        variables: {
          id: 'gid://shopify/ShippingPackage/1',
          package: {
            name: 'Updated box',
            type: 'BOX',
            default: true,
            weight: { value: 2.5, unit: 'POUNDS' },
            dimensions: { length: 12, width: 9, height: 5, unit: 'INCHES' },
          },
        },
      });

    expect(updateResponse.status).toBe(200);
    expect(updateResponse.body.data.shippingPackageUpdate).toEqual({ userErrors: [] });

    const defaultResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `
          mutation MakeDefault($id: ID!) {
            shippingPackageMakeDefault(id: $id) {
              userErrors { field message }
            }
          }
        `,
        variables: { id: 'gid://shopify/ShippingPackage/2' },
      });

    expect(defaultResponse.body.data.shippingPackageMakeDefault).toEqual({ userErrors: [] });

    const deleteResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `
          mutation DeletePackage($id: ID!) {
            shippingPackageDelete(id: $id) {
              deletedId
              userErrors { field message }
            }
          }
        `,
        variables: { id: 'gid://shopify/ShippingPackage/1' },
      });

    expect(deleteResponse.body.data.shippingPackageDelete).toEqual({
      deletedId: 'gid://shopify/ShippingPackage/1',
      userErrors: [],
    });

    const stateResponse = await request(app).get('/__meta/state');
    expect(stateResponse.body.stagedState.shippingPackages['gid://shopify/ShippingPackage/2']).toMatchObject({
      id: 'gid://shopify/ShippingPackage/2',
      default: true,
    });
    expect(stateResponse.body.stagedState.deletedShippingPackageIds).toEqual({
      'gid://shopify/ShippingPackage/1': true,
    });

    expect(fetchSpy).not.toHaveBeenCalled();
    expect(store.getLog().map((entry) => entry.operationName)).toEqual([
      'shippingPackageUpdate',
      'shippingPackageMakeDefault',
      'shippingPackageDelete',
    ]);
    expect(store.getLog().every((entry) => entry.status === 'staged')).toBe(true);
  });
});

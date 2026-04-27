import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

const repoRoot = resolve(import.meta.dirname, '../..');
const fixturePath = resolve(
  repoRoot,
  'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/app-billing-access-read.json',
);

type JsonObject = Record<string, unknown>;

function asObject(value: unknown): JsonObject {
  expect(value).toEqual(expect.any(Object));
  return value as JsonObject;
}

function readFixture() {
  return JSON.parse(readFileSync(fixturePath, 'utf8')) as JsonObject;
}

describe('app billing conformance fixture', () => {
  it('records current app installation and billing no-data shapes', () => {
    const fixture = readFixture();
    const currentInstallation = asObject(fixture['currentInstallation']);
    expect(currentInstallation['status']).toBe(200);

    const payload = asObject(currentInstallation['payload']);
    const data = asObject(payload['data']);
    const currentAppInstallation = asObject(data['currentAppInstallation']);
    expect(currentAppInstallation['id']).toMatch(/^gid:\/\/shopify\/AppInstallation\//u);
    expect(currentAppInstallation['accessScopes']).toEqual(
      expect.arrayContaining([expect.objectContaining({ handle: 'read_products' })]),
    );
    expect(asObject(currentAppInstallation['app'])['id']).toMatch(/^gid:\/\/shopify\/App\//u);
    expect(currentAppInstallation['activeSubscriptions']).toEqual([]);
    expect(currentAppInstallation['allSubscriptions']).toMatchObject({
      nodes: [],
      edges: [],
      pageInfo: {
        hasNextPage: false,
        hasPreviousPage: false,
        startCursor: null,
        endCursor: null,
      },
    });
    expect(currentAppInstallation['oneTimePurchases']).toMatchObject({
      nodes: [],
      edges: [],
      pageInfo: {
        hasNextPage: false,
        hasPreviousPage: false,
        startCursor: null,
        endCursor: null,
      },
    });
    expect(data['missingAppInstallation']).toBeNull();
  });

  it('records app lookup nullability and the current appInstallations access blocker', () => {
    const fixture = readFixture();
    const appLookupPayload = asObject(asObject(fixture['appLookups'])['payload']);
    const appLookupData = asObject(appLookupPayload['data']);
    const appById = asObject(appLookupData['appById']);
    expect(appById['id']).toBe(asObject(appLookupData['appByHandle'])['id']);
    expect(appById['id']).toBe(asObject(appLookupData['appByKey'])['id']);
    expect(appLookupData['missingAppById']).toBeNull();
    expect(appLookupData['missingAppByHandle']).toBeNull();
    expect(appLookupData['missingAppByKey']).toBeNull();

    const accessProbePayload = asObject(asObject(fixture['appInstallationsAccessProbe'])['payload']);
    expect(accessProbePayload['data']).toBeNull();
    expect(accessProbePayload['errors']).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          message: 'access denied',
          path: ['appInstallations'],
          extensions: expect.objectContaining({ code: 'ACCESS_DENIED' }),
        }),
      ]),
    );
  });
});

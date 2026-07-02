/* oxlint-disable no-console -- CLI scripts intentionally write capture status to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type GraphqlCapture = {
  query: string;
  variables: JsonRecord;
  result: ConformanceGraphqlResult;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'shipping-fulfillments');
const outputPath = path.join(outputDir, 'delivery-profile-create-disallowed-update-keys.json');

const deliveryProfileCreateMutation = `#graphql
  mutation DeliveryProfileCreateDisallowedUpdateKeys($profile: DeliveryProfileInput!) {
    deliveryProfileCreate(profile: $profile) {
      profile {
        id
        name
        version
        originLocationCount
        zoneCountryCount
        activeMethodDefinitionsCount
        profileLocationGroups {
          locationGroup {
            id
          }
          locationGroupZones(first: 5) {
            nodes {
              zone {
                id
                name
              }
              methodDefinitions(first: 5) {
                nodes {
                  id
                  name
                  active
                  rateProvider {
                    ... on DeliveryRateDefinition {
                      id
                      price {
                        amount
                        currencyCode
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const deliveryProfileRemoveMutation = `#graphql
  mutation DeliveryProfileCreateDisallowedUpdateKeysCleanup($id: ID!) {
    deliveryProfileRemove(id: $id) {
      job {
        id
        done
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const locationsHydrateQuery = `#graphql
  query ShippingDeliveryProfileLocationsHydrate {
    locationsAvailableForDeliveryProfilesConnection(first: 1) {
      nodes {
        id
        name
        isActive
        isFulfillmentService
      }
    }
  }
`;

function trimGraphql(query: string): string {
  return query.replace(/^#graphql\n/u, '').trim();
}

async function capture(query: string, variables: JsonRecord): Promise<GraphqlCapture> {
  return {
    query: trimGraphql(query),
    variables,
    result: await runGraphqlRequest(query, variables),
  };
}

function readObject(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readPath(value: unknown, pathParts: string[]): unknown {
  let current = value;
  for (const part of pathParts) {
    const object = readObject(current);
    current = object?.[part];
  }
  return current;
}

function assertHttpOk(label: string, result: ConformanceGraphqlResult): void {
  if (result.status < 200 || result.status >= 300) {
    throw new Error(`${label} expected HTTP 2xx, got ${result.status}: ${JSON.stringify(result.payload)}`);
  }
}

function userErrors(captureResult: GraphqlCapture): JsonRecord[] {
  const errors = readPath(captureResult.result.payload, ['data', 'deliveryProfileCreate', 'userErrors']);
  return Array.isArray(errors) ? errors.filter((error): error is JsonRecord => readObject(error) !== null) : [];
}

function assertFirstUserError(
  captureResult: GraphqlCapture,
  label: string,
  expectedField: unknown,
  expectedMessage: string,
): void {
  assertHttpOk(label, captureResult.result);
  const profile = readPath(captureResult.result.payload, ['data', 'deliveryProfileCreate', 'profile']);
  const errors = userErrors(captureResult);
  const first = errors[0];
  if (profile !== null || first === undefined) {
    throw new Error(
      `${label} expected null profile and a userError, got ${JSON.stringify(captureResult.result.payload)}`,
    );
  }
  if (JSON.stringify(first['field']) !== JSON.stringify(expectedField) || first['message'] !== expectedMessage) {
    throw new Error(
      `${label} expected ${JSON.stringify({ field: expectedField, message: expectedMessage })}, got ${JSON.stringify(first)}`,
    );
  }
}

function assertSuccessfulCreate(captureResult: GraphqlCapture): string {
  assertHttpOk('allowed methodDefinitionsToCreate', captureResult.result);
  const errors = userErrors(captureResult);
  const profile = readObject(readPath(captureResult.result.payload, ['data', 'deliveryProfileCreate', 'profile']));
  const id = profile?.['id'];
  if (errors.length !== 0 || typeof id !== 'string') {
    throw new Error(
      `allowed methodDefinitionsToCreate expected a profile and no userErrors, got ${JSON.stringify(captureResult.result.payload)}`,
    );
  }
  return id;
}

function firstUsableLocationId(hydrate: ConformanceGraphqlResult): string {
  assertHttpOk('locations hydrate', hydrate);
  const nodes = readPath(hydrate.payload, ['data', 'locationsAvailableForDeliveryProfilesConnection', 'nodes']);
  if (!Array.isArray(nodes)) {
    throw new Error(`locations hydrate expected nodes array, got ${JSON.stringify(hydrate.payload)}`);
  }
  for (const node of nodes) {
    const location = readObject(node);
    if (
      typeof location?.['id'] === 'string' &&
      location['isActive'] === true &&
      location['isFulfillmentService'] === false
    ) {
      return location['id'];
    }
  }
  throw new Error(`locations hydrate did not include an active merchant location: ${JSON.stringify(hydrate.payload)}`);
}

const locationsHydrate = await runGraphqlRequest(locationsHydrateQuery, {});
const locationId = firstUsableLocationId(locationsHydrate);

const variantsToDissociate = await capture(deliveryProfileCreateMutation, {
  profile: {
    name: 'Disallowed variants dissociate',
    variantsToDissociate: ['gid://shopify/ProductVariant/1'],
  },
});
assertFirstUserError(
  variantsToDissociate,
  'variantsToDissociate',
  null,
  'Cannot disassociate variants when creating a profile.',
);

const zonesToUpdate = await capture(deliveryProfileCreateMutation, {
  profile: {
    name: 'Disallowed zones update',
    locationGroupsToCreate: [
      {
        locations: [locationId],
        zonesToUpdate: [
          {
            id: 'gid://shopify/DeliveryZone/1',
            name: 'Renamed',
          },
        ],
      },
    ],
  },
});
assertFirstUserError(zonesToUpdate, 'zonesToUpdate', null, 'Cannot update zones when creating a profile.');

const methodDefinitionsToUpdate = await capture(deliveryProfileCreateMutation, {
  profile: {
    name: 'Disallowed method update',
    locationGroupsToCreate: [
      {
        locations: [locationId],
        zonesToCreate: [
          {
            name: 'Domestic',
            countries: [
              {
                code: 'US',
                includeAllProvinces: true,
              },
            ],
            methodDefinitionsToUpdate: [
              {
                id: 'gid://shopify/DeliveryMethodDefinition/1',
              },
            ],
          },
        ],
      },
    ],
  },
});
assertFirstUserError(
  methodDefinitionsToUpdate,
  'methodDefinitionsToUpdate',
  null,
  'Profile is invalid: Input cannot include method_definitions_to_update on create.',
);

const allowedMethodDefinitionsToCreate = await capture(deliveryProfileCreateMutation, {
  profile: {
    name: `Allowed method create ${Date.now()}`,
    locationGroupsToCreate: [
      {
        locations: [locationId],
        zonesToCreate: [
          {
            name: 'Domestic',
            countries: [
              {
                code: 'US',
                includeAllProvinces: true,
              },
            ],
            methodDefinitionsToCreate: [
              {
                name: 'Standard',
                active: true,
                rateDefinition: {
                  price: {
                    amount: '7.25',
                    currencyCode: 'USD',
                  },
                },
              },
            ],
          },
        ],
      },
    ],
  },
});
const createdProfileId = assertSuccessfulCreate(allowedMethodDefinitionsToCreate);
const cleanup = await capture(deliveryProfileRemoveMutation, { id: createdProfileId });

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      evidence: {
        locationUsed: locationId,
        deliveryProfileCreateUserErrorsType: 'UserError',
        userErrorFields: ['field', 'message'],
      },
      mutations: {
        variantsToDissociate,
        zonesToUpdate,
        methodDefinitionsToUpdate,
        allowedMethodDefinitionsToCreate,
      },
      cleanup: {
        removeCreatedProfile: cleanup,
      },
      notes: [
        'Captured with home-folder conformance auth against a disposable Shopify test store.',
        'The public Admin GraphQL 2026-04 schema exposes deliveryProfileCreate.userErrors as UserError with field/message only; these create-only guards return field: null with public messages, while runtime tests cover the proxy-local code convenience field.',
        'The allowed methodDefinitionsToCreate branch creates a disposable profile and removes it in cleanup.',
      ],
      upstreamCalls: [
        {
          operationName: 'ShippingDeliveryProfileLocationsHydrate',
          variables: {},
          query: trimGraphql(locationsHydrateQuery),
          response: {
            status: locationsHydrate.status,
            body: locationsHydrate.payload,
          },
        },
      ],
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(JSON.stringify({ ok: true, outputPath }, null, 2));

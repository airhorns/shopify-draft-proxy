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
const outputPath = path.join(outputDir, 'delivery-profile-update-validation.json');

const deliveryProfileCreateMutation = `#graphql
  mutation DeliveryProfileUpdateValidationCreate($profile: DeliveryProfileInput!) {
    deliveryProfileCreate(profile: $profile) {
      profile {
        id
        name
        version
        originLocationCount
        zoneCountryCount
        activeMethodDefinitionsCount
        profileItems(first: 5) {
          nodes {
            product {
              id
              title
            }
            variants(first: 5) {
              nodes {
                id
                title
              }
            }
          }
        }
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
                  methodConditions {
                    id
                    field
                    operator
                    conditionCriteria {
                      __typename
                      ... on Weight {
                        value
                        unit
                      }
                      ... on MoneyV2 {
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

const deliveryProfileUpdateMutation = `#graphql
  mutation DeliveryProfileUpdateValidation($id: ID!, $profile: DeliveryProfileInput!) {
    deliveryProfileUpdate(id: $id, profile: $profile) {
      profile {
        id
        name
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const deliveryProfileRemoveMutation = `#graphql
  mutation DeliveryProfileUpdateValidationCleanup($id: ID!) {
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
    locationsAvailableForDeliveryProfilesConnection(first: 2) {
      nodes {
        id
        name
        isActive
        isFulfillmentService
      }
    }
  }
`;

const variantsHydrateQuery = `#graphql
  query ShippingDeliveryProfileVariantsHydrate($ids: [ID!]!) {
    nodes(ids: $ids) {
      ... on ProductVariant {
        id
        title
        product {
          id
          title
          handle
        }
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

function readUpdateUserErrors(captureResult: GraphqlCapture): JsonRecord[] {
  const errors = readPath(captureResult.result.payload, ['data', 'deliveryProfileUpdate', 'userErrors']);
  return Array.isArray(errors) ? errors.filter((error): error is JsonRecord => readObject(error) !== null) : [];
}

function assertUpdateUserError(captureResult: GraphqlCapture, label: string, expectedMessage: string): void {
  assertHttpOk(label, captureResult.result);
  const profile = readPath(captureResult.result.payload, ['data', 'deliveryProfileUpdate', 'profile']);
  const first = readUpdateUserErrors(captureResult)[0];
  if (profile !== null || first === undefined || first['message'] !== expectedMessage) {
    throw new Error(
      `${label} expected null profile and userError message ${JSON.stringify(expectedMessage)}, got ${JSON.stringify(
        captureResult.result.payload,
      )}`,
    );
  }
}

function assertSuccessfulUpdate(captureResult: GraphqlCapture, label: string): void {
  assertHttpOk(label, captureResult.result);
  const profile = readObject(readPath(captureResult.result.payload, ['data', 'deliveryProfileUpdate', 'profile']));
  const errors = readUpdateUserErrors(captureResult);
  if (profile === null || errors.length !== 0) {
    throw new Error(`${label} expected profile and no userErrors, got ${JSON.stringify(captureResult.result.payload)}`);
  }
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

function assertSuccessfulCreate(captureResult: GraphqlCapture): string {
  assertHttpOk('base create', captureResult.result);
  const errors = readPath(captureResult.result.payload, ['data', 'deliveryProfileCreate', 'userErrors']);
  const profile = readObject(readPath(captureResult.result.payload, ['data', 'deliveryProfileCreate', 'profile']));
  const id = profile?.['id'];
  if ((Array.isArray(errors) && errors.length !== 0) || typeof id !== 'string') {
    throw new Error(
      `base create expected profile and no userErrors, got ${JSON.stringify(captureResult.result.payload)}`,
    );
  }
  return id;
}

function firstLocationGroupId(captureResult: GraphqlCapture): string {
  const groups = readPath(captureResult.result.payload, [
    'data',
    'deliveryProfileCreate',
    'profile',
    'profileLocationGroups',
  ]);
  const first = Array.isArray(groups) ? readObject(groups[0]) : null;
  const locationGroup = readObject(first?.['locationGroup']);
  const id = locationGroup?.['id'];
  if (typeof id !== 'string') {
    throw new Error(`base create expected a location group id, got ${JSON.stringify(captureResult.result.payload)}`);
  }
  return id;
}

const locationsHydrate = await runGraphqlRequest(locationsHydrateQuery, {});
const locationId = firstUsableLocationId(locationsHydrate);
const variantIds: string[] = [];
const variantsHydrate = await runGraphqlRequest(variantsHydrateQuery, { ids: variantIds });

const baseCreate = await capture(deliveryProfileCreateMutation, {
  profile: {
    name: `Update validation base ${Date.now()}`,
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
          },
        ],
      },
    ],
  },
});
const profileId = assertSuccessfulCreate(baseCreate);
const locationGroupId = firstLocationGroupId(baseCreate);

let cleanup: GraphqlCapture | undefined;
try {
  const longName = await capture(deliveryProfileUpdateMutation, {
    id: profileId,
    profile: {
      name: 'x'.repeat(300),
    },
  });
  assertUpdateUserError(longName, 'long name update', 'Profile name must be less than 128 characters long');

  const unknownCreateLocation = await capture(deliveryProfileUpdateMutation, {
    id: profileId,
    profile: {
      locationGroupsToCreate: [
        {
          locations: ['gid://shopify/Location/999999999'],
        },
      ],
    },
  });
  assertUpdateUserError(
    unknownCreateLocation,
    'unknown create location update',
    'The Location could not be found for this shop.',
  );

  const unknownAddLocation = await capture(deliveryProfileUpdateMutation, {
    id: profileId,
    profile: {
      locationGroupsToUpdate: [
        {
          id: locationGroupId,
          locationsToAdd: ['gid://shopify/Location/999999999'],
        },
      ],
    },
  });
  assertUpdateUserError(
    unknownAddLocation,
    'unknown add location update',
    'The Location could not be found for this shop.',
  );

  const unknownProfileGroupAddLocationProbe = await capture(deliveryProfileUpdateMutation, {
    id: profileId,
    profile: {
      profileLocationGroups: [
        {
          id: locationGroupId,
          locationsToAdd: ['gid://shopify/Location/999999999'],
        },
      ],
    },
  });
  assertSuccessfulUpdate(
    unknownProfileGroupAddLocationProbe,
    'unknown profile location group add location update probe',
  );

  const emptyCountries = await capture(deliveryProfileUpdateMutation, {
    id: profileId,
    profile: {
      locationGroupsToCreate: [
        {
          locations: [locationId],
          zonesToCreate: [
            {
              name: 'Empty countries',
              countries: [],
            },
          ],
        },
      ],
    },
  });
  assertUpdateUserError(
    emptyCountries,
    'empty countries update',
    'Profile is invalid: cannot create LocationGroupZone without countries.',
  );

  const overlappingZonesProbe = await capture(deliveryProfileUpdateMutation, {
    id: profileId,
    profile: {
      locationGroupsToCreate: [
        {
          locations: [locationId],
          zonesToCreate: [
            {
              name: 'One',
              countries: [
                {
                  code: 'US',
                  includeAllProvinces: true,
                },
              ],
            },
            {
              name: 'Two',
              countries: [
                {
                  code: 'US',
                  includeAllProvinces: true,
                },
              ],
            },
          ],
        },
      ],
    },
  });
  assertSuccessfulUpdate(overlappingZonesProbe, 'overlapping zones update probe');

  cleanup = await capture(deliveryProfileRemoveMutation, { id: profileId });

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
          deliveryProfileUpdateUserErrorsType: 'UserError',
          userErrorFields: ['field', 'message'],
          codeSelectionError: "Field 'code' doesn't exist on type 'UserError'",
        },
        mutations: {
          baseCreate,
          longName,
          unknownCreateLocation,
          unknownAddLocation,
          unknownProfileGroupAddLocationProbe,
          emptyCountries,
          overlappingZonesProbe,
        },
        cleanup: {
          removeCreatedProfile: cleanup,
        },
        notes: [
          'Captured with SHOPIFY_CONFORMANCE_API_VERSION=2026-04 and home-folder conformance auth.',
          'The capture creates one disposable delivery profile, records update-side validation branches against it, then removes the profile in cleanup.',
          'The public Admin GraphQL 2026-04 schema exposes deliveryProfileUpdate.userErrors as UserError with field/message only; runtime tests cover the proxy-local code convenience field.',
          'A profileLocationGroups.locationsToAdd unknown-location update probe was accepted by public Admin GraphQL 2026-04; local runtime tests still cover the ticket-required guardrail.',
          'A duplicate-country zonesToCreate update probe was accepted by public Admin GraphQL 2026-04; local runtime tests still cover the ticket-required overlapping-zone guardrail.',
        ],
        upstreamCalls: [
          {
            operationName: 'ShippingDeliveryProfileLocationsHydrate',
            variables: {},
            query: 'sha:hand-synthesized',
            response: {
              status: locationsHydrate.status,
              body: locationsHydrate.payload,
            },
          },
          {
            operationName: 'ShippingDeliveryProfileVariantsHydrate',
            variables: { ids: variantIds },
            query: 'sha:hand-synthesized',
            response: {
              status: variantsHydrate.status,
              body: variantsHydrate.payload,
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
} finally {
  if (cleanup === undefined) {
    const cleanupAfterFailure = await capture(deliveryProfileRemoveMutation, { id: profileId });
    console.log(JSON.stringify({ ok: false, cleanupAfterFailure }, null, 2));
  }
}

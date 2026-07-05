/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'shipping-fulfillments');
const createValidationOutputPath = path.join(outputDir, 'delivery-profile-create-validation.json');
const writesOutputPath = path.join(outputDir, 'delivery-profile-writes.json');
const readOutputPath = path.join(outputDir, 'delivery-profiles-read.json');

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const catalogQuery = `#graphql
  query DeliveryProfilesCatalog($first: Int, $last: Int, $after: String, $before: String, $reverse: Boolean, $merchantOwnedOnly: Boolean) {
    deliveryProfiles(
      first: $first
      last: $last
      after: $after
      before: $before
      reverse: $reverse
      merchantOwnedOnly: $merchantOwnedOnly
    ) {
      edges {
        cursor
        node {
          id
          name
          default
          version
          activeMethodDefinitionsCount
          locationsWithoutRatesCount
          originLocationCount
          zoneCountryCount
          productVariantsCount {
            count
            precision
          }
        }
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
  }
`;

const detailQuery = `#graphql
  query DeliveryProfileDetail($id: ID!) {
    deliveryProfile(id: $id) {
      id
      name
      default
      version
      activeMethodDefinitionsCount
      locationsWithoutRatesCount
      originLocationCount
      zoneCountryCount
      productVariantsCount {
        count
        precision
      }
      sellingPlanGroups(first: 2) {
        nodes {
          id
          name
        }
        pageInfo {
          hasNextPage
          hasPreviousPage
          startCursor
          endCursor
        }
      }
      profileItems(first: 2) {
        nodes {
          product {
            id
            title
          }
          variants(first: 2) {
            nodes {
              id
              title
            }
            pageInfo {
              hasNextPage
              hasPreviousPage
              startCursor
              endCursor
            }
          }
        }
        pageInfo {
          hasNextPage
          hasPreviousPage
          startCursor
          endCursor
        }
      }
      profileLocationGroups {
        countriesInAnyZone {
          zone
          country {
            id
            name
            translatedName
            code {
              countryCode
              restOfWorld
            }
            provinces {
              id
              name
              code
            }
          }
        }
        locationGroup {
          id
          locations(first: 2) {
            nodes {
              id
              name
            }
            pageInfo {
              hasNextPage
              hasPreviousPage
              startCursor
              endCursor
            }
          }
          locationsCount {
            count
            precision
          }
        }
        locationGroupZones(first: 2) {
          nodes {
            zone {
              id
              name
              countries {
                id
                name
                translatedName
                code {
                  countryCode
                  restOfWorld
                }
                provinces {
                  id
                  name
                  code
                }
              }
            }
            methodDefinitions(first: 2) {
              nodes {
                id
                name
                active
                description
                rateProvider {
                  ... on DeliveryRateDefinition {
                    id
                    price {
                      amount
                      currencyCode
                    }
                  }
                  ... on DeliveryParticipant {
                    id
                    fixedFee {
                      amount
                      currencyCode
                    }
                    percentageOfRateFee
                  }
                }
                methodConditions {
                  id
                  field
                  operator
                  conditionCriteria {
                    __typename
                    ... on MoneyV2 {
                      amount
                      currencyCode
                    }
                    ... on Weight {
                      unit
                      value
                    }
                  }
                }
              }
              pageInfo {
                hasNextPage
                hasPreviousPage
                startCursor
                endCursor
              }
            }
          }
          pageInfo {
            hasNextPage
            hasPreviousPage
            startCursor
            endCursor
          }
        }
      }
      unassignedLocationsPaginated(first: 2) {
        nodes {
          id
          name
        }
        pageInfo {
          hasNextPage
          hasPreviousPage
          startCursor
          endCursor
        }
      }
      unassignedLocations {
        id
        name
      }
    }
  }
`;

const locationsHydrateQuery = `#graphql
  query ShippingDeliveryProfileLocationsHydrate {
    locationsAvailableForDeliveryProfilesConnection(first: 3) {
      nodes {
        id
        name
        isActive
        isFulfillmentService
      }
    }
  }
`;

const defaultProfileHydrateQuery =
  'query ShippingDeliveryProfileHydrate($id: ID!) { deliveryProfile(id: $id) { id name default version } }';

type JsonRecord = Record<string, unknown>;

type GraphqlCapture = {
  query: string;
  variables: JsonRecord;
  result: ConformanceGraphqlResult;
};

async function capture(query: string, variables: Record<string, unknown> = {}): Promise<ConformanceGraphqlResult> {
  return runGraphqlRequest(query, variables);
}

function trimGraphql(query: string): string {
  return query.replace(/^#graphql\n/u, '').trim();
}

async function readRequest(name: string): Promise<string> {
  return readFile(path.join('config', 'parity-requests', 'shipping-fulfillments', name), 'utf8');
}

async function captureGraphql(query: string, variables: JsonRecord = {}): Promise<GraphqlCapture> {
  return {
    query: trimGraphql(query),
    variables,
    result: await capture(query, variables),
  };
}

function readObject(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readPath(value: unknown, pathParts: string[]): unknown {
  let current = value;
  for (const part of pathParts) {
    if (Array.isArray(current) && /^\d+$/u.test(part)) {
      current = current[Number(part)];
      continue;
    }
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

function readFirstProfileId(result: ConformanceGraphqlResult): string | null {
  const firstNode = readFirstProfileNode(result);
  const id = firstNode?.['id'];
  return typeof id === 'string' ? id : null;
}

function readFirstProfileCursor(result: ConformanceGraphqlResult): string | null {
  const firstEdge = readFirstProfileEdge(result);
  const cursor = firstEdge?.['cursor'];
  return typeof cursor === 'string' ? cursor : null;
}

function readFirstProfileNode(result: ConformanceGraphqlResult): Record<string, unknown> | null {
  const firstEdge = readFirstProfileEdge(result);
  const node = firstEdge?.['node'];
  return node && typeof node === 'object' && !Array.isArray(node) ? (node as Record<string, unknown>) : null;
}

function readFirstProfileEdge(result: ConformanceGraphqlResult): Record<string, unknown> | null {
  const data = result.payload.data;
  if (!data || typeof data !== 'object' || Array.isArray(data)) {
    return null;
  }

  const deliveryProfiles = (data as Record<string, unknown>)['deliveryProfiles'];
  if (!deliveryProfiles || typeof deliveryProfiles !== 'object' || Array.isArray(deliveryProfiles)) {
    return null;
  }

  const edges = (deliveryProfiles as Record<string, unknown>)['edges'];
  if (!Array.isArray(edges)) {
    return null;
  }

  const firstEdge = edges[0];
  if (!firstEdge || typeof firstEdge !== 'object' || Array.isArray(firstEdge)) {
    return null;
  }

  return firstEdge as Record<string, unknown>;
}

function readDefaultProfileId(result: ConformanceGraphqlResult): string {
  const data = readObject(result.payload.data);
  const deliveryProfiles = readObject(data?.['deliveryProfiles']);
  const edges = deliveryProfiles?.['edges'];
  if (!Array.isArray(edges)) {
    throw new Error(`deliveryProfiles catalog did not return edges: ${JSON.stringify(result.payload)}`);
  }

  for (const edge of edges) {
    const node = readObject(readObject(edge)?.['node']);
    if (node?.['default'] === true && typeof node['id'] === 'string') {
      return node['id'];
    }
  }

  throw new Error(`deliveryProfiles catalog did not include a default profile: ${JSON.stringify(result.payload)}`);
}

function deliveryProfilesCatalogIncludesId(result: ConformanceGraphqlResult, profileId: string): boolean {
  const data = readObject(result.payload.data);
  const deliveryProfiles = readObject(data?.['deliveryProfiles']);
  const edges = deliveryProfiles?.['edges'];
  if (!Array.isArray(edges)) {
    throw new Error(`deliveryProfiles catalog did not return edges: ${JSON.stringify(result.payload)}`);
  }
  return edges.some((edge) => {
    const node = readObject(readObject(edge)?.['node']);
    return node?.['id'] === profileId;
  });
}

function readUsableLocationIds(hydrate: GraphqlCapture): string[] {
  assertHttpOk('locations hydrate', hydrate.result);
  const nodes = readPath(hydrate.result.payload, ['data', 'locationsAvailableForDeliveryProfilesConnection', 'nodes']);
  if (!Array.isArray(nodes)) {
    throw new Error(`locations hydrate expected nodes array, got ${JSON.stringify(hydrate.result.payload)}`);
  }

  return nodes.flatMap((node): string[] => {
    const location = readObject(node);
    if (
      typeof location?.['id'] === 'string' &&
      location['isActive'] === true &&
      location['isFulfillmentService'] === false
    ) {
      return [location['id']];
    }
    return [];
  });
}

function readCreatedProfileId(
  captureResult: GraphqlCapture,
  root: 'deliveryProfileCreate' | 'deliveryProfileUpdate',
): string {
  assertHttpOk(root, captureResult.result);
  const id = readPath(captureResult.result.payload, ['data', root, 'profile', 'id']);
  if (typeof id !== 'string') {
    throw new Error(`${root} did not return a profile id: ${JSON.stringify(captureResult.result.payload)}`);
  }
  return id;
}

function requireString(value: unknown, label: string): string {
  if (typeof value !== 'string') {
    throw new Error(`${label} expected string, got ${JSON.stringify(value)}`);
  }
  return value;
}

async function waitForRemovedProfileRead(query: string, id: string): Promise<GraphqlCapture> {
  let latest = await captureGraphql(query, { id });
  for (let attempt = 0; attempt < 6; attempt += 1) {
    if (readPath(latest.result.payload, ['data', 'deliveryProfile']) === null) {
      return latest;
    }
    await delay(1_000);
    latest = await captureGraphql(query, { id });
  }
  return latest;
}

await mkdir(outputDir, { recursive: true });

const deliveryProfileCreateValidationMutation = await readRequest('delivery-profile-create-validation.graphql');
const deliveryProfileLifecycleBlankCreateMutation = await readRequest(
  'delivery-profile-lifecycle-blank-create.graphql',
);
const deliveryProfileLifecycleCreateMutation = await readRequest('delivery-profile-lifecycle-create.graphql');
const deliveryProfileLifecycleUpdateMutation = await readRequest('delivery-profile-lifecycle-update.graphql');
const deliveryProfileLifecycleReadAfterUpdateQuery = await readRequest(
  'delivery-profile-lifecycle-read-after-update.graphql',
);
const deliveryProfileLifecycleRemoveMutation = await readRequest('delivery-profile-lifecycle-remove.graphql');
const deliveryProfileLifecycleReadAfterRemoveQuery = await readRequest(
  'delivery-profile-lifecycle-read-after-remove.graphql',
);
const deliveryProfileLifecycleMissingUpdateMutation = await readRequest(
  'delivery-profile-lifecycle-missing-update.graphql',
);
const deliveryProfileLifecycleMissingRemoveMutation = await readRequest(
  'delivery-profile-lifecycle-missing-remove.graphql',
);
const deliveryProfileLifecycleDefaultRemoveMutation = await readRequest(
  'delivery-profile-lifecycle-default-remove.graphql',
);
const deliveryProfilesMergedReadQuery = await readRequest('delivery-profiles-merged-read.graphql');

const locationsHydrate = await captureGraphql(locationsHydrateQuery);
const usableLocationIds = readUsableLocationIds(locationsHydrate);
const primaryLocationId = usableLocationIds[0];
const secondaryLocationId = usableLocationIds.find((id) => id !== primaryLocationId);
if (primaryLocationId === undefined) {
  throw new Error(
    `No active merchant delivery-profile location found: ${JSON.stringify(locationsHydrate.result.payload)}`,
  );
}

const createBlankName = await captureGraphql(deliveryProfileCreateValidationMutation, {
  profile: { name: '' },
});
const createLongName = await captureGraphql(deliveryProfileCreateValidationMutation, {
  profile: { name: 'x'.repeat(300) },
});
const createUnknownLocation = await captureGraphql(deliveryProfileCreateValidationMutation, {
  profile: {
    name: 'Unknown location',
    locationGroupsToCreate: [{ locations: ['gid://shopify/Location/999999999'], zonesToCreate: [] }],
  },
});
const createEmptyCountries = await captureGraphql(deliveryProfileCreateValidationMutation, {
  profile: {
    name: 'Empty countries',
    locationGroupsToCreate: [
      {
        locations: [primaryLocationId],
        zonesToCreate: [{ name: 'Empty', countries: [] }],
      },
    ],
  },
});

await writeFile(
  createValidationOutputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      evidence: {
        locationUsed: primaryLocationId,
        deliveryProfileCreateUserErrorsType: 'UserError',
        userErrorFields: ['field', 'message'],
      },
      mutations: {
        blankName: createBlankName,
        longName: createLongName,
        unknownLocation: createUnknownLocation,
        emptyCountries: createEmptyCountries,
      },
      notes: [
        'Captured with home-folder conformance auth against a disposable Shopify test store.',
        'The capture records deliveryProfileCreate validation branches that do not create persistent delivery profiles.',
      ],
      upstreamCalls: [
        {
          operationName: 'ShippingDeliveryProfileLocationsHydrate',
          variables: {},
          query: trimGraphql(locationsHydrateQuery),
          response: {
            status: locationsHydrate.result.status,
            body: locationsHydrate.result.payload,
          },
        },
      ],
    },
    null,
    2,
  )}\n`,
  'utf8',
);

const lifecycleStamp = Date.now();
const lifecycleBlankCreate = await captureGraphql(deliveryProfileLifecycleBlankCreateMutation, {
  profile: { name: '' },
});
const lifecyclePreCreateCatalog = await captureGraphql(deliveryProfilesMergedReadQuery, { first: 10 });
const lifecycleNestedCreate = await captureGraphql(deliveryProfileLifecycleCreateMutation, {
  profile: {
    name: `Delivery profile lifecycle ${lifecycleStamp}`,
    locationGroupsToCreate: [
      {
        locations: [primaryLocationId],
        zonesToCreate: [
          {
            name: 'Domestic',
            countries: [
              { code: 'US', includeAllProvinces: true },
              { code: 'CA', includeAllProvinces: true },
            ],
            methodDefinitionsToCreate: [
              {
                name: 'Standard',
                description: 'Standard ground service',
                active: true,
                rateDefinition: { price: { amount: '7.25', currencyCode: 'USD' } },
                weightConditionsToCreate: [
                  {
                    operator: 'GREATER_THAN_OR_EQUAL_TO',
                    criteria: { value: 1, unit: 'KILOGRAMS' },
                  },
                ],
              },
            ],
          },
        ],
      },
    ],
  },
});
const lifecycleProfileId = readCreatedProfileId(lifecycleNestedCreate, 'deliveryProfileCreate');
const lifecycleLocationGroupId = requireString(
  readPath(lifecycleNestedCreate.result.payload, [
    'data',
    'deliveryProfileCreate',
    'profile',
    'profileLocationGroups',
    '0',
    'locationGroup',
    'id',
  ]),
  'created location group id',
);
const lifecycleZoneId = requireString(
  readPath(lifecycleNestedCreate.result.payload, [
    'data',
    'deliveryProfileCreate',
    'profile',
    'profileLocationGroups',
    '0',
    'locationGroupZones',
    'nodes',
    '0',
    'zone',
    'id',
  ]),
  'created zone id',
);
const lifecycleMethodId = requireString(
  readPath(lifecycleNestedCreate.result.payload, [
    'data',
    'deliveryProfileCreate',
    'profile',
    'profileLocationGroups',
    '0',
    'locationGroupZones',
    'nodes',
    '0',
    'methodDefinitions',
    'nodes',
    '0',
    'id',
  ]),
  'created method id',
);
const lifecycleRateId = requireString(
  readPath(lifecycleNestedCreate.result.payload, [
    'data',
    'deliveryProfileCreate',
    'profile',
    'profileLocationGroups',
    '0',
    'locationGroupZones',
    'nodes',
    '0',
    'methodDefinitions',
    'nodes',
    '0',
    'rateProvider',
    'id',
  ]),
  'created rate id',
);
const lifecycleConditionId = requireString(
  readPath(lifecycleNestedCreate.result.payload, [
    'data',
    'deliveryProfileCreate',
    'profile',
    'profileLocationGroups',
    '0',
    'locationGroupZones',
    'nodes',
    '0',
    'methodDefinitions',
    'nodes',
    '0',
    'methodConditions',
    '0',
    'id',
  ]),
  'created condition id',
);
const lifecycleReadAfterCreateCatalog = await captureGraphql(deliveryProfilesMergedReadQuery, { first: 10 });
readDefaultProfileId(lifecycleReadAfterCreateCatalog.result);
if (!deliveryProfilesCatalogIncludesId(lifecycleReadAfterCreateCatalog.result, lifecycleProfileId)) {
  throw new Error(
    `deliveryProfiles read after create did not include created profile ${lifecycleProfileId}: ${JSON.stringify(
      lifecycleReadAfterCreateCatalog.result.payload,
    )}`,
  );
}

const locationsToAdd = secondaryLocationId === undefined ? {} : { locationsToAdd: [secondaryLocationId] };
const lifecycleNestedUpdate = await captureGraphql(deliveryProfileLifecycleUpdateMutation, {
  id: lifecycleProfileId,
  profile: {
    name: 'Delivery profile lifecycle updated',
    conditionsToDelete: [lifecycleConditionId],
    locationGroupsToUpdate: [
      {
        id: lifecycleLocationGroupId,
        ...locationsToAdd,
        zonesToUpdate: [
          {
            id: lifecycleZoneId,
            name: 'Domestic updated',
            methodDefinitionsToUpdate: [
              {
                id: lifecycleMethodId,
                name: 'Standard updated',
                description: 'Updated standard ground service',
                active: false,
                rateDefinition: {
                  id: lifecycleRateId,
                  price: { amount: '8.50', currencyCode: 'USD' },
                },
              },
            ],
            methodDefinitionsToCreate: [
              {
                name: 'Express',
                description: 'Express air service',
                active: true,
                rateDefinition: { price: { amount: '12.00', currencyCode: 'USD' } },
                priceConditionsToCreate: [
                  {
                    operator: 'LESS_THAN_OR_EQUAL_TO',
                    criteria: { amount: '100.00', currencyCode: 'USD' },
                  },
                ],
              },
            ],
          },
        ],
      },
    ],
  },
});
const lifecycleReadAfterUpdate = await captureGraphql(deliveryProfileLifecycleReadAfterUpdateQuery, {
  id: lifecycleProfileId,
});
const lifecycleRemove = await captureGraphql(deliveryProfileLifecycleRemoveMutation, { id: lifecycleProfileId });
const lifecycleReadAfterRemovePoll = await waitForRemovedProfileRead(
  deliveryProfileLifecycleReadAfterRemoveQuery,
  lifecycleProfileId,
);
const lifecycleMissingUpdate = await captureGraphql(deliveryProfileLifecycleMissingUpdateMutation, {
  id: 'gid://shopify/DeliveryProfile/999999999999',
  profile: { name: 'Nope' },
});
const lifecycleMissingRemove = await captureGraphql(deliveryProfileLifecycleMissingRemoveMutation, {
  id: 'gid://shopify/DeliveryProfile/999999999999',
});

const catalogFirst = await capture(catalogQuery, { first: 2 });
const defaultProfileId = readDefaultProfileId(catalogFirst);
const defaultProfileHydrate = await captureGraphql(defaultProfileHydrateQuery, { id: defaultProfileId });
const lifecycleDefaultRemove = await captureGraphql(deliveryProfileLifecycleDefaultRemoveMutation, {
  id: defaultProfileId,
});

await writeFile(
  writesOutputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      evidence: {
        locationUsed: primaryLocationId,
        secondaryLocationUsed: secondaryLocationId ?? null,
        defaultProfileId,
      },
      mutations: {
        blankCreate: lifecycleBlankCreate,
        preCreateCatalog: lifecyclePreCreateCatalog,
        nestedCreate: lifecycleNestedCreate,
        readAfterCreateCatalog: lifecycleReadAfterCreateCatalog,
        nestedUpdate: lifecycleNestedUpdate,
        readAfterUpdate: lifecycleReadAfterUpdate,
        remove: lifecycleRemove,
        readAfterRemovePoll: lifecycleReadAfterRemovePoll,
        missingUpdate: lifecycleMissingUpdate,
        missingRemove: lifecycleMissingRemove,
        defaultRemove: lifecycleDefaultRemove,
      },
      notes: [
        'Captured with home-folder conformance auth against a disposable Shopify test store.',
        'The capture records deliveryProfiles before and after creating one disposable delivery profile so parity can replay the upstream default/base catalog and compare the downstream merged catalog read.',
        'The capture creates one disposable delivery profile, updates nested delivery profile state, removes it, records read-after-remove and missing/default profile validation branches.',
      ],
      upstreamCalls: [
        {
          operationName: 'ShippingDeliveryProfileLocationsHydrate',
          variables: {},
          query: trimGraphql(locationsHydrateQuery),
          response: {
            status: locationsHydrate.result.status,
            body: locationsHydrate.result.payload,
          },
        },
        {
          operationName: 'DeliveryProfilesCatalog',
          variables: lifecyclePreCreateCatalog.variables,
          query: lifecyclePreCreateCatalog.query,
          response: {
            status: lifecyclePreCreateCatalog.result.status,
            body: lifecyclePreCreateCatalog.result.payload,
          },
        },
        {
          operationName: 'ShippingDeliveryProfileHydrate',
          variables: { id: defaultProfileId },
          query: trimGraphql(defaultProfileHydrateQuery),
          response: {
            status: defaultProfileHydrate.result.status,
            body: defaultProfileHydrate.result.payload,
          },
        },
      ],
    },
    null,
    2,
  )}\n`,
  'utf8',
);

const firstProfileId = readFirstProfileId(catalogFirst);
const firstProfileCursor = readFirstProfileCursor(catalogFirst);
const detail = firstProfileId ? await capture(detailQuery, { id: firstProfileId }) : null;
const missing = await capture(detailQuery, { id: 'gid://shopify/DeliveryProfile/999999999999999' });
const reverseMerchantOwned = await capture(catalogQuery, {
  first: 2,
  reverse: true,
  merchantOwnedOnly: true,
});
const lastBeforeFirst = firstProfileCursor
  ? await capture(catalogQuery, {
      last: 1,
      before: firstProfileCursor,
    })
  : null;

const fixture = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  queries: {
    catalogFirst: {
      query: catalogQuery,
      variables: { first: 2 },
      result: catalogFirst,
    },
    reverseMerchantOwned: {
      query: catalogQuery,
      variables: {
        first: 2,
        reverse: true,
        merchantOwnedOnly: true,
      },
      result: reverseMerchantOwned,
    },
    lastBeforeFirst: firstProfileCursor
      ? {
          query: catalogQuery,
          variables: {
            last: 1,
            before: firstProfileCursor,
          },
          result: lastBeforeFirst,
        }
      : null,
    detail: firstProfileId
      ? {
          query: detailQuery,
          variables: { id: firstProfileId },
          result: detail,
        }
      : null,
    missing: {
      query: detailQuery,
      variables: { id: 'gid://shopify/DeliveryProfile/999999999999999' },
      result: missing,
    },
  },
};

await writeFile(readOutputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');

console.log(`wrote ${createValidationOutputPath}`);
console.log(`wrote ${writesOutputPath}`);
console.log(`wrote ${readOutputPath}`);

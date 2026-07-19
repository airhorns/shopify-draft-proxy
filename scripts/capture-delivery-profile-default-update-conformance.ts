/* oxlint-disable no-console -- CLI scripts intentionally write capture status to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
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
const outputPath = path.join(outputDir, 'delivery-profile-default-update.json');

const defaultProfileLookupQuery = `#graphql
  query DeliveryProfileDefaultLookup($first: Int!) {
    deliveryProfiles(first: $first) {
      edges {
        node {
          id
          name
          default
          version
        }
      }
    }
  }
`;

const defaultProfileHydrateQuery = `query ShippingDeliveryProfileUpdateHydrate($id: ID!) {
  deliveryProfile(id: $id) {
    id name default version activeMethodDefinitionsCount locationsWithoutRatesCount originLocationCount zoneCountryCount
    productVariantsCount { count precision }
    profileItems(first: 250) {
      nodes { id product { id title } variants(first: 250) { nodes { id title } pageInfo { hasNextPage endCursor } } }
      pageInfo { hasNextPage endCursor }
    }
    profileLocationGroups {
      countriesInAnyZone { zone country { id name translatedName code { countryCode restOfWorld } provinces { id name code } } }
      locationGroup {
        id locations(first: 250) { nodes { id name } pageInfo { hasNextPage endCursor } } locationsCount { count precision }
      }
      locationGroupZones(first: 250) {
        nodes {
          zone { id name countries { id name translatedName code { countryCode restOfWorld } provinces { id name code } } }
          methodDefinitions(first: 250) {
            nodes {
              id name active description
              rateProvider {
                ... on DeliveryRateDefinition { id price { amount currencyCode } }
                ... on DeliveryParticipant { id fixedFee { amount currencyCode } percentageOfRateFee }
              }
              methodConditions {
                id field operator conditionCriteria {
                  __typename ... on MoneyV2 { amount currencyCode } ... on Weight { unit value }
                }
              }
            }
            pageInfo { hasNextPage endCursor }
          }
        }
        pageInfo { hasNextPage endCursor }
      }
    }
    sellingPlanGroups(first: 250) { nodes { id name } pageInfo { hasNextPage endCursor } }
    unassignedLocationsPaginated(first: 250) { nodes { id name } pageInfo { hasNextPage endCursor } }
  }
}`;
const profileItemsPageQuery = `query ShippingDeliveryProfileItemsPage($id: ID!, $after: String!) {
  deliveryProfile(id: $id) {
    profileItems(first: 250, after: $after) {
      nodes { id product { id title } variants(first: 250) { nodes { id title } pageInfo { hasNextPage endCursor } } }
      pageInfo { hasNextPage endCursor }
    }
  }
}`;

const deliveryProfileDefaultUpdateMutation = await readFile(
  path.join('config', 'parity-requests', 'shipping-fulfillments', 'delivery-profile-default-update.graphql'),
  'utf8',
);
const deliveryProfileDefaultUpdateRead = await readFile(
  path.join('config', 'parity-requests', 'shipping-fulfillments', 'delivery-profile-default-update-read.graphql'),
  'utf8',
);

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
  if (readObject(result.payload)?.['errors'] !== undefined) {
    throw new Error(`${label} returned GraphQL errors: ${JSON.stringify(result.payload)}`);
  }
}

function readDefaultProfile(lookup: GraphqlCapture): JsonRecord {
  assertHttpOk('default profile lookup', lookup.result);
  const edges = readPath(lookup.result.payload, ['data', 'deliveryProfiles', 'edges']);
  if (!Array.isArray(edges)) {
    throw new Error(`default profile lookup expected edges array, got ${JSON.stringify(lookup.result.payload)}`);
  }
  for (const edge of edges) {
    const node = readObject(readObject(edge)?.['node']);
    if (node?.['default'] === true && typeof node['id'] === 'string' && typeof node['name'] === 'string') {
      return node;
    }
  }
  throw new Error(`default profile lookup did not return a default profile: ${JSON.stringify(lookup.result.payload)}`);
}

function assertSuccessfulDefaultUpdate(
  captureResult: GraphqlCapture,
  expectedId: string,
  expectedName: string,
): JsonRecord {
  assertHttpOk('default profile update', captureResult.result);
  const payload = readPath(captureResult.result.payload, ['data', 'deliveryProfileUpdate']);
  const profile = readObject(readObject(payload)?.['profile']);
  const userErrors = readObject(payload)?.['userErrors'];
  if (
    profile?.['id'] !== expectedId ||
    profile['name'] !== expectedName ||
    profile['default'] !== true ||
    !Array.isArray(userErrors) ||
    userErrors.length !== 0
  ) {
    throw new Error(
      `default profile update expected updated default profile and no userErrors, got ${JSON.stringify(
        captureResult.result.payload,
      )}`,
    );
  }
  return profile;
}

const lookup = await capture(defaultProfileLookupQuery, { first: 10 });
const defaultProfile = readDefaultProfile(lookup);
const defaultProfileId = defaultProfile['id'] as string;
const originalName = defaultProfile['name'] as string;
const updateName = `Default profile parity ${Date.now()}`;
const hydrateBeforeUpdate = await capture(defaultProfileHydrateQuery, { id: defaultProfileId });
assertHttpOk('default profile hydrate before update', hydrateBeforeUpdate.result);
const profileItemsHydratePages: GraphqlCapture[] = [];
let profileItemsPage = readObject(
  readPath(hydrateBeforeUpdate.result.payload, ['data', 'deliveryProfile', 'profileItems']),
);
while (readObject(profileItemsPage?.['pageInfo'])?.['hasNextPage'] === true) {
  const after = readObject(profileItemsPage?.['pageInfo'])?.['endCursor'];
  if (typeof after !== 'string' || after.length === 0) {
    throw new Error(
      `profileItems hydrate page reported hasNextPage without endCursor: ${JSON.stringify(profileItemsPage)}`,
    );
  }
  const page = await capture(profileItemsPageQuery, { id: defaultProfileId, after });
  assertHttpOk('default profile items hydrate page', page.result);
  profileItemsHydratePages.push(page);
  profileItemsPage = readObject(readPath(page.result.payload, ['data', 'deliveryProfile', 'profileItems']));
}
if (profileItemsHydratePages.length === 0) {
  throw new Error('default profile capture requires profileItems beyond the first hydrate page');
}

let cleanup: GraphqlCapture | undefined;
const mutation = await capture(deliveryProfileDefaultUpdateMutation, {
  id: defaultProfileId,
  profile: {
    name: updateName,
  },
});
const updatedProfile = assertSuccessfulDefaultUpdate(mutation, defaultProfileId, originalName);

const readBack = await capture(deliveryProfileDefaultUpdateRead, { id: defaultProfileId });
assertHttpOk('default profile readback', readBack.result);
const readBackProfile = readObject(readPath(readBack.result.payload, ['data', 'deliveryProfile']));
if (JSON.stringify(readBackProfile) !== JSON.stringify(updatedProfile)) {
  throw new Error(
    `default profile readback did not preserve the whole selected mutation profile: ${JSON.stringify({
      updatedProfile,
      readBackProfile,
    })}`,
  );
}

try {
  if (updatedProfile['name'] === updateName) {
    cleanup = await capture(deliveryProfileDefaultUpdateMutation, {
      id: defaultProfileId,
      profile: {
        name: originalName,
      },
    });
    assertSuccessfulDefaultUpdate(cleanup, defaultProfileId, originalName);
  }
} finally {
  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        setup: {
          defaultProfileLookup: lookup,
          defaultProfileHydrateBeforeUpdate: hydrateBeforeUpdate,
        },
        mutation,
        readBack,
        cleanup: {
          restoreOriginalName: cleanup ?? null,
        },
        expectedProxyLog: {
          entries: [
            {
              interpreted: {
                primaryRootField: 'deliveryProfileUpdate',
              },
              stagedResourceIds: [defaultProfileId],
              status: 'staged',
            },
          ],
        },
        notes: [
          'Captured with SHOPIFY_CONFORMANCE_API_VERSION=2026-04 and home-folder conformance auth.',
          'Setup reads deliveryProfiles to find the default profile id, hydrates that profile with the same read query used by the proxy update path, sends a name update for the default profile, reads it back, and restores the original name only if Shopify returns a changed display name.',
          'Admin GraphQL 2026-04 accepts the default-profile name update with empty userErrors and increments version; the selected public payload preserves the default profile display name.',
          'The hydrate cassette follows every profileItems page for this relationship-heavy default profile; the selected mutation profile and immediate readback are asserted equal by the recorder.',
          'Parity strictly compares the whole selected profile in the mutation payload and immediate readback, including connection rows and Count precision metadata.',
        ],
        upstreamCalls: [
          {
            operationName: 'ShippingDeliveryProfileUpdateHydrate',
            variables: { id: defaultProfileId },
            query: defaultProfileHydrateQuery,
            response: {
              status: hydrateBeforeUpdate.result.status,
              body: hydrateBeforeUpdate.result.payload,
            },
          },
          ...profileItemsHydratePages.map((page) => ({
            operationName: 'ShippingDeliveryProfileItemsPage',
            variables: page.variables,
            query: profileItemsPageQuery,
            response: {
              status: page.result.status,
              body: page.result.payload,
            },
          })),
        ],
      },
      null,
      2,
    )}\n`,
    'utf8',
  );
}

console.log(JSON.stringify({ ok: true, outputPath, defaultProfileId }, null, 2));

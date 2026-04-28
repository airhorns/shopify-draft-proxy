/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'shipping-fulfillments');
const outputPath = path.join(outputDir, 'delivery-profiles-read.json');

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

async function capture(query: string, variables: Record<string, unknown> = {}): Promise<ConformanceGraphqlResult> {
  return runGraphqlRequest(query, variables);
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

await mkdir(outputDir, { recursive: true });

const catalogFirst = await capture(catalogQuery, { first: 2 });
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

await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');

console.log(`wrote ${outputPath}`);

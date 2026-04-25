// @ts-nocheck
/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion);
const storePropertiesOutputPath = path.join(outputDir, 'store-properties-baseline.json');
const locationsCatalogOutputPath = path.join(outputDir, 'locations-catalog.json');
const businessEntitiesCatalogOutputPath = path.join(outputDir, 'business-entities-catalog.json');
const businessEntityFallbacksOutputPath = path.join(outputDir, 'business-entity-fallbacks.json');

const { runGraphql, runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function runGraphqlCapture(query, variables = {}) {
  const result = await runGraphqlRequest(query, variables);
  return result.payload;
}

const locationsCatalogQuery = `#graphql
  query LocationsCatalogRead($first: Int!) {
    locations(first: $first) {
      edges {
        cursor
        node {
          id
          name
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

const shopBaselineQuery = `#graphql
  query StorePropertiesShopBaseline {
    shop {
      id
      name
      myshopifyDomain
    }
  }
`;

const locationDetailFields = `#graphql
  fragment StorePropertiesLocationDetailFields on Location {
    id
    legacyResourceId
    name
    activatable
    addressVerified
    createdAt
    deactivatable
    deactivatedAt
    deletable
    fulfillmentService {
      id
      handle
      serviceName
    }
    fulfillsOnlineOrders
    hasActiveInventory
    hasUnfulfilledOrders
    isActive
    isFulfillmentService
    shipsInventory
    updatedAt
    address {
      address1
      address2
      city
      country
      countryCode
      formatted
      latitude
      longitude
      phone
      province
      provinceCode
      zip
    }
    suggestedAddresses {
      address1
      address2
      city
      country
      countryCode
      formatted
      province
      provinceCode
      zip
    }
    metafield(namespace: "custom", key: "hours") {
      id
      namespace
      key
      value
      type
    }
    metafields(first: 3) {
      nodes {
        id
        namespace
        key
        value
        type
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
    inventoryLevels(first: 3) {
      nodes {
        id
        item {
          id
        }
        location {
          id
          name
        }
        quantities(names: ["available", "committed", "on_hand"]) {
          name
          quantity
          updatedAt
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

const locationDetailsQuery = `#graphql
  ${locationDetailFields}

  query StorePropertiesLocationDetails($id: ID!, $unknownId: ID!) {
    primary: location {
      ...StorePropertiesLocationDetailFields
    }
    byId: location(id: $id) {
      ...StorePropertiesLocationDetailFields
    }
    byIdentifier: locationByIdentifier(identifier: { id: $id }) {
      ...StorePropertiesLocationDetailFields
    }
    unknownById: location(id: $unknownId) {
      id
      name
    }
    unknownByIdentifier: locationByIdentifier(identifier: { id: $unknownId }) {
      id
      name
    }
  }
`;

const locationInvalidIdentifierQuery = `#graphql
  query StorePropertiesLocationInvalidIdentifier {
    emptyIdentifier: locationByIdentifier(identifier: {}) {
      id
      name
    }
  }
`;

const locationInventoryLevelQuery = `#graphql
  query StorePropertiesLocationInventoryLevel($locationId: ID!, $inventoryItemId: ID!) {
    location(id: $locationId) {
      id
      inventoryLevel(inventoryItemId: $inventoryItemId) {
        id
        item {
          id
        }
        location {
          id
          name
        }
        quantities(names: ["available", "committed", "on_hand"]) {
          name
          quantity
          updatedAt
        }
      }
    }
  }
`;

const businessEntityFields = `#graphql
  fragment StorePropertiesBusinessEntityFields on BusinessEntity {
    id
    displayName
    companyName
    primary
    archived
    address {
      address1
      address2
      city
      countryCode
      province
      zip
    }
    shopifyPaymentsAccount {
      id
      activated
      country
      defaultCurrency
      onboardable
    }
  }
`;

const businessEntitiesCatalogQuery = `#graphql
  ${businessEntityFields}

  query StorePropertiesBusinessEntitiesCatalog {
    businessEntities {
      ...StorePropertiesBusinessEntityFields
    }
  }
`;

const businessEntityFallbacksQuery = `#graphql
  ${businessEntityFields}

  query StorePropertiesBusinessEntityFallbacks($knownId: ID!, $unknownId: ID!) {
    primary: businessEntity {
      ...StorePropertiesBusinessEntityFields
    }
    known: businessEntity(id: $knownId) {
      ...StorePropertiesBusinessEntityFields
    }
    unknown: businessEntity(id: $unknownId) {
      id
    }
  }
`;

const schemaInventoryQuery = `#graphql
  query StorePropertiesSchemaInventory {
    queryRoot: __type(name: "QueryRoot") {
      fields {
        name
        args {
          name
          type {
            ...TypeRef
          }
        }
        type {
          ...TypeRef
        }
      }
    }
    mutationRoot: __type(name: "Mutation") {
      fields {
        name
        args {
          name
          type {
            ...TypeRef
          }
        }
        type {
          ...TypeRef
        }
      }
    }
    shopType: __type(name: "Shop") {
      fields {
        name
        type {
          ...TypeRef
        }
      }
    }
    locationType: __type(name: "Location") {
      fields {
        name
        type {
          ...TypeRef
        }
      }
    }
    businessEntityType: __type(name: "BusinessEntity") {
      fields {
        name
        type {
          ...TypeRef
        }
      }
    }
    cashManagementLocationSummaryType: __type(name: "CashManagementLocationSummary") {
      fields {
        name
        type {
          ...TypeRef
        }
      }
    }
    shopPolicyType: __type(name: "ShopPolicy") {
      fields {
        name
        type {
          ...TypeRef
        }
      }
    }
    publishableType: __type(name: "Publishable") {
      fields {
        name
        type {
          ...TypeRef
        }
      }
    }
  }

  fragment TypeRef on __Type {
    kind
    name
    ofType {
      kind
      name
      ofType {
        kind
        name
        ofType {
          kind
          name
          ofType {
            kind
            name
          }
        }
      }
    }
  }
`;

const storePropertiesQueryRoots = [
  'shop',
  'location',
  'locationByIdentifier',
  'businessEntities',
  'businessEntity',
  'cashManagementLocationSummary',
];
const storePropertiesMutationRoots = [
  'locationAdd',
  'locationEdit',
  'locationActivate',
  'locationDeactivate',
  'locationDelete',
  'publishablePublish',
  'publishablePublishToCurrentChannel',
  'publishableUnpublish',
  'publishableUnpublishToCurrentChannel',
  'shopPolicyUpdate',
];

function filterRootFields(schemaInventory, rootKey, names) {
  const fields = schemaInventory.data?.[rootKey]?.fields ?? [];
  return fields.filter((field) => names.includes(field.name));
}

function mutationValidationProbePlan() {
  return [
    {
      root: 'location lifecycle',
      operations: ['locationAdd', 'locationEdit', 'locationActivate', 'locationDeactivate', 'locationDelete'],
      safety: 'side-effect-heavy; do not run happy paths against shared conformance stores',
      recommendedProbe: 'schema/variable validation only with deliberately invalid IDs or missing required input',
    },
    {
      root: 'generic publishable mutations',
      operations: [
        'publishablePublish',
        'publishablePublishToCurrentChannel',
        'publishableUnpublish',
        'publishableUnpublishToCurrentChannel',
      ],
      safety: 'can alter publication/channel visibility for any Publishable implementer',
      recommendedProbe:
        'invalid Publishable ID and missing publication/channel arguments before any success-path capture',
    },
    {
      root: 'shopPolicyUpdate',
      operations: ['shopPolicyUpdate'],
      safety: 'updates merchant legal policy content',
      recommendedProbe: 'input validation only until a disposable dev store policy fixture is available',
    },
  ];
}

await mkdir(outputDir, { recursive: true });

const first = 10;
const schemaInventory = await runGraphql(schemaInventoryQuery);
const shopBaseline = await runGraphql(shopBaselineQuery);
const locationsCatalog = await runGraphql(locationsCatalogQuery, { first });
const firstLocationId = locationsCatalog.data?.locations?.edges?.[0]?.node?.id ?? null;
const locationDetails =
  typeof firstLocationId === 'string' && firstLocationId.length > 0
    ? await runGraphqlCapture(locationDetailsQuery, {
        id: firstLocationId,
        unknownId: 'gid://shopify/Location/999999999999',
      })
    : null;
const firstInventoryItemId =
  locationDetails?.data?.byId?.inventoryLevels?.nodes?.find((node) => typeof node?.item?.id === 'string')?.item?.id ??
  null;
const locationInventoryLevel =
  typeof firstLocationId === 'string' &&
  firstLocationId.length > 0 &&
  typeof firstInventoryItemId === 'string' &&
  firstInventoryItemId.length > 0
    ? await runGraphqlCapture(locationInventoryLevelQuery, {
        locationId: firstLocationId,
        inventoryItemId: firstInventoryItemId,
      })
    : null;
const locationInvalidIdentifier = await runGraphqlCapture(locationInvalidIdentifierQuery);
const businessEntitiesCatalog = await runGraphqlCapture(businessEntitiesCatalogQuery);
const firstBusinessEntityId = businessEntitiesCatalog.data?.businessEntities?.[0]?.id ?? null;
const businessEntityFallbacks =
  typeof firstBusinessEntityId === 'string' && firstBusinessEntityId.length > 0
    ? await runGraphqlCapture(businessEntityFallbacksQuery, {
        knownId: firstBusinessEntityId,
        unknownId: 'gid://shopify/BusinessEntity/999999999999999999',
      })
    : null;

const storePropertiesBaseline = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  schemaInventory: {
    queryRoots: filterRootFields(schemaInventory, 'queryRoot', storePropertiesQueryRoots),
    mutationRoots: filterRootFields(schemaInventory, 'mutationRoot', storePropertiesMutationRoots),
    objectTypes: {
      shop: schemaInventory.data?.shopType ?? null,
      location: schemaInventory.data?.locationType ?? null,
      businessEntity: schemaInventory.data?.businessEntityType ?? null,
      cashManagementLocationSummary: schemaInventory.data?.cashManagementLocationSummaryType ?? null,
      shopPolicy: schemaInventory.data?.shopPolicyType ?? null,
      publishable: schemaInventory.data?.publishableType ?? null,
    },
  },
  readOnlyBaselines: {
    shop: shopBaseline,
    locationsCatalog,
    location: locationDetails,
    locationInventoryLevel,
    locationInvalidIdentifier,
    businessEntitiesCatalog,
    businessEntityFallbacks,
  },
  mutationValidationProbePlan: mutationValidationProbePlan(),
};

await writeFile(locationsCatalogOutputPath, `${JSON.stringify(locationsCatalog, null, 2)}\n`, 'utf8');
await writeFile(businessEntitiesCatalogOutputPath, `${JSON.stringify(businessEntitiesCatalog, null, 2)}\n`, 'utf8');
if (businessEntityFallbacks) {
  await writeFile(businessEntityFallbacksOutputPath, `${JSON.stringify(businessEntityFallbacks, null, 2)}\n`, 'utf8');
}
await writeFile(storePropertiesOutputPath, `${JSON.stringify(storePropertiesBaseline, null, 2)}\n`, 'utf8');

console.log(
  JSON.stringify(
    {
      ok: true,
      outputDir,
      files: [
        'locations-catalog.json',
        'business-entities-catalog.json',
        ...(businessEntityFallbacks ? ['business-entity-fallbacks.json'] : []),
        'store-properties-baseline.json',
      ],
      first,
    },
    null,
    2,
  ),
);

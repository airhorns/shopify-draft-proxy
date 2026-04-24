// @ts-nocheck
/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const requiredVars = ['SHOPIFY_CONFORMANCE_STORE_DOMAIN', 'SHOPIFY_CONFORMANCE_ADMIN_ORIGIN'];

const missingVars = requiredVars.filter((name) => !process.env[name]);
if (missingVars.length > 0) {
  console.error(`Missing required environment variables: ${missingVars.join(', ')}`);
  process.exit(1);
}

const storeDomain = process.env['SHOPIFY_CONFORMANCE_STORE_DOMAIN'];
const adminOrigin = process.env['SHOPIFY_CONFORMANCE_ADMIN_ORIGIN'];
const apiVersion = process.env['SHOPIFY_CONFORMANCE_API_VERSION'] || '2025-01';
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion);
const storePropertiesOutputPath = path.join(outputDir, 'store-properties-baseline.json');
const locationsCatalogOutputPath = path.join(outputDir, 'locations-catalog.json');

async function runGraphql(query, variables = {}) {
  const response = await fetch(`${adminOrigin}/admin/api/${apiVersion}/graphql.json`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      ...buildAdminAuthHeaders(adminAccessToken),
    },
    body: JSON.stringify({ query, variables }),
  });

  const payload = await response.json();
  if (!response.ok || payload.errors) {
    throw new Error(JSON.stringify({ status: response.status, payload }, null, 2));
  }

  return payload;
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

const locationByIdQuery = `#graphql
  query StorePropertiesLocationBaseline($id: ID!) {
    location(id: $id) {
      id
      name
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
const locationBaseline =
  typeof firstLocationId === 'string' && firstLocationId.length > 0
    ? await runGraphql(locationByIdQuery, { id: firstLocationId })
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
    location: locationBaseline,
  },
  mutationValidationProbePlan: mutationValidationProbePlan(),
};

await writeFile(locationsCatalogOutputPath, `${JSON.stringify(locationsCatalog, null, 2)}\n`, 'utf8');
await writeFile(storePropertiesOutputPath, `${JSON.stringify(storePropertiesBaseline, null, 2)}\n`, 'utf8');

console.log(
  JSON.stringify(
    {
      ok: true,
      outputDir,
      files: ['locations-catalog.json', 'store-properties-baseline.json'],
      first,
    },
    null,
    2,
  ),
);

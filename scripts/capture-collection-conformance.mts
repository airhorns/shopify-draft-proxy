// @ts-nocheck
import 'dotenv/config';

import { readFileSync } from 'node:fs';
import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

import { pickCollectionCaptureSeed } from './collection-conformance-lib.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const { runGraphql, runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});
const collectionDetailVariablesPath = path.join(
  'config',
  'parity-requests',
  'products',
  'collection-detail-read.variables.json',
);
const collectionIdentifierVariablesPath = path.join(
  'config',
  'parity-requests',
  'products',
  'collection-identifier-read.variables.json',
);
const collectionHandleVariablesPath = path.join(
  'config',
  'parity-requests',
  'products',
  'collection-handle-read.variables.json',
);
const collectionDetailDocument = readFileSync(
  path.join('config', 'parity-requests', 'products', 'collection-detail-read.graphql'),
  'utf8',
);
const collectionIdentifierDocument = readFileSync(
  path.join('config', 'parity-requests', 'products', 'collection-identifier-read.graphql'),
  'utf8',
);
const collectionHandleDocument = readFileSync(
  path.join('config', 'parity-requests', 'products', 'collection-handle-read.graphql'),
  'utf8',
);

const collectionSeedQuery = `#graphql
  query CollectionSeedCatalog {
    products(first: 10, sortKey: UPDATED_AT, reverse: true) {
      edges {
        node {
          id
          title
          collections(first: 5) {
            edges {
              node {
                id
                title
                handle
              }
            }
          }
        }
      }
    }
    collections(first: 20) {
      edges {
        node {
          id
          title
          handle
          ruleSet {
            appliedDisjunctively
            rules {
              column
              relation
              condition
            }
          }
          products(first: 1) {
            edges {
              node {
                id
              }
            }
          }
        }
      }
    }
  }
`;

const collectionsCatalogQuery = `#graphql
  query CollectionsCatalogRead(
    $catalogFirst: Int!
    $first: Int!
    $titleWildcardQuery: String!
    $customTypeQuery: String!
    $smartTypeQuery: String!
    $updatedSortQuery: String!
    $emptyQuery: String!
    $productMembershipQuery: String!
  ) {
    collections(first: $catalogFirst) {
      edges {
        cursor
        node {
          id
          legacyResourceId
          title
          handle
          updatedAt
          description
          descriptionHtml
          image {
            id
            url
            altText
            width
            height
          }
          productsCount {
            count
            precision
          }
          sortOrder
          templateSuffix
          seo {
            title
            description
          }
          ruleSet {
            appliedDisjunctively
            rules {
              column
              relation
              condition
            }
          }
          products(first: 2) {
            edges {
              cursor
              node {
                id
                title
                handle
                vendor
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
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
    titleWildcard: collections(first: $first, query: $titleWildcardQuery, sortKey: TITLE) {
      edges {
        cursor
        node {
          id
          title
          handle
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
    customCollections: collections(first: $first, query: $customTypeQuery, sortKey: ID) {
      edges {
        cursor
        node {
          id
          title
          handle
          ruleSet {
            appliedDisjunctively
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
    smartCollections: collections(first: $first, query: $smartTypeQuery, sortKey: TITLE) {
      edges {
        cursor
        node {
          id
          title
          handle
          ruleSet {
            appliedDisjunctively
            rules {
              column
              relation
              condition
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
    updatedNewest: collections(first: $first, query: $updatedSortQuery, sortKey: UPDATED_AT, reverse: true) {
      edges {
        cursor
        node {
          id
          title
          handle
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
    productMembership: collections(first: $first, query: $productMembershipQuery, sortKey: ID) {
      edges {
        cursor
        node {
          id
          title
          handle
        }
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
    emptyUnmatched: collections(first: $first, query: $emptyQuery) {
      edges {
        cursor
        node {
          id
          title
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

await mkdir(outputDir, { recursive: true });

const collectionSeedCatalog = await runGraphql(collectionSeedQuery);
const sampleCollection = pickCollectionCaptureSeed(collectionSeedCatalog);
const collectionEdges = collectionSeedCatalog.data.collections.edges;
const customCollection = collectionEdges.map((edge) => edge.node).find((collection) => collection.ruleSet === null);
const smartCollection = collectionEdges.map((edge) => edge.node).find((collection) => collection.ruleSet !== null);

if (!customCollection || !smartCollection) {
  throw new Error(
    JSON.stringify(
      {
        message: 'Could not find both custom and smart collection seeds in the live collection catalog.',
        customCollection: customCollection
          ? { id: customCollection.id, title: customCollection.title, handle: customCollection.handle }
          : null,
        smartCollection: smartCollection
          ? { id: smartCollection.id, title: smartCollection.title, handle: smartCollection.handle }
          : null,
      },
      null,
      2,
    ),
  );
}

const productId =
  customCollection.products.edges[0]?.node?.id ??
  smartCollection.products.edges[0]?.node?.id ??
  'gid://shopify/Product/0';
const collectionDetailVariables = {
  customCollectionId: customCollection.id,
  smartCollectionId: smartCollection.id,
  productId,
};
const collectionIdentifierVariables = {
  customCollectionId: customCollection.id,
  customCollectionHandle: customCollection.handle,
  productId,
};
const collectionHandleVariables = {
  customCollectionHandle: customCollection.handle,
  productId,
};
const collectionDetail = await runGraphqlRequest(collectionDetailDocument, collectionDetailVariables);
const collectionIdentifier = await runGraphqlRequest(collectionIdentifierDocument, collectionIdentifierVariables);
const collectionHandle = await runGraphqlRequest(collectionHandleDocument, collectionHandleVariables);
const smartCollectionProductId = smartCollection.products.edges[0]?.node?.id ?? productId;
const smartCollectionProductLegacyId = smartCollectionProductId.split('/').at(-1) ?? smartCollectionProductId;
// Catalog reads are de-seeded: the proxy cannot reconstruct Shopify's opaque
// pagination cursors or server-side query filtering, so it forwards the read
// upstream and snapshots the result (read-through cache). The recorded fixture
// therefore carries the forwarded `upstreamCalls` instead of a
// `/__meta/seed`-style `seedCollectionCatalog` precondition, and the spec's
// variables file is rewritten in lockstep so the cassette's variables match the
// runner's outgoing request on this store.
const collectionsCatalogVariables = {
  catalogFirst: 20,
  first: 3,
  titleWildcardQuery: `title:${smartCollection.title.slice(0, 3)}*`,
  customTypeQuery: 'collection_type:custom',
  smartTypeQuery: 'collection_type:smart',
  updatedSortQuery: 'collection_type:smart',
  emptyQuery: 'title:No collection should match this 157*',
  productMembershipQuery: `product_id:${smartCollectionProductLegacyId}`,
};
const collectionsCatalog = await runGraphql(collectionsCatalogQuery, collectionsCatalogVariables);
// The proxy forwards the canonical parity request document verbatim and the
// cassette is matched byte-for-byte, so store that exact document (read from
// disk) as the forwarded upstream call's query.
const collectionsCatalogVariablesPath = path.join(
  'config',
  'parity-requests',
  'products',
  'collections-catalog-read.variables.json',
);
const collectionsCatalogDocument = readFileSync(
  path.join('config', 'parity-requests', 'products', 'collections-catalog-read.graphql'),
  'utf8',
);

const captures = {
  'collection-detail.json': {
    ...collectionDetail.payload,
    upstreamCalls: [
      {
        operationName: 'CollectionDetailRead',
        variables: collectionDetailVariables,
        query: collectionDetailDocument,
        response: { status: collectionDetail.status, body: collectionDetail.payload },
      },
      {
        operationName: 'CollectionIdentifierRead',
        variables: collectionIdentifierVariables,
        query: collectionIdentifierDocument,
        response: { status: collectionIdentifier.status, body: collectionIdentifier.payload },
      },
      {
        operationName: 'CollectionHandleRead',
        variables: collectionHandleVariables,
        query: collectionHandleDocument,
        response: { status: collectionHandle.status, body: collectionHandle.payload },
      },
      {
        operationName: 'CollectionIdentifierRead',
        variables: collectionIdentifierVariables,
        query: collectionIdentifierDocument,
        response: { status: collectionIdentifier.status, body: collectionIdentifier.payload },
      },
    ],
  },
  'collections-catalog.json': {
    data: collectionsCatalog.data,
    ...(collectionsCatalog.extensions ? { extensions: collectionsCatalog.extensions } : {}),
    upstreamCalls: [
      {
        operationName: 'CollectionsCatalogRead',
        variables: collectionsCatalogVariables,
        query: collectionsCatalogDocument,
        response: { status: 200, body: collectionsCatalog },
      },
    ],
  },
};

for (const [filename, payload] of Object.entries(captures)) {
  await writeFile(path.join(outputDir, filename), `${JSON.stringify(payload, null, 2)}\n`, 'utf8');
}

// Keep the parity spec's variables in lockstep with the freshly recorded
// catalog read, so the runner's outgoing request matches the recorded cassette.
await writeFile(collectionsCatalogVariablesPath, `${JSON.stringify(collectionsCatalogVariables, null, 2)}\n`, 'utf8');
await writeFile(collectionDetailVariablesPath, `${JSON.stringify(collectionDetailVariables, null, 2)}\n`, 'utf8');
await writeFile(
  collectionIdentifierVariablesPath,
  `${JSON.stringify(collectionIdentifierVariables, null, 2)}\n`,
  'utf8',
);
await writeFile(collectionHandleVariablesPath, `${JSON.stringify(collectionHandleVariables, null, 2)}\n`, 'utf8');

// oxlint-disable-next-line no-console -- CLI capture result is intentionally written to stdout.
console.log(
  JSON.stringify(
    {
      ok: true,
      outputDir,
      sampleCollection,
      customCollection: {
        id: customCollection.id,
        title: customCollection.title,
        handle: customCollection.handle,
      },
      smartCollection: {
        id: smartCollection.id,
        title: smartCollection.title,
        handle: smartCollection.handle,
      },
      productId,
      smartCollectionProductId,
      smartCollectionProductLegacyId,
      files: Object.keys(captures),
    },
    null,
    2,
  ),
);

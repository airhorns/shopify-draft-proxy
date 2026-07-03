// @ts-nocheck
/* oxlint-disable no-console -- CLI scripts intentionally write capture status to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'admin-platform');
const outputPath = path.join(outputDir, 'admin-platform-utility-roots.json');
const taxonomyHierarchyOutputPath = path.join(outputDir, 'admin-platform-taxonomy-hierarchy-node-reads.json');
const byIdNotFoundOutputPath = path.join(outputDir, 'by-id-not-found-read.json');
const utilityReadsDocumentPath = path.join(
  'config',
  'parity-requests',
  'admin-platform',
  'admin-platform-utility-reads.graphql',
);
const utilityReadsVariablesPath = path.join(
  'config',
  'parity-requests',
  'admin-platform',
  'admin-platform-utility-reads.variables.json',
);
const backupRegionUpdateIdempotentDocumentPath = path.join(
  'config',
  'parity-requests',
  'admin-platform',
  'admin-platform-backup-region-update-idempotent.graphql',
);
const supportedNodeReadsDocumentPath = path.join(
  'config',
  'parity-requests',
  'admin-platform',
  'admin-platform-supported-node-reads.graphql',
);
const taxonomyHierarchyDocumentPath = path.join(
  'config',
  'parity-requests',
  'admin-platform',
  'admin-platform-taxonomy-hierarchy-node-reads.graphql',
);
const byIdNotFoundDocumentRoot = path.join('config', 'parity-requests', 'admin-platform', 'by-id-not-found');
const utilityReadsDocument = await readFile(utilityReadsDocumentPath, 'utf8');
const utilityReadsVariables = JSON.parse(await readFile(utilityReadsVariablesPath, 'utf8'));
const supportedNodeReadsDocument = await readFile(supportedNodeReadsDocumentPath, 'utf8');
const taxonomyHierarchyDocument = await readFile(taxonomyHierarchyDocumentPath, 'utf8');
const byIdNotFoundCaseConfigs = {
  discounts: {
    documentPath: path.join(byIdNotFoundDocumentRoot, 'discounts.graphql'),
    variables: {
      automaticDiscountNodeId: 'gid://shopify/DiscountAutomaticNode/0',
      codeDiscountNodeId: 'gid://shopify/DiscountCodeNode/0',
      discountNodeId: 'gid://shopify/DiscountCodeNode/0',
      discountRedeemCodeBulkCreationId: 'gid://shopify/DiscountRedeemCodeBulkCreation/999999999999',
    },
  },
  markets: {
    documentPath: path.join(byIdNotFoundDocumentRoot, 'markets.graphql'),
    variables: {
      catalogId: 'gid://shopify/MarketCatalog/999999999999',
      marketId: 'gid://shopify/Market/999999999999',
      priceListId: 'gid://shopify/PriceList/999999999999',
    },
  },
  products: {
    documentPath: path.join(byIdNotFoundDocumentRoot, 'products.graphql'),
    variables: {
      inventoryShipmentId: 'gid://shopify/InventoryShipment/999999999999',
    },
  },
  shippingFulfillments: {
    documentPath: path.join(byIdNotFoundDocumentRoot, 'shipping-fulfillments.graphql'),
    variables: {
      fulfillmentServiceId: 'gid://shopify/FulfillmentService/999999999999',
    },
  },
  storeProperties: {
    documentPath: path.join(byIdNotFoundDocumentRoot, 'store-properties.graphql'),
    variables: {
      locationId: 'gid://shopify/Location/999999999999',
    },
  },
  orders: {
    documentPath: path.join(byIdNotFoundDocumentRoot, 'orders.graphql'),
    variables: {
      returnId: 'gid://shopify/Return/999999999999',
      reverseDeliveryId: 'gid://shopify/ReverseDelivery/999999999999',
      reverseFulfillmentOrderId: 'gid://shopify/ReverseFulfillmentOrder/999999999999',
    },
  },
  customers: {
    documentPath: path.join(byIdNotFoundDocumentRoot, 'customers.graphql'),
    variables: {
      customerPaymentMethodId: 'gid://shopify/CustomerPaymentMethod/999999999999',
      storeCreditAccountId: 'gid://shopify/StoreCreditAccount/999999999999',
    },
  },
  giftCards: {
    documentPath: path.join(byIdNotFoundDocumentRoot, 'gift-cards.graphql'),
    variables: {
      giftCardId: 'gid://shopify/GiftCard/999999999999',
    },
  },
  onlineStore: {
    documentPath: path.join(byIdNotFoundDocumentRoot, 'online-store.graphql'),
    variables: {
      scriptTagId: 'gid://shopify/ScriptTag/999999999999',
      themeId: 'gid://shopify/OnlineStoreTheme/999999999999',
      urlRedirectId: 'gid://shopify/UrlRedirect/999999999999',
    },
  },
  functions: {
    documentPath: path.join(byIdNotFoundDocumentRoot, 'functions.graphql'),
    variables: {
      shopifyFunctionId: 'gid://shopify/ShopifyFunction/999999999999',
      validationId: 'gid://shopify/Validation/999999999999',
    },
  },
  segments: {
    documentPath: path.join(byIdNotFoundDocumentRoot, 'segments.graphql'),
    variables: {
      customerSegmentMembersQueryId: 'gid://shopify/CustomerSegmentMembersQuery/999999999999',
    },
  },
};

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function runGraphqlCapture(query, variables = {}) {
  const result = await runGraphqlRequest(query, variables);
  return {
    status: result.status,
    payload: result.payload,
  };
}

function operationNameFromDocument(document) {
  return /\b(?:query|mutation)\s+([_A-Za-z][_0-9A-Za-z]*)/u.exec(document)?.[1] ?? 'AnonymousOperation';
}

async function captureByIdNotFoundCases() {
  const cases = {};
  const upstreamCalls = [];

  for (const [caseName, caseConfig] of Object.entries(byIdNotFoundCaseConfigs)) {
    const query = await readFile(caseConfig.documentPath, 'utf8');
    const result = await runGraphqlCapture(query, caseConfig.variables);
    cases[caseName] = {
      query,
      variables: caseConfig.variables,
      response: {
        status: result.status,
        body: result.payload,
      },
    };
    upstreamCalls.push({
      operationName: operationNameFromDocument(query),
      variables: caseConfig.variables,
      query,
      response: {
        status: result.status,
        body: result.payload,
      },
    });
  }

  return { cases, upstreamCalls };
}

const rootTypeIntrospectionQuery = `#graphql
  query AdminPlatformUtilityRootTypes {
    nodeInterface: __type(name: "Node") {
      possibleTypes {
        name
      }
    }
    queryRoot: __type(name: "QueryRoot") {
      fields {
        name
        type {
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
        args {
          name
          type {
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
    }
    mutationRoot: __type(name: "Mutation") {
      fields {
        name
        type {
          kind
          name
          ofType {
            kind
            name
          }
        }
        args {
          name
          type {
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
  }
`;

const publicApiVersionsQuery = `#graphql
  query PublicApiVersionsRead {
    publicApiVersions {
      handle
      displayName
      supported
    }
  }
`;

const nodeNoDataQuery = `#graphql
  query NodeNoDataRead($ids: [ID!]!) {
    node(id: "gid://shopify/Product/0") {
      __typename
      id
    }
    nodes(ids: $ids) {
      __typename
      id
    }
  }
`;

const nodeMalformedMissingIdQuery = `#graphql
  query AdminPlatformNodeMalformedGid {
    node(id: "gid://shopify/Product") {
      __typename
      id
    }
  }
`;

const nodesMalformedMixedQuery = `#graphql
  query AdminPlatformNodesMalformedGid {
    nodes(ids: ["gid://shopify/Product/0", "gid://shopify/Product", "gid://shopify/UnknownType/123"]) {
      __typename
      id
    }
  }
`;

const jobDomainNoDataQuery = `#graphql
  query JobDomainNoDataRead($domainId: ID!, $jobId: ID!) {
    domain(id: $domainId) {
      id
      host
      url
      sslEnabled
    }
    job(id: $jobId) {
      id
      done
      query {
        __typename
      }
    }
  }
`;

const backupRegionQuery = `#graphql
  query BackupRegionRead {
    backupRegion {
      __typename
      id
      name
      ... on MarketRegionCountry {
        code
      }
    }
  }
`;

const backupRegionAccessScopesHydrateQuery =
  'query BackupRegionAccessScopes { currentAppInstallation { accessScopes { handle } } }';

const backupRegionMarketsHydrateQuery = `query BackupRegionMarketsHydrate($first: Int!, $regionsFirst: Int!) {
  markets(first: $first) {
    nodes {
      id
      name
      handle
      status
      type
      conditions {
        conditionTypes
        regionsCondition {
          regions(first: $regionsFirst) {
            nodes {
              __typename
              id
              name
              ... on MarketRegionCountry {
                code
              }
            }
          }
        }
      }
    }
  }
}`;

const backupRegionCurrentHydrateQuery = `query BackupRegionCurrentHydrate {
  backupRegion {
    __typename
    id
    name
    ... on MarketRegionCountry {
      code
    }
  }
}`;

const taxonomyEmptySearchQuery = `#graphql
  query TaxonomyEmptySearchRead {
    taxonomy {
      categories(first: 2, search: "zzzzzz-no-match-har-315") {
        nodes {
          id
          name
          fullName
          isRoot
          isLeaf
          level
          parentId
          ancestorIds
          childrenIds
          isArchived
        }
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
  }
`;

const taxonomyCatalogFirstPageQuery = `#graphql
  query TaxonomyCatalogFirstPageRead {
    taxonomy {
      categories(first: 4) {
        nodes {
          id
          name
          fullName
          isRoot
          isLeaf
          level
          parentId
          ancestorIds
          childrenIds
          isArchived
        }
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
  }
`;

const taxonomyCatalogNextPageQuery = `#graphql
  query TaxonomyCatalogNextPageRead($after: String!) {
    taxonomy {
      categories(first: 4, after: $after) {
        nodes {
          id
          name
          fullName
          isRoot
          isLeaf
          level
          parentId
          ancestorIds
          childrenIds
          isArchived
        }
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
  }
`;

const taxonomySearchApparelQuery = `#graphql
  query TaxonomySearchApparelRead {
    taxonomy {
      categories(first: 4, search: "apparel") {
        nodes {
          id
          name
          fullName
          isRoot
          isLeaf
          level
          parentId
          ancestorIds
          childrenIds
          isArchived
        }
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
  }
`;

const taxonomySearchApparelOverflowSeedQuery = `#graphql
  query TaxonomySearchApparelOverflowSeedRead {
    taxonomy {
      categories(first: 5, search: "apparel") {
        nodes {
          id
          name
          fullName
          isRoot
          isLeaf
          level
          parentId
          ancestorIds
          childrenIds
          isArchived
        }
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
  }
`;

const taxonomyHierarchyAndNodeReadsQuery = `#graphql
  query TaxonomyHierarchyAndNodeReads {
    taxonomy {
      children: categories(first: 5, childrenOf: "gid://shopify/TaxonomyCategory/ap-2") {
        nodes {
          id
          name
          fullName
          isRoot
          isLeaf
          level
          parentId
          ancestorIds
          childrenIds
          isArchived
        }
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
      descendants: categories(first: 5, descendantsOf: "gid://shopify/TaxonomyCategory/ap") {
        nodes {
          id
          name
          fullName
          isRoot
          isLeaf
          level
          parentId
          ancestorIds
          childrenIds
          isArchived
        }
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
      siblings: categories(first: 5, siblingsOf: "gid://shopify/TaxonomyCategory/ap-2-6") {
        nodes {
          id
          name
          fullName
          isRoot
          isLeaf
          level
          parentId
          ancestorIds
          childrenIds
          isArchived
        }
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
      missingChildren: categories(first: 5, childrenOf: "gid://shopify/TaxonomyCategory/missing") {
        nodes {
          id
          name
          fullName
          isRoot
          isLeaf
          level
          parentId
          ancestorIds
          childrenIds
          isArchived
        }
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
    node(id: "gid://shopify/TaxonomyCategory/aa") {
      __typename
      id
      ... on TaxonomyCategory {
        name
        fullName
        isRoot
        isLeaf
        level
        parentId
        ancestorIds
        childrenIds
        isArchived
      }
    }
    nodes(ids: ["gid://shopify/TaxonomyCategory/ap-2-6", "gid://shopify/TaxonomyCategory/missing"]) {
      __typename
      id
      ... on TaxonomyCategory {
        name
        fullName
        isRoot
        isLeaf
        level
        parentId
        ancestorIds
        childrenIds
        isArchived
      }
    }
  }
`;

const taxonomyHierarchySiblingOverflowSeedQuery = `#graphql
  query TaxonomyHierarchySiblingOverflowSeed {
    taxonomy {
      siblings: categories(first: 6, siblingsOf: "gid://shopify/TaxonomyCategory/ap-2-6") {
        nodes {
          id
          name
          fullName
          isRoot
          isLeaf
          level
          parentId
          ancestorIds
          childrenIds
          isArchived
        }
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
  }
`;

const supportedNodeSeedQuery = `#graphql
  query SupportedNodeSeedRead {
    products(first: 1) {
      nodes {
        id
        title
        handle
      }
    }
    collections(first: 1) {
      nodes {
        id
        title
        handle
      }
    }
    customers(first: 1) {
      nodes {
        id
        displayName
        email
      }
    }
    locations(first: 1) {
      nodes {
        id
        name
        isActive
      }
    }
  }
`;

const staffAccessBlockerQuery = `#graphql
  query StaffUtilityRead {
    staffMember {
      id
      exists
      active
      isShopOwner
      accountType
    }
    staffMembers(first: 1) {
      nodes {
        id
        exists
        active
        isShopOwner
        accountType
      }
      edges {
        cursor
        node {
          id
          exists
          active
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

const flowTriggerInvalidHandleMutation = `#graphql
  mutation FlowTriggerReceiveInvalid {
    flowTriggerReceive(handle: "har-374-missing", payload: { test: "value" }) {
      userErrors {
        field
        message
      }
    }
  }
`;

const flowTriggerBodyAndHandleConflictMutation = `#graphql
  mutation FlowTriggerReceiveBodyAndHandleConflict {
    flowTriggerReceive(body: "{\\"trigger_id\\":\\"abc\\",\\"properties\\":{}}", handle: "test") {
      userErrors {
        field
        message
      }
    }
  }
`;

const flowTriggerEmptyHandleEmptyBodyMutation = `#graphql
  mutation FlowTriggerReceiveEmptyHandleEmptyBody {
    flowTriggerReceive {
      userErrors {
        field
        message
      }
    }
  }
`;

const flowTriggerPayloadOnlyNoHandleMutation = `#graphql
  mutation FlowTriggerReceivePayloadOnlyNoHandle {
    flowTriggerReceive(payload: { test: "value" }) {
      userErrors {
        field
        message
      }
    }
  }
`;

const flowTriggerEmptyHandleStringMutation = `#graphql
  mutation FlowTriggerReceiveEmptyHandleString {
    flowTriggerReceive(handle: "") {
      userErrors {
        field
        message
      }
    }
  }
`;

const flowTriggerOversizeMutation = `#graphql
  mutation FlowTriggerReceiveOversize($payload: JSON) {
    flowTriggerReceive(handle: "har-374-missing", payload: $payload) {
      userErrors {
        field
        message
      }
    }
  }
`;

const flowTriggerBodyNotJsonMutation = `#graphql
  mutation FlowTriggerReceiveBodyNotJson {
    flowTriggerReceive(body: "not json") {
      userErrors {
        field
        message
      }
    }
  }
`;

const flowTriggerBodyPropertiesNotObjectMutation = `#graphql
  mutation FlowTriggerReceiveBodyPropertiesNotObject {
    flowTriggerReceive(body: "{\\"properties\\":\\"oops\\"}") {
      userErrors {
        field
        message
      }
    }
  }
`;

const flowTriggerBodyMissingResourceUrlMutation = `#graphql
  mutation FlowTriggerReceiveBodyMissingResourceUrl {
    flowTriggerReceive(body: "{\\"trigger_id\\":\\"abc\\",\\"resources\\":[{\\"name\\":\\"x\\"}],\\"properties\\":{}}") {
      userErrors {
        field
        message
      }
    }
  }
`;

const flowGenerateUnknownMutation = `mutation {
  flowGenerateSignature(id: "gid://shopify/FlowTrigger/0", payload: "{}") {
    signature
    userErrors {
      field
      message
    }
  }
}`;

function backupRegionUpdateIdempotentMutation(countryCode) {
  return `mutation BackupRegionUpdateIdempotent {
  backupRegionUpdate(region: { countryCode: ${countryCode} }) {
    backupRegion {
      __typename
      id
      name
      ... on MarketRegionCountry {
        code
      }
    }
    userErrors {
      field
      message
      code
    }
  }
}
`;
}

const backupRegionUpdateFallbackMutation = `#graphql
  mutation BackupRegionUpdateIdempotent {
    backupRegionUpdate(region: { countryCode: CA }) {
      backupRegion {
        __typename
        id
        name
        ... on MarketRegionCountry {
          code
        }
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const backupRegionUpdateInvalidMutation = `#graphql
  mutation BackupRegionUpdateInvalid {
    backupRegionUpdate(region: { countryCode: ZZ }) {
      backupRegion {
        __typename
        id
        name
        ... on MarketRegionCountry {
          code
        }
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const introspection = await runGraphqlCapture(rootTypeIntrospectionQuery);
function unwrapNamedType(type) {
  let current = type;
  while (current?.ofType) {
    current = current.ofType;
  }
  return typeof current?.name === 'string' ? current.name : null;
}

function hasIdArgument(field) {
  return field.args?.some((arg) => arg.name === 'id' && unwrapNamedType(arg.type) === 'ID') ?? false;
}

const utilityRootNames = new Set([
  'backupRegion',
  'backupRegionUpdate',
  'domain',
  'flowGenerateSignature',
  'flowTriggerReceive',
  'job',
  'node',
  'nodes',
  'publicApiVersions',
  'staffMember',
  'staffMembers',
  'taxonomy',
]);

const rootTypes = {
  queryRoot: introspection.payload.data?.queryRoot?.fields?.filter((field) => utilityRootNames.has(field.name)) ?? [],
  mutationRoot:
    introspection.payload.data?.mutationRoot?.fields?.filter((field) => utilityRootNames.has(field.name)) ?? [],
};
const nodePossibleTypeNames = new Set(
  introspection.payload.data?.nodeInterface?.possibleTypes
    ?.map((type) => type.name)
    .filter((name) => typeof name === 'string') ?? [],
);
const nodeCandidateRootFields =
  introspection.payload.data?.queryRoot?.fields
    ?.filter((field) => {
      const returnType = unwrapNamedType(field.type);
      return hasIdArgument(field) && returnType !== null && nodePossibleTypeNames.has(returnType);
    })
    .map((field) => ({
      name: field.name,
      typeName: unwrapNamedType(field.type),
    }))
    .sort((left, right) => left.name.localeCompare(right.name)) ?? [];

const supportedNodeSeed = {
  query: supportedNodeSeedQuery,
  result: await runGraphqlCapture(supportedNodeSeedQuery),
};
const supportedNodeSeedData = supportedNodeSeed.result.payload.data ?? {};
const supportedNodeIds = [
  supportedNodeSeedData.products?.nodes?.[0]?.id,
  supportedNodeSeedData.collections?.nodes?.[0]?.id,
  supportedNodeSeedData.customers?.nodes?.[0]?.id,
  supportedNodeSeedData.locations?.nodes?.[0]?.id,
].filter((id) => typeof id === 'string');
const taxonomyCatalogFirstPage = {
  query: taxonomyCatalogFirstPageQuery,
  result: await runGraphqlCapture(taxonomyCatalogFirstPageQuery),
};
const taxonomyCatalogAfterCursor =
  taxonomyCatalogFirstPage.result.payload.data?.taxonomy?.categories?.pageInfo?.endCursor;
const backupRegionCapture = {
  query: backupRegionQuery,
  result: await runGraphqlCapture(backupRegionQuery),
};
const currentBackupRegionCountryCode = backupRegionCapture.result.payload.data?.backupRegion?.code;
const backupRegionUpdateCurrentCountryMutation =
  typeof currentBackupRegionCountryCode === 'string' && /^[A-Z]{2}$/u.test(currentBackupRegionCountryCode)
    ? backupRegionUpdateIdempotentMutation(currentBackupRegionCountryCode)
    : backupRegionUpdateFallbackMutation;

const captures = {
  publicApiVersions: {
    query: publicApiVersionsQuery,
    result: await runGraphqlCapture(publicApiVersionsQuery),
  },
  nodeNoData: {
    query: nodeNoDataQuery,
    variables: {
      ids: ['gid://shopify/Product/0', 'gid://shopify/Job/0', 'gid://shopify/Domain/0'],
    },
    result: await runGraphqlCapture(nodeNoDataQuery, {
      ids: ['gid://shopify/Product/0', 'gid://shopify/Job/0', 'gid://shopify/Domain/0'],
    }),
  },
  nodeMalformedMissingId: {
    query: nodeMalformedMissingIdQuery,
    result: await runGraphqlCapture(nodeMalformedMissingIdQuery),
  },
  nodesMalformedMixed: {
    query: nodesMalformedMixedQuery,
    result: await runGraphqlCapture(nodesMalformedMixedQuery),
  },
  jobDomainNoData: {
    query: jobDomainNoDataQuery,
    variables: {
      domainId: 'gid://shopify/Domain/0',
      jobId: 'gid://shopify/Job/0',
    },
    result: await runGraphqlCapture(jobDomainNoDataQuery, {
      domainId: 'gid://shopify/Domain/0',
      jobId: 'gid://shopify/Job/0',
    }),
  },
  backupRegion: backupRegionCapture,
  backupRegionAccessScopesHydrate: {
    query: backupRegionAccessScopesHydrateQuery,
    result: await runGraphqlCapture(backupRegionAccessScopesHydrateQuery),
  },
  backupRegionMarketsHydrate: {
    query: backupRegionMarketsHydrateQuery,
    variables: { first: 250, regionsFirst: 250 },
    result: await runGraphqlCapture(backupRegionMarketsHydrateQuery, { first: 250, regionsFirst: 250 }),
  },
  backupRegionCurrentHydrate: {
    query: backupRegionCurrentHydrateQuery,
    result: await runGraphqlCapture(backupRegionCurrentHydrateQuery),
  },
  taxonomyEmptySearch: {
    query: taxonomyEmptySearchQuery,
    result: await runGraphqlCapture(taxonomyEmptySearchQuery),
  },
  taxonomyCatalogFirstPage,
  taxonomyCatalogNextPage: {
    query: taxonomyCatalogNextPageQuery,
    variables: {
      after: taxonomyCatalogAfterCursor,
    },
    result:
      typeof taxonomyCatalogAfterCursor === 'string'
        ? await runGraphqlCapture(taxonomyCatalogNextPageQuery, { after: taxonomyCatalogAfterCursor })
        : {
            status: 0,
            payload: {
              errors: [{ message: 'taxonomy catalog first page did not return an endCursor' }],
            },
          },
  },
  taxonomySearchApparel: {
    query: taxonomySearchApparelQuery,
    result: await runGraphqlCapture(taxonomySearchApparelQuery),
  },
  taxonomySearchApparelOverflowSeed: {
    query: taxonomySearchApparelOverflowSeedQuery,
    result: await runGraphqlCapture(taxonomySearchApparelOverflowSeedQuery),
  },
  taxonomyHierarchyAndNodeReads: {
    query: taxonomyHierarchyAndNodeReadsQuery,
    result: await runGraphqlCapture(taxonomyHierarchyAndNodeReadsQuery),
  },
  taxonomyHierarchySiblingOverflowSeed: {
    query: taxonomyHierarchySiblingOverflowSeedQuery,
    result: await runGraphqlCapture(taxonomyHierarchySiblingOverflowSeedQuery),
  },
  supportedNodeSeeds: supportedNodeSeed,
  supportedNodes: {
    query: supportedNodeReadsDocument,
    variables: {
      ids: supportedNodeIds,
    },
    result: await runGraphqlCapture(supportedNodeReadsDocument, {
      ids: supportedNodeIds,
    }),
  },
  staffAccessBlocker: {
    query: staffAccessBlockerQuery,
    result: await runGraphqlCapture(staffAccessBlockerQuery),
  },
  flowTriggerReceiveInvalid: {
    query: flowTriggerInvalidHandleMutation,
    result: await runGraphqlCapture(flowTriggerInvalidHandleMutation),
  },
  flowTriggerReceiveBodyAndHandleConflict: {
    query: flowTriggerBodyAndHandleConflictMutation,
    result: await runGraphqlCapture(flowTriggerBodyAndHandleConflictMutation),
  },
  flowTriggerReceiveEmptyHandleEmptyBody: {
    query: flowTriggerEmptyHandleEmptyBodyMutation,
    result: await runGraphqlCapture(flowTriggerEmptyHandleEmptyBodyMutation),
  },
  flowTriggerReceivePayloadOnlyNoHandle: {
    query: flowTriggerPayloadOnlyNoHandleMutation,
    result: await runGraphqlCapture(flowTriggerPayloadOnlyNoHandleMutation),
  },
  flowTriggerReceiveEmptyHandleString: {
    query: flowTriggerEmptyHandleStringMutation,
    result: await runGraphqlCapture(flowTriggerEmptyHandleStringMutation),
  },
  flowTriggerReceiveOversize: {
    query: flowTriggerOversizeMutation,
    variables: {
      payload: { value: 'x'.repeat(50_001) },
    },
    result: await runGraphqlCapture(flowTriggerOversizeMutation, {
      payload: { value: 'x'.repeat(50_001) },
    }),
  },
  flowTriggerReceiveBodyNotJson: {
    query: flowTriggerBodyNotJsonMutation,
    result: await runGraphqlCapture(flowTriggerBodyNotJsonMutation),
  },
  flowTriggerReceiveBodyPropertiesNotObject: {
    query: flowTriggerBodyPropertiesNotObjectMutation,
    result: await runGraphqlCapture(flowTriggerBodyPropertiesNotObjectMutation),
  },
  flowTriggerReceiveBodyMissingResourceUrl: {
    query: flowTriggerBodyMissingResourceUrlMutation,
    result: await runGraphqlCapture(flowTriggerBodyMissingResourceUrlMutation),
  },
  flowGenerateSignatureUnknown: {
    query: flowGenerateUnknownMutation,
    result: await runGraphqlCapture(flowGenerateUnknownMutation),
  },
  backupRegionUpdateIdempotent: {
    query: backupRegionUpdateCurrentCountryMutation,
    result: await runGraphqlCapture(backupRegionUpdateCurrentCountryMutation),
  },
  backupRegionAfterIdempotentUpdate: {
    query: backupRegionQuery,
    result: await runGraphqlCapture(backupRegionQuery),
  },
  backupRegionUpdateInvalid: {
    query: backupRegionUpdateInvalidMutation,
    result: await runGraphqlCapture(backupRegionUpdateInvalidMutation),
  },
  adminPlatformUtilityReads: {
    query: utilityReadsDocument,
    variables: utilityReadsVariables,
    result: await runGraphqlCapture(utilityReadsDocument, utilityReadsVariables),
  },
};

const byIdNotFoundCapture = await captureByIdNotFoundCases();
const capturedAt = new Date().toISOString();
const utilityUpstreamCalls = [
  {
    operationName: 'BackupRegionAccessScopes',
    variables: {},
    query: backupRegionAccessScopesHydrateQuery,
    response: {
      status: captures.backupRegionAccessScopesHydrate.result.status,
      body: captures.backupRegionAccessScopesHydrate.result.payload,
    },
  },
  {
    operationName: 'BackupRegionMarketsHydrate',
    variables: captures.backupRegionMarketsHydrate.variables,
    query: backupRegionMarketsHydrateQuery,
    response: {
      status: captures.backupRegionMarketsHydrate.result.status,
      body: captures.backupRegionMarketsHydrate.result.payload,
    },
  },
  {
    operationName: 'BackupRegionCurrentHydrate',
    variables: {},
    query: backupRegionCurrentHydrateQuery,
    response: {
      status: captures.backupRegionCurrentHydrate.result.status,
      body: captures.backupRegionCurrentHydrate.result.payload,
    },
  },
  {
    operationName: 'SupportedNodeRead',
    variables: captures.supportedNodes.variables,
    query: supportedNodeReadsDocument,
    response: {
      status: 200,
      body: {
        data: {
          nodes: captures.supportedNodes.result.payload.data?.nodes ?? [],
        },
      },
    },
  },
  {
    operationName: 'AdminPlatformUtilityReads',
    variables: captures.adminPlatformUtilityReads.variables,
    query: utilityReadsDocument,
    response: {
      status: captures.adminPlatformUtilityReads.result.status,
      body: captures.adminPlatformUtilityReads.result.payload,
    },
  },
];
const taxonomyHierarchyUpstreamCalls = [
  {
    operationName: 'AdminPlatformTaxonomyHierarchyNodeReads',
    variables: {},
    query: taxonomyHierarchyDocument,
    response: {
      status: 200,
      body: captures.taxonomyHierarchyAndNodeReads.result.payload,
    },
  },
];
const captureOutput = {
  capturedAt,
  storeDomain,
  apiVersion,
  introspection: {
    status: introspection.status,
    nodeInterface: introspection.payload.data?.nodeInterface ?? null,
    nodeCandidateRootFields,
    rootTypes,
  },
  nodeSeeds: {
    products: supportedNodeSeedData.products?.nodes ?? [],
    collections: supportedNodeSeedData.collections?.nodes ?? [],
    customers: supportedNodeSeedData.customers?.nodes ?? [],
    locations: supportedNodeSeedData.locations?.nodes ?? [],
  },
  captures,
  upstreamCalls: utilityUpstreamCalls,
};
const taxonomyHierarchyOutput = {
  capturedAt,
  storeDomain,
  apiVersion,
  captures: {
    taxonomyHierarchyAndNodeReads: captures.taxonomyHierarchyAndNodeReads,
    taxonomyHierarchySiblingOverflowSeed: captures.taxonomyHierarchySiblingOverflowSeed,
  },
  upstreamCalls: taxonomyHierarchyUpstreamCalls,
};
const byIdNotFoundOutput = {
  capturedAt,
  storeDomain,
  apiVersion,
  cases: byIdNotFoundCapture.cases,
  upstreamCalls: byIdNotFoundCapture.upstreamCalls,
};

await mkdir(outputDir, { recursive: true });
await writeFile(backupRegionUpdateIdempotentDocumentPath, backupRegionUpdateCurrentCountryMutation, 'utf8');
await writeFile(outputPath, `${JSON.stringify(captureOutput, null, 2)}\n`, 'utf8');
await writeFile(taxonomyHierarchyOutputPath, `${JSON.stringify(taxonomyHierarchyOutput, null, 2)}\n`, 'utf8');
await writeFile(byIdNotFoundOutputPath, `${JSON.stringify(byIdNotFoundOutput, null, 2)}\n`, 'utf8');

console.log(`Wrote ${backupRegionUpdateIdempotentDocumentPath}`);
console.log(`Wrote ${outputPath}`);
console.log(`Wrote ${taxonomyHierarchyOutputPath}`);
console.log(`Wrote ${byIdNotFoundOutputPath}`);

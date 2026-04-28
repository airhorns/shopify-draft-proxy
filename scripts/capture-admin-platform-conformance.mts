// @ts-nocheck
/* oxlint-disable no-console -- CLI scripts intentionally write capture status to stdio. */
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
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'admin-platform');
const outputPath = path.join(outputDir, 'admin-platform-utility-roots.json');

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

const rootTypeIntrospectionQuery = `#graphql
  query AdminPlatformUtilityRootTypes {
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

const flowGenerateUnknownMutation = `mutation {
  flowGenerateSignature(id: "gid://shopify/FlowTrigger/0", payload: "{}") {
    signature
    userErrors {
      field
      message
    }
  }
}`;

const backupRegionUpdateIdempotentMutation = `#graphql
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
  backupRegion: {
    query: backupRegionQuery,
    result: await runGraphqlCapture(backupRegionQuery),
  },
  taxonomyEmptySearch: {
    query: taxonomyEmptySearchQuery,
    result: await runGraphqlCapture(taxonomyEmptySearchQuery),
  },
  staffAccessBlocker: {
    query: staffAccessBlockerQuery,
    result: await runGraphqlCapture(staffAccessBlockerQuery),
  },
  flowTriggerReceiveInvalid: {
    query: flowTriggerInvalidHandleMutation,
    result: await runGraphqlCapture(flowTriggerInvalidHandleMutation),
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
  flowGenerateSignatureUnknown: {
    query: flowGenerateUnknownMutation,
    result: await runGraphqlCapture(flowGenerateUnknownMutation),
  },
  backupRegionUpdateIdempotent: {
    query: backupRegionUpdateIdempotentMutation,
    result: await runGraphqlCapture(backupRegionUpdateIdempotentMutation),
  },
  backupRegionAfterIdempotentUpdate: {
    query: backupRegionQuery,
    result: await runGraphqlCapture(backupRegionQuery),
  },
  backupRegionUpdateInvalid: {
    query: backupRegionUpdateInvalidMutation,
    result: await runGraphqlCapture(backupRegionUpdateInvalidMutation),
  },
};

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      introspection: {
        status: introspection.status,
        rootTypes,
      },
      captures,
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(`Wrote ${outputPath}`);

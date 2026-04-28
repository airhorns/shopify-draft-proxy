import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type RecordedGraphqlRequest = {
  query: string;
  variables?: Record<string, unknown>;
  status: number;
  payload: unknown;
};

const APP_ROOT_INTROSPECTION_QUERY = `#graphql
  query AppBillingRootIntrospection {
    queryRoot: __type(name: "QueryRoot") {
      fields {
        name
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
    mutationRoot: __type(name: "Mutation") {
      fields {
        name
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
`;

const CURRENT_APP_INSTALLATION_QUERY = `#graphql
  query CurrentAppBillingSafeRead($unknownInstallationId: ID!, $first: Int!) {
    currentAppInstallation {
      id
      launchUrl
      uninstallUrl
      accessScopes {
        handle
        description
      }
      app {
        id
        apiKey
        handle
        title
        developerName
        embedded
        previouslyInstalled
        requestedAccessScopes {
          handle
          description
        }
      }
      activeSubscriptions {
        id
        name
        status
        test
        trialDays
        currentPeriodEnd
        createdAt
        lineItems {
          id
          plan {
            pricingDetails {
              __typename
              ... on AppRecurringPricing {
                price {
                  amount
                  currencyCode
                }
                interval
                planHandle
              }
              ... on AppUsagePricing {
                cappedAmount {
                  amount
                  currencyCode
                }
                balanceUsed {
                  amount
                  currencyCode
                }
                interval
                terms
              }
            }
          }
        }
      }
      allSubscriptions(first: $first) {
        nodes {
          id
          name
          status
          test
          trialDays
          currentPeriodEnd
          createdAt
        }
        edges {
          cursor
          node {
            id
          }
        }
        pageInfo {
          hasNextPage
          hasPreviousPage
          startCursor
          endCursor
        }
      }
      oneTimePurchases(first: $first) {
        nodes {
          id
          name
          status
          test
          createdAt
          price {
            amount
            currencyCode
          }
        }
        edges {
          cursor
          node {
            id
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
    missingAppInstallation: appInstallation(id: $unknownInstallationId) {
      id
    }
  }
`;

const APP_LOOKUP_QUERY = `#graphql
  query AppLookupShapes(
    $id: ID!
    $missingId: ID!
    $handle: String!
    $missingHandle: String!
    $apiKey: String!
    $missingApiKey: String!
  ) {
    appById: app(id: $id) {
      id
      apiKey
      handle
      title
      developerName
      embedded
      previouslyInstalled
      requestedAccessScopes {
        handle
        description
      }
    }
    missingAppById: app(id: $missingId) {
      id
    }
    appByHandle(handle: $handle) {
      id
      apiKey
      handle
      title
      developerName
      embedded
      previouslyInstalled
    }
    missingAppByHandle: appByHandle(handle: $missingHandle) {
      id
    }
    appByKey(apiKey: $apiKey) {
      id
      apiKey
      handle
      title
      developerName
      embedded
      previouslyInstalled
    }
    missingAppByKey: appByKey(apiKey: $missingApiKey) {
      id
    }
  }
`;

const APP_INSTALLATION_DETAIL_QUERY = `#graphql
  query AppInstallationDetail($id: ID!) {
    appInstallation(id: $id) {
      id
      launchUrl
      uninstallUrl
      app {
        id
        apiKey
        handle
        title
        developerName
        embedded
      }
      accessScopes {
        handle
        description
      }
      activeSubscriptions {
        id
        name
        status
        test
      }
    }
  }
`;

const APP_INSTALLATIONS_ACCESS_PROBE_QUERY = `#graphql
  query AppInstallationsAccessProbe($first: Int!) {
    appInstallations(first: $first) {
      nodes {
        id
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

const APP_ROOT_NAMES = new Set([
  'app',
  'appByHandle',
  'appByKey',
  'appInstallation',
  'appInstallations',
  'currentAppInstallation',
  'appPurchaseOneTimeCreate',
  'appSubscriptionCreate',
  'appSubscriptionCancel',
  'appSubscriptionLineItemUpdate',
  'appSubscriptionTrialExtend',
  'appUsageRecordCreate',
  'appRevokeAccessScopes',
  'appUninstall',
  'delegateAccessTokenCreate',
  'delegateAccessTokenDestroy',
]);

function pickRelevantRootFields(payload: unknown) {
  const data = (payload as { data?: { queryRoot?: { fields?: unknown[] }; mutationRoot?: { fields?: unknown[] } } })
    .data;
  const queryFields = Array.isArray(data?.queryRoot?.fields) ? data.queryRoot.fields : [];
  const mutationFields = Array.isArray(data?.mutationRoot?.fields) ? data.mutationRoot.fields : [];

  return {
    queryRoots: queryFields.filter((field) => APP_ROOT_NAMES.has((field as { name?: string }).name ?? '')),
    mutationRoots: mutationFields.filter((field) => APP_ROOT_NAMES.has((field as { name?: string }).name ?? '')),
  };
}

function readPath<T>(value: unknown, pathSegments: string[]): T | null {
  let current = value;
  for (const segment of pathSegments) {
    if (typeof current !== 'object' || current === null || !(segment in current)) {
      return null;
    }
    current = (current as Record<string, unknown>)[segment];
  }
  return current as T;
}

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function record(query: string, variables: Record<string, unknown> = {}): Promise<RecordedGraphqlRequest> {
  const { status, payload } = await runGraphqlRequest(query, variables);
  return {
    query,
    ...(Object.keys(variables).length > 0 ? { variables } : {}),
    status,
    payload,
  };
}

const rootIntrospection = await record(APP_ROOT_INTROSPECTION_QUERY);
const currentInstallation = await record(CURRENT_APP_INSTALLATION_QUERY, {
  unknownInstallationId: 'gid://shopify/AppInstallation/0',
  first: 5,
});
const currentPayload = currentInstallation.payload;
const appId = readPath<string>(currentPayload, ['data', 'currentAppInstallation', 'app', 'id']);
const appHandle = readPath<string>(currentPayload, ['data', 'currentAppInstallation', 'app', 'handle']);
const appApiKey = readPath<string>(currentPayload, ['data', 'currentAppInstallation', 'app', 'apiKey']);
const installationId = readPath<string>(currentPayload, ['data', 'currentAppInstallation', 'id']);

if (!appId || !appHandle || !appApiKey || !installationId) {
  throw new Error('currentAppInstallation did not return the app/install identity needed for follow-up probes.');
}

const appLookups = await record(APP_LOOKUP_QUERY, {
  id: appId,
  missingId: 'gid://shopify/App/0',
  handle: appHandle,
  missingHandle: `missing-${appHandle}`,
  apiKey: appApiKey,
  missingApiKey: '00000000000000000000000000000000',
});
const appInstallationDetail = await record(APP_INSTALLATION_DETAIL_QUERY, { id: installationId });
const appInstallationsAccessProbe = await record(APP_INSTALLATIONS_ACCESS_PROBE_QUERY, { first: 5 });

const fixture = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  notes: [
    'HAR-301 safe app/billing/access read capture. No billing, uninstall, scope revocation, or delegated-token mutations are executed.',
    'The active conformance app has no active subscriptions, no allSubscriptions rows, and no oneTimePurchases rows, so the fixture captures Shopify empty/no-data billing connection behavior for the installed app.',
    'The current credential can read currentAppInstallation, app/appByHandle/appByKey, and appInstallation for the active install. appInstallations returns ACCESS_DENIED and is recorded as a credential/access blocker.',
  ],
  rootIntrospection: {
    query: rootIntrospection.query,
    status: rootIntrospection.status,
    errors:
      (rootIntrospection.payload as { errors?: unknown }).errors === undefined
        ? null
        : (rootIntrospection.payload as { errors?: unknown }).errors,
    relevantRoots: pickRelevantRootFields(rootIntrospection.payload),
  },
  currentInstallation,
  appLookups,
  appInstallationDetail,
  appInstallationsAccessProbe,
};

const outputPath = path.join(
  process.cwd(),
  'fixtures',
  'conformance',
  storeDomain,
  apiVersion,
  'apps',
  'app-billing-access-read.json',
);

await mkdir(path.dirname(outputPath), { recursive: true });
await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');

// oxlint-disable-next-line no-console -- CLI capture scripts intentionally write the generated fixture path.
console.log(outputPath);

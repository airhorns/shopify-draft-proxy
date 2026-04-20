/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';

const requiredVars = [
  'SHOPIFY_CONFORMANCE_STORE_DOMAIN',
  'SHOPIFY_CONFORMANCE_ADMIN_ORIGIN',
  'SHOPIFY_CONFORMANCE_ADMIN_ACCESS_TOKEN',
];

const missingVars = requiredVars.filter((name) => !process.env[name]);
if (missingVars.length > 0) {
  console.error(`Missing required environment variables: ${missingVars.join(', ')}`);
  process.exit(1);
}

const storeDomain = process.env['SHOPIFY_CONFORMANCE_STORE_DOMAIN'];
const adminOrigin = process.env['SHOPIFY_CONFORMANCE_ADMIN_ORIGIN'];
const adminAccessToken = process.env['SHOPIFY_CONFORMANCE_ADMIN_ACCESS_TOKEN'];
const apiVersion = process.env['SHOPIFY_CONFORMANCE_API_VERSION'] || '2025-01';
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion);
const pendingDir = path.join('pending');
const blockerPath = path.join(pendingDir, 'customer-conformance-protected-data-blocker.md');
const protectedCustomerDataDocsUrl = 'https://shopify.dev/docs/apps/launch/protected-customer-data';

function buildAdminAuthHeaders(token) {
  if (/^shp[a-z]+_/.test(token)) {
    return {
      'X-Shopify-Access-Token': token,
    };
  }

  const bearerToken = token.startsWith('Bearer ') ? token : `Bearer ${token}`;
  return {
    Authorization: bearerToken,
    'X-Shopify-Access-Token': bearerToken,
  };
}

async function clearBlocker() {
  await rm(blockerPath, { force: true });
}

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

const accessScopesQuery = `#graphql
  query CustomerConformanceAccessScopes {
    currentAppInstallation {
      accessScopes {
        handle
      }
    }
  }
`;

const customersCatalogQuery = `#graphql
  query CustomersCatalogConformance($first: Int!) {
    customers(first: $first) {
      edges {
        cursor
        node {
          id
          displayName
          email
          legacyResourceId
          locale
          note
          canDelete
          verifiedEmail
          taxExempt
          state
          numberOfOrders
          amountSpent {
            amount
            currencyCode
          }
          defaultEmailAddress {
            emailAddress
          }
          defaultPhoneNumber {
            phoneNumber
          }
          defaultAddress {
            address1
            city
            province
            country
            zip
            formattedArea
          }
          createdAt
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

const customerDetailQuery = `#graphql
  query CustomerDetailConformance($id: ID!) {
    customer(id: $id) {
      id
      firstName
      lastName
      displayName
      email
      legacyResourceId
      locale
      note
      canDelete
      verifiedEmail
      taxExempt
      state
      tags
      numberOfOrders
      amountSpent {
        amount
        currencyCode
      }
      defaultEmailAddress {
        emailAddress
      }
      defaultPhoneNumber {
        phoneNumber
      }
      defaultAddress {
        address1
        city
        province
        country
        zip
        formattedArea
      }
      createdAt
      updatedAt
    }
  }
`;

const customersSearchQuery = `#graphql
  query CustomersSearchConformance($first: Int!, $query: String!) {
    customers(first: $first, query: $query, sortKey: UPDATED_AT, reverse: true) {
      edges {
        cursor
        node {
          id
          displayName
          email
          legacyResourceId
          verifiedEmail
          state
          tags
          defaultPhoneNumber {
            phoneNumber
          }
          defaultAddress {
            address1
            city
            province
            country
            zip
            formattedArea
          }
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

const customersAdvancedSearchQuery = `#graphql
  query CustomersAdvancedSearchConformance($prefixQuery: String!, $orQuery: String!, $groupedQuery: String!) {
    prefix: customers(first: 2, query: $prefixQuery, sortKey: UPDATED_AT, reverse: true) {
      edges {
        cursor
        node {
          id
          displayName
          email
          legacyResourceId
          verifiedEmail
          state
          tags
          defaultPhoneNumber {
            phoneNumber
          }
          defaultAddress {
            address1
            city
            province
            country
            zip
            formattedArea
          }
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
    orMatches: customers(first: 5, query: $orQuery, sortKey: UPDATED_AT, reverse: true) {
      edges {
        cursor
        node {
          id
          displayName
          email
          legacyResourceId
          verifiedEmail
          state
          tags
          defaultPhoneNumber {
            phoneNumber
          }
          defaultAddress {
            address1
            city
            province
            country
            zip
            formattedArea
          }
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
    groupedExclusion: customers(first: 5, query: $groupedQuery, sortKey: UPDATED_AT, reverse: true) {
      edges {
        cursor
        node {
          id
          displayName
          email
          legacyResourceId
          verifiedEmail
          state
          tags
          defaultPhoneNumber {
            phoneNumber
          }
          defaultAddress {
            address1
            city
            province
            country
            zip
            formattedArea
          }
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

const customersSortKeysQuery = `#graphql
  query CustomersSortKeysConformance($first: Int!) {
    nameOrder: customers(first: $first, sortKey: NAME) {
      edges {
        cursor
        node {
          id
          displayName
          legacyResourceId
          defaultAddress {
            country
            province
            city
            formattedArea
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
    idOrder: customers(first: $first, sortKey: ID) {
      edges {
        cursor
        node {
          id
          displayName
          legacyResourceId
        }
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
    locationOrder: customers(first: $first, sortKey: LOCATION) {
      edges {
        cursor
        node {
          id
          displayName
          legacyResourceId
          defaultAddress {
            country
            province
            city
            formattedArea
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

const customersRelevanceSearchQuery = `#graphql
  query CustomersRelevanceSearchConformance($first: Int!, $query: String!) {
    customers(first: $first, query: $query, sortKey: RELEVANCE) {
      edges {
        cursor
        node {
          id
          displayName
          email
          legacyResourceId
          tags
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

const customersCountQuery = `#graphql
  query CustomersCountConformance($query: String!, $disabledQuery: String!) {
    total: customersCount {
      count
      precision
    }
    matching: customersCount(query: $query) {
      count
      precision
    }
    disabled: customersCount(query: $disabledQuery) {
      count
      precision
    }
  }
`;

function extractGraphqlError(result) {
  const graphqlError = result?.payload?.errors?.[0];
  return graphqlError && typeof graphqlError === 'object' ? graphqlError : null;
}

function isProtectedCustomerDataError(result) {
  const graphqlError = extractGraphqlError(result);
  return !!(
    graphqlError?.extensions?.code === 'ACCESS_DENIED' &&
    typeof graphqlError?.message === 'string' &&
    graphqlError.message.includes('not approved to access the Customer object')
  );
}

async function runGraphqlResult(query, variables = {}) {
  try {
    const payload = await runGraphql(query, variables);
    return { ok: true, status: 200, payload };
  } catch (error) {
    try {
      return { ok: false, ...JSON.parse(error instanceof Error ? error.message : String(error)) };
    } catch {
      throw error;
    }
  }
}

function renderProtectedCustomerDataBlocker({ message, accessScopeHandles, customersProbe, customerProbe }) {
  return [
    '# Customer conformance protected-data blocker',
    '',
    'Attempted to capture live conformance for the remaining customer read family (`customer`, `customers`, `customersCount`).',
    '',
    '## Observed blocker',
    '',
    '- root operations: `customer`, `customers`',
    '- status: `ACCESS_DENIED`',
    `- failing message: ${message}`,
    `- docs: ${protectedCustomerDataDocsUrl}`,
    '',
    '## Scope check',
    '',
    '- `currentAppInstallation.accessScopes` on the active token includes:',
    ...accessScopeHandles.map((scopeHandle) => `  - \`${scopeHandle}\``),
    '- `read_customers` / `write_customers` are already present, so this is not a missing-scope problem.',
    '',
    '## Root-specific probe results',
    '',
    '- `customers(first: ...)` returned the protected-data approval blocker directly.',
    '- `customer(id: ...)` cannot be settled directly without a discoverable live customer id; the fallback probe below only shows that a guessed id returns `null`, so detail capture remains blocked until the approval gate is lifted and a real id can be discovered safely.',
    '',
    '### `customers(first: ...)` probe payload',
    '',
    '```json',
    JSON.stringify(customersProbe, null, 2),
    '```',
    '',
    '### `customer(id: ...)` probe payload',
    '',
    '```json',
    JSON.stringify(customerProbe, null, 2),
    '```',
    '',
    '## Interpretation',
    '',
    'The runtime now supports live-hybrid customer hydration/serialization, but the configured conformance app/token on this host is still blocked by Shopify protected customer data approval. Until that approval exists, live customer detail/catalog fixtures cannot be captured safely from this store.',
    '',
    '## Next step',
    '',
    `1. obtain Shopify protected customer data approval for the conformance app/token (${protectedCustomerDataDocsUrl})`,
    '2. rerun `corepack pnpm conformance:capture-customers`',
    '',
  ].join('\n');
}

try {
  await mkdir(outputDir, { recursive: true });
  await mkdir(pendingDir, { recursive: true });

  const accessScopes = await runGraphql(accessScopesQuery);
  const accessScopeHandles = Array.isArray(accessScopes.data?.currentAppInstallation?.accessScopes)
    ? accessScopes.data.currentAppInstallation.accessScopes
        .map((scope) => (typeof scope?.handle === 'string' ? scope.handle : null))
        .filter(Boolean)
        .sort()
    : [];

  const catalogResult = await runGraphqlResult(customersCatalogQuery, { first: 3 });
  if (isProtectedCustomerDataError(catalogResult)) {
    const failingMessage =
      extractGraphqlError(catalogResult)?.message || 'Protected customer data approval is missing.';
    const customerProbe = await runGraphqlResult(customerDetailQuery, { id: 'gid://shopify/Customer/1' });

    await writeFile(
      blockerPath,
      renderProtectedCustomerDataBlocker({
        message: failingMessage,
        accessScopeHandles,
        customersProbe: catalogResult,
        customerProbe,
      }),
      'utf8',
    );
    console.error(
      JSON.stringify(
        { ok: false, blockerPath, message: failingMessage, docsUrl: protectedCustomerDataDocsUrl },
        null,
        2,
      ),
    );
    process.exit(1);
  }

  if (!catalogResult.ok) {
    throw new Error(JSON.stringify(catalogResult, null, 2));
  }

  const catalog = catalogResult.payload;
  const firstCustomerId = catalog.data?.customers?.edges?.[0]?.node?.id;
  if (typeof firstCustomerId !== 'string' || !firstCustomerId) {
    throw new Error('Customer catalog capture returned no customer ids.');
  }

  const detail = await runGraphql(customerDetailQuery, { id: firstCustomerId });
  const search = await runGraphql(customersSearchQuery, { first: 2, query: 'state:DISABLED' });
  const advancedSearch = await runGraphql(customersAdvancedSearchQuery, {
    prefixQuery: 'How*',
    orQuery: '(tag:VIP OR tag:referral) state:DISABLED',
    groupedQuery: 'state:DISABLED -(tag:VIP OR tag:referral)',
  });
  const sortKeys = await runGraphql(customersSortKeysQuery, { first: 5 });
  const relevanceSearch = await runGraphql(customersRelevanceSearchQuery, { first: 5, query: 'egnition' });
  const counts = await runGraphql(customersCountQuery, {
    query: 'email:grace@example.com',
    disabledQuery: 'state:DISABLED',
  });

  await writeFile(path.join(outputDir, 'customers-catalog.json'), `${JSON.stringify(catalog, null, 2)}\n`, 'utf8');
  await writeFile(path.join(outputDir, 'customer-detail.json'), `${JSON.stringify(detail, null, 2)}\n`, 'utf8');
  await writeFile(path.join(outputDir, 'customers-search.json'), `${JSON.stringify(search, null, 2)}\n`, 'utf8');
  await writeFile(
    path.join(outputDir, 'customers-advanced-search.json'),
    `${JSON.stringify(advancedSearch, null, 2)}\n`,
    'utf8',
  );
  await writeFile(path.join(outputDir, 'customers-sort-keys.json'), `${JSON.stringify(sortKeys, null, 2)}\n`, 'utf8');
  await writeFile(
    path.join(outputDir, 'customers-relevance-search.json'),
    `${JSON.stringify(relevanceSearch, null, 2)}\n`,
    'utf8',
  );
  await writeFile(path.join(outputDir, 'customers-count.json'), `${JSON.stringify(counts, null, 2)}\n`, 'utf8');
  await clearBlocker();

  console.log(
    JSON.stringify(
      {
        ok: true,
        outputDir,
        files: [
          'customers-catalog.json',
          'customer-detail.json',
          'customers-search.json',
          'customers-advanced-search.json',
          'customers-sort-keys.json',
          'customers-relevance-search.json',
          'customers-count.json',
        ],
        sampleCustomerId: firstCustomerId,
      },
      null,
      2,
    ),
  );
} catch (error) {
  throw error;
}

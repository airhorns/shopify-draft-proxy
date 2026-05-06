/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type UserError = {
  field?: string[] | null;
  message?: string;
  code?: string | null;
};

type MarketsReadData = {
  markets?: {
    nodes?: Array<{ id?: string } | null> | null;
  } | null;
};

type PriceListCreateData = {
  priceListCreate?: {
    priceList?: { id?: string } | null;
    userErrors?: UserError[];
  };
};

type PriceListDeleteData = {
  priceListDelete?: {
    deletedId?: string | null;
    userErrors?: UserError[];
  };
};

type CatalogCreateData = {
  catalogCreate?: {
    catalog?: { id?: string } | null;
    userErrors?: UserError[];
  };
};

type CatalogUpdateData = {
  catalogUpdate?: {
    catalog?: { id?: string } | null;
    userErrors?: UserError[];
  };
};

type CatalogDeleteData = {
  catalogDelete?: {
    deletedId?: string | null;
    userErrors?: UserError[];
  };
};

type CapturedCase<TData> = {
  name: string;
  query: string;
  variables: Record<string, unknown>;
  response: ConformanceGraphqlResult<TData>;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'markets');
const outputPath = path.join(outputDir, 'catalog-relation-validation.json');

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const marketsReadQuery = `#graphql
query CatalogRelationMarketsRead($first: Int!) {
  markets(first: $first) {
    nodes {
      id
      name
      handle
      status
      enabled
    }
  }
}
`;

const priceListCreateMutation = `#graphql
mutation PriceListCreateCatalogValidation($input: PriceListCreateInput!) {
  priceListCreate(input: $input) {
    priceList {
      id
      currency
      parent {
        adjustment {
          type
          value
        }
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

const priceListDeleteMutation = `#graphql
mutation CatalogRelationPriceListDelete($id: ID!) {
  priceListDelete(id: $id) {
    deletedId
    userErrors {
      field
      message
      code
    }
  }
}
`;

const catalogCreateMutation = `#graphql
mutation CatalogCreateRelationValidation($input: CatalogCreateInput!) {
  catalogCreate(input: $input) {
    catalog {
      id
    }
    userErrors {
      field
      message
      code
    }
  }
}
`;

const catalogUpdateMutation = `#graphql
mutation CatalogUpdateRelationValidation($id: ID!, $input: CatalogUpdateInput!) {
  catalogUpdate(id: $id, input: $input) {
    catalog {
      id
    }
    userErrors {
      field
      message
      code
    }
  }
}
`;

const catalogDeleteMutation = `#graphql
mutation CatalogRelationCatalogDelete($id: ID!) {
  catalogDelete(id: $id) {
    deletedId
    userErrors {
      field
      message
      code
    }
  }
}
`;

function userErrors<TData>(result: ConformanceGraphqlResult<TData>, root: keyof TData): UserError[] {
  const data = result.payload.data;
  if (typeof data !== 'object' || data === null) return [];
  const payload = data[root];
  if (typeof payload !== 'object' || payload === null) return [];
  const maybeErrors = (payload as { userErrors?: UserError[] }).userErrors;
  return Array.isArray(maybeErrors) ? maybeErrors : [];
}

function assertNoUserErrors<TData>(result: ConformanceGraphqlResult<TData>, root: keyof TData, label: string): void {
  const errors = userErrors(result, root);
  if (result.status !== 200 || result.payload.errors || errors.length > 0) {
    throw new Error(
      `${label} failed: status=${result.status} userErrors=${JSON.stringify(errors)} errors=${JSON.stringify(
        result.payload.errors ?? null,
      )}`,
    );
  }
}

function assertExpectedUserError<TData>(
  result: ConformanceGraphqlResult<TData>,
  root: keyof TData,
  expectedCode: string,
  expectedField: string[],
  label: string,
): void {
  const errors = userErrors(result, root);
  const hasExpectedError = errors.some(
    (error) => error.code === expectedCode && JSON.stringify(error.field ?? null) === JSON.stringify(expectedField),
  );
  if (result.status !== 200 || result.payload.errors || !hasExpectedError) {
    throw new Error(
      `${label} did not return expected userError ${expectedCode}: status=${result.status} userErrors=${JSON.stringify(
        errors,
      )} errors=${JSON.stringify(result.payload.errors ?? null)}`,
    );
  }
}

function firstMarketId(result: ConformanceGraphqlResult<MarketsReadData>): string {
  const id = result.payload.data?.markets?.nodes?.[0]?.id;
  if (typeof id !== 'string') {
    throw new Error(`markets(first: 1) did not return a market id: ${JSON.stringify(result.payload)}`);
  }
  return id;
}

function priceListId(result: ConformanceGraphqlResult<PriceListCreateData>): string {
  const id = result.payload.data?.priceListCreate?.priceList?.id;
  if (typeof id !== 'string') {
    throw new Error(`priceListCreate did not return an id: ${JSON.stringify(result.payload)}`);
  }
  return id;
}

function catalogId(result: ConformanceGraphqlResult<CatalogCreateData>): string {
  const id = result.payload.data?.catalogCreate?.catalog?.id;
  if (typeof id !== 'string') {
    throw new Error(`catalogCreate did not return an id: ${JSON.stringify(result.payload)}`);
  }
  return id;
}

async function captureCase<TData>(
  name: string,
  query: string,
  variables: Record<string, unknown>,
): Promise<CapturedCase<TData>> {
  return {
    name,
    query,
    variables,
    response: await runGraphqlRequest<TData>(query, variables),
  };
}

function priceListCreateVariables(label: string): Record<string, unknown> {
  return {
    input: {
      name: label,
      currency: 'DKK',
      parent: {
        adjustment: {
          type: 'PERCENTAGE_DECREASE',
          value: 10,
        },
      },
    },
  };
}

function catalogCreateVariables(
  title: string,
  marketIdValue: string,
  relation: Record<string, unknown> = {},
): Record<string, unknown> {
  return {
    input: {
      title,
      status: 'ACTIVE',
      context: {
        marketIds: [marketIdValue],
      },
      ...relation,
    },
  };
}

const unique = Date.now().toString(36);
const unknownPriceListId = 'gid://shopify/PriceList/9999999999';
const unknownPublicationId = 'gid://shopify/Publication/9999999999';
const createdPriceListIds: string[] = [];
const createdCatalogIds: string[] = [];
const cases: Array<CapturedCase<unknown>> = [];
const cleanup: Array<{
  type: 'catalog' | 'priceList';
  id: string;
  response: ConformanceGraphqlResult<unknown>;
}> = [];

const marketsReadVariables = { first: 1 };
const marketsRead = await captureCase<MarketsReadData>(
  'markets read for catalog relation validation',
  marketsReadQuery,
  marketsReadVariables,
);
if (marketsRead.response.status !== 200 || marketsRead.response.payload.errors) {
  throw new Error(`markets read failed: ${JSON.stringify(marketsRead.response.payload)}`);
}
const marketId = firstMarketId(marketsRead.response);
cases.push(marketsRead);

try {
  const catalogCreatePriceListNotFound = await captureCase<CatalogCreateData>(
    'catalogCreate priceListId not found',
    catalogCreateMutation,
    catalogCreateVariables('Catalog price-list missing', marketId, {
      priceListId: unknownPriceListId,
    }),
  );
  assertExpectedUserError(
    catalogCreatePriceListNotFound.response,
    'catalogCreate',
    'PRICE_LIST_NOT_FOUND',
    ['input', 'priceListId'],
    'catalogCreate priceListId not found',
  );
  cases.push(catalogCreatePriceListNotFound);

  const priceListForTaken = await captureCase<PriceListCreateData>(
    'priceListCreate for catalogCreate priceListId taken',
    priceListCreateMutation,
    priceListCreateVariables(`Catalog relation DKK ${unique}`),
  );
  assertNoUserErrors(priceListForTaken.response, 'priceListCreate', 'priceListCreate for priceListId taken');
  const takenPriceListId = priceListId(priceListForTaken.response);
  createdPriceListIds.push(takenPriceListId);
  cases.push(priceListForTaken);

  const firstCatalogForTaken = await captureCase<CatalogCreateData>(
    'catalogCreate first priceListId attachment',
    catalogCreateMutation,
    catalogCreateVariables('Catalog price-list first attachment', marketId, {
      priceListId: takenPriceListId,
    }),
  );
  assertNoUserErrors(firstCatalogForTaken.response, 'catalogCreate', 'catalogCreate first priceListId attachment');
  const takenCatalogId = catalogId(firstCatalogForTaken.response);
  createdCatalogIds.push(takenCatalogId);
  cases.push(firstCatalogForTaken);

  const secondCatalogForTaken = await captureCase<CatalogCreateData>(
    'catalogCreate priceListId taken',
    catalogCreateMutation,
    catalogCreateVariables('Catalog price-list second attachment', marketId, {
      priceListId: takenPriceListId,
    }),
  );
  assertExpectedUserError(
    secondCatalogForTaken.response,
    'catalogCreate',
    'TAKEN',
    ['input', 'priceListId'],
    'catalogCreate priceListId taken',
  );
  cases.push(secondCatalogForTaken);

  const catalogForPublicationNotFound = await captureCase<CatalogCreateData>(
    'catalogCreate for catalogUpdate publicationId not found',
    catalogCreateMutation,
    catalogCreateVariables('Catalog publication update target', marketId),
  );
  assertNoUserErrors(
    catalogForPublicationNotFound.response,
    'catalogCreate',
    'catalogCreate for publicationId not found',
  );
  const publicationUpdateCatalogId = catalogId(catalogForPublicationNotFound.response);
  createdCatalogIds.push(publicationUpdateCatalogId);
  cases.push(catalogForPublicationNotFound);

  const catalogUpdatePublicationNotFound = await captureCase<CatalogUpdateData>(
    'catalogUpdate publicationId not found',
    catalogUpdateMutation,
    {
      id: publicationUpdateCatalogId,
      input: {
        publicationId: unknownPublicationId,
      },
    },
  );
  assertExpectedUserError(
    catalogUpdatePublicationNotFound.response,
    'catalogUpdate',
    'PUBLICATION_NOT_FOUND',
    ['input', 'publicationId'],
    'catalogUpdate publicationId not found',
  );
  cases.push(catalogUpdatePublicationNotFound);
} finally {
  for (const id of createdCatalogIds.toReversed()) {
    cleanup.push({
      type: 'catalog',
      id,
      response: await runGraphqlRequest<CatalogDeleteData>(catalogDeleteMutation, { id }),
    });
  }
  for (const id of createdPriceListIds.toReversed()) {
    cleanup.push({
      type: 'priceList',
      id,
      response: await runGraphqlRequest<PriceListDeleteData>(priceListDeleteMutation, { id }),
    });
  }
}

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      scope: 'catalog relation validation',
      cases,
      cleanup,
      upstreamCalls: [
        {
          operationName: 'CatalogRelationMarketsRead',
          variables: marketsReadVariables,
          query: marketsReadQuery,
          response: {
            status: marketsRead.response.status,
            body: marketsRead.response.payload,
          },
        },
      ],
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(
  JSON.stringify(
    {
      ok: true,
      outputPath,
      storeDomain,
      apiVersion,
      marketId,
      cases: cases.map((capture) => ({
        name: capture.name,
        status: capture.response.status,
      })),
      cleanup: cleanup.map((entry) => ({ type: entry.type, id: entry.id, status: entry.response.status })),
    },
    null,
    2,
  ),
);

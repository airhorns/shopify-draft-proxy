/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';

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
    nodes?: Array<{ id?: string; name?: string } | null> | null;
  } | null;
};

type CatalogCreateData = {
  catalogCreate?: {
    catalog?: { id?: string } | null;
    userErrors?: UserError[];
  } | null;
};

type CatalogUpdateData = {
  catalogUpdate?: {
    catalog?: { id?: string } | null;
    userErrors?: UserError[];
  } | null;
};

type CatalogDeleteData = {
  catalogDelete?: {
    deletedId?: string | null;
    userErrors?: UserError[];
  } | null;
};

type PriceListCreateData = {
  priceListCreate?: {
    priceList?: { id?: string } | null;
    userErrors?: UserError[];
  } | null;
};

type PriceListUpdateData = {
  priceListUpdate?: {
    priceList?: { id?: string } | null;
    userErrors?: UserError[];
  } | null;
};

type PriceListDeleteData = {
  priceListDelete?: {
    deletedId?: string | null;
    userErrors?: UserError[];
  } | null;
};

type PublicationCreateData = {
  publicationCreate?: {
    publication?: { id?: string } | null;
    userErrors?: UserError[];
  } | null;
};

type PublicationDeleteData = {
  publicationDelete?: {
    deletedId?: string | null;
    userErrors?: UserError[];
  } | null;
};

type CapturedCase<TData = unknown> = {
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
const outputPath = path.join(outputDir, 'catalog-update-relation-read.json');

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const marketsReadDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'catalog-relation-markets-read.graphql'),
  'utf8',
);
const catalogCreateDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'catalog-update-relation-read-catalog-create.graphql'),
  'utf8',
);
const publicationCreateDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'catalog-update-relation-read-publication-create.graphql'),
  'utf8',
);
const priceListCreateDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'catalog-update-relation-read-price-list-create.graphql'),
  'utf8',
);
const priceListUpdateDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'catalog-update-relation-read-price-list-update.graphql'),
  'utf8',
);
const catalogUpdateDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'catalog-update-scalar-context-relations.graphql'),
  'utf8',
);
const readbackDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'catalog-update-scalar-context-relation-read.graphql'),
  'utf8',
);

const catalogDeleteDocument = `#graphql
mutation CatalogUpdateRelationReadCatalogCleanup($id: ID!) {
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

const priceListDeleteDocument = `#graphql
mutation CatalogUpdateRelationReadPriceListCleanup($id: ID!) {
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

const publicationDeleteDocument = `#graphql
mutation CatalogUpdateRelationReadPublicationCleanup($id: ID!) {
  publicationDelete(id: $id) {
    deletedId
    userErrors {
      field
      message
      code
    }
  }
}
`;

function userErrors<TData>(result: ConformanceGraphqlResult<TData>, root: string): UserError[] {
  const data = result.payload.data;
  if (typeof data !== 'object' || data === null) return [];
  const payload = (data as Record<string, unknown>)[root];
  if (typeof payload !== 'object' || payload === null) return [];
  const maybeErrors = (payload as { userErrors?: UserError[] }).userErrors;
  return Array.isArray(maybeErrors) ? maybeErrors : [];
}

function assertNoGraphqlErrors<TData>(result: ConformanceGraphqlResult<TData>, label: string): void {
  if (result.status !== 200 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertNoUserErrors<TData>(result: ConformanceGraphqlResult<TData>, root: string, label: string): void {
  assertNoGraphqlErrors(result, label);
  const errors = userErrors(result, root);
  if (errors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(errors)}`);
  }
}

function firstTwoMarketIds(result: ConformanceGraphqlResult<MarketsReadData>): [string, string] {
  const nodes = result.payload.data?.markets?.nodes ?? [];
  const ids = nodes.flatMap((node) => (typeof node?.id === 'string' ? [node.id] : []));
  if (ids.length < 2) {
    throw new Error(`markets(first: 2) must return at least two markets: ${JSON.stringify(result.payload)}`);
  }
  return [ids[0], ids[1]];
}

function catalogId(result: ConformanceGraphqlResult<CatalogCreateData>): string {
  const id = result.payload.data?.catalogCreate?.catalog?.id;
  if (typeof id !== 'string') {
    throw new Error(`catalogCreate did not return an id: ${JSON.stringify(result.payload)}`);
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

function publicationId(result: ConformanceGraphqlResult<PublicationCreateData>): string {
  const id = result.payload.data?.publicationCreate?.publication?.id;
  if (typeof id !== 'string') {
    throw new Error(`publicationCreate did not return an id: ${JSON.stringify(result.payload)}`);
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

async function captureReadbackWhenCatalogSearchIndexes(
  variables: Record<string, unknown>,
): Promise<CapturedCase<unknown>> {
  let latest: CapturedCase<unknown> | undefined;
  for (let attempt = 0; attempt < 10; attempt += 1) {
    latest = await captureCase(
      'catalog and price list read after catalogUpdate relation changes',
      readbackDocument,
      variables,
    );
    assertNoGraphqlErrors(latest.response, 'catalogUpdate readback');
    const data = latest.response.payload.data;
    const catalogs =
      typeof data === 'object' && data !== null
        ? ((data as { catalogs?: { nodes?: unknown[] | null } }).catalogs?.nodes ?? [])
        : [];
    if (catalogs.length > 0) {
      return latest;
    }
    await sleep(1500);
  }
  return latest as CapturedCase<unknown>;
}

const unique = `catalog-update-relation-${Date.now()}`;
const cases: Array<CapturedCase<unknown>> = [];
const cleanup: Array<{ type: string; id: string; response: ConformanceGraphqlResult<unknown> }> = [];
const createdCatalogIds: string[] = [];
const createdPriceListIds: string[] = [];
const createdPublicationIds: string[] = [];

const marketsReadVariables = { first: 2 };
const marketsRead = await captureCase<MarketsReadData>(
  'markets read for catalogUpdate relation read setup',
  marketsReadDocument,
  marketsReadVariables,
);
assertNoGraphqlErrors(marketsRead.response, 'markets read');
const [initialMarketId, updatedMarketId] = firstTwoMarketIds(marketsRead.response);
cases.push(marketsRead);

try {
  const initialTitle = `Catalog relation read initial ${unique}`;
  const updatedTitle = `Catalog relation read updated ${unique}`;
  const priceListName = `Catalog relation read prices ${unique}`;
  const catalogQuery = `title:${unique}`;

  const catalogSetup = await captureCase<CatalogCreateData>(
    'catalogCreate for catalogUpdate relation read',
    catalogCreateDocument,
    {
      input: {
        title: initialTitle,
        status: 'DRAFT',
        context: { marketIds: [initialMarketId] },
      },
    },
  );
  assertNoUserErrors(catalogSetup.response, 'catalogCreate', 'catalogCreate setup');
  const createdCatalogId = catalogId(catalogSetup.response);
  createdCatalogIds.push(createdCatalogId);
  cases.push(catalogSetup);

  const publicationSetup = await captureCase<PublicationCreateData>(
    'publicationCreate for catalogUpdate relation read',
    publicationCreateDocument,
    { input: { autoPublish: true } },
  );
  assertNoUserErrors(publicationSetup.response, 'publicationCreate', 'publicationCreate setup');
  const createdPublicationId = publicationId(publicationSetup.response);
  createdPublicationIds.push(createdPublicationId);
  cases.push(publicationSetup);

  const priceListSetup = await captureCase<PriceListCreateData>(
    'priceListCreate with catalog relation and compare-at settings',
    priceListCreateDocument,
    {
      input: {
        name: priceListName,
        currency: 'USD',
        catalogId: createdCatalogId,
        parent: {
          adjustment: { type: 'PERCENTAGE_DECREASE', value: 10 },
          settings: { compareAtMode: 'ADJUSTED' },
        },
      },
    },
  );
  assertNoUserErrors(priceListSetup.response, 'priceListCreate', 'priceListCreate setup');
  const createdPriceListId = priceListId(priceListSetup.response);
  createdPriceListIds.push(createdPriceListId);
  cases.push(priceListSetup);

  const priceListSettingsUpdate = await captureCase<PriceListUpdateData>(
    'priceListUpdate changes compare-at settings while catalog remains expanded',
    priceListUpdateDocument,
    {
      id: createdPriceListId,
      input: {
        parent: {
          adjustment: { type: 'PERCENTAGE_INCREASE', value: 15 },
          settings: { compareAtMode: 'NULLIFY' },
        },
      },
    },
  );
  assertNoUserErrors(priceListSettingsUpdate.response, 'priceListUpdate', 'priceListUpdate settings');
  cases.push(priceListSettingsUpdate);

  const catalogUpdate = await captureCase<CatalogUpdateData>(
    'catalogUpdate scalar context and relation readback',
    catalogUpdateDocument,
    {
      id: createdCatalogId,
      input: {
        title: updatedTitle,
        status: 'ACTIVE',
        context: { marketIds: [updatedMarketId] },
        publicationId: createdPublicationId,
      },
    },
  );
  assertNoUserErrors(catalogUpdate.response, 'catalogUpdate', 'catalogUpdate scalar/context/relation');
  cases.push(catalogUpdate);

  const readback = await captureReadbackWhenCatalogSearchIndexes({
    catalogId: createdCatalogId,
    priceListId: createdPriceListId,
    catalogQuery,
  });
  cases.push(readback);
} finally {
  for (const id of createdCatalogIds.slice().reverse()) {
    cleanup.push({
      type: 'catalog',
      id,
      response: await runGraphqlRequest<CatalogDeleteData>(catalogDeleteDocument, { id }),
    });
  }
  for (const id of createdPriceListIds.slice().reverse()) {
    cleanup.push({
      type: 'priceList',
      id,
      response: await runGraphqlRequest<PriceListDeleteData>(priceListDeleteDocument, { id }),
    });
  }
  for (const id of createdPublicationIds.slice().reverse()) {
    cleanup.push({
      type: 'publication',
      id,
      response: await runGraphqlRequest<PublicationDeleteData>(publicationDeleteDocument, { id }),
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
      scope: 'catalogUpdate scalar/context relation readback and price-list parent settings',
      cases,
      cleanup,
      upstreamCalls: [
        {
          operationName: 'CatalogRelationMarketsRead',
          variables: marketsReadVariables,
          query: marketsReadDocument,
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
      initialMarketId,
      updatedMarketId,
      cases: cases.map((entry) => ({
        name: entry.name,
        status: entry.response.status,
      })),
      cleanup: cleanup.map((entry) => ({
        type: entry.type,
        id: entry.id,
        status: entry.response.status,
      })),
    },
    null,
    2,
  ),
);

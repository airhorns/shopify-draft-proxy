/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type CapturedCase<TData = unknown> = {
  name: string;
  query: string;
  request: { variables: Record<string, unknown> };
  response: ConformanceGraphqlResult<TData>;
};

type RecordedUpstreamCall = {
  operationName: string;
  query: string;
  variables: Record<string, unknown>;
  response: {
    status: number;
    body: unknown;
  };
};

type JsonRecord = Record<string, unknown>;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'markets');
const outputPath = path.join(outputDir, 'market-mutation-first-hydration.json');

const preflightQuery = await readFile(
  path.join('config', 'parity-requests', 'markets', 'market-mutation-targets-hydrate.graphql'),
  'utf8',
);
const updateMutation = await readFile(
  path.join('config', 'parity-requests', 'markets', 'market-mutation-first-update.graphql'),
  'utf8',
);
const updateReadQuery = await readFile(
  path.join('config', 'parity-requests', 'markets', 'market-mutation-first-update-read.graphql'),
  'utf8',
);
const deleteMutation = await readFile(
  path.join('config', 'parity-requests', 'markets', 'market-delete-cascade-delete.graphql'),
  'utf8',
);
const deleteReadQuery = await readFile(
  path.join('config', 'parity-requests', 'markets', 'market-delete-cascade-read.graphql'),
  'utf8',
);
const updateUnknownMutation = await readFile(
  path.join('config', 'parity-requests', 'markets', 'market-update-unknown-id-validation.graphql'),
  'utf8',
);
const deleteUnknownMutation = await readFile(
  path.join('config', 'parity-requests', 'markets', 'market-delete-unknown-id-validation.graphql'),
  'utf8',
);

const marketCreateMutation = `#graphql
mutation MutationFirstMarketCreate($input: MarketCreateInput!) {
  marketCreate(input: $input) {
    market {
      id
      name
      handle
      status
      enabled
    }
    userErrors {
      field
      message
      code
    }
  }
}
`;

const catalogCreateMutation = `#graphql
mutation MutationFirstCatalogCreate($input: CatalogCreateInput!) {
  catalogCreate(input: $input) {
    catalog {
      id
      title
      status
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
mutation MutationFirstCatalogCleanup($id: ID!) {
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

const webPresenceCreateMutation = `#graphql
mutation MutationFirstWebPresenceCreate($input: WebPresenceCreateInput!) {
  webPresenceCreate(input: $input) {
    webPresence {
      id
      subfolderSuffix
    }
    userErrors {
      field
      message
      code
    }
  }
}
`;

const webPresenceDeleteMutation = `#graphql
mutation MutationFirstWebPresenceCleanup($id: ID!) {
  webPresenceDelete(id: $id) {
    deletedId
    userErrors {
      field
      message
      code
    }
  }
}
`;

const marketLinkWebPresenceMutation = `#graphql
mutation MutationFirstMarketLinkWebPresence($id: ID!, $input: MarketUpdateInput!) {
  marketUpdate(id: $id, input: $input) {
    market {
      id
      webPresences(first: 10) {
        nodes {
          id
          subfolderSuffix
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

const wrongResourceProductId = process.env['SHOPIFY_CONFORMANCE_PRODUCT_ID'] ?? 'gid://shopify/Product/9801098789170';
const unknownMarketId = 'gid://shopify/Market/999999999999';
const unique = Date.now().toString(36);
const letterUnique = unique.replace(/[0-9]/gu, (digit) => 'abcdefghij'[Number(digit)] ?? 'a');

async function captureCase<TData = unknown>(
  name: string,
  query: string,
  variables: Record<string, unknown>,
): Promise<CapturedCase<TData>> {
  const response = await runGraphqlRequest<TData>(query, variables);
  return {
    name,
    query,
    request: { variables },
    response,
  };
}

function rootPayload(response: ConformanceGraphqlResult<unknown>, root: string): JsonRecord | null {
  const data = response.payload.data;
  if (typeof data !== 'object' || data === null || Array.isArray(data)) return null;
  const payload = (data as JsonRecord)[root];
  if (typeof payload !== 'object' || payload === null || Array.isArray(payload)) return null;
  return payload as JsonRecord;
}

function userErrors(response: ConformanceGraphqlResult<unknown>, root: string): unknown[] {
  const payload = rootPayload(response, root);
  const errors = payload?.['userErrors'];
  return Array.isArray(errors) ? errors : [];
}

function assertNoUserErrors(response: ConformanceGraphqlResult<unknown>, root: string, label: string): void {
  const errors = userErrors(response, root);
  if (errors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(errors)}`);
  }
}

function readPayloadId(
  response: ConformanceGraphqlResult<unknown>,
  root: string,
  objectKey: string,
  label: string,
): string {
  const payload = rootPayload(response, root);
  const object = payload?.[objectKey];
  const id =
    typeof object === 'object' && object !== null && !Array.isArray(object) ? (object as JsonRecord)['id'] : null;
  if (typeof id !== 'string' || id.length === 0) {
    throw new Error(`${label} did not return ${objectKey}.id: ${JSON.stringify(response.payload)}`);
  }
  return id;
}

async function capturePreflight(name: string, ids: string[]): Promise<RecordedUpstreamCall> {
  const variables = { ids };
  const response = await runGraphqlRequest(preflightQuery, variables);
  if (response.status < 200 || response.status >= 300 || response.payload.errors) {
    throw new Error(`${name} preflight failed: ${JSON.stringify(response.payload)}`);
  }
  return {
    operationName: 'MarketsMutationPreflightHydrate',
    query: preflightQuery,
    variables,
    response: {
      status: response.status,
      body: response.payload,
    },
  };
}

async function createRelatedMarket(kind: string): Promise<{
  marketId: string;
  catalogId: string;
  webPresenceId: string;
  cases: Array<CapturedCase<unknown>>;
}> {
  const cases: Array<CapturedCase<unknown>> = [];
  let marketId: string | null = null;
  let catalogId: string | null = null;
  let webPresenceId: string | null = null;
  try {
    const market = await captureCase('setup market create', marketCreateMutation, {
      input: {
        name: `Mutation First ${kind} ${unique}`,
      },
    });
    assertNoUserErrors(market.response, 'marketCreate', `${kind} marketCreate`);
    marketId = readPayloadId(market.response, 'marketCreate', 'market', `${kind} marketCreate`);
    cases.push(market);

    const catalog = await captureCase('setup catalog create', catalogCreateMutation, {
      input: {
        title: `Mutation First ${kind} Catalog ${unique}`,
        status: 'ACTIVE',
        context: {
          marketIds: [marketId],
        },
      },
    });
    assertNoUserErrors(catalog.response, 'catalogCreate', `${kind} catalogCreate`);
    catalogId = readPayloadId(catalog.response, 'catalogCreate', 'catalog', `${kind} catalogCreate`);
    cases.push(catalog);

    const webPresence = await captureCase('setup web presence create', webPresenceCreateMutation, {
      input: {
        defaultLocale: 'en',
        alternateLocales: [],
        subfolderSuffix: `${kind.toLowerCase()}${letterUnique}`,
      },
    });
    assertNoUserErrors(webPresence.response, 'webPresenceCreate', `${kind} webPresenceCreate`);
    webPresenceId = readPayloadId(
      webPresence.response,
      'webPresenceCreate',
      'webPresence',
      `${kind} webPresenceCreate`,
    );
    cases.push(webPresence);

    const link = await captureCase('setup market web presence link', marketLinkWebPresenceMutation, {
      id: marketId,
      input: {
        webPresencesToAdd: [webPresenceId],
      },
    });
    assertNoUserErrors(link.response, 'marketUpdate', `${kind} webPresence link`);
    cases.push(link);

    return {
      marketId,
      catalogId,
      webPresenceId,
      cases,
    };
  } catch (error) {
    await Promise.allSettled([cleanupMarket(marketId), cleanupCatalog(catalogId), cleanupWebPresence(webPresenceId)]);
    throw error;
  }
}

async function cleanupMarket(id: string | null): Promise<ConformanceGraphqlResult<unknown> | null> {
  if (!id) return null;
  return runGraphqlRequest(deleteMutation, { id });
}

async function cleanupCatalog(id: string | null): Promise<ConformanceGraphqlResult<unknown> | null> {
  if (!id) return null;
  return runGraphqlRequest(catalogDeleteMutation, { id });
}

async function cleanupWebPresence(id: string | null): Promise<ConformanceGraphqlResult<unknown> | null> {
  if (!id) return null;
  return runGraphqlRequest(webPresenceDeleteMutation, { id });
}

let updateMarketId: string | null = null;
let updateCatalogId: string | null = null;
let updateWebPresenceId: string | null = null;
let deleteMarketId: string | null = null;
let deleteCatalogId: string | null = null;
let deleteWebPresenceId: string | null = null;
const upstreamCalls: RecordedUpstreamCall[] = [];
const cleanup: JsonRecord = {};

try {
  const updateSetup = await createRelatedMarket('Update');
  updateMarketId = updateSetup.marketId;
  updateCatalogId = updateSetup.catalogId;
  updateWebPresenceId = updateSetup.webPresenceId;

  const updatePreflight = await capturePreflight('mutation-first update', [updateMarketId]);
  upstreamCalls.push(updatePreflight);
  const update = await captureCase('mutation-first marketUpdate', updateMutation, {
    id: updateMarketId,
    input: {
      name: `Mutation First Update Renamed ${unique}`,
      handle: `mutation-first-update-renamed-${unique}`,
    },
  });
  assertNoUserErrors(update.response, 'marketUpdate', 'mutation-first marketUpdate');
  const updateRead = await captureCase('mutation-first update downstream read', updateReadQuery, {
    marketId: updateMarketId,
    catalogId: updateCatalogId,
  });

  const deleteSetup = await createRelatedMarket('Delete');
  deleteMarketId = deleteSetup.marketId;
  deleteCatalogId = deleteSetup.catalogId;
  deleteWebPresenceId = deleteSetup.webPresenceId;

  const deletePreflight = await capturePreflight('mutation-first delete', [deleteMarketId]);
  upstreamCalls.push(deletePreflight);
  const marketDelete = await captureCase('mutation-first marketDelete', deleteMutation, {
    id: deleteMarketId,
  });
  assertNoUserErrors(marketDelete.response, 'marketDelete', 'mutation-first marketDelete');
  const deleteRead = await captureCase('mutation-first delete downstream read', deleteReadQuery, {
    marketId: deleteMarketId,
    catalogId: deleteCatalogId,
    webPresencesFirst: 100,
  });

  upstreamCalls.push(await capturePreflight('unknown update validation', [unknownMarketId]));
  const updateUnknown = await captureCase('marketUpdate unknown id validation', updateUnknownMutation, {
    id: unknownMarketId,
    input: {
      name: 'Nope',
    },
  });
  upstreamCalls.push(await capturePreflight('unknown delete validation', [unknownMarketId]));
  const deleteUnknown = await captureCase('marketDelete unknown id validation', deleteUnknownMutation, {
    id: unknownMarketId,
  });
  const updateWrongResource = await captureCase('marketUpdate wrong resource id validation', updateMutation, {
    id: wrongResourceProductId,
    input: {
      name: 'Nope',
    },
  });
  const deleteWrongResource = await captureCase('marketDelete wrong resource id validation', deleteMutation, {
    id: wrongResourceProductId,
  });

  cleanup['updateMarketDelete'] = await cleanupMarket(updateMarketId);
  updateMarketId = null;
  cleanup['updateCatalogDelete'] = await cleanupCatalog(updateCatalogId);
  updateCatalogId = null;
  cleanup['updateWebPresenceDelete'] = await cleanupWebPresence(updateWebPresenceId);
  updateWebPresenceId = null;
  cleanup['deleteCatalogDelete'] = await cleanupCatalog(deleteCatalogId);
  deleteCatalogId = null;
  cleanup['deleteWebPresenceDelete'] = await cleanupWebPresence(deleteWebPresenceId);
  deleteWebPresenceId = null;

  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        scope:
          'Markets mutation-first hydration for existing marketUpdate and marketDelete targets, with catalog/web-presence relations and unknown-id validation.',
        liveSetup: {
          setupCreatesDisposableNonPrimaryMarkets: true,
          updateMarketId: updateSetup.marketId,
          updateCatalogId: updateSetup.catalogId,
          updateWebPresenceId: updateSetup.webPresenceId,
          deleteMarketId: deleteSetup.marketId,
          deleteCatalogId: deleteSetup.catalogId,
          deleteWebPresenceId: deleteSetup.webPresenceId,
          unknownMarketId,
          wrongResourceProductId,
        },
        mutationFirstUpdate: {
          setup: updateSetup.cases,
          preflight: updatePreflight,
          update,
          downstreamRead: updateRead,
        },
        mutationFirstDelete: {
          setup: deleteSetup.cases,
          preflight: deletePreflight,
          delete: marketDelete,
          downstreamRead: deleteRead,
        },
        validation: {
          updateUnknown,
          deleteUnknown,
          updateWrongResource,
          deleteWrongResource,
        },
        cleanup,
        upstreamCalls,
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
        updateMarketId: updateSetup.marketId,
        deleteMarketId: deleteSetup.marketId,
        upstreamCalls: upstreamCalls.length,
      },
      null,
      2,
    ),
  );
} finally {
  const finalCleanup = await Promise.allSettled([
    cleanupMarket(updateMarketId),
    cleanupCatalog(updateCatalogId),
    cleanupWebPresence(updateWebPresenceId),
    cleanupMarket(deleteMarketId),
    cleanupCatalog(deleteCatalogId),
    cleanupWebPresence(deleteWebPresenceId),
  ]);
  const rejected = finalCleanup.filter((result) => result.status === 'rejected');
  if (rejected.length > 0) {
    console.error(JSON.stringify({ cleanupWarnings: rejected.map((result) => String(result.reason)) }, null, 2));
  }
}

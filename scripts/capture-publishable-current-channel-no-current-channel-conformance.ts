/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import {
  createAdminGraphqlClient,
  type ConformanceGraphqlPayload,
  type ConformanceGraphqlResult,
} from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'store-properties');
const fixturePath = path.join(fixtureDir, 'publishable-current-channel-no-current-channel.json');

const publishablePublishCurrentDocument = await readFile(
  path.join(
    'config',
    'parity-requests',
    'store-properties',
    'publishable-current-channel-no-current-channel-publish.graphql',
  ),
  'utf8',
);
const publishableUnpublishCurrentDocument = await readFile(
  path.join(
    'config',
    'parity-requests',
    'store-properties',
    'publishable-current-channel-no-current-channel-unpublish.graphql',
  ),
  'utf8',
);
const publicationResourceHydrateQuery = await readFile(
  path.join('config', 'parity-requests', 'products', 'publication-resource-hydrate-nodes.graphql'),
  'utf8',
);
const currentAppPublicationHydrateQuery = await readFile(
  path.join('config', 'parity-requests', 'store-properties', 'current-app-publication-hydrate.graphql'),
  'utf8',
);

const { runGraphql, runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

type ProductCreateData = {
  productCreate: {
    product: { id: string; title: string; status: string } | null;
    userErrors: Array<{ field: string[] | null; message: string }>;
  };
};

type ProductDeleteData = {
  productDelete: {
    deletedProductId: string | null;
    userErrors: Array<{ field: string[] | null; message: string }>;
  };
};

type ChannelDetails = {
  id: string;
  handle: string | null;
  specificationHandle: string;
  accountId: string;
  accountName: string;
};

type CurrentAppChannelData = {
  currentAppInstallation: {
    channel: ChannelDetails | null;
    publication: { id: string } | null;
  } | null;
};

type ChannelDeleteData = {
  channelDelete: {
    userErrors: Array<{ field: string[] | null; message: string }>;
  } | null;
};

type ChannelCreateData = {
  channelCreate: {
    channel: ChannelDetails | null;
    userErrors: Array<{ field: string[] | null; message: string }>;
  } | null;
};

type MarketsData = {
  markets: {
    nodes: Array<{
      id: string;
      handle: string;
      name: string;
      regions: { nodes: Array<{ code?: string | null }> };
    }>;
  };
};

type MarketCreateData = {
  marketCreate: {
    market: { id: string; handle: string; name: string } | null;
    userErrors: Array<{ field: string[] | null; message: string; code?: string | null }>;
  } | null;
};

type PublishableIdVariables = {
  id: string;
};

type CapturedCase<TVariables extends Record<string, unknown>> = {
  variables: TVariables;
  response: ConformanceGraphqlResult;
};

type RecordedUpstreamCall = {
  operationName: string;
  variables: Record<string, unknown>;
  query: string;
  response: { status: number; body: unknown };
};

const createProductMutation = `#graphql
  mutation PublishableNoCurrentChannelCreateProduct($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
        id
        title
        status
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const deleteProductMutation = `#graphql
  mutation PublishableNoCurrentChannelDeleteProduct($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

const currentAppChannelQuery = `#graphql
  query PublishableNoCurrentChannelCurrentAppChannel {
    currentAppInstallation {
      channel {
        id
        handle
        specificationHandle
        accountId
        accountName
      }
      publication {
        id
      }
    }
  }
`;

const channelDeleteMutation = `#graphql
  mutation PublishableNoCurrentChannelDeleteChannel($id: ID!) {
    channelDelete(id: $id) {
      userErrors {
        field
        message
      }
    }
  }
`;

const channelCreateMutation = `#graphql
  mutation PublishableNoCurrentChannelCreateChannel($input: ChannelCreateInput!) {
    channelCreate(input: $input) {
      channel {
        id
        handle
        specificationHandle
        accountId
        accountName
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const marketsQuery = `#graphql
  query PublishableNoCurrentChannelMarkets {
    markets(first: 50) {
      nodes {
        id
        handle
        name
        regions(first: 250) {
          nodes {
            ... on MarketRegionCountry {
              code
            }
          }
        }
      }
    }
  }
`;

const marketCreateMutation = `#graphql
  mutation PublishableNoCurrentChannelCreateMarket($input: MarketCreateInput!) {
    marketCreate(input: $input) {
      market {
        id
        handle
        name
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

async function captureCase<TVariables extends Record<string, unknown>>(
  query: string,
  variables: TVariables,
): Promise<CapturedCase<TVariables>> {
  const response = await runGraphqlRaw(query, variables);
  assertNoGraphqlErrors('publishable current-channel case capture', response);
  return {
    variables,
    response,
  };
}

async function captureHydrate(resourceId: string): Promise<RecordedUpstreamCall> {
  const variables = { ids: [resourceId] };
  const response = await runGraphqlRaw(publicationResourceHydrateQuery, variables);
  return {
    operationName: 'PublicationResourceHydrate',
    variables,
    query: publicationResourceHydrateQuery,
    response: {
      status: response.status,
      body: response.payload,
    },
  };
}

async function captureCurrentAppPublicationHydrate(): Promise<RecordedUpstreamCall> {
  const variables = {};
  const response = await runGraphqlRaw(currentAppPublicationHydrateQuery, variables);
  return {
    operationName: 'StorePropertiesCurrentAppPublicationHydrate',
    variables,
    query: currentAppPublicationHydrateQuery,
    response: {
      status: response.status,
      body: response.payload,
    },
  };
}

function assertNoUserErrors(label: string, errors: Array<{ message: string }> | undefined): void {
  if (errors !== undefined && errors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(errors)}`);
  }
}

function assertNoGraphqlErrors(label: string, response: ConformanceGraphqlResult): void {
  if (Array.isArray(response.payload.errors) && response.payload.errors.length > 0) {
    throw new Error(`${label} returned GraphQL errors: ${JSON.stringify(response.payload.errors)}`);
  }
}

function channelCreateInput(channel: ChannelDetails): Record<string, string> {
  const input: Record<string, string> = {
    specificationHandle: channel.specificationHandle,
    accountId: channel.accountId,
    accountName: channel.accountName,
  };
  if (channel.handle) {
    input['handle'] = channel.handle;
  }
  return input;
}

async function ensureRestorableChannelPrerequisites(
  channel: ChannelDetails,
  suffix: string,
): Promise<ConformanceGraphqlResult<MarketCreateData> | null> {
  if (channel.specificationHandle !== 'example-us') {
    return null;
  }

  const markets = await runGraphqlRaw<MarketsData>(marketsQuery, {});
  assertNoGraphqlErrors('markets prerequisite query', markets);
  const hasUsMarket = markets.payload.data?.markets.nodes.some((market) =>
    market.regions.nodes.some((region) => region.code === 'US'),
  );
  if (hasUsMarket) {
    return null;
  }

  const repair = await runGraphqlRaw<MarketCreateData>(marketCreateMutation, {
    input: {
      name: `Publishable US channel restore ${suffix}`,
      handle: `publishable-us-channel-restore-${suffix}`,
      enabled: true,
      regions: [{ countryCode: 'US' }],
    },
  });
  assertNoGraphqlErrors('marketCreate channel restore prerequisite', repair);
  if (!repair.payload.data?.marketCreate) {
    throw new Error(`marketCreate channel restore prerequisite returned no payload: ${JSON.stringify(repair)}`);
  }
  assertNoUserErrors('marketCreate channel restore prerequisite', repair.payload.data.marketCreate.userErrors);
  return repair;
}

async function restoreCurrentAppChannel(channel: ChannelDetails): Promise<{
  channelRecreate: ConformanceGraphqlResult<ChannelCreateData>;
  postRecreateCurrentApp: ConformanceGraphqlResult<CurrentAppChannelData>;
}> {
  const channelRecreateResponse = await runGraphqlRaw<ChannelCreateData>(channelCreateMutation, {
    input: channelCreateInput(channel),
  });
  assertNoGraphqlErrors('channelCreate restore', channelRecreateResponse);
  if (!channelRecreateResponse.payload.data?.channelCreate) {
    throw new Error(`channelCreate restore returned no payload: ${JSON.stringify(channelRecreateResponse)}`);
  }
  assertNoUserErrors('channelCreate restore', channelRecreateResponse.payload.data.channelCreate.userErrors);

  const postRecreateCurrentAppResponse = await runGraphqlRaw<CurrentAppChannelData>(currentAppChannelQuery, {});
  const restoredChannel = postRecreateCurrentAppResponse.payload.data?.currentAppInstallation?.channel ?? null;
  if (!restoredChannel) {
    throw new Error(
      `channelCreate restore did not restore current app channel: ${JSON.stringify(postRecreateCurrentAppResponse)}`,
    );
  }

  return {
    channelRecreate: channelRecreateResponse,
    postRecreateCurrentApp: postRecreateCurrentAppResponse,
  };
}

await mkdir(fixtureDir, { recursive: true });

const runId = Date.now().toString(36);
let productId: string | null = null;
let setup: ConformanceGraphqlPayload<ProductCreateData> | null = null;
let cleanup: ConformanceGraphqlResult<ProductDeleteData> | null = null;
let channelBeforeDelete: ChannelDetails | null = null;
let channelDelete: ConformanceGraphqlResult<ChannelDeleteData> | null = null;
let channelRecreate: ConformanceGraphqlResult<ChannelCreateData> | null = null;
let marketRepair: ConformanceGraphqlResult<MarketCreateData> | null = null;
let channelDeleted = false;
let postDeleteCurrentApp: ConformanceGraphqlResult<CurrentAppChannelData> | null = null;
let postRecreateCurrentApp: ConformanceGraphqlResult<CurrentAppChannelData> | null = null;
let captureError: unknown = null;
let restoreError: unknown = null;
const cases: Record<string, CapturedCase<Record<string, unknown>>> = {};
const upstreamCalls: RecordedUpstreamCall[] = [];

try {
  setup = await runGraphql<ProductCreateData>(createProductMutation, {
    product: {
      title: `Publishable no current channel ${runId}`,
      status: 'ACTIVE',
    },
  });
  productId = setup.data?.productCreate.product?.id ?? null;
  if (!productId) {
    throw new Error(`Product setup failed: ${JSON.stringify(setup)}`);
  }

  const currentAppBeforeDelete = await runGraphql<CurrentAppChannelData>(currentAppChannelQuery);
  channelBeforeDelete = currentAppBeforeDelete.data?.currentAppInstallation?.channel ?? null;
  if (!channelBeforeDelete) {
    throw new Error(
      `No current app channel exists to delete/recreate for no-current-channel capture: ${JSON.stringify(currentAppBeforeDelete)}`,
    );
  }
  marketRepair = await ensureRestorableChannelPrerequisites(channelBeforeDelete, runId);

  channelDelete = await runGraphqlRaw<ChannelDeleteData>(channelDeleteMutation, {
    id: channelBeforeDelete.id,
  });
  assertNoGraphqlErrors('channelDelete', channelDelete);
  if (!channelDelete.payload.data?.channelDelete) {
    throw new Error(`channelDelete returned no payload: ${JSON.stringify(channelDelete)}`);
  }
  assertNoUserErrors('channelDelete', channelDelete.payload.data.channelDelete.userErrors);
  channelDeleted = true;

  postDeleteCurrentApp = await runGraphqlRaw<CurrentAppChannelData>(currentAppChannelQuery, {});
  const postDeletePublication = postDeleteCurrentApp.payload.data?.currentAppInstallation?.publication ?? null;
  if (postDeletePublication !== null) {
    throw new Error(`Current app still has a publication after channelDelete: ${JSON.stringify(postDeleteCurrentApp)}`);
  }

  const productVariables: PublishableIdVariables = { id: productId };
  upstreamCalls.push(await captureHydrate(productId));
  upstreamCalls.push(await captureCurrentAppPublicationHydrate());
  cases['publishCurrentNoChannel'] = await captureCase(publishablePublishCurrentDocument, productVariables);
  cases['unpublishCurrentNoChannel'] = await captureCase(publishableUnpublishCurrentDocument, productVariables);
} catch (error) {
  captureError = error;
} finally {
  if (channelDeleted && channelBeforeDelete) {
    try {
      const restored = await restoreCurrentAppChannel(channelBeforeDelete);
      channelRecreate = restored.channelRecreate;
      postRecreateCurrentApp = restored.postRecreateCurrentApp;
    } catch (error) {
      console.warn(
        JSON.stringify(
          {
            ok: false,
            cleanup: 'channelCreate',
            channelBeforeDelete,
            error: error instanceof Error ? error.message : String(error),
          },
          null,
          2,
        ),
      );
      restoreError = error;
    }
  }

  if (productId) {
    try {
      cleanup = await runGraphqlRaw<ProductDeleteData>(deleteProductMutation, {
        input: { id: productId },
      });
    } catch (error) {
      console.warn(
        JSON.stringify(
          {
            ok: false,
            cleanup: 'productDelete',
            productId,
            error: error instanceof Error ? error.message : String(error),
          },
          null,
          2,
        ),
      );
    }
  }
}

if (restoreError) {
  throw restoreError;
}

if (captureError) {
  throw captureError;
}

if (!productId || !setup || !channelBeforeDelete || Object.keys(cases).length === 0) {
  throw new Error('Publishable no-current-channel capture did not produce required setup/cases.');
}

await writeFile(
  fixturePath,
  `${JSON.stringify(
    {
      scenarioId: 'publishable-current-channel-no-current-channel',
      storeDomain,
      apiVersion,
      capturedAt: new Date().toISOString(),
      setup: {
        product: setup.data?.productCreate.product ?? null,
        currentChannelBeforeDelete: channelBeforeDelete,
        marketRepair,
      },
      channelDelete,
      postDeleteCurrentApp,
      cases,
      cleanup,
      channelRecreate,
      postRecreateCurrentApp,
      notes: [
        'Live Admin API capture for publishable current-channel mutations when the calling app has no current publication/channel.',
        'The recorder temporarily deletes the current app channel, records CHANNEL_DOES_NOT_EXIST on an ACTIVE product, and recreates the same channel in finally.',
        'When restoring an example-us channel would otherwise fail because the store lacks a US market, the recorder creates that prerequisite through marketCreate before deleting the channel.',
      ],
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
      fixturePath,
      productId,
      caseCount: Object.keys(cases).length,
      cleanupDeletedProductId: cleanup?.payload.data?.productDelete.deletedProductId ?? null,
      channelRestored: postRecreateCurrentApp?.payload.data?.currentAppInstallation?.channel?.id ?? null,
    },
    null,
    2,
  ),
);

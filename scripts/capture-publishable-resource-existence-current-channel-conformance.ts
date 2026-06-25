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
const fixturePath = path.join(fixtureDir, 'publishable-resource-existence-current-channel.json');

const publishablePublishDocument = await readFile(
  path.join('config', 'parity-requests', 'store-properties', 'publishable-input-validation.graphql'),
  'utf8',
);
const publishableUnpublishDocument = await readFile(
  path.join('config', 'parity-requests', 'store-properties', 'publishable-input-validation-unpublish.graphql'),
  'utf8',
);
const publishablePublishCurrentDocument = await readFile(
  path.join('config', 'parity-requests', 'store-properties', 'publishable-input-validation-publish-current.graphql'),
  'utf8',
);
const publishableUnpublishCurrentDocument = await readFile(
  path.join('config', 'parity-requests', 'store-properties', 'publishable-input-validation-unpublish-current.graphql'),
  'utf8',
);
const publishableCurrentMembershipDocument = await readFile(
  path.join('config', 'parity-requests', 'store-properties', 'publishable-current-channel-membership.graphql'),
  'utf8',
);
const publishableCurrentUnpublishMembershipDocument = await readFile(
  path.join(
    'config',
    'parity-requests',
    'store-properties',
    'publishable-current-channel-unpublish-membership.graphql',
  ),
  'utf8',
);
const publicationResourceHydrateQuery = await readFile(
  path.join('config', 'parity-requests', 'products', 'publication-resource-hydrate-nodes.graphql'),
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

type PublicationsData = {
  publications: {
    nodes: Array<{ id: string; name: string | null }>;
  };
};

type PublicationInput = {
  publicationId: string;
};

type PublishableInputVariables = {
  id: string;
  input: PublicationInput[];
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
  mutation PublishableResourceExistenceCreateProduct($product: ProductCreateInput!) {
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
  mutation PublishableResourceExistenceDeleteProduct($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

const publicationsQuery = `#graphql
  query PublishableResourceExistencePublications {
    publications(first: 20) {
      nodes {
        id
        name
      }
    }
  }
`;

async function captureCase<TVariables extends Record<string, unknown>>(
  query: string,
  variables: TVariables,
): Promise<CapturedCase<TVariables>> {
  return {
    variables,
    response: await runGraphqlRaw(query, variables),
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

await mkdir(fixtureDir, { recursive: true });

const runId = Date.now().toString(36);
const missingProductId = 'gid://shopify/Product/999999999999';
let productId: string | null = null;
let setup: ConformanceGraphqlPayload<ProductCreateData> | null = null;
let cleanup: ConformanceGraphqlResult<ProductDeleteData> | null = null;
let publicationsResponse: ConformanceGraphqlPayload<PublicationsData> | null = null;
const cases: Record<string, CapturedCase<Record<string, unknown>>> = {};
const upstreamCalls: RecordedUpstreamCall[] = [];

try {
  setup = await runGraphql<ProductCreateData>(createProductMutation, {
    product: {
      title: `Publishable resource existence ${runId}`,
      status: 'ACTIVE',
    },
  });
  productId = setup.data?.productCreate.product?.id ?? null;
  if (!productId) {
    throw new Error(`Product setup failed: ${JSON.stringify(setup)}`);
  }

  publicationsResponse = await runGraphql<PublicationsData>(publicationsQuery);
  const publicationId = publicationsResponse.data?.publications.nodes[0]?.id ?? null;
  if (!publicationId) {
    throw new Error('Publishable resource existence capture needs at least one shop publication.');
  }

  const missingInputVariables: PublishableInputVariables = {
    id: missingProductId,
    input: [{ publicationId }],
  };
  const missingCurrentVariables: PublishableIdVariables = { id: missingProductId };
  const productVariables: PublishableIdVariables = { id: productId };

  upstreamCalls.push(await captureHydrate(missingProductId));
  cases['publishUnknownId'] = await captureCase(publishablePublishDocument, missingInputVariables);

  upstreamCalls.push(await captureHydrate(missingProductId));
  cases['unpublishUnknownId'] = await captureCase(publishableUnpublishDocument, missingInputVariables);

  upstreamCalls.push(await captureHydrate(missingProductId));
  cases['publishCurrentUnknownId'] = await captureCase(publishablePublishCurrentDocument, missingCurrentVariables);

  upstreamCalls.push(await captureHydrate(missingProductId));
  cases['unpublishCurrentUnknownId'] = await captureCase(publishableUnpublishCurrentDocument, missingCurrentVariables);

  upstreamCalls.push(await captureHydrate(productId));
  cases['publishCurrentMembership'] = await captureCase(publishableCurrentMembershipDocument, productVariables);
  cases['unpublishCurrentMembership'] = await captureCase(
    publishableCurrentUnpublishMembershipDocument,
    productVariables,
  );
} finally {
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

if (!productId || !setup || !publicationsResponse || Object.keys(cases).length === 0) {
  throw new Error('Publishable resource existence capture did not produce required setup/cases.');
}

await writeFile(
  fixturePath,
  `${JSON.stringify(
    {
      scenarioId: 'publishable-resource-existence-current-channel',
      storeDomain,
      apiVersion,
      capturedAt: new Date().toISOString(),
      setup: {
        product: setup.data?.productCreate.product ?? null,
        publications: publicationsResponse.data?.publications.nodes ?? [],
      },
      cases,
      cleanup,
      notes: [
        'Live Admin API capture for generic publishable top-level id resource-existence validation.',
        'Live Admin API capture for current-channel publish/unpublish payload membership projection on an ACTIVE product.',
        'The available conformance credential resolves a current channel; no-current-channel live evidence requires a separate app/client with no publishable channel binding.',
      ],
      blockers: {
        noCurrentChannel:
          'No current-channel-negative app credential is available in this workspace; local runtime tests cover the no-current-channel branch until that store/app setup exists.',
      },
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
      noCurrentChannelCaptured: false,
    },
    null,
    2,
  ),
);

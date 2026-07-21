import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

import { type ConformanceGraphqlPayload, createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type UserError = { field?: string[] | null; message?: string };
type ProductNode = { id?: string; title?: string; handle?: string };
type ProductCreateData = {
  productCreate?: { product?: ProductNode | null; userErrors?: UserError[] };
};
type CollectionCreateData = {
  collectionCreate?: { collection?: { id?: string } | null; userErrors?: UserError[] };
};
type CollectionAddData = {
  collectionAddProducts?: { userErrors?: UserError[] };
};
type CollectionRemoveData = {
  collectionRemoveProducts?: { job?: { id?: string; done?: boolean } | null; userErrors?: UserError[] };
};
type CollectionReorderData = {
  collectionReorderProducts?: { job?: { id?: string; done?: boolean } | null; userErrors?: UserError[] };
};
type CollectionReadData = {
  collection?: {
    products?: {
      pageInfo?: { endCursor?: string | null };
    };
    productsCount?: { count?: number };
  } | null;
};
type JobReadData = { job?: { id?: string; done?: boolean } | null };

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const requestDir = path.join('config', 'parity-requests', 'products');
const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const fixturePath = path.join(fixtureDir, 'collection-membership-cross-page.json');
const addDocument = await readFile(path.join(requestDir, 'collection-membership-cross-page-add.graphql'), 'utf8');
const removeDocument = await readFile(path.join(requestDir, 'collection-membership-cross-page-remove.graphql'), 'utf8');
const reorderDocument = await readFile(
  path.join(requestDir, 'collection-membership-cross-page-reorder.graphql'),
  'utf8',
);
const afterReadDocument = await readFile(
  path.join(requestDir, 'collection-membership-cross-page-after-read.graphql'),
  'utf8',
);
const firstReadDocument = await readFile(
  path.join(requestDir, 'collection-membership-cross-page-first-read.graphql'),
  'utf8',
);
const targetsHydrateDocument = await readFile(
  path.join('src', 'proxy', 'product_helpers', 'collection_membership_targets_hydrate.graphql'),
  'utf8',
);
const windowHydrateDocument = await readFile(
  path.join('src', 'proxy', 'product_helpers', 'collection_membership_window_hydrate.graphql'),
  'utf8',
);

const productCreateDocument = `mutation CollectionMembershipCaptureProductCreate($product: ProductCreateInput!) {
  productCreate(product: $product) {
    product { id title handle }
    userErrors { field message }
  }
}`;
const productDeleteDocument = `mutation CollectionMembershipCaptureProductDelete($input: ProductDeleteInput!) {
  productDelete(input: $input) { deletedProductId userErrors { field message } }
}`;
const collectionCreateDocument = `mutation CollectionMembershipCaptureCollectionCreate($input: CollectionInput!) {
  collectionCreate(input: $input) {
    collection { id title handle sortOrder }
    userErrors { field message }
  }
}`;
const collectionDeleteDocument = `mutation CollectionMembershipCaptureCollectionDelete($input: CollectionDeleteInput!) {
  collectionDelete(input: $input) { deletedCollectionId userErrors { field message } }
}`;
const baselineReadDocument = `query CollectionMembershipCaptureBaseline($id: ID!) {
  collection(id: $id) {
    id
    products(first: 10, sortKey: MANUAL) {
      edges { cursor node { id title handle } }
      pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
    }
    productsCount { count precision }
  }
}`;
const jobReadDocument = `query CollectionMembershipCaptureJob($id: ID!) {
  job(id: $id) { id done }
}`;

function assertNoUserErrors(label: string, errors: UserError[] | undefined): void {
  if (Array.isArray(errors) && errors.length > 0) {
    throw new Error(`${label} returned user errors: ${JSON.stringify(errors)}`);
  }
}

function requireId(label: string, value: string | undefined): string {
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`${label} did not return an id.`);
  }
  return value;
}

function requireCursor(label: string, value: string | null | undefined): string {
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`${label} did not return a cursor.`);
  }
  return value;
}

function collectionQuery(collectionId: string): string {
  return `id:${collectionId.slice(collectionId.lastIndexOf('/') + 1)}`;
}

async function captureUpstreamCall(
  operationName: string,
  query: string,
  variables: Record<string, unknown>,
): Promise<{
  operationName: string;
  query: string;
  variables: Record<string, unknown>;
  response: { status: number; body: ConformanceGraphqlPayload };
}> {
  const response = await runGraphql(query, variables);
  return {
    operationName,
    query,
    variables,
    response: { status: 200, body: response },
  };
}

async function waitForJob(jobId: string | undefined): Promise<ConformanceGraphqlPayload<JobReadData> | null> {
  if (typeof jobId !== 'string' || jobId.length === 0) return null;
  let latest: ConformanceGraphqlPayload<JobReadData> | null = null;
  for (let attempt = 0; attempt < 30; attempt += 1) {
    latest = await runGraphql<JobReadData>(jobReadDocument, { id: jobId });
    if (latest.data?.job?.done === true) return latest;
    await delay(500);
  }
  throw new Error(`Job ${jobId} did not complete before the capture timeout.`);
}

const runId = Date.now().toString();
const productIds: string[] = [];
let collectionId: string | null = null;

try {
  const createdProducts: Array<{
    variables: Record<string, unknown>;
    response: ConformanceGraphqlPayload<ProductCreateData>;
  }> = [];
  for (let index = 1; index <= 14; index += 1) {
    const variables = {
      product: {
        title: `Membership ${runId} ${index.toString().padStart(2, '0')}`,
        status: 'ACTIVE',
      },
    };
    const response = await runGraphql<ProductCreateData>(productCreateDocument, variables);
    assertNoUserErrors(`productCreate ${index}`, response.data?.productCreate?.userErrors);
    const id = requireId(`productCreate ${index}`, response.data?.productCreate?.product?.id);
    productIds.push(id);
    createdProducts.push({ variables, response });
  }

  const collectionCreateVariables = {
    input: { title: `Membership cross page ${runId}`, sortOrder: 'MANUAL' },
  };
  const collectionCreateResponse = await runGraphql<CollectionCreateData>(
    collectionCreateDocument,
    collectionCreateVariables,
  );
  assertNoUserErrors('collectionCreate', collectionCreateResponse.data?.collectionCreate?.userErrors);
  collectionId = requireId('collectionCreate', collectionCreateResponse.data?.collectionCreate?.collection?.id);

  const setupAddVariables = {
    id: collectionId,
    productIds: productIds.slice(0, 13),
    targetId: productIds[12],
  };
  const setupAddResponse = await runGraphql<CollectionAddData>(addDocument, setupAddVariables);
  assertNoUserErrors('setup collectionAddProducts', setupAddResponse.data?.collectionAddProducts?.userErrors);

  const baselineVariables = { id: collectionId };
  const baselineResponse = await runGraphql<CollectionReadData>(baselineReadDocument, baselineVariables);
  if (baselineResponse.data?.collection?.productsCount?.count !== 13) {
    throw new Error(`Expected 13 baseline members: ${JSON.stringify(baselineResponse)}`);
  }
  const boundaryCursor = requireCursor(
    'baseline first page',
    baselineResponse.data.collection.products?.pageInfo?.endCursor,
  );

  const addedId = requireId('added product', productIds[13]);
  const removedId = requireId('removed product', productIds[12]);
  const movedId = requireId('moved product', productIds[11]);
  const untouchedId = requireId('untouched product', productIds[10]);
  const upstreamCalls = [];

  const addHydrateVariables = {
    collectionId,
    productIds: [addedId],
    collectionQuery: collectionQuery(collectionId),
    first: 12,
  };
  upstreamCalls.push(
    await captureUpstreamCall('CollectionMembershipTargetsHydrate', targetsHydrateDocument, addHydrateVariables),
  );
  const addDownstreamVariables = {
    collectionId,
    after: boundaryCursor,
    targetId: addedId,
    untouchedId: removedId,
  };
  upstreamCalls.push(
    await captureUpstreamCall('CollectionMembershipCrossPageAfterRead', afterReadDocument, addDownstreamVariables),
  );
  const addVariables = { id: collectionId, productIds: [addedId], targetId: addedId };
  const addResponse = await runGraphql<CollectionAddData>(addDocument, addVariables);
  assertNoUserErrors('collectionAddProducts', addResponse.data?.collectionAddProducts?.userErrors);
  const addDownstreamResponse = await runGraphql<CollectionReadData>(afterReadDocument, addDownstreamVariables);

  const removeHydrateVariables = {
    collectionId,
    productIds: [removedId],
    collectionQuery: collectionQuery(collectionId),
    first: 12,
  };
  upstreamCalls.push(
    await captureUpstreamCall('CollectionMembershipTargetsHydrate', targetsHydrateDocument, removeHydrateVariables),
  );
  const removeDownstreamVariables = {
    collectionId,
    after: boundaryCursor,
    targetId: removedId,
    untouchedId,
  };
  const removeBaselineCall = await captureUpstreamCall(
    'CollectionMembershipCrossPageAfterRead',
    afterReadDocument,
    removeDownstreamVariables,
  );
  upstreamCalls.push(removeBaselineCall);
  const removeBaselineBody = removeBaselineCall.response.body as ConformanceGraphqlPayload<CollectionReadData>;
  const removeBaselineEndCursor = requireCursor(
    'pre-remove connection window',
    removeBaselineBody.data?.collection?.products?.pageInfo?.endCursor,
  );
  const refillVariables = {
    id: collectionId,
    first: 4,
    after: removeBaselineEndCursor,
    last: null,
    before: null,
    reverse: false,
    sortKey: 'MANUAL',
  };
  upstreamCalls.push(
    await captureUpstreamCall('CollectionMembershipWindowHydrate', windowHydrateDocument, refillVariables),
  );

  const removeVariables = { id: collectionId, productIds: [removedId] };
  const removeResponse = await runGraphql<CollectionRemoveData>(removeDocument, removeVariables);
  assertNoUserErrors('collectionRemoveProducts', removeResponse.data?.collectionRemoveProducts?.userErrors);
  const removeJobRead = await waitForJob(removeResponse.data?.collectionRemoveProducts?.job?.id);
  const removeDownstreamResponse = await runGraphql<CollectionReadData>(afterReadDocument, removeDownstreamVariables);

  const reorderHydrateVariables = {
    collectionId,
    productIds: [],
    collectionQuery: collectionQuery(collectionId),
    first: 250,
  };
  upstreamCalls.push(
    await captureUpstreamCall('CollectionMembershipTargetsHydrate', targetsHydrateDocument, reorderHydrateVariables),
  );
  const reorderVariables = {
    id: collectionId,
    moves: [{ id: movedId, newPosition: '1' }],
  };
  const reorderResponse = await runGraphql<CollectionReorderData>(reorderDocument, reorderVariables);
  assertNoUserErrors('collectionReorderProducts', reorderResponse.data?.collectionReorderProducts?.userErrors);
  const reorderJobRead = await waitForJob(reorderResponse.data?.collectionReorderProducts?.job?.id);
  const reorderFirstVariables = { collectionId, targetId: movedId, untouchedId };
  const reorderFirstResponse = await runGraphql<CollectionReadData>(firstReadDocument, reorderFirstVariables);
  const reorderAfterVariables = {
    collectionId,
    after: boundaryCursor,
    targetId: movedId,
    untouchedId,
  };
  const reorderAfterResponse = await runGraphql<CollectionReadData>(afterReadDocument, reorderAfterVariables);

  const fixture = {
    fixtureKind: 'shopify-admin-graphql-live-capture',
    storeDomain,
    apiVersion,
    runId,
    setup: {
      createdProducts,
      collectionCreate: { variables: collectionCreateVariables, response: collectionCreateResponse },
      collectionAddProducts: { variables: setupAddVariables, response: setupAddResponse },
      baselineRead: { variables: baselineVariables, response: baselineResponse },
    },
    add: {
      mutation: { variables: addVariables, response: addResponse },
      downstreamRead: { variables: addDownstreamVariables, response: addDownstreamResponse },
    },
    remove: {
      mutation: { variables: removeVariables, response: removeResponse },
      jobRead: removeJobRead,
      downstreamRead: { variables: removeDownstreamVariables, response: removeDownstreamResponse },
    },
    reorder: {
      mutation: { variables: reorderVariables, response: reorderResponse },
      jobRead: reorderJobRead,
      firstRead: { variables: reorderFirstVariables, response: reorderFirstResponse },
      afterRead: { variables: reorderAfterVariables, response: reorderAfterResponse },
    },
    upstreamCalls,
  };

  await mkdir(fixtureDir, { recursive: true });
  await writeFile(fixturePath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  process.stdout.write(
    `${JSON.stringify(
      {
        ok: true,
        fixturePath,
        productsCreated: productIds.length,
        upstreamCalls: upstreamCalls.length,
      },
      null,
      2,
    )}\n`,
  );
} finally {
  if (collectionId !== null) {
    await runGraphql(collectionDeleteDocument, { input: { id: collectionId } }).catch(() => null);
  }
  for (const productId of [...productIds].reverse()) {
    await runGraphql(productDeleteDocument, { input: { id: productId } }).catch(() => null);
  }
}

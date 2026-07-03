/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { readFileSync } from 'node:fs';
import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlPayload } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type ProductCreateData = {
  productCreate?: {
    product?: {
      id?: string;
      title?: string;
      status?: string;
    } | null;
    userErrors?: Array<Record<string, unknown>>;
  } | null;
};

type ProductDuplicateData = {
  productDuplicate?: {
    newProduct?: {
      id?: string;
      title?: string;
      status?: string;
    } | null;
    userErrors?: Array<Record<string, unknown>>;
  } | null;
};

type ProductReadData = {
  product?: {
    id?: string;
    title?: string;
    status?: string;
  } | null;
};

type ProductDeleteData = {
  productDelete?: {
    deletedProductId?: string | null;
    userErrors?: Array<Record<string, unknown>>;
  } | null;
};

type CapturedOperation<TData> = {
  variables: Record<string, unknown>;
  response: ConformanceGraphqlPayload<TData>;
};

const repoRoot = path.resolve(path.dirname(new URL(import.meta.url).pathname), '..');
const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const outputPath = path.join(outputDir, 'product-duplicate-status-parity.json');

const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const productCreateMutation = readFileSync(
  path.join(repoRoot, 'config', 'parity-requests', 'products', 'productDuplicate-status-source-create.graphql'),
  'utf8',
);
const duplicateNoStatusMutation = readFileSync(
  path.join(repoRoot, 'config', 'parity-requests', 'products', 'productDuplicate-status-no-newStatus.graphql'),
  'utf8',
);
const duplicateNewStatusMutation = readFileSync(
  path.join(repoRoot, 'config', 'parity-requests', 'products', 'productDuplicate-status-newStatus.graphql'),
  'utf8',
);
const productStatusReadQuery = readFileSync(
  path.join(repoRoot, 'config', 'parity-requests', 'products', 'productDuplicate-status-read.graphql'),
  'utf8',
);

const productDeleteMutation = `#graphql
  mutation ProductDuplicateStatusDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

function expectNoUserErrors(label: string, userErrors: Array<Record<string, unknown>> | undefined): void {
  if (Array.isArray(userErrors) && userErrors.length === 0) {
    return;
  }

  throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors ?? null, null, 2)}`);
}

function productCreateId(label: string, response: ConformanceGraphqlPayload<ProductCreateData>): string {
  const id = response.data?.productCreate?.product?.id;
  if (!id) {
    throw new Error(`${label} did not return a product id.`);
  }
  return id;
}

function duplicateProductId(label: string, response: ConformanceGraphqlPayload<ProductDuplicateData>): string {
  const id = response.data?.productDuplicate?.newProduct?.id;
  if (!id) {
    throw new Error(`${label} did not return a duplicated product id.`);
  }
  return id;
}

async function deleteProduct(id: string): Promise<ConformanceGraphqlPayload<ProductDeleteData>> {
  return runGraphql<ProductDeleteData>(productDeleteMutation, { input: { id } });
}

async function createProduct(
  variables: Record<string, unknown>,
): Promise<ConformanceGraphqlPayload<ProductCreateData>> {
  const response = await runGraphql<ProductCreateData>(productCreateMutation, variables);
  expectNoUserErrors('productCreate', response.data?.productCreate?.userErrors);
  return response;
}

async function duplicateProduct(
  label: string,
  query: string,
  variables: Record<string, unknown>,
): Promise<ConformanceGraphqlPayload<ProductDuplicateData>> {
  const response = await runGraphql<ProductDuplicateData>(query, variables);
  expectNoUserErrors(label, response.data?.productDuplicate?.userErrors);
  return response;
}

const runId = `${Date.now()}`;
let activeSourceProductId: string | null = null;
let activeDuplicateProductId: string | null = null;
let draftSourceProductId: string | null = null;
let overrideDuplicateProductId: string | null = null;
const cleanup: Record<string, ConformanceGraphqlPayload<ProductDeleteData> | { error: string }> = {};

try {
  const activeSourceCreate: CapturedOperation<ProductCreateData> = {
    variables: {
      product: {
        title: `Hermes Duplicate Active Source ${runId}`,
        status: 'ACTIVE',
      },
    },
    response: await createProduct({
      product: {
        title: `Hermes Duplicate Active Source ${runId}`,
        status: 'ACTIVE',
      },
    }),
  };
  activeSourceProductId = productCreateId('active source productCreate', activeSourceCreate.response);

  const activeNoStatusDuplicate: CapturedOperation<ProductDuplicateData> = {
    variables: {
      productId: activeSourceProductId,
      newTitle: `Hermes Duplicate Inherited Active ${runId}`,
    },
    response: await duplicateProduct('active productDuplicate without newStatus', duplicateNoStatusMutation, {
      productId: activeSourceProductId,
      newTitle: `Hermes Duplicate Inherited Active ${runId}`,
    }),
  };
  activeDuplicateProductId = duplicateProductId(
    'active productDuplicate without newStatus',
    activeNoStatusDuplicate.response,
  );

  const activeNoStatusRead: CapturedOperation<ProductReadData> = {
    variables: { id: activeDuplicateProductId },
    response: await runGraphql<ProductReadData>(productStatusReadQuery, { id: activeDuplicateProductId }),
  };

  const draftSourceCreate: CapturedOperation<ProductCreateData> = {
    variables: {
      product: {
        title: `Hermes Duplicate Draft Source ${runId}`,
        status: 'DRAFT',
      },
    },
    response: await createProduct({
      product: {
        title: `Hermes Duplicate Draft Source ${runId}`,
        status: 'DRAFT',
      },
    }),
  };
  draftSourceProductId = productCreateId('draft source productCreate', draftSourceCreate.response);

  const draftOverrideDuplicate: CapturedOperation<ProductDuplicateData> = {
    variables: {
      productId: draftSourceProductId,
      newTitle: `Hermes Duplicate Override Active ${runId}`,
      newStatus: 'ACTIVE',
    },
    response: await duplicateProduct('draft productDuplicate with newStatus ACTIVE', duplicateNewStatusMutation, {
      productId: draftSourceProductId,
      newTitle: `Hermes Duplicate Override Active ${runId}`,
      newStatus: 'ACTIVE',
    }),
  };
  overrideDuplicateProductId = duplicateProductId(
    'draft productDuplicate with newStatus ACTIVE',
    draftOverrideDuplicate.response,
  );

  const draftOverrideRead: CapturedOperation<ProductReadData> = {
    variables: { id: overrideDuplicateProductId },
    response: await runGraphql<ProductReadData>(productStatusReadQuery, { id: overrideDuplicateProductId }),
  };

  await mkdir(outputDir, { recursive: true });
  const payload = {
    scenarioId: 'product-duplicate-status-inheritance-and-new-status',
    storeDomain,
    apiVersion,
    activeSourceCreate,
    activeNoStatusDuplicate,
    activeNoStatusRead,
    draftSourceCreate,
    draftOverrideDuplicate,
    draftOverrideRead,
    upstreamCalls: [],
  };
  await writeFile(outputPath, `${JSON.stringify(payload, null, 2)}\n`, 'utf8');

  console.log(
    JSON.stringify(
      {
        ok: true,
        outputPath,
        activeSourceProductId,
        activeDuplicateProductId,
        draftSourceProductId,
        overrideDuplicateProductId,
      },
      null,
      2,
    ),
  );
} finally {
  for (const [label, id] of [
    ['overrideDuplicateProduct', overrideDuplicateProductId],
    ['draftSourceProduct', draftSourceProductId],
    ['activeDuplicateProduct', activeDuplicateProductId],
    ['activeSourceProduct', activeSourceProductId],
  ] as const) {
    if (!id) {
      continue;
    }
    try {
      cleanup[label] = await deleteProduct(id);
    } catch (error) {
      cleanup[label] = { error: error instanceof Error ? error.message : String(error) };
    }
  }

  if (Object.keys(cleanup).length > 0) {
    console.error(JSON.stringify({ cleanup }, null, 2));
  }
}

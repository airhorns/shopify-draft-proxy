/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlPayload } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metafields');
const outputPath = path.join(outputDir, 'metafields-set-input-validation.json');
const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

type ProductCreateData = {
  productCreate: {
    product: { id: string; title: string } | null;
    userErrors: Array<{ field: string[] | null; message: string }>;
  };
};

type ProductDeleteData = {
  productDelete: {
    deletedProductId: string | null;
    userErrors: Array<{ field: string[] | null; message: string }>;
  };
};

type MetafieldsSetInput = {
  ownerId: string;
  namespace: string;
  key: string;
  type: string;
  value: string;
};

type MetafieldsSetVariables = {
  metafields: [MetafieldsSetInput];
};

type ValidationCase = {
  request: {
    variables: MetafieldsSetVariables;
  };
  response: ConformanceGraphqlPayload;
};

const createProductMutation = `#graphql
  mutation MetafieldsSetInputValidationCreateProduct($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
        id
        title
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const deleteProductMutation = `#graphql
  mutation MetafieldsSetInputValidationDeleteProduct($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

const metafieldsSetMutation = `#graphql
  mutation MetafieldsSetInputValidation($metafields: [MetafieldsSetInput!]!) {
    metafieldsSet(metafields: $metafields) {
      metafields {
        id
      }
      userErrors {
        field
        message
        code
        elementIndex
      }
    }
  }
`;

function metafield(
  ownerId: string,
  namespace: string,
  key: string,
  type: string,
  value: string,
): MetafieldsSetVariables {
  return {
    metafields: [
      {
        ownerId,
        namespace,
        key,
        type,
        value,
      },
    ],
  };
}

function buildCases(productId: string): Record<string, MetafieldsSetVariables> {
  return {
    shortNamespace: metafield(productId, 'ab', 'valid_key', 'single_line_text_field', 'v'),
    shortKey: metafield(productId, 'valid', 'x', 'single_line_text_field', 'v'),
    longNamespace: metafield(productId, 'n'.repeat(256), 'valid_key', 'single_line_text_field', 'v'),
    longKey: metafield(productId, 'loyalty', 'k'.repeat(65), 'single_line_text_field', 'v'),
    invalidCharacterNamespace: metafield(productId, 'bad$namespace', 'valid_key', 'single_line_text_field', 'v'),
    invalidCharacterKey: metafield(productId, 'loyalty', 'bad$key', 'single_line_text_field', 'v'),
    reservedNamespaceShopifyStandard: metafield(productId, 'shopify_standard', 'title', 'single_line_text_field', 'x'),
    reservedNamespaceProtected: metafield(productId, 'protected', 'title', 'single_line_text_field', 'x'),
    reservedNamespaceShopifyL10nFields: metafield(
      productId,
      'shopify-l10n-fields',
      'title',
      'single_line_text_field',
      'x',
    ),
    invalidValueNumberInteger: metafield(productId, 'loyalty', 'tier', 'number_integer', 'not a number'),
    invalidValueBoolean: metafield(productId, 'loyalty', 'flag', 'boolean', 'yes'),
    invalidValueColor: metafield(productId, 'loyalty', 'color', 'color', 'blue'),
    invalidValueDateTime: metafield(productId, 'loyalty', 'published_at', 'date_time', 'tomorrow'),
    invalidValueJson: metafield(productId, 'loyalty', 'payload', 'json', '{nope'),
    invalidReference: metafield(
      productId,
      'loyalty',
      'related',
      'product_reference',
      'gid://shopify/Product/not-a-product',
    ),
  };
}

async function captureCase(variables: MetafieldsSetVariables): Promise<ValidationCase> {
  const response = await runGraphql(metafieldsSetMutation, variables);
  return {
    request: { variables },
    response,
  };
}

await mkdir(outputDir, { recursive: true });

const runId = Date.now().toString(36);
let productId: string | null = null;
let cleanup: ConformanceGraphqlPayload<ProductDeleteData> | null = null;
const cases: Record<string, ValidationCase> = {};

try {
  const setup = await runGraphql<ProductCreateData>(createProductMutation, {
    product: {
      title: `HAR-695 metafieldsSet validation ${runId}`,
      status: 'DRAFT',
    },
  });
  productId = setup.data?.productCreate.product?.id ?? null;
  if (!productId) {
    throw new Error(`Product setup failed: ${JSON.stringify(setup)}`);
  }

  for (const [caseName, variables] of Object.entries(buildCases(productId))) {
    cases[caseName] = await captureCase(variables);
  }
} finally {
  if (productId) {
    try {
      cleanup = await runGraphql<ProductDeleteData>(deleteProductMutation, {
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

if (!productId || Object.keys(cases).length === 0) {
  throw new Error('metafieldsSet input validation capture did not produce cases.');
}

await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      scenarioId: 'metafields-set-input-validation',
      storeDomain,
      apiVersion,
      capturedAt: new Date().toISOString(),
      setup: {
        productId,
      },
      cases,
      cleanup,
      upstreamCalls: [],
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
      productId,
      caseCount: Object.keys(cases).length,
    },
    null,
    2,
  ),
);

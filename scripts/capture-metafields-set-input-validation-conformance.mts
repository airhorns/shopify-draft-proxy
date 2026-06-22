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
  const tooManyListValues = JSON.stringify(Array.from({ length: 129 }, (_value, index) => `item-${index}`));
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
    invalidValueNumberDecimal: metafield(productId, 'loyalty', 'decimal', 'number_decimal', '10000000000000.1'),
    invalidValueMoney: metafield(productId, 'loyalty', 'money', 'money', '{"amount":"12.00"}'),
    invalidValueUrl: metafield(productId, 'loyalty', 'url', 'url', 'example.com'),
    invalidValueDimensionNegative: metafield(
      productId,
      'loyalty',
      'dimension',
      'dimension',
      '{"value":-1,"unit":"cm"}',
    ),
    invalidValueWeightUnit: metafield(productId, 'loyalty', 'weight', 'weight', '{"value":1,"unit":"bogus"}'),
    invalidValueVolumeNumeric: metafield(
      productId,
      'loyalty',
      'volume',
      'volume',
      '{"value":"not-a-number","unit":"ml"}',
    ),
    invalidValueRatingBounds: metafield(
      productId,
      'loyalty',
      'rating',
      'rating',
      '{"value":"6.0","scale_min":"1.0","scale_max":"5.0"}',
    ),
    invalidValueDate: metafield(productId, 'loyalty', 'date', 'date', '2026/06/21'),
    invalidValueLinkScheme: metafield(
      productId,
      'loyalty',
      'link',
      'link',
      '{"label":"Docs","url":"ftp://example.com"}',
    ),
    invalidValueSingleLineBlank: metafield(productId, 'loyalty', 'blank_single', 'single_line_text_field', ''),
    invalidValueSingleLineNewline: metafield(
      productId,
      'loyalty',
      'newline_single',
      'single_line_text_field',
      'Line\nBreak',
    ),
    invalidValueMultiLineBlank: metafield(productId, 'loyalty', 'blank_multi', 'multi_line_text_field', '   '),
    invalidValueListNumberIntegerElement: metafield(
      productId,
      'loyalty',
      'list_integer',
      'list.number_integer',
      '[1,"x"]',
    ),
    invalidValueListTextLength: metafield(
      productId,
      'loyalty',
      'list_text',
      'list.single_line_text_field',
      tooManyListValues,
    ),
    invalidReference: metafield(
      productId,
      'loyalty',
      'related',
      'product_reference',
      'gid://shopify/Product/not-a-product',
    ),
    invalidReferenceMissingProduct: metafield(
      productId,
      'loyalty',
      'missing_related',
      'product_reference',
      'gid://shopify/Product/999999998',
    ),
    invalidValueListProductReference: metafield(
      productId,
      'loyalty',
      'list_related',
      'list.product_reference',
      '["gid://shopify/Product/999999997"]',
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
      title: `metafieldsSet input validation ${runId}`,
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

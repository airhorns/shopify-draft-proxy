/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlPayload } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metafields');
const outputPath = path.join(outputDir, 'metafieldsSet-value-type-fidelity.json');
const setDocumentPath = path.join(
  'config',
  'parity-requests',
  'metafields',
  'metafieldsSet-value-type-fidelity-set.graphql',
);
const readDocumentPath = path.join(
  'config',
  'parity-requests',
  'metafields',
  'metafieldsSet-value-type-fidelity-read.graphql',
);
const inputValidationDocumentPath = path.join(
  'config',
  'parity-requests',
  'metafields',
  'metafields-set-input-validation.graphql',
);

const [metafieldsSetMutation, downstreamReadQuery, inputValidationMutation] = await Promise.all([
  readFile(setDocumentPath, 'utf8'),
  readFile(readDocumentPath, 'utf8'),
  readFile(inputValidationDocumentPath, 'utf8'),
]);

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
  metafields: MetafieldsSetInput[];
};

type ProductReadVariables = {
  id: string;
  namespace: string;
};

type CapturedGraphqlCall<TVariables, TData = unknown> = {
  request: {
    variables: TVariables;
  };
  response: ConformanceGraphqlPayload<TData>;
};

const createProductMutation = `#graphql
  mutation MetafieldsSetValueTypeFidelityCreateProduct($product: ProductCreateInput!) {
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
  mutation MetafieldsSetValueTypeFidelityDeleteProduct($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

function metafield(ownerId: string, namespace: string, key: string, type: string, value: string): MetafieldsSetInput {
  return {
    ownerId,
    namespace,
    key,
    type,
    value,
  };
}

function oneMetafield(
  ownerId: string,
  namespace: string,
  key: string,
  type: string,
  value: string,
): MetafieldsSetVariables {
  return {
    metafields: [metafield(ownerId, namespace, key, type, value)],
  };
}

function buildAcceptedSet(ownerId: string, namespace: string): MetafieldsSetVariables {
  return {
    metafields: [
      metafield(ownerId, namespace, 'boolean_t', 'boolean', 't'),
      metafield(ownerId, namespace, 'boolean_true', 'boolean', 'true'),
      metafield(ownerId, namespace, 'boolean_one', 'boolean', '1'),
      metafield(ownerId, namespace, 'boolean_f', 'boolean', 'f'),
      metafield(ownerId, namespace, 'boolean_false', 'boolean', 'false'),
      metafield(ownerId, namespace, 'boolean_zero', 'boolean', '0'),
      metafield(ownerId, namespace, 'boolean_trimmed_upper', 'boolean', ' TRUE '),
      metafield(ownerId, namespace, 'integer_decimal_zero', 'number_integer', '5.0'),
      metafield(ownerId, namespace, 'integer_decimal_zeroes', 'number_integer', '5.000'),
      metafield(ownerId, namespace, 'decimal_truncated', 'number_decimal', '1.1234567891'),
    ],
  };
}

function buildInvalidCases(ownerId: string, namespace: string): Record<string, MetafieldsSetVariables> {
  return {
    integerPlus: oneMetafield(ownerId, namespace, 'integer_plus', 'number_integer', '+5'),
    moneyBlankCurrency: oneMetafield(
      ownerId,
      namespace,
      'money_blank_currency',
      'money',
      '{"amount":"1.00","currency_code":""}',
    ),
    moneyAmountType: oneMetafield(
      ownerId,
      namespace,
      'money_amount_type',
      'money',
      '{"amount":"abc","currency_code":"USD"}',
    ),
    moneyOutOfRange: oneMetafield(
      ownerId,
      namespace,
      'money_out_of_range',
      'money',
      '{"amount":"1000000000000000001","currency_code":"CAD"}',
    ),
    moneyInvalidShape: oneMetafield(ownerId, namespace, 'money_invalid_shape', 'money', '[]'),
    moneyInvalidCurrency: oneMetafield(
      ownerId,
      namespace,
      'money_invalid_currency',
      'money',
      '{"amount":"1.00","currency_code":"ZZZ"}',
    ),
    urlMissingScheme: oneMetafield(ownerId, namespace, 'url_missing_scheme', 'url', 'example.com'),
    urlUnsupportedScheme: oneMetafield(ownerId, namespace, 'url_unsupported_scheme', 'url', 'ftp://x'),
    dateTime: oneMetafield(ownerId, namespace, 'date_time', 'date_time', 'nope'),
    jsonBlank: oneMetafield(ownerId, namespace, 'json_blank', 'json', ''),
  };
}

async function captureMetafieldsSet(
  document: string,
  variables: MetafieldsSetVariables,
): Promise<CapturedGraphqlCall<MetafieldsSetVariables>> {
  return {
    request: { variables },
    response: await runGraphql(document, variables),
  };
}

await mkdir(outputDir, { recursive: true });

const runId = Date.now().toString(36);
const namespace = `value_type_${runId}`;
let productId: string | null = null;
let setup: CapturedGraphqlCall<{ product: { title: string; status: 'DRAFT' } }, ProductCreateData> | null = null;
let acceptedSet: CapturedGraphqlCall<MetafieldsSetVariables> | null = null;
let downstreamRead: CapturedGraphqlCall<ProductReadVariables> | null = null;
let cleanup: ConformanceGraphqlPayload<ProductDeleteData> | null = null;
const invalidCases: Record<string, CapturedGraphqlCall<MetafieldsSetVariables>> = {};

try {
  const setupVariables = {
    product: {
      title: `metafieldsSet value type fidelity ${runId}`,
      status: 'DRAFT' as const,
    },
  };
  setup = {
    request: { variables: setupVariables },
    response: await runGraphql<ProductCreateData>(createProductMutation, setupVariables),
  };
  productId = setup.response.data?.productCreate.product?.id ?? null;
  if (!productId) {
    throw new Error(`Product setup failed: ${JSON.stringify(setup.response)}`);
  }

  acceptedSet = await captureMetafieldsSet(metafieldsSetMutation, buildAcceptedSet(productId, namespace));

  const readVariables = {
    id: productId,
    namespace,
  };
  downstreamRead = {
    request: { variables: readVariables },
    response: await runGraphql(downstreamReadQuery, readVariables),
  };

  for (const [caseName, variables] of Object.entries(buildInvalidCases(productId, namespace))) {
    invalidCases[caseName] = await captureMetafieldsSet(inputValidationMutation, variables);
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

if (!setup || !productId || !acceptedSet || !downstreamRead || Object.keys(invalidCases).length === 0) {
  throw new Error('metafieldsSet value type fidelity capture did not produce all required calls.');
}

await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      scenarioId: 'metafieldsSet-value-type-fidelity',
      storeDomain,
      apiVersion,
      capturedAt: new Date().toISOString(),
      setup: {
        createProduct: setup,
      },
      acceptedSet,
      downstreamRead,
      invalidCases,
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
      namespace,
      invalidCaseCount: Object.keys(invalidCases).length,
    },
    null,
    2,
  ),
);

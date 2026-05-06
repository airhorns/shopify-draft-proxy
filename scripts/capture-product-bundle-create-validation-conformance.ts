/* oxlint-disable no-console -- CLI capture scripts intentionally report status to stdout/stderr. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlPayload } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type CaptureEntry = {
  document: string;
  variables: Record<string, unknown>;
  result: ConformanceGraphqlPayload;
};

type JsonRecord = Record<string, unknown>;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const outputPath = path.join(outputDir, 'productBundleCreate-validation.json');

const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const productCreateDocument = `#graphql
  mutation ProductBundleCreateValidationSetupProduct($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
        id
        title
        handle
        status
        options {
          id
          name
          position
          values
          optionValues {
            id
            name
            hasVariants
          }
        }
        variants(first: 10) {
          nodes {
            id
            title
            selectedOptions {
              name
              value
            }
          }
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const productBundleCreateDocument = `#graphql
  mutation ProductBundleCreateValidation($input: ProductBundleCreateInput!) {
    productBundleCreate(input: $input) {
      productBundleOperation {
        __typename
        id
        status
        product {
          id
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const productOperationReadDocument = `#graphql
  query ProductBundleOperationRead($id: ID!) {
    productOperation(id: $id) {
      __typename
      status
      product {
        id
      }
      ... on ProductBundleOperation {
        id
      }
    }
  }
`;

const productDeleteDocument = `#graphql
  mutation ProductBundleCreateValidationCleanupProduct($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

function readRecord(value: unknown): JsonRecord | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readPath(value: unknown, pathSegments: readonly string[]): unknown {
  let current: unknown = value;
  for (const segment of pathSegments) {
    if (Array.isArray(current)) {
      current = current[Number(segment)];
      continue;
    }

    const record = readRecord(current);
    if (!record) {
      return undefined;
    }
    current = record[segment];
  }
  return current;
}

function readStringPath(value: unknown, pathSegments: readonly string[], context: string): string {
  const resolved = readPath(value, pathSegments);
  if (typeof resolved !== 'string' || resolved.length === 0) {
    throw new Error(`Could not resolve ${context}.`);
  }
  return resolved;
}

function readOptionIds(productCreate: ConformanceGraphqlPayload): string[] {
  const options = readPath(productCreate, ['data', 'productCreate', 'product', 'options']);
  if (!Array.isArray(options) || options.length < 3) {
    throw new Error('Product bundle validation setup did not create at least three product options.');
  }

  return options.slice(0, 3).map((option, index) => readStringPath(option, ['id'], `option ${index + 1} id`));
}

async function capture(document: string, variables: Record<string, unknown>): Promise<CaptureEntry> {
  return {
    document,
    variables,
    result: await runGraphql(document, variables),
  };
}

function componentOptionSelections(optionIds: readonly string[]) {
  return [
    { componentOptionId: optionIds[0], name: 'Color', values: ['Red'] },
    { componentOptionId: optionIds[1], name: 'Size', values: ['Small'] },
    { componentOptionId: optionIds[2], name: 'Material', values: ['Cotton'] },
  ];
}

const runId = `bundle-validation-${Date.now()}`;
const createdProductIds: string[] = [];

try {
  const productCreateVariables = {
    product: {
      title: `${runId} Bundle Component`,
      status: 'DRAFT',
      productOptions: [
        { name: 'Color', values: [{ name: 'Red' }] },
        { name: 'Size', values: [{ name: 'Small' }] },
        { name: 'Material', values: [{ name: 'Cotton' }] },
      ],
    },
  };
  const productCreate = await capture(productCreateDocument, productCreateVariables);
  const componentProductId = readStringPath(
    productCreate.result,
    ['data', 'productCreate', 'product', 'id'],
    'setup product id',
  );
  createdProductIds.push(componentProductId);

  const optionIds = readOptionIds(productCreate.result);
  const validSelections = componentOptionSelections(optionIds);
  const componentBase = {
    productId: componentProductId,
    optionSelections: validSelections,
  };

  const missingComponentProduct = await capture(productBundleCreateDocument, {
    input: {
      title: `${runId} Missing Component Product`,
      components: [
        {
          productId: 'gid://shopify/Product/0',
          quantity: 1,
          optionSelections: [],
        },
      ],
    },
  });

  const optionSelectionsOverLimit = await capture(productBundleCreateDocument, {
    input: {
      title: `${runId} Option Mapping Invalid`,
      components: [
        {
          ...componentBase,
          quantity: 1,
          optionSelections: [
            ...validSelections,
            {
              componentOptionId: 'gid://shopify/ProductOption/0',
              name: 'Finish',
              values: ['Matte'],
            },
          ],
        },
      ],
    },
  });

  const quantityOverMaximum = await capture(productBundleCreateDocument, {
    input: {
      title: `${runId} Quantity Over Maximum`,
      components: [
        {
          ...componentBase,
          quantity: 2001,
        },
      ],
    },
  });

  const oneValueQuantityOption = await capture(productBundleCreateDocument, {
    input: {
      title: `${runId} One Value Quantity Option`,
      components: [
        {
          ...componentBase,
          quantityOption: {
            name: 'Pack',
            values: [{ name: 'Single', quantity: 1 }],
          },
        },
      ],
    },
  });

  const quantityZeroSuccess = await capture(productBundleCreateDocument, {
    input: {
      title: `${runId} Quantity Zero Success`,
      components: [
        {
          ...componentBase,
          quantity: 0,
        },
      ],
    },
  });
  const operationId = readStringPath(
    quantityZeroSuccess.result,
    ['data', 'productBundleCreate', 'productBundleOperation', 'id'],
    'product bundle operation id',
  );
  const operationRead = await capture(productOperationReadDocument, { id: operationId });

  const blankQuantityOptionNameSuccess = await capture(productBundleCreateDocument, {
    input: {
      title: `${runId} Blank Quantity Option Name Success`,
      components: [
        {
          ...componentBase,
          quantityOption: {
            name: '',
            values: [
              { name: 'One', quantity: 0 },
              { name: 'Two', quantity: 2 },
            ],
          },
        },
      ],
    },
  });

  const capturePayload = {
    scenarioId: 'productBundleCreate-validation',
    apiVersion,
    storeDomain,
    notes: [
      'productBundleCreate validation and async operation-shape capture.',
      'The setup product is disposable and supplies real component product/option IDs. The local parity spec substitutes the proxy-created product and option IDs into the same captured validation requests.',
      'Admin GraphQL 2025-01 accepts component quantity 0 and a blank quantityOption name when the quantity option has at least two values; this fixture records that behavior instead of the stale ticket expectation.',
      'The configured Admin GraphQL 2025-01 schema rejects top-level ProductBundleCreateInput.consolidatedOptions as an undefined field before resolver validation, so consolidated option resolver guardrails are covered by local runtime tests rather than this live parity fixture.',
    ],
    run: {
      runId,
      componentProductId,
      optionIds,
    },
    captures: {
      productCreate,
      missingComponentProduct,
      optionSelectionsOverLimit,
      quantityOverMaximum,
      oneValueQuantityOption,
      quantityZeroSuccess,
      operationRead,
      blankQuantityOptionNameSuccess,
    },
    upstreamCalls: [],
  };

  await mkdir(outputDir, { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(capturePayload, null, 2)}\n`, 'utf8');
  console.log(JSON.stringify({ ok: true, outputPath, componentProductId, operationId }, null, 2));
} finally {
  for (const productId of createdProductIds.reverse()) {
    try {
      await runGraphql(productDeleteDocument, { input: { id: productId } });
    } catch (error) {
      console.error(`Best-effort cleanup failed for ${productId}:`, error);
    }
  }
}

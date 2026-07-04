/* oxlint-disable no-console -- CLI scripts intentionally write capture status to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;
type GraphqlPayload = {
  data?: JsonRecord;
  errors?: unknown;
  extensions?: unknown;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const productCreateInputValidationPath = path.join(outputDir, 'product-create-input-validation.json');
const { runGraphql, runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const productCreateMutation = `#graphql
  mutation ProductCreateInputValidation($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
        id
        title
        vendor
        productType
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const productUpdateMutation = `#graphql
  mutation ProductUpdateInputLengthValidation($product: ProductUpdateInput!) {
    productUpdate(product: $product) {
      product {
        id
        title
        vendor
        productType
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const productSetMutation = `#graphql
  mutation ProductSetInputLengthValidation($input: ProductSetInput!, $synchronous: Boolean!) {
    productSet(input: $input, synchronous: $synchronous) {
      product {
        id
        title
        handle
        vendor
        productType
      }
      productSetOperation {
        id
        status
        userErrors {
          field
          message
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const productDeleteMutation = `#graphql
  mutation ProductScalarLengthValidationCleanup($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

function repeated(char: string): string {
  return char.repeat(256);
}

function readPath(value: unknown, pathParts: string[]): unknown {
  let current = value;
  for (const part of pathParts) {
    if (Array.isArray(current)) {
      const index = Number.parseInt(part, 10);
      current = Number.isNaN(index) ? undefined : current[index];
    } else if (typeof current === 'object' && current !== null) {
      current = (current as JsonRecord)[part];
    } else {
      current = undefined;
    }
  }
  return current;
}

async function cleanupProduct(productId: string | null): Promise<unknown> {
  if (productId === null) {
    return null;
  }
  try {
    return await runGraphql(productDeleteMutation, { input: { id: productId } });
  } catch (error) {
    console.warn(`cleanup failed for ${productId}:`, error);
    return null;
  }
}

async function runRaw(query: string, variables: JsonRecord): Promise<GraphqlPayload> {
  const { status, payload } = await runGraphqlRaw(query, variables);
  if (status < 200 || status >= 300) {
    throw new Error(`GraphQL request failed with HTTP ${status}: ${JSON.stringify(payload, null, 2)}`);
  }
  return payload as GraphqlPayload;
}

await mkdir(outputDir, { recursive: true });

const runId = `${Date.now()}`;
const capturedAt = new Date().toISOString();
let setupProductId: string | null = null;

try {
  const tooLongTitleCreateVariables = {
    product: {
      title: repeated('t'),
      vendor: 'v',
    },
  };
  const tooLongTitleCreateResponse = await runRaw(productCreateMutation, tooLongTitleCreateVariables);

  const setupVariables = {
    product: {
      title: `Hermes Length Validation Seed ${runId}`,
      vendor: 'Hermes',
      productType: 'Seed Type',
    },
  };
  const setupResponse = await runGraphql(productCreateMutation, setupVariables);
  const createdId = readPath(setupResponse, ['data', 'productCreate', 'product', 'id']);
  if (typeof createdId !== 'string') {
    throw new Error(`Setup productCreate did not return a product id: ${JSON.stringify(setupResponse, null, 2)}`);
  }
  setupProductId = createdId;

  const updateScenarios = {
    tooLongTitle: {
      variables: {
        product: {
          id: setupProductId,
          title: repeated('u'),
        },
      },
      response: await runRaw(productUpdateMutation, {
        product: {
          id: setupProductId,
          title: repeated('u'),
        },
      }),
    },
    tooLongVendor: {
      variables: {
        product: {
          id: setupProductId,
          vendor: repeated('v'),
        },
      },
      response: await runRaw(productUpdateMutation, {
        product: {
          id: setupProductId,
          vendor: repeated('v'),
        },
      }),
    },
    tooLongProductType: {
      variables: {
        product: {
          id: setupProductId,
          productType: repeated('p'),
        },
      },
      response: await runRaw(productUpdateMutation, {
        product: {
          id: setupProductId,
          productType: repeated('p'),
        },
      }),
    },
  };

  const productSetScenarios = {
    tooLongTitle: {
      variables: {
        synchronous: true,
        input: {
          title: repeated('s'),
          vendor: 'Hermes',
        },
      },
      response: await runRaw(productSetMutation, {
        synchronous: true,
        input: {
          title: repeated('s'),
          vendor: 'Hermes',
        },
      }),
    },
    tooLongHandle: {
      variables: {
        synchronous: true,
        input: {
          title: `Hermes Set Handle Length ${runId}`,
          handle: repeated('h'),
          vendor: 'Hermes',
        },
      },
      response: await runRaw(productSetMutation, {
        synchronous: true,
        input: {
          title: `Hermes Set Handle Length ${runId}`,
          handle: repeated('h'),
          vendor: 'Hermes',
        },
      }),
    },
    tooLongVendor: {
      variables: {
        synchronous: true,
        input: {
          title: `Hermes Set Vendor Length ${runId}`,
          vendor: repeated('v'),
        },
      },
      response: await runRaw(productSetMutation, {
        synchronous: true,
        input: {
          title: `Hermes Set Vendor Length ${runId}`,
          vendor: repeated('v'),
        },
      }),
    },
    tooLongProductType: {
      variables: {
        synchronous: true,
        input: {
          title: `Hermes Set Product Type Length ${runId}`,
          vendor: 'Hermes',
          productType: repeated('p'),
        },
      },
      response: await runRaw(productSetMutation, {
        synchronous: true,
        input: {
          title: `Hermes Set Product Type Length ${runId}`,
          vendor: 'Hermes',
          productType: repeated('p'),
        },
      }),
    },
  };

  const cleanup = await cleanupProduct(setupProductId);
  setupProductId = null;

  const createInputValidation = JSON.parse(await readFile(productCreateInputValidationPath, 'utf8')) as JsonRecord;
  const scenarios = {
    ...(createInputValidation['scenarios'] as JsonRecord | undefined),
    tooLongTitle: {
      variables: tooLongTitleCreateVariables,
      response: tooLongTitleCreateResponse,
    },
  };
  const notes = Array.isArray(createInputValidation['notes']) ? createInputValidation['notes'] : [];
  await writeFile(
    productCreateInputValidationPath,
    `${JSON.stringify(
      {
        ...createInputValidation,
        capturedAt,
        apiVersion,
        storeDomain,
        scenarios,
        notes,
        upstreamCalls: [],
      },
      null,
      2,
    )}\n`,
    'utf8',
  );

  await writeFile(
    path.join(outputDir, 'product-update-input-length-validation.json'),
    `${JSON.stringify(
      {
        capturedAt,
        apiVersion,
        storeDomain,
        setup: {
          variables: setupVariables,
          response: setupResponse,
        },
        scenarios: updateScenarios,
        cleanup,
        upstreamCalls: [],
      },
      null,
      2,
    )}\n`,
    'utf8',
  );

  await writeFile(
    path.join(outputDir, 'product-set-input-length-validation.json'),
    `${JSON.stringify(
      {
        capturedAt,
        apiVersion,
        storeDomain,
        scenarios: productSetScenarios,
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
        outputDir,
        files: [
          'product-create-input-validation.json',
          'product-update-input-length-validation.json',
          'product-set-input-length-validation.json',
        ],
      },
      null,
      2,
    ),
  );
} finally {
  if (setupProductId !== null) {
    await cleanupProduct(setupProductId);
  }
}

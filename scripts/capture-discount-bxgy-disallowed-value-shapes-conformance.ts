/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlPayload } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { assertDiscountConformanceScopes, probeDiscountConformanceScopes } from './discount-conformance-lib.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type ProductCreateData = {
  productCreate?: {
    product?: {
      id?: unknown;
      title?: unknown;
    } | null;
    userErrors?: Array<{ field?: unknown; message?: unknown }> | null;
  } | null;
};

type ProductRecord = {
  id: string;
  title: string;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'discounts');
const outputPath = path.join(outputDir, 'discount-bxgy-disallowed-value-shapes.json');
const requestPath = 'config/parity-requests/discounts/discount-bxgy-disallowed-value-shapes.graphql';
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const adminOptions = {
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
};
const { runGraphql, runGraphqlRaw } = createAdminGraphqlClient(adminOptions);

const productCreateMutation = `#graphql
  mutation DiscountBxgyDisallowedProductCreate($product: ProductCreateInput!) {
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

const productDeleteMutation = `#graphql
  mutation DiscountBxgyDisallowedProductDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

function assertNoUserErrors(label: string, userErrors: unknown): void {
  if (Array.isArray(userErrors) && userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors)}`);
  }
}

function readProduct(label: string, response: ConformanceGraphqlPayload<ProductCreateData>): ProductRecord {
  const create = response.data?.productCreate;
  assertNoUserErrors(label, create?.userErrors);

  const id = create?.product?.id;
  const title = create?.product?.title;
  if (typeof id !== 'string' || typeof title !== 'string') {
    throw new Error(`${label} did not return a product id/title: ${JSON.stringify(response)}`);
  }

  return { id, title };
}

function validDiscountOnQuantityValue(): Record<string, unknown> {
  return {
    discountOnQuantity: {
      quantity: '1',
      effect: {
        percentage: 0.5,
      },
    },
  };
}

function baseCustomerBuys(productId: string): Record<string, unknown> {
  return {
    value: {
      quantity: '1',
    },
    items: {
      products: {
        productsToAdd: [productId],
      },
    },
  };
}

function baseCustomerGets(productId: string, value: Record<string, unknown>): Record<string, unknown> {
  return {
    value,
    items: {
      products: {
        productsToAdd: [productId],
      },
    },
  };
}

function codeInput(
  stamp: number,
  suffix: string,
  buyProductId: string,
  customerGets: Record<string, unknown>,
): Record<string, unknown> {
  return {
    title: `HAR-599 code BXGY ${suffix} ${stamp}`,
    code: `HAR599${suffix}${stamp}`,
    startsAt: '2026-04-25T00:00:00Z',
    combinesWith: {
      productDiscounts: true,
      orderDiscounts: false,
      shippingDiscounts: false,
    },
    context: {
      all: 'ALL',
    },
    customerBuys: baseCustomerBuys(buyProductId),
    customerGets,
  };
}

function automaticInput(
  stamp: number,
  suffix: string,
  buyProductId: string,
  customerGets: Record<string, unknown>,
): Record<string, unknown> {
  return {
    title: `HAR-599 automatic BXGY ${suffix} ${stamp}`,
    startsAt: '2026-04-25T00:00:00Z',
    combinesWith: {
      productDiscounts: true,
      orderDiscounts: false,
      shippingDiscounts: false,
    },
    context: {
      all: 'ALL',
    },
    customerBuys: baseCustomerBuys(buyProductId),
    customerGets,
  };
}

await mkdir(outputDir, { recursive: true });

const scopeProbe = await probeDiscountConformanceScopes(adminOptions);
assertDiscountConformanceScopes(scopeProbe);

const document = await readFile(requestPath, 'utf8');
const stamp = Date.now();
const cleanup: Array<() => Promise<unknown>> = [];
const cleanupResponses: unknown[] = [];
const setupProducts: ProductRecord[] = [];
let variables: Record<string, unknown> | undefined;
let validationResponse: unknown;

try {
  const buyProductResponse = await runGraphql<ProductCreateData>(productCreateMutation, {
    product: {
      title: `HAR-599 BXGY buy product ${stamp}`,
      status: 'ACTIVE',
      vendor: 'HERMES',
      productType: 'CONFORMANCE',
      tags: ['conformance', 'discount-bxgy', 'disallowed-value-shapes', String(stamp)],
    },
  });
  const buyProduct = readProduct('buy productCreate', buyProductResponse);
  cleanup.push(() => runGraphqlRaw(productDeleteMutation, { input: { id: buyProduct.id } }));
  setupProducts.push(buyProduct);

  const getProductResponse = await runGraphql<ProductCreateData>(productCreateMutation, {
    product: {
      title: `HAR-599 BXGY get product ${stamp}`,
      status: 'ACTIVE',
      vendor: 'HERMES',
      productType: 'CONFORMANCE',
      tags: ['conformance', 'discount-bxgy', 'disallowed-value-shapes', String(stamp)],
    },
  });
  const getProduct = readProduct('get productCreate', getProductResponse);
  cleanup.push(() => runGraphqlRaw(productDeleteMutation, { input: { id: getProduct.id } }));
  setupProducts.push(getProduct);

  const percentageGets = baseCustomerGets(getProduct.id, { percentage: 0.5 });
  const discountAmountGets = baseCustomerGets(getProduct.id, {
    discountAmount: {
      amount: '5.00',
      appliesOnEachItem: false,
    },
  });
  const validGets = baseCustomerGets(getProduct.id, validDiscountOnQuantityValue());

  variables = {
    codePercentage: codeInput(stamp, 'PCT', buyProduct.id, percentageGets),
    codeDiscountAmount: codeInput(stamp, 'AMT', buyProduct.id, discountAmountGets),
    codeSubscription: codeInput(stamp, 'SUB', buyProduct.id, {
      ...validGets,
      appliesOnSubscription: true,
    }),
    codeOneTime: codeInput(stamp, 'OTP', buyProduct.id, {
      ...validGets,
      appliesOnOneTimePurchase: false,
    }),
    automaticPercentage: automaticInput(stamp, 'PCT', buyProduct.id, percentageGets),
    automaticDiscountAmount: automaticInput(stamp, 'AMT', buyProduct.id, discountAmountGets),
    automaticSubscription: automaticInput(stamp, 'SUB', buyProduct.id, {
      ...validGets,
      appliesOnSubscription: true,
    }),
    automaticOneTime: automaticInput(stamp, 'OTP', buyProduct.id, {
      ...validGets,
      appliesOnOneTimePurchase: false,
    }),
  };

  validationResponse = (await runGraphqlRaw(document, variables)).payload;
} finally {
  for (const cleanupStep of cleanup.reverse()) {
    try {
      cleanupResponses.push(await cleanupStep());
    } catch (error) {
      cleanupResponses.push({ error: error instanceof Error ? error.message : String(error) });
    }
  }
}

if (variables === undefined || validationResponse === undefined) {
  throw new Error('Capture did not complete validation variables/response.');
}

const fixture = {
  storeDomain,
  apiVersion,
  accessScopes: scopeProbe,
  setup: {
    products: setupProducts,
  },
  validation: {
    query: document,
    variables,
    response: validationResponse,
  },
  cleanup: cleanupResponses.map((response) => {
    if (typeof response === 'object' && response !== null && 'payload' in response) {
      return (response as { payload: unknown }).payload;
    }

    return response;
  }),
  upstreamCalls: [],
};

await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');

console.log(
  JSON.stringify(
    {
      ok: true,
      apiVersion,
      outputPath,
      products: setupProducts.map((product) => product.id),
    },
    null,
    2,
  ),
);

/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { readConformanceScriptConfig } from './conformance-script-config.js';
import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { assertDiscountConformanceScopes, probeDiscountConformanceScopes } from './discount-conformance-lib.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'discounts');
const outputPath = path.join(outputDir, 'discount-invalid-date-range-all-types.json');
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const adminOptions = {
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
};
const { runGraphqlRaw } = createAdminGraphqlClient(adminOptions);

const documents = {
  codeBasic: await readFile('config/parity-requests/discounts/discount-invalid-date-range-code-basic.graphql', 'utf8'),
  codeBxgy: await readFile('config/parity-requests/discounts/discount-invalid-date-range-code-bxgy.graphql', 'utf8'),
  codeFreeShipping: await readFile(
    'config/parity-requests/discounts/discount-invalid-date-range-code-free-shipping.graphql',
    'utf8',
  ),
  automaticBxgy: await readFile(
    'config/parity-requests/discounts/discount-invalid-date-range-automatic-bxgy.graphql',
    'utf8',
  ),
  automaticFreeShipping: await readFile(
    'config/parity-requests/discounts/discount-invalid-date-range-automatic-free-shipping.graphql',
    'utf8',
  ),
};

const productRefsDocument = `#graphql
  query DiscountInvalidDateRangeProductRefs {
    products(first: 2) {
      nodes {
        id
      }
    }
  }
`;

function codeSuffix(): string {
  return Date.now().toString();
}

function invertedDates(): Pick<Record<string, unknown>, 'startsAt' | 'endsAt'> {
  return {
    startsAt: '2026-06-01T00:00:00Z',
    endsAt: '2026-05-01T00:00:00Z',
  };
}

function contextAll(): Record<string, unknown> {
  return {
    context: {
      all: 'ALL',
    },
  };
}

function assertSingleInvalidDateError(response: unknown, inputName: string, label: string): void {
  const userErrors = (
    response as {
      data?: Record<string, { userErrors?: Array<{ field?: unknown; message?: unknown; code?: unknown }> }>;
    }
  ).data
    ? Object.values(
        (
          response as {
            data: Record<string, { userErrors?: Array<{ field?: unknown; message?: unknown; code?: unknown }> }>;
          }
        ).data,
      )[0]?.userErrors
    : undefined;
  const expectedField = [inputName, 'endsAt'];
  if (
    !Array.isArray(userErrors) ||
    userErrors.length !== 1 ||
    JSON.stringify(userErrors[0]?.field) !== JSON.stringify(expectedField) ||
    userErrors[0]?.code !== 'INVALID'
  ) {
    throw new Error(`${label} did not return the expected invalid date userError: ${JSON.stringify(response)}`);
  }
}

await mkdir(outputDir, { recursive: true });

const scopeProbe = await probeDiscountConformanceScopes(adminOptions);
assertDiscountConformanceScopes(scopeProbe);

const productRefsResponse = await runGraphqlRaw(productRefsDocument, {});
const productIds = (
  productRefsResponse.payload as { data?: { products?: { nodes?: Array<{ id?: unknown }> } } }
).data?.products?.nodes
  ?.map((node) => node.id)
  .filter((id): id is string => typeof id === 'string');

if (!productIds || productIds.length < 2) {
  throw new Error(
    `Need at least two existing products for BXGY validation capture: ${JSON.stringify(productRefsResponse)}`,
  );
}

const suffix = codeSuffix();
const cases = {
  codeBasic: {
    query: documents.codeBasic,
    variables: {
      input: {
        title: `HAR-595 code basic invalid dates ${suffix}`,
        code: `HAR595BASIC${suffix}`,
        ...invertedDates(),
        combinesWith: {
          productDiscounts: false,
          orderDiscounts: true,
          shippingDiscounts: false,
        },
        ...contextAll(),
        customerGets: {
          value: {
            percentage: 0.1,
          },
          items: {
            all: true,
          },
        },
      },
    },
  },
  codeBxgy: {
    query: documents.codeBxgy,
    variables: {
      input: {
        title: `HAR-595 code BXGY invalid dates ${suffix}`,
        code: `HAR595BXGY${suffix}`,
        ...invertedDates(),
        combinesWith: {
          productDiscounts: true,
          orderDiscounts: false,
          shippingDiscounts: false,
        },
        ...contextAll(),
        customerBuys: {
          value: {
            quantity: '1',
          },
          items: {
            products: {
              productsToAdd: [productIds[0]],
            },
          },
        },
        customerGets: {
          value: {
            discountOnQuantity: {
              quantity: '1',
              effect: {
                percentage: 1,
              },
            },
          },
          items: {
            products: {
              productsToAdd: [productIds[1]],
            },
          },
        },
      },
    },
  },
  codeFreeShipping: {
    query: documents.codeFreeShipping,
    variables: {
      input: {
        title: `HAR-595 code free shipping invalid dates ${suffix}`,
        code: `HAR595SHIP${suffix}`,
        ...invertedDates(),
        combinesWith: {
          productDiscounts: false,
          orderDiscounts: true,
          shippingDiscounts: false,
        },
        ...contextAll(),
        destination: {
          all: true,
        },
      },
    },
  },
  automaticBxgy: {
    query: documents.automaticBxgy,
    variables: {
      input: {
        title: `HAR-595 automatic BXGY invalid dates ${suffix}`,
        ...invertedDates(),
        combinesWith: {
          productDiscounts: true,
          orderDiscounts: false,
          shippingDiscounts: false,
        },
        ...contextAll(),
        customerBuys: {
          value: {
            quantity: '1',
          },
          items: {
            products: {
              productsToAdd: [productIds[0]],
            },
          },
        },
        customerGets: {
          value: {
            discountOnQuantity: {
              quantity: '1',
              effect: {
                percentage: 1,
              },
            },
          },
          items: {
            products: {
              productsToAdd: [productIds[1]],
            },
          },
        },
      },
    },
  },
  automaticFreeShipping: {
    query: documents.automaticFreeShipping,
    variables: {
      input: {
        title: `HAR-595 automatic free shipping invalid dates ${suffix}`,
        ...invertedDates(),
        combinesWith: {
          productDiscounts: false,
          orderDiscounts: true,
          shippingDiscounts: false,
        },
        ...contextAll(),
        destination: {
          all: true,
        },
      },
    },
  },
};

const responses = {
  codeBasic: await runGraphqlRaw(documents.codeBasic, cases.codeBasic.variables),
  codeBxgy: await runGraphqlRaw(documents.codeBxgy, cases.codeBxgy.variables),
  codeFreeShipping: await runGraphqlRaw(documents.codeFreeShipping, cases.codeFreeShipping.variables),
  automaticBxgy: await runGraphqlRaw(documents.automaticBxgy, cases.automaticBxgy.variables),
  automaticFreeShipping: await runGraphqlRaw(documents.automaticFreeShipping, cases.automaticFreeShipping.variables),
};

assertSingleInvalidDateError(responses.codeBasic.payload, 'basicCodeDiscount', 'discountCodeBasicCreate');
assertSingleInvalidDateError(responses.codeBxgy.payload, 'bxgyCodeDiscount', 'discountCodeBxgyCreate');
assertSingleInvalidDateError(
  responses.codeFreeShipping.payload,
  'freeShippingCodeDiscount',
  'discountCodeFreeShippingCreate',
);
assertSingleInvalidDateError(responses.automaticBxgy.payload, 'automaticBxgyDiscount', 'discountAutomaticBxgyCreate');
assertSingleInvalidDateError(
  responses.automaticFreeShipping.payload,
  'freeShippingAutomaticDiscount',
  'discountAutomaticFreeShippingCreate',
);

const fixture = {
  storeDomain,
  apiVersion,
  capturedAt: new Date().toISOString(),
  accessScopes: scopeProbe,
  setup: {
    productIds,
  },
  upstreamCalls: [],
  cases: {
    codeBasic: {
      ...cases.codeBasic,
      response: responses.codeBasic.payload,
    },
    codeBxgy: {
      ...cases.codeBxgy,
      response: responses.codeBxgy.payload,
    },
    codeFreeShipping: {
      ...cases.codeFreeShipping,
      response: responses.codeFreeShipping.payload,
    },
    automaticBxgy: {
      ...cases.automaticBxgy,
      response: responses.automaticBxgy.payload,
    },
    automaticFreeShipping: {
      ...cases.automaticFreeShipping,
      response: responses.automaticFreeShipping.payload,
    },
  },
};

await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');

console.log(
  JSON.stringify(
    {
      ok: true,
      apiVersion,
      output: outputPath,
      productIds,
    },
    null,
    2,
  ),
);

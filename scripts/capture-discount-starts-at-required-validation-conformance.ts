/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { readConformanceScriptConfig } from './conformance-script-config.js';
import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { assertDiscountConformanceScopes, probeDiscountConformanceScopes } from './discount-conformance-lib.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type MutationPayload = {
  codeDiscountNode?: { id?: unknown } | null;
  automaticDiscountNode?: { id?: unknown } | null;
  userErrors?: Array<{ field?: unknown; message?: unknown; code?: unknown }>;
};

type GraphqlPayload = {
  data?: Record<string, MutationPayload | null> | null;
  errors?: unknown;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'discounts');
const outputPath = path.join(outputDir, 'discount-starts-at-required-validation.json');
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const adminOptions = {
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
};
const { runGraphqlRaw } = createAdminGraphqlClient(adminOptions);

const document = await readFile(
  'config/parity-requests/discounts/discount-starts-at-required-validation.graphql',
  'utf8',
);

const productRefsDocument = `#graphql
  query DiscountStartsAtRequiredProductRefs {
    products(first: 2) {
      nodes {
        id
      }
    }
  }
`;

const userErrorsSelection = `#graphql
  userErrors {
    field
    message
    code
    extraInfo
  }
`;

const codeCleanupDocument = `#graphql
  mutation DiscountStartsAtRequiredCodeCleanup($id: ID!) {
    discountCodeDelete(id: $id) {
      deletedCodeDiscountId
      ${userErrorsSelection}
    }
  }
`;

const automaticCleanupDocument = `#graphql
  mutation DiscountStartsAtRequiredAutomaticCleanup($id: ID!) {
    discountAutomaticDelete(id: $id) {
      deletedAutomaticDiscountId
      ${userErrorsSelection}
    }
  }
`;

const responseRoots = [
  {
    alias: 'basicCode',
    inputName: 'basicCodeDiscount',
    nodeField: 'codeDiscountNode',
    cleanupDocument: codeCleanupDocument,
  },
  {
    alias: 'bxgyCode',
    inputName: 'bxgyCodeDiscount',
    nodeField: 'codeDiscountNode',
    cleanupDocument: codeCleanupDocument,
  },
  {
    alias: 'freeShippingCode',
    inputName: 'freeShippingCodeDiscount',
    nodeField: 'codeDiscountNode',
    cleanupDocument: codeCleanupDocument,
  },
  {
    alias: 'automaticBasic',
    inputName: 'automaticBasicDiscount',
    nodeField: 'automaticDiscountNode',
    cleanupDocument: automaticCleanupDocument,
  },
  {
    alias: 'automaticBxgy',
    inputName: 'automaticBxgyDiscount',
    nodeField: 'automaticDiscountNode',
    cleanupDocument: automaticCleanupDocument,
  },
  {
    alias: 'automaticFreeShipping',
    inputName: 'freeShippingAutomaticDiscount',
    nodeField: 'automaticDiscountNode',
    cleanupDocument: automaticCleanupDocument,
  },
] as const;

function codeSuffix(): string {
  return Date.now().toString();
}

function contextAll(): Record<string, unknown> {
  return {
    context: {
      all: 'ALL',
    },
  };
}

function withMaybeStartsAt(
  input: Record<string, unknown>,
  startsAt: string | null | undefined,
): Record<string, unknown> {
  if (startsAt === undefined) {
    return input;
  }

  return {
    ...input,
    startsAt,
  };
}

function variablesFor(
  productIds: [string, string],
  suffix: string,
  startsAt: string | null | undefined,
): Record<string, unknown> {
  return {
    basicCode: withMaybeStartsAt(
      {
        title: `StartsAt required code basic ${suffix}`,
        code: `STARTSATBASIC${suffix}`,
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
      startsAt,
    ),
    bxgyCode: withMaybeStartsAt(
      {
        title: `StartsAt required code BXGY ${suffix}`,
        code: `STARTSATBXGY${suffix}`,
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
      startsAt,
    ),
    freeShippingCode: withMaybeStartsAt(
      {
        title: `StartsAt required code free shipping ${suffix}`,
        code: `STARTSATSHIP${suffix}`,
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
      startsAt,
    ),
    automaticBasic: withMaybeStartsAt(
      {
        title: `StartsAt required automatic basic ${suffix}`,
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
      startsAt,
    ),
    automaticBxgy: withMaybeStartsAt(
      {
        title: `StartsAt required automatic BXGY ${suffix}`,
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
      startsAt,
    ),
    automaticFreeShipping: withMaybeStartsAt(
      {
        title: `StartsAt required automatic free shipping ${suffix}`,
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
      startsAt,
    ),
  };
}

function assertStartsAtRequiredResponse(payload: GraphqlPayload, label: string): void {
  if (payload.errors) {
    throw new Error(`${label} returned top-level errors: ${JSON.stringify(payload)}`);
  }
  if (!payload.data) {
    throw new Error(`${label} did not return a data object: ${JSON.stringify(payload)}`);
  }

  for (const root of responseRoots) {
    const rootPayload = payload.data[root.alias];
    if (!rootPayload) {
      throw new Error(`${label}.${root.alias} did not return a payload: ${JSON.stringify(payload)}`);
    }
    if (rootPayload[root.nodeField] !== null) {
      throw new Error(`${label}.${root.alias} unexpectedly created a discount: ${JSON.stringify(rootPayload)}`);
    }

    const userErrors = rootPayload.userErrors;
    const expectedField = [root.inputName, 'startsAt'];
    if (
      !Array.isArray(userErrors) ||
      userErrors.length !== 1 ||
      JSON.stringify(userErrors[0]?.field) !== JSON.stringify(expectedField) ||
      typeof userErrors[0]?.message !== 'string'
    ) {
      throw new Error(
        `${label}.${root.alias} did not return the expected startsAt userError: ${JSON.stringify(rootPayload)}`,
      );
    }
  }
}

function collectUnexpectedDiscounts(payload: GraphqlPayload): Array<{ id: string; cleanupDocument: string }> {
  if (!payload.data) {
    return [];
  }

  const ids: Array<{ id: string; cleanupDocument: string }> = [];
  for (const root of responseRoots) {
    const rootPayload = payload.data[root.alias];
    const node = rootPayload?.[root.nodeField];
    const id = node && typeof node === 'object' && 'id' in node ? node.id : undefined;
    if (typeof id === 'string' && id.length > 0) {
      ids.push({ id, cleanupDocument: root.cleanupDocument });
    }
  }
  return ids;
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
    `Need at least two existing products for startsAt required validation capture: ${JSON.stringify(productRefsResponse)}`,
  );
}

const productIdPair: [string, string] = [productIds[0]!, productIds[1]!];
const omittedVariables = variablesFor(productIdPair, `${codeSuffix()}OMITTED`, undefined);
const explicitNullVariables = variablesFor(productIdPair, `${codeSuffix()}NULL`, null);

const omittedResponse = await runGraphqlRaw(document, omittedVariables);
const explicitNullResponse = await runGraphqlRaw(document, explicitNullVariables);

const unexpectedDiscounts = [
  ...collectUnexpectedDiscounts(omittedResponse.payload as GraphqlPayload),
  ...collectUnexpectedDiscounts(explicitNullResponse.payload as GraphqlPayload),
];
const cleanup = [];
for (const discount of unexpectedDiscounts) {
  cleanup.push({
    id: discount.id,
    response: (await runGraphqlRaw(discount.cleanupDocument, { id: discount.id })).payload,
  });
}

assertStartsAtRequiredResponse(omittedResponse.payload as GraphqlPayload, 'omitted startsAt');
assertStartsAtRequiredResponse(explicitNullResponse.payload as GraphqlPayload, 'explicit null startsAt');

const fixture = {
  storeDomain,
  apiVersion,
  capturedAt: new Date().toISOString(),
  accessScopes: scopeProbe,
  setup: {
    productIds: productIdPair,
  },
  upstreamCalls: [],
  cleanup,
  cases: {
    omitted: {
      query: document,
      variables: omittedVariables,
      response: omittedResponse.payload,
    },
    explicitNull: {
      query: document,
      variables: explicitNullVariables,
      response: explicitNullResponse.payload,
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
      productIds: productIdPair,
      cleanupCount: cleanup.length,
    },
    null,
    2,
  ),
);

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
const validationOutputPath = path.join(outputDir, 'discount-validation-branches.json');
const codeRequiredValidationOutputPath = path.join(outputDir, 'discount-code-required-blank-validation.json');
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const adminOptions = {
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
};
const { runGraphqlRaw } = createAdminGraphqlClient(adminOptions);

const discountProductProbeDocument = `#graphql
  query DiscountCodeRequiredBlankValidationProducts {
    products(first: 1) {
      nodes {
        id
      }
    }
  }
`;

const discountValidationDocument = await readFile(
  'config/parity-requests/discounts/discount-validation-branches.graphql',
  'utf8',
);
const codeRequiredValidationDocument = await readFile(
  'config/parity-requests/discounts/discount-code-required-blank-validation.graphql',
  'utf8',
);
const missingInputDocument = await readFile(
  'config/parity-requests/discounts/discountCodeBasicCreate-missing-input.graphql',
  'utf8',
);
const inlineNullInputDocument = await readFile(
  'config/parity-requests/discounts/discountCodeBasicCreate-inline-null-input.graphql',
  'utf8',
);

const userErrorsSelection = `#graphql
  userErrors {
    field
    message
    code
    extraInfo
  }
`;

const seedCreateDocument = `#graphql
  mutation DiscountValidationSeedCreate($input: DiscountCodeBasicInput!) {
    discountCodeBasicCreate(basicCodeDiscount: $input) {
      codeDiscountNode {
        id
      }
      ${userErrorsSelection}
    }
  }
`;

const seedCleanupDocument = `#graphql
  mutation DiscountValidationSeedCleanup($id: ID!) {
    discountCodeDelete(id: $id) {
      deletedCodeDiscountId
      ${userErrorsSelection}
    }
  }
`;

function basicInput(code: string): Record<string, unknown> {
  return {
    title: `HAR-198 ${code}`,
    code,
    startsAt: '2026-04-25T00:00:00Z',
    combinesWith: {
      productDiscounts: false,
      orderDiscounts: true,
      shippingDiscounts: false,
    },
    context: {
      all: 'ALL',
    },
    minimumRequirement: {
      subtotal: {
        greaterThanOrEqualToSubtotal: '1.00',
      },
    },
    customerGets: {
      value: {
        percentage: 0.1,
      },
      items: {
        all: true,
      },
    },
  };
}

function withoutCode(input: Record<string, unknown>): Record<string, unknown> {
  const copy = { ...input };
  delete copy['code'];
  return copy;
}

function bxgyInput(code: string, productId: string): Record<string, unknown> {
  return {
    title: `HAR-596 BXGY ${code}`,
    code,
    startsAt: '2026-04-25T00:00:00Z',
    combinesWith: {
      productDiscounts: true,
      orderDiscounts: false,
      shippingDiscounts: false,
    },
    context: {
      all: 'ALL',
    },
    customerBuys: {
      value: {
        quantity: '1',
      },
      items: {
        products: {
          productsToAdd: [productId],
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
          productsToAdd: [productId],
        },
      },
    },
  };
}

function freeShippingInput(code: string): Record<string, unknown> {
  return {
    title: `HAR-596 free shipping ${code}`,
    code,
    startsAt: '2026-04-25T00:00:00Z',
    combinesWith: {
      productDiscounts: false,
      orderDiscounts: false,
      shippingDiscounts: false,
    },
    context: {
      all: 'ALL',
    },
    destination: {
      all: true,
    },
  };
}

function readCreatedSeedId(response: unknown): string {
  const id = (
    response as { payload?: { data?: { discountCodeBasicCreate?: { codeDiscountNode?: { id?: unknown } } } } }
  ).payload?.data?.discountCodeBasicCreate?.codeDiscountNode?.id;
  if (typeof id !== 'string' || id.length === 0) {
    throw new Error(`Seed discount create did not return an id: ${JSON.stringify(response)}`);
  }

  return id;
}

function readProductId(response: unknown): string {
  const id = (response as { payload?: { data?: { products?: { nodes?: Array<{ id?: unknown }> } } } }).payload?.data
    ?.products?.nodes?.[0]?.id;
  if (typeof id !== 'string' || id.length === 0) {
    throw new Error(
      `Discount code validation capture requires at least one product in the test shop: ${JSON.stringify(response)}`,
    );
  }

  return id;
}

await mkdir(outputDir, { recursive: true });

const scopeProbe = await probeDiscountConformanceScopes(adminOptions);
assertDiscountConformanceScopes(scopeProbe);

const stamp = Date.now();
const duplicateCode = `HAR198DUP${stamp}`;
const duplicateSeedInput = basicInput(duplicateCode);
const duplicateSeedCreate = await runGraphqlRaw(seedCreateDocument, { input: duplicateSeedInput });
const duplicateSeedId = readCreatedSeedId(duplicateSeedCreate);
const productProbe = await runGraphqlRaw(discountProductProbeDocument, {});
const productId = readProductId(productProbe);
const tooLongCode = 'X'.repeat(256);

const validationVariables = {
  duplicate: duplicateSeedInput,
  badRefs: {
    ...basicInput(`HAR198BADREF${stamp}`),
    customerGets: {
      value: {
        percentage: 0.1,
      },
      items: {
        products: {
          productsToAdd: ['gid://shopify/Product/0'],
          productVariantsToAdd: ['gid://shopify/ProductVariant/0'],
        },
        collections: {
          add: ['gid://shopify/Collection/0'],
        },
      },
    },
  },
  invalidAutomatic: {
    title: `HAR-198 invalid automatic dates ${stamp}`,
    startsAt: '2026-04-25T00:00:00Z',
    endsAt: '2026-04-24T00:00:00Z',
    combinesWith: {
      productDiscounts: false,
      orderDiscounts: true,
      shippingDiscounts: false,
    },
    context: {
      all: 'ALL',
    },
    minimumRequirement: {
      quantity: {
        greaterThanOrEqualToQuantity: '2',
      },
    },
    customerGets: {
      value: {
        percentage: 0.15,
      },
      items: {
        all: true,
      },
    },
  },
  blankCodeBxgy: {
    title: '',
    code: `HAR198BXGY${stamp}`,
    startsAt: '2026-04-25T00:00:00Z',
    combinesWith: {
      productDiscounts: true,
      orderDiscounts: true,
      shippingDiscounts: true,
    },
    context: {
      all: 'ALL',
    },
    customerBuys: {
      value: {
        quantity: '1',
      },
      items: {
        all: true,
      },
    },
    customerGets: {
      value: {
        discountOnQuantity: {
          quantity: '1',
          effect: {
            percentage: 1.0,
          },
        },
      },
      items: {
        all: true,
      },
    },
  },
  blankAutomaticBxgy: {
    title: '',
    startsAt: '2026-04-25T00:00:00Z',
    combinesWith: {
      productDiscounts: true,
      orderDiscounts: true,
      shippingDiscounts: true,
    },
    context: {
      all: 'ALL',
    },
    customerBuys: {
      value: {
        quantity: '1',
      },
      items: {
        all: true,
      },
    },
    customerGets: {
      value: {
        discountOnQuantity: {
          quantity: '1',
          effect: {
            percentage: 1.0,
          },
        },
      },
      items: {
        all: true,
      },
    },
  },
  invalidCodeFreeShipping: {
    title: '',
    code: `HAR198FREE${stamp}`,
    startsAt: '2026-04-25T00:00:00Z',
    combinesWith: {
      productDiscounts: true,
      orderDiscounts: true,
      shippingDiscounts: true,
    },
    context: {
      all: 'ALL',
    },
    destination: {
      all: true,
    },
  },
  invalidAutomaticFreeShipping: {
    title: '',
    startsAt: '2026-04-25T00:00:00Z',
    combinesWith: {
      productDiscounts: true,
      orderDiscounts: true,
      shippingDiscounts: true,
    },
    context: {
      all: 'ALL',
    },
    destination: {
      all: true,
    },
  },
  unknownUpdateId: 'gid://shopify/DiscountCodeNode/0',
  unknownUpdate: basicInput(`HAR198UNKNOWN${stamp}`),
  codeBulkIds: ['gid://shopify/DiscountCodeNode/0'],
  automaticBulkIds: ['gid://shopify/DiscountAutomaticNode/0'],
  bulkSearch: 'status:active',
};

const codeRequiredValidationVariables = {
  basicMissing: withoutCode(basicInput(`HAR596BASICMISSING${stamp}`)),
  basicEmpty: basicInput(''),
  basicNewline: basicInput('abc\ndef'),
  basicTooLong: {
    ...basicInput(tooLongCode),
    title: `HAR-596 basic too long ${stamp}`,
  },
  bxgyMissing: withoutCode(bxgyInput(`HAR596BXGYMISSING${stamp}`, productId)),
  bxgyEmpty: bxgyInput('', productId),
  bxgyNewline: bxgyInput('abc\ndef', productId),
  freeShippingMissing: withoutCode(freeShippingInput(`HAR596FREEMISSING${stamp}`)),
  freeShippingEmpty: freeShippingInput(''),
  freeShippingNewline: freeShippingInput('abc\ndef'),
};

const [missingInputResponse, inlineNullInputResponse, validationResponse, codeRequiredValidationResponse] =
  await Promise.all([
    runGraphqlRaw(missingInputDocument, {}),
    runGraphqlRaw(inlineNullInputDocument, {}),
    runGraphqlRaw(discountValidationDocument, validationVariables),
    runGraphqlRaw(codeRequiredValidationDocument, codeRequiredValidationVariables),
  ]);

const cleanup = await runGraphqlRaw(seedCleanupDocument, { id: duplicateSeedId });

const fixture = {
  storeDomain,
  apiVersion,
  accessScopes: scopeProbe,
  seedDiscounts: [
    {
      id: duplicateSeedId,
      discount: {
        __typename: 'DiscountCodeBasic',
        title: duplicateSeedInput['title'],
        status: 'ACTIVE',
        summary: '10% off entire order',
        startsAt: duplicateSeedInput['startsAt'],
        endsAt: null,
        asyncUsageCount: 0,
        discountClasses: ['ORDER'],
        combinesWith: duplicateSeedInput['combinesWith'],
        codes: {
          nodes: [
            {
              id: `gid://shopify/DiscountRedeemCode/${duplicateSeedId.split('/').at(-1)}`,
              code: duplicateCode,
              asyncUsageCount: 0,
            },
          ],
        },
      },
    },
  ],
  validation: {
    missingInput: {
      query: missingInputDocument,
      variables: {},
      response: missingInputResponse.payload,
    },
    inlineNullInput: {
      query: inlineNullInputDocument,
      variables: {},
      response: inlineNullInputResponse.payload,
    },
    omnibus: {
      query: discountValidationDocument,
      variables: validationVariables,
      response: validationResponse.payload,
    },
  },
  cleanup: cleanup.payload,
  upstreamCalls: [
    {
      operationName: 'DiscountUniquenessCheck',
      variables: {
        code: duplicateCode,
      },
      query: 'sha:DiscountUniquenessCheck',
      response: {
        status: 200,
        body: {
          data: {
            codeDiscountNodeByCode: {
              id: duplicateSeedId,
            },
          },
        },
      },
    },
  ],
};

const codeRequiredValidationFixture = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  accessScopes: scopeProbe,
  codeRequiredValidation: {
    query: codeRequiredValidationDocument,
    variables: codeRequiredValidationVariables,
    response: codeRequiredValidationResponse.payload,
  },
  upstreamCalls: [],
};

await writeFile(validationOutputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
await writeFile(
  codeRequiredValidationOutputPath,
  `${JSON.stringify(codeRequiredValidationFixture, null, 2)}\n`,
  'utf8',
);

console.log(
  JSON.stringify(
    {
      ok: true,
      apiVersion,
      outputs: [validationOutputPath, codeRequiredValidationOutputPath],
      duplicateSeedId,
    },
    null,
    2,
  ),
);

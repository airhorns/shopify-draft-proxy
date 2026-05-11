/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { assertDiscountConformanceScopes, probeDiscountConformanceScopes } from './discount-conformance-lib.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

function readPath(value: unknown, pathSegments: string[]): unknown {
  let current = value;
  for (const segment of pathSegments) {
    if (Array.isArray(current)) {
      const index = Number.parseInt(segment, 10);
      if (!Number.isInteger(index)) return undefined;
      current = current[index];
      continue;
    }
    if (current === null || typeof current !== 'object') return undefined;
    current = (current as JsonRecord)[segment];
  }
  return current;
}

function readRequiredString(result: ConformanceGraphqlResult, pathSegments: string[], context: string): string {
  const value = readPath(result.payload, pathSegments);
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`${context} did not return ${pathSegments.join('.')}: ${JSON.stringify(result.payload, null, 2)}`);
  }
  return value;
}

function assertNoTopLevelErrors(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertBadRequest(
  result: ConformanceGraphqlResult,
  context: string,
  message: string,
  responseKey: string,
): void {
  if (result.status < 200 || result.status >= 300) {
    throw new Error(`${context} failed with HTTP ${result.status}: ${JSON.stringify(result.payload, null, 2)}`);
  }
  const errors = result.payload.errors;
  if (!Array.isArray(errors) || errors.length !== 1) {
    throw new Error(`${context} expected one top-level error: ${JSON.stringify(result.payload, null, 2)}`);
  }
  const error = errors[0] as JsonRecord;
  if (error['message'] !== message) {
    throw new Error(`${context} expected message ${message}: ${JSON.stringify(result.payload, null, 2)}`);
  }
  const extensions = error['extensions'] as JsonRecord | undefined;
  if (extensions?.['code'] !== 'BAD_REQUEST') {
    throw new Error(`${context} expected BAD_REQUEST extension: ${JSON.stringify(result.payload, null, 2)}`);
  }
  const data = result.payload.data as JsonRecord | undefined;
  if (data?.[responseKey] !== null) {
    throw new Error(`${context} expected data.${responseKey} null: ${JSON.stringify(result.payload, null, 2)}`);
  }
}

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'discounts');
const outputPath = path.join(outputDir, 'discount-add-remove-overlap.json');
const setupDocument = await readFile(
  'config/parity-requests/discounts/discount-add-remove-overlap-setup.graphql',
  'utf8',
);
const basicCreateDocument = await readFile(
  'config/parity-requests/discounts/discount-add-remove-overlap-basic-create.graphql',
  'utf8',
);
const basicUpdateDocument = await readFile(
  'config/parity-requests/discounts/discount-add-remove-overlap-basic-update.graphql',
  'utf8',
);
const bxgyCreateDocument = await readFile(
  'config/parity-requests/discounts/discount-add-remove-overlap-bxgy-create.graphql',
  'utf8',
);
const bxgyUpdateDocument = await readFile(
  'config/parity-requests/discounts/discount-add-remove-overlap-bxgy-update.graphql',
  'utf8',
);
const freeShippingCreateDocument = await readFile(
  'config/parity-requests/discounts/discount-add-remove-overlap-free-shipping-create.graphql',
  'utf8',
);
const freeShippingUpdateDocument = await readFile(
  'config/parity-requests/discounts/discount-add-remove-overlap-free-shipping-update.graphql',
  'utf8',
);

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const adminOptions = {
  adminOrigin,
  apiVersion,
  accessToken: adminAccessToken,
  headers: buildAdminAuthHeaders(adminAccessToken),
};
const { runGraphqlRaw } = createAdminGraphqlClient(adminOptions);

await mkdir(outputDir, { recursive: true });

const scopeProbe = await probeDiscountConformanceScopes(adminOptions);
assertDiscountConformanceScopes(scopeProbe);

const runId = Date.now();
const startsAt = '2026-05-05T00:00:00Z';
const cleanup: JsonRecord = {};
const setupDiscountIds: string[] = [];

const productProbeDocument = `#graphql
  query DiscountAddRemoveOverlapProductProbe {
    products(first: 1) {
      nodes {
        id
        variants(first: 1) {
          nodes {
            id
          }
        }
      }
    }
    collections(first: 1) {
      nodes {
        id
      }
    }
  }
`;

const discountDeleteDocument = `#graphql
  mutation DiscountAddRemoveOverlapDiscountDelete($id: ID!) {
    discountCodeDelete(id: $id) {
      deletedCodeDiscountId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

function readFixtureRefs(productProbe: ConformanceGraphqlResult): {
  productId: string;
  variantId: string;
  collectionId: string;
} {
  const productId = readRequiredString(productProbe, ['data', 'products', 'nodes', '0', 'id'], 'product probe');
  const variantId = readRequiredString(
    productProbe,
    ['data', 'products', 'nodes', '0', 'variants', 'nodes', '0', 'id'],
    'product variant probe',
  );
  const collectionId = readRequiredString(
    productProbe,
    ['data', 'collections', 'nodes', '0', 'id'],
    'collection probe',
  );
  return { productId, variantId, collectionId };
}

function basicInput(code: string): JsonRecord {
  return {
    title: `Add remove overlap basic ${code}`,
    code,
    startsAt,
    context: { all: 'ALL' },
    customerGets: {
      value: { percentage: 0.1 },
      items: { all: true },
    },
  };
}

function bxgyInput(code: string, productId: string): JsonRecord {
  return {
    title: `Add remove overlap bxgy ${code}`,
    code,
    startsAt,
    context: { all: 'ALL' },
    customerBuys: {
      value: { quantity: '1' },
      items: { products: { productsToAdd: [productId] } },
    },
    customerGets: {
      value: {
        discountOnQuantity: {
          quantity: '1',
          effect: { percentage: 1 },
        },
      },
      items: { products: { productsToAdd: [productId] } },
    },
  };
}

function freeShippingInput(code: string): JsonRecord {
  return {
    title: `Add remove overlap shipping ${code}`,
    code,
    startsAt,
    context: { all: 'ALL' },
    destination: { all: true },
  };
}

function withCustomerOverlap(input: JsonRecord): JsonRecord {
  const { context: _context, ...rest } = input;
  return {
    ...rest,
    customerSelection: {
      customers: {
        add: ['gid://shopify/Customer/1'],
        remove: ['gid://shopify/Customer/1'],
      },
    },
  };
}

function withCustomerGetsCollectionOverlap(input: JsonRecord, collectionId: string): JsonRecord {
  return {
    ...input,
    customerGets: {
      ...(input['customerGets'] as JsonRecord | undefined),
      items: {
        collections: {
          add: [collectionId],
          remove: [collectionId],
        },
      },
    },
  };
}

function withCustomerBuysVariantOverlap(input: JsonRecord, variantId: string): JsonRecord {
  return {
    ...input,
    customerBuys: {
      ...(input['customerBuys'] as JsonRecord | undefined),
      items: {
        products: {
          productVariantsToAdd: [variantId],
          productVariantsToRemove: [variantId],
        },
      },
    },
  };
}

function withCountryOverlap(input: JsonRecord): JsonRecord {
  return {
    ...input,
    destination: {
      countries: {
        add: ['US'],
        remove: ['US'],
      },
    },
  };
}

function readSetupDiscountId(response: ConformanceGraphqlResult, alias: string): string {
  return readRequiredString(response, ['data', alias, 'codeDiscountNode', 'id'], `${alias} setup`);
}

try {
  const productProbe = await runGraphqlRaw(productProbeDocument, {});
  assertNoTopLevelErrors(productProbe, 'product/collection probe');
  const { productId, variantId, collectionId } = readFixtureRefs(productProbe);

  const setupCodes = {
    basicCode: `DOVRSETB${runId}`,
    bxgyCode: `DOVRSETX${runId}`,
    freeShippingCode: `DOVRSETS${runId}`,
  };
  const setupVariables = {
    basicCode: basicInput(setupCodes.basicCode),
    bxgyCode: bxgyInput(setupCodes.bxgyCode, productId),
    freeShippingCode: freeShippingInput(setupCodes.freeShippingCode),
  };
  const setup = await runGraphqlRaw(setupDocument, setupVariables);
  assertNoTopLevelErrors(setup, 'setup discount create');
  setupDiscountIds.push(readSetupDiscountId(setup, 'basicCode'));
  setupDiscountIds.push(readSetupDiscountId(setup, 'bxgyCode'));
  setupDiscountIds.push(readSetupDiscountId(setup, 'freeShippingCode'));

  const customerSelectionCreateVariables = {
    input: withCustomerOverlap(basicInput(`DOVRCUSC${runId}`)),
  };
  const customerSelectionCreate = await runGraphqlRaw(basicCreateDocument, customerSelectionCreateVariables);
  assertBadRequest(
    customerSelectionCreate,
    'customerSelection create overlap',
    'A customer id is present in `add` and `remove` fields',
    'discountCodeBasicCreate',
  );

  const customerGetsCreateVariables = {
    input: withCustomerGetsCollectionOverlap(basicInput(`DOVRGETC${runId}`), collectionId),
  };
  const customerGetsCreate = await runGraphqlRaw(basicCreateDocument, customerGetsCreateVariables);
  assertBadRequest(
    customerGetsCreate,
    'customerGets create overlap',
    "The same Collection id is present in both 'add' and 'remove' fields",
    'discountCodeBasicCreate',
  );

  const customerBuysCreateVariables = {
    input: withCustomerBuysVariantOverlap(bxgyInput(`DOVRBUYC${runId}`, productId), variantId),
  };
  const customerBuysCreate = await runGraphqlRaw(bxgyCreateDocument, customerBuysCreateVariables);
  assertBadRequest(
    customerBuysCreate,
    'customerBuys create overlap',
    "The same ProductVariant id is present in both 'add' and 'remove' fields",
    'discountCodeBxgyCreate',
  );

  const countriesCreateVariables = {
    input: withCountryOverlap(freeShippingInput(`DOVRCTYC${runId}`)),
  };
  const countriesCreate = await runGraphqlRaw(freeShippingCreateDocument, countriesCreateVariables);
  assertBadRequest(
    countriesCreate,
    'countries create overlap',
    'A country code is present in `add` and `remove` field',
    'discountCodeFreeShippingCreate',
  );

  const customerSelectionUpdateVariables = {
    id: setupDiscountIds[0],
    input: withCustomerOverlap(basicInput(setupCodes.basicCode)),
  };
  const customerSelectionUpdate = await runGraphqlRaw(basicUpdateDocument, customerSelectionUpdateVariables);
  assertBadRequest(
    customerSelectionUpdate,
    'customerSelection update overlap',
    'A customer id is present in `add` and `remove` fields',
    'discountCodeBasicUpdate',
  );

  const customerGetsUpdateVariables = {
    id: setupDiscountIds[0],
    input: withCustomerGetsCollectionOverlap(basicInput(setupCodes.basicCode), collectionId),
  };
  const customerGetsUpdate = await runGraphqlRaw(basicUpdateDocument, customerGetsUpdateVariables);
  assertBadRequest(
    customerGetsUpdate,
    'customerGets update overlap',
    "The same Collection id is present in both 'add' and 'remove' fields",
    'discountCodeBasicUpdate',
  );

  const customerBuysUpdateVariables = {
    id: setupDiscountIds[1],
    input: withCustomerBuysVariantOverlap(bxgyInput(setupCodes.bxgyCode, productId), variantId),
  };
  const customerBuysUpdate = await runGraphqlRaw(bxgyUpdateDocument, customerBuysUpdateVariables);
  assertBadRequest(
    customerBuysUpdate,
    'customerBuys update overlap',
    "The same ProductVariant id is present in both 'add' and 'remove' fields",
    'discountCodeBxgyUpdate',
  );

  const countriesUpdateVariables = {
    id: setupDiscountIds[2],
    input: withCountryOverlap(freeShippingInput(setupCodes.freeShippingCode)),
  };
  const countriesUpdate = await runGraphqlRaw(freeShippingUpdateDocument, countriesUpdateVariables);
  assertBadRequest(
    countriesUpdate,
    'countries update overlap',
    'A country code is present in `add` and `remove` field',
    'discountCodeFreeShippingUpdate',
  );

  for (const [index, id] of setupDiscountIds.entries()) {
    cleanup[`discountDelete${index + 1}`] = await runGraphqlRaw(discountDeleteDocument, { id });
  }

  const fixture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    accessScopes: scopeProbe,
    setup: {
      query: setupDocument,
      variables: setupVariables,
      response: setup.payload,
      discountIds: setupDiscountIds,
      productId,
      variantId,
      collectionId,
    },
    cases: {
      customerSelectionCreate: {
        query: basicCreateDocument,
        variables: customerSelectionCreateVariables,
        response: customerSelectionCreate.payload,
      },
      customerGetsCreate: {
        query: basicCreateDocument,
        variables: customerGetsCreateVariables,
        response: customerGetsCreate.payload,
      },
      customerBuysCreate: {
        query: bxgyCreateDocument,
        variables: customerBuysCreateVariables,
        response: customerBuysCreate.payload,
      },
      countriesCreate: {
        query: freeShippingCreateDocument,
        variables: countriesCreateVariables,
        response: countriesCreate.payload,
      },
      customerSelectionUpdate: {
        query: basicUpdateDocument,
        variables: customerSelectionUpdateVariables,
        response: customerSelectionUpdate.payload,
      },
      customerGetsUpdate: {
        query: basicUpdateDocument,
        variables: customerGetsUpdateVariables,
        response: customerGetsUpdate.payload,
      },
      customerBuysUpdate: {
        query: bxgyUpdateDocument,
        variables: customerBuysUpdateVariables,
        response: customerBuysUpdate.payload,
      },
      countriesUpdate: {
        query: freeShippingUpdateDocument,
        variables: countriesUpdateVariables,
        response: countriesUpdate.payload,
      },
    },
    cleanup,
    upstreamCalls: [],
  };

  await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');

  console.log(
    JSON.stringify(
      {
        ok: true,
        apiVersion,
        outputPath,
        setupDiscountIds,
        productId,
        variantId,
        collectionId,
      },
      null,
      2,
    ),
  );
} finally {
  for (const [index, id] of setupDiscountIds.entries()) {
    const key = `discountDelete${index + 1}`;
    if (!cleanup[key]) cleanup[`${key}AfterFailure`] = await runGraphqlRaw(discountDeleteDocument, { id });
  }
}

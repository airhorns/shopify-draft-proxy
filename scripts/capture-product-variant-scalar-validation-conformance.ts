/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type CaptureEntry = {
  query: string;
  variables: Record<string, unknown>;
  result: ConformanceGraphqlResult;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const outputPath = path.join(outputDir, 'productVariantsBulkCreate-validation.json');
const { runGraphql, runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const productCreateMutation = `#graphql
  mutation ProductVariantScalarValidationCreate($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
        id
        title
        status
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const productOptionsCreateMutation = `#graphql
  mutation ProductVariantScalarValidationOptions($productId: ID!, $options: [OptionCreateInput!]!) {
    productOptionsCreate(productId: $productId, options: $options) {
      product {
        options {
          id
          name
          optionValues {
            id
            name
          }
        }
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const productDeleteMutation = `#graphql
  mutation ProductVariantScalarValidationDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

const productStateQuery = `#graphql
  query ProductVariantScalarValidationState($id: ID!) {
    product(id: $id) {
      id
      totalInventory
      tracksInventory
      variants(first: 20) {
        nodes {
          id
          title
          sku
          barcode
          price
          compareAtPrice
          inventoryQuantity
          selectedOptions {
            name
            value
          }
        }
      }
    }
  }
`;

const locationsQuery = `#graphql
  query ProductVariantScalarValidationLocations {
    shop {
      resourceLimits {
        locationLimit
      }
    }
    locations(first: 25) {
      nodes {
        id
        name
        isActive
        fulfillsOnlineOrders
      }
    }
  }
`;

const bulkCreateMutation = `mutation ProductVariantsBulkCreateValidation($productId: ID!, $variants: [ProductVariantsBulkInput!]!) {
  productVariantsBulkCreate(productId: $productId, variants: $variants) {
    productVariants {
      id
    }
    userErrors {
      field
      message
      code
    }
  }
}
`;

function stableJson(value: unknown): string {
  return JSON.stringify(value);
}

function longText(prefix: string): string {
  return `${prefix}${'x'.repeat(256 - prefix.length)}`;
}

type SetupOptionValue = {
  id?: unknown;
  name?: unknown;
};

type SetupOption = {
  id?: unknown;
  name?: unknown;
  optionValues?: SetupOptionValue[] | null;
};

type LocationNode = {
  id?: unknown;
  isActive?: unknown;
};

type LocationLimitCapturePayload = {
  data?: {
    shop?: {
      resourceLimits?: {
        locationLimit?: unknown;
      } | null;
    } | null;
  };
};

function requireString(value: unknown, label: string): string {
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`Missing ${label}`);
  }
  return value;
}

function requireOption(options: SetupOption[], name: string): SetupOption & { id: string } {
  const option = options.find((candidate) => candidate.name === name);
  if (!option) {
    throw new Error(`Could not find setup option ${name}`);
  }
  return {
    ...option,
    id: requireString(option.id, `${name} option id`),
  };
}

function requireOptionValue(option: SetupOption, name: string): SetupOptionValue & { id: string } {
  const values = option.optionValues ?? [];
  const value = values.find((candidate) => candidate.name === name);
  if (!value) {
    throw new Error(`Could not find setup option value ${String(option.name)} / ${name}`);
  }
  return {
    ...value,
    id: requireString(value.id, `${String(option.name)} / ${name} option value id`),
  };
}

function locationIdsFromCapture(entry: CaptureEntry): string[] {
  const payload = entry.result.payload as { data?: { locations?: { nodes?: LocationNode[] | null } } };
  const nodes = payload.data?.locations?.nodes ?? [];
  return nodes
    .filter((location) => location.isActive !== false)
    .map((location) => location.id)
    .filter((id): id is string => typeof id === 'string' && id.length > 0);
}

function locationLimitFromCapture(entry: CaptureEntry): number {
  const payload = entry.result.payload as LocationLimitCapturePayload;
  const limit = payload.data?.shop?.resourceLimits?.locationLimit;
  if (typeof limit !== 'number' || !Number.isInteger(limit) || limit < 1) {
    throw new Error(`Missing shop.resourceLimits.locationLimit: ${JSON.stringify(entry.result.payload, null, 2)}`);
  }
  return limit;
}

async function capture(query: string, variables: Record<string, unknown>): Promise<CaptureEntry> {
  return {
    query,
    variables,
    result: await runGraphqlRaw(query, variables),
  };
}

async function productState(productId: string): Promise<unknown> {
  const result = await runGraphql<{ product?: unknown }>(productStateQuery, { id: productId });
  return result.data?.product ?? null;
}

const runId = `variant-validation-${Date.now()}`;
const createVariables = {
  product: {
    title: `${runId} Product Variant Scalar Validation`,
    status: 'DRAFT',
  },
};

await mkdir(outputDir, { recursive: true });

const productCreate = await capture(productCreateMutation, createVariables);
const productCreatePayload = productCreate.result.payload as {
  data?: { productCreate?: { product?: { id?: unknown } } };
};
const productId = productCreatePayload.data?.productCreate?.product?.id;
if (typeof productId !== 'string') {
  throw new Error(`Could not create disposable product: ${JSON.stringify(productCreate.result.payload, null, 2)}`);
}

try {
  const setupOptions = await capture(productOptionsCreateMutation, {
    productId,
    options: [
      { name: 'Color', values: [{ name: 'Red' }, { name: 'Blue' }] },
      { name: 'Size', values: [{ name: 'Small' }, { name: 'Large' }] },
    ],
  });
  const setupPayload = setupOptions.result.payload as {
    data?: { productOptionsCreate?: { product?: { options?: SetupOption[] | null }; userErrors?: unknown[] } };
  };
  const setupErrors = setupPayload.data?.productOptionsCreate?.userErrors ?? [];
  if (Array.isArray(setupErrors) && setupErrors.length > 0) {
    throw new Error(`Option setup returned userErrors: ${JSON.stringify(setupErrors, null, 2)}`);
  }
  const setupProductOptions = setupPayload.data?.productOptionsCreate?.product?.options ?? [];
  const colorOption = requireOption(setupProductOptions, 'Color');
  requireOption(setupProductOptions, 'Size');
  const blueValue = requireOptionValue(colorOption, 'Blue');
  const locations = await capture(locationsQuery, {});
  const locationIds = locationIdsFromCapture(locations);
  if (locationIds.length < 1) {
    throw new Error('Need at least one active location to capture inventory location validation.');
  }
  const locationLimit = locationLimitFromCapture(locations);
  const primaryLocationId = locationIds[0] ?? '';
  const tooManyInventoryLocationQuantities = Array.from({ length: locationLimit + 1 }, () => ({
    availableQuantity: 1,
    locationId: primaryLocationId,
  }));
  const inventoryQuantitiesLimitVariants = Array.from({ length: 2048 }, () => ({
    inventoryQuantities: Array.from({ length: 25 }, () => ({
      availableQuantity: 1,
      locationId: primaryLocationId,
    })),
  }));

  const validOptions = [
    { optionName: 'Color', name: 'Blue' },
    { optionName: 'Size', name: 'Large' },
  ];
  const cases: Record<string, CaptureEntry & { atomicNoWrite: boolean }> = {};
  const caseInputs: Array<[string, Array<Record<string, unknown>>]> = [
    ['priceNull', [{ price: null, optionValues: validOptions }]],
    ['priceNegative', [{ price: '-5', optionValues: validOptions }]],
    ['priceTooLarge', [{ price: '1000000000000000000', optionValues: validOptions }]],
    ['compareAtPriceTooLarge', [{ price: '10', compareAtPrice: '1000000000000000000', optionValues: validOptions }]],
    [
      'weightNegative',
      [
        {
          price: '10',
          inventoryItem: { measurement: { weight: { value: -1, unit: 'KILOGRAMS' } } },
          optionValues: validOptions,
        },
      ],
    ],
    [
      'weightTooLarge',
      [
        {
          price: '10',
          inventoryItem: { measurement: { weight: { value: 2_000_000_000, unit: 'KILOGRAMS' } } },
          optionValues: validOptions,
        },
      ],
    ],
    [
      'inventoryTooHigh',
      [
        {
          price: '10',
          inventoryQuantities: [{ availableQuantity: 2_000_000_000, locationId: 'gid://shopify/Location/1' }],
          optionValues: validOptions,
        },
      ],
    ],
    ['inventoryQuantitiesLimit', inventoryQuantitiesLimitVariants],
    [
      'tooManyInventoryLocations',
      [
        {
          price: '10',
          inventoryQuantities: tooManyInventoryLocationQuantities,
          optionValues: validOptions,
        },
      ],
    ],
    ['skuTooLong', [{ price: '10', inventoryItem: { sku: longText('sku-') }, optionValues: validOptions }]],
    ['barcodeTooLong', [{ price: '10', barcode: longText('barcode-'), optionValues: validOptions }]],
    [
      'optionValueTooLong',
      [
        {
          price: '10',
          optionValues: [
            { optionName: 'Color', name: longText('color-') },
            { optionName: 'Size', name: 'Large' },
          ],
        },
      ],
    ],
    [
      'optionsAndOptionValues',
      [
        {
          options: ['Blue', 'Large'],
          optionValues: validOptions,
        },
      ],
    ],
    [
      'optionIdAndOptionName',
      [
        {
          optionValues: [
            { optionId: colorOption.id, optionName: 'Color', name: 'Blue' },
            { optionName: 'Size', name: 'Large' },
          ],
        },
      ],
    ],
    [
      'optionValueIdAndName',
      [
        {
          optionValues: [
            { optionName: 'Color', id: blueValue.id, name: 'Blue' },
            { optionName: 'Size', name: 'Large' },
          ],
        },
      ],
    ],
    [
      'duplicateOptionTuple',
      [
        {
          optionValues: validOptions,
        },
        {
          optionValues: validOptions,
        },
      ],
    ],
    ['maxInputSize', Array.from({ length: 2049 }, () => ({ price: '1' }))],
  ];

  for (const [name, variants] of caseInputs) {
    const before = await productState(productId);
    const entry = await capture(bulkCreateMutation, { productId, variants });
    const after = await productState(productId);
    cases[name] = {
      ...entry,
      atomicNoWrite: stableJson(before) === stableJson(after),
    };
  }

  const payload = {
    notes: [
      'productVariantsBulkCreate scalar and option validation capture.',
      'Each rejected branch was captured against a disposable product with Color/Size options.',
      "inventoryQuantitiesLimit uses 2048 variants with 25 entries each to exceed Shopify's 50,000 cumulative limit while reusing one valid live location id.",
      'tooManyInventoryLocations uses shop.resourceLimits.locationLimit + 1 inventoryQuantity entries on one active live location to confirm the public per-variant cap.',
      'atomicNoWrite compares before/after product reads and must remain true for each captured rejection.',
    ],
    run: {
      runId,
      productId,
      storeDomain,
      apiVersion,
      activeLocationCount: locationIds.length,
      locationLimit,
    },
    captures: {
      productCreate,
      setupOptions,
      locations,
      ...cases,
    },
    upstreamCalls: [],
  };

  await writeFile(outputPath, `${JSON.stringify(payload, null, 2)}\n`, 'utf8');
  console.log(
    JSON.stringify(
      {
        ok: true,
        outputPath,
        cases: Object.keys(cases).length,
        allAtomic: Object.values(cases).every((entry) => entry.atomicNoWrite),
      },
      null,
      2,
    ),
  );
} finally {
  await runGraphql(productDeleteMutation, { input: { id: productId } }).catch((error: unknown) => {
    console.error(`Cleanup failed for ${productId}: ${String(error)}`);
  });
}

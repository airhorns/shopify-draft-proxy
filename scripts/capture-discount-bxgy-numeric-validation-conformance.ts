/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { assertDiscountConformanceScopes, probeDiscountConformanceScopes } from './discount-conformance-lib.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type RawCase = {
  query: string;
  variables: Record<string, unknown>;
  payload: unknown;
};

type CleanupStep = {
  label: string;
  run: () => Promise<ConformanceGraphqlResult<unknown>>;
};

type UserError = {
  field: string[];
  message: string;
  code: string;
  extraInfo: null;
};

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
const outputPath = path.join(outputDir, 'discount-bxgy-numeric-validation.json');
const codeCreateRequestPath = 'config/parity-requests/discounts/discount-bxgy-numeric-validation-code-create.graphql';
const codeUpdateRequestPath = 'config/parity-requests/discounts/discount-bxgy-numeric-validation-code-update.graphql';
const automaticCreateRequestPath =
  'config/parity-requests/discounts/discount-bxgy-numeric-validation-automatic-create.graphql';
const automaticUpdateRequestPath =
  'config/parity-requests/discounts/discount-bxgy-numeric-validation-automatic-update.graphql';

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const adminOptions = {
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
};
const { runGraphql, runGraphqlRaw } = createAdminGraphqlClient(adminOptions);

const productCreateMutation = `#graphql
  mutation DiscountBxgyNumericProductCreate($product: ProductCreateInput!) {
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
  mutation DiscountBxgyNumericProductDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

const codeDeleteMutation = `#graphql
  mutation DiscountBxgyNumericCodeCleanup($id: ID!) {
    discountCodeDelete(id: $id) {
      deletedCodeDiscountId
      userErrors {
        field
        message
        code
        extraInfo
      }
    }
  }
`;

const automaticDeleteMutation = `#graphql
  mutation DiscountBxgyNumericAutomaticCleanup($id: ID!) {
    discountAutomaticDelete(id: $id) {
      deletedAutomaticDiscountId
      userErrors {
        field
        message
        code
        extraInfo
      }
    }
  }
`;

function assertNoUserErrors(label: string, userErrors: unknown): void {
  if (Array.isArray(userErrors) && userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors)}`);
  }
}

function readProduct(label: string, response: { data?: ProductCreateData }): ProductRecord {
  const create = response.data?.productCreate;
  assertNoUserErrors(label, create?.userErrors);

  const id = create?.product?.id;
  const title = create?.product?.title;
  if (typeof id !== 'string' || typeof title !== 'string') {
    throw new Error(`${label} did not return a product id/title: ${JSON.stringify(response)}`);
  }

  return { id, title };
}

function readRecord(value: unknown, pathParts: string[]): Record<string, unknown> | undefined {
  let current = value;
  for (const part of pathParts) {
    if (typeof current !== 'object' || current === null || Array.isArray(current)) {
      return undefined;
    }

    current = (current as Record<string, unknown>)[part];
  }

  if (typeof current === 'object' && current !== null && !Array.isArray(current)) {
    return current as Record<string, unknown>;
  }

  return undefined;
}

function readString(value: unknown, pathParts: string[]): string | undefined {
  const parent = readRecord(value, pathParts.slice(0, -1));
  const leaf = parent?.[pathParts[pathParts.length - 1] ?? ''];
  return typeof leaf === 'string' ? leaf : undefined;
}

function withPatch(input: Record<string, unknown>, patch: Record<string, unknown>): Record<string, unknown> {
  return {
    ...input,
    ...patch,
  };
}

function withCustomerBuysQuantity(input: Record<string, unknown>, quantity: string): Record<string, unknown> {
  const customerBuys = input['customerBuys'];
  if (typeof customerBuys !== 'object' || customerBuys === null || Array.isArray(customerBuys)) {
    throw new Error(`Input missing customerBuys object: ${JSON.stringify(input)}`);
  }

  return {
    ...input,
    customerBuys: {
      ...customerBuys,
      value: {
        quantity,
      },
    },
  };
}

function withCustomerGetsQuantity(input: Record<string, unknown>, quantity: string): Record<string, unknown> {
  const customerGets = input['customerGets'];
  const customerGetsValue = readRecord(customerGets, ['value']);
  const discountOnQuantity = readRecord(customerGetsValue, ['discountOnQuantity']);
  if (typeof customerGets !== 'object' || customerGets === null || Array.isArray(customerGets)) {
    throw new Error(`Input missing customerGets object: ${JSON.stringify(input)}`);
  }
  if (customerGetsValue === undefined || discountOnQuantity === undefined) {
    throw new Error(`Input missing customerGets.value.discountOnQuantity: ${JSON.stringify(input)}`);
  }

  return {
    ...input,
    customerGets: {
      ...customerGets,
      value: {
        ...customerGetsValue,
        discountOnQuantity: {
          ...discountOnQuantity,
          quantity,
        },
      },
    },
  };
}

function customerBuys(productId: string, quantity: string): Record<string, unknown> {
  return {
    value: {
      quantity,
    },
    items: {
      products: {
        productsToAdd: [productId],
      },
    },
  };
}

function customerGets(productId: string, quantity: string): Record<string, unknown> {
  return {
    value: {
      discountOnQuantity: {
        quantity,
        effect: {
          percentage: 0.5,
        },
      },
    },
    items: {
      products: {
        productsToAdd: [productId],
      },
    },
  };
}

function codeInput(stamp: number, suffix: string, buyProductId: string, getProductId: string): Record<string, unknown> {
  return {
    title: `Conformance BXGY code ${suffix} ${stamp}`,
    code: `BXGYN${suffix}${stamp}`,
    startsAt: '2026-04-25T00:00:00Z',
    combinesWith: {
      productDiscounts: true,
      orderDiscounts: false,
      shippingDiscounts: false,
    },
    context: {
      all: 'ALL',
    },
    customerBuys: customerBuys(buyProductId, '1'),
    customerGets: customerGets(getProductId, '1'),
  };
}

function automaticInput(
  stamp: number,
  suffix: string,
  buyProductId: string,
  getProductId: string,
): Record<string, unknown> {
  return {
    title: `Conformance BXGY automatic ${suffix} ${stamp}`,
    startsAt: '2026-04-25T00:00:00Z',
    combinesWith: {
      productDiscounts: true,
      orderDiscounts: false,
      shippingDiscounts: false,
    },
    context: {
      all: 'ALL',
    },
    customerBuys: customerBuys(buyProductId, '1'),
    customerGets: customerGets(getProductId, '1'),
  };
}

function expectedUserError(field: string[], message: string, code: string): UserError {
  return {
    field,
    message,
    code,
    extraInfo: null,
  };
}

function usesLimitZeroError(inputName: string): UserError {
  return expectedUserError([inputName, 'usesPerOrderLimit'], 'Allocation limit cannot be zero', 'VALUE_OUTSIDE_RANGE');
}

function usesLimitNegativeError(inputName: string): UserError {
  return expectedUserError([inputName, 'usesPerOrderLimit'], 'Allocation limit must be greater than 0', 'GREATER_THAN');
}

function usesLimitTooLargeError(inputName: string): UserError {
  return expectedUserError(
    [inputName, 'usesPerOrderLimit'],
    'Allocation limit must be less than or equal to 2147483647',
    'LESS_THAN_OR_EQUAL_TO',
  );
}

function quantityZeroError(inputName: string, tail: string[], role: string): UserError {
  return expectedUserError(
    [inputName, ...tail],
    `Prerequisite to entitlement quantity ratio ${role} must be greater than 0`,
    'GREATER_THAN',
  );
}

function quantityTooLargeError(inputName: string, tail: string[], role: string): UserError {
  return expectedUserError(
    [inputName, ...tail],
    `Prerequisite to entitlement quantity ratio ${role} must be less than 100000`,
    'LESS_THAN',
  );
}

function assertUserErrors(
  label: string,
  payload: unknown,
  root: string,
  nodeField: string,
  expected: UserError[],
): void {
  const rootPayload = readRecord(payload, ['data', root]);
  if (rootPayload === undefined) {
    throw new Error(`${label} missing data.${root}: ${JSON.stringify(payload)}`);
  }
  if (rootPayload[nodeField] !== null) {
    throw new Error(`${label} unexpectedly returned ${nodeField}: ${JSON.stringify(rootPayload[nodeField])}`);
  }
  if (JSON.stringify(rootPayload['userErrors']) !== JSON.stringify(expected)) {
    throw new Error(`${label} unexpected userErrors: ${JSON.stringify(rootPayload['userErrors'])}`);
  }
}

function assertAccepted(label: string, payload: unknown, root: string, nodeField: string): string {
  const rootPayload = readRecord(payload, ['data', root]);
  if (rootPayload === undefined) {
    throw new Error(`${label} missing data.${root}: ${JSON.stringify(payload)}`);
  }
  const userErrors = rootPayload['userErrors'];
  if (!Array.isArray(userErrors) || userErrors.length !== 0) {
    throw new Error(`${label} returned unexpected userErrors: ${JSON.stringify(userErrors)}`);
  }
  const id = readString(rootPayload[nodeField], ['id']);
  if (id === undefined) {
    throw new Error(`${label} did not return ${nodeField}.id: ${JSON.stringify(rootPayload)}`);
  }

  return id;
}

function assertTopLevelInvalidVariable(label: string, payload: unknown, expectedIncludes: string[]): void {
  const errors = typeof payload === 'object' && payload !== null ? (payload as { errors?: unknown }).errors : undefined;
  const firstError = Array.isArray(errors) ? errors[0] : undefined;
  const errorRecord =
    typeof firstError === 'object' && firstError !== null && !Array.isArray(firstError)
      ? (firstError as Record<string, unknown>)
      : undefined;
  const message = typeof errorRecord?.['message'] === 'string' ? errorRecord['message'] : undefined;
  const code = readString(errorRecord, ['extensions', 'code']);
  if (message === undefined || code !== 'INVALID_VARIABLE') {
    throw new Error(`${label} did not return INVALID_VARIABLE: ${JSON.stringify(payload)}`);
  }
  for (const expected of expectedIncludes) {
    if (!message.includes(expected)) {
      throw new Error(`${label} missing message fragment ${expected}: ${message}`);
    }
  }
}

async function runCase(query: string, variables: Record<string, unknown>): Promise<RawCase> {
  const response = await runGraphqlRaw(query, variables);
  return {
    query,
    variables,
    payload: response.payload,
  };
}

await mkdir(outputDir, { recursive: true });

const scopeProbe = await probeDiscountConformanceScopes(adminOptions);
assertDiscountConformanceScopes(scopeProbe);

const codeCreateQuery = await readFile(codeCreateRequestPath, 'utf8');
const codeUpdateQuery = await readFile(codeUpdateRequestPath, 'utf8');
const automaticCreateQuery = await readFile(automaticCreateRequestPath, 'utf8');
const automaticUpdateQuery = await readFile(automaticUpdateRequestPath, 'utf8');

const stamp = Date.now();
const cleanup: CleanupStep[] = [];
const cleanupResponses: Array<{ label: string; payload?: unknown; error?: string }> = [];
const setupProducts: ProductRecord[] = [];
const validation: Record<string, RawCase> = {};
let codeCreate: RawCase | undefined;
let automaticCreate: RawCase | undefined;
let codeCreateSequence = 0;
let automaticCreateSequence = 0;

try {
  const buyProductResponse = await runGraphql<ProductCreateData>(productCreateMutation, {
    product: {
      title: `Conformance BXGY numeric buy product ${stamp}`,
      status: 'ACTIVE',
      vendor: 'HERMES',
      productType: 'CONFORMANCE',
      tags: ['conformance', 'discount-bxgy', 'numeric-validation', String(stamp)],
    },
  });
  const buyProduct = readProduct('buy productCreate', buyProductResponse);
  cleanup.push({
    label: 'buy productDelete',
    run: () => runGraphqlRaw(productDeleteMutation, { input: { id: buyProduct.id } }),
  });
  setupProducts.push(buyProduct);

  const getProductResponse = await runGraphql<ProductCreateData>(productCreateMutation, {
    product: {
      title: `Conformance BXGY numeric get product ${stamp}`,
      status: 'ACTIVE',
      vendor: 'HERMES',
      productType: 'CONFORMANCE',
      tags: ['conformance', 'discount-bxgy', 'numeric-validation', String(stamp)],
    },
  });
  const getProduct = readProduct('get productCreate', getProductResponse);
  cleanup.push({
    label: 'get productDelete',
    run: () => runGraphqlRaw(productDeleteMutation, { input: { id: getProduct.id } }),
  });
  setupProducts.push(getProduct);

  const baseCodeInput = codeInput(stamp, 'SETUP', buyProduct.id, getProduct.id);
  const baseAutomaticInput = automaticInput(stamp, 'SETUP', buyProduct.id, getProduct.id);

  codeCreate = await runCase(codeCreateQuery, {
    input: baseCodeInput,
  });
  const codeId = assertAccepted('code setup create', codeCreate.payload, 'discountCodeBxgyCreate', 'codeDiscountNode');
  cleanup.push({
    label: 'discountCodeDelete setup',
    run: () => runGraphqlRaw(codeDeleteMutation, { id: codeId }),
  });

  automaticCreate = await runCase(automaticCreateQuery, {
    input: baseAutomaticInput,
  });
  const automaticId = assertAccepted(
    'automatic setup create',
    automaticCreate.payload,
    'discountAutomaticBxgyCreate',
    'automaticDiscountNode',
  );
  cleanup.push({
    label: 'discountAutomaticDelete setup',
    run: () => runGraphqlRaw(automaticDeleteMutation, { id: automaticId }),
  });

  async function runCodeCreateUserError(
    key: string,
    input: Record<string, unknown>,
    expected: UserError[],
  ): Promise<void> {
    codeCreateSequence += 1;
    validation[key] = await runCase(codeCreateQuery, {
      input: {
        ...input,
        code: `BXGYNV${stamp}${codeCreateSequence}`,
      },
    });
    assertUserErrors(key, validation[key].payload, 'discountCodeBxgyCreate', 'codeDiscountNode', expected);
  }

  async function runCodeUpdateUserError(
    key: string,
    input: Record<string, unknown>,
    expected: UserError[],
  ): Promise<void> {
    validation[key] = await runCase(codeUpdateQuery, { id: codeId, input });
    assertUserErrors(key, validation[key].payload, 'discountCodeBxgyUpdate', 'codeDiscountNode', expected);
  }

  async function runAutomaticCreateUserError(
    key: string,
    input: Record<string, unknown>,
    expected: UserError[],
  ): Promise<void> {
    automaticCreateSequence += 1;
    validation[key] = await runCase(automaticCreateQuery, {
      input: {
        ...input,
        title: `Conformance BXGY automatic validation ${stamp} ${automaticCreateSequence}`,
      },
    });
    assertUserErrors(key, validation[key].payload, 'discountAutomaticBxgyCreate', 'automaticDiscountNode', expected);
  }

  async function runAutomaticUpdateUserError(
    key: string,
    input: Record<string, unknown>,
    expected: UserError[],
  ): Promise<void> {
    validation[key] = await runCase(automaticUpdateQuery, { id: automaticId, input });
    assertUserErrors(key, validation[key].payload, 'discountAutomaticBxgyUpdate', 'automaticDiscountNode', expected);
  }

  async function runTopLevelError(
    key: string,
    query: string,
    variables: Record<string, unknown>,
    expectedIncludes: string[],
  ): Promise<void> {
    validation[key] = await runCase(query, variables);
    assertTopLevelInvalidVariable(key, validation[key].payload, expectedIncludes);
  }

  async function runAcceptedRatio(
    key: string,
    query: string,
    variables: Record<string, unknown>,
    root: string,
    nodeField: string,
    cleanupLabel: string,
    cleanupMutation: string,
  ): Promise<void> {
    validation[key] = await runCase(query, variables);
    const id = assertAccepted(key, validation[key].payload, root, nodeField);
    cleanup.push({
      label: cleanupLabel,
      run: () => runGraphqlRaw(cleanupMutation, { id }),
    });
  }

  const buyQuantityTail = ['customerBuys', 'value', 'quantity'];
  const getQuantityTail = ['customerGets', 'value', 'discountOnQuantity', 'quantity'];
  const codeInputName = 'bxgyCodeDiscount';
  const automaticInputName = 'automaticBxgyDiscount';

  await runCodeCreateUserError('codeCreateUsesLimitZero', withPatch(baseCodeInput, { usesPerOrderLimit: 0 }), [
    usesLimitZeroError(codeInputName),
  ]);
  await runCodeUpdateUserError('codeUpdateUsesLimitZero', withPatch(baseCodeInput, { usesPerOrderLimit: 0 }), [
    usesLimitZeroError(codeInputName),
  ]);
  await runCodeCreateUserError('codeCreateUsesLimitNegative', withPatch(baseCodeInput, { usesPerOrderLimit: -1 }), [
    usesLimitNegativeError(codeInputName),
  ]);
  await runCodeUpdateUserError('codeUpdateUsesLimitNegative', withPatch(baseCodeInput, { usesPerOrderLimit: -1 }), [
    usesLimitNegativeError(codeInputName),
  ]);
  await runCodeCreateUserError(
    'codeCreateUsesLimitTooLarge',
    withPatch(baseCodeInput, { usesPerOrderLimit: 2_147_483_648 }),
    [usesLimitTooLargeError(codeInputName)],
  );
  await runCodeUpdateUserError(
    'codeUpdateUsesLimitTooLarge',
    withPatch(baseCodeInput, { usesPerOrderLimit: 2_147_483_648 }),
    [usesLimitTooLargeError(codeInputName)],
  );
  await runCodeCreateUserError('codeCreateBuyQuantityZero', withCustomerBuysQuantity(baseCodeInput, '0'), [
    quantityZeroError(codeInputName, buyQuantityTail, 'antecedent'),
  ]);
  await runCodeUpdateUserError('codeUpdateBuyQuantityZero', withCustomerBuysQuantity(baseCodeInput, '0'), [
    quantityZeroError(codeInputName, buyQuantityTail, 'antecedent'),
  ]);
  await runCodeCreateUserError('codeCreateBuyQuantityTooLarge', withCustomerBuysQuantity(baseCodeInput, '2147483648'), [
    quantityTooLargeError(codeInputName, buyQuantityTail, 'antecedent'),
  ]);
  await runCodeUpdateUserError('codeUpdateBuyQuantityTooLarge', withCustomerBuysQuantity(baseCodeInput, '2147483648'), [
    quantityTooLargeError(codeInputName, buyQuantityTail, 'antecedent'),
  ]);
  await runCodeCreateUserError('codeCreateGetQuantityZero', withCustomerGetsQuantity(baseCodeInput, '0'), [
    quantityZeroError(codeInputName, getQuantityTail, 'consequent'),
  ]);
  await runCodeUpdateUserError('codeUpdateGetQuantityZero', withCustomerGetsQuantity(baseCodeInput, '0'), [
    quantityZeroError(codeInputName, getQuantityTail, 'consequent'),
  ]);
  await runCodeCreateUserError('codeCreateGetQuantityTooLarge', withCustomerGetsQuantity(baseCodeInput, '2147483648'), [
    quantityTooLargeError(codeInputName, getQuantityTail, 'consequent'),
  ]);
  await runCodeUpdateUserError('codeUpdateGetQuantityTooLarge', withCustomerGetsQuantity(baseCodeInput, '2147483648'), [
    quantityTooLargeError(codeInputName, getQuantityTail, 'consequent'),
  ]);

  await runAutomaticCreateUserError(
    'automaticCreateUsesLimitZero',
    withPatch(baseAutomaticInput, { usesPerOrderLimit: '0' }),
    [usesLimitZeroError(automaticInputName)],
  );
  await runAutomaticUpdateUserError(
    'automaticUpdateUsesLimitZero',
    withPatch(baseAutomaticInput, { usesPerOrderLimit: '0' }),
    [usesLimitZeroError(automaticInputName)],
  );
  await runAutomaticCreateUserError(
    'automaticCreateUsesLimitTooLarge',
    withPatch(baseAutomaticInput, { usesPerOrderLimit: '2147483648' }),
    [usesLimitTooLargeError(automaticInputName)],
  );
  await runAutomaticUpdateUserError(
    'automaticUpdateUsesLimitTooLarge',
    withPatch(baseAutomaticInput, { usesPerOrderLimit: '2147483648' }),
    [usesLimitTooLargeError(automaticInputName)],
  );
  await runAutomaticCreateUserError(
    'automaticCreateBuyQuantityZero',
    withCustomerBuysQuantity(baseAutomaticInput, '0'),
    [quantityZeroError(automaticInputName, buyQuantityTail, 'antecedent')],
  );
  await runAutomaticUpdateUserError(
    'automaticUpdateBuyQuantityZero',
    withCustomerBuysQuantity(baseAutomaticInput, '0'),
    [quantityZeroError(automaticInputName, buyQuantityTail, 'antecedent')],
  );
  await runAutomaticCreateUserError(
    'automaticCreateBuyQuantityTooLarge',
    withCustomerBuysQuantity(baseAutomaticInput, '2147483648'),
    [quantityTooLargeError(automaticInputName, buyQuantityTail, 'antecedent')],
  );
  await runAutomaticUpdateUserError(
    'automaticUpdateBuyQuantityTooLarge',
    withCustomerBuysQuantity(baseAutomaticInput, '2147483648'),
    [quantityTooLargeError(automaticInputName, buyQuantityTail, 'antecedent')],
  );
  await runAutomaticCreateUserError(
    'automaticCreateGetQuantityZero',
    withCustomerGetsQuantity(baseAutomaticInput, '0'),
    [quantityZeroError(automaticInputName, getQuantityTail, 'consequent')],
  );
  await runAutomaticUpdateUserError(
    'automaticUpdateGetQuantityZero',
    withCustomerGetsQuantity(baseAutomaticInput, '0'),
    [quantityZeroError(automaticInputName, getQuantityTail, 'consequent')],
  );
  await runAutomaticCreateUserError(
    'automaticCreateGetQuantityTooLarge',
    withCustomerGetsQuantity(baseAutomaticInput, '2147483648'),
    [quantityTooLargeError(automaticInputName, getQuantityTail, 'consequent')],
  );
  await runAutomaticUpdateUserError(
    'automaticUpdateGetQuantityTooLarge',
    withCustomerGetsQuantity(baseAutomaticInput, '2147483648'),
    [quantityTooLargeError(automaticInputName, getQuantityTail, 'consequent')],
  );

  await runTopLevelError(
    'codeCreateUsesLimitFloatString',
    codeCreateQuery,
    { input: withPatch(baseCodeInput, { usesPerOrderLimit: '1.5' }) },
    ['usesPerOrderLimit', 'Could not coerce value "1.5" to Int'],
  );
  await runTopLevelError(
    'codeUpdateUsesLimitFloatString',
    codeUpdateQuery,
    { id: codeId, input: withPatch(baseCodeInput, { usesPerOrderLimit: '1.5' }) },
    ['usesPerOrderLimit', 'Could not coerce value "1.5" to Int'],
  );
  await runTopLevelError(
    'automaticCreateUsesLimitNegative',
    automaticCreateQuery,
    { input: withPatch(baseAutomaticInput, { usesPerOrderLimit: '-1' }) },
    ['usesPerOrderLimit', "UnsignedInt64 '-1' is out of range"],
  );
  await runTopLevelError(
    'automaticUpdateUsesLimitNegative',
    automaticUpdateQuery,
    { id: automaticId, input: withPatch(baseAutomaticInput, { usesPerOrderLimit: '-1' }) },
    ['usesPerOrderLimit', "UnsignedInt64 '-1' is out of range"],
  );
  await runTopLevelError(
    'automaticCreateUsesLimitFloatString',
    automaticCreateQuery,
    { input: withPatch(baseAutomaticInput, { usesPerOrderLimit: '1.5' }) },
    ['usesPerOrderLimit', "UnsignedInt64 invalid value '1.5'"],
  );
  await runTopLevelError(
    'automaticUpdateUsesLimitFloatString',
    automaticUpdateQuery,
    { id: automaticId, input: withPatch(baseAutomaticInput, { usesPerOrderLimit: '1.5' }) },
    ['usesPerOrderLimit', "UnsignedInt64 invalid value '1.5'"],
  );
  await runTopLevelError(
    'codeCreateBuyQuantityFloatString',
    codeCreateQuery,
    { input: withCustomerBuysQuantity(baseCodeInput, '1.5') },
    ['customerBuys.value.quantity', "UnsignedInt64 invalid value '1.5'"],
  );
  await runTopLevelError(
    'codeUpdateBuyQuantityFloatString',
    codeUpdateQuery,
    { id: codeId, input: withCustomerBuysQuantity(baseCodeInput, '1.5') },
    ['customerBuys.value.quantity', "UnsignedInt64 invalid value '1.5'"],
  );
  await runTopLevelError(
    'automaticCreateGetQuantityNegative',
    automaticCreateQuery,
    { input: withCustomerGetsQuantity(baseAutomaticInput, '-3') },
    ['customerGets.value.discountOnQuantity.quantity', "UnsignedInt64 '-3' is out of range"],
  );
  await runTopLevelError(
    'automaticUpdateGetQuantityNegative',
    automaticUpdateQuery,
    { id: automaticId, input: withCustomerGetsQuantity(baseAutomaticInput, '-3') },
    ['customerGets.value.discountOnQuantity.quantity', "UnsignedInt64 '-3' is out of range"],
  );
  await runTopLevelError(
    'automaticCreateGetQuantityFloatString',
    automaticCreateQuery,
    { input: withCustomerGetsQuantity(baseAutomaticInput, '1.5') },
    ['customerGets.value.discountOnQuantity.quantity', "UnsignedInt64 invalid value '1.5'"],
  );
  await runTopLevelError(
    'automaticUpdateGetQuantityFloatString',
    automaticUpdateQuery,
    { id: automaticId, input: withCustomerGetsQuantity(baseAutomaticInput, '1.5') },
    ['customerGets.value.discountOnQuantity.quantity', "UnsignedInt64 invalid value '1.5'"],
  );

  const ratioCodeInput = withCustomerGetsQuantity(
    withCustomerBuysQuantity(codeInput(stamp, 'RATIO', buyProduct.id, getProduct.id), '7'),
    '3',
  );
  const ratioCodeUpdateInput = withCustomerGetsQuantity(
    withCustomerBuysQuantity(codeInput(stamp, 'RATIOUP', buyProduct.id, getProduct.id), '7'),
    '3',
  );
  const ratioAutomaticInput = withCustomerGetsQuantity(
    withCustomerBuysQuantity(automaticInput(stamp, 'RATIO', buyProduct.id, getProduct.id), '7'),
    '3',
  );
  const ratioAutomaticUpdateInput = withCustomerGetsQuantity(
    withCustomerBuysQuantity(automaticInput(stamp, 'RATIOUP', buyProduct.id, getProduct.id), '7'),
    '3',
  );
  await runAcceptedRatio(
    'codeCreateRatioAccepted',
    codeCreateQuery,
    { input: ratioCodeInput },
    'discountCodeBxgyCreate',
    'codeDiscountNode',
    'discountCodeDelete ratio create',
    codeDeleteMutation,
  );
  await runAcceptedRatio(
    'codeUpdateRatioAccepted',
    codeUpdateQuery,
    { id: codeId, input: ratioCodeUpdateInput },
    'discountCodeBxgyUpdate',
    'codeDiscountNode',
    'discountCodeDelete ratio update',
    codeDeleteMutation,
  );
  await runAcceptedRatio(
    'automaticCreateRatioAccepted',
    automaticCreateQuery,
    { input: ratioAutomaticInput },
    'discountAutomaticBxgyCreate',
    'automaticDiscountNode',
    'discountAutomaticDelete ratio create',
    automaticDeleteMutation,
  );
  await runAcceptedRatio(
    'automaticUpdateRatioAccepted',
    automaticUpdateQuery,
    { id: automaticId, input: ratioAutomaticUpdateInput },
    'discountAutomaticBxgyUpdate',
    'automaticDiscountNode',
    'discountAutomaticDelete ratio update',
    automaticDeleteMutation,
  );
} finally {
  for (const cleanupStep of cleanup.reverse()) {
    try {
      const response = await cleanupStep.run();
      cleanupResponses.push({ label: cleanupStep.label, payload: response.payload });
    } catch (error) {
      cleanupResponses.push({
        label: cleanupStep.label,
        error: error instanceof Error ? error.message : String(error),
      });
    }
  }
}

if (codeCreate === undefined || automaticCreate === undefined || Object.keys(validation).length === 0) {
  throw new Error('Capture did not complete setup and validation cases.');
}

const fixture = {
  storeDomain,
  apiVersion,
  accessScopes: scopeProbe,
  setup: {
    products: setupProducts,
    codeCreate,
    automaticCreate,
  },
  validation,
  cleanup: cleanupResponses,
  upstreamCalls: [],
};

await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');

console.log(
  JSON.stringify(
    {
      ok: true,
      apiVersion,
      outputPath,
      validationCases: Object.keys(validation).length,
    },
    null,
    2,
  ),
);

/* oxlint-disable no-console -- CLI capture scripts intentionally write status output. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type OperationCapture = {
  request: {
    query: string;
    variables: Record<string, unknown>;
  };
  response: unknown;
  requestedRole?: string;
  proxyVariables?: Record<string, unknown>;
};

type CapturePayload = {
  scenarioId: string;
  capturedAt: string;
  storeDomain: string;
  apiVersion: string;
  notes: string[];
  operations: Record<string, OperationCapture>;
  cleanup: Array<{ id: string; response: unknown }>;
  upstreamCalls: Array<{
    operationName: string;
    variables: Record<string, unknown>;
    response: {
      status: number;
      body: unknown;
    };
  }>;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({
  adminOrigin,
  apiVersion,
});
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const outputPath = path.join(outputDir, 'combinedListingUpdate-validation.json');
const missingProductId = 'gid://shopify/Product/999999999999999999';

const productCreateMutation = `mutation CombinedListingUpdateValidationProductCreate($product: ProductCreateInput!) {
  productCreate(product: $product) {
    product {
      id
      title
      handle
      combinedListingRole
    }
    userErrors {
      field
      message
    }
  }
}`;

const productDeleteMutation = `mutation CombinedListingUpdateValidationProductDelete($input: ProductDeleteInput!) {
  productDelete(input: $input) {
    deletedProductId
    userErrors {
      field
      message
    }
  }
}`;

const combinedListingUpdateMutation = `mutation CombinedListingUpdateValidation(
  $parentProductId: ID!
  $title: String
  $productsAdded: [ChildProductRelationInput!]
  $productsEdited: [ChildProductRelationInput!]
  $productsRemovedIds: [ID!]
  $optionsAndValues: [OptionAndValueInput!]
) {
  combinedListingUpdate(
    parentProductId: $parentProductId
    title: $title
    productsAdded: $productsAdded
    productsEdited: $productsEdited
    productsRemovedIds: $productsRemovedIds
    optionsAndValues: $optionsAndValues
  ) {
    product {
      id
      title
      combinedListingRole
    }
    userErrors {
      field
      message
      code
    }
  }
}`;

function readObject(value: unknown, label: string): Record<string, unknown> {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    throw new Error(`${label} was not an object`);
  }
  return value as Record<string, unknown>;
}

function readProductId(result: ConformanceGraphqlResult, label: string): string {
  const payload = readObject(result.payload, `${label}.payload`);
  const data = readObject(payload['data'], `${label}.data`);
  const productCreate = readObject(data['productCreate'], `${label}.productCreate`);
  const userErrors = productCreate['userErrors'];
  if (!Array.isArray(userErrors) || userErrors.length !== 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors)}`);
  }
  const product = readObject(productCreate['product'], `${label}.product`);
  const id = product['id'];
  if (typeof id !== 'string') {
    throw new Error(`${label} did not return product.id`);
  }
  return id;
}

function assertNoTopLevelErrors(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertCombinedListingCode(result: ConformanceGraphqlResult, label: string, expectedCode: string): void {
  assertNoTopLevelErrors(result, label);
  const payload = readObject(result.payload, `${label}.payload`);
  const data = readObject(payload['data'], `${label}.data`);
  const combinedListingUpdate = readObject(data['combinedListingUpdate'], `${label}.combinedListingUpdate`);
  const userErrors = combinedListingUpdate['userErrors'];
  if (!Array.isArray(userErrors)) {
    throw new Error(`${label} did not return userErrors`);
  }
  const codes = userErrors
    .map((error) => readObject(error, `${label}.userError`)['code'])
    .filter((code): code is string => typeof code === 'string');
  if (!codes.includes(expectedCode)) {
    throw new Error(`${label} expected ${expectedCode}, got ${JSON.stringify(codes)}`);
  }
}

function assertCombinedListingSuccess(result: ConformanceGraphqlResult, label: string): void {
  assertNoTopLevelErrors(result, label);
  const payload = readObject(result.payload, `${label}.payload`);
  const data = readObject(payload['data'], `${label}.data`);
  const combinedListingUpdate = readObject(data['combinedListingUpdate'], `${label}.combinedListingUpdate`);
  const userErrors = combinedListingUpdate['userErrors'];
  if (!Array.isArray(userErrors) || userErrors.length !== 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors)}`);
  }
}

async function record(
  operations: Record<string, OperationCapture>,
  name: string,
  query: string,
  variables: Record<string, unknown>,
): Promise<ConformanceGraphqlResult> {
  const result = await runGraphqlRequest(query, variables);
  operations[name] = {
    request: {
      query,
      variables,
    },
    response: result.payload,
  };
  return result;
}

async function createProduct(
  operations: Record<string, OperationCapture>,
  ids: string[],
  name: string,
  product: Record<string, unknown>,
): Promise<string> {
  const result = await record(operations, name, productCreateMutation, { product });
  assertNoTopLevelErrors(result, name);
  const id = readProductId(result, name);
  ids.push(id);
  const operation = operations[name];
  const requestedRole = product['combinedListingRole'];
  if (operation && typeof requestedRole === 'string') {
    operation.requestedRole = requestedRole;
  }
  return id;
}

function selectedTitle() {
  return [{ name: 'Title', value: 'Default Title' }];
}

function optionValues() {
  return [{ name: 'Title', values: ['Default Title'] }];
}

const operations: Record<string, OperationCapture> = {};
const cleanup: Array<{ id: string; response: unknown }> = [];
const createdProductIds: string[] = [];
const suffix = `har-852-${Date.now()}`;

try {
  const parentValidationId = await createProduct(operations, createdProductIds, 'createParentValidation', {
    title: `${suffix} parent validation`,
    handle: `${suffix}-parent-validation`,
    combinedListingRole: 'PARENT',
  });
  const plainParentId = await createProduct(operations, createdProductIds, 'createPlainParent', {
    title: `${suffix} plain parent`,
    handle: `${suffix}-plain-parent`,
  });
  const childValidationId = await createProduct(operations, createdProductIds, 'createChildValidation', {
    title: `${suffix} child validation`,
    handle: `${suffix}-child-validation`,
  });
  const parentAlreadyId = await createProduct(operations, createdProductIds, 'createParentAlready', {
    title: `${suffix} parent already`,
    handle: `${suffix}-parent-already`,
    combinedListingRole: 'PARENT',
  });
  const childAlreadyId = await createProduct(operations, createdProductIds, 'createChildAlready', {
    title: `${suffix} child already`,
    handle: `${suffix}-child-already`,
  });
  const parentEditRemoveId = await createProduct(operations, createdProductIds, 'createParentEditRemove', {
    title: `${suffix} parent edit remove`,
    handle: `${suffix}-parent-edit-remove`,
    combinedListingRole: 'PARENT',
  });
  const childEditRemoveId = await createProduct(operations, createdProductIds, 'createChildEditRemove', {
    title: `${suffix} child edit remove`,
    handle: `${suffix}-child-edit-remove`,
  });

  const nonParent = await record(operations, 'nonParent', combinedListingUpdateMutation, {
    parentProductId: plainParentId,
  });
  assertCombinedListingCode(nonParent, 'nonParent', 'PARENT_PRODUCT_MUST_BE_A_COMBINED_LISTING');

  const missingOptionsAndValues = await record(operations, 'missingOptionsAndValues', combinedListingUpdateMutation, {
    parentProductId: parentValidationId,
    productsAdded: [
      {
        childProductId: childValidationId,
        selectedParentOptionValues: selectedTitle(),
      },
    ],
  });
  assertCombinedListingCode(missingOptionsAndValues, 'missingOptionsAndValues', 'MISSING_OPTION_VALUES');

  const parentAsChild = await record(operations, 'parentAsChild', combinedListingUpdateMutation, {
    parentProductId: parentValidationId,
    productsAdded: [
      {
        childProductId: parentValidationId,
        selectedParentOptionValues: selectedTitle(),
      },
    ],
    optionsAndValues: optionValues(),
  });
  assertCombinedListingCode(parentAsChild, 'parentAsChild', 'CANNOT_HAVE_PARENT_AS_CHILD');

  const duplicateProductsAdded = await record(operations, 'duplicateProductsAdded', combinedListingUpdateMutation, {
    parentProductId: parentValidationId,
    productsAdded: [
      {
        childProductId: childValidationId,
        selectedParentOptionValues: selectedTitle(),
      },
      {
        childProductId: childValidationId,
        selectedParentOptionValues: selectedTitle(),
      },
    ],
    optionsAndValues: optionValues(),
  });
  assertCombinedListingCode(duplicateProductsAdded, 'duplicateProductsAdded', 'CANNOT_HAVE_DUPLICATED_PRODUCTS');

  const missingChildVariables = {
    parentProductId: parentValidationId,
    productsAdded: [
      {
        childProductId: missingProductId,
        selectedParentOptionValues: selectedTitle(),
      },
    ],
    optionsAndValues: optionValues(),
  };
  const missingChildProduct = await record(
    operations,
    'missingChildProduct',
    combinedListingUpdateMutation,
    missingChildVariables,
  );
  const missingChildOperation = operations['missingChildProduct'];
  if (!missingChildOperation) {
    throw new Error('missingChildProduct operation was not recorded');
  }
  missingChildOperation.proxyVariables = {
    ...missingChildVariables,
    parentProductId: parentValidationId,
  };
  assertCombinedListingCode(missingChildProduct, 'missingChildProduct', 'PRODUCT_NOT_FOUND');

  const emptySelectedParentOptionValues = await record(
    operations,
    'emptySelectedParentOptionValues',
    combinedListingUpdateMutation,
    {
      parentProductId: parentValidationId,
      productsAdded: [
        {
          childProductId: childValidationId,
          selectedParentOptionValues: [],
        },
      ],
      optionsAndValues: optionValues(),
    },
  );
  assertCombinedListingCode(
    emptySelectedParentOptionValues,
    'emptySelectedParentOptionValues',
    'MUST_HAVE_SELECTED_OPTION_VALUES',
  );

  const titleTooLong = await record(operations, 'titleTooLong', combinedListingUpdateMutation, {
    parentProductId: parentValidationId,
    title: 'T'.repeat(256),
  });
  assertCombinedListingCode(titleTooLong, 'titleTooLong', 'TITLE_TOO_LONG');

  const addAlreadyChildSetup = await record(operations, 'addAlreadyChildSetup', combinedListingUpdateMutation, {
    parentProductId: parentAlreadyId,
    productsAdded: [
      {
        childProductId: childAlreadyId,
        selectedParentOptionValues: selectedTitle(),
      },
    ],
    optionsAndValues: optionValues(),
  });
  assertCombinedListingSuccess(addAlreadyChildSetup, 'addAlreadyChildSetup');

  const productIsAlreadyAChild = await record(operations, 'productIsAlreadyAChild', combinedListingUpdateMutation, {
    parentProductId: parentAlreadyId,
    productsAdded: [
      {
        childProductId: childAlreadyId,
        selectedParentOptionValues: selectedTitle(),
      },
    ],
    optionsAndValues: optionValues(),
  });
  assertCombinedListingCode(productIsAlreadyAChild, 'productIsAlreadyAChild', 'PRODUCT_IS_ALREADY_A_CHILD');

  const addEditRemoveSetup = await record(operations, 'addEditRemoveSetup', combinedListingUpdateMutation, {
    parentProductId: parentEditRemoveId,
    productsAdded: [
      {
        childProductId: childEditRemoveId,
        selectedParentOptionValues: selectedTitle(),
      },
    ],
    optionsAndValues: optionValues(),
  });
  assertCombinedListingSuccess(addEditRemoveSetup, 'addEditRemoveSetup');

  const editAndRemoveOnSameProducts = await record(
    operations,
    'editAndRemoveOnSameProducts',
    combinedListingUpdateMutation,
    {
      parentProductId: parentEditRemoveId,
      productsEdited: [
        {
          childProductId: childEditRemoveId,
          selectedParentOptionValues: selectedTitle(),
        },
      ],
      productsRemovedIds: [childEditRemoveId],
      optionsAndValues: optionValues(),
    },
  );
  assertCombinedListingCode(
    editAndRemoveOnSameProducts,
    'editAndRemoveOnSameProducts',
    'EDIT_AND_REMOVE_ON_SAME_PRODUCTS',
  );
} finally {
  for (const id of [...createdProductIds].reverse()) {
    const response = await runGraphqlRequest(productDeleteMutation, { input: { id } });
    cleanup.push({ id, response: response.payload });
  }
}

const capture: CapturePayload = {
  scenarioId: 'combinedListingUpdate-validation',
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  notes: [
    'Creates disposable PARENT, plain, and child products, captures combinedListingUpdate validation payloads, then deletes all setup products.',
    'The fixed missing child product id is intentionally absent; parity replay serves its Product hydration as null through the upstreamCalls cassette.',
  ],
  operations,
  cleanup,
  upstreamCalls: [
    {
      operationName: 'ProductsHydrateNodes',
      variables: { ids: [missingProductId] },
      response: {
        status: 200,
        body: {
          data: {
            nodes: [null],
          },
        },
      },
    },
  ],
};

await mkdir(outputDir, { recursive: true });
await writeFile(outputPath, `${JSON.stringify(capture, null, 2)}\n`, 'utf8');
console.log(JSON.stringify({ outputPath, operations: Object.keys(operations), cleanup: cleanup.length }, null, 2));

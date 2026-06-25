/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { assertDiscountConformanceScopes, probeDiscountConformanceScopes } from './discount-conformance-lib.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'discounts');
const outputPath = path.join(outputDir, 'discount-amount-applies-on-each-item.json');
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const adminOptions = {
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
};
const { runGraphqlRaw } = createAdminGraphqlClient(adminOptions);

const codeCreateDocument = await readFile(
  'config/parity-requests/discounts/discount-amount-applies-on-each-item-code-create.graphql',
  'utf8',
);
const productSetupDocument = await readFile(
  'config/parity-requests/discounts/discount-amount-applies-on-each-item-product-setup.graphql',
  'utf8',
);
const codeReadDocument = await readFile(
  'config/parity-requests/discounts/discount-amount-applies-on-each-item-read.graphql',
  'utf8',
);
const automaticCreateDocument = await readFile(
  'config/parity-requests/discounts/discount-amount-applies-on-each-item-automatic-create.graphql',
  'utf8',
);
const automaticUpdateDocument = await readFile(
  'config/parity-requests/discounts/discount-amount-applies-on-each-item-automatic-update.graphql',
  'utf8',
);
const codeDeleteDocument = `#graphql
  mutation DiscountAmountAppliesOnEachItemCodeCleanup($id: ID!) {
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
const automaticDeleteDocument = `#graphql
  mutation DiscountAmountAppliesOnEachItemAutomaticCleanup($id: ID!) {
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
const productDeleteDocument = `#graphql
  mutation DiscountAmountAppliesOnEachItemProductCleanup($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

type RawResponse = Awaited<ReturnType<typeof runGraphqlRaw>>;
type CleanupResult = {
  query: string;
  variables: Record<string, unknown>;
  status: number;
  response: unknown;
};

function discountItems(productId: string | null): Record<string, unknown> {
  if (productId) {
    return {
      products: {
        productsToAdd: [productId],
      },
    };
  }
  return {
    all: true,
  };
}

function codeInput(code: string, appliesOnEachItem: boolean, productId: string | null): Record<string, unknown> {
  return {
    title: `Discount amount ${code}`,
    code,
    startsAt: '2026-04-25T00:00:00Z',
    context: {
      all: 'ALL',
    },
    customerGets: {
      value: {
        discountAmount: {
          amount: '10.00',
          appliesOnEachItem,
        },
      },
      items: discountItems(productId),
    },
  };
}

function automaticInput(title: string, appliesOnEachItem: boolean, productId: string | null): Record<string, unknown> {
  return {
    title,
    startsAt: '2026-04-25T00:00:00Z',
    context: {
      all: 'ALL',
    },
    customerGets: {
      value: {
        discountAmount: {
          amount: '10.00',
          appliesOnEachItem,
        },
      },
      items: discountItems(productId),
    },
  };
}

function deprecatedFieldInput(code: string, field: 'each' | 'useEach'): Record<string, unknown> {
  const input = codeInput(code, true, null);
  const discountAmount = (
    input['customerGets'] as {
      value: { discountAmount: Record<string, unknown> };
    }
  ).value.discountAmount;
  discountAmount[field] = true;
  return input;
}

function productId(response: RawResponse): string {
  const id = (response.payload as { data?: { productCreate?: { product?: { id?: unknown } | null } } }).data
    ?.productCreate?.product?.id;
  if (typeof id !== 'string' || id.length === 0) {
    throw new Error(`productCreate did not return a product id: ${JSON.stringify(response.payload)}`);
  }
  return id;
}

function discountCodeId(response: RawResponse, caseName: string): string {
  const id = (response.payload as { data?: { discountCodeBasicCreate?: { codeDiscountNode?: { id?: unknown } } } }).data
    ?.discountCodeBasicCreate?.codeDiscountNode?.id;
  if (typeof id !== 'string' || id.length === 0) {
    throw new Error(`${caseName} did not return a code discount id: ${JSON.stringify(response.payload)}`);
  }
  return id;
}

function automaticDiscountId(response: RawResponse, caseName: string): string {
  const id = (
    response.payload as {
      data?: { discountAutomaticBasicCreate?: { automaticDiscountNode?: { id?: unknown } } };
    }
  ).data?.discountAutomaticBasicCreate?.automaticDiscountNode?.id;
  if (typeof id !== 'string' || id.length === 0) {
    throw new Error(`${caseName} did not return an automatic discount id: ${JSON.stringify(response.payload)}`);
  }
  return id;
}

function fixtureCase(
  query: string,
  variables: Record<string, unknown>,
  response: RawResponse,
): { query: string; variables: Record<string, unknown>; status: number; response: unknown } {
  return {
    query,
    variables,
    status: response.status,
    response: response.payload,
  };
}

await mkdir(outputDir, { recursive: true });

const scopeProbe = await probeDiscountConformanceScopes(adminOptions);
assertDiscountConformanceScopes(scopeProbe);

const stamp = Date.now();
const cleanup: Array<() => Promise<CleanupResult>> = [];
const cleanupResults: CleanupResult[] = [];

const productVariables = {
  product: {
    title: `Discount amount entitlement ${stamp}`,
  },
};
const productSetup = await runGraphqlRaw(productSetupDocument, productVariables);
const entitlementProductId = productId(productSetup);
cleanup.push(async () => {
  const response = await runGraphqlRaw(productDeleteDocument, { input: { id: entitlementProductId } });
  return fixtureCase(productDeleteDocument, { input: { id: entitlementProductId } }, response);
});

const codeTrueVariables = { input: codeInput(`AMTEACH${stamp}`, true, entitlementProductId) };
const codeTrueCreate = await runGraphqlRaw(codeCreateDocument, codeTrueVariables);
const codeTrueId = discountCodeId(codeTrueCreate, 'code true create');
cleanup.push(async () => {
  const response = await runGraphqlRaw(codeDeleteDocument, { id: codeTrueId });
  return fixtureCase(codeDeleteDocument, { id: codeTrueId }, response);
});
const codeTrueReadVariables = { id: codeTrueId };
const codeTrueRead = await runGraphqlRaw(codeReadDocument, codeTrueReadVariables);

const codeFalseVariables = { input: codeInput(`AMTACROSS${stamp}`, false, null) };
const codeFalseCreate = await runGraphqlRaw(codeCreateDocument, codeFalseVariables);
const codeFalseId = discountCodeId(codeFalseCreate, 'code false create');
cleanup.push(async () => {
  const response = await runGraphqlRaw(codeDeleteDocument, { id: codeFalseId });
  return fixtureCase(codeDeleteDocument, { id: codeFalseId }, response);
});

const automaticCreateVariables = {
  input: automaticInput(`Discount amount automatic each ${stamp}`, true, entitlementProductId),
};
const automaticCreate = await runGraphqlRaw(automaticCreateDocument, automaticCreateVariables);
const automaticId = automaticDiscountId(automaticCreate, 'automatic create');
cleanup.push(async () => {
  const response = await runGraphqlRaw(automaticDeleteDocument, { id: automaticId });
  return fixtureCase(automaticDeleteDocument, { id: automaticId }, response);
});
const automaticUpdateVariables = {
  id: automaticId,
  input: automaticInput(`Discount amount automatic across ${stamp}`, false, null),
};
const automaticUpdate = await runGraphqlRaw(automaticUpdateDocument, automaticUpdateVariables);

const deprecatedEachVariables = { input: deprecatedFieldInput(`AMTEACHOLD${stamp}`, 'each') };
const deprecatedEach = await runGraphqlRaw(codeCreateDocument, deprecatedEachVariables);
const deprecatedUseEachVariables = { input: deprecatedFieldInput(`AMTUSEEACH${stamp}`, 'useEach') };
const deprecatedUseEach = await runGraphqlRaw(codeCreateDocument, deprecatedUseEachVariables);

for (const runCleanup of cleanup.reverse()) {
  cleanupResults.push(await runCleanup());
}

const fixture = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  accessScopes: scopeProbe,
  setup: {
    product: fixtureCase(productSetupDocument, productVariables, productSetup),
  },
  cases: {
    codeTrueCreate: fixtureCase(codeCreateDocument, codeTrueVariables, codeTrueCreate),
    codeTrueRead: fixtureCase(codeReadDocument, codeTrueReadVariables, codeTrueRead),
    codeFalseCreate: fixtureCase(codeCreateDocument, codeFalseVariables, codeFalseCreate),
    automaticCreate: fixtureCase(automaticCreateDocument, automaticCreateVariables, automaticCreate),
    automaticUpdate: fixtureCase(automaticUpdateDocument, automaticUpdateVariables, automaticUpdate),
    deprecatedEach: fixtureCase(codeCreateDocument, deprecatedEachVariables, deprecatedEach),
    deprecatedUseEach: fixtureCase(codeCreateDocument, deprecatedUseEachVariables, deprecatedUseEach),
  },
  cleanup: cleanupResults,
  upstreamCalls: [],
};

await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');

console.log(
  JSON.stringify(
    {
      ok: true,
      apiVersion,
      output: outputPath,
      statuses: {
        codeTrueCreate: codeTrueCreate.status,
        codeTrueRead: codeTrueRead.status,
        codeFalseCreate: codeFalseCreate.status,
        automaticCreate: automaticCreate.status,
        automaticUpdate: automaticUpdate.status,
        deprecatedEach: deprecatedEach.status,
        deprecatedUseEach: deprecatedUseEach.status,
      },
      cleanupCount: cleanupResults.length,
    },
    null,
    2,
  ),
);

/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { assertDiscountConformanceScopes, probeDiscountConformanceScopes } from './discount-conformance-lib.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'discounts');
const outputPath = path.join(outputDir, 'discount-automatic-basic-lifecycle.json');
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const adminOptions = {
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
};
const { runGraphqlRequest } = createAdminGraphqlClient(adminOptions);

await mkdir(outputDir, { recursive: true });

const scopeProbe = await probeDiscountConformanceScopes(adminOptions);
assertDiscountConformanceScopes(scopeProbe);

const createDocumentPath = 'config/parity-requests/discounts/discount-automatic-basic-lifecycle-create.graphql';
const updateDocumentPath = 'config/parity-requests/discounts/discount-automatic-basic-lifecycle-update.graphql';
const deactivateDocumentPath = 'config/parity-requests/discounts/discount-automatic-basic-lifecycle-deactivate.graphql';
const activateDocumentPath = 'config/parity-requests/discounts/discount-automatic-basic-lifecycle-activate.graphql';
const deleteDocumentPath = 'config/parity-requests/discounts/discount-automatic-basic-lifecycle-delete.graphql';
const shopCurrencyDocumentPath = 'config/parity-requests/discounts/discount-shop-currency-hydrate.graphql';

const [createDocument, updateDocument, deactivateDocument, activateDocument, deleteDocument, shopCurrencyDocument] =
  await Promise.all([
    readFile(createDocumentPath, 'utf8'),
    readFile(updateDocumentPath, 'utf8'),
    readFile(deactivateDocumentPath, 'utf8'),
    readFile(activateDocumentPath, 'utf8'),
    readFile(deleteDocumentPath, 'utf8'),
    readFile(shopCurrencyDocumentPath, 'utf8'),
  ]);

type GraphqlRecord = {
  query: string;
  variables: Record<string, unknown>;
  response: ConformanceGraphqlResult['payload'];
  status: number;
};

function record(query: string, variables: Record<string, unknown>, result: ConformanceGraphqlResult): GraphqlRecord {
  return {
    query,
    variables,
    response: result.payload,
    status: result.status,
  };
}

function readAutomaticDiscountId(result: ConformanceGraphqlResult): string {
  const id = (
    result.payload as {
      data?: { discountAutomaticBasicCreate?: { automaticDiscountNode?: { id?: unknown } } };
    }
  ).data?.discountAutomaticBasicCreate?.automaticDiscountNode?.id;

  if (typeof id !== 'string' || id.length === 0) {
    throw new Error(
      `discountAutomaticBasicCreate did not return an automaticDiscountNode id: ${JSON.stringify(result)}`,
    );
  }

  return id;
}

const runId = Date.now();
const startsAt = '2026-04-25T00:00:00Z';
const shopCurrencyResult = await runGraphqlRequest(shopCurrencyDocument, {});

const createVariables = {
  input: {
    title: `Conformance automatic lifecycle ${runId}`,
    startsAt,
    endsAt: null,
    combinesWith: {
      productDiscounts: false,
      orderDiscounts: true,
      shippingDiscounts: false,
    },
    context: {
      all: 'ALL',
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
};

const createResult = await runGraphqlRequest(createDocument, createVariables);
const discountId = readAutomaticDiscountId(createResult);

const updateVariables = {
  id: discountId,
  input: {
    title: `Conformance automatic lifecycle updated ${runId}`,
    startsAt,
    endsAt: null,
    combinesWith: {
      productDiscounts: true,
      orderDiscounts: false,
      shippingDiscounts: false,
    },
    context: {
      all: 'ALL',
    },
    minimumRequirement: {
      subtotal: {
        greaterThanOrEqualToSubtotal: '10.00',
      },
    },
    customerGets: {
      value: {
        discountAmount: {
          amount: '5.00',
          appliesOnEachItem: false,
        },
      },
      items: {
        all: true,
      },
    },
  },
};

const updateResult = await runGraphqlRequest(updateDocument, updateVariables);
const idVariables = { id: discountId };
const deactivateResult = await runGraphqlRequest(deactivateDocument, idVariables);
const activateResult = await runGraphqlRequest(activateDocument, idVariables);
const deleteResult = await runGraphqlRequest(deleteDocument, idVariables);

const output = {
  storeDomain,
  apiVersion,
  shopCurrency: record(shopCurrencyDocument, {}, shopCurrencyResult),
  create: record(createDocument, createVariables, createResult),
  update: record(updateDocument, updateVariables, updateResult),
  deactivate: record(deactivateDocument, idVariables, deactivateResult),
  activate: record(activateDocument, idVariables, activateResult),
  delete: record(deleteDocument, idVariables, deleteResult),
  upstreamCalls: [],
};

await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');

console.log(
  JSON.stringify(
    {
      ok: true,
      apiVersion,
      outputPath,
      discountId,
    },
    null,
    2,
  ),
);

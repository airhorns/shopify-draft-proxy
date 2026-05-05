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
const outputPath = path.join(outputDir, 'discount-customer-gets-value-multiple-types.json');
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const adminOptions = {
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
};
const { runGraphqlRaw } = createAdminGraphqlClient(adminOptions);

const createDocument = await readFile(
  'config/parity-requests/discounts/discount-customer-gets-value-multiple-types-create.graphql',
  'utf8',
);
const updateDocument = await readFile(
  'config/parity-requests/discounts/discount-customer-gets-value-multiple-types-update.graphql',
  'utf8',
);
const cleanupDocument = `#graphql
  mutation DiscountCustomerGetsValueMultipleTypesCleanup($id: ID!) {
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

function basicInput(code: string): Record<string, unknown> {
  return {
    title: `HAR-782 ${code}`,
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
    customerGets: {
      value: {
        percentage: 0.1,
        discountAmount: {
          amount: '5.00',
          appliesOnEachItem: false,
        },
      },
      items: {
        all: true,
      },
    },
  };
}

const stamp = Date.now();
const createVariables = {
  input: basicInput(`HAR782CREATE${stamp}`),
};
const setupVariables = {
  input: {
    ...basicInput(`HAR782SETUP${stamp}`),
    customerGets: {
      value: {
        percentage: 0.1,
      },
      items: {
        all: true,
      },
    },
  },
};
const updateVariables = {
  id: '',
  input: {
    ...basicInput(`HAR782UPDATE${stamp}`),
    customerGets: {
      value: {
        percentage: 0.2,
        discountOnQuantity: {
          quantity: '1',
          effect: {
            percentage: 0.5,
          },
        },
      },
      items: {
        all: true,
      },
    },
  },
};

await mkdir(outputDir, { recursive: true });

const scopeProbe = await probeDiscountConformanceScopes(adminOptions);
assertDiscountConformanceScopes(scopeProbe);

const createResponse = await runGraphqlRaw(createDocument, createVariables);
const setupResponse = await runGraphqlRaw(createDocument, setupVariables);
const setupId = (
  setupResponse.payload as { data?: { discountCodeBasicCreate?: { codeDiscountNode?: { id?: unknown } } } }
).data?.discountCodeBasicCreate?.codeDiscountNode?.id;

if (typeof setupId !== 'string' || setupId.length === 0) {
  throw new Error(`Setup discount create did not return an id: ${JSON.stringify(setupResponse)}`);
}

updateVariables.id = setupId;
const updateResponse = await runGraphqlRaw(updateDocument, updateVariables);
const cleanupResponse = await runGraphqlRaw(cleanupDocument, { id: setupId });

const fixture = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  accessScopes: scopeProbe,
  cases: {
    create: {
      query: createDocument,
      variables: createVariables,
      status: createResponse.status,
      response: createResponse.payload,
    },
    setup: {
      query: createDocument,
      variables: setupVariables,
      status: setupResponse.status,
      response: setupResponse.payload,
    },
    update: {
      query: updateDocument,
      variables: updateVariables,
      status: updateResponse.status,
      response: updateResponse.payload,
    },
  },
  cleanup: {
    query: cleanupDocument,
    variables: { id: setupId },
    status: cleanupResponse.status,
    response: cleanupResponse.payload,
  },
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
        create: createResponse.status,
        setup: setupResponse.status,
        update: updateResponse.status,
        cleanup: cleanupResponse.status,
      },
    },
    null,
    2,
  ),
);

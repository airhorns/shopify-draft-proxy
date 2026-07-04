/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { assertDiscountConformanceScopes, probeDiscountConformanceScopes } from './discount-conformance-lib.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const scenarioId = 'discount-code-basic-name-alias-independence';
const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'discounts');
const outputPath = path.join(outputDir, `${scenarioId}.json`);
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
const code = `DRAFTNAME${runId}`;
const startsAt = new Date(Date.now() - 60_000).toISOString();

const createDocument = `#graphql
  mutation CreateDiscountForNameIndependence($input: DiscountCodeBasicInput!) {
    madeDiscount: discountCodeBasicCreate(basicCodeDiscount: $input) {
      codeDiscountNode {
        id
        codeDiscount {
          __typename
          ... on DiscountCodeBasic {
            title
            status
            asyncUsageCount
            discountClasses
            combinesWith {
              productDiscounts
              orderDiscounts
              shippingDiscounts
            }
            codes(first: 1) {
              nodes {
                code
                asyncUsageCount
              }
            }
            context {
              __typename
              ... on DiscountBuyerSelectionAll {
                all
              }
            }
          }
        }
      }
      userErrors {
        field
        message
        code
        extraInfo
      }
    }
  }
`;

const deleteDocument = `#graphql
  mutation CleanupDiscountNameIndependence($id: ID!) {
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

const createVariables = {
  input: {
    title: `Conformance name independence ${runId}`,
    code,
    startsAt,
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
      },
      items: {
        all: true,
      },
    },
  },
};

const create = await runGraphqlRaw(createDocument, createVariables);
const discountId = (
  create.payload as {
    data?: { madeDiscount?: { codeDiscountNode?: { id?: unknown } } };
  }
).data?.madeDiscount?.codeDiscountNode?.id;

if (typeof discountId !== 'string') {
  throw new Error(`Discount name-independence create did not return an id: ${JSON.stringify(create)}`);
}

const cleanupVariables = { id: discountId };
const cleanup = await runGraphqlRaw(deleteDocument, cleanupVariables);

const output = {
  scenarioId,
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  variables: {
    id: discountId,
    code,
  },
  requests: {
    create: { query: createDocument, variables: createVariables },
    cleanup: { query: deleteDocument, variables: cleanupVariables },
  },
  scopeProbe,
  create,
  cleanup,
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
      code,
    },
    null,
    2,
  ),
);

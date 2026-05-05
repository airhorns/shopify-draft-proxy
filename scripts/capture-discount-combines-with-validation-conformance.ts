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
const outputPath = path.join(outputDir, 'discount-combines-with-validation.json');
const document = await readFile('config/parity-requests/discounts/discount-combines-with-validation.graphql', 'utf8');

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const adminOptions = {
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
};
const { runGraphqlRaw } = createAdminGraphqlClient(adminOptions);

function basicOrderInput(code: string): Record<string, unknown> {
  return {
    title: `HAR-602 ${code}`,
    code,
    startsAt: '2026-05-05T00:00:00Z',
    combinesWith: {
      productDiscounts: true,
      productDiscountsWithTagsOnSameCartLine: {
        add: ['vip'],
      },
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
  };
}

function basicProductInputWithTagOverlap(code: string): Record<string, unknown> {
  return {
    title: `HAR-602 ${code}`,
    code,
    startsAt: '2026-05-05T00:00:00Z',
    combinesWith: {
      productDiscountsWithTagsOnSameCartLine: {
        add: ['same'],
        remove: ['same'],
      },
    },
    context: {
      all: 'ALL',
    },
    customerGets: {
      value: {
        percentage: 0.1,
      },
      items: {
        products: {
          productsToAdd: ['gid://shopify/Product/1'],
        },
      },
    },
  };
}

function freeShippingInput(code: string): Record<string, unknown> {
  return {
    title: `HAR-602 ${code}`,
    code,
    startsAt: '2026-05-05T00:00:00Z',
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
  };
}

await mkdir(outputDir, { recursive: true });

const scopeProbe = await probeDiscountConformanceScopes(adminOptions);
assertDiscountConformanceScopes(scopeProbe);

const stamp = Date.now();
const variables = {
  orderTagStacking: basicOrderInput(`ORDER-TAGS-${stamp}`),
  tagOverlap: basicProductInputWithTagOverlap(`TAG-OVERLAP-${stamp}`),
  freeShippingSelfCombine: freeShippingInput(`FREE-SHIP-${stamp}`),
};

const response = await runGraphqlRaw(document, variables);

const fixture = {
  storeDomain,
  apiVersion,
  accessScopes: scopeProbe,
  validation: {
    query: document,
    variables,
    response: response.payload,
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
    },
    null,
    2,
  ),
);

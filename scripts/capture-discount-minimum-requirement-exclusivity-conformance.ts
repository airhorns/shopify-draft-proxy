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
const outputPath = path.join(outputDir, 'discount-minimum-requirement-exclusivity.json');
const document = await readFile(
  'config/parity-requests/discounts/discount-minimum-requirement-exclusivity.graphql',
  'utf8',
);
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const adminOptions = {
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
};
const { runGraphqlRaw } = createAdminGraphqlClient(adminOptions);

function basicInput(code: string): Record<string, unknown> {
  return {
    title: `HAR-779 ${code}`,
    code,
    startsAt: '2026-04-25T00:00:00Z',
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

await mkdir(outputDir, { recursive: true });

const scopeProbe = await probeDiscountConformanceScopes(adminOptions);
assertDiscountConformanceScopes(scopeProbe);

const stamp = Date.now();
const variables = {
  bothCode: {
    ...basicInput(`BOTH${stamp}`),
    minimumRequirement: {
      quantity: {
        greaterThanOrEqualToQuantity: '2',
      },
      subtotal: {
        greaterThanOrEqualToSubtotal: '10.00',
      },
    },
  },
  quantityLimit: {
    ...basicInput(`QLIMIT${stamp}`),
    minimumRequirement: {
      quantity: {
        greaterThanOrEqualToQuantity: '9999999999',
      },
    },
  },
  subtotalLimit: {
    ...basicInput(`SLIMIT${stamp}`),
    minimumRequirement: {
      subtotal: {
        greaterThanOrEqualToSubtotal: '1000000000000000001.00',
      },
    },
  },
  bothAutomatic: {
    title: `HAR-779 automatic ${stamp}`,
    startsAt: '2026-04-25T00:00:00Z',
    customerGets: {
      value: {
        percentage: 0.1,
      },
      items: {
        all: true,
      },
    },
    minimumRequirement: {
      quantity: {
        greaterThanOrEqualToQuantity: '2',
      },
      subtotal: {
        greaterThanOrEqualToSubtotal: '10.00',
      },
    },
  },
};

const response = await runGraphqlRaw(document, variables);

const fixture = {
  storeDomain,
  apiVersion,
  accessScopes: scopeProbe,
  minimumRequirement: {
    query: document,
    variables,
    response: response.payload,
  },
  upstreamCalls: [],
};

await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`);
console.log(`Wrote ${outputPath}`);

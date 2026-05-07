/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type CaptureEntry = {
  document: string;
  variables: Record<string, unknown>;
  response: unknown;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const outputPath = path.join(outputDir, 'product-create-no-key-on-create.json');

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const productCreateDocument = `#graphql
  mutation ProductCreateNoKeyOnCreate($input: ProductInput!) {
    productCreate(input: $input) {
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

async function capture(document: string, variables: Record<string, unknown>): Promise<CaptureEntry> {
  const result: ConformanceGraphqlResult = await runGraphqlRequest(document, variables);
  if (result.status < 200 || result.status >= 300) {
    throw new Error(JSON.stringify(result, null, 2));
  }

  return { document, variables, response: result.payload };
}

const scenarios = {
  inputId: await capture(productCreateDocument, {
    input: {
      id: 'gid://shopify/Product/1',
      title: 'No Key ID',
    },
  }),
  inputIdBeforeBlankTitle: await capture(productCreateDocument, {
    input: {
      id: 'gid://shopify/Product/1',
      title: '',
    },
  }),
  variantProductId: await capture(productCreateDocument, {
    input: {
      title: 'No Key Variant',
      variants: [
        {
          productId: 'gid://shopify/Product/1',
          price: '1.00',
        },
      ],
    },
  }),
};

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      apiVersion,
      storeDomain,
      scenarios,
      notes: [
        'Public Admin GraphQL accepts the deprecated productCreate(input:) spelling in 2026-04 even though introspection only lists productCreate(product:).',
        'Legacy input.id returns a productCreate.userErrors payload with field ["input"] and message "id cannot be specified during creation"; it is not a top-level GraphQL error on this live store.',
        'Legacy input.variants is rejected by input coercion before resolver execution because ProductInput does not define variants.',
      ],
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
);

console.log(`Wrote ${outputPath}`);

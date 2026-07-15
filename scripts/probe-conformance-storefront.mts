/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { runStorefrontGraphqlRequest } from './conformance-graphql-client.js';
import { getStoredStorefrontAccessToken } from './shopify-conformance-auth.mjs';
import { readConformanceScriptConfig } from './conformance-script-config.js';

const STOREFRONT_TOKEN_PROBE = `#graphql
  query ConformanceStorefrontTokenProbe {
    products(first: 1) {
      nodes {
        id
        title
        tags
      }
    }
  }
`;

type StorefrontTokenProbeData = {
  products: {
    nodes: unknown[];
  };
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const storedAuth = await getStoredStorefrontAccessToken();
if (storedAuth.shop !== storeDomain) {
  throw new Error(
    `Storefront credential targets ${storedAuth.shop}, but the configured conformance store is ${storeDomain}. Regenerate the storefront token for the configured store.`,
  );
}

const result = await runStorefrontGraphqlRequest<StorefrontTokenProbeData>(
  {
    storeOrigin: adminOrigin,
    apiVersion,
    storefrontAccessToken: storedAuth.storefront_access_token,
  },
  STOREFRONT_TOKEN_PROBE,
);

if (result.status < 200 || result.status >= 300 || result.payload.errors) {
  throw new Error(`Storefront token probe failed with status ${result.status}: ${JSON.stringify(result.payload)}`);
}

const productNodes = result.payload.data?.products?.nodes;
process.stdout.write(
  JSON.stringify(
    {
      ok: true,
      shop: storedAuth.shop,
      storefrontTokenId: storedAuth.storefront_token_id,
      storefrontTokenTitle: storedAuth.storefront_token_title,
      storefrontAccessScopes: storedAuth.storefront_access_scopes,
      apiVersion,
      endpoint: `${adminOrigin}/api/${apiVersion}/graphql.json`,
      productNodeCount: Array.isArray(productNodes) ? productNodes.length : null,
      verifiedFields: ['products', 'products.nodes.id', 'products.nodes.title', 'products.nodes.tags'],
    },
    null,
    2,
  ) + '\n',
);

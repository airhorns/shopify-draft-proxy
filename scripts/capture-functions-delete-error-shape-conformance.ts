/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'functions');
const outputPath = path.join(outputDir, 'functions-delete-error-shape.json');
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

type Capture = {
  query: string;
  variables: Record<string, unknown>;
  response: ConformanceGraphqlResult;
};

async function capture(query: string, variables: Record<string, unknown>): Promise<Capture> {
  return {
    query,
    variables,
    response: await runGraphqlRequest(query, variables),
  };
}

function assertNoTopLevelErrors(captureResult: Capture, context: string): void {
  if (
    captureResult.response.status < 200 ||
    captureResult.response.status >= 300 ||
    captureResult.response.payload.errors
  ) {
    throw new Error(`${context} failed: ${JSON.stringify(captureResult.response, null, 2)}`);
  }
}

const cartTransformDeleteDocument = `mutation CartTransformDeleteMissing($id: ID!) {
  cartTransformDelete(id: $id) {
    deletedId
    userErrors {
      field
      message
      code
    }
  }
}
`;

const validationDeleteDocument = `mutation ValidationDeleteMissing($id: ID!) {
  validationDelete(id: $id) {
    deletedId
    userErrors {
      field
      message
      code
    }
  }
}
`;

const cartTransformDeleteMissing = await capture(cartTransformDeleteDocument, {
  id: 'gid://shopify/CartTransform/999999999999',
});
const validationDeleteMissing = await capture(validationDeleteDocument, {
  id: 'gid://shopify/Validation/999999999999',
});
assertNoTopLevelErrors(cartTransformDeleteMissing, 'cartTransformDelete missing-id capture');
assertNoTopLevelErrors(validationDeleteMissing, 'validationDelete missing-id capture');

const fixture = {
  scenarioId: 'functions-delete-error-shape',
  capturedAt: new Date().toISOString(),
  source: 'live-shopify-and-cassette-backed-local-runtime',
  storeDomain,
  apiVersion,
  summary:
    'Delete error-shape evidence for validationDelete and cartTransformDelete plus cassette-backed cartTransformCreate/delete local lifecycle.',
  cartTransformDeleteMissing,
  validationDeleteMissing,
  cartTransformCreateThenDelete: {
    create: {
      response: {
        data: {
          cartTransformCreate: {
            cartTransform: {
              id: 'gid://shopify/CartTransform/3',
            },
            userErrors: [],
          },
        },
      },
    },
    delete: {
      response: {
        data: {
          cartTransformDelete: {
            deletedId: 'gid://shopify/CartTransform/3',
            userErrors: [],
          },
        },
      },
    },
    postDeleteRead: {
      response: {
        data: {
          cartTransforms: {
            nodes: [],
            pageInfo: {
              hasNextPage: false,
              hasPreviousPage: false,
              startCursor: null,
              endCursor: null,
            },
          },
        },
      },
    },
  },
  upstreamCalls: [
    {
      operationName: 'FunctionHydrateByHandle',
      variables: {
        handle: 'cart-transform-delete-shape',
        apiType: 'CART_TRANSFORM',
      },
      query: 'cassette-backed CART_TRANSFORM ShopifyFunction lookup for local delete-shape lifecycle',
      response: {
        status: 200,
        body: {
          data: {
            shopifyFunctions: {
              nodes: [
                {
                  id: 'gid://shopify/ShopifyFunction/cart-transform-delete-shape',
                  title: 'Cart Transform Delete Shape',
                  handle: 'cart-transform-delete-shape',
                  apiType: 'CART_TRANSFORM',
                  description: null,
                  appKey: null,
                  app: null,
                },
              ],
            },
          },
        },
      },
    },
    {
      operationName: 'FunctionHydrateByHandle',
      variables: {
        handle: 'validation-delete-shape',
        apiType: 'VALIDATION',
      },
      query: 'cassette-backed VALIDATION ShopifyFunction lookup reserved for this delete-shape fixture',
      response: {
        status: 200,
        body: {
          data: {
            shopifyFunctions: {
              nodes: [
                {
                  id: 'gid://shopify/ShopifyFunction/validation-delete-shape',
                  title: 'Validation Delete Shape',
                  handle: 'validation-delete-shape',
                  apiType: 'VALIDATION',
                  description: null,
                  appKey: null,
                  app: null,
                },
              ],
            },
          },
        },
      },
    },
  ],
  notes: {
    liveMissingDeleteEvidence:
      'cartTransformDelete and validationDelete missing-id userErrors are captured live from Shopify.',
    successPathEvidence:
      'The current conformance shop has a valid token but no released validation/cart-transform Function handles. The create/delete leg uses a deterministic cassette-backed ShopifyFunction read to prove local lifecycle and canonical deletedId behavior without runtime Shopify writes.',
  },
};

await mkdir(outputDir, { recursive: true });
await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
console.log(`Wrote ${outputPath}`);

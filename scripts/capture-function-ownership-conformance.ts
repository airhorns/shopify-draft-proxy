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
const outputPath = path.join(outputDir, 'functions-live-owner-metadata-read.json');
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

function readRecord(value: unknown): Record<string, unknown> | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function readArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function readString(value: unknown): string | null {
  return typeof value === 'string' && value.length > 0 ? value : null;
}

function assertNoTopLevelErrors(capture: Capture, context: string): void {
  if (capture.response.status < 200 || capture.response.status >= 300 || capture.response.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(capture.response, null, 2)}`);
  }
}

function readFunctionNodes(capture: Capture): Record<string, unknown>[] {
  const data = readRecord(capture.response.payload.data);
  const connection = readRecord(data?.['shopifyFunctions']);
  return readArray(connection?.['nodes']).flatMap((node) => {
    const record = readRecord(node);
    return record ? [record] : [];
  });
}

function findFunctionNode(nodes: Record<string, unknown>[], handle: string): Record<string, unknown> {
  const node = nodes.find((candidate) => candidate['handle'] === handle);
  if (!node) {
    throw new Error(`Expected released ShopifyFunction handle ${handle} in live shopifyFunctions response.`);
  }
  return node;
}

function makeRequest(query: string, variables: Record<string, unknown> = {}): Promise<ConformanceGraphqlResult> {
  return runGraphqlRequest(query, variables);
}

async function capture(query: string, variables: Record<string, unknown> = {}): Promise<Capture> {
  return {
    query,
    variables,
    response: await makeRequest(query, variables),
  };
}

const functionOwnerReadDocument = `#graphql
  query ReadLiveFunctionOwnerMetadata($validationFunctionId: String!, $cartFunctionId: String!) {
    shopifyFunctions(first: 20) {
      nodes {
        id
        title
        handle
        apiType
        description
        appKey
        app {
          __typename
          id
          title
          handle
          apiKey
        }
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
    validationFunction: shopifyFunction(id: $validationFunctionId) {
      id
      title
      handle
      apiType
      description
      appKey
      app {
        __typename
        id
        title
        handle
        apiKey
      }
    }
    cartFunction: shopifyFunction(id: $cartFunctionId) {
      id
      title
      handle
      apiType
      description
      appKey
      app {
        __typename
        id
        title
        handle
        apiKey
      }
    }
  }
`;

const validationCreateDocument = `#graphql
  mutation FunctionValidationProbe($validation: ValidationCreateInput!) {
    validationCreate(validation: $validation) {
      validation {
        id
        title
        enabled
        blockOnFailure
        shopifyFunction {
          id
          handle
          apiType
          appKey
        }
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const cartTransformCreateDocument = `#graphql
  mutation FunctionCartTransformProbe(
    $functionHandle: String!
    $blockOnFailure: Boolean
    $metafields: [MetafieldInput!]
  ) {
    cartTransformCreate(
      functionHandle: $functionHandle
      blockOnFailure: $blockOnFailure
      metafields: $metafields
    ) {
      cartTransform {
        id
        functionId
        blockOnFailure
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const cartTransformCreateByIdDocument = `#graphql
  mutation FunctionCartTransformByIdProbe(
    $functionId: String!
    $blockOnFailure: Boolean
    $metafields: [MetafieldInput!]
  ) {
    cartTransformCreate(
      functionId: $functionId
      blockOnFailure: $blockOnFailure
      metafields: $metafields
    ) {
      cartTransform {
        id
        functionId
        blockOnFailure
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const cartTransformCreateBothIdentifiersDocument = `#graphql
  mutation FunctionCartTransformBothIdentifiersProbe(
    $functionId: String!
    $functionHandle: String!
    $blockOnFailure: Boolean
  ) {
    cartTransformCreate(
      functionId: $functionId
      functionHandle: $functionHandle
      blockOnFailure: $blockOnFailure
    ) {
      cartTransform {
        id
        functionId
        blockOnFailure
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const taxAppConfigureDocument = `#graphql
  mutation FunctionTaxAppAuthorityProbe($ready: Boolean!) {
    taxAppConfigure(ready: $ready) {
      taxAppConfiguration {
        state
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const functionResourceInventoryDocument = `#graphql
  query ReadFunctionResourceInventory {
    validations(first: 50) {
      nodes {
        id
        title
        shopifyFunction {
          id
          handle
          apiType
        }
      }
    }
    cartTransforms(first: 50) {
      nodes {
        id
        functionId
        blockOnFailure
      }
    }
  }
`;

const validationDeleteDocument = `#graphql
  mutation DeleteFunctionValidationProbe($id: ID!) {
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

const cartTransformDeleteDocument = `#graphql
  mutation DeleteFunctionCartTransformProbe($id: ID!) {
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

function inventoryValidations(captureResult: Capture): Record<string, unknown>[] {
  const data = readRecord(captureResult.response.payload.data);
  const validations = readRecord(data?.['validations']);
  return readArray(validations?.['nodes']).flatMap((node) => {
    const record = readRecord(node);
    return record ? [record] : [];
  });
}

function inventoryCartTransforms(captureResult: Capture): Record<string, unknown>[] {
  const data = readRecord(captureResult.response.payload.data);
  const cartTransforms = readRecord(data?.['cartTransforms']);
  return readArray(cartTransforms?.['nodes']).flatMap((node) => {
    const record = readRecord(node);
    return record ? [record] : [];
  });
}

async function cleanupFunctionProbeResources(cartFunctionId: string): Promise<{
  inventory: Capture;
  validationDeletes: Capture[];
  cartTransformDeletes: Capture[];
}> {
  const inventory = await capture(functionResourceInventoryDocument);
  const validationDeletes: Capture[] = [];
  const cartTransformDeletes: Capture[] = [];

  for (const validation of inventoryValidations(inventory)) {
    const id = readString(validation['id']);
    const title = readString(validation['title']);
    if (!id || !title?.startsWith('HAR-416 ')) {
      continue;
    }
    validationDeletes.push(await capture(validationDeleteDocument, { id }));
  }

  for (const cartTransform of inventoryCartTransforms(inventory)) {
    const id = readString(cartTransform['id']);
    if (!id || cartTransform['functionId'] !== cartFunctionId) {
      continue;
    }
    cartTransformDeletes.push(await capture(cartTransformDeleteDocument, { id }));
  }

  return { inventory, validationDeletes, cartTransformDeletes };
}

const initialFunctionRead = await capture(functionOwnerReadDocument, {
  validationFunctionId: 'pending-validation-function-id',
  cartFunctionId: 'pending-cart-function-id',
});
const functionNodes = readFunctionNodes(initialFunctionRead);
const validationFunction = findFunctionNode(functionNodes, 'conformance-validation');
const cartFunction = findFunctionNode(functionNodes, 'conformance-cart-transform');
const validationFunctionId = readString(validationFunction['id']);
const cartFunctionId = readString(cartFunction['id']);
if (!validationFunctionId || !cartFunctionId) {
  throw new Error('Expected validation and cart-transform Function ids in live shopifyFunctions response.');
}

const cleanupBefore = await cleanupFunctionProbeResources(cartFunctionId);
const functionOwnershipRead = await capture(functionOwnerReadDocument, {
  validationFunctionId,
  cartFunctionId,
});
assertNoTopLevelErrors(functionOwnershipRead, 'Function ownership read');

const mutationAuthorityProbes = {
  wrongApiTypeValidationWithCartFunction: await capture(validationCreateDocument, {
    validation: {
      functionHandle: readString(cartFunction['handle']),
      title: 'HAR-416 wrong API validation probe',
    },
  }),
  wrongApiTypeCartTransformWithValidationFunction: await capture(cartTransformCreateDocument, {
    functionHandle: readString(validationFunction['handle']),
    blockOnFailure: false,
  }),
  wrongApiTypeCartTransformWithValidationFunctionId: await capture(cartTransformCreateByIdDocument, {
    functionId: validationFunctionId,
    blockOnFailure: false,
  }),
  cartTransformBothIdentifiersProbe: await capture(cartTransformCreateBothIdentifiersDocument, {
    functionId: cartFunctionId,
    functionHandle: readString(cartFunction['handle']),
    blockOnFailure: false,
  }),
  validationUnknownFunctionReferenceProbe: await capture(validationCreateDocument, {
    validation: {
      functionHandle: 'har-416-unowned-validation-function',
      title: 'HAR-416 unowned validation function probe',
    },
  }),
  cartTransformUnknownFunctionReferenceProbe: await capture(cartTransformCreateDocument, {
    functionHandle: 'har-416-unowned-cart-function',
    blockOnFailure: false,
  }),
  validationMetafieldValidationProbe: await capture(validationCreateDocument, {
    validation: {
      functionHandle: readString(validationFunction['handle']),
      title: 'HAR-416 invalid metafield validation probe',
      metafields: [
        {
          namespace: '$app:har-416',
          key: 'bad json',
          type: 'json',
          value: 'not-json',
        },
      ],
    },
  }),
  cartTransformMetafieldValidationProbe: await capture(cartTransformCreateDocument, {
    functionHandle: readString(cartFunction['handle']),
    blockOnFailure: false,
    metafields: [
      {
        namespace: '$app:har-416',
        key: 'bad json',
        type: 'json',
        value: 'not-json',
      },
    ],
  }),
};

const setupAndDuplicateProbes = {
  validationCreateSetup: await capture(validationCreateDocument, {
    validation: {
      functionHandle: readString(validationFunction['handle']),
      title: 'HAR-416 validation duplicate setup',
    },
  }),
  duplicateValidationConstraintProbe: await capture(validationCreateDocument, {
    validation: {
      functionHandle: readString(validationFunction['handle']),
      title: 'HAR-416 validation duplicate follow-up',
    },
  }),
  cartTransformCreateSetup: await capture(cartTransformCreateByIdDocument, {
    functionId: cartFunctionId,
    blockOnFailure: false,
  }),
  duplicateCartTransformConstraintProbe: await capture(cartTransformCreateByIdDocument, {
    functionId: cartFunctionId,
    blockOnFailure: false,
  }),
};
const cleanupAfter = await cleanupFunctionProbeResources(cartFunctionId);
const taxAppReadinessAuthorityProbe = await capture(taxAppConfigureDocument, {
  ready: true,
});

const fixture = {
  scenarioId: 'functions-live-owner-metadata-read',
  capturedAt: new Date().toISOString(),
  source: 'live-shopify',
  storeDomain,
  apiVersion,
  summary:
    'Live Shopify Function ownership metadata for released validation and cart-transform functions in the conformance app.',
  conformanceApp: {
    clientId: '0db6d7e08e4ba05ce97440df36c7ed33',
    title: 'hermes-conformance-products',
    deployedVersionEvidence: [
      'hermes-conformance-products-37 HAR-416-function-conformance',
      'hermes-conformance-products-38 HAR-416-function-conformance-scopes',
      'hermes-conformance-products-39 HAR-416-noop-validation-function',
      'HAR-416-rework-functions HAR-416-rework-function-scope-retry',
    ],
    releasedFunctionHandles: ['conformance-validation', 'conformance-cart-transform'],
  },
  seedShopifyFunctions: readFunctionNodes(functionOwnershipRead),
  functionOwnershipRead,
  mutationAuthorityProbes,
  setupAndDuplicateProbes,
  cleanup: {
    before: cleanupBefore,
    after: cleanupAfter,
  },
  taxAppReadinessAuthorityProbe,
  blockers: {
    crossAppFunctionReferences:
      'The unattended conformance setup has one installed app with released Function extensions. The fixture records unknown/unowned handle userErrors as the reachable authority boundary, but true cross-app Function references require a second installed app and are not available in this shop setup.',
    duplicateValidationConstraint:
      'Shopify allowed multiple validationCreate calls for the same validation Function in this live shop. The fixture records that success behavior and cleans up both created validations; no duplicate-validation userError was observed to enforce.',
    taxAppReadiness:
      'taxAppConfigure is authority-gated by write_taxes and tax calculations app status. The current grant still records Shopify ACCESS_DENIED because the caller must also be a tax calculations app.',
  },
};

await mkdir(outputDir, { recursive: true });
await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
console.log(JSON.stringify({ ok: true, outputPath, storeDomain, apiVersion }, null, 2));

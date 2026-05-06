/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type CaptureStep = {
  variables: Record<string, unknown>;
  response: unknown;
};

type RawCapture = {
  label: string;
  status: number;
  response: unknown;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2025-01',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const outputPath = path.join(outputDir, 'product-variant-relationship-bulk-update-validation.json');
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const productCreateMutation = `#graphql
  mutation ProductVariantRelationshipValidationProductCreate($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
        id
        title
        handle
        status
        variants(first: 10) {
          nodes {
            id
            title
          }
        }
      }
      userErrors { field message }
    }
  }
`;

const productDeleteMutation = `#graphql
  mutation ProductVariantRelationshipValidationProductDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors { field message }
    }
  }
`;

const productVariantsBulkUpdateMutation = `#graphql
  mutation ProductVariantRelationshipValidationRequiresComponents($productId: ID!, $variants: [ProductVariantsBulkInput!]!) {
    productVariantsBulkUpdate(productId: $productId, variants: $variants) {
      product { id }
      productVariants {
        id
      }
      userErrors { field message code }
    }
  }
`;

const productVariantRelationshipBulkUpdateMutation = `#graphql
  mutation ProductVariantRelationshipBulkUpdateValidation($input: [ProductVariantRelationshipUpdateInput!]!) {
    productVariantRelationshipBulkUpdate(input: $input) {
      parentProductVariants {
        id
      }
      userErrors { field message code }
    }
  }
`;

function readObject(value: unknown, label: string): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`${label} was not an object.`);
  }
  return value as Record<string, unknown>;
}

function readArray(value: unknown, label: string): unknown[] {
  if (!Array.isArray(value)) {
    throw new Error(`${label} was not an array.`);
  }
  return value;
}

function readData(result: ConformanceGraphqlResult, label: string): Record<string, unknown> {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
  return readObject(result.payload.data, `${label}.data`);
}

function readMutationPayload(step: CaptureStep, root: string): Record<string, unknown> {
  return readObject(
    readObject(readObject(step.response, `${root}.response`)['data'], `${root}.response.data`)[root],
    `${root}.payload`,
  );
}

function readCreatedProduct(step: CaptureStep): Record<string, unknown> {
  return readObject(readMutationPayload(step, 'productCreate')['product'], 'productCreate.product');
}

function readId(source: Record<string, unknown>, label: string): string {
  if (typeof source['id'] !== 'string') {
    throw new Error(`${label} did not include an id.`);
  }
  return source['id'];
}

function readFirstVariantId(product: Record<string, unknown>): string {
  const variants = readObject(product['variants'], 'product.variants');
  const nodes = readArray(variants['nodes'], 'product.variants.nodes');
  const firstVariant = readObject(nodes[0], 'product.variants.nodes[0]');
  return readId(firstVariant, 'first variant');
}

async function runStep(label: string, query: string, variables: Record<string, unknown> = {}): Promise<CaptureStep> {
  const result = await runGraphqlRaw(query, variables);
  readData(result, label);
  return { variables, response: result.payload };
}

async function runProbe(label: string, query: string, variables: Record<string, unknown> = {}): Promise<CaptureStep> {
  const result = await runGraphqlRaw(query, variables);
  if (result.status < 200 || result.status >= 300) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
  return { variables, response: result.payload };
}

async function runRaw(label: string, query: string, variables: Record<string, unknown> = {}): Promise<RawCapture> {
  const result = await runGraphqlRaw(query, variables);
  return { label, status: result.status, response: result.payload };
}

const suffix = Date.now().toString(36);
const cleanup: RawCapture[] = [];
const productIds: string[] = [];
let fixturePayload: Record<string, unknown> | null = null;

try {
  const parentProductCreate = await runStep('parent product setup', productCreateMutation, {
    product: { title: `Variant relationship validation parent ${suffix}`, status: 'ACTIVE' },
  });
  const childProductCreate = await runStep('child product setup', productCreateMutation, {
    product: { title: `Variant relationship validation child ${suffix}`, status: 'ACTIVE' },
  });
  const parentProduct = readCreatedProduct(parentProductCreate);
  const childProduct = readCreatedProduct(childProductCreate);
  const parentProductId = readId(parentProduct, 'parent product');
  const childProductId = readId(childProduct, 'child product');
  const parentVariantId = readFirstVariantId(parentProduct);
  const childVariantId = readFirstVariantId(childProduct);
  productIds.push(parentProductId, childProductId);

  const parentRequiresComponentsUpdate = await runStep(
    'parent variant requiresComponents setup',
    productVariantsBulkUpdateMutation,
    {
      productId: parentProductId,
      variants: [{ id: parentVariantId, requiresComponents: true }],
    },
  );

  const parentAsChild = await runProbe('parent as child validation', productVariantRelationshipBulkUpdateMutation, {
    input: [
      {
        parentProductVariantId: parentVariantId,
        productVariantRelationshipsToCreate: [{ id: parentVariantId, quantity: 1 }],
      },
    ],
  });
  const quantityZero = await runProbe('quantity zero validation', productVariantRelationshipBulkUpdateMutation, {
    input: [
      {
        parentProductVariantId: parentVariantId,
        productVariantRelationshipsToCreate: [{ id: childVariantId, quantity: 0 }],
      },
    ],
  });
  const quantityTooHigh = await runProbe('quantity too high validation', productVariantRelationshipBulkUpdateMutation, {
    input: [
      {
        parentProductVariantId: parentVariantId,
        productVariantRelationshipsToCreate: [{ id: childVariantId, quantity: 1_000_000_000 }],
      },
    ],
  });
  const bothParentIds = await runProbe('both parent ids validation', productVariantRelationshipBulkUpdateMutation, {
    input: [
      {
        parentProductId,
        parentProductVariantId: parentVariantId,
        productVariantRelationshipsToCreate: [{ id: childVariantId, quantity: 1 }],
      },
    ],
  });
  const duplicateChild = await runProbe('duplicate child validation', productVariantRelationshipBulkUpdateMutation, {
    input: [
      {
        parentProductVariantId: parentVariantId,
        productVariantRelationshipsToCreate: [
          { id: childVariantId, quantity: 1 },
          { id: childVariantId, quantity: 2 },
        ],
      },
    ],
  });
  const duplicateParent = await runProbe('duplicate parent validation', productVariantRelationshipBulkUpdateMutation, {
    input: [
      {
        parentProductVariantId: parentVariantId,
        productVariantRelationshipsToCreate: [{ id: childVariantId, quantity: 1 }],
      },
      {
        parentProductVariantId: parentVariantId,
        productVariantRelationshipsToCreate: [{ id: childVariantId, quantity: 1 }],
      },
    ],
  });
  const updateNotChild = await runProbe('update not child validation', productVariantRelationshipBulkUpdateMutation, {
    input: [
      {
        parentProductVariantId: parentVariantId,
        productVariantRelationshipsToUpdate: [{ id: childVariantId, quantity: 1 }],
      },
    ],
  });

  fixturePayload = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    notes: [
      'Validation branches for productVariantRelationshipBulkUpdate against disposable parent/child products.',
      'The parent variant is set to requiresComponents before validation probes; products are deleted during cleanup.',
    ],
    setup: {
      parentProductCreate,
      childProductCreate,
      parentRequiresComponentsUpdate,
    },
    cases: {
      parentAsChild,
      quantityZero,
      quantityTooHigh,
      bothParentIds,
      duplicateChild,
      duplicateParent,
      updateNotChild,
    },
    cleanup,
    upstreamCalls: [],
  };
} finally {
  for (const productId of productIds.reverse()) {
    cleanup.push(await runRaw('cleanup productDelete', productDeleteMutation, { input: { id: productId } }));
  }
}

if (!fixturePayload) {
  throw new Error('Product variant relationship validation capture did not produce a fixture payload.');
}

await mkdir(outputDir, { recursive: true });
await writeFile(outputPath, `${JSON.stringify(fixturePayload, null, 2)}\n`, 'utf8');
console.log(`Wrote product variant relationship validation fixture to ${outputPath}`);

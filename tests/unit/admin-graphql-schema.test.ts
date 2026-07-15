import { readFileSync } from 'node:fs';
import path from 'node:path';

import { buildSchema, parse, validate, type GraphQLSchema } from 'graphql';
import { describe, expect, it } from 'vitest';

const repoRoot = new URL('../..', import.meta.url).pathname;
const manifest = JSON.parse(readFileSync(path.join(repoRoot, 'config', 'admin-graphql', 'manifest.json'), 'utf8')) as {
  executableVersions: string[];
};
const versions = manifest.executableVersions;

function loadSchema(version: string): GraphQLSchema {
  return buildSchema(readFileSync(path.join(repoRoot, 'config', 'admin-graphql', version, 'schema.graphql'), 'utf8'));
}

describe('captured Admin GraphQL schemas', () => {
  it.each(versions)('%s validates through a complete executable type graph', (version) => {
    const schema = loadSchema(version);
    expect(Object.keys(schema.getTypeMap()).length).toBeGreaterThan(500);
    expect(Object.keys(schema.getQueryType()?.getFields() ?? {}).length).toBeGreaterThan(100);
    expect(Object.keys(schema.getMutationType()?.getFields() ?? {}).length).toBeGreaterThan(300);

    const errors = validate(schema, parse('{ shop { id name } }'));
    expect(errors).toEqual([]);
  });

  it('enforces version-specific root availability', () => {
    const legacy = validate(
      loadSchema('2025-01'),
      parse('mutation { channelCreate(input: { appId: "gid://shopify/App/1" }) { userErrors { message } } }'),
    );
    const current = validate(
      loadSchema('2026-04'),
      parse('mutation { channelCreate(input: { appId: "gid://shopify/App/1" }) { userErrors { message } } }'),
    );

    expect(legacy.some((error) => error.message.includes('channelCreate'))).toBe(true);
    expect(current.some((error) => error.message.includes('channelCreate'))).toBe(false);
  });

  it.each(versions)('%s rejects removed single-variant compatibility mutations', (version) => {
    const schema = loadSchema(version);
    const documents = [
      'mutation { productVariantCreate(input: { productId: "gid://shopify/Product/1" }) { userErrors { message } } }',
      'mutation { productVariantUpdate(input: { id: "gid://shopify/ProductVariant/1" }) { userErrors { message } } }',
      'mutation { productVariantDelete(id: "gid://shopify/ProductVariant/1") { deletedProductVariantId } }',
    ];

    for (const [index, document] of documents.entries()) {
      const rootName = ['productVariantCreate', 'productVariantUpdate', 'productVariantDelete'][index];
      const errors = validate(schema, parse(document));
      expect(errors.some((error) => error.message.includes(rootName ?? ''))).toBe(true);
    }
  });

  it.each(versions)('%s rejects removed return and abandonment input values', (version) => {
    const schema = loadSchema(version);
    const returnErrors = validate(
      schema,
      parse(`
        mutation {
          returnCreate(returnInput: {
            orderId: "gid://shopify/Order/1"
            returnLineItems: [{
              fulfillmentLineItemId: "gid://shopify/FulfillmentLineItem/1"
              quantity: 1
              returnReason: OTHER
              customerNote: "Screen arrived cracked"
            }]
          }) { userErrors { message } }
        }
      `),
    );
    expect(returnErrors.some((error) => error.message.includes('customerNote'))).toBe(true);

    const abandonmentErrors = validate(
      schema,
      parse(`
        mutation {
          abandonmentUpdateActivitiesDeliveryStatuses(
            abandonmentId: "gid://shopify/Abandonment/1"
            marketingActivityId: "gid://shopify/MarketingActivity/1"
            deliveryStatus: DELIVERED
          ) { userErrors { message } }
        }
      `),
    );
    expect(abandonmentErrors.some((error) => error.message.includes('DELIVERED'))).toBe(true);
  });
});

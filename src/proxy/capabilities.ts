import type { ParsedOperation } from '../graphql/parse-operation.js';

export type CapabilityDomain = 'products' | 'unknown';
export type CapabilityExecution = 'overlay-read' | 'stage-locally' | 'passthrough';

export interface OperationCapability {
  type: ParsedOperation['type'];
  operationName: string | null;
  domain: CapabilityDomain;
  execution: CapabilityExecution;
}

const PRODUCT_QUERY_NAMES = new Set([
  'Product',
  'Products',
  'ProductsCount',
  'ProductVariant',
  'InventoryItem',
  'Collection',
  'Collections',
  'product',
  'products',
  'productsCount',
  'productVariant',
  'inventoryItem',
  'collection',
  'collections',
]);
const PRODUCT_MUTATION_NAMES = new Set([
  'ProductCreate',
  'ProductUpdate',
  'ProductDelete',
  'ProductDuplicate',
  'ProductSet',
  'ProductChangeStatus',
  'ProductPublish',
  'ProductUnpublish',
  'ProductOptionsCreate',
  'ProductOptionUpdate',
  'ProductOptionsDelete',
  'ProductVariantsBulkCreate',
  'ProductVariantsBulkUpdate',
  'ProductVariantsBulkDelete',
  'ProductVariantCreate',
  'ProductVariantUpdate',
  'ProductVariantDelete',
  'MetafieldsSet',
  'MetafieldDelete',
  'CollectionCreate',
  'CollectionUpdate',
  'CollectionDelete',
  'CollectionAddProducts',
  'CollectionRemoveProducts',
  'tagsAdd',
  'tagsRemove',
  'productCreate',
  'productUpdate',
  'productDelete',
  'productDuplicate',
  'productSet',
  'productChangeStatus',
  'productPublish',
  'productUnpublish',
  'productOptionsCreate',
  'productOptionUpdate',
  'productOptionsDelete',
  'productVariantsBulkCreate',
  'productVariantsBulkUpdate',
  'productVariantsBulkDelete',
  'productVariantCreate',
  'productVariantUpdate',
  'productVariantDelete',
  'collectionCreate',
  'collectionUpdate',
  'collectionDelete',
  'collectionAddProducts',
  'collectionRemoveProducts',
  'productCreateMedia',
  'productUpdateMedia',
  'productDeleteMedia',
  'inventoryAdjustQuantities',
  'metafieldsSet',
  'metafieldDelete',
  'ProductCreateMedia',
  'ProductUpdateMedia',
  'ProductDeleteMedia',
  'InventoryAdjustQuantities',
]);

function getCandidateOperationNames(operation: ParsedOperation): string[] {
  const names = [operation.name, operation.rootFields?.[0] ?? null].filter(
    (value): value is string => typeof value === 'string' && value.length > 0,
  );

  return [...new Set(names)];
}

export function getOperationCapability(operation: ParsedOperation): OperationCapability {
  const candidates = getCandidateOperationNames(operation);
  const matchedProductQuery = candidates.find((candidate) => PRODUCT_QUERY_NAMES.has(candidate));
  if (matchedProductQuery && operation.type === 'query') {
    return {
      type: operation.type,
      operationName: matchedProductQuery,
      domain: 'products',
      execution: 'overlay-read',
    };
  }

  const matchedProductMutation = candidates.find((candidate) => PRODUCT_MUTATION_NAMES.has(candidate));
  if (matchedProductMutation && operation.type === 'mutation') {
    return {
      type: operation.type,
      operationName: matchedProductMutation,
      domain: 'products',
      execution: 'stage-locally',
    };
  }

  return {
    type: operation.type,
    operationName: candidates[0] ?? null,
    domain: 'unknown',
    execution: 'passthrough',
  };
}

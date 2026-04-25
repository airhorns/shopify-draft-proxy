import { describe, expect, it } from 'vitest';
import { getOperationCapability } from '../../src/proxy/capabilities.js';

describe('getOperationCapability', () => {
  it('marks product queries as overlay-capable reads', () => {
    expect(getOperationCapability({ type: 'query', name: 'Product', rootFields: ['product'] })).toEqual({
      domain: 'products',
      execution: 'overlay-read',
      operationName: 'Product',
      type: 'query',
    });
  });

  it('marks productsCount queries as overlay-capable reads', () => {
    expect(getOperationCapability({ type: 'query', name: 'ProductsCount', rootFields: ['productsCount'] })).toEqual({
      domain: 'products',
      execution: 'overlay-read',
      operationName: 'ProductsCount',
      type: 'query',
    });
  });

  it('marks collection queries as overlay-capable reads', () => {
    expect(getOperationCapability({ type: 'query', name: 'Collection', rootFields: ['collection'] })).toEqual({
      domain: 'products',
      execution: 'overlay-read',
      operationName: 'Collection',
      type: 'query',
    });

    expect(getOperationCapability({ type: 'query', name: 'Collections', rootFields: ['collections'] })).toEqual({
      domain: 'products',
      execution: 'overlay-read',
      operationName: 'Collections',
      type: 'query',
    });

    expect(
      getOperationCapability({
        type: 'query',
        name: 'CollectionByIdentifier',
        rootFields: ['collectionByIdentifier'],
      }),
    ).toEqual({
      domain: 'products',
      execution: 'overlay-read',
      operationName: 'CollectionByIdentifier',
      type: 'query',
    });

    expect(
      getOperationCapability({
        type: 'query',
        name: 'CollectionByHandle',
        rootFields: ['collectionByHandle'],
      }),
    ).toEqual({
      domain: 'products',
      execution: 'overlay-read',
      operationName: 'CollectionByHandle',
      type: 'query',
    });
  });

  it('classifies collection identifier roots by root field when the operation name is misleading', () => {
    expect(
      getOperationCapability({ type: 'query', name: 'Collection', rootFields: ['collectionByIdentifier'] }),
    ).toEqual({
      domain: 'products',
      execution: 'overlay-read',
      operationName: 'collectionByIdentifier',
      type: 'query',
    });

    expect(getOperationCapability({ type: 'query', name: 'Collection', rootFields: ['collectionByHandle'] })).toEqual({
      domain: 'products',
      execution: 'overlay-read',
      operationName: 'collectionByHandle',
      type: 'query',
    });
  });

  it('marks customer queries and customersCount as overlay-capable reads', () => {
    expect(getOperationCapability({ type: 'query', name: 'Customer', rootFields: ['customer'] })).toEqual({
      domain: 'customers',
      execution: 'overlay-read',
      operationName: 'Customer',
      type: 'query',
    });

    expect(getOperationCapability({ type: 'query', name: 'Customers', rootFields: ['customers'] })).toEqual({
      domain: 'customers',
      execution: 'overlay-read',
      operationName: 'Customers',
      type: 'query',
    });

    expect(getOperationCapability({ type: 'query', name: 'CustomersCount', rootFields: ['customersCount'] })).toEqual({
      domain: 'customers',
      execution: 'overlay-read',
      operationName: 'CustomersCount',
      type: 'query',
    });

    expect(
      getOperationCapability({ type: 'query', name: 'CustomerByIdentifier', rootFields: ['customerByIdentifier'] }),
    ).toEqual({
      domain: 'customers',
      execution: 'overlay-read',
      operationName: 'CustomerByIdentifier',
      type: 'query',
    });
  });

  it('classifies customerByIdentifier by root field when the operation name is misleading', () => {
    expect(getOperationCapability({ type: 'query', name: 'Customer', rootFields: ['customerByIdentifier'] })).toEqual({
      domain: 'customers',
      execution: 'overlay-read',
      operationName: 'customerByIdentifier',
      type: 'query',
    });
  });

  it('marks customer create, update, and delete as locally staged mutations', () => {
    expect(
      getOperationCapability({ type: 'mutation', name: 'CustomerCreate', rootFields: ['customerCreate'] }),
    ).toEqual({
      domain: 'customers',
      execution: 'stage-locally',
      operationName: 'CustomerCreate',
      type: 'mutation',
    });

    expect(
      getOperationCapability({ type: 'mutation', name: 'CustomerUpdate', rootFields: ['customerUpdate'] }),
    ).toEqual({
      domain: 'customers',
      execution: 'stage-locally',
      operationName: 'CustomerUpdate',
      type: 'mutation',
    });

    expect(
      getOperationCapability({ type: 'mutation', name: 'CustomerDelete', rootFields: ['customerDelete'] }),
    ).toEqual({
      domain: 'customers',
      execution: 'stage-locally',
      operationName: 'CustomerDelete',
      type: 'mutation',
    });
  });

  it('marks top-level product variant, inventory item, and inventory level queries as overlay-capable reads', () => {
    expect(getOperationCapability({ type: 'query', name: 'ProductVariant', rootFields: ['productVariant'] })).toEqual({
      domain: 'products',
      execution: 'overlay-read',
      operationName: 'ProductVariant',
      type: 'query',
    });

    expect(getOperationCapability({ type: 'query', name: 'InventoryItem', rootFields: ['inventoryItem'] })).toEqual({
      domain: 'products',
      execution: 'overlay-read',
      operationName: 'InventoryItem',
      type: 'query',
    });

    expect(getOperationCapability({ type: 'query', name: 'InventoryLevel', rootFields: ['inventoryLevel'] })).toEqual({
      domain: 'products',
      execution: 'overlay-read',
      operationName: 'InventoryLevel',
      type: 'query',
    });
  });

  it('marks productCreate as a locally staged mutation', () => {
    expect(getOperationCapability({ type: 'mutation', name: 'ProductCreate', rootFields: ['productCreate'] })).toEqual({
      domain: 'products',
      execution: 'stage-locally',
      operationName: 'ProductCreate',
      type: 'mutation',
    });
  });

  it('marks productChangeStatus as a locally staged mutation', () => {
    expect(
      getOperationCapability({ type: 'mutation', name: 'ProductChangeStatus', rootFields: ['productChangeStatus'] }),
    ).toEqual({
      domain: 'products',
      execution: 'stage-locally',
      operationName: 'ProductChangeStatus',
      type: 'mutation',
    });
  });

  it('marks product publication mutations as locally staged mutations', () => {
    expect(
      getOperationCapability({ type: 'mutation', name: 'ProductPublish', rootFields: ['productPublish'] }),
    ).toEqual({
      domain: 'products',
      execution: 'stage-locally',
      operationName: 'ProductPublish',
      type: 'mutation',
    });

    expect(
      getOperationCapability({ type: 'mutation', name: 'ProductUnpublish', rootFields: ['productUnpublish'] }),
    ).toEqual({
      domain: 'products',
      execution: 'stage-locally',
      operationName: 'ProductUnpublish',
      type: 'mutation',
    });
  });

  it('marks product tag mutations as locally staged mutations', () => {
    expect(getOperationCapability({ type: 'mutation', name: 'tagsAdd', rootFields: ['tagsAdd'] })).toEqual({
      domain: 'products',
      execution: 'stage-locally',
      operationName: 'tagsAdd',
      type: 'mutation',
    });

    expect(getOperationCapability({ type: 'mutation', name: 'tagsRemove', rootFields: ['tagsRemove'] })).toEqual({
      domain: 'products',
      execution: 'stage-locally',
      operationName: 'tagsRemove',
      type: 'mutation',
    });
  });

  it('marks productDuplicate as a locally staged mutation', () => {
    expect(
      getOperationCapability({ type: 'mutation', name: 'ProductDuplicate', rootFields: ['productDuplicate'] }),
    ).toEqual({
      domain: 'products',
      execution: 'stage-locally',
      operationName: 'ProductDuplicate',
      type: 'mutation',
    });
  });

  it('marks productSet as a locally staged mutation', () => {
    expect(getOperationCapability({ type: 'mutation', name: 'ProductSet', rootFields: ['productSet'] })).toEqual({
      domain: 'products',
      execution: 'stage-locally',
      operationName: 'ProductSet',
      type: 'mutation',
    });
  });

  it('marks product option mutations as locally staged mutations', () => {
    expect(
      getOperationCapability({ type: 'mutation', name: 'ProductOptionsCreate', rootFields: ['productOptionsCreate'] }),
    ).toEqual({
      domain: 'products',
      execution: 'stage-locally',
      operationName: 'ProductOptionsCreate',
      type: 'mutation',
    });

    expect(
      getOperationCapability({ type: 'mutation', name: 'ProductOptionUpdate', rootFields: ['productOptionUpdate'] }),
    ).toEqual({
      domain: 'products',
      execution: 'stage-locally',
      operationName: 'ProductOptionUpdate',
      type: 'mutation',
    });

    expect(
      getOperationCapability({ type: 'mutation', name: 'ProductOptionsDelete', rootFields: ['productOptionsDelete'] }),
    ).toEqual({
      domain: 'products',
      execution: 'stage-locally',
      operationName: 'ProductOptionsDelete',
      type: 'mutation',
    });
  });

  it('marks product variant bulk mutations as locally staged mutations', () => {
    expect(
      getOperationCapability({
        type: 'mutation',
        name: 'ProductVariantsBulkCreate',
        rootFields: ['productVariantsBulkCreate'],
      }),
    ).toEqual({
      domain: 'products',
      execution: 'stage-locally',
      operationName: 'ProductVariantsBulkCreate',
      type: 'mutation',
    });

    expect(
      getOperationCapability({
        type: 'mutation',
        name: 'ProductVariantsBulkUpdate',
        rootFields: ['productVariantsBulkUpdate'],
      }),
    ).toEqual({
      domain: 'products',
      execution: 'stage-locally',
      operationName: 'ProductVariantsBulkUpdate',
      type: 'mutation',
    });

    expect(
      getOperationCapability({
        type: 'mutation',
        name: 'ProductVariantsBulkDelete',
        rootFields: ['productVariantsBulkDelete'],
      }),
    ).toEqual({
      domain: 'products',
      execution: 'stage-locally',
      operationName: 'ProductVariantsBulkDelete',
      type: 'mutation',
    });
  });

  it('marks singular product variant mutations as locally staged mutations', () => {
    expect(
      getOperationCapability({ type: 'mutation', name: 'ProductVariantCreate', rootFields: ['productVariantCreate'] }),
    ).toEqual({
      domain: 'products',
      execution: 'stage-locally',
      operationName: 'ProductVariantCreate',
      type: 'mutation',
    });

    expect(
      getOperationCapability({ type: 'mutation', name: 'ProductVariantUpdate', rootFields: ['productVariantUpdate'] }),
    ).toEqual({
      domain: 'products',
      execution: 'stage-locally',
      operationName: 'ProductVariantUpdate',
      type: 'mutation',
    });

    expect(
      getOperationCapability({ type: 'mutation', name: 'ProductVariantDelete', rootFields: ['productVariantDelete'] }),
    ).toEqual({
      domain: 'products',
      execution: 'stage-locally',
      operationName: 'ProductVariantDelete',
      type: 'mutation',
    });
  });

  it('marks metafield mutations as locally staged mutations', () => {
    expect(getOperationCapability({ type: 'mutation', name: 'MetafieldsSet', rootFields: ['metafieldsSet'] })).toEqual({
      domain: 'products',
      execution: 'stage-locally',
      operationName: 'MetafieldsSet',
      type: 'mutation',
    });

    expect(
      getOperationCapability({ type: 'mutation', name: 'MetafieldsDelete', rootFields: ['metafieldsDelete'] }),
    ).toEqual({
      domain: 'products',
      execution: 'stage-locally',
      operationName: 'MetafieldsDelete',
      type: 'mutation',
    });

    expect(
      getOperationCapability({ type: 'mutation', name: 'MetafieldDelete', rootFields: ['metafieldDelete'] }),
    ).toEqual({
      domain: 'products',
      execution: 'stage-locally',
      operationName: 'MetafieldDelete',
      type: 'mutation',
    });
  });

  it('marks collection mutations as locally staged mutations', () => {
    expect(
      getOperationCapability({ type: 'mutation', name: 'CollectionCreate', rootFields: ['collectionCreate'] }),
    ).toEqual({
      domain: 'products',
      execution: 'stage-locally',
      operationName: 'CollectionCreate',
      type: 'mutation',
    });

    expect(
      getOperationCapability({ type: 'mutation', name: 'CollectionUpdate', rootFields: ['collectionUpdate'] }),
    ).toEqual({
      domain: 'products',
      execution: 'stage-locally',
      operationName: 'CollectionUpdate',
      type: 'mutation',
    });

    expect(
      getOperationCapability({ type: 'mutation', name: 'CollectionDelete', rootFields: ['collectionDelete'] }),
    ).toEqual({
      domain: 'products',
      execution: 'stage-locally',
      operationName: 'CollectionDelete',
      type: 'mutation',
    });

    expect(
      getOperationCapability({
        type: 'mutation',
        name: 'CollectionReorderProducts',
        rootFields: ['collectionReorderProducts'],
      }),
    ).toEqual({
      domain: 'products',
      execution: 'stage-locally',
      operationName: 'CollectionReorderProducts',
      type: 'mutation',
    });
  });

  it('marks product media mutations as locally staged mutations', () => {
    expect(
      getOperationCapability({ type: 'mutation', name: 'ProductCreateMedia', rootFields: ['productCreateMedia'] }),
    ).toEqual({
      domain: 'products',
      execution: 'stage-locally',
      operationName: 'ProductCreateMedia',
      type: 'mutation',
    });

    expect(
      getOperationCapability({ type: 'mutation', name: 'ProductUpdateMedia', rootFields: ['productUpdateMedia'] }),
    ).toEqual({
      domain: 'products',
      execution: 'stage-locally',
      operationName: 'ProductUpdateMedia',
      type: 'mutation',
    });

    expect(
      getOperationCapability({ type: 'mutation', name: 'ProductDeleteMedia', rootFields: ['productDeleteMedia'] }),
    ).toEqual({
      domain: 'products',
      execution: 'stage-locally',
      operationName: 'ProductDeleteMedia',
      type: 'mutation',
    });
  });

  it('marks generic media file mutations as locally staged media mutations', () => {
    expect(getOperationCapability({ type: 'mutation', name: 'FileCreate', rootFields: ['fileCreate'] })).toEqual({
      domain: 'media',
      execution: 'stage-locally',
      operationName: 'FileCreate',
      type: 'mutation',
    });

    expect(getOperationCapability({ type: 'mutation', name: 'FileUpdate', rootFields: ['fileUpdate'] })).toEqual({
      domain: 'media',
      execution: 'stage-locally',
      operationName: 'FileUpdate',
      type: 'mutation',
    });

    expect(getOperationCapability({ type: 'mutation', name: 'FileDelete', rootFields: ['fileDelete'] })).toEqual({
      domain: 'media',
      execution: 'stage-locally',
      operationName: 'FileDelete',
      type: 'mutation',
    });
  });

  it('marks inventoryAdjustQuantities as a locally staged mutation', () => {
    expect(
      getOperationCapability({
        type: 'mutation',
        name: 'InventoryAdjustQuantities',
        rootFields: ['inventoryAdjustQuantities'],
      }),
    ).toEqual({
      domain: 'products',
      execution: 'stage-locally',
      operationName: 'InventoryAdjustQuantities',
      type: 'mutation',
    });
  });

  it('falls back to passthrough for unknown operations', () => {
    expect(
      getOperationCapability({ type: 'mutation', name: 'UnknownMutation', rootFields: ['unknownMutation'] }),
    ).toEqual({
      domain: 'unknown',
      execution: 'passthrough',
      operationName: 'UnknownMutation',
      type: 'mutation',
    });
  });

  it('routes implemented discount catalog and singular roots through the local overlay', () => {
    expect(getOperationCapability({ type: 'query', name: 'DiscountNodes', rootFields: ['discountNodes'] })).toEqual({
      domain: 'discounts',
      execution: 'overlay-read',
      operationName: 'DiscountNodes',
      type: 'query',
    });

    expect(
      getOperationCapability({
        type: 'query',
        name: 'DiscountNodesCount',
        rootFields: ['discountNodesCount'],
      }),
    ).toEqual({
      domain: 'discounts',
      execution: 'overlay-read',
      operationName: 'DiscountNodesCount',
      type: 'query',
    });

    expect(
      getOperationCapability({
        type: 'query',
        name: 'CodeDiscountNodeByCode',
        rootFields: ['codeDiscountNodeByCode'],
      }),
    ).toEqual({
      domain: 'discounts',
      execution: 'overlay-read',
      operationName: 'CodeDiscountNodeByCode',
      type: 'query',
    });

    expect(
      getOperationCapability({
        type: 'query',
        name: 'AutomaticDiscountNode',
        rootFields: ['automaticDiscountNode'],
      }),
    ).toEqual({
      domain: 'discounts',
      execution: 'overlay-read',
      operationName: 'AutomaticDiscountNode',
      type: 'query',
    });

    expect(
      getOperationCapability({
        type: 'mutation',
        name: 'CreateDiscount',
        rootFields: ['discountCodeBasicCreate'],
      }),
    ).toEqual({
      domain: 'unknown',
      execution: 'passthrough',
      operationName: 'CreateDiscount',
      type: 'mutation',
    });

    expect(
      getOperationCapability({
        type: 'mutation',
        name: 'DeleteAutomaticDiscounts',
        rootFields: ['discountAutomaticBulkDelete'],
      }),
    ).toEqual({
      domain: 'unknown',
      execution: 'passthrough',
      operationName: 'DeleteAutomaticDiscounts',
      type: 'mutation',
    });
  });

  it('marks inventoryItemUpdate as a locally staged mutation', () => {
    expect(
      getOperationCapability({ type: 'mutation', name: 'InventoryItemUpdate', rootFields: ['inventoryItemUpdate'] }),
    ).toEqual({
      domain: 'products',
      execution: 'stage-locally',
      operationName: 'InventoryItemUpdate',
      type: 'mutation',
    });
  });

  it('marks inventory linkage mutations as locally staged mutations', () => {
    expect(
      getOperationCapability({ type: 'mutation', name: 'InventoryActivate', rootFields: ['inventoryActivate'] }),
    ).toEqual({
      domain: 'products',
      execution: 'stage-locally',
      operationName: 'InventoryActivate',
      type: 'mutation',
    });
    expect(
      getOperationCapability({ type: 'mutation', name: 'InventoryDeactivate', rootFields: ['inventoryDeactivate'] }),
    ).toEqual({
      domain: 'products',
      execution: 'stage-locally',
      operationName: 'InventoryDeactivate',
      type: 'mutation',
    });
    expect(
      getOperationCapability({
        type: 'mutation',
        name: 'InventoryBulkToggleActivation',
        rootFields: ['inventoryBulkToggleActivation'],
      }),
    ).toEqual({
      domain: 'products',
      execution: 'stage-locally',
      operationName: 'InventoryBulkToggleActivation',
      type: 'mutation',
    });
  });

  it('falls back to passthrough for anonymous operations', () => {
    expect(getOperationCapability({ type: 'query', name: null, rootFields: [] })).toEqual({
      domain: 'unknown',
      execution: 'passthrough',
      operationName: null,
      type: 'query',
    });
  });

  it('routes implemented metafield definition read roots through the local overlay', () => {
    expect(
      getOperationCapability({
        type: 'query',
        name: 'MetafieldDefinitions',
        rootFields: ['metafieldDefinitions'],
      }),
    ).toEqual({
      domain: 'metafields',
      execution: 'overlay-read',
      operationName: 'MetafieldDefinitions',
      type: 'query',
    });

    expect(
      getOperationCapability({
        type: 'query',
        name: 'MetafieldDefinition',
        rootFields: ['metafieldDefinition'],
      }),
    ).toEqual({
      domain: 'metafields',
      execution: 'overlay-read',
      operationName: 'MetafieldDefinition',
      type: 'query',
    });
  });
});

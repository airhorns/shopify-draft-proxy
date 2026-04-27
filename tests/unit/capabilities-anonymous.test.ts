import { describe, expect, it } from 'vitest';
import { getOperationCapability } from '../../src/proxy/capabilities.js';

describe('getOperationCapability anonymous operations', () => {
  it('classifies anonymous productCreate by root field name', () => {
    expect(getOperationCapability({ type: 'mutation', name: null, rootFields: ['productCreate'] })).toEqual({
      domain: 'products',
      execution: 'stage-locally',
      operationName: 'productCreate',
      type: 'mutation',
    });
  });

  it('classifies anonymous productChangeStatus by root field name', () => {
    expect(getOperationCapability({ type: 'mutation', name: null, rootFields: ['productChangeStatus'] })).toEqual({
      domain: 'products',
      execution: 'stage-locally',
      operationName: 'productChangeStatus',
      type: 'mutation',
    });
  });

  it('classifies anonymous productSet by root field name', () => {
    expect(getOperationCapability({ type: 'mutation', name: null, rootFields: ['productSet'] })).toEqual({
      domain: 'products',
      execution: 'stage-locally',
      operationName: 'productSet',
      type: 'mutation',
    });
  });

  it('classifies anonymous product publication mutations by root field name', () => {
    expect(getOperationCapability({ type: 'mutation', name: null, rootFields: ['productPublish'] })).toEqual({
      domain: 'products',
      execution: 'stage-locally',
      operationName: 'productPublish',
      type: 'mutation',
    });

    expect(getOperationCapability({ type: 'mutation', name: null, rootFields: ['productUnpublish'] })).toEqual({
      domain: 'products',
      execution: 'stage-locally',
      operationName: 'productUnpublish',
      type: 'mutation',
    });
  });

  it('classifies anonymous product tag mutations by root field name', () => {
    expect(getOperationCapability({ type: 'mutation', name: null, rootFields: ['tagsAdd'] })).toEqual({
      domain: 'products',
      execution: 'stage-locally',
      operationName: 'tagsAdd',
      type: 'mutation',
    });

    expect(getOperationCapability({ type: 'mutation', name: null, rootFields: ['tagsRemove'] })).toEqual({
      domain: 'products',
      execution: 'stage-locally',
      operationName: 'tagsRemove',
      type: 'mutation',
    });
  });

  it('classifies anonymous payment customization mutations by root field name', () => {
    for (const rootField of [
      'paymentCustomizationActivation',
      'paymentCustomizationCreate',
      'paymentCustomizationDelete',
      'paymentCustomizationUpdate',
    ]) {
      expect(getOperationCapability({ type: 'mutation', name: null, rootFields: [rootField] })).toEqual({
        domain: 'payments',
        execution: 'stage-locally',
        operationName: rootField,
        type: 'mutation',
      });
    }
  });

  it('classifies anonymous product option mutations by root field name', () => {
    expect(getOperationCapability({ type: 'mutation', name: null, rootFields: ['productOptionsCreate'] })).toEqual({
      domain: 'products',
      execution: 'stage-locally',
      operationName: 'productOptionsCreate',
      type: 'mutation',
    });

    expect(getOperationCapability({ type: 'mutation', name: null, rootFields: ['productOptionUpdate'] })).toEqual({
      domain: 'products',
      execution: 'stage-locally',
      operationName: 'productOptionUpdate',
      type: 'mutation',
    });

    expect(getOperationCapability({ type: 'mutation', name: null, rootFields: ['productOptionsDelete'] })).toEqual({
      domain: 'products',
      execution: 'stage-locally',
      operationName: 'productOptionsDelete',
      type: 'mutation',
    });
  });

  it('classifies anonymous product variant bulk mutations by root field name', () => {
    expect(getOperationCapability({ type: 'mutation', name: null, rootFields: ['productVariantsBulkCreate'] })).toEqual(
      {
        domain: 'products',
        execution: 'stage-locally',
        operationName: 'productVariantsBulkCreate',
        type: 'mutation',
      },
    );

    expect(getOperationCapability({ type: 'mutation', name: null, rootFields: ['productVariantsBulkUpdate'] })).toEqual(
      {
        domain: 'products',
        execution: 'stage-locally',
        operationName: 'productVariantsBulkUpdate',
        type: 'mutation',
      },
    );

    expect(getOperationCapability({ type: 'mutation', name: null, rootFields: ['productVariantsBulkDelete'] })).toEqual(
      {
        domain: 'products',
        execution: 'stage-locally',
        operationName: 'productVariantsBulkDelete',
        type: 'mutation',
      },
    );
  });

  it('classifies anonymous singular product variant mutations by root field name', () => {
    expect(getOperationCapability({ type: 'mutation', name: null, rootFields: ['productVariantCreate'] })).toEqual({
      domain: 'products',
      execution: 'stage-locally',
      operationName: 'productVariantCreate',
      type: 'mutation',
    });

    expect(getOperationCapability({ type: 'mutation', name: null, rootFields: ['productVariantUpdate'] })).toEqual({
      domain: 'products',
      execution: 'stage-locally',
      operationName: 'productVariantUpdate',
      type: 'mutation',
    });

    expect(getOperationCapability({ type: 'mutation', name: null, rootFields: ['productVariantDelete'] })).toEqual({
      domain: 'products',
      execution: 'stage-locally',
      operationName: 'productVariantDelete',
      type: 'mutation',
    });
  });

  it('classifies anonymous metafield delete mutations by root field name', () => {
    expect(getOperationCapability({ type: 'mutation', name: null, rootFields: ['metafieldsDelete'] })).toEqual({
      domain: 'products',
      execution: 'stage-locally',
      operationName: 'metafieldsDelete',
      type: 'mutation',
    });

    expect(getOperationCapability({ type: 'mutation', name: null, rootFields: ['metafieldDelete'] })).toEqual({
      domain: 'products',
      execution: 'stage-locally',
      operationName: 'metafieldDelete',
      type: 'mutation',
    });
  });

  it('classifies anonymous collection mutations by root field name', () => {
    expect(getOperationCapability({ type: 'mutation', name: null, rootFields: ['collectionCreate'] })).toEqual({
      domain: 'products',
      execution: 'stage-locally',
      operationName: 'collectionCreate',
      type: 'mutation',
    });

    expect(getOperationCapability({ type: 'mutation', name: null, rootFields: ['collectionUpdate'] })).toEqual({
      domain: 'products',
      execution: 'stage-locally',
      operationName: 'collectionUpdate',
      type: 'mutation',
    });

    expect(getOperationCapability({ type: 'mutation', name: null, rootFields: ['collectionDelete'] })).toEqual({
      domain: 'products',
      execution: 'stage-locally',
      operationName: 'collectionDelete',
      type: 'mutation',
    });
  });

  it('classifies anonymous product media mutations by root field name', () => {
    expect(getOperationCapability({ type: 'mutation', name: null, rootFields: ['productCreateMedia'] })).toEqual({
      domain: 'products',
      execution: 'stage-locally',
      operationName: 'productCreateMedia',
      type: 'mutation',
    });

    expect(getOperationCapability({ type: 'mutation', name: null, rootFields: ['productUpdateMedia'] })).toEqual({
      domain: 'products',
      execution: 'stage-locally',
      operationName: 'productUpdateMedia',
      type: 'mutation',
    });

    expect(getOperationCapability({ type: 'mutation', name: null, rootFields: ['productDeleteMedia'] })).toEqual({
      domain: 'products',
      execution: 'stage-locally',
      operationName: 'productDeleteMedia',
      type: 'mutation',
    });
  });

  it('classifies anonymous generic media mutations by root field name', () => {
    expect(getOperationCapability({ type: 'mutation', name: null, rootFields: ['fileCreate'] })).toEqual({
      domain: 'media',
      execution: 'stage-locally',
      operationName: 'fileCreate',
      type: 'mutation',
    });

    expect(getOperationCapability({ type: 'mutation', name: null, rootFields: ['fileUpdate'] })).toEqual({
      domain: 'media',
      execution: 'stage-locally',
      operationName: 'fileUpdate',
      type: 'mutation',
    });

    expect(getOperationCapability({ type: 'mutation', name: null, rootFields: ['fileDelete'] })).toEqual({
      domain: 'media',
      execution: 'stage-locally',
      operationName: 'fileDelete',
      type: 'mutation',
    });
  });

  it('classifies anonymous inventory adjustments by root field name', () => {
    expect(getOperationCapability({ type: 'mutation', name: null, rootFields: ['inventoryAdjustQuantities'] })).toEqual(
      {
        domain: 'products',
        execution: 'stage-locally',
        operationName: 'inventoryAdjustQuantities',
        type: 'mutation',
      },
    );
  });

  it('classifies anonymous products query by root field name', () => {
    expect(getOperationCapability({ type: 'query', name: null, rootFields: ['products'] })).toEqual({
      domain: 'products',
      execution: 'overlay-read',
      operationName: 'products',
      type: 'query',
    });
  });

  it('classifies anonymous productsCount query by root field name', () => {
    expect(getOperationCapability({ type: 'query', name: null, rootFields: ['productsCount'] })).toEqual({
      domain: 'products',
      execution: 'overlay-read',
      operationName: 'productsCount',
      type: 'query',
    });
  });

  it('classifies anonymous collection queries by root field name', () => {
    expect(getOperationCapability({ type: 'query', name: null, rootFields: ['collection'] })).toEqual({
      domain: 'products',
      execution: 'overlay-read',
      operationName: 'collection',
      type: 'query',
    });

    expect(getOperationCapability({ type: 'query', name: null, rootFields: ['collections'] })).toEqual({
      domain: 'products',
      execution: 'overlay-read',
      operationName: 'collections',
      type: 'query',
    });

    expect(getOperationCapability({ type: 'query', name: null, rootFields: ['collectionByIdentifier'] })).toEqual({
      domain: 'products',
      execution: 'overlay-read',
      operationName: 'collectionByIdentifier',
      type: 'query',
    });

    expect(getOperationCapability({ type: 'query', name: null, rootFields: ['collectionByHandle'] })).toEqual({
      domain: 'products',
      execution: 'overlay-read',
      operationName: 'collectionByHandle',
      type: 'query',
    });
  });

  it('classifies anonymous customer queries and customersCount by root field name', () => {
    expect(getOperationCapability({ type: 'query', name: null, rootFields: ['customer'] })).toEqual({
      domain: 'customers',
      execution: 'overlay-read',
      operationName: 'customer',
      type: 'query',
    });

    expect(getOperationCapability({ type: 'query', name: null, rootFields: ['customers'] })).toEqual({
      domain: 'customers',
      execution: 'overlay-read',
      operationName: 'customers',
      type: 'query',
    });

    expect(getOperationCapability({ type: 'query', name: null, rootFields: ['customersCount'] })).toEqual({
      domain: 'customers',
      execution: 'overlay-read',
      operationName: 'customersCount',
      type: 'query',
    });

    expect(getOperationCapability({ type: 'query', name: null, rootFields: ['customerByIdentifier'] })).toEqual({
      domain: 'customers',
      execution: 'overlay-read',
      operationName: 'customerByIdentifier',
      type: 'query',
    });

    expect(getOperationCapability({ type: 'mutation', name: null, rootFields: ['customerSet'] })).toEqual({
      domain: 'customers',
      execution: 'stage-locally',
      operationName: 'customerSet',
      type: 'mutation',
    });
  });

  it('classifies anonymous top-level product variant and inventory item queries by root field name', () => {
    expect(getOperationCapability({ type: 'query', name: null, rootFields: ['productVariant'] })).toEqual({
      domain: 'products',
      execution: 'overlay-read',
      operationName: 'productVariant',
      type: 'query',
    });

    expect(getOperationCapability({ type: 'query', name: null, rootFields: ['inventoryItem'] })).toEqual({
      domain: 'products',
      execution: 'overlay-read',
      operationName: 'inventoryItem',
      type: 'query',
    });
  });
});

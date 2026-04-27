import { type SelectionNode } from 'graphql';

import { getRootFields } from '../graphql/root-field.js';
import { getDocumentFragments, getFieldResponseKey, projectGraphqlObject } from './graphql-helpers.js';

const DEFAULT_DELIVERY_SETTINGS: Record<string, unknown> = {
  __typename: 'DeliverySetting',
  legacyModeProfiles: false,
  legacyModeBlocked: {
    __typename: 'DeliveryLegacyModeBlocked',
    blocked: false,
    reasons: null,
  },
};

const DEFAULT_DELIVERY_PROMISE_SETTINGS: Record<string, unknown> = {
  __typename: 'DeliveryPromiseSetting',
  deliveryDatesEnabled: false,
  processingTime: null,
};

function projectSettings(source: Record<string, unknown>, selections: readonly SelectionNode[], document: string) {
  return projectGraphqlObject(source, selections, getDocumentFragments(document));
}

export function handleDeliverySettingsQuery(document: string): Record<string, unknown> {
  const data: Record<string, unknown> = {};

  for (const field of getRootFields(document)) {
    const key = getFieldResponseKey(field);
    switch (field.name.value) {
      case 'deliverySettings':
        data[key] = projectSettings(DEFAULT_DELIVERY_SETTINGS, field.selectionSet?.selections ?? [], document);
        break;
      case 'deliveryPromiseSettings':
        data[key] = projectSettings(DEFAULT_DELIVERY_PROMISE_SETTINGS, field.selectionSet?.selections ?? [], document);
        break;
      default:
        data[key] = null;
        break;
    }
  }

  return { data };
}

/* oxlint-disable no-console -- CLI scripts intentionally write capture status to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type CaptureResult = {
  status: number;
  payload: unknown;
};

type CaptureEntry = {
  query: string;
  variables: Record<string, unknown>;
  result: CaptureResult;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'shipping-fulfillments');
const outputPath = path.join(outputDir, 'shipping-settings-package-pickup-constraints.json');

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function runGraphqlCapture(query: string, variables: Record<string, unknown> = {}): Promise<CaptureEntry> {
  const result: ConformanceGraphqlResult = await runGraphqlRequest(query, variables);
  return {
    query,
    variables,
    result: {
      status: result.status,
      payload: result.payload,
    },
  };
}

function readRecord(value: unknown): Record<string, unknown> | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function readPath(value: unknown, pathParts: string[]): unknown {
  let current = value;
  for (const part of pathParts) {
    if (/^\d+$/u.test(part)) {
      current = Array.isArray(current) ? current[Number(part)] : undefined;
      continue;
    }
    current = readRecord(current)?.[part];
  }
  return current;
}

function readArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function readString(value: unknown): string | null {
  return typeof value === 'string' ? value : null;
}

function capturePayload(capture: CaptureEntry): unknown {
  return capture.result.payload;
}

const availabilityQuery = `#graphql
  query ShippingSettingsAvailability {
    availableCarrierServices {
      carrierService {
        id
        name
        formattedName
        active
        supportsServiceDiscovery
      }
    }
    locationsAvailableForDeliveryProfilesConnection(first: 3) {
      nodes {
        id
        name
        localPickupSettingsV2 {
          pickupTime
          instructions
        }
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
      }
    }
  }
`;

const localPickupReadQuery = `#graphql
  query ShippingLocalPickupRead($locationId: ID!) {
    location(id: $locationId) {
      id
      name
      localPickupSettingsV2 {
        pickupTime
        instructions
      }
    }
  }
`;

const localPickupEnableMutation = `#graphql
  mutation ShippingLocalPickupEnable($localPickupSettings: DeliveryLocationLocalPickupEnableInput!) {
    locationLocalPickupEnable(localPickupSettings: $localPickupSettings) {
      localPickupSettings {
        pickupTime
        instructions
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const localPickupDisableMutation = `#graphql
  mutation ShippingLocalPickupDisable($locationId: ID!) {
    locationLocalPickupDisable(locationId: $locationId) {
      locationId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const shippingPackageUpdateUnknownMutation = `#graphql
  mutation ShippingPackageUpdateUnknown($id: ID!, $shippingPackage: CustomShippingPackageInput!) {
    shippingPackageUpdate(id: $id, shippingPackage: $shippingPackage) {
      userErrors {
        field
        message
      }
    }
  }
`;

const shippingPackageMakeDefaultUnknownMutation = `#graphql
  mutation ShippingPackageMakeDefaultUnknown($id: ID!) {
    shippingPackageMakeDefault(id: $id) {
      userErrors {
        field
        message
      }
    }
  }
`;

const shippingPackageDeleteUnknownMutation = `#graphql
  mutation ShippingPackageDeleteUnknown($id: ID!) {
    shippingPackageDelete(id: $id) {
      deletedId
      userErrors {
        field
        message
      }
    }
  }
`;

const fulfillmentConstraintRulesQuery = `#graphql
  query FulfillmentConstraintRulesScopeBlocker {
    fulfillmentConstraintRules {
      id
    }
  }
`;

const fulfillmentConstraintRuleCreateMutation = `#graphql
  mutation FulfillmentConstraintRuleCreateScopeBlocker {
    fulfillmentConstraintRuleCreate(
      functionId: "gid://shopify/ShopifyFunction/0"
      deliveryMethodTypes: [SHIPPING]
    ) {
      fulfillmentConstraintRule {
        id
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const availability = await runGraphqlCapture(availabilityQuery);
const availabilityPayload = capturePayload(availability);
const locationNodes = readArray(
  readPath(availabilityPayload, ['data', 'locationsAvailableForDeliveryProfilesConnection', 'nodes']),
).filter((node): node is Record<string, unknown> => readRecord(node) !== null);
const targetLocationId =
  process.env['SHOPIFY_CONFORMANCE_PICKUP_LOCATION_ID'] ??
  readString(readRecord(locationNodes.at(-1))?.['id']) ??
  readString(readRecord(locationNodes[0])?.['id']);

if (!targetLocationId) {
  throw new Error('Cannot capture shipping local pickup without at least one delivery-profile location.');
}

const beforePickupRead = await runGraphqlCapture(localPickupReadQuery, { locationId: targetLocationId });
const beforePickupSettings = readRecord(
  readPath(capturePayload(beforePickupRead), ['data', 'location', 'localPickupSettingsV2']),
);
const enableVariables = {
  localPickupSettings: {
    locationId: targetLocationId,
    pickupTime: 'TWO_HOURS',
    instructions: 'HAR-320 parity pickup instructions',
  },
};
const enable = await runGraphqlCapture(localPickupEnableMutation, enableVariables);
const afterEnableRead = await runGraphqlCapture(localPickupReadQuery, { locationId: targetLocationId });
const disable = await runGraphqlCapture(localPickupDisableMutation, { locationId: targetLocationId });
const afterDisableRead = await runGraphqlCapture(localPickupReadQuery, { locationId: targetLocationId });

if (beforePickupSettings) {
  await runGraphqlCapture(localPickupEnableMutation, {
    localPickupSettings: {
      locationId: targetLocationId,
      pickupTime: beforePickupSettings['pickupTime'],
      instructions: beforePickupSettings['instructions'],
    },
  });
}

const unknownLocationVariables = {
  localPickupSettings: {
    locationId: 'gid://shopify/Location/999999999999',
    pickupTime: 'ONE_HOUR',
  },
};
const unknownLocationEnable = await runGraphqlCapture(localPickupEnableMutation, unknownLocationVariables);

const unknownShippingPackageId = 'gid://shopify/ShippingPackage/999999999999';
const unknownShippingPackageInput = {
  name: 'HAR-320 unknown package',
  type: 'BOX',
  default: false,
  weight: { value: 2.5, unit: 'POUNDS' },
  dimensions: { length: 12, width: 9, height: 5, unit: 'INCHES' },
};
const shippingPackageUpdateUnknown = await runGraphqlCapture(shippingPackageUpdateUnknownMutation, {
  id: unknownShippingPackageId,
  shippingPackage: unknownShippingPackageInput,
});
const shippingPackageMakeDefaultUnknown = await runGraphqlCapture(shippingPackageMakeDefaultUnknownMutation, {
  id: unknownShippingPackageId,
});
const shippingPackageDeleteUnknown = await runGraphqlCapture(shippingPackageDeleteUnknownMutation, {
  id: unknownShippingPackageId,
});

const fulfillmentConstraintReadBlocker = await runGraphqlCapture(fulfillmentConstraintRulesQuery);
const fulfillmentConstraintWriteBlocker = await runGraphqlCapture(fulfillmentConstraintRuleCreateMutation);

const seedCarrierServices = readArray(readPath(availabilityPayload, ['data', 'availableCarrierServices']))
  .map((entry) => readRecord(readRecord(entry)?.['carrierService']))
  .filter((service): service is Record<string, unknown> => service !== null);
const seedLocations = locationNodes.map((location) => ({
  ...location,
  isActive: true,
  isFulfillmentService: false,
  localPickupSettings: readRecord(location['localPickupSettingsV2']),
}));

const capture = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  captureType: 'shipping-settings-replayable-parity',
  rootShapes: {
    availableCarrierServices: 'returns [DeliveryCarrierServiceAndLocations!]!',
    locationsAvailableForDeliveryProfilesConnection: 'returns LocationConnection! with first/after/last/before/reverse',
    locationLocalPickupEnable: 'localPickupSettings: DeliveryLocationLocalPickupEnableInput!',
    locationLocalPickupDisable: 'locationId: ID!',
    shippingPackageUpdate: 'id: ID!, shippingPackage: CustomShippingPackageInput!',
    shippingPackageMakeDefault: 'id: ID!',
    shippingPackageDelete: 'id: ID!',
    fulfillmentConstraintRules: 'blocked by read_fulfillment_constraint_rules scope',
    fulfillmentConstraintRuleCreateUpdateDelete: 'blocked by write_fulfillment_constraint_rules scope',
  },
  fulfillmentConstraintScopeBlockers: {
    read: 'Access denied for fulfillmentConstraintRules field. Required access: `read_fulfillment_constraint_rules` access scope.',
    write:
      'Access denied for fulfillmentConstraintRuleCreate/Update/Delete fields. Required access: `write_fulfillment_constraint_rules` access scope.',
  },
  seed: {
    carrierServices: seedCarrierServices,
    locations: seedLocations,
  },
  captures: {
    availability,
    beforePickupRead,
    enable,
    afterEnableRead,
    disable,
    afterDisableRead,
    unknownLocationEnable,
    shippingPackageUpdateUnknown,
    shippingPackageMakeDefaultUnknown,
    shippingPackageDeleteUnknown,
    fulfillmentConstraintReadBlocker,
    fulfillmentConstraintWriteBlocker,
  },
};

await mkdir(outputDir, { recursive: true });
await writeFile(outputPath, `${JSON.stringify(capture, null, 2)}\n`, 'utf8');

console.log(JSON.stringify({ ok: true, outputPath, targetLocationId }, null, 2));

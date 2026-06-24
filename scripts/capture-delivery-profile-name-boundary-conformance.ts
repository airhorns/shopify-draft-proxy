/* oxlint-disable no-console -- CLI scripts intentionally write capture status to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type GraphqlCapture = {
  query: string;
  variables: JsonRecord;
  result: ConformanceGraphqlResult;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'shipping-fulfillments');
const outputPath = path.join(outputDir, 'delivery-profile-name-boundary.json');

const deliveryProfileCreateMutation = `#graphql
  mutation DeliveryProfileNameBoundaryCreate($profile: DeliveryProfileInput!) {
    deliveryProfileCreate(profile: $profile) {
      profile {
        id
        name
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const deliveryProfileUpdateMutation = `#graphql
  mutation DeliveryProfileNameBoundaryUpdate($id: ID!, $profile: DeliveryProfileInput!) {
    deliveryProfileUpdate(id: $id, profile: $profile) {
      profile {
        id
        name
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const deliveryProfileRemoveMutation = `#graphql
  mutation DeliveryProfileNameBoundaryCleanup($id: ID!) {
    deliveryProfileRemove(id: $id) {
      job {
        id
        done
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const locationsHydrateQuery = `#graphql
  query ShippingDeliveryProfileLocationsHydrate {
    locationsAvailableForDeliveryProfilesConnection(first: 2) {
      nodes {
        id
        name
        isActive
        isFulfillmentService
      }
    }
  }
`;

const codeSelectionProbeMutation = `#graphql
  mutation DeliveryProfileNameBoundaryCodeProbe($profile: DeliveryProfileInput!) {
    deliveryProfileCreate(profile: $profile) {
      profile {
        id
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

function trimGraphql(query: string): string {
  return query.replace(/^#graphql\n/u, '').trim();
}

async function capture(query: string, variables: JsonRecord): Promise<GraphqlCapture> {
  return {
    query: trimGraphql(query),
    variables,
    result: await runGraphqlRequest(query, variables),
  };
}

function fixedLengthName(prefix: string, length: number): string {
  const fillerLength = length - prefix.length;
  if (fillerLength < 0) {
    throw new Error(`Prefix ${JSON.stringify(prefix)} is longer than ${length} characters`);
  }
  const name = `${prefix}${'x'.repeat(fillerLength)}`;
  if ([...name].length !== length) {
    throw new Error(`Expected ${JSON.stringify(name)} to have length ${length}`);
  }
  return name;
}

function readObject(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readPath(value: unknown, pathParts: string[]): unknown {
  let current = value;
  for (const part of pathParts) {
    const object = readObject(current);
    current = object?.[part];
  }
  return current;
}

function assertHttpOk(label: string, result: ConformanceGraphqlResult): void {
  if (result.status < 200 || result.status >= 300) {
    throw new Error(`${label} expected HTTP 2xx, got ${result.status}: ${JSON.stringify(result.payload)}`);
  }
}

function readUserErrors(captureResult: GraphqlCapture, root: string): JsonRecord[] {
  const errors = readPath(captureResult.result.payload, ['data', root, 'userErrors']);
  return Array.isArray(errors) ? errors.filter((error): error is JsonRecord => readObject(error) !== null) : [];
}

function readProfile(captureResult: GraphqlCapture, root: string): JsonRecord | null {
  return readObject(readPath(captureResult.result.payload, ['data', root, 'profile']));
}

function assertAcceptedName(captureResult: GraphqlCapture, root: string, label: string, expectedName: string): string {
  assertHttpOk(label, captureResult.result);
  const profile = readProfile(captureResult, root);
  const errors = readUserErrors(captureResult, root);
  const id = profile?.['id'];
  if (profile?.['name'] !== expectedName || typeof id !== 'string' || errors.length !== 0) {
    throw new Error(
      `${label} expected profile name ${JSON.stringify(expectedName)} and no userErrors, got ${JSON.stringify(
        captureResult.result.payload,
      )}`,
    );
  }
  return id;
}

function assertTooLongName(captureResult: GraphqlCapture, root: string, label: string): string {
  assertHttpOk(label, captureResult.result);
  const profile = readPath(captureResult.result.payload, ['data', root, 'profile']);
  const errors = readUserErrors(captureResult, root);
  const first = errors[0];
  const field = first?.['field'];
  const message = first?.['message'];
  if (
    profile !== null ||
    errors.length !== 1 ||
    JSON.stringify(field) !== JSON.stringify(['profile', 'name']) ||
    typeof message !== 'string' ||
    message.length === 0
  ) {
    throw new Error(
      `${label} expected null profile and one profile.name userError, got ${JSON.stringify(
        captureResult.result.payload,
      )}`,
    );
  }
  return message;
}

function firstUsableLocationId(hydrate: ConformanceGraphqlResult): string {
  assertHttpOk('locations hydrate', hydrate);
  const nodes = readPath(hydrate.payload, ['data', 'locationsAvailableForDeliveryProfilesConnection', 'nodes']);
  if (!Array.isArray(nodes)) {
    throw new Error(`locations hydrate expected nodes array, got ${JSON.stringify(hydrate.payload)}`);
  }
  for (const node of nodes) {
    const location = readObject(node);
    if (
      typeof location?.['id'] === 'string' &&
      location['isActive'] === true &&
      location['isFulfillmentService'] === false
    ) {
      return location['id'];
    }
  }
  throw new Error(`locations hydrate did not include an active merchant location: ${JSON.stringify(hydrate.payload)}`);
}

function profileInput(name: string, locationId: string): JsonRecord {
  return {
    name,
    locationGroupsToCreate: [
      {
        locations: [locationId],
        zonesToCreate: [
          {
            name: 'Domestic',
            countries: [
              {
                code: 'US',
                includeAllProvinces: true,
              },
            ],
          },
        ],
      },
    ],
  };
}

const createAcceptedName = fixedLengthName('Boundary create ', 128);
const updateAcceptedName = fixedLengthName('Boundary update ', 128);
const rejectedName = fixedLengthName('Boundary reject ', 129);

const locationsHydrate = await runGraphqlRequest(locationsHydrateQuery, {});
const locationId = firstUsableLocationId(locationsHydrate);

const createAccepted = await capture(deliveryProfileCreateMutation, {
  profile: profileInput(createAcceptedName, locationId),
});
const profileId = assertAcceptedName(
  createAccepted,
  'deliveryProfileCreate',
  '128-character create',
  createAcceptedName,
);

let cleanup: GraphqlCapture | undefined;
try {
  const updateAccepted = await capture(deliveryProfileUpdateMutation, {
    id: profileId,
    profile: {
      name: updateAcceptedName,
    },
  });
  assertAcceptedName(updateAccepted, 'deliveryProfileUpdate', '128-character update', updateAcceptedName);

  const updateRejected = await capture(deliveryProfileUpdateMutation, {
    id: profileId,
    profile: {
      name: rejectedName,
    },
  });
  const updateRejectedMessage = assertTooLongName(updateRejected, 'deliveryProfileUpdate', '129-character update');

  const createRejected = await capture(deliveryProfileCreateMutation, {
    profile: profileInput(rejectedName, locationId),
  });
  const createRejectedMessage = assertTooLongName(createRejected, 'deliveryProfileCreate', '129-character create');

  if (createRejectedMessage !== updateRejectedMessage) {
    throw new Error(
      `Expected create/update too-long messages to match, got ${JSON.stringify({
        createRejectedMessage,
        updateRejectedMessage,
      })}`,
    );
  }

  const codeSelectionProbe = await capture(codeSelectionProbeMutation, {
    profile: {
      name: rejectedName,
    },
  });

  cleanup = await capture(deliveryProfileRemoveMutation, { id: profileId });

  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        evidence: {
          locationUsed: locationId,
          maximumNameLength: 128,
          rejectedNameLength: 129,
          tooLongMessage: createRejectedMessage,
          deliveryProfileUserErrorsType: 'UserError',
          userErrorFields: ['field', 'message'],
          codeSelectionProbe,
        },
        mutations: {
          createAccepted,
          updateAccepted,
          updateRejected,
          createRejected,
        },
        cleanup: {
          removeCreatedProfile: cleanup,
        },
        notes: [
          'Captured with SHOPIFY_CONFORMANCE_API_VERSION=2026-04 and home-folder conformance auth.',
          'The capture creates one disposable delivery profile with a 128-character name, updates it to another 128-character name, records 129-character create/update validation failures, then removes the profile in cleanup.',
          'Admin GraphQL 2026-04 exposes deliveryProfileCreate and deliveryProfileUpdate userErrors as generic UserError with field/message only; the code selection probe records that public schema behavior.',
        ],
        upstreamCalls: [
          {
            operationName: 'ShippingDeliveryProfileLocationsHydrate',
            variables: {},
            query: 'sha:hand-synthesized',
            response: {
              status: locationsHydrate.status,
              body: locationsHydrate.payload,
            },
          },
        ],
      },
      null,
      2,
    )}\n`,
    'utf8',
  );

  console.log(JSON.stringify({ ok: true, outputPath }, null, 2));
} finally {
  if (cleanup === undefined) {
    const cleanupAfterFailure = await capture(deliveryProfileRemoveMutation, { id: profileId });
    console.log(JSON.stringify({ ok: false, cleanupAfterFailure }, null, 2));
  }
}

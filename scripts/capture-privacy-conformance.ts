/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const captureMutations = process.env['SHOPIFY_CONFORMANCE_CAPTURE_PRIVACY_MUTATIONS'] === 'true';
const consentPoliciesEnv = 'SHOPIFY_CONFORMANCE_PRIVACY_CONSENT_POLICIES_JSON';
const featuresToDisableEnv = 'SHOPIFY_CONFORMANCE_PRIVACY_FEATURES_TO_DISABLE_JSON';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion);
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

function assertNoTopLevelErrors(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function readJsonArrayEnv(name: string): unknown[] {
  const rawValue = process.env[name];
  if (!rawValue) {
    throw new Error(`Missing ${name}; required when ${captureMutations ? 'mutation capture is enabled' : 'needed'}`);
  }

  const parsed = JSON.parse(rawValue) as unknown;
  if (!Array.isArray(parsed)) {
    throw new Error(`${name} must be a JSON array`);
  }

  return parsed;
}

const privacySettingsQuery = `#graphql
  query PrivacySettingsCapture {
    privacySettings {
      __typename
      banner {
        __typename
      }
      dataSaleOptOutPage {
        __typename
      }
      privacyPolicy {
        __typename
      }
    }
  }
`;

const consentPolicyQuery = `#graphql
  query ConsentPolicyCapture {
    consentPolicy {
      id
      shopId
      countryCode
      regionCode
      consentRequired
      dataSaleOptOutRequired
    }
  }
`;

const consentPolicyRegionsQuery = `#graphql
  query ConsentPolicyRegionsCapture {
    consentPolicyRegions {
      countryCode
      regionCode
    }
  }
`;

const consentPolicyUpdateMutation = `#graphql
  mutation ConsentPolicyUpdateCapture($consentPolicies: [ConsentPolicyInput!]!) {
    consentPolicyUpdate(consentPolicies: $consentPolicies) {
      updatedPolicies {
        id
        shopId
        countryCode
        regionCode
        consentRequired
        dataSaleOptOutRequired
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const privacyFeaturesDisableMutation = `#graphql
  mutation PrivacyFeaturesDisableCapture($featuresToDisable: [PrivacyFeaturesEnum!]!) {
    privacyFeaturesDisable(featuresToDisable: $featuresToDisable) {
      featuresDisabled
      userErrors {
        field
        message
        code
      }
    }
  }
`;

await mkdir(outputDir, { recursive: true });

const privacySettings = await runGraphqlRequest(privacySettingsQuery);
assertNoTopLevelErrors(privacySettings, 'privacySettings read');

const consentPolicy = await runGraphqlRequest(consentPolicyQuery);
assertNoTopLevelErrors(consentPolicy, 'consentPolicy read');

const consentPolicyRegions = await runGraphqlRequest(consentPolicyRegionsQuery);
assertNoTopLevelErrors(consentPolicyRegions, 'consentPolicyRegions read');

const fixture: Record<string, unknown> = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  reads: {
    privacySettings: {
      operationName: 'PrivacySettingsCapture',
      query: privacySettingsQuery,
      variables: {},
      response: privacySettings.payload,
    },
    consentPolicy: {
      operationName: 'ConsentPolicyCapture',
      query: consentPolicyQuery,
      variables: {},
      response: consentPolicy.payload,
    },
    consentPolicyRegions: {
      operationName: 'ConsentPolicyRegionsCapture',
      query: consentPolicyRegionsQuery,
      variables: {},
      response: consentPolicyRegions.payload,
    },
  },
  mutationCapture: captureMutations ? 'enabled' : 'skipped',
};

if (captureMutations) {
  const consentPolicyUpdateVariables = {
    consentPolicies: readJsonArrayEnv(consentPoliciesEnv),
  };
  const privacyFeaturesDisableVariables = {
    featuresToDisable: readJsonArrayEnv(featuresToDisableEnv),
  };

  const consentPolicyUpdate = await runGraphqlRequest(consentPolicyUpdateMutation, consentPolicyUpdateVariables);
  assertNoTopLevelErrors(consentPolicyUpdate, 'consentPolicyUpdate mutation');

  const privacyFeaturesDisable = await runGraphqlRequest(
    privacyFeaturesDisableMutation,
    privacyFeaturesDisableVariables,
  );
  assertNoTopLevelErrors(privacyFeaturesDisable, 'privacyFeaturesDisable mutation');

  fixture['mutations'] = {
    consentPolicyUpdate: {
      operationName: 'ConsentPolicyUpdateCapture',
      query: consentPolicyUpdateMutation,
      variables: consentPolicyUpdateVariables,
      response: consentPolicyUpdate.payload,
    },
    privacyFeaturesDisable: {
      operationName: 'PrivacyFeaturesDisableCapture',
      query: privacyFeaturesDisableMutation,
      variables: privacyFeaturesDisableVariables,
      response: privacyFeaturesDisable.payload,
    },
  };
} else {
  fixture['skippedMutations'] = {
    reason:
      'Set SHOPIFY_CONFORMANCE_CAPTURE_PRIVACY_MUTATIONS=true with explicit JSON inputs to capture side-effecting privacy mutations.',
    requiredEnv: [consentPoliciesEnv, featuresToDisableEnv],
  };
}

const outputPath = path.join(outputDir, 'privacy-conformance.json');
await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`);
console.log(`Wrote ${outputPath}`);

/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'store-properties');
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

function readObject(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as Record<string, unknown>) : null;
}

function readPolicies(result: ConformanceGraphqlResult): Array<Record<string, unknown>> {
  const payload = readObject(result.payload);
  const data = readObject(payload?.['data']);
  const shop = readObject(data?.['shop']);
  const policies = shop?.['shopPolicies'];
  return Array.isArray(policies)
    ? policies.filter((policy): policy is Record<string, unknown> => !!readObject(policy))
    : [];
}

function readContactPolicyBody(result: ConformanceGraphqlResult): string {
  const contactPolicy = readPolicies(result).find((policy) => policy['type'] === 'CONTACT_INFORMATION');
  return typeof contactPolicy?.['body'] === 'string' ? contactPolicy['body'] : '<p></p>';
}

function readPolicyBody(result: ConformanceGraphqlResult, type: string): string | undefined {
  const policy = readPolicies(result).find((candidate) => candidate['type'] === type);
  return typeof policy?.['body'] === 'string' ? policy['body'] : undefined;
}

function readMutationPolicyId(result: ConformanceGraphqlResult, context: string): string {
  const payload = readObject(result.payload);
  const data = readObject(payload?.['data']);
  const update = readObject(data?.['shopPolicyUpdate']);
  const policy = readObject(update?.['shopPolicy']);
  const id = policy?.['id'];
  if (typeof id !== 'string') {
    throw new Error(`${context} did not return a shop policy id: ${JSON.stringify(result, null, 2)}`);
  }
  return id;
}

function withoutPolicyTranslations(payload: unknown): unknown {
  const clone = JSON.parse(JSON.stringify(payload)) as unknown;
  const root = readObject(clone);
  const data = readObject(root?.['data']);
  const shop = readObject(data?.['shop']);
  const policies = shop?.['shopPolicies'];
  if (Array.isArray(policies)) {
    for (const policy of policies) {
      const policyObject = readObject(policy);
      if (policyObject) {
        delete policyObject['translations'];
      }
    }
  }
  return clone;
}

const shopPolicyCoreFields = `
  id
  title
  body
  type
  url
  createdAt
  updatedAt
`;

const shopPolicyFields = `
  ${shopPolicyCoreFields}
  translations(locale: "fr") {
    key
    locale
    outdated
    updatedAt
    value
    market {
      id
    }
  }
`;

const shopBaselineQuery = `#graphql
  query StorePropertiesShopPolicyBaseline {
    shop {
      id
      name
      myshopifyDomain
      url
      primaryDomain {
        id
        host
        url
        sslEnabled
      }
      contactEmail
      email
      currencyCode
      enabledPresentmentCurrencies
      ianaTimezone
      timezoneAbbreviation
      timezoneOffset
      timezoneOffsetMinutes
      taxesIncluded
      taxShipping
      unitSystem
      weightUnit
      shopAddress {
        id
        address1
        address2
        city
        company
        coordinatesValidated
        country
        countryCodeV2
        formatted
        formattedArea
        latitude
        longitude
        phone
        province
        provinceCode
        zip
      }
      plan {
        partnerDevelopment
        publicDisplayName
        shopifyPlus
      }
      resourceLimits {
        locationLimit
        maxProductOptions
        maxProductVariants
        redirectLimitReached
      }
      features {
        avalaraAvatax
        branding
        bundles {
          eligibleForBundles
          ineligibilityReason
          sellsBundles
        }
        captcha
        cartTransform {
          eligibleOperations {
            expandOperation
            mergeOperation
            updateOperation
          }
        }
        dynamicRemarketing
        eligibleForSubscriptionMigration
        eligibleForSubscriptions
        giftCards
        harmonizedSystemCode
        legacySubscriptionGatewayEnabled
        liveView
        paypalExpressSubscriptionGatewayStatus
        reports
        sellsSubscriptions
        showMetrics
        storefront
        unifiedMarkets
      }
      paymentSettings {
        supportedDigitalWallets
      }
      shopPolicies {
        ${shopPolicyFields}
      }
    }
  }
`;

const shopPolicyUpdateMutation = `#graphql
  mutation ShopPolicyUpdate($shopPolicy: ShopPolicyInput!) {
    shopPolicyUpdate(shopPolicy: $shopPolicy) {
      shopPolicy {
        ${shopPolicyFields}
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const shopPolicyUpdateCoreMutation = `#graphql
  mutation ShopPolicyUpdate($shopPolicy: ShopPolicyInput!) {
    shopPolicyUpdate(shopPolicy: $shopPolicy) {
      shopPolicy {
        ${shopPolicyCoreFields}
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const downstreamReadQuery = `#graphql
  query ShopPolicyDownstreamRead {
    shop {
      id
      myshopifyDomain
      shopPolicies {
        ${shopPolicyFields}
      }
    }
  }
`;

const shopPolicyNodeReadQuery = `#graphql
  query ShopPolicyNodeRead($id: ID!) {
    node(id: $id) {
      __typename
      ... on ShopPolicy {
        id
        title
        body
        type
        url
        createdAt
        updatedAt
      }
    }
  }
`;

const before = await runGraphqlRequest(shopBaselineQuery);
assertNoTopLevelErrors(before, 'baseline shop policy read');
const baselineHydratePayload = withoutPolicyTranslations(before.payload);

const originalContactBody = readContactPolicyBody(before);
const originalPrivacyBody = readPolicyBody(before, 'PRIVACY_POLICY') ?? '<p></p>';
const originalRefundBody = readPolicyBody(before, 'REFUND_POLICY') ?? '<p></p>';
const marker = `HAR-173 conformance shopPolicyUpdate capture ${new Date().toISOString()}`;
const mutationVariables = {
  shopPolicy: {
    type: 'CONTACT_INFORMATION',
    body: `<p>${marker}</p>`,
  },
};
const validationVariables = {
  shopPolicy: {
    type: 'CONTACT_INFORMATION',
    body: 'x'.repeat(512 * 1024 + 1),
  },
};
const cleanupVariables = {
  shopPolicy: {
    type: 'CONTACT_INFORMATION',
    body: originalContactBody,
  },
};

const validation = await runGraphqlRequest(shopPolicyUpdateMutation, validationVariables);
assertNoTopLevelErrors(validation, 'oversized body shopPolicyUpdate validation');

const mutation = await runGraphqlRequest(shopPolicyUpdateMutation, mutationVariables);
assertNoTopLevelErrors(mutation, 'shopPolicyUpdate mutation');

const downstreamRead = await runGraphqlRequest(downstreamReadQuery);
assertNoTopLevelErrors(downstreamRead, 'downstream shop policy read');

const cleanup = await runGraphqlRequest(shopPolicyUpdateMutation, cleanupVariables);
assertNoTopLevelErrors(cleanup, 'shopPolicyUpdate cleanup');

const finalRead = await runGraphqlRequest(downstreamReadQuery);
assertNoTopLevelErrors(finalRead, 'final shop policy read');

const privacyVariables = {
  shopPolicy: {
    type: 'PRIVACY_POLICY',
    body: 'Line one\nLine two',
  },
};
const refundVariables = {
  shopPolicy: {
    type: 'REFUND_POLICY',
    body: '<p>refund text</p>',
  },
};
const maxBodyVariables = {
  shopPolicy: {
    type: 'PRIVACY_POLICY',
    body: 'x'.repeat(524_287),
  },
};
const tooBigVariables = {
  shopPolicy: {
    type: 'PRIVACY_POLICY',
    body: 'x'.repeat(524_288),
  },
};

const privacyMutation = await runGraphqlRequest(shopPolicyUpdateCoreMutation, privacyVariables);
assertNoTopLevelErrors(privacyMutation, 'privacy policy shopPolicyUpdate mutation');
const privacyNodeRead = await runGraphqlRequest(shopPolicyNodeReadQuery, {
  id: readMutationPolicyId(privacyMutation, 'privacy policy shopPolicyUpdate mutation'),
});
assertNoTopLevelErrors(privacyNodeRead, 'privacy policy node read');

const refundMutation = await runGraphqlRequest(shopPolicyUpdateCoreMutation, refundVariables);
assertNoTopLevelErrors(refundMutation, 'refund policy shopPolicyUpdate mutation');
const refundNodeRead = await runGraphqlRequest(shopPolicyNodeReadQuery, {
  id: readMutationPolicyId(refundMutation, 'refund policy shopPolicyUpdate mutation'),
});
assertNoTopLevelErrors(refundNodeRead, 'refund policy node read');

const maxBodyMutation = await runGraphqlRequest(shopPolicyUpdateCoreMutation, maxBodyVariables);
assertNoTopLevelErrors(maxBodyMutation, 'maximum body shopPolicyUpdate mutation');

const tooBigValidation = await runGraphqlRequest(shopPolicyUpdateCoreMutation, tooBigVariables);
assertNoTopLevelErrors(tooBigValidation, 'too-big body shopPolicyUpdate validation');

const privacyCleanup = await runGraphqlRequest(shopPolicyUpdateMutation, {
  shopPolicy: {
    type: 'PRIVACY_POLICY',
    body: originalPrivacyBody,
  },
});
assertNoTopLevelErrors(privacyCleanup, 'privacy policy cleanup');
const refundCleanup = await runGraphqlRequest(shopPolicyUpdateMutation, {
  shopPolicy: {
    type: 'REFUND_POLICY',
    body: originalRefundBody,
  },
});
assertNoTopLevelErrors(refundCleanup, 'refund policy cleanup');

const fixture = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  readOnlyBaselines: {
    shop: baselineHydratePayload,
  },
  validation: {
    operationName: 'ShopPolicyUpdate',
    query: shopPolicyUpdateMutation,
    variables: validationVariables,
    response: validation.payload,
  },
  mutation: {
    operationName: 'ShopPolicyUpdate',
    query: shopPolicyUpdateMutation,
    variables: mutationVariables,
    response: mutation.payload,
  },
  downstreamRead: {
    query: downstreamReadQuery,
    variables: {},
    response: downstreamRead.payload,
  },
  cleanup: {
    operationName: 'ShopPolicyUpdate',
    query: shopPolicyUpdateMutation,
    variables: cleanupVariables,
    response: cleanup.payload,
  },
  finalRead: {
    query: downstreamReadQuery,
    variables: {},
    response: finalRead.payload,
  },
};

const titleUrlBodyFixture = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  readOnlyBaselines: {
    shop: baselineHydratePayload,
  },
  privacyMutation: {
    operationName: 'ShopPolicyUpdate',
    query: shopPolicyUpdateCoreMutation,
    variables: privacyVariables,
    response: privacyMutation.payload,
  },
  privacyNodeRead: {
    operationName: 'ShopPolicyNodeRead',
    query: shopPolicyNodeReadQuery,
    variables: { id: readMutationPolicyId(privacyMutation, 'privacy policy shopPolicyUpdate mutation') },
    response: privacyNodeRead.payload,
  },
  refundMutation: {
    operationName: 'ShopPolicyUpdate',
    query: shopPolicyUpdateCoreMutation,
    variables: refundVariables,
    response: refundMutation.payload,
  },
  refundNodeRead: {
    operationName: 'ShopPolicyNodeRead',
    query: shopPolicyNodeReadQuery,
    variables: { id: readMutationPolicyId(refundMutation, 'refund policy shopPolicyUpdate mutation') },
    response: refundNodeRead.payload,
  },
  maxBodyMutation: {
    operationName: 'ShopPolicyUpdate',
    query: shopPolicyUpdateCoreMutation,
    variables: maxBodyVariables,
    response: maxBodyMutation.payload,
  },
  tooBigValidation: {
    operationName: 'ShopPolicyUpdate',
    query: shopPolicyUpdateCoreMutation,
    variables: tooBigVariables,
    response: tooBigValidation.payload,
  },
  cleanup: {
    privacy: privacyCleanup.payload,
    refund: refundCleanup.payload,
  },
  upstreamCalls: [
    {
      operationName: 'StorePropertiesShopBaselineHydrate',
      variables: {},
      query: 'sha:hand-synthesized-StorePropertiesShopBaselineHydrate',
      response: {
        status: 200,
        body: baselineHydratePayload,
      },
    },
  ],
};

await mkdir(outputDir, { recursive: true });
const outputPath = path.join(outputDir, 'shop-policy-update-parity.json');
await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
console.log(`Wrote ${outputPath}`);
const titleUrlBodyOutputPath = path.join(outputDir, 'shop-policy-update-title-url-and-body-rendering.json');
await writeFile(titleUrlBodyOutputPath, `${JSON.stringify(titleUrlBodyFixture, null, 2)}\n`, 'utf8');
console.log(`Wrote ${titleUrlBodyOutputPath}`);

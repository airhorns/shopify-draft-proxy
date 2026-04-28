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

const shopPolicyFields = `
  id
  title
  body
  type
  url
  createdAt
  updatedAt
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

const before = await runGraphqlRequest(shopBaselineQuery);
assertNoTopLevelErrors(before, 'baseline shop policy read');

const originalContactBody = readContactPolicyBody(before);
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

const fixture = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  readOnlyBaselines: {
    shop: before.payload,
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

await mkdir(outputDir, { recursive: true });
const outputPath = path.join(outputDir, 'shop-policy-update-parity.json');
await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
console.log(`Wrote ${outputPath}`);

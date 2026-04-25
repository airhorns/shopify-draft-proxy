// @ts-nocheck
/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { extractManualStoreAuthTokenSummary } from './product-publication-conformance-lib.mjs';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest: runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const orderCreationBlockerNotePath = path.join('pending', 'order-creation-conformance-scope-blocker.md');
const orderEditingBlockerNotePath = path.join('pending', 'order-editing-conformance-scope-blocker.md');
const draftOrderReadBlockerNotePath = path.join('pending', 'draft-order-read-conformance-scope-blocker.md');
const fulfillmentLifecycleBlockerNotePath = path.join('pending', 'fulfillment-lifecycle-conformance-scope-blocker.md');
const manualStoreAuthTokenPath = '.manual-store-auth-token.json';

const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion);
const orderEmptyStateFixturePath = path.join(fixtureDir, 'order-empty-state.json');
const orderUpdateUnknownIdFixturePath = path.join(fixtureDir, 'order-update-unknown-id.json');
const orderUpdateMissingIdFixturePath = path.join(fixtureDir, 'order-update-missing-id.json');
const orderUpdateInlineMissingIdFixturePath = path.join(fixtureDir, 'order-update-inline-missing-id.json');
const orderUpdateInlineNullIdFixturePath = path.join(fixtureDir, 'order-update-inline-null-id.json');
const orderUpdateLiveParityFixturePath = path.join(fixtureDir, 'order-update-parity.json');
const fulfillmentCreateInvalidIdFixturePath = path.join(fixtureDir, 'fulfillment-create-invalid-id.json');
const fulfillmentTrackingInfoUpdateInlineMissingIdFixturePath = path.join(
  fixtureDir,
  'fulfillment-tracking-info-update-inline-missing-id.json',
);
const fulfillmentTrackingInfoUpdateInlineNullIdFixturePath = path.join(
  fixtureDir,
  'fulfillment-tracking-info-update-inline-null-id.json',
);
const fulfillmentTrackingInfoUpdateMissingIdFixturePath = path.join(
  fixtureDir,
  'fulfillment-tracking-info-update-missing-id.json',
);
const fulfillmentCancelInlineMissingIdFixturePath = path.join(fixtureDir, 'fulfillment-cancel-inline-missing-id.json');
const fulfillmentCancelInlineNullIdFixturePath = path.join(fixtureDir, 'fulfillment-cancel-inline-null-id.json');
const fulfillmentCancelMissingIdFixturePath = path.join(fixtureDir, 'fulfillment-cancel-missing-id.json');
const fulfillmentTrackingInfoUpdateParityFixturePath = path.join(
  fixtureDir,
  'fulfillment-tracking-info-update-parity.json',
);
const fulfillmentCancelParityFixturePath = path.join(fixtureDir, 'fulfillment-cancel-parity.json');
const orderCreateInlineMissingOrderFixturePath = path.join(fixtureDir, 'order-create-inline-missing-order.json');
const orderCreateInlineNullOrderFixturePath = path.join(fixtureDir, 'order-create-inline-null-order.json');
const orderCreateMissingOrderFixturePath = path.join(fixtureDir, 'order-create-missing-order.json');
const orderCreateParityFixturePath = path.join(fixtureDir, 'order-create-parity.json');
const draftOrderCreateInlineMissingInputFixturePath = path.join(
  fixtureDir,
  'draft-order-create-inline-missing-input.json',
);
const draftOrderCreateInlineNullInputFixturePath = path.join(fixtureDir, 'draft-order-create-inline-null-input.json');
const draftOrderCreateMissingInputFixturePath = path.join(fixtureDir, 'draft-order-create-missing-input.json');
const draftOrderCreateParityFixturePath = path.join(fixtureDir, 'draft-order-create-parity.json');
const draftOrderDetailFixturePath = path.join(fixtureDir, 'draft-order-detail.json');
const draftOrdersCatalogFixturePath = path.join(fixtureDir, 'draft-orders-catalog.json');
const draftOrdersCountFixturePath = path.join(fixtureDir, 'draft-orders-count.json');
const draftOrdersInvalidEmailQueryFixturePath = path.join(fixtureDir, 'draft-orders-invalid-email-query.json');
const draftOrderCompleteInlineMissingIdFixturePath = path.join(
  fixtureDir,
  'draft-order-complete-inline-missing-id.json',
);
const draftOrderCompleteInlineNullIdFixturePath = path.join(fixtureDir, 'draft-order-complete-inline-null-id.json');
const draftOrderCompleteMissingIdFixturePath = path.join(fixtureDir, 'draft-order-complete-missing-id.json');
const draftOrderCompleteParityFixturePath = path.join(fixtureDir, 'draft-order-complete-parity.json');
const orderEditBeginMissingIdFixturePath = path.join(fixtureDir, 'order-edit-begin-missing-id.json');
const orderEditAddVariantMissingIdFixturePath = path.join(fixtureDir, 'order-edit-add-variant-missing-id.json');
const orderEditSetQuantityMissingIdFixturePath = path.join(fixtureDir, 'order-edit-set-quantity-missing-id.json');
const orderEditCommitMissingIdFixturePath = path.join(fixtureDir, 'order-edit-commit-missing-id.json');

function summarizeCredential(token) {
  if (/^shpca_/i.test(token)) {
    return {
      family: 'shpca',
      headerMode: 'raw-x-shopify-access-token',
      summary:
        'the active conformance credential is a Shopify user access token (`shpca_...`) sent as raw `X-Shopify-Access-Token` on this host',
    };
  }

  if (/^shp[a-z]+_/i.test(token)) {
    return {
      family: 'shopify-app-token',
      headerMode: 'raw-x-shopify-access-token',
      summary:
        'the active conformance credential is a Shopify app/user token in the `^shp[a-z]+_` family sent as raw `X-Shopify-Access-Token` on this host',
    };
  }

  return {
    family: 'bearer-token',
    headerMode: 'authorization-bearer',
    summary:
      "the active conformance credential is being sent through the repo's bearer-token fallback path on this host",
  };
}

async function readJsonIfExists(filePath) {
  try {
    return JSON.parse(await readFile(filePath, 'utf8'));
  } catch (error) {
    if (error?.code === 'ENOENT') {
      return null;
    }
    throw error;
  }
}

async function readManualStoreAuthSummary() {
  try {
    const payload = JSON.parse(await readFile(manualStoreAuthTokenPath, 'utf8'));
    const summary = extractManualStoreAuthTokenSummary(payload);
    if (!summary) {
      return {
        status: 'missing-access-token',
        tokenPath: manualStoreAuthTokenPath,
        tokenFamily: 'unknown',
        cachedScopes: [],
        associatedUserScopes: [],
      };
    }

    const tokenFamily = summary.tokenFamily || 'unknown';
    const status = tokenFamily === 'shpca' ? 'present-shpca-user-token-not-offline-capable' : 'present-non-shpca-token';

    return {
      status,
      tokenPath: manualStoreAuthTokenPath,
      tokenFamily,
      cachedScopes: summary.scopeHandles,
      associatedUserScopes: summary.associatedUserScopeHandles,
    };
  } catch (error) {
    if (error?.code === 'ENOENT') {
      return {
        status: 'missing',
        tokenPath: manualStoreAuthTokenPath,
        tokenFamily: 'missing',
        cachedScopes: [],
        associatedUserScopes: [],
      };
    }
    throw error;
  }
}

function renderManualStoreAuthSection(manualStoreAuthSummary) {
  const cachedScopes =
    manualStoreAuthSummary.cachedScopes.length > 0
      ? manualStoreAuthSummary.cachedScopes.map((scope) => `\`${scope}\``).join(', ')
      : 'none recorded';
  const associatedUserScopes =
    manualStoreAuthSummary.associatedUserScopes.length > 0
      ? manualStoreAuthSummary.associatedUserScopes.map((scope) => `\`${scope}\``).join(', ')
      : 'none recorded';
  const interpretation =
    manualStoreAuthSummary.status === 'present-shpca-user-token-not-offline-capable'
      ? "The saved manual store-auth artifact still caches a `shpca` user token, so it does not satisfy Shopify's offline-token requirement for `orderCreate` even though its cached scope strings include order scopes."
      : manualStoreAuthSummary.status === 'present-non-shpca-token'
        ? 'A saved manual store-auth artifact exists on disk, but the active repo `.env` credential is still the source of truth for this run until a human intentionally switches conformance over.'
        : 'No saved manual store-auth artifact is currently available for this run.';

  return `## Saved manual store auth token on disk

- path: \`${manualStoreAuthSummary.tokenPath}\`
- status: \`${manualStoreAuthSummary.status}\`
- token family: \`${manualStoreAuthSummary.tokenFamily}\`
- cached scopes: ${cachedScopes}
- associated user scopes: ${associatedUserScopes}
- interpretation: ${interpretation}`;
}

async function readOrderCreationParityMetadata(credential, manualStoreAuthSummary) {
  const [
    orderCreateSpec,
    draftOrderCreateSpec,
    draftOrderCompleteSpec,
    draftOrdersSpec,
    draftOrdersCountSpec,
    orderEditBeginSpec,
    orderEditAddVariantSpec,
    orderEditSetQuantitySpec,
    orderEditCommitSpec,
    fulfillmentTrackingInfoUpdateSpec,
    fulfillmentCancelSpec,
  ] = await Promise.all([
    readJsonIfExists(path.join('config', 'parity-specs', 'orderCreate-parity-plan.json')),
    readJsonIfExists(path.join('config', 'parity-specs', 'draftOrderCreate-parity-plan.json')),
    refreshOrderDomainBlockerSpec(
      path.join('config', 'parity-specs', 'draftOrderComplete-parity-plan.json'),
      credential,
      manualStoreAuthSummary,
    ),
    refreshOrderDomainBlockerSpec(
      path.join('config', 'parity-specs', 'draftOrders-read-parity-plan.json'),
      credential,
      manualStoreAuthSummary,
    ),
    refreshOrderDomainBlockerSpec(
      path.join('config', 'parity-specs', 'draftOrdersCount-read-parity-plan.json'),
      credential,
      manualStoreAuthSummary,
    ),
    refreshOrderDomainBlockerSpec(
      path.join('config', 'parity-specs', 'orderEditBegin-parity-plan.json'),
      credential,
      manualStoreAuthSummary,
    ),
    refreshOrderDomainBlockerSpec(
      path.join('config', 'parity-specs', 'orderEditAddVariant-parity-plan.json'),
      credential,
      manualStoreAuthSummary,
    ),
    refreshOrderDomainBlockerSpec(
      path.join('config', 'parity-specs', 'orderEditSetQuantity-parity-plan.json'),
      credential,
      manualStoreAuthSummary,
    ),
    refreshOrderDomainBlockerSpec(
      path.join('config', 'parity-specs', 'orderEditCommit-parity-plan.json'),
      credential,
      manualStoreAuthSummary,
    ),
    refreshOrderDomainBlockerSpec(
      path.join('config', 'parity-specs', 'fulfillmentTrackingInfoUpdate-parity-plan.json'),
      credential,
      manualStoreAuthSummary,
    ),
    refreshOrderDomainBlockerSpec(
      path.join('config', 'parity-specs', 'fulfillmentCancel-parity-plan.json'),
      credential,
      manualStoreAuthSummary,
    ),
  ]);

  return {
    orderCreate: orderCreateSpec,
    draftOrderCreate: draftOrderCreateSpec,
    draftOrderComplete: draftOrderCompleteSpec,
    draftOrders: draftOrdersSpec,
    draftOrdersCount: draftOrdersCountSpec,
    orderEditBegin: orderEditBeginSpec,
    orderEditAddVariant: orderEditAddVariantSpec,
    orderEditSetQuantity: orderEditSetQuantitySpec,
    orderEditCommit: orderEditCommitSpec,
    fulfillmentTrackingInfoUpdate: fulfillmentTrackingInfoUpdateSpec,
    fulfillmentCancel: fulfillmentCancelSpec,
  };
}

async function writeJson(filePath, payload) {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${JSON.stringify(payload, null, 2)}\n`);
}

function readPayloadRoot(result, rootName) {
  return result?.payload?.data?.[rootName] ?? null;
}

function readUserErrors(result, rootName) {
  const userErrors = readPayloadRoot(result, rootName)?.userErrors;
  return Array.isArray(userErrors) ? userErrors : [];
}

function hasTopLevelErrors(result) {
  return Boolean(result?.payload?.errors);
}

function hasEmptyUserErrors(result, rootName) {
  return !hasTopLevelErrors(result) && readUserErrors(result, rootName).length === 0;
}

function findFulfillmentLifecycleCandidate(result) {
  const orders = result?.payload?.data?.orders?.nodes;
  if (!Array.isArray(orders)) {
    return null;
  }

  for (const order of orders) {
    if (order?.cancelledAt || order?.closed) {
      continue;
    }

    const fulfillmentOrders = order?.fulfillmentOrders?.nodes;
    if (!Array.isArray(fulfillmentOrders)) {
      continue;
    }

    for (const fulfillmentOrder of fulfillmentOrders) {
      const supportedActions = Array.isArray(fulfillmentOrder?.supportedActions)
        ? fulfillmentOrder.supportedActions
        : [];
      const canCreateFulfillment = supportedActions.some((action) => action?.action === 'CREATE_FULFILLMENT');
      const lineItems = fulfillmentOrder?.lineItems?.nodes;
      const lineItem = Array.isArray(lineItems)
        ? lineItems.find((candidateLineItem) => (candidateLineItem?.remainingQuantity ?? 0) > 0)
        : null;

      if (fulfillmentOrder?.status === 'OPEN' && canCreateFulfillment && lineItem) {
        return {
          order,
          fulfillmentOrder,
          lineItem,
        };
      }
    }
  }

  return null;
}

function mergeRuntimeBlockerDetails(existingDetails, credential, manualStoreAuthSummary) {
  return {
    ...existingDetails,
    activeCredentialTokenFamily: credential.family,
    activeCredentialHeaderMode: credential.headerMode,
    activeCredentialSummary: credential.summary,
    manualStoreAuthStatus: manualStoreAuthSummary.status,
    manualStoreAuthTokenPath: manualStoreAuthSummary.tokenPath,
    manualStoreAuthCachedScopes: manualStoreAuthSummary.cachedScopes,
    manualStoreAuthAssociatedUserScopes: manualStoreAuthSummary.associatedUserScopes,
  };
}

async function refreshOrderDomainBlockerSpec(specPath, credential, manualStoreAuthSummary) {
  const spec = await readJsonIfExists(specPath);
  if (!spec || typeof spec !== 'object' || !spec.blocker || typeof spec.blocker !== 'object') {
    return spec;
  }

  const nextSpec = {
    ...spec,
    blocker: {
      ...spec.blocker,
      details: mergeRuntimeBlockerDetails(spec.blocker.details, credential, manualStoreAuthSummary),
    },
  };

  await writeJson(specPath, nextSpec);
  return nextSpec;
}

function getAuthFailureMessage(result) {
  if (!result) {
    return null;
  }

  if (result.status === 401) {
    return result.payload?.errors?.[0]?.message ?? result.payload?.errors ?? '401 Unauthorized';
  }

  const errors = result.payload?.errors;
  if (
    typeof errors === 'string' &&
    /Invalid API key or access token|Service is not valid for authentication/i.test(errors)
  ) {
    return errors;
  }

  if (Array.isArray(errors)) {
    const authError = errors.find((error) =>
      /Invalid API key or access token|Service is not valid for authentication/i.test(error?.message ?? ''),
    );
    return authError?.message ?? null;
  }

  return null;
}

function formatRequiredAccessSummary(
  blockerDetails,
  liveError,
  defaultMessage = 'missing requiredScopes blocker metadata',
) {
  const requiredScopes = Array.isArray(blockerDetails?.requiredScopes)
    ? blockerDetails.requiredScopes.map((scope) => `\`${scope}\``).join(', ')
    : '';
  const requiredPermissions = Array.isArray(blockerDetails?.requiredPermissions)
    ? blockerDetails.requiredPermissions.map((permission) => `\`${permission}\``).join(', ')
    : '';

  if (requiredScopes) {
    return `${requiredScopes}${requiredPermissions ? `; required permissions: ${requiredPermissions}` : ''}`;
  }

  const liveRequiredAccess =
    typeof liveError?.extensions?.requiredAccess === 'string'
      ? liveError.extensions.requiredAccess
      : typeof liveError?.message === 'string'
        ? liveError.message
        : '';

  return liveRequiredAccess || defaultMessage;
}

const orderEmptyStateRead = `#graphql
  query OrderEmptyStateRead($missingOrderId: ID!, $first: Int!) {
    order(id: $missingOrderId) {
      id
      name
      createdAt
      updatedAt
      displayFinancialStatus
      displayFulfillmentStatus
      note
      tags
      currentTotalPriceSet { shopMoney { amount currencyCode } }
    }
    orders(first: $first, sortKey: CREATED_AT, reverse: true) {
      edges {
        cursor
        node {
          id
          name
        }
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
    ordersCount {
      count
      precision
    }
  }
`;

const orderUpdateUnknownIdProbe = `#graphql
  mutation OrderUpdateUnknownIdProbe($input: OrderInput!) {
    orderUpdate(input: $input) {
      order {
        id
        name
        updatedAt
        note
        tags
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const orderUpdateInlineMissingIdProbe = `#graphql
  mutation InlineMissingOrderId {
    orderUpdate(input: { note: "inline missing id", tags: ["inline", "missing-id"] }) {
      order { id }
      userErrors { field message }
    }
  }
`;

const orderUpdateInlineNullIdProbe = `#graphql
  mutation InlineNullOrderId {
    orderUpdate(input: { id: null, note: "inline null id", tags: ["inline", "null-id"] }) {
      order { id }
      userErrors { field message }
    }
  }
`;

const orderUpdateLiveProbe = `#graphql
  mutation OrderUpdateExpandedLiveParity($input: OrderInput!) {
    orderUpdate(input: $input) {
      order {
        id
        name
        updatedAt
        email
        phone
        poNumber
        note
        tags
        customer {
          id
          email
          displayName
        }
        customAttributes {
          key
          value
        }
        shippingAddress {
          firstName
          lastName
          address1
          address2
          company
          city
          province
          provinceCode
          country
          countryCodeV2
          zip
          phone
        }
        gift: metafield(namespace: "custom", key: "gift") {
          id
          namespace
          key
          type
          value
        }
        metafields(first: 10) {
          nodes {
            id
            namespace
            key
            type
            value
          }
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const orderReadAfterUpdate = `#graphql
  query OrderReadAfterUpdate($id: ID!) {
    order(id: $id) {
      id
      name
      updatedAt
      email
      phone
      poNumber
      note
      tags
      customer {
        id
        email
        displayName
      }
      customAttributes {
        key
        value
      }
      shippingAddress {
        firstName
        lastName
        address1
        address2
        company
        city
        province
        provinceCode
        country
        countryCodeV2
        zip
        phone
      }
      gift: metafield(namespace: "custom", key: "gift") {
        id
        namespace
        key
        type
        value
      }
      metafields(first: 10) {
        nodes {
          id
          namespace
          key
          type
          value
        }
      }
    }
  }
`;

const fulfillmentCreateInvalidIdProbe = `#graphql
  mutation FulfillmentCreateInvalidIdProbe($fulfillment: FulfillmentInput!, $message: String) {
    fulfillmentCreate(fulfillment: $fulfillment, message: $message) {
      fulfillment {
        id
        status
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const fulfillmentTrackingInfoUpdateInlineMissingIdProbe = `#graphql
  mutation FulfillmentTrackingInfoUpdateInlineMissingId($trackingInfoInput: FulfillmentTrackingInput!, $notifyCustomer: Boolean) {
    fulfillmentTrackingInfoUpdate(
      trackingInfoInput: $trackingInfoInput
      notifyCustomer: $notifyCustomer
    ) {
      fulfillment {
        id
        status
        trackingInfo {
          number
          url
          company
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const fulfillmentTrackingInfoUpdateInlineNullIdProbe = `#graphql
  mutation FulfillmentTrackingInfoUpdateInlineNullId($trackingInfoInput: FulfillmentTrackingInput!, $notifyCustomer: Boolean) {
    fulfillmentTrackingInfoUpdate(
      fulfillmentId: null
      trackingInfoInput: $trackingInfoInput
      notifyCustomer: $notifyCustomer
    ) {
      fulfillment {
        id
        status
        displayStatus
        trackingInfo {
          number
          url
          company
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const fulfillmentTrackingInfoUpdateMissingIdProbe = `#graphql
  mutation FulfillmentTrackingInfoUpdateMissingId($fulfillmentId: ID!, $trackingInfoInput: FulfillmentTrackingInput!, $notifyCustomer: Boolean) {
    fulfillmentTrackingInfoUpdate(
      fulfillmentId: $fulfillmentId
      trackingInfoInput: $trackingInfoInput
      notifyCustomer: $notifyCustomer
    ) {
      fulfillment {
        id
        status
        trackingInfo {
          number
          url
          company
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const fulfillmentTrackingInfoUpdateProbe = `#graphql
  mutation FulfillmentTrackingInfoUpdateProbe($fulfillmentId: ID!, $trackingInfoInput: FulfillmentTrackingInput!, $notifyCustomer: Boolean) {
    fulfillmentTrackingInfoUpdate(
      fulfillmentId: $fulfillmentId
      trackingInfoInput: $trackingInfoInput
      notifyCustomer: $notifyCustomer
    ) {
      fulfillment {
        id
        status
        trackingInfo {
          number
          url
          company
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const fulfillmentCancelInlineMissingIdProbe = `#graphql
  mutation FulfillmentCancelInlineMissingId {
    fulfillmentCancel {
      fulfillment {
        id
        status
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const fulfillmentCancelInlineNullIdProbe = `#graphql
  mutation FulfillmentCancelInlineNullId {
    fulfillmentCancel(id: null) {
      fulfillment {
        id
        status
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const fulfillmentCancelMissingIdProbe = `#graphql
  mutation FulfillmentCancelMissingId($id: ID!) {
    fulfillmentCancel(id: $id) {
      fulfillment {
        id
        status
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const fulfillmentLifecycleCandidateRead = `#graphql
  query FulfillmentLifecycleCandidate($first: Int!) {
    orders(first: $first, sortKey: CREATED_AT, reverse: true) {
      nodes {
        id
        name
        createdAt
        updatedAt
        displayFinancialStatus
        displayFulfillmentStatus
        cancelledAt
        closed
        note
        tags
        currentTotalPriceSet { shopMoney { amount currencyCode } }
        lineItems(first: 5) {
          nodes {
            id
            title
            quantity
          }
        }
        fulfillments(first: 5) {
          id
          status
          displayStatus
          createdAt
          updatedAt
          trackingInfo {
            number
            url
            company
          }
          fulfillmentLineItems(first: 5) {
            nodes {
              id
              quantity
              lineItem {
                id
                title
              }
            }
          }
        }
        fulfillmentOrders(first: 5) {
          nodes {
            id
            status
            requestStatus
            supportedActions {
              action
            }
            assignedLocation {
              name
            }
            lineItems(first: 5) {
              nodes {
                id
                totalQuantity
                remainingQuantity
                lineItem {
                  id
                  title
                  quantity
                  fulfillableQuantity
                }
              }
            }
          }
        }
      }
    }
  }
`;

const fulfillmentCreateLiveProbe = `#graphql
  mutation FulfillmentLifecycleCreate($fulfillment: FulfillmentInput!, $message: String) {
    fulfillmentCreate(fulfillment: $fulfillment, message: $message) {
      fulfillment {
        id
        status
        displayStatus
        trackingInfo {
          number
          url
          company
        }
        fulfillmentLineItems(first: 5) {
          nodes {
            id
            quantity
            lineItem {
              id
              title
            }
          }
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const orderReadAfterFulfillmentLifecycle = `#graphql
  query OrderReadAfterFulfillmentLifecycle($id: ID!) {
    order(id: $id) {
      id
      name
      updatedAt
      displayFulfillmentStatus
      fulfillments(first: 5) {
        id
        status
        displayStatus
        createdAt
        updatedAt
        trackingInfo {
          number
          url
          company
        }
        fulfillmentLineItems(first: 5) {
          nodes {
            id
            quantity
            lineItem {
              id
              title
            }
          }
        }
      }
      fulfillmentOrders(first: 5) {
        nodes {
          id
          status
          requestStatus
          lineItems(first: 5) {
            nodes {
              id
              totalQuantity
              remainingQuantity
              lineItem {
                id
                title
              }
            }
          }
        }
      }
    }
  }
`;

const fulfillmentCancelProbe = `#graphql
  mutation FulfillmentCancelProbe($id: ID!) {
    fulfillmentCancel(id: $id) {
      fulfillment {
        id
        status
        displayStatus
        trackingInfo {
          number
          url
          company
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const orderCreateInlineMissingOrderProbe = `#graphql
  mutation InlineMissingOrderArg {
    orderCreate {
      order { id }
      userErrors { field message }
    }
  }
`;

const orderCreateInlineNullOrderProbe = `#graphql
  mutation InlineNullOrderArg {
    orderCreate(order: null) {
      order { id }
      userErrors { field message }
    }
  }
`;

const orderCreateMissingOrderProbe = `#graphql
  mutation OrderCreateMissingOrder($order: OrderCreateOrderInput!) {
    orderCreate(order: $order) {
      order { id }
      userErrors { field message }
    }
  }
`;

const draftOrderCreateInlineMissingInputProbe = `#graphql
  mutation InlineMissingDraftOrderInput {
    draftOrderCreate {
      draftOrder { id }
      userErrors { field message }
    }
  }
`;

const draftOrderCreateInlineNullInputProbe = `#graphql
  mutation InlineNullDraftOrderInput {
    draftOrderCreate(input: null) {
      draftOrder { id }
      userErrors { field message }
    }
  }
`;

const draftOrderCreateMissingInputProbe = `#graphql
  mutation DraftOrderCreateMissingInput($input: DraftOrderInput!) {
    draftOrderCreate(input: $input) {
      draftOrder { id }
      userErrors { field message }
    }
  }
`;

const orderCreateProbe = `#graphql
  mutation OrderCreateParityPlan($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
    orderCreate(order: $order, options: $options) {
      order {
        id
        name
        email
        note
        tags
        customAttributes { key value }
        billingAddress { firstName lastName address1 city provinceCode countryCodeV2 zip }
        shippingAddress { firstName lastName address1 city provinceCode countryCodeV2 zip }
        shippingLines(first: 5) {
          nodes {
            title
            code
            source
            originalPriceSet { shopMoney { amount currencyCode } }
            taxLines {
              title
              rate
              priceSet { shopMoney { amount currencyCode } }
            }
          }
        }
        displayFinancialStatus
        displayFulfillmentStatus
        totalTaxSet { shopMoney { amount currencyCode } }
        totalDiscountsSet { shopMoney { amount currencyCode } }
        discountCodes
        currentTotalPriceSet { shopMoney { amount currencyCode } }
        lineItems(first: 5) {
          nodes {
            id
            title
            quantity
            sku
            variant { id }
            variantTitle
            originalUnitPriceSet {
              shopMoney { amount currencyCode }
              presentmentMoney { amount currencyCode }
            }
            taxLines {
              title
              rate
              priceSet { shopMoney { amount currencyCode } }
            }
          }
        }
      }
      userErrors { field message }
    }
  }
`;

const orderReadAfterCreate = `#graphql
  query OrderReadAfterCreate($id: ID!) {
    order(id: $id) {
      id
      name
      email
      note
      tags
      customAttributes { key value }
      billingAddress { firstName lastName address1 city provinceCode countryCodeV2 zip }
      shippingAddress { firstName lastName address1 city provinceCode countryCodeV2 zip }
      displayFinancialStatus
      displayFulfillmentStatus
      totalTaxSet { shopMoney { amount currencyCode } }
      totalDiscountsSet { shopMoney { amount currencyCode } }
      discountCodes
      currentTotalPriceSet { shopMoney { amount currencyCode } }
      lineItems(first: 5) {
        nodes {
          title
          quantity
          sku
          variant { id }
          taxLines {
            title
            rate
            priceSet { shopMoney { amount currencyCode } }
          }
        }
      }
    }
  }
`;

const draftOrderCreateProbe = `#graphql
  mutation DraftOrderCreateParityPlan($input: DraftOrderInput!) {
    draftOrderCreate(input: $input) {
      draftOrder {
        id
        name
        status
        ready
        email
        customer { id email displayName }
        taxExempt
        taxesIncluded
        reserveInventoryUntil
        paymentTerms {
          id
          overdue
          dueInDays
          paymentTermsName
          paymentTermsType
          translatedName
        }
        tags
        invoiceUrl
        customAttributes { key value }
        appliedDiscount {
          title
          description
          value
          valueType
          amountSet { shopMoney { amount currencyCode } }
        }
        billingAddress { firstName lastName address1 city provinceCode countryCodeV2 zip }
        shippingAddress { firstName lastName address1 city provinceCode countryCodeV2 zip }
        shippingLine {
          title
          code
          custom
          originalPriceSet { shopMoney { amount currencyCode } }
          discountedPriceSet { shopMoney { amount currencyCode } }
        }
        subtotalPriceSet { shopMoney { amount currencyCode } }
        totalDiscountsSet { shopMoney { amount currencyCode } }
        totalShippingPriceSet { shopMoney { amount currencyCode } }
        totalPriceSet { shopMoney { amount currencyCode } }
        totalQuantityOfLineItems
        lineItems(first: 5) {
          nodes {
            id
            title
            name
            quantity
            sku
            variantTitle
            custom
            requiresShipping
            taxable
            customAttributes { key value }
            appliedDiscount {
              title
              description
              value
              valueType
              amountSet { shopMoney { amount currencyCode } }
            }
            originalUnitPriceSet { shopMoney { amount currencyCode } }
            originalTotalSet { shopMoney { amount currencyCode } }
            discountedTotalSet { shopMoney { amount currencyCode } }
            totalDiscountSet { shopMoney { amount currencyCode } }
            variant { id title sku }
          }
        }
      }
      userErrors { field message }
    }
  }
`;

const draftOrderDetailRead = `#graphql
  query DraftOrderReadParityPlan($id: ID!) {
    draftOrder(id: $id) {
      id
      name
      status
      ready
      email
      customer { id email displayName }
      taxExempt
      taxesIncluded
      reserveInventoryUntil
      paymentTerms {
        id
        overdue
        dueInDays
        paymentTermsName
        paymentTermsType
        translatedName
      }
      tags
      invoiceUrl
      customAttributes { key value }
      appliedDiscount {
        title
        description
        value
        valueType
        amountSet { shopMoney { amount currencyCode } }
      }
      billingAddress { firstName lastName address1 city provinceCode countryCodeV2 zip }
      shippingAddress { firstName lastName address1 city provinceCode countryCodeV2 zip }
      shippingLine {
        title
        code
        custom
        originalPriceSet { shopMoney { amount currencyCode } }
        discountedPriceSet { shopMoney { amount currencyCode } }
      }
      subtotalPriceSet { shopMoney { amount currencyCode } }
      totalDiscountsSet { shopMoney { amount currencyCode } }
      totalShippingPriceSet { shopMoney { amount currencyCode } }
      totalPriceSet { shopMoney { amount currencyCode } }
      totalQuantityOfLineItems
      lineItems(first: 5) {
        nodes {
          id
          title
          name
          quantity
          sku
          variantTitle
          custom
          requiresShipping
          taxable
          customAttributes { key value }
          appliedDiscount {
            title
            description
            value
            valueType
            amountSet { shopMoney { amount currencyCode } }
          }
          originalUnitPriceSet { shopMoney { amount currencyCode } }
          originalTotalSet { shopMoney { amount currencyCode } }
          discountedTotalSet { shopMoney { amount currencyCode } }
          totalDiscountSet { shopMoney { amount currencyCode } }
          variant { id title sku }
        }
      }
    }
  }
`;

const draftOrderCompleteDownstreamRead = `#graphql
  query DraftOrderCompleteDownstreamRead($id: ID!) {
    draftOrder(id: $id) {
      id
      name
      status
      ready
      invoiceUrl
      completedAt
      totalPriceSet { shopMoney { amount currencyCode } }
      lineItems(first: 5) {
        nodes {
          id
          title
          quantity
          sku
          variantTitle
          originalUnitPriceSet { shopMoney { amount currencyCode } }
        }
      }
      order {
        id
        name
        sourceName
        paymentGatewayNames
        displayFinancialStatus
        displayFulfillmentStatus
        note
        tags
        currentTotalPriceSet { shopMoney { amount currencyCode } }
        lineItems(first: 5) {
          nodes {
            id
            title
            quantity
            sku
            variantTitle
            originalUnitPriceSet { shopMoney { amount currencyCode } }
          }
        }
      }
    }
  }
`;

const draftOrdersCatalogRead = `#graphql
  query DraftOrdersReadParityPlan($first: Int!) {
    draftOrders(first: $first, reverse: true) {
      edges {
        cursor
        node {
          id
          name
          status
          email
          tags
          createdAt
          updatedAt
          totalPriceSet {
            shopMoney {
              amount
              currencyCode
            }
          }
        }
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
  }
`;

const draftOrdersCountRead = `#graphql
  query DraftOrdersCountReadParityPlan($query: String) {
    draftOrdersCount(query: $query) {
      count
      precision
    }
  }
`;

const draftOrdersInvalidEmailQueryRead = `#graphql
  query DraftOrdersInvalidEmailQueryRead($first: Int!, $query: String!) {
    draftOrders(first: $first, query: $query) {
      edges {
        cursor
        node {
          id
          name
          email
          status
          ready
        }
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
    draftOrdersCount(query: $query) {
      count
      precision
    }
  }
`;

const draftOrderCompleteInlineMissingIdProbe = `#graphql
  mutation DraftOrderCompleteInlineMissingIdParity {
    draftOrderComplete(paymentGatewayId: null, sourceName: "hermes-cron-orders") {
      draftOrder {
        id
        name
        status
        ready
        invoiceUrl
      }
      userErrors { field message }
    }
  }
`;

const draftOrderCompleteInlineNullIdProbe = `#graphql
  mutation DraftOrderCompleteInlineNullIdParity {
    draftOrderComplete(id: null, paymentGatewayId: null, sourceName: "hermes-cron-orders") {
      draftOrder {
        id
        name
        status
        ready
        invoiceUrl
      }
      userErrors { field message }
    }
  }
`;

const draftOrderCompleteMissingIdProbe = `#graphql
  mutation DraftOrderCompleteMissingId($id: ID!, $paymentGatewayId: ID, $sourceName: String) {
    draftOrderComplete(id: $id, paymentGatewayId: $paymentGatewayId, sourceName: $sourceName) {
      draftOrder {
        id
        name
      }
      userErrors { field message }
    }
  }
`;

const draftOrderCompleteProbe = `#graphql
  mutation DraftOrderCompleteParityPlan($id: ID!, $paymentGatewayId: ID, $sourceName: String, $paymentPending: Boolean) {
    draftOrderComplete(
      id: $id
      paymentGatewayId: $paymentGatewayId
      sourceName: $sourceName
      paymentPending: $paymentPending
    ) {
      draftOrder {
        id
        name
        status
        ready
        invoiceUrl
        completedAt
        totalPriceSet { shopMoney { amount currencyCode } }
        lineItems(first: 5) {
          nodes {
            id
            title
            quantity
            sku
          }
        }
        order {
          id
          name
          sourceName
          paymentGatewayNames
          displayFinancialStatus
          displayFulfillmentStatus
          note
          tags
          currentTotalPriceSet { shopMoney { amount currencyCode } }
          lineItems(first: 5) {
            nodes {
              id
              title
              quantity
              sku
              variantTitle
              originalUnitPriceSet { shopMoney { amount currencyCode } }
            }
          }
        }
      }
      userErrors { field message }
    }
  }
`;

const orderEditBeginMissingIdProbe = `#graphql
  mutation OrderEditBeginMissingId($id: ID!) {
    orderEditBegin(id: $id) {
      calculatedOrder {
        id
      }
      userErrors { field message }
    }
  }
`;

const orderEditBeginProbe = `#graphql
  mutation OrderEditBeginParityPlan($id: ID!) {
    orderEditBegin(id: $id) {
      calculatedOrder {
        id
      }
      userErrors { field message }
    }
  }
`;

const orderEditAddVariantMissingIdProbe = `#graphql
  mutation OrderEditAddVariantMissingId($id: ID!, $variantId: ID!, $quantity: Int!) {
    orderEditAddVariant(id: $id, variantId: $variantId, quantity: $quantity) {
      calculatedOrder {
        id
      }
      calculatedLineItem {
        id
      }
      userErrors { field message }
    }
  }
`;

const orderEditAddVariantProbe = `#graphql
  mutation OrderEditAddVariantParityPlan($id: ID!, $variantId: ID!, $quantity: Int!) {
    orderEditAddVariant(id: $id, variantId: $variantId, quantity: $quantity) {
      calculatedOrder {
        id
      }
      calculatedLineItem {
        id
      }
      userErrors { field message }
    }
  }
`;

const orderEditSetQuantityMissingIdProbe = `#graphql
  mutation OrderEditSetQuantityMissingId($id: ID!, $lineItemId: ID!, $quantity: Int!) {
    orderEditSetQuantity(id: $id, lineItemId: $lineItemId, quantity: $quantity) {
      calculatedOrder {
        id
      }
      calculatedLineItem {
        id
      }
      userErrors { field message }
    }
  }
`;

const orderEditSetQuantityProbe = `#graphql
  mutation OrderEditSetQuantityParityPlan($id: ID!, $lineItemId: ID!, $quantity: Int!) {
    orderEditSetQuantity(id: $id, lineItemId: $lineItemId, quantity: $quantity) {
      calculatedOrder {
        id
      }
      calculatedLineItem {
        id
      }
      userErrors { field message }
    }
  }
`;

const orderEditCommitMissingIdProbe = `#graphql
  mutation OrderEditCommitMissingId($id: ID!, $notifyCustomer: Boolean, $staffNote: String) {
    orderEditCommit(id: $id, notifyCustomer: $notifyCustomer, staffNote: $staffNote) {
      order {
        id
        name
      }
      userErrors { field message }
    }
  }
`;

const orderEditCommitProbe = `#graphql
  mutation OrderEditCommitParityPlan($id: ID!, $notifyCustomer: Boolean, $staffNote: String) {
    orderEditCommit(id: $id, notifyCustomer: $notifyCustomer, staffNote: $staffNote) {
      order {
        id
        name
      }
      userErrors { field message }
    }
  }
`;

async function main() {
  const stamp = Date.now();
  const credential = summarizeCredential(adminAccessToken);
  const manualStoreAuthSummary = await readManualStoreAuthSummary();
  const parityMetadata = await readOrderCreationParityMetadata(credential, manualStoreAuthSummary);

  const orderEmptyStateVariables = { missingOrderId: 'gid://shopify/Order/0', first: 1 };
  const orderUpdateUnknownIdVariables = {
    input: {
      id: 'gid://shopify/Order/0',
      note: 'order update unknown-id parity probe',
      tags: ['parity-probe', 'order-update'],
    },
  };
  const orderUpdateMissingIdVariables = {
    input: {
      note: 'order update missing-id parity probe',
      tags: ['parity-probe', 'order-update', 'missing-id'],
    },
  };
  const orderUpdateLiveVariables = {
    input: {
      id: 'gid://shopify/Order/placeholder',
      email: `hermes-order-update-${stamp}@example.com`,
      poNumber: `PO-UPDATE-${stamp}`,
      note: 'order update expanded live parity captured note',
      tags: ['expanded-live-parity', 'order-update'],
      customAttributes: [
        {
          key: 'source',
          value: 'expanded-live-parity',
        },
      ],
      shippingAddress: {
        firstName: 'Ada',
        lastName: 'Lovelace',
        address1: '190 MacLaren',
        address2: 'Suite 200',
        company: 'Analytical Engines Ltd',
        city: 'Sudbury',
        province: 'Ontario',
        provinceCode: 'ON',
        country: 'Canada',
        countryCode: 'CA',
        zip: 'K2P0V6',
        phone: '+161****2222',
      },
      metafields: [
        {
          namespace: 'custom',
          key: 'gift',
          type: 'single_line_text_field',
          value: 'yes',
        },
      ],
    },
  };
  const fulfillmentCreateInvalidIdVariables = {
    fulfillment: {
      notifyCustomer: false,
      trackingInfo: {
        number: 'HERMES-PROBE',
        url: 'https://example.com/track/HERMES-PROBE',
        company: 'Hermes',
      },
      lineItemsByFulfillmentOrder: [
        {
          fulfillmentOrderId: 'gid://shopify/FulfillmentOrder/0',
        },
      ],
    },
    message: 'hermes fulfillment probe',
  };
  const fulfillmentTrackingInfoUpdateInlineVariables = {
    notifyCustomer: false,
    trackingInfoInput: {
      number: 'HERMES-TRACK-UPDATE',
      url: 'https://example.com/track/HERMES-TRACK-UPDATE',
      company: 'Hermes',
    },
  };
  const fulfillmentTrackingInfoUpdateVariables = {
    fulfillmentId: 'gid://shopify/Fulfillment/0',
    ...fulfillmentTrackingInfoUpdateInlineVariables,
  };
  const fulfillmentCancelVariables = {
    id: 'gid://shopify/Fulfillment/0',
  };

  const orderCreateVariables = {
    order: {
      email: `hermes-order-probe-${stamp}@example.com`,
      note: 'order create parity probe',
      tags: ['parity-probe', 'order-create'],
      test: true,
      currency: 'USD',
      presentmentCurrency: 'CAD',
      fulfillmentStatus: 'FULFILLED',
      discountCode: {
        itemFixedDiscountCode: {
          code: 'SAVE5',
          amountSet: {
            shopMoney: {
              amount: '5.00',
              currencyCode: 'USD',
            },
          },
        },
      },
      customAttributes: [
        {
          key: 'source',
          value: 'hermes-parity-plan',
        },
        {
          key: 'channel',
          value: 'cron-orders-bootstrap',
        },
      ],
      billingAddress: {
        firstName: 'Hermes',
        lastName: 'Operator',
        address1: '123 Queen St W',
        city: 'Toronto',
        provinceCode: 'ON',
        countryCode: 'CA',
        zip: 'M5H 2M9',
        phone: '+141****0101',
      },
      shippingAddress: {
        firstName: 'Hermes',
        lastName: 'Operator',
        address1: '123 Queen St W',
        city: 'Toronto',
        provinceCode: 'ON',
        countryCode: 'CA',
        zip: 'M5H 2M9',
        phone: '+141****0101',
      },
      shippingLines: [
        {
          title: 'Standard',
          code: 'STANDARD',
          source: 'hermes-parity-plan',
          priceSet: {
            shopMoney: {
              amount: '5.00',
              currencyCode: 'USD',
            },
          },
          taxLines: [
            {
              title: 'Shipping tax',
              rate: 0.1,
              priceSet: {
                shopMoney: {
                  amount: '0.50',
                  currencyCode: 'USD',
                },
              },
            },
          ],
        },
      ],
      lineItems: [
        {
          variantId: 'gid://shopify/ProductVariant/48540157378793',
          title: 'Hermes inventory-backed line item',
          quantity: 2,
          priceSet: {
            shopMoney: {
              amount: '20.00',
              currencyCode: 'USD',
            },
            presentmentMoney: {
              amount: '27.00',
              currencyCode: 'CAD',
            },
          },
          requiresShipping: true,
          taxable: true,
          sku: `hermes-order-probe-${stamp}`,
          taxLines: [
            {
              title: 'Line tax',
              rate: 0.05,
              priceSet: {
                shopMoney: {
                  amount: '2.00',
                  currencyCode: 'USD',
                },
              },
            },
          ],
        },
      ],
      transactions: [
        {
          kind: 'SALE',
          status: 'SUCCESS',
          gateway: 'manual',
          test: true,
          amountSet: {
            shopMoney: {
              amount: '42.50',
              currencyCode: 'USD',
            },
          },
        },
      ],
    },
    options: {
      inventoryBehaviour: 'BYPASS',
      sendReceipt: false,
      sendFulfillmentReceipt: false,
    },
  };

  const draftOrderReserveInventoryUntil = new Date(Date.now() + 30 * 24 * 60 * 60 * 1000)
    .toISOString()
    .replace(/\.\d{3}Z$/u, 'Z');
  const draftOrderCreateVariables = {
    input: {
      purchasingEntity: {
        customerId: 'gid://shopify/Customer/6157654556905',
      },
      email: `hermes-draft-order-probe-${stamp}@example.com`,
      note: 'merchant realistic draft order create parity probe',
      taxExempt: true,
      reserveInventoryUntil: draftOrderReserveInventoryUntil,
      tags: ['parity-probe', 'draft-order-create', 'merchant-realistic'],
      customAttributes: [
        {
          key: 'source',
          value: 'phone-order',
        },
        {
          key: 'purchase-order',
          value: `PO-${stamp}`,
        },
      ],
      appliedDiscount: {
        title: 'Loyalty credit',
        description: 'merchant order-level discount',
        value: 5,
        amount: 5,
        valueType: 'FIXED_AMOUNT',
      },
      billingAddress: {
        firstName: 'Hermes',
        lastName: 'Buyer',
        address1: '123 Queen St W',
        city: 'Toronto',
        provinceCode: 'ON',
        countryCode: 'CA',
        zip: 'M5H 2M9',
        phone: '+141****0101',
      },
      shippingAddress: {
        firstName: 'Hermes',
        lastName: 'Buyer',
        address1: '500 King St W',
        city: 'Toronto',
        provinceCode: 'ON',
        countryCode: 'CA',
        zip: 'M5V 1L9',
        phone: '+141****0102',
      },
      shippingLine: {
        title: 'Merchant Courier',
        priceWithCurrency: {
          amount: '7.25',
          currencyCode: 'CAD',
        },
      },
      lineItems: [
        {
          title: 'Custom installation service',
          quantity: 2,
          originalUnitPrice: '20.00',
          requiresShipping: false,
          taxable: false,
          sku: `hermes-custom-service-${stamp}`,
          appliedDiscount: {
            title: 'Service discount',
            description: '10 percent off service',
            value: 10,
            amount: 4,
            valueType: 'PERCENTAGE',
          },
          customAttributes: [{ key: 'appointment', value: 'morning' }],
        },
        {
          variantId: 'gid://shopify/ProductVariant/48540157378793',
          quantity: 1,
        },
      ],
    },
  };

  const draftOrderCompleteMissingIdVariables = {
    paymentGatewayId: null,
    sourceName: 'hermes-cron-orders',
  };
  const draftOrderCompleteBaseVariables = {
    paymentGatewayId: null,
    sourceName: 'hermes-cron-orders',
    paymentPending: false,
  };
  const orderEditBeginMissingIdVariables = {};
  const orderEditBeginVariables = {
    id: 'gid://shopify/Order/0',
  };
  const orderEditAddVariantMissingIdVariables = {
    variantId: 'gid://shopify/ProductVariant/0',
    quantity: 1,
  };
  const orderEditAddVariantVariables = {
    id: 'gid://shopify/CalculatedOrder/0',
    variantId: 'gid://shopify/ProductVariant/0',
    quantity: 1,
  };
  const orderEditSetQuantityMissingIdVariables = {
    lineItemId: 'gid://shopify/CalculatedLineItem/0',
    quantity: 1,
  };
  const orderEditSetQuantityVariables = {
    id: 'gid://shopify/CalculatedOrder/0',
    lineItemId: 'gid://shopify/CalculatedLineItem/0',
    quantity: 2,
  };
  const orderEditCommitMissingIdVariables = {
    notifyCustomer: false,
    staffNote: 'missing id probe',
  };
  const orderEditCommitVariables = {
    id: 'gid://shopify/CalculatedOrder/0',
    notifyCustomer: false,
    staffNote: 'order edit commit parity plan',
  };

  const orderEmptyStateResult = await runGraphql(orderEmptyStateRead, orderEmptyStateVariables);
  const orderUpdateUnknownIdResult = await runGraphql(orderUpdateUnknownIdProbe, orderUpdateUnknownIdVariables);
  const orderUpdateMissingIdResult = await runGraphql(orderUpdateUnknownIdProbe, orderUpdateMissingIdVariables);
  const orderUpdateInlineMissingIdResult = await runGraphql(orderUpdateInlineMissingIdProbe, {});
  const orderUpdateInlineNullIdResult = await runGraphql(orderUpdateInlineNullIdProbe, {});
  const fulfillmentCreateInvalidIdResult = await runGraphql(
    fulfillmentCreateInvalidIdProbe,
    fulfillmentCreateInvalidIdVariables,
  );
  const fulfillmentTrackingInfoUpdateInlineMissingIdResult = await runGraphql(
    fulfillmentTrackingInfoUpdateInlineMissingIdProbe,
    fulfillmentTrackingInfoUpdateInlineVariables,
  );
  const fulfillmentTrackingInfoUpdateInlineNullIdResult = await runGraphql(
    fulfillmentTrackingInfoUpdateInlineNullIdProbe,
    fulfillmentTrackingInfoUpdateInlineVariables,
  );
  const fulfillmentTrackingInfoUpdateMissingIdResult = await runGraphql(
    fulfillmentTrackingInfoUpdateMissingIdProbe,
    fulfillmentTrackingInfoUpdateInlineVariables,
  );
  const fulfillmentTrackingInfoUpdateResult = await runGraphql(
    fulfillmentTrackingInfoUpdateProbe,
    fulfillmentTrackingInfoUpdateVariables,
  );
  const fulfillmentCancelInlineMissingIdResult = await runGraphql(fulfillmentCancelInlineMissingIdProbe, {});
  const fulfillmentCancelInlineNullIdResult = await runGraphql(fulfillmentCancelInlineNullIdProbe, {});
  const fulfillmentCancelMissingIdResult = await runGraphql(fulfillmentCancelMissingIdProbe, {});
  const fulfillmentCancelResult = await runGraphql(fulfillmentCancelProbe, fulfillmentCancelVariables);
  const fulfillmentLifecycleCandidateResult = await runGraphql(fulfillmentLifecycleCandidateRead, { first: 25 });
  const fulfillmentLifecycleCandidate = findFulfillmentLifecycleCandidate(fulfillmentLifecycleCandidateResult);
  const fulfillmentLifecycleStamp = new Date()
    .toISOString()
    .replace(/[-:.TZ]/gu, '')
    .slice(0, 14);
  const fulfillmentCreateLiveVariables = fulfillmentLifecycleCandidate
    ? {
        fulfillment: {
          notifyCustomer: false,
          trackingInfo: {
            number: `HERMES-CREATE-${fulfillmentLifecycleStamp}`,
            url: `https://example.com/track/HERMES-CREATE-${fulfillmentLifecycleStamp}`,
            company: 'Hermes',
          },
          lineItemsByFulfillmentOrder: [
            {
              fulfillmentOrderId: fulfillmentLifecycleCandidate.fulfillmentOrder.id,
              fulfillmentOrderLineItems: [
                {
                  id: fulfillmentLifecycleCandidate.lineItem.id,
                  quantity: 1,
                },
              ],
            },
          ],
        },
        message: `HAR-187 fulfillment lifecycle capture ${fulfillmentLifecycleStamp}`,
      }
    : null;
  const fulfillmentCreateLiveResult = fulfillmentCreateLiveVariables
    ? await runGraphql(fulfillmentCreateLiveProbe, fulfillmentCreateLiveVariables)
    : null;
  const createdFulfillmentId = fulfillmentCreateLiveResult?.payload?.data?.fulfillmentCreate?.fulfillment?.id ?? null;
  const fulfillmentTrackingInfoUpdateLiveVariables =
    typeof createdFulfillmentId === 'string'
      ? {
          fulfillmentId: createdFulfillmentId,
          notifyCustomer: false,
          trackingInfoInput: {
            number: `HERMES-UPDATE-${fulfillmentLifecycleStamp}`,
            url: `https://example.com/track/HERMES-UPDATE-${fulfillmentLifecycleStamp}`,
            company: 'Hermes',
          },
        }
      : null;
  const fulfillmentTrackingInfoUpdateLiveResult = fulfillmentTrackingInfoUpdateLiveVariables
    ? await runGraphql(fulfillmentTrackingInfoUpdateProbe, fulfillmentTrackingInfoUpdateLiveVariables)
    : null;
  const fulfillmentReadAfterTrackingUpdateResult = fulfillmentLifecycleCandidate
    ? await runGraphql(orderReadAfterFulfillmentLifecycle, { id: fulfillmentLifecycleCandidate.order.id })
    : null;
  const fulfillmentCancelLiveVariables =
    typeof createdFulfillmentId === 'string'
      ? {
          id: createdFulfillmentId,
        }
      : null;
  const fulfillmentCancelLiveResult = fulfillmentCancelLiveVariables
    ? await runGraphql(fulfillmentCancelProbe, fulfillmentCancelLiveVariables)
    : null;
  const fulfillmentReadAfterCancelResult = fulfillmentLifecycleCandidate
    ? await runGraphql(orderReadAfterFulfillmentLifecycle, { id: fulfillmentLifecycleCandidate.order.id })
    : null;
  const fulfillmentLifecycleCaptured =
    Boolean(fulfillmentLifecycleCandidate) &&
    hasEmptyUserErrors(fulfillmentCreateLiveResult, 'fulfillmentCreate') &&
    hasEmptyUserErrors(fulfillmentTrackingInfoUpdateLiveResult, 'fulfillmentTrackingInfoUpdate') &&
    hasEmptyUserErrors(fulfillmentCancelLiveResult, 'fulfillmentCancel');
  const orderCreateInlineMissingOrderResult = await runGraphql(orderCreateInlineMissingOrderProbe, {});
  const orderCreateInlineNullOrderResult = await runGraphql(orderCreateInlineNullOrderProbe, {});
  const orderCreateMissingOrderResult = await runGraphql(orderCreateMissingOrderProbe, {});
  const orderCreateResult = await runGraphql(orderCreateProbe, orderCreateVariables);
  const draftOrderCreateInlineMissingInputResult = await runGraphql(draftOrderCreateInlineMissingInputProbe, {});
  const draftOrderCreateInlineNullInputResult = await runGraphql(draftOrderCreateInlineNullInputProbe, {});
  const draftOrderCreateMissingInputResult = await runGraphql(draftOrderCreateMissingInputProbe, {});
  const draftOrderCreateResult = await runGraphql(draftOrderCreateProbe, draftOrderCreateVariables);
  const draftOrderCompleteInlineMissingIdResult = await runGraphql(draftOrderCompleteInlineMissingIdProbe, {});
  const draftOrderCompleteInlineNullIdResult = await runGraphql(draftOrderCompleteInlineNullIdProbe, {});
  const draftOrderCompleteMissingIdResult = await runGraphql(
    draftOrderCompleteMissingIdProbe,
    draftOrderCompleteMissingIdVariables,
  );
  const createdDraftOrderId = draftOrderCreateResult.payload?.data?.draftOrderCreate?.draftOrder?.id ?? null;
  const draftOrderDetailResult = createdDraftOrderId
    ? await runGraphql(draftOrderDetailRead, { id: createdDraftOrderId })
    : null;
  const draftOrderCompleteVariables = {
    id: createdDraftOrderId ?? 'gid://shopify/DraftOrder/0',
    ...draftOrderCompleteBaseVariables,
  };
  const draftOrderCompleteResult = await runGraphql(draftOrderCompleteProbe, draftOrderCompleteVariables);
  const completedDraftOrder = draftOrderCompleteResult.payload?.data?.draftOrderComplete?.draftOrder ?? null;
  const completedDraftOrderId = completedDraftOrder?.id ?? null;
  const completedOrderId = completedDraftOrder?.order?.id ?? null;
  const draftOrderReadAfterCompleteResult = completedDraftOrderId
    ? await runGraphql(draftOrderCompleteDownstreamRead, { id: completedDraftOrderId })
    : null;
  const orderReadAfterDraftOrderCompleteResult = completedOrderId
    ? await runGraphql(orderReadAfterCreate, { id: completedOrderId })
    : null;
  const orderEditBeginMissingIdResult = await runGraphql(
    orderEditBeginMissingIdProbe,
    orderEditBeginMissingIdVariables,
  );
  const orderEditBeginResult = await runGraphql(orderEditBeginProbe, orderEditBeginVariables);
  const orderEditAddVariantMissingIdResult = await runGraphql(
    orderEditAddVariantMissingIdProbe,
    orderEditAddVariantMissingIdVariables,
  );
  const orderEditAddVariantResult = await runGraphql(orderEditAddVariantProbe, orderEditAddVariantVariables);
  const orderEditSetQuantityMissingIdResult = await runGraphql(
    orderEditSetQuantityMissingIdProbe,
    orderEditSetQuantityMissingIdVariables,
  );
  const orderEditSetQuantityResult = await runGraphql(orderEditSetQuantityProbe, orderEditSetQuantityVariables);
  const orderEditCommitMissingIdResult = await runGraphql(
    orderEditCommitMissingIdProbe,
    orderEditCommitMissingIdVariables,
  );
  const orderEditCommitResult = await runGraphql(orderEditCommitProbe, orderEditCommitVariables);

  const createdOrderId = orderCreateResult.payload?.data?.orderCreate?.order?.id ?? null;
  const orderReadAfterCreateResult = createdOrderId
    ? await runGraphql(orderReadAfterCreate, { id: createdOrderId })
    : null;
  const orderUpdateLiveVariablesForRun = createdOrderId
    ? {
        input: {
          ...orderUpdateLiveVariables.input,
          id: createdOrderId,
        },
      }
    : null;
  const orderUpdateLiveResult = orderUpdateLiveVariablesForRun
    ? await runGraphql(orderUpdateLiveProbe, orderUpdateLiveVariablesForRun)
    : null;
  const orderReadAfterUpdateResult = createdOrderId
    ? await runGraphql(orderReadAfterUpdate, { id: createdOrderId })
    : null;
  const draftOrdersCatalogVariables = { first: 5 };
  const draftOrdersCountVariables = { query: null };
  const draftOrdersInvalidEmailQueryVariables = { first: 2, query: 'email:hermes@example.com' };
  const draftOrdersCatalogResult = await runGraphql(draftOrdersCatalogRead, draftOrdersCatalogVariables);
  const draftOrdersCountResult = await runGraphql(draftOrdersCountRead, draftOrdersCountVariables);
  const draftOrdersInvalidEmailQueryResult = await runGraphql(
    draftOrdersInvalidEmailQueryRead,
    draftOrdersInvalidEmailQueryVariables,
  );

  const authFailureMessage =
    getAuthFailureMessage(orderEmptyStateResult) ??
    getAuthFailureMessage(orderUpdateUnknownIdResult) ??
    getAuthFailureMessage(orderUpdateMissingIdResult) ??
    getAuthFailureMessage(orderUpdateInlineMissingIdResult) ??
    getAuthFailureMessage(orderUpdateInlineNullIdResult) ??
    getAuthFailureMessage(fulfillmentCreateInvalidIdResult) ??
    getAuthFailureMessage(fulfillmentTrackingInfoUpdateResult) ??
    getAuthFailureMessage(fulfillmentCancelResult) ??
    getAuthFailureMessage(fulfillmentLifecycleCandidateResult) ??
    getAuthFailureMessage(fulfillmentCreateLiveResult) ??
    getAuthFailureMessage(fulfillmentTrackingInfoUpdateLiveResult) ??
    getAuthFailureMessage(fulfillmentReadAfterTrackingUpdateResult) ??
    getAuthFailureMessage(fulfillmentCancelLiveResult) ??
    getAuthFailureMessage(fulfillmentReadAfterCancelResult) ??
    getAuthFailureMessage(orderCreateInlineMissingOrderResult) ??
    getAuthFailureMessage(orderCreateInlineNullOrderResult) ??
    getAuthFailureMessage(orderCreateMissingOrderResult) ??
    getAuthFailureMessage(orderCreateResult) ??
    getAuthFailureMessage(orderUpdateLiveResult) ??
    getAuthFailureMessage(orderReadAfterUpdateResult) ??
    getAuthFailureMessage(draftOrdersCatalogResult) ??
    getAuthFailureMessage(draftOrdersCountResult) ??
    getAuthFailureMessage(draftOrdersInvalidEmailQueryResult) ??
    getAuthFailureMessage(draftOrderCreateInlineMissingInputResult) ??
    getAuthFailureMessage(draftOrderCreateInlineNullInputResult) ??
    getAuthFailureMessage(draftOrderCreateMissingInputResult) ??
    getAuthFailureMessage(draftOrderCreateResult) ??
    getAuthFailureMessage(draftOrderCompleteInlineMissingIdResult) ??
    getAuthFailureMessage(draftOrderCompleteInlineNullIdResult) ??
    getAuthFailureMessage(draftOrderCompleteMissingIdResult) ??
    getAuthFailureMessage(draftOrderCompleteResult) ??
    getAuthFailureMessage(draftOrderReadAfterCompleteResult) ??
    getAuthFailureMessage(orderReadAfterDraftOrderCompleteResult) ??
    getAuthFailureMessage(orderEditBeginMissingIdResult) ??
    getAuthFailureMessage(orderEditBeginResult) ??
    getAuthFailureMessage(orderEditAddVariantMissingIdResult) ??
    getAuthFailureMessage(orderEditAddVariantResult) ??
    getAuthFailureMessage(orderEditSetQuantityMissingIdResult) ??
    getAuthFailureMessage(orderEditSetQuantityResult) ??
    getAuthFailureMessage(orderEditCommitMissingIdResult) ??
    getAuthFailureMessage(orderEditCommitResult) ??
    getAuthFailureMessage(orderReadAfterCreateResult) ??
    getAuthFailureMessage(draftOrderDetailResult);
  const authRegressed = Boolean(authFailureMessage);

  if (!authRegressed) {
    await writeJson(orderEmptyStateFixturePath, {
      variables: orderEmptyStateVariables,
      response: orderEmptyStateResult.payload,
    });
    await writeJson(orderUpdateUnknownIdFixturePath, {
      variables: orderUpdateUnknownIdVariables,
      mutation: {
        response: orderUpdateUnknownIdResult.payload,
      },
    });
    await writeJson(orderUpdateMissingIdFixturePath, {
      variables: orderUpdateMissingIdVariables,
      mutation: {
        response: orderUpdateMissingIdResult.payload,
      },
    });
    await writeJson(orderUpdateInlineMissingIdFixturePath, {
      query: orderUpdateInlineMissingIdProbe.replace(/^#graphql\n/, '').trim(),
      variables: {},
      mutation: {
        response: orderUpdateInlineMissingIdResult.payload,
      },
    });
    await writeJson(orderUpdateInlineNullIdFixturePath, {
      query: orderUpdateInlineNullIdProbe.replace(/^#graphql\n/, '').trim(),
      variables: {},
      mutation: {
        response: orderUpdateInlineNullIdResult.payload,
      },
    });
    await writeJson(fulfillmentCreateInvalidIdFixturePath, {
      variables: fulfillmentCreateInvalidIdVariables,
      mutation: {
        response: fulfillmentCreateInvalidIdResult.payload,
      },
    });
    await writeJson(fulfillmentTrackingInfoUpdateInlineMissingIdFixturePath, {
      query: fulfillmentTrackingInfoUpdateInlineMissingIdProbe.replace(/^#graphql\n/, '').trim(),
      variables: fulfillmentTrackingInfoUpdateInlineVariables,
      mutation: {
        response: fulfillmentTrackingInfoUpdateInlineMissingIdResult.payload,
      },
    });
    await writeJson(fulfillmentTrackingInfoUpdateInlineNullIdFixturePath, {
      query: fulfillmentTrackingInfoUpdateInlineNullIdProbe.replace(/^#graphql\n/, '').trim(),
      variables: fulfillmentTrackingInfoUpdateInlineVariables,
      mutation: {
        response: fulfillmentTrackingInfoUpdateInlineNullIdResult.payload,
      },
    });
    await writeJson(fulfillmentTrackingInfoUpdateMissingIdFixturePath, {
      query: fulfillmentTrackingInfoUpdateMissingIdProbe.replace(/^#graphql\n/, '').trim(),
      variables: fulfillmentTrackingInfoUpdateInlineVariables,
      mutation: {
        response: fulfillmentTrackingInfoUpdateMissingIdResult.payload,
      },
    });
    await writeJson(fulfillmentCancelInlineMissingIdFixturePath, {
      query: fulfillmentCancelInlineMissingIdProbe.replace(/^#graphql\n/, '').trim(),
      variables: {},
      mutation: {
        response: fulfillmentCancelInlineMissingIdResult.payload,
      },
    });
    await writeJson(fulfillmentCancelInlineNullIdFixturePath, {
      query: fulfillmentCancelInlineNullIdProbe.replace(/^#graphql\n/, '').trim(),
      variables: {},
      mutation: {
        response: fulfillmentCancelInlineNullIdResult.payload,
      },
    });
    await writeJson(fulfillmentCancelMissingIdFixturePath, {
      query: fulfillmentCancelMissingIdProbe.replace(/^#graphql\n/, '').trim(),
      variables: {},
      mutation: {
        response: fulfillmentCancelMissingIdResult.payload,
      },
    });
    if (
      fulfillmentLifecycleCaptured &&
      fulfillmentLifecycleCandidate &&
      fulfillmentCreateLiveVariables &&
      fulfillmentCreateLiveResult &&
      fulfillmentTrackingInfoUpdateLiveVariables &&
      fulfillmentTrackingInfoUpdateLiveResult &&
      fulfillmentCancelLiveVariables &&
      fulfillmentCancelLiveResult
    ) {
      const setup = {
        candidate: fulfillmentLifecycleCandidate,
        fulfillmentCreate: {
          query: fulfillmentCreateLiveProbe.replace(/^#graphql\n/, '').trim(),
          variables: fulfillmentCreateLiveVariables,
          response: fulfillmentCreateLiveResult.payload,
        },
      };

      await writeJson(fulfillmentTrackingInfoUpdateParityFixturePath, {
        query: fulfillmentTrackingInfoUpdateProbe.replace(/^#graphql\n/, '').trim(),
        variables: fulfillmentTrackingInfoUpdateLiveVariables,
        setup,
        mutation: {
          response: fulfillmentTrackingInfoUpdateLiveResult.payload,
        },
        downstreamRead: {
          query: orderReadAfterFulfillmentLifecycle.replace(/^#graphql\n/, '').trim(),
          variables: { id: fulfillmentLifecycleCandidate.order.id },
          response: fulfillmentReadAfterTrackingUpdateResult?.payload ?? null,
        },
      });
      await writeJson(fulfillmentCancelParityFixturePath, {
        query: fulfillmentCancelProbe.replace(/^#graphql\n/, '').trim(),
        variables: fulfillmentCancelLiveVariables,
        setup: {
          ...setup,
          fulfillmentTrackingInfoUpdate: {
            query: fulfillmentTrackingInfoUpdateProbe.replace(/^#graphql\n/, '').trim(),
            variables: fulfillmentTrackingInfoUpdateLiveVariables,
            response: fulfillmentTrackingInfoUpdateLiveResult.payload,
          },
        },
        mutation: {
          response: fulfillmentCancelLiveResult.payload,
        },
        downstreamRead: {
          query: orderReadAfterFulfillmentLifecycle.replace(/^#graphql\n/, '').trim(),
          variables: { id: fulfillmentLifecycleCandidate.order.id },
          response: fulfillmentReadAfterCancelResult?.payload ?? null,
        },
      });
    }
    await writeJson(orderCreateInlineMissingOrderFixturePath, {
      query: orderCreateInlineMissingOrderProbe.replace(/^#graphql\n/, '').trim(),
      variables: {},
      mutation: {
        response: orderCreateInlineMissingOrderResult.payload,
      },
    });
    await writeJson(orderCreateInlineNullOrderFixturePath, {
      query: orderCreateInlineNullOrderProbe.replace(/^#graphql\n/, '').trim(),
      variables: {},
      mutation: {
        response: orderCreateInlineNullOrderResult.payload,
      },
    });
    await writeJson(orderCreateMissingOrderFixturePath, {
      query: orderCreateMissingOrderProbe.replace(/^#graphql\n/, '').trim(),
      variables: {},
      mutation: {
        response: orderCreateMissingOrderResult.payload,
      },
    });
    if (createdOrderId) {
      await writeJson(orderCreateParityFixturePath, {
        variables: orderCreateVariables,
        mutation: {
          response: orderCreateResult.payload,
        },
        downstreamRead: {
          variables: { id: createdOrderId },
          response: orderReadAfterCreateResult?.payload ?? null,
        },
      });
      if (orderUpdateLiveVariablesForRun && orderUpdateLiveResult) {
        await writeJson(orderUpdateLiveParityFixturePath, {
          variables: orderUpdateLiveVariablesForRun,
          mutation: {
            response: orderUpdateLiveResult.payload,
          },
          downstreamRead: {
            variables: { id: createdOrderId },
            response: orderReadAfterUpdateResult?.payload ?? null,
          },
        });
      }
    }
    await writeJson(draftOrderCreateInlineMissingInputFixturePath, {
      query: draftOrderCreateInlineMissingInputProbe.replace(/^#graphql\n/, '').trim(),
      variables: {},
      mutation: {
        response: draftOrderCreateInlineMissingInputResult.payload,
      },
    });
    await writeJson(draftOrderCreateInlineNullInputFixturePath, {
      query: draftOrderCreateInlineNullInputProbe.replace(/^#graphql\n/, '').trim(),
      variables: {},
      mutation: {
        response: draftOrderCreateInlineNullInputResult.payload,
      },
    });
    await writeJson(draftOrderCreateMissingInputFixturePath, {
      query: draftOrderCreateMissingInputProbe.replace(/^#graphql\n/, '').trim(),
      variables: {},
      mutation: {
        response: draftOrderCreateMissingInputResult.payload,
      },
    });
    if (createdDraftOrderId) {
      await writeJson(draftOrderCreateParityFixturePath, {
        variables: draftOrderCreateVariables,
        mutation: {
          response: draftOrderCreateResult.payload,
        },
        downstreamRead: {
          variables: { id: createdDraftOrderId },
          response: draftOrderDetailResult?.payload ?? null,
        },
      });
      await writeJson(draftOrderDetailFixturePath, {
        variables: { id: createdDraftOrderId },
        response: draftOrderDetailResult?.payload ?? null,
      });
    }
    await writeJson(draftOrdersCatalogFixturePath, {
      variables: draftOrdersCatalogVariables,
      response: draftOrdersCatalogResult.payload,
    });
    await writeJson(draftOrdersCountFixturePath, {
      variables: draftOrdersCountVariables,
      response: draftOrdersCountResult.payload,
    });
    await writeJson(draftOrdersInvalidEmailQueryFixturePath, {
      variables: draftOrdersInvalidEmailQueryVariables,
      response: draftOrdersInvalidEmailQueryResult.payload,
    });
    await writeJson(draftOrderCompleteInlineMissingIdFixturePath, {
      query: draftOrderCompleteInlineMissingIdProbe.replace(/^#graphql\n/, '').trim(),
      variables: {},
      mutation: {
        response: draftOrderCompleteInlineMissingIdResult.payload,
      },
    });
    await writeJson(draftOrderCompleteInlineNullIdFixturePath, {
      query: draftOrderCompleteInlineNullIdProbe.replace(/^#graphql\n/, '').trim(),
      variables: {},
      mutation: {
        response: draftOrderCompleteInlineNullIdResult.payload,
      },
    });
    await writeJson(draftOrderCompleteMissingIdFixturePath, {
      query: draftOrderCompleteMissingIdProbe.replace(/^#graphql\n/, '').trim(),
      variables: draftOrderCompleteMissingIdVariables,
      mutation: {
        response: draftOrderCompleteMissingIdResult.payload,
      },
    });
    if (completedDraftOrderId && completedOrderId) {
      await writeJson(draftOrderCompleteParityFixturePath, {
        setup: {
          draftOrderCreate: {
            variables: draftOrderCreateVariables,
            mutation: {
              response: draftOrderCreateResult.payload,
            },
            downstreamRead: {
              variables: { id: createdDraftOrderId },
              response: draftOrderDetailResult?.payload ?? null,
            },
          },
        },
        variables: draftOrderCompleteVariables,
        mutation: {
          response: draftOrderCompleteResult.payload,
        },
        downstreamRead: {
          variables: { id: completedDraftOrderId },
          response: draftOrderReadAfterCompleteResult?.payload ?? null,
        },
        downstreamOrderRead: {
          variables: { id: completedOrderId },
          response: orderReadAfterDraftOrderCompleteResult?.payload ?? null,
        },
      });
    }
    await writeJson(orderEditBeginMissingIdFixturePath, {
      query: orderEditBeginMissingIdProbe.replace(/^#graphql\n/, '').trim(),
      variables: orderEditBeginMissingIdVariables,
      mutation: {
        response: orderEditBeginMissingIdResult.payload,
      },
    });
    await writeJson(orderEditAddVariantMissingIdFixturePath, {
      query: orderEditAddVariantMissingIdProbe.replace(/^#graphql\n/, '').trim(),
      variables: orderEditAddVariantMissingIdVariables,
      mutation: {
        response: orderEditAddVariantMissingIdResult.payload,
      },
    });
    await writeJson(orderEditSetQuantityMissingIdFixturePath, {
      query: orderEditSetQuantityMissingIdProbe.replace(/^#graphql\n/, '').trim(),
      variables: orderEditSetQuantityMissingIdVariables,
      mutation: {
        response: orderEditSetQuantityMissingIdResult.payload,
      },
    });
    await writeJson(orderEditCommitMissingIdFixturePath, {
      query: orderEditCommitMissingIdProbe.replace(/^#graphql\n/, '').trim(),
      variables: orderEditCommitMissingIdVariables,
      mutation: {
        response: orderEditCommitMissingIdResult.payload,
      },
    });
  }

  const draftOrderCreateAccessSummary = formatRequiredAccessSummary(
    parityMetadata.draftOrderCreate?.blocker?.details,
    draftOrderCreateResult.payload?.errors?.[0],
  );
  const draftOrderCompleteAccessSummary = formatRequiredAccessSummary(
    parityMetadata.draftOrderComplete?.blocker?.details,
    draftOrderCompleteResult.payload?.errors?.[0],
  );
  const orderEditBeginAccessSummary = formatRequiredAccessSummary(
    parityMetadata.orderEditBegin?.blocker?.details,
    orderEditBeginResult.payload?.errors?.[0],
  );
  const orderEditAddVariantAccessSummary = formatRequiredAccessSummary(
    parityMetadata.orderEditAddVariant?.blocker?.details,
    orderEditAddVariantResult.payload?.errors?.[0],
  );
  const orderEditSetQuantityAccessSummary = formatRequiredAccessSummary(
    parityMetadata.orderEditSetQuantity?.blocker?.details,
    orderEditSetQuantityResult.payload?.errors?.[0],
  );
  const orderEditCommitAccessSummary = formatRequiredAccessSummary(
    parityMetadata.orderEditCommit?.blocker?.details,
    orderEditCommitResult.payload?.errors?.[0],
  );
  const fulfillmentTrackingInfoUpdateAccessSummary = formatRequiredAccessSummary(
    parityMetadata.fulfillmentTrackingInfoUpdate?.blocker?.details,
    fulfillmentTrackingInfoUpdateResult.payload?.errors?.[0],
  );
  const fulfillmentCancelAccessSummary = formatRequiredAccessSummary(
    parityMetadata.fulfillmentCancel?.blocker?.details,
    fulfillmentCancelResult.payload?.errors?.[0],
    'Shopify did not return a narrower required-scope string in the current payload',
  );

  const creationNote = authRegressed
    ? `# Order creation conformance blocker

## What this run checked

Refreshed the current orders-domain creation probes on \`${storeDomain}\` using the repo conformance credential.

- \`orderCreate\`
- \`draftOrderCreate\`
- \`draftOrderComplete\`
- \`corepack pnpm conformance:capture-orders\`

## Current credential summary

- credential family: \`${credential.family}\`
- header mode: \`${credential.headerMode}\`
- ${credential.summary}

${renderManualStoreAuthSection(manualStoreAuthSummary)}

## Current run summary

- current run is auth-regressed before the family-specific creation roots can be reprobed
- live probe failure: \`401\` / \`${authFailureMessage}\`
- the checked-in fixtures are the last verified live references and should not be overwritten with \`401\` payloads during this regression

### \`orderCreate\`
- result on this run: auth-regressed before the root-specific create path could be reprobed
- last verified happy-path fixture: \`${orderCreateParityFixturePath}\`
- the checked-in fixture preserves immediate \`order(id:)\` read-after-write visibility

### \`draftOrderCreate\`
- result on this run: auth-regressed before the root-specific create path could be reprobed
- last verified happy-path fixture: \`${draftOrderCreateParityFixturePath}\`
- immediate downstream detail fixture: \`${draftOrderDetailFixturePath}\`

### \`draftOrderComplete\`
- result on this run: auth-regressed before the root-specific completion blocker could be reprobed
- last verified family-specific access-denied evidence: ${parityMetadata.draftOrderComplete?.blocker?.details?.failingMessage ?? 'missing blocker metadata'}
- required access summary: ${draftOrderCompleteAccessSummary}

## Practical interpretation

- this auth regression does **not** invalidate the last verified merchant-facing create fixtures or the existing local runtime slices they back
- the remaining creation-family live blocker after auth is repaired is still \`draftOrderComplete\` requiring write access that can mark as paid or set payment terms

## Recommended next step

1. run \`corepack pnpm conformance:refresh-auth\`
2. if refresh returns \`invalid_request\` / \`This request requires an active refresh_token\`, stop retrying the dead saved grant and generate a fresh manual store-auth link before continuing
3. rerun:\n   - \`corepack pnpm conformance:probe\`\n   - \`corepack pnpm conformance:capture-orders\`

Refresh this note with \`corepack pnpm conformance:capture-orders\` after any credential or store-state change.`
    : `# Order creation conformance blocker

## What this run checked

Refreshed the current orders-domain creation probes on \`${storeDomain}\` using the repo conformance credential.

- \`orderCreate\`
- \`draftOrderCreate\`
- \`draftOrderComplete\`
- \`corepack pnpm conformance:capture-orders\`

## Current credential summary

- credential family: \`${credential.family}\`
- header mode: \`${credential.headerMode}\`
- ${credential.summary}

${renderManualStoreAuthSection(manualStoreAuthSummary)}

## Current run summary

### \`orderCreate\`
- result: ${createdOrderId ? 'captured success on the current repo credential' : 'not freshly captured on this run'}
- checked-in happy-path fixture: \`${orderCreateParityFixturePath}\`
- the checked-in fixture preserves immediate \`order(id:)\` read-after-write visibility
${createdOrderId ? '' : `- exact message: ${orderCreateResult.payload?.errors?.[0]?.message ?? 'missing error payload'}\n- required access summary: ${orderCreateResult.payload?.errors?.[0]?.extensions?.requiredAccess ?? 'missing requiredAccess payload'}`}

### \`draftOrderCreate\`
- result: ${createdDraftOrderId ? 'captured success on the current repo credential' : 'access denied on the current repo credential'}
- checked-in happy-path fixture: \`${draftOrderCreateParityFixturePath}\`
- immediate downstream detail fixture: \`${draftOrderDetailFixturePath}\`
${
  createdDraftOrderId
    ? '- the checked-in fixture preserves immediate `draftOrder(id:)` read-after-write visibility'
    : `- exact message: ${draftOrderCreateResult.payload?.errors?.[0]?.message ?? 'missing error payload'}\n- required access summary: ${draftOrderCreateAccessSummary}`
}

### \`draftOrderComplete\`
- result: ${completedDraftOrderId && completedOrderId ? 'captured success on the current repo credential' : 'access denied or not freshly captured on the current repo credential'}
${
  completedDraftOrderId && completedOrderId
    ? `- checked-in happy-path fixture: \`${draftOrderCompleteParityFixturePath}\`\n- immediate downstream draft detail and order reads were captured`
    : `- exact message: ${draftOrderCompleteResult.payload?.errors?.[0]?.message ?? parityMetadata.draftOrderComplete?.blocker?.details?.failingMessage ?? 'missing error payload'}\n- required access summary: ${draftOrderCompleteAccessSummary}`
}

## Repo impact

- \`${orderCreateParityFixturePath}\` and \`${draftOrderCreateParityFixturePath}\` remain the live references for the current happy-path creation slices
- the checked-in fixtures continue to back immediate \`order(id:)\` and \`draftOrder(id:)\` read-after-write visibility
${completedDraftOrderId && completedOrderId ? `- \`${draftOrderCompleteParityFixturePath}\` now backs successful draft-to-order completion plus downstream reads` : '- the creation family still keeps `draftOrderComplete` blocked until write access can mark as paid or set payment terms'}

Refresh this note with \`corepack pnpm conformance:capture-orders\` after any credential or store-state change.`;

  await mkdir(path.dirname(orderCreationBlockerNotePath), { recursive: true });
  await writeFile(orderCreationBlockerNotePath, `${creationNote}\n`);

  const orderEditingNote = `# Order editing conformance blocker

## What this run checked

Refreshed the first order-editing mutation probes on \`${storeDomain}\` using the current repo conformance credential.

- \`orderEditBegin\` — the session-start root for Shopify's order-edit flow
- \`orderEditAddVariant\` — the first merchant-realistic edit step for adding sellable items to a calculated order
- \`orderEditSetQuantity\` — the quantity-adjustment root for calculated order line items
- \`orderEditCommit\` — the commit/apply root that would eventually need local downstream order visibility after staged edits
- \`corepack pnpm conformance:capture-orders\`

## Current credential summary

- credential family: \`${credential.family}\`
- header mode: \`${credential.headerMode}\`
- ${credential.summary}

${renderManualStoreAuthSection(manualStoreAuthSummary)}

## Live blocker evidence for the order-edit family

### \`orderEditBegin\`

- exact message: ${orderEditBeginResult.payload?.errors?.[0]?.message ?? parityMetadata.orderEditBegin?.blocker?.details?.failingMessage ?? 'missing error payload'}
- required access summary: ${orderEditBeginAccessSummary}

### \`orderEditAddVariant\`

- exact message: ${orderEditAddVariantResult.payload?.errors?.[0]?.message ?? parityMetadata.orderEditAddVariant?.blocker?.details?.failingMessage ?? 'missing error payload'}
- required access summary: ${orderEditAddVariantAccessSummary}

### \`orderEditSetQuantity\`

- exact message: ${orderEditSetQuantityResult.payload?.errors?.[0]?.message ?? parityMetadata.orderEditSetQuantity?.blocker?.details?.failingMessage ?? 'missing error payload'}
- required access summary: ${orderEditSetQuantityAccessSummary}

### \`orderEditCommit\`

- exact message: ${orderEditCommitResult.payload?.errors?.[0]?.message ?? parityMetadata.orderEditCommit?.blocker?.details?.failingMessage ?? 'missing error payload'}
- required access summary: ${orderEditCommitAccessSummary}

## Practical interpretation

- the proxy already supports a first local calculated-order edit flow for synthetic/local orders in snapshot mode and live-hybrid mode
- safe missing-\`$id\` GraphQL validation coverage is now captured for \`orderEditBegin\`, \`orderEditAddVariant\`, \`orderEditSetQuantity\`, and \`orderEditCommit\`
- the remaining gap is live Shopify parity for non-local orders; happy-path Shopify probes for all four initial roots still hit \`write_order_edits\` on this host before the resolver reveals broader session-shape semantics

## Practical next step for order-edit parity

1. keep the checked-in first local calculated-order edit flow for synthetic/local orders as-is
2. provision a credential/install with \`write_order_edits\`
3. rerun:
   - \`corepack pnpm conformance:probe\`
   - \`corepack pnpm conformance:capture-orders\`
4. once the roots are writable, capture the smallest safe sequence in order:
   - \`orderEditBegin\`
   - \`orderEditAddVariant\`
   - \`orderEditSetQuantity\`
   - \`orderEditCommit\`
5. only after live evidence exists for non-local orders should the proxy broaden the calculated-order runtime beyond the current synthetic/local slice
`;

  await mkdir(path.dirname(orderEditingBlockerNotePath), { recursive: true });
  await writeFile(orderEditingBlockerNotePath, `${orderEditingNote}\n`);

  if (authRegressed) {
    const draftOrdersCatalogLastVerified =
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/draft-orders-catalog.json';
    const draftOrdersCountLastVerified =
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/draft-orders-count.json';
    const draftOrderReadNote = `# Draft-order read conformance blocker

## What this run checked

Refreshed the first draft-order read probes on \`${storeDomain}\` using the current repo conformance credential.

- \`draftOrder(id: ...)\` — direct draft-order detail read surface that downstream local staging would need immediately after a safe draft-order create/edit flow
- \`draftOrders(first: ...)\` — draft-order catalog surface for merchant list views and local overlay replay
- \`draftOrdersCount(query:)\` — draft-order count surface that needs to stay aligned with draft-order catalog filtering later
- \`corepack pnpm conformance:capture-orders\`

## Current credential summary

- credential family: \`${credential.family}\`
- header mode: \`${credential.headerMode}\`
- ${credential.summary}

${renderManualStoreAuthSection(manualStoreAuthSummary)}

## Current run summary

- current run is auth-regressed before the draft-order read roots could be reprobed
- live probe failure: \`401\` / \`${authFailureMessage}\`
- the checked-in fixtures are the last verified live references and should not be overwritten with \`401\` payloads during this regression

## Direct-order read baseline that remains safe

The current repo still keeps the last verified direct-order empty-state baseline in \`${orderEmptyStateFixturePath}\`:

- \`order(id: "gid://shopify/Order/0")\` -> \`null\`
- \`orders(first: 1, sortKey: CREATED_AT, reverse: true)\` -> empty connection with null cursors
- \`ordersCount\` -> \`{ count: 0, precision: EXACT }\`

## Last verified draft-order catalog/count evidence

### \`draftOrders\`

- result on this run: auth-regressed before the root could be reprobed
- last verified captured fixture: \`${draftOrdersCatalogLastVerified}\`

### \`draftOrdersCount\`

- result on this run: auth-regressed before the root could be reprobed
- last verified captured fixture: \`${draftOrdersCountLastVerified}\`

## Practical interpretation

- local proxy/runtime already supports the narrow unfiltered staged synthetic \`draftOrders\` / \`draftOrdersCount\` slice in snapshot mode and live-hybrid mode
- the remaining gap is auth repair for rerunning the now-captured live baseline, not missing draft-order catalog/count fixtures

## Recommended next step

1. run \`corepack pnpm conformance:refresh-auth\`
2. if refresh returns \`invalid_request\` / \`This request requires an active refresh_token\`, stop retrying the dead saved grant and generate a fresh manual store-auth link before continuing
3. rerun:
   - \`corepack pnpm conformance:probe\`
   - \`corepack pnpm conformance:capture-orders\`
4. once auth is healthy again, refresh \`${draftOrdersCatalogLastVerified}\` and \`${draftOrdersCountLastVerified}\`

Refresh this note with \`corepack pnpm conformance:capture-orders\` after any credential or store-state change.`;

    await mkdir(path.dirname(draftOrderReadBlockerNotePath), { recursive: true });
    await writeFile(draftOrderReadBlockerNotePath, `${draftOrderReadNote}\n`);
  } else {
    await rm(draftOrderReadBlockerNotePath, { force: true });
  }

  if (fulfillmentLifecycleCaptured) {
    await rm(fulfillmentLifecycleBlockerNotePath, { force: true });
  } else {
    const fulfillmentLifecycleNote = `# Fulfillment lifecycle conformance blocker

## What this run checked

Refreshed the next fulfillment lifecycle probes on \`${storeDomain}\` using the current repo conformance credential.

- \`fulfillmentTrackingInfoUpdate\` — the first merchant-facing fulfillment lifecycle root for updating tracking details after a fulfillment exists
- \`fulfillmentCancel\` — the adjacent cancellation root for reversing a fulfillment lifecycle step
- \`corepack pnpm conformance:capture-orders\`

## Current credential summary

- credential family: \`${credential.family}\`
- header mode: \`${credential.headerMode}\`
- ${credential.summary}

${renderManualStoreAuthSection(manualStoreAuthSummary)}

## Current run summary

### Captured pre-access validation slices

- \`fulfillmentTrackingInfoUpdate\` inline missing \`fulfillmentId\`
  - exact message: ${fulfillmentTrackingInfoUpdateInlineMissingIdResult.payload?.errors?.[0]?.message ?? 'missing error payload'}
- \`fulfillmentTrackingInfoUpdate\` inline \`fulfillmentId: null\`
  - exact message: ${fulfillmentTrackingInfoUpdateInlineNullIdResult.payload?.errors?.[0]?.message ?? 'missing error payload'}
- \`fulfillmentTrackingInfoUpdate\` missing \`$fulfillmentId\`
  - exact message: ${fulfillmentTrackingInfoUpdateMissingIdResult.payload?.errors?.[0]?.message ?? 'missing error payload'}
- \`fulfillmentCancel\` inline missing \`id\`
  - exact message: ${fulfillmentCancelInlineMissingIdResult.payload?.errors?.[0]?.message ?? 'missing error payload'}
- \`fulfillmentCancel\` inline \`id: null\`
  - exact message: ${fulfillmentCancelInlineNullIdResult.payload?.errors?.[0]?.message ?? 'missing error payload'}
- \`fulfillmentCancel\` missing \`$id\`
  - exact message: ${fulfillmentCancelMissingIdResult.payload?.errors?.[0]?.message ?? 'missing error payload'}

### Remaining live happy-path blockers

### \`fulfillmentTrackingInfoUpdate\`
- result: access denied on the current repo credential
- exact message: ${fulfillmentTrackingInfoUpdateResult.payload?.errors?.[0]?.message ?? parityMetadata.fulfillmentTrackingInfoUpdate?.blocker?.details?.failingMessage ?? 'missing error payload'}
- required access summary: ${fulfillmentTrackingInfoUpdateAccessSummary}

### \`fulfillmentCancel\`
- result: access denied on the current repo credential
- exact message: ${fulfillmentCancelResult.payload?.errors?.[0]?.message ?? parityMetadata.fulfillmentCancel?.blocker?.details?.failingMessage ?? 'missing error payload'}
- required access summary: ${fulfillmentCancelAccessSummary}

## Practical interpretation

The first fulfillment-domain increment now includes evidence-backed GraphQL validation slices for both \`fulfillmentTrackingInfoUpdate\` and \`fulfillmentCancel\`, alongside the earlier captured \`fulfillmentCreate\` invalid-id branch. The broader fulfillment lifecycle happy paths remain blocked on live access under the current repo credential.

Practical next step for fulfillment lifecycle parity:

1. provision a credential/install that can write the relevant fulfillment family
2. rerun:
   - \`corepack pnpm conformance:probe\`
   - \`corepack pnpm conformance:capture-orders\`
3. once the roots are reachable, capture the smallest safe fulfillment lifecycle sequence in order:
   - \`fulfillmentTrackingInfoUpdate\`
   - \`fulfillmentCancel\`
4. only after live write evidence exists should the proxy start staging tracking-update/cancel semantics or downstream fulfillment read effects locally
`;

    await mkdir(path.dirname(fulfillmentLifecycleBlockerNotePath), { recursive: true });
    await writeFile(fulfillmentLifecycleBlockerNotePath, `${fulfillmentLifecycleNote}\n`);
  }

  console.log(
    JSON.stringify(
      {
        ok: true,
        credential,
        manualStoreAuthSummary,
        createdOrderId,
        createdDraftOrderId,
        orderEditBeginMissingIdFixturePath,
        orderEditAddVariantMissingIdFixturePath,
        orderEditSetQuantityMissingIdFixturePath,
        orderEditCommitMissingIdFixturePath,
        orderCreateParityFixturePath,
        orderUpdateLiveParityFixturePath,
        draftOrderCreateParityFixturePath,
        draftOrderDetailFixturePath,
        draftOrdersCatalogFixturePath,
        draftOrdersCountFixturePath,
        draftOrdersInvalidEmailQueryFixturePath,
        orderCreationBlockerNotePath,
        orderEditingBlockerNotePath,
        draftOrderReadBlockerNotePath: authRegressed ? draftOrderReadBlockerNotePath : null,
        fulfillmentLifecycleCaptured,
        fulfillmentTrackingInfoUpdateParityFixturePath: fulfillmentLifecycleCaptured
          ? fulfillmentTrackingInfoUpdateParityFixturePath
          : null,
        fulfillmentCancelParityFixturePath: fulfillmentLifecycleCaptured ? fulfillmentCancelParityFixturePath : null,
        fulfillmentLifecycleBlockerNotePath: fulfillmentLifecycleCaptured ? null : fulfillmentLifecycleBlockerNotePath,
      },
      null,
      2,
    ),
  );
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});

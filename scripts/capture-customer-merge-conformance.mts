// @ts-nocheck
/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'customers');
const captureMode =
  process.env.SHOPIFY_CUSTOMER_MERGE_CAPTURE_MODE === 'attached-resources' ? 'attached-resources' : 'base';
const capturesAttachedResources = captureMode === 'attached-resources';
const { runGraphqlRequest: runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

function assertNoTopLevelErrors(result, context) {
  if (result.status < 200 || result.status >= 300 || result.payload?.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

const customerSlice = `
  id
  firstName
  lastName
  displayName
  email
  note
  tags
  numberOfOrders
  defaultEmailAddress { emailAddress }
  defaultPhoneNumber { phoneNumber }
  defaultAddress { id address1 city provinceCode countryCodeV2 zip }
  addressesV2(first: 10) {
    nodes { id address1 city provinceCode countryCodeV2 zip }
    pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
  }
  metafields(first: 10) {
    nodes { id namespace key type value }
    pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
  }
  orders(first: 10, sortKey: CREATED_AT, reverse: true) {
    nodes { id name email createdAt }
    pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
  }
  lastOrder { id name email createdAt }
  createdAt
  updatedAt
`;

const accessScopesQuery = `#graphql
  query CustomerMergeAccessScopes {
    currentAppInstallation {
      accessScopes {
        handle
      }
    }
  }
`;

const createCustomerMutation = `#graphql
  mutation CustomerMergeSeedCreate($input: CustomerInput!) {
    customerCreate(input: $input) {
      customer {
        ${customerSlice}
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const createCustomerAddressMutation = `#graphql
  mutation CustomerMergeAddressCreate($customerId: ID!, $address: MailingAddressInput!, $setAsDefault: Boolean) {
    customerAddressCreate(customerId: $customerId, address: $address, setAsDefault: $setAsDefault) {
      address {
        id
        address1
        city
        provinceCode
        countryCodeV2
        zip
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const orderCreateMutation = `#graphql
  mutation CustomerMergeOrderCreate($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
    orderCreate(order: $order, options: $options) {
      order {
        id
        name
        email
        createdAt
        customer { id email displayName }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const draftOrderCreateMutation = `#graphql
  mutation CustomerMergeDraftOrderCreate($input: DraftOrderInput!) {
    draftOrderCreate(input: $input) {
      draftOrder {
        id
        name
        status
        email
        customer { id email displayName }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const draftOrderDeleteMutation = `#graphql
  mutation CustomerMergeDraftOrderDelete($input: DraftOrderDeleteInput!) {
    draftOrderDelete(input: $input) {
      deletedId
      userErrors {
        field
        message
      }
    }
  }
`;

const previewQuery = `#graphql
  query CustomerMergePreviewParity($one: ID!, $two: ID!, $override: CustomerMergeOverrideFields) {
    customerMergePreview(customerOneId: $one, customerTwoId: $two, overrideFields: $override) {
      resultingCustomerId
      defaultFields {
        firstName
        lastName
        displayName
        email { emailAddress }
        phoneNumber { phoneNumber }
        note
        tags
      }
      alternateFields {
        firstName
        lastName
        email { emailAddress }
        phoneNumber { phoneNumber }
      }
      blockingFields {
        note
        tags
      }
      customerMergeErrors {
        errorFields
        message
      }
    }
  }
`;

const mergeMutation = `#graphql
  mutation CustomerMergeParity($one: ID!, $two: ID!, $override: CustomerMergeOverrideFields) {
    customerMerge(customerOneId: $one, customerTwoId: $two, overrideFields: $override) {
      resultingCustomerId
      job {
        id
        done
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const missingArgumentMutation = `#graphql
  mutation CustomerMergeMissingArgument($one: ID!) {
    customerMerge(customerOneId: $one) {
      resultingCustomerId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const blankLiteralIdsMutation = `#graphql
  mutation CustomerMergeBlankLiteralIds {
    customerMerge(customerOneId: "", customerTwoId: "") {
      resultingCustomerId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const jobStatusQuery = `#graphql
  query CustomerMergeJobStatusParity($jobId: ID!) {
    customerMergeJobStatus(jobId: $jobId) {
      jobId
      resultingCustomerId
      status
      customerMergeErrors {
        errorFields
        message
      }
    }
  }
`;

const downstreamQuery = `#graphql
  query CustomerMergeDownstreamParity($one: ID!, $two: ID!, $emailOne: String!, $emailTwo: String!, $jobId: ID!) {
    source: customer(id: $one) {
      ${customerSlice}
    }
    result: customer(id: $two) {
      ${customerSlice}
    }
    byEmailOne: customerByIdentifier(identifier: { emailAddress: $emailOne }) {
      id
      email
      defaultEmailAddress { emailAddress }
    }
    byEmailTwo: customerByIdentifier(identifier: { emailAddress: $emailTwo }) {
      id
      email
      defaultEmailAddress { emailAddress }
    }
    customersCount {
      count
      precision
    }
    mergeStatus: customerMergeJobStatus(jobId: $jobId) {
      jobId
      resultingCustomerId
      status
      customerMergeErrors {
        errorFields
        message
      }
    }
  }
`;

const attachedResourcesQuery = `#graphql
  query CustomerMergeAttachedResources($one: ID!, $two: ID!, $emailOne: String!, $emailTwo: String!) {
    source: customer(id: $one) {
      ${customerSlice}
    }
    result: customer(id: $two) {
      ${customerSlice}
    }
    byEmailOne: customerByIdentifier(identifier: { emailAddress: $emailOne }) {
      id
      email
      defaultAddress { id address1 city }
      addressesV2(first: 10) { nodes { id address1 city } }
      metafields(first: 10) { nodes { namespace key type value } }
      orders(first: 10, sortKey: CREATED_AT, reverse: true) { nodes { id name } }
      lastOrder { id name }
    }
    byEmailTwo: customerByIdentifier(identifier: { emailAddress: $emailTwo }) {
      id
      email
      defaultAddress { id address1 city }
      addressesV2(first: 10) { nodes { id address1 city } }
      metafields(first: 10) { nodes { namespace key type value } }
      orders(first: 10, sortKey: CREATED_AT, reverse: true) { nodes { id name } }
      lastOrder { id name }
    }
    customers(first: 10, query: "tag:har-291-merge") {
      nodes {
        id
        email
        tags
        numberOfOrders
        defaultAddress { id address1 city }
        addressesV2(first: 10) { nodes { id address1 city } }
        metafields(first: 10) { nodes { namespace key value } }
        orders(first: 10, sortKey: CREATED_AT, reverse: true) { nodes { id name } }
        lastOrder { id name }
      }
    }
    customersCount(query: "tag:har-291-merge") {
      count
      precision
    }
  }
`;

const deleteCustomerMutation = `#graphql
  mutation CustomerMergeCleanup($input: CustomerDeleteInput!) {
    customerDelete(input: $input) {
      deletedCustomerId
      userErrors {
        field
        message
      }
    }
  }
`;

async function main() {
  await mkdir(outputDir, { recursive: true });

  const accessScopes = await runGraphql(accessScopesQuery, {});
  assertNoTopLevelErrors(accessScopes, 'currentAppInstallation.accessScopes');
  const scopeHandles = new Set(
    accessScopes.payload?.data?.currentAppInstallation?.accessScopes?.map((scope) => scope.handle) ?? [],
  );
  for (const requiredScope of ['read_customer_merge', 'write_customer_merge']) {
    if (!scopeHandles.has(requiredScope)) {
      throw new Error(`Customer merge conformance requires ${requiredScope}.`);
    }
  }

  const stamp = Date.now();
  const sourcePhone = `+1647555${String(stamp).slice(-4)}`;
  const resultPhone = `+1647666${String(stamp).slice(-4)}`;
  const oneVariables = {
    input: {
      email: `hermes-merge-one-${stamp}@example.com`,
      phone: sourcePhone,
      firstName: 'Merge',
      lastName: 'One',
      note: 'merge-one-note',
      tags: ['har-291-merge', 'merge-one', `merge-${stamp}`],
      ...(capturesAttachedResources
        ? {
            metafields: [
              {
                namespace: 'custom',
                key: 'source_only',
                type: 'single_line_text_field',
                value: `source-${stamp}`,
              },
              {
                namespace: 'custom',
                key: 'conflict',
                type: 'single_line_text_field',
                value: `source-conflict-${stamp}`,
              },
            ],
          }
        : {}),
    },
  };
  const twoVariables = {
    input: {
      email: `hermes-merge-two-${stamp}@example.com`,
      phone: resultPhone,
      firstName: 'Merge',
      lastName: 'Two',
      note: 'merge-two-note',
      tags: ['har-291-merge', 'merge-two', `merge-${stamp}`],
      ...(capturesAttachedResources
        ? {
            metafields: [
              {
                namespace: 'custom',
                key: 'result_only',
                type: 'single_line_text_field',
                value: `result-${stamp}`,
              },
              {
                namespace: 'custom',
                key: 'conflict',
                type: 'single_line_text_field',
                value: `result-conflict-${stamp}`,
              },
            ],
          }
        : {}),
    },
  };

  const createOne = await runGraphql(createCustomerMutation, oneVariables);
  assertNoTopLevelErrors(createOne, 'customerCreate one');
  const createTwo = await runGraphql(createCustomerMutation, twoVariables);
  assertNoTopLevelErrors(createTwo, 'customerCreate two');
  const customerOneId = createOne.payload?.data?.customerCreate?.customer?.id;
  const customerTwoId = createTwo.payload?.data?.customerCreate?.customer?.id;
  if (typeof customerOneId !== 'string' || typeof customerTwoId !== 'string') {
    throw new Error(
      `customerCreate did not return merge seed IDs: ${JSON.stringify({ createOne, createTwo }, null, 2)}`,
    );
  }

  const addressOneVariables = {
    customerId: customerOneId,
    address: {
      firstName: 'Source',
      lastName: 'Address',
      address1: '1 Source Merge St',
      city: 'Ottawa',
      provinceCode: 'ON',
      countryCode: 'CA',
      zip: 'K1A 0B1',
    },
    setAsDefault: true,
  };
  const addressTwoVariables = {
    customerId: customerTwoId,
    address: {
      firstName: 'Result',
      lastName: 'Address',
      address1: '2 Result Merge Ave',
      city: 'Toronto',
      provinceCode: 'ON',
      countryCode: 'CA',
      zip: 'M5H 2N2',
    },
    setAsDefault: true,
  };
  const createAddressOne = capturesAttachedResources
    ? await runGraphql(createCustomerAddressMutation, addressOneVariables)
    : null;
  if (createAddressOne) {
    assertNoTopLevelErrors(createAddressOne, 'customerAddressCreate one');
  }
  const createAddressTwo = capturesAttachedResources
    ? await runGraphql(createCustomerAddressMutation, addressTwoVariables)
    : null;
  if (createAddressTwo) {
    assertNoTopLevelErrors(createAddressTwo, 'customerAddressCreate two');
  }

  const orderVariables = {
    order: {
      customerId: customerOneId,
      email: oneVariables.input.email,
      note: 'HAR-291 customer merge source order',
      tags: ['har-291-merge', `merge-${stamp}`],
      test: true,
      currency: 'CAD',
      lineItems: [
        {
          title: 'HAR-291 merge source order item',
          quantity: 1,
          priceSet: {
            shopMoney: {
              amount: '11.00',
              currencyCode: 'CAD',
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
  const orderCreate = capturesAttachedResources ? await runGraphql(orderCreateMutation, orderVariables) : null;

  const draftOrderVariables = {
    input: {
      purchasingEntity: {
        customerId: customerOneId,
      },
      email: oneVariables.input.email,
      note: 'HAR-291 customer merge source draft order',
      tags: ['har-291-merge', `merge-${stamp}`],
      lineItems: [
        {
          title: 'HAR-291 merge source draft item',
          quantity: 1,
          originalUnitPrice: '12.00',
          requiresShipping: false,
          taxable: false,
        },
      ],
    },
  };
  const draftOrderCreate = capturesAttachedResources
    ? await runGraphql(draftOrderCreateMutation, draftOrderVariables)
    : null;
  const draftOrderId = draftOrderCreate?.payload?.data?.draftOrderCreate?.draftOrder?.id;

  const attachedBeforeMergeVariables = {
    one: customerOneId,
    two: customerTwoId,
    emailOne: oneVariables.input.email,
    emailTwo: twoVariables.input.email,
  };
  const attachedBeforeMerge = capturesAttachedResources
    ? await runGraphql(attachedResourcesQuery, attachedBeforeMergeVariables)
    : null;
  if (attachedBeforeMerge) {
    assertNoTopLevelErrors(attachedBeforeMerge, 'customerMerge attached resources before merge');
  }

  const overrideFields = {
    customerIdOfEmailToKeep: customerTwoId,
    customerIdOfPhoneNumberToKeep: customerOneId,
    customerIdOfFirstNameToKeep: customerOneId,
    customerIdOfLastNameToKeep: customerTwoId,
    note: 'merged note',
    tags: ['har-291-merge', 'merged', `merge-${stamp}`],
  };
  const mergeVariables = {
    one: customerOneId,
    two: customerTwoId,
    override: overrideFields,
  };

  const missingArgument = await runGraphql(missingArgumentMutation, { one: customerOneId });
  const blankLiteralIds = await runGraphql(blankLiteralIdsMutation, {});
  const selfPreview = await runGraphql(previewQuery, { one: customerOneId, two: customerOneId });
  const selfMerge = await runGraphql(mergeMutation, { one: customerOneId, two: customerOneId });
  assertNoTopLevelErrors(selfMerge, 'customerMerge self validation');
  const unknownMerge = await runGraphql(mergeMutation, {
    one: customerOneId,
    two: 'gid://shopify/Customer/999999999999999',
  });
  assertNoTopLevelErrors(unknownMerge, 'customerMerge unknown validation');

  const preview = await runGraphql(previewQuery, mergeVariables);
  assertNoTopLevelErrors(preview, 'customerMergePreview');
  const merge = await runGraphql(mergeMutation, mergeVariables);
  assertNoTopLevelErrors(merge, 'customerMerge');

  const jobId = merge.payload?.data?.customerMerge?.job?.id;
  if (typeof jobId !== 'string') {
    throw new Error(`customerMerge did not return a job id: ${JSON.stringify(merge.payload, null, 2)}`);
  }

  let status = await runGraphql(jobStatusQuery, { jobId });
  assertNoTopLevelErrors(status, 'customerMergeJobStatus');
  const statusPolls = [status.payload];
  for (
    let attempt = 0;
    attempt < 10 && status.payload?.data?.customerMergeJobStatus?.status === 'IN_PROGRESS';
    attempt += 1
  ) {
    await new Promise((resolve) => setTimeout(resolve, 1000));
    status = await runGraphql(jobStatusQuery, { jobId });
    assertNoTopLevelErrors(status, 'customerMergeJobStatus');
    statusPolls.push(status.payload);
  }

  const downstreamVariables = {
    one: customerOneId,
    two: customerTwoId,
    emailOne: oneVariables.input.email,
    emailTwo: twoVariables.input.email,
    jobId,
  };
  const downstreamRead = await runGraphql(downstreamQuery, downstreamVariables);
  assertNoTopLevelErrors(downstreamRead, 'customerMerge downstream read');
  const attachedAfterMerge = capturesAttachedResources
    ? await runGraphql(attachedResourcesQuery, attachedBeforeMergeVariables)
    : null;
  if (attachedAfterMerge) {
    assertNoTopLevelErrors(attachedAfterMerge, 'customerMerge attached resources after merge');
  }

  const draftOrderCleanup =
    typeof draftOrderId === 'string'
      ? await runGraphql(draftOrderDeleteMutation, { input: { id: draftOrderId } })
      : null;
  const cleanup = await runGraphql(deleteCustomerMutation, { input: { id: customerTwoId } });

  const capture = {
    accessScopes: accessScopes.payload,
    precondition: {
      createOne: {
        variables: oneVariables,
        response: createOne.payload,
      },
      createTwo: {
        variables: twoVariables,
        response: createTwo.payload,
      },
      ...(capturesAttachedResources
        ? {
            createAddressOne: {
              variables: addressOneVariables,
              response: createAddressOne?.payload,
            },
            createAddressTwo: {
              variables: addressTwoVariables,
              response: createAddressTwo?.payload,
            },
            orderCreate: {
              variables: orderVariables,
              response: orderCreate?.payload,
            },
            draftOrderCreate: {
              variables: draftOrderVariables,
              response: draftOrderCreate?.payload,
            },
            attachedBeforeMerge: {
              variables: attachedBeforeMergeVariables,
              response: attachedBeforeMerge?.payload,
            },
          }
        : {}),
    },
    preview: {
      variables: mergeVariables,
      response: preview.payload,
    },
    mutation: {
      variables: mergeVariables,
      response: merge.payload,
    },
    status: {
      variables: { jobId },
      response: status.payload,
    },
    statusPolls: statusPolls.map((response, index) => ({
      variables: { jobId },
      attempt: index,
      response,
    })),
    downstreamRead: {
      variables: downstreamVariables,
      proxyVariables: {
        ...downstreamVariables,
        jobId: { fromPrimaryProxyPath: '$.data.customerMerge.job.id' },
      },
      response: downstreamRead.payload,
    },
    ...(capturesAttachedResources
      ? {
          attachedAfterMerge: {
            variables: attachedBeforeMergeVariables,
            response: attachedAfterMerge?.payload,
          },
        }
      : {}),
    validation: {
      missingArgument: {
        variables: { one: customerOneId },
        response: missingArgument.payload,
      },
      blankLiteralIds: {
        variables: {},
        response: blankLiteralIds.payload,
      },
      selfPreview: {
        variables: { one: customerOneId, two: customerOneId },
        response: selfPreview.payload,
      },
      selfMerge: {
        variables: { one: customerOneId, two: customerOneId },
        response: selfMerge.payload,
      },
      unknownCustomer: {
        variables: { one: customerOneId, two: 'gid://shopify/Customer/999999999999999' },
        response: unknownMerge.payload,
      },
    },
    cleanup: {
      draftOrderDelete: draftOrderCleanup
        ? {
            variables: { input: { id: draftOrderId } },
            response: draftOrderCleanup.payload,
          }
        : null,
      variables: { input: { id: customerTwoId } },
      response: cleanup.payload,
    },
  };

  const outputFilename = capturesAttachedResources
    ? 'customer-merge-attached-resources-parity.json'
    : 'customer-merge-parity.json';
  const outputPath = path.join(outputDir, outputFilename);
  await writeFile(outputPath, `${JSON.stringify(capture, null, 2)}\n`);
  console.log(`Wrote ${outputPath}`);
}

await main();

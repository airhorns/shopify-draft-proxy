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
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion);
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
  defaultEmailAddress { emailAddress }
  defaultPhoneNumber { phoneNumber }
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
  const oneVariables = {
    input: {
      email: `hermes-merge-one-${stamp}@example.com`,
      firstName: 'Merge',
      lastName: 'One',
      note: 'merge-one-note',
      tags: ['merge-one', `merge-${stamp}`],
    },
  };
  const twoVariables = {
    input: {
      email: `hermes-merge-two-${stamp}@example.com`,
      firstName: 'Merge',
      lastName: 'Two',
      note: 'merge-two-note',
      tags: ['merge-two', `merge-${stamp}`],
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

  const overrideFields = {
    customerIdOfEmailToKeep: customerTwoId,
    customerIdOfFirstNameToKeep: customerOneId,
    customerIdOfLastNameToKeep: customerTwoId,
    note: 'merged note',
    tags: ['merged', `merge-${stamp}`],
  };
  const mergeVariables = {
    one: customerOneId,
    two: customerTwoId,
    override: overrideFields,
  };

  const missingArgument = await runGraphql(missingArgumentMutation, { one: customerOneId });
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
  for (
    let attempt = 0;
    attempt < 10 && status.payload?.data?.customerMergeJobStatus?.status === 'RUNNING';
    attempt += 1
  ) {
    await new Promise((resolve) => setTimeout(resolve, 500));
    status = await runGraphql(jobStatusQuery, { jobId });
    assertNoTopLevelErrors(status, 'customerMergeJobStatus');
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
    downstreamRead: {
      variables: downstreamVariables,
      proxyVariables: {
        ...downstreamVariables,
        jobId: { fromPrimaryProxyPath: '$.data.customerMerge.job.id' },
      },
      response: downstreamRead.payload,
    },
    validation: {
      missingArgument: {
        variables: { one: customerOneId },
        response: missingArgument.payload,
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
      variables: { input: { id: customerTwoId } },
      response: cleanup.payload,
    },
  };

  const outputPath = path.join(outputDir, 'customer-merge-parity.json');
  await writeFile(outputPath, `${JSON.stringify(capture, null, 2)}\n`);
  console.log(`Wrote ${outputPath}`);
}

await main();

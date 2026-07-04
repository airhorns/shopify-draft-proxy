// @ts-nocheck
/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { readFile, mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const scenarioId = 'customer-update-inline-address-id-semantics';
const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'customers');
const { runGraphqlRequest: runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const createMutation = await readFile(
  'config/parity-requests/customers/customer-update-inline-addresses-create.graphql',
  'utf8',
);
const updateMutation = await readFile(
  'config/parity-requests/customers/customer-update-inline-addresses-update.graphql',
  'utf8',
);
const downstreamReadQuery = await readFile(
  'config/parity-requests/customers/customer-update-inline-addresses-read.graphql',
  'utf8',
);

const deleteMutation = `#graphql
  mutation CustomerUpdateInlineAddressesDelete($input: CustomerDeleteInput!) {
    customerDelete(input: $input) {
      deletedCustomerId
      userErrors {
        field
        message
      }
    }
  }
`;

function assertNoTopLevelErrors(result, context) {
  if (result.status < 200 || result.status >= 300 || result.payload?.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function customerNodes(payload, pathLabel) {
  const nodes = payload?.customer?.addressesV2?.nodes ?? payload?.customerByIdentifier?.addressesV2?.nodes;
  if (!Array.isArray(nodes)) {
    throw new Error(`${pathLabel} did not return address nodes: ${JSON.stringify(payload, null, 2)}`);
  }
  return nodes;
}

function mutationNodes(payload, root, pathLabel) {
  const nodes = payload?.data?.[root]?.customer?.addressesV2?.nodes;
  if (!Array.isArray(nodes)) {
    throw new Error(`${pathLabel} did not return mutation address nodes: ${JSON.stringify(payload, null, 2)}`);
  }
  return nodes;
}

function assertNoUserErrors(payload, root, context) {
  const errors = payload?.data?.[root]?.userErrors;
  if (!Array.isArray(errors) || errors.length !== 0) {
    throw new Error(`${context} returned userErrors: ${JSON.stringify(errors, null, 2)}`);
  }
}

async function main() {
  await mkdir(outputDir, { recursive: true });

  const stamp = Date.now();
  const email = `hermes-inline-address-id-${stamp}@example.com`;
  const unknownAddressId = `gid://shopify/MailingAddress/${stamp}999999?model_name=CustomerAddress`;
  let createdCustomerId = null;

  try {
    const createVariables = {
      input: {
        email,
        firstName: 'Hermes',
        lastName: 'InlineAddressIds',
        addresses: [
          {
            firstName: 'Hermes',
            lastName: 'Removed',
            address1: '100 Removed St',
            address2: 'Suite A',
            city: 'Ottawa',
            company: 'Removed Co',
            countryCode: 'CA',
            provinceCode: 'ON',
            zip: 'K1A 0B1',
            phone: '+14155550121',
          },
          {
            firstName: 'Hermes',
            lastName: 'Target',
            address1: '200 Target Ave',
            address2: 'Floor 2',
            city: 'Toronto',
            company: 'Target Co',
            countryCode: 'CA',
            provinceCode: 'ON',
            zip: 'M5H 2N2',
            phone: '+14155550122',
          },
        ],
      },
    };
    const create = await runGraphql(createMutation, createVariables);
    assertNoTopLevelErrors(create, 'customerCreate inline addresses');
    assertNoUserErrors(create.payload, 'customerCreate', 'customerCreate inline addresses');
    createdCustomerId = create.payload?.data?.customerCreate?.customer?.id;
    if (typeof createdCustomerId !== 'string' || !createdCustomerId) {
      throw new Error(`customerCreate did not return id: ${JSON.stringify(create.payload, null, 2)}`);
    }
    const initialNodes = mutationNodes(create.payload, 'customerCreate', 'customerCreate inline addresses');
    if (initialNodes.length !== 2) {
      throw new Error(`customerCreate expected two addresses, got ${initialNodes.length}`);
    }
    const removedAddressId = initialNodes[0]?.id;
    const targetAddressId = initialNodes[1]?.id;
    if (typeof removedAddressId !== 'string' || typeof targetAddressId !== 'string') {
      throw new Error(`customerCreate did not return address ids: ${JSON.stringify(initialNodes, null, 2)}`);
    }

    const updateInPlaceVariables = {
      input: {
        id: createdCustomerId,
        addresses: [
          {
            id: targetAddressId,
            address1: '999 Bryant St',
          },
        ],
      },
    };
    const updateInPlace = await runGraphql(updateMutation, updateInPlaceVariables);
    assertNoTopLevelErrors(updateInPlace, 'customerUpdate inline address update-in-place');
    assertNoUserErrors(updateInPlace.payload, 'customerUpdate', 'customerUpdate inline address update-in-place');
    const updateNodes = mutationNodes(
      updateInPlace.payload,
      'customerUpdate',
      'customerUpdate inline address update-in-place',
    );
    if (updateNodes.length !== 1 || updateNodes[0]?.id !== targetAddressId || updateNodes[0]?.id === removedAddressId) {
      throw new Error(`customerUpdate did not update/replace addresses as expected: ${JSON.stringify(updateNodes)}`);
    }
    if (updateNodes[0]?.address1 !== '999 Bryant St' || updateNodes[0]?.city !== 'Toronto') {
      throw new Error(`customerUpdate did not preserve existing address fields: ${JSON.stringify(updateNodes[0])}`);
    }

    const readAfterUpdateVariables = {
      id: createdCustomerId,
      identifier: { emailAddress: email },
    };
    const readAfterUpdate = await runGraphql(downstreamReadQuery, readAfterUpdateVariables);
    assertNoTopLevelErrors(readAfterUpdate, 'customerUpdate inline address read after update');
    const readAfterUpdateNodes = customerNodes(
      readAfterUpdate.payload?.data,
      'customerUpdate inline address read after update',
    );
    if (readAfterUpdateNodes.length !== 1 || readAfterUpdateNodes[0]?.id !== targetAddressId) {
      throw new Error(`read after update did not observe replacement: ${JSON.stringify(readAfterUpdateNodes)}`);
    }

    const unknownIdVariables = {
      input: {
        id: createdCustomerId,
        addresses: [
          {
            id: unknownAddressId,
            address1: 'Should Not Stage',
          },
        ],
      },
    };
    const unknownId = await runGraphql(updateMutation, unknownIdVariables);
    assertNoTopLevelErrors(unknownId, 'customerUpdate unknown inline address id');
    const unknownErrors = unknownId.payload?.data?.customerUpdate?.userErrors;
    if (
      !Array.isArray(unknownErrors) ||
      unknownErrors.length !== 1 ||
      unknownErrors[0]?.message !== 'Customer address does not exist'
    ) {
      throw new Error(`customerUpdate unknown id did not return expected message: ${JSON.stringify(unknownErrors)}`);
    }

    const readAfterUnknownId = await runGraphql(downstreamReadQuery, readAfterUpdateVariables);
    assertNoTopLevelErrors(readAfterUnknownId, 'customerUpdate inline address read after unknown id');
    const readAfterUnknownNodes = customerNodes(
      readAfterUnknownId.payload?.data,
      'customerUpdate inline address read after unknown id',
    );
    if (readAfterUnknownNodes.length !== 1 || readAfterUnknownNodes[0]?.id !== targetAddressId) {
      throw new Error(`unknown id update mutated addresses: ${JSON.stringify(readAfterUnknownNodes)}`);
    }

    const createWithoutIdVariables = {
      input: {
        id: createdCustomerId,
        addresses: [
          {
            firstName: 'Hermes',
            lastName: 'Created',
            address1: '300 Created Way',
            address2: 'Unit 3',
            city: 'Montreal',
            company: 'Created Co',
            countryCode: 'CA',
            provinceCode: 'QC',
            zip: 'H2Y 1C6',
            phone: '+14155550125',
          },
        ],
      },
    };
    const createWithoutId = await runGraphql(updateMutation, createWithoutIdVariables);
    assertNoTopLevelErrors(createWithoutId, 'customerUpdate inline address create without id');
    assertNoUserErrors(createWithoutId.payload, 'customerUpdate', 'customerUpdate inline address create without id');
    const createWithoutIdNodes = mutationNodes(
      createWithoutId.payload,
      'customerUpdate',
      'customerUpdate inline address create without id',
    );
    if (
      createWithoutIdNodes.length !== 1 ||
      createWithoutIdNodes[0]?.id === targetAddressId ||
      createWithoutIdNodes[0]?.address1 !== '300 Created Way'
    ) {
      throw new Error(
        `customerUpdate no-id address did not create replacement: ${JSON.stringify(createWithoutIdNodes)}`,
      );
    }

    const readAfterCreateWithoutId = await runGraphql(downstreamReadQuery, readAfterUpdateVariables);
    assertNoTopLevelErrors(readAfterCreateWithoutId, 'customerUpdate inline address read after create without id');
    const readAfterCreateWithoutIdNodes = customerNodes(
      readAfterCreateWithoutId.payload?.data,
      'customerUpdate inline address read after create without id',
    );
    if (
      readAfterCreateWithoutIdNodes.length !== 1 ||
      readAfterCreateWithoutIdNodes[0]?.id !== createWithoutIdNodes[0]?.id
    ) {
      throw new Error(
        `read after no-id create did not observe replacement: ${JSON.stringify(readAfterCreateWithoutIdNodes)}`,
      );
    }

    const result = {
      scenarioId,
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      create: {
        variables: createVariables,
        response: create.payload,
      },
      updateInPlace: {
        variables: updateInPlaceVariables,
        response: updateInPlace.payload,
      },
      readAfterUpdate: {
        variables: readAfterUpdateVariables,
        response: readAfterUpdate.payload,
      },
      unknownId: {
        variables: unknownIdVariables,
        response: unknownId.payload,
      },
      readAfterUnknownId: {
        variables: readAfterUpdateVariables,
        response: readAfterUnknownId.payload,
      },
      createWithoutId: {
        variables: createWithoutIdVariables,
        response: createWithoutId.payload,
      },
      readAfterCreateWithoutId: {
        variables: readAfterUpdateVariables,
        response: readAfterCreateWithoutId.payload,
      },
    };

    const outputPath = path.join(outputDir, `${scenarioId}.json`);
    await writeFile(outputPath, `${JSON.stringify(result, null, 2)}\n`, 'utf8');
    console.log(`Wrote ${outputPath}`);
  } finally {
    if (createdCustomerId) {
      const cleanup = await runGraphql(deleteMutation, { input: { id: createdCustomerId } });
      if (cleanup.status < 200 || cleanup.status >= 300 || cleanup.payload?.errors) {
        console.error(`Customer cleanup failed for ${createdCustomerId}: ${JSON.stringify(cleanup, null, 2)}`);
      }
    }
  }
}

await main();

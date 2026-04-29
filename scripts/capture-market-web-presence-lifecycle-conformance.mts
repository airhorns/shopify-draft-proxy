/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const webPresenceFields = `#graphql
  fragment MarketWebPresenceLifecycleFields on MarketWebPresence {
    id
    subfolderSuffix
    domain {
      id
      host
      url
      sslEnabled
    }
    rootUrls {
      locale
      url
    }
    defaultLocale {
      locale
      name
      primary
      published
    }
    alternateLocales {
      locale
      name
      primary
      published
    }
    markets(first: 5) {
      nodes {
        id
        name
        handle
        status
        type
      }
    }
  }
`;

const webPresencesReadQuery = `#graphql
  ${webPresenceFields}
  query MarketWebPresenceLifecycleRead($first: Int!) {
    webPresences(first: $first) {
      nodes {
        ...MarketWebPresenceLifecycleFields
      }
    }
  }
`;

const createMutation = `#graphql
  ${webPresenceFields}
  mutation MarketWebPresenceLifecycleCreate($input: WebPresenceCreateInput!) {
    webPresenceCreate(input: $input) {
      webPresence {
        ...MarketWebPresenceLifecycleFields
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const updateMutation = `#graphql
  ${webPresenceFields}
  mutation MarketWebPresenceLifecycleUpdate($id: ID!, $input: WebPresenceUpdateInput!) {
    webPresenceUpdate(id: $id, input: $input) {
      webPresence {
        ...MarketWebPresenceLifecycleFields
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const deleteMutation = `#graphql
  mutation MarketWebPresenceLifecycleDelete($id: ID!) {
    webPresenceDelete(id: $id) {
      deletedId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

function randomLetters(length: number): string {
  const alphabet = 'abcdefghijklmnopqrstuvwxyz';
  return Array.from({ length }, () => alphabet[Math.floor(Math.random() * alphabet.length)]).join('');
}

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const createSuffix = `har${randomLetters(10)}`;
const updateSuffix = `har${randomLetters(10)}`;
let createdWebPresenceId: string | null = null;
let cleanupResponse: unknown = null;

try {
  const baselineRead = await runGraphql(webPresencesReadQuery, { first: 20 });
  const createVariables = {
    input: {
      defaultLocale: 'en',
      alternateLocales: [],
      subfolderSuffix: createSuffix,
    },
  };
  const createResponse = await runGraphql(createMutation, createVariables);
  createdWebPresenceId = createResponse.data?.webPresenceCreate?.webPresence?.id ?? null;
  if (!createdWebPresenceId) {
    throw new Error('webPresenceCreate did not return a disposable web presence id.');
  }

  const updateVariables = {
    id: createdWebPresenceId,
    input: {
      defaultLocale: 'en',
      alternateLocales: [],
      subfolderSuffix: updateSuffix,
    },
  };
  const updateResponse = await runGraphql(updateMutation, updateVariables);
  const readAfterUpdate = await runGraphql(webPresencesReadQuery, { first: 20 });
  const deleteResponse = await runGraphql(deleteMutation, { id: createdWebPresenceId });
  cleanupResponse = deleteResponse;
  const readAfterDelete = await runGraphql(webPresencesReadQuery, { first: 20 });

  const fixture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    disposableSubfolderSuffixes: {
      created: createSuffix,
      updated: updateSuffix,
    },
    scope: 'HAR-448 market web presence create/update/delete lifecycle parity',
    data: {
      webPresences: baselineRead.data?.webPresences,
    },
    cases: [
      {
        name: 'webPresenceCreateSuccess',
        query: createMutation,
        variables: createVariables,
        response: {
          status: 200,
          payload: createResponse,
        },
      },
      {
        name: 'webPresenceUpdateSuccess',
        query: updateMutation,
        variables: updateVariables,
        response: {
          status: 200,
          payload: updateResponse,
        },
      },
      {
        name: 'webPresenceReadAfterUpdate',
        query: webPresencesReadQuery,
        variables: { first: 20 },
        response: {
          status: 200,
          payload: readAfterUpdate,
        },
      },
      {
        name: 'webPresenceDeleteSuccess',
        query: deleteMutation,
        variables: { id: createdWebPresenceId },
        response: {
          status: 200,
          payload: deleteResponse,
        },
      },
      {
        name: 'webPresenceReadAfterDelete',
        query: webPresencesReadQuery,
        variables: { first: 20 },
        response: {
          status: 200,
          payload: readAfterDelete,
        },
      },
    ],
    cleanup: {
      webPresenceDelete: {
        query: deleteMutation,
        variables: { id: createdWebPresenceId },
        response: {
          status: 200,
          payload: cleanupResponse,
        },
      },
    },
  };

  const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'markets');
  await mkdir(outputDir, { recursive: true });
  const outputPath = path.join(outputDir, 'market-web-presence-lifecycle-parity.json');
  await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  console.log(`Wrote ${outputPath}`);
} catch (error) {
  if (createdWebPresenceId && !cleanupResponse) {
    cleanupResponse = await runGraphql(deleteMutation, { id: createdWebPresenceId });
    console.error(JSON.stringify({ cleanupAfterFailure: cleanupResponse }, null, 2));
  }
  throw error;
}

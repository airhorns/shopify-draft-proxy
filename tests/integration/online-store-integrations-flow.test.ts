import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { resetSyntheticIdentity } from '../../src/state/synthetic-identity.js';
import { store } from '../../src/state/store.js';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

describe('online-store integration flow', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('stages theme, script, pixel, token, and mobile-app mutations locally with downstream reads', async () => {
    const fetchSpy = vi
      .spyOn(globalThis, 'fetch')
      .mockRejectedValue(new Error('online-store integration mutations must not fetch upstream'));
    const app = createApp(config).callback();

    const emptyRead = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query EmptyIntegrations($id: ID!) {
          theme(id: $id) { id name }
          themes(first: 2) { nodes { id } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } }
          scriptTag(id: $id) { id src }
          scriptTags(first: 2) { nodes { id } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } }
          webPixel { id settings }
          serverPixel { id status webhookEndpointAddress }
          mobilePlatformApplication(id: $id) { __typename ... on AndroidApplication { id applicationId } ... on AppleApplication { id appId } }
          mobilePlatformApplications(first: 2) { nodes { __typename } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } }
        }`,
        variables: { id: 'gid://shopify/OnlineStoreTheme/999' },
      });
    expect(emptyRead.body.data).toMatchObject({
      theme: null,
      themes: {
        nodes: [],
        pageInfo: { hasNextPage: false, hasPreviousPage: false, startCursor: null, endCursor: null },
      },
      scriptTag: null,
      scriptTags: {
        nodes: [],
        pageInfo: { hasNextPage: false, hasPreviousPage: false, startCursor: null, endCursor: null },
      },
      webPixel: null,
      serverPixel: null,
      mobilePlatformApplication: null,
      mobilePlatformApplications: {
        nodes: [],
        pageInfo: { hasNextPage: false, hasPreviousPage: false, startCursor: null, endCursor: null },
      },
    });

    const themeCreate = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation CreateTheme($source: URL!, $name: String) {
          themeCreate(source: $source, name: $name, role: UNPUBLISHED) {
            theme { id name role processing processingFailed files(first: 5) { nodes { filename } userErrors { field message } } }
            userErrors { field message }
          }
        }`,
        variables: { source: 'https://example.com/theme.zip', name: 'Local preview theme' },
      });
    expect(themeCreate.body.data.themeCreate.userErrors).toEqual([]);
    const themeId = themeCreate.body.data.themeCreate.theme.id;

    const filesUpsert = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation UpsertThemeFiles($themeId: ID!, $files: [OnlineStoreThemeFilesUpsertFileInput!]!) {
          themeFilesUpsert(themeId: $themeId, files: $files) {
            job { id done }
            upsertedThemeFiles { filename size checksumMd5 }
            userErrors { field message }
          }
        }`,
        variables: {
          themeId,
          files: [
            {
              filename: 'templates/index.json',
              body: { type: 'TEXT', value: '{"sections":{},"order":[]}' },
            },
          ],
        },
      });
    expect(filesUpsert.body.data.themeFilesUpsert.userErrors).toEqual([]);
    expect(filesUpsert.body.data.themeFilesUpsert.upsertedThemeFiles[0]).toMatchObject({
      filename: 'templates/index.json',
      size: 26,
    });

    const themePublish = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation PublishTheme($id: ID!) {
          themePublish(id: $id) { theme { id role } userErrors { field message } }
        }`,
        variables: { id: themeId },
      });
    expect(themePublish.body.data.themePublish).toMatchObject({
      theme: { id: themeId, role: 'MAIN' },
      userErrors: [],
    });

    const scriptCreate = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation Script($input: ScriptTagInput!) {
          scriptTagCreate(input: $input) {
            scriptTag { id src displayScope cache }
            userErrors { field message }
          }
        }`,
        variables: {
          input: {
            src: 'https://cdn.example.com/local.js',
            displayScope: 'ONLINE_STORE',
            cache: true,
          },
        },
      });
    expect(scriptCreate.body.data.scriptTagCreate.userErrors).toEqual([]);
    const scriptTagId = scriptCreate.body.data.scriptTagCreate.scriptTag.id;

    const webPixelCreate = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation WebPixel($webPixel: WebPixelInput!) {
          webPixelCreate(webPixel: $webPixel) {
            webPixel { id settings }
            userErrors { field message }
          }
        }`,
        variables: { webPixel: { settings: { accountId: 'local-account' } } },
      });
    expect(webPixelCreate.body.data.webPixelCreate.userErrors).toEqual([]);
    const webPixelId = webPixelCreate.body.data.webPixelCreate.webPixel.id;

    const serverPixelCreate = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation {
          serverPixelCreate { serverPixel { id status webhookEndpointAddress } userErrors { field message } }
        }`,
      });
    expect(serverPixelCreate.body.data.serverPixelCreate.userErrors).toEqual([]);
    const serverPixelId = serverPixelCreate.body.data.serverPixelCreate.serverPixel.id;

    const eventBridgeUpdate = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation {
          eventBridgeServerPixelUpdate(arn: "arn:aws:events:us-east-1:123456789012:event-bus/local") {
            serverPixel { id status webhookEndpointAddress }
            userErrors { field message }
          }
        }`,
      });
    expect(eventBridgeUpdate.body.data.eventBridgeServerPixelUpdate).toMatchObject({
      serverPixel: {
        id: serverPixelId,
        webhookEndpointAddress: 'arn:aws:events:us-east-1:123456789012:event-bus/local',
      },
      userErrors: [],
    });

    const pubSubUpdate = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation {
          pubSubServerPixelUpdate(pubSubProject: "local-project", pubSubTopic: "local-topic") {
            serverPixel { id status webhookEndpointAddress }
            userErrors { field message }
          }
        }`,
      });
    expect(pubSubUpdate.body.data.pubSubServerPixelUpdate).toMatchObject({
      serverPixel: { id: serverPixelId, webhookEndpointAddress: 'local-project/local-topic' },
      userErrors: [],
    });

    const tokenCreate = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation Token($input: StorefrontAccessTokenInput!) {
          storefrontAccessTokenCreate(input: $input) {
            storefrontAccessToken { id title accessToken }
            userErrors { field message }
          }
        }`,
        variables: { input: { title: 'Headless preview' } },
      });
    expect(tokenCreate.body.data.storefrontAccessTokenCreate.storefrontAccessToken).toMatchObject({
      title: 'Headless preview',
      accessToken: 'shpat_redacted',
    });
    const tokenId = tokenCreate.body.data.storefrontAccessTokenCreate.storefrontAccessToken.id;
    const stateAfterTokenCreate = await request(app).get('/__meta/state');
    expect(stateAfterTokenCreate.body.stagedState.onlineStoreStorefrontAccessTokens[tokenId].data).toMatchObject({
      title: 'Headless preview',
      accessToken: 'shpat_redacted',
    });
    expect(JSON.stringify(stateAfterTokenCreate.body)).not.toContain('shpat_headless-preview');

    const mobileCreate = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation Mobile($input: MobilePlatformApplicationCreateInput!) {
          mobilePlatformApplicationCreate(input: $input) {
            mobilePlatformApplication {
              __typename
              ... on AndroidApplication { id applicationId appLinksEnabled sha256CertFingerprints }
            }
            userErrors { field message }
          }
        }`,
        variables: {
          input: {
            android: {
              applicationId: 'com.example.local',
              appLinksEnabled: true,
              sha256CertFingerprints: ['AA:BB'],
            },
          },
        },
      });
    expect(mobileCreate.body.data.mobilePlatformApplicationCreate.userErrors).toEqual([]);
    const mobileId = mobileCreate.body.data.mobilePlatformApplicationCreate.mobilePlatformApplication.id;

    const readAfter = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query ReadIntegrations($themeId: ID!, $scriptTagId: ID!, $mobileId: ID!) {
          theme(id: $themeId) {
            id
            name
            role
            files(first: 5) {
              nodes { filename size body { __typename ... on OnlineStoreThemeFileBodyText { content } } }
              userErrors { field message }
            }
          }
          themes(first: 5, roles: [MAIN]) { nodes { id name role } }
          scriptTag(id: $scriptTagId) { id src displayScope cache }
          scriptTags(first: 5, src: "https://cdn.example.com/local.js") { nodes { id src } }
          webPixel { id settings }
          serverPixel { id status webhookEndpointAddress }
          mobilePlatformApplication(id: $mobileId) {
            __typename
            ... on AndroidApplication { id applicationId appLinksEnabled sha256CertFingerprints }
          }
          mobilePlatformApplications(first: 5) {
            nodes {
              __typename
              ... on AndroidApplication { id applicationId appLinksEnabled }
            }
          }
        }`,
        variables: { themeId, scriptTagId, mobileId },
      });
    expect(readAfter.body.data).toMatchObject({
      theme: {
        id: themeId,
        name: 'Local preview theme',
        role: 'MAIN',
        files: {
          nodes: [
            {
              filename: 'templates/index.json',
              size: 26,
              body: { __typename: 'OnlineStoreThemeFileBodyText', content: '{"sections":{},"order":[]}' },
            },
          ],
          userErrors: [],
        },
      },
      themes: { nodes: [{ id: themeId, name: 'Local preview theme', role: 'MAIN' }] },
      scriptTag: {
        id: scriptTagId,
        src: 'https://cdn.example.com/local.js',
        displayScope: 'ONLINE_STORE',
        cache: true,
      },
      scriptTags: { nodes: [{ id: scriptTagId, src: 'https://cdn.example.com/local.js' }] },
      webPixel: { id: webPixelId, settings: { accountId: 'local-account' } },
      serverPixel: { id: serverPixelId, status: 'CONNECTED', webhookEndpointAddress: 'local-project/local-topic' },
      mobilePlatformApplication: {
        __typename: 'AndroidApplication',
        id: mobileId,
        applicationId: 'com.example.local',
        appLinksEnabled: true,
        sha256CertFingerprints: ['AA:BB'],
      },
      mobilePlatformApplications: {
        nodes: [
          {
            __typename: 'AndroidApplication',
            id: mobileId,
            applicationId: 'com.example.local',
            appLinksEnabled: true,
          },
        ],
      },
    });

    const themeUpdate = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation UpdateTheme($id: ID!, $input: OnlineStoreThemeInput!) {
          themeUpdate(id: $id, input: $input) {
            theme { id name role }
            userErrors { field message }
          }
        }`,
        variables: { id: themeId, input: { name: 'Renamed preview theme' } },
      });
    expect(themeUpdate.body.data.themeUpdate).toMatchObject({
      theme: { id: themeId, name: 'Renamed preview theme', role: 'MAIN' },
      userErrors: [],
    });

    const filesCopy = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation CopyThemeFiles($themeId: ID!, $files: [OnlineStoreThemeFilesCopyFileInput!]!) {
          themeFilesCopy(themeId: $themeId, files: $files) {
            copiedThemeFiles { filename size checksumMd5 }
            userErrors { field message }
          }
        }`,
        variables: {
          themeId,
          files: [{ srcFilename: 'templates/index.json', dstFilename: 'templates/copy.json' }],
        },
      });
    expect(filesCopy.body.data.themeFilesCopy).toMatchObject({
      copiedThemeFiles: [{ filename: 'templates/copy.json', size: 26 }],
      userErrors: [],
    });

    const filesDelete = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation DeleteThemeFiles($themeId: ID!, $files: [String!]!) {
          themeFilesDelete(themeId: $themeId, files: $files) {
            deletedThemeFiles { filename size }
            userErrors { field message }
          }
        }`,
        variables: { themeId, files: ['templates/index.json'] },
      });
    expect(filesDelete.body.data.themeFilesDelete).toMatchObject({
      deletedThemeFiles: [{ filename: 'templates/index.json', size: 26 }],
      userErrors: [],
    });

    const scriptUpdate = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation UpdateScript($id: ID!, $input: ScriptTagInput!) {
          scriptTagUpdate(id: $id, input: $input) {
            scriptTag { id src displayScope cache }
            userErrors { field message }
          }
        }`,
        variables: {
          id: scriptTagId,
          input: {
            src: 'https://cdn.example.com/updated.js',
            displayScope: 'ALL',
            cache: false,
          },
        },
      });
    expect(scriptUpdate.body.data.scriptTagUpdate).toMatchObject({
      scriptTag: {
        id: scriptTagId,
        src: 'https://cdn.example.com/updated.js',
        displayScope: 'ALL',
        cache: false,
      },
      userErrors: [],
    });

    const webPixelUpdate = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation UpdateWebPixel($webPixel: WebPixelInput!) {
          webPixelUpdate(webPixel: $webPixel) {
            webPixel { id settings }
            userErrors { field message }
          }
        }`,
        variables: { webPixel: { settings: { accountId: 'updated-account' } } },
      });
    expect(webPixelUpdate.body.data.webPixelUpdate).toMatchObject({
      webPixel: { id: webPixelId, settings: { accountId: 'updated-account' } },
      userErrors: [],
    });

    const mobileUpdate = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation UpdateMobile($id: ID!, $input: MobilePlatformApplicationUpdateInput!) {
          mobilePlatformApplicationUpdate(id: $id, input: $input) {
            mobilePlatformApplication {
              __typename
              ... on AndroidApplication { id applicationId appLinksEnabled sha256CertFingerprints }
            }
            userErrors { field message }
          }
        }`,
        variables: {
          id: mobileId,
          input: {
            android: {
              applicationId: 'com.example.updated',
              appLinksEnabled: false,
              sha256CertFingerprints: ['CC:DD'],
            },
          },
        },
      });
    expect(mobileUpdate.body.data.mobilePlatformApplicationUpdate).toMatchObject({
      mobilePlatformApplication: {
        __typename: 'AndroidApplication',
        id: mobileId,
        applicationId: 'com.example.updated',
        appLinksEnabled: false,
        sha256CertFingerprints: ['CC:DD'],
      },
      userErrors: [],
    });

    const readAfterUpdates = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query ReadUpdatedIntegrations($themeId: ID!, $scriptTagId: ID!, $mobileId: ID!) {
          theme(id: $themeId) {
            id
            name
            role
            files(first: 5) { nodes { filename size } userErrors { field message } }
          }
          scriptTag(id: $scriptTagId) { id src displayScope cache }
          webPixel { id settings }
          mobilePlatformApplication(id: $mobileId) {
            __typename
            ... on AndroidApplication { id applicationId appLinksEnabled sha256CertFingerprints }
          }
        }`,
        variables: { themeId, scriptTagId, mobileId },
      });
    expect(readAfterUpdates.body.data).toMatchObject({
      theme: {
        id: themeId,
        name: 'Renamed preview theme',
        role: 'MAIN',
        files: { nodes: [{ filename: 'templates/copy.json', size: 26 }], userErrors: [] },
      },
      scriptTag: {
        id: scriptTagId,
        src: 'https://cdn.example.com/updated.js',
        displayScope: 'ALL',
        cache: false,
      },
      webPixel: { id: webPixelId, settings: { accountId: 'updated-account' } },
      mobilePlatformApplication: {
        __typename: 'AndroidApplication',
        id: mobileId,
        applicationId: 'com.example.updated',
        appLinksEnabled: false,
        sha256CertFingerprints: ['CC:DD'],
      },
    });

    const tokenDelete = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation DeleteToken($input: StorefrontAccessTokenDeleteInput!) {
          storefrontAccessTokenDelete(input: $input) {
            deletedStorefrontAccessTokenId
            userErrors { field message }
          }
        }`,
        variables: { input: { id: tokenId } },
      });
    expect(tokenDelete.body.data.storefrontAccessTokenDelete).toEqual({
      deletedStorefrontAccessTokenId: tokenId,
      userErrors: [],
    });

    const scriptDelete = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation DeleteScript($id: ID!) {
          scriptTagDelete(id: $id) { deletedScriptTagId userErrors { field message } }
        }`,
        variables: { id: scriptTagId },
      });
    expect(scriptDelete.body.data.scriptTagDelete).toEqual({
      deletedScriptTagId: scriptTagId,
      userErrors: [],
    });

    const webPixelDelete = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation DeleteWebPixel($id: ID!) {
          webPixelDelete(id: $id) { deletedWebPixelId userErrors { field message } }
        }`,
        variables: { id: webPixelId },
      });
    expect(webPixelDelete.body.data.webPixelDelete).toEqual({
      deletedWebPixelId: webPixelId,
      userErrors: [],
    });

    const serverPixelDelete = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation {
          serverPixelDelete { deletedServerPixelId userErrors { field message } }
        }`,
      });
    expect(serverPixelDelete.body.data.serverPixelDelete).toEqual({
      deletedServerPixelId: serverPixelId,
      userErrors: [],
    });

    const mobileDelete = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation DeleteMobile($id: ID!) {
          mobilePlatformApplicationDelete(id: $id) {
            deletedMobilePlatformApplicationId
            userErrors { field message }
          }
        }`,
        variables: { id: mobileId },
      });
    expect(mobileDelete.body.data.mobilePlatformApplicationDelete).toEqual({
      deletedMobilePlatformApplicationId: mobileId,
      userErrors: [],
    });

    const themeDelete = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation DeleteTheme($id: ID!) {
          themeDelete(id: $id) { deletedThemeId userErrors { field message } }
        }`,
        variables: { id: themeId },
      });
    expect(themeDelete.body.data.themeDelete).toEqual({
      deletedThemeId: themeId,
      userErrors: [],
    });

    const readAfterDeletes = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query ReadDeletedIntegrations($themeId: ID!, $scriptTagId: ID!, $mobileId: ID!) {
          theme(id: $themeId) { id }
          themes(first: 5) { nodes { id } }
          scriptTag(id: $scriptTagId) { id }
          scriptTags(first: 5) { nodes { id } }
          webPixel { id }
          serverPixel { id }
          mobilePlatformApplication(id: $mobileId) { __typename ... on AndroidApplication { id } }
          mobilePlatformApplications(first: 5) { nodes { __typename } }
        }`,
        variables: { themeId, scriptTagId, mobileId },
      });
    expect(readAfterDeletes.body.data).toMatchObject({
      theme: null,
      themes: { nodes: [] },
      scriptTag: null,
      scriptTags: { nodes: [] },
      webPixel: null,
      serverPixel: null,
      mobilePlatformApplication: null,
      mobilePlatformApplications: { nodes: [] },
    });

    const unknownUpdate = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation UnknownScript($id: ID!, $input: ScriptTagInput!) {
          scriptTagUpdate(id: $id, input: $input) {
            scriptTag { id }
            userErrors { field message }
          }
        }`,
        variables: { id: 'gid://shopify/ScriptTag/999', input: { src: 'https://cdn.example.com/other.js' } },
      });
    expect(unknownUpdate.body.data.scriptTagUpdate).toEqual({
      scriptTag: null,
      userErrors: [{ field: ['id'], message: 'Script tag does not exist' }],
    });

    const logResponse = await request(app).get('/__meta/log');
    expect(logResponse.body.entries.map((entry: { operationName: string }) => entry.operationName)).toEqual([
      'themeCreate',
      'themeFilesUpsert',
      'themePublish',
      'scriptTagCreate',
      'webPixelCreate',
      'serverPixelCreate',
      'eventBridgeServerPixelUpdate',
      'pubSubServerPixelUpdate',
      'storefrontAccessTokenCreate',
      'mobilePlatformApplicationCreate',
      'themeUpdate',
      'themeFilesCopy',
      'themeFilesDelete',
      'scriptTagUpdate',
      'webPixelUpdate',
      'mobilePlatformApplicationUpdate',
      'storefrontAccessTokenDelete',
      'scriptTagDelete',
      'webPixelDelete',
      'serverPixelDelete',
      'mobilePlatformApplicationDelete',
      'themeDelete',
      'scriptTagUpdate',
    ]);
    expect(logResponse.body.entries[8].requestBody.variables.input.title).toBe('Headless preview');

    const stateResponse = await request(app).get('/__meta/state');
    expect(stateResponse.body.stagedState.onlineStoreStorefrontAccessTokens[tokenId]).toBeUndefined();
    expect(stateResponse.body.stagedState.deletedOnlineStoreStorefrontAccessTokenIds[tokenId]).toBe(true);
    expect(stateResponse.body.stagedState.deletedOnlineStoreThemeIds[themeId]).toBe(true);
    expect(stateResponse.body.stagedState.deletedOnlineStoreScriptTagIds[scriptTagId]).toBe(true);
    expect(stateResponse.body.stagedState.deletedOnlineStoreWebPixelIds[webPixelId]).toBe(true);
    expect(stateResponse.body.stagedState.deletedOnlineStoreServerPixelIds[serverPixelId]).toBe(true);
    expect(stateResponse.body.stagedState.deletedOnlineStoreMobilePlatformApplicationIds[mobileId]).toBe(true);
    expect(JSON.stringify(stateResponse.body)).not.toContain('shpat_headless-preview');
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});

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

const definitionSelection = `
  id
  type
  name
  displayNameKey
  capabilities { publishable { enabled } translatable { enabled } renderable { enabled } onlineStore { enabled } }
  fieldDefinitions { key name required type { name category } validations { name value } }
  metaobjectsCount
`;

const rowSelection = `
  id
  handle
  type
  displayName
  capabilities { publishable { status } onlineStore { templateSuffix } }
  definition { displayNameKey fieldDefinitions { key name required type { name category } validations { name value } } }
  fields { key type value jsonValue definition { key name required type { name category } } }
  titleField: field(key: "title") { key type value definition { key name required type { name category } } }
  summaryField: field(key: "summary") { key type value definition { key name required type { name category } } }
  legacyField: field(key: "legacy") { key value }
`;

describe('metaobject schema change flow', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('stages definition and row lifecycle before and after schema changes without upstream writes', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('metaobject schema-change lifecycle must stay local');
    });
    const app = createApp(config).callback();

    const createDefinitionResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        operationName: 'CreateSchemaChangeDefinition',
        query: `mutation CreateSchemaChangeDefinition($definition: MetaobjectDefinitionCreateInput!) {
          metaobjectDefinitionCreate(definition: $definition) {
            metaobjectDefinition { ${definitionSelection} }
            userErrors { field message code elementKey elementIndex }
          }
        }`,
        variables: {
          definition: {
            type: 'codex_schema_change',
            name: 'Codex Schema Change',
            description: 'Definition used for row lifecycle schema-change coverage.',
            displayNameKey: 'title',
            capabilities: {
              publishable: { enabled: true },
              translatable: { enabled: false },
              renderable: { enabled: false },
              onlineStore: { enabled: false },
            },
            fieldDefinitions: [
              { key: 'title', name: 'Title', type: 'single_line_text_field', required: true },
              { key: 'body', name: 'Body', type: 'multi_line_text_field', required: false },
              { key: 'legacy', name: 'Legacy', type: 'single_line_text_field', required: false },
            ],
          },
        },
      });

    expect(createDefinitionResponse.status).toBe(200);
    expect(createDefinitionResponse.body.data.metaobjectDefinitionCreate.userErrors).toEqual([]);
    const definitionId = createDefinitionResponse.body.data.metaobjectDefinitionCreate.metaobjectDefinition.id;
    expect(createDefinitionResponse.body.data.metaobjectDefinitionCreate.metaobjectDefinition).toMatchObject({
      id: 'gid://shopify/MetaobjectDefinition/1?shopify-draft-proxy=synthetic',
      type: 'codex_schema_change',
      displayNameKey: 'title',
      fieldDefinitions: [{ key: 'title' }, { key: 'body' }, { key: 'legacy' }],
      metaobjectsCount: 0,
    });

    const createPreChangeResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        operationName: 'CreatePreChangeMetaobject',
        query: `mutation CreatePreChangeMetaobject($metaobject: MetaobjectCreateInput!) {
          metaobjectCreate(metaobject: $metaobject) {
            metaobject { ${rowSelection} }
            userErrors { field message code elementKey elementIndex }
          }
        }`,
        variables: {
          metaobject: {
            type: 'codex_schema_change',
            handle: 'pre-change-row',
            capabilities: { publishable: { status: 'ACTIVE' } },
            fields: [
              { key: 'title', value: 'Pre title' },
              { key: 'body', value: 'Pre body' },
              { key: 'legacy', value: 'Pre legacy' },
            ],
          },
        },
      });

    expect(createPreChangeResponse.body.data.metaobjectCreate.userErrors).toEqual([]);
    const preChangeRow = createPreChangeResponse.body.data.metaobjectCreate.metaobject;
    expect(preChangeRow).toMatchObject({
      handle: 'pre-change-row',
      displayName: 'Pre title',
      fields: [
        { key: 'title', value: 'Pre title' },
        { key: 'body', value: 'Pre body' },
        { key: 'legacy', value: 'Pre legacy' },
      ],
    });

    const updatePreChangeResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        operationName: 'UpdatePreChangeMetaobject',
        query: `mutation UpdatePreChangeMetaobject($id: ID!, $metaobject: MetaobjectUpdateInput!) {
          metaobjectUpdate(id: $id, metaobject: $metaobject) {
            metaobject { ${rowSelection} }
            userErrors { field message code elementKey elementIndex }
          }
        }`,
        variables: {
          id: preChangeRow.id,
          metaobject: {
            handle: 'pre-change-updated',
            fields: [
              { key: 'title', value: 'Pre title updated' },
              { key: 'body', value: 'Pre body updated' },
            ],
          },
        },
      });

    expect(updatePreChangeResponse.body.data.metaobjectUpdate).toMatchObject({
      userErrors: [],
      metaobject: {
        id: preChangeRow.id,
        handle: 'pre-change-updated',
        displayName: 'Pre title updated',
      },
    });

    const upsertBeforeDeleteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        operationName: 'UpsertBeforeDeleteMetaobject',
        query: `mutation UpsertBeforeDeleteMetaobject($handle: MetaobjectHandleInput!, $metaobject: MetaobjectUpsertInput!) {
          metaobjectUpsert(handle: $handle, metaobject: $metaobject) {
            metaobject { ${rowSelection} }
            userErrors { field message code elementKey elementIndex }
          }
        }`,
        variables: {
          handle: { type: 'codex_schema_change', handle: 'pre-change-delete' },
          metaobject: {
            fields: [
              { key: 'title', value: 'Delete before title' },
              { key: 'body', value: 'Delete before body' },
            ],
          },
        },
      });

    expect(upsertBeforeDeleteResponse.body.data.metaobjectUpsert.userErrors).toEqual([]);
    const beforeDeleteRow = upsertBeforeDeleteResponse.body.data.metaobjectUpsert.metaobject;

    const deleteBeforeChangeResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        operationName: 'DeleteBeforeChangeMetaobject',
        query: `mutation DeleteBeforeChangeMetaobject($id: ID!) {
          metaobjectDelete(id: $id) { deletedId userErrors { field message code elementKey elementIndex } }
        }`,
        variables: { id: beforeDeleteRow.id },
      });

    expect(deleteBeforeChangeResponse.body.data.metaobjectDelete).toEqual({
      deletedId: beforeDeleteRow.id,
      userErrors: [],
    });

    const beforeSchemaRead = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query BeforeSchemaRead($id: ID!, $handle: MetaobjectHandleInput!, $deletedId: ID!, $type: String!) {
          detail: metaobject(id: $id) { ${rowSelection} }
          byHandle: metaobjectByHandle(handle: $handle) { id handle displayName }
          deleted: metaobject(id: $deletedId) { id }
          catalog: metaobjects(type: $type, first: 10) { nodes { id handle displayName fields { key value } } }
          definition: metaobjectDefinitionByType(type: $type) { id displayNameKey metaobjectsCount fieldDefinitions { key } }
        }`,
        variables: {
          id: preChangeRow.id,
          handle: { type: 'codex_schema_change', handle: 'pre-change-updated' },
          deletedId: beforeDeleteRow.id,
          type: 'codex_schema_change',
        },
      });

    expect(beforeSchemaRead.body.data).toMatchObject({
      detail: {
        id: preChangeRow.id,
        displayName: 'Pre title updated',
        fields: [
          { key: 'title', value: 'Pre title updated' },
          { key: 'body', value: 'Pre body updated' },
          { key: 'legacy', value: 'Pre legacy' },
        ],
      },
      byHandle: { id: preChangeRow.id, handle: 'pre-change-updated', displayName: 'Pre title updated' },
      deleted: null,
      catalog: {
        nodes: [
          {
            id: preChangeRow.id,
            handle: 'pre-change-updated',
            displayName: 'Pre title updated',
          },
        ],
      },
      definition: {
        id: definitionId,
        displayNameKey: 'title',
        metaobjectsCount: 1,
        fieldDefinitions: [{ key: 'title' }, { key: 'body' }, { key: 'legacy' }],
      },
    });

    const updateDefinitionResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        operationName: 'UpdateSchemaDefinition',
        query: `mutation UpdateSchemaDefinition($id: ID!, $definition: MetaobjectDefinitionUpdateInput!) {
          metaobjectDefinitionUpdate(id: $id, definition: $definition, resetFieldOrder: true) {
            metaobjectDefinition { ${definitionSelection} }
            userErrors { field message code elementKey elementIndex }
          }
        }`,
        variables: {
          id: definitionId,
          definition: {
            name: 'Codex Schema Change Updated',
            displayNameKey: 'summary',
            capabilities: {
              publishable: { enabled: false },
              translatable: { enabled: true },
              renderable: { enabled: true },
            },
            fieldDefinitions: [
              {
                create: {
                  key: 'summary',
                  name: 'Summary',
                  type: 'single_line_text_field',
                  required: true,
                  validations: [{ name: 'max', value: '80' }],
                },
              },
              {
                update: {
                  key: 'title',
                  name: 'Short title',
                  required: false,
                },
              },
              {
                update: {
                  key: 'body',
                  name: 'Body summary',
                  type: 'single_line_text_field',
                  required: false,
                  validations: [{ name: 'max', value: '120' }],
                },
              },
              { delete: { key: 'legacy' } },
            ],
          },
        },
      });

    expect(updateDefinitionResponse.body.data.metaobjectDefinitionUpdate).toMatchObject({
      userErrors: [],
      metaobjectDefinition: {
        id: definitionId,
        name: 'Codex Schema Change Updated',
        displayNameKey: 'summary',
        capabilities: {
          publishable: { enabled: false },
          translatable: { enabled: true },
          renderable: { enabled: true },
        },
        fieldDefinitions: [
          {
            key: 'summary',
            name: 'Summary',
            required: true,
            validations: [{ name: 'max', value: '80' }],
          },
          { key: 'title', name: 'Short title', required: false },
          {
            key: 'body',
            name: 'Body summary',
            type: { name: 'single_line_text_field', category: 'TEXT' },
            validations: [{ name: 'max', value: '120' }],
          },
        ],
        metaobjectsCount: 1,
      },
    });

    const afterDefinitionRead = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query AfterDefinitionRead($id: ID!, $handle: MetaobjectHandleInput!, $type: String!) {
          detail: metaobject(id: $id) { ${rowSelection} }
          byHandle: metaobjectByHandle(handle: $handle) { id handle displayName fields { key type value definition { key name type { name category } } } }
          catalog: metaobjects(type: $type, first: 10) { nodes { id handle displayName fields { key type value definition { key name type { name category } } } } }
          definition: metaobjectDefinitionByType(type: $type) { displayNameKey metaobjectsCount fieldDefinitions { key name type { name category } validations { name value } } capabilities { publishable { enabled } translatable { enabled } renderable { enabled } } }
        }`,
        variables: {
          id: preChangeRow.id,
          handle: { type: 'codex_schema_change', handle: 'pre-change-updated' },
          type: 'codex_schema_change',
        },
      });

    expect(afterDefinitionRead.body.data.detail).toMatchObject({
      id: preChangeRow.id,
      displayName: null,
      fields: [
        { key: 'title', type: 'single_line_text_field', value: 'Pre title updated' },
        {
          key: 'body',
          type: 'single_line_text_field',
          value: 'Pre body updated',
          definition: { key: 'body', name: 'Body summary', type: { name: 'single_line_text_field', category: 'TEXT' } },
        },
      ],
      titleField: {
        key: 'title',
        value: 'Pre title updated',
        definition: { key: 'title', name: 'Short title' },
      },
      summaryField: null,
      legacyField: null,
      definition: {
        displayNameKey: 'summary',
        fieldDefinitions: [{ key: 'summary' }, { key: 'title' }, { key: 'body' }],
      },
    });
    expect(afterDefinitionRead.body.data.byHandle).toMatchObject({
      id: preChangeRow.id,
      handle: 'pre-change-updated',
      displayName: null,
      fields: [
        { key: 'title', type: 'single_line_text_field', value: 'Pre title updated' },
        { key: 'body', type: 'single_line_text_field', value: 'Pre body updated' },
      ],
    });
    expect(afterDefinitionRead.body.data.catalog.nodes).toHaveLength(1);
    expect(afterDefinitionRead.body.data.definition).toMatchObject({
      displayNameKey: 'summary',
      metaobjectsCount: 1,
      fieldDefinitions: [{ key: 'summary' }, { key: 'title' }, { key: 'body' }],
      capabilities: {
        publishable: { enabled: false },
        translatable: { enabled: true },
        renderable: { enabled: true },
      },
    });

    const invalidPostCreateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        operationName: 'CreateMissingSummaryMetaobject',
        query: `mutation CreateMissingSummaryMetaobject($metaobject: MetaobjectCreateInput!) {
          metaobjectCreate(metaobject: $metaobject) {
            metaobject { id }
            userErrors { field message code elementKey elementIndex }
          }
        }`,
        variables: {
          metaobject: {
            type: 'codex_schema_change',
            handle: 'missing-summary',
            fields: [{ key: 'title', value: 'Missing summary title' }],
          },
        },
      });

    expect(invalidPostCreateResponse.body.data.metaobjectCreate).toEqual({
      metaobject: null,
      userErrors: [
        {
          field: ['metaobject', 'fields', 'summary'],
          message: "Summary can't be blank",
          code: 'BLANK',
          elementKey: 'summary',
          elementIndex: null,
        },
      ],
    });

    const invalidRemovedFieldUpdateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        operationName: 'UpdateRemovedFieldMetaobject',
        query: `mutation UpdateRemovedFieldMetaobject($id: ID!, $metaobject: MetaobjectUpdateInput!) {
          metaobjectUpdate(id: $id, metaobject: $metaobject) {
            metaobject { id }
            userErrors { field message code elementKey elementIndex }
          }
        }`,
        variables: {
          id: preChangeRow.id,
          metaobject: { fields: [{ key: 'legacy', value: 'Cannot update legacy' }] },
        },
      });

    expect(invalidRemovedFieldUpdateResponse.body.data.metaobjectUpdate).toEqual({
      metaobject: null,
      userErrors: [
        {
          field: ['metaobject', 'fields', '0', 'key'],
          message: 'Field definition not found.',
          code: 'NOT_FOUND',
          elementKey: 'legacy',
          elementIndex: 0,
        },
      ],
    });

    const updatePreAfterSchemaResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        operationName: 'UpdatePreAfterSchemaMetaobject',
        query: `mutation UpdatePreAfterSchemaMetaobject($id: ID!, $metaobject: MetaobjectUpdateInput!) {
          metaobjectUpdate(id: $id, metaobject: $metaobject) {
            metaobject { ${rowSelection} }
            userErrors { field message code elementKey elementIndex }
          }
        }`,
        variables: {
          id: preChangeRow.id,
          metaobject: {
            fields: [
              { key: 'summary', value: 'Pre summary after schema' },
              { key: 'body', value: 'Pre body after schema' },
            ],
          },
        },
      });

    expect(updatePreAfterSchemaResponse.body.data.metaobjectUpdate).toMatchObject({
      userErrors: [],
      metaobject: {
        id: preChangeRow.id,
        displayName: 'Pre summary after schema',
        fields: [
          { key: 'summary', type: 'single_line_text_field', value: 'Pre summary after schema' },
          { key: 'title', type: 'single_line_text_field', value: 'Pre title updated' },
          { key: 'body', type: 'single_line_text_field', value: 'Pre body after schema' },
        ],
      },
    });

    const createPostVisibleResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        operationName: 'CreatePostSchemaVisibleMetaobject',
        query: `mutation CreatePostSchemaVisibleMetaobject($metaobject: MetaobjectCreateInput!) {
          metaobjectCreate(metaobject: $metaobject) {
            metaobject { ${rowSelection} }
            userErrors { field message code elementKey elementIndex }
          }
        }`,
        variables: {
          metaobject: {
            type: 'codex_schema_change',
            handle: 'post-visible',
            fields: [
              { key: 'summary', value: 'Post summary' },
              { key: 'title', value: 'Post title' },
              { key: 'body', value: 'Post body' },
            ],
          },
        },
      });

    expect(createPostVisibleResponse.body.data.metaobjectCreate.userErrors).toEqual([]);
    const postVisibleRow = createPostVisibleResponse.body.data.metaobjectCreate.metaobject;
    expect(postVisibleRow).toMatchObject({
      handle: 'post-visible',
      displayName: 'Post summary',
      fields: [
        { key: 'summary', value: 'Post summary' },
        { key: 'title', value: 'Post title' },
        { key: 'body', value: 'Post body' },
      ],
    });

    const updatePostVisibleResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        operationName: 'UpdatePostSchemaVisibleMetaobject',
        query: `mutation UpdatePostSchemaVisibleMetaobject($id: ID!, $metaobject: MetaobjectUpdateInput!) {
          metaobjectUpdate(id: $id, metaobject: $metaobject) {
            metaobject { ${rowSelection} }
            userErrors { field message code elementKey elementIndex }
          }
        }`,
        variables: {
          id: postVisibleRow.id,
          metaobject: {
            handle: 'post-visible-updated',
            fields: [{ key: 'summary', value: 'Post summary updated' }],
          },
        },
      });

    expect(updatePostVisibleResponse.body.data.metaobjectUpdate).toMatchObject({
      userErrors: [],
      metaobject: {
        id: postVisibleRow.id,
        handle: 'post-visible-updated',
        displayName: 'Post summary updated',
      },
    });

    const createPostDeleteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        operationName: 'CreatePostSchemaDeleteMetaobject',
        query: `mutation CreatePostSchemaDeleteMetaobject($metaobject: MetaobjectCreateInput!) {
          metaobjectCreate(metaobject: $metaobject) {
            metaobject { id handle displayName fields { key value } }
            userErrors { field message code elementKey elementIndex }
          }
        }`,
        variables: {
          metaobject: {
            type: 'codex_schema_change',
            handle: 'post-delete',
            fields: [
              { key: 'summary', value: 'Post delete summary' },
              { key: 'title', value: 'Post delete title' },
            ],
          },
        },
      });

    expect(createPostDeleteResponse.body.data.metaobjectCreate.userErrors).toEqual([]);
    const postDeleteRow = createPostDeleteResponse.body.data.metaobjectCreate.metaobject;

    const deletePostSchemaResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        operationName: 'DeletePostSchemaMetaobject',
        query: `mutation DeletePostSchemaMetaobject($id: ID!) {
          metaobjectDelete(id: $id) { deletedId userErrors { field message code elementKey elementIndex } }
        }`,
        variables: { id: postDeleteRow.id },
      });

    expect(deletePostSchemaResponse.body.data.metaobjectDelete).toEqual({
      deletedId: postDeleteRow.id,
      userErrors: [],
    });

    const finalReadResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query FinalSchemaRead($preId: ID!, $postId: ID!, $deletedId: ID!, $preHandle: MetaobjectHandleInput!, $postHandle: MetaobjectHandleInput!, $deletedHandle: MetaobjectHandleInput!, $type: String!) {
          pre: metaobject(id: $preId) { id handle displayName fields { key type value definition { key name type { name category } } } }
          post: metaobject(id: $postId) { id handle displayName fields { key value } }
          deleted: metaobject(id: $deletedId) { id }
          preHandle: metaobjectByHandle(handle: $preHandle) { id displayName }
          postHandle: metaobjectByHandle(handle: $postHandle) { id handle displayName }
          deletedHandle: metaobjectByHandle(handle: $deletedHandle) { id }
          catalog: metaobjects(type: $type, first: 10, sortKey: "display_name") { nodes { id handle displayName fields { key value } } }
          definition: metaobjectDefinitionByType(type: $type) { metaobjectsCount fieldDefinitions { key } }
        }`,
        variables: {
          preId: preChangeRow.id,
          postId: postVisibleRow.id,
          deletedId: postDeleteRow.id,
          preHandle: { type: 'codex_schema_change', handle: 'pre-change-updated' },
          postHandle: { type: 'codex_schema_change', handle: 'post-visible-updated' },
          deletedHandle: { type: 'codex_schema_change', handle: 'post-delete' },
          type: 'codex_schema_change',
        },
      });

    expect(finalReadResponse.body.data).toMatchObject({
      pre: {
        id: preChangeRow.id,
        handle: 'pre-change-updated',
        displayName: 'Pre summary after schema',
        fields: [
          { key: 'summary', value: 'Pre summary after schema' },
          { key: 'title', value: 'Pre title updated' },
          { key: 'body', value: 'Pre body after schema' },
        ],
      },
      post: {
        id: postVisibleRow.id,
        handle: 'post-visible-updated',
        displayName: 'Post summary updated',
      },
      deleted: null,
      preHandle: { id: preChangeRow.id, displayName: 'Pre summary after schema' },
      postHandle: {
        id: postVisibleRow.id,
        handle: 'post-visible-updated',
        displayName: 'Post summary updated',
      },
      deletedHandle: null,
      definition: {
        metaobjectsCount: 2,
        fieldDefinitions: [{ key: 'summary' }, { key: 'title' }, { key: 'body' }],
      },
    });
    expect(finalReadResponse.body.data.catalog.nodes).toHaveLength(2);
    expect(finalReadResponse.body.data.catalog.nodes.map((node: { handle: string }) => node.handle).sort()).toEqual([
      'post-visible-updated',
      'pre-change-updated',
    ]);

    const stateResponse = await request(app).get('/__meta/state');
    expect(stateResponse.body.stagedState.metaobjectDefinitions[definitionId]).toMatchObject({
      id: definitionId,
      displayNameKey: 'summary',
      metaobjectsCount: 2,
      fieldDefinitions: [{ key: 'summary' }, { key: 'title' }, { key: 'body' }],
    });
    expect(stateResponse.body.stagedState.metaobjects[preChangeRow.id]).toMatchObject({
      id: preChangeRow.id,
      handle: 'pre-change-updated',
      displayName: 'Pre summary after schema',
    });
    expect(stateResponse.body.stagedState.metaobjects[postVisibleRow.id]).toMatchObject({
      id: postVisibleRow.id,
      handle: 'post-visible-updated',
      displayName: 'Post summary updated',
    });
    expect(stateResponse.body.stagedState.deletedMetaobjectIds).toMatchObject({
      [beforeDeleteRow.id]: true,
      [postDeleteRow.id]: true,
    });

    const logResponse = await request(app).get('/__meta/log');
    expect(logResponse.body.entries.map((entry: { operationName: string }) => entry.operationName)).toEqual([
      'metaobjectDefinitionCreate',
      'metaobjectCreate',
      'metaobjectUpdate',
      'metaobjectUpsert',
      'metaobjectDelete',
      'metaobjectDefinitionUpdate',
      'metaobjectCreate',
      'metaobjectUpdate',
      'metaobjectUpdate',
      'metaobjectCreate',
      'metaobjectUpdate',
      'metaobjectCreate',
      'metaobjectDelete',
    ]);
    expect(logResponse.body.entries.map((entry: { status: string }) => entry.status)).toEqual(
      Array.from({ length: 13 }, () => 'staged'),
    );
    expect(
      logResponse.body.entries.map((entry: { requestBody: { operationName?: string } }) => entry.requestBody),
    ).toEqual([
      expect.objectContaining({ operationName: 'CreateSchemaChangeDefinition' }),
      expect.objectContaining({ operationName: 'CreatePreChangeMetaobject' }),
      expect.objectContaining({ operationName: 'UpdatePreChangeMetaobject' }),
      expect.objectContaining({ operationName: 'UpsertBeforeDeleteMetaobject' }),
      expect.objectContaining({ operationName: 'DeleteBeforeChangeMetaobject' }),
      expect.objectContaining({ operationName: 'UpdateSchemaDefinition' }),
      expect.objectContaining({ operationName: 'CreateMissingSummaryMetaobject' }),
      expect.objectContaining({ operationName: 'UpdateRemovedFieldMetaobject' }),
      expect.objectContaining({ operationName: 'UpdatePreAfterSchemaMetaobject' }),
      expect.objectContaining({ operationName: 'CreatePostSchemaVisibleMetaobject' }),
      expect.objectContaining({ operationName: 'UpdatePostSchemaVisibleMetaobject' }),
      expect.objectContaining({ operationName: 'CreatePostSchemaDeleteMetaobject' }),
      expect.objectContaining({ operationName: 'DeletePostSchemaMetaobject' }),
    ]);
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});

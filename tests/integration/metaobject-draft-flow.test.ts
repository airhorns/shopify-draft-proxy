import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { resetSyntheticIdentity } from '../../src/state/synthetic-identity.js';
import { store } from '../../src/state/store.js';
import type { MetaobjectDefinitionRecord, MetaobjectRecord } from '../../src/state/types.js';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

function makeDefinition(overrides: Partial<MetaobjectDefinitionRecord> = {}): MetaobjectDefinitionRecord {
  return {
    id: overrides.id ?? 'gid://shopify/MetaobjectDefinition/244',
    type: overrides.type ?? 'codex_metaobject_rows',
    name: overrides.name ?? 'Codex Metaobject Rows',
    description: overrides.description ?? 'Definition fixture for row lifecycle staging.',
    displayNameKey: overrides.displayNameKey ?? 'title',
    access: overrides.access ?? {
      admin: 'PUBLIC_READ_WRITE',
      storefront: 'NONE',
    },
    capabilities: overrides.capabilities ?? {
      publishable: { enabled: true },
      translatable: { enabled: false },
      renderable: { enabled: false },
      onlineStore: { enabled: false },
    },
    fieldDefinitions: overrides.fieldDefinitions ?? [
      {
        key: 'title',
        name: 'Title',
        description: 'Display title.',
        required: true,
        type: { name: 'single_line_text_field', category: 'TEXT' },
        validations: [],
      },
      {
        key: 'body',
        name: 'Body',
        description: 'Body copy.',
        required: false,
        type: { name: 'multi_line_text_field', category: 'TEXT' },
        validations: [],
      },
    ],
    hasThumbnailField: overrides.hasThumbnailField ?? false,
    metaobjectsCount: overrides.metaobjectsCount ?? 0,
    standardTemplate: overrides.standardTemplate ?? null,
    createdAt: overrides.createdAt ?? null,
    updatedAt: overrides.updatedAt ?? null,
  };
}

function makeEntry(overrides: Partial<MetaobjectRecord> = {}): MetaobjectRecord {
  return {
    id: overrides.id ?? 'gid://shopify/Metaobject/2440',
    handle: overrides.handle ?? 'base-entry',
    type: overrides.type ?? 'codex_metaobject_rows',
    displayName: overrides.displayName ?? 'Base title',
    fields: overrides.fields ?? [
      {
        key: 'title',
        type: 'single_line_text_field',
        value: 'Base title',
        jsonValue: 'Base title',
        definition: {
          key: 'title',
          name: 'Title',
          required: true,
          type: { name: 'single_line_text_field', category: 'TEXT' },
        },
      },
      {
        key: 'body',
        type: 'multi_line_text_field',
        value: 'Base body',
        jsonValue: 'Base body',
        definition: {
          key: 'body',
          name: 'Body',
          required: false,
          type: { name: 'multi_line_text_field', category: 'TEXT' },
        },
      },
    ],
    capabilities: overrides.capabilities ?? {
      publishable: { status: 'ACTIVE' },
      onlineStore: null,
    },
    createdAt: overrides.createdAt ?? '2026-04-25T22:40:00Z',
    updatedAt: overrides.updatedAt ?? '2026-04-25T22:40:46Z',
  };
}

const entrySelection = `
  id
  handle
  type
  displayName
  updatedAt
  capabilities { publishable { status } onlineStore { templateSuffix } }
  fields { key type value jsonValue definition { key name required type { name category } } }
  titleField: field(key: "title") { key value jsonValue definition { key name required type { name category } } }
`;

describe('metaobject draft flow', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('stages create, update, upsert, delete, downstream reads, and meta API visibility without upstream writes', async () => {
    store.upsertBaseMetaobjectDefinitions([makeDefinition()]);
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('metaobject row mutations must stay local');
    });
    const app = createApp(config).callback();

    const createResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        operationName: 'CreateMetaobject',
        query: `mutation CreateMetaobject($metaobject: MetaobjectCreateInput!) {
          metaobjectCreate(metaobject: $metaobject) {
            metaobject { ${entrySelection} }
            userErrors { field message code elementKey elementIndex }
          }
        }`,
        variables: {
          metaobject: {
            type: 'codex_metaobject_rows',
            handle: 'created-entry',
            capabilities: { publishable: { status: 'ACTIVE' } },
            fields: [
              { key: 'title', value: 'Created title' },
              { key: 'body', value: 'Created body' },
            ],
          },
        },
      });

    expect(createResponse.status).toBe(200);
    expect(createResponse.body.data.metaobjectCreate.userErrors).toEqual([]);
    const created = createResponse.body.data.metaobjectCreate.metaobject;
    expect(created).toMatchObject({
      id: 'gid://shopify/Metaobject/1?shopify-draft-proxy=synthetic',
      handle: 'created-entry',
      type: 'codex_metaobject_rows',
      displayName: 'Created title',
      capabilities: {
        publishable: { status: 'ACTIVE' },
        onlineStore: null,
      },
      titleField: {
        key: 'title',
        value: 'Created title',
        jsonValue: 'Created title',
      },
    });

    const readCreatedResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query ReadCreated($id: ID!, $handle: MetaobjectHandleInput!, $type: String!) {
          detail: metaobject(id: $id) { id handle displayName }
          byHandle: metaobjectByHandle(handle: $handle) { id handle displayName }
          catalog: metaobjects(type: $type, first: 10) { nodes { id handle displayName } }
          definition: metaobjectDefinitionByType(type: $type) { metaobjectsCount }
        }`,
        variables: {
          id: created.id,
          handle: { type: 'codex_metaobject_rows', handle: 'created-entry' },
          type: 'codex_metaobject_rows',
        },
      });

    expect(readCreatedResponse.body.data).toMatchObject({
      detail: { id: created.id, handle: 'created-entry', displayName: 'Created title' },
      byHandle: { id: created.id, handle: 'created-entry', displayName: 'Created title' },
      catalog: { nodes: [{ id: created.id, handle: 'created-entry', displayName: 'Created title' }] },
      definition: { metaobjectsCount: 1 },
    });

    const updateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        operationName: 'UpdateMetaobject',
        query: `mutation UpdateMetaobject($id: ID!, $metaobject: MetaobjectUpdateInput!) {
          metaobjectUpdate(id: $id, metaobject: $metaobject) {
            metaobject { ${entrySelection} }
            userErrors { field message code elementKey elementIndex }
          }
        }`,
        variables: {
          id: created.id,
          metaobject: {
            handle: 'renamed-entry',
            capabilities: { publishable: { status: 'DRAFT' } },
            fields: [
              { key: 'title', value: 'Updated title' },
              { key: 'body', value: 'Updated body' },
            ],
          },
        },
      });

    expect(updateResponse.body.data.metaobjectUpdate).toMatchObject({
      userErrors: [],
      metaobject: {
        id: created.id,
        handle: 'renamed-entry',
        displayName: 'Updated title',
        capabilities: { publishable: { status: 'DRAFT' } },
      },
    });

    const upsertUpdateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        operationName: 'UpsertExistingMetaobject',
        query: `mutation UpsertExistingMetaobject($handle: MetaobjectHandleInput!, $metaobject: MetaobjectUpsertInput!) {
          metaobjectUpsert(handle: $handle, metaobject: $metaobject) {
            metaobject { ${entrySelection} }
            userErrors { field message code elementKey elementIndex }
          }
        }`,
        variables: {
          handle: { type: 'codex_metaobject_rows', handle: 'renamed-entry' },
          metaobject: {
            capabilities: { publishable: { status: 'ACTIVE' } },
            fields: [{ key: 'body', value: 'Upserted body' }],
          },
        },
      });

    expect(upsertUpdateResponse.body.data.metaobjectUpsert).toMatchObject({
      userErrors: [],
      metaobject: {
        id: created.id,
        handle: 'renamed-entry',
        displayName: 'Updated title',
        capabilities: { publishable: { status: 'ACTIVE' } },
      },
    });
    expect(upsertUpdateResponse.body.data.metaobjectUpsert.metaobject.fields).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ key: 'title', value: 'Updated title' }),
        expect.objectContaining({ key: 'body', value: 'Upserted body' }),
      ]),
    );

    const upsertCreateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        operationName: 'UpsertNewMetaobject',
        query: `mutation UpsertNewMetaobject($handle: MetaobjectHandleInput!, $metaobject: MetaobjectUpsertInput!) {
          metaobjectUpsert(handle: $handle, metaobject: $metaobject) {
            metaobject { id handle displayName capabilities { publishable { status } onlineStore { templateSuffix } } }
            userErrors { field message code elementKey elementIndex }
          }
        }`,
        variables: {
          handle: { type: 'codex_metaobject_rows', handle: 'upsert-created' },
          metaobject: {
            fields: [
              { key: 'title', value: 'Upsert created title' },
              { key: 'body', value: 'Upsert created body' },
            ],
          },
        },
      });

    expect(upsertCreateResponse.body.data.metaobjectUpsert).toMatchObject({
      userErrors: [],
      metaobject: {
        handle: 'upsert-created',
        displayName: 'Upsert created title',
        capabilities: {
          publishable: { status: 'ACTIVE' },
          onlineStore: null,
        },
      },
    });
    const upsertCreated = upsertCreateResponse.body.data.metaobjectUpsert.metaobject;

    const deleteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        operationName: 'DeleteMetaobject',
        query: `mutation DeleteMetaobject($id: ID!) {
          metaobjectDelete(id: $id) { deletedId userErrors { field message code elementKey elementIndex } }
        }`,
        variables: { id: upsertCreated.id },
      });

    expect(deleteResponse.body.data.metaobjectDelete).toEqual({
      deletedId: upsertCreated.id,
      userErrors: [],
    });

    const missingDeleteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        operationName: 'DeleteMissingMetaobject',
        query: `mutation DeleteMissingMetaobject($id: ID!) {
          metaobjectDelete(id: $id) { deletedId userErrors { field message code elementKey elementIndex } }
        }`,
        variables: { id: 'gid://shopify/Metaobject/404' },
      });

    expect(missingDeleteResponse.body.data.metaobjectDelete).toEqual({
      deletedId: null,
      userErrors: [
        {
          field: ['id'],
          message: 'Metaobject not found.',
          code: 'NOT_FOUND',
          elementKey: null,
          elementIndex: null,
        },
      ],
    });

    const readAfterMutationResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query ReadAfterMutations($id: ID!, $oldHandle: MetaobjectHandleInput!, $newHandle: MetaobjectHandleInput!, $deletedId: ID!, $deletedHandle: MetaobjectHandleInput!, $type: String!) {
          detail: metaobject(id: $id) { id handle displayName fields { key value } capabilities { publishable { status } } }
          oldHandle: metaobjectByHandle(handle: $oldHandle) { id }
          newHandle: metaobjectByHandle(handle: $newHandle) { id handle }
          deleted: metaobject(id: $deletedId) { id }
          deletedHandle: metaobjectByHandle(handle: $deletedHandle) { id }
          catalog: metaobjects(type: $type, first: 10) { nodes { id handle displayName } }
          definition: metaobjectDefinitionByType(type: $type) { metaobjectsCount }
        }`,
        variables: {
          id: created.id,
          oldHandle: { type: 'codex_metaobject_rows', handle: 'created-entry' },
          newHandle: { type: 'codex_metaobject_rows', handle: 'renamed-entry' },
          deletedId: upsertCreated.id,
          deletedHandle: { type: 'codex_metaobject_rows', handle: 'upsert-created' },
          type: 'codex_metaobject_rows',
        },
      });

    expect(readAfterMutationResponse.body.data).toMatchObject({
      detail: {
        id: created.id,
        handle: 'renamed-entry',
        displayName: 'Updated title',
        capabilities: { publishable: { status: 'ACTIVE' } },
      },
      oldHandle: null,
      newHandle: { id: created.id, handle: 'renamed-entry' },
      deleted: null,
      deletedHandle: null,
      catalog: { nodes: [{ id: created.id, handle: 'renamed-entry', displayName: 'Updated title' }] },
      definition: { metaobjectsCount: 1 },
    });

    const logResponse = await request(app).get('/__meta/log');
    expect(logResponse.body.entries.map((entry: { operationName: string }) => entry.operationName)).toEqual([
      'metaobjectCreate',
      'metaobjectUpdate',
      'metaobjectUpsert',
      'metaobjectUpsert',
      'metaobjectDelete',
      'metaobjectDelete',
    ]);
    expect(logResponse.body.entries[0].requestBody.operationName).toBe('CreateMetaobject');

    const stateResponse = await request(app).get('/__meta/state');
    expect(stateResponse.body.stagedState.metaobjects[created.id]).toMatchObject({
      id: created.id,
      handle: 'renamed-entry',
      displayName: 'Updated title',
    });
    expect(stateResponse.body.stagedState.deletedMetaobjectIds[upsertCreated.id]).toBe(true);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages metaobjectBulkDelete with ordered missing-row errors and downstream absence', async () => {
    store.upsertBaseMetaobjectDefinitions([makeDefinition({ metaobjectsCount: 2 })]);
    store.upsertBaseMetaobjects([
      makeEntry({ id: 'gid://shopify/Metaobject/2441', handle: 'bulk-one', displayName: 'Bulk one' }),
      makeEntry({ id: 'gid://shopify/Metaobject/2442', handle: 'bulk-two', displayName: 'Bulk two' }),
    ]);
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('metaobject bulk delete must stay local');
    });
    const app = createApp(config).callback();

    const deleteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        operationName: 'BulkDeleteMetaobjects',
        query: `mutation BulkDeleteMetaobjects($ids: [ID!]!) {
          metaobjectBulkDelete(ids: $ids) {
            job { id done }
            userErrors { field message code elementKey elementIndex }
          }
        }`,
        variables: {
          ids: ['gid://shopify/Metaobject/2441', 'gid://shopify/Metaobject/missing', 'gid://shopify/Metaobject/2442'],
        },
      });

    expect(deleteResponse.status).toBe(200);
    expect(deleteResponse.body.data.metaobjectBulkDelete.job).toMatchObject({ done: true });
    expect(deleteResponse.body.data.metaobjectBulkDelete.job.id).toMatch(/^gid:\/\/shopify\/Job\//u);
    expect(deleteResponse.body.data.metaobjectBulkDelete.userErrors).toEqual([
      {
        field: ['ids', '1'],
        message: 'Metaobject not found.',
        code: 'NOT_FOUND',
        elementKey: null,
        elementIndex: 1,
      },
    ]);

    const readResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query BulkDeleteReads($type: String!) {
          first: metaobject(id: "gid://shopify/Metaobject/2441") { id }
          second: metaobject(id: "gid://shopify/Metaobject/2442") { id }
          catalog: metaobjects(type: $type, first: 10) { nodes { id } }
          definition: metaobjectDefinitionByType(type: $type) { metaobjectsCount }
        }`,
        variables: { type: 'codex_metaobject_rows' },
      });

    expect(readResponse.body.data).toEqual({
      first: null,
      second: null,
      catalog: { nodes: [] },
      definition: { metaobjectsCount: 0 },
    });

    const stateResponse = await request(app).get('/__meta/state');
    expect(stateResponse.body.stagedState.deletedMetaobjectIds).toMatchObject({
      'gid://shopify/Metaobject/2441': true,
      'gid://shopify/Metaobject/2442': true,
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});

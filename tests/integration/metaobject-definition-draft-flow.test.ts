import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { resetSyntheticIdentity } from '../../src/state/synthetic-identity.js';
import { store } from '../../src/state/store.js';
import type { MetaobjectDefinitionRecord } from '../../src/state/types.js';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

function makeDefinition(overrides: Partial<MetaobjectDefinitionRecord> = {}): MetaobjectDefinitionRecord {
  return {
    id: overrides.id ?? 'gid://shopify/MetaobjectDefinition/900',
    type: overrides.type ?? 'codex_existing_definition',
    name: overrides.name ?? 'Codex Existing Definition',
    description: overrides.description ?? 'Existing definition.',
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
        type: {
          name: 'single_line_text_field',
          category: 'TEXT',
        },
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

describe('metaobject definition draft flow', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('stages create, update, delete, downstream reads, and meta API visibility without upstream writes', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('metaobject definition mutations must stay local');
    });
    const app = createApp(config).callback();

    const createResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        operationName: 'CreateDefinition',
        query: `mutation CreateDefinition($definition: MetaobjectDefinitionCreateInput!) {
          metaobjectDefinitionCreate(definition: $definition) {
            metaobjectDefinition {
              id
              type
              name
              description
              displayNameKey
              access { admin storefront }
              capabilities {
                publishable { enabled }
                translatable { enabled }
                renderable { enabled }
                onlineStore { enabled }
              }
              fieldDefinitions {
                key
                name
                description
                required
                type { name category }
                validations { name value }
              }
              metaobjectsCount
              standardTemplate { type name }
            }
            userErrors { field message code elementKey elementIndex }
          }
        }`,
        variables: {
          definition: {
            type: 'codex_stage_definition',
            name: 'Codex Stage Definition',
            description: 'A locally staged metaobject definition.',
            capabilities: {
              publishable: { enabled: true },
              translatable: { enabled: false },
            },
            displayNameKey: 'title',
            fieldDefinitions: [
              {
                key: 'title',
                name: 'Title',
                description: 'Title field.',
                type: 'single_line_text_field',
                required: true,
              },
              {
                key: 'body',
                name: 'Body',
                description: 'Body field.',
                type: 'multi_line_text_field',
                required: false,
                validations: [{ name: 'max', value: '500' }],
              },
            ],
          },
        },
      });

    expect(createResponse.status).toBe(200);
    expect(createResponse.body.data.metaobjectDefinitionCreate.userErrors).toEqual([]);
    const createdDefinition = createResponse.body.data.metaobjectDefinitionCreate.metaobjectDefinition;
    expect(createdDefinition).toMatchObject({
      id: 'gid://shopify/MetaobjectDefinition/1?shopify-draft-proxy=synthetic',
      type: 'codex_stage_definition',
      name: 'Codex Stage Definition',
      description: 'A locally staged metaobject definition.',
      displayNameKey: 'title',
      access: {
        admin: 'PUBLIC_READ_WRITE',
        storefront: 'NONE',
      },
      capabilities: {
        publishable: { enabled: true },
        translatable: { enabled: false },
        renderable: { enabled: false },
        onlineStore: { enabled: false },
      },
      fieldDefinitions: [
        {
          key: 'title',
          name: 'Title',
          description: 'Title field.',
          required: true,
          type: { name: 'single_line_text_field', category: 'TEXT' },
          validations: [],
        },
        {
          key: 'body',
          name: 'Body',
          description: 'Body field.',
          required: false,
          type: { name: 'multi_line_text_field', category: 'TEXT' },
          validations: [{ name: 'max', value: '500' }],
        },
      ],
      metaobjectsCount: 0,
      standardTemplate: null,
    });

    const updateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        operationName: 'UpdateDefinition',
        query: `mutation UpdateDefinition($id: ID!, $definition: MetaobjectDefinitionUpdateInput!) {
          metaobjectDefinitionUpdate(id: $id, definition: $definition, resetFieldOrder: true) {
            metaobjectDefinition {
              id
              type
              name
              description
              displayNameKey
              access { admin storefront }
              capabilities { publishable { enabled } translatable { enabled } renderable { enabled } onlineStore { enabled } }
              fieldDefinitions { key name description required type { name category } validations { name value } }
            }
            userErrors { field message code elementKey elementIndex }
          }
        }`,
        variables: {
          id: createdDefinition.id,
          definition: {
            name: 'Codex Updated Definition',
            description: 'Updated locally.',
            displayNameKey: 'body',
            access: {
              storefront: 'PUBLIC_READ',
            },
            capabilities: {
              translatable: { enabled: true },
              renderable: { enabled: true },
            },
            fieldDefinitions: [
              {
                update: {
                  key: 'body',
                  name: 'Body Copy',
                  description: 'Updated body.',
                  required: true,
                  validations: [{ name: 'max', value: '250' }],
                },
              },
              {
                create: {
                  key: 'summary',
                  name: 'Summary',
                  description: 'Summary field.',
                  type: 'single_line_text_field',
                  required: false,
                },
              },
              {
                delete: {
                  key: 'title',
                },
              },
            ],
          },
        },
      });

    expect(updateResponse.status).toBe(200);
    expect(updateResponse.body.data.metaobjectDefinitionUpdate.userErrors).toEqual([]);
    expect(updateResponse.body.data.metaobjectDefinitionUpdate.metaobjectDefinition).toMatchObject({
      id: createdDefinition.id,
      type: 'codex_stage_definition',
      name: 'Codex Updated Definition',
      description: 'Updated locally.',
      displayNameKey: 'body',
      access: {
        admin: 'PUBLIC_READ_WRITE',
        storefront: 'PUBLIC_READ',
      },
      capabilities: {
        publishable: { enabled: true },
        translatable: { enabled: true },
        renderable: { enabled: true },
        onlineStore: { enabled: false },
      },
      fieldDefinitions: [
        {
          key: 'body',
          name: 'Body Copy',
          description: 'Updated body.',
          required: true,
          type: { name: 'multi_line_text_field', category: 'TEXT' },
          validations: [{ name: 'max', value: '250' }],
        },
        {
          key: 'summary',
          name: 'Summary',
          description: 'Summary field.',
          required: false,
          type: { name: 'single_line_text_field', category: 'TEXT' },
          validations: [],
        },
      ],
    });

    const readResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query ReadDefinition($id: ID!, $type: String!) {
          byId: metaobjectDefinition(id: $id) { id name displayNameKey fieldDefinitions { key } }
          byType: metaobjectDefinitionByType(type: $type) { id name displayNameKey fieldDefinitions { key } }
          catalog: metaobjectDefinitions(first: 5) {
            nodes { id type name }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }`,
        variables: {
          id: createdDefinition.id,
          type: 'codex_stage_definition',
        },
      });

    expect(readResponse.status).toBe(200);
    expect(readResponse.body.data.byId).toEqual({
      id: createdDefinition.id,
      name: 'Codex Updated Definition',
      displayNameKey: 'body',
      fieldDefinitions: [{ key: 'body' }, { key: 'summary' }],
    });
    expect(readResponse.body.data.byType).toEqual(readResponse.body.data.byId);
    expect(readResponse.body.data.catalog.nodes).toEqual([
      {
        id: createdDefinition.id,
        type: 'codex_stage_definition',
        name: 'Codex Updated Definition',
      },
    ]);

    const stateResponse = await request(app).get('/__meta/state');
    expect(stateResponse.status).toBe(200);
    expect(stateResponse.body.stagedState.metaobjectDefinitions[createdDefinition.id]).toMatchObject({
      id: createdDefinition.id,
      name: 'Codex Updated Definition',
      displayNameKey: 'body',
    });

    const deleteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        operationName: 'DeleteDefinition',
        query: `mutation DeleteDefinition($id: ID!) {
          metaobjectDefinitionDelete(id: $id) {
            deletedId
            userErrors { field message code elementKey elementIndex }
          }
        }`,
        variables: {
          id: createdDefinition.id,
        },
      });

    expect(deleteResponse.status).toBe(200);
    expect(deleteResponse.body.data.metaobjectDefinitionDelete).toEqual({
      deletedId: createdDefinition.id,
      userErrors: [],
    });

    const afterDeleteRead = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query ReadDeleted($id: ID!, $type: String!) {
          byId: metaobjectDefinition(id: $id) { id }
          byType: metaobjectDefinitionByType(type: $type) { id }
          catalog: metaobjectDefinitions(first: 5) { nodes { id } }
        }`,
        variables: {
          id: createdDefinition.id,
          type: 'codex_stage_definition',
        },
      });

    expect(afterDeleteRead.body.data).toEqual({
      byId: null,
      byType: null,
      catalog: {
        nodes: [],
      },
    });

    const logResponse = await request(app).get('/__meta/log');
    expect(logResponse.body.entries.map((entry: { operationName: string }) => entry.operationName)).toEqual([
      'metaobjectDefinitionCreate',
      'metaobjectDefinitionUpdate',
      'metaobjectDefinitionDelete',
    ]);
    expect(logResponse.body.entries.map((entry: { status: string }) => entry.status)).toEqual([
      'staged',
      'staged',
      'staged',
    ]);
    expect(logResponse.body.entries.map((entry: { stagedResourceIds: string[] }) => entry.stagedResourceIds)).toEqual([
      [createdDefinition.id],
      [createdDefinition.id],
      [createdDefinition.id],
    ]);
    expect(
      logResponse.body.entries.map((entry: { requestBody: { operationName?: string } }) => entry.requestBody),
    ).toEqual([
      expect.objectContaining({ operationName: 'CreateDefinition' }),
      expect.objectContaining({ operationName: 'UpdateDefinition' }),
      expect.objectContaining({ operationName: 'DeleteDefinition' }),
    ]);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('returns local userErrors for captured guardrails and unsupported destructive branches', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('metaobject definition guardrails must stay local');
    });
    store.upsertBaseMetaobjectDefinitions([
      makeDefinition({
        id: 'gid://shopify/MetaobjectDefinition/901',
        type: 'codex_nonempty_definition',
        metaobjectsCount: 2,
      }),
    ]);
    const app = createApp(config).callback();

    const accessResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation InvalidAccess($definition: MetaobjectDefinitionCreateInput!) {
          metaobjectDefinitionCreate(definition: $definition) {
            metaobjectDefinition { id }
            userErrors { field message code elementKey elementIndex }
          }
        }`,
        variables: {
          definition: {
            type: 'codex_bad_access',
            name: 'Bad Access',
            access: {
              admin: 'MERCHANT_READ',
            },
            fieldDefinitions: [{ key: 'title', name: 'Title', type: 'single_line_text_field' }],
          },
        },
      });

    expect(accessResponse.status).toBe(200);
    expect(accessResponse.body.data.metaobjectDefinitionCreate).toEqual({
      metaobjectDefinition: null,
      userErrors: [
        {
          field: ['definition', 'access', 'admin'],
          message: 'Admin access can only be specified on metaobject definitions that have an app-reserved type.',
          code: 'ADMIN_ACCESS_INPUT_NOT_ALLOWED',
          elementKey: null,
          elementIndex: null,
        },
      ],
    });

    const deleteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation DeleteNonEmpty($id: ID!) {
          metaobjectDefinitionDelete(id: $id) {
            deletedId
            userErrors { field message code elementKey elementIndex }
          }
        }`,
        variables: {
          id: 'gid://shopify/MetaobjectDefinition/901',
        },
      });

    expect(deleteResponse.status).toBe(200);
    expect(deleteResponse.body.data.metaobjectDefinitionDelete).toEqual({
      deletedId: null,
      userErrors: [
        {
          field: ['id'],
          message:
            'Local proxy cannot delete a metaobject definition with associated metaobjects until entry cascade behavior is modeled.',
          code: 'UNSUPPORTED',
          elementKey: null,
          elementIndex: null,
        },
      ],
    });

    const missingUpdateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation UpdateMissingDefinition($id: ID!, $definition: MetaobjectDefinitionUpdateInput!) {
          metaobjectDefinitionUpdate(id: $id, definition: $definition) {
            metaobjectDefinition { id }
            userErrors { field message code elementKey elementIndex }
          }
        }`,
        variables: {
          id: 'gid://shopify/MetaobjectDefinition/0',
          definition: { name: 'Missing' },
        },
      });

    expect(missingUpdateResponse.body.data.metaobjectDefinitionUpdate).toEqual({
      metaobjectDefinition: null,
      userErrors: [
        {
          field: ['id'],
          message: 'Record not found',
          code: 'RECORD_NOT_FOUND',
          elementKey: null,
          elementIndex: null,
        },
      ],
    });

    const missingVariableResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation DeleteMissingVariable($id: ID!) {
          metaobjectDefinitionDelete(id: $id) {
            deletedId
            userErrors { field message code }
          }
        }`,
        variables: {},
      });

    expect(missingVariableResponse.body).toEqual({
      errors: [
        {
          message: 'Variable $id of type ID! was provided invalid value',
          extensions: {
            code: 'INVALID_VARIABLE',
            value: null,
            problems: [{ path: [], explanation: 'Expected value to not be null' }],
          },
        },
      ],
    });

    const readResponse = await request(app).post('/admin/api/2026-04/graphql.json').send({
      query: `query { metaobjectDefinition(id: "gid://shopify/MetaobjectDefinition/901") { id type metaobjectsCount } }`,
    });

    expect(readResponse.body.data.metaobjectDefinition).toEqual({
      id: 'gid://shopify/MetaobjectDefinition/901',
      type: 'codex_nonempty_definition',
      metaobjectsCount: 2,
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages captured-safe standard metaobject definition enablement and blocks unknown templates locally', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('standard metaobject definition enablement must stay local');
    });
    const app = createApp(config).callback();

    const enableResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation EnableStandard($type: String!) {
          standardMetaobjectDefinitionEnable(type: $type) {
            metaobjectDefinition {
              id
              type
              name
              displayNameKey
              standardTemplate { type name }
              fieldDefinitions { key name required type { name category } }
            }
            userErrors { field message code }
          }
        }`,
        variables: {
          type: 'shopify--qa-pair',
        },
      });

    expect(enableResponse.status).toBe(200);
    expect(enableResponse.body.data.standardMetaobjectDefinitionEnable).toEqual({
      metaobjectDefinition: {
        id: 'gid://shopify/MetaobjectDefinition/1?shopify-draft-proxy=synthetic',
        type: 'shopify--qa-pair',
        name: 'Q&A pair',
        displayNameKey: 'question',
        standardTemplate: {
          type: 'shopify--qa-pair',
          name: 'Q&A pair',
        },
        fieldDefinitions: [
          {
            key: 'question',
            name: 'Question',
            required: true,
            type: { name: 'single_line_text_field', category: 'TEXT' },
          },
          {
            key: 'answer',
            name: 'Answer',
            required: true,
            type: { name: 'multi_line_text_field', category: 'TEXT' },
          },
        ],
      },
      userErrors: [],
    });

    const unknownResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation EnableUnknown($type: String!) {
          standardMetaobjectDefinitionEnable(type: $type) {
            metaobjectDefinition { id }
            userErrors { field message code }
          }
        }`,
        variables: {
          type: 'shopify--unknown-template',
        },
      });

    expect(unknownResponse.status).toBe(200);
    expect(unknownResponse.body.data.standardMetaobjectDefinitionEnable).toEqual({
      metaobjectDefinition: null,
      userErrors: [
        {
          field: ['type'],
          message: "A standard metaobject definition wasn't found for the specified type.",
          code: 'TEMPLATE_NOT_FOUND',
        },
      ],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});

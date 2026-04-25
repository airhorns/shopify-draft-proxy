import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { resetSyntheticIdentity } from '../../src/state/synthetic-identity.js';
import { store } from '../../src/state/store.js';
import type { MetaobjectDefinitionRecord } from '../../src/state/types.js';

const snapshotConfig: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

const liveHybridConfig: AppConfig = {
  ...snapshotConfig,
  readMode: 'live-hybrid',
};

function makeDefinition(overrides: Partial<MetaobjectDefinitionRecord> = {}): MetaobjectDefinitionRecord {
  return {
    id: overrides.id ?? 'gid://shopify/MetaobjectDefinition/100',
    type: overrides.type ?? 'codex_metaobject_test',
    name: overrides.name ?? 'Codex Metaobject Test',
    description: overrides.description ?? 'Metaobject definition fixture.',
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
      {
        key: 'body',
        name: 'Body',
        description: 'Body text.',
        required: false,
        type: {
          name: 'multi_line_text_field',
          category: 'TEXT',
        },
        validations: [{ name: 'max', value: '500' }],
      },
    ],
    hasThumbnailField: overrides.hasThumbnailField ?? false,
    metaobjectsCount: overrides.metaobjectsCount ?? 1,
    standardTemplate: overrides.standardTemplate ?? null,
    createdAt: overrides.createdAt ?? null,
    updatedAt: overrides.updatedAt ?? null,
  };
}

describe('metaobject definition query shapes', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('returns null singular lookups and empty catalog connections without live access in snapshot mode', async () => {
    vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('metaobject definition snapshot reads must stay local'));
    const app = createApp(snapshotConfig).callback();

    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query MissingDefinitions($id: ID!, $type: String!) {
          byId: metaobjectDefinition(id: $id) { id name }
          byType: metaobjectDefinitionByType(type: $type) { id name }
          catalog: metaobjectDefinitions(first: 5) {
            nodes { id }
            edges { cursor node { id } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }`,
        variables: {
          id: 'gid://shopify/MetaobjectDefinition/0',
          type: 'missing_type',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        byId: null,
        byType: null,
        catalog: {
          nodes: [],
          edges: [],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: false,
            startCursor: null,
            endCursor: null,
          },
        },
      },
    });
    expect(globalThis.fetch).not.toHaveBeenCalled();
  });

  it('serializes catalog, detail, type lookup, aliases, fragments, and field order from normalized snapshot state', async () => {
    store.upsertBaseMetaobjectDefinitions([
      makeDefinition({
        id: 'gid://shopify/MetaobjectDefinition/200',
        type: 'other_type',
        name: 'Other Type',
        displayNameKey: 'heading',
        metaobjectsCount: 0,
      }),
      makeDefinition(),
    ]);
    const app = createApp(snapshotConfig).callback();

    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `fragment DefinitionFields on MetaobjectDefinition {
          id
          definitionType: type
          name
          description
          displayKey: displayNameKey
          access { admin storefront }
          capabilities {
            publishable { enabled }
            translatable { enabled }
            renderable { enabled }
            onlineStore { enabled }
          }
          fieldDefinitions {
            fieldKey: key
            name
            description
            required
            type { name category }
            validations { name value }
          }
          hasThumbnailField
          metaobjectsCount
          standardTemplate { type name }
        }

        query DefinitionReads($id: ID!, $type: String!) {
          catalog: metaobjectDefinitions(first: 5, reverse: true) {
            edges { cursor node { ...DefinitionFields } }
            nodes { ...DefinitionFields }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          detail: metaobjectDefinition(id: $id) { ...DefinitionFields }
          byType: metaobjectDefinitionByType(type: $type) { ...DefinitionFields }
        }`,
        variables: {
          id: 'gid://shopify/MetaobjectDefinition/100',
          type: 'codex_metaobject_test',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.detail).toEqual({
      id: 'gid://shopify/MetaobjectDefinition/100',
      definitionType: 'codex_metaobject_test',
      name: 'Codex Metaobject Test',
      description: 'Metaobject definition fixture.',
      displayKey: 'title',
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
          fieldKey: 'title',
          name: 'Title',
          description: 'Display title.',
          required: true,
          type: { name: 'single_line_text_field', category: 'TEXT' },
          validations: [],
        },
        {
          fieldKey: 'body',
          name: 'Body',
          description: 'Body text.',
          required: false,
          type: { name: 'multi_line_text_field', category: 'TEXT' },
          validations: [{ name: 'max', value: '500' }],
        },
      ],
      hasThumbnailField: false,
      metaobjectsCount: 1,
      standardTemplate: null,
    });
    expect(response.body.data.byType).toEqual(response.body.data.detail);
    expect(response.body.data.catalog.nodes.map((definition: { id: string }) => definition.id)).toEqual([
      'gid://shopify/MetaobjectDefinition/200',
      'gid://shopify/MetaobjectDefinition/100',
    ]);
    expect(response.body.data.catalog.edges[1]).toEqual({
      cursor: 'cursor:gid://shopify/MetaobjectDefinition/100',
      node: response.body.data.detail,
    });
    expect(response.body.data.catalog.pageInfo).toEqual({
      hasNextPage: false,
      hasPreviousPage: false,
      startCursor: 'cursor:gid://shopify/MetaobjectDefinition/200',
      endCursor: 'cursor:gid://shopify/MetaobjectDefinition/100',
    });
  });

  it('overlays local definitions on live-hybrid upstream catalog, id, and type responses', async () => {
    const definition = makeDefinition({
      id: 'gid://shopify/MetaobjectDefinition/300',
      type: 'staged_metaobject_type',
      name: 'Staged Metaobject Type',
    });
    store.upsertStagedMetaobjectDefinitions([definition]);
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          data: {
            catalog: {
              nodes: [],
              edges: [],
              pageInfo: {
                hasNextPage: false,
                hasPreviousPage: false,
                startCursor: null,
                endCursor: null,
              },
            },
            byId: null,
            byType: null,
          },
        }),
        { status: 200, headers: { 'content-type': 'application/json' } },
      ),
    );
    const app = createApp(liveHybridConfig).callback();

    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query OverlayDefinitions($id: ID!, $type: String!) {
          catalog: metaobjectDefinitions(first: 5) {
            nodes { id type name displayNameKey metaobjectsCount }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          byId: metaobjectDefinition(id: $id) { id type name }
          byType: metaobjectDefinitionByType(type: $type) { id type name }
        }`,
        variables: {
          id: 'gid://shopify/MetaobjectDefinition/300',
          type: 'staged_metaobject_type',
        },
      });

    expect(response.status).toBe(200);
    expect(fetchSpy).toHaveBeenCalledOnce();
    expect(response.body.data).toEqual({
      catalog: {
        nodes: [
          {
            id: 'gid://shopify/MetaobjectDefinition/300',
            type: 'staged_metaobject_type',
            name: 'Staged Metaobject Type',
            displayNameKey: 'title',
            metaobjectsCount: 1,
          },
        ],
        pageInfo: {
          hasNextPage: false,
          hasPreviousPage: false,
          startCursor: 'cursor:gid://shopify/MetaobjectDefinition/300',
          endCursor: 'cursor:gid://shopify/MetaobjectDefinition/300',
        },
      },
      byId: {
        id: 'gid://shopify/MetaobjectDefinition/300',
        type: 'staged_metaobject_type',
        name: 'Staged Metaobject Type',
      },
      byType: {
        id: 'gid://shopify/MetaobjectDefinition/300',
        type: 'staged_metaobject_type',
        name: 'Staged Metaobject Type',
      },
    });
  });

  it('preserves upstream no-data/null responses in live-hybrid when no local definition exists', async () => {
    const upstreamBody = {
      data: {
        catalog: {
          nodes: [],
          edges: [],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: false,
            startCursor: null,
            endCursor: null,
          },
        },
        byId: null,
        byType: null,
      },
    };
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify(upstreamBody), { status: 200, headers: { 'content-type': 'application/json' } }),
    );
    const app = createApp(liveHybridConfig).callback();

    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query PreserveNoData($id: ID!, $type: String!) {
          catalog: metaobjectDefinitions(first: 5) {
            nodes { id }
            edges { cursor node { id } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          byId: metaobjectDefinition(id: $id) { id }
          byType: metaobjectDefinitionByType(type: $type) { id }
        }`,
        variables: {
          id: 'gid://shopify/MetaobjectDefinition/0',
          type: 'missing_metaobject_type',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual(upstreamBody);
  });
});

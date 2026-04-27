import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { resetSyntheticIdentity } from '../../src/state/synthetic-identity.js';
import { store } from '../../src/state/store.js';
import type { MetaobjectDefinitionRecord, MetaobjectRecord } from '../../src/state/types.js';

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
    fieldDefinitions: overrides.fieldDefinitions ?? [],
    hasThumbnailField: overrides.hasThumbnailField ?? false,
    metaobjectsCount: overrides.metaobjectsCount ?? 1,
    standardTemplate: overrides.standardTemplate ?? null,
    createdAt: overrides.createdAt ?? null,
    updatedAt: overrides.updatedAt ?? null,
  };
}

function makeEntry(overrides: Partial<MetaobjectRecord> = {}): MetaobjectRecord {
  const title = overrides.displayName ?? 'Alpha title';

  return {
    id: overrides.id ?? 'gid://shopify/Metaobject/100',
    handle: overrides.handle ?? 'alpha-entry',
    type: overrides.type ?? 'codex_metaobject_test',
    displayName: title,
    createdAt: overrides.createdAt ?? '2026-04-25T22:40:00Z',
    updatedAt: overrides.updatedAt ?? '2026-04-25T22:40:46Z',
    capabilities: overrides.capabilities ?? {
      publishable: { status: 'ACTIVE' },
      onlineStore: null,
    },
    fields: overrides.fields ?? [
      {
        key: 'title',
        type: 'single_line_text_field',
        value: title,
        jsonValue: title,
        definition: {
          key: 'title',
          name: 'Title',
          required: true,
          type: {
            name: 'single_line_text_field',
            category: 'TEXT',
          },
        },
      },
      {
        key: 'body',
        type: 'multi_line_text_field',
        value: 'Alpha body',
        jsonValue: 'Alpha body',
        definition: {
          key: 'body',
          name: 'Body',
          required: false,
          type: {
            name: 'multi_line_text_field',
            category: 'TEXT',
          },
        },
      },
    ],
  };
}

describe('metaobject query shapes', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('returns null singular lookups and empty catalog connections without live access in snapshot mode', async () => {
    vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('metaobject snapshot reads must stay local'));
    const app = createApp(snapshotConfig).callback();

    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query MissingEntries($id: ID!, $handle: MetaobjectHandleInput!, $type: String!) {
          byId: metaobject(id: $id) { id handle }
          byHandle: metaobjectByHandle(handle: $handle) { id handle }
          catalog: metaobjects(type: $type, first: 5) {
            nodes { id }
            edges { cursor node { id } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }`,
        variables: {
          id: 'gid://shopify/Metaobject/0',
          handle: {
            type: 'missing_type',
            handle: 'missing-handle',
          },
          type: 'missing_type',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        byId: null,
        byHandle: null,
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

  it('serializes entry detail, handle lookup, aliases, fields, field(key:), and empty referencedBy', async () => {
    store.upsertBaseMetaobjectDefinitions([makeDefinition()]);
    store.upsertBaseMetaobjects([makeEntry()]);
    const app = createApp(snapshotConfig).callback();

    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `fragment EntryFields on Metaobject {
          entryId: id
          handle
          type
          displayName
          updatedAt
          capabilities {
            publishable { status }
            onlineStore { templateSuffix }
          }
          fields {
            key
            type
            value
            jsonValue
            definition { key name required type { name category } }
          }
          titleField: field(key: $fieldKey) {
            key
            type
            value
            jsonValue
            definition { key name required type { name category } }
          }
          missingField: field(key: "missing") { key }
          refs: referencedBy(first: 5) {
            nodes { __typename }
            edges { cursor node { __typename } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }

        query EntryReads($id: ID!, $handle: MetaobjectHandleInput!, $type: String!, $fieldKey: String!) {
          catalog: metaobjects(type: $type, first: 5) {
            edges { cursor node { ...EntryFields } }
            nodes { ...EntryFields }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          detail: metaobject(id: $id) { ...EntryFields }
          byHandle: metaobjectByHandle(handle: $handle) { ...EntryFields }
        }`,
        variables: {
          id: 'gid://shopify/Metaobject/100',
          handle: {
            type: 'codex_metaobject_test',
            handle: 'alpha-entry',
          },
          type: 'codex_metaobject_test',
          fieldKey: 'title',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.detail).toEqual({
      entryId: 'gid://shopify/Metaobject/100',
      handle: 'alpha-entry',
      type: 'codex_metaobject_test',
      displayName: 'Alpha title',
      updatedAt: '2026-04-25T22:40:46Z',
      capabilities: {
        publishable: { status: 'ACTIVE' },
        onlineStore: null,
      },
      fields: [
        {
          key: 'title',
          type: 'single_line_text_field',
          value: 'Alpha title',
          jsonValue: 'Alpha title',
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
          value: 'Alpha body',
          jsonValue: 'Alpha body',
          definition: {
            key: 'body',
            name: 'Body',
            required: false,
            type: { name: 'multi_line_text_field', category: 'TEXT' },
          },
        },
      ],
      titleField: {
        key: 'title',
        type: 'single_line_text_field',
        value: 'Alpha title',
        jsonValue: 'Alpha title',
        definition: {
          key: 'title',
          name: 'Title',
          required: true,
          type: { name: 'single_line_text_field', category: 'TEXT' },
        },
      },
      missingField: null,
      refs: {
        nodes: [],
        edges: [],
        pageInfo: {
          hasNextPage: false,
          hasPreviousPage: false,
          startCursor: null,
          endCursor: null,
        },
      },
    });
    expect(response.body.data.byHandle).toEqual(response.body.data.detail);
    expect(response.body.data.catalog.nodes).toEqual([response.body.data.detail]);
    expect(response.body.data.catalog.edges).toEqual([
      {
        cursor: 'cursor:gid://shopify/Metaobject/100',
        node: response.body.data.detail,
      },
    ]);
    expect(response.body.data.catalog.pageInfo).toEqual({
      hasNextPage: false,
      hasPreviousPage: false,
      startCursor: 'cursor:gid://shopify/Metaobject/100',
      endCursor: 'cursor:gid://shopify/Metaobject/100',
    });
  });

  it('honors type scoping, pagination, reverse sorting, and field-value query filters', async () => {
    store.upsertBaseMetaobjects([
      makeEntry({
        id: 'gid://shopify/Metaobject/100',
        handle: 'alpha-entry',
        displayName: 'Alpha title',
      }),
      makeEntry({
        id: 'gid://shopify/Metaobject/200',
        handle: 'bravo-entry',
        displayName: 'Bravo title',
        updatedAt: '2026-04-25T22:41:46Z',
      }),
      makeEntry({
        id: 'gid://shopify/Metaobject/300',
        handle: 'other-entry',
        type: 'other_type',
        displayName: 'Other title',
      }),
    ]);
    const app = createApp(snapshotConfig).callback();

    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query CatalogVariants($type: String!, $after: String!) {
          firstPage: metaobjects(type: $type, first: 1, sortKey: "display_name", reverse: true) {
            nodes { id displayName }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          secondPage: metaobjects(type: $type, first: 1, after: $after, sortKey: "display_name", reverse: true) {
            nodes { id displayName }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          filtered: metaobjects(type: $type, first: 5, query: "fields.title:Alpha") {
            nodes { id displayName }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }`,
        variables: {
          type: 'codex_metaobject_test',
          after: 'cursor:gid://shopify/Metaobject/200',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.firstPage).toEqual({
      nodes: [{ id: 'gid://shopify/Metaobject/200', displayName: 'Bravo title' }],
      pageInfo: {
        hasNextPage: true,
        hasPreviousPage: false,
        startCursor: 'cursor:gid://shopify/Metaobject/200',
        endCursor: 'cursor:gid://shopify/Metaobject/200',
      },
    });
    expect(response.body.data.secondPage).toEqual({
      nodes: [{ id: 'gid://shopify/Metaobject/100', displayName: 'Alpha title' }],
      pageInfo: {
        hasNextPage: false,
        hasPreviousPage: true,
        startCursor: 'cursor:gid://shopify/Metaobject/100',
        endCursor: 'cursor:gid://shopify/Metaobject/100',
      },
    });
    expect(response.body.data.filtered).toEqual({
      nodes: [{ id: 'gid://shopify/Metaobject/100', displayName: 'Alpha title' }],
      pageInfo: {
        hasNextPage: false,
        hasPreviousPage: false,
        startCursor: 'cursor:gid://shopify/Metaobject/100',
        endCursor: 'cursor:gid://shopify/Metaobject/100',
      },
    });
  });

  it('overlays local entries on live-hybrid upstream catalog, id, and handle responses', async () => {
    const entry = makeEntry({
      id: 'gid://shopify/Metaobject/400',
      handle: 'staged-entry',
      displayName: 'Staged title',
    });
    store.upsertStagedMetaobjects([entry]);
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
            byHandle: null,
          },
        }),
        { status: 200, headers: { 'content-type': 'application/json' } },
      ),
    );
    const app = createApp(liveHybridConfig).callback();

    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query OverlayEntries($id: ID!, $handle: MetaobjectHandleInput!, $type: String!) {
          catalog: metaobjects(type: $type, first: 5) {
            nodes { id handle displayName }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          byId: metaobject(id: $id) { id handle displayName }
          byHandle: metaobjectByHandle(handle: $handle) { id handle displayName }
        }`,
        variables: {
          id: 'gid://shopify/Metaobject/400',
          handle: {
            type: 'codex_metaobject_test',
            handle: 'staged-entry',
          },
          type: 'codex_metaobject_test',
        },
      });

    expect(response.status).toBe(200);
    expect(fetchSpy).toHaveBeenCalledOnce();
    expect(response.body.data).toEqual({
      catalog: {
        nodes: [
          {
            id: 'gid://shopify/Metaobject/400',
            handle: 'staged-entry',
            displayName: 'Staged title',
          },
        ],
        pageInfo: {
          hasNextPage: false,
          hasPreviousPage: false,
          startCursor: 'cursor:gid://shopify/Metaobject/400',
          endCursor: 'cursor:gid://shopify/Metaobject/400',
        },
      },
      byId: {
        id: 'gid://shopify/Metaobject/400',
        handle: 'staged-entry',
        displayName: 'Staged title',
      },
      byHandle: {
        id: 'gid://shopify/Metaobject/400',
        handle: 'staged-entry',
        displayName: 'Staged title',
      },
    });
  });

  it('preserves upstream no-data/null responses in live-hybrid when no local entry exists', async () => {
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
        byHandle: null,
      },
    };
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify(upstreamBody), { status: 200, headers: { 'content-type': 'application/json' } }),
    );
    const app = createApp(liveHybridConfig).callback();

    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query PreserveNoData($id: ID!, $handle: MetaobjectHandleInput!, $type: String!) {
          catalog: metaobjects(type: $type, first: 5) {
            nodes { id }
            edges { cursor node { id } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          byId: metaobject(id: $id) { id }
          byHandle: metaobjectByHandle(handle: $handle) { id }
        }`,
        variables: {
          id: 'gid://shopify/Metaobject/0',
          handle: {
            type: 'missing_type',
            handle: 'missing-handle',
          },
          type: 'missing_type',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual(upstreamBody);
  });
});

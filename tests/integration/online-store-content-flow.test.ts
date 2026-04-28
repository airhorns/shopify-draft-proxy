import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../support/runtime.js';
import type { AppConfig } from '../../src/config.js';
import { resetSyntheticIdentity } from '../support/runtime.js';
import { store } from '../support/runtime.js';
import type { OnlineStoreContentRecord } from '../../src/state/types.js';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

function baseBlog(): OnlineStoreContentRecord {
  return {
    id: 'gid://shopify/Blog/100',
    kind: 'blog',
    createdAt: '2026-04-25T10:00:00Z',
    updatedAt: '2026-04-25T10:00:00Z',
    data: {
      __typename: 'Blog',
      id: 'gid://shopify/Blog/100',
      title: 'News',
      handle: 'news',
      commentPolicy: 'MODERATED',
      tags: [],
      templateSuffix: null,
      createdAt: '2026-04-25T10:00:00Z',
      updatedAt: '2026-04-25T10:00:00Z',
    },
  };
}

function baseArticle(blogId = 'gid://shopify/Blog/100'): OnlineStoreContentRecord {
  return {
    id: 'gid://shopify/Article/200',
    kind: 'article',
    parentId: blogId,
    createdAt: '2026-04-25T10:05:00Z',
    updatedAt: '2026-04-25T10:05:00Z',
    data: {
      __typename: 'Article',
      id: 'gid://shopify/Article/200',
      blogId,
      title: 'Launch notes',
      handle: 'launch-notes',
      body: '<p>Initial content</p>',
      summary: '<p>Initial</p>',
      tags: ['release', 'news'],
      author: { name: 'Ada Lovelace' },
      isPublished: true,
      publishedAt: '2026-04-25T10:05:00Z',
      createdAt: '2026-04-25T10:05:00Z',
      updatedAt: '2026-04-25T10:05:00Z',
      templateSuffix: null,
    },
  };
}

function baseComment(articleId = 'gid://shopify/Article/200'): OnlineStoreContentRecord {
  return {
    id: 'gid://shopify/Comment/300',
    kind: 'comment',
    parentId: articleId,
    createdAt: '2026-04-25T10:10:00Z',
    updatedAt: '2026-04-25T10:10:00Z',
    data: {
      __typename: 'Comment',
      id: 'gid://shopify/Comment/300',
      body: 'Helpful post',
      bodyHtml: '<p>Helpful post</p>',
      status: 'PENDING',
      isPublished: false,
      publishedAt: null,
      createdAt: '2026-04-25T10:10:00Z',
      updatedAt: '2026-04-25T10:10:00Z',
      ip: null,
      userAgent: null,
      author: { name: 'Reader' },
    },
  };
}

describe('online-store content flow', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('returns Shopify-like empty/null snapshot reads without upstream access', async () => {
    const fetchSpy = vi
      .spyOn(globalThis, 'fetch')
      .mockRejectedValue(new Error('snapshot read must not fetch upstream'));
    const app = createApp(config).callback();

    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query EmptyOnlineStore($id: ID!) {
          article(id: $id) { id title }
          blog(id: $id) { id title }
          page(id: $id) { id title }
          comment(id: $id) { id body }
          articles(first: 2) { nodes { id } edges { cursor node { id } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } }
          articleAuthors(first: 2) { nodes { name } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } }
          articleTags(limit: 10)
          blogs(first: 2) { nodes { id } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } }
          blogsCount { count precision }
          pages(first: 2) { nodes { id } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } }
          pagesCount { count precision }
          comments(first: 2) { nodes { id } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } }
        }`,
        variables: { id: 'gid://shopify/Article/999' },
      });

    expect(response.status).toBe(200);
    expect(response.body.data).toMatchObject({
      article: null,
      blog: null,
      page: null,
      comment: null,
      articles: {
        nodes: [],
        edges: [],
        pageInfo: { hasNextPage: false, hasPreviousPage: false, startCursor: null, endCursor: null },
      },
      articleAuthors: {
        nodes: [],
        pageInfo: { hasNextPage: false, hasPreviousPage: false, startCursor: null, endCursor: null },
      },
      articleTags: [],
      blogs: {
        nodes: [],
        pageInfo: { hasNextPage: false, hasPreviousPage: false, startCursor: null, endCursor: null },
      },
      blogsCount: { count: 0, precision: 'EXACT' },
      pages: {
        nodes: [],
        pageInfo: { hasNextPage: false, hasPreviousPage: false, startCursor: null, endCursor: null },
      },
      pagesCount: { count: 0, precision: 'EXACT' },
      comments: {
        nodes: [],
        pageInfo: { hasNextPage: false, hasPreviousPage: false, startCursor: null, endCursor: null },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages blog, page, and article lifecycle mutations locally with downstream reads and log visibility', async () => {
    const fetchSpy = vi
      .spyOn(globalThis, 'fetch')
      .mockRejectedValue(new Error('content mutations must not fetch upstream'));
    const app = createApp(config).callback();

    const blogCreate = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation CreateBlog($blog: BlogCreateInput!) {
          blogCreate(blog: $blog) {
            blog { id title handle commentPolicy }
            userErrors { field message }
          }
        }`,
        variables: { blog: { title: 'Journal', commentPolicy: 'MODERATED' } },
      });
    expect(blogCreate.body.data.blogCreate.userErrors).toEqual([]);
    const blogId = blogCreate.body.data.blogCreate.blog.id;

    const pageCreate = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation CreatePage($page: PageCreateInput!) {
          pageCreate(page: $page) {
            page { id title handle body bodySummary isPublished publishedAt }
            userErrors { field message }
          }
        }`,
        variables: { page: { title: 'Sizing guide', body: '<p>Measure twice</p>', isPublished: true } },
      });
    expect(pageCreate.body.data.pageCreate.userErrors).toEqual([]);
    const pageId = pageCreate.body.data.pageCreate.page.id;

    const articleCreate = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation CreateArticle($article: ArticleCreateInput!) {
          articleCreate(article: $article) {
            article { id title handle tags author { name } isPublished }
            userErrors { field message }
          }
        }`,
        variables: {
          article: {
            blogId,
            title: 'Spring launch',
            body: '<p>Fresh arrivals</p>',
            tags: ['launch', 'spring'],
            isPublished: true,
            author: { name: 'Grace Hopper' },
          },
        },
      });
    expect(articleCreate.body.data.articleCreate.userErrors).toEqual([]);
    const articleId = articleCreate.body.data.articleCreate.article.id;

    const readAfterCreate = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query ReadContent($blogId: ID!, $pageId: ID!, $articleId: ID!) {
          blog(id: $blogId) {
            id
            title
            articlesCount { count precision }
            articles(first: 5) { nodes { id title handle } pageInfo { hasNextPage hasPreviousPage } }
          }
          page(id: $pageId) { id title bodySummary isPublished }
          article(id: $articleId) { id title blog { id title } commentsCount { count precision } }
          articles(first: 5, query: "spring") { nodes { id title } }
          articleAuthors(first: 5) { nodes { name } }
          articleTags(limit: 5)
          blogsCount { count precision }
          pagesCount { count precision }
        }`,
        variables: { blogId, pageId, articleId },
      });

    expect(readAfterCreate.body.data).toMatchObject({
      blog: {
        id: blogId,
        title: 'Journal',
        articlesCount: { count: 1, precision: 'EXACT' },
        articles: {
          nodes: [{ id: articleId, title: 'Spring launch', handle: 'spring-launch' }],
          pageInfo: { hasNextPage: false, hasPreviousPage: false },
        },
      },
      page: {
        id: pageId,
        title: 'Sizing guide',
        bodySummary: 'Measure twice',
        isPublished: true,
      },
      article: {
        id: articleId,
        title: 'Spring launch',
        blog: { id: blogId, title: 'Journal' },
        commentsCount: { count: 0, precision: 'EXACT' },
      },
      articles: { nodes: [{ id: articleId, title: 'Spring launch' }] },
      articleAuthors: { nodes: [{ name: 'Grace Hopper' }] },
      articleTags: ['launch', 'spring'],
      blogsCount: { count: 1, precision: 'EXACT' },
      pagesCount: { count: 1, precision: 'EXACT' },
    });

    const articleUpdate = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation UpdateArticle($id: ID!, $article: ArticleUpdateInput!) {
          articleUpdate(id: $id, article: $article) {
            article { id title handle isPublished }
            userErrors { field message }
          }
        }`,
        variables: { id: articleId, article: { title: 'Spring launch updated', isPublished: false } },
      });
    expect(articleUpdate.body.data.articleUpdate.article).toMatchObject({
      id: articleId,
      title: 'Spring launch updated',
      isPublished: false,
    });

    const pageDelete = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation DeletePage($id: ID!) {
          pageDelete(id: $id) { deletedPageId userErrors { field message } }
        }`,
        variables: { id: pageId },
      });
    expect(pageDelete.body.data.pageDelete).toEqual({ deletedPageId: pageId, userErrors: [] });

    const readAfterDelete = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query ReadAfterDelete($id: ID!) {
          page(id: $id) { id }
          pagesCount { count precision }
        }`,
        variables: { id: pageId },
      });
    expect(readAfterDelete.body.data).toEqual({
      page: null,
      pagesCount: { count: 0, precision: 'EXACT' },
    });

    const logResponse = await request(app).get('/__meta/log');
    expect(logResponse.body.entries.map((entry: { operationName: string }) => entry.operationName)).toEqual([
      'blogCreate',
      'pageCreate',
      'articleCreate',
      'articleUpdate',
      'pageDelete',
    ]);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages comment moderation and deletion locally for snapshot comments', async () => {
    const fetchSpy = vi
      .spyOn(globalThis, 'fetch')
      .mockRejectedValue(new Error('comment moderation must not fetch upstream'));
    store.upsertBaseOnlineStoreContent([baseBlog(), baseArticle(), baseComment()]);
    const app = createApp(config).callback();

    const approve = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation ApproveComment($id: ID!) {
          commentApprove(id: $id) { comment { id status isPublished } userErrors { field message } }
        }`,
        variables: { id: 'gid://shopify/Comment/300' },
      });
    expect(approve.body.data.commentApprove).toEqual({
      comment: { id: 'gid://shopify/Comment/300', status: 'PUBLISHED', isPublished: true },
      userErrors: [],
    });

    const spam = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation SpamComment($id: ID!) {
          commentSpam(id: $id) { comment { id status isPublished } userErrors { field message } }
        }`,
        variables: { id: 'gid://shopify/Comment/300' },
      });
    expect(spam.body.data.commentSpam.comment).toMatchObject({
      id: 'gid://shopify/Comment/300',
      status: 'SPAM',
      isPublished: false,
    });

    const read = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query CommentRead($id: ID!, $articleId: ID!) {
          comment(id: $id) { id status article { id title } }
          comments(first: 5, query: "status:spam") { nodes { id status } }
          article(id: $articleId) {
            comments(first: 5) { nodes { id status } pageInfo { hasNextPage hasPreviousPage } }
            commentsCount { count precision }
          }
        }`,
        variables: {
          id: 'gid://shopify/Comment/300',
          articleId: 'gid://shopify/Article/200',
        },
      });
    expect(read.body.data).toMatchObject({
      comment: {
        id: 'gid://shopify/Comment/300',
        status: 'SPAM',
        article: { id: 'gid://shopify/Article/200', title: 'Launch notes' },
      },
      comments: { nodes: [{ id: 'gid://shopify/Comment/300', status: 'SPAM' }] },
      article: {
        comments: {
          nodes: [{ id: 'gid://shopify/Comment/300', status: 'SPAM' }],
          pageInfo: { hasNextPage: false, hasPreviousPage: false },
        },
        commentsCount: { count: 1, precision: 'EXACT' },
      },
    });

    const deleteComment = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation DeleteComment($id: ID!) {
          commentDelete(id: $id) { deletedCommentId userErrors { field message } }
        }`,
        variables: { id: 'gid://shopify/Comment/300' },
      });
    expect(deleteComment.body.data.commentDelete).toEqual({
      deletedCommentId: 'gid://shopify/Comment/300',
      userErrors: [],
    });

    const readDeleted = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query ReadDeletedComment($id: ID!) {
          comment(id: $id) { id }
          comments(first: 5) { nodes { id } }
        }`,
        variables: { id: 'gid://shopify/Comment/300' },
      });
    expect(readDeleted.body.data).toEqual({
      comment: null,
      comments: { nodes: [] },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});

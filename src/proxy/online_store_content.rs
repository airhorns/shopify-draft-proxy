use super::*;

mod connection;
mod content;
mod online_store_helpers;
mod sales_channel;
mod search;

pub(in crate::proxy) use self::online_store_helpers::*;

const ONLINE_STORE_TITLE_MAX_CHARS: usize = 255;
const ONLINE_STORE_HANDLE_MAX_CHARS: usize = 255;
const ONLINE_STORE_ARTICLE_HANDLE_MAX_CHARS: usize = 265;
const ONLINE_STORE_PAGE_BODY_MAX_BYTES: usize = 524_287;
const ONLINE_STORE_ARTICLE_BODY_MAX_BYTES: usize = 1_048_576;
const ONLINE_STORE_COMMENT_HYDRATE_QUERY: &str = "query OnlineStoreCommentHydrate($id: ID!) { comment(id: $id) { __typename id status body bodyHtml isPublished publishedAt createdAt updatedAt article { id } } }";
const ONLINE_STORE_COMMENT_ARTICLE_HYDRATE_QUERY: &str = "query OnlineStoreCommentArticleHydrate($id: ID!) { article(id: $id) { __typename id title handle body summary tags isPublished publishedAt createdAt updatedAt templateSuffix author { name } blog { __typename id title handle commentPolicy createdAt updatedAt } commentsCount { count precision } } }";
const ONLINE_STORE_PAGE_HYDRATE_QUERY: &str = "query OnlineStorePageHydrate($id: ID!) { page(id: $id) { __typename id title handle body bodySummary isPublished publishedAt createdAt updatedAt templateSuffix } }";
const ONLINE_STORE_ARTICLE_CASCADE_HYDRATE_QUERY: &str = "query OnlineStoreArticleDeleteCascadeHydrate($id: ID!) { article(id: $id) { __typename id title handle createdAt updatedAt blog { id } comments(first: 50) { nodes { __typename id status body bodyHtml isPublished publishedAt createdAt updatedAt article { id } } } } }";
const ONLINE_STORE_BLOG_CASCADE_HYDRATE_QUERY: &str = "query OnlineStoreBlogDeleteCascadeHydrate($id: ID!) { blog(id: $id) { __typename id title handle createdAt updatedAt commentPolicy articles(first: 50) { nodes { __typename id title handle createdAt updatedAt blog { id } comments(first: 50) { nodes { __typename id status body bodyHtml isPublished publishedAt createdAt updatedAt article { id } } } } } } }";
const BLOGS_COUNT_Q: &str = "query OnlineStoreBlogsCountHydrate { blogsCount { count precision } }";
const PAGES_COUNT_Q: &str = "query OnlineStorePagesCountHydrate { pagesCount { count precision } }";

#[derive(Clone, Copy, PartialEq, Eq)]
enum OnlineStoreKind {
    Blog,
    Page,
    Article,
    Comment,
}

impl OnlineStoreKind {
    fn resource_key(self) -> &'static str {
        match self {
            Self::Blog => "blog",
            Self::Page => "page",
            Self::Article => "article",
            Self::Comment => "comment",
        }
    }

    fn deleted_key(self) -> &'static str {
        match self {
            Self::Blog => "deletedBlogId",
            Self::Page => "deletedPageId",
            Self::Article => "deletedArticleId",
            Self::Comment => "deletedCommentId",
        }
    }

    fn hydrate_query(self) -> &'static str {
        match self {
            Self::Blog => ONLINE_STORE_BLOG_CASCADE_HYDRATE_QUERY,
            Self::Page => ONLINE_STORE_PAGE_HYDRATE_QUERY,
            Self::Article => ONLINE_STORE_ARTICLE_CASCADE_HYDRATE_QUERY,
            Self::Comment => ONLINE_STORE_COMMENT_HYDRATE_QUERY,
        }
    }

    fn not_found_error(self) -> Value {
        match self {
            Self::Blog => user_error(vec!["id"], "Blog does not exist", Some("NOT_FOUND")),
            Self::Page => user_error(vec!["id"], "Page does not exist", Some("NOT_FOUND")),
            Self::Article => user_error(vec!["id"], "Article does not exist", Some("NOT_FOUND")),
            Self::Comment => user_error(vec!["id"], "Comment does not exist", Some("NOT_FOUND")),
        }
    }

    fn records(self, staged: &StagedState) -> &BTreeMap<String, Value> {
        match self {
            Self::Blog => &staged.online_store_blogs,
            Self::Page => &staged.online_store_pages,
            Self::Article => &staged.online_store_articles,
            Self::Comment => &staged.online_store_comments,
        }
    }

    fn records_mut(self, staged: &mut StagedState) -> &mut BTreeMap<String, Value> {
        match self {
            Self::Blog => &mut staged.online_store_blogs,
            Self::Page => &mut staged.online_store_pages,
            Self::Article => &mut staged.online_store_articles,
            Self::Comment => &mut staged.online_store_comments,
        }
    }

    fn order(self, staged: &StagedState) -> &[String] {
        match self {
            Self::Blog => &staged.online_store_blog_order,
            Self::Page => &staged.online_store_page_order,
            Self::Article => &staged.online_store_article_order,
            Self::Comment => &staged.online_store_comment_order,
        }
    }

    fn order_mut(self, staged: &mut StagedState) -> &mut Vec<String> {
        match self {
            Self::Blog => &mut staged.online_store_blog_order,
            Self::Page => &mut staged.online_store_page_order,
            Self::Article => &mut staged.online_store_article_order,
            Self::Comment => &mut staged.online_store_comment_order,
        }
    }

    fn deleted_ids(self, staged: &StagedState) -> &BTreeSet<String> {
        match self {
            Self::Blog => &staged.deleted_online_store_blog_ids,
            Self::Page => &staged.deleted_online_store_page_ids,
            Self::Article => &staged.deleted_online_store_article_ids,
            Self::Comment => &staged.deleted_online_store_comment_ids,
        }
    }

    fn deleted_ids_mut(self, staged: &mut StagedState) -> &mut BTreeSet<String> {
        match self {
            Self::Blog => &mut staged.deleted_online_store_blog_ids,
            Self::Page => &mut staged.deleted_online_store_page_ids,
            Self::Article => &mut staged.deleted_online_store_article_ids,
            Self::Comment => &mut staged.deleted_online_store_comment_ids,
        }
    }

    fn count_base(self, staged: &StagedState) -> Option<usize> {
        match self {
            Self::Blog => staged.online_store_blogs_count_base,
            Self::Page => staged.online_store_pages_count_base,
            Self::Article | Self::Comment => None,
        }
    }

    fn count_base_mut(self, staged: &mut StagedState) -> Option<&mut Option<usize>> {
        match self {
            Self::Blog => Some(&mut staged.online_store_blogs_count_base),
            Self::Page => Some(&mut staged.online_store_pages_count_base),
            Self::Article | Self::Comment => None,
        }
    }
}

const ONLINE_STORE_COUNT_ROOTS: [(&str, OnlineStoreKind, &str); 2] = [
    ("blogsCount", OnlineStoreKind::Blog, BLOGS_COUNT_Q),
    ("pagesCount", OnlineStoreKind::Page, PAGES_COUNT_Q),
];

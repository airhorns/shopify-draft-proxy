use super::search::is_online_store_content_query_root;
use super::*;

pub(in crate::proxy) fn online_store_field_resolver_registrations() -> Vec<FieldResolverRegistration>
{
    [
        (
            "Article",
            "comments",
            online_store_article_comments_field as crate::resolver_registry::FieldResolverHandler,
        ),
        (
            "Article",
            "commentsCount",
            online_store_article_comments_count_field,
        ),
        ("Article", "metafield", online_store_content_metafield_field),
        (
            "Article",
            "metafields",
            online_store_content_metafields_field,
        ),
        ("Blog", "metafield", online_store_content_metafield_field),
        ("Blog", "metafields", online_store_content_metafields_field),
        ("Comment", "article", online_store_comment_article_field),
        ("Blog", "articles", online_store_blog_articles_field),
        (
            "Blog",
            "articlesCount",
            online_store_blog_articles_count_field,
        ),
        ("OnlineStoreTheme", "files", online_store_theme_files_field),
    ]
    .into_iter()
    .map(|(parent_type, field_name, handler)| {
        FieldResolverRegistration::explicit(ApiSurface::Admin, parent_type, field_name, handler)
    })
    .collect()
}

fn online_store_theme_files_field(
    _proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let arguments = resolved_arguments_from_json(&invocation.arguments);
    let filename_patterns = resolved_string_list_arg(&arguments, "filenames");
    let files = theme_file_nodes(invocation.parent)
        .into_iter()
        .filter(|file| {
            filename_patterns.is_empty()
                || file
                    .get("filename")
                    .and_then(Value::as_str)
                    .is_some_and(|filename| {
                        filename_patterns
                            .iter()
                            .any(|pattern| theme_filename_matches(pattern, filename))
                    })
        })
        .collect();
    Ok(connection_value_with_args(
        files,
        &arguments,
        theme_file_cursor,
    ))
}

fn online_store_parent_id<'a>(
    invocation: &'a crate::admin_graphql::FieldResolverInvocation<'_>,
    parent_type: &str,
) -> Result<&'a str, String> {
    invocation
        .parent
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{parent_type} parent has no canonical id"))
}

fn online_store_article_comments_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let article_id = online_store_parent_id(invocation, "Article")?;
    let comments = proxy
        .online_store_records(OnlineStoreKind::Comment)
        .into_iter()
        .filter(|comment| comment["articleId"].as_str() == Some(article_id))
        .collect();
    Ok(proxy.online_store_connection_from_records(
        OnlineStoreKind::Comment,
        comments,
        &resolved_arguments_from_json(&invocation.arguments),
    ))
}

fn online_store_article_comments_count_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let article_id = online_store_parent_id(invocation, "Article")?;
    Ok(count_object(
        proxy
            .online_store_records(OnlineStoreKind::Comment)
            .iter()
            .filter(|comment| comment["articleId"].as_str() == Some(article_id))
            .count(),
    ))
}

fn online_store_content_metafield_field(
    _proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let namespace = invocation
        .arguments
        .get("namespace")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let key = invocation
        .arguments
        .get("key")
        .and_then(Value::as_str)
        .unwrap_or_default();
    Ok(online_store_content_metafield(invocation.parent, namespace, key).unwrap_or(Value::Null))
}

fn online_store_content_metafields_field(
    _proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let namespace = invocation
        .arguments
        .get("namespace")
        .and_then(Value::as_str);
    Ok(connection_value_with_args(
        online_store_content_metafield_nodes(invocation.parent, namespace),
        &resolved_arguments_from_json(&invocation.arguments),
        value_id_cursor,
    ))
}

fn online_store_blog_articles_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let blog_id = online_store_parent_id(invocation, "Blog")?;
    let articles = proxy
        .online_store_records(OnlineStoreKind::Article)
        .into_iter()
        .filter(|article| article["blogId"].as_str() == Some(blog_id))
        .collect();
    Ok(proxy.online_store_connection_from_records(
        OnlineStoreKind::Article,
        articles,
        &resolved_arguments_from_json(&invocation.arguments),
    ))
}

fn online_store_blog_articles_count_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let blog_id = online_store_parent_id(invocation, "Blog")?;
    let known_articles = proxy
        .online_store_records(OnlineStoreKind::Article)
        .into_iter()
        .filter(|article| article["blogId"].as_str() == Some(blog_id))
        .collect::<Vec<_>>();
    Ok(count_object(proxy.online_store_blog_articles_count(
        invocation.parent,
        &known_articles,
    )))
}

fn online_store_comment_article_field(
    proxy: &mut DraftProxy,
    request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let article_id = invocation
        .parent
        .get("articleId")
        .and_then(Value::as_str)
        .or_else(|| {
            invocation
                .parent
                .pointer("/article/id")
                .and_then(Value::as_str)
        })
        .unwrap_or_default();
    if article_id.is_empty() {
        return Ok(Value::Null);
    }
    if proxy
        .online_store_record(OnlineStoreKind::Article, article_id)
        .is_none()
        && proxy.config.read_mode != ReadMode::Snapshot
    {
        proxy.hydrate_online_store_content_from_upstream(
            request,
            article_id,
            ONLINE_STORE_COMMENT_ARTICLE_HYDRATE_QUERY,
        );
    }
    Ok(proxy
        .online_store_record(OnlineStoreKind::Article, article_id)
        .map(|article| proxy.article_parent_record(&article))
        .unwrap_or(Value::Null))
}

impl DraftProxy {
    pub(in crate::proxy) fn online_store_content_query_value(
        &self,
        field: &OnlineStoreRootCall,
    ) -> Option<Value> {
        match field.name.as_str() {
            "blog" => {
                let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                Some(
                    self.online_store_value(OnlineStoreKind::Blog, &id)
                        .unwrap_or(Value::Null),
                )
            }
            "blogs" => Some(self.online_store_connection_value(OnlineStoreKind::Blog, field)),
            "blogsCount" => Some(count_object(self.online_store_count(OnlineStoreKind::Blog))),
            "page" => {
                let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                Some(
                    self.online_store_value(OnlineStoreKind::Page, &id)
                        .unwrap_or(Value::Null),
                )
            }
            "pages" => Some(self.online_store_connection_value(OnlineStoreKind::Page, field)),
            "pagesCount" => Some(count_object(self.online_store_count(OnlineStoreKind::Page))),
            "article" => {
                let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                Some(
                    self.online_store_value(OnlineStoreKind::Article, &id)
                        .unwrap_or(Value::Null),
                )
            }
            "articles" => Some(self.online_store_connection_value(OnlineStoreKind::Article, field)),
            "articleAuthors" => {
                let mut names = BTreeSet::new();
                for article in self.online_store_records(OnlineStoreKind::Article) {
                    if let Some(name) = article["author"]["name"].as_str() {
                        if !name.is_empty() {
                            names.insert(name.to_string());
                        }
                    }
                }
                let records = names
                    .into_iter()
                    .map(|name| json!({ "name": name }))
                    .collect::<Vec<_>>();
                Some(connection_value_with_args(
                    records,
                    &field.arguments,
                    |_| String::new(),
                ))
            }
            "articleTags" => {
                let limit = resolved_int_field(&field.arguments, "limit")
                    .and_then(|limit| (limit >= 0).then_some(limit as usize));
                let mut tags = BTreeSet::new();
                for article in self.online_store_records(OnlineStoreKind::Article) {
                    if let Some(values) = article["tags"].as_array() {
                        for tag in values {
                            if let Some(tag) = tag.as_str() {
                                tags.insert(tag.to_string());
                            }
                        }
                    }
                }
                let mut tags = tags.into_iter().map(Value::String).collect::<Vec<_>>();
                if let Some(limit) = limit {
                    tags.truncate(limit);
                }
                Some(Value::Array(tags))
            }
            "comment" => {
                let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                Some(
                    self.online_store_value(OnlineStoreKind::Comment, &id)
                        .unwrap_or(Value::Null),
                )
            }
            "comments" => Some(self.online_store_connection_value(OnlineStoreKind::Comment, field)),
            _ => None,
        }
    }

    pub(in crate::proxy) fn online_store_content_mutation_value(
        &mut self,
        field: &OnlineStoreRootCall,
        request: &Request,
        staged_ids: &mut Vec<String>,
    ) -> Option<Value> {
        match field.name.as_str() {
            "blogCreate" => Some(self.online_store_blog_create(field, staged_ids)),
            "blogUpdate" => Some(self.online_store_blog_update(field, request, staged_ids)),
            "blogDelete" => Some(self.online_store_blog_delete(field, request, staged_ids)),
            "pageCreate" => Some(self.online_store_page_create(field, staged_ids)),
            "pageUpdate" => Some(self.online_store_page_update(field, request, staged_ids)),
            "pageDelete" => Some(self.online_store_page_delete(field, request, staged_ids)),
            "articleCreate" => Some(self.online_store_article_create(field, request, staged_ids)),
            "articleUpdate" => Some(self.online_store_article_update(field, request, staged_ids)),
            "articleDelete" => Some(self.online_store_article_delete(field, request, staged_ids)),
            "commentApprove" => Some(self.online_store_comment_moderate(
                field,
                request,
                "commentApprove",
                staged_ids,
            )),
            "commentSpam" => {
                Some(self.online_store_comment_moderate(field, request, "commentSpam", staged_ids))
            }
            "commentNotSpam" => Some(self.online_store_comment_moderate(
                field,
                request,
                "commentNotSpam",
                staged_ids,
            )),
            "commentDelete" => Some(self.online_store_comment_delete(field, request, staged_ids)),
            _ => None,
        }
    }

    pub(in crate::proxy) fn online_store_content_node_value(&self, id: &str) -> Option<Value> {
        let kind = match shopify_gid_resource_type(id) {
            Some("Blog") => OnlineStoreKind::Blog,
            Some("Page") => OnlineStoreKind::Page,
            Some("Article") => OnlineStoreKind::Article,
            Some("Comment") => OnlineStoreKind::Comment,
            _ => return None,
        };
        if kind.deleted_ids(&self.store.staged).contains(id) {
            return Some(Value::Null);
        }
        self.online_store_record(kind, id)
            .map(|record| self.enriched_online_store_record(kind, &record))
    }

    pub(in crate::proxy) fn online_store_content_query_needs_upstream(
        &self,
        field: &OnlineStoreRootCall,
    ) -> bool {
        if self.config.read_mode == ReadMode::Snapshot {
            return false;
        }
        is_online_store_content_query_root(&field.name) && !self.has_online_store_content_state()
    }

    pub(in crate::proxy) fn observe_online_store_content_response(&mut self, body: &Value) {
        let Some(data) = body.get("data") else {
            return;
        };
        for (root, kind, _) in ONLINE_STORE_COUNT_ROOTS {
            if let Some(count) = data
                .get(root)
                .and_then(|value| value.get("count"))
                .and_then(Value::as_u64)
            {
                if let Some(count_base) = kind.count_base_mut(&mut self.store.staged) {
                    *count_base = Some(count as usize);
                }
            }
        }
        self.observe_online_store_content_node(data, None, None);
    }

    pub(in crate::proxy) fn has_online_store_content_state(&self) -> bool {
        !self.store.staged.online_store_blogs.is_empty()
            || !self.store.staged.online_store_pages.is_empty()
            || !self.store.staged.online_store_articles.is_empty()
            || !self.store.staged.online_store_comments.is_empty()
            || !self.store.staged.deleted_online_store_blog_ids.is_empty()
            || !self.store.staged.deleted_online_store_page_ids.is_empty()
            || !self
                .store
                .staged
                .deleted_online_store_article_ids
                .is_empty()
            || !self
                .store
                .staged
                .deleted_online_store_comment_ids
                .is_empty()
            || ONLINE_STORE_COUNT_ROOTS
                .iter()
                .any(|(_, kind, _)| kind.count_base(&self.store.staged).is_some())
    }

    fn hydrate_online_store_content_from_upstream(
        &mut self,
        request: &Request,
        id: &str,
        query: &str,
    ) {
        if self.config.read_mode == ReadMode::Snapshot || id.is_empty() {
            return;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": query,
                "variables": { "id": id }
            }),
        );
        if response.status < 400 {
            self.observe_online_store_content_response(&response.body);
        }
    }

    fn load_online_store_record_for_update(
        &mut self,
        request: &Request,
        kind: OnlineStoreKind,
        id: &str,
    ) -> Option<Value> {
        if kind.deleted_ids(&self.store.staged).contains(id) {
            return None;
        }
        let current = self.online_store_record(kind, id);
        if kind.mutation_hydrate_query().is_some()
            && current
                .as_ref()
                .is_none_or(|record| !kind.complete_for_mutation(record))
        {
            if !self.hydrate_online_store_mutation_record(request, kind, id) {
                return None;
            }
        } else if current.is_none() {
            self.hydrate_online_store_content_from_upstream(request, id, kind.hydrate_query());
        }
        self.online_store_record(kind, id)
    }

    fn hydrate_online_store_mutation_record(
        &mut self,
        request: &Request,
        kind: OnlineStoreKind,
        id: &str,
    ) -> bool {
        let Some((query, operation_name)) = kind.mutation_hydrate_query() else {
            return false;
        };
        if self.config.read_mode == ReadMode::Snapshot || id.is_empty() {
            return false;
        }

        let mut record = self
            .online_store_record(kind, id)
            .unwrap_or_else(|| json!({}));
        if let Some(object) = record.as_object_mut() {
            object.remove("metafields");
        }
        let mut metafields = Vec::new();
        let mut after = Value::Null;
        for _ in 0..20 {
            let response = self.upstream_post(
                request,
                json!({
                    "query": query,
                    "operationName": operation_name,
                    "variables": { "id": id, "after": after.clone() }
                }),
            );
            if !(200..300).contains(&response.status) {
                return false;
            }
            let Some(observed) = response
                .body
                .get("data")
                .and_then(|data| data.get(kind.resource_key()))
                .filter(|observed| observed.is_object())
                .cloned()
            else {
                return false;
            };
            if observed.get("id").and_then(Value::as_str) != Some(id) {
                return false;
            }

            if kind == OnlineStoreKind::Article {
                if let Some(blog) = observed.get("blog") {
                    self.observe_online_store_content_node(blog, None, None);
                }
            }

            let Some(connection) = observed.get("metafields").cloned() else {
                return false;
            };
            metafields.extend(connection_nodes(&connection));
            let mut observed_without_metafields = observed;
            if let Some(object) = observed_without_metafields.as_object_mut() {
                object.remove("metafields");
            }
            record = merge_online_store_observed_values(&record, &observed_without_metafields);

            if connection
                .pointer("/pageInfo/hasNextPage")
                .and_then(Value::as_bool)
                != Some(true)
            {
                record["metafields"] = connection_json(metafields);
                let normalized = match kind {
                    OnlineStoreKind::Blog => normalize_observed_blog(&record),
                    OnlineStoreKind::Article => normalize_observed_article(&record, None),
                    OnlineStoreKind::Page | OnlineStoreKind::Comment => return false,
                };
                self.stage_online_store_record(kind, id.to_string(), normalized);
                return true;
            }
            let Some(end_cursor) = connection
                .pointer("/pageInfo/endCursor")
                .and_then(Value::as_str)
            else {
                return false;
            };
            after = json!(end_cursor);
        }
        false
    }

    fn guard_online_store_delete(
        &mut self,
        request: &Request,
        kind: OnlineStoreKind,
        id: &str,
    ) -> bool {
        if !kind.records(&self.store.staged).contains_key(id) {
            self.hydrate_online_store_content_from_upstream(request, id, kind.hydrate_query());
        }
        self.online_store_record(kind, id).is_some()
    }

    fn observe_online_store_content_node(
        &mut self,
        node: &Value,
        parent_blog_id: Option<String>,
        parent_article_id: Option<String>,
    ) {
        match node {
            Value::Array(entries) => {
                for entry in entries {
                    self.observe_online_store_content_node(
                        entry,
                        parent_blog_id.clone(),
                        parent_article_id.clone(),
                    );
                }
            }
            Value::Object(object) => {
                let mut next_parent_blog_id = parent_blog_id.clone();
                let mut next_parent_article_id = parent_article_id.clone();
                if let Some(id) = object.get("id").and_then(Value::as_str) {
                    match shopify_gid_resource_type(id) {
                        Some("Blog") if should_stage_observed_blog(node) => {
                            let node = self.merge_online_store_observation(
                                OnlineStoreKind::Blog,
                                id,
                                node,
                            );
                            self.stage_online_store_record(
                                OnlineStoreKind::Blog,
                                id.to_string(),
                                normalize_observed_blog(&node),
                            );
                            next_parent_blog_id = Some(id.to_string());
                        }
                        Some("Page") if should_stage_observed_page(node) => {
                            let node = self.merge_online_store_observation(
                                OnlineStoreKind::Page,
                                id,
                                node,
                            );
                            self.stage_online_store_record(
                                OnlineStoreKind::Page,
                                id.to_string(),
                                normalize_observed_page(&node),
                            );
                        }
                        Some("Article") if should_stage_observed_article(node) => {
                            let node = self.merge_online_store_observation(
                                OnlineStoreKind::Article,
                                id,
                                node,
                            );
                            self.stage_online_store_record(
                                OnlineStoreKind::Article,
                                id.to_string(),
                                normalize_observed_article(&node, parent_blog_id.as_deref()),
                            );
                            next_parent_article_id = Some(id.to_string());
                        }
                        Some("Comment") if should_stage_observed_comment(node) => {
                            let node = self.merge_online_store_observation(
                                OnlineStoreKind::Comment,
                                id,
                                node,
                            );
                            self.stage_online_store_record(
                                OnlineStoreKind::Comment,
                                id.to_string(),
                                normalize_observed_comment(&node, parent_article_id.as_deref()),
                            );
                        }
                        _ => {}
                    }
                }
                for value in object.values() {
                    self.observe_online_store_content_node(
                        value,
                        next_parent_blog_id.clone(),
                        next_parent_article_id.clone(),
                    );
                }
            }
            _ => {}
        }
    }

    /// A single GraphQL document can observe the same resource through several
    /// roots with different selections. Preserve fields learned from an earlier,
    /// richer occurrence when a later occurrence only contains an ID or another
    /// partial slice of the object.
    fn merge_online_store_observation(
        &self,
        kind: OnlineStoreKind,
        id: &str,
        observed: &Value,
    ) -> Value {
        kind.records(&self.store.staged)
            .get(id)
            .map(|existing| merge_online_store_observed_values(existing, observed))
            .unwrap_or_else(|| observed.clone())
    }

    fn online_store_blog_create(
        &mut self,
        field: &OnlineStoreRootCall,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let Some(input) = resolved_object_field(&field.arguments, "blog") else {
            return resource_payload(
                "blog",
                Value::Null,
                vec![user_error(
                    vec!["blog", "title"],
                    "Title can't be blank",
                    None,
                )],
            );
        };
        if let Some(error) = title_blank_error(&input, "blog", None, true) {
            return resource_payload("blog", Value::Null, vec![error]);
        }
        if let Some(error) =
            content_length_error(&input, "blog", ONLINE_STORE_HANDLE_MAX_CHARS, None)
        {
            return resource_payload("blog", Value::Null, vec![error]);
        }
        let id = self.next_online_store_id("Blog");
        let timestamp = online_store_operation_timestamp();
        let record = blog_record(&id, &input, &timestamp);
        self.stage_online_store_record(OnlineStoreKind::Blog, id.clone(), record.clone());
        staged_ids.push(id);
        resource_payload("blog", self.enriched_blog_record(&record), Vec::new())
    }

    fn online_store_blog_update(
        &mut self,
        field: &OnlineStoreRootCall,
        request: &Request,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let kind = OnlineStoreKind::Blog;
        let Some(mut record) = self.load_online_store_record_for_update(request, kind, &id) else {
            return resource_payload(
                kind.resource_key(),
                Value::Null,
                vec![kind.not_found_error()],
            );
        };
        let input = resolved_object_field(&field.arguments, "blog").unwrap_or_default();
        if let Some(error) = title_blank_error(&input, "blog", None, false) {
            return resource_payload("blog", Value::Null, vec![error]);
        }
        if let Some(error) =
            content_length_error(&input, "blog", ONLINE_STORE_HANDLE_MAX_CHARS, None)
        {
            return resource_payload("blog", Value::Null, vec![error]);
        }
        let timestamp = online_store_operation_timestamp();
        apply_blog_input(&mut record, &input, &timestamp);
        self.stage_online_store_record(kind, id.clone(), record.clone());
        staged_ids.push(id);
        resource_payload("blog", self.enriched_blog_record(&record), Vec::new())
    }

    fn online_store_blog_delete(
        &mut self,
        field: &OnlineStoreRootCall,
        request: &Request,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let kind = OnlineStoreKind::Blog;
        if !self.guard_online_store_delete(request, kind, &id) {
            return resource_payload(
                kind.deleted_key(),
                Value::Null,
                vec![kind.not_found_error()],
            );
        }
        kind.deleted_ids_mut(&mut self.store.staged)
            .insert(id.clone());
        let article_ids = self
            .store
            .staged
            .online_store_articles
            .values()
            .filter(|article| article["blogId"].as_str() == Some(id.as_str()))
            .filter_map(|article| article["id"].as_str().map(str::to_string))
            .collect::<Vec<_>>();
        for article_id in article_ids {
            self.tombstone_online_store_article(&article_id);
        }
        staged_ids.push(id.clone());
        resource_payload(kind.deleted_key(), json!(id), Vec::new())
    }

    fn online_store_page_create(
        &mut self,
        field: &OnlineStoreRootCall,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let input = resolved_object_field(&field.arguments, "page").unwrap_or_default();
        if let Some(error) = title_blank_error(&input, "page", Some("BLANK"), true) {
            return resource_payload("page", Value::Null, vec![error]);
        }
        if let Some(error) = content_length_error(
            &input,
            "page",
            ONLINE_STORE_HANDLE_MAX_CHARS,
            Some((
                ONLINE_STORE_PAGE_BODY_MAX_BYTES,
                "Content is too big (maximum is 512 KB)",
                Some("TOO_BIG"),
            )),
        ) {
            return resource_payload("page", Value::Null, vec![error]);
        }
        if let Some(handle) = resolved_string_field(&input, "handle") {
            if self.online_store_page_handle_taken(&handle, None) {
                return resource_payload(
                    "page",
                    Value::Null,
                    vec![user_error(
                        vec!["page", "handle"],
                        "Handle has already been taken",
                        Some("TAKEN"),
                    )],
                );
            }
        }
        if let Some(error) = invalid_publish_date_error(&input, "page", None, true) {
            return resource_payload("page", Value::Null, vec![error]);
        }
        let id = self.next_online_store_id("Page");
        let timestamp = online_store_operation_timestamp();
        let mut record = page_record(&id, &input, None, &timestamp);
        if !input.contains_key("handle") {
            let handle = record["handle"].as_str().unwrap_or_default();
            record["handle"] = json!(self.unique_online_store_page_handle(handle, None));
        }
        self.stage_online_store_record(OnlineStoreKind::Page, id.clone(), record.clone());
        staged_ids.push(id);
        resource_payload("page", record, Vec::new())
    }

    fn online_store_page_update(
        &mut self,
        field: &OnlineStoreRootCall,
        request: &Request,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let kind = OnlineStoreKind::Page;
        let Some(mut record) = self.load_online_store_record_for_update(request, kind, &id) else {
            return resource_payload(
                kind.resource_key(),
                Value::Null,
                vec![kind.not_found_error()],
            );
        };
        let input = resolved_object_field(&field.arguments, "page").unwrap_or_default();
        if let Some(error) = title_blank_error(&input, "page", Some("BLANK"), false) {
            return resource_payload("page", Value::Null, vec![error]);
        }
        if let Some(error) = content_length_error(
            &input,
            "page",
            ONLINE_STORE_HANDLE_MAX_CHARS,
            Some((
                ONLINE_STORE_PAGE_BODY_MAX_BYTES,
                "Content is too big (maximum is 512 KB)",
                Some("TOO_BIG"),
            )),
        ) {
            return resource_payload("page", Value::Null, vec![error]);
        }
        if let Some(handle) = resolved_string_field(&input, "handle") {
            if self.online_store_page_handle_taken(&handle, Some(id.as_str())) {
                return resource_payload(
                    "page",
                    Value::Null,
                    vec![user_error(
                        vec!["page", "handle"],
                        "Handle has already been taken",
                        Some("TAKEN"),
                    )],
                );
            }
        }
        if let Some(error) = invalid_publish_date_error(&input, "page", Some(&record), false) {
            return resource_payload("page", Value::Null, vec![error]);
        }
        let timestamp = online_store_operation_timestamp();
        apply_page_input(&mut record, &input, &timestamp);
        if input.contains_key("title") && !input.contains_key("handle") {
            let handle = record["handle"].as_str().unwrap_or_default();
            record["handle"] =
                json!(self.unique_online_store_page_handle(handle, Some(id.as_str())));
        }
        self.stage_online_store_record(kind, id.clone(), record.clone());
        staged_ids.push(id);
        resource_payload("page", record, Vec::new())
    }

    fn online_store_page_delete(
        &mut self,
        field: &OnlineStoreRootCall,
        request: &Request,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let kind = OnlineStoreKind::Page;
        if !self.guard_online_store_delete(request, kind, &id) {
            return resource_payload(
                kind.deleted_key(),
                Value::Null,
                vec![kind.not_found_error()],
            );
        }
        kind.deleted_ids_mut(&mut self.store.staged)
            .insert(id.clone());
        staged_ids.push(id.clone());
        resource_payload(kind.deleted_key(), json!(id), Vec::new())
    }

    fn online_store_article_create(
        &mut self,
        field: &OnlineStoreRootCall,
        request: &Request,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let input = resolved_object_field(&field.arguments, "article").unwrap_or_default();
        if let Some(error) = title_blank_error(&input, "article", Some("BLANK"), true) {
            return resource_payload("article", Value::Null, vec![error]);
        }
        if let Some(error) = content_length_error(
            &input,
            "article",
            ONLINE_STORE_ARTICLE_HANDLE_MAX_CHARS,
            Some((
                ONLINE_STORE_ARTICLE_BODY_MAX_BYTES,
                "Content is too big (maximum is 1 MB)",
                None,
            )),
        ) {
            return resource_payload("article", Value::Null, vec![error]);
        }
        if let Some(error) = invalid_publish_date_error(&input, "article", None, true) {
            return resource_payload("article", Value::Null, vec![error]);
        }
        let inline_blog = resolved_object_field(&field.arguments, "blog");
        let blog_id = resolved_string_field(&input, "blogId");
        let timestamp = online_store_operation_timestamp();
        if blog_id.is_some() && inline_blog.is_some() {
            return resource_payload(
                "article",
                Value::Null,
                vec![user_error(
                    vec!["article"],
                    "Can't create a blog from input if a blog ID is supplied.",
                    Some("AMBIGUOUS_BLOG"),
                )],
            );
        }
        let blog_id = if let Some(blog_id) = blog_id {
            if self
                .load_online_store_record_for_update(request, OnlineStoreKind::Blog, &blog_id)
                .is_none()
            {
                return article_blog_not_found_payload("article");
            }
            blog_id
        } else if let Some(blog) = inline_blog {
            if let Some(error) = title_blank_error(&blog, "blog", None, true) {
                return resource_payload("article", Value::Null, vec![error]);
            }
            let id = self.next_online_store_id("Blog");
            let record = blog_record(&id, &blog, &timestamp);
            self.stage_online_store_record(OnlineStoreKind::Blog, id.clone(), record);
            staged_ids.push(id.clone());
            id
        } else {
            return resource_payload(
                "article",
                Value::Null,
                vec![user_error(
                    vec!["article"],
                    "Must reference or create a blog when creating an article.",
                    Some("BLOG_REFERENCE_REQUIRED"),
                )],
            );
        };
        if let Some(error) = article_author_error(&input, true, true) {
            return resource_payload("article", Value::Null, vec![error]);
        }
        let id = self.next_online_store_id("Article");
        let record = article_record(&id, &blog_id, &input, None, &timestamp);
        self.stage_online_store_record(OnlineStoreKind::Article, id.clone(), record.clone());
        self.touch_online_store_blog(&blog_id, &timestamp);
        staged_ids.push(id);
        self.online_store_article_payload(self.enriched_article_record(&record), Vec::new())
    }

    fn online_store_article_update(
        &mut self,
        field: &OnlineStoreRootCall,
        request: &Request,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let kind = OnlineStoreKind::Article;
        let Some(mut record) = self.load_online_store_record_for_update(request, kind, &id) else {
            return resource_payload(
                kind.resource_key(),
                Value::Null,
                vec![kind.not_found_error()],
            );
        };
        let input = resolved_object_field(&field.arguments, "article").unwrap_or_default();
        if let Some(error) = title_blank_error(&input, "article", Some("BLANK"), false) {
            return resource_payload("article", Value::Null, vec![error]);
        }
        if let Some(error) = content_length_error(
            &input,
            "article",
            ONLINE_STORE_ARTICLE_HANDLE_MAX_CHARS,
            Some((
                ONLINE_STORE_ARTICLE_BODY_MAX_BYTES,
                "Content is too big (maximum is 1 MB)",
                None,
            )),
        ) {
            return resource_payload("article", Value::Null, vec![error]);
        }
        if let Some(error) = invalid_publish_date_error(&input, "article", Some(&record), false) {
            return resource_payload("article", Value::Null, vec![error]);
        }
        if let Some(blog_id) = resolved_string_field(&input, "blogId") {
            if self
                .load_online_store_record_for_update(request, OnlineStoreKind::Blog, &blog_id)
                .is_none()
            {
                return article_blog_not_found_payload("article");
            }
            record["blogId"] = json!(blog_id);
        }
        if let Some(error) = article_author_error(&input, false, false) {
            return resource_payload("article", Value::Null, vec![error]);
        }
        if let Some(error) = article_image_update_error(&record, &input) {
            return resource_payload("article", Value::Null, vec![error]);
        }
        let timestamp = online_store_operation_timestamp();
        apply_article_input(&mut record, &input, &timestamp);
        self.stage_online_store_record(kind, id.clone(), record.clone());
        if let Some(blog_id) = record["blogId"].as_str() {
            let blog_id = blog_id.to_string();
            self.touch_online_store_blog(&blog_id, &timestamp);
        }
        staged_ids.push(id);
        self.online_store_article_payload(self.enriched_article_record(&record), Vec::new())
    }

    fn online_store_article_delete(
        &mut self,
        field: &OnlineStoreRootCall,
        request: &Request,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let kind = OnlineStoreKind::Article;
        if !self.guard_online_store_delete(request, kind, &id) {
            return resource_payload(
                kind.deleted_key(),
                Value::Null,
                vec![kind.not_found_error()],
            );
        }
        self.tombstone_online_store_article(&id);
        staged_ids.push(id.clone());
        resource_payload(kind.deleted_key(), json!(id), Vec::new())
    }

    fn online_store_comment_moderate(
        &mut self,
        field: &OnlineStoreRootCall,
        request: &Request,
        root: &str,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let kind = OnlineStoreKind::Comment;
        if self
            .store
            .staged
            .deleted_online_store_comment_ids
            .contains(&id)
        {
            return resource_payload(
                kind.resource_key(),
                Value::Null,
                vec![kind.not_found_error()],
            );
        }
        let Some(mut comment) = self.load_online_store_record_for_update(request, kind, &id) else {
            return resource_payload(
                kind.resource_key(),
                Value::Null,
                vec![kind.not_found_error()],
            );
        };
        let status = comment["status"]
            .as_str()
            .unwrap_or("UNAPPROVED")
            .to_string();
        let next_status = match comment_moderation_transition(root, &status) {
            Ok(next_status) => next_status,
            Err(message) => {
                return resource_payload(
                    "comment",
                    Value::Null,
                    vec![user_error(vec!["id"], message, None)],
                )
            }
        };
        let changed = status != next_status;
        let timestamp = online_store_operation_timestamp();
        comment["status"] = json!(next_status.clone());
        comment["isPublished"] = json!(next_status == "PUBLISHED");
        if next_status == "PUBLISHED" && comment["publishedAt"].is_null() {
            comment["publishedAt"] = json!(timestamp.clone());
        } else if next_status != "PUBLISHED" {
            comment["publishedAt"] = Value::Null;
        }
        if changed {
            comment["updatedAt"] = json!(timestamp);
            self.stage_online_store_record(kind, id.clone(), comment.clone());
            staged_ids.push(id);
        }
        resource_payload(
            "comment",
            self.enriched_comment_record(&comment),
            Vec::new(),
        )
    }

    fn online_store_comment_delete(
        &mut self,
        field: &OnlineStoreRootCall,
        request: &Request,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let kind = OnlineStoreKind::Comment;
        if self
            .store
            .staged
            .deleted_online_store_comment_ids
            .contains(&id)
        {
            return resource_payload(
                kind.deleted_key(),
                Value::Null,
                vec![kind.not_found_error()],
            );
        }
        if !self.guard_online_store_delete(request, kind, &id) {
            return resource_payload(
                kind.deleted_key(),
                Value::Null,
                vec![kind.not_found_error()],
            );
        }
        if let Some(article_id) = self
            .store
            .staged
            .online_store_comments
            .get(&id)
            .and_then(|comment| string_value(comment, "articleId"))
        {
            if !article_id.is_empty()
                && !self
                    .store
                    .staged
                    .online_store_articles
                    .contains_key(&article_id)
                && !self
                    .store
                    .staged
                    .deleted_online_store_article_ids
                    .contains(&article_id)
            {
                self.hydrate_online_store_content_from_upstream(
                    request,
                    &article_id,
                    ONLINE_STORE_ARTICLE_CASCADE_HYDRATE_QUERY,
                );
            }
        }
        self.store.staged.online_store_comments.remove(&id);
        self.store
            .staged
            .online_store_comment_order
            .retain(|comment_id| comment_id != &id);
        self.store
            .staged
            .deleted_online_store_comment_ids
            .insert(id.clone());
        staged_ids.push(id.clone());
        resource_payload(kind.deleted_key(), json!(id), Vec::new())
    }

    fn online_store_value(&self, kind: OnlineStoreKind, id: &str) -> Option<Value> {
        self.online_store_record(kind, id)
            .map(|record| self.enriched_online_store_record(kind, &record))
    }

    fn online_store_record(&self, kind: OnlineStoreKind, id: &str) -> Option<Value> {
        (!kind.deleted_ids(&self.store.staged).contains(id))
            .then(|| kind.records(&self.store.staged).get(id).cloned())
            .flatten()
    }

    pub(super) fn online_store_records(&self, kind: OnlineStoreKind) -> Vec<Value> {
        self.online_store_raw_records(kind)
            .into_iter()
            .map(|record| self.enriched_online_store_record(kind, &record))
            .collect()
    }

    fn online_store_raw_records(&self, kind: OnlineStoreKind) -> Vec<Value> {
        kind.order(&self.store.staged)
            .iter()
            .filter_map(|id| self.online_store_record(kind, id))
            .collect()
    }

    fn online_store_article_payload(&self, article: Value, user_errors: Vec<Value>) -> Value {
        json!({
            "article": article,
            "userErrors": user_errors
        })
    }

    pub(super) fn enriched_online_store_record(
        &self,
        kind: OnlineStoreKind,
        record: &Value,
    ) -> Value {
        match kind {
            OnlineStoreKind::Blog => self.enriched_blog_record(record),
            OnlineStoreKind::Page => record.clone(),
            OnlineStoreKind::Article => self.enriched_article_record(record),
            OnlineStoreKind::Comment => self.enriched_comment_record(record),
        }
    }

    fn enriched_blog_record(&self, record: &Value) -> Value {
        let mut record = self.blog_parent_record(record);
        let id = record["id"].as_str().unwrap_or_default();
        let articles = self
            .online_store_raw_records(OnlineStoreKind::Article)
            .into_iter()
            .filter(|article| article["blogId"].as_str() == Some(id))
            .map(|article| self.enriched_article_record(&article))
            .collect::<Vec<_>>();
        record["articlesCount"] =
            count_object(self.online_store_blog_articles_count(&record, &articles));
        record["articles"] = connection_json(articles);
        record
    }

    fn blog_parent_record(&self, record: &Value) -> Value {
        let mut record = record.clone();
        let id = record["id"].as_str().unwrap_or_default();
        let articles = self
            .online_store_raw_records(OnlineStoreKind::Article)
            .into_iter()
            .filter(|article| article["blogId"].as_str() == Some(id))
            .collect::<Vec<_>>();
        let mut tags = record
            .get("tags")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect::<BTreeSet<_>>();
        for article in &articles {
            tags.extend(
                article
                    .get("tags")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(str::to_string),
            );
        }
        record["tags"] = json!(tags.into_iter().collect::<Vec<_>>());
        record["articlesCount"] =
            count_object(self.online_store_blog_articles_count(&record, &articles));
        record["articles"] = connection_json(articles);
        record
    }

    fn touch_online_store_blog(&mut self, id: &str, timestamp: &str) {
        let Some(mut blog) = self.online_store_record(OnlineStoreKind::Blog, id) else {
            return;
        };
        blog["updatedAt"] = json!(timestamp);
        self.stage_online_store_record(OnlineStoreKind::Blog, id.to_string(), blog);
    }

    fn online_store_blog_articles_count(&self, blog: &Value, known_articles: &[Value]) -> usize {
        let Some(blog_id) = blog.get("id").and_then(Value::as_str) else {
            return known_articles.len();
        };
        if is_synthetic_gid(blog_id) {
            return known_articles.len();
        }

        let Some(baseline) = blog
            .get(OBSERVED_BLOG_ARTICLES_COUNT_FIELD)
            .and_then(Value::as_u64)
            .or_else(|| blog.pointer("/articlesCount/count").and_then(Value::as_u64))
            .map(|count| count as usize)
        else {
            return known_articles.len();
        };
        let synthetic_additions = known_articles
            .iter()
            .filter_map(|article| article.get("id").and_then(Value::as_str))
            .filter(|id| is_synthetic_gid(id))
            .count();
        let deleted_baseline = self
            .store
            .staged
            .deleted_online_store_article_ids
            .iter()
            .filter(|id| !is_synthetic_gid(id))
            .filter(|id| {
                self.store
                    .staged
                    .online_store_articles
                    .get(*id)
                    .and_then(|article| article.get("blogId"))
                    .and_then(Value::as_str)
                    == Some(blog_id)
            })
            .count();
        baseline
            .saturating_sub(deleted_baseline)
            .saturating_add(synthetic_additions)
            .max(known_articles.len())
    }

    fn enriched_article_record(&self, record: &Value) -> Value {
        let mut record = record.clone();
        let article_id = record["id"].as_str().unwrap_or_default().to_string();
        let blog_id = record["blogId"].as_str().unwrap_or_default().to_string();
        record["blog"] = self
            .online_store_record(OnlineStoreKind::Blog, &blog_id)
            .map(|blog| self.blog_parent_record(&blog))
            .unwrap_or(Value::Null);
        let comments = self
            .online_store_records(OnlineStoreKind::Comment)
            .into_iter()
            .filter(|comment| comment["articleId"].as_str() == Some(article_id.as_str()))
            .collect::<Vec<_>>();
        record["commentsCount"] = count_object(comments.len());
        record["comments"] = connection_json(comments);
        record
    }

    fn article_parent_record(&self, record: &Value) -> Value {
        let mut record = record.clone();
        let article_id = record["id"].as_str().unwrap_or_default().to_string();
        let blog_id = record["blogId"].as_str().unwrap_or_default().to_string();
        record["blog"] = self
            .online_store_record(OnlineStoreKind::Blog, &blog_id)
            .map(|blog| self.blog_parent_record(&blog))
            .unwrap_or(Value::Null);
        let comments = self
            .online_store_raw_records(OnlineStoreKind::Comment)
            .into_iter()
            .filter(|comment| comment["articleId"].as_str() == Some(article_id.as_str()))
            .collect::<Vec<_>>();
        record["commentsCount"] = count_object(comments.len());
        record["comments"] = connection_json(comments);
        record
    }

    fn enriched_comment_record(&self, record: &Value) -> Value {
        let mut record = record.clone();
        let article_id = record["articleId"].as_str().unwrap_or_default();
        record["article"] = self
            .online_store_record(OnlineStoreKind::Article, article_id)
            .map(|article| self.article_parent_record(&article))
            .unwrap_or(Value::Null);
        record
    }

    fn stage_online_store_record(&mut self, kind: OnlineStoreKind, id: String, record: Value) {
        kind.deleted_ids_mut(&mut self.store.staged).remove(&id);
        if !kind.records(&self.store.staged).contains_key(&id) {
            kind.order_mut(&mut self.store.staged).push(id.clone());
        }
        kind.records_mut(&mut self.store.staged).insert(id, record);
    }

    fn tombstone_online_store_article(&mut self, id: &str) {
        self.store
            .staged
            .deleted_online_store_article_ids
            .insert(id.to_string());
        let comment_ids = self
            .store
            .staged
            .online_store_comments
            .values()
            .filter(|comment| comment["articleId"].as_str() == Some(id))
            .filter_map(|comment| comment["id"].as_str().map(str::to_string))
            .collect::<Vec<_>>();
        for comment_id in comment_ids {
            self.store.staged.online_store_comments.remove(&comment_id);
            self.store
                .staged
                .online_store_comment_order
                .retain(|id| id != &comment_id);
            self.store
                .staged
                .deleted_online_store_comment_ids
                .insert(comment_id);
        }
    }

    fn online_store_page_handle_taken(&self, handle: &str, excluding_id: Option<&str>) -> bool {
        self.store
            .staged
            .online_store_pages
            .values()
            .filter(|page| page["id"].as_str() != excluding_id)
            .filter(|page| {
                !page["id"]
                    .as_str()
                    .is_some_and(|id| self.store.staged.deleted_online_store_page_ids.contains(id))
            })
            .any(|page| page["handle"].as_str() == Some(handle))
    }

    fn unique_online_store_page_handle(&self, base: &str, excluding_id: Option<&str>) -> String {
        if !self.online_store_page_handle_taken(base, excluding_id) {
            return base.to_string();
        }
        for index in 1.. {
            let candidate = format!("{base}-{index}");
            if !self.online_store_page_handle_taken(&candidate, excluding_id) {
                return candidate;
            }
        }
        unreachable!("unbounded page handle suffix search should return")
    }
}

fn comment_moderation_transition(root: &str, status: &str) -> Result<String, &'static str> {
    let Some((next_status, allowed_statuses, error_message)) = (match root {
        "commentApprove" => Some((
            "PUBLISHED",
            &["PUBLISHED", "UNAPPROVED", "PENDING"][..],
            "Status cannot transition via \"approve\"",
        )),
        "commentSpam" => Some((
            "SPAM",
            &["SPAM", "PUBLISHED", "UNAPPROVED", "PENDING"][..],
            "Status cannot transition via \"spam\"",
        )),
        "commentNotSpam" => Some((
            "PUBLISHED",
            &["PUBLISHED", "SPAM"][..],
            "Status cannot transition via \"not spam\"",
        )),
        _ => None,
    }) else {
        return Ok(status.to_string());
    };
    if allowed_statuses.contains(&status) {
        Ok(next_status.to_string())
    } else {
        Err(error_message)
    }
}

fn input_title_and_handle(input: &BTreeMap<String, ResolvedValue>) -> (String, String) {
    let title = resolved_string_field(input, "title").unwrap_or_default();
    let handle = resolved_string_field(input, "handle").unwrap_or_else(|| slugify_handle(&title));
    (title, handle)
}

fn apply_title_and_handle(record: &mut Value, input: &BTreeMap<String, ResolvedValue>) {
    if let Some(title) = resolved_string_field(input, "title") {
        record["title"] = json!(title);
    }
    if let Some(handle) = resolved_string_field(input, "handle") {
        record["handle"] = json!(handle);
    }
}

fn blog_record(id: &str, input: &BTreeMap<String, ResolvedValue>, timestamp: &str) -> Value {
    let (title, handle) = input_title_and_handle(input);
    let comment_policy =
        resolved_string_field(input, "commentPolicy").unwrap_or_else(|| "CLOSED".to_string());
    json!({
        "__typename": "Blog",
        "id": id,
        "title": title,
        "handle": handle,
        "commentPolicy": comment_policy,
        "tags": resolved_string_list_field(input, "tags"),
        "templateSuffix": optional_string_value(input, "templateSuffix"),
        "createdAt": timestamp,
        "updatedAt": timestamp,
        "metafields": connection_json(Vec::new()),
        "articlesCount": count_object(0),
        "articles": connection_json(Vec::new())
    })
}

fn apply_blog_input(record: &mut Value, input: &BTreeMap<String, ResolvedValue>, timestamp: &str) {
    apply_title_and_handle(record, input);
    if let Some(comment_policy) = resolved_string_field(input, "commentPolicy") {
        record["commentPolicy"] = json!(comment_policy);
    }
    if input.contains_key("tags") {
        record["tags"] = json!(resolved_string_list_field(input, "tags"));
    }
    if input.contains_key("templateSuffix") {
        record["templateSuffix"] = optional_string_value(input, "templateSuffix");
    }
    record["updatedAt"] = json!(timestamp);
}

fn page_record(
    id: &str,
    input: &BTreeMap<String, ResolvedValue>,
    existing: Option<&Value>,
    timestamp: &str,
) -> Value {
    let (title, handle) = input_title_and_handle(input);
    let body = resolved_string_field(input, "body").unwrap_or_default();
    let (is_published, published_at) = publication_state(input, existing, true, timestamp);
    json!({
        "__typename": "Page",
        "id": id,
        "title": title,
        "handle": handle,
        "body": body,
        "bodySummary": body_summary(&body),
        "isPublished": is_published,
        "publishedAt": published_at,
        "createdAt": timestamp,
        "updatedAt": timestamp,
        "templateSuffix": optional_string_value(input, "templateSuffix")
    })
}

fn apply_page_input(record: &mut Value, input: &BTreeMap<String, ResolvedValue>, timestamp: &str) {
    apply_title_and_handle(record, input);
    if let Some(body) = resolved_string_field(input, "body") {
        record["body"] = json!(body);
        record["bodySummary"] = json!(body_summary(record["body"].as_str().unwrap_or_default()));
    }
    if input.contains_key("isPublished") || input.contains_key("publishDate") {
        let (is_published, published_at) = publication_state(input, Some(record), false, timestamp);
        record["isPublished"] = json!(is_published);
        record["publishedAt"] = published_at;
    }
    if input.contains_key("templateSuffix") {
        record["templateSuffix"] = optional_string_value(input, "templateSuffix");
    }
    record["updatedAt"] = json!(timestamp);
}

fn article_record(
    id: &str,
    blog_id: &str,
    input: &BTreeMap<String, ResolvedValue>,
    existing: Option<&Value>,
    timestamp: &str,
) -> Value {
    let (title, handle) = input_title_and_handle(input);
    let body = resolved_string_field(input, "body").unwrap_or_default();
    let summary = optional_string_value(input, "summary");
    let (is_published, published_at) = publication_state(input, existing, true, timestamp);
    let mut record = json!({
        "__typename": "Article",
        "id": id,
        "blogId": blog_id,
        "title": title,
        "handle": handle,
        "body": body,
        "summary": summary,
        "tags": resolved_string_list_field(input, "tags"),
        "isPublished": is_published,
        "publishedAt": published_at,
        "createdAt": timestamp,
        "updatedAt": timestamp,
        "templateSuffix": optional_string_value(input, "templateSuffix"),
        "author": article_author_json(input),
        "image": article_image_json(input),
        "metafields": connection_json(Vec::new()),
        "commentsCount": count_object(0),
        "comments": connection_json(Vec::new())
    });
    apply_online_store_metafields_input(&mut record, input, timestamp, "ARTICLE");
    record
}

fn apply_article_input(
    record: &mut Value,
    input: &BTreeMap<String, ResolvedValue>,
    timestamp: &str,
) {
    apply_title_and_handle(record, input);
    if let Some(body) = resolved_string_field(input, "body") {
        record["body"] = json!(body);
    }
    if input.contains_key("summary") {
        record["summary"] = optional_string_value(input, "summary");
    }
    if input.contains_key("tags") {
        record["tags"] = json!(resolved_string_list_field(input, "tags"));
    }
    if input.contains_key("author") || input.contains_key("authorV2") {
        record["author"] = article_author_json(input);
    }
    if input.contains_key("image") {
        record["image"] = article_image_json(input);
    }
    if input.contains_key("metafields") {
        apply_online_store_metafields_input(record, input, timestamp, "ARTICLE");
    }
    if input.contains_key("isPublished") || input.contains_key("publishDate") {
        let (is_published, published_at) = publication_state(input, Some(record), false, timestamp);
        record["isPublished"] = json!(is_published);
        record["publishedAt"] = published_at;
    }
    if input.contains_key("templateSuffix") {
        record["templateSuffix"] = optional_string_value(input, "templateSuffix");
    }
    record["updatedAt"] = json!(timestamp);
}

fn publication_state(
    input: &BTreeMap<String, ResolvedValue>,
    existing: Option<&Value>,
    create: bool,
    timestamp: &str,
) -> (bool, Value) {
    let supplied_date = resolved_string_field(input, "publishDate");
    let existing_published_at = existing
        .map(|record| record["publishedAt"].clone())
        .unwrap_or(Value::Null);
    let is_published = effective_published(input, existing, create);
    let published_at = if let Some(date) = supplied_date {
        json!(date)
    } else if is_published {
        if existing_published_at.is_null() {
            json!(timestamp)
        } else {
            existing_published_at
        }
    } else {
        Value::Null
    };
    (is_published, published_at)
}

fn effective_published(
    input: &BTreeMap<String, ResolvedValue>,
    existing: Option<&Value>,
    create: bool,
) -> bool {
    let supplied_published = resolved_bool_field(input, "isPublished");
    let supplied_date = resolved_string_field(input, "publishDate");
    let existing_published = existing
        .and_then(|record| record["isPublished"].as_bool())
        .unwrap_or(false);

    supplied_published.unwrap_or_else(|| {
        if create && supplied_date.is_none() {
            true
        } else {
            existing_published
        }
    })
}

fn invalid_publish_date_error(
    input: &BTreeMap<String, ResolvedValue>,
    root: &'static str,
    existing: Option<&Value>,
    create: bool,
) -> Option<Value> {
    let effective_is_published = effective_published(input, existing, create);
    let publish_date = resolved_string_field(input, "publishDate");
    if effective_is_published && publish_date.as_deref().is_some_and(is_future_date) {
        Some(user_error(
            vec![root],
            "Can\u{2019}t set isPublished to true and also set a future publish date.",
            Some("INVALID_PUBLISH_DATE"),
        ))
    } else {
        None
    }
}

pub(super) fn online_store_operation_timestamp() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .expect("UTC timestamps should format as RFC3339")
}

fn is_future_date(value: &str) -> bool {
    let Some(publish_date) = parse_rfc3339_epoch_seconds(value) else {
        return false;
    };
    let Ok(now) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) else {
        return false;
    };
    publish_date > now.as_secs() as i64
}

fn title_blank_error(
    input: &BTreeMap<String, ResolvedValue>,
    root: &'static str,
    code: Option<&'static str>,
    required: bool,
) -> Option<Value> {
    let error = || match code {
        Some("BLANK") => presence_user_error(vec![root, "title"], "Title"),
        _ => user_error(vec![root, "title"], "Title can't be blank", code),
    };
    match input.get("title") {
        Some(ResolvedValue::String(title)) if title.trim().is_empty() => Some(error()),
        None if required => Some(error()),
        _ => None,
    }
}

fn content_length_error(
    input: &BTreeMap<String, ResolvedValue>,
    root: &'static str,
    handle_limit: usize,
    body_limit: Option<(usize, &'static str, Option<&'static str>)>,
) -> Option<Value> {
    if resolved_string_field(input, "title")
        .as_deref()
        .is_some_and(|title| title.chars().count() > ONLINE_STORE_TITLE_MAX_CHARS)
    {
        return Some(length_user_error(
            vec![root, "title"],
            "Title",
            LengthUserErrorBound::TooLong {
                maximum: ONLINE_STORE_TITLE_MAX_CHARS,
            },
        ));
    }
    if resolved_string_field(input, "handle")
        .as_deref()
        .is_some_and(|handle| handle.chars().count() > handle_limit)
    {
        return Some(length_user_error(
            vec![root, "handle"],
            "Handle",
            LengthUserErrorBound::TooLong {
                maximum: handle_limit,
            },
        ));
    }
    if let Some((limit, message, code)) = body_limit {
        if resolved_string_field(input, "body")
            .as_deref()
            .is_some_and(|body| body.len() > limit)
        {
            return Some(user_error(vec![root, "body"], message, code));
        }
    }
    None
}

fn article_author_error(
    input: &BTreeMap<String, ResolvedValue>,
    create: bool,
    required: bool,
) -> Option<Value> {
    let author = resolved_object_field(input, "author");
    let author_v2 = resolved_object_field(input, "authorV2");
    if author.is_none() && author_v2.is_none() && !required {
        return None;
    }
    let name = author
        .as_ref()
        .and_then(|author| resolved_string_field(author, "name"))
        .or_else(|| {
            author_v2
                .as_ref()
                .and_then(|author| resolved_string_field(author, "name"))
        });
    let user_id = author
        .as_ref()
        .and_then(|author| resolved_string_field(author, "userId"))
        .or_else(|| {
            author_v2
                .as_ref()
                .and_then(|author| resolved_string_field(author, "userId"))
        });
    let has_name = name.as_deref().is_some_and(|name| !name.trim().is_empty());
    let has_user_id = user_id
        .as_deref()
        .is_some_and(|user_id| !user_id.trim().is_empty());
    if has_name && has_user_id {
        return Some(user_error(
            vec!["article"],
            if create {
                "Can't create an article author if both author name and user ID are supplied."
            } else {
                "Can't update an article author if both author name and user ID are supplied."
            },
            Some("AMBIGUOUS_AUTHOR"),
        ));
    }
    if has_user_id {
        return Some(user_error(
            vec!["article"],
            "User must exist if a user ID is supplied.",
            Some("AUTHOR_MUST_EXIST"),
        ));
    }
    if (required || author.is_some() || author_v2.is_some()) && !has_name {
        return Some(user_error(
            vec!["article"],
            if create {
                "Can't create an article if both author name and user ID are blank."
            } else {
                "Can't update an article if both author name and user ID are blank."
            },
            Some("AUTHOR_FIELD_REQUIRED"),
        ));
    }
    None
}

fn article_image_update_error(
    record: &Value,
    input: &BTreeMap<String, ResolvedValue>,
) -> Option<Value> {
    let image = resolved_object_field(input, "image")?;
    let has_alt_text = resolved_string_field(&image, "altText")
        .as_deref()
        .is_some_and(|alt_text| !alt_text.trim().is_empty());
    let has_new_url = resolved_string_field(&image, "url")
        .as_deref()
        .is_some_and(|url| !url.trim().is_empty());
    let has_existing_url = record["image"]["url"]
        .as_str()
        .is_some_and(|url| !url.trim().is_empty());
    if has_alt_text && !has_new_url && !has_existing_url {
        Some(user_error(
            vec!["article", "image"],
            "Cannot update image alt text without an existing image or providing a new image URL",
            Some("INVALID"),
        ))
    } else {
        None
    }
}

fn article_author_json(input: &BTreeMap<String, ResolvedValue>) -> Value {
    if let Some(author) = resolved_object_field(input, "author") {
        if let Some(name) = resolved_string_field(&author, "name") {
            return json!({ "name": name });
        }
    }
    if let Some(author) = resolved_object_field(input, "authorV2") {
        if let Some(name) = resolved_string_field(&author, "name") {
            return json!({ "name": name });
        }
        if let Some(user_id) = resolved_string_field(&author, "userId") {
            return json!({ "name": user_id });
        }
    }
    Value::Null
}

fn article_image_json(input: &BTreeMap<String, ResolvedValue>) -> Value {
    let Some(image) = resolved_object_field(input, "image") else {
        return Value::Null;
    };
    let url = resolved_string_field(&image, "url");
    let alt_text = resolved_string_field(&image, "altText");
    if url.is_none() && alt_text.is_none() {
        return Value::Null;
    }
    json!({
        "url": url,
        "altText": alt_text
    })
}

fn online_store_content_metafield(record: &Value, namespace: &str, key: &str) -> Option<Value> {
    online_store_content_metafield_nodes(record, Some(namespace))
        .into_iter()
        .find(|metafield| metafield.get("key").and_then(Value::as_str) == Some(key))
}

fn online_store_content_metafield_nodes(record: &Value, namespace: Option<&str>) -> Vec<Value> {
    let mut nodes = connection_nodes(&record["metafields"]);
    if let Some(metafield) = record.get("metafield").filter(|value| value.is_object()) {
        let duplicate = nodes.iter().any(|node| {
            node.get("namespace").and_then(Value::as_str)
                == metafield.get("namespace").and_then(Value::as_str)
                && node.get("key").and_then(Value::as_str)
                    == metafield.get("key").and_then(Value::as_str)
        });
        if !duplicate {
            nodes.push(metafield.clone());
        }
    }

    nodes
        .into_iter()
        .filter(|metafield| {
            namespace.is_none_or(|namespace| {
                metafield.get("namespace").and_then(Value::as_str) == Some(namespace)
            })
        })
        .collect()
}

fn apply_online_store_metafields_input(
    record: &mut Value,
    input: &BTreeMap<String, ResolvedValue>,
    timestamp: &str,
    owner_type: &str,
) {
    let owner_id = record["id"].as_str().unwrap_or_default().to_string();
    let mut records = connection_nodes(&record["metafields"]);

    for metafield in resolved_object_list_field(input, "metafields") {
        let Some(namespace) = resolved_string_field(&metafield, "namespace") else {
            continue;
        };
        let Some(key) = resolved_string_field(&metafield, "key") else {
            continue;
        };
        let position = records.iter().position(|existing| {
            existing.get("namespace").and_then(Value::as_str) == Some(namespace.as_str())
                && existing.get("key").and_then(Value::as_str) == Some(key.as_str())
        });
        let existing = position.map(|index| records[index].clone());
        let metafield = online_store_metafield_record(
            &owner_id,
            owner_type,
            &metafield,
            existing.as_ref(),
            timestamp,
        );
        match position {
            Some(index) => records[index] = metafield,
            None => records.push(metafield),
        }
    }

    record["metafields"] = connection_json(records);
}

fn online_store_metafield_record(
    owner_id: &str,
    owner_type: &str,
    input: &BTreeMap<String, ResolvedValue>,
    existing: Option<&Value>,
    timestamp: &str,
) -> Value {
    let namespace = resolved_string_field(input, "namespace").unwrap_or_default();
    let key = resolved_string_field(input, "key").unwrap_or_default();
    let metafield_type = resolved_string_field(input, "type").unwrap_or_else(|| {
        existing
            .and_then(|metafield| metafield.get("type"))
            .and_then(Value::as_str)
            .unwrap_or("single_line_text_field")
            .to_string()
    });
    let raw_value = resolved_string_field(input, "value").unwrap_or_else(|| {
        existing
            .and_then(|metafield| metafield.get("value"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    });
    let normalized_value = normalize_metafield_value_string(&metafield_type, &raw_value);
    let created_at = existing
        .and_then(|metafield| metafield.get("createdAt"))
        .and_then(Value::as_str)
        .unwrap_or(timestamp);
    let updated_at = existing
        .filter(|metafield| {
            metafield.get("value").and_then(Value::as_str) == Some(normalized_value.as_str())
        })
        .and_then(|metafield| metafield.get("updatedAt"))
        .and_then(Value::as_str)
        .unwrap_or(timestamp);

    json!({
        "__typename": "Metafield",
        "id": existing
            .and_then(|metafield| metafield.get("id"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| online_store_metafield_id(owner_id, &namespace, &key)),
        "namespace": namespace,
        "key": key,
        "type": metafield_type.clone(),
        "value": normalized_value.clone(),
        "jsonValue": metafield_json_value(&metafield_type, &raw_value),
        "compareDigest": metafield_compare_digest(&normalized_value),
        "ownerType": owner_type,
        "createdAt": created_at,
        "updatedAt": updated_at
    })
}

fn online_store_metafield_id(owner_id: &str, namespace: &str, key: &str) -> String {
    let digest = metafield_compare_digest(&format!("{owner_id}\n{namespace}\n{key}"));
    shopify_gid("Metafield", &digest[..16])
}

fn body_summary(body: &str) -> String {
    let mut output = String::new();
    let mut in_tag = false;
    for character in body.chars() {
        match character {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => output.push(character),
            _ => {}
        }
    }
    output.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn optional_string_value(input: &BTreeMap<String, ResolvedValue>, field: &str) -> Value {
    match input.get(field) {
        Some(ResolvedValue::String(value)) => json!(value),
        Some(ResolvedValue::Null) => Value::Null,
        _ => Value::Null,
    }
}

fn article_blog_not_found_payload(key: &str) -> Value {
    resource_payload(
        key,
        Value::Null,
        vec![user_error(
            vec!["article"],
            "Must reference an existing blog.",
            Some("NOT_FOUND"),
        )],
    )
}

fn should_stage_observed_blog(record: &Value) -> bool {
    record.get("title").is_some()
        || record.get("handle").is_some()
        || record.get("commentPolicy").is_some()
        || record.get("tags").is_some()
        || record.get("templateSuffix").is_some()
        || record.get("metafields").is_some()
        || record.get("articles").is_some()
}

fn should_stage_observed_page(record: &Value) -> bool {
    record.get("title").is_some() || record.get("handle").is_some() || record.get("body").is_some()
}

fn should_stage_observed_article(record: &Value) -> bool {
    record.get("title").is_some()
        || record.get("handle").is_some()
        || record.get("body").is_some()
        || record.get("summary").is_some()
        || record.get("tags").is_some()
        || record.get("isPublished").is_some()
        || record.get("publishedAt").is_some()
        || record.get("templateSuffix").is_some()
        || record.get("author").is_some()
        || record.get("image").is_some()
        || record.get("metafields").is_some()
        || record.get("comments").is_some()
}

fn should_stage_observed_comment(record: &Value) -> bool {
    record.get("status").is_some()
        || record.get("body").is_some()
        || record.get("bodyHtml").is_some()
        || record.get("article").is_some()
}

fn merge_online_store_observed_values(existing: &Value, observed: &Value) -> Value {
    let (Some(existing), Some(observed)) = (existing.as_object(), observed.as_object()) else {
        return observed.clone();
    };
    let mut merged = existing.clone();
    for (key, value) in observed {
        let value = merged
            .get(key)
            .map(|current| merge_online_store_observed_values(current, value))
            .unwrap_or_else(|| value.clone());
        merged.insert(key.clone(), value);
    }
    Value::Object(merged)
}

fn string_value(record: &Value, key: &str) -> Option<String> {
    record.get(key).and_then(Value::as_str).map(str::to_string)
}

fn bool_value(record: &Value, key: &str) -> Option<bool> {
    record.get(key).and_then(Value::as_bool)
}

fn observed_title_and_handle(record: &Value) -> (String, String) {
    let title = string_value(record, "title").unwrap_or_default();
    let handle = string_value(record, "handle").unwrap_or_else(|| slugify_handle(&title));
    (title, handle)
}

fn normalize_observed_blog(record: &Value) -> Value {
    let mut record = record.clone();
    let articles = record.get("articles").map(connection_nodes);
    record["__typename"] = json!("Blog");
    if record.get("articlesCount").is_none() {
        if let Some(articles) = articles.as_ref() {
            record["articlesCount"] = count_object(articles.len());
        }
    }
    if let Some(count) = record
        .pointer("/articlesCount/count")
        .and_then(Value::as_u64)
    {
        let previous = record
            .get(OBSERVED_BLOG_ARTICLES_COUNT_FIELD)
            .and_then(Value::as_u64)
            .unwrap_or(0);
        record[OBSERVED_BLOG_ARTICLES_COUNT_FIELD] = json!(previous.max(count));
    }
    record
}

fn normalize_observed_page(record: &Value) -> Value {
    let mut record = record.clone();
    let (title, handle) = observed_title_and_handle(&record);
    let body = string_value(&record, "body").unwrap_or_default();
    record["__typename"] = json!("Page");
    record["title"] = json!(title);
    record["handle"] = json!(handle);
    record["body"] = json!(body);
    if record.get("bodySummary").is_none() {
        record["bodySummary"] = json!(body_summary(record["body"].as_str().unwrap_or_default()));
    }
    if record.get("isPublished").is_none() {
        record["isPublished"] = json!(false);
    }
    if record.get("publishedAt").is_none() {
        record["publishedAt"] = Value::Null;
    }
    if record.get("templateSuffix").is_none() {
        record["templateSuffix"] = Value::Null;
    }
    record
}

fn normalize_observed_article(record: &Value, parent_blog_id: Option<&str>) -> Value {
    let mut record = record.clone();
    let blog_id = string_value(&record, "blogId")
        .or_else(|| {
            record
                .get("blog")
                .and_then(|blog| blog.get("id"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .or_else(|| parent_blog_id.map(str::to_string));
    let comments = record.get("comments").map(connection_nodes);
    record["__typename"] = json!("Article");
    if let Some(blog_id) = blog_id {
        record["blogId"] = json!(blog_id);
    }
    if record.get("commentsCount").is_none() {
        if let Some(comments) = comments.as_ref() {
            record["commentsCount"] = count_object(comments.len());
        }
    }
    record
}

fn normalize_observed_comment(record: &Value, parent_article_id: Option<&str>) -> Value {
    let mut record = record.clone();
    let status = string_value(&record, "status")
        .map(|status| match status.as_str() {
            "pending" => "UNAPPROVED".to_string(),
            "published" => "PUBLISHED".to_string(),
            "spam" => "SPAM".to_string(),
            _ => status,
        })
        .unwrap_or_else(|| "UNAPPROVED".to_string());
    let article_id = string_value(&record, "articleId")
        .or_else(|| {
            record
                .get("article")
                .and_then(|article| article.get("id"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .or_else(|| parent_article_id.map(str::to_string))
        .unwrap_or_default();
    let body = string_value(&record, "body")
        .or_else(|| string_value(&record, "bodyHtml"))
        .unwrap_or_default();
    let body_html = string_value(&record, "bodyHtml").unwrap_or_else(|| body.clone());
    let is_published = bool_value(&record, "isPublished").unwrap_or(status == "PUBLISHED");
    record["__typename"] = json!("Comment");
    record["articleId"] = json!(article_id);
    record["status"] = json!(status);
    record["isPublished"] = json!(is_published);
    record["body"] = json!(body);
    record["bodyHtml"] = json!(body_html);
    if record.get("publishedAt").is_none() {
        record["publishedAt"] = Value::Null;
    }
    record
}

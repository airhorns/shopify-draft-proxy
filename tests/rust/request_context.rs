use super::*;

fn response(data: Value) -> Response {
    Response {
        status: 200,
        headers: BTreeMap::new(),
        body: json!({ "data": data }),
    }
}

fn disposition(query: &str, read_mode: ReadMode) -> AdminRequestDisposition {
    let variables = BTreeMap::new();
    let document = parsed_document(query, &variables).expect("query should parse");
    let proxy = DraftProxy::new(Config {
        read_mode,
        ..Config::default()
    });
    let request = Request::default();
    let context = proxy.admin_operation_context(
        &request,
        document.operation_type,
        &document.root_fields,
        &variables,
    );
    proxy.admin_request_disposition(&context)
}

#[test]
fn caller_transport_and_canonical_values_remain_separate() {
    let mut cache = RequestCache::default();
    cache.record_caller_response(
        response(json!({ "aliasedProduct": { "aliasedTitle": "Bag" } })),
        json!({ "aliasedProduct": { "title": "Bag" } }),
    );

    assert_eq!(
        cache
            .caller_response()
            .and_then(|response| response.body.pointer("/data/aliasedProduct/aliasedTitle")),
        Some(&json!("Bag"))
    );
    assert_eq!(
        cache
            .caller_data()
            .and_then(|data| data.pointer("/aliasedProduct/title")),
        Some(&json!("Bag"))
    );
}

#[test]
fn cache_keys_and_entity_evidence_preserve_their_invariants() {
    assert_ne!(
        RequestCacheKey::composite("owner-metafield", &["owner:a", "b"]),
        RequestCacheKey::composite("owner-metafield", &["owner", "a:b"]),
    );

    let mut cache = RequestCache::default();
    let id = "gid://shopify/Product/1";
    cache.mark_entity("owner-metafields", id, EntityEvidenceState::Observed);
    cache.mark_entity("owner-metafields", id, EntityEvidenceState::Requested);

    assert_eq!(
        cache.entity_state("owner-metafields", id),
        Some(EntityEvidenceState::Observed)
    );
    assert!(cache.entity_was_hydrated("owner-metafields", id));
    assert!(!cache.entity_is_missing("owner-metafields", id));
}

#[test]
fn request_disposition_preserves_whole_document_passthrough_cases() {
    assert!(
        disposition(
            "query { products(first: 1) { nodes { id } } }",
            ReadMode::LiveHybrid,
        )
        .direct_full_query_passthrough
    );
    assert!(
        disposition(
            "query { one: shop { id } two: shop { name } }",
            ReadMode::LiveHybrid,
        )
        .direct_full_query_passthrough
    );
    assert!(
        !disposition(
            "query { events(first: 1) { nodes { id } } }",
            ReadMode::Snapshot,
        )
        .direct_full_query_passthrough
    );
}

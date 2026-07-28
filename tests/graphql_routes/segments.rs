use super::common::*;
use pretty_assertions::assert_eq;

#[test]
fn segment_create_hydrates_authoritative_name_and_count_before_staging() {
    let persisted_id = "gid://shopify/Segment/7000";
    let upstream_requests = Arc::new(Mutex::new(Vec::<Request>::new()));
    let captured_requests = Arc::clone(&upstream_requests);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let body: Value = serde_json::from_str(&request.body).expect("upstream body parses");
            captured_requests.lock().unwrap().push(request);
            assert_eq!(
                body["operationName"],
                json!("SegmentAuthoritativePrerequisites")
            );
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "count": { "count": 1, "precision": "EXACT" },
                        "name0": {
                            "nodes": [{
                                "id": persisted_id,
                                "name": "Persisted collision",
                                "query": "number_of_orders >= 1",
                                "creationDate": "2026-07-01T12:00:00Z",
                                "lastEditDate": "2026-07-01T12:00:00Z"
                            }],
                            "pageInfo": { "hasNextPage": false }
                        }
                    }
                }),
            }
        });

    let mutation = r#"
        mutation CreateAgainstPersistedName($name: String!, $query: String!) {
          segmentCreate(name: $name, query: $query) {
            segment { id name query }
            userErrors { field message }
          }
        }
    "#;
    let variables = json!({
        "name": "Persisted collision",
        "query": "number_of_orders >= 2"
    });
    let mut request = json_graphql_request(mutation, variables.clone());
    request.path = "/admin/api/2025-01/graphql.json".to_string();
    request.headers.insert(
        "x-shopify-access-token".to_string(),
        "segment-prerequisite-test-token".to_string(),
    );
    let raw_body = request.body.clone();
    let response = proxy.process_request(request);

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["segmentCreate"]["segment"]["name"],
        json!("Persisted collision (2)"),
        "{}",
        response.body
    );
    assert_eq!(
        response.body["data"]["segmentCreate"]["userErrors"],
        json!([])
    );

    let requests = upstream_requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].path, "/admin/api/2025-01/graphql.json");
    assert_eq!(
        requests[0].headers.get("x-shopify-access-token"),
        Some(&"segment-prerequisite-test-token".to_string())
    );
    let upstream_body: Value =
        serde_json::from_str(&requests[0].body).expect("upstream body parses");
    assert!(upstream_body["query"]
        .as_str()
        .unwrap()
        .contains("segmentsCount"));
    assert!(upstream_body["query"]
        .as_str()
        .unwrap()
        .contains("segments("));
    assert!(!upstream_body["query"]
        .as_str()
        .unwrap()
        .contains("segmentCreate"));
    drop(requests);

    let log = log_snapshot(&proxy);
    assert_eq!(log["entries"].as_array().unwrap().len(), 1);
    assert_eq!(log["entries"][0]["rawBody"], raw_body);
    assert_eq!(
        log["entries"][0]["variables"], variables,
        "the original mutation variables remain replayable"
    );
}

#[test]
fn segment_prerequisites_batch_deduplicate_and_cache_hits_and_misses() {
    let update_id = "gid://shopify/Segment/7100";
    let delete_id = "gid://shopify/Segment/7101";
    let member_id = "gid://shopify/Segment/7102";
    let missing_id = "gid://shopify/Segment/7199";
    let upstream_requests = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_requests = Arc::clone(&upstream_requests);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let body: Value = serde_json::from_str(&request.body).expect("upstream body parses");
            captured_requests.lock().unwrap().push(body.clone());
            assert_eq!(body["operationName"], json!("SegmentPrerequisiteNodes"));
            assert_eq!(
                body["variables"]["ids"],
                json!([update_id, delete_id, member_id, missing_id])
            );
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "nodes": [
                            {
                                "id": update_id,
                                "name": "Update target",
                                "query": "number_of_orders >= 1",
                                "creationDate": "2026-07-01T12:00:00Z",
                                "lastEditDate": "2026-07-01T12:00:00Z"
                            },
                            {
                                "id": delete_id,
                                "name": "Delete target",
                                "query": "number_of_orders >= 2",
                                "creationDate": "2026-07-01T12:00:00Z",
                                "lastEditDate": "2026-07-01T12:00:00Z"
                            },
                            {
                                "id": member_id,
                                "name": "Member target",
                                "query": "number_of_orders >= 3",
                                "creationDate": "2026-07-01T12:00:00Z",
                                "lastEditDate": "2026-07-01T12:00:00Z"
                            },
                            null
                        ]
                    }
                }),
            }
        });

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation SegmentPrerequisiteBatch(
          $updateId: ID!
          $deleteId: ID!
          $memberId: ID!
          $missingId: ID!
        ) {
          update: segmentUpdate(id: $updateId, query: "number_of_orders >= 10") {
            segment { id query }
            userErrors { field message }
          }
          delete: segmentDelete(id: $deleteId) {
            deletedSegmentId
            userErrors { field message }
          }
          member: customerSegmentMembersQueryCreate(input: { segmentId: $memberId }) {
            customerSegmentMembersQuery { id currentCount done }
            userErrors { field code message }
          }
          missing: customerSegmentMembersQueryCreate(input: { segmentId: $missingId }) {
            customerSegmentMembersQuery { id }
            userErrors { field code message }
          }
          missingAgain: customerSegmentMembersQueryCreate(input: { segmentId: $missingId }) {
            customerSegmentMembersQuery { id }
            userErrors { field code message }
          }
        }
        "#,
        json!({
            "updateId": update_id,
            "deleteId": delete_id,
            "memberId": member_id,
            "missingId": missing_id
        }),
    ));

    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(response.body["data"]["update"]["userErrors"], json!([]));
    assert_eq!(
        response.body["data"]["delete"],
        json!({ "deletedSegmentId": delete_id, "userErrors": [] })
    );
    assert_eq!(response.body["data"]["member"]["userErrors"], json!([]));
    for key in ["missing", "missingAgain"] {
        assert_eq!(
            response.body["data"][key]["userErrors"],
            json!([{
                "field": null,
                "code": "INVALID",
                "message": "Invalid segment ID."
            }])
        );
    }
    assert_eq!(upstream_requests.lock().unwrap().len(), 1);

    let cached_miss = proxy.process_request(json_graphql_request(
        r#"
        mutation CachedSegmentMiss($input: CustomerSegmentMembersQueryInput!) {
          customerSegmentMembersQueryCreate(input: $input) {
            customerSegmentMembersQuery { id }
            userErrors { field code message }
          }
        }
        "#,
        json!({ "input": { "segmentId": missing_id } }),
    ));
    assert_eq!(
        cached_miss.body["data"]["customerSegmentMembersQueryCreate"]["userErrors"][0]["message"],
        json!("Invalid segment ID.")
    );
    assert_eq!(upstream_requests.lock().unwrap().len(), 1);
    let log = log_snapshot(&proxy);
    assert_eq!(log["entries"].as_array().unwrap().len(), 1);
    assert_eq!(
        log["entries"][0]["stagedResourceIds"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
}

#[test]
fn segment_transport_failure_is_not_cached_as_an_authoritative_miss() {
    let target_id = "gid://shopify/Segment/7200";
    let call_count = Arc::new(Mutex::new(0usize));
    let captured_count = Arc::clone(&call_count);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |_request| {
            let mut count = captured_count.lock().unwrap();
            *count += 1;
            if *count == 1 {
                Response {
                    status: 503,
                    headers: Default::default(),
                    body: json!({ "errors": [{ "message": "upstream unavailable" }] }),
                }
            } else {
                Response {
                    status: 200,
                    headers: Default::default(),
                    body: json!({ "data": { "segment": null } }),
                }
            }
        });
    let mutation = r#"
        mutation UpdateColdSegment($id: ID!) {
          segmentUpdate(id: $id, query: "number_of_orders >= 4") {
            segment { id }
            userErrors { field message }
          }
        }
    "#;
    let before = proxy.process_request(request_with_body("POST", "/__meta/dump", ""));

    let unresolved =
        proxy.process_request(json_graphql_request(mutation, json!({ "id": target_id })));
    assert_eq!(unresolved.status, 503, "{}", unresolved.body);
    assert_eq!(
        unresolved.body["errors"][0]["message"],
        json!("upstream unavailable")
    );
    assert!(
        unresolved.body["data"].is_null() || unresolved.body["data"]["segmentUpdate"].is_null()
    );
    let after_unresolved = proxy.process_request(request_with_body("POST", "/__meta/dump", ""));
    assert_eq!(after_unresolved.body["state"], before.body["state"]);
    assert_eq!(
        after_unresolved.body["nextSyntheticId"],
        before.body["nextSyntheticId"]
    );
    assert_eq!(after_unresolved.body["log"], before.body["log"]);

    let confirmed_miss =
        proxy.process_request(json_graphql_request(mutation, json!({ "id": target_id })));
    assert_eq!(confirmed_miss.status, 200);
    assert_eq!(
        confirmed_miss.body["data"]["segmentUpdate"]["userErrors"],
        json!([{ "field": ["id"], "message": "Segment does not exist" }])
    );
    let cached_miss =
        proxy.process_request(json_graphql_request(mutation, json!({ "id": target_id })));
    assert_eq!(
        cached_miss.body["data"]["segmentUpdate"]["userErrors"],
        confirmed_miss.body["data"]["segmentUpdate"]["userErrors"]
    );
    let after_miss = proxy.process_request(request_with_body("POST", "/__meta/dump", ""));
    assert_eq!(
        after_miss.body["state"]["stagedState"],
        before.body["state"]["stagedState"]
    );
    assert_eq!(
        after_miss.body["nextSyntheticId"],
        before.body["nextSyntheticId"]
    );
    assert_eq!(after_miss.body["log"], before.body["log"]);

    let cached_detail_miss = proxy.process_request(json_graphql_request(
        "query CachedSegmentMiss($id: ID!) { segment(id: $id) { id } }",
        json!({ "id": target_id }),
    ));
    assert_eq!(cached_detail_miss.body["data"]["segment"], Value::Null);
    assert_eq!(
        cached_detail_miss.body["errors"][0]["extensions"]["code"],
        json!("NOT_FOUND")
    );
    assert_eq!(*call_count.lock().unwrap(), 2);

    let snapshot_calls = Arc::new(Mutex::new(0usize));
    let captured_snapshot_calls = Arc::clone(&snapshot_calls);
    let mut snapshot =
        configured_proxy(ReadMode::Snapshot, None).with_upstream_transport(move |_request| {
            *captured_snapshot_calls.lock().unwrap() += 1;
            panic!("snapshot mutations must not hydrate")
        });
    let missing =
        snapshot.process_request(json_graphql_request(mutation, json!({ "id": target_id })));
    assert_eq!(
        missing.body["data"]["segmentUpdate"]["userErrors"][0]["message"],
        json!("Segment does not exist")
    );
    assert_eq!(*snapshot_calls.lock().unwrap(), 0);
}

#[test]
fn segment_limit_probe_is_cached_and_rejected_create_does_not_allocate_or_log() {
    let upstream_requests = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_requests = Arc::clone(&upstream_requests);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let body: Value = serde_json::from_str(&request.body).expect("upstream body parses");
            captured_requests.lock().unwrap().push(body);
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "count": { "count": 6000, "precision": "AT_LEAST" },
                        "name0": {
                            "nodes": [],
                            "pageInfo": { "hasNextPage": false }
                        }
                    }
                }),
            }
        });
    let before = proxy.process_request(request_with_body("POST", "/__meta/dump", ""));
    let mutation = r#"
        mutation SegmentCreateAtLimit($name: String!) {
          segmentCreate(name: $name, query: "number_of_orders >= 1") {
            segment { id name }
            userErrors { field message }
          }
        }
    "#;
    for name in ["Over limit one", "Over limit two"] {
        let response =
            proxy.process_request(json_graphql_request(mutation, json!({ "name": name })));
        assert_eq!(
            response.body["data"]["segmentCreate"],
            json!({
                "segment": null,
                "userErrors": [{
                    "field": null,
                    "message": "Segment limit reached. Delete an existing segment to create more."
                }]
            })
        );
    }
    let after = proxy.process_request(request_with_body("POST", "/__meta/dump", ""));
    assert_eq!(
        after.body["nextSyntheticId"],
        before.body["nextSyntheticId"]
    );
    assert_eq!(after.body["log"], before.body["log"]);
    assert_eq!(after.body["state"]["stagedState"]["segments"], json!({}));
    assert_eq!(upstream_requests.lock().unwrap().len(), 1);
}

#[test]
fn customer_segment_job_poll_hydrates_cold_jobs_without_hiding_staged_jobs() {
    let persisted_job_id = "gid://shopify/CustomerSegmentMembersQuery/persisted-job";
    let upstream_requests = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_requests = Arc::clone(&upstream_requests);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let body: Value = serde_json::from_str(&request.body).expect("upstream body parses");
            captured_requests.lock().unwrap().push(body.clone());
            assert_eq!(
                body["operationName"],
                json!("CustomerSegmentMembersQueryHydrate")
            );
            assert_eq!(body["variables"], json!({ "ids": [persisted_job_id] }));
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "nodes": [{
                            "id": persisted_job_id,
                            "currentCount": 42,
                            "done": true
                        }]
                    }
                }),
            }
        });
    let created = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateLocalMemberJob($input: CustomerSegmentMembersQueryInput!) {
          customerSegmentMembersQueryCreate(input: $input) {
            customerSegmentMembersQuery { id currentCount done }
            userErrors { field code message }
          }
        }
        "#,
        json!({ "input": { "query": "number_of_orders >= 1" } }),
    ));
    let staged_job_id = created.body["data"]["customerSegmentMembersQueryCreate"]
        ["customerSegmentMembersQuery"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let query = r#"
        query PollStagedAndPersistedJobs($staged: ID!, $persisted: ID!) {
          staged: customerSegmentMembersQuery(id: $staged) { id currentCount done }
          persisted: customerSegmentMembersQuery(id: $persisted) { id currentCount done }
        }
    "#;
    let variables = json!({ "staged": staged_job_id, "persisted": persisted_job_id });
    let response = proxy.process_request(json_graphql_request(query, variables.clone()));
    assert_eq!(
        response.body["data"]["staged"],
        json!({ "id": staged_job_id, "currentCount": 0, "done": false })
    );
    assert_eq!(
        response.body["data"]["persisted"],
        json!({ "id": persisted_job_id, "currentCount": 42, "done": true })
    );
    let warm = proxy.process_request(json_graphql_request(query, variables));
    assert_eq!(warm.body["data"], response.body["data"]);
    assert_eq!(upstream_requests.lock().unwrap().len(), 1);

    let dump = proxy.process_request(request_with_body("POST", "/__meta/dump", ""));
    assert_eq!(
        dump.body["state"]["baseState"]["customerSegmentMemberQueries"][persisted_job_id]["done"],
        json!(true)
    );
    assert!(
        dump.body["state"]["stagedState"]["customerSegmentMemberQueries"]
            .get(&staged_job_id)
            .is_some()
    );
}

#[test]
fn segment_prerequisite_hydration_has_one_call_budget_on_a_large_partial_catalog() {
    let target_ids = (0..32)
        .map(|index| format!("gid://shopify/Segment/{index:04}"))
        .collect::<Vec<_>>();
    let expected_target_ids = target_ids.clone();
    let upstream_requests = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_requests = Arc::clone(&upstream_requests);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let body: Value = serde_json::from_str(&request.body).expect("upstream body parses");
            captured_requests.lock().unwrap().push(body.clone());
            assert_eq!(body["operationName"], json!("SegmentPrerequisiteNodes"));
            assert_eq!(body["variables"]["ids"], json!(expected_target_ids));
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "nodes": expected_target_ids
                            .iter()
                            .enumerate()
                            .map(|(index, id)| json!({
                                "id": id,
                                "name": format!("Persisted target {index}"),
                                "query": "number_of_orders >= 1",
                                "creationDate": "2026-07-01T12:00:00Z",
                                "lastEditDate": "2026-07-01T12:00:00Z"
                            }))
                            .collect::<Vec<_>>()
                    }
                }),
            }
        });
    restore_state_with(&mut proxy, |state| {
        let mut segments = serde_json::Map::new();
        let mut order = Vec::new();
        for index in 0..2_000 {
            let id = format!("gid://shopify/Segment/base-{index:04}");
            order.push(json!(id.clone()));
            segments.insert(
                id.clone(),
                json!({
                    "id": id,
                    "name": format!("Partial catalog {index}"),
                    "query": "number_of_orders >= 1",
                    "creationDate": "2026-06-01T12:00:00Z",
                    "lastEditDate": "2026-06-01T12:00:00Z"
                }),
            );
        }
        state["baseState"]["segments"] = Value::Object(segments);
        state["baseState"]["segmentOrder"] = Value::Array(order);
        state["baseState"]["segmentCountBaseline"] =
            json!({ "count": 5_000, "precision": "EXACT" });
        state["baseState"]["segmentCatalogComplete"] = json!(false);
    });

    let definitions = target_ids
        .iter()
        .enumerate()
        .map(|(index, _)| format!("$id{index}: ID!"))
        .collect::<Vec<_>>()
        .join(", ");
    let roots = target_ids
        .iter()
        .enumerate()
        .map(|(index, _)| {
            format!(
                "target{index}: segmentUpdate(id: $id{index}, query: \"number_of_orders >= 2\") {{ segment {{ id }} userErrors {{ field message }} }}"
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let variables = target_ids
        .iter()
        .enumerate()
        .map(|(index, id)| (format!("id{index}"), json!(id)))
        .collect::<serde_json::Map<_, _>>();
    let response = proxy.process_request(json_graphql_request(
        &format!("mutation LargePartialSegmentBatch({definitions}) {{\n{roots}\n}}"),
        Value::Object(variables),
    ));

    assert_eq!(response.status, 200, "{}", response.body);
    for index in 0..target_ids.len() {
        assert_eq!(
            response.body["data"][format!("target{index}")]["userErrors"],
            json!([])
        );
    }
    assert_eq!(
        upstream_requests.lock().unwrap().len(),
        1,
        "submitted Segment IDs must hydrate in one deduplicated nodes request"
    );
    let log = log_snapshot(&proxy);
    assert_eq!(log["entries"].as_array().unwrap().len(), 1);
    assert_eq!(
        log["entries"][0]["stagedResourceIds"]
            .as_array()
            .unwrap()
            .len(),
        target_ids.len(),
        "one submitted mutation document should retain every staged Segment ID"
    );
}

#[test]
fn segment_and_member_job_state_round_trip_and_reset_preserves_authoritative_base() {
    let base_segment_id = "gid://shopify/Segment/base-round-trip";
    let deleted_segment_id = "gid://shopify/Segment/deleted-round-trip";
    let missing_segment_id = "gid://shopify/Segment/missing-round-trip";
    let base_job_id = "gid://shopify/CustomerSegmentMembersQuery/base-round-trip";
    let missing_job_id = "gid://shopify/CustomerSegmentMembersQuery/missing-round-trip";
    let mut proxy = snapshot_proxy();
    restore_state_with(&mut proxy, |state| {
        state["baseState"]["segments"] = json!({
            base_segment_id: {
                "id": base_segment_id,
                "name": "Persisted collision",
                "query": "number_of_orders >= 1",
                "creationDate": "2026-06-01T12:00:00Z",
                "lastEditDate": "2026-06-01T12:00:00Z"
            },
            deleted_segment_id: {
                "id": deleted_segment_id,
                "name": "Delete after restore",
                "query": "number_of_orders >= 2",
                "creationDate": "2026-06-01T12:00:00Z",
                "lastEditDate": "2026-06-01T12:00:00Z"
            }
        });
        state["baseState"]["segmentOrder"] = json!([base_segment_id, deleted_segment_id]);
        state["baseState"]["segmentNameIds"] = json!({
            "Persisted collision": [base_segment_id],
            "Delete after restore": [deleted_segment_id]
        });
        state["baseState"]["segmentCompleteNameProbes"] = json!(["Persisted collision"]);
        state["baseState"]["segmentKnownMissingIds"] = json!([missing_segment_id]);
        state["baseState"]["segmentCountBaseline"] = json!({ "count": 25, "precision": "EXACT" });
        state["baseState"]["segmentCatalogComplete"] = json!(false);
        state["baseState"]["customerSegmentMemberQueries"] = json!({
            base_job_id: { "id": base_job_id, "currentCount": 9, "done": true }
        });
        state["baseState"]["customerSegmentMemberQueryKnownMissingIds"] = json!([missing_job_id]);
    });

    let update = proxy.process_request(json_graphql_request(
        "mutation($id: ID!) { segmentUpdate(id: $id, query: \"number_of_orders >= 3\") { segment { id } userErrors { message } } }",
        json!({ "id": base_segment_id }),
    ));
    assert_eq!(
        update.body["data"]["segmentUpdate"]["userErrors"],
        json!([])
    );
    let delete = proxy.process_request(json_graphql_request(
        "mutation($id: ID!) { segmentDelete(id: $id) { deletedSegmentId userErrors { message } } }",
        json!({ "id": deleted_segment_id }),
    ));
    assert_eq!(
        delete.body["data"]["segmentDelete"]["userErrors"],
        json!([])
    );
    let create = proxy.process_request(json_graphql_request(
        "mutation { segmentCreate(name: \"Persisted collision\", query: \"number_of_orders >= 4\") { segment { id name } userErrors { message } } }",
        json!({}),
    ));
    let staged_segment_id = create.body["data"]["segmentCreate"]["segment"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        create.body["data"]["segmentCreate"]["segment"]["name"],
        json!("Persisted collision (2)")
    );
    let member_job = proxy.process_request(json_graphql_request(
        "mutation { customerSegmentMembersQueryCreate(input: { query: \"number_of_orders >= 1\" }) { customerSegmentMembersQuery { id } userErrors { message } } }",
        json!({}),
    ));
    let staged_job_id = member_job.body["data"]["customerSegmentMembersQueryCreate"]
        ["customerSegmentMembersQuery"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let dump = proxy.process_request(request_with_body("POST", "/__meta/dump", ""));
    let mut restored = snapshot_proxy().with_upstream_transport(|_| {
        panic!("restored Snapshot Segment and job state must not hydrate")
    });
    let restore = restored.process_request(request_with_body(
        "POST",
        "/__meta/restore",
        &dump.body.to_string(),
    ));
    assert_eq!(restore.status, 200, "{}", restore.body);
    let round_trip = restored.process_request(request_with_body("POST", "/__meta/dump", ""));
    for path in [
        "/state/baseState/segments",
        "/state/baseState/segmentNameIds",
        "/state/baseState/segmentCompleteNameProbes",
        "/state/baseState/segmentKnownMissingIds",
        "/state/baseState/segmentCountBaseline",
        "/state/baseState/segmentCatalogComplete",
        "/state/baseState/customerSegmentMemberQueries",
        "/state/baseState/customerSegmentMemberQueryKnownMissingIds",
        "/state/stagedState/segments",
        "/state/stagedState/deletedSegmentIds",
        "/state/stagedState/customerSegmentMemberQueries",
        "/nextSyntheticId",
        "/log",
    ] {
        assert_eq!(
            round_trip.body.pointer(path),
            dump.body.pointer(path),
            "dump/restore changed {path}"
        );
    }

    let read = restored.process_request(json_graphql_request(
        r#"
        query RestoredSegmentAndJobState(
          $updated: ID!
          $created: ID!
          $missingSegment: ID!
          $baseJob: ID!
          $stagedJob: ID!
        ) {
          updated: segment(id: $updated) { id query }
          created: segment(id: $created) { id name }
          missing: segment(id: $missingSegment) { id }
          segmentsCount { count precision }
          baseJob: customerSegmentMembersQuery(id: $baseJob) { id currentCount done }
          stagedJob: customerSegmentMembersQuery(id: $stagedJob) { id currentCount done }
        }
        "#,
        json!({
            "updated": base_segment_id,
            "created": staged_segment_id,
            "missingSegment": missing_segment_id,
            "baseJob": base_job_id,
            "stagedJob": staged_job_id
        }),
    ));
    assert_eq!(
        read.body["data"]["updated"]["query"],
        json!("number_of_orders >= 3")
    );
    assert_eq!(
        read.body["data"]["created"]["name"],
        json!("Persisted collision (2)")
    );
    assert_eq!(read.body["data"]["missing"], Value::Null);
    assert_eq!(
        read.body["data"]["segmentsCount"],
        json!({ "count": 25, "precision": "EXACT" })
    );
    assert_eq!(read.body["data"]["baseJob"]["currentCount"], json!(9));
    assert_eq!(read.body["data"]["stagedJob"]["done"], json!(false));

    let missing_job = restored.process_request(json_graphql_request(
        "query($id: ID!) { customerSegmentMembersQuery(id: $id) { id } }",
        json!({ "id": missing_job_id }),
    ));
    assert_eq!(
        missing_job.body["errors"][0]["extensions"]["code"],
        json!("INTERNAL_SERVER_ERROR")
    );

    let reset = restored.process_request(request_with_body("POST", "/__meta/reset", ""));
    assert_eq!(reset.status, 200);
    let reset_dump = restored.process_request(request_with_body("POST", "/__meta/dump", ""));
    assert_eq!(
        reset_dump.body["state"]["baseState"],
        dump.body["state"]["baseState"]
    );
    assert_eq!(
        reset_dump.body["state"]["stagedState"]["segments"],
        json!({})
    );
    assert_eq!(
        reset_dump.body["state"]["stagedState"]["deletedSegmentIds"],
        json!([])
    );
    assert_eq!(
        reset_dump.body["state"]["stagedState"]["customerSegmentMemberQueries"],
        json!({})
    );
    assert_eq!(reset_dump.body["nextSyntheticId"], json!(1));
    assert_eq!(reset_dump.body["log"], json!({ "entries": [] }));
}

#[test]
fn segment_and_member_job_commit_replays_original_mutations_and_reset_discards_new_work() {
    let replayed = Arc::new(Mutex::new(Vec::<Request>::new()));
    let captured_replayed = Arc::clone(&replayed);
    let mut proxy = snapshot_proxy().with_commit_transport(move |request| {
        captured_replayed.lock().unwrap().push(request);
        Response {
            status: 200,
            headers: Default::default(),
            body: json!({ "data": { "ok": true } }),
        }
    });
    let create_document =
        "mutation CreateForReplay($name: String!) { segmentCreate(name: $name, query: \"number_of_orders >= 1\") { segment { id } userErrors { message } } }";
    let create_variables = json!({ "name": "Replay segment" });
    let create_request = json_graphql_request(create_document, create_variables.clone());
    let create_raw_body = create_request.body.clone();
    let create = proxy.process_request(create_request);
    assert_eq!(
        create.body["data"]["segmentCreate"]["userErrors"],
        json!([])
    );

    let job_document =
        "mutation JobForReplay($input: CustomerSegmentMembersQueryInput!) { customerSegmentMembersQueryCreate(input: $input) { customerSegmentMembersQuery { id } userErrors { message } } }";
    let job_variables = json!({ "input": { "query": "number_of_orders >= 2" } });
    let job_request = json_graphql_request(job_document, job_variables.clone());
    let job_raw_body = job_request.body.clone();
    let job = proxy.process_request(job_request);
    assert_eq!(
        job.body["data"]["customerSegmentMembersQueryCreate"]["userErrors"],
        json!([])
    );
    assert!(replayed.lock().unwrap().is_empty());

    let commit = proxy.process_request(request_with_body("POST", "/__meta/commit", ""));
    assert_eq!(commit.status, 200, "{}", commit.body);
    assert_eq!(commit.body["committed"], json!(2));
    let replayed_requests = replayed.lock().unwrap();
    assert_eq!(replayed_requests.len(), 2);
    assert_eq!(replayed_requests[0].body, create_raw_body);
    assert_eq!(replayed_requests[1].body, job_raw_body);
    assert_eq!(
        serde_json::from_str::<Value>(&replayed_requests[0].body).unwrap()["variables"],
        create_variables
    );
    assert_eq!(
        serde_json::from_str::<Value>(&replayed_requests[1].body).unwrap()["variables"],
        job_variables
    );
    drop(replayed_requests);
    assert!(log_snapshot(&proxy)["entries"]
        .as_array()
        .unwrap()
        .iter()
        .all(|entry| entry["status"] == json!("committed")));

    let discarded = proxy.process_request(json_graphql_request(
        "mutation { segmentCreate(name: \"Discard me\", query: \"number_of_orders >= 3\") { segment { id } userErrors { message } } }",
        json!({}),
    ));
    assert_eq!(
        discarded.body["data"]["segmentCreate"]["userErrors"],
        json!([])
    );
    let reset = proxy.process_request(request_with_body("POST", "/__meta/reset", ""));
    assert_eq!(reset.status, 200);
    let empty_commit = proxy.process_request(request_with_body("POST", "/__meta/commit", ""));
    assert_eq!(empty_commit.body["committed"], json!(0));
    assert_eq!(replayed.lock().unwrap().len(), 2);
}

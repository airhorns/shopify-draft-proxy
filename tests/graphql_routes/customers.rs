use super::common::*;
use pretty_assertions::assert_eq;

fn create_customer(
    proxy: &mut DraftProxy,
    email: &str,
    first_name: &str,
    last_name: &str,
    tags: Vec<String>,
    note: Option<&str>,
) -> String {
    let mut input = json!({
        "email": email,
        "firstName": first_name,
        "lastName": last_name,
        "tags": tags
    });
    if let Some(note) = note {
        input["note"] = json!(note);
    }
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation CustomerCreateParityPlan($input: CustomerInput!) {
          customerCreate(input: $input) {
            customer { id email firstName lastName displayName tags note defaultEmailAddress { emailAddress } }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": input }),
    ));
    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["customerCreate"]["userErrors"],
        json!([])
    );
    response.body["data"]["customerCreate"]["customer"]["id"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn customer_merge_stages_and_downstream_reads_are_operation_name_independent() {
    let mut proxy = snapshot_proxy();
    let source_id = create_customer(
        &mut proxy,
        "merge-source@example.test",
        "Merge",
        "Source",
        vec!["source".to_string()],
        Some("source note"),
    );
    let result_id = create_customer(
        &mut proxy,
        "merge-result@example.test",
        "Merge",
        "Result",
        vec!["result".to_string()],
        None,
    );

    let merge = proxy.process_request(json_graphql_request(
        r#"
        mutation TotallyArbitraryMergeName($one: ID!, $two: ID!, $override: CustomerMergeOverrideFields) {
          customerMerge(customerOneId: $one, customerTwoId: $two, overrideFields: $override) {
            resultingCustomerId
            job { id done }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "one": source_id,
            "two": result_id,
            "override": {
                "customerIdOfEmailToKeep": result_id,
                "customerIdOfFirstNameToKeep": source_id,
                "customerIdOfLastNameToKeep": result_id,
                "note": "merged note",
                "tags": ["merged", "source", "result"]
            }
        }),
    ));
    assert_eq!(merge.status, 200);
    assert_eq!(
        merge.body["data"]["customerMerge"]["resultingCustomerId"],
        json!(result_id)
    );
    assert_eq!(merge.body["data"]["customerMerge"]["userErrors"], json!([]));
    assert_eq!(
        merge.body["data"]["customerMerge"]["job"]["done"],
        json!(false)
    );
    let job_id = merge.body["data"]["customerMerge"]["job"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let downstream = proxy.process_request(json_graphql_request(
        r#"
        query MergeReadAfterWrite($source: ID!, $result: ID!, $sourceEmail: String!, $resultEmail: String!, $job: ID!) {
          source: customer(id: $source) { id email }
          result: customer(id: $result) { id email firstName lastName displayName note tags defaultEmailAddress { emailAddress } }
          bySourceEmail: customerByIdentifier(identifier: { emailAddress: $sourceEmail }) { id email }
          byResultEmail: customerByIdentifier(identifier: { emailAddress: $resultEmail }) { id email defaultEmailAddress { emailAddress } }
          customers(first: 5) { nodes { id email } pageInfo { hasNextPage hasPreviousPage } }
          customersCount { count precision }
          mergeStatus: customerMergeJobStatus(jobId: $job) {
            jobId
            resultingCustomerId
            status
            customerMergeErrors { errorFields message }
          }
          job(id: $job) { id done }
          node(id: $job) { ... on Job { id done } }
        }
        "#,
        json!({
            "source": source_id,
            "result": result_id,
            "sourceEmail": "merge-source@example.test",
            "resultEmail": "merge-result@example.test",
            "job": job_id
        }),
    ));
    assert_eq!(downstream.status, 200);
    assert_eq!(downstream.body["data"]["source"], Value::Null);
    assert_eq!(
        downstream.body["data"]["result"],
        json!({
            "id": result_id,
            "email": "merge-result@example.test",
            "firstName": "Merge",
            "lastName": "Result",
            "displayName": "Merge Result",
            "note": "merged note",
            "tags": ["merged", "result", "source"],
            "defaultEmailAddress": { "emailAddress": "merge-result@example.test" }
        })
    );
    assert_eq!(downstream.body["data"]["bySourceEmail"], Value::Null);
    assert_eq!(
        downstream.body["data"]["byResultEmail"]["id"],
        json!(result_id)
    );
    assert_eq!(
        downstream.body["data"]["customers"]["nodes"],
        json!([{ "id": result_id, "email": "merge-result@example.test" }])
    );
    assert_eq!(
        downstream.body["data"]["customersCount"],
        json!({ "count": 176, "precision": "EXACT" })
    );
    assert_eq!(
        downstream.body["data"]["mergeStatus"],
        json!({
            "jobId": job_id,
            "resultingCustomerId": result_id,
            "status": "COMPLETED",
            "customerMergeErrors": []
        })
    );
    assert_eq!(downstream.body["data"]["job"]["id"], json!(job_id));
    assert_eq!(downstream.body["data"]["job"]["done"], json!(true));
    assert_eq!(downstream.body["data"]["node"]["id"], json!(job_id));
    assert_eq!(downstream.body["data"]["node"]["done"], json!(true));

    let state = proxy.get_state_snapshot();
    assert_eq!(
        state["stagedState"]["mergedCustomerIds"][source_id.as_str()],
        json!(result_id)
    );
    assert_eq!(
        state["stagedState"]["customerMergeRequests"][job_id.as_str()]["resultingCustomerId"],
        json!(result_id)
    );
    assert_eq!(
        state["stagedState"]["deletedCustomerIds"],
        json!([source_id])
    );
    let log = proxy.get_log_snapshot();
    assert_eq!(
        log["entries"][2]["interpreted"]["primaryRootField"],
        json!("customerMerge")
    );
    assert!(log["entries"][2]["rawBody"]
        .as_str()
        .unwrap()
        .contains("TotallyArbitraryMergeName"));
}

#[test]
fn customer_merge_validations_and_blockers_return_shopify_shaped_errors() {
    let mut proxy = snapshot_proxy();
    let first_id = create_customer(
        &mut proxy,
        "merge-validation-one@example.test",
        "Validation",
        "One",
        vec!["one".to_string()],
        None,
    );
    let second_id = create_customer(
        &mut proxy,
        "merge-validation-two@example.test",
        "Validation",
        "Two",
        vec!["two".to_string()],
        None,
    );

    let self_merge = proxy.process_request(json_graphql_request(
        r#"
        mutation ArbitrarySelfMerge($one: ID!, $two: ID!) {
          customerMerge(customerOneId: $one, customerTwoId: $two) {
            resultingCustomerId
            job { id done }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "one": first_id, "two": first_id }),
    ));
    assert_eq!(
        self_merge.body["data"]["customerMerge"],
        json!({
            "resultingCustomerId": null,
            "job": null,
            "userErrors": [{
                "field": null,
                "message": "Customers IDs should not match",
                "code": "INVALID_CUSTOMER_ID"
            }]
        })
    );

    let unknown = proxy.process_request(json_graphql_request(
        r#"
        mutation ArbitraryUnknownMerge($one: ID!, $two: ID!) {
          customerMerge(customerOneId: $one, customerTwoId: $two) {
            resultingCustomerId
            job { id done }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "one": first_id,
            "two": "gid://shopify/Customer/999999999999999"
        }),
    ));
    assert_eq!(
        unknown.body["data"]["customerMerge"]["userErrors"],
        json!([{
            "field": ["customerTwoId"],
            "message": "Customer does not exist with ID 999999999999999",
            "code": "INVALID_CUSTOMER_ID"
        }])
    );

    let missing = proxy.process_request(json_graphql_request(
        r#"
        mutation MissingArgumentNameDoesNotMatter($one: ID!) {
          customerMerge(customerOneId: $one) {
            resultingCustomerId
            userErrors { field message code }
          }
        }
        "#,
        json!({ "one": first_id }),
    ));
    assert_eq!(
        missing.body["errors"][0]["extensions"]["code"],
        json!("missingRequiredArguments")
    );
    assert_eq!(
        missing.body["errors"][0]["extensions"]["arguments"],
        json!("customerTwoId")
    );

    let blank = proxy.process_request(json_graphql_request(
        r#"
        mutation BlankLiteralNameDoesNotMatter {
          customerMerge(customerOneId: "", customerTwoId: "") {
            resultingCustomerId
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(blank.body["errors"].as_array().unwrap().len(), 2);
    assert_eq!(
        blank.body["errors"][0]["extensions"]["code"],
        json!("argumentLiteralsIncompatible")
    );
    assert_eq!(
        blank.body["errors"][1]["extensions"]["code"],
        json!("argumentLiteralsIncompatible")
    );

    let tag_one = create_customer(
        &mut proxy,
        "merge-tags-one@example.test",
        "Tags",
        "One",
        (0..126).map(|index| format!("tag-a-{index}")).collect(),
        None,
    );
    let tag_two = create_customer(
        &mut proxy,
        "merge-tags-two@example.test",
        "Tags",
        "Two",
        (0..126).map(|index| format!("tag-b-{index}")).collect(),
        None,
    );
    let tags_overflow = proxy.process_request(json_graphql_request(
        r#"
        mutation TagsBlockerNameDoesNotMatter($one: ID!, $two: ID!) {
          customerMerge(customerOneId: $one, customerTwoId: $two) {
            resultingCustomerId
            job { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "one": tag_one, "two": tag_two }),
    ));
    assert_eq!(
        tags_overflow.body["data"]["customerMerge"],
        json!({
            "resultingCustomerId": null,
            "job": null,
            "userErrors": [
                {
                    "field": ["customerOneId"],
                    "message": "Customers must have 250 tags or less.",
                    "code": "INVALID_CUSTOMER"
                },
                {
                    "field": ["customerTwoId"],
                    "message": "Customers must have 250 tags or less.",
                    "code": "INVALID_CUSTOMER"
                }
            ]
        })
    );

    let note_one = create_customer(
        &mut proxy,
        "merge-note-one@example.test",
        "Note",
        "One",
        Vec::new(),
        Some(&"a".repeat(2501)),
    );
    let note_two = create_customer(
        &mut proxy,
        "merge-note-two@example.test",
        "Note",
        "Two",
        Vec::new(),
        Some(&"b".repeat(2500)),
    );
    let note_overflow = proxy.process_request(json_graphql_request(
        r#"
        mutation NotesBlockerNameDoesNotMatter($one: ID!, $two: ID!) {
          customerMerge(customerOneId: $one, customerTwoId: $two) {
            resultingCustomerId
            job { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "one": note_one, "two": note_two }),
    ));
    assert_eq!(
        note_overflow.body["data"]["customerMerge"]["userErrors"],
        json!([
            {
                "field": ["customerOneId"],
                "message": "Customer notes must be 5,000 characters or less.",
                "code": "INVALID_CUSTOMER"
            },
            {
                "field": ["customerTwoId"],
                "message": "Customer notes must be 5,000 characters or less.",
                "code": "INVALID_CUSTOMER"
            }
        ])
    );

    assert_eq!(
        proxy.get_state_snapshot()["stagedState"]["mergedCustomerIds"],
        json!({})
    );
    assert_eq!(
        proxy.get_state_snapshot()["stagedState"]["customers"][second_id.as_str()]["email"],
        json!("merge-validation-two@example.test")
    );
}

#[test]
fn customer_data_erasure_request_and_cancel_stage_sensitive_side_effects() {
    let mut proxy = snapshot_proxy();
    let customer_id = create_customer(
        &mut proxy,
        "data-erasure@example.test",
        "Data",
        "Erasure",
        vec!["erasure".to_string()],
        None,
    );

    let request = proxy.process_request(json_graphql_request(
        r#"
        mutation NotTheCapturedRequestName($customerId: ID!) {
          customerRequestDataErasure(customerId: $customerId) {
            customerId
            userErrors { field message code }
          }
        }
        "#,
        json!({ "customerId": customer_id }),
    ));
    assert_eq!(
        request.body["data"]["customerRequestDataErasure"],
        json!({ "customerId": customer_id, "userErrors": [] })
    );
    assert_eq!(
        proxy.get_state_snapshot()["stagedState"]["customerDataErasureRequests"]
            [customer_id.as_str()]["status"],
        json!("REQUESTED")
    );

    let downstream = proxy.process_request(json_graphql_request(
        r#"
        query DataErasureLeavesCustomerReadable($id: ID!) {
          customer(id: $id) { id email tags defaultEmailAddress { emailAddress } }
        }
        "#,
        json!({ "id": customer_id }),
    ));
    assert_eq!(
        downstream.body["data"]["customer"]["email"],
        json!("data-erasure@example.test")
    );

    let cancel = proxy.process_request(json_graphql_request(
        r#"
        mutation AlsoNotTheCapturedCancelName($customerId: ID!) {
          customerCancelDataErasure(customerId: $customerId) {
            customerId
            userErrors { field message code }
          }
        }
        "#,
        json!({ "customerId": customer_id }),
    ));
    assert_eq!(
        cancel.body["data"]["customerCancelDataErasure"],
        json!({ "customerId": customer_id, "userErrors": [] })
    );
    assert_eq!(
        proxy.get_state_snapshot()["stagedState"]["customerDataErasureRequests"]
            [customer_id.as_str()]["status"],
        json!("CANCELED")
    );

    let repeat_cancel = proxy.process_request(json_graphql_request(
        r#"
        mutation RepeatCancel($customerId: ID!) {
          customerCancelDataErasure(customerId: $customerId) {
            customerId
            userErrors { field message code }
          }
        }
        "#,
        json!({ "customerId": customer_id }),
    ));
    assert_eq!(
        repeat_cancel.body["data"]["customerCancelDataErasure"],
        json!({
            "customerId": null,
            "userErrors": [{
                "field": ["customerId"],
                "message": "Customer's data is not scheduled for erasure",
                "code": "NOT_BEING_ERASED"
            }]
        })
    );

    for root in [
        "customerRequestDataErasure",
        "customerCancelDataErasure",
        "customerCancelDataErasure",
    ] {
        assert!(proxy.get_log_snapshot()["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["interpreted"]["primaryRootField"] == json!(root)));
    }
    let log = proxy.get_log_snapshot();
    assert!(log["entries"][1]["rawBody"]
        .as_str()
        .unwrap()
        .contains("NotTheCapturedRequestName"));
    assert!(log["entries"][2]["rawBody"]
        .as_str()
        .unwrap()
        .contains("AlsoNotTheCapturedCancelName"));

    let unknown_request = proxy.process_request(json_graphql_request(
        r#"
        mutation UnknownRequest($customerId: ID!) {
          customerRequestDataErasure(customerId: $customerId) {
            customerId
            userErrors { field message code }
          }
        }
        "#,
        json!({ "customerId": "gid://shopify/Customer/999999999999999" }),
    ));
    assert_eq!(
        unknown_request.body["data"]["customerRequestDataErasure"],
        json!({
            "customerId": null,
            "userErrors": [{
                "field": ["customerId"],
                "message": "Customer does not exist",
                "code": "DOES_NOT_EXIST"
            }]
        })
    );

    let unknown_cancel = proxy.process_request(json_graphql_request(
        r#"
        mutation UnknownCancel($customerId: ID!) {
          customerCancelDataErasure(customerId: $customerId) {
            customerId
            userErrors { field message code }
          }
        }
        "#,
        json!({ "customerId": "gid://shopify/Customer/999999999999999" }),
    ));
    assert_eq!(
        unknown_cancel.body["data"]["customerCancelDataErasure"],
        json!({
            "customerId": null,
            "userErrors": [{
                "field": ["customerId"],
                "message": "Customer does not exist",
                "code": "DOES_NOT_EXIST"
            }]
        })
    );
}

#[test]
fn customer_merge_and_erasure_roots_do_not_write_upstream_in_live_hybrid() {
    let forwarded = Arc::new(Mutex::new(Vec::<Request>::new()));
    let captured = Arc::clone(&forwarded);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            captured.lock().unwrap().push(request);
            Response {
                status: 500,
                headers: Default::default(),
                body: json!({ "errors": [{ "message": "unexpected upstream call" }] }),
            }
        });
    let first_id = create_customer(
        &mut proxy,
        "local-only-one@example.test",
        "Local",
        "One",
        Vec::new(),
        None,
    );
    let second_id = create_customer(
        &mut proxy,
        "local-only-two@example.test",
        "Local",
        "Two",
        Vec::new(),
        None,
    );

    let merge = proxy.process_request(json_graphql_request(
        r#"
        mutation LocalOnlyMerge($one: ID!, $two: ID!) {
          customerMerge(customerOneId: $one, customerTwoId: $two) {
            resultingCustomerId
            job { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "one": first_id, "two": second_id }),
    ));
    assert_eq!(merge.status, 200);
    assert_eq!(merge.body["data"]["customerMerge"]["userErrors"], json!([]));

    let request = proxy.process_request(json_graphql_request(
        r#"
        mutation LocalOnlyErase($customerId: ID!) {
          customerRequestDataErasure(customerId: $customerId) {
            customerId
            userErrors { field message code }
          }
        }
        "#,
        json!({ "customerId": second_id }),
    ));
    assert_eq!(
        request.body["data"]["customerRequestDataErasure"]["userErrors"],
        json!([])
    );

    let cancel = proxy.process_request(json_graphql_request(
        r#"
        mutation LocalOnlyCancel($customerId: ID!) {
          customerCancelDataErasure(customerId: $customerId) {
            customerId
            userErrors { field message code }
          }
        }
        "#,
        json!({ "customerId": second_id }),
    ));
    assert_eq!(
        cancel.body["data"]["customerCancelDataErasure"]["userErrors"],
        json!([])
    );
    assert_eq!(forwarded.lock().unwrap().len(), 0);
}

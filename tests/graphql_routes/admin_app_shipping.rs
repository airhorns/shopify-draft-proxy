use super::common::*;
use pretty_assertions::assert_eq;
use std::collections::BTreeMap;

#[test]
fn bulk_operation_query_status_and_cancel_reads_stage_local_operations() {
    let mut proxy = snapshot_proxy();

    let empty = proxy.process_request(json_graphql_request(
        r#"
        query BulkOperationStatusParityRead($unknownId: ID!, $first: Int, $runningQuery: String, $runningMutation: String) {
          unknown: bulkOperation(id: $unknownId) { id status }
          runningQueries: bulkOperations(first: $first, query: $runningQuery) { edges { cursor node { id } } nodes { id } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } }
          runningMutations: bulkOperations(first: $first, query: $runningMutation) { edges { cursor node { id } } nodes { id } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } }
          currentMutation: currentBulkOperation(type: MUTATION) { id }
        }
        "#,
        json!({
            "unknownId": "gid://shopify/BulkOperation/unknown",
            "first": 5,
            "runningQuery": "status:RUNNING type:QUERY",
            "runningMutation": "status:RUNNING type:MUTATION"
        }),
    ));
    assert_eq!(empty.body["data"]["unknown"], Value::Null);
    assert_eq!(empty.body["data"]["runningQueries"]["nodes"], json!([]));
    assert_eq!(empty.body["data"]["runningQueries"]["edges"], json!([]));
    assert_eq!(
        empty.body["data"]["runningQueries"]["pageInfo"]["hasNextPage"],
        json!(false)
    );
    assert_eq!(empty.body["data"]["currentMutation"], Value::Null);

    let run = proxy.process_request(json_graphql_request(
        r#"
        mutation BulkOperationRunQueryGroupObjectsTrue($query: String!) {
          bulkOperationRunQuery(query: $query, groupObjects: true) {
            bulkOperation { id status type errorCode createdAt completedAt objectCount rootObjectCount fileSize url partialDataUrl query }
            userErrors { field message }
          }
        }
        "#,
        json!({ "query": "#graphql\n{\n  products {\n    edges {\n      node {\n        id\n        title\n      }\n    }\n  }\n}" }),
    ));
    let id = run.body["data"]["bulkOperationRunQuery"]["bulkOperation"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        run.body["data"]["bulkOperationRunQuery"]["userErrors"],
        json!([])
    );
    assert_eq!(
        run.body["data"]["bulkOperationRunQuery"]["bulkOperation"]["status"],
        json!("CREATED")
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query BulkOperationByIdParity($id: ID!) {
          bulkOperation(id: $id) { id status type errorCode createdAt completedAt objectCount rootObjectCount fileSize url partialDataUrl query }
        }
        "#,
        json!({ "id": id }),
    ));
    assert_eq!(
        read.body["data"]["bulkOperation"]["status"],
        json!("COMPLETED")
    );
    assert_eq!(read.body["data"]["bulkOperation"]["type"], json!("QUERY"));
    assert_eq!(
        read.body["data"]["bulkOperation"]["objectCount"],
        json!("1432")
    );

    let cancel = proxy.process_request(json_graphql_request(
        r#"
        mutation BulkOperationCancelParity($id: ID!) {
          bulkOperationCancel(id: $id) {
            bulkOperation { id status type errorCode createdAt completedAt objectCount rootObjectCount fileSize url partialDataUrl query }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": "gid://shopify/BulkOperation/7689772990770" }),
    ));
    assert_eq!(
        cancel.body["data"]["bulkOperationCancel"]["bulkOperation"]["status"],
        json!("CANCELING")
    );
    assert_eq!(
        cancel.body["data"]["bulkOperationCancel"]["userErrors"],
        json!([])
    );
}

#[test]
fn bulk_operation_reads_are_operation_name_independent_and_store_backed() {
    let mut proxy = snapshot_proxy();

    let initial = proxy.process_request(json_graphql_request(
        r#"
        query ConsumerPollBeforeRun {
          currentBulkOperation { id }
          bulkOperations(first: 2) {
            nodes { id status type }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({}),
    ));

    assert_eq!(initial.status, 200);
    assert_eq!(initial.body["data"]["currentBulkOperation"], Value::Null);
    assert_eq!(initial.body["data"]["bulkOperations"]["nodes"], json!([]));

    let run = proxy.process_request(json_graphql_request(
        r#"
        mutation BulkOperationRunQueryParity($query: String!) {
          bulkOperationRunQuery(query: $query, groupObjects: true) {
            bulkOperation { id status type createdAt completedAt }
            userErrors { field message }
          }
        }
        "#,
        json!({ "query": "#graphql\n{\n  products {\n    edges {\n      node {\n        id\n        title\n      }\n    }\n  }\n}" }),
    ));
    let id = run.body["data"]["bulkOperationRunQuery"]["bulkOperation"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let read = proxy.process_request(json_graphql_request(
        r#"
        query ConsumerBulkOperationPoll($id: ID!) {
          byId: bulkOperation(id: $id) { id status type objectCount }
          currentBulkOperation(type: QUERY) { id status type objectCount }
          bulkOperations(first: 1) {
            edges { cursor node { id status type objectCount } }
            nodes { id status type objectCount }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({ "id": id }),
    ));

    assert_eq!(read.status, 200);
    assert_eq!(read.body["errors"], Value::Null);
    assert_eq!(read.body["data"]["byId"]["id"], json!(id));
    assert_eq!(read.body["data"]["byId"]["status"], json!("COMPLETED"));
    assert_eq!(read.body["data"]["currentBulkOperation"]["id"], json!(id));
    assert_eq!(
        read.body["data"]["bulkOperations"]["nodes"][0]["id"],
        json!(id)
    );
    assert_eq!(
        read.body["data"]["bulkOperations"]["edges"][0]["cursor"],
        json!(id)
    );
    assert_eq!(
        read.body["data"]["bulkOperations"]["pageInfo"]["startCursor"],
        json!(id)
    );
}

#[test]
fn bulk_operation_run_query_validates_admin_query_branches() {
    let cases = [
        (
            "nodesInsteadOfEdges",
            "#graphql\n{\n  products {\n    nodes {\n      id\n      title\n    }\n  }\n}",
            json!([
                {
                    "field": ["query"],
                    "message": "All connection fields in a bulk query must select their contents using 'edges' > 'node', e.g: 'products { edges { node {'. Selecting via 'nodes' is not supported. Invalid connection fields: 'products'.",
                    "code": "INVALID"
                }
            ]),
        ),
        (
            "topLevelNode",
            "#graphql\n{\n  node(id: \"gid://shopify/Product/0\") {\n    id\n  }\n}",
            json!([
                {
                    "field": ["query"],
                    "message": "Bulk queries cannot contain a top level `node` field.",
                    "code": "INVALID"
                },
                {
                    "field": ["query"],
                    "message": "Bulk queries must contain at least one connection.",
                    "code": "INVALID"
                }
            ]),
        ),
        (
            "depthThreeNesting",
            "#graphql\n{\n  collections {\n    edges { node { id products { edges { node { id variants { edges { node { id metafields { edges { node { id } } } } } } } } } } }\n  }\n}",
            json!([
                {
                    "field": ["query"],
                    "message": "Bulk queries cannot contain connections with a nesting depth greater than 2.",
                    "code": "INVALID"
                }
            ]),
        ),
        (
            "sixConnections",
            "#graphql\n{\n  products {\n    edges { node { id variants { edges { node { id } } } metafields { edges { node { id } } } collections { edges { node { id } } } media { edges { node { id } } } sellingPlanGroups { edges { node { id } } } } }\n  }\n}",
            json!([
                {
                    "field": ["query"],
                    "message": "Bulk queries cannot contain more than 5 connections.",
                    "code": "INVALID"
                }
            ]),
        ),
        (
            "nestedWithoutParentId",
            "#graphql\n{\n  products {\n    edges { node { title variants { edges { node { id } } } } }\n  }\n}",
            json!([
                {
                    "field": ["query"],
                    "message": "The parent 'node' field for a nested connection must select the 'id' field without an alias and must be of 'ID' return type. Connection fields without 'id': products.",
                    "code": "INVALID"
                }
            ]),
        ),
        (
            "invalidOperationType",
            "#graphql\nmutation {\n  productCreate(input: { title: \"Bulk validator invalid operation type\" }) {\n    product { id }\n  }\n}",
            json!([
                {
                    "field": ["query"],
                    "message": "Invalid operation type. Only `query` operations are supported.",
                    "code": "INVALID"
                }
            ]),
        ),
        (
            "connectionWithinList",
            "#graphql\n{\n  orders {\n    edges { node { id fulfillments { events { edges { node { id } } } } } }\n  }\n}",
            json!([
                {
                    "field": ["query"],
                    "message": "Queries that contain a connection field within a list field are not currently supported.",
                    "code": "INVALID"
                }
            ]),
        ),
        (
            "emptyQuery",
            "",
            json!([
                {
                    "field": ["query"],
                    "message": "Invalid bulk query: syntax error, unexpected end of file",
                    "code": "INVALID"
                }
            ]),
        ),
    ];

    for (name, bulk_query, expected_user_errors) in cases {
        let mut proxy = snapshot_proxy();
        let response = proxy.process_request(json_graphql_request(
            r#"
            mutation BulkOperationRunQueryValidatorParity($query: String!) {
              bulkOperationRunQuery(query: $query) {
                bulkOperation { id status }
                userErrors { field message code }
              }
            }
            "#,
            json!({ "query": bulk_query }),
        ));

        assert_eq!(response.status, 200, "{name}");
        assert_eq!(
            response.body["data"]["bulkOperationRunQuery"]["bulkOperation"],
            Value::Null,
            "{name}"
        );
        assert_eq!(
            response.body["data"]["bulkOperationRunQuery"]["userErrors"], expected_user_errors,
            "{name}"
        );
    }
}

#[test]
fn bulk_operation_run_query_routes_ordinary_operation_names_locally() {
    let mut proxy = snapshot_proxy();
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation RunBulkExport($query: String!) {
          bulkOperationRunQuery(query: $query) {
            bulkOperation { id status type query }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "query": "{ products { edges { node { id } } } }" }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["bulkOperationRunQuery"]["bulkOperation"]["status"],
        json!("CREATED")
    );
    assert_eq!(
        response.body["data"]["bulkOperationRunQuery"]["bulkOperation"]["type"],
        json!("QUERY")
    );
    assert_eq!(
        response.body["data"]["bulkOperationRunQuery"]["userErrors"],
        json!([])
    );
}

#[test]
fn bulk_operation_run_query_throttles_when_query_operation_in_progress() {
    let mut proxy = snapshot_proxy();
    let cancel = proxy.process_request(json_graphql_request(
        r#"
        mutation BulkOperationCancelParity($id: ID!) {
          bulkOperationCancel(id: $id) {
            bulkOperation { id status type }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": "gid://shopify/BulkOperation/7689772990770" }),
    ));
    assert_eq!(
        cancel.body["data"]["bulkOperationCancel"]["bulkOperation"]["status"],
        json!("CANCELING")
    );

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation BulkOperationRunQueryUserErrorCodes($query: String!) {
          bulkOperationRunQuery(query: $query) {
            bulkOperation { id status }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "query": "#graphql\n{\n  products {\n    edges {\n      node {\n        id\n      }\n    }\n  }\n}" }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["bulkOperationRunQuery"]["bulkOperation"],
        Value::Null
    );
    assert_eq!(
        response.body["data"]["bulkOperationRunQuery"]["userErrors"],
        json!([
            {
                "field": null,
                "message": "A bulk query operation for this app and shop is already in progress: gid://shopify/BulkOperation/7689772990770.",
                "code": "OPERATION_IN_PROGRESS"
            }
        ])
    );
}

fn cancel_bulk_operation(proxy: &mut DraftProxy, id: &str, api_version: &str) -> Value {
    let mut cancel_request = json_graphql_request(
        r#"
        mutation BulkOperationCancelParity($id: ID!) {
          bulkOperationCancel(id: $id) {
            bulkOperation { id status type }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": id }),
    );
    cancel_request.path = format!("/admin/api/{api_version}/graphql.json");
    let response = proxy.process_request(cancel_request);
    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["bulkOperationCancel"]["userErrors"],
        json!([])
    );
    response.body["data"]["bulkOperationCancel"]["bulkOperation"].clone()
}

fn run_bulk_operation_query(proxy: &mut DraftProxy, api_version: &str) -> Value {
    let mut request = json_graphql_request(
        r#"
        mutation BulkOperationRunQueryUserErrorCodes($query: String!) {
          bulkOperationRunQuery(query: $query) {
            bulkOperation { id status type }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "query": "{ products { edges { node { id } } } }" }),
    );
    request.path = format!("/admin/api/{api_version}/graphql.json");
    let response = proxy.process_request(request);
    assert_eq!(response.status, 200);
    response.body["data"]["bulkOperationRunQuery"].clone()
}

#[test]
fn bulk_operation_run_query_allows_five_query_operations_before_2026_04_throttle() {
    let mut proxy = snapshot_proxy();
    for index in 0..4 {
        let id = format!("gid://shopify/BulkOperation/990000000000{index}");
        let operation = cancel_bulk_operation(&mut proxy, &id, "2026-04");
        assert_eq!(operation["type"], json!("QUERY"));
        assert_eq!(operation["status"], json!("CANCELING"));

        let allowed = run_bulk_operation_query(&mut proxy, "2026-04");
        assert!(
            allowed["bulkOperation"].is_object(),
            "2026-04 must allow query run while only {} query operations are non-terminal: {allowed}",
            index + 1
        );
        assert_eq!(allowed["userErrors"], json!([]));
    }

    let fifth_id = "gid://shopify/BulkOperation/9900000000004";
    cancel_bulk_operation(&mut proxy, fifth_id, "2026-04");
    let throttled = run_bulk_operation_query(&mut proxy, "2026-04");
    assert_eq!(throttled["bulkOperation"], Value::Null);
    assert_eq!(
        throttled["userErrors"],
        json!([{
            "field": null,
            "message": "A bulk query operation for this app and shop is already in progress: gid://shopify/BulkOperation/9900000000000, gid://shopify/BulkOperation/9900000000001, gid://shopify/BulkOperation/9900000000002, gid://shopify/BulkOperation/9900000000003, gid://shopify/BulkOperation/9900000000004.",
            "code": "OPERATION_IN_PROGRESS"
        }])
    );
}

#[test]
fn bulk_operation_run_mutation_validates_without_dispatcher_errors() {
    let cases = [
        (
            "missingUpload",
            "mutation ProductCreate($product: ProductCreateInput!) { productCreate(product: $product) { product { id title } userErrors { field message } } }",
            "tmp/92891250994/bulk/missing/non-recording.jsonl",
            Value::Null,
            json!([{
                "field": null,
                "message": "The JSONL file could not be found. Try uploading the file again, and check that you've entered the URL correctly for the stagedUploadPath mutation argument.",
                "code": "NO_SUCH_FILE"
            }]),
        ),
        (
            "invalidMutationSyntax",
            "mutation { not parseable",
            "valid",
            Value::Null,
            json!([{
                "field": null,
                "message": "Failed to parse the mutation - syntax error, unexpected end of file",
                "code": "INVALID_MUTATION"
            }]),
        ),
        (
            "queryInsteadOfMutation",
            "query { products { edges { node { id } } } }",
            "valid",
            Value::Null,
            json!([{
                "field": null,
                "message": "Invalid operation type. Only `mutation` operations are supported.",
                "code": "INVALID_MUTATION"
            }]),
        ),
        (
            "multipleRoots",
            "mutation BulkProducts($product: ProductCreateInput!, $update: ProductUpdateInput!) { productCreate(product: $product) { product { id } } productUpdate(product: $update) { product { id } } }",
            "valid",
            Value::Null,
            json!([{
                "field": ["mutation"],
                "message": "You must specify a single top level mutation.",
                "code": null
            }]),
        ),
        (
            "disallowedRoot",
            "mutation Probe($mutation: String!, $stagedUploadPath: String!) { bulkOperationRunMutation(mutation: $mutation, stagedUploadPath: $stagedUploadPath) { bulkOperation { id } userErrors { field message } } }",
            "valid",
            Value::Null,
            json!([{
                "field": ["mutation"],
                "message": "You must use an allowed mutation name.",
                "code": null
            }]),
        ),
    ];

    for (name, inner_mutation, staged_upload_path, expected_operation, expected_errors) in cases {
        let mut proxy = snapshot_proxy();
        let response = proxy.process_request(json_graphql_request(
            r#"
            mutation RunBulkImport($mutation: String!, $path: String!) {
              bulkOperationRunMutation(mutation: $mutation, stagedUploadPath: $path) {
                bulkOperation { id status type }
                userErrors { field message code }
              }
            }
            "#,
            json!({ "mutation": inner_mutation, "path": staged_upload_path }),
        ));

        assert_eq!(response.status, 200, "{name}");
        assert_eq!(
            response.body["data"]["bulkOperationRunMutation"]["bulkOperation"], expected_operation,
            "{name}"
        );
        assert_eq!(
            response.body["data"]["bulkOperationRunMutation"]["userErrors"], expected_errors,
            "{name}"
        );
    }
}

fn staged_bulk_mutation_upload_path(
    proxy: &mut DraftProxy,
    filename: &str,
    file_size: &str,
) -> String {
    let staged = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateBulkUpload($input: [StagedUploadInput!]!) {
          stagedUploadsCreate(input: $input) {
            stagedTargets { parameters { name value } }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": [{
                "resource": "BULK_MUTATION_VARIABLES",
                "filename": filename,
                "mimeType": "text/jsonl",
                "httpMethod": "POST",
                "fileSize": file_size
            }]
        }),
    ));
    assert_eq!(staged.status, 200);
    assert_eq!(
        staged.body["data"]["stagedUploadsCreate"]["userErrors"],
        json!([])
    );
    staged.body["data"]["stagedUploadsCreate"]["stagedTargets"][0]["parameters"]
        .as_array()
        .unwrap()
        .iter()
        .find(|parameter| parameter["name"] == "key")
        .and_then(|parameter| parameter["value"].as_str())
        .unwrap()
        .to_string()
}

#[test]
fn bulk_operation_run_mutation_stages_created_status_from_staged_upload() {
    let mut proxy = snapshot_proxy();
    let staged = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateBulkUpload($input: [StagedUploadInput!]!) {
          stagedUploadsCreate(input: $input) {
            stagedTargets {
              resourceUrl
              parameters { name value }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": [{
                "resource": "BULK_MUTATION_VARIABLES",
                "filename": "ordinary-import.jsonl",
                "mimeType": "text/jsonl",
                "httpMethod": "POST",
                "fileSize": "42"
            }]
        }),
    ));
    assert_eq!(staged.status, 200);
    let path = staged.body["data"]["stagedUploadsCreate"]["stagedTargets"][0]["parameters"]
        .as_array()
        .unwrap()
        .iter()
        .find(|parameter| parameter["name"] == "key")
        .and_then(|parameter| parameter["value"].as_str())
        .unwrap()
        .to_string();

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation RunBulkImport($mutation: String!, $path: String!) {
          bulkOperationRunMutation(mutation: $mutation, stagedUploadPath: $path) {
            bulkOperation {
              id
              status
              type
              errorCode
              createdAt
              completedAt
              objectCount
              rootObjectCount
              fileSize
              url
              partialDataUrl
              query
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "mutation": "mutation CustomerCreate($input: CustomerInput!) { customerCreate(input: $input) { customer { id email } userErrors { field message } } }",
            "path": path
        }),
    ));

    assert_eq!(response.status, 200);
    let operation = &response.body["data"]["bulkOperationRunMutation"]["bulkOperation"];
    assert!(operation["id"]
        .as_str()
        .unwrap()
        .starts_with("gid://shopify/BulkOperation/"));
    assert_eq!(operation["status"], json!("CREATED"));
    assert_eq!(operation["type"], json!("MUTATION"));
    assert_eq!(operation["completedAt"], Value::Null);
    assert_eq!(operation["objectCount"], json!("0"));
    assert_eq!(operation["rootObjectCount"], json!("0"));
    assert_eq!(operation["fileSize"], Value::Null);
    assert_eq!(operation["url"], Value::Null);
    assert_eq!(
        response.body["data"]["bulkOperationRunMutation"]["userErrors"],
        json!([])
    );
}

#[test]
fn bulk_operation_run_mutation_rejects_oversized_staged_upload_with_shopify_error_shape() {
    let mut proxy = snapshot_proxy();
    let path = staged_bulk_mutation_upload_path(&mut proxy, "oversized-import.jsonl", "104857601");
    let log_before = proxy.get_log_snapshot();

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation RunBulkImport($mutation: String!, $path: String!) {
          bulkOperationRunMutation(mutation: $mutation, stagedUploadPath: $path) {
            bulkOperation { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "mutation": "mutation ProductCreate($product: ProductCreateInput!) { productCreate(product: $product) { product { id } userErrors { field message } } }",
            "path": path
        }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["bulkOperationRunMutation"]["bulkOperation"],
        Value::Null
    );
    assert_eq!(
        response.body["data"]["bulkOperationRunMutation"]["userErrors"],
        json!([{
            "field": null,
            "message": "The input file size exceeds the maximum allowed size of 100 MB.",
            "code": "INVALID_STAGED_UPLOAD_FILE"
        }])
    );
    assert_eq!(proxy.get_log_snapshot(), log_before);

    let current = proxy.process_request(json_graphql_request(
        r#"
        query CurrentMutation {
          currentBulkOperation(type: MUTATION) { id }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        current.body["data"]["currentBulkOperation"],
        Value::Null,
        "oversized validation must not stage a mutation bulk operation"
    );
}

#[test]
fn bulk_operation_run_mutation_validates_client_identifier_and_configured_file_size() {
    let max_bytes = 2 * 1024 * 1024;
    let mut proxy =
        configured_proxy_with_bulk_mutation_max(ReadMode::Snapshot, None, Some(max_bytes));
    let path = staged_bulk_mutation_upload_path(
        &mut proxy,
        "configured-oversized-import.jsonl",
        &(max_bytes + 1).to_string(),
    );
    let mutation =
        "mutation ProductCreate($product: ProductCreateInput!) { productCreate(product: $product) { product { id } userErrors { field message } } }";

    let too_short = proxy.process_request(json_graphql_request(
        r#"
        mutation RunBulkImport($mutation: String!, $path: String!, $clientIdentifier: String) {
          bulkOperationRunMutation(mutation: $mutation, stagedUploadPath: $path, clientIdentifier: $clientIdentifier) {
            bulkOperation { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "mutation": mutation, "path": "valid", "clientIdentifier": "abc" }),
    ));
    assert_eq!(
        too_short.body["data"]["bulkOperationRunMutation"]["userErrors"],
        json!([{
            "field": ["clientIdentifier"],
            "message": "is too short (minimum is 10 characters)",
            "code": "INVALID_MUTATION"
        }])
    );

    let oversized = proxy.process_request(json_graphql_request(
        r#"
        mutation RunBulkImport($mutation: String!, $path: String!) {
          bulkOperationRunMutation(mutation: $mutation, stagedUploadPath: $path) {
            bulkOperation { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "mutation": mutation, "path": path }),
    ));
    assert_eq!(
        oversized.body["data"]["bulkOperationRunMutation"]["bulkOperation"],
        Value::Null
    );
    assert_eq!(
        oversized.body["data"]["bulkOperationRunMutation"]["userErrors"],
        json!([{
            "field": null,
            "message": "The input file size exceeds the maximum allowed size of 2 MB.",
            "code": "INVALID_STAGED_UPLOAD_FILE"
        }])
    );
}

#[test]
fn bulk_operation_run_mutation_file_size_error_precedes_in_progress_throttle() {
    let max_bytes = 2 * 1024 * 1024;
    let mut proxy =
        configured_proxy_with_bulk_mutation_max(ReadMode::Snapshot, None, Some(max_bytes));
    let path = staged_bulk_mutation_upload_path(
        &mut proxy,
        "oversized-import-with-running-mutation.jsonl",
        &(max_bytes + 1).to_string(),
    );
    let mut cancel_request = json_graphql_request(
        r#"
        mutation CancelCapturedMutation($id: ID!) {
          bulkOperationCancel(id: $id) {
            bulkOperation { id status type }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": "gid://shopify/BulkOperation/7749099127090" }),
    );
    cancel_request.path = "/admin/api/2025-01/graphql.json".to_string();
    let cancel = proxy.process_request(cancel_request);
    assert_eq!(
        cancel.body["data"]["bulkOperationCancel"]["bulkOperation"]["type"],
        json!("MUTATION")
    );
    assert_eq!(
        cancel.body["data"]["bulkOperationCancel"]["bulkOperation"]["status"],
        json!("CANCELING")
    );
    let log_before = proxy.get_log_snapshot();

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation RunBulkImport($mutation: String!, $path: String!) {
          bulkOperationRunMutation(mutation: $mutation, stagedUploadPath: $path) {
            bulkOperation { id status type }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "mutation": "mutation ProductCreate($product: ProductCreateInput!) { productCreate(product: $product) { product { id } userErrors { field message } } }",
            "path": path
        }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["bulkOperationRunMutation"]["bulkOperation"],
        Value::Null
    );
    assert_eq!(
        response.body["data"]["bulkOperationRunMutation"]["userErrors"],
        json!([{
            "field": null,
            "message": "The input file size exceeds the maximum allowed size of 2 MB.",
            "code": "INVALID_STAGED_UPLOAD_FILE"
        }])
    );
    assert_eq!(
        proxy.get_log_snapshot(),
        log_before,
        "oversized validation must not append a bulk mutation log entry"
    );
}

#[test]
fn bulk_operation_run_mutation_throttles_when_mutation_operation_in_progress() {
    let mut proxy = snapshot_proxy();
    let mut cancel_request = json_graphql_request(
        r#"
        mutation CancelCapturedMutation($id: ID!) {
          bulkOperationCancel(id: $id) {
            bulkOperation { id status type }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": "gid://shopify/BulkOperation/7749099127090" }),
    );
    cancel_request.path = "/admin/api/2025-01/graphql.json".to_string();
    let cancel = proxy.process_request(cancel_request);
    assert_eq!(
        cancel.body["data"]["bulkOperationCancel"]["bulkOperation"]["type"],
        json!("MUTATION")
    );
    assert_eq!(
        cancel.body["data"]["bulkOperationCancel"]["bulkOperation"]["status"],
        json!("CANCELING")
    );

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation RunBulkImport($mutation: String!, $path: String!) {
          bulkOperationRunMutation(mutation: $mutation, stagedUploadPath: $path) {
            bulkOperation { id status type }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "mutation": "mutation ProductCreate($product: ProductCreateInput!) { productCreate(product: $product) { product { id } userErrors { field message } } }",
            "path": "valid"
        }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["bulkOperationRunMutation"]["bulkOperation"],
        Value::Null
    );
    assert_eq!(
        response.body["data"]["bulkOperationRunMutation"]["userErrors"],
        json!([{
            "field": null,
            "message": "A bulk mutation operation for this app and shop is already in progress: gid://shopify/BulkOperation/7749099127090.",
            "code": "OPERATION_IN_PROGRESS"
        }])
    );
}

fn mutation_bulk_operation_fixture(id: &str) -> Value {
    json!({
        "id": id,
        "status": "CREATED",
        "type": "MUTATION",
        "errorCode": null,
        "createdAt": "2026-06-01T00:00:00Z",
        "completedAt": null,
        "objectCount": "0",
        "rootObjectCount": "0",
        "fileSize": null,
        "url": null,
        "partialDataUrl": null,
        "query": "#graphql\nmutation ProductCreate($product: ProductCreateInput!) { productCreate(product: $product) { product { id } userErrors { field message } } }"
    })
}

fn live_hybrid_proxy_with_bulk_operation_hydration(
    operations: BTreeMap<String, Value>,
) -> DraftProxy {
    configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
        let body = serde_json::from_str::<Value>(&request.body).unwrap_or(Value::Null);
        let id = body
            .get("variables")
            .and_then(|variables| variables.get("id"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        shopify_draft_proxy::proxy::Response {
            status: 200,
            headers: Default::default(),
            body: json!({
                "data": {
                    "bulkOperation": operations.get(id).cloned().unwrap_or(Value::Null)
                }
            }),
        }
    })
}

fn run_bulk_operation_mutation(proxy: &mut DraftProxy, api_version: &str) -> Value {
    let mut request = json_graphql_request(
        r#"
        mutation RunBulkImport($mutation: String!, $path: String!) {
          bulkOperationRunMutation(mutation: $mutation, stagedUploadPath: $path) {
            bulkOperation { id status type }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "mutation": "mutation ProductCreate($product: ProductCreateInput!) { productCreate(product: $product) { product { id } userErrors { field message } } }",
            "path": "valid"
        }),
    );
    request.path = format!("/admin/api/{api_version}/graphql.json");
    let response = proxy.process_request(request);
    assert_eq!(response.status, 200);
    response.body["data"]["bulkOperationRunMutation"].clone()
}

#[test]
fn bulk_operation_run_mutation_allows_five_mutation_operations_before_2026_04_throttle() {
    let hydrated_operations = (0..5)
        .map(|index| {
            let id = format!("gid://shopify/BulkOperation/991000000000{index}");
            (id.clone(), mutation_bulk_operation_fixture(&id))
        })
        .collect::<BTreeMap<_, _>>();
    let mut proxy = live_hybrid_proxy_with_bulk_operation_hydration(hydrated_operations);

    for index in 0..4 {
        let id = format!("gid://shopify/BulkOperation/991000000000{index}");
        let operation = cancel_bulk_operation(&mut proxy, &id, "2026-04");
        assert_eq!(operation["type"], json!("MUTATION"));
        assert_eq!(operation["status"], json!("CANCELING"));

        let allowed = run_bulk_operation_mutation(&mut proxy, "2026-04");
        assert!(
            allowed["bulkOperation"].is_object(),
            "2026-04 must allow mutation run while only {} mutation operations are non-terminal: {allowed}",
            index + 1
        );
        assert_eq!(allowed["userErrors"], json!([]));
    }

    let fifth_id = "gid://shopify/BulkOperation/9910000000004";
    cancel_bulk_operation(&mut proxy, fifth_id, "2026-04");
    let throttled = run_bulk_operation_mutation(&mut proxy, "2026-04");
    assert_eq!(throttled["bulkOperation"], Value::Null);
    assert_eq!(
        throttled["userErrors"],
        json!([{
            "field": null,
            "message": "A bulk mutation operation for this app and shop is already in progress: gid://shopify/BulkOperation/9910000000000, gid://shopify/BulkOperation/9910000000001, gid://shopify/BulkOperation/9910000000002, gid://shopify/BulkOperation/9910000000003, gid://shopify/BulkOperation/9910000000004.",
            "code": "OPERATION_IN_PROGRESS"
        }])
    );
}

#[test]
fn bulk_operation_cancel_unknown_gid_returns_not_found_without_staging() {
    let mut proxy = snapshot_proxy();
    let id = "gid://shopify/BulkOperation/9999999999999";
    let cancel = proxy.process_request(json_graphql_request(
        r#"
        mutation BulkOperationCancelParity($id: ID!) {
          bulkOperationCancel(id: $id) {
            bulkOperation { id status type createdAt query }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": id }),
    ));

    assert_eq!(cancel.status, 200);
    assert_eq!(
        cancel.body["data"]["bulkOperationCancel"]["bulkOperation"],
        Value::Null
    );
    assert_eq!(
        cancel.body["data"]["bulkOperationCancel"]["userErrors"],
        json!([{ "field": ["id"], "message": "Bulk operation does not exist" }])
    );

    let mut run_request = json_graphql_request(
        r#"
        mutation BulkOperationRunQueryUserErrorCodes($query: String!) {
          bulkOperationRunQuery(query: $query) {
            bulkOperation { id status }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "query": "{ products { edges { node { id } } } }" }),
    );
    run_request.path = "/admin/api/2025-01/graphql.json".to_string();
    let response = proxy.process_request(run_request);

    assert_eq!(
        response.body["data"]["bulkOperationRunQuery"]["userErrors"],
        json!([])
    );
    let missing_read = proxy.process_request(json_graphql_request(
        r#"
        query MissingBulkOperation($id: ID!) {
          bulkOperation(id: $id) { id status }
        }
        "#,
        json!({ "id": id }),
    ));
    assert_eq!(missing_read.body["data"]["bulkOperation"], Value::Null);
    let log = proxy.get_log_snapshot();
    assert_eq!(log["entries"].as_array().unwrap().len(), 1);
    assert_eq!(log["entries"][0]["operationName"], Value::Null);
    assert_eq!(
        log["entries"][0]["interpreted"]["primaryRootField"],
        json!("bulkOperationRunQuery")
    );
}

#[test]
fn bulk_operation_cancel_completed_staged_operation_echoes_terminal_without_mutation() {
    let mut proxy = snapshot_proxy();
    let run = proxy.process_request(json_graphql_request(
        r#"
        mutation RunCompleted($query: String!) {
          bulkOperationRunQuery(query: $query) {
            bulkOperation { id status }
            userErrors { field message }
          }
        }
        "#,
        json!({ "query": "{ products { edges { node { id } } } }" }),
    ));
    let id = run.body["data"]["bulkOperationRunQuery"]["bulkOperation"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let cancel = proxy.process_request(json_graphql_request(
        r#"
        mutation CancelCompleted($id: ID!) {
          bulkOperationCancel(id: $id) {
            bulkOperation { id status type }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": id }),
    ));

    assert_eq!(
        cancel.body["data"]["bulkOperationCancel"],
        json!({
            "bulkOperation": {
                "id": id,
                "status": "COMPLETED",
                "type": "QUERY"
            },
            "userErrors": [{
                "field": null,
                "message": "A bulk operation cannot be canceled when it is completed"
            }]
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query ReadAfterCancelCompleted($id: ID!) {
          bulkOperation(id: $id) { id status }
          currentBulkOperation(type: QUERY) { id status }
        }
        "#,
        json!({ "id": id }),
    ));
    assert_eq!(
        read.body["data"]["bulkOperation"]["status"],
        json!("COMPLETED")
    );
    assert_eq!(
        read.body["data"]["currentBulkOperation"]["status"],
        json!("COMPLETED")
    );
    let log = proxy.get_log_snapshot();
    assert_eq!(log["entries"].as_array().unwrap().len(), 1);
    assert_eq!(
        log["entries"][0]["interpreted"]["primaryRootField"],
        json!("bulkOperationRunQuery")
    );
}

#[test]
fn bulk_operation_cancel_terminal_hydrated_operation_echoes_existing_record() {
    let id = "gid://shopify/BulkOperation/7689772204338";
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport({
        let operation = bulk_operation_test_record(
            id,
            "FAILED",
            "QUERY",
            "2026-04-27T20:34:58Z",
            "#graphql\n{\n  products {\n    edges {\n      node {\n        id\n        title\n      }\n    }\n  }\n}",
        );
        move |_request| bulk_operation_hydrate_response(operation.clone())
    });

    let cancel = proxy.process_request(json_graphql_request(
        r#"
        mutation CancelFailed($id: ID!) {
          bulkOperationCancel(id: $id) {
            bulkOperation { id status type }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": id }),
    ));

    assert_eq!(
        cancel.body["data"]["bulkOperationCancel"],
        json!({
            "bulkOperation": {
                "id": id,
                "status": "FAILED",
                "type": "QUERY"
            },
            "userErrors": [{
                "field": null,
                "message": "A bulk operation cannot be canceled when it is failed"
            }]
        })
    );
    assert_eq!(
        proxy.get_log_snapshot()["entries"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
}

#[test]
fn bulk_operation_list_filters_paginates_and_selects_current_by_type() {
    let mut proxy = snapshot_proxy();
    let older_id = "gid://shopify/BulkOperation/9999999999999";
    let run = proxy.process_request(json_graphql_request(
        r#"
        mutation BulkOperationRunQueryGroupObjectsTrue($query: String!) {
          bulkOperationRunQuery(query: $query, groupObjects: true) {
            bulkOperation { id status type createdAt }
            userErrors { field message }
          }
        }
        "#,
        json!({ "query": "#graphql\n{\n  products {\n    edges {\n      node {\n        id\n      }\n    }\n  }\n}" }),
    ));
    let query_id = run.body["data"]["bulkOperationRunQuery"]["bulkOperation"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let cancel = proxy.process_request(json_graphql_request(
        r#"
        mutation AnyCancelName($id: ID!) {
          bulkOperationCancel(id: $id) {
            bulkOperation { id status type createdAt }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": older_id }),
    ));
    assert_eq!(cancel.status, 200);

    let read = proxy.process_request(json_graphql_request(
        r#"
        query MixedBulkOperations($after: String!) {
          defaultCurrent: currentBulkOperation { id type status }
          queryOnly: bulkOperations(first: 5, query: "operation_type:QUERY") { nodes { id type } }
          cancelingQueries: bulkOperations(first: 5, query: "status:CANCELING operation_type:QUERY") { nodes { id type status } }
          firstPage: bulkOperations(first: 1, sortKey: CREATED_AT) {
            nodes { id type }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          secondPage: bulkOperations(first: 1, after: $after, sortKey: CREATED_AT) {
            nodes { id type }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          reversePage: bulkOperations(first: 1, reverse: true, sortKey: CREATED_AT) {
            nodes { id type }
          }
          lastPage: bulkOperations(last: 1, sortKey: CREATED_AT) {
            nodes { id type }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({ "after": query_id }),
    ));

    assert_eq!(read.status, 200);
    assert_eq!(read.body["data"]["defaultCurrent"]["id"], json!(query_id));
    assert_eq!(read.body["data"]["defaultCurrent"]["type"], json!("QUERY"));
    assert_eq!(
        read.body["data"]["queryOnly"]["nodes"],
        json!([
            { "id": query_id, "type": "QUERY" },
            { "id": older_id, "type": "QUERY" }
        ])
    );
    assert_eq!(
        read.body["data"]["cancelingQueries"]["nodes"],
        json!([{ "id": older_id, "type": "QUERY", "status": "CANCELING" }])
    );
    assert_eq!(
        read.body["data"]["firstPage"]["nodes"][0]["id"],
        json!(query_id)
    );
    assert_eq!(
        read.body["data"]["firstPage"]["pageInfo"],
        json!({
            "hasNextPage": true,
            "hasPreviousPage": false,
            "startCursor": query_id,
            "endCursor": query_id
        })
    );
    assert_eq!(
        read.body["data"]["secondPage"]["nodes"][0]["id"],
        json!(older_id)
    );
    assert_eq!(
        read.body["data"]["secondPage"]["pageInfo"]["hasPreviousPage"],
        json!(true)
    );
    assert_eq!(
        read.body["data"]["reversePage"]["nodes"][0]["id"],
        json!(older_id)
    );
    assert_eq!(
        read.body["data"]["lastPage"]["nodes"][0]["id"],
        json!(older_id)
    );
}

#[test]
fn bulk_operation_reads_validate_ids_windows_sort_keys_and_search_warnings() {
    let mut proxy = snapshot_proxy();

    let malformed = proxy.process_request(json_graphql_request(
        r#"
        query NotARecordedOperation {
          bulkOperation(id: "not-a-gid") { id }
        }
        "#,
        json!({}),
    ));
    assert_eq!(malformed.status, 200);
    assert_eq!(
        malformed.body["errors"][0]["message"],
        json!("Invalid global id 'not-a-gid'")
    );
    assert_eq!(
        malformed.body["errors"][0]["extensions"]["code"],
        json!("argumentLiteralsIncompatible")
    );

    let non_bulk_gid = proxy.process_request(json_graphql_request(
        r#"
        query NotARecordedOperation {
          bulkOperation(id: "gid://shopify/Product/1") { id }
        }
        "#,
        json!({}),
    ));
    assert_eq!(non_bulk_gid.status, 200);
    assert_eq!(
        non_bulk_gid.body["errors"][0]["message"],
        json!("Invalid id: gid://shopify/Product/1")
    );
    assert_eq!(non_bulk_gid.body["data"]["bulkOperation"], Value::Null);

    let missing_window = proxy.process_request(json_graphql_request(
        r#"
        query NotARecordedOperation {
          bulkOperations { nodes { id } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        missing_window.body["errors"][0]["message"],
        json!("you must provide one of first or last")
    );
    assert_eq!(missing_window.body["data"], Value::Null);

    let first_and_last = proxy.process_request(json_graphql_request(
        r#"
        query NotARecordedOperation {
          bulkOperations(first: 1, last: 1) { nodes { id } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        first_and_last.body["errors"][0]["message"],
        json!("providing both first and last is not supported")
    );

    let id_sort = proxy.process_request(json_graphql_request(
        r#"
        query NotARecordedOperation {
          bulkOperations(first: 1, sortKey: ID) { nodes { id } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        id_sort.body["errors"][0]["extensions"]["argumentName"],
        json!("sortKey")
    );

    let invalid_created_at = proxy.process_request(json_graphql_request(
        r#"
        query NotARecordedOperation {
          bulkOperations(first: 1, query: "created_at:not-a-date") { nodes { id } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        invalid_created_at.body["errors"][0]["message"],
        json!("Invalid timestamp for query filter `created_at`.")
    );

    let invalid_status = proxy.process_request(json_graphql_request(
        r#"
        query NotARecordedOperation {
          bulkOperations(first: 1, query: "status:EXPIRED") { nodes { id } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        invalid_status.body["data"]["bulkOperations"]["nodes"],
        json!([])
    );
    assert_eq!(
        invalid_status.body["extensions"]["search"][0]["warnings"][0]["code"],
        json!("invalid_value")
    );
}

#[test]
fn bulk_operation_empty_connection_preserves_selection_aliases() {
    let mut proxy = snapshot_proxy();

    let response = proxy.process_request(json_graphql_request(
        r#"
        query BulkOperationStatusParityRead {
          ops: bulkOperations(first: 5) {
            aliasedNodes: nodes { id }
            aliasedEdges: edges { cursor node { id } }
            info: pageInfo {
              next: hasNextPage
              previous: hasPreviousPage
              start: startCursor
              end: endCursor
            }
          }
        }
        "#,
        json!({}),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body,
        json!({
            "data": {
                "ops": {
                    "aliasedNodes": [],
                    "aliasedEdges": [],
                    "info": {
                        "next": false,
                        "previous": false,
                        "start": null,
                        "end": null
                    }
                }
            }
        })
    );
}

#[test]
fn bulk_operation_cold_live_hybrid_sort_key_reads_fall_back_to_upstream_transport() {
    let forwarded = Arc::new(Mutex::new(Vec::<Request>::new()));
    let captured = Arc::clone(&forwarded);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            captured.lock().unwrap().push(request);
            shopify_draft_proxy::proxy::Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": { "bulkOperations": { "nodes": [{ "id": "gid://shopify/BulkOperation/upstream" }] } }
                }),
            }
        });

    let response = proxy.process_request(json_graphql_request(
        r#"
        query BulkOperationsSortKeyCapture {
          bulkOperations(first: 5, sortKey: COMPLETED_AT) { nodes { id } }
        }
        "#,
        json!({}),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["bulkOperations"]["nodes"][0]["id"],
        json!("gid://shopify/BulkOperation/upstream")
    );
    assert_eq!(forwarded.lock().unwrap().len(), 1);
}

#[test]
fn customer_create_stages_record_for_downstream_customer_reads_and_counts() {
    let mut proxy = snapshot_proxy();
    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CustomerCreateParityPlan($input: CustomerInput!) {
          customerCreate(input: $input) {
            customer {
              id firstName lastName displayName email locale note verifiedEmail taxExempt taxExemptions tags state canDelete
              loyalty: metafield(namespace: "custom", key: "loyalty") { id namespace key type value }
              metafields(first: 5) { nodes { id namespace key type value } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } }
              defaultEmailAddress { emailAddress }
              defaultPhoneNumber { phoneNumber }
              defaultAddress { address1 city province country zip formattedArea }
              createdAt updatedAt
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "email": "hermes-customer-create@example.com",
                "firstName": "Hermes",
                "lastName": "Create",
                "locale": "en",
                "note": "customer create parity probe",
                "phone": "+14155550123",
                "tags": ["parity", "create"],
                "taxExempt": true
            }
        }),
    ));
    let customer = &create.body["data"]["customerCreate"]["customer"];
    let id = customer["id"].as_str().unwrap();
    assert!(id.starts_with("gid://shopify/Customer/"));
    assert_eq!(customer["displayName"], json!("Hermes Create"));
    assert_eq!(customer["tags"], json!(["create", "parity"]));
    assert_eq!(
        create.body["data"]["customerCreate"]["userErrors"],
        json!([])
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query CustomerMutationDownstream($id: ID!, $query: String!, $first: Int!) {
          customer(id: $id) { id firstName lastName displayName email tags defaultEmailAddress { emailAddress } defaultPhoneNumber { phoneNumber } }
          customers(first: $first, query: $query, sortKey: UPDATED_AT, reverse: true) { nodes { id email } pageInfo { hasNextPage hasPreviousPage } }
          customersCount { count precision }
        }
        "#,
        json!({ "id": id, "query": "__customer_parity_no_match__", "first": 5 }),
    ));
    assert_eq!(read.body["data"]["customer"]["id"], json!(id));
    assert_eq!(
        read.body["data"]["customer"]["email"],
        json!("hermes-customer-create@example.com")
    );
    assert_eq!(
        read.body["data"]["customers"],
        json!({
            "nodes": [],
            "pageInfo": { "hasNextPage": false, "hasPreviousPage": false }
        })
    );
    assert_eq!(
        read.body["data"]["customersCount"],
        json!({ "count": 177, "precision": "EXACT" })
    );
}

#[test]
fn customer_mutations_are_operation_name_independent_and_store_backed() {
    let mut proxy = configured_proxy(
        ReadMode::Snapshot,
        Some(shopify_draft_proxy::proxy::UnsupportedMutationMode::Reject),
    );

    let invalid = proxy.process_request(json_graphql_request(
        r#"
        mutation MakeCustomer($input: CustomerInput!) {
          made: customerCreate(input: $input) {
            customer { id email }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "firstName": "Alice", "email": "not-an-email" } }),
    ));
    assert_eq!(invalid.status, 200);
    assert_eq!(invalid.body["errors"], Value::Null);
    assert_eq!(invalid.body["data"]["made"]["customer"], Value::Null);
    assert_eq!(
        invalid.body["data"]["made"]["userErrors"],
        json!([{ "field": ["email"], "message": "Email is invalid" }])
    );

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation ConsumerCreateCustomer($input: CustomerInput!) {
          customerCreate(input: $input) {
            customer { id firstName lastName displayName email phone locale verifiedEmail tags defaultEmailAddress { emailAddress } defaultPhoneNumber { phoneNumber } }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "firstName": "Alice",
                "lastName": "Buyer",
                "email": "Alice@Example.COM",
                "phone": "+1 (613) 450-5293",
                "tags": ["Retail, VIP", "vip"]
            }
        }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(create.body["errors"], Value::Null);
    assert_eq!(
        create.body["data"]["customerCreate"]["userErrors"],
        json!([])
    );
    let customer = &create.body["data"]["customerCreate"]["customer"];
    let id = customer["id"].as_str().unwrap();
    assert!(id.starts_with("gid://shopify/Customer/"));
    assert_eq!(customer["email"], json!("alice@example.com"));
    assert_eq!(customer["phone"], json!("+16134505293"));
    assert_eq!(customer["locale"], json!("en"));
    assert_eq!(customer["verifiedEmail"], json!(false));
    assert_eq!(customer["tags"], json!(["Retail", "VIP"]));

    let mut versioned_proxy = configured_proxy(
        ReadMode::Snapshot,
        Some(shopify_draft_proxy::proxy::UnsupportedMutationMode::Reject),
    );
    let mut versioned_create = json_graphql_request(
        r#"
        mutation VersionedCustomerCreate($input: CustomerInput!) {
          customerCreate(input: $input) {
            customer { id email verifiedEmail }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "email": "legacy-version@example.com" } }),
    );
    versioned_create.path = "/admin/api/2025-01/graphql.json".to_string();
    let versioned_create = versioned_proxy.process_request(versioned_create);
    assert_eq!(versioned_create.status, 200);
    assert_eq!(
        versioned_create.body["data"]["customerCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        versioned_create.body["data"]["customerCreate"]["customer"]["verifiedEmail"],
        json!(true)
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query ReadConsumerCustomer($id: ID!, $identifier: CustomerIdentifierInput!) {
          customer(id: $id) { id email phone locale verifiedEmail tags }
          customerByIdentifier(identifier: $identifier) { id email phone }
        }
        "#,
        json!({ "id": id, "identifier": { "email": "alice@example.com" } }),
    ));
    assert_eq!(read.body["data"]["customer"]["id"], json!(id));
    assert_eq!(read.body["data"]["customerByIdentifier"]["id"], json!(id));

    let duplicate = proxy.process_request(json_graphql_request(
        r#"
        mutation ConsumerDuplicateCustomer($input: CustomerInput!) {
          customerCreate(input: $input) {
            customer { id }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "email": "ALICE@example.com", "firstName": "Duplicate" } }),
    ));
    assert_eq!(duplicate.status, 200);
    assert_eq!(
        duplicate.body["data"]["customerCreate"]["customer"],
        Value::Null
    );
    assert_eq!(
        duplicate.body["data"]["customerCreate"]["userErrors"],
        json!([{ "field": ["email"], "message": "Email has already been taken" }])
    );

    let log = proxy.get_log_snapshot();
    assert_eq!(log["entries"].as_array().unwrap().len(), 1);
    assert_eq!(
        log["entries"][0]["interpreted"]["primaryRootField"],
        json!("customerCreate")
    );
    assert!(log["entries"][0]["rawBody"]
        .as_str()
        .unwrap()
        .contains("ConsumerCreateCustomer"));
}

#[test]
fn customer_tax_exemption_roots_stage_and_project_downstream_reads() {
    let mut proxy = snapshot_proxy();
    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CustomerCreateParityPlan($input: CustomerInput!) {
          customerCreate(input: $input) {
            customer { id taxExempt taxExemptions }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "email": "tax-exemption-roots@example.test",
                "firstName": "Tax",
                "lastName": "Roots",
                "taxExempt": false
            }
        }),
    ));
    let id = create.body["data"]["customerCreate"]["customer"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let add = proxy.process_request(json_graphql_request(
        r#"
        mutation UnrelatedAddName($id: ID!, $taxExemptions: [TaxExemption!]!) {
          aliasedAdd: customerAddTaxExemptions(customerId: $id, taxExemptions: $taxExemptions) {
            customer { id taxExempt taxExemptions }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "id": id,
            "taxExemptions": [
                "CA_BC_RESELLER_EXEMPTION",
                "US_CA_RESELLER_EXEMPTION",
                "CA_BC_RESELLER_EXEMPTION"
            ]
        }),
    ));
    assert_eq!(add.status, 200);
    assert_eq!(
        add.body["data"]["aliasedAdd"]["customer"]["taxExempt"],
        json!(false)
    );
    assert_eq!(
        add.body["data"]["aliasedAdd"]["customer"]["taxExemptions"],
        json!(["CA_BC_RESELLER_EXEMPTION", "US_CA_RESELLER_EXEMPTION"])
    );
    assert_eq!(add.body["data"]["aliasedAdd"]["userErrors"], json!([]));

    let remove = proxy.process_request(json_graphql_request(
        r#"
        mutation UnrelatedRemoveName($id: ID!, $taxExemptions: [TaxExemption!]!) {
          customerRemoveTaxExemptions(customerId: $id, taxExemptions: $taxExemptions) {
            customer { id taxExemptions }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "id": id,
            "taxExemptions": ["CA_STATUS_CARD_EXEMPTION", "CA_BC_RESELLER_EXEMPTION"]
        }),
    ));
    assert_eq!(
        remove.body["data"]["customerRemoveTaxExemptions"]["customer"]["taxExemptions"],
        json!(["US_CA_RESELLER_EXEMPTION"])
    );

    let replace = proxy.process_request(json_graphql_request(
        r#"
        mutation UnrelatedReplaceName($id: ID!, $taxExemptions: [TaxExemption!]!) {
          customerReplaceTaxExemptions(customerId: $id, taxExemptions: $taxExemptions) {
            customer { id taxExempt taxExemptions }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "id": id,
            "taxExemptions": [
                "US_CA_RESELLER_EXEMPTION",
                "CA_STATUS_CARD_EXEMPTION",
                "US_CA_RESELLER_EXEMPTION"
            ]
        }),
    ));
    assert_eq!(
        replace.body["data"]["customerReplaceTaxExemptions"]["customer"]["taxExemptions"],
        json!(["US_CA_RESELLER_EXEMPTION", "CA_STATUS_CARD_EXEMPTION"])
    );
    assert_eq!(
        replace.body["data"]["customerReplaceTaxExemptions"]["customer"]["taxExempt"],
        json!(false)
    );

    let downstream = proxy.process_request(json_graphql_request(
        r#"
        query TaxRootDownstream($id: ID!, $identifier: CustomerIdentifierInput!, $query: String!, $first: Int!) {
          customer(id: $id) { id taxExempt taxExemptions }
          customerByIdentifier(identifier: $identifier) { id taxExempt taxExemptions }
          customers(first: $first, query: $query, sortKey: UPDATED_AT, reverse: true) {
            nodes { id taxExempt taxExemptions }
            pageInfo { hasNextPage hasPreviousPage }
          }
          customersCount { count precision }
        }
        "#,
        json!({
            "id": id,
            "identifier": { "id": id },
            "query": "__customer_parity_no_match__",
            "first": 5
        }),
    ));
    let expected = json!({
        "id": id,
        "taxExempt": false,
        "taxExemptions": ["US_CA_RESELLER_EXEMPTION", "CA_STATUS_CARD_EXEMPTION"]
    });
    assert_eq!(downstream.body["data"]["customer"], expected);
    assert_eq!(downstream.body["data"]["customerByIdentifier"], expected);
    assert_eq!(
        downstream.body["data"]["customers"],
        json!({
            "nodes": [],
            "pageInfo": { "hasNextPage": false, "hasPreviousPage": false }
        })
    );
    assert_eq!(
        downstream.body["data"]["customersCount"],
        json!({ "count": 177, "precision": "EXACT" })
    );

    let empty_replace = proxy.process_request(json_graphql_request(
        r#"
        mutation EmptyReplace($id: ID!, $taxExemptions: [TaxExemption!]!) {
          customerReplaceTaxExemptions(customerId: $id, taxExemptions: $taxExemptions) {
            customer { id taxExemptions }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": id, "taxExemptions": [] }),
    ));
    assert_eq!(
        empty_replace.body["data"]["customerReplaceTaxExemptions"]["customer"]["taxExemptions"],
        json!([])
    );

    let log = proxy.get_log_snapshot();
    let entries = log["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 5);
    assert_eq!(
        entries[1]["interpreted"]["primaryRootField"],
        json!("customerAddTaxExemptions")
    );
    assert_eq!(entries[1]["status"], json!("staged"));
    assert!(entries[1]["rawBody"]
        .as_str()
        .unwrap()
        .contains("UnrelatedAddName"));
    assert_eq!(
        entries[4]["interpreted"]["primaryRootField"],
        json!("customerReplaceTaxExemptions")
    );
}

#[test]
fn customer_tax_exemption_roots_return_unknown_customer_user_errors() {
    let mut proxy = snapshot_proxy();
    for root in [
        "customerAddTaxExemptions",
        "customerRemoveTaxExemptions",
        "customerReplaceTaxExemptions",
    ] {
        let query = format!(
            r#"
            mutation UnknownTaxRoot($id: ID!, $taxExemptions: [TaxExemption!]!) {{
              {root}(customerId: $id, taxExemptions: $taxExemptions) {{
                customer {{ id taxExemptions }}
                userErrors {{ field message }}
              }}
            }}
            "#
        );
        let response = proxy.process_request(json_graphql_request(
            &query,
            json!({
                "id": "gid://shopify/Customer/999999999999999",
                "taxExemptions": ["CA_BC_RESELLER_EXEMPTION"]
            }),
        ));
        assert_eq!(response.status, 200);
        assert_eq!(response.body["data"][root]["customer"], Value::Null);
        assert_eq!(
            response.body["data"][root]["userErrors"],
            json!([{ "field": ["customerId"], "message": "Customer does not exist." }])
        );
    }
    assert!(proxy.get_log_snapshot()["entries"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn customer_tax_exemption_roots_reject_invalid_enum_variables_before_staging() {
    let mut proxy = snapshot_proxy();
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation InvalidTaxVariable($id: ID!, $taxExemptions: [TaxExemption!]!) {
          customerAddTaxExemptions(customerId: $id, taxExemptions: $taxExemptions) {
            customer { id taxExemptions }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "id": "gid://shopify/Customer/9102966915305",
            "taxExemptions": ["NOT_A_REAL_EXEMPTION"]
        }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["errors"][0]["extensions"]["code"],
        json!("INVALID_VARIABLE")
    );
    assert!(response.body["errors"][0]["message"]
        .as_str()
        .is_some_and(|message| message.contains("NOT_A_REAL_EXEMPTION")
            && message.contains("CA_STATUS_CARD_EXEMPTION")
            && message.contains("CA_BC_RESELLER_EXEMPTION")
            && message.contains("US_CA_RESELLER_EXEMPTION")));
    assert!(proxy.get_log_snapshot()["entries"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn customer_tax_exemption_roots_reject_invalid_enum_literals_before_staging() {
    let mut proxy = snapshot_proxy();
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation InvalidTaxLiteral {
          customerAddTaxExemptions(
            customerId: "gid://shopify/Customer/9102966915305",
            taxExemptions: [NOT_A_REAL_EXEMPTION]
          ) {
            customer { id taxExemptions }
            userErrors { field message }
          }
        }
        "#,
        json!({}),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["errors"][0]["extensions"]["code"],
        json!("argumentLiteralsIncompatible")
    );
    assert_eq!(
        response.body["errors"][0]["extensions"]["argumentName"],
        json!("taxExemptions")
    );
    assert!(response.body["errors"][0]["message"]
        .as_str()
        .is_some_and(|message| message.contains("NOT_A_REAL_EXEMPTION")
            && message.contains("CA_STATUS_CARD_EXEMPTION")));
    assert!(proxy.get_log_snapshot()["entries"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn customer_tax_exemption_roots_hydrate_live_hybrid_customer_before_staging() {
    let customer_id = "gid://shopify/Customer/10540996428082";
    let upstream_queries = Arc::new(Mutex::new(Vec::<String>::new()));
    let captured_queries = Arc::clone(&upstream_queries);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let body: Value =
                serde_json::from_str(&request.body).expect("upstream request body parses");
            let query = body["query"]
                .as_str()
                .expect("upstream query is a string")
                .to_string();
            captured_queries.lock().unwrap().push(query.clone());
            let response = if query.contains("CustomerHydrate") {
                json!({
                    "data": {
                        "customer": {
                            "id": customer_id,
                            "firstName": "Hermes",
                            "lastName": "Tax",
                            "displayName": "Hermes Tax",
                            "email": "hermes-tax@example.com",
                            "taxExempt": false,
                            "taxExemptions": [],
                            "tags": ["parity"],
                            "defaultEmailAddress": { "emailAddress": "hermes-tax@example.com" },
                            "createdAt": "2026-04-25T22:56:29Z",
                            "updatedAt": "2026-04-25T22:56:29Z"
                        }
                    }
                })
            } else if query.contains("CustomerCountHydrate") {
                json!({
                    "data": {
                        "customersCount": { "count": 23, "precision": "EXACT" }
                    }
                })
            } else {
                json!({"errors": [{"message": format!("unexpected upstream query: {query}")}]})
            };
            shopify_draft_proxy::proxy::Response {
                status: 200,
                headers: Default::default(),
                body: response,
            }
        });

    let add = proxy.process_request(json_graphql_request(
        r#"
        mutation HydrateTaxRoot($id: ID!, $taxExemptions: [TaxExemption!]!) {
          customerAddTaxExemptions(customerId: $id, taxExemptions: $taxExemptions) {
            customer { id email taxExempt taxExemptions }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "id": customer_id,
            "taxExemptions": ["EU_REVERSE_CHARGE_EXEMPTION_RULE"]
        }),
    ));
    assert_eq!(add.status, 200);
    assert_eq!(
        add.body["data"]["customerAddTaxExemptions"]["userErrors"],
        json!([])
    );
    assert_eq!(
        add.body["data"]["customerAddTaxExemptions"]["customer"],
        json!({
            "id": customer_id,
            "email": "hermes-tax@example.com",
            "taxExempt": false,
            "taxExemptions": ["EU_REVERSE_CHARGE_EXEMPTION_RULE"]
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query HydratedTaxRead($id: ID!, $identifier: CustomerIdentifierInput!) {
          customer(id: $id) { id taxExemptions }
          customerByIdentifier(identifier: $identifier) { id taxExemptions }
          customersCount { count precision }
        }
        "#,
        json!({
            "id": customer_id,
            "identifier": { "id": customer_id }
        }),
    ));
    assert_eq!(
        read.body["data"]["customer"]["taxExemptions"],
        json!(["EU_REVERSE_CHARGE_EXEMPTION_RULE"])
    );
    assert_eq!(
        read.body["data"]["customerByIdentifier"]["taxExemptions"],
        json!(["EU_REVERSE_CHARGE_EXEMPTION_RULE"])
    );
    assert_eq!(
        read.body["data"]["customersCount"],
        json!({ "count": 23, "precision": "EXACT" })
    );

    let queries = upstream_queries.lock().unwrap();
    assert_eq!(queries.len(), 2);
    assert!(queries[0].contains("CustomerHydrate"));
    assert!(queries[1].contains("CustomerCountHydrate"));
}

#[test]
fn customer_update_rejects_inline_marketing_consent_without_mutating_customer() {
    let mut proxy = snapshot_proxy();
    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CustomerUpdateInlineConsentCreate($input: CustomerInput!) {
          customerCreate(input: $input) {
            customer { id }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "firstName": "Hermes", "lastName": "Original", "email": "inline-consent-baseline@example.com" } }),
    ));
    let id = create.body["data"]["customerCreate"]["customer"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let create_baseline = proxy.process_request(json_graphql_request(
        r#"
        mutation CustomerUpdateInlineConsentBaseline($input: CustomerInput!) {
          customerUpdate(input: $input) {
            customer { id firstName lastName displayName tags }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "id": id,
                "firstName": "Hermes",
                "lastName": "Baseline",
                "tags": ["stable"]
            }
        }),
    ));
    assert_eq!(
        create_baseline.body["data"]["customerUpdate"]["customer"]["displayName"],
        json!("Hermes Baseline")
    );

    let email_rejection = proxy.process_request(json_graphql_request(
        r#"
        mutation CustomerUpdateInlineConsentRejection($input: CustomerInput!) {
          customerUpdate(input: $input) {
            customer { id firstName lastName displayName tags }
            userErrors { field message }
            customerUpdateUserErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "id": id,
                "firstName": "ShouldNot",
                "lastName": "Apply",
                "tags": ["mutated"],
                "emailMarketingConsent": {
                    "marketingState": "SUBSCRIBED"
                }
            }
        }),
    ));
    let email_errors = json!([{
        "field": ["emailMarketingConsent"],
        "message": "To update emailMarketingConsent, please use the customerEmailMarketingConsentUpdate Mutation instead"
    }]);
    assert_eq!(
        email_rejection.body["data"]["customerUpdate"]["customer"],
        Value::Null
    );
    assert_eq!(
        email_rejection.body["data"]["customerUpdate"]["userErrors"],
        email_errors
    );
    assert_eq!(
        email_rejection.body["data"]["customerUpdate"]["customerUpdateUserErrors"],
        email_errors
    );

    let inline_literal_rejection = proxy.process_request(json_graphql_request(
        r#"
        mutation {
          customerUpdate(input: {
            id: "gid://shopify/Customer/999999999999999",
            emailMarketingConsent: { marketingState: SUBSCRIBED }
          }) {
            customer { id }
            userErrors { field message }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        inline_literal_rejection.body["data"]["customerUpdate"]["customer"],
        Value::Null
    );
    assert_eq!(
        inline_literal_rejection.body["data"]["customerUpdate"]["userErrors"],
        email_errors
    );

    let sms_rejection_unknown_customer = proxy.process_request(json_graphql_request(
        r#"
        mutation CustomerUpdateInlineConsentUnknownCustomer($input: CustomerInput!) {
          customerUpdate(input: $input) {
            customer { id }
            userErrors { field message }
            customerUpdateUserErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "id": "gid://shopify/Customer/999999999999999",
                "smsMarketingConsent": {
                    "marketingState": "UNSUBSCRIBED"
                }
            }
        }),
    ));
    let sms_errors = json!([{
        "field": ["smsMarketingConsent"],
        "message": "To update smsMarketingConsent, please use the customerSmsMarketingConsentUpdate Mutation instead"
    }]);
    assert_eq!(
        sms_rejection_unknown_customer.body["data"]["customerUpdate"]["customer"],
        Value::Null
    );
    assert_eq!(
        sms_rejection_unknown_customer.body["data"]["customerUpdate"]["userErrors"],
        sms_errors
    );
    assert_eq!(
        sms_rejection_unknown_customer.body["data"]["customerUpdate"]["customerUpdateUserErrors"],
        sms_errors
    );

    let both_rejection = proxy.process_request(json_graphql_request(
        r#"
        mutation CustomerUpdateInlineConsentBoth($input: CustomerInput!) {
          customerUpdate(input: $input) {
            customer { id }
            userErrors { field message }
            customerUpdateUserErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "id": id,
                "emailMarketingConsent": {
                    "marketingState": "SUBSCRIBED"
                },
                "smsMarketingConsent": {
                    "marketingState": "UNSUBSCRIBED"
                }
            }
        }),
    ));
    let both_errors = json!([
        {
            "field": ["smsMarketingConsent"],
            "message": "To update smsMarketingConsent, please use the customerSmsMarketingConsentUpdate Mutation instead"
        },
        {
            "field": ["emailMarketingConsent"],
            "message": "To update emailMarketingConsent, please use the customerEmailMarketingConsentUpdate Mutation instead"
        }
    ]);
    assert_eq!(
        both_rejection.body["data"]["customerUpdate"]["customer"],
        Value::Null
    );
    assert_eq!(
        both_rejection.body["data"]["customerUpdate"]["userErrors"],
        both_errors
    );
    assert_eq!(
        both_rejection.body["data"]["customerUpdate"]["customerUpdateUserErrors"],
        both_errors
    );

    let downstream = proxy.process_request(json_graphql_request(
        r#"
        query CustomerUpdateInlineConsentDownstream($id: ID!, $identifier: CustomerIdentifierInput!) {
          customer(id: $id) { id firstName lastName displayName tags }
          customerByIdentifier(identifier: $identifier) { id firstName lastName displayName tags }
        }
        "#,
        json!({ "id": id, "identifier": { "id": id } }),
    ));
    let expected_customer = json!({
        "id": id,
        "firstName": "Hermes",
        "lastName": "Baseline",
        "displayName": "Hermes Baseline",
        "tags": ["stable"]
    });
    assert_eq!(downstream.body["data"]["customer"], expected_customer);
    assert_eq!(
        downstream.body["data"]["customerByIdentifier"],
        expected_customer
    );
}

#[test]
fn customer_update_delete_and_set_are_root_field_routed() {
    let mut proxy = configured_proxy(
        ReadMode::Snapshot,
        Some(shopify_draft_proxy::proxy::UnsupportedMutationMode::Reject),
    );

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation SeedCustomerForSet($input: CustomerInput!) {
          customerCreate(input: $input) {
            customer { id email phone }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "email": "set-route@example.com", "phone": "+1 415 555 0101" } }),
    ));
    let id = create.body["data"]["customerCreate"]["customer"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let set_create = proxy.process_request(json_graphql_request(
        r#"
        mutation ConsumerCustomerSetCreate($input: CustomerSetInput!) {
          customerSet(input: $input) {
            customer { id firstName email locale verifiedEmail }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "input": { "firstName": "Bob", "email": "set-create@example.com" } }),
    ));
    assert_eq!(set_create.status, 200);
    assert_eq!(
        set_create.body["data"]["customerSet"]["userErrors"],
        json!([])
    );
    assert_eq!(
        set_create.body["data"]["customerSet"]["customer"]["email"],
        json!("set-create@example.com")
    );

    let set_update = proxy.process_request(json_graphql_request(
        r#"
        mutation ConsumerCustomerSetByEmail($identifier: CustomerSetIdentifiers, $input: CustomerSetInput!) {
          customerSet(identifier: $identifier, input: $input) {
            customer {
              id
              firstName
              email
              defaultAddress { address1 city province country zip formattedArea }
              addressesV2(first: 5) {
                nodes { id address1 city province country zip formattedArea }
                pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "identifier": { "email": "set-route@example.com" },
            "input": {
                "email": "set-route@example.com",
                "firstName": "Updated",
                "addresses": [{
                    "address1": "11 Upsert St",
                    "city": "Toronto",
                    "countryCode": "CA",
                    "provinceCode": "ON",
                    "zip": "M5H 2N2"
                }]
            }
        }),
    ));
    assert_eq!(set_update.status, 200);
    assert_eq!(
        set_update.body["data"]["customerSet"]["userErrors"],
        json!([])
    );
    assert_eq!(
        set_update.body["data"]["customerSet"]["customer"]["id"],
        json!(id)
    );
    assert_eq!(
        set_update.body["data"]["customerSet"]["customer"]["firstName"],
        json!("Updated")
    );
    assert_eq!(
        set_update.body["data"]["customerSet"]["customer"]["defaultAddress"],
        json!({
            "address1": "11 Upsert St",
            "city": "Toronto",
            "province": "Ontario",
            "country": "Canada",
            "zip": "M5H 2N2",
            "formattedArea": "Toronto ON, Canada"
        })
    );
    assert_eq!(
        set_update.body["data"]["customerSet"]["customer"]["addressesV2"]["nodes"][0]["address1"],
        json!("11 Upsert St")
    );

    let null_addresses = proxy.process_request(json_graphql_request(
        r#"
        mutation ConsumerCustomerSetNullAddresses($identifier: CustomerSetIdentifiers, $input: CustomerSetInput!) {
          customerSet(identifier: $identifier, input: $input) {
            customer {
              defaultAddress { address1 city province country zip formattedArea }
              addressesV2(first: 5) { nodes { address1 city } }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "identifier": { "id": id },
            "input": { "email": "set-route@example.com", "addresses": null }
        }),
    ));
    assert_eq!(null_addresses.status, 200);
    assert_eq!(
        null_addresses.body["data"]["customerSet"]["customer"]["defaultAddress"]["address1"],
        json!("11 Upsert St")
    );
    assert_eq!(
        null_addresses.body["data"]["customerSet"]["customer"]["addressesV2"]["nodes"][0]["city"],
        json!("Toronto")
    );

    let clear_addresses = proxy.process_request(json_graphql_request(
        r#"
        mutation ConsumerCustomerSetClearAddresses($identifier: CustomerSetIdentifiers, $input: CustomerSetInput!) {
          customerSet(identifier: $identifier, input: $input) {
            customer {
              defaultAddress { address1 }
              addressesV2(first: 5) { nodes { address1 } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "identifier": { "id": id },
            "input": { "email": "set-route@example.com", "addresses": [] }
        }),
    ));
    assert_eq!(clear_addresses.status, 200);
    assert_eq!(
        clear_addresses.body["data"]["customerSet"]["customer"]["defaultAddress"],
        Value::Null
    );
    assert_eq!(
        clear_addresses.body["data"]["customerSet"]["customer"]["addressesV2"],
        json!({
            "nodes": [],
            "pageInfo": { "hasNextPage": false, "hasPreviousPage": false, "startCursor": null, "endCursor": null }
        })
    );

    let update_identity = proxy.process_request(json_graphql_request(
        r#"
        mutation ConsumerCustomerUpdate($input: CustomerInput!) {
          customerUpdate(input: $input) {
            customer { id }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "id": id, "firstName": "", "lastName": "", "email": "", "phone": "" } }),
    ));
    assert_eq!(update_identity.status, 200);
    assert_eq!(
        update_identity.body["data"]["customerUpdate"]["userErrors"],
        json!([{ "field": null, "message": "A name, phone number, or email address must be present" }])
    );

    let delete = proxy.process_request(json_graphql_request(
        r#"
        mutation ConsumerCustomerDelete($input: CustomerDeleteInput!) {
          customerDelete(input: $input) {
            deletedCustomerId
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "id": create.body["data"]["customerCreate"]["customer"]["id"] } }),
    ));
    assert_eq!(delete.status, 200);
    assert_eq!(
        delete.body["data"]["customerDelete"]["deletedCustomerId"],
        create.body["data"]["customerCreate"]["customer"]["id"]
    );
    assert_eq!(
        delete.body["data"]["customerDelete"]["userErrors"],
        json!([])
    );
}

#[test]
fn delegate_access_token_create_shop_payload_expires_parent_and_destroy_lifecycle() {
    let mut proxy = snapshot_proxy();

    let expires_after_parent = proxy.process_request(json_graphql_request(
        r#"
        mutation DelegateAccessTokenCreateExpiresAfterParent {
          delegateAccessTokenCreate(input: { delegateAccessScope: ["read_products"], expiresIn: 99999999 }) {
            delegateAccessToken { accessToken accessScopes createdAt expiresIn }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        expires_after_parent.body["data"]["delegateAccessTokenCreate"],
        json!({
            "delegateAccessToken": null,
            "userErrors": [{
                "field": null,
                "message": "The delegate token can't expire after the parent token.",
                "code": "EXPIRES_AFTER_PARENT"
            }]
        })
    );

    let missing = proxy.process_request(json_graphql_request(
        r#"
        mutation DelegateAccessTokenDestroyCodes($token: String!) {
          delegateAccessTokenDestroy(accessToken: $token) {
            status
            shop { id name }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "token": "shpat_does_not_exist" }),
    ));
    assert_eq!(
        missing.body["data"]["delegateAccessTokenDestroy"],
        json!({
            "status": false,
            "shop": { "id": "gid://shopify/Shop/92891250994", "name": "harry-test-heelo" },
            "userErrors": [{ "field": null, "message": "Access token does not exist.", "code": "ACCESS_TOKEN_NOT_FOUND" }]
        })
    );

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation DelegateAccessTokenCreateShopPayload {
          delegateAccessTokenCreate(input: { delegateAccessScope: ["read_products"], expiresIn: 300 }) {
            delegateAccessToken { accessToken }
            shop { id myshopifyDomain currencyCode }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        create.body["data"]["delegateAccessTokenCreate"]["shop"],
        json!({
            "id": "gid://shopify/Shop/92891250994",
            "myshopifyDomain": "harry-test-heelo.myshopify.com",
            "currencyCode": "USD"
        })
    );
    assert_eq!(
        create.body["data"]["delegateAccessTokenCreate"]["userErrors"],
        json!([])
    );
    let token = create.body["data"]["delegateAccessTokenCreate"]["delegateAccessToken"]
        ["accessToken"]
        .as_str()
        .unwrap()
        .to_string();

    let destroy = proxy.process_request(json_graphql_request(
        r#"
        mutation DelegateAccessTokenDestroyShopPayload($token: String!) {
          delegateAccessTokenDestroy(accessToken: $token) {
            shop { id }
            status
            userErrors { field message code }
          }
        }
        "#,
        json!({ "token": token }),
    ));
    assert_eq!(
        destroy.body["data"]["delegateAccessTokenDestroy"],
        json!({
            "shop": { "id": "gid://shopify/Shop/92891250994" },
            "status": true,
            "userErrors": []
        })
    );

    let repeat = proxy.process_request(json_graphql_request(
        r#"
        mutation DelegateAccessTokenDestroyShopPayloadUnknown {
          delegateAccessTokenDestroy(accessToken: "shpat_unknown") {
            shop { id }
            status
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        repeat.body["data"]["delegateAccessTokenDestroy"],
        json!({
            "shop": { "id": "gid://shopify/Shop/92891250994" },
            "status": false,
            "userErrors": [{ "field": null, "message": "Access token does not exist.", "code": "ACCESS_TOKEN_NOT_FOUND" }]
        })
    );

    let mut self_delete = json_graphql_request(
        r#"
        mutation DelegateAccessTokenDestroyCodes($token: String!) {
          delegateAccessTokenDestroy(accessToken: $token) {
            status
            shop { id name }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "token": "shpat_parent_destroy_self" }),
    );
    self_delete.headers.insert(
        "X-Shopify-Access-Token".to_string(),
        "shpat_parent_destroy_self".to_string(),
    );
    let self_delete = proxy.process_request(self_delete);
    assert_eq!(
        self_delete.body["data"]["delegateAccessTokenDestroy"]["userErrors"],
        json!([{ "field": null, "message": "Can only delete delegate tokens.", "code": "CAN_ONLY_DELETE_DELEGATE_TOKENS" }])
    );
}

#[test]
fn customer_mutations_hydrate_existing_live_customers_without_passthrough_writes() {
    let upstream_calls = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_calls = Arc::clone(&upstream_calls);
    let mut proxy = configured_proxy(
        ReadMode::LiveHybrid,
        Some(shopify_draft_proxy::proxy::UnsupportedMutationMode::Reject),
    )
    .with_upstream_transport(move |request| {
        let body: Value = serde_json::from_str(&request.body).unwrap();
        captured_calls.lock().unwrap().push(body.clone());
        match (
            body["operationName"].as_str().unwrap_or_default(),
            body["variables"]["query"].as_str(),
            body["variables"]["id"].as_str(),
        ) {
            ("CustomerDuplicateHydrate", Some("email:upstream@example.com"), _) => {
                shopify_draft_proxy::proxy::Response {
                    status: 200,
                    headers: Default::default(),
                    body: json!({
                        "data": {
                            "customers": {
                                "nodes": [{ "id": "gid://shopify/Customer/upstream" }]
                            }
                        }
                    }),
                }
            }
            ("CustomerHydrate", _, Some("gid://shopify/Customer/upstream")) => {
                shopify_draft_proxy::proxy::Response {
                    status: 200,
                    headers: Default::default(),
                    body: json!({
                        "data": {
                            "customer": {
                                "id": "gid://shopify/Customer/upstream",
                                "email": "upstream@example.com",
                                "displayName": "Upstream Customer",
                                "defaultEmailAddress": { "emailAddress": "upstream@example.com" },
                                "defaultPhoneNumber": { "phoneNumber": "+14155550199" },
                                "canDelete": true,
                                "verifiedEmail": true,
                                "taxExempt": false,
                                "taxExemptions": [],
                                "tags": [],
                                "state": "DISABLED",
                                "locale": "en"
                            }
                        }
                    }),
                }
            }
            _ => shopify_draft_proxy::proxy::Response {
                status: 200,
                headers: Default::default(),
                body: json!({ "data": { "customers": { "nodes": [] }, "customer": null } }),
            },
        }
    });

    let duplicate = proxy.process_request(json_graphql_request(
        r#"
        mutation OrdinaryDuplicate($input: CustomerInput!) {
          customerCreate(input: $input) {
            customer { id }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "firstName": "Dupe", "email": "upstream@example.com" } }),
    ));
    assert_eq!(duplicate.status, 200);
    assert_eq!(
        duplicate.body["data"]["customerCreate"]["customer"],
        Value::Null
    );
    assert_eq!(
        duplicate.body["data"]["customerCreate"]["userErrors"],
        json!([{ "field": ["email"], "message": "Email has already been taken" }])
    );

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation OrdinaryUpdate($input: CustomerInput!) {
          customerUpdate(input: $input) {
            customer { id firstName email defaultPhoneNumber { phoneNumber } verifiedEmail }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "id": "gid://shopify/Customer/upstream", "firstName": "Updated" } }),
    ));
    assert_eq!(update.status, 200);
    assert_eq!(
        update.body["data"]["customerUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        update.body["data"]["customerUpdate"]["customer"]["id"],
        json!("gid://shopify/Customer/upstream")
    );
    assert_eq!(
        update.body["data"]["customerUpdate"]["customer"]["firstName"],
        json!("Updated")
    );
    assert_eq!(
        update.body["data"]["customerUpdate"]["customer"]["defaultPhoneNumber"]["phoneNumber"],
        json!("+14155550199")
    );
    assert_eq!(
        update.body["data"]["customerUpdate"]["customer"]["verifiedEmail"],
        json!(true)
    );

    let calls = upstream_calls.lock().unwrap();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0]["operationName"], json!("CustomerDuplicateHydrate"));
    assert_eq!(calls[1]["operationName"], json!("CustomerHydrate"));
    assert!(calls.iter().all(|call| !call["query"]
        .as_str()
        .unwrap_or_default()
        .contains("mutation")));
}

#[test]
fn customer_update_and_delete_stage_known_fixture_customer_reads() {
    let mut proxy = snapshot_proxy();
    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CustomerUpdateDeleteSeed($input: CustomerInput!) {
          customerCreate(input: $input) {
            customer { id }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "email": "update-delete-seed@example.com", "firstName": "Hermes", "lastName": "Create", "phone": "+14155550123" } }),
    ));
    let id = create.body["data"]["customerCreate"]["customer"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation CustomerUpdateParityPlan($input: CustomerInput!) {
          customerUpdate(input: $input) {
            customer { id firstName lastName displayName email note taxExempt taxExemptions tags defaultPhoneNumber { phoneNumber } loyalty: metafield(namespace: "custom", key: "loyalty") { id namespace key type value } metafields(first: 5) { nodes { id namespace key type value } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "id": id,
                "firstName": "Hermes",
                "lastName": "Updated",
                "note": "customer update parity probe",
                "tags": ["parity", "updated"],
                "taxExempt": false,
                "taxExemptions": ["CA_BC_RESELLER_EXEMPTION"],
                "metafields": [{ "namespace": "custom", "key": "loyalty", "type": "single_line_text_field", "value": "gold" }]
            }
        }),
    ));
    assert_eq!(
        update.body["data"]["customerUpdate"]["customer"]["displayName"],
        json!("Hermes Updated")
    );
    assert_eq!(
        update.body["data"]["customerUpdate"]["customer"]["loyalty"]["value"],
        json!("gold")
    );
    assert_eq!(
        update.body["data"]["customerUpdate"]["customer"]["defaultPhoneNumber"]["phoneNumber"],
        json!("+14155550123")
    );

    let delete = proxy.process_request(json_graphql_request(
        r#"
        mutation CustomerDeleteParityPlan($input: CustomerDeleteInput!) {
          customerDelete(input: $input) { deletedCustomerId shop { id } userErrors { field message } }
        }
        "#,
        json!({ "input": { "id": id } }),
    ));
    assert_eq!(
        delete.body["data"]["customerDelete"],
        json!({
            "deletedCustomerId": id,
            "shop": { "id": "gid://shopify/Shop/1?shopify-draft-proxy=synthetic" },
            "userErrors": []
        })
    );
    let read = proxy.process_request(json_graphql_request(
        "query($id: ID!) { customer(id: $id) { id email } }",
        json!({ "id": id }),
    ));
    assert_eq!(read.body["data"]["customer"], Value::Null);
}

#[test]
fn customer_delete_order_precondition_blocks_only_when_order_exists() {
    let mut proxy = snapshot_proxy();

    let create_query = r#"
        mutation CustomerDeleteOrderPreconditionCustomerCreate($input: CustomerInput!) {
          customerCreate(input: $input) {
            customer { id email displayName }
            userErrors { field message }
          }
        }
        "#;
    let create = proxy.process_request(json_graphql_request(
        create_query,
        json!({
            "input": {
                "email": "har-773-blocked@example.test",
                "firstName": "Blocked",
                "lastName": "Delete"
            }
        }),
    ));
    assert_eq!(
        create.body["data"]["customerCreate"]["userErrors"],
        json!([])
    );
    let customer_id = create.body["data"]["customerCreate"]["customer"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let order = proxy.process_request(json_graphql_request(
        r#"
        mutation CustomerDeleteOrderPreconditionOrderCreate($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order { id customer { id email displayName } }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "order": {
                "email": "har-773-order@example.test",
                "customerId": customer_id,
                "currency": "CAD",
                "lineItems": [{ "title": "HAR-773 blocking line", "quantity": 1 }]
            }
        }),
    ));
    assert_eq!(order.body["data"]["orderCreate"]["userErrors"], json!([]));

    let blocked = proxy.process_request(json_graphql_request(
        r#"
        mutation CustomerDeleteOrderPreconditionDelete($input: CustomerDeleteInput!) {
          customerDelete(input: $input) { deletedCustomerId userErrors { field message } }
        }
        "#,
        json!({ "input": { "id": customer_id } }),
    ));
    assert_eq!(
        blocked.body["data"]["customerDelete"],
        json!({
            "deletedCustomerId": null,
            "userErrors": [{ "field": ["id"], "message": "Customer can’t be deleted because they have associated orders" }]
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query CustomerDeleteOrderPreconditionRead($id: ID!) {
          customer(id: $id) {
            id email displayName
            orders(first: 5) { nodes { id customer { id email displayName } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } }
          }
        }
        "#,
        json!({ "id": customer_id }),
    ));
    assert_eq!(read.body["data"]["customer"]["id"], json!(customer_id));
    assert_eq!(
        read.body["data"]["customer"]["orders"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn customer_orders_connection_paginates_edges_nodes_and_page_info_consistently() {
    let mut proxy = snapshot_proxy();

    let create_customer = proxy.process_request(json_graphql_request(
        r#"
        mutation CustomerDeleteOrderPreconditionCustomerCreate($input: CustomerInput!) {
          customerCreate(input: $input) { customer { id email } userErrors { field message } }
        }
        "#,
        json!({"input": {"email": "relay-orders@example.test"}}),
    ));
    let customer_id = create_customer.body["data"]["customerCreate"]["customer"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    for title in ["First order", "Second order", "Third order"] {
        let order = proxy.process_request(json_graphql_request(
            r#"
            mutation CustomerDeleteOrderPreconditionOrderCreate($order: OrderCreateOrderInput!) {
              orderCreate(order: $order) { order { id } userErrors { field message code } }
            }
            "#,
            json!({"order": {
                "email": "relay-orders@example.test",
                "customerId": customer_id,
                "lineItems": [{ "title": title, "quantity": 1 }]
            }}),
        ));
        assert_eq!(order.body["data"]["orderCreate"]["userErrors"], json!([]));
    }

    let first_page = proxy.process_request(json_graphql_request(
        r#"
        query CustomerOrdersRelayPage($id: ID!, $first: Int!) {
          customer(id: $id) {
            orders(first: $first) {
              nodes { id }
              edges { cursor node { id } }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
          }
        }
        "#,
        json!({"id": customer_id, "first": 2}),
    ));
    assert_eq!(
        first_page.body["data"]["customer"]["orders"],
        json!({
            "nodes": [
                {"id": "gid://shopify/Order/1"},
                {"id": "gid://shopify/Order/2"}
            ],
            "edges": [
                {"cursor": "gid://shopify/Order/1", "node": {"id": "gid://shopify/Order/1"}},
                {"cursor": "gid://shopify/Order/2", "node": {"id": "gid://shopify/Order/2"}}
            ],
            "pageInfo": {
                "hasNextPage": true,
                "hasPreviousPage": false,
                "startCursor": "gid://shopify/Order/1",
                "endCursor": "gid://shopify/Order/2"
            }
        })
    );

    let second_page = proxy.process_request(json_graphql_request(
        r#"
        query CustomerOrdersRelayAfter($id: ID!, $first: Int!, $after: String!) {
          customer(id: $id) {
            orders(first: $first, after: $after) {
              nodes { id }
              edges { cursor node { id } }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
          }
        }
        "#,
        json!({
            "id": customer_id,
            "first": 2,
            "after": first_page.body["data"]["customer"]["orders"]["pageInfo"]["endCursor"]
        }),
    ));
    assert_eq!(
        second_page.body["data"]["customer"]["orders"],
        json!({
            "nodes": [{"id": "gid://shopify/Order/3"}],
            "edges": [{"cursor": "gid://shopify/Order/3", "node": {"id": "gid://shopify/Order/3"}}],
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": true,
                "startCursor": "gid://shopify/Order/3",
                "endCursor": "gid://shopify/Order/3"
            }
        })
    );
}

#[test]
fn customer_create_supports_consent_precondition_shapes_without_synthesizing_missing_contacts() {
    let mut proxy = snapshot_proxy();
    let phone_only = proxy.process_request(json_graphql_request(
        r#"
        mutation CustomerInputInlineConsentCreate($input: CustomerInput!) {
          customerCreate(input: $input) {
            customer { id email defaultEmailAddress { emailAddress } defaultPhoneNumber { phoneNumber } }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "firstName": "Hermes", "lastName": "ConsentPhoneOnly", "phone": "+141****6021" } }),
    ));
    assert_eq!(
        phone_only.body["data"]["customerCreate"]["userErrors"],
        json!([])
    );
    let phone_customer = &phone_only.body["data"]["customerCreate"]["customer"];
    assert_eq!(phone_customer["email"], Value::Null);
    assert_eq!(phone_customer["defaultEmailAddress"], Value::Null);
    assert_eq!(
        phone_customer["defaultPhoneNumber"]["phoneNumber"],
        json!("+141****6021")
    );

    let email_only = proxy.process_request(json_graphql_request(
        r#"
        mutation CustomerInputInlineConsentCreate($input: CustomerInput!) {
          customerCreate(input: $input) {
            customer { id email defaultEmailAddress { emailAddress } defaultPhoneNumber { phoneNumber } }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "firstName": "Hermes", "lastName": "ConsentEmailOnly", "email": "hermes-consent-email-only-1777943566021@example.com" } }),
    ));
    let email_customer = &email_only.body["data"]["customerCreate"]["customer"];
    assert_eq!(
        email_only.body["data"]["customerCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        email_customer["email"],
        json!("hermes-consent-email-only-1777943566021@example.com")
    );
    assert_eq!(
        email_customer["defaultEmailAddress"]["emailAddress"],
        json!("hermes-consent-email-only-1777943566021@example.com")
    );
    assert_eq!(email_customer["defaultPhoneNumber"], Value::Null);
}

#[test]
fn customer_marketing_consent_updates_stage_and_project_downstream_reads() {
    let mut proxy = snapshot_proxy();
    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CustomerCreateParityPlan($input: CustomerInput!) {
          customerCreate(input: $input) {
            customer {
              id
              email
              defaultEmailAddress { emailAddress marketingState marketingOptInLevel marketingUpdatedAt }
              defaultPhoneNumber { phoneNumber marketingState marketingOptInLevel marketingUpdatedAt marketingCollectedFrom }
              emailMarketingConsent { marketingState marketingOptInLevel consentUpdatedAt }
              smsMarketingConsent { marketingState marketingOptInLevel consentUpdatedAt consentCollectedFrom }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "email": "hermes-consent-stage@example.com",
                "firstName": "Hermes",
                "lastName": "Consent",
                "phone": "+14155556021"
            }
        }),
    ));
    let customer_id = create.body["data"]["customerCreate"]["customer"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let email_update = proxy.process_request(json_graphql_request(
        r#"
        mutation ConsumerNamedEmailConsent($input: CustomerEmailMarketingConsentUpdateInput!) {
          consentAlias: customerEmailMarketingConsentUpdate(input: $input) {
            customer {
              id
              defaultEmailAddress { emailAddress marketingState marketingOptInLevel marketingUpdatedAt }
              emailMarketingConsent { marketingState marketingOptInLevel consentUpdatedAt }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "input": {
                "customerId": customer_id,
                "emailMarketingConsent": {
                    "marketingState": "SUBSCRIBED",
                    "marketingOptInLevel": "SINGLE_OPT_IN",
                    "consentUpdatedAt": "2026-04-25T02:10:00Z"
                }
            }
        }),
    ));
    assert_eq!(
        email_update.body["data"]["consentAlias"]["userErrors"],
        json!([])
    );
    assert_eq!(
        email_update.body["data"]["consentAlias"]["customer"]["defaultEmailAddress"],
        json!({
            "emailAddress": "hermes-consent-stage@example.com",
            "marketingState": "SUBSCRIBED",
            "marketingOptInLevel": "SINGLE_OPT_IN",
            "marketingUpdatedAt": "2026-04-25T02:10:00Z"
        })
    );
    assert_eq!(
        email_update.body["data"]["consentAlias"]["customer"]["emailMarketingConsent"],
        json!({
            "marketingState": "SUBSCRIBED",
            "marketingOptInLevel": "SINGLE_OPT_IN",
            "consentUpdatedAt": "2026-04-25T02:10:00Z"
        })
    );

    let sms_update = proxy.process_request(json_graphql_request(
        r#"
        mutation ConsumerNamedSmsConsent($input: CustomerSmsMarketingConsentUpdateInput!) {
          customerSmsMarketingConsentUpdate(input: $input) {
            customer {
              id
              defaultPhoneNumber { phoneNumber marketingState marketingOptInLevel marketingUpdatedAt marketingCollectedFrom }
              smsMarketingConsent { marketingState marketingOptInLevel consentUpdatedAt consentCollectedFrom }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "input": {
                "customerId": customer_id,
                "smsMarketingConsent": {
                    "marketingState": "SUBSCRIBED",
                    "marketingOptInLevel": "SINGLE_OPT_IN",
                    "consentUpdatedAt": "2026-04-25T02:11:00Z"
                }
            }
        }),
    ));
    assert_eq!(
        sms_update.body["data"]["customerSmsMarketingConsentUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        sms_update.body["data"]["customerSmsMarketingConsentUpdate"]["customer"]
            ["defaultPhoneNumber"],
        json!({
            "phoneNumber": "+14155556021",
            "marketingState": "SUBSCRIBED",
            "marketingOptInLevel": "SINGLE_OPT_IN",
            "marketingUpdatedAt": "2026-04-25T02:11:00Z",
            "marketingCollectedFrom": "OTHER"
        })
    );

    let downstream = proxy.process_request(json_graphql_request(
        r#"
        query ConsumerConsentDownstream($id: ID!, $identifier: CustomerIdentifierInput!) {
          customer(id: $id) {
            id
            defaultEmailAddress { emailAddress marketingState marketingOptInLevel marketingUpdatedAt }
            defaultPhoneNumber { phoneNumber marketingState marketingOptInLevel marketingUpdatedAt marketingCollectedFrom }
            emailMarketingConsent { marketingState marketingOptInLevel consentUpdatedAt }
            smsMarketingConsent { marketingState marketingOptInLevel consentUpdatedAt consentCollectedFrom }
          }
          customerByIdentifier(identifier: $identifier) {
            id
            defaultEmailAddress { emailAddress marketingState marketingOptInLevel marketingUpdatedAt }
            defaultPhoneNumber { phoneNumber marketingState marketingOptInLevel marketingUpdatedAt marketingCollectedFrom }
            emailMarketingConsent { marketingState marketingOptInLevel consentUpdatedAt }
            smsMarketingConsent { marketingState marketingOptInLevel consentUpdatedAt consentCollectedFrom }
          }
        }
        "#,
        json!({
            "id": customer_id,
            "identifier": { "emailAddress": "hermes-consent-stage@example.com" }
        }),
    ));
    assert_eq!(
        downstream.body["data"]["customer"],
        downstream.body["data"]["customerByIdentifier"]
    );
    assert_eq!(
        downstream.body["data"]["customer"]["defaultEmailAddress"]["marketingUpdatedAt"],
        json!("2026-04-25T02:10:00Z")
    );
    assert_eq!(
        downstream.body["data"]["customer"]["smsMarketingConsent"]["consentCollectedFrom"],
        json!("OTHER")
    );

    let log = proxy.process_request(Request {
        method: "GET".to_string(),
        path: "/__meta/log".to_string(),
        headers: Default::default(),
        body: String::new(),
    });
    assert_eq!(log.body["entries"].as_array().unwrap().len(), 3);
    assert!(log.body["entries"][1]["rawBody"]
        .as_str()
        .unwrap()
        .contains("ConsumerNamedEmailConsent"));
    assert_eq!(
        log.body["entries"][1]["stagedResourceIds"],
        json!([customer_id.clone()])
    );
    assert!(log.body["entries"][2]["rawBody"]
        .as_str()
        .unwrap()
        .contains("ConsumerNamedSmsConsent"));
}

#[test]
fn customer_marketing_consent_resolver_errors_do_not_mutate_state() {
    let mut proxy = snapshot_proxy();
    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CustomerCreateParityPlan($input: CustomerInput!) {
          customerCreate(input: $input) {
            customer { id email defaultEmailAddress { marketingState marketingUpdatedAt } defaultPhoneNumber { phoneNumber marketingState marketingUpdatedAt marketingCollectedFrom } }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "email": "hermes-consent-errors@example.com",
                "firstName": "Hermes",
                "lastName": "Consent",
                "phone": "+14155556023"
            }
        }),
    ));
    let customer_id = create.body["data"]["customerCreate"]["customer"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let pending_error = proxy.process_request(json_graphql_request(
        r#"
        mutation AnyEmailResolverName($input: CustomerEmailMarketingConsentUpdateInput!) {
          customerEmailMarketingConsentUpdate(input: $input) {
            customer { id defaultEmailAddress { marketingState marketingOptInLevel marketingUpdatedAt } }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "input": {
                "customerId": customer_id,
                "emailMarketingConsent": {
                    "marketingState": "PENDING",
                    "marketingOptInLevel": "SINGLE_OPT_IN",
                    "consentUpdatedAt": "2026-04-25T02:20:00Z"
                }
            }
        }),
    ));
    assert_eq!(
        pending_error.body["data"]["customerEmailMarketingConsentUpdate"]["userErrors"],
        json!([{
            "field": ["input", "emailMarketingConsent", "marketingOptInLevel"],
            "message": "Marketing opt in level must be confirmed opt-in for pending consent state",
            "code": "INVALID"
        }])
    );
    assert_eq!(
        pending_error.body["data"]["customerEmailMarketingConsentUpdate"]["customer"]
            ["defaultEmailAddress"]["marketingState"],
        json!("NOT_SUBSCRIBED")
    );

    let disallowed = proxy.process_request(json_graphql_request(
        r#"
        mutation AnyEmailResolverName($input: CustomerEmailMarketingConsentUpdateInput!) {
          customerEmailMarketingConsentUpdate(input: $input) {
            customer { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "input": {
                "customerId": customer_id,
                "emailMarketingConsent": {
                    "marketingState": "NOT_SUBSCRIBED",
                    "marketingOptInLevel": "SINGLE_OPT_IN",
                    "consentUpdatedAt": "2026-04-25T02:21:00Z"
                }
            }
        }),
    ));
    assert_eq!(
        disallowed.body["errors"][0]["message"],
        json!("Cannot specify NOT_SUBSCRIBED as a marketing state input")
    );
    assert_eq!(
        disallowed.body["data"]["customerEmailMarketingConsentUpdate"],
        Value::Null
    );

    let valid_pending = proxy.process_request(json_graphql_request(
        r#"
        mutation AnyEmailResolverName($input: CustomerEmailMarketingConsentUpdateInput!) {
          customerEmailMarketingConsentUpdate(input: $input) {
            customer { id defaultEmailAddress { marketingState marketingOptInLevel marketingUpdatedAt } }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "input": {
                "customerId": customer_id,
                "emailMarketingConsent": {
                    "marketingState": "PENDING",
                    "marketingOptInLevel": "CONFIRMED_OPT_IN",
                    "consentUpdatedAt": "2026-04-25T02:22:00Z"
                }
            }
        }),
    ));
    assert_eq!(
        valid_pending.body["data"]["customerEmailMarketingConsentUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        valid_pending.body["data"]["customerEmailMarketingConsentUpdate"]["customer"]
            ["defaultEmailAddress"],
        json!({
            "marketingState": "PENDING",
            "marketingOptInLevel": "CONFIRMED_OPT_IN",
            "marketingUpdatedAt": "2026-04-25T02:22:00Z"
        })
    );

    let phone_only = proxy.process_request(json_graphql_request(
        r#"
        mutation CustomerInputInlineConsentCreate($input: CustomerInput!) {
          customerCreate(input: $input) {
            customer { id defaultEmailAddress { emailAddress } defaultPhoneNumber { phoneNumber } }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "firstName": "Hermes", "lastName": "PhoneOnly", "phone": "+14155556024" } }),
    ));
    let phone_only_id = phone_only.body["data"]["customerCreate"]["customer"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let email_noop = proxy.process_request(json_graphql_request(
        r#"
        mutation AnyEmailNoop($input: CustomerEmailMarketingConsentUpdateInput!) {
          customerEmailMarketingConsentUpdate(input: $input) {
            customer { id defaultEmailAddress { marketingState marketingUpdatedAt } }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "input": {
                "customerId": phone_only_id,
                "emailMarketingConsent": {
                    "marketingState": "SUBSCRIBED",
                    "marketingOptInLevel": "SINGLE_OPT_IN",
                    "consentUpdatedAt": "2026-04-25T02:23:00Z"
                }
            }
        }),
    ));
    assert_eq!(
        email_noop.body["data"]["customerEmailMarketingConsentUpdate"]["customer"]
            ["defaultEmailAddress"],
        Value::Null
    );
    assert_eq!(
        email_noop.body["data"]["customerEmailMarketingConsentUpdate"]["userErrors"],
        json!([])
    );

    let email_only = proxy.process_request(json_graphql_request(
        r#"
        mutation CustomerInputInlineConsentCreate($input: CustomerInput!) {
          customerCreate(input: $input) {
            customer { id defaultEmailAddress { emailAddress } defaultPhoneNumber { phoneNumber } }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "firstName": "Hermes", "lastName": "EmailOnly", "email": "hermes-consent-email-only-errors@example.com" } }),
    ));
    let email_only_id = email_only.body["data"]["customerCreate"]["customer"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let sms_no_phone = proxy.process_request(json_graphql_request(
        r#"
        mutation AnySmsNoPhone($input: CustomerSmsMarketingConsentUpdateInput!) {
          customerSmsMarketingConsentUpdate(input: $input) {
            customer { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "input": {
                "customerId": email_only_id,
                "smsMarketingConsent": {
                    "marketingState": "SUBSCRIBED",
                    "marketingOptInLevel": "SINGLE_OPT_IN",
                    "consentUpdatedAt": "2026-04-25T02:24:00Z"
                }
            }
        }),
    ));
    assert_eq!(
        sms_no_phone.body["data"]["customerSmsMarketingConsentUpdate"]["customer"],
        Value::Null
    );
    assert_eq!(
        sms_no_phone.body["data"]["customerSmsMarketingConsentUpdate"]["userErrors"],
        json!([{
            "field": ["input", "smsMarketingConsent"],
            "message": "A phone number is required to set the SMS consent state.",
            "code": "INVALID"
        }])
    );
}

#[test]
fn customer_by_identifier_supports_id_for_input_validation_downstream_reads() {
    let mut proxy = snapshot_proxy();
    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CustomerInputValidationSeed($input: CustomerInput!) {
          customerCreate(input: $input) {
            customer { id }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "email": "input-validation-downstream@example.com", "phone": "+14155550123" } }),
    ));
    let id = create.body["data"]["customerCreate"]["customer"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation CustomerUpdateParityPlan($input: CustomerInput!) {
          customerUpdate(input: $input) {
            customer { id firstName defaultPhoneNumber { phoneNumber } tags }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "id": id, "firstName": "", "lastName": "", "phone": "", "tags": ["Zulu", "alpha", "spaced tag"] } }),
    ));
    let id = update.body["data"]["customerUpdate"]["customer"]["id"]
        .as_str()
        .unwrap();
    let read = proxy.process_request(json_graphql_request(
        r#"
        query CustomerInputValidationDownstreamRead($id: ID!, $identifier: CustomerIdentifierInput!) {
          customer(id: $id) { id defaultPhoneNumber { phoneNumber } tags }
          customerByIdentifier(identifier: $identifier) { id defaultPhoneNumber { phoneNumber } tags }
        }
        "#,
        json!({ "id": id, "identifier": { "id": id } }),
    ));
    assert_eq!(read.body["data"]["customerByIdentifier"]["id"], json!(id));
    assert_eq!(
        read.body["data"]["customerByIdentifier"]["defaultPhoneNumber"],
        Value::Null
    );
    assert_eq!(
        read.body["data"]["customerByIdentifier"]["tags"],
        json!(["alpha", "spaced tag", "Zulu"])
    );
}

#[test]
fn customer_set_id_and_unknown_identifier_guards_do_not_stage_or_log() {
    let mut proxy = snapshot_proxy();
    let id_not_allowed = proxy.process_request(json_graphql_request(
        r#"
        mutation CustomerSetIdNotAllowed($input: CustomerSetInput!, $identifier: CustomerSetIdentifiers) {
          customerSet(input: $input, identifier: $identifier) {
            customer { id email }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "identifier": { "email": "customer-set-id-not-allowed@example.com" },
            "input": {
                "id": "gid://shopify/Customer/999999999999998",
                "email": "customer-set-id-not-allowed@example.com",
                "firstName": "IdNotAllowed"
            }
        }),
    ));
    assert_eq!(
        id_not_allowed.body["data"]["customerSet"],
        json!({
            "customer": null,
            "userErrors": [{
                "field": ["input"],
                "message": "The id field is not allowed if identifier is provided.",
                "code": "ID_NOT_ALLOWED"
            }]
        })
    );

    let unknown_id_query = r#"
        mutation CustomerSetUnknownIdErrors($input: CustomerSetInput!, $identifier: CustomerSetIdentifiers) {
          customerSet(input: $input, identifier: $identifier) {
            customer { id email }
            userErrors { field message code }
          }
        }
        "#;
    let unknown_id = proxy.process_request(json_graphql_request(
        unknown_id_query,
        json!({
            "identifier": { "id": "gid://shopify/Customer/999999999" },
            "input": { "email": "buyer@example.com" }
        }),
    ));
    assert_eq!(
        unknown_id.body["data"]["customerSet"],
        json!({
            "customer": null,
            "userErrors": [{
                "field": ["input"],
                "message": "Resource matching the identifier was not found.",
                "code": "NOT_FOUND"
            }]
        })
    );

    let arbitrary_unknown_id = proxy.process_request(json_graphql_request(
        unknown_id_query,
        json!({
            "identifier": { "id": "gid://shopify/Customer/999999999999999" },
            "input": { "firstName": "Ghost" }
        }),
    ));
    assert_eq!(
        arbitrary_unknown_id.body["data"]["customerSet"],
        json!({
            "customer": null,
            "userErrors": [{
                "field": ["input"],
                "message": "Resource matching the identifier was not found.",
                "code": "NOT_FOUND"
            }]
        })
    );

    assert_eq!(proxy.get_log_snapshot()["entries"], json!([]));
    assert_eq!(
        proxy.get_state_snapshot()["stagedState"]["products"],
        json!({})
    );
}

#[test]
fn data_sale_opt_out_stages_existing_customer_and_downstream_reads_without_upstream_mutation() {
    let mut proxy = snapshot_proxy();
    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CustomerCreateParityPlan($input: CustomerInput!) {
          customerCreate(input: $input) {
            customer { id email defaultEmailAddress { emailAddress } }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "email": "hermes-data-sale-local@example.com" } }),
    ));
    assert_eq!(create.status, 200);
    let id = create.body["data"]["customerCreate"]["customer"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let mutation_query = r#"
        mutation DataSaleOptOut($email: String!) {
          dataSaleOptOut(email: $email) {
            customerId
            userErrors { field message code }
          }
        }
        "#;
    let opt_out = proxy.process_request(json_graphql_request(
        mutation_query,
        json!({ "email": "hermes-data-sale-local@example.com" }),
    ));
    assert_eq!(opt_out.status, 200);
    assert_eq!(
        opt_out.body["data"]["dataSaleOptOut"],
        json!({ "customerId": id, "userErrors": [] })
    );

    let repeat = proxy.process_request(json_graphql_request(
        mutation_query,
        json!({ "email": "hermes-data-sale-local@example.com" }),
    ));
    assert_eq!(
        repeat.body["data"]["dataSaleOptOut"],
        json!({ "customerId": id, "userErrors": [] })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query DataSaleOptOutDownstream($id: ID!, $identifier: CustomerIdentifierInput!, $query: String!, $first: Int!) {
          customer(id: $id) { id email dataSaleOptOut defaultEmailAddress { emailAddress } }
          customerByIdentifier(identifier: $identifier) { id email dataSaleOptOut defaultEmailAddress { emailAddress } }
          customers(first: $first, query: $query, sortKey: UPDATED_AT, reverse: true) {
            nodes { id email dataSaleOptOut }
            pageInfo { hasNextPage hasPreviousPage }
          }
        }
        "#,
        json!({
            "id": id,
            "identifier": { "id": id },
            "query": "__customer_parity_no_match__",
            "first": 5
        }),
    ));
    let expected_customer = json!({
        "id": id,
        "email": "hermes-data-sale-local@example.com",
        "dataSaleOptOut": true,
        "defaultEmailAddress": { "emailAddress": "hermes-data-sale-local@example.com" }
    });
    assert_eq!(read.body["data"]["customer"], expected_customer);
    assert_eq!(read.body["data"]["customerByIdentifier"], expected_customer);
    assert_eq!(
        read.body["data"]["customers"],
        json!({
            "nodes": [],
            "pageInfo": { "hasNextPage": false, "hasPreviousPage": false }
        })
    );

    let log = proxy.get_log_snapshot();
    assert_eq!(log["entries"][1]["status"], json!("staged"));
    assert_eq!(
        log["entries"][1]["interpreted"]["capability"],
        json!({
            "operationName": "dataSaleOptOut",
            "domain": "privacy",
            "execution": "stage-locally"
        })
    );
    assert!(log["entries"][1]["rawBody"]
        .as_str()
        .unwrap()
        .contains("dataSaleOptOut"));
}

#[test]
fn data_sale_opt_out_resolves_existing_upstream_customer_id_without_forwarding_mutation() {
    let forwarded = Arc::new(Mutex::new(Vec::<String>::new()));
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport({
        let forwarded = Arc::clone(&forwarded);
        move |request| {
            forwarded.lock().unwrap().push(request.body.clone());
            if request.body.contains("DataSaleOptOutCustomerLookup") {
                return Response {
                    status: 200,
                    headers: Default::default(),
                    body: json!({
                        "data": {
                            "customerByIdentifier": {
                                "id": "gid://shopify/Customer/10582642524466",
                                "email": "hermes-data-sale-upstream@example.com",
                                "dataSaleOptOut": false,
                                "defaultEmailAddress": {
                                    "emailAddress": "hermes-data-sale-upstream@example.com"
                                }
                            }
                        }
                    }),
                };
            }
            Response {
                status: 500,
                headers: Default::default(),
                body: json!({ "errors": [{ "message": "mutation should not be forwarded" }] }),
            }
        }
    });

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation DataSaleOptOut($email: String!) {
          dataSaleOptOut(email: $email) {
            customerId
            userErrors { field message code }
          }
        }
        "#,
        json!({ "email": "hermes-data-sale-upstream@example.com" }),
    ));
    assert_eq!(
        response.body["data"]["dataSaleOptOut"],
        json!({
            "customerId": "gid://shopify/Customer/10582642524466",
            "userErrors": []
        })
    );
    let forwarded = forwarded.lock().unwrap();
    assert_eq!(forwarded.len(), 1);
    assert!(forwarded[0].contains("DataSaleOptOutCustomerLookup"));
    assert!(!forwarded[0].contains("mutation DataSaleOptOut"));
}

#[test]
fn data_sale_opt_out_validation_and_sanitization_boundaries_match_captured_shape() {
    let mut proxy = snapshot_proxy();
    let mutation = r#"
        mutation DataSaleOptOut($email: String!) {
          dataSaleOptOut(email: $email) {
            customerId
            userErrors { field message code }
          }
        }
        "#;
    for email in ["not-an-email", "", "   ", "tab\tinside@example.com"] {
        let response =
            proxy.process_request(json_graphql_request(mutation, json!({ "email": email })));
        assert_eq!(response.status, 200);
        assert_eq!(
            response.body["data"]["dataSaleOptOut"],
            json!({
                "customerId": null,
                "userErrors": [{
                    "field": null,
                    "message": "Data sale opt out failed.",
                    "code": "FAILED"
                }]
            })
        );
    }

    let whitespace = proxy.process_request(json_graphql_request(
        mutation,
        json!({ "email": "hermes data\nsale@example.com" }),
    ));
    let id = whitespace.body["data"]["dataSaleOptOut"]["customerId"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(id.starts_with("gid://shopify/Customer/"));
    assert_eq!(
        whitespace.body["data"]["dataSaleOptOut"]["userErrors"],
        json!([])
    );
    let read = proxy.process_request(json_graphql_request(
        r#"
        query DataSaleOptOutWhitespaceDownstream($id: ID!, $identifier: CustomerIdentifierInput!) {
          customer(id: $id) { id email dataSaleOptOut defaultEmailAddress { emailAddress } }
          customerByIdentifier(identifier: $identifier) { id email dataSaleOptOut defaultEmailAddress { emailAddress } }
        }
        "#,
        json!({ "id": id, "identifier": { "id": id } }),
    ));
    assert_eq!(
        read.body["data"]["customer"]["email"],
        json!("hermesdatasale@example.com")
    );
    assert_eq!(
        read.body["data"]["customerByIdentifier"]["dataSaleOptOut"],
        json!(true)
    );
}

#[test]
fn data_sale_opt_out_missing_or_null_email_is_schema_coercion_error() {
    let mut proxy = snapshot_proxy();
    let missing = proxy.process_request(json_graphql_request(
        r#"
        mutation DataSaleOptOutMissingEmail {
          dataSaleOptOut {
            customerId
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(missing.status, 200);
    assert!(missing.body.get("data").is_none());
    assert_eq!(
        missing.body["errors"][0]["extensions"]["code"],
        json!("missingRequiredArguments")
    );

    let explicit_null = proxy.process_request(json_graphql_request(
        r#"
        mutation DataSaleOptOutNullEmail($email: String!) {
          dataSaleOptOut(email: $email) {
            customerId
            userErrors { field message code }
          }
        }
        "#,
        json!({ "email": null }),
    ));
    assert_eq!(explicit_null.status, 200);
    assert!(explicit_null.body.get("data").is_none());
    assert_eq!(
        explicit_null.body["errors"][0]["extensions"]["code"],
        json!("missingRequiredArguments")
    );
}

#[test]
fn data_sale_opt_out_unknown_valid_email_creates_customer_defaults_and_tag_search_read() {
    let mut proxy = snapshot_proxy();
    let mutation = proxy.process_request(json_graphql_request(
        r#"
        mutation DataSaleOptOut($email: String!) {
          dataSaleOptOut(email: $email) {
            customerId
            userErrors { field message code }
          }
        }
        "#,
        json!({ "email": "hermes-data-sale-defaults@example.com" }),
    ));
    let id = mutation.body["data"]["dataSaleOptOut"]["customerId"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        mutation.body["data"]["dataSaleOptOut"]["userErrors"],
        json!([])
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query DataSaleOptOutNewCustomerDefaultsRead($id: ID!) {
          customer(id: $id) {
            id email tags locale verifiedEmail state dataSaleOptOut defaultEmailAddress { emailAddress }
          }
        }
        "#,
        json!({ "id": id }),
    ));
    assert_eq!(
        read.body["data"]["customer"],
        json!({
            "id": id,
            "email": "hermes-data-sale-defaults@example.com",
            "tags": ["created-by-dns-form"],
            "locale": "en",
            "verifiedEmail": true,
            "state": "DISABLED",
            "dataSaleOptOut": true,
            "defaultEmailAddress": { "emailAddress": "hermes-data-sale-defaults@example.com" }
        })
    );

    let search = proxy.process_request(json_graphql_request(
        r#"
        query DataSaleOptOutDnsTagSearch($query: String!, $first: Int!) {
          customers(query: $query, first: $first) {
            nodes { id email tags }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({ "query": "tag:created-by-dns-form", "first": 5 }),
    ));
    assert_eq!(
        search.body["data"]["customers"]["nodes"],
        json!([{
            "id": id,
            "email": "hermes-data-sale-defaults@example.com",
            "tags": ["created-by-dns-form"]
        }])
    );
    assert_eq!(
        search.body["data"]["customers"]["pageInfo"]["hasNextPage"],
        json!(false)
    );
}

#[test]
fn delegate_access_token_create_validates_and_stages_synthetic_secret() {
    let mut proxy = snapshot_proxy();

    let empty = proxy.process_request(json_graphql_request(
        r#"
        mutation DelegateAccessTokenCreateEmptyScopeValidation {
          delegateAccessTokenCreate(input: { delegateAccessScope: [] }) {
            delegateAccessToken { accessToken accessScopes createdAt expiresIn }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        empty.body["data"]["delegateAccessTokenCreate"],
        json!({
            "delegateAccessToken": null,
            "userErrors": [{ "field": null, "message": "The access scope can't be empty.", "code": "EMPTY_ACCESS_SCOPE" }]
        })
    );

    let negative_expires = proxy.process_request(json_graphql_request(
        r#"
        mutation DelegateAccessTokenCreateNegativeExpiresValidation {
          delegateAccessTokenCreate(input: { delegateAccessScope: ["read_products"], expiresIn: -1 }) {
            delegateAccessToken { accessToken accessScopes createdAt expiresIn }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        negative_expires.body["data"]["delegateAccessTokenCreate"],
        json!({
            "delegateAccessToken": null,
            "userErrors": [{ "field": null, "message": "The expires_in value must be greater than 0.", "code": "NEGATIVE_EXPIRES_IN" }]
        })
    );

    let unknown_scope = proxy.process_request(json_graphql_request(
        r#"
        mutation DelegateAccessTokenCreateUnknownScopeValidation {
          delegateAccessTokenCreate(input: { delegateAccessScope: ["fake_scope"] }) {
            delegateAccessToken { accessToken accessScopes createdAt expiresIn }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        unknown_scope.body["data"]["delegateAccessTokenCreate"],
        json!({
            "delegateAccessToken": null,
            "userErrors": [{ "field": null, "message": "The access scope is invalid: fake_scope", "code": "UNKNOWN_SCOPES" }]
        })
    );

    let happy = proxy.process_request(json_graphql_request(
        r#"
        mutation DelegateAccessTokenCreateHappyValidation {
          aliasCreate: delegateAccessTokenCreate(input: { delegateAccessScope: ["read_products"], expiresIn: 300 }) {
            delegateAccessToken { accessToken accessScopes createdAt expiresIn }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        happy.body["data"]["aliasCreate"]["delegateAccessToken"]["accessScopes"],
        json!(["read_products"])
    );
    assert!(
        happy.body["data"]["aliasCreate"]["delegateAccessToken"]["accessToken"]
            .as_str()
            .is_some_and(|token| token.starts_with("shpat_delegate_proxy_"))
    );
    assert_eq!(
        happy.body["data"]["aliasCreate"]["delegateAccessToken"]["createdAt"],
        json!("2026-04-28T02:10:00.000Z")
    );
    assert_eq!(
        happy.body["data"]["aliasCreate"]["delegateAccessToken"]["expiresIn"],
        json!(300)
    );
    assert_eq!(happy.body["data"]["aliasCreate"]["userErrors"], json!([]));
}

#[test]
fn apps_mutations_dispatch_by_root_field_for_ordinary_operation_names() {
    let mut proxy = configured_proxy(
        ReadMode::Snapshot,
        Some(shopify_draft_proxy::proxy::UnsupportedMutationMode::Reject),
    );

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateSub($lineItems: [AppSubscriptionLineItemInput!]!) {
          appSubscriptionCreate(
            name: "Ordinary"
            returnUrl: "https://app.example.test/return"
            test: true
            lineItems: $lineItems
          ) {
            confirmationUrl
            appSubscription { id status }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "lineItems": [{
                "plan": {
                    "appUsagePricingDetails": {
                        "cappedAmount": { "amount": 100, "currencyCode": "USD" },
                        "terms": "usage"
                    }
                }
            }]
        }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["appSubscriptionCreate"],
        json!({
            "confirmationUrl": "https://app.example.test/local-confirmation",
            "appSubscription": {
                "id": "gid://shopify/AppSubscription/expected",
                "status": "ACTIVE"
            },
            "userErrors": []
        })
    );

    let usage = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateUsage($id: ID!) {
          appUsageRecordCreate(
            subscriptionLineItemId: $id
            price: { amount: "1.00", currencyCode: USD }
            description: "ordinary usage"
            idempotencyKey: "ordinary-usage-1"
          ) {
            appUsageRecord { id }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": "gid://shopify/AppSubscriptionLineItem/expected" }),
    ));
    assert_eq!(usage.status, 200);
    assert_eq!(
        usage.body["data"]["appUsageRecordCreate"],
        json!({
            "appUsageRecord": { "id": "gid://shopify/AppUsageRecord/expected" },
            "userErrors": []
        })
    );

    let roots = [
        (
            "CancelSub",
            r#"mutation CancelSub($id: ID!) { appSubscriptionCancel(id: $id) { appSubscription { id status } userErrors { field message } } }"#,
            json!({ "id": "gid://shopify/AppSubscription/expected" }),
            "appSubscriptionCancel",
        ),
        (
            "ExtendTrial",
            r#"mutation ExtendTrial($id: ID!) { appSubscriptionTrialExtend(id: $id, days: 3) { appSubscription { id trialDays } userErrors { field message code } } }"#,
            json!({ "id": "gid://shopify/AppSubscription/expected" }),
            "appSubscriptionTrialExtend",
        ),
        (
            "UpdateLineItem",
            r#"mutation UpdateLineItem($id: ID!) { appSubscriptionLineItemUpdate(id: $id, cappedAmount: { amount: 101, currencyCode: USD }) { appSubscription { id } userErrors { field message } } }"#,
            json!({ "id": "gid://shopify/AppSubscriptionLineItem/expected" }),
            "appSubscriptionLineItemUpdate",
        ),
        (
            "OneTime",
            r#"mutation OneTime { appPurchaseOneTimeCreate(name: "Import", returnUrl: "https://app.example.test/return", price: { amount: 5, currencyCode: USD }, test: false) { appPurchaseOneTime { id test } confirmationUrl userErrors { field message code } } }"#,
            json!({}),
            "appPurchaseOneTimeCreate",
        ),
        (
            "RevokeScopes",
            r#"mutation RevokeScopes { appRevokeAccessScopes(scopes: ["fake_scope"]) { revoked { handle } userErrors { field message code } } }"#,
            json!({}),
            "appRevokeAccessScopes",
        ),
        (
            "CreateDelegate",
            r#"mutation CreateDelegate { delegateAccessTokenCreate(input: { delegateAccessScope: ["read_products"], expiresIn: 300 }) { delegateAccessToken { accessToken } userErrors { field message code } } }"#,
            json!({}),
            "delegateAccessTokenCreate",
        ),
    ];

    let mut delegate_token = String::new();
    for (_name, query, variables, root) in roots {
        let response = proxy.process_request(json_graphql_request(query, variables));
        assert_eq!(response.status, 200, "{root} should dispatch locally");
        assert!(
            response.body["data"][root].is_object(),
            "{root} should return a local payload, got {}",
            response.body
        );
        if root == "delegateAccessTokenCreate" {
            delegate_token = response.body["data"][root]["delegateAccessToken"]["accessToken"]
                .as_str()
                .unwrap()
                .to_string();
        }
    }

    let destroy = proxy.process_request(json_graphql_request(
        r#"
        mutation DestroyDelegate($token: String!) {
          delegateAccessTokenDestroy(accessToken: $token) {
            status
            userErrors { field message code }
          }
        }
        "#,
        json!({ "token": delegate_token }),
    ));
    assert_eq!(destroy.status, 200);
    assert_eq!(
        destroy.body["data"]["delegateAccessTokenDestroy"],
        json!({ "status": true, "userErrors": [] })
    );
}

#[test]
fn app_revoke_access_scopes_validates_atomically_and_updates_current_installation() {
    let mut proxy = snapshot_proxy();

    let unknown = proxy.process_request(json_graphql_request(
        r#"
        mutation AppRevokeAccessScopesFakeScope {
          appRevokeAccessScopes(scopes: ["fake_scope"]) {
            revoked { handle description }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        unknown.body["data"]["appRevokeAccessScopes"],
        json!({
            "revoked": [],
            "userErrors": [{
                "field": ["scopes"],
                "message": "The requested list of scopes to revoke includes invalid handles.",
                "code": "UNKNOWN_SCOPES"
            }]
        })
    );

    let mixed = proxy.process_request(json_graphql_request(
        r#"
        mutation AppRevokeAccessScopesMixedFakeScope {
          appRevokeAccessScopes(scopes: ["read_products", "fake_scope"]) {
            revoked { handle description }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        mixed.body["data"]["appRevokeAccessScopes"]["revoked"],
        json!([])
    );
    assert_eq!(
        mixed.body["data"]["appRevokeAccessScopes"]["userErrors"],
        json!([
            {
                "field": ["scopes"],
                "message": "Scopes that are declared as required cannot be revoked.",
                "code": "CANNOT_REVOKE_REQUIRED_SCOPES"
            },
            {
                "field": ["scopes"],
                "message": "The requested list of scopes to revoke includes invalid handles.",
                "code": "UNKNOWN_SCOPES"
            }
        ])
    );

    let required = proxy.process_request(json_graphql_request(
        r#"
        mutation AppRevokeAccessScopesRequiredReadProducts {
          appRevokeAccessScopes(scopes: ["read_products"]) {
            revoked { handle description }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        required.body["data"]["appRevokeAccessScopes"],
        json!({
            "revoked": [],
            "userErrors": [{
                "field": ["scopes"],
                "message": "Scopes that are declared as required cannot be revoked.",
                "code": "CANNOT_REVOKE_REQUIRED_SCOPES"
            }]
        })
    );

    let missing_source_app = proxy.process_request(json_graphql_request(
        r#"
        mutation AppRevokeAccessScopesErrorCodes {
          appRevokeAccessScopes(scopes: ["write_products"]) {
            revoked { handle description }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        missing_source_app.body["data"]["appRevokeAccessScopes"],
        json!({
            "revoked": [],
            "userErrors": [{ "field": ["base"], "message": "Source app is missing.", "code": "MISSING_SOURCE_APP" }]
        })
    );

    let optional = proxy.process_request(json_graphql_request(
        r#"
        mutation AppRevokeAccessScopesOptionalWriteProducts {
          appRevokeAccessScopes(scopes: ["write_products"]) {
            revoked { handle description }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        optional.body["data"]["appRevokeAccessScopes"],
        json!({
            "revoked": [{ "handle": "write_products", "description": null }],
            "userErrors": []
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query AppAccessScopesLocalRead {
          currentAppInstallation { accessScopes { handle } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        read.body,
        json!({ "data": { "currentAppInstallation": { "accessScopes": [{ "handle": "read_products" }] } } })
    );
}

#[test]
fn app_purchase_one_time_create_validates_and_stages_selected_fields() {
    let mut proxy = snapshot_proxy();

    let blank = proxy.process_request(json_graphql_request(
        r#"
        mutation AppPurchaseOneTimeCreateValidationBlankName {
          create: appPurchaseOneTimeCreate(name: "   ", returnUrl: "https://app.example.test/return", price: { amount: "5.00", currencyCode: USD }, test: true) {
            appPurchaseOneTime { id }
            confirmationUrl
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        blank.body["data"]["create"],
        json!({
            "appPurchaseOneTime": null,
            "confirmationUrl": null,
            "userErrors": [{ "field": ["name"], "message": "Name can't be blank", "code": null }]
        })
    );

    let zero_price = proxy.process_request(json_graphql_request(
        r#"
        mutation AppPurchaseOneTimeCreateValidationZeroPrice {
          appPurchaseOneTimeCreate(name: "Pro", returnUrl: "https://app.example.test/return", price: { amount: "0", currencyCode: USD }, test: true) {
            appPurchaseOneTime { id }
            confirmationUrl
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        zero_price.body["data"]["appPurchaseOneTimeCreate"],
        json!({
            "appPurchaseOneTime": null,
            "confirmationUrl": null,
            "userErrors": [{ "field": null, "message": "Validation failed: Price must be greater than or equal to 0.5", "code": null }]
        })
    );

    let currency_mismatch = proxy.process_request(json_graphql_request(
        r#"
        mutation AppPurchaseOneTimeCreateValidationCurrencyMismatch {
          appPurchaseOneTimeCreate(name: "Pro", returnUrl: "https://app.example.test/return", price: { amount: "5.00", currencyCode: EUR }, test: true) {
            appPurchaseOneTime { id }
            confirmationUrl
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        currency_mismatch.body["data"]["appPurchaseOneTimeCreate"],
        json!({
            "appPurchaseOneTime": null,
            "confirmationUrl": null,
            "userErrors": [{ "field": ["price"], "message": "Currency code must be USD", "code": null }]
        })
    );

    let missing_return_url = proxy.process_request(json_graphql_request(
        r#"
        mutation AppPurchaseOneTimeCreateValidationMissingReturnUrl {
          appPurchaseOneTimeCreate(name: "Pro", price: { amount: "5.00", currencyCode: USD }, test: true) {
            appPurchaseOneTime { id }
            confirmationUrl
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        missing_return_url.body["errors"][0]["extensions"],
        json!({
            "code": "missingRequiredArguments",
            "className": "Field",
            "name": "appPurchaseOneTimeCreate",
            "arguments": "returnUrl"
        })
    );

    let success = proxy.process_request(json_graphql_request(
        r#"
        mutation AppPurchaseOneTimeCreateValidationSuccess {
          appPurchaseOneTimeCreate(name: "HAR-646 valid test", returnUrl: "https://app.example.test/return", price: { amount: "5.00", currencyCode: USD }, test: true) {
            appPurchaseOneTime { id name status test createdAt price { amount currencyCode } }
            confirmationUrl
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        success.body["data"]["appPurchaseOneTimeCreate"],
        json!({
            "appPurchaseOneTime": {
                "id": "gid://shopify/AppPurchaseOneTime/expected",
                "name": "HAR-646 valid test",
                "status": "ACTIVE",
                "test": true,
                "createdAt": "2024-01-01T00:00:00.000Z",
                "price": { "amount": "5.00", "currencyCode": "USD" }
            },
            "confirmationUrl": "https://app.example.test/local-confirmation",
            "userErrors": []
        })
    );
}

#[test]
fn apps_user_errors_are_typed_and_selection_projected() {
    let mut proxy = snapshot_proxy();

    let uninstall = proxy.process_request(json_graphql_request(
        r#"
        mutation {
          appUninstall(input: { id: "gid://shopify/App/missing" }) {
            app { id }
            userErrors {
              __typename
              message
              ... on AppUninstallError { code }
            }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        uninstall.body["data"]["appUninstall"],
        json!({
            "app": null,
            "userErrors": [{
                "__typename": "AppUninstallError",
                "message": "The app cannot be found.",
                "code": "APP_NOT_FOUND"
            }]
        })
    );

    let revoke = proxy.process_request(json_graphql_request(
        r#"
        mutation {
          appRevokeAccessScopes(scopes: ["fake_scope"]) {
            userErrors {
              __typename
              field
              ... on AppRevokeScopeError { code }
            }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        revoke.body["data"]["appRevokeAccessScopes"],
        json!({
            "userErrors": [{
                "__typename": "AppRevokeScopeError",
                "field": ["scopes"],
                "code": "UNKNOWN_SCOPES"
            }]
        })
    );

    let delegate = proxy.process_request(json_graphql_request(
        r#"
        mutation {
          delegateAccessTokenCreate(input: { delegateAccessScope: [] }) {
            userErrors { __typename message }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        delegate.body["data"]["delegateAccessTokenCreate"],
        json!({
            "userErrors": [{
                "__typename": "UserError",
                "message": "The access scope can't be empty."
            }]
        })
    );
}

#[test]
fn app_subscription_create_cancel_and_repeat_cancel_stages_status_transitions() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation AppSubscriptionCreateLocalLifecycle($lineItems: [AppSubscriptionLineItemInput!]!) {
          appSubscriptionCreate(
            name: "Local plan"
            returnUrl: "https://app.example.test/return"
            trialDays: 7
            test: true
            lineItems: $lineItems
          ) {
            confirmationUrl
            appSubscription {
              id
              name
              status
              test
              trialDays
              lineItems {
                id
                plan {
                  pricingDetails {
                    __typename
                    ... on AppUsagePricing {
                      cappedAmount { amount currencyCode }
                      balanceUsed { amount currencyCode }
                      interval
                      terms
                    }
                  }
                }
              }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "lineItems": [{
                "plan": {
                    "appUsagePricingDetails": {
                        "cappedAmount": { "amount": 100, "currencyCode": "USD" },
                        "terms": "usage terms"
                    }
                }
            }]
        }),
    ));
    assert_eq!(
        create.body["data"]["appSubscriptionCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        create.body["data"]["appSubscriptionCreate"]["appSubscription"],
        json!({
            "id": "gid://shopify/AppSubscription/expected",
            "name": "Local plan",
            "status": "ACTIVE",
            "test": true,
            "trialDays": 7,
            "lineItems": [{
                "id": "gid://shopify/AppSubscriptionLineItem/expected",
                "plan": { "pricingDetails": {
                    "__typename": "AppUsagePricing",
                    "cappedAmount": { "amount": "100", "currencyCode": "USD" },
                    "balanceUsed": { "amount": "0.0", "currencyCode": "USD" },
                    "interval": "EVERY_30_DAYS",
                    "terms": "usage terms"
                }}
            }]
        })
    );

    let cancel = proxy.process_request(json_graphql_request(
        r#"
        mutation AppSubscriptionCancelLocalLifecycle($id: ID!) {
          appSubscriptionCancel(id: $id, prorate: true) {
            appSubscription { id status trialDays }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": "gid://shopify/AppSubscription/expected" }),
    ));
    assert_eq!(
        cancel.body["data"]["appSubscriptionCancel"],
        json!({
            "appSubscription": { "id": "gid://shopify/AppSubscription/expected", "status": "CANCELLED", "trialDays": 7 },
            "userErrors": []
        })
    );

    let repeat = proxy.process_request(json_graphql_request(
        r#"
        mutation AppSubscriptionCancelLocalLifecycle($id: ID!) {
          appSubscriptionCancel(id: $id, prorate: true) {
            appSubscription { id status trialDays }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": "gid://shopify/AppSubscription/expected" }),
    ));
    assert_eq!(
        repeat.body["data"]["appSubscriptionCancel"],
        json!({
            "appSubscription": null,
            "userErrors": [{ "field": ["id"], "message": "Cannot transition status via :cancel from :cancelled" }]
        })
    );
}

#[test]
fn app_usage_record_create_caps_idempotency_and_readback_balance() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation AppSubscriptionCreateLocalLifecycle($lineItems: [AppSubscriptionLineItemInput!]!) {
          appSubscriptionCreate(
            name: "Local plan"
            returnUrl: "https://app.example.test/return"
            trialDays: 7
            test: true
            lineItems: $lineItems
          ) {
            appSubscription {
              id
              lineItems {
                id
                plan { pricingDetails { __typename ... on AppUsagePricing { cappedAmount { amount currencyCode } } } }
              }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "lineItems": [{
                "plan": { "appUsagePricingDetails": { "cappedAmount": { "amount": 5, "currencyCode": "USD" }, "terms": "usage terms" } }
            }]
        }),
    ));
    assert_eq!(
        create.body["data"]["appSubscriptionCreate"]["appSubscription"]["lineItems"][0]["id"],
        json!("gid://shopify/AppSubscriptionLineItem/expected")
    );

    let success_query = r#"
        mutation AppUsageRecordCreateCapSuccess($id: ID!) {
          appUsageRecordCreate(
            subscriptionLineItemId: $id
            price: { amount: "3.00", currencyCode: USD }
            description: "first"
            idempotencyKey: "usage-key-cap-1"
          ) {
            appUsageRecord {
              id
              description
              price { amount currencyCode }
              subscriptionLineItem { id plan { pricingDetails { __typename ... on AppUsagePricing { balanceUsed { amount currencyCode } } } } }
            }
            userErrors { field message }
          }
        }
    "#;
    let success = proxy.process_request(json_graphql_request(
        success_query,
        json!({ "id": "gid://shopify/AppSubscriptionLineItem/expected" }),
    ));
    assert_eq!(
        success.body["data"]["appUsageRecordCreate"],
        json!({
            "appUsageRecord": {
                "id": "gid://shopify/AppUsageRecord/expected",
                "description": "first",
                "price": { "amount": "3.00", "currencyCode": "USD" },
                "subscriptionLineItem": {
                    "id": "gid://shopify/AppSubscriptionLineItem/expected",
                    "plan": { "pricingDetails": { "__typename": "AppUsagePricing", "balanceUsed": { "amount": "3.00", "currencyCode": "USD" } } }
                }
            },
            "userErrors": []
        })
    );

    let duplicate = proxy.process_request(json_graphql_request(
        success_query,
        json!({ "id": "gid://shopify/AppSubscriptionLineItem/expected" }),
    ));
    assert_eq!(duplicate.body, success.body);

    let over_cap = proxy.process_request(json_graphql_request(
        r#"
        mutation AppUsageRecordCreateCapOverLimit($id: ID!) {
          appUsageRecordCreate(
            subscriptionLineItemId: $id
            price: { amount: "3.00", currencyCode: USD }
            description: "second"
            idempotencyKey: "usage-key-cap-2"
          ) {
            appUsageRecord { id }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": "gid://shopify/AppSubscriptionLineItem/expected" }),
    ));
    assert_eq!(
        over_cap.body["data"]["appUsageRecordCreate"],
        json!({
            "appUsageRecord": null,
            "userErrors": [{ "field": null, "message": "Total price exceeds balance remaining" }]
        })
    );

    let long_key = proxy.process_request(json_graphql_request(
        r#"
        mutation AppUsageRecordCreateLongIdempotencyKey($id: ID!, $key: String) {
          appUsageRecordCreate(
            subscriptionLineItemId: $id
            price: { amount: "1.00", currencyCode: USD }
            description: "too long"
            idempotencyKey: $key
          ) {
            appUsageRecord { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "id": "gid://shopify/AppSubscriptionLineItem/expected",
            "key": "x".repeat(256)
        }),
    ));
    assert_eq!(
        long_key.body["data"]["appUsageRecordCreate"],
        json!({
            "appUsageRecord": null,
            "userErrors": [{ "field": ["idempotencyKey"], "message": "Idempotency key exceeds the maximum length.", "code": null }]
        })
    );

    let missing_description = proxy.process_request(json_graphql_request(
        r#"
        mutation UsageMissingDescription($id: ID!) {
          appUsageRecordCreate(
            subscriptionLineItemId: $id
            price: { amount: "1.00", currencyCode: USD }
            idempotencyKey: "usage-key-missing-description"
          ) {
            appUsageRecord { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": "gid://shopify/AppSubscriptionLineItem/expected" }),
    ));
    assert_eq!(
        missing_description.body["data"]["appUsageRecordCreate"],
        json!({
            "appUsageRecord": null,
            "userErrors": [{ "field": ["description"], "message": "Description can't be blank", "code": null }]
        })
    );

    let invalid_line_item_id = proxy.process_request(json_graphql_request(
        r#"
        mutation UsageInvalidLineItem {
          appUsageRecordCreate(
            subscriptionLineItemId: "not-a-gid"
            price: { amount: "1.00", currencyCode: USD }
            description: "invalid"
            idempotencyKey: "usage-key-invalid-line-item"
          ) {
            appUsageRecord { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        invalid_line_item_id.body["data"]["appUsageRecordCreate"],
        json!({
            "appUsageRecord": null,
            "userErrors": [{ "field": ["subscriptionLineItemId"], "message": "Invalid id", "code": null }]
        })
    );

    let readback = proxy.process_request(json_graphql_request(
        r#"
        query AppUsageRecordCreateCapRead {
          currentAppInstallation {
            allSubscriptions(first: 5) {
              nodes {
                lineItems {
                  plan { pricingDetails { __typename ... on AppUsagePricing { balanceUsed { amount currencyCode } } } }
                  usageRecords { nodes { id description price { amount currencyCode } } }
                }
              }
            }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        readback.body["data"]["currentAppInstallation"],
        json!({
            "allSubscriptions": { "nodes": [{
                "lineItems": [{
                    "plan": { "pricingDetails": { "__typename": "AppUsagePricing", "balanceUsed": { "amount": "3.00", "currencyCode": "USD" } } },
                    "usageRecords": { "nodes": [{
                        "id": "gid://shopify/AppUsageRecord/expected",
                        "description": "first",
                        "price": { "amount": "3.00", "currencyCode": "USD" }
                    }] }
                }]
            }] }
        })
    );
}

#[test]
fn app_billing_access_local_lifecycle_reads_nodes_and_uninstall_cascade() {
    let mut proxy = snapshot_proxy();

    let create_subscription = proxy.process_request(json_graphql_request(
        r#"
        mutation AppSubscriptionCreateLocalLifecycle($lineItems: [AppSubscriptionLineItemInput!]!) {
          appSubscriptionCreate(name: "Local plan", returnUrl: "https://app.example.test/return", trialDays: 7, test: true, lineItems: $lineItems) {
            appSubscription { id status trialDays lineItems { id } }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "lineItems": [{
                "plan": { "appUsagePricingDetails": { "cappedAmount": { "amount": 100, "currencyCode": "USD" }, "terms": "usage terms" } }
            }]
        }),
    ));
    assert_eq!(
        create_subscription.body["data"]["appSubscriptionCreate"]["appSubscription"]["id"],
        json!("gid://shopify/AppSubscription/expected")
    );

    let one_time = proxy.process_request(json_graphql_request(
        r#"
        mutation AppPurchaseOneTimeCreateLocalLifecycle {
          appPurchaseOneTimeCreate(name: "Import package", returnUrl: "https://app.example.test/return", price: { amount: 10, currencyCode: USD }, test: true) {
            confirmationUrl
            appPurchaseOneTime { id name status test price { amount currencyCode } }
            userErrors { field message }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        one_time.body["data"]["appPurchaseOneTimeCreate"],
        json!({
            "confirmationUrl": "https://app.example.test/local-confirmation",
            "appPurchaseOneTime": {
                "id": "gid://shopify/AppPurchaseOneTime/expected",
                "name": "Import package",
                "status": "ACTIVE",
                "test": true,
                "price": { "amount": "10", "currencyCode": "USD" }
            },
            "userErrors": []
        })
    );

    let mut one_time_test_proxy = snapshot_proxy();
    let one_time_test_false = one_time_test_proxy.process_request(json_graphql_request(
        r#"
        mutation OneTimeTestFalse {
          appPurchaseOneTimeCreate(name: "Import package 2", returnUrl: "https://app.example.test/return", price: { amount: 10, currencyCode: USD }, test: false) {
            appPurchaseOneTime { id test }
            userErrors { field message }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        one_time_test_false.body["data"]["appPurchaseOneTimeCreate"],
        json!({
            "appPurchaseOneTime": {
                "id": "gid://shopify/AppPurchaseOneTime/expected",
                "test": false
            },
            "userErrors": []
        })
    );

    let usage = proxy.process_request(json_graphql_request(
        r#"
        mutation AppUsageRecordCreateLocalLifecycle($id: ID!) {
          appUsageRecordCreate(subscriptionLineItemId: $id, price: { amount: "12.5", currencyCode: USD }, description: "metered import", idempotencyKey: "usage-local-1") {
            appUsageRecord { id description price { amount currencyCode } subscriptionLineItem { id } }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": "gid://shopify/AppSubscriptionLineItem/expected" }),
    ));
    assert_eq!(
        usage.body["data"]["appUsageRecordCreate"]["appUsageRecord"],
        json!({
            "id": "gid://shopify/AppUsageRecord/expected",
            "description": "metered import",
            "price": { "amount": "12.5", "currencyCode": "USD" },
            "subscriptionLineItem": { "id": "gid://shopify/AppSubscriptionLineItem/expected" }
        })
    );

    let expired_trial = proxy.process_request(json_graphql_request(
        r#"
        mutation AppSubscriptionTrialExtendLocalLifecycle($id: ID!) {
          appSubscriptionTrialExtend(id: $id, days: 3) {
            appSubscription { id trialDays }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": "gid://shopify/AppSubscription/expected" }),
    ));
    assert_eq!(
        expired_trial.body["data"]["appSubscriptionTrialExtend"],
        json!({
            "appSubscription": null,
            "userErrors": [{ "field": ["id"], "message": "The trial can't be extended after expiration." }]
        })
    );

    proxy.process_request(json_graphql_request(
        r#"
        mutation AppSubscriptionCancelLocalLifecycle($id: ID!) {
          appSubscriptionCancel(id: $id, prorate: true) { appSubscription { id status trialDays } userErrors { field message } }
        }
        "#,
        json!({ "id": "gid://shopify/AppSubscription/expected" }),
    ));

    let readback = proxy.process_request(json_graphql_request(
        r#"
        query AppBillingLocalRead {
          currentAppInstallation {
            id
            activeSubscriptions { id }
            allSubscriptions(first: 5) { nodes { id status trialDays lineItems { id usageRecords(first: 5) { nodes { description price { amount currencyCode } } } } } }
            oneTimePurchases(first: 5) { nodes { name status price { amount currencyCode } } }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        readback.body["data"]["currentAppInstallation"],
        json!({
            "id": "gid://shopify/AppInstallation/expected",
            "activeSubscriptions": [],
            "allSubscriptions": { "nodes": [{
                "id": "gid://shopify/AppSubscription/expected",
                "status": "CANCELLED",
                "trialDays": 7,
                "lineItems": [{
                    "id": "gid://shopify/AppSubscriptionLineItem/expected",
                    "usageRecords": { "nodes": [{
                        "description": "metered import",
                        "price": { "amount": "12.5", "currencyCode": "USD" }
                    }] }
                }]
            }] },
            "oneTimePurchases": { "nodes": [{
                "name": "Import package",
                "status": "ACTIVE",
                "price": { "amount": "10", "currencyCode": "USD" }
            }] }
        })
    );

    let node_read = proxy.process_request(json_graphql_request(
        r#"
        query AppBillingNodeRead($id: ID!) {
          node(id: $id) {
            ... on AppPurchaseOneTime { id name status test price { amount currencyCode } }
          }
        }
        "#,
        json!({ "id": "gid://shopify/AppPurchaseOneTime/expected" }),
    ));
    assert_eq!(
        node_read.body["data"]["node"],
        json!({
            "id": "gid://shopify/AppPurchaseOneTime/expected",
            "name": "Import package",
            "status": "ACTIVE",
            "test": true,
            "price": { "amount": "10", "currencyCode": "USD" }
        })
    );

    let uninstall = proxy.process_request(json_graphql_request(
        r#"
        mutation AppUninstallLocalLifecycle { appUninstall { app { id handle } userErrors { field message } } }
        "#,
        json!({}),
    ));
    assert_eq!(
        uninstall.body["data"]["appUninstall"],
        json!({
            "app": { "id": "gid://shopify/App/expected", "handle": "shopify-draft-proxy" },
            "userErrors": []
        })
    );

    let after_uninstall = proxy.process_request(json_graphql_request(
        r#"query AppInstallationIdLocalRead { currentAppInstallation { id } }"#,
        json!({}),
    ));
    assert_eq!(
        after_uninstall.body["data"]["currentAppInstallation"],
        Value::Null
    );
}

#[test]
fn app_subscription_line_item_update_validates_recurring_currency_and_amount() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation AppSubscriptionCreateLocalLifecycle($lineItems: [AppSubscriptionLineItemInput!]!) {
          appSubscriptionCreate(
            name: "Local plan"
            returnUrl: "https://app.example.test/return"
            trialDays: 7
            test: true
            lineItems: $lineItems
          ) {
            confirmationUrl
            appSubscription {
              id
              lineItems {
                id
                plan {
                  pricingDetails {
                    __typename
                    ... on AppUsagePricing { cappedAmount { amount currencyCode } }
                    ... on AppRecurringPricing { price { amount currencyCode } }
                  }
                }
              }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "lineItems": [
                { "plan": { "appUsagePricingDetails": { "cappedAmount": { "amount": 5, "currencyCode": "USD" }, "terms": "usage terms" } } },
                { "plan": { "appRecurringPricingDetails": { "price": { "amount": 1, "currencyCode": "USD" }, "interval": "EVERY_30_DAYS" } } }
            ]
        }),
    ));
    assert_eq!(
        create.body["data"]["appSubscriptionCreate"],
        json!({
            "confirmationUrl": "https://app.example.test/local-confirmation",
            "appSubscription": {
                "id": "gid://shopify/AppSubscription/expected",
                "lineItems": [
                    {
                        "id": "gid://shopify/AppSubscriptionLineItem/usage",
                        "plan": { "pricingDetails": {
                            "__typename": "AppUsagePricing",
                            "cappedAmount": { "amount": "5", "currencyCode": "USD" }
                        }}
                    },
                    {
                        "id": "gid://shopify/AppSubscriptionLineItem/recurring",
                        "plan": { "pricingDetails": {
                            "__typename": "AppRecurringPricing",
                            "price": { "amount": "1", "currencyCode": "USD" }
                        }}
                    }
                ]
            },
            "userErrors": []
        })
    );

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation AppSubscriptionLineItemUpdateValidation($usageLineItemId: ID!, $recurringLineItemId: ID!) {
          recurring: appSubscriptionLineItemUpdate(id: $recurringLineItemId, cappedAmount: { amount: 10, currencyCode: USD }) {
            appSubscription { id }
            userErrors { field message }
          }
          currencyMismatch: appSubscriptionLineItemUpdate(id: $usageLineItemId, cappedAmount: { amount: 10, currencyCode: EUR }) {
            appSubscription { id }
            userErrors { field message }
          }
          nonIncreasing: appSubscriptionLineItemUpdate(id: $usageLineItemId, cappedAmount: { amount: 3, currencyCode: USD }) {
            appSubscription { id }
            userErrors { field message }
          }
          success: appSubscriptionLineItemUpdate(id: $usageLineItemId, cappedAmount: { amount: 10, currencyCode: USD }) {
            confirmationUrl
            appSubscription {
              id
              lineItems {
                id
                plan {
                  pricingDetails {
                    __typename
                    ... on AppUsagePricing { cappedAmount { amount currencyCode } }
                    ... on AppRecurringPricing { price { amount currencyCode } }
                  }
                }
              }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "usageLineItemId": "gid://shopify/AppSubscriptionLineItem/usage",
            "recurringLineItemId": "gid://shopify/AppSubscriptionLineItem/recurring"
        }),
    ));

    assert_eq!(
        update.body["data"],
        json!({
            "recurring": {
                "appSubscription": null,
                "userErrors": [{ "field": ["cappedAmount"], "message": "Only usage-pricing line items support cappedAmount updates" }]
            },
            "currencyMismatch": {
                "appSubscription": null,
                "userErrors": [{ "field": null, "message": "Currency code must be USD" }]
            },
            "nonIncreasing": {
                "appSubscription": null,
                "userErrors": [{ "field": ["cappedAmount"], "message": "Spending limit can only be increased. Please contact the app developer to decrease spending limit." }]
            },
            "success": {
                "confirmationUrl": "https://app.example.test/local-confirmation",
                "appSubscription": {
                    "id": "gid://shopify/AppSubscription/expected",
                    "lineItems": [
                        {
                            "id": "gid://shopify/AppSubscriptionLineItem/usage",
                            "plan": { "pricingDetails": {
                                "__typename": "AppUsagePricing",
                                "cappedAmount": { "amount": "5", "currencyCode": "USD" }
                            }}
                        },
                        {
                            "id": "gid://shopify/AppSubscriptionLineItem/recurring",
                            "plan": { "pricingDetails": {
                                "__typename": "AppRecurringPricing",
                                "price": { "amount": "1", "currencyCode": "USD" }
                            }}
                        }
                    ]
                },
                "userErrors": []
            }
        })
    );

    let synchronous_update = proxy.process_request(json_graphql_request(
        r#"
        mutation AppSubscriptionLineItemUpdateNoApproval($usageLineItemId: ID!) {
          appSubscriptionLineItemUpdate(
            id: $usageLineItemId
            cappedAmount: { amount: 12, currencyCode: USD }
            requireApproval: false
          ) {
            confirmationUrl
            appSubscription {
              lineItems {
                id
                plan {
                  pricingDetails {
                    __typename
                    ... on AppUsagePricing { cappedAmount { amount currencyCode } }
                  }
                }
              }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({ "usageLineItemId": "gid://shopify/AppSubscriptionLineItem/usage" }),
    ));
    assert_eq!(
        synchronous_update.body["data"]["appSubscriptionLineItemUpdate"],
        json!({
            "confirmationUrl": null,
            "appSubscription": {
                "lineItems": [
                    {
                        "id": "gid://shopify/AppSubscriptionLineItem/usage",
                        "plan": { "pricingDetails": {
                            "__typename": "AppUsagePricing",
                            "cappedAmount": { "amount": "12", "currencyCode": "USD" }
                        }}
                    },
                    {
                        "id": "gid://shopify/AppSubscriptionLineItem/recurring",
                        "plan": { "pricingDetails": {
                            "__typename": "AppRecurringPricing"
                        }}
                    }
                ]
            },
            "userErrors": []
        })
    );
}

#[test]
fn app_subscription_trial_extend_validates_days_unknown_and_inactive_status() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation AppSubscriptionCreatePendingLocalLifecycle($lineItems: [AppSubscriptionLineItemInput!]!) {
          appSubscriptionCreate(
            name: "Local plan"
            returnUrl: "https://app.example.test/return"
            trialDays: 7
            test: false
            lineItems: $lineItems
          ) {
            appSubscription { id status trialDays }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "lineItems": [{
                "plan": {
                    "appUsagePricingDetails": {
                        "cappedAmount": { "amount": 100, "currencyCode": "USD" },
                        "terms": "usage terms"
                    }
                }
            }]
        }),
    ));
    assert_eq!(
        create.body["data"]["appSubscriptionCreate"],
        json!({
            "appSubscription": {
                "id": "gid://shopify/AppSubscription/expected",
                "status": "PENDING",
                "trialDays": 7
            },
            "userErrors": []
        })
    );

    let trial_extend_query = r#"
        mutation AppSubscriptionTrialExtendValidation($id: ID!, $days: Int!) {
          appSubscriptionTrialExtend(id: $id, days: $days) {
            appSubscription { id trialDays }
            userErrors { field message code }
          }
        }
    "#;

    let days_zero = proxy.process_request(json_graphql_request(
        trial_extend_query,
        json!({ "id": "gid://shopify/AppSubscription/expected", "days": 0 }),
    ));
    assert_eq!(
        days_zero.body["data"]["appSubscriptionTrialExtend"],
        json!({
            "appSubscription": null,
            "userErrors": [{ "field": ["days"], "message": "Days must be greater than 0", "code": null }]
        })
    );

    let days_too_large = proxy.process_request(json_graphql_request(
        trial_extend_query,
        json!({ "id": "gid://shopify/AppSubscription/expected", "days": 1001 }),
    ));
    assert_eq!(
        days_too_large.body["data"]["appSubscriptionTrialExtend"],
        json!({
            "appSubscription": null,
            "userErrors": [{ "field": ["days"], "message": "Days must be less than or equal to 1000", "code": null }]
        })
    );

    let unknown = proxy.process_request(json_graphql_request(
        trial_extend_query,
        json!({ "id": "gid://shopify/AppSubscription/unknown", "days": 5 }),
    ));
    assert_eq!(
        unknown.body["data"]["appSubscriptionTrialExtend"],
        json!({
            "appSubscription": null,
            "userErrors": [{ "field": ["id"], "message": "The app subscription wasn't found.", "code": "SUBSCRIPTION_NOT_FOUND" }]
        })
    );

    let pending = proxy.process_request(json_graphql_request(
        trial_extend_query,
        json!({ "id": "gid://shopify/AppSubscription/expected", "days": 5 }),
    ));
    assert_eq!(
        pending.body["data"]["appSubscriptionTrialExtend"],
        json!({
            "appSubscription": null,
            "userErrors": [{ "field": ["id"], "message": "The trial can't be extended on inactive app subscriptions.", "code": "SUBSCRIPTION_NOT_ACTIVE" }]
        })
    );
}

#[test]
fn app_subscription_create_activates_test_charge_and_reads_back_current_installation() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation AppSubscriptionCreateActivationReadback {
          subscription: appSubscriptionCreate(
            name: "Activation readback plan"
            returnUrl: "https://app.example.test/return"
            trialDays: 7
            test: true
            lineItems: [
              { plan: { appRecurringPricingDetails: { price: { amount: "10.00", currencyCode: USD }, interval: EVERY_30_DAYS } } }
            ]
          ) {
            confirmationUrl
            appSubscription { id status test trialDays currentPeriodEnd }
            userErrors { field message }
          }
        }
        "#,
        json!({}),
    ));
    let subscription_id = create.body["data"]["subscription"]["appSubscription"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        create.body["data"]["subscription"],
        json!({
            "confirmationUrl": "https://app.example.test/local-confirmation",
            "appSubscription": {
                "id": subscription_id,
                "status": "ACTIVE",
                "test": true,
                "trialDays": 7,
                "currentPeriodEnd": "2024-02-07T00:00:00.000Z"
            },
            "userErrors": []
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query AppSubscriptionActivationRead {
          installation: currentAppInstallation {
            activeSubscriptions { id status currentPeriodEnd }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        read.body,
        json!({
            "data": {
                "installation": {
                    "activeSubscriptions": [{
                        "id": subscription_id,
                        "status": "ACTIVE",
                        "currentPeriodEnd": "2024-02-07T00:00:00.000Z"
                    }]
                }
            }
        })
    );
}

#[test]
fn fulfillment_service_lifecycle_stages_location_reads_deletes_and_validates() {
    let mut proxy = snapshot_proxy();
    let invalid = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateBlank($name: String!, $callbackUrl: URL) {
          fulfillmentServiceCreate(
            name: $name
            callbackUrl: $callbackUrl
            trackingSupport: true
            inventoryManagement: true
            requiresShippingMethod: true
          ) {
            fulfillmentService { id }
            userErrors { field message }
          }
        }
        "#,
        json!({ "name": "", "callbackUrl": "https://example.com/fulfillment-service/moeomiux" }),
    ));
    assert_eq!(
        invalid.body["data"]["fulfillmentServiceCreate"],
        json!({
            "fulfillmentService": null,
            "userErrors": [
                { "field": ["name"], "message": "Name can't be blank" },
                { "field": ["callbackUrl"], "message": "Callback url is not allowed" }
            ]
        })
    );

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateFs($name: String!) {
          fulfillmentServiceCreate(name: $name, trackingSupport: true, inventoryManagement: true, requiresShippingMethod: true) {
            fulfillmentService {
              id handle serviceName callbackUrl trackingSupport inventoryManagement requiresShippingMethod type
              location { id name isFulfillmentService fulfillsOnlineOrders shipsInventory }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({ "name": "Hermes FS moeompnx" }),
    ));
    let service_id = create.body["data"]["fulfillmentServiceCreate"]["fulfillmentService"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let location_id = create.body["data"]["fulfillmentServiceCreate"]["fulfillmentService"]
        ["location"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        create.body["data"]["fulfillmentServiceCreate"]["fulfillmentService"],
        json!({
            "id": service_id,
            "handle": "hermes-fs-moeompnx",
            "serviceName": "Hermes FS moeompnx",
            "callbackUrl": null,
            "trackingSupport": true,
            "inventoryManagement": true,
            "requiresShippingMethod": true,
            "type": "THIRD_PARTY",
            "location": {
                "id": location_id,
                "name": "Hermes FS moeompnx",
                "isFulfillmentService": true,
                "fulfillsOnlineOrders": true,
                "shipsInventory": false
            }
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query FulfillmentServiceAfterCreate($id: ID!, $locationId: ID!) {
          fulfillmentService(id: $id) {
            id handle serviceName callbackUrl trackingSupport inventoryManagement requiresShippingMethod type
            location { id name isFulfillmentService fulfillsOnlineOrders shipsInventory }
          }
          location(id: $locationId) { id name isFulfillmentService fulfillsOnlineOrders shipsInventory }
        }
        "#,
        json!({ "id": service_id, "locationId": location_id }),
    ));
    assert_eq!(
        read.body["data"]["fulfillmentService"],
        create.body["data"]["fulfillmentServiceCreate"]["fulfillmentService"]
    );
    assert_eq!(
        read.body["data"]["location"],
        create.body["data"]["fulfillmentServiceCreate"]["fulfillmentService"]["location"]
    );

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateFs($id: ID!, $name: String!) {
          fulfillmentServiceUpdate(id: $id, name: $name, trackingSupport: false, inventoryManagement: false, requiresShippingMethod: false) {
            fulfillmentService {
              id handle serviceName callbackUrl trackingSupport inventoryManagement requiresShippingMethod type
              location { id name isFulfillmentService fulfillsOnlineOrders shipsInventory }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": service_id, "name": "Hermes FS Updated moeompnx" }),
    ));
    assert_eq!(
        update.body["data"]["fulfillmentServiceUpdate"]["fulfillmentService"]["serviceName"],
        json!("Hermes FS Updated moeompnx")
    );
    assert_eq!(
        update.body["data"]["fulfillmentServiceUpdate"]["fulfillmentService"]["location"]["name"],
        json!("Hermes FS Updated moeompnx")
    );

    let delete = proxy.process_request(json_graphql_request(
        r#"
        mutation DeleteFs($id: ID!) {
          fulfillmentServiceDelete(id: $id, inventoryAction: DELETE) { deletedId userErrors { field message } }
        }
        "#,
        json!({ "id": service_id }),
    ));
    assert_eq!(
        delete.body["data"]["fulfillmentServiceDelete"],
        json!({ "deletedId": service_id.replace("?id=true", ""), "userErrors": [] })
    );

    let after_delete = proxy.process_request(json_graphql_request(
        r#"
        query Loc($id: ID!) { location(id: $id) { id name isFulfillmentService isActive } }
        "#,
        json!({ "id": location_id }),
    ));
    assert_eq!(after_delete.body["data"]["location"], json!(null));

    let unknown_update = proxy.process_request(json_graphql_request(
        r#"
        mutation UnknownUpdate($id: ID!) {
          fulfillmentServiceUpdate(id: $id, name: "Nope") { fulfillmentService { id } userErrors { field message } }
        }
        "#,
        json!({ "id": "gid://shopify/FulfillmentService/999999999999" }),
    ));
    assert_eq!(
        unknown_update.body["data"]["fulfillmentServiceUpdate"],
        json!({
            "fulfillmentService": null,
            "userErrors": [{ "field": ["id"], "message": "Fulfillment service could not be found." }]
        })
    );
}

#[test]
fn fulfillment_service_create_rejects_removed_public_schema_arguments_before_staging() {
    for removed_argument in [
        "permitsSkuSharing",
        "inventorySyncEnabled",
        "fulfillmentOrdersOptIn",
    ] {
        let mut proxy = snapshot_proxy();
        let mutation = format!(
            "mutation FulfillmentServiceRemovedArgumentValidation($name: String!) {{\n  fulfillmentServiceCreate(\n    name: $name\n    {removed_argument}: false\n    trackingSupport: true\n    inventoryManagement: true\n    requiresShippingMethod: true\n  ) {{\n    fulfillmentService {{\n      id\n      serviceName\n      inventoryManagement\n      requiresShippingMethod\n      trackingSupport\n    }}\n    userErrors {{ field message }}\n  }}\n}}\n"
        );

        let response = proxy.process_request(json_graphql_request(
            &mutation,
            json!({ "name": format!("FS Removed Arg {removed_argument}") }),
        ));
        assert_eq!(response.status, 200);
        assert!(response.body.get("data").is_none(), "{removed_argument}");
        assert_eq!(
            response.body["errors"],
            json!([{
                "message": format!(
                    "Field 'fulfillmentServiceCreate' doesn't accept argument '{removed_argument}'"
                ),
                "locations": [{ "line": 4, "column": 5 }],
                "path": [
                    "mutation FulfillmentServiceRemovedArgumentValidation",
                    "fulfillmentServiceCreate",
                    removed_argument
                ],
                "extensions": {
                    "code": "argumentNotAccepted",
                    "name": "fulfillmentServiceCreate",
                    "typeName": "Field",
                    "argumentName": removed_argument
                }
            }]),
            "{removed_argument}"
        );

        let log = proxy.process_request(Request {
            method: "GET".to_string(),
            path: "/__meta/log".to_string(),
            headers: Default::default(),
            body: String::new(),
        });
        assert_eq!(log.body["entries"], json!([]), "{removed_argument}");

        let state = proxy.process_request(Request {
            method: "GET".to_string(),
            path: "/__meta/state".to_string(),
            headers: Default::default(),
            body: String::new(),
        });
        assert_eq!(
            state.body["stagedState"]["locations"],
            json!({}),
            "{removed_argument}"
        );
    }
}

#[test]
fn fulfillment_service_name_whitespace_validation_rejects_without_staging_or_logging() {
    let mut proxy = snapshot_proxy();
    let create_query = r#"
        mutation FulfillmentServiceNameWhitespaceCreate($name: String!) {
          fulfillmentServiceCreate(
            name: $name
            trackingSupport: true
            inventoryManagement: true
            requiresShippingMethod: true
          ) {
            fulfillmentService { id serviceName location { id name } }
            userErrors { field message }
          }
        }
    "#;
    let update_query = r#"
        mutation FulfillmentServiceNameWhitespaceUpdate($id: ID!, $name: String!) {
          fulfillmentServiceUpdate(id: $id, name: $name) {
            fulfillmentService { id serviceName location { id name } }
            userErrors { field message }
          }
        }
    "#;

    let padded_create = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "name": "  FS Whitespace rejected  " }),
    ));
    assert_eq!(
        padded_create.body["data"]["fulfillmentServiceCreate"],
        json!({
            "fulfillmentService": null,
            "userErrors": [
                { "field": ["name"], "message": "Name cannot begin with a whitespace character" },
                { "field": ["name"], "message": "Name cannot end with a whitespace character" }
            ]
        })
    );
    assert_eq!(
        proxy.get_log_snapshot()["entries"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    let leading_create = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "name": "\tFS Leading Whitespace rejected" }),
    ));
    assert_eq!(
        leading_create.body["data"]["fulfillmentServiceCreate"],
        json!({
            "fulfillmentService": null,
            "userErrors": [
                { "field": ["name"], "message": "Name cannot begin with a whitespace character" }
            ]
        })
    );

    let trailing_create = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "name": "FS Trailing Whitespace rejected\n" }),
    ));
    assert_eq!(
        trailing_create.body["data"]["fulfillmentServiceCreate"],
        json!({
            "fulfillmentService": null,
            "userErrors": [
                { "field": ["name"], "message": "Name cannot end with a whitespace character" }
            ]
        })
    );
    assert_eq!(
        proxy.get_log_snapshot()["entries"]
            .as_array()
            .unwrap()
            .len(),
        0
    );

    let valid_create = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "name": "FS Whitespace Update Source" }),
    ));
    let service_id = valid_create.body["data"]["fulfillmentServiceCreate"]["fulfillmentService"]
        ["id"]
        .as_str()
        .unwrap()
        .to_string();
    let location_id = valid_create.body["data"]["fulfillmentServiceCreate"]["fulfillmentService"]
        ["location"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(service_id.starts_with("gid://shopify/FulfillmentService/1"));
    assert!(location_id.starts_with("gid://shopify/Location/2"));
    assert_eq!(
        proxy.get_log_snapshot()["entries"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    let update_log_len_before = proxy.get_log_snapshot()["entries"]
        .as_array()
        .unwrap()
        .len();
    let update_state_before = proxy.get_state_snapshot();
    let leading_update = proxy.process_request(json_graphql_request(
        update_query,
        json!({ "id": service_id, "name": " FS Whitespace Update Rejected" }),
    ));
    assert_eq!(
        leading_update.body["data"]["fulfillmentServiceUpdate"],
        json!({
            "fulfillmentService": null,
            "userErrors": [
                { "field": ["name"], "message": "Name cannot begin with a whitespace character" }
            ]
        })
    );
    assert_eq!(
        proxy.get_log_snapshot()["entries"]
            .as_array()
            .unwrap()
            .len(),
        update_log_len_before
    );
    assert_eq!(proxy.get_state_snapshot(), update_state_before);

    let trailing_update = proxy.process_request(json_graphql_request(
        update_query,
        json!({ "id": service_id, "name": "FS Whitespace Update Rejected " }),
    ));
    assert_eq!(
        trailing_update.body["data"]["fulfillmentServiceUpdate"],
        json!({
            "fulfillmentService": null,
            "userErrors": [
                { "field": ["name"], "message": "Name cannot end with a whitespace character" }
            ]
        })
    );
    assert_eq!(
        proxy.get_log_snapshot()["entries"]
            .as_array()
            .unwrap()
            .len(),
        update_log_len_before
    );
    assert_eq!(proxy.get_state_snapshot(), update_state_before);
}

#[test]
fn fulfillment_service_callback_url_validation_matches_captured_shopify_behavior() {
    let mut proxy = DraftProxy::new(Config {
        read_mode: ReadMode::Snapshot,
        unsupported_mutation_mode: None,
        bulk_operation_run_mutation_max_input_file_size_bytes: None,
        port: 0,
        shopify_admin_origin: "https://harry-test-heelo.myshopify.com".to_string(),
        snapshot_path: None,
    });

    let primary = proxy.process_request(json_graphql_request(
        r#"
        mutation FulfillmentServiceCallbackUrlValidation(
          $validHttpsName: String!
          $validHttpsCallbackUrl: URL!
          $validHttpName: String!
          $validHttpCallbackUrl: URL!
          $originName: String!
          $originCallbackUrl: URL!
          $ftpName: String!
          $ftpCallbackUrl: URL!
          $exampleName: String!
          $exampleCallbackUrl: URL!
          $shopifyName: String!
          $shopifyCallbackUrl: URL!
        ) {
          validHttpsCreate: fulfillmentServiceCreate(name: $validHttpsName, callbackUrl: $validHttpsCallbackUrl, trackingSupport: false, inventoryManagement: false, requiresShippingMethod: false) {
            fulfillmentService { id handle serviceName callbackUrl trackingSupport inventoryManagement requiresShippingMethod type location { id name isFulfillmentService fulfillsOnlineOrders shipsInventory } }
            userErrors { field message }
          }
          validHttpCreate: fulfillmentServiceCreate(name: $validHttpName, callbackUrl: $validHttpCallbackUrl, trackingSupport: false, inventoryManagement: false, requiresShippingMethod: false) {
            fulfillmentService { id handle serviceName callbackUrl trackingSupport inventoryManagement requiresShippingMethod type location { id name isFulfillmentService fulfillsOnlineOrders shipsInventory } }
            userErrors { field message }
          }
          originCreate: fulfillmentServiceCreate(name: $originName, callbackUrl: $originCallbackUrl, trackingSupport: false, inventoryManagement: false, requiresShippingMethod: false) {
            fulfillmentService { id handle serviceName callbackUrl trackingSupport inventoryManagement requiresShippingMethod type location { id name isFulfillmentService fulfillsOnlineOrders shipsInventory } }
            userErrors { field message }
          }
          ftpCreate: fulfillmentServiceCreate(name: $ftpName, callbackUrl: $ftpCallbackUrl, trackingSupport: false, inventoryManagement: false, requiresShippingMethod: false) {
            fulfillmentService { id callbackUrl }
            userErrors { field message }
          }
          exampleCreate: fulfillmentServiceCreate(name: $exampleName, callbackUrl: $exampleCallbackUrl, trackingSupport: false, inventoryManagement: false, requiresShippingMethod: false) {
            fulfillmentService { id callbackUrl }
            userErrors { field message }
          }
          shopifyCreate: fulfillmentServiceCreate(name: $shopifyName, callbackUrl: $shopifyCallbackUrl, trackingSupport: false, inventoryManagement: false, requiresShippingMethod: false) {
            fulfillmentService { id callbackUrl }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "validHttpsName": "Hermes Callback HTTPS 1778113515444",
            "validHttpsCallbackUrl": "https://mock.shop/fulfillment-service-callback",
            "validHttpName": "Hermes Callback HTTP 1778113515444",
            "validHttpCallbackUrl": "http://mock.shop/fulfillment-service-callback",
            "originName": "Hermes Callback Origin 1778113515444",
            "originCallbackUrl": "https://harry-test-heelo.myshopify.com/fulfillment-service-callback",
            "ftpName": "Hermes Callback FTP 1778113515444",
            "ftpCallbackUrl": "ftp://mock.shop/fulfillment-service-callback",
            "exampleName": "Hermes Callback Example 1778113515444",
            "exampleCallbackUrl": "https://example.com/fulfillment-service-callback",
            "shopifyName": "Hermes Callback Shopify 1778113515444",
            "shopifyCallbackUrl": "https://shopify.com/fulfillment-service-callback"
        }),
    ));

    for (key, callback_url) in [
        (
            "validHttpsCreate",
            "https://mock.shop/fulfillment-service-callback",
        ),
        (
            "validHttpCreate",
            "http://mock.shop/fulfillment-service-callback",
        ),
        (
            "originCreate",
            "https://harry-test-heelo.myshopify.com/fulfillment-service-callback",
        ),
    ] {
        assert_eq!(
            primary.body["data"][key]["userErrors"],
            json!([]),
            "{key} should be accepted"
        );
        assert_eq!(
            primary.body["data"][key]["fulfillmentService"]["callbackUrl"],
            json!(callback_url)
        );
    }
    assert_eq!(
        primary.body["data"]["ftpCreate"],
        json!({
            "fulfillmentService": null,
            "userErrors": [{
                "field": ["callbackUrl"],
                "message": "Callback url protocol ftp:// is not supported"
            }]
        })
    );
    assert_eq!(
        primary.body["data"]["exampleCreate"],
        json!({
            "fulfillmentService": null,
            "userErrors": [{
                "field": ["callbackUrl"],
                "message": "Callback url is not allowed"
            }]
        })
    );
    assert_eq!(
        primary.body["data"]["shopifyCreate"],
        json!({
            "fulfillmentService": null,
            "userErrors": [{
                "field": ["callbackUrl"],
                "message": "Callback url is not allowed"
            }]
        })
    );

    let service_id = primary.body["data"]["validHttpsCreate"]["fulfillmentService"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let update_allowed = proxy.process_request(json_graphql_request(
        r#"
        mutation FulfillmentServiceCallbackUrlValidationUpdateAllowed($id: ID!, $callbackUrl: URL!) {
          fulfillmentServiceUpdate(id: $id, callbackUrl: $callbackUrl) {
            fulfillmentService { id callbackUrl }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "id": service_id,
            "callbackUrl": "http://mock.shop/fulfillment-service-callback-updated"
        }),
    ));
    assert_eq!(
        update_allowed.body["data"]["fulfillmentServiceUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        update_allowed.body["data"]["fulfillmentServiceUpdate"]["fulfillmentService"]
            ["callbackUrl"],
        json!("http://mock.shop/fulfillment-service-callback-updated")
    );

    let update_disallowed = proxy.process_request(json_graphql_request(
        r#"
        mutation FulfillmentServiceCallbackUrlValidationUpdateDisallowed($id: ID!, $callbackUrl: URL!) {
          fulfillmentServiceUpdate(id: $id, callbackUrl: $callbackUrl) {
            fulfillmentService { id callbackUrl }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "id": primary.body["data"]["validHttpCreate"]["fulfillmentService"]["id"],
            "callbackUrl": "https://example.com/fulfillment-service-callback-updated"
        }),
    ));
    assert_eq!(
        update_disallowed.body["data"]["fulfillmentServiceUpdate"],
        json!({
            "fulfillmentService": null,
            "userErrors": [{
                "field": ["callbackUrl"],
                "message": "Callback url is not allowed"
            }]
        })
    );

    let update_protocol = proxy.process_request(json_graphql_request(
        r#"
        mutation FulfillmentServiceCallbackUrlValidationUpdateProtocol($id: ID!, $callbackUrl: URL!) {
          fulfillmentServiceUpdate(id: $id, callbackUrl: $callbackUrl) {
            fulfillmentService { id callbackUrl }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "id": primary.body["data"]["originCreate"]["fulfillmentService"]["id"],
            "callbackUrl": "ftp://mock.shop/fulfillment-service-callback-updated"
        }),
    ));
    assert_eq!(
        update_protocol.body["data"]["fulfillmentServiceUpdate"],
        json!({
            "fulfillmentService": null,
            "userErrors": [{
                "field": ["callbackUrl"],
                "message": "Callback url protocol ftp:// is not supported"
            }]
        })
    );
}

#[test]
fn fulfillment_service_uniqueness_rejects_name_handle_and_reserved_collisions() {
    let mut proxy = snapshot_proxy();

    let create_query = r#"
        mutation FulfillmentServiceUniquenessCreate($name: String!) {
          fulfillmentServiceCreate(
            name: $name
            trackingSupport: true
            inventoryManagement: true
            requiresShippingMethod: true
          ) {
            fulfillmentService {
              id handle serviceName callbackUrl trackingSupport inventoryManagement requiresShippingMethod type
              location { id name isFulfillmentService fulfillsOnlineOrders shipsInventory }
            }
            userErrors { field message }
          }
        }
    "#;
    let update_query = r#"
        mutation FulfillmentServiceUniquenessUpdate($id: ID!, $name: String!) {
          fulfillmentServiceUpdate(
            id: $id
            name: $name
            trackingSupport: false
            inventoryManagement: false
            requiresShippingMethod: false
          ) {
            fulfillmentService {
              id handle serviceName callbackUrl trackingSupport inventoryManagement requiresShippingMethod type
              location { id name isFulfillmentService fulfillsOnlineOrders shipsInventory }
            }
            userErrors { field message }
          }
        }
    "#;

    let create_a = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "name": "FS Unique Acme fsuniq-mowo6bal" }),
    ));
    let service_a = &create_a.body["data"]["fulfillmentServiceCreate"]["fulfillmentService"];
    assert!(service_a["id"]
        .as_str()
        .unwrap()
        .starts_with("gid://shopify/FulfillmentService/"));
    assert!(service_a["location"]["id"]
        .as_str()
        .unwrap()
        .starts_with("gid://shopify/Location/"));
    assert_eq!(
        service_a,
        &json!({
            "id": service_a["id"],
            "handle": "fs-unique-acme-fsuniq-mowo6bal",
            "serviceName": "FS Unique Acme fsuniq-mowo6bal",
            "callbackUrl": null,
            "trackingSupport": true,
            "inventoryManagement": true,
            "requiresShippingMethod": true,
            "type": "THIRD_PARTY",
            "location": {
                "id": service_a["location"]["id"],
                "name": "FS Unique Acme fsuniq-mowo6bal",
                "isFulfillmentService": true,
                "fulfillsOnlineOrders": true,
                "shipsInventory": false
            }
        })
    );

    for duplicate_name in [
        "FS Unique Acme fsuniq-mowo6bal",
        "FS UNIQUE ACME FSUNIQ-MOWO6BAL",
    ] {
        let duplicate = proxy.process_request(json_graphql_request(
            create_query,
            json!({ "name": duplicate_name }),
        ));
        assert_eq!(
            duplicate.body["data"]["fulfillmentServiceCreate"],
            json!({
                "fulfillmentService": null,
                "userErrors": [{ "field": ["name"], "message": "Name has already been taken" }]
            })
        );
    }

    let spaced = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "name": "FS Unique AB fsuniq-mowo6bal" }),
    ));
    assert_eq!(
        spaced.body["data"]["fulfillmentServiceCreate"]["fulfillmentService"]["handle"],
        json!("fs-unique-ab-fsuniq-mowo6bal")
    );

    let handle_collision = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "name": "fs-unique-ab-fsuniq-mowo6bal" }),
    ));
    assert_eq!(
        handle_collision.body["data"]["fulfillmentServiceCreate"],
        json!({
            "fulfillmentService": null,
            "userErrors": [{ "field": ["name"], "message": "Name has already been taken" }]
        })
    );

    let diacritic = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "name": "FS Unique Café__3PL fsuniq-mowo6bal!!!" }),
    ));
    assert_eq!(
        diacritic.body["data"]["fulfillmentServiceCreate"]["fulfillmentService"]["handle"],
        json!("fs-unique-cafe__3pl-fsuniq-mowo6bal")
    );

    for reserved_name in ["Manual", "Gift_Card", "Shopify", "Amazon"] {
        let log_len_before = proxy.get_log_snapshot()["entries"]
            .as_array()
            .unwrap()
            .len();
        let reserved = proxy.process_request(json_graphql_request(
            create_query,
            json!({ "name": reserved_name }),
        ));
        assert_eq!(
            reserved.body["data"]["fulfillmentServiceCreate"],
            json!({
                "fulfillmentService": null,
                "userErrors": [{ "field": ["name"], "message": "Name is reserved" }]
            })
        );
        assert_eq!(
            proxy.get_log_snapshot()["entries"]
                .as_array()
                .unwrap()
                .len(),
            log_len_before
        );
    }

    proxy.process_request(json_graphql_request(
        create_query,
        json!({ "name": "FS Unique Source fsuniq-mowo6bal" }),
    ));
    let target = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "name": "FS Unique Target fsuniq-mowo6bal" }),
    ));
    let target_id = target.body["data"]["fulfillmentServiceCreate"]["fulfillmentService"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let update_duplicate = proxy.process_request(json_graphql_request(
        update_query,
        json!({ "id": target_id, "name": "FS Unique Source fsuniq-mowo6bal" }),
    ));
    assert_eq!(
        update_duplicate.body["data"]["fulfillmentServiceUpdate"],
        json!({
            "fulfillmentService": null,
            "userErrors": [{ "field": ["name"], "message": "Name has already been taken" }]
        })
    );

    for reserved_name in ["Manual", "Gift_Card", "Shopify", "Amazon"] {
        let log_len_before = proxy.get_log_snapshot()["entries"]
            .as_array()
            .unwrap()
            .len();
        let update_reserved = proxy.process_request(json_graphql_request(
            update_query,
            json!({ "id": target_id, "name": reserved_name }),
        ));
        assert_eq!(
            update_reserved.body["data"]["fulfillmentServiceUpdate"],
            json!({
                "fulfillmentService": null,
                "userErrors": [{ "field": ["name"], "message": "Name is reserved" }]
            })
        );
        assert_eq!(
            proxy.get_log_snapshot()["entries"]
                .as_array()
                .unwrap()
                .len(),
            log_len_before
        );
    }
}

#[test]
fn carrier_service_lifecycle_stages_reads_filters_deletes_and_validates() {
    let mut proxy = snapshot_proxy();
    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CarrierServiceCreateProbe($input: DeliveryCarrierServiceCreateInput!) {
          carrierServiceCreate(input: $input) {
            carrierService { id name formattedName callbackUrl active supportsServiceDiscovery }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": {
            "name": "Hermes Carrier Local",
            "callbackUrl": "https://mock.shop/carrier-service-rates",
            "supportsServiceDiscovery": true,
            "active": false
        }}),
    ));
    let id = create.body["data"]["carrierServiceCreate"]["carrierService"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(id.starts_with("gid://shopify/DeliveryCarrierService/"));
    assert_eq!(
        create.body["data"]["carrierServiceCreate"]["carrierService"]["formattedName"],
        json!("Hermes Carrier Local (Rates provided by app)")
    );

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation CarrierServiceUpdateProbe($input: DeliveryCarrierServiceUpdateInput!) {
          carrierServiceUpdate(input: $input) {
            carrierService { id name formattedName callbackUrl active supportsServiceDiscovery }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": {
            "id": id,
            "name": "Hermes Carrier Updated",
            "callbackUrl": "https://mock.shop/carrier-service-rates-updated",
            "supportsServiceDiscovery": false,
            "active": true
        }}),
    ));
    assert_eq!(
        update.body["data"]["carrierServiceUpdate"]["carrierService"]["name"],
        json!("Hermes Carrier Updated")
    );

    let downstream = proxy.process_request(json_graphql_request(
        r#"
        query CarrierServiceAfterUpdate($id: ID!, $first: Int!, $activeQuery: String) {
          carrierService(id: $id) { id name formattedName callbackUrl active supportsServiceDiscovery }
          active: carrierServices(first: $first, query: $activeQuery, sortKey: ID) {
            nodes { id name formattedName callbackUrl active supportsServiceDiscovery }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({ "id": id, "first": 5, "activeQuery": "active:true" }),
    ));
    assert_eq!(
        downstream.body["data"]["carrierService"]["active"],
        json!(true)
    );
    assert_eq!(
        downstream.body["data"]["active"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    let delete = proxy.process_request(json_graphql_request(
        r#"
        mutation CarrierServiceDeleteProbe($id: ID!) {
          carrierServiceDelete(id: $id) { deletedId userErrors { field message } }
        }
        "#,
        json!({ "id": id }),
    ));
    assert_eq!(
        delete.body["data"]["carrierServiceDelete"]["userErrors"],
        json!([])
    );

    let missing = proxy.process_request(json_graphql_request(
        r#"
        mutation CarrierServiceDeleteProbe($id: ID!) {
          carrierServiceDelete(id: $id) { deletedId userErrors { field message } }
        }
        "#,
        json!({ "id": "gid://shopify/DeliveryCarrierService/999999999999" }),
    ));
    assert_eq!(
        missing.body["data"]["carrierServiceDelete"]["userErrors"][0]["message"],
        json!("The carrier or app could not be found.")
    );
}

#[test]
fn carrier_service_create_validates_callback_url_and_projects_error_codes() {
    let mut proxy = snapshot_proxy();
    let http_create = proxy.process_request(json_graphql_request(
        r#"
        mutation InvalidCarrierServiceCreate($input: DeliveryCarrierServiceCreateInput!) {
          carrierServiceCreate(input: $input) {
            carrierService { id callbackUrl }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "input": {
            "name": "HTTP Carrier",
            "callbackUrl": "http://example.com/rates"
        }}),
    ));
    assert_eq!(
        http_create.body["data"]["carrierServiceCreate"],
        json!({
            "carrierService": null,
            "userErrors": [{
                "field": null,
                "message": "Shipping rate provider callback url must use HTTPS",
                "code": "CARRIER_SERVICE_CREATE_FAILED"
            }]
        })
    );

    let banned_create = proxy.process_request(json_graphql_request(
        r#"
        mutation InvalidCarrierServiceCreate($input: DeliveryCarrierServiceCreateInput!) {
          carrierServiceCreate(input: $input) {
            carrierService { id callbackUrl }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "input": {
            "name": "Banned Carrier",
            "callbackUrl": "https://localhost/rates"
        }}),
    ));
    assert_eq!(
        banned_create.body["data"]["carrierServiceCreate"],
        json!({
            "carrierService": null,
            "userErrors": [{
                "field": null,
                "message": "Shipping rate provider callback url invalid host",
                "code": "CARRIER_SERVICE_CREATE_FAILED"
            }]
        })
    );

    let unparseable_create = proxy.process_request(json_graphql_request(
        r#"
        mutation InvalidCarrierServiceCreate($input: DeliveryCarrierServiceCreateInput!) {
          carrierServiceCreate(input: $input) {
            carrierService { id callbackUrl }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "input": {
            "name": "Unparseable Carrier",
            "callbackUrl": "not-a-url"
        }}),
    ));
    assert_eq!(
        unparseable_create.body["errors"][0]["extensions"]["code"],
        json!("INVALID_VARIABLE")
    );
    assert_eq!(
        unparseable_create.body["errors"][0]["extensions"]["problems"][0]["explanation"],
        json!("Invalid url 'not-a-url', missing scheme")
    );

    let blank_name = proxy.process_request(json_graphql_request(
        r#"
        mutation InvalidCarrierServiceCreate($input: DeliveryCarrierServiceCreateInput!) {
          carrierServiceCreate(input: $input) {
            carrierService { id callbackUrl }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "input": {
            "name": "",
            "callbackUrl": "https://mock.shop/carrier-service-rates"
        }}),
    ));
    assert_eq!(
        blank_name.body["data"]["carrierServiceCreate"],
        json!({
            "carrierService": null,
            "userErrors": [{
                "field": null,
                "message": "Shipping rate provider name can't be blank",
                "code": "CARRIER_SERVICE_CREATE_FAILED"
            }]
        })
    );

    let valid_create = proxy.process_request(json_graphql_request(
        r#"
        mutation CarrierServiceCreateProbe($input: DeliveryCarrierServiceCreateInput!) {
          carrierServiceCreate(input: $input) {
            carrierService { id name callbackUrl }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "input": {
            "name": "Hermes Carrier Local",
            "callbackUrl": "https://mock.shop/carrier-service-rates"
        }}),
    ));
    assert_eq!(
        valid_create.body["data"]["carrierServiceCreate"]["carrierService"]["callbackUrl"],
        json!("https://mock.shop/carrier-service-rates")
    );
    assert_eq!(
        valid_create.body["data"]["carrierServiceCreate"]["userErrors"],
        json!([])
    );
}

#[test]
fn carrier_service_create_missing_required_booleans_returns_coercion_errors_before_staging() {
    let mut proxy = snapshot_proxy();
    let document = r#"
        mutation CarrierServiceCreateProbe($input: DeliveryCarrierServiceCreateInput!) {
          carrierServiceCreate(input: $input) {
            carrierService { id name active supportsServiceDiscovery }
            userErrors { field message code }
          }
        }
        "#;

    let missing_active = proxy.process_request(json_graphql_request(
        document,
        json!({ "input": {
            "name": "Hermes Missing Active",
            "callbackUrl": "https://mock.shop/carrier-service-rates",
            "supportsServiceDiscovery": false
        }}),
    ));
    assert_eq!(
        missing_active.body,
        json!({
            "errors": [{
                "message": "Variable $input of type DeliveryCarrierServiceCreateInput! was provided invalid value for active (Expected value to not be null)",
                "locations": [{ "line": 2, "column": 44 }],
                "extensions": {
                    "code": "INVALID_VARIABLE",
                    "value": {
                        "callbackUrl": "https://mock.shop/carrier-service-rates",
                        "name": "Hermes Missing Active",
                        "supportsServiceDiscovery": false
                    },
                    "problems": [{ "path": ["active"], "explanation": "Expected value to not be null" }]
                }
            }]
        })
    );

    let missing_supports = proxy.process_request(json_graphql_request(
        document,
        json!({ "input": {
            "name": "Hermes Missing Supports",
            "callbackUrl": "https://mock.shop/carrier-service-rates",
            "active": false
        }}),
    ));
    assert_eq!(
        missing_supports.body,
        json!({
            "errors": [{
                "message": "Variable $input of type DeliveryCarrierServiceCreateInput! was provided invalid value for supportsServiceDiscovery (Expected value to not be null)",
                "locations": [{ "line": 2, "column": 44 }],
                "extensions": {
                    "code": "INVALID_VARIABLE",
                    "value": {
                        "active": false,
                        "callbackUrl": "https://mock.shop/carrier-service-rates",
                        "name": "Hermes Missing Supports"
                    },
                    "problems": [{ "path": ["supportsServiceDiscovery"], "explanation": "Expected value to not be null" }]
                }
            }]
        })
    );

    let missing_both = proxy.process_request(json_graphql_request(
        document,
        json!({ "input": {
            "name": "Hermes Missing Both",
            "callbackUrl": "https://mock.shop/carrier-service-rates"
        }}),
    ));
    assert_eq!(
        missing_both.body["errors"][0]["extensions"]["problems"],
        json!([
            { "path": ["supportsServiceDiscovery"], "explanation": "Expected value to not be null" },
            { "path": ["active"], "explanation": "Expected value to not be null" }
        ])
    );

    let services = proxy.process_request(json_graphql_request(
        r#"query CarrierServiceAfterRejectedCreates {
          carrierServices(first: 10) {
            nodes { id }
          }
        }"#,
        json!({}),
    ));
    assert_eq!(services.body["data"]["carrierServices"]["nodes"], json!([]));
    assert_eq!(proxy.get_log_snapshot()["entries"], json!([]));
}

#[test]
fn carrier_service_update_validates_changed_callback_url_and_codes_unknowns() {
    let mut proxy = snapshot_proxy();
    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CarrierServiceCreateProbe($input: DeliveryCarrierServiceCreateInput!) {
          carrierServiceCreate(input: $input) {
            carrierService { id name callbackUrl }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "input": {
            "name": "Hermes Carrier Local",
            "callbackUrl": "https://mock.shop/carrier-service-rates"
        }}),
    ));
    let id = create.body["data"]["carrierServiceCreate"]["carrierService"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let log_len_after_create = proxy.get_log_snapshot()["entries"]
        .as_array()
        .unwrap()
        .len();

    let blank_name_update = proxy.process_request(json_graphql_request(
        r#"
        mutation CarrierServiceUpdateProbe($input: DeliveryCarrierServiceUpdateInput!) {
          carrierServiceUpdate(input: $input) {
            carrierService { id name callbackUrl }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "input": {
            "id": id,
            "name": ""
        }}),
    ));
    assert_eq!(
        blank_name_update.body["data"]["carrierServiceUpdate"],
        json!({
            "carrierService": null,
            "userErrors": [{
                "field": null,
                "message": "Shipping rate provider name can't be blank",
                "code": "CARRIER_SERVICE_UPDATE_FAILED"
            }]
        })
    );
    assert_eq!(
        proxy.get_log_snapshot()["entries"]
            .as_array()
            .unwrap()
            .len(),
        log_len_after_create
    );
    let after_blank_name_update = proxy.process_request(json_graphql_request(
        r#"
        query CarrierServiceAfterUpdate($id: ID!) {
          carrierService(id: $id) { id name callbackUrl }
        }
        "#,
        json!({ "id": id }),
    ));
    assert_eq!(
        after_blank_name_update.body["data"]["carrierService"]["name"],
        json!("Hermes Carrier Local")
    );

    let unchanged_callback = proxy.process_request(json_graphql_request(
        r#"
        mutation CarrierServiceUpdateProbe($input: DeliveryCarrierServiceUpdateInput!) {
          carrierServiceUpdate(input: $input) {
            carrierService { id name callbackUrl }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "input": {
            "id": id,
            "name": "Hermes Carrier Renamed",
            "callbackUrl": "https://mock.shop/carrier-service-rates"
        }}),
    ));
    assert_eq!(
        unchanged_callback.body["data"]["carrierServiceUpdate"]["carrierService"]["name"],
        json!("Hermes Carrier Renamed")
    );
    assert_eq!(
        unchanged_callback.body["data"]["carrierServiceUpdate"]["userErrors"],
        json!([])
    );

    let omitted_name = proxy.process_request(json_graphql_request(
        r#"
        mutation CarrierServiceUpdateProbe($input: DeliveryCarrierServiceUpdateInput!) {
          carrierServiceUpdate(input: $input) {
            carrierService { id name callbackUrl active }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "input": {
            "id": id,
            "active": true
        }}),
    ));
    assert_eq!(
        omitted_name.body["data"]["carrierServiceUpdate"]["carrierService"]["name"],
        json!("Hermes Carrier Renamed")
    );
    assert_eq!(
        omitted_name.body["data"]["carrierServiceUpdate"]["carrierService"]["active"],
        json!(true)
    );
    assert_eq!(
        omitted_name.body["data"]["carrierServiceUpdate"]["userErrors"],
        json!([])
    );

    let http_update = proxy.process_request(json_graphql_request(
        r#"
        mutation CarrierServiceUpdateProbe($input: DeliveryCarrierServiceUpdateInput!) {
          carrierServiceUpdate(input: $input) {
            carrierService { id name callbackUrl }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "input": {
            "id": id,
            "callbackUrl": "http://example.com/rates"
        }}),
    ));
    assert_eq!(
        http_update.body["data"]["carrierServiceUpdate"],
        json!({
            "carrierService": null,
            "userErrors": [{
                "field": null,
                "message": "Shipping rate provider callback url must use HTTPS",
                "code": "CARRIER_SERVICE_UPDATE_FAILED"
            }]
        })
    );

    let banned_update = proxy.process_request(json_graphql_request(
        r#"
        mutation CarrierServiceUpdateProbe($input: DeliveryCarrierServiceUpdateInput!) {
          carrierServiceUpdate(input: $input) {
            carrierService { id name callbackUrl }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "input": {
            "id": id,
            "callbackUrl": "https://shopify.com/rates"
        }}),
    ));
    assert_eq!(
        banned_update.body["data"]["carrierServiceUpdate"],
        json!({
            "carrierService": null,
            "userErrors": [{
                "field": null,
                "message": "Shipping rate provider callback url invalid host",
                "code": "CARRIER_SERVICE_UPDATE_FAILED"
            }]
        })
    );

    let after_rejected_update = proxy.process_request(json_graphql_request(
        r#"
        query CarrierServiceAfterUpdate($id: ID!) {
          carrierService(id: $id) { id name callbackUrl }
        }
        "#,
        json!({ "id": id }),
    ));
    assert_eq!(
        after_rejected_update.body["data"]["carrierService"]["callbackUrl"],
        json!("https://mock.shop/carrier-service-rates")
    );

    let unknown_update = proxy.process_request(json_graphql_request(
        r#"
        mutation UnknownCarrierServiceUpdate($input: DeliveryCarrierServiceUpdateInput!) {
          carrierServiceUpdate(input: $input) {
            carrierService { id callbackUrl }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "input": {
            "id": "gid://shopify/DeliveryCarrierService/999999999999",
            "callbackUrl": "https://mock.shop/carrier-service-rates"
        }}),
    ));
    assert_eq!(
        unknown_update.body["data"]["carrierServiceUpdate"],
        json!({
            "carrierService": null,
            "userErrors": [{
                "field": null,
                "message": "The carrier or app could not be found.",
                "code": "CARRIER_SERVICE_UPDATE_FAILED"
            }]
        })
    );

    let unknown_delete = proxy.process_request(json_graphql_request(
        r#"
        mutation UnknownCarrierServiceDelete($id: ID!) {
          carrierServiceDelete(id: $id) {
            deletedId
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": "gid://shopify/DeliveryCarrierService/999999999999" }),
    ));
    assert_eq!(
        unknown_delete.body["data"]["carrierServiceDelete"],
        json!({
            "deletedId": null,
            "userErrors": [{
                "field": ["id"],
                "message": "The carrier or app could not be found.",
                "code": "CARRIER_SERVICE_DELETE_FAILED"
            }]
        })
    );
}

#[test]
fn carrier_services_connection_paginates_edges_nodes_and_active_false_filter() {
    let mut proxy = snapshot_proxy();

    for (name, active) in [
        ("Carrier inactive one", false),
        ("Carrier active", true),
        ("Carrier inactive two", false),
    ] {
        let create = proxy.process_request(json_graphql_request(
            r#"
            mutation CarrierServiceCreateProbe($input: DeliveryCarrierServiceCreateInput!) {
              carrierServiceCreate(input: $input) {
                carrierService { id name active }
                userErrors { field message }
              }
            }
            "#,
            json!({ "input": {
                "name": name,
                "callbackUrl": "https://mock.shop/rates",
                "supportsServiceDiscovery": true,
                "active": active
            }}),
        ));
        assert_eq!(
            create.body["data"]["carrierServiceCreate"]["userErrors"],
            json!([])
        );
    }

    let first_page = proxy.process_request(json_graphql_request(
        r#"
        query CarrierServiceAfterUpdate($first: Int!, $query: String) {
          carrierServices(first: $first, query: $query, sortKey: ID) {
            nodes { id name active }
            edges { cursor node { id name active } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({"first": 1, "query": "active:false"}),
    ));
    assert_eq!(
        first_page.body["data"]["carrierServices"],
        json!({
            "nodes": [{
                "id": "gid://shopify/DeliveryCarrierService/1?shopify-draft-proxy=synthetic",
                "name": "Carrier inactive one",
                "active": false
            }],
            "edges": [{
                "cursor": "cursor:gid://shopify/DeliveryCarrierService/1?shopify-draft-proxy=synthetic",
                "node": {
                    "id": "gid://shopify/DeliveryCarrierService/1?shopify-draft-proxy=synthetic",
                    "name": "Carrier inactive one",
                    "active": false
                }
            }],
            "pageInfo": {
                "hasNextPage": true,
                "hasPreviousPage": false,
                "startCursor": "cursor:gid://shopify/DeliveryCarrierService/1?shopify-draft-proxy=synthetic",
                "endCursor": "cursor:gid://shopify/DeliveryCarrierService/1?shopify-draft-proxy=synthetic"
            }
        })
    );

    let second_page = proxy.process_request(json_graphql_request(
        r#"
        query CarrierServiceAfterUpdate($first: Int!, $after: String!, $query: String) {
          carrierServices(first: $first, after: $after, query: $query, sortKey: ID) {
            nodes { id name active }
            edges { cursor node { id name active } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({
            "first": 1,
            "after": first_page.body["data"]["carrierServices"]["pageInfo"]["endCursor"],
            "query": "active:false"
        }),
    ));
    assert_eq!(
        second_page.body["data"]["carrierServices"],
        json!({
            "nodes": [{
                "id": "gid://shopify/DeliveryCarrierService/3?shopify-draft-proxy=synthetic",
                "name": "Carrier inactive two",
                "active": false
            }],
            "edges": [{
                "cursor": "cursor:gid://shopify/DeliveryCarrierService/3?shopify-draft-proxy=synthetic",
                "node": {
                    "id": "gid://shopify/DeliveryCarrierService/3?shopify-draft-proxy=synthetic",
                    "name": "Carrier inactive two",
                    "active": false
                }
            }],
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": true,
                "startCursor": "cursor:gid://shopify/DeliveryCarrierService/3?shopify-draft-proxy=synthetic",
                "endCursor": "cursor:gid://shopify/DeliveryCarrierService/3?shopify-draft-proxy=synthetic"
            }
        })
    );
}

#[test]
fn delivery_settings_roots_return_read_only_settings_with_aliases_and_selected_fields() {
    let mut proxy = snapshot_proxy();
    let response = proxy.process_request(json_graphql_request(
        r#"
        query DeliverySettingsRead {
          deliverySettingsAlias: deliverySettings {
            legacyModeProfiles
            legacyModeBlocked { blocked reasons }
          }
          deliveryPromiseSettingsAlias: deliveryPromiseSettings {
            deliveryDatesEnabled
            processingTime
          }
        }
        "#,
        json!({}),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body,
        json!({
            "data": {
                "deliverySettingsAlias": {
                    "legacyModeProfiles": false,
                    "legacyModeBlocked": { "blocked": false, "reasons": null }
                },
                "deliveryPromiseSettingsAlias": {
                    "deliveryDatesEnabled": false,
                    "processingTime": null
                }
            }
        })
    );
}

#[test]
fn delivery_profile_lifecycle_stages_nested_state_reads_and_removal_job() {
    let mut proxy = snapshot_proxy();
    let create_query = r#"
        mutation DeliveryProfileLifecycleCreate($profile: DeliveryProfileInput!) {
          deliveryProfileCreate(profile: $profile) {
            profile {
              id
              name
              version
              originLocationCount
              zoneCountryCount
              activeMethodDefinitionsCount
              productVariantsCount { count precision }
              profileItems(first: 5) {
                nodes {
                  product { id title }
                  variants(first: 5) { nodes { id title } }
                }
              }
              profileLocationGroups {
                locationGroup {
                  id
                  locations(first: 5) {
                    nodes { id name }
                    pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
                  }
                  locationsCount { count precision }
                }
                locationGroupZones(first: 5) {
                  nodes {
                    zone { id name countries { code { countryCode restOfWorld } } }
                    methodDefinitions(first: 5) {
                      nodes {
                        id
                        name
                        active
                        rateProvider { ... on DeliveryRateDefinition { id price { amount currencyCode } } }
                        methodConditions {
                          id
                          field
                          operator
                          conditionCriteria {
                            __typename
                            ... on Weight { value unit }
                            ... on MoneyV2 { amount currencyCode }
                          }
                        }
                      }
                    }
                  }
                }
              }
            }
            userErrors { field message }
          }
        }
    "#;
    let update_query = r#"
        mutation DeliveryProfileLifecycleUpdate($id: ID!, $profile: DeliveryProfileInput!) {
          deliveryProfileUpdate(id: $id, profile: $profile) {
            profile {
              id
              name
              version
              originLocationCount
              activeMethodDefinitionsCount
              productVariantsCount { count precision }
              profileItems(first: 5) { nodes { product { id } variants(first: 5) { nodes { id } } } }
            }
            userErrors { field message }
          }
        }
    "#;
    let read_query = r#"
        query DeliveryProfileDownstreamRead($id: ID!) {
          deliveryProfile(id: $id) {
            id
            name
            originLocationCount
            activeMethodDefinitionsCount
          }
          deliveryProfiles(first: 5) {
            edges { cursor node { id name } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
    "#;
    let remove_query = r#"
        mutation DeliveryProfileLifecycleRemove($id: ID!) {
          deliveryProfileRemove(id: $id) {
            job { id done }
            userErrors { field message }
          }
        }
    "#;

    let create = proxy.process_request(json_graphql_request(
        create_query,
        json!({
            "profile": {
                "name": "Local heavy goods",
                "locationGroupsToCreate": [{
                    "locations": ["gid://shopify/Location/106318430514"],
                    "zonesToCreate": [{
                        "name": "Domestic",
                        "countries": [{ "code": "US", "includeAllProvinces": true }],
                        "methodDefinitionsToCreate": [{
                            "name": "Standard",
                            "active": true,
                            "rateDefinition": { "price": { "amount": "7.25", "currencyCode": "USD" } },
                            "weightConditionsToCreate": [{
                                "operator": "GREATER_THAN_OR_EQUAL_TO",
                                "criteria": { "value": 1, "unit": "KILOGRAMS" }
                            }]
                        }]
                    }]
                }],
                "variantsToAssociate": ["gid://shopify/ProductVariant/51098706739506"]
            }
        }),
    ));

    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["deliveryProfileCreate"]["userErrors"],
        json!([])
    );
    let profile = &create.body["data"]["deliveryProfileCreate"]["profile"];
    let profile_id = profile["id"].as_str().unwrap().to_string();
    let group_id = profile["profileLocationGroups"][0]["locationGroup"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let zone_id = profile["profileLocationGroups"][0]["locationGroupZones"]["nodes"][0]["zone"]
        ["id"]
        .as_str()
        .unwrap()
        .to_string();
    let method_id = profile["profileLocationGroups"][0]["locationGroupZones"]["nodes"][0]
        ["methodDefinitions"]["nodes"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let rate_id = profile["profileLocationGroups"][0]["locationGroupZones"]["nodes"][0]
        ["methodDefinitions"]["nodes"][0]["rateProvider"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let condition_id = profile["profileLocationGroups"][0]["locationGroupZones"]["nodes"][0]
        ["methodDefinitions"]["nodes"][0]["methodConditions"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(profile_id.starts_with("gid://shopify/DeliveryProfile/"));
    assert_eq!(profile["name"], json!("Local heavy goods"));
    assert_eq!(profile["version"], json!(1));
    assert_eq!(profile["originLocationCount"], json!(1));
    assert_eq!(profile["zoneCountryCount"], json!(1));
    assert_eq!(profile["activeMethodDefinitionsCount"], json!(1));
    assert_eq!(
        profile["productVariantsCount"],
        json!({ "count": 1, "precision": "EXACT" })
    );
    assert_eq!(
        profile["profileItems"]["nodes"][0]["variants"]["nodes"][0]["id"],
        json!("gid://shopify/ProductVariant/51098706739506")
    );

    let read = proxy.process_request(json_graphql_request(
        read_query,
        json!({ "id": profile_id }),
    ));
    assert_eq!(
        read.body["data"]["deliveryProfile"]["name"],
        json!("Local heavy goods")
    );
    assert_eq!(
        read.body["data"]["deliveryProfiles"]["edges"][0]["node"]["id"],
        json!(profile_id)
    );

    let update = proxy.process_request(json_graphql_request(
        update_query,
        json!({
            "id": profile_id,
            "profile": {
                "name": "Local heavy goods updated",
                "variantsToDissociate": ["gid://shopify/ProductVariant/51098706739506"],
                "conditionsToDelete": [condition_id],
                "locationGroupsToUpdate": [{
                    "id": group_id,
                    "locationsToAdd": ["gid://shopify/Location/106318463282"],
                    "zonesToUpdate": [{
                        "id": zone_id,
                        "name": "Domestic updated",
                        "methodDefinitionsToUpdate": [{
                            "id": method_id,
                            "name": "Standard updated",
                            "active": false,
                            "rateDefinition": {
                                "id": rate_id,
                                "price": { "amount": "8.50", "currencyCode": "USD" }
                            }
                        }],
                        "methodDefinitionsToCreate": [{
                            "name": "Express",
                            "active": true,
                            "rateDefinition": { "price": { "amount": "12.00", "currencyCode": "USD" } },
                            "priceConditionsToCreate": [{
                                "operator": "LESS_THAN_OR_EQUAL_TO",
                                "criteria": { "amount": "100.00", "currencyCode": "USD" }
                            }]
                        }]
                    }]
                }]
            }
        }),
    ));

    assert_eq!(update.status, 200);
    let updated = &update.body["data"]["deliveryProfileUpdate"]["profile"];
    assert_eq!(updated["id"], json!(profile_id));
    assert_eq!(updated["name"], json!("Local heavy goods updated"));
    assert_eq!(updated["version"], json!(2));
    assert_eq!(updated["originLocationCount"], json!(2));
    assert_eq!(updated["activeMethodDefinitionsCount"], json!(1));
    assert_eq!(
        updated["productVariantsCount"],
        json!({ "count": 0, "precision": "EXACT" })
    );
    assert_eq!(updated["profileItems"]["nodes"], json!([]));

    let remove = proxy.process_request(json_graphql_request(
        remove_query,
        json!({ "id": profile_id }),
    ));
    assert_eq!(remove.status, 200);
    assert_eq!(
        remove.body["data"]["deliveryProfileRemove"]["userErrors"],
        json!([])
    );
    assert!(remove.body["data"]["deliveryProfileRemove"]["job"]["id"]
        .as_str()
        .unwrap()
        .starts_with("gid://shopify/Job/"));
    assert_eq!(
        remove.body["data"]["deliveryProfileRemove"]["job"]["done"],
        json!(false)
    );

    let read_after_remove = proxy.process_request(json_graphql_request(
        r#"query ReadRemovedDeliveryProfile($id: ID!) { deliveryProfile(id: $id) { id } }"#,
        json!({ "id": profile_id }),
    ));
    assert_eq!(
        read_after_remove.body["data"]["deliveryProfile"],
        Value::Null
    );

    let log = proxy.get_log_snapshot();
    assert_eq!(
        log["entries"][0]["interpreted"]["primaryRootField"],
        json!("deliveryProfileCreate")
    );
    assert_eq!(log["entries"][0]["rawBody"].is_string(), true);
    assert_eq!(
        log["entries"][1]["interpreted"]["primaryRootField"],
        json!("deliveryProfileUpdate")
    );
    assert_eq!(
        log["entries"][2]["interpreted"]["primaryRootField"],
        json!("deliveryProfileRemove")
    );
    assert_eq!(log["entries"][2]["status"], json!("staged"));
}

#[test]
fn delivery_profile_validations_match_captured_write_subset() {
    let mut proxy = snapshot_proxy();
    let create_query = r#"
        mutation DeliveryProfileCreateValidation($profile: DeliveryProfileInput!) {
          deliveryProfileCreate(profile: $profile) {
            profile { id name }
            userErrors { field message }
          }
        }
    "#;
    let update_query = r#"
        mutation DeliveryProfileUpdateValidation($id: ID!, $profile: DeliveryProfileInput!) {
          deliveryProfileUpdate(id: $id, profile: $profile) {
            profile { id name }
            userErrors { field message }
          }
        }
    "#;

    let blank = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "profile": { "name": "" } }),
    ));
    assert_eq!(
        blank.body["data"]["deliveryProfileCreate"],
        json!({
            "profile": null,
            "userErrors": [{ "field": ["profile", "name"], "message": "Add a profile name" }]
        })
    );

    let long_name = "x".repeat(128);
    let too_long = proxy.process_request(json_graphql_request(
        create_query,
        json!({ "profile": { "name": long_name } }),
    ));
    assert_eq!(
        too_long.body["data"]["deliveryProfileCreate"]["userErrors"][0]["message"],
        json!("Profile name must be less than 128 characters long")
    );

    let disallowed = proxy.process_request(json_graphql_request(
        create_query,
        json!({
            "profile": {
                "name": "Disallowed",
                "variantsToDissociate": ["gid://shopify/ProductVariant/1"]
            }
        }),
    ));
    assert_eq!(
        disallowed.body["data"]["deliveryProfileCreate"]["userErrors"][0]["message"],
        json!("Cannot disassociate variants when creating a profile.")
    );

    let unknown_location = proxy.process_request(json_graphql_request(
        create_query,
        json!({
            "profile": {
                "name": "Unknown location",
                "locationGroupsToCreate": [{ "locations": ["gid://shopify/Location/999999999"] }]
            }
        }),
    ));
    assert_eq!(
        unknown_location.body["data"]["deliveryProfileCreate"]["userErrors"][0],
        json!({ "field": null, "message": "The Location could not be found for this shop." })
    );

    let empty_countries = proxy.process_request(json_graphql_request(
        create_query,
        json!({
            "profile": {
                "name": "Empty countries",
                "locationGroupsToCreate": [{
                    "locations": ["gid://shopify/Location/106318430514"],
                    "zonesToCreate": [{ "name": "Empty", "countries": [] }]
                }]
            }
        }),
    ));
    assert_eq!(
        empty_countries.body["data"]["deliveryProfileCreate"]["userErrors"][0]["message"],
        json!("Profile is invalid: cannot create LocationGroupZone without countries.")
    );

    let create = proxy.process_request(json_graphql_request(
        create_query,
        json!({
            "profile": {
                "name": "Validation base",
                "locationGroupsToCreate": [{
                    "locations": ["gid://shopify/Location/106318430514"],
                    "zonesToCreate": [{ "name": "Domestic", "countries": [{ "code": "US", "includeAllProvinces": true }] }]
                }]
            }
        }),
    ));
    let id = create.body["data"]["deliveryProfileCreate"]["profile"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let missing_update = proxy.process_request(json_graphql_request(
        update_query,
        json!({ "id": "gid://shopify/DeliveryProfile/999999999999", "profile": { "name": "Nope" } }),
    ));
    assert_eq!(
        missing_update.body["data"]["deliveryProfileUpdate"],
        json!({
            "profile": null,
            "userErrors": [{ "field": null, "message": "Profile could not be updated." }]
        })
    );

    let update_unknown_location = proxy.process_request(json_graphql_request(
        update_query,
        json!({
            "id": id,
            "profile": {
                "locationGroupsToCreate": [{ "locations": ["gid://shopify/Location/999999999"] }]
            }
        }),
    ));
    assert_eq!(
        update_unknown_location.body["data"]["deliveryProfileUpdate"]["userErrors"][0]["message"],
        json!("The Location could not be found for this shop.")
    );

    let missing_remove = proxy.process_request(json_graphql_request(
        r#"
        mutation MissingDeliveryProfileRemove($id: ID!) {
          deliveryProfileRemove(id: $id) { job { id done } userErrors { field message } }
        }
        "#,
        json!({ "id": "gid://shopify/DeliveryProfile/999999999999" }),
    ));
    assert_eq!(
        missing_remove.body["data"]["deliveryProfileRemove"],
        json!({
            "job": null,
            "userErrors": [{ "field": null, "message": "The Delivery Profile cannot be found for the shop." }]
        })
    );
}

#[test]
fn shipping_package_lifecycle_stages_state_defaults_deletes_and_log_order() {
    let mut proxy = snapshot_proxy();
    let update_query = r#"
        mutation ShippingPackageUpdateLocalRuntime($id: ID!, $shippingPackage: CustomShippingPackageInput!) {
          shippingPackageUpdate(id: $id, shippingPackage: $shippingPackage) { userErrors { field message } }
        }
    "#;
    let make_default_query = r#"
        mutation ShippingPackageMakeDefaultLocalRuntime($id: ID!) {
          shippingPackageMakeDefault(id: $id) { userErrors { field message } }
        }
    "#;
    let delete_query = r#"
        mutation ShippingPackageDeleteLocalRuntime($id: ID!) {
          shippingPackageDelete(id: $id) { deletedId userErrors { field message } }
        }
    "#;

    let update = proxy.process_request(json_graphql_request(
        update_query,
        json!({
            "id": "gid://shopify/ShippingPackage/1",
            "shippingPackage": {
                "name": "Updated box",
                "type": "BOX",
                "default": true,
                "weight": { "value": 2.5, "unit": "POUNDS" },
                "dimensions": { "length": 12, "width": 9, "height": 5, "unit": "INCHES" }
            }
        }),
    ));
    assert_eq!(
        update.body["data"]["shippingPackageUpdate"],
        json!({ "userErrors": [] })
    );
    assert_eq!(
        proxy.get_state_snapshot()["stagedState"]["shippingPackages"]
            ["gid://shopify/ShippingPackage/1"]["updatedAt"],
        json!("2024-01-01T00:00:01.000Z")
    );

    let make_default = proxy.process_request(json_graphql_request(
        make_default_query,
        json!({ "id": "gid://shopify/ShippingPackage/2" }),
    ));
    assert_eq!(
        make_default.body["data"]["shippingPackageMakeDefault"],
        json!({ "userErrors": [] })
    );
    let state = proxy.get_state_snapshot();
    assert_eq!(
        state["stagedState"]["shippingPackages"]["gid://shopify/ShippingPackage/1"]["default"],
        json!(false)
    );
    assert_eq!(
        state["stagedState"]["shippingPackages"]["gid://shopify/ShippingPackage/2"]["default"],
        json!(true)
    );

    let restore = proxy.process_request(json_graphql_request(
        update_query,
        json!({
            "id": "gid://shopify/ShippingPackage/1",
            "shippingPackage": { "default": true }
        }),
    ));
    assert_eq!(
        restore.body["data"]["shippingPackageUpdate"],
        json!({ "userErrors": [] })
    );
    let state = proxy.get_state_snapshot();
    assert_eq!(
        state["stagedState"]["shippingPackages"]["gid://shopify/ShippingPackage/1"]["default"],
        json!(true)
    );
    assert_eq!(
        state["stagedState"]["shippingPackages"]["gid://shopify/ShippingPackage/2"]["default"],
        json!(false)
    );

    let delete = proxy.process_request(json_graphql_request(
        delete_query,
        json!({ "id": "gid://shopify/ShippingPackage/1" }),
    ));
    assert_eq!(
        delete.body["data"]["shippingPackageDelete"],
        json!({ "deletedId": "gid://shopify/ShippingPackage/1", "userErrors": [] })
    );
    let state = proxy.get_state_snapshot();
    assert_eq!(
        state["stagedState"]["deletedShippingPackageIds"]["gid://shopify/ShippingPackage/1"],
        json!(true)
    );
    assert!(state["stagedState"]["shippingPackages"]
        .get("gid://shopify/ShippingPackage/1")
        .is_none());

    let log = proxy.get_log_snapshot();
    assert_eq!(
        log["entries"][0]["operationName"],
        json!("shippingPackageUpdate")
    );
    assert_eq!(
        log["entries"][1]["operationName"],
        json!("shippingPackageMakeDefault")
    );
    assert_eq!(
        log["entries"][2]["operationName"],
        json!("shippingPackageUpdate")
    );
    assert_eq!(
        log["entries"][3]["operationName"],
        json!("shippingPackageDelete")
    );
    assert_eq!(log["entries"][3]["status"], json!("staged"));
}

#[test]
fn location_local_pickup_enable_disable_stage_settings_and_downstream_reads() {
    let mut proxy = snapshot_proxy();
    let add = proxy.process_request(json_graphql_request(
        r#"
        mutation SeedPickupLocation($input: LocationAddInput!) {
          locationAdd(input: $input) { location { id name localPickupSettingsV2 { pickupTime instructions } } userErrors { field message code } }
        }
        "#,
        json!({
            "input": {
                "name": "Pickup Warehouse",
                "address": { "countryCode": "US" },
                "fulfillsOnlineOrders": false
            }
        }),
    ));
    let location_id = add.body["data"]["locationAdd"]["location"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let enable_query = r#"
        mutation ConsumerNamedAnything($localPickupSettings: DeliveryLocationLocalPickupEnableInput!) {
          aliasedEnable: locationLocalPickupEnable(localPickupSettings: $localPickupSettings) {
            localPickupSettings { pickupTime instructions }
            userErrors { field message code }
          }
        }
    "#;
    let enable = proxy.process_request(json_graphql_request(
        enable_query,
        json!({
            "localPickupSettings": {
                "locationId": location_id,
                "pickupTime": "ONE_HOUR",
                "instructions": "Ring bell"
            }
        }),
    ));
    assert_eq!(enable.status, 200);
    assert_eq!(
        enable.body["data"]["aliasedEnable"],
        json!({
            "localPickupSettings": { "pickupTime": "ONE_HOUR", "instructions": "Ring bell" },
            "userErrors": []
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query ReadPickup($id: ID!) {
          location(id: $id) { id name localPickupSettingsV2 { pickupTime instructions } }
          locationsAvailableForDeliveryProfilesConnection(first: 5) {
            nodes { id localPickupSettingsV2 { pickupTime instructions } }
            pageInfo { hasNextPage hasPreviousPage }
          }
        }
        "#,
        json!({ "id": location_id }),
    ));
    assert_eq!(
        read.body["data"]["location"]["localPickupSettingsV2"],
        json!({ "pickupTime": "ONE_HOUR", "instructions": "Ring bell" })
    );
    assert_eq!(
        read.body["data"]["locationsAvailableForDeliveryProfilesConnection"]["nodes"][0]
            ["localPickupSettingsV2"],
        json!({ "pickupTime": "ONE_HOUR", "instructions": "Ring bell" })
    );

    let disable_query = r#"
        mutation DisablePickup($locationId: ID!) {
          locationLocalPickupDisable(locationId: $locationId) {
            locationId
            userErrors { field message code }
          }
        }
    "#;
    let disable = proxy.process_request(json_graphql_request(
        disable_query,
        json!({ "locationId": location_id }),
    ));
    assert_eq!(
        disable.body["data"]["locationLocalPickupDisable"],
        json!({ "locationId": location_id, "userErrors": [] })
    );

    let after_disable = proxy.process_request(json_graphql_request(
        r#"
        query ReadPickup($id: ID!) {
          location(id: $id) { id localPickupSettingsV2 { pickupTime instructions } }
        }
        "#,
        json!({ "id": location_id }),
    ));
    assert_eq!(
        after_disable.body["data"]["location"]["localPickupSettingsV2"],
        Value::Null
    );

    let state = proxy.get_state_snapshot();
    assert_eq!(
        state["stagedState"]["locations"][location_id.as_str()]["localPickupSettingsV2"],
        Value::Null
    );
    let log = proxy.get_log_snapshot();
    assert_eq!(log["entries"][1]["status"], json!("staged"));
    assert_eq!(
        log["entries"][1]["interpreted"]["primaryRootField"],
        json!("locationLocalPickupEnable")
    );
    let raw_body: Value =
        serde_json::from_str(log["entries"][1]["rawBody"].as_str().unwrap()).unwrap();
    assert_eq!(raw_body["query"], json!(enable_query));
    assert_eq!(
        log["entries"][2]["interpreted"]["primaryRootField"],
        json!("locationLocalPickupDisable")
    );
}

#[test]
fn location_local_pickup_enable_validates_pickup_time_and_location_status() {
    let mut proxy = snapshot_proxy();
    let add = proxy.process_request(json_graphql_request(
        r#"
        mutation SeedPickupLocation($input: LocationAddInput!) {
          locationAdd(input: $input) { location { id } userErrors { field message code } }
        }
        "#,
        json!({ "input": { "name": "Pickup Validation", "address": { "countryCode": "US" } } }),
    ));
    let location_id = add.body["data"]["locationAdd"]["location"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let query = r#"
        mutation ValidatePickup($localPickupSettings: DeliveryLocationLocalPickupEnableInput!) {
          locationLocalPickupEnable(localPickupSettings: $localPickupSettings) {
            localPickupSettings { pickupTime instructions }
            userErrors { field message code }
          }
        }
    "#;

    let invalid_time = proxy.process_request(json_graphql_request(
        query,
        json!({
            "localPickupSettings": {
                "locationId": location_id,
                "pickupTime": "CUSTOM",
                "instructions": "Nope"
            }
        }),
    ));
    assert_eq!(
        invalid_time.body["data"]["locationLocalPickupEnable"],
        json!({
            "localPickupSettings": null,
            "userErrors": [{
                "field": ["localPickupSettings"],
                "message": "Custom pickup time is not allowed for local pickup settings.",
                "code": "CUSTOM_PICKUP_TIME_NOT_ALLOWED"
            }]
        })
    );

    let unknown = proxy.process_request(json_graphql_request(
        query,
        json!({
            "localPickupSettings": {
                "locationId": "gid://shopify/Location/999999999999",
                "pickupTime": "ONE_HOUR"
            }
        }),
    ));
    assert_eq!(
        unknown.body["data"]["locationLocalPickupEnable"],
        json!({
            "localPickupSettings": null,
            "userErrors": [{
                "field": ["localPickupSettings"],
                "message": "Unable to find an active location for location ID 999999999999",
                "code": "ACTIVE_LOCATION_NOT_FOUND"
            }]
        })
    );

    let inactive_id = "gid://shopify/Location/112849158450";
    let inactive = proxy.process_request(json_graphql_request(
        query,
        json!({
            "localPickupSettings": {
                "locationId": inactive_id,
                "pickupTime": "ONE_HOUR"
            }
        }),
    ));
    assert_eq!(
        inactive.body["data"]["locationLocalPickupEnable"]["userErrors"][0]["code"],
        json!("ACTIVE_LOCATION_NOT_FOUND")
    );
    assert_eq!(
        proxy.get_log_snapshot()["entries"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn location_local_pickup_live_hybrid_mutations_are_local_and_overlay_observed_reads() {
    let forwarded = Arc::new(Mutex::new(Vec::<Request>::new()));
    let captured = Arc::clone(&forwarded);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            captured.lock().unwrap().push(request);
            shopify_draft_proxy::proxy::Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "locationsAvailableForDeliveryProfilesConnection": {
                            "nodes": [{
                                "id": "gid://shopify/Location/106318496050",
                                "name": "Snow City Warehouse",
                                "localPickupSettingsV2": null,
                                "isActive": true,
                                "isFulfillmentService": false
                            }],
                            "pageInfo": { "hasNextPage": false, "hasPreviousPage": false }
                        }
                    }
                }),
            }
        });

    let hydrate = proxy.process_request(json_graphql_request(
        r#"
        query HydratePickupLocations {
          locationsAvailableForDeliveryProfilesConnection(first: 1) {
            nodes { id name localPickupSettingsV2 { pickupTime instructions } }
            pageInfo { hasNextPage hasPreviousPage }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(hydrate.status, 200);
    assert_eq!(forwarded.lock().unwrap().len(), 1);

    let enable = proxy.process_request(json_graphql_request(
        r#"
        mutation PickupEnable($localPickupSettings: DeliveryLocationLocalPickupEnableInput!) {
          locationLocalPickupEnable(localPickupSettings: $localPickupSettings) {
            localPickupSettings { pickupTime instructions }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "localPickupSettings": {
                "locationId": "gid://shopify/Location/106318496050",
                "pickupTime": "TWO_HOURS",
                "instructions": "Desk pickup"
            }
        }),
    ));
    assert_eq!(
        enable.body["data"]["locationLocalPickupEnable"]["userErrors"],
        json!([])
    );
    assert_eq!(
        forwarded.lock().unwrap().len(),
        1,
        "supported local-pickup mutation must not write upstream"
    );

    let after = proxy.process_request(json_graphql_request(
        r#"
        query ReadAfterPickupEnable($id: ID!) {
          location(id: $id) { id name localPickupSettingsV2 { pickupTime instructions } }
          locationsAvailableForDeliveryProfilesConnection(first: 1) {
            nodes { id localPickupSettingsV2 { pickupTime instructions } }
          }
        }
        "#,
        json!({ "id": "gid://shopify/Location/106318496050" }),
    ));
    assert_eq!(
        after.body["data"]["location"]["localPickupSettingsV2"],
        json!({ "pickupTime": "TWO_HOURS", "instructions": "Desk pickup" })
    );
    assert_eq!(
        after.body["data"]["locationsAvailableForDeliveryProfilesConnection"]["nodes"][0]
            ["localPickupSettingsV2"],
        json!({ "pickupTime": "TWO_HOURS", "instructions": "Desk pickup" })
    );
    assert_eq!(forwarded.lock().unwrap().len(), 1);
}

#[test]
fn shipping_package_update_rejects_flat_rate_packages_without_staging_state() {
    let mut proxy = snapshot_proxy();
    let update_query = r#"
        mutation ShippingPackageUpdateFlatRate($id: ID!, $shippingPackage: CustomShippingPackageInput!) {
          shippingPackageUpdate(id: $id, shippingPackage: $shippingPackage) { userErrors { field message code } }
        }
    "#;

    let response = proxy.process_request(json_graphql_request(
        update_query,
        json!({
            "id": "gid://shopify/ShippingPackage/10",
            "shippingPackage": {
                "dimensions": { "length": 999, "width": 8, "height": 4, "unit": "CENTIMETERS" }
            }
        }),
    ));

    assert_eq!(
        response.body["data"]["shippingPackageUpdate"],
        json!({
            "userErrors": [{
                "field": ["shippingPackage"],
                "message": "Custom shipping box is not updatable",
                "code": "CUSTOM_SHIPPING_BOX_NOT_UPDATABLE"
            }]
        })
    );
    assert_eq!(
        proxy.get_state_snapshot()["stagedState"]["shippingPackages"],
        json!({})
    );
}

#[test]
fn store_credit_credit_debit_stage_account_transactions_and_readbacks() {
    let mut proxy = snapshot_proxy();
    let customer_id = create_store_credit_customer(&mut proxy);

    let credit_query = r#"
        mutation StoreCreditNonRecordingCredit($id: ID!, $amt: MoneyInput!, $notify: Boolean!) {
          storeCreditAccountCredit(id: $id, creditInput: { creditAmount: $amt, notify: $notify }) {
            storeCreditAccountTransaction {
              id
              amount { amount currencyCode }
              balanceAfterTransaction { amount currencyCode }
              event
              origin
              account {
                id
                balance { amount currencyCode }
                owner { ... on Customer { id email displayName } }
              }
            }
            userErrors { field message code }
          }
        }
    "#;
    let credit_variables = json!({
        "id": customer_id,
        "amt": { "amount": "7.23", "currencyCode": "USD" },
        "notify": true
    });
    let credit =
        proxy.process_request(json_graphql_request(credit_query, credit_variables.clone()));
    assert_eq!(credit.status, 200);
    assert_eq!(
        credit.body["data"]["storeCreditAccountCredit"]["userErrors"],
        json!([])
    );
    let credit_transaction =
        &credit.body["data"]["storeCreditAccountCredit"]["storeCreditAccountTransaction"];
    let account_id = credit_transaction["account"]["id"].as_str().unwrap();
    assert!(account_id.starts_with("gid://shopify/StoreCreditAccount/"));
    assert_eq!(
        credit_transaction["amount"],
        json!({ "amount": "7.23", "currencyCode": "USD" })
    );
    assert_eq!(
        credit_transaction["balanceAfterTransaction"],
        json!({ "amount": "7.23", "currencyCode": "USD" })
    );
    assert_eq!(credit_transaction["event"], json!("ADJUSTMENT"));
    assert_eq!(credit_transaction["origin"], Value::Null);
    assert_eq!(
        credit_transaction["account"]["owner"]["id"],
        json!(customer_id)
    );

    let debit_query = r#"
        mutation StoreCreditNonRecordingDebit($accountId: ID!, $amt: MoneyInput!) {
          spend: storeCreditAccountDebit(id: $accountId, debitInput: { debitAmount: $amt }) {
            storeCreditAccountTransaction {
              id
              amount { amount currencyCode }
              balanceAfterTransaction { amount currencyCode }
              account {
                id
                balance { amount currencyCode }
                transactions(first: 5) {
                  nodes { id amount { amount currencyCode } balanceAfterTransaction { amount currencyCode } }
                  pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
                }
              }
            }
            userErrors { field message code }
          }
        }
    "#;
    let debit_variables = json!({
        "accountId": account_id,
        "amt": { "amount": "2.22", "currencyCode": "USD" }
    });
    let debit = proxy.process_request(json_graphql_request(debit_query, debit_variables.clone()));
    assert_eq!(debit.status, 200);
    assert_eq!(debit.body["data"]["spend"]["userErrors"], json!([]));
    let debit_transaction = &debit.body["data"]["spend"]["storeCreditAccountTransaction"];
    assert_eq!(
        debit_transaction["amount"],
        json!({ "amount": "-2.22", "currencyCode": "USD" })
    );
    assert_eq!(
        debit_transaction["balanceAfterTransaction"],
        json!({ "amount": "5.01", "currencyCode": "USD" })
    );
    assert_eq!(
        debit_transaction["account"]["transactions"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query StoreCreditReadback($customerId: ID!, $accountId: ID!) {
          customer(id: $customerId) {
            id
            storeCreditAccounts(first: 5) {
              nodes { id balance { amount currencyCode } owner { ... on Customer { id email } } }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
          }
          storeCreditAccount(id: $accountId) {
            id
            balance { amount currencyCode }
            transactions(first: 5) { nodes { amount { amount currencyCode } } }
          }
        }
        "#,
        json!({ "customerId": customer_id, "accountId": account_id }),
    ));
    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"]["customer"]["storeCreditAccounts"]["nodes"][0]["balance"],
        json!({ "amount": "5.01", "currencyCode": "USD" })
    );
    assert_eq!(
        read.body["data"]["storeCreditAccount"]["balance"],
        json!({ "amount": "5.01", "currencyCode": "USD" })
    );
    assert_eq!(
        read.body["data"]["storeCreditAccount"]["transactions"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    let log = proxy.get_log_snapshot();
    let entries = log["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[1]["status"], json!("staged"));
    assert_eq!(
        entries[1]["interpreted"]["primaryRootField"],
        json!("storeCreditAccountCredit")
    );
    assert_eq!(entries[1]["stagedResourceIds"], json!([account_id]));
    assert_eq!(
        entries[1]["rawBody"],
        json!({ "query": credit_query, "variables": credit_variables }).to_string()
    );
    assert_eq!(entries[2]["status"], json!("staged"));
    assert_eq!(
        entries[2]["interpreted"]["primaryRootField"],
        json!("storeCreditAccountDebit")
    );
    assert_eq!(entries[2]["stagedResourceIds"], json!([account_id]));
    assert_eq!(
        entries[2]["rawBody"],
        json!({ "query": debit_query, "variables": debit_variables }).to_string()
    );
}

#[test]
fn store_credit_validations_match_shopify_user_error_shapes_without_staging_failures() {
    let mut proxy = snapshot_proxy();
    let customer_id = create_store_credit_customer(&mut proxy);
    let account_id = store_credit_account_id_from_credit(&mut proxy, &customer_id, "10.00", "USD");

    let zero_credit = store_credit_credit_error(
        &mut proxy,
        &customer_id,
        json!({ "amount": "0", "currencyCode": "USD" }),
        None,
    );
    assert_eq!(
        zero_credit,
        json!([{
            "field": ["creditInput", "creditAmount", "amount"],
            "message": "A positive amount must be used to credit a store credit account",
            "code": "NEGATIVE_OR_ZERO_AMOUNT"
        }])
    );

    let zero_debit = store_credit_debit_error(
        &mut proxy,
        &account_id,
        json!({ "amount": "0", "currencyCode": "USD" }),
    );
    assert_eq!(
        zero_debit,
        json!([{
            "field": ["debitInput", "debitAmount", "amount"],
            "message": "A positive amount must be used to debit a store credit account",
            "code": "NEGATIVE_OR_ZERO_AMOUNT"
        }])
    );

    let mismatch = store_credit_credit_error(
        &mut proxy,
        &account_id,
        json!({ "amount": "1.00", "currencyCode": "CAD" }),
        None,
    );
    assert_eq!(
        mismatch,
        json!([{
            "field": ["creditInput", "creditAmount", "currencyCode"],
            "message": "The currency provided does not match the currency of the store credit account",
            "code": "MISMATCHING_CURRENCY"
        }])
    );

    let overdraw = store_credit_debit_error(
        &mut proxy,
        &account_id,
        json!({ "amount": "11.00", "currencyCode": "USD" }),
    );
    assert_eq!(
        overdraw,
        json!([{
            "field": ["debitInput", "debitAmount", "amount"],
            "message": "The store credit account does not have sufficient funds to satisfy the request",
            "code": "INSUFFICIENT_FUNDS"
        }])
    );

    let past_expiry = store_credit_credit_error(
        &mut proxy,
        &customer_id,
        json!({ "amount": "1.00", "currencyCode": "USD" }),
        Some("2024-01-01T00:00:00Z"),
    );
    assert_eq!(
        past_expiry,
        json!([{
            "field": ["creditInput", "expiresAt"],
            "message": "The expiry date must be in the future",
            "code": "EXPIRES_AT_IN_PAST"
        }])
    );

    let unsupported_currency = store_credit_credit_error(
        &mut proxy,
        &customer_id,
        json!({ "amount": "1.00", "currencyCode": "XXX" }),
        None,
    );
    assert_eq!(
        unsupported_currency,
        json!([{
            "field": ["creditInput", "creditAmount", "currencyCode"],
            "message": "Currency is not supported",
            "code": "UNSUPPORTED_CURRENCY"
        }])
    );

    let credit_limit = store_credit_credit_error(
        &mut proxy,
        &account_id,
        json!({ "amount": "99990.00", "currencyCode": "USD" }),
        None,
    );
    assert_eq!(
        credit_limit,
        json!([{
            "field": ["creditInput", "creditAmount", "amount"],
            "message": "The operation would cause the account's credit limit to be exceeded",
            "code": "CREDIT_LIMIT_EXCEEDED"
        }])
    );

    let missing_account = store_credit_debit_error(
        &mut proxy,
        "gid://shopify/StoreCreditAccount/999",
        json!({ "amount": "1.00", "currencyCode": "USD" }),
    );
    assert_eq!(
        missing_account,
        json!([{
            "field": ["id"],
            "message": "Store credit account does not exist",
            "code": "NOT_FOUND"
        }])
    );

    let missing_owner = store_credit_credit_error(
        &mut proxy,
        "gid://shopify/Customer/999",
        json!({ "amount": "1.00", "currencyCode": "USD" }),
        None,
    );
    assert_eq!(
        missing_owner,
        json!([{
            "field": ["id"],
            "message": "Owner does not exist",
            "code": "NOT_FOUND"
        }])
    );

    let entries = proxy.get_log_snapshot()["entries"]
        .as_array()
        .unwrap()
        .len();
    assert_eq!(
        entries, 2,
        "only customerCreate and the successful setup credit should be staged"
    );
}

#[test]
fn store_credit_credit_creates_company_location_account() {
    let mut proxy = snapshot_proxy();
    let location_id = "gid://shopify/CompanyLocation/4?shopify-draft-proxy=synthetic";

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation CompanyLocationStoreCredit($id: ID!) {
          storeCreditAccountCredit(id: $id, creditInput: { creditAmount: { amount: "3.00", currencyCode: USD } }) {
            storeCreditAccountTransaction {
              account {
                id
                balance { amount currencyCode }
                owner { ... on CompanyLocation { id name } }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": location_id }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["storeCreditAccountCredit"]["userErrors"],
        json!([])
    );
    assert_eq!(
        response.body["data"]["storeCreditAccountCredit"]["storeCreditAccountTransaction"]
            ["account"]["balance"],
        json!({ "amount": "3.0", "currencyCode": "USD" })
    );
    assert_eq!(
        response.body["data"]["storeCreditAccountCredit"]["storeCreditAccountTransaction"]
            ["account"]["owner"]["id"],
        json!(location_id)
    );
}

#[test]
fn store_credit_schema_rejects_non_public_variable_fields() {
    let mut proxy = snapshot_proxy();
    let customer_id = create_store_credit_customer(&mut proxy);

    let invalid_credit = proxy.process_request(json_graphql_request(
        r#"
        mutation StoreCreditInvalidCredit($id: ID!, $input: StoreCreditAccountCreditInput!) {
          storeCreditAccountCredit(id: $id, creditInput: $input) {
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "id": customer_id,
            "input": {
                "creditAmount": { "amount": "1.00", "currencyCode": "USD" },
                "attribution": { "app": "draft-proxy" }
            }
        }),
    ));
    assert_eq!(invalid_credit.status, 200);
    assert_eq!(
        invalid_credit.body["errors"][0]["extensions"]["code"],
        json!("INVALID_VARIABLE")
    );
    assert!(invalid_credit.body["errors"][0]["message"]
        .as_str()
        .is_some_and(|message| message.contains("attribution")));

    let invalid_debit = proxy.process_request(json_graphql_request(
        r#"
        mutation StoreCreditInvalidDebit($id: ID!, $input: StoreCreditAccountDebitInput!) {
          storeCreditAccountDebit(id: $id, debitInput: $input) {
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "id": "gid://shopify/StoreCreditAccount/999",
            "input": {
                "debitAmount": { "amount": "1.00", "currencyCode": "USD" },
                "notify": true,
                "attribution": { "app": "draft-proxy" }
            }
        }),
    ));
    assert_eq!(invalid_debit.status, 200);
    assert_eq!(
        invalid_debit.body["errors"][0]["extensions"]["code"],
        json!("INVALID_VARIABLE")
    );
    let message = invalid_debit.body["errors"][0]["message"].as_str().unwrap();
    assert!(message.contains("notify"));
    assert!(message.contains("attribution"));
    assert_eq!(
        proxy.get_log_snapshot()["entries"]
            .as_array()
            .unwrap()
            .len(),
        1,
        "invalid schema variables should not stage store-credit mutations"
    );
}

fn create_store_credit_customer(proxy: &mut DraftProxy) -> String {
    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation StoreCreditSetupCustomer($input: CustomerInput!) {
          customerCreate(input: $input) {
            customer { id email displayName }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "email": "store-credit@example.test",
                "firstName": "Store",
                "lastName": "Credit"
            }
        }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["customerCreate"]["userErrors"],
        json!([])
    );
    create.body["data"]["customerCreate"]["customer"]["id"]
        .as_str()
        .unwrap()
        .to_string()
}

fn store_credit_account_id_from_credit(
    proxy: &mut DraftProxy,
    owner_id: &str,
    amount: &str,
    currency: &str,
) -> String {
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation StoreCreditSetupCredit($id: ID!, $amt: MoneyInput!) {
          storeCreditAccountCredit(id: $id, creditInput: { creditAmount: $amt }) {
            storeCreditAccountTransaction { account { id } }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": owner_id, "amt": { "amount": amount, "currencyCode": currency } }),
    ));
    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["storeCreditAccountCredit"]["userErrors"],
        json!([])
    );
    response.body["data"]["storeCreditAccountCredit"]["storeCreditAccountTransaction"]["account"]
        ["id"]
        .as_str()
        .unwrap()
        .to_string()
}

fn store_credit_credit_error(
    proxy: &mut DraftProxy,
    id: &str,
    amount: Value,
    expires_at: Option<&str>,
) -> Value {
    let mut credit_input = json!({ "creditAmount": amount });
    if let Some(expires_at) = expires_at {
        credit_input["expiresAt"] = json!(expires_at);
    }
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation StoreCreditValidationCredit($id: ID!, $input: StoreCreditAccountCreditInput!) {
          storeCreditAccountCredit(id: $id, creditInput: $input) {
            storeCreditAccountTransaction { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": id, "input": credit_input }),
    ));
    assert_eq!(response.status, 200);
    response.body["data"]["storeCreditAccountCredit"]["userErrors"].clone()
}

fn store_credit_debit_error(proxy: &mut DraftProxy, id: &str, amount: Value) -> Value {
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation StoreCreditValidationDebit($id: ID!, $input: StoreCreditAccountDebitInput!) {
          storeCreditAccountDebit(id: $id, debitInput: $input) {
            storeCreditAccountTransaction { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": id, "input": { "debitAmount": amount } }),
    ));
    assert_eq!(response.status, 200);
    response.body["data"]["storeCreditAccountDebit"]["userErrors"].clone()
}

fn bulk_operation_hydrate_response(operation: Value) -> shopify_draft_proxy::proxy::Response {
    shopify_draft_proxy::proxy::Response {
        status: 200,
        headers: Default::default(),
        body: json!({ "data": { "bulkOperation": operation } }),
    }
}

fn bulk_operation_test_record(
    id: &str,
    status: &str,
    operation_type: &str,
    created_at: &str,
    query: &str,
) -> Value {
    let completed = status == "COMPLETED";
    json!({
        "id": id,
        "status": status,
        "type": operation_type,
        "errorCode": null,
        "createdAt": created_at,
        "completedAt": if completed { json!(created_at) } else { Value::Null },
        "objectCount": if completed { "1424" } else { "0" },
        "rootObjectCount": if completed { "1424" } else { "0" },
        "fileSize": if completed { json!("112704") } else { Value::Null },
        "url": if completed { json!("https://example.test/bulk.jsonl") } else { Value::Null },
        "partialDataUrl": null,
        "query": query
    })
}

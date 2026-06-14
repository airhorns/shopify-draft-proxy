use super::common::*;
use pretty_assertions::assert_eq;

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
fn bulk_operation_run_mutation_validates_client_identifier_and_file_size() {
    let mut proxy = configured_proxy_with_bulk_mutation_max(ReadMode::Snapshot, None, Some(10));
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
                "filename": "oversized-import.jsonl",
                "mimeType": "text/jsonl",
                "httpMethod": "POST",
                "fileSize": "11"
            }]
        }),
    ));
    let path = staged.body["data"]["stagedUploadsCreate"]["stagedTargets"][0]["parameters"]
        .as_array()
        .unwrap()
        .iter()
        .find(|parameter| parameter["name"] == "key")
        .and_then(|parameter| parameter["value"].as_str())
        .unwrap()
        .to_string();
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
            "field": ["stagedUploadPath"],
            "message": "The JSONL file exceeds the maximum allowed size of 100 MB.",
            "code": "INVALID_MUTATION"
        }])
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

#[test]
fn bulk_operation_cancel_routes_arbitrary_bulk_operation_gids_locally() {
    let mut proxy = snapshot_proxy();
    let id = "gid://shopify/BulkOperation/9999999999999";
    let mut cancel_request = json_graphql_request(
        r#"
        mutation BulkOperationCancelParity($id: ID!) {
          bulkOperationCancel(id: $id) {
            bulkOperation { id status type createdAt query }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": id }),
    );
    cancel_request.path = "/admin/api/2025-01/graphql.json".to_string();

    let cancel = proxy.process_request(cancel_request);

    assert_eq!(cancel.status, 200);
    assert_eq!(
        cancel.body["data"]["bulkOperationCancel"]["bulkOperation"]["id"],
        json!(id)
    );
    assert_eq!(
        cancel.body["data"]["bulkOperationCancel"]["bulkOperation"]["status"],
        json!("CANCELING")
    );
    assert_eq!(
        cancel.body["data"]["bulkOperationCancel"]["bulkOperation"]["createdAt"],
        json!("2026-05-05T20:33:59Z")
    );
    assert_eq!(
        cancel.body["data"]["bulkOperationCancel"]["bulkOperation"]["query"],
        json!("#graphql\n{\n  products {\n    edges {\n      node {\n        id\n      }\n    }\n  }\n}")
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
        json!({ "query": "{ products { edges { node { id } } } }" }),
    ));

    assert_eq!(
        response.body["data"]["bulkOperationRunQuery"]["userErrors"],
        json!([
            {
                "field": null,
                "message": "A bulk query operation for this app and shop is already in progress: gid://shopify/BulkOperation/9999999999999.",
                "code": "OPERATION_IN_PROGRESS"
            }
        ])
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
fn customer_update_and_delete_stage_known_fixture_customer_reads() {
    let mut proxy = snapshot_proxy();
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
                "id": "gid://shopify/Customer/9102966915305",
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
        json!({ "input": { "id": "gid://shopify/Customer/9102966915305" } }),
    ));
    assert_eq!(
        delete.body["data"]["customerDelete"],
        json!({
            "deletedCustomerId": "gid://shopify/Customer/9102966915305",
            "shop": { "id": "gid://shopify/Shop/1?shopify-draft-proxy=synthetic" },
            "userErrors": []
        })
    );
    let read = proxy.process_request(json_graphql_request(
        "query($id: ID!) { customer(id: $id) { id email } }",
        json!({ "id": "gid://shopify/Customer/9102966915305" }),
    ));
    assert_eq!(read.body["data"]["customer"], Value::Null);
}

#[test]
fn customer_update_rejects_inline_marketing_consent_without_mutating_customer() {
    let mut proxy = snapshot_proxy();
    let id = "gid://shopify/Customer/9102966915305";
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
fn customer_by_identifier_supports_id_for_input_validation_downstream_reads() {
    let mut proxy = snapshot_proxy();
    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation CustomerUpdateParityPlan($input: CustomerInput!) {
          customerUpdate(input: $input) {
            customer { id firstName defaultPhoneNumber { phoneNumber } tags }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "id": "gid://shopify/Customer/9102966915305", "firstName": "", "lastName": "", "phone": "", "tags": ["Zulu", "alpha", "spaced tag"] } }),
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
        json!(["Zulu", "alpha", "spaced tag"])
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
fn quantity_pricing_by_variant_update_returns_seeded_variant_ids_for_b2b_quantity_rules() {
    let mut proxy = snapshot_proxy();

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation QuantityPricingByVariantUpdate($priceListId: ID!, $input: QuantityPricingByVariantUpdateInput!) {
          quantityPricingByVariantUpdate(priceListId: $priceListId, input: $input) {
            productVariants { id }
            userErrors { field code message }
          }
        }
        "#,
        json!({
            "priceListId": "gid://shopify/PriceList/31575376178",
            "input": {
                "pricesToAdd": [{
                    "variantId": "gid://shopify/ProductVariant/49875425296690",
                    "price": { "amount": "20.00", "currencyCode": "CAD" }
                }],
                "quantityRulesToAdd": [{
                    "variantId": "gid://shopify/ProductVariant/49875425296690",
                    "minimum": 1,
                    "maximum": 20,
                    "increment": 1
                }],
                "quantityPriceBreaksToAdd": [{
                    "variantId": "gid://shopify/ProductVariant/49875425296690",
                    "minimumQuantity": 10,
                    "price": { "amount": "18.00", "currencyCode": "CAD" }
                }]
            }
        }),
    ));

    assert_eq!(
        response.body["data"]["quantityPricingByVariantUpdate"],
        json!({
            "productVariants": [{ "id": "gid://shopify/ProductVariant/49875425296690" }],
            "userErrors": []
        })
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

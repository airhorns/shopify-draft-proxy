use super::common::*;
use shopify_draft_proxy::proxy::UnsupportedMutationMode;

#[test]
fn storefront_graphql_route_proxies_request_with_storefront_token_header() {
    let observed_requests = Arc::new(Mutex::new(Vec::<Request>::new()));
    let observed_for_proxy = Arc::clone(&observed_requests);
    let mut proxy = configured_proxy(
        ReadMode::LiveHybrid,
        Some(UnsupportedMutationMode::Passthrough),
    )
    .with_upstream_transport(move |request| {
        observed_for_proxy.lock().unwrap().push(request);
        Response {
            status: 200,
            headers: Default::default(),
            body: json!({
                "data": {
                    "shop": {
                        "name": "Storefront cassette shop"
                    }
                }
            }),
        }
    });

    let request_body = json!({
        "query": "query StorefrontShopNameProxyParity { shop { name } }",
        "variables": {}
    })
    .to_string();
    let response = proxy.process_request(Request {
        method: "POST".to_string(),
        path: "/api/2026-04/graphql.json".to_string(),
        headers: [(
            "x-shopify-storefront-access-token".to_string(),
            "shpat_storefront_token".to_string(),
        )]
        .into(),
        body: request_body.clone(),
    });

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["shop"]["name"],
        json!("Storefront cassette shop")
    );

    let observed = observed_requests.lock().unwrap();
    assert_eq!(observed.len(), 1);
    assert_eq!(observed[0].method, "POST");
    assert_eq!(observed[0].path, "/api/2026-04/graphql.json");
    assert_eq!(
        observed[0].headers.get("x-shopify-storefront-access-token"),
        Some(&"shpat_storefront_token".to_string())
    );
    assert_eq!(observed[0].body, request_body);
}

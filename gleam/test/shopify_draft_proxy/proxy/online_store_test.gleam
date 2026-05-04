import gleam/dict
import gleam/json
import gleam/string
import shopify_draft_proxy/proxy/draft_proxy
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, Request, Response,
}

fn proxy() -> DraftProxy {
  draft_proxy.new()
  |> draft_proxy.with_default_registry()
}

fn graphql_request(query: String) -> Request {
  Request(
    method: "POST",
    path: "/admin/api/2026-04/graphql.json",
    headers: dict.new(),
    body: json.to_string(json.object([#("query", json.string(query))])),
  )
}

fn meta_state_request() -> Request {
  Request(method: "GET", path: "/__meta/state", headers: dict.new(), body: "")
}

fn run_graphql(proxy: DraftProxy, query: String) -> #(String, DraftProxy) {
  let #(Response(status: status, body: body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))
  assert status == 200
  #(json.to_string(body), proxy)
}

fn read_state(proxy: DraftProxy) -> String {
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, meta_state_request())
  assert status == 200
  json.to_string(body)
}

pub fn web_pixel_duplicate_create_returns_taken_error_test() {
  let query =
    "mutation { webPixelCreate(webPixel: { settings: \"{\\\"accountID\\\":\\\"abc\\\"}\" }) { webPixel { id status } userErrors { __typename code field message } } }"
  let #(first, proxy) = run_graphql(proxy(), query)
  assert first
    == "{\"data\":{\"webPixelCreate\":{\"webPixel\":{\"id\":\"gid://shopify/WebPixel/1?shopify-draft-proxy=synthetic\",\"status\":\"CONNECTED\"},\"userErrors\":[]}}}"

  let #(second, _) = run_graphql(proxy, query)
  assert second
    == "{\"data\":{\"webPixelCreate\":{\"webPixel\":null,\"userErrors\":[{\"__typename\":\"WebPixelUserError\",\"code\":\"TAKEN\",\"field\":null,\"message\":\"Web pixel is taken.\"}]}}}"
}

pub fn web_pixel_create_without_settings_needs_configuration_test() {
  let query =
    "mutation { webPixelCreate(webPixel: {}) { webPixel { id status settings } userErrors { __typename code field message } } }"
  let #(body, _) = run_graphql(proxy(), query)
  assert body
    == "{\"data\":{\"webPixelCreate\":{\"webPixel\":{\"id\":\"gid://shopify/WebPixel/1?shopify-draft-proxy=synthetic\",\"status\":\"NEEDS_CONFIGURATION\",\"settings\":null},\"userErrors\":[]}}}"
}

pub fn web_pixel_update_and_delete_errors_use_web_pixel_user_error_test() {
  let update_query =
    "mutation { webPixelUpdate(id: \"gid://shopify/WebPixel/missing\", webPixel: { settings: \"{}\" }) { webPixel { id } userErrors { __typename code field message } } }"
  let #(update_body, proxy) = run_graphql(proxy(), update_query)
  assert update_body
    == "{\"data\":{\"webPixelUpdate\":{\"webPixel\":null,\"userErrors\":[{\"__typename\":\"WebPixelUserError\",\"code\":null,\"field\":[\"id\"],\"message\":\"Pixel does not exist\"}]}}}"

  let delete_query =
    "mutation { webPixelDelete(id: \"gid://shopify/WebPixel/missing\") { deletedWebPixelId userErrors { __typename code field message } } }"
  let #(delete_body, _) = run_graphql(proxy, delete_query)
  assert delete_body
    == "{\"data\":{\"webPixelDelete\":{\"deletedWebPixelId\":null,\"userErrors\":[{\"__typename\":\"WebPixelUserError\",\"code\":null,\"field\":[\"id\"],\"message\":\"Integration does not exist\"}]}}}"
}

pub fn web_pixel_state_omits_webhook_endpoint_address_test() {
  let query =
    "mutation { webPixelCreate(webPixel: { settings: \"{}\" }) { webPixel { id status webhookEndpointAddress } userErrors { field message } } }"
  let #(body, proxy) = run_graphql(proxy(), query)
  assert body
    == "{\"data\":{\"webPixelCreate\":{\"webPixel\":{\"id\":\"gid://shopify/WebPixel/1?shopify-draft-proxy=synthetic\",\"status\":\"CONNECTED\",\"webhookEndpointAddress\":null},\"userErrors\":[]}}}"

  let state = read_state(proxy)
  assert string.contains(state, "onlineStoreWebPixels")
  assert string.contains(state, "webhookEndpointAddress") == False
}

pub fn server_pixel_state_keeps_webhook_endpoint_address_test() {
  let query =
    "mutation { serverPixelCreate { serverPixel { id status webhookEndpointAddress } userErrors { field message } } }"
  let #(body, proxy) = run_graphql(proxy(), query)
  assert body
    == "{\"data\":{\"serverPixelCreate\":{\"serverPixel\":{\"id\":\"gid://shopify/ServerPixel/1?shopify-draft-proxy=synthetic\",\"status\":\"CONNECTED\",\"webhookEndpointAddress\":null},\"userErrors\":[]}}}"

  let state = read_state(proxy)
  assert string.contains(state, "onlineStoreServerPixels")
  assert string.contains(state, "webhookEndpointAddress")
}

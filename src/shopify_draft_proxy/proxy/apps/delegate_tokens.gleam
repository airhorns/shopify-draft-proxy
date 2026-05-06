//// Delegated access token helpers for app mutations.

import gleam/dict.{type Dict}
import gleam/option.{type Option, None, Some}
import gleam/string
import shopify_draft_proxy/crypto
import shopify_draft_proxy/proxy/app_identity
import shopify_draft_proxy/proxy/apps/types as app_types
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/types.{type DelegatedAccessTokenRecord}

@internal
pub fn active_parent_is_delegate(
  store: Store,
  request_headers: Dict(String, String),
) -> Bool {
  case active_access_token(request_headers) {
    Some(raw) ->
      case store.find_delegated_access_token_by_hash(store, token_hash(raw)) {
        Some(_) -> True
        None -> False
      }
    None -> False
  }
}

@internal
pub fn active_access_token(headers: Dict(String, String)) -> Option(String) {
  active_access_token_from_pairs(dict.to_list(headers))
}

fn active_access_token_from_pairs(
  headers: List(#(String, String)),
) -> Option(String) {
  case headers {
    [] -> None
    [#(key, value), ..rest] -> {
      case string.lowercase(key) {
        "x-shopify-access-token" -> Some(string.trim(value))
        "authorization" -> bearer_token(value, rest)
        _ -> active_access_token_from_pairs(rest)
      }
    }
  }
}

fn bearer_token(
  value: String,
  rest: List(#(String, String)),
) -> Option(String) {
  let trimmed = string.trim(value)
  case string.starts_with(string.lowercase(trimmed), "bearer ") {
    True -> Some(string.trim(string.drop_start(trimmed, 7)))
    False -> active_access_token_from_pairs(rest)
  }
}

@internal
pub fn caller_api_client_id(
  store: Store,
  request_headers: Dict(String, String),
) -> String {
  case app_identity.read_requesting_api_client_id(request_headers) {
    Some(id) -> id
    None ->
      case store.get_current_app_installation(store) {
        Some(installation) -> installation.app_id
        None -> app_types.default_delegate_api_client_id
      }
  }
}

@internal
pub fn delegated_token_hash_exists(store: Store, hash: String) -> Bool {
  case
    find_delegated_token_by_hash_any_state(
      dict.to_list(store.staged_state.delegated_access_tokens),
      hash,
    )
  {
    True -> True
    False ->
      find_delegated_token_by_hash_any_state(
        dict.to_list(store.base_state.delegated_access_tokens),
        hash,
      )
  }
}

fn find_delegated_token_by_hash_any_state(
  tokens: List(#(String, DelegatedAccessTokenRecord)),
  hash: String,
) -> Bool {
  case tokens {
    [] -> False
    [#(_, token), ..rest] ->
      case token.access_token_sha256 == hash {
        True -> True
        False -> find_delegated_token_by_hash_any_state(rest, hash)
      }
  }
}

@internal
pub fn destroy_error(
  message: String,
  code: String,
) -> app_types.DelegateAccessTokenUserError {
  app_types.DelegateAccessTokenUserError(
    field: None,
    message: message,
    code: Some(code),
  )
}

@internal
pub fn destroy_in_hierarchy(
  record: DelegatedAccessTokenRecord,
  active_token_hash: Option(String),
) -> Bool {
  case active_token_hash {
    Some(hash) ->
      record.access_token_sha256 == hash
      || record.parent_access_token_sha256 == Some(hash)
      || record.parent_access_token_sha256 == None
    None -> record.parent_access_token_sha256 == None
  }
}

fn token_hash(raw: String) -> String {
  crypto.sha256_hex(raw)
}

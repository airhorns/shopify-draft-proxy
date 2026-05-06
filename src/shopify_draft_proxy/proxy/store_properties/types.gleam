//// Shared internal Store Properties domain types.

import gleam/json.{type Json}
import gleam/option.{type Option}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{type ShopPolicyRecord}

@internal
pub type ShopPolicyUserError {
  ShopPolicyUserError(
    field: Option(List(String)),
    message: String,
    code: Option(String),
  )
}

@internal
pub type PolicyValidation {
  PolicyValidation(
    type_: Option(String),
    body: Option(String),
    user_errors: List(ShopPolicyUserError),
  )
}

@internal
pub type StagePolicyResult {
  StagePolicyResult(
    shop_policy: Option(ShopPolicyRecord),
    user_errors: List(ShopPolicyUserError),
    store: Store,
    identity: SyntheticIdentityRegistry,
    staged_resource_ids: List(String),
  )
}

@internal
pub type GenericMutationResult {
  GenericMutationResult(
    payload: Json,
    store: Store,
    identity: SyntheticIdentityRegistry,
    staged_resource_ids: List(String),
    top_level_errors: List(Json),
    should_log: Bool,
  )
}

@internal
pub type LocationEditUserError {
  LocationEditUserError(
    field: List(String),
    message: String,
    code: Option(String),
  )
}

@internal
pub type QueryFieldResult {
  QueryFieldResult(key: String, value: Json, errors: List(Json))
}

@internal
pub type LocationInventoryQuantity {
  LocationInventoryQuantity(name: String, quantity: Int)
}

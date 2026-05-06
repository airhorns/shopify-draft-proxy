//// Shared internal customer domain types.

import gleam/json
import gleam/option.{type Option}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type CustomerAddressRecord, type CustomerMetafieldRecord,
  type CustomerOrderSummaryRecord, type CustomerRecord,
  type StoreCreditAccountRecord,
}

@internal
pub type CustomersError {
  ParseFailed(root_field.RootFieldError)
}

@internal
pub type UserError {
  UserError(field: List(String), message: String, code: Option(String))
}

@internal
pub type AddressZoneResolution {
  AddressZoneResolution(
    country: Option(String),
    country_code: Option(String),
    province: Option(String),
    province_code: Option(String),
  )
}

@internal
pub type StoreCreditAccountResolution {
  StoreCreditAccountResolved(
    account: StoreCreditAccountRecord,
    identity: SyntheticIdentityRegistry,
  )
  StoreCreditAccountResolutionError(error: UserError)
}

@internal
pub type MutationFieldResult {
  MutationFieldResult(
    key: String,
    payload: json.Json,
    staged_resource_ids: List(String),
    root_name: String,
  )
}

@internal
pub type CustomerHydrateResult {
  CustomerHydrateResult(
    customer: CustomerRecord,
    addresses: List(CustomerAddressRecord),
    metafields: List(CustomerMetafieldRecord),
    orders: List(CustomerOrderSummaryRecord),
    store_credit_accounts: List(StoreCreditAccountRecord),
  )
}

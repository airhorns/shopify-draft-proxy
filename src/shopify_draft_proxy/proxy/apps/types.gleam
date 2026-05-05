//// Shared apps implementation types.

import gleam/option.{type Option}

@internal
pub type UserError {
  UserError(field: List(String), message: String, code: Option(String))
}

@internal
pub type DelegateAccessTokenUserError {
  DelegateAccessTokenUserError(
    field: Option(List(String)),
    message: String,
    code: Option(String),
  )
}

@internal
pub const default_billing_currency = "USD"

@internal
pub const minimum_one_time_purchase_amount = 0.5

@internal
pub const minimum_one_time_purchase_amount_label = "0.50"

@internal
pub const synthetic_shop_id = "gid://shopify/Shop/1?shopify-draft-proxy=synthetic"

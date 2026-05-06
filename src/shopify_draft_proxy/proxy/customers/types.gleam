//// Public type aliases for the split customer domain.

import shopify_draft_proxy/proxy/customers/customer_types

@internal
pub type CustomersError =
  customer_types.CustomersError

@internal
pub type UserError =
  customer_types.UserError

@internal
pub type AddressZoneResolution =
  customer_types.AddressZoneResolution

@internal
pub type StoreCreditAccountResolution =
  customer_types.StoreCreditAccountResolution

@internal
pub type MutationFieldResult =
  customer_types.MutationFieldResult

@internal
pub type CustomerHydrateResult =
  customer_types.CustomerHydrateResult

//// Shared bulk-operations domain types.

import shopify_draft_proxy/graphql/root_field

pub type BulkOperationsError {
  ParseFailed(root_field.RootFieldError)
}

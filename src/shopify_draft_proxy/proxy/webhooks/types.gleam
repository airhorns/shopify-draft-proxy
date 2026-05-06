//// Shared webhook domain types.

import shopify_draft_proxy/graphql/root_field

@internal
pub type WebhooksError {
  ParseFailed(root_field.RootFieldError)
}

@internal
pub type UserError {
  UserError(field: List(String), message: String)
}

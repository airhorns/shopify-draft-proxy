//// Shared gift-card domain types.

import gleam/option.{type Option}
import shopify_draft_proxy/graphql/root_field

/// Errors specific to the gift-cards handler.
pub type GiftCardsError {
  ParseFailed(root_field.RootFieldError)
}

/// User-error payload. Most gift-card mutation guardrails expose Shopify's
/// typed enum code strings; duplicate-code create validation preserves the
/// public Admin API's captured `null` code.
pub type UserError {
  UserError(field: List(String), code: Option(String), message: String)
}

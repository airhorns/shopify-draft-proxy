//// Shared Files API domain types.

import gleam/option.{type Option}
import shopify_draft_proxy/graphql/root_field

pub type MediaError {
  ParseFailed(root_field.RootFieldError)
}

@internal
pub type FilesUserError {
  FilesUserError(field: List(String), message: String, code: String)
}

@internal
pub type StagedTarget {
  StagedTarget(
    url: Option(String),
    resource_url: Option(String),
    parameters: List(#(String, String)),
  )
}

import gleam/option.{type Option}

/// User-error payload. Mirrors the TS `FunctionUserError` shape (path,
/// message, optional code).
@internal
pub type UserError {
  UserError(field: List(String), message: String, code: Option(String))
}

@internal
pub type FunctionReference {
  FunctionReference(
    function_id: Option(String),
    function_handle: Option(String),
  )
}

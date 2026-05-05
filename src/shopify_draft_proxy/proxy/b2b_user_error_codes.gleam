//// Canonical `BusinessCustomerUserError` code values.
////
//// Shopify exposes these as GraphQL enum values serialized in UPPER_SNAKE
//// form. Keep B2B mutation handlers passing this opaque type instead of raw
//// strings so local error responses cannot drift to unsupported code values.

import gleam/list

pub opaque type Code {
  Code(value: String)
}

pub const resource_not_found: Code = Code("RESOURCE_NOT_FOUND")

pub const taken: Code = Code("TAKEN")

pub const invalid_input: Code = Code("INVALID_INPUT")

pub const limit_reached: Code = Code("LIMIT_REACHED")

pub const no_input: Code = Code("NO_INPUT")

pub const internal_error: Code = Code("INTERNAL_ERROR")

pub const failed_to_delete: Code = Code("FAILED_TO_DELETE")

pub const required: Code = Code("REQUIRED")

pub const invalid: Code = Code("INVALID")

pub const blank: Code = Code("BLANK")

pub const too_long: Code = Code("TOO_LONG")

pub const unexpected_type: Code = Code("UNEXPECTED_TYPE")

pub fn value(code: Code) -> String {
  code.value
}

pub fn all() -> List(Code) {
  [
    internal_error,
    resource_not_found,
    failed_to_delete,
    required,
    no_input,
    invalid_input,
    unexpected_type,
    too_long,
    limit_reached,
    invalid,
    blank,
    taken,
  ]
}

pub fn all_values() -> List(String) {
  all() |> list.map(value)
}

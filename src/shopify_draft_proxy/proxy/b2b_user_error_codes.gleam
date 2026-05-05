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

pub const invalid: Code = Code("INVALID")

pub const blank: Code = Code("BLANK")

pub const too_long: Code = Code("TOO_LONG")

pub const contains_html_tags: Code = Code("CONTAINS_HTML_TAGS")

pub const invalid_locale_format: Code = Code("INVALID_LOCALE_FORMAT")

pub const duplicate_external_id: Code = Code("DUPLICATE_EXTERNAL_ID")

pub const duplicate_location_external_id: Code = Code(
  "DUPLICATE_LOCATION_EXTERNAL_ID",
)

pub const duplicate_email_address: Code = Code("DUPLICATE_EMAIL_ADDRESS")

pub const duplicate_phone_number: Code = Code("DUPLICATE_PHONE_NUMBER")

pub const customer_not_found: Code = Code("CUSTOMER_NOT_FOUND")

pub const customer_already_a_contact: Code = Code("CUSTOMER_ALREADY_A_CONTACT")

pub const customer_email_must_exist: Code = Code("CUSTOMER_EMAIL_MUST_EXIST")

pub const company_contact_max_cap_reached: Code = Code(
  "COMPANY_CONTACT_MAX_CAP_REACHED",
)

pub const role_assignments_max_cap_reached: Code = Code(
  "ROLE_ASSIGNMENTS_MAX_CAP_REACHED",
)

pub const failed_to_delete: Code = Code("FAILED_TO_DELETE")

pub const one_role_already_assigned: Code = Code("ONE_ROLE_ALREADY_ASSIGNED")

pub const contact_does_not_match_company: Code = Code(
  "CONTACT_DOES_NOT_MATCH_COMPANY",
)

pub const existing_orders: Code = Code("EXISTING_ORDERS")

pub fn value(code: Code) -> String {
  code.value
}

pub fn all() -> List(Code) {
  [
    resource_not_found,
    taken,
    invalid_input,
    limit_reached,
    no_input,
    internal_error,
    invalid,
    blank,
    too_long,
    contains_html_tags,
    invalid_locale_format,
    duplicate_external_id,
    duplicate_location_external_id,
    duplicate_email_address,
    duplicate_phone_number,
    customer_not_found,
    customer_already_a_contact,
    customer_email_must_exist,
    company_contact_max_cap_reached,
    role_assignments_max_cap_reached,
    failed_to_delete,
    one_role_already_assigned,
    contact_does_not_match_company,
    existing_orders,
  ]
}

pub fn all_values() -> List(String) {
  all() |> list.map(value)
}

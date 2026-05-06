//// Shared internal B2B domain types and constants.

import gleam/option.{type Option}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/b2b_user_error_codes as user_error_code
import shopify_draft_proxy/proxy/graphql_helpers.{type SourceValue}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type B2BCompanyContactRecord, type B2BCompanyLocationRecord,
  type B2BCompanyRecord,
}

pub const domain = "b2b"

pub const default_string_max_length = 255

pub const notes_max_length = 5000

pub const external_id_max_length = 64

pub const external_id_invalid_chars_detail = "external_id_contains_invalid_chars"

pub const external_id_invalid_chars_message = "External Id can only contain numbers, letters, and some special characters, including !@#$%^&*(){}[]\\/?<>_-~,.;:'`\""

pub const company_contact_maximum_cap = 10_000

pub const bulk_actions_max_size = 50

pub const bulk_action_limit_reached_message = "Cannot perform more than 50 actions in a single request."

pub const contains_html_tags_detail = "contains_html_tags"

pub const invalid_locale_format_detail = "invalid_locale_format"

pub const duplicate_external_id_detail = "duplicate_external_id"

pub const duplicate_location_external_id_detail = "duplicate_location_external_id"

pub const duplicate_email_address_detail = "duplicate_email_address"

pub const duplicate_phone_number_detail = "duplicate_phone_number"

pub const customer_not_found_detail = "customer_not_found"

pub const customer_already_a_contact_detail = "customer_already_a_contact"

pub const customer_email_must_exist_detail = "customer_email_must_exist"

pub const company_contact_max_cap_reached_detail = "company_contact_max_cap_reached"

pub const one_role_already_assigned_detail = "one_role_already_assigned"

pub const contact_does_not_match_company_detail = "contact_does_not_match_company"

pub const existing_orders_detail = "existing_orders"

@internal
pub type UserError {
  UserError(
    field: Option(List(String)),
    message: String,
    code: user_error_code.Code,
    detail: Option(String),
  )
}

@internal
pub type Payload {
  Payload(
    company: Option(B2BCompanyRecord),
    company_contact: Option(B2BCompanyContactRecord),
    company_location: Option(B2BCompanyLocationRecord),
    company_contact_role_assignment: Option(SourceValue),
    role_assignments: List(SourceValue),
    addresses: List(SourceValue),
    company_location_staff_member_assignments: List(SourceValue),
    deleted_company_id: Option(String),
    deleted_company_ids: List(String),
    deleted_company_contact_id: Option(String),
    deleted_company_contact_ids: List(String),
    deleted_company_location_id: Option(String),
    deleted_company_location_ids: List(String),
    deleted_address_id: Option(String),
    revoked_company_contact_role_assignment_id: Option(String),
    revoked_role_assignment_ids: List(String),
    revoked_role_assignment_ids_null: Bool,
    deleted_company_location_staff_member_assignment_ids: List(String),
    removed_company_contact_id: Option(String),
    user_errors: List(UserError),
  )
}

@internal
pub type RootResult {
  RootResult(
    payload: Payload,
    store: Store,
    identity: SyntheticIdentityRegistry,
    staged_ids: List(String),
  )
}

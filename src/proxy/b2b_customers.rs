use super::*;

mod addresses;
mod companies;
mod consent;
mod customers;
mod merge_erasure;

use self::addresses::{
    customer_address_contains_url, customer_address_cursor, customer_address_dedup_key,
    customer_address_field_path, customer_address_nodes, customer_address_string,
    customer_country_from_input, customer_mailing_addresses, customer_rebuild_addresses,
    customer_update_mailing_address, selected_customer_addresses_connection,
};
pub(in crate::proxy) use self::companies::*;
pub(in crate::proxy) use self::consent::{
    b2b_tax_settings_invalid_enum_response, customer_sms_consent_invalid_enum_response,
    customer_tax_exemptions_invalid_enum_response,
};
use self::consent::{customer_update_inline_consent_errors, resolved_inline_consent_state};
use self::customers::apply_customer_marketing_consent;
pub(in crate::proxy) use self::customers::is_valid_customer_email;
use self::merge_erasure::{
    connection_has_nodes, customer_merge_extract_order_records, customer_merge_job_from_request,
    nodes_connection, order_connection_cursor,
};

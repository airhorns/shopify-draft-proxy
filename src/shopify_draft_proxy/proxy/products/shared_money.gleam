//// Products-domain submodule: shared_money.
//// Combines layered files: shared_l02.

import gleam/dict.{type Dict}

import gleam/option.{None, Some}

import shopify_draft_proxy/graphql/ast.{type Selection}

import shopify_draft_proxy/graphql/root_field.{type ResolvedValue}

import shopify_draft_proxy/proxy/graphql_helpers.{
  type SourceValue, SrcString, src_object,
}

import shopify_draft_proxy/proxy/products/products_core.{format_price_amount}
import shopify_draft_proxy/proxy/products/shared.{
  admin_api_version_from_path, compare_admin_api_versions,
  parse_admin_api_version, read_idempotency_key,
}

// ===== from shared_l02 =====
@internal
pub fn money_v2_source(amount: String, currency_code: String) -> SourceValue {
  src_object([
    #("__typename", SrcString("MoneyV2")),
    #("amount", SrcString(format_price_amount(amount))),
    #("currencyCode", SrcString(currency_code)),
  ])
}

@internal
pub fn admin_api_version_at_least(
  request_path: String,
  minimum_version: String,
) -> Bool {
  case
    admin_api_version_from_path(request_path),
    parse_admin_api_version(minimum_version)
  {
    Some(version), Some(minimum) -> compare_admin_api_versions(version, minimum)
    _, _ -> False
  }
}

@internal
pub fn has_idempotency_key(
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> Bool {
  case read_idempotency_key(field, variables) {
    Some(_) -> True
    None -> False
  }
}

//// Products-domain submodule: selling_plans_core.
//// Combines layered files: selling_plans_l00, selling_plans_l01, selling_plans_l02, selling_plans_l03, selling_plans_l04.

import gleam/dict.{type Dict}

import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}

import shopify_draft_proxy/graphql/ast.{type Selection}

import shopify_draft_proxy/graphql/root_field.{type ResolvedValue, ObjectVal}

import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, SerializeConnectionConfig, SrcList,
  SrcString, default_connection_page_info_options,
  default_connection_window_options, default_selected_field_options,
  get_selected_child_fields, paginate_connection_items, project_graphql_value,
  serialize_connection, src_object,
}

import shopify_draft_proxy/proxy/products/product_types.{
  type ProductUserError, ProductUserError,
}
import shopify_draft_proxy/proxy/products/products_core.{
  existing_group_app_id, existing_group_description,
  existing_group_merchant_code, existing_group_name, existing_group_position,
}
import shopify_draft_proxy/proxy/products/shared.{
  captured_int_field, captured_json_source, captured_number_string_field,
  captured_object_field, captured_object_or_null, captured_string_array_field,
  captured_string_field, dedupe_preserving_order, read_int_field,
  read_number_captured_field, read_object_field, read_object_list_field,
  read_string_argument, read_string_field, read_string_list_field,
  resolved_value_to_captured,
}
import shopify_draft_proxy/proxy/products/variants_helpers.{
  existing_group_options, option_to_result, optional_captured_int,
  optional_captured_string,
}

import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type CapturedJsonValue, type SellingPlanGroupRecord, type SellingPlanRecord,
  CapturedArray, CapturedNull, CapturedObject, CapturedString,
  SellingPlanGroupRecord, SellingPlanRecord,
}

// ===== from selling_plans_l00 =====
@internal
pub fn selling_plan_group_cursor(
  group: SellingPlanGroupRecord,
  _index: Int,
) -> String {
  case group.cursor {
    Some(cursor) -> cursor
    None -> group.id
  }
}

@internal
pub fn selling_plan_cursor(plan: SellingPlanRecord, _index: Int) -> String {
  plan.id
}

@internal
pub fn selling_plan_group_summary_source(
  group: SellingPlanGroupRecord,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("SellingPlanGroup")),
    #("id", SrcString(group.id)),
    #("name", SrcString(group.name)),
    #("merchantCode", SrcString(group.merchant_code)),
  ])
}

@internal
pub fn selling_plan_group_staged_ids(
  group: SellingPlanGroupRecord,
) -> List(String) {
  [group.id, ..list.map(group.selling_plans, fn(plan) { plan.id })]
}

@internal
pub fn find_selling_plan(
  plans: List(SellingPlanRecord),
  id: String,
) -> Option(SellingPlanRecord) {
  plans
  |> list.find(fn(plan) { plan.id == id })
  |> option.from_result
}

@internal
pub fn replace_selling_plan(
  plans: List(SellingPlanRecord),
  next_plan: SellingPlanRecord,
) -> List(SellingPlanRecord) {
  list.map(plans, fn(plan) {
    case plan.id == next_plan.id {
      True -> next_plan
      False -> plan
    }
  })
}

// ===== from selling_plans_l01 =====
@internal
pub fn serialize_selling_plans_connection(
  plans: List(SellingPlanRecord),
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let window =
    paginate_connection_items(
      plans,
      field,
      variables,
      selling_plan_cursor,
      default_connection_window_options(),
    )
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: selling_plan_cursor,
      serialize_node: fn(plan, node_field, _index) {
        project_graphql_value(
          captured_json_source(plan.data),
          get_selected_child_fields(
            node_field,
            default_selected_field_options(),
          ),
          fragments,
        )
      },
      selected_field_options: default_selected_field_options(),
      page_info_options: default_connection_page_info_options(),
    ),
  )
}

@internal
pub fn product_variant_count_for_selling_plan_group(
  store: Store,
  group: SellingPlanGroupRecord,
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> Int {
  case read_string_argument(field, variables, "productId") {
    Some(product_id) ->
      group.product_variant_ids
      |> list.filter(fn(variant_id) {
        case store.get_effective_variant_by_id(store, variant_id) {
          Some(variant) -> variant.product_id == product_id
          None -> False
        }
      })
      |> list.length
    None -> list.length(group.product_variant_ids)
  }
}

@internal
pub fn serialize_selling_plan_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  let plan =
    store.list_effective_selling_plan_groups(store)
    |> list.flat_map(fn(group) { group.selling_plans })
    |> list.find(fn(plan) { plan.id == id })
  case plan {
    Ok(plan) ->
      project_graphql_value(
        captured_json_source(plan.data),
        selections,
        fragments,
      )
    Error(_) -> json.null()
  }
}

@internal
pub fn selling_plan_group_connection_source(
  groups: List(SellingPlanGroupRecord),
) -> SourceValue {
  src_object([
    #("nodes", SrcList(list.map(groups, selling_plan_group_summary_source))),
  ])
}

@internal
pub fn selling_plan_delivery_policy(
  input: Dict(String, ResolvedValue),
  existing: Option(CapturedJsonValue),
) -> CapturedJsonValue {
  case read_object_field(input, "recurring") {
    Some(recurring) ->
      CapturedObject([
        #("__typename", CapturedString("SellingPlanRecurringDeliveryPolicy")),
        #(
          "interval",
          optional_captured_string(read_string_field(recurring, "interval")),
        ),
        #(
          "intervalCount",
          optional_captured_int(read_int_field(recurring, "intervalCount")),
        ),
        #("anchors", captured_anchor_inputs(recurring)),
        #("cutoff", optional_captured_int(read_int_field(recurring, "cutoff"))),
        #(
          "intent",
          CapturedString(
            read_string_field(recurring, "intent")
            |> option.unwrap("FULFILLMENT_BEGIN"),
          ),
        ),
        #(
          "preAnchorBehavior",
          CapturedString(
            read_string_field(recurring, "preAnchorBehavior")
            |> option.unwrap("ASAP"),
          ),
        ),
      ])
    None ->
      case read_object_field(input, "fixed") {
        Some(fixed) ->
          CapturedObject([
            #("__typename", CapturedString("SellingPlanFixedDeliveryPolicy")),
            #("cutoff", optional_captured_int(read_int_field(fixed, "cutoff"))),
            #(
              "fulfillmentTrigger",
              optional_captured_string(read_string_field(
                fixed,
                "fulfillmentTrigger",
              )),
            ),
            #(
              "fulfillmentExactTime",
              optional_captured_string(read_string_field(
                fixed,
                "fulfillmentExactTime",
              )),
            ),
            #(
              "intent",
              optional_captured_string(read_string_field(fixed, "intent")),
            ),
            #(
              "preAnchorBehavior",
              optional_captured_string(read_string_field(
                fixed,
                "preAnchorBehavior",
              )),
            ),
          ])
        None ->
          existing
          |> option.unwrap(
            CapturedObject([
              #(
                "__typename",
                CapturedString("SellingPlanRecurringDeliveryPolicy"),
              ),
            ]),
          )
      }
  }
}

@internal
pub fn selling_plan_policy_value(
  input: Dict(String, ResolvedValue),
) -> CapturedJsonValue {
  case read_number_captured_field(input, "fixedValue") {
    Some(value) ->
      CapturedObject([
        #("__typename", CapturedString("SellingPlanPricingPolicyFixedValue")),
        #("fixedValue", value),
      ])
    None ->
      case read_string_field(input, "fixedValue") {
        Some(value) ->
          CapturedObject([
            #(
              "__typename",
              CapturedString("SellingPlanPricingPolicyFixedValue"),
            ),
            #("fixedValue", CapturedString(value)),
          ])
        None ->
          CapturedObject([
            #(
              "__typename",
              CapturedString("SellingPlanPricingPolicyPercentageValue"),
            ),
            #(
              "percentage",
              read_number_captured_field(input, "percentage")
                |> option.unwrap(CapturedNull),
            ),
          ])
      }
  }
}

@internal
pub fn selling_plan_group_does_not_exist_error() -> ProductUserError {
  ProductUserError(
    ["id"],
    "Selling plan group does not exist.",
    Some("GROUP_DOES_NOT_EXIST"),
  )
}

pub const max_selling_plan_group_memberships = 31

@internal
pub fn selling_plan_group_ids_blank_error() -> ProductUserError {
  ProductUserError(
    ["sellingPlanGroupIds"],
    "Selling plan group IDs can't be blank",
    Some("BLANK"),
  )
}

@internal
pub fn duplicate_selling_plan_group_ids_error() -> ProductUserError {
  ProductUserError(
    ["sellingPlanGroupIds"],
    "Selling plan group IDs contains duplicate values.",
    Some("DUPLICATE"),
  )
}

@internal
pub fn too_many_selling_plan_groups_error() -> ProductUserError {
  ProductUserError(
    ["sellingPlanGroupIds"],
    "Cannot join more than 31 selling plan groups.",
    Some("SELLING_PLAN_GROUPS_TOO_MANY"),
  )
}

@internal
pub fn selling_plan_group_not_member_error() -> ProductUserError {
  ProductUserError(
    ["sellingPlanGroupIds"],
    "Selling plan group is not a member.",
    Some("NOT_A_MEMBER"),
  )
}

// ===== from selling_plans_l02 =====
@internal
pub fn update_product_selling_plan_group_membership(
  store: Store,
  product_id: String,
  group_ids: List(String),
  join: Bool,
) -> #(Store, List(ProductUserError), List(String)) {
  case
    validate_product_selling_plan_group_membership(
      store,
      product_id,
      group_ids,
      join,
    )
  {
    [_, ..] as errors -> #(store, errors, [])
    [] ->
      apply_product_selling_plan_group_membership(
        store,
        product_id,
        group_ids,
        join,
      )
  }
}

fn apply_product_selling_plan_group_membership(
  store: Store,
  product_id: String,
  group_ids: List(String),
  join: Bool,
) -> #(Store, List(ProductUserError), List(String)) {
  let #(next_store, errors, staged_ids) =
    list.fold(group_ids, #(store, [], []), fn(acc, group_id) {
      let #(current_store, current_errors, current_ids) = acc
      case
        store.get_effective_selling_plan_group_by_id(current_store, group_id)
      {
        None -> #(
          current_store,
          list.append(current_errors, [
            selling_plan_group_does_not_exist_error(),
          ]),
          current_ids,
        )
        Some(group) -> {
          let next_product_ids = case join {
            True ->
              dedupe_preserving_order(
                list.append(group.product_ids, [
                  product_id,
                ]),
              )
            False ->
              group.product_ids
              |> list.filter(fn(existing_id) { existing_id != product_id })
          }
          let next_group =
            SellingPlanGroupRecord(..group, product_ids: next_product_ids)
          let #(_, updated_store) =
            store.upsert_staged_selling_plan_group(current_store, next_group)
          #(updated_store, current_errors, list.append(current_ids, [group_id]))
        }
      }
    })
  #(next_store, errors, staged_ids)
}

@internal
pub fn update_variant_selling_plan_group_membership(
  store: Store,
  variant_id: String,
  group_ids: List(String),
  join: Bool,
) -> #(Store, List(ProductUserError), List(String)) {
  case
    validate_variant_selling_plan_group_membership(
      store,
      variant_id,
      group_ids,
      join,
    )
  {
    [_, ..] as errors -> #(store, errors, [])
    [] ->
      apply_variant_selling_plan_group_membership(
        store,
        variant_id,
        group_ids,
        join,
      )
  }
}

fn apply_variant_selling_plan_group_membership(
  store: Store,
  variant_id: String,
  group_ids: List(String),
  join: Bool,
) -> #(Store, List(ProductUserError), List(String)) {
  let #(next_store, errors, staged_ids) =
    list.fold(group_ids, #(store, [], []), fn(acc, group_id) {
      let #(current_store, current_errors, current_ids) = acc
      case
        store.get_effective_selling_plan_group_by_id(current_store, group_id)
      {
        None -> #(
          current_store,
          list.append(current_errors, [
            selling_plan_group_does_not_exist_error(),
          ]),
          current_ids,
        )
        Some(group) -> {
          let next_variant_ids = case join {
            True ->
              dedupe_preserving_order(
                list.append(group.product_variant_ids, [
                  variant_id,
                ]),
              )
            False ->
              group.product_variant_ids
              |> list.filter(fn(existing_id) { existing_id != variant_id })
          }
          let next_group =
            SellingPlanGroupRecord(
              ..group,
              product_variant_ids: next_variant_ids,
            )
          let #(_, updated_store) =
            store.upsert_staged_selling_plan_group(current_store, next_group)
          #(updated_store, current_errors, list.append(current_ids, [group_id]))
        }
      }
    })
  #(next_store, errors, staged_ids)
}

fn validate_product_selling_plan_group_membership(
  store: Store,
  product_id: String,
  group_ids: List(String),
  join: Bool,
) -> List(ProductUserError) {
  validate_selling_plan_group_membership(store, group_ids, join, fn(group) {
    list.contains(group.product_ids, product_id)
  })
}

fn validate_variant_selling_plan_group_membership(
  store: Store,
  variant_id: String,
  group_ids: List(String),
  join: Bool,
) -> List(ProductUserError) {
  validate_selling_plan_group_membership(store, group_ids, join, fn(group) {
    list.contains(group.product_variant_ids, variant_id)
  })
}

fn validate_selling_plan_group_membership(
  store: Store,
  group_ids: List(String),
  join: Bool,
  is_member: fn(SellingPlanGroupRecord) -> Bool,
) -> List(ProductUserError) {
  case group_ids {
    [] -> [selling_plan_group_ids_blank_error()]
    _ ->
      case has_duplicate_strings(group_ids) {
        True -> [duplicate_selling_plan_group_ids_error()]
        False -> {
          let groups =
            list.map(group_ids, fn(group_id) {
              store.get_effective_selling_plan_group_by_id(store, group_id)
            })
          case list.any(groups, option.is_none) {
            True -> [selling_plan_group_does_not_exist_error()]
            False -> {
              let known_groups =
                list.filter_map(groups, fn(group) { group |> option_to_result })
              case join {
                True ->
                  validate_join_selling_plan_group_memberships(
                    store,
                    known_groups,
                    is_member,
                  )
                False ->
                  validate_leave_selling_plan_group_memberships(
                    known_groups,
                    is_member,
                  )
              }
            }
          }
        }
      }
  }
}

fn validate_join_selling_plan_group_memberships(
  store: Store,
  groups_to_join: List(SellingPlanGroupRecord),
  is_member: fn(SellingPlanGroupRecord) -> Bool,
) -> List(ProductUserError) {
  let current_count =
    store.list_effective_selling_plan_groups(store)
    |> list.filter(is_member)
    |> list.length
  let new_count =
    groups_to_join
    |> list.filter(fn(group) { !is_member(group) })
    |> list.length
  case current_count + new_count > max_selling_plan_group_memberships {
    True -> [too_many_selling_plan_groups_error()]
    False -> []
  }
}

fn validate_leave_selling_plan_group_memberships(
  groups_to_leave: List(SellingPlanGroupRecord),
  is_member: fn(SellingPlanGroupRecord) -> Bool,
) -> List(ProductUserError) {
  case list.any(groups_to_leave, fn(group) { !is_member(group) }) {
    True -> [selling_plan_group_not_member_error()]
    False -> []
  }
}

fn has_duplicate_strings(values: List(String)) -> Bool {
  list.length(values) != list.length(dedupe_preserving_order(values))
}

fn captured_anchor_inputs(
  input: Dict(String, ResolvedValue),
) -> CapturedJsonValue {
  CapturedArray(
    read_object_list_field(input, "anchors")
    |> list.map(fn(anchor) { resolved_value_to_captured(ObjectVal(anchor)) }),
  )
}

@internal
pub fn selling_plan_billing_policy(
  input: Dict(String, ResolvedValue),
  existing: Option(CapturedJsonValue),
) -> CapturedJsonValue {
  case read_object_field(input, "recurring") {
    Some(recurring) ->
      CapturedObject([
        #("__typename", CapturedString("SellingPlanRecurringBillingPolicy")),
        #(
          "interval",
          optional_captured_string(read_string_field(recurring, "interval")),
        ),
        #(
          "intervalCount",
          optional_captured_int(read_int_field(recurring, "intervalCount")),
        ),
        #("anchors", captured_anchor_inputs(recurring)),
        #(
          "minCycles",
          optional_captured_int(read_int_field(recurring, "minCycles")),
        ),
        #(
          "maxCycles",
          optional_captured_int(read_int_field(recurring, "maxCycles")),
        ),
      ])
    None ->
      case read_object_field(input, "fixed") {
        Some(fixed) ->
          CapturedObject([
            #("__typename", CapturedString("SellingPlanFixedBillingPolicy")),
            #(
              "checkoutCharge",
              captured_object_or_null(read_object_field(fixed, "checkoutCharge")),
            ),
            #(
              "remainingBalanceChargeTrigger",
              optional_captured_string(read_string_field(
                fixed,
                "remainingBalanceChargeTrigger",
              )),
            ),
            #(
              "remainingBalanceChargeExactTime",
              optional_captured_string(read_string_field(
                fixed,
                "remainingBalanceChargeExactTime",
              )),
            ),
            #(
              "remainingBalanceChargeTimeAfterCheckout",
              optional_captured_string(read_string_field(
                fixed,
                "remainingBalanceChargeTimeAfterCheckout",
              )),
            ),
          ])
        None ->
          existing
          |> option.unwrap(
            CapturedObject([
              #(
                "__typename",
                CapturedString("SellingPlanRecurringBillingPolicy"),
              ),
            ]),
          )
      }
  }
}

@internal
pub fn selling_plan_inventory_policy(
  input: Dict(String, ResolvedValue),
  existing: Option(CapturedJsonValue),
) -> CapturedJsonValue {
  CapturedObject([
    #(
      "reserve",
      optional_captured_string(
        read_string_field(input, "reserve")
        |> option.or(
          option.then(existing, fn(value) {
            captured_string_field(value, "reserve")
          }),
        ),
      ),
    ),
  ])
}

@internal
pub fn selling_plan_pricing_policy(
  input: Dict(String, ResolvedValue),
) -> CapturedJsonValue {
  case read_object_field(input, "fixed") {
    Some(fixed) ->
      CapturedObject([
        #("__typename", CapturedString("SellingPlanFixedPricingPolicy")),
        #(
          "adjustmentType",
          optional_captured_string(read_string_field(fixed, "adjustmentType")),
        ),
        #(
          "adjustmentValue",
          selling_plan_policy_value(
            read_object_field(fixed, "adjustmentValue")
            |> option.unwrap(dict.new()),
          ),
        ),
      ])
    None -> {
      let recurring =
        read_object_field(input, "recurring") |> option.unwrap(dict.new())
      CapturedObject([
        #("__typename", CapturedString("SellingPlanRecurringPricingPolicy")),
        #(
          "adjustmentType",
          optional_captured_string(read_string_field(
            recurring,
            "adjustmentType",
          )),
        ),
        #(
          "adjustmentValue",
          selling_plan_policy_value(
            read_object_field(recurring, "adjustmentValue")
            |> option.unwrap(dict.new()),
          ),
        ),
        #(
          "afterCycle",
          optional_captured_int(read_int_field(recurring, "afterCycle")),
        ),
      ])
    }
  }
}

@internal
pub fn first_selling_plan_percentage(
  value: CapturedJsonValue,
) -> Option(String) {
  case captured_object_field(value, "pricingPolicies") {
    Some(CapturedArray(policies)) ->
      policies
      |> list.find_map(fn(policy) {
        case
          captured_object_field(policy, "adjustmentValue")
          |> option.then(fn(adjustment) {
            captured_number_string_field(adjustment, "percentage")
          })
        {
          Some(value) -> Ok(value <> "%")
          None -> Error(Nil)
        }
      })
      |> option.from_result
    _ -> None
  }
}

// ===== from selling_plans_l03 =====
@internal
pub fn make_selling_plan_record(
  identity: SyntheticIdentityRegistry,
  input: Dict(String, ResolvedValue),
  existing: Option(SellingPlanRecord),
) -> #(SellingPlanRecord, SyntheticIdentityRegistry) {
  let #(id, identity_after_id) = case read_string_field(input, "id"), existing {
    Some(id), _ -> #(id, identity)
    None, Some(plan) -> #(plan.id, identity)
    None, None ->
      synthetic_identity.make_proxy_synthetic_gid(identity, "SellingPlan")
  }
  let previous = case existing {
    Some(plan) -> plan.data
    None -> CapturedObject([])
  }
  let #(created_at, next_identity) = case
    captured_string_field(previous, "createdAt")
  {
    Some(value) -> #(value, identity_after_id)
    None -> synthetic_identity.make_synthetic_timestamp(identity_after_id)
  }
  let data =
    CapturedObject([
      #("__typename", CapturedString("SellingPlan")),
      #("id", CapturedString(id)),
      #(
        "name",
        CapturedString(
          read_string_field(input, "name")
          |> option.or(captured_string_field(previous, "name"))
          |> option.unwrap("Selling plan"),
        ),
      ),
      #(
        "description",
        optional_captured_string(
          read_string_field(input, "description")
          |> option.or(captured_string_field(previous, "description")),
        ),
      ),
      #(
        "options",
        CapturedArray(
          read_string_list_field(input, "options")
          |> option.or(captured_string_array_field(previous, "options"))
          |> option.unwrap([])
          |> list.map(CapturedString),
        ),
      ),
      #(
        "position",
        optional_captured_int(
          read_int_field(input, "position")
          |> option.or(captured_int_field(previous, "position")),
        ),
      ),
      #(
        "category",
        optional_captured_string(
          read_string_field(input, "category")
          |> option.or(captured_string_field(previous, "category")),
        ),
      ),
      #("createdAt", CapturedString(created_at)),
      #(
        "billingPolicy",
        selling_plan_billing_policy(
          read_object_field(input, "billingPolicy") |> option.unwrap(dict.new()),
          captured_object_field(previous, "billingPolicy"),
        ),
      ),
      #(
        "deliveryPolicy",
        selling_plan_delivery_policy(
          read_object_field(input, "deliveryPolicy")
            |> option.unwrap(dict.new()),
          captured_object_field(previous, "deliveryPolicy"),
        ),
      ),
      #(
        "inventoryPolicy",
        selling_plan_inventory_policy(
          read_object_field(input, "inventoryPolicy")
            |> option.unwrap(dict.new()),
          captured_object_field(previous, "inventoryPolicy"),
        ),
      ),
      #("pricingPolicies", case dict.has_key(input, "pricingPolicies") {
        True ->
          CapturedArray(list.map(
            read_object_list_field(input, "pricingPolicies"),
            selling_plan_pricing_policy,
          ))
        False -> CapturedArray([])
      }),
    ])
  #(SellingPlanRecord(id: id, data: data), next_identity)
}

@internal
pub fn summarize_selling_plan_group(
  plans: List(SellingPlanRecord),
) -> Option(String) {
  let percentage =
    plans
    |> list.find_map(fn(plan) {
      first_selling_plan_percentage(plan.data) |> option_to_result
    })
    |> option.from_result
    |> option.unwrap("")
  Some(
    int.to_string(list.length(plans))
    <> " delivery frequency, "
    <> percentage
    <> " discount",
  )
}

// ===== from selling_plans_l04 =====
@internal
pub fn make_selling_plan_group_record(
  identity: SyntheticIdentityRegistry,
  input: Dict(String, ResolvedValue),
  existing: Option(SellingPlanGroupRecord),
  resources: Dict(String, ResolvedValue),
) -> #(SellingPlanGroupRecord, SyntheticIdentityRegistry) {
  let current_plans = case existing {
    Some(group) -> group.selling_plans
    None -> []
  }
  let #(created_plans, identity_after_creates) =
    read_object_list_field(input, "sellingPlansToCreate")
    |> list.fold(#(current_plans, identity), fn(acc, plan_input) {
      let #(plans, current_identity) = acc
      let #(plan, next_identity) =
        make_selling_plan_record(current_identity, plan_input, None)
      #(list.append(plans, [plan]), next_identity)
    })
  let #(updated_plans, identity_after_updates) =
    read_object_list_field(input, "sellingPlansToUpdate")
    |> list.fold(#(created_plans, identity_after_creates), fn(acc, plan_input) {
      let #(plans, current_identity) = acc
      case read_string_field(plan_input, "id") {
        None -> acc
        Some(plan_id) ->
          case find_selling_plan(plans, plan_id) {
            None -> acc
            Some(existing_plan) -> {
              let #(next_plan, next_identity) =
                make_selling_plan_record(
                  current_identity,
                  plan_input,
                  Some(existing_plan),
                )
              #(replace_selling_plan(plans, next_plan), next_identity)
            }
          }
      }
    })
  let deleted_plan_ids =
    read_string_list_field(input, "sellingPlansToDelete") |> option.unwrap([])
  let plans =
    updated_plans
    |> list.filter(fn(plan) { !list.contains(deleted_plan_ids, plan.id) })
  let #(created_at, identity_after_timestamp) = case existing {
    Some(group) -> #(group.created_at, identity_after_updates)
    None -> {
      let #(timestamp, next_identity) =
        synthetic_identity.make_synthetic_timestamp(identity_after_updates)
      #(Some(timestamp), next_identity)
    }
  }
  let #(id, next_identity) = case existing {
    Some(group) -> #(group.id, identity_after_timestamp)
    None ->
      synthetic_identity.make_proxy_synthetic_gid(
        identity_after_timestamp,
        "SellingPlanGroup",
      )
  }
  let existing_product_ids = case existing {
    Some(group) -> group.product_ids
    None -> []
  }
  let existing_variant_ids = case existing {
    Some(group) -> group.product_variant_ids
    None -> []
  }
  let group =
    SellingPlanGroupRecord(
      id: id,
      app_id: read_string_field(input, "appId")
        |> option.or(existing_group_app_id(existing)),
      name: read_string_field(input, "name")
        |> option.unwrap(existing_group_name(existing)),
      merchant_code: read_string_field(input, "merchantCode")
        |> option.unwrap(existing_group_merchant_code(existing)),
      description: read_string_field(input, "description")
        |> option.or(existing_group_description(existing)),
      options: read_string_list_field(input, "options")
        |> option.unwrap(existing_group_options(existing)),
      position: read_int_field(input, "position")
        |> option.or(existing_group_position(existing)),
      summary: summarize_selling_plan_group(plans),
      created_at: created_at,
      product_ids: dedupe_preserving_order(list.append(
        existing_product_ids,
        read_string_list_field(resources, "productIds") |> option.unwrap([]),
      )),
      product_variant_ids: dedupe_preserving_order(list.append(
        existing_variant_ids,
        read_string_list_field(resources, "productVariantIds")
          |> option.unwrap([]),
      )),
      selling_plans: plans,
      cursor: case existing {
        Some(group) -> group.cursor
        None -> None
      },
    )
  #(group, next_identity)
}

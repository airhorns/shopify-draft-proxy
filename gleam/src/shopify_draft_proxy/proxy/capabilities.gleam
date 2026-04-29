//// Mirrors `src/proxy/capabilities.ts`.
////
//// Given a parsed operation and a (already-loaded) operation registry,
//// resolve the capability — domain + execution mode + a normalised
//// operation name. The TS version caches a `Map` of match-name → entry
//// at module load. We don't bother: lookup walks the implemented
//// subset of the registry, which is small. Callers that want to amortise
//// can wrap this themselves.

import gleam/list
import gleam/option.{type Option, None, Some}
import shopify_draft_proxy/graphql/parse_operation.{
  type GraphQLOperationType, type ParsedOperation, MutationOperation,
  QueryOperation,
}
import shopify_draft_proxy/proxy/operation_registry.{
  type CapabilityDomain, type CapabilityExecution, type RegistryEntry, Mutation,
  Passthrough, Query, Unknown,
}

/// Mirrors the TS `OperationCapability` interface. `operation_name` is
/// the chosen candidate name (root field or operation name) when the
/// lookup matched, or the operation's declared name / first root field
/// in the fallback.
pub type OperationCapability {
  OperationCapability(
    type_: GraphQLOperationType,
    operation_name: Option(String),
    domain: CapabilityDomain,
    execution: CapabilityExecution,
  )
}

/// Mirrors `getOperationCapability`. Resolution order:
///
///   1. First *root field* whose match-name matches an implemented
///      entry of the right type.
///   2. Otherwise, first candidate (root fields, then operation name)
///      that matches.
///   3. Otherwise, fall back to `unknown` / `passthrough` and surface
///      the operation name (or first root field) verbatim.
///
/// The TS additionally re-derives `operationNameEntry` to prefer the
/// operation's declared name over the matched root field iff both
/// resolve to the same registry entry. We replicate that.
pub fn get_operation_capability(
  operation: ParsedOperation,
  registry: List(RegistryEntry),
) -> OperationCapability {
  let implemented = operation_registry.list_implemented(registry)
  let registry_type = to_registry_type(operation.type_)
  let candidates = candidate_names(operation)

  let matched_root_field =
    list.find(operation.root_fields, fn(name) {
      has_match(implemented, registry_type, name)
    })
    |> result_to_option

  let matched_candidate = case matched_root_field {
    Some(_) -> matched_root_field
    None ->
      list.find(candidates, fn(name) {
        has_match(implemented, registry_type, name)
      })
      |> result_to_option
  }

  let matched_entry = case matched_candidate {
    Some(name) -> entry_for(implemented, registry_type, name)
    None -> None
  }

  case matched_candidate, matched_entry {
    Some(candidate), Some(entry) -> {
      let operation_name =
        pick_operation_name(
          operation,
          candidate,
          entry,
          implemented,
          registry_type,
        )
      OperationCapability(
        type_: operation.type_,
        operation_name: Some(operation_name),
        domain: entry.domain,
        execution: entry.execution,
      )
    }
    _, _ ->
      OperationCapability(
        type_: operation.type_,
        operation_name: fallback_operation_name(operation),
        domain: Unknown,
        execution: Passthrough,
      )
  }
}

/// Mirror of `getCandidateOperationNames`: root fields + operation name,
/// filtered for non-empty strings, deduplicated while preserving order.
fn candidate_names(operation: ParsedOperation) -> List(String) {
  let raw = case operation.name {
    Some(n) -> list.append(operation.root_fields, [n])
    None -> operation.root_fields
  }
  let non_empty =
    list.filter(raw, fn(value) {
      case value {
        "" -> False
        _ -> True
      }
    })
  unique_preserving_order(non_empty)
}

fn unique_preserving_order(items: List(String)) -> List(String) {
  unique_loop(items, [])
}

fn unique_loop(items: List(String), seen: List(String)) -> List(String) {
  case items {
    [] -> []
    [head, ..rest] ->
      case list.contains(seen, head) {
        True -> unique_loop(rest, seen)
        False -> [head, ..unique_loop(rest, [head, ..seen])]
      }
  }
}

fn pick_operation_name(
  operation: ParsedOperation,
  matched_candidate: String,
  matched_entry: RegistryEntry,
  implemented: List(RegistryEntry),
  registry_type: operation_registry.OperationType,
) -> String {
  case operation.name {
    Some(op_name) -> {
      case entry_for(implemented, registry_type, op_name) {
        Some(name_entry) ->
          case name_entry.name == matched_entry.name {
            True -> op_name
            False -> matched_candidate
          }
        None -> matched_candidate
      }
    }
    None -> matched_candidate
  }
}

fn fallback_operation_name(operation: ParsedOperation) -> Option(String) {
  case operation.name {
    Some(n) -> Some(n)
    None ->
      case operation.root_fields {
        [first, ..] -> Some(first)
        [] -> None
      }
  }
}

fn has_match(
  registry: List(RegistryEntry),
  type_: operation_registry.OperationType,
  name: String,
) -> Bool {
  case entry_for(registry, type_, name) {
    Some(_) -> True
    None -> False
  }
}

fn entry_for(
  registry: List(RegistryEntry),
  type_: operation_registry.OperationType,
  name: String,
) -> Option(RegistryEntry) {
  list.find(registry, fn(entry) {
    entry.type_ == type_ && list.contains(entry.match_names, name)
  })
  |> result_to_option
}

fn to_registry_type(
  op: GraphQLOperationType,
) -> operation_registry.OperationType {
  case op {
    QueryOperation -> Query
    MutationOperation -> Mutation
  }
}

fn result_to_option(r: Result(a, b)) -> Option(a) {
  case r {
    Ok(v) -> Some(v)
    Error(_) -> None
  }
}

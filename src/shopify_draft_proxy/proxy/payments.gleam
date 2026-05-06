//// Public entrypoint for payments domain handling.
////
//// Implementation is split across the payments/* submodules; this file keeps
//// the original public API surface stable for callers.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{type SourceValue}
import shopify_draft_proxy/proxy/mutation_helpers.{type MutationOutcome}
import shopify_draft_proxy/proxy/payments/mutations
import shopify_draft_proxy/proxy/payments/queries
import shopify_draft_proxy/proxy/payments/serializers
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response,
}
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type CustomerPaymentMethodInstrumentRecord,
}

pub type PaymentsError {
  ParseFailed(root_field.RootFieldError)
}

pub fn is_payments_query_root(name: String) -> Bool {
  case name {
    "paymentTermsTemplates"
    | "customerPaymentMethod"
    | "draftOrder"
    | "paymentCustomizations"
    | "paymentCustomization"
    | "cashTrackingSession"
    | "cashTrackingSessions"
    | "pointOfSaleDevice"
    | "dispute"
    | "disputeEvidence"
    | "disputes"
    | "shopPayPaymentRequestReceipt"
    | "shopPayPaymentRequestReceipts"
    | "shopifyPaymentsAccount" -> True
    _ -> False
  }
}

pub fn is_payments_mutation_root(name: String) -> Bool {
  case name {
    "paymentCustomizationCreate"
    | "paymentCustomizationUpdate"
    | "paymentCustomizationDelete"
    | "paymentCustomizationActivation"
    | "customerPaymentMethodCreditCardCreate"
    | "customerPaymentMethodCreditCardUpdate"
    | "customerPaymentMethodRemoteCreate"
    | "customerPaymentMethodPaypalBillingAgreementCreate"
    | "customerPaymentMethodPaypalBillingAgreementUpdate"
    | "customerPaymentMethodGetDuplicationData"
    | "customerPaymentMethodCreateFromDuplicationData"
    | "customerPaymentMethodGetUpdateUrl"
    | "customerPaymentMethodRevoke"
    | "paymentTermsCreate"
    | "paymentTermsUpdate"
    | "paymentTermsDelete"
    | "paymentReminderSend" -> True
    _ -> False
  }
}

pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, PaymentsError) {
  case queries.process(store, document, variables) {
    Ok(data) -> Ok(data)
    Error(err) -> Error(ParseFailed(err))
  }
}

/// Uniform query entrypoint matching the dispatcher's signature.
pub fn handle_query_request(
  proxy: DraftProxy,
  request: Request,
  parsed: parse_operation.ParsedOperation,
  primary_root_field: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Response, DraftProxy) {
  queries.handle_query_request(
    proxy,
    request,
    parsed,
    primary_root_field,
    document,
    variables,
  )
}

pub fn handle_payments_query(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, PaymentsError) {
  case queries.handle_payments_query(store, document, variables) {
    Ok(data) -> Ok(data)
    Error(err) -> Error(ParseFailed(err))
  }
}

pub fn process_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> MutationOutcome {
  mutations.process_mutation(
    store,
    identity,
    request_path,
    document,
    variables,
    upstream,
  )
}

pub fn instrument_source(
  instrument: CustomerPaymentMethodInstrumentRecord,
) -> SourceValue {
  serializers.instrument_source(instrument)
}

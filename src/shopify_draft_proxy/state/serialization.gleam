import gleam/dynamic/decode.{type Decoder}
import gleam/json.{type Json}
import shopify_draft_proxy/state/serialization/base_state
import shopify_draft_proxy/state/serialization/staged_state
import shopify_draft_proxy/state/store

pub fn serialize_base_state(state: store.BaseState) -> Json {
  base_state.serialize_base_state(state)
}

pub fn base_state_dump_field_names() -> List(String) {
  base_state.base_state_dump_field_names()
}

pub fn serialize_staged_state(state: store.StagedState) -> Json {
  staged_state.serialize_staged_state(state)
}

pub fn staged_state_dump_field_names() -> List(String) {
  staged_state.staged_state_dump_field_names()
}

pub fn strict_base_state_decoder() -> Decoder(store.BaseState) {
  base_state.strict_base_state_decoder()
}

pub fn base_state_decoder() -> Decoder(store.BaseState) {
  base_state.base_state_decoder()
}

pub fn strict_staged_state_decoder() -> Decoder(store.StagedState) {
  staged_state.strict_staged_state_decoder()
}

pub fn staged_state_decoder() -> Decoder(store.StagedState) {
  staged_state.staged_state_decoder()
}

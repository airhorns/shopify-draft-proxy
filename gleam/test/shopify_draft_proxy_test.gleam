import gleeunit
import shopify_draft_proxy

pub fn main() -> Nil {
  gleeunit.main()
}

pub fn hello_returns_phase_zero_marker_test() {
  assert shopify_draft_proxy.hello()
    == "shopify_draft_proxy gleam port: phase 0"
}

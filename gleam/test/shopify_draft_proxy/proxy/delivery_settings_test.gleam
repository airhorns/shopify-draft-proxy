import gleam/json
import shopify_draft_proxy/proxy/delivery_settings

fn handle(query: String) -> String {
  let assert Ok(data) = delivery_settings.handle_delivery_settings_query(query)
  json.to_string(data)
}

pub fn delivery_settings_minimal_selection_test() {
  let result =
    handle("{ deliverySettings { legacyModeProfiles } }")
  assert result == "{\"deliverySettings\":{\"legacyModeProfiles\":false}}"
}

pub fn delivery_settings_full_selection_test() {
  let result =
    handle(
      "{ deliverySettings { __typename legacyModeProfiles legacyModeBlocked { __typename blocked reasons } } }",
    )
  assert result
    == "{\"deliverySettings\":{\"__typename\":\"DeliverySetting\",\"legacyModeProfiles\":false,\"legacyModeBlocked\":{\"__typename\":\"DeliveryLegacyModeBlocked\",\"blocked\":false,\"reasons\":null}}}"
}

pub fn delivery_promise_settings_test() {
  let result =
    handle(
      "{ deliveryPromiseSettings { deliveryDatesEnabled processingTime } }",
    )
  assert result
    == "{\"deliveryPromiseSettings\":{\"deliveryDatesEnabled\":false,\"processingTime\":null}}"
}

pub fn unknown_root_field_is_null_test() {
  let result = handle("{ unknown }")
  assert result == "{\"unknown\":null}"
}

pub fn alias_used_as_response_key_test() {
  let result = handle("{ ds: deliverySettings { legacyModeProfiles } }")
  assert result == "{\"ds\":{\"legacyModeProfiles\":false}}"
}

pub fn process_wraps_in_data_envelope_test() {
  let assert Ok(envelope) =
    delivery_settings.process("{ deliverySettings { legacyModeProfiles } }")
  assert json.to_string(envelope)
    == "{\"data\":{\"deliverySettings\":{\"legacyModeProfiles\":false}}}"
}

pub fn parse_failure_propagates_test() {
  let assert Error(delivery_settings.ParseFailed(_)) =
    delivery_settings.handle_delivery_settings_query("{ deliverySettings(")
}

pub fn fragment_spread_within_settings_test() {
  let result =
    handle(
      "fragment Bits on DeliverySetting { legacyModeProfiles __typename } { deliverySettings { ...Bits } }",
    )
  assert result
    == "{\"deliverySettings\":{\"legacyModeProfiles\":false,\"__typename\":\"DeliverySetting\"}}"
}

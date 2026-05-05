import shopify_draft_proxy/graphql/location.{SourceLocation}
import shopify_draft_proxy/graphql/source

pub fn single_line_position_test() {
  let s = source.new("hello, world")
  assert location.get_location(s, position: 0) == SourceLocation(1, 1)
  assert location.get_location(s, position: 5) == SourceLocation(1, 6)
}

pub fn unix_newlines_position_test() {
  let s = source.new("a\nbc\ndef")
  // 'a' on line 1, 'b' on line 2, 'd' on line 3.
  assert location.get_location(s, position: 0) == SourceLocation(1, 1)
  assert location.get_location(s, position: 2) == SourceLocation(2, 1)
  assert location.get_location(s, position: 5) == SourceLocation(3, 1)
}

pub fn windows_newlines_position_test() {
  let s = source.new("a\r\nb")
  assert location.get_location(s, position: 0) == SourceLocation(1, 1)
  assert location.get_location(s, position: 3) == SourceLocation(2, 1)
}

pub fn classic_mac_newlines_position_test() {
  let s = source.new("a\rb")
  assert location.get_location(s, position: 0) == SourceLocation(1, 1)
  assert location.get_location(s, position: 2) == SourceLocation(2, 1)
}

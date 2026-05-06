import gleam/int
import gleam/option.{type Option, None, Some}
import gleam/string

pub fn at_least(request_path: String, minimum_version: String) -> Bool {
  case from_path(request_path), parse(minimum_version) {
    Some(version), Some(minimum) -> compare(version, minimum)
    _, _ -> False
  }
}

pub fn from_path(path: String) -> Option(#(Int, Int)) {
  case string.split(path, "/") {
    ["", "admin", "api", version, "graphql.json"] -> parse(version)
    _ -> None
  }
}

pub fn parse(version: String) -> Option(#(Int, Int)) {
  case string.split(version, "-") {
    [year, month] ->
      case int.parse(year), int.parse(month) {
        Ok(parsed_year), Ok(parsed_month) -> Some(#(parsed_year, parsed_month))
        _, _ -> None
      }
    _ -> None
  }
}

fn compare(version: #(Int, Int), minimum: #(Int, Int)) {
  let #(year, month) = version
  let #(minimum_year, minimum_month) = minimum
  year > minimum_year || { year == minimum_year && month >= minimum_month }
}

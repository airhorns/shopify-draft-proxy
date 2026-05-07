import gleam/dict.{type Dict}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/string

pub const api_client_id_header: String = "x-shopify-draft-proxy-api-client-id"

pub const internal_visibility_header: String = "x-shopify-draft-proxy-internal-visibility"

pub fn read_requesting_api_client_id(
  request_headers: Dict(String, String),
) -> Option(String) {
  let found =
    dict.to_list(request_headers)
    |> list.find_map(fn(header) {
      let #(name, value) = header
      case string.lowercase(name) == api_client_id_header {
        True -> Ok(string.trim(value))
        False -> Error(Nil)
      }
    })

  case found {
    Ok("") | Error(_) -> None
    Ok(value) -> Some(value)
  }
}

pub fn has_internal_visibility(request_headers: Dict(String, String)) -> Bool {
  let found =
    dict.to_list(request_headers)
    |> list.find_map(fn(header) {
      let #(name, value) = header
      case string.lowercase(name) == internal_visibility_header {
        True -> Ok(string.lowercase(string.trim(value)))
        False -> Error(Nil)
      }
    })

  case found {
    Ok("true") | Ok("1") | Ok("yes") -> True
    _ -> False
  }
}

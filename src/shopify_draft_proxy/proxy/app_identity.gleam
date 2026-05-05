import gleam/dict.{type Dict}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/string

pub const api_client_id_header: String = "x-shopify-draft-proxy-api-client-id"

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

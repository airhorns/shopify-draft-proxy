import parity/spec

pub fn proxy_request_local_setups_are_rejected_test() {
  let source =
    "
{
  \"scenarioId\": \"local-setup-rejected\",
  \"liveCaptureFiles\": [\"fixtures/conformance/example.json\"],
  \"proxyRequest\": {
    \"documentPath\": \"config/parity-requests/example.graphql\",
    \"localSetups\": [{ \"kind\": \"seedSegments\", \"count\": 1 }]
  },
  \"comparison\": {
    \"targets\": [
      {
        \"name\": \"primary\",
        \"capturePath\": \"$.data\",
        \"proxyPath\": \"$.data\"
      }
    ]
  }
}
"

  let assert Error(spec.DecodeError(_)) = spec.decode(source)
}

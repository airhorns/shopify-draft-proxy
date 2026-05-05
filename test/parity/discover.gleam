//// Walk `config/parity-specs/**` and return every `.json` spec file.
//// Mirrors the TS `glob('**/*.json', { cwd: parityRoot })` discovery
//// in `scripts/conformance-parity-lib.ts`. Erlang-only — JS doesn't
//// have ambient filesystem access in our test harness.

import gleam/list
import gleam/string
import simplifile

pub type DiscoverError {
  DiscoverError(message: String)
}

/// Return absolute paths to every `.json` file under `root`. Order is
/// implementation-defined; callers that want deterministic ordering
/// should `list.sort` the result.
pub fn discover(root: String) -> Result(List(String), DiscoverError) {
  case simplifile.get_files(root) {
    Ok(files) ->
      Ok(list.filter(files, fn(path) { string.ends_with(path, ".json") }))
    Error(err) ->
      Error(DiscoverError(
        message: "could not walk " <> root <> ": " <> describe(err),
      ))
  }
}

fn describe(err: simplifile.FileError) -> String {
  // simplifile's error type renders fine via Erlang's default
  // formatter; we just stringify here for the panic message. Detail
  // doesn't matter for assertion failures.
  case err {
    simplifile.Enoent -> "ENOENT"
    simplifile.Eacces -> "EACCES"
    _ -> "filesystem error"
  }
}

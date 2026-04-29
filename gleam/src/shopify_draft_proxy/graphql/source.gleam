//// Mirrors `graphql-js` `language/source.ts`.
////
//// A representation of GraphQL source input. `name` and `location_offset`
//// support clients that store GraphQL documents in named source files at a
//// known line/column offset. Both `line` and `column` are 1-indexed.

/// A 1-indexed (line, column) origin used to report parser errors against
/// the original document, not the substring fed into the parser.
pub type LocationOffset {
  LocationOffset(line: Int, column: Int)
}

/// Source document plus identification metadata.
pub type Source {
  Source(body: String, name: String, location_offset: LocationOffset)
}

/// Construct a `Source` with the same defaults `graphql-js` applies.
pub fn new(body: String) -> Source {
  Source(
    body: body,
    name: "GraphQL request",
    location_offset: LocationOffset(line: 1, column: 1),
  )
}

/// Construct a `Source` with explicit `name` and `location_offset`.
pub fn new_with(
  body body: String,
  name name: String,
  location_offset offset: LocationOffset,
) -> Source {
  Source(body: body, name: name, location_offset: offset)
}

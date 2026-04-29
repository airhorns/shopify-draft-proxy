import { Ok, Error } from "../../gleam.mjs";

export function format_iso(ms) {
  return new Date(ms).toISOString();
}

export function parse_iso(s) {
  const ms = Date.parse(s);
  if (Number.isNaN(ms)) {
    return new Error(undefined);
  }
  return new Ok(ms);
}

export function now_iso() {
  return new Date().toISOString();
}

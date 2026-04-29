-module(iso_timestamp_ffi).
-export([format_iso/1, parse_iso/1, now_iso/0]).

format_iso(Ms) ->
    list_to_binary(calendar:system_time_to_rfc3339(Ms, [{unit, millisecond}, {offset, "Z"}])).

parse_iso(IsoBin) ->
    try calendar:rfc3339_to_system_time(binary_to_list(IsoBin), [{unit, millisecond}]) of
        Ms when is_integer(Ms) -> {ok, Ms}
    catch
        _:_ -> {error, nil}
    end.

now_iso() ->
    format_iso(erlang:system_time(millisecond)).

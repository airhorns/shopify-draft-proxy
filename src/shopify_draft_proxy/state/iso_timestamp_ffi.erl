-module(iso_timestamp_ffi).
-export([format_iso/1, parse_iso/1, now_iso/0]).

format_iso(Ms) ->
    list_to_binary(calendar:system_time_to_rfc3339(Ms, [{unit, millisecond}, {offset, "Z"}])).

parse_iso(IsoBin) ->
    Iso = binary_to_list(IsoBin),
    parse_iso_candidates(parse_candidates(Iso)).

parse_candidates(Iso) ->
    case is_date_only(Iso) of
        true -> [Iso ++ "T00:00:00Z", Iso];
        false -> [Iso]
    end.

is_date_only([Y1, Y2, Y3, Y4, $-, M1, M2, $-, D1, D2]) ->
    lists:all(
        fun is_digit/1,
        [Y1, Y2, Y3, Y4, M1, M2, D1, D2]
    );
is_date_only(_) ->
    false.

is_digit(C) ->
    C >= $0 andalso C =< $9.

parse_iso_candidates([]) ->
    {error, nil};
parse_iso_candidates([Iso | Rest]) ->
    try calendar:rfc3339_to_system_time(Iso, [{unit, millisecond}]) of
        Ms when is_integer(Ms) -> {ok, Ms}
    catch
        _:_ -> parse_iso_candidates(Rest)
    end.

now_iso() ->
    format_iso(erlang:system_time(millisecond)).

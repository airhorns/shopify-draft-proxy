-module(crypto_ffi).
-export([sha256_hex/1, md5_hex/1]).

sha256_hex(Input) ->
    Digest = crypto:hash(sha256, Input),
    binary:encode_hex(Digest, lowercase).

md5_hex(Input) ->
    Digest = crypto:hash(md5, Input),
    binary:encode_hex(Digest, lowercase).

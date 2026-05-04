shipment_dir = Path.expand("../../build/erlang-shipment", __DIR__)

unless File.dir?(shipment_dir) do
  raise """
  Gleam erlang shipment not found at #{shipment_dir}.

  Build it from the gleam/ project root with:

      gleam export erlang-shipment

  This smoke test loads the precompiled BEAM artefacts produced by that
  command rather than recompiling Gleam from mix; real consumers will pull
  the package from Hex once the port is published.
  """
end

shipment_dir
|> File.ls!()
|> Enum.each(fn pkg ->
  ebin = Path.join([shipment_dir, pkg, "ebin"])
  if File.dir?(ebin), do: Code.prepend_path(ebin)
end)

# `:live` tests hit a real Shopify test store and need conformance auth
# env vars (see test/live_hybrid_e2e_test.exs). They are excluded by
# default so `mix test` from this directory stays a pure offline smoke
# of the Elixir wrapper. Run them via `pnpm e2e:elixir-product-create-
# commit-smoke` from the repo root, which refreshes the conformance
# token and forwards the env to `mix test --only live`.
ExUnit.start(exclude: [:live])

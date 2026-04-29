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

ExUnit.start()

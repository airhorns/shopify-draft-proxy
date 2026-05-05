defmodule ShopifyDraftProxy.ElixirSmoke.MixProject do
  @moduledoc """
  Phase 0 BEAM interop smoke for the Gleam port of `shopify-draft-proxy`.

  This mix project exists only to assert that the Gleam package's compiled
  BEAM artefacts can be loaded and called from a stock Elixir mix project,
  matching what real Elixir consumers will do once the port has domain
  coverage. Real domain code lands in Phase 2; see ../README.md.
  """

  use Mix.Project

  def project do
    [
      app: :shopify_draft_proxy_elixir_smoke,
      version: "0.1.0",
      elixir: "~> 1.18",
      start_permanent: false,
      deps: []
    ]
  end

  def application do
    # `:inets` and `:ssl` back the `:httpc` calls that the live e2e
    # smoke (`test/live_hybrid_e2e_test.exs`) uses to talk to Shopify
    # directly for cleanup verification. The unit interop tests don't
    # need them, but they are cheap to start and keep both flows in one
    # mix project.
    [extra_applications: [:logger, :inets, :ssl]]
  end
end

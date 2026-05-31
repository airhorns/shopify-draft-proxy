# frozen_string_literal: true

Gem::Specification.new do |spec|
  spec.name = "shopify-draft-proxy"
  spec.version = "0.1.0"
  spec.summary = "Ruby bindings for the Shopify Draft Proxy Rust runtime"
  spec.description = "Native Ruby extension bindings for the Rust Shopify Admin GraphQL draft proxy runtime."
  spec.authors = ["Harphony"]
  spec.license = "Apache-2.0"
  spec.required_ruby_version = ">= 3.1"

  spec.files = Dir["lib/**/*.rb"] + Dir["native/**/*.{rs,toml}"] + ["README.md", "Rakefile"]
  spec.require_paths = ["lib"]

  spec.add_development_dependency "minitest", "~> 5.0"
  spec.add_development_dependency "rake", "~> 13.0"
end

defmodule RockboxFfi.MixProject do
  use Mix.Project

  @version "0.8.0"
  @source_url "https://github.com/tsirysndr/rockboxd"
  @source_ref "master"

  def project do
    [
      app: :rockbox_ex_ffi,
      version: @version,
      elixir: "~> 1.15",
      deps: deps(),
      description: "Elixir bindings for the Rockbox DSP / metadata / playback engine",
      package: package(),
      name: "rockbox_ex_ffi",
      source_url: @source_url,
      docs: docs()
    ]
  end

  def application, do: [extra_applications: []]

  # The native NIF lives in the shared `rockbox_ffi_nif` package
  # (bindings/erlang): it owns the C shim + Erlang loader and downloads the
  # matching prebuilt `.so` on first load. Local monorepo development uses the
  # sibling path dependency (run `make` in bindings/erlang to build the .so);
  # releases depend on the published Hex version — `publish-elixir.sh` sets
  # ROCKBOX_NIF_HEX=<version> so the tarball carries a proper version requirement
  # (Hex rejects path deps).
  defp deps do
    [
      {:rockbox_ffi_nif, nif_dep()},
      {:ex_doc, "~> 0.34", only: :dev, runtime: false}
    ]
  end

  defp nif_dep do
    case System.get_env("ROCKBOX_NIF_HEX") do
      nil -> [path: "../erlang"]
      # Floor at the built-against version, ceiling at the next major — so a
      # minor NIF bump (new functions, no removals) is picked up automatically.
      # NOTE: a 3-part `~> #{ver}` would cap at < next-MINOR, which is too tight;
      # this mirrors the Gleam binding's `>= ver and < nextmajor.0.0`.
      ver -> ">= #{ver} and < #{nif_next_major(ver)}.0.0"
    end
  end

  defp nif_next_major(ver) do
    ver |> String.split(".") |> hd() |> String.to_integer() |> Kernel.+(1)
  end

  defp docs do
    [
      main: "readme",
      extras: ["README.md"],
      source_url: @source_url,
      source_ref: @source_ref,
      # This package lives in a monorepo subdirectory, so ExDoc's file paths
      # (relative to bindings/elixir/) must be prefixed with that path for the
      # "source" links — on modules, functions, and the README — to resolve.
      source_url_pattern: "#{@source_url}/blob/#{@source_ref}/bindings/elixir/%{path}#L%{line}",
      groups_for_modules: [
        Core: [Rockbox],
        Metadata: [Rockbox.Metadata],
        DSP: [Rockbox.Dsp],
        Playback: [Rockbox.Player, Rockbox.InsertPosition],
        Codecs: [Rockbox.Decoder]
      ]
    ]
  end

  defp package do
    [
      licenses: ["GPL-2.0-or-later"],
      # Only the Elixir ergonomic wrappers ship here; the native NIF is pulled in
      # via the `rockbox_ffi_nif` dependency.
      files: ~w(lib mix.exs README.md),
      links: %{"GitHub" => "https://github.com/tsirysndr/rockboxd"}
    ]
  end
end

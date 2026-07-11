defmodule RockboxFfi.MixProject do
  use Mix.Project

  def project do
    [
      app: :rockbox_ex_ffi,
      version: "0.1.2",
      elixir: "~> 1.15",
      # elixir_make runs the Makefile (builds priv/rockbox_ffi_nif.so) before
      # compiling Elixir; the shared NIF loader in src/ is compiled by Mix.
      compilers: [:elixir_make | Mix.compilers()],
      make_targets: ["all"],
      make_clean: ["clean"],
      erlc_paths: ["src"],
      deps: deps(),
      description: "Elixir bindings for the Rockbox DSP / metadata / playback engine",
      package: package(),
      name: "rockbox_ex_ffi",
      source_url: "https://github.com/tsirysndr/rockboxd",
      docs: docs()
    ]
  end

  def application, do: [extra_applications: [:crypto]]

  defp deps do
    [
      {:elixir_make, "~> 0.8", runtime: false},
      {:ex_doc, "~> 0.34", only: :dev, runtime: false}
    ]
  end

  @source_url "https://github.com/tsirysndr/rockboxd"
  @source_ref "master"

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
        Playback: [Rockbox.Player, Rockbox.InsertPosition]
      ]
    ]
  end

  defp package do
    [
      licenses: ["GPL-2.0-or-later"],
      files: ~w(lib src c_src Makefile mix.exs README.md),
      links: %{"GitHub" => "https://github.com/tsirysndr/rockboxd"}
    ]
  end
end

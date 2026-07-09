defmodule RockboxFfi.MixProject do
  use Mix.Project

  def project do
    [
      app: :rockbox_ffi,
      version: "0.1.0",
      elixir: "~> 1.15",
      # elixir_make runs the Makefile (builds priv/rockbox_ffi_nif.so) before
      # compiling Elixir; the shared NIF loader in src/ is compiled by Mix.
      compilers: [:elixir_make | Mix.compilers()],
      make_targets: ["all"],
      make_clean: ["clean"],
      erlc_paths: ["src"],
      deps: deps(),
      description: "Elixir bindings for the Rockbox DSP / metadata / playback engine",
      package: package()
    ]
  end

  def application, do: [extra_applications: [:crypto]]

  defp deps do
    [{:elixir_make, "~> 0.8", runtime: false}]
  end

  defp package do
    [
      licenses: ["GPL-2.0-or-later"],
      files: ~w(lib src c_src Makefile mix.exs README.md),
      links: %{"GitHub" => "https://github.com/tsirysndr/rockboxd"}
    ]
  end
end

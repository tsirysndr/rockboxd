{
  description = "Rockbox Daemon — rockboxd daemon (gRPC/GraphQL/HTTP/MPD audio server)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs    = import nixpkgs { inherit system overlays; };
        lib     = pkgs.lib;

        # ── Rust 1.95 stable ────────────────────────────────────────────────
        rustToolchain = pkgs.rust-bin.stable."1.95.0".default.override {
          extensions = [ "rust-src" "rustfmt" "clippy" ];
        };

        # ── Zig 0.16.0 (fetched from upstream) ──────────────────────────────
        zigVersion = "0.16.0";

        zigBySystem = {
          "x86_64-linux"   = { plat = "x86_64-linux";  sha256 = "70e49664a74374b48b51e6f3fdfbf437f6395d42509050588bd49abe52ba3d00"; };
          "aarch64-linux"  = { plat = "aarch64-linux"; sha256 = "ea4b09bfb22ec6f6c6ceac57ab63efb6b46e17ab08d21f69f3a48b38e1534f17"; };
          "x86_64-darwin"  = { plat = "x86_64-macos";  sha256 = "0387557ed1877bc6a2e1802c8391953baddba76081876301c522f52977b52ba7"; };
          "aarch64-darwin" = { plat = "aarch64-macos"; sha256 = "b23d70deaa879b5c2d486ed3316f7eaa53e84acf6fc9cc747de152450d401489"; };
        };

        zigInfo = zigBySystem.${system};

        zig = pkgs.stdenv.mkDerivation {
          pname   = "zig";
          version = zigVersion;
          src = pkgs.fetchurl {
            url    = "https://ziglang.org/download/${zigVersion}/zig-${zigInfo.plat}-${zigVersion}.tar.xz";
            sha256 = zigInfo.sha256;
          };
          dontConfigure = true;
          dontBuild     = true;
          installPhase  = ''
            mkdir -p $out/bin $out/lib
            cp -r lib $out/lib/zig
            cp zig  $out/bin/zig
          '';
          meta = with lib; {
            description = "Zig ${zigVersion} compiler and toolchain";
            homepage    = "https://ziglang.org";
            license     = licenses.mit;
            platforms   = builtins.attrNames zigBySystem;
          };
        };

        # ── Platform-specific packages ───────────────────────────────────────

        # Linux: ALSA (cpal), D-Bus, libunwind — linked into rockboxd.
        linuxPkgs = lib.optionals pkgs.stdenv.isLinux (with pkgs; [
          alsa-lib alsa-lib.dev
          dbus     dbus.dev
          libunwind libunwind.dev
        ]);

        # macOS: llvm-objcopy for codec --redefine-sym inside build-headless.sh.
        # Use .llvm (not .bintools — bintools wraps Apple ld and needs the
        # removed apple_sdk_11_0 stub).  System frameworks are ambient via CLT.
        darwinPkgs = lib.optionals pkgs.stdenv.isDarwin (with pkgs; [
          llvmPackages_18.llvm
        ]);

        # ── PKG_CONFIG_PATH / LD_LIBRARY_PATH helpers (devShell only) ────────

        pkgConfigDirs = lib.concatStringsSep ":" (
          [
            "${pkgs.SDL2.dev}/lib/pkgconfig"
            "${pkgs.freetype.dev}/lib/pkgconfig"
            "${pkgs.zlib.dev}/lib/pkgconfig"
            "${pkgs.libusb1.dev}/lib/pkgconfig"
          ]
          ++ lib.optionals pkgs.stdenv.isLinux [
            "${pkgs.alsa-lib.dev}/lib/pkgconfig"
            "${pkgs.dbus.dev}/lib/pkgconfig"
            "${pkgs.libunwind.dev}/lib/pkgconfig"
          ]
        );

        ldLibDirs = lib.concatStringsSep ":" (
          [ "${pkgs.SDL2}/lib" "${pkgs.freetype}/lib" "${pkgs.zlib}/lib" ]
          ++ lib.optionals pkgs.stdenv.isLinux [
            "${pkgs.alsa-lib}/lib"
            "${pkgs.dbus}/lib"
            "${pkgs.libunwind}/lib"
          ]
        );

        # ── WebUI static assets ──────────────────────────────────────────────
        # Compiled from webui/rockbox/ and embedded by rockbox-server.
        #
        # To obtain / update npmDepsHash:
        #   nix build .#webui-assets 2>&1 | grep 'got:'
        # then paste the printed hash below.
        webuiAssets = pkgs.buildNpmPackage {
          pname   = "rockbox-webui";
          version = "0.1.0";
          src     = ./webui/rockbox;

          npmDepsHash = "sha256-zJxCDqddiRmZ7EFGtEBXPkr+A5Yq1Wtk2YQnFf+NMWQ=";

          # --legacy-peer-deps: the lockfile pins graphql@15.7.2 while
          #   graphql-ws wants graphql@^15.10.1; bun/deno tolerate this,
          #   npm's strict peer resolution does not.
          # --ignore-scripts: the `build` script produces the web dist via
          #   vite; electron is only a devDependency for the desktop variant
          #   and its postinstall tries to download a binary over the network
          #   (blocked in the sandbox). None of the web-build deps need
          #   install scripts.
          npmFlags = [ "--legacy-peer-deps" "--ignore-scripts" ];

          # Only the compiled dist/ is needed; skip npm's default pack step.
          installPhase = ''
            runHook preInstall
            mkdir -p $out
            cp -r dist/. $out/
            runHook postInstall
          '';
        };

        # ── S3 admin WebUI static assets ─────────────────────────────────────
        # Compiled from crates/s3/s3webui/ and embedded by rockbox-s3
        # (rust-embed folder $CARGO_MANIFEST_DIR/s3webui/dist in
        # crates/s3/src/admin.rs). rockbox-s3 is a dependency of rockbox-server,
        # so these assets must exist before the Rust build.
        #
        # To obtain / update npmDepsHash:
        #   nix build .#s3webui-assets 2>&1 | grep 'got:'
        # then paste the printed hash below.
        s3webuiAssets = pkgs.buildNpmPackage {
          pname   = "rockbox-s3-webui";
          version = "0.0.0";
          src = lib.fileset.toSource {
            root = ./crates/s3/s3webui;
            # Full source tree needed for `vite build`. Only the checked-in
            # inputs are listed; generated/vendored dirs (node_modules, dist,
            # .tanstack) are gitignored and deliberately excluded.
            fileset = lib.fileset.unions [
              ./crates/s3/s3webui/package.json
              ./crates/s3/s3webui/package-lock.json
              ./crates/s3/s3webui/index.html
              ./crates/s3/s3webui/vite.config.ts
              ./crates/s3/s3webui/tsconfig.json
              ./crates/s3/s3webui/tsconfig.app.json
              ./crates/s3/s3webui/tsconfig.node.json
              ./crates/s3/s3webui/tsr.config.json
              ./crates/s3/s3webui/.oxlintrc.json
              ./crates/s3/s3webui/src
              ./crates/s3/s3webui/public
            ];
          };

          npmDepsHash = "sha256-FKkuvP7JfewxGCe8WUjZlDgFSL48ll5/3WE4lGMdE/w=";

          # Only the compiled dist/ is needed; skip npm's default pack step.
          installPhase = ''
            runHook preInstall
            mkdir -p $out
            cp -r dist/. $out/
            runHook postInstall
          '';
        };

        # ── Vendored Cargo sources ────────────────────────────────────────────
        # fetchCargoVendor runs `cargo vendor` once and caches the result.
        # Single hash covers the entire workspace including all transitive deps.
        #
        # To obtain / update the hash:
        #   nix build .#rockboxd 2>&1 | grep 'got:'
        # then paste the printed hash below.
        cargoDeps = pkgs.rustPlatform.fetchCargoVendor {
          src  = ./.;
          hash = lib.fakeHash;
        };

        # ── rockboxd derivation ───────────────────────────────────────────────
        # Build order mirrors scripts/build-headless.sh:
        #   0. webui assets (done above, injected via preBuild)
        #   1. configure + make lib  (headless C firmware)
        #   2. cargo build           (Rust crates, offline via cargoDeps)
        #   3. zig build             (final link)
        rockboxd = pkgs.stdenv.mkDerivation {
          pname   = "rockboxd";
          version = "0.1.0";
          src     = ./.;

          nativeBuildInputs = with pkgs; [
            zig
            rustToolchain
            gnumake
            gcc
            pkg-config
            cmake
            perl       # tools/configure is a Perl script
            python3
            protobuf   # protoc for Rust codegen
            # Wires up offline Cargo registry from cargoDeps.
            rustPlatform.cargoSetupHook
          ] ++ darwinPkgs;

          # Libraries linked into the final binary.
          buildInputs = with pkgs; [
            freetype freetype.dev
            zlib zlib.dev
            libusb1 libusb1.dev
          ] ++ linuxPkgs;

          inherit cargoDeps;

          preBuild = ''
            # Inject compiled webui where rockbox-server's build.rs expects it.
            mkdir -p webui/rockbox/dist
            cp -r ${webuiAssets}/. webui/rockbox/dist/

            # Inject compiled S3 admin webui where rockbox-s3's rust-embed
            # (crates/s3/src/admin.rs) expects it at compile time.
            mkdir -p crates/s3/s3webui/dist
            cp -r ${s3webuiAssets}/. crates/s3/s3webui/dist/
          '';

          buildPhase = ''
            runHook preBuild
            export ZIG_GLOBAL_CACHE_DIR="$TMPDIR/zig-cache"
            export ZIG_LOCAL_CACHE_DIR="$TMPDIR/zig-cache"
            bash scripts/build-headless.sh
            runHook postBuild
          '';

          installPhase = ''
            runHook preInstall
            mkdir -p $out/bin
            cp zig/zig-out/bin/rockboxd $out/bin/rockboxd
            runHook postInstall
          '';

          meta = with lib; {
            description = "Rockbox daemon — gRPC / GraphQL / HTTP / MPD audio server";
            homepage    = "https://github.com/tsirysndr/rockboxd";
            license     = licenses.lgpl21;
            platforms   = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
            mainProgram = "rockboxd";
          };
        };

      in
      {
        # ── packages ────────────────────────────────────────────────────────────
        # nix build / nix shell / nix profile install all use packages.default.
        packages = {
          default        = rockboxd;       # ← what gets installed
          inherit rockboxd;
          webui-assets   = webuiAssets;    # exposed separately to ease hash updates
          s3webui-assets = s3webuiAssets;  # exposed separately to ease hash updates
        };

        # ── nix develop ─────────────────────────────────────────────────────────
        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            zig
            rustToolchain
            gnumake
            gcc
            pkg-config
            cmake
            perl
            python3
            SDL2 SDL2.dev
            freetype freetype.dev
            zlib zlib.dev
            libusb1 libusb1.dev
            protobuf
            buf
            grpcurl
            evans
            bun
            deno
          ] ++ linuxPkgs ++ darwinPkgs;

          shellHook = ''
            echo "Rockbox Daemon development environment"
            echo "  Zig:  $(zig version)"
            echo "  Rust: $(rustc --version)"
            echo ""
            echo "Headless build (cpal, no SDL):"
            echo "  cd webui/rockbox && deno install --allow-scripts && deno task build && cd ../.."
            echo "  bash scripts/build-headless.sh"
            echo ""
            echo "SDL build:"
            echo "  cd webui/rockbox && deno install --allow-scripts && deno task build && cd ../.."
            echo "  cd build-lib && make lib -j\$(nproc)"
            echo "  cargo build --release -p rockbox-cli -p rockbox-server"
            echo "  cd zig && zig build"

            export PKG_CONFIG_PATH="${pkgConfigDirs}"
            export ZIG_GLOBAL_CACHE_DIR="$PWD/.zig-cache"
            export ZIG_LOCAL_CACHE_DIR="$PWD/.zig-cache"
          '' + lib.optionalString pkgs.stdenv.isLinux ''
            export LD_LIBRARY_PATH="${ldLibDirs}"
          '' + lib.optionalString pkgs.stdenv.isDarwin ''
            export DYLD_LIBRARY_PATH="${pkgs.SDL2}/lib:${pkgs.freetype}/lib:${pkgs.zlib}/lib"
            export ROCKBOX_LLVM_OBJCOPY="$(command -v llvm-objcopy 2>/dev/null)"
          '';
        };

        # ── nix run .#<name> convenience scripts ─────────────────────────────
        apps = {
          # Full headless build: webui → firmware → Rust → Zig
          build-headless = {
            type    = "app";
            program = "${pkgs.writeShellScript "build-headless" ''
              set -euo pipefail
              echo "==> Step 0: WebUI"
              (cd webui/rockbox && deno install --allow-scripts && deno task build)
              exec bash scripts/build-headless.sh "$@"
            ''}";
          };

          # SDL build: webui → make lib → cargo → zig build
          build-sdl = {
            type    = "app";
            program = "${pkgs.writeShellScript "build-sdl" ''
              set -euo pipefail
              NCPU=$(nproc 2>/dev/null || sysctl -n hw.logicalcpu 2>/dev/null || echo 4)
              echo "==> Step 0: WebUI"
              (cd webui/rockbox && deno install --allow-scripts && deno task build)
              echo "==> Step 1: firmware (build-lib)"
              (cd build-lib && make lib -j"$NCPU")
              echo "==> Step 2: Rust crates"
              cargo build --release -p rockbox-cli -p rockbox-server
              echo "==> Step 3: Zig link"
              (cd zig && zig build)
              echo "Done: zig/zig-out/bin/rockboxd"
            ''}";
          };

          default = {
            type    = "app";
            program = "${pkgs.writeShellScript "rockboxd-info" ''
              echo "Usage:"
              echo "  nix build .                # build rockboxd"
              echo "  nix shell .                # shell with rockboxd in PATH"
              echo "  nix profile install .      # install rockboxd permanently"
              echo "  nix run .#build-headless   # build in the working tree (headless)"
              echo "  nix run .#build-sdl        # build in the working tree (SDL)"
              echo "  nix develop                # dev shell with all toolchains"
            ''}";
          };
        };
      }
    );
}

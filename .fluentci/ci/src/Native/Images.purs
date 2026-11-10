module Native.Images where

-- Preserve the immutable image digests of the original CI recipes.
resolve :: String -> String
resolve image = case image of
  "docker.io/library/node:24-bookworm-slim" -> "docker.io/library/node:24-bookworm-slim@sha256:55fb5f5a527ea2c5c4a95f963e48ef90aa38c7b346054fdc8ee9957a2b55d205"
  "node:24-bookworm-slim" -> "docker.io/library/node:24-bookworm-slim@sha256:55fb5f5a527ea2c5c4a95f963e48ef90aa38c7b346054fdc8ee9957a2b55d205"
  "docker.io/library/rust:bookworm" -> "docker.io/library/rust:bookworm@sha256:48b7f435b8fc6378ce43b44f15e3a47fd8919b2825f835c39bd4bd3da50af5b8"
  "rust:bookworm" -> "docker.io/library/rust:bookworm@sha256:48b7f435b8fc6378ce43b44f15e3a47fd8919b2825f835c39bd4bd3da50af5b8"
  "docker.io/nixos/nix:2.28.5" -> "docker.io/nixos/nix:2.28.5@sha256:e2bff159b88c242722022a273d3832a4392f01dba35b796204d852b89042526e"
  "nixos/nix:2.28.5" -> "docker.io/nixos/nix:2.28.5@sha256:e2bff159b88c242722022a273d3832a4392f01dba35b796204d852b89042526e"
  "docker.io/curlimages/curl:8.12.1" -> "docker.io/curlimages/curl:8.12.1@sha256:94e9e444bcba979c2ea12e27ae39bee4cd10bc7041a472c4727a558e213744e6"
  "curlimages/curl:8.12.1" -> "docker.io/curlimages/curl:8.12.1@sha256:94e9e444bcba979c2ea12e27ae39bee4cd10bc7041a472c4727a558e213744e6"
  "alpine:3.23" -> "docker.io/library/alpine:3.23@sha256:469c9ea410b9215eaee7976ba6c0639039b444be8e5d638f7abbada189971eb2"
  _ -> image

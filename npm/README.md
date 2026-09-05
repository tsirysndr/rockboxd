# @rockboxd/cli

Installs the [Rockbox Daemon](https://github.com/tsirysndr/rockboxd) command-line
tools — the `rockbox` CLI and the `rockboxd` daemon — by downloading the latest
prebuilt binaries from GitHub releases.

## Install

```sh
npm install -g @rockboxd/cli
```

Then:

```sh
rockboxd            # start the daemon
rockbox --help      # control it from the CLI
```

## How it works

The `postinstall` script resolves the latest release of
`tsirysndr/rockboxd`, downloads the tarball matching your platform, verifies
its sha256 checksum against the published `.sha256` asset, and extracts the
`rockbox` and `rockboxd` binaries into the package's `native/` directory. The
`rockbox` / `rockboxd` commands are thin Node shims that exec those binaries.

## Supported platforms

| Platform | Architectures   |
| -------- | --------------- |
| macOS    | arm64, x86_64   |
| Linux    | x86_64, aarch64 |
| FreeBSD  | x86_64          |

## Environment variables

| Variable                    | Effect                                                           |
| --------------------------- | ---------------------------------------------------------------- |
| `ROCKBOX_VERSION`           | Pin a specific release tag (e.g. `2026.07.28`) instead of latest |
| `GITHUB_TOKEN` / `GH_TOKEN` | Authenticate GitHub API requests (avoids rate limits in CI)      |

## License

GPL-2.0, same as Rockbox.

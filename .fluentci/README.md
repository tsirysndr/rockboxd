# FluentCI workflow entrypoints

GitHub installs `fluentci-io/setup-fluentci@next`, then calls published setup
modules for toolchains. Local PureScript components run artifact discovery,
release uploads, Gemfury credential handling, Cachix operations, and FlakeHub
publishing with literal argv. No new shell scripts implement pipeline logic.

The existing typed `.dagger/` container graphs remain the build backend for jobs
that already used them, preserving cross-compilation sysroots, glibc baselines,
Android SDK mounts, and packaging. The `dagger` entrypoint expands environment
references once in PureScript; it never evaluates a shell command. Existing
native and BSD VM build commands remain in their original environments.

Workflows check out tooling at `github.workflow_sha` into `.ci-tooling/`, so an
older release source does not have to contain these modules. That tooling tree
is excluded from container source inputs. GitHub setup results pass through the
small `github-env.mjs` bridge to persist PATH and toolchain environment between
CLI processes. The toolchain composite generates ignored `.sdk/` files from the installed CLI
in a temporary workspace, then the component builds from committed npm and
Spago lockfiles. Local source builds need the same SDK-generation step.

```sh
fluentci run ci --help
fluentci run ci dagger --directory . --arguments '["version"]'
fluentci run ci github-assets --directory dist --tag v1.0.0 --create=false
```

Publishing commands are real writes. They run only in the corresponding release
workflows. GitHub reads `GH_TOKEN`; Gemfury reads `FURY_TOKEN` and `FURY_ACCOUNT`;
Cachix pushes read `CACHIX_AUTH_TOKEN`. Missing Gemfury/Cachix tokens skip pushes.
FlakeHub keeps the workflow OIDC permission and selected visibility/output flags.
Disabled workflows remain untouched. Legacy `.dagger/check.py` workflow migration
assertions are retired; graph tests and module compilation remain available.

The pinned Dagger `1.0.0-beta.21` CLI uses the local PureScript `setup-dagger`
function: this beta is distributed by `dl.dagger.io`, while the shared setup
module looks for GitHub Releases. The installer verifies the original four
platform SHA-256 pins before extraction and persists PATH for later steps.

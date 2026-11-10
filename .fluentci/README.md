# FluentCI workflows

GitHub installs `fluentci-io/setup-fluentci@next` and published `setup-*` modules.
The `.fluentci/ci/src` PureScript component owns CI control flow, packaging,
artifact uploads, Cachix and FlakeHub operations. Commands use literal argument
vectors. FluentCI does not install or execute Dagger; `.dagger` is retained as
legacy source.

Tooling is checked out at `github.workflow_sha` in `.ci-tooling`, independently
of the selected release source. The toolchain action pins Node 24.13.1 and
hydrates the ignored SDK before compilation. `github-env.mjs` persists setup
module environment results between processes.

The ported container recipes live in `CI/`, using the PureScript `Native/`
executor to call Docker directly. It retains container images, compiler targets,
source exclusions, named caches, and artifact layouts. Each job owns and cleans
up its containers, temporary snapshots, and staging directories. Expected probe
failures are explicit; build failures propagate synchronously to the component.
Nix jobs run directly on GitHub runners after `setup-determinate-nix`.

`fluentci-check.yml` compiles the component, exercises a small real container on
both x86_64 and native ARM64 runners, runs regression tests, and lints workflows.
No project or Nix builds are performed by the container smoke test.

Release commands perform real writes. GitHub uploads use `GH_TOKEN`; Gemfury
uses `FURY_TOKEN` and `FURY_ACCOUNT`; Cachix pushes use `CACHIX_AUTH_TOKEN`.
Missing publishing credentials skip the corresponding optional upload.

Docker publishing builds each platform on its native GitHub runner, exports an
OCI archive with Buildx, then publishes architecture images and multi-platform
manifests to both registries. Registry loops and tag validation are PureScript.
The source filter explicitly preserves `firmware/target`.

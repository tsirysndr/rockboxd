"""Hatchling build hook: tag the wheel platform-specific when a lib is bundled.

The binding loads ``librockbox_ffi`` via cffi/dlopen, so a wheel that bundles
the native lib is Python-ABI-independent (``py3-none``) but *platform*-specific.
Without this hook hatchling sees only Python sources plus a data file and emits
a ``py3-none-any`` wheel — which installs on every platform yet carries the
wrong (or no) binary. This forces ``py3-none-<platform>`` whenever
``src/rockbox_ffi/_lib/`` contains a shared library.

When ``_lib/`` is empty (e.g. building the sdist, or a wheel without a staged
binary) the wheel stays pure ``py3-none-any`` and must NOT be published — it is
only useful with ``ROCKBOX_FFI_LIB`` set or from a repo checkout.

CI can set ``ROCKBOX_WHEEL_PLATFORM`` to cross-tag (e.g. build the
``macosx_10_12_x86_64`` wheel on an arm64/Linux host); otherwise the host
platform tag is used.
"""

from __future__ import annotations

import os
import sysconfig

from hatchling.builders.hooks.plugin.interface import BuildHookInterface

_LIB_SUFFIXES = (".so", ".dylib", ".dll")


class CustomBuildHook(BuildHookInterface):
    def initialize(self, version: str, build_data: dict) -> None:
        lib_dir = os.path.join(self.root, "src", "rockbox_ffi", "_lib")
        has_lib = os.path.isdir(lib_dir) and any(
            f.endswith(_LIB_SUFFIXES) for f in os.listdir(lib_dir)
        )
        if not has_lib:
            return  # no binary -> leave it pure (do not publish this wheel)

        plat = os.environ.get("ROCKBOX_WHEEL_PLATFORM") or _host_platform_tag()
        build_data["pure_python"] = False
        build_data["tag"] = f"py3-none-{plat}"


def _host_platform_tag() -> str:
    # e.g. "macosx-11.0-arm64" -> "macosx_11_0_arm64", "linux-x86_64" -> "linux_x86_64"
    return sysconfig.get_platform().replace("-", "_").replace(".", "_")

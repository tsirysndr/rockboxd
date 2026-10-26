#!/usr/bin/env bash
# Preserve the four-archive release format used by the old standalone jobs.
set -euo pipefail
bash /ci/scripts/sdl.sh
source /ci/scripts/common.sh
suffix="$ARCH-linux"
tar czf "rockbox_${TAG}_${suffix}.tar.gz" -C /usr/local/bin rockbox
tar czf "rockboxd_${TAG}_${suffix}.tar.gz" -C /usr/local/bin rockboxd
tar czf "rockbox-codecs-${TAG}-${suffix}.tar.gz" -C /usr/local/lib/rockbox .
tar czf "rockbox-assets-${TAG}-${suffix}.tar.gz" -C /usr/local/share/rockbox .
for archive in "rockbox_${TAG}_${suffix}.tar.gz" "rockboxd_${TAG}_${suffix}.tar.gz" "rockbox-codecs-${TAG}-${suffix}.tar.gz" "rockbox-assets-${TAG}-${suffix}.tar.gz"; do
  sha256sum "$archive" > "$archive.sha256"
done
rm -rf /out/runtime
collect '*.tar.gz*'

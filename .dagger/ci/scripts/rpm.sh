#!/usr/bin/env bash
set -euo pipefail
source /ci/scripts/download-release.sh
# The legacy job published x86_64 RPMs only. Use a matching SDL payload manifest;
# the live dist/rpm spec now also requires Slint desktop and Typesense files.
mkdir -p /tmp/rpmbuild/{BUILD,BUILDROOT,RPMS,SOURCES,SPECS,SRPMS}
cp -a /tmp/rbrelease/amd64 /tmp/rpmbuild/SOURCES/
cat > /tmp/rpmbuild/SPECS/rockbox.spec <<SPEC
Name: rockbox
Version: ${TAG#v}
Release: 1
Summary: Rockbox audio player and daemon
License: GPL-2.0
BuildArch: x86_64
Requires: SDL2, libunwind, alsa-lib, dbus-libs
%description
Rockbox SDL player, daemon, codecs and assets.
%install
mkdir -p %{buildroot}/usr
cp -a %{_sourcedir}/amd64/usr/local %{buildroot}/usr/
%files
/usr/local/bin/*
/usr/local/lib/rockbox
/usr/local/share/rockbox
SPEC
rpmbuild --define '_topdir /tmp/rpmbuild' -bb /tmp/rpmbuild/SPECS/rockbox.spec
cp /tmp/rpmbuild/RPMS/x86_64/*.rpm /out/
(cd /out && sha256sum ./*.rpm > SHA256SUMS)

Name:           rockbox
Version:        2026.07.12
Release:        1%{?dist}
Summary:        High quality audio player

License:        GPL-2.0

BuildArch:      aarch64

Requires: freetype, libunwind, alsa-utils, alsa-lib-devel, dbus-devel, bluez, pulseaudio-module-bluetooth, libxkbcommon-devel, libxkbcommon-x11-devel, libxcb-devel

%description
Rockbox open source high quality audio player

%prep
# Prepare the build environment

%build
# Build steps (if any)

%install
mkdir -p %{buildroot}/usr/local/bin
mkdir -p %{buildroot}/usr/local/lib
mkdir -p %{buildroot}/usr/local/share
cp -r %{_sourcedir}/arm64/usr %{buildroot}/

%files
/usr/local/bin/rockbox
/usr/local/bin/rockboxd
/usr/local/lib/rockbox/*
/usr/local/share/rockbox/*
/usr/lib/systemd/user/rockbox.service

%post
# Enable systemd user service
if [ "$1" -eq 1 ]; then
    # Fresh install
    if [ -n "$SUDO_USER" ] && [ "$SUDO_USER" != "root" ]; then
        # Get the user's UID
        USER_UID=$(id -u "$SUDO_USER" 2>/dev/null)
        if [ -n "$USER_UID" ]; then
            sudo -u "$SUDO_USER" XDG_RUNTIME_DIR=/run/user/$USER_UID systemctl --user daemon-reload &> /dev/null || :
            sudo -u "$SUDO_USER" XDG_RUNTIME_DIR=/run/user/$USER_UID systemctl --user enable rockbox.service &> /dev/null || :
            echo "Rockbox systemd service has been enabled for user $SUDO_USER"
            echo "To start the service now, run: systemctl --user start rockbox.service"
        fi
    else
        # Enable globally for all users
        systemctl --global enable rockbox.service &> /dev/null || :
        echo "Rockbox systemd service has been enabled globally"
        echo "Users can start the service with: systemctl --user start rockbox.service"
    fi
fi

%preun
# Stop and disable service on uninstall
if [ "$1" -eq 0 ]; then
    # Uninstall (not upgrade)
    if [ -n "$SUDO_USER" ] && [ "$SUDO_USER" != "root" ]; then
        USER_UID=$(id -u "$SUDO_USER" 2>/dev/null)
        if [ -n "$USER_UID" ]; then
            sudo -u "$SUDO_USER" XDG_RUNTIME_DIR=/run/user/$USER_UID systemctl --user stop rockbox.service &> /dev/null || :
            sudo -u "$SUDO_USER" XDG_RUNTIME_DIR=/run/user/$USER_UID systemctl --user disable rockbox.service &> /dev/null || :
        fi
    else
        systemctl --global disable rockbox.service &> /dev/null || :
    fi
fi

%postun
# Reload systemd after uninstall
if [ "$1" -eq 0 ]; then
    # Uninstall (not upgrade)
    if [ -n "$SUDO_USER" ] && [ "$SUDO_USER" != "root" ]; then
        USER_UID=$(id -u "$SUDO_USER" 2>/dev/null)
        if [ -n "$USER_UID" ]; then
            sudo -u "$SUDO_USER" XDG_RUNTIME_DIR=/run/user/$USER_UID systemctl --user daemon-reload &> /dev/null || :
        fi
    fi
fi

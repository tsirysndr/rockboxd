# Running `rockboxd` as a service

`rockboxd` is a long-running music server. This directory ships ready-to-use
service definitions so it starts at boot and is supervised (restarted on crash)
by your platform's init system.

| Platform         | Init system    | File                                              |
| ---------------- | -------------- | ------------------------------------------------- |
| Linux            | systemd        | [`systemd/rockbox.service`](systemd/rockbox.service)             |
| macOS            | launchd        | [`LaunchAgents/com.github.rockbox.plist`](LaunchAgents/com.github.rockbox.plist) |
| FreeBSD          | rc.d           | [`rc.d/freebsd/rockboxd`](rc.d/freebsd/rockboxd)                 |
| NetBSD           | rc.d           | [`rc.d/netbsd/rockboxd`](rc.d/netbsd/rockboxd)                   |

All of them assume `rockboxd` is installed on `PATH` (typically
`/usr/local/bin/rockboxd`, or `/usr/pkg/bin/rockboxd` on NetBSD/pkgsrc) and that
its settings live in `~/.config/rockbox.org/settings.toml` for the user the
service runs as. Because `rockboxd` links SDL, the service files set
`SDL_VIDEODRIVER=dummy` so it runs headless.

---

## Linux (systemd)

```sh
# Per-user service (recommended — picks up ~/.config/rockbox.org/settings.toml):
mkdir -p ~/.config/systemd/user
cp systemd/rockbox.service ~/.config/systemd/user/rockbox.service
systemctl --user daemon-reload
systemctl --user enable --now rockbox.service

# Follow logs:
journalctl --user -u rockbox.service -f
```

To keep a user service running without an active login session:
`loginctl enable-linger $USER`.

For a system-wide service, copy the unit into `/etc/systemd/system/` instead,
add `User=` / `Group=` lines under `[Service]`, and use `systemctl` without
`--user`.

---

## macOS (launchd)

```sh
cp LaunchAgents/com.github.rockbox.plist ~/Library/LaunchAgents/
launchctl load ~/Library/LaunchAgents/com.github.rockbox.plist

# Stop / unload:
launchctl unload ~/Library/LaunchAgents/com.github.rockbox.plist
```

Logs are written to `/tmp/rockbox.log` and `/tmp/rockbox.err.log`. Adjust
`ProgramArguments` if your binary is not at `/usr/local/bin/rockbox`.

---

## FreeBSD (rc.d)

```sh
# Install the script:
cp rc.d/freebsd/rockboxd /usr/local/etc/rc.d/rockboxd
chmod 555 /usr/local/etc/rc.d/rockboxd

# Create a dedicated user (once):
pw useradd rockbox -m -s /usr/sbin/nologin

# Enable and start:
sysrc rockboxd_enable="YES"
sysrc rockboxd_user="rockbox"
service rockboxd start
service rockboxd status
```

The script wraps `rockboxd` with `daemon(8)` for backgrounding, supervision
(`-r` restarts it if it exits), privilege drop, and logging to
`/var/log/rockboxd.log`.

Tunables (set with `sysrc` in `/etc/rc.conf`):

| Variable            | Default                  | Description                        |
| ------------------- | ------------------------ | ---------------------------------- |
| `rockboxd_enable`   | `NO`                     | Enable the service at boot         |
| `rockboxd_user`     | `rockbox`                | User the daemon runs as            |
| `rockboxd_group`    | `${rockboxd_user}`       | Group the daemon runs as           |
| `rockboxd_bin`      | `/usr/local/bin/rockboxd`| Path to the binary                 |
| `rockboxd_logfile`  | `/var/log/rockboxd.log`  | Log destination                    |
| `rockboxd_env`      | `SDL_VIDEODRIVER=dummy`  | Environment passed to the binary   |
| `rockboxd_flags`    | *(empty)*                | Extra flags for `rockboxd`         |

---

## NetBSD (rc.d)

```sh
# Install the script:
cp rc.d/netbsd/rockboxd /etc/rc.d/rockboxd
chmod 555 /etc/rc.d/rockboxd

# Create a dedicated user (once):
useradd -m -s /sbin/nologin rockbox

# Enable in /etc/rc.conf:
echo 'rockboxd=YES'            >> /etc/rc.conf
echo 'rockboxd_user="rockbox"' >> /etc/rc.conf

# Start:
/etc/rc.d/rockboxd start
/etc/rc.d/rockboxd status
```

The script looks for the binary at `/usr/pkg/bin/rockboxd` first (pkgsrc),
then falls back to `/usr/local/bin/rockboxd`. It backgrounds `rockboxd`,
drops privileges with `su`, and logs to `/var/log/rockboxd.log`.

NetBSD's base system has no `daemon(8)` supervisor, so the script does not
auto-restart on crash. If you need supervision, run it under
[daemontools](http://cr.yp.to/daemontools.html) or
[runit](http://smarden.org/runit/) from pkgsrc.

Tunables (set in `/etc/rc.conf`):

| Variable            | Default                 | Description                       |
| ------------------- | ----------------------- | --------------------------------- |
| `rockboxd`          | `NO`                    | Enable the service at boot        |
| `rockboxd_user`     | `rockbox`               | User the daemon runs as           |
| `rockboxd_logfile`  | `/var/log/rockboxd.log` | Log destination                   |
| `rockboxd_env`      | `SDL_VIDEODRIVER=dummy` | Environment passed to the binary  |
| `rockboxd_flags`    | *(empty)*               | Extra flags for `rockboxd`        |

# scale_tail

A small system-tray control centre for the Tailscale CLI, written in Rust.

- Connect / disconnect
- Pick or clear an exit node
- Browse peers, copy IPs / DNS names
- Login prompt when Tailscale needs auth

Talks to the `tailscale` CLI and shows up as a StatusNotifierItem (KDE, GNOME with AppIndicator, etc).

## Run

Download the AppImage from the latest CI run / release, or:

    cargo run --release

Runtime needs: `tailscale`, `libnotify` (notify-send), `xdg-utils`, and `wl-clipboard` or `xclip`.
To control Tailscale without root: `sudo tailscale set --operator=$USER`.

## Build the AppImage

    packaging/build-appimage.sh   # needs appimagetool on PATH

# scale_tail

A small system-tray control centre for the Tailscale CLI, written in Rust.

- Connect / disconnect, auto-connect on launch, start on login
- Pick or clear an exit node, or use the suggested one
- Browse peers: copy IPs / DNS names, ping, send files (Taildrop)
- Switch accounts
- Toggle Tailscale DNS, subnet routes, LAN access with exit node, shields up,
  Tailscale SSH and advertising this machine as an exit node
- Login prompt when Tailscale needs auth

Talks to the `tailscale` CLI and shows up as a StatusNotifierItem (KDE, GNOME with AppIndicator, etc).

## Run

Download the AppImage from the latest CI run / release, or:

    cargo run --release

Runtime needs: `tailscale`, `libnotify` (notify-send), `xdg-utils`, and `wl-clipboard` or `xclip`, and `zenity` for sending files.
To control Tailscale without root: `sudo tailscale set --operator=$USER`.

## Build the AppImage

    packaging/build-appimage.sh   # needs appimagetool on PATH

## License

MIT, see [LICENSE](LICENSE).

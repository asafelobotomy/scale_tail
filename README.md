# scale_tail

A small system-tray control centre for the Tailscale CLI, written in Rust.

- Connect / disconnect, auto-connect on launch, start on login
- Pick or clear an exit node, or use the suggested one
- Browse peers: copy IPs / DNS names, ping, send files (Taildrop)
- Switch accounts
- Toggle Tailscale DNS, subnet routes, LAN access with exit node, shields up,
  Tailscale SSH and advertising this machine as an exit node
- Login prompt when Tailscale needs auth
- Update checker: notifies about new GitHub releases (toggle in Settings)

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

## Updating the AppImage

The AppImage embeds update information (`gh-releases-zsync`) and each release ships a
`.zsync` file, so any AppImage launcher or updater that supports the standard update mechanism (Gear Lever,
AppImageUpdate, AppImageLauncher, etc.) can detect and apply updates. The desktop entry also carries
`X-AppImage-Version`, and `scale_tail --version` prints the version. Release notes
are published in the AppStream metadata and on the GitHub release page.

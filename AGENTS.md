# scale_tail

System-tray control centre for the Tailscale CLI, in Rust (single crate, StatusNotifierItem via `ksni`). Shells out to the `tailscale` CLI.

## Commands
- Run: `cargo run --release`.
- Build: `cargo build --release`.
- Test: `cargo test` (CONTRIBUTING.md; CI does not run tests, clippy or fmt).
- AppImage: `packaging/build-appimage.sh` (needs `appimagetool` on PATH; `zsyncmake` optional, for the `.zsync` file). Writes `scale_tail-<version>-x86_64.AppImage` in the repo root (git-ignored).

## Layout
- `src/main.rs`: the whole app.
- `packaging/`: `build-appimage.sh`, desktop file, AppStream metainfo, icons (`gen_icons.py` generates them), SVGs. `packaging/aur/`: PKGBUILD and .SRCINFO (release tarball; regenerate .SRCINFO after edits).
- `uq/`: uQ desktop integration (quick-settings widget), installed to `/usr/share/uq/` by the PKGBUILD. Check with `uqctl integrations check uq/`; `tests/uq/tst_widget.qml` tests the widget (`QT_QPA_PLATFORM=offscreen qmltestrunner -input tests/uq/tst_widget.qml`).

## Conventions
- Rust edition 2024, MIT. Work on `main`.  `Cargo.lock` is committed.
- Version comes from `Cargo.toml`; `build-appimage.sh` reads it for the file name and `X-AppImage-Version`. When bumping it, also add a `<release version=... date=...>` entry in `packaging/io.github.asafelobotomy.scale_tail.metainfo.xml` (the existing releases are listed there; release notes ship in this metadata).
- CI (`ci.yml`) builds the AppImage on `ubuntu-22.04` (old glibc for compatibility) on pushes to main, PRs and `v*` tags; a `v*` tag also attaches the AppImage and `.zsync` to the GitHub release.
- The AppImage embeds `gh-releases-zsync` update info; keep `scale_tail --version` output working.

## Gotchas
- Runtime needs: `tailscale`, `libnotify` (notify-send), `xdg-utils`, `wl-clipboard` or `xclip`, `zenity` (sending files). Controlling Tailscale without root needs `tailscale set --operator=$USER` (run as root).

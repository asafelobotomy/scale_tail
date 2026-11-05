#!/bin/sh
# Builds scale_tail-<version>-x86_64.AppImage. Requires `appimagetool` on PATH.
set -eu
cd "$(dirname "$0")/.."
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
cargo build --release
A=target/AppDir
rm -rf "$A"; mkdir -p "$A/usr/bin"
cp target/release/scale_tail "$A/usr/bin/"
cp packaging/scale_tail.png "$A/"
# X-AppImage-* keys are read by AppImage managers (AppImageLauncher, AppManager, etc.)
DESK=packaging/io.github.asafelobotomy.scale_tail.desktop
for d in "$A" "$A/usr/share/applications"; do mkdir -p "$d"; { cat "$DESK"; echo "X-AppImage-Name=scale_tail"; echo "X-AppImage-Version=$VERSION"; } > "$d/io.github.asafelobotomy.scale_tail.desktop"; done
mkdir -p "$A/usr/share/metainfo" "$A/usr/share/applications"
cp packaging/io.github.asafelobotomy.scale_tail.metainfo.xml "$A/usr/share/metainfo/io.github.asafelobotomy.scale_tail.appdata.xml"
cp packaging/scale_tail.png "$A/.DirIcon"
cat > "$A/AppRun" <<'RUN'
#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
exec "$HERE/usr/bin/scale_tail" "$@"
RUN
chmod +x "$A/AppRun"
# Embedded update info is the standard AppImage mechanism: any launcher/updater that
# supports it (Gear Lever, AppImageUpdate, AppImageLauncher, ...) can find new GitHub releases;
# with zsyncmake installed this also writes the matching .zsync delta file.
UPDINFO="gh-releases-zsync|asafelobotomy|scale_tail|latest|scale_tail-*x86_64.AppImage.zsync"
command -v zsyncmake >/dev/null || echo "note: zsyncmake not found, skipping .zsync file (update info is still embedded)"
ARCH=x86_64 appimagetool --appimage-extract-and-run -u "$UPDINFO" "$A" "scale_tail-$VERSION-x86_64.AppImage"

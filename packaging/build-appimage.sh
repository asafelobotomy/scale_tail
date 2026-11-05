#!/bin/sh
# Builds scale_tail-<version>-x86_64.AppImage. Requires `appimagetool` on PATH.
set -eu
cd "$(dirname "$0")/.."
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
cargo build --release
A=target/AppDir
rm -rf "$A"; mkdir -p "$A/usr/bin"
cp target/release/scale_tail "$A/usr/bin/"
cp packaging/io.github.asafelobotomy.scale_tail.desktop packaging/scale_tail.png "$A/"
mkdir -p "$A/usr/share/metainfo" "$A/usr/share/applications"
cp packaging/io.github.asafelobotomy.scale_tail.desktop "$A/usr/share/applications/"
cp packaging/io.github.asafelobotomy.scale_tail.metainfo.xml "$A/usr/share/metainfo/io.github.asafelobotomy.scale_tail.appdata.xml"
cp packaging/scale_tail.png "$A/.DirIcon"
cat > "$A/AppRun" <<'RUN'
#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
exec "$HERE/usr/bin/scale_tail" "$@"
RUN
chmod +x "$A/AppRun"
ARCH=x86_64 appimagetool --appimage-extract-and-run "$A" "scale_tail-$VERSION-x86_64.AppImage"

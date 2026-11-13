#!/bin/sh
set -eu

case "$(uname -s)" in
    Darwin) ;;
    *)
        printf '%s\n' "This script packages the native macOS application." >&2
        exit 1
        ;;
esac

package_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
repo_root=$(git -C "$package_dir" rev-parse --show-toplevel)
app_bundle="$repo_root/target/macos/Tektronix 4051 Emulator.app"

cd "$repo_root"
cargo build --locked --target-dir "$repo_root/target" \
    --manifest-path "$package_dir/Cargo.toml" \
    --bin tek4051-emulator

mkdir -p "$app_bundle/Contents/MacOS"
cp "$repo_root/target/debug/tek4051-emulator" \
    "$app_bundle/Contents/MacOS/tek4051-emulator"
cp "$package_dir/macos/Info.plist" "$app_bundle/Contents/Info.plist"
printf 'APPL????' > "$app_bundle/Contents/PkgInfo"

printf 'Created %s\n' "$app_bundle"

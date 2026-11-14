#!/bin/sh
set -eu

case "$(uname -s)" in
    Darwin) ;;
    *)
        printf '%s\n' "This script packages macOS Flash Drive test applications." >&2
        exit 1
        ;;
esac

if [ "$#" -gt 1 ]; then
    printf '%s\n' "Usage: sh package-macos-app.sh [simple_demo|battlestar|star_wars|benchmark|asteroids|f35|fractals|pictures|svg2fg|spacelab|michlanglo]" >&2
    exit 1
fi
test_target=${1:-simple_demo}
case "$test_target" in
    simple_demo) title=SimpleDemo; bundle_id=simpledemo ;;
    battlestar) title=Battlestar; bundle_id=battlestar ;;
    star_wars) title=StarWars; bundle_id=starwars ;;
    benchmark) title=Benchmark; bundle_id=benchmark ;;
    asteroids) title=Asteroids; bundle_id=asteroids ;;
    f35) title=F35; bundle_id=f35 ;;
    fractals) title=Fractals; bundle_id=fractals ;;
    pictures) title=Pictures; bundle_id=pictures ;;
    svg2fg) title=SVG2FG; bundle_id=svg2fg ;;
    spacelab) title=Spacelab; bundle_id=spacelab ;;
    michlanglo) title=Michlanglo; bundle_id=michlanglo ;;
    *)
        printf 'Unknown Flash Drive test target: %s\n' "$test_target" >&2
        exit 1
        ;;
esac

package_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
repo_root=$(git -C "$package_dir" rev-parse --show-toplevel)
app_name="Tek4051 $title Test"
app_bundle="$repo_root/target/macos/$app_name.app"

cd "$repo_root"
executable=$(cargo test --locked --target-dir "$repo_root/target" \
    --manifest-path "$package_dir/Cargo.toml" \
    --features interactive --test "$test_target" --no-run --message-format=json |
    node -e '
        let data = "";
        process.stdin.on("data", chunk => data += chunk);
        process.stdin.on("end", () => {
            let executable;
            let succeeded = false;
            for (const line of data.trim().split("\n").filter(Boolean)) {
                const message = JSON.parse(line);
                if (message.reason === "compiler-artifact" &&
                    message.target.name === process.argv[1] && message.executable) {
                    executable = message.executable;
                }
                if (message.reason === "build-finished") succeeded = message.success;
            }
            if (!succeeded || !executable) {
                console.error("No successful " + process.argv[1] + " test executable was produced.");
                process.exit(1);
            }
            process.stdout.write(executable);
        });
    ' "$test_target")

mkdir -p "$app_bundle/Contents/MacOS"
cp "$executable" "$app_bundle/Contents/MacOS/simple-demo-test"
cp "$package_dir/macos/Info.plist" "$app_bundle/Contents/Info.plist"
plutil -replace CFBundleName -string "$app_name" "$app_bundle/Contents/Info.plist"
plutil -replace CFBundleDisplayName -string "$app_name" "$app_bundle/Contents/Info.plist"
plutil -replace CFBundleIdentifier -string "com.github.hghpublic.tek4051-$bundle_id-test" "$app_bundle/Contents/Info.plist"
printf 'APPL????' > "$app_bundle/Contents/PkgInfo"

printf 'Created %s\n' "$app_bundle"
printf '%s\n' "Open this app manually in Finder to verify the visible window."
printf '%s\n' "The compiled test still reads the original assets from this workspace."

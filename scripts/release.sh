#!/bin/bash
# Build, sign and publish a release that the app can update itself to (docs/updates.md).
#
#   scripts/release.sh 0.8.0
#
# Needs the updater signing key in the environment:
#   TAURI_SIGNING_PRIVATE_KEY           the private key (content or path)
#   TAURI_SIGNING_PRIVATE_KEY_PASSWORD  its password
# If `op-claude` and a .env with op:// references exist, the script runs the
# build through it, so the key comes from 1Password and never touches disk.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

v="${1:-}"
[[ "$v" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "usage: scripts/release.sh X.Y.Z" >&2; exit 2; }
tag="v$v"
fail() { echo "release: $*" >&2; exit 1; }

# 1. Checks
[ "$(git branch --show-current)" = main ] || fail "not on main"
[ -z "$(git status --porcelain)" ] || fail "uncommitted changes"
git fetch -q origin main
[ "$(git rev-parse HEAD)" = "$(git rev-parse origin/main)" ] || fail "main is not pushed (or is behind origin)"
git rev-parse -q --verify "refs/tags/$tag" >/dev/null && fail "tag $tag already exists"
gh release view "$tag" >/dev/null 2>&1 && fail "release $tag already exists on GitHub"
for f in package.json src-tauri/tauri.conf.json; do
  grep -q "\"version\": \"$v\"" "$f" || fail "$f is not at version $v"
done
grep -q "^version = \"$v\"" src-tauri/Cargo.toml || fail "src-tauri/Cargo.toml is not at version $v"
grep -q "REPLACE_WITH_UPDATER_PUBLIC_KEY" src-tauri/tauri.conf.json && fail "no updater public key in tauri.conf.json"
notes=$(awk -v v="$v" '$0 ~ "^## "v"( |$)" {on=1; next} /^## / {on=0} on' CHANGELOG.md | sed '/./,$!d')
[ -n "$notes" ] || fail "CHANGELOG.md has no '## $v' section"

repo=$(gh repo view --json nameWithOwner -q .nameWithOwner)

# 2. Build with updater artifacts (.app.tar.gz and .sig)
run=()
if command -v op-claude >/dev/null && [ -f .env ] && grep -q '^TAURI_SIGNING_PRIVATE_KEY=' .env; then
  run=(op-claude run --env-file .env --)
elif [ -z "${TAURI_SIGNING_PRIVATE_KEY:-}" ]; then
  fail "set TAURI_SIGNING_PRIVATE_KEY (or add it to .env as an op:// reference)"
fi
echo "Building $tag…"
${run[@]+"${run[@]}"} env CI=true npm run tauri build -- --config '{"bundle":{"createUpdaterArtifacts":true}}'

# 3. Collect files with URL-safe names
b=src-tauri/target/release/bundle
out="releases/$tag"
rm -rf "$out" && mkdir -p "$out"
cp "$b/macos/Ruan's Harness.app.tar.gz" "$out/Ruans-Harness.app.tar.gz"
cp "$b/macos/Ruan's Harness.app.tar.gz.sig" "$out/Ruans-Harness.app.tar.gz.sig"
cp "$b/dmg/Ruan's Harness_${v}_aarch64.dmg" "$out/Ruans-Harness_${v}_aarch64.dmg"

# The signature must carry this version, or the app refuses it (requireSignedVersion).
base64 -d < "$out/Ruans-Harness.app.tar.gz.sig" | grep -qE "^trusted comment:.*version:${v//./\\.}([^0-9.]|$)" \
  || fail "the signature doesn't name version $v"

# 4. latest.json
NOTES="$notes" SIG="$(cat "$out/Ruans-Harness.app.tar.gz.sig")" \
URL="https://github.com/$repo/releases/download/$tag/Ruans-Harness.app.tar.gz" V="$v" \
python3 -c '
import json, os, datetime
print(json.dumps({
  "version": os.environ["V"],
  "notes": os.environ["NOTES"],
  "pub_date": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
  "platforms": {"darwin-aarch64": {"signature": os.environ["SIG"], "url": os.environ["URL"]}},
}, indent=2))' > "$out/latest.json"

echo
ls -la "$out"
echo
echo "Release notes:"
echo "$notes"
echo
read -r -p "Publish $tag to github.com/$repo now? [y/N] " ok
[ "$ok" = y ] || { echo "Not published. Files are in $out."; exit 0; }

# 5. Publish (the tag is created on GitHub at this commit)
gh release create "$tag" "$out"/* --repo "$repo" --target "$(git rev-parse HEAD)" \
  --title "Ruan's Harness $v" --notes "$notes"
git fetch -q --tags origin
echo "Published: https://github.com/$repo/releases/tag/$tag"

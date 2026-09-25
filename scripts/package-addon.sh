#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
version=${1:-0.1.0}
output_dir="$repo_root/dist"
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT HUP INT TERM

mkdir -p "$output_dir" "$stage/WoWCoachCollector"
# Every TOC ships: the base one targets the Classic/TBC clients and the
# flavor-suffixed ones let a client pick the interface version it expects.
for toc in "$repo_root"/addon/WoWCoachCollector/*.toc; do
    cp "$toc" "$stage/WoWCoachCollector/"
done
cp "$repo_root/addon/WoWCoachCollector/WoWCoachCollector.lua" "$stage/WoWCoachCollector/"

archive="$output_dir/WoWCoachCollector-$version.zip"
rm -f "$archive"
(cd "$stage" && zip -q -r "$archive" WoWCoachCollector)
printf '%s\n' "$archive"

#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
version=${1:-0.1.0}
output_dir="$repo_root/dist"
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT HUP INT TERM

mkdir -p "$output_dir" "$stage/WoWCoachCollector"
cp "$repo_root/addon/WoWCoachCollector/WoWCoachCollector.toc" "$stage/WoWCoachCollector/"
cp "$repo_root/addon/WoWCoachCollector/WoWCoachCollector.lua" "$stage/WoWCoachCollector/"

archive="$output_dir/WoWCoachCollector-$version.zip"
rm -f "$archive"
(cd "$stage" && zip -q -r "$archive" WoWCoachCollector)
printf '%s\n' "$archive"

#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
addon="$repo_root/addon/WoWCoachCollector"
toc="$addon/WoWCoachCollector.toc"
lua="$addon/WoWCoachCollector.lua"

test -f "$toc"
test -f "$lua"
grep -q '^## SavedVariables: WoWCoachCollectorDB$' "$toc"
grep -q '^## Version:' "$toc"
grep -q 'schemaVersion' "$lua"
grep -q 'gameFlavor' "$lua"

if grep -Eiq 'SendAddonMessage|C_ChatInfo|Http|socket|CastSpell|UseAction|RunMacro' "$lua"; then
    echo "collector contains a prohibited network or automation API" >&2
    exit 1
fi

if command -v luac >/dev/null 2>&1; then
    luac -p "$lua"
else
    echo "warning: luac unavailable; static validation only" >&2
fi

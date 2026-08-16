#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
addon="$repo_root/addon/WoWCoachCollector"
toc="$addon/WoWCoachCollector.toc"
lua="$addon/WoWCoachCollector.lua"
release_metadata="$repo_root/addon/release-metadata.env"

test -f "$toc"
test -f "$lua"
test -f "$release_metadata"

interface_line=$(grep '^anniversary_interface=' "$release_metadata")
expected_interface=${interface_line#anniversary_interface=}
case "$expected_interface" in
    ''|*[!0-9]*)
        echo "invalid anniversary_interface in release metadata" >&2
        exit 1
        ;;
esac

grep -qx "## Interface: $expected_interface" "$toc"
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
    echo "warning: luac unavailable; skipping bytecode syntax validation" >&2
fi

if command -v lua5.1 >/dev/null 2>&1; then
    lua_command=lua5.1
elif command -v lua >/dev/null 2>&1; then
    lua_command=lua
else
    echo "Lua interpreter is required to run addon tests" >&2
    exit 1
fi

"$lua_command" "$repo_root/tests/addon_collector_test.lua" "$lua"

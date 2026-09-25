#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
addon="$repo_root/addon/WoWCoachCollector"
toc="$addon/WoWCoachCollector.toc"
mainline_toc="$addon/WoWCoachCollector_Mainline.toc"
lua="$addon/WoWCoachCollector.lua"
release_metadata="$repo_root/addon/release-metadata.env"

test -f "$toc"
test -f "$mainline_toc"
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

forever_line=$(grep '^forever_interface=' "$release_metadata")
expected_forever=${forever_line#forever_interface=}
case "$expected_forever" in
    ''|*[!0-9]*)
        echo "invalid forever_interface in release metadata" >&2
        exit 1
        ;;
esac
grep -qx "## Interface: $expected_forever" "$mainline_toc"

# Both TOCs must load the same Lua and declare the same SavedVariables, or one
# client family silently collects into a different table.
for candidate in "$toc" "$mainline_toc"; do
    grep -q '^## SavedVariables: WoWCoachCollectorDB$' "$candidate"
    grep -qx 'WoWCoachCollector.lua' "$candidate"
done
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
"$lua_command" "$repo_root/tests/addon_collector_forever_test.lua" "$lua"

# The committed fixtures are what the Rust parser is tested against, so they
# must match what the collector writes today for each client family.
check_fixture() {
    suite="$1"
    fixture="$2"
    [ -f "$fixture" ] || return 0
    regenerated=$(mktemp)
    "$lua_command" "$repo_root/tests/$suite" "$lua" "$regenerated" >/dev/null
    if ! diff -u "$fixture" "$regenerated"; then
        rm -f "$regenerated"
        echo "fixture is stale: $fixture" >&2
        echo "  regenerate with: lua5.1 tests/$suite \\" >&2
        echo "    addon/WoWCoachCollector/WoWCoachCollector.lua \\" >&2
        echo "    $fixture" >&2
        exit 1
    fi
    rm -f "$regenerated"
}

check_fixture addon_collector_test.lua "$repo_root/fixtures/collector-v2.tbc.lua"
check_fixture addon_collector_forever_test.lua "$repo_root/fixtures/collector-v2.forever.lua"

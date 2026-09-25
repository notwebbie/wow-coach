//! `wow-coach` — read the collector's SavedVariables and show the roster.
//!
//! This is the first thing in the project that produces output a person reads,
//! and it stays useful afterwards as the way to see what the core actually made
//! of a file. Every client is a frontend over the same core, so anything shown
//! here is what the web and desktop clients will show too.
//!
//! It only reads. Nothing here writes to a WoW folder.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use wow_coach_core::auctionator::{self, PriceDb};
use wow_coach_core::classify::{self, Confidence};
use wow_coach_core::coaching::{self, Rested};
use wow_coach_core::collector::{self, CharacterRecord};
use wow_coach_core::economy;
use wow_coach_core::gap::{self, KnownCharacter};
use wow_coach_core::history::{self, History, Snapshot, XpGain};
use wow_coach_core::roster::{self, Role, RosterConfig};

const COLLECTOR_FILE: &str = "WoWCoachCollector.lua";
const AUCTIONATOR_FILE: &str = "Auctionator.lua";

/// How many holdings to print before summarising. A roster can be sitting on
/// hundreds of item types and a wall of them answers nothing.
const DEFAULT_LIMIT: usize = 20;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn usage() -> &'static str {
    "\
wow-coach — show your roster from the collector addon's saved data

USAGE:
    wow-coach [roster] [--file <path>] [--config <path>]
    wow-coach next [--file <path>] [--config <path>]
    wow-coach history [<character>] [--file <path>] [--store <path>]
    wow-coach doctor
    wow-coach classify [--dry-run] [--file <path>] [--config <path>]
    wow-coach set-role <character> <active|bank|parked|unset> [--config <path>]
    wow-coach economy [--all] [--prices <path>] [--file <path>] [--config <path>]
    wow-coach where

`next` ranks your rotation by what is being lost: a character at the rested cap
has stopped accruing, so every hour it stays parked is wasted, while one still
filling is doing its job by being left alone.

`history` shows what has changed over time. Snapshots are recorded
automatically whenever this tool sees a capture it has not stored — the addon
overwrites each character on every login, so anything not recorded is lost.

`classify` proposes a role for each character you have not decided about, shows
the evidence, and asks. It never sets one on its own.

`economy` shows what the whole roster makes and holds, valued against
Auctionator's record of the last minimum buyout it saw. Every character counts
here whatever its role — that is what marking a bank alt is for. Those values
are before the auction house's cut and assume the stock would sell.

`doctor` checks which supporting addons are installed.

Roles decide which rules apply to a character, not whether it is shown.
A bank alt leaves the play rotation but keeps its professions, bags and gold.

With no --file, every WoW installation this can find is searched."
}

fn run(args: &[String]) -> Result<(), String> {
    let mut command = "roster";
    let mut file: Option<PathBuf> = None;
    let mut config_path: Option<PathBuf> = None;
    let mut store_path: Option<PathBuf> = None;
    let mut dry_run = false;
    let mut prices_path: Option<PathBuf> = None;
    let mut show_all = false;
    let mut positional: Vec<&str> = Vec::new();

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "-h" | "--help" => {
                println!("{}", usage());
                return Ok(());
            }
            "--file" => {
                index += 1;
                file = Some(PathBuf::from(args.get(index).ok_or("--file needs a path")?));
            }
            "--store" => {
                index += 1;
                store_path = Some(PathBuf::from(
                    args.get(index).ok_or("--store needs a path")?,
                ));
            }
            "--dry-run" => dry_run = true,
            "--all" => show_all = true,
            "--prices" => {
                index += 1;
                prices_path = Some(PathBuf::from(
                    args.get(index).ok_or("--prices needs a path")?,
                ));
            }
            "--config" => {
                index += 1;
                config_path = Some(PathBuf::from(
                    args.get(index).ok_or("--config needs a path")?,
                ));
            }
            other if index == 0 && !other.starts_with('-') => command = other,
            other if other.starts_with('-') => return Err(format!("unknown option {other}")),
            other => positional.push(other),
        }
        index += 1;
    }

    let config_path = config_path.unwrap_or_else(default_config_path);
    let store_path = store_path.unwrap_or_else(default_store_path);

    match command {
        "where" => {
            let found = discover();
            if found.is_empty() {
                println!("No collector file found. Is the addon installed and enabled?");
                println!("Saved data only appears after you log out or /reload.");
                let accounts = discover_accounts();
                for account in &accounts {
                    println!("  account: {}", account.display());
                }
                let waiting = gap::find_gap(&known_characters(&accounts), &BTreeMap::new()).unseen;
                if !waiting.is_empty() {
                    println!("  {} character folders are already there", waiting.len());
                }
            } else {
                for path in found {
                    println!("{}", path.display());
                }
            }
            Ok(())
        }
        "set-role" => {
            let name = positional
                .first()
                .ok_or("which character? try: wow-coach set-role <name> bank")?;
            let role = positional
                .get(1)
                .ok_or("which role? active, bank or parked")?;
            set_role(&config_path, file.as_deref(), name, role)
        }
        "doctor" => {
            doctor();
            Ok(())
        }
        "classify" => classify_roles(file.as_deref(), &config_path, &store_path, dry_run),
        "history" => show_history(positional.first().copied(), file.as_deref(), &store_path),
        "economy" => show_economy(
            file.as_deref(),
            prices_path.as_deref(),
            &config_path,
            if show_all { usize::MAX } else { DEFAULT_LIMIT },
        ),
        "next" => show_next(file.as_deref(), &config_path),
        "roster" => show_roster(file.as_deref(), &config_path),
        other => Err(format!("unknown command {other}\n\n{}", usage())),
    }
}

fn default_config_path() -> PathBuf {
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".config/wow-coach/roster.json");
    }
    PathBuf::from("roster.json")
}

/// Look for collector files in the usual install locations.
///
/// WoW keeps one tree per flavor, each with its own accounts, so a player can
/// easily have several files. All of them are read rather than guessing which
/// one is wanted.
/// Where WoW is normally installed. Shared by both discoveries so they cannot
/// disagree about which installs exist.
fn install_roots() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(&home).join("Applications/World of Warcraft"));
    }
    roots.push(PathBuf::from("/Applications/World of Warcraft"));
    roots.push(PathBuf::from(r"C:\Program Files (x86)\World of Warcraft"));
    roots.push(PathBuf::from(r"C:\Program Files\World of Warcraft"));
    roots
}

fn discover() -> Vec<PathBuf> {
    let roots = install_roots();
    let mut found = Vec::new();
    for root in roots {
        let Ok(flavors) = std::fs::read_dir(&root) else {
            continue;
        };
        for flavor in flavors.flatten() {
            // Flavor directories are named _anniversary_, _classic_beta_, and
            // so on. The set changes, so the leading underscore is the test
            // rather than a list of known names.
            if !flavor.file_name().to_string_lossy().starts_with('_') {
                continue;
            }
            let accounts = flavor.path().join("WTF/Account");
            let Ok(entries) = std::fs::read_dir(&accounts) else {
                continue;
            };
            for account in entries.flatten() {
                let candidate = account.path().join("SavedVariables").join(COLLECTOR_FILE);
                if candidate.is_file() {
                    found.push(candidate);
                }
            }
        }
    }
    found.sort();
    found
}

/// Every character the game has a folder for, beside a collector file we read.
///
/// WoW creates `WTF/Account/<account>/<realm>/<character>/` the first time you
/// log in on a character, so the full roster is on disk whether or not the
/// collector has ever seen it. Only installs we actually read are walked, so
/// the gap is never reported against an account whose data we did not load.
fn known_characters(account_dirs: &[PathBuf]) -> Vec<KnownCharacter> {
    let mut known = Vec::new();
    for account_dir in account_dirs {
        let account_dir = account_dir.as_path();
        let flavor_dir = account_dir
            .ancestors()
            .nth(3)
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned());

        let Ok(realms) = std::fs::read_dir(account_dir) else {
            continue;
        };
        for realm in realms.flatten() {
            if !realm.path().is_dir() {
                continue;
            }
            let realm_name = realm.file_name().to_string_lossy().into_owned();
            // The account's own SavedVariables sits beside the realms.
            if realm_name == "SavedVariables" {
                continue;
            }
            let Ok(entries) = std::fs::read_dir(realm.path()) else {
                continue;
            };
            for character in entries.flatten() {
                let path = character.path();
                if !path.is_dir() {
                    continue;
                }
                // A character folder holds at least one of these. Requiring a
                // marker keeps stray directories out of the roster, and being
                // permissive about WHICH one matters because the Forever
                // client writes some folders with only an addon list in them.
                let looks_like_character = ["AddOns.txt", "SavedVariables", "config-cache.wtf"]
                    .iter()
                    .any(|marker| path.join(marker).exists());
                if !looks_like_character {
                    continue;
                }
                known.push(KnownCharacter {
                    flavor_dir: flavor_dir.clone(),
                    realm: realm_name.clone(),
                    name: character.file_name().to_string_lossy().into_owned(),
                });
            }
        }
    }
    known
}

/// Every account directory under every installation we can find.
///
/// This does not need a collector file to exist, which matters: on a first run
/// there is no data at all, and "I can see 18 characters, log in on one" is a
/// far better answer than "nothing found".
fn discover_accounts() -> Vec<PathBuf> {
    let mut accounts = Vec::new();
    for root in install_roots() {
        let Ok(flavors) = std::fs::read_dir(&root) else {
            continue;
        };
        for flavor in flavors.flatten() {
            if !flavor.file_name().to_string_lossy().starts_with('_') {
                continue;
            }
            // An install without the addon will never produce data, so naming
            // its characters would be telling the player to do something that
            // cannot help.
            if !flavor
                .path()
                .join("Interface/AddOns/WoWCoachCollector")
                .is_dir()
            {
                continue;
            }
            let Ok(entries) = std::fs::read_dir(flavor.path().join("WTF/Account")) else {
                continue;
            };
            for account in entries.flatten() {
                if !account.path().is_dir() {
                    continue;
                }
                // The account-wide SavedVariables folder sits beside the
                // accounts; it is not one.
                if account.file_name() == "SavedVariables" {
                    continue;
                }
                accounts.push(account.path());
            }
        }
    }
    accounts.sort();
    accounts
}

/// An addon we care about, and whether it is there.
struct AddonStatus {
    flavor: String,
    name: &'static str,
    installed: bool,
    required: bool,
    purpose: &'static str,
}

/// Auctionator is an optional dependency, and deliberately so.
///
/// Auction prices are the one thing better taken from an established addon
/// than collected ourselves: our own addon can only see scans the player
/// personally runs, whereas Auctionator holds a price history built from every
/// scan they have ever done. Everywhere else in this project the reverse is
/// true, which is why this is the only outside addon we lean on — and why its
/// absence disables a feature rather than breaking the tool.
fn check_addons() -> Vec<AddonStatus> {
    let mut statuses = Vec::new();
    for root in install_roots() {
        let Ok(flavors) = std::fs::read_dir(&root) else {
            continue;
        };
        for flavor in flavors.flatten() {
            let flavor_name = flavor.file_name().to_string_lossy().into_owned();
            if !flavor_name.starts_with('_') {
                continue;
            }
            let addons = flavor.path().join("Interface/AddOns");
            if !addons.is_dir() {
                continue;
            }
            for (name, required, purpose) in [
                (
                    "WoWCoachCollector",
                    true,
                    "captures your characters; nothing works without it",
                ),
                (
                    "Auctionator",
                    false,
                    "auction prices, for what is worth crafting and selling",
                ),
            ] {
                statuses.push(AddonStatus {
                    flavor: flavor_name.clone(),
                    name,
                    installed: addons.join(name).is_dir(),
                    required,
                    purpose,
                });
            }
        }
    }
    statuses
}

fn auctionator_advice(statuses: &[AddonStatus]) -> Option<String> {
    // Only worth mentioning for an install that has our collector: an install
    // we do not read is none of our business.
    let relevant: Vec<&AddonStatus> = statuses
        .iter()
        .filter(|status| {
            status.name == "Auctionator"
                && !status.installed
                && statuses.iter().any(|other| {
                    other.flavor == status.flavor
                        && other.name == "WoWCoachCollector"
                        && other.installed
                })
        })
        .collect();
    if relevant.is_empty() {
        return None;
    }
    let flavors: Vec<&str> = relevant
        .iter()
        .map(|status| status.flavor.as_str())
        .collect();
    Some(format!(
        "Auctionator is not installed ({}). It is optional, but without it there are \
         no auction prices, so nothing can say what is worth crafting or selling. \
         Install it from CurseForge or https://github.com/Auctionator/Auctionator.",
        flavors.join(", ")
    ))
}

fn doctor() {
    let statuses = check_addons();
    if statuses.is_empty() {
        println!("No World of Warcraft installation found.");
        return;
    }
    let mut flavor = String::new();
    for status in &statuses {
        if status.flavor != flavor {
            flavor = status.flavor.clone();
            println!("{flavor}");
        }
        println!(
            "  [{}] {:<18} {} — {}",
            if status.installed { "x" } else { " " },
            status.name,
            if status.required {
                "required"
            } else {
                "optional"
            },
            status.purpose
        );
    }
    if let Some(advice) = auctionator_advice(&statuses) {
        println!();
        println!("{advice}");
    }
}

fn load_config(path: &Path) -> RosterConfig {
    // A missing config is the normal first run, not a problem.
    let Ok(text) = std::fs::read_to_string(path) else {
        return RosterConfig::default();
    };
    serde_json::from_str(&text).unwrap_or_else(|error| {
        eprintln!(
            "warning: {} could not be read ({error}); continuing with default roles",
            path.display()
        );
        RosterConfig::default()
    })
}

fn save_config(path: &Path, config: &RosterConfig) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
    }
    let text = serde_json::to_string_pretty(config)
        .map_err(|error| format!("could not encode the config: {error}"))?;
    std::fs::write(path, text)
        .map_err(|error| format!("could not write {}: {error}", path.display()))
}

/// Read every file we were given or could find, merging their characters.
type Gathered = (BTreeMap<String, CharacterRecord>, Vec<String>, Vec<PathBuf>);

fn gather(file: Option<&Path>) -> Result<Gathered, String> {
    let paths = match file {
        Some(path) => vec![path.to_path_buf()],
        None => discover(),
    };
    if paths.is_empty() {
        // Nothing captured yet — but the game's own folders still know the
        // roster, and naming it beats reporting nothing. This is the first-run
        // case the whole gap feature exists for.
        let waiting =
            gap::find_gap(&known_characters(&discover_accounts()), &BTreeMap::new()).unseen;
        let mut message = String::from(
            "No collector data yet. Log in on a character, then log out or /reload — \
             the game only writes its saved variables then.",
        );
        if !waiting.is_empty() {
            let mut names: Vec<&str> = waiting
                .iter()
                .map(|character| character.name.as_str())
                .collect();
            names.sort_unstable();
            names.dedup();
            let _ = write!(
                message,
                "\n\nThe game already has folders for {} characters: {}.",
                names.len(),
                names.join(", ")
            );
        }
        return Err(message);
    }

    let mut characters = BTreeMap::new();
    let mut notices = Vec::new();
    let read_paths = paths.clone();
    for path in paths {
        let text = std::fs::read_to_string(&path)
            .map_err(|error| format!("could not read {}: {error}", path.display()))?;
        let loaded =
            collector::load(&text).map_err(|error| format!("{}: {error}", path.display()))?;
        notices.extend(loaded.notices);
        for (key, record) in loaded.db.characters {
            // The same character can appear in several files only if the same
            // account is installed twice; keep whichever was captured later.
            match characters.entry(key) {
                std::collections::btree_map::Entry::Vacant(slot) => {
                    slot.insert(record);
                }
                std::collections::btree_map::Entry::Occupied(mut slot) => {
                    if record.captured_at > slot.get().captured_at {
                        slot.insert(record);
                    }
                }
            }
        }
    }
    Ok((characters, notices, read_paths))
}

fn set_role(config_path: &Path, file: Option<&Path>, name: &str, role: &str) -> Result<(), String> {
    // `unset` is not a role: it removes the decision entirely. Setting a
    // character back to `active` would still count as having decided, and
    // would silence suggestions for it forever.
    let clearing = matches!(
        role.to_ascii_lowercase().as_str(),
        "unset" | "none" | "clear"
    );
    let role = if clearing {
        Role::Active
    } else {
        match role.to_ascii_lowercase().as_str() {
            "active" => Role::Active,
            "bank" => Role::Bank,
            "parked" => Role::Parked,
            other => {
                return Err(format!(
                    "{other} is not a role; use active, bank, parked, or unset to undo"
                ))
            }
        }
    };

    let (characters, _, _) = gather(file)?;
    let key = RosterConfig::resolve(&characters, name)
        .ok_or_else(|| format!("no character called {name} in your saved data"))?
        .to_string();

    let mut config = load_config(config_path);
    if clearing {
        if config.clear_role(&key) {
            save_config(config_path, &config)?;
            println!("{name} is unclassified again, and may be suggested a role.");
        } else {
            println!("{name} had no role set.");
        }
        return Ok(());
    }

    config.set_role(key, role);
    save_config(config_path, &config)?;

    println!("{name} is now {}.", describe_role(role));
    if role != Role::Active {
        println!("It stays in professions, materials and gold; it just leaves the play rotation.");
    } else {
        println!("That is an explicit choice, so it will not be suggested a role again.");
    }
    Ok(())
}

fn describe_role(role: Role) -> &'static str {
    match role {
        Role::Active => "active",
        Role::Bank => "a bank alt",
        Role::Parked => "parked",
    }
}

/// Rank the rotation and say why.
fn default_store_path() -> PathBuf {
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".config/wow-coach/history.jsonl");
    }
    PathBuf::from("wow-coach-history.jsonl")
}

fn load_history(path: &Path) -> History {
    let Ok(text) = std::fs::read_to_string(path) else {
        return History::default();
    };
    let (history, problems) = history::parse_jsonl(&text);
    for problem in problems {
        eprintln!("warning: {problem}");
    }
    history
}

/// Record anything we have not seen before.
///
/// The addon overwrites each character's record on every capture, so a state
/// not recorded here is gone for good. Recording happens automatically rather
/// than on a command nobody would remember, and deduplication on capture time
/// makes running the tool repeatedly a no-op.
fn record_history(
    path: &Path,
    characters: &BTreeMap<String, CharacterRecord>,
) -> Result<(History, usize), String> {
    let mut store = load_history(path);
    let mut fresh = Vec::new();
    for (key, record) in characters {
        let Some(snapshot) = Snapshot::from_record(key, record) else {
            continue;
        };
        if store.record(snapshot.clone()) {
            fresh.push(snapshot);
        }
    }
    if fresh.is_empty() {
        return Ok((store, 0));
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
    }
    let mut appended = String::new();
    for snapshot in &fresh {
        appended.push_str(&history::to_jsonl_line(snapshot)?);
        appended.push('\n');
    }
    use std::io::Write as _;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| format!("could not open {}: {error}", path.display()))?;
    file.write_all(appended.as_bytes())
        .map_err(|error| format!("could not write {}: {error}", path.display()))?;
    Ok((store, fresh.len()))
}

fn gold(copper: i64) -> String {
    let sign = if copper < 0 { "-" } else { "" };
    let copper = copper.unsigned_abs();
    format!(
        "{sign}{}g {}s {}c",
        copper / 10_000,
        (copper / 100) % 100,
        copper % 100
    )
}

fn when(epoch: i64) -> String {
    // No date library here on purpose: days elapsed is what the reader wants,
    // and it needs no timezone to be right.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(epoch);
    let days = (now - epoch) / 86_400;
    match days {
        d if d <= 0 => "today".to_string(),
        1 => "yesterday".to_string(),
        d => format!("{d} days ago"),
    }
}

fn describe_xp(gain: &XpGain) -> String {
    match gain {
        XpGain::Exact(0) => "no XP".to_string(),
        XpGain::Exact(amount) => format!("{amount} XP"),
        XpGain::AtLeast(amount) => format!("at least {amount} XP"),
        XpGain::Unknown => "XP not known".to_string(),
    }
}

fn now_epoch() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or_default()
}

/// Propose a role for each unclassified character and ask.
///
/// Tagging a large roster by hand is a chore nobody does, and an untagged
/// roster makes every rotation suggestion worse. So the tool guesses from what
/// it can see — and then asks, because a bank alt and a character whose owner
/// was on holiday look identical from the outside.
fn classify_roles(
    file: Option<&Path>,
    config_path: &Path,
    store_path: &Path,
    dry_run: bool,
) -> Result<(), String> {
    let (characters, _, _) = gather(file)?;
    let (store, _) = record_history(store_path, &characters)?;
    let mut config = load_config(config_path);
    let entries = roster::roster(&characters, &config);
    let suggestions = classify::suggest_all(&entries, &config, &store, now_epoch());

    if suggestions.is_empty() {
        println!("Nothing to propose — every character is either classified or looks active.");
        return Ok(());
    }

    println!(
        "{} character(s) look like they might not belong in the play rotation.\n",
        suggestions.len()
    );

    let mut changed = 0;
    for suggestion in &suggestions {
        println!(
            "{}  —  suggest {} ({})",
            suggestion.name,
            describe_role(suggestion.suggested),
            suggestion.confidence.describe()
        );
        for line in &suggestion.evidence {
            println!("    {line}");
        }
        if suggestion.confidence == Confidence::Possible {
            println!("    This is a guess from a single capture; it may simply be new.");
        }

        if dry_run {
            println!();
            continue;
        }

        print!("    accept / [a]ctive / [b]ank / [p]arked / [s]kip / [q]uit? [accept] ");
        use std::io::Write as _;
        let _ = std::io::stdout().flush();

        let mut answer = String::new();
        if std::io::stdin().read_line(&mut answer).is_err() {
            println!();
            break;
        }
        let chosen = match answer.trim().to_ascii_lowercase().as_str() {
            "" | "accept" | "y" | "yes" => Some(suggestion.suggested),
            "a" | "active" => Some(Role::Active),
            "b" | "bank" => Some(Role::Bank),
            "p" | "parked" => Some(Role::Parked),
            "q" | "quit" => break,
            _ => None,
        };
        match chosen {
            Some(role) => {
                config.set_role(suggestion.key.clone(), role);
                changed += 1;
                println!("    -> {}", describe_role(role));
            }
            None => println!("    -> left undecided"),
        }
        println!();
    }

    if dry_run {
        println!("Nothing was changed. Drop --dry-run to decide.");
        return Ok(());
    }
    if changed > 0 {
        save_config(config_path, &config)?;
        println!("Saved {changed} role(s) to {}.", config_path.display());
    } else {
        println!("Nothing changed.");
    }
    Ok(())
}

fn show_history(name: Option<&str>, file: Option<&Path>, store_path: &Path) -> Result<(), String> {
    let (characters, _, _) = gather(file)?;
    let (store, added) = record_history(store_path, &characters)?;
    if added > 0 {
        println!("Recorded {added} new snapshot(s).");
        println!();
    }
    if store.is_empty() {
        println!("No history yet. It builds up as you play and run this.");
        return Ok(());
    }

    if let Some(name) = name {
        let key = store
            .snapshots()
            .iter()
            .find(|snapshot| snapshot.name.eq_ignore_ascii_case(name))
            .map(|snapshot| snapshot.key.clone())
            .ok_or_else(|| format!("no history for {name}"))?;

        let snapshots = store.for_character(&key);
        println!("{} — {} snapshot(s)", snapshots[0].name, snapshots.len());
        if snapshots.len() < 2 {
            println!();
            println!(
                "  Only one capture so far, so there is nothing to compare it against yet. \
                 Play, log out, and run this again."
            );
            return Ok(());
        }
        println!();
        for pair in snapshots.windows(2) {
            let change = history::delta(pair[0], pair[1]);
            if change.is_idle() {
                continue;
            }
            let mut parts = vec![describe_xp(&change.xp)];
            if change.levels_gained != 0 {
                parts.push(format!("{:+} level(s)", change.levels_gained));
            }
            if let Some(copper) = change.copper.filter(|copper| *copper != 0) {
                parts.push(gold(copper));
            }
            for (skill, gained) in &change.skills {
                parts.push(format!("{skill} {gained:+}"));
            }
            println!("  {:<14} {}", when(change.to), parts.join(", "));
        }
        if let Some(total) = history::total_delta(&store, &key) {
            println!();
            println!(
                "  Total since {}: {}{}{}",
                when(total.from),
                describe_xp(&total.xp),
                if total.levels_gained != 0 {
                    format!(", {:+} level(s)", total.levels_gained)
                } else {
                    String::new()
                },
                total
                    .copper
                    .filter(|copper| *copper != 0)
                    .map(|copper| format!(", {}", gold(copper)))
                    .unwrap_or_default()
            );
        }
        return Ok(());
    }

    println!(
        "{:<14} {:>10} {:>18} CHANGE",
        "CHARACTER", "SNAPSHOTS", "SINCE"
    );
    for (key, latest) in store.latest_per_character() {
        let count = store.for_character(key).len();
        match history::total_delta(&store, key) {
            Some(total) => {
                let mut parts = vec![describe_xp(&total.xp)];
                if total.levels_gained != 0 {
                    parts.push(format!("{:+} level(s)", total.levels_gained));
                }
                println!(
                    "{:<14} {:>10} {:>18} {}",
                    latest.name,
                    count,
                    when(total.from),
                    parts.join(", ")
                );
            }
            None => println!(
                "{:<14} {:>10} {:>18} first capture — nothing to compare yet",
                latest.name,
                count,
                when(latest.captured_at)
            ),
        }
    }

    // Two weeks with captures either side and nothing gained.
    let fortnight = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64 - 14 * 86_400)
        .unwrap_or(0);
    let stalled = history::stalled(&store, fortnight);
    if !stalled.is_empty() {
        println!();
        println!(
            "Captured a fortnight ago and since, with nothing gained: {}",
            stalled
                .iter()
                .map(|change| change.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    Ok(())
}

/// Rank the rotation and say why.
fn show_next(file: Option<&Path>, config_path: &Path) -> Result<(), String> {
    let (characters, notices, read_paths) = gather(file)?;
    // Record before anything else: the addon overwrites its file on every
    // login, so a state we do not keep now is gone.
    if let Err(error) = record_history(&default_store_path(), &characters) {
        eprintln!("warning: history not recorded: {error}");
    }
    let config = load_config(config_path);
    let entries = roster::roster(&characters, &config);

    let account_dirs: Vec<PathBuf> = read_paths
        .iter()
        .filter_map(|path| path.parent().and_then(Path::parent))
        .map(Path::to_path_buf)
        .collect();
    let roster_gap = gap::find_gap(&known_characters(&account_dirs), &characters);
    let advice = coaching::advise(&entries, roster_gap.is_complete());

    match advice.play_next() {
        Some(next) => {
            // Only claim a recommendation when something is actually at stake.
            // If every character is still banking rest, the honest answer is
            // that nothing is urgent — saying "play this next" while the
            // reason reads "leaving it parked is working" is incoherent.
            match &next.rested {
                Rested::AtCap { .. } | Rested::NearCap { .. } => {
                    println!("Play {} next.", next.name);
                }
                Rested::Filling { .. } => {
                    println!(
                        "Nothing is urgent — every character is still banking rest.\n\
                         If you want to play now, {} is closest to its cap and has the \
                         most a session would clear.",
                        next.name
                    );
                }
                Rested::NotResting { .. } | Rested::Unknown { .. } => {
                    println!("No ranking is possible from the rest data available.");
                }
            }
            println!();
            for reason in &next.reasons {
                println!("  {reason}");
            }
            // A wall of bullets is not advice. Show what one session would
            // most usefully clear, and say how many more there are.
            const SHOWN: usize = 4;
            for friction in next.frictions.iter().take(SHOWN) {
                println!("  - {}", friction.summary);
            }
            if next.frictions.len() > SHOWN {
                println!("  - and {} more", next.frictions.len() - SHOWN);
            }
        }
        None => println!("Nothing to suggest: no character is both in the rotation and resting."),
    }

    if advice.play.len() > 1 {
        println!();
        println!("Then:");
        for suggestion in advice.play.iter().skip(1) {
            println!(
                "  {:<14} {}",
                suggestion.name,
                match &suggestion.rested {
                    Rested::AtCap { .. } => "at the rested cap".to_string(),
                    Rested::NearCap { rested, cap } => format!("near the cap ({rested}/{cap})"),
                    Rested::Filling { rested, cap } => format!("filling ({rested}/{cap})"),
                    Rested::NotResting { .. } => "not resting".to_string(),
                    Rested::Unknown { .. } => "cannot be ranked on rest".to_string(),
                }
            );
        }
    }

    if !advice.park.is_empty() {
        println!();
        println!("Park these — they are accruing no rest where they are:");
        for suggestion in &advice.park {
            println!("  {}", suggestion.name);
        }
    }

    if !advice.caveats.is_empty() {
        println!();
        for caveat in &advice.caveats {
            println!("Note: {caveat}");
        }
    }
    if let Some(summary) = roster_gap.summary() {
        println!("{summary}");
    }
    if let Some(advice) = auctionator_advice(&check_addons()) {
        println!();
        println!("{advice}");
    }
    for notice in &notices {
        println!("  - {notice}");
    }
    Ok(())
}

fn show_roster(file: Option<&Path>, config_path: &Path) -> Result<(), String> {
    let (characters, notices, read_paths) = gather(file)?;
    // Record before anything else: the addon overwrites its file on every
    // login, so a state we do not keep now is gone.
    if let Err(error) = record_history(&default_store_path(), &characters) {
        eprintln!("warning: history not recorded: {error}");
    }
    let config = load_config(config_path);
    let mut entries = roster::roster(&characters, &config);

    // Sorted by how full the rested bar is, because that is the fact the
    // coaching rules will key on. This is not yet a recommendation — the
    // engine that turns it into one is still to come, and saying otherwise
    // would repeat the prototype's mistake of dressing an arbitrary order up
    // as advice.
    entries.sort_by(|a, b| {
        b.record
            .rested_fraction()
            .unwrap_or(-1.0)
            .partial_cmp(&a.record.rested_fraction().unwrap_or(-1.0))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.name().cmp(b.name()))
    });

    if entries.is_empty() {
        println!("The file has no characters yet. Log in on one and then log out.");
        return Ok(());
    }

    println!(
        "{:<14} {:>3} {:<10} {:<7} {:>10} {:>6} {:>7} {:>8}",
        "CHARACTER", "LVL", "CLASS", "ROLE", "RESTED", "BAGS", "QUESTS", "GOLD"
    );
    for entry in &entries {
        let record = entry.record;
        println!(
            "{:<14} {:>3} {:<10} {:<7} {:>10} {:>6} {:>7} {:>8}",
            truncate(entry.name(), 14),
            record.level.map(|l| l.to_string()).unwrap_or_else(dash),
            truncate(record.class.as_deref().unwrap_or("—"), 10),
            match entry.role {
                Role::Active => "active",
                Role::Bank => "bank",
                Role::Parked => "parked",
            },
            rested_column(record),
            record
                .free_bag_slots()
                .map(|s| s.to_string())
                .unwrap_or_else(dash),
            record
                .quest_count()
                .map(|c| c.to_string())
                .unwrap_or_else(dash),
            record.gold().map(|g| format!("{g}g")).unwrap_or_else(dash),
        );
    }

    // <flavor>/WTF/Account/<account>/SavedVariables/WoWCoachCollector.lua
    let account_dirs: Vec<PathBuf> = read_paths
        .iter()
        .filter_map(|file| file.parent().and_then(Path::parent))
        .map(Path::to_path_buf)
        .collect();
    let roster_gap = gap::find_gap(&known_characters(&account_dirs), &characters);
    if let Some(summary) = roster_gap.summary() {
        println!();
        println!("{summary}");
        println!(
            "Until then, anything about which character to play is being decided \
             without seeing those."
        );
    }

    let banked: Vec<_> = entries
        .iter()
        .filter(|entry| !entry.role.in_play_rotation())
        .collect();
    if !banked.is_empty() {
        println!();
        println!(
            "{} out of the play rotation, still counted for professions and gold: {}",
            banked.len(),
            banked
                .iter()
                .map(|entry| entry.name())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    if entries
        .iter()
        .any(|entry| !entry.record.rested_cap_is_known())
    {
        println!();
        println!(
            "Rested percentages are blank for Forever characters: the Legacy \"Well Rested\" \
             perk changes both the rested cap and the rate it fills at, and nothing in the \
             game's API reports either. Measuring it is what the beta probe is for."
        );
    }

    if let Some(advice) = auctionator_advice(&check_addons()) {
        println!();
        println!("{advice}");
    }

    if !entries.is_empty() {
        println!();
        let store = load_history(&default_store_path());
        let pending = classify::suggest_all(&entries, &config, &store, now_epoch()).len();
        if pending > 0 {
            println!(
                "Run `wow-coach next` for what to play, or `wow-coach classify` — \
                 {pending} character(s) may not belong in the rotation."
            );
        } else {
            println!("Run `wow-coach next` for what to play and why.");
        }
    }

    if !notices.is_empty() {
        println!();
        println!("Notes:");
        for notice in &notices {
            println!("  - {notice}");
        }
    }

    Ok(())
}

fn dash() -> String {
    "—".to_string()
}

/// The rested column, blank where the model is not established rather than
/// showing a number that would be wrong.
fn rested_column(record: &CharacterRecord) -> String {
    if !record.rested_cap_is_known() {
        return record
            .rested_xp
            .map(|xp| format!("{xp} xp"))
            .unwrap_or_else(dash);
    }
    match (record.rested_fraction(), record.rested_xp) {
        (Some(fraction), Some(_)) => {
            let percent = (fraction * 100.0).round() as i64;
            let mut out = String::new();
            let _ = write!(out, "{percent}%");
            if fraction >= 0.999 {
                out.push_str(" CAP");
            }
            out
        }
        _ => dash(),
    }
}

fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    text.chars()
        .take(width.saturating_sub(1))
        .collect::<String>()
        + "…"
}

/// Where Auctionator keeps its price database.
///
/// Account-level, not per character: the database is shared across every
/// character on the account, which is exactly why it is worth reading. The
/// per-character files beside it hold that character's own settings and no
/// prices.
fn discover_prices() -> Vec<PathBuf> {
    let mut found = Vec::new();
    for root in install_roots() {
        let Ok(flavors) = std::fs::read_dir(&root) else {
            continue;
        };
        for flavor in flavors.flatten() {
            if !flavor.file_name().to_string_lossy().starts_with('_') {
                continue;
            }
            let Ok(accounts) = std::fs::read_dir(flavor.path().join("WTF/Account")) else {
                continue;
            };
            for account in accounts.flatten() {
                let candidate = account.path().join("SavedVariables").join(AUCTIONATOR_FILE);
                if candidate.is_file() {
                    found.push(candidate);
                }
            }
        }
    }
    found.sort();
    found
}

/// Read every Auctionator database we were given or could find.
///
/// A file that will not parse costs its prices and nothing else: the rest of
/// the economy view is still worth showing, and a hard failure here would mean
/// one bad file hides the player's whole roster.
fn load_prices(paths: Option<&Path>) -> (PriceDb, Vec<String>) {
    let paths = match paths {
        Some(path) => vec![path.to_path_buf()],
        None => discover_prices(),
    };
    let mut db = PriceDb::default();
    let mut problems = Vec::new();
    for path in paths {
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                problems.push(format!("could not read {}: {error}", path.display()));
                continue;
            }
        };
        match auctionator::load(&bytes) {
            Ok(loaded) => {
                problems.extend(loaded.notes);
                db.vendor.extend(loaded.vendor);
                for (realm, prices) in loaded.realms {
                    // Two accounts can both have scanned the same realm.
                    // Whichever saw more items is the better database.
                    match db.realms.entry(realm) {
                        std::collections::btree_map::Entry::Vacant(slot) => {
                            slot.insert(prices);
                        }
                        std::collections::btree_map::Entry::Occupied(mut slot) => {
                            if prices.items.len() > slot.get().items.len() {
                                slot.insert(prices);
                            }
                        }
                    }
                }
            }
            Err(error) => problems.push(format!("{}: {error}", path.display())),
        }
    }
    (db, problems)
}

fn show_economy(
    file: Option<&Path>,
    prices_path: Option<&Path>,
    config_path: &Path,
    limit: usize,
) -> Result<(), String> {
    let (characters, notices, _) = gather(file)?;
    if let Err(error) = record_history(&default_store_path(), &characters) {
        eprintln!("warning: history not recorded: {error}");
    }
    let config = load_config(config_path);
    let entries = roster::roster(&characters, &config);
    let (prices, problems) = load_prices(prices_path);

    let survey = economy::survey(&entries, &prices);

    println!("PROFESSIONS");
    if survey.professions.is_empty() {
        println!(
            "  None captured. Open each profession window once in game, then log out — \
             the client only tells an addon about a trade skill it has been shown."
        );
    } else {
        for holder in &survey.professions {
            let rank = match (holder.rank, holder.max_rank) {
                (Some(rank), Some(max)) => format!("{rank}/{max}"),
                (Some(rank), None) => rank.to_string(),
                _ => "—".to_string(),
            };
            let recipes = holder
                .recipes_known
                .filter(|known| *known > 0)
                .map(|known| format!(", {known} recipes"))
                .unwrap_or_default();
            let note = if holder.needs_training() {
                "  ← at its cap, needs a trainer"
            } else {
                ""
            };
            println!(
                "  {:<16} {:<14} {:>9}{}{}",
                holder.profession,
                format!("{} ({})", holder.name, describe_role_short(holder.role)),
                rank,
                recipes,
                note
            );
        }
    }

    println!();
    println!("HOLDINGS");
    if survey.holdings.is_empty() && survey.unpriced.is_empty() {
        println!("  Nothing captured in anybody's bags.");
    } else {
        let markets: std::collections::BTreeSet<&str> = survey
            .holdings
            .iter()
            .chain(&survey.unpriced)
            .filter_map(|holding| holding.market.as_deref())
            .collect();
        let split = markets.len() > 1;
        if split {
            println!(
                "  {:<26} {:<9} {:>7} {:>13} {:>12}  HELD BY",
                "ITEM", "MARKET", "COUNT", "EACH", "VALUE"
            );
        } else {
            println!(
                "  {:<28} {:>7} {:>13} {:>12}  HELD BY",
                "ITEM", "COUNT", "EACH", "VALUE"
            );
        }
        for holding in survey.holdings.iter().take(limit) {
            // A price nothing else supports is marked where it is read, not
            // only in a note at the bottom that nobody reaches.
            let each = if holding.price_looks_like_an_outlier() {
                format!("{}?", money(holding.unit_price.unwrap_or(0)))
            } else {
                money(holding.unit_price.unwrap_or(0))
            };
            if split {
                println!(
                    "  {:<26} {:<9} {:>7} {:>13} {:>12}  {}",
                    truncate(&item_label(holding), 26),
                    market_tag(holding),
                    holding.count,
                    each,
                    money(holding.value.unwrap_or(0)),
                    truncate(&holders(holding), 34),
                );
            } else {
                println!(
                    "  {:<28} {:>7} {:>13} {:>12}  {}",
                    truncate(&item_label(holding), 28),
                    holding.count,
                    each,
                    money(holding.value.unwrap_or(0)),
                    truncate(&holders(holding), 40),
                );
            }
        }
        if survey.holdings.len() > limit {
            println!(
                "  … and {} more priced item type(s); --all shows every one.",
                survey.holdings.len() - limit
            );
        }
    }

    println!();
    let (gold, silver, copper) = economy::coin(survey.gold_copper);
    println!("Coin carried                {gold}g {silver}s {copper}c");
    let (gold, silver, copper) = economy::coin(survey.holdings_value);
    println!("Bags, at last seen buyout   {gold}g {silver}s {copper}c");
    // The second total exists only because a flagged listing moved the first.
    // With nothing flagged the two are equal, and printing both would imply a
    // disagreement that is not there.
    if survey.holdings_value_typical != survey.holdings_value {
        let (gold, silver, copper) = economy::coin(survey.holdings_value_typical);
        println!("Without those listings      {gold}g {silver}s {copper}c");
    }

    if !survey.unpriced.is_empty() {
        println!();
        println!("UNPRICED — held, but Auctionator has never seen it on the auction house");
        for holding in survey.unpriced.iter().take(limit.min(10)) {
            println!(
                "  {:<28} {:>7}  {}",
                truncate(&item_label(holding), 28),
                holding.count,
                truncate(&holders(holding), 40)
            );
        }
        if survey.unpriced.len() > limit.min(10) {
            println!("  … and {} more.", survey.unpriced.len() - limit.min(10));
        }
    }

    if !survey.caveats.is_empty() {
        println!();
        for caveat in &survey.caveats {
            println!("Note: {caveat}");
        }
    }
    for problem in problems {
        eprintln!("warning: {problem}");
    }
    for notice in notices {
        eprintln!("note: {notice}");
    }
    if prices.realms.is_empty() {
        if let Some(advice) = auctionator_advice(&check_addons()) {
            println!();
            println!("{advice}");
        }
    }
    Ok(())
}

fn item_label(holding: &economy::Holding) -> String {
    match &holding.name {
        Some(name) => name.clone(),
        None => format!("item {}", holding.item_id),
    }
}

fn holders(holding: &economy::Holding) -> String {
    holding
        .held_by
        .iter()
        .map(|(name, count)| format!("{name} {count}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The faction half of a market key, for the column that only appears when the
/// roster actually spans two auction houses. Printing "Dreamscythe Horde" on
/// every row of a single-realm roster is noise; printing nothing when there
/// are two markets is a lie by omission.
fn market_tag(holding: &economy::Holding) -> String {
    holding
        .market
        .as_deref()
        .and_then(|market| {
            market
                .rsplit_once(' ')
                .map(|(_, faction)| faction.to_string())
        })
        .unwrap_or_else(|| "?".to_string())
}

/// Copper as the game writes it, dropping the units that are zero so a column
/// of prices stays readable.
fn money(copper: u64) -> String {
    let (gold, silver, copper) = economy::coin(copper);
    if gold > 0 {
        format!("{gold}g {silver}s")
    } else if silver > 0 {
        format!("{silver}s {copper}c")
    } else {
        format!("{copper}c")
    }
}

fn describe_role_short(role: Role) -> &'static str {
    match role {
        Role::Active => "active",
        Role::Bank => "bank",
        Role::Parked => "parked",
    }
}

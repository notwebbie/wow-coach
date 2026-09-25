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

use wow_coach_core::collector::{self, CharacterRecord};
use wow_coach_core::roster::{self, Role, RosterConfig};

const COLLECTOR_FILE: &str = "WoWCoachCollector.lua";

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
    wow-coach set-role <character> <active|bank|parked> [--config <path>]
    wow-coach where

Roles decide which rules apply to a character, not whether it is shown.
A bank alt leaves the play rotation but keeps its professions, bags and gold.

With no --file, every WoW installation this can find is searched."
}

fn run(args: &[String]) -> Result<(), String> {
    let mut command = "roster";
    let mut file: Option<PathBuf> = None;
    let mut config_path: Option<PathBuf> = None;
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

    match command {
        "where" => {
            let found = discover();
            if found.is_empty() {
                println!("No collector file found. Is the addon installed and have you logged in?");
                println!("Saved data only appears after you log out or /reload.");
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
fn discover() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(&home).join("Applications/World of Warcraft"));
    }
    roots.push(PathBuf::from("/Applications/World of Warcraft"));
    roots.push(PathBuf::from(r"C:\Program Files (x86)\World of Warcraft"));
    roots.push(PathBuf::from(r"C:\Program Files\World of Warcraft"));

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
fn gather(file: Option<&Path>) -> Result<(BTreeMap<String, CharacterRecord>, Vec<String>), String> {
    let paths = match file {
        Some(path) => vec![path.to_path_buf()],
        None => discover(),
    };
    if paths.is_empty() {
        return Err(
            "No collector file found. Install the addon, log in, then log out or \
                    /reload so the game writes its saved variables.\n\
                    Run `wow-coach where` to see where it looks."
                .to_string(),
        );
    }

    let mut characters = BTreeMap::new();
    let mut notices = Vec::new();
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
    Ok((characters, notices))
}

fn set_role(config_path: &Path, file: Option<&Path>, name: &str, role: &str) -> Result<(), String> {
    let role = match role.to_ascii_lowercase().as_str() {
        "active" => Role::Active,
        "bank" => Role::Bank,
        "parked" => Role::Parked,
        other => return Err(format!("{other} is not a role; use active, bank or parked")),
    };

    let (characters, _) = gather(file)?;
    let key = RosterConfig::resolve(&characters, name)
        .ok_or_else(|| format!("no character called {name} in your saved data"))?
        .to_string();

    let mut config = load_config(config_path);
    config.set_role(key, role);
    save_config(config_path, &config)?;

    println!("{name} is now {}.", describe_role(role));
    if role != Role::Active {
        println!("It stays in professions, materials and gold; it just leaves the play rotation.");
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

fn show_roster(file: Option<&Path>, config_path: &Path) -> Result<(), String> {
    let (characters, notices) = gather(file)?;
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

#[cfg(feature = "desktop-runtime")]
use tauri_plugin_sql::{Migration, MigrationKind};

#[cfg(feature = "desktop-runtime")]
const HISTORY_MIGRATION: &str = include_str!("../migrations/0001_history.sql");

#[cfg(feature = "desktop-runtime")]
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let migrations = vec![Migration {
        version: 1,
        description: "create_character_snapshot_history",
        sql: HISTORY_MIGRATION,
        kind: MigrationKind::Up,
    }];

    tauri::Builder::default()
        .plugin(
            tauri_plugin_sql::Builder::default()
                .add_migrations("sqlite:wow-coach.db", migrations)
                .build(),
        )
        .run(tauri::generate_context!())
        .expect("failed to run WoW Coach desktop application");
}

#[cfg(not(feature = "desktop-runtime"))]
pub fn run() {
    eprintln!("native runtime is disabled; see desktop/README.md");
}

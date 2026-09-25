// Scratch tool: show what the Lua parser makes of a file, before serde sees it.
fn main() {
    let path = std::env::args().nth(1).expect("usage: dump <file>");
    let text = std::fs::read_to_string(&path).unwrap();
    let globals = wow_coach_core::lua::parse_saved_variables(&text).expect("parse");
    let value = globals.get("WoWCoachCollectorDB").expect("global");
    let json = wow_coach_core::lua::to_json(value).expect("json");
    // Walk it and report the type of every field the schema expects as a list.
    fn walk(value: &serde_json::Value, path: &str) {
        const LISTS: [&str; 6] = ["skills", "quests", "trees", "bags", "contents", "recipes"];
        match value {
            serde_json::Value::Object(map) => {
                for (key, item) in map {
                    let here = format!("{path}.{key}");
                    if LISTS.contains(&key.as_str()) && !item.is_array() {
                        println!("NOT AN ARRAY: {here} is {}", kind(item));
                    }
                    walk(item, &here);
                }
            }
            serde_json::Value::Array(items) => {
                for (index, item) in items.iter().enumerate() {
                    walk(item, &format!("{path}[{index}]"));
                }
            }
            _ => {}
        }
    }
    fn kind(value: &serde_json::Value) -> &'static str {
        match value {
            serde_json::Value::Object(_) => "an object",
            serde_json::Value::Array(_) => "an array",
            serde_json::Value::String(_) => "a string",
            serde_json::Value::Number(_) => "a number",
            serde_json::Value::Bool(_) => "a bool",
            serde_json::Value::Null => "null",
        }
    }
    walk(&json, "$");
    println!("--- now try the typed load ---");
    match wow_coach_core::collector::load(&text) {
        Ok(loaded) => println!("loaded {} characters", loaded.db.characters.len()),
        Err(error) => println!("ERROR: {error}"),
    }
}

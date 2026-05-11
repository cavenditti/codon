//! Convert `toml::Value` to `serde_json::Value`.
//!
//! Translation is structural — strings, bools, numbers, and arrays map to
//! their JSON counterparts; tables become objects. TOML datetime values are
//! serialised as RFC 3339 strings (the only sensible JSON representation;
//! consumers can re-parse if they need a typed datetime, which Zed settings
//! don't).
//!
//! Mismatches from the schema (e.g. user wrote `font_size = "14"` where Zed
//! expects an integer) are *not* fixed up here — the downstream
//! `serde_json::from_value::<SettingsContent>` call surfaces those as a
//! settings-parse error with the precise key path, which is the right place
//! for the user-visible diagnostic.

use serde_json::{Map, Value, json};

/// Translate a parsed TOML value into the equivalent JSON value. Pure
/// structural mapping — no schema validation.
pub fn translate(toml_value: &toml::Value) -> Value {
    match toml_value {
        toml::Value::String(s) => Value::String(s.clone()),
        toml::Value::Integer(i) => json!(*i),
        toml::Value::Float(f) => json!(*f),
        toml::Value::Boolean(b) => Value::Bool(*b),
        toml::Value::Datetime(dt) => Value::String(dt.to_string()),
        toml::Value::Array(arr) => Value::Array(arr.iter().map(translate).collect()),
        toml::Value::Table(table) => {
            let mut obj = Map::with_capacity(table.len());
            for (key, value) in table {
                obj.insert(key.clone(), translate(value));
            }
            Value::Object(obj)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> toml::Value {
        src.parse::<toml::Value>().expect("test fixture parses")
    }

    #[test]
    fn primitives_translate() {
        let toml_doc = parse(
            "
            s = \"hello\"
            i = 42
            f = 3.14
            b = true
        ",
        );
        let json = translate(&toml_doc);
        assert_eq!(json["s"], json!("hello"));
        assert_eq!(json["i"], json!(42));
        assert_eq!(json["f"], json!(3.14));
        assert_eq!(json["b"], json!(true));
    }

    #[test]
    fn nested_tables_become_nested_objects() {
        let toml_doc = parse(
            "
            [theme]
            mode = \"system\"
            [theme.colors]
            background = \"#000\"
        ",
        );
        let json = translate(&toml_doc);
        assert_eq!(json["theme"]["mode"], json!("system"));
        assert_eq!(json["theme"]["colors"]["background"], json!("#000"));
    }

    #[test]
    fn arrays_translate_recursively() {
        let toml_doc = parse(
            "
            scopes = [\"a\", \"b\"]
            [[keymap]]
            context = \"Editor\"
            [[keymap]]
            context = \"Terminal\"
        ",
        );
        let json = translate(&toml_doc);
        assert_eq!(json["scopes"], json!(["a", "b"]));
        assert_eq!(json["keymap"][0]["context"], json!("Editor"));
        assert_eq!(json["keymap"][1]["context"], json!("Terminal"));
    }

    #[test]
    fn quoted_keys_preserved() {
        let toml_doc = parse(
            "
            [languages.\"c++\"]
            formatter = \"clang-format\"
        ",
        );
        let json = translate(&toml_doc);
        assert_eq!(json["languages"]["c++"]["formatter"], json!("clang-format"));
    }
}

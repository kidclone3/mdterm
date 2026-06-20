use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

#[derive(Deserialize)]
pub struct Config {
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default)]
    pub line_numbers: bool,
    #[serde(default)]
    pub width: usize,
    #[serde(default)]
    #[allow(dead_code)]
    pub pos: PosConfig,
}

#[derive(Deserialize, Clone, Debug)]
pub struct PosConfig {
    #[serde(default)]
    #[allow(dead_code)]
    pub enabled: bool,
    #[serde(default)]
    #[allow(dead_code)]
    pub categories: Option<Vec<String>>,
}

impl Default for PosConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            categories: None,
        }
    }
}

fn default_theme() -> String {
    "dark".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            line_numbers: false,
            width: 0,
            pos: PosConfig::default(),
        }
    }
}

impl Config {
    pub fn load() -> Self {
        if let Some(path) = config_path()
            && let Ok(contents) = fs::read_to_string(&path)
            && let Ok(config) = toml::from_str(&contents)
        {
            return config;
        }
        Config::default()
    }
}

fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("mdterm").join("config.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_pos_is_disabled_and_all_categories() {
        let c = Config::default();
        assert!(!c.pos.enabled);
        assert!(c.pos.categories.is_none());
    }

    #[test]
    fn parse_pos_enabled_with_categories() {
        let toml = r#"
[pos]
enabled = true
categories = ["noun", "verb"]
"#;
        let c: Config = toml::from_str(toml).unwrap();
        assert!(c.pos.enabled);
        assert_eq!(
            c.pos.categories.as_deref(),
            Some(&["noun".to_string(), "verb".to_string()][..])
        );
    }

    #[test]
    fn parse_pos_enabled_only_defaults_categories_none() {
        let toml = "[pos]\nenabled = true\n";
        let c: Config = toml::from_str(toml).unwrap();
        assert!(c.pos.enabled);
        assert!(c.pos.categories.is_none());
    }
}

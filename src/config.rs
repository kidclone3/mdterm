use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

fn deserialize_categories<'de, D>(d: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    enum Cat {
        List(Vec<String>),
        Single(String),
    }
    match Option::deserialize(d)? {
        Some(Cat::List(v)) => Ok(Some(v)),
        Some(Cat::Single(s)) => Ok(Some(vec![s])),
        None => Ok(None),
    }
}

#[derive(Deserialize)]
pub struct Config {
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default)]
    pub line_numbers: bool,
    #[serde(default)]
    pub width: usize,
    #[serde(default = "default_mermaid_render")]
    pub mermaid_render: String,
    #[serde(default)]
    #[allow(dead_code)]
    pub pos: PosConfig,
}

#[derive(Deserialize, Clone, Debug)]
pub struct PosConfig {
    #[serde(default = "default_pos_enabled")]
    #[allow(dead_code)]
    pub enabled: bool,
    #[serde(default, deserialize_with = "deserialize_categories")]
    #[allow(dead_code)]
    pub categories: Option<Vec<String>>,
}

fn default_theme() -> String {
    "dark".to_string()
}

fn default_mermaid_render() -> String {
    "auto".to_string()
}

fn default_pos_enabled() -> bool {
    true
}

impl Default for PosConfig {
    fn default() -> Self {
        Self {
            enabled: default_pos_enabled(),
            categories: None,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            line_numbers: false,
            width: 0,
            mermaid_render: default_mermaid_render(),
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
    fn default_pos_is_enabled_and_all_categories() {
        let c = Config::default();
        assert!(c.pos.enabled);
        assert!(c.pos.categories.is_none());
    }

    #[test]
    fn default_mermaid_render_is_auto() {
        let c = Config::default();
        assert_eq!(c.mermaid_render, "auto");
    }

    #[test]
    fn parse_mermaid_render_config() {
        let c: Config = toml::from_str("mermaid_render = \"ascii\"\n").unwrap();
        assert_eq!(c.mermaid_render, "ascii");
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

    #[test]
    fn parse_pos_scalar_categories_does_not_destroy_config() {
        let toml =
            "theme = \"light\"\nline_numbers = true\n[pos]\nenabled = true\ncategories = \"all\"\n";
        let c: Config = toml::from_str(toml).unwrap();
        assert!(c.pos.enabled);
        assert_eq!(c.pos.categories, Some(vec!["all".to_string()]));
        // Rest of config survives (the silent-config-loss bug is fixed).
        assert_eq!(c.theme, "light");
        assert!(c.line_numbers);
    }

    #[test]
    fn parse_pos_scalar_single_category() {
        let toml = "[pos]\ncategories = \"noun\"\n";
        let c: Config = toml::from_str(toml).unwrap();
        assert!(c.pos.enabled);
        assert_eq!(c.pos.categories, Some(vec!["noun".to_string()]));
    }
}

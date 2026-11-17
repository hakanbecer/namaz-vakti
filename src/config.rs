//! Yapılandırma modülü (config.json)

#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Uygulama yapılandırma ayarları
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Config {
    pub city: String,
    pub country: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            city: "Malatya".to_string(),
            country: "Turkey".to_string(),
        }
    }
}

impl Config {
    pub fn new(city: impl Into<String>, country: impl Into<String>) -> Self {
        Self {
            city: city.into(),
            country: country.into(),
        }
    }

    /// Verilen yoldan config.json'ı okur; dosya yoksa varsayılan ayarlarla oluşturur.
    pub fn load_or_create(path: &Path) -> Result<Self, String> {
        if path.exists() {
            let content = std::fs::read_to_string(path)
                .map_err(|e| format!("Config okuma hatası: {}", e))?;
            let cfg: Config = serde_json::from_str(&content)
                .map_err(|e| format!("Bozuk config.json formatı: {}", e))?;
            Ok(cfg)
        } else {
            let default_cfg = Config::default();
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let serialized = serde_json::to_string_pretty(&default_cfg)
                .map_err(|e| format!("Config serileştirme hatası: {}", e))?;
            std::fs::write(path, serialized)
                .map_err(|e| format!("Config yazma hatası: {}", e))?;
            Ok(default_cfg)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let cfg = Config::default();
        assert_eq!(cfg.city, "Malatya");
        assert_eq!(cfg.country, "Turkey");
    }

    #[test]
    fn test_config_load_or_create() {
        let temp_dir = std::env::temp_dir().join("namaz_test_config");
        let _ = std::fs::create_dir_all(&temp_dir);
        let config_path = temp_dir.join("test_config.json");
        let _ = std::fs::remove_file(&config_path);

        // 1) Dosya yokken varsayılan oluşturulmalı
        let cfg1 = Config::load_or_create(&config_path).unwrap();
        assert_eq!(cfg1.city, "Malatya");
        assert!(config_path.exists());

        // 2) Dosya varken okunmalı
        let cfg2 = Config::load_or_create(&config_path).unwrap();
        assert_eq!(cfg2, cfg1);

        // 3) Bozuk JSON testi
        std::fs::write(&config_path, "invalid json content").unwrap();
        let err = Config::load_or_create(&config_path).unwrap_err();
        assert!(err.contains("Bozuk config.json"));

        let _ = std::fs::remove_file(&config_path);
        let _ = std::fs::remove_dir(&temp_dir);
    }
}

//! Hata ve durum günlüğü (logging) modülü.
//! Dosya boyutu en fazla 100 KB ile sınırlandırılmıştır.
//! Hiçbir unwrap/expect içermez; loglama hataları uygulamayı asla çökertmez.

#![allow(dead_code)]

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use crate::data::LocalDate;
use crate::time_logic::TimeOfDay;

/// Günlük dosyasının maksimum boyutu (100 KB = 102,400 bayt)
pub const MAX_LOG_SIZE: u64 = 100 * 1024;

/// Boyut aşıldığında korunacak son log verisi miktarı (~50 KB)
pub const RETAIN_LOG_SIZE: usize = 50 * 1024;

/// %APPDATA%\namaz-widget\app.log yolunu döner.
pub fn get_log_path() -> Result<PathBuf, String> {
    let appdata = std::env::var("APPDATA")
        .map_err(|_| "APPDATA ortam değişkeni bulunamadı".to_string())?;
    let dir = PathBuf::from(appdata).join("namaz-widget");
    let _ = std::fs::create_dir_all(&dir);
    Ok(dir.join("app.log"))
}

/// Bilgi günlüğü yazar
pub fn log_info(msg: &str) {
    write_log_entry("INFO", msg);
}

/// Hata günlüğü yazar
pub fn log_error(msg: &str) {
    write_log_entry("ERROR", msg);
}

/// Boyutu 100 KB'ı aşmayacak şekilde dosyaya log yazar.
fn write_log_entry(level: &str, msg: &str) {
    let path = match get_log_path() {
        Ok(p) => p,
        Err(_) => return,
    };

    let date = LocalDate::now();
    let time = TimeOfDay::now();
    let entry = format!(
        "[{:04}-{:02}-{:02} {:02}:{:02}:{:02}] [{}] {}\n",
        date.year, date.month, date.day, time.hour, time.minute, time.second, level, msg
    );

    // Dosya boyutunu kontrol et ve gerekiyorsa budama yap
    if let Ok(metadata) = std::fs::metadata(&path) {
        if metadata.len() + (entry.len() as u64) > MAX_LOG_SIZE {
            truncate_log_file(&path);
        }
    }

    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = file.write_all(entry.as_bytes());
    }
}

/// Log dosyası 100 KB'ı aştığında son 50 KB'ı koruyup baş tarafı budar.
fn truncate_log_file(path: &PathBuf) {
    if let Ok(content) = std::fs::read(path) {
        if content.len() > RETAIN_LOG_SIZE {
            let start = content.len() - RETAIN_LOG_SIZE;
            // İlk tam satır başını bul
            let slice = &content[start..];
            let clean_start = slice
                .iter()
                .position(|&b| b == b'\n')
                .map(|p| p + 1)
                .unwrap_or(0);
            let retained = &slice[clean_start..];
            let _ = std::fs::write(path, retained);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_file_size_limit() {
        let temp_dir = std::env::temp_dir().join("namaz_test_logger");
        let _ = std::fs::create_dir_all(&temp_dir);
        let log_file = temp_dir.join("test.log");
        let _ = std::fs::remove_file(&log_file);

        // 120 KB veri yaz
        let big_data = vec![b'A'; 120 * 1024];
        std::fs::write(&log_file, big_data).unwrap();

        // Budama fonksiyonunu çalıştır
        truncate_log_file(&log_file);

        let new_len = std::fs::metadata(&log_file).unwrap().len();
        assert!(new_len <= MAX_LOG_SIZE);
        assert!(new_len <= (RETAIN_LOG_SIZE as u64) + 100);

        let _ = std::fs::remove_file(&log_file);
        let _ = std::fs::remove_dir(&temp_dir);
    }

    #[test]
    fn test_log_error_and_info_no_panic() {
        // Bu çağrılar hata fırlatmamalı veya panic üretmemeli
        log_info("Test bilgi mesaji");
        log_error("Test hata mesaji");
    }
}

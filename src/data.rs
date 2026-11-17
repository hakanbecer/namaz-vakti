//! API veri çekme (Aladhan), yerel önbellekleme (cache.json) ve yapılandırma modülü.
//! Tokio / async runtime içermez; senkron HTTP (ureq) ve serde_json kullanır.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::time_logic::{DayTimes, TimeOfDay};

/// Aladhan API takvim uç noktası
pub const ALADHAN_BASE_URL: &str = "https://api.aladhan.com/v1/calendarByCity";

/// Aladhan API hesaplama yöntemi:
/// /v1/methods çıktısında:
/// TURKEY: { id: 13, name: "Diyanet İşleri Başkanlığı, Turkey (experimental)" }
pub const METHOD_DIYANET: u32 = 13;

/// Veri modülü hata türleri
#[derive(Debug)]
pub enum DataError {
    Io(std::io::Error),
    Json(String),
    Network(String),
    NoDataForDate(String),
    NoNetworkAndNoCache(String),
    Config(String),
}

impl fmt::Display for DataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DataError::Io(e) => write!(f, "Dosya G/Ç hatası: {}", e),
            DataError::Json(msg) => write!(f, "JSON hatası: {}", msg),
            DataError::Network(msg) => write!(f, "Ağ hatası: {}", msg),
            DataError::NoDataForDate(date) => write!(f, "Bu tarih için vakit verisi bulunamadı: {}", date),
            DataError::NoNetworkAndNoCache(msg) => write!(f, "Ağ yok ve önbellek bulunamadı: {}", msg),
            DataError::Config(msg) => write!(f, "Yapılandırma hatası: {}", msg),
        }
    }
}

impl std::error::Error for DataError {}

impl From<std::io::Error> for DataError {
    fn from(err: std::io::Error) -> Self {
        DataError::Io(err)
    }
}

impl From<serde_json::Error> for DataError {
    fn from(err: serde_json::Error) -> Self {
        DataError::Json(err.to_string())
    }
}

/// Artık yıl kontrolü (Gregoryen takvimi)
pub fn is_leap_year(year: u32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

/// Verilen ayın kaç gün çektiğini döner.
pub fn days_in_month(year: u32, month: u8) -> u8 {
    match month {
        1 => 31,
        2 => {
            if is_leap_year(year) {
                29
            } else {
                28
            }
        }
        3 => 31,
        4 => 30,
        5 => 31,
        6 => 30,
        7 => 31,
        8 => 31,
        9 => 30,
        10 => 31,
        11 => 30,
        12 => 31,
        _ => 30,
    }
}

/// Bir sonraki takvim gününü döner (yıl, ay, gün).
pub fn next_day(year: u32, month: u8, day: u8) -> (u32, u8, u8) {
    let dim = days_in_month(year, month);
    if day < dim {
        (year, month, day + 1)
    } else if month < 12 {
        (year, month + 1, 1)
    } else {
        (year + 1, 1, 1)
    }
}

/// Bir sonraki ayı döner (yıl, ay).
pub fn next_month(year: u32, month: u8) -> (u32, u8) {
    if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    }
}

/// Yerel takvim tarihi (chrono'suz)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LocalDate {
    pub year: u32,
    pub month: u8,
    pub day: u8,
}

impl LocalDate {
    pub const fn new(year: u32, month: u8, day: u8) -> Self {
        Self { year, month, day }
    }

    /// Windows Win32 API (GetLocalTime) ile anlık yerel tarihi alır.
    pub fn now() -> Self {
        #[repr(C)]
        struct SystemTime {
            year: u16,
            month: u16,
            day_of_week: u16,
            day: u16,
            hour: u16,
            minute: u16,
            second: u16,
            milliseconds: u16,
        }
        unsafe extern "system" {
            fn GetLocalTime(st: *mut SystemTime);
        }
        let mut st = SystemTime {
            year: 0,
            month: 0,
            day_of_week: 0,
            day: 0,
            hour: 0,
            minute: 0,
            second: 0,
            milliseconds: 0,
        };
        unsafe {
            GetLocalTime(&mut st);
        }
        LocalDate {
            year: st.year as u32,
            month: st.month as u8,
            day: st.day as u8,
        }
    }

    /// "YYYY-MM-DD" biçiminde ISO 8601 dizgisi döner.
    pub fn to_iso_string(self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }

    /// Bir sonraki günü hesaplar.
    pub fn next_day(self) -> Self {
        let (y, m, d) = next_day(self.year, self.month, self.day);
        Self {
            year: y,
            month: m,
            day: d,
        }
    }
}

/// Aladhan API yanıtı için iç veri modelleri
#[derive(Debug, Deserialize)]
struct AladhanResponse {
    code: u16,
    status: String,
    data: Vec<AladhanDayEntry>,
}

#[derive(Debug, Deserialize)]
struct AladhanDayEntry {
    timings: AladhanTimings,
    date: AladhanDate,
}

#[derive(Debug, Deserialize)]
struct AladhanTimings {
    #[serde(rename = "Fajr")]
    fajr: String,
    #[serde(rename = "Sunrise")]
    sunrise: String,
    #[serde(rename = "Dhuhr")]
    dhuhr: String,
    #[serde(rename = "Asr")]
    asr: String,
    #[serde(rename = "Maghrib")]
    maghrib: String,
    #[serde(rename = "Isha")]
    isha: String,
}

#[derive(Debug, Deserialize)]
struct AladhanDate {
    gregorian: AladhanGregorian,
}

#[derive(Debug, Deserialize)]
struct AladhanGregorian {
    date: String, // "DD-MM-YYYY"
}

/// "05:23 (+03)" veya "05:23" biçimindeki saat dizgisini TimeOfDay'e çevirir.
pub fn parse_timing_str(s: &str) -> Result<TimeOfDay, DataError> {
    let clean = s
        .split_whitespace()
        .next()
        .ok_or_else(|| DataError::Json(format!("Geçersiz saat dizgisi: '{}'", s)))?;
    TimeOfDay::parse_hm(clean)
        .ok_or_else(|| DataError::Json(format!("Saat ayrıştırılamadı (HH:MM bekleniyordu): '{}'", s)))
}

/// "DD-MM-YYYY" formatını "YYYY-MM-DD" ISO formatına dönüştürür.
pub fn dd_mm_yyyy_to_iso(s: &str) -> Option<String> {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() == 3 {
        let day = parts[0].trim().parse::<u8>().ok()?;
        let month = parts[1].trim().parse::<u8>().ok()?;
        let year = parts[2].trim().parse::<u32>().ok()?;
        Some(format!("{:04}-{:02}-{:02}", year, month, day))
    } else {
        None
    }
}

/// Aladhan JSON takvim yanıtını ayrıştırır ve (YYYY-MM-DD -> DayTimes) eşlemesi döner.
pub fn parse_aladhan_response(json: &str) -> Result<BTreeMap<String, DayTimes>, DataError> {
    let resp: AladhanResponse = serde_json::from_str(json)
        .map_err(|e| DataError::Json(format!("Aladhan yanıtı ayrıştırılamadı: {}", e)))?;

    if resp.code != 200 && resp.status != "OK" {
        return Err(DataError::Network(format!(
            "Aladhan API hata kodu döndü: {} ({})",
            resp.code, resp.status
        )));
    }

    let mut result = BTreeMap::new();
    for entry in resp.data {
        let iso_date = dd_mm_yyyy_to_iso(&entry.date.gregorian.date).ok_or_else(|| {
            DataError::Json(format!(
                "Geçersiz tarih formatı: '{}'",
                entry.date.gregorian.date
            ))
        })?;

        // Diyanet metodunda (13):
        // Fajr = 18.0° = İmsak
        // Sunrise = Güneş
        // Dhuhr = Öğle
        // Asr = İkindi
        // Maghrib = Akşam
        // Isha = Yatsı (17.0°)
        let day_times = DayTimes {
            imsak: parse_timing_str(&entry.timings.fajr)?,
            gunes: parse_timing_str(&entry.timings.sunrise)?,
            ogle: parse_timing_str(&entry.timings.dhuhr)?,
            ikindi: parse_timing_str(&entry.timings.asr)?,
            aksam: parse_timing_str(&entry.timings.maghrib)?,
            yatsi: parse_timing_str(&entry.timings.isha)?,
        };

        result.insert(iso_date, day_times);
    }

    Ok(result)
}

/// Ağ soyutlama trait'i (Birim testlerinde mock kullanımı için)
pub trait PrayerApiClient: Send + Sync {
    fn fetch_calendar(
        &self,
        city: &str,
        country: &str,
        year: u32,
        month: u8,
    ) -> Result<String, DataError>;
}

/// Gerçek Aladhan HTTP istemcisi (ureq, 5 sn timeout)
#[derive(Debug, Clone)]
pub struct AladhanClient {
    timeout: Duration,
}

impl Default for AladhanClient {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(5),
        }
    }
}

impl AladhanClient {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_timeout(timeout: Duration) -> Self {
        Self { timeout }
    }
}

impl PrayerApiClient for AladhanClient {
    fn fetch_calendar(
        &self,
        city: &str,
        country: &str,
        year: u32,
        month: u8,
    ) -> Result<String, DataError> {
        let url = format!(
            "{}?city={}&country={}&method={}&month={}&year={}",
            ALADHAN_BASE_URL, city, country, METHOD_DIYANET, month, year
        );

        let resp = ureq::get(&url)
            .timeout(self.timeout)
            .call()
            .map_err(|e| DataError::Network(format!("Ağ isteği başarısız (URL: {}): {}", url, e)))?;

        resp.into_string()
            .map_err(|e| DataError::Network(format!("Yanıt metni okunamadı: {}", e)))
    }
}

/// Yerel disk önbelleği (cache.json) veri yapısı
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CacheData {
    pub city: String,
    pub country: String,
    /// "YYYY-MM-DD" -> DayTimes
    pub days: BTreeMap<String, DayTimes>,
}

impl CacheData {
    pub fn new(city: impl Into<String>, country: impl Into<String>) -> Self {
        Self {
            city: city.into(),
            country: country.into(),
            days: BTreeMap::new(),
        }
    }

    pub fn load_from_file(path: &Path) -> Result<Self, DataError> {
        let content = std::fs::read_to_string(path)?;
        let data: CacheData = serde_json::from_str(&content)
            .map_err(|e| DataError::Json(format!("Bozuk cache.json: {}", e)))?;
        Ok(data)
    }

    pub fn save_to_file(&self, path: &Path) -> Result<(), DataError> {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    pub fn has_date(&self, date_str: &str) -> bool {
        self.days.contains_key(date_str)
    }

    pub fn get_day(&self, date_str: &str) -> Option<DayTimes> {
        self.days.get(date_str).copied()
    }

    pub fn merge_days(&mut self, new_days: impl IntoIterator<Item = (String, DayTimes)>) {
        self.days.extend(new_days);
    }
}

/// %APPDATA%\namaz-widget dizinini döner.
pub fn get_appdata_dir() -> Result<PathBuf, DataError> {
    let appdata = std::env::var("APPDATA")
        .map_err(|_| DataError::Config("APPDATA ortam değişkeni bulunamadı".to_string()))?;
    Ok(PathBuf::from(appdata).join("namaz-widget"))
}

/// %APPDATA%\namaz-widget\cache.json yolunu döner.
pub fn get_cache_path() -> Result<PathBuf, DataError> {
    Ok(get_appdata_dir()?.join("cache.json"))
}

/// %APPDATA%\namaz-widget\config.json yolunu döner.
pub fn get_config_path() -> Result<PathBuf, DataError> {
    Ok(get_appdata_dir()?.join("config.json"))
}

/// Veri yönetim yöneticisi:
/// - Önbellek geçerliyse ağa gitmez.
/// - Ay bitimine az kala (gün >= 25) sonraki ayı da önbelleğe çeker.
/// - Ağ yoksa önbellekten döner, önbellek de yoksa anlamlı hata fırlatır (panic yok).
pub struct DataManager<C: PrayerApiClient = AladhanClient> {
    pub client: C,
    pub cache_path: PathBuf,
    pub config_path: PathBuf,
}

impl DataManager<AladhanClient> {
    /// Varsayılan Aladhan istemcisi ve APPDATA dosya yolları ile oluşturur.
    pub fn new_default() -> Result<Self, DataError> {
        Ok(Self {
            client: AladhanClient::new(),
            cache_path: get_cache_path()?,
            config_path: get_config_path()?,
        })
    }
}

impl<C: PrayerApiClient> DataManager<C> {
    pub fn with_paths(client: C, cache_path: PathBuf, config_path: PathBuf) -> Self {
        Self {
            client,
            cache_path,
            config_path,
        }
    }

    /// Verilen tarih için bugün ve yarının vakitlerini getirir.
    /// Önbellek geçerliyse doğrudan önbellekten döner (ağa gitmez).
    pub fn get_or_fetch_today_and_tomorrow(
        &mut self,
        today: &LocalDate,
    ) -> Result<(DayTimes, DayTimes), DataError> {
        let config = Config::load_or_create(&self.config_path)
            .map_err(DataError::Config)?;

        let today_str = today.to_iso_string();
        let tomorrow = today.next_day();
        let tomorrow_str = tomorrow.to_iso_string();

        let mut cache = CacheData::load_from_file(&self.cache_path).unwrap_or_else(|_| {
            CacheData::new(&config.city, &config.country)
        });

        // Şehir veya ülke değişmişse eski önbellek geçersiz sayılır
        let city_match = cache.city.eq_ignore_ascii_case(&config.city)
            && cache.country.eq_ignore_ascii_case(&config.country);

        let cache_valid = city_match
            && cache.has_date(&today_str)
            && cache.has_date(&tomorrow_str);

        if cache_valid {
            // Açılışta cache geçerliyse ağa gitme!
            let dt_today = cache.get_day(&today_str).ok_or_else(|| {
                DataError::NoDataForDate(format!("Bugün ({}) verisi bulunamadı", today_str))
            })?;
            let dt_tomorrow = cache.get_day(&tomorrow_str).ok_or_else(|| {
                DataError::NoDataForDate(format!("Yarın ({}) verisi bulunamadı", tomorrow_str))
            })?;

            // Ay bitmeden (örn. ayın 25'i ve sonrası) sonraki ayı da önbelleğe çek
            if today.day >= 25 {
                let (next_y, next_m) = next_month(today.year, today.month);
                let check_date = format!("{:04}-{:02}-01", next_y, next_m);
                if !cache.has_date(&check_date) {
                    // Sonraki ayı çekmeyi dene; ağ hatası olursa mevcut günü bozma
                    if let Ok(json) = self.client.fetch_calendar(
                        &config.city,
                        &config.country,
                        next_y,
                        next_m,
                    ) {
                        if let Ok(next_month_days) = parse_aladhan_response(&json) {
                            cache.merge_days(next_month_days);
                            let _ = cache.save_to_file(&self.cache_path);
                        }
                    }
                }
            }

            return Ok((dt_today, dt_tomorrow));
        }

        // Önbellek eksik veya geçersiz -> Ağa git
        let net_res = self.client.fetch_calendar(
            &config.city,
            &config.country,
            today.year,
            today.month,
        );

        match net_res {
            Ok(json_data) => {
                let month_days = parse_aladhan_response(&json_data)?;
                cache.city = config.city.clone();
                cache.country = config.country.clone();
                cache.merge_days(month_days);

                // Ay sonundaysak ve yarın sonraki aya taşıyorsa veya ayın son günleriysek:
                if today.day >= 25 || tomorrow.month != today.month {
                    let (next_y, next_m) = next_month(today.year, today.month);
                    if let Ok(next_json) = self.client.fetch_calendar(
                        &config.city,
                        &config.country,
                        next_y,
                        next_m,
                    ) {
                        if let Ok(next_days) = parse_aladhan_response(&next_json) {
                            cache.merge_days(next_days);
                        }
                    }
                }

                // Önbelleğe kaydet
                let _ = cache.save_to_file(&self.cache_path);

                let dt_today = cache.get_day(&today_str).ok_or_else(|| {
                    DataError::NoDataForDate(format!("Bugün ({}) verisi bulunamadı", today_str))
                })?;
                let dt_tomorrow = cache.get_day(&tomorrow_str).ok_or_else(|| {
                    DataError::NoDataForDate(format!("Yarın ({}) verisi bulunamadı", tomorrow_str))
                })?;

                Ok((dt_today, dt_tomorrow))
            }
            Err(net_err) => {
                // İnternet yoksa: cache'ten çalışmayı dene
                if let (Some(dt_today), Some(dt_tomorrow)) =
                    (cache.get_day(&today_str), cache.get_day(&tomorrow_str))
                {
                    Ok((dt_today, dt_tomorrow))
                } else {
                    // Cache de yoksa anlamlı hata dön (panic yok)
                    Err(DataError::NoNetworkAndNoCache(format!(
                        "Ağ çağrısı başarısız oldu ({}) ve yerel önbellekte [{}, {}] bulunamadı",
                        net_err, today_str, tomorrow_str
                    )))
                }
            }
        }
    }
}

/// Ağ çağrısını UI thread'inde yapmadan arka planda başlatan yardımcı fonksiyon.
/// Win32 mesaj döngüsünde `Receiver::try_recv()` ile non-blocking kontrol edilir.
pub fn start_background_fetch<C: PrayerApiClient + Clone + 'static>(
    client: C,
    config: Config,
    year: u32,
    month: u8,
) -> mpsc::Receiver<Result<BTreeMap<String, DayTimes>, DataError>> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let result = (|| {
            let json_str = client.fetch_calendar(&config.city, &config.country, year, month)?;
            parse_aladhan_response(&json_str)
        })();
        let _ = tx.send(result);
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Testler için örnek Aladhan JSON yanıtı
    const SAMPLE_ALADHAN_JSON: &str = r#"{
        "code": 200,
        "status": "OK",
        "data": [
            {
                "timings": {
                    "Fajr": "05:23 (+03)",
                    "Sunrise": "06:46 (+03)",
                    "Dhuhr": "12:15 (+03)",
                    "Asr": "15:09 (+03)",
                    "Sunset": "17:34 (+03)",
                    "Maghrib": "17:34 (+03)",
                    "Isha": "18:52 (+03)",
                    "Imsak": "05:13 (+03)",
                    "Midnight": "00:10 (+03)"
                },
                "date": {
                    "readable": "01 Nov 2026",
                    "gregorian": {
                        "date": "01-11-2026"
                    }
                }
            },
            {
                "timings": {
                    "Fajr": "05:24 (+03)",
                    "Sunrise": "06:47 (+03)",
                    "Dhuhr": "12:15 (+03)",
                    "Asr": "15:08 (+03)",
                    "Sunset": "17:33 (+03)",
                    "Maghrib": "17:33 (+03)",
                    "Isha": "18:51 (+03)",
                    "Imsak": "05:14 (+03)",
                    "Midnight": "00:10 (+03)"
                },
                "date": {
                    "readable": "02 Nov 2026",
                    "gregorian": {
                        "date": "02-11-2026"
                    }
                }
            }
        ]
    }"#;

    /// Mock API İstemcisi
    #[derive(Clone)]
    struct MockApiClient {
        pub response: Result<String, String>,
        pub call_count: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    }

    impl MockApiClient {
        fn success(json: &str) -> Self {
            Self {
                response: Ok(json.to_string()),
                call_count: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            }
        }

        fn failure(err: &str) -> Self {
            Self {
                response: Err(err.to_string()),
                call_count: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            }
        }
    }

    impl PrayerApiClient for MockApiClient {
        fn fetch_calendar(
            &self,
            _city: &str,
            _country: &str,
            _year: u32,
            _month: u8,
        ) -> Result<String, DataError> {
            self.call_count
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            match &self.response {
                Ok(data) => Ok(data.clone()),
                Err(e) => Err(DataError::Network(e.clone())),
            }
        }
    }

    #[test]
    fn test_cache_read_write() {
        let temp_dir = std::env::temp_dir().join("namaz_test_cache_rw");
        let _ = std::fs::create_dir_all(&temp_dir);
        let cache_file = temp_dir.join("test_cache.json");
        let _ = std::fs::remove_file(&cache_file);

        let mut cache = CacheData::new("Malatya", "Turkey");
        let sample_dt = DayTimes {
            imsak: TimeOfDay::from_hm(5, 23),
            gunes: TimeOfDay::from_hm(6, 46),
            ogle: TimeOfDay::from_hm(12, 15),
            ikindi: TimeOfDay::from_hm(15, 9),
            aksam: TimeOfDay::from_hm(17, 34),
            yatsi: TimeOfDay::from_hm(18, 52),
        };
        cache.days.insert("2026-11-01".to_string(), sample_dt);

        // Yaz
        cache.save_to_file(&cache_file).unwrap();
        assert!(cache_file.exists());

        // Oku
        let loaded = CacheData::load_from_file(&cache_file).unwrap();
        assert_eq!(loaded.city, "Malatya");
        assert_eq!(loaded.country, "Turkey");
        assert_eq!(loaded.days.get("2026-11-01"), Some(&sample_dt));

        let _ = std::fs::remove_file(&cache_file);
        let _ = std::fs::remove_dir(&temp_dir);
    }

    #[test]
    fn test_corrupted_json() {
        let temp_dir = std::env::temp_dir().join("namaz_test_corrupt");
        let _ = std::fs::create_dir_all(&temp_dir);
        let cache_file = temp_dir.join("corrupt_cache.json");

        // Bozuk cache dosyası
        std::fs::write(&cache_file, "{ bozuk json verisi ...").unwrap();
        let res = CacheData::load_from_file(&cache_file);
        assert!(res.is_err());
        match res.unwrap_err() {
            DataError::Json(msg) => assert!(msg.contains("Bozuk cache.json")),
            other => panic!("Beklenmeyen hata türü: {:?}", other),
        }

        // Bozuk Aladhan yanıtı
        let res_api = parse_aladhan_response("{ non-json ");
        assert!(res_api.is_err());
        match res_api.unwrap_err() {
            DataError::Json(_) => {}
            other => panic!("Beklenmeyen hata türü: {:?}", other),
        }

        let _ = std::fs::remove_file(&cache_file);
        let _ = std::fs::remove_dir(&temp_dir);
    }

    #[test]
    fn test_network_down_with_valid_cache() {
        let temp_dir = std::env::temp_dir().join("namaz_test_net_down_cache");
        let _ = std::fs::create_dir_all(&temp_dir);
        let cache_file = temp_dir.join("cache.json");
        let config_file = temp_dir.join("config.json");
        let _ = std::fs::remove_file(&cache_file);
        let _ = std::fs::remove_file(&config_file);

        // Config yaz
        let config = Config::new("Malatya", "Turkey");
        std::fs::write(&config_file, serde_json::to_string(&config).unwrap()).unwrap();

        // Önbelleğe bugünü ve yarını yaz
        let mut cache = CacheData::new("Malatya", "Turkey");
        let dt1 = DayTimes {
            imsak: TimeOfDay::from_hm(5, 23),
            gunes: TimeOfDay::from_hm(6, 46),
            ogle: TimeOfDay::from_hm(12, 15),
            ikindi: TimeOfDay::from_hm(15, 9),
            aksam: TimeOfDay::from_hm(17, 34),
            yatsi: TimeOfDay::from_hm(18, 52),
        };
        let dt2 = DayTimes {
            imsak: TimeOfDay::from_hm(5, 24),
            gunes: TimeOfDay::from_hm(6, 47),
            ogle: TimeOfDay::from_hm(12, 15),
            ikindi: TimeOfDay::from_hm(15, 8),
            aksam: TimeOfDay::from_hm(17, 33),
            yatsi: TimeOfDay::from_hm(18, 51),
        };
        cache.days.insert("2026-11-01".to_string(), dt1);
        cache.days.insert("2026-11-02".to_string(), dt2);
        cache.save_to_file(&cache_file).unwrap();

        // Mock: Ağ kapalı (hata döner)
        let mock_client = MockApiClient::failure("İnternet bağlantısı yok");
        let mut manager = DataManager::with_paths(mock_client.clone(), cache_file.clone(), config_file.clone());

        // Bugün: 2026-11-01
        let today = LocalDate::new(2026, 11, 1);
        let res = manager.get_or_fetch_today_and_tomorrow(&today);

        // Ağ kapalı olmasına rağmen önbellek geçerli olduğu için başarılı olmalı!
        assert!(res.is_ok());
        let (today_times, tomorrow_times) = res.unwrap();
        assert_eq!(today_times, dt1);
        assert_eq!(tomorrow_times, dt2);

        // Önbellek geçerli olduğu için ağ çağrısı dahi yapılmamalı!
        assert_eq!(mock_client.call_count.load(std::sync::atomic::Ordering::SeqCst), 0);

        let _ = std::fs::remove_file(&cache_file);
        let _ = std::fs::remove_file(&config_file);
        let _ = std::fs::remove_dir(&temp_dir);
    }

    #[test]
    fn test_network_down_without_cache() {
        let temp_dir = std::env::temp_dir().join("namaz_test_net_down_no_cache");
        let _ = std::fs::create_dir_all(&temp_dir);
        let cache_file = temp_dir.join("non_existent_cache.json");
        let config_file = temp_dir.join("config.json");
        let _ = std::fs::remove_file(&cache_file);
        let _ = std::fs::remove_file(&config_file);

        let mock_client = MockApiClient::failure("DNS çözümleme başarısız");
        let mut manager = DataManager::with_paths(mock_client, cache_file, config_file);

        let today = LocalDate::new(2026, 11, 1);
        let res = manager.get_or_fetch_today_and_tomorrow(&today);

        // Ağ yok ve cache yok -> panic olmaksızın anlamlı hata dönmeli
        assert!(res.is_err());
        match res.unwrap_err() {
            DataError::NoNetworkAndNoCache(msg) => {
                assert!(msg.contains("Ağ çağrısı başarısız oldu"));
                assert!(msg.contains("2026-11-01"));
            }
            other => panic!("Beklenmeyen hata türü: {:?}", other),
        }

        let _ = std::fs::remove_dir(&temp_dir);
    }

    #[test]
    fn test_successful_fetch_and_cache_update() {
        let temp_dir = std::env::temp_dir().join("namaz_test_fetch_cache");
        let _ = std::fs::create_dir_all(&temp_dir);
        let cache_file = temp_dir.join("cache.json");
        let config_file = temp_dir.join("config.json");
        let _ = std::fs::remove_file(&cache_file);
        let _ = std::fs::remove_file(&config_file);

        let mock_client = MockApiClient::success(SAMPLE_ALADHAN_JSON);
        let mut manager = DataManager::with_paths(mock_client.clone(), cache_file.clone(), config_file.clone());

        let today = LocalDate::new(2026, 11, 1);
        let res = manager.get_or_fetch_today_and_tomorrow(&today);
        assert!(res.is_ok());

        let (t_today, t_tomorrow) = res.unwrap();
        assert_eq!(t_today.imsak, TimeOfDay::from_hm(5, 23));
        assert_eq!(t_tomorrow.imsak, TimeOfDay::from_hm(5, 24));

        // Ağ çağrısı 1 kez yapılmış olmalı
        assert_eq!(mock_client.call_count.load(std::sync::atomic::Ordering::SeqCst), 1);
        // Önbellek dosyası oluşturulmuş olmalı
        assert!(cache_file.exists());

        // Tekrar çağrıldığında ağa gitmeden önbellekten almalı
        let res2 = manager.get_or_fetch_today_and_tomorrow(&today);
        assert!(res2.is_ok());
        assert_eq!(mock_client.call_count.load(std::sync::atomic::Ordering::SeqCst), 1);

        let _ = std::fs::remove_file(&cache_file);
        let _ = std::fs::remove_file(&config_file);
        let _ = std::fs::remove_dir(&temp_dir);
    }

    #[test]
    fn test_date_math_and_leap_year() {
        // Artık yıl kontrolleri
        assert!(is_leap_year(2024));
        assert!(!is_leap_year(2025));
        assert!(!is_leap_year(2100)); // 100'e bölünür ama 400'e bölünmez
        assert!(is_leap_year(2000)); // 400'e bölünür

        // Şubat gün sayıları
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(2025, 2), 28);

        // Ay sonu geçişleri
        assert_eq!(next_day(2026, 11, 30), (2026, 12, 1));
        assert_eq!(next_day(2026, 12, 31), (2027, 1, 1));
        assert_eq!(next_day(2024, 2, 28), (2024, 2, 29));
        assert_eq!(next_day(2024, 2, 29), (2024, 3, 1));

        // Ay geçişi
        assert_eq!(next_month(2026, 11), (2026, 12));
        assert_eq!(next_month(2026, 12), (2027, 1));

        // ISO formatlama
        let d = LocalDate::new(2026, 5, 9);
        assert_eq!(d.to_iso_string(), "2026-05-09");
        assert_eq!(dd_mm_yyyy_to_iso("09-05-2026"), Some("2026-05-09".to_string()));
    }

    #[test]
    fn test_background_fetch_thread() {
        let mock_client = MockApiClient::success(SAMPLE_ALADHAN_JSON);
        let config = Config::new("Malatya", "Turkey");

        let rx = start_background_fetch(mock_client, config, 2026, 11);
        // thread sonucunu bekle
        let res = rx.recv_timeout(Duration::from_secs(2)).expect("Yanıt zaman aşımına uğradı");
        assert!(res.is_ok());
        let days = res.unwrap();
        assert_eq!(days.len(), 2);
        assert!(days.contains_key("2026-11-01"));
        assert!(days.contains_key("2026-11-02"));
    }
}

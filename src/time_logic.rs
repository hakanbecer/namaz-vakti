//! Zaman ve namaz vakti hesaplama/kalan süre saf mantık modülü.
//! UI, ağ veya chrono bağımlılığı içermez.

#![allow(dead_code)]

use std::time::Duration;
use serde::{Deserialize, Serialize};

/// Uyarı eşiği (dakika ve Duration)
pub const UYARI_ESIK_DK: u64 = 30;
pub const UYARI_ESIK: Duration = Duration::from_secs(UYARI_ESIK_DK * 60);

/// Kritik eşik (dakika ve Duration)
pub const KRITIK_ESIK_DK: u64 = 15;
pub const KRITIK_ESIK: Duration = Duration::from_secs(KRITIK_ESIK_DK * 60);

/// Günün 6 namaz vakti
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Vakit {
    Imsak,
    Gunes,
    Ogle,
    Ikindi,
    Aksam,
    Yatsi,
}

impl Vakit {
    /// Vaktin Türkçe adı
    pub const fn name_tr(&self) -> &'static str {
        match self {
            Vakit::Imsak => "İmsak",
            Vakit::Gunes => "Güneş",
            Vakit::Ogle => "Öğle",
            Vakit::Ikindi => "İkindi",
            Vakit::Aksam => "Akşam",
            Vakit::Yatsi => "Yatsı",
        }
    }
}

impl std::fmt::Display for Vakit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name_tr())
    }
}

/// Kalan süreye göre bildirim seviyesi
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Level {
    Normal,
    Uyari,
    Kritik,
}

impl Level {
    /// Seviyenin Türkçe adı
    pub const fn name_tr(&self) -> &'static str {
        match self {
            Level::Normal => "Normal",
            Level::Uyari => "Uyarı",
            Level::Kritik => "Kritik",
        }
    }
}

impl std::fmt::Display for Level {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name_tr())
    }
}

/// Gün içi saati (saat, dakika, saniye) chrono'suz saf struct olarak tutar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
pub struct TimeOfDay {
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

impl TimeOfDay {
    /// Saat, dakika ve saniye ile yeni bir TimeOfDay oluşturur.
    pub const fn new(hour: u8, minute: u8, second: u8) -> Self {
        Self { hour, minute, second }
    }

    /// Saat ve dakika ile oluşturur (saniye 0).
    pub const fn from_hm(hour: u8, minute: u8) -> Self {
        Self { hour, minute, second: 0 }
    }

    /// Windows Win32 API ile anlık yerel saati alır.
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
        TimeOfDay {
            hour: st.hour as u8,
            minute: st.minute as u8,
            second: st.second as u8,
        }
    }

    /// Gün başlangıcından (00:00:00) itibaren toplam saniye sayısı.
    pub const fn total_seconds(&self) -> u32 {
        self.hour as u32 * 3600 + self.minute as u32 * 60 + self.second as u32
    }

    /// Toplam saniyeden TimeOfDay üretir (24 saat modu).
    pub const fn from_total_seconds(secs: u32) -> Self {
        let secs = secs % (24 * 3600);
        let hour = (secs / 3600) as u8;
        let minute = ((secs % 3600) / 60) as u8;
        let second = (secs % 60) as u8;
        Self { hour, minute, second }
    }

    /// Bu andan hedef vakte kadar olan süreyi hesaplar.
    /// Hedef geçmişteyse (gece yarısını aşıyorsa) 24 saat ekler.
    pub fn duration_until(&self, target: &Self) -> Duration {
        let s1 = self.total_seconds();
        let s2 = target.total_seconds();
        if s2 >= s1 {
            Duration::from_secs((s2 - s1) as u64)
        } else {
            Duration::from_secs((24 * 3600 - s1 + s2) as u64)
        }
    }

    /// "HH:MM" formatındaki dizgiyi ayrıştırır.
    pub fn parse_hm(s: &str) -> Option<Self> {
        let mut parts = s.split(':');
        let h = parts.next()?.trim().parse::<u8>().ok()?;
        let m = parts.next()?.trim().parse::<u8>().ok()?;
        if h < 24 && m < 60 {
            Some(Self::from_hm(h, m))
        } else {
            None
        }
    }

    /// "HH:MM:SS" formatındaki dizgiyi ayrıştırır.
    pub fn parse_hms(s: &str) -> Option<Self> {
        let mut parts = s.split(':');
        let h = parts.next()?.trim().parse::<u8>().ok()?;
        let m = parts.next()?.trim().parse::<u8>().ok()?;
        let s = parts.next()?.trim().parse::<u8>().ok()?;
        if h < 24 && m < 60 && s < 60 {
            Some(Self::new(h, m, s))
        } else {
            None
        }
    }

    /// "HH:MM" olarak biçimlendirir.
    pub fn format_hm(&self) -> String {
        format!("{:02}:{:02}", self.hour, self.minute)
    }

    /// "HH:MM:SS" olarak biçimlendirir.
    pub fn format_hms(&self) -> String {
        format!("{:02}:{:02}:{:02}", self.hour, self.minute, self.second)
    }
}

impl std::fmt::Display for TimeOfDay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:02}:{:02}:{:02}", self.hour, self.minute, self.second)
    }
}

/// Günlük 6 namaz vaktini tutan yapı
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DayTimes {
    pub imsak: TimeOfDay,
    pub gunes: TimeOfDay,
    pub ogle: TimeOfDay,
    pub ikindi: TimeOfDay,
    pub aksam: TimeOfDay,
    pub yatsi: TimeOfDay,
}

impl DayTimes {
    pub const fn new(
        imsak: TimeOfDay,
        gunes: TimeOfDay,
        ogle: TimeOfDay,
        ikindi: TimeOfDay,
        aksam: TimeOfDay,
        yatsi: TimeOfDay,
    ) -> Self {
        Self {
            imsak,
            gunes,
            ogle,
            ikindi,
            aksam,
            yatsi,
        }
    }
}

/// Bir sonraki vakti ve kalan süreyi döner.
/// Yatsıdan sonra (now > today.yatsi) yarının imsakını döndürür.
/// Tam vakit anında (now == vakit) kalan süre 0 olarak o vakit döner.
pub fn next_prayer(
    now: TimeOfDay,
    today: &DayTimes,
    tomorrow: &DayTimes,
) -> (Vakit, Duration) {
    if now <= today.imsak {
        (Vakit::Imsak, now.duration_until(&today.imsak))
    } else if now <= today.gunes {
        (Vakit::Gunes, now.duration_until(&today.gunes))
    } else if now <= today.ogle {
        (Vakit::Ogle, now.duration_until(&today.ogle))
    } else if now <= today.ikindi {
        (Vakit::Ikindi, now.duration_until(&today.ikindi))
    } else if now <= today.aksam {
        (Vakit::Aksam, now.duration_until(&today.aksam))
    } else if now <= today.yatsi {
        (Vakit::Yatsi, now.duration_until(&today.yatsi))
    } else {
        // Yatsıdan sonra: yarının imsakını döndür
        (Vakit::Imsak, now.duration_until(&tomorrow.imsak))
    }
}

/// Kalan süreye göre uyarı seviyesini döner:
/// - <= 15 dk: Kritik
/// - <= 30 dk: Uyarı
/// - Diğer: Normal
pub fn level(kalan: Duration) -> Level {
    if kalan <= KRITIK_ESIK {
        Level::Kritik
    } else if kalan <= UYARI_ESIK {
        Level::Uyari
    } else {
        Level::Normal
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_today() -> DayTimes {
        DayTimes {
            imsak: TimeOfDay::from_hm(5, 30),
            gunes: TimeOfDay::from_hm(7, 0),
            ogle: TimeOfDay::from_hm(12, 45),
            ikindi: TimeOfDay::from_hm(15, 45),
            aksam: TimeOfDay::from_hm(18, 15),
            yatsi: TimeOfDay::from_hm(19, 45),
        }
    }

    fn sample_tomorrow() -> DayTimes {
        DayTimes {
            imsak: TimeOfDay::from_hm(5, 31),
            gunes: TimeOfDay::from_hm(7, 1),
            ogle: TimeOfDay::from_hm(12, 45),
            ikindi: TimeOfDay::from_hm(15, 46),
            aksam: TimeOfDay::from_hm(18, 16),
            yatsi: TimeOfDay::from_hm(19, 46),
        }
    }

    #[test]
    fn test_normal_gun() {
        let today = sample_today();
        let tomorrow = sample_tomorrow();

        // 14:00:00 -> Öğle ile İkindi arası, sıradaki vakit İkindi (15:45)
        let now = TimeOfDay::from_hm(14, 0);
        let (vakit, kalan) = next_prayer(now, &today, &tomorrow);
        assert_eq!(vakit, Vakit::Ikindi);
        // 15:45 - 14:00 = 1 saat 45 dakika = 105 dakika = 6300 saniye
        assert_eq!(kalan, Duration::from_secs(105 * 60));
        assert_eq!(level(kalan), Level::Normal);

        // 15:20:00 -> İkindiye 25 dakika kala: Uyarı seviyesi
        let now_uyari = TimeOfDay::from_hm(15, 20);
        let (vakit_uyari, kalan_uyari) = next_prayer(now_uyari, &today, &tomorrow);
        assert_eq!(vakit_uyari, Vakit::Ikindi);
        assert_eq!(kalan_uyari, Duration::from_secs(25 * 60));
        assert_eq!(level(kalan_uyari), Level::Uyari);

        // 15:35:00 -> İkindiye 10 dakika kala: Kritik seviye
        let now_kritik = TimeOfDay::from_hm(15, 35);
        let (vakit_kritik, kalan_kritik) = next_prayer(now_kritik, &today, &tomorrow);
        assert_eq!(vakit_kritik, Vakit::Ikindi);
        assert_eq!(kalan_kritik, Duration::from_secs(10 * 60));
        assert_eq!(level(kalan_kritik), Level::Kritik);
    }

    #[test]
    fn test_yatsi_sonrasi() {
        let today = sample_today();
        let tomorrow = sample_tomorrow();

        // Yatsı 19:45. 21:00'de yatsıdan sonradır. Sıradaki vakit yarının imsakı (05:31) olmalı.
        let now = TimeOfDay::from_hm(21, 0);
        let (vakit, kalan) = next_prayer(now, &today, &tomorrow);
        assert_eq!(vakit, Vakit::Imsak);

        // 21:00'den gece yarısına (24:00) 3 saat (180 dakika)
        // Gece yarısından yarının imsakına (05:31) 5 saat 31 dakika (331 dakika)
        // Toplam = 511 dakika = 30660 saniye
        assert_eq!(kalan, Duration::from_secs(511 * 60));
        assert_eq!(level(kalan), Level::Normal);

        // Gece yarısından hemen önce: 23:59:00
        let now_pre_midnight = TimeOfDay::from_hm(23, 59);
        let (vakit_pm, kalan_pm) = next_prayer(now_pre_midnight, &today, &tomorrow);
        assert_eq!(vakit_pm, Vakit::Imsak);
        // 1 dakika + 331 dakika = 332 dakika
        assert_eq!(kalan_pm, Duration::from_secs(332 * 60));
    }

    #[test]
    fn test_gece_yarisi_sonrasi_imsaktan_once() {
        let today = sample_today();
        let tomorrow = sample_tomorrow();

        // Saat 02:00: Gece yarısını geçmiş, bugünün imsakı (05:30) henüz girmemiş.
        let now = TimeOfDay::from_hm(2, 0);
        let (vakit, kalan) = next_prayer(now, &today, &tomorrow);
        assert_eq!(vakit, Vakit::Imsak);
        // 05:30 - 02:00 = 3 saat 30 dakika = 210 dakika = 12600 saniye
        assert_eq!(kalan, Duration::from_secs(210 * 60));
        assert_eq!(level(kalan), Level::Normal);

        // Saat 00:00: Tam gece yarısı
        let midnight = TimeOfDay::from_hm(0, 0);
        let (vakit_mid, kalan_mid) = next_prayer(midnight, &today, &tomorrow);
        assert_eq!(vakit_mid, Vakit::Imsak);
        assert_eq!(kalan_mid, Duration::from_secs((5 * 60 + 30) * 60));

        // Saat 05:16: İmsaka 14 dakika kala -> Kritik
        let now_imsak_kritik = TimeOfDay::from_hm(5, 16);
        let (vakit_ik, kalan_ik) = next_prayer(now_imsak_kritik, &today, &tomorrow);
        assert_eq!(vakit_ik, Vakit::Imsak);
        assert_eq!(kalan_ik, Duration::from_secs(14 * 60));
        assert_eq!(level(kalan_ik), Level::Kritik);
    }

    #[test]
    fn test_tam_vakit_ani() {
        let today = sample_today();
        let tomorrow = sample_tomorrow();

        // 1) Tam Öğle Vakti: 12:45:00
        let now_ogle = today.ogle;
        let (vakit_ogle, kalan_ogle) = next_prayer(now_ogle, &today, &tomorrow);
        assert_eq!(vakit_ogle, Vakit::Ogle);
        assert_eq!(kalan_ogle, Duration::ZERO);
        assert_eq!(level(kalan_ogle), Level::Kritik);

        // 2) Tam Yatsı Vakti: 19:45:00
        let now_yatsi = today.yatsi;
        let (vakit_yatsi, kalan_yatsi) = next_prayer(now_yatsi, &today, &tomorrow);
        assert_eq!(vakit_yatsi, Vakit::Yatsi);
        assert_eq!(kalan_yatsi, Duration::ZERO);
        assert_eq!(level(kalan_yatsi), Level::Kritik);

        // 3) Tam İmsak Vakti: 05:30:00
        let now_imsak = today.imsak;
        let (vakit_imsak, kalan_imsak) = next_prayer(now_imsak, &today, &tomorrow);
        assert_eq!(vakit_imsak, Vakit::Imsak);
        assert_eq!(kalan_imsak, Duration::ZERO);
        assert_eq!(level(kalan_imsak), Level::Kritik);

        // 4) Vaktin tam anından 1 saniye sonra bir sonraki vakte geçildiğinin doğrulanması:
        // 12:45:01 -> Öğleden sonra, sıradaki İkindi (15:45:00)
        let now_after_ogle = TimeOfDay::new(12, 45, 1);
        let (vakit_next, kalan_next) = next_prayer(now_after_ogle, &today, &tomorrow);
        assert_eq!(vakit_next, Vakit::Ikindi);
        // 15:45:00 - 12:45:01 = 2 saat 59 dakika 59 saniye = 10799 saniye
        assert_eq!(kalan_next, Duration::from_secs(10799));
        assert_eq!(level(kalan_next), Level::Normal);

        // 19:45:01 -> Yatsıdan sonra, sıradaki yarının İmsakı (05:31:00)
        let now_after_yatsi = TimeOfDay::new(19, 45, 1);
        let (vakit_tomorrow, kalan_tomorrow) = next_prayer(now_after_yatsi, &today, &tomorrow);
        assert_eq!(vakit_tomorrow, Vakit::Imsak);
        assert_eq!(level(kalan_tomorrow), Level::Normal);
    }

    #[test]
    fn test_esik_sinirlari() {
        // Eşikler: <= 30 dk Uyarı, <= 15 dk Kritik

        // 30 dk 1 saniye -> Normal
        assert_eq!(level(Duration::from_secs(30 * 60 + 1)), Level::Normal);
        assert_eq!(level(Duration::from_secs(31 * 60)), Level::Normal);
        assert_eq!(level(Duration::from_secs(60 * 60)), Level::Normal);

        // Tam 30 dakika (sınır: <= 30 dk) -> Uyarı
        assert_eq!(level(Duration::from_secs(30 * 60)), Level::Uyari);
        // 20 dakika -> Uyarı
        assert_eq!(level(Duration::from_secs(20 * 60)), Level::Uyari);
        // 15 dk 1 saniye -> Uyarı
        assert_eq!(level(Duration::from_secs(15 * 60 + 1)), Level::Uyari);

        // Tam 15 dakika (sınır: <= 15 dk) -> Kritik
        assert_eq!(level(Duration::from_secs(15 * 60)), Level::Kritik);
        // 14 dk 59 saniye -> Kritik
        assert_eq!(level(Duration::from_secs(14 * 60 + 59)), Level::Kritik);
        // 1 dakika -> Kritik
        assert_eq!(level(Duration::from_secs(60)), Level::Kritik);
        // 0 saniye -> Kritik
        assert_eq!(level(Duration::ZERO), Level::Kritik);
    }

    #[test]
    fn test_time_of_day_utils() {
        let t1 = TimeOfDay::parse_hm("05:30").unwrap();
        assert_eq!(t1, TimeOfDay::from_hm(5, 30));
        assert_eq!(t1.format_hm(), "05:30");

        let t2 = TimeOfDay::parse_hms("12:45:30").unwrap();
        assert_eq!(t2, TimeOfDay::new(12, 45, 30));
        assert_eq!(t2.format_hms(), "12:45:30");

        assert!(TimeOfDay::parse_hm("25:00").is_none());
        assert!(TimeOfDay::parse_hm("12:60").is_none());
        assert!(TimeOfDay::parse_hm("not_time").is_none());

        let t_midnight_wrap = TimeOfDay::from_total_seconds(24 * 3600 + 3661);
        assert_eq!(t_midnight_wrap, TimeOfDay::new(1, 1, 1));
    }
}

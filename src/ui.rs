//! Win32 & GDI arayüz modülü.
//! Kenarlıksız, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_NOACTIVATE, WS_EX_LAYERED pencere.
//! "İkindi 1:23" biçiminde vakit adı ve kalan süre gösterimi.
//! Sol tık: 6 vakti gösteren yukarı doğru açılır liste (vurgulu sonraki vakit, soluk geçmiş).
//! Odak kaybı / Esc / Tekrar tık ile küçülme.
//! Sağ tık menüsü: Şehri aç (config.json), Windows ile başlat (aç/kapa), Çıkış.
//! Sıfır unwrap/expect; tüm hatalar 100 KB sınırlı log dosyasına yazılır.
//! Gün değişimi (00:00) gece yarısı 1 sn hassasiyetiyle yakalanır.

#![allow(dead_code)]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint, FillRect,
    InvalidateRect, SelectObject, SetBkMode, SetTextColor, DT_CENTER, DT_LEFT, DT_RIGHT,
    DT_SINGLELINE, DT_VCENTER, FW_BOLD, FW_SEMIBOLD, HDC, PAINTSTRUCT, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
    HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_SZ, REG_VALUE_TYPE,
};
use windows::Win32::UI::HiDpi::{
    GetDpiForWindow, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{SetFocus, VK_ESCAPE};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
    DispatchMessageW, GetCursorPos, GetMessageW, KillTimer, PostQuitMessage, RegisterClassW,
    SetForegroundWindow, SetLayeredWindowAttributes, SetTimer, SetWindowPos, ShowWindow,
    SystemParametersInfoW, TrackPopupMenu, TranslateMessage, HWND_TOPMOST, LWA_ALPHA,
    MF_CHECKED, MF_SEPARATOR, MF_STRING, MF_UNCHECKED, MSG, SPI_GETWORKAREA, SWP_NOACTIVATE,
    SWP_SHOWWINDOW, SW_SHOW, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, TPM_BOTTOMALIGN,
    TPM_RIGHTBUTTON, WA_INACTIVE, WM_ACTIVATE, WM_COMMAND, WM_CREATE, WM_DESTROY,
    WM_DISPLAYCHANGE, WM_DPICHANGED, WM_ERASEBKGND, WM_KEYDOWN, WM_KILLFOCUS, WM_LBUTTONDOWN,
    WM_PAINT, WM_POWERBROADCAST, WM_RBUTTONUP, WM_SETTINGCHANGE, WM_TIMECHANGE, WM_TIMER,
    WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

use crate::data::{DataManager, LocalDate};
use crate::logger::{log_error, log_info};
use crate::time_logic::{level, next_prayer, DayTimes, Level, TimeOfDay, Vakit};

/// Temel pencere genişliği (96 DPI / %100 ölçek için)
pub const BASE_WIDTH: i32 = 110;

/// Temel pencere yüksekliği (96 DPI / %100 ölçek için)
pub const BASE_HEIGHT: i32 = 28;

/// Genişletilmiş pencere genişliği (6 vakit listesi için)
pub const EXPANDED_WIDTH: i32 = 135;

/// Genişletilmiş pencere yüksekliği (6 vakit listesi için)
pub const EXPANDED_HEIGHT: i32 = 162;

/// Temel yazı tipi yüksekliği (negatif piksel cinsinden)
pub const BASE_FONT_HEIGHT: i32 = -13;

/// Pencere sınıfı adı
pub const WINDOW_CLASS_NAME: PCWSTR = w!("NamazVaktiWidget");

/// Timer tanımlayıcısı
pub const WIDGET_TIMER_ID: usize = 1;

/// Normal timer aralığı (60 saniye = 60000 ms)
pub const TIMER_INTERVAL_NORMAL_MS: u32 = 60_000;

/// Son 5 dakika ve gece yarısı kritik timer aralığı (1 saniye = 1000 ms)
pub const TIMER_INTERVAL_CRITICAL_MS: u32 = 1_000;

/// Pencere opaklık değeri (0..255; 225 = hafif şeffaf / %88 opaklık)
pub const WINDOW_ALPHA: u8 = 225;

/// Menü Komut Kimlikleri
pub const CMD_OPEN_CONFIG: usize = 1001;
pub const CMD_TOGGLE_AUTOSTART: usize = 1002;
pub const CMD_EXIT: usize = 1003;

/// RGB renk üretici fonksiyonu
pub const fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF(r as u32 | ((g as u32) << 8) | ((b as u32) << 16))
}

/// Normal durum rengi: Nötr açık gri / yumuşak beyaz
pub const COLOR_NORMAL: COLORREF = rgb(230, 230, 230);

/// Uyarı durum rengi: Canlı turuncu
pub const COLOR_UYARI: COLORREF = rgb(255, 165, 0);

/// Kritik durum rengi: Dikkat çekici kırmızı
pub const COLOR_KRITIK: COLORREF = rgb(255, 75, 75);

/// Vurgu rengi (Genişletilmiş modda sonraki vakit): Canlı altın sarısı/turuncu
pub const COLOR_HIGHLIGHT: COLORREF = rgb(255, 210, 60);

/// Geçmiş vakit rengi: Soluk gri
pub const COLOR_DIMMED: COLORREF = rgb(125, 125, 125);

/// Gelecek vakit rengi: Yumuşak açık gri
pub const COLOR_FUTURE: COLORREF = rgb(210, 210, 210);

/// Widget çalışır durumda mı kontrol bayrağı
static IS_RUNNING: AtomicBool = AtomicBool::new(false);

/// Widget'ın çalışma zamanı durumu
pub struct WidgetState {
    pub current_date: Option<LocalDate>,
    pub today_times: Option<DayTimes>,
    pub tomorrow_times: Option<DayTimes>,
    pub current_text: String,
    pub current_level: Level,
    pub next_vakit: Option<Vakit>,
    pub is_after_yatsi: bool,
    pub timer_interval_ms: u32,
    pub is_expanded: bool,
    pub error_message: Option<String>,
}

impl Default for WidgetState {
    fn default() -> Self {
        Self {
            current_date: None,
            today_times: None,
            tomorrow_times: None,
            current_text: "00:00".to_string(),
            current_level: Level::Normal,
            next_vakit: None,
            is_after_yatsi: false,
            timer_interval_ms: TIMER_INTERVAL_NORMAL_MS,
            is_expanded: false,
            error_message: None,
        }
    }
}

/// Global widget durumu
static STATE: Mutex<Option<WidgetState>> = Mutex::new(None);

/// Kalan süreyi "1:23" formatında biçimlendirir.
/// - Saat > 0 ise: "H:MM" (örn. "1:23", "2:05")
/// - Saat == 0 ve süre > 5 dk ise: "0:MM" (örn. "0:25")
/// - Süre <= 5 dk ise: "M:SS" (örn. "4:59", "1:23", "0:05", "0:00")
pub fn format_remaining(kalan: Duration) -> String {
    let total_secs = kalan.as_secs();
    let hours = total_secs / 3600;
    let mins = (total_secs % 3600) / 60;
    let secs = total_secs % 60;

    if hours > 0 {
        format!("{}:{:02}", hours, mins)
    } else if total_secs > 300 {
        format!("0:{:02}", mins)
    } else {
        format!("{}:{:02}", mins, secs)
    }
}

/// DPI değerine göre piksel boyutunu ölçekler.
pub const fn scale_dpi(base: i32, dpi: u32) -> i32 {
    (base * dpi as i32 + 48) / 96
}

/// SPI_GETWORKAREA çalışma alanı RECT'ine göre pencerenin (x, y) konumunu hesaplar.
/// Görev çubuğunun konumundan bağımsız olarak daima çalışma alanının sağ alt köşesinde kalır.
pub const fn calculate_position(work_area: RECT, width: i32, height: i32) -> (i32, i32) {
    let x = work_area.right - width;
    let y = work_area.bottom - height;
    (x, y)
}

/// Çalışma alanını (SPI_GETWORKAREA) sorgular.
pub fn get_work_area() -> windows::core::Result<RECT> {
    let mut work_area = RECT::default();
    unsafe {
        SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some(&mut work_area as *mut _ as *mut _),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )?;
    }
    Ok(work_area)
}

/// Pencereyi çalışma alanının sağ alt köşesine ve güncel genişleme/DPI boyutuna göre yeniden konumlandırır.
pub fn reposition_window(hwnd: HWND, is_expanded: bool) {
    unsafe {
        let dpi = GetDpiForWindow(hwnd);
        let dpi = if dpi == 0 { 96 } else { dpi };

        let (base_w, base_h) = if is_expanded {
            (EXPANDED_WIDTH, EXPANDED_HEIGHT)
        } else {
            (BASE_WIDTH, BASE_HEIGHT)
        };

        let w = scale_dpi(base_w, dpi);
        let h = scale_dpi(base_h, dpi);

        let work_area = match get_work_area() {
            Ok(wa) => wa,
            Err(e) => {
                log_error(&format!("get_work_area başarısız oldu: {}", e));
                RECT {
                    left: 0,
                    top: 0,
                    right: 1920,
                    bottom: 1080,
                }
            }
        };

        let (x, y) = calculate_position(work_area, w, h);

        let _ = SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            x,
            y,
            w,
            h,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
        let _ = InvalidateRect(hwnd, None, true);
    }
}

/// Windows ile otomatik başlatma durumunu Registry'den okur.
pub fn is_autostart_enabled() -> bool {
    unsafe {
        let mut hkey = HKEY::default();
        let subkey = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
        if RegOpenKeyExW(HKEY_CURRENT_USER, subkey, 0, KEY_READ, &mut hkey).is_err() {
            return false;
        }
        let val_name = w!("NamazVaktiWidget");
        let mut data_type = REG_VALUE_TYPE(0);
        let mut data_len = 0u32;
        let res = RegQueryValueExW(
            hkey,
            val_name,
            None,
            Some(&mut data_type),
            None,
            Some(&mut data_len),
        );
        let _ = RegCloseKey(hkey);
        res.is_ok() && data_len > 0
    }
}

/// Windows ile otomatik başlatmayı açar veya kapatır.
pub fn set_autostart(enable: bool) -> windows::core::Result<()> {
    unsafe {
        let mut hkey = HKEY::default();
        let subkey = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
        RegOpenKeyExW(HKEY_CURRENT_USER, subkey, 0, KEY_WRITE, &mut hkey).ok()?;
        let val_name = w!("NamazVaktiWidget");

        let result = if enable {
            let exe_path = match std::env::current_exe() {
                Ok(p) => p,
                Err(e) => {
                    log_error(&format!("current_exe yolu alınamadı: {}", e));
                    return Err(windows::core::Error::new(windows::core::HRESULT(-1), e.to_string()));
                }
            };
            let path_str = format!("\"{}\"", exe_path.display());
            let wide_path: Vec<u16> = path_str.encode_utf16().chain(std::iter::once(0)).collect();
            let byte_slice = std::slice::from_raw_parts(
                wide_path.as_ptr() as *const u8,
                wide_path.len() * 2,
            );
            RegSetValueExW(hkey, val_name, 0, REG_SZ, Some(byte_slice)).ok()
        } else {
            let res = RegDeleteValueW(hkey, val_name);
            if res.is_ok() || res == windows::Win32::Foundation::ERROR_FILE_NOT_FOUND {
                Ok(())
            } else {
                res.ok()
            }
        };
        let _ = RegCloseKey(hkey);
        result
    }
}

/// Günlük vakit verilerini gerektiğinde (gün değiştiğinde veya açılışta) yükler.
/// Gün değişimini (00:00) doğru yakalayarak önbelleği/API'yi tazeler.
fn ensure_day_times_loaded(state: &mut WidgetState) {
    let today = LocalDate::now();
    let is_day_changed = state.current_date.map(|d| d != today).unwrap_or(false);

    if is_day_changed {
        log_info(&format!(
            "Gün değişimi algılandı: {:?} -> {:?}",
            state.current_date, today
        ));
        state.today_times = None;
        state.tomorrow_times = None;
    }

    if state.current_date != Some(today) || state.today_times.is_none() {
        match DataManager::new_default() {
            Ok(mut dm) => match dm.get_or_fetch_today_and_tomorrow(&today) {
                Ok((t, tom)) => {
                    state.current_date = Some(today);
                    state.today_times = Some(t);
                    state.tomorrow_times = Some(tom);
                    state.error_message = None;
                    log_info(&format!("Vakit verileri güncellendi: {}", today.to_iso_string()));
                }
                Err(e) => {
                    log_error(&format!("Vakit verisi getirme hatası: {}", e));
                    state.error_message = Some(e.to_string());
                }
            },
            Err(e) => {
                log_error(&format!("DataManager başlatma hatası: {}", e));
                state.error_message = Some(e.to_string());
            }
        }
    }
}

/// Widget durumunu günceller:
/// - "İkindi 1:23" metnini ve renk seviyesini hesaplar.
/// - Sadece metin veya renk değiştiğinde `InvalidateRect` tetikler.
/// - Kalan süre <= 5 dk veya gece yarısına <= 5 dk kala timer'ı 1 sn'ye çeker; aksi halde 60 sn yapar.
pub fn update_widget_state(hwnd: HWND, force_repaint: bool) {
    let mut guard = match STATE.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };

    let state = match guard.as_mut() {
        Some(s) => s,
        None => return,
    };

    ensure_day_times_loaded(state);

    let (new_text, new_level, new_next_vakit, new_after_yatsi, needed_interval_ms) =
        if let Some(ref err) = state.error_message {
            let msg = if err.contains("Ağ yok") {
                "Ağ Yok".to_string()
            } else {
                "Hata".to_string()
            };
            (msg, Level::Kritik, None, false, TIMER_INTERVAL_NORMAL_MS)
        } else if let (Some(today), Some(tomorrow)) = (state.today_times, state.tomorrow_times) {
            let now = TimeOfDay::now();
            let (vakit, kalan) = next_prayer(now, &today, &tomorrow);
            let text = format!("{} {}", vakit.name_tr(), format_remaining(kalan));
            let lvl = level(kalan);
            let is_after_yatsi = now > today.yatsi;

            // Gece yarısına (00:00:00) kalan süre hesabı:
            let secs_until_midnight = (24 * 3600u32).saturating_sub(now.total_seconds());

            // Kalan süre <= 5 dk ise VEYA gece yarısına <= 5 dk kala timer 1 sn'ye geçer
            let interval = if kalan <= Duration::from_secs(300) || secs_until_midnight <= 300 {
                TIMER_INTERVAL_CRITICAL_MS
            } else {
                TIMER_INTERVAL_NORMAL_MS
            };
            (text, lvl, Some(vakit), is_after_yatsi, interval)
        } else {
            ("Yükleniyor...".to_string(), Level::Normal, None, false, TIMER_INTERVAL_NORMAL_MS)
        };

    let text_changed = state.current_text != new_text;
    let level_changed = state.current_level != new_level;

    state.next_vakit = new_next_vakit;
    state.is_after_yatsi = new_after_yatsi;

    // Çizim SADECE metin veya renk değiştiğinde (veya zorlandığında) tetiklenir
    if text_changed || level_changed || force_repaint {
        state.current_text = new_text;
        state.current_level = new_level;
        unsafe {
            let _ = InvalidateRect(hwnd, None, true);
        }
    }

    // Timer aralığı değiştiyse güncelle
    if state.timer_interval_ms != needed_interval_ms {
        state.timer_interval_ms = needed_interval_ms;
        unsafe {
            let _ = SetTimer(hwnd, WIDGET_TIMER_ID, needed_interval_ms, None);
        }
    }
}

/// GDI ile pencere içeriğini çizer (kompakt mod veya 6 vakit genişletilmiş mod).
unsafe fn paint_window(hwnd: HWND, hdc: HDC) {
    unsafe {
        let mut client_rect = RECT::default();
        let _ = windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut client_rect);

        // Koyu modern arka plan: RGB(22, 22, 22)
        let bg_color = COLORREF(0x00161616);
        let brush = CreateSolidBrush(bg_color);
        FillRect(hdc, &client_rect, brush);
        let _ = DeleteObject(brush);

        let (is_expanded, text, lvl, today_times, next_vakit, is_after_yatsi) = {
            let guard = STATE.lock().unwrap_or_else(|p| p.into_inner());
            if let Some(ref s) = *guard {
                (
                    s.is_expanded,
                    s.current_text.clone(),
                    s.current_level,
                    s.today_times,
                    s.next_vakit,
                    s.is_after_yatsi,
                )
            } else {
                (false, "00:00".to_string(), Level::Normal, None, None, false)
            }
        };

        let dpi = GetDpiForWindow(hwnd);
        let dpi = if dpi == 0 { 96 } else { dpi };
        let font_height = scale_dpi(BASE_FONT_HEIGHT, dpi);

        SetBkMode(hdc, TRANSPARENT);

        if !is_expanded {
            // --- KOMPAKT MOD: "İkindi 1:23" ---
            let font = CreateFontW(
                font_height,
                0,
                0,
                0,
                FW_SEMIBOLD.0 as i32,
                0,
                0,
                0,
                windows::Win32::Graphics::Gdi::DEFAULT_CHARSET.0 as u32,
                windows::Win32::Graphics::Gdi::OUT_DEFAULT_PRECIS.0 as u32,
                windows::Win32::Graphics::Gdi::CLIP_DEFAULT_PRECIS.0 as u32,
                windows::Win32::Graphics::Gdi::CLEARTYPE_QUALITY.0 as u32,
                windows::Win32::Graphics::Gdi::VARIABLE_PITCH.0 as u32,
                w!("Segoe UI"),
            );
            let old_font = SelectObject(hdc, font);

            let text_color = match lvl {
                Level::Normal => COLOR_NORMAL,
                Level::Uyari => COLOR_UYARI,
                Level::Kritik => COLOR_KRITIK,
            };
            SetTextColor(hdc, text_color);

            let mut wide_chars: Vec<u16> = text.encode_utf16().collect();
            DrawTextW(
                hdc,
                &mut wide_chars,
                &mut client_rect,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            );

            SelectObject(hdc, old_font);
            let _ = DeleteObject(font);
        } else {
            // --- GENİŞLETİLMİŞ MOD: 6 Vaktin Hepsi ---
            let font_normal = CreateFontW(
                font_height,
                0,
                0,
                0,
                FW_SEMIBOLD.0 as i32,
                0,
                0,
                0,
                windows::Win32::Graphics::Gdi::DEFAULT_CHARSET.0 as u32,
                windows::Win32::Graphics::Gdi::OUT_DEFAULT_PRECIS.0 as u32,
                windows::Win32::Graphics::Gdi::CLIP_DEFAULT_PRECIS.0 as u32,
                windows::Win32::Graphics::Gdi::CLEARTYPE_QUALITY.0 as u32,
                windows::Win32::Graphics::Gdi::VARIABLE_PITCH.0 as u32,
                w!("Segoe UI"),
            );

            let font_bold = CreateFontW(
                font_height,
                0,
                0,
                0,
                FW_BOLD.0 as i32,
                0,
                0,
                0,
                windows::Win32::Graphics::Gdi::DEFAULT_CHARSET.0 as u32,
                windows::Win32::Graphics::Gdi::OUT_DEFAULT_PRECIS.0 as u32,
                windows::Win32::Graphics::Gdi::CLIP_DEFAULT_PRECIS.0 as u32,
                windows::Win32::Graphics::Gdi::CLEARTYPE_QUALITY.0 as u32,
                windows::Win32::Graphics::Gdi::VARIABLE_PITCH.0 as u32,
                w!("Segoe UI"),
            );

            let old_font = SelectObject(hdc, font_normal);

            if let Some(dt) = today_times {
                let prayers = [
                    (Vakit::Imsak, dt.imsak),
                    (Vakit::Gunes, dt.gunes),
                    (Vakit::Ogle, dt.ogle),
                    (Vakit::Ikindi, dt.ikindi),
                    (Vakit::Aksam, dt.aksam),
                    (Vakit::Yatsi, dt.yatsi),
                ];

                let pad_x = scale_dpi(10, dpi);
                let pad_y = scale_dpi(6, dpi);
                let total_h = client_rect.bottom - client_rect.top - 2 * pad_y;
                let row_h = total_h / 6;

                for (i, (vakit, time)) in prayers.iter().enumerate() {
                    let y1 = pad_y + (i as i32) * row_h;
                    let y2 = y1 + row_h;

                    // Sonraki vakit vurgulu, geçmişler soluk, gelecekler normal
                    let is_target = match next_vakit {
                        Some(nv) => {
                            if is_after_yatsi {
                                *vakit == Vakit::Imsak
                            } else {
                                *vakit == nv
                            }
                        }
                        None => false,
                    };

                    let is_past = match next_vakit {
                        Some(nv) => {
                            if is_after_yatsi {
                                true
                            } else {
                                *vakit < nv
                            }
                        }
                        None => false,
                    };

                    if is_target {
                        let highlight_rect = RECT {
                            left: client_rect.left + scale_dpi(4, dpi),
                            top: y1,
                            right: client_rect.right - scale_dpi(4, dpi),
                            bottom: y2,
                        };
                        let h_brush = CreateSolidBrush(rgb(45, 45, 45));
                        FillRect(hdc, &highlight_rect, h_brush);
                        let _ = DeleteObject(h_brush);
                        SelectObject(hdc, font_bold);
                        SetTextColor(hdc, COLOR_HIGHLIGHT);
                    } else if is_past {
                        SelectObject(hdc, font_normal);
                        SetTextColor(hdc, COLOR_DIMMED);
                    } else {
                        SelectObject(hdc, font_normal);
                        SetTextColor(hdc, COLOR_FUTURE);
                    }

                    // Sol: Vakit adı
                    let name_str = if is_target {
                        format!("> {}", vakit.name_tr())
                    } else {
                        vakit.name_tr().to_string()
                    };
                    let mut wide_name: Vec<u16> = name_str.encode_utf16().collect();
                    let mut name_rect = RECT {
                        left: client_rect.left + pad_x,
                        top: y1,
                        right: client_rect.left + scale_dpi(70, dpi),
                        bottom: y2,
                    };
                    DrawTextW(hdc, &mut wide_name, &mut name_rect, DT_LEFT | DT_VCENTER | DT_SINGLELINE);

                    // Sağ: Saat (HH:MM)
                    let time_str = time.format_hm();
                    let mut wide_time: Vec<u16> = time_str.encode_utf16().collect();
                    let mut time_rect = RECT {
                        left: client_rect.right - scale_dpi(55, dpi),
                        top: y1,
                        right: client_rect.right - pad_x,
                        bottom: y2,
                    };
                    DrawTextW(hdc, &mut wide_time, &mut time_rect, DT_RIGHT | DT_VCENTER | DT_SINGLELINE);
                }
            }

            SelectObject(hdc, old_font);
            let _ = DeleteObject(font_normal);
            let _ = DeleteObject(font_bold);
        }
    }
}

/// Win32 pencere prosedürü (Window Proc)
unsafe extern "system" fn wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            unsafe {
                let _ = SetTimer(hwnd, WIDGET_TIMER_ID, TIMER_INTERVAL_NORMAL_MS, None);
            }
            update_widget_state(hwnd, true);
            LRESULT(0)
        }
        WM_TIMER => {
            update_widget_state(hwnd, false);
            LRESULT(0)
        }
        WM_POWERBROADCAST | WM_TIMECHANGE => {
            log_info("Sistem uyanışı veya saat değişimi algılandı, yenileniyor");
            update_widget_state(hwnd, true);
            LRESULT(0)
        }
        WM_PAINT => {
            unsafe {
                let mut ps = PAINTSTRUCT::default();
                let hdc = BeginPaint(hwnd, &mut ps);
                paint_window(hwnd, hdc);
                let _ = EndPaint(hwnd, &ps);
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let is_exp = {
                let mut guard = STATE.lock().unwrap_or_else(|p| p.into_inner());
                if let Some(ref mut s) = *guard {
                    s.is_expanded = !s.is_expanded;
                    s.is_expanded
                } else {
                    false
                }
            };
            reposition_window(hwnd, is_exp);
            if is_exp {
                unsafe {
                    let _ = SetForegroundWindow(hwnd);
                    let _ = SetFocus(hwnd);
                }
            }
            LRESULT(0)
        }
        WM_KILLFOCUS => {
            let was_expanded = {
                let mut guard = STATE.lock().unwrap_or_else(|p| p.into_inner());
                if let Some(ref mut s) = *guard {
                    if s.is_expanded {
                        s.is_expanded = false;
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            };
            if was_expanded {
                reposition_window(hwnd, false);
            }
            LRESULT(0)
        }
        WM_ACTIVATE => {
            if (wparam.0 & 0xFFFF) == WA_INACTIVE as usize {
                let was_expanded = {
                    let mut guard = STATE.lock().unwrap_or_else(|p| p.into_inner());
                    if let Some(ref mut s) = *guard {
                        if s.is_expanded {
                            s.is_expanded = false;
                            true
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                };
                if was_expanded {
                    reposition_window(hwnd, false);
                }
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            if wparam.0 == VK_ESCAPE.0 as usize {
                let was_expanded = {
                    let mut guard = STATE.lock().unwrap_or_else(|p| p.into_inner());
                    if let Some(ref mut s) = *guard {
                        if s.is_expanded {
                            s.is_expanded = false;
                            true
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                };
                if was_expanded {
                    reposition_window(hwnd, false);
                }
            }
            LRESULT(0)
        }
        WM_RBUTTONUP => {
            unsafe {
                let mut pt = windows::Win32::Foundation::POINT::default();
                let _ = GetCursorPos(&mut pt);

                if let Ok(hmenu) = CreatePopupMenu() {
                    let _ = AppendMenuW(hmenu, MF_STRING, CMD_OPEN_CONFIG, w!("Şehri aç (config.json)"));

                    let autostart = is_autostart_enabled();
                    let check_flag = if autostart { MF_CHECKED } else { MF_UNCHECKED };
                    let _ = AppendMenuW(
                        hmenu,
                        MF_STRING | check_flag,
                        CMD_TOGGLE_AUTOSTART,
                        w!("Windows ile başlat"),
                    );

                    let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, PCWSTR::null());
                    let _ = AppendMenuW(hmenu, MF_STRING, CMD_EXIT, w!("Çıkış"));

                    let _ = SetForegroundWindow(hwnd);
                    let _ = TrackPopupMenu(
                        hmenu,
                        TPM_RIGHTBUTTON | TPM_BOTTOMALIGN,
                        pt.x,
                        pt.y,
                        0,
                        hwnd,
                        None,
                    );
                    let _ = DestroyMenu(hmenu);
                }
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            let cmd_id = wparam.0 & 0xFFFF;
            match cmd_id {
                CMD_OPEN_CONFIG => {
                    if let Ok(config_path) = crate::data::get_config_path() {
                        let _ = crate::config::Config::load_or_create(&config_path);
                        if let Err(e) = open_with_notepad(&config_path) {
                            log_error(&format!("Notepad başlatılamadı: {}", e));
                        }
                    }
                }
                CMD_TOGGLE_AUTOSTART => {
                    let current = is_autostart_enabled();
                    if let Err(e) = set_autostart(!current) {
                        log_error(&format!("Autostart değiştirme hatası: {}", e));
                    } else {
                        log_info(&format!("Autostart durumu değiştirildi: {}", !current));
                    }
                }
                CMD_EXIT => {
                    unsafe {
                        let _ = DestroyWindow(hwnd);
                    }
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_DISPLAYCHANGE | WM_SETTINGCHANGE => {
            let is_exp = {
                let guard = STATE.lock().unwrap_or_else(|p| p.into_inner());
                guard.as_ref().map(|s| s.is_expanded).unwrap_or(false)
            };
            reposition_window(hwnd, is_exp);
            LRESULT(0)
        }
        WM_DPICHANGED => {
            let is_exp = {
                let guard = STATE.lock().unwrap_or_else(|p| p.into_inner());
                guard.as_ref().map(|s| s.is_expanded).unwrap_or(false)
            };
            reposition_window(hwnd, is_exp);
            LRESULT(0)
        }
        WM_DESTROY => {
            IS_RUNNING.store(false, Ordering::SeqCst);
            unsafe {
                let _ = KillTimer(hwnd, WIDGET_TIMER_ID);
                PostQuitMessage(0);
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

/// Win32 pencere sınıfını kaydeder.
pub fn register_widget_class() -> windows::core::Result<()> {
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let wnd_class = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: instance.into(),
            lpszClassName: WINDOW_CLASS_NAME,
            ..Default::default()
        };
        let atom = RegisterClassW(&wnd_class);
        if atom == 0 {
            let err = windows::core::Error::from_win32();
            if err.code().0 != windows::Win32::Foundation::ERROR_CLASS_ALREADY_EXISTS.0 as i32 {
                return Err(err);
            }
        }
        Ok(())
    }
}

/// Win32 widget penceresini oluşturur ve gösterir.
pub fn create_widget_window() -> windows::core::Result<HWND> {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }

    register_widget_class()?;

    // State'i başlat
    {
        let mut guard = STATE.lock().unwrap_or_else(|p| p.into_inner());
        *guard = Some(WidgetState::default());
    }

    unsafe {
        let instance = GetModuleHandleW(None)?;

        let ex_style = WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_LAYERED;
        let style = WS_POPUP;

        let dpi = 96;
        let w = scale_dpi(BASE_WIDTH, dpi);
        let h = scale_dpi(BASE_HEIGHT, dpi);
        let work_area = get_work_area().unwrap_or(RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        });
        let (x, y) = calculate_position(work_area, w, h);

        let hwnd = CreateWindowExW(
            ex_style,
            WINDOW_CLASS_NAME,
            w!("Namaz Vakti Widget"),
            style,
            x,
            y,
            w,
            h,
            None,
            None,
            instance,
            None,
        )?;

        // Arka planı hafif şeffaf yap (WINDOW_ALPHA)
        let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), WINDOW_ALPHA, LWA_ALPHA);

        // Gerçek monitör DPI'ına göre son konumu ayarla
        reposition_window(hwnd, false);

        let _ = ShowWindow(hwnd, SW_SHOW);
        IS_RUNNING.store(true, Ordering::SeqCst);

        // İlk durumu güncelle
        update_widget_state(hwnd, true);

        // Boştaki Working Set RAM kullanımını optimize et (Windows bellek yöneticisine fazla sayfaları boşaltmasını bildir)
        let _ = windows::Win32::System::Threading::SetProcessWorkingSetSize(
            windows::Win32::System::Threading::GetCurrentProcess(),
            usize::MAX,
            usize::MAX,
        );

        Ok(hwnd)
    }
}

/// CPU ~0 mesaj döngüsünü çalıştırır. (GetMessageW bloklar, busy-loop yoktur).
pub fn run_message_loop() {
    unsafe {
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

/// Notepad ile yapılandırma dosyasını açar (CreateProcessW kullanarak hafifçe).
fn open_with_notepad(path: &std::path::Path) -> Result<(), String> {
    use windows::core::PWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        CreateProcessW, PROCESS_CREATION_FLAGS, PROCESS_INFORMATION, STARTUPINFOW,
    };

    let cmd_str = format!("notepad.exe \"{}\"\0", path.display());
    let mut cmd_utf16: Vec<u16> = cmd_str.encode_utf16().collect();
    let si = STARTUPINFOW {
        cb: std::mem::size_of::<STARTUPINFOW>() as u32,
        ..Default::default()
    };
    let mut pi = PROCESS_INFORMATION::default();

    let success = unsafe {
        CreateProcessW(
            None,
            PWSTR(cmd_utf16.as_mut_ptr()),
            None,
            None,
            false,
            PROCESS_CREATION_FLAGS(0),
            None,
            None,
            &si,
            &mut pi,
        )
    };

    if success.is_ok() {
        unsafe {
            let _ = CloseHandle(pi.hProcess);
            let _ = CloseHandle(pi.hThread);
        }
        Ok(())
    } else {
        Err("CreateProcessW başarısız oldu".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_remaining() {
        assert_eq!(format_remaining(Duration::from_secs(5000)), "1:23");
        assert_eq!(format_remaining(Duration::from_secs(2 * 3600 + 5 * 60)), "2:05");
        assert_eq!(format_remaining(Duration::from_secs(25 * 60)), "0:25");
        assert_eq!(format_remaining(Duration::from_secs(300)), "5:00");
        assert_eq!(format_remaining(Duration::from_secs(4 * 60 + 32)), "4:32");
        assert_eq!(format_remaining(Duration::from_secs(83)), "1:23");
        assert_eq!(format_remaining(Duration::from_secs(5)), "0:05");
        assert_eq!(format_remaining(Duration::ZERO), "0:00");
    }

    #[test]
    fn test_color_definitions() {
        assert_eq!(COLOR_NORMAL, rgb(230, 230, 230));
        assert_eq!(COLOR_UYARI, rgb(255, 165, 0));
        assert_eq!(COLOR_KRITIK, rgb(255, 75, 75));
        assert_eq!(COLOR_HIGHLIGHT, rgb(255, 210, 60));
        assert_eq!(COLOR_DIMMED, rgb(125, 125, 125));
        assert_eq!(COLOR_FUTURE, rgb(210, 210, 210));
    }

    #[test]
    fn test_scale_dpi() {
        assert_eq!(scale_dpi(BASE_WIDTH, 96), 110);
        assert_eq!(scale_dpi(BASE_HEIGHT, 96), 28);
        assert_eq!(scale_dpi(EXPANDED_WIDTH, 96), 135);
        assert_eq!(scale_dpi(EXPANDED_HEIGHT, 96), 162);
    }

    #[test]
    fn test_calculate_position_corner_anchored() {
        let work_area = RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1040,
        };

        // Kompakt mod
        let (x_compact, y_compact) = calculate_position(work_area, 110, 28);
        assert_eq!(x_compact, 1810);
        assert_eq!(y_compact, 1012);
        assert_eq!(x_compact + 110, work_area.right);
        assert_eq!(y_compact + 28, work_area.bottom);

        // Genişletilmiş mod
        let (x_exp, y_exp) = calculate_position(work_area, 135, 162);
        assert_eq!(x_exp, 1920 - 135);
        assert_eq!(y_exp, 1040 - 162);
        assert_eq!(x_exp + 135, work_area.right);
        assert_eq!(y_exp + 162, work_area.bottom);
    }

    #[test]
    fn test_widget_state_interval_selection_midnight_and_critical() {
        // Kalan süre > 5 dk ve gece yarısına uzak -> 60 sn
        let remaining_long = Duration::from_secs(10 * 60);
        let secs_until_midnight_far = 3600;
        let interval_normal = if remaining_long <= Duration::from_secs(300) || secs_until_midnight_far <= 300 {
            TIMER_INTERVAL_CRITICAL_MS
        } else {
            TIMER_INTERVAL_NORMAL_MS
        };
        assert_eq!(interval_normal, 60_000);

        // Kalan süre <= 5 dk -> 1 sn
        let remaining_short = Duration::from_secs(4 * 60);
        let interval_crit = if remaining_short <= Duration::from_secs(300) || secs_until_midnight_far <= 300 {
            TIMER_INTERVAL_CRITICAL_MS
        } else {
            TIMER_INTERVAL_NORMAL_MS
        };
        assert_eq!(interval_crit, 1_000);

        // Gece yarısına <= 5 dk kala (00:00 yakalama) -> 1 sn
        let secs_until_midnight_close = 120;
        let interval_midnight = if remaining_long <= Duration::from_secs(300) || secs_until_midnight_close <= 300 {
            TIMER_INTERVAL_CRITICAL_MS
        } else {
            TIMER_INTERVAL_NORMAL_MS
        };
        assert_eq!(interval_midnight, 1_000);
    }

    #[test]
    fn test_autostart_registry_check() {
        let _ = is_autostart_enabled();
    }
}

//! Namaz Vakti Hafif Masaüstü Widget'ı
//! Windows, Win32 + GDI, CPU ~0

#![windows_subsystem = "windows"]

mod config;
mod data;
mod logger;
mod time_logic;
mod ui;

use windows::core::w;
use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
use windows::Win32::System::Threading::CreateMutexW;

fn main() {
    // 1) Tek örnek (named mutex): İkinci açılış sessizce kapansın
    let mutex_name = w!("Local\\NamazVaktiWidget_SingleInstance");
    let mutex_handle: HANDLE = match unsafe { CreateMutexW(None, true, mutex_name) } {
        Ok(handle) => handle,
        Err(e) => {
            logger::log_error(&format!("Mutex oluşturma hatası: {}", e));
            return;
        }
    };

    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        // İkinci örnek çalışıyor, sessizce kapan
        let _ = unsafe { CloseHandle(mutex_handle) };
        return;
    }

    logger::log_info("Namaz Vakti widget başlatıldı");

    // 2) Widget penceresini oluştur
    if let Err(e) = ui::create_widget_window() {
        logger::log_error(&format!("Widget penceresi oluşturulamadı: {}", e));
        let _ = unsafe { CloseHandle(mutex_handle) };
        return;
    }

    // 3) Mesaj döngüsünü çalıştır (CPU ~0)
    ui::run_message_loop();

    logger::log_info("Namaz Vakti widget sonlandırıldı");

    // Mutex'i kapat
    let _ = unsafe { CloseHandle(mutex_handle) };
}

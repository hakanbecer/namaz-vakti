# Namaz Vakti Masaüstü Widget'ı (Windows)

Rust ile Windows için geliştirilmiş, sistem kaynaklarını neredeyse hiç tüketmeyen, ultra hafif ve bağımsız bir masaüstü namaz vakti widget'ı.

![Windows](https://img.shields.io/badge/Platform-Windows-blue)
![Rust](https://img.shields.io/badge/Language-Rust-orange)
![Binary Size](https://img.shields.io/badge/Binary%20Size-466%20KB-success)
![Memory](https://img.shields.io/badge/Memory-~2.3%20MB-success)
![CPU](https://img.shields.io/badge/CPU-0%25-success)

---

## Özellikler

- **Ultra Hafif & Hızlı:** Webview, Electron, Tauri veya harici ağır GUI kütüphaneleri içermez. Doğrudan Windows Win32 API ve GDI kullanılarak çizilir.
- **Düşük Kaynak Tüketimi:**
  - **Çalıştırılabilir Dosya:** Sadece **~466 KB**
  - **RAM (Working Set):** Sadece **~2.3 MB**
  - **RAM (Private Bytes):** Sadece **~1.3 MB**
  - **CPU:** Boşta **%0** (Windows mesaj döngüsünde `GetMessageW` ile uyur, busy-loop yoktur).
- **Zarif & Kullanışlı Arayüz:**
  - Ekranın sağ alt köşesinde (saatin hemen üzerinde) kenarlıksız, hafif şeffaf modern koyu kutu.
  - **Kompakt Mod:** Sıradaki vakit ve kalan süreyi gösterir (`İkindi 1:23`).
  - **Dinamik Renkler:** Normal (beyaz), Uyarı (≤30 dk - turuncu), Kritik (≤15 dk - kırmızı).
  - **Genişletilmiş Mod:** Widget'a sol tıklandığında yukarıya doğru genişleyerek 6 vaktin tamamını listeler. Sıradaki vakit vurgulanır, geçmiş vakitler soluklaşır. Odak kaybı, tekrar sol tık veya `Esc` ile kompakt moda döner.
- **Sağ Tık Menüsü:**
  - **Şehri aç (config.json):** Not Defteri ile şehir/ülke ayarını açar (varsayılan: `Malatya, Turkey`).
  - **Windows ile başlat:** `HKCU\...\Run` kayıt defteri üzerinden başlangıçta çalışma durumunu tek tıkla açıp kapatır.
  - **Çıkış:** Uygulamayı sonlandırır.
- **Akıllı Veri ve Önbellek Yönetimi:**
  - Aladhan API (Diyanet İşleri Başkanlığı hesaplama yöntemi) üzerinden aylık takvim çeker.
  - Verileri `%APPDATA%\namaz-widget\cache.json` içine kaydeder; internet olmasa bile önbellekten kesintisiz çalışır.
  - Ay bitimine yaklaşıldığında (25. günden sonra) bir sonraki ayın verisini arka planda otomatik günceller.
- **Yüksek Çözünürlük (Per-Monitor V2 DPI):** Yüksek DPI ekranlarda ve ölçekleme değişikliklerinde bulanıklaşmadan net çizim yapar.
- **Tek Örnek (Named Mutex):** İkinci bir kopya açıldığında arkada çalışan kopyayı bozmadan sessizce sonlanır.

---

## Kurulum ve Derleme

### Gereksinimler
- Windows 10 / 11
- Rust derleyicisi (`cargo`, `rustc`)

### Derleme

```bash
# Projeyi klonlayın
git clone https://github.com/hakanbecer/namaz-vakti.git
cd namaz-vakti

# Release modunda derleyin
cargo build --release
```

Derlenen bağımsız `.exe` dosyası `target\release\namazvakti.exe` konumunda oluşur.

### Çalıştırma

`target\release\namazvakti.exe` dosyasına çift tıklayarak doğrudan çalıştırabilirsiniz. Konsol penceresi açılmaz, arka planda masaüstü widget'ı olarak çalışır.

---

## Yapılandırma

Ayarlar `%APPDATA%\namaz-widget\config.json` dosyasında tutulur:

```json
{
  "city": "Malatya",
  "country": "Turkey"
}
```

Farklı bir şehir girmek için widget'a sağ tıklayıp **"Şehri aç (config.json)"** diyebilir veya bu dosyayı dilediğiniz editörle düzenleyebilirsiniz.

---

## Lisans

MIT Lisansı altında sunulmaktadır.

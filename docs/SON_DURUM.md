# OziiDPI - Proje Son Durum Raporu (Final)

Bu belge, OziiDPI projesinin backend ve frontend entegrasyon süreçlerinde baştan sona yapılan **tüm mühendislik ve geliştirme çalışmalarını** detaylı bir şekilde açıklamaktadır. 

Proje, yalnızca Discord trafiğini yönlendiren (sistemdeki diğer hiçbir trafiği etkilemeyen), kendi kendini onarabilen ve modern bir Tauri masaüstü arayüzüne sahip, güvenli bir DPI aşma (bypass) yazılımına dönüştürülmüştür.

---

## 1. Mimari ve Güvenlik (Backend)
OziiDPI'ın arka planında çalışan Rust tabanlı çekirdek (ozii-core), sistem stabilitesini korumak için ciddi güvenlik katmanlarıyla donatıldı.

*   **Sadece Discord Kuralı (Discord-only Routing):** Sistemdeki tüm trafiği proxy'e sokmak yerine, **SADECE** Discord alan adları (`discord.com`, `gateway.discord.gg`, vb.) yerel proxy'e yönlendirilir. Oyunlar, Windows güncellemeleri veya diğer tarayıcı trafikleri (`DIRECT` kalarak) kesinlikle etkilenmez.
*   **Adapter Proxy (Güvenlik Kalkanı):** Kötü niyetli yazılımların veya web sitelerinin yerel proxy'i kullanarak iç ağa saldırmasını (SSRF) engellemek için özel bir TCP kalkanı yazıldı. `127.0.0.1`, `localhost` veya `192.168.x.x` gibi özel IP'lere veya Discord harici herhangi bir hedefe gelen bağlantı anında reddedilir (`403 Forbidden`).
*   **SpoofDPI Motoru Entegrasyonu:** TLS fragmentasyonu için doğrudan Go ile derlenmiş orijinal ve güvenilir `SpoofDPI` (v1.2.1) kullanıldı. Windows'ta soket kilitlenmelerini önlemek için özel bir patch (yama) uygulandı.

## 2. Çökme Koruması ve Otomatik Onarım (Crash Recovery)
Windows Proxy (WinINET) ayarlarını değiştirmek tehlikeli bir işlemdir. Uygulama aniden kapanırsa internet kesilebilir. Bunu tamamen çözdük:
*   **İzole JSON Oturum (Session) Yönetimi:** Windows proxy ayarları değiştirilmeden saniyeler önce `session.json` dosyasına orijinal ayarlar yedeklenir. 
*   **Güvenli Başlangıç (Fail Open):** Uygulama aniden çökerse (Örn: Görev Yöneticisinden zorla kapatılırsa), bir sonraki açılışta bu kirli oturum anında tespit edilir. Uygulama interneti eski haline getirene kadar yeni bağlantıya izin vermez.
*   **Bozuk Dosya Koruması:** Sistem gücü aniden kesilir ve JSON dosyası bozulursa (corrupted), Rust motoru çökmez. Bozuk dosyayı `session.corrupt.<TARİH>.json` olarak karantinaya alır ve sistemin kilitlenmesini engeller.

## 3. Tauri + React + Vite Masaüstü Arayüzü (Frontend)
Kullanıcıya güven veren, modern, karanlık (dark mode) ve minimal bir masaüstü uygulaması sıfırdan inşa edildi.
*   **Thin Client (İnce İstemci) Mimarisi:** Arayüz, kayıt defterine (registry) müdahale etmez, exe çalıştırmaz. Sadece Rust backend ile TypeScript üzerinden haberleşir. Tüm ağır işleri `ozii-core` yapar.
*   **Canlı Metrikler:** Arayüzde anlık olarak Discord'a giden bağlantı sayısı ve başarıyla engellenen Discord harici bağlantılar canlı olarak izlenir.
*   **Durum Yönetimi:** `Starting`, `Connected`, `Faulted` ve `Recovering` durumları UI üzerinde anında renk kodlarıyla (Yeşil, Sarı, Kırmızı) güncellenir.
*   **Güvenli Kapanış:** Uygulama penceresi (X) ile kapatıldığında, Tauri `RunEvent::ExitRequested` tetiklenir, arka plandaki Rust motoruna güvenli durdurma sinyali gönderilir, Windows proxy ayarları orijinal haline çevrilir ve sonrasında yazılım kapanır.

## 4. Tam Otomatik Derleme ve Test Süreçleri
Geliştirme, test ve dağıtım süreçleri (CI/CD mantığıyla) tamamen otomatikleştirildi. Proje kök dizinindeki `scripts/` klasörüne aşağıdaki PowerShell betikleri eklendi:

*   **`build-all.ps1` (Release Sürümü Alma):** Tek tuşla Go SpoofDPI motorunu derler, Rust CLI ve Core modüllerini derler, Tauri arayüzünü paketler, `dist/` klasörüne kopyalar ve güvenlik doğrulama amaçlı **SHA-256** hash'lerini üretir.
*   **`dev.ps1` (Geliştirici Modu):** Tauri'yi canlı yenileme (hot-reload) özelliğiyle tek komutta başlatır.
*   **`test-all.ps1` (Kalite Kontrol):** Rust (`cargo test`, `fmt`, `clippy`) ve Go (`vet`, `test`) dillerindeki tüm güvenlik ve performans testlerini sırayla çalıştırıp raporlar.
*   **`clean.ps1` (Güvenli Temizleme):** Derleme artıklarını siler ancak kullanıcının kurtarma dosyalarına (LOCALAPPDATA) asla dokunmaz.

## 5. Çıktılar ve Kurulum (Installers)
Tauri yapılandırması başarıyla tamamlandığı için, `build-all.ps1` çalıştığında aşağıdaki dağıtıma hazır kurulum dosyaları otomatik olarak üretilmiştir:
1.  **MSI Windows Kurulum Dosyası (Installer):**
    `main/app/src-tauri/target/release/bundle/msi/app_0.1.0_x64_en-US.msi`
2.  **NSIS Setup Dosyası:** 
    `main/app/src-tauri/target/release/bundle/nsis/app_0.1.0_x64-setup.exe`

## Özet
OziiDPI, basit bir bypass betiğinden çıkıp; mimarisi güçlü, kendi kendini hatalara karşı koruyan, Windows ağ yapısına saygılı, performanslı ve piyasaya sürülmeye/paylaşılmaya tamamen hazır kurumsal kalitede bir masaüstü yazılımı haline gelmiştir. Tüm hedeflenen entegrasyonlar **başarıyla tamamlanmıştır.**

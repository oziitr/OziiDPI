# OziiDPI

OziiDPI, Discord masaüstü uygulamasını ve kullanıcı istediğinde normal Google
Chrome profilini yerel DPI tüneline bağlayan Windows uygulamasıdır. Windows'un
genel proxy ayarını değiştirmez; oyunlar ve diğer uygulamalar normal bağlantılarını
kullanmaya devam eder.

> Sürüm: 1.1.1 · Platform: Windows x64 · Yapımcı: ozii

## Özellikler

- Discord için otomatik süreç seçimi, modül denetimi ve onarım
- Her başlatmada güncel Discord sürümünü yeniden bulma; kayıt defteri, kısayol ve kurulum dizini taraması
- Yeni modül dizini (`modules/discord_*-<revision>/`) desteği; aynı sürümün kendi dosyalarıyla onarım
- Discord'a süreç parametresiyle uygulama bazlı proxy
- Normal Chrome profiline isteğe bağlı uzantı üzerinden proxy
- Chrome'u ve açık sekmeleri kapatmadan tüneli açıp kapatma
- Windows başlangıcında otomatik çalışma
- Pencere kapatıldığında sistem tepsisinde çalışmaya devam etme
- Tek uygulama örneği ve kullanıcı başına kurulum

## Kurulum

1. GitHub'daki **Releases** bölümünden `OziiDPI-Setup.exe` dosyasını indirin.
2. Kurulumu tamamlayıp OziiDPI'yi açın.
3. **DISCORD'U BAŞLAT** düğmesine basın. Windows başlangıcında çalıştırma etkinse Discord otomatik açılır.

Kurulum yönetici yetkisi istemez ve varsayılan olarak
`%LOCALAPPDATA%\Programs\OziiDPI` klasörünü kullanır.

### Normal Chrome profili

Chrome desteği isteğe bağlıdır ve ayrı bir Chrome profili oluşturmaz.

1. OziiDPI içinde **NORMAL CHROME'U BAĞLA** düğmesine basın.
2. Açılan `chrome://extensions` sayfasında **Geliştirici modu** seçeneğini açın.
3. **Paketlenmemiş öğe yükle** seçeneğiyle kurulum klasöründeki
   `chrome-extension` klasörünü bir kez seçin.
4. OziiDPI anahtarından Chrome tünelini istediğiniz zaman açıp kapatın.

OziiDPI kapatıldığında uzantı Chrome proxy ayarını temizler.

## Nasıl çalışır?

```text
Discord.exe ───────────────┐
                           ├─> 127.0.0.1:39572 ─> yerel DPI motoru ─> internet
Chrome + OziiDPI uzantısı ─┘

Oyunlar ve diğer uygulamalar ────────────────────────────────> normal bağlantı
```

Yerel backend yalnız `127.0.0.1:39572` üzerinde dinler. Uygulama Windows Internet
Settings, WinHTTP, hosts, DNS, güvenlik duvarı veya yönlendirme tablosunu global
olarak değiştirmez.

## Kaynaktan derleme

Gereksinimler:

- Windows 10/11 x64
- Rust stable (MSVC toolchain), Visual Studio C++ Build Tools ve Windows SDK
- Node.js 22 LTS (22.12 veya sonrası) ve npm
- Git ve Go
- Inno Setup 6.7 veya üzeri

Önce arayüz bağımlılıklarını kurup motoru derleyin:

```powershell
npm --prefix app ci
powershell -NoProfile -ExecutionPolicy Bypass -File .\engine\build.ps1
```

Uygulama ve backend denetimleri:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\test-all.ps1
```

Kurulum paketini oluşturun:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-installer.ps1
```

Kurulum dosyası `dist\installer\OziiDPI-Setup.exe` konumunda oluşur.

### Discord onarım varlıkları

Discord modülleri kaynak depoya ve yeni kurulum paketine dahil edilmez. Uygulama,
kurulu Discord'un aynı sürüme ait dosyalarını kullanır; güncelleme sonrası yeni
`app-*` dizinini otomatik seçer. Eski sürümün yerel modülleri yeni sürüme kopyalanmaz.
Kurulum gerçekten eksikse uygulama hangi modülün bulunamadığını bildirir.

### Bilinen sınırlar

Discord'un yerel güncelleyicisi açılış sırasında geçici olarak atlanır; dosyanın
orijinali geri yüklenir. Bu nedenle uygulama içinde otomatik Discord güncellemesi
garanti edilmez. Ayrı olarak kurulmuş bir Discord güncellemesi bir sonraki
başlatmada bulunur. Bu araç tam cihaz VPN'i veya UDP/ses tüneli değildir.
Discord gelecekte başlatıcı veya modül biçimini değiştirirse uyumluluk güncellemesi
gerekebilir. `docs/` içindeki eski 0.1.0 raporları tarihsel kayıtlardır; güncel
kullanım için bu README esas alınmalıdır.

## Proje yapısı

- `app/`: React + Tauri masaüstü arayüzü
- `backend/`: Rust yerel servis ve komut satırı aracı
- `chrome-extension/`: Normal Chrome profili entegrasyonu
- `engine/`: Sabitlenmiş SpoofDPI motoru derleme betiği
- `installer/`: Inno Setup kurulum tanımı
- `scripts/`: Derleme, paketleme ve test betikleri
- `docs/`: Mimari ve sorun giderme belgeleri

## Üçüncü taraf bileşenleri

DPI motoru SpoofDPI v1.2.1'in sabitlenmiş bir sürümünden derlenir. Kullanılan
bileşenler ve lisansları [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)
dosyasında listelenmiştir.

Bu depo için ayrıca bir açık kaynak lisansı belirtilmemiştir.

## Sorumluluk

Bu yazılım yalnız kendi cihazınızda ve bulunduğunuz yerdeki kurallara uygun
şekilde kullanılmalıdır. Ağ sağlayıcınızın veya hizmetlerin kullanım koşullarını
kontrol etmek kullanıcının sorumluluğundadır.

Yapımcı: ozii

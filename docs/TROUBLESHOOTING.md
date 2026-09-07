# OziiDPI Sorun Giderme (Troubleshooting)

## Engine Missing (Motor Bulunamadı)

**Belirti:** Bağlanmaya çalışırken `ENGINE_MISSING` hatası.

**Çözüm:**
- OziiDPI'ı yeniden kurun. Kurulum sırasında `ozii-dpi-engine.exe` dosyası paketlenir.
- Antivirüs yazılımınız motoru karantinaya almış olabilir — istisna ekleyin.

## Existing PAC Detected (Mevcut PAC Algılandı)

**Belirti:** `EXISTING_PAC_CONFIGURED` hatası.

**Çözüm:**
- Sisteminizde zaten bir otomatik proxy yapılandırma (PAC) dosyası aktif.
- OziiDPI, mevcut PAC ayarlarını bozmamak için bağlantıyı reddeder.
- Mevcut PAC ayarını kaldırın veya devre dışı bırakın: 
  Windows Ayarlar → Ağ ve İnternet → Proxy → Otomatik proxy kurulumu → Kapat

## WPAD Conflict (WPAD Çakışması)

**Belirti:** `WPAD_CONFLICT` hatası.

**Çözüm:**
- Ağınız WPAD (Web Proxy Auto-Discovery) kullanıyor.
- Windows Ayarlar → Proxy → "Otomatik olarak ayarları algıla" seçeneğini kapatın.

## Discord Algılanmıyor

**Belirti:** `Connected` durumda ama Discord trafiği sayaçlarda görünmüyor.

**Çözüm:**
- Discord Desktop'u OziiDPI bağlandıktan **sonra** yeniden başlatın.
- Tarayıcıda Discord kullanıyorsanız, tarayıcıyı kapatıp yeniden açın.
- Chrome/Edge genellikle sistem proxy'sini otomatik kullanır.

## Advanced Mode Unavailable (Gelişmiş Mod Kullanılamıyor)

**Belirti:** StrongAdvanced modu seçilemiyor.

**Çözüm:**
- Bu mod Npcap gerektirir. https://npcap.com adresinden indirip kurun.

## Recovery Required (Kurtarma Gerekli)

**Belirti:** Uygulama açıldığında sarı uyarı banner'ı.

**Çözüm:**
- Önceki oturum düzgün kapatılmamış. **Recover** butonuna tıklayın.
- Sistem proxy ayarları otomatik olarak orijinal haline döner.

## Tarayıcı PAC Önbelleği Sorunu

**Belirti:** OziiDPI bağlandıktan sonra Discord hâlâ doğrudan bağlanıyor.

**Çözüm:**
- Tarayıcıyı tamamen kapatıp yeniden açın.
- Chrome'da `chrome://net-internals/#proxy` adresinden proxy durumunu kontrol edin.

## Firefox Desteği

Firefox kendi proxy ayarlarını yönetir ve varsayılan olarak Windows sistem proxy'sini kullanmaz.

**Çözüm:**
- Firefox → Ayarlar → Genel → Ağ Ayarları → Ayarlar → "Sistem proxy ayarlarını kullan" seçin.

## Internet Tamamen Kesildi

**Belirti:** OziiDPI çöktükten sonra hiçbir site açılmıyor.

**Çözüm:**
1. OziiDPI'ı yeniden açın → **Recover** butonuna tıklayın.
2. Eğer uygulama açılmıyorsa:
   - Windows Ayarlar → Ağ ve İnternet → Proxy
   - "Kurulum betiği kullan" → Kapat
   - "El ile proxy kurulumu" → Kapat
   
> ⚠️ Bu durum nadirdir. OziiDPI'ın kurtarma sistemi çoğu durumda otomatik olarak çalışır.

## SmartScreen Uyarısı

OziiDPI henüz kod imzası taşımıyor. Windows SmartScreen "Bilinmeyen uygulama" uyarısı gösterebilir.

**Çözüm:** "Yine de çalıştır" seçeneğini tıklayın. Gelecek sürümlerde kod imzası eklenecektir.

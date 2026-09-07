# OziiDPI Kullanım Kılavuzu (User Install Guide)

## Kurulum

1. `OziiDPI_0.1.0_x64-setup.exe` (NSIS) veya `OziiDPI_0.1.0_x64_en-US.msi` (MSI) dosyasını çalıştırın.
2. Windows SmartScreen uyarısı çıkabilir (henüz imzalanmamış). "Yine de çalıştır" seçin.
3. Kurulum sihirbazını takip edin. Varsayılan konum: `C:\Program Files\OziiDPI\`

## İlk Kullanım

1. Başlat Menüsü veya Masaüstünden **OziiDPI** uygulamasını açın.
2. Ana ekranda durum göstergesi **Disconnected** olarak görünecektir.

## Bağlanma (Connect)

1. **Connect** butonuna tıklayın.
2. Durum sırasıyla şu şekilde değişir:
   - `Disconnected` → `Starting` → `Connected`
3. **Connected** durumunda yalnızca Discord trafiği DPI bypass'tan geçer.
4. Diğer tüm internet trafiğiniz (oyunlar, YouTube, tarayıcı vb.) etkilenmez.

## Mod Seçimi

- **Turbo**: SNI split — en hafif mod.
- **Balanced**: Chunk split — dengeli performans (varsayılan).
- **Strong**: 1-byte chunk — güçlü DPI bypass.
- **StrongAdvanced**: Fake paketli bypass — Npcap gerektirir.

## Bağlantıyı Kesme (Disconnect)

1. **Disconnect** butonuna tıklayın.
2. Durum: `Connected` → `Stopping` → `Disconnected`
3. Windows proxy ayarları otomatik olarak eski haline döner.

## Kurtarma (Recovery)

Eğer uygulama anormal şekilde kapanırsa (örneğin Görev Yöneticisinden zorla kapatma):

1. Bir sonraki açılışta sarı bir **uyarı banner'ı** görünür.
2. **Recover** butonuna tıklayın.
3. Sistem, Windows proxy ayarlarını orijinal haline geri yükler.

## Kaldırma (Uninstall)

1. Önce OziiDPI bağlantısını kesin (Disconnect).
2. Windows Ayarlar → Uygulamalar → OziiDPI → Kaldır.
3. Veya Denetim Masası → Program Kaldır → OziiDPI.

## Önemli Notlar

- OziiDPI yalnızca **Discord** trafiğini yönlendirir.
- Ses (Voice) trafiği UDP kullandığı için bu sürümde kapsam dışıdır.
- Firefox kendi proxy ayarlarını kullanır; sistem PAC'ını otomatik takip etmeyebilir.
- Discord Desktop uygulamasını OziiDPI bağlandıktan **sonra** yeniden başlatmanız önerilir.

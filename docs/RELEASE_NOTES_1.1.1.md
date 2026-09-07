# OziiDPI 1.1.1

Yapımcı: ozii

## Discord güncellemeleri ve taşınmış kurulumlar

- Discord her başlatmada yeniden aranır. Yaygın kurulum dizinleri, Windows kayıt
  defteri, Discord kısayolları ve son başarılı kurulum kökü kontrol edilir.
- Sabit `app-1.0.9255` tercihi kaldırıldı. Geçerli kurulumlar sayısal sürüm
  sırasıyla değerlendirilir; eksik bir sürüm varsa diğer kurulu sürüm denenir.
- Yeni `modules/discord_*-<revision>/discord_*` dizin düzeni desteklenir.
- Onarım aynı Discord sürümünün kendi dosyalarıyla yapılır. Başka bir sürümün
  native modülleri kopyalanmaz. Mevcut sağlıklı modül kayıtları korunur.
- Başlatma hataları tepsi/otomatik başlatmada da durum alanına yansır.
- Kurulum paketinde Discord'a ait modüller dağıtılmaz.
- Backend derleme yolu kullanıcı adına veya masaüstü konumuna bağlı değildir.

## Doğrulama

- Discord 1.0.9256 canlı başlatıldı: ana pencere ve Gateway READY doğrulandı.
- Discord ana sürecinde yerel proxy parametresi doğrulandı.
- Windows genel proxy kapalı, WinHTTP doğrudan bağlantıda kaldı.
- 6 Discord keşif/onarım testi ve 19 backend testi geçti; mevcut 2 backend
  soket testi kararsız oldukları için önceden devre dışıdır.
- Chrome uzantısının normal profile kurulumu bu sürümde otomatik yapılmaz.

Discord'un gelecekteki tüm dağıtım biçimleri ve ağ koşulları için kesintisiz
çalışma garantisi verilmez. Ayrıntılar ana README'dedir.

OziiDPI - Discord + Chrome (Uygulama Bazli)
================================================

Yapimci: ozii

1. OziiDPI.exe dosyasini acin.
2. Discord icin "DISCORD'U BASLAT" dugmesine basin.
3. Normal Chrome icin "Normal Chrome'u Bagla" dugmesine basin.
4. Chrome uzantilar sayfasinda Gelistirici modu > Paketlenmemis oge yukle ile
   uygulamanin chrome-extension klasorunu bir kez secin.

Kurulum surumunde OziiDPI Windows ile otomatik baslar ve Discord'u otomatik
olarak tunelden acar. Pencerenin X dugmesi uygulamayi kapatmaz; uygulama sistem
tepsisinde calismaya devam eder. Sistem tepsisi menusunden Discord veya normal
Chrome tuneli acilip kapatilabilir.

Yonetici onayi gerekmez. Paket WinDivert koruyucusunu baslatmaz.
Yalnizca launcher'dan acilan uygulamalara --proxy-server ve --disable-quic
parametreleri verilir.

Discord onarimi:
- Eksik Discord modulleri launcher tarafindan kontrol edilir.
- Kurulum dizinleri, kayit defteri ve Discord kisayollari her acilista taranir.
- Guncelleme sonrasi yeni app-* dizini otomatik bulunur.
- Kurulu Discord'un ayni surume ait modules/discord_*-<revision> dosyalari kullanilir.
- Eski Discord surumunun modulleri yeni surume kopyalanmaz.
- Discord'un engellenen yerel guncelleyicisi yalniz baslangic sirasinda gecici
  olarak atlanir.
- resources\build_info.json dosyasinin orijinal baytlari iki saniye sonra geri
  yuklenir. Kesinti olursa bir sonraki acilista yedekten otomatik kurtarilir.

Chrome:
- Ayri profil veya yeni bir Chrome kurulumu olusturulmaz.
- Normal Chrome ve acik sekmeler kapatilmadan tunel acilip kapatilir.
- OziiDPI kapanirsa uzanti Chrome proxy ayarini otomatik temizler.
- Chrome guvenligi nedeniyle uzanti klasoru kullanici tarafindan bir kez
  chrome://extensions sayfasindan onaylanmalidir.

Program sunlari DEGISTIRMEZ:
- Windows sistem proxy ayarlari
- Registry Internet Settings
- WinHTTP proxy
- hosts veya DNS
- firewall veya routing
- global HTTP_PROXY / HTTPS_PROXY ortam degiskenleri

Oyunlar ve diger programlar DIRECT kalir.

Discord modullerini ayrica paketlemek gerekmez; Discord'un kurulu olmasi gerekir.
ozii-cli.exe ve ozii-dpi-engine.exe gerekli yardimci dosyalardir.

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import {
  Activity,
  Globe,
  Home,
  LogOut,
  Power,
  Settings as SettingsIcon,
  ShieldCheck,
} from 'lucide-react';

type View = 'main' | 'settings' | 'diagnostics';

type PerAppStatus = {
  backend_ready: boolean;
  discord_running: boolean;
  proxy_url: string;
  last_message: string;
};

type ChromeTunnelStatus = {
  enabled: boolean;
  extension_connected: boolean;
  effective: boolean;
  extension_path: string;
};

const emptyStatus: PerAppStatus = {
  backend_ready: false,
  discord_running: false,
  proxy_url: 'http://127.0.0.1:39572',
  last_message: 'Hazır',
};

const emptyChromeStatus: ChromeTunnelStatus = {
  enabled: false,
  extension_connected: false,
  effective: false,
  extension_path: '',
};

export function MainScreen() {
  const [status, setStatus] = useState<PerAppStatus>(emptyStatus);
  const [chromeTunnel, setChromeTunnel] = useState<ChromeTunnelStatus>(emptyChromeStatus);
  const [view, setView] = useState<View>('main');
  const [autostart, setAutostart] = useState(false);
  const [busy, setBusy] = useState<'discord' | 'chrome' | 'stop' | null>(null);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);
  const [notice, setNotice] = useState('Hazır');
  const isTauri = !!(window as any).__TAURI_INTERNALS__;

  const refresh = async () => {
    if (!isTauri) return;
    try {
      const [current, chrome] = await Promise.all([
        invoke<PerAppStatus>('get_app_status'),
        invoke<ChromeTunnelStatus>('get_chrome_tunnel_status'),
      ]);
      setStatus(current);
      setChromeTunnel(chrome);
      if (current.last_message) {
        setNotice(current.last_message);
        if (current.last_message.startsWith('Hata: ')) setErrorMsg(current.last_message.slice(6));
      }
    } catch (error) {
      setErrorMsg(String(error));
    }
  };

  useEffect(() => {
    if (!isTauri) return;
    invoke<boolean>('get_auto_start').then(setAutostart).catch(() => {});
    refresh();
    const timer = window.setInterval(refresh, 1200);
    return () => window.clearInterval(timer);
  }, []);

  const run = async (command: 'launch_discord' | 'stop_discord') => {
    setErrorMsg(null);
    setBusy(command === 'stop_discord' ? 'stop' : 'discord');
    setNotice(
      command === 'stop_discord'
        ? 'Discord kapatılıyor...'
        : 'Discord ve yerel tünel hazırlanıyor...',
    );
    try {
      const message = await invoke<string>(command);
      setNotice(message);
      await refresh();
    } catch (error) {
      setErrorMsg(String(error));
    } finally {
      setBusy(null);
    }
  };

  const toggleChromeTunnel = async () => {
    setErrorMsg(null);
    setBusy('chrome');
    try {
      if (!chromeTunnel.extension_connected) {
        if (!chromeTunnel.enabled) {
          await invoke<string>('set_chrome_tunnel', { enabled: true });
        }
        const message = await invoke<string>('prepare_chrome_extension');
        setNotice(message);
      } else {
        const message = await invoke<string>('set_chrome_tunnel', {
          enabled: !chromeTunnel.enabled,
        });
        setNotice(message);
      }
      await refresh();
    } catch (error) {
      setErrorMsg(String(error));
    } finally {
      setBusy(null);
    }
  };

  const toggleAutostart = async (enabled: boolean) => {
    try {
      await invoke('set_auto_start', { enabled });
      setAutostart(enabled);
      setNotice(`Windows başlangıcında çalışma ${enabled ? 'açıldı' : 'kapatıldı'}`);
    } catch (error) {
      setErrorMsg(String(error));
    }
  };

  const active = status.backend_ready && (status.discord_running || chromeTunnel.effective);
  const processing = busy !== null;
  const badgeClass = active ? 'active' : processing ? 'processing' : 'passive';
  const shieldClass = active ? 'connected' : processing ? 'processing' : '';
  const statusLabel = active ? 'KORUMA AKTİF' : processing ? 'HAZIRLANIYOR' : 'HAZIR';

  return (
    <div className="app-container">
      {!isTauri && <div className="dev-banner">Tauri uygulamasıyla çalıştırın</div>}

      <header className="app-header">
        <div className="brand">
          <ShieldCheck size={20} color="var(--accent-blue)" />
          <span className="brand-name">OZIIDPI</span>
        </div>
        <div className={`status-badge ${badgeClass}`}>
          <span className="status-dot" />
          <span>{statusLabel}</span>
        </div>
      </header>

      {view === 'main' && (
        <main className="main-content">
          <div className={`shield-circle ${shieldClass}`}>
            <ShieldCheck size={56} strokeWidth={1.6} />
          </div>

          <div className="status-text">
            <h1 className={`status-title ${shieldClass}`}>{statusLabel}</h1>
            <p className="status-desc">
              {status.discord_running && chromeTunnel.effective
                ? 'Discord ve normal Chrome uygulama bazlı tünelden bağlanıyor'
                : status.discord_running
                  ? 'Discord yalnız uygulama bazlı tünelden bağlanıyor'
                  : chromeTunnel.effective
                    ? 'Normal Chrome kapanmadan uygulama bazlı tüneli kullanıyor'
                    : 'Oyunlar ve Windows ağı doğrudan bağlantıda kalır'}
            </p>
            {status.backend_ready && <span className="port-pill">{status.proxy_url}</span>}
          </div>

          <div className="scope-note">{notice}</div>

          {errorMsg && (
            <div className="error-banner">
              <strong>Hata:</strong> {errorMsg}
            </div>
          )}

          <button
            className={`main-btn ${status.discord_running ? 'disconnect' : 'connect'}`}
            onClick={() => run(status.discord_running ? 'stop_discord' : 'launch_discord')}
            disabled={processing || !isTauri}
          >
            <Power size={20} strokeWidth={2.5} />
            {busy === 'discord'
              ? 'DISCORD AÇILIYOR'
              : busy === 'stop'
                ? 'KAPATILIYOR'
                : status.discord_running
                  ? 'DISCORD\'U KAPAT'
                  : 'DISCORD\'U BAŞLAT'}
          </button>

          <button
            className={`main-btn ${chromeTunnel.effective ? 'disconnect' : 'secondary-btn'}`}
            onClick={toggleChromeTunnel}
            disabled={processing || !isTauri}
          >
            <Globe size={20} strokeWidth={2.2} />
            {busy === 'chrome'
              ? 'CHROME AYARLANIYOR'
              : !chromeTunnel.extension_connected
                ? 'NORMAL CHROME\'U BAĞLA'
                : chromeTunnel.enabled
                  ? 'CHROME TÜNELİNİ KAPAT'
                  : 'CHROME TÜNELİNİ AÇ'}
          </button>

          <div className="stats-row">
            <div className="stat-chip">
              <span className="stat-label">Discord</span>
              <span className={`stat-value ${status.discord_running ? 'ok' : ''}`}>
                {status.discord_running ? 'AÇIK' : 'KAPALI'}
              </span>
            </div>
            <div className="stat-chip">
              <span className="stat-label">Chrome</span>
              <span className={`stat-value ${chromeTunnel.effective ? 'ok' : ''}`}>
                {chromeTunnel.effective ? 'TÜNEL' : chromeTunnel.extension_connected ? 'DİREKT' : 'BAĞLA'}
              </span>
            </div>
            <div className="stat-chip">
              <span className="stat-label">Oyunlar</span>
              <span className="stat-value ok">DIRECT</span>
            </div>
          </div>
        </main>
      )}

      {view === 'settings' && (
        <main className="panel-view">
          <h3 className="section-title">UYGULAMA AYARLARI</h3>

          <div className="toggle-row">
            <label htmlFor="autostart">
              Başlangıçta çalıştır
              <small>Windows açılışında tepsiye yerleşir ve Discord'u tünelden açar</small>
            </label>
            <label className="switch">
              <input
                id="autostart"
                type="checkbox"
                checked={autostart}
                onChange={(event) => toggleAutostart(event.target.checked)}
              />
              <span className="slider" />
            </label>
          </div>

          <div className="info-card">
            <strong>Otomatik Discord bulma ve onarım</strong>
            <small>
              Her açılışta kurulum dizinleri, kayıt defteri ve kısayollar yeniden taranır.
              Güncellenen Discord'un kendi modülleri kullanılır; eski bir sürüme sabitlenmez.
            </small>
          </div>
          <div className="info-card">
            <strong>Uygulama bazlı ağ</strong>
            <small>
              Windows proxy, WinHTTP, DNS, hosts, güvenlik duvarı ve oyun bağlantıları değiştirilmez.
            </small>
          </div>
          <div className="info-card">
            <strong>Normal Chrome entegrasyonu</strong>
            <small>
              Bir kez uzantı bağlandıktan sonra mevcut Chrome ve sekmeler kapanmadan tünel açılıp
              kapatılır. OziiDPI kapanırsa Chrome eski ayarına otomatik döner.
            </small>
          </div>
          <div className="info-card">
            <strong>Sistem tepsisi</strong>
            <small>Pencerenin X düğmesi OziiDPI'yi kapatmaz; uygulama tepside çalışır.</small>
          </div>

          <div className="creator-mark">Yapımcı: ozii</div>
        </main>
      )}

      {view === 'diagnostics' && (
        <main className="panel-view">
          <h3 className="section-title">CANLI DURUM</h3>
          <div className="diag-list">
            <div className="diag-row"><span>Yerel tünel</span><b>{status.backend_ready ? 'HAZIR' : 'BEKLİYOR'}</b></div>
            <div className="diag-row"><span>Discord</span><b>{status.discord_running ? 'AÇIK' : 'KAPALI'}</b></div>
            <div className="diag-row"><span>Normal Chrome</span><b>{chromeTunnel.effective ? 'TÜNEL' : 'DİREKT'}</b></div>
            <div className="diag-row"><span>Chrome uzantısı</span><b>{chromeTunnel.extension_connected ? 'BAĞLI' : 'BEKLİYOR'}</b></div>
            <div className="diag-row"><span>Proxy</span><b>{status.proxy_url}</b></div>
            <div className="diag-row"><span>Kapsam</span><b>Discord + normal Chrome</b></div>
            <div className="diag-row"><span>Windows genel proxy</span><b>DEĞİŞMEZ</b></div>
            <div className="diag-row"><span>Oyunlar</span><b>DIRECT</b></div>
            <div className="diag-row"><span>Sürüm</span><b>1.1.1</b></div>
            <div className="diag-row"><span>Yapımcı</span><b>ozii</b></div>
          </div>
          <button className="main-btn secondary-btn" onClick={refresh}>
            DURUMU YENİLE
          </button>
        </main>
      )}

      <nav className="bottom-nav">
        <button className={`nav-btn ${view === 'main' ? 'active' : ''}`} onClick={() => setView('main')}>
          <Home size={17} />
          <span>Ana Ekran</span>
        </button>
        <div className="nav-divider" />
        <button className={`nav-btn ${view === 'diagnostics' ? 'active' : ''}`} onClick={() => setView('diagnostics')}>
          <Activity size={17} />
          <span>Tanı</span>
        </button>
        <div className="nav-divider" />
        <button className={`nav-btn ${view === 'settings' ? 'active' : ''}`} onClick={() => setView('settings')}>
          <SettingsIcon size={17} />
          <span>Ayarlar</span>
        </button>
        <div className="nav-divider" />
        <button className="nav-btn exit" onClick={() => invoke('quit_app')}>
          <LogOut size={17} />
          <span>Çıkış</span>
        </button>
      </nav>
    </div>
  );
}

const HOST_NAME = 'com.ozii.dpi';
const PROXY_CONFIG = {
  mode: 'fixed_servers',
  rules: {
    singleProxy: {
      scheme: 'http',
      host: '127.0.0.1',
      port: 39572,
    },
    bypassList: ['<local>', 'localhost', '127.0.0.1'],
  },
};

let nativePort = null;
let pollTimer = null;
let reconnectTimer = null;
let proxyApplied = false;

async function updateIndicator(state, error = false) {
  const text = error ? '!' : state ? 'ON' : 'OFF';
  const color = error ? '#ef4444' : state ? '#10b981' : '#64748b';
  await chrome.action.setBadgeText({ text });
  await chrome.action.setBadgeBackgroundColor({ color });
  await chrome.action.setTitle({
    title: error
      ? 'OziiDPI bağlantısı bekleniyor'
      : state
        ? 'OziiDPI normal Chrome tüneli açık'
        : 'OziiDPI normal Chrome tüneli kapalı',
  });
}

async function applyProxy(enabled) {
  try {
    if (enabled) {
      await chrome.proxy.settings.set({ value: PROXY_CONFIG, scope: 'regular' });
    } else {
      await chrome.proxy.settings.clear({ scope: 'regular' });
    }
    proxyApplied = enabled;
    await updateIndicator(enabled);
  } catch (_) {
    proxyApplied = false;
    await updateIndicator(false, true);
  }
}

function requestState() {
  if (!nativePort) return;
  try {
    nativePort.postMessage({ type: 'get_state' });
  } catch (_) {
    disconnect();
  }
}

function disconnect() {
  if (pollTimer) clearInterval(pollTimer);
  pollTimer = null;
  nativePort = null;
  void applyProxy(false);
  if (!reconnectTimer) {
    reconnectTimer = setTimeout(() => {
      reconnectTimer = null;
      connect();
    }, 1500);
  }
}

function connect() {
  if (nativePort) return;
  try {
    nativePort = chrome.runtime.connectNative(HOST_NAME);
    nativePort.onMessage.addListener((message) => {
      void applyProxy(Boolean(message?.enabled));
    });
    nativePort.onDisconnect.addListener(disconnect);
    pollTimer = setInterval(requestState, 1000);
    requestState();
  } catch (_) {
    disconnect();
  }
}

chrome.proxy.onProxyError.addListener(() => {
  void updateIndicator(proxyApplied, true);
});

chrome.runtime.onInstalled.addListener(connect);
chrome.runtime.onStartup.addListener(connect);
connect();


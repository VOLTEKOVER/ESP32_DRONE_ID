'use strict';
const { app, BrowserWindow, ipcMain, dialog } = require('electron');
const path = require('path');
const fs = require('fs');
const https = require('https');
const { autoUpdater } = require('electron-updater');
const { Tracker } = require('./src/tracker');
const { WiFiCapture, BLECapture, SerialCapture } = require('./src/capture');
const { extractOdidFromBeacon, formatSummary, decodeOdidMessage } = require('./src/decoder');

let mainWindow = null;
const tracker = new Tracker();
const wifiCapture = new WiFiCapture();
const bleCapture = new BLECapture();
const serialCapture = new SerialCapture();

let captureSources = [];

function emitToRenderer(channel, data) {
  if (mainWindow && !mainWindow.isDestroyed()) {
    mainWindow.webContents.send(channel, data);
  }
}

function onCaptureData(data) {
  const clean = tracker.onCapture(data);
  const snap = tracker.getSnapshot();
  const devices = snap.devices.map(d => ({
    mac: d.mac,
    basicId: d.basic_id,
    operatorId: d.operator_id,
    uaType: d.ua_type,
    packetsCount: d.packet_count,
    rssiAvg: d.rssi_avg,
    rssiLast: d.rssi_last,
    rssiMin: d.rssi_min,
    rssiMax: d.rssi_max,
    firstSeen: d.first_seen * 1000,
    lastSeen: d.last_seen * 1000,
    lat: d.last_location?.latitude || 0,
    lon: d.last_location?.longitude || 0,
    altitude: d.last_location?.altitude_geodetic ?? null,
    altitudePressure: d.last_location?.altitude_pressure ?? null,
    height: d.last_location?.height ?? null,
    speed: d.last_location?.speed_horizontal ?? null,
    verticalSpeed: d.last_location?.speed_vertical ?? null,
    direction: d.last_location?.direction ?? null,
    status: d.last_location?.status ?? null,
    selfId: d.self_id,
    trail: d.location_trail.map(p => ({
      lat: p.lat, lon: p.lon, timestamp: p.ts * 1000, rssi: null,
    })),
    seenMessages: d.messages_seen || [],
  }));
  emitToRenderer('rid-packet', {
    raw: clean,
    snapshot: devices,
    totalPackets: snap.stats.total_packets,
    sessionPackets: snap.stats.session_packets,
  });
}

wifiCapture.onPacket = onCaptureData;
bleCapture.onPacket = onCaptureData;
serialCapture.onPacket = onCaptureData;

function createWindow() {
  const isMac = process.platform === 'darwin';
  mainWindow = new BrowserWindow({
    width: 1280, height: 860, minWidth: 800, minHeight: 600,
    title: 'OmniRID Desktop',
    frame: isMac,
    titleBarStyle: isMac ? 'hidden' : 'default',
    ...(isMac ? { trafficLightPosition: { x: 12, y: 8 } } : {}),
    webPreferences: {
      preload: path.join(__dirname, 'preload.js'),
      contextIsolation: true,
      nodeIntegration: false,
    },
  });
  mainWindow.loadFile(path.join(__dirname, 'index.html'));
  mainWindow.on('closed', () => { mainWindow = null; });
  mainWindow.on('maximize', () => { mainWindow?.webContents.send('window-state', true); });
  mainWindow.on('unmaximize', () => { mainWindow?.webContents.send('window-state', false); });
}

let updateInterval = null;

// === PAGE CACHE (auto-sync docs/ from GitHub) ===
const PAGE_CACHE_URLS = {
  landing: 'https://raw.githubusercontent.com/valeriogiacomo/ESP32_DRONE_ID/main/docs/index.html',
  guide: 'https://raw.githubusercontent.com/valeriogiacomo/ESP32_DRONE_ID/main/docs/guide.html',
  configure: 'https://raw.githubusercontent.com/valeriogiacomo/ESP32_DRONE_ID/main/docs/config(demo).html',
};

const PAGE_CACHE_DIR = path.join(app.getPath('userData'), 'page-cache');

function ensurePageCacheDir() {
  if (!fs.existsSync(PAGE_CACHE_DIR)) {
    fs.mkdirSync(PAGE_CACHE_DIR, { recursive: true });
  }
}

function httpGetText(url) {
  return new Promise((resolve, reject) => {
    https.get(url, { headers: { 'User-Agent': 'RID-Hub' } }, (res) => {
      if (res.statusCode !== 200) {
        reject(new Error(`HTTP ${res.statusCode}`));
        return;
      }
      let body = '';
      res.on('data', d => body += d);
      res.on('end', () => resolve(body));
    }).on('error', reject);
  });
}

async function updatePageCache(name) {
  const url = PAGE_CACHE_URLS[name];
  if (!url) return { error: 'unknown page' };
  try {
    const html = await httpGetText(url);
    ensurePageCacheDir();
    const filePath = path.join(PAGE_CACHE_DIR, `${name}.html`);
    fs.writeFileSync(filePath, html, 'utf-8');
    return { success: true, cachedAt: Date.now() };
  } catch (e) {
    return { error: e.message };
  }
}

function getCachedPage(name) {
  const filePath = path.join(PAGE_CACHE_DIR, `${name}.html`);
  if (fs.existsSync(filePath)) {
    return fs.readFileSync(filePath, 'utf-8');
  }
  const fallbackPaths = {
    landing: path.join(__dirname, '..', 'docs', 'index.html'),
    guide: path.join(__dirname, '..', 'docs', 'guide.html'),
    configure: path.join(__dirname, '..', 'docs', 'config(demo).html'),
  };
  const fb = fallbackPaths[name];
  if (fb && fs.existsSync(fb)) {
    return fs.readFileSync(fb, 'utf-8');
  }
  return null;
}

async function refreshAllPages() {
  const results = {};
  for (const name of Object.keys(PAGE_CACHE_URLS)) {
    results[name] = await updatePageCache(name);
  }
  if (mainWindow && !mainWindow.isDestroyed()) {
    mainWindow.webContents.send('page-cache-update', results);
  }
  return results;
}

// === UPDATER ===
autoUpdater.autoDownload = false;
autoUpdater.autoInstallOnAppQuit = true;

function checkForUpdates() {
  if (app.isPackaged) {
    autoUpdater.checkForUpdates().catch(() => {});
  } else {
    const repo = 'VOLTEKOVER/ESP_DRONE_REMOTEID';
    https.get(`https://api.github.com/repos/${repo}/releases/latest`, {
      headers: { 'User-Agent': 'RID-Hub' },
    }, (res) => {
      let body = '';
      res.on('data', d => body += d);
      res.on('end', () => {
        try {
          const rel = JSON.parse(body);
          const tag = rel.tag_name || '';
          const current = app.getVersion();
          const hasUpdate = tag.replace(/^v/i,'') !== current;
          mainWindow?.webContents.send('update-checked', {
            hasUpdate, current, latest: tag,
            url: rel.html_url, body: rel.body,
          });
        } catch (_) {}
      });
    }).on('error', () => {});
  }
}

autoUpdater.on('checking-for-update', () => {
  mainWindow?.webContents.send('update-status', 'checking');
});
autoUpdater.on('update-available', (info) => {
  mainWindow?.webContents.send('update-available', {
    version: info.version, url: info.files?.[0]?.url,
  });
});
autoUpdater.on('update-not-available', () => {
  mainWindow?.webContents.send('update-status', 'up-to-date');
});
autoUpdater.on('download-progress', (p) => {
  mainWindow?.webContents.send('update-progress', {
    percent: Math.round(p.percent),
    bytesPerSecond: p.bytesPerSecond,
    transferred: p.transferred,
    total: p.total,
  });
});
autoUpdater.on('update-downloaded', () => {
  mainWindow?.webContents.send('update-status', 'downloaded');
});
autoUpdater.on('error', (err) => {
  mainWindow?.webContents.send('update-error', err.message);
});

app.whenReady().then(() => {
  createWindow();
  checkForUpdates();

  // Initial page cache sync (fire-and-forget)
  refreshAllPages();

  // Periodic refresh every 60 minutes
  updateInterval = setInterval(refreshAllPages, 60 * 60 * 1000);

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow();
  });
});

app.on('window-all-closed', () => {
  stopAllCapture();
  if (updateInterval) clearInterval(updateInterval);
  if (process.platform !== 'darwin') app.quit();
});

app.on('before-quit', () => { stopAllCapture(); });

function stopAllCapture() {
  wifiCapture.stop();
  bleCapture.stop();
  serialCapture.stop();
  captureSources = [];
}

ipcMain.handle('get-snapshot', () => tracker.getSnapshot());
ipcMain.handle('get-device-detail', (_, mac) => tracker.getDeviceDetail(mac));
ipcMain.handle('reset-stats', () => { tracker.reset(); return true; });

ipcMain.handle('start-recording', () => { tracker.startRecording(); return true; });
ipcMain.handle('stop-recording', () => { tracker.stopRecording(); return true; });
ipcMain.handle('get-session', () => tracker.sessionPackets);

ipcMain.handle('export-csv', () => tracker.generateCSV());
ipcMain.handle('export-kml', () => tracker.generateKML());

ipcMain.handle('list-ports', async () => {
  try {
    const { SerialPort } = require('serialport');
    const ports = await SerialPort.list();
    return ports.map(p => ({ path: p.path, manufacturer: p.manufacturer || '' }));
  } catch (_) { return []; }
});

ipcMain.handle('connect-serial', async (_, path, baud) => {
  serialCapture.stop();
  await serialCapture.start(path, baud || 115200);
  captureSources.push('serial');
  return true;
});

ipcMain.handle('disconnect-serial', () => {
  serialCapture.stop();
  captureSources = captureSources.filter(s => s !== 'serial');
  return true;
});

ipcMain.handle('start-wifi-capture', (_, iface) => {
  wifiCapture.start(iface);
  captureSources.push('wifi');
  return true;
});

ipcMain.handle('stop-wifi-capture', () => {
  wifiCapture.stop();
  captureSources = captureSources.filter(s => s !== 'wifi');
  return true;
});

ipcMain.handle('start-ble-capture', () => {
  bleCapture.start();
  captureSources.push('ble');
  return true;
});

ipcMain.handle('stop-ble-capture', () => {
  bleCapture.stop();
  captureSources = captureSources.filter(s => s !== 'ble');
  return true;
});

ipcMain.handle('import-pcap', async () => {
  const result = await dialog.showOpenDialog(mainWindow, {
    filters: [{ name: 'PCAP Files', extensions: ['pcap', 'pcapng'] }],
    properties: ['openFile'],
  });
  if (result.canceled || !result.filePaths.length) return;
  const filePath = result.filePaths[0];
  try {
    const pcapModule = require('pcap');
    const parser = new pcapModule.PCAPOffline(filePath);
    parser.on('packet', (raw) => {
      const msgs = extractOdidFromBeacon(Buffer.from(raw));
      if (msgs && msgs.length) {
        const data = {
          timestamp: Date.now() / 1000,
          source_mac: 'pcap',
          rssi: null,
          channel: 0,
          summary: formatSummary(msgs),
          messages: msgs,
        };
        tracker.onCapture(data);
      }
    });
    parser.on('complete', () => { emitToRenderer('rid-pcap-done', {}); });
    return filePath;
  } catch (e) {
    return { error: e.message };
  }
});

ipcMain.handle('get-version', () => app.getVersion());

// === PAGE CACHE IPC ===
ipcMain.handle('get-page-cache', (_, name) => {
  const html = getCachedPage(name);
  return { html, name };
});

ipcMain.handle('refresh-page-cache', async (_, name) => {
  const result = await updatePageCache(name);
  if (result.success) {
    emitToRenderer('page-cache-update', { [name]: result });
  }
  return result;
});

ipcMain.handle('refresh-all-pages', async () => {
  return await refreshAllPages();
});

// === FIRMWARE (GitHub Releases + cache) ===
const FIRMWARE_CACHE_DIR = path.join(app.getPath('userData'), 'firmware-cache');

function ensureFirmwareCache() {
  if (!fs.existsSync(FIRMWARE_CACHE_DIR)) fs.mkdirSync(FIRMWARE_CACHE_DIR, { recursive: true });
}

function githubApiGet(url) {
  return new Promise((resolve, reject) => {
    https.get(url, { headers: { 'User-Agent': 'RID-Hub' } }, (res) => {
      let body = '';
      res.on('data', d => body += d);
      res.on('end', () => { try { resolve(JSON.parse(body)); } catch (_) { reject(new Error('Parse failed')); } });
    }).on('error', reject);
  });
}

ipcMain.handle('check-firmware-releases', async () => {
  try {
    const rel = await githubApiGet('https://api.github.com/repos/VOLTEKOVER/ESP_DRONE_REMOTEID/releases?per_page=5');
    const releases = (Array.isArray(rel) ? rel : [rel]).map(r => ({
      tag: r.tag_name || '',
      name: r.name || r.tag_name || '',
      published: r.published_at || '',
      prerelease: r.prerelease || false,
      assets: (r.assets || []).map(a => ({
        name: a.name, size: a.size, url: a.browser_download_url,
        contentType: a.content_type,
      })),
    }));
    return releases;
  } catch (_) { return []; }
});

ipcMain.handle('download-firmware-asset', async (_, url, fileName) => {
  ensureFirmwareCache();
  const dest = path.join(FIRMWARE_CACHE_DIR, fileName);
  if (fs.existsSync(dest)) return { path: dest, cached: true };
  return new Promise((resolve) => {
    https.get(url, { headers: { 'User-Agent': 'RID-Hub' } }, (res) => {
      const file = fs.createWriteStream(dest);
      const total = parseInt(res.headers['content-length'] || '0', 10);
      let downloaded = 0;
      res.on('data', (chunk) => {
        downloaded += chunk.length;
        const percent = total ? Math.round(downloaded / total * 100) : 0;
        mainWindow?.webContents.send('firmware-download-progress', { fileName, percent, downloaded, total });
      });
      res.pipe(file);
      file.on('finish', () => { file.close(); resolve({ path: dest, cached: false }); });
      file.on('error', () => resolve({ error: 'Write failed' }));
    }).on('error', () => resolve({ error: 'Download failed' }));
  });
});

ipcMain.handle('get-cached-firmware', () => {
  ensureFirmwareCache();
  try {
    const files = fs.readdirSync(FIRMWARE_CACHE_DIR);
    return files.map(f => {
      const p = path.join(FIRMWARE_CACHE_DIR, f);
      const stat = fs.statSync(p);
      return { name: f, size: stat.size, modified: stat.mtime.toISOString() };
    });
  } catch (_) { return []; }
});

ipcMain.handle('delete-cached-firmware', (_, fileName) => {
  try {
    const p = path.join(FIRMWARE_CACHE_DIR, fileName);
    if (fs.existsSync(p)) fs.unlinkSync(p);
    return true;
  } catch (_) { return false; }
});

// === DEVICE CONFIG ===
ipcMain.handle('send-device-command', async (_, command) => {
  try {
    if (serialCapture._port && serialCapture._port.isOpen) {
      serialCapture._port.write(command + '\n');
      return true;
    }
    return false;
  } catch (_) { return false; }
});

ipcMain.handle('reboot-device', async () => {
  try {
    if (serialCapture._port && serialCapture._port.isOpen) {
      serialCapture._port.write('reboot\n');
      return true;
    }
    return false;
  } catch (_) { return false; }
});

// === APP UPDATE ===
ipcMain.handle('check-update', () => {
  if (app.isPackaged) {
    autoUpdater.checkForUpdates();
    return { status: 'checking' };
  } else {
    checkForUpdates();
    return { status: 'checking' };
  }
});
ipcMain.handle('download-update', () => {
  if (app.isPackaged) {
    autoUpdater.downloadUpdate();
    return { status: 'downloading' };
  }
  return { status: 'dev-mode', msg: 'Scarica manualmente da GitHub' };
});
ipcMain.handle('restart-app', () => {
  if (app.isPackaged) {
    autoUpdater.quitAndInstall();
  } else {
    app.relaunch();
    app.exit();
  }
});

ipcMain.handle('window-minimize', () => { mainWindow?.minimize(); });
ipcMain.handle('window-maximize', () => {
  if (mainWindow?.isMaximized()) mainWindow.unmaximize();
  else mainWindow?.maximize();
});
ipcMain.handle('window-close', () => mainWindow?.close());
ipcMain.handle('window-is-maximized', () => mainWindow?.isMaximized());

ipcMain.handle('save-file', async (_, data, defaultName, filter) => {
  const result = await dialog.showSaveDialog(mainWindow, {
    defaultPath: defaultName,
    filters: filter || [{ name: 'All Files', extensions: ['*'] }],
  });
  if (result.canceled || !result.filePath) return false;
  try {
    fs.writeFileSync(result.filePath, data, 'utf8');
    return true;
  } catch (e) {
    return { error: e.message };
  }
});

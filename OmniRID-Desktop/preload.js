'use strict';
const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('RID', {
  // === Ground app core ===
  getSnapshot: () => ipcRenderer.invoke('get-snapshot'),
  getDeviceDetail: (mac) => ipcRenderer.invoke('get-device-detail', mac),
  resetStats: () => ipcRenderer.invoke('reset-stats'),
  startRecording: () => ipcRenderer.invoke('start-recording'),
  stopRecording: () => ipcRenderer.invoke('stop-recording'),
  getSession: () => ipcRenderer.invoke('get-session'),
  exportCSV: () => ipcRenderer.invoke('export-csv'),
  exportKML: () => ipcRenderer.invoke('export-kml'),
  saveFile: (data, name, filter) => ipcRenderer.invoke('save-file', data, name, filter),
  listPorts: () => ipcRenderer.invoke('list-ports'),
  connectSerial: (path, baud) => ipcRenderer.invoke('connect-serial', path, baud),
  disconnectSerial: () => ipcRenderer.invoke('disconnect-serial'),
  startWiFi: (iface) => ipcRenderer.invoke('start-wifi-capture', iface),
  stopWiFi: () => ipcRenderer.invoke('stop-wifi-capture'),
  startBLE: () => ipcRenderer.invoke('start-ble-capture'),
  stopBLE: () => ipcRenderer.invoke('stop-ble-capture'),
  importPcap: () => ipcRenderer.invoke('import-pcap'),
  onPacket: (cb) => { ipcRenderer.on('rid-packet', (_, data) => cb(data)); },
  onPcapDone: (cb) => { ipcRenderer.on('rid-pcap-done', () => cb()); },

  // === Window controls ===
  windowMinimize: () => ipcRenderer.invoke('window-minimize'),
  windowMaximize: () => ipcRenderer.invoke('window-maximize'),
  windowClose: () => ipcRenderer.invoke('window-close'),
  windowIsMaximized: () => ipcRenderer.invoke('window-is-maximized'),
  onWindowState: (cb) => { ipcRenderer.on('window-state', (_, max) => cb(max)); },

  // === App update ===
  checkUpdate: () => ipcRenderer.invoke('check-update'),
  downloadUpdate: () => ipcRenderer.invoke('download-update'),
  restartApp: () => ipcRenderer.invoke('restart-app'),
  onUpdateChecked: (cb) => { ipcRenderer.on('update-checked', (_, d) => cb(d)); },
  onUpdateAvailable: (cb) => { ipcRenderer.on('update-available', (_, d) => cb(d)); },
  onUpdateProgress: (cb) => { ipcRenderer.on('update-progress', (_, d) => cb(d)); },
  onUpdateStatus: (cb) => { ipcRenderer.on('update-status', (_, s) => cb(s)); },
  onUpdateError: (cb) => { ipcRenderer.on('update-error', (_, m) => cb(m)); },
  getVersion: () => ipcRenderer.invoke('get-version'),

  // === Page cache (auto-sync docs/) ===
  getPageCache: (name) => ipcRenderer.invoke('get-page-cache', name),
  refreshPageCache: (name) => ipcRenderer.invoke('refresh-page-cache', name),
  refreshAllPages: () => ipcRenderer.invoke('refresh-all-pages'),
  onPageCacheUpdate: (cb) => { ipcRenderer.on('page-cache-update', (_, d) => cb(d)); },

  // === Firmware ===
  checkFirmwareReleases: () => ipcRenderer.invoke('check-firmware-releases'),
  downloadFirmwareAsset: (url, name) => ipcRenderer.invoke('download-firmware-asset', url, name),
  getCachedFirmware: () => ipcRenderer.invoke('get-cached-firmware'),
  deleteCachedFirmware: (name) => ipcRenderer.invoke('delete-cached-firmware', name),
  onFirmwareDownloadProgress: (cb) => { ipcRenderer.on('firmware-download-progress', (_, d) => cb(d)); },

  // === Device config (serial bridge) ===
  sendDeviceCommand: (cmd) => ipcRenderer.invoke('send-device-command', cmd),
  rebootDevice: () => ipcRenderer.invoke('reboot-device'),
});

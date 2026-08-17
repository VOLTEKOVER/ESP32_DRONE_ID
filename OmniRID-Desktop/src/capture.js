'use strict';

const { extractOdidFromBeacon, decodeOdidMessage, formatSummary } = require('./decoder');

let pcap = null;
try { pcap = require('pcap'); } catch (_) {}
let noble = null;
try { noble = require('@abandonware/noble'); } catch (_) {}
let SerialPort = null;
try { SerialPort = require('serialport'); } catch (_) {}

class WiFiCapture {
  constructor() {
    this._running = false;
    this._session = null;
    this._pcap = pcap;
    this._onPacket = null;
    this._channels = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13];
    this._channelIndex = 0;
    this._hopInterval = 2000;
    this._hopTimer = null;
  }

  get available() { return !!this._pcap; }

  set onPacket(cb) { this._onPacket = cb; }

  set channels(list) { if (list && list.length) this._channels = list; }

  start(iface = null) {
    if (!this._pcap || this._running) return;
    this._running = true;
    const ifaceName = iface || (this._pcap && this._pcap.findalldevs ? this._pcap.findalldevs()[0] : null);
    if (!ifaceName) { this._running = false; return; }
    try {
      this._session = this._pcap.createSession(ifaceName, { filter: 'type mgt subtype beacon', monitor: true });
      this._session.on('packet', (raw) => this._processPacket(raw));
      this._startHopping();
    } catch (e) {
      this._running = false;
    }
  }

  stop() {
    this._running = false;
    if (this._hopTimer) clearInterval(this._hopTimer);
    if (this._session) { try { this._session.close(); } catch (_) {} }
  }

  _startHopping() {
    if (this._channels.length <= 1) return;
    this._hopTimer = setInterval(() => {
      if (!this._running) return;
      this._channelIndex = (this._channelIndex + 1) % this._channels.length;
      const ch = this._channels[this._channelIndex];
      try {
        if (this._pcap && this._session) {
          if (typeof this._pcap.setFilter === 'function') {
            this._pcap.setFilter(this._session, `type mgt subtype beacon and channel ${ch}`);
          }
        }
      } catch (_) {}
    }, this._hopInterval);
  }

  _processPacket(raw) {
    if (!this._onPacket) return;
    try {
      const buf = Buffer.from(raw);
      const frameBody = buf;
      const msgs = extractOdidFromBeacon(frameBody);
      if (!msgs || !msgs.length) return;
      const ts = Date.now() / 1000;
      this._onPacket({
        timestamp: ts,
        source_mac: buf.length > 10 ? buf.slice(10, 16).toString('hex').replace(/(..)/g, '$1:').slice(0, -1) : '?',
        rssi: null,
        channel: this._channels[this._channelIndex],
        summary: formatSummary(msgs),
        messages: msgs,
      });
    } catch (_) {}
  }
}

class BLECapture {
  constructor() {
    this._running = false;
    this._onPacket = null;
    this._noble = noble;
  }

  get available() { return !!this._noble; }

  set onPacket(cb) { this._onPacket = cb; }

  start() {
    if (!this._noble || this._running) return;
    this._running = true;
    try {
      this._noble.on('stateChange', (state) => {
        if (state === 'poweredOn') this._noble.startScanningAsync(['0000fffb-0000-1000-8000-00805f9b34fb'], false);
      });
      this._noble.on('discover', (peripheral) => {
        if (!this._onPacket) return;
        const adv = peripheral.advertisement;
        const mfg = adv.manufacturerData;
        if (mfg && mfg.length > 0) {
          const msgs = [decodeOdidMessage(mfg)];
          this._onPacket({
            timestamp: Date.now() / 1000,
            source_mac: peripheral.id,
            rssi: peripheral.rssi,
            channel: 0,
            summary: formatSummary(msgs),
            messages: msgs,
          });
        }
      });
    } catch (_) { this._running = false; }
  }

  stop() {
    if (!this._running) return;
    this._running = false;
    try { this._noble.stopScanningAsync(); } catch (_) {}
  }
}

class SerialCapture {
  constructor() {
    this._port = null;
    this._onPacket = null;
    this._buffer = '';
  }

  get available() { return !!SerialPort; }

  set onPacket(cb) { this._onPacket = cb; }

  async start(path, baud = 115200) {
    if (!SerialPort) return;
    try {
      this._port = new SerialPort(path, { baudRate: baud });
      this._port.on('data', (data) => this._processSerial(data));
    } catch (_) {}
  }

  stop() {
    if (this._port) { try { this._port.close(); } catch (_) {} }
  }

  _processSerial(data) {
    if (!this._onPacket) return;
    this._buffer += data.toString('utf8');
    const lines = this._buffer.split('\n');
    this._buffer = lines.pop();
    for (const line of lines) {
      try {
        const obj = JSON.parse(line.trim());
        if (obj.messages || obj.source_mac) {
          this._onPacket({
            timestamp: obj.timestamp || Date.now() / 1000,
            source_mac: obj.source_mac || 'serial',
            rssi: obj.rssi || null,
            channel: 0,
            summary: obj.summary || '',
            messages: obj.messages || [],
          });
        }
      } catch (_) {}
    }
  }
}

module.exports = { WiFiCapture, BLECapture, SerialCapture };

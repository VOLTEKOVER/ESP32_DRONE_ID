'use strict';

class RIDDevice {
  constructor(mac, firstSeen) {
    this.mac = mac;
    this.firstSeen = firstSeen;
    this.lastSeen = firstSeen;
    this.rssiSamples = [];
    this.basicId = '';
    this.operatorId = '';
    this.uaType = 0;
    this.lastLocation = null;
    this.lastSystem = null;
    this.packetCount = 0;
    this.messagesSeen = new Set();
    this.locationTrail = [];
    this.selfId = '';
  }

  get avgRssi() {
    if (!this.rssiSamples.length) return null;
    return this.rssiSamples.reduce((a, b) => a + b, 0) / this.rssiSamples.length;
  }

  toSnapshot() {
    return {
      mac: this.mac,
      basic_id: this.basicId,
      operator_id: this.operatorId,
      ua_type: this.uaType,
      packet_count: this.packetCount,
      rssi_avg: this.avgRssi != null ? Math.round(this.avgRssi * 10) / 10 : null,
      rssi_last: this.rssiSamples.length ? this.rssiSamples[this.rssiSamples.length - 1] : null,
      rssi_min: this.rssiSamples.length ? Math.min(...this.rssiSamples) : null,
      rssi_max: this.rssiSamples.length ? Math.max(...this.rssiSamples) : null,
      first_seen: this.firstSeen,
      last_seen: this.lastSeen,
      last_location: this.lastLocation,
      last_system: this.lastSystem,
      location_trail: this.locationTrail.slice(-10),
      self_id: this.selfId,
      messages_seen: [...this.messagesSeen].sort(),
    };
  }

  toDetail() {
    return {
      mac: this.mac,
      basic_id: this.basicId,
      operator_id: this.operatorId,
      ua_type: this.uaType,
      self_id: this.selfId,
      packet_count: this.packetCount,
      rssi_avg: this.avgRssi != null ? Math.round(this.avgRssi * 10) / 10 : null,
      rssi_last: this.rssiSamples.length ? this.rssiSamples[this.rssiSamples.length - 1] : null,
      rssi_min: this.rssiSamples.length ? Math.min(...this.rssiSamples) : null,
      rssi_max: this.rssiSamples.length ? Math.max(...this.rssiSamples) : null,
      rssi_samples: this.rssiSamples.slice(-100),
      first_seen: this.firstSeen,
      last_seen: this.lastSeen,
      last_location: this.lastLocation,
      last_system: this.lastSystem,
      location_trail: this.locationTrail,
      messages_seen: [...this.messagesSeen].sort(),
    };
  }
}

class Tracker {
  constructor() {
    this._devices = new Map();
    this._packetHistory = [];
    this._historyMax = 2000;
    this._recording = false;
    this._sessionPackets = [];
    this._sessionStart = 0;
  }

  onCapture(data) {
    const mac = data.source_mac || '?';
    const ts = data.timestamp || Date.now() / 1000;
    let dev = this._devices.get(mac);
    if (!dev) {
      dev = new RIDDevice(mac, ts);
      this._devices.set(mac, dev);
    }
    dev.lastSeen = ts;
    dev.packetCount++;
    const rssi = data.rssi;
    if (rssi != null) {
      dev.rssiSamples.push(rssi);
      if (dev.rssiSamples.length > 500) dev.rssiSamples = dev.rssiSamples.slice(-500);
    }
    for (const msg of (data.messages || [])) {
      const dec = msg.decoded || {};
      const t = dec.type || '?';
      dev.messagesSeen.add(t);
      if (t === 'Basic ID') {
        if (dec.uas_id) dev.basicId = dec.uas_id;
        if (dec.ua_type) dev.uaType = dec.ua_type;
      } else if (t === 'Operator ID') {
        if (dec.operator_id) dev.operatorId = dec.operator_id;
      } else if (t === 'Location') {
        dev.lastLocation = dec;
        if (dec.latitude != null && dec.longitude != null) {
          const entry = { lat: dec.latitude, lon: dec.longitude, ts: Math.floor(ts) };
          const trail = dev.locationTrail;
          if (!trail.length || trail[trail.length - 1].lat !== entry.lat || trail[trail.length - 1].lon !== entry.lon) {
            trail.push(entry);
            if (trail.length > 500) trail.splice(0, trail.length - 500);
          }
        }
      } else if (t === 'System') {
        dev.lastSystem = dec;
      } else if (t === 'Self ID') {
        if (dec.description) dev.selfId = dec.description;
      }
    }
    const clean = { ts, mac, rssi, channel: data.channel || 0, summary: data.summary || '' };
    this._packetHistory.push(clean);
    if (this._packetHistory.length > this._historyMax) this._packetHistory.splice(0, this._packetHistory.length - this._historyMax);
    if (this._recording) this._sessionPackets.push(clean);
    return clean;
  }

  getSnapshot() {
    const now = Date.now() / 1000;
    return {
      devices: [...this._devices.values()].sort((a, b) => b.lastSeen - a.lastSeen).map(d => d.toSnapshot()),
      stats: this.getStats(now),
    };
  }

  getStats(now) {
    now = now || Date.now() / 1000;
    const total = this._devices.size;
    let active = 0, basicIds = new Set();
    for (const d of this._devices.values()) {
      if (now - d.lastSeen < 30) active++;
      if (d.basicId) basicIds.add(d.basicId);
    }
    const recent = this._packetHistory.filter(p => now - p.ts < 60).length;
    return {
      total_devices: total, active_devices: active, unique_ids: basicIds.size,
      packets_last_60s: recent, total_packets: this._packetHistory.length,
      recording: this._recording, session_packets: this._sessionPackets.length,
    };
  }

  reset() {
    this._devices.clear();
    this._packetHistory = [];
    this._sessionPackets = [];
    this._recording = false;
    this._sessionStart = 0;
  }

  startRecording() {
    this._sessionPackets = [];
    this._recording = true;
    this._sessionStart = Date.now() / 1000;
  }

  stopRecording() {
    this._recording = false;
  }

  getDeviceDetail(mac) {
    const dev = this._devices.get(mac);
    return dev ? dev.toDetail() : null;
  }

  generateCSV() {
    const lines = ['MAC,Basic ID,UA Type,Operator ID,Latitude,Longitude,RSSI Avg,RSSI Last,First Seen,Last Seen,Packet Count,Self ID'];
    for (const d of this._devices.values()) {
      const lat = d.lastLocation ? d.lastLocation.latitude : '';
      const lon = d.lastLocation ? d.lastLocation.longitude : '';
      const first = new Date(d.firstSeen * 1000).toISOString().replace('T', ' ').slice(0, 19);
      const last = new Date(d.lastSeen * 1000).toISOString().replace('T', ' ').slice(0, 19);
      lines.push([
        d.mac, d.basicId, d.uaType, d.operatorId, lat, lon,
        d.avgRssi != null ? Math.round(d.avgRssi * 10) / 10 : '',
        d.rssiSamples.length ? d.rssiSamples[d.rssiSamples.length - 1] : '',
        first, last, d.packetCount, d.selfId,
      ].join(','));
    }
    return lines.join('\n');
  }

  generateKML() {
    let kml = `<?xml version="1.0" encoding="UTF-8"?>
<kml xmlns="http://www.opengis.net/kml/2.2">
<Document><name>OmniRID Desktop — Drone Report</name>`;
    for (const d of this._devices.values()) {
      const name = d.basicId || d.mac;
      kml += `<Placemark><name>${name}</name>`;
      kml += `<description>MAC: ${d.mac}\\nOperator: ${d.operatorId}\\nType: ${d.uaType}\\nPackets: ${d.packetCount}</description>`;
      if (d.locationTrail.length >= 2) {
        kml += '<LineString><extrude>1</extrude><tessellate>1</tessellate><coordinates>';
        for (const pt of d.locationTrail) kml += `${pt.lon},${pt.lat},${pt.ts} `;
        kml += '</coordinates></LineString>';
      }
      if (d.lastLocation) {
        kml += `<Point><coordinates>${d.lastLocation.longitude},${d.lastLocation.latitude},0</coordinates></Point>`;
      }
      kml += '</Placemark>';
    }
    kml += '</Document></kml>';
    return kml;
  }

  get sessionPackets() { return this._sessionPackets; }
  get isRecording() { return this._recording; }
  get totalDevices() { return this._devices.size; }
}

module.exports = { Tracker, RIDDevice };

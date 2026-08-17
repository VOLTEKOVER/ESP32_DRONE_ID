function app() {
  return {
    // === STATE ===
    theme: 'light',
    activeTab: 'ground',
    groundTab: 'dashboard',
    isSidebarCollapsed: false,
    devices: [],
    packets: [],
    totalPackets: 0,
    sessionPackets: 0,
    isRecording: false,
    wifiActive: false,
    bleActive: false,
    serialActive: false,
    selectedDeviceMac: null,
    toasts: [],
    isMaximized: false,
    deviceSearch: '',
    sortKey: 'lastSeen',
    sortAsc: false,
    macFilter: '',
    msgFilter: '',
    packetIdCounter: 0,
    packetTimestamps: [],
    startTime: Date.now(),
    appVersion: '',
    updateStatus: 'idle',
    updateLatest: '',
    updateProgress: 0,
    updateDownloading: false,
    updateError: '',
    showSettingsModal: false,
    _timer: null,

    // === PAGE CACHE (auto-sync docs/) ===
    pageCacheStatus: { landing: '', guide: '', configure: '' },
    pageCacheContent: { landing: '', guide: '', configure: '' },

    // === FIRMWARE ===
    firmwareReleases: [],
    firmwareDownloading: null,
    firmwareProgress: 0,
    firmwareCached: [],
    firmwareStatus: '',

    navItems: [
      { id: 'ground',    label: 'Ground',      icon: 'ti-layout-dashboard' },
      { id: 'landing',   label: 'Landing',      icon: 'ti-world' },
      { id: 'guide',     label: 'Guide',        icon: 'ti-book' },
      { id: 'configure', label: 'Configure',    icon: 'ti-settings-cog' },
      { id: 'firmware',  label: 'Firmware',     icon: 'ti-microchip' },
    ],
    groundSubs: [
      { id: 'dashboard', label: 'Dashboard', icon: 'ti-layout-dashboard' },
      { id: 'devices',   label: 'Devices',   icon: 'ti-radio' },
      { id: 'map',       label: 'Map',       icon: 'ti-map' },
      { id: 'timeline',  label: 'Timeline',  icon: 'ti-timeline' },
      { id: 'capture',   label: 'Capture',   icon: 'ti-player-record' },
    ],

    // === COMPUTED ===
    get activeDrones() {
      return this.devices.filter(d => Date.now() - new Date(d.lastSeen).getTime() < 10000).length
    },
    get avgRssi() {
      return this.devices.length
        ? Math.round(this.devices.reduce((s, d) => s + d.rssiLast, 0) / this.devices.length)
        : 0
    },
    get bestRssi() {
      return this.devices.length ? Math.max(...this.devices.map(d => d.rssiLast)) : 0
    },
    get validDevices() {
      return this.devices.filter(d => d.lat !== 0 || d.lon !== 0)
    },
    get selectedDevice() {
      return this.devices.find(d => d.mac === this.selectedDeviceMac) || null
    },
    get filteredDevices() {
      return this.devices
        .filter(d => d.mac.toLowerCase().includes(this.deviceSearch.toLowerCase())
          || d.basicId.toLowerCase().includes(this.deviceSearch.toLowerCase()))
        .sort((a, b) => {
          let cmp = 0
          switch (this.sortKey) {
            case 'mac': cmp = a.mac.localeCompare(b.mac); break
            case 'rssiLast': cmp = a.rssiLast - b.rssiLast; break
            case 'packetsCount': cmp = a.packetsCount - b.packetsCount; break
            case 'lastSeen': cmp = new Date(a.lastSeen).getTime() - new Date(b.lastSeen).getTime(); break
            case 'firstSeen': cmp = new Date(a.firstSeen).getTime() - new Date(b.firstSeen).getTime(); break
            case 'uaType': cmp = a.uaType.localeCompare(b.uaType); break
          }
          return this.sortAsc ? cmp : -cmp
      })
    },
    get filteredPackets() {
      return this.packets.filter(p => {
        if (this.macFilter && !p.mac.toLowerCase().includes(this.macFilter.toLowerCase())) return false
        if (this.msgFilter && !p.msgType.toLowerCase().includes(this.msgFilter.toLowerCase())) return false
        return true
      }).slice().reverse()
    },
    get macColorList() {
      const macs = [...new Set(this.packets.slice(-80).map(p => p.mac))]
      return macs.map(mac => ({ mac, color: this._macColor(mac) }))
    },
    get packetRate() {
      if (this.packetTimestamps.length < 2) return 0
      const now = Date.now()
      const recent = this.packetTimestamps.filter(t => now - t < 3000)
      return recent.length / 3
    },
    get uptime() {
      const s = Math.floor((Date.now() - this.startTime) / 1000)
      const m = Math.floor(s / 60)
      const h = Math.floor(m / 60)
      if (h > 0) return `${h}h ${m % 60}m`
      if (m > 0) return `${m}m ${s % 60}s`
      return `${s}s`
    },
    get hasCaptureActive() {
      return this.wifiActive || this.bleActive || this.serialActive
    },
    get captureSourcesList() {
      const s = []
      if (this.wifiActive) s.push('WiFi')
      if (this.bleActive) s.push('BLE')
      if (this.serialActive) s.push('Serial')
      return s.join(' · ') || '—'
    },
    get activityBars() {
      const now = Date.now()
      const bars = []
      for (let i = 29; i >= 0; i--) {
        const start = now - (i + 1) * 1000
        const end = now - i * 1000
        const count = this.packetTimestamps.filter(t => t >= start && t < end).length
        bars.push(count)
      }
      const max = Math.max(...bars, 1)
      return { bars, max, w: 600, h: 80 }
    },
    get activityChartSvg() {
      const data = this.activityBars
      let bg = '', bars = ''
      const accent = 'var(--accent)'
      const border = 'var(--border)'
      for (let i = 0; i < data.bars.length; i++) {
        const x = i * 20
        const v = data.bars[i]
        const h = (v / data.max) * 80
        const isLast = i === data.bars.length - 1
        const fillBg = isLast ? accent : border
        const fillBar = isLast ? accent : `color-mix(in srgb,${accent} 40%,transparent)`
        bg += `<rect x="${x}" y="0" width="18" height="80" fill="${fillBg}" opacity="0.5"/>`
        bars += `<rect x="${x}" y="${80 - h}" width="18" height="${h}" fill="${fillBar}" rx="2"/>`
      }
      return `<g>${bg}${bars}</g>`
    },
    get mapGridSvg() {
      let s = ''
      const stroke = this.theme === 'dark' ? 'rgba(255,255,255,.04)' : 'rgba(0,0,0,.06)'
      for (let i = 0; i < 13; i++) {
        const lon = -180 + i * 30
        const lat = -90 + i * 15
        s += `<line x1="${lon}" y1="-90" x2="${lon}" y2="90" stroke="${stroke}" stroke-width="0.3"/>`
        s += `<line x1="-180" y1="${lat}" x2="180" y2="${lat}" stroke="${stroke}" stroke-width="0.3"/>`
      }
      return s
    },
    get mapMarkersSvg() {
      let s = ''
      for (const d of this.validDevices) {
        const color = d.rssiLast > -70 ? 'var(--accent-green)' : 'var(--accent-orange)'
        const label = d.basicId ? d.basicId.slice(0, 6) : d.mac.slice(0, 8)
        if (d.trail.length > 1) {
          const pts = d.trail.slice(-5).map(p => `${p.lon},${-p.lat}`).join(' ')
          s += `<polyline points="${pts}" fill="none" stroke="${color}" stroke-width="0.2" opacity="0.5"/>`
        }
        s += `<circle cx="${d.lon}" cy="${-d.lat}" r="1.2" fill="${color}" stroke="#fff" stroke-width="0.3" opacity="0.9" style="cursor:pointer" onclick="Alpine.\$data(document.querySelector('[x-data]')).selectDevice('${d.mac}')"/>`
        s += `<circle cx="${d.lon}" cy="${-d.lat}" r="3" fill="none" stroke="${color}" stroke-width="0.15" opacity="0.2"/>`
        s += `<text x="${d.lon}" y="${-d.lat - 2}" text-anchor="middle" fill="var(--text)" font-size="0.7" font-weight="600" opacity="0.8">${label}</text>`
      }
      return s
    },
    get timelineChartSvg() {
      const pts = this.packets.slice(-80)
      if (pts.length < 2) return ''
      const groups = {}
      for (const p of pts) {
        if (!groups[p.mac]) groups[p.mac] = []
        groups[p.mac].push(p)
      }
      const allRssis = pts.map(p => p.rssi)
      const min = Math.min(...allRssis)
      const max = Math.max(...allRssis)
      const range = max - min || 1
      const w = 600, h = 120, pad = 5
      let lines = ''
      for (const [mac, macPts] of Object.entries(groups)) {
        const color = this._macColor(mac)
        if (macPts.length < 2) continue
        const ptsStr = macPts.map((p, i) => {
          const x = pad + (i / (macPts.length - 1)) * (w - 2 * pad)
          const y = pad + ((max - p.rssi) / range) * (h - 2 * pad)
          return `${x.toFixed(1)},${y.toFixed(1)}`
        }).join(' ')
        lines += `<polyline points="${ptsStr}" fill="none" stroke="${color}" stroke-width="1.5" stroke-linejoin="round" stroke-linecap="round" opacity="0.8"/>`
      }
      return lines
    },
    _macColor(mac) {
      let hash = 0
      for (let i = 0; i < mac.length; i++) {
        hash = mac.charCodeAt(i) + ((hash << 5) - hash)
      }
      const hue = Math.abs(hash) % 360
      return `hsl(${hue}, 55%, ${this.theme === 'dark' ? 50 : 42}%)`
    },

    // === METHODS ===
    init() {
      document.documentElement.setAttribute('data-theme', this.theme)

      if (window.RID?.windowIsMaximized) {
        window.RID.windowIsMaximized().then(m => { this.isMaximized = m })
      }
      window.RID?.onWindowState(m => { this.isMaximized = m })

      if (window.RID?.getVersion) {
        window.RID.getVersion().then(v => { this.appVersion = v })
      } else {
        this.appVersion = '1.0.0'
      }

      // Update listeners
      window.RID?.onUpdateChecked(d => {
        if (d.hasUpdate) { this.updateStatus = 'available'; this.updateLatest = d.latest }
        else { this.updateStatus = 'up-to-date' }
      })
      window.RID?.onUpdateAvailable(d => { this.updateStatus = 'available'; this.updateLatest = d.version })
      window.RID?.onUpdateProgress(d => { this.updateDownloading = true; this.updateProgress = d.percent })
      window.RID?.onUpdateStatus(s => {
        this.updateDownloading = false
        if (s === 'downloaded') this.updateStatus = 'downloaded'
        else if (s === 'up-to-date') this.updateStatus = 'up-to-date'
        else if (s === 'checking') this.updateStatus = 'checking'
      })
      window.RID?.onUpdateError(m => { this.updateStatus = 'error'; this.updateError = m; this.updateDownloading = false })

      // Packet listener
      window.RID?.onPacket(data => {
        const devices = data.snapshot || []
        this.devices = devices.map(d => ({
          ...d,
          trail: d.trail || [],
          seenMessages: d.seenMessages || [],
        }))
        this.totalPackets = data.totalPackets || 0
        this.sessionPackets = data.sessionPackets || 0
        if (data.raw) {
          const now = Date.now()
          this.packetTimestamps.push(now)
          if (this.packetTimestamps.length > 3000) this.packetTimestamps = this.packetTimestamps.slice(-2000)
          this.packets.push({
            _idx: this.packetIdCounter++,
            mac: data.raw.mac || 'unknown',
            rssi: data.raw.rssi || 0,
            msgType: data.raw.summary || data.raw.msg_type || 'UNKNOWN',
            timestamp: now,
          })
          if (this.packets.length > 5000) this.packets = this.packets.slice(-3000)
        }
      })

      // Page cache listeners
      window.RID?.onPageCacheUpdate(results => {
        for (const [name, result] of Object.entries(results)) {
          if (this.pageCacheStatus[name] === 'syncing') {
            this.pageCacheStatus[name] = result.success ? 'ok' : 'error'
          }
        }
      })

      // Firmware progress
      window.RID?.onFirmwareDownloadProgress(d => {
        if (d.fileName === this.firmwareDownloading) {
          this.firmwareProgress = d.percent
        }
      })

      // PostMessage bridge for Configure iframe (serial commands)
      window.addEventListener('message', (e) => {
        if (e.source?.frameElement?.id !== 'iframe-configure') return
        const msg = e.data
        if (!msg || !msg.type) return
        switch (msg.type) {
          case 'serial-send':
            if (window.RID?.sendDeviceCommand) {
              window.RID.sendDeviceCommand(msg.cmd)
            }
            break
          case 'reboot-device':
            if (window.RID?.rebootDevice) {
              window.RID.rebootDevice()
            }
            break
          case 'list-ports':
            window.RID?.listPorts().then(ports => {
              if (e.source) {
                e.source.postMessage({ type: 'list-ports-response', ports }, '*')
              }
            })
            break
        }
      })

      // Load firmware cache
      this.loadFirmwareCached()
    },

    setTab(t) {
      this.activeTab = t
      // Load page cache content when switching to a doc tab
      if (['landing', 'guide', 'configure'].includes(t)) {
        this.$nextTick(() => this.loadPageContent(t))
      }
    },

    toggleTheme() {
      this.theme = this.theme === 'dark' ? 'light' : 'dark'
      document.documentElement.setAttribute('data-theme', this.theme)
      this.notify(`Tema ${this.theme === 'dark' ? 'scuro' : 'chiaro'} attivato`, 'success')
    },

    minimize() { window.RID?.windowMinimize() },
    maximize() { window.RID?.windowMaximize() },
    closeWindow() { window.RID?.windowClose() },

    toggleRecording() {
      this.isRecording = !this.isRecording
      if (this.isRecording) { window.RID?.startRecording(); this.notify('Registrazione avviata', 'success') }
      else { window.RID?.stopRecording(); this.notify('Registrazione fermata', 'info') }
    },

    toggleWifi() {
      this.wifiActive = !this.wifiActive
      if (this.wifiActive) {
        window.RID?.startWiFi().then(ok => {
          if (!ok) { this.wifiActive = false; this.notify('Errore avvio WiFi', 'error') }
          else this.notify('Cattura WiFi avviata', 'success')
        })
      } else { window.RID?.stopWiFi(); this.notify('Cattura WiFi fermata', 'info') }
    },

    toggleBle() {
      this.bleActive = !this.bleActive
      if (this.bleActive) {
        window.RID?.startBLE().then(ok => {
          if (!ok) { this.bleActive = false; this.notify('Errore avvio BLE', 'error') }
          else this.notify('Cattura BLE avviata', 'success')
        })
      } else { window.RID?.stopBLE(); this.notify('Cattura BLE fermata', 'info') }
    },

    toggleSerial() {
      this.serialActive = !this.serialActive
      if (this.serialActive) {
        window.RID?.listPorts().then(ports => {
          if (ports.length > 0) {
            window.RID?.connectSerial(ports[0].path, 115200).then(ok => {
              if (ok) this.notify('Connessione seriale avviata', 'success')
              else { this.serialActive = false; this.notify('Errore connessione seriale', 'error') }
            })
          } else { this.serialActive = false; this.notify('Nessuna porta seriale trovata', 'error') }
        })
      } else { window.RID?.disconnectSerial(); this.notify('Connessione seriale chiusa', 'info') }
    },

    importPcap() {
      window.RID?.importPcap().then(result => {
        if (typeof result === 'string') { this.notify(`Importati pacchetti PCAP`, 'success') }
        else if (result && result.error) { this.notify(`Errore PCAP: ${result.error}`, 'error') }
      })
    },

    resetSession() {
      this.packets = []; this.packetTimestamps = []; this.sessionPackets = 0; this.totalPackets = 0
      this.isRecording = false; this.startTime = Date.now()
      window.RID?.resetStats()
      this.notify('Sessione azzerata', 'success')
    },

    factoryReset() {
      this.wifiActive = false; this.bleActive = false; this.serialActive = false
      this.isRecording = false; this.devices = []; this.packets = []; this.packetTimestamps = []
      this.totalPackets = 0; this.sessionPackets = 0; this.startTime = Date.now()
      this.activeTab = 'ground'; this.groundTab = 'dashboard'
      this.theme = 'dark'
      document.documentElement.setAttribute('data-theme', 'dark')
      window.RID?.resetStats()
      this.notify('Ground Station ripristinata', 'success')
    },

    exportData(fmt) {
      if (fmt === 'CSV') window.RID?.exportCSV().then(d => d && window.RID?.saveFile(d, `rid_export.${fmt.toLowerCase()}`, [{ name: `${fmt} Files`, extensions: [fmt.toLowerCase()] }]))
      else if (fmt === 'KML') window.RID?.exportKML().then(d => d && window.RID?.saveFile(d, `rid_export.${fmt.toLowerCase()}`, [{ name: `${fmt} Files`, extensions: [fmt.toLowerCase()] }]))
      else this.notify(`Export ${fmt} non implementato`, 'info')
      this.notify(`Export ${fmt} completato`, 'success')
    },

    selectDevice(mac) { this.selectedDeviceMac = mac },
    closeDetail() { this.selectedDeviceMac = null },

    toggleSort(key) {
      if (this.sortKey === key) this.sortAsc = !this.sortAsc
      else { this.sortKey = key; this.sortAsc = false }
    },

    clearLog() { this.packets = []; this.notify('Log svuotato', 'info') },

    checkUpdate() {
      this.updateStatus = 'checking'
      if (window.RID) { window.RID.checkUpdate() }
      else { setTimeout(() => { this.updateStatus = 'error'; this.updateError = 'No back-end' }, 1000) }
    },
    downloadUpdate() { this.updateDownloading = true; this.updateProgress = 0; if (window.RID) { window.RID.downloadUpdate() } },
    restartApp() { window.RID?.restartApp() },

    // === PAGE CACHE ===
    async loadPageContent(name) {
      const iframe = document.getElementById(`iframe-${name}`)
      if (!iframe) return
      this.pageCacheStatus[name] = 'syncing'
      if (window.RID?.getPageCache) {
        const result = await window.RID.getPageCache(name)
        const html = result?.html
        if (html) {
          iframe.srcdoc = html
          this.pageCacheStatus[name] = 'ok'
        } else {
          iframe.srcdoc = '<p>Contenuto non disponibile offline.</p>'
          this.pageCacheStatus[name] = 'error'
        }
      } else {
        iframe.srcdoc = '<p>Bridge non disponibile.</p>'
        this.pageCacheStatus[name] = 'error'
      }
    },

    async refreshPage(name) {
      if (window.RID?.refreshPageCache) {
        this.pageCacheStatus[name] = 'syncing'
        const result = await window.RID.refreshPageCache(name)
        if (result.success) {
          this.loadPageContent(name)
          this.notify(`${name} aggiornato`, 'success')
        } else {
          this.pageCacheStatus[name] = 'error'
          this.notify(`Errore aggiornamento ${name}`, 'error')
        }
      }
    },

    // === FIRMWARE ===
    async loadFirmwareReleases() {
      this.firmwareStatus = 'checking'
      if (window.RID?.checkFirmwareReleases) {
        const releases = await window.RID.checkFirmwareReleases()
        this.firmwareReleases = releases || []
        this.firmwareStatus = releases.length ? 'available' : 'empty'
      } else { this.firmwareStatus = 'error' }
    },

    async loadFirmwareCached() {
      if (window.RID?.getCachedFirmware) {
        this.firmwareCached = (await window.RID.getCachedFirmware()) || []
      }
    },

    async downloadFirmwareAsset(url, name) {
      this.firmwareDownloading = name
      this.firmwareProgress = 0
      if (window.RID?.downloadFirmwareAsset) {
        const result = await window.RID.downloadFirmwareAsset(url, name)
        if (result.error) { this.notify(`Errore download ${name}: ${result.error}`, 'error') }
        else { this.notify(`Scaricato ${name}`, 'success') }
        this.firmwareDownloading = null
        this.loadFirmwareCached()
      }
    },

    deleteCachedFirmware(name) {
      if (window.RID?.deleteCachedFirmware) {
        window.RID.deleteCachedFirmware(name)
        this.loadFirmwareCached()
        this.notify(`Eliminato ${name}`, 'info')
      }
    },

    firmwareAssetIcon(name) {
      if (name.includes('bootloader')) return 'ti-device-floppy'
      if (name.includes('partition')) return 'ti-table'
      if (name.includes('esp-drone')) return 'ti-microchip'
      return 'ti-file'
    },

    fmtTime(ts) { return new Date(ts).toLocaleTimeString() },

    notify(text, type = 'info') {
      const id = Math.random().toString(36).substring(2, 9)
      this.toasts.push({ id, text, type })
      setTimeout(() => { this.toasts = this.toasts.filter(t => t.id !== id) }, 3500)
    },
  }
}

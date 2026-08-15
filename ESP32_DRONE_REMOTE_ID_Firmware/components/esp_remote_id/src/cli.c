#include <stdio.h>
#include <string.h>
#include <stdlib.h>
#include <ctype.h>
#include "esp_log.h"
#include "esp_system.h"
#include "esp_timer.h"
#include "esp_heap_caps.h"
#include "esp_wifi.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "cli.h"
#include "esp_remote_id.h"
#include "protocol_detect.h"

#define TAG "CLI"
#define CLI_STACK_SIZE 4096
#define MAX_ARGS 16
#define MAX_LINE 256

static const char *proto_name(rid_protocol_t p)
{
    switch (p) {
        case RID_PROTOCOL_UNKNOWN: return "UNKNOWN";
        case RID_PROTOCOL_MAVLINK: return "MAVLink";
        case RID_PROTOCOL_MSP:     return "MSP";
        case RID_PROTOCOL_NMEA:   return "NMEA";
        case RID_PROTOCOL_NONE:   return "NONE";
        case RID_PROTOCOL_AUTO:   return "AUTO";
        default: return "?";
    }
}

static int cmd_help(int argc, char **argv);
static int cmd_status(int argc, char **argv);
static int cmd_config(int argc, char **argv);
static int cmd_restart(int argc, char **argv);
static int cmd_reset(int argc, char **argv);
static int cmd_protocol(int argc, char **argv);
static int cmd_heap(int argc, char **argv);
static int cmd_log_level(int argc, char **argv);
static int cmd_patrol(int argc, char **argv);
static int cmd_transmit(int argc, char **argv);
static int cmd_mac(int argc, char **argv);
static int cmd_uptime(int argc, char **argv);
static int cmd_kalman(int argc, char **argv);

typedef struct { const char *name; const char *help; int (*func)(int, char**); } cmd_t;

static const cmd_t cmds[] = {
    { "help",     "Show this help", cmd_help },
    { "status",   "Show system status (protocol, GPS, TX, heap, uptime)", cmd_status },
    { "config",   "Show/set configuration: config [set <field> <value>]", cmd_config },
    { "restart",  "Restart the device", cmd_restart },
    { "reboot",   "Restart the device (alias)", cmd_restart },
    { "reset",    "Factory reset and restart", cmd_reset },
    { "factory",  "Factory reset and restart (alias)", cmd_reset },
    { "protocol", "Show/set: protocol [auto|mavlink|msp|nmea|none]", cmd_protocol },
    { "heap",     "Show heap memory info", cmd_heap },
    { "log_level","Set: log_level <tag> <NONE|ERROR|WARN|INFO|DEBUG|VERBOSE>", cmd_log_level },
    { "patrol",   "Toggle demo patrol mode: patrol [on|off]", cmd_patrol },
    { "transmit", "Show/set TX: transmit <wifi_bcn|wifi_nan|ble4|ble5|all> <on|off>", cmd_transmit },
    { "mac",      "Show MAC addresses", cmd_mac },
    { "uptime",   "Show system uptime", cmd_uptime },
    { "kalman",   "Show/set Kalman filter: kalman [on|off]", cmd_kalman },
};
#define NCMDS (sizeof(cmds) / sizeof(cmds[0]))

static int cmd_help(int argc, char **argv)
{
    printf("\n  Commands:\n");
    for (size_t i = 0; i < NCMDS; i++) {
        printf("  %-12s %s\n", cmds[i].name, cmds[i].help);
    }
    printf("\n");
    return 0;
}

static int cmd_status(int argc, char **argv)
{
    rid_config_t cfg;
    rid_state_t state;
    esp_rid_get_config(&cfg);
    esp_rid_get_state(&state);
    printf("\n  ESP DRONE REMOTEID v%s\n\n", ESP_RID_VERSION);
    printf("  Protocol  : %s\n", proto_name(state.active_protocol));
    printf("  GPS Fix   : %s  (fix=%d, sats=%u)\n",
           state.gps_valid ? "YES" : "NO", state.gps.fix_type, state.gps.satellites);
    printf("  Position  : %.6f / %.6f  alt=%.1f m\n",
           state.gps.latitude, state.gps.longitude, (double)state.gps.altitude_msl);
    printf("  Speed     : %.1f m/s  heading=%d\n", (double)state.gps.speed, state.gps.heading);
    printf("  TX total  : %lu  (BCN=%lu  NAN=%lu  BLE4=%lu  BLE5=%lu)\n",
           (unsigned long)state.transmissions_count,
           (unsigned long)state.wifi_bcn_count, (unsigned long)state.wifi_nan_count,
           (unsigned long)state.ble4_count, (unsigned long)state.ble5_count);
    uint32_t heap_free = esp_get_free_heap_size();
    uint32_t heap_total = heap_caps_get_total_size(MALLOC_CAP_DEFAULT);
    int64_t us = esp_timer_get_time();
    uint32_t sec = (uint32_t)(us / 1000000);
    printf("  Heap      : %lu KB / %lu KB\n", (unsigned long)(heap_free / 1024), (unsigned long)(heap_total / 1024));
    printf("  Uptime    : %02lu:%02lu:%02lu\n", (unsigned long)(sec / 3600), (unsigned long)((sec % 3600) / 60), (unsigned long)(sec % 60));
    printf("  Kalman    : %s\n", (cfg.options & RID_OPT_KALMAN_FILTER) ? "ON" : "OFF");
    printf("  Lock lvl  : %d\n\n", cfg.lock_level);
    return 0;
}

static int cmd_config(int argc, char **argv)
{
    rid_config_t cfg;
    esp_rid_get_config(&cfg);

    if (argc >= 2 && strcasecmp(argv[1], "set") == 0) {
        if (argc < 4) {
            printf("Usage: config set <field> <value>\n");
            return 1;
        }
        const char *f = argv[2];
        const char *v = argv[3];

        if (strcasecmp(f, "uas_id") == 0 || strcasecmp(f, "operator_id") == 0 ||
            strcasecmp(f, "self_id") == 0 || strcasecmp(f, "wifi_ssid") == 0 ||
            strcasecmp(f, "wifi_password") == 0) {
            char *dst;
            size_t dst_len;
            if (strcasecmp(f, "uas_id") == 0) { dst = cfg.uas_id; dst_len = sizeof(cfg.uas_id); }
            else if (strcasecmp(f, "operator_id") == 0) { dst = cfg.operator_id; dst_len = sizeof(cfg.operator_id); }
            else if (strcasecmp(f, "self_id") == 0) { dst = cfg.self_id_text; dst_len = sizeof(cfg.self_id_text); }
            else if (strcasecmp(f, "wifi_ssid") == 0) { dst = cfg.wifi_ssid; dst_len = sizeof(cfg.wifi_ssid); }
            else { dst = cfg.wifi_password; dst_len = sizeof(cfg.wifi_password); }
            strncpy(dst, v, dst_len - 1);
            dst[dst_len - 1] = '\0';
        } else if (strcasecmp(f, "ua_type") == 0) cfg.ua_type = (uint8_t)strtol(v, NULL, 0);
        else if (strcasecmp(f, "id_type") == 0) cfg.id_type = (uint8_t)strtol(v, NULL, 0);
        else if (strcasecmp(f, "wifi_channel") == 0) cfg.wifi_channel = (uint8_t)strtol(v, NULL, 0);
        else if (strcasecmp(f, "mavlink_sysid") == 0) cfg.mavlink_sysid = (uint8_t)strtol(v, NULL, 0);
        else if (strcasecmp(f, "bcast_powerup") == 0) cfg.bcast_powerup = (uint8_t)strtol(v, NULL, 0);
        else if (strcasecmp(f, "webserver") == 0) cfg.webserver_en = (uint8_t)(strcasecmp(v, "on") == 0 || strtol(v, NULL, 0) != 0);
        else if (strcasecmp(f, "lock_level") == 0) cfg.lock_level = (int8_t)strtol(v, NULL, 0);
        else if (strcasecmp(f, "baud_rate") == 0) cfg.baud_rate = (uint32_t)strtoul(v, NULL, 0);
        else if (strcasecmp(f, "wifi_power_dbm") == 0) cfg.wifi_power_dbm = (float)strtod(v, NULL);
        else if (strcasecmp(f, "wifi_bcn_rate") == 0) cfg.wifi_bcn_rate_hz = (float)strtod(v, NULL);
        else if (strcasecmp(f, "wifi_nan_rate") == 0) cfg.wifi_nan_rate_hz = (float)strtod(v, NULL);
        else if (strcasecmp(f, "ble4_rate") == 0) cfg.ble4_rate_hz = (float)strtod(v, NULL);
        else if (strcasecmp(f, "ble4_power") == 0) cfg.ble4_power_dbm = (float)strtod(v, NULL);
        else if (strcasecmp(f, "ble5_rate") == 0) cfg.ble5_rate_hz = (float)strtod(v, NULL);
        else if (strcasecmp(f, "ble5_power") == 0) cfg.ble5_power_dbm = (float)strtod(v, NULL);
        else if (strcasecmp(f, "operator_lat") == 0) cfg.operator_lat = strtod(v, NULL);
        else if (strcasecmp(f, "operator_lon") == 0) cfg.operator_lon = strtod(v, NULL);
        else if (strcasecmp(f, "operator_alt") == 0) cfg.operator_alt = (float)strtod(v, NULL);
        else if (strcasecmp(f, "start_delay_ms") == 0) cfg.start_delay_ms = (uint32_t)strtoul(v, NULL, 0);
        else {
            printf("Unknown field: %s\n", f);
            return 1;
        }

        esp_rid_set_config(&cfg);
        printf("  %s set to '%s'\n", f, v);
        return 0;
    }

    printf("\n  Configuration:\n\n");
    printf("  Protocol    : %s\n", proto_name(cfg.protocol));
    printf("  UART        : port=%d baud=%lu tx=%d rx=%d\n", cfg.uart_port, (unsigned long)cfg.baud_rate, cfg.tx_pin, cfg.rx_pin);
    printf("  UAS ID      : %s\n", cfg.uas_id);
    printf("  Operator ID : %s\n", cfg.operator_id);
    printf("  UA Type     : %d  ID Type: %d\n", cfg.ua_type, cfg.id_type);
    printf("  TX Modes    : ");
    if (cfg.tx_modes & RID_TRANSMIT_WIFI_BCN) printf("WiFi_BCN ");
    if (cfg.tx_modes & RID_TRANSMIT_WIFI_NAN) printf("WiFi_NAN ");
    if (cfg.tx_modes & RID_TRANSMIT_BLE4)     printf("BLE4 ");
    if (cfg.tx_modes & RID_TRANSMIT_BLE5)     printf("BLE5 ");
    printf("\n");
    printf("  WiFi        : CH=%d  power=%.1f dBm  SSID=%s\n", cfg.wifi_channel, (double)cfg.wifi_power_dbm, cfg.wifi_ssid);
    printf("  WiFi Beacon : %.1f Hz  NAN=%.1f Hz\n", (double)cfg.wifi_bcn_rate_hz, (double)cfg.wifi_nan_rate_hz);
    printf("  BLE4        : %.1f Hz  power=%.1f dBm\n", (double)cfg.ble4_rate_hz, (double)cfg.ble4_power_dbm);
    printf("  BLE5        : %.1f Hz  power=%.1f dBm\n", (double)cfg.ble5_rate_hz, (double)cfg.ble5_power_dbm);
    printf("  Operator    : %.6f / %.6f  alt=%.1f\n", cfg.operator_lat, cfg.operator_lon, (double)cfg.operator_alt);
    printf("  Options     : 0x%02x  (Kalman=%s)\n", cfg.options,
           (cfg.options & RID_OPT_KALMAN_FILTER) ? "ON" : "OFF");
    printf("  Web server  : %s\n", cfg.webserver_en ? "enabled" : "disabled");
    printf("  Lock level  : %d\n", cfg.lock_level);
    printf("  Fields      : uas_id operator_id self_id wifi_ssid wifi_password ua_type id_type\n");
    printf("                wifi_channel mavlink_sysid bcast_powerup webserver lock_level\n");
    printf("                baud_rate wifi_power_dbm wifi_bcn_rate wifi_nan_rate ble4_rate\n");
    printf("                ble4_power ble5_rate ble5_power operator_lat operator_lon\n");
    printf("                operator_alt start_delay_ms\n\n");
    return 0;
}

static int cmd_restart(int argc, char **argv)
{
    printf("Restarting...\n");
    esp_restart();
    return 0;
}

static int cmd_reset(int argc, char **argv)
{
    printf("Factory reset...\n");
    esp_rid_factory_reset();
    esp_restart();
    return 0;
}

static int cmd_protocol(int argc, char **argv)
{
    rid_config_t cfg;
    esp_rid_get_config(&cfg);
    if (argc == 1) {
        printf("Protocol: %s\n", proto_name(cfg.protocol));
        return 0;
    }
    rid_protocol_t p = RID_PROTOCOL_AUTO;
    if (strcasecmp(argv[1], "auto") == 0) p = RID_PROTOCOL_AUTO;
    else if (strcasecmp(argv[1], "mavlink") == 0) p = RID_PROTOCOL_MAVLINK;
    else if (strcasecmp(argv[1], "msp") == 0) p = RID_PROTOCOL_MSP;
    else if (strcasecmp(argv[1], "nmea") == 0) p = RID_PROTOCOL_NMEA;
    else if (strcasecmp(argv[1], "none") == 0) p = RID_PROTOCOL_NONE;
    else { printf("Unknown: %s  (use: auto, mavlink, msp, nmea, none)\n", argv[1]); return 1; }
    cfg.protocol = p;
    esp_rid_set_config(&cfg);
    printf("Protocol set to: %s\n", proto_name(p));
    return 0;
}

static int cmd_heap(int argc, char **argv)
{
    uint32_t heap_free = esp_get_free_heap_size();
    uint32_t heap_total = heap_caps_get_total_size(MALLOC_CAP_DEFAULT);
    uint32_t min = esp_get_minimum_free_heap_size();
    printf("  Heap free : %lu KB / %lu KB\n", (unsigned long)(heap_free / 1024), (unsigned long)(heap_total / 1024));
    printf("  Heap min  : %lu KB\n", (unsigned long)(min / 1024));
    return 0;
}

static int cmd_log_level(int argc, char **argv)
{
    if (argc < 3) {
        printf("Usage: log_level <tag> <NONE|ERROR|WARN|INFO|DEBUG|VERBOSE>\n");
        return 1;
    }
    esp_log_level_t level;
    if (strcasecmp(argv[2], "NONE") == 0) level = ESP_LOG_NONE;
    else if (strcasecmp(argv[2], "ERROR") == 0) level = ESP_LOG_ERROR;
    else if (strcasecmp(argv[2], "WARN") == 0) level = ESP_LOG_WARN;
    else if (strcasecmp(argv[2], "INFO") == 0) level = ESP_LOG_INFO;
    else if (strcasecmp(argv[2], "DEBUG") == 0) level = ESP_LOG_DEBUG;
    else if (strcasecmp(argv[2], "VERBOSE") == 0) level = ESP_LOG_VERBOSE;
    else { printf("Unknown level: %s\n", argv[2]); return 1; }
    esp_log_level_set(argv[1], level);
    printf("Log level for '%s' set to %s\n", argv[1], argv[2]);
    return 0;
}

static int cmd_patrol(int argc, char **argv)
{
    rid_config_t cfg;
    esp_rid_get_config(&cfg);
    if (argc >= 2 && strcasecmp(argv[1], "off") == 0)
        cfg.options &= ~RID_OPT_DEMO_MODE;
    else if (argc >= 2 && strcasecmp(argv[1], "on") == 0)
        cfg.options |= RID_OPT_DEMO_MODE;
    else
        cfg.options ^= RID_OPT_DEMO_MODE;
    esp_rid_set_config(&cfg);
    printf("Demo patrol: %s\n", (cfg.options & RID_OPT_DEMO_MODE) ? "ON" : "OFF");
    return 0;
}

static int cmd_transmit(int argc, char **argv)
{
    rid_config_t cfg;
    esp_rid_get_config(&cfg);
    if (argc < 2) {
        printf("  TX: %s%s%s%s\n  Usage: transmit <wifi_bcn|wifi_nan|ble4|ble5|all> <on|off>\n",
            (cfg.tx_modes & RID_TRANSMIT_WIFI_BCN) ? "WiFi_BCN " : "",
            (cfg.tx_modes & RID_TRANSMIT_WIFI_NAN) ? "WiFi_NAN " : "",
            (cfg.tx_modes & RID_TRANSMIT_BLE4) ? "BLE4 " : "",
            (cfg.tx_modes & RID_TRANSMIT_BLE5) ? "BLE5 " : "");
        return 0;
    }
    if (argc < 3) { printf("Missing on/off\n"); return 1; }
    uint8_t mask = 0;
    if (strcasecmp(argv[1], "wifi_bcn") == 0) mask = RID_TRANSMIT_WIFI_BCN;
    else if (strcasecmp(argv[1], "wifi_nan") == 0) mask = RID_TRANSMIT_WIFI_NAN;
    else if (strcasecmp(argv[1], "ble4") == 0) mask = RID_TRANSMIT_BLE4;
    else if (strcasecmp(argv[1], "ble5") == 0) mask = RID_TRANSMIT_BLE5;
    else if (strcasecmp(argv[1], "all") == 0) mask = 0x0F;
    else { printf("Unknown mode: %s\n", argv[1]); return 1; }
    bool on = (strcasecmp(argv[2], "on") == 0);
    if (mask == 0x0F) { cfg.tx_modes = on ? 0x0F : 0; }
    else if (on) { cfg.tx_modes |= mask; }
    else { cfg.tx_modes &= ~mask; }
    esp_rid_set_config(&cfg);
    printf("TX '%s' set to %s\n", argv[1], on ? "ON" : "OFF");
    return 0;
}

static int cmd_mac(int argc, char **argv)
{
    uint8_t ap[6]={0}, sta[6]={0};
    esp_wifi_get_mac(WIFI_IF_AP, ap);
    esp_wifi_get_mac(WIFI_IF_STA, sta);
    printf("  MAC AP  : %02X:%02X:%02X:%02X:%02X:%02X\n", ap[0],ap[1],ap[2],ap[3],ap[4],ap[5]);
    printf("  MAC STA : %02X:%02X:%02X:%02X:%02X:%02X\n", sta[0],sta[1],sta[2],sta[3],sta[4],sta[5]);
    return 0;
}

static int cmd_uptime(int argc, char **argv)
{
    int64_t us = esp_timer_get_time();
    uint32_t sec = (uint32_t)(us / 1000000);
    printf("  Uptime: %02lu:%02lu:%02lu\n", (unsigned long)(sec / 3600), (unsigned long)((sec % 3600) / 60), (unsigned long)(sec % 60));
    return 0;
}

static int cmd_kalman(int argc, char **argv)
{
    rid_config_t cfg;
    esp_rid_get_config(&cfg);
    if (argc >= 2) {
        if (strcasecmp(argv[1], "on") == 0)
            cfg.options |= RID_OPT_KALMAN_FILTER;
        else if (strcasecmp(argv[1], "off") == 0)
            cfg.options &= ~RID_OPT_KALMAN_FILTER;
        else {
            printf("Usage: kalman [on|off]\n");
            return 1;
        }
        esp_rid_set_config(&cfg);
    }
    printf("Kalman filter: %s\n", (cfg.options & RID_OPT_KALMAN_FILTER) ? "ON" : "OFF");
    return 0;
}

/* ---------- parser ---------- */
static int parse_line(char *line, char **argv)
{
    int argc = 0;
    char *p = line;
    while (*p) {
        while (*p && isspace((unsigned char)*p)) p++;
        if (!*p) break;
        if (argc < MAX_ARGS - 1) argv[argc++] = p;
        while (*p && !isspace((unsigned char)*p)) p++;
        if (*p) { *p++ = '\0'; }
    }
    argv[argc] = NULL;
    return argc;
}

/* ---------- task ---------- */
static void cli_task(void *arg)
{
    char line[MAX_LINE];
    char *argv[MAX_ARGS];

    printf("\n  ESP DRONE REMOTEID CLI\n");
    printf("  Type 'help' for commands\n\n");

    while (1) {
        printf("rid> ");
        fflush(stdout);

        if (fgets(line, sizeof(line), stdin) == NULL) {
            vTaskDelay(pdMS_TO_TICKS(10));
            continue;
        }

        size_t len = strlen(line);
        while (len > 0 && (line[len-1] == '\n' || line[len-1] == '\r'))
            line[--len] = '\0';
        if (len == 0) continue;

        int argc = parse_line(line, argv);
        if (argc == 0) continue;

        bool found = false;
        for (size_t i = 0; i < NCMDS; i++) {
            if (strcmp(argv[0], cmds[i].name) == 0) {
                cmds[i].func(argc, argv);
                found = true;
                break;
            }
        }
        if (!found)
            printf("Unknown: %s  (type 'help')\n", argv[0]);
    }
}

void cli_init(void)
{
    BaseType_t ret = xTaskCreate(cli_task, "cli_task", CLI_STACK_SIZE, NULL, 5, NULL);
    if (ret != pdPASS) {
        ESP_LOGE(TAG, "Failed to create cli_task (err=%d)", (int)ret);
        return;
    }
    ESP_LOGI(TAG, "CLI initialized");
}

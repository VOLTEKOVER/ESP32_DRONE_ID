#include <stdio.h>
#include <string.h>
#include "esp_log.h"
#include "esp_remote_id.h"
#include "esp_mac.h"
#include "esp_wifi.h"
#include "esp_timer.h"
#include "esp_efuse.h"
#include "sdkconfig.h"
#include "psa/crypto.h"

#define TAG "MAIN"

/* Fallback MAC when eFuse MAC is corrupted (common on ESP32-S0WD) */
#define FALLBACK_MAC {0x24, 0x0A, 0xC4, 0x12, 0x34, 0x56}

static void fix_mac_if_needed(void)
{
    uint8_t mac[6] = {0};
    esp_err_t err = esp_efuse_mac_get_default(mac);
    if (err != ESP_OK || (mac[0] == 0 && mac[1] == 0 && mac[2] == 0)) {
        ESP_LOGW(TAG, "eFuse MAC CRC error — using fallback MAC address");
        uint8_t fallback[] = FALLBACK_MAC;
        esp_base_mac_addr_set(fallback);
    }
}

#define C_BLU  "\x1b[34m"
#define C_GRN  "\x1b[32m"
#define C_AMB  "\x1b[33m"
#define C_RED  "\x1b[31m"
#define C_RST  "\x1b[0m"

static void print_splash(void)
{
    uint8_t mac[6] = {0};
    esp_wifi_get_mac(WIFI_IF_AP, mac);

    printf("\n");
    printf(C_BLU);
    printf("  \xE2\x95\x94\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x97\n");
    printf(C_RST);
    printf("  \xE2\x95\x91                                                                  \xE2\x95\x91\n");
    printf("  \xE2\x95\x91                  ..                                               \xE2\x95\x91\n");
    printf("  \xE2\x95\x91                -:...:-:                                           \xE2\x95\x91\n");
    printf("  \xE2\x95\x91                +     .+*-                                         \xE2\x95\x91\n");
    printf("  \xE2\x95\x91                +:      :*=.                                       \xE2\x95\x91\n");
    printf("  \xE2\x95\x91              .--+.       -=:                                      \xE2\x95\x91\n");
    printf("  \xE2\x95\x91              --  =:       :=-                                     \xE2\x95\x91\n");
    printf("  \xE2\x95\x91       ......  -.  +:.:::.  ---.                                   \xE2\x95\x91\n");
    printf("  \xE2\x95\x91     .=.  .:-++-=-  =:   ..::=:-:.                                 \xE2\x95\x91\n");
    printf("  \xE2\x95\x91     .=       .:-++::+-      .-=--::                               \xE2\x95\x91\n");
    printf("  \xE2\x95\x91      .-:         .-+===.      .=:::-:                             \xE2\x95\x91\n");
    printf("  \xE2\x95\x91        .-:          :--+=:      -::.:-                            \xE2\x95\x91\n");
    printf("  \xE2\x95\x91          .-=-:::::::::---=++=:.  .:--:=.          .               \xE2\x95\x91\n");
    printf("  \xE2\x95\x91            .:-.        ..:---===-:..:=++:     =*##+=.             \xE2\x95\x91\n");
    printf("  \xE2\x95\x91               :-:          .::::-=++-:=%%###*+@@@@+**             \xE2\x95\x91\n");
    printf("  \xE2\x95\x91                 .::.            ..:=*#%%@@@@@@@@@%%*#*+:             \xE2\x95\x91\n");
    printf("  \xE2\x95\x91                    .:::..      ..:-=%%%%@@@@@@@@@%%*-               \xE2\x95\x91\n");
    printf("  \xE2\x95\x91                        .........-#%%@@%%%%****++#-* ==              \xE2\x95\x91\n");
    printf("  \xE2\x95\x91                               :*@@%%*+:    -=. *  :-::::::.        \xE2\x95\x91\n");
    printf("  \xE2\x95\x91                            .-++=:.      .+-   =-..                \xE2\x95\x91\n");
    printf("  \xE2\x95\x91        .:::::           .-+=:            =-     ..::::            \xE2\x95\x91\n");
    printf("  \xE2\x95\x91    .=+==-----.       :=*+-                .--                     \xE2\x95\x91\n");
    printf("  \xE2\x95\x91  .++::-=====-    .-+##=.                    .=                    \xE2\x95\x91\n");
    printf("  \xE2\x95\x91 :%%- +*:-:-===. .=*%%%%*:                                              \xE2\x95\x91\n");
    printf("  \xE2\x95\x91 #+ *# -#-. .-*%%%%*-                                                 \xE2\x95\x91\n");
    printf("  \xE2\x95\x91 -  =. =:  :*#+-=  -  -.                                            \xE2\x95\x91\n");
    printf("  \xE2\x95\x91           .   -%% :@..@:                                           \xE2\x95\x91\n");
    printf("  \xE2\x95\x91           :-=++.:#= +#                                             \xE2\x95\x91\n");
    printf("  \xE2\x95\x91           :----++:.**                                             \xE2\x95\x91\n");
    printf("  \xE2\x95\x91           -------++:                                               \xE2\x95\x91\n");
    printf("  \xE2\x95\x91           =+++==:.                                                 \xE2\x95\x91\n");
    printf("  \xE2\x95\x91                                                                  \xE2\x95\x91\n");
    printf("  \xE2\x95\x91  ___ ___ ___    ___  ___   ___  _  _ ___                          \xE2\x95\x91\n");
    printf("  \xE2\x95\x91 | __/ __| _ \\  |   \\| _ \\ / _ \\| \\| | __|                          \xE2\x95\x91\n");
    printf("  \xE2\x95\x91 | _|\\__ \\  _/  | |) |   /| (_) | .` | _|                           \xE2\x95\x91\n");
    printf("  \xE2\x95\x91 |___|___/_|    |___/|_|_\\ \\___/|_|\\_|___|                          \xE2\x95\x91\n");
    printf("  \xE2\x95\x91                                                                  \xE2\x95\x91\n");
    printf("  \xE2\x95\x91  ___ ___ __  __ ___ _____ ___   ___ ___                            \xE2\x95\x91\n");
    printf("  \xE2\x95\x91 | _ \\ __|  \\/  |  \\_   _| __| | |_ _|   \\                          \xE2\x95\x91\n");
    printf("  \xE2\x95\x91 |   / _|| |\\/| | () || | | _|    | || |) |                         \xE2\x95\x91\n");
    printf("  \xE2\x95\x91 |_|_\\___|_|  |_|\\___/|_| |___|  |___|___/                          \xE2\x95\x91\n");
    printf("  \xE2\x95\x91                                                                  \xE2\x95\x91\n");
    printf("  \xE2\x95\x91  ESP DRONE REMOTEID — Open DroneID Transmitter                    \xE2\x95\x91\n");
    printf("  \xE2\x95\x91                                                                  \xE2\x95\x91\n");
    printf(C_GRN);
    printf("  \xE2\x95\x91  v%-12s                   ASTM F3411-22a                   \xE2\x95\x91\n", ESP_RID_VERSION);
    printf(C_BLU);
    printf("  \xE2\x95\xA0\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\xA3\n");
    printf(C_RST);
    printf("  \xE2\x95\x91  WiFi AP    │ ESP-RID                                            \xE2\x95\x91\n");
    printf("  \xE2\x95\x91  Config URL │ http://192.168.4.1                                 \xE2\x95\x91\n");
    printf("  \xE2\x95\x91  MAC AP     │ %02x:%02x:%02x:%02x:%02x:%02x                                        \xE2\x95\x91\n", mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]);
    printf(C_BLU);
    printf("  \xE2\x95\x9A\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x90\xE2\x95\x9D\n");
    printf(C_RST);
    printf("\n");
}

void app_main(void)
{
    psa_crypto_init();
    fix_mac_if_needed();
    esp_rid_init();
    esp_rid_start();

    print_splash();
}

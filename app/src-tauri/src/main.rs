// SPDX-License-Identifier: GPL-3.0-or-later
// Évite l'ouverture d'une console sous Windows en version release. NE PAS RETIRER.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![forbid(unsafe_code)]

fn main() {
    mega_app_lib::run()
}

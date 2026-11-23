//! Thin native binary entry point for the Dennett desktop shell.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    dennett_desktop_lib::run();
}

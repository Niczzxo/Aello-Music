//! Binary entry point for Aello for Windows.
//!
//! All application logic lives in the library crate (`aello_lib`).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    aello_lib::run();
}

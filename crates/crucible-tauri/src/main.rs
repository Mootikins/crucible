// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod events;

use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::greet,
            commands::initialize_database,
            commands::search_documents,
            commands::get_document,
            commands::create_document,
            commands::update_document,
            commands::delete_document,
            commands::list_documents,
            commands::search_by_tags,
            commands::search_by_properties,
            commands::semantic_search,
            commands::index_vault,
            commands::get_note_metadata,
            commands::update_note_properties,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}


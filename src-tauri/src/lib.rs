//! Gallery desktop Menzies: Tauri entry, paths, and gallery commands.

mod gallery;
mod paths;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            if let Err(err) = paths::ensure_app_dirs() {
                log::warn!("could not create Menzies app dirs: {err}");
            }

            #[cfg(desktop)]
            {
                app.handle()
                    .plugin(tauri_plugin_updater::Builder::new().build())?;
            }

            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            paths::app_paths,
            gallery::get_default_pictures_dir,
            gallery::list_gallery_entries,
            gallery::load_gallery_settings,
            gallery::save_gallery_settings,
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|err| {
            eprintln!("error while running tauri application: {err}");
            std::process::exit(1);
        });
}

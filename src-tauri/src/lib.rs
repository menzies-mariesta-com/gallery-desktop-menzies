//! Gallery desktop Menzies: Tauri entry, paths, and gallery commands.

mod ext_registry;
mod ext_sign;
mod face_db;
mod face_ext;
mod face_index;
mod face_infer;
mod face_math;
mod gallery;
mod hashutil;
mod image_edit;
mod organize;
mod paths;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_http::init())
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
        .on_window_event(|_window, event| {
            if matches!(
                event,
                tauri::WindowEvent::CloseRequested { .. } | tauri::WindowEvent::Destroyed
            ) {
                face_index::request_cancel();
            }
        })
        .invoke_handler(tauri::generate_handler![
            paths::app_paths,
            gallery::get_default_pictures_dir,
            gallery::list_gallery_entries,
            gallery::load_gallery_settings,
            gallery::save_gallery_settings,
            image_edit::suggest_edited_name,
            image_edit::image_file_info,
            image_edit::preview_image_edit,
            image_edit::commit_image_edit,
            image_edit::discard_edit_preview,
            organize::duplicate_gallery_image,
            organize::delete_gallery_image,
            organize::rename_gallery_image,
            organize::copy_image_to_clipboard,
            organize::load_image_order,
            organize::save_image_order,
            face_ext::face_extension_status,
            face_ext::set_face_recognition_experimental,
            face_ext::install_face_extension,
            face_ext::uninstall_face_extension,
            ext_registry::extension_catalog,
            ext_registry::extension_check,
            ext_registry::extension_install,
            ext_registry::extension_update,
            ext_registry::extension_set_update_mode,
            ext_registry::extension_uninstall,
            ext_registry::extension_cancel_transfer,
            face_index::face_index_start,
            face_index::face_index_cancel,
            face_index::face_people,
            face_index::face_person_images,
            face_index::face_person_rename,
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|err| {
            eprintln!("error while running tauri application: {err}");
            std::process::exit(1);
        });
}

use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    App,
};

use crate::{app::trigger_selected_translation, window};

pub fn setup(app: &mut App) -> tauri::Result<()> {
    let translate = MenuItem::with_id(app, "translate", "翻译选中文字", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "设置", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&translate, &settings, &quit])?;
    let icon = app.default_window_icon().cloned();

    let mut builder = TrayIconBuilder::new()
        .tooltip("QuickTranslate")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "translate" => {
                trigger_selected_translation(app.clone());
            }
            "settings" => window::show_settings(app),
            "quit" => app.exit(0),
            _ => {}
        });
    if let Some(icon) = icon {
        builder = builder.icon(icon);
    }
    builder.build(app)?;
    Ok(())
}

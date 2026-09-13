use crate::capabilities::pet_wardrobe::{self, Outcome};
use crate::domain::pet::Action;
use crate::state::SharedState;
use tauri::{Emitter, Manager};

#[tauri::command(async)]
pub(crate) fn pet_get_wardrobe(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedState>,
) -> Result<Outcome, String> {
    pet_wardrobe::transact(&app, state.inner(), None, false)
}
#[tauri::command(async)]
pub(crate) fn pet_wardrobe_action(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedState>,
    action: Action,
) -> Result<Outcome, String> {
    pet_wardrobe::transact(&app, state.inner(), Some(action), false)
}
#[tauri::command(async)]
pub(crate) fn pet_settle_activity(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedState>,
) -> Result<Outcome, String> {
    pet_wardrobe::transact(&app, state.inner(), None, true)
}

#[tauri::command]
pub(crate) fn pet_set_menu_open(open: bool) {
    pet_wardrobe::set_menu_open(open);
}

#[tauri::command(async)]
pub(crate) fn pet_open_wardrobe(app: tauri::AppHandle) -> Result<(), String> {
    let window = app.get_webview_window("main").ok_or("主窗口尚未就绪")?;
    crate::window_placement::ensure_main_window_visible(&app);
    window.unminimize().map_err(|e| e.to_string())?;
    window.show().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())?;
    app.emit_to("main", "pet-open-wardrobe", ())
        .map_err(|e| e.to_string())
}

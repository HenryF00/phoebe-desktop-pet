use serde::{Deserialize, Serialize};
use std::fs;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};
use std::time::{Duration, Instant};
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    App, AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, PhysicalRect, PhysicalSize,
    WindowEvent,
};

const PET_BASE_WIDTH: f64 = 240.0;
// Eight logical pixels give vertical motion room while keeping the 240x320 art at full width.
const PET_BASE_HEIGHT: f64 = 328.0;
const PET_ICON_SIZE: f64 = 84.0;

#[derive(Default)]
pub struct PositionDebouncer {
    enabled: AtomicBool,
    running: AtomicBool,
    last_move: Mutex<Option<Instant>>,
}

#[derive(Debug, Deserialize, Serialize)]
struct SavedPetPosition {
    version: u8,
    monitor_name: Option<String>,
    monitor_width: u32,
    monitor_height: u32,
    scale_factor: f64,
    pet_height_logical: f64,
    normalized_x: f64,
    normalized_y: f64,
}

fn window(app: &AppHandle, label: &str) -> Result<tauri::WebviewWindow, String> {
    app.get_webview_window(label)
        .ok_or_else(|| format!("{label} 窗口不可用"))
}

fn monitor_for_pet(pet: &tauri::WebviewWindow) -> Result<tauri::window::Monitor, String> {
    let monitor = match pet.current_monitor().map_err(|_| "无法读取当前显示器")? {
        Some(monitor) => monitor,
        None => pet
            .primary_monitor()
            .map_err(|_| "无法读取主显示器")?
            .ok_or("没有可用显示器")?,
    };
    Ok(monitor)
}

fn work_area(pet: &tauri::WebviewWindow) -> Result<PhysicalRect<i32, u32>, String> {
    Ok(monitor_for_pet(pet)?.work_area().clone())
}

fn bounded_axis(desired: i64, start: i32, extent: u32, window_extent: u32) -> i32 {
    let min = i64::from(start);
    let max = min + i64::from(extent) - i64::from(window_extent);
    desired.clamp(min, max.max(min)) as i32
}

fn clamp_position(
    desired: PhysicalPosition<i32>,
    size: PhysicalSize<u32>,
    area: &PhysicalRect<i32, u32>,
) -> PhysicalPosition<i32> {
    PhysicalPosition::new(
        bounded_axis(
            i64::from(desired.x),
            area.position.x,
            area.size.width,
            size.width,
        ),
        bounded_axis(
            i64::from(desired.y),
            area.position.y,
            area.size.height,
            size.height,
        ),
    )
}

fn bottom_center_anchored_position(
    old_position: PhysicalPosition<i32>,
    old_size: PhysicalSize<u32>,
    new_size: PhysicalSize<u32>,
) -> PhysicalPosition<i32> {
    PhysicalPosition::new(
        (i64::from(old_position.x) + (i64::from(old_size.width) - i64::from(new_size.width)) / 2)
            .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
        (i64::from(old_position.y) + i64::from(old_size.height) - i64::from(new_size.height))
            .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
    )
}

fn normalized_axis(position: i32, start: i32, extent: u32, window_extent: u32) -> f64 {
    let travel = f64::from(extent.saturating_sub(window_extent));
    if travel == 0.0 {
        0.0
    } else {
        ((f64::from(position) - f64::from(start)) / travel).clamp(0.0, 1.0)
    }
}

fn restored_axis(value: f64, start: i32, extent: u32, window_extent: u32) -> i32 {
    let travel = f64::from(extent.saturating_sub(window_extent));
    bounded_axis(
        (f64::from(start) + value * travel).round() as i64,
        start,
        extent,
        window_extent,
    )
}

fn position_file(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|_| "无法取得应用数据目录")?
        .join("window-position.json"))
}

fn restore_position(app: &AppHandle) -> Result<(), String> {
    let bytes = fs::read(position_file(app)?).map_err(|_| "尚无桌宠位置记录")?;
    if bytes.len() > 4096 {
        return Err("桌宠位置记录过大".into());
    }
    let record: SavedPetPosition =
        serde_json::from_slice(&bytes).map_err(|_| "桌宠位置记录不可读取")?;
    if record.version != 1
        || !(0.0..=1.0).contains(&record.normalized_x)
        || !(0.0..=1.0).contains(&record.normalized_y)
        || !(0.5..=8.0).contains(&record.scale_factor)
        || !record.pet_height_logical.is_finite()
        || record.pet_height_logical <= 0.0
    {
        return Err("桌宠位置记录无效".into());
    }
    let pet = window(app, "pet")?;
    let monitors = pet.available_monitors().map_err(|_| "无法列出显示器")?;
    let target = monitors.into_iter().find(|monitor| {
        monitor.name() == record.monitor_name.as_ref()
            && monitor.size().width == record.monitor_width
            && monitor.size().height == record.monitor_height
    });
    let monitor = match target {
        Some(monitor) => monitor,
        None => pet
            .primary_monitor()
            .map_err(|_| "无法读取主显示器")?
            .ok_or("没有可用显示器")?,
    };
    let area = monitor.work_area();
    let size = pet.outer_size().map_err(|_| "无法读取桌宠尺寸")?;
    pet.set_position(PhysicalPosition::new(
        restored_axis(
            record.normalized_x,
            area.position.x,
            area.size.width,
            size.width,
        ),
        restored_axis(
            record.normalized_y,
            area.position.y,
            area.size.height,
            size.height,
        ),
    ))
    .map_err(|_| "无法恢复桌宠位置".to_owned())
}

fn save_position(app: &AppHandle) -> Result<(), String> {
    let pet = window(app, "pet")?;
    let monitor = monitor_for_pet(&pet)?;
    let area = monitor.work_area();
    let size = pet.outer_size().map_err(|_| "无法读取桌宠尺寸")?;
    let position = pet.outer_position().map_err(|_| "无法读取桌宠位置")?;
    let scale = pet.scale_factor().map_err(|_| "无法读取窗口缩放")?;
    let record = SavedPetPosition {
        version: 1,
        monitor_name: monitor.name().cloned(),
        monitor_width: monitor.size().width,
        monitor_height: monitor.size().height,
        scale_factor: scale,
        pet_height_logical: f64::from(size.height) / scale,
        normalized_x: normalized_axis(position.x, area.position.x, area.size.width, size.width),
        normalized_y: normalized_axis(position.y, area.position.y, area.size.height, size.height),
    };
    let path = position_file(app)?;
    fs::create_dir_all(path.parent().ok_or("应用数据目录不可用")?)
        .map_err(|_| "无法建立应用数据目录")?;
    fs::write(
        path,
        serde_json::to_vec(&record).map_err(|_| "无法编码桌宠位置")?,
    )
    .map_err(|_| "无法保存桌宠位置".to_owned())
}

pub fn persist_pet_position(app: &AppHandle) -> Result<(), String> {
    save_position(app)
}

fn schedule_save(app: &AppHandle) {
    let state = app.state::<PositionDebouncer>();
    if !state.enabled.load(Ordering::SeqCst) {
        return;
    }
    if let Ok(mut last_move) = state.last_move.lock() {
        *last_move = Some(Instant::now());
    }
    if state.running.swap(true, Ordering::SeqCst) {
        return;
    }
    let handle = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(600));
        let state = handle.state::<PositionDebouncer>();
        let recent = state
            .last_move
            .lock()
            .ok()
            .and_then(|time| *time)
            .is_some_and(|time| time.elapsed() < Duration::from_millis(600));
        if recent {
            continue;
        }
        let _ = save_position(&handle);
        state.running.store(false, Ordering::SeqCst);
        let changed = state
            .last_move
            .lock()
            .ok()
            .and_then(|time| *time)
            .is_some_and(|time| time.elapsed() < Duration::from_millis(600));
        if changed {
            schedule_save(&handle);
        }
        break;
    });
}

pub fn enable_position_saves(app: &AppHandle) {
    app.state::<PositionDebouncer>()
        .enabled
        .store(true, Ordering::SeqCst);
    schedule_save(app);
}

fn chat_position(
    pet_position: PhysicalPosition<i32>,
    pet_size: PhysicalSize<u32>,
    chat_size: PhysicalSize<u32>,
    area: &PhysicalRect<i32, u32>,
    gap: i32,
) -> PhysicalPosition<i32> {
    let left = i64::from(pet_position.x) - i64::from(chat_size.width) - i64::from(gap);
    let right = i64::from(pet_position.x) + i64::from(pet_size.width) + i64::from(gap);
    let work_left = i64::from(area.position.x);
    let work_right = work_left + i64::from(area.size.width);
    let x = if left >= work_left {
        left
    } else if right + i64::from(chat_size.width) <= work_right {
        right
    } else {
        // On a narrow screen the windows may overlap, but the chat stays on-screen.
        right
    };
    clamp_position(
        PhysicalPosition::new(
            x.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
            (i64::from(pet_position.y) + i64::from(pet_size.height) - i64::from(chat_size.height))
                .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
        ),
        chat_size,
        area,
    )
}

fn menu_position(
    pet_position: PhysicalPosition<i32>,
    pet_size: PhysicalSize<u32>,
    menu_size: PhysicalSize<u32>,
    area: &PhysicalRect<i32, u32>,
    gap: i32,
) -> PhysicalPosition<i32> {
    let left = i64::from(pet_position.x) - i64::from(menu_size.width) - i64::from(gap);
    let right = i64::from(pet_position.x) + i64::from(pet_size.width) + i64::from(gap);
    let work_left = i64::from(area.position.x);
    let work_right = work_left + i64::from(area.size.width);
    let aligned_y =
        i64::from(pet_position.y) + i64::from(pet_size.height) - i64::from(menu_size.height);
    if left >= work_left || right + i64::from(menu_size.width) <= work_right {
        let x = if left >= work_left { left } else { right };
        return clamp_position(
            PhysicalPosition::new(x as i32, aligned_y as i32),
            menu_size,
            area,
        );
    }

    // On a narrow work area, place the menu above or below instead of covering the pet.
    let above = i64::from(pet_position.y) - i64::from(menu_size.height) - i64::from(gap);
    let below = i64::from(pet_position.y) + i64::from(pet_size.height) + i64::from(gap);
    let work_top = i64::from(area.position.y);
    let work_bottom = work_top + i64::from(area.size.height);
    if above >= work_top || below + i64::from(menu_size.height) <= work_bottom {
        let y = if above >= work_top { above } else { below };
        return clamp_position(
            PhysicalPosition::new(pet_position.x, y as i32),
            menu_size,
            area,
        );
    }
    clamp_position(
        PhysicalPosition::new(right as i32, aligned_y as i32),
        menu_size,
        area,
    )
}

pub fn clamp_pet(app: &AppHandle) -> Result<(), String> {
    let pet = window(app, "pet")?;
    let area = work_area(&pet)?;
    let position = pet.outer_position().map_err(|_| "无法读取桌宠位置")?;
    let size = pet.outer_size().map_err(|_| "无法读取桌宠尺寸")?;
    let bounded = clamp_position(position, size, &area);
    if bounded != position {
        pet.set_position(bounded).map_err(|_| "无法限制桌宠位置")?;
    }
    Ok(())
}

pub fn apply_pet_scale(app: &AppHandle, percent: u16) -> Result<(), String> {
    if !crate::settings::valid_pet_scale_percent(percent) {
        return Err("仅支持图标化以及 75%、100%、125% 和 150% 的桌宠大小".into());
    }
    let pet = window(app, "pet")?;
    let area = work_area(&pet)?;
    let old_position = pet.outer_position().map_err(|_| "无法读取桌宠位置")?;
    let old_size = pet.outer_size().map_err(|_| "无法读取桌宠尺寸")?;
    let target_size = if percent == 0 {
        LogicalSize::new(PET_ICON_SIZE, PET_ICON_SIZE)
    } else {
        let ratio = f64::from(percent) / 100.0;
        LogicalSize::new(PET_BASE_WIDTH * ratio, PET_BASE_HEIGHT * ratio)
    };
    pet.set_size(target_size).map_err(|_| "无法缩放桌宠窗口")?;
    let new_size = pet.outer_size().map_err(|_| "无法读取缩放后的桌宠尺寸")?;

    // Keep the character's feet and horizontal center fixed while its hit area scales.
    let desired = bottom_center_anchored_position(old_position, old_size, new_size);
    pet.set_position(clamp_position(desired, new_size, &area))
        .map_err(|_| "无法定位缩放后的桌宠")?;
    if is_chat_visible(app).unwrap_or(false) {
        let _ = position_chat(app);
    }
    if window(app, "menu")?.is_visible().unwrap_or(false) {
        let _ = position_pet_menu(app);
    }
    Ok(())
}

pub fn position_chat(app: &AppHandle) -> Result<(), String> {
    let pet = window(app, "pet")?;
    let chat = window(app, "chat")?;
    let area = work_area(&pet)?;
    let pet_position = pet.outer_position().map_err(|_| "无法读取桌宠位置")?;
    let pet_size = pet.outer_size().map_err(|_| "无法读取桌宠尺寸")?;
    let chat_size = chat.outer_size().map_err(|_| "无法读取聊天窗尺寸")?;
    let gap = (12.0 * pet.scale_factor().unwrap_or(1.0)).round() as i32;
    chat.set_position(chat_position(pet_position, pet_size, chat_size, &area, gap))
        .map_err(|_| "无法移动聊天窗".to_owned())
}

fn position_pet_menu(app: &AppHandle) -> Result<(), String> {
    let pet = window(app, "pet")?;
    let menu = window(app, "menu")?;
    let area = work_area(&pet)?;
    let pet_position = pet.outer_position().map_err(|_| "无法读取桌宠位置")?;
    let pet_size = pet.outer_size().map_err(|_| "无法读取桌宠尺寸")?;
    let menu_size = menu.outer_size().map_err(|_| "无法读取菜单尺寸")?;
    let gap = (10.0 * pet.scale_factor().unwrap_or(1.0)).round() as i32;
    menu.set_position(menu_position(pet_position, pet_size, menu_size, &area, gap))
        .map_err(|_| "无法移动快捷菜单".to_owned())
}

pub fn show_pet_menu(app: &AppHandle) -> Result<(), String> {
    position_pet_menu(app)?;
    let menu = window(app, "menu")?;
    menu.show().map_err(|_| "无法显示快捷菜单")?;
    menu.set_focus().map_err(|_| "无法聚焦快捷菜单")?;
    let _ = app.emit("pet-menu-visibility", true);
    Ok(())
}

pub fn hide_pet_menu(app: &AppHandle, return_focus: bool) -> Result<(), String> {
    window(app, "menu")?
        .hide()
        .map_err(|_| "无法隐藏快捷菜单")?;
    let _ = app.emit("pet-menu-visibility", false);
    if return_focus {
        let _ = window(app, "pet")?.set_focus();
    }
    Ok(())
}

pub fn is_chat_visible(app: &AppHandle) -> Result<bool, String> {
    window(app, "chat")?
        .is_visible()
        .map_err(|_| "无法读取聊天窗状态".to_owned())
}

pub fn show_chat(app: &AppHandle) -> Result<(), String> {
    position_chat(app)?;
    let chat = window(app, "chat")?;
    chat.show().map_err(|_| "无法显示聊天窗")?;
    chat.set_focus().map_err(|_| "无法聚焦聊天窗")?;
    let _ = app.emit("chat-visibility", true);
    Ok(())
}

pub fn hide_chat(app: &AppHandle) -> Result<(), String> {
    window(app, "chat")?
        .hide()
        .map_err(|_| "无法隐藏聊天窗".to_owned())?;
    let _ = app.emit("chat-visibility", false);
    Ok(())
}

pub fn show_settings(app: &AppHandle) -> Result<(), String> {
    let settings = window(app, "settings")?;
    settings.show().map_err(|_| "无法显示设置窗")?;
    settings.set_focus().map_err(|_| "无法聚焦设置窗")?;
    let _ = app.emit("settings-visibility", true);
    Ok(())
}

pub fn hide_settings(app: &AppHandle) -> Result<(), String> {
    window(app, "settings")?
        .hide()
        .map_err(|_| "无法隐藏设置窗")?;
    let _ = app.emit("settings-visibility", false);
    Ok(())
}

pub fn toggle_chat(app: &AppHandle) -> Result<bool, String> {
    if is_chat_visible(app)? {
        hide_chat(app)?;
        Ok(false)
    } else {
        show_chat(app)?;
        Ok(true)
    }
}

pub fn show_pet(app: &AppHandle) -> Result<(), String> {
    let _ = hide_pet_menu(app, false);
    let pet = window(app, "pet")?;
    pet.show().map_err(|_| "无法显示桌宠")?;
    let _ = clamp_pet(app);
    pet.set_focus().map_err(|_| "无法聚焦桌宠")?;
    if is_chat_visible(app).unwrap_or(false) {
        let _ = position_chat(app);
    }
    Ok(())
}

pub fn hide_pet(app: &AppHandle) -> Result<(), String> {
    let _ = hide_pet_menu(app, false);
    hide_chat(app)?;
    window(app, "pet")?
        .hide()
        .map_err(|_| "无法隐藏桌宠".to_owned())
}

fn place_pet_initial(app: &AppHandle) -> Result<(), String> {
    let pet = window(app, "pet")?;
    let area = work_area(&pet)?;
    let size = pet.outer_size().map_err(|_| "无法读取桌宠尺寸")?;
    let inset = (24.0 * pet.scale_factor().unwrap_or(1.0)).round() as i64;
    let desired = PhysicalPosition::new(
        bounded_axis(
            i64::from(area.position.x) + i64::from(area.size.width) - i64::from(size.width) - inset,
            area.position.x,
            area.size.width,
            size.width,
        ),
        bounded_axis(
            i64::from(area.position.y) + i64::from(area.size.height)
                - i64::from(size.height)
                - inset,
            area.position.y,
            area.size.height,
            size.height,
        ),
    );
    pet.set_position(desired)
        .map_err(|_| "无法定位桌宠".to_owned())
}

pub fn setup_tray(app: &mut App) -> tauri::Result<()> {
    let show_pet_item = MenuItem::with_id(app, "show_pet", "显示菲比", true, None::<&str>)?;
    let show_chat_item = MenuItem::with_id(app, "show_chat", "打开聊天", true, None::<&str>)?;
    let show_settings_item =
        MenuItem::with_id(app, "show_settings", "打开设置", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "退出菲比助手", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &show_pet_item,
            &show_chat_item,
            &show_settings_item,
            &quit_item,
        ],
    )?;
    TrayIconBuilder::new()
        .icon(
            app.default_window_icon()
                .ok_or(tauri::Error::WindowNotFound)?
                .clone(),
        )
        .menu(&menu)
        .show_menu_on_left_click(true)
        .tooltip("菲比助手 · 点击可显示隐藏的菲比")
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show_pet" => {
                let _ = show_pet(app);
            }
            "show_chat" => {
                let _ = show_pet(app);
                let _ = show_chat(app);
            }
            "show_settings" => {
                let _ = show_settings(app);
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    if restore_position(app.handle()).is_err() {
        let _ = place_pet_initial(app.handle());
    }
    Ok(())
}

pub fn on_window_event(window: &tauri::Window, event: &WindowEvent) {
    match (window.label(), event) {
        ("pet", WindowEvent::Moved(_)) => {
            let app = window.app_handle();
            let _ = clamp_pet(&app);
            if is_chat_visible(&app).unwrap_or(false) {
                let _ = position_chat(&app);
            }
            schedule_save(&app);
        }
        ("menu", WindowEvent::Focused(false)) => {
            let _ = hide_pet_menu(&window.app_handle(), false);
        }
        ("chat", WindowEvent::CloseRequested { api, .. }) => {
            api.prevent_close();
            let _ = hide_chat(&window.app_handle());
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::{
        bottom_center_anchored_position, chat_position, clamp_position, menu_position,
        normalized_axis, restored_axis,
    };
    use tauri::{PhysicalPosition, PhysicalRect, PhysicalSize};

    #[test]
    fn clamps_pet_to_negative_origin_work_area() {
        let area = PhysicalRect {
            position: PhysicalPosition::new(-1920, 40),
            size: PhysicalSize::new(1920, 1040),
        };
        let size = PhysicalSize::new(240, 320);
        assert_eq!(
            clamp_position(PhysicalPosition::new(-3000, 9999), size, &area),
            PhysicalPosition::new(-1920, 760)
        );
    }

    #[test]
    fn scaling_keeps_bottom_center_anchor_stable() {
        assert_eq!(
            bottom_center_anchored_position(
                PhysicalPosition::new(100, 100),
                PhysicalSize::new(240, 320),
                PhysicalSize::new(360, 480),
            ),
            PhysicalPosition::new(40, -60)
        );
    }

    #[test]
    fn places_chat_to_left_or_right_without_leaving_work_area() {
        let area = PhysicalRect {
            position: PhysicalPosition::new(0, 24),
            size: PhysicalSize::new(1440, 800),
        };
        let pet_size = PhysicalSize::new(240, 320);
        let chat_size = PhysicalSize::new(420, 440);
        assert_eq!(
            chat_position(
                PhysicalPosition::new(1176, 480),
                pet_size,
                chat_size,
                &area,
                12
            ),
            PhysicalPosition::new(744, 360)
        );
        assert_eq!(
            chat_position(
                PhysicalPosition::new(20, 480),
                pet_size,
                chat_size,
                &area,
                12
            ),
            PhysicalPosition::new(272, 360)
        );
    }

    #[test]
    fn places_menu_beside_pet_and_uses_vertical_fallback_on_narrow_screens() {
        let area = PhysicalRect {
            position: PhysicalPosition::new(0, 24),
            size: PhysicalSize::new(1440, 800),
        };
        let pet_size = PhysicalSize::new(240, 320);
        let menu_size = PhysicalSize::new(220, 274);
        assert_eq!(
            menu_position(
                PhysicalPosition::new(1176, 480),
                pet_size,
                menu_size,
                &area,
                10
            ),
            PhysicalPosition::new(946, 526)
        );
        assert_eq!(
            menu_position(
                PhysicalPosition::new(20, 480),
                pet_size,
                menu_size,
                &area,
                10
            ),
            PhysicalPosition::new(270, 526)
        );
        let narrow = PhysicalRect {
            position: PhysicalPosition::new(0, 24),
            size: PhysicalSize::new(390, 900),
        };
        assert_eq!(
            menu_position(
                PhysicalPosition::new(100, 560),
                pet_size,
                menu_size,
                &narrow,
                10
            ),
            PhysicalPosition::new(100, 276)
        );
    }

    #[test]
    fn normalized_coordinates_restore_across_work_area_and_dpi_changes() {
        let x = normalized_axis(-940, -1920, 1920, 240);
        assert!((x - 0.58333).abs() < 0.001);
        assert_eq!(restored_axis(x, 0, 2560, 480), 1213);
        assert_eq!(restored_axis(1.0, -1920, 1920, 240), -240);
    }
}

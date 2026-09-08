//! Preferences window backed by the TOML config.

use crate::app::Omashot;
use crate::config::{AfterCapture, Corner, ImageFormat};
use adw::prelude::*;
use gtk::{gio, glib};
use std::rc::Rc;

fn switch_row(title: &str, subtitle: Option<&str>, value: bool, on_change: impl Fn(bool) + 'static) -> adw::SwitchRow {
    let row = adw::SwitchRow::new();
    row.set_title(title);
    if let Some(s) = subtitle {
        row.set_subtitle(s);
    }
    row.set_active(value);
    row.connect_active_notify(move |r| on_change(r.is_active()));
    row
}

fn spin_row(title: &str, subtitle: Option<&str>, min: f64, max: f64, step: f64, value: f64, on_change: impl Fn(f64) + 'static) -> adw::SpinRow {
    let row = adw::SpinRow::with_range(min, max, step);
    row.set_title(title);
    if let Some(s) = subtitle {
        row.set_subtitle(s);
    }
    row.set_value(value);
    row.connect_value_notify(move |r| on_change(r.value()));
    row
}

fn entry_row(title: &str, value: &str, on_change: impl Fn(String) + 'static) -> adw::EntryRow {
    let row = adw::EntryRow::new();
    row.set_title(title);
    row.set_text(value);
    row.connect_apply(move |r| on_change(r.text().to_string()));
    row.set_show_apply_button(true);
    row
}

fn combo_row(title: &str, items: &[&str], selected: u32, on_change: impl Fn(u32) + 'static) -> adw::ComboRow {
    let row = adw::ComboRow::new();
    row.set_title(title);
    row.set_model(Some(&gtk::StringList::new(items)));
    row.set_selected(selected);
    row.connect_selected_notify(move |r| on_change(r.selected()));
    row
}

fn matrix_group(gb: &Rc<Omashot>, title: &str, get: fn(&crate::config::Config) -> AfterCapture, set: fn(&mut crate::config::Config, AfterCapture)) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    group.set_title(title);
    let cur = get(&gb.config.get());
    let mk = |label: &str, value: bool, apply: fn(&mut AfterCapture, bool)| {
        let gb = gb.clone();
        switch_row(label, None, value, move |v| {
            gb.config.update(|c| {
                let mut a = get(c);
                apply(&mut a, v);
                set(c, a);
            })
        })
    };
    group.add(&mk("Save to disk", cur.save, |a, v| a.save = v));
    group.add(&mk("Copy to clipboard", cur.copy, |a, v| a.copy = v));
    group.add(&mk("Show Quick Access", cur.quick_access, |a, v| a.quick_access = v));
    group.add(&mk("Open in editor", cur.annotate, |a, v| a.annotate = v));
    group
}

pub fn open(gb: &Rc<Omashot>) {
    let outer = gb.clone();
    let cfg = gb.config.get();
    // Categories in a side list, the selected page in the main panel.
    let win = adw::ApplicationWindow::builder().application(&gb.app).title("Omashot Preferences").default_width(860).default_height(640).build();
    win.add_css_class("omashot-window");
    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let header = adw::HeaderBar::new();
    root.append(&header);
    let body = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    body.set_vexpand(true);
    let nav = gtk::ListBox::new();
    nav.add_css_class("navigation-sidebar");
    nav.add_css_class("prefs-nav");
    nav.set_selection_mode(gtk::SelectionMode::Single);
    nav.set_size_request(210, -1);
    let nav_scroller = gtk::ScrolledWindow::new();
    nav_scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    nav_scroller.set_child(Some(&nav));
    body.append(&nav_scroller);
    body.append(&gtk::Separator::new(gtk::Orientation::Vertical));
    let stack = gtk::Stack::new();
    stack.set_hexpand(true);
    stack.set_transition_type(gtk::StackTransitionType::Crossfade);
    body.append(&stack);
    root.append(&body);
    win.set_content(Some(&root));
    {
        let stack = stack.clone();
        nav.connect_row_selected(move |_, row| {
            if let Some(name) = row.and_then(|r| r.widget_name().to_string().into()) {
                stack.set_visible_child_name(&name);
            }
        });
    }
    let add_page = |page: &adw::PreferencesPage| {
        let name = page.title().to_string();
        let scroller = gtk::ScrolledWindow::new();
        scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroller.set_child(Some(page));
        stack.add_titled(&scroller, Some(&name), &name);
        let row = gtk::ListBoxRow::new();
        row.set_widget_name(&name);
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        content.set_margin_top(8);
        content.set_margin_bottom(8);
        content.set_margin_start(6);
        content.set_margin_end(6);
        if let Some(icon) = page.icon_name() {
            content.append(&gtk::Image::from_icon_name(&icon));
        }
        content.append(&gtk::Label::new(Some(&name)));
        row.set_child(Some(&content));
        nav.append(&row);
        if nav.selected_row().is_none() {
            nav.select_row(Some(&row));
        }
    };

    // ----- General -----
    let general = adw::PreferencesPage::new();
    general.set_title("General");
    general.set_icon_name(Some("preferences-system-symbolic"));
    let g_files = adw::PreferencesGroup::new();
    g_files.set_title("Files");
    let folder_row = adw::ActionRow::new();
    folder_row.set_title("Save folder");
    folder_row.set_subtitle(&cfg.general.save_folder.to_string_lossy());
    let pick = gtk::Button::from_icon_name("folder-open-symbolic");
    pick.set_valign(gtk::Align::Center);
    {
        let gb = outer.clone();
        let win = win.clone();
        let row = folder_row.clone();
        pick.connect_clicked(move |_| {
            let dialog = gtk::FileDialog::new();
            dialog.set_title("Choose screenshots folder");
            let gb = gb.clone();
            let row = row.clone();
            dialog.select_folder(Some(&win), gio::Cancellable::NONE, move |res| {
                if let Ok(f) = res {
                    if let Some(p) = f.path() {
                        row.set_subtitle(&p.to_string_lossy());
                        gb.config.update(|c| c.general.save_folder = p);
                    }
                }
            });
        });
    }
    folder_row.add_suffix(&pick);
    g_files.add(&folder_row);
    {
        let gb = outer.clone();
        g_files.add(&entry_row("Filename pattern (strftime)", &cfg.general.filename_pattern, move |v| gb.config.update(|c| c.general.filename_pattern = v)));
    }
    {
        let gb = outer.clone();
        let sel = match cfg.general.format {
            ImageFormat::Png => 0,
            ImageFormat::Jpg => 1,
            ImageFormat::Webp => 2,
        };
        g_files.add(&combo_row("Format", &["PNG", "JPG", "WebP"], sel, move |i| {
            gb.config.update(|c| c.general.format = match i {
                1 => ImageFormat::Jpg,
                2 => ImageFormat::Webp,
                _ => ImageFormat::Png,
            })
        }));
    }
    {
        let gb = outer.clone();
        g_files.add(&spin_row("JPG quality", None, 1.0, 100.0, 1.0, cfg.general.quality as f64, move |v| gb.config.update(|c| c.general.quality = v as u8)));
    }
    general.add(&g_files);
    let g_capture = adw::PreferencesGroup::new();
    g_capture.set_title("Capture");
    {
        let gb = outer.clone();
        g_capture.add(&switch_row("Include cursor", None, cfg.general.include_cursor, move |v| gb.config.update(|c| c.general.include_cursor = v)));
        let gb = outer.clone();
        g_capture.add(&switch_row("Shutter sound", None, cfg.general.sound, move |v| gb.config.update(|c| c.general.sound = v)));
        let gb = outer.clone();
        g_capture.add(&switch_row("Notifications", Some("Shown when Quick Access is off"), cfg.general.notifications, move |v| gb.config.update(|c| c.general.notifications = v)));
        let gb = outer.clone();
        g_capture.add(&switch_row("Remember last area", Some("Press Enter in the overlay to reuse it"), cfg.general.remember_last_area, move |v| gb.config.update(|c| c.general.remember_last_area = v)));
        let gb = outer.clone();
        g_capture.add(&spin_row("Capture delay (ms)", None, 0.0, 10_000.0, 100.0, cfg.general.delay_ms as f64, move |v| gb.config.update(|c| c.general.delay_ms = v as u32)));
    }
    general.add(&g_capture);
    add_page(&general);

    // ----- After capture -----
    let post = adw::PreferencesPage::new();
    post.set_title("After Capture");
    post.set_icon_name(Some("emblem-ok-symbolic"));
    post.add(&matrix_group(gb, "Fullscreen", |c| c.post_capture.fullscreen, |c, a| c.post_capture.fullscreen = a));
    post.add(&matrix_group(gb, "Area", |c| c.post_capture.area, |c, a| c.post_capture.area = a));
    post.add(&matrix_group(gb, "Window", |c| c.post_capture.window, |c, a| c.post_capture.window = a));
    post.add(&matrix_group(gb, "Editor save", |c| c.post_capture.annotate_export, |c, a| c.post_capture.annotate_export = a));
    add_page(&post);

    // ----- Quick Access -----
    let qa = adw::PreferencesPage::new();
    qa.set_title("Quick Access");
    qa.set_icon_name(Some("view-grid-symbolic"));
    let g_qa = adw::PreferencesGroup::new();
    {
        let gb = outer.clone();
        g_qa.add(&switch_row("Show Quick Access panel", None, cfg.quick_access.enabled, move |v| gb.config.update(|c| c.quick_access.enabled = v)));
        let gb = outer.clone();
        let sel = match cfg.quick_access.corner {
            Corner::TopLeft => 0,
            Corner::TopRight => 1,
            Corner::BottomLeft => 2,
            Corner::BottomRight => 3,
        };
        g_qa.add(&combo_row("Position", &["Top left", "Top right", "Bottom left", "Bottom right"], sel, move |i| {
            gb.config.update(|c| c.quick_access.corner = match i {
                0 => Corner::TopLeft,
                1 => Corner::TopRight,
                2 => Corner::BottomLeft,
                _ => Corner::BottomRight,
            })
        }));
        let gb = outer.clone();
        g_qa.add(&spin_row("Auto-dismiss (seconds)", Some("0 keeps cards until dismissed"), 0.0, 120.0, 1.0, cfg.quick_access.auto_dismiss_secs as f64, move |v| gb.config.update(|c| c.quick_access.auto_dismiss_secs = v as u32)));
        let gb = outer.clone();
        g_qa.add(&spin_row("Maximum cards", None, 1.0, 10.0, 1.0, cfg.quick_access.max_cards as f64, move |v| gb.config.update(|c| c.quick_access.max_cards = v as usize)));
        let gb = outer.clone();
        g_qa.add(&spin_row("Thumbnail width", None, 120.0, 480.0, 10.0, cfg.quick_access.thumbnail_width as f64, move |v| gb.config.update(|c| c.quick_access.thumbnail_width = v as i32)));
        let gb = outer.clone();
        g_qa.add(&switch_row("Keep editing after drag", Some("Leave the editor open after dragging into another app"), cfg.quick_access.keep_editing_after_drag, move |v| gb.config.update(|c| c.quick_access.keep_editing_after_drag = v)));
    }
    qa.add(&g_qa);
    add_page(&qa);

    // ----- Annotate -----
    let ann = adw::PreferencesPage::new();
    ann.set_title("Editor");
    ann.set_icon_name(Some("document-edit-symbolic"));
    let g_ann = adw::PreferencesGroup::new();
    g_ann.set_title("Defaults");
    {
        let gb = outer.clone();
        g_ann.add(&entry_row("Stroke color (hex)", &cfg.annotate.stroke_color, move |v| gb.config.update(|c| c.annotate.stroke_color = v)));
        let gb = outer.clone();
        g_ann.add(&spin_row("Stroke width", None, 1.0, 20.0, 1.0, cfg.annotate.stroke_width, move |v| gb.config.update(|c| c.annotate.stroke_width = v)));
        let gb = outer.clone();
        g_ann.add(&entry_row("Font family", &cfg.annotate.font_family, move |v| gb.config.update(|c| c.annotate.font_family = v)));
        let gb = outer.clone();
        g_ann.add(&spin_row("Font size", None, 6.0, 200.0, 1.0, cfg.annotate.font_size, move |v| gb.config.update(|c| c.annotate.font_size = v)));
        let gb = outer.clone();
        g_ann.add(&combo_row("Blur style", &["Pixelate", "Gaussian"], if cfg.annotate.blur_style == "gaussian" { 1 } else { 0 }, move |i| {
            gb.config.update(|c| c.annotate.blur_style = if i == 1 { "gaussian".into() } else { "pixelate".into() })
        }));
        let gb = outer.clone();
        g_ann.add(&spin_row("Blur strength", None, 1.0, 20.0, 1.0, cfg.annotate.blur_strength, move |v| gb.config.update(|c| c.annotate.blur_strength = v)));
        let gb = outer.clone();
        g_ann.add(&entry_row("Watermark text", &cfg.annotate.watermark_text, move |v| gb.config.update(|c| c.annotate.watermark_text = v)));
    }
    ann.add(&g_ann);
    add_page(&ann);

    // ----- History & OCR -----
    let hist = adw::PreferencesPage::new();
    hist.set_title("History & OCR");
    hist.set_icon_name(Some("document-open-recent-symbolic"));
    let g_hist = adw::PreferencesGroup::new();
    g_hist.set_title("History");
    {
        let gb = outer.clone();
        g_hist.add(&switch_row("Record captures", None, cfg.history.enabled, move |v| gb.config.update(|c| c.history.enabled = v)));
        let gb = outer.clone();
        g_hist.add(&spin_row("Retention (days)", Some("0 keeps entries forever; files on disk are never deleted"), 0.0, 3650.0, 1.0, cfg.history.retention_days as f64, move |v| gb.config.update(|c| c.history.retention_days = v as u32)));
        let gb = outer.clone();
        g_hist.add(&spin_row("Maximum entries", None, 0.0, 100_000.0, 50.0, cfg.history.max_entries as f64, move |v| gb.config.update(|c| c.history.max_entries = v as u32)));
    }
    hist.add(&g_hist);
    let g_ocr = adw::PreferencesGroup::new();
    g_ocr.set_title("OCR");
    {
        let gb = outer.clone();
        g_ocr.add(&entry_row("Tesseract languages", &cfg.ocr.languages, move |v| gb.config.update(|c| c.ocr.languages = v)));
        let gb = outer.clone();
        g_ocr.add(&switch_row("Copy recognized text to clipboard", None, cfg.ocr.copy_to_clipboard, move |v| gb.config.update(|c| c.ocr.copy_to_clipboard = v)));
    }
    hist.add(&g_ocr);
    add_page(&hist);

    // ----- Shortcuts -----
    let keys = adw::PreferencesPage::new();
    keys.set_title("Shortcuts");
    keys.set_icon_name(Some("input-keyboard-symbolic"));
    let g_keys = adw::PreferencesGroup::new();
    g_keys.set_title("Hyprland bindings");
    g_keys.set_description(Some("Global shortcuts belong to the compositor. Add these lines to ~/.config/hypr/bindings.conf (or your keybinds file) and reload Hyprland."));
    let exe = std::env::current_exe().map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|_| "omashot".into());
    let snippet = format!(
        "bind = , PRINT, exec, {exe} area\nbind = SHIFT, PRINT, exec, {exe} window\nbind = CTRL, PRINT, exec, {exe} full\nbind = ALT, PRINT, exec, {exe} area --annotate\nbind = SUPER SHIFT, T, exec, {exe} ocr\nbind = SUPER SHIFT, H, exec, {exe} history\nexec-once = {exe} daemon\n"
    );
    let view = gtk::TextView::new();
    view.set_editable(false);
    view.set_monospace(true);
    view.buffer().set_text(&snippet);
    view.set_margin_top(8);
    view.set_margin_bottom(8);
    view.set_margin_start(8);
    view.set_margin_end(8);
    let frame = gtk::Frame::new(None);
    frame.set_child(Some(&view));
    g_keys.add(&frame);
    let copy = gtk::Button::with_label("Copy bindings");
    copy.set_halign(gtk::Align::Start);
    copy.set_margin_top(8);
    {
        let snippet = snippet.clone();
        copy.connect_clicked(move |_| {
            let _ = crate::clipboard::copy_text(&snippet);
        });
    }
    g_keys.add(&copy);
    keys.add(&g_keys);
    let g_editor_keys = adw::PreferencesGroup::new();
    g_editor_keys.set_title("Editor keys");
    for (k, d) in [
        ("V C R F O A L T H B S N W P", "Select, Crop, Rectangle, Filled, Oval, Arrow, Line, Text, Highlighter, Blur, Spotlight, Counter, Watermark, Pencil"),
        ("Ctrl+Z / Ctrl+Shift+Z", "Undo / redo"),
        ("Ctrl+S / Ctrl+Shift+C / Ctrl+E", "Save / copy and close / export as"),
        ("Ctrl+C, Ctrl+V, Ctrl+D, Ctrl+A", "Copy, paste, duplicate, select all annotations"),
        ("Ctrl+scroll, Ctrl+0, Ctrl+1", "Zoom, fit, actual size; hold Space to pan"),
        ("Enter / Esc / A (crop)", "Apply crop / cancel / auto-crop to content"),
        ("Shift while drawing", "Square, circle, or 45° constraint"),
    ] {
        let row = adw::ActionRow::new();
        row.set_title(k);
        row.set_subtitle(d);
        g_editor_keys.add(&row);
    }
    keys.add(&g_editor_keys);
    add_page(&keys);

    let _ = glib::MainContext::default();
    win.present();
}

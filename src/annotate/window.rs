//! The annotation editor window: toolbar, property bar, sidebar, canvas, bottom bar.

use super::canvas::{Canvas, Tool, ToolOptions};
use super::icons::tool_icon;
use super::model::*;
use super::{redact, session};
use crate::app::Omacapture;
use crate::capture::Frame;
use adw::prelude::*;
use gtk::{gdk, gio, glib};
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

pub struct EditorWindow {
    pub win: adw::ApplicationWindow,
    pub canvas: Canvas,
    gb: Rc<Omacapture>,
    source: RefCell<Option<PathBuf>>,
    props: Props,
    sidebar: Sidebar,
    tool_buttons: RefCell<Vec<(Tool, gtk::ToggleButton)>>,
    zoom_label: gtk::Label,
    undo_btn: gtk::Button,
    redo_btn: gtk::Button,
    words: RefCell<Option<Vec<crate::ocr::Word>>>,
    updating: Cell<bool>,
    toast: adw::ToastOverlay,
    _hold: gio::ApplicationHoldGuard,
}

struct Props {
    bar: gtk::Box,
    palette: gtk::Box,
    color_btn: gtk::ColorDialogButton,
    width: gtk::SpinButton,
    width_box: gtk::Box,
    line_style: gtk::DropDown,
    radius: gtk::SpinButton,
    radius_box: gtk::Box,
    font_size: gtk::SpinButton,
    font_box: gtk::Box,
    text_pres: gtk::DropDown,
    arrow_style: gtk::DropDown,
    arrow_type: gtk::DropDown,
    head_start: gtk::DropDown,
    head_end: gtk::DropDown,
    blur_effect: gtk::DropDown,
    blur_strength: gtk::Scale,
    blur_box: gtk::Box,
    redact_btn: gtk::Button,
    dim: gtk::Scale,
    dim_box: gtk::Box,
    counter_size: gtk::Scale,
    counter_box: gtk::Box,
    wm_text: gtk::Entry,
    wm_style: gtk::DropDown,
    wm_opacity: gtk::Scale,
    wm_rotation: gtk::Scale,
    wm_box: gtk::Box,
    text_snap: gtk::ToggleButton,
    crop_box: gtk::Box,
    crop_aspect: gtk::DropDown,
    crop_portrait: gtk::ToggleButton,
    crop_snap: gtk::ToggleButton,
    hint: gtk::Label,
}

struct Sidebar {
    revealer: gtk::Revealer,
    bg_kind: gtk::DropDown,
    gradients: gtk::Box,
    solid: gtk::ColorDialogButton,
    image_btn: gtk::Button,
    blur_strength: gtk::Scale,
    padding: gtk::Scale,
    radius: gtk::Scale,
    shadow: gtk::Scale,
    aspect: gtk::DropDown,
    portrait: gtk::ToggleButton,
}

fn dropdown(items: &[&str]) -> gtk::DropDown {
    let list = gtk::StringList::new(items);
    let d = gtk::DropDown::new(Some(list), gtk::Expression::NONE);
    d.set_valign(gtk::Align::Center);
    d
}

fn labeled(label: &str, w: &impl IsA<gtk::Widget>) -> gtk::Box {
    let b = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    let l = gtk::Label::new(Some(label));
    l.add_css_class("dim-label");
    l.add_css_class("caption");
    b.append(&l);
    b.append(w);
    b.set_valign(gtk::Align::Center);
    b
}

fn scale(min: f64, max: f64, step: f64, width: i32) -> gtk::Scale {
    let s = gtk::Scale::with_range(gtk::Orientation::Horizontal, min, max, step);
    s.set_size_request(width, -1);
    s.set_draw_value(false);
    s.set_valign(gtk::Align::Center);
    s
}

const CROP_ASPECTS: [(&str, Option<(f64, f64)>); 6] = [
    ("Free", None),
    ("1:1", Some((1.0, 1.0))),
    ("4:3", Some((4.0, 3.0))),
    ("3:2", Some((3.0, 2.0))),
    ("16:9", Some((16.0, 9.0))),
    ("21:9", Some((21.0, 9.0))),
];
const CANVAS_ASPECTS: [(&str, Option<(f64, f64)>); 5] =
    [("Auto", None), ("1:1", Some((1.0, 1.0))), ("4:3", Some((4.0, 3.0))), ("3:2", Some((3.0, 2.0))), ("16:9", Some((16.0, 9.0)))];

impl EditorWindow {
    pub fn open(gb: &Rc<Omacapture>, frame: Frame, source: Option<PathBuf>) -> Rc<Self> {
        let cfg = gb.config.get();
        // Restore an editable session when the file has one.
        let (frame, sheet) = match source.as_deref().and_then(session::load) {
            Some((f, sheet)) => (f, Some(sheet)),
            None => (frame, None),
        };
        let mut style = Style::default();
        style.color = Color::parse(&cfg.annotate.stroke_color).unwrap_or(style.color);
        // With the stock default, follow the Omarchy theme's red instead.
        if cfg.annotate.stroke_color == "#ff3b30" {
            if let Some(c) = crate::theme::current().and_then(|t| Color::parse(&t.red)) {
                style.color = c;
            }
        }
        style.width = cfg.annotate.stroke_width;
        style.font_family = cfg.annotate.font_family.clone();
        style.font_size = cfg.annotate.font_size;
        let options = ToolOptions {
            blur_strength: cfg.annotate.blur_strength.clamp(1.0, 20.0),
            blur_effect: if cfg.annotate.blur_style == "gaussian" { BlurEffect::Gaussian } else { BlurEffect::Pixelate },
            watermark_text: cfg.annotate.watermark_text.clone(),
            ..ToolOptions::default()
        };

        let canvas = Canvas::new(frame, style, options);
        if let Some(sheet) = sheet {
            canvas.state.borrow_mut().doc.sheet = sheet;
        }

        let win = adw::ApplicationWindow::builder()
            .application(&gb.app)
            .default_width(1200)
            .default_height(800)
            .title(Self::title_for(source.as_deref()))
            .build();
        win.add_css_class("omacapture-window");

        let toast = adw::ToastOverlay::new();
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let header = adw::HeaderBar::new();
        let toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        let toolbar_scroller = gtk::ScrolledWindow::new();
        toolbar_scroller.set_policy(gtk::PolicyType::External, gtk::PolicyType::Never);
        toolbar_scroller.set_child(Some(&toolbar));
        toolbar_scroller.set_propagate_natural_width(true);
        toolbar_scroller.set_propagate_natural_height(true);
        header.set_title_widget(Some(&toolbar_scroller));

        let undo_btn = gtk::Button::from_icon_name("edit-undo-symbolic");
        undo_btn.set_tooltip_text(Some("Undo (Ctrl+Z)"));
        let redo_btn = gtk::Button::from_icon_name("edit-redo-symbolic");
        redo_btn.set_tooltip_text(Some("Redo (Ctrl+Shift+Z)"));
        header.pack_start(&undo_btn);
        header.pack_start(&redo_btn);

        let sidebar_btn = gtk::ToggleButton::new();
        sidebar_btn.set_icon_name("sidebar-show-right-symbolic");
        sidebar_btn.set_tooltip_text(Some("Canvas & background (Ctrl+B)"));
        header.pack_end(&sidebar_btn);
        let menu_btn = gtk::MenuButton::new();
        menu_btn.set_icon_name("open-menu-symbolic");
        header.pack_end(&menu_btn);

        root.append(&header);
        let props = Self::build_props();
        // Never let a wide property bar dictate the window's minimum width.
        let props_scroller = gtk::ScrolledWindow::new();
        props_scroller.set_policy(gtk::PolicyType::External, gtk::PolicyType::Never);
        props_scroller.set_child(Some(&props.bar));
        props_scroller.set_propagate_natural_height(true);
        root.append(&props_scroller);

        let body = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        body.append(&canvas.widget);
        let sidebar = Self::build_sidebar();
        body.append(&sidebar.revealer);
        body.set_vexpand(true);
        root.append(&body);

        let bottom = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        bottom.add_css_class("toolbar");
        bottom.set_margin_start(8);
        bottom.set_margin_end(8);
        let zoom_label = gtk::Label::new(Some("100%"));
        zoom_label.set_width_chars(5);
        root.append(&bottom);
        toast.set_child(Some(&root));
        win.set_content(Some(&toast));

        let this = Rc::new(EditorWindow {
            win: win.clone(),
            canvas: canvas.clone(),
            gb: gb.clone(),
            source: RefCell::new(source),
            props,
            sidebar,
            tool_buttons: RefCell::new(Vec::new()),
            zoom_label: zoom_label.clone(),
            undo_btn: undo_btn.clone(),
            redo_btn: redo_btn.clone(),
            words: RefCell::new(None),
            updating: Cell::new(false),
            toast,
            _hold: gb.app.hold(),
        });

        // Tool buttons.
        for tool in Tool::ALL {
            let b = gtk::ToggleButton::new();
            b.set_child(Some(&tool_icon(tool)));
            b.set_tooltip_text(Some(&format!("{} ({})", tool.label(), tool.key().to_ascii_uppercase())));
            b.add_css_class("flat");
            b.add_css_class("tool-button");
            let t = this.clone();
            b.connect_toggled(move |btn| {
                if btn.is_active() && !t.updating.get() {
                    t.canvas.set_tool(tool);
                }
            });
            toolbar.append(&b);
            this.tool_buttons.borrow_mut().push((tool, b));
        }

        // Bottom bar.
        let zoom_menu = dropdown(&["Fit", "25%", "50%", "100%", "200%", "400%"]);
        zoom_menu.set_selected(gtk::INVALID_LIST_POSITION);
        {
            let t = this.clone();
            zoom_menu.connect_selected_notify(move |d| {
                if t.updating.get() {
                    return;
                }
                match d.selected() {
                    0 => t.canvas.zoom_fit(),
                    1 => t.canvas.zoom_to(0.25),
                    2 => t.canvas.zoom_to(0.5),
                    3 => t.canvas.zoom_actual(),
                    4 => t.canvas.zoom_to(2.0),
                    5 => t.canvas.zoom_to(4.0),
                    _ => {}
                }
            });
        }
        bottom.append(&zoom_menu);
        bottom.append(&zoom_label);
        let zoom_out = gtk::Button::from_icon_name("zoom-out-symbolic");
        let zoom_in = gtk::Button::from_icon_name("zoom-in-symbolic");
        zoom_out.add_css_class("flat");
        zoom_in.add_css_class("flat");
        {
            let t = this.clone();
            zoom_out.connect_clicked(move |_| t.canvas.zoom_by(1.0 / 1.25));
            let t = this.clone();
            zoom_in.connect_clicked(move |_| t.canvas.zoom_by(1.25));
        }
        bottom.append(&zoom_out);
        bottom.append(&zoom_in);

        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        bottom.append(&spacer);

        let drag_handle = gtk::Button::new();
        let dh_content = adw::ButtonContent::new();
        dh_content.set_icon_name("list-drag-handle-symbolic");
        dh_content.set_label("Drag to app");
        drag_handle.set_child(Some(&dh_content));
        drag_handle.add_css_class("flat");
        drag_handle.set_tooltip_text(Some("Drag the rendered image into another application"));
        let drag_src = gtk::DragSource::new();
        drag_src.set_actions(gdk::DragAction::COPY);
        {
            let t = this.clone();
            drag_src.connect_prepare(move |src, _, _| {
                let img = t.canvas.render_export();
                let path = crate::export::write_temp_png(&img).ok()?;
                let thumb =
                    image::imageops::thumbnail(&img, 160, (160.0 * img.height() as f64 / img.width().max(1) as f64).max(1.0) as u32);
                src.set_icon(Some(&Frame { image: thumb, scale: 1.0 }.to_texture()), 0, 0);
                let file = gio::File::for_path(&path);
                let uri = format!("{}\r\n", file.uri());
                let png = crate::export::encode_png(&img).ok()?;
                Some(gdk::ContentProvider::new_union(&[
                    gdk::ContentProvider::for_value(&gdk::FileList::from_array(&[file]).to_value()),
                    gdk::ContentProvider::for_bytes("text/uri-list", &glib::Bytes::from_owned(uri.into_bytes())),
                    gdk::ContentProvider::for_bytes("image/png", &glib::Bytes::from_owned(png)),
                ]))
            });
            let t = this.clone();
            drag_src.connect_drag_end(move |_, _, _| {
                if t.gb.config.get().quick_access.keep_editing_after_drag {
                    t.win.present();
                } else {
                    t.save(false);
                    t.win.destroy();
                }
            });
        }
        drag_handle.add_controller(drag_src);
        bottom.append(&drag_handle);

        let spacer2 = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer2.set_hexpand(true);
        bottom.append(&spacer2);

        let mk = |icon: &str, label: &str, tip: &str| {
            let b = gtk::Button::new();
            let c = adw::ButtonContent::new();
            c.set_icon_name(icon);
            c.set_label(label);
            b.set_child(Some(&c));
            b.set_tooltip_text(Some(tip));
            b
        };
        let copy_btn = mk("edit-copy-symbolic", "Copy", "Copy image (Ctrl+C)");
        let copy_close_btn = mk("edit-copy-symbolic", "Copy & Close", "Copy image and close (Ctrl+Shift+C)");
        let save_btn = mk("document-save-symbolic", "Save", "Save (Ctrl+S)");
        save_btn.add_css_class("suggested-action");
        {
            let t = this.clone();
            copy_btn.connect_clicked(move |_| t.copy_image());
            let t = this.clone();
            copy_close_btn.connect_clicked(move |_| {
                t.copy_image();
                t.win.destroy();
            });
            let t = this.clone();
            save_btn.connect_clicked(move |_| {
                t.save(false);
            });
        }
        bottom.append(&copy_btn);
        bottom.append(&copy_close_btn);
        bottom.append(&save_btn);

        // Window menu.
        let menu = gio::Menu::new();
        menu.append(Some("Save"), Some("editor.save"));
        menu.append(Some("Export As…"), Some("editor.export"));
        menu.append(Some("Extract Text"), Some("editor.extract-text"));
        menu.append(Some("Redact Sensitive Data"), Some("editor.redact"));
        menu.append(Some("Open in Default App"), Some("editor.open-external"));
        menu.append(Some("Delete File"), Some("editor.delete-file"));
        menu.append(Some("Preferences"), Some("editor.preferences"));
        menu_btn.set_menu_model(Some(&menu));
        this.install_actions();
        {
            let t = this.clone();
            sidebar_btn.connect_toggled(move |b| t.sidebar.revealer.set_reveal_child(b.is_active()));
        }

        // Context menu on the canvas.
        let ctx_menu = gio::Menu::new();
        ctx_menu.append(Some("Copy Image"), Some("editor.copy-image"));
        ctx_menu.append(Some("Extract Text"), Some("editor.extract-text"));
        ctx_menu.append(Some("Duplicate"), Some("editor.duplicate"));
        ctx_menu.append(Some("Delete"), Some("editor.delete"));
        let popover = gtk::PopoverMenu::from_model(Some(&ctx_menu));
        popover.set_parent(&canvas.area);
        popover.set_has_arrow(false);
        let right = gtk::GestureClick::new();
        right.set_button(3);
        {
            let popover = popover.clone();
            right.connect_pressed(move |_, _, x, y| {
                popover.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
                popover.popup();
            });
        }
        canvas.area.add_controller(right);

        // Undo / redo / keyboard.
        {
            let t = this.clone();
            undo_btn.connect_clicked(move |_| t.canvas.undo());
            let t = this.clone();
            redo_btn.connect_clicked(move |_| t.canvas.redo());
        }
        this.install_keys();
        this.wire_props();
        this.wire_sidebar();

        {
            let t = this.clone();
            canvas.set_on_change(move || t.refresh());
        }
        {
            let t = this.clone();
            win.connect_close_request(move |_| t.confirm_close());
        }
        this.refresh();
        this.start_ocr();
        win.present();
        canvas.area.grab_focus();
        this
    }

    fn title_for(source: Option<&std::path::Path>) -> String {
        match source.and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string()) {
            Some(n) => format!("{n} — Omacapture"),
            None => "Untitled — Omacapture".into(),
        }
    }

    // ----- property bar -----

    #[allow(deprecated)]
    fn build_props() -> Props {
        let bar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        bar.add_css_class("toolbar");
        bar.add_css_class("props-bar");
        bar.set_margin_start(8);
        bar.set_margin_end(8);

        let palette = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        let swatches: Vec<String> =
            crate::theme::current().map(|t| t.palette()).unwrap_or_else(|| PALETTE.iter().map(|s| s.to_string()).collect());
        for hex in swatches {
            let hex = hex.as_str();
            let b = gtk::Button::new();
            b.add_css_class("swatch");
            b.add_css_class("flat");
            let css = gtk::CssProvider::new();
            css.load_from_string(&format!(".swatch {{ background: {hex}; min-width: 14px; min-height: 14px; border-radius: 0; padding: 2px; margin: 2px; border: 1px solid rgba(0,0,0,0.35); }} .swatch:hover {{ border-color: @window_fg_color; }}"));
            b.style_context().add_provider(&css, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1);
            b.set_tooltip_text(Some(hex));
            b.set_valign(gtk::Align::Center);
            palette.append(&b);
        }
        let color_btn = gtk::ColorDialogButton::new(Some(gtk::ColorDialog::new()));
        color_btn.set_valign(gtk::Align::Center);
        color_btn.set_tooltip_text(Some("Custom color"));
        palette.append(&color_btn);
        bar.append(&palette);

        let width = gtk::SpinButton::with_range(1.0, 20.0, 1.0);
        width.set_valign(gtk::Align::Center);
        let width_box = labeled("Width", &width);
        bar.append(&width_box);
        let line_style = dropdown(&["Solid", "Dashed", "Dotted"]);
        bar.append(&line_style);
        let radius = gtk::SpinButton::with_range(0.0, 200.0, 1.0);
        radius.set_valign(gtk::Align::Center);
        let radius_box = labeled("Radius", &radius);
        bar.append(&radius_box);
        let font_size = gtk::SpinButton::with_range(6.0, 300.0, 1.0);
        font_size.set_valign(gtk::Align::Center);
        let font_box = labeled("Size", &font_size);
        bar.append(&font_box);
        let text_pres = dropdown(&["Plain", "Label", "Callout"]);
        bar.append(&text_pres);
        let arrow_style = dropdown(&["Straight", "Curved right", "Curved left"]);
        let arrow_type = dropdown(&["Classic", "Tapered", "Outlined"]);
        let head_start = dropdown(&["Start: none", "Start: arrow", "Start: circle"]);
        let head_end = dropdown(&["End: none", "End: arrow", "End: circle"]);
        for d in [&arrow_style, &arrow_type, &head_start, &head_end] {
            bar.append(d);
        }
        let blur_effect = dropdown(&BlurEffect::ALL.map(|e| e.label()));
        let blur_strength = scale(1.0, 20.0, 1.0, 110);
        let redact_btn = gtk::Button::with_label("Auto-redact");
        redact_btn.set_tooltip_text(Some("Detect and pixelate emails, phone numbers, tokens, and card numbers"));
        redact_btn.set_valign(gtk::Align::Center);
        let blur_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        blur_box.append(&blur_effect);
        blur_box.append(&labeled("Strength", &blur_strength));
        blur_box.append(&redact_btn);
        bar.append(&blur_box);
        let dim = scale(0.1, 0.9, 0.05, 110);
        let dim_box = labeled("Dim", &dim);
        bar.append(&dim_box);
        let counter_size = scale(1.0, 12.0, 1.0, 110);
        let counter_box = labeled("Size", &counter_size);
        bar.append(&counter_box);
        let wm_text = gtk::Entry::new();
        wm_text.set_placeholder_text(Some("Watermark text"));
        wm_text.set_valign(gtk::Align::Center);
        let wm_style = dropdown(&["Single", "Diagonal", "Tiled"]);
        let wm_opacity = scale(0.05, 1.0, 0.05, 90);
        let wm_rotation = scale(-45.0, 45.0, 1.0, 90);
        let wm_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        wm_box.append(&wm_text);
        wm_box.append(&wm_style);
        wm_box.append(&labeled("Opacity", &wm_opacity));
        wm_box.append(&labeled("Angle", &wm_rotation));
        bar.append(&wm_box);
        let text_snap = gtk::ToggleButton::with_label("Snap to text");
        text_snap.set_tooltip_text(Some("Snap highlighter strokes to detected text lines (hold Ctrl to bypass)"));
        text_snap.set_valign(gtk::Align::Center);
        bar.append(&text_snap);

        let crop_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let crop_aspect = dropdown(&CROP_ASPECTS.map(|a| a.0));
        let crop_portrait = gtk::ToggleButton::with_label("Portrait");
        crop_portrait.set_valign(gtk::Align::Center);
        let crop_snap = gtk::ToggleButton::with_label("Snap edges");
        crop_snap.set_valign(gtk::Align::Center);
        let crop_auto = gtk::Button::with_label("Auto (A)");
        crop_auto.set_valign(gtk::Align::Center);
        let crop_cancel = gtk::Button::with_label("Cancel");
        crop_cancel.set_valign(gtk::Align::Center);
        let crop_apply = gtk::Button::with_label("Apply crop");
        crop_apply.add_css_class("suggested-action");
        crop_apply.set_valign(gtk::Align::Center);
        crop_box.append(&labeled("Aspect", &crop_aspect));
        crop_box.append(&crop_portrait);
        crop_box.append(&crop_snap);
        crop_box.append(&crop_auto);
        crop_box.append(&crop_cancel);
        crop_box.append(&crop_apply);
        crop_auto.set_widget_name("crop-auto");
        crop_cancel.set_widget_name("crop-cancel");
        crop_apply.set_widget_name("crop-apply");
        bar.append(&crop_box);

        let hint = gtk::Label::new(Some("Select an item or pick a tool"));
        hint.add_css_class("dim-label");
        bar.append(&hint);

        Props {
            bar,
            palette,
            color_btn,
            width,
            width_box,
            line_style,
            radius,
            radius_box,
            font_size,
            font_box,
            text_pres,
            arrow_style,
            arrow_type,
            head_start,
            head_end,
            blur_effect,
            blur_strength,
            blur_box,
            redact_btn,
            dim,
            dim_box,
            counter_size,
            counter_box,
            wm_text,
            wm_style,
            wm_opacity,
            wm_rotation,
            wm_box,
            text_snap,
            crop_box,
            crop_aspect,
            crop_portrait,
            crop_snap,
            hint,
        }
    }

    fn wire_props(self: &Rc<Self>) {
        let p = &self.props;
        // Palette swatches.
        let mut child = p.palette.first_child();
        while let Some(c) = child {
            if let Some(b) = c.downcast_ref::<gtk::Button>() {
                if let Some(hex) = b.tooltip_text().map(|t| t.to_string()) {
                    let t = self.clone();
                    b.connect_clicked(move |_| {
                        if let Some(col) = Color::parse(&hex) {
                            t.canvas.apply_style("color", move |s| s.color = col);
                        }
                    });
                }
            }
            child = c.next_sibling();
        }
        {
            let t = self.clone();
            p.color_btn.connect_rgba_notify(move |b| {
                if t.updating.get() {
                    return;
                }
                let col = Color::from_gdk(&b.rgba());
                t.canvas.apply_style("color", move |s| s.color = col);
            });
        }
        macro_rules! on_value {
            ($w:expr, $key:literal, $sig:ident, $get:expr, $apply:expr) => {{
                let t = self.clone();
                $w.$sig(move |w| {
                    if t.updating.get() {
                        return;
                    }
                    let v = $get(w);
                    t.canvas.apply_style($key, move |s| $apply(s, v));
                });
            }};
        }
        on_value!(p.width, "width", connect_value_changed, |w: &gtk::SpinButton| w.value(), |s: &mut Style, v| s.width = v);
        on_value!(p.radius, "radius", connect_value_changed, |w: &gtk::SpinButton| w.value(), |s: &mut Style, v| s.corner_radius = v);
        on_value!(p.font_size, "font", connect_value_changed, |w: &gtk::SpinButton| w.value(), |s: &mut Style, v| s.font_size = v);
        on_value!(p.wm_opacity, "opacity", connect_value_changed, |w: &gtk::Scale| w.value(), |s: &mut Style, v| s.opacity = v);
        on_value!(p.wm_rotation, "rotation", connect_value_changed, |w: &gtk::Scale| w.value(), |s: &mut Style, v| s.rotation = v);
        on_value!(
            p.line_style,
            "line",
            connect_selected_notify,
            |w: &gtk::DropDown| match w.selected() {
                1 => LineStyle::Dashed,
                2 => LineStyle::Dotted,
                _ => LineStyle::Solid,
            },
            |s: &mut Style, v| s.line_style = v
        );

        // Kind-level options: update tool defaults and any selected items.
        {
            let t = self.clone();
            p.text_pres.connect_selected_notify(move |w| {
                if t.updating.get() {
                    return;
                }
                let v = match w.selected() {
                    1 => TextPresentation::Label,
                    2 => TextPresentation::Callout,
                    _ => TextPresentation::Plain,
                };
                t.canvas.state.borrow_mut().options.text_presentation = v;
                t.canvas.apply_kind("pres", move |k| {
                    if let Kind::Text { presentation, .. } = k {
                        *presentation = v;
                    }
                });
            });
        }
        {
            let t = self.clone();
            p.arrow_style.connect_selected_notify(move |w| {
                if t.updating.get() {
                    return;
                }
                let v = match w.selected() {
                    1 => ArrowStyle::CurvedRight,
                    2 => ArrowStyle::CurvedLeft,
                    _ => ArrowStyle::Straight,
                };
                t.canvas.state.borrow_mut().options.arrow_style = v;
                t.canvas.apply_kind("astyle", move |k| {
                    if let Kind::Arrow { a, b, ctrl, style, .. } = k {
                        *style = v;
                        *ctrl = super::canvas::curve_ctrl(*a, *b, v);
                    }
                });
            });
        }
        {
            let t = self.clone();
            p.arrow_type.connect_selected_notify(move |w| {
                if t.updating.get() {
                    return;
                }
                let v = match w.selected() {
                    1 => ArrowType::Tapered,
                    2 => ArrowType::Outlined,
                    _ => ArrowType::Classic,
                };
                t.canvas.state.borrow_mut().options.arrow_type = v;
                t.canvas.apply_kind("atype", move |k| {
                    if let Kind::Arrow { kind, .. } = k {
                        *kind = v;
                    }
                });
            });
        }
        let head_of = |i: u32| match i {
            0 => Head::None,
            2 => Head::Circle,
            _ => Head::Arrow,
        };
        {
            let t = self.clone();
            p.head_start.connect_selected_notify(move |w| {
                if t.updating.get() {
                    return;
                }
                let v = head_of(w.selected());
                t.canvas.state.borrow_mut().options.head_start = v;
                t.canvas.apply_kind("hs", move |k| {
                    if let Kind::Arrow { head_start, .. } = k {
                        *head_start = v;
                    }
                });
            });
            let t = self.clone();
            p.head_end.connect_selected_notify(move |w| {
                if t.updating.get() {
                    return;
                }
                let v = head_of(w.selected());
                t.canvas.state.borrow_mut().options.head_end = v;
                t.canvas.apply_kind("he", move |k| {
                    if let Kind::Arrow { head_end, .. } = k {
                        *head_end = v;
                    }
                });
            });
        }
        {
            let t = self.clone();
            p.blur_effect.connect_selected_notify(move |w| {
                if t.updating.get() {
                    return;
                }
                let v = BlurEffect::ALL[(w.selected() as usize).min(BlurEffect::ALL.len() - 1)];
                t.canvas.state.borrow_mut().options.blur_effect = v;
                t.canvas.apply_kind("beffect", move |k| {
                    if let Kind::Blur { effect, .. } = k {
                        *effect = v;
                    }
                });
            });
            let t = self.clone();
            p.blur_strength.connect_value_changed(move |w| {
                if t.updating.get() {
                    return;
                }
                let v = w.value();
                t.canvas.state.borrow_mut().options.blur_strength = v;
                t.canvas.apply_kind("bstrength", move |k| {
                    if let Kind::Blur { strength, .. } = k {
                        *strength = v;
                    }
                });
            });
            let t = self.clone();
            p.redact_btn.connect_clicked(move |_| t.auto_redact());
            let t = self.clone();
            p.dim.connect_value_changed(move |w| {
                if !t.updating.get() {
                    t.canvas.set_spotlight_dim(w.value());
                }
            });
            let t = self.clone();
            p.counter_size.connect_value_changed(move |w| {
                if t.updating.get() {
                    return;
                }
                let v = w.value();
                t.canvas.state.borrow_mut().options.counter_size = v;
                t.canvas.apply_kind("csize", move |k| {
                    if let Kind::Counter { size, .. } = k {
                        *size = v;
                    }
                });
            });
            let t = self.clone();
            p.wm_text.connect_changed(move |w| {
                if t.updating.get() {
                    return;
                }
                let v = w.text().to_string();
                t.canvas.state.borrow_mut().options.watermark_text = v.clone();
                t.canvas.apply_kind("wmtext", move |k| {
                    if let Kind::Watermark { text, .. } = k {
                        *text = v.clone();
                    }
                });
            });
            let t = self.clone();
            p.wm_style.connect_selected_notify(move |w| {
                if t.updating.get() {
                    return;
                }
                let v = match w.selected() {
                    0 => WatermarkStyle::Single,
                    2 => WatermarkStyle::Tiled,
                    _ => WatermarkStyle::Diagonal,
                };
                t.canvas.state.borrow_mut().options.watermark_style = v;
                t.canvas.apply_kind("wmstyle", move |k| {
                    if let Kind::Watermark { style, .. } = k {
                        *style = v;
                    }
                });
            });
            let t = self.clone();
            p.text_snap.connect_toggled(move |w| {
                if !t.updating.get() {
                    t.canvas.state.borrow_mut().options.text_snap = w.is_active();
                }
            });
            let t = self.clone();
            p.crop_snap.connect_toggled(move |w| {
                if !t.updating.get() {
                    t.canvas.state.borrow_mut().options.crop_snap = w.is_active();
                }
            });
            let t = self.clone();
            p.crop_aspect.connect_selected_notify(move |_| t.apply_crop_aspect());
            let t = self.clone();
            p.crop_portrait.connect_toggled(move |_| t.apply_crop_aspect());
        }
        // Crop buttons by name.
        let mut child = p.crop_box.first_child();
        while let Some(c) = child {
            if let Some(b) = c.downcast_ref::<gtk::Button>() {
                let t = self.clone();
                match b.widget_name().as_str() {
                    "crop-auto" => b.connect_clicked(move |_| {
                        if !t.canvas.auto_crop() {
                            t.toast("No content edges found");
                        }
                    }),
                    "crop-cancel" => b.connect_clicked(move |_| t.canvas.cancel_crop()),
                    "crop-apply" => b.connect_clicked(move |_| t.canvas.commit_crop()),
                    _ => b.connect_clicked(|_| {}),
                };
            }
            child = c.next_sibling();
        }
    }

    fn apply_crop_aspect(&self) {
        if self.updating.get() {
            return;
        }
        let idx = self.props.crop_aspect.selected() as usize;
        let mut aspect = CROP_ASPECTS.get(idx).and_then(|a| a.1);
        if self.props.crop_portrait.is_active() {
            aspect = aspect.map(|(w, h)| (h, w));
        }
        self.canvas.set_crop_aspect(aspect);
    }

    // ----- sidebar -----

    #[allow(deprecated)]
    fn build_sidebar() -> Sidebar {
        let revealer = gtk::Revealer::new();
        revealer.set_transition_type(gtk::RevealerTransitionType::SlideLeft);
        let outer = gtk::Box::new(gtk::Orientation::Vertical, 12);
        outer.set_size_request(250, -1);
        outer.set_margin_top(12);
        outer.set_margin_bottom(12);
        outer.set_margin_start(12);
        outer.set_margin_end(12);
        outer.add_css_class("sidebar");
        let title = gtk::Label::new(Some("Canvas"));
        title.add_css_class("heading");
        title.set_xalign(0.0);
        outer.append(&title);

        let bg_kind = dropdown(&["No background", "Gradient", "Solid color", "Blurred image", "Image file"]);
        outer.append(&labeled_v("Background", &bg_kind));
        let gradients = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        gradients.set_halign(gtk::Align::Start);
        let flow = gtk::FlowBox::new();
        flow.set_max_children_per_line(4);
        flow.set_selection_mode(gtk::SelectionMode::None);
        for (name, a, b) in GRADIENTS {
            let btn = gtk::Button::new();
            btn.set_size_request(44, 30);
            btn.set_tooltip_text(Some(name));
            let css = gtk::CssProvider::new();
            css.load_from_string(&format!("button {{ background: linear-gradient(135deg, {a}, {b}); border-radius: 0; }}"));
            btn.style_context().add_provider(&css, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1);
            flow.insert(&btn, -1);
        }
        gradients.append(&flow);
        outer.append(&gradients);
        let solid = gtk::ColorDialogButton::new(Some(gtk::ColorDialog::new()));
        solid.set_rgba(&gdk::RGBA::new(0.12, 0.12, 0.14, 1.0));
        outer.append(&labeled_v("Solid color", &solid));
        let image_btn = gtk::Button::with_label("Choose image…");
        outer.append(&image_btn);
        let blur_strength = scale(1.0, 20.0, 1.0, 180);
        blur_strength.set_value(8.0);
        outer.append(&labeled_v("Blur strength", &blur_strength));
        let padding = scale(0.0, 300.0, 4.0, 180);
        outer.append(&labeled_v("Padding", &padding));
        let radius = scale(0.0, 80.0, 1.0, 180);
        outer.append(&labeled_v("Corner radius", &radius));
        let shadow = scale(0.0, 1.0, 0.05, 180);
        shadow.set_value(0.3);
        outer.append(&labeled_v("Shadow", &shadow));
        let aspect = dropdown(&CANVAS_ASPECTS.map(|a| a.0));
        let portrait = gtk::ToggleButton::with_label("Portrait");
        let ab = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        ab.append(&aspect);
        ab.append(&portrait);
        outer.append(&labeled_v("Aspect ratio", &ab));
        let scroller = gtk::ScrolledWindow::new();
        scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroller.set_child(Some(&outer));
        revealer.set_child(Some(&scroller));
        Sidebar { revealer, bg_kind, gradients, solid, image_btn, blur_strength, padding, radius, shadow, aspect, portrait }
    }

    fn wire_sidebar(self: &Rc<Self>) {
        let sb = &self.sidebar;
        {
            let t = self.clone();
            sb.bg_kind.connect_selected_notify(move |_| t.apply_background());
            let t = self.clone();
            sb.solid.connect_rgba_notify(move |_| t.apply_background());
            let t = self.clone();
            sb.blur_strength.connect_value_changed(move |_| t.apply_background());
            let t = self.clone();
            sb.image_btn.connect_clicked(move |_| {
                let dialog = gtk::FileDialog::new();
                dialog.set_title("Choose background image");
                let t2 = t.clone();
                dialog.open(Some(&t.win), gio::Cancellable::NONE, move |res| {
                    if let Ok(file) = res {
                        if let Some(path) = file.path() {
                            t2.sidebar.bg_kind.set_selected(4);
                            t2.canvas.update_canvas("bg", move |c| c.background = Background::Image { path });
                        }
                    }
                });
            });
        }
        // Gradient presets.
        if let Some(flow) = sb.gradients.first_child().and_then(|c| c.downcast::<gtk::FlowBox>().ok()) {
            let mut i = 0;
            let mut child = flow.first_child();
            while let Some(c) = child {
                if let Some(btn) =
                    c.downcast_ref::<gtk::FlowBoxChild>().and_then(|fc| fc.child()).and_then(|w| w.downcast::<gtk::Button>().ok())
                {
                    let t = self.clone();
                    let (_, a, b) = GRADIENTS[i.min(GRADIENTS.len() - 1)];
                    btn.connect_clicked(move |_| {
                        t.updating.set(true);
                        t.sidebar.bg_kind.set_selected(1);
                        t.updating.set(false);
                        let (from, to) = (Color::parse(a).unwrap(), Color::parse(b).unwrap());
                        t.canvas.update_canvas("bg", move |c| {
                            c.background = Background::Gradient { from, to, angle: 135.0 };
                            if c.padding <= 0.0 {
                                c.padding = 48.0;
                            }
                            if c.corner_radius <= 0.0 {
                                c.corner_radius = 12.0;
                            }
                        });
                    });
                    i += 1;
                }
                child = c.next_sibling();
            }
        }
        {
            let t = self.clone();
            sb.padding.connect_value_changed(move |w| {
                if !t.updating.get() {
                    let v = w.value();
                    t.canvas.update_canvas("pad", move |c| c.padding = v);
                }
            });
            let t = self.clone();
            sb.radius.connect_value_changed(move |w| {
                if !t.updating.get() {
                    let v = w.value();
                    t.canvas.update_canvas("crad", move |c| c.corner_radius = v);
                }
            });
            let t = self.clone();
            sb.shadow.connect_value_changed(move |w| {
                if !t.updating.get() {
                    let v = w.value();
                    t.canvas.update_canvas("shadow", move |c| c.shadow = v);
                }
            });
            let t = self.clone();
            sb.aspect.connect_selected_notify(move |_| t.apply_canvas_aspect());
            let t = self.clone();
            sb.portrait.connect_toggled(move |_| t.apply_canvas_aspect());
        }
    }

    fn apply_canvas_aspect(&self) {
        if self.updating.get() {
            return;
        }
        let idx = self.sidebar.aspect.selected() as usize;
        let mut aspect = CANVAS_ASPECTS.get(idx).and_then(|a| a.1);
        if self.sidebar.portrait.is_active() {
            aspect = aspect.map(|(w, h)| (h, w));
        }
        self.canvas.update_canvas("aspect", move |c| c.aspect = aspect);
    }

    fn apply_background(&self) {
        if self.updating.get() {
            return;
        }
        let sb = &self.sidebar;
        let bg = match sb.bg_kind.selected() {
            0 => Background::None,
            1 => {
                let cur = self.canvas.state.borrow().doc.sheet.canvas.background.clone();
                match cur {
                    Background::Gradient { .. } => cur,
                    _ => {
                        let (_, a, b) = GRADIENTS[1];
                        Background::Gradient { from: Color::parse(a).unwrap(), to: Color::parse(b).unwrap(), angle: 135.0 }
                    }
                }
            }
            2 => Background::Solid { color: Color::from_gdk(&sb.solid.rgba()) },
            3 => Background::Blurred { strength: sb.blur_strength.value(), dim: 0.15 },
            _ => {
                let cur = self.canvas.state.borrow().doc.sheet.canvas.background.clone();
                match cur {
                    Background::Image { .. } => cur,
                    _ => return,
                }
            }
        };
        let is_none = bg == Background::None;
        self.canvas.update_canvas("bg", move |c| {
            c.background = bg;
            if !is_none && c.padding <= 0.0 {
                c.padding = 48.0;
            }
        });
    }

    // ----- refresh UI from state -----

    fn refresh(&self) {
        if self.updating.get() {
            return;
        }
        self.updating.set(true);
        let s = self.canvas.state.borrow();
        let tool = s.tool;
        for (t, b) in self.tool_buttons.borrow().iter() {
            b.set_active(*t == tool);
        }
        self.zoom_label.set_text(&format!("{}%", (s.zoom * 100.0).round() as i64));
        self.undo_btn.set_sensitive(s.doc.can_undo());
        self.redo_btn.set_sensitive(s.doc.can_redo());
        let title = Self::title_for(self.source.borrow().as_deref());
        self.win.set_title(Some(&if s.doc.dirty { format!("• {title}") } else { title }));

        // Determine which properties apply.
        let selected: Vec<&Item> = s.selected_items();
        let kinds: Vec<&Kind> = selected.iter().map(|i| &i.kind).collect();
        let cropping = s.crop.is_some();
        let has = |f: &dyn Fn(&Kind) -> bool| kinds.iter().any(|k| f(k));
        let tool_is = |ts: &[Tool]| ts.contains(&tool);
        let style = selected.first().map(|i| i.style.clone()).unwrap_or_else(|| s.style.clone());

        let show_color = !cropping
            && (tool_is(&[
                Tool::Rect,
                Tool::FilledRect,
                Tool::Oval,
                Tool::Arrow,
                Tool::Line,
                Tool::Text,
                Tool::Highlight,
                Tool::Counter,
                Tool::Watermark,
                Tool::Pencil,
            ]) || (!kinds.is_empty()
                && !has(&|k| matches!(k, Kind::Blur { .. } | Kind::Spotlight { .. }))
                && kinds.iter().all(|k| !matches!(k, Kind::Blur { .. } | Kind::Spotlight { .. }))));
        let show_width = !cropping
            && (tool_is(&[Tool::Rect, Tool::FilledRect, Tool::Oval, Tool::Arrow, Tool::Line, Tool::Highlight, Tool::Pencil])
                || has(&|k| {
                    matches!(
                        k,
                        Kind::Rect { .. }
                            | Kind::Oval { .. }
                            | Kind::Arrow { .. }
                            | Kind::Line { .. }
                            | Kind::Highlight { .. }
                            | Kind::Pencil { .. }
                    )
                }));
        let show_line = !cropping
            && (tool_is(&[Tool::Rect, Tool::FilledRect, Tool::Oval, Tool::Arrow, Tool::Line])
                || has(&|k| matches!(k, Kind::Rect { .. } | Kind::Oval { .. } | Kind::Arrow { .. } | Kind::Line { .. })));
        let show_radius = !cropping
            && (tool_is(&[Tool::Rect, Tool::FilledRect, Tool::Spotlight, Tool::Blur])
                || has(&|k| {
                    matches!(
                        k,
                        Kind::Rect { .. }
                            | Kind::Spotlight { .. }
                            | Kind::Blur { .. }
                            | Kind::Text { presentation: TextPresentation::Label | TextPresentation::Callout, .. }
                    )
                }));
        let show_font =
            !cropping && (tool_is(&[Tool::Text, Tool::Watermark]) || has(&|k| matches!(k, Kind::Text { .. } | Kind::Watermark { .. })));
        let show_text = !cropping && (tool == Tool::Text || has(&|k| matches!(k, Kind::Text { .. })));
        let show_arrow = !cropping && (tool == Tool::Arrow || has(&|k| matches!(k, Kind::Arrow { .. })));
        let show_blur = !cropping && (tool == Tool::Blur || has(&|k| matches!(k, Kind::Blur { .. })));
        let show_dim = !cropping && (tool == Tool::Spotlight || has(&|k| matches!(k, Kind::Spotlight { .. })));
        let show_counter = !cropping && (tool == Tool::Counter || has(&|k| matches!(k, Kind::Counter { .. })));
        let show_wm = !cropping && (tool == Tool::Watermark || has(&|k| matches!(k, Kind::Watermark { .. })));
        let show_snap = !cropping && tool == Tool::Highlight;

        let p = &self.props;
        p.palette.set_visible(show_color);
        p.width_box.set_visible(show_width);
        p.line_style.set_visible(show_line);
        p.radius_box.set_visible(show_radius);
        p.font_box.set_visible(show_font);
        p.text_pres.set_visible(show_text);
        for d in [&p.arrow_style, &p.arrow_type, &p.head_start, &p.head_end] {
            d.set_visible(show_arrow);
        }
        p.blur_box.set_visible(show_blur);
        p.dim_box.set_visible(show_dim);
        p.counter_box.set_visible(show_counter);
        p.wm_box.set_visible(show_wm);
        p.text_snap.set_visible(show_snap);
        p.crop_box.set_visible(cropping);
        let any = show_color || show_width || show_blur || show_dim || show_counter || show_wm || show_snap || cropping;
        p.hint.set_visible(!any);

        p.color_btn.set_rgba(&style.color.to_gdk());
        p.width.set_value(style.width);
        p.radius.set_value(style.corner_radius);
        p.font_size.set_value(style.font_size);
        p.line_style.set_selected(match style.line_style {
            LineStyle::Solid => 0,
            LineStyle::Dashed => 1,
            LineStyle::Dotted => 2,
        });
        p.wm_opacity.set_value(style.opacity);
        p.wm_rotation.set_value(style.rotation);
        let o = &s.options;
        let first = kinds.first();
        p.text_pres.set_selected(match first {
            Some(Kind::Text { presentation, .. }) => *presentation as u32,
            _ => o.text_presentation as u32,
        });
        let (astyle, atype, hs, he) = match first {
            Some(Kind::Arrow { style, kind, head_start, head_end, .. }) => (*style, *kind, *head_start, *head_end),
            _ => (o.arrow_style, o.arrow_type, o.head_start, o.head_end),
        };
        p.arrow_style.set_selected(astyle as u32);
        p.arrow_type.set_selected(atype as u32);
        p.head_start.set_selected(hs as u32);
        p.head_end.set_selected(he as u32);
        let (beff, bstr) = match first {
            Some(Kind::Blur { effect, strength, .. }) => (*effect, *strength),
            _ => (o.blur_effect, o.blur_strength),
        };
        p.blur_effect.set_selected(BlurEffect::ALL.iter().position(|e| *e == beff).unwrap_or(0) as u32);
        p.blur_strength.set_value(bstr);
        p.dim.set_value(s.doc.sheet.spotlight_dim);
        p.counter_size.set_value(match first {
            Some(Kind::Counter { size, .. }) => *size,
            _ => o.counter_size,
        });
        let (wtext, wstyle) = match first {
            Some(Kind::Watermark { text, style, .. }) => (text.clone(), *style),
            _ => (o.watermark_text.clone(), o.watermark_style),
        };
        if p.wm_text.text().as_str() != wtext {
            p.wm_text.set_text(&wtext);
        }
        p.wm_style.set_selected(wstyle as u32);
        p.text_snap.set_active(o.text_snap);
        p.crop_snap.set_active(o.crop_snap);
        p.redact_btn.set_sensitive(self.words.borrow().is_some());

        // Sidebar values.
        let c = &s.doc.sheet.canvas;
        self.sidebar.bg_kind.set_selected(match c.background {
            Background::None => 0,
            Background::Gradient { .. } => 1,
            Background::Solid { .. } => 2,
            Background::Blurred { .. } => 3,
            Background::Image { .. } => 4,
        });
        self.sidebar.padding.set_value(c.padding);
        self.sidebar.radius.set_value(c.corner_radius);
        self.sidebar.shadow.set_value(c.shadow);
        drop(s);
        self.updating.set(false);
    }

    // ----- keyboard -----

    fn install_keys(self: &Rc<Self>) {
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let t = self.clone();
        keys.connect_key_pressed(move |_, key, _, mods| {
            let ctrl = mods.contains(gdk::ModifierType::CONTROL_MASK);
            let shift = mods.contains(gdk::ModifierType::SHIFT_MASK);
            // Let text entry widgets keep their keys.
            let in_text = GtkWindowExt::focus(&t.win)
                .map(|w| w.is::<gtk::Text>() || w.is::<gtk::TextView>() || w.is::<gtk::Entry>())
                .unwrap_or(false);
            if in_text && key != gdk::Key::Escape {
                if key == gdk::Key::s && ctrl {
                    t.save(false);
                    return glib::Propagation::Stop;
                }
                return glib::Propagation::Proceed;
            }
            let cropping = t.canvas.state.borrow().crop.is_some();
            if ctrl {
                match key {
                    gdk::Key::z | gdk::Key::Z if shift => t.canvas.redo(),
                    gdk::Key::z => t.canvas.undo(),
                    gdk::Key::y => t.canvas.redo(),
                    gdk::Key::s | gdk::Key::S => {
                        t.save(false);
                    }
                    gdk::Key::c | gdk::Key::C if shift => {
                        t.copy_image();
                        t.win.destroy();
                    }
                    gdk::Key::c => {
                        if !t.canvas.copy_items() {
                            t.copy_image();
                        }
                    }
                    gdk::Key::v => t.canvas.paste_items(),
                    gdk::Key::d => t.canvas.duplicate(),
                    gdk::Key::a => t.canvas.select_all(),
                    gdk::Key::b => {
                        let r = &t.sidebar.revealer;
                        r.set_reveal_child(!r.reveals_child());
                    }
                    gdk::Key::plus | gdk::Key::equal | gdk::Key::KP_Add => t.canvas.zoom_by(1.25),
                    gdk::Key::minus | gdk::Key::KP_Subtract => t.canvas.zoom_by(1.0 / 1.25),
                    gdk::Key::_0 | gdk::Key::KP_0 => t.canvas.zoom_fit(),
                    gdk::Key::_1 | gdk::Key::KP_1 => t.canvas.zoom_actual(),
                    gdk::Key::e => t.export_as(),
                    gdk::Key::w => {
                        if t.confirm_close() == glib::Propagation::Proceed {
                            t.win.destroy();
                        }
                    }
                    _ => return glib::Propagation::Proceed,
                }
                return glib::Propagation::Stop;
            }
            let step = if shift { 10.0 } else { 1.0 };
            match key {
                gdk::Key::Escape => {
                    if cropping {
                        t.canvas.cancel_crop();
                    } else if t.canvas.is_text_editing() {
                        t.canvas.commit_text_edit();
                    } else if !t.canvas.state.borrow().selection.is_empty() {
                        t.canvas.deselect();
                    } else {
                        t.canvas.set_tool(Tool::Select);
                    }
                }
                gdk::Key::Return | gdk::Key::KP_Enter if cropping => t.canvas.commit_crop(),
                gdk::Key::Delete | gdk::Key::BackSpace => t.canvas.delete_selection(),
                gdk::Key::Left => t.canvas.nudge(-step, 0.0),
                gdk::Key::Right => t.canvas.nudge(step, 0.0),
                gdk::Key::Up => t.canvas.nudge(0.0, -step),
                gdk::Key::Down => t.canvas.nudge(0.0, step),
                gdk::Key::space => {
                    t.canvas.state.borrow_mut().space_down = true;
                    t.canvas.area.set_cursor_from_name(Some("grab"));
                }
                gdk::Key::a | gdk::Key::A if cropping => {
                    if !t.canvas.auto_crop() {
                        t.toast("No content edges found");
                    }
                }
                _ => {
                    let ch = key.to_unicode().map(|c| c.to_ascii_lowercase());
                    if let Some(tool) = Tool::ALL.iter().find(|tl| Some(tl.key()) == ch) {
                        t.canvas.set_tool(*tool);
                    } else {
                        return glib::Propagation::Proceed;
                    }
                }
            }
            glib::Propagation::Stop
        });
        let t = self.clone();
        keys.connect_key_released(move |_, key, _, _| {
            if key == gdk::Key::space {
                t.canvas.state.borrow_mut().space_down = false;
                t.canvas.area.set_cursor_from_name(Some("default"));
            }
        });
        self.win.add_controller(keys);
    }

    fn install_actions(self: &Rc<Self>) {
        let group = gio::SimpleActionGroup::new();
        let add = |name: &str, f: Box<dyn Fn()>| {
            let a = gio::SimpleAction::new(name, None);
            a.connect_activate(move |_, _| f());
            group.add_action(&a);
        };
        let t = self.clone();
        add(
            "save",
            Box::new(move || {
                t.save(false);
            }),
        );
        let t = self.clone();
        add("export", Box::new(move || t.export_as()));
        let t = self.clone();
        add("extract-text", Box::new(move || t.extract_text()));
        let t = self.clone();
        add("redact", Box::new(move || t.auto_redact()));
        let t = self.clone();
        add("copy-image", Box::new(move || t.copy_image()));
        let t = self.clone();
        add("duplicate", Box::new(move || t.canvas.duplicate()));
        let t = self.clone();
        add("delete", Box::new(move || t.canvas.delete_selection()));
        let t = self.clone();
        add(
            "open-external",
            Box::new(move || {
                if let Some(p) = t.source.borrow().clone() {
                    let _ = std::process::Command::new("xdg-open").arg(p).spawn();
                }
            }),
        );
        let t = self.clone();
        add("delete-file", Box::new(move || t.delete_file()));
        let t = self.clone();
        add("preferences", Box::new(move || super::preferences::open(&t.gb)));
        self.win.insert_action_group("editor", Some(&group));
    }

    // ----- output actions -----

    fn toast(&self, text: &str) {
        self.toast.add_toast(adw::Toast::new(text));
    }

    pub fn copy_image(&self) {
        let img = self.canvas.render_export();
        match crate::export::encode_png(&img).and_then(|b| crate::clipboard::copy_png(&b)) {
            Ok(_) => self.toast("Image copied"),
            Err(e) => self.toast(&format!("Copy failed: {e}")),
        }
    }

    /// Save in place (or to a new file in the save folder), persist the session, and route post-actions.
    pub fn save(&self, quiet: bool) -> Option<PathBuf> {
        let img = self.canvas.render_export();
        let cfg = self.gb.config.get();
        let existing = self.source.borrow().clone();
        let path = match &existing {
            Some(p) => match crate::export::save_to(&img, p, cfg.general.quality, true) {
                Ok(_) => p.clone(),
                Err(e) => {
                    self.toast(&format!("Save failed: {e}"));
                    return None;
                }
            },
            None => match crate::export::save(&img, &cfg) {
                Ok(p) => {
                    if cfg.history.enabled {
                        let _ = self.gb.history.borrow().insert(&p, img.width(), img.height(), None);
                    }
                    p
                }
                Err(e) => {
                    self.toast(&format!("Save failed: {e}"));
                    return None;
                }
            },
        };
        {
            let s = self.canvas.state.borrow();
            match session::save(&path, &s.doc.source, &s.doc.sheet) {
                Ok(sp) => {
                    let _ = self.gb.history.borrow().set_session(&path, &sp);
                }
                Err(e) => tracing::warn!("session save failed: {e}"),
            }
        }
        self.canvas.state.borrow_mut().doc.dirty = false;
        *self.source.borrow_mut() = Some(path.clone());
        let actions = cfg.post_capture.annotate_export;
        if actions.copy {
            if let Ok(b) = crate::export::encode_png(&img) {
                let _ = crate::clipboard::copy_png(&b);
            }
        }
        // A save is not a new capture: refresh the card that already shows this file
        // (if any) and only add one when the user asked for it in Preferences.
        if cfg.quick_access.enabled {
            let frame = Frame { image: img.clone(), scale: 1.0 };
            let mut qa = self.gb.quick_access.borrow_mut();
            if !qa.refresh(&path, &frame) && actions.quick_access {
                qa.push(&self.gb, frame, path.clone(), true);
            }
        }
        if !quiet {
            self.toast(&format!("Saved {}", path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()));
        }
        self.refresh();
        if self.gb.wait_mode.get() {
            crate::app::report_wait_result(&self.gb, Some(&path), img.width(), img.height());
        }
        Some(path)
    }

    fn export_as(self: &Rc<Self>) {
        let dialog = gtk::FileDialog::new();
        dialog.set_title("Export image");
        let cfg = self.gb.config.get();
        let name =
            self.source.borrow().as_ref().and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string())).unwrap_or_else(|| {
                format!("{}.{}", chrono::Local::now().format(&cfg.general.filename_pattern), cfg.general.format.extension())
            });
        dialog.set_initial_name(Some(&name));
        dialog.set_initial_folder(Some(&gio::File::for_path(&cfg.general.save_folder)));
        let t = self.clone();
        dialog.save(Some(&self.win), gio::Cancellable::NONE, move |res| {
            if let Ok(file) = res {
                if let Some(path) = file.path() {
                    let img = t.canvas.render_export();
                    match crate::export::save_to(&img, &path, cfg.general.quality, true) {
                        Ok(_) => {
                            let _ = t.gb.history.borrow().insert(&path, img.width(), img.height(), None);
                            t.toast(&format!("Exported {}", path.display()));
                        }
                        Err(e) => t.toast(&format!("Export failed: {e}")),
                    }
                }
            }
        });
    }

    fn delete_file(self: &Rc<Self>) {
        let Some(path) = self.source.borrow().clone() else {
            self.win.destroy();
            return;
        };
        let dialog =
            adw::AlertDialog::new(Some("Delete this screenshot?"), Some(&format!("{} will be moved to the trash.", path.display())));
        dialog.add_responses(&[("cancel", "Cancel"), ("delete", "Delete")]);
        dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
        let t = self.clone();
        dialog.connect_response(None, move |_, resp| {
            if resp == "delete" {
                let file = gio::File::for_path(&path);
                if file.trash(gio::Cancellable::NONE).is_err() {
                    let _ = std::fs::remove_file(&path);
                }
                let _ = t.gb.history.borrow().delete_by_path(&path);
                session::delete(&path);
                t.canvas.state.borrow_mut().doc.dirty = false;
                t.win.destroy();
            }
        });
        dialog.present(Some(&self.win));
    }

    fn confirm_close(self: &Rc<Self>) -> glib::Propagation {
        if !self.canvas.state.borrow().doc.dirty {
            return glib::Propagation::Proceed;
        }
        let dialog = adw::AlertDialog::new(Some("Save changes?"), Some("Your annotations will be lost if you close without saving."));
        dialog.add_responses(&[("cancel", "Cancel"), ("discard", "Discard"), ("save", "Save")]);
        dialog.set_response_appearance("discard", adw::ResponseAppearance::Destructive);
        dialog.set_response_appearance("save", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("save"));
        let t = self.clone();
        dialog.connect_response(None, move |_, resp| match resp {
            "save" => {
                if t.save(true).is_some() {
                    t.win.destroy();
                }
            }
            "discard" => {
                t.canvas.state.borrow_mut().doc.dirty = false;
                t.win.destroy();
            }
            _ => {}
        });
        dialog.present(Some(&self.win));
        glib::Propagation::Stop
    }

    // ----- OCR-backed features -----

    fn start_ocr(self: &Rc<Self>) {
        let png = match crate::export::encode_png(&self.canvas.state.borrow().doc.source.image) {
            Ok(p) => p,
            Err(_) => return,
        };
        let langs = self.gb.config.get().ocr.languages;
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(crate::ocr::words(&png, &langs));
        });
        let t = self.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(100), move || match rx.try_recv() {
            Ok(Ok(words)) => {
                t.canvas.state.borrow_mut().text_lines = Some(super::canvas::group_lines(&words));
                *t.words.borrow_mut() = Some(words);
                t.refresh();
                glib::ControlFlow::Break
            }
            Ok(Err(e)) => {
                tracing::info!("OCR unavailable: {e}");
                glib::ControlFlow::Break
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(_) => glib::ControlFlow::Break,
        });
    }

    fn auto_redact(&self) {
        let words = self.words.borrow().clone();
        let Some(words) = words else {
            self.toast("Text detection is still running");
            return;
        };
        let (style, strength) = {
            let s = self.canvas.state.borrow();
            (s.style.clone(), s.options.blur_strength)
        };
        let items = redact::redaction_items(&words, &style, strength);
        let n = items.len();
        if n == 0 {
            self.toast("No sensitive text found");
        } else {
            self.canvas.add_items(items);
            self.toast(&format!("Redacted {n} item{}", if n == 1 { "" } else { "s" }));
        }
    }

    fn extract_text(self: &Rc<Self>) {
        let png = match crate::export::encode_png(&self.canvas.state.borrow().doc.source.image) {
            Ok(p) => p,
            Err(_) => return,
        };
        let langs = self.gb.config.get().ocr.languages;
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(crate::ocr::recognize(&png, &langs));
        });
        let t = self.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(100), move || match rx.try_recv() {
            Ok(Ok(text)) => {
                if text.is_empty() {
                    t.toast("No text found");
                } else {
                    let _ = crate::clipboard::copy_text(&text);
                    t.toast("Text copied to clipboard");
                }
                glib::ControlFlow::Break
            }
            Ok(Err(e)) => {
                t.toast(&format!("OCR failed: {e}"));
                glib::ControlFlow::Break
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(_) => glib::ControlFlow::Break,
        });
    }
}

fn labeled_v(label: &str, w: &impl IsA<gtk::Widget>) -> gtk::Box {
    let b = gtk::Box::new(gtk::Orientation::Vertical, 4);
    let l = gtk::Label::new(Some(label));
    l.add_css_class("dim-label");
    l.add_css_class("caption");
    l.set_xalign(0.0);
    b.append(&l);
    b.append(w);
    b
}

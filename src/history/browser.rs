//! History browser window.

use crate::app::Grabbit;
use adw::prelude::*;
use gtk::prelude::*;
use std::rc::Rc;

pub fn open(gb: &Rc<Grabbit>) {
    let win = adw::ApplicationWindow::builder()
        .application(&gb.app)
        .title("Capture History")
        .default_width(900)
        .default_height(600)
        .build();
    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let header = adw::HeaderBar::new();
    let search = gtk::SearchEntry::new();
    search.set_placeholder_text(Some("Search filenames"));
    search.set_hexpand(true);
    header.set_title_widget(Some(&search));
    content.append(&header);

    let flow = gtk::FlowBox::new();
    flow.set_selection_mode(gtk::SelectionMode::None);
    flow.set_max_children_per_line(6);
    flow.set_column_spacing(12);
    flow.set_row_spacing(12);
    flow.set_margin_top(12);
    flow.set_margin_bottom(12);
    flow.set_margin_start(12);
    flow.set_margin_end(12);
    flow.set_valign(gtk::Align::Start);
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_vexpand(true);
    scroller.set_child(Some(&flow));
    content.append(&scroller);
    win.set_content(Some(&content));

    let populate = {
        let gb = gb.clone();
        let flow = flow.clone();
        Rc::new(move |query: String| {
            while let Some(child) = flow.first_child() {
                flow.remove(&child);
            }
            let entries = gb.history.borrow().list(&query, 500).unwrap_or_default();
            for e in entries {
                let card = gtk::Box::new(gtk::Orientation::Vertical, 6);
                card.set_size_request(200, -1);
                let pic = gtk::Picture::for_filename(&e.path);
                pic.set_size_request(200, 130);
                pic.set_content_fit(gtk::ContentFit::Cover);
                pic.add_css_class("history-thumb");
                let frame = gtk::Frame::new(None);
                frame.set_child(Some(&pic));
                card.append(&frame);
                let name = e.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                let label = gtk::Label::new(Some(&name));
                label.set_ellipsize(pango::EllipsizeMode::Middle);
                label.set_max_width_chars(24);
                label.add_css_class("caption");
                card.append(&label);
                let when = chrono::DateTime::from_timestamp(e.created_at, 0)
                    .map(|t| t.with_timezone(&chrono::Local).format("%b %-d, %H:%M").to_string())
                    .unwrap_or_default();
                let sub = gtk::Label::new(Some(&format!("{when}  ·  {}×{}", e.width, e.height)));
                sub.add_css_class("dim-label");
                sub.add_css_class("caption");
                card.append(&sub);

                let actions = gtk::Box::new(gtk::Orientation::Horizontal, 4);
                actions.set_halign(gtk::Align::Center);
                let mk = |icon: &str, tip: &str| {
                    let b = gtk::Button::from_icon_name(icon);
                    b.add_css_class("flat");
                    b.set_tooltip_text(Some(tip));
                    b
                };
                let copy = mk("edit-copy-symbolic", "Copy");
                let edit = mk("document-edit-symbolic", "Annotate");
                let openb = mk("folder-open-symbolic", "Open");
                let del = mk("user-trash-symbolic", "Delete");
                {
                    let p = e.path.clone();
                    copy.connect_clicked(move |_| {
                        if let Ok(bytes) = std::fs::read(&p) {
                            let _ = crate::clipboard::copy_png(&bytes);
                        }
                    });
                }
                {
                    let gb = gb.clone();
                    let p = e.path.clone();
                    edit.connect_clicked(move |_| crate::annotate::open_file(&gb, &p));
                }
                {
                    let p = e.path.clone();
                    openb.connect_clicked(move |_| {
                        let _ = std::process::Command::new("xdg-open").arg(&p).spawn();
                    });
                }
                {
                    let gb = gb.clone();
                    let id = e.id;
                    let p = e.path.clone();
                    let card = card.clone();
                    del.connect_clicked(move |_| {
                        let _ = std::fs::remove_file(&p);
                        let _ = gb.history.borrow().delete(id);
                        if let Some(parent) = card.parent() {
                            parent.set_visible(false);
                        }
                    });
                }
                for b in [&copy, &edit, &openb, &del] {
                    actions.append(b);
                }
                card.append(&actions);
                flow.insert(&card, -1);
            }
        })
    };
    populate(String::new());
    let p2 = populate.clone();
    search.connect_search_changed(move |s| p2(s.text().to_string()));
    win.present();
}

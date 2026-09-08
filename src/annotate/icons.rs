//! Cairo-drawn tool icons so the toolbar does not depend on icon theme coverage.

use super::canvas::Tool;
use gtk::prelude::*;
use std::f64::consts::PI;

pub fn tool_icon(tool: Tool) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::new();
    area.set_content_width(18);
    area.set_content_height(18);
    area.set_draw_func(move |a, cr, w, h| {
        let color = a.color();
        cr.set_source_rgba(color.red() as f64, color.green() as f64, color.blue() as f64, color.alpha() as f64);
        cr.set_line_width(1.6);
        cr.set_line_cap(cairo::LineCap::Round);
        cr.set_line_join(cairo::LineJoin::Round);
        let (w, h) = (w as f64, h as f64);
        let (cx, cy) = (w / 2.0, h / 2.0);
        match tool {
            Tool::Select => {
                cr.move_to(4.0, 3.0);
                cr.line_to(4.0, 14.0);
                cr.line_to(7.0, 11.0);
                cr.line_to(9.5, 16.0);
                cr.line_to(11.5, 15.0);
                cr.line_to(9.0, 10.5);
                cr.line_to(13.0, 10.0);
                cr.close_path();
                cr.fill().ok();
            }
            Tool::Crop => {
                cr.move_to(5.0, 1.5);
                cr.line_to(5.0, 13.0);
                cr.line_to(16.5, 13.0);
                cr.move_to(1.5, 5.0);
                cr.line_to(13.0, 5.0);
                cr.line_to(13.0, 16.5);
                cr.stroke().ok();
            }
            Tool::Rect => {
                cr.rectangle(3.0, 4.0, 12.0, 10.0);
                cr.stroke().ok();
            }
            Tool::FilledRect => {
                cr.rectangle(3.0, 4.0, 12.0, 10.0);
                cr.fill().ok();
            }
            Tool::Oval => {
                cr.save().ok();
                cr.translate(cx, cy);
                cr.scale(6.5, 5.0);
                cr.arc(0.0, 0.0, 1.0, 0.0, 2.0 * PI);
                cr.restore().ok();
                cr.stroke().ok();
            }
            Tool::Arrow => {
                cr.move_to(3.5, 14.5);
                cr.line_to(13.5, 4.5);
                cr.stroke().ok();
                cr.move_to(14.5, 3.5);
                cr.line_to(8.5, 4.0);
                cr.line_to(14.0, 9.5);
                cr.close_path();
                cr.fill().ok();
            }
            Tool::Line => {
                cr.move_to(3.5, 14.5);
                cr.line_to(14.5, 3.5);
                cr.stroke().ok();
            }
            Tool::Text => {
                cr.set_line_width(2.0);
                cr.move_to(4.0, 4.0);
                cr.line_to(14.0, 4.0);
                cr.move_to(9.0, 4.0);
                cr.line_to(9.0, 15.0);
                cr.stroke().ok();
            }
            Tool::Highlight => {
                cr.set_line_width(5.0);
                cr.set_source_rgba(color.red() as f64, color.green() as f64, color.blue() as f64, 0.45);
                cr.move_to(3.0, 9.0);
                cr.line_to(15.0, 9.0);
                cr.stroke().ok();
                cr.set_source_rgba(color.red() as f64, color.green() as f64, color.blue() as f64, color.alpha() as f64);
                cr.set_line_width(1.6);
                cr.move_to(4.0, 14.0);
                cr.line_to(14.0, 14.0);
                cr.stroke().ok();
            }
            Tool::Blur => {
                for i in 0..3 {
                    for j in 0..3 {
                        let a = if (i + j) % 2 == 0 { 0.9 } else { 0.4 };
                        cr.set_source_rgba(color.red() as f64, color.green() as f64, color.blue() as f64, a);
                        cr.rectangle(3.0 + i as f64 * 4.0, 3.0 + j as f64 * 4.0, 3.6, 3.6);
                        cr.fill().ok();
                    }
                }
            }
            Tool::Spotlight => {
                cr.set_source_rgba(color.red() as f64, color.green() as f64, color.blue() as f64, 0.35);
                cr.set_fill_rule(cairo::FillRule::EvenOdd);
                cr.rectangle(2.0, 2.0, 14.0, 14.0);
                cr.arc(cx, cy, 4.0, 0.0, 2.0 * PI);
                cr.fill().ok();
                cr.set_source_rgba(color.red() as f64, color.green() as f64, color.blue() as f64, color.alpha() as f64);
                cr.arc(cx, cy, 4.0, 0.0, 2.0 * PI);
                cr.stroke().ok();
            }
            Tool::Counter => {
                cr.arc(cx, cy, 6.5, 0.0, 2.0 * PI);
                cr.stroke().ok();
                cr.set_line_width(1.8);
                cr.move_to(cx, 6.0);
                cr.line_to(cx, 12.5);
                cr.move_to(cx - 2.0, 7.5);
                cr.line_to(cx, 6.0);
                cr.stroke().ok();
            }
            Tool::Watermark => {
                cr.save().ok();
                cr.translate(cx, cy);
                cr.rotate(-0.4);
                cr.set_line_width(2.0);
                cr.move_to(-6.0, -3.0);
                cr.line_to(6.0, -3.0);
                cr.move_to(0.0, -3.0);
                cr.line_to(0.0, 5.0);
                cr.stroke().ok();
                cr.restore().ok();
                cr.set_source_rgba(color.red() as f64, color.green() as f64, color.blue() as f64, 0.4);
                cr.rectangle(2.0, 2.0, 14.0, 14.0);
                cr.set_line_width(1.0);
                cr.stroke().ok();
            }
            Tool::Pencil => {
                cr.set_line_width(3.0);
                cr.move_to(4.0, 14.0);
                cr.line_to(13.0, 5.0);
                cr.stroke().ok();
                cr.set_line_width(1.6);
                cr.move_to(3.0, 15.5);
                cr.line_to(5.5, 15.0);
                cr.stroke().ok();
            }
        }
    });
    area
}

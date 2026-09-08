use gio::prelude::*;

pub fn send(app: &gtk::Application, id: &str, title: &str, body: &str, image: Option<&std::path::Path>) {
    let n = gio::Notification::new(title);
    n.set_body(Some(body));
    if let Some(p) = image {
        let icon = gio::FileIcon::new(&gio::File::for_path(p));
        n.set_icon(&icon);
    }
    app.send_notification(Some(id), &n);
}

pub fn play_shutter() {
    // Best effort: canberra is not always present, so fall back silently.
    std::thread::spawn(|| {
        let _ = std::process::Command::new("canberra-gtk-play")
            .args(["-i", "camera-shutter"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    });
}
